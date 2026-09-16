//! Configuration load/save Tauri commands.
//!
//! See issue #76.

use crate::config::{self, AppConfig, ConfigPatch};
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
/// Returns the config as PERSISTED (clamped), so the caller can adopt the
/// same value. Issue #297: the frontend previously stored its own unclamped
/// input, so the UI showed a value that was never written to disk.
///
/// Whole-document replace: the caller must already hold a complete, current
/// `AppConfig` (Settings clones the loaded store, so it does). A caller that
/// only knows part of the document must use [`update_config`] instead —
/// this command will happily wipe every field it did not receive (the
/// backend half of the #531 family, CfgDiag#0 / issue #535).
pub async fn save_config(
    app: AppHandle,
    config: AppConfig,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<AppConfig, String> {
    log::info!(
        "{CMD} save_config: ENTRY - config.spotify.client_id.len={}",
        config.spotify.client_id.len()
    );

    // #215: serialization + atomic_write_json (fsync) holds the write lock
    // across IO. Offload the entire read-modify-write critical section to
    // the blocking pool so the async runtime is not blocked and the lock
    // is not held across an await.
    let state_clone = Arc::clone(state.inner());
    // Issue #297: `save_config` persists a CLAMPED copy, so store that same
    // value in AppState. Storing the raw input left the in-memory config (the
    // one the polling loop reads) disagreeing with config.json until restart.
    let config_clone = config::clamped_config(&config);
    let persisted = tauri::async_runtime::spawn_blocking(move || {
        // Hold the write lock for the entire read-modify-write to prevent races
        // with concurrent reads from the polling loop. See bug #26.
        let mut config_guard = state_clone.config.get_mut();
        match config::save_config(&config_clone) {
            Ok(()) => {
                log::info!("{CMD} save_config: file saved successfully");
                // Issue #536: `save_config` stamps the binary-owned schema
                // version, so the in-memory copy must carry the same value
                // that reached disk (the #297 invariant).
                let mut persisted = config_clone.clone();
                config::stamp_schema_version(&mut persisted);
                *config_guard = Some(persisted.clone());
                Ok::<AppConfig, String>(persisted)
            }
            Err(e) => {
                log::error!("{CMD} save_config: FAILED - {}", e);
                Err(e)
            }
        }
    })
    .await
    .map_err(|e| format!("save_config spawn_blocking panicked: {:?}", e))??;

    after_persist(&app, &persisted).await;
    log::info!("{CMD} save_config: SUCCESS");
    Ok(persisted)
}

#[tauri::command]
/// Apply a field-level patch to the stored config and return what was
/// actually persisted (clamped, with the binary-owned schema version).
///
/// CfgDiag#0 (issue #535): `save_config` replaces the whole document, so a
/// caller that only knows some of it silently resets the rest. This command
/// merges instead — a caller can name `teams.status_format` and be certain
/// the user's quiet hours, track rules, logging level and every other field
/// are still there afterwards.
///
/// The base is the in-memory config (what the polling loop reads and what
/// the last write stored); before the first load has run, it is read from
/// disk. Either way the read-modify-write happens inside the same single
/// write guard `save_config` uses, so a concurrent write cannot interleave.
pub async fn update_config(
    app: AppHandle,
    patch: ConfigPatch,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<AppConfig, String> {
    log::info!("{CMD} update_config: ENTRY");

    // #215 pattern: the whole read-merge-write critical section runs on the
    // blocking pool, holding the config write lock across the fsync.
    let state_clone = Arc::clone(state.inner());
    let persisted = tauri::async_runtime::spawn_blocking(move || {
        let mut config_guard = state_clone.config.get_mut();
        let base = match config_guard.as_ref() {
            Some(current) => current.clone(),
            None => config::load_config()?,
        };

        let mut merged = base;
        config::apply_patch(&mut merged, &patch);

        let mut persisted = config::clamped_config(&merged);
        config::stamp_schema_version(&mut persisted);
        match config::save_config(&persisted) {
            Ok(()) => {
                *config_guard = Some(persisted.clone());
                Ok::<AppConfig, String>(persisted)
            }
            Err(e) => {
                log::error!("{CMD} update_config: FAILED - {}", e);
                Err(e)
            }
        }
    })
    .await
    .map_err(|e| format!("update_config spawn_blocking panicked: {:?}", e))??;

    after_persist(&app, &persisted).await;
    log::info!("{CMD} update_config: SUCCESS");
    Ok(persisted)
}

/// Side effects that must follow a successful config write, shared by every
/// write command so they cannot drift apart.
///
/// Ordering is load-bearing: the logger is re-armed first (CfgDiag#4, issue
/// #539 — a `logging.enabled` / `log_level` change takes effect immediately
/// instead of at the next launch, and the level also governs whether the
/// OS-side effects below are logged), then the macOS activation policy
/// (which borrows `app`, hence before the by-value `set_autostart_enabled`),
/// then the OS autostart entry.
#[cfg_attr(not(desktop), allow(unused_variables))]
async fn after_persist(app: &AppHandle, persisted: &AppConfig) {
    config::apply_log_level(&persisted.logging);

    // On macOS, sync the app's activation policy with the saved
    // `start_minimized` preference so the dock icon disappears when the
    // user wants tray-only behavior and reappears when they disable it.
    // Setting on every save (not just on toggle) keeps the policy
    // idempotent and avoids tracking previous state. See audit Q4.
    #[cfg(target_os = "macos")]
    {
        let policy = if persisted.teams.start_minimized {
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
        if let Err(e) = super::window::set_autostart_enabled(app.clone(), persisted.autostart).await
        {
            log::warn!("{CMD} failed to sync autostart state: {}", e);
        }
    }
}
