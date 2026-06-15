//! Token persistence with atomic temp-file + rename writes.
//!
//! Background: `tauri-plugin-store` writes the store JSON in-place; a crash
//! during a write can leave a half-written file that fails to deserialize
//! and bounces the user back to Onboarding. Issue #65 mandates atomic
//! writes for the tokens file. We bypass `tauri-plugin-store` for the
//! tokens file and write a small `{ spotify_tokens, teams_tokens }` JSON
//! directly to `<app-config-dir>/PresenceJam/tokens.json` using the same
//! temp-file + rename pattern as `config::save_config`.
//!
//! The pending-auth blobs (PKCE verifier, device code) are intentionally
//! NOT written here. They live in `AppState` only — a 10–15 min bearer
//! credential is not worth a crash-recovery story that leaks the secret
//! to disk. See issue #65 / HIGH #3 in the security review.

use crate::spotify::SpotifyTokens;
use crate::teams::TeamsTokens;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Manager;

/// Shape of `tokens.json` on disk.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokensFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spotify_tokens: Option<SpotifyTokens>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub teams_tokens: Option<TeamsTokens>,
}

/// Resolve the path to `tokens.json` under the app's config dir.
///
/// We use `app_config_dir` (not `app_data_dir`) so the file is co-located
/// with `config.json` under `dirs::config_dir()/PresenceJam/` — same dir
/// as `config::config_dir()`. This keeps user-visible backup/restore
/// instructions simple: one folder, two files.
pub fn tokens_file_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("Failed to get app config dir: {}", e))?;
    // Mirror config.rs::config_dir() so the file lives in the same
    // `<base>/PresenceJam/` folder as `config.json`.
    let dir = base.join("PresenceJam");
    if !dir.exists() {
        fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create tokens dir '{}': {}", dir.display(), e))?;
    }
    Ok(dir.join("tokens.json"))
}

/// Read tokens from disk. Returns a default `TokensFile` if the file
/// does not exist or is empty. Returns `Err(...)` if the file exists
/// and is non-empty but cannot be deserialised; the caller in `lib::run`
/// setup logs the error and continues with default state, matching the
/// previous `tauri-plugin-store` path's behaviour.
pub fn read_tokens_at(app: &tauri::AppHandle) -> Result<TokensFile, String> {
    let path = tokens_file_path(app)?;
    if !path.exists() {
        log::info!("[TOKEN_IO] read_tokens_at: no file at {}, returning default", path.display());
        return Ok(TokensFile::default());
    }
    let s = fs::read_to_string(&path).map_err(|e| {
        format!("Failed to read tokens file '{}': {}", path.display(), e)
    })?;
    if s.trim().is_empty() {
        log::info!("[TOKEN_IO] read_tokens_at: file is empty, returning default");
        return Ok(TokensFile::default());
    }
    match serde_json::from_str::<TokensFile>(&s) {
        Ok(tf) => {
            log::info!(
                "[TOKEN_IO] read_tokens_at: loaded - has_spotify={}, has_teams={}",
                tf.spotify_tokens.is_some(),
                tf.teams_tokens.is_some()
            );
            Ok(tf)
        }
        Err(e) => {
            // Surface a structured error. The caller (lib::run setup) is
            // expected to log and continue with default state — same
            // behaviour as the previous tauri-plugin-store path.
            Err(format!(
                "Failed to parse tokens file '{}': {}",
                path.display(),
                e
            ))
        }
    }
}

/// Atomic write: serialize to a temp file in the same directory, fsync,
/// then rename onto the target. The rename is atomic on POSIX (and on
/// Windows for same-volume renames), so a process kill mid-write cannot
/// leave a half-written file. The pattern mirrors `config::save_config`.
pub fn write_tokens_atomic(path: &PathBuf, contents: &TokensFile) -> Result<(), String> {
    let dir = path.parent().ok_or_else(|| {
        format!("tokens path '{}' has no parent dir", path.display())
    })?;
    if !dir.exists() {
        fs::create_dir_all(dir)
            .map_err(|e| format!("Failed to create dir '{}': {}", dir.display(), e))?;
    }
    let json = serde_json::to_string_pretty(contents)
        .map_err(|e| format!("Failed to serialize tokens: {}", e))?;

    let temp_path = path.with_extension("json.tmp");
    {
        let mut f = fs::File::create(&temp_path).map_err(|e| {
            format!(
                "Failed to create temp tokens file '{}': {}",
                temp_path.display(),
                e
            )
        })?;
        f.write_all(json.as_bytes())
            .map_err(|e| format!("Failed to write temp tokens file: {}", e))?;
        f.sync_all()
            .map_err(|e| format!("Failed to fsync temp tokens file: {}", e))?;
    }
    fs::rename(&temp_path, path).map_err(|e| {
        format!(
            "Failed to rename '{}' to '{}': {}",
            temp_path.display(),
            path.display(),
            e
        )
    })?;
    log::info!(
        "[TOKEN_IO] write_tokens_atomic: wrote {} bytes atomically to {}",
        json.len(),
        path.display()
    );
    Ok(())
}

/// Persist the current in-memory token state from `AppState` to disk
/// atomically. This is the single entry point that should be used after
/// any token change. It reads the in-memory state (which is the source
/// of truth) and writes the whole file atomically.
///
/// If a `save_*_tokens` call returns Ok, the in-memory state was just
/// updated; the next call (or a follow-up persist) flushes to disk.
pub fn persist_tokens(state: &Arc<crate::AppState>, app: &tauri::AppHandle) -> Result<(), String> {
    let path = tokens_file_path(app)?;
    let spotify_tokens = state.spotify_tokens.read().clone();
    let teams_tokens = state.teams_tokens.read().clone();
    let contents = TokensFile {
        spotify_tokens,
        teams_tokens,
    };
    write_tokens_atomic(&path, &contents)
}

/// Delete the tokens file. Reserved for future reconnect flows that need
/// to fully wipe persisted credentials. Currently unused — re-introduce
/// when reconnect_spotify / reconnect_teams need to clear state without
/// going through the empty-TokensFile write path.
#[allow(dead_code)]
pub fn clear_tokens_file(app: &tauri::AppHandle) -> Result<(), String> {
    let path = tokens_file_path(app)?;
    if path.exists() {
        fs::remove_file(&path)
            .map_err(|e| format!("Failed to delete tokens file '{}': {}", path.display(), e))?;
        log::info!("[TOKEN_IO] clear_tokens_file: deleted {}", path.display());
    } else {
        log::info!("[TOKEN_IO] clear_tokens_file: nothing to delete at {}", path.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn sample_spotify() -> SpotifyTokens {
        SpotifyTokens {
            access_token: "at".to_string(),
            refresh_token: "rt".to_string(),
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(3600),
        }
    }

    fn sample_teams() -> TeamsTokens {
        TeamsTokens {
            access_token: "tat".to_string(),
            refresh_token: Some("trt".to_string()),
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(3600),
        }
    }

    fn tmp_path(name: &str) -> PathBuf {
        let mut p = env::temp_dir();
        p.push(format!("presencejam-test-{}-{}", std::process::id(), name));
        p
    }

    fn read_tokens_inner(path: &std::path::Path) -> Result<TokensFile, String> {
        if !path.exists() {
            return Ok(TokensFile::default());
        }
        let s = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read tokens file '{}': {}", path.display(), e))?;
        if s.trim().is_empty() {
            return Ok(TokensFile::default());
        }
        serde_json::from_str::<TokensFile>(&s)
            .map_err(|e| format!("Failed to parse tokens file: {}", e))
    }

    #[test]
    fn roundtrip_empty() {
        let path = tmp_path("empty.json");
        let _ = fs::remove_file(&path);
        // read on missing file → default
        let tf = read_tokens_inner(&path).unwrap_or_default();
        assert!(tf.spotify_tokens.is_none());
        assert!(tf.teams_tokens.is_none());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn roundtrip_with_tokens() {
        let path = tmp_path("with.json");
        let _ = fs::remove_file(&path);
        let tf = TokensFile {
            spotify_tokens: Some(sample_spotify()),
            teams_tokens: Some(sample_teams()),
        };
        write_tokens_atomic(&path, &tf).unwrap();
        let loaded = read_tokens_inner(&path).unwrap();
        assert!(loaded.spotify_tokens.is_some());
        assert!(loaded.teams_tokens.is_some());
        assert_eq!(loaded.spotify_tokens.unwrap().access_token, "at");
        assert_eq!(loaded.teams_tokens.unwrap().access_token, "tat");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn empty_file_returns_default() {
        let path = tmp_path("zero.json");
        let _ = fs::remove_file(&path);
        fs::write(&path, "").unwrap();
        let loaded = read_tokens_inner(&path).unwrap();
        assert!(loaded.spotify_tokens.is_none());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn corrupt_file_returns_error() {
        let path = tmp_path("bad.json");
        let _ = fs::remove_file(&path);
        fs::write(&path, "{not valid json").unwrap();
        assert!(read_tokens_inner(&path).is_err());
        let _ = fs::remove_file(&path);
    }
}
