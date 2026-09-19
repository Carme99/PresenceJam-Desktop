//! Microsoft Teams (device-code) authentication Tauri commands.
//!
//! See issue #76. Teams uses an OAuth 2.0 device-code flow rather than the
//! PKCE/redirect flow that Spotify uses.

use crate::polling::{cas_refresh_or_discard, CasOutcome};
use crate::teams::{decode_teams_granted_scopes, DeviceCodeResponse, TeamsApiError};
use crate::token_io;
use crate::AppState;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

/// Log tag prefix for this submodule (issue #79 item 3).
const CMD: &str = "[CMD.TEAMS_AUTH]";

/// Run one blocking round-trip on the async runtime's blocking pool.
///
/// Issue #878: Tauri executes a plain `#[tauri::command]` body inline on the
/// IPC thread, so a synchronous HTTPS request freezes the webview for its
/// whole duration — normally a few hundred ms, up to the client timeout
/// behind a captive portal, a dead network or a blackholed DNS. A join
/// failure is mapped to the same `String` error the flow reports everywhere
/// else.
async fn offload_blocking<T, F>(label: &'static str, work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| format!("{CMD} {label} task panicked: {e}"))
}

#[tauri::command]
pub async fn start_teams_auth_device_code(
    window: tauri::Window,
    app: AppHandle,
) -> Result<DeviceCodeResponse, String> {
    // Issue #241: detached windows never legitimately start the device-code
    // flow (Onboarding/Reconnect/+layout are main-window-only; detached
    // Settings goes through `reconnect_teams` + `poll_teams_auth`).
    super::require_main_window(&window)?;
    log::debug!("{CMD} start_teams_auth_device_code: ENTRY");

    // Issue #878: fetching the device code is a blocking HTTPS POST plus a
    // JSON parse; a synchronous command body would run it on the IPC thread
    // and freeze the window before the code and verification URL appear.
    let request = crate::teams::start_teams_auth_device_code;
    let response = match offload_blocking("start_teams_auth_device_code", request).await {
        // `offload_blocking` maps a join failure to the same `String`, so the
        // request's own `Result` is the only one left to classify here.
        Ok(Ok(r)) => r,
        Ok(Err(e)) | Err(e) => {
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
) -> Result<(), String> {
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
                *guard = Some(tokens);
                log::info!("{CMD} poll_teams_auth: tokens stored in AppState");
            }
            // Issue #562: the sign-in already succeeded — the token endpoint
            // returned tokens and they are live in AppState. A persist failure
            // (keychain, full/read-only disk) must NOT be `?`-propagated: the
            // frontend treats the poll's Err as a failed sign-in, and because
            // an Entra device code is single-use the user would have to fetch
            // a brand-new code even though sync works until restart. Keep the
            // in-memory commit and surface the persistence gap on its own
            // event, mirroring the polling loop's policy (poll_once.rs).
            match token_io::persist_tokens(state.inner(), &app) {
                Ok(()) => log::info!("{CMD} poll_teams_auth: tokens persisted atomically"),
                Err(e) => {
                    log::warn!(
                        "{CMD} poll_teams_auth: sign-in succeeded but tokens could not be persisted (session is live until restart): {}",
                        e
                    );
                    let _ = app.emit("teams-auth-persist-warning", e);
                }
            }

            // Issue #70: invalidate the onboarding cache.
            state.onboarding_cache.invalidate();
            log::info!("{CMD} poll_teams_auth: onboarding_cache invalidated");

            log::info!("{CMD} poll_teams_auth: EMIT teams-auth-complete event");
            let _ = app.emit("teams-auth-complete", ());

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
            Ok(())
        }
        Err(err_string) => {
            log::error!("{CMD} poll_teams_auth: poll failed: {}", err_string);
            let _ = app.emit("teams-auth-failed", err_string.clone());
            Err(err_string)
        }
    }
}

#[tauri::command]
pub fn refresh_teams(
    window: tauri::Window,
    state: tauri::State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<(), String> {
    // Issue #241: token refresh touches persisted tokens; the polling loop
    // uses `teams::refresh_teams_token` directly and the frontend never
    // invokes this from a detached window.
    super::require_main_window(&window)?;
    log::debug!("{CMD} refresh_teams: ENTRY");

    let current_tokens = {
        let guard = state.tokens.teams();
        guard.clone().ok_or_else(|| {
            log::error!("{CMD} refresh_teams: No Teams tokens in state");
            "No Teams tokens to refresh".to_string()
        })?
    };
    log::info!("{CMD} refresh_teams: current tokens found");

    // The refresh error stays typed (`CasOutcome<T, E>` is generic over
    // `E`) so the re-auth policy can classify it instead of string-sniffing
    // every provider error into the same IPC message (issue #564). Pre-fix
    // this hand-rolled the CAS and stringified `InvalidGrant`, so a dead
    // refresh token stayed in AppState and in tokens.json while the UI saw a
    // generic failure.
    let pre_refresh_access_token = current_tokens.access_token.clone();
    let outcome = cas_refresh_or_discard(
        "teams-command",
        &mut *state.tokens.teams_mut(),
        &pre_refresh_access_token,
        || crate::teams::refresh_teams_token(&current_tokens),
        |t| &t.access_token,
    );
    match outcome {
        // Issue #180: the write guard reborrowed into the CAS call above dies
        // at the end of that statement, so persisting here cannot re-lock the
        // same RwLock for reading.
        CasOutcome::Committed(new_tokens) => {
            token_io::persist_tokens(state.inner(), &app)?;
            log::info!(
                "{CMD} refresh_teams: SUCCESS (state updated and persisted, access_token.len={})",
                new_tokens.access_token.len()
            );
            Ok(())
        }
        // Somebody else replaced the token we refreshed from: whatever is in
        // the slot now is newer, so this refresh is a no-op.
        CasOutcome::Discarded { current } if current.is_some() => {
            log::info!("{CMD} refresh_teams: NOOP (concurrent state change; not persisted)");
            Ok(())
        }
        CasOutcome::Discarded { .. } => {
            log::error!(
                "{CMD} refresh_teams: state was cleared during the refresh; re-auth required"
            );
            Err(TEAMS_REAUTH_MSG.to_string())
        }
        CasOutcome::RefreshFailed(TeamsApiError::InvalidGrant) => {
            log::error!(
                "{CMD} refresh_teams: Teams refresh token is dead (invalid_grant); discarding tokens and requiring re-auth"
            );
            *state.tokens.teams_mut() = None;
            // Issue #180: the clearing statement above drops its guard at the
            // end of that statement, so this persist cannot self-deadlock.
            if let Err(e) = token_io::persist_tokens(state.inner(), &app) {
                log::warn!(
                    "{CMD} refresh_teams: failed to persist cleared teams tokens: {}",
                    e
                );
            }
            Err(TEAMS_REAUTH_MSG.to_string())
        }
        CasOutcome::RefreshFailed(e) => {
            log::warn!("{CMD} refresh_teams: refresh failed (session kept): {}", e);
            Err(e.to_string())
        }
    }
}

/// IPC error text for a session the user has to re-authorize. The frontend
/// renders the error string verbatim, so it has to name the action rather
/// than a provider error code (issue #564).
const TEAMS_REAUTH_MSG: &str =
    "Your Microsoft Teams session has expired. Sign in again from Settings.";

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

#[cfg(test)]
mod tests {
    use super::offload_blocking;

    /// Issue #878: the point of the offload is that the thread which *awaits*
    /// the request is not the thread which *runs* it. `block_on` parks the
    /// calling thread, so work that ran inline would report the caller's own
    /// thread id — exactly the freeze the issue describes.
    #[test]
    fn offloaded_work_runs_off_the_awaiting_thread() {
        let awaiting = std::thread::current().id();
        let worker = tauri::async_runtime::block_on(offload_blocking("test", std::thread::current))
            .expect("the offloaded work must not panic");
        assert_ne!(
            worker, awaiting,
            "the device-code request must not run on the thread that awaits it: \
             that thread is the IPC thread, and the HTTPS round-trip would \
             freeze the window until the request answered"
        );
    }

    /// Normalised source of the `start_teams_auth_device_code` command: from
    /// its (async) signature to the brace that closes its body. The
    /// assertions below cannot read the whole file, because this test module
    /// contains the very patterns they look for and a whole-file grep would
    /// pass vacuously. Panics when the command is not async — that is the
    /// pre-#878 shape.
    fn device_code_command_source() -> String {
        let src = include_str!("teams_auth.rs");
        let start = src
            .find("pub async fn start_teams_auth_device_code(")
            .expect("the device-code command must be an async `#[tauri::command]`");
        let open = start + src[start..].find('{').expect("a command body opener");
        let mut depth = 0usize;
        for (offset, ch) in src[open..].char_indices() {
            if ch == '{' {
                depth += 1;
            } else if ch == '}' {
                depth -= 1;
                if depth == 0 {
                    let body = &src[start..=open + offset];
                    return body.split_whitespace().collect::<Vec<_>>().join(" ");
                }
            }
        }
        panic!("the device-code command body is unterminated");
    }

    /// A synchronous call left in the command body is the pre-#878
    /// regression: it runs the HTTPS round-trip on the IPC thread.
    #[test]
    fn device_code_command_offloads_its_request() {
        let body = device_code_command_source();
        assert!(
            body.contains("let request = crate::teams::start_teams_auth_device_code;"),
            "the command must take the blocking request as its offload payload"
        );
        assert!(
            body.contains(
                "match offload_blocking(\"start_teams_auth_device_code\", request).await"
            ),
            "the request must be awaited through `offload_blocking`, or the window \
             freezes for the round trip"
        );
    }
}
