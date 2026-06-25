use crate::config::{self, AppConfig};
use crate::spotify::{SpotifyTokens, TrackInfo};
use crate::teams::{DeviceCodeResponse, TeamsTokens};
use crate::token_io;
use crate::{polling, tray, AppState, PendingSpotifyAuth};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use url::Url;

/// TTL for the `is_onboarding_complete` result cache. The front-end remounts this
/// command on every Onboarding view enter, and the upstream HTTPS calls can take
/// up to 20s in the worst case (token validation against Spotify/Graph APIs), so
/// a short cache is needed to avoid hammering the upstream APIs.
const ONBOARDING_CACHE_TTL: Duration = Duration::from_secs(30);

/// Validates that a URL uses http or https scheme, has a host, and
/// contains no userinfo (the `user:pass@` form). Returns the parsed URL
/// on success, or an error string on failure. See issue #67.
fn validate_http_url(url: &str) -> Result<Url, String> {
    Url::parse(url)
        .map_err(|_| "Invalid URL format".to_string())
        .and_then(|parsed| {
            match parsed.scheme() {
                "http" | "https" => {}
                other => {
                    return Err(format!(
                        "Invalid URL scheme '{}': only http/https allowed",
                        other
                    ));
                }
            }
            if parsed.host_str().map(str::is_empty).unwrap_or(true) {
                return Err("URL has no host".to_string());
            }
            if !parsed.username().is_empty() || parsed.password().is_some() {
                return Err("URL has userinfo (user:pass@) — disallowed".to_string());
            }
            Ok(parsed)
        })
}

/// Validates a Spotify client_id (32 alphanumeric chars).
/// See issue #67.
fn validate_spotify_client_id(id: &str) -> Result<(), String> {
    if id.len() != 32 || !id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(format!(
            "Invalid client_id: must be 32 alphanumeric characters (got len={})",
            id.len()
        ));
    }
    Ok(())
}

/// Validates a Spotify client_secret (>= 32 chars, non-empty).
/// See issue #67.
fn validate_spotify_client_secret(secret: &str) -> Result<(), String> {
    if secret.len() < 32 {
        return Err(format!(
            "Invalid client_secret: must be at least 32 characters (got len={})",
            secret.len()
        ));
    }
    Ok(())
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
    log::debug!("[CMD] load_config: ENTRY");
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

    // On macOS, sync the app's activation policy with the saved
    // `start_minimized` preference so the dock icon disappears when the
    // user wants tray-only behavior and reappears when they disable it.
    // Setting on every save (not just on toggle) keeps the policy
    // idempotent and avoids tracking previous state. See audit Q4.
    //
    // Run BEFORE `set_autostart_enabled` because that helper takes
    // `app` by value, and `set_activation_policy` borrows it. This
    // ordering matches the new doc note on the lib.rs side.
    #[cfg(target_os = "macos")]
    {
        let policy = if config.teams.start_minimized {
            tauri::ActivationPolicy::Accessory
        } else {
            tauri::ActivationPolicy::Regular
        };
        // tauri::AppHandle::set_activation_policy returns () on success;
        // the underlying call logs its own errors via the tauri-runtime-wry
        // layer. We deliberately discard the unit value rather than wrapping
        // in `if let Err(...)`.
        let _ = app.set_activation_policy(policy);
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

/// Common PKCE OAuth flow for Spotify authorization. Builds the auth
/// URL, generates verifier/challenge/state, stores the pending auth
/// in AppState (in-memory only — never persisted), and opens the
/// browser. Does NOT touch the keychain — that's the caller's job.
///
/// Used by both `start_spotify_auth` (initial onboarding, writes
/// secret to keychain) and `start_spotify_reconnect` (keychain
/// already populated, reads from it).
fn run_spotify_oauth_flow(
    client_id: String,
    redirect_uri: String,
    state: &tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let verifier = crate::pkce::generate_verifier();
    log::info!(
        "[CMD] run_spotify_oauth_flow: verifier generated, len={}",
        verifier.len()
    );

    let challenge = crate::pkce::generate_challenge(&verifier);
    log::info!("[CMD] run_spotify_oauth_flow: challenge generated");

    let csrf_state = crate::pkce::generate_verifier();
    log::info!(
        "[CMD] run_spotify_oauth_flow: state generated, len={}",
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
        "[CMD] run_spotify_oauth_flow: auth_url created, length={}",
        auth_url.len()
    );

    // Spotify authorization codes expire in 10 minutes (600 seconds)
    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(600);

    // Store pending auth in AppState only. We deliberately do NOT persist
    // the PKCE verifier to disk — it's a 10-minute bearer credential and
    // disk persistence leaks it to filesystem-level attackers. See issue
    // #65 / HIGH #3.
    {
        let mut pending = state.pending_spotify_auth.write();
        *pending = Some(PendingSpotifyAuth {
            verifier,
            state: csrf_state,
            client_id,
            redirect_uri,
            expires_at,
        });
        log::info!(
            "[CMD] run_spotify_oauth_flow: stored pending auth in AppState (in-memory only)"
        );
    }

    if let Err(e) = tauri_plugin_opener::open_url(&auth_url, None::<&str>) {
        log::warn!(
            "[CMD] run_spotify_oauth_flow: Failed to open browser: {}",
            e
        );
    } else {
        log::info!("[CMD] run_spotify_oauth_flow: Browser opened successfully");
    }

    Ok(())
}

#[tauri::command]
pub fn start_spotify_auth(
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    _app: AppHandle,
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

    // Issue #67: validate the inputs at the IPC boundary, not just in
    // the frontend. The frontend regex was a UX nicety, not a security
    // boundary — a devtools-pasted invoke() with arbitrary strings was
    // accepted before this check.
    validate_spotify_client_id(&client_id)?;
    validate_spotify_client_secret(&client_secret)?;

    // Store the client_secret in the OS keychain. This is the only place the
    // secret is persisted from this point forward; it is intentionally NOT
    // included in `pending_spotify_auth` (AppState or store). See issue #9.
    crate::keychain::store_spotify_client_secret(&client_secret)?;
    log::info!("[CMD] start_spotify_auth: client_secret stored in keychain");

    run_spotify_oauth_flow(client_id, redirect_uri, &state)?;

    log::info!("[CMD] start_spotify_auth: SUCCESS - Spotify auth started");
    Ok(())
}

/// Reconnect Spotify by reading the existing `client_secret` from the
/// OS keychain (set during Onboarding). The frontend already verifies
/// the keychain entry via `is_spotify_client_secret_set` before calling
/// this; if it's missing the user is redirected to Onboarding instead.
///
/// Replaces the previous pattern of calling `start_spotify_auth` with
/// `clientSecret: ''`, which the #67 validator correctly rejected
/// (and which would have overwritten the existing keychain entry with
/// an empty string). See issues #9, #67, and the v2.6.4 verifier report.
#[tauri::command]
pub fn start_spotify_reconnect(
    client_id: String,
    redirect_uri: String,
    _app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    log::info!(
        "[CMD] start_spotify_reconnect: ENTRY - client_id.len={}, redirect_uri={}",
        client_id.len(),
        redirect_uri
    );

    if client_id.is_empty() {
        log::error!("[CMD] start_spotify_reconnect: client_id is empty");
        return Err("client_id is required".to_string());
    }

    // Read the existing secret from the keychain. This will return an
    // error if the entry is missing (e.g., user cleared the keychain
    // after Onboarding), in which case the frontend should redirect to
    // Onboarding rather than retry. The `_` prefix tells the compiler
    // we intentionally discard the value here — its presence (and the
    // `?` above) proves the keychain entry exists.
    let _client_secret = crate::keychain::get_spotify_client_secret()?;
    log::info!("[CMD] start_spotify_reconnect: client_secret loaded from keychain");

    // #67 validation: client_id format only — we never validate the
    // secret here because it's already in the keychain (validated at
    // Onboarding time).
    validate_spotify_client_id(&client_id)?;

    run_spotify_oauth_flow(client_id, redirect_uri, &state)?;

    log::info!("[CMD] start_spotify_reconnect: SUCCESS - Spotify reconnect started");
    Ok(())
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

    {
        let mut tokens_guard = state.spotify_tokens.write();
        *tokens_guard = Some(tokens.clone());
        log::info!("[CMD] complete_spotify_auth_manual: tokens stored in AppState");
    }
    token_io::persist_tokens(state.inner(), &app)?;
    log::info!("[CMD] complete_spotify_auth_manual: tokens persisted atomically");

    // Issue #70: invalidate the onboarding cache.
    state.onboarding_cache.invalidate();
    log::info!("[CMD] complete_spotify_auth_manual: onboarding_cache invalidated");

    log::info!("[CMD] complete_spotify_auth_manual: EMIT spotify-auth-complete event");
    let _ = app.emit("spotify-auth-complete", &tokens);

    log::info!("[CMD] complete_spotify_auth_manual: SUCCESS (manual fallback)");
    Ok(tokens)
}

// See issue #16: the cache-first, store-fallback pattern used to live in
// `crate::token_cache::get_cached_or_load`. Both `get_spotify_tokens` and
// `get_teams_tokens` have been removed — see issue #65. The webview no
// longer has a path to read tokens.

#[tauri::command]
pub fn refresh_spotify(
    state: tauri::State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<(), String> {
    log::debug!("[CMD] refresh_spotify: ENTRY");

    // Spotify client_id lives in the config (it's not a secret). The
    // client_secret is in the OS keychain (see issue #9). The previous
    // implementation read client_id from `tauri-plugin-store` — that
    // path is removed as part of issue #65.
    let client_id = {
        let guard = state.config.read();
        guard
            .as_ref()
            .map(|c| c.spotify.client_id.clone())
            .filter(|id| !id.is_empty())
            .ok_or_else(|| {
                log::error!("[CMD] refresh_spotify: Spotify client ID not found in config");
                "Spotify client ID not found".to_string()
            })?
    };
    let client_secret = crate::keychain::get_spotify_client_secret()?;
    log::info!("[CMD] refresh_spotify: credentials loaded (id from config, secret from keychain)");

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

    // CAS: only commit if state still holds the access token we refreshed
    // from. If state changed during the refresh (e.g. user clicked
    // Reconnect from another command), discard the result.
    let pre_refresh_access_token = current_tokens.access_token.clone();
    let committed = {
        let mut guard = state.spotify_tokens.write();
        if guard.as_ref().map(|t| &t.access_token) == Some(&pre_refresh_access_token) {
            *guard = Some(new_tokens.clone());
            true
        } else {
            log::warn!("[CMD] refresh_spotify: state changed during refresh, discarding result");
            false
        }
    };
    if committed {
        token_io::persist_tokens(state.inner(), &app)?;
        log::info!("[CMD] refresh_spotify: SUCCESS (state updated and persisted)");
    } else {
        log::info!("[CMD] refresh_spotify: NOOP (concurrent state change; not persisted)");
    }

    Ok(())
}

#[tauri::command]
pub fn open_external_url(url: String) -> Result<(), String> {
    log::debug!("[CMD] open_external_url: ENTRY - url.len={}", url.len());

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
    _app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<DeviceCodeResponse, String> {
    log::debug!("[CMD] start_teams_auth_device_code: ENTRY");

    let response = crate::teams::start_teams_auth_device_code()?;
    log::info!("[CMD] start_teams_auth_device_code: got device code response");
    log::info!(
        "[CMD] start_teams_auth_device_code: user_code={}, verification_url={}",
        response.user_code,
        response.verification_url
    );

    // Calculate expiry time (device codes typically expire in 900 seconds / 15 minutes)
    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(response.expires_in as i64);

    // Populate pending_teams_auth in AppState only. We deliberately do NOT
    // persist the device_code to disk — it's a 15-minute bearer credential
    // and disk persistence leaks it to filesystem-level attackers. See
    // issue #65 / HIGH #3. If the user crashes between this call and
    // `poll_teams_auth`, they re-start the device-code flow (cheap UX).
    {
        let mut pending = state.pending_teams_auth.write();
        *pending = Some(crate::PendingTeamsAuth {
            verifier: response.device_code.clone(),
            client_id: crate::teams::MICROSOFT_GRAPH_CLIENT_ID.to_string(),
            redirect_uri: "presencejam://callback".to_string(),
            expires_at,
        });
    }
    log::info!("[CMD] start_teams_auth_device_code: pending_teams_auth populated in AppState (in-memory only)");

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

    {
        let mut guard = state.teams_tokens.write();
        *guard = Some(tokens.clone());
        log::info!("[CMD] poll_teams_auth: tokens stored in AppState");
    }
    token_io::persist_tokens(state.inner(), &app)?;
    log::info!("[CMD] poll_teams_auth: tokens persisted atomically");

    // Clear pending Teams auth in AppState (no disk side — we never wrote it)
    {
        let mut pending = state.pending_teams_auth.write();
        *pending = None;
    }

    // Issue #70: invalidate the onboarding cache.
    state.onboarding_cache.invalidate();
    log::info!("[CMD] poll_teams_auth: onboarding_cache invalidated");

    log::info!("[CMD] poll_teams_auth: EMIT teams-auth-complete event");
    let _ = app.emit("teams-auth-complete", &tokens);

    log::info!("[CMD] poll_teams_auth: SUCCESS");
    Ok(tokens)
}

#[tauri::command]
pub fn refresh_teams(state: tauri::State<'_, Arc<AppState>>, app: AppHandle) -> Result<(), String> {
    log::debug!("[CMD] refresh_teams: ENTRY");

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

    // CAS: only commit if state still holds the access token we refreshed
    // from. If state changed during the refresh (e.g. user clicked
    // Reconnect from another command), discard the result.
    let pre_refresh_access_token = current_tokens.access_token.clone();
    let committed = {
        let mut guard = state.teams_tokens.write();
        if guard.as_ref().map(|t| &t.access_token) == Some(&pre_refresh_access_token) {
            *guard = Some(new_tokens.clone());
            true
        } else {
            log::warn!("[CMD] refresh_teams: state changed during refresh, discarding result");
            false
        }
    };
    if committed {
        token_io::persist_tokens(state.inner(), &app)?;
        log::info!("[CMD] refresh_teams: SUCCESS (state updated and persisted)");
    } else {
        log::info!("[CMD] refresh_teams: NOOP (concurrent state change; not persisted)");
    }

    Ok(())
}

#[tauri::command]
pub fn start_syncing(state: tauri::State<'_, Arc<AppState>>, app: AppHandle) -> Result<(), String> {
    log::debug!("[CMD] start_syncing: ENTRY");

    // Issue #69: drain any previous polling thread BEFORE claiming the
    // is_syncing flag. Without this, a fast Stop+Start cycle (within the
    // 2s stop_polling_and_join budget) can leave a stale thread running
    // while a new one starts — both read state.spotify_tokens, both call
    // the Spotify/Graph APIs, both rebuild the tray menu.
    //
    // Only drain if a thread is actually running; the common case
    // (start_syncing from a fresh app start) skips this entirely.
    if state.is_syncing.load(Ordering::Acquire) {
        log::info!("[CMD] start_syncing: previous thread still running; draining");
        stop_polling_and_join(state.inner(), "start_syncing_drain");
    }

    // Use compare_exchange for an atomic check-and-set. AcqRel on
    // success preserves the happens-before relationship with subsequent
    // reads of is_syncing (e.g. the polling loop and tray).
    {
        if state
            .is_syncing
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            log::info!("[CMD] start_syncing: already syncing (race lost), returning early");
            return Ok(());
        }
        log::info!("[CMD] start_syncing: is_syncing flag set to true");
    }

    let handle = match polling::start_polling(Arc::clone(state.inner()), app.clone()) {
        Ok(h) => h,
        Err(e) => {
            // Roll back is_syncing flag since no handle was created
            log::error!(
                "[CMD] start_syncing: polling start failed - {}; rolling back is_syncing",
                e
            );
            state.is_syncing.store(false, Ordering::Release);
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
            log::warn!(
                "[CMD] {}: polling thread did not terminate within 2s, attempting final join",
                context
            );
            match handle.join() {
                Ok(()) => {
                    log::info!("[CMD] {}: polling thread ended (final join)", context);
                }
                Err(e) => {
                    log::error!(
                        "[CMD] {}: polling thread panicked (final join): {:?}",
                        context,
                        e
                    );
                }
            }
        }
    }
}

#[tauri::command]
pub fn stop_syncing(state: tauri::State<'_, Arc<AppState>>, app: AppHandle) -> Result<(), String> {
    log::debug!("[CMD] stop_syncing: ENTRY");

    stop_polling_and_join(state.inner(), "stop_syncing");

    log::info!("[CMD] stop_syncing: EMIT sync-stopped event");
    let _ = app.emit("sync-stopped", ());

    log::info!("[CMD] stop_syncing: SUCCESS");
    Ok(())
}

#[tauri::command]
pub fn app_exit(state: tauri::State<'_, Arc<AppState>>, app: AppHandle) -> Result<(), String> {
    log::debug!("[CMD] app_exit: ENTRY");

    let is_syncing = state.is_syncing.load(Ordering::Acquire);

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
    log::debug!("[CMD] get_sync_status: ENTRY");

    let is_syncing = state.is_syncing.load(Ordering::Acquire);

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
    log::debug!("[CMD] show_window: ENTRY");

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
pub fn set_autostart_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    log::debug!("[CMD] set_autostart_enabled: ENTRY - enabled={}", enabled);

    let autolaunch_manager = app.state::<tauri_plugin_autostart::AutoLaunchManager>();
    let is_enabled = autolaunch_manager.is_enabled().map_err(|e| {
        log::error!(
            "[CMD] set_autostart_enabled: is_enabled check FAILED - {}",
            e
        );
        e.to_string()
    })?;

    if is_enabled == enabled {
        log::info!(
            "[CMD] set_autostart_enabled: already in desired state (enabled={}), no-op",
            enabled
        );
        return Ok(());
    }

    if enabled {
        autolaunch_manager.enable().map_err(|e| {
            log::error!("[CMD] set_autostart_enabled: enable FAILED - {}", e);
            e.to_string()
        })?;
        log::info!("[CMD] set_autostart_enabled: enable SUCCESS");
    } else {
        autolaunch_manager.disable().map_err(|e| {
            log::error!("[CMD] set_autostart_enabled: disable FAILED - {}", e);
            e.to_string()
        })?;
        log::info!("[CMD] set_autostart_enabled: disable SUCCESS");
    }
    Ok(())
}

#[tauri::command]
pub fn open_logs_folder(app: AppHandle) -> Result<(), String> {
    log::debug!("[CMD] open_logs_folder: ENTRY");

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

// `open_external` and `get_current_track` were removed in v2.6.4 (issue #77).
// `open_external_url` is the only URL-opener the Svelte code calls; the
// current track is read from `spotify-track-changed` events and from
// `get_sync_status`. Both dead commands were byte-for-byte duplicates of
// live paths.

/// Renders a status-format template against a sample track so the Svelte
/// Settings page can show a live preview without needing a real playing
/// track. Keeps the Rust `format_status` as the single source of truth for
/// the `{artist}` / `{track}` / `{album}` / `{emoji}` substitution rules.
/// See issue #74.
#[tauri::command]
pub fn preview_status(format: String) -> String {
    log::debug!("[CMD] preview_status: ENTRY - format.len={}", format.len());
    let result = crate::spotify::preview_status_with_sample(&format);
    log::debug!("[CMD] preview_status: SUCCESS");
    result
}

/// Onboarding check: `true` if both Spotify and Teams are configured and have a non-expired
/// token. Network errors (5xx, 429) are treated as "still valid" (transient) so a flaky
/// network doesn't bounce the user back into the onboarding flow.
///
/// Result is cached on `AppState.onboarding_cache` for [`ONBOARDING_CACHE_TTL`] —
/// the front-end remounts this command on every Onboarding view enter, and the
/// upstream HTTPS calls can take up to 20s in the worst case (token validation
/// against Spotify/Graph APIs).
#[tauri::command]
pub async fn is_onboarding_complete(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<bool, String> {
    log::debug!("[CMD] is_onboarding_complete: ENTRY");

    // Cache hit — return immediately.
    {
        let guard = state.onboarding_cache.lock();
        if let Some((ts, result)) = *guard {
            if ts.elapsed() < ONBOARDING_CACHE_TTL {
                log::info!(
                    "[CMD] is_onboarding_complete: cache HIT (age={:.2}s, result={})",
                    ts.elapsed().as_secs_f32(),
                    result
                );
                return Ok(result);
            }
        }
    }

    // Cache miss — run the actual validation on a blocking thread (HTTPS round-trips).
    let state_clone: Arc<AppState> = Arc::clone(&state);
    let result =
        tauri::async_runtime::spawn_blocking(move || is_onboarding_complete_impl(&state_clone))
            .await
            .map_err(|e| format!("is_onboarding_complete task panicked: {}", e))??;

    // Store result in cache. We cache both `true` and `false` outcomes — a recent "complete"
    // result is just as valid as a recent "incomplete" one for the 30s window.
    *state.onboarding_cache.lock() = Some((Instant::now(), result));
    log::info!(
        "[CMD] is_onboarding_complete: cache MISS, stored fresh result={}",
        result
    );
    Ok(result)
}

/// Blocking implementation of the onboarding check. Run via `spawn_blocking` from
/// `is_onboarding_complete` so the async runtime can keep serving other commands while
/// the HTTPS round-trips to Spotify/Graph complete.
fn is_onboarding_complete_impl(state: &Arc<AppState>) -> Result<bool, String> {
    let config = config::load_config()?;
    let spotify_configured = !config.spotify.client_id.is_empty();

    // Check Teams tokens — only ExpiredToken (401/403) means invalid.
    // RateLimited (429) and Transient (5xx, network) are temporary → treat as valid.
    let (teams_configured, teams_valid) = {
        let guard = state.teams_tokens.read();
        match guard.as_ref() {
            Some(tokens) => {
                let valid = match crate::teams::validate_teams_token(tokens) {
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
                let valid = match crate::spotify::validate_spotify_token(tokens) {
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
    log::debug!("[CMD] complete_onboarding: ENTRY");

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
    log::debug!("[CMD] reconnect_spotify: ENTRY");

    // Clear Spotify tokens from state
    *state.spotify_tokens.write() = None;
    log::info!("[CMD] reconnect_spotify: cleared spotify_tokens");

    // Clear pending Spotify auth
    *state.pending_spotify_auth.write() = None;
    log::info!("[CMD] reconnect_spotify: cleared pending_spotify_auth");

    // Persist the cleared state to disk atomically.
    if let Err(e) = token_io::persist_tokens(state.inner(), &app) {
        log::warn!(
            "[CMD] reconnect_spotify: failed to persist cleared state - {}",
            e
        );
    }

    // Issue #70: invalidate the onboarding cache so the UI sees the cleared state.
    state.onboarding_cache.invalidate();
    log::info!("[CMD] reconnect_spotify: onboarding_cache invalidated");

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
    log::debug!("[CMD] reconnect_teams: ENTRY");

    // Clear Teams tokens from state
    *state.teams_tokens.write() = None;
    log::info!("[CMD] reconnect_teams: cleared teams_tokens");

    // Clear pending Teams auth
    *state.pending_teams_auth.write() = None;
    log::info!("[CMD] reconnect_teams: cleared pending_teams_auth");

    // Persist the cleared state to disk atomically.
    if let Err(e) = token_io::persist_tokens(state.inner(), &app) {
        log::warn!(
            "[CMD] reconnect_teams: failed to persist cleared state - {}",
            e
        );
    }

    // Issue #70: invalidate the onboarding cache.
    state.onboarding_cache.invalidate();
    log::info!("[CMD] reconnect_teams: onboarding_cache invalidated");

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
