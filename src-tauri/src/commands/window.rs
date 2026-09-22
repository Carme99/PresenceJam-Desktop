//! Window/system Tauri commands (show window, autostart, open URL/folder).
//!
//! See issue #76. Also owns the `validate_http_url` helper used by
//! `open_external_url` (issue #67).

use crate::commands::shortcut_reason::ShortcutReason;
use crate::{config, AppState};
use std::path::Path;
use std::sync::Arc;
use tauri::{AppHandle, Manager};
use url::Url;

/// Log tag prefix for this submodule (issue #79 item 3).
const CMD: &str = "[CMD.WINDOW]";

/// Validates that a URL uses http or https scheme, has a host, and
/// contains no userinfo (the `user:pass@` form). Returns the parsed URL
/// on success, or an error string on failure. See issue #67.
fn validate_http_url(url: &str) -> Result<Url, String> {
    Url::parse(url)
        .map_err(|_| "Invalid URL format".to_string())
        .and_then(|parsed| {
            match parsed.scheme() {
                "http" | "https" => {}
                other => {
                    return Err(format!(
                        "Invalid URL scheme '{}': only http/https allowed",
                        other
                    ));
                }
            }
            if parsed.host_str().map(str::is_empty).unwrap_or(true) {
                return Err("URL has no host".to_string());
            }
            if !parsed.username().is_empty() || parsed.password().is_some() {
                return Err("URL has userinfo (user:pass@) — disallowed".to_string());
            }
            Ok(parsed)
        })
}

/// Show the main window. Stays synchronous (#215): this is a fast
/// window-manager call (show + focus) with no disk/network/keychain IO.
/// Offloading to spawn_blocking would add latency and risks calling
/// `window.show()` off the main thread. Documented per #215 slice.
#[tauri::command]
pub fn show_window(app: AppHandle) -> Result<(), String> {
    log::debug!("{CMD} show_window: ENTRY");

    // Issue #826: the raise either happens or it does not. This used to log
    // SUCCESS on every exit path — including the not-found one, where the
    // warn immediately above it described the opposite outcome — so a tray
    // click that raised no window was recorded as a success and the one log
    // line a post-mortem would use to confirm "the raise fired but the
    // window was gone" said the raise worked. Failing here also lets the
    // frontend's `invoke('show_window')` catch and the tray arm see it.
    let Some(window) = app.get_webview_window("main") else {
        log::warn!("{CMD} show_window: main window not found");
        return Err("main window not found".to_string());
    };

    log::info!("{CMD} show_window: window found, showing and focusing");
    // Issue #826: a refused raise or focus on an existing window is not
    // fatal — the deliberate discards tray.rs uses for the same raise —
    // so keep `let _ =` here; do not turn them into errors later.
    let _ = window.show();
    // Issue #391: a minimized window stays minimized after show() —
    // unminimize first (mirrors the single-instance raise in lib.rs).
    let _ = window.unminimize();
    let _ = window.set_focus();

    log::info!("{CMD} show_window: SUCCESS");
    Ok(())
}

/// Issue #811: the OS half of the autostart toggle — flips the login entry
/// without touching `config.autostart`. [`set_autostart_enabled`] (the Tauri
/// command) calls this and then persists the flag; `config::after_persist`
/// calls this to re-derive the entry from the persisted config. Splitting the
/// OS write out keeps `after_persist` from re-entering the command (which
/// would persist again and recurse).
pub(crate) async fn apply_os_autostart(
    app: &AppHandle,
    enabled: bool,
) -> Result<(), ShortcutReason> {
    // #215: AutoLaunchManager touches the OS autostart registry/file
    // (disk + OS service). Offload to blocking pool so the UI thread
    // is not blocked while the manager reads/writes the autostart entry.
    let app_clone = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let autolaunch_manager = app_clone.state::<tauri_plugin_autostart::AutoLaunchManager>();
        let is_enabled = autolaunch_manager.is_enabled().map_err(|e| {
            log::error!(
                "{CMD} set_autostart_enabled: is_enabled check FAILED - {}",
                e
            );
            ShortcutReason::autostart(&e.to_string())
        })?;

        if is_enabled == enabled {
            log::info!(
                "{CMD} set_autostart_enabled: already in desired state (enabled={}), no-op",
                enabled
            );
            return Ok(());
        }

        if enabled {
            autolaunch_manager.enable().map_err(|e| {
                log::error!("{CMD} set_autostart_enabled: enable FAILED - {}", e);
                ShortcutReason::autostart(&e.to_string())
            })?;
            log::info!("{CMD} set_autostart_enabled: enable SUCCESS");
        } else {
            autolaunch_manager.disable().map_err(|e| {
                log::error!("{CMD} set_autostart_enabled: disable FAILED - {}", e);
                ShortcutReason::autostart(&e.to_string())
            })?;
            log::info!("{CMD} set_autostart_enabled: disable SUCCESS");
        }
        Ok(())
    })
    .await
    .map_err(|e| {
        ShortcutReason::autostart(&format!(
            "set_autostart_enabled spawn_blocking panicked: {:?}",
            e
        ))
    })?
}

#[tauri::command]
pub async fn set_autostart_enabled(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    enabled: bool,
) -> Result<(), ShortcutReason> {
    log::debug!("{CMD} set_autostart_enabled: ENTRY - enabled={}", enabled);
    apply_os_autostart(&app, enabled).await?;

    // Issue #811: the command owns both halves — the OS login entry above
    // and the `config.autostart` flag that `config::after_persist` re-derives
    // the entry from after every write. Without this persist a later write
    // from another surface (`set_locale`, `update_config`, an import) carries
    // the still-persisted `autostart: false` and silently disables the entry
    // the toggle just enabled. The write runs on the blocking pool under the
    // same single write guard `save_config`/`update_config` use, so a
    // concurrent write cannot interleave; `after_persist`'s re-sync then
    // re-applies this same value and is idempotent. A no-op OS toggle whose
    // flag already matches still skips the write below, but converges a
    // drifted flag without touching the OS.
    let state_clone = Arc::clone(state.inner());
    let persisted = tauri::async_runtime::spawn_blocking(move || {
        let mut config_guard = state_clone.config.get_mut();
        let mut merged = match config_guard.as_ref() {
            Some(current) => current.clone(),
            None => config::load_config().map_err(|e| {
                log::error!("{CMD} set_autostart_enabled: config load FAILED - {}", e);
                ShortcutReason::autostart(&e)
            })?,
        };
        if merged.autostart == enabled {
            return Ok::<Option<crate::config::AppConfig>, ShortcutReason>(None);
        }
        merged.autostart = enabled;
        let mut persisted = config::clamped_config(&merged);
        config::stamp_schema_version(&mut persisted);
        match config::save_config(&persisted) {
            Ok(()) => {
                *config_guard = Some(persisted.clone());
                log::info!(
                    "{CMD} set_autostart_enabled: config persisted (autostart={})",
                    enabled
                );
                Ok::<Option<crate::config::AppConfig>, ShortcutReason>(Some(persisted))
            }
            Err(e) => {
                log::error!("{CMD} set_autostart_enabled: config persist FAILED - {}", e);
                Err(ShortcutReason::autostart(&e))
            }
        }
    })
    .await
    .map_err(|e| {
        ShortcutReason::autostart(&format!(
            "set_autostart_enabled spawn_blocking panicked: {:?}",
            e
        ))
    })??;
    if let Some(persisted) = persisted {
        super::config::after_persist(&app, &persisted).await;
    }
    log::info!("{CMD} set_autostart_enabled: SUCCESS - enabled={}", enabled);
    Ok(())
}

#[tauri::command]
pub async fn open_logs_folder(app: AppHandle) -> Result<(), String> {
    log::debug!("{CMD} open_logs_folder: ENTRY");

    // #215: app_log_dir() touches the filesystem (app data dir resolution)
    // and opener::open_path spawns a shell process. Offload both to the
    // blocking pool.
    let app_clone = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let logs_path = app_clone.path().app_log_dir().map_err(|e| {
            log::error!("{CMD} open_logs_folder: failed to get log dir - {}", e);
            e.to_string()
        })?;
        log::info!("{CMD} open_logs_folder: log path={}", logs_path.display());

        match open_logs_dir(&logs_path) {
            Ok(()) => {
                log::info!("{CMD} open_logs_folder: SUCCESS");
                Ok(())
            }
            Err(e) => {
                log::error!("{CMD} open_logs_folder: FAILED - {}", e);
                Err(e)
            }
        }
    })
    .await
    .map_err(|e| format!("open_logs_folder spawn_blocking panicked: {:?}", e))?
}

/// Hands the path to the desktop file manager and returns the opener's
/// outcome.
///
/// This is intentionally a thin wrapper: `tauri_plugin_opener::open_path`
/// performs a `metadata()` check first, so a non-existent target surfaces
/// as an `io::Error` here (instead of being silently "spawned" the way
/// `open_url` would treat a path-as-URL string). The plugin then calls
/// `open::that_detached`, which is best-effort — once the spawn has been
/// dispatched we cannot guarantee the desktop actually opened it, so the
/// returned `Ok(())` means "spawn dispatched" rather than "user is
/// looking at the folder".
///
/// Issue #979: replaced `open_url` with `open_path` so a missing log
/// directory is an error the caller can show, not a silent success.
pub(crate) fn open_logs_dir(path: &Path) -> Result<(), String> {
    tauri_plugin_opener::open_path(path, None::<&str>).map_err(|e| e.to_string())
}

// `open_external` and `get_current_track` were removed in v2.6.4 (issue #77).
// `open_external_url` is the only URL-opener the Svelte code calls; the
// current track is read from `spotify-track-changed` events and from
// `get_sync_status`. Both dead commands were byte-for-byte duplicates of
// live paths.

#[tauri::command]
pub async fn open_external_url(url: String) -> Result<(), String> {
    log::debug!("{CMD} open_external_url: ENTRY - url.len={}", url.len());

    // Validate URL scheme - only allow http/https. See issue #14.
    // Pure validation, no IO — keep on async thread before blocking.
    validate_http_url(&url)?;

    // #215: opener::open_url spawns a shell process (blocking). Offload.
    let url_clone = url.clone();
    tauri::async_runtime::spawn_blocking(move || {
        match tauri_plugin_opener::open_url(&url_clone, None::<&str>) {
            Ok(()) => {
                log::info!("{CMD} open_external_url: SUCCESS");
                Ok(())
            }
            Err(e) => {
                log::error!("{CMD} open_external_url: FAILED - {}", e);
                Err(format!("Failed to open URL: {}", e))
            }
        }
    })
    .await
    .map_err(|e| format!("open_external_url spawn_blocking panicked: {:?}", e))?
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::path::PathBuf;

    /// Issue #979: a non-existent logs directory must surface as an error.
    /// `open_url` (the previous implementation) silently accepted any
    /// path-as-URL string and reported SUCCESS once `that_detached` was
    /// dispatched — including for paths the desktop could never open.
    /// `open_path` runs `path.metadata()` first, so a missing target
    /// returns an IO error that the caller can render to the user.
    #[test]
    fn open_logs_dir_returns_err_for_missing_directory() {
        let bogus = PathBuf::from("/nonexistent/path/that/should/not/exist/anywhere");
        // Sanity: the path really is absent — otherwise this test would
        // silently pass on a machine that happened to have it.
        assert!(
            !bogus.exists(),
            "precondition: the test path must not exist on this machine"
        );

        let result = super::open_logs_dir(&bogus);
        assert!(
            result.is_err(),
            "open_logs_dir must surface a missing directory as Err, \
             not dispatch a no-op spawn (issue #979)"
        );
        // The error string comes from `path.metadata()` -> io::Error, which
        // already names the kind — keep it informative rather than mapping
        // it down to a generic "failed" message.
        let err = result.unwrap_err();
        assert!(
            !err.is_empty(),
            "error message must not be empty — the frontend surfaces it verbatim"
        );
    }

    /// Issue #979: a path that exists must dispatch the opener and return
    /// `Ok`. `temp_dir()` always exists on every supported platform, so
    /// this case is safe to assert unconditionally. The actual GUI window
    /// opening is best-effort (the spawn is detached); the assertion is on
    /// the spawn dispatch, not on a user-visible file-manager window.
    #[test]
    fn open_logs_dir_returns_ok_for_existing_directory() {
        let existing = env::temp_dir();
        assert!(
            existing.exists(),
            "precondition: env::temp_dir() must exist (issue #979)"
        );

        let result = super::open_logs_dir(&existing);
        assert!(
            result.is_ok(),
            "open_logs_dir must dispatch the opener for an existing path, \
             got: {:?}",
            result.err()
        );
    }

    /// Issue #391: show_window must unminimize (a minimized window stays
    /// minimized after show()). Brace-counted body isolation
    /// (order-independent): do not anchor on the next fn.
    #[test]
    fn show_window_unminimizes() {
        let src = include_str!("window.rs");
        let sig_idx = src
            .find("pub fn show_window(")
            .expect("show_window must exist");
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
                panic!("unbalanced braces in show_window");
            }
        };
        let body = &src[body_start + 1..body_end];
        assert!(
            body.contains("window.unminimize()"),
            "show_window must unminimize (mirror lib.rs single-instance raise)"
        );
        assert!(
            body.contains("window.show()"),
            "show_window must still show the window"
        );
        assert!(
            body.contains("window.set_focus()"),
            "show_window must still focus the window"
        );
    }

    /// Issue #826: the not-found arm must fail instead of falling through to
    /// a SUCCESS line that describes the opposite outcome.
    #[test]
    fn show_window_fails_and_logs_no_success_when_the_main_window_is_missing() {
        let src = include_str!("window.rs");
        let sig_idx = src
            .find("pub fn show_window(")
            .expect("show_window must exist");
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
                panic!("unbalanced braces in show_window");
            }
        };
        let body = &src[body_start + 1..body_end];

        let not_found = body
            .find("main window not found")
            .expect("show_window must name the missing-window condition (issue #826)");
        // Anchored to the log literal: the bare word SUCCESS also appears in
        // the production comment above the guard, which would match first.
        let success = body
            .find("show_window: SUCCESS")
            .expect("show_window must still report success on the raise path (issue #826)");

        assert!(
            body[not_found..].contains("return Err("),
            "the not-found arm must return Err, not fall through to Ok (issue #826)"
        );
        assert!(
            not_found < success,
            "SUCCESS must sit on the raise path after the missing-window arm, \
             never on the exit path (issue #826)"
        );
    }
    /// Issue #811: `set_autostart_enabled` must own both halves of the toggle —
    /// the OS login entry AND the `config.autostart` flag `after_persist`
    /// re-derives the entry from. Extract the command body by brace-counting
    /// (order-independent: never anchor on the next fn) and require the
    /// guarded persist plus the idempotent `after_persist` convergence.
    #[test]
    fn set_autostart_enabled_persists_the_flag_it_toggles() {
        let src = include_str!("window.rs");
        // Strip this test module so the assertions below cannot match their
        // own literals.
        let prod = src
            .split("#[cfg(test)]")
            .next()
            .expect("window.rs must have a test module");
        let sig_idx = prod
            .find("pub async fn set_autostart_enabled(")
            .expect("set_autostart_enabled must exist");
        let brace_open_rel = prod[sig_idx..]
            .find('{')
            .expect("command body must have an opening brace");
        let body_start = sig_idx + brace_open_rel;
        let mut depth: u32 = 0;
        let mut i = body_start;
        let body_end = loop {
            match prod.as_bytes()[i] {
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
            if i >= prod.len() {
                panic!("unbalanced braces in set_autostart_enabled");
            }
        };
        let body = &prod[body_start + 1..body_end];
        assert!(
            body.contains("merged.autostart = enabled"),
            "the toggle must write config.autostart (issue #811)"
        );
        assert!(
            body.contains("config::save_config("),
            "the toggle must persist through the guarded write path (issue #811)"
        );
        assert!(
            body.contains("after_persist("),
            "the toggle must converge through after_persist so the re-sync is idempotent (issue #811)"
        );
    }
}
