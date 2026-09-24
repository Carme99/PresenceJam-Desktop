//! Onboarding Tauri commands: completion check, transition, and disconnect/reconnect.
//!
//! See issue #76. Owns `is_onboarding_complete_impl` (the spawn_blocking body
//! for the async cache-first check) and the `ONBOARDING_CACHE_TTL` constant.

use crate::config;
use crate::keychain::{self, KeychainPresence};
use crate::polling::{cas_refresh_spotify, cas_refresh_teams, CasOutcome};
use crate::token_io;
use crate::AppState;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// TTL for the `is_onboarding_complete` result cache. The front-end remounts this
/// command on every Onboarding view enter, and the check's refresh leaves can
/// take up to 20s in the worst case (HTTPS round-trips to Spotify/Graph), so a
/// short cache is needed to avoid hammering the upstream APIs.
const ONBOARDING_CACHE_TTL: Duration = Duration::from_secs(30);

/// Log tag prefix for this submodule (issue #79 item 3).
const CMD: &str = "[CMD.ONBOARDING]";

/// Onboarding check: `true` if both Spotify and Teams are configured and hold a
/// usable session.
///
/// Issue #530: a locally-expired access token is NOT a dead session. This gate
/// spends the refresh token (and persists the result) before deciding, exactly
/// like the polling loop does; only `invalid_grant` or missing credentials ask
/// the user to sign in again. Pre-fix it probed the upstream APIs with the stale
/// bearer, took the unavoidable 401, and sent every returning user — whose app
/// had been closed longer than the ~1 h access-token lifetime — back into the
/// setup wizard. Transient failures (no network, 429, 5xx) still count as
/// "valid" so a flaky network never bounces the user into onboarding.
///
/// Result is cached on `AppState.onboarding_cache` for [`ONBOARDING_CACHE_TTL`] —
/// the front-end remounts this command on every Onboarding view enter, and the
/// refresh leaves of the check make upstream calls.
#[tauri::command]
pub async fn is_onboarding_complete(
    state: tauri::State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<bool, String> {
    log::debug!("{CMD} is_onboarding_complete: ENTRY");

    // Cache hit — return immediately.
    if let Some(result) = cached_verdict(&state, "cache HIT") {
        return Ok(result);
    }

    // Cache miss — run the actual check on a blocking thread (HTTPS
    // round-trips). Overlapping callers share one run (issue #942): the front
    // end gives up on the boot probe after 8 s and offers Retry, so two gate
    // runs used to refresh from clones of the same refresh token.
    let state_clone: Arc<AppState> = Arc::clone(&state);
    let app_clone = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        single_flight(
            &BOOT_GATE_FLIGHT,
            // A waiter shares only a verdict that landed: the cache holds
            // `bool`s and a failed check is not cacheable, so the error type
            // rides along in `T` for the owner's own `Result` (issue #942).
            || {
                cached_verdict(&state_clone, "in-flight check already landed")
                    .map(Ok::<bool, String>)
            },
            || {
                let result = is_onboarding_complete_impl(&state_clone, &app_clone)?;
                // Store result in cache. We cache both `true` and `false`
                // outcomes — a recent "complete" result is just as valid as a
                // recent "incomplete" one for the 30s window. The write happens
                // inside the flight lock, so a caller that waited for this run
                // observes this verdict instead of starting another one.
                *state_clone.onboarding_cache.lock() = Some((Instant::now(), result));
                log::info!(
                    "{CMD} is_onboarding_complete: cache MISS, stored fresh result={result}"
                );
                Ok(result)
            },
        )
    })
    .await
    .map_err(|e| format!("is_onboarding_complete task panicked: {}", e))?
}

/// Freshness-window read of the 30 s verdict cache (issues #70, #942).
/// `why` completes the log line, so a verdict shared from a check that was in
/// flight stays distinguishable from a plain cache hit.
fn cached_verdict(state: &Arc<AppState>, why: &str) -> Option<bool> {
    let guard = state.onboarding_cache.lock();
    if let Some((ts, result)) = *guard {
        if ts.elapsed() < ONBOARDING_CACHE_TTL {
            log::info!(
                "{CMD} is_onboarding_complete: {why} (age={:.2}s, result={result})",
                ts.elapsed().as_secs_f32()
            );
            return Some(result);
        }
    }
    None
}

/// Process-wide single-flight lock for the boot gate (issue #942).
///
/// The drain-style cache dedupes only *finished* checks, so two overlapping
/// callers — the boot probe and the Retry the front end offers after its 8 s
/// `BOOT_TIMEOUT_MS` — each ran the gate and refreshed from clones of the same
/// refresh token. `cas_refresh_or_discard` refreshes before it compares, so
/// against a provider that rotated the token the loser's `invalid_grant` arm
/// clears and persists a session that is alive: a full re-auth for a healthy
/// user. A plain blocking mutex (parking_lot's, which has no poisoning error
/// path to unwrap) is the right shape here — every caller reaches it on a
/// blocking thread (`spawn_blocking`), so waiting parks a pool thread instead
/// of stalling the async runtime.
static BOOT_GATE_FLIGHT: Mutex<()> = Mutex::new(());

/// Single-flight core of the boot gate: while one check runs, a second caller
/// waits for it and shares its verdict instead of spending the same refresh
/// token again. `cached` is consulted *after* the wait (double-checked): the
/// owner writes its verdict into the cache before it releases the flight lock,
/// so `run` only executes when there genuinely is no verdict to share.
fn single_flight<T>(
    flight: &Mutex<()>,
    cached: impl Fn() -> Option<T>,
    run: impl FnOnce() -> T,
) -> T {
    let _in_flight = flight.lock();
    if let Some(verdict) = cached() {
        return verdict;
    }
    run()
}

/// Boot-gate verdict for one provider's session (issue #530).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionVerdict {
    /// Usable session — never ask this user to sign in again.
    Valid,
    /// Dead or unrepairable session — re-auth required.
    ReauthRequired,
}

/// Why a refresh attempt did not yield a usable session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RefreshFailure {
    /// `invalid_grant`: the refresh token itself is gone (Spotify's 6-month
    /// lifetime, Microsoft's 90-day inactivity window, or a revocation).
    Dead,
    /// The credential pair a refresh needs is unavailable, and retrying
    /// cannot fix it: an empty `client_id`, or a Spotify `client_secret` the
    /// keychain positively reports as absent (`keyring::Error::NoEntry`).
    Unavailable,
    /// Network error, 429 or 5xx: the session may well be fine.
    Transient,
}

/// Boot-gate decision table (issue #530).
///
/// `refresh` runs ONLY for a locally-expired access token, so a fresh session
/// never pays for a network round-trip. Spending the refresh token instead of
/// probing an API with a stale bearer is what keeps a returning user out of the
/// setup wizard; a transient refresh failure keeps the pre-existing "a flaky
/// network is not a dead session" policy.
fn session_verdict(
    expired: bool,
    refresh: impl FnOnce() -> Result<(), RefreshFailure>,
) -> SessionVerdict {
    if !expired {
        return SessionVerdict::Valid;
    }
    match refresh() {
        Ok(()) | Err(RefreshFailure::Transient) => SessionVerdict::Valid,
        Err(RefreshFailure::Dead) | Err(RefreshFailure::Unavailable) => {
            SessionVerdict::ReauthRequired
        }
    }
}

/// Persist a refreshed session. Failure is logged, not propagated: the gate must
/// still answer, and the polling loop re-persists on its next refresh.
fn persist_refreshed(state: &Arc<AppState>, app: &AppHandle, label: &str) {
    match token_io::persist_tokens(state, app) {
        Ok(()) => log::info!(
            "{CMD} is_onboarding_complete: {label} session refreshed (access token was expired at launch)"
        ),
        Err(e) => log::warn!(
            "{CMD} is_onboarding_complete: refreshed {label} tokens could not be persisted: {e}"
        ),
    }
}

/// Drop a dead session from `AppState`, persist the cleared file, and make the
/// re-auth reason loud — the same policy the polling loop applies to a dead
/// refresh token (#160/#219). `clear` takes the write guard, so the guard is
/// dropped before `persist_tokens` re-locks the slot for reading (issue #180).
fn discard_dead_session(
    label: &str,
    provider: crate::TokenProvider,
    pre_refresh_access_token: &str,
    state: &Arc<AppState>,
    app: &AppHandle,
) {
    let cleared = match provider {
        crate::TokenProvider::Spotify => state
            .tokens_load
            .clear_spotify_if_current(&state.tokens, pre_refresh_access_token),
        crate::TokenProvider::Teams => state
            .tokens_load
            .clear_teams_if_current(&state.tokens, pre_refresh_access_token),
    };
    if !cleared {
        return;
    }
    if let Err(e) = token_io::persist_tokens(state, app) {
        log::warn!("{CMD} is_onboarding_complete: failed to persist cleared {label} tokens: {e}");
    }
    log::error!(
        "{CMD} is_onboarding_complete: {label} refresh token is dead (invalid_grant); re-auth required"
    );
}

/// Boot-gate check for the Spotify session (issue #530).
///
/// Refreshing needs the `client_id` (config) plus the `client_secret` (OS
/// keychain); a missing pair is `Unavailable` — retrying cannot fix it, so
/// the user is routed to reconnect instead of looping.
///
/// Issue #561: a keychain that cannot answer *right now* is not a missing
/// credential. See [`boot_gate_client_secret`].
fn spotify_session_verdict(
    state: &Arc<AppState>,
    app: &AppHandle,
    config: &config::AppConfig,
    tokens: &crate::spotify::SpotifyTokens,
) -> SessionVerdict {
    use crate::spotify::{is_token_expired, refresh_spotify_token, SpotifyApiError};

    session_verdict(is_token_expired(tokens), || {
        let client_id = &config.spotify.client_id;
        if client_id.is_empty() {
            log::warn!(
                "{CMD} is_onboarding_complete: Spotify access token expired but no client_id is configured; re-auth required"
            );
            return Err(RefreshFailure::Unavailable);
        }
        // Issue #760: one typed read instead of a presence probe plus a fetch.
        // The two keychain operations had a TOCTOU window between them, and
        // the flattened `Result<String, String>` threw away the difference
        // between "no secret stored" and "the keychain cannot answer".
        let read = keychain::read_spotify_client_secret();
        record_client_secret_state(state, &presence_from_read(&read));
        let client_secret = boot_gate_client_secret(read)?;

        let pre_refresh_access_token = tokens.access_token.clone();
        let outcome = cas_refresh_spotify(
            state,
            "spotify-onboarding",
            &pre_refresh_access_token,
            || refresh_spotify_token(tokens, client_id, &client_secret),
        );
        match outcome {
            // The guard above is a temporary that died at the end of the
            // statement, so persisting here cannot re-lock the held slot (#180).
            CasOutcome::Committed(_) => {
                persist_refreshed(state, app, "Spotify");
                Ok(())
            }
            // Somebody else replaced the token we refreshed from: whatever is
            // in the slot now is newer, so the session is alive.
            CasOutcome::Discarded { current } if current.is_some() => Ok(()),
            CasOutcome::Discarded { .. } => Err(RefreshFailure::Dead),
            CasOutcome::RefreshFailed {
                error: SpotifyApiError::InvalidGrant,
                replaced: false,
            } => {
                discard_dead_session(
                    "Spotify",
                    crate::TokenProvider::Spotify,
                    &pre_refresh_access_token,
                    state,
                    app,
                );
                Err(RefreshFailure::Dead)
            }
            // Issue #798: the error is about a superseded token — the newer
            // session in the slot is alive.
            CasOutcome::RefreshFailed { replaced: true, .. } => Ok(()),
            CasOutcome::RefreshFailed { .. } => Err(RefreshFailure::Transient),
        }
    })
}

/// Mirror a keychain observation into the in-memory config (issue #560).
///
/// [`config::with_keychain_flags`] stamps the derived `client_secret_state`
/// on every config *load*, but the boot gate probes the keychain on its own
/// path and can learn something the loaded copy does not know yet (a keyring
/// that locked between launch and the verdict, or a credential deleted from
/// the OS UI). Without this, a config write later in the session
/// (`update_config` bases its merge on the in-memory copy) would return the
/// stale state to the UI, which is exactly where "the keychain is locked"
/// silently degrades back into "not configured".
fn record_client_secret_state(state: &Arc<AppState>, presence: &KeychainPresence) {
    let mut guard = state.config.get_mut();
    let Some(config) = guard.as_mut() else {
        // The frontend has not loaded a config yet: there is nothing to
        // correct, and the next load stamps the state itself.
        return;
    };
    config.spotify.client_secret_state = config::ClientSecretState::from(presence);
    config.spotify.client_secret_set = matches!(presence, KeychainPresence::Present);
}

/// Project one typed keychain read onto the tri-state the config surface
/// renders (issue #560), so the boot gate's observation still reaches the
/// in-memory config from the single read (issue #760).
///
/// A corrupt entry is reported as unavailable rather than absent: the item is
/// in the keychain, so "nothing is stored" — the answer that sends the user
/// through re-onboarding — would be wrong.
fn presence_from_read(read: &Result<String, keychain::KeychainReadError>) -> KeychainPresence {
    match read {
        Ok(_) => KeychainPresence::Present,
        Err(keychain::KeychainReadError::Absent) => KeychainPresence::Absent,
        Err(keychain::KeychainReadError::Unavailable(help)) => {
            KeychainPresence::Unavailable(help.clone())
        }
        Err(keychain::KeychainReadError::Corrupt(detail)) => {
            KeychainPresence::Unavailable(detail.clone())
        }
    }
}

/// Resolve the Spotify `client_secret` for the boot gate from one typed
/// keychain read (issue #760).
///
/// Issue #561: the keychain is a *tri-state* — present, absent, or unavailable
/// (no Secret Service daemon, a locked keyring, denied storage access). Only a
/// positively absent entry justifies sending the user to reconnect; an
/// unavailable keychain is transient by construction (the secret is still
/// there) and must keep the session, exactly like a flaky network does.
/// Pre-#561 this collapsed every keychain error into an empty string via
/// `unwrap_or_default()`, which the gate read as "not configured" — so a
/// locked keyring at launch bounced a fully credentialed user into the setup
/// wizard, contradicting this module's own transient-failure policy.
///
/// Issue #760: the caller hands over the result of the one read, so the
/// classification needs no second keychain probe and cannot disagree with the
/// read it is classifying.
fn boot_gate_client_secret(
    read: Result<String, keychain::KeychainReadError>,
) -> Result<String, RefreshFailure> {
    match read {
        Ok(secret) if !secret.is_empty() => Ok(secret),
        Ok(_) => {
            log::warn!(
                "{CMD} is_onboarding_complete: keychain holds an empty Spotify client_secret; re-auth required"
            );
            Err(RefreshFailure::Unavailable)
        }
        Err(keychain::KeychainReadError::Absent) => {
            log::warn!(
                "{CMD} is_onboarding_complete: Spotify access token expired but no client_secret is stored; re-auth required"
            );
            Err(RefreshFailure::Unavailable)
        }
        Err(keychain::KeychainReadError::Unavailable(help)) => {
            log::warn!(
                "{CMD} is_onboarding_complete: OS keychain unavailable, keeping the session and retrying later: {help}"
            );
            Err(RefreshFailure::Transient)
        }
        Err(keychain::KeychainReadError::Corrupt(detail)) => {
            // The client-secret read does not produce this today; it is
            // transient so a future corrupt item can never send a fully
            // credentialed user back through onboarding.
            log::warn!(
                "{CMD} is_onboarding_complete: stored Spotify client_secret is unreadable, keeping the session and retrying later: {detail}"
            );
            Err(RefreshFailure::Transient)
        }
    }
}

/// Boot-gate check for the Teams session (issue #530). Mirrors the Spotify
/// version; the device-code flow needs no client credentials to refresh.
fn teams_session_verdict(
    state: &Arc<AppState>,
    app: &AppHandle,
    tokens: &crate::teams::TeamsTokens,
) -> SessionVerdict {
    use crate::teams::{is_token_expired, refresh_teams_token, TeamsApiError};

    session_verdict(is_token_expired(tokens), || {
        let pre_refresh_access_token = tokens.access_token.clone();
        let outcome =
            cas_refresh_teams(state, "teams-onboarding", &pre_refresh_access_token, || {
                refresh_teams_token(tokens)
            });
        match outcome {
            CasOutcome::Committed(_) => {
                persist_refreshed(state, app, "Teams");
                Ok(())
            }
            CasOutcome::Discarded { current } if current.is_some() => Ok(()),
            CasOutcome::Discarded { .. } => Err(RefreshFailure::Dead),
            CasOutcome::RefreshFailed {
                error: TeamsApiError::InvalidGrant,
                replaced: false,
            } => {
                discard_dead_session(
                    "Teams",
                    crate::TokenProvider::Teams,
                    &pre_refresh_access_token,
                    state,
                    app,
                );
                Err(RefreshFailure::Dead)
            }
            // Issue #798: the error is about a superseded token — the newer
            // session in the slot is alive.
            CasOutcome::RefreshFailed { replaced: true, .. } => Ok(()),
            CasOutcome::RefreshFailed { .. } => Err(RefreshFailure::Transient),
        }
    })
}

/// Blocking implementation of the onboarding check. Run via `spawn_blocking`
/// from `is_onboarding_complete` so the async runtime can keep serving other
/// commands while the refresh round-trips complete.
fn is_onboarding_complete_impl(state: &Arc<AppState>, app: &AppHandle) -> Result<bool, String> {
    let config = config::load_config()?;
    let spotify_configured = !config.spotify.client_id.is_empty();

    // Clone out of the token locks BEFORE any network call: a read guard held
    // across a 10 s HTTPS round-trip would block the polling thread's write to
    // the same slot for that whole window.
    let spotify_tokens = state.tokens.spotify().clone();
    let teams_tokens = state.tokens.teams().clone();

    let spotify_valid = spotify_tokens.as_ref().is_some_and(|tokens| {
        spotify_session_verdict(state, app, &config, tokens) == SessionVerdict::Valid
    });
    let teams_configured = teams_tokens.is_some();
    let teams_valid = teams_tokens
        .as_ref()
        .is_some_and(|tokens| teams_session_verdict(state, app, tokens) == SessionVerdict::Valid);

    // Onboarding is complete only if:
    // 1. Spotify is configured AND its session is usable
    // 2. Teams is configured AND its session is usable
    let complete = spotify_configured && spotify_valid && teams_configured && teams_valid;
    log::info!(
        "{CMD} is_onboarding_complete: result={} (spotify_configured={}, spotify_valid={}, teams_configured={}, teams_valid={})",
        complete,
        spotify_configured,
        spotify_valid,
        teams_configured,
        teams_valid
    );

    Ok(complete)
}

/// Machine-readable codes `complete_onboarding` returns when a token slot is
/// empty (issue #978). The wizard maps each one to localised copy plus the
/// auth step that fixes it; both-missing yields both codes in a fixed order,
/// Spotify first, so the wizard sends the user to the first step and the next
/// Finish surfaces the other.
const SPOTIFY_NOT_CONNECTED: &str = "spotify_not_connected";
const TEAMS_NOT_CONNECTED: &str = "teams_not_connected";
const BOTH_NOT_CONNECTED: &str = "spotify_not_connected,teams_not_connected";

/// The routable code for a `complete_onboarding` finish, or `None` when both
/// providers are connected and sync may start.
fn missing_tokens_error(has_spotify: bool, has_teams: bool) -> Option<&'static str> {
    match (has_spotify, has_teams) {
        (true, true) => None,
        (false, true) => Some(SPOTIFY_NOT_CONNECTED),
        (true, false) => Some(TEAMS_NOT_CONNECTED),
        (false, false) => Some(BOTH_NOT_CONNECTED),
    }
}

#[tauri::command]
pub async fn complete_onboarding(
    window: tauri::Window,
    state: tauri::State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<(), String> {
    // Issue #241: onboarding completion starts sync + writes state; the
    // Onboarding view is main-window-only. The window is forwarded to the
    // Rust-side `start_syncing` call below so its guard sees a main label.
    super::require_main_window(&window)?;
    log::debug!("{CMD} complete_onboarding: ENTRY");

    let has_spotify = {
        let guard = state.tokens.spotify();
        guard.is_some()
    };

    let has_teams = {
        let guard = state.tokens.teams();
        guard.is_some()
    };

    log::info!(
        "{CMD} complete_onboarding: has_spotify={}, has_teams={}",
        has_spotify,
        has_teams
    );

    if let Some(code) = missing_tokens_error(has_spotify, has_teams) {
        log::error!(
            "{CMD} complete_onboarding: missing tokens, cannot start sync (spotify={}, teams={})",
            has_spotify,
            has_teams
        );
        // Issue #978: a stable code instead of the formatted "Missing tokens:
        // spotify=…, teams=…" internals sentence. The wizard routes on it —
        // localised copy plus the step that fixes it — and the booleans stay
        // in the log line above for diagnostics.
        return Err(code.to_string());
    }

    log::info!("{CMD} complete_onboarding: both tokens present, starting sync");
    super::sync::start_syncing(window, state, app).await?;
    log::info!("{CMD} complete_onboarding: sync started successfully");

    log::info!("{CMD} complete_onboarding: SUCCESS");
    Ok(())
}

#[tauri::command]
pub async fn reconnect_spotify(
    state: tauri::State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<(), String> {
    // Issue #928: the body rewrites tokens.json and clears the keychain
    // entry — blocking I/O that must not run inline on the IPC thread.
    let state = Arc::clone(state.inner());
    tauri::async_runtime::spawn_blocking(move || reconnect_spotify_impl(&state, &app))
        .await
        .map_err(|e| format!("reconnect_spotify task panicked: {e}"))?
}

/// Blocking body of [`reconnect_spotify`]: drop the session, persist the
/// cleared file, forget the keychain secret, and ask the UI to re-auth.
fn reconnect_spotify_impl(state: &Arc<AppState>, app: &AppHandle) -> Result<(), String> {
    // Clear Spotify tokens from state
    state.tokens_load.clear_spotify(&state.tokens);
    log::info!("{CMD} reconnect_spotify: cleared spotify_tokens");

    // Clear pending Spotify auth
    *state.pending.spotify_mut() = None;
    log::info!("{CMD} reconnect_spotify: cleared pending_spotify_auth");

    // Persist the cleared state to disk atomically.
    if let Err(e) = token_io::persist_tokens(state, app) {
        log::warn!(
            "{CMD} reconnect_spotify: failed to persist cleared state - {}",
            e
        );
    }

    // Issue #70: invalidate the onboarding cache so the UI sees the cleared state.
    state.onboarding_cache.invalidate();
    log::info!("{CMD} reconnect_spotify: onboarding_cache invalidated");

    // Clear the client_secret from the OS keychain (see issue #9).
    // Best-effort: don't fail the disconnect if the keychain entry is
    // already gone or unavailable.
    if let Err(e) = crate::keychain::delete_spotify_client_secret() {
        log::warn!(
            "{CMD} reconnect_spotify: failed to clear keychain entry - {}",
            e
        );
    }

    // Emit event so UI can show re-auth flow
    if let Err(e) = app.emit("spotify-reconnect-required", ()) {
        log::error!("{CMD} reconnect_spotify: failed to emit event - {}", e);
    } else {
        log::info!("{CMD} reconnect_spotify: EMIT spotify-reconnect-required event");
    }

    log::info!("{CMD} reconnect_spotify: SUCCESS");
    Ok(())
}

#[tauri::command]
pub async fn reconnect_teams(
    state: tauri::State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<(), String> {
    // Issue #928: the body rewrites tokens.json — blocking I/O that must not
    // run inline on the IPC thread.
    let state = Arc::clone(state.inner());
    tauri::async_runtime::spawn_blocking(move || reconnect_teams_impl(&state, &app))
        .await
        .map_err(|e| format!("reconnect_teams task panicked: {e}"))?
}

/// Blocking body of [`reconnect_teams`]: drop the session, persist the cleared
/// file, and ask the UI to re-auth.
fn reconnect_teams_impl(state: &Arc<AppState>, app: &AppHandle) -> Result<(), String> {
    // Clear Teams tokens from state
    state.tokens_load.clear_teams(&state.tokens);
    log::info!("{CMD} reconnect_teams: cleared teams_tokens");

    // Persist the cleared state to disk atomically.
    if let Err(e) = token_io::persist_tokens(state, app) {
        log::warn!(
            "{CMD} reconnect_teams: failed to persist cleared state - {}",
            e
        );
    }

    // Issue #70: invalidate the onboarding cache.
    state.onboarding_cache.invalidate();
    log::info!("{CMD} reconnect_teams: onboarding_cache invalidated");

    // Emit event so UI can show re-auth flow. #675: this is the ONLY
    // user-initiated emitter of `teams-reconnect-required` ("Reconnect Teams"
    // in Settings), so it is marked as such — the notification consumer toasts
    // only the genuinely-dead-session emitters in `poll_once` and stays quiet
    // for the reconnect the user just asked for. The always-mounted layout
    // opens the device-code flow here either way.
    if let Err(e) = app.emit(
        "teams-reconnect-required",
        serde_json::json!({ "user_initiated": true }),
    ) {
        log::error!("{CMD} reconnect_teams: failed to emit event - {}", e);
    } else {
        log::info!("{CMD} reconnect_teams: EMIT teams-reconnect-required event");
    }

    log::info!("{CMD} reconnect_teams: SUCCESS");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        boot_gate_client_secret, cached_verdict, missing_tokens_error, presence_from_read,
        record_client_secret_state, session_verdict, single_flight, RefreshFailure, SessionVerdict,
        ONBOARDING_CACHE_TTL,
    };
    use crate::config::{AppConfig, ClientSecretState};
    use crate::keychain::{KeychainPresence, KeychainReadError};
    use crate::AppState;
    use parking_lot::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    /// Issue #530: the boot gate must spend the refresh token for a
    /// locally-expired access token instead of reporting a dead session.
    /// Pre-fix the gate probed the API with the stale bearer, took the 401 and
    /// sent a fully credentialed returning user back to the setup wizard.
    #[test]
    fn expired_token_with_successful_refresh_is_a_live_session() {
        assert_eq!(
            session_verdict(true, || Ok(())),
            SessionVerdict::Valid,
            "a refreshable session must never be reported as needing re-auth"
        );
    }

    /// #675: `reconnect_teams` is the ONLY user-initiated emitter of
    /// `teams-reconnect-required`, and the desktop-notification consumer
    /// depends on that: it toasts the poller's dead-session emitters and stays
    /// quiet for the reconnect the user just asked for. The signal is the
    /// payload, so both halves are pinned here — the marker on this emitter,
    /// and the poller's emitters *not* carrying it. Structural because the emit
    /// needs a live `AppHandle`; whitespace is normalised so rustfmt reflowing
    /// the call cannot break the guard.
    #[test]
    fn reconnect_teams_marks_its_emit_user_initiated() {
        let mark = |src: &str| src.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            mark(include_str!("onboarding.rs")).contains(
                "app.emit( \"teams-reconnect-required\", serde_json::json!({ \"user_initiated\": true }), )"
            ),
            "the user-initiated reconnect must mark its emit, or #675 would report the \
             reconnect the user just clicked back to them as an expired session"
        );
        assert!(
            !include_str!("../polling/poll_once.rs").contains("user_initiated"),
            "the poller's `teams-reconnect-required` emitters are the dead-session ones and \
             must NOT claim to be user-initiated: if one starts doing so, a genuine expiry \
             would be silently swallowed and the user would never be told to sign in again"
        );
    }

    /// The refresh is only paid for when it is needed: a locally-fresh access
    /// token must short-circuit without touching the network.
    #[test]
    fn fresh_token_never_calls_the_refresh() {
        let calls = std::cell::Cell::new(0);
        let verdict = session_verdict(false, || {
            calls.set(calls.get() + 1);
            Ok(())
        });
        assert_eq!(verdict, SessionVerdict::Valid);
        assert_eq!(calls.get(), 0, "fresh token must not trigger a refresh");
    }

    /// `invalid_grant` is the only refresh outcome that really means "sign in
    /// again", and unavailable credentials cannot be repaired by retrying.
    #[test]
    fn dead_or_unavailable_credentials_require_reauth() {
        assert_eq!(
            session_verdict(true, || Err(RefreshFailure::Dead)),
            SessionVerdict::ReauthRequired
        );
        assert_eq!(
            session_verdict(true, || Err(RefreshFailure::Unavailable)),
            SessionVerdict::ReauthRequired
        );
    }

    /// A flaky network must not bounce the user into onboarding — the
    /// pre-existing policy that the refresh path has to preserve.
    #[test]
    fn transient_refresh_failure_keeps_the_session() {
        assert_eq!(
            session_verdict(true, || Err(RefreshFailure::Transient)),
            SessionVerdict::Valid
        );
    }

    /// The decision table must classify every provider error: a new
    /// `RefreshFailure` arm cannot silently fall through to "valid" or to
    /// "re-auth" — each is asserted above against the literal it maps to.
    #[test]
    fn every_refresh_failure_is_classified() {
        let cases = [
            (RefreshFailure::Dead, SessionVerdict::ReauthRequired),
            (RefreshFailure::Unavailable, SessionVerdict::ReauthRequired),
            (RefreshFailure::Transient, SessionVerdict::Valid),
        ];
        for (failure, expected) in cases {
            assert_eq!(
                session_verdict(true, || Err(failure)),
                expected,
                "{failure:?} must map to {expected:?}"
            );
        }
    }

    /// Issue #760: the gate classifies the *one* typed read it is handed, so
    /// the three keychain answers keep three different verdicts. A positively
    /// absent entry is the only one that can mean "re-onboard"; anything the
    /// keychain cannot answer must keep the session, because the secret is
    /// still stored and retrying is free (issue #561).
    #[test]
    fn absent_credentials_require_reauth_and_unreadable_ones_do_not() {
        let absent = boot_gate_client_secret(Err(KeychainReadError::Absent))
            .expect_err("an absent secret cannot refresh");
        assert_eq!(absent, RefreshFailure::Unavailable);

        let locked = boot_gate_client_secret(Err(KeychainReadError::Unavailable(
            "Secret Service locked".to_string(),
        )))
        .expect_err("a locked keychain cannot refresh");
        assert_eq!(
            locked,
            RefreshFailure::Transient,
            "a locked keychain is recoverable; the secret is still stored"
        );

        let corrupt = boot_gate_client_secret(Err(KeychainReadError::Corrupt(
            "undecodable ciphertext".to_string(),
        )))
        .expect_err("a corrupt entry cannot refresh");
        assert_eq!(
            corrupt,
            RefreshFailure::Transient,
            "an unreadable item is not an absent one: re-onboarding would not fix it"
        );
    }

    /// The end-to-end boot-gate consequence: an unreadable keychain must leave
    /// a fully credentialed returning user out of the setup wizard, while an
    /// actually absent secret still routes them to reconnect.
    #[test]
    fn boot_gate_keeps_the_session_when_the_keychain_is_locked() {
        let verdict_for = |read: Result<String, KeychainReadError>| {
            session_verdict(true, || boot_gate_client_secret(read).map(|_| ()))
        };

        assert_eq!(
            verdict_for(Err(KeychainReadError::Unavailable("locked".to_string()))),
            SessionVerdict::Valid
        );
        assert_eq!(
            verdict_for(Err(KeychainReadError::Absent)),
            SessionVerdict::ReauthRequired
        );
    }

    /// The success path: a readable secret is returned verbatim, while an entry
    /// the keychain stores as empty is a missing credential, not a usable one.
    #[test]
    fn a_readable_secret_is_returned_and_an_empty_one_is_not() {
        let secret = boot_gate_client_secret(Ok("live-secret".to_string()))
            .expect("a readable secret must refresh");
        assert_eq!(secret, "live-secret");

        let empty =
            boot_gate_client_secret(Ok(String::new())).expect_err("an empty secret cannot refresh");
        assert_eq!(empty, RefreshFailure::Unavailable);
    }

    /// Issue #560 has to keep working through the typed read: the single read's
    /// answer is what `update_config` hands back to the UI, so a keyring that
    /// locked must not be laundered into `absent`.
    #[test]
    fn the_reads_observation_maps_onto_the_config_tri_state() {
        assert!(matches!(
            presence_from_read(&Ok("secret".to_string())),
            KeychainPresence::Present
        ));
        assert!(matches!(
            presence_from_read(&Err(KeychainReadError::Absent)),
            KeychainPresence::Absent
        ));
        assert!(matches!(
            presence_from_read(&Err(KeychainReadError::Unavailable("locked".into()))),
            KeychainPresence::Unavailable(help) if help == "locked"
        ));
        assert!(
            matches!(
                presence_from_read(&Err(KeychainReadError::Corrupt("undecodable".into()))),
                KeychainPresence::Unavailable(_)
            ),
            "an item that is stored but unreadable is not absent: reporting it as \
             absent would send the user through re-onboarding"
        );
    }

    /// Issue #560: the boot gate's observation has to reach the in-memory
    /// config, not just the log. `update_config` merges onto that copy and
    /// returns it to the UI, so without this a keyring that locked before the
    /// verdict would be laundered back into `absent` — the state that sends
    /// the user through re-onboarding.
    #[test]
    fn boot_gate_observation_lands_in_the_in_memory_config() {
        let state = Arc::new(AppState::new());
        *state.config.get_mut() = Some(AppConfig::default());

        record_client_secret_state(&state, &KeychainPresence::Unavailable("locked".into()));
        let config = state.config.get().clone().expect("config was just stored");
        assert_eq!(
            config.spotify.client_secret_state,
            ClientSecretState::Unavailable
        );
        assert!(
            !config.spotify.client_secret_set,
            "the Present-only projection must agree with the tri-state"
        );

        // The other two observations are mirrored the same way.
        record_client_secret_state(&state, &KeychainPresence::Present);
        let config = state.config.get().clone().unwrap();
        assert_eq!(
            config.spotify.client_secret_state,
            ClientSecretState::Present
        );
        assert!(config.spotify.client_secret_set);

        record_client_secret_state(&state, &KeychainPresence::Absent);
        let config = state.config.get().clone().unwrap();
        assert_eq!(
            config.spotify.client_secret_state,
            ClientSecretState::Absent
        );
        assert!(!config.spotify.client_secret_set);
    }

    /// Before the frontend has loaded a config there is nothing to correct,
    /// and the boot gate must not panic on that path.
    #[test]
    fn recording_without_a_loaded_config_is_a_no_op() {
        let state = Arc::new(AppState::new());
        record_client_secret_state(&state, &KeychainPresence::Unavailable("locked".into()));
        assert!(state.config.get().is_none());
    }

    /// Issue #942: two overlapping checks must share one refresh, and the
    /// caller that arrived second must observe the first one's verdict.
    /// Pre-fix the cache was written only after a check finished, so the Retry
    /// the front end offers when its 8 s `BOOT_TIMEOUT_MS` fires started a
    /// second gate run and spent the same refresh token; against a provider
    /// that rotates tokens the loser's `invalid_grant` arm cleared a session
    /// that was alive. The refresh counter is the observable.
    #[test]
    fn overlapping_boot_checks_share_one_refresh() {
        let flight = Mutex::new(());
        let cache = Mutex::new(None::<(Instant, bool)>);
        let refreshes = AtomicUsize::new(0);
        let in_check = AtomicBool::new(false);

        let cached = || -> Option<bool> {
            let guard = cache.lock();
            let verdict = guard
                .as_ref()
                .filter(|(ts, _)| ts.elapsed() < ONBOARDING_CACHE_TTL)
                .map(|(_, result)| *result);
            verdict
        };
        std::thread::scope(|scope| {
            let first = scope.spawn(|| {
                single_flight(&flight, cached, || {
                    refreshes.fetch_add(1, Ordering::SeqCst);
                    in_check.store(true, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(150));
                    *cache.lock() = Some((Instant::now(), true));
                    true
                })
            });

            // Wait until the first run is provably inside its refresh, so the
            // second call below overlaps it instead of racing it.
            let deadline = Instant::now() + Duration::from_secs(5);
            while !in_check.load(Ordering::SeqCst) {
                assert!(Instant::now() < deadline, "the first check never started");
                std::thread::sleep(Duration::from_millis(1));
            }

            let shared = single_flight(&flight, cached, || {
                refreshes.fetch_add(1, Ordering::SeqCst);
                false
            });

            assert_eq!(
                refreshes.load(Ordering::SeqCst),
                1,
                "a retry issued while the gate was in flight must not refresh a second time"
            );
            assert!(
                shared,
                "the overlapping caller must share the in-flight verdict, not its own"
            );
            assert!(first.join().expect("the first check must not panic"));
        });
    }

    /// The double-check is what makes sharing work: a verdict that is already
    /// fresh answers without starting another gate run, and the flight lock is
    /// released with the run rather than poisoned by it.
    #[test]
    fn a_fresh_verdict_is_shared_without_running_the_check() {
        let flight = Mutex::new(());
        // `T` is `bool` here: the cached closure supplies `Option<bool>` and
        // the run closure supplies the verdict itself.
        let shared = single_flight(
            &flight,
            || Some(false),
            || panic!("a fresh verdict must not start another gate run"),
        );
        assert!(
            !shared,
            "the shared verdict is returned verbatim, not re-derived"
        );
        assert!(
            single_flight(&flight, || None, || true),
            "the flight lock must be released, or the next boot check would block forever"
        );
    }

    /// The command's own cache read: an empty cache has no verdict to share,
    /// and the verdict a caller just stored is the one the next one gets.
    #[test]
    fn cached_verdict_reports_the_stored_result() {
        let state = Arc::new(AppState::new());
        assert_eq!(cached_verdict(&state, "test"), None);
        *state.onboarding_cache.lock() = Some((Instant::now(), true));
        assert_eq!(cached_verdict(&state, "test"), Some(true));
    }
    /// Issue #978: the wizard routes on these codes, so every missing-token
    /// combination must yield a stable, machine-readable marker. Pre-fix the
    /// command returned the internals sentence `Missing tokens: spotify=false,
    /// teams=true`, which the wizard rendered verbatim through
    /// `validation.setupFailed` — untranslated, and naming no action.
    #[test]
    fn missing_token_error_is_a_routable_code() {
        assert_eq!(
            missing_tokens_error(true, true),
            None,
            "both connected: sync starts, no code"
        );
        assert_eq!(
            missing_tokens_error(false, true),
            Some("spotify_not_connected")
        );
        assert_eq!(
            missing_tokens_error(true, false),
            Some("teams_not_connected")
        );
        assert_eq!(
            missing_tokens_error(false, false),
            Some("spotify_not_connected,teams_not_connected"),
            "both slots empty: both codes, Spotify first, so the wizard can \
             route to the first step and surface the other on the next finish"
        );
    }

    /// Issue #942: the single-flight lock is only worth anything if the command
    /// actually routes through it — a direct `is_onboarding_complete_impl` call
    /// in the command body is the pre-#942 shape, where two overlapping callers
    /// each refreshed from the same token. Structural because the command needs
    /// a live `AppHandle` and a real refresh round-trip.
    #[test]
    fn the_command_routes_through_the_single_flight_lock() {
        let body = crate::token_io::test_scan::fn_body(
            include_str!("onboarding.rs"),
            "fn is_onboarding_complete(",
        );
        let flat = body.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            flat.contains("single_flight( &BOOT_GATE_FLIGHT,"),
            "the command must take the process-wide flight lock, or a Retry can \
             start a second gate run"
        );
        assert!(
            flat.contains("is_onboarding_complete_impl(&state_clone, &app_clone)"),
            "the gate body must run as the single-flight payload"
        );
    }

    /// Issue #760: the boot gate reads the client secret exactly once. The
    /// presence probe plus the fetch are what the typed island replaced, and
    /// re-introducing either restores the TOCTOU window between them.
    #[test]
    fn the_spotify_gate_reads_the_secret_once() {
        let body = crate::token_io::test_scan::fn_body(
            include_str!("onboarding.rs"),
            "fn spotify_session_verdict(",
        );
        assert_eq!(
            body.matches("keychain::read_spotify_client_secret()")
                .count(),
            1,
            "the gate must perform exactly one keychain read"
        );
        for gone in [
            "spotify_client_secret_presence",
            "get_spotify_client_secret",
            "peek_spotify_client_secret",
        ] {
            assert!(
                !body.contains(gone),
                "`{gone}` must not return to the boot gate: it is the second probe \
                 the typed read replaced"
            );
        }
    }
}
