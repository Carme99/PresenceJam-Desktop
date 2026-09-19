use tauri::{
    menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder},
    AppHandle, Emitter, Manager, Runtime, WebviewWindow,
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
        // Issue #483: a minimized window stays minimized after show() --
        // unminimize first (mirrors the single-instance raise in lib.rs).
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
    // Issue #592: this changes window visibility, which drives the tray's
    // Show/Hide label — repaint from backend state on a worker (the rebuild
    // may perform blocking Spotify HTTP, issue #587).
    crate::tray::refresh_tray_from_state(app);
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
                .map(|s| !s.polling.is_syncing(std::sync::atomic::Ordering::Acquire))
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

/// Builds the application menu bar (macOS/Windows) from the installed locale.
///
/// Generic over the manager so the same builder serves both the startup path
/// ([`setup_app_menu`], which holds a `&App`) and a live locale change
/// ([`rebuild_app_menu`], which holds an `&AppHandle`) — issue #674.
///
/// Every label comes from [`crate::i18n::current`]; the Edit menu's
/// Undo/Redo/Cut/Copy/Paste/Select All entries are `PredefinedMenuItem`s built
/// with no text, so the platform localizes those itself.
pub fn build_app_menu<R: Runtime, M: Manager<R>>(
    manager: &M,
) -> Result<tauri::menu::Menu<R>, String> {
    let s = crate::i18n::current();

    // File menu
    let file_menu = SubmenuBuilder::new(manager, s.menu_file)
        .item(
            &MenuItemBuilder::with_id(ID_SETTINGS, s.menu_settings)
                .accelerator("CmdOrCtrl+,")
                .build(manager)
                .map_err(|e| e.to_string())?,
        )
        .item(
            &MenuItemBuilder::with_id(ID_OPEN_LOGS, s.open_logs_folder)
                .accelerator("CmdOrCtrl+Shift+L")
                .build(manager)
                .map_err(|e| e.to_string())?,
        )
        .separator()
        .item(
            &MenuItemBuilder::with_id(ID_QUIT, s.menu_quit)
                .accelerator("CmdOrCtrl+Q")
                .build(manager)
                .map_err(|e| e.to_string())?,
        )
        .build()
        .map_err(|e| e.to_string())?;

    // Edit menu (standard macOS clipboard shortcuts for text fields)
    let edit_menu = SubmenuBuilder::new(manager, s.menu_edit)
        .item(&PredefinedMenuItem::undo(manager, None).map_err(|e| e.to_string())?)
        .item(&PredefinedMenuItem::redo(manager, None).map_err(|e| e.to_string())?)
        .separator()
        .item(&PredefinedMenuItem::cut(manager, None).map_err(|e| e.to_string())?)
        .item(&PredefinedMenuItem::copy(manager, None).map_err(|e| e.to_string())?)
        .item(&PredefinedMenuItem::paste(manager, None).map_err(|e| e.to_string())?)
        .item(&PredefinedMenuItem::select_all(manager, None).map_err(|e| e.to_string())?)
        .build()
        .map_err(|e| e.to_string())?;

    // View menu
    let view_menu = SubmenuBuilder::new(manager, s.menu_view)
        .item(
            &MenuItemBuilder::with_id(ID_SHOW_DASHBOARD, s.menu_show_dashboard)
                .accelerator("CmdOrCtrl+1")
                .build(manager)
                .map_err(|e| e.to_string())?,
        )
        .item(
            &MenuItemBuilder::with_id(ID_SHOW_LOGS, s.menu_show_logs)
                .accelerator("CmdOrCtrl+2")
                .build(manager)
                .map_err(|e| e.to_string())?,
        )
        .build()
        .map_err(|e| e.to_string())?;

    // Help menu
    // No accelerator for About — intentional (no standard macOS convention)
    let help_menu = SubmenuBuilder::new(manager, s.menu_help)
        .item(
            &MenuItemBuilder::with_id(ID_ABOUT, s.menu_about)
                .build(manager)
                .map_err(|e| e.to_string())?,
        )
        .build()
        .map_err(|e| e.to_string())?;

    // Build the full menu bar
    MenuBuilder::new(manager)
        .item(&file_menu)
        .item(&edit_menu)
        .item(&view_menu)
        .item(&help_menu)
        .build()
        .map_err(|e| e.to_string())
}

/// Sets a built menu as the window's menu bar.
///
/// Using `window.set_menu()` instead of `app.set_menu()` keeps click events
/// routed through `on_menu_event`. [`Window::set_menu`] marshals the native
/// call onto the main thread itself, so a caller on a command thread is safe.
fn apply_app_menu<R: Runtime>(
    window: &WebviewWindow<R>,
    menu: tauri::menu::Menu<R>,
) -> Result<(), String> {
    window
        .set_menu(menu)
        .map_err(|e| format!("Failed to set window menu: {}", e))?;
    Ok(())
}

/// Builds and applies the application menu bar for the startup path.
pub fn setup_app_menu(app: &tauri::App, window: &WebviewWindow) -> Result<(), String> {
    // 4.7.0 (issue #674): install the locale stored in the config before the
    // labels are read. `setup_tray` does the same, so the menu stays correct
    // even when the tray failed to initialise.
    let state = app.state::<std::sync::Arc<crate::AppState>>();
    crate::i18n::install_from_app_state(state.inner());
    let menu = build_app_menu(app)?;
    apply_app_menu(window, menu)?;
    log::info!("[MENU] setup_app_menu: window menu bar created successfully");
    Ok(())
}

/// Rebuilds the application menu bar for the newly installed locale (issue
/// #674), so switching language relabels the native menu without a restart.
pub fn rebuild_app_menu(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    let menu = build_app_menu(app)?;
    apply_app_menu(&window, menu)?;
    log::info!("[MENU] rebuild_app_menu: menu bar relabelled for the new locale");
    Ok(())
}

/// The action one application-menu id selects.
///
/// The id → action half of the menu handling is a pure function so it can be
/// asserted without an `AppHandle` (issues #761/#778); the effects stay in
/// [`handle_app_menu_event`], which is the only match over this type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppMenuAction {
    /// Show and focus the main window on this frontend view.
    Navigate(&'static str),
    /// Ask the frontend to reveal the logs folder.
    OpenLogsFolder,
    ShowAbout,
    /// Drain the poller and exit — never a bare `exit`, see
    /// [`request_graceful_shutdown`].
    Quit,
}

/// Map an application-menu id to the action it triggers, or `None` for an id
/// this menu never installs.
fn app_menu_action(event_id: &str) -> Option<AppMenuAction> {
    match event_id {
        ID_SETTINGS => Some(AppMenuAction::Navigate("settings")),
        ID_OPEN_LOGS => Some(AppMenuAction::OpenLogsFolder),
        ID_QUIT => Some(AppMenuAction::Quit),
        ID_SHOW_DASHBOARD => Some(AppMenuAction::Navigate("dashboard")),
        ID_SHOW_LOGS => Some(AppMenuAction::Navigate("logs")),
        ID_ABOUT => Some(AppMenuAction::ShowAbout),
        _ => None,
    }
}

/// Handle menu events from the app menu bar.
///
/// The Quit arm is the load-bearing one: it must reach
/// [`request_graceful_shutdown`], which emits `app-shutdown` and waits out the
/// bounded grace, instead of exiting under a live polling thread (#383/#415).
pub fn handle_app_menu_event(app: &AppHandle, event_id: &str) {
    match app_menu_action(event_id) {
        Some(AppMenuAction::Navigate(view)) => {
            let _ = app.emit("navigate", view);
            show_and_focus_main_window(app);
        }
        Some(AppMenuAction::OpenLogsFolder) => {
            let _ = app.emit("open-logs-folder", ());
        }
        Some(AppMenuAction::Quit) => {
            request_graceful_shutdown(app);
        }
        Some(AppMenuAction::ShowAbout) => {
            let _ = app.emit("show-about", ());
        }
        None => {
            log::warn!(
                "[MENU] handle_app_menu_event: unknown event_id={}",
                event_id
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Issue #415: the app-menu Quit path must give the frontend's `app_exit`
    /// drain time to run (polling stop + staged-update install) instead of a
    /// fixed 500 ms sleep-then-exit.
    ///
    /// Source-level by necessity: the body is an emit, a watchdog thread and an
    /// `AppHandle::exit`, so observing it means exiting the test process. The
    /// scan pins the policy in the one place that implements it — no fixed
    /// sleep, a bounded grace, a drain check, an unconditional exit — and the
    /// id → action half is asserted behaviourally below.
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

    /// Issue #383: the Quit action must stay wired through
    /// `request_graceful_shutdown` so the forced exit cannot be dropped
    /// without this test failing.
    ///
    /// Source-level by necessity: the arm's effect needs a live `AppHandle`
    /// (emit + `AppHandle::exit` on the watchdog thread), so the scan pins the
    /// wiring and `app_menu_ids_map_to_their_action` pins which id reaches it.
    #[test]
    fn quit_action_routes_through_graceful_shutdown() {
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
        // The arm the builder wires `ID_QUIT` to, read after the mapping
        // itself — asserted behaviourally by `app_menu_ids_map_to_their_action`.
        let quit_pos = body
            .find("AppMenuAction::Quit)")
            .expect("handle_app_menu_event must handle the Quit action");
        let tail = &body[quit_pos..];
        assert!(
            tail.contains("request_graceful_shutdown(app)"),
            "ID_QUIT arm must route through request_graceful_shutdown"
        );
    }

    /// Issue #761/#778: the menu ids are the only link between the native menu
    /// items the builder installs and what the app actually does, and a typo in
    /// one is invisible at runtime — the item simply stops working. The mapping
    /// is asserted by running it, including an id this menu never installs,
    /// which must not fall through to any action.
    #[test]
    fn app_menu_ids_map_to_their_action() {
        let cases = [
            (ID_SETTINGS, AppMenuAction::Navigate("settings")),
            (ID_OPEN_LOGS, AppMenuAction::OpenLogsFolder),
            (ID_QUIT, AppMenuAction::Quit),
            (ID_SHOW_DASHBOARD, AppMenuAction::Navigate("dashboard")),
            (ID_SHOW_LOGS, AppMenuAction::Navigate("logs")),
            (ID_ABOUT, AppMenuAction::ShowAbout),
        ];
        for (id, action) in cases {
            assert_eq!(app_menu_action(id), Some(action));
        }
        assert_eq!(app_menu_action("not-a-menu-id"), None);
    }
}
