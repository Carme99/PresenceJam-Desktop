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
    format_status, get_currently_playing, is_token_expired, refresh_spotify_token, SpotifyApiError,
};
use crate::teams::{
    clear_teams_presence, clear_teams_status_message, get_teams_presence,
    is_presence_gated, is_token_expired as is_teams_token_expired, presence_gate_reason,
    refresh_teams_token, set_teams_presence, set_teams_status_message, TeamsApiError,
};
use crate::token_io;
use crate::AppState;

use super::{emit_error, ErrorSeverity};

const ERROR_RETRY_INTERVAL_SECONDS: u64 = 30;
const RATE_LIMIT_BACKOFF_SECONDS: u64 = 60;
const DEBOUNCE_MS: u64 = 500;
const TRANSIENT_FAILURE_EXIT_THRESHOLD: u8 = 5;
/// Minimum gap between setPresence re-arms while a track plays (issue
/// #3.0-P1). Available sessions FADE after 5 minutes regardless of
/// `expirationDuration` (Microsoft Learn v1.0), so the session must be
/// re-armed well inside that window; 4 minutes leaves slack.
const AVAILABILITY_REARM_SECONDS: u64 = 4 * 60;

/// What the driver should do after this iteration.
pub(crate) enum PollIteration {
    Sleep { seconds: u64 },
    Break,
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
        if spotify_tokens.is_some() { "Some" } else { "None" }
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
            );
        }
    };

    let token_expired = is_token_expired(&spotify_tokens);
    log::debug!("[POLLING] poll_once: token_expired={}", token_expired);

    let (client_id, client_secret) = get_spotify_credentials(&config);
    let spotify_tokens = if token_expired {
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
                    || Ok(new_tokens.clone()),
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
                            );
                        }
                    },
                    CasOutcome::RefreshFailed(_) => unreachable!("inner refresh_fn is Ok-wrapping"),
                }
            }
            Err(e) => {
                log::error!("[POLLING] poll_once: Failed to refresh Spotify token: {}", e);
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
                );
            }
        }
    } else {
        spotify_tokens
    };

    let access_token = spotify_tokens.access_token.clone();
    log::debug!("[POLLING] poll_once: calling get_currently_playing");

    let last_poll_instant = Instant::now();

    let result = get_currently_playing(&access_token);

    match result {
        Ok(Some(track)) => {
            log::info!(
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
            );
            *transient_failure_count = 0;
            PollIteration::Sleep { seconds: sleep_duration }
        }
        Ok(None) => {
            log::info!("[POLLING] poll_once: no track playing");
            let no_track_backoff = handle_no_track(
                app,
                state,
                last_track_key,
                &config,
                last_posted_placeholder,
                last_availability_arm,
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
                            let committed =
                            match cas_refresh_or_discard(
                                "spotify",
                                &mut *state.tokens.spotify_mut(),
                                &pre_refresh_access_token,
                                || Ok(new_tokens.clone()),
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
                                match get_currently_playing(&retry_token) {
                                    Ok(Some(track)) => {
                                        log::info!(
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
                                        );
                                        *transient_failure_count = 0;
                                        return PollIteration::Sleep { seconds: _sleep };
                                    }
                                    Ok(None) => {
                                        log::info!("[POLLING] poll_once: retry no track");
                                        let no_track_backoff = handle_no_track(
                                            app,
                                            state,
                                            last_track_key,
                                            &config,
                                            last_posted_placeholder,
                                            last_availability_arm,
                                        );
                                        *transient_failure_count = 0;
                                        let mut iteration = record_no_track_outcome(
                                            consecutive_pauses,
                                            &config,
                                        );
                                        if let PollIteration::Sleep { seconds } = &mut iteration {
                                            // Issue #154: a throttled Teams
                                            // clear extends the next poll to
                                            // the server-directed delay.
                                            *seconds = (*seconds).max(no_track_backoff);
                                        }
                                        return iteration;
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

            if *transient_failure_count >= TRANSIENT_FAILURE_EXIT_THRESHOLD {
                log::error!("[POLLING] poll_once: 5 consecutive transient failures, exiting and requiring reconnect");
                let _ = app.emit("reconnect-required", json!(null));
                return PollIteration::Break;
            }

            emit_error(
                app,
                "spotify",
                format!("Failed to get currently playing: {}", final_err),
                ErrorSeverity::Warning,
            );
            interruptible_sleep(stop_rx, backoff_secs, "backoff sleep")
        }
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

fn interruptible_sleep(
    stop_rx: &mpsc::Receiver<()>,
    seconds: u64,
    label: &str,
) -> PollIteration {
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

enum CasOutcome<T> {
    Committed(T),
    Discarded { current: Option<T> },
    RefreshFailed(String),
}

fn cas_refresh_or_discard<T, F, G>(
    label: &str,
    lock: &mut Option<T>,
    pre_refresh_access_token: &str,
    refresh_fn: F,
    access_token_of: G,
) -> CasOutcome<T>
where
    T: Clone,
    F: FnOnce() -> Result<T, String>,
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

/// True when the Available-presence session should be re-armed (issue
/// #3.0-P1): no arm yet, or the last arm is at least
/// `AVAILABILITY_REARM_SECONDS` old. Available sessions FADE after 5
/// minutes regardless of `expirationDuration`, so the re-arm cadence must
/// be strictly inside that window (4 min < 5 min).
fn should_rearm_availability(last_arm: Option<Instant>, now: Instant) -> bool {
    match last_arm {
        Some(arm) => now.duration_since(arm).as_secs() >= AVAILABILITY_REARM_SECONDS,
        None => true,
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

    let track_key = format!("{} - {}", track.title, track.artist);
    let changed = last_track_key.as_ref() != Some(&track_key);

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

    let teams_tokens = state.tokens.teams().clone();

    let teams_tokens = if let Some(ref tok) = teams_tokens {
        let expired = is_teams_token_expired(tok);
        if expired {
            log::info!("[POLLING] process_track: Teams token expired, refreshing...");

            let pre_refresh_access_token = tok.access_token.clone();

            let teams_refresh_outcome = cas_refresh_or_discard(
                "teams",
                &mut *state.tokens.teams_mut(),
                &pre_refresh_access_token,
                || refresh_teams_token(tok).map_err(|e| e.to_string()),
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
                            "[POLLING] poll_once: failed to persist refreshed teams tokens: {}",
                            e
                        );
                    }
                    Some(new_tokens)
                }
                CasOutcome::Discarded { current } => current,
                CasOutcome::RefreshFailed(e) => {
                    log::error!("[POLLING] process_track: Failed to refresh Teams token: {}", e);
                    *state.tokens.teams_mut() = None;
                    let _ = app.emit("teams-reconnect-required", json!(null));
                    None
                }
            }
        } else {
            teams_tokens
        }
    } else {
        teams_tokens
    };

    let should_skip_api_call = if changed {
        if let Some(last_update) = last_teams_update {
            (last_update.elapsed().as_millis() as u64) < DEBOUNCE_MS
        } else {
            false
        }
    } else {
        false
    };

    if let Some(teams_tok) = teams_tokens {
        if track.is_playing {
            *consecutive_pauses = 0;
            // Issue #155: a real track replaces any placeholder, so the next
            // pause/no-track must post a fresh placeholder again.
            *last_posted_placeholder = None;

            // P2 (issue #3.0-P2): presence-aware gating. On a track change,
            // read the user's Teams presence; when busy/DND/in a
            // meeting/call/presenting, suppress the status write for the
            // whole track (recorded in `gated_track_key`) and emit
            // `presence-gated`. Evaluated before the debounce so a rapid
            // track change can't bypass the gate. Fail-safe: a failed read
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

            if gated_track_key.as_deref() == Some(track_key.as_str()) {
                log::debug!(
                    "[POLLING] process_track: track presence-gated, skipping status write"
                );
                let remaining_ms = corrected_progress_ms
                    .map(|c| track.duration_ms.saturating_sub(c));
                return playing_track_sleep(remaining_ms, config);
            }

            if should_skip_api_call {
                log::debug!(
                    "[POLLING] process_track: debounce active, skipping Teams API call (changed={}, elapsed={}ms)",
                    changed,
                    last_teams_update.map(|i| i.elapsed().as_millis() as u64).unwrap_or(0)
                );
                let remaining_ms = corrected_progress_ms
                    .map(|c| track.duration_ms.saturating_sub(c));
                return playing_track_sleep(remaining_ms, config);
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

            let remaining_ms = corrected_progress_ms
                .map(|c| track.duration_ms.saturating_sub(c));
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
                    let _ = app.emit(
                        "presence-updated",
                        json!({
                            "status": final_status,
                            "timestamp": Utc::now().to_rfc3339()
                        }),
                    );
                }
                Err(e) => {
                    log::error!("[POLLING] process_track: Failed to set Teams status: {}", e);
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
                log::debug!("[POLLING] process_track: paused placeholder unchanged, skipping clear POST");
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
                            teams_backoff_secs =
                                teams_backoff_secs.max(rate_limit_sleep_secs(&e));
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
pub(crate) fn handle_no_track(
    app: &AppHandle,
    state: &Arc<AppState>,
    last_track_key: &mut Option<String>,
    config: &Option<crate::config::AppConfig>,
    last_posted_placeholder: &mut Option<String>,
    last_availability_arm: &mut Option<Instant>,
) -> u64 {
    if last_track_key.is_some() {
        *last_track_key = None;
        *state.polling.current_track_mut() = None;
    } else {
        return 0;
    }

    let teams_tokens = state.tokens.teams().clone();
    let teams_tok = match teams_tokens {
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
        log::debug!("[POLLING] handle_no_track: no-track placeholder unchanged, skipping clear POST");
        return teams_backoff_secs;
    }

    let expiry_str = placeholder_expiry_str();
    match clear_teams_status_message(
        &teams_tok.access_token,
        placeholder,
        Some(&expiry_str),
    ) {
        Ok(_) => {
            *last_posted_placeholder = Some(placeholder.to_string());
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
            // Issue #154: honor the server's Retry-After on a throttled clear.
            teams_backoff_secs.max(rate_limit_sleep_secs(&e))
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
    let mut rng = rand::thread_rng();
    let jitter_range = base_secs as f64 * 0.2;
    let jitter = rng.gen_range(-jitter_range..=jitter_range);
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
        assert_eq!(
            discard_count, 1,
            "poll_once.rs must have exactly one CAS-discard log line. Found {}.",
            discard_count
        );
        let helper_def = prod_source.matches("fn cas_refresh_or_discard").count();
        assert_eq!(helper_def, 1, "helper defined {} times", helper_def);
        let helper_call_count = prod_source.matches("cas_refresh_or_discard(").count();
        // Expect 3 calls: Spotify proactive, Spotify 401-retry, Teams.
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
                || Ok(new_tokens.clone()),
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

        let committed = rx
            .recv_timeout(Duration::from_secs(10))
            .expect(
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
            .split("fn cas_refresh_or_discard<T, F, G>(")
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
        // returns (guard provably dropped): the two invalid_grant clear paths
        // (proactive + 401-retry) plus the three refresh-success call sites
        // (Spotify proactive, Spotify 401-retry, Teams).
        let persist_count = prod_source.matches("token_io::persist_tokens(").count();
        assert_eq!(
            persist_count, 5,
            "expected exactly 5 persist_tokens call sites in production (2 invalid_grant \
             clear (proactive + 401-retry) + 3 refresh-success call sites); found {}. If a \
             call-site persist is removed, refreshed tokens stop being flushed to disk; if \
             one is added inside cas_refresh_or_discard, the #180 self-deadlock returns.",
            persist_count
        );
    }

    /// Regression guard for issue #72 drift point #1: the two no-track
    /// code paths (main `Ok(None)` arm and 401-retry `Ok(None)` arm)
    /// must both funnel through `record_no_track_outcome` so they
    /// cannot drift apart.
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
        // Call sites of `record_no_track_outcome(` in production code.
        // We don't need to subtract the `fn` definition because the
        // `fn record_no_track_outcome(` definition includes the
        // open-paren but is on its own line in the source, so it WILL
        // match the substring. We want exactly 3 matches in prod
        // source: 2 call sites (lines 183 + 249) plus the 1 fn
        // definition (line 309). Anything else is a regression.
        let call_count = prod_source.matches("record_no_track_outcome(").count();
        assert_eq!(
            call_count, 3,
            "Expected 3 occurrences in production (2 call sites: main \
             Ok(None) + 401-retry Ok(None), plus the fn definition). \
             Found {}. If a future contributor adds a third no-track \
             handling site outside the helper, the increment order \
             can drift again. See issue #72 drift point #1.",
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
        assert_eq!(
            canonical_msg_count, 1,
            "expected exactly 1 'Failed to get currently playing:' emit_error; found {}",
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
            .matches("get_currently_playing(&access_token)")
            .count();
        let retry = prod_source
            .matches("get_currently_playing(&retry_token)")
            .count();
        assert_eq!(
            top_level, 1,
            "expected exactly 1 top-level get_currently_playing call; found {}",
            top_level
        );
        assert_eq!(
            retry, 1,
            "expected exactly 1 401-retry get_currently_playing call; found {}",
            retry
        );
    }


    /// Regression guard for issue #60.
    #[test]
    fn test_start_polling_does_not_claim_is_syncing() {
        let source = include_str!("state.rs");
        let body = source
            .split("pub fn start_polling(")
            .nth(1)
            .and_then(|s| {
                s.split("log::info!(\"[POLLING] start_polling: SUCCESS")
                    .next()
            })
            .unwrap_or("");
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
        let raw_count = prod_source.matches(r#"emit("error","#).count();
        assert_eq!(
            raw_count, 0,
            "poll_once.rs must not emit raw \"error\" events directly. Found {}.",
            raw_count
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
        assert_eq!(rate_limit_sleep_secs(&TeamsApiError::RateLimited(Some(90))), 90);
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
    #[test]
    fn test_status_expiry_known_and_unknown_position() {
        let config = Some(crate::config::AppConfig::default());
        let s = status_expiry_str(Some(120_000), &config)
            .expect("known position must yield an expiry");
        assert!(
            !s.contains('+') && !s.contains('Z'),
            "offset leaked into status expiry: {}",
            s
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
}
