use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpotifyConfig {
    pub client_id: String,
    pub client_secret: String,
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
}

fn default_status_format() -> String {
    "🎵 {artist} - {track} 🎧".to_string()
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
    10
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
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
}

fn default_logging_enabled() -> bool {
    true
}

fn default_log_level() -> String {
    "Info".to_string()
}

fn default_retention_days() -> u32 {
    30
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
}

impl Default for SpotifyConfig {
    fn default() -> Self {
        Self {
            client_id: String::new(),
            client_secret: String::new(),
            redirect_uri: default_redirect_uri(),
            scopes: default_scopes(),
        }
    }
}

impl Default for TeamsConfig {
    fn default() -> Self {
        Self {
            status_format: default_status_format(),
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
            retention_days: default_retention_days(),
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            spotify: SpotifyConfig::default(),
            teams: TeamsConfig::default(),
            polling: PollingConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
}

pub fn config_dir() -> Result<PathBuf, String> {
    let base_dir = dirs::config_dir()
        .ok_or_else(|| "Failed to get config directory: dirs::config_dir() returned None".to_string())?;
    
    let app_dir = base_dir.join("PresenceJam");
    
    if !app_dir.exists() {
        fs::create_dir_all(&app_dir)
            .map_err(|e| format!("Failed to create config directory '{}': {}", app_dir.display(), e))?;
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
        log::info!("Config file not found at '{}', using defaults", path.display());
        return Ok(AppConfig::default());
    }
    
    let mut file = fs::File::open(&path)
        .map_err(|e| format!("Failed to open config file '{}': {}", path.display(), e))?;
    
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .map_err(|e| format!("Failed to read config file '{}': {}", path.display(), e))?;
    
    let config: AppConfig = serde_json::from_str(&contents)
        .map_err(|e| format!("Failed to parse config file '{}': {}", path.display(), e))?;
    
    log::info!("Loaded configuration from '{}'", path.display());
    Ok(config)
}

pub fn save_config(config: &AppConfig) -> Result<(), String> {
    let path = get_config_path()?;
    
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config to JSON: {}", e))?;
    
    let mut file = fs::File::create(&path)
        .map_err(|e| format!("Failed to create config file '{}': {}", path.display(), e))?;
    
    file.write_all(json.as_bytes())
        .map_err(|e| format!("Failed to write config file '{}': {}", path.display(), e))?;
    
    log::info!("Saved configuration to '{}'", path.display());
    Ok(())
}

pub fn load_credentials() -> Result<Credentials, String> {
    let dir = config_dir()?;
    let path = dir.join("credentials.json");
    
    if !path.exists() {
        return Err("Credentials file not found".to_string());
    }
    
    let mut file = fs::File::open(&path)
        .map_err(|e| format!("Failed to open credentials file '{}': {}", path.display(), e))?;
    
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .map_err(|e| format!("Failed to read credentials file '{}': {}", path.display(), e))?;
    
    let credentials: Credentials = serde_json::from_str(&contents)
        .map_err(|e| format!("Failed to parse credentials file '{}': {}", path.display(), e))?;
    
    log::info!("Loaded credentials from '{}'", path.display());
    Ok(credentials)
}

pub fn save_credentials(credentials: &Credentials) -> Result<(), String> {
    let dir = config_dir()?;
    let path = dir.join("credentials.json");
    
    let json = serde_json::to_string_pretty(credentials)
        .map_err(|e| format!("Failed to serialize credentials to JSON: {}", e))?;
    
    let mut file = fs::File::create(&path)
        .map_err(|e| format!("Failed to create credentials file '{}': {}", path.display(), e))?;
    
    file.write_all(json.as_bytes())
        .map_err(|e| format!("Failed to write credentials file '{}': {}", path.display(), e))?;
    
    log::info!("Saved credentials to '{}'", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.spotify.redirect_uri, "http://localhost:7890/callback");
        assert_eq!(config.teams.status_format, "🎵 {artist} - {track} 🎧");
        assert_eq!(config.polling.default_interval_seconds, 30);
        assert!(config.logging.enabled);
    }

    #[test]
    fn test_config_dir_creation() {
        let dir = config_dir().expect("config_dir should return valid path");
        assert!(dir.to_str().unwrap().ends_with("PresenceJam"));
    }
}
