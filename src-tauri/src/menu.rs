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

/// Bounded grace between `app-shutdown` and the unconditional `exit(0)`
/// (issues #383/#415). Gives the frontend's `app_exit` command path time to
/// run its polling drain (`commands/sync.rs::stop_polling_and_join_for_exit`
/// waits up to 2 s for the polling thread) before the forced exit; the
/// staged-update install then runs on `RunEvent::Exit` in `lib.rs` either
/// way. When the frontend is already gone the wait simply elapses and the
/// process still terminates — Quit can never wedge on a dead listener.
pub const SHUTDOWN_GRACE: std::time::Duration = std::time::Duration::from_secs(8);

/// Graceful shutdown shared by the app-menu Quit and the tray Quit arms:
/// emits `app-shutdown` so live listeners can drain (the frontend's
/// `app_exit` stops polling and exits on its own), then polls for the drain
/// acknowledgement — `is_syncing == false`, cleared by the join side once
/// the polling thread lands — with `SHUTDOWN_GRACE` as the upper bound, and
/// exits unconditionally afterwards. A fast drain exits early (plus a short
/// settle so a live frontend's own `exit(0)` wins the race); a dead frontend
/// never clears the flag, so the deadline still forces the exit. The
/// watchdog thread dies with the process when the frontend-driven path wins
/// the race, so the trailing `exit(0)` only fires when nothing else did.
pub fn request_graceful_shutdown(app: &AppHandle) {
    let _ = app.emit("app-shutdown", ());
    let app_handle = app.clone();
    std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + SHUTDOWN_GRACE;
        loop {
            // No managed state means no polling drain to wait for (state was
            // never registered or the frontend is already gone) — treat as
            // drained so Quit exits fast instead of idling the full grace.
            let drained = app_handle
                .try_state::<std::sync::Arc<crate::AppState>>()
                .map(|s| {
                    !s.polling
                        .is_syncing(std::sync::atomic::Ordering::Acquire)
                })
                .unwrap_or(true);
            if drained {
                std::thread::sleep(std::time::Duration::from_millis(300));
                log::info!("[MENU] quit: polling drain acknowledged, exiting");
                break;
            }
            if std::time::Instant::now() >= deadline {
                log::info!("[MENU] quit: shutdown grace elapsed, exiting unconditionally");
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        // `AppHandle::exit` returns `()` on every platform — it handles
        // its own error internally. The `let_unit_value` allow is therefore
        // needed because clippy reads `let _ = unit;` as a no-op let. Do
        // not rewrite this as `if let Err(e) = ...`: that fails to compile
        // everywhere (E0308), which is why the superseded `fix-quit-handler`
        // branch was never merged.
        #[allow(clippy::let_unit_value)]
        {
            let _ = app_handle.exit(0);
        }
    });
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
            request_graceful_shutdown(app);
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

#[cfg(test)]
mod tests {
    /// Issue #415: the app-menu Quit path must give the frontend's `app_exit`
    /// drain time to run (polling stop + staged-update install) instead of a
    /// fixed 500 ms sleep-then-exit. Brace-counted body isolation
    /// (order-independent): do not anchor on the next fn.
    #[test]
    fn quit_arm_uses_bounded_graceful_shutdown() {
        let src = include_str!("menu.rs");
        let sig_idx = src
            .find("fn request_graceful_shutdown(")
            .expect("request_graceful_shutdown must exist");
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
                panic!("unbalanced braces in request_graceful_shutdown");
            }
        };
        let body = &src[body_start + 1..body_end];
        assert!(
            body.contains("\"app-shutdown\""),
            "quit must emit app-shutdown first for graceful listeners"
        );
        assert!(
            body.contains("SHUTDOWN_GRACE"),
            "quit must wait the bounded SHUTDOWN_GRACE, not a fixed 500 ms sleep"
        );
        assert!(
            body.contains("is_syncing"),
            "quit must poll for the polling-drain acknowledgement instead of a blind sleep"
        );
        assert!(
            body.contains("deadline"),
            "quit must bound the acknowledgement wait so a dead frontend still forces exit"
        );
        assert!(
            !body.contains("from_millis(500)"),
            "the fixed 500 ms sleep-then-exit race (issue #415) must stay gone"
        );
        assert!(
            !body.contains("forced exit fallback"),
            "the old 500 ms forced-exit fallback log must stay gone"
        );
        assert!(
            body.contains("unwrap_or(true)"),
            "missing managed state means no drain to wait for — must exit fast, not idle the full grace"
        );
        assert!(
            body.contains(".exit(0)"),
            "quit must still exit unconditionally after the grace elapses"
        );
        assert!(
            super::SHUTDOWN_GRACE.as_secs() >= 3,
            "SHUTDOWN_GRACE must cover the ~2 s polling drain plus margin"
        );
    }

    /// Issue #383: the app-menu Quit handler must stay wired through
    /// `request_graceful_shutdown` so the forced exit cannot be dropped
    /// without this test failing.
    #[test]
    fn quit_handler_routes_through_graceful_shutdown() {
        let src = include_str!("menu.rs");
        let sig_idx = src
            .find("fn handle_app_menu_event(")
            .expect("handle_app_menu_event must exist");
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
                panic!("unbalanced braces in handle_app_menu_event");
            }
        };
        let body = &src[body_start + 1..body_end];
        let quit_pos = body
            .find("ID_QUIT =>")
            .expect("handle_app_menu_event must handle ID_QUIT");
        let tail = &body[quit_pos..];
        assert!(
            tail.contains("request_graceful_shutdown(app)"),
            "ID_QUIT arm must route through request_graceful_shutdown"
        );
    }
}
