use tauri::{
    menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder},
    AppHandle, Emitter, Manager, WebviewWindow,
};

// Menu item IDs — shared between tray and app menu for consistency
const ID_SETTINGS: &str = "settings";
const ID_OPEN_LOGS: &str = "open_logs";
const ID_QUIT: &str = "quit";
const ID_SHOW_DASHBOARD: &str = "show_dashboard";
const ID_SHOW_LOGS: &str = "show_logs";
const ID_ABOUT: &str = "about";

/// Show and focus the main window.
fn show_and_focus_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Builds the application menu bar (macOS/Windows).
/// This creates native File, Edit, View, and Help menus.
pub fn setup_app_menu(app: &tauri::App, window: &WebviewWindow) -> Result<(), String> {
    // File menu
    let file_menu = SubmenuBuilder::new(app, "File")
        .item(
            &MenuItemBuilder::with_id(ID_SETTINGS, "Settings...")
                .accelerator("CmdOrCtrl+,")
                .build(app)
                .map_err(|e| e.to_string())?,
        )
        .item(
            &MenuItemBuilder::with_id(ID_OPEN_LOGS, "Open Logs Folder")
                .accelerator("CmdOrCtrl+Shift+L")
                .build(app)
                .map_err(|e| e.to_string())?,
        )
        .separator()
        .item(
            &MenuItemBuilder::with_id(ID_QUIT, "Quit PresenceJam")
                .accelerator("CmdOrCtrl+Q")
                .build(app)
                .map_err(|e| e.to_string())?,
        )
        .build()
        .map_err(|e| e.to_string())?;

    // Edit menu (standard macOS clipboard shortcuts for text fields)
    let edit_menu = SubmenuBuilder::new(app, "Edit")
        .item(&PredefinedMenuItem::undo(app, None).map_err(|e| e.to_string())?)
        .item(&PredefinedMenuItem::redo(app, None).map_err(|e| e.to_string())?)
        .separator()
        .item(&PredefinedMenuItem::cut(app, None).map_err(|e| e.to_string())?)
        .item(&PredefinedMenuItem::copy(app, None).map_err(|e| e.to_string())?)
        .item(&PredefinedMenuItem::paste(app, None).map_err(|e| e.to_string())?)
        .item(&PredefinedMenuItem::select_all(app, None).map_err(|e| e.to_string())?)
        .build()
        .map_err(|e| e.to_string())?;

    // View menu
    let view_menu = SubmenuBuilder::new(app, "View")
        .item(
            &MenuItemBuilder::with_id(ID_SHOW_DASHBOARD, "Show Dashboard")
                .accelerator("CmdOrCtrl+1")
                .build(app)
                .map_err(|e| e.to_string())?,
        )
        .item(
            &MenuItemBuilder::with_id(ID_SHOW_LOGS, "Show Logs")
                .accelerator("CmdOrCtrl+2")
                .build(app)
                .map_err(|e| e.to_string())?,
        )
        .build()
        .map_err(|e| e.to_string())?;

    // Help menu
    // No accelerator for About — intentional (no standard macOS convention)
    let help_menu = SubmenuBuilder::new(app, "Help")
        .item(
            &MenuItemBuilder::with_id(ID_ABOUT, "About PresenceJam")
                .build(app)
                .map_err(|e| e.to_string())?,
        )
        .build()
        .map_err(|e| e.to_string())?;

    // Build the full menu bar
    let menu = MenuBuilder::new(app)
        .item(&file_menu)
        .item(&edit_menu)
        .item(&view_menu)
        .item(&help_menu)
        .build()
        .map_err(|e| e.to_string())?;

    // Set as the window menu on macOS (appears in menu bar)
    // Using window.set_menu() instead of app.set_menu() to ensure
    // click events are properly routed through on_menu_event
    window
        .set_menu(menu)
        .map_err(|e| format!("Failed to set window menu: {}", e))?;

    log::info!("[MENU] setup_app_menu: window menu bar created successfully");
    Ok(())
}

/// Handle menu events from the app menu bar.
pub fn handle_app_menu_event(app: &AppHandle, event_id: &str) {
    match event_id {
        ID_SETTINGS => {
            let _ = app.emit("navigate", "settings");
            show_and_focus_main_window(app);
        }
        ID_OPEN_LOGS => {
            let _ = app.emit("open-logs-folder", ());
        }
        ID_QUIT => {
            let _ = app.emit("app-shutdown", ());
            let app_handle = app.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(500));
                log::info!("[MENU] quit: forced exit fallback");
                // let _ suppresses unused_must_use lint cross-platform
                // (returns Result on Unix/macOS, () on Windows).
                // The `let_unit_value` allow is needed because clippy reads
                // the `let _ = unit_or_result;` pattern as a no-op let.
                #[allow(clippy::let_unit_value)]
                {
                    let _ = app_handle.exit(0);
                }
            });
        }
        ID_SHOW_DASHBOARD => {
            let _ = app.emit("navigate", "dashboard");
            show_and_focus_main_window(app);
        }
        ID_SHOW_LOGS => {
            let _ = app.emit("navigate", "logs");
            show_and_focus_main_window(app);
        }
        ID_ABOUT => {
            let _ = app.emit("show-about", ());
        }
        _ => {
            log::warn!(
                "[MENU] handle_app_menu_event: unknown event_id={}",
                event_id
            );
        }
    }
}
