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
    /// The last status text the poller confirmed written to Teams this
    /// session (issue #670 / finding D2). Read from the poller's shared
    /// write-decision clocks, never re-derived from a fresh Spotify or Graph
    /// call: `presence-updated` fires only on a *change*, so a view that
    /// mounts mid-session has no other way to seed its status preview.
    pub last_posted_status: Option<String>,
    /// True while the poller recorded the current track's status write as
    /// gate-suppressed (busy / in a call / quiet hours / track rule), so a
    /// freshly mounted Dashboard can render the gate chip without waiting for
    /// the next `presence-gated` event (issue #670).
    pub presence_gated: bool,
    /// True while the playback the poller last observed for `current_track`
    /// is paused. A pause is not a stop (issue #670 / finding D2, poller half
    /// in #669), so a mounting Dashboard shows the paused track card instead
    /// of "Nothing playing".
    pub presence_paused: bool,
}

#[tauri::command]
pub async fn start_syncing(
    window: tauri::Window,
    state: tauri::State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<(), String> {
    // Issue #241: polling lifecycle is main-window-only (Dashboard/+page and
    // the Rust-side `complete_onboarding` caller, which forwards its own
    // main-window handle). Detached windows never legitimately start sync.
    super::require_main_window(&window)?;
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
    let needs_drain =
        { state.polling.handle().is_some() || state.polling.is_syncing(Ordering::Acquire) };
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
        log::info!(
            "{CMD} start_syncing: polling handle stored with thread id {:?}",
            tid
        );
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
        // Issue #395: no handle was stored — either we raced with another
        // drain that already took it, or the thread self-exited (5-strikes
        // exit) and the ownership-checked cleanup in state.rs already ran.
        // Warn with the caller context and defensively clear a wedged flag
        // (set but owned by no live thread) so a future start is never
        // stuck; when another thread still owns the state, its in-flight
        // join owns the clear and we leave the flag alone.
        if state.polling.is_syncing(Ordering::Acquire) {
            let owner = *state.polling.thread_id();
            match owner {
                None => {
                    log::warn!(
                        "{CMD} {context}: no polling handle and no owner thread while is_syncing is set; clearing wedged flag"
                    );
                    state.polling.set_syncing(false, Ordering::Release);
                    *state.polling.thread_id_mut() = None;
                }
                Some(tid) => {
                    log::warn!(
                        "{CMD} {context}: no polling handle but thread {:?} still owns polling state; leaving flag for the in-flight join",
                        tid
                    );
                }
            }
        } else {
            log::debug!(
                "{CMD} {context}: no polling handle and is_syncing already false; nothing to drain"
            );
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
                    Ok(()) => log::info!(
                        "{CMD} {}: polling thread ended (detached final join)",
                        context
                    ),
                    Err(e) => log::error!(
                        "{CMD} {}: polling thread panicked (detached final join): {:?}",
                        context,
                        e
                    ),
                }
                state_detached.polling.set_syncing(false, Ordering::Release);
                *state_detached.polling.thread_id_mut() = None;
            });
        }
    }
}

#[tauri::command]
pub async fn stop_syncing(
    window: tauri::Window,
    state: tauri::State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<(), String> {
    // Issue #241: polling lifecycle is main-window-only (Dashboard/+page are
    // main-window surfaces; detached windows never legitimately stop sync).
    super::require_main_window(&window)?;
    log::debug!("{CMD} stop_syncing: ENTRY");

    let state_clone = Arc::clone(state.inner());
    stop_polling_and_join(state_clone, "stop_syncing").await;

    log::info!("{CMD} stop_syncing: EMIT sync-stopped event");
    let _ = app.emit("sync-stopped", ());

    log::info!("{CMD} stop_syncing: SUCCESS");
    Ok(())
}

#[tauri::command]
pub async fn app_exit(
    window: tauri::Window,
    state: tauri::State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<(), String> {
    // Issue #241: process exit is main-window-only (+page exit path);
    // a detached window must never terminate the app out from under the user.
    super::require_main_window(&window)?;
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

/// Issue #398: snapshot consistency + lock ordering.
///
/// `SyncStatus` is assembled from four independent slots (the atomic
/// `is_syncing` flag, the polling `current_track`, the Spotify tokens, the
/// config, and the Teams tokens). There is no single lock covering all of
/// them, so a fully atomic snapshot is impossible; instead this fn takes a
/// single critical section over the read guards in a FIXED order —
/// `polling.current_track` -> `tokens.spotify` -> `config` -> `tokens.teams`
/// — and clones every field while all four guards are held. Holding all
/// readers simultaneously means no writer (token refresh, config save,
/// track store) can interleave the clones, so impossible combinations such
/// as "track present but both providers disconnected at the same instant"
/// are unobservable in the returned struct. The atomic `is_syncing` flag is
/// loaded while the guards are held so it reflects the snapshot instant,
/// not an earlier read. New code MUST acquire these locks in the same order
/// (never `teams` before `spotify`, never `config` before `current_track`)
/// or risk a lock-ordering deadlock with this critical section.
#[tauri::command]
pub fn get_sync_status(state: tauri::State<'_, Arc<AppState>>) -> Result<SyncStatus, String> {
    log::debug!("{CMD} get_sync_status: ENTRY");
    Ok(sync_status_from_state(state.inner()))
}

/// Assemble the status snapshot from `state`.
///
/// The body of the `get_sync_status` command, split out (issue #679) so the
/// headless `--status` CLI flag reports exactly the shape and the field
/// semantics the IPC returns instead of a second, drifting copy of them. See
/// the doc comment above for the lock-ordering contract this fn implements.
pub fn sync_status_from_state(state: &AppState) -> SyncStatus {
    // Single critical section: all read guards held at once, clones below
    // cannot observe a writer interleaving between fields.
    let track_guard = state.polling.current_track();
    let spotify_guard = state.tokens.spotify();
    let config_guard = state.config.get();
    let teams_guard = state.tokens.teams();
    let is_syncing = state.polling.is_syncing(Ordering::Acquire);
    // #670: the poller's presence bookkeeping, read from the shared
    // write-decision clocks (finding PollCore#4 / #572). `WRITE_CLOCKS` is a
    // leaf lock — every production touch clones it in or out and never
    // acquires another lock while holding it — so it does not participate in
    // the `current_track -> spotify -> config -> teams` ordering above, and
    // this read neither mutates nor resets it.
    let clocks = polling::load_write_clocks();

    let current_track = track_guard.clone();
    // A pause is a state change, not a stop: the poller keeps the observed
    // `TrackInfo` for a paused track (finding D6, #669), so the stored
    // playback state is what the Dashboard renders as a paused card.
    let presence_paused = current_track.as_ref().is_some_and(|t| !t.is_playing);

    let spotify_connected = spotify_guard.is_some()
        && config_guard
            .as_ref()
            .map(|c| !c.spotify.client_id.is_empty())
            .unwrap_or(false);

    let teams_connected = teams_guard.is_some();

    log::info!(
        "{CMD} sync_status_from_state: is_syncing={}, spotify_connected={}, teams_connected={}, presence_gated={}, presence_paused={}",
        is_syncing,
        spotify_connected,
        teams_connected,
        clocks.gated_track_key.is_some(),
        presence_paused
    );

    SyncStatus {
        is_syncing,
        current_track,
        spotify_connected,
        teams_connected,
        last_posted_status: clocks.last_posted_status.clone(),
        presence_gated: clocks.gated_track_key.is_some(),
        presence_paused,
    }
}
#[tauri::command]
pub async fn refresh_status(
    window: tauri::Window,
    state: tauri::State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<(), String> {
    super::require_main_window(&window)?;
    log::debug!("{CMD} refresh_status: ENTRY");

    if !state.polling.is_syncing(Ordering::Acquire) {
        return Err("Sync is not running".to_string());
    }

    let state_inner = Arc::clone(state.inner());
    let app_clone = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::polling::run_oneshot(&state_inner, &app_clone);
        let is_syncing = state_inner.polling.is_syncing(Ordering::Acquire);
        let current_track = state_inner.polling.current_track().clone();
        if let Err(e) = crate::tray::update_tray_menu(&app_clone, is_syncing, current_track) {
            log::warn!("{CMD} refresh_status: failed to update tray menu: {}", e);
        }
    })
    .await
    .map_err(|e| format!("refresh_status spawn_blocking panicked: {:?}", e))?;

    log::info!("{CMD} refresh_status: SUCCESS");
    Ok(())
}

#[cfg(test)]
mod tests {
    /// Brace-counted body isolation (house style — never boundary anchors,
    /// which drift).
    fn fn_body<'a>(prod_source: &'a str, sig: &str) -> &'a str {
        let after_sig = prod_source
            .split(sig)
            .nth(1)
            .unwrap_or_else(|| panic!("sync.rs has no `{}`", sig));
        let open = after_sig
            .find('{')
            .unwrap_or_else(|| panic!("{} has no opening brace", sig));
        let mut depth = 0usize;
        let mut end = None;
        for (i, ch) in after_sig[open..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(open + i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        &after_sig[..end.unwrap_or_else(|| panic!("{} body never closed", sig))]
    }

    /// Issue #395: the no-handle branch of `stop_polling_and_join` must
    /// warn-log with the caller context and defensively clear a wedged
    /// syncing flag (set but owned by no live thread), while leaving the
    /// flag alone when another thread still owns the state.
    #[test]
    fn test_no_handle_branch_warns_and_clears_wedged_flag() {
        let source = include_str!("sync.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("sync.rs has no #[cfg(test)] mod tests block");
        let body = fn_body(prod_source, "async fn stop_polling_and_join(");
        assert!(
            body.contains("no polling handle"),
            "the no-handle branch must log the drain outcome (issue #395)"
        );
        assert!(
            body.contains("clearing wedged flag"),
            "the no-handle branch must clear a wedged flag when no thread owns the state (issue #395)"
        );
        assert!(
            body.contains("leaving flag for the in-flight join"),
            "the no-handle branch must not steal the flag from a live owner's in-flight join (issue #395)"
        );
    }

    /// Issue #398: the status snapshot must be assembled under a single
    /// critical section — all four read guards held at once — so torn snapshots
    /// are unobservable, with the lock order documented.
    ///
    /// Issue #679 moved the assembly out of the `get_sync_status` command into
    /// `sync_status_from_state` (the command, and the headless `--status` CLI
    /// flag, both call it) — the invariant follows the code, so the guard names
    /// the fn that now holds the guards.
    #[test]
    fn test_get_sync_status_reads_under_single_critical_section() {
        let source = include_str!("sync.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("sync.rs has no #[cfg(test)] mod tests block");
        let body = fn_body(prod_source, "pub fn sync_status_from_state(");
        for marker in [
            "state.polling.current_track()",
            "state.tokens.spotify()",
            "state.config.get()",
            "state.tokens.teams()",
        ] {
            assert!(
                body.contains(marker),
                "sync_status_from_state must hold {} inside its critical section (issue #398)",
                marker
            );
        }
        assert!(
            prod_source.contains("Single critical section"),
            "the lock-ordering contract must stay documented (issue #398)"
        );
        assert!(
            fn_body(prod_source, "pub fn get_sync_status(").contains("sync_status_from_state("),
            "the command must return the shared derivation, never a second copy of it (issue #679)"
        );
    }
}
