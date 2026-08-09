//! Microsoft Teams (device-code) authentication Tauri commands.
//!
//! See issue #76. Teams uses an OAuth 2.0 device-code flow rather than the
//! PKCE/redirect flow that Spotify uses.

use crate::teams::{DeviceCodeResponse, TeamsTokens};
use crate::token_io;
use crate::AppState;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

/// Log tag prefix for this submodule (issue #79 item 3).
const CMD: &str = "[CMD.TEAMS_AUTH]";

#[tauri::command]
pub fn start_teams_auth_device_code(
    _app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<DeviceCodeResponse, String> {
    log::debug!("{CMD} start_teams_auth_device_code: ENTRY");

    let response = crate::teams::start_teams_auth_device_code()?;
    log::info!("{CMD} start_teams_auth_device_code: got device code response");
    log::info!(
        "{CMD} start_teams_auth_device_code: user_code={}, verification_url={}",
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
        let mut pending = state.pending.teams_mut();
        *pending = Some(crate::PendingTeamsAuth {
            verifier: response.device_code.clone(),
            client_id: crate::teams::MICROSOFT_GRAPH_CLIENT_ID.to_string(),
            redirect_uri: "presencejam://callback".to_string(),
            expires_at,
        });
    }
    log::info!("{CMD} start_teams_auth_device_code: pending_teams_auth populated in AppState (in-memory only)");

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

    // Clear pending Teams auth in AppState (no disk side — we never wrote it)
    {
        let mut pending = state.pending.teams_mut();
        *pending = None;
    }

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

    let new_tokens = crate::teams::refresh_teams_token(&current_tokens)?;
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