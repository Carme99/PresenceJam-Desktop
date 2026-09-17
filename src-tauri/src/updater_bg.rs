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
//!
//! Staging UX (issue #590): the download used to be a black box — a
//! multi-minute payload with no progress and no way to abort, and once
//! staged the only exit was applying it at the next quit. The stage now
//! streams throttled `update-stage-progress` events (see
//! [`StageProgressThrottle`]) and [`cancel_deferred_update`] discards the
//! staged update on demand, which also releases the verified payload bytes
//! instead of holding them for the rest of the session. The bytes are still
//! held in memory rather than re-read from disk at exit on purpose: the
//! plugin verifies the signature inside `download()`, so a file-backed
//! payload would be installable after an unverified post-stage swap.

use crate::config::UpdateChannel;
use parking_lot::Mutex;
use std::future::Future;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::{Update, UpdaterExt};
use url::Url;

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
// ---------------------------------------------------------------------
// Staging progress (issue #590)
// ---------------------------------------------------------------------

/// Emitted on `update-stage-progress` while a deferred update downloads, so
/// the banner can drive its progress bar instead of sitting on "Preparing…"
/// for the whole payload. No `ts_rs` export: the shape is mirrored by a
/// local interface in `UpdatePrompt.svelte` (same convention as
/// [`StageDeferredOutcome`]).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct StageProgress {
    /// Bytes downloaded so far. The plugin's callback reports the running
    /// total, not the size of the chunk just read.
    pub downloaded: u64,
    /// Total payload size, when the server sent a `Content-Length`.
    pub total: Option<u64>,
}

/// Minimum wall-clock gap between two progress emissions.
const STAGE_PROGRESS_MIN_INTERVAL: Duration = Duration::from_millis(250);

/// Whole-percent advance that forces an emission even inside the interval:
/// a fast link still shows movement, and a slow one cannot flood the webview
/// with one event per chunk.
const STAGE_PROGRESS_PCT_STEP: u64 = 5;

/// Emission throttle for `update-stage-progress`. The first chunk always
/// emits, so the UI leaves "Preparing…" immediately; afterwards an event
/// goes out once [`STAGE_PROGRESS_MIN_INTERVAL`] has elapsed or the
/// whole-percent position advanced by [`STAGE_PROGRESS_PCT_STEP`].
///
/// Pure state machine over `Instant`s, so it is unit-tested with synthetic
/// clocks and no webview or network.
#[derive(Debug)]
struct StageProgressThrottle {
    last_emit: Option<Instant>,
    last_pct: u64,
}

impl StageProgressThrottle {
    fn new() -> Self {
        Self {
            last_emit: None,
            last_pct: 0,
        }
    }

    /// Returns the event to emit for this chunk, or `None` when throttled.
    fn observe(
        &mut self,
        now: Instant,
        downloaded: u64,
        total: Option<u64>,
    ) -> Option<StageProgress> {
        let pct = percent_of(downloaded, total);
        let due = match self.last_emit {
            None => true,
            Some(last) => {
                now.duration_since(last) >= STAGE_PROGRESS_MIN_INTERVAL
                    || pct >= self.last_pct.saturating_add(STAGE_PROGRESS_PCT_STEP)
            }
        };
        if !due {
            return None;
        }
        self.last_emit = Some(now);
        self.last_pct = pct;
        Some(StageProgress { downloaded, total })
    }
}

/// Whole-percent position of `downloaded` within `total` (clamped to 100).
/// `0` when the server sent no length (or a zero one), which leaves the
/// interval as the only throttle.
fn percent_of(downloaded: u64, total: Option<u64>) -> u64 {
    match total {
        Some(total) if total > 0 => (downloaded.saturating_mul(100) / total).min(100),
        _ => 0,
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
/// Tauri command: discards a staged deferred update (issue #590). Before
/// this existed, staging was one-way — the only exit was applying the
/// payload at the next quit, so an accidental click could not be undone and
/// the verified bytes stayed resident for the rest of the session. Dropping
/// the [`StagedUpdate`] releases that payload immediately.
#[cfg(desktop)]
#[tauri::command]
pub fn cancel_deferred_update(app: AppHandle) -> Result<(), String> {
    use tauri::Manager;

    let state = app.state::<PendingUpdate>();
    if state.0.lock().take().is_some() {
        log::info!("{TAG} cancel_deferred_update: staged update discarded");
    } else {
        log::debug!("{TAG} cancel_deferred_update: nothing staged");
    }
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

// ---------------------------------------------------------------------
// Update channel resolution + check (issue #678)
// ---------------------------------------------------------------------

/// Stable-channel update manifest: the published release's `latest.json`.
const STABLE_ENDPOINT: &str =
    "https://github.com/Carme99/PresenceJam-Desktop/releases/latest/download/latest.json";

/// Beta-channel update manifest: the stable URL with `latest-beta.json`.
///
/// 4.7.0 ships the channel switch without publishing a beta build, so this
/// URL answers `404` and the check falls through to [`STABLE_ENDPOINT`].
const BETA_ENDPOINT: &str =
    "https://github.com/Carme99/PresenceJam-Desktop/releases/latest/download/latest-beta.json";

/// Ordered update-endpoint list for `channel` (issue #678).
///
/// The stable URL is always last: [`walk_endpoints`] keeps the first endpoint
/// that answers, so a not-yet-published beta manifest falls through to the
/// stable release instead of failing the whole check.
pub fn update_endpoints(channel: UpdateChannel) -> Vec<String> {
    match channel {
        UpdateChannel::Stable => vec![STABLE_ENDPOINT.to_string()],
        UpdateChannel::Beta => vec![BETA_ENDPOINT.to_string(), STABLE_ENDPOINT.to_string()],
    }
}

/// Parses [`update_endpoints`] into the `Url` list the updater builder takes.
fn endpoint_urls(channel: UpdateChannel) -> Result<Vec<Url>, String> {
    update_endpoints(channel)
        .into_iter()
        .map(|raw| Url::parse(&raw).map_err(|e| format!("invalid update endpoint {raw}: {e}")))
        .collect()
}

/// Runs `attempt` over `urls` in order until one of them answers (issue
/// #678), logging every endpoint that was skipped.
///
/// Mirrors the updater plugin's own multi-endpoint `check()` loop: it stops at
/// the first endpoint that produced a manifest (including one that is no
/// newer than the running build, i.e. `Ok(None)`), falls through only when an
/// endpoint FAILED, and reports the last failure when none of them answered.
/// The difference is the log line — the plugin's fall-through is silent at the
/// default log level, so a beta check quietly serving the stable release would
/// otherwise be indistinguishable from a beta release having shipped.
async fn walk_endpoints<T, F, Fut>(urls: &[Url], mut attempt: F) -> Result<T, String>
where
    F: FnMut(Url) -> Fut,
    Fut: Future<Output = Result<T, String>>,
{
    let mut last_error: Option<String> = None;
    for (idx, url) in urls.iter().enumerate() {
        match attempt(url.clone()).await {
            Ok(found) => {
                if idx > 0 {
                    log::info!(
                        "{TAG} update check: {prev} did not serve a manifest; falling through \
                         to {url}",
                        prev = urls[idx - 1]
                    );
                }
                return Ok(found);
            }
            Err(e) => {
                log::info!("{TAG} update check: {url} failed ({e}); trying the next endpoint");
                last_error = Some(e);
            }
        }
    }
    Err(last_error.unwrap_or_else(|| "no update endpoints configured".to_string()))
}

/// Checks for an update on the configured release channel.
///
/// The endpoint list comes from `AppConfig.updates.channel`, NOT from the
/// static `plugins.updater.endpoints` entry in `tauri.conf.json`:
/// `UpdaterBuilder::endpoints` replaces that config value, so the
/// channel-driven list governs every Rust-side check ([`check_for_update`] and
/// [`stage_deferred_update`]). The JS `@tauri-apps/plugin-updater` path
/// (`check()` / `downloadAndInstall()`) cannot take endpoints, so it keeps
/// using the static config entry — which is why `UpdatePrompt.svelte` offers
/// it on the stable channel only.
async fn check_with_channel(
    app: &AppHandle,
    channel: UpdateChannel,
) -> Result<Option<Update>, String> {
    let urls = endpoint_urls(channel)?;
    let listed = urls.iter().map(Url::as_str).collect::<Vec<_>>().join(", ");
    log::info!("{TAG} update check: channel={channel:?} endpoints=[{listed}]");
    walk_endpoints(&urls, |url| async move {
        let updater = app
            .updater_builder()
            .endpoints(vec![url])
            .map_err(|e| format!("invalid update endpoint: {e}"))?
            .build()
            .map_err(|e| format!("updater unavailable: {e}"))?;
        updater.check().await.map_err(|e| e.to_string())
    })
    .await
}

/// Banner payload of [`check_for_update`] (issue #678). No `ts_rs` export:
/// the shape is mirrored by a local interface in `UpdatePrompt.svelte` (same
/// convention as [`StageDeferredOutcome`]).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct UpdateCheckOutcome {
    /// The version the manifest announces.
    pub version: String,
    /// Release notes from the manifest (`body` in the update manifest).
    /// Deliberately the manifest's `notes` rather than a hand-rendered one.
    pub notes: Option<String>,
    /// Publish date exactly as the manifest spells it — RFC 3339, e.g.
    /// `2026-09-16T21:07:35Z`.
    pub pub_date: Option<String>,
}

/// The manifest's own `pub_date` literal.
///
/// Read from the release JSON rather than rendered from [`Update::date`]:
/// `time::OffsetDateTime::to_string()` uses `time`'s `SmartDisplay` human form
/// (`2026-09-16 21:07:35.0 +00:00:00`), which is neither RFC 3339 nor what the
/// manifest published. A manifest without the key (or with a non-string value)
/// reports `None` instead of inventing a date.
fn manifest_pub_date(raw_json: &serde_json::Value) -> Option<String> {
    raw_json
        .get("pub_date")
        .and_then(|value| value.as_str())
        .map(str::to_string)
}

/// Tauri command: check for an update on the configured channel (issue #678).
///
/// The banner uses this instead of the plugin's JS `check()` because the JS
/// API cannot pass endpoints — it is hard-wired to the static
/// `plugins.updater.endpoints` entry, which is the stable manifest.
///
/// #215 convention: config read and network both happen on a blocking-pool
/// thread. `Ok(None)` means the running build is already current; `Err` means
/// every endpoint of the channel failed.
#[cfg(desktop)]
#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> Result<Option<UpdateCheckOutcome>, String> {
    let found = tauri::async_runtime::spawn_blocking(move || {
        let channel = crate::config::load_config()
            .map_err(|e| {
                // A read failure must never be a silent, permanent
                // "no update" state: the reason lands in the log file, and the
                // next check (mount / 24h tick) re-reads the config, so a
                // transient failure restores itself. Parse failures do not
                // reach here at all — `load_config` quarantines the file and
                // boots on defaults (verified in config.rs).
                log::warn!("{TAG} check_for_update: config unreadable ({e}); update check failed");
                format!("config load failed: {e}")
            })?
            .updates
            .channel;
        tauri::async_runtime::block_on(check_with_channel(&app, channel))
    })
    .await
    .map_err(|e| format!("check_for_update spawn_blocking panicked: {e:?}"))??;
    match &found {
        Some(update) => log::info!("{TAG} check_for_update: v{} available", update.version),
        None => log::info!("{TAG} check_for_update: already current"),
    }
    Ok(found.map(|update| UpdateCheckOutcome {
        version: update.version,
        notes: update.body,
        pub_date: manifest_pub_date(&update.raw_json),
    }))
}

// ---------------------------------------------------------------------
// Stage completion (issue #678)
// ---------------------------------------------------------------------

/// Emitted once on `update-stage-complete` after a deferred update has been
/// staged successfully, so an always-mounted consumer can notify without
/// being the webview that invoked [`stage_deferred_update`]. No `ts_rs`
/// export: the shape is mirrored by a local interface where it is consumed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct StageComplete {
    /// Version that is now staged for install on quit.
    pub version: String,
}

/// Hands `emit` the staged version iff a deferred stage actually succeeded.
///
/// Factored out of [`stage_deferred_update`] so the two rules its consumers
/// depend on are testable without a webview: the event carries the staged
/// version, and it does NOT fire for an outcome that staged nothing.
/// `stage_deferred_update` returns `staged: None` both when the app is already
/// current and when the candidate was declined as stale, and announcing either
/// as "ready to install" would be a false notification.
fn emit_stage_complete<E: FnOnce(&str)>(staged: Option<&str>, emit: E) {
    if let Some(version) = staged {
        emit(version);
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
/// via `block_on`. Progress is streamed on `update-stage-progress` through
/// [`StageProgressThrottle`] (issue #590); the payload itself is only
/// reported once, at completion.
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
    let emit_app = app.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        tauri::async_runtime::block_on(async move {
            let current = env!("CARGO_PKG_VERSION").to_string();
            let channel = crate::config::load_config()
                .map_err(|e| {
                    // The channel decides which manifest is staged, so an
                    // unreadable config is a visible failure for THIS call —
                    // never a silent guess — with the reason in the log file.
                    // The next attempt re-reads the config.
                    log::warn!(
                        "{TAG} stage_deferred_update: config unreadable ({e}); cannot resolve \
                         the release channel"
                    );
                    format!("config load failed: {e}")
                })?
                .updates
                .channel;
            let Some(update) = check_with_channel(&app, channel).await? else {
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
            // Issue #590: stream throttled progress while the payload
            // downloads, so a multi-minute stage is not a black box.
            let progress_app = app.clone();
            let mut progress = StageProgressThrottle::new();
            let bytes = update
                .download(
                    move |chunk_len, content_length| {
                        if let Some(event) =
                            progress.observe(Instant::now(), chunk_len as u64, content_length)
                        {
                            let _ = progress_app.emit("update-stage-progress", event);
                        }
                    },
                    || {},
                )
                .await
                .map_err(|e| format!("update download failed: {e}"))?;
            // Terminal event: the last throttled chunk can land short of the
            // end (and a `total` the server never sent leaves the bar
            // indeterminate), so the completion is announced explicitly.
            let staged_len = bytes.len() as u64;
            let _ = app.emit(
                "update-stage-progress",
                StageProgress {
                    downloaded: staged_len,
                    total: Some(staged_len),
                },
            );
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
    emit_stage_complete(outcome.staged.as_deref(), |version| {
        let payload = StageComplete {
            version: version.to_string(),
        };
        if let Err(e) = emit_app.emit("update-stage-complete", payload) {
            log::warn!("{TAG} stage_deferred_update: update-stage-complete emit failed - {e}");
        }
    });
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
    use std::sync::Arc;

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
    /// Issue #484: `tauri.conf.json` must keep `allowDowngrades: false`
    /// so a stale staged update can never silently downgrade the app --
    /// the `is_stale_version` skip is the only downgrade path (explicit
    /// force through the quit-time confirmation surface).
    #[test]
    fn test_tauri_conf_disallows_downgrades() {
        let conf = include_str!("../tauri.conf.json");
        let value: serde_json::Value =
            serde_json::from_str(conf).expect("tauri.conf.json must parse");
        let flag = value.pointer("/bundle/windows/allowDowngrades");
        assert_eq!(
            flag,
            Some(&serde_json::Value::Bool(false)),
            "allowDowngrades must be false (issue #484)"
        );
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

    // -----------------------------------------------------------------
    // Staging progress throttle (issue #590)
    // -----------------------------------------------------------------

    /// The first chunk must always emit: until it does, the banner has no
    /// idea the download started and sits on "Preparing…".
    #[test]
    fn test_stage_progress_emits_first_chunk() {
        let mut throttle = StageProgressThrottle::new();
        let now = Instant::now();
        assert_eq!(
            throttle.observe(now, 4096, Some(1_048_576)),
            Some(StageProgress {
                downloaded: 4096,
                total: Some(1_048_576),
            }),
            "the first chunk must emit so the UI leaves the preparing state"
        );
    }

    /// A burst of chunks inside the interval, with no percentage movement,
    /// must be coalesced — one event per chunk would flood the webview.
    #[test]
    fn test_stage_progress_throttles_within_interval() {
        let mut throttle = StageProgressThrottle::new();
        let start = Instant::now();
        let total = 1_000_000u64;
        assert!(throttle.observe(start, 1_000, Some(total)).is_some());
        assert_eq!(
            throttle.observe(start + Duration::from_millis(10), 1_200, Some(total)),
            None,
            "a chunk inside the interval with <5% movement must be coalesced"
        );
        assert_eq!(
            throttle.observe(start + Duration::from_millis(249), 1_300, Some(total)),
            None,
            "still inside the interval"
        );
    }

    /// Progress must be visible on a fast link too: a whole-percent jump of
    /// at least the step emits immediately, without waiting out the interval.
    #[test]
    fn test_stage_progress_emits_on_percent_step() {
        let mut throttle = StageProgressThrottle::new();
        let start = Instant::now();
        let total = 100_000u64;
        assert!(throttle.observe(start, 1_000, Some(total)).is_some());
        assert_eq!(
            throttle.observe(start + Duration::from_millis(5), 6_000, Some(total)),
            Some(StageProgress {
                downloaded: 6_000,
                total: Some(total),
            }),
            "a 5-point jump must emit even inside the interval"
        );
        assert_eq!(
            throttle.observe(start + Duration::from_millis(10), 9_000, Some(total)),
            None,
            "a 3-point jump inside the interval is still coalesced"
        );
    }

    /// The interval alone must eventually let an event through, and a
    /// missing `Content-Length` must fall back to it (no percentage basis).
    #[test]
    fn test_stage_progress_emits_after_interval_without_total() {
        let mut throttle = StageProgressThrottle::new();
        let start = Instant::now();
        assert!(
            throttle.observe(start, 1_000, None).is_some(),
            "the first chunk emits even with no Content-Length"
        );
        assert_eq!(
            throttle.observe(start + Duration::from_millis(100), 2_000, None),
            None,
            "no total means no percentage step to trip the throttle"
        );
        assert_eq!(
            throttle.observe(start + STAGE_PROGRESS_MIN_INTERVAL, 3_000, None),
            Some(StageProgress {
                downloaded: 3_000,
                total: None,
            }),
            "the interval must release the next event"
        );
    }

    /// Percentage arithmetic guards the divide-by-zero (and the overflow a
    /// large payload could cause) rather than panicking mid-download.
    #[test]
    fn test_percent_of_guards_zero_total_and_overflow() {
        assert_eq!(percent_of(0, Some(0)), 0, "a zero total has no percentage");
        assert_eq!(percent_of(5, Some(10)), 50);
        assert_eq!(
            percent_of(u64::MAX, Some(1)),
            100,
            "overshoot clamps to 100"
        );
        assert_eq!(
            percent_of(u64::MAX, Some(u64::MAX)),
            1,
            "a saturating multiply keeps huge payloads from overflowing"
        );
    }

    /// Issue #678: the endpoint lists and their order are the contract the
    /// beta fall-through rests on — the stable manifest must stay last, and
    /// the beta URL must be the stable URL's `latest-beta.json` sibling.
    #[test]
    fn test_update_endpoints_lists_and_order() {
        assert_eq!(
            update_endpoints(UpdateChannel::Stable),
            vec![STABLE_ENDPOINT.to_string()]
        );
        assert_eq!(
            update_endpoints(UpdateChannel::Beta),
            vec![BETA_ENDPOINT.to_string(), STABLE_ENDPOINT.to_string()],
            "beta is tried first and the stable manifest stays the fallback"
        );
        assert_eq!(
            BETA_ENDPOINT,
            STABLE_ENDPOINT.replace("latest.json", "latest-beta.json"),
            "the beta manifest is the same release path with the beta file name"
        );
        assert_eq!(
            endpoint_urls(UpdateChannel::Beta).map(|urls| urls.len()),
            Ok(2),
            "every advertised endpoint must be a parseable URL"
        );
    }

    /// Issue #678: a failing endpoint must not abort the check — that is the
    /// entire point of listing the stable manifest after the (unpublished)
    /// beta one.
    #[test]
    fn test_check_walks_past_a_failing_endpoint() {
        let urls = vec![
            Url::parse("https://example.invalid/latest-beta.json").unwrap(),
            Url::parse("https://example.invalid/latest.json").unwrap(),
        ];
        let tried = Arc::new(Mutex::new(Vec::new()));
        let seen = tried.clone();
        let found = tauri::async_runtime::block_on(walk_endpoints(&urls, move |url| {
            let seen = seen.clone();
            async move {
                // What the real repo answers today: no beta manifest.
                let beta = url.path().ends_with("latest-beta.json");
                seen.lock().push(url.to_string());
                if beta {
                    Err("HTTP 404".to_string())
                } else {
                    Ok(Some("4.6.0".to_string()))
                }
            }
        }));
        assert_eq!(found, Ok(Some("4.6.0".to_string())));
        assert_eq!(
            tried.lock().as_slice(),
            [
                "https://example.invalid/latest-beta.json",
                "https://example.invalid/latest.json"
            ],
            "the fall-through must try the endpoints in list order"
        );
    }

    /// The walk stops at the first endpoint that answers, so a published beta
    /// release costs no second round-trip to the stable manifest.
    #[test]
    fn test_check_stops_at_the_first_answering_endpoint() {
        let urls = vec![
            Url::parse("https://example.invalid/latest-beta.json").unwrap(),
            Url::parse("https://example.invalid/latest.json").unwrap(),
        ];
        let attempts = Arc::new(Mutex::new(0usize));
        let seen = attempts.clone();
        let found = tauri::async_runtime::block_on(walk_endpoints(&urls, move |_url| {
            let seen = seen.clone();
            async move {
                *seen.lock() += 1;
                Ok(Some("4.7.0-beta.1".to_string()))
            }
        }));
        assert_eq!(found, Ok(Some("4.7.0-beta.1".to_string())));
        assert_eq!(
            *attempts.lock(),
            1,
            "no fall-through when the first endpoint answers"
        );
    }

    /// When every endpoint fails, the LAST failure is reported (mirroring the
    /// plugin's `last_error` semantics) instead of a swallowed `Ok(None)`,
    /// which the banner would render as "already current".
    #[test]
    fn test_check_reports_the_last_error_when_no_endpoint_answers() {
        let urls = vec![
            Url::parse("https://example.invalid/latest-beta.json").unwrap(),
            Url::parse("https://example.invalid/latest.json").unwrap(),
        ];
        let found: Result<Option<String>, String> =
            tauri::async_runtime::block_on(walk_endpoints(&urls, |url| {
                let msg = format!("{} unreachable", url.path());
                async move { Err(msg) }
            }));
        assert_eq!(found, Err("/latest.json unreachable".to_string()));
    }

    /// Issue #678: the payload S7's `update_staged` notification is built
    /// from, and the rule that a stage which staged nothing (already current,
    /// or declined as stale) must not announce itself as staged.
    #[test]
    fn test_stage_complete_fires_only_for_a_staged_version() {
        let mut fired: Vec<String> = Vec::new();
        emit_stage_complete(Some("4.7.0"), |version| fired.push(version.to_string()));
        assert_eq!(fired, ["4.7.0"]);

        let mut quiet: Vec<String> = Vec::new();
        emit_stage_complete(None, |version| quiet.push(version.to_string()));
        assert!(
            quiet.is_empty(),
            "an outcome with no staged version must not emit update-stage-complete"
        );
    }

    /// The wire shape the always-mounted consumer switches on.
    #[test]
    fn test_stage_complete_payload_shape() {
        assert_eq!(
            serde_json::to_string(&StageComplete {
                version: "4.7.0".to_string()
            })
            .unwrap(),
            r#"{"version":"4.7.0"}"#
        );
    }

    /// The banner shows the manifest's own `pub_date` literal. `Update::date`
    /// rendered through `time`'s `Display` is not RFC 3339 (it is
    /// `SmartDisplay`'s human form), and a manifest that omits the key must
    /// report `None` rather than invent a date.
    #[test]
    fn test_manifest_pub_date_reads_the_manifest_literal() {
        assert_eq!(
            manifest_pub_date(&serde_json::json!({
                "version": "4.7.0",
                "pub_date": "2026-09-16T21:07:35Z",
            })),
            Some("2026-09-16T21:07:35Z".to_string())
        );
        assert_eq!(
            manifest_pub_date(&serde_json::json!({ "version": "4.7.0" })),
            None
        );
        assert_eq!(
            manifest_pub_date(&serde_json::json!({ "pub_date": 42 })),
            None
        );
    }
}
