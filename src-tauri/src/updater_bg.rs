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
//!
//! Failed-install marker (issue #244): the exit-time install runs after the
//! event loop has finished, so a failure cannot be reported to the user in
//! that session — the app is already going away. The failure is therefore
//! recorded in `<config_dir()>/update-install-failed.json` and surfaced on
//! the next launch through the Diagnostics snapshot (`DiagnosticsSnapshot.
//! failed_update_install`), which `build_snapshot` populates.
//! A marker is cleared whenever an install succeeds, and a corrupt or
//! unreadable marker reads as `None` rather than surfacing an error.

use parking_lot::Mutex;
use tauri::AppHandle;
use tauri_plugin_updater::Update;

/// Log tag prefix for this submodule (mirrors the `[CMD.MISC]` pattern).
const TAG: &str = "[UPDATER.BG]";

/// File name of the failed-install marker inside `config_dir()`.
const MARKER_FILE_NAME: &str = "update-install-failed.json";

/// An update that has been downloaded and signature-verified but not yet
/// applied; applied at process exit if present.
struct StagedUpdate {
    update: Update,
    /// Verified payload bytes returned by [`Update::download`] — `install`
    /// consumes them.
    bytes: Vec<u8>,
    /// Explicit user consent to install even when the staged version is
    /// stale (older than or equal to the running version, issue #431).
    /// Set only through the quit-time confirmation surface, which shows
    /// both versions before offering the override.
    forced: bool,
}

/// Outcome of a `stage_deferred_update` call (issue #431): the staged
/// version — `None` when the app is already current or the available
/// version was declined as stale — plus the running version from backend
/// truth (`CARGO_PKG_VERSION`), so the quit-time confirmation surface
/// can show staged-vs-current without an extra round-trip or new
/// frontend permissions. No `ts_rs` export: the shape is mirrored by a
/// local interface in `UpdatePrompt.svelte`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StageDeferredOutcome {
    pub staged: Option<String>,
    pub current: String,
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

// ---------------------------------------------------------------------
// Failed-install marker (issue #244)
// ---------------------------------------------------------------------

/// Record of an exit-time update install that failed. Written to
/// `<config_dir()>/update-install-failed.json` by
/// [`install_pending_on_exit`] and surfaced on the next launch.
///
/// `timestamp` is RFC 3339 in UTC — `to_rfc3339()` renders the offset as
/// `+00:00`, not `Z`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct FailedUpdateInstall {
    /// Version of the update whose install failed.
    pub version: String,
    /// Updater error text (English; Rust-side strings stay untranslated).
    pub error: String,
    /// RFC 3339 timestamp of the failed attempt.
    pub timestamp: String,
}

/// Resolves the marker path, or `None` when the config directory is
/// unavailable (the marker is best-effort: never fail an exit path over it).
fn marker_path() -> Option<std::path::PathBuf> {
    match crate::config::config_dir() {
        Ok(dir) => Some(dir.join(MARKER_FILE_NAME)),
        Err(e) => {
            log::warn!("{TAG} marker_path: config directory unavailable - {e}");
            None
        }
    }
}

/// Writes the marker to `path`. Path-parameterised so it is unit-testable
/// without an `AppHandle`.
fn write_failed_install_marker_at(
    path: &std::path::Path,
    rec: &FailedUpdateInstall,
) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(rec)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, json)
}

/// Reads the marker at `path`. A missing, unreadable or unparseable file
/// yields `None` (fail-safe: a corrupt marker must never panic or surface
/// an error to the user).
fn read_failed_install_marker_at(path: &std::path::Path) -> Option<FailedUpdateInstall> {
    let data = match std::fs::read(path) {
        Ok(data) => data,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            log::warn!(
                "{TAG} read_failed_install_marker: cannot read '{}' - {}",
                path.display(),
                e
            );
            return None;
        }
    };
    match serde_json::from_slice::<FailedUpdateInstall>(&data) {
        Ok(rec) => Some(rec),
        Err(e) => {
            log::warn!(
                "{TAG} read_failed_install_marker: ignoring corrupt marker at '{}' - {}",
                path.display(),
                e
            );
            None
        }
    }
}

/// Removes the marker at `path`; a missing file is already the desired
/// state, so it succeeds. Any other failure (read-only directory, the
/// file held open by a scanner) is returned so `clear_failed_update_install`
/// can report it instead of silently leaving the record on disk.
fn clear_failed_install_marker_at(path: &std::path::Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => {
            log::warn!(
                "{TAG} clear_failed_install_marker: cannot remove '{}' - {}",
                path.display(),
                e
            );
            Err(e)
        }
    }
}

/// Persists `rec` to the standard marker location. Best-effort: a write
/// failure is logged, never returned — this runs on the exit path.
fn write_failed_install_marker(rec: &FailedUpdateInstall) {
    let Some(path) = marker_path() else {
        return;
    };
    match write_failed_install_marker_at(&path, rec) {
        Ok(()) => log::info!(
            "{TAG} write_failed_install_marker: recorded failed install of v{} at '{}'",
            rec.version,
            path.display()
        ),
        Err(e) => log::warn!(
            "{TAG} write_failed_install_marker: cannot write '{}' - {}",
            path.display(),
            e
        ),
    }
}

/// Reads the standard marker location, or `None` when absent/corrupt.
pub fn read_failed_install_marker() -> Option<FailedUpdateInstall> {
    let path = marker_path()?;
    read_failed_install_marker_at(&path)
}

/// Removes the standard marker location (no-op when absent). Returns
/// `Ok(())` when the file is gone or absent; returns the underlying
/// `io::Error` when something else prevents removal so callers can
/// surface the failure instead of silently leaving the record on disk.
fn clear_failed_install_marker() -> std::io::Result<()> {
    let Some(path) = marker_path() else {
        return Ok(());
    };
    clear_failed_install_marker_at(&path)
}

/// Tauri command: discards the recorded failed install (the Diagnostics
/// "Dismiss" action). Reports a removal failure to the frontend via the
/// `Result` so it can set `diagnostics.failedInstallDismissFailed` rather
/// than claiming success while the file persists.
#[cfg(desktop)]
#[tauri::command]
pub fn clear_failed_update_install() -> Result<(), String> {
    log::info!("{TAG} clear_failed_update_install: ENTRY");
    clear_failed_install_marker().map_err(|e| {
        log::warn!("{TAG} clear_failed_update_install: FAILED - {}", e);
        format!("cannot remove failed-install marker: {e}")
    })?;
    log::info!("{TAG} clear_failed_update_install: SUCCESS");
    Ok(())
}

// ---------------------------------------------------------------------
// Stale-stage guard (issue #431)
// ---------------------------------------------------------------------

/// File name of the stale-stage skip marker inside `config_dir()`.
const STALE_SKIPPED_FILE_NAME: &str = "update-stale-skipped.json";

/// Record of a staged update that was declined because it was stale —
/// older than or equal to the running version. Written to
/// `<config_dir()>/update-stale-skipped.json` by `stage_deferred_update`
/// and [`install_pending_on_exit`]; `stage_deferred_update` consults it
/// to avoid re-downloading the same stale stage on every subsequent
/// attempt, so a skipped stale stage does not re-prompt on every quit.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct StaleSkippedUpdate {
    staged_version: String,
    current_version: String,
    /// RFC 3339 in UTC (`to_rfc3339()` renders the offset as `+00:00`).
    timestamp: String,
}

/// Parses the numeric core of a semver version string: an optional leading
/// `v`, `major.minor.patch`, an optional `-prerelease`, and optional
/// `+build` metadata (ignored in ordering per semver §10). Returns
/// `(major, minor, patch, prerelease)`, or `None` when the string is not
/// a well-formed triple. Deliberately dependency-free (`Cargo.toml` is
/// outside this slice's ownership): ordering only needs the numeric
/// core plus the release-vs-prerelease rule.
fn parse_semver_core(v: &str) -> Option<(u64, u64, u64, Option<String>)> {
    let s = v.trim();
    let s = s
        .strip_prefix('v')
        .or_else(|| s.strip_prefix('V'))
        .unwrap_or(s);
    let s = s.split('+').next().unwrap_or(s);
    let (core, prerelease) = match s.split_once('-') {
        Some((c, p)) => (c, Some(p.to_string())),
        None => (s, None),
    };
    if let Some(p) = &prerelease {
        if p.is_empty() {
            return None;
        }
    }
    let mut parts = core.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch = parts.next()?.parse::<u64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch, prerelease))
}

/// Semver ordering for two version strings. A plain release outranks its
/// own prereleases; two prereleases compare lexically (a documented
/// simplification — updater feed versions are plain numeric triples, so
/// the prerelease arm only needs to be deterministic, not dot-separated
/// aware). Returns `None` when either side is unparseable.
fn compare_semver(a: &str, b: &str) -> Option<std::cmp::Ordering> {
    use std::cmp::Ordering;
    let (major_a, minor_a, patch_a, pre_a) = parse_semver_core(a)?;
    let (major_b, minor_b, patch_b, pre_b) = parse_semver_core(b)?;
    match (major_a, minor_a, patch_a).cmp(&(major_b, minor_b, patch_b)) {
        Ordering::Equal => match (pre_a, pre_b) {
            (None, None) => Some(Ordering::Equal),
            (None, Some(_)) => Some(Ordering::Greater),
            (Some(_), None) => Some(Ordering::Less),
            (Some(x), Some(y)) => Some(x.cmp(&y)),
        },
        ord => Some(ord),
    }
}

/// True when `staged` is older than or equal to `current` under semver
/// ordering. Unparseable input fails OPEN (`false`): a version scheme we
/// do not understand must never block an update the updater feed offered;
/// the case is logged so it stays visible in the log file.
fn is_stale_version(staged: &str, current: &str) -> bool {
    match compare_semver(staged, current) {
        Some(ord) => ord != std::cmp::Ordering::Greater,
        None => {
            log::debug!(
                "{TAG} is_stale_version: unparseable version(s) staged={staged:?} current={current:?}; treating as fresh"
            );
            false
        }
    }
}

/// Resolves the stale-skip marker path, or `None` when the config
/// directory is unavailable (best-effort: never fail a staging or exit
/// path over it).
fn stale_skipped_path() -> Option<std::path::PathBuf> {
    match crate::config::config_dir() {
        Ok(dir) => Some(dir.join(STALE_SKIPPED_FILE_NAME)),
        Err(e) => {
            log::warn!("{TAG} stale_skipped_path: config directory unavailable - {e}");
            None
        }
    }
}

/// Writes the stale-skip marker to `path`. Path-parameterised so it is
/// unit-testable without an `AppHandle`.
fn write_stale_skipped_marker_at(
    path: &std::path::Path,
    rec: &StaleSkippedUpdate,
) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(rec)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, json)
}

/// Reads the stale-skip marker at `path`. A missing, unreadable or
/// unparseable file yields `None` (fail-safe, mirroring the
/// failed-install marker).
fn read_stale_skipped_marker_at(path: &std::path::Path) -> Option<StaleSkippedUpdate> {
    let data = match std::fs::read(path) {
        Ok(data) => data,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            log::warn!(
                "{TAG} read_stale_skipped_marker: cannot read '{}' - {}",
                path.display(),
                e
            );
            return None;
        }
    };
    match serde_json::from_slice::<StaleSkippedUpdate>(&data) {
        Ok(rec) => Some(rec),
        Err(e) => {
            log::warn!(
                "{TAG} read_stale_skipped_marker: ignoring corrupt marker at '{}' - {}",
                path.display(),
                e
            );
            None
        }
    }
}

/// Removes the stale-skip marker at `path`; a missing file is already the
/// desired state, so it succeeds.
fn clear_stale_skipped_marker_at(path: &std::path::Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => {
            log::warn!(
                "{TAG} clear_stale_skipped_marker: cannot remove '{}' - {}",
                path.display(),
                e
            );
            Err(e)
        }
    }
}

/// Persists `rec` to the standard stale-skip marker location.
/// Best-effort: a write failure is logged, never returned.
fn write_stale_skipped_marker(rec: &StaleSkippedUpdate) {
    let Some(path) = stale_skipped_path() else {
        return;
    };
    match write_stale_skipped_marker_at(&path, rec) {
        Ok(()) => log::info!(
            "{TAG} write_stale_skipped_marker: recorded stale skip of v{} (current v{}) at '{}'",
            rec.staged_version,
            rec.current_version,
            path.display()
        ),
        Err(e) => log::warn!(
            "{TAG} write_stale_skipped_marker: cannot write '{}' - {}",
            path.display(),
            e
        ),
    }
}

/// Reads the standard stale-skip marker location, or `None` when
/// absent/corrupt.
fn read_stale_skipped_marker() -> Option<StaleSkippedUpdate> {
    let path = stale_skipped_path()?;
    read_stale_skipped_marker_at(&path)
}

/// Removes the standard stale-skip marker location (no-op when absent).
fn clear_stale_skipped_marker() {
    let Some(path) = stale_skipped_path() else {
        return;
    };
    if let Err(e) = clear_stale_skipped_marker_at(&path) {
        log::warn!(
            "{TAG} clear_stale_skipped_marker: cannot remove '{}' - {}",
            path.display(),
            e
        );
    }
}

/// Tauri command: check for an update, download it, verify its signature,
/// and hold it for install-on-quit. Returns a [`StageDeferredOutcome`]:
/// `staged` is the staged version string, or `None` when the app is
/// already current (e.g. the banner's information went stale between the
/// frontend `check()` and this call) or when the available version is
/// stale — older than or equal to the running version (issue #431; a
/// skip marker is recorded so the same stale stage is not re-downloaded
/// on the next attempt). `current` is always the running version, so the
/// quit-time surface can show staged-vs-current without another call.
///
/// `force` is the explicit downgrade override from the quit-time
/// confirmation surface: the frontend shows the staged-vs-current versions
/// first and passes `true` only when the user confirmed the install knowing
/// both. The immediate download+install path is untouched by this flag.
///
/// The network round-trips happen through the plugin's async reqwest client
/// (`Updater::check` / `Update::download` are async and non-blocking), but
/// per the #215 convention that heavy IO stays off the async runtime's
/// worker threads, the whole staging flow runs on a blocking-pool thread
/// via `block_on`. Progress callbacks are unused: the UI only needs
/// completion of the deferred stage.
#[cfg(desktop)]
#[tauri::command]
pub async fn stage_deferred_update(
    window: tauri::Window,
    app: AppHandle,
    force: bool,
) -> Result<StageDeferredOutcome, String> {
    // Issue #241: update staging downloads + verifies payloads into managed
    // state; UpdatePrompt is main-window-only so detached windows never
    // legitimately stage. Guarded via the commands-layer helper.
    crate::commands::require_main_window(&window)?;
    log::info!("{TAG} stage_deferred_update: ENTRY");
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        tauri::async_runtime::block_on(async move {
            use tauri_plugin_updater::UpdaterExt;

            let current = env!("CARGO_PKG_VERSION").to_string();
            let updater = app
                .updater()
                .map_err(|e| format!("updater unavailable: {e}"))?;
            let Some(update) = updater
                .check()
                .await
                .map_err(|e| format!("update check failed: {e}"))?
            else {
                log::info!("{TAG} stage_deferred_update: no update available");
                return Ok::<StageDeferredOutcome, String>(StageDeferredOutcome {
                    staged: None,
                    current,
                });
            };
            let version = update.version.clone();
            // Issue #431: never stage a stale update unless the user
            // explicitly forced it after seeing both versions on the
            // quit-time confirmation surface. A previously skipped stale
            // stage short-circuits here so the payload is not downloaded
            // again on every subsequent attempt.
            if !force {
                if let Some(skipped) = read_stale_skipped_marker() {
                    if skipped.staged_version == version {
                        log::info!(
                            "{TAG} stage_deferred_update: v{} already skipped as stale (current v{}); not re-downloading",
                            version,
                            current
                        );
                        return Ok::<StageDeferredOutcome, String>(StageDeferredOutcome {
                            staged: None,
                            current,
                        });
                    }
                }
                if is_stale_version(&version, &current) {
                    log::info!(
                        "{TAG} stage_deferred_update: v{} <= current v{}; skipping stale stage",
                        version,
                        current
                    );
                    write_stale_skipped_marker(&StaleSkippedUpdate {
                        staged_version: version,
                        current_version: current.clone(),
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    });
                    return Ok::<StageDeferredOutcome, String>(StageDeferredOutcome {
                        staged: None,
                        current,
                    });
                }
            }
            let bytes = update
                .download(|_chunk_len, _content_length| {}, || {})
                .await
                .map_err(|e| format!("update download failed: {e}"))?;
            log::info!(
                "{TAG} stage_deferred_update: staged v{} ({} bytes){}",
                version,
                bytes.len(),
                if force { " (forced)" } else { "" }
            );
            let state = app.state::<PendingUpdate>();
            *state.0.lock() = Some(StagedUpdate { update, bytes, forced: force });
            // Clear any older failed-install marker now that a fresh update
            // is staged. On Windows the plugin's install_inner exits the
            // process (ShellExecuteW + process::exit(0)), so the Ok arm of
            // install_pending_on_exit is unreachable there; the stage path
            // is the one reliable cross-platform success signal.
            let _ = clear_failed_install_marker();
            // A (possibly forced) fresh stage supersedes any older
            // stale-skip record for a previous version.
            clear_stale_skipped_marker();
            Ok(StageDeferredOutcome {
                staged: Some(version),
                current,
            })
        })
    })
    .await
    .map_err(|e| format!("stage_deferred_update spawn_blocking panicked: {:?}", e))??;
    log::info!("{TAG} stage_deferred_update: SUCCESS");
    Ok(outcome)
}

/// Applies any staged deferred update. Called from the `RunEvent::Exit`
/// arm of the run loop in `lib.rs::run` — i.e. after the event loop has
/// finished (tray Quit via `menu.rs` or the frontend `app_exit` command,
/// both of which funnel into `AppHandle::exit`). Never panics; a failed
/// install only logs so the plain-exit path stays intact, and additionally
/// records a marker (issue #244) that the next launch surfaces in
/// Diagnostics. Any older marker is cleared once a fresh update is
/// successfully staged (`stage_deferred_update`), and again here on the
/// `Ok` arm — but that arm is unreachable on Windows, where the updater
/// plugin's `install_inner` runs the installer and then calls
/// `std::process::exit(0)` without returning.
///
/// Stale-stage guard (issue #431): a staged version older than or equal
/// to the running version is never installed — it is skipped with a log
/// line and a skip marker, unless the stage was explicitly forced through
/// the quit-time confirmation surface (which shows both versions first).
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
    // Issue #431: a stale stage (older than or equal to the running
    // version) must never downgrade the app silently at quit. Skip it
    // with a log line and a skip marker so the next staging attempt
    // short-circuits instead of re-downloading; an explicitly forced
    // stage (user confirmed both versions) still installs.
    let current = env!("CARGO_PKG_VERSION");
    if !staged.forced && is_stale_version(&version, current) {
        log::info!(
            "{TAG} install_pending_on_exit: staged v{version} <= current v{current}; skipping stale install",
        );
        write_stale_skipped_marker(&StaleSkippedUpdate {
            staged_version: version,
            current_version: current.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
        return;
    }
    log::info!(
        "{TAG} install_pending_on_exit: installing v{} on quit",
        version
    );
    match staged.update.install(staged.bytes) {
        Ok(()) => {
            // Best-effort: the install succeeded, so a stale marker is
            // already harmless, but prefer to clear it so Diagnostics does
            // not keep reporting a failure that has since been fixed.
            if let Err(e) = clear_failed_install_marker() {
                log::warn!(
                    "{TAG} install_pending_on_exit: v{} installed but marker clear failed - {}",
                    version,
                    e
                );
            }
            log::info!(
                "{TAG} install_pending_on_exit: v{} installed; takes effect on next launch \
                 (Windows installer relaunches automatically)",
                version
            );
        }
        Err(e) => {
            write_failed_install_marker(&FailedUpdateInstall {
                version: version.clone(),
                error: e.to_string(),
                timestamp: chrono::Utc::now().to_rfc3339(),
            });
            log::error!(
                "{TAG} install_pending_on_exit: FAILED for v{} - {}",
                version,
                e
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fresh unique temp directory per call. Returns
    /// `(dir, marker_path)`; callers remove `dir` when done.
    fn temp_marker_path(label: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "pj-updater-bg-{}-{}-{}",
            label,
            std::process::id(),
            unique
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(MARKER_FILE_NAME);
        (dir, path)
    }

    fn sample_record() -> FailedUpdateInstall {
        FailedUpdateInstall {
            version: "9.9.9".to_string(),
            error: "installer exited with code 1".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }

    #[test]
    fn test_marker_round_trip() {
        let (dir, path) = temp_marker_path("roundtrip");
        let record = sample_record();

        write_failed_install_marker_at(&path, &record).expect("marker write must succeed");
        assert!(path.exists(), "marker file must exist after a write");

        let read = read_failed_install_marker_at(&path).expect("written marker must read back");
        assert_eq!(read.version, record.version);
        assert_eq!(read.error, record.error);
        assert_eq!(read.timestamp, record.timestamp);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_clear_marker_makes_read_none() {
        let (dir, path) = temp_marker_path("clear");
        write_failed_install_marker_at(&path, &sample_record()).expect("marker write must succeed");
        assert!(
            read_failed_install_marker_at(&path).is_some(),
            "marker must be present before clearing"
        );

        clear_failed_install_marker_at(&path).expect("clearing a present marker must succeed");
        assert!(
            read_failed_install_marker_at(&path).is_none(),
            "cleared marker must read as None"
        );

        // Clearing an already-absent marker is already the desired state,
        // so it succeeds rather than reporting a spurious failure.
        clear_failed_install_marker_at(&path).expect("clearing an absent marker must succeed");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A marker that cannot be removed must be reported: the Dismiss
    /// action's `Result` exists so the UI can say "failed" instead of
    /// nulling the snapshot while the record survives on disk. A
    /// directory in the marker's place reproduces an unremovable path
    /// on every platform (remove_file fails with a non-NotFound error).
    #[test]
    fn test_unremovable_marker_reports_error() {
        let (dir, path) = temp_marker_path("unremovable");
        std::fs::create_dir(&path).unwrap();

        let err = clear_failed_install_marker_at(&path)
            .expect_err("a non-removable marker must surface an error");
        assert_ne!(
            err.kind(),
            std::io::ErrorKind::NotFound,
            "the reported error must be the real removal failure"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_truncated_marker_reads_none() {
        let (dir, path) = temp_marker_path("truncated");
        // A crash mid-write leaves a prefix of valid JSON — must not panic
        // and must not be surfaced as a partial record.
        let json = serde_json::to_vec(&sample_record()).expect("serialize sample");
        std::fs::write(&path, &json[..json.len() / 2]).unwrap();

        assert!(read_failed_install_marker_at(&path).is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_garbage_marker_reads_none() {
        let (dir, path) = temp_marker_path("garbage");
        std::fs::write(&path, b"\x00\x01not json at all\xff").unwrap();

        assert!(read_failed_install_marker_at(&path).is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_missing_marker_reads_none() {
        let (dir, path) = temp_marker_path("missing");

        assert!(!path.exists(), "fixture must start with no marker");
        assert!(read_failed_install_marker_at(&path).is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }
    // -----------------------------------------------------------------
    // Stale-stage guard (issue #431)
    // -----------------------------------------------------------------

    #[test]
    fn test_parse_semver_core_triple() {
        assert_eq!(parse_semver_core("4.1.1"), Some((4, 1, 1, None)));
        // Leading `v`, surrounding whitespace, and build metadata are
        // tolerated/ignored; prereleases are captured.
        assert_eq!(
            parse_semver_core("  v4.2.0-beta.1+build.5 "),
            Some((4, 2, 0, Some("beta.1".to_string())))
        );
        assert_eq!(parse_semver_core("1.0.0+nightly"), Some((1, 0, 0, None)));
    }

    #[test]
    fn test_parse_semver_core_rejects_malformed() {
        assert!(parse_semver_core("").is_none());
        assert!(parse_semver_core("4.1").is_none());
        assert!(parse_semver_core("4.1.1.1").is_none());
        assert!(parse_semver_core("four.one.one").is_none());
        assert!(parse_semver_core("1.0.0-").is_none());
    }

    #[test]
    fn test_compare_semver_orders_releases() {
        use std::cmp::Ordering;
        assert_eq!(compare_semver("4.1.0", "4.1.1"), Some(Ordering::Less));
        assert_eq!(compare_semver("4.1.1", "4.1.1"), Some(Ordering::Equal));
        assert_eq!(compare_semver("4.2.0", "4.1.9"), Some(Ordering::Greater));
        // Numeric, not lexical: 10 > 9 in every position.
        assert_eq!(compare_semver("4.1.10", "4.1.9"), Some(Ordering::Greater));
        // Build metadata never affects precedence.
        assert_eq!(
            compare_semver("4.1.1+build.1", "4.1.1+build.2"),
            Some(Ordering::Equal)
        );
        // A release outranks its own prerelease.
        assert_eq!(
            compare_semver("4.1.1", "4.1.1-rc.1"),
            Some(Ordering::Greater)
        );
        assert_eq!(compare_semver("nope", "4.1.1"), None);
        assert_eq!(compare_semver("4.1.1", "nope"), None);
    }

    #[test]
    fn test_is_stale_version_guards_downgrades_and_equal() {
        // Older staged versions are stale, and so is an equal version
        // (reinstalling the running build at quit is never intended).
        assert!(is_stale_version("4.1.0", "4.1.1"));
        assert!(is_stale_version("4.1.1", "4.1.1"));
        assert!(is_stale_version("3.9.9", "4.0.0"));
        // Newer staged versions are fresh.
        assert!(!is_stale_version("4.1.2", "4.1.1"));
        assert!(!is_stale_version("5.0.0", "4.9.9"));
        // Unparseable input fails open: never block an offered update.
        assert!(!is_stale_version("nightly", "4.1.1"));
        assert!(!is_stale_version("4.1.2", "nightly"));
    }

    fn sample_stale_record() -> StaleSkippedUpdate {
        StaleSkippedUpdate {
            staged_version: "4.1.0".to_string(),
            current_version: "4.1.1".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }

    #[test]
    fn test_stale_marker_round_trip() {
        let (dir, path) = temp_marker_path("stale-roundtrip");
        let record = sample_stale_record();

        write_stale_skipped_marker_at(&path, &record).expect("marker write must succeed");
        let read = read_stale_skipped_marker_at(&path).expect("written marker must read back");
        assert_eq!(read.staged_version, record.staged_version);
        assert_eq!(read.current_version, record.current_version);

        clear_stale_skipped_marker_at(&path).expect("clearing a present marker must succeed");
        assert!(read_stale_skipped_marker_at(&path).is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_stale_marker_missing_and_garbage_read_none() {
        let (dir, path) = temp_marker_path("stale-missing");
        assert!(read_stale_skipped_marker_at(&path).is_none());
        std::fs::write(&path, b"\x00\x01not json at all\xff").unwrap();
        assert!(read_stale_skipped_marker_at(&path).is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
