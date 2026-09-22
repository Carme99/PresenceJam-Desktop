//! Polling thread lifecycle glue.
//!
//! `start_polling` claims no state of its own (the `is_syncing` flag is
//! already claimed by `commands::start_syncing` per issue #60 — see the
//! regression test in the bottom of this file's sibling modules).
//! This module's job is:
//!   1. Create the stop channel and store the sender in `AppState`.
//!   2. Spawn the polling thread with a panic guard that releases
//!      `is_syncing` + clears `stop_tx`/`thread_id` for the owning
//!      thread so a future `start_syncing` is not wedged (see #69
//!      ownership check).
//!   3. Provide `stop_polling` that closes the channel (flag is left
//!      set until the join side observes completion), waking the thread
//!      immediately from its interruptible sleeps.
//!
//! The actual iteration logic lives in [`super::loop_`] and
//! [`super::poll_once`].

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;

use serde_json::json;
use tauri::{AppHandle, Emitter};

use crate::AppState;

/// Findings D1/D5 (issues #684/#688): the Teams-facing residue of the current
/// (or most recently ended) polling session.
///
/// Why it exists: `polling_loop`'s exit tail calls
/// [`super::poll_once::reset_write_clocks`] before `RunEvent::Exit` runs
/// `clear_presence_on_exit`, so a quit mid-song used to find cold clocks and
/// skip the cleanup entirely — exactly what #636 was written for. This
/// snapshot is written at every successful Teams write / presence arm and is
/// deliberately NOT reset by a session's exit tail — or by a session START: it
/// records what this app has live on Teams, which stopping or restarting sync
/// does not change. Only a newer write/arm (which records over it) or a
/// completed exit cleanup clears it.
///
/// It describes what THIS app left on Teams, not what the thread is still
/// tracking.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ExitSnapshot {
    /// The `setPresence` session this app armed, as
    /// `(availability, activity, label)`. `None` = no session of ours is live.
    pub(crate) armed_presence: Option<(String, String, String)>,
    /// The playing-status text this app last posted to Teams. `None` = Teams
    /// shows a placeholder (or nothing of ours).
    pub(crate) last_posted_status: Option<String>,
    /// Review round 2 (item 7): the last observed verdict of
    /// [`super::poll_once::manual_status_blocks_write`] — `true` while a status
    /// message the USER owns (not one of ours) is in force on Teams. Recorded
    /// from every presence sample the poller takes, so the exit path can honour
    /// the shipped 4.6 respect-the-manual-status behaviour without a Graph call
    /// of its own: quitting must not replace the user's own Teams status with
    /// our "Paused" placeholder. `false` when nothing of the sort was observed.
    pub(crate) manual_status_blocks: bool,
}

static EXIT_SNAPSHOT: Mutex<ExitSnapshot> = Mutex::new(ExitSnapshot {
    armed_presence: None,
    last_posted_status: None,
    manual_status_blocks: false,
});

/// Snapshot the exit state. A poisoned lock is recovered rather than
/// propagated, exactly like the write-decision clocks: this is best-effort
/// cleanup bookkeeping, and losing it costs at most one skipped quit cleanup.
pub(crate) fn load_exit_snapshot() -> ExitSnapshot {
    EXIT_SNAPSHOT
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

/// Publish a whole exit snapshot (the accessor the tests and the exit path
/// use).
pub(crate) fn store_exit_snapshot(snapshot: ExitSnapshot) {
    *EXIT_SNAPSHOT.lock().unwrap_or_else(|e| e.into_inner()) = snapshot;
}

/// Record a successful `setPresence` arm (`Some`) or `clearPresence` (`None`).
pub(crate) fn record_armed_presence(pair: Option<(&str, &str, &str)>) {
    let mut snapshot = EXIT_SNAPSHOT.lock().unwrap_or_else(|e| e.into_inner());
    snapshot.armed_presence = pair.map(|(availability, activity, label)| {
        (
            availability.to_string(),
            activity.to_string(),
            label.to_string(),
        )
    });
}

/// Record a successful playing-status POST (`Some`) or placeholder clear
/// (`None`) — the two states `clear_presence_on_exit` tells apart.
pub(crate) fn record_posted_status(status: Option<&str>) {
    let mut snapshot = EXIT_SNAPSHOT.lock().unwrap_or_else(|e| e.into_inner());
    snapshot.last_posted_status = status.map(|s| s.to_string());
}

/// Record the manual-status verdict observed alongside a presence sample
/// (review round 2, item 7). Called from the poller's read sites with the same
/// predicate the write gate uses, so the exit path's view cannot drift from it.
pub(crate) fn record_manual_status_blocks(blocks: bool) {
    let mut snapshot = EXIT_SNAPSHOT.lock().unwrap_or_else(|e| e.into_inner());
    snapshot.manual_status_blocks = blocks;
}
/// Issue #863: consecutive AUTH-credential failures toward the 5-strikes
/// exit (`transient_failure_count`) and consecutive NETWORK failures toward
/// the warning-only backoff (`consecutive_network_failures`). The driver owns
/// the mutation (its loop locals); this slot only mirrors the latest values
/// after every iteration so diagnostics can tell reconnect-versus-backoff
/// apart — following the `token_metadata` read pattern on the consume side.
static TRANSIENT_FAILURE_COUNT: AtomicU8 = AtomicU8::new(0);
static CONSECUTIVE_NETWORK_FAILURES: AtomicU8 = AtomicU8::new(0);

/// Issue #863: the reason the presence gate currently holds writes back
/// (`"busy"`, `"in a call"`, `"quiet-hours"`, …). Mirrored by the driver from
/// the newest `presence-gated` history entry while a gate is recorded;
/// cleared when no gate is. Reason token only — never posted text.
static LAST_GATE_REASON: Mutex<Option<String>> = Mutex::new(None);

/// Mirror the driver's loop-local counters into the shared slot (issue #863).
/// Relaxed ordering: best-effort triage data, read once per snapshot — the
/// same laxity as the snooze/quiet atomics in `poll_once`.
pub(crate) fn record_failure_counters(transient: u8, network: u8) {
    TRANSIENT_FAILURE_COUNT.store(transient, Ordering::Relaxed);
    CONSECUTIVE_NETWORK_FAILURES.store(network, Ordering::Relaxed);
}

/// Read the mirrored failure counters (issue #863).
pub(crate) fn load_failure_counters() -> (u8, u8) {
    (
        TRANSIENT_FAILURE_COUNT.load(Ordering::Relaxed),
        CONSECUTIVE_NETWORK_FAILURES.load(Ordering::Relaxed),
    )
}

/// Publish the current presence-gate reason (`None` clears it, issue #863).
pub(crate) fn record_gate_reason(reason: Option<String>) {
    *LAST_GATE_REASON.lock().unwrap_or_else(|e| e.into_inner()) = reason;
}

/// Read the current presence-gate reason (issue #863).
pub(crate) fn load_gate_reason() -> Option<String> {
    LAST_GATE_REASON
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

/// Forget the mirrored sync state once the session that produced it has
/// ended, so a stopped poller reports zeros/`None` instead of the last
/// session's counters (issue #863). Called from the driver's exit tail next
/// to `reset_write_clocks`; deliberately separate from `reset_exit_snapshot`,
/// which must SURVIVE a session end (finding D1).
pub(crate) fn reset_sync_state() {
    record_failure_counters(0, 0);
    record_gate_reason(None);
}

/// Issue #877: the `(title, artist)` pair the poller currently tracks,
/// read by the `emit_presence_gated` history emitter so an Activity card
/// entry can pin the gate to the track it targeted. Updated in lockstep
/// with [`super::state::current_track`].
pub(crate) fn current_track_fingerprint() -> Option<crate::history::TrackFingerprint> {
    CURRENT_TRACK_FINGERPRINT
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
}

/// Issue #877: the writer side of [`current_track_fingerprint`]. The
/// poller calls this on every track change + same-track pause so the
/// gate emitter always sees the latest fingerprint without holding an
/// `AppState` lock across the Graph call.
pub(crate) fn record_current_track_fingerprint(
    fingerprint: Option<crate::history::TrackFingerprint>,
) {
    if let Ok(mut guard) = CURRENT_TRACK_FINGERPRINT.lock() {
        *guard = fingerprint;
    }
}

static CURRENT_TRACK_FINGERPRINT: Mutex<Option<crate::history::TrackFingerprint>> =
    Mutex::new(None);

/// Forget the snapshot, once a completed exit cleanup has made it moot (and so
/// a repeated `RunEvent::Exit` is a no-op). Deliberately NOT called when a
/// session starts: see the struct docs.
pub(crate) fn reset_exit_snapshot() {
    store_exit_snapshot(ExitSnapshot::default());
}

/// Spawn the polling thread.
///
/// Caller contract (issue #60): `commands::start_syncing` has already
/// claimed `is_syncing = true`. This function does NOT do a second
/// CAS — doing so would always lose and surface a "Polling is already
/// running" error after every fresh install. The `is_syncing` flag is
/// released by `stop_polling` (clean exit) or by the panic-cleanup
/// block below (crash exit).
pub fn start_polling(
    state: Arc<AppState>,
    app: AppHandle,
) -> Result<thread::JoinHandle<()>, String> {
    log::info!("[POLLING] start_polling: ENTRY");

    // Create interruptible stop channel so stop_syncing can wake the thread immediately.
    // See issue #10 (Polling thread cannot be cancelled mid-request).
    let (stop_tx, stop_rx) = mpsc::channel::<()>();

    // Finding PollCore#4 (issue #572): a fresh session starts with cold
    // write-decision clocks. `polling_loop` resets them on a clean exit; this
    // covers the case it cannot — a previous thread that died by panic, whose
    // `catch_unwind` below never reaches the loop's own reset.
    super::poll_once::reset_write_clocks();
    // Finding D1 (issue #684): the EXIT SNAPSHOT is deliberately NOT reset
    // here, unlike the clocks above. It is not a dedup input but a record of
    // what this app currently has live on Teams — and stopping or starting a
    // session does not change that: Teams keeps showing the same status and the
    // armed presence session survives, so a stop→start→quit sequence would
    // forget them and skip the very cleanup #636/D1 exist for. Only an actual
    // write/arm (which records over it) or a completed exit cleanup clears it.
    {
        let mut tx_guard = state.polling.stop_tx_mut();
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
                super::loop_::polling_loop(state_clone, app_clone, stop_rx);
            }));
            let panicked = result.is_err();
            let exit_reason = if panicked { "panicked" } else { "loop returned" };
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
                let _ = app.emit("polling-thread-panicked", json!(null));
            }
            // Release sync state on ALL thread exits (panic OR normal return).
            // Ownership-checked: only the thread that still owns the
            // stored `thread_id` may clear is_syncing / stop_tx / thread_id.
            // Without this, an async Stop→Start that clears the flag
            // while the old thread lingers (~30 s sequential HTTP) would
            // have its exit wipe the new thread's flag/stop_tx — the #69
            // regression. The drain gate in sync.rs (handle, not flag) is
            // the companion fix.
            let this_tid = std::thread::current().id();
            let is_owner = {
                let stored = *state_for_cleanup.polling.thread_id();
                stored == Some(this_tid)
            };
            if is_owner {
                // Finding D5 (issue #688) + review round 2 (item 2): the poller
                // can end the session on ITS OWN — the five-strike auth exit in
                // `poll_once`, an externally cleared `is_syncing`, a closed
                // channel — and none of those paths emitted `sync-stopped`, so
                // the Dashboard mirror stayed on "Syncing" and the tray on
                // "Pause Sync" until a restart. This ownership-checked exit point
                // is the one place every thread exit funnels through.
                //
                // Which exits must announce it is NOT `is_syncing`: an explicit
                // `commands::sync::stop_syncing` joins this thread first and
                // emits `sync-stopped` itself afterwards (the flag is still true
                // here, so emitting would double the event — two user-visible
                // toasts once S7's notification class lands), while a
                // self-terminating exit may well have `is_syncing == false`
                // already (so gating on the flag emitted nothing at all —
                // exactly backwards).
                //
                // `stop_polling` is the ONLY writer that clears the stored stop
                // sender, so "the sender is gone while we still own the thread"
                // is precisely "a stop was requested, and its emitter is
                // sync.rs": announce only when that is NOT the case. That gives
                // exactly one `sync-stopped` per stop on both paths.
                let stop_requested = state_for_cleanup.polling.stop_tx().is_none();
                if stop_requested {
                    log::debug!(
                        "[POLLING] start_polling: polling thread {:?} ended after a requested stop; commands::sync::stop_syncing owns the sync-stopped emit",
                        this_tid
                    );
                } else {
                    log::info!(
                        "[POLLING] start_polling: polling thread {:?} ended on its own ({} exit); emitting sync-stopped so the frontend mirror cannot stay on \"Syncing\"",
                        this_tid,
                        exit_reason
                    );
                    // #675: the payload says WHICH path ended the session. A
                    // desktop-notification consumer toasts only the surprise —
                    // a self-termination — and stays quiet for the stop the
                    // user just asked for, which `commands::sync::stop_syncing`
                    // (the only other emitter) marks `self_terminated: false`.
                    let _ = app.emit("sync-stopped", json!({ "self_terminated": true }));
                }
                if state_for_cleanup.polling.is_syncing(Ordering::Acquire) {
                    log::warn!(
                        "[POLLING] start_polling: polling thread {:?} exited without stop_syncing; cleaning up sync state for owner",
                        this_tid
                    );
                    state_for_cleanup.polling.set_syncing(false, Ordering::Release);
                }
                *state_for_cleanup.polling.stop_tx_mut() = None;
                *state_for_cleanup.polling.thread_id_mut() = None;
            } else {
                log::debug!(
                    "[POLLING] start_polling: thread {:?} exited but is no longer owner (stored {:?}); leaving new owner's state intact",
                    this_tid,
                    *state_for_cleanup.polling.thread_id()
                );
            }
            log::info!("[POLLING] start_polling: thread ended");
        })
        .map_err(|e| {
            log::error!("[POLLING] start_polling: thread spawn failed - {}", e);
            // Reset is_syncing so future start_polling calls are not permanently wedged.
            state.polling.set_syncing(false, Ordering::Release);
            // Also clean up the stop channel sender and thread_id we just stored.
            *state.polling.stop_tx_mut() = None;
            *state.polling.thread_id_mut() = None;
            format!("Failed to spawn polling thread: {}", e)
        })?;

    log::info!("[POLLING] start_polling: SUCCESS - handle returned");
    Ok(handle)
}

/// Close the stop channel (wakes the thread from interruptible sleeps).
/// `is_syncing` is NOT cleared here — it stays true until the joining
/// side (sync.rs `stop_polling_and_join`) observes the join completion
/// or the owning thread's exit cleanup fires. This keeps the flag
/// accurate while the old thread lingers in blocking HTTP, so an async
/// Stop→Start correctly sees `is_syncing==true` or a live handle and
/// drains. The flag is cleared by the join side or by the ownership-
/// checked cleanup in `start_polling`'s thread. Idempotent for the
/// channel; flag clearing is deferred.
pub fn stop_polling(state: &AppState) {
    log::info!("[POLLING] stop_polling: ENTRY");

    // Close the stop channel to immediately wake the polling thread from all
    // recv_timeout calls. This prevents the up-to-30s freeze when stopping sync.
    {
        let mut tx_guard = state.polling.stop_tx_mut();
        *tx_guard = None; // Drop the sender, closing the channel
    }

    log::info!("[POLLING] stop_polling: stop channel closed (is_syncing left set until join)");
}

/// Serialises the tests (in this module and in `poll_once`) that mutate the
/// process-wide write-clock / exit-snapshot statics. `cargo test` runs tests in
/// parallel threads and those slots are shared, so several tests touching both
/// need ONE lock rather than one each.
#[cfg(test)]
pub(crate) fn global_state_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Finding D1 (issue #684), review round 2 (item 6) — the ordering
    /// guarantee as BEHAVIOUR, not source text: `polling_loop`'s exit tail
    /// resets the write-decision clocks before `RunEvent::Exit` runs the
    /// cleanup, so the snapshot must survive that reset. If the reset ever
    /// reaches the snapshot (or the cleanup loses its read), a quit mid-song
    /// silently stops clearing Teams again.
    #[test]
    fn test_exit_snapshot_survives_the_write_clock_reset() {
        let _guard = global_state_lock();
        reset_exit_snapshot();
        record_posted_status(Some("\u{1F3B5} A - T \u{1F3A7}"));
        record_armed_presence(Some(("Available", "Available", "Listening (Available)")));

        // Exactly what `polling_loop`'s exit tail does before `RunEvent::Exit`:
        crate::polling::poll_once::reset_write_clocks();

        let snapshot = load_exit_snapshot();
        assert_eq!(
            snapshot.last_posted_status.as_deref(),
            Some("\u{1F3B5} A - T \u{1F3A7}"),
            "the session-end clock reset must not erase what Teams still shows (finding D1)"
        );
        assert_eq!(
            snapshot.armed_presence.as_ref().map(|(a, _, _)| a.as_str()),
            Some("Available"),
            "the session-end clock reset must not erase the armed presence session (finding D1)"
        );

        // The clocks really did reset — the snapshot is what survives.
        let cold = crate::polling::poll_once::load_write_clocks();
        assert!(
            cold.last_posted_status.is_none() && cold.last_availability_arm.is_none(),
            "this test only means something if the reset actually happened"
        );
    }

    /// Review round 2 (item 7): the exit path must not replace a Teams status
    /// the user owns. The poller records the verdict it already observes, and
    /// the exit plan honours it without a Graph read of its own.
    #[test]
    fn test_manual_status_verdict_round_trips_into_the_snapshot() {
        let _guard = global_state_lock();
        reset_exit_snapshot();
        record_posted_status(Some("\u{1F3B5} A - T \u{1F3A7}"));
        record_manual_status_blocks(true);
        assert!(
            load_exit_snapshot().manual_status_blocks,
            "the observed manual-status verdict must be part of the exit snapshot"
        );
        reset_exit_snapshot();
        assert!(
            !load_exit_snapshot().manual_status_blocks,
            "a fresh snapshot starts with no manual-status verdict on record"
        );
    }

    /// #681: the write-decision clocks are ONE process-wide slot. The driver
    /// loads it once per iteration and publishes its `&mut` view back; the
    /// manual `Refresh status` one-shot loads the same slot. If they were
    /// per-thread, that refresh could re-arm the availability session on a
    /// fresh clock or re-POST a status the #384 guard would skip (issue #572).
    /// Observed across threads on purpose: the sharing is the contract.
    #[test]
    fn test_write_clocks_are_one_process_wide_slot() {
        let _guard = global_state_lock();
        crate::polling::poll_once::reset_write_clocks();

        let mut mine = crate::polling::poll_once::load_write_clocks();
        mine.last_track_key = Some("cross-thread-key".to_string());
        crate::polling::poll_once::store_write_clocks(&mine);

        let other = std::thread::spawn(crate::polling::poll_once::load_write_clocks)
            .join()
            .expect("the clock-reading thread must not panic");
        assert_eq!(
            other.last_track_key.as_deref(),
            Some("cross-thread-key"),
            "a snapshot published by one thread must be the one another thread \
             loads — the driver and the manual refresh share a single slot (#572)"
        );

        crate::polling::poll_once::reset_write_clocks();
        assert!(
            crate::polling::poll_once::load_write_clocks()
                .last_track_key
                .is_none(),
            "resetting the shared slot is what a session boundary does, so the \
             next load must be cold (#572)"
        );
    }

    /// Findings D1/D5 (issues #684/#688): the snapshot's three fields are
    /// written and retired independently, and the exit cleanup tells them apart
    /// ("Teams shows our music status" vs "we armed an Available session" vs
    /// "the user's own status is in force"). A writer that cleared the whole
    /// snapshot would silently lose one of those and skip the quit cleanup #636
    /// exists for, so each transition is observed on its own.
    #[test]
    fn test_snapshot_fields_record_and_retire_independently() {
        let _guard = global_state_lock();
        reset_exit_snapshot();
        record_posted_status(Some("\u{1F3B5} A - T \u{1F3A7}"));
        record_armed_presence(Some(("Available", "Available", "Listening (Available)")));
        record_manual_status_blocks(true);

        // A placeholder clear retires the posted status only.
        record_posted_status(None);
        let after_status_clear = load_exit_snapshot();
        assert!(
            after_status_clear.last_posted_status.is_none(),
            "a placeholder clear must retire the posted status"
        );
        assert_eq!(
            after_status_clear
                .armed_presence
                .as_ref()
                .map(|(availability, _, _)| availability.as_str()),
            Some("Available"),
            "clearing the posted status must not erase the armed session: both \
             are needed to decide the quit cleanup (finding D1)"
        );
        assert!(
            after_status_clear.manual_status_blocks,
            "clearing the posted status must not erase the manual-status verdict \
             (review round 2, item 7)"
        );

        // ...and a session clear retires the presence arm only.
        record_posted_status(Some("\u{1F3B5} A - T \u{1F3A7}"));
        record_armed_presence(None);
        let after_presence_clear = load_exit_snapshot();
        assert!(
            after_presence_clear.armed_presence.is_none(),
            "clearing the presence session must retire the arm"
        );
        assert_eq!(
            after_presence_clear.last_posted_status.as_deref(),
            Some("\u{1F3B5} A - T \u{1F3A7}"),
            "clearing the presence session must not erase the posted status"
        );

        // A completed cleanup retires all three, so a repeated
        // `RunEvent::Exit` is a no-op (finding D1).
        reset_exit_snapshot();
        assert_eq!(
            load_exit_snapshot(),
            ExitSnapshot::default(),
            "a completed exit cleanup must retire the whole snapshot"
        );
    }
    /// Issue #863: the driver's loop-local counters and the current gate
    /// reason round-trip through the shared slot, and the session-end reset
    /// retires them without touching the exit residue (finding D1).
    #[test]
    fn test_sync_state_mirror_records_and_retires() {
        let _guard = global_state_lock();
        reset_exit_snapshot();
        reset_sync_state();
        record_failure_counters(3, 7);
        record_gate_reason(Some("busy".to_string()));
        let (transient, network) = load_failure_counters();
        assert_eq!((transient, network), (3, 7));
        assert_eq!(load_gate_reason().as_deref(), Some("busy"));
        reset_sync_state();
        let (transient, network) = load_failure_counters();
        assert_eq!((transient, network), (0, 0));
        assert!(load_gate_reason().is_none());
    }
}
