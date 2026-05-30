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
        .icon(app.default_window_icon().cloned().ok_or("No default icon")?)
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
                let is_syncing = *state.is_syncing.read();
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
    TRAY.set(tray).map_err(|_| "Tray already initialized".to_string())?;

    // Immediately update tray menu to reflect actual state (Bug 11 fix).
    // Without this, the initial menu always shows "Pause Sync" regardless of actual
    // sync state, and the menu doesn't show the current track if one is cached.
    let state = app.state::<std::sync::Arc<crate::AppState>>();
    let is_syncing = *state.is_syncing.read();
    let current_track = state.current_track.read().clone();
    if let Err(e) = update_tray_menu(app.handle(), is_syncing, current_track) {
        log::warn!("[TRAY] setup_tray: failed to update initial tray menu: {}", e);
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

    // Determine Show/Hide label based on window visibility
    let show_hide_label = if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            "Hide Window"
        } else {
            "Show Window"
        }
    } else {
        "Show Window"
    };

    // Build all items, collecting errors instead of aborting on the first failure.
    // A partial menu is better than a stale one.
    let mut menu_builder = MenuBuilder::new(app);

    macro_rules! try_add_item {
        ($builder:expr, $item:expr, $name:expr) => {
            match $item {
                Ok(item) => $builder = $builder.item(&item),
                Err(e) => log::warn!("[TRAY] update_tray_menu: failed to build {}: {}", $name, e),
            }
        };
    }

    try_add_item!(
        menu_builder,
        MenuItemBuilder::with_id(ID_SHOW_HIDE, show_hide_label).build(app),
        "show_hide"
    );
    try_add_item!(
        menu_builder,
        MenuItemBuilder::with_id(
            if is_syncing { ID_PAUSE_SYNC } else { ID_RESUME_SYNC },
            if is_syncing { "Pause Sync" } else { "Resume Sync" },
        )
        .build(app),
        "pause_resume"
    );

    // Optionally add current track info
    if let Some(track) = &current_track {
        if track.is_playing {
            if let Ok(sep) = PredefinedMenuItem::separator(app) {
                menu_builder = menu_builder.item(&sep);
            } else {
                log::warn!("[TRAY] update_tray_menu: failed to build separator_before_track");
            }
            try_add_item!(
                menu_builder,
                MenuItemBuilder::with_id(
                    ID_CURRENT_TRACK,
                    format!("🎵 {} - {}", track.artist, track.title),
                )
                .enabled(false)
                .build(app),
                "track_item"
            );
        }
    }

    if current_track.as_ref().map(|t| t.is_playing).unwrap_or(false) {
        if let Ok(sep) = PredefinedMenuItem::separator(app) {
            menu_builder = menu_builder.item(&sep);
        } else {
            log::warn!("[TRAY] update_tray_menu: failed to build separator_after_track");
        }
    }

    try_add_item!(
        menu_builder,
        MenuItemBuilder::with_id(ID_OPEN_SETTINGS, "Open Settings").build(app),
        "open_settings"
    );
    try_add_item!(
        menu_builder,
        MenuItemBuilder::with_id(ID_OPEN_LOGS, "Open Logs Folder").build(app),
        "open_logs"
    );
    try_add_item!(
        menu_builder,
        MenuItemBuilder::with_id(ID_QUIT, "Quit").build(app),
        "quit"
    );

    let menu = menu_builder
        .build()
        .map_err(|e| {
            log::warn!("[TRAY] update_tray_menu: failed to build menu: {}", e);
            e.to_string()
        })?;

    tray.set_menu(Some(menu))
        .map_err(|e| {
            log::warn!("[TRAY] update_tray_menu: failed to set tray menu: {}", e);
            format!("Failed to set tray menu: {}", e)
        })?;

    log::info!(
        "[TRAY] update_tray_menu: tray menu updated - is_syncing={}, track={:?}",
        is_syncing,
        current_track.as_ref().map(|t| format!("{} - {}", t.artist, t.title))
    );
    Ok(())
}
