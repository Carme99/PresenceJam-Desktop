//! Spotify playback-control Tauri commands.
//!
//! See issue #3.0-P3. Thin wrappers over `crate::spotify` player fns: the
//! tray menu dispatches player actions directly from Rust (no frontend
//! roundtrip), while `get_spotify_granted_scopes` powers the Settings
//! one-time-reconnect banner for the new `user-modify-playback-state`
//! scope.

use crate::spotify::{
    decode_spotify_granted_scopes, is_token_expired, refresh_spotify_token, DeviceInfo, QueueInfo,
    SpotifyApiError, SpotifyTokens,
};
use std::sync::Arc;
use tauri::{AppHandle, State};

/// Log tag prefix for this submodule (issue #79 item 3).
const CMD: &str = "[CMD.PLAYBACK]";

/// Never-re-auth token getter (issues #375/#428): return the stored Spotify
/// access token when it is still fresh; otherwise attempt one
/// `refresh_spotify_token` + CAS-commit + persist and return the fresh
/// token. Only a dead refresh token (`InvalidGrant`) surfaces the reconnect
/// wording — any other refresh failure falls back to the stale token so the
/// caller can attempt the call and classify the wire error itself.
fn refreshed_access_token(state: &Arc<crate::AppState>, app: &AppHandle) -> Result<String, String> {
    let current = state
        .tokens
        .spotify()
        .clone()
        .ok_or_else(|| "Spotify is not connected".to_string())?;
    if !is_token_expired(&current) {
        return Ok(current.access_token);
    }
    log::info!("{CMD} refreshed_access_token: Spotify token expired, refreshing...");
    match try_refresh_spotify_token(state, app, &current) {
        Ok(token) => Ok(token),
        Err(SpotifyApiError::InvalidGrant) => {
            Err("Spotify session invalid - reconnect from Settings".to_string())
        }
        Err(_) => Ok(current.access_token),
    }
}

/// One Spotify refresh + CAS-commit + persist. Returns the committed (or
/// concurrently-winning) access token; the typed `SpotifyApiError` lets the
/// caller keep reconnect wording for dead credentials only.
fn try_refresh_spotify_token(
    state: &Arc<crate::AppState>,
    app: &AppHandle,
    current: &SpotifyTokens,
) -> Result<String, SpotifyApiError> {
    let client_id = state
        .config
        .get()
        .as_ref()
        .map(|c| c.spotify.client_id.clone())
        .unwrap_or_default();
    let client_secret = crate::keychain::peek_spotify_client_secret().unwrap_or_default();
    if client_id.is_empty() || client_secret.is_empty() {
        return Err(SpotifyApiError::Other(
            "Spotify credentials unavailable".to_string(),
        ));
    }
    let pre_refresh_access_token = current.access_token.clone();
    let new_tokens = refresh_spotify_token(current, &client_id, &client_secret)?;
    // CAS: only commit if state still holds the token we refreshed from.
    let committed = {
        let mut guard = state.tokens.spotify_mut();
        if guard.as_ref().map(|t| &t.access_token) == Some(&pre_refresh_access_token) {
            *guard = Some(new_tokens.clone());
            true
        } else {
            log::warn!(
                "{CMD} try_refresh_spotify_token: state changed during refresh, keeping current tokens"
            );
            false
        }
    };
    if committed {
        if let Err(e) = crate::token_io::persist_tokens(state, app) {
            log::warn!(
                "{CMD} try_refresh_spotify_token: failed to persist refreshed spotify tokens: {}",
                e
            );
        }
        Ok(new_tokens.access_token)
    } else {
        Ok(state
            .tokens
            .spotify()
            .clone()
            .map(|t| t.access_token)
            .unwrap_or(pre_refresh_access_token))
    }
}

/// Runs a Spotify player call with never-re-auth semantics (issues #375,
/// #428): proactive `refreshed_access_token`, then on `ExpiredToken` one
/// refresh + single retry before surfacing the reconnect message. All six
/// player commands route through here so the refresh policy lives in one
/// place.
fn player_with_refresh<T>(
    state: &Arc<crate::AppState>,
    app: &AppHandle,
    label: &str,
    call: impl Fn(&str) -> Result<T, SpotifyApiError>,
) -> Result<T, String> {
    let token = refreshed_access_token(state, app)?;
    match call(&token) {
        Err(SpotifyApiError::ExpiredToken) => {
            log::info!("{CMD} {label}: ExpiredToken, attempting one refresh + retry");
            let current = state.tokens.spotify().clone();
            match current {
                Some(current_tokens) if current_tokens.access_token != token => {
                    // A concurrent refresh already won; retry once with it.
                    call(&current_tokens.access_token).map_err(friendly_playback_error)
                }
                Some(current_tokens) => {
                    match try_refresh_spotify_token(state, app, &current_tokens) {
                        Ok(retry_token) => call(&retry_token).map_err(friendly_playback_error),
                        Err(SpotifyApiError::InvalidGrant) => {
                            Err("Spotify session invalid - reconnect from Settings".to_string())
                        }
                        Err(e) => Err(e.to_string()),
                    }
                }
                None => Err("Spotify is not connected".to_string()),
            }
        }
        result => result.map_err(friendly_playback_error),
    }
}

/// Maps a `SpotifyApiError` from a player call to a user-facing message,
/// with dedicated wording for the no-active-device and non-Premium cases.
fn friendly_playback_error(err: SpotifyApiError) -> String {
    match err {
        SpotifyApiError::NoActiveDevice => {
            "No active playback device - pick one from the tray Devices menu".to_string()
        }
        SpotifyApiError::NotPremium => "Playback control requires Spotify Premium".to_string(),
        SpotifyApiError::ExpiredToken => {
            "Spotify session expired - reconnect from Settings".to_string()
        }
        SpotifyApiError::InvalidGrant => {
            "Spotify session invalid - reconnect from Settings".to_string()
        }
        SpotifyApiError::RateLimited(retry_after) => match retry_after {
            Some(secs) => format!("Spotify is rate limiting requests (retry after {}s)", secs),
            None => "Spotify is rate limiting requests".to_string(),
        },
        SpotifyApiError::Other(s) => s,
    }
}

/// Resumes playback on the active device (or the given one via
/// `playback_transfer`). See issue #3.0-P3.
#[tauri::command]
pub async fn playback_play(
    state: State<'_, Arc<crate::AppState>>,
    app: AppHandle,
) -> Result<(), String> {
    log::debug!("{CMD} playback_play: ENTRY");
    let state_inner = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        player_with_refresh(&state_inner, &app, "playback_play", |token| {
            crate::spotify::player_play(token, None)
        })
    })
    .await
    .map_err(|e| format!("{CMD} playback_play: task panicked: {e}"))?
}

/// Pauses playback on the active device. See issue #3.0-P3.
#[tauri::command]
pub async fn playback_pause(
    state: State<'_, Arc<crate::AppState>>,
    app: AppHandle,
) -> Result<(), String> {
    log::debug!("{CMD} playback_pause: ENTRY");
    let state_inner = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        player_with_refresh(&state_inner, &app, "playback_pause", |token| {
            crate::spotify::player_pause(token, None)
        })
    })
    .await
    .map_err(|e| format!("{CMD} playback_pause: task panicked: {e}"))?
}

/// Skips to the next track on the active device. See issue #3.0-P3.
#[tauri::command]
pub async fn playback_next(
    state: State<'_, Arc<crate::AppState>>,
    app: AppHandle,
) -> Result<(), String> {
    log::debug!("{CMD} playback_next: ENTRY");
    let state_inner = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        player_with_refresh(&state_inner, &app, "playback_next", |token| {
            crate::spotify::player_next(token, None)
        })
    })
    .await
    .map_err(|e| format!("{CMD} playback_next: task panicked: {e}"))?
}

/// Skips to the previous track on the active device. See issue #3.0-P3.
#[tauri::command]
pub async fn playback_previous(
    state: State<'_, Arc<crate::AppState>>,
    app: AppHandle,
) -> Result<(), String> {
    log::debug!("{CMD} playback_previous: ENTRY");
    let state_inner = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        player_with_refresh(&state_inner, &app, "playback_previous", |token| {
            crate::spotify::player_previous(token, None)
        })
    })
    .await
    .map_err(|e| format!("{CMD} playback_previous: task panicked: {e}"))?
}

/// Transfers playback to the given device id, starting playback there.
/// See issue #3.0-P3.
#[tauri::command]
pub async fn playback_transfer(
    device_id: String,
    state: State<'_, Arc<crate::AppState>>,
    app: AppHandle,
) -> Result<(), String> {
    log::debug!(
        "{CMD} playback_transfer: ENTRY - device_id.len={}",
        device_id.len()
    );
    let state_inner = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        player_with_refresh(&state_inner, &app, "playback_transfer", |token| {
            crate::spotify::player_transfer(token, &device_id, true)
        })
    })
    .await
    .map_err(|e| format!("{CMD} playback_transfer: task panicked: {e}"))?
}

/// Lists the user's available playback devices. See issue #3.0-P3.
#[tauri::command]
pub async fn get_playback_devices(
    state: State<'_, Arc<crate::AppState>>,
    app: AppHandle,
) -> Result<Vec<DeviceInfo>, String> {
    log::debug!("{CMD} get_playback_devices: ENTRY");
    let state_inner = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        player_with_refresh(&state_inner, &app, "get_playback_devices", |token| {
            crate::spotify::get_devices(token)
        })
    })
    .await
    .map_err(|e| format!("{CMD} get_playback_devices: task panicked: {e}"))?
}

/// Fetches the user's playback queue (currently playing + up to the whole
/// up-next list). See issue #3.0-P3.
#[tauri::command]
pub async fn get_playback_queue(
    state: State<'_, Arc<crate::AppState>>,
    app: AppHandle,
) -> Result<QueueInfo, String> {
    log::debug!("{CMD} get_playback_queue: ENTRY");
    let state_inner = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        player_with_refresh(&state_inner, &app, "get_playback_queue", |token| {
            crate::spotify::get_queue(token)
        })
    })
    .await
    .map_err(|e| format!("{CMD} get_playback_queue: task panicked: {e}"))?
}

/// Returns the scopes granted on the stored Spotify access token by
/// base64url-decoding its JWT payload (informational only — no signature
/// verification). Empty when undecodable. The Settings page uses this to
/// detect a missing `user-modify-playback-state` and show the one-time
/// reconnect banner. See issue #3.0-P3.
#[tauri::command]
pub fn get_spotify_granted_scopes(state: State<'_, Arc<crate::AppState>>) -> Vec<String> {
    log::debug!("{CMD} get_spotify_granted_scopes: ENTRY");
    match state.tokens.spotify().as_ref() {
        Some(tokens) => decode_spotify_granted_scopes(&tokens.access_token),
        None => Vec::new(),
    }
}
