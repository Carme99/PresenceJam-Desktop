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
#[tauri::command]
pub fn preview_status(format: String) -> String {
    log::debug!("{CMD} preview_status: ENTRY - format.len={}", format.len());
    let result = crate::spotify::preview_status_with_sample(&format);
    log::debug!("{CMD} preview_status: SUCCESS");
    result
}

#[tauri::command]
pub fn update_tray_menu_state(
    app: AppHandle,
    is_syncing: bool,
    current_track: Option<TrackInfo>,
) -> Result<(), String> {
    log::info!(
        "{CMD} update_tray_menu_state: ENTRY - is_syncing={}",
        is_syncing
    );
    tray::update_tray_menu(&app, is_syncing, current_track)?;
    log::info!("{CMD} update_tray_menu_state: SUCCESS");
    Ok(())
}