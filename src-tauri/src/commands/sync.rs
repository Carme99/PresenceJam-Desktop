//! Sync lifecycle Tauri commands (start/stop/status/exit).
//!
//! See issue #76. Owns the polling-thread lifecycle and the shared
//! `stop_polling_and_join` helper used by both `stop_syncing` and `app_exit`.

use crate::spotify::TrackInfo;
use crate::{polling, AppState};
use serde::Serialize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

/// Log tag prefix for this submodule (issue #79 item 3).
const CMD: &str = "[CMD.SYNC]";

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct SyncStatus {
    pub is_syncing: bool,
    pub current_track: Option<TrackInfo>,
    pub spotify_connected: bool,
    pub teams_connected: bool,
}

#[tauri::command]
pub async fn start_syncing(state: tauri::State<'_, Arc<AppState>>, app: AppHandle) -> Result<(), String> {
    log::debug!("{CMD} start_syncing: ENTRY");

    // Issue #69: drain any previous polling thread BEFORE claiming the
    // is_syncing flag. Without this, a fast Stop+Start cycle (within the
    // 2s stop_polling_and_join budget) can leave a stale thread running
    // while a new one starts — both read state.spotify_tokens, both call
    // the Spotify/Graph APIs, both rebuild the tray menu.
    //
    // Only drain if a thread handle is still stored; the common case
    // (start_syncing from a fresh app start) skips this entirely.
    // #218: drain is awaited via spawn_blocking so the UI thread is not
    // blocked when the poll thread is stuck in sequential HTTP (10 s per
    // phase in poll_once).
    // #69 regression fix: gate on the stored handle, not on `is_syncing`.
    // `stop_polling` clears the flag immediately while the old thread may
    // linger up to ~30 s in sequential HTTP; an async Stop→Start would
    // otherwise see flag==false, skip the drain, and start a second
    // concurrent poller. Checking `handle` is the source of truth for
    // liveness (see state.rs ThreadId ownership check for the companion
    // fix).
    let needs_drain = {
        state.polling.handle().is_some() || state.polling.is_syncing(Ordering::Acquire)
    };
    if needs_drain {
        log::info!("{CMD} start_syncing: previous thread still considered live (handle present or is_syncing true); draining");
        let state_clone = Arc::clone(state.inner());
        stop_polling_and_join(state_clone, "start_syncing_drain").await;
    }

    // Use Polling::try_claim for an atomic check-and-set. The AcqRel /
    // Acquire orderings are encapsulated inside try_claim() so the
    // happens-before relationship with subsequent reads of is_syncing
    // (polling loop, tray) is preserved exactly. See Polling::try_claim
    // for the original `compare_exchange(false, true, AcqRel, Acquire)`
    // this replaces.
    if !state.polling.try_claim() {
        log::info!("{CMD} start_syncing: already syncing (race lost), returning early");
        return Ok(());
    }
    log::info!("{CMD} start_syncing: is_syncing flag set to true");

    // #215: start_polling thread creation is offloaded to the blocking pool
    // so the async command does not block the Tauri async runtime. The
    // returned JoinHandle is stored under the polling lock.
    let state_for_spawn = Arc::clone(state.inner());
    let app_for_spawn = app.clone();
    let handle = tauri::async_runtime::spawn_blocking(move || {
        polling::start_polling(state_for_spawn, app_for_spawn)
    })
    .await
    .map_err(|e| format!("start_syncing spawn_blocking panicked: {:?}", e))?
    .map_err(|e| {
        // Roll back is_syncing flag and thread_id since no handle was created
        log::error!(
            "{CMD} start_syncing: polling start failed - {}; rolling back is_syncing",
            e
        );
        state.polling.set_syncing(false, Ordering::Release);
        *state.polling.thread_id_mut() = None;
        e
    })?;
    // Ensure rollback on spawn failure is handled above; on success we
    // still need to clear the flag if the inner Result was Err, but the
    // map_err above already did. This second branch handles the Ok handle
    // path only.
    log::info!("{CMD} start_syncing: polling task spawned");

    {
        let tid = handle.thread().id();
        {
            let mut handle_guard = state.polling.handle_mut();
            *handle_guard = Some(handle);
        }
        *state.polling.thread_id_mut() = Some(tid);
        log::info!("{CMD} start_syncing: polling handle stored with thread id {:?}", tid);
    }

    log::info!("{CMD} start_syncing: EMIT sync-started event");
    let _ = app.emit("sync-started", ());

    log::info!("{CMD} start_syncing: SUCCESS");
    Ok(())
}

/// Stop the polling thread and join it without blocking the caller thread.
///
/// #218: the blocking `JoinHandle::join` (which can freeze for tens of
/// seconds when the poll thread is blocked in sequential HTTP —
/// presence+status+availability each ~10 s in poll_once) is moved into
/// `spawn_blocking`. Callers `await` this async fn, so the UI thread
/// stays responsive. The 2 s grace poll remains inside the blocking
/// closure so the await only blocks a pool thread, not the UI.
async fn stop_polling_and_join(state: Arc<AppState>, context: &'static str) {
    // Close the stop channel but keep is_syncing true until the join
    // completes — otherwise an async Stop→Start would see flag==false
    // while the old thread still lingers in blocking HTTP and start a
    // second concurrent poller (the #69 regression). The companion
    // ownership check in state.rs ensures the old thread's exit does not
    // wipe the new thread's flag/stop_tx/thread_id.
    polling::stop_polling(&state);
    let handle_opt = {
        let mut handle_guard = state.polling.handle_mut();
        handle_guard.take()
    };
    if let Some(handle) = handle_opt {
        let ctx = context.to_string();
        let state_for_flag = Arc::clone(&state);
        let res = tauri::async_runtime::spawn_blocking(move || {
            // Give thread up to 2 seconds to finish cooperatively
            let started = std::time::Instant::now();
            while started.elapsed() < std::time::Duration::from_secs(2) {
                if handle.is_finished() {
                    match handle.join() {
                        Ok(()) => {
                            log::info!("{CMD} {}: polling thread ended", ctx);
                        }
                        Err(e) => {
                            log::error!("{CMD} {}: polling thread panicked: {:?}", ctx, e);
                        }
                    }
                    // Join completed — clear the sync flag and thread_id
                    // that were kept set during the grace period.
                    state_for_flag.polling.set_syncing(false, Ordering::Release);
                    *state_for_flag.polling.thread_id_mut() = None;
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }

            // Timeout reached - try one final join (may block briefly, but on
            // blocking pool, not the caller thread)
            log::warn!(
                "{CMD} {}: polling thread did not terminate within 2s, attempting final join",
                ctx
            );
            match handle.join() {
                Ok(()) => {
                    log::info!("{CMD} {}: polling thread ended (final join)", ctx);
                }
                Err(e) => {
                    log::error!(
                        "{CMD} {}: polling thread panicked (final join): {:?}",
                        ctx,
                        e
                    );
                }
            }
            state_for_flag.polling.set_syncing(false, Ordering::Release);
            *state_for_flag.polling.thread_id_mut() = None;
        })
        .await;
        if let Err(e) = res {
            log::error!("{CMD} {}: spawn_blocking panicked: {:?}", context, e);
            // Ensure flag is cleared even if the blocking task panicked
            state.polling.set_syncing(false, Ordering::Release);
            *state.polling.thread_id_mut() = None;
        }
    } else {
        // No handle was stored — either we raced with another drain that
        // already took it, or the thread self-exited (5-strikes). Ensure
        // flag is cleared if no thread is live. The ownership-checked
        // cleanup in state.rs will have cleared it for self-exit, but
        // for the race we clear here.
        if !state.polling.is_syncing(Ordering::Acquire) {
            // already cleared
        } else {
            // Check if any thread is still considered owner — if handle
            // is None and flag is true, the join is still in flight on
            // another task. Don't clear here; let that task clear.
            // This branch is for the case where handle was None and flag
            // was true but no join is in flight (should not happen).
        }
    }
}

/// Variant for `app_exit`: awaits only the 2 s grace on the blocking pool,
/// then detaches the final blocking `join` so the process can exit without
/// waiting tens of seconds. Mirrors `stop_polling_and_join` but with a
/// timeout + detached drain. The detached `spawn_blocking` may not get
/// to log before `app.exit(0)` terminates the process — that race is
/// harmless but noted for log readers.
async fn stop_polling_and_join_for_exit(state: Arc<AppState>, context: &'static str) {
    polling::stop_polling(&state);
    let handle_opt = {
        let mut handle_guard = state.polling.handle_mut();
        handle_guard.take()
    };
    if let Some(handle) = handle_opt {
        let ctx = context.to_string();
        let state_for_flag = Arc::clone(&state);
        // First, await only the 2 s grace on the blocking pool.
        let still_running = tauri::async_runtime::spawn_blocking(move || {
            let started = std::time::Instant::now();
            while started.elapsed() < std::time::Duration::from_secs(2) {
                if handle.is_finished() {
                    match handle.join() {
                        Ok(()) => log::info!("{CMD} {}: polling thread ended", ctx),
                        Err(e) => log::error!("{CMD} {}: polling thread panicked: {:?}", ctx, e),
                    }
                    state_for_flag.polling.set_syncing(false, Ordering::Release);
                    *state_for_flag.polling.thread_id_mut() = None;
                    return None;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            log::warn!(
                "{CMD} {}: polling thread did not terminate within 2s, detaching final join",
                ctx
            );
            Some(handle)
        })
        .await
        .unwrap_or(None);

        if let Some(handle) = still_running {
            // Spawn detached drain for the final blocking join - caller has
            // already proceeded to exit. Use spawn_blocking so the join
            // does not block the async runtime. The detached task will
            // clear flag/thread_id when it completes, but app.exit may
            // terminate the process before it logs — that race is harmless.
            let state_detached = Arc::clone(&state);
            tauri::async_runtime::spawn_blocking(move || {
                match handle.join() {
                    Ok(()) => log::info!("{CMD} {}: polling thread ended (detached final join)", context),
                    Err(e) => log::error!(
                        "{CMD} {}: polling thread panicked (detached final join): {:?}",
                        context, e
                    ),
                }
                state_detached.polling.set_syncing(false, Ordering::Release);
                *state_detached.polling.thread_id_mut() = None;
            });
        }
    }
}

#[tauri::command]
pub async fn stop_syncing(state: tauri::State<'_, Arc<AppState>>, app: AppHandle) -> Result<(), String> {
    log::debug!("{CMD} stop_syncing: ENTRY");

    let state_clone = Arc::clone(state.inner());
    stop_polling_and_join(state_clone, "stop_syncing").await;

    log::info!("{CMD} stop_syncing: EMIT sync-stopped event");
    let _ = app.emit("sync-stopped", ());

    log::info!("{CMD} stop_syncing: SUCCESS");
    Ok(())
}

#[tauri::command]
pub async fn app_exit(state: tauri::State<'_, Arc<AppState>>, app: AppHandle) -> Result<(), String> {
    log::debug!("{CMD} app_exit: ENTRY");

    let is_syncing = state.polling.is_syncing(Ordering::Acquire);

    if is_syncing {
        log::info!("{CMD} app_exit: stopping polling first");
        let state_clone = Arc::clone(state.inner());
        stop_polling_and_join_for_exit(state_clone, "app_exit").await;
    }

    log::info!("{CMD} app_exit: calling app.exit(0)");
    app.exit(0);
    Ok(())
}

#[tauri::command]
pub fn get_sync_status(state: tauri::State<'_, Arc<AppState>>) -> Result<SyncStatus, String> {
    log::debug!("{CMD} get_sync_status: ENTRY");

    let is_syncing = state.polling.is_syncing(Ordering::Acquire);

    let current_track = {
        let guard = state.polling.current_track();
        guard.clone()
    };

    let spotify_connected = {
        let tokens = state.tokens.spotify();
        let config = state.config.get();
        tokens.is_some()
            && config
                .as_ref()
                .map(|c| !c.spotify.client_id.is_empty())
                .unwrap_or(false)
    };

    let teams_connected = {
        let guard = state.tokens.teams();
        guard.is_some()
    };

    log::info!(
        "{CMD} get_sync_status: is_syncing={}, spotify_connected={}, teams_connected={}",
        is_syncing,
        spotify_connected,
        teams_connected
    );

    Ok(SyncStatus {
        is_syncing,
        current_track,
        spotify_connected,
        teams_connected,
    })
}
