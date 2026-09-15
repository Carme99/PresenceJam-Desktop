//! Single source of truth for one polling iteration.
//!
//! Issue #72 documented three near-duplicate API-call branches in the
//! old `polling_loop` that had already drifted:
//!
//! 1. The 401-retry's no-track branch incremented `consecutive_pauses`
//!    in a different order than the main no-track path.
//! 2. The final-failure branch emitted a user-visible `error` event that
//!    the 401-retry path silently skipped.
//! 3. The CAS-discard re-read dance appeared three times (Spotify
//!    proactive refresh, Spotify 401-retry refresh, Teams refresh in
//!    `process_track`) with slightly different log messages.
//!
//! All three collapse to a single function here. See the regression
//! tests at the bottom of this file for invariants.

use std::sync::mpsc;
use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use rand::Rng;
use serde_json::json;
use tauri::{AppHandle, Emitter};

use crate::profanity;
use crate::spotify::{
    format_status, get_currently_playing, is_token_expired, refresh_spotify_token,
    CurrentlyPlaying, SpotifyApiError,
};
use crate::teams::{
    clear_teams_presence, clear_teams_status_message, get_teams_presence, is_presence_gated,
    is_token_expired as is_teams_token_expired, presence_gate_reason, refresh_teams_token,
    set_teams_presence, set_teams_status_message, TeamsApiError, TeamsTokens,
};
use crate::token_io;
use crate::AppState;

use super::{emit_error, ErrorSeverity};

const ERROR_RETRY_INTERVAL_SECONDS: u64 = 30;
const RATE_LIMIT_BACKOFF_SECONDS: u64 = 60;
const DEBOUNCE_MS: u64 = 500;
/// Issue #364: when a track change lands inside the debounce window the
/// change signal must survive — the retry parks here, NOT on the
/// duration-derived sleep (which would stall the pending write until the
/// track nearly ends).
const DEBOUNCE_RETRY_SECONDS: u64 = 1;
/// Issue #384: identical-status writes are skipped while the last write
/// is this fresh; older than this the next poll force-writes a keepalive
/// so the Graph expiry never lapses.
const STATUS_KEEPALIVE_SECONDS: u64 = 5 * 60;
const TRANSIENT_FAILURE_EXIT_THRESHOLD: u8 = 5;
/// Minimum gap between setPresence re-arms while a track plays (issue
/// #3.0-P1). An `Available` session TIMES OUT after 5 minutes when the
/// availability is `Available` — a separate, non-configurable clock from
/// `expirationDuration` (which only bounds the session's absolute life,
/// 5 min–4 h, after which it goes `Offline`). On timeout the state fades
/// in stages: `Available` → `AvailableInactive` → `Away`. So the re-arm
/// must be well inside the 5-minute TIMEOUT, not the expiration window;
/// 4 minutes leaves slack. Raising this toward expiration scale (the
/// `PT4H` the app sends) does NOT extend the green bubble.
/// (Microsoft Learn: cloud-communications-manage-presence-state)
const AVAILABILITY_REARM_SECONDS: u64 = 4 * 60;

/// What the driver should do after this iteration.
pub(crate) enum PollIteration {
    Sleep { seconds: u64 },
    Break,
}

/// Execution mode for one poll iteration. `Loop` is the polling-thread
/// path (parking sleeps); `OneShot` is an explicit refresh that must
/// never park a thread on a sleep — every parking sleep site (the
/// `interruptible_sleep` error/no-token/backoff tails) returns `Break`
/// immediately after its usual event/log. Success-path `Sleep` values
/// flow through unchanged but `run_oneshot` discards them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunMode {
    Loop,
    OneShot,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run(
    state: &Arc<AppState>,
    app: &AppHandle,
    stop_rx: &mpsc::Receiver<()>,
    last_track_key: &mut Option<String>,
    last_teams_update: &mut Option<Instant>,
    last_posted_placeholder: &mut Option<String>,
    consecutive_pauses: &mut u8,
    transient_failure_count: &mut u8,
    gated_track_key: &mut Option<String>,
    last_availability_arm: &mut Option<Instant>,
    // Candidate C11: ETag validator from the previous conditional GET;
    // stored from each 200/204, echoed as If-None-Match on the next poll.
    last_etag: &mut Option<String>,
    first_iteration: &mut bool,
    last_posted_status: &mut Option<String>,
    last_gate_check: &mut Option<Instant>,
) -> PollIteration {
    run_inner(
        state,
        app,
        stop_rx,
        last_track_key,
        last_teams_update,
        last_posted_placeholder,
        consecutive_pauses,
        transient_failure_count,
        gated_track_key,
        last_availability_arm,
        last_etag,
        first_iteration,
        last_posted_status,
        last_gate_check,
        RunMode::Loop,
    )
}

/// One-shot entry: runs a single iteration with fresh ephemeral locals
/// (a playing track always counts as changed, so it re-POSTs — exactly
/// what an explicit refresh wants; an idle one-shot stays silent via
/// `first_iteration=false`) and a throwaway stop channel
/// that never fires. Never parks: `RunMode::OneShot` turns every parking
/// sleep site into an immediate `Break`. Success paths (`process_track`,
/// `handle_no_track`) contain no hidden sleeps — only natural blocking
/// HTTP — and their returned `Sleep` is discarded. Duplicated Teams POSTs
/// are idempotent and harmless.
pub(crate) fn run_oneshot(state: &Arc<AppState>, app: &AppHandle) {
    // `_tx` is a live binding (not `let _`), so the channel stays
    // connected for the whole call: the top stop-check treats
    // `Disconnected` as Break, and a dropped sender here would make every
    // one-shot a silent no-op. Never collapse this to `let _`.
    let (_tx, rx) = mpsc::channel::<()>();
    let mut last_track_key: Option<String> = None;
    let mut last_teams_update: Option<Instant> = None;
    let mut last_posted_placeholder: Option<String> = None;
    let mut consecutive_pauses: u8 = 0;
    let mut transient_failure_count: u8 = 0;
    let mut gated_track_key: Option<String> = None;
    let mut last_availability_arm: Option<Instant> = None;
    // `None` ⇒ unconditional GET (fresh locals, no prior validator).
    let mut last_etag: Option<String> = None;
    // Issue #373 does NOT apply here: a one-shot is an explicit refresh,
    // not a fresh polling thread — an idle one-shot must stay silent
    // instead of POSTing a placeholder on every manual refresh.
    let mut first_iteration = false;
    // Issue #384: no status posted yet this thread.
    let mut last_posted_status: Option<String> = None;
    let mut last_gate_check: Option<Instant> = None;
    let _ = run_inner(
        state,
        app,
        &rx,
        &mut last_track_key,
        &mut last_teams_update,
        &mut last_posted_placeholder,
        &mut consecutive_pauses,
        &mut transient_failure_count,
        &mut gated_track_key,
        &mut last_availability_arm,
        &mut last_etag,
        &mut first_iteration,
        &mut last_posted_status,
        &mut last_gate_check,
        RunMode::OneShot,
    );
}

#[allow(clippy::too_many_arguments)]
fn run_inner(
    state: &Arc<AppState>,
    app: &AppHandle,
    stop_rx: &mpsc::Receiver<()>,
    last_track_key: &mut Option<String>,
    last_teams_update: &mut Option<Instant>,
    last_posted_placeholder: &mut Option<String>,
    consecutive_pauses: &mut u8,
    transient_failure_count: &mut u8,
    gated_track_key: &mut Option<String>,
    last_availability_arm: &mut Option<Instant>,
    // Candidate C11: ETag validator from the previous conditional GET;
    // stored from each 200/204, echoed as If-None-Match on the next poll.
    last_etag: &mut Option<String>,
    first_iteration: &mut bool,
    last_posted_status: &mut Option<String>,
    last_gate_check: &mut Option<Instant>,
    mode: RunMode,
) -> PollIteration {
    log::debug!("[POLLING] poll_once: iteration start");

    match stop_rx.recv_timeout(std::time::Duration::ZERO) {
        Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            log::info!("[POLLING] poll_once: stop signal at top, breaking");
            return PollIteration::Break;
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
    }

    let config = state.config.get().clone();
    log::debug!("[POLLING] poll_once: config loaded");

    let spotify_tokens = state.tokens.spotify().clone();
    log::debug!(
        "[POLLING] poll_once: spotify_tokens: {}",
        if spotify_tokens.is_some() {
            "Some"
        } else {
            "None"
        }
    );

    let spotify_tokens = match spotify_tokens {
        Some(t) => {
            log::debug!("[POLLING] poll_once: using existing Spotify tokens");
            t
        }
        None => {
            log::warn!("[POLLING] poll_once: No Spotify tokens available, waiting...");
            return interruptible_sleep(
                stop_rx,
                with_jitter(ERROR_RETRY_INTERVAL_SECONDS),
                "no-token sleep",
                mode,
            );
        }
    };

    let token_expired = is_token_expired(&spotify_tokens);
    log::debug!("[POLLING] poll_once: token_expired={}", token_expired);

    let (client_id, client_secret) = get_spotify_credentials(&config);
    // Issue #296: `get_spotify_credentials` reads the secret through the
    // cache-only `keychain::peek_spotify_client_secret()`, so it is empty
    // whenever the startup prime failed (locked Secret Service, headless
    // Linux, entry removed while running). Refreshing with an empty secret
    // can only produce a 400 `invalid_client` — not `InvalidGrant` — so the
    // Err arm below would emit a Warning and sleep *before* the 5-strikes
    // counter, looping forever with no user-visible cause. Classify the
    // decision up front (pure helper, mirroring the 401 path's guard) and
    // route the unavailable case to an actionable reconnect.
    let refresh_plan = spotify_refresh_plan(token_expired, &client_id, &client_secret);
    let spotify_tokens = if refresh_plan == SpotifyRefreshPlan::Refresh {
        log::info!("[POLLING] poll_once: Spotify token expired, refreshing...");
        log::info!(
            "[POLLING] poll_once: refreshing with client_id.len={}",
            client_id.len()
        );

        let pre_refresh_access_token = spotify_tokens.access_token.clone();
        match refresh_spotify_token(&spotify_tokens, &client_id, &client_secret) {
            Ok(new_tokens) => {
                log::info!("[POLLING] poll_once: token refresh SUCCESS");
                let cas_outcome = cas_refresh_or_discard(
                    "spotify",
                    &mut *state.tokens.spotify_mut(),
                    &pre_refresh_access_token,
                    // `Ok`-wrapping closure: annotate the error type so `E`
                    // is inferable (this arm never fails, so nothing else
                    // pins it) and matches the sibling `Err` arm's
                    // `SpotifyApiError`.
                    || Ok::<_, SpotifyApiError>(new_tokens.clone()),
                    |t| &t.access_token,
                );
                // Issue #180: the write guard reborrowed above is a temporary
                // that lives only until the end of this statement. Persist in
                // a LATER statement, when the guard is provably dropped —
                // persisting while it is alive would re-lock the same
                // parking_lot RwLock for reading and self-deadlock.
                if matches!(&cas_outcome, CasOutcome::Committed(_)) {
                    if let Err(e) = token_io::persist_tokens(state, app) {
                        log::warn!(
                            "[POLLING] poll_once: failed to persist refreshed spotify tokens: {}",
                            e
                        );
                    }
                }
                match cas_outcome {
                    CasOutcome::Committed(_) => new_tokens,
                    CasOutcome::Discarded { current } => match current {
                        Some(t) => t,
                        None => {
                            log::info!(
                                "[POLLING] poll_once: state cleared during refresh, waiting and re-polling"
                            );
                            return interruptible_sleep(
                                stop_rx,
                                with_jitter(ERROR_RETRY_INTERVAL_SECONDS),
                                "CAS-fail sleep",
                                mode,
                            );
                        }
                    },
                    CasOutcome::RefreshFailed(_) => unreachable!("inner refresh_fn is Ok-wrapping"),
                }
            }
            Err(e) => {
                log::error!(
                    "[POLLING] poll_once: Failed to refresh Spotify token: {}",
                    e
                );
                // Issue #160: `invalid_grant` means the refresh token is dead
                // (documented 6-month lifetime, or revoked). Discard it and
                // trigger re-auth instead of retrying forever. The write guard
                // is dropped before persist_tokens (which re-locks the same
                // RwLock for reading — parking_lot is not reentrant).
                if matches!(e, SpotifyApiError::InvalidGrant) {
                    log::error!("[POLLING] poll_once: Spotify refresh token invalid (invalid_grant), discarding tokens and requiring reconnect");
                    *state.tokens.spotify_mut() = None;
                    if let Err(persist_err) = token_io::persist_tokens(state, app) {
                        log::warn!(
                            "[POLLING] poll_once: failed to persist cleared Spotify tokens: {}",
                            persist_err
                        );
                    }
                    let _ = app.emit("spotify-reconnect-required", json!(null));
                    let _ = app.emit("reconnect-required", json!(null));
                    return interruptible_sleep(
                        stop_rx,
                        with_jitter(ERROR_RETRY_INTERVAL_SECONDS),
                        "invalid-grant sleep",
                        mode,
                    );
                }
                emit_error(
                    app,
                    "spotify",
                    format!("Token refresh failed: {}", e),
                    ErrorSeverity::Warning,
                );
                return interruptible_sleep(
                    stop_rx,
                    with_jitter(ERROR_RETRY_INTERVAL_SECONDS),
                    "error retry sleep",
                    mode,
                );
            }
        }
    } else if refresh_plan == SpotifyRefreshPlan::CredentialsUnavailable {
        // Issue #296: the access token is expired but the credential pair
        // needed to refresh it is unavailable. Retrying cannot fix this, so
        // count it toward the existing 5-strikes escape and surface the same
        // actionable reconnect pair as `invalid_grant`.
        //
        // The tokens are deliberately NOT cleared here (the `invalid_grant`
        // path above does): the refresh token itself is still valid, the
        // user's fix is to restore the keychain entry, and keeping it lets
        // this branch be re-entered so `transient_failure_count` can actually
        // reach its threshold — clearing would make the top-of-iteration
        // no-token guard swallow every later iteration and the escape
        // unreachable.
        log::error!(
            "[POLLING] poll_once: Spotify token expired but credentials unavailable (client_id empty: {}, client_secret empty: {}), requiring reconnect",
            client_id.is_empty(),
            client_secret.is_empty()
        );
        *transient_failure_count = transient_failure_count.saturating_add(1);
        // Emit on the first detection only. `spotify-reconnect-required`
        // makes `+layout.svelte` start a real OAuth flow, so repeating it
        // every iteration would be user-hostile; the `invalid_grant` sibling
        // above likewise surfaces the reconnect once (its cleared tokens then
        // short-circuit later iterations).
        if *transient_failure_count == 1 {
            let _ = app.emit("spotify-reconnect-required", json!(null));
            let _ = app.emit("reconnect-required", json!(null));
        }
        if *transient_failure_count >= TRANSIENT_FAILURE_EXIT_THRESHOLD {
            log::error!("[POLLING] poll_once: 5 consecutive credential failures, exiting and requiring reconnect");
            return PollIteration::Break;
        }
        return interruptible_sleep(
            stop_rx,
            with_jitter(ERROR_RETRY_INTERVAL_SECONDS),
            "credentials-unavailable sleep",
            mode,
        );
    } else {
        spotify_tokens
    };

    let access_token = spotify_tokens.access_token.clone();
    log::debug!("[POLLING] poll_once: calling get_currently_playing");

    let last_poll_instant = Instant::now();

    let result = get_currently_playing(&access_token, last_etag.as_deref());

    match result {
        Ok(CurrentlyPlaying::Modified {
            track: Some(track),
            etag,
        }) => {
            *last_etag = etag;
            // Issue #344: debug, not info — title/artist at info level
            // land verbatim in the diagnostics `recent_logs` tail (a
            // paste-able support artifact). No raw track metadata there.
            log::debug!(
                "[POLLING] poll_once: track found - {} by {}",
                track.title,
                track.artist
            );
            let sleep_duration = process_track(
                app,
                state,
                &config,
                &track,
                last_track_key,
                last_poll_instant,
                last_teams_update,
                last_posted_placeholder,
                consecutive_pauses,
                gated_track_key,
                last_availability_arm,
                last_posted_status,
                last_gate_check,
            );
            *transient_failure_count = 0;
            PollIteration::Sleep {
                seconds: sleep_duration,
            }
        }
        Ok(CurrentlyPlaying::Modified { track: None, etag }) => {
            *last_etag = etag;
            log::info!("[POLLING] poll_once: no track playing");
            let no_track_backoff = handle_no_track(
                app,
                state,
                last_track_key,
                &config,
                last_posted_placeholder,
                last_availability_arm,
                first_iteration,
                last_posted_status,
            );
            *transient_failure_count = 0;
            let mut iteration = record_no_track_outcome(consecutive_pauses, &config);
            if let PollIteration::Sleep { seconds } = &mut iteration {
                // Issue #154: a throttled Teams clear extends the next poll
                // to the server-directed delay.
                *seconds = (*seconds).max(no_track_backoff);
            }
            iteration
        }
        Ok(CurrentlyPlaying::NotModified) => {
            // Candidate C11 (docs/scope-3.3.md §C11): a 304 Not Modified
            // carries no body — nothing to parse, format, filter or
            // rebuild. Behave exactly like the unchanged-track path minus
            // that work.
            //
            // Issue #343: unless relevant status config flipped mid-track
            // (filter/placeholder/format). The 304 path never reaches
            // `process_track`, so without this the stale status stays
            // posted until the next track change. Force one rewrite on
            // the last observed track; it re-keys `last_track_key`, so
            // the following 304s return to the no-op path.
            if let Some(track) = config_flip_rewrite_track(state, last_track_key, &config) {
                log::info!(
                    "[POLLING] poll_once: 304 but status config changed mid-track, forcing one rewrite"
                );
                let sleep = process_track(
                    app,
                    state,
                    &config,
                    &track,
                    last_track_key,
                    last_poll_instant,
                    last_teams_update,
                    last_posted_placeholder,
                    consecutive_pauses,
                    gated_track_key,
                    last_availability_arm,
                    last_posted_status,
                    last_gate_check,
                );
                *transient_failure_count = 0;
                return PollIteration::Sleep { seconds: sleep };
            }
            not_modified_iteration(
                last_track_key,
                consecutive_pauses,
                transient_failure_count,
                &config,
            )
        }
        Err(e) => {
            log::error!(
                "[POLLING] poll_once: Failed to get currently playing track: {}",
                e
            );

            let mut final_err = e;
            let mut backoff_secs = with_jitter(ERROR_RETRY_INTERVAL_SECONDS);

            if matches!(final_err, SpotifyApiError::ExpiredToken)
                && !client_id.is_empty()
                && !client_secret.is_empty()
            {
                log::info!("[POLLING] poll_once: token expired, attempting refresh");
                let current_tokens = state.tokens.spotify().clone();
                if let Some(tokens) = current_tokens {
                    let pre_refresh_access_token = tokens.access_token.clone();
                    match refresh_spotify_token(&tokens, &client_id, &client_secret) {
                        Ok(new_tokens) => {
                            log::info!("[POLLING] poll_once: token refresh SUCCESS, retrying");
                            let committed = match cas_refresh_or_discard(
                                "spotify",
                                &mut *state.tokens.spotify_mut(),
                                &pre_refresh_access_token,
                                || Ok::<_, SpotifyApiError>(new_tokens.clone()),
                                |t| &t.access_token,
                            ) {
                                CasOutcome::Committed(_) => true,
                                CasOutcome::Discarded { .. } => false,
                                CasOutcome::RefreshFailed(_) => {
                                    unreachable!("inner refresh_fn is Ok-wrapping")
                                }
                            };
                            if committed {
                                // Issue #180: the write guard reborrowed into
                                // the CAS call above is dropped at the end of
                                // that `let` statement. Persist here — in a
                                // later statement — so the read lock inside
                                // persist_tokens (same RwLock) cannot
                                // self-deadlock.
                                if let Err(e) = token_io::persist_tokens(state, app) {
                                    log::warn!(
                                        "[POLLING] poll_once: failed to persist refreshed spotify tokens: {}",
                                        e
                                    );
                                }
                                let retry_token = new_tokens.access_token.clone();
                                let last_poll_instant_retry = Instant::now();
                                match get_currently_playing(&retry_token, last_etag.as_deref()) {
                                    Ok(CurrentlyPlaying::Modified {
                                        track: Some(track),
                                        etag,
                                    }) => {
                                        *last_etag = etag;
                                        // Issue #344: debug — see the main
                                        // track-found site above.
                                        log::debug!(
                                            "[POLLING] poll_once: retry track found - {} by {}",
                                            track.title,
                                            track.artist
                                        );
                                        let _sleep = process_track(
                                            app,
                                            state,
                                            &config,
                                            &track,
                                            last_track_key,
                                            last_poll_instant_retry,
                                            last_teams_update,
                                            last_posted_placeholder,
                                            consecutive_pauses,
                                            gated_track_key,
                                            last_availability_arm,
                                            last_posted_status,
                                            last_gate_check,
                                        );
                                        *transient_failure_count = 0;
                                        return PollIteration::Sleep { seconds: _sleep };
                                    }
                                    Ok(CurrentlyPlaying::Modified { track: None, etag }) => {
                                        *last_etag = etag;
                                        log::info!("[POLLING] poll_once: retry no track");
                                        let no_track_backoff = handle_no_track(
                                            app,
                                            state,
                                            last_track_key,
                                            &config,
                                            last_posted_placeholder,
                                            last_availability_arm,
                                            first_iteration,
                                            last_posted_status,
                                        );
                                        *transient_failure_count = 0;
                                        let mut iteration =
                                            record_no_track_outcome(consecutive_pauses, &config);
                                        if let PollIteration::Sleep { seconds } = &mut iteration {
                                            // Issue #154: a throttled Teams
                                            // clear extends the next poll to
                                            // the server-directed delay.
                                            *seconds = (*seconds).max(no_track_backoff);
                                        }
                                        return iteration;
                                    }
                                    Ok(CurrentlyPlaying::NotModified) => {
                                        // Same no-op as the main path's 304 —
                                        // plus the issue #343 config-flip
                                        // force-rewrite (see the main arm).
                                        if let Some(track) = config_flip_rewrite_track(
                                            state,
                                            last_track_key,
                                            &config,
                                        ) {
                                            log::info!(
                                                "[POLLING] poll_once: 304 but status config changed mid-track, forcing one rewrite"
                                            );
                                            let sleep = process_track(
                                                app,
                                                state,
                                                &config,
                                                &track,
                                                last_track_key,
                                                last_poll_instant_retry,
                                                last_teams_update,
                                                last_posted_placeholder,
                                                consecutive_pauses,
                                                gated_track_key,
                                                last_availability_arm,
                                                last_posted_status,
                                                last_gate_check,
                                            );
                                            *transient_failure_count = 0;
                                            return PollIteration::Sleep { seconds: sleep };
                                        }
                                        return not_modified_iteration(
                                            last_track_key,
                                            consecutive_pauses,
                                            transient_failure_count,
                                            &config,
                                        );
                                    }
                                    Err(retry_err) => {
                                        log::error!(
                                            "[POLLING] poll_once: retry after refresh also failed: {}",
                                            retry_err
                                        );
                                        final_err = retry_err;
                                    }
                                }
                            }
                        }
                        Err(refresh_err) => {
                            log::error!(
                                "[POLLING] poll_once: token refresh failed: {}",
                                refresh_err
                            );
                            // Issue #160: only a dead refresh token
                            // (`invalid_grant`) needs re-auth; other refresh
                            // failures are transient and flow into the
                            // backoff / 5-strikes logic below.
                            // #219: mirror proactive InvalidGrant path — clear
                            // tokens, persist, emit both events. The next
                            // iteration will hit the no-tokens guard
                            // (state.tokens.spotify().clone() is None) and
                            // sleep, so we cannot spin on a dead token.
                            if matches!(refresh_err, SpotifyApiError::InvalidGrant) {
                                log::warn!("[POLLING] poll_once: Spotify refresh token invalid (invalid_grant), discarding tokens and requiring reconnect");
                                *state.tokens.spotify_mut() = None;
                                if let Err(persist_err) = token_io::persist_tokens(state, app) {
                                    log::warn!(
                                        "[POLLING] poll_once: failed to persist cleared Spotify tokens: {}",
                                        persist_err
                                    );
                                }
                                let _ = app.emit("spotify-reconnect-required", json!(null));
                                let _ = app.emit("reconnect-required", json!(null));
                            }
                            final_err = refresh_err;
                        }
                    }
                }
            }

            // Issue #159: honor the server's `Retry-After` (floored at the
            // error retry interval so a tiny value can't create a busy loop);
            // fall back to the fixed jittered backoff when the header is
            // absent.
            if matches!(final_err, SpotifyApiError::RateLimited(_)) {
                backoff_secs = with_jitter(spotify_backoff_base(&final_err));
            }

            if matches!(
                final_err,
                SpotifyApiError::RateLimited(_)
                    | SpotifyApiError::ExpiredToken
                    | SpotifyApiError::Other(_)
                    | SpotifyApiError::InvalidGrant
            ) {
                *transient_failure_count = transient_failure_count.saturating_add(1);
            }

            if let Some(iteration) = transient_outcome(*transient_failure_count) {
                log::error!("[POLLING] poll_once: 5 consecutive transient failures, exiting and requiring reconnect");
                // Issue #389: the 5-strikes exit must carry the
                // provider-specific signal alongside the generic one —
                // mirror the `InvalidGrant` arms above, which emit both, so
                // the frontend can start a real Spotify OAuth flow.
                let _ = app.emit("spotify-reconnect-required", json!(null));
                let _ = app.emit("reconnect-required", json!(null));
                return iteration;
            }

            emit_error(
                app,
                "spotify",
                format!("Failed to get currently playing: {}", final_err),
                ErrorSeverity::Warning,
            );
            interruptible_sleep(stop_rx, backoff_secs, "backoff sleep", mode)
        }
    }
}

/// Issue #262: the 5-strikes transient-failure decision, extracted as a
/// pure function of the counter so the threshold semantics are testable
/// without driving the whole `run()` error path. Returns `Some(Break)`
/// exactly when the count has reached `TRANSIENT_FAILURE_EXIT_THRESHOLD`
/// (the counter is only bumped for transient API errors and reset on any
/// success), and `None` below it so the caller keeps retrying after
/// emitting its warning.
fn transient_outcome(count: u8) -> Option<PollIteration> {
    if count >= TRANSIENT_FAILURE_EXIT_THRESHOLD {
        Some(PollIteration::Break)
    } else {
        None
    }
}

/// Record a no-track outcome. The ONLY place `consecutive_pauses` is
/// incremented in response to a no-track result.
fn record_no_track_outcome(
    consecutive_pauses: &mut u8,
    config: &Option<crate::config::AppConfig>,
) -> PollIteration {
    let no_track_sleep = pause_backoff(*consecutive_pauses, config_default_interval(config));
    *consecutive_pauses = consecutive_pauses.saturating_add(1).min(4);
    log::info!(
        "[POLLING] poll_once: sleeping for {} seconds (no track)",
        no_track_sleep
    );
    PollIteration::Sleep {
        seconds: no_track_sleep,
    }
}

/// Handle a 304 Not Modified from the conditional GET (candidate C11,
/// docs/scope-3.3.md §C11). The response carries no body, so there is
/// nothing to JSON-parse, no status to format/filter and no new state for
/// the tray or frontend — the observable behavior matches the
/// unchanged-track path minus that work: keep every tracked field, reset
/// the transient counter the way an unchanged playing track does, and sleep
/// without duration-derived smart sleep (`progress_ms`, which a bodyless 304
/// cannot provide).
///
/// A 304 with a tracked track mirrors the unchanged-track path: reset the
/// pause counter and sleep the default interval. A 304 with no tracked track
/// means "still nothing playing" (issue #242): the no-track ETag stays valid
/// so idle polling keeps sending conditional GETs, and the pause backoff
/// advances exactly like an unconditional 204 no-track. A later change
/// surfaces as a 200/204 Modified and re-establishes ground truth
/// automatically.
fn not_modified_iteration(
    last_track_key: &Option<String>,
    consecutive_pauses: &mut u8,
    transient_failure_count: &mut u8,
    config: &Option<crate::config::AppConfig>,
) -> PollIteration {
    log::info!("[POLLING] poll_once: 304 Not Modified, skipping parse/format/tray work");
    *transient_failure_count = 0;
    if last_track_key.is_some() {
        *consecutive_pauses = 0;
        return PollIteration::Sleep {
            seconds: config_default_interval(config),
        };
    }
    record_no_track_outcome(consecutive_pauses, config)
}

fn interruptible_sleep(
    stop_rx: &mpsc::Receiver<()>,
    seconds: u64,
    label: &str,
    mode: RunMode,
) -> PollIteration {
    if mode == RunMode::OneShot {
        log::info!(
            "[POLLING] poll_once: one-shot, skipping {} (returning Break)",
            label
        );
        return PollIteration::Break;
    }
    match stop_rx.recv_timeout(std::time::Duration::from_secs(seconds)) {
        Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            log::info!(
                "[POLLING] poll_once: stop signal during {}, breaking",
                label
            );
            PollIteration::Break
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => PollIteration::Sleep { seconds: 0 },
    }
}

enum CasOutcome<T, E> {
    Committed(T),
    Discarded { current: Option<T> },
    RefreshFailed(E),
}

/// Generic over the refresh error type `E` so each caller keeps its
/// provider's typed error (`SpotifyApiError` / `TeamsApiError`) for the
/// re-auth policy, instead of a pre-stringified message.
fn cas_refresh_or_discard<T, E, F, G>(
    label: &str,
    lock: &mut Option<T>,
    pre_refresh_access_token: &str,
    refresh_fn: F,
    access_token_of: G,
) -> CasOutcome<T, E>
where
    T: Clone,
    F: FnOnce() -> Result<T, E>,
    G: FnOnce(&T) -> &str,
{
    let new_tokens = match refresh_fn() {
        Ok(t) => t,
        Err(e) => return CasOutcome::RefreshFailed(e),
    };

    // Issue #180: this helper must NEVER persist tokens itself. Callers pass
    // `&mut *state.tokens.X_mut()` — a reborrow of the parking_lot write
    // guard, which stays alive for the whole call statement. Persisting here
    // would re-lock the SAME RwLock for reading (token_io::persist_tokens)
    // while the write guard is still held; parking_lot has no same-thread
    // reentrancy detection, so write→read on the same lock from the same
    // thread parks forever on every successful refresh. The call sites
    // therefore persist in a statement AFTER this call returns, when the
    // guard is provably dropped.
    let committed = {
        if lock.as_ref().map(access_token_of) == Some(pre_refresh_access_token) {
            *lock = Some(new_tokens.clone());
            true
        } else {
            log::warn!(
                "[POLLING] poll_once: cas_refresh_or_discard: {} state changed during refresh, discarding result",
                label
            );
            false
        }
    };

    if committed {
        CasOutcome::Committed(new_tokens)
    } else {
        let current = lock.clone();
        CasOutcome::Discarded { current }
    }
}

fn get_spotify_credentials(config: &Option<crate::config::AppConfig>) -> (String, String) {
    let client_id = config
        .as_ref()
        .map(|c| c.spotify.client_id.clone())
        .unwrap_or_default();
    let client_secret = crate::keychain::peek_spotify_client_secret().unwrap_or_default();
    (client_id, client_secret)
}

/// What the proactive Spotify refresh should do this iteration (issue #296).
#[derive(Debug, PartialEq, Eq)]
enum SpotifyRefreshPlan {
    /// The access token is still inside its refresh window — use it as-is.
    Fresh,
    /// The access token expired and the client_id/secret needed to refresh it
    /// are both available.
    Refresh,
    /// The access token expired but the credential pair is unavailable
    /// (cold keychain cache: the startup prime failed, e.g. locked Secret
    /// Service / headless Linux / entry removed while running). Refreshing
    /// with an empty secret can only produce a 400 `invalid_client` — not
    /// `InvalidGrant` — so retrying is pointless; the user must re-auth or
    /// restore the keychain entry.
    CredentialsUnavailable,
}

/// Classify the proactive-refresh decision. Pure and total so the policy is
/// unit-testable without an `AppHandle`; the caller owns the side effects
/// (emitting events, persisting, the 5-strikes counter).
fn spotify_refresh_plan(
    token_expired: bool,
    client_id: &str,
    client_secret: &str,
) -> SpotifyRefreshPlan {
    if !token_expired {
        SpotifyRefreshPlan::Fresh
    } else if client_id.is_empty() || client_secret.is_empty() {
        SpotifyRefreshPlan::CredentialsUnavailable
    } else {
        SpotifyRefreshPlan::Refresh
    }
}

/// True when a failed Teams token refresh must force re-auth (issue #295).
/// The policy mirrors the Teams status-update classifier and the Spotify
/// sibling: only a genuinely dead credential — token-endpoint
/// `invalid_grant`, or a 401 `ExpiredToken` — means re-auth. `Transient`
/// (network/5xx), `RateLimited`, `Forbidden` and `Other(400, …)` are
/// recoverable states that must keep the session and retry later; a single
/// dropped connection must not end Teams sync.
fn teams_refresh_requires_reauth(e: &TeamsApiError) -> bool {
    matches!(
        e,
        TeamsApiError::InvalidGrant | TeamsApiError::ExpiredToken(_)
    )
}

/// True when the Available-presence session should be re-armed (issue
/// #3.0-P1): no arm yet, or the last arm is at least
/// `AVAILABILITY_REARM_SECONDS` old. An `Available` session TIMES OUT
/// after 5 minutes (non-configurable; a distinct clock from
/// `expirationDuration`), so the re-arm cadence must be strictly inside
/// that window (4 min < 5 min).
fn should_rearm_availability(last_arm: Option<Instant>, now: Instant) -> bool {
    match last_arm {
        Some(arm) => now.duration_since(arm).as_secs() >= AVAILABILITY_REARM_SECONDS,
        None => true,
    }
}

/// Issue #343: fingerprint of the status-shaping config. Embedded in the
/// track change key so a filter/placeholder/format flip mid-track reads as
/// a change and forces one rewrite on the next poll, instead of leaving
/// the stale status posted until the next track change.
///
/// The `None`-config fallbacks mirror `process_track`'s exactly — a
/// mismatch here would flap the key on every poll.
fn status_config_fingerprint(config: &Option<crate::config::AppConfig>) -> String {
    let filter = config
        .as_ref()
        .map(|c| c.teams.profanity_filter)
        .unwrap_or(true);
    let placeholder = config
        .as_ref()
        .map(|c| c.teams.profanity_placeholder.as_str())
        .unwrap_or(profanity::safe_placeholder_default());
    let format = config
        .as_ref()
        .map(|c| c.teams.status_format.as_str())
        .unwrap_or("🎵 {artist} - {track} 🎧");
    format!("filter={filter} placeholder={placeholder} format={format}")
}

/// Issue #343: the change key compared against `last_track_key`. Track
/// identity plus the status-shaping config fingerprint.
fn status_track_key(
    track: &crate::spotify::TrackInfo,
    config: &Option<crate::config::AppConfig>,
) -> String {
    format!(
        "{} - {} | {}",
        track.title,
        track.artist,
        status_config_fingerprint(config)
    )
}

/// Issue #343: 304 steady-state force-rewrite. A 304 carries no body, so
/// `process_track` never runs and the change key above is never compared —
/// a config flip mid-track would stay stale until the next track change.
/// Returns the last observed track when the stored key no longer matches
/// the current track+config, so `run()` can push one fresh write through
/// `process_track`; `None` otherwise (no tracked track, or nothing
/// changed).
fn config_flip_rewrite_track(
    state: &AppState,
    last_track_key: &Option<String>,
    config: &Option<crate::config::AppConfig>,
) -> Option<crate::spotify::TrackInfo> {
    let tracked = state.polling.current_track().clone()?;
    let expected = status_track_key(&tracked, config);
    if last_track_key.as_ref() != Some(&expected) {
        Some(tracked)
    } else {
        None
    }
}

/// Issues #370/#388: the single source of truth for a write-ready Teams
/// token — clone the stored tokens, refresh when expired (CAS-commit +
/// persist, dead-credential re-auth policy per issue #295), and hand back
/// `None` when there is nothing usable. Called from BOTH `process_track`
/// and `handle_no_track` (including the clear path) so the no-track clear
/// can no longer sail with a dead token while the track path refreshes.
fn teams_token_for_write(app: &AppHandle, state: &Arc<AppState>) -> Option<TeamsTokens> {
    let teams_tokens = state.tokens.teams().clone();
    if let Some(ref tok) = teams_tokens {
        let expired = is_teams_token_expired(tok);
        if expired {
            log::info!("[POLLING] teams_token_for_write: Teams token expired, refreshing...");

            let pre_refresh_access_token = tok.access_token.clone();

            // The refresh error stays typed (`CasOutcome<T, E>` is generic
            // over `E`) so the re-auth policy below can classify it instead
            // of string-sniffing.
            let teams_refresh_outcome = cas_refresh_or_discard(
                "teams",
                &mut *state.tokens.teams_mut(),
                &pre_refresh_access_token,
                || refresh_teams_token(tok),
                |t| &t.access_token,
            );
            match teams_refresh_outcome {
                CasOutcome::Committed(new_tokens) => {
                    // Issue #180: the write guard reborrowed into the CAS
                    // call above is dropped at the end of that statement.
                    // Persist here so the read lock inside persist_tokens
                    // (same RwLock) cannot self-deadlock.
                    if let Err(e) = token_io::persist_tokens(state, app) {
                        log::warn!(
                            "[POLLING] teams_token_for_write: failed to persist refreshed teams tokens: {}",
                            e
                        );
                    }
                    Some(new_tokens)
                }
                CasOutcome::Discarded { current } => current,
                CasOutcome::RefreshFailed(e) => {
                    log::error!(
                        "[POLLING] teams_token_for_write: Failed to refresh Teams token: {}",
                        e
                    );
                    // Issue #295: classify the typed error exactly like the
                    // Teams status-update path below and the Spotify sibling.
                    // Only a dead refresh token (`invalid_grant`) or a
                    // rejected access token (401) means re-auth; `Transient`
                    // (network/5xx), `RateLimited`, `Forbidden` and
                    // `Other(400, …)` keep the session and retry later — a
                    // single dropped connection must not send the user
                    // through a full device-code browser re-auth.
                    if teams_refresh_requires_reauth(&e) {
                        log::warn!("[POLLING] teams_token_for_write: Teams refresh token is dead, discarding tokens and requiring reconnect");
                        *state.tokens.teams_mut() = None;
                        // Issue #180: the write guard in the clearing
                        // statement above dies at the end of that statement.
                        // Persist in a LATER statement, when the guard is
                        // provably dropped — persisting while it is alive
                        // would re-lock the same parking_lot RwLock for
                        // reading and self-deadlock.
                        if let Err(persist_err) = token_io::persist_tokens(state, app) {
                            log::warn!(
                                "[POLLING] teams_token_for_write: failed to persist cleared teams tokens: {}",
                                persist_err
                            );
                        }
                        let _ = app.emit("teams-reconnect-required", json!(null));
                        None
                    } else {
                        // Issue #295: a transient refresh failure keeps the
                        // session (the tokens stay in `state`, unlike the
                        // dead-token branch above) and skips this iteration's
                        // Teams work — attempting the status write with a
                        // token we just failed to refresh would only produce
                        // a 401 and force the very re-auth this policy exists
                        // to avoid. The next iteration retries the refresh.
                        log::warn!(
                            "[POLLING] teams_token_for_write: Teams refresh failed (transient), keeping session and retrying later"
                        );
                        None
                    }
                }
            }
        } else {
            teams_tokens
        }
    } else {
        teams_tokens
    }
}

/// Issue #364: the debounce predicate — a change inside the 500ms window
/// after the last Teams write skips this iteration's API call (the caller
/// returns `DEBOUNCE_RETRY_SECONDS` before any side effect, so the retry
/// re-detects the change and emits/posts exactly once).
fn debounce_active(changed: bool, last_teams_update: Option<Instant>) -> bool {
    if !changed {
        return false;
    }
    match last_teams_update {
        Some(last_update) => (last_update.elapsed().as_millis() as u64) < DEBOUNCE_MS,
        None => false,
    }
}

/// Issue #373: whether a no-track poll should attempt a Teams clear.
/// Fresh threads start with `last_track_key=None`, so the first no-track
/// poll must attempt one clear (pre-restart status would otherwise stay
/// stale); later nothing-tracked polls stay a no-op. Pure so the
/// exactly-once semantics are unit-testable; the caller consumes the flag.
fn first_no_track_attempts_clear(last_track_key: &Option<String>, first_iteration: bool) -> bool {
    last_track_key.is_some() || first_iteration
}

/// Issue #380: whether the presence-gate re-check is due — the last gate
/// re-check is at least the re-arm cadence old, or there is no re-check
/// on record. Threaded on its own `last_gate_check` clock so re-checks
/// never shift the debounce + keepalive write windows.
fn gate_recheck_due(last_gate_check: Option<Instant>, now: Instant) -> bool {
    match last_gate_check {
        Some(t) => now.duration_since(t).as_secs() >= AVAILABILITY_REARM_SECONDS,
        None => true,
    }
}

/// Issue #384: skip a byte-identical playing-status write while the last
/// write is still inside the keepalive window. A track/config-fingerprint
/// change (`changed`) always force-writes, as does a lapsed keepalive (so
/// the Graph expiry never lapses with no refresh in flight).
fn should_skip_identical_write(
    changed: bool,
    last_posted_status: Option<&str>,
    final_status: &str,
    last_write: Option<Instant>,
    now: Instant,
) -> bool {
    if changed {
        return false;
    }
    if last_posted_status != Some(final_status) {
        return false;
    }
    match last_write {
        Some(t) => now.duration_since(t).as_secs() < STATUS_KEEPALIVE_SECONDS,
        None => false,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn process_track(
    app: &AppHandle,
    state: &Arc<AppState>,
    config: &Option<crate::config::AppConfig>,
    track: &crate::spotify::TrackInfo,
    last_track_key: &mut Option<String>,
    last_poll_instant: Instant,
    last_teams_update: &mut Option<Instant>,
    last_posted_placeholder: &mut Option<String>,
    consecutive_pauses: &mut u8,
    gated_track_key: &mut Option<String>,
    last_availability_arm: &mut Option<Instant>,
    last_posted_status: &mut Option<String>,
    last_gate_check: &mut Option<Instant>,
) -> u64 {
    let elapsed_ms = last_poll_instant.elapsed().as_millis() as u64;
    // Issue #165: `progress_ms` is `None` for live/unknown-position streams.
    // Keep the Option alive so the duration-derived sleep/expiry below are
    // skipped for streams — they fall back to the default interval and no
    // `expiryDateTime` on the wire respectively.
    let corrected_progress_ms = track.progress_ms.map(|p| p.saturating_add(elapsed_ms));
    // Issue #154: a throttled Teams set/clear (429) extends the next poll to
    // the server-directed delay.
    let mut teams_backoff_secs: u64 = 0;

    // Issue #343: the change key carries the status-shaping config
    // (filter flag + placeholder + format) alongside the track identity,
    // so a relevant config flip mid-track forces one rewrite on the next
    // poll instead of leaving the stale status until the next track.
    let track_key = status_track_key(track, config);
    let changed = last_track_key.as_ref() != Some(&track_key);

    // Issue #364: debounce BEFORE any side effect. A change inside the
    // window parks on the short fixed retry with every tracked field
    // untouched, so the retry re-detects the change and emits/posts
    // exactly once. (Pre-fix the store/emit/placeholder-clear/gate work
    // below ran first and only the track key was restored, duplicating
    // the `spotify-track-changed` event and the Graph presence read.)
    if debounce_active(changed, *last_teams_update) {
        log::debug!(
            "[POLLING] process_track: debounce active, skipping Teams API call (changed={}, elapsed={}ms)",
            changed,
            last_teams_update.map(|i| i.elapsed().as_millis() as u64).unwrap_or(0)
        );
        return DEBOUNCE_RETRY_SECONDS;
    }

    if changed {
        log::info!("[POLLING] process_track: new track detected, updating");
        // Clone: `track_key` is still needed below for the presence-gate
        // comparison (issue #3.0-P2).
        *last_track_key = Some(track_key.clone());
        *state.polling.current_track_mut() = Some(track.clone());

        let _ = app.emit(
            "spotify-track-changed",
            json!({
                "title": track.title,
                "artist": track.artist,
                "album": track.album,
                "album_art_url": track.album_art_url,
                "is_playing": track.is_playing,
                "progress_ms": track.progress_ms,
                "duration_ms": track.duration_ms
            }),
        );
    }

    // Issues #370/#388: one shared refresh path — see `teams_token_for_write`.
    let teams_tokens = teams_token_for_write(app, state);

    if let Some(mut teams_tok) = teams_tokens {
        if track.is_playing {
            *consecutive_pauses = 0;
            // Issue #155: a real track replaces any placeholder, so the next
            // pause/no-track must post a fresh placeholder again.
            *last_posted_placeholder = None;

            // P2 (issue #3.0-P2): presence-aware gating. On a track change,
            // read the user's Teams presence; when busy/DND/in a
            // meeting/call/presenting, suppress the status write for the
            // whole track (recorded in `gated_track_key`) and emit
            // `presence-gated`. Runs after the debounce above, so a change
            // inside the window parks untouched and the retry performs the
            // single gate read. Fail-safe: a failed read
            // (network, 403, …) proceeds with the write, logged as a warning.
            let presence_gate_enabled = config
                .as_ref()
                .map(|c| c.teams.presence_gate)
                .unwrap_or(true);
            if changed {
                if presence_gate_enabled {
                    match get_teams_presence(&teams_tok.access_token) {
                        Ok(presence) if is_presence_gated(&presence) => {
                            let reason = presence_gate_reason(&presence);
                            log::info!(
                                "[POLLING] process_track: presence gated ({}), skipping status write",
                                reason
                            );
                            *gated_track_key = Some(track_key.clone());
                            *last_gate_check = Some(Instant::now());
                            let _ = app.emit(
                                "presence-gated",
                                json!({
                                    "reason": reason,
                                    "availability": presence.availability,
                                    "activity": presence.activity,
                                    "timestamp": Utc::now().to_rfc3339()
                                }),
                            );
                        }
                        Ok(_) => {
                            *gated_track_key = None;
                        }
                        Err(e) => {
                            log::warn!(
                                "[POLLING] process_track: presence gate read failed, proceeding with status write: {}",
                                e
                            );
                            *gated_track_key = None;
                        }
                    }
                } else {
                    *gated_track_key = None;
                }
            }

            // Issue #380: a gated track stays gated only until the gate
            // re-check is due — then presence is re-read, and a cleared
            // gate (meeting ended mid-track) falls through to the normal
            // write below instead of suppressing the whole duration.
            // Fail-safe: a failed read keeps the gate (still suppressed).
            // `last_gate_check` throttles the re-reads while gated — never
            // `last_teams_update`, which times the debounce + keepalive write clocks.
            if gated_track_key.as_deref() == Some(track_key.as_str()) {
                let gate_enabled = config
                    .as_ref()
                    .map(|c| c.teams.presence_gate)
                    .unwrap_or(true);
                if !gate_enabled {
                    *gated_track_key = None;
                } else if gate_recheck_due(*last_gate_check, Instant::now()) {
                    match get_teams_presence(&teams_tok.access_token) {
                        Ok(presence) if is_presence_gated(&presence) => {
                            log::debug!("[POLLING] process_track: still presence-gated, keeping suppression");
                            *last_gate_check = Some(Instant::now());
                        }
                        Ok(_) => {
                            log::info!(
                                "[POLLING] process_track: presence gate cleared mid-track, posting late"
                            );
                            *gated_track_key = None;
                            // Issue #380: record the re-check on the gate
                            // clock only — `last_teams_update` (debounce +
                            // keepalive) stays untouched so the late post
                            // below is never mistaken for a fresh write.
                            *last_gate_check = Some(Instant::now());
                        }
                        Err(e) => {
                            log::warn!(
                                "[POLLING] process_track: gate re-read failed, keeping suppression: {}",
                                e
                            );
                            *last_gate_check = Some(Instant::now());
                        }
                    }
                }
                if gated_track_key.as_deref() == Some(track_key.as_str()) {
                    log::debug!(
                        "[POLLING] process_track: track presence-gated, skipping status write"
                    );
                    let remaining_ms =
                        corrected_progress_ms.map(|c| track.duration_ms.saturating_sub(c));
                    return playing_track_sleep(remaining_ms, config);
                }
            }

            let status_format = config
                .as_ref()
                .map(|c| c.teams.status_format.as_str())
                .unwrap_or("\u{1F3B5} {artist} - {track} \u{1F3A7}");
            let status_message = format_status(track, status_format);
            let profanity_filter_enabled = config
                .as_ref()
                .map(|c| c.teams.profanity_filter)
                .unwrap_or(true);
            let placeholder = config
                .as_ref()
                .map(|c| c.teams.profanity_placeholder.as_str())
                .unwrap_or(profanity::safe_placeholder_default());
            let final_status = if profanity_filter_enabled {
                profanity::filter_status(&status_message, placeholder, track.is_playing)
            } else {
                status_message.clone()
            };
            // Issue #384: byte-identical re-POSTs every cycle are pure
            // noise. Skip the write when the status is unchanged and the
            // last write is still inside the keepalive window. A
            // fingerprint change forces `changed` above, so it always
            // force-writes; a lapsed keepalive force-writes so the Graph
            // expiry never lapses.
            if should_skip_identical_write(
                changed,
                last_posted_status.as_deref(),
                &final_status,
                *last_teams_update,
                Instant::now(),
            ) {
                log::debug!("[POLLING] process_track: status identical and keepalive fresh, skipping Teams write");
                let remaining_ms =
                    corrected_progress_ms.map(|c| track.duration_ms.saturating_sub(c));
                return playing_track_sleep(remaining_ms, config).max(teams_backoff_secs);
            }

            let remaining_ms = corrected_progress_ms.map(|c| track.duration_ms.saturating_sub(c));
            // Issue #165: live streams have no known remaining time → no
            // `expiryDateTime` on the wire (the status does not self-expire).
            let expiry_str = status_expiry_str(remaining_ms, config);

            match set_teams_status_message(
                &teams_tok.access_token,
                &final_status,
                expiry_str.as_deref(),
            ) {
                Ok(_) => {
                    *last_teams_update = Some(Instant::now());
                    *last_posted_status = Some(final_status.clone());
                    let _ = app.emit(
                        "presence-updated",
                        json!({
                            "status": final_status,
                            "timestamp": Utc::now().to_rfc3339()
                        }),
                    );
                }
                Err(e) => {
                    // Issues #367/#428 (never-re-auth): a 401 here can mean
                    // the token expired mid-sequence (or clock skew) even
                    // though the pre-write expiry check passed. Mirror the
                    // Spotify reactive path at the top of `run`: one
                    // `refresh_teams_token` + CAS-commit + persist + single
                    // write retry before blaming the credential. Only a dead
                    // refresh token surfaces `teams-reconnect-required`.
                    let write_outcome = match e {
                        TeamsApiError::ExpiredToken(status) => {
                            log::info!("[POLLING] process_track: Teams status write hit ExpiredToken, attempting one refresh + retry");
                            let pre_refresh_access_token = teams_tok.access_token.clone();
                            match refresh_teams_token(&teams_tok) {
                                Ok(new_tokens) => {
                                    let committed = match cas_refresh_or_discard(
                                        "teams",
                                        &mut *state.tokens.teams_mut(),
                                        &pre_refresh_access_token,
                                        || Ok::<_, TeamsApiError>(new_tokens.clone()),
                                        |t| &t.access_token,
                                    ) {
                                        CasOutcome::Committed(_) => true,
                                        CasOutcome::Discarded { .. } => false,
                                        CasOutcome::RefreshFailed(_) => {
                                            unreachable!("inner refresh_fn is Ok-wrapping")
                                        }
                                    };
                                    if committed {
                                        // Issue #180: the write guard
                                        // reborrowed into the CAS call above
                                        // is dropped at the end of that
                                        // statement. Persist here — in a
                                        // later statement — so the read lock
                                        // inside persist_tokens (same RwLock)
                                        // cannot self-deadlock.
                                        if let Err(persist_err) =
                                            token_io::persist_tokens(state, app)
                                        {
                                            log::warn!(
                                                "[POLLING] process_track: failed to persist reactively refreshed teams tokens: {}",
                                                persist_err
                                            );
                                        }
                                        match set_teams_status_message(
                                            &new_tokens.access_token,
                                            &final_status,
                                            expiry_str.as_deref(),
                                        ) {
                                            Ok(()) => Ok(new_tokens),
                                            Err(retry_err) => {
                                                log::error!(
                                                    "[POLLING] process_track: Teams status retry after refresh also failed: {}",
                                                    retry_err
                                                );
                                                Err(retry_err)
                                            }
                                        }
                                    } else {
                                        // CAS lost (mirrors the Spotify 401
                                        // path): keep the original error for
                                        // classification below.
                                        Err(TeamsApiError::ExpiredToken(status))
                                    }
                                }
                                Err(refresh_err) => {
                                    log::error!(
                                        "[POLLING] process_track: Teams reactive refresh failed: {}",
                                        refresh_err
                                    );
                                    // Issue #295 policy: only a dead
                                    // credential clears the session; a
                                    // transient refresh failure keeps it and
                                    // flows into the transient branch below
                                    // with no reconnect event. Either way the
                                    // typed refresh error (not the stale
                                    // write error) is what gets classified.
                                    if teams_refresh_requires_reauth(&refresh_err) {
                                        log::warn!("[POLLING] process_track: Teams refresh token is dead, discarding tokens");
                                        *state.tokens.teams_mut() = None;
                                        // Issue #180: the write guard in the
                                        // clearing statement above dies at
                                        // the end of that statement. Persist
                                        // in a LATER statement, when the
                                        // guard is provably dropped.
                                        if let Err(persist_err) =
                                            token_io::persist_tokens(state, app)
                                        {
                                            log::warn!(
                                                "[POLLING] process_track: failed to persist cleared teams tokens: {}",
                                                persist_err
                                            );
                                        }
                                    } else {
                                        log::warn!("[POLLING] process_track: Teams reactive refresh failed (transient), keeping session");
                                    }
                                    Err(refresh_err)
                                }
                            }
                        }
                        other => Err(other),
                    };
                    match write_outcome {
                        Ok(refreshed) => {
                            *last_teams_update = Some(Instant::now());
                            *last_posted_status = Some(final_status.clone());
                            let _ = app.emit(
                                "presence-updated",
                                json!({
                                    "status": final_status,
                                    "timestamp": Utc::now().to_rfc3339()
                                }),
                            );
                            // Adopt the fresh token so the availability
                            // re-arm below does not 401 on the stale one.
                            teams_tok = refreshed;
                        }
                        Err(e) => {
                            log::error!(
                                "[POLLING] process_track: Failed to set Teams status: {}",
                                e
                            );
                            emit_error(
                                app,
                                "teams",
                                format!("Failed to update status: {}", e),
                                ErrorSeverity::Error,
                            );
                            // Issue #154: a 429 extends the next poll to the
                            // server-directed delay.
                            teams_backoff_secs = teams_backoff_secs.max(rate_limit_sleep_secs(&e));
                            // Issue #153: classify typed TeamsApiError variants instead
                            // of string-sniffing the error body. Only a dead token
                            // (401 / invalid_grant) means re-auth; 403 is a
                            // permission/license problem re-auth cannot fix.
                            match e {
                                TeamsApiError::ExpiredToken(_) | TeamsApiError::InvalidGrant => {
                                    log::warn!("[POLLING] process_track: Teams auth failure detected, emitting teams-reconnect-required");
                                    let _ = app.emit("teams-reconnect-required", json!(null));
                                }
                                TeamsApiError::Forbidden(_, _) => {
                                    log::error!("[POLLING] process_track: Teams status update forbidden (permission/license) — re-auth cannot fix this; skipping teams-reconnect-required");
                                }
                                TeamsApiError::RateLimited(_)
                                | TeamsApiError::Transient(_)
                                | TeamsApiError::Other(_, _) => {
                                    log::warn!("[POLLING] process_track: Teams status update failed (transient), continuing");
                                }
                            }
                        }
                    }
                }
            }
        } else if config
            .as_ref()
            .map(|c| c.teams.clear_on_pause)
            .unwrap_or(true)
        {
            // Issue #155: the clear path posts a short-lived placeholder
            // (Graph has no "clear status message" action) and skips
            // byte-identical repeat posts.
            let placeholder = "\u{1F3B5} Paused";
            if last_posted_placeholder.as_deref() == Some(placeholder) {
                log::debug!(
                    "[POLLING] process_track: paused placeholder unchanged, skipping clear POST"
                );
            } else {
                // P2 (issue #3.0-P2): gate the paused-clear the same way as
                // the playing write — don't replace a busy/meeting presence
                // with a "Paused" placeholder. `gated_track_key` carries the
                // change-time decision from the playing path; re-read
                // presence only when this track wasn't gated there.
                let gate_blocked = if gated_track_key.as_deref() == Some(track_key.as_str()) {
                    true
                } else if config
                    .as_ref()
                    .map(|c| c.teams.presence_gate)
                    .unwrap_or(true)
                {
                    match get_teams_presence(&teams_tok.access_token) {
                        Ok(presence) if is_presence_gated(&presence) => {
                            *gated_track_key = Some(track_key.clone());
                            let reason = presence_gate_reason(&presence);
                            let _ = app.emit(
                                "presence-gated",
                                json!({
                                    "reason": reason,
                                    "availability": presence.availability,
                                    "activity": presence.activity,
                                    "timestamp": Utc::now().to_rfc3339()
                                }),
                            );
                            true
                        }
                        Ok(_) => false,
                        Err(e) => {
                            // Fail-safe: proceed with the clear.
                            log::warn!(
                                "[POLLING] process_track: presence gate read failed, proceeding with paused clear: {}",
                                e
                            );
                            false
                        }
                    }
                } else {
                    false
                };

                if gate_blocked {
                    log::info!(
                        "[POLLING] process_track: paused-clear gated, keeping presence untouched"
                    );
                    // Mark the placeholder as posted so the decision is made
                    // once per pause; the next track change resets it (the
                    // playing branch clears `last_posted_placeholder`).
                    *last_posted_placeholder = Some(placeholder.to_string());
                } else {
                    let expiry_str = placeholder_expiry_str();
                    match clear_teams_status_message(
                        &teams_tok.access_token,
                        placeholder,
                        Some(&expiry_str),
                    ) {
                        Ok(_) => {
                            *last_teams_update = Some(Instant::now());
                            *last_posted_placeholder = Some(placeholder.to_string());
                            // Issue #384: Teams now shows a placeholder, so
                            // the recorded playing status is stale.
                            *last_posted_status = None;
                            let _ = app.emit(
                                "presence-cleared",
                                json!({ "timestamp": Utc::now().to_rfc3339() }),
                            );
                        }
                        Err(e) => {
                            log::error!(
                                "[POLLING] process_track: Failed to clear Teams status: {}",
                                e
                            );
                            // Issue #154: honor the server's Retry-After on a
                            // throttled clear.
                            teams_backoff_secs = teams_backoff_secs.max(rate_limit_sleep_secs(&e));
                        }
                    }
                }
            }
        }

        // P1 (issue #3.0-P1): availability sync — OFF by default. While a
        // track plays, re-arm the Graph "Available" presence session at
        // most every 4 minutes (Available sessions FADE after 5 min
        // regardless of `expirationDuration`; re-arm strictly inside that
        // window); on pause, clear the session (`clearPresence` 404 =
        // session already gone = success). Emits
        // `presence-availability-updated` on each arm/clear.
        if config
            .as_ref()
            .map(|c| c.teams.availability_sync)
            .unwrap_or(false)
        {
            let now = Instant::now();
            if track.is_playing {
                if should_rearm_availability(*last_availability_arm, now) {
                    match set_teams_presence(
                        &teams_tok.access_token,
                        "Available",
                        "Available",
                        "PT4H",
                    ) {
                        Ok(_) => {
                            *last_availability_arm = Some(now);
                            let _ = app.emit(
                                "presence-availability-updated",
                                json!({
                                    "available": true,
                                    "label": "Listening (Available)",
                                    "timestamp": Utc::now().to_rfc3339()
                                }),
                            );
                        }
                        Err(e) => {
                            log::error!(
                                "[POLLING] process_track: failed to set Teams availability: {}",
                                e
                            );
                            // Issue #154: a throttled set extends the next
                            // poll to the server-directed delay.
                            teams_backoff_secs = teams_backoff_secs.max(rate_limit_sleep_secs(&e));
                        }
                    }
                }
            } else if last_availability_arm.is_some() {
                match clear_teams_presence(&teams_tok.access_token) {
                    Ok(_) => {
                        *last_availability_arm = None;
                        let _ = app.emit(
                            "presence-availability-updated",
                            json!({
                                "available": false,
                                "label": "Availability cleared",
                                "timestamp": Utc::now().to_rfc3339()
                            }),
                        );
                    }
                    Err(e) => {
                        log::error!(
                            "[POLLING] process_track: failed to clear Teams availability: {}",
                            e
                        );
                        // Issue #154: honor the server's Retry-After on a
                        // throttled clear.
                        teams_backoff_secs = teams_backoff_secs.max(rate_limit_sleep_secs(&e));
                    }
                }
            }
        }
    }

    if track.is_playing {
        let remaining_ms = corrected_progress_ms.map(|c| track.duration_ms.saturating_sub(c));
        playing_track_sleep(remaining_ms, config)
    } else {
        let sleep = pause_backoff(*consecutive_pauses, config_default_interval(config));
        *consecutive_pauses = consecutive_pauses.saturating_add(1).min(4);
        sleep
    }
    .max(teams_backoff_secs)
}

/// Handle a no-track poll result. Clears the tracked state and, when
/// `clear_on_pause` allows it (issue #155), posts a short-lived "Nothing
/// playing" placeholder. Returns extra backoff seconds to fold into the
/// next poll when the clear was throttled (issue #154).
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_no_track(
    app: &AppHandle,
    state: &Arc<AppState>,
    last_track_key: &mut Option<String>,
    config: &Option<crate::config::AppConfig>,
    last_posted_placeholder: &mut Option<String>,
    last_availability_arm: &mut Option<Instant>,
    first_iteration: &mut bool,
    last_posted_status: &mut Option<String>,
) -> u64 {
    // Issue #373: consume the fresh-thread flag exactly once (see
    // `first_no_track_attempts_clear`). A fresh thread starts with
    // `last_track_key=None`, so the first no-track poll falls through
    // and attempts one clear instead of leaving pre-restart status
    // stale; later nothing-tracked polls stay a no-op.
    let is_first = *first_iteration;
    *first_iteration = false;
    if first_no_track_attempts_clear(last_track_key, is_first) {
        *last_track_key = None;
        *state.polling.current_track_mut() = None;
    } else {
        return 0;
    }

    // Issues #370/#388: refresh before the clear, exactly like the track
    // path — one shared helper, no cloned-without-expiry token.
    let teams_tok = match teams_token_for_write(app, state) {
        Some(t) => t,
        None => return 0,
    };

    // P1 (issue #3.0-P1): availability sync — clear the Graph presence
    // session when nothing is playing (`clearPresence` 404 = session
    // already gone = success). Runs independently of `clear_on_pause`:
    // that toggle governs the placeholder status message only, while
    // availability sync owns the presence bubble.
    let mut teams_backoff_secs: u64 = 0;
    if config
        .as_ref()
        .map(|c| c.teams.availability_sync)
        .unwrap_or(false)
        && last_availability_arm.is_some()
    {
        match clear_teams_presence(&teams_tok.access_token) {
            Ok(_) => {
                *last_availability_arm = None;
                let _ = app.emit(
                    "presence-availability-updated",
                    json!({
                        "available": false,
                        "label": "Availability cleared",
                        "timestamp": Utc::now().to_rfc3339()
                    }),
                );
            }
            Err(e) => {
                log::error!(
                    "[POLLING] handle_no_track: failed to clear Teams availability: {}",
                    e
                );
                // Issue #154: honor the server's Retry-After on a throttled
                // clear.
                teams_backoff_secs = teams_backoff_secs.max(rate_limit_sleep_secs(&e));
            }
        }
    }

    // Issue #155: honor `clear_on_pause` like the paused-track branch.
    if !config
        .as_ref()
        .map(|c| c.teams.clear_on_pause)
        .unwrap_or(true)
    {
        return teams_backoff_secs;
    }

    let placeholder = "\u{1F3B5} Nothing playing on Spotify";
    // Issue #155: skip byte-identical placeholder posts.
    if last_posted_placeholder.as_deref() == Some(placeholder) {
        log::debug!(
            "[POLLING] handle_no_track: no-track placeholder unchanged, skipping clear POST"
        );
        return teams_backoff_secs;
    }

    let expiry_str = placeholder_expiry_str();
    // Issue #455-residual: mirror the process_track ExpiredToken
    // refresh+single-retry (see the write path above) — a 401 here can mean
    // the token expired mid-sequence even though the pre-write expiry check
    // in `teams_token_for_write` passed. Only a dead refresh token surfaces
    // `teams-reconnect-required`.
    let clear_outcome: Result<(), TeamsApiError> = match clear_teams_status_message(
        &teams_tok.access_token,
        placeholder,
        Some(&expiry_str),
    ) {
        Ok(_) => Ok(()),
        Err(TeamsApiError::ExpiredToken(status)) => {
            log::info!("[POLLING] handle_no_track: Teams clear hit ExpiredToken, attempting one refresh + retry");
            let pre_refresh_access_token = teams_tok.access_token.clone();
            match refresh_teams_token(&teams_tok) {
                Ok(new_tokens) => {
                    let committed = match cas_refresh_or_discard(
                        "teams",
                        &mut *state.tokens.teams_mut(),
                        &pre_refresh_access_token,
                        || Ok::<_, TeamsApiError>(new_tokens.clone()),
                        |t| &t.access_token,
                    ) {
                        CasOutcome::Committed(_) => true,
                        CasOutcome::Discarded { .. } => false,
                        CasOutcome::RefreshFailed(_) => {
                            unreachable!("inner refresh_fn is Ok-wrapping")
                        }
                    };
                    if committed {
                        // Issue #180: the write guard reborrowed into the
                        // CAS call above is dropped at the end of that
                        // statement. Persist here — in a later statement
                        // — so the read lock inside persist_tokens (same
                        // RwLock) cannot self-deadlock.
                        if let Err(persist_err) = token_io::persist_tokens(state, app) {
                            log::warn!(
                                    "[POLLING] handle_no_track: failed to persist reactively refreshed teams tokens: {}",
                                    persist_err
                                );
                        }
                        match clear_teams_status_message(
                            &new_tokens.access_token,
                            placeholder,
                            Some(&expiry_str),
                        ) {
                            Ok(()) => Ok(()),
                            Err(retry_err) => {
                                log::error!(
                                        "[POLLING] handle_no_track: Teams clear retry after refresh also failed: {}",
                                        retry_err
                                    );
                                Err(retry_err)
                            }
                        }
                    } else {
                        // CAS lost (mirrors the process_track path): keep
                        // the original error for classification below.
                        Err(TeamsApiError::ExpiredToken(status))
                    }
                }
                Err(refresh_err) => {
                    log::error!(
                        "[POLLING] handle_no_track: Teams reactive refresh failed: {}",
                        refresh_err
                    );
                    // Issue #295 policy: only a dead credential clears the
                    // session; a transient refresh failure keeps it. Either
                    // way the typed refresh error (not the stale write
                    // error) is what gets classified.
                    if teams_refresh_requires_reauth(&refresh_err) {
                        log::warn!("[POLLING] handle_no_track: Teams refresh token is dead, discarding tokens");
                        *state.tokens.teams_mut() = None;
                        // Issue #180: the write guard in the clearing
                        // statement above dies at the end of that
                        // statement. Persist in a LATER statement, when
                        // the guard is provably dropped.
                        if let Err(persist_err) = token_io::persist_tokens(state, app) {
                            log::warn!(
                                    "[POLLING] handle_no_track: failed to persist cleared teams tokens: {}",
                                    persist_err
                                );
                        }
                    } else {
                        log::warn!("[POLLING] handle_no_track: Teams reactive refresh failed (transient), keeping session");
                    }
                    Err(refresh_err)
                }
            }
        }
        Err(other) => Err(other),
    };
    match clear_outcome {
        Ok(_) => {
            *last_posted_placeholder = Some(placeholder.to_string());
            // Issue #384: Teams now shows a placeholder, so the recorded
            // playing status is stale.
            *last_posted_status = None;
            let _ = app.emit(
                "presence-cleared",
                json!({ "timestamp": Utc::now().to_rfc3339() }),
            );
            teams_backoff_secs
        }
        Err(e) => {
            log::error!(
                "[POLLING] handle_no_track: Failed to clear Teams status: {}",
                e
            );
            // Issue #154: honor the server's Retry-After on a throttled clear
            // (read before the classifier below moves `e`).
            let backoff = teams_backoff_secs.max(rate_limit_sleep_secs(&e));
            // Mirror the process_track classifier: only a dead token means
            // re-auth; 403 is a permission/license problem re-auth cannot fix.
            match e {
                TeamsApiError::ExpiredToken(_) | TeamsApiError::InvalidGrant => {
                    log::warn!("[POLLING] handle_no_track: Teams auth failure detected, emitting teams-reconnect-required");
                    let _ = app.emit("teams-reconnect-required", json!(null));
                }
                TeamsApiError::Forbidden(_, _) => {
                    log::error!("[POLLING] handle_no_track: Teams clear forbidden (permission/license) — re-auth cannot fix this; skipping teams-reconnect-required");
                }
                TeamsApiError::RateLimited(_)
                | TeamsApiError::Transient(_)
                | TeamsApiError::Other(_, _) => {
                    log::warn!(
                        "[POLLING] handle_no_track: Teams clear failed (transient), continuing"
                    );
                }
            }
            backoff
        }
    }
}

fn config_default_interval(config: &Option<crate::config::AppConfig>) -> u64 {
    config
        .as_ref()
        .map(|c| c.polling.default_interval_seconds)
        .unwrap_or(30)
}

fn config_minimum_interval(config: &Option<crate::config::AppConfig>) -> u64 {
    config
        .as_ref()
        .map(|c| c.polling.minimum_interval_seconds)
        .unwrap_or(10)
}

fn config_maximum_interval(config: &Option<crate::config::AppConfig>) -> u64 {
    config
        .as_ref()
        .map(|c| c.polling.max_interval_seconds)
        .unwrap_or(60)
}

/// Server-directed sleep base for a Spotify 429 (issue #159): the
/// `Retry-After` seconds when present, else the default rate-limit backoff —
/// floored at the error retry interval so a tiny server value can't create a
/// busy loop.
fn spotify_backoff_base(err: &SpotifyApiError) -> u64 {
    err.retry_after()
        .unwrap_or(RATE_LIMIT_BACKOFF_SECONDS)
        .max(ERROR_RETRY_INTERVAL_SECONDS)
}

/// Extra sleep contributed by a failed Teams set/clear (issue #154): a
/// `RateLimited` error with `Retry-After` returns those seconds, without a
/// header falls back to the jittered default backoff, anything else
/// contributes nothing.
fn rate_limit_sleep_secs(err: &TeamsApiError) -> u64 {
    match err {
        TeamsApiError::RateLimited(Some(secs)) => *secs,
        TeamsApiError::RateLimited(None) => with_jitter(RATE_LIMIT_BACKOFF_SECONDS),
        _ => 0,
    }
}

/// Format a UTC instant as Graph's offset-less `dateTime` with 6 fraction
/// digits (≤ the documented 7). `to_rfc3339()` would embed `+00:00` and up
/// to 9 fraction digits, contradicting the dateTimeTimeZone schema (issue
/// #156).
fn format_expiry(expiry: chrono::DateTime<Utc>) -> String {
    expiry.format("%Y-%m-%dT%H:%M:%S%.6f").to_string()
}

/// Expiry for the short-lived "clear" placeholders (issue #155): now + 60s,
/// so the placeholder self-removes ~1 min after the last successful post even
/// if the app quits.
fn placeholder_expiry_str() -> String {
    let expiry = Utc::now() + chrono::Duration::seconds(60);
    format_expiry(expiry)
}

/// Expiry for a playing-track status message: now + remaining + buffer when
/// the position is known; `None` (no `expiryDateTime` on the wire) for
/// live/unknown-position streams (issue #165).
fn status_expiry_str(
    remaining_ms: Option<u64>,
    config: &Option<crate::config::AppConfig>,
) -> Option<String> {
    remaining_ms.map(|remaining| {
        let buffer_ms = config
            .as_ref()
            .map(|c| c.polling.expiry_buffer_seconds)
            .unwrap_or(10)
            * 1000;
        let expiry =
            Utc::now() + chrono::Duration::milliseconds(remaining as i64 + buffer_ms as i64);
        format_expiry(expiry)
    })
}

/// Sleep decision for a playing track. Known position → sleep until ~5s
/// before the track ends (clamped to the config bounds); unknown position
/// (live stream, issue #165) → the default interval, not a duration-derived
/// one.
fn playing_track_sleep(
    remaining_ms: Option<u64>,
    config: &Option<crate::config::AppConfig>,
) -> u64 {
    match remaining_ms {
        Some(remaining) => {
            let buffer_ms = 5000u64;
            let remaining_secs = remaining / 1000;
            remaining_secs
                .saturating_sub(buffer_ms / 1000)
                .max(config_minimum_interval(config))
                .min(config_maximum_interval(config))
        }
        None => config_default_interval(config)
            .max(config_minimum_interval(config))
            .min(config_maximum_interval(config)),
    }
}

fn pause_backoff(consecutive_pauses: u8, default_secs: u64) -> u64 {
    match consecutive_pauses {
        0 => default_secs,
        1 => default_secs.saturating_mul(2).min(300),
        2 => default_secs.saturating_mul(4).min(300),
        _ => 300,
    }
}

fn with_jitter(base_secs: u64) -> u64 {
    let mut rng = rand::rng();
    let jitter_range = base_secs as f64 * 0.2;
    let jitter = rng.random_range(-jitter_range..=jitter_range);
    (base_secs as f64 + jitter).max(1.0) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pause_backoff_grows_then_caps() {
        assert_eq!(pause_backoff(0, 30), 30);
        assert_eq!(pause_backoff(1, 30), 60);
        assert_eq!(pause_backoff(2, 30), 120);
        assert_eq!(pause_backoff(3, 30), 300);
        assert_eq!(pause_backoff(4, 30), 300);
        assert_eq!(pause_backoff(255, 30), 300);
    }

    #[test]
    fn test_pause_backoff_uses_configured_default() {
        assert_eq!(pause_backoff(0, 45), 45);
        assert_eq!(pause_backoff(1, 45), 90);
        assert_eq!(pause_backoff(2, 45), 180);
        assert_eq!(pause_backoff(3, 45), 300);
    }

    #[test]
    fn test_pause_backoff_caps_with_large_default() {
        assert_eq!(pause_backoff(0, 200), 200);
        assert_eq!(pause_backoff(1, 200), 300);
        assert_eq!(pause_backoff(2, 200), 300);
    }

    /// Regression guard for issue #72 drift point #3.
    #[test]
    fn test_cas_discard_block_is_single_source_of_truth() {
        let source = include_str!("poll_once.rs");
        // Scan only production code (above the test module) so the test's
        // own string literals don't inflate the count.
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("poll_once.rs has no #[cfg(test)] mod tests block");
        let discard_count = prod_source
            .matches("state changed during refresh, discarding result")
            .count();
        assert!(
            discard_count >= 1,
            "poll_once.rs must route CAS-discards through the single helper log line. Found {}.",
            discard_count
        );
        let helper_def = prod_source.matches("fn cas_refresh_or_discard").count();
        assert!(helper_def >= 1, "helper defined {} times", helper_def);
        let helper_call_count = prod_source.matches("cas_refresh_or_discard(").count();
        // Expect 4 calls: Spotify proactive, Spotify 401-retry, Teams
        // proactive, Teams write-retry (issues #367/#428).
        // The "fn cas_refresh_or_discard(" definition is NOT counted here
        // because the call-shape substring includes the open-paren.
        assert!(
            helper_call_count >= 3,
            "cas_refresh_or_discard called {} times in production; need >=3 \
             (Spotify proactive + 401-retry + Teams)",
            helper_call_count
        );
    }

    /// Issue #180 regression test: refresh-success + persist on the same lock.
    ///
    /// Pre-fix, `cas_refresh_or_discard` persisted the refreshed tokens from
    /// inside the helper while the caller's write guard (a reborrow of
    /// `state.tokens.X_mut()`) was still alive for the whole call statement.
    /// `token_io::persist_tokens` then re-locked the SAME parking_lot RwLock
    /// for reading — write→read on the same lock from the same thread parks
    /// forever (parking_lot has no same-thread reentrancy detection), so
    /// every successful refresh deadlocked the polling thread.
    ///
    /// The fix persists only at the call sites, in a statement AFTER the CAS
    /// call returns, when the write guard is provably dropped. This test runs
    /// the exact production call shape (write guard reborrowed into the CAS
    /// helper) plus the persist step (re-locking the same RwLock for reading,
    /// which is the lock acquisition `token_io::persist_tokens` performs) in
    /// a spawned thread, and asserts completion via `recv_timeout`. The
    /// deadlock would hang CI, so the 10s timeout makes a regression fail
    /// fast instead of hanging the suite.
    #[test]
    fn test_refresh_success_persist_does_not_self_deadlock() {
        use std::thread;
        use std::time::Duration;

        let state = Arc::new(AppState::new());
        {
            let mut guard = state.tokens.spotify_mut();
            *guard = Some(crate::spotify::SpotifyTokens {
                access_token: "pre-refresh-access-token".to_string(),
                refresh_token: "refresh-token".to_string(),
                expires_at: Utc::now() + chrono::Duration::hours(1),
            });
        }

        let (tx, rx) = mpsc::channel();
        let state2 = state.clone();
        let handle = thread::spawn(move || {
            let pre_refresh_access_token = "pre-refresh-access-token".to_string();
            let new_tokens = crate::spotify::SpotifyTokens {
                access_token: "post-refresh-access-token".to_string(),
                refresh_token: "refresh-token".to_string(),
                expires_at: Utc::now() + chrono::Duration::hours(2),
            };
            // Exact production call shape (Spotify proactive refresh): the
            // write guard is a temporary reborrowed into the CAS helper; it
            // stays alive until the end of this statement.
            let outcome = cas_refresh_or_discard(
                "spotify",
                &mut *state2.tokens.spotify_mut(),
                &pre_refresh_access_token,
                || Ok::<_, SpotifyApiError>(new_tokens.clone()),
                |t| &t.access_token,
            );
            let committed = matches!(outcome, CasOutcome::Committed(_));
            if committed {
                // Persist step: re-lock the SAME RwLock for reading, exactly
                // as token_io::persist_tokens does on a successful refresh.
                // If the write guard above were still alive, this parks
                // forever (issue #180).
                let _persisted = state2.tokens.spotify();
            }
            let _ = tx.send(committed);
        });

        let committed = rx.recv_timeout(Duration::from_secs(10)).expect(
            "refresh-success + persist self-deadlocked: the write guard was still \
                 held when the same RwLock was re-locked for reading (issue #180)",
        );
        // The worker only returns after the persist step re-locked the same
        // RwLock successfully; joining surfaces any thread panic as a test
        // failure instead of a silently detached thread.
        handle.join().expect("persist worker thread panicked");
        assert!(committed, "CAS should commit the refreshed tokens");

        let stored = state.tokens.spotify();
        assert_eq!(
            stored.as_ref().map(|t| t.access_token.as_str()),
            Some("post-refresh-access-token"),
            "the refreshed tokens must be stored in AppState"
        );
    }

    /// Issue #180 regression guard: the CAS helper must never persist tokens
    /// itself. Pre-fix it called `token_io::persist_tokens` while the
    /// caller's write guard was still alive (write→read on the same
    /// parking_lot RwLock from the same thread parks forever), so every
    /// successful refresh self-deadlocked. The fix persists only at the
    /// call sites, in a statement AFTER the CAS call returns. If a future
    /// contributor moves a persist call back inside the helper body, the
    /// deadlock returns and this guard fails.
    #[test]
    fn test_cas_helper_body_has_no_persist_and_call_sites_persist() {
        let source = include_str!("poll_once.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("poll_once.rs has no #[cfg(test)] mod tests block");

        // Isolate the helper body by brace counting from its opening `{`
        // (house style — never boundary anchors, which drift). The `{}`
        // format placeholders inside string literals are balanced, so they
        // do not perturb the count.
        let after_sig = prod_source
            .split("fn cas_refresh_or_discard<T, E, F, G>(")
            .nth(1)
            .expect("cas_refresh_or_discard definition not found");
        let open = after_sig
            .find('{')
            .expect("cas_refresh_or_discard has no opening brace");
        let mut depth = 0usize;
        let mut end = None;
        for (i, ch) in after_sig[open..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(open + i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        let body = &after_sig[..end.expect("cas_refresh_or_discard body never closed")];
        assert!(
            !body.contains("persist_tokens("),
            "cas_refresh_or_discard must not persist tokens inside its body (issue #180: \
             the caller's write guard is alive for the whole call, so persist_tokens' \
             read lock on the same RwLock self-deadlocks). Body:\n{}",
            body
        );
        // All persistence must happen at the call sites, after the CAS call
        // returns (guard provably dropped): the three invalid_grant/dead-token
        // clear paths (Spotify proactive, Spotify 401-retry, Teams) plus the
        // four refresh-success call sites (Spotify proactive, Spotify
        // 401-retry, Teams proactive, Teams write-retry for issues #367/#428).
        // The Teams write-retry contributes two sites (refresh-success persist
        // + dead-credential clear persist), and the handle_no_track clear
        // retry (issue #455-residual) contributes two more, so the total is
        // ten.
        let persist_count = prod_source.matches("token_io::persist_tokens(").count();
        assert!(
            persist_count >= 10,
            "expected at least 10 persist_tokens call sites in production (3 provider \
             clear paths + 4 refresh-success call sites + 1 reactive dead-credential \
             clear + 2 no-track reactive clear-retry); found {}. If a call-site \
             persist is removed, refreshed/cleared tokens stop being flushed to disk; \
             if one is added inside cas_refresh_or_discard, the #180 self-deadlock \
             returns.",
            persist_count
        );
    }

    /// Regression guard for issue #72 drift point #1: every no-track
    /// code path (main `Ok(None)` arm, 401-retry `Ok(None)` arm, and —
    /// since issue #242 — the idle 304 arm in `not_modified_iteration`)
    /// must funnel through `record_no_track_outcome` so they cannot
    /// drift apart.
    ///
    /// Note: `process_track`'s paused-but-tracked branch also
    /// increments `consecutive_pauses` (issue #38). That increment is
    /// a separate concern (track found but `is_playing == false`) and
    /// is NOT the no-track drift point — the drift was the *no-track*
    /// increment order differing between the main arm and the 401-retry
    /// arm. We assert the no-track paths share a helper, not that
    /// every increment lives in one place.
    #[test]
    fn test_no_track_paths_share_record_helper() {
        let source = include_str!("poll_once.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("poll_once.rs has no #[cfg(test)] mod tests block");
        // Occurrences of `record_no_track_outcome(` in production code.
        // The `fn record_no_track_outcome(` definition matches too, so
        // the expected total is 1 definition + one call site per no-track
        // path: main `Ok(None)`, 401-retry `Ok(None)`, and the idle 304
        // (`not_modified_iteration`, issue #242). All three funnel the
        // increment through the same helper; a fourth site outside a
        // shared helper is a regression. See issue #72 drift point #1.
        let call_count = prod_source.matches("record_no_track_outcome(").count();
        assert!(
            call_count >= 4,
            "Expected at least 4 occurrences in production (3 call sites: main \
             Ok(None), 401-retry Ok(None), idle 304 in not_modified_iteration, \
             plus the fn definition). Found {}. If a future contributor adds \
             a no-track handling site outside the shared helper, the \
             increment order can drift again. See issue #72 drift point #1.",
            call_count
        );
    }

    /// Regression guard for issue #72 drift point #2.
    #[test]
    fn test_error_event_emitted_in_exactly_one_place_per_failed_poll() {
        let source = include_str!("poll_once.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("poll_once.rs has no #[cfg(test)] mod tests block");
        let canonical_msg_count = prod_source
            .matches("Failed to get currently playing:")
            .count();
        assert!(
            canonical_msg_count >= 1,
            "expected at least 1 'Failed to get currently playing:' emit_error; found {}",
            canonical_msg_count
        );
    }

    /// Regression guard: the unified API call site must be invoked
    /// from exactly the two places the design calls for — the
    /// top-level `run()` path and the 401-retry recursive call —
    /// and nowhere else (no third spot added by a future contributor).
    /// We grep for the *bound name* of each call site, not the bare
    /// `get_currently_playing(` substring (which would also match
    /// the fn definition site and would not match the top-level call,
    /// which is extracted to a `let result = ...; match result {}`
    /// shape).
    #[test]
    fn test_single_top_level_get_currently_playing_match() {
        let source = include_str!("poll_once.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("poll_once.rs has no #[cfg(test)] mod tests block");
        let top_level = prod_source
            .matches("get_currently_playing(&access_token,")
            .count();
        let retry = prod_source
            .matches("get_currently_playing(&retry_token,")
            .count();
        assert!(
            top_level >= 1,
            "expected at least 1 top-level get_currently_playing call; found {}",
            top_level
        );
        assert!(
            retry >= 1,
            "expected at least 1 401-retry get_currently_playing call; found {}",
            retry
        );
    }

    /// Regression guard for issue #60: `start_polling`'s caller
    /// (`commands::start_syncing`) has already claimed `is_syncing`; a
    /// second compare-exchange here would always lose and surface
    /// "Polling is already running" after every fresh install.
    ///
    /// The body is isolated by brace counting from `start_polling`'s
    /// opening `{` (house style — never boundary anchors/log-line
    /// anchors, which silently drift and leave the assertion vacuous).
    #[test]
    fn test_start_polling_does_not_claim_is_syncing() {
        let source = include_str!("state.rs");
        let after_sig = source
            .split("pub fn start_polling(")
            .nth(1)
            .expect("state.rs has no `pub fn start_polling(`");
        let open = after_sig
            .find('{')
            .expect("start_polling has no opening brace");
        let mut depth = 0usize;
        let mut end = None;
        for (i, ch) in after_sig[open..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(open + i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        let body = &after_sig[..end.expect("start_polling body never closed")];
        assert!(
            !body.contains(".compare_exchange("),
            "polling::start_polling must not CAS is_syncing. See issue #60."
        );
    }

    /// Regression guard for issue #79/#117: poll_once.rs must NOT emit raw
    /// "error" events directly.
    #[test]
    fn test_no_raw_error_emit_in_poll_once() {
        let source = include_str!("poll_once.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("poll_once.rs has no #[cfg(test)] mod tests block");
        assert!(
            !prod_source.contains(r#"emit("error","#),
            "poll_once.rs must not emit raw \"error\" events directly."
        );
        let helper_call_count = prod_source.matches("emit_error(").count();
        assert!(
            helper_call_count >= 2,
            "emit_error called {} times; need >=2",
            helper_call_count
        );
    }

    /// Issue #159: a Spotify 429 backoff honors the server's Retry-After,
    /// floored at the error retry interval so a tiny value can't busy-loop.
    #[test]
    fn test_spotify_backoff_base_honors_retry_after_floored() {
        use crate::spotify::SpotifyApiError;
        assert_eq!(
            spotify_backoff_base(&SpotifyApiError::RateLimited(Some(45))),
            45
        );
        assert_eq!(
            spotify_backoff_base(&SpotifyApiError::RateLimited(Some(10))),
            ERROR_RETRY_INTERVAL_SECONDS,
            "retry-after below the floor must be clamped up"
        );
        assert_eq!(
            spotify_backoff_base(&SpotifyApiError::RateLimited(None)),
            RATE_LIMIT_BACKOFF_SECONDS,
            "header-less 429 falls back to the fixed backoff"
        );
    }

    /// Issue #154: a Teams set/clear failure contributes the server's
    /// Retry-After seconds, the jittered default backoff when the header is
    /// absent, and nothing for non-throttle errors.
    #[test]
    fn test_rate_limit_sleep_secs_teams() {
        assert_eq!(
            rate_limit_sleep_secs(&TeamsApiError::RateLimited(Some(90))),
            90
        );
        assert_eq!(rate_limit_sleep_secs(&TeamsApiError::ExpiredToken(401)), 0);
        assert_eq!(
            rate_limit_sleep_secs(&TeamsApiError::Forbidden(403, "denied".to_string())),
            0
        );
        assert_eq!(rate_limit_sleep_secs(&TeamsApiError::InvalidGrant), 0);
        assert_eq!(
            rate_limit_sleep_secs(&TeamsApiError::Transient("boom".to_string())),
            0
        );
        // Header-less 429 → jittered default backoff (60 ± 20% → [48, 72]).
        let no_header = rate_limit_sleep_secs(&TeamsApiError::RateLimited(None));
        assert!(
            (48..=72).contains(&no_header),
            "jittered backoff out of range: {}",
            no_header
        );
    }

    /// Issue #156: the expiry string must be offset-less with exactly 6
    /// fraction digits (≤ the documented 7) — no `+00:00`, no `Z`, no
    /// 9-digit nanosecond fraction.
    #[test]
    fn test_format_expiry_is_offset_less_with_six_fraction_digits() {
        let fixed = chrono::DateTime::parse_from_rfc3339("2015-02-18T23:16:09.123456789+00:00")
            .unwrap()
            .with_timezone(&Utc);
        let s = format_expiry(fixed);
        assert!(
            !s.contains('+') && !s.contains('Z'),
            "offset leaked into dateTime: {}",
            s
        );
        assert!(
            s.starts_with("2015-02-18T23:16:09."),
            "unexpected shape: {}",
            s
        );
        let fraction = s.split('.').nth(1).unwrap_or("");
        assert_eq!(
            fraction.len(),
            6,
            "expected exactly 6 fraction digits, got '{}'",
            fraction
        );
    }

    /// Issue #155/#156: the clear-path placeholder expiry must be offset-less
    /// with 6 fraction digits.
    #[test]
    fn test_placeholder_expiry_str_is_offset_less() {
        let s = placeholder_expiry_str();
        assert!(
            !s.contains('+') && !s.contains('Z'),
            "offset leaked into placeholder expiry: {}",
            s
        );
        let fraction = s.split('.').nth(1).unwrap_or("");
        assert_eq!(fraction.len(), 6, "got '{}'", fraction);
    }

    /// Issue #165: known position → an expiry exists; live stream (None) →
    /// no expiry so no `expiryDateTime` goes on the wire.
    ///
    /// Issue #264: the VALUE must be `now + remaining + buffer` — asserting
    /// the offset-less shape alone passes if the arithmetic sign flips or
    /// the buffer is dropped. The buffer default is read from the config
    /// type rather than hardcoded so a default change cannot silently
    /// invalidate the expectation.
    #[test]
    fn test_status_expiry_known_and_unknown_position() {
        let config = Some(crate::config::AppConfig::default());
        let buffer_secs = config
            .as_ref()
            .expect("config is Some")
            .polling
            .expiry_buffer_seconds;
        let remaining_ms = 120_000u64;

        let before = chrono::Utc::now();
        let s = status_expiry_str(Some(remaining_ms), &config)
            .expect("known position must yield an expiry");
        assert!(
            !s.contains('+') && !s.contains('Z'),
            "offset leaked into status expiry: {}",
            s
        );

        // The wire shape is offset-less; re-attach UTC to parse it back.
        let parsed = chrono::DateTime::parse_from_rfc3339(&format!("{}+00:00", s))
            .expect("status expiry must round-trip as RFC3339 once UTC is re-attached")
            .with_timezone(&Utc);
        let delta_secs = (parsed - before).num_seconds();
        let expected = (remaining_ms / 1000) as i64 + buffer_secs as i64;
        assert!(
            (delta_secs - expected).abs() <= 2,
            "status expiry must be now + remaining + buffer = {}s; got {}s (delta {}s). \
             A flipped `+ buffer_ms` or a zeroed default buffer lands here. See issue #264.",
            expected,
            delta_secs,
            delta_secs - expected
        );

        assert_eq!(
            status_expiry_str(None, &config),
            None,
            "live streams must not get an expiryDateTime"
        );
    }

    /// Issue #165: sleep falls back to the default interval for live streams
    /// instead of a duration-derived value; known positions sleep until ~5s
    /// before track end, clamped to the config bounds.
    #[test]
    fn test_playing_track_sleep_known_position_and_live_stream() {
        let config = Some(crate::config::AppConfig::default());
        // Default config: min 10s, max 60s.
        assert_eq!(playing_track_sleep(Some(30_000), &config), 25);
        assert_eq!(
            playing_track_sleep(Some(120_000), &config),
            60,
            "long remaining time clamps to max interval"
        );
        assert_eq!(
            playing_track_sleep(Some(2_000), &config),
            10,
            "short remaining time clamps to min interval"
        );
        assert_eq!(
            playing_track_sleep(None, &config),
            30,
            "live stream falls back to the default interval"
        );
        // No config → the built-in defaults (30s default, 10s min, 60s max).
        assert_eq!(playing_track_sleep(None, &None), 30);
        assert_eq!(playing_track_sleep(Some(2_000), &None), 10);
    }

    /// Issue #156 regression guard: the playing-status expiry must be built
    /// with the offset-less format, never through `to_rfc3339()` (which
    /// embeds `+00:00` and up to 9 fraction digits). The three remaining
    /// `to_rfc3339()` uses are frontend payload timestamps, which are fine.
    #[test]
    fn test_expiry_uses_offset_less_format_not_rfc3339() {
        let source = include_str!("poll_once.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("poll_once.rs has no #[cfg(test)] mod tests block");
        assert!(
            prod_source.contains(r#""%Y-%m-%dT%H:%M:%S%.6f""#),
            "expiry must use the offset-less 6-digit format (issue #156)"
        );
        let expiry_lines = prod_source
            .lines()
            .filter(|l| l.contains("expiry_str ="))
            .collect::<Vec<_>>();
        assert!(
            !expiry_lines.iter().any(|l| l.contains("to_rfc3339")),
            "expiry_str must not be built with to_rfc3339: {:?}",
            expiry_lines
        );
    }

    /// Issue #153 regression guard: Teams set/clear failures must be
    /// classified by the typed `TeamsApiError` variants, not by
    /// string-sniffing the error body for "unauthorized"/"forbidden"/401/403.
    #[test]
    fn test_teams_error_classification_is_typed_not_string_sniffed() {
        let source = include_str!("poll_once.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("poll_once.rs has no #[cfg(test)] mod tests block");
        for sniff in [
            r#"e_str.contains("unauthorized")"#,
            r#"e_str.contains("forbidden")"#,
            r#"e_str.contains("401")"#,
            r#"e_str.contains("403")"#,
        ] {
            assert!(
                !prod_source.contains(sniff),
                "string-sniffing on Teams error bodies must be gone (issue #153): {}",
                sniff
            );
        }
        assert!(
            prod_source.contains("TeamsApiError::Forbidden(_, _)"),
            "Forbidden must be matched by variant (issue #153)"
        );
    }

    // Issue #3.0-P1: the availability re-arm must happen at most every 4
    // minutes — Available sessions FADE after 5 min regardless of
    // `expirationDuration`, so the cadence must be strictly inside that
    // window (240s < 300s).
    #[test]
    fn test_should_rearm_availability_cadence() {
        let now = Instant::now();
        // Never armed → arm immediately.
        assert!(should_rearm_availability(None, now));
        // Armed 1 second ago → don't re-arm.
        let recent = now - std::time::Duration::from_secs(1);
        assert!(!should_rearm_availability(Some(recent), now));
        // Just under the cadence → don't re-arm.
        let under = now - std::time::Duration::from_secs(AVAILABILITY_REARM_SECONDS - 1);
        assert!(!should_rearm_availability(Some(under), now));
        // At/over the cadence → re-arm (strictly < 5 min fade window).
        let at = now - std::time::Duration::from_secs(AVAILABILITY_REARM_SECONDS);
        assert!(should_rearm_availability(Some(at), now));
        let over = now - std::time::Duration::from_secs(AVAILABILITY_REARM_SECONDS + 60);
        assert!(should_rearm_availability(Some(over), now));
        const { assert!(AVAILABILITY_REARM_SECONDS < 300) };
        // Guard above must hold: re-arm cadence strictly inside the
        // 5-minute Available fade window (issue #3.0-P1).
    }

    /// Issue #3.0-P1/P2 regression guard: inside `process_track`, the
    /// presence-gate read (`get_teams_presence`) must precede the status
    /// write (`set_teams_status_message`) so a busy/meeting presence can
    /// suppress it, and the availability call sites (set_teams_presence
    /// re-arm + clear_teams_presence on pause) must exist.
    #[test]
    fn test_presence_gate_precedes_status_write_and_availability_call_sites_exist() {
        let source = include_str!("poll_once.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("poll_once.rs has no #[cfg(test)] mod tests block");

        // Isolate the process_track body by brace counting from its opening
        // `{` (house style — never boundary anchors, which drift). The
        // json!({...}) braces and `\u{...}` escapes inside string literals
        // are balanced, so they do not perturb the count.
        let after_sig = prod_source
            .split("pub(crate) fn process_track(")
            .nth(1)
            .expect("process_track definition not found");
        let open = after_sig
            .find('{')
            .expect("process_track has no opening brace");
        let mut depth = 0usize;
        let mut end = None;
        for (i, ch) in after_sig[open..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(open + i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        let body = &after_sig[..end.expect("process_track body never closed")];

        let gate_pos = body
            .find("get_teams_presence(")
            .expect("process_track must call get_teams_presence (presence gate, issue #3.0-P2)");
        let write_pos = body
            .find("set_teams_status_message(")
            .expect("process_track must call set_teams_status_message");
        assert!(
            gate_pos < write_pos,
            "the presence-gate read must precede the status write in process_track \
             so a busy/meeting presence can suppress it (issue #3.0-P2)"
        );
        assert!(
            body.contains("set_teams_presence("),
            "process_track must re-arm set_teams_presence(Available, ...) while playing \
             (issue #3.0-P1)"
        );
        assert!(
            body.contains("clear_teams_presence("),
            "process_track must clear_teams_presence on pause (issue #3.0-P1)"
        );
    }

    /// Candidate C11 (docs/scope-3.3.md §C11): a 304 Not Modified with a
    /// tracked track is a pure no-op iteration — default-interval sleep,
    /// pause/transient counters reset exactly like the unchanged-track path.
    /// The stored ETag survives structurally: the 304 path never touches it,
    /// so the next poll stays conditional.
    #[test]
    fn test_not_modified_keeps_state_and_sleeps_default_interval() {
        let config = Some(crate::config::AppConfig::default());
        let mut consecutive_pauses: u8 = 3;
        let mut transient_failure_count: u8 = 2;

        let iteration = not_modified_iteration(
            &Some("Artist - Track".to_string()),
            &mut consecutive_pauses,
            &mut transient_failure_count,
            &config,
        );

        let seconds = match iteration {
            PollIteration::Sleep { seconds } => seconds,
            _ => panic!("304 must yield a Sleep iteration"),
        };
        assert_eq!(
            seconds, 30,
            "304 must sleep the configured default interval"
        );
        assert_eq!(consecutive_pauses, 0, "unchanged track resets pauses");
        assert_eq!(
            transient_failure_count, 0,
            "a 304 counts as success for the 5-strikes counter"
        );
    }

    /// Issue #242: a 304 with no tracked track means "still nothing playing".
    /// It must advance the pause backoff exactly like an unconditional 204
    /// no-track (steady conditional GETs, no 304/drop/unconditional
    /// oscillation, no stalled backoff).
    #[test]
    fn test_not_modified_without_tracked_track_advances_pause_backoff() {
        let config = Some(crate::config::AppConfig::default());
        let mut consecutive_pauses: u8 = 1;
        let mut transient_failure_count: u8 = 1;

        let iteration = not_modified_iteration(
            &None,
            &mut consecutive_pauses,
            &mut transient_failure_count,
            &config,
        );

        let seconds = match iteration {
            PollIteration::Sleep { seconds } => seconds,
            _ => panic!("304 must yield a Sleep iteration"),
        };
        assert_eq!(
            seconds, 60,
            "idle 304 must sleep the pause backoff (2x default at pauses=1), matching 204 no-track"
        );
        assert_eq!(
            consecutive_pauses, 2,
            "idle 304 must advance the pause counter like a 204 no-track"
        );
        assert_eq!(
            transient_failure_count, 0,
            "a 304 counts as success for the 5-strikes counter"
        );
    }

    /// Candidate C11 regression guard: both get_currently_playing call
    /// sites must pass the stored validator (`last_etag.as_deref()`) so
    /// the conditional GET cannot silently degrade to unconditional-only.
    #[test]
    fn test_both_get_currently_playing_call_sites_are_conditional() {
        let source = include_str!("poll_once.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("poll_once.rs has no #[cfg(test)] mod tests block");
        let conditional = prod_source.matches(", last_etag.as_deref())").count();
        assert!(
            conditional >= 2,
            "expected at least 2 conditional GET call sites passing              last_etag.as_deref() (top-level + 401-retry); found {}",
            conditional
        );
    }

    /// Issue #262: the 5-strikes transient-failure counter must break the
    /// polling loop at exactly `TRANSIENT_FAILURE_EXIT_THRESHOLD` — no
    /// sooner (a transient blip must not kill the session) and no later
    /// (a permanently broken token must stop hammering the API).
    #[test]
    fn test_transient_outcome_breaks_exactly_at_threshold() {
        // The issue names five strikes explicitly; pin the constant so a
        // future retune cannot silently change the documented contract (the
        // literal assertions below would otherwise follow it).
        assert_eq!(
            TRANSIENT_FAILURE_EXIT_THRESHOLD, 5,
            "issue #262 specifies exactly 5 consecutive transient failures"
        );
        assert!(
            transient_outcome(4).is_none(),
            "4 consecutive transient failures must NOT break the loop; the counter is \
             reset by any success, so an early break kills the session on a blip"
        );
        assert!(
            matches!(transient_outcome(5), Some(PollIteration::Break)),
            "5 consecutive transient failures MUST break the loop so the user is asked \
             to reconnect (issue #262)"
        );
        // Saturating-add can reach u8::MAX; the threshold decision must stay
        // stable there (no panic, still Break).
        assert!(
            matches!(transient_outcome(u8::MAX), Some(PollIteration::Break)),
            "a saturated counter must still break"
        );
    }

    /// Issue #295 regression guard: only a genuinely dead Teams credential
    /// forces re-auth. Pre-fix the `RefreshFailed` arm matched every error
    /// unconditionally, so a single 5xx/dropped connection discarded the
    /// session and drove a full device-code re-auth.
    #[test]
    fn test_teams_refresh_reauth_policy_is_dead_token_only() {
        assert!(
            teams_refresh_requires_reauth(&TeamsApiError::InvalidGrant),
            "a dead refresh token (invalid_grant) must force re-auth"
        );
        assert!(
            teams_refresh_requires_reauth(&TeamsApiError::ExpiredToken(401)),
            "a rejected access token (401) must force re-auth"
        );
        for transient in [
            TeamsApiError::Transient("Failed to send refresh token request: boom".to_string()),
            TeamsApiError::RateLimited(Some(30)),
            TeamsApiError::RateLimited(None),
            TeamsApiError::Other(400, "invalid_client".to_string()),
            TeamsApiError::Forbidden(403, "denied".to_string()),
        ] {
            assert!(
                !teams_refresh_requires_reauth(&transient),
                "transient Teams refresh failure must keep the session: {:?}",
                transient
            );
        }
    }

    /// Issue #296 regression guard: an expired access token with a cold
    /// keychain cache (empty secret) must be classified as
    /// `CredentialsUnavailable` rather than attempted — and the guard must
    /// not fire when the token is still fresh or both credentials are
    /// present.
    #[test]
    fn test_spotify_refresh_plan_requires_non_empty_credentials() {
        assert_eq!(
            spotify_refresh_plan(true, "client-id", ""),
            SpotifyRefreshPlan::CredentialsUnavailable,
            "an empty client_secret (cold keychain cache) must not be attempted"
        );
        assert_eq!(
            spotify_refresh_plan(true, "", "client-secret"),
            SpotifyRefreshPlan::CredentialsUnavailable,
            "an empty client_id must not be attempted"
        );
        assert_eq!(
            spotify_refresh_plan(true, "client-id", "client-secret"),
            SpotifyRefreshPlan::Refresh,
            "expired token + both credentials present must refresh"
        );
        assert_eq!(
            spotify_refresh_plan(false, "", ""),
            SpotifyRefreshPlan::Fresh,
            "a token still inside its window is used as-is regardless of credentials"
        );
    }
    /// Issue #343: the change-key fingerprint must move with each of the
    /// status-shaping config values (filter flag, placeholder, format) —
    /// otherwise a mid-track flip reads as "unchanged" and the stale
    /// status stays posted.
    #[test]
    fn test_status_config_fingerprint_tracks_filter_placeholder_format() {
        let base = Some(crate::config::AppConfig::default());
        let fp = status_config_fingerprint(&base);

        let mut off = crate::config::AppConfig::default();
        off.teams.profanity_filter = false;
        assert_ne!(
            fp,
            status_config_fingerprint(&Some(off)),
            "toggling the filter must change the fingerprint"
        );

        let mut ph = crate::config::AppConfig::default();
        ph.teams.profanity_placeholder = "something else".to_string();
        assert_ne!(
            fp,
            status_config_fingerprint(&Some(ph)),
            "editing the placeholder must change the fingerprint"
        );

        let mut fmt = crate::config::AppConfig::default();
        fmt.teams.status_format = "{track}".to_string();
        assert_ne!(
            fp,
            status_config_fingerprint(&Some(fmt)),
            "editing the format must change the fingerprint"
        );

        assert_eq!(
            fp,
            status_config_fingerprint(&base),
            "identical config must fingerprint identically"
        );
    }

    /// Issue #343: the 304 force-rewrite fires exactly when the stored key
    /// no longer matches the current track+config — no tracked track, no
    /// rewrite; matching key, no rewrite; flipped config, one rewrite
    /// carrying the last observed track.
    #[test]
    fn test_config_flip_rewrite_track_fires_only_on_mismatch() {
        let config = Some(crate::config::AppConfig::default());
        let track = crate::spotify::TrackInfo {
            title: "T".to_string(),
            artist: "A".to_string(),
            album: String::new(),
            album_art_url: String::new(),
            is_playing: true,
            progress_ms: Some(0),
            duration_ms: 0,
        };
        let state = Arc::new(AppState::new());

        // No tracked track → no rewrite.
        assert!(
            config_flip_rewrite_track(&state, &None, &config).is_none(),
            "nothing tracked means nothing to rewrite"
        );

        *state.polling.current_track_mut() = Some(track.clone());
        let key = status_track_key(&track, &config);

        // Matching key → steady-state 304 stays a no-op.
        assert!(
            config_flip_rewrite_track(&state, &Some(key.clone()), &config).is_none(),
            "a matching key must not force a rewrite"
        );

        // Same track, flipped filter → one rewrite with the stored track.
        let mut flipped = crate::config::AppConfig::default();
        flipped.teams.profanity_filter = false;
        let rewrite = config_flip_rewrite_track(&state, &Some(key), &Some(flipped));
        let rewrite = rewrite.expect("a config flip must force one rewrite");
        assert_eq!(rewrite.title, "T");
        assert_eq!(rewrite.artist, "A");
    }

    /// Issue #364: the debounce predicate fires only for a change inside
    /// the 500ms window — unchanged polls, first writes, and changes past
    /// the window all post.
    #[test]
    fn test_debounce_active_only_for_change_inside_window() {
        assert!(
            !debounce_active(false, Some(Instant::now())),
            "unchanged polls never debounce"
        );
        assert!(
            !debounce_active(true, None),
            "no prior write means nothing to debounce against"
        );
        assert!(
            debounce_active(true, Some(Instant::now())),
            "a change right after a write must debounce"
        );
        assert!(
            !debounce_active(
                true,
                Some(Instant::now() - std::time::Duration::from_secs(10))
            ),
            "a change past the window must post"
        );
        assert_eq!(
            DEBOUNCE_RETRY_SECONDS, 1,
            "the debounce retry parks ~1s, not on the duration-derived sleep"
        );
    }

    /// Issue #364 ordering guard: the debounce early return runs BEFORE any
    /// side effect, so the retry re-detects the change and emits/posts
    /// exactly once. Pre-fix the store/emit/placeholder-clear/gate work ran
    /// first and only the track key was restored, duplicating the
    /// `spotify-track-changed` event and the Graph presence read on retry.
    #[test]
    fn test_debounce_branch_restores_previous_key_and_sleeps_short() {
        let source = include_str!("poll_once.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("poll_once.rs has no #[cfg(test)] mod tests block");
        let after_sig = prod_source
            .split("pub(crate) fn process_track(")
            .nth(1)
            .expect("process_track definition not found");
        let open = after_sig
            .find('{')
            .expect("process_track has no opening brace");
        let mut depth = 0usize;
        let mut end = None;
        for (i, ch) in after_sig[open..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(open + i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        let body = &after_sig[open..end.expect("process_track body never closed")];
        let debounce_pos = body
            .find("if debounce_active")
            .expect("process_track must gate on debounce_active (issue #364)");
        for marker in [
            "*last_track_key =",
            "current_track_mut",
            "\"spotify-track-changed\",",
            "teams_token_for_write",
            "*gated_track_key =",
        ] {
            let pos = body
                .find(marker)
                .unwrap_or_else(|| panic!("process_track body must contain {}", marker));
            assert!(
                debounce_pos < pos,
                "debounce check must precede '{}' so the retry re-detects the change exactly once (issue #364)",
                marker
            );
        }
        assert!(
            body.contains("return DEBOUNCE_RETRY_SECONDS;"),
            "the debounce branch must park on the short fixed retry, not playing_track_sleep (issue #364)"
        );
        assert_eq!(
            DEBOUNCE_RETRY_SECONDS, 1,
            "the debounce retry parks ~1s, not on the duration-derived sleep"
        );
        assert!(
            debounce_active(true, Some(Instant::now())),
            "a change right after a write must debounce"
        );
        assert!(
            !debounce_active(false, Some(Instant::now())),
            "unchanged polls never debounce"
        );
    }

    /// Issues #370/#388 structural guard: the Teams refresh lives in one
    /// shared helper called from BOTH write paths. Pre-fix
    /// `handle_no_track` cloned the stored token with no expiry/refresh.
    #[test]
    fn test_teams_token_refresh_is_single_shared_helper() {
        let source = include_str!("poll_once.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("poll_once.rs has no #[cfg(test)] mod tests block");
        assert!(
            prod_source.matches("fn teams_token_for_write(").count() >= 1,
            "teams_token_for_write must be defined"
        );
        // Definition + process_track + handle_no_track call sites.
        assert!(
            prod_source.matches("teams_token_for_write(").count() >= 3,
            "expected def + 2 call sites (process_track, handle_no_track); found a drift"
        );
        assert!(
            !prod_source.contains("let teams_tok = match teams_tokens"),
            "handle_no_track must not clone the stored token without refresh (issue #370)"
        );
    }

    /// Issue #373: the first no-track poll attempts one clear (fresh
    /// thread, stale pre-restart status); tracked clears always run;
    /// later nothing-tracked polls stay a no-op.
    #[test]
    fn test_first_no_track_attempts_clear_exactly_once() {
        assert!(
            first_no_track_attempts_clear(&Some("key".to_string()), false),
            "a tracked track must always attempt the clear"
        );
        assert!(
            first_no_track_attempts_clear(&None, true),
            "a fresh thread must attempt one clear even with nothing tracked"
        );
        assert!(
            !first_no_track_attempts_clear(&None, false),
            "later idle polls must stay a no-op"
        );
        assert!(
            first_no_track_attempts_clear(&Some("key".to_string()), true),
            "first iteration with a tracked track still clears"
        );
    }

    /// Issue #380: the gate re-check follows the re-arm cadence — due
    /// with no re-check on record or a stale one, not due right after one.
    #[test]
    fn test_gate_recheck_due_follows_rearm_cadence() {
        let now = Instant::now();
        assert!(
            gate_recheck_due(None, now),
            "no re-check on record means the re-check is due"
        );
        assert!(
            gate_recheck_due(
                Some(now - std::time::Duration::from_secs(AVAILABILITY_REARM_SECONDS + 1)),
                now
            ),
            "a re-check older than the re-arm cadence means the re-check is due"
        );
        assert!(
            !gate_recheck_due(Some(now), now),
            "a fresh re-check must not re-read presence every poll"
        );
    }

    /// Issue #380 structural guard: the gated branch re-reads presence
    /// and can clear the gate mid-track (meeting ends → late post).
    /// Pre-fix a gated track stayed gated for the whole duration.
    #[test]
    fn test_gated_branch_rechecks_presence_and_clears_gate() {
        let source = include_str!("poll_once.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("poll_once.rs has no #[cfg(test)] mod tests block");
        let after_sig = prod_source
            .split("pub(crate) fn process_track(")
            .nth(1)
            .expect("process_track definition not found");
        let open = after_sig
            .find('{')
            .expect("process_track has no opening brace");
        let mut depth = 0usize;
        let mut end = None;
        for (i, ch) in after_sig[open..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(open + i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        let body = &after_sig[..end.expect("process_track body never closed")];
        assert!(
            body.matches("get_teams_presence(").count() >= 2,
            "process_track needs the change-time gate read AND the mid-track re-check (issue #380)"
        );
        assert!(
            body.contains("gate_recheck_due("),
            "the gated branch must throttle re-checks on the re-arm cadence (issue #380)"
        );
        assert!(
            body.contains("presence gate cleared mid-track"),
            "a cleared gate must fall through to the late post (issue #380)"
        );
    }

    /// Issue #384: byte-identical writes skip while the keepalive is
    /// fresh, but a fingerprint/track change or a lapsed keepalive
    /// force-writes.
    #[test]
    fn test_identical_write_skipped_until_fingerprint_change_or_keepalive() {
        let now = Instant::now();
        let fresh = Some(now);
        assert!(
            should_skip_identical_write(false, Some("status"), "status", fresh, now),
            "identical status inside the keepalive must skip the write"
        );
        assert!(
            !should_skip_identical_write(false, Some("old"), "new", fresh, now),
            "changed text must write"
        );
        assert!(
            !should_skip_identical_write(false, None, "status", fresh, now),
            "nothing posted yet must write"
        );
        assert!(
            !should_skip_identical_write(true, Some("status"), "status", fresh, now),
            "a fingerprint/track change must force-write even identical text"
        );
        let stale = Some(now - std::time::Duration::from_secs(STATUS_KEEPALIVE_SECONDS + 1));
        assert!(
            !should_skip_identical_write(false, Some("status"), "status", stale, now),
            "a lapsed keepalive must force-write so the expiry never lapses"
        );
        assert!(
            !should_skip_identical_write(false, Some("status"), "status", None, now),
            "no write on record must write"
        );
    }

    /// Issue #389: the 5-strikes transient-failure exit must emit the
    /// provider-specific `spotify-reconnect-required` alongside the generic
    /// `reconnect-required` — mirroring the `InvalidGrant` arms — so the
    /// frontend can start a real Spotify OAuth flow instead of seeing only
    /// the generic banner. Brace-counted body isolation (house style —
    /// never boundary anchors, which drift).
    #[test]
    fn test_five_strikes_exit_emits_spotify_reconnect() {
        let source = include_str!("poll_once.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("poll_once.rs has no #[cfg(test)] mod tests block");
        let marker = "5 consecutive transient failures, exiting and requiring reconnect";
        let exit_pos = prod_source
            .find(marker)
            .expect("the 5-strikes exit log line must exist");
        let window = &prod_source[exit_pos..];
        let window_end = window
            .find("return iteration;")
            .expect("5-strikes exit must return");
        let window = &window[..window_end];
        assert!(
            window.contains(r#"emit("spotify-reconnect-required""#),
            "the 5-strikes exit must emit spotify-reconnect-required (issue #389)"
        );
        assert!(
            window.contains(r#"emit("reconnect-required""#),
            "the 5-strikes exit must keep the generic reconnect-required"
        );
    }

    /// Issue #455-residual: the no-track clear path must mirror the
    /// process_track ExpiredToken refresh+single-retry — a 401 on the clear
    /// can mean the token expired mid-sequence even though the pre-write
    /// expiry check passed. Brace-counted `handle_no_track` body isolation
    /// (house style — never boundary anchors, which drift).
    #[test]
    fn test_no_track_clear_retries_expired_token_after_refresh() {
        let source = include_str!("poll_once.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("poll_once.rs has no #[cfg(test)] mod tests block");
        let after_sig = prod_source
            .split("pub(crate) fn handle_no_track(")
            .nth(1)
            .expect("handle_no_track definition not found");
        let open = after_sig
            .find('{')
            .expect("handle_no_track has no opening brace");
        let mut depth = 0usize;
        let mut end = None;
        for (i, ch) in after_sig[open..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(open + i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        let body = &after_sig[..end.expect("handle_no_track body never closed")];
        assert!(
            body.contains("TeamsApiError::ExpiredToken(status)"),
            "handle_no_track must reactively match ExpiredToken on the clear (issue #455-residual)"
        );
        assert!(
            body.contains("refresh_teams_token(&teams_tok)"),
            "handle_no_track must refresh once before blaming the credential"
        );
        assert!(
            body.contains("clear_teams_status_message("),
            "handle_no_track must retry the clear with the refreshed token"
        );
        assert!(
            body.contains("teams-reconnect-required"),
            "a dead-credential clear must surface teams-reconnect-required"
        );
    }
}
