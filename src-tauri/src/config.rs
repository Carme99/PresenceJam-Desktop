use crate::profanity;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    #[serde(default = "default_scopes")]
    pub scopes: Vec<String>,
}

fn default_redirect_uri() -> String {
    "presencejam://callback".to_string()
}

fn default_scopes() -> Vec<String> {
    vec![
        "user-read-currently-playing".to_string(),
        "user-read-playback-state".to_string(),
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    5
}

fn default_max_interval_seconds() -> u64 {
    60
}

fn default_expiry_buffer_seconds() -> u64 {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
            scopes: default_scopes(),
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

    let mut file = fs::File::open(&path)
        .map_err(|e| format!("Failed to open config file '{}': {}", path.display(), e))?;

    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .map_err(|e| format!("Failed to read config file '{}': {}", path.display(), e))?;

    let config: AppConfig = serde_json::from_str(&contents)
        .map_err(|e| format!("Failed to parse config file '{}': {}", path.display(), e))?;

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
/// One-shot startup migration:
/// `spotify.client_secret` field (legacy from ≤ v2.5.0), write it to
/// the OS keychain and strip the plaintext from the file. Idempotent
/// and safe to call on every startup.
///
/// Conflict policy: if the keychain already holds a *different*
/// secret, the migration is a no-op (we don't clobber a working
/// keychain entry with another install's plaintext). The user can
/// resolve the conflict via Settings → Reconnect Spotify. See audit
/// Q3 and issue #9.
pub fn migrate_legacy_client_secret() {
    let path = match get_config_path() {
        Ok(p) => p,
        Err(e) => {
            log::warn!(
                "[CFG] migrate_legacy_client_secret: config path unavailable: {}",
                e
            );
            return;
        }
    };
    if !path.exists() {
        return; // Fresh install — nothing to migrate.
    }
    let contents = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("[CFG] migrate_legacy_client_secret: read failed: {}", e);
            return;
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
            return;
        }
    };
    let plaintext = root
        .get("spotify")
        .and_then(|s| s.get("client_secret"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let plaintext = match plaintext {
        Some(s) if !s.is_empty() => s,
        _ => {
            log::debug!("[CFG] migrate_legacy_client_secret: no legacy plaintext field");
            return;
        }
    };
    // Conflict check: if keychain already holds a *different* secret,
    // don't clobber it. Leave the plaintext in place; the user can
    // resolve via Settings → Reconnect Spotify.
    match crate::keychain::get_spotify_client_secret() {
        Ok(existing) if existing == plaintext => {
            log::info!(
                "[CFG] migrate_legacy_client_secret: keychain already holds this value, stripping plaintext only"
            );
        }
        Ok(existing) => {
            log::warn!(
                "[CFG] migrate_legacy_client_secret: keychain holds a different secret; leaving config.json untouched (user should Reconnect)"
            );
            log::warn!(
                "[CFG] migrate_legacy_client_secret: plaintext.len={}, keychain.len={}",
                plaintext.len(),
                existing.len()
            );
            return;
        }
        Err(_) => {
            // Keychain empty (the typical pre-v2.6.0-upgrader case).
            // Write the plaintext into the keychain, then strip the file.
            log::info!("[CFG] migrate_legacy_client_secret: keychain empty, writing plaintext into keychain");
            if let Err(e) = crate::keychain::store_spotify_client_secret(&plaintext) {
                log::warn!(
                    "[CFG] migrate_legacy_client_secret: keychain write failed: {} (plaintext left in config.json)",
                    e
                );
                return;
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
            return;
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
}

fn atomic_write_json(path: &std::path::Path, json: &str) -> Result<(), String> {
    let temp_path = path.with_extension("tmp");

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

    if path.exists() {
        std::fs::remove_file(path)
            .map_err(|e| format!("Failed to remove existing file '{}': {}", path.display(), e))?;
    }

    std::fs::rename(&temp_path, path)
        .map_err(|e| format!("Failed to rename temp file to '{}': {}", path.display(), e))?;

    Ok(())
}

pub fn save_config(config: &AppConfig) -> Result<(), String> {
    let path = get_config_path()?;

    let json = serde_json::to_string_pretty(config)
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
        assert_eq!(config.polling.default_interval_seconds, 30);
        assert!(config.logging.enabled);
    }

    #[test]
    fn test_config_dir_creation() {
        let dir = config_dir().expect("config_dir should return valid path");
        assert!(dir.to_str().unwrap().ends_with("PresenceJam"));
    }
}
