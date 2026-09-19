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
//! S4 (issue #672): a quiet-hours entry with `pause_polling` short-circuits the
//! dispatch — the iteration is skipped before the clocks are loaded, so it
//! performs no request and moves no clock.
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

/// Issue #866: the snooze-start path of the preferred-presence feature.
/// Resolves the configured pair and POSTs `setUserPreferredPresence` so the
/// user's Teams bubble carries Busy/DND/BeRightBack/Away for the duration of
/// the snooze, then clears at expiry (the poll-loop tick in
/// `poll_once::clear_expired_preferred_presence`) and on `RunEvent::Exit`.
/// No-op when the feature is off, the user's manual-status window is in
/// force, the snooze is for an unsupported pair, or the Teams token is
/// unavailable — every gate the rule path uses, just routed through the
/// snooze trigger instead of a matching rule.
fn arm_preferred_for_snooze(state: &AppState, app: &tauri::AppHandle) {
    let cfg = state.config.get();
    let Some(config) = cfg.as_ref() else { return };
    let pair = match crate::config::preferred_presence_pair(
        &config.teams,
        config.teams.respect_manual_status,
    ) {
        Some(p) => p,
        None => return,
    };
    let Some(tokens) = state.tokens.teams().clone() else {
        return;
    };
    super::poll_once::arm_preferred_presence_session(
        app,
        &tokens.access_token,
        &pair,
        &crate::config::preferred_presence_expiry_duration(&config.teams),
        "Snooze preferred presence",
    );
}

/// Issue #866: the snooze-end sibling of [`arm_preferred_for_snooze`]. Called
/// when `snooze_gate` reports the deadline lapsed, before the
/// `clear_snooze_if_expired` step that resets the stored value. No-op when no
/// preferred-presence session is armed — a snooze that started without the
/// feature must end without firing a clear.
fn clear_preferred_for_snooze(state: &AppState, app: &tauri::AppHandle) {
    let Some(tokens) = state.tokens.teams().clone() else {
        return;
    };
    super::poll_once::clear_preferred_presence_session(
        app,
        &tokens.access_token,
        "Snooze preferred presence cleared",
    );
}

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

        // S9 (issue #677): a tray snooze outranks everything else here — the
        // user asked for quiet on purpose. Same shape as the quiet-hours pause
        // below and ahead of it, so a snoozed iteration issues no
        // Spotify/Graph request and moves no keepalive/debounce clock. The
        // deadline is re-derived from the stored value on every iteration, so
        // the snooze ends by itself and the thread is never stopped or parked.
        let snooze_gate = {
            // Scoped: the config read guard must not be held across the sleep.
            let config = state.config.get();
            super::poll_once::snooze_gate(&config)
        };
        match snooze_gate {
            super::poll_once::SnoozeGate::Skipped(seconds) => {
                // Issue #866: the snooze transition is the second site (after
                // `rule_gate_at`) where the app drives the
                // `setUserPreferredPresence` POST. `snooze_gate` already
                // emitted the start-of-snooze log line on the false→true edge,
                // so we only POST here when the same edge fired — the
                // `Skipped` arm is reached on every iteration the snooze
                // remains live, and the helper's own debounce suppresses the
                // re-arms.
                //
                // The trigger fires once per snooze, the `Box::leak` in
                // `arm_preferred_presence_session` makes the label stable
                // across iterations, and the `Eq` arm there skips the POST
                // while we are still inside the same expiry window.
                arm_preferred_for_snooze(&state, &app);
                // Issue #877: append a "snooze-start" entry to the bounded
                // decision history so the Dashboard's Activity card can show
                // a snooze episode as a single decision, not as a series of
                // suppressed writes.
                crate::history::append(
                    crate::history::PresenceHistoryEntry {
                        at: chrono::Utc::now(),
                        kind: "snooze-start".to_string(),
                        note: format!("snoozed for {}s", seconds),
                        track_fingerprint: super::state::current_track_fingerprint(),
                        posted_status: None,
                        gate_reason: Some("snooze".to_string()),
                    },
                    state.config.get().as_ref(),
                );
                // Repaint the tray so its countdown line and the submenu's
                // "Resume sync now" entry follow the snooze. The rebuild is
                // forced into its cache-only fetch mode by the active snooze
                // itself, so this costs no Spotify request. Skipped when the
                // cache is cold — the Devices/Up Next submenus then show what
                // they last showed, exactly like any other rebuild.
                let is_syncing = state.polling.is_syncing(Ordering::Acquire);
                let current_track = state.polling.current_track().clone();
                if let Err(e) = tray::update_tray_menu(&app, is_syncing, current_track) {
                    log::warn!(
                        "[POLLING] polling_loop: tray update while snoozed failed: {}",
                        e
                    );
                }
                log::debug!(
                    "[POLLING] polling_loop: snoozed, sleeping for {} seconds",
                    seconds
                );
                match stop_rx.recv_timeout(StdDuration::from_secs(seconds)) {
                    Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        log::info!(
                            "[POLLING] polling_loop: stop signal during a snooze, breaking loop"
                        );
                        break;
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                }
            }
            super::poll_once::SnoozeGate::Expired => {
                // Issue #866: a snooze ending is the natural clear of the
                // preferred-presence session — the user opted into
                // "be Busy/DND for the duration of the snooze" and the
                // duration just elapsed. The clear runs through the regular
                // `clear_preferred_presence_session` so the dashboard's
                // `preferred-presence-updated` event lands and the tray
                // submenu can drop its preferred-state badge.
                clear_preferred_for_snooze(&state, &app);
                // Issue #877: append a "snooze-end" entry so the Activity
                // card's timeline shows the snooze episode as a
                // self-contained event.
                crate::history::append(
                    crate::history::PresenceHistoryEntry {
                        at: chrono::Utc::now(),
                        kind: "snooze-end".to_string(),
                        note: "snooze cleared".to_string(),
                        track_fingerprint: super::state::current_track_fingerprint(),
                        posted_status: None,
                        gate_reason: Some("snooze".to_string()),
                    },
                    state.config.get().as_ref(),
                );
                // The deadline passed while the thread slept. Clear the stored
                // value once, so the chip and the tray stop claiming a snooze,
                // then fall through to a normal iteration.
                super::poll_once::clear_snooze_if_expired(&state);
            }
            super::poll_once::SnoozeGate::Inactive => {}
        }
        // S4 (issue #672): quiet hours may stop polling entirely for the
        // duration of the window. Decided BEFORE the write clocks are loaded
        // and before `poll_once::run`, so a skipped iteration issues no
        // Spotify/Graph request and moves no keepalive/debounce clock. The
        // window is re-derived from the local clock on every iteration — the
        // thread is never stopped or parked, because a parked thread could not
        // notice the window ending — and the pause/resume transition is logged
        // once each by the gate itself.
        let quiet_pause_seconds = {
            // Scoped: the config read guard must not be held across the sleep.
            let config = state.config.get();
            super::poll_once::quiet_pause_iteration(&config)
        };
        if let Some(seconds) = quiet_pause_seconds {
            log::debug!(
                "[POLLING] polling_loop: quiet hours pause, sleeping for {} seconds",
                seconds
            );
            match stop_rx.recv_timeout(StdDuration::from_secs(seconds)) {
                Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    log::info!(
                        "[POLLING] polling_loop: stop signal during a quiet-hours pause, breaking loop"
                    );
                    break;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            }
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
            &mut clocks.suppressed_placeholder,
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
    // Finding D1 (issue #684): the clocks die here, but what this session left
    // on TEAMS does not — the exit snapshot in `polling/state.rs` is
    // deliberately NOT reset on this path. `RunEvent::Exit` runs
    // `clear_presence_on_exit` AFTER this tail, so resetting the snapshot here
    // (or deciding the cleanup from these now-cold clocks) is precisely the
    // defect: a quit mid-song would leave the music status and the armed
    // `Available` session live. Only a newer write/arm (which records over it)
    // or a completed exit cleanup clears it — never a session boundary.
    tray::set_presence_gated_badge(&app, false);

    log::info!("[POLLING] polling_loop: ENDED");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::RecvTimeoutError;

    /// #681, start/stop/restart: the driver's only two waits are
    /// `stop_rx.recv_timeout(ZERO)` at the top of an iteration and the
    /// stop-aware sleep between iterations; both break on `Ok(())` /
    /// `Disconnected` and continue on `Timeout`. `stop_polling` is the ONLY
    /// writer that clears the stored sender, and that drop is what wakes the
    /// driver immediately instead of after up to one `max_interval` (issue #10
    /// — the 300 s freeze) and what the exit block reads to decide whether
    /// `commands::sync` already owns the `sync-stopped` emit (finding D5).
    ///
    /// Companion: `test_every_driver_wait_site_is_stop_aware` below. This test
    /// exercises the handshake through the real `stop_polling`, but it cannot
    /// execute `polling_loop` (that needs an `AppHandle<Wry>` and this crate has
    /// no mock runtime), so the driver's own wait *calls* are pinned at the
    /// source there rather than left implicitly assumed here.
    #[test]
    fn test_stop_handshake_wakes_the_driver_and_restart_isolates_the_old_thread() {
        let state = AppState::new();
        // Exactly the pair `start_polling` installs before handing `rx` to the
        // driver; `stop_polling` only has to drop the stored sender.
        let (tx, rx) = mpsc::channel::<()>();
        *state.polling.stop_tx_mut() = Some(tx);
        state.polling.set_syncing(true, Ordering::Release);

        assert!(
            matches!(
                rx.recv_timeout(StdDuration::ZERO),
                Err(RecvTimeoutError::Timeout)
            ),
            "with a live stop channel the driver's pre-iteration check must \
             report no stop, so the loop keeps polling"
        );

        crate::polling::state::stop_polling(&state);

        assert!(
            state.polling.stop_tx().is_none(),
            "stop_polling must clear the stored sender: it is the sole writer \
             that does, which is how the driver's exit block tells a requested \
             stop from a self-termination (finding D5)"
        );
        assert!(
            matches!(
                rx.recv_timeout(StdDuration::ZERO),
                Ok(()) | Err(RecvTimeoutError::Disconnected)
            ),
            "closing the channel must wake the driver's very next wait — Pause \
             Sync may not block for up to one poll interval (issue #10)"
        );
        assert!(
            state.polling.is_syncing(Ordering::Acquire),
            "stop_polling must leave is_syncing set for the join side: a \
             concurrent Stop->Start must not be able to claim the flag while \
             the old thread is still in blocking HTTP (#69)"
        );

        // Restart: a new session installs a fresh pair. The superseded
        // receiver stays dead, so the old driver cannot be woken into — or
        // mistaken for — the new session.
        let (tx2, rx2) = mpsc::channel::<()>();
        *state.polling.stop_tx_mut() = Some(tx2);
        assert!(
            matches!(
                rx.recv_timeout(StdDuration::ZERO),
                Ok(()) | Err(RecvTimeoutError::Disconnected)
            ),
            "a superseded driver's waits must stay closed across a restart"
        );
        assert!(
            matches!(
                rx2.recv_timeout(StdDuration::ZERO),
                Err(RecvTimeoutError::Timeout)
            ),
            "the restarted session must get a live stop channel"
        );
    }

    /// Finding D1 (issue #684), driver side: the exit tail's first act leaves
    /// the session's clocks cold, and the quit cleanup must NOT be decided from
    /// them — the Teams residue lives in the exit snapshot, which no session
    /// boundary clears. Asserted through the accessors the cleanup reads and
    /// across a stop->start boundary (this tail, then the next session's start,
    /// which resets the same slot), so a tail that started clearing the
    /// snapshot fails here instead of silently leaving the music status and the
    /// armed `Available` session live on Teams after a quit.
    #[test]
    fn test_a_cold_session_end_still_carries_the_teams_residue() {
        let _guard = crate::polling::state::global_state_lock();
        // Start from a clean slate through the poller's own recorders: this
        // file must stay free of a snapshot-reset call (the D1 source guard in
        // `poll_once.rs` reads this file, and the driver must never retire the
        // residue — only a completed exit cleanup may).
        crate::polling::state::record_posted_status(None);
        crate::polling::state::record_armed_presence(None);
        crate::polling::state::record_manual_status_blocks(false);
        crate::polling::state::record_posted_status(Some("\u{1F3B5} A - T \u{1F3A7}"));
        crate::polling::state::record_armed_presence(Some((
            "Available",
            "Available",
            "Listening (Available)",
        )));

        // The exit tail, verbatim...
        crate::polling::poll_once::reset_write_clocks();
        // ...and the next session's start, which resets the same slot.
        crate::polling::poll_once::reset_write_clocks();

        let cold = crate::polling::poll_once::load_write_clocks();
        assert!(
            cold.last_track_key.is_none()
                && cold.last_posted_status.is_none()
                && cold.armed_presence.is_none(),
            "the tail must leave the clocks cold — that is precisely why the \
             cleanup cannot read them (finding D1)"
        );

        let residue = crate::polling::state::load_exit_snapshot();
        assert_eq!(
            residue.last_posted_status.as_deref(),
            Some("\u{1F3B5} A - T \u{1F3A7}"),
            "a cold session boundary must not erase the status Teams still \
             shows (finding D1)"
        );
        assert_eq!(
            residue
                .armed_presence
                .as_ref()
                .map(|(availability, _, _)| availability.as_str()),
            Some("Available"),
            "a cold session boundary must not erase the armed presence session \
             (finding D1)"
        );
        // The retirement of a completed cleanup is asserted in `state.rs`
        // (`test_snapshot_fields_record_and_retire_independently`); the driver
        // owns only the "do not clear it here" half of that contract, and this
        // file is deliberately free of a `reset_exit_snapshot` call — the D1
        // source guard in `poll_once.rs` reads this file to prove it.
    }

    /// #681, the driver's wait sites themselves. `polling_loop` cannot be driven
    /// in a unit test — it takes an `AppHandle<Wry>` and this crate has no mock
    /// runtime — so a change *inside* the loop (to a wait call, or to how a wait
    /// decides) would leave the behavioural test above green. Rather than hide
    /// that gap, this pins the driver's wait contract at the source, in the shape
    /// this repo already uses for exactly that reason (the #572/D1 and S4 guards
    /// in `poll_once.rs`): exactly four stop-aware wait sites — the
    /// non-blocking pre-iteration probe, the **snooze pause** (S9, #677) and the
    /// quiet-hours pause, and the between-iteration sleep — no plain
    /// `thread::sleep`, and a closed channel treated as "break" at every one of
    /// them, which is what makes `stop_polling` immediate (issue #10).
    ///
    /// The count is deliberately exact: adding a fifth gate that waits without
    /// consulting `stop_rx` is precisely the regression this pins, so a new
    /// wait site must be added here *and* be stop-aware.
    #[test]
    fn test_every_driver_wait_site_is_stop_aware() {
        let source = include_str!("loop.rs");
        // Production half only: this module's own text names these calls in prose
        // and would otherwise be scanned.
        let prod = &source[..source
            .find("#[cfg(test)]")
            .expect("loop.rs must keep its test module last")];

        assert_eq!(
            prod.matches("stop_rx.recv_timeout(").count(),
            4,
            "the driver must wait on the STOP-AWARE receiver at all four sites: \
             the pre-iteration probe, the snooze pause, the quiet-hours pause and \
             the between-iteration sleep"
        );
        assert!(
            !prod.contains("thread::sleep("),
            "no plain sleep in the driver: it cannot be woken by stop_polling, \
             which is the up-to-30s Pause Sync freeze (issue #10)"
        );
        assert_eq!(
            prod.matches("StdDuration::ZERO").count(),
            1,
            "the pre-iteration probe must stay non-blocking, so a stop lands \
             before the next iteration does any work"
        );
        assert_eq!(
            prod.matches("StdDuration::from_secs(").count(),
            3,
            "every wait that can outlive a request — the snooze pause, the \
             quiet-hours pause and the iteration sleep — must be bounded by the \
             returned interval"
        );
        for (idx, _) in prod.match_indices("stop_rx.recv_timeout(") {
            // Wide enough to swallow the longest arm — the snooze and quiet-hours
            // pause arms (each ~400 bytes: they log before breaking) — while still
            // being local to the site, so a matched arm cannot come from the next
            // wait.
            let arm = &prod[idx..(idx + 700).min(prod.len())];
            assert!(
                arm.contains("Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected)"),
                "every wait site must treat a closed channel as 'break': dropping \
                 the stored stop_tx is exactly how stop_polling wakes the driver"
            );
            assert!(
                arm.contains("RecvTimeoutError::Timeout"),
                "every wait site must spell out the timeout arm, so 'no stop yet' \
                 is an explicit continue rather than a catch-all"
            );
        }
    }
}
