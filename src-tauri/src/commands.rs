use crate::config::{self, AppConfig};
use crate::spotify::{SpotifyTokens, TrackInfo};
use crate::teams::{DeviceCodeResponse, TeamsTokens};
use crate::{polling, tray, AppState, PendingSpotifyAuth};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_store::StoreExt;
use url::Url;

#[tauri::command]
pub fn get_recent_logs(_app: AppHandle) -> Result<(), String> {
    log::info!("[CMD] get_recent_logs: ENTRY");
    // Note: tauri_plugin_log v2 doesn't provide a way to retrieve cached log entries.
    // Log entries are streamed to the frontend via the Webview target (log://log event).
    // This command is kept for future use if such API becomes available.
    log::info!("[CMD] get_recent_logs: SUCCESS");
    Ok(())
}

/// Validates that a URL uses http or https scheme.
/// Returns the parsed URL on success, or an error string on failure.
/// See issue #14.
fn validate_http_url(url: &str) -> Result<Url, String> {
    Url::parse(url).map_err(|_| "Invalid URL format".to_string()).and_then(|parsed| {
        match parsed.scheme() {
            "http" | "https" => Ok(parsed),
            other => Err(format!(
                "Invalid URL scheme '{}': only http/https allowed",
                other
            )),
        }
    })
}

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
pub fn save_config(
    app: AppHandle,
    config: AppConfig,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    log::info!(
        "[CMD] save_config: ENTRY - config.spotify.client_id.len={}",
        config.spotify.client_id.len()
    );

    // Hold the write lock for the entire read-modify-write to prevent races
    // with concurrent reads from the polling loop. See bug #26.
    {
        let mut config_guard = state.config.write();
        match config::save_config(&config) {
            Ok(()) => {
                log::info!("[CMD] save_config: file saved successfully");
                *config_guard = Some(config.clone());
            }
            Err(e) => {
                log::error!("[CMD] save_config: FAILED - {}", e);
                return Err(e);
            }
        }
    }

    // Sync autostart state with the OS autostart manager
    #[cfg(desktop)]
    {
        if let Err(e) = set_autostart_enabled(app, config.autostart) {
            log::warn!("[CMD] save_config: failed to sync autostart state: {}", e);
        }
    }
    log::info!("[CMD] save_config: SUCCESS");
    Ok(())
}

/// Returns true iff the Spotify `client_secret` is currently stored in the
/// OS keychain. The frontend uses this to decide whether the user can
/// reconnect (keychain populated) or must re-enter the secret via
/// Onboarding (keychain empty). See issue #9.
#[tauri::command]
pub fn is_spotify_client_secret_set() -> bool {
    crate::keychain::has_spotify_client_secret()
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

    if client_id.is_empty() || client_secret.is_empty() {
        log::error!("[CMD] start_spotify_auth: client_id or client_secret is empty");
        return Err("client_id and client_secret are required".to_string());
    }

    let verifier = crate::spotify::pkce_generate_verifier();
    log::info!(
        "[CMD] start_spotify_auth: verifier generated, len={}",
        verifier.len()
    );

    let challenge = crate::spotify::pkce_generate_challenge(&verifier);
    log::info!("[CMD] start_spotify_auth: challenge generated");

    let csrf_state = crate::spotify::pkce_generate_verifier();
    log::info!(
        "[CMD] start_spotify_auth: state generated, len={}",
        csrf_state.len()
    );

    let auth_url = format!(
        "https://accounts.spotify.com/authorize\
         ?client_id={}\
         &response_type=code\
         &redirect_uri={}\
         &code_challenge_method=S256\
         &code_challenge={}\
         &state={}\
         &scope=user-read-currently-playing user-read-playback-state",
        client_id,
        urlencoding::encode(&redirect_uri),
        urlencoding::encode(&challenge),
        urlencoding::encode(&csrf_state)
    );
    log::info!(
        "[CMD] start_spotify_auth: auth_url created, length={}",
        auth_url.len()
    );

    // Spotify authorization codes expire in 10 minutes (600 seconds)
    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(600);

    // Store the client_secret in the OS keychain. This is the only place the
    // secret is persisted from this point forward; it is intentionally NOT
    // included in `pending_spotify_auth` (AppState or store). See issue #9.
    crate::keychain::store_spotify_client_secret(&client_secret)?;
    log::info!("[CMD] start_spotify_auth: client_secret stored in keychain");

    // Store pending auth in AppState (without the secret)
    {
        let mut pending = state.pending_spotify_auth.write();
        *pending = Some(PendingSpotifyAuth {
            verifier: verifier.clone(),
            state: csrf_state.clone(),
            client_id: client_id.clone(),
            redirect_uri: redirect_uri.clone(),
            expires_at,
        });
        log::info!("[CMD] start_spotify_auth: stored pending auth in AppState");
    }

    // Persist complete pending auth to store for crash recovery
    // (no client_secret — it's in the keychain)
    let store = app.store("tokens").map_err(|e| e.to_string())?;
    store.set(
        "pending_spotify_auth",
        serde_json::json!({
            "verifier": verifier,
            "state": csrf_state,
            "client_id": client_id,
            "redirect_uri": redirect_uri,
            "expires_at": expires_at.to_rfc3339(),
        }),
    );
    store.save().map_err(|e| e.to_string())?;
    log::info!("[CMD] start_spotify_auth: pending auth persisted to store");

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
    redirect_uri: String,
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<SpotifyTokens, String> {
    log::info!(
        "[CMD] complete_spotify_auth: ENTRY - code.len={}, verifier.len={}",
        code.len(),
        verifier.len()
    );

    // The client_secret is no longer accepted as a parameter — it is read
    // from the OS keychain. See issue #9. The keychain entry is populated
    // by `start_spotify_auth` (the normal flow) and survives a crash.
    let client_secret = crate::keychain::get_spotify_client_secret()?;

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

    // Clear pending Spotify auth from store and AppState on success
    {
        let mut pending = state.pending_spotify_auth.write();
        *pending = None;
    }
    let store = app.store("tokens").map_err(|e| e.to_string())?;
    store.delete("pending_spotify_auth");
    store.save().map_err(|e| e.to_string())?;
    log::info!("[CMD] complete_spotify_auth: pending Spotify auth cleared from store");

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

    // Read the client_secret from the keychain (it was placed there by
    // `start_spotify_auth` — see issue #9).
    let client_secret = crate::keychain::get_spotify_client_secret()?;

    let tokens = crate::spotify::complete_spotify_auth(
        &code,
        &pending.verifier,
        &pending.client_id,
        &client_secret,
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

    // Clear pending Spotify auth from store and AppState on success
    {
        let mut pending = state.pending_spotify_auth.write();
        *pending = None;
    }
    let store = app.store("tokens").map_err(|e| e.to_string())?;
    store.delete("pending_spotify_auth");
    store.save().map_err(|e| e.to_string())?;
    log::info!("[CMD] complete_spotify_auth_manual: pending Spotify auth cleared from store");

    log::info!("[CMD] complete_spotify_auth_manual: EMIT spotify-auth-complete event");
    let _ = app.emit("spotify-auth-complete", &tokens);

    log::info!("[CMD] complete_spotify_auth_manual: SUCCESS (manual fallback)");
    Ok(tokens)
}

// NOTE: get_spotify_tokens and get_teams_tokens have similar structure but are
// kept separate for clarity. The token types, store keys, and extraction logic differ
// enough that extracting a generic helper would reduce readability without adding
// much value. See issue #16.
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
    // Client secret now lives in the OS keychain (see issue #9); it is no
    // longer persisted to the store.
    let client_secret = crate::keychain::get_spotify_client_secret()?;
    log::info!("[CMD] refresh_spotify: credentials loaded (id from store, secret from keychain)");

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

    // Validate URL scheme - only allow http/https. See issue #14.
    validate_http_url(&url)?;

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
pub fn start_teams_auth_device_code(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<DeviceCodeResponse, String> {
    log::info!("[CMD] start_teams_auth_device_code: ENTRY");

    let response = crate::teams::start_teams_auth_device_code()?;
    log::info!("[CMD] start_teams_auth_device_code: got device code response");
    log::info!(
        "[CMD] start_teams_auth_device_code: user_code={}, verification_url={}",
        response.user_code,
        response.verification_url
    );

    // Calculate expiry time (device codes typically expire in 900 seconds / 15 minutes)
    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(response.expires_in as i64);

    let store = app.store("tokens").map_err(|e| e.to_string())?;
    store.set("teams_device_code", serde_json::json!(response.device_code));
    store.set(
        "pending_teams_auth",
        serde_json::json!({
            "verifier": response.device_code,
            "client_id": crate::teams::MICROSOFT_GRAPH_CLIENT_ID,
            "redirect_uri": "presencejam://callback",
            "expires_at": expires_at.to_rfc3339(),
        }),
    );
    store.save().map_err(|e| e.to_string())?;
    log::info!("[CMD] start_teams_auth_device_code: device code and pending auth persisted to store");

    // Populate pending_teams_auth so complete_teams_auth_manual and handle_teams_callback can work.
    // See issue #8.
    {
        let mut pending = state.pending_teams_auth.write();
        *pending = Some(crate::PendingTeamsAuth {
            verifier: response.device_code.clone(),
            client_id: crate::teams::MICROSOFT_GRAPH_CLIENT_ID.to_string(),
            redirect_uri: "presencejam://callback".to_string(),
            expires_at,
        });
    }
    log::info!("[CMD] start_teams_auth_device_code: pending_teams_auth populated");

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

    // Clear pending Teams auth from store and AppState on success
    {
        let mut pending = state.pending_teams_auth.write();
        *pending = None;
    }
    let store = app.store("tokens").map_err(|e| e.to_string())?;
    store.delete("pending_teams_auth");
    store.save().map_err(|e| e.to_string())?;
    log::info!("[CMD] poll_teams_auth: pending Teams auth cleared from store");

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

    // Clear pending Teams auth from store and AppState on success
    {
        let mut pending = state.pending_teams_auth.write();
        *pending = None;
    }
    let store = app.store("tokens").map_err(|e| e.to_string())?;
    store.delete("pending_teams_auth");
    store.save().map_err(|e| e.to_string())?;
    log::info!("[CMD] complete_teams_auth_manual: pending Teams auth cleared from store");

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

    // Acquire write lock first to prevent TOCTOU race with concurrent calls.
    // See issue #14.
    {
        let mut is_syncing_guard = state.is_syncing.write();
        if *is_syncing_guard {
            log::info!("[CMD] start_syncing: already syncing, returning early");
            return Ok(());
        }
        *is_syncing_guard = true;
        log::info!("[CMD] start_syncing: is_syncing flag set to true");
    }

    let handle = match polling::start_polling(Arc::clone(&state.inner()), app.clone()) {
        Ok(h) => h,
        Err(e) => {
            // Roll back is_syncing flag since no handle was created
            log::error!("[CMD] start_syncing: polling start failed - {}; rolling back is_syncing", e);
            let mut guard = state.is_syncing.write();
            *guard = false;
            return Err(e);
        }
    };
    log::info!("[CMD] start_syncing: polling task spawned");

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

fn stop_polling_and_join(state: &Arc<AppState>, context: &str) {
    polling::stop_polling(state);
    {
        let mut handle_guard = state.polling_handle.write();
        if let Some(handle) = handle_guard.take() {
            drop(handle_guard); // Release lock while waiting

            // Give thread up to 2 seconds to finish cooperatively
            let started = std::time::Instant::now();
            while started.elapsed() < std::time::Duration::from_secs(2) {
                if handle.is_finished() {
                    match handle.join() {
                        Ok(()) => {
                            log::info!("[CMD] {}: polling thread ended", context);
                        }
                        Err(e) => {
                            log::error!("[CMD] {}: polling thread panicked: {:?}", context, e);
                        }
                    }
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }

            // Timeout reached - try one final join (may block briefly)
            log::warn!("[CMD] {}: polling thread did not terminate within 2s, attempting final join", context);
            match handle.join() {
                Ok(()) => {
                    log::info!("[CMD] {}: polling thread ended (final join)", context);
                }
                Err(e) => {
                    log::error!("[CMD] {}: polling thread panicked (final join): {:?}", context, e);
                }
            }
        }
    }
}

#[tauri::command]
pub fn stop_syncing(state: tauri::State<'_, Arc<AppState>>, app: AppHandle) -> Result<(), String> {
    log::info!("[CMD] stop_syncing: ENTRY");

    stop_polling_and_join(state.inner(), "stop_syncing");

    log::info!("[CMD] stop_syncing: EMIT sync-stopped event");
    let _ = app.emit("sync-stopped", ());

    log::info!("[CMD] stop_syncing: SUCCESS");
    Ok(())
}

#[tauri::command]
pub fn app_exit(state: tauri::State<'_, Arc<AppState>>, app: AppHandle) -> Result<(), String> {
    log::info!("[CMD] app_exit: ENTRY");

    let is_syncing = {
        let guard = state.is_syncing.read();
        *guard
    };

    if is_syncing {
        log::info!("[CMD] app_exit: stopping polling first");
        stop_polling_and_join(state.inner(), "app_exit");
    }

    log::info!("[CMD] app_exit: calling app.exit(0)");
    app.exit(0);
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
        let tokens = state.spotify_tokens.read();
        let config = state.config.read();
        tokens.is_some()
            && config
                .as_ref()
                .map(|c| !c.spotify.client_id.is_empty())
                .unwrap_or(false)
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

    // Validate URL scheme - only allow http/https. See issue #14.
    validate_http_url(&url)?;

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
pub fn is_onboarding_complete(state: tauri::State<'_, Arc<AppState>>) -> Result<bool, String> {
    log::info!("[CMD] is_onboarding_complete: ENTRY");

    let config = config::load_config()?;
    let spotify_configured = !config.spotify.client_id.is_empty();

    // Check Teams tokens — only ExpiredToken (401/403) means invalid.
    // RateLimited (429) and Transient (5xx, network) are temporary → treat as valid.
    let (teams_configured, teams_valid) = {
        let guard = state.teams_tokens.read();
        match guard.as_ref() {
            Some(tokens) => {
                let valid = match crate::teams::validate_teams_token(&tokens.access_token) {
                    Ok(()) => true,
                    Err(crate::teams::TeamsApiError::ExpiredToken(_)) => false,
                    Err(_) => true, // transient — still valid for onboarding
                };
                (true, valid)
            }
            None => (false, false),
        }
    };

    // Check Spotify tokens — only ExpiredToken means invalid.
    // RateLimited and Other are transient → treat as valid.
    let (spotify_valid, _spotify_token) = {
        let guard = state.spotify_tokens.read();
        match guard.as_ref() {
            Some(tokens) => {
                let valid = match crate::spotify::validate_spotify_token(&tokens.access_token) {
                    Ok(()) => true,
                    Err(crate::spotify::SpotifyApiError::ExpiredToken) => false,
                    Err(_) => true, // transient — still valid for onboarding
                };
                (valid, Some(tokens.clone()))
            }
            None => (false, None),
        }
    };

    // Onboarding is complete only if:
    // 1. Spotify is configured AND token is not permanently expired
    // 2. Teams is configured AND token is not permanently expired
    let complete = spotify_configured && spotify_valid && teams_configured && teams_valid;
    log::info!(
        "[CMD] is_onboarding_complete: result={} (spotify_configured={}, spotify_valid={}, teams_configured={}, teams_valid={})",
        complete,
        spotify_configured,
        spotify_valid,
        teams_configured,
        teams_valid
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
        log::error!(
            "[CMD] complete_onboarding: missing tokens, cannot start sync (spotify={}, teams={})",
            has_spotify,
            has_teams
        );
        return Err(format!(
            "Missing tokens: spotify={}, teams={}",
            has_spotify, has_teams
        ));
    }

    log::info!("[CMD] complete_onboarding: SUCCESS");
    Ok(())
}

#[tauri::command]
pub fn reconnect_spotify(
    state: tauri::State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<(), String> {
    log::info!("[CMD] reconnect_spotify: ENTRY");

    // Clear Spotify tokens from state
    *state.spotify_tokens.write() = None;
    log::info!("[CMD] reconnect_spotify: cleared spotify_tokens");

    // Clear pending Spotify auth
    *state.pending_spotify_auth.write() = None;
    log::info!("[CMD] reconnect_spotify: cleared pending_spotify_auth");

    // Clear persisted Spotify tokens
    if let Err(e) = polling::clear_spotify_tokens(&app) {
        log::warn!(
            "[CMD] reconnect_spotify: failed to clear persisted tokens - {}",
            e
        );
    }

    // Clear the client_secret from the OS keychain (see issue #9).
    // Best-effort: don't fail the disconnect if the keychain entry is
    // already gone or unavailable.
    if let Err(e) = crate::keychain::delete_spotify_client_secret() {
        log::warn!(
            "[CMD] reconnect_spotify: failed to clear keychain entry - {}",
            e
        );
    }

    // Emit event so UI can show re-auth flow
    if let Err(e) = app.emit("spotify-reconnect-required", ()) {
        log::error!("[CMD] reconnect_spotify: failed to emit event - {}", e);
    } else {
        log::info!("[CMD] reconnect_spotify: EMIT spotify-reconnect-required event");
    }

    log::info!("[CMD] reconnect_spotify: SUCCESS");
    Ok(())
}

#[tauri::command]
pub fn reconnect_teams(
    state: tauri::State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<(), String> {
    log::info!("[CMD] reconnect_teams: ENTRY");

    // Clear Teams tokens from state
    *state.teams_tokens.write() = None;
    log::info!("[CMD] reconnect_teams: cleared teams_tokens");

    // Clear pending Teams auth
    *state.pending_teams_auth.write() = None;
    log::info!("[CMD] reconnect_teams: cleared pending_teams_auth");

    // Clear persisted Teams tokens
    if let Err(e) = polling::clear_teams_tokens(&app) {
        log::warn!(
            "[CMD] reconnect_teams: failed to clear persisted tokens - {}",
            e
        );
    }

    // Emit event so UI can show re-auth flow
    if let Err(e) = app.emit("teams-reconnect-required", ()) {
        log::error!("[CMD] reconnect_teams: failed to emit event - {}", e);
    } else {
        log::info!("[CMD] reconnect_teams: EMIT teams-reconnect-required event");
    }

    log::info!("[CMD] reconnect_teams: SUCCESS");
    Ok(())
}

#[tauri::command]
pub fn update_tray_menu_state(
    app: AppHandle,
    is_syncing: bool,
    current_track: Option<TrackInfo>,
) -> Result<(), String> {
    log::info!(
        "[CMD] update_tray_menu_state: ENTRY - is_syncing={}",
        is_syncing
    );
    tray::update_tray_menu(&app, is_syncing, current_track)?;
    log::info!("[CMD] update_tray_menu_state: SUCCESS");
    Ok(())
}
