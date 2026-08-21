//! Miscellaneous Tauri commands that don't fit the auth/sync/window/onboarding
//! split.
//!
//! See issue #76. Currently holds:
//! - `preview_status` — Settings-page preview renderer (issue #74)
//! - `update_tray_menu_state` — tray menu state update from the frontend

use crate::spotify::TrackInfo;
use crate::tray;
use tauri::AppHandle;

/// Log tag prefix for this submodule (issue #79 item 3).
const CMD: &str = "[CMD.MISC]";

/// Renders a status-format template against a sample track so the Svelte
/// Settings page can show a live preview without needing a real playing
/// track. Keeps the Rust `format_status` as the single source of truth for
/// the `{artist}` / `{track}` / `{album}` / `{emoji}` substitution rules.
/// See issue #74.
///
/// #215 decision: stays synchronous. This is pure string substitution via
/// `spotify::preview_status_with_sample` — no disk, network, or keychain
/// IO (see `spotify.rs:preview_status_with_sample` which builds a sample
/// TrackInfo and calls `format_status`). Offloading to spawn_blocking
/// would add overhead with no benefit.
#[tauri::command]
pub fn preview_status(format: String) -> String {
    log::debug!("{CMD} preview_status: ENTRY - format.len={}", format.len());
    let result = crate::spotify::preview_status_with_sample(&format);
    log::debug!("{CMD} preview_status: SUCCESS");
    result
}

#[tauri::command]
pub async fn update_tray_menu_state(
    app: AppHandle,
    is_syncing: bool,
    current_track: Option<TrackInfo>,
) -> Result<(), String> {
    log::info!(
        "{CMD} update_tray_menu_state: ENTRY - is_syncing={}",
        is_syncing
    );
    // #215: tray::update_tray_menu builds the native menu and may fetch
    // Spotify devices/queue via blocking HTTP (cached_devices / cached_queue
    // in tray.rs call spotify::get_devices with 10 s timeout). Offload to
    // the blocking pool so the UI thread is not frozen.
    let app_clone = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        tray::update_tray_menu(&app_clone, is_syncing, current_track)
    })
    .await
    .map_err(|e| format!("update_tray_menu_state spawn_blocking panicked: {:?}", e))?
    .map_err(|e| {
        log::error!("{CMD} update_tray_menu_state: FAILED - {}", e);
        e
    })?;
    log::info!("{CMD} update_tray_menu_state: SUCCESS");
    Ok(())
}

/// Restarts the app process. Invoked by the frontend after an update has
/// been downloaded and installed so the new version takes effect.
/// `AppHandle::restart` never returns (it exits the process), so the
/// `!` tail expression coerces into the `Result<(), String>` signature.
///
/// #215: offloaded to spawn_blocking as it touches process state. The
/// blocking thread will exit the process; the async wrapper simply awaits
/// the blocking task (which never returns on success).
#[tauri::command]
pub async fn relaunch_app(app: AppHandle) -> Result<(), String> {
    log::info!("{CMD} relaunch_app: ENTRY");
    tauri::async_runtime::spawn_blocking(move || {
        app.restart();
        // app.restart() never returns; this is unreachable, but keep a
        // fallback error shape for the type checker.
        #[allow(unreachable_code)]
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("relaunch_app spawn_blocking panicked: {:?}", e))?
}
