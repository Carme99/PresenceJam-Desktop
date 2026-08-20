//! Token persistence with atomic temp-file + rename writes, encrypted at
//! rest with AES-256-GCM (issue #140).
//!
//! Background: `tauri-plugin-store` writes the store JSON in-place; a crash
//! during a write can leave a half-written file that fails to deserialize
//! and bounces the user back to Onboarding. Issue #65 mandates atomic
//! writes for the tokens file. We bypass `tauri-plugin-store` for the
//! tokens file and write a small `{ spotify_tokens, teams_tokens }`
//! structure directly to `<app-config-dir>/PresenceJam/tokens.json` using
//! the same temp-file + rename pattern as `config::save_config`.
//!
//! Since v3.0 (issue #140) the file is **AES-256-GCM ciphertext**, never
//! plaintext JSON: the 256-bit key is generated on first use and held in
//! the OS keychain under `tokens_aes_key:com.presencejam.app` (see
//! `keychain::get_or_create_tokens_aes_key`). On-disk format:
//!
//! ```text
//! b"PJENC" | version_byte (0x01) | 12-byte random nonce | AES-256-GCM ciphertext
//! ```
//!
//! The magic prefix + version byte let a future cipher change co-exist
//! with the current one: readers dispatch on the version byte, so v3.0
//! files and any successor format can be handled side by side (unknown
//! versions are rejected rather than mis-decrypted).
//!
//! Legacy plaintext JSON files (releases ≤ v2.10.0) are migrated on first
//! read: parsed, then immediately re-written encrypted (see
//! `read_tokens_at`). The pending-auth blobs (PKCE verifier, device code)
//! are intentionally NOT written here. They live in `AppState` only — a
//! 10–15 min bearer credential is not worth a crash-recovery story that
//! leaks the secret to disk. See issue #65 / HIGH #3 in the security
//! review.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use crate::spotify::SpotifyTokens;
use crate::teams::TeamsTokens;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::Manager;

/// Shape of `tokens.json` on disk (the *plaintext* payload — the file
/// itself is AES-256-GCM ciphertext, see the module docs).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokensFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spotify_tokens: Option<SpotifyTokens>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub teams_tokens: Option<TeamsTokens>,
}

/// Magic prefix of the encrypted on-disk format, followed by a single
/// format-version byte, a 12-byte random nonce, and the AES-256-GCM
/// ciphertext. See the module docs for the full layout.
const TOKENS_MAGIC: &[u8; 5] = b"PJENC";

/// Current cipher format version. Version 1 = AES-256-GCM with a 12-byte
/// nonce and a 256-bit key from the `tokens_aes_key:com.presencejam.app`
/// keychain slot. A future cipher change bumps this byte; readers reject
/// unknown versions so old data is never mis-decrypted.
const TOKENS_VERSION: u8 = 1;

/// AES-GCM standard nonce length (96 bits).
const TOKENS_NONCE_LEN: usize = 12;

/// Bytes before the ciphertext: magic (5) + version (1) + nonce (12).
const TOKENS_HEADER_LEN: usize = TOKENS_MAGIC.len() + 1 + TOKENS_NONCE_LEN;

/// Encrypt a serialized-JSON payload for `tokens.json`.
///
/// Layout: `TOKENS_MAGIC || TOKENS_VERSION || 12-byte random nonce ||
/// AES-256-GCM ciphertext`. A fresh CSPRNG nonce is used per write, so
/// two writes of identical content produce different files (no
/// ciphertext-pattern leakage) and the nonce never repeats.
fn encrypt_tokens(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>, String> {
    let mut nonce_bytes = [0u8; TOKENS_NONCE_LEN];
    rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| "AES-256 key must be 32 bytes".to_string())?;
    let ciphertext = cipher
        .encrypt(
            &Nonce::try_from(&nonce_bytes[..]).map_err(|_| "AES-GCM nonce must be 12 bytes".to_string())?,
            plaintext,
        )
        .map_err(|e| format!("AES-GCM encryption failed: {}", e))?;
    let mut out = Vec::with_capacity(TOKENS_HEADER_LEN + ciphertext.len());
    out.extend_from_slice(TOKENS_MAGIC);
    out.push(TOKENS_VERSION);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Decrypt a `tokens.json` byte buffer produced by [`encrypt_tokens`].
///
/// Validates the magic prefix, the format-version byte, and the GCM tag
/// (which authenticates the ciphertext and the nonce). Returns an error
/// for a wrong magic, an unknown version byte, a truncated header, or a
/// tag mismatch (corrupt file, tampering, or a key that no longer
/// matches the ciphertext — e.g. the keychain slot was deleted).
fn decrypt_tokens(key: &[u8; 32], bytes: &[u8]) -> Result<Vec<u8>, String> {
    if !bytes.starts_with(TOKENS_MAGIC) {
        return Err(
            "tokens file does not start with the PresenceJam encrypted-tokens magic prefix"
                .to_string(),
        );
    }
    let version = bytes[TOKENS_MAGIC.len()];
    if version != TOKENS_VERSION {
        return Err(format!(
            "unsupported tokens cipher version byte {} (this build only reads version {})",
            version, TOKENS_VERSION
        ));
    }
    if bytes.len() < TOKENS_HEADER_LEN {
        return Err(format!(
            "tokens file too short for the {} byte header + ciphertext ({} bytes)",
            TOKENS_HEADER_LEN,
            bytes.len()
        ));
    }
    let nonce = &bytes[TOKENS_MAGIC.len() + 1..TOKENS_HEADER_LEN];
    let ciphertext = &bytes[TOKENS_HEADER_LEN..];
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| "AES-256 key must be 32 bytes".to_string())?;
    cipher
        .decrypt(
            &Nonce::try_from(nonce).map_err(|_| "AES-GCM nonce must be 12 bytes".to_string())?,
            ciphertext,
        )
        .map_err(|_| {
        "tokens file failed AES-GCM authentication (corrupt ciphertext, tampered file, or key mismatch — re-authentication required)"
            .to_string()
    })
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
        #[cfg(unix)]
        {
            let _ = fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
        }
    }
    Ok(dir.join("tokens.json"))
}

/// Read tokens from disk. Returns a default `TokensFile` if the file
/// does not exist or is empty. Returns `Err(...)` if the file exists and
/// is non-empty but cannot be decrypted or deserialised; the caller in
/// `lib::run` setup logs the error and continues with default state,
/// matching the previous `tauri-plugin-store` path's behaviour.
///
/// Since v3.0 the file is AES-256-GCM ciphertext (issue #140); a legacy
/// plaintext JSON file from ≤ v2.10.0 is migrated to ciphertext on first
/// read. A missing keychain key or undecryptable ciphertext surfaces as
/// `Err`, which drives the same re-auth recovery as a corrupt file.
pub fn read_tokens_at(app: &tauri::AppHandle) -> Result<TokensFile, String> {
    let path = tokens_file_path(app)?;
    if !path.exists() {
        log::info!(
            "[TOKEN_IO] read_tokens_at: no file at {}, returning default",
            path.display()
        );
        return Ok(TokensFile::default());
    }
    // Issue #135 path A: tighten the mode of any pre-existing tokens.json
    // that was created loose by an older PresenceJam version (default umask
    // 022 → 0644). Idempotent on a file that is already 0600. Windows
    // default ACL is user-only, so this is a no-op there.
    #[cfg(unix)]
    {
        let current = fs::metadata(&path)
            .map_err(|e| format!("Failed to stat tokens file '{}': {}", path.display(), e))?
            .permissions();
        let current_mode = current.mode() & 0o777;
        if current_mode != 0o600 {
            log::warn!(
                "[TOKEN_IO] tightening tokens.json mode from {:o} to 0600 (issue #135)",
                current_mode
            );
            let mut tightened = current;
            tightened.set_mode(0o600);
            fs::set_permissions(&path, tightened).map_err(|e| {
                format!(
                    "Failed to chmod tokens file '{}' to 0600: {}",
                    path.display(),
                    e
                )
            })?;
        }
    }
    let bytes = fs::read(&path)
        .map_err(|e| format!("Failed to read tokens file '{}': {}", path.display(), e))?;
    if bytes.iter().all(|b| b.is_ascii_whitespace()) {
        log::info!("[TOKEN_IO] read_tokens_at: file is empty, returning default");
        return Ok(TokensFile::default());
    }
    tokens_from_bytes(&path, &bytes)
}

/// Decode the raw bytes of a `tokens.json` file into a [`TokensFile`],
/// fetching the decryption key from the OS keychain.
///
/// - Encrypted (starts with the `PJENC` magic): the key must already
///   exist (`keychain::get_tokens_aes_key`); a missing key is an error
///   that drives re-auth, exactly like a corrupt file.
/// - Legacy plaintext JSON (starts with `{`, i.e. any release ≤ v2.10.0):
///   parsed, then immediately re-written encrypted — the atomic write
///   replaces the plaintext file and pre-clears any stale plaintext
///   sidecar, so the plaintext is gone from the tokens path.
fn tokens_from_bytes(path: &Path, bytes: &[u8]) -> Result<TokensFile, String> {
    if bytes.starts_with(TOKENS_MAGIC) {
        let key = crate::keychain::get_tokens_aes_key()?;
        tokens_from_bytes_with_key(path, bytes, &key)
    } else if bytes.starts_with(b"{") {
        let key = crate::keychain::get_or_create_tokens_aes_key()?;
        tokens_from_bytes_with_key(path, bytes, &key)
    } else {
        Err(format!(
            "tokens file '{}' is neither PJENC-encrypted nor plaintext JSON; refusing to parse",
            path.display()
        ))
    }
}

/// Core decode with an explicit key — used by [`tokens_from_bytes`] and
/// by the test suite (which injects a fixed key so tests never touch the
/// OS keychain).
fn tokens_from_bytes_with_key(
    path: &Path,
    bytes: &[u8],
    key: &[u8; 32],
) -> Result<TokensFile, String> {
    if bytes.starts_with(TOKENS_MAGIC) {
        let plaintext = decrypt_tokens(key, bytes)?;
        let tf = serde_json::from_slice::<TokensFile>(&plaintext).map_err(|e| {
            format!(
                "Failed to parse decrypted tokens file '{}': {}",
                path.display(),
                e
            )
        })?;
        log::info!(
            "[TOKEN_IO] read_tokens_at: loaded - has_spotify={}, has_teams={}",
            tf.spotify_tokens.is_some(),
            tf.teams_tokens.is_some()
        );
        Ok(tf)
    } else if bytes.starts_with(b"{") {
        // Legacy ≤ v2.10.0 plaintext JSON → parse and migrate to
        // ciphertext on the spot.
        let tf = serde_json::from_slice::<TokensFile>(bytes).map_err(|e| {
            format!(
                "Failed to parse legacy plaintext tokens file '{}': {}",
                path.display(),
                e
            )
        })?;
        write_tokens_atomic_with_key(&path.to_path_buf(), &tf, key)?;
        log::info!(
            "[TOKEN_IO] migrated legacy plaintext tokens.json to AES-256-GCM ciphertext at {}",
            path.display()
        );
        Ok(tf)
    } else {
        Err(format!(
            "tokens file '{}' is neither PJENC-encrypted nor plaintext JSON; refusing to parse",
            path.display()
        ))
    }
}

/// Atomic write: serialize, AES-256-GCM encrypt, write the *ciphertext*
/// to a temp file in the same directory, fsync, then rename onto the
/// target. The rename is atomic on POSIX (and on Windows for same-volume
/// renames), so a process kill mid-write cannot leave a half-written
/// file. The pattern mirrors `config::save_config`.
///
/// No plaintext ever touches the disk: only the ciphertext reaches the
/// temp file and the rename source, so the invariant holds even if the
/// process is killed between any two steps. The encryption key is
/// generated on first use and stored in the OS keychain (issue #140); a
/// missing/unavailable keychain is a hard error here — silently falling
/// back to plaintext would regress #140.
pub fn write_tokens_atomic(path: &PathBuf, contents: &TokensFile) -> Result<(), String> {
    let key = crate::keychain::get_or_create_tokens_aes_key()?;
    write_tokens_atomic_with_key(path, contents, &key)
}

/// Core atomic write with an explicit key — used by [`write_tokens_atomic`]
/// and by the test suite (which injects a fixed key so tests never touch
/// the OS keychain).
fn write_tokens_atomic_with_key(
    path: &PathBuf,
    contents: &TokensFile,
    key: &[u8; 32],
) -> Result<(), String> {
    let dir = path
        .parent()
        .ok_or_else(|| format!("tokens path '{}' has no parent dir", path.display()))?;
    if !dir.exists() {
        fs::create_dir_all(dir)
            .map_err(|e| format!("Failed to create dir '{}': {}", dir.display(), e))?;
    }
    let json = serde_json::to_vec_pretty(contents)
        .map_err(|e| format!("Failed to serialize tokens: {}", e))?;
    // Encrypt BEFORE anything touches the disk: only the ciphertext ever
    // reaches the temp file or the rename source.
    let ciphertext = encrypt_tokens(key, &json)?;

    let temp_path = path.with_extension("json.tmp");
    // Issue #135 path A: create the temp file with mode 0600 atomically.
    // Pre-clear any stale sidecar from a previous crash (between temp-write
    // and rename). Without this pre-clear, create_new(true) would error with
    // AlreadyExists on a leftover `.json.tmp`, turning a one-off crash into
    // a permanent save failure until the user manually deletes the sidecar.
    // The pre-clear also removes any stale *plaintext* sidecar left by a
    // ≤ v2.10.0 crash, so the plaintext→ciphertext migration has no
    // leftover plaintext next to the live file. Deletion of a non-existent
    // file is fine — we ignore NotFound.
    if let Err(e) = fs::remove_file(&temp_path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            return Err(format!(
                "Failed to remove stale temp tokens file '{}': {}",
                temp_path.display(),
                e
            ));
        }
    }
    // OpenOptions::create_new(true) prevents racing with a leftover sidecar;
    // .mode(0o600) sets the mode at file-creation time (no chmod-after-create
    // window where tokens would be world-readable). The subsequent
    // rename() preserves the source mode on POSIX, so the live tokens.json
    // ends up at 0600 too. On Windows, the new file inherits the user-only
    // default ACL of the parent directory (no explicit ACL change needed —
    // see SECURITY.md "Data Storage" section).
    #[cfg(unix)]
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp_path)
        .map_err(|e| {
            format!(
                "Failed to create temp tokens file '{}': {}",
                temp_path.display(),
                e
            )
        })?;
    #[cfg(not(unix))]
    let mut f = fs::File::create(&temp_path).map_err(|e| {
        format!(
            "Failed to create temp tokens file '{}': {}",
            temp_path.display(),
            e
        )
    })?;
    f.write_all(&ciphertext)
        .map_err(|e| format!("Failed to write temp tokens file: {}", e))?;
    f.sync_all()
        .map_err(|e| format!("Failed to fsync temp tokens file: {}", e))?;
    drop(f); // close before rename — Windows fails rename of an open file.
    fs::rename(&temp_path, path).map_err(|e| {
        format!(
            "Failed to rename '{}' to '{}': {}",
            temp_path.display(),
            path.display(),
            e
        )
    })?;
    #[cfg(unix)]
    {
        if let Some(parent) = path.parent() {
            if let Ok(dir) = std::fs::File::open(parent) {
                if let Err(e) = dir.sync_all() {
                    log::warn!(
                        "Failed to fsync tokens dir '{}': {}",
                        parent.display(),
                        e
                    );
                }
            }
        }
    }
    log::info!(
        "[TOKEN_IO] write_tokens_atomic: wrote {} encrypted bytes atomically to {}",
        ciphertext.len(),
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
///
/// The on-disk file is AES-256-GCM ciphertext (issue #140); the key is
/// created on first use via `write_tokens_atomic` → keychain.
pub fn persist_tokens(state: &Arc<crate::AppState>, app: &tauri::AppHandle) -> Result<(), String> {
    let path = tokens_file_path(app)?;
    let spotify_tokens = state.tokens.spotify().clone();
    let teams_tokens = state.tokens.teams().clone();
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
///
/// The keychain-held AES key is deliberately kept: the key is a small
/// per-install secret shared by the whole app (not a per-file credential),
/// and deleting it would gain nothing — the tokens file itself is the
/// credential container.
#[allow(dead_code)]
pub fn clear_tokens_file(app: &tauri::AppHandle) -> Result<(), String> {
    let path = tokens_file_path(app)?;
    if path.exists() {
        fs::remove_file(&path)
            .map_err(|e| format!("Failed to delete tokens file '{}': {}", path.display(), e))?;
        log::info!("[TOKEN_IO] clear_tokens_file: deleted {}", path.display());
    } else {
        log::info!(
            "[TOKEN_IO] clear_tokens_file: nothing to delete at {}",
            path.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    /// Fixed key for tests: the suite must never touch the OS keychain,
    /// so every path under test uses the key-injected internals
    /// (`write_tokens_atomic_with_key` / `tokens_from_bytes_with_key`).
    fn test_key() -> [u8; 32] {
        [0x42u8; 32]
    }

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

    fn sample_file() -> TokensFile {
        TokensFile {
            spotify_tokens: Some(sample_spotify()),
            teams_tokens: Some(sample_teams()),
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
        let bytes = fs::read(path)
            .map_err(|e| format!("Failed to read tokens file '{}': {}", path.display(), e))?;
        if bytes.iter().all(|b| b.is_ascii_whitespace()) {
            return Ok(TokensFile::default());
        }
        tokens_from_bytes_with_key(path, &bytes, &test_key())
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
        write_tokens_atomic_with_key(&path, &sample_file(), &test_key()).unwrap();
        // The file on disk must be ciphertext, not plaintext JSON.
        let raw = fs::read(&path).unwrap();
        assert!(
            raw.starts_with(TOKENS_MAGIC),
            "file must start with the PJENC magic prefix"
        );
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
    fn encrypt_decrypt_roundtrip() {
        let json = br#"{"spotify_tokens":{"access_token":"at"}}"#;
        let ct = encrypt_tokens(&test_key(), json).unwrap();
        // Header layout: magic, then version byte, then nonce.
        assert!(ct.starts_with(TOKENS_MAGIC));
        assert_eq!(ct[TOKENS_MAGIC.len()], TOKENS_VERSION);
        // ciphertext = payload + 16-byte GCM tag.
        assert_eq!(ct.len(), TOKENS_HEADER_LEN + json.len() + 16);
        assert_eq!(decrypt_tokens(&test_key(), &ct).unwrap(), json);
        // Fresh nonce per write: identical input must not produce
        // identical ciphertext (guards nonce reuse / pattern leakage).
        assert_ne!(ct, encrypt_tokens(&test_key(), json).unwrap());
        // Wrong key must fail GCM authentication, not yield garbage.
        let other_key = [0x24u8; 32];
        assert!(decrypt_tokens(&other_key, &ct).is_err());
        // Truncated ciphertext must fail.
        assert!(decrypt_tokens(&test_key(), &ct[..ct.len() - 1]).is_err());
    }

    #[test]
    fn corrupt_ciphertext_returns_error() {
        let path = tmp_path("corrupt.json");
        let _ = fs::remove_file(&path);
        let json = serde_json::to_vec(&sample_file()).unwrap();
        let mut ct = encrypt_tokens(&test_key(), &json).unwrap();
        // Flip one bit inside the ciphertext region (after the header).
        ct[TOKENS_HEADER_LEN + 3] ^= 0x01;
        fs::write(&path, &ct).unwrap();
        let err = read_tokens_inner(&path).unwrap_err();
        assert!(
            err.contains("AES-GCM authentication"),
            "unexpected error: {}",
            err
        );
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn unknown_format_returns_error() {
        let path = tmp_path("bad.json");
        let _ = fs::remove_file(&path);
        // Starts with `{` → legacy-plaintext branch → JSON parse error.
        fs::write(&path, "{not valid json").unwrap();
        assert!(read_tokens_inner(&path).is_err());
        // Matches neither magic nor `{` → format error, not a silent
        // default or misparse.
        fs::write(&path, b"\x00\x01\x02binary garbage").unwrap();
        let err = read_tokens_inner(&path).unwrap_err();
        assert!(
            err.contains("neither PJENC-encrypted nor plaintext JSON"),
            "unexpected error: {}",
            err
        );
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn magic_and_version_byte_parse() {
        let json = serde_json::to_vec(&sample_file()).unwrap();
        let ct = encrypt_tokens(&test_key(), &json).unwrap();
        // Unknown format-version byte → rejected, never mis-decrypted.
        let mut future = ct.clone();
        future[TOKENS_MAGIC.len()] = 0x02;
        let err = decrypt_tokens(&test_key(), &future).unwrap_err();
        assert!(
            err.contains("unsupported tokens cipher version"),
            "unexpected error: {}",
            err
        );
        // Corrupted magic prefix → rejected.
        let mut bad_magic = ct.clone();
        bad_magic[0] = b'X';
        let err = decrypt_tokens(&test_key(), &bad_magic).unwrap_err();
        assert!(err.contains("magic"), "unexpected error: {}", err);
        // Truncated header (magic + version, no nonce/ciphertext) → rejected.
        assert!(decrypt_tokens(&test_key(), &ct[..TOKENS_MAGIC.len() + 1]).is_err());
    }

    #[test]
    fn plaintext_migrates_to_ciphertext_on_read() {
        let path = tmp_path("legacy.json");
        let _ = fs::remove_file(&path);
        // Simulate a ≤ v2.10.0 on-disk file: plaintext JSON.
        let legacy = serde_json::to_vec_pretty(&sample_file()).unwrap();
        fs::write(&path, &legacy).unwrap();

        let loaded = tokens_from_bytes_with_key(&path, &legacy, &test_key()).unwrap();
        assert_eq!(loaded.spotify_tokens.unwrap().access_token, "at");
        assert_eq!(loaded.teams_tokens.unwrap().access_token, "tat");

        // The on-disk file must now be ciphertext and the plaintext gone.
        let raw = fs::read(&path).unwrap();
        assert!(
            raw.starts_with(TOKENS_MAGIC),
            "migrated file must start with the PJENC magic prefix"
        );
        assert!(
            !raw
                .windows(b"\"access_token\"".len())
                .any(|w| w == b"\"access_token\""),
            "plaintext JSON must not remain in the migrated file"
        );
        // And the migrated file must round-trip through the decrypt path.
        let reloaded = read_tokens_inner(&path).unwrap();
        assert_eq!(reloaded.spotify_tokens.unwrap().access_token, "at");
        assert_eq!(reloaded.teams_tokens.unwrap().access_token, "tat");
        let _ = fs::remove_file(&path);
    }

    /// Regression guard for issue #135: a stale `.json.tmp` from a previous
    /// crash must not block the next write. Without the pre-clear, the new
    /// create_new(true) on a leftover sidecar would error with AlreadyExists
    /// and turn a one-off crash into a permanent save failure. The pre-clear
    /// must also consume a stale *plaintext* sidecar (what a ≤ v2.10.0
    /// crash would have left) so no plaintext lingers next to the live file.
    #[test]
    fn recovers_from_stale_tmp_sidecar() {
        let path = tmp_path("recover.json");
        let sidecar = path.with_extension("json.tmp");
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&sidecar);

        // Simulate a previous crash that left the sidecar behind.
        fs::write(
            &sidecar,
            b"{\"spotify_tokens\":{\"access_token\":\"LEAKED_PLAINTEXT\"}}",
        )
        .unwrap();
        assert!(sidecar.exists(), "sidecar must exist before recovery");

        write_tokens_atomic_with_key(&path, &sample_file(), &test_key())
            .expect("write must succeed despite stale sidecar");
        assert!(path.exists(), "tokens.json must exist after write");
        assert!(
            !sidecar.exists(),
            "sidecar must be consumed by rename (no .json.tmp leftover)"
        );

        let raw = fs::read(&path).unwrap();
        assert!(raw.starts_with(TOKENS_MAGIC));
        assert!(
            !raw
                .windows(b"LEAKED_PLAINTEXT".len())
                .any(|w| w == b"LEAKED_PLAINTEXT"),
            "stale plaintext sidecar content must not leak into the live file"
        );

        let loaded = read_tokens_inner(&path).unwrap();
        assert!(loaded.spotify_tokens.is_some());

        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&sidecar);
    }
}
