use std::sync::atomic::Ordering;
use std::sync::OnceLock;
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem},
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
                let is_syncing = state.is_syncing.load(Ordering::Acquire);
                let current_track = state.current_track.read().clone();
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
    let is_syncing = state.is_syncing.load(Ordering::Acquire);
    let current_track = state.current_track.read().clone();
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

    MenuBuilder::new(app)
        .items(&[
            &show_hide,
            &pause_sync,
            &separator,
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

    // Build menu with optional track info
    let mut menu_builder = MenuBuilder::new(app).items(&[&show_hide, &pause_resume, &separator1]);

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
