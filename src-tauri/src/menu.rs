use tauri::{
    menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder},
    AppHandle, Emitter, Manager, Runtime, WebviewWindow,
};

// Menu item IDs shared by the tray and app menu. The single dispatcher owns
// the click routing; both modules import the same constants so dispatch and
// diagnostics cannot drift onto parallel id definitions.
pub(crate) const ID_SETTINGS: &str = "settings";
pub(crate) const ID_OPEN_LOGS: &str = "open_logs";
pub(crate) const ID_QUIT: &str = "quit";
pub(crate) const ID_SHOW_DASHBOARD: &str = "show_dashboard";
pub(crate) const ID_SHOW_LOGS: &str = "show_logs";
pub(crate) const ID_ABOUT: &str = "about";

/// Show and focus the main window.
fn show_and_focus_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        // Issue #886: report the raise to the mirror the tray dedup key reads.
        crate::tray::note_window_visibility(true);
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
                .map(|s| !s.polling.is_syncing())
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

    // The app's own Quit item, shared by both layouts (see the File menu below).
    let quit_item = MenuItemBuilder::with_id(ID_QUIT, s.menu_quit)
        .accelerator("CmdOrCtrl+Q")
        .build(manager)
        .map_err(|e| e.to_string())?;

    // File menu
    let file_builder = SubmenuBuilder::new(manager, s.menu_file)
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
        .separator();
    // #788 item 2: on macOS Cmd+Q lives in the application submenu, where the OS
    // expects it, so it is left out here — otherwise the bar carries two Quit
    // items with the same accelerator. It is the SAME `ID_QUIT` item wherever it
    // sits, so the click still reaches `request_graceful_shutdown`;
    // `PredefinedMenuItem::quit` is deliberately not used, since it would quit
    // through the OS and skip the graceful state flush.
    #[cfg(not(target_os = "macos"))]
    let file_builder = file_builder.item(&quit_item);
    let file_menu = file_builder.build().map_err(|e| e.to_string())?;

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

    // Build the full menu bar.
    //
    // #788 item 2: on macOS the FIRST submenu is what the OS renders as the
    // application menu — the title in the menu bar, plus the conventional
    // About/Hide/Quit items and Cmd+Q. Installing this bar app-wide without it
    // would title the app menu "File" and drop the standard items, trading one
    // macOS defect for a subtler one. `Menu::default` supplies exactly that
    // submenu, so prepend it there and nowhere else.
    #[cfg(target_os = "macos")]
    {
        // Mirrors the app submenu `tauri::menu::Menu::default` builds, but as a
        // `Submenu` — a macOS menubar `Menu` may only contain `Submenu`s, so
        // `Menu::default`'s `Menu` cannot be nested here. Building it inline also
        // avoids that helper's own File/Edit/View/Window/Help submenus, which
        // would duplicate the ones below and add an English "Window".
        //
        // No `AboutMetadata` / `PredefinedMenuItem::about` here: this crate has
        // its own `ID_ABOUT` in the Help submenu, routed to `emit_about()`. Adding
        // the predefined About as well would give macOS two About items.
        let app_handle = manager.app_handle();
        let pkg_info = app_handle.package_info();
        // `Submenu::with_items` takes `&[&dyn IsMenuItem<R>]`. Binding each entry
        // to that trait object explicitly is required here: the array mixes
        // `PredefinedMenuItem` (services/separator/hide/hide_others) with this
        // crate's own `MenuItem` (`quit_item`), and inference otherwise latches
        // onto the first element's type and rejects the rest. Inline
        // `map_err(...)?` cannot coerce through the unsized trait object, hence
        // the `let` bindings.
        let services_item: &dyn tauri::menu::IsMenuItem<R> =
            &PredefinedMenuItem::services(app_handle, None).map_err(|e| e.to_string())?;
        let sep_1: &dyn tauri::menu::IsMenuItem<R> =
            &PredefinedMenuItem::separator(app_handle).map_err(|e| e.to_string())?;
        let hide_item: &dyn tauri::menu::IsMenuItem<R> =
            &PredefinedMenuItem::hide(app_handle, None).map_err(|e| e.to_string())?;
        let hide_others_item: &dyn tauri::menu::IsMenuItem<R> =
            &PredefinedMenuItem::hide_others(app_handle, None).map_err(|e| e.to_string())?;
        let sep_2: &dyn tauri::menu::IsMenuItem<R> =
            &PredefinedMenuItem::separator(app_handle).map_err(|e| e.to_string())?;
        let quit_ref: &dyn tauri::menu::IsMenuItem<R> = &quit_item;
        let app_items: [&dyn tauri::menu::IsMenuItem<R>; 6] = [
            services_item,
            sep_1,
            hide_item,
            hide_others_item,
            sep_2,
            quit_ref,
        ];
        // Fully qualified rather than imported: `Submenu` is only used by this
        // macOS arm, so a top-level `use` would be an unused import (and a
        // clippy -D warnings failure) on every other platform.
        let app_submenu =
            tauri::menu::Submenu::with_items(app_handle, pkg_info.name.clone(), true, &app_items)
                .map_err(|e| format!("Failed to build the macOS app submenu: {}", e))?;
        MenuBuilder::new(manager)
            .item(&app_submenu)
            .item(&file_menu)
            .item(&edit_menu)
            .item(&view_menu)
            .item(&help_menu)
            .build()
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        // Windows and Linux keep exactly the bar they had before #788.
        MenuBuilder::new(manager)
            .item(&file_menu)
            .item(&edit_menu)
            .item(&view_menu)
            .item(&help_menu)
            .build()
            .map_err(|e| e.to_string())
    }
}

/// Installs a built menu as the application menu bar.
///
/// #788: this must be [`AppHandle::set_menu`], not [`WebviewWindow::set_menu`].
/// The window form is not supported on macOS — its main-thread closure carries
/// only `#[cfg(windows)]` and `#[cfg(any(target_os = "linux", target_os = "bsd"))]`
/// arms, and Tauri's own documentation states the macOS menu is app-wide and
/// directs callers here. Nothing in this crate calls `app.set_menu()` or
/// `Builder::menu()` anywhere else, so the localized File/View/Help bar built by
/// [`build_app_menu`] was never installed on macOS: Mac users saw only Tauri's
/// default app/Edit/Window menu, and `rebuild_app_menu` logged "menu bar
/// relabelled for the new locale" while changing nothing.
///
/// Click routing is unaffected — events arrive through `on_menu_event` either
/// way; only the installation call changes.
fn apply_app_menu<R: Runtime>(
    app: &AppHandle<R>,
    menu: tauri::menu::Menu<R>,
) -> Result<(), String> {
    // `AppHandle::set_menu` hands back the menu it replaced (an `Option`), not
    // `()`; discard it so this keeps the `Result<(), String>` the callers use.
    app.set_menu(menu)
        .map(|_replaced| ())
        .map_err(|e| format!("Failed to set app menu: {}", e))
}

/// Builds and applies the application menu bar for the startup path.
pub fn setup_app_menu(app: &tauri::App, _window: &WebviewWindow) -> Result<(), String> {
    // 4.7.0 (issue #674): install the locale stored in the config before the
    // labels are read. `setup_tray` does the same, so the menu stays correct
    // even when the tray failed to initialise.
    let state = app.state::<std::sync::Arc<crate::AppState>>();
    crate::i18n::install_from_app_state(state.inner());
    let menu = build_app_menu(app)?;
    apply_app_menu(app.handle(), menu)?;
    log::info!("[MENU] setup_app_menu: app menu bar created successfully");
    Ok(())
}

/// Rebuilds the application menu bar for the newly installed locale (issue
/// #674), so switching language relabels the native menu without a restart.
pub fn rebuild_app_menu(app: &AppHandle) -> Result<(), String> {
    let menu = build_app_menu(app)?;
    apply_app_menu(app, menu)?;
    log::info!("[MENU] rebuild_app_menu: app menu bar relabelled for the new locale");
    Ok(())
}

/// Effects performed for an app-menu event.
///
/// Production uses [`AppHandle`]. Keeping the sink at this boundary lets the
/// unknown-event regression drive the real dispatcher and capture the exact
/// message that would be emitted without constructing a GUI runtime.
pub(crate) trait AppMenuEventTarget {
    fn emit_navigate(&self, destination: &str);
    fn show_and_focus(&self);
    fn emit_about(&self);
    fn log_unknown_event(&self, record: &str);
}

impl AppMenuEventTarget for AppHandle {
    fn emit_navigate(&self, destination: &str) {
        let _ = self.emit("navigate", destination);
    }

    fn show_and_focus(&self) {
        show_and_focus_main_window(self);
    }

    fn emit_about(&self) {
        let _ = self.emit("show-about", ());
    }

    fn log_unknown_event(&self, record: &str) {
        log::debug!("{record}");
    }
}

///
/// Handle menu events from the app menu bar.
///
/// Issue #804: only the window-menu-only ids live here. The tray-owned ids
/// (`settings`, `open_logs`, `quit`) are handled by the single dispatcher
/// [`crate::tray::handle_menu_event`], which delegates here for these three —
/// a second copy would double-fire every shared id.
pub(crate) fn handle_app_menu_event(target: &impl AppMenuEventTarget, event_id: &str) {
    match event_id {
        ID_SHOW_DASHBOARD => {
            target.emit_navigate("dashboard");
            target.show_and_focus();
        }
        ID_SHOW_LOGS => {
            target.emit_navigate("logs");
            target.show_and_focus();
        }
        ID_ABOUT => {
            target.emit_about();
            // #788 item 3: the About view can land behind another window, so
            // raise it the way the two navigation arms already do.
            target.show_and_focus();
        }
        _ => {
            target.log_unknown_event(&format!(
                "[MENU] handle_app_menu_event: unknown event_id={}",
                crate::tray::menu_event_id_for_log(event_id)
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{handle_app_menu_event, AppMenuEventTarget};

    /// Issue #918: drive the production unknown-event branch and capture the
    /// record at its logging seam. Reverting the dispatcher to interpolate the
    /// raw event id makes this fail with the leaked value in the message.
    #[test]
    fn unknown_app_menu_event_record_redacts_device_id() {
        #[derive(Default)]
        struct RecordingTarget {
            records: std::cell::RefCell<Vec<String>>,
        }

        impl AppMenuEventTarget for RecordingTarget {
            fn emit_navigate(&self, _destination: &str) {}

            fn show_and_focus(&self) {}

            fn emit_about(&self) {}

            fn log_unknown_event(&self, record: &str) {
                self.records.borrow_mut().push(record.to_owned());
            }
        }

        let device_id = "aB3deviceCredentialValueWithThirtyTwoChars";
        let event_id = format!("devices|{device_id}");
        let target = RecordingTarget::default();

        handle_app_menu_event(&target, &event_id);

        let records = target.records.into_inner();
        assert_eq!(records.len(), 1, "the unknown branch must emit one record");
        let record = &records[0];
        assert!(
            record.contains("handle_app_menu_event: unknown event_id="),
            "the production record shape changed: {record}"
        );
        assert!(!record.contains(device_id), "device id leaked: {record}");
        assert!(
            !record.contains(&event_id),
            "device menu id leaked: {record}"
        );
    }

    /// Issue #415: the app-menu Quit path must give the frontend's `app_exit`
    /// drain time to run (polling stop + staged-update install) instead of a
    /// fixed 500 ms sleep-then-exit. Brace-counted body isolation
    /// (order-independent): do not anchor on the next fn.
    ///
    /// Why a source scan survives here (issue #778 allows exactly this
    /// shape): the invariant is the shutdown SEQUENCE — emit, bounded
    /// poll-for-drain, unconditional exit — and every step needs a live
    /// `AppHandle` (`app.emit`, `try_state`, `app.exit`) that no hermetic
    /// unit test can construct. Driving the fn behaviourally would exit the
    /// test process; asserting the sequence textually at its own body is the
    /// only observable pin. Scoped to `request_graceful_shutdown`'s body so
    /// prose elsewhere cannot satisfy it.
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
    ///
    /// Why a source scan survives here (issue #778 allows exactly this
    /// shape): the invariant is WHICH exit path the Quit decision arm calls,
    /// and the arm needs a live `AppHandle` to run. The decision half IS
    /// covered behaviourally (`tray_click_target_orders_every_arm` pins the
    /// id-to-Quit-decision mapping); what this scan pins is the arm's body,
    /// which no hermetic test can execute. Scoped to the dispatcher's body.
    #[test]
    fn quit_handler_routes_through_graceful_shutdown() {
        // Issue #804: Quit moved to the single dispatcher
        // (`tray::handle_menu_event`) — this handler keeps only the
        // window-menu-only arms, so the graceful-shutdown routing is pinned
        // from the dispatcher side instead.
        let src = include_str!("tray/mod.rs");
        let sig_idx = src
            .find("pub fn handle_menu_event(")
            .expect("handle_menu_event must exist");
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
                panic!("unbalanced braces in handle_menu_event");
            }
        };
        let body = &src[body_start + 1..body_end];
        let quit_pos = body
            .find("TrayClickTarget::Quit =>")
            .expect("handle_menu_event must own the Quit decision (issue #804)");
        let tail = &body[quit_pos..];
        assert!(
            tail.contains("request_graceful_shutdown(app)"),
            "Quit arm must route through request_graceful_shutdown"
        );
    }
}
