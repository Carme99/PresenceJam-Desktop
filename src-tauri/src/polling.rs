use chrono::Utc;
use rand::Rng;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration as StdDuration, Instant};
use tauri::{AppHandle, Emitter};
use tauri_plugin_store::StoreExt;

use crate::profanity;
use crate::spotify::{
    format_status, get_currently_playing, is_token_expired, refresh_spotify_token, SpotifyApiError,
    SpotifyTokens,
};
use crate::teams::{clear_teams_status_message, refresh_teams_token, set_teams_status_message, TeamsTokens};
use crate::{tray, AppState};

const ERROR_RETRY_INTERVAL_SECONDS: u64 = 30;
const RATE_LIMIT_BACKOFF_SECONDS: u64 = 60;
const DEBOUNCE_MS: u64 = 500;

/// Read the configured default poll interval (with a hard fallback).
fn config_default_interval(config: &Option<crate::config::AppConfig>) -> u64 {
    config
        .as_ref()
        .map(|c| c.polling.default_interval_seconds)
        .unwrap_or(30)
}

/// Read the configured minimum poll interval (with a hard fallback).
fn config_minimum_interval(config: &Option<crate::config::AppConfig>) -> u64 {
    config
        .as_ref()
        .map(|c| c.polling.minimum_interval_seconds)
        .unwrap_or(5)
}

/// Read the configured maximum poll interval (with a hard fallback).
fn config_maximum_interval(config: &Option<crate::config::AppConfig>) -> u64 {
    config
        .as_ref()
        .map(|c| c.polling.max_interval_seconds)
        .unwrap_or(60)
}

/// Pause-aware exponential backoff. When the user has paused Spotify (or has
/// nothing playing at all), the polling loop would otherwise hammer the
/// currently-playing endpoint every `default_secs` for hours. This grows the
/// sleep on consecutive empty/paused responses: `default_secs` → 2× → 4× → 300s
/// (cap).
///
/// The first arm returns the configured default so the user's Settings value
/// (`AppConfig.polling.default_interval_seconds`) actually takes effect at
/// runtime. Arms 1 and 2 double and quadruple, but are clamped to the same
/// 300s ceiling to keep the cap behaviour stable for users with large
/// configured defaults.
///
/// The counter is reset to 0 the moment we see a non-paused, non-empty
/// response, so resuming playback immediately goes back to the normal cadence.
///
/// See issue #38.
fn pause_backoff(consecutive_pauses: u8, default_secs: u64) -> u64 {
    match consecutive_pauses {
        0 => default_secs,
        1 => default_secs.saturating_mul(2).min(300),
        2 => default_secs.saturating_mul(4).min(300),
        _ => 300,
    }
}

/// Adds +/- 20% jitter to retry intervals to prevent thundering herd.
/// See issue #17.
/// Uses thread-local RNG to avoid per-call initialization overhead.
fn with_jitter(base_secs: u64) -> u64 {
    let mut rng = rand::thread_rng();
    let jitter_range = base_secs as f64 * 0.2;
    let jitter = rng.gen_range(-jitter_range..=jitter_range);
    (base_secs as f64 + jitter).max(1.0) as u64
}

// process_track is intentionally a single helper that owns the per-poll
// mutable state in one place. Threading the same fields through every
// call site would either duplicate them across the polling loop or
// require a new "PollContext" struct that mirrors the existing parameter
// list. The signature has grown because the polling loop has grown;
// refactoring is a separate concern from PR-time CI hygiene.
#[allow(clippy::too_many_arguments)]
fn process_track(
    app: &AppHandle,
    state: &Arc<AppState>,
    config: &Option<crate::config::AppConfig>,
    track: &crate::spotify::TrackInfo,
    last_track_key: &mut Option<String>,
    last_poll_instant: Instant,
    last_teams_update: &mut Option<Instant>,
    consecutive_pauses: &mut u8,
) -> u64 {
    // Bug 13: Correct progress_ms for elapsed time since last poll
    let elapsed_ms = last_poll_instant.elapsed().as_millis() as u64;
    let corrected_progress_ms = track.progress_ms.saturating_add(elapsed_ms);

    let track_key = format!("{} - {}", track.title, track.artist);
    let changed = last_track_key.as_ref() != Some(&track_key);

    if changed {
        log::info!("[POLLING] process_track: new track detected, updating");
        *last_track_key = Some(track_key);
        *state.current_track.write() = Some(track.clone());

        let _ = app.emit(
            "spotify-track-changed",
            serde_json::json!({
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

    let teams_tokens = {
        let guard = state.teams_tokens.read();
        guard.clone()
    };

    // Check if Teams token is expired and refresh if needed (similar to Spotify token refresh)
    let teams_tokens = if let Some(ref tok) = teams_tokens {
        let expired = Utc::now() >= tok.expires_at - chrono::Duration::seconds(60);
        if expired {
            log::info!("[POLLING] process_track: Teams token expired, refreshing...");

            // Snapshot the access token before refresh so we can detect a
            // concurrent writer (e.g. user clicked Reconnect) that mutated
            // state between our read and our eventual write.
            let pre_refresh_access_token = tok.access_token.clone();

            match refresh_teams_token(tok) {
                Ok(new_tokens) => {
                    log::info!("[POLLING] process_track: Teams token refresh SUCCESS");
                    // CAS: only commit if state still holds the access token
                    // we refreshed from. If state changed during the refresh
                    // (e.g. user clicked Reconnect), discard the result.
                    let committed = {
                        let mut guard = state.teams_tokens.write();
                        if guard.as_ref().map(|t| &t.access_token)
                            == Some(&pre_refresh_access_token)
                        {
                            *guard = Some(new_tokens.clone());
                            true
                        } else {
                            log::warn!(
                                "[POLLING] process_track: teams state changed during refresh, discarding result"
                            );
                            false
                        }
                    };
                    if committed {
                        if let Err(e) = save_teams_tokens(app, &new_tokens) {
                            log::warn!("[POLLING] process_track: failed to persist refreshed teams tokens: {}", e);
                        }
                        Some(new_tokens)
                    } else {
                        // CAS failed: a concurrent writer (Reconnect, a winning
                        // refresh_teams command) mutated state. Re-read the
                        // in-memory state and return that. If it's None, the
                        // outer `if let Some(teams_tok)` correctly skips the
                        // Teams API call. If it's Some(other), a different
                        // writer won the race and we use those tokens instead
                        // of the discarded refresh.
                        state.teams_tokens.read().clone()
                    }
                }
                Err(e) => {
                    log::error!("[POLLING] process_track: Failed to refresh Teams token: {}", e);
                    // Clear the tokens so we don't keep trying with expired ones
                    *state.teams_tokens.write() = None;
                    let _ = app.emit("teams-reconnect-required", serde_json::json!(null));
                    None
                }
            }
        } else {
            teams_tokens
        }
    } else {
        teams_tokens
    };

    // Bug 17: Debounce Teams API calls to prevent showing stale track info
    // when skipping through tracks quickly. Skip the API call if:
    // - The track changed, AND
    // - Less than DEBOUNCE_MS has passed since the last Teams update
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
            // Resuming playback — reset the pause-backoff counter before the
            // debounce early-return below, so a track that resumes inside the
            // 500ms debounce window doesn't carry a stale backoff level into
            // the next pause cycle.
            *consecutive_pauses = 0;
            if should_skip_api_call {
                log::debug!(
                    "[POLLING] process_track: debounce active, skipping Teams API call (changed={}, elapsed={}ms)",
                    changed,
                    last_teams_update.map(|i| i.elapsed().as_millis() as u64).unwrap_or(0)
                );
                // Still return a reasonable sleep duration based on track progress
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
                .unwrap_or("🎵 {artist} - {track} 🎧");
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
            let expiry =
                Utc::now() + chrono::Duration::milliseconds(remaining_ms as i64 + buffer_ms as i64);
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
                        serde_json::json!({
                            "status": final_status,
                            "timestamp": Utc::now().to_rfc3339()
                        }),
                    );
                }
                Err(e) => {
                    log::error!("[POLLING] process_track: Failed to set Teams status: {}", e);
                    let _ = app.emit(
                        "error",
                        serde_json::json!({
                            "source": "teams",
                            "message": format!("Failed to update status: {}", e)
                        }),
                    );
                    // Emit reconnect-required so the frontend knows to re-auth Teams
                    let e_str = e.to_lowercase();
                    if e_str.contains("unauthorized") || e_str.contains("forbidden") || e_str.contains("401") || e_str.contains("403") {
                        log::warn!("[POLLING] process_track: Teams auth failure detected, emitting teams-reconnect-required");
                        let _ = app.emit("teams-reconnect-required", serde_json::json!(null));
                    }
                }
            }
        } else if config.as_ref().map(|c| c.teams.clear_on_pause).unwrap_or(true) {
            // Issue #39: removed the `is_first_poll` guard. It was preventing
            // legitimate status-clearing on app start for the first 30s when
            // the user opens PresenceJam while Spotify is already paused.
            // The (small) risk of wiping a status the user set manually before
            // opening the app is acceptable: a stale \"now playing\" status
            // is a worse UX than a missing \"paused\" status, and the user can
            // just hit play/pause to re-sync.
            match clear_teams_status_message(&teams_tok.access_token, "🎵 Paused") {
                Ok(_) => {
                    *last_teams_update = Some(Instant::now());
                    let _ = app.emit(
                        "presence-cleared",
                        serde_json::json!({ "timestamp": Utc::now().to_rfc3339() }),
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
        // The consecutive_pauses counter was already reset earlier in
        // process_track (top of the is_playing branch inside the teams_tokens
        // block) so the debounce early-return can't leave it stale.
        let remaining_ms = track.duration_ms.saturating_sub(corrected_progress_ms);
        let buffer_ms = 5000u64;
        let remaining_secs = remaining_ms / 1000;
        let sleep_secs = remaining_secs.saturating_sub(buffer_ms / 1000);
        sleep_secs
            .max(config_minimum_interval(config))
            .min(config_maximum_interval(config))
    } else {
        // Track present but paused — grow the sleep on consecutive pauses
        // so a paused user doesn't trigger 720 calls/day.
        let sleep = pause_backoff(*consecutive_pauses, config_default_interval(config));
        *consecutive_pauses = consecutive_pauses.saturating_add(1).min(4);
        sleep
    }
}

fn handle_no_track(app: &AppHandle, state: &Arc<AppState>, last_track_key: &mut Option<String>) {
    if last_track_key.is_some() {
        *last_track_key = None;
        *state.current_track.write() = None;

        let teams_tokens = {
            let guard = state.teams_tokens.read();
            guard.clone()
        };
        if let Some(teams_tok) = teams_tokens {
            match clear_teams_status_message(&teams_tok.access_token, "🎵 Nothing playing on Spotify") {
                Ok(_) => {
                    let _ = app.emit(
                        "presence-cleared",
                        serde_json::json!({ "timestamp": Utc::now().to_rfc3339() }),
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

pub fn start_polling(
    state: Arc<AppState>,
    app: AppHandle,
) -> Result<thread::JoinHandle<()>, String> {
    log::info!("[POLLING] start_polling: ENTRY");

    // Guard against spawning duplicate polling thread.
    // Use compare_exchange for an atomic check-and-set: replaces the
    // previous RwLock write guard. AcqRel on success preserves the
    // happens-before relationship with subsequent reads of is_syncing
    // (e.g. the polling loop and tray).
    // This mirrors commands.rs:start_syncing which uses the same
    // compare_exchange. The polling loop only reads is_syncing
    // (via is_syncing.load(Acquire)) so concurrent read access is
    // unaffected.
    if state
        .is_syncing
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        log::warn!("[POLLING] start_polling: polling already running, returning early");
        return Err("Polling is already running".to_string());
    }

    log::info!("[POLLING] start_polling: is_syncing flag set to true");

    // Create interruptible stop channel so stop_syncing can wake the thread immediately.
    // See issue #10 (Polling thread cannot be cancelled mid-request).
    let (stop_tx, stop_rx) = mpsc::channel::<()>();

    {
        let mut tx_guard = state.stop_tx.write();
        *tx_guard = Some(stop_tx);
    }

    // Clone Arc for the thread
    let state_clone = Arc::clone(&state);
    let state_for_cleanup = Arc::clone(&state);
    let app_clone = app.clone();

    let handle = thread::Builder::new()
        .name("presencejam-polling".to_string())
        .stack_size(1024 * 1024) // 1MB stack for safety
        .spawn(move || {
            log::info!("[POLLING] start_polling: thread started");
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                polling_loop(state_clone, app_clone, stop_rx);
            }));
            if let Err(panic_info) = result {
                if let Some(s) = panic_info.downcast_ref::<&str>() {
                    log::error!(
                        "[POLLING] start_polling: polling_loop panicked with &str: {}",
                        s
                    );
                } else if let Some(s) = panic_info.downcast_ref::<String>() {
                    log::error!(
                        "[POLLING] start_polling: polling_loop panicked with String: {}",
                        s
                    );
                } else {
                    log::error!(
                        "[POLLING] start_polling: polling_loop panicked with non-string payload"
                    );
                }
                // app_clone is moved into polling_loop above, so use app here
                let _ = app.emit("polling-thread-panicked", serde_json::json!(null));
                // Reset state so a panic doesn't wedge the app in "syncing"
                state_for_cleanup.is_syncing.store(false, Ordering::Release);
                *state_for_cleanup.stop_tx.write() = None;
            }
            log::info!("[POLLING] start_polling: thread ended");
        })
        .map_err(|e| {
            log::error!("[POLLING] start_polling: thread spawn failed - {}", e);
            // Reset is_syncing so future start_polling calls are not permanently wedged.
            state.is_syncing.store(false, Ordering::Release);
            // Also clean up the stop channel sender we just stored.
            *state.stop_tx.write() = None;
            format!("Failed to spawn polling thread: {}", e)
        })?;

    log::info!("[POLLING] start_polling: SUCCESS - handle returned");
    Ok(handle)
}

/// Returns the (client_id, client_secret) pair needed to call Spotify APIs.
///
/// The `client_id` is read from the in-memory AppState config. The
/// `client_secret` is read from the OS keychain (see issue #9). If the
/// keychain entry is missing, the returned `client_secret` is an empty
/// string — callers should treat that as a "user must re-onboard" signal.
fn get_spotify_credentials(config: &Option<crate::config::AppConfig>) -> (String, String) {
    let client_id = config
        .as_ref()
        .map(|c| c.spotify.client_id.clone())
        .unwrap_or_default();
    let client_secret = crate::keychain::get_spotify_client_secret().unwrap_or_default();
    (client_id, client_secret)
}

fn polling_loop(state: Arc<AppState>, app: AppHandle, stop_rx: mpsc::Receiver<()>) {
    log::info!("[POLLING] polling_loop: STARTED");
    let mut last_track_key: Option<String> = None;
    let mut last_teams_update: Option<Instant> = None;
    let mut transient_failure_count: u8 = 0;
    // Tracks consecutive empty/paused responses so we can widen the poll
    // interval (30→60→120→300s) instead of hammering the API on a paused user.
    // See issue #38.
    let mut consecutive_pauses: u8 = 0;

    loop {
        log::debug!("[POLLING] polling_loop: iteration start");

        // Check if we should stop — use interruptible channel so stop_syncing
        // can wake the thread immediately instead of waiting for the sleep to expire.
        // Also check the is_syncing flag for external stop requests.
        match stop_rx.recv_timeout(StdDuration::ZERO) {
            Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                log::info!("[POLLING] polling_loop: stop signal received, breaking loop");
                break;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // Continue if no stop signal
            }
        }
        {
            let is_syncing = state.is_syncing.load(Ordering::Acquire);
            if !is_syncing {
                log::info!("[POLLING] polling_loop: is_syncing=false, breaking loop");
                break;
            }
        }

        // Get config
        let config = {
            let guard = state.config.read();
            guard.clone()
        };
        log::debug!("[POLLING] polling_loop: config loaded");

        // Get Spotify tokens
        let spotify_tokens = {
            let guard = state.spotify_tokens.read();
            guard.clone()
        };
        log::debug!(
            "[POLLING] polling_loop: spotify_tokens: {}",
            if spotify_tokens.is_some() {
                "Some"
            } else {
                "None"
            }
        );

        let spotify_tokens = match spotify_tokens {
            Some(t) => {
                log::debug!("[POLLING] polling_loop: using existing Spotify tokens");
                t
            }
            None => {
                log::warn!("[POLLING] polling_loop: No Spotify tokens available, waiting...");
                match stop_rx.recv_timeout(StdDuration::from_secs(with_jitter(ERROR_RETRY_INTERVAL_SECONDS))) {
                    Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        log::info!("[POLLING] polling_loop: stop signal during no-token sleep, breaking");
                        break;
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                }
                continue;
            }
        };

        // Check if token expired
        let token_expired = is_token_expired(&spotify_tokens);
        log::debug!("[POLLING] polling_loop: token_expired={}", token_expired);

        // Refresh token if needed
        let (client_id, client_secret) = get_spotify_credentials(&config);
        let spotify_tokens = if token_expired {
            log::info!("[POLLING] polling_loop: Spotify token expired, refreshing...");
            log::info!(
                "[POLLING] polling_loop: refreshing with client_id.len={}",
                client_id.len()
            );

            // Snapshot the access token before refresh so we can detect a
            // concurrent writer (e.g. user clicked Reconnect) that mutated
            // state between our read and our eventual write.
            let pre_refresh_access_token = spotify_tokens.access_token.clone();

            match refresh_spotify_token(&spotify_tokens, &client_id, &client_secret) {
                Ok(new_tokens) => {
                    log::info!("[POLLING] polling_loop: token refresh SUCCESS");
                    // CAS: only commit if state still holds the access token
                    // we refreshed from. If state changed during the refresh
                    // (e.g. user clicked Reconnect), discard the result.
                    let committed = {
                        let mut guard = state.spotify_tokens.write();
                        if guard.as_ref().map(|t| &t.access_token)
                            == Some(&pre_refresh_access_token)
                        {
                            *guard = Some(new_tokens.clone());
                            true
                        } else {
                            log::warn!(
                                "[POLLING] polling_loop: spotify state changed during refresh, discarding result"
                            );
                            false
                        }
                    };
                    if committed {
                        // Persist refreshed tokens to store so they survive app restarts
                        if let Err(e) = save_spotify_tokens(&app, &new_tokens) {
                            log::warn!("[POLLING] polling_loop: failed to persist refreshed tokens: {}", e);
                        }
                        new_tokens
                    } else {
                        // CAS failed: re-read in-memory state. If a different
                        // writer won the race (e.g. refresh_spotify command
                        // raced with us), use their tokens. If state was
                        // cleared (Reconnect), short-circuit and re-poll on
                        // the next iteration rather than using the discarded
                        // refresh — the caller dereferences spotify_tokens
                        // unconditionally a few lines below.
                        match state.spotify_tokens.read().clone() {
                            Some(t) => t,
                            None => {
                                log::info!(
                                    "[POLLING] polling_loop: state cleared during refresh, waiting and re-polling"
                                );
                                match stop_rx.recv_timeout(StdDuration::from_secs(with_jitter(ERROR_RETRY_INTERVAL_SECONDS))) {
                                    Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                                        log::info!("[POLLING] polling_loop: stop signal during CAS-fail sleep, breaking");
                                        break;
                                    }
                                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                                        continue;
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    log::error!(
                        "[POLLING] polling_loop: Failed to refresh Spotify token: {}",
                        e
                    );
                    let _ = app.emit(
                        "error",
                        serde_json::json!({
                            "source": "spotify",
                            "message": format!("Token refresh failed: {}", e)
                        }),
                    );
                    log::info!("[POLLING] polling_loop: EMIT error event");
                    match stop_rx.recv_timeout(StdDuration::from_secs(with_jitter(ERROR_RETRY_INTERVAL_SECONDS))) {
                        Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                            log::info!("[POLLING] polling_loop: stop signal during error retry sleep, breaking");
                            break;
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                            continue;
                        }
                    }
                }
            }
        } else {
            spotify_tokens
        };

        let access_token = spotify_tokens.access_token.clone();
        log::debug!("[POLLING] polling_loop: calling get_currently_playing");

        // Bug 13: Capture instant before API call to correct progress_ms elapsed time
        let last_poll_instant = Instant::now();

        // Get currently playing track (blocking call)
        let result = get_currently_playing(&access_token);

        match result {
            Ok(Some(track)) => {
                log::info!(
                    "[POLLING] polling_loop: track found - {} by {}",
                    track.title,
                    track.artist
                );
                let sleep_duration =
                    process_track(&app, &state, &config, &track, &mut last_track_key, last_poll_instant, &mut last_teams_update, &mut consecutive_pauses);
                transient_failure_count = 0;
                // Update tray menu with current sync state and track info (Bug 24+25 fix)
                let is_syncing = state.is_syncing.load(Ordering::Acquire);
                let current_track = state.current_track.read().clone();
                if let Err(e) = tray::update_tray_menu(&app, is_syncing, current_track) {
                    log::warn!("[POLLING] polling_loop: failed to update tray menu: {}", e);
                }
                log::debug!(
                    "[POLLING] polling_loop: sleeping for {} seconds",
                    sleep_duration
                );
                // Use interruptible sleep: wait for either a stop signal or timeout
                match stop_rx.recv_timeout(StdDuration::from_secs(sleep_duration)) {
                    Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        log::info!("[POLLING] polling_loop: stop signal during track sleep, breaking");
                        break;
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        // Normal timeout — continue to next poll
                    }
                }
            }
            Ok(None) => {
                log::info!("[POLLING] polling_loop: no track playing");
                handle_no_track(&app, &state, &mut last_track_key);
                transient_failure_count = 0;
                // Update tray menu with current sync state and track info (Bug 24+25 fix)
                let is_syncing = state.is_syncing.load(Ordering::Acquire);
                let current_track = state.current_track.read().clone();
                if let Err(e) = tray::update_tray_menu(&app, is_syncing, current_track) {
                    log::warn!("[POLLING] polling_loop: failed to update tray menu: {}", e);
                }
                // Apply pause-aware exponential backoff: default→2×→4×→300s.
                // The counter is reset by process_track on the first playing track.
                let no_track_sleep = pause_backoff(consecutive_pauses, config_default_interval(&config));
                consecutive_pauses = consecutive_pauses.saturating_add(1).min(4);
                log::info!(

                    "[POLLING] polling_loop: sleeping for {} seconds (no track)",
                    no_track_sleep
                );
                match stop_rx.recv_timeout(StdDuration::from_secs(no_track_sleep)) {
                    Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        log::info!("[POLLING] polling_loop: stop signal during no-track sleep, breaking");
                        break;
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                }
            }
            Err(e) => {
                log::error!(
                    "[POLLING] polling_loop: Failed to get currently playing track: {}",
                    e
                );

                let mut final_err = e;
                let mut backoff_secs = with_jitter(ERROR_RETRY_INTERVAL_SECONDS);

                if matches!(final_err, SpotifyApiError::ExpiredToken) {
                    log::info!("[POLLING] polling_loop: token expired, attempting refresh");

                    if !client_id.is_empty() && !client_secret.is_empty() {
                        let current_tokens = {
                            let guard = state.spotify_tokens.read();
                            guard.clone()
                        };

                        if let Some(tokens) = current_tokens {
                            // Snapshot the access token before refresh so we can
                            // detect a concurrent writer (e.g. user clicked
                            // Reconnect, or the refresh_spotify command) that
                            // mutated state between our read and the write below.
                            let pre_refresh_access_token = tokens.access_token.clone();

                            match refresh_spotify_token(&tokens, &client_id, &client_secret) {
                                Ok(new_tokens) => {
                                    log::info!(
                                        "[POLLING] polling_loop: token refresh SUCCESS, retrying"
                                    );
                                    // CAS: only commit if state still holds the
                                    // access token we refreshed from. If state
                                    // changed during the refresh (e.g. user
                                    // clicked Reconnect), discard the result.
                                    let committed = {
                                        let mut guard = state.spotify_tokens.write();
                                        if guard.as_ref().map(|t| &t.access_token)
                                            == Some(&pre_refresh_access_token)
                                        {
                                            *guard = Some(new_tokens.clone());
                                            true
                                        } else {
                                            log::warn!(
                                                "[POLLING] polling_loop: 401-retry: spotify state changed during refresh, discarding result"
                                            );
                                            false
                                        }
                                    };
                                    if committed {
                                        // Persist refreshed tokens so they survive app restarts
                                        if let Err(e) = save_spotify_tokens(&app, &new_tokens) {
                                            log::warn!("[POLLING] polling_loop: failed to persist refreshed tokens: {}", e);
                                        }
                                        let retry_token = new_tokens.access_token.clone();
                                                // Bug 13: Capture instant before API call to correct progress_ms elapsed time
                                                let last_poll_instant = Instant::now();
                                        match get_currently_playing(&retry_token) {
                                        Ok(Some(track)) => {
                                            log::info!(
                                                "[POLLING] polling_loop: retry track found - {} by {}",
                                                track.title,
                                                track.artist
                                            );
                                            let _sleep = process_track(
                                                &app,
                                                &state,
                                                &config,
                                                &track,
                                                &mut last_track_key,
                                                last_poll_instant,
                                                &mut last_teams_update,
                                                &mut consecutive_pauses,
                                            );
                                            transient_failure_count = 0;
                                            continue;
                                        }
                                        Ok(None) => {
                                            log::info!("[POLLING] polling_loop: retry no track");
                                            handle_no_track(&app, &state, &mut last_track_key);
                                            let retry_no_track_sleep = pause_backoff(consecutive_pauses, config_default_interval(&config));
                                            consecutive_pauses = consecutive_pauses.saturating_add(1).min(4);
                                            match stop_rx.recv_timeout(StdDuration::from_secs(retry_no_track_sleep)) {
                                                Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                                                    log::info!("[POLLING] polling_loop: stop signal during retry no-track sleep, breaking");
                                                    break;
                                                }
                                                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                                            }
                                            transient_failure_count = 0;
                                            continue;
                                        }
                                        Err(retry_err) => {
                                            log::error!("[POLLING] polling_loop: retry after refresh also failed: {}", retry_err);
                                            final_err = retry_err;
                                        }
                                    }
                                    } // close if committed
                                }
                                Err(refresh_err) => {
                                    log::error!(
                                        "[POLLING] polling_loop: token refresh failed: {}",
                                        refresh_err
                                    );
                                    // Permanent failure after refresh retry — require re-auth
                                    log::warn!("[POLLING] polling_loop: Spotify token refresh permanently failed, emitting spotify-reconnect-required");
                                    let _ = app.emit("spotify-reconnect-required", serde_json::json!(null));
                                    final_err = SpotifyApiError::Other(refresh_err.to_string());
                                }
                            }
                        }
                    }
                }

                if matches!(final_err, SpotifyApiError::RateLimited) {
                    backoff_secs = with_jitter(RATE_LIMIT_BACKOFF_SECONDS);
                }

                // Only count truly transient errors toward the retry limit.
                // ExpiredToken triggers a refresh/retry above; if we reach here the
                // retry failed — still count it as transient so the loop can give up.
                // Auth/Other errors are non-transient and do not contribute.
                if matches!(final_err, SpotifyApiError::RateLimited | SpotifyApiError::ExpiredToken) {
                    transient_failure_count += 1;
                }

                // After 5 consecutive transient failures, exit and require reconnect
                if transient_failure_count >= 5 {
                    log::error!("[POLLING] polling_loop: 5 consecutive transient failures, exiting and requiring reconnect");
                    let _ = app.emit("reconnect-required", serde_json::json!(null));
                    break;
                }

                let _ = app.emit(
                    "error",
                    serde_json::json!({
                        "source": "spotify",
                        "message": format!("Failed to get currently playing: {}", final_err)
                    }),
                );
                log::info!("[POLLING] polling_loop: EMIT error event");
                match stop_rx.recv_timeout(StdDuration::from_secs(backoff_secs)) {
                    Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        log::info!("[POLLING] polling_loop: stop signal during backoff sleep, breaking");
                        break;
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                }
            }
        }
    }

    log::info!("[POLLING] polling_loop: ENDED");
}

pub fn stop_polling(state: &AppState) {
    log::info!("[POLLING] stop_polling: ENTRY");

    // Close the stop channel to immediately wake the polling thread from all
    // recv_timeout calls. This prevents the up-to-30s freeze when stopping sync.
    {
        let mut tx_guard = state.stop_tx.write();
        *tx_guard = None; // Drop the sender, closing the channel
    }

    state.is_syncing.store(false, Ordering::Release);
    log::info!("[POLLING] stop_polling: stop channel closed and is_syncing set to false");
}


pub fn save_spotify_tokens(app: &AppHandle, tokens: &SpotifyTokens) -> Result<(), String> {
    log::info!(
        "[POLLING] save_spotify_tokens: ENTRY - access_token.len={}",
        tokens.access_token.len()
    );

    let store = app.store("tokens").map_err(|e| {
        log::error!("[POLLING] save_spotify_tokens: store open failed - {}", e);
        e.to_string()
    })?;
    store.set(
        "spotify_tokens",
        serde_json::to_value(tokens).map_err(|e| {
            log::error!("[POLLING] save_spotify_tokens: serialize failed - {}", e);
            e.to_string()
        })?,
    );
    store.save().map_err(|e| {
        log::error!("[POLLING] save_spotify_tokens: save failed - {}", e);
        e.to_string()
    })?;
    log::info!("[POLLING] save_spotify_tokens: SUCCESS");
    Ok(())
}

pub fn load_spotify_tokens(app: &AppHandle) -> Result<Option<SpotifyTokens>, String> {
    log::info!("[POLLING] load_spotify_tokens: ENTRY");

    let store = app.store("tokens").map_err(|e| {
        log::error!("[POLLING] load_spotify_tokens: store open failed - {}", e);
        e.to_string()
    })?;
    let value = store.get("spotify_tokens");

    match value {
        Some(v) => {
            let tokens: SpotifyTokens = serde_json::from_value(v.clone()).map_err(|e| {
                log::error!("[POLLING] load_spotify_tokens: deserialize failed - {}", e);
                e.to_string()
            })?;
            log::info!("[POLLING] load_spotify_tokens: SUCCESS - tokens loaded");
            Ok(Some(tokens))
        }
        None => {
            log::info!("[POLLING] load_spotify_tokens: no tokens in store");
            Ok(None)
        }
    }
}

pub fn save_teams_tokens(app: &AppHandle, tokens: &TeamsTokens) -> Result<(), String> {
    log::info!(
        "[POLLING] save_teams_tokens: ENTRY - access_token.len={}",
        tokens.access_token.len()
    );

    let store = app.store("tokens").map_err(|e| {
        log::error!("[POLLING] save_teams_tokens: store open failed - {}", e);
        e.to_string()
    })?;
    store.set(
        "teams_tokens",
        serde_json::to_value(tokens).map_err(|e| {
            log::error!("[POLLING] save_teams_tokens: serialize failed - {}", e);
            e.to_string()
        })?,
    );
    store.save().map_err(|e| {
        log::error!("[POLLING] save_teams_tokens: save failed - {}", e);
        e.to_string()
    })?;
    log::info!("[POLLING] save_teams_tokens: SUCCESS");
    Ok(())
}

pub fn load_teams_tokens(app: &AppHandle) -> Result<Option<TeamsTokens>, String> {
    log::info!("[POLLING] load_teams_tokens: ENTRY");

    let store = app.store("tokens").map_err(|e| {
        log::error!("[POLLING] load_teams_tokens: store open failed - {}", e);
        e.to_string()
    })?;
    let value = store.get("teams_tokens");

    match value {
        Some(v) => {
            let tokens: TeamsTokens = serde_json::from_value(v.clone()).map_err(|e| {
                log::error!("[POLLING] load_teams_tokens: deserialize failed - {}", e);
                e.to_string()
            })?;
            log::info!("[POLLING] load_teams_tokens: SUCCESS - tokens loaded");
            Ok(Some(tokens))
        }
        None => {
            log::info!("[POLLING] load_teams_tokens: no tokens in store");
            Ok(None)
        }
    }
}

pub fn clear_spotify_tokens(app: &AppHandle) -> Result<(), String> {
    log::info!("[POLLING] clear_spotify_tokens: ENTRY");

    let store = app.store("tokens").map_err(|e| {
        log::error!("[POLLING] clear_spotify_tokens: store open failed - {}", e);
        e.to_string()
    })?;
    store.delete("spotify_tokens");
    store.delete("spotify_client_id");
    // `spotify_client_secret` is no longer persisted to the store — the
    // secret now lives in the OS keychain. See issue #9.
    store.delete("spotify_client_secret");
    store.delete("spotify_redirect_uri");
    store.delete("spotify_verifier");
    store.delete("spotify_csrf_state");
    store.delete("pending_spotify_auth");
    store.save().map_err(|e| {
        log::error!("[POLLING] clear_spotify_tokens: save failed - {}", e);
        e.to_string()
    })?;

    // Best-effort: also clear the keychain entry. If the keychain is
    // unavailable (e.g. headless CI), we don't fail the whole call.
    if let Err(e) = crate::keychain::delete_spotify_client_secret() {
        log::warn!(
            "[POLLING] clear_spotify_tokens: failed to clear keychain entry - {}",
            e
        );
    }
    log::info!("[POLLING] clear_spotify_tokens: SUCCESS - tokens cleared from store and keychain");
    Ok(())
}

pub fn clear_teams_tokens(app: &AppHandle) -> Result<(), String> {
    log::info!("[POLLING] clear_teams_tokens: ENTRY");

    let store = app.store("tokens").map_err(|e| {
        log::error!("[POLLING] clear_teams_tokens: store open failed - {}", e);
        e.to_string()
    })?;
    store.delete("teams_tokens");
    store.delete("teams_device_code");
    store.delete("pending_teams_auth");
    store.save().map_err(|e| {
        log::error!("[POLLING] clear_teams_tokens: save failed - {}", e);
        e.to_string()
    })?;
    log::info!("[POLLING] clear_teams_tokens: SUCCESS - tokens cleared from store");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pause_backoff_grows_then_caps() {
        // With default 30s: 0→30, 1→60, 2→120, 3→300 (cap), 4→300 (cap)
        assert_eq!(pause_backoff(0, 30), 30);
        assert_eq!(pause_backoff(1, 30), 60);
        assert_eq!(pause_backoff(2, 30), 120);
        assert_eq!(pause_backoff(3, 30), 300);
        assert_eq!(pause_backoff(4, 30), 300);
        assert_eq!(pause_backoff(255, 30), 300); // any larger counter stays capped
    }

    #[test]
    fn test_pause_backoff_uses_configured_default() {
        // The 0 arm must use the configured default, not a hard-coded 30.
        // A user who sets default_interval_seconds=45 should see 45s for the
        // first backoff arm — proves the config is actually wired through.
        assert_eq!(pause_backoff(0, 45), 45);
        // Subsequent arms grow from 45: 45*2=90, 45*4=180, then cap at 300.
        assert_eq!(pause_backoff(1, 45), 90);
        assert_eq!(pause_backoff(2, 45), 180);
        assert_eq!(pause_backoff(3, 45), 300);
    }

    #[test]
    fn test_pause_backoff_caps_with_large_default() {
        // With a large configured default (e.g. user sets 200s), the doubling
        // arms must be clamped to 300 so the cap behaviour is preserved.
        assert_eq!(pause_backoff(0, 200), 200); // 0 arm returns the default uncapped
        assert_eq!(pause_backoff(1, 200), 300); // 200*2=400 → cap
        assert_eq!(pause_backoff(2, 200), 300); // 200*4=800 → cap
    }
}
