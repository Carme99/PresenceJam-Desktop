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

    if let Some(window) = app.get_webview_window("main") {
        log::info!("{CMD} show_window: window found, showing and focusing");
        let _ = window.show();
        let _ = window.set_focus();
    } else {
        log::warn!("{CMD} show_window: main window not found");
    }

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
