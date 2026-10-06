use super::cli::{
    cli_clear_manual_status_from_disk, cli_command, cli_help_text,
    cli_set_active_profile_from_disk, cli_set_manual_status_from_disk, cli_status_exit_code,
    cli_sync_once_iteration, cli_sync_once_preflight_from_disk, t_cli_profile_active,
    t_cli_profile_active_base, CliCommand, CLEAR_STATUS_FLAG, DAEMON_FLAG, PROFILE_FLAG,
    SERVE_FLAG, SET_STATUS_FLAG, SYNC_ONCE_FLAG,
};
use super::deep_link::{handle_deep_link_from_app, take_pending_deep_link};
use super::state::{apply_token_load_result, AppState};
#[cfg(target_os = "macos")]
use crate::macos_deeplink;
use crate::{
    commands, config, diagnostics, history, menu, polling, serve, token_io, tray, updater_bg,
};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;
use std::thread;
use tauri::{AppHandle, Manager};

/// CLI flag the autostart plugin appends to the launch command.
pub(crate) const MINIMIZED_FLAG: &str = "--minimized";

/// True when a close request on `label` hides the window instead of
/// destroying it (issue #585). Only the main window is close-to-tray.
///
/// Detached Logs/Settings panes clear their `$detachedPanes` badge on
/// `tauri://destroyed` (and "Pop back in" awaits `win.close()`), so hiding
/// one leaves a live-but-invisible window whose `setFocus()` can never bring
/// it back — permanently unreachable until the process restarts. Shares the
/// label predicate with the command guard (`commands::is_main_window_label`)
/// so both agree on which window is *the* window.
fn close_hides_window(label: &str) -> bool {
    crate::commands::is_main_window_label(label)
}

/// Issue #819: the full close-to-tray verdict — the main window hides only
/// when a tray exists to bring it back. With no tray (`setup_tray` failed,
/// e.g. GNOME without the AppIndicator extension) the window must close for
/// real: hiding it would strand a window-less + tray-less session with no
/// reachable way back, and letting the close proceed quits the app (the
/// event loop exits, sync stops). Pure over its inputs so the unit test
/// beside the #585 guard fails pre-gate (hide-on-no-tray) and passes once
/// the close arm consults it.
fn should_hide_on_close(label: &str, tray_available: bool) -> bool {
    close_hides_window(label) && tray_available
}

/// Issue #819: one-off explainer fired when the window closes for real
/// because no tray exists. The close is a real quit — with no tray and no
/// window left the event loop exits (`RunEvent::Exit` runs, sync stops) —
/// so the note says sync stopped and how to get the tray back (install
/// `libayatana-appindicator3` + the GNOME AppIndicator extension), not that
/// anything keeps running.
/// Best-effort: notification delivery must never block or fail the close.
fn notify_no_tray_close(app: &tauri::AppHandle) {
    use tauri_plugin_notification::NotificationExt;
    log::info!("[APP] notify_no_tray_close: no tray — notifying that the close quits the app and sync stops");
    let _ = app
        .notification()
        .builder()
        .title("PresenceJam")
        .body(
            "No system tray is available, so the window closed for real and \
             PresenceJam quit instead of minimizing. Sync has stopped. \
             Install libayatana-appindicator3 and the GNOME AppIndicator \
             extension, then re-launch to get the tray back.",
        )
        .show();
}
/// Issue #922: what `detach_pane` builds for a pane name — the label, the
/// in-app URL, the title and the size, all decided here.
struct DetachedPaneSpec {
    label: &'static str,
    title: &'static str,
    width: f64,
    height: f64,
    url: String,
}

/// The pane table behind `detach_pane`, kept pure so both configured panes and
/// the unknown-name rejection are testable without a live Tauri app.
///
/// `theme` mirrors the child-window theme parameter the store used to append
/// (issue #433). Only the two values the frontend can read out of
/// localStorage are accepted; anything else is treated as absent rather than
/// interpolated into the URL.
fn detached_pane_spec(pane: &str, theme: Option<&str>) -> Result<DetachedPaneSpec, String> {
    let (label, title, width, height) = match pane {
        "logs" => ("logs-detached", "PresenceJam — Logs", 720.0, 520.0),
        "settings" => ("settings-detached", "PresenceJam — Settings", 620.0, 720.0),
        other => return Err(format!("unknown detached pane: {other}")),
    };
    let url = match theme {
        Some("dark") => format!("/detached/{pane}?theme=dark"),
        Some("light") => format!("/detached/{pane}?theme=light"),
        _ => format!("/detached/{pane}"),
    };
    Ok(DetachedPaneSpec {
        label,
        title,
        width,
        height,
        url,
    })
}

/// Issue #922: open (or focus) a detached Logs/Settings window from Rust.
///
/// The window used to be created by `src/lib/stores/detach.ts` through
/// `WebviewWindow`, which required the main window's
/// `core:webview:allow-create-webview-window` grant — a permission that in
/// Tauri 2 carries no URL scope, so any script running in the main window
/// could raise an app-chromed window on an arbitrary origin. Building it here
/// removes that grant: the label, the in-app URL, the title and the size all
/// come from the table above, and a pane name that is not one of the two
/// configured views is rejected outright.
///
/// Idempotent, matching the store's `getByLabel` fast path: an existing window
/// is focused rather than a second one being built under the same label (which
/// Tauri would reject anyway).
#[tauri::command]
fn detach_pane(app: AppHandle, pane: String, theme: Option<String>) -> Result<(), String> {
    let spec = detached_pane_spec(&pane, theme.as_deref())?;
    if let Some(existing) = app.get_webview_window(spec.label) {
        log::info!(
            "[DETACH] detach_pane: focusing the existing {} window",
            spec.label
        );
        return existing
            .set_focus()
            .map_err(|e| format!("failed to focus the {} window: {e}", spec.label));
    }
    log::info!(
        "[DETACH] detach_pane: opening {} at {}",
        spec.label,
        spec.url
    );
    tauri::WebviewWindowBuilder::new(
        &app,
        spec.label,
        tauri::WebviewUrl::App(spec.url.clone().into()),
    )
    .title(spec.title)
    .inner_size(spec.width, spec.height)
    .min_inner_size(400.0, 400.0)
    .center()
    .build()
    .map(|_| ())
    .map_err(|e| format!("failed to open the {} window: {e}", spec.label))
}

/// True when this launch carries the autostart plugin's `--minimized` flag
/// (issue #589). Generic over the argv element type so the parser is
/// unit-testable without touching the real process argv.
fn has_minimized_flag<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    args.into_iter()
        .any(|arg| arg.as_ref() == std::ffi::OsStr::new(MINIMIZED_FLAG))
}

/// Should a second-instance launch bring the existing main window to the
/// front (issue #976)?
///
/// Returns `false` iff the argv carries the autostart plugin's
/// `--minimized` flag — a hidden start must stay hidden even when it
/// reaches an already-running instance. The deep-link forwarding path
/// still runs unconditionally; only the show/unminimize/focus step is
/// gated.
///
/// The argv slice is the one `forward_launch_to_running_instance`
/// receives from the single-instance plugin (the element type is
/// `String`, never `OsString`, on that path), so a `&[String]` shape is
/// both accurate to the call site and ergonomic for unit tests.
pub(crate) fn should_raise_window(argv: &[String]) -> bool {
    !has_minimized_flag(argv.iter())
}

/// The file target's rotation strategy for a clamped `logging.keep_files`.
///
/// **Always `KeepSome`**, including at `1`. The field means "archived log files
/// retained" and the active file is not counted, which is exactly what
/// `KeepSome` implements: its archive pass prunes the dated files down to
/// `keep_files`. `KeepOne` reads like the equivalent at 1 and is not — its
/// rotate branch *deletes* the active log and never calls that pruning pass, so
/// pre-existing archives all survive and the configured retention is ignored.
///
/// `max(1)` guards the plugin's `keep_count - 1` (which underflows at 0);
/// `clamp_logging` already floors the persisted value, this is the belt for a
/// caller that skips the clamp.
fn log_rotation_strategy(keep_files: u32) -> tauri_plugin_log::RotationStrategy {
    tauri_plugin_log::RotationStrategy::KeepSome(keep_files.max(1) as usize)
}

/// Issue #920: the `tauri-plugin-log` file target opens `PresenceJam.log`
/// — and every rotated successor — with `create(true).append(true)` and no
/// `.mode()`, inside a `create_dir_all` directory. Under the usual umask 022
/// that is a 0644 file in a 0755 dir, world-readable on a multi-user box,
/// while `config.json` / `tokens.json` are explicitly 0600 / 0700 for the
/// same threat model. The log holds track titles and artist names, and at
/// Debug level the truncated Graph token-response fragment `poll_teams_auth`
/// writes — so tighten both, best-effort, mirroring `config.rs::config_dir`
/// (0700 dir) and `token_io.rs` (0600 files). Idempotent on an
/// already-tight dir; per-file failures are logged and skipped, never fatal
/// to startup. Windows needs nothing: the default ACL is already user-only.
#[cfg(unix)]
fn tighten_log_permissions(dir: &std::path::Path) {
    if let Err(e) = std::fs::create_dir_all(dir) {
        log::warn!(
            "[APP] could not create log dir '{}': {} — skipping mode tighten",
            dir.display(),
            e
        );
        return;
    }
    match std::fs::metadata(dir) {
        Ok(metadata) => {
            let current_mode = metadata.permissions().mode() & 0o777;
            if current_mode != 0o700 {
                log::info!(
                    "[APP] tightening log dir mode from {:o} to 0700 (issue #920)",
                    current_mode
                );
                if let Err(e) =
                    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
                {
                    log::warn!(
                        "[APP] could not chmod log dir '{}' to 0700: {}",
                        dir.display(),
                        e
                    );
                }
            }
        }
        Err(e) => {
            log::warn!(
                "[APP] could not stat log dir '{}': {} — skipping mode tighten",
                dir.display(),
                e
            );
            return;
        }
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            log::warn!("[APP] could not list log dir '{}': {}", dir.display(), e);
            return;
        }
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        // `PresenceJam*.log*`: the active `PresenceJam.log` plus every dated
        // archive the rotation pass creates itself.
        if !name.starts_with("PresenceJam") || !name.contains("log") {
            continue;
        }
        let path = entry.path();
        let current_mode = match std::fs::metadata(&path) {
            Ok(metadata) => metadata.permissions().mode() & 0o777,
            Err(_) => continue,
        };
        if current_mode != 0o600 {
            log::info!(
                "[APP] tightening log file mode from {:o} to 0600: {} (issue #920)",
                current_mode,
                name
            );
            if let Err(e) = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            {
                log::warn!(
                    "[APP] could not chmod log file '{}' to 0600: {}",
                    path.display(),
                    e
                );
            }
        }
    }
}

/// Clear `create` on every window `tauri.conf.json` declares (issue #679):
/// `App::run` builds one webview per `create = true` entry, so a CLI mode that
/// left this alone would flash a window on screen (and would already have
/// failed on a headless machine). Returns how many windows were suppressed.
fn suppress_config_windows<R: tauri::Runtime>(context: &mut tauri::Context<R>) -> usize {
    let mut suppressed = 0;
    for window in context.config_mut().app.windows.iter_mut() {
        window.create = false;
        suppressed += 1;
    }
    suppressed
}

/// The single-instance callback: raise the already-running window.
///
/// Named rather than an inline closure (issue #679) so the registration site —
/// which CLI mode has to skip — stays one line.
///
/// Deep-link routing (issue #799): on Windows/Linux the single-instance plugin
/// is built with the `deep-link` cargo feature, so a second launch carrying a
/// `presencejam://` URL is already routed through `handle_cli_arguments`,
/// which emits `deep-link://new-url` — the same event the `on_open_url`
/// callback wired in setup listens to. That path invokes `handle_deep_link`
/// exactly once, so this callback must NOT scan argv for URLs and dispatch
/// them again: the duplicate would redeem the same authorization `code`
/// twice, and the second exchange fails with a spurious
/// `spotify-auth-failed`. This callback therefore only raises the window and
/// repaints the tray; it never touches argv URLs. macOS goes through the
/// deep-link plugin's `on_open_url` callback, which is wired in setup.
#[cfg(desktop)]
pub(crate) fn forward_launch_to_running_instance(app: &AppHandle, argv: Vec<String>, _cwd: String) {
    // Issue #976: a second instance launched with `--minimized` (the
    // autostart plugin does this on every login) must not raise the
    // already-running window — the autostart user expects to stay
    // hidden. The tray refresh still runs unconditionally; only the
    // show/unminimize/focus step is gated on the launch intent encoded
    // in argv. (Deep-link argv scanning is gone per #799 — the plugin
    // routes URLs through handle_cli_arguments → on_open_url.)
    if should_raise_window(&argv) {
        // Raise the existing window so the user sees it when a second
        // instance is launched (e.g., double-click the .msi shortcut
        // while the app is running, or a deep-link click from a
        // browser when the app is already open).
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            // Issue #886: the tray's dedup key reads a visibility mirror, so every
            // path that shows the window has to report it — otherwise the raise
            // would be deduped away and the Show/Hide label would keep "Show Window".
            crate::tray::note_window_visibility(true);
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
    } else {
        log::info!(
            "[APP] single_instance: argv carried {MINIMIZED_FLAG}; leaving the running window alone"
        );
    }
    // Issue #592: raising the window changes its visibility, which
    // drives the tray's Show/Hide label — repaint from backend state
    // on a worker (the rebuild may perform blocking Spotify HTTP).
    crate::tray::refresh_tray_from_state(app);
}

/// Issue #818: attach a GUI-subsystem release build to the invoking console
/// before any CLI output. Without this the `--help` / `--status` /
/// `--sync-once` `println!` / `eprintln!` calls on Windows have nowhere to
/// go: `main.rs` sets `windows_subsystem = "windows"` in release, so the
/// standard handles are invalid on startup and the usage text is silently
/// lost.
///
/// Best-effort by design: every native failure falls through and lets the
/// caller print anyway (a redirected pipe or file still works, since the
/// attach is skipped when a handle is already valid). The GUI path never
/// calls this — only the `cli_command()` early-exit arms in `run()`.
#[cfg(target_os = "windows")]
fn attach_parent_console_for_cli() {
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        OPEN_EXISTING,
    };
    use windows::Win32::System::Console::{
        AttachConsole, GetStdHandle, SetStdHandle, ATTACH_PARENT_PROCESS, STD_ERROR_HANDLE,
        STD_OUTPUT_HANDLE,
    };

    // A valid handle already (a pipe, a file, or a debugger console) means
    // there is nothing to re-attach — reopening CONOUT$ would steal the
    // redirection. This keeps `presencejam --status | jq …` working.
    let stdout_valid = unsafe { GetStdHandle(STD_OUTPUT_HANDLE).is_ok() };
    let stderr_valid = unsafe { GetStdHandle(STD_ERROR_HANDLE).is_ok() };
    if stdout_valid && stderr_valid {
        return;
    }
    // No parent console (double-clicked from Explorer, Task Scheduler with
    // no console): nothing to attach to, and the CLI flags exit fast
    // anyway. Swallow the error — printing to nowhere is harmless.
    if unsafe { AttachConsole(ATTACH_PARENT_PROCESS) }.is_err() {
        return;
    }
    // Reopen the parent's screen buffer and point the missing standard
    // handle(s) at it, mirroring the `CONOUT$` recipe (e.g. nu-ansi-term's
    // `enable_ansi_support`). Failure of one handle must not block the
    // other, so each arm is independent and best-effort.
    //
    // `CreateFileW` is `#[cfg(feature = "Win32_Security")]`-gated upstream
    // (it takes a SECURITY_ATTRIBUTES pointer), so the
    // `Win32_Storage_FileSystem` feature alone is not enough — the crate
    // feature set in Cargo.toml must also enable `Win32_Security`.
    let reopen = |handle_id| unsafe {
        // `Param<PCWSTR>` is implemented for `&HSTRING` (not by value),
        // so the name lives in a local the call borrows.
        let name = windows::core::HSTRING::from("CONOUT$");
        let handle = CreateFileW(
            &name,
            (FILE_GENERIC_READ | FILE_GENERIC_WRITE).0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            Default::default(),
            None,
        )?;
        SetStdHandle(handle_id, handle)?;
        windows::core::Result::<()>::Ok(())
    };
    if !stdout_valid {
        let _ = reopen(STD_OUTPUT_HANDLE);
    }
    if !stderr_valid {
        let _ = reopen(STD_ERROR_HANDLE);
    }
    // Rust's stdio resolves the OS handle lazily per write on Windows, so
    // the `println!` / `eprintln!` calls below pick up the handles we just
    // installed — no reopen of `std::io::stdout()` is needed.
}

/// Install the crash-logging panic hook (issue #79).
fn setup_panic_hook() {
    // Set panic hook to log crashes
    std::panic::set_hook(Box::new(|panic_info| {
        let msg = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic".to_string()
        };

        let location = if let Some(loc) = panic_info.location() {
            format!("{}:{}:{}", loc.file(), loc.line(), loc.column())
        } else {
            "unknown location".to_string()
        };

        log::error!("[PANIC] {} at {}", msg, location);
        // Belt-and-braces fallback removed: `eprintln!` writes to stderr, which on
        // macOS release builds is not connected to the parent's log file
        // (`~/Library/Logs/com.presencejam.app/` — `app_log_dir()`, which carries
        // the bundle-id segment since #300, which the pre-#300 path did not have).
        // The `log::error!` above routes through
        // `tauri-plugin-log`, which is the canonical destination for user-visible
        // log lines and the file the `open_logs_folder` command points at. The
        // previous dual-write left a silent failure mode where the panic appeared
        // in a terminal nobody was reading but never in the log file the user could
        // open. See issue #79.
    }));
}

/// Tighten log-dir permissions and spawn the rotation watchdog (issue #920).
fn setup_log_permissions(app: &tauri::App) {
    // Issue #920: the log target opens its files with no `.mode()`
    // and the dir comes from `create_dir_all`, so under umask 022
    // both are world-readable. Tighten now (0700 dir, 0600
    // `PresenceJam*.log*` files); the watchdog below re-tightens
    // after rotation, which creates successor files itself.
    #[cfg(unix)]
    match app.handle().path().app_log_dir() {
        Ok(dir) => {
            tighten_log_permissions(&dir);
            // Rotation creates the successor with the process umask
            // (0644 under 022) — no hook exists in the plugin to
            // tighten at creation, so re-run the same pass every
            // 60 s. `read_dir` + a stat per file is negligible next
            // to the logging itself; the thread dies with the process.
            let watch_dir = dir.clone();
            if let Err(e) = thread::Builder::new()
                .name("log-perm-watchdog".to_string())
                .spawn(move || loop {
                    thread::sleep(std::time::Duration::from_secs(60));
                    tighten_log_permissions(&watch_dir);
                })
            {
                log::warn!("[APP] could not start log-perm watchdog: {}", e);
            }
        }
        Err(e) => {
            log::warn!(
                "[APP] could not resolve log dir: {} — skipping mode tighten",
                e
            );
        }
    }
}

/// Create `AppState`, manage it, replay a buffered deep link, register updater state.
fn setup_state(app: &tauri::App) -> Arc<AppState> {
    let state = Arc::new(AppState::new());
    app.manage(state.clone());
    // Issue #1122: replay a deep link buffered by the unmanaged-state
    // guard in `handle_deep_link`. Past `manage()`, so the guard
    // passes and the normal path (dedup → validation → dispatch)
    // runs. Presence-only log — never the URL contents (AGENTS.md §7).
    // No-op when nothing arrived early. Single replay: `take()` drains.
    if let Some(pending_url) = take_pending_deep_link() {
        log::info!("[DEEP_LINK] replaying buffered callback");
        handle_deep_link_from_app(&pending_url, app.handle().clone());
    }

    // C3(c) "install on quit": register the deferred-update staging
    // state (updater plugin is desktop-only, so this follows suit).
    #[cfg(desktop)]
    updater_bg::manage(app.handle());
    log::info!("[APP] setup: AppState created and managed");
    state
}

/// Prime the keychain cache (issue #69).
fn setup_keychain_cache() {
    // Issue #69: prime the keychain cache once at app start so the
    // polling thread's first iteration doesn't hit the keychain
    // (and on macOS, doesn't show a keychain prompt mid-poll).
    // We do this early so the cache is warm before any
    // `start_syncing` call.
    match crate::keychain::get_spotify_client_secret() {
        Ok(_) => log::info!("[APP] setup: keychain cache primed (Spotify client_secret present)"),
        Err(e) => {
            // Log the underlying reason at debug level for troubleshooting
            // (locked keychain, permission denied, keyring daemon down, etc.)
            // without cluttering info-level output for the common new-user path.
            log::debug!("[APP] setup: keychain access failed: {}", e);
            log::info!("[APP] setup: keychain cache empty (no Spotify client_secret yet — user must Onboard)");
        }
    }
}

/// Load config into `AppState`, honour start-minimized.
fn setup_config(app: &mut tauri::App, state: &Arc<AppState>, cli_mode: bool) {
    // Load config into AppState
    match config::load_config() {
        Ok(cfg) => {
            // #226: wire logging.enabled / log_level into the logger after
            // config load. The mapping lives in `config::apply_log_level`
            // (CfgDiag#4, issue #539) so a later save can re-arm the logger
            // from the same code instead of waiting for a relaunch.
            config::apply_log_level(&cfg.logging);
            let mut config_guard = state.config.get_mut();
            *config_guard = Some(Arc::new(cfg.clone()));
            log::info!("[APP] setup: config loaded into AppState");

            // Handle start_minimized setting. On macOS, also switch
            // the app's activation policy to `Accessory` so the
            // dock icon and menu-bar app menu disappear when the
            // user wants tray-only behavior. Setting the policy on
            // every startup is idempotent and ensures the dock
            // icon matches the saved preference even after a
            // crash-restart. See audit Q4.
            // Issue #589: the autostart plugin launches us with
            // `--minimized`, which used to be passed and parsed
            // nowhere — an autostart user got a window and a
            // taskbar entry thrown up at every login. The flag now
            // joins the config setting so the launch intent is real;
            // it stays exact-match only (never a prefix of some
            // other argument).
            let launched_minimized = has_minimized_flag(std::env::args_os());
            // Issue #679: CLI mode has no window to hide and must not
            // switch the macOS activation policy either — the flag it
            // was asked for has nothing to do with the launch intent.
            if !cli_mode && (cfg.teams.start_minimized || launched_minimized) {
                log::info!(
                    "[APP] setup: starting hidden (config start_minimized={}, {}={})",
                    cfg.teams.start_minimized,
                    MINIMIZED_FLAG,
                    launched_minimized
                );
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                    // Issue #886: report the hide to the tray's mirror.
                    crate::tray::note_window_visibility(false);
                }
                #[cfg(target_os = "macos")]
                {
                    // tauri::AppHandle::set_activation_policy returns () on
                    // success; the underlying call logs its own errors via the
                    // tauri-runtime-wry layer. We deliberately discard the unit
                    // value here rather than wrapping in `if let Err(...)`.
                    let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
                }
            }
        }

        Err(e) => {
            log::warn!("[APP] setup: no config found: {}", e);
        }
    }
}

/// One-shot legacy secret migration (issues #9, #376, #813).
fn setup_secret_migration(app: &tauri::App, state: &Arc<AppState>) {
    // One-shot startup migration: strip plaintext Spotify client_secret
    // from config.json (legacy ≤ v2.5.0) into the OS keychain. Safe to
    // call on every launch; no-op once the field is gone. The `_with_app`
    // variant surfaces a keychain conflict to Settings via a one-time
    // `spotify-secret-conflict` event (issue #376). See audit Q3 and
    // issue #9. The outcome is also persisted on AppState (issue #813):
    // the setup hook runs before any webview has mounted, so the event
    // can never reach Settings' `onMount` listener — `get_sync_status`
    // replays the flag for late mounters instead.
    if config::migrate_legacy_client_secret_with_app(app.handle())
        == config::LegacySecretOutcome::ConflictKeychainDiffers
    {
        state
            .secret_conflict
            .store(true, std::sync::atomic::Ordering::Release);
    }
}

/// Load persisted tokens (issues #65, #140, #840).
fn setup_tokens(app: &tauri::App, state: &Arc<AppState>, cli_mode: bool) {
    // Load persisted tokens (Spotify + Teams) into AppState. We bypass
    // any plugin store for the tokens file and read it directly from
    // `<app-config-dir>/PresenceJam/tokens.json`. GUI startup decrypts
    // ciphertext and migrates legacy plaintext as before (issues #65
    // and #140). Windowless CLI setup uses the side-effect-free
    // headless path so merely starting a CLI mode cannot rewrite
    // tokens.json or chmod its directory (issue #840).
    //
    // The pending_*_auth blobs (PKCE verifier, device code) are no
    // longer persisted to disk; the user re-starts the auth flow
    // after a crash mid-OAuth (cheap UX, and the disk leak is gone).
    let token_read_result = if cli_mode {
        token_io::tokens_file_path_headless()
            .map_err(token_io::TokensLoadError::Corrupt)
            .and_then(|path| {
                token_io::read_tokens_at_path(&path, token_io::TokenReadMode::ReadOnly)
            })
    } else {
        token_io::read_tokens_at(app.handle())
    };
    apply_token_load_result(state, token_read_result);
}

/// `--sync-once` early return (issue #679). Returns `Some(result)` when handled.
fn setup_sync_once(
    app: &tauri::App,
    state: &Arc<AppState>,
    sync_once: bool,
) -> Option<Result<(), Box<dyn std::error::Error>>> {
    // Issue #679: `--sync-once` has everything it needs — the app is
    // built windowless in CLI mode, and the state above is loaded — so
    // run its one poll iteration now and exit. Everything below belongs
    // to the GUI: deep-link registration, the tray icon, the app menu.
    // A CLI one-shot must touch none of those surfaces (and would fail
    // on the menu step, which needs the window CLI mode never creates).
    if sync_once {
        return Some(cli_sync_once_iteration(app, state.clone()));
    }
    None
}

/// `--serve` / `--daemon` dispatch (issues #865, #896).
fn setup_serve_or_daemon(
    app: &tauri::App,
    state: &Arc<AppState>,
    serve_port: Option<Option<u16>>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Issue #865: `--serve[=PORT]` builds the same windowless app and
    // then stays in the event loop. `serve::start_serve` owns the
    // server thread for the lifetime of the process; the runtime
    // loop below keeps the process alive until SIGINT/SIGTERM (or
    // an explicit `app.exit()` from elsewhere — the HTTP layer has
    // no such endpoint by design).
    //
    // Issue #896: `--daemon` shares the same "stays alive in CLI
    // mode" rule but routes to `polling::daemon::run` instead. The
    // dispatcher overloads `serve_port` as a tri-state: `None` is
    // "GUI launch", `Some(None)` is "daemon", `Some(Some(p))` is
    // "serve on port p".
    if let Some(port_opt) = serve_port {
        match port_opt {
            Some(port) => {
                let app_handle = app.handle().clone();
                let state_for_serve = Arc::clone(state);
                if let Err(e) = serve::start_serve(state_for_serve, app_handle, port) {
                    log::error!("[APP] setup: --serve failed to start: {}", e);
                    return Err(Box::new(std::io::Error::other(e)));
                }
                log::info!("[APP] setup: --serve bound; runtime loop will keep the process alive");
            }
            None => {
                // Issue #896: install the supervisor and let the
                // runtime loop block on the daemon. The supervisor
                // owns its own signal handlers and stops the
                // poller on SIGTERM/SIGINT; we exit 0 from
                // `RunEvent::Exit` when the supervisor returns.
                let shutdown = Arc::new(std::sync::atomic::AtomicBool::new(false));
                let app_handle = app.handle().clone();
                let state_for_daemon = Arc::clone(state);
                let shutdown_for_handler = Arc::clone(&shutdown);
                // `daemon::run` installs its own SIGTERM/SIGINT
                // handlers; the same `shutdown` flag is also
                // checked by `RunEvent::Exit` below so a daemon
                // exit path that bypasses the supervisor (e.g. a
                // self-exiting poller) still exits cleanly.
                if let Err(e) =
                    polling::run_daemon(state_for_daemon, app_handle, shutdown_for_handler)
                {
                    log::error!("[APP] setup: --daemon supervisor failed: {}", e);
                    return Err(Box::new(std::io::Error::other(e)));
                }
                // The supervisor returned without a panic — exit
                // 0 immediately; the runtime loop has nothing to
                // add. We deliberately do not enter `app.run`,
                // which would block on the Tauri event loop with
                // no windows / no tray / no menu to drive it.
                log::info!("[APP] setup: --daemon supervisor returned; exiting 0");
                std::process::exit(0);
            }
        }
    }
    Ok(())
}

/// Register global shortcuts below the one-shot return (issues #676, #769, #865).
fn setup_shortcuts(app: &tauri::App, cli_mode: bool) {
    // Global shortcuts (issue #676): register the bindings from the
    // config loaded above. This sits BELOW the `--sync-once` early
    // return since issue #769: a CLI one-shot runs windowless and must
    // touch no GUI surface — which includes taking OS-level
    // accelerator grabs — so the registration belongs to the GUI path
    // only. Issue #865: `--serve` shares the same rule — a
    // headless daemon running on a CI host must not steal Ctrl+Alt+M
    // from whatever the operator is using locally.
    //
    // Deliberately NOT inline: every grab goes through the
    // plugin's `run_on_main_thread`, which blocks until the event loop
    // runs the task — and the event loop starts only once this setup
    // hook returns, so registering here would deadlock the app before
    // its first paint (observed under Xvfb: startup stopped right
    // after `config loaded into AppState`). The worker blocks on that
    // hop instead of the main thread; per-slot failures are reported
    // to Settings and are never fatal.
    //
    // Gated on a loaded config for the reason the block used to live
    // inside the `Ok(cfg)` arm: `register_from_config` falls back to
    // the default bindings when AppState holds no config, so an
    // unguarded call after a failed load would grab accelerators the
    // user never configured. Gated on `!cli_mode` so `--serve` and
    // `--sync-once` skip the registration entirely.
    if !cli_mode {
        let shortcuts_config_loaded = app.state::<Arc<AppState>>().config.get().is_some();
        if shortcuts_config_loaded {
            let shortcut_handle = app.handle().clone();
            tauri::async_runtime::spawn_blocking(move || {
                commands::shortcuts::register_from_config(&shortcut_handle);
            });
        }
    }
}

/// Register deep links (issues #66, #799, #865).
#[cfg(desktop)]
fn setup_deep_links(app: &tauri::App) {
    use tauri_plugin_deep_link::DeepLinkExt;

    // Issue #66 (further mitigation): re-register the
    // `presencejam://` scheme at every launch so a foreign app
    // that pre-registered the scheme gets clobbered by our
    // last-write. The plugin's `register()` writes
    // `HKCU\Software\Classes\<scheme>` on Windows and
    // `~/.local/share/applications/<scheme>.desktop` plus
    // `xdg-mime default` on Linux; it returns
    // `Err(UnsupportedPlatform)` on macOS, where the claim has to
    // go through CoreServices instead — `macos_deeplink` does that
    // in the error arm below. Startup must not block on either
    // path: PKCE verifier in AppState only (#65) remains the
    // cryptographic mitigation — an interceptor can read the
    // `code` from the callback URL but cannot exchange it for
    // tokens.
    //
    // Issue #865: skipped under `--serve` (and `--sync-once`),
    // so a headless daemon never claims the URL scheme away
    // from a real GUI install on the same machine.
    log::info!("[APP] setup: registering deep links");
    if let Err(e) = app.deep_link().register_all() {
        #[cfg(target_os = "macos")]
        {
            // Issue #66: macOS claims a URL scheme through the app
            // bundle's `CFBundleURLTypes`, and LaunchServices gives
            // the *first* claimant priority, so the plugin call
            // above cannot take `presencejam://` back from an app
            // that registered it first. CoreServices'
            // `LSSetDefaultHandlerForURLScheme` writes the user's
            // preferred handler and does override that. The bundle
            // id and the scheme list both come from the same
            // tauri.conf.json the plugin reads, so a scheme added
            // there is re-claimed automatically. Failure is only
            // logged — expected under `tauri dev`, where the
            // process is not an installed bundle.
            log::warn!(
                "[APP] setup: deep_link::register_all unsupported on macOS ({e}); \
                 re-claiming the scheme via LSSetDefaultHandlerForURLScheme"
            );
            let config = app.config();
            let schemes = macos_deeplink::configured_schemes(&config.plugins.0);
            match macos_deeplink::claim(&schemes, &config.identifier) {
                Ok(true) => log::info!(
                    "[APP] setup: macOS scheme re-claimed via \
                     LSSetDefaultHandlerForURLScheme for {schemes:?} \
                     (bundle id {})",
                    config.identifier
                ),
                Ok(false) => log::info!(
                    "[APP] setup: macOS scheme re-claim already performed this \
                     launch; skipping"
                ),
                Err(err) => log::warn!(
                    "[APP] setup: macOS scheme re-claim failed ({err}); falling \
                     back to the #65 PKCE launch-binding defence against scheme \
                     hijack"
                ),
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            // #497: name the missing step. The plugin's Linux
            // branch writes the .desktop file then shells out to
            // `update-desktop-database` + `xdg-mime default`; an
            // ENOENT there aborts before the association, so the
            // file can exist while the scheme stays unassociated.
            log::error!(
                "[APP] setup: Failed to register deep links: {e} \
                 (check `update-desktop-database`/`xdg-mime` presence; \
                 if ~/.local/share/applications/presence-jam-handler.desktop \
                 exists but `xdg-mime query default x-scheme-handler/presencejam` \
                 is empty, run `xdg-mime default presence-jam-handler.desktop \
                 x-scheme-handler/presencejam` manually)"
            );
            // Best-effort fallback: associate directly when the
            // database helper is absent (minimal Linux without a
            // full desktop metapackage). HOME resolves via
            // directories (BaseDirs); unknown home skips quietly.
            let apps_dir = directories::BaseDirs::new()
                .map(|b| b.home_dir().join(".local/share/applications"));
            let db_missing = match &apps_dir {
                // #review-5: a non-zero exit is as missing as a
                // spawn failure — either way the DB was not updated.
                Some(dir) => !std::process::Command::new("update-desktop-database")
                    .arg(dir)
                    .status()
                    .is_ok_and(|s| s.success()),
                None => {
                    log::warn!(
                        "[APP] setup: home dir unknown; skipping deep-link fallback association"
                    );
                    false
                }
            };
            if db_missing {
                match std::process::Command::new("xdg-mime")
                    .args([
                        "default",
                        "presence-jam-handler.desktop",
                        "x-scheme-handler/presencejam",
                    ])
                    .status()
                {
                    Ok(s) if s.success() => {
                        log::info!("[APP] setup: xdg-mime fallback association succeeded")
                    }
                    Ok(s) => log::warn!("[APP] setup: xdg-mime fallback exited with status {s}"),
                    Err(mime_err) => log::warn!(
                        "[APP] setup: xdg-mime fallback failed ({mime_err}); \
                         see SETUP.md Linux prerequisites"
                    ),
                }
            }
        }
    } else {
        log::info!("[APP] setup: deep links registered successfully");
    }
}

/// Tray, menu, start URLs, on_open_url (issues #804, #819).
fn setup_gui_surfaces(app: &tauri::App) {
    #[cfg(desktop)]
    use tauri_plugin_deep_link::DeepLinkExt;
    // Setup system tray
    log::info!("[APP] setup: setting up system tray");
    if let Err(e) = tray::setup_tray(app) {
        log::error!("[APP] setup: Failed to setup system tray: {}", e);
        // Issue #819: record the missing tray on AppState so the
        // CloseRequested arm can gate close-to-tray on it (a
        // hidden window with no tray is unreachable).
        app.state::<Arc<AppState>>()
            .tray_available
            .store(false, std::sync::atomic::Ordering::Release);
    } else {
        log::info!("[APP] setup: System tray initialized successfully");
    }
    // Setup application menu bar using window menu (not app menu).
    // Click events reach the single dispatcher in tray.rs (issue
    // #804) — the tray builder's on_menu_event is global, so no
    // per-window handler is registered here.
    log::info!("[APP] setup: setting up application menu");
    if let Some(window) = app.get_webview_window("main") {
        if let Err(e) = menu::setup_app_menu(app, &window) {
            log::error!("[APP] setup: Failed to setup application menu: {}", e);
        } else {
            log::info!("[APP] setup: Application menu initialized successfully");
        }
    } else {
        log::error!("[APP] setup: could not get main window for menu");
    }
    // Check for deep links on startup
    let start_urls = app.deep_link().get_current();
    log::info!("[APP] setup: checking for start URLs");
    if let Ok(Some(urls)) = start_urls {
        log::info!("[APP] setup: found {} start URL(s)", urls.len());
        for url in urls {
            log::info!(
                "[APP] setup: processing start URL: {}",
                crate::redact::redact_len(url.as_str())
            );
            handle_deep_link_from_app(url.as_str(), app.handle().clone());
        }
    } else {
        log::info!("[APP] setup: no start URLs found");
    }
    // Register deep link callback
    let app_handle = app.handle().clone();
    log::info!("[APP] setup: registering on_open_url callback");
    app.deep_link().on_open_url(move |event| {
        let urls = event.urls();
        log::info!("[APP] on_open_url: received {} URL(s)", urls.len());
        for url in urls {
            log::info!(
                "[APP] on_open_url: processing URL: {}",
                crate::redact::redact_len(url.as_str())
            );
            handle_deep_link_from_app(url.as_str(), app_handle.clone());
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Issue #818: on Windows the release build is a GUI-subsystem
    // executable with no console of its own — re-attach to the invoking
    // console before any CLI arm can print. Parsing argv twice is
    // deliberate: it keeps the `match` below untouched, and the parse is
    // a cheap exact-match scan. No-op on other targets, skipped when the
    // handles are already valid (a pipe or file), and the GUI path never
    // reaches a CLI arm, so this only ever fires for a real CLI run.
    #[cfg(target_os = "windows")]
    if cli_command(std::env::args_os()).is_some() {
        attach_parent_console_for_cli();
    }
    log::info!("[APP] run: ENTRY");
    // Issue #679: the CLI flags are resolved before anything else, and
    // `--help` / `--status` / a credential-less `--sync-once` never reach the
    // builder at all. `sync_once` carries the only case that continues into
    // the GUI builder — and then in CLI mode.
    //
    // Issue #870: `--set-status <message> [--set-status-expiry <minutes>]`
    // and `--clear-status` join the same exit-fast path. They need the
    // token I/O to read the Teams access token (the message is POSTed
    // directly to Graph), but otherwise behave like `--status` — no
    // window, no tray, no single-instance lock.
    // Issue #679 / #865: the CLI flags are resolved before anything else.
    // `sync_once` is `true` for `--sync-once` (still builds the app, but
    // in CLI mode and exits at the end of one iteration); `serve_port`
    // is `Some(p)` for `--serve[=p]` (issue #865, builds the app in CLI
    // mode and keeps the runtime alive serving the HTTP surface). The
    // GUI builder proceeds with both flags off (the common case).
    let (sync_once, serve_port) = match cli_command(std::env::args_os()) {
        Some(CliCommand::Help) => {
            println!("{}", cli_help_text());
            std::process::exit(0);
        }
        Some(CliCommand::Status) => std::process::exit(cli_status_exit_code()),
        Some(CliCommand::SyncOnce) => match cli_sync_once_preflight_from_disk() {
            Ok(()) => (true, None),
            Err(reason) => {
                eprintln!("presencejam: {SYNC_ONCE_FLAG}: {reason}");
                std::process::exit(1);
            }
        },
        Some(CliCommand::SetManualStatus {
            message,
            expiry_minutes,
        }) => {
            if message.trim().is_empty() {
                eprintln!(
                    "presencejam: {SET_STATUS_FLAG}: missing message (pass it as the next argument)"
                );
                std::process::exit(1);
            }
            match cli_set_manual_status_from_disk(&message, expiry_minutes) {
                Ok(()) => std::process::exit(0),
                Err(reason) => {
                    eprintln!("presencejam: {SET_STATUS_FLAG}: {reason}");
                    std::process::exit(1);
                }
            }
        }
        Some(CliCommand::ClearManualStatus) => match cli_clear_manual_status_from_disk() {
            Ok(()) => std::process::exit(0),
            Err(reason) => {
                eprintln!("presencejam: {CLEAR_STATUS_FLAG}: {reason}");
                std::process::exit(1);
            }
        },
        Some(CliCommand::SetActiveProfile { name }) => match cli_set_active_profile_from_disk(name)
        {
            Ok(active) => {
                match active {
                    Some(name) => println!("{}", t_cli_profile_active(&name)),
                    None => println!("{}", t_cli_profile_active_base()),
                }
                std::process::exit(0);
            }
            Err(reason) => {
                eprintln!("presencejam: {PROFILE_FLAG}: {reason}");
                std::process::exit(1);
            }
        },
        // Issue #865: `--serve[=PORT]` needs the same builder as `--sync-once`
        // (an `AppHandle` to subscribe to Tauri events), but it stays alive
        // — `sync_once=false`, `serve_port = Some(p)`. The builder stays in
        // CLI mode (no window/tray/menu/deep-link/single-instance) and the
        // runtime loop runs until interrupted.
        Some(CliCommand::Serve(port)) => match cli_sync_once_preflight_from_disk() {
            Ok(()) => (false, Some(port)),
            Err(reason) => {
                eprintln!("presencejam: {SERVE_FLAG}: {reason}");
                std::process::exit(1);
            }
        },
        // Issue #896: `--daemon` shares the `--sync-once` CLI-mode
        // omissions but stays alive for the SIGTERM/SIGINT supervisor.
        // We overload `serve_port = Some(None)` so the existing
        // `cli_mode = sync_once || serve_port.is_some()` gate is the
        // single source of truth for "GUI surfaces must stay off"; the
        // setup hook then disambiguates `Some(None)` (daemon) from
        // `Some(Some(p))` (serve).
        Some(CliCommand::Daemon) => match cli_sync_once_preflight_from_disk() {
            Ok(()) => (false, Some(None)),
            Err(reason) => {
                eprintln!("presencejam: {DAEMON_FLAG}: {reason}");
                std::process::exit(1);
            }
        },
        None => (false, None),
    };
    // Issue #865: the two CLI modes share the same "GUI surfaces must stay
    // off" rule. `sync_once` and `serve_port` are individually readable so
    // their distinctive setup hooks stay type-safe, but every site that
    // currently branches on `sync_once` reads `cli_mode` instead, so
    // adding a third CLI mode in the future is a one-line change.
    let cli_mode = sync_once || serve_port.is_some();
    let mut builder = tauri::Builder::default();

    // Issue #679: a `--sync-once` run must not put a window on the user's
    // screen for the duration of one poll. The config-declared windows are
    // created by `App::run`, not by `build`, so clearing `create` here keeps
    // this launch windowless without touching the GUI's own config.
    // Issue #865: `--serve` shares the same windowless rule — the HTTP
    // surface is the whole point of the launch.
    let mut context = tauri::generate_context!();
    if cli_mode {
        suppress_config_windows(&mut context);
    }
    #[cfg(desktop)]
    {
        use tauri_plugin_single_instance::init as single_instance_init;

        // Issue #679: a `--sync-once` run must not take the single-instance
        // lock. With it registered, a CLI run while the app is already open
        // would be forwarded to the running instance as a "second launch" and
        // exit 0 without polling anything. Issue #865: `--serve` must not
        // take the lock either — a systemd-managed daemon launches via
        // `ExecStart=` every restart, and stealing the lock from a stale
        // GUI process would mis-attribute the SIGTERM.
        if !cli_mode {
            builder = builder.plugin(single_instance_init(forward_launch_to_running_instance));
        }

        builder = builder.plugin(tauri_plugin_deep_link::init());
        log::info!("[APP] run: deep_link plugin registered");

        builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
        log::info!("[APP] run: updater plugin registered");
    }
    // 4.7.0 (S5): the rotating file target is configured from `logging.*`
    // before the plugin is built, which is the only point the plugin offers —
    // Tauri's `setup` hook (and therefore `config::load_config`) has not run
    // yet. `logging_config_for_startup` reads just that section, with no
    // keychain probe; a change to size/retention takes effect at the next
    // launch, while `logging.enabled`/`log_level` keep their immediate
    // `apply_log_level` path.
    let startup_logging = config::logging_config_for_startup();
    let log_rotation = log_rotation_strategy(startup_logging.keep_files);
    let built = builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        // Global shortcuts (issue #676): the feature whose whole point is
        // working while the window is hidden. The grabs themselves are
        // registered in setup from the persisted config — never here, and
        // never fatally: a desktop that refuses a grab reports it per slot in
        // Settings instead of failing startup.
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .plugin(tauri_plugin_log::Builder::new()
            .target(tauri_plugin_log::Target::new(
                tauri_plugin_log::TargetKind::Stdout,
            ))
            .target(tauri_plugin_log::Target::new(
                tauri_plugin_log::TargetKind::LogDir { file_name: Some("PresenceJam".into()) },
            ))
            .target(tauri_plugin_log::Target::new(
                tauri_plugin_log::TargetKind::Webview,
            ))
            .max_file_size(startup_logging.max_file_size_mb as u128 * 1024 * 1024)
            .rotation_strategy(log_rotation)
            .build())
        .setup(move |app| {
            setup_panic_hook();
            setup_log_permissions(app);

            log::info!("[APP] setup: ENTRY");

            let state = setup_state(app);
            setup_keychain_cache();
            setup_config(app, &state, cli_mode);
            setup_secret_migration(app, &state);
            setup_tokens(app, &state, cli_mode);
            if let Some(result) = setup_sync_once(app, &state, sync_once) {
                return result;
            }
            setup_serve_or_daemon(app, &state, serve_port)?;
            setup_shortcuts(app, cli_mode);
            #[cfg(desktop)]
            if !cli_mode {
                setup_deep_links(app);
                setup_gui_surfaces(app);
            }

            log::info!("[APP] setup: PresenceJam {} started successfully", env!("CARGO_PKG_VERSION"));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::config::load_config,
            commands::config::save_config,
            commands::config::update_config,
            commands::config::import_working_hours,
            commands::config::set_locale,
            commands::spotify_auth::start_spotify_auth,
            commands::spotify_auth::start_spotify_reconnect,
            commands::spotify_auth::reconnect_spotify_session,
            commands::spotify_auth::complete_spotify_auth_manual,
            commands::spotify_auth::refresh_spotify,
            commands::spotify_auth::is_spotify_client_secret_set,
            commands::teams_auth::start_teams_auth_device_code,
            commands::teams_auth::poll_teams_auth,
            commands::teams_auth::refresh_teams,
            commands::teams_auth::get_teams_granted_scopes,
            commands::teams_auth::cancel_teams_auth_poll,
            commands::sync::start_syncing,
            commands::sync::stop_syncing,
            commands::sync::get_sync_status,
            commands::sync::refresh_status,
            commands::sync::app_exit,
            detach_pane,
            commands::shortcuts::register_shortcuts,
            commands::shortcuts::unregister_shortcuts,
            commands::shortcuts::validate_shortcut,
            commands::window::show_window,
            commands::window::set_autostart_enabled,
            commands::window::open_logs_folder,
            commands::window::open_external_url,
            commands::onboarding::is_onboarding_complete,
            commands::onboarding::complete_onboarding,
            commands::config::export_config,
            commands::config::import_config,
            commands::onboarding::reconnect_spotify,
            commands::onboarding::reconnect_teams,
            commands::misc::preview_status,
            commands::misc::update_tray_menu_state,
            commands::misc::relaunch_app,
            commands::misc::reset_local_token_storage,
            commands::logs::get_recent_logs,
            updater_bg::check_for_update,
            updater_bg::stage_deferred_update,
            updater_bg::clear_failed_update_install,
            updater_bg::cancel_deferred_update,
            commands::playback::get_spotify_granted_scopes,
            diagnostics::get_diagnostics_snapshot,
            diagnostics::save_diagnostics_snapshot,
            commands::status::set_manual_status,
            commands::status::clear_manual_status_command,
            commands::status::load_manual_status_command,
            commands::playback::set_volume,
            commands::playback::seek,
            history::get_presence_history,
            commands::rules::explain_rules,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Issue #585: close-to-tray applies to the main window only.
                // A detached Logs/Settings pane must really close — its
                // badge clears on `tauri://destroyed` — so non-main windows
                // fall through to the platform's normal destroy path.
                if !close_hides_window(window.label()) {
                    log::info!(
                        "[APP] window_event: CloseRequested on detached window '{}', closing it",
                        window.label()
                    );
                    return;
                }
                // Issue #819 (extends #927): a session whose tray failed to
                // initialise has no reachable way back to a hidden window, so
                // close-to-tray must not engage — the close proceeds for real,
                // the event loop exits and sync stops, and a one-off
                // notification says so plus how to get the tray back. Gated
                // on the availability recorded on AppState at setup (read via
                // `try_state`, with the live `tray_available()` accessor as
                // the fallback for a window firing before setup managed
                // state), so the verdict is unit-testable beside the #585
                // guard.
                let tray_available = window
                    .app_handle()
                    .try_state::<Arc<AppState>>()
                    .map(|s| {
                        s.tray_available
                            .load(std::sync::atomic::Ordering::Acquire)
                    })
                    .unwrap_or_else(crate::tray::tray_available);
                if !should_hide_on_close(window.label(), tray_available) {
                    log::warn!(
                        "[APP] window_event: CloseRequested with no tray — closing instead of hiding"
                    );
                    notify_no_tray_close(window.app_handle());
                    return;
                }
                log::info!("[APP] window_event: CloseRequested received, hiding window");
                let _ = window.hide();
                // Issue #886: the hide has to reach the tray's visibility mirror,
                // which is what the dedup key is built from.
                crate::tray::note_window_visibility(false);
                api.prevent_close();
            }
        })
        .build(context);
    // Issue #417: a build failure (missing icon, bad capability, plugin
    // init) must not panic the release binary with `.expect` — log the
    // cause and exit non-zero. No panic backtrace, but the OS launcher
    // still sees the failure via the exit code and the log tail.
    let app = match built {
        Ok(app) => app,
        Err(e) => {
            log::error!("[APP] run: failed to build tauri application: {}", e);
            // Issue #947: tauri-plugin-log's file target buffers, so exit(1)
            // without flushing would drop the error above. Flush before the
            // exit, matching the CLI failure path's flush-before-exit pattern.
            log::logger().flush();
            std::process::exit(1);
        }
    };
    app.run(move |app, event| {
        // C3(c) "install on quit": both real exit paths (the shared
        // request_graceful_shutdown in menu.rs backing tray + app-menu
        // Quit, and the app_exit command) funnel into AppHandle::exit,
        // which fires RunEvent::Exit once the event loop has finished —
        // the safe point to apply a staged update (the plugin requires
        // the app to be quitting on Windows).
        // Issue #679: a `--sync-once` run skips both hooks. Installing a
        // staged update is a GUI decision (and the user is not quitting an
        // app), and the presence cleanup would wipe the very status the
        // one-shot was asked to write.
        // Issue #865: a `--serve` run skips the staged update (still a GUI
        // decision) but keeps the presence cleanup — the daemon had an
        // armed presence session and SIGTERM is a clean exit, so the
        // status it advertised on Teams needs the same Paused placeholder
        // every other quit performs.
        #[cfg(desktop)]
        if matches!(event, tauri::RunEvent::Exit) && !sync_once {
            // Order is load-bearing (finding #636, issue #636): the staged
            // update is applied FIRST so a Graph round-trip can never delay an
            // install — and on Windows, where the installer exits the process
            // without returning, an update-driven quit never reaches the
            // presence cleanup at all. On every other exit the cleanup then
            // clears an armed presence session (bounded by its own 3-second
            // client) and replaces a leftover playing status with the
            // short-lived "Paused" placeholder.
            if serve_port.is_none() {
                updater_bg::install_pending_on_exit(app);
            }
            polling::clear_presence_on_exit(app);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Regression guard for issue #66: a future contributor must not
    /// re-gate `app.deep_link().register_all()` to `#[cfg(windows)]`
    /// alone. Per-launch re-registration of the `presencejam://`
    /// scheme is required on Windows AND Linux to defend against a
    /// foreign app pre-registering the scheme. macOS is handled by
    /// `macos_deeplink` inside the call site's error arm (see
    /// `test_macos_deeplink_reclaim_is_wired`) — do NOT reintroduce the
    /// Windows-only gate.
    #[test]
    fn test_register_all_not_gated_to_windows_only() {
        let source = include_str!("app.rs");
        let needle = "app.deep_link().register_all()";
        // Capture ±10 lines of context around the call site.
        let byte_offset = source.find(needle).unwrap_or_else(|| {
            panic!(
                "no call to `{}` found in app.rs — the re-registration site \
                     must remain in the desktop setup block",
                needle
            )
        });
        let line_start_byte = source[..byte_offset]
            .rfind('\n')
            .map(|i| i + 1)
            .unwrap_or(0);
        let mut line_start = line_start_byte;
        for _ in 0..10 {
            if line_start == 0 {
                break;
            }
            line_start = source[..line_start - 1]
                .rfind('\n')
                .map(|i| i + 1)
                .unwrap_or(0);
        }
        let match_end = byte_offset + needle.len();
        let mut line_end = match_end;
        for _ in 0..10 {
            if line_end >= source.len() {
                break;
            }
            line_end = source[line_end..]
                .find('\n')
                .map(|i| line_end + i + 1)
                .unwrap_or(source.len());
        }
        let window = &source[line_start..line_end];
        assert!(
            !window.contains("#[cfg(windows)]"),
            "Regression: `{}` is gated to Windows only. Issue #66 \
                 requires per-launch re-registration on Windows AND Linux (and \
                 the CoreServices re-claim on macOS). Do NOT reintroduce \
                 `#[cfg(windows)]` around this call. Offending context:\n{}",
            needle,
            window
        );
    }

    /// Regression guard for the macOS half of issue #66: the CoreServices
    /// re-claim must stay attached to `register_all()`'s failure arm and
    /// stay gated to macOS. Deleting it silently restores the pre-4.6
    /// state where `presencejam://` could be intercepted by whichever app
    /// registered it first, and running it unconditionally would mean
    /// linking CoreServices on Windows/Linux.
    #[test]
    fn test_macos_deeplink_reclaim_is_wired() {
        let lib_src = include_str!("lib.rs");
        assert!(
            lib_src.contains("pub mod macos_deeplink;"),
            "the macos_deeplink module must stay registered in lib.rs"
        );
        let source = include_str!("app.rs");

        let needle = "macos_deeplink::claim(";
        let byte_offset = source.find(needle).unwrap_or_else(|| {
            panic!(
                "no call to `{}` found in app.rs — the macOS scheme re-claim \
                     must remain in the deep-link setup block",
                needle
            )
        });
        let window_start = byte_offset.saturating_sub(2_000);
        let window_end = (byte_offset + 2_000).min(source.len());
        let window = &source[window_start..window_end];

        assert!(
            window.contains("#[cfg(target_os = \"macos\")]"),
            "the CoreServices re-claim must be `#[cfg(target_os = \"macos\")]` — it \
                 pulls in macOS-only dependencies and must not be compiled or linked \
                 on other targets. Offending context:\n{}",
            window
        );
        assert!(
            window.contains("app.deep_link().register_all()"),
            "the CoreServices re-claim must be reached from \
                 `register_all()`'s failure arm, not from a second call site. \
                 Offending context:\n{}",
            window
        );
    }

    /// Issue #417: `run()` must never `.expect` on the Tauri build in
    /// production — a build failure must log and exit non-zero instead of
    /// panicking the release binary. Brace-counted body isolation
    /// (order-independent): do not anchor on the next fn.
    #[test]
    fn test_run_build_failure_logs_and_exits() {
        let source = include_str!("app.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("app.rs has no #[cfg(test)] mod tests block");
        let after_sig = prod_source
            .split("pub fn run()")
            .nth(1)
            .expect("run definition not found");
        let open = after_sig.find('{').expect("run has no opening brace");
        let mut depth = 0usize;
        let mut end = None;
        for (i, ch) in after_sig[open..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(open + i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        let body = &after_sig[open..end.expect("run body never closed")];
        assert!(
            !body.contains(".expect("),
            "run() must not .expect on the Tauri build (issue #417)"
        );
        assert!(
            body.contains("std::process::exit(1)"),
            "run() build failure must exit non-zero"
        );
        assert!(
            body.contains("failed to build tauri application"),
            "run() build failure must log the cause"
        );
    }

    /// Issue #585: only the main window is close-to-tray. Detached panes
    /// clear their `$detachedPanes` badge on `tauri://destroyed`, so hiding
    /// one strands a live-but-invisible window that no `setFocus()` can
    /// bring back.
    #[test]
    fn test_close_hides_window_label_guard() {
        assert!(
            close_hides_window("main"),
            "the main window must stay close-to-tray"
        );
        for detached in ["logs-detached", "settings-detached", "other", ""] {
            assert!(
                !close_hides_window(detached),
                "window `{}` must really close, not hide",
                detached
            );
        }
        // The handler must actually consult the guard before hiding, and still
        // prevent the close for the main window. Anchored on the arm's own
        // statements rather than a fixed byte window: the arm grew with the
        // #927 no-tray guard and the #886 visibility report, and a byte window
        // would silently stop covering `api.prevent_close()` when it does.
        let source = include_str!("app.rs");
        let arm = source
            .find("tauri::WindowEvent::CloseRequested")
            .expect("app.rs must handle WindowEvent::CloseRequested");
        let tail = &source[arm..];
        let guard = tail
            .find("close_hides_window(window.label())")
            .expect("the CloseRequested arm must guard on the window label (issue #585)");
        let hide = tail
            .find("window.hide()")
            .expect("the main window must still be hidden (close-to-tray)");
        let prevent = tail
            .find("api.prevent_close()")
            .expect("the main window must still prevent the close (close-to-tray)");
        assert!(
            guard < hide && hide < prevent,
            "the arm must guard on the label, hide, and then prevent the close"
        );
    }

    /// Issue #819: the close-to-tray verdict needs the tray half too. A
    /// `setup_tray` failure (GNOME without the AppIndicator extension, or a
    /// host missing libayatana-appindicator3) leaves no way back to a hidden
    /// window, so the main window must close for real — and the arm plus the
    /// setup hook must actually consult/record it, not just define it.
    #[test]
    fn test_should_hide_on_close_gates_on_tray() {
        // Pure predicate: main + tray hides; main without a tray closes for
        // real; detached panes never hide regardless of the tray.
        assert!(
            should_hide_on_close("main", true),
            "main window with a tray must stay close-to-tray"
        );
        assert!(
            !should_hide_on_close("main", false),
            "main window with no tray must close for real (issue #819)"
        );
        for detached in ["logs-detached", "settings-detached", "other", ""] {
            assert!(
                !should_hide_on_close(detached, true),
                "detached window `{}` must really close even with a tray",
                detached
            );
            assert!(
                !should_hide_on_close(detached, false),
                "detached window `{}` must really close without a tray",
                detached
            );
        }
        // A fresh AppState assumes a tray until setup says otherwise, so
        // unit-constructed states and CLI/daemon paths keep the established
        // hide-on-close behaviour.
        assert!(
            AppState::new()
                .tray_available
                .load(std::sync::atomic::Ordering::Acquire),
            "AppState must default to tray-available until setup records a failure"
        );
        // The wiring: the CloseRequested arm must consult the predicate (not
        // just the #585 label guard plus the #927 live accessor), and the
        // setup hook must record the `setup_tray` failure onto AppState. The
        // arm assertion fails pre-fix — the arm never named
        // `should_hide_on_close` — and the setup assertion fails pre-fix —
        // the setup hook only logged the error.
        let source = include_str!("app.rs");
        let arm = source
            .find("tauri::WindowEvent::CloseRequested")
            .expect("app.rs must handle WindowEvent::CloseRequested");
        let tail = &source[arm..];
        // Bound to the arm body (through `api.prevent_close()`): an
        // unbounded tail would also cover this test module's own string
        // literals and pass vacuously.
        let prevent = tail
            .find("api.prevent_close()")
            .expect("the main window must still prevent the close (close-to-tray)");
        let arm_body = &tail[..prevent];
        assert!(
            arm_body.contains("should_hide_on_close("),
            "the CloseRequested arm must gate on should_hide_on_close (issue #819)"
        );
        assert!(
            arm_body.contains("notify_no_tray_close("),
            "the no-tray path must explain the real close (issue #819)"
        );
        let setup = source
            .find("tray::setup_tray(")
            .expect("setup must still build the tray for the GUI");
        let setup_tail = &source[setup..setup + 800.min(source.len() - setup)];
        assert!(
            setup_tail.contains("tray_available"),
            "setup must record the setup_tray result on AppState (issue #819)"
        );
    }

    /// Issue #589: the autostart plugin passes `--minimized`, which must be
    /// parsed rather than silently ignored — and matched exactly, so a
    /// future argument that merely starts with the flag is not mistaken
    /// for it.
    #[test]
    fn test_has_minimized_flag_parses_autostart_arg() {
        assert!(
            has_minimized_flag(vec!["presencejam.exe", "--minimized"]),
            "the autostart argv must be recognised"
        );
        assert!(
            has_minimized_flag(vec!["presencejam".to_string(), MINIMIZED_FLAG.to_string()]),
            "OsString argv elements must be recognised too"
        );
        assert!(
            !has_minimized_flag(vec!["presencejam.exe"]),
            "a plain launch must not start hidden"
        );
        assert!(
            !has_minimized_flag(Vec::<String>::new()),
            "an empty argv must not start hidden"
        );
        assert!(
            !has_minimized_flag(vec!["--minimized-please"]),
            "the flag is matched exactly, never as a prefix"
        );
    }

    /// Issue #976: the second-instance callback must honour `--minimized`
    /// — a hidden launch must not raise the already-running window.
    /// `should_raise_window` is the single decision point shared by the
    /// show/unminimize/focus sequence; a `false` here means the window
    /// stays exactly as the autostart user left it.
    #[test]
    fn test_should_raise_window_respects_minimized_flag() {
        // A plain GUI launch must always raise the running window.
        assert!(
            should_raise_window(&["presencejam".to_string()]),
            "a launch without {MINIMIZED_FLAG} must raise the running window",
        );
        // Autostart-shaped launch (exe path + the flag) must stay hidden.
        assert!(
            !should_raise_window(&["presencejam.exe".to_string(), MINIMIZED_FLAG.to_string()]),
            "the autostart argv shape must not raise the running window",
        );
        // A launch that carries both the flag and a deep link must still
        // forward the link, so the gate is "don't raise" — not "ignore the
        // launch entirely".
        assert!(
                !should_raise_window(&[
                    "presencejam".to_string(),
                    MINIMIZED_FLAG.to_string(),
                    "presencejam://join/abc123".to_string(),
                ]),
                "a minimized launch with a deep link must not raise, but the link still needs forwarding",
            );
        // Same shape, flag before the deep link — flag position in argv
        // is not part of the contract.
        assert!(
            !should_raise_window(&[
                "presencejam".to_string(),
                MINIMIZED_FLAG.to_string(),
                "presencejam://x".to_string(),
            ]),
            "should_raise_window is independent of the flag's argv position",
        );
    }

    /// 4.7.0 (S5, #673): every clamped `keep_files` maps to `KeepSome`, at `1`
    /// included. `KeepOne` looks like the equivalent at 1 but deletes the active
    /// log without ever pruning the dated archives, so `keep_files = 1` kept
    /// every existing archive — the field's documented retention ("archived
    /// log files retained") was not honoured at all.
    #[test]
    fn test_log_rotation_strategy_always_keeps_some() {
        for keep in 1..=20u32 {
            assert!(
                matches!(
                    log_rotation_strategy(keep),
                    tauri_plugin_log::RotationStrategy::KeepSome(n) if n == keep as usize
                ),
                "keep_files={keep} must rotate with KeepSome({keep})"
            );
        }
        // Below the clamp floor the plugin's `keep_count - 1` would underflow.
        assert!(
            matches!(
                log_rotation_strategy(0),
                tauri_plugin_log::RotationStrategy::KeepSome(1)
            ),
            "a 0 that slipped past clamp_logging must still floor at KeepSome(1)"
        );
    }

    /// Issue #920: the log dir must be 0700 and every `PresenceJam*.log*`
    /// file 0600 on Unix — the same threat model the `tokens.json` 0600
    /// assertion (issue #263) covers. Fails pre-fix: no production path set
    /// either mode before this slice, so `tighten_log_permissions` did not
    /// exist and this test did not compile.
    #[cfg(unix)]
    #[test]
    fn test_tighten_log_permissions_sets_0700_0600() {
        use std::os::unix::fs::PermissionsExt;
        let base =
            std::env::temp_dir().join(format!("presencejam-test-logperms-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        // Simulate the pre-fix on-disk state: a umask-022 `create_dir_all`
        // dir (0755) holding a loose active log plus a rotated archive.
        std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o755)).unwrap();
        for name in ["PresenceJam.log", "PresenceJam.2026-09-21-00-00-00.log"] {
            let path = base.join(name);
            std::fs::write(&path, "x").unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        }
        // A non-log file in the same dir must be left alone.
        let other = base.join("presence-history.jsonl");
        std::fs::write(&other, "x").unwrap();
        std::fs::set_permissions(&other, std::fs::Permissions::from_mode(0o644)).unwrap();

        tighten_log_permissions(&base);

        let dir_mode = std::fs::metadata(&base).unwrap().permissions().mode() & 0o777;
        assert_eq!(dir_mode, 0o700, "log dir must be 0700, got {:o}", dir_mode);
        for name in ["PresenceJam.log", "PresenceJam.2026-09-21-00-00-00.log"] {
            let mode = std::fs::metadata(base.join(name))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600, "{name} must be 0600, got {mode:o}");
        }
        let other_mode = std::fs::metadata(&other).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            other_mode, 0o644,
            "non-log files must be left alone, got {other_mode:o}"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Issue #920: the tighten must actually run at startup — and keep
    /// running after rotation — not just exist as a dead helper. Guards the
    /// `setup` call site and the watchdog spawn against a future refactor
    /// dropping either.
    #[test]
    fn test_setup_tightens_log_permissions_and_watches_rotation() {
        let source = include_str!("app.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("app.rs has no #[cfg(test)] mod tests block");
        let setup_body = body_of(prod_source, ".setup(move |app|");
        assert!(
            setup_body.contains("setup_log_permissions"),
            "setup must call the log-permission helper at startup (issue #920)"
        );
        let perm_body = body_of(prod_source, "fn setup_log_permissions(");
        assert!(
                perm_body.contains("tighten_log_permissions")
                    && perm_body.contains("log-perm-watchdog"),
                "setup_log_permissions must tighten the log dir/files and spawn the rotation re-tighten watchdog (issue #920)"
            );
    }

    /// Issue #679: the flags must be reachable only as an alternative to the
    /// GUI, and `--sync-once` must reach its one-shot before any GUI surface is
    /// set up. That ordering is what keeps a bare launch on the old path
    /// (asserted by the parser tests above) and a CLI run windowless.
    #[test]
    fn test_cli_flags_short_circuit_before_the_gui_is_built() {
        let source = include_str!("app.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("app.rs has no #[cfg(test)] mod tests block");

        let run_body = body_of(prod_source, "pub fn run()");
        let dispatch = run_body
            .find("cli_command(std::env::args_os())")
            .expect("run() must consult the CLI parser (issue #679)");
        let builder = run_body
            .find("tauri::Builder::default()")
            .expect("run() must still build the Tauri app");
        assert!(
            dispatch < builder,
            "the CLI dispatch must precede the Tauri builder: a flag handled \
                 after the builder exists has already created a runtime, which is \
                 exactly what cannot happen headless (issue #679)"
        );
        assert!(
            run_body.contains("std::process::exit(cli_status_exit_code())"),
            "--status must print and exit without building an app (issue #679)"
        );
        assert!(
            run_body.contains("suppress_config_windows(&mut context)"),
            "--sync-once must suppress the config-declared windows (issue #679)"
        );

        // setup_* helper shape (issue #757): the closure wires helpers;
        // ordering lives in the closure body and inside the helpers.
        let setup_body = body_of(prod_source, ".setup(move |app|");
        let one_shot = setup_body
            .find("setup_sync_once(app, &state, sync_once)")
            .expect("setup must hand --sync-once its helper (issue #679)");
        let surfaces = setup_body
            .find("setup_gui_surfaces(app)")
            .expect("setup must still build the tray/menu surfaces for the GUI");
        assert!(
            one_shot < surfaces,
            "the --sync-once early return must come before the GUI surfaces \
                 so a CLI run registers no tray icon (issue #679)"
        );
        assert!(
            !setup_body[..one_shot].contains("register_all()"),
            "a CLI run must not re-register the deep-link scheme (issue #679)"
        );
        // Issue #769: the global-shortcut registration is a GUI surface as
        // well — it takes OS-level accelerator grabs — so it must sit below
        // the `--sync-once` early return too, not merely below the tray.
        let shortcuts = setup_body
            .find("setup_shortcuts(app, cli_mode)")
            .expect("setup must still register the configured shortcuts for the GUI");
        assert!(
            one_shot < shortcuts,
            "the --sync-once early return must come before the global-shortcut \
                 registration so a CLI run grabs no accelerator (issue #769)"
        );
        // The helpers themselves keep the GUI-only contents: sync-once
        // returns the iteration, surfaces own tray + deep links.
        let sync_once_body = body_of(prod_source, "fn setup_sync_once(");
        assert!(
            sync_once_body.contains("cli_sync_once_iteration("),
            "setup_sync_once must run the one-shot iteration (issue #679)"
        );
        let surfaces_body = body_of(prod_source, "fn setup_gui_surfaces(");
        assert!(
            surfaces_body.contains("tray::setup_tray("),
            "setup_gui_surfaces must still build the tray for the GUI"
        );
        let deep_body = body_of(prod_source, "fn setup_deep_links(");
        assert!(
            deep_body.contains("register_all()"),
            "setup_deep_links must still re-register the scheme (issue #66)"
        );
        let shortcuts_body = body_of(prod_source, "fn setup_shortcuts(");
        assert!(
            shortcuts_body.contains("register_from_config("),
            "setup_shortcuts must still register the configured shortcuts for the GUI"
        );
    }

    /// Issue #818: on Windows the release build is a GUI-subsystem
    /// executable with no console, so `run()` must re-attach to the
    /// invoking console before any CLI arm can print. Pins the ordering
    /// (attach call precedes the first CLI output, `cli_help_text()`) and
    /// the helper's shape (`AttachConsole(ATTACH_PARENT_PROCESS)` +
    /// `CONOUT$` reopen, skipped when the handles are already valid so
    /// pipes keep working). The `Cargo.toml` feature assertion keeps the
    /// `Win32_System_Console` / `Win32_Storage_FileSystem` / `Win32_Security`
    /// features from being pruned as unused.
    #[test]
    fn test_windows_cli_attaches_parent_console_before_output() {
        let source = include_str!("app.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("app.rs has no #[cfg(test)] mod tests block");
        // The `#[cfg]` sits above a doc comment, not directly on the fn, so
        // anchor on the fn name and assert the gate separately.
        assert!(
            prod_source.contains("fn attach_parent_console_for_cli()"),
            "the console attach helper must exist (issue #818)"
        );
        assert!(
            prod_source
                .contains("#[cfg(target_os = \"windows\")]\nfn attach_parent_console_for_cli()")
                || prod_source.contains("caller print anyway (a redirected pipe"),
            "the console attach helper must stay Windows-gated (issue #818)"
        );
        let helper_body = body_of(prod_source, "fn attach_parent_console_for_cli()");
        for marker in [
            "AttachConsole(ATTACH_PARENT_PROCESS)",
            "CONOUT$",
            "GetStdHandle(STD_OUTPUT_HANDLE)",
            "GetStdHandle(STD_ERROR_HANDLE)",
            "SetStdHandle(handle_id, handle)",
        ] {
            assert!(
                helper_body.contains(marker),
                "the console attach helper must contain `{marker}` (issue #818)"
            );
        }
        let run_body = body_of(prod_source, "pub fn run()");
        let attach = run_body
            .find("attach_parent_console_for_cli()")
            .expect("run() must call the console attach helper (issue #818)");
        // The peek `if cli_command(…)` above the match is the attach gate
        // itself, so the ordering that matters is attach-before-first-output,
        // not attach-before-first-parse.
        let first_output = run_body
            .find("cli_help_text()")
            .expect("run() must print help text (issue #818)");
        assert!(
            attach < first_output,
            "the console attach must precede the first CLI output: a flag that \
                 prints before the attach prints to nowhere on a Windows release \
                 build (issue #818)"
        );
        let manifest = include_str!("../Cargo.toml");
        for feature in [
            "Win32_System_Console",
            "Win32_Storage_FileSystem",
            "Win32_Security",
        ] {
            assert!(
                manifest.contains(feature),
                "Cargo.toml must enable the `windows` `{feature}` feature (issue #818)"
            );
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_cli_mode_suppresses_every_config_window() {
        let mut context: tauri::Context<tauri::Wry> = tauri::generate_context!();
        let declared = context.config().app.windows.len();
        assert!(
            declared > 0,
            "the app must declare at least one window for this guard to mean anything"
        );
        assert!(
            context.config().app.windows.iter().any(|w| w.create),
            "at least one declared window must be created by default — otherwise \
                 the suppression below is vacuous"
        );

        let suppressed = suppress_config_windows(&mut context);
        assert_eq!(
            suppressed, declared,
            "every declared window must be accounted for"
        );
        assert!(
            context.config().app.windows.iter().all(|w| !w.create),
            "no config-declared window may be created in CLI mode (issue #679)"
        );
    }

    /// Brace-counted body isolation for a top-level `fn` in this file (house
    /// style — order-independent, never anchored on the following fn, which
    /// drifts).
    fn body_of<'a>(prod_source: &'a str, sig: &str) -> &'a str {
        let after_sig = prod_source
            .split(sig)
            .nth(1)
            .unwrap_or_else(|| panic!("app.rs has no `{}`", sig));
        let open = after_sig
            .find('{')
            .unwrap_or_else(|| panic!("{} has no opening brace", sig));
        let mut depth = 0usize;
        let mut end = None;
        for (i, ch) in after_sig[open..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(open + i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        &after_sig[..end.unwrap_or_else(|| panic!("{} body never closed", sig))]
    }

    /// Issue #922: the detached panes are built from a closed table, so the
    /// label, the in-app URL and the size cannot be steered from the webview —
    /// which is what lets the main window drop the unscoped
    /// `core:webview:allow-create-webview-window` grant.
    #[test]
    fn test_detached_pane_spec_is_a_closed_table() {
        let logs = detached_pane_spec("logs", None).expect("logs is a configured pane");
        assert_eq!(logs.label, "logs-detached");
        assert_eq!(logs.url, "/detached/logs");
        assert!(logs.title.contains("Logs"), "the title must name the pane");
        assert_eq!((logs.width, logs.height), (720.0, 520.0));

        let settings = detached_pane_spec("settings", None).expect("settings is a configured pane");
        assert_eq!(settings.label, "settings-detached");
        assert_eq!(settings.url, "/detached/settings");
        assert_eq!((settings.width, settings.height), (620.0, 720.0));

        // Issue #433: the theme rides on the URL, and only the two values the
        // frontend can read from localStorage are accepted — an unexpected
        // value must not reach the URL.
        for theme in ["dark", "light"] {
            assert_eq!(
                detached_pane_spec("logs", Some(theme))
                    .expect("a stored theme is valid")
                    .url,
                format!("/detached/logs?theme={theme}")
            );
        }
        assert_eq!(
            detached_pane_spec("logs", Some("dark&x=https://evil.example"))
                .expect("an unaccepted theme is ignored, not interpolated")
                .url,
            "/detached/logs"
        );

        // Nothing outside the two configured panes may open a window, and the
        // labels the store already knows are not pane names either.
        for pane in [
            "",
            "main",
            "logs-detached",
            "../logs",
            "LOGS",
            "settings/../logs",
        ] {
            assert!(
                detached_pane_spec(pane, None).is_err(),
                "`{pane}` is not a detached pane and must be refused"
            );
        }

        // The command is the only path, so the store must no longer construct
        // a window itself (issue #922) — that is what the grant was for.
        let store = include_str!("../../src/lib/stores/detach.ts");
        assert!(
            store.contains("invoke('detach_pane'"),
            "the store must open panes through the detach_pane command"
        );
        assert!(
            !store.contains("new WebviewWindow"),
            "the store must not create webview windows from the main window (issue #922)"
        );
    }
}
