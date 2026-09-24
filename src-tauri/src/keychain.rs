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
//! Caching: the reusable secret value and the short-lived presence
//! observation are held independently in process-wide locks after the first
//! read, so polling and normal config loads avoid another OS keychain call on
//! the happy path. Issue #69/#881.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use rand::TryRngCore;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

const KEYRING_SERVICE: &str = "presencejam";

/// Primary keychain user field. Namespaced by the Tauri bundle identifier
/// (`tauri.conf.json` → `identifier`, currently `com.presencejam.app`)
/// so side-by-side installs on the same OS user — prod, dev build,
/// beta channel — get isolated slots. A test build's onboarding no
/// longer silently overwrites the prod install's secret or vice-versa.
/// See audit M2.
const SPOTIFY_CLIENT_SECRET_USER: &str = "spotify_client_secret:com.presencejam.app";

/// Keychain user field for the `--serve` localhost API bearer token
/// (issue #865). 32 random bytes, base64url-encoded; required as
/// `Authorization: Bearer <token>` on every mutating route. Namespaced
/// the same way as the Spotify secret so dev builds and prod builds
/// cannot authenticate against each other's running daemon.
const SERVE_TOKEN_USER: &str = "serve_token:com.presencejam.app";

/// Legacy unnamespaced key used through v2.7.2. New writes go to
/// [`SPOTIFY_CLIENT_SECRET_USER`]; reads fall back to this constant on
/// miss and migrate the value forward (write to the namespaced slot,
/// delete the legacy slot) so existing v2.7.2 users don't have to
/// re-onboard after upgrading.
const SPOTIFY_CLIENT_SECRET_USER_LEGACY: &str = "spotify_client_secret";

/// How long a successful platform presence probe may satisfy config loads.
/// Explicit secret reads remain cache-backed independently; this TTL only
/// bounds how long a config load can avoid revalidating an externally changed
/// OS keychain entry.
const SPOTIFY_CLIENT_SECRET_PRESENCE_TTL: Duration = Duration::from_secs(30);

struct CachedSpotifyClientPresence {
    presence: KeychainPresence,
    refreshed_at: Instant,
}

#[derive(Default)]
struct SpotifyClientSecretCache {
    /// Reusable value cache used by explicit secret reads. A presence-only
    /// probe must never populate this: a legacy value would otherwise bypass
    /// its forward migration on the next read.
    secret: parking_lot::RwLock<Option<String>>,
    /// Presence metadata used only by config loads. It deliberately does not
    /// carry a secret value.
    presence: parking_lot::RwLock<Option<CachedSpotifyClientPresence>>,
    /// Cache-commit epoch. Every successful cache commit advances it, including
    /// explicit mutations, successful reads, and presence observations. A
    /// probe captures this before entering the OS and may commit only if no
    /// competing cache commit happened while it was in flight.
    generation: AtomicU64,
    /// Serialises the generation check with the cache write, closing the
    /// check-then-store race between a delayed probe and an explicit change.
    commit: parking_lot::Mutex<()>,
}

impl SpotifyClientSecretCache {
    fn peek(&self) -> Option<String> {
        self.secret.read().clone()
    }

    fn store(&self, secret: &str, presence_refreshed_at: Instant) {
        let _commit = self.commit.lock();
        *self.secret.write() = Some(secret.to_string());
        *self.presence.write() = Some(CachedSpotifyClientPresence {
            presence: KeychainPresence::Present,
            refreshed_at: presence_refreshed_at,
        });
        self.generation.fetch_add(1, Ordering::AcqRel);
    }

    fn clear(&self) {
        let _commit = self.commit.lock();
        *self.secret.write() = None;
        *self.presence.write() = None;
        self.generation.fetch_add(1, Ordering::AcqRel);
    }

    fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    fn store_if_current(
        &self,
        observed_generation: u64,
        secret: &str,
        presence_refreshed_at: Instant,
    ) -> bool {
        let _commit = self.commit.lock();
        if self.generation.load(Ordering::Acquire) != observed_generation {
            return false;
        }
        *self.secret.write() = Some(secret.to_string());
        *self.presence.write() = Some(CachedSpotifyClientPresence {
            presence: KeychainPresence::Present,
            refreshed_at: presence_refreshed_at,
        });
        self.generation.fetch_add(1, Ordering::AcqRel);
        true
    }

    fn record_presence_if_current(
        &self,
        observed_generation: u64,
        presence: KeychainPresence,
        refreshed_at: Instant,
    ) -> bool {
        let _commit = self.commit.lock();
        if self.generation.load(Ordering::Acquire) != observed_generation {
            return false;
        }
        match presence {
            KeychainPresence::Present => {
                *self.presence.write() = Some(CachedSpotifyClientPresence {
                    presence: KeychainPresence::Present,
                    refreshed_at,
                });
            }
            KeychainPresence::Absent => {
                *self.secret.write() = None;
                *self.presence.write() = None;
            }
            KeychainPresence::Unavailable(help) => {
                // An unavailable platform is not proof of deletion. Keep the
                // reusable secret, but do not let a stale present observation
                // satisfy a later config load.
                *self.presence.write() = Some(CachedSpotifyClientPresence {
                    presence: KeychainPresence::Unavailable(help),
                    refreshed_at,
                });
            }
        }
        self.generation.fetch_add(1, Ordering::AcqRel);
        true
    }

    fn fresh_presence(&self, now: Instant) -> Option<KeychainPresence> {
        self.presence.read().as_ref().and_then(|cached| {
            (matches!(&cached.presence, KeychainPresence::Present)
                && now.saturating_duration_since(cached.refreshed_at)
                    < SPOTIFY_CLIENT_SECRET_PRESENCE_TTL)
                .then(|| cached.presence.clone())
        })
    }

    fn current_presence(&self) -> Option<KeychainPresence> {
        self.presence
            .read()
            .as_ref()
            .map(|cached| cached.presence.clone())
    }
}

static CACHE: LazyLock<SpotifyClientSecretCache> = LazyLock::new(SpotifyClientSecretCache::default);

fn cache() -> &'static SpotifyClientSecretCache {
    &CACHE
}

/// Serialises keychain reads that may forward-migrate a legacy slot with
/// explicit store/delete operations. Presence probes intentionally do not take
/// this lock: their result is guarded by the cache generation, so they can
/// overlap an explicit mutation without publishing a stale value.
static SPOTIFY_CLIENT_SECRET_MUTATION_LOCK: LazyLock<parking_lot::Mutex<()>> =
    LazyLock::new(|| parking_lot::Mutex::new(()));

fn spotify_client_secret_mutation_lock() -> &'static parking_lot::Mutex<()> {
    &SPOTIFY_CLIENT_SECRET_MUTATION_LOCK
}

#[cfg(test)]
type TestSpotifyProbe =
    std::sync::Arc<dyn Fn(&str) -> Result<String, keyring::Error> + Send + Sync>;
#[cfg(test)]
type TestSpotifyStore = std::sync::Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;
#[cfg(test)]
type TestSpotifyOperation = std::sync::Arc<dyn Fn() -> Result<(), String> + Send + Sync>;
#[cfg(test)]
type TestSpotifyMigration = std::sync::Arc<dyn Fn(&str) + Send + Sync>;

#[cfg(test)]
#[derive(Clone)]
struct TestSpotifyClientSecretBackend {
    probe: TestSpotifyProbe,
    read: TestSpotifyProbe,
    store_namespaced: TestSpotifyStore,
    drop_legacy: TestSpotifyOperation,
    delete_namespaced: TestSpotifyOperation,
    migrate_legacy: TestSpotifyMigration,
}

#[cfg(test)]
static TEST_SPOTIFY_CLIENT_SECRET_BACKEND: LazyLock<
    parking_lot::RwLock<Option<TestSpotifyClientSecretBackend>>,
> = LazyLock::new(|| parking_lot::RwLock::new(None));

#[cfg(test)]
static TEST_SPOTIFY_CLIENT_SECRET_BACKEND_LOCK: LazyLock<parking_lot::Mutex<()>> =
    LazyLock::new(|| parking_lot::Mutex::new(()));
#[cfg(test)]
fn test_spotify_client_secret_backend() -> Option<TestSpotifyClientSecretBackend> {
    TEST_SPOTIFY_CLIENT_SECRET_BACKEND.read().clone()
}

#[cfg(test)]
struct TestSpotifyBackendOverride;

#[cfg(test)]
impl Drop for TestSpotifyBackendOverride {
    fn drop(&mut self) {
        cache().clear();
        *TEST_SPOTIFY_CLIENT_SECRET_BACKEND.write() = None;
    }
}

#[cfg(test)]
fn with_test_spotify_client_secret_backend<T>(
    backend: TestSpotifyClientSecretBackend,
    test: impl FnOnce() -> T,
) -> T {
    let _serialized = TEST_SPOTIFY_CLIENT_SECRET_BACKEND_LOCK.lock();
    cache().clear();
    *TEST_SPOTIFY_CLIENT_SECRET_BACKEND.write() = Some(backend);
    let _override = TestSpotifyBackendOverride;
    test()
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
/// and updates the in-process cache. The legacy unnamespaced slot used through
/// v2.7.2 is deleted best-effort afterwards: this is the one place that knows a
/// new value has superseded the old one, so a rotated secret must not stay
/// retrievable under the previous name (issue #917). See audit M2.
pub fn store_spotify_client_secret(secret: &str) -> Result<(), String> {
    store_spotify_client_secret_platform(secret)?;
    log::info!("[KEYCHAIN] Stored Spotify client_secret in OS keychain (cache updated)");
    Ok(())
}

fn store_spotify_client_secret_platform(secret: &str) -> Result<(), String> {
    #[cfg(test)]
    if let Some(backend) = test_spotify_client_secret_backend() {
        return store_spotify_client_secret_with_cache(
            cache(),
            secret,
            {
                let backend = backend.clone();
                move |value| (backend.store_namespaced)(value)
            },
            {
                let backend = backend.clone();
                move || (backend.drop_legacy)()
            },
        );
    }

    store_spotify_client_secret_with_cache(
        cache(),
        secret,
        |s: &str| -> Result<(), String> {
            let entry = map_keychain_err(keyring::Entry::new(
                KEYRING_SERVICE,
                SPOTIFY_CLIENT_SECRET_USER,
            ))?;
            map_keychain_err(entry.set_password(s))
        },
        || delete_keychain_entry(SPOTIFY_CLIENT_SECRET_USER_LEGACY),
    )
}

/// Core of [`store_spotify_client_secret`] with the cache and keychain
/// operations injected, so the mutation's ordering is deterministic in tests.
/// The cache update is deliberately part of this core: production callers and
/// the mutation tests therefore exercise the same generation bump.
fn store_spotify_client_secret_with_cache(
    cache: &SpotifyClientSecretCache,
    secret: &str,
    store_namespaced: impl FnOnce(&str) -> Result<(), String>,
    drop_legacy: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let _mutation = spotify_client_secret_mutation_lock().lock();
    store_spotify_client_secret_with(secret, store_namespaced, drop_legacy)?;
    // A successful store also starts a fresh presence window for config loads.
    cache.store(secret, Instant::now());
    Ok(())
}

/// Core of [`store_spotify_client_secret`] with the keychain operations
/// injected, so the store-supersedes-legacy ordering is unit-testable without
/// an OS keychain (issue #917).
///
/// `drop_legacy` is best-effort, mirroring [`delete_spotify_client_secret`]: the
/// credential the user just saved is already in the namespaced slot, so a failed
/// (or absent) legacy delete must not fail the store.
fn store_spotify_client_secret_with(
    secret: &str,
    store_namespaced: impl FnOnce(&str) -> Result<(), String>,
    drop_legacy: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    store_namespaced(secret)?;
    if let Err(e) = drop_legacy() {
        log::warn!(
            "[KEYCHAIN] legacy spotify_client_secret delete after store failed: {} (continuing)",
            e
        );
    }
    Ok(())
}

/// Read the Spotify `client_secret`. Fast path: returns the in-process
/// cache without touching the keychain. Slow path: reads from the OS
/// keychain and populates the cache for subsequent calls.
///
/// String-presenting wrapper over [`read_spotify_client_secret`], for the
/// callers that only need the user-facing text. Both messages are unchanged
/// from before #801.
pub fn get_spotify_client_secret() -> Result<String, String> {
    read_spotify_client_secret().map_err(|e| e.into_message(SPOTIFY_CLIENT_SECRET_NOT_FOUND_MSG))
}

/// Read the Spotify `client_secret` with a typed failure (issues #760/#801).
///
/// Fast path: the in-process cache. Slow path: the OS keychain, with the
/// legacy v2.7.2 unnamespaced slot as a fallback that forwards the value to the
/// namespaced slot and deletes the legacy one (audit M2).
///
/// The failure category is load-bearing: [`KeychainReadError::Absent`] is the
/// only outcome that means "the user never configured a secret". A locked
/// vault, a denied access prompt or an unreadable item arrives as
/// [`KeychainReadError::Unavailable`], so callers that start onboarding on an
/// absent secret cannot make the user re-enter a working credential (and
/// overwrite it) just because the keychain was briefly unreadable (issue #801).
pub fn read_spotify_client_secret() -> Result<String, KeychainReadError> {
    let _mutation = spotify_client_secret_mutation_lock().lock();
    let (result, cache_populated) = read_spotify_client_secret_platform();
    if cache_populated {
        log::info!("[KEYCHAIN] Loaded Spotify client_secret from OS keychain (cache populated)");
    }
    result
}

fn read_spotify_client_secret_platform() -> (Result<String, KeychainReadError>, bool) {
    #[cfg(test)]
    if let Some(backend) = test_spotify_client_secret_backend() {
        return read_spotify_client_secret_with_outcome(
            cache(),
            || {
                lookup_spotify_client_secret({
                    let backend = backend.clone();
                    move |user| (backend.read)(user)
                })
            },
            {
                let backend = backend.clone();
                move |secret| (backend.migrate_legacy)(secret)
            },
        );
    }

    read_spotify_client_secret_with_outcome(
        cache(),
        || lookup_spotify_client_secret(probe_keychain_entry),
        forward_migrate_legacy_secret,
    )
}

/// Core of [`read_spotify_client_secret`] with the lookup and migration
/// operations injected. A presence-only probe intentionally does not enter
/// this path: it cannot prime the reusable secret cache.
fn read_spotify_client_secret_with_outcome(
    cache: &SpotifyClientSecretCache,
    lookup: impl FnOnce() -> Result<(String, SecretSlot), KeychainReadError>,
    migrate_legacy: impl FnOnce(&str),
) -> (Result<String, KeychainReadError>, bool) {
    if let Some(cached) = cache.peek() {
        return (Ok(cached), false);
    }
    let observed_generation = cache.generation();
    let (secret, slot) = match lookup() {
        Ok(secret) => secret,
        Err(error) => return (Err(error), false),
    };
    if slot == SecretSlot::Legacy {
        // Best-effort forward migration: the caller already holds the value, so
        // a failed migration must not block it (audit M2).
        migrate_legacy(&secret);
    }
    // A probe/read that began before an explicit mutation must not republish
    // the old value after that mutation has committed.
    let cache_populated = cache.store_if_current(observed_generation, &secret, Instant::now());
    (Ok(secret), cache_populated)
}

/// Which keychain slot a `client_secret` read resolved to (audit M2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SecretSlot {
    /// The namespaced slot current installs write to.
    Namespaced,
    /// The unnamespaced slot used through v2.7.2. The value is still returned,
    /// but it must be forwarded to the namespaced slot.
    Legacy,
}

/// Core of [`read_spotify_client_secret`] with the slot lookup injected, so the
/// failure classification is unit-testable without an OS keychain (issue #801).
///
/// The legacy fallback mirrors the pre-#801 read: the namespaced slot is tried
/// first and, only on `NoEntry`, the unnamespaced one. A failure on *either*
/// slot is classified — `NoEntry` on both means the secret was never
/// configured, while a locked vault, a denied access prompt or an unreadable
/// item on the legacy slot is the same platform problem it is on the namespaced
/// slot, and must not be reported as "please re-enter".
fn lookup_spotify_client_secret(
    probe: impl Fn(&str) -> Result<String, keyring::Error>,
) -> Result<(String, SecretSlot), KeychainReadError> {
    match probe(SPOTIFY_CLIENT_SECRET_USER) {
        Ok(secret) => Ok((secret, SecretSlot::Namespaced)),
        Err(keyring::Error::NoEntry) => {
            let legacy_secret =
                probe(SPOTIFY_CLIENT_SECRET_USER_LEGACY).map_err(classify_read_failure)?;
            Ok((legacy_secret, SecretSlot::Legacy))
        }
        Err(e) => Err(classify_read_failure(e)),
    }
}

/// Forward-migrate a secret read from the legacy unnamespaced slot: write it to
/// the namespaced slot and, on success, delete the legacy slot.
///
/// Entirely best-effort — the caller already holds the value and must not be
/// blocked by a failed migration. See audit M2.
fn forward_migrate_legacy_secret(secret: &str) {
    let Ok(forward_entry) = keyring::Entry::new(KEYRING_SERVICE, SPOTIFY_CLIENT_SECRET_USER) else {
        log::warn!(
            "[KEYCHAIN] could not open the namespaced slot for the legacy→namespaced \
             migration (continuing with the legacy value)"
        );
        return;
    };
    if let Err(e) = forward_entry.set_password(secret) {
        log::warn!(
            "[KEYCHAIN] legacy→namespaced forward-write failed: {} (continuing with legacy value)",
            e
        );
        return;
    }
    if let Err(e) = delete_keychain_entry(SPOTIFY_CLIENT_SECRET_USER_LEGACY) {
        log::warn!(
            "[KEYCHAIN] legacy spotify_client_secret delete after migration failed: {} (continuing)",
            e
        );
    }
    log::info!("[KEYCHAIN] migrated legacy spotify_client_secret to namespaced slot");
}

/// Read the Spotify `client_secret` from the cache only — no OS keychain
/// call. Returns `None` if the cache is cold. Used by the polling thread
/// to avoid the keychain prompt on every iteration. See issue #69.
pub fn peek_spotify_client_secret() -> Option<String> {
    cache().peek()
}

/// Check whether the Spotify `client_secret` is present in the OS keychain.
///
/// This consults the keychain directly and does not reuse the in-process
/// presence observation, so it reflects the current keychain state even if
/// the entry was deleted while the app is running. A successful direct probe
/// refreshes the warm config-load cache; an unavailable result deliberately
/// leaves any cached secret intact for explicit reads.
/// Called from `is_spotify_client_secret_set` (user-action gated).
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
        Err(other) => {
            KeychainPresence::Unavailable(keychain_error_help(&other).unwrap_or_else(|| {
                format!(
                    "OS keychain error: {}. On Linux see {} for setup help.",
                    other, LINUX_KEYRING_DOC
                )
            }))
        }
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

/// Typed outcome of a keychain read (issues #760/#801/#935).
///
/// Exists so callers can tell three situations apart that a single `String`
/// error collapses into one: the credential was never configured, the OS
/// keychain could not answer at that moment (locked vault, no Secret Service
/// daemon, a denied access prompt), and an entry is present but unusable. Only
/// the first is a reason to ask the user to re-enter anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeychainReadError {
    /// No entry in any slot the credential may legitimately live in — the user
    /// has never configured it, or explicitly removed it.
    Absent,
    /// The OS keychain could not answer. Recoverable without the user
    /// re-entering the credential; the payload is the actionable help text from
    /// [`keychain_error_help`].
    Unavailable(String),
    /// An entry exists but its value cannot be used (e.g. a truncated or
    /// undecodable stored tokens key). The payload carries the recovery text.
    Corrupt(String),
}

impl KeychainReadError {
    /// Collapse to the user-facing string, using `absent_msg` for the
    /// [`KeychainReadError::Absent`] case. The `Unavailable` and `Corrupt`
    /// cases already carry their own actionable text (from
    /// [`keychain_error_help`] / [`corrupt_tokens_aes_key_help`]), so each
    /// credential names its own not-configured message without the enum having
    /// to know which credential it came from.
    pub fn into_message(self, absent_msg: &str) -> String {
        match self {
            KeychainReadError::Absent => absent_msg.to_string(),
            KeychainReadError::Unavailable(help) | KeychainReadError::Corrupt(help) => help,
        }
    }
}

/// Classify one failed keychain read. Mirrors [`classify_keychain_lookup`]'s
/// table — `NoEntry` is the only error that means "absent", every other
/// `keyring::Error` is a platform problem worth the setup help — while keeping
/// the caller's own absent text available instead of discarding the failure
/// into a `Present`/`Absent` pair (issue #801).
fn classify_read_failure(err: keyring::Error) -> KeychainReadError {
    match err {
        keyring::Error::NoEntry => KeychainReadError::Absent,
        // `keychain_error_help` returns `Some` for every non-`NoEntry` error;
        // the fallback is defensive only.
        other => KeychainReadError::Unavailable(
            keychain_error_help(&other).unwrap_or_else(|| format!("OS keychain error: {}", other)),
        ),
    }
}

/// The "user must re-onboard" text for an absent Spotify client secret. Kept
/// byte-identical to the pre-#801 message: Onboarding and the boot probe branch
/// on it.
const SPOTIFY_CLIENT_SECRET_NOT_FOUND_MSG: &str =
    "Spotify client secret not found in keychain. Please re-enter via Onboarding.";

/// Tri-state presence of the Spotify `client_secret`, consulting the OS
/// keychain directly (never a warm cache hit) exactly like
/// [`has_spotify_client_secret`] — so it reflects a deletion made while the
/// app runs. The namespaced slot is checked first and the legacy
/// unnamespaced slot second, keeping the categories meaningful on both
/// generations of installs (audit M2).
pub fn spotify_client_secret_presence() -> KeychainPresence {
    #[cfg(test)]
    if let Some(backend) = test_spotify_client_secret_backend() {
        return refresh_spotify_client_secret_presence(cache(), move |user| (backend.probe)(user));
    }

    refresh_spotify_client_secret_presence(cache(), probe_keychain_entry)
}

fn refresh_spotify_client_secret_presence(
    cache: &SpotifyClientSecretCache,
    probe: impl Fn(&str) -> Result<String, keyring::Error>,
) -> KeychainPresence {
    let observed_generation = cache.generation();
    let now = Instant::now();
    let presence = probe_spotify_client_secret_presence(probe);
    // The probe may overlap an explicit store/delete. Its result is only
    // metadata; the generation check prevents a delayed probe from
    // resurrecting a secret or a deleted presence observation.
    if cache.record_presence_if_current(observed_generation, presence.clone(), now) {
        presence
    } else {
        cache.current_presence().unwrap_or(KeychainPresence::Absent)
    }
}

/// Config-load presence: a fresh warm cache hit answers without touching the
/// OS keychain; a cold or expired cache falls back to the direct tri-state
/// probe above. The fallback keeps `Absent` and `Unavailable` distinct.
pub fn cached_spotify_client_secret_presence() -> KeychainPresence {
    cached_spotify_client_secret_presence_with(cache(), Instant::now(), || {
        spotify_client_secret_presence()
    })
}

fn cached_spotify_client_secret_presence_with(
    cache: &SpotifyClientSecretCache,
    now: Instant,
    probe: impl FnOnce() -> KeychainPresence,
) -> KeychainPresence {
    cache.fresh_presence(now).unwrap_or_else(probe)
}

/// Direct platform probe. It intentionally returns only the tri-state
/// presence: a legacy value is not reusable until the explicit read path can
/// forward-migrate it.
fn probe_spotify_client_secret_presence(
    probe: impl Fn(&str) -> Result<String, keyring::Error>,
) -> KeychainPresence {
    combine_presence(
        [
            SPOTIFY_CLIENT_SECRET_USER,
            SPOTIFY_CLIENT_SECRET_USER_LEGACY,
        ]
        .into_iter()
        .map(|user| classify_keychain_lookup(probe(user))),
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
    delete_spotify_client_secret_platform()?;
    log::info!("[KEYCHAIN] Deleted Spotify client_secret from keychain (cache cleared)");
    Ok(())
}

fn delete_spotify_client_secret_platform() -> Result<(), String> {
    #[cfg(test)]
    if let Some(backend) = test_spotify_client_secret_backend() {
        return delete_spotify_client_secret_with_cache(
            cache(),
            {
                let backend = backend.clone();
                move || (backend.delete_namespaced)()
            },
            {
                let backend = backend.clone();
                move || (backend.drop_legacy)()
            },
        );
    }

    delete_spotify_client_secret_with_cache(
        cache(),
        || delete_keychain_entry(SPOTIFY_CLIENT_SECRET_USER),
        || delete_keychain_entry(SPOTIFY_CLIENT_SECRET_USER_LEGACY),
    )
}

/// Core of [`delete_spotify_client_secret`] with the cache and keychain
/// operations injected. The explicit cache clear shares the same generation
/// protocol as store, so a probe already in flight cannot repopulate it.
fn delete_spotify_client_secret_with_cache(
    cache: &SpotifyClientSecretCache,
    delete_namespaced: impl FnOnce() -> Result<(), String>,
    drop_legacy: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let _mutation = spotify_client_secret_mutation_lock().lock();
    delete_namespaced()?;
    if let Err(e) = drop_legacy() {
        log::warn!(
            "[KEYCHAIN] legacy spotify_client_secret delete failed: {} (continuing)",
            e
        );
    }
    cache.clear();
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
/// [`CACHE`]. It is a *hint* for the write path rather than the truth: the
/// keychain slot can be deleted or replaced while the app runs — including by
/// the recovery step this module's own corrupt-key error text recommends — so
/// [`get_or_create_tokens_aes_key`] revalidates the cached key against the slot
/// before using it (issue #936), and [`delete_tokens_aes_key`] clears it.
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

/// Read the tokens.json AES-256-GCM key from the OS keychain, with a typed
/// failure (issue #935).
///
/// The key is stored base64-encoded under
/// `(KEYRING_SERVICE, TOKENS_AES_KEY_USER)`. The in-process cache is consulted
/// first: this is the *read* path, whose callers only decrypt. A stale cached
/// key is caught on the write path ([`get_or_create_tokens_aes_key`]), which
/// revalidates the cache before encrypting (issue #936).
///
/// The categories are what let the token store tell a *locked keychain* apart
/// from *corrupt ciphertext* at launch: `Absent`/`Unavailable` mean the file on
/// disk may still be perfectly recoverable once the keychain answers again,
/// while only a present-but-undecodable key is genuinely `Corrupt`. This is a
/// pure read: it never creates the key.
pub fn read_tokens_aes_key() -> Result<[u8; 32], KeychainReadError> {
    if let Some(key) = cached_tokens_aes_key() {
        return Ok(key);
    }
    let b64 = probe_keychain_entry(TOKENS_AES_KEY_USER).map_err(classify_read_failure)?;
    let key = decode_tokens_aes_key(&b64)
        .map_err(|detail| KeychainReadError::Corrupt(corrupt_tokens_aes_key_help(detail)))?;
    *tokens_key_cache().lock() = Some(key);
    Ok(key)
}

/// String-presenting wrapper over [`read_tokens_aes_key`], for the callers that
/// report the failure to the user. Both messages are unchanged from before
/// #935.
pub fn get_tokens_aes_key() -> Result<[u8; 32], String> {
    read_tokens_aes_key().map_err(|e| e.into_message(TOKENS_AES_KEY_NOT_FOUND_MSG))
}

/// The re-auth text for an absent tokens key: without it the ciphertext on disk
/// cannot be decrypted, so the safest recovery is to discard the tokens and
/// re-onboard. Kept byte-identical to the pre-#935 message.
pub(crate) const TOKENS_AES_KEY_NOT_FOUND_MSG: &str =
    "Tokens encryption key not found in OS keychain; cannot decrypt tokens.json (re-authentication required).";

/// Typed read-or-create variant used by legacy token migration. Keeping the
/// classification at this boundary lets the token loader distinguish a locked
/// platform store from a malformed on-disk store before migration can touch
/// the file.
pub fn read_or_create_tokens_aes_key() -> Result<[u8; 32], KeychainReadError> {
    // The slot is opened lazily, inside the closures: a keychain that cannot be
    // opened at all must not defeat the revalidation below, which deliberately
    // keeps a cached key when the keychain does not answer.
    let key = get_or_create_tokens_aes_key_with(
        cached_tokens_aes_key(),
        || probe_keychain_entry(TOKENS_AES_KEY_USER),
        |b64| {
            let entry = keyring::Entry::new(KEYRING_SERVICE, TOKENS_AES_KEY_USER)?;
            entry.set_password(b64)
        },
        generate_tokens_aes_key,
    )?;
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
///
/// The cached key is revalidated against the keychain slot before it is used
/// (issue #936): a slot deleted or replaced while the app runs — including by
/// the recovery step [`corrupt_tokens_aes_key_help`] recommends, or an OS
/// keychain UI — would otherwise make every later persist encrypt with a key
/// the keychain no longer holds, so tokens.json fails GCM authentication at the
/// next launch and the user is pushed through onboarding with no explanation.
/// String-presenting wrapper for write callers that do not need the transient
/// load classification.
pub fn get_or_create_tokens_aes_key() -> Result<[u8; 32], String> {
    read_or_create_tokens_aes_key().map_err(|e| e.into_message(TOKENS_AES_KEY_NOT_FOUND_MSG))
}

/// Core of [`get_or_create_tokens_aes_key`]: cached-key revalidation (issue
/// #936) followed by one locked read-or-create
/// ([`create_or_adopt_tokens_key`]). The keychain access is injected so the
/// decision table is unit-testable without an OS keychain.
///
/// The cache is a hint here, not the truth. A cached key is used only while the
/// slot still holds it; persists happen at token-refresh frequency rather than
/// on the polling hot path, so the extra read is affordable. When the slot is
/// gone, replaced or undecodable the decision is delegated to
/// [`create_or_adopt_tokens_key`], which regenerates on `NoEntry`, adopts a
/// different stored key, and hard-errors on a corrupt one.
///
/// When the keychain merely cannot *answer* (locked vault, no Secret Service
/// daemon, denied access prompt) the cached key is kept: that is not a
/// deletion, the cached key is still this install's key, and failing a persist
/// the keychain cannot answer for would lose the session on disk.
fn get_or_create_tokens_aes_key_with(
    cached: Option<[u8; 32]>,
    read: impl Fn() -> Result<String, keyring::Error>,
    store: impl Fn(&str) -> Result<(), keyring::Error>,
    generate: impl FnOnce() -> Result<[u8; 32], String>,
) -> Result<[u8; 32], KeychainReadError> {
    if let Some(cached) = cached {
        match read() {
            Ok(b64) if decode_tokens_aes_key(&b64) == Ok(cached) => return Ok(cached),
            Ok(_) | Err(keyring::Error::NoEntry) => log::warn!(
                "[KEYCHAIN] cached tokens AES key is no longer the keychain's; \
                 re-resolving from the keychain"
            ),
            Err(e) => {
                log::warn!(
                    "[KEYCHAIN] could not revalidate the cached tokens AES key ({}); \
                     using the cached key",
                    e
                );
                return Ok(cached);
            }
        }
    }
    create_or_adopt_tokens_key(read, store, generate)
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
) -> Result<[u8; 32], KeychainReadError> {
    let _guard = tokens_key_create_lock().lock();
    match read() {
        Ok(b64) => decode_tokens_aes_key(&b64)
            .map_err(|detail| KeychainReadError::Corrupt(corrupt_tokens_aes_key_help(detail))),
        Err(keyring::Error::NoEntry) => {
            let key = generate().map_err(KeychainReadError::Unavailable)?;
            let b64 = STANDARD.encode(key);
            store(&b64).map_err(classify_read_failure)?;
            log::info!("[KEYCHAIN] Generated + stored tokens.json AES key in OS keychain");
            // Verify-after-set: a lost write would otherwise leave ciphertext
            // under a key the keychain does not hold.
            match read() {
                Ok(stored) => {
                    let stored_key = decode_tokens_aes_key(&stored).map_err(|detail| {
                        KeychainReadError::Corrupt(corrupt_tokens_aes_key_help(detail))
                    })?;
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
        Err(e) => Err(classify_read_failure(e)),
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

// ---------------------------------------------------------------------------
// Issue #865: `--serve` bearer token
// ---------------------------------------------------------------------------
//
// The HTTP server binds 127.0.0.1 only, so the network surface is the local
// UID — but local UIDs are still untrusted (other apps run as the same user,
// shared containers, remote desktop sessions, etc.), so every mutating
// route requires `Authorization: Bearer <token>`. The token is 32 random
// bytes, base64url-encoded (43 chars, no padding) so it survives HTTP
// headers and shell arguments unchanged, and lives in the OS keychain —
// never on disk in plaintext, never in argv, never in env.

/// Read the serve bearer token from the keychain.
///
/// `None` means the daemon has never been started in `--serve` mode on
/// this install; `Err` is reserved for OS keychain failures (locked
/// vault, denied access prompt, missing Secret Service). The caller is
/// responsible for distinguishing those via [`crate::token_io`].
pub fn read_serve_token() -> Result<Option<String>, String> {
    match keyring::Entry::new(KEYRING_SERVICE, SERVE_TOKEN_USER) {
        Ok(entry) => match entry.get_password() {
            Ok(t) => Ok(Some(t)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(map_keychain_err(Err::<String, _>(e)).unwrap_err()),
        },
        Err(e) => Err(map_keychain_err(Err::<String, _>(e)).unwrap_err()),
    }
}

/// Generate a fresh 32-byte serve token and store it in the OS keychain,
/// overwriting any prior value. Called by the `--serve` startup path the
/// first time the daemon boots in serve mode; subsequent boots read the
/// existing token via [`read_serve_token`] so the bearer credential is
/// stable across restarts (otherwise every restart would invalidate every
/// external scheduler / dashboard that holds the token).
///
/// The generation is entropy-safe (`rand::rngs::OsRng`), and the storage
/// path uses the same map-keychain-error plumbing as every other keychain
/// write here, so a locked vault surfaces the same `SETUP.md`-pointing
/// help text the Spotify secret does.
pub fn rotate_serve_token() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(|e| format!("OS RNG refused to fill 32 bytes for serve token: {}", e))?;
    // base64url, no padding — survives HTTP headers and shell args.
    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let entry = map_keychain_err(keyring::Entry::new(KEYRING_SERVICE, SERVE_TOKEN_USER))?;
    map_keychain_err(entry.set_password(&token))?;
    log::info!("[KEYCHAIN] Rotated serve bearer token in OS keychain");
    Ok(token)
}

/// Test-only: delete the serve token slot without going through any cache.
/// Mirrors [`delete_tokens_aes_key`] so the slot can be cleaned up between
/// tests without leaving a credential behind on the dev keychain.
#[cfg(test)]
#[allow(dead_code)]
fn delete_serve_token() -> Result<(), String> {
    delete_keychain_entry(SERVE_TOKEN_USER)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A platform failure of the shape keyring reports on Linux when no
    /// Secret Service daemon answers (or the keyring is locked).
    fn platform_failure(msg: &'static str) -> keyring::Error {
        keyring::Error::PlatformFailure(Box::new(std::io::Error::other(msg)))
    }

    fn test_backend(
        probe: impl Fn(&str) -> Result<String, keyring::Error> + Send + Sync + 'static,
        read: impl Fn(&str) -> Result<String, keyring::Error> + Send + Sync + 'static,
        store_namespaced: impl Fn(&str) -> Result<(), String> + Send + Sync + 'static,
        drop_legacy: impl Fn() -> Result<(), String> + Send + Sync + 'static,
        delete_namespaced: impl Fn() -> Result<(), String> + Send + Sync + 'static,
        migrate_legacy: impl Fn(&str) + Send + Sync + 'static,
    ) -> TestSpotifyClientSecretBackend {
        let drop_legacy: TestSpotifyOperation = std::sync::Arc::new(drop_legacy);
        TestSpotifyClientSecretBackend {
            probe: std::sync::Arc::new(probe),
            read: std::sync::Arc::new(read),
            store_namespaced: std::sync::Arc::new(store_namespaced),
            drop_legacy: std::sync::Arc::clone(&drop_legacy),
            delete_namespaced: std::sync::Arc::new(delete_namespaced),
            migrate_legacy: std::sync::Arc::new(migrate_legacy),
        }
    }

    /// A cold config load performs one platform probe and refreshes only the
    /// presence metadata, so the immediately repeated load needs no second
    /// probe without making a legacy value look like a reusable secret.
    #[test]
    fn cold_presence_probe_warms_repeated_config_loads() {
        let cache = SpotifyClientSecretCache::default();
        let probe_calls = std::sync::atomic::AtomicUsize::new(0);
        let cold = cached_spotify_client_secret_presence_with(&cache, Instant::now(), || {
            refresh_spotify_client_secret_presence(&cache, |user| {
                assert_eq!(user, SPOTIFY_CLIENT_SECRET_USER);
                probe_calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok("warm-secret".to_string())
            })
        });
        assert_eq!(cold, KeychainPresence::Present);
        assert_eq!(
            cache.peek(),
            None,
            "presence-only observations must not populate the explicit-read cache"
        );

        let warm = cached_spotify_client_secret_presence_with(&cache, Instant::now(), || {
            panic!("the warmed config load must not probe again")
        });
        assert_eq!(warm, KeychainPresence::Present);
        assert_eq!(probe_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    /// A normal config load uses a fresh warm observation without a second
    /// platform probe. Once the TTL lapses, the direct fallback runs and an
    /// unreadable keychain remains `Unavailable`, not `Absent`.
    #[test]
    fn warm_presence_avoids_repeated_probes_until_ttl() {
        let cache = SpotifyClientSecretCache::default();
        let observed_at = Instant::now();
        let probe_calls = std::sync::atomic::AtomicUsize::new(0);
        cache.store("warm-secret", observed_at);

        let warm = cached_spotify_client_secret_presence_with(
            &cache,
            observed_at + SPOTIFY_CLIENT_SECRET_PRESENCE_TTL - Duration::from_secs(1),
            || {
                probe_calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                panic!("a fresh warm cache must not probe the platform")
            },
        );
        assert_eq!(warm, KeychainPresence::Present);
        assert_eq!(probe_calls.load(std::sync::atomic::Ordering::SeqCst), 0);

        let expired = cached_spotify_client_secret_presence_with(
            &cache,
            observed_at + SPOTIFY_CLIENT_SECRET_PRESENCE_TTL,
            || {
                refresh_spotify_client_secret_presence(&cache, |_| {
                    Err(platform_failure("keyring locked"))
                })
            },
        );
        assert!(matches!(expired, KeychainPresence::Unavailable(_)));
        assert_eq!(
            cache.peek().as_deref(),
            Some("warm-secret"),
            "an unavailable probe must not discard the explicit-read cache"
        );
        let after_external_delete = cached_spotify_client_secret_presence_with(
            &cache,
            observed_at + SPOTIFY_CLIENT_SECRET_PRESENCE_TTL + Duration::from_secs(1),
            || {
                refresh_spotify_client_secret_presence(&cache, |user| {
                    assert!(matches!(
                        user,
                        SPOTIFY_CLIENT_SECRET_USER | SPOTIFY_CLIENT_SECRET_USER_LEGACY
                    ));
                    probe_calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Err(keyring::Error::NoEntry)
                })
            },
        );
        assert_eq!(after_external_delete, KeychainPresence::Absent);
        assert_eq!(cache.peek(), None);
        assert_eq!(probe_calls.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    /// Explicit store/delete operations own the cache update. The regression
    /// calls the public helpers through a test-only keychain backend, so a
    /// wrapper that bypasses the generation bump cannot stay green.
    #[test]
    fn explicit_cache_changes_refresh_the_next_presence_load() {
        let backend = test_backend(
            |_user| Err(keyring::Error::NoEntry),
            |_user| Err(keyring::Error::NoEntry),
            |_value| Ok(()),
            || Ok(()),
            || Ok(()),
            |_value| {},
        );
        with_test_spotify_client_secret_backend(backend, || {
            cache().clear();
            store_spotify_client_secret("old-secret").unwrap();
            store_spotify_client_secret("rotated-secret").unwrap();
            assert_eq!(cache().peek().as_deref(), Some("rotated-secret"));

            delete_spotify_client_secret().unwrap();
            assert_eq!(spotify_client_secret_presence(), KeychainPresence::Absent);
            assert_eq!(cache().peek(), None);
        });
    }

    /// A presence probe that started before an explicit store must not publish
    /// its stale value after the store commits.
    #[test]
    fn delayed_presence_probe_cannot_overwrite_explicit_store() {
        let probe_started = std::sync::Arc::new(std::sync::Barrier::new(2));
        let release_probe = std::sync::Arc::new(std::sync::Barrier::new(2));
        let probe_calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let probe_started_backend = std::sync::Arc::clone(&probe_started);
        let release_probe_backend = std::sync::Arc::clone(&release_probe);
        let probe_calls_backend = std::sync::Arc::clone(&probe_calls);
        let backend = test_backend(
            move |_user| {
                if probe_calls_backend.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                    probe_started_backend.wait();
                    release_probe_backend.wait();
                }
                Ok("stale-secret".to_string())
            },
            |_user| Ok("unused-read".to_string()),
            |_value| Ok(()),
            || Ok(()),
            || Ok(()),
            |_value| {},
        );
        with_test_spotify_client_secret_backend(backend, || {
            cache().clear();
            let probe = std::thread::spawn(spotify_client_secret_presence);
            probe_started.wait();
            store_spotify_client_secret("rotated-secret").unwrap();
            release_probe.wait();

            assert_eq!(
                probe.join().expect("probe thread must not panic"),
                KeychainPresence::Present
            );
            assert_eq!(cache().peek().as_deref(), Some("rotated-secret"));
        });
    }

    /// A presence probe that started before an explicit delete must not
    /// resurrect either the secret or its fresh presence metadata.
    #[test]
    fn delayed_presence_probe_cannot_resurrect_explicit_delete() {
        let probe_started = std::sync::Arc::new(std::sync::Barrier::new(2));
        let release_probe = std::sync::Arc::new(std::sync::Barrier::new(2));
        let probe_calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let probe_started_backend = std::sync::Arc::clone(&probe_started);
        let release_probe_backend = std::sync::Arc::clone(&release_probe);
        let probe_calls_backend = std::sync::Arc::clone(&probe_calls);
        let backend = test_backend(
            move |_user| {
                if probe_calls_backend.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                    probe_started_backend.wait();
                    release_probe_backend.wait();
                }
                Ok("stale-secret".to_string())
            },
            |_user| Ok("old-secret".to_string()),
            |_value| Ok(()),
            || Ok(()),
            || Ok(()),
            |_value| {},
        );
        with_test_spotify_client_secret_backend(backend, || {
            cache().clear();
            store_spotify_client_secret("old-secret").unwrap();
            let probe = std::thread::spawn(spotify_client_secret_presence);
            probe_started.wait();
            delete_spotify_client_secret().unwrap();
            release_probe.wait();

            assert_eq!(
                probe.join().expect("probe thread must not panic"),
                KeychainPresence::Absent
            );
            assert_eq!(cache().peek(), None);
            assert!(cache().fresh_presence(Instant::now()).is_none());
        });
    }

    /// A read that commits a freshly looked-up secret also advances the
    /// generation, so a delayed absence probe cannot clear that read cache.
    #[test]
    fn delayed_presence_probe_cannot_clear_a_concurrent_read_commit() {
        let probe_started = std::sync::Arc::new(std::sync::Barrier::new(2));
        let release_probe = std::sync::Arc::new(std::sync::Barrier::new(2));
        let probe_calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let probe_started_backend = std::sync::Arc::clone(&probe_started);
        let release_probe_backend = std::sync::Arc::clone(&release_probe);
        let probe_calls_backend = std::sync::Arc::clone(&probe_calls);
        let backend = test_backend(
            move |_user| {
                if probe_calls_backend.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                    probe_started_backend.wait();
                    release_probe_backend.wait();
                }
                Err(keyring::Error::NoEntry)
            },
            |_user| Ok("read-secret".to_string()),
            |_value| Ok(()),
            || Ok(()),
            || Ok(()),
            |_value| panic!("a namespaced read must not invoke legacy migration"),
        );
        with_test_spotify_client_secret_backend(backend, || {
            cache().clear();
            let probe = std::thread::spawn(spotify_client_secret_presence);
            probe_started.wait();
            let secret = read_spotify_client_secret().expect("the concurrent read must succeed");
            release_probe.wait();

            assert_eq!(secret, "read-secret");
            assert_eq!(
                probe.join().expect("probe thread must not panic"),
                KeychainPresence::Present
            );
            assert_eq!(cache().peek().as_deref(), Some("read-secret"));
        });
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
            let message = match err {
                KeychainReadError::Corrupt(message) => message,
                other => panic!("corrupt entry must classify as Corrupt, got {other:?}"),
            };
            assert!(
                message.contains(LINUX_KEYRING_DOC),
                "corrupt-key error must carry a recovery pointer, got: {message}"
            );
        }
    }

    #[test]
    fn creator_preserves_platform_unavailable_classification() {
        let err = create_or_adopt_tokens_key(
            || Err::<String, _>(platform_failure("no secret service")),
            |_| panic!("an unavailable keychain must not be written"),
            || panic!("an unavailable keychain must not regenerate a key"),
        )
        .expect_err("a platform read failure must stop legacy migration");
        let message = match err {
            KeychainReadError::Unavailable(message) => message,
            other => panic!("platform failure must classify as Unavailable, got {other:?}"),
        };
        assert!(message.contains(LINUX_KEYRING_DOC));
    }

    /// Issue #801: the legacy-slot fallback must classify its failures. Only
    /// `NoEntry` means "the user never configured a secret"; a locked vault, a
    /// denied access prompt or an unreadable item on the legacy slot is the
    /// same platform problem it is on the namespaced slot. Reporting it as
    /// "not found" pushed users through a full re-onboarding that overwrote a
    /// working credential. Pre-fix the legacy arm matched `Err(_)` and returned
    /// the re-enter message for every error.
    #[test]
    fn legacy_lookup_failures_are_classified_not_reported_as_absent() {
        // Namespaced slot empty (as on a v2.7.2 install), legacy slot locked.
        let err = lookup_spotify_client_secret(|user| {
            if user == SPOTIFY_CLIENT_SECRET_USER {
                Err(keyring::Error::NoEntry)
            } else {
                Err(platform_failure("no secret service"))
            }
        })
        .expect_err("an unreadable legacy slot must not yield a secret");

        assert!(
            matches!(err, KeychainReadError::Unavailable(_)),
            "a platform failure on the legacy slot must classify as Unavailable, got {err:?}"
        );
        let message = err.into_message(SPOTIFY_CLIENT_SECRET_NOT_FOUND_MSG);
        assert!(
            !message.contains("Please re-enter via Onboarding"),
            "an unreadable credential must not be reported as never configured, got: {message}"
        );
        assert!(
            message.contains(LINUX_KEYRING_DOC),
            "the unavailable message must carry the actionable setup help, got: {message}"
        );

        // Both slots empty is the one genuine "never configured" outcome, and it
        // must keep the onboarding message that Onboarding branches on.
        let absent = lookup_spotify_client_secret(|_| Err(keyring::Error::NoEntry))
            .expect_err("two empty slots must be Absent");
        assert_eq!(absent, KeychainReadError::Absent);
        assert!(
            absent
                .into_message(SPOTIFY_CLIENT_SECRET_NOT_FOUND_MSG)
                .contains("Please re-enter via Onboarding"),
            "a never-configured secret must still prompt for onboarding"
        );
    }

    /// Issue #801: the fallback still reads the legacy value, and reports which
    /// slot it came from so the caller can forward-migrate it. The migration
    /// itself is deliberately not part of this core: that is what keeps the
    /// classification unit-testable without touching an OS keychain.
    #[test]
    fn legacy_slot_value_is_returned_and_tagged_for_migration() {
        let (secret, slot) = lookup_spotify_client_secret(|user| {
            if user == SPOTIFY_CLIENT_SECRET_USER {
                Err(keyring::Error::NoEntry)
            } else {
                Ok("legacy-secret".to_string())
            }
        })
        .expect("a legacy value must be returned");
        assert_eq!(secret, "legacy-secret");
        assert_eq!(slot, SecretSlot::Legacy);

        let (secret, slot) = lookup_spotify_client_secret(|_| Ok("current".to_string()))
            .expect("a namespaced value must be returned");
        assert_eq!(secret, "current");
        assert_eq!(slot, SecretSlot::Namespaced);
    }

    /// A presence probe that finds only the legacy slot must not prime the
    /// reusable read cache. The subsequent public read has to observe the
    /// legacy slot again and run its forward migration.
    #[test]
    fn legacy_only_presence_does_not_bypass_forward_migration() {
        let migrated = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let migrated_backend = std::sync::Arc::clone(&migrated);
        let backend = test_backend(
            |user| {
                if user == SPOTIFY_CLIENT_SECRET_USER {
                    Err(keyring::Error::NoEntry)
                } else {
                    Ok("legacy-secret".to_string())
                }
            },
            |user| {
                if user == SPOTIFY_CLIENT_SECRET_USER {
                    Err(keyring::Error::NoEntry)
                } else {
                    Ok("legacy-secret".to_string())
                }
            },
            |_value| Ok(()),
            || Ok(()),
            || Ok(()),
            move |value| {
                assert_eq!(value, "legacy-secret");
                migrated_backend.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            },
        );
        with_test_spotify_client_secret_backend(backend, || {
            cache().clear();
            assert_eq!(spotify_client_secret_presence(), KeychainPresence::Present);
            assert_eq!(cache().peek(), None);

            let secret =
                read_spotify_client_secret().expect("the legacy value must remain readable");
            assert_eq!(secret, "legacy-secret");
            assert_eq!(migrated.load(std::sync::atomic::Ordering::SeqCst), 1);
            assert_eq!(cache().peek().as_deref(), Some("legacy-secret"));
        });
    }

    /// Issue #917: after a store, the superseded legacy slot must hold nothing —
    /// a secret rotated because of a suspected exposure must not stay
    /// retrievable under the pre-v2.7.2 name. The legacy delete is attempted on
    /// every store, and a failure of it must not fail the store (the new value
    /// is already in the namespaced slot).
    #[test]
    fn store_supersedes_the_legacy_slot() {
        let stored = parking_lot::Mutex::new(None::<String>);
        let legacy_deletes = parking_lot::Mutex::new(0u32);

        store_spotify_client_secret_with(
            "new-secret",
            |s: &str| -> Result<(), String> {
                *stored.lock() = Some(s.to_string());
                Ok(())
            },
            || -> Result<(), String> {
                *legacy_deletes.lock() += 1;
                Ok(())
            },
        )
        .expect("a store must succeed");

        assert_eq!(stored.lock().clone().as_deref(), Some("new-secret"));
        assert_eq!(
            *legacy_deletes.lock(),
            1,
            "the legacy slot delete must be attempted on every store"
        );

        store_spotify_client_secret_with(
            "newer",
            |_: &str| -> Result<(), String> { Ok(()) },
            || -> Result<(), String> { Err("locked".to_string()) },
        )
        .expect("a failed legacy delete must not fail the store");
    }

    /// Issue #936: the cached tokens AES key is a hint, not the truth — it must
    /// be revalidated against the keychain slot before it is used to encrypt a
    /// write. Deleting the slot at runtime (exactly the recovery step the
    /// corrupt-key error text recommends, and what an OS keychain UI can do) has
    /// to regenerate and store a fresh key; otherwise tokens.json is written
    /// under a key the keychain no longer holds and fails GCM authentication at
    /// the next launch. Pre-fix the cached key was returned without consulting
    /// the keychain at all.
    #[test]
    fn cached_tokens_key_is_revalidated_against_the_slot() {
        let slot = parking_lot::Mutex::new(None::<String>);
        let store_calls = parking_lot::Mutex::new(0u32);
        let read = || -> Result<String, keyring::Error> {
            match slot.lock().clone() {
                Some(b64) => Ok(b64),
                None => Err(keyring::Error::NoEntry),
            }
        };
        let store = |b64: &str| -> Result<(), keyring::Error> {
            *store_calls.lock() += 1;
            *slot.lock() = Some(b64.to_string());
            Ok(())
        };
        let stale = [0xAAu8; 32];
        let fresh = [0xBBu8; 32];

        // The slot is gone: the stale cached key must be replaced by a fresh one
        // that the keychain really holds, i.e. the key the next write uses.
        let regenerated = get_or_create_tokens_aes_key_with(Some(stale), read, store, || Ok(fresh))
            .expect("a deleted slot must be regenerated, not reused");
        assert_eq!(
            regenerated, fresh,
            "the stale cached key must not be reused"
        );
        assert_eq!(*store_calls.lock(), 1, "the fresh key must be stored");
        let stored_b64 = slot.lock().clone().expect("the fresh key must be stored");
        assert_eq!(
            decode_tokens_aes_key(&stored_b64).unwrap(),
            fresh,
            "the keychain must hold the key the write will encrypt with"
        );

        // The slot still holds the cached key: no keychain write, no
        // regeneration.
        *slot.lock() = Some(STANDARD.encode(fresh));
        let confirmed = get_or_create_tokens_aes_key_with(Some(fresh), read, store, || {
            panic!("a key the slot still holds must never be regenerated")
        })
        .expect("a confirmed cached key must be used");
        assert_eq!(confirmed, fresh);
        assert_eq!(
            *store_calls.lock(),
            1,
            "a confirmed cached key must not write to the keychain"
        );

        // The slot holds a *different* key: the keychain's key wins, because the
        // ciphertext written by whoever installed it must stay decryptable.
        let replaced = [0xCCu8; 32];
        *slot.lock() = Some(STANDARD.encode(replaced));
        let adopted = get_or_create_tokens_aes_key_with(Some(fresh), read, store, || {
            panic!("a replaced slot must be adopted, not overwritten")
        })
        .expect("a replaced slot must be adopted");
        assert_eq!(adopted, replaced);
        assert_eq!(*store_calls.lock(), 1);

        // The keychain cannot answer at all: a locked vault is not a deletion,
        // so the cached key is kept rather than failing the persist.
        let unavailable =
            || -> Result<String, keyring::Error> { Err(platform_failure("no secret service")) };
        let kept = get_or_create_tokens_aes_key_with(Some(fresh), unavailable, store, || {
            panic!("an unreadable slot must not trigger a regeneration")
        })
        .expect("an unreadable keychain must not discard a confirmed key");
        assert_eq!(kept, fresh);
    }
}
