//! Microsoft Teams (device-code) authentication Tauri commands.
//!
//! See issue #76. Teams uses an OAuth 2.0 device-code flow rather than the
//! PKCE/redirect flow that Spotify uses.

use crate::polling::{cas_refresh_teams, CasOutcome};
use crate::teams::{decode_teams_granted_scopes, DeviceCodeResponse, TeamsApiError};
use crate::token_io;
use crate::AppState;
use parking_lot::Mutex;
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

/// Device code of the sign-in flow the UI is currently running (issue #933).
///
/// [`start_teams_auth_device_code`] makes its code current, and a poll may
/// only commit while its own code is still the current one. The device code is
/// the natural key: it is exactly what the frontend threads through
/// [`poll_teams_auth`], so a newer sign-in supersedes an older attempt with no
/// new wire field. Supersession is not cosmetic — the token slot and
/// `cas_refresh_or_discard` are shared, so a stale poll that lands overwrites
/// the newer flow's tokens, persists them, invalidates the onboarding cache,
/// and navigates the user to Settings while the flow they are actually running
/// is still on screen.
static CURRENT_FLOW: Mutex<Option<String>> = Mutex::new(None);

/// Whether a poll for `device_code` may still commit the tokens it polled
/// (issue #933). `None` — cancelled, or never started — discards them.
fn may_commit(current: &Mutex<Option<String>>, device_code: &str) -> bool {
    current.lock().as_deref() == Some(device_code)
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

    // Issue #933: this is now the flow the UI is running, which supersedes any
    // older attempt — its poll may no longer commit.
    *CURRENT_FLOW.lock() = Some(response.device_code.clone());
    log::info!(
        "{CMD} start_teams_auth_device_code: flow is current (device_code.len={})",
        response.device_code.len()
    );

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

    // Issue #933: the poll runs for up to 900 s. If the user started a newer
    // sign-in — or abandoned this one — meanwhile, this attempt must not land:
    // committing would overwrite the newer flow's tokens, persist them and
    // navigate away from the code the user is actually looking at. Both arms
    // are discarded, so a superseded attempt cannot report its failure onto
    // the newer flow's screen either.
    if !may_commit(&CURRENT_FLOW, &device_code) {
        log::warn!(
            "{CMD} poll_teams_auth: flow superseded or cancelled; discarding the polled result without committing"
        );
        return Ok(());
    }

    match poll_result {
        Ok(tokens) => {
            log::info!(
                "{CMD} poll_teams_auth: poll successful - access_token.len={}",
                tokens.access_token.len()
            );

            {
                state
                    .tokens_load
                    .commit_teams(&state.tokens, tokens);
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

/// Supersede the running device-code poll (issue #933).
///
/// The frontend calls this from `resetTeamsAuthFlow()` with the code it is
/// abandoning. Only a slot still holding *that* code is cleared: the reset
/// fires this fire-and-forget while the restart path immediately fetches a new
/// device code, so a cancel that lands late must not clear the newer flow's
/// registration — its own successful poll would then discard itself. The
/// abandoned poll keeps its HTTP attempt (the retry loop lives in
/// `teams::poll_teams_auth`), but its result can no longer commit, persist or
/// navigate, and the same reset releases the frontend's poll mutex, so a
/// restarted sign-in is not skipped. No main-window guard: a detached Settings
/// window may abandon the flow it handed back to the main window.
#[tauri::command]
pub fn cancel_teams_auth_poll(device_code: String) {
    let cancelled = cancel_flow(&CURRENT_FLOW, &device_code);
    log::info!("{CMD} cancel_teams_auth_poll: ENTRY - cancelled={cancelled}");
}

/// Testable core of [`cancel_teams_auth_poll`]: clear the slot only while it
/// still holds `device_code`, so a late cancel cannot revoke a newer flow.
fn cancel_flow(current: &Mutex<Option<String>>, device_code: &str) -> bool {
    let mut slot = current.lock();
    let is_current = slot.as_deref() == Some(device_code);
    if is_current {
        *slot = None;
    }
    is_current
}

#[tauri::command]
pub async fn refresh_teams(
    window: tauri::Window,
    state: tauri::State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<(), String> {
    // Issue #241: token refresh touches persisted tokens; the polling loop
    // uses `teams::refresh_teams_token` directly and the frontend never
    // invokes this from a detached window.
    super::require_main_window(&window)?;

    // Issue #928: the refresh is a blocking HTTPS round-trip and the commit
    // rewrites tokens.json — neither may run inline on the IPC thread.
    let state = Arc::clone(state.inner());
    offload_blocking("refresh_teams", move || refresh_teams_impl(&state, &app)).await?
}

/// Blocking body of [`refresh_teams`].
fn refresh_teams_impl(state: &Arc<AppState>, app: &AppHandle) -> Result<(), String> {
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
    let outcome = cas_refresh_teams(state, "teams-command", &pre_refresh_access_token, || {
        crate::teams::refresh_teams_token(&current_tokens)
    });
    match outcome {
        // Issue #180: the write guard reborrowed into the CAS call above dies
        // at the end of that statement, so persisting here cannot re-lock the
        // same RwLock for reading.
        CasOutcome::Committed(new_tokens) => {
            token_io::persist_tokens(state, app)?;
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
        CasOutcome::RefreshFailed {
            error: TeamsApiError::InvalidGrant,
            replaced: false,
        } => {
            log::error!(
                "{CMD} refresh_teams: Teams refresh token is dead (invalid_grant); discarding tokens and requiring re-auth"
            );
            // `clear_teams` marks the tombstone before setting None.
            state
                .tokens_load
                .clear_teams_if_current(&state.tokens, &pre_refresh_access_token);
            // The gate releases its slot guard before persistence retries.
            if let Err(e) = token_io::persist_tokens(state, app) {
                log::warn!(
                    "{CMD} refresh_teams: failed to persist cleared teams tokens: {}",
                    e
                );
            }
            Err(TEAMS_REAUTH_MSG.to_string())
        }
        // Issue #798: the failed refresh never matched the slot — a newer
        // session was installed mid-flight, so it is alive and this refresh
        // is a no-op (mirrors the Discarded-Some arm above).
        CasOutcome::RefreshFailed { replaced: true, .. } => {
            log::info!("{CMD} refresh_teams: NOOP (slot replaced mid-refresh; not persisted)");
            Ok(())
        }
        CasOutcome::RefreshFailed { error: e, .. } => {
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
    use super::{cancel_flow, may_commit, offload_blocking};
    use parking_lot::Mutex;

    /// Issue #878: the point of the offload is that the thread which *awaits*
    /// the request is not the thread which *runs* it. `block_on` parks the
    /// calling thread, so work that ran inline would report the caller's own
    /// thread id — exactly the freeze the issue describes.
    #[test]
    fn offloaded_work_runs_off_the_awaiting_thread() {
        let awaiting = std::thread::current().id();
        let worker = tauri::async_runtime::block_on(offload_blocking("test", std::thread::current))
            .expect("the offloaded work must not panic")
            .id();
        assert_ne!(
            worker, awaiting,
            "the device-code request must not run on the thread that awaits it: \
             that thread is the IPC thread, and the HTTPS round-trip would \
             freeze the window until the request answered"
        );
    }

    /// The command's body, sliced by the shared literal-aware scanner (so a
    /// brace inside a log string cannot end the slice early) and with the
    /// test module cut off: this module contains the very patterns the
    /// assertions look for, so a whole-file grep would pass vacuously.
    fn device_code_command_source() -> String {
        let src = include_str!("teams_auth.rs");
        let production = &src[..src.find("\n#[cfg(test)]").expect("a test module")];
        assert!(
            production.contains("pub async fn start_teams_auth_device_code("),
            "the command must be an async `#[tauri::command]`: a synchronous body \
             runs inline on the IPC thread while it performs the POST"
        );
        crate::token_io::test_scan::fn_body(production, "fn start_teams_auth_device_code(")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
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

    /// Issue #933: a poll may only commit while its own device code is still
    /// the flow the UI is running. Pre-fix the spawned task committed whatever
    /// the code yielded, so an abandoned or superseded attempt still landed —
    /// including a sign-in as a different Microsoft account, which overwrote
    /// the newer flow's tokens and navigated the user away from its code.
    #[test]
    fn superseded_or_cancelled_flows_may_not_commit() {
        let current = Mutex::new(None);

        // Nothing registered: a poll whose start never ran has nothing to land on.
        assert!(!may_commit(&current, "code-a"));

        *current.lock() = Some("code-a".to_string());
        assert!(
            may_commit(&current, "code-a"),
            "the flow the UI started may commit its tokens"
        );

        // A newer sign-in supersedes it: the older poll is discarded, the
        // newer one may still commit.
        *current.lock() = Some("code-b".to_string());
        assert!(!may_commit(&current, "code-a"));
        assert!(may_commit(&current, "code-b"));

        // Cancelling is identity-scoped. The slot now holds the newer flow, so
        // the late cancel the restarted sign-in's reset fired for the code it
        // abandoned is a no-op: it must not revoke the flow that replaced it.
        assert!(
            !cancel_flow(&current, "code-a"),
            "a cancel for an abandoned code must not clear the newer flow"
        );
        assert!(
            may_commit(&current, "code-b"),
            "the newer flow must still be able to commit"
        );
        assert!(cancel_flow(&current, "code-b"));
        assert!(
            !may_commit(&current, "code-b"),
            "a cancelled flow may not commit"
        );
        assert!(
            !cancel_flow(&current, "code-b"),
            "cancelling an already-cleared slot is a no-op"
        );
    }

    /// The gate has to sit before the recovery-aware commit wrapper. A check
    /// placed after `commit_teams` would already have overwritten the newer
    /// flow's tokens. Structural because reaching those lines needs a live
    /// `AppHandle` and the blocking poll loop.
    #[test]
    fn the_commit_gate_precedes_the_token_slot_write() {
        let body = crate::token_io::test_scan::fn_body(
            include_str!("teams_auth.rs"),
            "fn poll_teams_auth(",
        );
        let gate = body
            .find("may_commit(")
            .expect("poll_teams_auth must gate its commit on the flow being current");
        let commit = body
            .find(".commit_teams(")
            .expect("poll_teams_auth must commit the tokens it polled");
        assert!(
            gate < commit,
            "the supersession gate must run before the token slot is written, or \
             the stale commit has already clobbered the newer flow's tokens"
        );
    }
}
