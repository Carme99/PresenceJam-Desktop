//! Microsoft Teams (device-code) authentication Tauri commands.
//!
//! See issue #76. Teams uses an OAuth 2.0 device-code flow rather than the
//! PKCE/redirect flow that Spotify uses.

use crate::teams::{decode_teams_granted_scopes, DeviceCodeResponse, TeamsTokens};
use crate::token_io;
use crate::AppState;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

/// Log tag prefix for this submodule (issue #79 item 3).
const CMD: &str = "[CMD.TEAMS_AUTH]";

#[tauri::command]
pub fn start_teams_auth_device_code(_app: AppHandle) -> Result<DeviceCodeResponse, String> {
    log::debug!("{CMD} start_teams_auth_device_code: ENTRY");

    let response = crate::teams::start_teams_auth_device_code()?;
    log::info!("{CMD} start_teams_auth_device_code: got device code response");
    log::info!(
        "{CMD} start_teams_auth_device_code: user_code={}, verification_url={}",
        response.user_code,
        response.verification_url
    );

    // No pending state is stored: the device code travels to the poll
    // command via the frontend, and a device-code flow needs no registered
    // redirect URI (Entra reply-url docs). See issue #158.

    log::info!("{CMD} start_teams_auth_device_code: SUCCESS");
    Ok(response)
}

#[tauri::command]
pub fn poll_teams_auth(
    device_code: String,
    interval: u64,
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<TeamsTokens, String> {
    log::info!(
        "{CMD} poll_teams_auth: ENTRY - device_code.len={}, interval={}",
        device_code.len(),
        interval
    );

    let tokens = crate::teams::poll_teams_auth(&device_code, interval)?;
    log::info!(
        "{CMD} poll_teams_auth: poll successful - access_token.len={}",
        tokens.access_token.len()
    );

    {
        let mut guard = state.tokens.teams_mut();
        *guard = Some(tokens.clone());
        log::info!("{CMD} poll_teams_auth: tokens stored in AppState");
    }
    token_io::persist_tokens(state.inner(), &app)?;
    log::info!("{CMD} poll_teams_auth: tokens persisted atomically");

    // Issue #70: invalidate the onboarding cache.
    state.onboarding_cache.invalidate();
    log::info!("{CMD} poll_teams_auth: onboarding_cache invalidated");

    log::info!("{CMD} poll_teams_auth: EMIT teams-auth-complete event");
    let _ = app.emit("teams-auth-complete", &tokens);

    log::info!("{CMD} poll_teams_auth: SUCCESS");
    Ok(tokens)
}

#[tauri::command]
pub fn refresh_teams(state: tauri::State<'_, Arc<AppState>>, app: AppHandle) -> Result<(), String> {
    log::debug!("{CMD} refresh_teams: ENTRY");

    let current_tokens = {
        let guard = state.tokens.teams();
        guard.clone().ok_or_else(|| {
            log::error!("{CMD} refresh_teams: No Teams tokens in state");
            "No Teams tokens to refresh".to_string()
        })?
    };
    log::info!("{CMD} refresh_teams: current tokens found");

    let new_tokens = crate::teams::refresh_teams_token(&current_tokens)
        .map_err(|e| e.to_string())?;
    log::info!("{CMD} refresh_teams: new tokens received");

    // CAS: only commit if state still holds the access token we refreshed
    // from. If state changed during the refresh (e.g. user clicked
    // Reconnect from another command), discard the result.
    let pre_refresh_access_token = current_tokens.access_token.clone();
    let committed = {
        let mut guard = state.tokens.teams_mut();
        if guard.as_ref().map(|t| &t.access_token) == Some(&pre_refresh_access_token) {
            *guard = Some(new_tokens.clone());
            true
        } else {
            log::warn!("{CMD} refresh_teams: state changed during refresh, discarding result");
            false
        }
    };
    if committed {
        token_io::persist_tokens(state.inner(), &app)?;
        log::info!("{CMD} refresh_teams: SUCCESS (state updated and persisted)");
    } else {
        log::info!("{CMD} refresh_teams: NOOP (concurrent state change; not persisted)");
    }

    Ok(())
}

/// Decodes the `scp` claim from the stored Teams access token's JWT payload
/// (empty when undecodable or no token). Powers the Settings one-time
/// reconnect banner when `Presence.Read` or `profile` is missing — those
/// scopes are needed by the presence gate / availability sync (issue
/// #3.0-P1/P2). Mirrors `get_spotify_granted_scopes`.
#[tauri::command]
pub fn get_teams_granted_scopes(state: tauri::State<'_, Arc<AppState>>) -> Vec<String> {
    log::debug!("{CMD} get_teams_granted_scopes: ENTRY");
    match state.tokens.teams().as_ref() {
        Some(tokens) => decode_teams_granted_scopes(&tokens.access_token),
        None => Vec::new(),
    }
}