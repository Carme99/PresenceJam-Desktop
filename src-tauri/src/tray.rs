use std::sync::OnceLock;
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};

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
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show_hide_window" => {
                if let Some(window) = app.get_webview_window("main") {
                    if window.is_visible().unwrap_or(false) {
                        let _ = window.hide();
                    } else {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
            }
            "pause_sync" | "resume_sync" => {
                let _ = app.emit("toggle-pause", ());
            }
            "quit" => {
                let _ = app.emit("app-shutdown", ());
                let app_handle = app.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    log::info!("[TRAY] quit: forced exit fallback after app-shutdown");
                    let _ = app_handle.exit(0);
                });
            }
            // Menu items handled by app menu (settings, open_logs) also come through here
            "settings" => {
                let _ = app.emit("navigate", "settings");
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "open_logs" => {
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

    // Store the TrayIcon globally
    TRAY.set(tray).map_err(|_| "Tray already initialized".to_string())?;

    log::info!("[TRAY] setup_tray: system tray initialized successfully");
    Ok(())
}

/// Builds the initial tray menu.
fn build_initial_menu(app: &tauri::App) -> Result<tauri::menu::Menu<tauri::Wry>, String> {
    let show_hide = MenuItemBuilder::with_id("show_hide_window", "Show Window")
        .build(app)
        .map_err(|e| e.to_string())?;

    let pause_sync = MenuItemBuilder::with_id("pause_sync", "Pause Sync")
        .build(app)
        .map_err(|e| e.to_string())?;

    let separator = PredefinedMenuItem::separator(app).map_err(|e| e.to_string())?;

    let open_settings = MenuItemBuilder::with_id("settings", "Open Settings")
        .build(app)
        .map_err(|e| e.to_string())?;

    let open_logs = MenuItemBuilder::with_id("open_logs", "Open Logs Folder")
        .build(app)
        .map_err(|e| e.to_string())?;

    let quit = MenuItemBuilder::with_id("quit", "Quit")
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
    let tray = get_tray().ok_or_else(|| "Tray not initialized".to_string())?;

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

    let show_hide = MenuItemBuilder::with_id("show_hide_window", show_hide_label)
        .build(app)
        .map_err(|e| e.to_string())?;

    // Pause/Resume label based on sync state
    let pause_resume_id = if is_syncing { "pause_sync" } else { "resume_sync" };
    let pause_resume_label = if is_syncing { "Pause Sync" } else { "Resume Sync" };
    let pause_resume = MenuItemBuilder::with_id(pause_resume_id, pause_resume_label)
        .build(app)
        .map_err(|e| e.to_string())?;

    let separator1 = PredefinedMenuItem::separator(app).map_err(|e| e.to_string())?;
    let separator2 = PredefinedMenuItem::separator(app).map_err(|e| e.to_string())?;
    let separator3 = PredefinedMenuItem::separator(app).map_err(|e| e.to_string())?;

    let open_settings = MenuItemBuilder::with_id("settings", "Open Settings")
        .build(app)
        .map_err(|e| e.to_string())?;

    let open_logs = MenuItemBuilder::with_id("open_logs", "Open Logs Folder")
        .build(app)
        .map_err(|e| e.to_string())?;

    let quit = MenuItemBuilder::with_id("quit", "Quit")
        .build(app)
        .map_err(|e| e.to_string())?;

    // Build menu with optional track info
    let mut menu_builder = MenuBuilder::new(app).items(&[&show_hide, &pause_resume, &separator1]);

    // Add current track item if playing
    if let Some(track) = &current_track {
        if track.is_playing {
            let track_item = MenuItemBuilder::with_id(
                "current_track",
                format!("🎵 {} - {}", track.artist, track.title),
            )
            .enabled(false)
            .build(app)
            .map_err(|e| e.to_string())?;
            menu_builder = menu_builder.item(&track_item);
        }
    }

    let menu = menu_builder
        .items(&[&separator2, &open_settings, &open_logs, &separator3, &quit])
        .build()
        .map_err(|e| e.to_string())?;

    tray.set_menu(Some(menu))
        .map_err(|e| format!("Failed to set tray menu: {}", e))?;

    log::info!(
        "[TRAY] update_tray_menu: tray menu updated - is_syncing={}, track={:?}",
        is_syncing,
        current_track.as_ref().map(|t| format!("{} - {}", t.artist, t.title))
    );
    Ok(())
}
