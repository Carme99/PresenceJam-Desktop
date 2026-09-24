//! Token persistence with atomic temp-file + rename writes, encrypted at
//! rest with AES-256-GCM (issue #140).
//!
//! Background: the pre-#65 persistence layer wrote the store JSON
//! in-place; a crash during a write can leave a half-written file that
//! fails to deserialize and bounces the user back to Onboarding.
//! Issue #65 mandates atomic writes for the tokens file. We therefore
//! bypass any plugin store for the tokens file and write a small
//! `{ spotify_tokens, teams_tokens }`
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
//! Legacy plaintext JSON files (releases ≤ v2.10.0) are migrated by the GUI
//! on first read: parsed, then immediately re-written encrypted. Headless
//! CLI reads parse the same payload without touching storage, so reporting
//! and preflight commands remain read-only. The pending-auth blobs (PKCE
//! verifier, device code) are intentionally NOT written here. They live in
//! `AppState` only — a 10–15 min bearer credential is not worth a
//! crash-recovery story that leaks the secret to disk. See issue #65 / HIGH #3
//! in the security review.

use crate::spotify::SpotifyTokens;
use crate::teams::TeamsTokens;
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use rand::TryRngCore;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
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
    rand::rngs::OsRng
        .try_fill_bytes(&mut nonce_bytes)
        .map_err(|e| format!("OS CSPRNG nonce generation failed: {}", e))?;
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|_| "AES-256 key must be 32 bytes".to_string())?;
    let ciphertext = cipher
        .encrypt(
            &Nonce::try_from(&nonce_bytes[..])
                .map_err(|_| "AES-GCM nonce must be 12 bytes".to_string())?,
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
    // Length first: the version-byte read below indexes TOKENS_MAGIC.len(),
    // so a file that is exactly the 5-byte magic would panic on an
    // out-of-bounds index instead of returning the Err that drives the
    // documented re-auth recovery. Every rejection must be an Err.
    if bytes.len() < TOKENS_HEADER_LEN {
        return Err(format!(
            "tokens file too short for the {} byte header + ciphertext ({} bytes)",
            TOKENS_HEADER_LEN,
            bytes.len()
        ));
    }
    let version = bytes[TOKENS_MAGIC.len()];
    if version != TOKENS_VERSION {
        return Err(format!(
            "unsupported tokens cipher version byte {} (this build only reads version {})",
            version, TOKENS_VERSION
        ));
    }
    let nonce = &bytes[TOKENS_MAGIC.len() + 1..TOKENS_HEADER_LEN];
    let ciphertext = &bytes[TOKENS_HEADER_LEN..];
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|_| "AES-256 key must be 32 bytes".to_string())?;
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
/// NOTE (issue #300): this is intentionally NOT the same directory as
/// `config.json`. Tauri's `app_config_dir()` already appends the bundle
/// identifier, so tokens live under `<base>/com.presencejam.app/PresenceJam/`
/// (e.g. `~/.config/com.presencejam.app/PresenceJam/tokens.json` on Linux)
/// while `config::config_dir()` is `<base>/PresenceJam/` (e.g.
/// `~/.config/PresenceJam/config.json`). Keep user-visible backup/restore
/// instructions naming BOTH directories.
pub fn tokens_file_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("Failed to get app config dir: {}", e))?;
    // NOTE (issue #300): unlike config.rs::config_dir(), the Tauri base
    // already contains the bundle id, so this is a DIFFERENT folder from
    // `config.json` — `<base>/com.presencejam.app/PresenceJam/`.
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

/// Bundle identifier from `tauri.conf.json` → `identifier`.
///
/// Tauri's `app_config_dir()` — the directory [`tokens_file_path`] resolves
/// under — is `<platform config root>/<identifier>`, so the headless path
/// below has to name the same id. The test at the bottom of this file pins the
/// constant against `tauri.conf.json`, so the two cannot silently diverge.
pub(crate) const BUNDLE_IDENTIFIER: &str = "com.presencejam.app";

/// Resolve `tokens.json` WITHOUT a `tauri::AppHandle` (issue #679).
///
/// The CLI flags (`--status`, `--sync-once`) resolve argv before any Tauri app
/// is built — on a machine with no display no runtime can be created at all —
/// so they cannot go through [`tokens_file_path`]. This applies the same rule
/// Tauri's `app_config_dir()` applies (the platform config root plus the bundle
/// identifier) and then the same `PresenceJam/` segment, i.e. the identical
/// file, and deliberately does NOT create the directory: a reporting or
/// one-shot CLI run must not write to disk just to read.
pub fn tokens_file_path_headless() -> Result<PathBuf, String> {
    // Mirrors `config::config_dir()`'s use of `directories` (issue #418: the
    // maintained replacement for the unmaintained `dirs` crate) — the same
    // roots Tauri's `dirs::config_dir()` resolves on every desktop platform.
    let base = directories::BaseDirs::new()
        .map(|b| b.config_dir().to_path_buf())
        .ok_or_else(|| {
            "Failed to resolve the tokens path: BaseDirs::new() returned None".to_string()
        })?;
    Ok(base
        .join(BUNDLE_IDENTIFIER)
        .join("PresenceJam")
        .join("tokens.json"))
}

/// Side effects selected when reading a legacy plaintext token file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenReadMode {
    /// Parse legacy plaintext, then atomically replace it with ciphertext.
    MigrateLegacy,
    /// Parse without changing file contents, metadata, or directory entries.
    ReadOnly,
}

/// Why an existing token store could not be loaded.
///
/// `KeychainUnavailable` is transient: the encrypted file may become readable
/// once the platform credential store unlocks, so callers must not replace it
/// with an empty in-memory snapshot. `Corrupt` retains the established
/// recovery behavior: start empty and allow an explicit re-auth/reset to write
/// a fresh store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokensLoadError {
    KeychainUnavailable(String),
    Corrupt(String),
}

impl TokensLoadError {
    pub fn into_message(self) -> String {
        match self {
            Self::KeychainUnavailable(message) | Self::Corrupt(message) => message,
        }
    }
}

impl From<crate::keychain::KeychainReadError> for TokensLoadError {
    fn from(error: crate::keychain::KeychainReadError) -> Self {
        match error {
            crate::keychain::KeychainReadError::Unavailable(message) => {
                Self::KeychainUnavailable(message)
            }
            crate::keychain::KeychainReadError::Absent => {
                Self::Corrupt(crate::keychain::TOKENS_AES_KEY_NOT_FOUND_MSG.to_string())
            }
            crate::keychain::KeychainReadError::Corrupt(message) => Self::Corrupt(message),
        }
    }
}

/// Migrating GUI read from the Tauri app path. Returns a default
/// [`TokensFile`] if the file does not exist or is empty. A platform keychain
/// failure remains typed so setup can protect the recoverable ciphertext from
/// an empty-state persist.
pub fn read_tokens_at(app: &tauri::AppHandle) -> Result<TokensFile, TokensLoadError> {
    let path = tokens_file_path(app).map_err(TokensLoadError::Corrupt)?;
    read_tokens_at_path(&path, TokenReadMode::MigrateLegacy)
}

/// Read tokens from an explicit path (issues #679 and #840).
///
/// The body of [`read_tokens_at`], split out so headless CLI flags use
/// [`TokenReadMode::ReadOnly`]: they share parsing and decryption but never
/// chmod, migrate, rename, or delete storage merely to report state.
pub fn read_tokens_at_path(
    path: &Path,
    mode: TokenReadMode,
) -> Result<TokensFile, TokensLoadError> {
    read_tokens_at_path_with_key_fetcher(path, mode, crate::keychain::read_tokens_aes_key)
}

/// Read an explicit token path with the encrypted-store key lookup injected.
/// Production passes [`crate::keychain::read_tokens_aes_key`]; tests inject
/// platform failures and fixed keys without touching the OS credential store.
fn read_tokens_at_path_with_key_fetcher(
    path: &Path,
    mode: TokenReadMode,
    fetch_key: impl FnOnce() -> Result<[u8; 32], crate::keychain::KeychainReadError>,
) -> Result<TokensFile, TokensLoadError> {
    if !path.exists() {
        log::info!(
            "[TOKEN_IO] read_tokens_at: no file at {}, returning default",
            path.display()
        );
        return Ok(TokensFile::default());
    }
    // Issue #135 path A: tighten the mode of any pre-existing tokens.json
    // that was created loose by an older PresenceJam version (default umask
    // 022 → 0644). This is a GUI migration side effect, so a read-only CLI
    // must skip it even when the legacy payload itself needs no migration.
    if mode == TokenReadMode::MigrateLegacy {
        #[cfg(unix)]
        {
            let current = fs::metadata(path)
                .map_err(|e| {
                    TokensLoadError::Corrupt(format!(
                        "Failed to stat tokens file '{}': {}",
                        path.display(),
                        e
                    ))
                })?
                .permissions();
            let current_mode = current.mode() & 0o777;
            if current_mode != 0o600 {
                log::warn!(
                    "[TOKEN_IO] tightening tokens.json mode from {:o} to 0600 (issue #135)",
                    current_mode
                );
                let mut tightened = current;
                tightened.set_mode(0o600);
                fs::set_permissions(path, tightened).map_err(|e| {
                    TokensLoadError::Corrupt(format!(
                        "Failed to chmod tokens file '{}' to 0600: {}",
                        path.display(),
                        e
                    ))
                })?;
            }
        }
    }
    let bytes = fs::read(path).map_err(|e| {
        TokensLoadError::Corrupt(format!(
            "Failed to read tokens file '{}': {}",
            path.display(),
            e
        ))
    })?;
    if bytes.iter().all(|b| b.is_ascii_whitespace()) {
        log::info!("[TOKEN_IO] read_tokens_at: file is empty, returning default");
        return Ok(TokensFile::default());
    }
    tokens_from_bytes(path, &bytes, mode, fetch_key)
}

/// Parse legacy ≤ v2.10.0 plaintext tokens JSON (issue #352). Shared by the
/// legacy decode paths (`decode_legacy_with_key_fetcher` and
/// [`tokens_from_bytes_with_key`]), so the plaintext→ciphertext migration
/// has a single error string.
fn parse_legacy_tokens_file(bytes: &[u8], path: &Path) -> Result<TokensFile, String> {
    serde_json::from_slice::<TokensFile>(bytes).map_err(|e| {
        format!(
            "Failed to parse legacy plaintext tokens file '{}': {}",
            path.display(),
            e
        )
    })
}

/// Decode the raw bytes of a `tokens.json` file, fetching the encrypted-store
/// key through the injected typed keychain reader.
///
/// A platform-unavailable key lookup is kept distinct from a present key that
/// fails authentication or a malformed token payload. This distinction is what
/// lets setup protect recoverable ciphertext from an empty-state overwrite.
fn tokens_from_bytes(
    path: &Path,
    bytes: &[u8],
    mode: TokenReadMode,
    fetch_key: impl FnOnce() -> Result<[u8; 32], crate::keychain::KeychainReadError>,
) -> Result<TokensFile, TokensLoadError> {
    if bytes.starts_with(TOKENS_MAGIC) {
        let key = fetch_key().map_err(TokensLoadError::from)?;
        tokens_from_bytes_with_key(path, bytes, &key, mode).map_err(TokensLoadError::Corrupt)
    } else if bytes.starts_with(b"{") {
        // Issue #352: parse the legacy plaintext BEFORE touching the OS
        // keychain — see `decode_legacy_with_key_fetcher`, which parses
        // once, fetches the key only for valid JSON in migrating mode, and
        // threads the value into the migration write.
        decode_legacy_with_key_fetcher(
            path,
            bytes,
            mode,
            crate::keychain::read_or_create_tokens_aes_key,
        )
    } else {
        Err(TokensLoadError::Corrupt(format!(
            "tokens file '{}' is neither PJENC-encrypted nor plaintext JSON; refusing to parse",
            path.display()
        )))
    }
}

/// Legacy `{` branch with an injectable key fetcher (issues #352 and #840).
/// Parses the plaintext FIRST. Migrating mode then fetches the key and
/// rewrites storage; read-only mode returns immediately, so neither the
/// keychain nor disk is touched. [`tokens_from_bytes`] passes the real
/// keychain fetcher; tests inject a failing or panic-on-call fetcher to
/// observe the ordering behaviorally.
fn decode_legacy_with_key_fetcher(
    path: &Path,
    bytes: &[u8],
    mode: TokenReadMode,
    fetch_key: impl FnOnce() -> Result<[u8; 32], crate::keychain::KeychainReadError>,
) -> Result<TokensFile, TokensLoadError> {
    let parsed = parse_legacy_tokens_file(bytes, path).map_err(TokensLoadError::Corrupt)?;
    if mode == TokenReadMode::ReadOnly {
        return Ok(parsed);
    }
    let key = fetch_key()?;
    migrate_parsed_legacy(path, parsed, &key).map_err(TokensLoadError::Corrupt)
}

/// Re-write an already-parsed legacy [`TokensFile`] as ciphertext (issue
/// #352). Exists so the gate in [`tokens_from_bytes`] can parse once and
/// hand the value over instead of re-parsing in the migration path.
fn migrate_parsed_legacy(
    path: &Path,
    parsed: TokensFile,
    key: &[u8; 32],
) -> Result<TokensFile, String> {
    write_tokens_atomic_with_key(path, &parsed, key)?;
    log::info!(
        "[TOKEN_IO] migrated legacy plaintext tokens.json to AES-256-GCM ciphertext at {}",
        path.display()
    );
    Ok(parsed)
}

/// Core decode with an explicit key — used by [`tokens_from_bytes`] and
/// by the test suite (which injects a fixed key so tests never touch the
/// OS keychain). Read-only legacy input is parsed but never rewritten.
fn tokens_from_bytes_with_key(
    path: &Path,
    bytes: &[u8],
    key: &[u8; 32],
    mode: TokenReadMode,
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
        // The gate in [`tokens_from_bytes`] has already parsed before
        // touching the keychain; this key-injected path parses because
        // tests supply the key directly (no keychain involved).
        let tf = parse_legacy_tokens_file(bytes, path)?;
        if mode == TokenReadMode::ReadOnly {
            return Ok(tf);
        }
        migrate_parsed_legacy(path, tf, key)
    } else {
        Err(format!(
            "tokens file '{}' is neither PJENC-encrypted nor plaintext JSON; refusing to parse",
            path.display()
        ))
    }
}

/// Process-wide writer lock for tokens.json (issue #565).
///
/// The temp-file + rename write is atomic *per write*, but two overlapping
/// persists (the Spotify refresh thread and the Teams device-code poll both
/// write the whole file) would race on the same sidecar path and the last
/// rename would win with whichever snapshot it happened to capture — which
/// is how a stale access token can end up paired with a fresh refresh token
/// on disk. Serialising every writer makes the last write the newest state.
static WRITE_LOCK: LazyLock<parking_lot::Mutex<()>> = LazyLock::new(|| parking_lot::Mutex::new(()));

/// Run `f` holding the process-wide tokens.json write lock. Every persist
/// funnels through here, and the snapshot it writes must be taken inside the
/// same critical section (see [`persist_tokens`]).
fn with_tokens_write_lock<T>(f: impl FnOnce() -> T) -> T {
    let _guard = WRITE_LOCK.lock();
    f()
}

/// Temp path for one write: the live file's name plus a `.json.tmp.<pid>`
/// suffix, so two processes can never share an in-flight sidecar.
fn temp_tokens_path(path: &Path) -> PathBuf {
    path.with_extension(format!("json.tmp.{}", std::process::id()))
}

/// Remove stale temp sidecars for `path` — crash leftovers from this or a
/// previous process, plus the fixed-name `.json.tmp` used by ≤ 4.5 and the
/// stale *plaintext* sidecar a ≤ v2.10.0 crash could have left next to the
/// live file. `keep` (this write's own sidecar, which a concurrent writer in
/// this process may be using) is left alone. A missing directory is not an
/// error: callers create it before writing.
///
/// Issue #934: a per-pid candidate is only removed when its owner is *provably*
/// gone. `presencejam --sync-once` deliberately runs without the single-instance
/// lock while the GUI polls and persists, so removing that writer's in-flight
/// sidecar makes its rename fail with ENOENT (POSIX) or a sharing violation
/// (Windows) and the refreshed token pair is never saved — see
/// [`sidecar_owner_is_provably_gone`] for what "provably" means per platform.
/// The non-pid names (the ≤ 4.5 fixed name and the ≤ v2.10.0 plaintext sidecar)
/// carry no owner and are always swept, so no plaintext remnant survives.
fn remove_stale_tokens_sidecars(path: &Path, keep: &Path) -> Result<(), String> {
    let Some(dir) = path.parent() else {
        return Ok(());
    };
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return Ok(());
    };
    let prefix = format!("{}.tmp", name);
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            log::warn!(
                "[TOKEN_IO] could not scan '{}' for stale tokens sidecars: {}",
                dir.display(),
                e
            );
            return Ok(());
        }
    };
    for entry in entries.flatten() {
        let candidate = entry.path();
        if candidate == keep {
            continue;
        }
        let matches = candidate
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with(&prefix));
        if !matches {
            continue;
        }
        // Issue #934: never remove another process's in-flight sidecar.
        if let Some(pid) = sidecar_owner_pid(&candidate, &prefix) {
            if !sidecar_owner_is_provably_gone(pid) {
                log::debug!(
                    "[TOKEN_IO] leaving sidecar '{}' alone: pid {} may still be writing",
                    candidate.display(),
                    pid
                );
                continue;
            }
        }
        if let Err(e) = fs::remove_file(&candidate) {
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(format!(
                    "Failed to remove stale temp tokens file '{}': {}",
                    candidate.display(),
                    e
                ));
            }
        }
    }
    Ok(())
}

/// The pid encoded in a per-process sidecar name (`tokens.json.tmp.<pid>`), if
/// the candidate carries one. The ≤ 4.5 fixed name and the ≤ v2.10.0 plaintext
/// sidecar have no pid suffix and yield `None`.
fn sidecar_owner_pid(candidate: &Path, prefix: &str) -> Option<u32> {
    candidate
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.strip_prefix(prefix))
        .and_then(|suffix| suffix.strip_prefix('.'))
        .and_then(|pid| pid.parse().ok())
}

/// May a per-pid sidecar be removed — i.e. is its owner provably gone?
///
/// There is no portable std primitive for "does this pid exist", and a wrong
/// answer is only safe in one direction: leaving an unreclaimed sidecar costs a
/// few hundred bytes in the config directory, whereas removing a live writer's
/// sidecar loses that writer's refreshed token pair (issue #934). So this
/// answers `false` unless the platform can positively prove the pid is gone:
///
/// * Linux exposes every live pid as `/proc/<pid>`, so a missing entry is proof;
/// * elsewhere the sweep declines to remove a per-pid sidecar at all. Adding
///   `kill(pid, 0)` / `OpenProcess` probes needs a `libc` / `windows-sys`
///   dependency, which this module does not have.
fn sidecar_owner_is_provably_gone(pid: u32) -> bool {
    match platform_pid_exists(pid) {
        Some(exists) => !exists,
        None => false,
    }
}

#[cfg(target_os = "linux")]
fn platform_pid_exists(pid: u32) -> Option<bool> {
    Some(Path::new("/proc").join(pid.to_string()).exists())
}

#[cfg(not(target_os = "linux"))]
fn platform_pid_exists(_pid: u32) -> Option<bool> {
    None
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
pub fn write_tokens_atomic(path: &Path, contents: &TokensFile) -> Result<(), String> {
    let key = crate::keychain::get_or_create_tokens_aes_key()?;
    write_tokens_atomic_with_key(path, contents, &key)
}

/// Core atomic write with an explicit key — used by [`write_tokens_atomic`]
/// and by the test suite (which injects a fixed key so tests never touch
/// the OS keychain).
fn write_tokens_atomic_with_key(
    path: &Path,
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

    // Issue #565: the temp file is per-process, so a second instance of the
    // app (or a future caller that bypasses the write lock) cannot clobber
    // this writer's in-flight sidecar. In-process writers are additionally
    // serialised by `with_tokens_write_lock`.
    let temp_path = temp_tokens_path(path);
    // Issue #135 path A: create the temp file with mode 0600 atomically.
    // Sweep stale sidecars from a previous crash (between temp-write and
    // rename) first. Without this, create_new(true) would error with
    // AlreadyExists on a leftover, turning a one-off crash into a permanent
    // save failure until the user manually deletes the sidecar. The sweep
    // also removes a stale *plaintext* sidecar left by a ≤ v2.10.0 crash and
    // the fixed-name `.json.tmp` used by ≤ 4.5, so no plaintext lingers next
    // to the live file. Missing files are fine — everything here is
    // best-effort except an unexpected removal failure.
    remove_stale_tokens_sidecars(path, &temp_path)?;
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
                    log::warn!("Failed to fsync tokens dir '{}': {}", parent.display(), e);
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
///
/// Issue #565: writers are serialised and the snapshot is taken inside the
/// same critical section as the write. The two slot guards are held
/// together, so the pair written to disk is a consistent cut — with two
/// independent reads, a Teams commit landing between them stayed in memory
/// but was dropped from the file, leaving a stale access token paired with
/// a fresh refresh token on the next launch.
pub fn persist_tokens(state: &Arc<crate::AppState>, app: &tauri::AppHandle) -> Result<(), String> {
    let path = tokens_file_path(app)?;
    persist_tokens_at_with_retry(
        state,
        &path,
        || read_tokens_at_path(&path, TokenReadMode::ReadOnly),
        write_tokens_atomic,
    )
}

/// Path-level persist core that first gives a keychain-gated snapshot one
/// chance to recover from the same encrypted file. A successful retry hydrates
/// empty in-memory slots, preserves present token values committed since the
/// blocked persist, clears the gate, and proceeds with the normal write. Any
/// retry failure preserves the existing ciphertext and returns the original
/// persist refusal.
fn persist_tokens_at_with_retry(
    state: &Arc<crate::AppState>,
    path: &Path,
    read: impl FnOnce() -> Result<TokensFile, TokensLoadError>,
    write: impl FnOnce(&Path, &TokensFile) -> Result<(), String>,
) -> Result<(), String> {
    with_tokens_write_lock(|| {
        let blocked_error = "Refusing to overwrite tokens.json after a keychain-unavailable load; \
             retry the token read or reset token storage first"
            .to_string();
        // The marker mutex is the lock-order root. Every token commit/clear
        // takes it before a slot guard, and this persist keeps it through
        // recovery, both slot guards, the snapshot, and the disk write.
        let recovery = state.tokens_load.recovery_guard();
        let mut spotify = state.tokens.spotify_mut();
        let mut teams = state.tokens.teams_mut();
        if state.tokens_load.blocks_persist() {
            let recovered = read().map_err(|_| blocked_error.clone())?;
            if spotify.is_none()
                && !recovery.blocks(crate::TokenProvider::Spotify)
            {
                *spotify = recovered.spotify_tokens;
            }
            if teams.is_none() && !recovery.blocks(crate::TokenProvider::Teams) {
                *teams = recovered.teams_tokens;
            }
            // Do not clear provider tombstones here: an explicit None must
            // continue to win over stale ciphertext after the gate opens.
            state.tokens_load.mark_ready_locked();
        }
        // Both slot guards are acquired before either clone. The write stays
        // inside this critical section so a racing clear cannot be overwritten
        // by a stale snapshot.
        let contents = TokensFile {
            spotify_tokens: spotify.clone(),
            teams_tokens: teams.clone(),
        };
        write(path, &contents)
    })
}

/// Recovery path for a tokens.json that cannot be decrypted: Drop the
/// keychain-held AES key, then delete the tokens file.
///
/// This is the actionable half of issue #566 (a corrupt key entry made both
/// the read *and* the write path fail forever) and reuses
/// [`clear_tokens_file`]. The order matters: deleting the key first means a
/// failure leaves the hash at worst with an orphan ciphertext file, which
/// [`read_tokens_at`] already treats as "start empty"; deleting the file
/// first and then failing to drop the key would leave nothing to recover
/// from at all. The next persist generates a fresh key, and the user signs
/// in again.
pub fn reset_tokens_storage(app: &tauri::AppHandle) -> Result<(), String> {
    with_tokens_write_lock(|| {
        crate::keychain::delete_tokens_aes_key()?;
        clear_tokens_file(app)?;
        let state = app.state::<Arc<crate::AppState>>();
        state.tokens_load.clear_all(&state.tokens);
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// Issue #865: `--serve` bearer token
// ---------------------------------------------------------------------------
//
// The bearer token is owned by the keychain, not the app's config dir:
// it must never be persisted to disk in plaintext (tokens.json is
// already AES-encrypted; the bearer token is a one-process secret that
// lives for the lifetime of a serve-mode daemon, so its keychain slot
// is the only storage location). The token is regenerated only by an
// explicit operator action (`keychain::rotate_serve_token`), so
// external clients that hold the token (schedulers, dashboards) are not
// invalidated every time the daemon restarts.

/// Read the existing serve bearer token from the OS keychain, or
/// generate + persist one if the slot is empty. The "first boot"
/// rotation is intentional — a fresh install gets a token the operator
/// can copy once and reuse across restarts.
///
/// Locked / unavailable keychains propagate as `Err` so the caller can
/// retry (the daemon path uses [`read_or_create_serve_token_with_backoff`]
/// for that). On a keychain-unavailable system the `--serve` mode is
/// not safe to enable, and the error is what surfaces to the operator.
pub fn read_or_create_serve_token() -> Result<String, String> {
    match crate::keychain::read_serve_token() {
        Ok(Some(token)) => Ok(token),
        Ok(None) => crate::keychain::rotate_serve_token(),
        Err(e) => Err(e),
    }
}

/// Same as [`read_or_create_serve_token`] but with bounded exponential
/// backoff for daemon boot (issue #896 — "a locked or missing keychain
/// at boot must retry with backoff rather than exit").
///
/// `attempts` is the maximum number of tries including the first;
/// `base_delay` is the delay before the *second* try (the first try is
/// immediate). The schedule is `base_delay`, `2*base_delay`, `4*base_delay`,
/// … capped by `max_delay`. After the final attempt the last error is
/// returned. A locked `gnome-keyring` (Linux) or a Credential Manager
/// awaiting user consent (Windows) typically resolves within a couple
/// of seconds, so the defaults give the platform room to settle before
/// the daemon gives up and exits with a non-zero status.
pub fn read_or_create_serve_token_with_backoff(
    attempts: u32,
    base_delay: std::time::Duration,
    max_delay: std::time::Duration,
) -> Result<String, String> {
    if attempts == 0 {
        return Err("read_or_create_serve_token_with_backoff: attempts must be > 0".to_string());
    }
    let mut last_err: Option<String> = None;
    let mut delay = base_delay;
    for i in 0..attempts {
        match read_or_create_serve_token() {
            Ok(token) => {
                if i > 0 {
                    log::info!(
                        "[TOKEN_IO] read_or_create_serve_token_with_backoff: succeeded on attempt {}",
                        i + 1
                    );
                }
                return Ok(token);
            }
            Err(e) => {
                log::warn!(
                    "[TOKEN_IO] read_or_create_serve_token_with_backoff: attempt {} failed: {}",
                    i + 1,
                    e
                );
                last_err = Some(e);
                if i + 1 < attempts {
                    std::thread::sleep(delay);
                    delay = std::cmp::min(delay.saturating_mul(2), max_delay);
                }
            }
        }
    }
    Err(last_err.unwrap_or_else(|| "no attempts made".to_string()))
}

/// Delete the tokens file and every stale sidecar next to it. Used by
/// [`reset_tokens_storage`] (issue #766); reconnect flows clear state
/// through the empty-`TokensFile` write path instead.
///
/// The keychain-held AES key is deliberately kept here: the key is a small
/// per-install secret shared by the whole app (not a per-file credential),
/// and deleting it would gain nothing — the tokens file itself is the
/// credential container. [`reset_tokens_storage`] deletes the key itself
/// and then calls this for the files.
///
/// Sidecars are swept with a `keep` path that cannot match any real file
/// (the live path plus a `.nonexistent-keep` suffix), so every candidate
/// the [`remove_stale_tokens_sidecars`] prefix scan finds is removed —
/// the fixed-name `.json.tmp`, every per-pid in-flight leftover, and the
/// ≤ v2.10.0 plaintext sidecar — and no plaintext remnant survives the
/// reset. The per-pid provably-gone guard still applies, so a sidecar of a
/// still-running writer is left alone; the live `tokens.json` delete above
/// does not depend on the sweep. A per-file sweep failure fails the reset
/// (a directory-scan failure only warns — see [`remove_stale_tokens_sidecars`]).
pub fn clear_tokens_file(app: &tauri::AppHandle) -> Result<(), String> {
    clear_tokens_file_at(&tokens_file_path(app)?)
}

/// Path-level half of [`clear_tokens_file`] (issue #766): delete the live
/// file, then sweep every stale sidecar next to it. Split out so the reset
/// file-sweep is testable without a `tauri::AppHandle` — tests call this
/// with a temp dir instead of the real config dir.
fn clear_tokens_file_at(path: &Path) -> Result<(), String> {
    if path.exists() {
        fs::remove_file(path)
            .map_err(|e| format!("Failed to delete tokens file '{}': {}", path.display(), e))?;
        log::info!("[TOKEN_IO] clear_tokens_file: deleted {}", path.display());
    } else {
        log::info!(
            "[TOKEN_IO] clear_tokens_file: nothing to delete at {}",
            path.display()
        );
    }
    let keep = path.with_extension("json.nonexistent-keep");
    remove_stale_tokens_sidecars(path, &keep)?;
    log::info!(
        "[TOKEN_IO] clear_tokens_file: swept sidecars next to {}",
        path.display()
    );
    Ok(())
}

/// Shared structural-source scanner for `#[cfg(test)]` ordering guards
/// (issues #351/#352). Isolates a function body by depth-counting from
/// its opening `{` while skipping string/byte-string/raw-string literals,
/// char literals, and line/block comments — a naive `{`/`}` byte counter
/// breaks on braces inside literals (e.g. `"{CMD} … {} …"` or `b"{"`).
#[cfg(test)]
pub(crate) mod test_scan {
    /// Return the body of the function whose signature contains
    /// `fn_marker` (e.g. `"fn complete_spotify_auth_manual("`), without
    /// the outer braces. The signature must carry no string literals, so
    /// the first `{` after the marker opens the body.
    pub(crate) fn fn_body<'a>(src: &'a str, fn_marker: &str) -> &'a str {
        let sig = src.find(fn_marker).expect("function must exist");
        // The signature carries no string literals, so the first `{` after
        // it opens the body.
        let rel = src[sig..].find('{').expect("function must have a body");
        let b = src.as_bytes();
        let n = b.len();
        let mut i = sig + rel;
        let mut depth: u32 = 0;
        let end = loop {
            assert!(i < n, "function body has unbalanced braces");
            let c = b[i];
            // Line comment: skip to newline.
            if c == b'/' && i + 1 < n && b[i + 1] == b'/' {
                while i < n && b[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            // Block comment (nesting, as in Rust): skip to close.
            if c == b'/' && i + 1 < n && b[i + 1] == b'*' {
                let mut nest: u32 = 1;
                i += 2;
                while i < n && nest > 0 {
                    if b[i] == b'/' && i + 1 < n && b[i + 1] == b'*' {
                        nest += 1;
                        i += 2;
                    } else if b[i] == b'*' && i + 1 < n && b[i + 1] == b'/' {
                        nest -= 1;
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                continue;
            }
            // Raw string `r"…"`, `r#"…"#`, or byte-raw `br"…"`: skip to the
            // closing quote plus matching hashes.
            let raw_hashes = if c == b'r' && i + 1 < n && (b[i + 1] == b'"' || b[i + 1] == b'#') {
                Some(i + 1)
            } else if c == b'b' && i + 1 < n && b[i + 1] == b'r' && i + 2 < n {
                Some(i + 2)
            } else {
                None
            };
            if let Some(mut j) = raw_hashes {
                let mut hashes = 0;
                while j < n && b[j] == b'#' {
                    hashes += 1;
                    j += 1;
                }
                if j < n && b[j] == b'"' {
                    j += 1;
                    loop {
                        assert!(j < n, "raw string never terminates");
                        if b[j] == b'"' {
                            let mut k = j + 1;
                            let mut seen = 0;
                            while seen < hashes && k < n && b[k] == b'#' {
                                seen += 1;
                                k += 1;
                            }
                            if seen == hashes {
                                i = k;
                                break;
                            }
                        }
                        j += 1;
                    }
                    continue;
                }
            }
            // Ordinary `"…"` or byte `"…"` string: skip with `\` escapes.
            if c == b'"' || (c == b'b' && i + 1 < n && b[i + 1] == b'"') {
                i += if c == b'"' { 1 } else { 2 };
                while i < n && b[i] != b'"' {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
                i += 1;
                continue;
            }
            // Char `'x'` / byte-char `b'x'` literal (with `\` escapes); a
            // bare `'` (lifetime) just advances one byte.
            if c == b'\'' || (c == b'b' && i + 1 < n && b[i + 1] == b'\'') {
                let q = if c == b'\'' { i } else { i + 1 };
                if q + 2 < n && b[q + 1] == b'\\' {
                    let mut k = q + 2;
                    while k < n && b[k] != b'\'' {
                        k += 1;
                    }
                    i = k + 1;
                } else if q + 2 < n && b[q + 2] == b'\'' {
                    i = q + 3;
                } else {
                    i += 1;
                }
                continue;
            }
            match c {
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
        };
        &src[sig + rel + 1..end]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;

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

    /// A fresh directory for one test, removed by that test on its normal exit
    /// path. The shared temp dir is not a sandbox: sidecar names are keyed on
    /// the pid, so a leftover from an earlier run at a recycled pid — or from a
    /// test running in parallel — would otherwise decide a test's outcome. The
    /// unique name means a panic (which skips the cleanup) cannot poison a
    /// later run.
    fn unique_tmp_dir(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = env::temp_dir().join(format!(
            "presencejam-test-{}-{}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
            nanos,
            tag
        ));
        fs::create_dir_all(&dir).expect("a unique test dir must be creatable");
        dir
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
        tokens_from_bytes_with_key(path, &bytes, &test_key(), TokenReadMode::MigrateLegacy)
    }

    #[test]
    fn keychain_unavailable_retry_recovers_before_next_persist() {
        let dir = unique_tmp_dir("keychain-retry");
        let path = dir.join("tokens.json");
        write_tokens_atomic_with_key(&path, &sample_file(), &test_key()).unwrap();
        let state = Arc::new(crate::AppState::new());

        let unavailable =
            read_tokens_at_path_with_key_fetcher(&path, TokenReadMode::ReadOnly, || {
                Err(crate::keychain::KeychainReadError::Unavailable(
                    "credential store locked".to_string(),
                ))
            });
        assert!(matches!(
            &unavailable,
            Err(TokensLoadError::KeychainUnavailable(_))
        ));
        crate::apply_token_load_result(&state, unavailable);
        assert_eq!(
            state.tokens_load.state(),
            crate::TokensLoadState::KeychainUnavailable
        );

        let mut writer_called = false;
        let first = persist_tokens_at_with_retry(
            &state,
            &path,
            || {
                read_tokens_at_path_with_key_fetcher(&path, TokenReadMode::ReadOnly, || {
                    Ok(test_key())
                })
            },
            |_path, _contents| {
                writer_called = true;
                Ok(())
            },
        );
        assert!(
            first.is_ok(),
            "a successful retry must make the persist writable"
        );
        assert!(writer_called, "the successful retry must reach the writer");
        assert_eq!(state.tokens_load.state(), crate::TokensLoadState::Ready);
        assert_eq!(state.tokens.spotify().as_ref().unwrap().access_token, "at");
        assert_eq!(state.tokens.teams().as_ref().unwrap().access_token, "tat");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn keychain_unavailable_retry_preserves_post_failure_token_changes() {
        let dir = unique_tmp_dir("keychain-retry-fresh-token");
        let path = dir.join("tokens.json");
        write_tokens_atomic_with_key(&path, &sample_file(), &test_key()).unwrap();
        let state = Arc::new(crate::AppState::new());
        state.tokens_load.mark_keychain_unavailable();

        let blocked = persist_tokens_at_with_retry(
            &state,
            &path,
            || {
                Err(TokensLoadError::KeychainUnavailable(
                    "credential store locked".to_string(),
                ))
            },
            |_path, _contents| panic!("blocked recovery must not reach the writer"),
        );
        assert!(blocked.is_err());

        let fresh_spotify = SpotifyTokens {
            access_token: "fresh-spotify-access".to_string(),
            refresh_token: "fresh-spotify-refresh".to_string(),
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(7200),
        };
        let fresh_teams = TeamsTokens {
            access_token: "fresh-teams-access".to_string(),
            refresh_token: Some("fresh-teams-refresh".to_string()),
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(7200),
        };
        *state.tokens.spotify_mut() = Some(fresh_spotify.clone());
        *state.tokens.teams_mut() = Some(fresh_teams.clone());

        let mut written = None;
        persist_tokens_at_with_retry(
            &state,
            &path,
            || Ok(sample_file()),
            |_path, contents| {
                written = Some(contents.clone());
                Ok(())
            },
        )
        .expect("the recovered keychain must unblock persistence");

        let written = written.expect("the recovered persist must reach the writer");
        assert_eq!(written.spotify_tokens, Some(fresh_spotify));
        assert_eq!(written.teams_tokens, Some(fresh_teams));
        assert_eq!(state.tokens_load.state(), crate::TokensLoadState::Ready);
        assert_eq!(
            state.tokens.spotify().as_ref(),
            written.spotify_tokens.as_ref()
        );
        assert_eq!(state.tokens.teams().as_ref(), written.teams_tokens.as_ref());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn keychain_unavailable_retry_keeps_explicit_provider_clear() {
        let dir = unique_tmp_dir("keychain-retry-explicit-clear");
        let path = dir.join("tokens.json");
        write_tokens_atomic_with_key(&path, &sample_file(), &test_key()).unwrap();
        let state = Arc::new(crate::AppState::new());
        state.tokens_load.mark_keychain_unavailable();

        let blocked = persist_tokens_at_with_retry(
            &state,
            &path,
            || Err(TokensLoadError::KeychainUnavailable("locked".to_string())),
            |_path, _contents| panic!("blocked recovery must not write"),
        );
        assert!(blocked.is_err());

        let fresh_spotify = SpotifyTokens {
            access_token: "fresh-spotify-access".to_string(),
            refresh_token: "fresh-spotify-refresh".to_string(),
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(7200),
        };
        state
            .tokens_load
            .commit_spotify(&state.tokens, fresh_spotify.clone());
        state.tokens_load.clear_teams(&state.tokens);

        let mut written = None;
        persist_tokens_at_with_retry(
            &state,
            &path,
            || Ok(sample_file()),
            |_path, contents| {
                written = Some(contents.clone());
                Ok(())
            },
        )
        .expect("a successful retry must persist the merged state");

        let written = written.expect("retry must reach writer");
        assert_eq!(written.spotify_tokens, Some(fresh_spotify));
        assert_eq!(written.teams_tokens, None);
        assert_eq!(state.tokens_load.state(), crate::TokensLoadState::Ready);
        assert_eq!(state.tokens.teams(), &None);
        fs::remove_dir_all(dir).unwrap();
    }


    #[test]
    fn persist_holds_both_token_guards_before_cloning() {
        let body = test_scan::fn_body(
            include_str!("token_io.rs"),
            "fn persist_tokens_at_with_retry(",
        );
        let spotify_guard = body
            .find("let mut spotify = state.tokens.spotify_mut();")
            .expect("Spotify guard must be acquired");
        let teams_guard = body
            .find("let mut teams = state.tokens.teams_mut();")
            .expect("Teams guard must be acquired");
        let spotify_clone = body
            .find("spotify_tokens: spotify.clone()")
            .expect("Spotify snapshot clone must exist");
        let teams_clone = body
            .find("teams_tokens: teams.clone()")
            .expect("Teams snapshot clone must exist");
        assert!(
            spotify_guard < spotify_clone && teams_guard < teams_clone,
            "both slot guards must be acquired before either snapshot clone"
        );
    }
    #[test]
    fn keychain_unavailable_retry_preserves_ciphertext_when_still_locked() {
        let dir = unique_tmp_dir("keychain-still-locked");
        let path = dir.join("tokens.json");
        write_tokens_atomic_with_key(&path, &sample_file(), &test_key()).unwrap();
        let ciphertext = fs::read(&path).unwrap();
        let state = Arc::new(crate::AppState::new());
        state.tokens_load.mark_keychain_unavailable();

        let mut writer_called = false;
        let persist = persist_tokens_at_with_retry(
            &state,
            &path,
            || {
                Err(TokensLoadError::KeychainUnavailable(
                    "credential store locked".to_string(),
                ))
            },
            |_path, _contents| {
                writer_called = true;
                Ok(())
            },
        );
        assert!(persist.is_err());
        assert!(!writer_called, "a failed retry must not reach the writer");
        assert_eq!(fs::read(&path).unwrap(), ciphertext);
        assert_eq!(
            state.tokens_load.state(),
            crate::TokensLoadState::KeychainUnavailable
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn corrupt_ciphertext_is_separate_and_keeps_recovery_persistence() {
        let dir = unique_tmp_dir("corrupt-ciphertext");
        let path = dir.join("tokens.json");
        write_tokens_atomic_with_key(&path, &sample_file(), &test_key()).unwrap();
        let mut ciphertext = fs::read(&path).unwrap();
        *ciphertext.last_mut().unwrap() ^= 1;
        fs::write(&path, &ciphertext).unwrap();
        let state = Arc::new(crate::AppState::new());

        let result =
            read_tokens_at_path_with_key_fetcher(&path, TokenReadMode::ReadOnly, || Ok(test_key()));
        assert!(matches!(&result, Err(TokensLoadError::Corrupt(_))));
        crate::apply_token_load_result(&state, result);
        assert_eq!(state.tokens_load.state(), crate::TokensLoadState::Ready);

        let mut writer_called = false;
        persist_tokens_at_with_retry(
            &state,
            &path,
            || Err(TokensLoadError::KeychainUnavailable("unused".to_string())),
            |_path, _contents| {
                writer_called = true;
                Ok(())
            },
        )
        .expect("corrupt-store recovery must remain writable");
        assert!(writer_called);
        fs::remove_dir_all(dir).unwrap();
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
        // Issue #263: the atomic write creates the temp file with
        // `.mode(0o600)` and the subsequent rename() preserves that mode onto
        // the live file, so tokens.json must be user-only readable. Without
        // this assertion, dropping `.mode(0o600)` silently regresses to the
        // process umask (typically 0644) and every access/refresh token
        // becomes world-readable on a multi-user Linux box. Asserting the
        // mode rather than the exact builder options keeps the test focused
        // on the observable on-disk invariant.
        #[cfg(unix)]
        {
            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(
                mode, 0o600,
                "tokens.json must be user-only readable, got {:o}",
                mode
            );
        }
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
        // Issue #294: every prefix SHORTER than a full header must be
        // rejected without panicking. The version-byte read indexes
        // TOKENS_MAGIC.len(), so a prefix of exactly the 5-byte magic would
        // panic out of bounds here rather than returning the Err that drives
        // the documented re-auth recovery — and this runs on the startup
        // path, so the panic aborts the app before a window exists.
        for len in 0..TOKENS_HEADER_LEN {
            let err = decrypt_tokens(&test_key(), &ct[..len])
                .expect_err(&format!("{} byte prefix must be rejected", len));
            assert!(
                err.contains("magic") || err.contains("too short"),
                "{} byte prefix gave an unexpected error: {}",
                len,
                err
            );
        }
    }

    #[test]
    fn plaintext_migrates_to_ciphertext_on_read() {
        let path = tmp_path("legacy.json");
        let _ = fs::remove_file(&path);
        // Simulate a ≤ v2.10.0 on-disk file: plaintext JSON.
        let legacy = serde_json::to_vec_pretty(&sample_file()).unwrap();
        fs::write(&path, &legacy).unwrap();

        let loaded =
            tokens_from_bytes_with_key(&path, &legacy, &test_key(), TokenReadMode::MigrateLegacy)
                .unwrap();
        assert_eq!(loaded.spotify_tokens.unwrap().access_token, "at");
        assert_eq!(loaded.teams_tokens.unwrap().access_token, "tat");

        // The on-disk file must now be ciphertext and the plaintext gone.
        let raw = fs::read(&path).unwrap();
        assert!(
            raw.starts_with(TOKENS_MAGIC),
            "migrated file must start with the PJENC magic prefix"
        );
        assert!(
            !raw.windows(b"\"access_token\"".len())
                .any(|w| w == b"\"access_token\""),
            "plaintext JSON must not remain in the migrated file"
        );
        // And the migrated file must round-trip through the decrypt path.
        let reloaded = read_tokens_inner(&path).unwrap();
        assert_eq!(reloaded.spotify_tokens.unwrap().access_token, "at");
        assert_eq!(reloaded.teams_tokens.unwrap().access_token, "tat");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn read_only_legacy_read_returns_tokens_without_touching_storage() {
        let dir = unique_tmp_dir("readonly-legacy");
        let path = dir.join("tokens.json");
        let legacy = serde_json::to_vec_pretty(&sample_file()).unwrap();
        fs::write(&path, &legacy).unwrap();
        let before = fs::metadata(&path).unwrap();
        let before_modified = before.modified().unwrap();
        #[cfg(unix)]
        let before_mode = before.permissions().mode() & 0o777;
        #[cfg(unix)]
        let before_inode = before.ino();

        let loaded = read_tokens_at_path(&path, TokenReadMode::ReadOnly).unwrap();
        assert_eq!(loaded.spotify_tokens.unwrap().access_token, "at");
        assert_eq!(loaded.teams_tokens.unwrap().access_token, "tat");

        assert!(path.exists(), "read-only CLI must not delete tokens.json");
        assert_eq!(
            fs::read(&path).unwrap(),
            legacy,
            "file bytes must be unchanged"
        );
        let after = fs::metadata(&path).unwrap();
        assert_eq!(
            after.modified().unwrap(),
            before_modified,
            "mtime must be unchanged"
        );
        #[cfg(unix)]
        {
            assert_eq!(
                after.permissions().mode() & 0o777,
                before_mode,
                "mode must be unchanged"
            );
            assert_eq!(
                after.ino(),
                before_inode,
                "read must not rename tokens.json"
            );
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn read_only_legacy_decoder_never_fetches_key_or_rewrites_file() {
        let dir = unique_tmp_dir("readonly-legacy-decoder");
        let path = dir.join("tokens.json");
        let legacy = serde_json::to_vec_pretty(&sample_file()).unwrap();
        fs::write(&path, &legacy).unwrap();

        let loaded =
            decode_legacy_with_key_fetcher(&path, &legacy, TokenReadMode::ReadOnly, || {
                panic!("read-only legacy decode must not fetch a key")
            })
            .unwrap();

        assert_eq!(loaded.spotify_tokens.unwrap().access_token, "at");
        assert_eq!(loaded.teams_tokens.unwrap().access_token, "tat");
        assert_eq!(
            fs::read(&path).unwrap(),
            legacy,
            "decoder must not rewrite tokens.json"
        );
        fs::remove_dir_all(dir).unwrap();
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
            !raw.windows(b"LEAKED_PLAINTEXT".len())
                .any(|w| w == b"LEAKED_PLAINTEXT"),
            "stale plaintext sidecar content must not leak into the live file"
        );

        let loaded = read_tokens_inner(&path).unwrap();
        assert!(loaded.spotify_tokens.is_some());

        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&sidecar);
    }

    // Issue #352 (wiring guard): the legacy `{` branch in `tokens_from_bytes`
    // must delegate to `decode_legacy_with_key_fetcher`, which parses the
    // JSON before fetching the key (pinned behaviorally by
    // `legacy_key_fetch_follows_parse` below). This source scan is a
    // deliberate, load-bearing proxy: `tokens_from_bytes` touches the real
    // OS keychain, so no unit test can drive it without keychain side
    // effects — the scan only guards that a future refactor keeps the
    // delegation instead of inlining a fetch-first branch. The body is
    // isolated with the shared literal-aware scanner (`test_scan`), not a
    // next-function boundary anchor: the dispatch arms contain `b"{"`,
    // whose brace would corrupt a naive byte counter.
    #[test]
    fn legacy_branch_delegates_to_ordered_decoder() {
        let src = include_str!("token_io.rs");
        let body = test_scan::fn_body(src, "fn tokens_from_bytes(");
        assert!(
            body.contains("decode_legacy_with_key_fetcher("),
            "legacy branch must delegate to the parse-before-keychain decoder"
        );
    }

    // Issue #352 (behavioral): the injectable legacy decoder parses BEFORE
    // fetching the key. Corrupt legacy input with a spy fetcher must
    // surface the parse error without ever calling the fetcher; valid
    // legacy input must reach the fetcher (its error surfacing here proves
    // the key is fetched only after the JSON proves valid).
    #[test]
    fn legacy_key_fetch_follows_parse() {
        use std::cell::Cell;
        let path = tmp_path("legacy-order.json");
        // Corrupt legacy bytes: parse fails, fetcher must never run.
        let called = Cell::new(false);
        let bytes = b"{not valid json";
        let err =
            decode_legacy_with_key_fetcher(&path, bytes, TokenReadMode::MigrateLegacy, || {
                called.set(true);
                Ok(test_key())
            })
            .expect_err("corrupt legacy must fail");
        let message = match err {
            TokensLoadError::Corrupt(message) => message,
            other => panic!("parse failure must be corrupt, got {other:?}"),
        };
        assert!(
            message.contains("Failed to parse legacy plaintext tokens file"),
            "parse error expected, got: {message}"
        );
        assert!(
            !called.get(),
            "key fetcher must not run for corrupt legacy input"
        );
        // Valid legacy bytes: parse succeeds, so the fetcher runs and its
        // error surfaces (no migration write happens — the fetch fails
        // first, so no file is created).
        let valid = serde_json::to_vec(&sample_file()).unwrap();
        let called = Cell::new(false);
        let err =
            decode_legacy_with_key_fetcher(&path, &valid, TokenReadMode::MigrateLegacy, || {
                called.set(true);
                Err::<[u8; 32], crate::keychain::KeychainReadError>(
                    crate::keychain::KeychainReadError::Unavailable(
                        "keychain unavailable".to_string(),
                    ),
                )
            })
            .expect_err("failing fetcher must fail");
        assert!(called.get(), "key fetcher must run for valid legacy input");
        assert!(
            matches!(&err, TokensLoadError::KeychainUnavailable(message) if message == "keychain unavailable"),
            "typed unavailable error expected, got: {err:?}"
        );
    }

    // Issue #352 (behavioral): corrupt legacy `{` bytes surface the legacy
    // parse error through the key-injected path (no keychain involved).
    #[test]
    fn corrupt_legacy_plaintext_yields_parse_error() {
        let path = tmp_path("corrupt-legacy.json");
        let bytes = b"{not valid json";
        let err = tokens_from_bytes_with_key(&path, bytes, &test_key(), TokenReadMode::ReadOnly)
            .expect_err("corrupt legacy must fail");
        assert!(
            err.contains("Failed to parse legacy plaintext tokens file"),
            "legacy parse error expected, got: {}",
            err
        );
    }

    // Issue #565 (behavioral): concurrent writers must not fight over the
    // temp sidecar. Two threads writing the same file through the write lock
    // (as `persist_tokens` does) must each succeed, and the surviving file
    // must be decryptable — with the old shared `.json.tmp` name and no
    // serialisation, one writer's `remove_file` + `create_new` window made
    // the other fail with AlreadyExists or a vanished sidecar, and the loser
    // could also rename a half-torn-down temp into the live path.
    #[test]
    fn concurrent_writers_never_fight_over_the_sidecar() {
        let path = tmp_path("concurrent.json");
        let _ = fs::remove_file(&path);
        let key = test_key();

        std::thread::scope(|scope| {
            let handles: Vec<_> = (0..4)
                .map(|thread| {
                    let path = path.clone();
                    scope.spawn(move || {
                        for i in 0..15 {
                            let mut file = sample_file();
                            file.spotify_tokens.as_mut().unwrap().access_token =
                                format!("at-{thread}-{i}");
                            with_tokens_write_lock(|| {
                                write_tokens_atomic_with_key(&path, &file, &key)
                            })
                            .expect("a serialised write must never fail");
                        }
                    })
                })
                .collect();
            for handle in handles {
                handle.join().expect("writer thread must not panic");
            }
        });

        // Whatever the interleaving, the file must hold a complete, decryptable
        // snapshot — never a torn or unreadable one.
        let loaded = read_tokens_inner(&path).unwrap();
        assert!(loaded.spotify_tokens.is_some());
        assert!(loaded.teams_tokens.is_some());
        // No sidecars survive a completed write.
        assert!(!temp_tokens_path(&path).exists());
        assert!(!path.with_extension("json.tmp").exists());
        let _ = fs::remove_file(&path);
    }

    // Issue #565 (behavioral): the write path sweeps stale sidecars of every
    // generation — the fixed-name `.json.tmp` (≤ 4.5) and any per-pid leftover
    // from a crashed process — but never its own in-flight sidecar.
    //
    // The fixture works in a directory of its own and names the foreign sidecar
    // with a pid no live process can hold. `temp_tokens_path` keys a sidecar on
    // *this* process's pid, so a fabricated file at that name is not a foreign
    // leftover at all: it is the very path the write below opens with
    // `create_new(true)`, the `keep` contract preserves it, and a recycled pid
    // re-arms it from a previous run — a permanent AlreadyExists.
    #[test]
    fn stale_sidecars_of_every_generation_are_swept() {
        // Above every platform's pid_max ceiling, so it can never be — nor
        // later be reused as — this process's pid.
        const FOREIGN_PID: u32 = u32::MAX;
        assert_ne!(
            FOREIGN_PID,
            std::process::id(),
            "the foreign pid must not be this process's"
        );
        let dir = unique_tmp_dir("sweep");
        let path = dir.join("tokens.json");
        let legacy_sidecar = path.with_extension("json.tmp");
        let foreign_sidecar = path.with_extension(format!("json.tmp.{}", FOREIGN_PID));
        let own_sidecar = temp_tokens_path(&path);
        for sidecar in [&legacy_sidecar, &foreign_sidecar, &own_sidecar] {
            fs::write(sidecar, b"{\"leaked\":\"PLAINTEXT\"}").unwrap();
        }

        remove_stale_tokens_sidecars(&path, &own_sidecar).unwrap();

        assert!(
            !legacy_sidecar.exists(),
            "the ≤ 4.5 fixed-name sidecar must be swept"
        );
        // Issue #934: a per-pid leftover is swept only when its owner is
        // provably gone. `FOREIGN_PID` is above every platform's pid ceiling,
        // so it is dead wherever a liveness probe exists (`/proc` on Linux);
        // where the sweep cannot tell, it declines to remove and the leftover
        // stays.
        #[cfg(target_os = "linux")]
        assert!(
            !foreign_sidecar.exists(),
            "a dead process's crashed sidecar must be swept"
        );
        #[cfg(not(target_os = "linux"))]
        assert!(
            foreign_sidecar.exists(),
            "without a pid-liveness probe the sweep must not remove another \
             process's sidecar (issue #934)"
        );
        assert!(
            own_sidecar.exists(),
            "this write's own sidecar must be left alone"
        );

        // Write phase: the sweep above consumed the foreign sidecar, so plant a
        // fresh one — and drop the fixture's stand-in at this process's own
        // sidecar path, which the `keep` contract preserves and which would
        // therefore make the write's `create_new(true)` fail with AlreadyExists.
        fs::write(&foreign_sidecar, b"{\"leaked\":\"PLAINTEXT\"}").unwrap();
        fs::remove_file(&own_sidecar).unwrap();

        write_tokens_atomic_with_key(&path, &sample_file(), &test_key())
            .expect("write must succeed with a foreign-pid sidecar present");
        assert!(path.exists());
        assert!(!own_sidecar.exists(), "rename must consume the sidecar");
        #[cfg(target_os = "linux")]
        assert!(
            !foreign_sidecar.exists(),
            "the write's own pre-clear must consume the dead process's sidecar"
        );
        #[cfg(not(target_os = "linux"))]
        assert!(
            foreign_sidecar.exists(),
            "the write must not consume a per-pid sidecar it cannot attribute \
             to a dead process (issue #934)"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    // Issue #934 (behavioral): a sidecar whose owning pid is still alive must
    // survive the sweep. `presencejam --sync-once` deliberately runs without the
    // single-instance lock while the GUI polls and persists, so a persist that
    // removed the CLI's in-flight sidecar made its rename fail with ENOENT
    // (POSIX) or a sharing violation (Windows) and the refreshed token pair was
    // never saved.
    //
    // This process stands in for the live owner: its pid is alive by
    // construction. `keep` is deliberately a path that does not match the
    // candidate, so the sweep really evaluates it instead of skipping it as this
    // write's own sidecar.
    #[test]
    fn a_live_owners_sidecar_survives_the_sweep() {
        let dir = unique_tmp_dir("live-sidecar");
        let path = dir.join("tokens.json");
        let live_sidecar = path.with_extension(format!("json.tmp.{}", std::process::id()));
        let dead_sidecar = path.with_extension(format!("json.tmp.{}", u32::MAX));
        let keep = dir.join("tokens.json.tmp.keep");
        for sidecar in [&live_sidecar, &dead_sidecar, &keep] {
            fs::write(sidecar, b"{\"leaked\":\"PLAINTEXT\"}").unwrap();
        }

        remove_stale_tokens_sidecars(&path, &keep).unwrap();

        assert!(
            live_sidecar.exists(),
            "a sidecar owned by a running process must never be removed"
        );
        assert!(keep.exists(), "the `keep` path must be left alone");
        #[cfg(target_os = "linux")]
        assert!(
            !dead_sidecar.exists(),
            "an owner that is provably gone must still be swept"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    // Issue #565: two processes must never share a sidecar path.
    #[test]
    fn temp_path_is_per_process() {
        let path = PathBuf::from("/tmp/presencejam-tokens.json");
        let temp = temp_tokens_path(&path);
        assert_eq!(
            temp,
            PathBuf::from(format!(
                "/tmp/presencejam-tokens.json.tmp.{}",
                std::process::id()
            ))
        );
        assert_ne!(temp, path);
        assert_eq!(
            temp.parent(),
            path.parent(),
            "the sidecar must stay in the target directory for an atomic rename"
        );
    }

    /// Issue #679: the headless path the CLI flags resolve must name the same
    /// directory Tauri's `app_config_dir()` names, i.e. the platform config
    /// root plus the bundle identifier in `tauri.conf.json`. Pinned against the
    /// conf file itself so a renamed identifier cannot silently send the CLI
    /// looking in a folder the app never writes to.
    #[test]
    fn headless_path_uses_the_configured_bundle_identifier() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri.conf.json JSON");
        assert_eq!(
            conf["identifier"].as_str(),
            Some(BUNDLE_IDENTIFIER),
            "BUNDLE_IDENTIFIER must match tauri.conf.json's identifier"
        );
    }

    /// Issue #679: the headless reader resolves `<config>/<bundle id>/
    /// PresenceJam/tokens.json` — the same file the app-side
    /// `tokens_file_path` resolves — and reads a missing file as the empty
    /// default WITHOUT creating anything: a reporting/one-shot CLI run must not
    /// write to disk just to read.
    #[test]
    fn headless_reader_resolves_the_same_file_and_never_creates_it() {
        let path = tokens_file_path_headless().expect("headless tokens path must resolve");
        assert_eq!(
            path.file_name().and_then(|n| n.to_str()),
            Some("tokens.json"),
            "the headless path must end at tokens.json"
        );
        assert_eq!(
            path.parent()
                .and_then(|d| d.file_name())
                .and_then(|n| n.to_str()),
            Some("PresenceJam"),
            "the headless path must use the app's `PresenceJam/` segment"
        );
        assert!(
            path.parent()
                .and_then(|d| d.parent())
                .and_then(|d| d.file_name())
                .and_then(|n| n.to_str())
                == Some(BUNDLE_IDENTIFIER),
            "the headless path must be nested under the bundle identifier, like \
             Tauri's app_config_dir()"
        );

        // Read from a path shaped like the real one, but inside a directory
        // this test owns: the file is absent, so the read must return the empty
        // default and leave the tree untouched.
        let dir = unique_tmp_dir("headless-read");
        let absent = dir
            .join(BUNDLE_IDENTIFIER)
            .join("PresenceJam")
            .join("tokens.json");
        let tokens = read_tokens_at_path(&absent, TokenReadMode::ReadOnly)
            .expect("a missing tokens file must read empty");
        assert!(tokens.spotify_tokens.is_none() && tokens.teams_tokens.is_none());
        assert!(
            !absent.parent().expect("parent dir").exists(),
            "the headless reader must not create the tokens directory"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    // Issue #766: `clear_tokens_file` must sweep every sidecar next to the
    // live file, not just delete `tokens.json`. Pre-fix it removed only the
    // live path, so a plaintext `tokens.json.tmp` leftover from a ≤ v2.10.0
    // crash survived the reset and this assertion fails on that shape.
    #[test]
    fn clear_tokens_file_sweeps_live_file_and_sidecars() {
        let dir = unique_tmp_dir("clear-sweep");
        let path = dir.join("tokens.json");
        let legacy_sidecar = path.with_extension("json.tmp");
        let foreign_sidecar = path.with_extension(format!("json.tmp.{}", u32::MAX));
        fs::write(&path, b"ciphertext-sentinel").unwrap();
        fs::write(&legacy_sidecar, b"{\"leaked\":\"PLAINTEXT\"}").unwrap();
        fs::write(&foreign_sidecar, b"{\"leaked\":\"PLAINTEXT\"}").unwrap();

        clear_tokens_file_at(&path).expect("reset file-sweep must succeed");

        assert!(!path.exists(), "tokens.json must be gone after reset");
        assert!(
            !legacy_sidecar.exists(),
            "the fixed-name plaintext sidecar must be swept"
        );
        #[cfg(target_os = "linux")]
        assert!(
            !foreign_sidecar.exists(),
            "a dead process's sidecar must be swept"
        );
        let _ = fs::remove_dir_all(&dir);
    }
    // Issue #766 (wiring/mutant-killer guard, not a regression proof): this
    // passes pre-fix by design. `reset_tokens_storage` must drop the
    // keychain key BEFORE deleting the tokens file. A failure then leaves at
    // worst an orphan ciphertext file (which `read_tokens_at` treats as
    // "start empty"); the reverse order could leave an undecryptable file
    // with no key to recover from. Keychain-touching behavior itself is not
    // exercised here — the suite never touches the OS keychain — so the
    // ordering is pinned by a source scan of the isolated body.
    #[test]
    fn reset_deletes_the_key_before_the_file() {
        let src = include_str!("token_io.rs");
        let body = test_scan::fn_body(src, "fn reset_tokens_storage(");
        let drop_key = body
            .find("delete_tokens_aes_key")
            .expect("reset_tokens_storage must drop the keychain key");
        let clear_file = body
            .find("clear_tokens_file")
            .expect("reset_tokens_storage must clear the tokens file");
        assert!(
            drop_key < clear_file,
            "the keychain key must be deleted before the tokens file, or a \
             failure leaves an undecryptable file with no recovery path \
             (issue #766)"
        );
    }
}
