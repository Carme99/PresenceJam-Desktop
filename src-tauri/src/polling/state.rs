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

use std::sync::atomic::Ordering;
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
/// deliberately NOT reset by a session's exit tail; it is cleared when a new
/// session starts (a fresh session must not inherit the previous residue) and
/// after a complete exit cleanup. It describes what THIS app left on Teams,
/// not what the thread is still tracking.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ExitSnapshot {
    /// The `setPresence` session this app armed, as
    /// `(availability, activity, label)`. `None` = no session of ours is live.
    pub(crate) armed_presence: Option<(String, String, String)>,
    /// The playing-status text this app last posted to Teams. `None` = Teams
    /// shows a placeholder (or nothing of ours).
    pub(crate) last_posted_status: Option<String>,
}

static EXIT_SNAPSHOT: Mutex<ExitSnapshot> = Mutex::new(ExitSnapshot {
    armed_presence: None,
    last_posted_status: None,
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

/// Forget the snapshot — a new session must not inherit the previous one's
/// Teams residue, and a completed exit cleanup must not repeat.
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
    // Finding D1 (issue #684): the exit snapshot describes what a SESSION left
    // on Teams — a new one starts cold, so a quit before its first write must
    // not clear the previous session's residue. The clocks' own reset above
    // cannot carry this: `polling_loop`'s exit tail resets the clocks BEFORE
    // `clear_presence_on_exit` reads them, which is the D1 defect.
    reset_exit_snapshot();
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
                if state_for_cleanup.polling.is_syncing(Ordering::Acquire) {
                    // Finding D5 (issue #688): the poller can end the session on
                    // ITS OWN — the five-strike auth exit in `poll_once`, an
                    // externally cleared `is_syncing`, or a closed stop channel
                    // — and none of those paths emitted `sync-stopped` (only
                    // `commands::sync::stop_syncing` did, after a join that
                    // self-termination never goes through), so the Dashboard
                    // mirror stayed on "Syncing" and the tray on "Pause Sync"
                    // until a restart. This ownership-checked exit point is the
                    // one place every thread exit funnels through.
                    log::info!(
                        "[POLLING] start_polling: polling thread {:?} ended ({} exit) while is_syncing was still true; emitting sync-stopped so the frontend mirror cannot stay on \"Syncing\"",
                        this_tid,
                        exit_reason
                    );
                    // Payload shape copied verbatim from the event's only other
                    // emitter, `commands::sync::stop_syncing`: a unit payload.
                    let _ = app.emit("sync-stopped", ());
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
