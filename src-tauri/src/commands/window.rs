//! Window/system Tauri commands (show window, autostart, open URL/folder).
//!
//! See issue #76. Also owns the `validate_http_url` helper used by
//! `open_external_url` (issue #67).

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

#[tauri::command]
pub async fn set_autostart_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    log::debug!("{CMD} set_autostart_enabled: ENTRY - enabled={}", enabled);

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
            e.to_string()
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
                e.to_string()
            })?;
            log::info!("{CMD} set_autostart_enabled: enable SUCCESS");
        } else {
            autolaunch_manager.disable().map_err(|e| {
                log::error!("{CMD} set_autostart_enabled: disable FAILED - {}", e);
                e.to_string()
            })?;
            log::info!("{CMD} set_autostart_enabled: disable SUCCESS");
        }
        Ok(())
    })
    .await
    .map_err(|e| format!("set_autostart_enabled spawn_blocking panicked: {:?}", e))?
}

#[tauri::command]
pub async fn open_logs_folder(app: AppHandle) -> Result<(), String> {
    log::debug!("{CMD} open_logs_folder: ENTRY");

    // #215: app_log_dir() touches the filesystem (app data dir resolution)
    // and opener::open_url spawns a shell process. Offload both to the
    // blocking pool.
    let app_clone = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let logs_path = app_clone.path().app_log_dir().map_err(|e| {
            log::error!("{CMD} open_logs_folder: failed to get log dir - {}", e);
            e.to_string()
        })?;
        let path_str = logs_path.to_string_lossy();
        log::info!("{CMD} open_logs_folder: log path={}", path_str);

        match tauri_plugin_opener::open_url(&path_str, None::<&str>) {
            Ok(()) => {
                log::info!("{CMD} open_logs_folder: SUCCESS");
                Ok(())
            }
            Err(e) => {
                log::error!("{CMD} open_logs_folder: FAILED - {}", e);
                Err(e.to_string())
            }
        }
    })
    .await
    .map_err(|e| format!("open_logs_folder spawn_blocking panicked: {:?}", e))?
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
    use super::validate_http_url;

    /// Issue #67/#761: `open_external_url` is the app's only shell-open path
    /// and the frontend hands it an arbitrary string, so the scheme/host/
    /// userinfo gate is asserted by running it — the two schemes it accepts,
    /// the three rejections it documents, and the parse failure. Before this,
    /// window.rs's whole suite was a source scan.
    #[test]
    fn validate_http_url_accepts_only_plain_http_urls_with_a_host() {
        assert!(validate_http_url("https://learn.microsoft.com/device").is_ok());
        assert!(validate_http_url("http://127.0.0.1:8899/callback").is_ok());

        // A non-http scheme would hand the OS opener a script or a local file.
        for url in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "data:text/html,<script>x</script>",
        ] {
            assert!(validate_http_url(url).is_err(), "{url} must be denied");
        }

        // No host: there is nothing to open.
        assert!(validate_http_url("https://").is_err());
        // Userinfo: a credential-bearing URL must never reach the OS opener.
        assert!(validate_http_url("https://user:pass@example.com").is_err());
        assert!(validate_http_url("not a url").is_err());
    }

    /// Issue #391: every show path must unminimize a minimized window (it stays
    /// minimized after `show()`), then show and focus it. Source-level because
    /// the invariant is the sequence of calls on a live `tauri::Window`, which
    /// no unit test can build — the window handle is the entire fixture — so
    /// the scan pins that all three calls are still made. Brace-counted body
    /// isolation (order-independent): do not anchor on the next fn.
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
}
