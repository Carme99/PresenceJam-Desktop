use tauri::{
    menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager,
};

pub fn setup_tray(app: &tauri::App) -> Result<(), String> {
    let show_window = MenuItemBuilder::with_id("show_window", "Show Window")
        .build(app)
        .map_err(|e| e.to_string())?;

    let pause_sync = MenuItemBuilder::with_id("pause_sync", "Pause Sync")
        .build(app)
        .map_err(|e| e.to_string())?;

    let separator = PredefinedMenuItem::separator(app).map_err(|e| e.to_string())?;

    let quit = MenuItemBuilder::with_id("quit", "Quit")
        .build(app)
        .map_err(|e| e.to_string())?;

    let menu = MenuBuilder::new(app)
        .items(&[&show_window, &pause_sync, &separator, &quit])
        .build()
        .map_err(|e| e.to_string())?;

    let _tray = TrayIconBuilder::new()
        .tooltip("PresenceJam")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show_window" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "pause_sync" => {
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

    Ok(())
}
