//! Deferred ("install on quit") update staging — candidate C3(c) of
//! docs/scope-3.3.md.
//!
//! The frontend `UpdatePrompt.svelte` banner offers two paths once an update
//! is found: the existing immediate **Download & Install** (JS
//! `downloadAndInstall()` → `relaunch_app`) and a deferred **Install on
//! quit**. The deferred path cannot reuse the JS plugin API because
//! `downloadAndInstall()` applies the payload immediately on Windows (it
//! runs the MSI/NSIS installer, killing the app). Instead this module does
//! its own Rust-side `check()` + `download()` and holds the verified bytes
//! in managed state until the process exits, whereupon
//! [`install_pending_on_exit`] (wired to `tauri::RunEvent::Exit` in
//! `lib.rs::run`) applies them.
//!
//! Platform notes for the exit-time install:
//! - Windows: the installer is launched silently and relaunches the app by
//!   itself (plugin behaviour).
//! - macOS: the `.app` bundle is replaced in place; the next launch picks
//!   up the new version. We are inside `RunEvent::Exit`, so no manual
//!   respawn is attempted (respawning here races the exiting process and
//!   the single-instance plugin).
//! - Linux AppImage: the AppImage file is replaced in place; same
//!   next-launch story.

use parking_lot::Mutex;
use tauri::AppHandle;
use tauri_plugin_updater::Update;

/// Log tag prefix for this submodule (mirrors the `[CMD.MISC]` pattern).
const TAG: &str = "[UPDATER.BG]";

/// An update that has been downloaded and signature-verified but not yet
/// applied; applied at process exit if present.
struct StagedUpdate {
    update: Update,
    /// Verified payload bytes returned by [`Update::download`] — `install`
    /// consumes them.
    bytes: Vec<u8>,
}

/// Managed state holding at most one staged deferred update.
pub struct PendingUpdate(Mutex<Option<StagedUpdate>>);

impl PendingUpdate {
    pub fn new() -> Self {
        Self(Mutex::new(None))
    }
}

impl Default for PendingUpdate {
    fn default() -> Self {
        Self::new()
    }
}

/// Registers the pending-update state on the app. Called from `setup`.
#[cfg(desktop)]
pub fn manage(app: &tauri::AppHandle) {
    use tauri::Manager;
    app.manage(PendingUpdate::new());
    log::info!("{TAG} manage: PendingUpdate state registered");
}

/// Tauri command: check for an update, download it, verify its signature,
/// and hold it for install-on-quit. Returns the staged version string, or
/// `None` when the app is already current (e.g. the banner's information
/// went stale between the frontend `check()` and this call).
///
/// The network round-trips happen through the plugin's async reqwest client
/// (`Updater::check` / `Update::download` are async and non-blocking), but
/// per the #215 convention that heavy IO stays off the async runtime's
/// worker threads, the whole staging flow runs on a blocking-pool thread
/// via `block_on`. Progress callbacks are unused: the UI only needs
/// completion of the deferred stage.
#[cfg(desktop)]
#[tauri::command]
pub async fn stage_deferred_update(app: AppHandle) -> Result<Option<String>, String> {
    log::info!("{TAG} stage_deferred_update: ENTRY");
    let version = tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        tauri::async_runtime::block_on(async move {
            use tauri_plugin_updater::UpdaterExt;

            let updater = app
                .updater()
                .map_err(|e| format!("updater unavailable: {e}"))?;
            let Some(update) = updater
                .check()
                .await
                .map_err(|e| format!("update check failed: {e}"))?
            else {
                log::info!("{TAG} stage_deferred_update: no update available");
                return Ok::<Option<String>, String>(None);
            };
            let version = update.version.clone();
            let bytes = update
                .download(|_chunk_len, _content_length| {}, || {})
                .await
                .map_err(|e| format!("update download failed: {e}"))?;
            log::info!(
                "{TAG} stage_deferred_update: staged v{} ({} bytes)",
                version,
                bytes.len()
            );
            let state = app.state::<PendingUpdate>();
            *state.0.lock() = Some(StagedUpdate { update, bytes });
            Ok(Some(version))
        })
    })
    .await
    .map_err(|e| format!("stage_deferred_update spawn_blocking panicked: {:?}", e))??;
    log::info!("{TAG} stage_deferred_update: SUCCESS");
    Ok(version)
}

/// Applies any staged deferred update. Called from the `RunEvent::Exit`
/// arm of the run loop in `lib.rs::run` — i.e. after the event loop has
/// finished (tray Quit via `menu.rs` or the frontend `app_exit` command,
/// both of which funnel into `AppHandle::exit`). Never panics; a failed
/// install only logs so the plain-exit path stays intact.
#[cfg(desktop)]
pub fn install_pending_on_exit(app: &AppHandle) {
    use tauri::Manager;

    let Some(state) = app.try_state::<PendingUpdate>() else {
        return;
    };
    let Some(staged) = state.0.lock().take() else {
        return;
    };
    let version = staged.update.version.clone();
    log::info!(
        "{TAG} install_pending_on_exit: installing v{} on quit",
        version
    );
    match staged.update.install(staged.bytes) {
        Ok(()) => log::info!(
            "{TAG} install_pending_on_exit: v{} installed; takes effect on next launch \
             (Windows installer relaunches automatically)",
            version
        ),
        Err(e) => log::error!(
            "{TAG} install_pending_on_exit: FAILED for v{} - {}",
            version,
            e
        ),
    }
}
