//! Spotify playback-control Tauri commands.
//!
//! See issue #3.0-P3. Thin wrappers over `crate::spotify` player fns: the
//! tray menu dispatches player actions directly from Rust (no frontend
//! roundtrip), while `get_spotify_granted_scopes` powers the Settings
//! one-time-reconnect banner for the new `user-modify-playback-state`
//! scope.

use crate::spotify::{decode_spotify_granted_scopes, DeviceInfo, QueueInfo, SpotifyApiError};
use std::sync::Arc;
use tauri::State;

/// Log tag prefix for this submodule (issue #79 item 3).
const CMD: &str = "[CMD.PLAYBACK]";

/// Reads the stored Spotify access token, failing with a friendly message
/// when the user isn't connected.
fn stored_access_token(state: &State<'_, Arc<crate::AppState>>) -> Result<String, String> {
    state
        .tokens
        .spotify()
        .as_ref()
        .map(|t| t.access_token.clone())
        .ok_or_else(|| "Spotify is not connected".to_string())
}

/// Maps a `SpotifyApiError` from a player call to a user-facing message,
/// with dedicated wording for the no-active-device and non-Premium cases.
fn friendly_playback_error(err: SpotifyApiError) -> String {
    match err {
        SpotifyApiError::NoActiveDevice => {
            "No active playback device - pick one from the tray Devices menu".to_string()
        }
        SpotifyApiError::NotPremium => {
            "Playback control requires Spotify Premium".to_string()
        }
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
pub fn playback_play(state: State<'_, Arc<crate::AppState>>) -> Result<(), String> {
    log::debug!("{CMD} playback_play: ENTRY");
    let token = stored_access_token(&state)?;
    crate::spotify::player_play(&token, None).map_err(friendly_playback_error)
}

/// Pauses playback on the active device. See issue #3.0-P3.
#[tauri::command]
pub fn playback_pause(state: State<'_, Arc<crate::AppState>>) -> Result<(), String> {
    log::debug!("{CMD} playback_pause: ENTRY");
    let token = stored_access_token(&state)?;
    crate::spotify::player_pause(&token, None).map_err(friendly_playback_error)
}

/// Skips to the next track on the active device. See issue #3.0-P3.
#[tauri::command]
pub fn playback_next(state: State<'_, Arc<crate::AppState>>) -> Result<(), String> {
    log::debug!("{CMD} playback_next: ENTRY");
    let token = stored_access_token(&state)?;
    crate::spotify::player_next(&token, None).map_err(friendly_playback_error)
}

/// Skips to the previous track on the active device. See issue #3.0-P3.
#[tauri::command]
pub fn playback_previous(state: State<'_, Arc<crate::AppState>>) -> Result<(), String> {
    log::debug!("{CMD} playback_previous: ENTRY");
    let token = stored_access_token(&state)?;
    crate::spotify::player_previous(&token, None).map_err(friendly_playback_error)
}

/// Transfers playback to the given device id, starting playback there.
/// See issue #3.0-P3.
#[tauri::command]
pub fn playback_transfer(device_id: String, state: State<'_, Arc<crate::AppState>>) -> Result<(), String> {
    log::debug!("{CMD} playback_transfer: ENTRY - device_id.len={}", device_id.len());
    let token = stored_access_token(&state)?;
    crate::spotify::player_transfer(&token, &device_id, true).map_err(friendly_playback_error)
}

/// Lists the user's available playback devices. See issue #3.0-P3.
#[tauri::command]
pub fn get_playback_devices(state: State<'_, Arc<crate::AppState>>) -> Result<Vec<DeviceInfo>, String> {
    log::debug!("{CMD} get_playback_devices: ENTRY");
    let token = stored_access_token(&state)?;
    crate::spotify::get_devices(&token).map_err(friendly_playback_error)
}

/// Fetches the user's playback queue (currently playing + up to the whole
/// up-next list). See issue #3.0-P3.
#[tauri::command]
pub fn get_playback_queue(state: State<'_, Arc<crate::AppState>>) -> Result<QueueInfo, String> {
    log::debug!("{CMD} get_playback_queue: ENTRY");
    let token = stored_access_token(&state)?;
    crate::spotify::get_queue(&token).map_err(friendly_playback_error)
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
