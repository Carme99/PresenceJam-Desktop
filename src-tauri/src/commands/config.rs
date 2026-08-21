//! Configuration load/save Tauri commands.
//!
//! See issue #76.

use crate::config::{self, AppConfig};
use crate::AppState;
use std::sync::Arc;
use tauri::AppHandle;

/// Log tag prefix for this submodule (issue #79 item 3).
const CMD: &str = "[CMD.CONFIG]";

#[tauri::command]
pub fn load_config() -> Result<AppConfig, String> {
    log::debug!("{CMD} load_config: ENTRY");
    match config::load_config() {
        Ok(cfg) => {
            log::info!(
                "{CMD} load_config: SUCCESS - spotify.client_id.len={}",
                cfg.spotify.client_id.len()
            );
            Ok(cfg)
        }
        Err(e) => {
            log::error!("{CMD} load_config: FAILED - {}", e);
            Err(e)
        }
    }
}

#[tauri::command]
pub async fn save_config(
    app: AppHandle,
    config: AppConfig,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    log::info!(
        "{CMD} save_config: ENTRY - config.spotify.client_id.len={}",
        config.spotify.client_id.len()
    );

    // #215: serialization + atomic_write_json (fsync) holds the write lock
    // across IO. Offload the entire read-modify-write critical section to
    // the blocking pool so the async runtime is not blocked and the lock
    // is not held across an await.
    let state_clone = Arc::clone(state.inner());
    let config_clone = config.clone();
    tauri::async_runtime::spawn_blocking(move || {
        // Hold the write lock for the entire read-modify-write to prevent races
        // with concurrent reads from the polling loop. See bug #26.
        let mut config_guard = state_clone.config.get_mut();
        match config::save_config(&config_clone) {
            Ok(()) => {
                log::info!("{CMD} save_config: file saved successfully");
                *config_guard = Some(config_clone);
                Ok::<(), String>(())
            }
            Err(e) => {
                log::error!("{CMD} save_config: FAILED - {}", e);
                Err(e)
            }
        }
    })
    .await
    .map_err(|e| format!("save_config spawn_blocking panicked: {:?}", e))??;

    // On macOS, sync the app's activation policy with the saved
    // `start_minimized` preference so the dock icon disappears when the
    // user wants tray-only behavior and reappears when they disable it.
    // Setting on every save (not just on toggle) keeps the policy
    // idempotent and avoids tracking previous state. See audit Q4.
    //
    // Run BEFORE `set_autostart_enabled` because that helper takes
    // `app` by value, and `set_activation_policy` borrows it. This
    // ordering matches the new doc note on the lib.rs side.
    // This part is fast (no disk IO beyond the already-completed save)
    // and must run on the main thread, so it stays outside spawn_blocking.
    #[cfg(target_os = "macos")]
    {
        let policy = if config.teams.start_minimized {
            tauri::ActivationPolicy::Accessory
        } else {
            tauri::ActivationPolicy::Regular
        };
        // tauri::AppHandle::set_activation_policy returns () on success;
        // the underlying call logs its own errors via the tauri-runtime-wry
        // layer. We deliberately discard the unit value rather than wrapping
        // in `if let Err(...)`.
        let _ = app.set_activation_policy(policy);
    }

    // Sync autostart state with the OS autostart manager. The command is
    // now async (it touches the autostart registry/file), so we await it.
    #[cfg(desktop)]
    {
        if let Err(e) = super::window::set_autostart_enabled(app.clone(), config.autostart).await {
            log::warn!("{CMD} save_config: failed to sync autostart state: {}", e);
        }
    }
    log::info!("{CMD} save_config: SUCCESS");
    Ok(())
}
