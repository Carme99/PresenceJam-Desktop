use crate::token_io;
use parking_lot::{Mutex, RwLock};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::OnceLock;
use std::thread;
use std::time::Instant;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct PendingSpotifyAuth {
    pub verifier: String,
    pub state: String,
    pub client_id: String,
    // NOTE: `client_secret` is intentionally absent. The secret lives in the
    // OS keychain (see `keychain::store_spotify_client_secret`) and is read
    // back at token-exchange time. Storing it here would defeat the purpose
    // of the keychain migration. See issue #9.
    pub redirect_uri: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// 30s result cache for `is_onboarding_complete`.
///
/// `Some((ts, result))` means a successful or failed validation ran at `ts`
/// and produced `result`. `None` means the cache is cold (or was cleared
/// after a token refresh). Lives in its own sub-struct (not as a top-level
/// field on `AppState`) so the AppState struct-of-states refactor (#80)
/// can fold other sub-structs alongside it.
///
/// **Lock encapsulation (load-bearing):** all access goes through the
/// `lock()` method on this sub-struct — never via `self.state.lock()`
/// from the call site. The pattern keeps the lock acquisition
/// observable in one place, which is what makes future work like
/// "lock must not be held across an await" enforceable (a `lock_async`
/// method could replace `lock` later without rewriting every call
/// site). See issue #80.
pub struct OnboardingCache {
    state: Mutex<Option<(Instant, bool)>>,
}

impl OnboardingCache {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(None),
        }
    }

    /// Acquire the cache lock. Use this instead of touching `self.state`
    /// directly so future refactors (e.g. async-aware locks) only
    /// need to change one site.
    pub fn lock(&self) -> parking_lot::MutexGuard<'_, Option<(Instant, bool)>> {
        self.state.lock()
    }

    /// Clear the cache. Convenience wrapper for the common
    /// `*state.lock() = None;` pattern; used after any auth flow that
    /// could change the onboarding result (token refresh, reconnect,
    /// initial setup completion).
    pub fn invalidate(&self) {
        *self.state.lock() = None;
    }
}

impl Default for OnboardingCache {
    /// Required by `clippy::new_without_default`. `Default::default()`
    /// produces an empty cache, identical to `OnboardingCache::new()`.
    fn default() -> Self {
        Self::new()
    }
}
// =====================================================================
// AppState sub-structs (issue #80 step 2)
// =====================================================================
//
// Each sub-struct follows the OnboardingCache pattern established in step 1:
//   * All fields are private.
//   * `lock_*` / `get*` / `is_syncing` / `set_syncing` / `try_claim`
//     methods are the only path to the inner data. Call sites never
//     name the underlying Mutex/RwLock/AtomicBool.
//   * `Default` is implemented alongside `new()` so clippy's
//     `new_without_default` lint stays quiet (this bit PR #118).
//
// The atomic ordering on is_syncing (Acquire load, Release store,
// AcqRel compare-exchange) is preserved exactly: see `Polling::is_syncing`
// / `Polling::set_syncing` / `Polling::try_claim`.

/// OAuth tokens for Spotify + Teams. The two locks are independent so
/// a refresh on one provider can't block reads/writes on the other.
pub struct Tokens {
    spotify: RwLock<Option<crate::spotify::SpotifyTokens>>,
    teams: RwLock<Option<crate::teams::TeamsTokens>>,
}

impl Tokens {
    pub fn new() -> Self {
        Self {
            spotify: RwLock::new(None),
            teams: RwLock::new(None),
        }
    }

    /// Read guard for the Spotify token slot. Use this instead of
    /// touching the `spotify` field directly.
    pub fn spotify(
        &self,
    ) -> parking_lot::RwLockReadGuard<'_, Option<crate::spotify::SpotifyTokens>> {
        self.spotify.read()
    }

    /// Write guard for the Spotify token slot. Use this instead of
    /// touching the `spotify` field directly.
    pub fn spotify_mut(
        &self,
    ) -> parking_lot::RwLockWriteGuard<'_, Option<crate::spotify::SpotifyTokens>> {
        self.spotify.write()
    }

    /// Attempt to acquire the Spotify write guard without blocking.
    ///
    /// Issue #778: the deterministic concurrency-test seam proving the
    /// snapshot holds its critical section — `None` while a reader holds the
    /// slot, `Some` once released. Mirrors `Config::try_get_mut`.
    #[cfg(test)]
    pub fn try_spotify_mut(
        &self,
    ) -> Option<parking_lot::RwLockWriteGuard<'_, Option<crate::spotify::SpotifyTokens>>> {
        self.spotify.try_write()
    }

    /// Read guard for the Teams token slot. Use this instead of
    /// touching the `teams` field directly.
    pub fn teams(&self) -> parking_lot::RwLockReadGuard<'_, Option<crate::teams::TeamsTokens>> {
        self.teams.read()
    }

    /// Write guard for the Teams token slot. Use this instead of
    /// touching the `teams` field directly.
    pub fn teams_mut(
        &self,
    ) -> parking_lot::RwLockWriteGuard<'_, Option<crate::teams::TeamsTokens>> {
        self.teams.write()
    }

    /// Attempt to acquire the Teams write guard without blocking.
    ///
    /// Issue #778: the deterministic concurrency-test seam proving the
    /// snapshot holds its critical section — `None` while a reader holds the
    /// slot, `Some` once released. Mirrors `try_spotify_mut`.
    #[cfg(test)]
    pub fn try_teams_mut(
        &self,
    ) -> Option<parking_lot::RwLockWriteGuard<'_, Option<crate::teams::TeamsTokens>>> {
        self.teams.try_write()
    }

    /// Attempt to acquire the Spotify read guard without blocking.
    ///
    /// Issue #1126: the non-blocking seam for the status snapshot — a
    /// writer holding the slot (the poller's commit, a reconnect/disconnect
    /// clear) makes `try_read` miss instead of parking the reader, so
    /// `sync_status_from_state` can serve its cached snapshot. Mirrors
    /// `Config::try_get_mut`.
    pub fn try_spotify(
        &self,
    ) -> Option<parking_lot::RwLockReadGuard<'_, Option<crate::spotify::SpotifyTokens>>> {
        self.spotify.try_read()
    }

    /// Attempt to acquire the Teams read guard without blocking.
    ///
    /// Issue #1126: the non-blocking seam for the status snapshot — see
    /// `try_spotify`. Mirrors `Config::try_get_mut`.
    pub fn try_teams(
        &self,
    ) -> Option<parking_lot::RwLockReadGuard<'_, Option<crate::teams::TeamsTokens>>> {
        self.teams.try_read()
    }
}

impl Default for Tokens {
    /// Required by `clippy::new_without_default`. Equivalent to
    /// `Tokens::new()`.
    fn default() -> Self {
        Self::new()
    }
}

/// Pending PKCE auth, in flight between the OAuth URL
/// being opened and the callback landing. Never persisted to disk
/// (issue #65 / HIGH #3). Teams uses the device-code flow and stores no
/// pending state (see issue #158).
pub struct PendingAuths {
    spotify: RwLock<Option<PendingSpotifyAuth>>,
}

impl PendingAuths {
    pub fn new() -> Self {
        Self {
            spotify: RwLock::new(None),
        }
    }

    pub fn spotify(&self) -> parking_lot::RwLockReadGuard<'_, Option<PendingSpotifyAuth>> {
        self.spotify.read()
    }

    pub fn spotify_mut(&self) -> parking_lot::RwLockWriteGuard<'_, Option<PendingSpotifyAuth>> {
        self.spotify.write()
    }
}

impl Default for PendingAuths {
    /// Required by `clippy::new_without_default`. Equivalent to
    /// `PendingAuths::new()`.
    fn default() -> Self {
        Self::new()
    }
}

/// Persistent user config (`AppConfig`). Accessors keep the inner lock
/// private so every read-modify-write path can hold one guard across its full
/// critical section.
pub struct Config {
    config: RwLock<Option<Arc<crate::config::AppConfig>>>,
}

impl Config {
    pub fn new() -> Self {
        Self {
            config: RwLock::new(None),
        }
    }

    /// Read guard for the config slot. Use this instead of touching
    /// the `config` field directly. Prefer [`Config::snapshot`] unless
    /// the guard must be held across a critical section.
    pub fn get(&self) -> parking_lot::RwLockReadGuard<'_, Option<Arc<crate::config::AppConfig>>> {
        self.config.read()
    }

    /// Snapshot the config slot behind a shared pointer (issue #893).
    ///
    /// Clones the `Arc`, not the document: the polling loop calls this
    /// once per iteration and never holds the read guard across the
    /// iteration body, so a config save never waits on an in-flight
    /// poll. The snapshot is immutable — writers publish a new `Arc`
    /// under the write guard, so an iteration never observes a
    /// partially updated config.
    pub fn snapshot(&self) -> Option<Arc<crate::config::AppConfig>> {
        self.config.read().clone()
    }

    /// Write guard for the config slot. Use this instead of touching
    /// the `config` field directly.
    pub fn get_mut(
        &self,
    ) -> parking_lot::RwLockWriteGuard<'_, Option<Arc<crate::config::AppConfig>>> {
        self.config.write()
    }

    /// Attempt to acquire the config write guard without blocking.
    ///
    /// This is the deterministic concurrency-test seam for proving that a
    /// blocking import still owns the guard after replacement and before reload.
    pub fn try_get_mut(
        &self,
    ) -> Option<parking_lot::RwLockWriteGuard<'_, Option<Arc<crate::config::AppConfig>>>> {
        self.config.try_write()
    }
}

impl Default for Config {
    /// Required by `clippy::new_without_default`. Equivalent to
    /// `Config::new()`.
    fn default() -> Self {
        Self::new()
    }
}

/// Live polling state: the sync flag, the worker thread handle, the
/// stop-channel sender, and the last observed track. The sync flag
/// hardcodes its ordering pair — Acquire load, Release store (AcqRel
/// compare-exchange in `try_claim`) — so the happens-before chain
/// between the poller's exit path and the tray menu rebuild or the
/// UI's sync state holds by construction and no call site can
/// silently drop it with a weaker ordering (issue #759).
pub struct Polling {
    is_syncing: AtomicBool,
    handle: RwLock<Option<thread::JoinHandle<()>>>,
    stop_tx: RwLock<Option<mpsc::Sender<()>>>,
    current_track: RwLock<Option<crate::spotify::TrackInfo>>,
    /// Owner thread id for the running poller. Used to make the
    /// thread-exit cleanup ownership-checked so an old thread that
    /// lingers after an async Stop→Start does not wipe the new
    /// thread's flag/stop_tx. See #69 regression.
    thread_id: RwLock<Option<thread::ThreadId>>,
}

impl Polling {
    pub fn new() -> Self {
        Self {
            is_syncing: AtomicBool::new(false),
            handle: RwLock::new(None),
            stop_tx: RwLock::new(None),
            current_track: RwLock::new(None),
            thread_id: RwLock::new(None),
        }
    }

    /// Load the sync flag with Acquire ordering, so a poller-exit
    /// store is observed before the tray menu rebuild or UI sync
    /// state reads that follow (issue #759).
    pub fn is_syncing(&self) -> bool {
        self.is_syncing.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Store a new value into the sync flag with Release ordering,
    /// publishing all prior writes to the next Acquire load
    /// (issue #759).
    pub fn set_syncing(&self, value: bool) {
        self.is_syncing
            .store(value, std::sync::atomic::Ordering::Release);
    }

    /// Attempt to atomically claim the sync flag (false -> true). Returns
    /// `true` if this caller won the claim, `false` if the flag was
    /// already set. Uses AcqRel on success and Acquire on failure so the
    /// happens-before relationship with subsequent reads of `is_syncing`
    /// (polling loop, tray menu) is preserved exactly. This is the only
    /// site that does the CAS-equivalent operation; the polling
    /// submodule itself is intentionally CAS-free (see the
    /// `test_start_polling_does_not_claim_is_syncing` regression guard
    /// at the bottom of `polling/poll_once.rs::tests`).
    pub fn try_claim(&self) -> bool {
        self.is_syncing
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .is_ok()
    }

    /// Read guard for the worker thread handle.
    pub fn handle(&self) -> parking_lot::RwLockReadGuard<'_, Option<thread::JoinHandle<()>>> {
        self.handle.read()
    }

    /// Write guard for the worker thread handle.
    pub fn handle_mut(&self) -> parking_lot::RwLockWriteGuard<'_, Option<thread::JoinHandle<()>>> {
        self.handle.write()
    }

    /// Read guard for the stop-channel sender.
    pub fn stop_tx(&self) -> parking_lot::RwLockReadGuard<'_, Option<mpsc::Sender<()>>> {
        self.stop_tx.read()
    }

    /// Write guard for the stop-channel sender.
    pub fn stop_tx_mut(&self) -> parking_lot::RwLockWriteGuard<'_, Option<mpsc::Sender<()>>> {
        self.stop_tx.write()
    }

    /// Read guard for the last observed track.
    pub fn current_track(
        &self,
    ) -> parking_lot::RwLockReadGuard<'_, Option<crate::spotify::TrackInfo>> {
        self.current_track.read()
    }

    /// Write guard for the last observed track.
    pub fn current_track_mut(
        &self,
    ) -> parking_lot::RwLockWriteGuard<'_, Option<crate::spotify::TrackInfo>> {
        self.current_track.write()
    }

    /// Attempt to acquire the track write guard without blocking.
    ///
    /// Issue #778: the deterministic concurrency-test seam proving the
    /// snapshot holds its critical section — `None` while a reader holds the
    /// slot, `Some` once released. Mirrors `Tokens::try_spotify_mut`.
    #[cfg(test)]
    pub fn try_current_track_mut(
        &self,
    ) -> Option<parking_lot::RwLockWriteGuard<'_, Option<crate::spotify::TrackInfo>>> {
        self.current_track.try_write()
    }

    /// Read guard for the stored polling thread id.
    pub fn thread_id(&self) -> parking_lot::RwLockReadGuard<'_, Option<thread::ThreadId>> {
        self.thread_id.read()
    }

    /// Write guard for the stored polling thread id.
    pub fn thread_id_mut(&self) -> parking_lot::RwLockWriteGuard<'_, Option<thread::ThreadId>> {
        self.thread_id.write()
    }
}

impl Default for Polling {
    /// Required by `clippy::new_without_default`. Equivalent to
    /// `Polling::new()`.
    fn default() -> Self {
        Self::new()
    }
}

/// Single-flight gate for Spotify OAuth callbacks (issue #799).
///
/// On Windows/Linux a second-instance launch carrying a `presencejam://` URL
/// reaches the running instance through the single-instance plugin's
/// `deep-link` feature (`handle_cli_arguments` → `deep-link://new-url` →
/// `on_open_url` → `handle_deep_link`). The argv scan that used to
/// re-dispatch the same URL from `forward_launch_to_running_instance` is
/// gone, but `handle_deep_link` keeps this gate as belt-and-braces for the
/// pre-setup window (a `get_current` start URL followed by the same
/// `on_open_url` event): the first delivery of a `(code, state)` pair wins,
/// an identical repeat inside the dedup window is dropped before any token
/// exchange is spawned, and a different pair proceeds normally.
///
/// **Lock encapsulation (load-bearing, issue #80):** all access goes through
/// `claim` / `claim_at` — never via the inner field from the call site.
pub struct DeepLinkDedup {
    seen: Mutex<Option<(u64, Instant)>>,
}

/// Identical `(code, state)` repeats are dropped inside this window.
pub(crate) const DEEP_LINK_DEDUP_WINDOW: std::time::Duration = std::time::Duration::from_secs(10);

impl DeepLinkDedup {
    pub fn new() -> Self {
        Self {
            seen: Mutex::new(None),
        }
    }

    /// Claim a callback key. Returns `true` when this delivery may proceed,
    /// `false` when it is an identical repeat inside the dedup window.
    pub fn claim(&self, key: u64) -> bool {
        self.claim_at(key, Instant::now())
    }

    /// Deterministic core of `claim`: the clock is a parameter so tests can
    /// drive window expiry without sleeping.
    pub fn claim_at(&self, key: u64, now: Instant) -> bool {
        let mut guard = self.seen.lock();
        if let Some((prev_key, prev_at)) = *guard {
            if prev_key == key && now.duration_since(prev_at) < DEEP_LINK_DEDUP_WINDOW {
                return false;
            }
        }
        *guard = Some((key, now));
        true
    }
}

impl Default for DeepLinkDedup {
    /// Required by `clippy::new_without_default`. Equivalent to
    /// `DeepLinkDedup::new()`.
    fn default() -> Self {
        Self::new()
    }
}

/// Hash a `(code, state)` callback pair to a dedup key (issue #799). A
/// missing `state` maps to the empty string so the key stays total; it never
/// collides with a real delivery because a stateless callback returns early
/// before reaching the gate.
pub(crate) fn deep_link_callback_key(code: &str, state: Option<&str>) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    code.hash(&mut hasher);
    state.unwrap_or("").hash(&mut hasher);
    hasher.finish()
}

/// Whether the current process may persist the in-memory token snapshot.
///
/// A keychain-unavailable read leaves the encrypted file intact but the
/// in-memory token slots empty. Persisting that empty snapshot would destroy
/// sessions that become readable when the platform keychain unlocks, so the
/// transient load failure is retained until a later read succeeds or the user
/// explicitly resets token storage (issue #935).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokensLoadState {
    Ready,
    KeychainUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenProvider {
    Spotify,
    Teams,
}

#[derive(Debug)]
pub enum TokenCommitOutcome<T> {
    Committed(T),
    Discarded(Option<T>),
}

/// Per-provider markers for an explicit clear that happened after the
/// keychain-unavailable load. The marker mutex is the lock-order root for
#[derive(Default)]
pub struct RecoveryMarkers {
    pub(crate) spotify: bool,
    pub(crate) teams: bool,
}

impl RecoveryMarkers {
    pub(crate) fn blocks(&self, provider: TokenProvider) -> bool {
        match provider {
            TokenProvider::Spotify => self.spotify,
            TokenProvider::Teams => self.teams,
        }
    }

    pub(crate) fn clear(&mut self, provider: TokenProvider) {
        match provider {
            TokenProvider::Spotify => self.spotify = false,
            TokenProvider::Teams => self.teams = false,
        }
    }

    pub(crate) fn mark(&mut self, provider: TokenProvider) {
        match provider {
            TokenProvider::Spotify => self.spotify = true,
            TokenProvider::Teams => self.teams = true,
        }
    }
}

pub struct TokensLoadGate {
    state: AtomicU8,
    recovery: Mutex<RecoveryMarkers>,
}

impl TokensLoadGate {
    fn new() -> Self {
        Self {
            state: AtomicU8::new(TokensLoadState::Ready as u8),
            recovery: Mutex::new(RecoveryMarkers::default()),
        }
    }

    pub fn state(&self) -> TokensLoadState {
        match self.state.load(Ordering::Acquire) {
            1 => TokensLoadState::KeychainUnavailable,
            _ => TokensLoadState::Ready,
        }
    }

    pub fn mark_keychain_unavailable(&self) {
        let _recovery = self.recovery.lock();
        self.state.store(
            TokensLoadState::KeychainUnavailable as u8,
            Ordering::Release,
        );
    }

    pub fn mark_ready(&self) {
        let _recovery = self.recovery.lock();
        self.mark_ready_locked();
    }
    pub(crate) fn mark_ready_locked(&self) {
        self.state
            .store(TokensLoadState::Ready as u8, Ordering::Release);
    }

    pub fn blocks_persist(&self) -> bool {
        self.state() == TokensLoadState::KeychainUnavailable
    }

    pub fn recovery_guard(&self) -> parking_lot::MutexGuard<'_, RecoveryMarkers> {
        self.recovery.lock()
    }

    pub fn clear_spotify(&self, tokens: &Tokens) {
        let mut recovery = self.recovery.lock();
        recovery.mark(TokenProvider::Spotify);
        *tokens.spotify_mut() = None;
    }

    pub fn clear_teams(&self, tokens: &Tokens) {
        let mut recovery = self.recovery.lock();
        recovery.mark(TokenProvider::Teams);
        *tokens.teams_mut() = None;
    }

    pub fn clear_spotify_if_current(
        &self,
        tokens: &Tokens,
        pre_refresh_access_token: &str,
    ) -> bool {
        let mut recovery = self.recovery.lock();
        let mut guard = tokens.spotify_mut();
        if guard
            .as_ref()
            .is_some_and(|tokens| tokens.access_token == pre_refresh_access_token)
        {
            recovery.mark(TokenProvider::Spotify);
            *guard = None;
            true
        } else {
            false
        }
    }

    pub fn clear_teams_if_current(&self, tokens: &Tokens, pre_refresh_access_token: &str) -> bool {
        let mut recovery = self.recovery.lock();
        let mut guard = tokens.teams_mut();
        if guard
            .as_ref()
            .is_some_and(|tokens| tokens.access_token == pre_refresh_access_token)
        {
            recovery.mark(TokenProvider::Teams);
            *guard = None;
            true
        } else {
            false
        }
    }

    pub(crate) fn clear_all(&self, tokens: &Tokens) {
        let mut recovery = self.recovery.lock();
        recovery.spotify = true;
        recovery.teams = true;
        *tokens.spotify_mut() = None;
        *tokens.teams_mut() = None;
        self.mark_ready_locked();
    }

    pub(crate) fn commit_spotify(&self, tokens: &Tokens, value: crate::spotify::SpotifyTokens) {
        let mut recovery = self.recovery.lock();
        recovery.clear(TokenProvider::Spotify);
        *tokens.spotify_mut() = Some(value);
    }

    pub(crate) fn commit_teams(&self, tokens: &Tokens, value: crate::teams::TeamsTokens) {
        let mut recovery = self.recovery.lock();
        recovery.clear(TokenProvider::Teams);
        *tokens.teams_mut() = Some(value);
    }

    pub(crate) fn install_loaded(&self, tokens: &Tokens, loaded: token_io::TokensFile) {
        let mut recovery = self.recovery.lock();
        if loaded.spotify_tokens.is_some() {
            recovery.clear(TokenProvider::Spotify);
        }
        if loaded.teams_tokens.is_some() {
            recovery.clear(TokenProvider::Teams);
        }
        *tokens.spotify_mut() = loaded.spotify_tokens;
        *tokens.teams_mut() = loaded.teams_tokens;
        self.mark_ready_locked();
    }

    pub(crate) fn cas_spotify(
        &self,
        tokens: &Tokens,
        pre_refresh_access_token: &str,
        new_tokens: crate::spotify::SpotifyTokens,
    ) -> TokenCommitOutcome<crate::spotify::SpotifyTokens> {
        let mut recovery = self.recovery.lock();
        let mut guard = tokens.spotify_mut();
        if guard
            .as_ref()
            .map(|t| t.access_token.as_str())
            .is_some_and(|access| access == pre_refresh_access_token)
        {
            recovery.clear(TokenProvider::Spotify);
            *guard = Some(new_tokens.clone());
            TokenCommitOutcome::Committed(new_tokens)
        } else {
            TokenCommitOutcome::Discarded(guard.clone())
        }
    }

    pub(crate) fn cas_teams(
        &self,
        tokens: &Tokens,
        pre_refresh_access_token: &str,
        new_tokens: crate::teams::TeamsTokens,
    ) -> TokenCommitOutcome<crate::teams::TeamsTokens> {
        let mut recovery = self.recovery.lock();
        let mut guard = tokens.teams_mut();
        if guard
            .as_ref()
            .map(|t| t.access_token.as_str())
            .is_some_and(|access| access == pre_refresh_access_token)
        {
            recovery.clear(TokenProvider::Teams);
            *guard = Some(new_tokens.clone());
            TokenCommitOutcome::Committed(new_tokens)
        } else {
            TokenCommitOutcome::Discarded(guard.clone())
        }
    }

    pub(crate) fn provider_matches_spotify(
        &self,
        tokens: &Tokens,
        pre_refresh_access_token: &str,
    ) -> bool {
        let _recovery = self.recovery.lock();
        tokens
            .spotify()
            .as_ref()
            .is_some_and(|t| t.access_token == pre_refresh_access_token)
    }

    pub(crate) fn provider_matches_teams(
        &self,
        tokens: &Tokens,
        pre_refresh_access_token: &str,
    ) -> bool {
        let _recovery = self.recovery.lock();
        tokens
            .teams()
            .as_ref()
            .is_some_and(|t| t.access_token == pre_refresh_access_token)
    }
}

/// Issue #758 slice 2: per-`AppState` tray + config caches and mirrors.
///
/// The tray throttled caches (`devices`/`queue`), the post-action fetch
/// instants, the dedup snapshot, the window/playing/mode mirrors, the delayed
/// refresh coalescing guard, and the config quarantine/conflict flags used to
/// live as module-level statics, which forced test-wide locks and meant two
/// `AppState`s in one process shared one tray. They now live here, owned by
/// `AppState` (see `AppState::caches`): each test constructs its own
/// `AppCaches::new()`, and production paths reach them via `state.caches`.
///
/// Genuinely process-wide handles stay statics (issue #758 slice 3 keeps
/// them): the tray icon handle (`tray::TRAY`), the tray write serialization
/// lock (`tray::TRAY_WRITE_LOCK`), `macos_deeplink::CLAIMED`, and the
/// keychain/HTTP client caches. The locale table (`i18n::LocaleState`) moved
/// onto `AppState::locale` in slice 3, so no test-wide lock survives.
///
/// Lock shapes match the previous statics exactly (`parking_lot` mutexes for
/// the cache slots and dedup snapshot, atomics elsewhere), so the
/// single-session behaviour is unchanged. All fields are private; the
/// `*_slot` / `*_flag` accessors are the only path to the inner data.
pub struct AppCaches {
    devices_cache: Mutex<Option<(Instant, Vec<crate::spotify::DeviceInfo>)>>,
    queue_cache: Mutex<Option<(Instant, crate::spotify::QueueInfo)>>,
    last_tray_action: Mutex<Option<Instant>>,
    last_action_fetch: Mutex<Option<Instant>>,
    last_tray_state: Mutex<Option<crate::tray::TrayStateSnapshot>>,
    window_visible: AtomicBool,
    last_playing_state: AtomicBool,
    last_shuffle_state: AtomicBool,
    last_repeat_state: AtomicU8,
    delayed_refresh_in_flight: AtomicBool,
    config_quarantined: AtomicBool,
    conflict_event_sent: AtomicBool,
}

impl AppCaches {
    pub(crate) fn new() -> Self {
        Self {
            devices_cache: Mutex::new(None),
            queue_cache: Mutex::new(None),
            last_tray_action: Mutex::new(None),
            last_action_fetch: Mutex::new(None),
            last_tray_state: Mutex::new(None),
            window_visible: AtomicBool::new(false),
            last_playing_state: AtomicBool::new(false),
            last_shuffle_state: AtomicBool::new(false),
            last_repeat_state: AtomicU8::new(crate::spotify::RepeatState::Off as u8),
            delayed_refresh_in_flight: AtomicBool::new(false),
            config_quarantined: AtomicBool::new(false),
            conflict_event_sent: AtomicBool::new(false),
        }
    }
    pub(crate) fn devices_slot(
        &self,
    ) -> &Mutex<Option<(Instant, Vec<crate::spotify::DeviceInfo>)>> {
        &self.devices_cache
    }
    pub(crate) fn queue_slot(&self) -> &Mutex<Option<(Instant, crate::spotify::QueueInfo)>> {
        &self.queue_cache
    }
    pub(crate) fn last_tray_action_slot(&self) -> &Mutex<Option<Instant>> {
        &self.last_tray_action
    }
    pub(crate) fn last_action_fetch_slot(&self) -> &Mutex<Option<Instant>> {
        &self.last_action_fetch
    }
    pub(crate) fn last_tray_state_slot(&self) -> &Mutex<Option<crate::tray::TrayStateSnapshot>> {
        &self.last_tray_state
    }
    pub(crate) fn window_visible_flag(&self) -> &AtomicBool {
        &self.window_visible
    }
    pub(crate) fn playing_flag(&self) -> &AtomicBool {
        &self.last_playing_state
    }
    pub(crate) fn shuffle_flag(&self) -> &AtomicBool {
        &self.last_shuffle_state
    }
    pub(crate) fn repeat_flag(&self) -> &AtomicU8 {
        &self.last_repeat_state
    }
    pub(crate) fn delayed_refresh_flag(&self) -> &AtomicBool {
        &self.delayed_refresh_in_flight
    }
    pub(crate) fn quarantined_flag(&self) -> &AtomicBool {
        &self.config_quarantined
    }
    pub(crate) fn conflict_sent_flag(&self) -> &AtomicBool {
        &self.conflict_event_sent
    }
}

impl Default for AppCaches {
    /// Required by `clippy::new_without_default`. Equivalent to
    /// `AppCaches::new()`.
    fn default() -> Self {
        Self::new()
    }
}

pub struct AppState {
    pub tokens: Tokens,
    pub polling: Polling,
    pub pending: PendingAuths,
    pub config: Config,
    pub onboarding_cache: OnboardingCache,
    /// Issue #867: cached Outlook calendar gate. Shared between the polling
    /// loop (consults `busy_at` + `next_boundary`) and the tray snooze
    /// handler (consults `meeting_active` + `current_meeting_end` for the
    /// "Until this meeting ends" preset).
    pub calendar: crate::calendar::CalendarGate,
    /// Per-launch OAuth anti-hijack binding (`pkce::LaunchBinding`), held in
    /// memory only (`OnceLock`), never persisted. Two layers (issue #66;
    /// scope-3.3 §C1): the per-launch secret is bound into the OAuth `state`
    /// param as `<csrf>.<launch_secret>` — Spotify echoes `state` verbatim, so
    /// `handle_deep_link` can reject foreign callbacks — and the SHA-256 of the
    /// in-flight flow's PKCE verifier is bound at authorize time and consumed
    /// (single-use) at callback time, so a replayed callback fails
    /// closed per RFC 6749 §10.12:
    /// https://datatracker.ietf.org/doc/html/rfc6749#section-10.12 .
    /// **macOS:** the `presencejam://`
    /// scheme is registered at build time via `tauri.conf.json`, and
    /// `tauri-plugin-deep-link`'s runtime `register_all()` is unsupported
    /// there. `macos_deeplink` closes that gap by re-claiming the scheme
    /// through CoreServices' `LSSetDefaultHandlerForURLScheme` at every
    /// launch, which overrides a hostile app's earlier registration.
    pub launch_binding: OnceLock<crate::pkce::LaunchBinding>,
    /// Issue #819: whether `setup_tray` succeeded this session. Set once from
    /// the `setup_tray` result in the setup hook (GNOME without the
    /// AppIndicator extension — or a host missing libayatana-appindicator3 —
    /// has no tray, in which case close-to-tray must not engage: a hidden
    /// window would be unreachable). Defaults to true so unit-constructed
    /// states and CLI/daemon paths (no tray setup either way) keep the
    /// established hide-on-close behaviour.
    pub tray_available: AtomicBool,
    /// Single-flight gate for OAuth callbacks (issue #799): the first
    /// delivery of a `(code, state)` pair wins; an identical repeat inside
    /// the dedup window is dropped before any token exchange is spawned.
    /// See `DeepLinkDedup`.
    pub deep_link_seen: DeepLinkDedup,

    /// Transient protection for recoverable token ciphertext after a
    /// platform-keychain-unavailable startup read (issue #935).
    pub tokens_load: TokensLoadGate,
    /// Issue #813: replayable record of the startup legacy-plaintext
    /// migration conflict (`config::LegacySecretOutcome::ConflictKeychainDiffers`).
    /// The `spotify-secret-conflict` event is emitted from the setup hook
    /// before any webview has mounted (issue #376), so no listener can ever
    /// observe it; the flag persists the outcome on process state instead,
    /// and `get_sync_status` surfaces it for late mounters. Cleared when a
    /// Spotify reconnect succeeds.
    pub secret_conflict: AtomicBool,
    /// Issue #1126: last assembled `SyncStatus`, served when a token slot
    /// is contended. A per-`AppState` field (not a process static) so
    /// unit-constructed states each own their cache and the CLI/daemon
    /// paths get the same fallback with no shared mutable state; updated
    /// only on the fresh path, never from the fallback arm itself.
    pub last_sync_snapshot: RwLock<Option<crate::commands::sync::SyncStatus>>,
    /// Issue #758: per-session polling state (write-decision clocks,
    /// quiet/snooze latches, last-now-playing cache, preferred-presence
    /// session, exit snapshot, diagnostics mirrors). Owned here so each
    /// test constructs an isolated session and production session
    /// boundaries are per-session resets, not global resets.
    pub session: crate::polling::SessionState,
    /// Issue #758 slice 3 (final): the installed native-surface locale table.
    /// Owned here so each test installs its own language and two `AppState`s
    /// never share one table; the tray/menu builders render from the state
    /// they already hold instead of a process-wide static.
    pub locale: crate::i18n::LocaleState,
    /// Issue #758 slice 2: per-`AppState` tray + config caches and mirrors
    /// (throttled devices/queue caches, post-action fetch instants, dedup
    /// snapshot, window/playing/mode mirrors, delayed-refresh guard,
    /// quarantine/conflict flags). Owned here so each test constructs
    /// isolated caches and two `AppState`s never share one tray.
    pub caches: AppCaches,
}

impl AppState {
    pub fn new() -> Self {
        log::info!("[APP_STATE] AppState::new: creating new AppState");
        let launch_binding = OnceLock::new();
        // Initialise per-launch binding immediately so every AppState
        // (including those created in tests) has one. Production startup via
        // `run()` will have already set it, but `OnceLock::set` is harmless if
        // already initialised — we ignore the error.
        let _ = launch_binding.set(crate::pkce::LaunchBinding::new(
            crate::pkce::generate_launch_secret(),
        ));
        Self {
            tokens: Tokens::new(),
            polling: Polling::new(),
            pending: PendingAuths::new(),
            config: Config::new(),
            onboarding_cache: OnboardingCache::new(),
            calendar: crate::calendar::CalendarGate::new(),
            launch_binding,
            tray_available: AtomicBool::new(true),
            deep_link_seen: DeepLinkDedup::new(),
            tokens_load: TokensLoadGate::new(),
            secret_conflict: AtomicBool::new(false),
            last_sync_snapshot: RwLock::new(None),
            session: crate::polling::SessionState::new(),
            locale: crate::i18n::LocaleState::new(),
            caches: AppCaches::new(),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        // Routes through `new()` so the AppState creation log line still fires.
        Self::new()
    }
}

/// Apply one token-store read to process state. A successful read unblocks
/// persistence and installs the recovered snapshot. A platform-unavailable
/// read blocks persistence while leaving the encrypted file untouched. Any
/// other read failure is treated as a corrupt store: token state stays empty,
/// but normal recovery/re-auth persistence remains available.
pub(crate) fn apply_token_load_result(
    state: &AppState,
    result: Result<token_io::TokensFile, token_io::TokensLoadError>,
) {
    match result {
        Ok(tf) => {
            let has_spotify = tf.spotify_tokens.is_some();
            let has_teams = tf.teams_tokens.is_some();
            // The gate installs both slots under its recovery-marker mutex.
            // Keep the load flags captured before the gate consumes the file.
            state.tokens_load.install_loaded(&state.tokens, tf);
            if has_spotify {
                log::info!("[APP] setup: spotify_tokens loaded into AppState");
            } else {
                log::info!("[APP] setup: no spotify_tokens in tokens.json");
            }
            if has_teams {
                log::info!("[APP] setup: teams_tokens loaded into AppState");
            } else {
                log::info!("[APP] setup: no teams_tokens in tokens.json");
            }
        }
        Err(token_io::TokensLoadError::KeychainUnavailable(message)) => {
            state.tokens_load.mark_keychain_unavailable();
            log::warn!("[APP] setup: token store deferred: {}", message);
        }
        Err(token_io::TokensLoadError::Corrupt(message)) => {
            // `clear_all` owns both tombstones and slot writes.
            state
                .tokens_load
                .install_loaded(&state.tokens, token_io::TokensFile::default());
            log::warn!("[APP] setup: failed to load tokens.json: {}", message);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_onboarding_cache_lock_and_invalidate() {
        // Issue #80: OnboardingCache is now its own sub-struct with a
        // load-bearing lock() method. The lock() / invalidate() API
        // must be the only way to reach the inner state from outside
        // the sub-struct — direct access to the `state` field would
        // defeat the encapsulation that the struct-of-states refactor
        // is supposed to give us.
        let cache = OnboardingCache::new();

        // Initially cold: lock returns None.
        assert!(cache.lock().is_none(), "fresh cache must be cold");

        // Write a value via lock() and read it back.
        *cache.lock() = Some((Instant::now(), true));
        let guard = cache.lock();
        assert!(guard.is_some(), "value written via lock() must round-trip");
        let (ts, result) = guard.unwrap();
        assert!(result, "value must be the one we wrote");
        // Timestamp must be recent (within the last 5s).
        assert!(
            ts.elapsed() < std::time::Duration::from_secs(5),
            "timestamp should be ~now, not stale"
        );
        drop(guard);

        // invalidate() must clear the cache back to None.
        cache.invalidate();
        assert!(
            cache.lock().is_none(),
            "invalidate() must reset cache to None"
        );
    }

    #[test]
    fn test_onboarding_cache_encapsulation_no_direct_state_access() {
        // Regression guard for issue #80: a future contributor must
        // not re-expose the `state` field as `pub`. The struct-of-
        // states refactor relies on every AppState sub-struct hiding
        // its inner mutex behind a method. If someone makes
        // `OnboardingCache::state` public, callers can bypass
        // invalidate() and lock() — and the "lock must not be held
        // across await" future invariant becomes unenforceable.
        let source = include_str!("state.rs");
        // Find the OnboardingCache struct definition.
        let start = source
            .find("pub struct OnboardingCache")
            .expect("state.rs must contain OnboardingCache struct");
        let end = source[start..]
            .find("\n}\n")
            .map(|i| start + i + 2)
            .expect("OnboardingCache struct must end with closing brace");
        let struct_body = &source[start..end];
        // The `state` field must be private (no `pub` keyword on the
        // field declaration). We grep for `pub state:` to detect a
        // regression. Whitespace-tolerant.
        assert!(
            !struct_body
                .lines()
                .any(|l| l.trim_start().starts_with("pub state")),
            "OnboardingCache::state must remain private. The struct-of-states \
                 refactor (#80) relies on the inner mutex being hidden behind a \
                 method (lock/invalidate). Found 'pub state' in the struct body:\n{}",
            struct_body
        );
    }

    #[test]
    fn test_tokens_sub_struct_lock_and_invalidate() {
        // Issue #80 step 2: Tokens is now its own sub-struct with
        // private inner fields. All access goes through `spotify()` /
        // `teams()` / `spotify_mut()` / `teams_mut()`. This test
        // exercises the public API: cold state, write/read round-trip,
        // and the take pattern used by handle_spotify_callback.
        use crate::spotify::SpotifyTokens;
        use crate::teams::TeamsTokens;

        let tokens = Tokens::new();

        // Cold state: both guards return None.
        assert!(
            tokens.spotify().is_none(),
            "fresh Tokens must have no Spotify token"
        );
        assert!(
            tokens.teams().is_none(),
            "fresh Tokens must have no Teams token"
        );

        // Write a Spotify token via spotify_mut(); read back via spotify().
        *tokens.spotify_mut() = Some(SpotifyTokens {
            access_token: "spotify-access".to_string(),
            refresh_token: "spotify-refresh".to_string(),
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(3600),
        });
        {
            let guard = tokens.spotify();
            let read = guard.as_ref().expect("Spotify token must round-trip");
            assert_eq!(read.access_token, "spotify-access");
        }

        // Same round-trip for Teams.
        *tokens.teams_mut() = Some(TeamsTokens {
            access_token: "teams-access".to_string(),
            refresh_token: Some("teams-refresh".to_string()),
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(3600),
        });
        {
            let guard = tokens.teams();
            let read = guard.as_ref().expect("Teams token must round-trip");
            assert_eq!(read.access_token, "teams-access");
        }

        // Take pattern (used by handle_spotify_callback):
        // pulling the Option out leaves the slot None.
        let taken = tokens.spotify_mut().take();
        assert!(
            taken.is_some(),
            "take() must return the stored Spotify token"
        );
        assert!(
            tokens.spotify().is_none(),
            "take() must leave the Spotify slot empty"
        );
    }

    #[test]
    fn conditional_token_clear_replacement_wins_but_matching_stale_token_clears() {
        let state = AppState::new();
        let spotify = |access: &str| crate::spotify::SpotifyTokens {
            access_token: access.to_string(),
            refresh_token: "spotify-refresh".to_string(),
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        };
        let teams = |access: &str| crate::teams::TeamsTokens {
            access_token: access.to_string(),
            refresh_token: Some("teams-refresh".to_string()),
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        };

        state
            .tokens_load
            .commit_spotify(&state.tokens, spotify("old-spotify"));
        state
            .tokens_load
            .commit_spotify(&state.tokens, spotify("new-spotify"));
        assert!(!state
            .tokens_load
            .clear_spotify_if_current(&state.tokens, "old-spotify"));
        assert_eq!(
            state
                .tokens
                .spotify()
                .as_ref()
                .map(|tokens| tokens.access_token.as_str()),
            Some("new-spotify"),
            "a replacement must win Spotify's conditional clear",
        );
        assert!(state
            .tokens_load
            .clear_spotify_if_current(&state.tokens, "new-spotify"));
        assert!(state.tokens.spotify().is_none());

        state
            .tokens_load
            .commit_teams(&state.tokens, teams("old-teams"));
        state
            .tokens_load
            .commit_teams(&state.tokens, teams("new-teams"));
        assert!(!state
            .tokens_load
            .clear_teams_if_current(&state.tokens, "old-teams"));
        assert_eq!(
            state
                .tokens
                .teams()
                .as_ref()
                .map(|tokens| tokens.access_token.as_str()),
            Some("new-teams"),
            "a replacement must win Teams' conditional clear",
        );
        assert!(state
            .tokens_load
            .clear_teams_if_current(&state.tokens, "new-teams"));
        assert!(state.tokens.teams().is_none());
    }

    #[test]
    fn test_polling_sub_struct_lock_and_invalidate() {
        use std::sync::mpsc;
        let polling = Polling::new();
        assert!(!polling.is_syncing());
        assert!(polling.current_track().is_none());
        assert!(polling.handle().is_none());
        assert!(polling.stop_tx().is_none());
        assert!(polling.try_claim());
        assert!(polling.is_syncing());
        assert!(!polling.try_claim());
        polling.set_syncing(false);
        assert!(!polling.is_syncing());
        let (tx, _rx) = mpsc::channel::<()>();
        *polling.stop_tx_mut() = Some(tx);
        assert!(polling.stop_tx().is_some());
        let handle = std::thread::Builder::new().spawn(|| {}).expect("spawn");
        *polling.handle_mut() = Some(handle);
        assert!(polling.handle().is_some());
    }

    /// Issue #759: the sync flag hardcodes Acquire-load / Release-store
    /// (no `Ordering` parameter). A Release store of `true` must be
    /// observable from another thread's Acquire load — the
    /// poller-exit-then-observe chain the menu/tray rebuilds rely on —
    /// and clearing the flag the same way must read back `false`.
    #[test]
    fn test_polling_sync_flag_publish_then_observe() {
        use std::sync::Arc;
        let polling = Arc::new(Polling::new());
        assert!(!polling.is_syncing());
        let polling_writer = Arc::clone(&polling);
        let writer = std::thread::spawn(move || {
            polling_writer.set_syncing(true);
        });
        writer.join().expect("writer thread panicked");
        assert!(polling.is_syncing());
        polling.set_syncing(false);
        assert!(!polling.is_syncing());
        assert!(polling.try_claim());
        assert!(polling.is_syncing());
    }

    /// Issue #813 acceptance: the setup hook must persist a
    /// `ConflictKeychainDiffers` migration outcome on `AppState::secret_conflict`
    /// (the one-shot `spotify-secret-conflict` event fires before any webview
    /// has mounted, so nothing persists it otherwise), and both auth-completion
    /// paths must clear the flag again. Fails pre-fix: no such store exists.
    #[test]
    fn test_secret_conflict_flag_survives_setup_and_clears_on_reconnect() {
        use std::sync::atomic::Ordering;
        let app_source = include_str!("app.rs");
        let app_prod = app_source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("app.rs has no #[cfg(test)] mod tests block");
        // Wiring half: setup persists the conflict outcome onto process state.
        for marker in [
            "migrate_legacy_client_secret_with_app(",
            "LegacySecretOutcome::ConflictKeychainDiffers",
            "secret_conflict",
        ] {
            assert!(
                app_prod.contains(marker),
                "setup must persist the migration conflict on AppState ({marker}) (issue #813)"
            );
        }
        // Clearing half: both auth-completion paths share the
        // `commit_spotify_session` seam, which clears the replayable flag.
        // The deep-link path reaches it through deep_link.rs and the manual
        // fallback through commands/spotify_auth.rs (both must, or the
        // banner survives a reconnect on one path).
        let dl_source = include_str!("deep_link.rs");
        let dl_prod = dl_source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("deep_link.rs has no #[cfg(test)] mod tests block");
        assert!(
            dl_prod.contains("commit_spotify_session"),
            "handle_spotify_callback must route through the seam that clears the flag (issue #813)"
        );
        let seam_source = include_str!("commands/spotify_auth.rs");
        let seam_prod = seam_source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("spotify_auth.rs has no #[cfg(test)] mod tests block");
        assert!(
            seam_prod.contains("secret_conflict"),
            "commit_spotify_session must clear the flag on success (issue #813)"
        );
        // Behaviour half: the flag defaults off.
        let state = AppState::new();
        assert!(
            !state.secret_conflict.load(Ordering::Acquire),
            "a fresh AppState must report no secret conflict (issue #813)"
        );
    }

    #[test]
    fn test_app_state_sub_encapsulation_no_pub_inner_fields() {
        let source = include_str!("state.rs");
        for name in ["Tokens", "Polling", "PendingAuths", "Config"] {
            let header = format!("pub struct {}", name);
            let start = source
                .find(&header)
                .unwrap_or_else(|| panic!("missing struct {}", name));
            let end = source[start..]
                .find("\n}\n")
                .map(|i| start + i + 2)
                .unwrap_or_else(|| panic!("no closing brace for {}", name));
            let body = &source[start..end];
            for line in body.lines() {
                let t = line.trim_start();
                if t.starts_with("pub ") && t.contains(':') && !t.starts_with("pub struct") {
                    panic!("{} has pub field `{}`; must stay private", name, t);
                }
            }
        }
    }

    /// Issue #758 slice 2: two `AppState`s never share one tray. Quarantining
    /// through one state's caches raises only its flag; the other's stays
    /// down. Fails pre-fix by construction: a single process-wide
    /// `CONFIG_QUARANTINED` static is raised no matter which state loads.
    #[test]
    fn test_two_app_states_do_not_share_caches() {
        use std::sync::atomic::Ordering;
        let first = AppState::new();
        let second = AppState::new();
        first
            .caches
            .quarantined_flag()
            .store(true, Ordering::SeqCst);
        assert!(
            crate::config::config_was_quarantined(&first.caches),
            "the quarantined state must observe its own flag"
        );
        assert!(
            !crate::config::config_was_quarantined(&second.caches),
            "a second AppState must not observe the first one's quarantine"
        );
        crate::tray::note_window_visibility(&first.caches, true);
        assert!(
            crate::tray::window_visible(&first.caches),
            "the first state's mirror must report what was recorded"
        );
        assert!(
            !crate::tray::window_visible(&second.caches),
            "the second state's mirror must stay at its default"
        );
    }
}
