//! Polling driver.
//!
//! `polling_loop` is the outermost loop: it owns the per-thread mutable
//! state whose lifetime spans iterations (`last_track_key`, `last_etag`,
//! `last_teams_update`, `last_posted_placeholder`, `consecutive_pauses`,
//! `transient_failure_count`, `gated_track_key`, `last_availability_arm`),
//! checks the stop channel and the `is_syncing` flag, dispatches one
//! iteration to [`super::poll_once::run`], refreshes the tray, and
//! sleeps for the duration the iteration returned.
//!
//! The actual fetch / 401-retry / no-track / CAS-discard logic lives in
//! [`super::poll_once`] — the single source of truth for one iteration,
//! per issue #72. The driver owns state lifetime; `poll_once` mutates
//! state as a side effect.

use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration as StdDuration, Instant};

use tauri::AppHandle;

use crate::tray;
use crate::AppState;

/// Drive the polling loop. Owns the mutable per-thread state across
/// iterations; each iteration's logic lives in
/// [`super::poll_once::run`].
pub(crate) fn polling_loop(state: Arc<AppState>, app: AppHandle, stop_rx: mpsc::Receiver<()>) {
    log::info!("[POLLING] polling_loop: STARTED");
    // Tracks consecutive empty/paused responses so we can widen the poll
    // interval (30→60→120→300s) instead of hammering the API on a paused user.
    // See issue #38. Owned by the driver because the counter's lifetime spans
    // iterations; `poll_once::run` mutates it as a side effect of computing
    // the per-iteration sleep. There is exactly ONE increment site per
    // no-track outcome — inside poll_once::run — so the 401-retry no-track
    // branch and the main no-track branch cannot drift apart. (Issue #72
    // drift point #1.)
    let mut consecutive_pauses: u8 = 0;
    let mut last_track_key: Option<String> = None;
    let mut last_teams_update: Option<Instant> = None;
    // Tracks the last placeholder content posted by the clear path so
    // byte-identical pause/no-track POSTs are skipped (issue #155). Owned by
    // the driver because the value's lifetime spans iterations; `poll_once`
    // sets it on a successful placeholder post and clears it when a real
    // track starts. Reset on a successful post or a real track, so the gate
    // never suppresses a placeholder that was replaced by real content.
    let mut last_posted_placeholder: Option<String> = None;
    // Counts consecutive transient errors toward the 5-strikes exit that
    // emits `reconnect-required`. Owned by the driver because the counter's
    // lifetime spans iterations; `poll_once::run` mutates it inside the
    // single error arm. Reset by `poll_once` on any non-error iteration.
    let mut transient_failure_count: u8 = 0;
    // P2 (issue #3.0-P2): the track key whose status write was suppressed
    // by the presence gate. Owned by the driver because the decision spans
    // iterations — a gated track stays gated until the next track change
    // re-evaluates the presence. `poll_once::run` sets/clears it inside
    // `process_track`.
    let mut gated_track_key: Option<String> = None;
    // P1 (issue #3.0-P1): when the "Available" presence session was last
    // armed via setPresence. Owned by the driver because the re-arm cadence
    // (≤4 min, sessions fade after 5) spans iterations; `poll_once::run`
    // arms/clears it inside `process_track` / `handle_no_track`.
    let mut last_availability_arm: Option<Instant> = None;
    // Candidate C11 (docs/scope-3.3.md §C11): the ETag validator from the
    // last conditional GET /me/player/currently-playing response. Owned by
    // the driver because its lifetime spans iterations; `poll_once::run`
    // stores it from each 200/204 and echoes it back as `If-None-Match` on
    // the next poll. Absent ⇒ unconditional GET (graceful degradation:
    // Spotify's ETag support is empirical, not documented).
    let mut last_etag: Option<String> = None;

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
        if !state.polling.is_syncing(Ordering::Acquire) {
            log::info!("[POLLING] polling_loop: is_syncing=false, breaking loop");
            break;
        }

        // Delegate the iteration. The driver passes `&mut` to per-iteration
        // state so poll_once can mutate consecutive_pauses / last_track_key /
        // last_teams_update / last_posted_placeholder / transient_failure_count
        // without owning them.
        // The returned `PollIteration` tells the driver what to do next:
        // sleep N seconds, or break.
        let iteration = super::poll_once::run(
            &state,
            &app,
            &stop_rx,
            &mut last_track_key,
            &mut last_teams_update,
            &mut last_posted_placeholder,
            &mut consecutive_pauses,
            &mut transient_failure_count,
            &mut gated_track_key,
            &mut last_availability_arm,
            &mut last_etag,
        );

        // Post-iteration tray sync — independent of the API result.
        let is_syncing = state.polling.is_syncing(Ordering::Acquire);
        let current_track = state.polling.current_track().clone();
        if let Err(e) = tray::update_tray_menu(&app, is_syncing, current_track) {
            log::warn!("[POLLING] polling_loop: failed to update tray menu: {}", e);
        }

        tray::set_presence_gated_badge(&app, gated_track_key.is_some());
        match iteration {
            super::poll_once::PollIteration::Break => {
                log::info!("[POLLING] polling_loop: poll_once requested break");
                break;
            }
            super::poll_once::PollIteration::Sleep { seconds } => {
                log::debug!("[POLLING] polling_loop: sleeping for {} seconds", seconds);
                match stop_rx.recv_timeout(StdDuration::from_secs(seconds)) {
                    Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        log::info!("[POLLING] polling_loop: stop signal during sleep, breaking");
                        break;
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        // Normal timeout — continue to next poll
                    }
                }
            }
        }
    }

    tray::set_presence_gated_badge(&app, false);

    log::info!("[POLLING] polling_loop: ENDED");
}
