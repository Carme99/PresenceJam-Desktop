//! Miscellaneous Tauri commands that don't fit the auth/sync/window/onboarding
//! split.
//!
//! See issue #76. Currently holds:
//! - `preview_status` — Settings-page preview renderer (issue #74)
//! - `update_tray_menu_state` — tray menu state update from the frontend

use crate::spotify::TrackInfo;
use crate::tray;
use std::sync::Arc;
use tauri::{AppHandle, Manager};

/// Log tag prefix for this submodule (issue #79 item 3).
const CMD: &str = "[CMD.MISC]";

/// Renders a status-format template against a sample track so the Svelte
/// Settings page can show a live preview without needing a real playing
/// track. Keeps the Rust `format_status` as the single source of truth for
/// the `{artist}` / `{track}` / `{album}` / `{emoji}` substitution rules.
/// See issue #74.
///
/// Issue #342: when the filter is enabled the formatted sample is routed
/// through `filter_status` exactly like the runtime polling loop does, so
/// the preview demonstrates the effective fallback (a whitespace-only
/// placeholder renders the canonical default, not the raw format). The
/// optional profane sample lets the user see that fallback path with a
/// clean-looking template.
///
/// #215 decision: stays synchronous — pure string substitution, no disk,
/// network, or keychain IO. Offloading to spawn_blocking would add
/// overhead with no benefit.
#[tauri::command]
pub fn preview_status(
    format: String,
    filter_enabled: Option<bool>,
    placeholder: Option<String>,
    profane_sample: Option<bool>,
) -> String {
    log::debug!("{CMD} preview_status: ENTRY - format.len={}", format.len());
    let result = if filter_enabled.unwrap_or(false) {
        let sample = TrackInfo {
            title: if profane_sample.unwrap_or(false) {
                "Shit".to_string()
            } else {
                "Sample Track".to_string()
            },
            artist: "Sample Artist".to_string(),
            album: "Sample Album".to_string(),
            album_art_url: String::new(),
            is_playing: true,
            progress_ms: Some(0),
            duration_ms: 0,
        };
        let formatted = crate::spotify::format_status(&sample, &format);
        let effective_placeholder = placeholder
            .as_deref()
            .unwrap_or(crate::profanity::safe_placeholder_default());
        crate::profanity::filter_status(&formatted, effective_placeholder, sample.is_playing)
    } else {
        crate::spotify::preview_status_with_sample(&format)
    };
    log::debug!("{CMD} preview_status: SUCCESS");
    result
}

/// Rebuilds the tray menu from authoritative backend state (issue #592).
///
/// The frontend used to pass its own `is_syncing` / `current_track` mirrors
/// here. Those mirrors are event-driven (and absent entirely on a view that
/// never mounted), so committing them made the tray's dedup baseline a tuple
/// the backend never produced: a stale mirror could erase the track row or
/// show "Pause Sync" while `polling.is_syncing` was false, and the truthful
/// rebuild stayed suppressed until the dedup key changed. The command is now
/// a push-only refresh trigger — it takes no state, and any payload the
/// caller still sends is ignored.
#[tauri::command]
pub async fn update_tray_menu_state(app: AppHandle) -> Result<(), String> {
    log::info!("{CMD} update_tray_menu_state: ENTRY");
    // #215: tray::update_tray_menu builds the native menu and may fetch
    // Spotify devices/queue via blocking HTTP (cached_devices / cached_queue
    // in tray.rs call spotify::get_devices with 10 s timeout). Offload to
    // the blocking pool so the UI thread is not frozen.
    let app_clone = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(state) = app_clone.try_state::<Arc<crate::AppState>>() else {
            return Err("AppState not registered".to_string());
        };
        let is_syncing = state
            .polling
            .is_syncing(std::sync::atomic::Ordering::Acquire);
        let current_track = state.polling.current_track().clone();
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
pub async fn relaunch_app(window: tauri::Window, app: AppHandle) -> Result<(), String> {
    // Issue #241: process restart is main-window-only (UpdatePrompt is gated
    // to the main window in +layout.svelte); a detached window must never
    // restart the app out from under the user.
    super::require_main_window(&window)?;
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
