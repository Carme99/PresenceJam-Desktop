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
    config::load_config()
}

#[tauri::command]
pub fn save_config(config: AppConfig) -> Result<(), String> {
    config::save_config(&config)
}

#[tauri::command]
pub fn get_config_dir() -> Result<String, String> {
    config::config_dir().map(|p| p.to_string_lossy().to_string())
}

#[tauri::command]
pub fn start_spotify_auth(
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let verifier = crate::spotify::pkce_generate_verifier();
    let challenge = crate::spotify::pkce_generate_challenge(&verifier);

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

    // Store pending auth in AppState
    {
        let mut pending = state.pending_spotify_auth.write();
        *pending = Some(PendingSpotifyAuth {
            verifier: verifier.clone(),
            client_id: client_id.clone(),
            client_secret: client_secret.clone(),
            redirect_uri: redirect_uri.clone(),
        });
    }

    // Also persist to store for crash recovery
    let store = app.store("tokens").map_err(|e| e.to_string())?;
    store.set("spotify_client_id", serde_json::json!(client_id));
    store.set("spotify_client_secret", serde_json::json!(client_secret));
    store.set("spotify_redirect_uri", serde_json::json!(redirect_uri));
    store.set("spotify_verifier", serde_json::json!(verifier));
    store.save().map_err(|e| e.to_string())?;

    if let Err(e) = tauri_plugin_opener::open_url(&auth_url, None::<&str>) {
        log::warn!("Failed to open browser for Spotify auth: {}", e);
    }

    log::info!("Spotify auth started with redirect_uri: {}", redirect_uri);
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
    let tokens = crate::spotify::complete_spotify_auth(
        &code,
        &verifier,
        &client_id,
        &client_secret,
        &redirect_uri,
    )?;

    polling::save_spotify_tokens(&app, &tokens)?;

    {
        let mut tokens_guard = state.spotify_tokens.write();
        *tokens_guard = Some(tokens.clone());
    }

    let _ = app.emit("spotify-auth-complete", &tokens);

    log::info!("Spotify authentication completed successfully");
    Ok(tokens)
}

#[tauri::command]
pub fn complete_spotify_auth_manual(
    code: String,
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<SpotifyTokens, String> {
    // Get pending auth from AppState
    let pending = {
        let mut guard = state.pending_spotify_auth.write();
        guard
            .take()
            .ok_or("No pending Spotify auth. Please start auth again.")?
    };

    let tokens = crate::spotify::complete_spotify_auth(
        &code,
        &pending.verifier,
        &pending.client_id,
        &pending.client_secret,
        &pending.redirect_uri,
    )?;

    polling::save_spotify_tokens(&app, &tokens)?;

    {
        let mut tokens_guard = state.spotify_tokens.write();
        *tokens_guard = Some(tokens.clone());
    }

    let _ = app.emit("spotify-auth-complete", &tokens);

    log::info!("Spotify authentication completed (manual fallback)");
    Ok(tokens)
}

#[tauri::command]
pub fn get_spotify_tokens(
    state: tauri::State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<Option<SpotifyTokens>, String> {
    let state_tokens = {
        let guard = state.spotify_tokens.read();
        guard.clone()
    };

    if state_tokens.is_some() {
        return Ok(state_tokens);
    }

    let loaded = polling::load_spotify_tokens(&app)?;
    if let Some(tokens) = &loaded {
        let mut guard = state.spotify_tokens.write();
        *guard = Some(tokens.clone());
    }
    Ok(loaded)
}

#[tauri::command]
pub fn refresh_spotify(
    state: tauri::State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<(), String> {
    let store = app.store("tokens").map_err(|e| e.to_string())?;

    let client_id = store
        .get("spotify_client_id")
        .and_then(|v| v.as_str().map(String::from))
        .ok_or("Spotify client ID not found")?;
    let client_secret = store
        .get("spotify_client_secret")
        .and_then(|v| v.as_str().map(String::from))
        .ok_or("Spotify client secret not found")?;

    let current_tokens = {
        let guard = state.spotify_tokens.read();
        guard.clone().ok_or("No Spotify tokens to refresh")?
    };

    let new_tokens =
        crate::spotify::refresh_spotify_token(&current_tokens, &client_id, &client_secret)?;

    polling::save_spotify_tokens(&app, &new_tokens)?;

    {
        let mut guard = state.spotify_tokens.write();
        *guard = Some(new_tokens);
    }

    log::info!("Spotify token refreshed successfully");
    Ok(())
}

#[tauri::command]
pub fn open_external_url(url: String) -> Result<(), String> {
    tauri_plugin_opener::open_url(&url, None::<&str>)
        .map_err(|e| format!("Failed to open URL: {}", e))
}

#[tauri::command]
pub fn start_teams_auth_device_code(app: AppHandle) -> Result<DeviceCodeResponse, String> {
    log::info!("start_teams_auth_device_code called");
    let response = crate::teams::start_teams_auth_device_code()?;
    log::info!("start_teams_auth_device_code: got response");

    let store = app.store("tokens").map_err(|e| e.to_string())?;
    store.set("teams_device_code", serde_json::json!(response.device_code));
    store.save().map_err(|e| e.to_string())?;

    log::info!("Teams device code auth started");
    Ok(response)
}

#[tauri::command]
pub fn poll_teams_auth(
    device_code: String,
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<TeamsTokens, String> {
    let tokens = crate::teams::poll_teams_auth(&device_code)?;

    polling::save_teams_tokens(&app, &tokens)?;

    {
        let mut guard = state.teams_tokens.write();
        *guard = Some(tokens.clone());
    }

    let _ = app.emit("teams-auth-complete", &tokens);

    log::info!("Teams authentication completed successfully");
    Ok(tokens)
}

#[tauri::command]
pub fn complete_teams_auth_manual(
    code: String,
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<TeamsTokens, String> {
    let pending = {
        let mut guard = state.pending_teams_auth.write();
        guard
            .take()
            .ok_or("No pending Teams auth. Please start auth again.")?
    };

    let tokens = crate::teams::complete_teams_auth(
        &code,
        &pending.verifier,
        &pending.client_id,
        &pending.redirect_uri,
    )?;

    polling::save_teams_tokens(&app, &tokens)?;

    {
        let mut guard = state.teams_tokens.write();
        *guard = Some(tokens.clone());
    }

    let _ = app.emit("teams-auth-complete", &tokens);

    log::info!("Teams authentication completed (manual fallback)");
    Ok(tokens)
}

#[tauri::command]
pub fn get_teams_tokens(
    state: tauri::State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<Option<TeamsTokens>, String> {
    let state_tokens = {
        let guard = state.teams_tokens.read();
        guard.clone()
    };

    if state_tokens.is_some() {
        return Ok(state_tokens);
    }

    let loaded = polling::load_teams_tokens(&app)?;
    if let Some(tokens) = &loaded {
        let mut guard = state.teams_tokens.write();
        *guard = Some(tokens.clone());
    }
    Ok(loaded)
}

#[tauri::command]
pub fn refresh_teams(state: tauri::State<'_, Arc<AppState>>, app: AppHandle) -> Result<(), String> {
    let current_tokens = {
        let guard = state.teams_tokens.read();
        guard.clone().ok_or("No Teams tokens to refresh")?
    };

    let new_tokens = crate::teams::refresh_teams_token(&current_tokens)?;

    polling::save_teams_tokens(&app, &new_tokens)?;

    {
        let mut guard = state.teams_tokens.write();
        *guard = Some(new_tokens);
    }

    log::info!("Teams token refreshed successfully");
    Ok(())
}

#[tauri::command]
pub fn start_syncing(state: tauri::State<'_, Arc<AppState>>, app: AppHandle) -> Result<(), String> {
    let is_syncing = {
        let guard = state.is_syncing.read();
        *guard
    };

    if is_syncing {
        return Ok(());
    }

    let handle = polling::start_polling(Arc::clone(&state.inner()), app.clone())?;

    {
        let mut is_syncing_guard = state.is_syncing.write();
        *is_syncing_guard = true;
    }

    {
        let mut handle_guard = state.polling_handle.write();
        *handle_guard = Some(handle);
    }

    let _ = app.emit("sync-started", ());

    log::info!("Sync started");
    Ok(())
}

#[tauri::command]
pub fn stop_syncing(state: tauri::State<'_, Arc<AppState>>, app: AppHandle) -> Result<(), String> {
    polling::stop_polling(state.inner());

    {
        let mut handle_guard = state.polling_handle.write();
        if let Some(handle) = handle_guard.take() {
            handle.abort();
        }
    }

    let _ = app.emit("sync-stopped", ());

    log::info!("Sync stopped");
    Ok(())
}

#[tauri::command]
pub fn get_sync_status(state: tauri::State<'_, Arc<AppState>>) -> Result<SyncStatus, String> {
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

    Ok(SyncStatus {
        is_syncing,
        current_track,
        spotify_connected,
        teams_connected,
    })
}

#[tauri::command]
pub fn show_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
    Ok(())
}

#[tauri::command]
pub fn hide_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    Ok(())
}

#[tauri::command]
pub fn get_autostart_enabled(app: AppHandle) -> Result<bool, String> {
    let autolaunch_manager = app.state::<tauri_plugin_autostart::AutoLaunchManager>();
    let is_enabled = autolaunch_manager.is_enabled().map_err(|e| e.to_string())?;
    Ok(is_enabled)
}

#[tauri::command]
pub fn set_autostart_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    let autolaunch_manager = app.state::<tauri_plugin_autostart::AutoLaunchManager>();
    if enabled {
        autolaunch_manager.enable().map_err(|e| e.to_string())?;
    } else {
        autolaunch_manager.disable().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn open_logs_folder(app: AppHandle) -> Result<(), String> {
    let logs_path = app.path().app_log_dir().map_err(|e| e.to_string())?;
    let path_str = logs_path.to_string_lossy();
    tauri_plugin_opener::open_url(&path_str, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_external(url: String) -> Result<(), String> {
    tauri_plugin_opener::open_url(&url, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_current_track(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Option<TrackInfo>, String> {
    let guard = state.current_track.read();
    Ok(guard.clone())
}

#[tauri::command]
pub fn is_onboarding_complete() -> Result<bool, String> {
    let config = config::load_config()?;
    Ok(!config.spotify.client_id.is_empty())
}

#[tauri::command]
pub fn complete_onboarding(
    state: tauri::State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<(), String> {
    let has_spotify = {
        let guard = state.spotify_tokens.read();
        guard.is_some()
    };

    let has_teams = {
        let guard = state.teams_tokens.read();
        guard.is_some()
    };

    log::info!(
        "complete_onboarding: has_spotify={}, has_teams={}",
        has_spotify,
        has_teams
    );

    if has_spotify && has_teams {
        log::info!("complete_onboarding: starting sync");
        start_syncing(state, app)?;
        log::info!("complete_onboarding: sync started successfully");
    } else {
        log::warn!("complete_onboarding: missing tokens, not starting sync");
    }

    Ok(())
}
