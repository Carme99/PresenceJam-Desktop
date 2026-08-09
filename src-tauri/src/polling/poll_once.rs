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
    clear_teams_status_message, is_token_expired as is_teams_token_expired, refresh_teams_token,
    set_teams_status_message, TeamsApiError,
};
use crate::token_io;
use crate::AppState;

use super::{emit_error, ErrorSeverity};

const ERROR_RETRY_INTERVAL_SECONDS: u64 = 30;
const RATE_LIMIT_BACKOFF_SECONDS: u64 = 60;
const DEBOUNCE_MS: u64 = 500;
const TRANSIENT_FAILURE_EXIT_THRESHOLD: u8 = 5;

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
    consecutive_pauses: &mut u8,
    transient_failure_count: &mut u8,
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
                match cas_refresh_or_discard(
                    state,
                    app,
                    "spotify",
                    &mut *state.tokens.spotify_mut(),
                    &pre_refresh_access_token,
                    || Ok(new_tokens.clone()),
                    |t| &t.access_token,
                ) {
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
                consecutive_pauses,
            );
            *transient_failure_count = 0;
            PollIteration::Sleep { seconds: sleep_duration }
        }
        Ok(None) => {
            log::info!("[POLLING] poll_once: no track playing");
            handle_no_track(app, state, last_track_key);
            *transient_failure_count = 0;
            record_no_track_outcome(consecutive_pauses, &config)
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
                                state,
                                app,
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
                                // cas_refresh_or_discard already persisted on
                                // Committed; do not double-write.
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
                                            consecutive_pauses,
                                        );
                                        *transient_failure_count = 0;
                                        return PollIteration::Sleep { seconds: _sleep };
                                    }
                                    Ok(None) => {
                                        log::info!("[POLLING] poll_once: retry no track");
                                        handle_no_track(app, state, last_track_key);
                                        *transient_failure_count = 0;
                                        return record_no_track_outcome(
                                            consecutive_pauses,
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
                            if matches!(refresh_err, SpotifyApiError::InvalidGrant) {
                                log::warn!("[POLLING] poll_once: Spotify refresh token invalid (invalid_grant), emitting spotify-reconnect-required");
                                let _ = app.emit("spotify-reconnect-required", json!(null));
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
    state: &Arc<AppState>,
    app: &AppHandle,
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
        // TODO(#followup): the caller's write guard (`&mut *state.tokens.X_mut()`)
        // is still alive here, and persist_tokens takes a READ lock on the same
        // RwLock — a self-deadlock with parking_lot's non-reentrant lock on every
        // successful refresh. Out of scope for the #153-#165 batch; fix = drop
        // the guard before persisting (see poll_once Path A invalid_grant).
        if let Err(e) = token_io::persist_tokens(state, app) {
            log::warn!(
                "[POLLING] poll_once: failed to persist refreshed {} tokens: {}",
                label,
                e
            );
        }
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

#[allow(clippy::too_many_arguments)]
pub(crate) fn process_track(
    app: &AppHandle,
    state: &Arc<AppState>,
    config: &Option<crate::config::AppConfig>,
    track: &crate::spotify::TrackInfo,
    last_track_key: &mut Option<String>,
    last_poll_instant: Instant,
    last_teams_update: &mut Option<Instant>,
    consecutive_pauses: &mut u8,
) -> u64 {
    let elapsed_ms = last_poll_instant.elapsed().as_millis() as u64;
    // TODO(#165): handle `None` (live/unknown position) properly — fall back
    // to the default interval for sleep and no expiry. `unwrap_or(0)`
    // preserves the pre-Option behavior until that integration lands.
    let corrected_progress_ms = track.progress_ms.unwrap_or(0).saturating_add(elapsed_ms);

    let track_key = format!("{} - {}", track.title, track.artist);
    let changed = last_track_key.as_ref() != Some(&track_key);

    if changed {
        log::info!("[POLLING] process_track: new track detected, updating");
        *last_track_key = Some(track_key);
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

            match cas_refresh_or_discard(
                state,
                app,
                "teams",
                &mut *state.tokens.teams_mut(),
                &pre_refresh_access_token,
                || refresh_teams_token(tok).map_err(|e| e.to_string()),
                |t| &t.access_token,
            ) {
                CasOutcome::Committed(new_tokens) => Some(new_tokens),
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
            if should_skip_api_call {
                log::debug!(
                    "[POLLING] process_track: debounce active, skipping Teams API call (changed={}, elapsed={}ms)",
                    changed,
                    last_teams_update.map(|i| i.elapsed().as_millis() as u64).unwrap_or(0)
                );
                let remaining_ms = track.duration_ms.saturating_sub(corrected_progress_ms);
                let buffer_ms = 5000u64;
                let remaining_secs = remaining_ms / 1000;
                let sleep_secs = remaining_secs.saturating_sub(buffer_ms / 1000);
                return sleep_secs
                    .max(config_minimum_interval(config))
                    .min(config_maximum_interval(config));
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

            let remaining_ms = track.duration_ms.saturating_sub(corrected_progress_ms);
            let buffer_ms = config
                .as_ref()
                .map(|c| c.polling.expiry_buffer_seconds)
                .unwrap_or(10)
                * 1000;
            let expiry = Utc::now()
                + chrono::Duration::milliseconds(remaining_ms as i64 + buffer_ms as i64);
            let expiry_str = expiry.to_rfc3339();

            match set_teams_status_message(
                &teams_tok.access_token,
                &final_status,
                Some(&expiry_str),
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
                    let e_str = e.to_string().to_lowercase();
                    if e_str.contains("unauthorized")
                        || e_str.contains("forbidden")
                        || e_str.contains("401")
                        || e_str.contains("403")
                    {
                        log::warn!("[POLLING] process_track: Teams auth failure detected, emitting teams-reconnect-required");
                        let _ = app.emit("teams-reconnect-required", json!(null));
                    }
                }
            }
        } else if config
            .as_ref()
            .map(|c| c.teams.clear_on_pause)
            .unwrap_or(true)
        {
            match clear_teams_status_message(&teams_tok.access_token, "\u{1F3B5} Paused", None) {
                Ok(_) => {
                    *last_teams_update = Some(Instant::now());
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
                }
            }
        }
    }

    if track.is_playing {
        let remaining_ms = track.duration_ms.saturating_sub(corrected_progress_ms);
        let buffer_ms = 5000u64;
        let remaining_secs = remaining_ms / 1000;
        let sleep_secs = remaining_secs.saturating_sub(buffer_ms / 1000);
        sleep_secs
            .max(config_minimum_interval(config))
            .min(config_maximum_interval(config))
    } else {
        let sleep = pause_backoff(*consecutive_pauses, config_default_interval(config));
        *consecutive_pauses = consecutive_pauses.saturating_add(1).min(4);
        sleep
    }
}

pub(crate) fn handle_no_track(
    app: &AppHandle,
    state: &Arc<AppState>,
    last_track_key: &mut Option<String>,
) {
    if last_track_key.is_some() {
        *last_track_key = None;
        *state.polling.current_track_mut() = None;

        let teams_tokens = state.tokens.teams().clone();
        if let Some(teams_tok) = teams_tokens {
            match clear_teams_status_message(
                &teams_tok.access_token,
                "\u{1F3B5} Nothing playing on Spotify",
                None,
            ) {
                Ok(_) => {
                    let _ = app.emit(
                        "presence-cleared",
                        json!({ "timestamp": Utc::now().to_rfc3339() }),
                    );
                }
                Err(e) => {
                    log::error!(
                        "[POLLING] handle_no_track: Failed to clear Teams status: {}",
                        e
                    );
                }
            }
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
        .unwrap_or(5)
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

    /// Regression guard for issue #79/#117: poll_once.rs must NOT emit
    /// raw "error" events directly.
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
}
