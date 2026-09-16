//! Polling driver.
//!
//! `polling_loop` is the outermost loop: it owns the per-thread state whose
//! lifetime spans iterations (`consecutive_pauses`, `transient_failure_count`,
//! `consecutive_network_failures`, `last_etag`, `first_iteration`) and the
//! write-decision clocks (`WriteClocks`, which live in [`super::poll_once`]
//! because the manual `run_oneshot` refresh shares them), checks the stop
//! channel and the `is_syncing` flag, dispatches one iteration to
//! [`super::poll_once::run`], refreshes the tray, and sleeps for the duration
//! the iteration returned.
//!
//! The actual fetch / 401-retry / no-track / CAS-discard logic lives in
//! [`super::poll_once`] — the single source of truth for one iteration,
//! per issue #72. The driver owns state lifetime; `poll_once` mutates
//! state as a side effect.

use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration as StdDuration;

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
    // Counts consecutive AUTH failures toward the 5-strikes exit that emits
    // `reconnect-required`. Owned by the driver because the counter's lifetime
    // spans iterations; `poll_once::run` mutates it inside the single error arm.
    // Reset by `poll_once` on any non-error iteration. Finding PollCore#0
    // (issue #568): this counter is bumped ONLY for genuinely dead credentials
    // (`ExpiredToken`/`InvalidGrant`) — a network failure can no longer stop the
    // session nor pop an OAuth window.
    let mut transient_failure_count: u8 = 0;
    // Finding PollCore#0 (issue #568): consecutive NETWORK failures (transport
    // errors, 5xx, JSON parse failures and 429s). Warning-only: once it reaches
    // its own higher threshold it escalates the backoff (capped) and is
    // surfaced as a warning — it never breaks the loop and never asks for a
    // reconnect. Reset by any successful iteration.
    let mut consecutive_network_failures: u8 = 0;
    // Candidate C11 (docs/scope-3.3.md §C11): the ETag validator from the
    // last conditional GET /me/player/currently-playing response. Owned by the
    // driver because its lifetime spans iterations; `poll_once::run` stores it
    // from each 200/204 and echoes it back as `If-None-Match` on the next poll.
    // Absent ⇒ unconditional GET (graceful degradation: Spotify's ETag support
    // is empirical, not documented).
    let mut last_etag: Option<String> = None;
    // Issue #373: fresh threads start with `last_track_key=None` — this
    // flag lets the first no-track poll attempt one clear instead of
    // returning early and leaving pre-restart status stale. Consumed
    // exactly once; `poll_once` owns the consumption.
    let mut first_iteration = true;

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

        // Finding PollCore#4 (issue #572): the write-decision clocks describe
        // the single Teams status this app shows, so they are process-wide
        // rather than per-thread. Load them for this iteration and store them
        // back afterwards — the manual refresh (`run_oneshot`) loads the same
        // slot, which is what stops it re-arming the availability session on a
        // fresh clock or re-POSTing a status the #384 guard would skip. A
        // concurrent load/store can only lose dedup precision (one redundant
        // write), never correctness.
        let mut clocks = super::poll_once::load_write_clocks();

        // Delegate the iteration. The driver passes `&mut` to per-iteration
        // state so poll_once can mutate consecutive_pauses / transient counters
        // / the shared write clocks without owning them.
        // The returned `PollIteration` tells the driver what to do next:
        // sleep N seconds, or break.
        let iteration = super::poll_once::run(
            &state,
            &app,
            &stop_rx,
            &mut clocks.last_track_key,
            &mut clocks.last_teams_update,
            &mut clocks.last_posted_placeholder,
            &mut consecutive_pauses,
            &mut transient_failure_count,
            &mut consecutive_network_failures,
            &mut clocks.gated_track_key,
            &mut clocks.last_availability_arm,
            &mut clocks.armed_presence,
            &mut last_etag,
            &mut first_iteration,
            &mut clocks.last_posted_status,
            &mut clocks.last_gate_check,
        );
        super::poll_once::store_write_clocks(&clocks);

        // Post-iteration tray sync — independent of the API result.
        let is_syncing = state.polling.is_syncing(Ordering::Acquire);
        let current_track = state.polling.current_track().clone();
        if let Err(e) = tray::update_tray_menu(&app, is_syncing, current_track) {
            log::warn!("[POLLING] polling_loop: failed to update tray menu: {}", e);
        }

        tray::set_presence_gated_badge(&app, clocks.gated_track_key.is_some());
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

    // Finding PollCore#4 (issue #572): a session's clocks die with the
    // session. Without this the next session (or a manual refresh issued while
    // no session runs) would read a stale `last_track_key`/gate and skip the
    // `spotify-track-changed` emit and the `current_track` update for a track
    // that is already playing.
    super::poll_once::reset_write_clocks();
    tray::set_presence_gated_badge(&app, false);

    log::info!("[POLLING] polling_loop: ENDED");
}
