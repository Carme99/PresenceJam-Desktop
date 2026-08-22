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
pub fn start_teams_auth_device_code(app: AppHandle) -> Result<DeviceCodeResponse, String> {
    log::debug!("{CMD} start_teams_auth_device_code: ENTRY");

    let response = match crate::teams::start_teams_auth_device_code() {
        Ok(r) => r,
        Err(e) => {
            log::error!("{CMD} start_teams_auth_device_code: failed: {}", e);
            let _ = app.emit("teams-auth-failed", e.clone());
            return Err(e);
        }
    };
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
pub async fn poll_teams_auth(
    device_code: String,
    interval: u64,
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<TeamsTokens, String> {
    // Security: server interval is untrusted (devtools can inject u64::MAX).
    // Clamp before any use so spawn_blocking cannot sleep for hours.
    let interval = interval.clamp(1, 15);
    log::info!(
        "{CMD} poll_teams_auth: ENTRY - device_code.len={}, interval={}",
        device_code.len(),
        interval
    );

    let device_code_for_thread = device_code.clone();
    let poll_result = tauri::async_runtime::spawn_blocking(move || {
        crate::teams::poll_teams_auth(&device_code_for_thread, interval)
    })
    .await
    .map_err(|e| format!("poll_teams_auth task panicked: {}", e))?;

    match poll_result {
        Ok(tokens) => {
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

            // C2 deep-link single-instance UX (docs/scope-3.3.md): the
            // user finished the device-code flow in a browser, so land them
            // back on Settings. Per the Microsoft Entra device authorization
            // grant, this point means the polled token endpoint returned
            // access tokens —
            // https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-device-code.
            // +page.svelte ignores 'navigate' while Onboarding owns the view.
            log::info!("{CMD} poll_teams_auth: EMIT navigate -> settings");
            let _ = app.emit("navigate", "settings");

            log::info!("{CMD} poll_teams_auth: SUCCESS");
            Ok(tokens)
        }
        Err(err_string) => {
            log::error!("{CMD} poll_teams_auth: poll failed: {}", err_string);
            let _ = app.emit("teams-auth-failed", err_string.clone());
            Err(err_string)
        }
    }
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
