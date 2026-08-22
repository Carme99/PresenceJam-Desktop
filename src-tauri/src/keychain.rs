//! OS keychain integration for storing OAuth secrets.
//!
//! Wraps the `keyring` crate to provide a stable, minimal API for storing
//! the Spotify `client_secret` in the platform's secure credential store
//! (Windows Credential Manager, macOS Keychain, Linux Secret Service).
//!
//! Migration note: prior to this module, the secret was persisted in plain
//! text inside `config.json`. The first run
//! after this change will need the user to re-enter the secret via
//! Onboarding. See issue #9.
//!
//! Caching: the secret is held in a process-wide `RwLock<Option<String>>`
//! after the first read, so the polling thread (which calls
//! `peek_spotify_client_secret` on every 30s iteration) does not hit the
//! OS keychain on the happy path. Issue #69.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use rand::RngCore;
use std::sync::OnceLock;

const KEYRING_SERVICE: &str = "presencejam";

/// Primary keychain user field. Namespaced by the Tauri bundle identifier
/// (`tauri.conf.json` → `identifier`, currently `com.presencejam.app`)
/// so side-by-side installs on the same OS user — prod, dev build,
/// beta channel — get isolated slots. A test build's onboarding no
/// longer silently overwrites the prod install's secret or vice-versa.
/// See audit M2.
const SPOTIFY_CLIENT_SECRET_USER: &str = "spotify_client_secret:com.presencejam.app";

/// Legacy unnamespaced key used through v2.7.2. New writes go to
/// [`SPOTIFY_CLIENT_SECRET_USER`]; reads fall back to this constant on
/// miss and migrate the value forward (write to the namespaced slot,
/// delete the legacy slot) so existing v2.7.2 users don't have to
/// re-onboard after upgrading.
const SPOTIFY_CLIENT_SECRET_USER_LEGACY: &str = "spotify_client_secret";

static CACHE: OnceLock<parking_lot::RwLock<Option<String>>> = OnceLock::new();

fn cache() -> &'static parking_lot::RwLock<Option<String>> {
    CACHE.get_or_init(|| parking_lot::RwLock::new(None))
}

/// DOC ANCHOR — referenced from keychain error messages. Bump the
/// anchor if SETUP.md is restructured. See audit Q7.
const LINUX_KEYRING_DOC: &str = "SETUP.md#linux-keyring";

/// Map a `keyring::Error` to a user-actionable error message when it
/// indicates the OS keychain is unavailable or inaccessible. Returns
/// `Some(help)` when the error is "no keychain" / "keychain locked";
/// returns `None` for `NoEntry` (a missing credential is a normal
/// onboarding flow, not a platform problem).
///
/// On Linux, the most common failure modes — no Secret Service daemon
/// running, locked `gnome-keyring`, missing `kwallet` — surface as
/// `PlatformFailure` or `NoStorageAccess` wrapping a platform-specific
/// inner error. We match both broadly and point the user at SETUP.md
/// instead of trying to distinguish "no Secret Service" from "locked
/// keychain" (the inner-error text varies across keyring-crate and
/// platform versions). See audit Q7.
fn keychain_error_help(err: &keyring::Error) -> Option<String> {
    match err {
        keyring::Error::NoEntry => None,
        keyring::Error::PlatformFailure(_) | keyring::Error::NoStorageAccess(_) => Some(format!(
            "OS keychain is unavailable: {}. On Linux install/enable a system \
                 keyring (gnome-keyring, kwallet, or systemd-creds) and log in to a \
                 graphical session; see {}.",
            err, LINUX_KEYRING_DOC
        )),
        _ => Some(format!(
            "OS keychain error: {}. On Linux see {} for setup help.",
            err, LINUX_KEYRING_DOC
        )),
    }
}

/// Wrap a `Result<T, keyring::Error>` with a platform-aware help
/// message when the error is "keychain unavailable". Pass-through the
/// success value unchanged. Used by every keychain function that can
/// fail at the OS layer. See audit Q7.
fn map_keychain_err<T>(result: Result<T, keyring::Error>) -> Result<T, String> {
    result.map_err(|e| keychain_error_help(&e).unwrap_or_else(|| format!("{}", e)))
}

/// Persist the Spotify `client_secret` in the OS keychain.
///
/// Overwrites any existing entry for `(KEYRING_SERVICE, SPOTIFY_CLIENT_SECRET_USER)`
/// and updates the in-process cache.
pub fn store_spotify_client_secret(secret: &str) -> Result<(), String> {
    let entry = map_keychain_err(keyring::Entry::new(
        KEYRING_SERVICE,
        SPOTIFY_CLIENT_SECRET_USER,
    ))?;
    map_keychain_err(entry.set_password(secret))?;
    *cache().write() = Some(secret.to_string());
    log::info!("[KEYCHAIN] Stored Spotify client_secret in OS keychain (cache updated)");
    Ok(())
}

/// Read the Spotify `client_secret`. Fast path: returns the in-process
/// cache without touching the keychain. Slow path: reads from the OS
/// keychain and populates the cache for subsequent calls.
///
/// Returns an error if the entry is missing — the caller should treat
/// this as a "user must re-onboard" signal, not a fatal error.
pub fn get_spotify_client_secret() -> Result<String, String> {
    // Fast path: cache hit
    {
        let r = cache().read();
        if let Some(s) = r.as_ref() {
            return Ok(s.clone());
        }
    }
    // Slow path: read from OS keychain (namespaced slot — see audit M2).
    let entry = map_keychain_err(keyring::Entry::new(
        KEYRING_SERVICE,
        SPOTIFY_CLIENT_SECRET_USER,
    ))?;
    let secret = match entry.get_password() {
        Ok(s) => s,
        Err(keyring::Error::NoEntry) => {
            // Legacy fallback for v2.7.2 and earlier users who onboarded
            // under the unnamespaced key. Read the legacy slot, write it
            // forward to the namespaced slot, delete the legacy slot, and
            // return the value. Best-effort migration: if the
            // forward-write or legacy-delete fails, still return the
            // legacy secret so the caller isn't blocked. See audit M2.
            let legacy_entry = match keyring::Entry::new(
                KEYRING_SERVICE,
                SPOTIFY_CLIENT_SECRET_USER_LEGACY,
            ) {
                Ok(e) => e,
                Err(_) => {
                    return Err(
                        "Spotify client secret not found in keychain. Please re-enter via Onboarding."
                            .to_string(),
                    );
                }
            };
            let legacy_secret = match legacy_entry.get_password() {
                Ok(s) => s,
                Err(_) => {
                    return Err(
                        "Spotify client secret not found in keychain. Please re-enter via Onboarding."
                            .to_string(),
                    );
                }
            };
            if let Ok(forward_entry) =
                keyring::Entry::new(KEYRING_SERVICE, SPOTIFY_CLIENT_SECRET_USER)
            {
                if let Err(e) = forward_entry.set_password(&legacy_secret) {
                    log::warn!(
                        "[KEYCHAIN] legacy→namespaced forward-write failed: {} (continuing with legacy value)",
                        e
                    );
                } else {
                    let _ = legacy_entry.delete_credential();
                    log::info!(
                        "[KEYCHAIN] migrated legacy spotify_client_secret to namespaced slot"
                    );
                }
            }
            legacy_secret
        }
        Err(e) => {
            return Err(keychain_error_help(&e).unwrap_or_else(|| {
                format!("Failed to read Spotify client secret from keychain: {}", e)
            }));
        }
    };
    // Populate cache for next call
    *cache().write() = Some(secret.clone());
    log::info!("[KEYCHAIN] Loaded Spotify client_secret from OS keychain (cache populated)");
    Ok(secret)
}

/// Read the Spotify `client_secret` from the cache only — no OS keychain
/// call. Returns `None` if the cache is cold. Used by the polling thread
/// to avoid the keychain prompt on every iteration. See issue #69.
pub fn peek_spotify_client_secret() -> Option<String> {
    cache().read().clone()
}

/// Check whether the Spotify `client_secret` is present in the OS keychain.
///
/// This consults the keychain directly and does not use the in-process
/// cache, so it reflects the current keychain state even if the entry
/// was deleted while the app is running (e.g. via the macOS Keychain
/// Access app, the Windows Credential Manager UI, or `secret-tool` on
/// Linux). Called from `is_spotify_client_secret_set` (user-action
/// gated) and from `config::with_keychain_flags` (called only on
/// config load), both of which are off the polling hot path.
///
/// Checks both the namespaced slot (current installs) and the legacy
/// unnamespaced slot (v2.7.2 and earlier installs that haven't yet
/// triggered the read-time migration). See audit M2.
pub fn has_spotify_client_secret() -> bool {
    has_keychain_entry(SPOTIFY_CLIENT_SECRET_USER)
        || has_keychain_entry(SPOTIFY_CLIENT_SECRET_USER_LEGACY)
}

fn has_keychain_entry(user: &str) -> bool {
    match keyring::Entry::new(KEYRING_SERVICE, user) {
        Ok(entry) => entry.get_password().is_ok(),
        Err(_) => false,
    }
}

/// Remove the Spotify `client_secret` from the OS keychain (both the
/// current namespaced slot and the legacy unnamespaced slot used
/// through v2.7.2) and clear the in-process cache. Called on user
/// disconnect / reconnect to wipe the secret. The legacy-slot delete
/// is best-effort: a missing legacy entry is not an error. See audit
/// M2.
pub fn delete_spotify_client_secret() -> Result<(), String> {
    delete_keychain_entry(SPOTIFY_CLIENT_SECRET_USER)?;
    if let Err(e) = delete_keychain_entry(SPOTIFY_CLIENT_SECRET_USER_LEGACY) {
        log::warn!(
            "[KEYCHAIN] legacy spotify_client_secret delete failed: {} (continuing)",
            e
        );
    }
    *cache().write() = None;
    log::info!("[KEYCHAIN] Deleted Spotify client_secret from keychain (cache cleared)");
    Ok(())
}

/// Delete a single keychain entry. Returns Ok(()) if the entry was
/// deleted or didn't exist; surfaces other keyring errors.
fn delete_keychain_entry(user: &str) -> Result<(), String> {
    let entry = map_keychain_err(keyring::Entry::new(KEYRING_SERVICE, user))?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(keychain_error_help(&e)
            .unwrap_or_else(|| format!("Failed to delete keychain entry '{}': {}", user, e))),
    }
}

/// Keychain slot for the tokens.json AES-256-GCM encryption key (issue
/// #140). Namespaced by the Tauri bundle identifier exactly like
/// [`SPOTIFY_CLIENT_SECRET_USER`], so side-by-side installs (prod, dev,
/// beta) get isolated keys and a test build's first persist can never
/// overwrite the prod install's key (which would make the prod
/// ciphertext undecryptable).
const TOKENS_AES_KEY_USER: &str = "tokens_aes_key:com.presencejam.app";

/// Decode a stored tokens AES key from its base64 (STANDARD) form. The
/// keyring crate stores passwords as strings, so the 32 raw key bytes
/// are base64-encoded at rest. Rejects any value that is not exactly
/// 256 bits — a truncated/corrupted entry must not silently produce a
/// weaker key.
fn decode_tokens_aes_key(b64: &str) -> Result<[u8; 32], String> {
    let bytes = STANDARD
        .decode(b64.trim())
        .map_err(|e| format!("Stored tokens AES key is not valid base64: {}", e))?;
    <[u8; 32]>::try_from(bytes.as_slice()).map_err(|_| {
        format!(
            "Stored tokens AES key has wrong length ({} bytes, expected 32)",
            bytes.len()
        )
    })
}

/// Read the tokens.json AES-256-GCM key from the OS keychain.
///
/// The key is stored base64-encoded under
/// `(KEYRING_SERVICE, TOKENS_AES_KEY_USER)`.
///
/// Returns an error when the entry is missing — the caller
/// (`token_io::read_tokens_at`) treats that as a re-auth signal: the
/// ciphertext on disk cannot be decrypted without this key, so the
/// safest recovery is to discard the tokens and re-onboard (same path
/// as a corrupt file). This is a pure read: it never creates the key.
pub fn get_tokens_aes_key() -> Result<[u8; 32], String> {
    let entry = map_keychain_err(keyring::Entry::new(KEYRING_SERVICE, TOKENS_AES_KEY_USER))?;
    let b64 = entry.get_password().map_err(|e| match e {
        keyring::Error::NoEntry => {
            "Tokens encryption key not found in OS keychain; cannot decrypt tokens.json (re-authentication required).".to_string()
        }
        other => keychain_error_help(&other).unwrap_or_else(|| {
            format!("Failed to read tokens encryption key from keychain: {}", other)
        }),
    })?;
    decode_tokens_aes_key(&b64)
}

/// Read the tokens.json AES-256-GCM key, generating and storing a fresh
/// random 256-bit key on first use.
///
/// First-use path: no entry exists in the OS keychain → generate 32
/// random bytes from the OS CSPRNG, store them base64-encoded under
/// `(KEYRING_SERVICE, TOKENS_AES_KEY_USER)`, and return them. This is
/// the entry point used by the token *write* path (and by the
/// plaintext→ciphertext migration), so the key always exists by the
/// time ciphertext is written. A keychain that is unavailable or locked
/// surfaces as an error (with Linux setup help) rather than falling
/// back to weaker storage — silently degrading to a non-keychain key
/// would defeat the point of issue #140.
pub fn get_or_create_tokens_aes_key() -> Result<[u8; 32], String> {
    let entry = map_keychain_err(keyring::Entry::new(KEYRING_SERVICE, TOKENS_AES_KEY_USER))?;
    match entry.get_password() {
        Ok(b64) => decode_tokens_aes_key(&b64),
        Err(keyring::Error::NoEntry) => {
            let mut key = [0u8; 32];
            rand::rngs::OsRng.fill_bytes(&mut key);
            let b64 = STANDARD.encode(key);
            map_keychain_err(entry.set_password(&b64))?;
            log::info!("[KEYCHAIN] Generated + stored tokens.json AES key in OS keychain");
            Ok(key)
        }
        Err(e) => Err(keychain_error_help(&e).unwrap_or_else(|| {
            format!("Failed to read tokens encryption key from keychain: {}", e)
        })),
    }
}
