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
use rand::TryRngCore;
use std::sync::LazyLock;

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

static CACHE: LazyLock<parking_lot::RwLock<Option<String>>> =
    LazyLock::new(|| parking_lot::RwLock::new(None));

fn cache() -> &'static parking_lot::RwLock<Option<String>> {
    &CACHE
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
            "OS keychain is unavailable: {}. On Linux install/enable a Secret Service provider \
                 (gnome-keyring with headless unlock, kwallet, or a KeePassXC bridge) and log in to a \
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
///
/// `Present`-only projection of [`spotify_client_secret_presence`]: callers
/// that must tell "keychain locked/unavailable" apart from "never
/// configured" have to use the tri-state instead of this bool (issue
/// #560).
pub fn has_spotify_client_secret() -> bool {
    matches!(spotify_client_secret_presence(), KeychainPresence::Present)
}

/// What the OS keychain says about a credential slot (issue #560).
///
/// The three-way split is load-bearing: a locked or unavailable Secret
/// Service is NOT "the user never configured a secret". Collapsing the two
/// made every Linux user with a locked keyring (or without a Secret
/// Service daemon at all) look unconfigured, and pushed them through a full
/// re-onboarding even though their secret was still in the keychain and
/// merely unreadable at that moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeychainPresence {
    /// The credential exists and is readable.
    Present,
    /// No entry for this slot: the normal "onboarding needed" answer.
    Absent,
    /// The OS keychain could not answer — no Secret Service daemon, a
    /// locked keyring, denied storage access. Carries the same
    /// platform-aware, user-actionable help text the read paths return.
    Unavailable(String),
}

/// Classify one raw keychain lookup. `NoEntry` is the only error that means
/// "absent"; every other `keyring::Error` is a platform problem that can
/// recover without the user re-entering anything.
fn classify_keychain_lookup(result: Result<String, keyring::Error>) -> KeychainPresence {
    match result {
        Ok(_) => KeychainPresence::Present,
        Err(keyring::Error::NoEntry) => KeychainPresence::Absent,
        Err(other) => KeychainPresence::Unavailable(
            keychain_error_help(&other).unwrap_or_else(|| {
                format!(
                    "OS keychain error: {}. On Linux see {} for setup help.",
                    other, LINUX_KEYRING_DOC
                )
            }),
        ),
    }
}

/// Combine per-slot classifications for one credential. `Present` wins
/// (either slot counts); otherwise a platform problem outranks `Absent` —
/// never the other way round, or a locked keychain would read as "never
/// configured".
fn combine_presence(probes: impl IntoIterator<Item = KeychainPresence>) -> KeychainPresence {
    let mut unavailable: Option<String> = None;
    for probe in probes {
        match probe {
            KeychainPresence::Present => return KeychainPresence::Present,
            KeychainPresence::Unavailable(help) => {
                unavailable.get_or_insert(help);
            }
            KeychainPresence::Absent => {}
        }
    }
    match unavailable {
        Some(help) => KeychainPresence::Unavailable(help),
        None => KeychainPresence::Absent,
    }
}

/// Tri-state presence of the Spotify `client_secret`, consulting the OS
/// keychain directly (never the in-process cache) exactly like
/// [`has_spotify_client_secret`] — so it reflects a deletion made while the
/// app runs. The namespaced slot is checked first and the legacy
/// unnamespaced slot second, keeping the categories meaningful on both
/// generations of installs (audit M2).
pub fn spotify_client_secret_presence() -> KeychainPresence {
    combine_presence(
        [SPOTIFY_CLIENT_SECRET_USER, SPOTIFY_CLIENT_SECRET_USER_LEGACY]
            .into_iter()
            .map(|user| classify_keychain_lookup(probe_keychain_entry(user))),
    )
}

/// Raw lookup for a single slot. Failing to even build the entry handle is
/// a platform failure, not an absent credential.
fn probe_keychain_entry(user: &str) -> Result<String, keyring::Error> {
    keyring::Entry::new(KEYRING_SERVICE, user)?.get_password()
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

/// Process-wide serialisation of the tokens-key *create* path (issue #563).
///
/// Every write path funnels through [`get_or_create_tokens_aes_key`], and at
/// least a dozen of them can run concurrently (the Teams device-code poll,
/// the Spotify refresh thread, the deep-link callback). On a fresh install
/// two callers that both observe `NoEntry` would each generate 32 random
/// bytes and each call `set_password`; the loser's key wins the keychain
/// while the loser's *ciphertext* is what lands in tokens.json — after which
/// every launch fails GCM authentication and the user is forced through
/// onboarding. Re-reading inside this lock turns the race into "first writer
/// wins, everyone adopts".
static TOKENS_KEY_CREATE_LOCK: LazyLock<parking_lot::Mutex<()>> =
    LazyLock::new(|| parking_lot::Mutex::new(()));

/// Process-wide cache of the tokens AES key, mirroring the client-secret
/// [`CACHE`]. The key is immutable for the life of an install, so the cache
/// cannot go stale except through [`delete_tokens_aes_key`] (the corrupt-key
/// recovery path), which clears it.
static TOKENS_KEY_CACHE: LazyLock<parking_lot::Mutex<Option<[u8; 32]>>> =
    LazyLock::new(|| parking_lot::Mutex::new(None));

fn tokens_key_create_lock() -> &'static parking_lot::Mutex<()> {
    &TOKENS_KEY_CREATE_LOCK
}

fn tokens_key_cache() -> &'static parking_lot::Mutex<Option<[u8; 32]>> {
    &TOKENS_KEY_CACHE
}

fn cached_tokens_aes_key() -> Option<[u8; 32]> {
    *tokens_key_cache().lock()
}

/// Generate a fresh 256-bit key from the OS CSPRNG.
fn generate_tokens_aes_key() -> Result<[u8; 32], String> {
    let mut key = [0u8; 32];
    rand::rngs::OsRng
        .try_fill_bytes(&mut key)
        .map_err(|e| format!("Failed to generate tokens AES key from OS CSPRNG: {}", e))?;
    Ok(key)
}

/// Actionable message for a keychain entry that exists but is not a valid
/// 32-byte AES key — a truncated Secret Service item, a partially written
/// migration, a mis-reporting provider.
///
/// Pre-4.6 this surfaced as a bare decode error with no recovery step and no
/// documentation pointer, so the app could neither decrypt tokens.json nor
/// write a new one and the user had nothing to act on (issue #566). The
/// pointer to the setup doc is attached here, like every other message this
/// module returns.
fn corrupt_tokens_aes_key_help(detail: String) -> String {
    format!(
        "{} The stored tokens encryption key is unusable, so tokens.json cannot be decrypted. \
         Reset the local token storage (delete the keychain entry, then sign in again) to \
         generate a fresh key. See {} for keychain setup help.",
        detail, LINUX_KEYRING_DOC
    )
}

/// Read the tokens.json AES-256-GCM key from the OS keychain.
///
/// The key is stored base64-encoded under
/// `(KEYRING_SERVICE, TOKENS_AES_KEY_USER)`. The in-process cache is
/// consulted first (the key cannot change except via
/// [`delete_tokens_aes_key`]).
///
/// Returns an error when the entry is missing — the caller
/// (`token_io::read_tokens_at`) treats that as a re-auth signal: the
/// ciphertext on disk cannot be decrypted without this key, so the safest
/// recovery is to discard the tokens and re-onboard (same path as a corrupt
/// file). This is a pure read: it never creates the key.
pub fn get_tokens_aes_key() -> Result<[u8; 32], String> {
    if let Some(key) = cached_tokens_aes_key() {
        return Ok(key);
    }
    let entry = map_keychain_err(keyring::Entry::new(KEYRING_SERVICE, TOKENS_AES_KEY_USER))?;
    let b64 = entry.get_password().map_err(|e| match e {
        keyring::Error::NoEntry => {
            "Tokens encryption key not found in OS keychain; cannot decrypt tokens.json (re-authentication required).".to_string()
        }
        other => keychain_error_help(&other).unwrap_or_else(|| {
            format!("Failed to read tokens encryption key from keychain: {}", other)
        }),
    })?;
    let key = decode_tokens_aes_key(&b64).map_err(corrupt_tokens_aes_key_help)?;
    *tokens_key_cache().lock() = Some(key);
    Ok(key)
}

/// Read the tokens.json AES-256-GCM key, generating and storing a fresh
/// random 256-bit key on first use.
///
/// First-use path: no entry exists in the OS keychain → generate 32 random
/// bytes from the OS CSPRNG, store them base64-encoded under
/// `(KEYRING_SERVICE, TOKENS_AES_KEY_USER)`, and return them. This is the
/// entry point used by the token *write* path (and by the
/// plaintext→ciphertext migration), so the key always exists by the time
/// ciphertext is written. A keychain that is unavailable or locked surfaces
/// as an error (with Linux setup help) rather than falling back to weaker
/// storage — silently degrading to a non-keychain key would defeat the point
/// of issue #140.
///
/// Creation is serialised process-wide and double-checked
/// ([`create_or_adopt_tokens_key`], issue #563), and a present-but-corrupt
/// entry is reported through [`corrupt_tokens_aes_key_help`] instead of
/// dead-ending the user (issue #566).
pub fn get_or_create_tokens_aes_key() -> Result<[u8; 32], String> {
    if let Some(key) = cached_tokens_aes_key() {
        return Ok(key);
    }
    let entry = map_keychain_err(keyring::Entry::new(KEYRING_SERVICE, TOKENS_AES_KEY_USER))?;
    let key = create_or_adopt_tokens_key(
        || entry.get_password(),
        |b64| entry.set_password(b64),
        generate_tokens_aes_key,
    )?;
    *tokens_key_cache().lock() = Some(key);
    Ok(key)
}

/// Core of [`get_or_create_tokens_aes_key`]: one locked read-or-create.
///
/// The keychain access is injected so the decision table is unit-testable
/// without an OS keychain.
///
/// * an existing decodable entry is returned as-is;
/// * `NoEntry` generates a key and stores it, then re-reads to verify the
///   store actually landed. A concurrent writer (another process, since this
///   process is serialised by [`tokens_key_create_lock`]) that won the race
///   is *adopted* rather than overwritten — the surviving key is the one the
///   keychain holds, so adopting it is the only choice that keeps the
///   ciphertext written by either party decryptable;
/// * a present-but-undecodable entry is a hard error with a recovery path,
///   never a silent regeneration (the old ciphertext is not ours to
///   discard).
fn create_or_adopt_tokens_key(
    read: impl Fn() -> Result<String, keyring::Error>,
    store: impl Fn(&str) -> Result<(), keyring::Error>,
    generate: impl FnOnce() -> Result<[u8; 32], String>,
) -> Result<[u8; 32], String> {
    let _guard = tokens_key_create_lock().lock();
    match read() {
        Ok(b64) => decode_tokens_aes_key(&b64).map_err(corrupt_tokens_aes_key_help),
        Err(keyring::Error::NoEntry) => {
            let key = generate()?;
            let b64 = STANDARD.encode(key);
            map_keychain_err(store(&b64))?;
            log::info!("[KEYCHAIN] Generated + stored tokens.json AES key in OS keychain");
            // Verify-after-set: a lost write would otherwise leave ciphertext
            // under a key the keychain does not hold.
            match read() {
                Ok(stored) => {
                    let stored_key = decode_tokens_aes_key(&stored)
                        .map_err(corrupt_tokens_aes_key_help)?;
                    if stored_key != key {
                        log::warn!(
                            "[KEYCHAIN] tokens AES key store raced with another writer; adopting the stored key"
                        );
                    }
                    Ok(stored_key)
                }
                Err(e) => {
                    log::warn!(
                        "[KEYCHAIN] tokens AES key verify-after-set read failed: {} (using the key just stored)",
                        e
                    );
                    Ok(key)
                }
            }
        }
        Err(e) => Err(keychain_error_help(&e).unwrap_or_else(|| {
            format!("Failed to read tokens encryption key from keychain: {}", e)
        })),
    }
}

/// Delete the tokens.json AES key from the OS keychain and clear the
/// in-process cache.
///
/// This is the recovery half of issue #566 (a corrupt key entry) and the
/// first step of `token_io::reset_tokens_storage`: after it, the next write
/// generates a fresh key. The ciphertext already on disk becomes
/// permanently unreadable, which is why the caller must also clear the
/// tokens file rather than leaving an undecryptable one behind.
pub fn delete_tokens_aes_key() -> Result<(), String> {
    delete_keychain_entry(TOKENS_AES_KEY_USER)?;
    *tokens_key_cache().lock() = None;
    log::info!("[KEYCHAIN] Deleted tokens.json AES key from keychain (cache cleared)");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A platform failure of the shape keyring reports on Linux when no
    /// Secret Service daemon answers (or the keyring is locked).
    fn platform_failure(msg: &'static str) -> keyring::Error {
        keyring::Error::PlatformFailure(Box::new(std::io::Error::other(msg)))
    }

    /// Issue #560: `NoEntry` is the only error that means "the user never
    /// configured a secret". Everything else is a platform problem that the
    /// UI must present as fixable — collapsing it into `Absent` sent every
    /// Linux user with a locked keyring back through onboarding.
    #[test]
    fn classify_splits_absent_from_unavailable() {
        assert_eq!(
            classify_keychain_lookup(Ok("secret".to_string())),
            KeychainPresence::Present
        );
        assert_eq!(
            classify_keychain_lookup(Err(keyring::Error::NoEntry)),
            KeychainPresence::Absent
        );

        for err in [
            platform_failure("no secret service"),
            keyring::Error::NoStorageAccess(Box::new(std::io::Error::other("locked"))),
        ] {
            match classify_keychain_lookup(Err(err)) {
                KeychainPresence::Unavailable(help) => {
                    assert!(
                        help.contains(LINUX_KEYRING_DOC),
                        "unavailable help must point at the setup doc, got: {help}"
                    );
                }
                other => panic!("platform failure must classify as Unavailable, got {other:?}"),
            }
        }
    }

    /// The two keychain slots (namespaced + legacy) are probed separately, so
    /// the combination rule decides the answer: a hit anywhere is Present; a
    /// platform problem anywhere must never be masked by an `Absent` sibling
    /// slot.
    #[test]
    fn presence_combination_never_hides_a_platform_failure() {
        use KeychainPresence::{Absent, Present, Unavailable};

        let unavailable = || Unavailable("locked".to_string());

        assert_eq!(combine_presence([]), Absent);
        assert_eq!(combine_presence([Absent, Absent]), Absent);
        assert_eq!(combine_presence([Absent, Present]), Present);
        assert_eq!(combine_presence([Absent, unavailable()]), unavailable());
        assert_eq!(combine_presence([unavailable(), Absent]), unavailable());
        // A readable entry outranks a sibling slot's platform failure: the
        // credential IS available, so the UI must not show an error banner.
        assert_eq!(combine_presence([unavailable(), Present]), Present);
    }

    /// Issue #563: no entry → generate, store, and re-read to verify the
    /// store landed. The returned key must be the one the store now holds.
    #[test]
    fn missing_entry_generates_and_verifies_the_store() {
        let store = parking_lot::Mutex::new(None::<String>);
        let store_calls = parking_lot::Mutex::new(0u32);
        let read = || match store.lock().clone() {
            Some(b64) => Ok(b64),
            None => Err(keyring::Error::NoEntry),
        };
        let write = |b64: &str| {
            *store_calls.lock() += 1;
            *store.lock() = Some(b64.to_string());
            Ok(())
        };
        let expected = [7u8; 32];

        let key = create_or_adopt_tokens_key(read, write, || Ok(expected)).unwrap();

        assert_eq!(key, expected);
        assert_eq!(*store_calls.lock(), 1, "one create must store once");
        assert_eq!(
            STANDARD.encode(expected),
            store.lock().clone().unwrap(),
            "the keychain must hold the generated key, base64-encoded"
        );
    }

    /// Issue #563: the create path is serialised and double-checked. A
    /// concurrent creator that already stored a key (or one that won the
    /// cross-process race) must be adopted, not overwritten — overwriting it
    /// is what would leave tokens.json encrypted under a key the keychain no
    /// longer holds.
    #[test]
    fn existing_key_is_adopted_without_regenerating() {
        let existing = [0x11u8; 32];
        let stored = STANDARD.encode(existing);
        let store_calls = parking_lot::Mutex::new(0u32);

        let key = create_or_adopt_tokens_key(
            || Ok(stored.clone()),
            |_| {
                *store_calls.lock() += 1;
                Ok(())
            },
            || panic!("an existing key must never be regenerated"),
        )
        .unwrap();

        assert_eq!(key, existing);
        assert_eq!(*store_calls.lock(), 0);
    }

    /// Issue #563: two callers whose generator ran before either store have
    /// to converge on a single key. With the create lock held across the
    /// read→generate→store→verify sequence, the second caller re-reads and
    /// adopts; without it, each would install its own key and the loser's
    /// ciphertext would be undecryptable on the next launch.
    #[test]
    fn concurrent_creators_converge_on_one_key() {
        let store = parking_lot::Mutex::new(None::<String>);
        let generated = std::sync::atomic::AtomicUsize::new(0);

        let keys: Vec<[u8; 32]> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8)
                .map(|_| {
                    scope.spawn(|| {
                        let read = || match store.lock().clone() {
                            Some(b64) => Ok(b64),
                            None => Err(keyring::Error::NoEntry),
                        };
                        let write = |b64: &str| {
                            // Last write wins, like a real keychain slot.
                            *store.lock() = Some(b64.to_string());
                            Ok(())
                        };
                        let generate = || {
                            let n = generated.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                            Ok([n as u8; 32])
                        };
                        create_or_adopt_tokens_key(read, write, generate).unwrap()
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().expect("creator thread must not panic"))
                .collect()
        });

        let stored = STANDARD.decode(store.lock().clone().unwrap()).unwrap();
        let stored_key: [u8; 32] = stored.try_into().unwrap();
        for (i, key) in keys.iter().enumerate() {
            assert_eq!(
                *key, stored_key,
                "creator #{i} returned a key the keychain does not hold"
            );
        }
    }

    /// Issue #566: a present-but-undecodable entry must not be silently
    /// regenerated (the ciphertext on disk is not ours to discard), and the
    /// error must name a recovery step instead of dead-ending the user.
    #[test]
    fn corrupt_entry_reports_an_actionable_recovery_error() {
        for corrupt in ["not base64!!", &STANDARD.encode([0u8; 8])] {
            let err = create_or_adopt_tokens_key(
                || Ok(corrupt.to_string()),
                |_| panic!("a corrupt entry must never be overwritten"),
                || panic!("a corrupt entry must never be regenerated"),
            )
            .expect_err("a corrupt key entry must be a hard error");
            assert!(
                err.contains(LINUX_KEYRING_DOC),
                "corrupt-key error must carry a recovery pointer, got: {err}"
            );
        }
    }
}
