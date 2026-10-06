//! System tray menu (mod.rs keeps the shell; #756 split the concerns out).
use crate::events::PlaybackStateChanged;
use crate::i18n;
use crate::menu::{ID_ABOUT, ID_OPEN_LOGS, ID_QUIT, ID_SETTINGS, ID_SHOW_DASHBOARD, ID_SHOW_LOGS};
use std::borrow::Cow;
use std::sync::atomic::Ordering;
use std::sync::OnceLock;
use std::time::Instant;
use tauri::{
    menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Listener, Manager,
};
pub mod actions;
pub mod cache;
pub mod dedup;
pub mod devices;
pub mod snooze;
pub(crate) mod testkit;
pub use actions::{
    await_sync_toggle, force_tray_refresh, force_tray_refresh_from_app, refresh_tray_for_locale,
    refresh_tray_from_state, repaint_tray_from_state, run_player_action, tray_write_lock,
    DELAYED_REFRESH_IN_FLIGHT, TOGGLE_SETTLE_POLL, TOGGLE_SETTLE_TIMEOUT,
};
pub use cache::{
    action_fetch_due, cache_buckets, cached_devices, cached_queue, devices_for_menu,
    paint_fetch_mode, queue_for_menu, tray_fetch_mode, DeviceCacheSlot, TrayFetch, TrayPaint,
    DEVICES_CACHE, LAST_ACTION_FETCH, LAST_TRAY_ACTION, QUEUE_CACHE, TRAY_POST_ACTION_FETCH_MIN,
    TRAY_SPOTIFY_FETCH_THROTTLE,
};
pub use dedup::{
    consume_playback_state_changed, last_repeat_state, last_tray_state, live_window_visible,
    note_playback_modes, note_playing_state, note_window_visibility, repeat_menu_label,
    shuffle_toggle_target, sync_status_line, tray_snapshot_for, tray_state_changed, window_visible,
    TrayStateSnapshot, LAST_PLAYING_STATE, LAST_REPEAT_STATE, LAST_SHUFFLE_STATE, LAST_TRAY_STATE,
    WINDOW_VISIBLE,
};
pub use devices::{
    build_devices_submenu_from_devices, build_queue_submenu_from_queue, build_seek_submenu,
    build_volume_submenu, parse_device_menu_id, parse_seek_menu_id, parse_volume_menu_id,
    resolve_device_id, selected_for_log, DeviceMenuSelection, SeekMenuSelection,
    VolumeMenuSelection,
};
pub use snooze::{
    build_manual_status_submenu, build_profile_submenu, build_snooze_submenu,
    clear_expired_snooze_at_startup, parse_snooze_menu_id, resolve_snooze, snooze_dedup_key,
    snooze_from_app_state, snooze_status_line, store_active_profile, store_snooze,
    write_active_profile, write_snooze, SnoozeMenuSelection, TraySnooze,
};
// Menu item IDs
const ID_SHOW_HIDE: &str = "show_hide_window";
const ID_PAUSE_SYNC: &str = "pause_sync";
const ID_RESUME_SYNC: &str = "resume_sync";
const ID_CURRENT_TRACK: &str = "current_track";
/// Disabled head-of-menu item that states what the app is actually doing
/// (issue #591) — the Pause/Resume verb alone left sync state unstated, and
/// the presence-gate badge is macOS-only.
const ID_SYNC_STATUS: &str = "sync_status";
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
// snooze always sits beside the way in. Issue #867 adds a fifth id — the
// "until this meeting ends" entry — that is appended only while a busy
// Outlook meeting is currently in progress.
const ID_SNOOZE_30: &str = "snooze|30m";
const ID_SNOOZE_1H: &str = "snooze|1h";
const ID_SNOOZE_TOMORROW: &str = "snooze|tomorrow";
const ID_SNOOZE_NEXT_MEETING: &str = "snooze|next_meeting";
const ID_SNOOZE_RESUME: &str = "snooze|resume";
/// Menu-item id prefix for the snooze submenu. Mirrors the ids above literally
/// (`concat!` cannot take a const); keep both in sync when either changes.
const SNOOZE_ITEM_PREFIX: &str = "snooze|";
const SNOOZE_30_SUFFIX: &str = "30m";
const SNOOZE_1H_SUFFIX: &str = "1h";
const SNOOZE_TOMORROW_SUFFIX: &str = "tomorrow";
const SNOOZE_NEXT_MEETING_SUFFIX: &str = "next_meeting";
const SNOOZE_RESUME_SUFFIX: &str = "resume";

// Issue #870: the "Recent statuses" submenu entries carry the
// `{MANUAL_STATUS_ITEM_PREFIX}|<index>` shape — the index resolves to a
// recent-status entry, and a stale index falls off the back of the ring
// cleanly (the click handler ignores unknown indices). The literal suffixes
// declared below are stable across the GUI and the tray, so a menu rebuilt
// against a stale snapshot cannot dispatch an unknown action.
const MANUAL_STATUS_ITEM_PREFIX: &str = "manualstatus|";
const ID_MANUAL_STATUS_CLEAR: &str = "manualstatus|clear";

// Issue #871: the Volume submenu entries carry `{VOLUME_ITEM_PREFIX}|{n}`
// where `n` is the literal Spotify volume percentage. The Seek submenu
// entries carry `{SEEK_ITEM_PREFIX}|{+|-}{ms}` (the sign is in the id so
// the click handler cannot mistake a seek-back for a seek-forward).
const VOLUME_ITEM_PREFIX: &str = "volume|";
const SEEK_ITEM_PREFIX: &str = "seek|";
const SEEK_BACK_30_MS: u64 = 30_000;
const SEEK_FORWARD_30_MS: u64 = 30_000;

// Issue #869: the presence-profile submenu. The "no profile" entry clears
// the active profile (back to the base configuration); every other entry
// carries `{PROFILE_ITEM_PREFIX}|{profile name}` and the click handler
// resolves the name against the clamped list. Names are unique and ≤ 32
// chars after `clamp_presence_profiles`, so the id dispatch never has to
// disambiguate.
const ID_PROFILE_BASE: &str = "profile|base";
const PROFILE_ITEM_PREFIX: &str = "profile|";

/// A safe diagnostic label for a native menu event id.
///
/// Fixed action ids remain readable so support diagnostics keep their useful
/// dispatch context. Ids carrying a menu payload expose only the stable prefix
/// and suffix length; unknown fixed ids expose only their length. This follows
/// the device-selection redaction discipline and keeps bearer-adjacent Spotify
/// device ids out of every dispatcher log line.
pub(crate) fn menu_event_id_for_log(id: &str) -> Cow<'_, str> {
    if matches!(
        id,
        ID_SHOW_HIDE
            | ID_PAUSE_SYNC
            | ID_RESUME_SYNC
            | ID_CURRENT_TRACK
            | ID_SYNC_STATUS
            | ID_PLAY_PAUSE
            | ID_PREVIOUS
            | ID_NEXT
            | ID_SHUFFLE
            | ID_REPEAT
            | ID_QUIT
            | ID_DEVICES
            | ID_QUEUE
            | ID_SETTINGS
            | ID_OPEN_LOGS
            | ID_SHOW_DASHBOARD
            | ID_SHOW_LOGS
            | ID_ABOUT
    ) {
        return Cow::Borrowed(id);
    }

    match id.split_once('|') {
        Some((prefix, suffix)) => Cow::Owned(format!("<{prefix} len={}>", suffix.len())),
        None => Cow::Owned(format!("<unknown len={}>", id.len())),
    }
}

pub(crate) fn log_dispatched_menu_event_with(id: &str, emit: impl FnOnce(&log::Record<'_>)) {
    let redacted = menu_event_id_for_log(id);
    let message = format!("[TRAY] menu event: id={redacted}");
    let args = format_args!("{message}");
    let record = log::Record::builder()
        .args(args)
        .level(log::Level::Info)
        .target(module_path!())
        .build();
    emit(&record);
}

pub(crate) fn log_dispatched_menu_event(id: &str) {
    let level = log::Level::Info;
    if level > log::STATIC_MAX_LEVEL || level > log::max_level() {
        return;
    }
    let metadata = log::Metadata::builder()
        .level(level)
        .target(module_path!())
        .build();
    if !log::logger().enabled(&metadata) {
        return;
    }
    log_dispatched_menu_event_with(id, |record| log::Log::log(log::logger(), record));
}

pub(crate) static TRAY: OnceLock<TrayIcon> = OnceLock::new();

/// Get the global TrayIcon instance.
pub fn get_tray() -> Option<&'static TrayIcon> {
    TRAY.get()
}

/// Single dispatcher for every native menu click (issue #804).
///
/// The tray builder's `on_menu_event` is a *global* listener — Tauri calls it
/// for window-menu events too — so this one function owns every tray-built id
/// and delegates anything else (the app-menu-only ids) to
/// [`crate::menu::handle_app_menu_event`]. It is registered exactly once (on
/// the tray builder in [`setup_tray`]): one click then logs and emits exactly
/// once, and tray-only ids never reach the app-menu handler's unknown-event log.
/// The second `window.on_menu_event` registration lib.rs used to carry
/// double-fired every shared id and is gone.
pub fn handle_menu_event(app: &AppHandle, id: &str) {
    log_dispatched_menu_event(id);
    match id {
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
                .is_syncing();
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
        // Shared with the app menu (issue #804): the tray builder's listener is
        // global, so this arm owns every `settings` click from either surface.
        ID_SETTINGS => {
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
                    Ok(crate::spotify::CurrentlyPlaying::Modified { now: Some(now), .. }) => {
                        now.media.is_playing
                    }
                    Ok(_) => false,
                    Err(e) => {
                        log::warn!("[TRAY] play/pause: playback state read failed: {}", e);
                        let _ = app_handle.emit("playback-error", e.to_string());
                        return;
                    }
                };
                if should_pause {
                    run_player_action(&app_handle, "pause", Some(false), None, |t| {
                        crate::spotify::player_pause(t, None)
                    });
                } else {
                    run_player_action(&app_handle, "play", Some(true), None, |t| {
                        crate::spotify::player_play(t, None)
                    });
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
                        // Issue #867: the meeting-bound preset needs the
                        // calendar cache at click time. We capture it here
                        // (no fetch) — the entry is only built while a
                        // meeting is active, so the cache is fresh
                        // enough.
                        let next_meeting_end =
                            if preset == crate::config::SnoozePreset::UntilNextMeetingEnds {
                                let state = app_handle.state::<std::sync::Arc<crate::AppState>>();
                                state.calendar.current_meeting_end(now_utc)
                            } else {
                                None
                            };
                        let deadline = crate::config::snooze_preset_deadline(
                            preset,
                            now_utc,
                            chrono::Local::now(),
                            next_meeting_end,
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
                        log::warn!(
                            "[TRAY] snooze: unrecognized menu id '{}'",
                            menu_event_id_for_log(&raw)
                        );
                    }
                }
                repaint_tray_from_state(&app_handle, "snooze");
            });
        }
        id if id == ID_PROFILE_BASE || id.starts_with(PROFILE_ITEM_PREFIX) => {
            // Issue #869: a profile click writes `config.json` (atomic
            // write + fsync), so it must run off the menu-event thread
            // like the snooze arm above — a slow disk would otherwise
            // wedge the native menu. The id dispatch never has to
            // disambiguate names because `clamp_presence_profiles`
            // dedupes and rejects pipes at load/save, so the
            // `{PROFILE_ITEM_PREFIX}|{name}` format is unambiguous.
            //
            // The id is copied out of the borrowed `event` before the
            // worker takes ownership (it is used in the unknown-name
            // warn and the profile-not-found path).
            let raw = id.to_string();
            let app_handle = app.clone();
            std::thread::spawn(move || {
                // Resolve the picked name from the stored config.
                // Reading state.config here (off the menu-event thread)
                // keeps the click handler off the config lock.
                let state = app_handle.state::<std::sync::Arc<crate::AppState>>();
                let target: Option<String> = if raw == ID_PROFILE_BASE {
                    None
                } else if let Some(stripped) = raw.strip_prefix(PROFILE_ITEM_PREFIX) {
                    // Skip the disabled empty placeholder.
                    if stripped == "empty" {
                        repaint_tray_from_state(&app_handle, "profile (empty placeholder)");
                        return;
                    }
                    // Validate against the clamped list — a name that
                    // was deleted between the rebuild and the click
                    // (or a stale menu from a previous build) must
                    // fall back to base, not silently land on a
                    // phantom id.
                    let exists =
                        state.config.get().as_ref().is_some_and(|c| {
                            c.presence_profiles.iter().any(|p| p.name == stripped)
                        });
                    if !exists {
                        log::warn!(
                            "[TRAY] profile: {:?} not found — falling back to base",
                            menu_event_id_for_log(&raw)
                        );
                        None
                    } else {
                        Some(stripped.to_string())
                    }
                } else {
                    log::warn!(
                        "[TRAY] profile: unrecognized menu id '{}'",
                        menu_event_id_for_log(&raw)
                    );
                    return;
                };
                if let Err(e) = write_active_profile(&app_handle, target) {
                    log::error!("[TRAY] profile: could not persist the switch: {}", e);
                }
                repaint_tray_from_state(&app_handle, "profile");
            });
        }
        id if id == ID_MANUAL_STATUS_CLEAR => {
            // Issue #870: the tray's "Clear manual status" entry. Routes
            // through the same `clear_manual_status_inner` helper the
            // Dashboard composer uses, on a worker thread — the Graph
            // POST + the persisted record reset must not run on the
            // menu-event thread.
            let app_handle = app.clone();
            std::thread::spawn(move || {
                let state = app_handle.state::<std::sync::Arc<crate::AppState>>();
                if let Err(e) =
                    crate::commands::status::clear_manual_status_inner(state.inner(), &app_handle)
                {
                    log::error!("[TRAY] manual status clear: {}", e);
                }
                repaint_tray_from_state(&app_handle, "manual status clear");
            });
        }
        id if id.starts_with(MANUAL_STATUS_ITEM_PREFIX) => {
            // Issue #870: a "Recent statuses" pick. The trailing index
            // resolves to one of the ring's slots; a stale index (the
            // ring rotated since the menu was built) is logged and
            // ignored. The pick carries the user's text verbatim, so the
            // Dashboard composer and the tray share the same store.
            let raw = id.to_string();
            let index: usize = raw
                .strip_prefix(MANUAL_STATUS_ITEM_PREFIX)
                .and_then(|s| s.parse().ok())
                .unwrap_or(99);
            let recent = crate::commands::status::load_recent_statuses();
            let picked = recent.get(index).cloned();
            let app_handle = app.clone();
            std::thread::spawn(move || {
                let Some(entry) = picked else {
                    log::warn!(
                        "[TRAY] manual status pick: stale index {index}; the recent ring rotated"
                    );
                    repaint_tray_from_state(&app_handle, "manual status pick (stale)");
                    return;
                };
                let state = app_handle.state::<std::sync::Arc<crate::AppState>>();
                let expiry = crate::commands::status::clamp_expiry_public(60);
                if let Err(e) = crate::commands::status::set_manual_status_inner(
                    state.inner(),
                    &app_handle,
                    &entry.message,
                    expiry,
                ) {
                    log::error!("[TRAY] manual status pick: {}", e);
                }
                repaint_tray_from_state(&app_handle, "manual status pick");
            });
        }
        id if id.starts_with(VOLUME_ITEM_PREFIX) => {
            // Issue #871: the tray's Volume submenu picked a percentage.
            // The click handler offloads the Spotify HTTP and records the
            // new volume so the Dashboard slider mirrors the new value
            // on its next SyncStatus fetch. A stale id (an old build's
            // menu) is logged and ignored.
            let raw = id.to_string();
            let selection = parse_volume_menu_id(&raw);
            let app_handle = app.clone();
            std::thread::spawn(move || {
                let Some(VolumeMenuSelection::Percent(percent)) = selection else {
                    log::warn!(
                        "[TRAY] volume: unrecognized menu id '{}'",
                        menu_event_id_for_log(&raw)
                    );
                    repaint_tray_from_state(&app_handle, "volume (stale)");
                    return;
                };
                run_player_action(&app_handle, "volume", None, None, |token| {
                    crate::spotify::player_set_volume(token, percent, None)
                });
            });
        }
        id if id.starts_with(SEEK_ITEM_PREFIX) => {
            // Issue #871: the tray's Seek submenu picked a delta. The
            // click handler reads the stored progress + duration from
            // the AppState, computes the new position, and dispatches
            // through `commands/playback::seek` so the same token
            // refresh + retry-once policy applies. A stale id (an old
            // build's menu) is logged and ignored.
            let raw = id.to_string();
            let selection = parse_seek_menu_id(&raw);
            let app_handle = app.clone();
            std::thread::spawn(move || {
                let Some(SeekMenuSelection::Delta(delta)) = selection else {
                    log::warn!(
                        "[TRAY] seek: unrecognized menu id '{}'",
                        menu_event_id_for_log(&raw)
                    );
                    repaint_tray_from_state(&app_handle, "seek (stale)");
                    return;
                };
                let state = app_handle.state::<std::sync::Arc<crate::AppState>>();
                let current = state
                    .polling
                    .current_track()
                    .as_ref()
                    .map(|t| (t.progress_ms, t.duration_ms));
                let Some((progress, duration)) = current else {
                    log::warn!("[TRAY] seek: no current track, cannot seek");
                    repaint_tray_from_state(&app_handle, "seek (no track)");
                    return;
                };
                // Spotify reports `progress_ms` as `Option` (issue #3.0-P3
                // — the documented "Can be `null`"); the duration is
                // always present. A missing `progress` defaults to 0 so
                // the user still gets a forward-30s jump from the
                // beginning of the track.
                let progress = progress.unwrap_or(0);
                let new_position = if delta >= 0 {
                    progress.saturating_add(delta as u64).min(duration)
                } else {
                    progress.saturating_sub((-delta) as u64)
                };
                run_player_action(&app_handle, "seek", None, None, |token| {
                    crate::spotify::player_seek(token, new_position as i64, None)
                });
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
        _ => crate::menu::handle_app_menu_event(app, id),
    }
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
        // Issue #911: macOS wants a monochrome TEMPLATE image (marked as one
        // right after the build below); Windows and Linux keep the
        // application icon.
        .icon(tray_icon(app)?)
        // Issue #971: Tauri documents this flag as unsupported on Linux, where
        // a left click opens the AppIndicator menu unconditionally. It only
        // ever changes behaviour on Windows and macOS.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| handle_menu_event(app, event.id().as_ref()))
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
        });
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

    // Issue #911: mark the menu-bar icon as a template, so macOS draws it from
    // its alpha channel and tints it — black stays black on a light menu bar and
    // inverts on a dark one, and it dims with the bar for a modal. The call is a
    // no-op off macOS (Tauri only implements it there), so it stays gated.
    #[cfg(target_os = "macos")]
    if let Err(e) = tray.set_icon_as_template(true) {
        log::warn!(
            "[TRAY] setup_tray: failed to mark the menu-bar icon as a template: {}",
            e
        );
    }

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
    let is_syncing = state.polling.is_syncing();
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
pub(crate) fn update_tray_menu_startup(
    app: &AppHandle,
    is_syncing: bool,
    current_track: Option<crate::spotify::TrackInfo>,
) -> Result<(), String> {
    rebuild_tray_menu(app, is_syncing, current_track.as_ref(), TrayPaint::Startup)
}

/// The rebuild both entry points above share, so the tray has one layout.
pub(crate) fn rebuild_tray_menu(
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
    // Issue #869: the active profile id rides the dedup key so a switch —
    // tray click, CLI, or Settings card — repaints the submenu's check
    // mark. Cloned out of the short-lived read guard because the snapshot
    // outlives this scope (the dedup is the next call's read source).
    let active_profile_key = state
        .config
        .get()
        .as_ref()
        .and_then(|c| c.active_profile.clone());
    let snapshot = tray_snapshot_for(
        is_syncing,
        key_visible,
        current_track,
        snooze_key,
        devices_bucket,
        queue_bucket,
        active_profile_key,
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
    // Issue #871: the active device's capability flags gate the playback
    // submenu — Volume disabled when `actions.setting_volume` is false,
    // Seek disabled when `actions.seeking` is false, Shuffle disabled when
    // the device is restricted or `actions.toggling_shuffle` is false,
    // Repeat the same. A device list that lacks the active device (e.g.
    // a freshly-restarted Spotify session that has not yet published
    // devices) returns `None`; the playback submenu then disables the
    // capability-gated items by default so a stale-capabilities click
    // cannot reach Graph with a 403.
    let active_capabilities = devices
        .iter()
        .find(|d| d.is_active)
        .map(|d| (d.supports_volume, d.is_restricted, d.actions.clone()));
    let (active_supports_volume, active_is_restricted, active_actions) =
        active_capabilities.unwrap_or((false, true, crate::spotify::DeviceActions::default()));

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

    let open_settings = MenuItemBuilder::with_id(ID_SETTINGS, s.open_settings)
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
    // Issue #871: the playback items gate on the active device's documented
    // capabilities. The Dashboard composer and the slider share the same
    // gate (`commands/playback::set_volume` / `seek` refuse to issue the
    // Graph call when the device refuses it; here the tray items are
    // disabled for the same reason so a stale-capabilities click never
    // even reaches the click handler).
    let play_pause = CheckMenuItemBuilder::with_id(ID_PLAY_PAUSE, s.play_pause)
        .checked(is_playing)
        .enabled(active_actions.resuming || active_actions.pausing)
        .build(app)
        .map_err(|e| {
            log::warn!(
                "[TRAY] update_tray_menu: failed to build play_pause item: {}",
                e
            );
            e.to_string()
        })?;
    let previous = MenuItemBuilder::with_id(ID_PREVIOUS, s.previous)
        .enabled(!active_is_restricted && active_actions.skipping_prev)
        .build(app)
        .map_err(|e| {
            log::warn!(
                "[TRAY] update_tray_menu: failed to build previous item: {}",
                e
            );
            e.to_string()
        })?;
    let next = MenuItemBuilder::with_id(ID_NEXT, s.next)
        .enabled(!active_is_restricted && active_actions.skipping_next)
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
    //
    // Issue #871: the existing `is_restricted` gate stays; the Spotify
    // capability flags are the documented twin (issue #871 — Spotify
    // documents both as sources of truth for "can I do this?"; the
    // capability flag wins when the device reports it).
    let shuffle = CheckMenuItemBuilder::with_id(ID_SHUFFLE, s.shuffle)
        .checked(LAST_SHUFFLE_STATE.load(Ordering::Acquire))
        .enabled(!active_is_restricted && active_actions.toggling_shuffle)
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
        .enabled(
            !active_is_restricted
                && (active_actions.toggling_repeat_context || active_actions.toggling_repeat_track),
        )
        .build(app)
        .map_err(|e| {
            log::warn!(
                "[TRAY] update_tray_menu: failed to build repeat item: {}",
                e
            );
            e.to_string()
        })?;
    // Issue #871: the Volume submenu. The 0/25/50/75/100 picks are the
    // documented Spotify boundaries (`PUT /me/player/volume` clamps to
    // 0..=100). Disabled when the active device refuses the volume
    // capability — `supports_volume` is the older twin, the new
    // `actions.setting_volume` flag is the documented source of truth.
    let volume_submenu = build_volume_submenu(
        app,
        s,
        active_supports_volume && active_actions.setting_volume,
    )
    .map_err(|e| {
        log::warn!(
            "[TRAY] update_tray_menu: failed to build volume submenu: {}",
            e
        );
        e.to_string()
    })?;
    // Issue #871: the Seek submenu. The +/- 30 s picks are the documented
    // surface for `PUT /me/player/seek`; the Dashboard's click-to-seek
    // progress bar reaches the same endpoint through
    // `commands/playback::seek`. Disabled when the device refuses the
    // seek capability.
    let seek_submenu = build_seek_submenu(app, s, !active_is_restricted && active_actions.seeking)
        .map_err(|e| {
            log::warn!(
                "[TRAY] update_tray_menu: failed to build seek submenu: {}",
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
    // Issue #870: the "Recent statuses" submenu. The recent ring is a
    // process-global; this build call is the only thing that turns it into a
    // tray surface, and the `manualstatus|clear` entry only appears while a
    // manual status is armed. The Dashboard composer reads the same ring,
    // so a click on the tray's pick hits the same record the Dashboard would.
    let manual_status_submenu = build_manual_status_submenu(
        app,
        s,
        &crate::commands::status::load_recent_statuses(),
        crate::commands::status::load_manual_status().as_ref(),
    )
    .map_err(|e| {
        log::warn!(
            "[TRAY] update_tray_menu: failed to build manual-status submenu: {}",
            e
        );
        e
    })?;
    // Issue #869: the "Active profile" submenu. Sits next to the snooze
    // items because both are runtime state changes — the spec asks for
    // the picker beside the snooze items, not under Settings, so the
    // user can flip overlays without leaving the tray.
    let profile_submenu = {
        let profiles: Vec<crate::config::PresenceProfile> = state
            .config
            .get()
            .as_ref()
            .map(|c| c.presence_profiles.clone())
            .unwrap_or_default();
        let active = state
            .config
            .get()
            .as_ref()
            .and_then(|c| c.active_profile.clone());
        build_profile_submenu(app, s, &profiles, active.as_deref())
    }
    .map_err(|e| {
        log::warn!(
            "[TRAY] update_tray_menu: failed to build profile submenu: {}",
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
            &profile_submenu,
            &manual_status_submenu,
            &separator1,
        ])
        .items(&[
            &play_pause,
            &previous,
            &next,
            &shuffle,
            &repeat,
            &volume_submenu,
            &seek_submenu,
        ])
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
pub(crate) fn guard_tray_panic<T>(
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

/// Side of [`TEMPLATE_GLYPH`] in cells (issue #911). macOS sizes menu-bar items
/// at 22 pt, so the glyph is drawn as a 22-cell square and doubled for `@2x`.
#[cfg(any(target_os = "macos", test))]
pub(crate) const TEMPLATE_GLYPH_SIZE: u32 = 22;

/// The menu-bar glyph: a quarter note, `#` inked and `.` transparent.
///
/// A bitmap in the source rather than a shipped PNG: the tray then needs no
/// macOS-only asset and no PNG-decoding feature to read one, and the shape is
/// pure data that every platform's test run can check. The stem starts at the
/// top-right and runs down into the note head, with the flag off its top.
#[cfg(any(target_os = "macos", test))]
pub(crate) const TEMPLATE_GLYPH: [&str; TEMPLATE_GLYPH_SIZE as usize] = [
    "......................",
    "......................",
    "......................",
    "......................",
    "...........#####......",
    "...........######.....",
    "...........######.....",
    "...........######.....",
    "...........##.........",
    "...........##.........",
    "...........##.........",
    "...........##.........",
    ".......#...##.........",
    ".....#####.##.........",
    "....#########.........",
    "....#########.........",
    "...##########.........",
    "....#########.........",
    "....#########.........",
    ".....#####.##.........",
    ".......#...##.........",
    "......................",
];

/// The RGBA bytes for [`TEMPLATE_GLYPH`] at `scale` (issue #911): opaque black
/// for ink, fully transparent elsewhere.
///
/// A template image carries no colour — macOS reads the alpha channel and lets
/// the menu bar tint the result — so the colour channels are zero everywhere and
/// only the alpha distinguishes ink from background. Each cell becomes a
/// `scale`×`scale` block, which is how the `@2x` twin is produced.
#[cfg(any(target_os = "macos", test))]
pub(crate) fn template_icon_rgba(scale: u32) -> Vec<u8> {
    let scale = scale.max(1);
    let side = TEMPLATE_GLYPH_SIZE * scale;
    let mut rgba = Vec::with_capacity((side * side * 4) as usize);
    for row in TEMPLATE_GLYPH {
        for _ in 0..scale {
            for cell in row.bytes() {
                let alpha = if cell == b'#' { 255u8 } else { 0u8 };
                for _ in 0..scale {
                    rgba.extend_from_slice(&[0, 0, 0, alpha]);
                }
            }
        }
    }
    rgba
}

/// The macOS menu-bar icon (issue #911): [`TEMPLATE_GLYPH`] at its `@2x` size, so
/// the same image is sharp on a Retina bar without a second asset.
#[cfg(target_os = "macos")]
pub(crate) fn macos_template_icon() -> tauri::image::Image<'static> {
    const SCALE: u32 = 2;
    let side = TEMPLATE_GLYPH_SIZE * SCALE;
    let rgba = template_icon_rgba(SCALE);
    debug_assert_eq!(rgba.len(), (side * side * 4) as usize);
    tauri::image::Image::new_owned(rgba, side, side)
}

/// The tray icon this platform wants (issue #911).
///
/// macOS gets the monochrome template glyph — the full-colour application icon
/// (`default_window_icon()`, 32/128 px) is what the menu bar rendered oversized
/// and untinted. Windows' notification area and Linux's indicators are unaffected
/// by the missing template flag, so they keep that icon.
pub(crate) fn tray_icon(app: &tauri::App) -> Result<tauri::image::Image<'_>, String> {
    #[cfg(target_os = "macos")]
    {
        Ok(macos_template_icon())
    }
    #[cfg(not(target_os = "macos"))]
    {
        app.default_window_icon()
            .cloned()
            .ok_or_else(|| "No default icon".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tray::testkit::{body_of, prod_source, tray_prod_source};
    /// Issue #918: the dispatcher's normal log must preserve known action ids
    /// while reducing payload-bearing and unknown ids to their shape.
    #[test]
    fn menu_event_log_redacts_payload_and_unknown_ids() {
        assert_eq!(menu_event_id_for_log("play_pause"), "play_pause");
        assert_eq!(menu_event_id_for_log("about"), "about");

        let device_id = "aB3deviceCredentialValueWithThirtyTwoChars";
        let formatted_device_id = format!("devices|{device_id}");
        let logged_device = menu_event_id_for_log(&formatted_device_id);
        assert_eq!(logged_device, "<devices len=42>");
        assert!(!logged_device.contains(device_id));
        assert!(!logged_device.contains(&format!("devices|{device_id}")));

        let logged_unknown = menu_event_id_for_log("mystery");
        assert_eq!(logged_unknown, "<unknown len=7>");
        assert!(!logged_unknown.contains("mystery"));
    }
    /// Issue #918: capture the production global-dispatcher record, then bind
    /// the real handler to that seam. A raw-id call at the handler is therefore
    /// a regression even when the formatter itself still behaves correctly.
    #[test]
    fn dispatched_menu_event_record_redacts_device_id() {
        let device_id = "aB3deviceCredentialValueWithThirtyTwoChars";
        let event_id = format!("devices|{device_id}");
        let mut captured = None;

        log_dispatched_menu_event_with(&event_id, |record| {
            assert_eq!(record.level(), log::Level::Info);
            captured = Some(record.args().to_string());
        });

        let record = captured.expect("the dispatcher seam must emit one record");
        assert!(
            record.contains("[TRAY] menu event: id="),
            "the production record shape changed: {record}"
        );
        assert!(!record.contains(device_id), "device id leaked: {record}");
        assert!(
            !record.contains(&event_id),
            "device menu id leaked: {record}"
        );

        let mut known_record = None;
        log_dispatched_menu_event_with("play_pause", |record| {
            known_record = Some(record.args().to_string());
        });
        assert_eq!(
            known_record.as_deref(),
            Some("[TRAY] menu event: id=play_pause")
        );

        let prod = tray_prod_source();
        let dispatcher = body_of(prod, "pub fn handle_menu_event(");
        assert!(
            dispatcher.contains("log_dispatched_menu_event(id);"),
            "handle_menu_event must emit through the captured production seam"
        );
        let wrapper = body_of(prod, "fn log_dispatched_menu_event(");
        assert!(
            wrapper.contains("log_dispatched_menu_event_with(id,"),
            "the production wrapper must delegate to the captured record seam"
        );
    }
    /// Issues #383/#386: the tray Quit arm must terminate via the shared
    /// graceful shutdown, and the playback/device click arms must offload
    /// blocking Spotify HTTP onto worker threads. Issue #587: the Show/Hide
    /// repaint rebuilds a menu that can fetch Spotify over the network, so
    /// it must be offloaded too.
    #[test]
    fn tray_click_arms_quit_and_offload() {
        let src = tray_prod_source();
        let body = body_of(prod_source(src), "pub fn handle_menu_event(");
        // #383: Quit terminates even with no frontend listener.
        let quit_pos = body
            .find("ID_QUIT =>")
            .expect("handle_menu_event must handle ID_QUIT");
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
            .expect("handle_menu_event must handle ID_SHOW_HIDE");
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
            .expect("handle_menu_event must handle ID_PAUSE_SYNC | ID_RESUME_SYNC");
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
    /// Issue #804: every menu click must fire exactly once. The tray
    /// builder's `on_menu_event` is a global listener (it sees window-menu
    /// events too), so the `window.on_menu_event` registration lib.rs carried
    /// double-fired every shared id — and tray-only ids fell into menu.rs's
    /// unknown-event warn. The single dispatcher `handle_menu_event` owns
    /// every tray id and delegates the app-menu-only ids to
    /// `menu::handle_app_menu_event`, which keeps only those three arms.
    /// Fails pre-fix with twins (and with the second lib.rs registration).
    #[test]
    fn menu_events_route_through_one_dispatcher() {
        let tray_prod = tray_prod_source();
        let menu_prod = prod_source(include_str!("../menu.rs"));
        let app_prod = prod_source(include_str!("../app.rs"));
        // One registration: the tray builder's global listener delegating to
        // the named dispatcher; no per-window handler in app.rs.
        assert!(
            tray_prod.contains(
                ".on_menu_event(|app, event| handle_menu_event(app, event.id().as_ref()))"
            ),
            "setup_tray must register the single dispatcher, not an inline match"
        );
        assert!(
            !app_prod.contains("window.on_menu_event"),
            "app.rs must not register a second menu handler (issue #804: double-fire)"
        );
        let dispatcher = body_of(tray_prod, "pub fn handle_menu_event(");
        // The dispatcher owns every tray-built id ...
        for marker in [
            "ID_SHOW_HIDE =>",
            "ID_PAUSE_SYNC | ID_RESUME_SYNC =>",
            "ID_QUIT =>",
            "ID_SETTINGS =>",
            "ID_OPEN_LOGS =>",
            "ID_PLAY_PAUSE =>",
            "ID_PREVIOUS =>",
            "ID_NEXT =>",
            "ID_SHUFFLE =>",
            "ID_REPEAT =>",
            "SNOOZE_ITEM_PREFIX",
            "ID_PROFILE_BASE",
            "PROFILE_ITEM_PREFIX",
            "ID_MANUAL_STATUS_CLEAR",
            "MANUAL_STATUS_ITEM_PREFIX",
            "VOLUME_ITEM_PREFIX",
            "SEEK_ITEM_PREFIX",
            "DEVICE_ITEM_PREFIX",
        ] {
            assert!(
                dispatcher.contains(marker),
                "handle_menu_event must own `{}`",
                marker
            );
        }
        // ... and hands anything else to the app-menu handler.
        assert!(
            dispatcher.contains("crate::menu::handle_app_menu_event(app, id)"),
            "handle_menu_event must delegate app-menu-only ids to menu::handle_app_menu_event"
        );
        // setup_tray itself carries no match arms any more.
        let setup = body_of(tray_prod, "pub fn setup_tray(");
        assert!(
            !setup.contains("ID_QUIT =>"),
            "setup_tray must not keep a second copy of the dispatch arms"
        );
        // The app-menu handler keeps only its three window-menu-only arms, so
        // no id is owned twice and tray-only ids never hit its unknown warn.
        let app_menu = body_of(menu_prod, "pub(crate) fn handle_app_menu_event(");
        for marker in ["ID_SHOW_DASHBOARD =>", "ID_SHOW_LOGS =>", "ID_ABOUT =>"] {
            assert!(
                app_menu.contains(marker),
                "handle_app_menu_event must keep `{}`",
                marker
            );
        }
        // NOTE: `app_menu` is the handler body only (not the whole file),
        // so these assertions cannot match the test module's own prose below.
        for marker in ["ID_SETTINGS =>", "ID_OPEN_LOGS =>", "ID_QUIT =>"] {
            assert!(
                !app_menu.contains(marker),
                "handle_app_menu_event must not twin `{}` (issue #804)",
                marker
            );
        }
    }
    /// Issue #689 (D6): the tray's belief only moves if the poller's
    /// `playback-state-changed` event is actually subscribed. A mistyped or
    /// deleted `app.listen` would leave the functional test above green — it
    /// calls the consumer directly — while the running app ignored the event.
    /// Guards the registration inside `setup_tray`, like the click-arm scans.
    #[test]
    fn setup_tray_subscribes_to_playback_state_changes() {
        let src = tray_prod_source();
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
        let prod = tray_prod_source();
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
    /// Issue #956: one rebuild described two different playing states. The
    /// status line and the Play/Pause check mark read the rebuild's own
    /// `is_playing`, while the now-playing row and the tooltip glyph read the
    /// poller's stored `TrackInfo` — which a tray pause does not re-store. So a
    /// pause left the row naming a track that was no longer playing and the
    /// tooltip claiming ▶ over a menu that said paused, until the next poll.
    #[test]
    fn now_playing_row_and_tooltip_follow_the_rebuilds_playing_state() {
        let prod = tray_prod_source();
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
        let prod = tray_prod_source();
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

        let prod = tray_prod_source();
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
        let prod = tray_prod_source();
        let setup = body_of(prod, "pub fn setup_tray(");
        assert!(
            setup.contains("guard_tray_panic(") && setup.contains("\"tray icon\""),
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
    /// Issue #911: macOS wants a monochrome TEMPLATE image for the menu bar,
    /// because the bar draws the icon from its alpha channel and tints it — that
    /// is what makes the item invert in a dark bar and dim with the bar for a
    /// modal. The tray used `default_window_icon()`, the full-colour 32/128 px
    /// application icon, which is why the item was oversized and ignored the tint.
    #[test]
    fn the_menu_bar_icon_is_a_monochrome_template_at_menu_bar_size() {
        // The glyph is a square bitmap of ink and transparency, and it is a mark
        // rather than a stray pixel.
        assert_eq!(TEMPLATE_GLYPH.len(), TEMPLATE_GLYPH_SIZE as usize);
        for row in TEMPLATE_GLYPH {
            assert_eq!(row.len(), TEMPLATE_GLYPH_SIZE as usize, "row {:?}", row);
            assert!(
                row.bytes().all(|b| b == b'#' || b == b'.'),
                "row {:?} must be ink or transparency only",
                row
            );
        }
        let ink = TEMPLATE_GLYPH
            .iter()
            .flat_map(|row| row.bytes())
            .filter(|b| *b == b'#')
            .count();
        assert!(
            ink > 60,
            "the glyph must be a legible mark, got {} cells",
            ink
        );

        // The RGBA handed to macOS is black plus alpha at both menu-bar sizes,
        // and the ink covers exactly the glyph's cells scaled up — a template
        // image gets its colour from the menu bar, so a coloured pixel would be
        // ignored there anyway.
        for scale in [1u32, 2] {
            let side = TEMPLATE_GLYPH_SIZE * scale;
            let rgba = template_icon_rgba(scale);
            assert_eq!(rgba.len(), (side * side * 4) as usize, "scale {}", scale);
            for px in rgba.chunks_exact(4) {
                assert_eq!(&px[..3], &[0, 0, 0], "a template image is black");
                assert!(px[3] == 0 || px[3] == 255, "alpha is on or off");
            }
            assert_eq!(
                rgba.chunks_exact(4).filter(|px| px[3] == 255).count(),
                ink * (scale * scale) as usize,
                "scale {} must cover the glyph and nothing else",
                scale
            );
        }

        // The wiring: macOS builds the tray from the template glyph and marks it
        // as a template; Windows and Linux keep the application icon.
        let prod = tray_prod_source();
        let setup = body_of(prod, "pub fn setup_tray(");
        assert!(
            setup.contains(".icon(tray_icon(app)?)"),
            "the tray icon must come from the platform-aware source (issue #911)"
        );
        assert!(
            setup.contains("set_icon_as_template(true)"),
            "the macOS icon must be marked as a template (issue #911)"
        );
        let source = body_of(prod, "fn tray_icon(");
        assert!(
            source.contains("default_window_icon()"),
            "Windows and Linux must keep the application icon (issue #911)"
        );
        assert!(
            source.contains("macos_template_icon()"),
            "macOS must build the monochrome template glyph (issue #911)"
        );
    }
}
