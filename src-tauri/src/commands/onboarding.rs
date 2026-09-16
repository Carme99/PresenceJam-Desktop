//! Onboarding Tauri commands: completion check, transition, and disconnect/reconnect.
//!
//! See issue #76. Owns `is_onboarding_complete_impl` (the spawn_blocking body
//! for the async cache-first check) and the `ONBOARDING_CACHE_TTL` constant.

use crate::config;
use crate::keychain::{self, KeychainPresence};
use crate::polling::{cas_refresh_or_discard, CasOutcome};
use crate::token_io;
use crate::AppState;
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
    {
        let guard = state.onboarding_cache.lock();
        if let Some((ts, result)) = *guard {
            if ts.elapsed() < ONBOARDING_CACHE_TTL {
                log::info!(
                    "{CMD} is_onboarding_complete: cache HIT (age={:.2}s, result={})",
                    ts.elapsed().as_secs_f32(),
                    result
                );
                return Ok(result);
            }
        }
    }

    // Cache miss — run the actual check on a blocking thread (HTTPS round-trips).
    let state_clone: Arc<AppState> = Arc::clone(&state);
    let app_clone = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        is_onboarding_complete_impl(&state_clone, &app_clone)
    })
    .await
    .map_err(|e| format!("is_onboarding_complete task panicked: {}", e))??;

    // Store result in cache. We cache both `true` and `false` outcomes — a recent "complete"
    // result is just as valid as a recent "incomplete" one for the 30s window.
    *state.onboarding_cache.lock() = Some((Instant::now(), result));
    log::info!(
        "{CMD} is_onboarding_complete: cache MISS, stored fresh result={}",
        result
    );
    Ok(result)
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
fn discard_dead_session(label: &str, clear: impl FnOnce(), state: &Arc<AppState>, app: &AppHandle) {
    clear();
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
        let client_secret = boot_gate_client_secret(
            keychain::peek_spotify_client_secret(),
            || {
                let presence = keychain::spotify_client_secret_presence();
                // Issue #560: this is the only probe the boot gate makes, and
                // it runs before any frontend config surface has loaded. Leave
                // its answer in the in-memory config so a later save cannot
                // hand the UI a `client_secret_state` that contradicts it.
                record_client_secret_state(state, &presence);
                presence
            },
            keychain::get_spotify_client_secret,
        )?;

        let pre_refresh_access_token = tokens.access_token.clone();
        // Shared CAS guard (ARCHITECTURE.md § Token-refresh concurrency): a
        // concurrent poll-thread refresh must win, and its newer token must not
        // be clobbered by ours.
        let outcome = cas_refresh_or_discard(
            "spotify-onboarding",
            &mut *state.tokens.spotify_mut(),
            &pre_refresh_access_token,
            || refresh_spotify_token(tokens, client_id, &client_secret),
            |t| &t.access_token,
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
            CasOutcome::RefreshFailed(SpotifyApiError::InvalidGrant) => {
                discard_dead_session("Spotify", || *state.tokens.spotify_mut() = None, state, app);
                Err(RefreshFailure::Dead)
            }
            CasOutcome::RefreshFailed(_) => Err(RefreshFailure::Transient),
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

/// Resolve the Spotify `client_secret` for the boot gate.
///
/// Issue #561: the keychain is a *tri-state* — present, absent, or
/// unavailable (no Secret Service daemon, a locked keyring, denied storage
/// access). Only a positively absent entry justifies sending the user to
/// reconnect; an unavailable keychain is transient by construction (the
/// secret is still there) and must keep the session, exactly like a flaky
/// network does. Pre-fix this collapsed every keychain error into an empty
/// string via `unwrap_or_default()`, which the gate read as "not
/// configured" — so a locked keyring at launch bounced a fully credentialed
/// user into the setup wizard, contradicting this module's own
/// transient-failure policy.
fn boot_gate_client_secret(
    peeked: Option<String>,
    presence: impl FnOnce() -> KeychainPresence,
    fetch: impl FnOnce() -> Result<String, String>,
) -> Result<String, RefreshFailure> {
    // The polling thread primes the cache; a hit costs no keychain call.
    if let Some(secret) = peeked.filter(|s| !s.is_empty()) {
        return Ok(secret);
    }
    match presence() {
        KeychainPresence::Present => match fetch() {
            Ok(secret) if !secret.is_empty() => Ok(secret),
            Ok(_) => {
                log::warn!(
                    "{CMD} is_onboarding_complete: keychain holds an empty Spotify client_secret; re-auth required"
                );
                Err(RefreshFailure::Unavailable)
            }
            Err(e) => {
                // Readable a moment ago, failed now (the entry was deleted
                // from the OS UI mid-call, or the keyring just locked):
                // retryable, not re-auth.
                log::warn!(
                    "{CMD} is_onboarding_complete: keychain reported the Spotify client_secret present but the read failed: {}",
                    e
                );
                Err(RefreshFailure::Transient)
            }
        },
        KeychainPresence::Absent => {
            log::warn!(
                "{CMD} is_onboarding_complete: Spotify access token expired but no client_secret is stored; re-auth required"
            );
            Err(RefreshFailure::Unavailable)
        }
        KeychainPresence::Unavailable(help) => {
            log::warn!(
                "{CMD} is_onboarding_complete: OS keychain unavailable, keeping the session and retrying later: {}",
                help
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
        let outcome = cas_refresh_or_discard(
            "teams-onboarding",
            &mut *state.tokens.teams_mut(),
            &pre_refresh_access_token,
            || refresh_teams_token(tokens),
            |t| &t.access_token,
        );
        match outcome {
            CasOutcome::Committed(_) => {
                persist_refreshed(state, app, "Teams");
                Ok(())
            }
            CasOutcome::Discarded { current } if current.is_some() => Ok(()),
            CasOutcome::Discarded { .. } => Err(RefreshFailure::Dead),
            CasOutcome::RefreshFailed(TeamsApiError::InvalidGrant) => {
                discard_dead_session("Teams", || *state.tokens.teams_mut() = None, state, app);
                Err(RefreshFailure::Dead)
            }
            CasOutcome::RefreshFailed(_) => Err(RefreshFailure::Transient),
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

    if has_spotify && has_teams {
        log::info!("{CMD} complete_onboarding: both tokens present, starting sync");
        super::sync::start_syncing(window, state, app).await?;
        log::info!("{CMD} complete_onboarding: sync started successfully");
    } else {
        log::error!(
            "{CMD} complete_onboarding: missing tokens, cannot start sync (spotify={}, teams={})",
            has_spotify,
            has_teams
        );
        return Err(format!(
            "Missing tokens: spotify={}, teams={}",
            has_spotify, has_teams
        ));
    }

    log::info!("{CMD} complete_onboarding: SUCCESS");
    Ok(())
}

#[tauri::command]
pub fn reconnect_spotify(
    state: tauri::State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<(), String> {
    log::debug!("{CMD} reconnect_spotify: ENTRY");

    // Clear Spotify tokens from state
    *state.tokens.spotify_mut() = None;
    log::info!("{CMD} reconnect_spotify: cleared spotify_tokens");

    // Clear pending Spotify auth
    *state.pending.spotify_mut() = None;
    log::info!("{CMD} reconnect_spotify: cleared pending_spotify_auth");

    // Persist the cleared state to disk atomically.
    if let Err(e) = token_io::persist_tokens(state.inner(), &app) {
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
pub fn reconnect_teams(
    state: tauri::State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<(), String> {
    log::debug!("{CMD} reconnect_teams: ENTRY");

    // Clear Teams tokens from state
    *state.tokens.teams_mut() = None;
    log::info!("{CMD} reconnect_teams: cleared teams_tokens");

    // Persist the cleared state to disk atomically.
    if let Err(e) = token_io::persist_tokens(state.inner(), &app) {
        log::warn!(
            "{CMD} reconnect_teams: failed to persist cleared state - {}",
            e
        );
    }

    // Issue #70: invalidate the onboarding cache.
    state.onboarding_cache.invalidate();
    log::info!("{CMD} reconnect_teams: onboarding_cache invalidated");

    // Emit event so UI can show re-auth flow
    if let Err(e) = app.emit("teams-reconnect-required", ()) {
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
        boot_gate_client_secret, record_client_secret_state, session_verdict, RefreshFailure,
        SessionVerdict,
    };
    use crate::config::{AppConfig, ClientSecretState};
    use crate::keychain::KeychainPresence;
    use crate::AppState;
    use std::sync::Arc;

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

    /// The priming cache hit must not touch the keychain at all: the polling
    /// thread fills it, and a boot check runs on every onboarding remount.
    #[test]
    fn cached_client_secret_short_circuits_the_keychain() {
        let secret = boot_gate_client_secret(
            Some("cached-secret".to_string()),
            || panic!("a cache hit must not probe the keychain"),
            || panic!("a cache hit must not read the keychain"),
        )
        .expect("a primed cache is a usable credential");
        assert_eq!(secret, "cached-secret");
    }

    /// Issue #561: present, absent and unavailable are three different
    /// answers. A positively *absent* entry is the only one that can mean
    /// "re-onboard"; anything the keychain cannot answer must keep the
    /// session, because the secret is still stored and retrying is free.
    #[test]
    fn keychain_error_is_transient_not_unavailable() {
        let absent = boot_gate_client_secret(
            None,
            || KeychainPresence::Absent,
            || panic!("an absent entry must not be read"),
        )
        .expect_err("an absent secret cannot refresh");
        assert_eq!(absent, RefreshFailure::Unavailable);

        let locked = boot_gate_client_secret(
            None,
            || KeychainPresence::Unavailable("Secret Service locked".to_string()),
            || panic!("an unavailable keychain must not be read"),
        )
        .expect_err("a locked keychain cannot refresh");
        assert_eq!(
            locked,
            RefreshFailure::Transient,
            "a locked keychain is recoverable; the secret is still stored"
        );
    }

    /// The end-to-end boot-gate consequence: a keychain error must leave a
    /// fully credentialed returning user out of the setup wizard, while an
    /// actually absent secret still routes them to reconnect.
    #[test]
    fn boot_gate_keeps_the_session_when_the_keychain_is_locked() {
        let verdict_for = |presence: KeychainPresence| {
            session_verdict(true, || {
                boot_gate_client_secret(None, || presence, || panic!("must not read")).map(|_| ())
            })
        };

        assert_eq!(
            verdict_for(KeychainPresence::Unavailable("locked".to_string())),
            SessionVerdict::Valid
        );
        assert_eq!(
            verdict_for(KeychainPresence::Absent),
            SessionVerdict::ReauthRequired
        );
    }

    /// A keychain that reports the entry present and then fails the read
    /// (deleted from the OS UI mid-call, or locked between the two calls)
    /// must not be treated as a missing credential either.
    #[test]
    fn present_then_failing_read_is_transient() {
        let failure = boot_gate_client_secret(
            None,
            || KeychainPresence::Present,
            || Err("Failed to read Spotify client secret from keychain".to_string()),
        )
        .expect_err("a failing read cannot refresh");
        assert_eq!(failure, RefreshFailure::Transient);
    }

    /// The success path: a present, readable secret is returned verbatim.
    #[test]
    fn present_readable_secret_is_returned() {
        let secret = boot_gate_client_secret(
            None,
            || KeychainPresence::Present,
            || Ok("live-secret".to_string()),
        )
        .expect("a readable secret must refresh");
        assert_eq!(secret, "live-secret");
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
}
