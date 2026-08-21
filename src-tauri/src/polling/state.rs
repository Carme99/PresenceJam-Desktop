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
use std::thread;

use serde_json::json;
use tauri::{AppHandle, Emitter};

use crate::AppState;

/// Spawn the polling thread.
///
/// Caller contract (issue #60): `commands::start_syncing` has already
/// claimed `is_syncing = true`. This function does NOT do a second
/// CAS — doing so would always lose and surface a "Polling is already
/// running" error after every fresh install. The `is_syncing` flag is
/// released by `stop_polling` (clean exit) or by the panic-cleanup
/// block below (crash exit).
pub fn start_polling(state: Arc<AppState>, app: AppHandle) -> Result<thread::JoinHandle<()>, String> {
    log::info!("[POLLING] start_polling: ENTRY");

    // Create interruptible stop channel so stop_syncing can wake the thread immediately.
    // See issue #10 (Polling thread cannot be cancelled mid-request).
    let (stop_tx, stop_rx) = mpsc::channel::<()>();

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
