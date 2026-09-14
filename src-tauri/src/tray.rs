use std::sync::atomic::Ordering;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tauri::{
    menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder, PredefinedMenuItem, Submenu},
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
// ids of the form `{ID_DEVICES}|{spotify device id}` so the click handler
// resolves the stable id instead of racing a list index (issue #388).
const ID_PLAY_PAUSE: &str = "play_pause";
/// Static label for the Play/Pause check item — the playing state is
/// conveyed by the native checked mark instead of a swapped label
/// (docs/scope-3.3.md C4).
const PLAY_PAUSE_LABEL: &str = "Play/Pause";
const ID_PREVIOUS: &str = "previous";
const ID_NEXT: &str = "next";
const ID_DEVICES: &str = "devices";
const ID_QUEUE: &str = "queue";
/// Menu-item id prefix for device submenu entries (`{ID_DEVICES}|{device id}`).
/// `concat!` cannot take a const, so this mirrors `ID_DEVICES` literally;
/// keep the two in sync when either changes.
const DEVICE_ITEM_PREFIX: &str = "devices|";

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
                    let _ = window.set_focus();
                }
            }
            ID_OPEN_LOGS => {
                let _ = app.emit("open-logs-folder", ());
            }
            // Spotify playback control (issue #3.0-P3). These dispatch
            // directly against the Spotify API with the stored access
            ID_PLAY_PAUSE => {
                // Issue #386: the click-path blocking Spotify HTTP must not
                // run on the menu-event thread — a slow network would wedge
                // the tray menu. Snapshot the token and offload everything
                // (the currently-playing GET plus the play/pause action)
                // to a worker thread.
                let state = app.state::<std::sync::Arc<crate::AppState>>();
                let token = match state.tokens.spotify().as_ref() {
                    Some(t) => t.access_token.clone(),
                    None => {
                        log::warn!("[TRAY] play/pause: no Spotify token stored");
                        return;
                    }
                };
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
                    let should_pause = match crate::spotify::get_currently_playing(&token, None) {
                        Ok(crate::spotify::CurrentlyPlaying::Modified {
                            track: Some(track),
                            ..
                        }) => track.is_playing,
                        _ => false,
                    };
                    if should_pause {
                        run_player_action(&app_handle, "pause", Some(false), |t| {
                            crate::spotify::player_pause(t, None)
                        });
                    } else {
                        run_player_action(&app_handle, "play", Some(true), |t| {
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
                    run_player_action(&app_handle, "previous", None, |token| {
                        crate::spotify::player_previous(token, None)
                    });
                });
            }
            ID_NEXT => {
                // Issue #386: offload the blocking Spotify HTTP off the
                // menu-event thread. Skipping doesn't change playing state.
                let app_handle = app.clone();
                std::thread::spawn(move || {
                    run_player_action(&app_handle, "next", None, |token| {
                        crate::spotify::player_next(token, None)
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
                            run_player_action(&app_handle, "transfer", Some(true), |token| {
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
    // real state — so the Play/Pause toggle starts unchecked and the
    // Devices/Up Next submenus start as placeholders (no network at
    // startup).
    let play_pause = CheckMenuItemBuilder::with_id(ID_PLAY_PAUSE, PLAY_PAUSE_LABEL)
        .checked(false)
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

/// Last known Spotify playing state, driving the Play/Pause toggle label
/// (and its fallback dispatch). Seeded from the polling loop's stored track
/// on a genuine track change and updated by the tray's own successful
/// play/pause/transfer actions. Needed because the polling loop only
/// re-stores `current_track` on title/artist change, so its `is_playing`
/// goes stale on a same-track pause — dispatching on it directly would
/// invert the toggle. Issue #3.0-P3.
static LAST_PLAYING_STATE: std::sync::LazyLock<std::sync::atomic::AtomicBool> =
    std::sync::LazyLock::new(|| std::sync::atomic::AtomicBool::new(false));

/// Coalescing guard for the delayed one-shot refresh kicked after a
/// successful tray player action: rapid next/previous clicks must not pile
/// up unbounded 2 s-sleep threads each firing blocking Spotify+Teams HTTP.
/// First claimant spawns; losers skip (their track change is covered by the
/// in-flight refresh's unconditional GET plus the polling loop).
static DELAYED_REFRESH_IN_FLIGHT: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Returns cached devices when the throttle window hasn't elapsed, else
/// fetches fresh ones. On a fetch failure the stale cache is returned so
/// the submenu doesn't flicker to "(no devices)" on a transient error.
///
/// The cache mutex is held only to clone the snapshot and to store the
/// fresh result — the HTTP fetch runs outside any lock so a cold/stale
/// cache never blocks tray interactions. If two threads race on a stale
/// cache both will fetch; the last writer wins. This benign double-fetch
/// wastes one request but cannot corrupt state. See issue #217.
fn cached_devices(access_token: &str) -> Vec<crate::spotify::DeviceInfo> {
    // Snapshot under short lock, then drop before deciding staleness.
    let snapshot = {
        let cache = DEVICES_CACHE.lock();
        cache.clone()
    };
    let needs_fetch = match &snapshot {
        Some((fetched_at, _)) => fetched_at.elapsed() >= TRAY_SPOTIFY_FETCH_THROTTLE,
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
fn cached_queue(access_token: &str) -> Option<crate::spotify::QueueInfo> {
    // Snapshot under short lock, then drop before deciding staleness.
    let snapshot = {
        let cache = QUEUE_CACHE.lock();
        cache.clone()
    };
    let needs_fetch = match &snapshot {
        Some((fetched_at, _)) => fetched_at.elapsed() >= TRAY_SPOTIFY_FETCH_THROTTLE,
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
/// menu-event thread — the fallback performs blocking HTTP (issue #386).
fn resolve_device_id(app: &AppHandle, selected: &DeviceMenuSelection) -> Option<String> {
    let state = app.state::<std::sync::Arc<crate::AppState>>();
    let token = state.tokens.spotify().as_ref()?.access_token.clone();
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
            match crate::spotify::get_devices(&token) {
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

/// Builds the Devices submenu. `access_token` is `None` before the app has
/// Spotify tokens (initial menu build) — the submenu then shows a single
/// disabled "(no devices)" placeholder.
fn build_devices_submenu(
    app: &AppHandle,
    access_token: Option<&str>,
) -> Result<Submenu<tauri::Wry>, String> {
    let devices = match access_token {
        Some(token) => cached_devices(token),
        None => Vec::new(),
    };
    build_devices_submenu_from_devices(app, &devices)
}

/// Builds the Devices submenu from an already-fetched slice. No HTTP is
/// performed here — the caller must have fetched outside any tray lock.
/// See issue #217.
fn build_devices_submenu_from_devices(
    app: &AppHandle,
    devices: &[crate::spotify::DeviceInfo],
) -> Result<Submenu<tauri::Wry>, String> {
    let submenu = Submenu::with_id(app, ID_DEVICES, "Devices", true).map_err(|e| e.to_string())?;
    if devices.is_empty() {
        let empty = MenuItemBuilder::with_id(format!("{}|none", ID_DEVICES), "(no devices)")
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

/// Builds the Up Next submenu from the cached queue, showing at most 3
/// upcoming tracks as disabled items and "(queue empty)" when none.
fn build_queue_submenu(
    app: &AppHandle,
    access_token: Option<&str>,
) -> Result<Submenu<tauri::Wry>, String> {
    let queue = match access_token {
        Some(token) => cached_queue(token),
        None => None,
    };
    build_queue_submenu_from_queue(app, queue.as_ref())
}

/// Builds the Up Next submenu from an already-fetched queue snapshot.
/// No HTTP here — fetch must have happened outside the tray lock. See issue #217.
fn build_queue_submenu_from_queue(
    app: &AppHandle,
    queue: Option<&crate::spotify::QueueInfo>,
) -> Result<Submenu<tauri::Wry>, String> {
    let submenu = Submenu::with_id(app, ID_QUEUE, "Up Next", true).map_err(|e| e.to_string())?;
    let up_next: Vec<crate::spotify::TrackInfo> = queue
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
    // Include is_playing in the key so a same-track pause is not deduped away. See issue #229.
    let track_key = current_track
        .as_ref()
        .map(|t| format!("{}|{}|{}", t.artist, t.title, t.is_playing));
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

    // Issue #71 + #229: dedup guard. The polling thread calls this on every
    // successful poll; the menu only needs rebuilding when is_syncing,
    // window visibility (drives the Show/Hide label), or the track's
    // title/artist/is_playing actually changes. is_playing is included so a
    // same-track pause flips the Play/Pause label without waiting for a poll.
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
        .map(|t| format!("{}|{}|{}", t.artist, t.title, t.is_playing));
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

    // Fetch Spotify data OUTSIDE the tray write lock. The throttled caches
    // are snapshotted and fetched without holding either cache mutex across
    // HTTP (see cached_devices/cached_queue), and the tray lock is not yet
    // held so a concurrent tray click or polling update never blocks on the
    // network. Benign double-fetch race documented on those helpers. See issue #217.
    let state = app.state::<std::sync::Arc<crate::AppState>>();
    let access_token = state
        .tokens
        .spotify()
        .as_ref()
        .map(|t| t.access_token.clone());
    let devices: Vec<crate::spotify::DeviceInfo> = match access_token.as_deref() {
        Some(token) => cached_devices(token),
        None => Vec::new(),
    };
    let queue: Option<crate::spotify::QueueInfo> = match access_token.as_deref() {
        Some(token) => cached_queue(token),
        None => None,
    };

    // Build menu items without holding the tray write lock. Only the final
    // tray.set_menu call needs serialising — everything above is pure data
    // preparation and menu-item construction.
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
    // check-item whose native checked state comes from LAST_PLAYING_STATE
    // (see the static's docs — the polling loop's stored track goes stale
    // on a same-track pause); the Devices/Up Next submenus are built from
    // the pre-fetched throttled caches so the polling loop's per-iteration
    // rebuilds don't hammer the Spotify API. The checkmark is derived from
    // (track_id, is_playing) via the track_key dedup and LAST_PLAYING_STATE,
    // so a same-track pause flips without waiting for the next poll. See
    // issues #229 and #217.
    let is_playing = LAST_PLAYING_STATE.load(Ordering::Acquire);
    let play_pause = CheckMenuItemBuilder::with_id(ID_PLAY_PAUSE, PLAY_PAUSE_LABEL)
        .checked(is_playing)
        .build(app)
        .map_err(|e| {
            log::warn!(
                "[TRAY] update_tray_menu: failed to build play_pause item: {}",
                e
            );
            e.to_string()
        })?;
    let previous = MenuItemBuilder::with_id(ID_PREVIOUS, "Previous")
        .build(app)
        .map_err(|e| {
            log::warn!(
                "[TRAY] update_tray_menu: failed to build previous item: {}",
                e
            );
            e.to_string()
        })?;
    let next = MenuItemBuilder::with_id(ID_NEXT, "Next")
        .build(app)
        .map_err(|e| {
            log::warn!("[TRAY] update_tray_menu: failed to build next item: {}", e);
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
    let tooltip = match &current_track {
        Some(t) => format!(
            "{} — {} ({})",
            t.artist,
            t.title,
            if t.is_playing { "▶" } else { "⏸" }
        ),
        None => "PresenceJam".to_string(),
    };
    if let Err(e) = tray.set_tooltip(Some(tooltip)) {
        log::warn!("[TRAY] update_tray_menu: failed to set tooltip: {}", e);
    }

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

#[cfg(test)]
mod tests {
    use super::*;

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

    /// Issue #388: the click handler must resolve by id with a live
    /// re-fetch fallback instead of `devices.get(i)`. Brace-counted body
    /// isolation (order-independent): do not anchor on the next fn.
    #[test]
    fn device_click_resolves_by_id_with_live_fallback() {
        let src = include_str!("tray.rs");
        let sig_idx = src
            .find("fn resolve_device_id(")
            .expect("resolve_device_id must exist");
        let brace_open_rel = src[sig_idx..]
            .find('{')
            .expect("function body must have an opening brace");
        let body_start = sig_idx + brace_open_rel;
        let mut depth: u32 = 0;
        let mut i = body_start;
        let body_end = loop {
            match src.as_bytes()[i] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        break i;
                    }
                }
                _ => {}
            }
            i += 1;
            if i >= src.len() {
                panic!("unbalanced braces in resolve_device_id");
            }
        };
        let body = &src[body_start + 1..body_end];
        assert!(
            body.contains("get_devices(&token)"),
            "resolve_device_id must live re-fetch when the cache misses"
        );
        assert!(
            !body.contains("devices.get(*i).cloned()") || body.contains("LegacyIndex"),
            "index lookup must survive only on the LegacyIndex back-compat path"
        );
    }

    /// Issues #383/#386: the tray Quit arm must terminate via the shared
    /// graceful shutdown, and the playback/device click arms must offload
    /// blocking Spotify HTTP onto worker threads. Brace-counted body
    /// isolation (order-independent): do not anchor on the next fn.
    #[test]
    fn tray_click_arms_quit_and_offload() {
        let src = include_str!("tray.rs");
        let sig_idx = src
            .find("pub fn setup_tray(")
            .expect("setup_tray must exist");
        let brace_open_rel = src[sig_idx..]
            .find('{')
            .expect("function body must have an opening brace");
        let body_start = sig_idx + brace_open_rel;
        let mut depth: u32 = 0;
        let mut i = body_start;
        let body_end = loop {
            match src.as_bytes()[i] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        break i;
                    }
                }
                _ => {}
            }
            i += 1;
            if i >= src.len() {
                panic!("unbalanced braces in setup_tray");
            }
        };
        let body = &src[body_start + 1..body_end];
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
            "get_currently_playing(&token, None)",
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
    }
}
