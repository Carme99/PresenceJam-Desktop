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

/// Issue #809: the explicit-start session guard.
///
/// `start_syncing_with` used to gate only on `is_syncing`, so a user whose
/// tokens had just been cleared — the poller's `invalid_grant` path clears
/// them mid-session, and `reconnect_spotify_session` clears them too — could
/// press Resume sync and get "Syncing" plus a "Pause Sync" tray entry while
/// the poller slept in its tolerant no-token branch
/// (`poll_once`: "No Spotify tokens available, waiting...") and nothing ever
/// reached Teams. That tolerant sleep is the right answer to a mid-session
/// loss a later reconnect heals; it is the wrong answer to an explicit user
/// request nothing can satisfy.
///
/// Returns the machine-readable code naming the missing side, so the
/// Dashboard toggle, the tray toggle and the sync hotkey can all prompt the
/// right sign-in; `None` means both sessions are present.
fn missing_session_code(state: &AppState) -> Option<&'static str> {
    if state.tokens.spotify().is_none() {
        return Some("spotify_not_connected");
    }
    if state.tokens.teams().is_none() {
        return Some("teams_not_connected");
    }
    None
}

/// Issue #941: publish the claim-window owner sentinel.
///
/// `try_claim()` sets only `is_syncing`; the poller's own `ThreadId` is
/// stored later, after `spawn_blocking` has queued (and run)
/// `start_polling`. That window therefore held a flag with no owner, and
/// `stop_polling_and_join`'s no-handle branch read `thread_id == None` as
/// "wedged flag" and cleared it out from under the start: the fresh
/// poller's first loop check then saw `is_syncing == false` and broke
/// immediately, so a Start that should have run never polled and a
/// "sync stopped unexpectedly" notification fired for a stop the user
/// issued. The window covers the whole `spawn_blocking` queue wait, so it
/// widens under blocking-pool load.
///
/// Publishing the claiming thread's id as owner until the real id replaces
/// it makes the window look like what it is — a live owner — so a stop
/// arriving in it leaves the flag to the start instead of stealing it.
fn publish_start_sentinel(state: &AppState) -> std::thread::ThreadId {
    let tid = std::thread::current().id();
    *state.polling.thread_id_mut() = Some(tid);
    tid
}

/// Issue #941 (F2): conclude a stop that won the race against this start.
///
/// `stop_tx == None` immediately after the handle store means no live poller:
/// either a racing stop dropped the channel `start_polling` had just installed
/// — so the fresh poller takes `Disconnected` and breaks on its first loop
/// check — or the poller already self-exited. The drain that produced that
/// state had no handle to join and deliberately left the flag to this start,
/// so the start has to finish the stop: publishing the session now would leave
/// `is_syncing == true`, a finished handle and nothing polling, which is
/// exactly the "Syncing" UI with no Teams traffic the issue is about. The
/// finished handle stays for the next start's drain to reclaim.
///
/// A *live* channel means the racing stop landed before `start_polling`
/// installed the channel (it closed nothing) — the start owns the session and
/// keeps its flag, which is the claim-window behaviour the issue's acceptance
/// criteria ask for. Returns true when the stop was concluded here.
fn conclude_raced_stop(state: &AppState) -> bool {
    if state.polling.stop_tx().is_some() {
        return false;
    }
    log::warn!(
        "{CMD} start_syncing: stop channel already closed when the poller started - a stop won the race; clearing is_syncing instead of publishing a dead session"
    );
    state.polling.set_syncing(false, Ordering::Release);
    *state.polling.thread_id_mut() = None;
    true
}

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
    start_syncing_with(Arc::clone(state.inner()), &app).await
}

/// `start_syncing`'s body, callable outside an IPC context (issue #676): the
/// global-shortcut handler has no `State` to hand a command, and asking the
/// frontend back over an event would make a shortcut — whose entire point is
/// working with no visible window — depend on a live webview. The IPC command
/// is a thin wrapper (window guard + this call), so both paths run exactly one
/// implementation of the lifecycle.
pub async fn start_syncing_with(state: Arc<AppState>, app: &AppHandle) -> Result<(), String> {
    log::debug!("{CMD} start_syncing: ENTRY");

    // Issue #809: refuse an explicit start that no session can satisfy,
    // before the drain, so a refusal never tears down a running poller.
    // The typed code names the side the user has to reconnect; leaving
    // `is_syncing` untouched keeps the flag honest (it is still false).
    if let Some(missing) = missing_session_code(&state) {
        log::warn!(
            "{CMD} start_syncing: refusing to start ({missing}) - no session to sync; is_syncing left false"
        );
        return Err(missing.to_string());
    }

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
        let state_clone = Arc::clone(&state);
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

    // Issue #941: own the claim window immediately. Until this line the
    // flag was set with `thread_id == None`, which a concurrent stop read
    // as a wedged flag and cleared.
    let sentinel = publish_start_sentinel(&state);
    log::debug!("{CMD} start_syncing: claim-window owner sentinel {sentinel:?} published");

    // #215: start_polling thread creation is offloaded to the blocking pool
    // so the async command does not block the Tauri async runtime. The
    // returned JoinHandle is stored under the polling lock.
    let state_for_spawn = Arc::clone(&state);
    let app_for_spawn = app.clone();
    let handle = tauri::async_runtime::spawn_blocking(move || {
        polling::start_polling(state_for_spawn, app_for_spawn)
    })
    .await
    .map_err(|e| {
        // Issue #941: a start that panicked before storing a handle must
        // release the claim it published, sentinel included — otherwise the
        // slot stays owned by a thread that is not polling and the next
        // start is wedged.
        log::error!(
            "{CMD} start_syncing: spawn_blocking panicked - {:?}; rolling back is_syncing",
            e
        );
        state.polling.set_syncing(false, Ordering::Release);
        *state.polling.thread_id_mut() = None;
        format!("start_syncing spawn_blocking panicked: {:?}", e)
    })?
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
        // Issue #941: the real poller id replaces the claim-window sentinel.
        *state.polling.thread_id_mut() = Some(tid);
        log::info!(
            "{CMD} start_syncing: polling handle stored with thread id {:?}",
            tid
        );
    }

    // Issue #941 (F2): if a stop won the race while this start was in flight,
    // the poller it raced is already dead — conclude the stop rather than
    // publishing a session that can never poll.
    if conclude_raced_stop(&state) {
        return Ok(());
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
        //
        // Issue #941: an owner is not only a running poller — a start
        // between `try_claim` and the handle store owns the slot through
        // the sentinel `start_syncing_with` publishes. Clearing the flag
        // there would stop the poller the user just asked for, so the flag
        // is only ever cleared when no owner at all is stored.
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
                        "{CMD} {context}: no polling handle but thread {:?} still owns polling state (running poller or a start in its claim window); leaving flag for the in-flight owner",
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
    stop_syncing_with(Arc::clone(state.inner()), &app).await
}

/// `stop_syncing`'s body, callable outside an IPC context — see
/// [`start_syncing_with`] for why the global-shortcut handler needs it
/// (issue #676).
pub async fn stop_syncing_with(state: Arc<AppState>, app: &AppHandle) -> Result<(), String> {
    log::debug!("{CMD} stop_syncing: ENTRY");

    stop_polling_and_join(state, "stop_syncing").await;

    log::info!("{CMD} stop_syncing: EMIT sync-stopped event");
    // #675: this emitter is the explicit user stop, so the payload says so —
    // the notification consumer toasts only `self_terminated: true` exits
    // (polling/state.rs) and must not report the user's own click back to them.
    let _ = app.emit(
        "sync-stopped",
        serde_json::json!({ "self_terminated": false }),
    );

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
pub async fn get_sync_status(state: tauri::State<'_, Arc<AppState>>) -> Result<SyncStatus, String> {
    log::debug!("{CMD} get_sync_status: ENTRY");
    sync_status_offloaded(Arc::clone(state.inner())).await
}

/// Issue #879: `get_sync_status`'s snapshot, handed to the blocking pool.
///
/// The snapshot takes four read guards, and the token writers hold theirs
/// ACROSS the HTTPS refresh — `cas_refresh_or_discard` is given
/// `&mut *state.tokens.teams_mut()` (poller) or
/// `&mut *state.tokens.spotify_mut()` (boot gate) before it runs its refresh
/// closure. As a synchronous command this ran on the main thread, so the
/// Dashboard's mount-time and post-event calls parked the webview's UI thread
/// until the in-flight refresh completed: a frozen window rather than a late
/// status. `SyncStatus` is plain data with no frontend change needed, so the
/// assembly moves off the UI thread and the caller only awaits. Lifted out of
/// the command (not inlined) so the offload itself is covered by a test.
async fn sync_status_offloaded(state: Arc<AppState>) -> Result<SyncStatus, String> {
    tauri::async_runtime::spawn_blocking(move || sync_status_from_state(&state))
        .await
        .map_err(|e| format!("get_sync_status spawn_blocking panicked: {:?}", e))
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
            body.contains("leaving flag for the in-flight owner"),
            "the no-handle branch must not steal the flag from a live owner's in-flight drain (issue #395), nor from a start in its claim window (issue #941)"
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
        // Issue #879: the assembly is reached through the offload, so the
        // command must still return the shared derivation and nothing else.
        let command_body = fn_body(prod_source, "pub async fn get_sync_status(");
        assert!(
            command_body.contains("sync_status_offloaded("),
            "the command must return the shared derivation through the blocking-pool offload, never a second copy of it (issues #679, #879)"
        );
        assert!(
            fn_body(prod_source, "async fn sync_status_offloaded(")
                .contains("sync_status_from_state("),
            "the offload must assemble the snapshot through sync_status_from_state (issue #879)"
        );
    }

    /// Issue #809: an explicit start with a session missing must be refused
    /// by name instead of claiming the polling flag and sleeping in the
    /// poller's tolerant no-token branch.
    #[test]
    fn test_start_syncing_refuses_without_both_sessions() {
        use super::{missing_session_code, AppState};
        use std::sync::atomic::Ordering;

        let state = AppState::new();
        assert_eq!(
            missing_session_code(&state),
            Some("spotify_not_connected"),
            "with no session at all the guard must name Spotify first (issue #809)"
        );
        assert!(
            !state.polling.is_syncing(Ordering::Acquire),
            "a refused start must leave the polling flag false (issue #809)"
        );

        *state.tokens.spotify_mut() = Some(crate::spotify::SpotifyTokens {
            access_token: "access".to_string(),
            refresh_token: "refresh".to_string(),
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        });
        assert_eq!(
            missing_session_code(&state),
            Some("teams_not_connected"),
            "a Spotify-only session must name Teams as the missing side (issue #809)"
        );

        *state.tokens.teams_mut() = Some(crate::teams::TeamsTokens {
            access_token: "teams-access".to_string(),
            refresh_token: Some("teams-refresh".to_string()),
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        });
        assert_eq!(
            missing_session_code(&state),
            None,
            "with both sessions present the start must proceed (issue #809)"
        );
    }

    /// Issue #809 acceptance: the guard runs before the flag is claimed, so a
    /// refusal can never leave a claimed `is_syncing` behind.
    #[test]
    fn test_start_syncing_guard_runs_before_the_claim() {
        let source = include_str!("sync.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("sync.rs has no #[cfg(test)] mod tests block");
        let body = fn_body(prod_source, "pub async fn start_syncing_with(");
        let guard = body
            .find("missing_session_code(")
            .expect("start_syncing_with must run the session guard (issue #809)");
        let claim = body
            .find("try_claim()")
            .expect("start_syncing_with must claim the polling flag");
        assert!(
            guard < claim,
            "the session guard must run before try_claim, so a refused start \
             never claims the flag (issue #809)"
        );
    }

    /// Issue #941: a stop arriving between `try_claim` and the handle store
    /// must leave the flag to the in-flight start. Before the fix the claim
    /// window was an ownerless flag, the drain's no-handle branch read that
    /// as "wedged" and cleared it, and the poller the user had just started
    /// broke on its first loop check.
    #[test]
    fn test_stop_in_the_claim_window_leaves_the_flag_for_the_start() {
        use super::{publish_start_sentinel, stop_polling_and_join, AppState};
        use std::sync::atomic::Ordering;
        use std::sync::Arc;

        let state = Arc::new(AppState::new());

        // The production claim, in order: flag, then owner sentinel, then
        // (only later, on the blocking pool) the real handle + thread id.
        assert!(
            state.polling.try_claim(),
            "a fresh state must hand the claim to the first start"
        );
        publish_start_sentinel(&state);
        assert!(
            state.polling.thread_id().is_some(),
            "the claim window must publish an owner, or a stop in it reads \
             the flag as wedged (issue #941)"
        );

        // The racing stop, through the real drain path.
        tauri::async_runtime::block_on(stop_polling_and_join(
            Arc::clone(&state),
            "test_claim_window",
        ));

        assert!(
            state.polling.is_syncing(Ordering::Acquire),
            "a stop in the claim window must leave is_syncing true for the \
             start that owns it (issue #941)"
        );
        assert!(
            state.polling.thread_id().is_some(),
            "the winning start's owner entry must survive the racing stop \
             (issue #941)"
        );
    }

    /// Issue #941 acceptance: wedged-flag recovery must still work — a flag
    /// set with no owner at all (no poller, no start in flight) is cleared.
    #[test]
    fn test_wedged_flag_without_any_owner_is_still_recovered() {
        use super::{stop_polling_and_join, AppState};
        use std::sync::atomic::Ordering;
        use std::sync::Arc;

        let state = Arc::new(AppState::new());
        state.polling.set_syncing(true, Ordering::Release);
        assert!(state.polling.thread_id().is_none());

        tauri::async_runtime::block_on(stop_polling_and_join(
            Arc::clone(&state),
            "test_wedged_flag",
        ));

        assert!(
            !state.polling.is_syncing(Ordering::Acquire),
            "an ownerless flag must still be cleared so a future start is not \
             permanently wedged (issue #395/#941)"
        );
    }

    /// Issue #879: with a token refresh in flight (a writer holding the Teams
    /// guard, as `cas_refresh_or_discard` does across its HTTPS call) the
    /// status command must hand the snapshot to the blocking pool and yield
    /// its own thread — the UI thread — instead of parking on the read guard
    /// until the refresh commits. Before the fix the command was synchronous
    /// and assembled the snapshot inline, so it could not yield at all.
    #[test]
    fn test_sync_status_offload_yields_instead_of_parking_the_command_thread() {
        use super::{sync_status_offloaded, AppState};
        use std::future::Future;
        use std::sync::mpsc;
        use std::sync::Arc;
        use std::task::{Context, Poll, Waker};

        let state = Arc::new(AppState::new());

        // The in-flight refresh: another thread holds the Teams write guard
        // until the test releases it.
        let writer_state = Arc::clone(&state);
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let (held_tx, held_rx) = mpsc::channel::<()>();
        let writer = std::thread::spawn(move || {
            let _guard = writer_state.tokens.teams_mut();
            let _ = held_tx.send(());
            let _ = release_rx.recv();
        });
        held_rx
            .recv()
            .expect("the simulated refresh must take the write guard");

        let mut snapshot = Box::pin(sync_status_offloaded(Arc::clone(&state)));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(
            matches!(snapshot.as_mut().poll(&mut cx), Poll::Pending),
            "the command must yield to the blocking pool while a refresh holds \
             the token lock, not assemble the snapshot on the caller's own \
             thread (issue #879)"
        );

        // The refresh commits, and the snapshot still lands — internally
        // consistent, reporting the slot it actually read.
        release_tx
            .send(())
            .expect("the simulated refresh must still be waiting");
        writer.join().expect("the simulated refresh must finish");
        let status = tauri::async_runtime::block_on(snapshot)
            .expect("the offloaded snapshot must be produced");
        assert!(
            !status.teams_connected && !status.spotify_connected,
            "an empty token slot must report disconnected, not a torn snapshot \
             (issues #398, #879)"
        );
    }

    /// Issue #941 acceptance, wiring half: the sentinel must be published by
    /// `start_syncing_with` itself, between the claim and the spawn, and the
    /// real poller id must replace it once the handle is stored — a sentinel
    /// that is never published, or never replaced, leaves the claim window
    /// ownerless again (or the poller's own exit cleanup unmatched).
    #[test]
    fn test_start_syncing_publishes_the_sentinel_between_claim_and_spawn() {
        let source = include_str!("sync.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("sync.rs has no #[cfg(test)] mod tests block");
        let body = fn_body(prod_source, "pub async fn start_syncing_with(");

        let claim = body
            .find("try_claim()")
            .expect("start_syncing_with must claim the polling flag");
        let sentinel = body
            .find("publish_start_sentinel(")
            .expect("start_syncing_with must publish the claim-window sentinel (issue #941)");
        let spawn = body
            .find("spawn_blocking(")
            .expect("start_syncing_with must spawn the poller");
        assert!(
            claim < sentinel && sentinel < spawn,
            "the sentinel must be published after try_claim and before the \
             poller spawn, so the whole claim window has an owner (issue #941)"
        );

        let store = body
            .find("*state.polling.thread_id_mut() = Some(tid)")
            .expect("start_syncing_with must store the real poller id");
        assert!(
            store > spawn,
            "the real poller id must replace the sentinel once the handle is \
             stored (issue #941)"
        );
        let sentinels = body.matches("publish_start_sentinel(").count();
        let clears = body
            .matches("*state.polling.thread_id_mut() = None")
            .count();
        assert_eq!(
            (sentinels, clears),
            (1, 2),
            "one publication, and None in both rollback paths (spawn_blocking \
             join failure and polling-start failure), so a failed start never \
             leaves an owner behind (issue #941)"
        );
    }

    /// Issue #941 (F2): a stop landing after `start_polling` installed its stop
    /// channel but before the handle store drops that fresh channel — the
    /// poller then takes `Disconnected` and breaks on its first loop check,
    /// while the drain, which has no handle to join, leaves the flag to the
    /// in-flight start. The start must conclude the stop instead of publishing
    /// a session that can never poll (which would show "Syncing" with nothing
    /// reaching Teams).
    #[test]
    fn test_start_concludes_a_stop_that_won_the_race() {
        use super::{conclude_raced_stop, publish_start_sentinel, AppState};
        use std::sync::atomic::Ordering;

        let state = AppState::new();

        // Start-wins: the racing stop landed before `start_polling` installed
        // the channel, so it closed nothing and the poller is alive.
        assert!(state.polling.try_claim());
        publish_start_sentinel(&state);
        let (stop_tx, _stop_rx) = std::sync::mpsc::channel::<()>();
        *state.polling.stop_tx_mut() = Some(stop_tx);
        assert!(
            !conclude_raced_stop(&state),
            "a live stop channel means the poller is running and the start owns \
             the session (issue #941)"
        );
        assert!(
            state.polling.is_syncing(Ordering::Acquire),
            "a start that won the race keeps is_syncing true (issue #941)"
        );

        // Stop-wins, exactly as the drain leaves it: the channel is gone, so
        // the poller is already breaking out, and the flag is still the
        // in-flight start's.
        *state.polling.stop_tx_mut() = None;
        assert!(
            conclude_raced_stop(&state),
            "a closed stop channel with the handle just stored means the stop \
             won (issue #941, F2)"
        );
        assert!(
            !state.polling.is_syncing(Ordering::Acquire),
            "concluding the raced stop must clear is_syncing, or the UI reports \
             Syncing while nothing polls (issue #941, F2)"
        );
        assert!(
            state.polling.thread_id().is_none(),
            "the owner entry must be released with the flag (issue #941, F2)"
        );
    }
}
