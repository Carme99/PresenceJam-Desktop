use std::sync::atomic::Ordering;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tauri::{
    menu::{
        CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder, PredefinedMenuItem, Submenu,
        SubmenuBuilder,
    },
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Listener, Manager,
};

use crate::spotify::RepeatState;

use crate::i18n::{self, Strings};

// Menu item IDs
const ID_SHOW_HIDE: &str = "show_hide_window";
const ID_PAUSE_SYNC: &str = "pause_sync";
const ID_RESUME_SYNC: &str = "resume_sync";
const ID_CURRENT_TRACK: &str = "current_track";
/// Disabled head-of-menu item that states what the app is actually doing
/// (issue #591) — the Pause/Resume verb alone left sync state unstated, and
/// the presence-gate badge is macOS-only.
const ID_SYNC_STATUS: &str = "sync_status";
const ID_OPEN_SETTINGS: &str = "settings";
const ID_OPEN_LOGS: &str = "open_logs";
const ID_QUIT: &str = "quit";
// Spotify playback control (issue #3.0-P3). Device submenu items carry
// ids of the form `{ID_DEVICES}|{spotify device id}` so the click handler
// resolves the stable id instead of racing a list index (issue #388).
const ID_PLAY_PAUSE: &str = "play_pause";
const ID_PREVIOUS: &str = "previous";
const ID_NEXT: &str = "next";
/// Shuffle toggle (issue #582). A check item whose mark comes from the
/// `shuffle_state` the poll body already carries — no extra request. The
/// label is the `shuffle` entry of the i18n table (issue #674): the state
/// itself stays the native check mark.
const ID_SHUFFLE: &str = "shuffle";
/// Repeat toggle (issue #582). Repeat has THREE states (`off`/`context`/
/// `track`), so the item's label spells the mode out — a check mark alone
/// cannot tell `context` from `track`.
const ID_REPEAT: &str = "repeat";
const ID_DEVICES: &str = "devices";
const ID_QUEUE: &str = "queue";
/// Menu-item id prefix for device submenu entries (`{ID_DEVICES}|{device id}`).
/// `concat!` cannot take a const, so this mirrors `ID_DEVICES` literally;
/// keep the two in sync when either changes.
const DEVICE_ITEM_PREFIX: &str = "devices|";

// Snooze submenu (4.7.0, S9 / issue #677). The three unqualified ids are the
// presets; the fourth only exists while a snooze is active, so the way out of a
// snooze always sits beside the way in.
const ID_SNOOZE_30: &str = "snooze|30m";
const ID_SNOOZE_1H: &str = "snooze|1h";
const ID_SNOOZE_TOMORROW: &str = "snooze|tomorrow";
const ID_SNOOZE_RESUME: &str = "snooze|resume";
/// Menu-item id prefix for the snooze submenu. Mirrors the ids above literally
/// (`concat!` cannot take a const); keep both in sync when either changes.
const SNOOZE_ITEM_PREFIX: &str = "snooze|";
const SNOOZE_30_SUFFIX: &str = "30m";
const SNOOZE_1H_SUFFIX: &str = "1h";
const SNOOZE_TOMORROW_SUFFIX: &str = "tomorrow";
const SNOOZE_RESUME_SUFFIX: &str = "resume";

static TRAY: OnceLock<TrayIcon> = OnceLock::new();

/// Get the global TrayIcon instance.
pub fn get_tray() -> Option<&'static TrayIcon> {
    TRAY.get()
}

pub fn setup_tray(app: &tauri::App) -> Result<(), String> {
    // 4.7.0 (issue #674): the tray renders in the locale stored in the config.
    // Installed before the first menu build so the initial menu (and the
    // immediately following `update_tray_menu`) already carry the right
    // labels; an unknown/absent value installs English.
    let state = app.state::<std::sync::Arc<crate::AppState>>();
    crate::i18n::install_from_app_state(state.inner());
    // 4.7.0 (S9 / issue #677): the startup cleanup for a snooze deadline that
    // expired while the app was closed. Runs before the first menu build so the
    // tray never renders a state the config no longer holds, and only writes
    // when there is actually something to clear.
    clear_expired_snooze_at_startup(app.handle());
    // Issue #768: the tray is built WITHOUT a menu and `update_tray_menu` below
    // sets the real one. The transient initial menu this used to build was a
    // second, already-diverged layout (no status row, no now-playing row,
    // hardcoded Pause/Show labels) that the immediate rebuild replaced
    // microseconds later — and that a FAILED rebuild left on screen. An empty
    // tray can only be empty; it is reachable for the first moments of
    // startup, while the window is not yet interactive.

    let builder = TrayIconBuilder::new()
        .tooltip("PresenceJam")
        .icon(
            app.default_window_icon()
                .cloned()
                .ok_or("No default icon")?,
        )
        // Issue #971: Tauri documents this flag as unsupported on Linux, where
        // a left click opens the AppIndicator menu unconditionally. It only
        // ever changes behaviour on Windows and macOS.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            ID_SHOW_HIDE => {
                if let Some(window) = app.get_webview_window("main") {
                    if window.is_visible().unwrap_or(false) {
                        let _ = window.hide();
                        // Issue #886: the dedup key reads the visibility mirror,
                        // so the tray's own window changes must report it.
                        note_window_visibility(false);
                    } else {
                        let _ = window.show();
                        note_window_visibility(true);
                        // Issue #483: a minimized window stays minimized
                        // after show() -- unminimize first (mirrors the
                        // single-instance raise in lib.rs and show_window).
                        let _ = window.unminimize();
                        let _ = window.set_focus();
                    }
                } else {
                    // Residual of #826: this arm is the tray's second window-raise
                    // path, and it must not stay silent about the same condition
                    // `commands::window::show_window` warns about.
                    log::warn!("[TRAY] show/hide: main window not found");
                }
                // Issue #587: the repaint performs blocking Spotify HTTP
                // (devices/queue fetches, 10 s timeout each) whenever the
                // 60 s throttle has lapsed, so it must never run on the
                // menu-event thread — offload it like the #386 player arms.
                refresh_tray_from_state(app);
            }
            ID_PAUSE_SYNC | ID_RESUME_SYNC => {
                // Issue #588: this arm only *asks* the frontend to toggle
                // (the frontend owns the start/stop call), so the running
                // flag settles asynchronously. Watch it from a worker and
                // repaint from backend truth, instead of relying on the
                // Dashboard route being mounted to mirror the change back.
                let before = app
                    .state::<std::sync::Arc<crate::AppState>>()
                    .polling
                    .is_syncing(Ordering::Acquire);
                let _ = app.emit("toggle-pause", ());
                let app_handle = app.clone();
                std::thread::spawn(move || {
                    if !await_sync_toggle(&app_handle, before) {
                        log::debug!(
                            "[TRAY] pause/resume: sync flag unchanged after {:?}; repainting anyway",
                            TOGGLE_SETTLE_TIMEOUT
                        );
                    }
                    repaint_tray_from_state(&app_handle, "pause/resume");
                });
            }
            ID_QUIT => {
                // Issue #383: Quit must terminate the process even with no
                // frontend listener — the old hide-only arm wedged the app
                // in the tray with no way out. Route through the shared
                // graceful shutdown (emits app-shutdown, then exits
                // unconditionally after SHUTDOWN_GRACE).
                crate::menu::request_graceful_shutdown(app);
            }
            // Menu items handled by app menu (settings, open_logs) also come through here
            ID_OPEN_SETTINGS => {
                let _ = app.emit("navigate", "settings");
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                note_window_visibility(true);
                    // Issue #483: mirror the unminimize in the Show arm.
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
                // Issue #592: showing the window here changes the Show/Hide
                // label, so the tray must be repainted from backend state.
                refresh_tray_from_state(app);
            }
            ID_OPEN_LOGS => {
                let _ = app.emit("open-logs-folder", ());
            }
            // Spotify playback control (issue #3.0-P3). These dispatch
            // directly against the Spotify API with the stored access
            ID_PLAY_PAUSE => {
                // Issue #386: the click-path blocking Spotify HTTP must not
                // run on the menu-event thread — a slow network would wedge
                // the tray menu. Offload everything (the currently-playing
                // GET plus the play/pause action) to a worker thread.
                //
                // Issue #586: the worker resolves its token through the
                // shared refresh-aware policy rather than snapshotting
                // `state.tokens.spotify()`, so an expired access token is
                // refreshed (and retried once) exactly like the command layer.
                let app_handle = app.clone();
                std::thread::spawn(move || {
                    // Resolve the ACTUAL playing state from the API rather
                    // than the stored track: the polling loop's `is_playing`
                    // goes stale on a same-track pause (it's only re-stored
                    // on title/artist change), and an external device may
                    // have changed state since. One extra GET per click is
                    // fine — this is user-initiated. Unknown → resume.
                    // Unconditional GET (`None`): user-initiated one-off
                    // click with no stored validator. C11 signature.
                    let state = app_handle.state::<std::sync::Arc<crate::AppState>>();
                    let should_pause = match crate::commands::playback::player_with_refresh_typed(
                        state.inner(),
                        &app_handle,
                        "play/pause state",
                        |token| crate::spotify::get_currently_playing(token, None),
                    ) {
                        Ok(crate::spotify::CurrentlyPlaying::Modified {
                            now: Some(now),
                            ..
                        }) => now.media.is_playing,
                        Ok(_) => false,
                        Err(e) => {
                            log::warn!("[TRAY] play/pause: playback state read failed: {}", e);
                            let _ = app_handle.emit("playback-error", e.to_string());
                            return;
                        }
                    };
                    if should_pause {
                        run_player_action(
                            &app_handle,
                            "pause",
                            Some(false),
                            None,
                            |t| crate::spotify::player_pause(t, None),
                        );
                    } else {
                        run_player_action(
                            &app_handle,
                            "play",
                            Some(true),
                            None,
                            |t| crate::spotify::player_play(t, None),
                        );
                    }
                });
            }
            ID_PREVIOUS => {
                // Issue #386: offload the blocking Spotify HTTP off the
                // menu-event thread. Skipping doesn't change playing state.
                let app_handle = app.clone();
                std::thread::spawn(move || {
                    run_player_action(&app_handle, "previous", None, None, |token| {
                        crate::spotify::player_previous(token, None)
                    });
                });
            }
            ID_NEXT => {
                // Issue #386: offload the blocking Spotify HTTP off the
                // menu-event thread. Skipping doesn't change playing state.
                let app_handle = app.clone();
                std::thread::spawn(move || {
                    run_player_action(&app_handle, "next", None, None, |token| {
                        crate::spotify::player_next(token, None)
                    });
                });
            }
            ID_SHUFFLE => {
                // Issue #582: the target state is the inverse of the last
                // state we know about (the poll body's `shuffle_state`, or
                // this item's own last successful toggle). Issue #386: the
                // blocking Spotify HTTP must not run on the menu-event
                // thread.
                //
                // The new state is handed to `run_player_action` instead of
                // being stored here: that function records it in its success
                // arm *before* the menu rebuild it triggers, so the rebuilt
                // item shows the state the API just accepted (storing after
                // the call would repaint the old state and no later rebuild
                // would correct it). On a 403 from a non-Premium account
                // nothing is recorded and the item keeps showing the truth.
                let app_handle = app.clone();
                std::thread::spawn(move || {
                    let target = shuffle_toggle_target(LAST_SHUFFLE_STATE.load(Ordering::Acquire));
                    let repeat = last_repeat_state();
                    run_player_action(
                        &app_handle,
                        "shuffle",
                        None,
                        Some((target, repeat)),
                        |token| crate::spotify::player_set_shuffle(token, target, None),
                    );
                });
            }
            ID_REPEAT => {
                // Issue #582: repeat cycles off → context → track → off,
                // matching Spotify's own player button, so "repeat one" is
                // reachable from the tray. Same off-thread and
                // record-on-success discipline as Shuffle above.
                let app_handle = app.clone();
                std::thread::spawn(move || {
                    let target = last_repeat_state().next();
                    let shuffle = LAST_SHUFFLE_STATE.load(Ordering::Acquire);
                    run_player_action(
                        &app_handle,
                        "repeat",
                        None,
                        Some((shuffle, target)),
                        |token| crate::spotify::player_set_repeat(token, target, None),
                    );
                });
            }
            id if id.starts_with(SNOOZE_ITEM_PREFIX) => {
                // S9 (issue #677): a snooze click writes `config.json` (atomic
                // write + fsync), so it must run off the menu-event thread like
                // every other arm here — a slow disk would otherwise wedge the
                // native menu. The repaint follows the write, so it renders the
                // new countdown, the "Resume sync now" entry and the
                // snooze-forced cache-only fetch mode in one pass.
                // The id is copied out of the borrowed `event` before the
                // worker takes ownership (it is used in the unknown-id warn).
                let raw = id.to_string();
                let selection = parse_snooze_menu_id(&raw);
                let app_handle = app.clone();
                std::thread::spawn(move || {
                    match selection {
                        Some(SnoozeMenuSelection::Preset(preset)) => {
                            let now_utc = chrono::Utc::now();
                            let deadline = crate::config::snooze_preset_deadline(
                                preset,
                                now_utc,
                                chrono::Local::now(),
                            );
                            if let Err(e) = write_snooze(&app_handle, Some(deadline)) {
                                log::error!("[TRAY] snooze: could not store the deadline: {}", e);
                            }
                        }
                        Some(SnoozeMenuSelection::Resume) => {
                            if let Err(e) = write_snooze(&app_handle, None) {
                                log::error!("[TRAY] snooze: could not clear the deadline: {}", e);
                            }
                        }
                        None => {
                            log::warn!("[TRAY] snooze: unrecognized menu id '{}'", raw);
                        }
                    }
                    repaint_tray_from_state(&app_handle, "snooze");
                });
            }
            id if id.starts_with(DEVICE_ITEM_PREFIX) => {
                // Device submenu item: `{ID_DEVICES}|{stable device id}`
                // resolved by id (issue #388), with a live re-fetch fallback
                // when the cached list went stale. Issue #386: the whole
                // resolution + transfer runs on a worker thread so no HTTP
                // touches the menu-event thread.
                let raw = id
                    .strip_prefix(DEVICE_ITEM_PREFIX)
                    .unwrap_or("")
                    .to_string();
                let selected = parse_device_menu_id(&raw);
                let app_handle = app.clone();
                std::thread::spawn(move || {
                    let device_id = resolve_device_id(&app_handle, &selected);
                    match device_id {
                        Some(device_id) => {
                            // Transfer starts playback on the target device.
                            run_player_action(&app_handle, "transfer", Some(true), None, |token| {
                                crate::spotify::player_transfer(token, &device_id, true)
                            });
                        }
                        None => {
                            log::warn!(
                                "[TRAY] transfer: unknown or id-less device selected (id={})",
                                selected_for_log(&selected)
                            );
                        }
                    }
                });
            }
            _ => {}
        })
        // Issue #971: Linux delivers no tray click events at all (Tauri lists
        // `TrayIconEvent::Click` as unsupported there), so the `tray-click`
        // emit below — and the frontend listener for it — are inert on that
        // platform. Nothing may depend on that event: the window is raised from
        // the menu's Show/Hide item, which every platform has.
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                let _ = app.emit("tray-click", ());
            }
        })
        ;
    // Issue #927: the tray build is the one call here that panics instead of
    // returning `Err`. On Linux it dlopens libayatana-appindicator3.so.1 /
    // libappindicator3.so.1 through `libappindicator-sys`, whose `Lazy<Library>`
    // panics when neither soname resolves — and this runs inline on the setup
    // thread, so the panic would unwind across the event-loop callback instead of
    // becoming the error `lib.rs` already handles by running without a tray.
    // The panic is raised on this thread, above the FFI boundary, so it is
    // catchable.
    let tray = guard_tray_panic(
        "tray icon",
        std::panic::AssertUnwindSafe(|| builder.build(app).map_err(|e| e.to_string())),
    )?;

    // Store the TrayIcon globally (idempotent)
    if TRAY.get().is_some() {
        log::warn!("[TRAY] setup_tray: already initialized, skipping");
        return Ok(());
    }
    TRAY.set(tray)
        .map_err(|_| "Tray already initialized".to_string())?;

    // Issue #689 (D6): the polling loop emits `playback-state-changed` when
    // a track's playing state changes without the track itself changing.
    // The Play/Pause mark and the status line read `LAST_PLAYING_STATE`,
    // which a rebuild only re-seeds on a new track key — so a same-track
    // pause would leave the mark claiming "playing" until the next track.
    // Consuming the event here keeps the tray's belief truthful; the
    // poller's own re-store then drives the rebuild that paints it.
    app.listen("playback-state-changed", |event| {
        consume_playback_state_changed(event.payload());
    });

    // Immediately set the real menu to reflect actual state (Bug 11 fix).
    // Without this the tray would stay menu-less until the first poll, and a
    // track cached from the previous session would not be shown.
    // Issue #768: there is no throwaway menu underneath this any more, so a
    // failure here cannot leave a "Pause Sync"-labelled stale layout behind.
    // The dedup snapshot is only committed by a successful rebuild, so the
    // next poll retries this paint.
    // Issue #882: this paint is CACHE-ONLY. It runs on the main thread inside
    // Tauri's `setup()`, before the event loop starts and while the dedup
    // snapshot is still empty — so a normal rebuild would issue the devices
    // and queue GETs right here (10 s timeout each) with no window on screen
    // to explain the wait, and could never take the "nothing changed" early
    // return.
    let state = app.state::<std::sync::Arc<crate::AppState>>();
    let is_syncing = state.polling.is_syncing(Ordering::Acquire);
    let current_track = state.polling.current_track().clone();
    if let Err(e) = update_tray_menu_startup(app.handle(), is_syncing, current_track) {
        log::warn!(
            "[TRAY] setup_tray: failed to update initial tray menu: {}",
            e
        );
    }
    // Issue #882: the real devices/queue fetch belongs to the worker refresh,
    // off the setup thread, so the startup paint costs no network at all and
    // the submenus still converge to the full content a moment later.
    refresh_tray_from_state(app.handle());

    log::info!("[TRAY] setup_tray: system tray initialized successfully");
    Ok(())
}

/// Snapshot of the last tray-menu state, used for the dedup guard in
/// `update_tray_menu` (issue #71). The polling thread calls
/// `update_tray_menu` on every successful poll; the menu only needs to
/// change when `is_syncing`, window visibility (the Show/Hide label), the
/// current track's title/is_playing, or one of the two playback modes
/// changes.
///
/// The two playback modes are part of the key (issue #691): their atoms
/// drive the Shuffle/Repeat check marks, so an external mode change has to
/// force a rebuild — otherwise the marks stayed on the previous mode. The
/// click target is unaffected: `shuffle_toggle_target` reads the live atom.
///
/// The snooze is part of the key too (4.7.0, S9 / issue #677): it drives the
/// status line, the presence of the "Resume sync now" entry AND the fetch mode
/// of the Devices/Up Next submenus, so a snooze that starts, ends or ticks over
/// to the next minute must repaint rather than early-return. The tick is why
/// the key carries [`snooze_dedup_key`]'s minute bucket and not just the
/// deadline — a key of the deadline alone would freeze the countdown at the
/// minute it was set.
#[derive(Clone, PartialEq, Eq)]
struct TrayStateSnapshot {
    is_syncing: bool,
    is_window_visible: bool,
    /// `artist|title|is_playing` — is_playing is in the key so a same-track
    /// pause repaints the Play/Pause mark (issue #229).
    track_key: Option<String>,
    shuffle: bool,
    repeat: RepeatState,
    /// Throttle bucket of the devices cache when this key was built (issue
    /// #805): the number of full [`TRAY_SPOTIFY_FETCH_THROTTLE`] windows since
    /// it was filled, `0` while it is empty.
    ///
    /// The key carries the bucket and not the timestamp because the bucket is
    /// what the fetchers act on: a session where nothing else moves now
    /// repaints — and therefore re-fetches — once per window, while a rebuild
    /// inside the window still dedupes to a no-op. Without it the early return
    /// below skipped the fetches themselves, and the Devices/Up Next submenus
    /// kept whatever the last rebuild happened to render, indefinitely.
    devices_bucket: u64,
    /// Same, for the Up Next queue cache (issue #805).
    queue_bucket: u64,
    snooze_key: Option<String>,
}

/// True when `next` differs from the last committed snapshot, i.e. when the
/// tray menu must be rebuilt. Pure, so the dedup contract is unit-testable
/// without a Tauri runtime.
fn tray_state_changed(prev: Option<&TrayStateSnapshot>, next: &TrayStateSnapshot) -> bool {
    prev != Some(next)
}

/// Builds the dedup key from the same inputs the rebuild renders from: the
/// caller's sync flag and precomputed window visibility, the track's
/// artist/title/is_playing, the two playback-mode atoms the polling loop feeds
/// (`note_playback_modes`), the snooze the rebuild will render (4.7.0, S9) and
/// the throttle bucket of both caches (issue #805).
/// Single construction site so the key can never be built from a subset of what
/// the menu shows (issue #691).
fn tray_snapshot_for(
    is_syncing: bool,
    is_window_visible: bool,
    current_track: Option<&crate::spotify::TrackInfo>,
    snooze_key: Option<String>,
    /// The two cache throttle buckets (issue #805). Read by the caller rather
    /// than here so the key stays a pure function of its inputs.
    devices_bucket: u64,
    queue_bucket: u64,
) -> TrayStateSnapshot {
    TrayStateSnapshot {
        is_syncing,
        is_window_visible,
        track_key: current_track.map(|t| format!("{}|{}|{}", t.artist, t.title, t.is_playing)),
        shuffle: LAST_SHUFFLE_STATE.load(Ordering::Acquire),
        repeat: last_repeat_state(),
        snooze_key,
        devices_bucket,
        queue_bucket,
    }
}

/// The throttle bucket a cache slot is in (issue #805): how many full throttle
/// windows have elapsed since it was filled, `0` while it is empty.
///
/// Pure in its timestamp, so "an unchanged rebuild inside the window is still
/// a no-op, one window later it repaints" is asserted without sleeping.
fn throttle_bucket(fetched_at: Option<Instant>, throttle: Duration) -> u64 {
    match fetched_at {
        None => 0,
        Some(at) => at.elapsed().as_secs() / throttle.as_secs().max(1),
    }
}

/// The throttle buckets of both caches (issue #805), read under short locks —
/// no HTTP, and no lock held past the read.
fn cache_buckets() -> (u64, u64) {
    let devices_at = DEVICES_CACHE.lock().as_ref().map(|(at, _)| *at);
    let queue_at = QUEUE_CACHE.lock().as_ref().map(|(at, _)| *at);
    (
        throttle_bucket(devices_at, TRAY_SPOTIFY_FETCH_THROTTLE),
        throttle_bucket(queue_at, TRAY_SPOTIFY_FETCH_THROTTLE),
    )
}

/// The active snooze as one rebuild renders it (4.7.0, S9 / issue #677).
#[derive(Debug, Clone, Copy)]
struct TraySnooze {
    status: crate::config::SnoozeStatus,
    /// Whole minutes the countdown shows (rounded up, never below 1).
    minutes_left: i64,
}

/// The snooze half of the dedup key: the deadline plus the minute bucket the
/// countdown is on, so the rebuilt status line cannot be deduped away while the
/// sleep runs (4.7.0, S9 / issue #677).
fn snooze_dedup_key(status: &crate::config::SnoozeStatus) -> String {
    format!(
        "{}|{}",
        crate::config::snooze_store_form(status.deadline),
        crate::config::snooze_minutes_left(status.remaining_seconds)
    )
}

/// Resolves the snooze a rebuild must render from the mounted config
/// (4.7.0, S9 / issue #677). Absent config, absent field and an already-passed
/// deadline all mean "not snoozed", which is what makes the tray agree with the
/// poller without a second source of truth.
fn resolve_snooze(config: Option<&crate::config::AppConfig>) -> Option<TraySnooze> {
    let status = crate::config::snooze_status(config?, chrono::Utc::now())?;
    Some(TraySnooze {
        minutes_left: crate::config::snooze_minutes_left(status.remaining_seconds),
        status,
    })
}

static LAST_TRAY_STATE: std::sync::OnceLock<parking_lot::Mutex<Option<TrayStateSnapshot>>> =
    std::sync::OnceLock::new();

fn last_tray_state() -> &'static parking_lot::Mutex<Option<TrayStateSnapshot>> {
    LAST_TRAY_STATE.get_or_init(|| parking_lot::Mutex::new(None))
}

/// Module-level mutex that serialises the two writers to the tray
/// (polling thread and frontend command). Issue #71.
static TRAY_WRITE_LOCK: std::sync::OnceLock<parking_lot::Mutex<()>> = std::sync::OnceLock::new();

fn tray_write_lock() -> &'static parking_lot::Mutex<()> {
    TRAY_WRITE_LOCK.get_or_init(|| parking_lot::Mutex::new(()))
}

/// Last known main-window visibility, which is what the dedup key carries
/// (issue #886).
///
/// The rebuild used to ask the window directly, and `is_visible()` blocks the
/// calling thread on an event-loop reply — a hop paid by every poll, including
/// the ones the dedup key discards a few lines later. [`note_window_visibility`]
/// keeps the mirror honest, and the rebuild re-reads the real window whenever it
/// paints anyway (see the self-heal in `rebuild_tray_menu`).
static WINDOW_VISIBLE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Records the main window's visibility (issue #886). Every path that shows or
/// hides the window should report it — the tray's own Show/Hide and Open
/// Settings arms do, and the window commands / close-to-tray paths are expected
/// to; the poll loop then never has to ask the event loop for it.
pub fn note_window_visibility(visible: bool) {
    WINDOW_VISIBLE.store(visible, Ordering::Release);
}

/// The visibility the dedup key is built from (issue #886) — the mirror, so the
/// discarded path performs no event-loop hop.
fn window_visible() -> bool {
    WINDOW_VISIBLE.load(Ordering::Acquire)
}

/// The real main-window visibility, queried once per paint (issue #886). Only
/// the paths that are already off the hot path may call this: the startup paint
/// (on the main thread) and the rebuild that passed the dedup guard.
fn live_window_visible(app: &AppHandle) -> bool {
    app.get_webview_window("main")
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false)
}

/// The snooze a rebuild must render, read out of the mounted config under a
/// scoped guard (issue #886).
///
/// Only `snooze_until` (plus the clock) is needed, so that one field is copied
/// out instead of cloning the whole `AppConfig` — which allocates its status
/// rules on every poll, including the ones the dedup key discards. The guard
/// lives only for this call, so none survives into the blocking Spotify HTTP
/// below.
fn snooze_from_app_state(state: &crate::AppState) -> Option<TraySnooze> {
    let config = state.config.get();
    resolve_snooze(config.as_ref())
}

/// Which entry point a rebuild came through (issue #882).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrayPaint {
    /// An ordinary rebuild: the dedup key decides whether the menu is rebuilt,
    /// and a successful rebuild commits the snapshot.
    Deduped,
    /// The startup paint. Built on the main thread before the event loop runs:
    /// it renders the throttled caches without touching the network, and it
    /// does not commit the snapshot (see `update_tray_menu_startup`).
    Startup,
}

/// The fetch mode a rebuild runs under (issues #677, #882).
///
/// Pure, so "the startup paint performs no request" and "a snooze performs no
/// request" are asserted directly rather than through the rebuild's source.
fn paint_fetch_mode(paint: TrayPaint, snoozed: bool, action_refresh_due: bool) -> TrayFetch {
    match paint {
        // Issue #882: no network before the event loop exists.
        TrayPaint::Startup => TrayFetch::CacheOnly,
        TrayPaint::Deduped => {
            let base = tray_fetch_mode(snoozed);
            // A snooze outranks an action: "no Spotify request while snoozed" is
            // that feature's acceptance criterion (issue #677).
            if base == TrayFetch::CacheOnly {
                TrayFetch::CacheOnly
            } else if action_refresh_due {
                // Issue #883: the action asks for fresh Devices/Up Next lists.
                TrayFetch::RefreshNow
            } else {
                base
            }
        }
    }
}

/// Throttle window for the tray's Spotify devices/queue fetches
/// (issue #3.0-P3). The polling loop calls `update_tray_menu` on every
/// successful poll; without a cap the Devices/Up Next submenus would
/// hammer the Spotify API on every iteration. Fetched data is cached
/// here and re-used until the window lapses.
const TRAY_SPOTIFY_FETCH_THROTTLE: Duration = Duration::from_secs(60);

/// The shuffle state a click switches to: the inverse of the last observed
/// one. Pure so the toggle contract is unit-testable.
fn shuffle_toggle_target(current: bool) -> bool {
    !current
}

/// Cache slot for the throttled devices fetch: `(fetched_at, devices)`.
/// Fast path for the device submenu's click dispatch, which resolves the
/// stable `{ID_DEVICES}|{device id}` against this list and falls back to a
/// live re-fetch when stale (issue #388).
type DeviceCacheSlot = Option<(Instant, Vec<crate::spotify::DeviceInfo>)>;

static DEVICES_CACHE: std::sync::LazyLock<parking_lot::Mutex<DeviceCacheSlot>> =
    std::sync::LazyLock::new(|| parking_lot::Mutex::new(None));

static QUEUE_CACHE: std::sync::LazyLock<
    parking_lot::Mutex<Option<(Instant, crate::spotify::QueueInfo)>>,
> = std::sync::LazyLock::new(|| parking_lot::Mutex::new(None));

/// Last known Spotify playing state, driving the Play/Pause check mark and
/// the tray status line (and the toggle's fallback dispatch). Re-seeded
/// from the polling loop's stored track on a genuine track change, from the
/// poller's `playback-state-changed` event when the playing state changes
/// for the same track (issue #689), and by the tray's own successful
/// play/pause/transfer actions. Issue #3.0-P3.
static LAST_PLAYING_STATE: std::sync::LazyLock<std::sync::atomic::AtomicBool> =
    std::sync::LazyLock::new(|| std::sync::atomic::AtomicBool::new(false));

/// Records the playing state the Play/Pause mark and the status line render.
/// Every path that learns the truth writes it here, so the mark never infers
/// playback from a candidate that may already be stale. Issue #3.0-P3.
fn note_playing_state(is_playing: bool) {
    LAST_PLAYING_STATE.store(is_playing, Ordering::Release);
}

/// Payload of the polling loop's `playback-state-changed` event (issue
/// #689). Only `is_playing` is consumed: the event's `track_key` is part of
/// the shared contract, but the tray's dedup key already carries the track
/// identity.
#[derive(serde::Deserialize)]
struct PlaybackStateChanged {
    is_playing: bool,
}

/// Consumes a `playback-state-changed` payload (issue #689, tray half): the
/// poll body's playing state is authoritative for a same-track change, and
/// the poller has already re-stored it, so recording it here keeps the
/// Play/Pause mark and the status line truthful without waiting for the
/// next track. An unparsable payload keeps the last known state rather than
/// inventing one.
fn consume_playback_state_changed(payload: &str) {
    match serde_json::from_str::<PlaybackStateChanged>(payload) {
        Ok(state) => note_playing_state(state.is_playing),
        Err(e) => log::warn!(
            "[TRAY] playback-state-changed: unparsable payload, keeping the last playing state: {}",
            e
        ),
    }
}

/// Last known shuffle state, driving the Shuffle item's check mark
/// (issue #582). Written by the polling loop from the poll body
/// (`note_playback_modes`) and optimistically by the tray's own successful
/// toggle, so a same-track toggle does not wait for the next poll. It is a
/// module-level atomic rather than a field on the app's frozen `TrackInfo`
/// because that type is the ts-rs-exported IPC shape shared with the
/// Dashboard and built by exhaustive literals outside this module.
static LAST_SHUFFLE_STATE: std::sync::LazyLock<std::sync::atomic::AtomicBool> =
    std::sync::LazyLock::new(|| std::sync::atomic::AtomicBool::new(false));

/// Last known repeat mode, encoded as [`RepeatState`]'s `u8` discriminant.
/// Same lifecycle as `LAST_SHUFFLE_STATE`.
static LAST_REPEAT_STATE: std::sync::LazyLock<std::sync::atomic::AtomicU8> =
    std::sync::LazyLock::new(|| std::sync::atomic::AtomicU8::new(RepeatState::Off as u8));

/// Records the playback modes a poll body reported. Called by the polling
/// loop for every observed item (playing or paused) — the poll response is
/// the source of truth for both toggles, so no extra Spotify request is
/// needed to render them. See issue #582.
pub(crate) fn note_playback_modes(shuffle: bool, repeat: RepeatState) {
    LAST_SHUFFLE_STATE.store(shuffle, Ordering::Release);
    LAST_REPEAT_STATE.store(repeat as u8, Ordering::Release);
}

/// The tray's view of the current repeat mode. An out-of-range byte (only
/// possible if the encoder above is changed without this decoder) degrades
/// to `Off` rather than panicking in a menu build.
fn last_repeat_state() -> RepeatState {
    match LAST_REPEAT_STATE.load(Ordering::Acquire) {
        1 => RepeatState::Context,
        2 => RepeatState::Track,
        _ => RepeatState::Off,
    }
}

/// Menu label for the Repeat item: the mode is spelled out because the
/// documented state space has three values and a check mark only carries
/// on/off. Pure, and localized from the table it is handed (issue #674), so
/// the label contract is unit-testable in every locale.
fn repeat_menu_label(strings: &Strings, state: RepeatState) -> &'static str {
    match state {
        RepeatState::Off => strings.repeat_off,
        RepeatState::Context => strings.repeat_context,
        RepeatState::Track => strings.repeat_track,
    }
}

/// Coalescing guard for the delayed one-shot refresh kicked after a
/// successful tray player action: rapid next/previous clicks must not pile
/// up unbounded 2 s-sleep threads each firing blocking Spotify+Teams HTTP.
/// First claimant spawns; losers skip (their track change is covered by the
/// in-flight refresh's unconditional GET plus the polling loop).
static DELAYED_REFRESH_IN_FLIGHT: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Shortest gap between the post-action re-fetches of the Devices/Up Next lists
/// (issue #883). A player action wants those submenus to mirror what just
/// happened, but a burst of clicks must share one devices+queue pair: five
/// seconds is long enough to coalesce a burst, short enough that the menu still
/// matches the click the user just made.
const TRAY_POST_ACTION_FETCH_MIN: Duration = Duration::from_secs(5);

/// The most recent tray player action that wants the Devices/Up Next lists
/// refreshed (issue #883). `force_tray_refresh` records the action here instead
/// of emptying both caches, which bypassed the fetch throttle for every click.
static LAST_TRAY_ACTION: std::sync::LazyLock<parking_lot::Mutex<Option<Instant>>> =
    std::sync::LazyLock::new(|| parking_lot::Mutex::new(None));

/// When the most recent post-action re-fetch was issued (issue #883). Recorded
/// before the requests run, so a click landing while they are in flight reuses
/// them instead of paying for a pair of its own.
static LAST_ACTION_FETCH: std::sync::LazyLock<parking_lot::Mutex<Option<Instant>>> =
    std::sync::LazyLock::new(|| parking_lot::Mutex::new(None));

/// Whether a rebuild must re-fetch the Devices/Up Next lists on behalf of a
/// player action (issue #883). Pure in its instants, so the coalescing rule —
/// ten rapid clicks, one pair of requests — is unit-testable without waiting.
fn action_fetch_due(
    last_action: Option<Instant>,
    last_action_fetch: Option<Instant>,
    now: Instant,
    min_interval: Duration,
) -> bool {
    match (last_action, last_action_fetch) {
        // No action has asked for anything.
        (None, _) => false,
        // An action nothing has fetched for yet is due immediately.
        (Some(_), None) => true,
        // Only an action newer than the last post-action fetch — and far enough
        // after it — is worth two more requests; a burst inside `min_interval`
        // shares the pair already issued.
        (Some(action), Some(fetched)) => {
            action > fetched && now.duration_since(fetched) >= min_interval
        }
    }
}

/// Returns cached devices when the throttle window hasn't elapsed, else
/// fetches fresh ones. On a fetch failure the stale cache is returned so
/// the submenu doesn't flicker to "(no devices)" on a transient error.
///
/// The cache mutex is held only to clone the snapshot and to store the
/// fresh result — the HTTP fetch runs outside any lock so a cold/stale
/// cache never blocks tray interactions. If two threads race on a stale
/// cache both will fetch; the last writer wins. This benign double-fetch
/// wastes one request but cannot corrupt state. See issue #217.
/// `min_interval` is this fetch's throttle (issue #883): the full window for an
/// ordinary rebuild, `Duration::ZERO` for the one a player action asks for.
fn cached_devices(access_token: &str, min_interval: Duration) -> Vec<crate::spotify::DeviceInfo> {
    // Snapshot under short lock, then drop before deciding staleness.
    let snapshot = {
        let cache = DEVICES_CACHE.lock();
        cache.clone()
    };
    let needs_fetch = match &snapshot {
        Some((fetched_at, _)) => fetched_at.elapsed() >= min_interval,
        None => true,
    };
    if !needs_fetch {
        return snapshot.unwrap().1;
    }
    // Throttled fetch OUTSIDE any lock — never hold DEVICES_CACHE across HTTP.
    match crate::spotify::get_devices(access_token) {
        Ok(devices) => {
            // Re-acquire only to store the fresh result.
            *DEVICES_CACHE.lock() = Some((Instant::now(), devices.clone()));
            devices
        }
        Err(e) => {
            log::warn!("[TRAY] cached_devices: failed to fetch devices: {}", e);
            snapshot.map(|(_, devices)| devices).unwrap_or_default()
        }
    }
}

/// Returns cached queue when the throttle window hasn't elapsed, else
/// fetches fresh. Falls back to the stale cache on failure.
///
/// Same lock discipline as `cached_devices`: snapshot, drop, fetch outside
/// lock, re-acquire to store. Benign double-fetch on a race. See issue #217.
fn cached_queue(access_token: &str, min_interval: Duration) -> Option<crate::spotify::QueueInfo> {
    // Snapshot under short lock, then drop before deciding staleness.
    let snapshot = {
        let cache = QUEUE_CACHE.lock();
        cache.clone()
    };
    let needs_fetch = match &snapshot {
        Some((fetched_at, _)) => fetched_at.elapsed() >= min_interval,
        None => true,
    };
    if !needs_fetch {
        return snapshot.map(|(_, queue)| queue);
    }
    // Throttled fetch OUTSIDE any lock — never hold QUEUE_CACHE across HTTP.
    match crate::spotify::get_queue(access_token) {
        Ok(queue) => {
            // Re-acquire only to store.
            *QUEUE_CACHE.lock() = Some((Instant::now(), queue.clone()));
            Some(queue)
        }
        Err(e) => {
            log::warn!("[TRAY] cached_queue: failed to fetch queue: {}", e);
            snapshot.map(|(_, queue)| queue)
        }
    }
}

/// Where a rebuild may take its Devices / Up Next lists from (4.7.0, S9 /
/// issue #677).
///
/// A snooze means "the user asked for no activity", and that covers the app's
/// own background requests: the acceptance for the feature is that a snoozed
/// session produces no Spotify GET at all. `update_tray_menu` is called from
/// the polling driver and from the snooze click arm, so the fetch mode has to
/// be a property of the rebuild rather than of its caller — otherwise a
/// countdown repaint would silently re-fetch devices every 60 s (the throttle
/// window) and break that guarantee.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrayFetch {
    /// Normal rebuild: reuse the throttled cache, fetching when it lapsed.
    Refresh,
    /// Issue #883: a player action just changed the playback/device state these
    /// lists mirror, so this one rebuild bypasses the throttle. A burst of
    /// clicks still shares one pair of requests — see [`action_fetch_due`].
    RefreshNow,
    /// A snooze is active: render the last cached lists (or nothing) and issue
    /// no request. The submenus go stale by design until the snooze ends.
    CacheOnly,
}

/// The fetch mode a rebuild must use (4.7.0, S9 / issue #677).
///
/// Pure, so the "a snooze means no Spotify request" decision is asserted
/// directly instead of only through the rebuild's source text.
fn tray_fetch_mode(snoozed: bool) -> TrayFetch {
    if snoozed {
        TrayFetch::CacheOnly
    } else {
        TrayFetch::Refresh
    }
}

/// The Devices list for a rebuild, honouring [`TrayFetch`]. A missing access
/// token is an empty submenu either way — there is nothing to fetch with.
fn devices_for_menu(
    access_token: Option<&str>,
    fetch: TrayFetch,
) -> Vec<crate::spotify::DeviceInfo> {
    match (access_token, fetch) {
        (Some(token), TrayFetch::Refresh) => cached_devices(token, TRAY_SPOTIFY_FETCH_THROTTLE),
        (Some(token), TrayFetch::RefreshNow) => cached_devices(token, Duration::ZERO),
        (_, TrayFetch::CacheOnly) => DEVICES_CACHE
            .lock()
            .clone()
            .map(|(_, devices)| devices)
            .unwrap_or_default(),
        (None, _) => Vec::new(),
    }
}

/// The Up Next snapshot for a rebuild, honouring [`TrayFetch`]. Same contract
/// as [`devices_for_menu`].
fn queue_for_menu(
    access_token: Option<&str>,
    fetch: TrayFetch,
) -> Option<crate::spotify::QueueInfo> {
    match (access_token, fetch) {
        (Some(token), TrayFetch::Refresh) => cached_queue(token, TRAY_SPOTIFY_FETCH_THROTTLE),
        (Some(token), TrayFetch::RefreshNow) => cached_queue(token, Duration::ZERO),
        (_, TrayFetch::CacheOnly) => QUEUE_CACHE.lock().clone().map(|(_, queue)| queue),
        (None, _) => None,
    }
}

/// What the `{ID_DEVICES}|{…}` suffix resolved to. Device menu ids carry the
/// stable Spotify device id (issue #388); the `LegacyIndex` variant accepts
/// ids minted by an older menu build still on screen when the app updated.
#[derive(Debug, PartialEq, Eq)]
enum DeviceMenuSelection {
    DeviceId(String),
    LegacyIndex(usize),
    Invalid,
}

/// Parses the suffix of a device menu-item id. A numeric suffix from an old
/// menu build is kept as `LegacyIndex` for back-compat; anything else is a
/// stable Spotify device id (`DeviceId`), including the empty string and the
/// `none` placeholder, which both resolve to `Invalid` downstream.
fn parse_device_menu_id(suffix: &str) -> DeviceMenuSelection {
    if suffix == "none" {
        return DeviceMenuSelection::Invalid;
    }
    // Spotify device ids are opaque base62 strings; a pure-ASCII-digit
    // suffix can only have come from the old `{index}` scheme.
    if !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()) {
        if let Ok(i) = suffix.parse::<usize>() {
            return DeviceMenuSelection::LegacyIndex(i);
        }
    }
    if suffix.is_empty() {
        return DeviceMenuSelection::Invalid;
    }
    DeviceMenuSelection::DeviceId(suffix.to_string())
}

/// Redacted label for the unknown-device warn log: ids are bearer-adjacent,
/// so log only the length, never the id itself.
fn selected_for_log(selected: &DeviceMenuSelection) -> String {
    match selected {
        DeviceMenuSelection::DeviceId(id) => format!("<id len={}>", id.len()),
        DeviceMenuSelection::LegacyIndex(i) => format!("<legacy index={}>", i),
        DeviceMenuSelection::Invalid => "<invalid>".to_string(),
    }
}

/// Resolves a parsed device-menu selection to a live Spotify device id.
/// Cache first (no IO), then a live `get_devices` re-fetch when the cached
/// list went stale (issue #388: the old `devices.get(i)` raced the
/// 60 s-throttled cache against the live device list). MUST run off the
/// menu-event thread — the fallback performs blocking HTTP (issue #386) —
/// and resolves its token through the shared refresh-aware policy, so an
/// expired access token does not strand a transfer on "unknown device"
/// (issue #586).
fn resolve_device_id(app: &AppHandle, selected: &DeviceMenuSelection) -> Option<String> {
    match selected {
        DeviceMenuSelection::DeviceId(id) => {
            // Fast path: still in the cached list and transferable.
            let cached = DEVICES_CACHE.lock().as_ref().and_then(|(_, devices)| {
                devices
                    .iter()
                    .find(|d| d.id.as_deref() == Some(id.as_str()))
                    .and_then(|d| d.id.clone())
            });
            if cached.is_some() {
                return cached;
            }
            // Slow path: live re-fetch; the device may have appeared after
            // the submenu was built, or the cache may be stale.
            let state = app.state::<std::sync::Arc<crate::AppState>>();
            match crate::commands::playback::player_with_refresh_typed(
                state.inner(),
                app,
                "transfer device list",
                crate::spotify::get_devices,
            ) {
                Ok(devices) => {
                    *DEVICES_CACHE.lock() = Some((Instant::now(), devices.clone()));
                    devices
                        .into_iter()
                        .find(|d| d.id.as_deref() == Some(id.as_str()))
                        .and_then(|d| d.id)
                }
                Err(e) => {
                    log::warn!("[TRAY] transfer: live device re-fetch failed: {}", e);
                    None
                }
            }
        }
        DeviceMenuSelection::LegacyIndex(i) => DEVICES_CACHE
            .lock()
            .as_ref()
            .and_then(|(_, devices)| devices.get(*i).cloned())
            .and_then(|device| device.id),
        DeviceMenuSelection::Invalid => None,
    }
}

/// Builds the Devices submenu from an already-fetched slice. No HTTP is
/// performed here — the caller must have fetched outside any tray lock.
/// See issue #217.
fn build_devices_submenu_from_devices(
    app: &AppHandle,
    devices: &[crate::spotify::DeviceInfo],
) -> Result<Submenu<tauri::Wry>, String> {
    let s = i18n::current();
    let submenu = Submenu::with_id(app, ID_DEVICES, s.devices, true).map_err(|e| e.to_string())?;
    if devices.is_empty() {
        let empty = MenuItemBuilder::with_id(format!("{}|none", ID_DEVICES), s.no_devices)
            .enabled(false)
            .build(app)
            .map_err(|e| e.to_string())?;
        submenu.append(&empty).map_err(|e| e.to_string())?;
    } else {
        for device in devices.iter() {
            let label = if device.is_active {
                format!("✓ {}", device.name)
            } else {
                device.name.clone()
            };
            // The active device (and id-less devices) can't be transferred to.
            let enabled = !device.is_active && device.id.is_some();
            // Issue #388: carry the stable Spotify device id (not the list
            // index) so a click resolves even when the cached list raced a
            // live device change. Id-less devices reuse the `none` id: they
            // are disabled and parse back to `Invalid`, which the click
            // handler rejects with a warn.
            let suffix = device.id.as_deref().unwrap_or("none");
            let item = MenuItemBuilder::with_id(format!("{}|{}", ID_DEVICES, suffix), label)
                .enabled(enabled)
                .build(app)
                .map_err(|e| e.to_string())?;
            submenu.append(&item).map_err(|e| e.to_string())?;
        }
    }
    Ok(submenu)
}

/// Builds the Up Next submenu from an already-fetched queue snapshot.
/// No HTTP here — fetch must have happened outside the tray lock. See issue #217.
fn build_queue_submenu_from_queue(
    app: &AppHandle,
    queue: Option<&crate::spotify::QueueInfo>,
) -> Result<Submenu<tauri::Wry>, String> {
    let s = i18n::current();
    let submenu = Submenu::with_id(app, ID_QUEUE, s.up_next, true).map_err(|e| e.to_string())?;
    let up_next: Vec<crate::spotify::TrackInfo> = queue
        .map(|q| q.up_next.iter().take(3).cloned().collect())
        .unwrap_or_default();
    if up_next.is_empty() {
        let empty = MenuItemBuilder::with_id(format!("{}|none", ID_QUEUE), s.queue_empty)
            .enabled(false)
            .build(app)
            .map_err(|e| e.to_string())?;
        submenu.append(&empty).map_err(|e| e.to_string())?;
    } else {
        for (index, track) in up_next.iter().enumerate() {
            let item = MenuItemBuilder::with_id(
                format!("{}|none|{}", ID_QUEUE, index),
                format!("{} - {}", track.artist, track.title),
            )
            .enabled(false)
            .build(app)
            .map_err(|e| e.to_string())?;
            submenu.append(&item).map_err(|e| e.to_string())?;
        }
    }
    Ok(submenu)
}

/// Builds the snooze submenu (4.7.0, S9 / issue #677): the three presets, plus
/// a "Resume sync now" entry while a snooze is active.
///
/// The three presets stay enabled during a snooze on purpose — picking another
/// one re-snoozes from now, which is how a user extends a 30-minute snooze to
/// "until tomorrow" without first resuming. Every label comes from the
/// installed i18n table, so the submenu localizes with the rest of the tray.
///
/// No IO: the caller has already resolved the snooze (and, while one is active,
/// is required to be in [`TrayFetch::CacheOnly`] mode — this builder performs
/// no fetch of its own).
fn build_snooze_submenu(
    app: &AppHandle,
    s: &Strings,
    snooze: Option<&TraySnooze>,
) -> Result<Submenu<tauri::Wry>, String> {
    let thirty = MenuItemBuilder::with_id(ID_SNOOZE_30, s.snooze_30_minutes)
        .build(app)
        .map_err(|e| e.to_string())?;
    let hour = MenuItemBuilder::with_id(ID_SNOOZE_1H, s.snooze_1_hour)
        .build(app)
        .map_err(|e| e.to_string())?;
    let tomorrow = MenuItemBuilder::with_id(ID_SNOOZE_TOMORROW, s.snooze_until_tomorrow)
        .build(app)
        .map_err(|e| e.to_string())?;
    let submenu = SubmenuBuilder::new(app, s.snooze_pause_menu)
        .items(&[&thirty, &hour, &tomorrow])
        .build()
        .map_err(|e| e.to_string())?;

    if snooze.is_some() {
        let separator = PredefinedMenuItem::separator(app).map_err(|e| e.to_string())?;
        let resume = MenuItemBuilder::with_id(ID_SNOOZE_RESUME, s.snooze_resume_now)
            .build(app)
            .map_err(|e| e.to_string())?;
        submenu.append(&separator).map_err(|e| e.to_string())?;
        submenu.append(&resume).map_err(|e| e.to_string())?;
    }
    Ok(submenu)
}

/// What a snooze menu id asked for (4.7.0, S9 / issue #677). Parsed rather
/// than switched on the raw id so the click arm stays a dispatch table and the
/// mapping is unit-testable without a menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SnoozeMenuSelection {
    Preset(crate::config::SnoozePreset),
    Resume,
}

/// Parses a snooze menu-item id. `None` for anything that is not one of the
/// four known ids — a stale menu from an older build must not snooze by
/// accident.
fn parse_snooze_menu_id(id: &str) -> Option<SnoozeMenuSelection> {
    match id.strip_prefix(SNOOZE_ITEM_PREFIX)? {
        SNOOZE_30_SUFFIX => Some(SnoozeMenuSelection::Preset(
            crate::config::SnoozePreset::ThirtyMinutes,
        )),
        SNOOZE_1H_SUFFIX => Some(SnoozeMenuSelection::Preset(
            crate::config::SnoozePreset::OneHour,
        )),
        SNOOZE_TOMORROW_SUFFIX => Some(SnoozeMenuSelection::Preset(
            crate::config::SnoozePreset::UntilTomorrow,
        )),
        SNOOZE_RESUME_SUFFIX => Some(SnoozeMenuSelection::Resume),
        _ => None,
    }
}

/// Persists a snooze deadline (or clears it) into `AppConfig::snooze_until`
/// (4.7.0, S9 / issue #677).
///
/// The write half only — callers log the action, so the copy matches what
/// happened (a preset, an explicit resume, or the startup cleanup of a deadline
/// that expired while the app was closed). Follows
/// `commands::config::update_config` exactly: hold the config write guard across
/// the atomic write, persist the CLAMPED copy with the binary-owned
/// `schema_version` stamped, and store THAT value — otherwise the in-memory
/// config (what the poller reads) and config.json disagree until the next launch
/// (issues #297 / #536).
///
/// `None` clears the field, which is both "Resume sync now" and the state the
/// frontend writes when its chip's Resume button is used.
fn store_snooze(
    app: &AppHandle,
    until: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<(), String> {
    let state = app.state::<std::sync::Arc<crate::AppState>>();
    let mut guard = state.config.get_mut();
    let base = match guard.as_ref() {
        Some(current) => current.clone(),
        None => crate::config::load_config()?,
    };
    let mut next = base;
    next.snooze_until = until.map(crate::config::snooze_store_form);

    let mut persisted = crate::config::clamped_config(&next);
    crate::config::stamp_schema_version(&mut persisted);
    crate::config::save_config(&persisted)?;
    *guard = Some(persisted);

    Ok(())
}

/// Persists a user-chosen snooze (or its removal) and logs the action
/// (4.7.0, S9 / issue #677). The tray's three click paths all land here.
fn write_snooze(
    app: &AppHandle,
    until: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<(), String> {
    store_snooze(app, until)?;
    match until {
        Some(deadline) => log::info!(
            "[TRAY] snooze: polling paused until {} (local {}, {} min)",
            crate::config::snooze_store_form(deadline),
            deadline.with_timezone(&chrono::Local).format("%H:%M"),
            crate::config::snooze_minutes_left((deadline - chrono::Utc::now()).num_seconds()),
        ),
        None => log::info!("[TRAY] snooze: resumed by the user"),
    }
    Ok(())
}

/// Clears a stored snooze deadline that has already passed, once, as the app
/// comes up (4.7.0, S9 / issue #677).
///
/// `config::load_config` is a READER on paths that hold no config-write guard,
/// so it reports an expired deadline without touching the document (a reader
/// that fixed the field in memory would hide the expiry from every writer that
/// can correct `config.json` — see that fn). This is the writer: a guarded
/// store through [`store_snooze`], run at startup, so a snooze that expired
/// while the app was closed does not sit in `config.json` logging a clamp line
/// on every launch. The poller's own expiry arm covers a deadline that lapses
/// while the app is running; either writer alone is sufficient.
fn clear_expired_snooze_at_startup(app: &AppHandle) {
    let state = app.state::<std::sync::Arc<crate::AppState>>();
    let expired = state
        .config
        .get()
        .as_ref()
        .is_some_and(|cfg| crate::config::snooze_expired_deadline(cfg, chrono::Utc::now()));
    if !expired {
        return;
    }
    match store_snooze(app, None) {
        Ok(()) => {
            log::info!("[TRAY] snooze: cleared the expired deadline left by the previous session")
        }
        Err(e) => log::warn!(
            "[TRAY] snooze: could not clear the expired deadline ({}); the poller will retry",
            e
        ),
    }
}

/// Runs a Spotify player action from a tray click using the stored access
/// token. On success the tray menu is force-refreshed and the action's
/// deterministic outcome is recorded for the items that mirror it:
/// `resulting_playing` for the Play/Pause item (`None` for actions that do
/// not change the playing state), `resulting_modes` for the shuffle/repeat
/// toggles (issue #582). Both are applied BEFORE `force_tray_refresh`, so the
/// rebuild renders the state the API just accepted; recording them in the
/// caller after this function returned would repaint the replaced state and
/// the tray's dedup key would not change again until the next track ends. On
/// failure the error is logged and emitted on the `playback-error` event so
/// the frontend can surface it; a `NoActiveDevice` error (404) is logged
/// distinctly — the Devices submenu offers transfer in that case. Returns
/// `true` when the action succeeded.
/// See issues #3.0-P3 and #582.
fn run_player_action(
    app: &AppHandle,
    label: &str,
    resulting_playing: Option<bool>,
    resulting_modes: Option<(bool, RepeatState)>,
    action: impl Fn(&str) -> Result<(), crate::spotify::SpotifyApiError>,
) -> bool {
    // Issue #586: route the tray's player actions through the shared
    // refresh-aware policy instead of a raw `state.tokens.spotify()`
    // snapshot — a stale access token is refreshed proactively, and an
    // `ExpiredToken` response gets one refresh + retry, exactly as the
    // command layer does.
    let state = app.state::<std::sync::Arc<crate::AppState>>();
    match crate::commands::playback::player_with_refresh_typed(state.inner(), app, label, action) {
        Ok(()) => {
            log::info!("[TRAY] {}: success", label);
            if let Some(playing) = resulting_playing {
                note_playing_state(playing);
            }
            if let Some((shuffle, repeat)) = resulting_modes {
                note_playback_modes(shuffle, repeat);
            }
            force_tray_refresh(app);
            // Immediate Teams catch-up after a successful player action:
            // wait 2 s for Spotify's currently-playing to catch up after
            // a skip, then run a one-shot poll (no-op when sync is off).
            // Coalesced: rapid clicks skip while a delayed refresh is
            // already pending; its unconditional GET covers their tracks.
            if DELAYED_REFRESH_IN_FLIGHT
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                let app_clone = app.clone();
                let label_owned = label.to_string();
                std::thread::spawn(move || {
                    // RAII: a panic in run_oneshot must not wedge future
                    // refreshes (a manual clear on each return path would).
                    struct ResetOnDrop;
                    impl Drop for ResetOnDrop {
                        fn drop(&mut self) {
                            DELAYED_REFRESH_IN_FLIGHT.store(false, Ordering::Release);
                        }
                    }
                    let _reset = ResetOnDrop;
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    let state = app_clone.state::<std::sync::Arc<crate::AppState>>();
                    if !state.polling.is_syncing(Ordering::Acquire) {
                        log::debug!("[TRAY] {}: delayed refresh skipped (sync off)", label_owned);
                        return;
                    }
                    crate::polling::run_oneshot(state.inner(), &app_clone);
                    let is_syncing = state.polling.is_syncing(Ordering::Acquire);
                    let current_track = state.polling.current_track().clone();
                    if let Err(e) = update_tray_menu(&app_clone, is_syncing, current_track) {
                        log::warn!(
                            "[TRAY] {}: delayed refresh tray update failed: {}",
                            label_owned,
                            e
                        );
                    }
                });
            } else {
                log::debug!(
                    "[TRAY] {}: delayed refresh already pending, coalesced",
                    label
                );
            }
            true
        }
        Err(crate::spotify::SpotifyApiError::NoActiveDevice) => {
            log::warn!(
                "[TRAY] {}: no active device — pick one from the Devices menu",
                label
            );
            let _ = app.emit(
                "playback-error",
                "No active playback device - pick one from the tray Devices menu",
            );
            false
        }
        Err(e) => {
            log::error!("[TRAY] {}: failed: {}", label, e);
            let _ = app.emit("playback-error", e.to_string());
            false
        }
    }
}

/// Upper bound on the wait for the frontend's Pause/Resume toggle to land
/// (issue #588). The frontend owns the actual `start_syncing` /
/// `stop_syncing` call, so the flag settles only after that round-trip.
const TOGGLE_SETTLE_TIMEOUT: Duration = Duration::from_secs(3);

/// Poll cadence while waiting for the toggle to land.
const TOGGLE_SETTLE_POLL: Duration = Duration::from_millis(150);

/// Repaints the tray from authoritative backend state on the CURRENT
/// thread. Callers must already be off the menu/app-event thread — the
/// rebuild performs blocking Spotify HTTP whenever the fetch throttle has
/// lapsed. `context` only labels the failure log.
fn repaint_tray_from_state(app: &AppHandle, context: &str) {
    let Some(state) = app.try_state::<std::sync::Arc<crate::AppState>>() else {
        log::debug!(
            "[TRAY] {}: AppState not registered yet, skipping repaint",
            context
        );
        return;
    };
    let is_syncing = state.polling.is_syncing(Ordering::Acquire);
    let current_track = state.polling.current_track().clone();
    if let Err(e) = update_tray_menu(app, is_syncing, current_track) {
        log::warn!("[TRAY] {}: tray repaint failed: {}", context, e);
    }
}

/// Repaints the tray from backend state on a worker thread.
///
/// Issues #587/#592: every caller runs on a menu/app-event thread (tray
/// click arms, app-menu items, the single-instance raise), and
/// `update_tray_menu` fetches Spotify devices/queue with a 10 s timeout
/// whenever the 60 s throttle has lapsed — doing that inline wedges the
/// native menu (the freeze issue #386 removed from the player arms).
pub(crate) fn refresh_tray_from_state(app: &AppHandle) {
    let app_handle = app.clone();
    std::thread::spawn(move || repaint_tray_from_state(&app_handle, "refresh_tray_from_state"));
}

/// Repaints the tray after a locale change (issue #674).
///
/// A language switch changes only labels, so the dedup snapshot would
/// otherwise hide it — `force_tray_refresh` nudges that snapshot and rebuilds
/// from the freshly installed table. It runs on a worker thread for the same
/// reason as [`refresh_tray_from_state`]: the rebuild may perform blocking
/// Spotify HTTP, which must never run on a menu/app-event thread.
pub(crate) fn refresh_tray_for_locale(app: &AppHandle) {
    let app_handle = app.clone();
    std::thread::spawn(move || force_tray_refresh(&app_handle));
}

/// Waits — bounded by [`TOGGLE_SETTLE_TIMEOUT`] — for the frontend's
/// Pause/Resume toggle to move the running flag away from `before`.
/// Returns `true` when it moved, `false` when the window elapsed or state
/// was unavailable; the caller repaints either way.
fn await_sync_toggle(app: &AppHandle, before: bool) -> bool {
    let deadline = Instant::now() + TOGGLE_SETTLE_TIMEOUT;
    while Instant::now() < deadline {
        std::thread::sleep(TOGGLE_SETTLE_POLL);
        let Some(state) = app.try_state::<std::sync::Arc<crate::AppState>>() else {
            return false;
        };
        if state.polling.is_syncing(Ordering::Acquire) != before {
            return true;
        }
    }
    false
}

/// Forces the next `update_tray_menu` call to rebuild and asks it for fresh
/// Devices/Up Next lists: records the action (issue #883 — the caches stay, and
/// the rebuild re-fetches under `TRAY_POST_ACTION_FETCH_MIN`), nudges the dedup
/// snapshot so the rebuild can't early-return, then rebuilds immediately.
/// User-initiated tray actions call this so the menu reflects the new
/// playback/device state right away — the dedup key alone wouldn't change on
/// e.g. a pause or a transfer.
///
/// The snapshot is nudged (not cleared) with the *current* track key so the
/// re-seed logic in `update_tray_menu` (which only fires on a genuine track
/// change) doesn't clobber the toggle state set by the action. See
/// issue #3.0-P3.
fn force_tray_refresh(app: &AppHandle) {
    let state = app.state::<std::sync::Arc<crate::AppState>>();
    let is_syncing = state.polling.is_syncing(Ordering::Acquire);
    let current_track = state.polling.current_track().clone();
    // S9 (issue #677): the snooze is read here only for the dedup key below —
    // while one is active the rebuild renders the CACHED Devices/Up Next lists
    // instead of fetching (`TrayFetch::CacheOnly`) and an action cannot override
    // that. The nudge below still forces the repaint, which is all a snooze-time
    // repaint needs.
    let snooze = snooze_from_app_state(state.inner());
    // Issue #883: the caches stay. Emptying them was how this helper asked for
    // fresh Devices/Up Next lists, and it bypassed `TRAY_SPOTIFY_FETCH_THROTTLE`
    // on every player action; the action is recorded instead, and the rebuild
    // re-fetches under `TRAY_POST_ACTION_FETCH_MIN` — which coalesces a burst
    // into one pair of requests. The nudge below still forces the repaint.
    *LAST_TRAY_ACTION.lock() = Some(Instant::now());
    let snooze_key = snooze.as_ref().map(|sn| snooze_dedup_key(&sn.status));
    // Nudge the dedup snapshot (not clear it) with the *current* track key so
    // the rebuild below can't early-return while the re-seed logic stays
    // inert: a cleared snapshot would look like a genuine track change and
    // clobber the toggle state the action just recorded. Flipping the sync
    // bit is enough — the real snapshot is committed by that rebuild.
    let (devices_bucket, queue_bucket) = cache_buckets();
    let mut nudge = tray_snapshot_for(
        is_syncing,
        false,
        current_track.as_ref(),
        snooze_key,
        devices_bucket,
        queue_bucket,
    );
    nudge.is_syncing = !is_syncing;
    *last_tray_state().lock() = Some(nudge);
    let _ = update_tray_menu(app, is_syncing, current_track);
}

/// One-line sync/status summary for the tray's status item and tooltip
/// (issue #591). The Pause/Resume verb on its own left the sync state
/// unstated, and the presence-gated dock badge is macOS-only. `is_playing`
/// is `LAST_PLAYING_STATE` — the same source as the Play/Pause checkmark —
/// not the polling loop's copy, which goes stale on a same-track pause.
///
/// Localized from the table it is handed (issue #674); the artist/title and
/// the em-dash separators are locale-neutral, so only the status word and the
/// track-less line come from the table.
fn sync_status_line(
    strings: &Strings,
    is_syncing: bool,
    is_playing: bool,
    track: Option<&crate::spotify::TrackInfo>,
) -> String {
    if !is_syncing {
        return strings.status_not_syncing.to_string();
    }
    match track {
        None => strings.status_syncing_no_track.to_string(),
        Some(t) if is_playing => format!("{} — {} — {}", strings.status_syncing, t.artist, t.title),
        Some(t) => format!("{} — {} — {}", strings.status_paused, t.artist, t.title),
    }
}

/// The tray's status line while a snooze is active (4.7.0, S9 / issue #677):
/// the word, the remaining time and the local deadline.
///
/// This replaces [`sync_status_line`] rather than prefixing it: a snoozed
/// session is deliberately not syncing anything, so any words about the current
/// track would be a claim about work that is not happening. The countdown needs
/// no locale-specific plural handling — the unit is a table entry, and the
/// minute count is rounded so it never reads "0 min left" while the snooze is
/// still live (see `config::snooze_minutes_left`).
///
/// Localized from the table it is handed, like every other label here; the
/// `→` and `—` separators are locale-neutral, matching `sync_status_line`.
fn snooze_status_line(strings: &Strings, snooze: &TraySnooze) -> String {
    format!(
        "{} — {} {} (→ {})",
        strings.snooze_paused,
        snooze.minutes_left,
        strings.snooze_minutes_left,
        snooze
            .status
            .deadline
            .with_timezone(&chrono::Local)
            .format("%H:%M")
    )
}

/// Rebuilds the tray menu with current state.
/// Called by menu.rs when sync state or track changes.
pub fn update_tray_menu(
    app: &AppHandle,
    is_syncing: bool,
    current_track: Option<crate::spotify::TrackInfo>,
) -> Result<(), String> {
    rebuild_tray_menu(app, is_syncing, current_track.as_ref(), TrayPaint::Deduped)
}

/// The menu the tray shows before the event loop is running (issue #882).
///
/// Two differences from a normal rebuild, both consequences of running on the
/// main thread inside `setup()`: it never touches the network (the throttled
/// caches are rendered as they stand — empty on a cold start), and it does NOT
/// commit the dedup snapshot. This paint shows the caches only, so recording it
/// would make the first honest rebuild look like a no-op and the Devices/Up
/// Next submenus would stay empty for the rest of the session.
fn update_tray_menu_startup(
    app: &AppHandle,
    is_syncing: bool,
    current_track: Option<crate::spotify::TrackInfo>,
) -> Result<(), String> {
    rebuild_tray_menu(app, is_syncing, current_track.as_ref(), TrayPaint::Startup)
}

/// The rebuild both entry points above share, so the tray has one layout.
fn rebuild_tray_menu(
    app: &AppHandle,
    is_syncing: bool,
    current_track: Option<&crate::spotify::TrackInfo>,
    paint: TrayPaint,
) -> Result<(), String> {
    let tray = match get_tray() {
        Some(t) => t,
        None => {
            log::warn!("[TRAY] update_tray_menu: Tray not initialized");
            return Err("Tray not initialized".to_string());
        }
    };

    // Issue #71 + #229 + #691: dedup guard. The polling thread calls this on
    // every successful poll; the menu only needs rebuilding when is_syncing,
    // window visibility (drives the Show/Hide label), the track's
    // title/artist/is_playing, or one of the playback modes actually
    // changes. is_playing is included so a same-track pause flips the
    // Play/Pause mark without waiting for a poll; the modes are included so
    // a change made in another Spotify client repaints the Shuffle/Repeat
    // check marks (and the next click toggles from the fresh belief).
    //
    // Window visibility is computed up front so the dedup key includes
    // it — otherwise a hide/show click would early-return and the label
    // would go stale.
    // Issue #886: the key reads the visibility mirror, so this path performs no
    // event-loop hop — `live_window_visible` below re-reads the real window on
    // the way to a paint, which is where the label is built.
    let key_visible = window_visible();
    // 4.7.0 (S9 / issue #677): the snooze is resolved once, up front, because it
    // feeds three separate decisions below — the dedup key, the fetch mode and
    // the rendered status line. Issue #886: it comes from a scoped read of the
    // mounted config, so no whole-`AppConfig` clone is allocated per poll and no
    // read guard survives into the blocking Spotify HTTP below. `state` is the
    // same handle the rest of the rebuild uses.
    let state = app.state::<std::sync::Arc<crate::AppState>>();
    let snooze = snooze_from_app_state(state.inner());
    let snooze_key = snooze.as_ref().map(|sn| snooze_dedup_key(&sn.status));
    // Issue #883: a player action asks for fresh Devices/Up Next lists, but only
    // when it is not already covered by a fetch in flight (`action_fetch_due`),
    // and never while snoozed (`paint_fetch_mode`).
    let action_refresh_due = action_fetch_due(
        *LAST_TRAY_ACTION.lock(),
        *LAST_ACTION_FETCH.lock(),
        Instant::now(),
        TRAY_POST_ACTION_FETCH_MIN,
    );
    let fetch = paint_fetch_mode(paint, snooze.is_some(), action_refresh_due);
    // Issue #805: the two cache buckets are part of the key, so a session where
    // nothing else moves still repaints — and therefore re-fetches — once per
    // throttle window instead of keeping whatever the last rebuild rendered.
    let (devices_bucket, queue_bucket) = cache_buckets();
    let snapshot = tray_snapshot_for(
        is_syncing,
        key_visible,
        current_track,
        snooze_key,
        devices_bucket,
        queue_bucket,
    );
    {
        let last = last_tray_state().lock();
        if !tray_state_changed(last.as_ref(), &snapshot) {
            // No-op: menu state hasn't changed.
            return Ok(());
        }
        // Re-seed the Play/Pause toggle's playing state when the polling
        // loop observed a genuinely new track (or a stop): its stored
        // `is_playing` is only fresh at track-change time. Tray-initiated
        // actions and the poller's `playback-state-changed` event update
        // LAST_PLAYING_STATE themselves, and a forced rebuild keeps the
        // same track_key, so neither path re-seeds here.
        let track_changed = match last.as_ref() {
            Some(prev) => prev.track_key != snapshot.track_key,
            None => true,
        };
        if track_changed {
            let fresh_playing = current_track
                .as_ref()
                .map(|t| t.is_playing)
                .unwrap_or(false);
            note_playing_state(fresh_playing);
        }
        // Do NOT update the snapshot yet. If the rebuild below fails
        // (e.g., set_menu returns Err), we want the next call with the
        // same state to retry rather than no-op on a stale snapshot.
    }

    // Issue #886: the key was built from the mirror; the label below is built
    // from the real window — one query, paid only on this path, which is about
    // to perform Spotify HTTP anyway. A show/hide that did not report itself
    // therefore cannot leave the Show/Hide label wrong, and the mirror self-heals
    // for the polls that follow.
    let is_window_visible = live_window_visible(app);
    note_window_visibility(is_window_visible);

    // Fetch Spotify data OUTSIDE the tray write lock, and only when the fetch
    // mode allows it. The throttled caches are snapshotted and fetched without
    // holding either cache mutex across HTTP (see cached_devices/cached_queue),
    // and the tray lock is not yet held so a concurrent tray click or polling
    // update never blocks on the network. Benign double-fetch race documented on
    // those helpers. See issue #217.
    let access_token = state
        .tokens
        .spotify()
        .as_ref()
        .map(|t| t.access_token.clone());
    if fetch == TrayFetch::RefreshNow {
        // Recorded BEFORE the requests run: a click that lands while they are in
        // flight must reuse them, not start a pair of its own (issue #883).
        *LAST_ACTION_FETCH.lock() = Some(Instant::now());
    }
    let devices: Vec<crate::spotify::DeviceInfo> = devices_for_menu(access_token.as_deref(), fetch);
    let queue: Option<crate::spotify::QueueInfo> = queue_for_menu(access_token.as_deref(), fetch);

    // Build menu items without holding the tray write lock. Only the final
    // tray.set_menu call needs serialising — everything above is pure data
    // preparation and menu-item construction.
    // Issue #674: every label below comes from the installed i18n table, so a
    // language switch only has to rebuild the menu.
    let s = i18n::current();
    // Determine Show/Hide label based on the precomputed visibility.
    let show_hide_label = if is_window_visible {
        s.hide_window
    } else {
        s.show_window
    };

    let show_hide = MenuItemBuilder::with_id(ID_SHOW_HIDE, show_hide_label)
        .build(app)
        .map_err(|e| {
            log::warn!(
                "[TRAY] update_tray_menu: failed to build show_hide menu item: {}",
                e
            );
            e.to_string()
        })?;

    // Pause/Resume label based on sync state
    let pause_resume_id = if is_syncing {
        ID_PAUSE_SYNC
    } else {
        ID_RESUME_SYNC
    };
    let pause_resume_label = if is_syncing {
        s.pause_sync
    } else {
        s.resume_sync
    };
    let pause_resume = MenuItemBuilder::with_id(pause_resume_id, pause_resume_label)
        .build(app)
        .map_err(|e| {
            log::warn!(
                "[TRAY] update_tray_menu: failed to build pause_resume menu item: {}",
                e
            );
            e.to_string()
        })?;

    let separator1 = PredefinedMenuItem::separator(app).map_err(|e| {
        log::warn!("[TRAY] update_tray_menu: failed to build separator1: {}", e);
        e.to_string()
    })?;
    // separator2 inserted only when track is added (see below)
    let separator3 = PredefinedMenuItem::separator(app).map_err(|e| {
        log::warn!("[TRAY] update_tray_menu: failed to build separator3: {}", e);
        e.to_string()
    })?;

    let open_settings = MenuItemBuilder::with_id(ID_OPEN_SETTINGS, s.open_settings)
        .build(app)
        .map_err(|e| {
            log::warn!(
                "[TRAY] update_tray_menu: failed to build open_settings menu item: {}",
                e
            );
            e.to_string()
        })?;

    let open_logs = MenuItemBuilder::with_id(ID_OPEN_LOGS, s.open_logs_folder)
        .build(app)
        .map_err(|e| {
            log::warn!(
                "[TRAY] update_tray_menu: failed to build open_logs menu item: {}",
                e
            );
            e.to_string()
        })?;

    let quit = MenuItemBuilder::with_id(ID_QUIT, s.quit)
        .build(app)
        .map_err(|e| {
            log::warn!(
                "[TRAY] update_tray_menu: failed to build quit menu item: {}",
                e
            );
            e.to_string()
        })?;

    // Spotify playback controls (issue #3.0-P3). Play/Pause is a single
    // check-item whose native checked state comes from LAST_PLAYING_STATE
    // (see the static's docs — the polling loop's stored track goes stale
    // on a same-track pause); the Devices/Up Next submenus are built from
    // the pre-fetched throttled caches so the polling loop's per-iteration
    // rebuilds don't hammer the Spotify API. The checkmark is derived from
    // (track_id, is_playing) via the track_key dedup and LAST_PLAYING_STATE,
    // so a same-track pause flips without waiting for the next poll. See
    // issues #229 and #217.
    let is_playing = LAST_PLAYING_STATE.load(Ordering::Acquire);
    // Issue #591: a disabled head-of-menu status line stating what the app
    // is actually doing (syncing / paused / not syncing). Everything below
    // is derived from state already in scope for this rebuild, so the line
    // cannot drift from the Show/Hide and Pause/Resume items beside it.
    // S9 (issue #677): a snooze replaces the line outright — the remaining time
    // and the local deadline are the whole answer while polling is suspended,
    // and naming a track would describe work the app is deliberately not doing.
    let status_line = match &snooze {
        Some(sn) => snooze_status_line(s, sn),
        None => sync_status_line(s, is_syncing, is_playing, current_track),
    };
    let sync_status = MenuItemBuilder::with_id(ID_SYNC_STATUS, status_line.clone())
        .enabled(false)
        .build(app)
        .map_err(|e| {
            log::warn!(
                "[TRAY] update_tray_menu: failed to build sync_status item: {}",
                e
            );
            e.to_string()
        })?;
    let play_pause = CheckMenuItemBuilder::with_id(ID_PLAY_PAUSE, s.play_pause)
        .checked(is_playing)
        .build(app)
        .map_err(|e| {
            log::warn!(
                "[TRAY] update_tray_menu: failed to build play_pause item: {}",
                e
            );
            e.to_string()
        })?;
    let previous = MenuItemBuilder::with_id(ID_PREVIOUS, s.previous)
        .build(app)
        .map_err(|e| {
            log::warn!(
                "[TRAY] update_tray_menu: failed to build previous item: {}",
                e
            );
            e.to_string()
        })?;
    let next = MenuItemBuilder::with_id(ID_NEXT, s.next)
        .build(app)
        .map_err(|e| {
            log::warn!("[TRAY] update_tray_menu: failed to build next item: {}", e);
            e.to_string()
        })?;
    // Issue #582: the playback-mode toggles. Their marks come from the
    // module-level atoms the polling loop feeds from the poll body (the
    // shuffle/repeat state is free — the same response the app already
    // parses), so no extra request and no cache machinery is involved.
    // Both atoms are part of the dedup snapshot (issue #691), so a mode
    // changed from another client forces the rebuild that repaints the
    // mark; a click on these items rebuilds through `force_tray_refresh`
    // and is reflected immediately.
    let shuffle = CheckMenuItemBuilder::with_id(ID_SHUFFLE, s.shuffle)
        .checked(LAST_SHUFFLE_STATE.load(Ordering::Acquire))
        .build(app)
        .map_err(|e| {
            log::warn!(
                "[TRAY] update_tray_menu: failed to build shuffle item: {}",
                e
            );
            e.to_string()
        })?;
    let repeat_state = last_repeat_state();
    let repeat = CheckMenuItemBuilder::with_id(ID_REPEAT, repeat_menu_label(s, repeat_state))
        .checked(repeat_state.is_on())
        .build(app)
        .map_err(|e| {
            log::warn!(
                "[TRAY] update_tray_menu: failed to build repeat item: {}",
                e
            );
            e.to_string()
        })?;
    let playback_separator = PredefinedMenuItem::separator(app).map_err(|e| {
        log::warn!(
            "[TRAY] update_tray_menu: failed to build playback_separator: {}",
            e
        );
        e.to_string()
    })?;

    let devices_submenu = build_devices_submenu_from_devices(app, &devices).map_err(|e| {
        log::warn!(
            "[TRAY] update_tray_menu: failed to build devices submenu: {}",
            e
        );
        e
    })?;
    let queue_submenu = build_queue_submenu_from_queue(app, queue.as_ref()).map_err(|e| {
        log::warn!(
            "[TRAY] update_tray_menu: failed to build queue submenu: {}",
            e
        );
        e
    })?;
    // S9 (issue #677): the snooze submenu, rebuilt with the resolved snooze so
    // the "Resume sync now" entry appears exactly while one is active. Labels
    // come from the same installed table as everything above.
    let snooze_submenu = build_snooze_submenu(app, s, snooze.as_ref()).map_err(|e| {
        log::warn!(
            "[TRAY] update_tray_menu: failed to build snooze submenu: {}",
            e
        );
        e
    })?;

    // Build menu with optional track info
    let mut menu_builder = MenuBuilder::new(app)
        .items(&[
            &sync_status,
            &show_hide,
            &pause_resume,
            &snooze_submenu,
            &separator1,
        ])
        .items(&[&play_pause, &previous, &next, &shuffle, &repeat])
        .item(&playback_separator)
        .items(&[&devices_submenu, &queue_submenu]);

    // Add current track item if playing — insert separator2 here too.
    // Issue #956: the gate is the rebuild's OWN `is_playing` binding — the same
    // one the Play/Pause mark and the status line read — not the stored track's
    // flag. `run_player_action` records the new playing state and forces this
    // rebuild without re-storing the track, so a tray-initiated pause used to
    // leave this row naming a track the tray had just paused.
    if is_playing {
        if let Some(track) = &current_track {
            let separator2 = PredefinedMenuItem::separator(app).map_err(|e| {
                log::warn!("[TRAY] update_tray_menu: failed to build separator2: {}", e);
                e.to_string()
            })?;
            let track_item = MenuItemBuilder::with_id(
                ID_CURRENT_TRACK,
                format!("🎵 {} - {}", track.artist, track.title),
            )
            .enabled(false)
            .build(app)
            .map_err(|e| {
                log::warn!("[TRAY] update_tray_menu: failed to build track_item: {}", e);
                e.to_string()
            })?;
            menu_builder = menu_builder.item(&track_item).item(&separator2);
        }
    }

    let menu = menu_builder
        .items(&[&open_settings, &open_logs, &separator3, &quit])
        .build()
        .map_err(|e| {
            log::warn!("[TRAY] update_tray_menu: failed to build menu: {}", e);
            e.to_string()
        })?;

    // Acquire the tray write lock ONLY around the final set_menu. The long
    // HTTP fetches and the entire menu build above ran without it, so neither
    // a polling-thread rebuild nor a main-thread tray click blocks on the
    // network. See issue #217.
    {
        let _write_guard = tray_write_lock().lock();
        tray.set_menu(Some(menu)).map_err(|e| {
            log::warn!("[TRAY] update_tray_menu: failed to set tray menu: {}", e);
            format!("Failed to set tray menu: {}", e)
        })?;
    }

    // C4 polish: keep the tray tooltip live — "Artist — Track (▶|⏸)" while
    // a track is known, the plain app name otherwise. This runs on every
    // rebuild, i.e. exactly whenever track info changes (the dedup key
    // already covers artist/title/is_playing), and performs no IO and no
    // extra locking beyond the tray handle itself.
    // Issue #956: the glyph comes from the rebuild's own `is_playing` binding,
    // like the row above and the check mark, so one hover cannot contradict the
    // menu it belongs to.
    let track_tooltip = match &current_track {
        Some(t) => format!(
            "{} — {} ({})",
            t.artist,
            t.title,
            if is_playing { "▶" } else { "⏸" }
        ),
        None => "PresenceJam".to_string(),
    };
    // Issue #591: the status line leads the tooltip, so a hover states
    // whether PresenceJam is syncing even when no track row is present
    // (and off macOS, where the presence-gated dock badge is a no-op).
    let tooltip = format!("{} · {}", status_line, track_tooltip);
    if let Err(e) = tray.set_tooltip(Some(tooltip)) {
        log::warn!("[TRAY] update_tray_menu: failed to set tooltip: {}", e);
    }

    // Issue #971: `set_tooltip` is a documented no-op on Linux, which has no
    // hover surface for an AppIndicator — so the same summary rides as the
    // indicator's title, which Linux renders beside the icon. That closes the
    // gap where the status line, the current track and the snooze countdown
    // were only visible after opening the menu. Windows and macOS keep the
    // tooltip and set no title (the call is a no-op there anyway).
    #[cfg(target_os = "linux")]
    if let Err(e) = tray.set_title(Some(status_line.clone())) {
        log::warn!("[TRAY] update_tray_menu: failed to set tray title: {}", e);
    }

    // Commit the snapshot only after a successful set_menu. A failed
    // set_menu above left the snapshot at the previous value, so the
    // next call with the same state will retry rather than no-op.
    // Issue #882: the startup paint never commits — it renders the caches
    // only, so recording it would dedup away the first real rebuild.
    if paint == TrayPaint::Deduped {
        *last_tray_state().lock() = Some(snapshot);
    }

    log::info!(
        "[TRAY] update_tray_menu: tray menu updated - is_syncing={}, visible={}, track={:?}",
        is_syncing,
        is_window_visible,
        current_track
            .as_ref()
            .map(|t| format!("{} - {}", t.artist, t.title))
    );
    Ok(())
}

/// C4 polish: reflect presence-gated sync on the macOS dock icon. While a
/// track's Teams status write is suppressed by the presence gate
/// (`polling/poll_once.rs` records it in `gated_track_key`), the dock icon
/// shows a badge so the user can see presence updates are being held back;
/// cleared when the gate lifts or syncing stops. No-op off macOS so call
/// sites stay cfg-free and compile everywhere.
///
/// SINGLE CALL-SITE (owned by the polling driver, feat/v4-c11): in
/// `polling/loop.rs::polling_loop`, immediately AFTER the post-iteration
/// `tray::update_tray_menu(...)` block, add
/// `tray::set_presence_gated_badge(&app, gated_track_key.is_some());`,
/// and after the loop exits, add
/// `tray::set_presence_gated_badge(&app, false);`.
#[cfg(target_os = "macos")]
pub fn set_presence_gated_badge(app: &AppHandle, gated: bool) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if let Err(e) = window.set_badge_count(if gated { Some(1) } else { None }) {
        log::warn!(
            "[TRAY] set_presence_gated_badge: failed to set dock badge: {}",
            e
        );
    }
}

#[cfg(not(target_os = "macos"))]
pub fn set_presence_gated_badge(_app: &AppHandle, _gated: bool) {}

/// Runs a tray-library call, turning the panic a missing native tray library
/// raises into the `Err` this module's contract promises (issue #927).
///
/// The reason is logged once, at error level, together with what failed: an
/// AppImage whose bundler could not see a dlopen-only dependency is the likely
/// host, and the log line is the only thing that says so.
fn guard_tray_panic<T>(
    what: &str,
    f: impl FnOnce() -> Result<T, String> + std::panic::UnwindSafe,
) -> Result<T, String> {
    match std::panic::catch_unwind(f) {
        Ok(result) => result,
        Err(payload) => {
            let reason = payload
                .downcast_ref::<&str>()
                .map(|s| (*s).to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".to_string());
            log::error!(
                "[TRAY] {}: the tray library is unavailable ({}); running without a tray",
                what,
                reason
            );
            Err(format!("{} unavailable: {}", what, reason))
        }
    }
}

/// Whether a tray icon actually exists (issue #927).
///
/// `setup_tray` can now fail without panicking, and a session with no tray has
/// no reachable way back to a window that close-to-tray has hidden — so the
/// caller that hides it (lib.rs) has to gate on this.
pub fn tray_available() -> bool {
    get_tray().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    // Issue #674: the label helpers are table-driven, so the tests name the
    // tables directly instead of relying on the process-wide locale.
    use crate::i18n::{DE, EN, FR};

    /// Issue #388: device menu ids must parse to stable Spotify device ids,
    /// never to a list index. Pure helper — no Tauri runtime needed.
    #[test]
    fn parse_device_menu_id_carries_stable_id() {
        assert_eq!(
            parse_device_menu_id("abc123XYZ"),
            DeviceMenuSelection::DeviceId("abc123XYZ".to_string())
        );
        assert_eq!(
            parse_device_menu_id("0"),
            DeviceMenuSelection::LegacyIndex(0)
        );
        assert_eq!(
            parse_device_menu_id("12"),
            DeviceMenuSelection::LegacyIndex(12)
        );
        assert_eq!(parse_device_menu_id("none"), DeviceMenuSelection::Invalid);
        assert_eq!(parse_device_menu_id(""), DeviceMenuSelection::Invalid);
    }
    /// Issue #482: id-snapshot edge cases -- a reorder between menu build
    /// and click must still hit the labelled device (id-keyed resolution),
    /// and unknown/empty selections must resolve to Invalid, never to a
    /// neighboring device by index.
    #[test]
    fn parse_device_menu_id_rejects_unknown_and_empty() {
        // Spotify ids are opaque base62; digit-only suffixes are the legacy
        // index scheme and must keep parsing as LegacyIndex, never as an id.
        assert_eq!(
            parse_device_menu_id("007"),
            DeviceMenuSelection::LegacyIndex(7)
        );
        // Mixed alphanumeric ids (even with a leading digit) are stable ids.
        assert_eq!(
            parse_device_menu_id("0abc"),
            DeviceMenuSelection::DeviceId("0abc".to_string())
        );
        // The placeholder and the empty string are never a device.
        assert_eq!(parse_device_menu_id("none"), DeviceMenuSelection::Invalid);
        assert_eq!(parse_device_menu_id(""), DeviceMenuSelection::Invalid);
        // Redaction helper never leaks the id itself.
        let logged = selected_for_log(&DeviceMenuSelection::DeviceId("secret-id-123".to_string()));
        assert!(
            !logged.contains("secret-id-123"),
            "device id must not appear in logs"
        );
        assert!(
            logged.contains("len="),
            "redacted label must carry the length"
        );
    }

    /// Issue #388: the click handler must resolve by id with a live
    /// re-fetch fallback instead of `devices.get(i)`. Issue #586: that
    /// re-fetch must resolve its token through the shared refresh-aware
    /// policy, so an expired token does not strand a transfer.
    #[test]
    fn device_click_resolves_by_id_with_live_fallback() {
        let src = include_str!("tray.rs");
        let body = body_of(prod_source(src), "fn resolve_device_id(");
        assert!(
            body.contains("crate::spotify::get_devices"),
            "resolve_device_id must live re-fetch when the cache misses"
        );
        assert!(
            body.contains("player_with_refresh_typed("),
            "the live re-fetch must use the refresh-aware token policy (issue #586)"
        );
        assert!(
            !body.contains("tokens.spotify()"),
            "resolve_device_id must not snapshot the raw access token (issue #586)"
        );
        assert!(
            !body.contains("devices.get(*i).cloned()") || body.contains("LegacyIndex"),
            "index lookup must survive only on the LegacyIndex back-compat path"
        );
    }

    /// Issues #383/#386: the tray Quit arm must terminate via the shared
    /// graceful shutdown, and the playback/device click arms must offload
    /// blocking Spotify HTTP onto worker threads. Issue #587: the Show/Hide
    /// repaint rebuilds a menu that can fetch Spotify over the network, so
    /// it must be offloaded too.
    #[test]
    fn tray_click_arms_quit_and_offload() {
        let src = include_str!("tray.rs");
        let body = body_of(prod_source(src), "pub fn setup_tray(");
        // #383: Quit terminates even with no frontend listener.
        let quit_pos = body
            .find("ID_QUIT =>")
            .expect("setup_tray must handle ID_QUIT");
        let quit_tail = &body[quit_pos..quit_pos + 600.min(body.len() - quit_pos)];
        assert!(
            quit_tail.contains("request_graceful_shutdown"),
            "tray ID_QUIT arm must route through request_graceful_shutdown"
        );
        assert!(
            !quit_tail.contains("window.hide()"),
            "the hide-only quit arm (issue #383) must stay gone"
        );
        // #386: no blocking Spotify HTTP directly on the menu-event thread.
        for marker in [
            "get_currently_playing(token, None)",
            "player_pause(t, None)",
            "player_play(t, None)",
            "player_previous(token, None)",
            "player_next(token, None)",
            "player_transfer(token,",
        ] {
            let pos = body
                .find(marker)
                .unwrap_or_else(|| panic!("expected click-path marker `{}` in setup_tray", marker));
            let before = &body[..pos];
            assert!(
                before.rfind("std::thread::spawn").is_some(),
                "blocking call `{}` must run inside a spawned worker thread",
                marker
            );
        }
        // #587: the Show/Hide repaint must be offloaded. `update_tray_menu`
        // fetches Spotify devices/queue with a 10 s timeout once the fetch
        // throttle lapses, so rebuilding inline would wedge the menu the
        // same way the #386 player arms used to.
        let show_pos = body
            .find("ID_SHOW_HIDE =>")
            .expect("setup_tray must handle ID_SHOW_HIDE");
        let show_end = body[show_pos..]
            .find("ID_PAUSE_SYNC")
            .map(|i| show_pos + i)
            .unwrap_or(body.len());
        let show_arm = &body[show_pos..show_end];
        assert!(
            !show_arm.contains("update_tray_menu("),
            "the Show/Hide arm must not rebuild the tray menu inline (issue #587)"
        );
        assert!(
            show_arm.contains("refresh_tray_from_state("),
            "the Show/Hide arm must repaint through the offloading refresh helper"
        );
        // #588: the Pause/Resume label is repainted from backend truth after
        // the frontend's asynchronous toggle, not left to the Dashboard.
        let pause_pos = body
            .find("ID_PAUSE_SYNC | ID_RESUME_SYNC =>")
            .expect("setup_tray must handle ID_PAUSE_SYNC | ID_RESUME_SYNC");
        let pause_end = body[pause_pos..]
            .find("ID_QUIT =>")
            .map(|i| pause_pos + i)
            .unwrap_or(body.len());
        let pause_arm = &body[pause_pos..pause_end];
        assert!(
            pause_arm.contains("std::thread::spawn"),
            "the Pause/Resume arm must repaint from a worker (issue #588)"
        );
        assert!(
            pause_arm.contains("await_sync_toggle("),
            "the Pause/Resume arm must wait for the toggle to settle before repainting"
        );
        // #586: no raw token snapshot on the click path — the refresh-aware
        // policy resolves the token (and retries once on ExpiredToken).
        assert!(
            !show_arm.contains("tokens.spotify()"),
            "the click path must not read the raw Spotify token snapshot"
        );
    }

    /// Production half of `src` — everything before the inline test module,
    /// so a scan can never match the assertions themselves.
    fn prod_source(src: &str) -> &str {
        src.split("#[cfg(test)]\nmod tests")
            .next()
            .expect("tray.rs has no #[cfg(test)] mod tests block")
    }

    /// Drops `//` line comments so a source-scan assertion is not fooled by
    /// prose that quotes the very construct it forbids: a comment saying
    /// "never snapshot `state.tokens.spotify()`" must not read as a
    /// snapshot. String literals are not parsed, so a `//` inside a literal
    /// truncates the rest of that line — that can only lose trailing text,
    /// never invent it. Stripping also removes any brace a comment mentions,
    /// which keeps the brace counting below from being unbalanced by prose.
    fn strip_line_comments(src: &str) -> String {
        src.lines()
            .map(|line| match line.find("//") {
                Some(i) => &line[..i],
                None => line,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Comment-stripped, brace-counted body isolation for `sig`'s fn.
    /// Order-independent: never anchor on the next fn.
    fn body_of(prod: &str, sig: &str) -> String {
        let stripped = strip_line_comments(prod);
        let after_sig = stripped
            .split(sig)
            .nth(1)
            .unwrap_or_else(|| panic!("tray.rs has no `{}`", sig));
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
        after_sig[..end.unwrap_or_else(|| panic!("{} body never closed", sig))].to_string()
    }

    /// Issue #586: tray player actions used to snapshot
    /// `state.tokens.spotify()` — an expired access token meant a failed
    /// click with no refresh and no retry, while the refresh-aware policy in
    /// `commands/playback.rs` had no callers. Both tray paths must now route
    /// through that shared policy.
    #[test]
    fn tray_player_actions_use_refresh_aware_token() {
        let src = include_str!("tray.rs");
        let prod = src
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("tray.rs has no #[cfg(test)] mod tests block");
        let action_body = body_of(prod, "fn run_player_action(");
        assert!(
            action_body.contains("player_with_refresh_typed("),
            "run_player_action must route through the shared refresh-aware policy (issue #586)"
        );
        assert!(
            !action_body.contains("tokens.spotify()"),
            "run_player_action must not snapshot the raw access token (issue #586)"
        );
        let setup_body = body_of(prod, "pub fn setup_tray(");
        let play_pos = setup_body
            .find("ID_PLAY_PAUSE =>")
            .expect("setup_tray must handle ID_PLAY_PAUSE");
        let play_end = setup_body[play_pos..]
            .find("ID_PREVIOUS =>")
            .map(|i| play_pos + i)
            .unwrap_or(setup_body.len());
        let play_arm = &setup_body[play_pos..play_end];
        assert!(
            play_arm.contains("player_with_refresh_typed("),
            "the Play/Pause arm must read playback state through the refresh-aware policy (issue #586)"
        );
        assert!(
            !play_arm.contains("tokens.spotify()"),
            "the Play/Pause arm must not snapshot the raw access token (issue #586)"
        );
    }

    /// Issue #591: the tray must state whether the app is syncing instead of
    /// leaving it to be inferred from the Pause/Resume verb. Issue #674: the
    /// status word comes from the table the caller hands in, so the same
    /// helper renders German/French without a second code path.
    #[test]
    fn sync_status_line_reports_backend_state_in_every_locale() {
        let track = crate::spotify::TrackInfo {
            title: "Track".to_string(),
            artist: "Artist".to_string(),
            album: "Album".to_string(),
            album_art_url: String::new(),
            is_playing: true,
            progress_ms: None,
            duration_ms: 0,
        };
        assert_eq!(
            sync_status_line(&EN, false, true, Some(&track)),
            "Not syncing",
            "a stopped poller outranks any remembered track"
        );
        assert_eq!(
            sync_status_line(&EN, false, false, None),
            "Not syncing",
            "no track and no sync is still Not syncing"
        );
        assert_eq!(
            sync_status_line(&EN, true, false, None),
            "Syncing — no track",
            "syncing with nothing playing must say so rather than claim a track"
        );
        assert_eq!(
            sync_status_line(&EN, true, true, Some(&track)),
            "Syncing — Artist — Track"
        );
        assert_eq!(
            sync_status_line(&EN, true, false, Some(&track)),
            "Paused — Artist — Track",
            "a same-track pause is reported from the live playing state"
        );

        // Issue #674: the selected table is what the user reads — Deutsch and
        // Français must not fall back to the English status words.
        assert_eq!(
            sync_status_line(&DE, true, true, Some(&track)),
            format!("{} — Artist — Track", DE.status_syncing)
        );
        assert_eq!(
            sync_status_line(&DE, true, false, Some(&track)),
            format!("{} — Artist — Track", DE.status_paused)
        );
        assert_eq!(
            sync_status_line(&DE, false, false, None),
            DE.status_not_syncing
        );
        assert_eq!(
            sync_status_line(&FR, true, false, None),
            FR.status_syncing_no_track
        );
    }

    /// Issue #582: a check mark can only carry on/off, while `repeat_state`
    /// has three documented values — so the label must name the mode, or a
    /// user cannot tell "repeat one" from "repeat the playlist". Issue #674:
    /// the label is read from the installed table.
    #[test]
    fn repeat_menu_label_spells_out_the_mode() {
        assert_eq!(repeat_menu_label(&EN, RepeatState::Off), "Repeat: Off");
        assert_eq!(
            repeat_menu_label(&EN, RepeatState::Context),
            "Repeat: Context"
        );
        assert_eq!(repeat_menu_label(&EN, RepeatState::Track), "Repeat: Track");

        assert_eq!(repeat_menu_label(&DE, RepeatState::Off), "Wiederholen: Aus");
        assert_eq!(
            repeat_menu_label(&DE, RepeatState::Context),
            "Wiederholen: Kontext"
        );
        assert_eq!(
            repeat_menu_label(&DE, RepeatState::Track),
            "Wiederholen: Titel"
        );
        assert_eq!(
            repeat_menu_label(&FR, RepeatState::Track),
            "Répéter : titre"
        );
    }

    /// Issue #582: the two toggles render the state the poll body reported
    /// (`note_playback_modes` → the atoms the menu build reads), and the
    /// click target is the documented cycle, so a successful toggle leaves
    /// the item showing what the API was just told to adopt.
    #[test]
    fn playback_modes_feed_both_toggle_items() {
        // The mode atoms are process-global and read by the rebuild path, so
        // the tests that drive them must not interleave.
        let _guard = MODE_ATOM_LOCK.lock();
        note_playback_modes(true, RepeatState::Track);
        assert!(LAST_SHUFFLE_STATE.load(Ordering::Acquire));
        assert_eq!(last_repeat_state(), RepeatState::Track);
        assert_eq!(repeat_menu_label(&EN, last_repeat_state()), "Repeat: Track");
        // The click targets: Repeat advances along the documented cycle,
        // Shuffle flips whatever was last observed.
        assert_eq!(last_repeat_state().next(), RepeatState::Off);
        assert!(!shuffle_toggle_target(
            LAST_SHUFFLE_STATE.load(Ordering::Acquire)
        ));

        // A poll that reports everything off must clear both items.
        note_playback_modes(false, RepeatState::Off);
        assert!(!LAST_SHUFFLE_STATE.load(Ordering::Acquire));
        assert_eq!(last_repeat_state(), RepeatState::Off);
        assert!(!last_repeat_state().is_on());
    }

    /// Serialises the tests that drive the process-global playback-mode
    /// atoms, which the rebuild path also reads.
    static MODE_ATOM_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

    /// Issue #691: the dedup key is built by the same function the rebuild
    /// path uses, and it carries both playback modes — so a shuffle/repeat
    /// change made in another Spotify client forces the repaint that updates
    /// the check marks (and the next click toggles from the fresh belief),
    /// while an unchanged poll still dedupes to a no-op.
    #[test]
    fn mode_change_forces_a_tray_rebuild() {
        let _guard = MODE_ATOM_LOCK.lock();
        let track = |is_playing: bool| crate::spotify::TrackInfo {
            title: "Title".to_string(),
            artist: "Artist".to_string(),
            album: "Album".to_string(),
            album_art_url: String::new(),
            is_playing,
            progress_ms: None,
            duration_ms: 0,
        };
        let key = |sync: bool, visible: bool| {
            tray_snapshot_for(sync, visible, Some(&track(true)), None, 0, 0)
        };

        note_playback_modes(false, RepeatState::Off);
        let base = key(true, true);

        // The poll body reports a shuffle the user toggled in the Spotify app.
        note_playback_modes(true, RepeatState::Off);
        let shuffled = key(true, true);
        assert!(
            tray_state_changed(Some(&base), &shuffled),
            "an external shuffle change must repaint the tray (issue #691)"
        );

        // …and a repeat-mode change.
        note_playback_modes(true, RepeatState::Context);
        let repeated = key(true, true);
        assert!(
            tray_state_changed(Some(&shuffled), &repeated),
            "an external repeat change must repaint the tray (issue #691)"
        );

        // An unchanged poll is still a no-op.
        assert!(
            !tray_state_changed(Some(&repeated), &key(true, true)),
            "an unchanged poll must not rebuild the menu"
        );
        assert!(
            tray_state_changed(None, &repeated),
            "the first call has no snapshot and must always rebuild"
        );

        // The pre-existing parts of the key still repaint.
        assert!(
            tray_state_changed(Some(&repeated), &key(false, true)),
            "a sync toggle must still repaint (issue #71)"
        );
        assert!(
            tray_state_changed(Some(&repeated), &key(true, false)),
            "a Show/Hide click must still repaint (issue #71)"
        );

        // A same-track pause lives in the track half of the key (issue #229).
        let paused = tray_snapshot_for(true, true, Some(&track(false)), None, 0, 0);
        assert!(
            tray_state_changed(Some(&repeated), &paused),
            "a same-track pause must still repaint the Play/Pause mark (#229)"
        );

        // Leave the shared atoms as a fresh poll would find them.
        note_playback_modes(false, RepeatState::Off);
    }

    /// Issue #689 (D6, tray half): the poller emits `playback-state-changed`
    /// when a track's playing state changes without the track itself
    /// changing. Consuming it moves the Play/Pause mark and the status line
    /// off "playing" — the tray used to keep claiming the stale state until
    /// the next track emerged.
    #[test]
    fn playback_state_event_moves_the_play_pause_mark() {
        let track = crate::spotify::TrackInfo {
            title: "Track".to_string(),
            artist: "Artist".to_string(),
            album: "Album".to_string(),
            album_art_url: String::new(),
            is_playing: true,
            progress_ms: None,
            duration_ms: 0,
        };
        let mark = |playing: bool| sync_status_line(&EN, true, playing, Some(&track));

        note_playing_state(true);
        assert_eq!(
            mark(LAST_PLAYING_STATE.load(Ordering::Acquire)),
            "Syncing — Artist — Track"
        );

        // The exact payload shape the poller emits for a same-track pause.
        consume_playback_state_changed(r#"{"is_playing":false,"track_key":"Artist|Title"}"#);
        assert!(
            !LAST_PLAYING_STATE.load(Ordering::Acquire),
            "the tray must believe a same-track pause (issue #689)"
        );
        assert_eq!(
            mark(LAST_PLAYING_STATE.load(Ordering::Acquire)),
            "Paused — Artist — Track"
        );

        consume_playback_state_changed(r#"{"is_playing":true,"track_key":"Artist|Title"}"#);
        assert!(LAST_PLAYING_STATE.load(Ordering::Acquire));
        assert_eq!(
            mark(LAST_PLAYING_STATE.load(Ordering::Acquire)),
            "Syncing — Artist — Track"
        );

        // A payload the tray cannot parse must keep the last known state
        // rather than invent one.
        consume_playback_state_changed("not json");
        assert!(
            LAST_PLAYING_STATE.load(Ordering::Acquire),
            "an unparsable payload must not clobber the last playing state"
        );
    }

    /// Issue #689 (D6): the tray's belief only moves if the poller's
    /// `playback-state-changed` event is actually subscribed. A mistyped or
    /// deleted `app.listen` would leave the functional test above green — it
    /// calls the consumer directly — while the running app ignored the event.
    /// Guards the registration inside `setup_tray`, like the click-arm scans.
    #[test]
    fn setup_tray_subscribes_to_playback_state_changes() {
        let src = include_str!("tray.rs");
        let body = body_of(prod_source(src), "pub fn setup_tray(");
        assert!(
            body.contains("app.listen(\"playback-state-changed\""),
            "setup_tray must subscribe to the poller's playback-state-changed event (issue #689)"
        );
        assert!(
            body.contains("consume_playback_state_changed(event.payload())"),
            "the subscription must hand the payload to the tray's consumer"
        );
    }

    // -----------------------------------------------------------------------
    // S9 (issue #677): the snooze submenu, the countdown status line and the
    // snooze-aware dedup key / fetch mode.
    // -----------------------------------------------------------------------

    /// A config carrying `snooze_until` as stored.
    fn snoozed_config(stored: &str) -> crate::config::AppConfig {
        crate::config::AppConfig {
            snooze_until: Some(stored.to_string()),
            ..crate::config::AppConfig::default()
        }
    }

    /// A deadline `seconds` from now, in the stored spelling.
    fn deadline_in(seconds: i64) -> String {
        crate::config::snooze_store_form(chrono::Utc::now() + chrono::TimeDelta::seconds(seconds))
    }

    /// Every id the submenu can mint maps to exactly one action, and anything
    /// else is refused — a stale menu from an older build must not snooze.
    #[test]
    fn snooze_menu_ids_parse_to_their_action() {
        use crate::config::SnoozePreset;
        assert_eq!(
            parse_snooze_menu_id(ID_SNOOZE_30),
            Some(SnoozeMenuSelection::Preset(SnoozePreset::ThirtyMinutes))
        );
        assert_eq!(
            parse_snooze_menu_id(ID_SNOOZE_1H),
            Some(SnoozeMenuSelection::Preset(SnoozePreset::OneHour))
        );
        assert_eq!(
            parse_snooze_menu_id(ID_SNOOZE_TOMORROW),
            Some(SnoozeMenuSelection::Preset(SnoozePreset::UntilTomorrow))
        );
        assert_eq!(
            parse_snooze_menu_id(ID_SNOOZE_RESUME),
            Some(SnoozeMenuSelection::Resume)
        );
        // The prefix alone, an unknown suffix and a foreign id all resolve to
        // nothing rather than to a default preset.
        for unknown in [
            SNOOZE_ITEM_PREFIX,
            "snooze|2h",
            "play_pause",
            "devices|abc",
            "",
        ] {
            assert_eq!(parse_snooze_menu_id(unknown), None, "id {:?}", unknown);
        }
    }

    /// The status line states the remaining time and the local deadline, in
    /// every locale (issue #677 + the #674 table contract).
    #[test]
    fn snooze_status_line_counts_down_in_every_locale() {
        let snooze = TraySnooze {
            status: crate::config::SnoozeStatus {
                // A fixed instant so the rendered `HH:MM` is stable; the local
                // hour depends on the runner's zone, so it is rebuilt here from
                // the same instant rather than hard-coded.
                deadline: chrono::Utc::now() + chrono::TimeDelta::minutes(30),
                remaining_seconds: 30 * 60,
            },
            minutes_left: 30,
        };
        let local = snooze
            .status
            .deadline
            .with_timezone(&chrono::Local)
            .format("%H:%M")
            .to_string();

        assert_eq!(
            snooze_status_line(&EN, &snooze),
            format!("Snoozed — 30 min left (→ {})", local)
        );
        assert_eq!(
            snooze_status_line(&DE, &snooze),
            format!("Sync pausiert — 30 Min. verbleibend (→ {})", local)
        );
        assert_eq!(
            snooze_status_line(&FR, &snooze),
            format!("Synchro en pause — 30 min restant (→ {})", local)
        );
        // The countdown is the ONLY thing the line says — a snoozed session is
        // not syncing, so it must not name a track.
        assert!(!snooze_status_line(&EN, &snooze).contains("Syncing"));
    }

    /// A snooze is resolved from the stored value alone, and everything that is
    /// not a live deadline resolves to "not snoozed" — which is what keeps the
    /// tray and the poller agreeing without a second source of truth.
    #[test]
    fn resolve_snooze_accepts_only_a_live_deadline() {
        assert!(resolve_snooze(None).is_none(), "no config loaded");
        assert!(resolve_snooze(Some(&crate::config::AppConfig::default())).is_none());
        assert!(resolve_snooze(Some(&snoozed_config(&deadline_in(-60)))).is_none());
        assert!(resolve_snooze(Some(&snoozed_config("not a timestamp"))).is_none());

        let live = resolve_snooze(Some(&snoozed_config(&deadline_in(30 * 60))))
            .expect("a future deadline is an active snooze");
        assert_eq!(live.minutes_left, 30, "a fresh 30-minute snooze reads 30");
        assert!(live.status.remaining_seconds > 0);
    }

    /// The dedup key carries the snooze, so a snooze that starts, ends or ticks
    /// over to the next minute repaints instead of early-returning as a no-op
    /// (the countdown would otherwise freeze at the minute the menu was built).
    #[test]
    fn snooze_change_forces_a_tray_rebuild() {
        let _guard = MODE_ATOM_LOCK.lock();
        note_playback_modes(false, RepeatState::Off);
        let track = crate::spotify::TrackInfo {
            title: "Title".to_string(),
            artist: "Artist".to_string(),
            album: "Album".to_string(),
            album_art_url: String::new(),
            is_playing: true,
            progress_ms: None,
            duration_ms: 0,
        };
        let at = |snooze: Option<String>| tray_snapshot_for(true, true, Some(&track), snooze, 0, 0);

        let none = at(None);
        let snoozed = at(Some(snooze_dedup_key(&crate::config::SnoozeStatus {
            deadline: chrono::Utc::now() + chrono::TimeDelta::minutes(30),
            remaining_seconds: 30 * 60,
        })));
        assert!(
            tray_state_changed(Some(&none), &snoozed),
            "starting a snooze must repaint (issue #677)"
        );
        assert!(
            tray_state_changed(Some(&snoozed), &none),
            "ending a snooze must repaint (issue #677)"
        );

        // The minute bucket is part of the key: same deadline, next minute.
        let deadline = chrono::Utc::now() + chrono::TimeDelta::minutes(30);
        let minute_29 = Some(snooze_dedup_key(&crate::config::SnoozeStatus {
            deadline,
            remaining_seconds: 29 * 60,
        }));
        let minute_30 = Some(snooze_dedup_key(&crate::config::SnoozeStatus {
            deadline,
            remaining_seconds: 30 * 60,
        }));
        assert!(
            tray_state_changed(Some(&at(minute_30)), &at(minute_29.clone())),
            "the countdown must move once a minute, not once per snooze"
        );
        assert!(
            !tray_state_changed(Some(&at(minute_29.clone())), &at(minute_29)),
            "a rebuild with nothing changed still dedupes"
        );
    }

    /// A snooze means "no Spotify request", and the tray's own rebuild is one
    /// of the two things that still runs while snoozed — so its fetch mode must
    /// follow the snooze rather than the caller.
    #[test]
    fn snooze_makes_the_tray_rebuild_cache_only() {
        assert_eq!(tray_fetch_mode(true), TrayFetch::CacheOnly);
        assert_eq!(tray_fetch_mode(false), TrayFetch::Refresh);

        // Structural: the rebuild derives its fetch choice from the snooze and
        // hands it to BOTH submenu sources — a literal `Refresh` in either call
        // would re-fetch devices/queue every throttle window while snoozed.
        let prod = prod_source(include_str!("tray.rs"));
        let body = body_of(prod, "fn rebuild_tray_menu(");
        assert!(
            body.contains("paint_fetch_mode(paint, snooze.is_some(), action_refresh_due)"),
            "the rebuild's fetch mode must come from the paint and the snooze"
        );
        // …and the snooze rule itself is unchanged: an ordinary rebuild while
        // snoozed is cache-only (issue #677), while the startup paint is
        // cache-only whatever the snooze says (issue #882).
        let decider = body_of(prod, "fn paint_fetch_mode(");
        assert!(
            decider.contains("tray_fetch_mode(snoozed)"),
            "an ordinary rebuild must take its fetch mode from the snooze (issue #677)"
        );
        for call in [
            "devices_for_menu(access_token.as_deref(), fetch)",
            "queue_for_menu(access_token.as_deref(), fetch)",
        ] {
            assert!(
                body.contains(call),
                "`{}` must honour the snooze-aware fetch mode",
                call
            );
        }
        assert!(
            !body.contains("cached_devices(") && !body.contains("cached_queue("),
            "the rebuild must go through the fetch-mode helpers, not the raw fetchers"
        );
    }

    /// The snooze must be resolved BEFORE the dedup comparison — a key computed
    /// after the early return would never see it.
    #[test]
    fn snooze_is_resolved_before_the_dedup_guard() {
        let prod = prod_source(include_str!("tray.rs"));
        let body = body_of(prod, "fn rebuild_tray_menu(");
        // Issue #886: the snooze comes from a scoped read of the mounted config
        // (`snooze_from_app_state`) instead of a whole-`AppConfig` clone.
        let resolve = body
            .find("snooze_from_app_state(")
            .expect("update_tray_menu must resolve the snooze");
        let snapshot = body
            .find("tray_snapshot_for(")
            .expect("update_tray_menu must build the dedup key");
        let guard = body
            .find("tray_state_changed(")
            .expect("update_tray_menu must keep the dedup guard");
        assert!(
            resolve < snapshot && snapshot < guard,
            "the snooze must be part of the dedup key, evaluated before the guard"
        );
        assert!(
            body.contains("snooze_key"),
            "the dedup key must carry the snooze (deadline + minute bucket)"
        );
    }

    /// A snooze click writes `config.json`, so it must happen off the
    /// menu-event thread, and the repaint must follow the write.
    #[test]
    fn snooze_click_arms_write_off_thread_and_repaint() {
        let prod = prod_source(include_str!("tray.rs"));
        let body = body_of(prod, "pub fn setup_tray(");
        let arm_pos = body
            .find("id if id.starts_with(SNOOZE_ITEM_PREFIX)")
            .expect("setup_tray must handle the snooze submenu ids");
        let arm_end = body[arm_pos..]
            .find("id if id.starts_with(DEVICE_ITEM_PREFIX)")
            .map(|i| arm_pos + i)
            .unwrap_or(body.len());
        let arm = &body[arm_pos..arm_end];
        assert!(
            arm.contains("std::thread::spawn"),
            "the snooze arm must not write config.json on the menu-event thread"
        );
        assert!(
            arm.contains("write_snooze("),
            "the snooze arm must go through the config-writing helper"
        );
        assert!(
            arm.contains("repaint_tray_from_state("),
            "the snooze arm must repaint after the write"
        );
        // The worker must own the id: the menu-event borrow cannot outlive it
        // (a `&str` borrowed from `event` fails to compile in the spawn).
        assert!(
            arm.contains("id.to_string()"),
            "the snooze arm must copy the id before spawning the worker"
        );
        assert!(
            !arm.contains("tokens.spotify()"),
            "the snooze arm must not touch the Spotify token path"
        );
    }

    /// The tray's snooze writer follows the same store-what-was-persisted
    /// discipline as `commands::config::update_config` (issues #297 / #536) — an
    /// in-memory value that disagrees with disk would show a countdown for a
    /// snooze the next launch does not honour.
    #[test]
    fn store_snooze_persists_the_clamped_stamped_value() {
        let prod = prod_source(include_str!("tray.rs"));
        let body = body_of(prod, "fn store_snooze(");
        for marker in [
            "state.config.get_mut()",
            "crate::config::clamped_config(&next)",
            "crate::config::stamp_schema_version(&mut persisted)",
            "crate::config::save_config(&persisted)",
            "*guard = Some(persisted)",
        ] {
            assert!(
                body.contains(marker),
                "store_snooze must contain `{}`",
                marker
            );
        }
        // A failed write must leave the in-memory config alone, so the tray
        // never renders a snooze that is not on disk.
        assert!(
            body.find("save_config(&persisted)?").unwrap_or(usize::MAX)
                < body.find("*guard = Some(persisted)").unwrap_or(0),
            "the store must happen only after a successful save"
        );
        // The write half holds no log lines: the copy belongs to the caller, so
        // the startup cleanup cannot masquerade as a user resume.
        assert!(
            !body.contains("log::info!"),
            "store_snooze must not log — the caller owns the message"
        );
    }

    /// The tray must log what it did: the deadline on the way in and the
    /// explicit resume on the way out (issue #677's logging contract).
    #[test]
    fn write_snooze_logs_both_edges() {
        let prod = prod_source(include_str!("tray.rs"));
        let body = body_of(prod, "fn write_snooze(");
        assert!(
            body.contains("store_snooze(app, until)?"),
            "write_snooze must be the logging wrapper around the writer"
        );
        assert!(
            body.contains("[TRAY] snooze: polling paused until"),
            "the snooze start must be logged with its deadline"
        );
        assert!(
            body.contains("[TRAY] snooze: resumed by the user"),
            "the manual resume must be logged"
        );
    }

    /// A deadline that expired while the app was closed is cleared from
    /// `config.json` at startup, so the clamp log line cannot repeat on every
    /// launch and the persisted document matches what the app honours.
    #[test]
    fn startup_cleanup_clears_an_expired_deadline_through_the_guarded_writer() {
        let prod = prod_source(include_str!("tray.rs"));
        let body = body_of(prod, "fn clear_expired_snooze_at_startup(");
        assert!(
            body.contains("crate::config::snooze_expired_deadline(cfg"),
            "the cleaner must ask the shared predicate whether the deadline is dead"
        );
        assert!(
            body.contains("store_snooze(app, None)"),
            "the cleaner must go through the guarded writer, not write the file itself"
        );
        assert!(
            body.contains("cleared the expired deadline left by the previous session"),
            "the cleanup must be logged, and with its own copy"
        );
        // Wired into startup: a cleaner nothing calls would leave the stale
        // deadline on disk forever.
        let setup = body_of(prod, "pub fn setup_tray(");
        assert!(
            setup.contains("clear_expired_snooze_at_startup(app.handle())"),
            "setup_tray must run the cleanup once, before the first menu build"
        );
        let cleanup = setup
            .find("clear_expired_snooze_at_startup(")
            .expect("call site");
        let menu = setup.find("update_tray_menu_startup(").expect("tray paint");
        assert!(
            cleanup < menu,
            "the cleanup must precede the first menu build, so the tray never \
             renders a state the config no longer holds"
        );
    }

    /// Issue #882: `setup_tray` runs on the main thread inside Tauri's
    /// `setup()`, before the event loop exists, so its paint must not perform
    /// the devices/queue GETs (10 s timeout each, with no window on screen to
    /// explain the wait). It renders the throttled caches and hands the real
    /// fetch to the worker refresh.
    #[test]
    fn startup_paint_is_cache_only_and_fetches_nothing() {
        // The decision itself: the startup paint renders the caches whether or
        // not a snooze is active, while an ordinary rebuild still follows the
        // snooze rule (issue #677).
        assert_eq!(
            paint_fetch_mode(TrayPaint::Startup, false, false),
            TrayFetch::CacheOnly
        );
        assert_eq!(
            paint_fetch_mode(TrayPaint::Startup, true, false),
            TrayFetch::CacheOnly
        );
        assert_eq!(
            paint_fetch_mode(TrayPaint::Deduped, false, false),
            TrayFetch::Refresh
        );
        assert_eq!(
            paint_fetch_mode(TrayPaint::Deduped, true, false),
            TrayFetch::CacheOnly
        );
        // Issue #883: an action makes one rebuild bypass the throttle — unless
        // a snooze is active, which outranks it (issue #677).
        assert_eq!(
            paint_fetch_mode(TrayPaint::Deduped, false, true),
            TrayFetch::RefreshNow
        );
        assert_eq!(
            paint_fetch_mode(TrayPaint::Deduped, true, true),
            TrayFetch::CacheOnly
        );

        // The wiring: setup_tray must paint through the cache-only entry point
        // and must not run a fetching rebuild inline, and the devices/queue
        // fetch must be handed to the off-thread refresh instead.
        let prod = prod_source(include_str!("tray.rs"));
        let setup = body_of(prod, "pub fn setup_tray(");
        assert!(
            setup.contains("update_tray_menu_startup("),
            "setup_tray must paint through the cache-only entry point (issue #882)"
        );
        assert!(
            !setup.contains("update_tray_menu(app.handle()"),
            "setup_tray must not run a fetching rebuild on the setup thread"
        );
        assert!(
            setup.contains("refresh_tray_from_state(app.handle())"),
            "the startup paint must hand the real fetch to the worker refresh"
        );
        // The startup paint must not record the dedup snapshot: it renders the
        // caches only, so recording it would make the first honest rebuild —
        // the worker refresh just spawned — look like a no-op and leave the
        // Devices/Up Next submenus empty.
        let rebuild = body_of(prod, "fn rebuild_tray_menu(");
        assert!(
            rebuild.contains("if paint == TrayPaint::Deduped"),
            "only an ordinary rebuild may commit the dedup snapshot (issue #882)"
        );
    }

    /// Issue #805: the dedup key carries the throttle bucket of both caches.
    /// Without it the early return also skipped the devices/queue fetches, so
    /// a session where nothing else moved showed whatever the last rebuild had
    /// rendered — possibly hours old, listing devices that had long since
    /// disconnected. The bucket makes a quiet session repaint exactly once per
    /// throttle window while every rebuild inside the window still dedupes.
    #[test]
    fn stale_caches_force_a_tray_rebuild_once_per_throttle_window() {
        // The bucket is a pure function of the cache's age, so the window
        // boundary is asserted without sleeping.
        assert_eq!(
            throttle_bucket(None, TRAY_SPOTIFY_FETCH_THROTTLE),
            0,
            "an empty cache must not force a rebuild on its own"
        );
        assert_eq!(
            throttle_bucket(Some(Instant::now()), TRAY_SPOTIFY_FETCH_THROTTLE),
            0
        );
        assert_eq!(
            throttle_bucket(
                Some(Instant::now() - TRAY_SPOTIFY_FETCH_THROTTLE),
                TRAY_SPOTIFY_FETCH_THROTTLE
            ),
            1,
            "a cache exactly one window old is due for a re-fetch"
        );
        assert_eq!(
            throttle_bucket(
                Some(Instant::now() - 10 * TRAY_SPOTIFY_FETCH_THROTTLE),
                TRAY_SPOTIFY_FETCH_THROTTLE
            ),
            10
        );

        let _guard = MODE_ATOM_LOCK.lock();
        note_playback_modes(false, RepeatState::Off);
        let track = crate::spotify::TrackInfo {
            title: "Title".to_string(),
            artist: "Artist".to_string(),
            album: "Album".to_string(),
            album_art_url: String::new(),
            is_playing: true,
            progress_ms: None,
            duration_ms: 0,
        };
        let at = |devices: u64, queue: u64| {
            tray_snapshot_for(true, true, Some(&track), None, devices, queue)
        };

        // Identical track, window, modes and caches: still a no-op.
        assert!(
            !tray_state_changed(Some(&at(0, 0)), &at(0, 0)),
            "a rebuild inside the throttle window must still dedupe"
        );
        // One window later either cache is due, and that must repaint — the
        // repaint is what re-runs the fetchers.
        assert!(
            tray_state_changed(Some(&at(0, 0)), &at(1, 0)),
            "a devices cache one throttle window old must repaint (issue #805)"
        );
        assert!(
            tray_state_changed(Some(&at(0, 0)), &at(0, 1)),
            "a queue cache one throttle window old must repaint (issue #805)"
        );
        assert!(
            !tray_state_changed(Some(&at(1, 1)), &at(1, 1)),
            "the bucket alone must not make every rebuild a repaint"
        );
    }

    /// Issue #956: one rebuild described two different playing states. The
    /// status line and the Play/Pause check mark read the rebuild's own
    /// `is_playing`, while the now-playing row and the tooltip glyph read the
    /// poller's stored `TrackInfo` — which a tray pause does not re-store. So a
    /// pause left the row naming a track that was no longer playing and the
    /// tooltip claiming ▶ over a menu that said paused, until the next poll.
    #[test]
    fn now_playing_row_and_tooltip_follow_the_rebuilds_playing_state() {
        let prod = prod_source(include_str!("tray.rs"));
        let body = body_of(prod, "fn rebuild_tray_menu(");

        // The now-playing row: the gate is the shared binding, placed before
        // the row is built, and nothing in the row re-reads a stored flag.
        let gate = body
            .find("if is_playing {")
            .expect("the now-playing row must be gated on the playing state");
        let row_pos = body
            .find("ID_CURRENT_TRACK")
            .expect("the rebuild must build the now-playing row");
        let row_end = body[row_pos..]
            .find("open_settings")
            .map(|i| row_pos + i)
            .unwrap_or(body.len());
        let row = &body[row_pos..row_end];
        assert!(
            gate < row_pos,
            "the now-playing row must be gated on the rebuild's playing state (issue #956)"
        );
        assert!(
            !row.contains(".is_playing"),
            "the now-playing row must not re-read the stored track's flag (issue #956)"
        );

        // The tooltip glyph: same binding, so a hover agrees with the menu.
        let tip_pos = body
            .find("let track_tooltip")
            .expect("the rebuild must build the tooltip");
        let tip = &body[tip_pos..];
        assert!(
            tip.contains("if is_playing { \"▶\" } else { \"⏸\" }"),
            "the tooltip glyph must come from the rebuild's playing state (issue #956)"
        );
        assert!(
            !tip.contains(".is_playing"),
            "the tooltip must not read the stored track's playing flag (issue #956)"
        );
    }

    /// Issue #971: on Linux Tauri supports neither tray click events nor
    /// `set_tooltip`, so the status summary — which the tooltip carries on
    /// Windows and macOS — must ride as the AppIndicator's title there, or the
    /// sync state, the current track and the snooze countdown are only visible
    /// after opening the menu.
    #[test]
    fn linux_tray_title_carries_the_status_line() {
        let prod = prod_source(include_str!("tray.rs"));
        let body = body_of(prod, "fn rebuild_tray_menu(");
        let tooltip_pos = body
            .find("tray.set_tooltip(")
            .expect("the rebuild must still write the tooltip");
        let after = &body[tooltip_pos..];
        assert!(
            after.contains("#[cfg(target_os = \"linux\")]"),
            "the title mirror must be Linux-only (issue #971)"
        );
        assert!(
            after.contains("tray.set_title(Some(status_line.clone()))"),
            "the Linux title must carry the status line the tooltip carries (issue #971)"
        );

        // The build site names the behaviours that are inert on Linux: the
        // left-click flag, the click handler it belongs to, and the tooltip
        // above. The flag itself must stay (it is what Windows/macOS need).
        let setup = body_of(prod, "pub fn setup_tray(");
        assert!(
            setup.contains(".show_menu_on_left_click(false)"),
            "the tray must keep the documented left-click behaviour (issue #971)"
        );
        assert!(
            setup.contains(".on_tray_icon_event(|tray, event|"),
            "the click handler must stay for Windows and macOS (issue #971)"
        );
    }

    /// Issue #883: every tray player action used to empty both throttled caches,
    /// so each click paid two fresh Spotify fetches and the 60 s throttle that
    /// protects the API was bypassed entirely. The action is now recorded and the
    /// rebuild re-fetches under a short minimum interval, which a burst of clicks
    /// shares instead of multiplying.
    #[test]
    fn post_action_refresh_coalesces_a_burst_of_clicks() {
        let min = TRAY_POST_ACTION_FETCH_MIN;
        let t0 = Instant::now();
        let click = t0 + Duration::from_millis(10);

        // No action: the ordinary throttle decides.
        assert!(!action_fetch_due(None, None, t0, min));
        // An action nothing has fetched for yet is due immediately, and stays
        // due until a fetch is issued on its behalf.
        assert!(action_fetch_due(Some(t0), None, t0, min));
        assert!(action_fetch_due(Some(click), None, click, min));

        // The pair of requests that action asked for, recorded as it is issued.
        let fetched = click + Duration::from_millis(1);
        assert!(
            !action_fetch_due(Some(click), Some(fetched), fetched, min),
            "the fetch already in flight covers the action that asked for it"
        );

        // Ten rapid clicks inside the interval share that single pair.
        let mut pairs = 1;
        for n in 1..10u64 {
            let at = t0 + Duration::from_millis(n * 300);
            if action_fetch_due(Some(at), Some(fetched), at + Duration::from_millis(1), min) {
                pairs += 1;
            }
        }
        assert_eq!(
            pairs, 1,
            "ten clicks inside the interval must share one devices+queue pair (issue #883)"
        );

        // A click that lands after the interval is worth a fresh pair…
        assert!(action_fetch_due(
            Some(t0 + Duration::from_secs(30)),
            Some(fetched),
            fetched + min,
            min
        ));
        // …while a fetch that already followed the action is not repeated.
        assert!(!action_fetch_due(
            Some(click),
            Some(fetched),
            fetched + min,
            min
        ));

        // The wiring: the caches must no longer be emptied on a player action,
        // and the rebuild must pass the action's fetch mode to both submenu
        // sources (it marks the fetch in flight before issuing it).
        let prod = prod_source(include_str!("tray.rs"));
        let force = body_of(prod, "fn force_tray_refresh(");
        assert!(
            force.contains("LAST_TRAY_ACTION.lock() = Some(Instant::now())"),
            "a player action must be recorded, not enforced by emptying the caches"
        );
        assert!(
            !force.contains("DEVICES_CACHE.lock() = None")
                && !force.contains("QUEUE_CACHE.lock() = None"),
            "the caches must stay: emptying them bypassed the fetch throttle (issue #883)"
        );
        let body = body_of(prod, "fn rebuild_tray_menu(");
        let mark = body
            .find("LAST_ACTION_FETCH.lock() = Some(Instant::now())")
            .expect("a post-action rebuild must mark its fetch in flight");
        let devices = body
            .find("devices_for_menu(access_token.as_deref(), fetch)")
            .expect("the rebuild must build the devices submenu from the fetch mode");
        assert!(
            mark < devices,
            "the in-flight mark must be recorded before the requests run (issue #883)"
        );
    }

    /// Issue #886: the poll loop calls the rebuild on every iteration and the
    /// dedup key discards most of those calls — but only after a window query
    /// that blocks the calling thread on an event-loop reply and a clone of the
    /// whole `AppConfig`. The key now reads a visibility mirror and a scoped
    /// snooze read, and the real query sits below the early return.
    #[test]
    fn discarded_rebuilds_query_neither_the_window_nor_a_config_clone() {
        // The mirror is the key's source and round-trips.
        note_window_visibility(true);
        assert!(window_visible(), "the mirror must report what was recorded");
        note_window_visibility(false);
        assert!(!window_visible());

        let prod = prod_source(include_str!("tray.rs"));
        let body = body_of(prod, "fn rebuild_tray_menu(");
        let guard = body
            .find("tray_state_changed(")
            .expect("the rebuild must keep the dedup guard");
        let mirror = body
            .find("window_visible()")
            .expect("the dedup key must read the visibility mirror (issue #886)");
        assert!(
            mirror < guard,
            "the key must be built from the mirror, so a discarded rebuild costs no hop"
        );
        // The real query, and anything else that can block, sits below it.
        let live = body
            .find("live_window_visible(")
            .expect("a repaint must still render the real window state (issue #886)");
        assert!(
            live > guard,
            "the visibility query belongs below the dedup early-return (issue #886)"
        );
        assert!(
            !body.contains("is_visible()"),
            "the rebuild itself must not query the event loop (issue #886)"
        );
        assert!(
            !body.contains("config.get().clone()"),
            "the snooze must not cost a whole-AppConfig clone per poll (issue #886)"
        );
        assert!(
            body.contains("snooze_from_app_state("),
            "the snooze must come from the scoped read (issue #886)"
        );
        // The same scoped read replaces the clone on the forced path.
        let force = body_of(prod, "fn force_tray_refresh(");
        assert!(
            !force.contains("config.get().clone()"),
            "force_tray_refresh must not clone the config either (issue #886)"
        );
        assert!(
            force.contains("snooze_from_app_state("),
            "force_tray_refresh must use the scoped snooze read (issue #886)"
        );
    }

    /// Issue #927: on a host where neither appindicator soname resolves, the tray
    /// build panics inside `libappindicator-sys` (its `Lazy<Library>` dlopens
    /// both and panics when neither is there). `setup_tray` is written to fail as
    /// `Result` — lib.rs logs the error and carries on without a tray — so the
    /// panic has to arrive as that error, and `tray_available()` is what the
    /// close-to-tray guard reads.
    ///
    /// The panic message this test provokes is expected output.
    #[test]
    fn a_missing_tray_library_is_an_error_not_a_panic() {
        let err = guard_tray_panic("tray icon", || -> Result<(), String> {
            panic!("libayatana-appindicator3.so.1: cannot open shared object file")
        })
        .expect_err("a panicking tray build must be reported as an error");
        assert!(
            err.contains("libayatana-appindicator3.so.1"),
            "the error must name the reason, got: {}",
            err
        );
        assert!(err.contains("tray icon"), "the error must name what failed");

        // A normal failure passes through untouched, and a success returns its
        // value — the guard must not swallow either.
        assert_eq!(
            guard_tray_panic("tray icon", || Err::<(), String>("nope".to_string())),
            Err("nope".to_string())
        );
        assert_eq!(guard_tray_panic("tray icon", || Ok(7)), Ok(7));

        // The wiring: the build is the guarded call, and the close-to-tray guard
        // has an accessor to gate on (lib.rs owns that call site).
        let prod = prod_source(include_str!("tray.rs"));
        let setup = body_of(prod, "pub fn setup_tray(");
        assert!(
            setup.contains("guard_tray_panic(\"tray icon\""),
            "the tray build must be guarded (issue #927)"
        );
        assert!(
            setup.contains("builder.build(app)"),
            "the guarded call must be the tray build itself"
        );
        assert!(
            prod.contains("pub fn tray_available()"),
            "close-to-tray needs an accessor for whether a tray exists (issue #927)"
        );
    }
}
