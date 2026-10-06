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

/// Shipped English values identify untouched paused-status fields at post
/// time; they are never written back into the user's configuration.
const DEFAULT_PAUSED_STATUS_FORMAT: &str = crate::i18n::EN.status_paused_default;

/// Resolve the paused text from one locale-explicit config snapshot. Only an
/// empty field or a byte-equal shipped English value is localized; every other
pub(crate) fn paused_status_text(
    config: &Option<std::sync::Arc<crate::config::AppConfig>>,
) -> &str {
    let locale = config.as_ref().and_then(|cfg| cfg.locale.as_deref());
    let configured = config
        .as_ref()
        .map(|cfg| cfg.teams.paused_status_format.as_str());
    match configured {
        Some(value) if !value.trim().is_empty() && value != DEFAULT_PAUSED_STATUS_FORMAT => value,
        _ => crate::i18n::strings_for(crate::i18n::resolve_tag(locale)).status_paused_default,
    }
}

/// Add the standard music prefix to the shared paused-text result.
pub(crate) fn paused_status_placeholder(
    config: &Option<std::sync::Arc<crate::config::AppConfig>>,
) -> String {
    format!(
        "{} {}",
        crate::polling::MUSIC_EMOJI,
        paused_status_text(config)
    )
}

/// Issue #870: the safe-placeholder text the manual status clear posts.
/// Uses the same snapshot resolver as polling and the headless clear.
pub fn safe_placeholder_text(state: &AppState) -> String {
    let config = state.config.get();
    paused_status_placeholder(&config)
}

/// Issue #870: the placeholder expiry the manual status clear carries —
/// 60 seconds, matching the paused/stopped clear paths so a user who
/// clears a manual status finds Teams in the same shape the auto-paused
/// path leaves it. Mirrors `placeholder_expiry_str` in the polling
/// module without importing a private function.
pub fn placeholder_expiry_minutes() -> u32 {
    // Exposed in minutes (the SyncStatus wire shape) for the Dashboard chip
    // and the diagnostics snapshot. The Graph-side string is built at the
    // call site through `teams::placeholder_expiry_rfc3339()` so this fn
    // is a single-purpose accessor.
    1
}

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
    state.polling.set_syncing(false);
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
    /// Issue #870: the user-composed manual status currently armed on
    /// Teams. `None` when the composer is empty, the user cleared it, or
    /// the expiry lapsed. The Dashboard composer reads this to decide
    /// whether to render the "Clear manual status" button.
    pub manual_status: Option<crate::commands::status::ManualStatus>,
    /// Issue #870: the three most-recently-used manual statuses, newest
    /// first. The Dashboard composer renders this as the quick-pick list
    /// and the tray's "Recent statuses" submenu reads the same array.
    pub recent_manual_statuses: Vec<crate::commands::status::RecentManualStatus>,
    /// Issue #813: replay of the startup legacy-plaintext migration conflict
    /// (`config::LegacySecretOutcome::ConflictKeychainDiffers`, persisted on
    /// `AppState::secret_conflict`). The one-shot `spotify-secret-conflict`
    /// event (issue #376) fires from the setup hook before any webview has
    /// mounted, so Settings reads this field next to `get_sync_status` in
    /// `onMount` and raises the reconnect banner from it. Cleared on a
    /// successful Spotify reconnect.
    pub spotify_secret_conflict: bool,
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
    let needs_drain = { state.polling.handle().is_some() || state.polling.is_syncing() };
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
        state.polling.set_syncing(false);
        *state.polling.thread_id_mut() = None;
        format!("start_syncing spawn_blocking panicked: {:?}", e)
    })?
    .map_err(|e| {
        // Roll back is_syncing flag and thread_id since no handle was created
        log::error!(
            "{CMD} start_syncing: polling start failed - {}; rolling back is_syncing",
            e
        );
        state.polling.set_syncing(false);
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
                    state_for_flag.polling.set_syncing(false);
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
            state_for_flag.polling.set_syncing(false);
            *state_for_flag.polling.thread_id_mut() = None;
        })
        .await;
        if let Err(e) = res {
            log::error!("{CMD} {}: spawn_blocking panicked: {:?}", context, e);
            // Ensure flag is cleared even if the blocking task panicked
            state.polling.set_syncing(false);
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
        if state.polling.is_syncing() {
            let owner = *state.polling.thread_id();
            match owner {
                None => {
                    log::warn!(
                        "{CMD} {context}: no polling handle and no owner thread while is_syncing is set; clearing wedged flag"
                    );
                    state.polling.set_syncing(false);
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
                    state_for_flag.polling.set_syncing(false);
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
                state_detached.polling.set_syncing(false);
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
        crate::events::SyncStopped {
            self_terminated: false,
        },
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

    let is_syncing = state.polling.is_syncing();

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
///
/// Issue #879 adds the DURATION half of that contract. The section covers
/// exactly: the four guards themselves, the `is_syncing` load, the
/// `current_track` clone, the three presence/connected booleans derived from
/// the guarded values, the `secret_conflict` atomic load, and the write-clocks
/// clone (it must stay inside: the poller publishes a track and THEN that
/// track's clocks, so reading the clocks after releasing `track_guard` could
/// pair a track with the next track's clocks). Everything after the drops —
/// the manual-status record (a `std::sync::Mutex`), the struct assembly, the
/// log line — runs with all four guards released, because the writers of
/// those same slots are the token refresh paths and must not queue behind
/// work that needs no lock. New code MUST NOT move work into the section, and
/// MUST NOT hold a guard while calling anything that acquires a lock.
/// Issue #1126 extends the DURATION half with a contention fallback. The two
/// token slots are probed with `try_read` (in this same order) before the
/// section: a writer holding either slot makes the probe miss instead of
/// parking the snapshot, and the fn serves the last cached `SyncStatus`
/// (published on `AppState` by the fresh path only). The fresh path below is
/// unchanged — same order, same instant, same bytes.
#[tauri::command]
pub async fn get_sync_status(state: tauri::State<'_, Arc<AppState>>) -> Result<SyncStatus, String> {
    log::debug!("{CMD} get_sync_status: ENTRY");
    sync_status_offloaded(Arc::clone(state.inner())).await
}

/// Issue #879: `get_sync_status`'s snapshot, handed to the blocking pool.
///
/// The snapshot takes four read guards on slots whose writers are the token
/// refresh paths (the poller's Teams commit, the boot gate's Spotify
/// refresh). As a synchronous command this ran on the main thread, so the
/// Dashboard's mount-time and post-event calls parked the webview's UI thread
/// until those guards were available: a frozen window rather than a late
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
///
/// Issue #879: the four read guards are held only for the values that must
/// share one instant with each other — the `current_track` clone, the
/// connected/paused booleans derived from the guarded values, `is_syncing`,
/// the `secret_conflict` flag and the write clocks — and are dropped
/// immediately afterwards. What runs after the drops (the manual-status
/// record, the struct assembly, the log line) runs with the token slots free,
/// which matters because the writers of those same slots are the refresh
/// paths: the poller's token commit and the boot gate's refresh both take
/// `tokens.*_mut()`, so a status command that held a token read guard across
/// its tail work queued itself in front of a refresh for work that never
/// needed a lock. The manual-status record and the recent-status ring are
/// process-global state that the poller never pairs with a track, so reading
/// them outside the section cannot tear the snapshot.
///
/// Issue #1126: token-slot contention serves the previous snapshot. Both
/// token slots are probed with `try_read` first — in the #398 order — and a
/// miss on either one returns the last cached `SyncStatus` instead of
/// blocking. The served value is NEVER a conservative default: a synthesized
/// `teams_connected: false` would flash the Dashboard's disconnected banner
/// during every refresh, while the cache is the last state the Dashboard
/// already rendered. First call with contention and an empty cache has no
/// previous instant to serve, so the fn falls through to the blocking fresh
/// path — the one wait is bounded by the writer that is already finishing,
/// and every later call hits the cache. The log names only the slot
/// (`spotify` / `teams`); token contents never reach a log line.
pub fn sync_status_from_state(state: &AppState) -> SyncStatus {
    // Issue #1126: contention probe. Both token slots are `try_read` in the
    // #398 order BEFORE any guard is taken, so a writer holding either slot
    // (the poller's commit, a reconnect/disconnect clear) makes the probe
    // miss instead of parking this snapshot. On a miss the fn serves the
    // last cached `SyncStatus` — the last instant the Dashboard already
    // rendered — never a conservative default (a synthesized
    // `teams_connected: false` would flash the disconnected banner during
    // every refresh). The only case with no cache is the first call racing
    // a writer before any fresh assembly ever ran: there is no previous
    // instant to serve, so the fn falls through to the blocking fresh path
    // below and the cache fills for every later call. Slot names only —
    // no token contents — reach the log line.
    let (spotify_missed, teams_missed) = (
        state.tokens.try_spotify().is_none(),
        state.tokens.try_teams().is_none(),
    );
    if spotify_missed || teams_missed {
        if let Some(cached) = state.last_sync_snapshot.read().clone() {
            let slot = match (spotify_missed, teams_missed) {
                (true, true) => "spotify+teams",
                (true, false) => "spotify",
                _ => "teams",
            };
            log::info!(
                "{CMD} sync_status_from_state: token slot `{}` contended, serving the previous snapshot",
                slot
            );
            return cached;
        }
        log::info!(
            "{CMD} sync_status_from_state: token slot contended with an empty snapshot cache, taking the blocking fresh path once"
        );
    }
    // Single critical section: all read guards held at once, clones below
    // cannot observe a writer interleaving between fields.
    let track_guard = state.polling.current_track();
    let spotify_guard = state.tokens.spotify();
    let config_guard = state.config.get();
    let teams_guard = state.tokens.teams();
    let is_syncing = state.polling.is_syncing();

    let current_track = track_guard.clone();
    // A pause is a state change, not a stop: the poller keeps the observed
    // `TrackInfo` for a paused track (finding D6, #669), so the stored
    // playback state is what the Dashboard renders as a paused card.
    let presence_paused = current_track.as_ref().is_some_and(|t| !t.is_playing);
    let spotify_session_present = spotify_guard.is_some();
    let spotify_client_configured = config_guard
        .as_ref()
        .map(|c| !c.spotify.client_id.is_empty())
        .unwrap_or(false);
    let teams_session_present = teams_guard.is_some();
    // The two reads that must stay INSIDE the section, because their values
    // are only consistent with `current_track` while `track_guard` is held:
    // the poller publishes a new track and THEN its write clocks for that
    // track, so reading the clocks after releasing `track_guard` can pair a
    // track with the next track's `presence_gated` / `last_posted_status`.
    // Both are cheap: `secret_conflict` is an atomic load and `WRITE_CLOCKS`
    // is a documented leaf lock (every production touch clones it and never
    // acquires another lock while holding it), so neither inverts the
    // `current_track -> spotify -> config -> teams` ordering (#398).
    // Issue #813: the startup migration conflict is process state, not lock
    // state.
    let spotify_secret_conflict = state.secret_conflict.load(Ordering::Acquire);
    // #670: the poller's presence bookkeeping, read from the session's
    // write-decision clocks (finding PollCore#4 / #572).
    let clocks = polling::load_write_clocks(&state.session);

    // Issue #879: end of the critical section. The two connected booleans
    // below are computed from values read above, and every field that had to
    // agree with `current_track` was read above the drops — so releasing here
    // cannot make the snapshot inconsistent. What is left below is the
    // manual-status record, the struct assembly and the log line, none of
    // which can be observed torn against a track.
    drop(track_guard);
    drop(spotify_guard);
    drop(config_guard);
    drop(teams_guard);
    #[cfg(test)]
    section_released_probe();

    // --- no AppState guard is held below this line ---
    let spotify_connected = spotify_session_present && spotify_client_configured;
    let teams_connected = teams_session_present;

    log::info!(
        "{CMD} sync_status_from_state: is_syncing={}, spotify_connected={}, teams_connected={}, presence_gated={}, presence_paused={}",
        is_syncing,
        spotify_connected,
        teams_connected,
        clocks.gated_track_key.is_some(),
        presence_paused
    );

    let status = SyncStatus {
        is_syncing,
        current_track,
        spotify_connected,
        teams_connected,
        last_posted_status: clocks.last_posted_status.clone(),
        presence_gated: clocks.gated_track_key.is_some(),
        presence_paused,
        manual_status: crate::commands::status::load_manual_status(),
        recent_manual_statuses: crate::commands::status::load_recent_statuses(),
        spotify_secret_conflict,
    };
    // Issue #1126: publish the fresh assembly for the contention arm above.
    // Fresh-path only — the fallback arm returns before reaching this line,
    // so a stale snapshot can never overwrite a newer one with itself.
    *state.last_sync_snapshot.write() = Some(status.clone());
    status
}

// Issue #879 test seam: fires at the exact instant the snapshot releases its
// critical section — guards dropped, the rest of the snapshot still to build —
// so a test can observe the token slots from a would-be writer while the
// command is mid-assembly. Compiled out of production builds, so the command
// path carries no cost for it.
#[cfg(test)]
thread_local! {
    static SECTION_RELEASED_PROBE: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        std::cell::RefCell::new(None);
}

/// Install `probe` for the duration of `body`, on this thread.
#[cfg(test)]
pub(super) fn with_section_released_probe<R>(
    probe: impl FnOnce() + 'static,
    body: impl FnOnce() -> R,
) -> R {
    SECTION_RELEASED_PROBE.with(|slot| *slot.borrow_mut() = Some(Box::new(probe)));
    let assembled = body();
    SECTION_RELEASED_PROBE.with(|slot| slot.borrow_mut().take());
    assembled
}

/// One-shot invocation point for [`with_section_released_probe`].
#[cfg(test)]
fn section_released_probe() {
    let probe = SECTION_RELEASED_PROBE.with(|slot| slot.borrow_mut().take());
    if let Some(probe) = probe {
        probe();
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

    if !state.polling.is_syncing() {
        return Err("Sync is not running".to_string());
    }

    let state_inner = Arc::clone(state.inner());
    let app_clone = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::polling::run_oneshot(&state_inner, &app_clone);
        let is_syncing = state_inner.polling.is_syncing();
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
            // Issue #1126: the contention probe runs ahead of the section in
            // the same order, so its markers join the #398 order guard.
            "state.tokens.try_spotify()",
            "state.tokens.try_teams()",
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

        let state = AppState::new();
        assert_eq!(
            missing_session_code(&state),
            Some("spotify_not_connected"),
            "with no session at all the guard must name Spotify first (issue #809)"
        );
        assert!(
            !state.polling.is_syncing(),
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

    #[test]
    fn sync_status_reports_every_connection_combination() {
        use super::{sync_status_from_state, AppState};

        let state = AppState::new();
        let mut config = crate::config::AppConfig::default();
        config.spotify.client_id = "spotify-client".to_string();
        *state.config.get_mut() = Some(std::sync::Arc::new(config));

        let spotify_tokens = crate::spotify::SpotifyTokens {
            access_token: "spotify-access".to_string(),
            refresh_token: "spotify-refresh".to_string(),
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        };
        let teams_tokens = crate::teams::TeamsTokens {
            access_token: "teams-access".to_string(),
            refresh_token: Some("teams-refresh".to_string()),
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        };

        let connections = |state: &AppState| {
            let status = sync_status_from_state(state);
            (status.spotify_connected, status.teams_connected)
        };

        assert_eq!(connections(&state), (false, false));

        *state.tokens.spotify_mut() = Some(spotify_tokens.clone());
        assert_eq!(connections(&state), (true, false));

        *state.tokens.spotify_mut() = None;
        *state.tokens.teams_mut() = Some(teams_tokens);
        assert_eq!(connections(&state), (false, true));

        *state.tokens.spotify_mut() = Some(spotify_tokens);
        assert_eq!(connections(&state), (true, true));
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
            state.polling.is_syncing(),
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
        use std::sync::Arc;

        let state = Arc::new(AppState::new());
        state.polling.set_syncing(true);
        assert!(state.polling.thread_id().is_none());

        tauri::async_runtime::block_on(stop_polling_and_join(
            Arc::clone(&state),
            "test_wedged_flag",
        ));

        assert!(
            !state.polling.is_syncing(),
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

    /// Issue #879, critical-section half: ALL FOUR read guards must be
    /// released as soon as the values they protect have been read, so nothing
    /// else in the snapshot — the manual-status record, the struct assembly,
    /// the log line — runs with a slot held. Pre-fix the guards lived to the
    /// end of the fn, so the writers of those same slots (the poller's track
    /// store and token commit, the config save, the boot gate's refresh) all
    /// queued behind work that never needed a lock.
    ///
    /// The probe fires at the release instant and stands a writer on another
    /// thread that takes all four slots in the section's documented order and
    /// signals only after the fourth: it must get in while the snapshot is
    /// still mid-assembly. Removing any one of the four drops leaves the
    /// writer blocked and fails this test.
    #[test]
    fn test_snapshot_releases_every_section_guard_before_its_tail_work() {
        use super::{sync_status_from_state, with_section_released_probe, AppState};
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::mpsc;
        use std::sync::Arc;
        use std::time::Duration;

        let state = Arc::new(AppState::new());
        let slots_free = Arc::new(AtomicBool::new(false));
        let probe_flag = Arc::clone(&slots_free);
        let probe_state = Arc::clone(&state);

        let status = with_section_released_probe(
            move || {
                let writer_state = Arc::clone(&probe_state);
                let (taken_tx, taken_rx) = mpsc::channel::<()>();
                let writer = std::thread::spawn(move || {
                    // All four slots of the snapshot's section, taken in the
                    // documented order (`current_track -> spotify -> config ->
                    // teams`), and signalling only after the fourth is held —
                    // so one bounded wait covers every guard, and a release
                    // order matching the section cannot deadlock here.
                    let _track = writer_state.polling.current_track_mut();
                    let _spotify = writer_state.tokens.spotify_mut();
                    let _config = writer_state.config.get_mut();
                    let _teams = writer_state.tokens.teams_mut();
                    let _ = taken_tx.send(());
                });
                // Bounded: a snapshot that still holds any of the four guards
                // at this instant never lets the writer in, and the wait ends.
                let taken = taken_rx.recv_timeout(Duration::from_secs(5)).is_ok();
                if taken {
                    writer.join().expect("the writer thread must finish");
                }
                probe_flag.store(taken, Ordering::Release);
            },
            || sync_status_from_state(&state),
        );

        assert!(
            slots_free.load(Ordering::Acquire),
            "the snapshot must drop ALL FOUR guards as soon as it has read their \
             values — a writer standing in for the poller's track store or token \
             commit, the config save, or the boot gate's refresh was still blocked while the tail work ran (issue #879)"
        );
        assert!(
            !status.teams_connected && !status.spotify_connected,
            "narrowing the section must not change what the snapshot reports: an \
             empty token slot still reads as disconnected (issues #398, #879)"
        );
    }

    /// Issue #1126, sibling of the release-instant probe above: with a token
    /// write guard held — the poller's commit or a reconnect/disconnect clear
    /// mid-write — the snapshot must still complete bounded and serve the
    /// previous instant, never block on the writer and never synthesize a
    /// conservative default (a fabricated `teams_connected: false` would
    /// flash the disconnected banner during every refresh). Pre-fix the
    /// token slots had no `try_read`, so the snapshot parked on the held
    /// guard and the bounded wait below expired: this test fails there.
    #[test]
    fn test_snapshot_serves_the_previous_instant_while_a_token_slot_is_held() {
        use super::{sync_status_from_state, AppState};
        use std::sync::Arc;
        use std::time::Duration;

        let state = Arc::new(AppState::new());
        let mut config = crate::config::AppConfig::default();
        config.spotify.client_id = "spotify-client".to_string();
        *state.config.get_mut() = Some(std::sync::Arc::new(config));
        *state.tokens.spotify_mut() = Some(crate::spotify::SpotifyTokens {
            access_token: "spotify-access".to_string(),
            refresh_token: "spotify-refresh".to_string(),
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        });
        *state.tokens.teams_mut() = Some(crate::teams::TeamsTokens {
            access_token: "teams-access".to_string(),
            refresh_token: Some("teams-refresh".to_string()),
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        });

        // The previous instant: a fresh assembly while nothing is held, so
        // the cache fills and the expected value is pinned.
        let previous = sync_status_from_state(&state);
        assert!(
            previous.spotify_connected && previous.teams_connected,
            "the pre-contention snapshot must see both sessions (issue #1126)"
        );

        // The poller's commit, standing in: holds the Teams slot across the
        // whole read below.
        let writer_state = Arc::clone(&state);
        let held = writer_state.tokens.teams_mut();
        let reader_state = Arc::clone(&state);
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let status = sync_status_from_state(&reader_state);
            let _ = done_tx.send(status);
        });
        // Bounded: pre-fix the token slots had no `try_read`, so the snapshot
        // parked on the held guard and this wait expires. Post-fix the probe
        // misses, the cache answers, and the reader finishes while `held`
        // is still alive.
        let contended = done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the contended snapshot must complete bounded, not park on the held token slot (issue #1126)");
        // Dropping here ends the stand-in commit.
        drop(held);
        assert!(
            contended.spotify_connected && contended.teams_connected,
            "a contended token slot must serve the previous snapshot, not a \
             conservative default that flashes the disconnected banner (issue #1126)"
        );
        assert_eq!(
            serde_json::to_value(&contended).expect("SyncStatus must serialise"),
            serde_json::to_value(&previous).expect("SyncStatus must serialise"),
            "the contended answer must equal the previous instant field-for-field (issue #1126)"
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
            state.polling.is_syncing(),
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
            !state.polling.is_syncing(),
            "concluding the raced stop must clear is_syncing, or the UI reports \
             Syncing while nothing polls (issue #941, F2)"
        );
        assert!(
            state.polling.thread_id().is_none(),
            "the owner entry must be released with the flag (issue #941, F2)"
        );
    }

    /// Issue #813: the startup migration conflict must be replayable — a
    /// `ConflictKeychainDiffers` outcome persists on `AppState::secret_conflict`
    /// and `sync_status_from_state` surfaces it, so a Settings view mounting
    /// after setup (when the one-shot `spotify-secret-conflict` event has
    /// already fired into the void) still raises the reconnect banner.
    /// Clearing on a successful reconnect stops the banner re-appearing.
    #[test]
    fn test_sync_status_replays_the_secret_conflict_flag() {
        use super::{sync_status_from_state, AppState};
        use std::sync::atomic::Ordering;

        let state = AppState::new();
        // Fresh state: no conflict — banner stays hidden.
        assert!(
            !sync_status_from_state(&state).spotify_secret_conflict,
            "a fresh AppState must report no secret conflict (issue #813)"
        );
        // The setup hook persists `ConflictKeychainDiffers` on the flag.
        state.secret_conflict.store(true, Ordering::Release);
        assert!(
            sync_status_from_state(&state).spotify_secret_conflict,
            "a stored conflict must surface through get_sync_status so a late-mounting Settings raises the banner (issue #813)"
        );
        // A completed reconnect clears it.
        state.secret_conflict.store(false, Ordering::Release);
        assert!(
            !sync_status_from_state(&state).spotify_secret_conflict,
            "clearing the flag on reconnect must hide the banner again (issue #813)"
        );
    }
    #[test]
    fn manual_status_clear_uses_config_paused_fallback() {
        use super::{safe_placeholder_text, AppState};
        let state = AppState::new();
        for (locale, fallback, custom) in [
            ("de", "Pausiert", "Kurze Pause ✨"),
            ("fr", "En pause", "Pause ✨ personnalisée"),
        ] {
            let mut config = crate::config::AppConfig {
                locale: Some(locale.to_string()),
                ..Default::default()
            };
            for configured in ["", "Paused", "   "] {
                config.teams.paused_status_format = configured.to_string();
                *state.config.get_mut() = Some(std::sync::Arc::new(config.clone()));
                assert_eq!(
                    safe_placeholder_text(&state),
                    format!("🎵 {fallback}"),
                    "empty, whitespace-only, and shipped-English paused text use the config locale"
                );
            }

            config.teams.paused_status_format = custom.to_string();
            *state.config.get_mut() = Some(std::sync::Arc::new(config));
            assert_eq!(
                safe_placeholder_text(&state),
                format!("🎵 {custom}"),
                "custom paused text remains byte-identical"
            );
        }
    }
}
