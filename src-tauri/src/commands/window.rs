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
pub fn set_autostart_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    log::debug!("{CMD} set_autostart_enabled: ENTRY - enabled={}", enabled);

    let autolaunch_manager = app.state::<tauri_plugin_autostart::AutoLaunchManager>();
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
}

#[tauri::command]
pub fn open_logs_folder(app: AppHandle) -> Result<(), String> {
    log::debug!("{CMD} open_logs_folder: ENTRY");

    let logs_path = app.path().app_log_dir().map_err(|e| {
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
}

// `open_external` and `get_current_track` were removed in v2.6.4 (issue #77).
// `open_external_url` is the only URL-opener the Svelte code calls; the
// current track is read from `spotify-track-changed` events and from
// `get_sync_status`. Both dead commands were byte-for-byte duplicates of
// live paths.

#[tauri::command]
pub fn open_external_url(url: String) -> Result<(), String> {
    log::debug!("{CMD} open_external_url: ENTRY - url.len={}", url.len());

    // Validate URL scheme - only allow http/https. See issue #14.
    validate_http_url(&url)?;

    match tauri_plugin_opener::open_url(&url, None::<&str>) {
        Ok(()) => {
            log::info!("{CMD} open_external_url: SUCCESS");
            Ok(())
        }
        Err(e) => {
            log::error!("{CMD} open_external_url: FAILED - {}", e);
            Err(format!("Failed to open URL: {}", e))
        }
    }
}