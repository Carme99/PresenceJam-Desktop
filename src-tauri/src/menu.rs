use tauri::{
    menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder},
    AppHandle, Emitter, Manager, WebviewWindow,
};

/// Builds the application menu bar (macOS/Windows).
/// This creates native File, View, and Help menus.
pub fn setup_app_menu(app: &tauri::App, window: &WebviewWindow) -> Result<(), String> {
    // File menu
    let file_menu = SubmenuBuilder::new(app, "File")
        .item(
            &MenuItemBuilder::with_id("settings", "Settings...")
                .accelerator("CmdOrCtrl+,")
                .build(app)
                .map_err(|e| e.to_string())?,
        )
        .item(
            &MenuItemBuilder::with_id("open_logs", "Open Logs Folder")
                .accelerator("CmdOrCtrl+Shift+L")
                .build(app)
                .map_err(|e| e.to_string())?,
        )
        .separator()
        .item(
            &MenuItemBuilder::with_id("quit", "Quit PresenceJam")
                .accelerator("CmdOrCtrl+Q")
                .build(app)
                .map_err(|e| e.to_string())?,
        )
        .build()
        .map_err(|e| e.to_string())?;

    // View menu
    let view_menu = SubmenuBuilder::new(app, "View")
        .item(
            &MenuItemBuilder::with_id("show_dashboard", "Show Dashboard")
                .accelerator("CmdOrCtrl+1")
                .build(app)
                .map_err(|e| e.to_string())?,
        )
        .item(
            &MenuItemBuilder::with_id("show_logs", "Show Logs")
                .accelerator("CmdOrCtrl+2")
                .build(app)
                .map_err(|e| e.to_string())?,
        )
        .build()
        .map_err(|e| e.to_string())?;

    // Help menu
    let help_menu = SubmenuBuilder::new(app, "Help")
        .item(
            &MenuItemBuilder::with_id("about", "About PresenceJam")
                .build(app)
                .map_err(|e| e.to_string())?,
        )
        .build()
        .map_err(|e| e.to_string())?;

    // Build the full menu bar
    let menu = MenuBuilder::new(app)
        .item(&file_menu)
        .item(&view_menu)
        .item(&help_menu)
        .build()
        .map_err(|e| e.to_string())?;

    // Set as the window menu on macOS (appears in menu bar)
    // Using window.set_menu() instead of app.set_menu() to ensure
    // click events are properly routed through on_menu_event
    window.set_menu(menu)
        .map_err(|e| format!("Failed to set window menu: {}", e))?;

    log::info!("[MENU] setup_app_menu: window menu bar created successfully");
    Ok(())
}

/// Handle menu events from the app menu bar.
pub fn handle_app_menu_event(app: &AppHandle, event_id: &str) {
    log::info!("[MENU] handle_app_menu_event: event_id={}", event_id);
    match event_id {
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
        "quit" => {
            let _ = app.emit("app-shutdown", ());
            let app_handle = app.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(500));
                log::info!("[MENU] quit: forced exit fallback");
                let _ = app_handle.exit(0);
            });
        }
        "show_dashboard" => {
            let _ = app.emit("navigate", "dashboard");
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
        "show_logs" => {
            let _ = app.emit("navigate", "logs");
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
        "about" => {
            let _ = app.emit("show-about", ());
        }
        _ => {}
    }
}
