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

#[derive(Debug, Clone, Serialize)]
pub struct SyncStatus {
    pub is_syncing: bool,
    pub current_track: Option<TrackInfo>,
    pub spotify_connected: bool,
    pub teams_connected: bool,
}

#[tauri::command]
pub fn start_syncing(state: tauri::State<'_, Arc<AppState>>, app: AppHandle) -> Result<(), String> {
    log::debug!("{CMD} start_syncing: ENTRY");

    // Issue #69: drain any previous polling thread BEFORE claiming the
    // is_syncing flag. Without this, a fast Stop+Start cycle (within the
    // 2s stop_polling_and_join budget) can leave a stale thread running
    // while a new one starts — both read state.spotify_tokens, both call
    // the Spotify/Graph APIs, both rebuild the tray menu.
    //
    // Only drain if a thread is actually running; the common case
    // (start_syncing from a fresh app start) skips this entirely.
    if state.is_syncing.load(Ordering::Acquire) {
        log::info!("{CMD} start_syncing: previous thread still running; draining");
        stop_polling_and_join(state.inner(), "start_syncing_drain");
    }

    // Use compare_exchange for an atomic check-and-set. AcqRel on
    // success preserves the happens-before relationship with subsequent
    // reads of is_syncing (e.g. the polling loop and tray).
    {
        if state
            .is_syncing
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            log::info!("{CMD} start_syncing: already syncing (race lost), returning early");
            return Ok(());
        }
        log::info!("{CMD} start_syncing: is_syncing flag set to true");
    }

    let handle = match polling::start_polling(Arc::clone(state.inner()), app.clone()) {
        Ok(h) => h,
        Err(e) => {
            // Roll back is_syncing flag since no handle was created
            log::error!(
                "{CMD} start_syncing: polling start failed - {}; rolling back is_syncing",
                e
            );
            state.is_syncing.store(false, Ordering::Release);
            return Err(e);
        }
    };
    log::info!("{CMD} start_syncing: polling task spawned");

    {
        let mut handle_guard = state.polling_handle.write();
        *handle_guard = Some(handle);
        log::info!("{CMD} start_syncing: polling handle stored");
    }

    log::info!("{CMD} start_syncing: EMIT sync-started event");
    let _ = app.emit("sync-started", ());

    log::info!("{CMD} start_syncing: SUCCESS");
    Ok(())
}

fn stop_polling_and_join(state: &Arc<AppState>, context: &str) {
    polling::stop_polling(state);
    {
        let mut handle_guard = state.polling_handle.write();
        if let Some(handle) = handle_guard.take() {
            drop(handle_guard); // Release lock while waiting

            // Give thread up to 2 seconds to finish cooperatively
            let started = std::time::Instant::now();
            while started.elapsed() < std::time::Duration::from_secs(2) {
                if handle.is_finished() {
                    match handle.join() {
                        Ok(()) => {
                            log::info!("{CMD} {}: polling thread ended", context);
                        }
                        Err(e) => {
                            log::error!("{CMD} {}: polling thread panicked: {:?}", context, e);
                        }
                    }
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }

            // Timeout reached - try one final join (may block briefly)
            log::warn!(
                "{CMD} {}: polling thread did not terminate within 2s, attempting final join",
                context
            );
            match handle.join() {
                Ok(()) => {
                    log::info!("{CMD} {}: polling thread ended (final join)", context);
                }
                Err(e) => {
                    log::error!(
                        "{CMD} {}: polling thread panicked (final join): {:?}",
                        context,
                        e
                    );
                }
            }
        }
    }
}

#[tauri::command]
pub fn stop_syncing(state: tauri::State<'_, Arc<AppState>>, app: AppHandle) -> Result<(), String> {
    log::debug!("{CMD} stop_syncing: ENTRY");

    stop_polling_and_join(state.inner(), "stop_syncing");

    log::info!("{CMD} stop_syncing: EMIT sync-stopped event");
    let _ = app.emit("sync-stopped", ());

    log::info!("{CMD} stop_syncing: SUCCESS");
    Ok(())
}

#[tauri::command]
pub fn app_exit(state: tauri::State<'_, Arc<AppState>>, app: AppHandle) -> Result<(), String> {
    log::debug!("{CMD} app_exit: ENTRY");

    let is_syncing = state.is_syncing.load(Ordering::Acquire);

    if is_syncing {
        log::info!("{CMD} app_exit: stopping polling first");
        stop_polling_and_join(state.inner(), "app_exit");
    }

    log::info!("{CMD} app_exit: calling app.exit(0)");
    app.exit(0);
    Ok(())
}

#[tauri::command]
pub fn get_sync_status(state: tauri::State<'_, Arc<AppState>>) -> Result<SyncStatus, String> {
    log::debug!("{CMD} get_sync_status: ENTRY");

    let is_syncing = state.is_syncing.load(Ordering::Acquire);

    let current_track = {
        let guard = state.current_track.read();
        guard.clone()
    };

    let spotify_connected = {
        let tokens = state.spotify_tokens.read();
        let config = state.config.read();
        tokens.is_some()
            && config
                .as_ref()
                .map(|c| !c.spotify.client_id.is_empty())
                .unwrap_or(false)
    };

    let teams_connected = {
        let guard = state.teams_tokens.read();
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