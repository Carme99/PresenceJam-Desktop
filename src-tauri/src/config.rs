use crate::profanity;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;
use tauri::Emitter;

#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct SpotifyConfig {
    pub client_id: String,
    /// True iff the Spotify `client_secret` is currently stored in the OS
    /// keychain. This is a derived/display field — it is populated by
    /// `load_config` (and not persisted to disk). The actual secret lives
    /// in the keychain, not in `config.json`. See issue #9.
    #[serde(default)]
    pub client_secret_set: bool,
    #[serde(default = "default_redirect_uri")]
    pub redirect_uri: String,
}

fn default_redirect_uri() -> String {
    "presencejam://callback".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct TeamsConfig {
    #[serde(default = "default_status_format")]
    pub status_format: String,
    #[serde(default = "default_clear_on_pause")]
    pub clear_on_pause: bool,
    #[serde(default = "default_profanity_filter")]
    pub profanity_filter: bool,
    #[serde(default = "default_profanity_placeholder")]
    pub profanity_placeholder: String,
    #[serde(default)]
    pub start_minimized: bool,
    /// P1 (issue #3.0-P1): drive the Teams presence bubble
    /// (Available/Available while a track plays) via Graph
    /// setPresence/clearPresence. OFF by default — it overrides the
    /// user's manual presence bubble.
    #[serde(default = "default_availability_sync")]
    pub availability_sync: bool,
    /// P2 (issue #3.0-P2): before writing a status message, read the
    /// user's presence and skip the write when busy/DND/in a
    /// meeting/in a call/presenting. ON by default.
    #[serde(default = "default_presence_gate")]
    pub presence_gate: bool,
}

fn default_status_format() -> String {
    "🎵 {artist} - {track} 🎧".to_string()
}

fn default_start_minimized() -> bool {
    false
}

fn default_clear_on_pause() -> bool {
    true
}

fn default_profanity_filter() -> bool {
    true
}

fn default_profanity_placeholder() -> String {
    profanity::safe_placeholder_default().to_string()
}

fn default_availability_sync() -> bool {
    false
}

fn default_presence_gate() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct PollingConfig {
    #[serde(default = "default_interval_seconds")]
    pub default_interval_seconds: u64,
    #[serde(default = "default_min_interval_seconds")]
    pub minimum_interval_seconds: u64,
    #[serde(default = "default_max_interval_seconds")]
    pub max_interval_seconds: u64,
    #[serde(default = "default_expiry_buffer_seconds")]
    pub expiry_buffer_seconds: u64,
}

fn default_interval_seconds() -> u64 {
    30
}

fn default_min_interval_seconds() -> u64 {
    10
}

fn default_max_interval_seconds() -> u64 {
    60
}

fn default_expiry_buffer_seconds() -> u64 {
    10
}
fn clamp_polling(cfg: &mut PollingConfig) {
    cfg.default_interval_seconds = cfg.default_interval_seconds.clamp(5, 300);
    cfg.minimum_interval_seconds = cfg.minimum_interval_seconds.clamp(5, 30);
    cfg.max_interval_seconds = cfg
        .max_interval_seconds
        .clamp(cfg.minimum_interval_seconds, 300);
    cfg.expiry_buffer_seconds = cfg.expiry_buffer_seconds.clamp(0, 60);
}

#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct LoggingConfig {
    #[serde(default = "default_logging_enabled")]
    pub enabled: bool,
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_logging_enabled() -> bool {
    true
}

fn default_log_level() -> String {
    "Info".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct AppConfig {
    #[serde(default)]
    pub spotify: SpotifyConfig,
    #[serde(default)]
    pub teams: TeamsConfig,
    #[serde(default)]
    pub polling: PollingConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub autostart: bool,
}

impl Default for SpotifyConfig {
    fn default() -> Self {
        Self {
            client_id: String::new(),
            client_secret_set: false,
            redirect_uri: default_redirect_uri(),
        }
    }
}

impl Default for TeamsConfig {
    fn default() -> Self {
        Self {
            status_format: default_status_format(),
            clear_on_pause: default_clear_on_pause(),
            profanity_filter: default_profanity_filter(),
            profanity_placeholder: default_profanity_placeholder(),
            start_minimized: default_start_minimized(),
            availability_sync: default_availability_sync(),
            presence_gate: default_presence_gate(),
        }
    }
}

impl Default for PollingConfig {
    fn default() -> Self {
        Self {
            default_interval_seconds: default_interval_seconds(),
            minimum_interval_seconds: default_min_interval_seconds(),
            max_interval_seconds: default_max_interval_seconds(),
            expiry_buffer_seconds: default_expiry_buffer_seconds(),
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            enabled: default_logging_enabled(),
            log_level: default_log_level(),
        }
    }
}

// `Default::default()` can't be derived because `LoggingConfig` uses
// `default_*()` helper functions to seed its fields with non-`Default`
// values (a default log level, a default "enabled" flag). The helper
// calls are intentional, not a candidate for `#[derive(Default)]`.
#[allow(clippy::derivable_impls)]
impl Default for AppConfig {
    fn default() -> Self {
        Self {
            spotify: SpotifyConfig::default(),
            teams: TeamsConfig::default(),
            polling: PollingConfig::default(),
            logging: LoggingConfig::default(),
            autostart: false,
        }
    }
}

pub fn config_dir() -> Result<PathBuf, String> {
    let base_dir = dirs::config_dir().ok_or_else(|| {
        "Failed to get config directory: dirs::config_dir() returned None".to_string()
    })?;

    let app_dir = base_dir.join("PresenceJam");

    if !app_dir.exists() {
        fs::create_dir_all(&app_dir).map_err(|e| {
            format!(
                "Failed to create config directory '{}': {}",
                app_dir.display(),
                e
            )
        })?;
        #[cfg(unix)]
        {
            let _ = fs::set_permissions(&app_dir, std::fs::Permissions::from_mode(0o700));
        }
        log::info!("Created config directory at '{}'", app_dir.display());
    }

    Ok(app_dir)
}

pub fn get_config_path() -> Result<PathBuf, String> {
    let dir = config_dir()?;
    Ok(dir.join("config.json"))
}

pub fn load_config() -> Result<AppConfig, String> {
    let path = get_config_path()?;

    if !path.exists() {
        log::info!(
            "Config file not found at '{}', using defaults",
            path.display()
        );
        return Ok(with_keychain_flags(AppConfig::default()));
    }

    // Issue #135 path A: tighten mode of any pre-existing config.json that
    // was created loose by an older PresenceJam version (default umask 022
    // → 0644). Idempotent on a file that is already 0600. Windows default
    // ACL is user-only, so this is a no-op there.
    #[cfg(unix)]
    {
        let current = fs::metadata(&path)
            .map_err(|e| format!("Failed to stat config file '{}': {}", path.display(), e))?
            .permissions();
        let current_mode = current.mode() & 0o777;
        if current_mode != 0o600 {
            log::warn!(
                "Tightening config.json mode from {:o} to 0600 (issue #135)",
                current_mode
            );
            let mut tightened = current;
            tightened.set_mode(0o600);
            fs::set_permissions(&path, tightened).map_err(|e| {
                format!(
                    "Failed to chmod config file '{}' to 0600: {}",
                    path.display(),
                    e
                )
            })?;
        }
    }

    let mut file = fs::File::open(&path)
        .map_err(|e| format!("Failed to open config file '{}': {}", path.display(), e))?;

    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .map_err(|e| format!("Failed to read config file '{}': {}", path.display(), e))?;

    let mut config: AppConfig = serde_json::from_str(&contents)
        .map_err(|e| format!("Failed to parse config file '{}': {}", path.display(), e))?;
    clamp_polling(&mut config.polling);

    log::info!("Loaded configuration from '{}'", path.display());
    Ok(with_keychain_flags(config))
}

/// Populate derived/display fields that are not persisted to disk.
///
/// Currently this only covers `spotify.client_secret_set`, which reflects
/// whether the OS keychain holds a Spotify client secret. See issue #9.
fn with_keychain_flags(mut config: AppConfig) -> AppConfig {
    config.spotify.client_secret_set = crate::keychain::has_spotify_client_secret();
    config
}
/// Frontend event emitted (once per process) when the legacy-plaintext
/// migration finds a *different* secret already in the OS keychain.
///
/// The Settings view should listen for this event and prompt the user to
/// run Settings → Reconnect Spotify. See issue #376.
pub const SPOTIFY_SECRET_CONFLICT_EVENT: &str = "spotify-secret-conflict";
/// One-shot startup migration for the legacy `spotify.client_secret`
/// field (≤ v2.5.0): write it to
/// the OS keychain and strip the plaintext from the file. Idempotent
/// and safe to call on every startup.
///
/// Conflict policy: if the keychain already holds a *different*
/// secret, the migration is a no-op (we don't clobber a working
/// keychain entry with another install's plaintext, and we do NOT delete
/// the plaintext unilaterally — the user may need it). The user resolves
/// the conflict via Settings → Reconnect Spotify. See audit Q3 and
/// issues #9 and #376.
///
/// Bounded notification: the conflict is surfaced exactly once per process
/// (see `migrate_legacy_client_secret_with_app`; a process-wide flag guards
/// the emit) — there is no retry loop or timeout that auto-deletes the
/// plaintext. Manual step: after Reconnect Spotify stores the current
/// secret in the keychain, the next launch either completes the migration
/// (keychain empty / identical value → plaintext stripped) or re-emits
/// this event while the stale plaintext is still present.
/// Log-only variant kept for backward compatibility (no `AppHandle`
/// available at some call sites). Prefer
/// `migrate_legacy_client_secret_with_app`, which additionally surfaces a
/// keychain conflict to the UI via [`SPOTIFY_SECRET_CONFLICT_EVENT`].
pub fn migrate_legacy_client_secret() {
    run_legacy_secret_migration();
}
/// Startup migration with user-visible conflict surfacing (issue #376).
///
/// Runs the same migration as [`migrate_legacy_client_secret`]; when the
/// outcome is [`LegacySecretOutcome::ConflictKeychainDiffers`], emits a
/// one-time [`SPOTIFY_SECRET_CONFLICT_EVENT`] so Settings can prompt
/// Settings → Reconnect Spotify (payload carries the manual step).
/// All other outcomes are silent apart from the usual `[CFG]` logs.
///
/// Wiring note (orchestrator): `lib.rs` setup currently calls the log-only
/// `migrate_legacy_client_secret()`; swap that call site to
/// `config::migrate_legacy_client_secret_with_app(app.handle())` so the
/// conflict becomes user-visible. This file is slice-D owned, so the
/// one-line swap lives outside this change.
pub fn migrate_legacy_client_secret_with_app(app: &tauri::AppHandle) {
    if run_legacy_secret_migration() == LegacySecretOutcome::ConflictKeychainDiffers {
        emit_spotify_secret_conflict_once(app);
    }
}
/// Observable outcome of one [`run_legacy_secret_migration`] pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegacySecretOutcome {
    /// No `spotify.client_secret` plaintext field (or it was empty):
    /// nothing to do. Also returned when the config file is missing,
    /// unreadable, or unparsable, or a keychain write / file rewrite
    /// failed part-way (plaintext left on disk in those cases).
    NoLegacyField,
    /// Plaintext migrated into an empty keychain (or the keychain
    /// already held the identical value) and the strip pass ran.
    Migrated,
    /// Keychain already holds a *different* secret: plaintext deliberately
    /// left on disk. The caller must surface this via
    /// [`SPOTIFY_SECRET_CONFLICT_EVENT`].
    ConflictKeychainDiffers,
}
/// Pure decision step of the migration: given the legacy plaintext (if any)
/// and the current keychain read, decide the outcome without touching disk
/// or the keychain. Unit-tested directly (issue #376).
fn decide_legacy_secret_outcome(
    plaintext: Option<&str>,
    keychain: &Result<String, String>,
) -> LegacySecretOutcome {
    let plaintext = match plaintext {
        Some(s) if !s.is_empty() => s,
        _ => return LegacySecretOutcome::NoLegacyField,
    };
    match keychain {
        Ok(existing) if existing == plaintext => LegacySecretOutcome::Migrated,
        Ok(_) => LegacySecretOutcome::ConflictKeychainDiffers,
        Err(_) => LegacySecretOutcome::Migrated,
    }
}
/// Process-wide guard so the conflict event fires at most once per launch,
/// no matter how often the migration entry points are called.
static CONFLICT_EVENT_SENT: AtomicBool = AtomicBool::new(false);
/// Emit [`SPOTIFY_SECRET_CONFLICT_EVENT`] unless already sent this process.
/// Follows the `let _ = app.emit(...)` pattern used in `poll_once.rs`;
/// the payload tells Settings to prompt Reconnect Spotify. Returns true
/// when this call performed the (single) emit.
fn emit_spotify_secret_conflict_once(app: &tauri::AppHandle) -> bool {
    if CONFLICT_EVENT_SENT.swap(true, Ordering::AcqRel) {
        return false;
    }
    log::warn!(
        "[CFG] migrate_legacy_client_secret: EMIT {} event (prompt Settings → Reconnect Spotify)",
        SPOTIFY_SECRET_CONFLICT_EVENT
    );
    let _ = app.emit(
        SPOTIFY_SECRET_CONFLICT_EVENT,
        serde_json::json!({
            "action": "reconnect-spotify",
            "message": "The Spotify client secret in config.json differs from the one in the OS keychain. Open Settings → Reconnect Spotify to resolve. The legacy plaintext is left untouched until then.",
        }),
    );
    true
}
/// Executes the migration IO and returns its observable outcome.
fn run_legacy_secret_migration() -> LegacySecretOutcome {
    let path = match get_config_path() {
        Ok(p) => p,
        Err(e) => {
            log::warn!(
                "[CFG] migrate_legacy_client_secret: config path unavailable: {}",
                e
            );
            return LegacySecretOutcome::NoLegacyField;
        }
    };
    if !path.exists() {
        return LegacySecretOutcome::NoLegacyField; // Fresh install — nothing to migrate.
    }
    let contents = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("[CFG] migrate_legacy_client_secret: read failed: {}", e);
            return LegacySecretOutcome::NoLegacyField;
        }
    };
    // Parse as raw Value so we can inspect unknown / pre-v2.6.0 fields
    // without AppConfig's silent-drop on unknown keys. (AppConfig does
    // not declare `client_secret`, so `serde_json::from_str::<AppConfig>`
    // would discard it before we got a chance to migrate.)
    let mut root: serde_json::Value = match serde_json::from_str(&contents) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("[CFG] migrate_legacy_client_secret: parse failed: {}", e);
            return LegacySecretOutcome::NoLegacyField;
        }
    };
    let plaintext = root
        .get("spotify")
        .and_then(|s| s.get("client_secret"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let keychain_read = crate::keychain::get_spotify_client_secret();
    let outcome = decide_legacy_secret_outcome(plaintext.as_deref(), &keychain_read);
    match (&outcome, &keychain_read) {
        (LegacySecretOutcome::NoLegacyField, _) => {
            log::debug!("[CFG] migrate_legacy_client_secret: no legacy plaintext field");
            return outcome;
        }
        (LegacySecretOutcome::ConflictKeychainDiffers, Ok(existing)) => {
            // Conflict check: if keychain already holds a *different* secret,
            // don't clobber it. Leave the plaintext in place; the user can
            // resolve via Settings → Reconnect Spotify (surfaced via
            // `spotify-secret-conflict`; see `migrate_legacy_client_secret_with_app`).
            let plaintext = plaintext.unwrap_or_default();
            log::warn!(
                "[CFG] migrate_legacy_client_secret: keychain holds a different secret; leaving config.json untouched (user should Reconnect)"
            );
            log::warn!(
                "[CFG] migrate_legacy_client_secret: plaintext.len={}, keychain.len={}",
                plaintext.len(),
                existing.len()
            );
            return outcome;
        }
        _ => {
            // Keychain empty (the typical pre-v2.6.0-upgrader case), or it
            // already holds the identical value (strip-only). Write the
            // plaintext into the keychain only when the keychain is empty.
            if keychain_read.is_err() {
                log::info!("[CFG] migrate_legacy_client_secret: keychain empty, writing plaintext into keychain");
                // `plaintext` is `Some(non-empty)` here: `decide_*` only
                // returns `Migrated` for `Some(non-empty)` input.
                let plaintext = plaintext.unwrap_or_default();
                if let Err(e) = crate::keychain::store_spotify_client_secret(&plaintext) {
                    log::warn!(
                        "[CFG] migrate_legacy_client_secret: keychain write failed: {} (plaintext left in config.json)",
                        e
                    );
                    return LegacySecretOutcome::NoLegacyField;
                }
            } else {
                log::info!(
                    "[CFG] migrate_legacy_client_secret: keychain already holds this value, stripping plaintext only"
                );
            }
        }
    }
    // Strip the plaintext field and re-serialise.
    if let Some(spotify_obj) = root.get_mut("spotify").and_then(|v| v.as_object_mut()) {
        spotify_obj.remove("client_secret");
    }
    let new_contents = match serde_json::to_string_pretty(&root) {
        Ok(s) => s,
        Err(e) => {
            log::warn!(
                "[CFG] migrate_legacy_client_secret: re-serialise failed: {}",
                e
            );
            return LegacySecretOutcome::NoLegacyField;
        }
    };
    if let Err(e) = atomic_write_json(&path, &new_contents) {
        log::warn!(
            "[CFG] migrate_legacy_client_secret: atomic rewrite failed: {} (keychain has the value, plaintext remains on disk)",
            e
        );
    } else {
        log::info!(
            "[CFG] migrate_legacy_client_secret: SUCCESS — plaintext stripped from config.json"
        );
    }
    LegacySecretOutcome::Migrated
}

fn atomic_write_json(path: &std::path::Path, json: &str) -> Result<(), String> {
    let temp_path = path.with_extension("tmp");

    // Issue #135 path A: create the temp file with mode 0600 atomically.
    // Pre-clear any stale sidecar from a previous crash (between temp-write
    // and rename). Without this pre-clear, create_new(true) would error with
    // AlreadyExists on a leftover `.tmp`, turning a one-off crash into a
    // permanent save failure until the user manually deletes the sidecar.
    // Deletion of a non-existent file is fine — we ignore NotFound.
    if let Err(e) = fs::remove_file(&temp_path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            return Err(format!(
                "Failed to remove stale temp file '{}': {}",
                temp_path.display(),
                e
            ));
        }
    }
    // OpenOptions::create_new(true) prevents racing with a leftover sidecar;
    // .mode(0o600) sets the mode at file-creation time (no chmod-after-create
    // window where config.json could briefly sit world-readable). The
    // subsequent rename() preserves the source mode on POSIX. On Windows,
    // the new file inherits the user-only default ACL of the parent.
    #[cfg(unix)]
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp_path)
        .map_err(|e| {
            format!(
                "Failed to create temp file '{}': {}",
                temp_path.display(),
                e
            )
        })?;

    #[cfg(not(unix))]
    let mut file = fs::File::create(&temp_path).map_err(|e| {
        format!(
            "Failed to create temp file '{}': {}",
            temp_path.display(),
            e
        )
    })?;

    file.write_all(json.as_bytes())
        .map_err(|e| format!("Failed to write temp file '{}': {}", temp_path.display(), e))?;

    file.sync_all()
        .map_err(|e| format!("Failed to sync temp file '{}': {}", temp_path.display(), e))?;

    std::fs::rename(&temp_path, path)
        .map_err(|e| format!("Failed to rename temp file to '{}': {}", path.display(), e))?;
    #[cfg(unix)]
    {
        if let Some(parent) = path.parent() {
            if let Ok(dir) = std::fs::File::open(parent) {
                if let Err(e) = dir.sync_all() {
                    log::warn!("Failed to fsync config dir '{}': {}", parent.display(), e);
                }
            }
        }
    }

    Ok(())
}

/// The config as it will actually be persisted: `clamp_polling` applied.
///
/// `save_config` writes a clamped copy, so the caller must store THIS value
/// in `AppState` rather than its own unclamped input — otherwise a value the
/// UI can type (the number inputs' `min`/`max` attributes do not constrain a
/// typed value) lives in memory while a different one sits on disk, and the
/// two silently reconcile only on the next launch. See issue #297.
pub fn clamped_config(config: &AppConfig) -> AppConfig {
    let mut cfg = config.clone();
    clamp_polling(&mut cfg.polling);
    cfg
}

pub fn save_config(config: &AppConfig) -> Result<(), String> {
    let path = get_config_path()?;

    let cfg = clamped_config(config);
    let json = serde_json::to_string_pretty(&cfg)
        .map_err(|e| format!("Failed to serialize config to JSON: {}", e))?;

    atomic_write_json(&path, &json)?;

    log::info!("Saved configuration to '{}'", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.spotify.redirect_uri, "presencejam://callback");
        assert_eq!(config.teams.status_format, "🎵 {artist} - {track} 🎧");
        assert!(config.teams.clear_on_pause);
        assert!(config.teams.profanity_filter);
        assert_eq!(
            config.teams.profanity_placeholder,
            profanity::safe_placeholder_default()
        );
        // Issue #3.0-P1/P2: availability sync OFF, presence gate ON.
        assert!(!config.teams.availability_sync);
        assert!(config.teams.presence_gate);
        assert_eq!(config.polling.default_interval_seconds, 30);
        assert!(config.logging.enabled);
    }

    #[test]
    fn test_config_dir_creation() {
        let dir = config_dir().expect("config_dir should return valid path");
        assert!(dir.to_str().unwrap().ends_with("PresenceJam"));
    }

    /// Regression guard for issue found in PR review: a redundant
    /// `fs::remove_file(path)` before the final `rename` opened a window
    /// where a process crash leaves config.json missing. Drop the
    /// remove_file; rename() atomically replaces the destination on
    /// POSIX + same-volume Windows renames. Mirrors token_io.rs pattern.
    #[test]
    fn test_atomic_write_json_replaces_existing_file() {
        let dir = std::env::temp_dir().join(format!(
            "pj-test-{}-{}",
            std::process::id(),
            chrono_like_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, b"OLD_CONTENTS_AAA").unwrap();

        atomic_write_json(&path, "NEW_CONTENTS_BBB").expect("write should succeed");

        // After atomic_write_json, the destination must hold the new bytes
        // (no mix with the old), and there must be no leftover .tmp sidecar.
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "NEW_CONTENTS_BBB");
        let sidecar = path.with_extension("tmp");
        assert!(
            !sidecar.exists(),
            "temp sidecar {} must be consumed by rename (rename atomicity)",
            sidecar.display()
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Source-level regression guard for the crash-window bug: a redundant
    /// `fs::remove_file(path)` between temp-write-fsync and `rename()`
    /// breaks the rename-atomicity guarantee (rename atomically replaces
    /// the destination on POSIX + Windows same-volume renames, removing
    /// first leaves a crash window where the destination is gone and the
    /// rename never happens).
    ///
    /// Robust anchor: walk a brace count from the first `{` after the
    /// `fn atomic_write_json(...)` signature. The body's `{`/`}` count is
    /// independent of what other functions are declared around it, so this
    /// test survives reordering / splitting / renaming of adjacent code.
    #[test]
    fn test_atomic_write_json_does_not_remove_destination_first() {
        let src = include_str!("config.rs");
        // Find the function signature (the line that starts the body).
        let sig_idx = src
            .find("fn atomic_write_json(")
            .expect("atomic_write_json must exist");
        // Walk forward until the first `{`, then count braces to find the
        // matching `}`. Robust to whatever comes after the function.
        let brace_open_rel = src[sig_idx..]
            .find('{')
            .expect("atomic_write_json body must have an opening brace");
        let body_start = sig_idx + brace_open_rel;
        let mut depth: u32 = 0;
        let mut i = body_start;
        let body_end = loop {
            let ch = src.as_bytes()[i];
            match ch {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        break i;
                    }
                }
                _ => {}
            }
            i += 1;
            if i >= src.len() {
                panic!("atomic_write_json body has unbalanced braces");
            }
        };
        let body = &src[body_start + 1..body_end];
        // Allow `remove_file(&temp_path)` (pre-clearing a stale sidecar from
        // a prior crash, introduced by issue #135 path A) but forbid
        // `remove_file(path)` (removing the destination before rename would
        // break the rename-atomicity guarantee). The latter is the original
        // PR #133 regression. We anchor on the destination-path identifier
        // to be string-literal-safe (the brace counter excludes braces
        // inside string contents only by accident, so we rely on the
        // specific `remove_file(path` token rather than free-form
        // `remove_file`).
        assert!(
            !body.contains("remove_file(path"),
            "atomic_write_json must not call remove_file(path) on the destination \
             before rename — rename atomically replaces the destination on POSIX \
             + Windows same-volume renames, and the explicit remove breaks \
             crash-safety. `remove_file(&temp_path)` on the sidecar IS allowed \
             (issue #135 path A: pre-clear stale sidecar from a prior crash). \
             See ARCHITECTURE.md 'Storage' section and PR #133 for the \
             original regression context."
        );
    }

    fn chrono_like_nanos() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }

    /// Regression guard for issue #135: a stale `.tmp` from a previous crash
    /// must not block the next write. Without the pre-clear, the new
    /// create_new(true) on a leftover sidecar would error with AlreadyExists
    /// and turn a one-off crash into a permanent save failure.
    #[test]
    fn test_atomic_write_json_recovers_from_stale_tmp_sidecar() {
        let dir = std::env::temp_dir().join(format!(
            "pj-test-recover-{}-{}",
            std::process::id(),
            chrono_like_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let sidecar = path.with_extension("tmp");

        // Simulate a previous crash that left the sidecar behind.
        std::fs::write(&sidecar, b"PARTIAL_GARBAGE_FROM_CRASH").unwrap();
        assert!(sidecar.exists(), "sidecar must exist before recovery");

        atomic_write_json(&path, "NEW_CONTENTS_AFTER_CRASH")
            .expect("write must succeed despite stale sidecar");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "NEW_CONTENTS_AFTER_CRASH"
        );
        assert!(
            !sidecar.exists(),
            "sidecar must be consumed by rename (no .tmp leftover)"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Issue #376: the keychain/config conflict branch must resolve to a
    /// dedicated outcome (which the `_with_app` entry point turns into a
    /// user-visible `spotify-secret-conflict` event) — never silently to
    /// `Migrated` (which strips the file) or `NoLegacyField` (which stays
    /// log-only). Case from the issue repro: config plaintext "AAA" vs
    /// keychain "BBB".
    #[test]
    fn test_decide_legacy_secret_conflict_keychain_differs() {
        let outcome = decide_legacy_secret_outcome(
            Some("AAA"),
            &Ok("BBB".to_string()),
        );
        assert_eq!(outcome, LegacySecretOutcome::ConflictKeychainDiffers);
    }
    #[test]
    fn test_decide_legacy_secret_outcome_matrix() {
        // No plaintext at all (fresh install / already migrated).
        assert_eq!(
            decide_legacy_secret_outcome(None, &Err("empty".to_string())),
            LegacySecretOutcome::NoLegacyField
        );
        // Empty-string field is not a secret.
        assert_eq!(
            decide_legacy_secret_outcome(Some(""), &Err("empty".to_string())),
            LegacySecretOutcome::NoLegacyField
        );
        // Empty keychain (typical pre-v2.6.0 upgrader): migrate + strip.
        assert_eq!(
            decide_legacy_secret_outcome(Some("AAA"), &Err("empty".to_string())),
            LegacySecretOutcome::Migrated
        );
        // Keychain already holds the identical value: strip-only.
        assert_eq!(
            decide_legacy_secret_outcome(Some("AAA"), &Ok("AAA".to_string())),
            LegacySecretOutcome::Migrated
        );
        // Same value with surrounding keychain state must not count as a
        // conflict: equality is exact, so near-misses still conflict.
        assert_eq!(
            decide_legacy_secret_outcome(Some("AAA"), &Ok("AAA ".to_string())),
            LegacySecretOutcome::ConflictKeychainDiffers
        );
    }
    /// The frontend listens on the literal event name, so a rename of the
    /// constant silently breaks Settings without a compile error on either
    /// side. Pin the contract string (issue #376).
    #[test]
    fn test_spotify_secret_conflict_event_name_contract() {
        assert_eq!(SPOTIFY_SECRET_CONFLICT_EVENT, "spotify-secret-conflict");
    }
}
