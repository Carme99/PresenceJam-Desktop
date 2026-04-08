use crate::config::{self, AppConfig};
use crate::spotify::{SpotifyTokens, TrackInfo};
use crate::teams::{DeviceCodeResponse, TeamsTokens};
use crate::{polling, AppState, PendingSpotifyAuth};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_store::StoreExt;

#[derive(Debug, Clone, serde::Serialize)]
pub struct SpotifyAuthResponse {
    pub auth_url: String,
    pub verifier: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SyncStatus {
    pub is_syncing: bool,
    pub current_track: Option<TrackInfo>,
    pub spotify_connected: bool,
    pub teams_connected: bool,
}

#[tauri::command]
pub fn load_config() -> Result<AppConfig, String> {
    log::info!("[CMD] load_config: ENTRY");
    match config::load_config() {
        Ok(cfg) => {
            log::info!(
                "[CMD] load_config: SUCCESS - spotify.client_id.len={}",
                cfg.spotify.client_id.len()
            );
            Ok(cfg)
        }
        Err(e) => {
            log::error!("[CMD] load_config: FAILED - {}", e);
            Err(e)
        }
    }
}

#[tauri::command]
pub fn save_config(config: AppConfig) -> Result<(), String> {
    log::info!(
        "[CMD] save_config: ENTRY - config.spotify.client_id.len={}",
        config.spotify.client_id.len()
    );
    match config::save_config(&config) {
        Ok(()) => {
            log::info!("[CMD] save_config: SUCCESS");
            Ok(())
        }
        Err(e) => {
            log::error!("[CMD] save_config: FAILED - {}", e);
            Err(e)
        }
    }
}

#[tauri::command]
pub fn get_config_dir() -> Result<String, String> {
    log::info!("[CMD] get_config_dir: ENTRY");
    match config::config_dir().map(|p| p.to_string_lossy().to_string()) {
        Ok(path) => {
            log::info!("[CMD] get_config_dir: SUCCESS - path={}", path);
            Ok(path)
        }
        Err(e) => {
            log::error!("[CMD] get_config_dir: FAILED - {}", e);
            Err(e)
        }
    }
}

#[tauri::command]
pub fn start_spotify_auth(
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    log::info!(
        "[CMD] start_spotify_auth: ENTRY - client_id.len={}, redirect_uri={}",
        client_id.len(),
        redirect_uri
    );

    let verifier = crate::spotify::pkce_generate_verifier();
    log::info!(
        "[CMD] start_spotify_auth: verifier generated, len={}",
        verifier.len()
    );

    let challenge = crate::spotify::pkce_generate_challenge(&verifier);
    log::info!("[CMD] start_spotify_auth: challenge generated");

    let auth_url = format!(
        "https://accounts.spotify.com/authorize\
         ?client_id={}\
         &response_type=code\
         &redirect_uri={}\
         &code_challenge_method=S256\
         &code_challenge={}\
         &scope=user-read-currently-playing user-read-playback-state",
        client_id,
        urlencoding::encode(&redirect_uri),
        urlencoding::encode(&challenge)
    );
    log::info!(
        "[CMD] start_spotify_auth: auth_url created, length={}",
        auth_url.len()
    );

    // Store pending auth in AppState
    {
        let mut pending = state.pending_spotify_auth.write();
        *pending = Some(PendingSpotifyAuth {
            verifier: verifier.clone(),
            client_id: client_id.clone(),
            client_secret: client_secret.clone(),
            redirect_uri: redirect_uri.clone(),
        });
        log::info!("[CMD] start_spotify_auth: stored pending auth in AppState");
    }

    // Also persist to store for crash recovery
    let store = app.store("tokens").map_err(|e| e.to_string())?;
    store.set("spotify_client_id", serde_json::json!(client_id));
    store.set("spotify_client_secret", serde_json::json!(client_secret));
    store.set("spotify_redirect_uri", serde_json::json!(redirect_uri));
    store.set("spotify_verifier", serde_json::json!(verifier));
    store.save().map_err(|e| e.to_string())?;
    log::info!("[CMD] start_spotify_auth: persisted to store");

    if let Err(e) = tauri_plugin_opener::open_url(&auth_url, None::<&str>) {
        log::warn!("[CMD] start_spotify_auth: Failed to open browser: {}", e);
    } else {
        log::info!("[CMD] start_spotify_auth: Browser opened successfully");
    }

    log::info!("[CMD] start_spotify_auth: SUCCESS - Spotify auth started");
    Ok(())
}

#[tauri::command]
pub fn complete_spotify_auth(
    code: String,
    verifier: String,
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<SpotifyTokens, String> {
    log::info!(
        "[CMD] complete_spotify_auth: ENTRY - code.len={}, verifier.len={}",
        code.len(),
        verifier.len()
    );

    let tokens = crate::spotify::complete_spotify_auth(
        &code,
        &verifier,
        &client_id,
        &client_secret,
        &redirect_uri,
    )?;
    log::info!(
        "[CMD] complete_spotify_auth: token exchange successful - access_token.len={}",
        tokens.access_token.len()
    );

    polling::save_spotify_tokens(&app, &tokens)?;
    log::info!("[CMD] complete_spotify_auth: tokens saved to store");

    {
        let mut tokens_guard = state.spotify_tokens.write();
        *tokens_guard = Some(tokens.clone());
        log::info!("[CMD] complete_spotify_auth: tokens stored in AppState");
    }

    log::info!("[CMD] complete_spotify_auth: EMIT spotify-auth-complete event");
    let _ = app.emit("spotify-auth-complete", &tokens);

    log::info!("[CMD] complete_spotify_auth: SUCCESS");
    Ok(tokens)
}

#[tauri::command]
pub fn complete_spotify_auth_manual(
    code: String,
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<SpotifyTokens, String> {
    log::info!(
        "[CMD] complete_spotify_auth_manual: ENTRY - code.len={}",
        code.len()
    );

    // Get pending auth from AppState
    let pending = {
        let mut guard = state.pending_spotify_auth.write();
        log::info!("[CMD] complete_spotify_auth_manual: taking pending auth from AppState");
        guard.take().ok_or_else(|| {
            log::error!("[CMD] complete_spotify_auth_manual: No pending Spotify auth");
            "No pending Spotify auth. Please start auth again.".to_string()
        })?
    };
    log::info!(
        "[CMD] complete_spotify_auth_manual: pending auth found - verifier.len={}",
        pending.verifier.len()
    );

    let tokens = crate::spotify::complete_spotify_auth(
        &code,
        &pending.verifier,
        &pending.client_id,
        &pending.client_secret,
        &pending.redirect_uri,
    )?;
    log::info!("[CMD] complete_spotify_auth_manual: token exchange successful");

    polling::save_spotify_tokens(&app, &tokens)?;
    log::info!("[CMD] complete_spotify_auth_manual: tokens saved to store");

    {
        let mut tokens_guard = state.spotify_tokens.write();
        *tokens_guard = Some(tokens.clone());
        log::info!("[CMD] complete_spotify_auth_manual: tokens stored in AppState");
    }

    log::info!("[CMD] complete_spotify_auth_manual: EMIT spotify-auth-complete event");
    let _ = app.emit("spotify-auth-complete", &tokens);

    log::info!("[CMD] complete_spotify_auth_manual: SUCCESS (manual fallback)");
    Ok(tokens)
}

#[tauri::command]
pub fn get_spotify_tokens(
    state: tauri::State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<Option<SpotifyTokens>, String> {
    log::info!("[CMD] get_spotify_tokens: ENTRY");

    let state_tokens = {
        let guard = state.spotify_tokens.read();
        guard.clone()
    };

    if state_tokens.is_some() {
        log::info!("[CMD] get_spotify_tokens: found tokens in AppState");
        return Ok(state_tokens);
    }
    log::info!("[CMD] get_spotify_tokens: not in AppState, checking store");

    let loaded = polling::load_spotify_tokens(&app)?;
    if let Some(tokens) = &loaded {
        log::info!(
            "[CMD] get_spotify_tokens: loaded from store - access_token.len={}",
            tokens.access_token.len()
        );
        let mut guard = state.spotify_tokens.write();
        *guard = Some(tokens.clone());
    } else {
        log::info!("[CMD] get_spotify_tokens: no tokens found in store");
    }
    Ok(loaded)
}

#[tauri::command]
pub fn refresh_spotify(
    state: tauri::State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<(), String> {
    log::info!("[CMD] refresh_spotify: ENTRY");

    let store = app.store("tokens").map_err(|e| e.to_string())?;
    log::info!("[CMD] refresh_spotify: store opened");

    let client_id = store
        .get("spotify_client_id")
        .and_then(|v| v.as_str().map(String::from))
        .ok_or_else(|| {
            log::error!("[CMD] refresh_spotify: Spotify client ID not found in store");
            "Spotify client ID not found".to_string()
        })?;
    let client_secret = store
        .get("spotify_client_secret")
        .and_then(|v| v.as_str().map(String::from))
        .ok_or_else(|| {
            log::error!("[CMD] refresh_spotify: Spotify client secret not found in store");
            "Spotify client secret not found".to_string()
        })?;
    log::info!("[CMD] refresh_spotify: credentials loaded from store");

    let current_tokens = {
        let guard = state.spotify_tokens.read();
        guard.clone().ok_or_else(|| {
            log::error!("[CMD] refresh_spotify: No Spotify tokens in state");
            "No Spotify tokens to refresh".to_string()
        })?
    };
    log::info!("[CMD] refresh_spotify: current tokens found");

    let new_tokens =
        crate::spotify::refresh_spotify_token(&current_tokens, &client_id, &client_secret)?;
    log::info!("[CMD] refresh_spotify: new tokens received");

    polling::save_spotify_tokens(&app, &new_tokens)?;

    {
        let mut guard = state.spotify_tokens.write();
        *guard = Some(new_tokens);
    }

    log::info!("[CMD] refresh_spotify: SUCCESS");
    Ok(())
}

#[tauri::command]
pub fn open_external_url(url: String) -> Result<(), String> {
    log::info!("[CMD] open_external_url: ENTRY - url.len={}", url.len());
    match tauri_plugin_opener::open_url(&url, None::<&str>) {
        Ok(()) => {
            log::info!("[CMD] open_external_url: SUCCESS");
            Ok(())
        }
        Err(e) => {
            log::error!("[CMD] open_external_url: FAILED - {}", e);
            Err(format!("Failed to open URL: {}", e))
        }
    }
}

#[tauri::command]
pub fn start_teams_auth_device_code(app: AppHandle) -> Result<DeviceCodeResponse, String> {
    log::info!("[CMD] start_teams_auth_device_code: ENTRY");

    let response = crate::teams::start_teams_auth_device_code()?;
    log::info!("[CMD] start_teams_auth_device_code: got device code response");
    log::info!(
        "[CMD] start_teams_auth_device_code: user_code={}, verification_url={}",
        response.user_code,
        response.verification_url
    );

    let store = app.store("tokens").map_err(|e| e.to_string())?;
    store.set("teams_device_code", serde_json::json!(response.device_code));
    store.save().map_err(|e| e.to_string())?;
    log::info!("[CMD] start_teams_auth_device_code: device code persisted to store");

    log::info!("[CMD] start_teams_auth_device_code: SUCCESS");
    Ok(response)
}

#[tauri::command]
pub fn poll_teams_auth(
    device_code: String,
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<TeamsTokens, String> {
    log::info!(
        "[CMD] poll_teams_auth: ENTRY - device_code.len={}",
        device_code.len()
    );

    let tokens = crate::teams::poll_teams_auth(&device_code)?;
    log::info!(
        "[CMD] poll_teams_auth: poll successful - access_token.len={}",
        tokens.access_token.len()
    );

    polling::save_teams_tokens(&app, &tokens)?;
    log::info!("[CMD] poll_teams_auth: tokens saved to store");

    {
        let mut guard = state.teams_tokens.write();
        *guard = Some(tokens.clone());
        log::info!("[CMD] poll_teams_auth: tokens stored in AppState");
    }

    log::info!("[CMD] poll_teams_auth: EMIT teams-auth-complete event");
    let _ = app.emit("teams-auth-complete", &tokens);

    log::info!("[CMD] poll_teams_auth: SUCCESS");
    Ok(tokens)
}

#[tauri::command]
pub fn complete_teams_auth_manual(
    code: String,
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<TeamsTokens, String> {
    log::info!(
        "[CMD] complete_teams_auth_manual: ENTRY - code.len={}",
        code.len()
    );

    let pending = {
        let mut guard = state.pending_teams_auth.write();
        log::info!("[CMD] complete_teams_auth_manual: taking pending auth from AppState");
        guard.take().ok_or_else(|| {
            log::error!("[CMD] complete_teams_auth_manual: No pending Teams auth");
            "No pending Teams auth. Please start auth again.".to_string()
        })?
    };
    log::info!("[CMD] complete_teams_auth_manual: pending auth found");

    let tokens = crate::teams::complete_teams_auth(
        &code,
        &pending.verifier,
        &pending.client_id,
        &pending.redirect_uri,
    )?;
    log::info!("[CMD] complete_teams_auth_manual: token exchange successful");

    polling::save_teams_tokens(&app, &tokens)?;

    {
        let mut guard = state.teams_tokens.write();
        *guard = Some(tokens.clone());
        log::info!("[CMD] complete_teams_auth_manual: tokens stored in AppState");
    }

    log::info!("[CMD] complete_teams_auth_manual: EMIT teams-auth-complete event");
    let _ = app.emit("teams-auth-complete", &tokens);

    log::info!("[CMD] complete_teams_auth_manual: SUCCESS (manual fallback)");
    Ok(tokens)
}

#[tauri::command]
pub fn get_teams_tokens(
    state: tauri::State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<Option<TeamsTokens>, String> {
    log::info!("[CMD] get_teams_tokens: ENTRY");

    let state_tokens = {
        let guard = state.teams_tokens.read();
        guard.clone()
    };

    if state_tokens.is_some() {
        log::info!("[CMD] get_teams_tokens: found tokens in AppState");
        return Ok(state_tokens);
    }
    log::info!("[CMD] get_teams_tokens: not in AppState, checking store");

    let loaded = polling::load_teams_tokens(&app)?;
    if let Some(tokens) = &loaded {
        log::info!(
            "[CMD] get_teams_tokens: loaded from store - access_token.len={}",
            tokens.access_token.len()
        );
        let mut guard = state.teams_tokens.write();
        *guard = Some(tokens.clone());
    } else {
        log::info!("[CMD] get_teams_tokens: no tokens found in store");
    }
    Ok(loaded)
}

#[tauri::command]
pub fn refresh_teams(state: tauri::State<'_, Arc<AppState>>, app: AppHandle) -> Result<(), String> {
    log::info!("[CMD] refresh_teams: ENTRY");

    let current_tokens = {
        let guard = state.teams_tokens.read();
        guard.clone().ok_or_else(|| {
            log::error!("[CMD] refresh_teams: No Teams tokens in state");
            "No Teams tokens to refresh".to_string()
        })?
    };
    log::info!("[CMD] refresh_teams: current tokens found");

    let new_tokens = crate::teams::refresh_teams_token(&current_tokens)?;
    log::info!("[CMD] refresh_teams: new tokens received");

    polling::save_teams_tokens(&app, &new_tokens)?;

    {
        let mut guard = state.teams_tokens.write();
        *guard = Some(new_tokens);
    }

    log::info!("[CMD] refresh_teams: SUCCESS");
    Ok(())
}

#[tauri::command]
pub fn start_syncing(state: tauri::State<'_, Arc<AppState>>, app: AppHandle) -> Result<(), String> {
    log::info!("[CMD] start_syncing: ENTRY");

    let is_syncing = {
        let guard = state.is_syncing.read();
        *guard
    };
    log::info!("[CMD] start_syncing: current is_syncing={}", is_syncing);

    if is_syncing {
        log::info!("[CMD] start_syncing: already syncing, returning early");
        return Ok(());
    }

    let handle = polling::start_polling(Arc::clone(&state.inner()), app.clone())?;
    log::info!("[CMD] start_syncing: polling task spawned");

    {
        let mut is_syncing_guard = state.is_syncing.write();
        *is_syncing_guard = true;
        log::info!("[CMD] start_syncing: is_syncing flag set to true");
    }

    {
        let mut handle_guard = state.polling_handle.write();
        *handle_guard = Some(handle);
        log::info!("[CMD] start_syncing: polling handle stored");
    }

    log::info!("[CMD] start_syncing: EMIT sync-started event");
    let _ = app.emit("sync-started", ());

    log::info!("[CMD] start_syncing: SUCCESS");
    Ok(())
}

#[tauri::command]
pub fn stop_syncing(state: tauri::State<'_, Arc<AppState>>, app: AppHandle) -> Result<(), String> {
    log::info!("[CMD] stop_syncing: ENTRY");

    polling::stop_polling(state.inner());
    log::info!("[CMD] stop_syncing: polling stopped");

    {
        let mut handle_guard = state.polling_handle.write();
        if let Some(handle) = handle_guard.take() {
            // For std::thread, we wait for it to finish (it exits when is_syncing=false)
            match handle.join() {
                Ok(()) => {
                    log::info!("[CMD] stop_syncing: polling thread finished");
                }
                Err(e) => {
                    log::error!("[CMD] stop_syncing: polling thread panicked: {:?}", e);
                }
            }
        }
    }

    log::info!("[CMD] stop_syncing: EMIT sync-stopped event");
    let _ = app.emit("sync-stopped", ());

    log::info!("[CMD] stop_syncing: SUCCESS");
    Ok(())
}

#[tauri::command]
pub fn get_sync_status(state: tauri::State<'_, Arc<AppState>>) -> Result<SyncStatus, String> {
    log::info!("[CMD] get_sync_status: ENTRY");

    let is_syncing = {
        let guard = state.is_syncing.read();
        *guard
    };

    let current_track = {
        let guard = state.current_track.read();
        guard.clone()
    };

    let spotify_connected = {
        let guard = state.spotify_tokens.read();
        guard.is_some()
    };

    let teams_connected = {
        let guard = state.teams_tokens.read();
        guard.is_some()
    };

    log::info!(
        "[CMD] get_sync_status: is_syncing={}, spotify_connected={}, teams_connected={}",
        is_syncing,
        spotify_connected,
        teams_connected
    );

    Ok(SyncStatus {
        is_syncing,
        current_track,
        spotify_connected,
        teams_connected,
    })
}

#[tauri::command]
pub fn show_window(app: AppHandle) -> Result<(), String> {
    log::info!("[CMD] show_window: ENTRY");

    if let Some(window) = app.get_webview_window("main") {
        log::info!("[CMD] show_window: window found, showing and focusing");
        let _ = window.show();
        let _ = window.set_focus();
    } else {
        log::warn!("[CMD] show_window: main window not found");
    }

    log::info!("[CMD] show_window: SUCCESS");
    Ok(())
}

#[tauri::command]
pub fn hide_window(app: AppHandle) -> Result<(), String> {
    log::info!("[CMD] hide_window: ENTRY");

    if let Some(window) = app.get_webview_window("main") {
        log::info!("[CMD] hide_window: window found, hiding");
        let _ = window.hide();
    } else {
        log::warn!("[CMD] hide_window: main window not found");
    }

    log::info!("[CMD] hide_window: SUCCESS");
    Ok(())
}

#[tauri::command]
pub fn get_autostart_enabled(app: AppHandle) -> Result<bool, String> {
    log::info!("[CMD] get_autostart_enabled: ENTRY");

    let autolaunch_manager = app.state::<tauri_plugin_autostart::AutoLaunchManager>();
    match autolaunch_manager.is_enabled() {
        Ok(is_enabled) => {
            log::info!("[CMD] get_autostart_enabled: is_enabled={}", is_enabled);
            Ok(is_enabled)
        }
        Err(e) => {
            log::error!("[CMD] get_autostart_enabled: FAILED - {}", e);
            Err(e.to_string())
        }
    }
}

#[tauri::command]
pub fn set_autostart_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    log::info!("[CMD] set_autostart_enabled: ENTRY - enabled={}", enabled);

    let autolaunch_manager = app.state::<tauri_plugin_autostart::AutoLaunchManager>();
    if enabled {
        match autolaunch_manager.enable() {
            Ok(()) => {
                log::info!("[CMD] set_autostart_enabled: enable SUCCESS");
                Ok(())
            }
            Err(e) => {
                log::error!("[CMD] set_autostart_enabled: enable FAILED - {}", e);
                Err(e.to_string())
            }
        }
    } else {
        match autolaunch_manager.disable() {
            Ok(()) => {
                log::info!("[CMD] set_autostart_enabled: disable SUCCESS");
                Ok(())
            }
            Err(e) => {
                log::error!("[CMD] set_autostart_enabled: disable FAILED - {}", e);
                Err(e.to_string())
            }
        }
    }
}

#[tauri::command]
pub fn open_logs_folder(app: AppHandle) -> Result<(), String> {
    log::info!("[CMD] open_logs_folder: ENTRY");

    let logs_path = app.path().app_log_dir().map_err(|e| {
        log::error!("[CMD] open_logs_folder: failed to get log dir - {}", e);
        e.to_string()
    })?;
    let path_str = logs_path.to_string_lossy();
    log::info!("[CMD] open_logs_folder: log path={}", path_str);

    match tauri_plugin_opener::open_url(&path_str, None::<&str>) {
        Ok(()) => {
            log::info!("[CMD] open_logs_folder: SUCCESS");
            Ok(())
        }
        Err(e) => {
            log::error!("[CMD] open_logs_folder: FAILED - {}", e);
            Err(e.to_string())
        }
    }
}

#[tauri::command]
pub fn open_external(url: String) -> Result<(), String> {
    log::info!("[CMD] open_external: ENTRY - url.len={}", url.len());

    match tauri_plugin_opener::open_url(&url, None::<&str>) {
        Ok(()) => {
            log::info!("[CMD] open_external: SUCCESS");
            Ok(())
        }
        Err(e) => {
            log::error!("[CMD] open_external: FAILED - {}", e);
            Err(e.to_string())
        }
    }
}

#[tauri::command]
pub fn get_current_track(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Option<TrackInfo>, String> {
    log::info!("[CMD] get_current_track: ENTRY");

    let guard = state.current_track.read();
    let track = guard.clone();

    if let Some(ref t) = track {
        log::info!(
            "[CMD] get_current_track: returning track - title={}",
            t.title
        );
    } else {
        log::info!("[CMD] get_current_track: no track playing");
    }

    Ok(track)
}

#[tauri::command]
pub fn is_onboarding_complete() -> Result<bool, String> {
    log::info!("[CMD] is_onboarding_complete: ENTRY");

    let config = config::load_config()?;
    let complete = !config.spotify.client_id.is_empty();
    log::info!(
        "[CMD] is_onboarding_complete: result={} (client_id.len={})",
        complete,
        config.spotify.client_id.len()
    );

    Ok(complete)
}

#[tauri::command]
pub fn complete_onboarding(
    state: tauri::State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<(), String> {
    log::info!("[CMD] complete_onboarding: ENTRY");

    let has_spotify = {
        let guard = state.spotify_tokens.read();
        guard.is_some()
    };

    let has_teams = {
        let guard = state.teams_tokens.read();
        guard.is_some()
    };

    log::info!(
        "[CMD] complete_onboarding: has_spotify={}, has_teams={}",
        has_spotify,
        has_teams
    );

    if has_spotify && has_teams {
        log::info!("[CMD] complete_onboarding: both tokens present, starting sync");
        start_syncing(state, app)?;
        log::info!("[CMD] complete_onboarding: sync started successfully");
    } else {
        log::warn!(
            "[CMD] complete_onboarding: missing tokens, not starting sync (spotify={}, teams={})",
            has_spotify,
            has_teams
        );
    }

    log::info!("[CMD] complete_onboarding: SUCCESS");
    Ok(())
}
