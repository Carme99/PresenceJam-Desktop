use std::sync::atomic::Ordering;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};

// Menu item IDs
const ID_SHOW_HIDE: &str = "show_hide_window";
const ID_PAUSE_SYNC: &str = "pause_sync";
const ID_RESUME_SYNC: &str = "resume_sync";
const ID_CURRENT_TRACK: &str = "current_track";
const ID_OPEN_SETTINGS: &str = "settings";
const ID_OPEN_LOGS: &str = "open_logs";
const ID_QUIT: &str = "quit";
// Spotify playback control (issue #3.0-P3). Device submenu items carry
// ids of the form `{ID_DEVICES}|{index}` so the click handler can look
// the selected device up in the cached device list.
const ID_PLAY_PAUSE: &str = "play_pause";
const ID_PREVIOUS: &str = "previous";
const ID_NEXT: &str = "next";
const ID_DEVICES: &str = "devices";
const ID_QUEUE: &str = "queue";
/// Menu-item id prefix for device submenu entries (`{ID_DEVICES}|{index}`).
const DEVICE_ITEM_PREFIX: &str = concat!(ID_DEVICES, "|");

static TRAY: OnceLock<TrayIcon> = OnceLock::new();

/// Get the global TrayIcon instance.
pub fn get_tray() -> Option<&'static TrayIcon> {
    TRAY.get()
}

pub fn setup_tray(app: &tauri::App) -> Result<(), String> {
    // Build initial menu
    let menu = build_initial_menu(app)?;

    let tray = TrayIconBuilder::new()
        .tooltip("PresenceJam")
        .icon(
            app.default_window_icon()
                .cloned()
                .ok_or("No default icon")?,
        )
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            ID_SHOW_HIDE => {
                if let Some(window) = app.get_webview_window("main") {
                    if window.is_visible().unwrap_or(false) {
                        let _ = window.hide();
                    } else {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
                // Refresh tray menu label (Show ↔ Hide) and sync state
                let state = app.state::<std::sync::Arc<crate::AppState>>();
                let is_syncing = state.polling.is_syncing(Ordering::Acquire);
                let current_track = state.polling.current_track().clone();
                let _ = update_tray_menu(app, is_syncing, current_track);
            }
            ID_PAUSE_SYNC | ID_RESUME_SYNC => {
                let _ = app.emit("toggle-pause", ());
            }
            ID_QUIT => {
                let _ = app.emit("app-shutdown", ());
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
            // Menu items handled by app menu (settings, open_logs) also come through here
            ID_OPEN_SETTINGS => {
                let _ = app.emit("navigate", "settings");
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            ID_OPEN_LOGS => {
                let _ = app.emit("open-logs-folder", ());
            }
            // Spotify playback control (issue #3.0-P3). These dispatch
            // directly against the Spotify API with the stored access
            // token — no frontend roundtrip — and force a tray rebuild on
            // success so the new state is reflected immediately.
            ID_PLAY_PAUSE => {
                let state = app.state::<std::sync::Arc<crate::AppState>>();
                let token = match state.tokens.spotify().as_ref() {
                    Some(t) => t.access_token.clone(),
                    None => {
                        log::warn!("[TRAY] play/pause: no Spotify token stored");
                        return;
                    }
                };
                // Resolve the ACTUAL playing state from the API rather than
                // the stored track: the polling loop's `is_playing` goes
                // stale on a same-track pause (it's only re-stored on
                // title/artist change), and an external device may have
                // changed state since. One extra GET per click is fine —
                // this is user-initiated. Unknown → resume (play).
                let should_pause =
                    match crate::spotify::get_currently_playing(&token) {
                        Ok(Some(track)) => track.is_playing,
                        _ => false,
                    };
                if should_pause {
                    run_player_action(app, "pause", Some(false), |t| {
                        crate::spotify::player_pause(t, None)
                    });
                } else {
                    run_player_action(app, "play", Some(true), |t| {
                        crate::spotify::player_play(t, None)
                    });
                }
            }
            ID_PREVIOUS => {
                // Skipping doesn't change the playing state.
                run_player_action(app, "previous", None, |token| {
                    crate::spotify::player_previous(token, None)
                });
            }
            ID_NEXT => {
                run_player_action(app, "next", None, |token| {
                    crate::spotify::player_next(token, None)
                });
            }
            id if id.starts_with(DEVICE_ITEM_PREFIX) => {
                // Device submenu item: `{ID_DEVICES}|{index}` into the
                // cached device list.
                let index = id
                    .strip_prefix(DEVICE_ITEM_PREFIX)
                    .and_then(|s| s.parse::<usize>().ok());
                let device_id = index
                    .and_then(|i| {
                        DEVICES_CACHE
                            .lock()
                            .as_ref()
                            .and_then(|(_, devices)| devices.get(i).cloned())
                    })
                    .and_then(|device| device.id);
                match device_id {
                    Some(device_id) => {
                        // Transfer starts playback on the target device.
                        run_player_action(app, "transfer", Some(true), |token| {
                            crate::spotify::player_transfer(token, &device_id, true)
                        });
                    }
                    None => {
                        log::warn!(
                            "[TRAY] transfer: unknown or id-less device selected (id={})",
                            id
                        );
                    }
                }
            }
            _ => {}
        })
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
        .build(app)
        .map_err(|e| e.to_string())?;

    // Store the TrayIcon globally (idempotent)
    if TRAY.get().is_some() {
        log::warn!("[TRAY] setup_tray: already initialized, skipping");
        return Ok(());
    }
    TRAY.set(tray)
        .map_err(|_| "Tray already initialized".to_string())?;

    // Immediately update tray menu to reflect actual state (Bug 11 fix).
    // Without this, the initial menu always shows "Pause Sync" regardless of actual
    // sync state, and the menu doesn't show the current track if one is cached.
    let state = app.state::<std::sync::Arc<crate::AppState>>();
    let is_syncing = state.polling.is_syncing(Ordering::Acquire);
    let current_track = state.polling.current_track().clone();
    if let Err(e) = update_tray_menu(app.handle(), is_syncing, current_track) {
        log::warn!(
            "[TRAY] setup_tray: failed to update initial tray menu: {}",
            e
        );
    }

    log::info!("[TRAY] setup_tray: system tray initialized successfully");
    Ok(())
}

/// Builds the initial tray menu.
fn build_initial_menu(app: &tauri::App) -> Result<tauri::menu::Menu<tauri::Wry>, String> {
    let show_hide = MenuItemBuilder::with_id(ID_SHOW_HIDE, "Show Window")
        .build(app)
        .map_err(|e| e.to_string())?;

    let pause_sync = MenuItemBuilder::with_id(ID_PAUSE_SYNC, "Pause Sync")
        .build(app)
        .map_err(|e| e.to_string())?;

    let separator = PredefinedMenuItem::separator(app).map_err(|e| e.to_string())?;

    let open_settings = MenuItemBuilder::with_id(ID_OPEN_SETTINGS, "Open Settings")
        .build(app)
        .map_err(|e| e.to_string())?;

    let open_logs = MenuItemBuilder::with_id(ID_OPEN_LOGS, "Open Logs Folder")
        .build(app)
        .map_err(|e| e.to_string())?;

    let quit = MenuItemBuilder::with_id(ID_QUIT, "Quit")
        .build(app)
        .map_err(|e| e.to_string())?;

    // Spotify playback controls (issue #3.0-P3). This initial menu is
    // transient — `setup_tray` immediately calls `update_tray_menu` with
    // real state — so the Play/Pause label defaults to "Play" and the
    // Devices/Up Next submenus start as placeholders (no network at
    // startup).
    let play_pause = MenuItemBuilder::with_id(ID_PLAY_PAUSE, "Play")
        .build(app)
        .map_err(|e| e.to_string())?;
    let previous = MenuItemBuilder::with_id(ID_PREVIOUS, "Previous")
        .build(app)
        .map_err(|e| e.to_string())?;
    let next = MenuItemBuilder::with_id(ID_NEXT, "Next")
        .build(app)
        .map_err(|e| e.to_string())?;
    let playback_separator = PredefinedMenuItem::separator(app).map_err(|e| e.to_string())?;
    let devices_submenu = build_devices_submenu(app.handle(), None)?;
    let queue_submenu = build_queue_submenu(app.handle(), None)?;

    MenuBuilder::new(app)
        .items(&[
            &show_hide,
            &pause_sync,
            &separator,
            &play_pause,
            &previous,
            &next,
            &playback_separator,
            &devices_submenu,
            &queue_submenu,
            &open_settings,
            &open_logs,
            &separator,
            &quit,
        ])
        .build()
        .map_err(|e| e.to_string())
}

/// Snapshot of the last tray-menu state, used for the dedup guard in
/// `update_tray_menu` (issue #71). The polling thread calls
/// `update_tray_menu` on every successful poll; the menu only needs to
/// change when `is_syncing`, window visibility (the Show/Hide label),
/// or the current track's title/is_playing change.
type TrayStateSnapshot = (bool, bool, Option<String>); // (is_syncing, is_window_visible, track_key)

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

/// Throttle window for the tray's Spotify devices/queue fetches
/// (issue #3.0-P3). The polling loop calls `update_tray_menu` on every
/// successful poll; without a cap the Devices/Up Next submenus would
/// hammer the Spotify API on every iteration. Fetched data is cached
/// here and re-used until the window lapses.
const TRAY_SPOTIFY_FETCH_THROTTLE: Duration = Duration::from_secs(60);

/// Cache of the last devices fetch: `(fetched_at, devices)`. Also serves
/// as the source of truth for the device submenu's click dispatch — menu
/// item ids are `{ID_DEVICES}|{index}` into this list.
static DEVICES_CACHE: std::sync::LazyLock<
    parking_lot::Mutex<Option<(Instant, Vec<crate::spotify::DeviceInfo>)>>,
> = std::sync::LazyLock::new(|| parking_lot::Mutex::new(None));

static QUEUE_CACHE: std::sync::LazyLock<
    parking_lot::Mutex<Option<(Instant, crate::spotify::QueueInfo)>>,
> = std::sync::LazyLock::new(|| parking_lot::Mutex::new(None));

/// Last known Spotify playing state, driving the Play/Pause toggle label
/// (and its fallback dispatch). Seeded from the polling loop's stored track
/// on a genuine track change and updated by the tray's own successful
/// play/pause/transfer actions. Needed because the polling loop only
/// re-stores `current_track` on title/artist change, so its `is_playing`
/// goes stale on a same-track pause — dispatching on it directly would
/// invert the toggle. Issue #3.0-P3.
static LAST_PLAYING_STATE: std::sync::LazyLock<std::sync::atomic::AtomicBool> =
    std::sync::LazyLock::new(|| std::sync::atomic::AtomicBool::new(false));

/// Returns cached devices when the throttle window hasn't elapsed, else
/// fetches fresh ones. On a fetch failure the stale cache is returned so
/// the submenu doesn't flicker to "(no devices)" on a transient error.
fn cached_devices(access_token: &str) -> Vec<crate::spotify::DeviceInfo> {
    let mut cache = DEVICES_CACHE.lock();
    if let Some((fetched_at, devices)) = cache.as_ref() {
        if fetched_at.elapsed() < TRAY_SPOTIFY_FETCH_THROTTLE {
            return devices.clone();
        }
    }
    match crate::spotify::get_devices(access_token) {
        Ok(devices) => {
            *cache = Some((Instant::now(), devices.clone()));
            devices
        }
        Err(e) => {
            log::warn!("[TRAY] cached_devices: failed to fetch devices: {}", e);
            cache
                .as_ref()
                .map(|(_, devices)| devices.clone())
                .unwrap_or_default()
        }
    }
}

/// Returns cached queue when the throttle window hasn't elapsed, else
/// fetches fresh. Falls back to the stale cache on failure.
fn cached_queue(access_token: &str) -> Option<crate::spotify::QueueInfo> {
    let mut cache = QUEUE_CACHE.lock();
    if let Some((fetched_at, queue)) = cache.as_ref() {
        if fetched_at.elapsed() < TRAY_SPOTIFY_FETCH_THROTTLE {
            return Some(queue.clone());
        }
    }
    match crate::spotify::get_queue(access_token) {
        Ok(queue) => {
            *cache = Some((Instant::now(), queue.clone()));
            Some(queue)
        }
        Err(e) => {
            log::warn!("[TRAY] cached_queue: failed to fetch queue: {}", e);
            cache.as_ref().map(|(_, queue)| queue.clone())
        }
    }
}

/// Builds the Devices submenu. `access_token` is `None` before the app has
/// Spotify tokens (initial menu build) — the submenu then shows a single
/// disabled "(no devices)" placeholder.
fn build_devices_submenu(
    app: &AppHandle,
    access_token: Option<&str>,
) -> Result<Submenu<tauri::Wry>, String> {
    let submenu = Submenu::with_id(app, ID_DEVICES, "Devices", true)
        .map_err(|e| e.to_string())?;
    let devices = match access_token {
        Some(token) => cached_devices(token),
        None => Vec::new(),
    };
    if devices.is_empty() {
        let empty = MenuItemBuilder::with_id(format!("{}|none", ID_DEVICES), "(no devices)")
            .enabled(false)
            .build(app)
            .map_err(|e| e.to_string())?;
        submenu.append(&empty).map_err(|e| e.to_string())?;
    } else {
        for (index, device) in devices.iter().enumerate() {
            let label = if device.is_active {
                format!("✓ {}", device.name)
            } else {
                device.name.clone()
            };
            // The active device (and id-less devices) can't be transferred to.
            let enabled = !device.is_active && device.id.is_some();
            let item = MenuItemBuilder::with_id(format!("{}|{}", ID_DEVICES, index), label)
                .enabled(enabled)
                .build(app)
                .map_err(|e| e.to_string())?;
            submenu.append(&item).map_err(|e| e.to_string())?;
        }
    }
    Ok(submenu)
}

/// Builds the Up Next submenu from the cached queue, showing at most 3
/// upcoming tracks as disabled items and "(queue empty)" when none.
fn build_queue_submenu(
    app: &AppHandle,
    access_token: Option<&str>,
) -> Result<Submenu<tauri::Wry>, String> {
    let submenu = Submenu::with_id(app, ID_QUEUE, "Up Next", true)
        .map_err(|e| e.to_string())?;
    let queue = match access_token {
        Some(token) => cached_queue(token),
        None => None,
    };
    let up_next: Vec<crate::spotify::TrackInfo> = queue
        .as_ref()
        .map(|q| q.up_next.iter().take(3).cloned().collect())
        .unwrap_or_default();
    if up_next.is_empty() {
        let empty = MenuItemBuilder::with_id(format!("{}|none", ID_QUEUE), "(queue empty)")
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

/// Runs a Spotify player action from a tray click using the stored access
/// token. On success the tray menu is force-refreshed and, when the action
/// deterministically changes the playing state (`resulting_playing` is
/// `Some`), that state is recorded for the Play/Pause toggle; `None` leaves
/// the toggle unchanged (next/previous don't change playing state). On
/// failure the error is logged and emitted on the `playback-error` event so
/// the frontend can surface it. A `NoActiveDevice` error (404) is logged
/// distinctly — the Devices submenu offers transfer in that case.
/// See issue #3.0-P3.
fn run_player_action(
    app: &AppHandle,
    label: &str,
    resulting_playing: Option<bool>,
    action: impl FnOnce(&str) -> Result<(), crate::spotify::SpotifyApiError>,
) {
    let state = app.state::<std::sync::Arc<crate::AppState>>();
    let token = match state.tokens.spotify().as_ref() {
        Some(t) => t.access_token.clone(),
        None => {
            log::warn!("[TRAY] {}: no Spotify token stored", label);
            return;
        }
    };
    match action(&token) {
        Ok(()) => {
            log::info!("[TRAY] {}: success", label);
            if let Some(playing) = resulting_playing {
                LAST_PLAYING_STATE.store(playing, Ordering::Release);
            }
            force_tray_refresh(app);
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
        }
        Err(e) => {
            log::error!("[TRAY] {}: failed: {}", label, e);
            let _ = app.emit("playback-error", e.to_string());
        }
    }
}

/// Forces the next `update_tray_menu` call to rebuild: clears both throttled
/// caches (so devices/queue are re-fetched), nudges the dedup snapshot so
/// the rebuild can't early-return, then rebuilds immediately. User-initiated
/// tray actions call this so the menu reflects the new playback/device
/// state right away — the dedup key alone wouldn't change on e.g. a pause
/// or a transfer.
///
/// The snapshot is nudged (not cleared) with the *current* track key so the
/// re-seed logic in `update_tray_menu` (which only fires on a genuine track
/// change) doesn't clobber the toggle state set by the action. See
/// issue #3.0-P3.
fn force_tray_refresh(app: &AppHandle) {
    *DEVICES_CACHE.lock() = None;
    *QUEUE_CACHE.lock() = None;
    let state = app.state::<std::sync::Arc<crate::AppState>>();
    let is_syncing = state.polling.is_syncing(Ordering::Acquire);
    let current_track = state.polling.current_track().clone();
    let track_key = current_track
        .as_ref()
        .filter(|t| t.is_playing)
        .map(|t| format!("{}|{}", t.artist, t.title));
    // Flip the sync bit: the real (is_syncing, visible, track_key) tuple is
    // committed by the rebuild below, so this can never match the dedup key.
    *last_tray_state().lock() = Some((!is_syncing, false, track_key));
    let _ = update_tray_menu(app, is_syncing, current_track);
}

/// Rebuilds the tray menu with current state.
/// Called by menu.rs when sync state or track changes.
pub fn update_tray_menu(
    app: &AppHandle,
    is_syncing: bool,
    current_track: Option<crate::spotify::TrackInfo>,
) -> Result<(), String> {
    let tray = match get_tray() {
        Some(t) => t,
        None => {
            log::warn!("[TRAY] update_tray_menu: Tray not initialized");
            return Err("Tray not initialized".to_string());
        }
    };

    // Issue #71: dedup guard. The polling thread calls this on every
    // successful poll; the menu only needs rebuilding when is_syncing,
    // window visibility (drives the Show/Hide label), or the track's
    // title/is_playing actually changes.
    //
    // Window visibility is computed up front so the dedup key includes
    // it — otherwise a hide/show click would early-return and the label
    // would go stale.
    let is_window_visible = app
        .get_webview_window("main")
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);
    let track_key = current_track
        .as_ref()
        .filter(|t| t.is_playing)
        .map(|t| format!("{}|{}", t.artist, t.title));
    {
        let last = last_tray_state().lock();
        if last.as_ref() == Some(&(is_syncing, is_window_visible, track_key.clone())) {
            // No-op: menu state hasn't changed.
            return Ok(());
        }
        // Re-seed the Play/Pause toggle's playing state when the polling
        // loop observed a genuinely new track (or a stop): its stored
        // `is_playing` is only fresh at track-change time. Tray-initiated
        // actions update LAST_PLAYING_STATE themselves, and a forced
        // rebuild keeps the same track_key, so neither path re-seeds here.
        let track_changed = match last.as_ref() {
            Some((_, _, prev_key)) => prev_key != &track_key,
            None => true,
        };
        if track_changed {
            let fresh_playing = current_track
                .as_ref()
                .map(|t| t.is_playing)
                .unwrap_or(false);
            LAST_PLAYING_STATE.store(fresh_playing, Ordering::Release);
        }
        // Do NOT update the snapshot yet. If the rebuild below fails
        // (e.g., set_menu returns Err), we want the next call with the
        // same state to retry rather than no-op on a stale snapshot.
    }

    // Issue #71: serialise the two writers. Acquiring before the long
    // menu build means the polling thread and the frontend command
    // never interleave a `set_menu` call.
    let _write_guard = tray_write_lock().lock();

    // Determine Show/Hide label based on the precomputed visibility.
    let show_hide_label = if is_window_visible {
        "Hide Window"
    } else {
        "Show Window"
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
        "Pause Sync"
    } else {
        "Resume Sync"
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

    let open_settings = MenuItemBuilder::with_id(ID_OPEN_SETTINGS, "Open Settings")
        .build(app)
        .map_err(|e| {
            log::warn!(
                "[TRAY] update_tray_menu: failed to build open_settings menu item: {}",
                e
            );
            e.to_string()
        })?;

    let open_logs = MenuItemBuilder::with_id(ID_OPEN_LOGS, "Open Logs Folder")
        .build(app)
        .map_err(|e| {
            log::warn!(
                "[TRAY] update_tray_menu: failed to build open_logs menu item: {}",
                e
            );
            e.to_string()
        })?;

    let quit = MenuItemBuilder::with_id(ID_QUIT, "Quit")
        .build(app)
        .map_err(|e| {
            log::warn!(
                "[TRAY] update_tray_menu: failed to build quit menu item: {}",
                e
            );
            e.to_string()
        })?;

    // Spotify playback controls (issue #3.0-P3). Play/Pause is a single
    // toggle labelled from LAST_PLAYING_STATE (see the static's docs — the
    // polling loop's stored track goes stale on a same-track pause); the
    // Devices/Up Next submenus are built from the throttled caches so the
    // polling loop's per-iteration rebuilds don't hammer the Spotify API.
    let is_playing = LAST_PLAYING_STATE.load(Ordering::Acquire);
    let play_pause = MenuItemBuilder::with_id(
        ID_PLAY_PAUSE,
        if is_playing { "Pause" } else { "Play" },
    )
    .build(app)
    .map_err(|e| {
        log::warn!("[TRAY] update_tray_menu: failed to build play_pause item: {}", e);
        e.to_string()
    })?;
    let previous = MenuItemBuilder::with_id(ID_PREVIOUS, "Previous")
        .build(app)
        .map_err(|e| {
            log::warn!("[TRAY] update_tray_menu: failed to build previous item: {}", e);
            e.to_string()
        })?;
    let next = MenuItemBuilder::with_id(ID_NEXT, "Next")
        .build(app)
        .map_err(|e| {
            log::warn!("[TRAY] update_tray_menu: failed to build next item: {}", e);
            e.to_string()
        })?;
    let playback_separator = PredefinedMenuItem::separator(app).map_err(|e| {
        log::warn!("[TRAY] update_tray_menu: failed to build playback_separator: {}", e);
        e.to_string()
    })?;

    let state = app.state::<std::sync::Arc<crate::AppState>>();
    let access_token = state
        .tokens
        .spotify()
        .as_ref()
        .map(|t| t.access_token.clone());
    let devices_submenu = build_devices_submenu(app, access_token.as_deref()).map_err(|e| {
        log::warn!("[TRAY] update_tray_menu: failed to build devices submenu: {}", e);
        e
    })?;
    let queue_submenu = build_queue_submenu(app, access_token.as_deref()).map_err(|e| {
        log::warn!("[TRAY] update_tray_menu: failed to build queue submenu: {}", e);
        e
    })?;

    // Build menu with optional track info
    let mut menu_builder = MenuBuilder::new(app)
        .items(&[&show_hide, &pause_resume, &separator1])
        .items(&[&play_pause, &previous, &next, &playback_separator])
        .items(&[&devices_submenu, &queue_submenu]);

    // Add current track item if playing — insert separator2 here too
    if let Some(track) = &current_track {
        if track.is_playing {
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

    tray.set_menu(Some(menu)).map_err(|e| {
        log::warn!("[TRAY] update_tray_menu: failed to set tray menu: {}", e);
        format!("Failed to set tray menu: {}", e)
    })?;

    // Commit the snapshot only after a successful set_menu. A failed
    // set_menu above left the snapshot at the previous value, so the
    // next call with the same state will retry rather than no-op.
    *last_tray_state().lock() = Some((is_syncing, is_window_visible, track_key));

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
