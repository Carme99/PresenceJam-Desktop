use parking_lot::{Mutex, RwLock};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::OnceLock;
use std::thread;
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager};

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

/// Persistent user config (`AppConfig`). `set()` is provided so the
/// save_config read-modify-write path can hold one write guard for the
/// whole critical section without naming the inner lock.
pub struct Config {
    config: RwLock<Option<crate::config::AppConfig>>,
}

impl Config {
    pub fn new() -> Self {
        Self {
            config: RwLock::new(None),
        }
    }

    /// Read guard for the config slot. Use this instead of touching
    /// the `config` field directly.
    pub fn get(&self) -> parking_lot::RwLockReadGuard<'_, Option<crate::config::AppConfig>> {
        self.config.read()
    }

    /// Write guard for the config slot. Use this instead of touching
    /// the `config` field directly.
    pub fn get_mut(&self) -> parking_lot::RwLockWriteGuard<'_, Option<crate::config::AppConfig>> {
        self.config.write()
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
/// stop-channel sender, and the last observed track. The atomic flag
/// keeps its original orderings (Acquire load, Release store, AcqRel
/// compare-exchange) so the happens-before chain with the polling
/// loop and tray menu stays identical to the pre-refactor code.
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

    /// Load the sync flag with the caller's chosen ordering. Preserves
    /// the original `state.is_syncing.load(Ordering::Acquire)` semantics.
    pub fn is_syncing(&self, ordering: std::sync::atomic::Ordering) -> bool {
        self.is_syncing.load(ordering)
    }

    /// Store a new value into the sync flag with the caller's chosen
    /// ordering. Preserves the original `state.is_syncing.store(.., Ordering::Release)`
    /// semantics.
    pub fn set_syncing(&self, value: bool, ordering: std::sync::atomic::Ordering) {
        self.is_syncing.store(value, ordering);
    }

    /// Attempt to atomically claim the sync flag (false -> true). Returns
    /// `true` if this caller won the claim, `false` if the flag was
    /// already set. Uses AcqRel on success and Acquire on failure so the
    /// happens-before relationship with subsequent reads of `is_syncing`
    /// (polling loop, tray menu) is preserved exactly. This is the only
    /// site that does the CAS-equivalent operation; `polling.rs` itself
    /// is intentionally CAS-free (see the regression guard at the
    /// bottom of `polling.rs::tests`).
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
const DEEP_LINK_DEDUP_WINDOW: std::time::Duration = std::time::Duration::from_secs(10);

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
fn deep_link_callback_key(code: &str, state: Option<&str>) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    code.hash(&mut hasher);
    state.unwrap_or("").hash(&mut hasher);
    hasher.finish()
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
    /// Single-flight gate for OAuth callbacks (issue #799): the first
    /// delivery of a `(code, state)` pair wins; an identical repeat inside
    /// the dedup window is dropped before any token exchange is spawned.
    /// See `DeepLinkDedup`.
    pub deep_link_seen: DeepLinkDedup,
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
            deep_link_seen: DeepLinkDedup::new(),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        // Routes through `new()` so the AppState creation log line still fires.
        Self::new()
    }
}

pub mod calendar;
pub mod commands;
pub mod config;
pub mod diagnostics;
pub mod history;
pub mod i18n;
pub mod keychain;
pub mod macos_deeplink;
pub mod menu;
pub mod pkce;
pub mod platform;
pub mod polling;
pub mod profanity;
pub mod serve;
pub mod sources;
pub mod spotify;
pub mod teams;
pub mod token_io;
pub mod tray;
pub mod updater_bg;

async fn handle_spotify_callback(
    code: &str,
    state_param: Option<&str>,
    app: &AppHandle,
) -> Result<(), String> {
    log::debug!(
        "[CALLBACK] handle_spotify_callback: ENTRY - code.len={}",
        code.len()
    );

    let app_state = app.state::<Arc<AppState>>();
    log::info!("[CALLBACK] handle_spotify_callback: got app state");

    // Issue #555: peek, do not take. A callback that fails validation (expired
    // pending, replayed state, transient exchange failure) must leave the
    // pending in place so the same flow stays retryable instead of destroying
    // it on the way to an error the user cannot act on. The pending is claimed
    // below, once the code has actually been redeemed.
    let pending = {
        let guard = app_state.pending.spotify();
        guard.as_ref().cloned().ok_or_else(|| {
            log::error!("[CALLBACK] handle_spotify_callback: No pending Spotify auth found");
            "No pending Spotify auth".to_string()
        })?
    };
    log::info!(
        "[CALLBACK] handle_spotify_callback: pending auth found - verifier.len={}",
        pending.verifier.len()
    );

    // Re-check expiry at submit time. The expiry was set on creation
    // (lib.rs setup, or commands.rs::start_spotify_auth) but only
    // consulted on disk-load. If the OS suspended the process for
    // >10 minutes, the auth code may now be rejected by Spotify as
    // expired. See issue #34.
    if pending.expires_at < chrono::Utc::now() {
        log::error!("[CALLBACK] handle_spotify_callback: auth state expired at submit time");
        return Err("Auth state expired — please try signing in again.".to_string());
    }

    // Verify state matches to prevent CSRF attacks
    if let Some(state_str) = state_param {
        if !crate::pkce::ct_eq(state_str, &pending.state) {
            log::error!(
                "[CALLBACK] handle_spotify_callback: state mismatch - CSRF attack detected"
            );
            return Err("State mismatch - possible CSRF attack".to_string());
        }
        log::info!("[CALLBACK] handle_spotify_callback: state verified successfully");
    } else {
        log::error!("[CALLBACK] handle_spotify_callback: missing state parameter in callback URL");
        return Err("Missing state parameter - possible CSRF attack".to_string());
    }

    // Note on #66: deep-link interception by another app is mitigated by
    // the verifier being in AppState only (#65) and, at the OS level, by
    // re-claiming the scheme at every launch — `macos_deeplink` on macOS,
    // `tauri-plugin-deep-link`'s `register_all()` on Windows/Linux.
    // Fetch the client_secret from the OS keychain. It was placed there by
    // `start_spotify_auth` and is never persisted to disk. See issue #9.
    log::info!("[CALLBACK] handle_spotify_callback: reading client_secret from keychain");
    let client_secret = crate::keychain::get_spotify_client_secret()?;

    log::info!(
        "[CALLBACK] handle_spotify_callback: calling complete_spotify_auth (on blocking pool)"
    );
    // The HTTP round-trip uses reqwest::blocking internally; offload it to
    // Tauri's blocking pool so we don't pin an async worker for the full call.
    let code = code.to_string();
    let verifier = pending.verifier.clone();
    let client_id = pending.client_id.clone();
    let client_secret = client_secret.clone();
    let redirect_uri = pending.redirect_uri.clone();
    let exchange = tauri::async_runtime::spawn_blocking(move || {
        crate::spotify::complete_spotify_auth(
            &code,
            &verifier,
            &client_id,
            &client_secret,
            &redirect_uri,
        )
    })
    .await;
    // #555: a failed exchange must not burn the flow — the authorization code
    // was never redeemed, so put the pending back and let the user retry the
    // same callback instead of starting over from a fresh consent screen.
    let tokens = match exchange {
        Ok(Ok(tokens)) => tokens,
        Ok(Err(e)) => {
            crate::commands::spotify_auth::restore_pending_after_failed_exchange(
                &app_state, pending,
            );
            log::error!(
                "[CALLBACK] handle_spotify_callback: token exchange failed - {}",
                e
            );
            return Err(e);
        }
        Err(e) => {
            crate::commands::spotify_auth::restore_pending_after_failed_exchange(
                &app_state, pending,
            );
            let msg = format!("Spotify OAuth callback task failed: {}", e);
            log::error!("[CALLBACK] handle_spotify_callback: {}", msg);
            return Err(msg);
        }
    };
    log::info!(
        "[CALLBACK] handle_spotify_callback: token exchange successful - access_token.len={}",
        tokens.access_token.len()
    );

    // The code is redeemed, so claim the pending for this flow and consume the
    // single-use launch binding — and only now (#555). A mismatch means a
    // concurrent flow replaced the pending while the exchange was in flight:
    // restore what we took and fail closed rather than claiming another flow's
    // pending (#351).
    {
        let mut guard = app_state.pending.spotify_mut();
        match guard.take() {
            Some(taken) if crate::pkce::ct_eq(&taken.state, &pending.state) => {}
            Some(taken) => {
                *guard = Some(taken);
                log::error!(
                    "[CALLBACK] handle_spotify_callback: pending changed during the exchange"
                );
                return Err("No pending Spotify auth".to_string());
            }
            None => {
                log::error!("[CALLBACK] handle_spotify_callback: pending consumed concurrently");
                return Err("No pending Spotify auth".to_string());
            }
        }
    }
    if let Some(binding) = app_state.launch_binding.get() {
        let secret_component = pending.state.rsplit('.').next().unwrap_or("");
        if let Err(reason) = binding.validate_and_consume(secret_component, &pending.verifier) {
            log::warn!(
                "[CALLBACK] handle_spotify_callback: launch binding not consumed after a successful exchange ({}) [REDACTED]",
                reason
            );
        }
    }

    {
        let mut guard = app_state.tokens.spotify_mut();
        *guard = Some(tokens.clone());
        log::info!("[CALLBACK] handle_spotify_callback: tokens stored in AppState");
    }
    token_io::persist_tokens(&app_state, app)?;
    log::info!("[CALLBACK] handle_spotify_callback: tokens persisted atomically");

    // Issue #70: invalidate the onboarding cache.
    app_state.onboarding_cache.invalidate();
    log::info!("[CALLBACK] handle_spotify_callback: onboarding_cache invalidated");

    log::info!("[CALLBACK] handle_spotify_callback: EMIT spotify-auth-complete event");
    let _ = app.emit("spotify-auth-complete", ());

    log::info!("[CALLBACK] handle_spotify_callback: SUCCESS");
    Ok(())
}

fn handle_deep_link(url: &str, app: AppHandle) {
    // #228: never log raw callback URL (contains code + state). Log only length/prefix.
    log::debug!(
        "[DEEP_LINK] handle_deep_link: ENTRY - url_len={} prefix={}…[REDACTED]",
        url.len(),
        url.chars().take(4).collect::<String>()
    );

    if let Ok(parsed) = url::Url::parse(url) {
        log::info!("[DEEP_LINK] handle_deep_link: URL parsed successfully");
        let scheme = parsed.scheme();
        log::info!("[DEEP_LINK] handle_deep_link: scheme={}", scheme);

        if scheme == "presencejam" {
            log::info!("[DEEP_LINK] handle_deep_link: recognized as presencejam scheme");

            let code = parsed
                .query_pairs()
                .find(|(k, _)| k == "code")
                .map(|(_, v)| v.to_string());
            let state_param = parsed
                .query_pairs()
                .find(|(k, _)| k == "state")
                .map(|(_, v)| v.to_string());

            if let Some(code_str) = code {
                log::info!(
                    "[DEEP_LINK] handle_deep_link: code found - code.len={}",
                    code_str.len()
                );
                // Issue #799: single-flight per (code, state). A Win/Linux
                // second-instance launch reaches the running instance through
                // `handle_cli_arguments` → `deep-link://new-url` →
                // `on_open_url` → `handle_deep_link`; the argv scan that used
                // to re-dispatch it from `forward_launch_to_running_instance`
                // is gone, and this gate drops any identical repeat that still
                // arrives inside the dedup window (e.g. a `get_current` start
                // URL followed by the same `on_open_url` event) before a
                // second token exchange can be spawned. Checked before
                // validation on purpose: a repeat is byte-identical, so
                // whatever the first delivery decides applies to it too.
                let callback_key = deep_link_callback_key(&code_str, state_param.as_deref());
                {
                    let app_state = app.state::<Arc<AppState>>();
                    if !app_state.deep_link_seen.claim(callback_key) {
                        log::info!(
                            "[DEEP_LINK] handle_deep_link: duplicate callback inside the dedup window — dropping before any token exchange"
                        );
                        return;
                    }
                }
                // #66 option b + scope-3.3 §C1: per-launch secret bound into
                // state as `<csrf>.<launch_secret>`. Spotify echoes state
                // verbatim (https://developer.spotify.com/documentation/web-api/tutorials/code-flow),
                // so we can validate the callback against the binding stored in
                // AppState (OnceLock, in-memory only). Defense-in-depth layers,
                // all fail-closed before any token exchange:
                //   1. Strict structure: exactly `<csrf>.<secret>`, both
                //      non-empty base64url components — rejects truncated or
                //      malformed states.
                //   2. Constant-time compare of the full echoed state against
                //      the stored pending state (`pkce::ct_eq`, no early-exit
                //      content leak).
                //   3. PKCE linkage: the launch binding's verifier hash (bound
                //      at authorize time) must match the pending auth's
                //      verifier.
                //   4. Single-use consumption (RFC 6749 §10.12
                //      https://datatracker.ietf.org/doc/html/rfc6749#section-10.12):
                //      the check here is the NON-consuming
                //      `LaunchBinding::validate` — it must reject a foreign or
                //      replayed callback before any exchange, yet leave the
                //      slot intact so a *transient* exchange failure stays
                //      retryable (#555). The consumption happens in
                //      `handle_spotify_callback`, after the code was redeemed,
                //      and only there. The full state compare + `take()` of the
                //      pending auth also stay in `handle_spotify_callback`.
                // Scheme stays `presencejam://`
                // because macOS bundle scheme registration is config-time only (Info.plist) —
                // runtime re-registration is not supported, so hijack remains possible on macOS
                // but the intercepted `code` is useless without secret + PKCE verifier.
                // #228: redact state in logs — never log raw values.
                if let Some(st) = &state_param {
                    let secret_ok = {
                        let app_state = app.state::<Arc<AppState>>();
                        match app_state.launch_binding.get() {
                            Some(binding) => {
                                let parts: Vec<&str> = st.splitn(2, '.').collect();
                                if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty()
                                {
                                    // Peek (do not take) the pending auth: consumption of the
                                    // pending itself stays in handle_spotify_callback.
                                    let pending_peek = app_state.pending.spotify_mut();
                                    match pending_peek.as_ref() {
                                        Some(pending) if crate::pkce::ct_eq(st, &pending.state) => {
                                            match binding.validate(parts[1], &pending.verifier) {
                                                Ok(()) => true,
                                                Err(reason) => {
                                                    let prefix: String =
                                                        st.chars().take(4).collect();
                                                    log::warn!(
                                                        "[DEEP_LINK] handle_deep_link: launch binding rejected ({}) — state prefix={}… len={} — ignoring callback (possible hijack/replay) [REDACTED len {}]",
                                                        reason,
                                                        prefix,
                                                        st.len(),
                                                        st.len()
                                                    );
                                                    false
                                                }
                                            }
                                        }
                                        Some(_) => {
                                            let prefix: String = st.chars().take(4).collect();
                                            log::warn!(
                                                "[DEEP_LINK] handle_deep_link: state mismatch vs pending auth — state prefix={}… len={} — ignoring callback (possible hijack) [REDACTED len {}]",
                                                prefix,
                                                st.len(),
                                                st.len()
                                            );
                                            false
                                        }
                                        None => {
                                            log::warn!(
                                                "[DEEP_LINK] handle_deep_link: no pending Spotify auth in AppState — ignoring callback (stale or replayed) [REDACTED len {}]",
                                                st.len()
                                            );
                                            false
                                        }
                                    }
                                } else {
                                    let prefix: String = st.chars().take(4).collect();
                                    log::warn!(
                                        "[DEEP_LINK] handle_deep_link: malformed state (expected <csrf>.<secret>) — state prefix={}… len={} — ignoring callback (possible truncation/hijack) [REDACTED len {}]",
                                        prefix,
                                        st.len(),
                                        st.len()
                                    );
                                    false
                                }
                            }
                            None => {
                                log::warn!(
                                    "[DEEP_LINK] handle_deep_link: no launch binding in AppState — ignoring callback [REDACTED len {}]",
                                    st.len()
                                );
                                false
                            }
                        }
                    };
                    if !secret_ok {
                        return;
                    }
                } else {
                    log::warn!(
                        "[DEEP_LINK] handle_deep_link: missing state in callback — ignoring"
                    );
                    return;
                }
                let app_clone = app.clone();
                let code_clone = code_str.clone();
                let state_clone = state_param.clone();

                log::info!("[DEEP_LINK] handle_deep_link: routing to Spotify callback");
                tauri::async_runtime::spawn(async move {
                    log::info!("[DEEP_LINK] handle_deep_link: spawning Spotify callback handler");
                    if let Err(e) =
                        handle_spotify_callback(&code_clone, state_clone.as_deref(), &app_clone)
                            .await
                    {
                        log::error!("[DEEP_LINK] handle_spotify_callback: FAILED - {}", e);
                        log::info!("[DEEP_LINK] handle_deep_link: EMIT spotify-auth-failed event");
                        let _ = app_clone.emit("spotify-auth-failed", e);
                    }
                });
            } else {
                log::warn!("[DEEP_LINK] handle_deep_link: no code found in URL");
            }
        } else {
            log::warn!("[DEEP_LINK] handle_deep_link: unknown scheme - {}", scheme);
        }
    } else {
        log::error!("[DEEP_LINK] handle_deep_link: failed to parse URL");
    }
}

/// CLI flag the autostart plugin appends to the launch command.
const MINIMIZED_FLAG: &str = "--minimized";

/// True when a close request on `label` hides the window instead of
/// destroying it (issue #585). Only the main window is close-to-tray.
///
/// Detached Logs/Settings panes clear their `$detachedPanes` badge on
/// `tauri://destroyed` (and "Pop back in" awaits `win.close()`), so hiding
/// one leaves a live-but-invisible window whose `setFocus()` can never bring
/// it back — permanently unreachable until the process restarts. Shares the
/// label predicate with the command guard (`commands::is_main_window_label`)
/// so both agree on which window is *the* window.
fn close_hides_window(label: &str) -> bool {
    crate::commands::is_main_window_label(label)
}

/// Issue #922: what `detach_pane` builds for a pane name — the label, the
/// in-app URL, the title and the size, all decided here.
struct DetachedPaneSpec {
    label: &'static str,
    title: &'static str,
    width: f64,
    height: f64,
    url: String,
}

/// The pane table behind `detach_pane`, kept pure so both configured panes and
/// the unknown-name rejection are testable without a live Tauri app.
///
/// `theme` mirrors the child-window theme parameter the store used to append
/// (issue #433). Only the two values the frontend can read out of
/// localStorage are accepted; anything else is treated as absent rather than
/// interpolated into the URL.
fn detached_pane_spec(pane: &str, theme: Option<&str>) -> Result<DetachedPaneSpec, String> {
    let (label, title, width, height) = match pane {
        "logs" => ("logs-detached", "PresenceJam — Logs", 720.0, 520.0),
        "settings" => ("settings-detached", "PresenceJam — Settings", 620.0, 720.0),
        other => return Err(format!("unknown detached pane: {other}")),
    };
    let url = match theme {
        Some("dark") => format!("/detached/{pane}?theme=dark"),
        Some("light") => format!("/detached/{pane}?theme=light"),
        _ => format!("/detached/{pane}"),
    };
    Ok(DetachedPaneSpec {
        label,
        title,
        width,
        height,
        url,
    })
}

/// Issue #922: open (or focus) a detached Logs/Settings window from Rust.
///
/// The window used to be created by `src/lib/stores/detach.ts` through
/// `WebviewWindow`, which required the main window's
/// `core:webview:allow-create-webview-window` grant — a permission that in
/// Tauri 2 carries no URL scope, so any script running in the main window
/// could raise an app-chromed window on an arbitrary origin. Building it here
/// removes that grant: the label, the in-app URL, the title and the size all
/// come from the table above, and a pane name that is not one of the two
/// configured views is rejected outright.
///
/// Idempotent, matching the store's `getByLabel` fast path: an existing window
/// is focused rather than a second one being built under the same label (which
/// Tauri would reject anyway).
#[tauri::command]
fn detach_pane(app: AppHandle, pane: String, theme: Option<String>) -> Result<(), String> {
    let spec = detached_pane_spec(&pane, theme.as_deref())?;
    if let Some(existing) = app.get_webview_window(spec.label) {
        log::info!(
            "[DETACH] detach_pane: focusing the existing {} window",
            spec.label
        );
        return existing
            .set_focus()
            .map_err(|e| format!("failed to focus the {} window: {e}", spec.label));
    }
    log::info!(
        "[DETACH] detach_pane: opening {} at {}",
        spec.label,
        spec.url
    );
    tauri::WebviewWindowBuilder::new(
        &app,
        spec.label,
        tauri::WebviewUrl::App(spec.url.clone().into()),
    )
    .title(spec.title)
    .inner_size(spec.width, spec.height)
    .min_inner_size(400.0, 400.0)
    .center()
    .build()
    .map(|_| ())
    .map_err(|e| format!("failed to open the {} window: {e}", spec.label))
}

/// True when this launch carries the autostart plugin's `--minimized` flag
/// (issue #589). Generic over the argv element type so the parser is
/// unit-testable without touching the real process argv.
fn has_minimized_flag<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    args.into_iter()
        .any(|arg| arg.as_ref() == std::ffi::OsStr::new(MINIMIZED_FLAG))
}

/// The file target's rotation strategy for a clamped `logging.keep_files`.
///
/// **Always `KeepSome`**, including at `1`. The field means "archived log files
/// retained" and the active file is not counted, which is exactly what
/// `KeepSome` implements: its archive pass prunes the dated files down to
/// `keep_files`. `KeepOne` reads like the equivalent at 1 and is not — its
/// rotate branch *deletes* the active log and never calls that pruning pass, so
/// pre-existing archives all survive and the configured retention is ignored.
///
/// `max(1)` guards the plugin's `keep_count - 1` (which underflows at 0);
/// `clamp_logging` already floors the persisted value, this is the belt for a
/// caller that skips the clamp.
fn log_rotation_strategy(keep_files: u32) -> tauri_plugin_log::RotationStrategy {
    tauri_plugin_log::RotationStrategy::KeepSome(keep_files.max(1) as usize)
}

// =====================================================================
// CLI flags (issue #679)
// =====================================================================
//
// `--help`, `--status` and `--sync-once` are resolved at the TOP of `run()`,
// before any Tauri app exists. That placement is load-bearing rather than
// stylistic: the runtime is created by `Builder::build()`, and on a machine
// with no display (a cron host, a CI box, `XDG_SESSION_TYPE=tty`) creating it
// fails outright — a flag routed through the normal launch could therefore
// never answer headless. `--help` and `--status` never build an app at all;
// `--sync-once` builds one only once it has credentials to poll with (see
// [`cli_sync_once_preflight`]) and then builds it in CLI mode: no window
// (`context.config_mut()` clears `create`), no tray icon, no app menu, no
// deep-link registration, no global-shortcut grabs (issue #769), no
// single-instance lock and no quit-time cleanup.
//
// Unknown arguments keep the pre-#679 behaviour — ignored, GUI launches — the
// same treatment `--minimized` and a `presencejam://` URL already get.

/// `--status`: print the current `SyncStatus` as JSON on stdout, exit 0.
const STATUS_FLAG: &str = "--status";
/// `--sync-once`: run exactly one poll iteration, then exit 0/1.
const SYNC_ONCE_FLAG: &str = "--sync-once";
/// `--help`: print usage text and exit 0.
const HELP_FLAG: &str = "--help";
/// `--set-status <message>`: post a manual Teams status with the default
/// expiry and exit. Pairs with `--clear-status` (issue #870).
const SET_STATUS_FLAG: &str = "--set-status";
/// `--set-status-expiry <minutes>`: the expiry override for `--set-status`.
/// Defaults to 60; clamped to the documented `5..=720` window.
const SET_STATUS_EXPIRY_FLAG: &str = "--set-status-expiry";
/// `--clear-status`: clear the user's manual Teams status and exit.
const CLEAR_STATUS_FLAG: &str = "--clear-status";
/// `--profile <id>`: switch the active presence profile to `<id>` (or
/// to "base" — `None` — when the id is missing or unknown) and exit
/// 0. Issue #869: the same switch path the tray profile submenu and
/// the `toggle_profile` hotkey use, so the CLI is the third surface
/// the runtime state machine exposes without writing the on-disk base
/// values.
const PROFILE_FLAG: &str = "--profile";

/// `--serve[=PORT]`: token-guarded localhost control + event API
/// (issue #865). Optional port is split off the flag, so the parser
/// sees `--serve`, `--serve=8649`, etc. The default port lives in
/// [`crate::serve::DEFAULT_PORT`].
const SERVE_FLAG: &str = "--serve";
/// `--daemon`: supervised headless daemon (issue #896). Runs the same
/// poller the GUI runs, but installs SIGTERM/SIGINT handlers, omits
/// every GUI surface (window, tray, app menu, deep-link, single-
/// instance lock), and exits 0 on a clean stop signal. Packaging units
/// live under `packaging/{systemd,launchd,windows}/`.
const DAEMON_FLAG: &str = "--daemon";

/// What the argv asked for. A CLI flag is an *alternative* to launching the
/// GUI, never a modifier of it — which is why an unrecognised argument still
/// launches normally.
#[derive(Debug, Clone, PartialEq, Eq)]
enum CliCommand {
    Help,
    Status,
    SyncOnce,
    /// Issue #870: post a manual Teams status with the supplied message
    /// and (optionally) expiry. Both are required for the dispatcher to
    /// produce a useful command; an `--set-status` without a message is
    /// treated like `--help` (usage + exit 1) for ergonomic CLI behaviour.
    SetManualStatus {
        message: String,
        expiry_minutes: u32,
    },
    /// Issue #870: clear the user's manual Teams status, no arguments.
    ClearManualStatus,
    /// Issue #869: switch the active presence profile. `name` is
    /// `None` when the user passed `--profile base` (or `--profile`
    /// with no argument), the documented way to clear the active
    /// profile and fall back to the base configuration.
    SetActiveProfile {
        name: Option<String>,
    },
    /// Issue #865: launch the token-guarded localhost HTTP control
    /// + event API. `port` is `None` for `--serve` (default port from
    ///   [`crate::serve::DEFAULT_PORT`]) and `Some(p)` for `--serve=p`.
    Serve(Option<u16>),
    /// Issue #896: supervised headless daemon. Same Tauri runtime as
    /// `--sync-once` (windowless, no GUI surfaces) plus SIGTERM/SIGINT
    /// handling and a bounded poller join. Exits 0 on clean stop.
    Daemon,
}

/// Parse the CLI intent out of argv; `None` means "launch the GUI".
///
/// Matching is exact and left-to-right: the first recognised flag wins, and
/// every unrecognised argument is ignored. Generic over the argv element type
/// so the parser is unit-testable without touching the real process argv
/// (mirroring `has_minimized_flag`).
///
/// Issue #870: `--set-status <message>` + the optional
/// `--set-status-expiry <minutes>` are parsed together, so a CLI invocation
/// is a single atomic decision — the partial-flag case (`--set-status`
/// without a message) is treated as "argument missing", not as a
/// separate CliCommand variant.
fn cli_command<I, S>(args: I) -> Option<CliCommand>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut args_iter = args.into_iter();
    while let Some(arg) = args_iter.next() {
        let arg = arg.as_ref();
        if arg == std::ffi::OsStr::new(HELP_FLAG) {
            return Some(CliCommand::Help);
        }
        if arg == std::ffi::OsStr::new(STATUS_FLAG) {
            return Some(CliCommand::Status);
        }
        if arg == std::ffi::OsStr::new(SYNC_ONCE_FLAG) {
            return Some(CliCommand::SyncOnce);
        }
        if arg == std::ffi::OsStr::new(CLEAR_STATUS_FLAG) {
            return Some(CliCommand::ClearManualStatus);
        }
        if arg == std::ffi::OsStr::new(SET_STATUS_FLAG) {
            // Collect the rest of argv as owned strings; the manual status
            // parse is a flat two-pair shape (`--set-status <message>` plus
            // an optional `--set-status-expiry <minutes>`), so a single
            // vector is simpler than juggling iterator clones (issue #928
            // — `args_iter.clone()` does not exist for owned iterators).
            let rest: Vec<String> = args_iter
                .map(|m| m.as_ref().to_string_lossy().into_owned())
                .collect();
            let mut message = String::new();
            let mut expiry_minutes: u32 = 60;
            let mut i = 0;
            if let Some(first) = rest.first() {
                message = first.clone();
                i = 1;
            }
            while i + 1 < rest.len() {
                if rest[i] == SET_STATUS_EXPIRY_FLAG {
                    if let Ok(parsed) = rest[i + 1].parse::<u32>() {
                        expiry_minutes = parsed;
                    }
                    i += 2;
                } else {
                    i += 1;
                }
            }
            return Some(CliCommand::SetManualStatus {
                message,
                expiry_minutes,
            });
        }
        if arg == std::ffi::OsStr::new(PROFILE_FLAG) {
            // The next token — if any — is the profile id. `--profile` with
            // no argument (or `--profile base`) clears the active profile;
            // any other id attempts a switch and the dispatcher validates
            // against the on-disk list (unknown → "base", with a warning).
            let name = args_iter.next().and_then(|s| {
                let s = s.as_ref().to_string_lossy().into_owned();
                if s.is_empty() {
                    None
                } else {
                    Some(s)
                }
            });
            return Some(CliCommand::SetActiveProfile { name });
        }
        // Issue #865: `--serve[=PORT]`. The port is part of the same argv
        // token (`--serve=8649`), not a separate arg — a stray `--serve`
        // followed by a numeric token is the pre-#865 behaviour (unknown
        // flag, GUI launches) and we don't change it.
        if let Some(port) = parse_serve_arg(arg) {
            return Some(CliCommand::Serve(port));
        }
        // Issue #896: `--daemon` is an exact-match bare flag.
        if arg == std::ffi::OsStr::new(DAEMON_FLAG) {
            return Some(CliCommand::Daemon);
        }
    }
    None
}

/// Recognise `--serve` (default port) and `--serve=PORT` (issue #865).
/// Returns `Some(None)` for the bare flag, `Some(Some(port))` for the
/// explicit form, and `None` for any other argument.
///
/// Valid ports are 1..=65535. Port 0 is the OS-assigned "give me any free
/// port" sentinel, which the serve path does not bind directly — the
/// security model is "operator-controlled port" so we reject it and fall
/// through to the GUI launch (better than a silent rebind). Malformed
/// port strings (`--serve=abc`, `--serve=99999`) likewise leave the GUI
/// path alone: a typo is louder than a silent error.
fn parse_serve_arg(arg: &std::ffi::OsStr) -> Option<Option<u16>> {
    let bytes = arg.as_encoded_bytes();
    if bytes == SERVE_FLAG.as_bytes() {
        return Some(None);
    }
    let prefix = SERVE_FLAG.as_bytes();
    if bytes.len() <= prefix.len() + 1 || !bytes.starts_with(prefix) || bytes[prefix.len()] != b'='
    {
        return None;
    }
    let port_str = std::str::from_utf8(&bytes[prefix.len() + 1..]).ok()?;
    let port: u16 = port_str.parse().ok()?;
    if port == 0 {
        return None;
    }
    Some(Some(port))
}

/// Usage text for `--help`.
fn cli_help_text() -> String {
    format!(
        "\
PresenceJam {version}

USAGE:
  presencejam [FLAG]

FLAGS:
  --status      Print the current sync status as JSON on stdout and exit 0.
                Runs without a window, a tray icon or the single-instance lock,
                so it also works on a headless machine. The fields are the ones
                the app's `get_sync_status` command returns; a freshly started
                process has no poller, so `is_syncing`, `current_track` and the
                presence fields are empty unless this process polls.
  --sync-once   Run exactly one poll iteration (including the Teams status
                write) and exit 0 on success, or exit 1 with the reason on
                stderr. Logs go to the normal log file. Requires the app to be
                signed in to Spotify and Teams; without credentials it exits 1
                before anything else happens, and on Linux it needs a display
                server (it drives the app's own poller) — use xvfb-run on a
                bare machine.
  --set-status <message>            Post a manual Teams status (issue #870) and
                exit 0 on success, or exit 1 on stderr. The message is
                profanity-filtered (using teams.profanity_extra_words) and
                bounded to 128 characters, exactly like a rule's replacement
                text. A pair of `--set-status` + `--set-status-expiry` is
                the documented way to script a \"Right back in 30\" button
                from CI; the expiry defaults to 60 minutes and is clamped to
                the documented 5..=720 minute window.
  --set-status-expiry <minutes>     The expiry override for `--set-status`.
                Clamped to 5..=720; the Dashboard composer reads the same
                bounds.
  --clear-status                    Clear any manual Teams status (issue #870)
                and exit 0 on success, or exit 1 on stderr.
  --serve[=PORT]                    Start the token-guarded localhost HTTP
                control + event API (issue #865) on 127.0.0.1:PORT (default
                8649) and run until interrupted. `GET /status` returns the
                same JSON shape as `--status`; `GET /events` streams the
                three presence-related Tauri events as SSE. `POST /pause`,
                `/resume`, `/snooze?minutes=N` and `/profile?id=<id>` are
                mutating and require `Authorization: Bearer <token>`. The
                token is 32 random bytes stored in the OS keychain (not
                on disk in plaintext); an operator retrieves it via
                `secret-tool`/`security`/`Credential Manager` on first
                boot. No route writes configuration or token material.
  --daemon     Run as a supervised headless daemon (issue #896): no
                window, no tray, no app menu, no deep-link registration,
                no single-instance lock. The poller starts automatically
                and exits 0 on SIGTERM (Unix; systemd / launchd) or on a
                `taskkill` (Windows; Task Scheduler). Packaging units for
                systemd, launchd and Task Scheduler live under
                `packaging/`.
  --help        Print this help and exit 0.
  --minimized   Start with the window hidden. The autostart plugin passes
                this, and it still launches the GUI.

Any other argument is ignored and the app starts normally, as it always has.
",
        version = env!("CARGO_PKG_VERSION")
    )
}

/// Read the stored tokens without a `tauri::AppHandle` (issue #679), through
/// the same reader the app's setup uses.
fn cli_read_tokens() -> Result<token_io::TokensFile, String> {
    let path = token_io::tokens_file_path_headless()?;
    token_io::read_tokens_at_path(&path)
}

/// Build the `AppState` the GUI's setup builds, without a Tauri app.
///
/// Config and tokens come from the same files (`config::load_config`, the
/// headless tokens read), so a CLI process reports the state the app would
/// report. A missing or unreadable file is not fatal here — setup degrades to
/// "no config / no tokens" the same way, and `--status` must still answer the
/// question it was asked. Returns the load failures instead of logging them:
/// the log plugin only exists on the app path, so a headless `log::warn!`
/// would go nowhere and the caller decides what to say on stderr.
fn cli_headless_state() -> (Arc<AppState>, Vec<String>) {
    let state = Arc::new(AppState::new());
    let mut failures = Vec::new();
    match config::load_config() {
        Ok(cfg) => *state.config.get_mut() = Some(cfg),
        Err(e) => failures.push(format!(
            "no config loaded ({e}); reporting the built-in defaults"
        )),
    }
    match cli_read_tokens() {
        Ok(tokens) => {
            *state.tokens.spotify_mut() = tokens.spotify_tokens;
            *state.tokens.teams_mut() = tokens.teams_tokens;
        }
        Err(e) => failures.push(format!(
            "no tokens loaded ({e}); reporting both providers as disconnected"
        )),
    }
    (state, failures)
}

/// Issue #870: `--set-status <message>` body. Same filter + clamp + Graph
/// POST pipeline the Dashboard composer runs, with the same error strings,
/// just without an `AppHandle` (the emit is a no-op on this path — there is
/// no Dashboard listening for the event). Builds the `AppState` from disk
/// the same way `cli_headless_state` does, so the CLI flag and the GUI
/// share the profanity lexicon and the placeholder text.
fn cli_set_manual_status_from_disk(message: &str, expiry_minutes: u32) -> Result<(), String> {
    use crate::commands::status as status_cmd;
    let (state, failures) = cli_headless_state();
    for failure in &failures {
        eprintln!("presencejam: {SET_STATUS_FLAG}: {failure}");
    }
    // Issue #870: the preflight mirrors `cli_sync_once_preflight`. The Teams
    // token must be live; the Spotify token is irrelevant for the manual
    // status POST (no Spotify data is read).
    let tokens = state.tokens.teams().clone();
    let Some(tokens) = tokens else {
        return Err("Teams is not connected; cannot set a manual status".to_string());
    };
    if tokens.expires_at <= chrono::Utc::now() {
        return Err(
            "Teams token is expired; sign in again from the app before using this flag".to_string(),
        );
    }

    // Step 1: trim + clamp + profanity filter (issue #538). Mirror the
    // Dashboard composer exactly: empty text is a clear, oversized text
    // is truncated to `MAX_RULE_STATUS_CHARS`.
    let mut text = message.trim().to_string();
    crate::config::clamp_rule_text(&mut text);
    let expiry_minutes = status_cmd::clamp_expiry_public(expiry_minutes);
    if text.is_empty() {
        // Same UX as the Dashboard: blank submit clears.
        return cli_clear_manual_status_from_disk();
    }
    let placeholder = state
        .config
        .get()
        .as_ref()
        .map(|c| c.teams.profanity_placeholder.clone())
        .unwrap_or_default();
    let cfg_guard = state.config.get();
    let extra_words = crate::config::profanity_extra_words_for_filter(cfg_guard.as_ref());
    let posted_text = crate::profanity::filter_status(&text, &placeholder, true, extra_words);

    let now = chrono::Utc::now();
    let expires_at = now + chrono::Duration::minutes(expiry_minutes as i64);
    let expiry_str = crate::teams::manual_status_expiry_rfc3339(expires_at);
    crate::teams::set_teams_status_message(&tokens.access_token, &posted_text, Some(&expiry_str))
        .map_err(|e| format!("failed to post manual status to Teams: {}", e))?;
    let manual = status_cmd::ManualStatus {
        message: posted_text.clone(),
        expires_at,
        set_at: now,
    };
    status_cmd::record_manual_status_cli(
        manual,
        status_cmd::RecentManualStatus {
            message: text.clone(),
            used_at: now,
        },
    );
    log::info!(
        "[CLI] set_manual_status: posted {} chars (filtered={}), expires in {} min",
        posted_text.chars().count(),
        posted_text != text,
        expiry_minutes
    );
    Ok(())
}

/// Issue #870: `--clear-status` body. Same Teams clear path the Dashboard
/// composer's Clear button runs. No Spotify data is read.
fn cli_clear_manual_status_from_disk() -> Result<(), String> {
    let (state, failures) = cli_headless_state();
    for failure in &failures {
        eprintln!("presencejam: {CLEAR_STATUS_FLAG}: {failure}");
    }
    let tokens = state.tokens.teams().clone();
    let Some(tokens) = tokens else {
        // No Teams session — clear the local record and report success
        // (the next sign-in starts from a clean slate).
        crate::commands::status::clear_manual_status_record_cli();
        return Ok(());
    };
    let placeholder = crate::commands::sync::safe_placeholder_text(&state);
    crate::teams::clear_teams_status_message(
        &tokens.access_token,
        &placeholder,
        Some(&crate::teams::placeholder_expiry_rfc3339()),
    )
    .map_err(|e| format!("failed to clear manual status on Teams: {}", e))?;
    crate::commands::status::clear_manual_status_record_cli();
    log::info!("[CLI] clear_manual_status: manual status cleared");
    Ok(())
}

/// Issue #869: `--profile <id>` body. Switches the active presence
/// profile through the same `clamped_config` write path every other
/// config change uses; returns the (clamped) new value so the
/// dispatcher prints a single-line confirmation. `--profile base`
/// (or no argument) clears the active profile back to the base
/// configuration; an unknown id is treated as "base" with a warning
/// — `clamped_config` already does the same thing, so the dispatcher's
/// only job here is to validate the input shape.
fn cli_set_active_profile_from_disk(name: Option<String>) -> Result<Option<String>, String> {
    let (state, failures) = cli_headless_state();
    for failure in &failures {
        eprintln!("presencejam: {PROFILE_FLAG}: {failure}");
    }
    let mut cfg = state
        .config
        .get()
        .clone()
        .ok_or_else(|| "no config loaded".to_string())?;
    let target: Option<String> = match name {
        None => None,
        Some(raw) if raw.eq_ignore_ascii_case("base") || raw.eq_ignore_ascii_case("none") => None,
        Some(raw) => {
            // Match against the clamped list (the same names the tray /
            // hotkey use). The clamp trims + truncates + dedupes so a
            // raw CLI id can only match a clamped id, never a phantom.
            let exists = cfg.presence_profiles.iter().any(|p| p.name == raw);
            if !exists {
                log::warn!(
                    "[CLI] set_active_profile: profile {:?} not found — falling back to base",
                    raw
                );
                None
            } else {
                Some(raw)
            }
        }
    };
    cfg.active_profile = target.clone();
    let clamped = crate::config::clamped_config(&cfg);
    // Persist + republish the active-profile change so the running
    // app picks it up on the next poll. The CLI flag is
    // deliberately NOT a process restart — the doc says it just
    // rewrites `active_profile`.
    let path = crate::config::get_config_path()
        .map_err(|e| format!("failed to resolve config path: {}", e))?;
    let serialized = serde_json::to_string_pretty(&clamped)
        .map_err(|e| format!("failed to serialize: {}", e))?;
    std::fs::write(&path, serialized)
        .map_err(|e| format!("failed to persist to {}: {}", path.display(), e))?;
    log::info!("[CLI] set_active_profile: {:?}", clamped.active_profile);
    Ok(clamped.active_profile)
}

/// `--profile <id>`: localised confirmation strings for the CLI
/// dispatcher. The keys live in `en` / `de` / `fr`; the CLI never
/// reads the i18n table directly because it is built before the i18n
/// module is reachable. Inline copies are intentional — the CLI is
/// the only surface that prints these messages and keeping them out
/// of the i18n table means a CLI run never has to load the
/// dictionaries.
fn t_cli_profile_active(name: &str) -> String {
    format!("Active profile is now \"{name}\".")
}

fn t_cli_profile_active_base() -> String {
    "Active profile cleared — using base configuration.".to_string()
}

/// `--status`: print the status JSON and return the process exit code.
fn cli_status_exit_code() -> i32 {
    let (state, failures) = cli_headless_state();
    for failure in &failures {
        // stderr, not the log file: this path never registers the log plugin
        // (no app is built), and stdout must stay parseable JSON.
        eprintln!("presencejam: {STATUS_FLAG}: {failure}");
    }
    let status = crate::commands::sync::sync_status_from_state(&state);
    match serde_json::to_string_pretty(&status) {
        Ok(json) => {
            // `println!` is this flag's output channel, not logging: see the
            // note above — a caller pipes stdout into `jq`.
            println!("{json}");
            0
        }
        Err(e) => {
            eprintln!("presencejam: {STATUS_FLAG}: failed to serialise the status: {e}");
            1
        }
    }
}

/// The credential test a `--sync-once` run must pass before anything else
/// happens (issue #679). Pure, so the "with credentials it would exit 0" half
/// of the contract is pinned without a signed-in machine: the credentialed
/// run itself needs a display (the poller works through a `tauri::AppHandle`,
/// which only exists once the GUI runtime does).
///
/// Both providers are required: the iteration's whole point is the Teams
/// status write, and with no Spotify tokens the poller logs "No Spotify tokens
/// available, waiting..." and does nothing — a silent no-op is the worst
/// possible exit-0.
fn cli_sync_once_preflight(
    config: &crate::config::AppConfig,
    tokens: &token_io::TokensFile,
) -> Result<(), String> {
    if config.spotify.client_id.trim().is_empty() {
        return Err(
            "no Spotify client_id configured — sign in from the app before using this flag"
                .to_string(),
        );
    }
    if tokens.spotify_tokens.is_none() {
        return Err(
            "not signed in to Spotify (no Spotify tokens stored) — sign in from the app first"
                .to_string(),
        );
    }
    if tokens.teams_tokens.is_none() {
        return Err(
            "not signed in to Microsoft Teams (no Teams tokens stored) — the status write needs it"
                .to_string(),
        );
    }
    Ok(())
}

/// Load the files [`cli_sync_once_preflight`] decides on, turning a load
/// failure into the reason the CLI prints.
fn cli_sync_once_preflight_from_disk() -> Result<(), String> {
    let config =
        config::load_config().map_err(|e| format!("cannot read the stored config: {e}"))?;
    let tokens = cli_read_tokens().map_err(|e| format!("cannot read the stored tokens: {e}"))?;
    cli_sync_once_preflight(&config, &tokens)
}

/// Subscribe to the poller's failure signals for the duration of one
/// `--sync-once` iteration: first failure wins.
fn cli_listen_for_failures(
    handle: &AppHandle,
    sink: Arc<Mutex<Option<String>>>,
) -> Vec<tauri::EventId> {
    use tauri::Listener;

    // `error` is `polling::emit_error`'s centralised shape (every Spotify
    // fetch/refresh failure and every failed Teams write goes through it);
    // `reconnect-required` is the poller's "the user must sign in again"
    // signal, and `spotify-reconnect-required` / `teams-reconnect-required`
    // are its provider-specific siblings (the Teams one is emitted on its own).
    const FAILURE_EVENTS: [&str; 4] = [
        "error",
        "reconnect-required",
        "spotify-reconnect-required",
        "teams-reconnect-required",
    ];
    let mut ids = Vec::with_capacity(FAILURE_EVENTS.len());
    for event in FAILURE_EVENTS {
        let sink = Arc::clone(&sink);
        ids.push(handle.listen(event, move |message| {
            let mut slot = sink.lock();
            if slot.is_none() {
                *slot = Some(cli_failure_reason(event, message.payload()));
            }
        }));
    }
    ids
}

/// One line for stderr out of a poller failure event.
fn cli_failure_reason(event: &str, payload: &str) -> String {
    if event != "error" {
        return format!("{event}: a provider needs to be reconnected");
    }
    match serde_json::from_str::<serde_json::Value>(payload) {
        Ok(value) => {
            let source = value
                .get("source")
                .and_then(|s| s.as_str())
                .unwrap_or("unknown");
            let message = value
                .get("message")
                .and_then(|s| s.as_str())
                .unwrap_or(payload);
            format!("{source}: {message}")
        }
        Err(_) => payload.to_string(),
    }
}

/// `--sync-once` inside the CLI-mode app: run one iteration through the
/// poller's own one-shot entry point — the same `polling::run_oneshot` the
/// tray's "Refresh" uses, so the status write, the dedup clocks and the
/// presence gate are the ones a loop iteration gets — then exit with its
/// verdict.
///
/// The verdict comes from the poller's failure events rather than from
/// `run_oneshot`'s return value: that fn deliberately discards the iteration
/// verdict (in `RunMode::OneShot` every parking sleep is an immediate Break),
/// but every failure it can hit announces itself — `polling::emit_error` for a
/// Spotify/Teams error, the `*-reconnect-required` pair for dead credentials.
/// Anything else (no track playing, a deduped write, a suppressed gate) is a
/// completed iteration.
fn cli_sync_once_iteration(
    app: &tauri::App,
    state: Arc<AppState>,
) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::Listener;

    let handle = app.handle().clone();
    let failure: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let listeners = cli_listen_for_failures(&handle, Arc::clone(&failure));

    log::info!("[CLI] {SYNC_ONCE_FLAG}: running one poll iteration");
    polling::run_oneshot(&state, &handle);

    for id in listeners {
        handle.unlisten(id);
    }
    match failure.lock().clone() {
        None => {
            log::info!("[CLI] {SYNC_ONCE_FLAG}: iteration completed");
            // Graceful teardown: the runtime flushes the log plugin in
            // `cleanup_before_exit`, and 0 is the code it exits with anyway.
            handle.exit(cli_sync_once_exit_code(None));
        }
        Some(reason) => {
            let code = cli_sync_once_exit_code(Some(&reason));
            eprintln!("presencejam: {SYNC_ONCE_FLAG}: {reason}");
            log::warn!("[CLI] {SYNC_ONCE_FLAG}: iteration failed: {reason}");
            // Issue #679 review round 2: `AppHandle::exit(code)` cannot report a
            // non-zero code — tauri-runtime-wry turns `RequestExit(code)` into
            // `ControlFlow::Exit`, which tao maps to `process::exit(0)` (the
            // string `ExitWithCode` appears nowhere in the runtime) — so a
            // scripted caller would read a failed iteration as success. Exit the
            // process here instead, after flushing the logger (the plugin's file
            // target buffers, so an unflushed exit would lose this very line).
            log::logger().flush();
            std::process::exit(code);
        }
    }
    Ok(())
}

/// The process exit code for one `--sync-once` iteration: a completed
/// iteration is success, a captured failure signal is not.
///
/// Split out so the mapping is pinned by a test (issue #679 review round 2:
/// the failure branch used to hand its code to `AppHandle::exit`, which drops
/// it — the flag printed a reason and then exited 0).
fn cli_sync_once_exit_code(failure: Option<&str>) -> i32 {
    if failure.is_some() {
        1
    } else {
        0
    }
}

/// Clear `create` on every window `tauri.conf.json` declares (issue #679):
/// `App::run` builds one webview per `create = true` entry, so a CLI mode that
/// left this alone would flash a window on screen (and would already have
/// failed on a headless machine). Returns how many windows were suppressed.
fn suppress_config_windows<R: tauri::Runtime>(context: &mut tauri::Context<R>) -> usize {
    let mut suppressed = 0;
    for window in context.config_mut().app.windows.iter_mut() {
        window.create = false;
        suppressed += 1;
    }
    suppressed
}

/// The single-instance callback: raise the already-running window.
///
/// Named rather than an inline closure (issue #679) so the registration site —
/// which CLI mode has to skip — stays one line.
///
/// Deep-link routing (issue #799): on Windows/Linux the single-instance plugin
/// is built with the `deep-link` cargo feature, so a second launch carrying a
/// `presencejam://` URL is already routed through `handle_cli_arguments`,
/// which emits `deep-link://new-url` — the same event the `on_open_url`
/// callback wired in setup listens to. That path invokes `handle_deep_link`
/// exactly once, so this callback must NOT scan argv for URLs and dispatch
/// them again: the duplicate would redeem the same authorization `code`
/// twice, and the second exchange fails with a spurious
/// `spotify-auth-failed`. This callback therefore only raises the window and
/// repaints the tray; it never touches argv URLs. macOS goes through the
/// deep-link plugin's `on_open_url` callback, which is wired in setup.
#[cfg(desktop)]
fn forward_launch_to_running_instance(app: &AppHandle, _argv: Vec<String>, _cwd: String) {
    // Raise the existing window so the user sees it when a second
    // instance is launched (e.g., double-click the .msi shortcut
    // while the app is running, or a deep-link click from a
    // browser when the app is already open).
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        // Issue #886: the tray's dedup key reads a visibility mirror, so every
        // path that shows the window has to report it — otherwise the raise
        // would be deduped away and the Show/Hide label would keep "Show Window".
        crate::tray::note_window_visibility(true);
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
    // Issue #592: raising the window changes its visibility, which
    // drives the tray's Show/Hide label — repaint from backend state
    // on a worker (the rebuild may perform blocking Spotify HTTP).
    crate::tray::refresh_tray_from_state(app);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    log::info!("[APP] run: ENTRY");

    // Issue #679: the CLI flags are resolved before anything else, and
    // `--help` / `--status` / a credential-less `--sync-once` never reach the
    // builder at all. `sync_once` carries the only case that continues into
    // the GUI builder — and then in CLI mode.
    //
    // Issue #870: `--set-status <message> [--set-status-expiry <minutes>]`
    // and `--clear-status` join the same exit-fast path. They need the
    // token I/O to read the Teams access token (the message is POSTed
    // directly to Graph), but otherwise behave like `--status` — no
    // window, no tray, no single-instance lock.
    // Issue #679 / #865: the CLI flags are resolved before anything else.
    // `sync_once` is `true` for `--sync-once` (still builds the app, but
    // in CLI mode and exits at the end of one iteration); `serve_port`
    // is `Some(p)` for `--serve[=p]` (issue #865, builds the app in CLI
    // mode and keeps the runtime alive serving the HTTP surface). The
    // GUI builder proceeds with both flags off (the common case).
    let (sync_once, serve_port) = match cli_command(std::env::args_os()) {
        Some(CliCommand::Help) => {
            println!("{}", cli_help_text());
            std::process::exit(0);
        }
        Some(CliCommand::Status) => std::process::exit(cli_status_exit_code()),
        Some(CliCommand::SyncOnce) => match cli_sync_once_preflight_from_disk() {
            Ok(()) => (true, None),
            Err(reason) => {
                eprintln!("presencejam: {SYNC_ONCE_FLAG}: {reason}");
                std::process::exit(1);
            }
        },
        Some(CliCommand::SetManualStatus {
            message,
            expiry_minutes,
        }) => {
            if message.trim().is_empty() {
                eprintln!(
                    "presencejam: {SET_STATUS_FLAG}: missing message (pass it as the next argument)"
                );
                std::process::exit(1);
            }
            match cli_set_manual_status_from_disk(&message, expiry_minutes) {
                Ok(()) => std::process::exit(0),
                Err(reason) => {
                    eprintln!("presencejam: {SET_STATUS_FLAG}: {reason}");
                    std::process::exit(1);
                }
            }
        }
        Some(CliCommand::ClearManualStatus) => match cli_clear_manual_status_from_disk() {
            Ok(()) => std::process::exit(0),
            Err(reason) => {
                eprintln!("presencejam: {CLEAR_STATUS_FLAG}: {reason}");
                std::process::exit(1);
            }
        },
        Some(CliCommand::SetActiveProfile { name }) => match cli_set_active_profile_from_disk(name)
        {
            Ok(active) => {
                match active {
                    Some(name) => println!("{}", t_cli_profile_active(&name)),
                    None => println!("{}", t_cli_profile_active_base()),
                }
                std::process::exit(0);
            }
            Err(reason) => {
                eprintln!("presencejam: {PROFILE_FLAG}: {reason}");
                std::process::exit(1);
            }
        },
        // Issue #865: `--serve[=PORT]` needs the same builder as `--sync-once`
        // (an `AppHandle` to subscribe to Tauri events), but it stays alive
        // — `sync_once=false`, `serve_port = Some(p)`. The builder stays in
        // CLI mode (no window/tray/menu/deep-link/single-instance) and the
        // runtime loop runs until interrupted.
        Some(CliCommand::Serve(port)) => match cli_sync_once_preflight_from_disk() {
            Ok(()) => (false, Some(port)),
            Err(reason) => {
                eprintln!("presencejam: {SERVE_FLAG}: {reason}");
                std::process::exit(1);
            }
        },
        // Issue #896: `--daemon` shares the `--sync-once` CLI-mode
        // omissions but stays alive for the SIGTERM/SIGINT supervisor.
        // We overload `serve_port = Some(None)` so the existing
        // `cli_mode = sync_once || serve_port.is_some()` gate is the
        // single source of truth for "GUI surfaces must stay off"; the
        // setup hook then disambiguates `Some(None)` (daemon) from
        // `Some(Some(p))` (serve).
        Some(CliCommand::Daemon) => match cli_sync_once_preflight_from_disk() {
            Ok(()) => (false, Some(None)),
            Err(reason) => {
                eprintln!("presencejam: {DAEMON_FLAG}: {reason}");
                std::process::exit(1);
            }
        },
        None => (false, None),
    };
    // Issue #865: the two CLI modes share the same "GUI surfaces must stay
    // off" rule. `sync_once` and `serve_port` are individually readable so
    // their distinctive setup hooks stay type-safe, but every site that
    // currently branches on `sync_once` reads `cli_mode` instead, so
    // adding a third CLI mode in the future is a one-line change.
    let cli_mode = sync_once || serve_port.is_some();

    let mut builder = tauri::Builder::default();

    // Issue #679: a `--sync-once` run must not put a window on the user's
    // screen for the duration of one poll. The config-declared windows are
    // created by `App::run`, not by `build`, so clearing `create` here keeps
    // this launch windowless without touching the GUI's own config.
    // Issue #865: `--serve` shares the same windowless rule — the HTTP
    // surface is the whole point of the launch.
    let mut context = tauri::generate_context!();
    if cli_mode {
        suppress_config_windows(&mut context);
    }

    #[cfg(desktop)]
    {
        use tauri_plugin_single_instance::init as single_instance_init;

        // Issue #679: a `--sync-once` run must not take the single-instance
        // lock. With it registered, a CLI run while the app is already open
        // would be forwarded to the running instance as a "second launch" and
        // exit 0 without polling anything. Issue #865: `--serve` must not
        // take the lock either — a systemd-managed daemon launches via
        // `ExecStart=` every restart, and stealing the lock from a stale
        // GUI process would mis-attribute the SIGTERM.
        if !cli_mode {
            builder = builder.plugin(single_instance_init(forward_launch_to_running_instance));
        }

        builder = builder.plugin(tauri_plugin_deep_link::init());
        log::info!("[APP] run: deep_link plugin registered");

        builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
        log::info!("[APP] run: updater plugin registered");
    }

    // 4.7.0 (S5): the rotating file target is configured from `logging.*`
    // before the plugin is built, which is the only point the plugin offers —
    // Tauri's `setup` hook (and therefore `config::load_config`) has not run
    // yet. `logging_config_for_startup` reads just that section, with no
    // keychain probe; a change to size/retention takes effect at the next
    // launch, while `logging.enabled`/`log_level` keep their immediate
    // `apply_log_level` path.
    let startup_logging = config::logging_config_for_startup();
    let log_rotation = log_rotation_strategy(startup_logging.keep_files);

    let built = builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        // Global shortcuts (issue #676): the feature whose whole point is
        // working while the window is hidden. The grabs themselves are
        // registered in setup from the persisted config — never here, and
        // never fatally: a desktop that refuses a grab reports it per slot in
        // Settings instead of failing startup.
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .plugin(tauri_plugin_log::Builder::new()
            .target(tauri_plugin_log::Target::new(
                tauri_plugin_log::TargetKind::Stdout,
            ))
            .target(tauri_plugin_log::Target::new(
                tauri_plugin_log::TargetKind::LogDir { file_name: Some("PresenceJam".into()) },
            ))
            .target(tauri_plugin_log::Target::new(
                tauri_plugin_log::TargetKind::Webview,
            ))
            .max_file_size(startup_logging.max_file_size_mb as u128 * 1024 * 1024)
            .rotation_strategy(log_rotation)
            .build())
        .setup(move |app| {
            // Set panic hook to log crashes
            std::panic::set_hook(Box::new(|panic_info| {
                let msg = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
                    s.clone()
                } else {
                    "Unknown panic".to_string()
                };

                let location = if let Some(loc) = panic_info.location() {
                    format!("{}:{}:{}", loc.file(), loc.line(), loc.column())
                } else {
                    "unknown location".to_string()
                };

                log::error!("[PANIC] {} at {}", msg, location);
                // Belt-and-braces fallback removed: `eprintln!` writes to stderr, which on
                // macOS release builds is not connected to the parent's log file
                // (`~/Library/Logs/com.presencejam.app/` — `app_log_dir()`, which carries
                // the bundle-id segment since #300, which the pre-#300 path did not have).
                // The `log::error!` above routes through
                // `tauri-plugin-log`, which is the canonical destination for user-visible
                // log lines and the file the `open_logs_folder` command points at. The
                // previous dual-write left a silent failure mode where the panic appeared
                // in a terminal nobody was reading but never in the log file the user could
                // open. See issue #79.
            }));


            log::info!("[APP] setup: ENTRY");

            let state = Arc::new(AppState::new());
            app.manage(state.clone());

            // C3(c) "install on quit": register the deferred-update staging
            // state (updater plugin is desktop-only, so this follows suit).
            #[cfg(desktop)]
            updater_bg::manage(app.handle());
            log::info!("[APP] setup: AppState created and managed");

            // Issue #69: prime the keychain cache once at app start so the
            // polling thread's first iteration doesn't hit the keychain
            // (and on macOS, doesn't show a keychain prompt mid-poll).
            // We do this early so the cache is warm before any
            // `start_syncing` call.
            match crate::keychain::get_spotify_client_secret() {
                Ok(_) => log::info!("[APP] setup: keychain cache primed (Spotify client_secret present)"),
                Err(e) => {
                    // Log the underlying reason at debug level for troubleshooting
                    // (locked keychain, permission denied, keyring daemon down, etc.)
                    // without cluttering info-level output for the common new-user path.
                    log::debug!("[APP] setup: keychain access failed: {}", e);
                    log::info!("[APP] setup: keychain cache empty (no Spotify client_secret yet — user must Onboard)");
                }
            }

            // Load config into AppState
            match config::load_config() {
                Ok(cfg) => {
                    // #226: wire logging.enabled / log_level into the logger after
                    // config load. The mapping lives in `config::apply_log_level`
                    // (CfgDiag#4, issue #539) so a later save can re-arm the logger
                    // from the same code instead of waiting for a relaunch.
                    config::apply_log_level(&cfg.logging);
                    let mut config_guard = state.config.get_mut();
                    *config_guard = Some(cfg.clone());
                    log::info!("[APP] setup: config loaded into AppState");

                    // Handle start_minimized setting. On macOS, also switch
                    // the app's activation policy to `Accessory` so the
                    // dock icon and menu-bar app menu disappear when the
                    // user wants tray-only behavior. Setting the policy on
                    // every startup is idempotent and ensures the dock
                    // icon matches the saved preference even after a
                    // crash-restart. See audit Q4.
                    // Issue #589: the autostart plugin launches us with
                    // `--minimized`, which used to be passed and parsed
                    // nowhere — an autostart user got a window and a
                    // taskbar entry thrown up at every login. The flag now
                    // joins the config setting so the launch intent is real;
                    // it stays exact-match only (never a prefix of some
                    // other argument).
                    let launched_minimized = has_minimized_flag(std::env::args_os());
                    // Issue #679: CLI mode has no window to hide and must not
                    // switch the macOS activation policy either — the flag it
                    // was asked for has nothing to do with the launch intent.
                    if !cli_mode && (cfg.teams.start_minimized || launched_minimized) {
                        log::info!(
                            "[APP] setup: starting hidden (config start_minimized={}, {}={})",
                            cfg.teams.start_minimized,
                            MINIMIZED_FLAG,
                            launched_minimized
                        );
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.hide();
                            // Issue #886: report the hide to the tray's mirror.
                            crate::tray::note_window_visibility(false);
                        }
                        #[cfg(target_os = "macos")]
                        {
                            // tauri::AppHandle::set_activation_policy returns () on
                            // success; the underlying call logs its own errors via the
                            // tauri-runtime-wry layer. We deliberately discard the unit
                            // value here rather than wrapping in `if let Err(...)`.
                            let _ = app.set_activation_policy(
                                tauri::ActivationPolicy::Accessory,
                            );
                        }
                    }
                }

                Err(e) => {
                    log::warn!("[APP] setup: no config found: {}", e);
                }
            }

            // One-shot startup migration: strip plaintext Spotify client_secret
            // from config.json (legacy ≤ v2.5.0) into the OS keychain. Safe to
            // call on every launch; no-op once the field is gone. The `_with_app`
            // variant surfaces a keychain conflict to Settings via a one-time
            // `spotify-secret-conflict` event (issue #376). See audit Q3 and
            // issue #9.
            config::migrate_legacy_client_secret_with_app(app.handle());

            // Load persisted tokens (Spotify + Teams) into AppState. We bypass
            // any plugin store for the tokens file and read it directly
            // from `<app-config-dir>/PresenceJam/tokens.json` — since v3.0
            // (issue #140) the file is AES-256-GCM ciphertext, decrypted here;
            // a legacy plaintext file (≤ v2.10.0) is migrated on read. See
            // issues #65 and #140.
            //
            // The pending_*_auth blobs (PKCE verifier, device code) are no
            // longer persisted to disk; the user re-starts the auth flow
            // after a crash mid-OAuth (cheap UX, and the disk leak is gone).
            match token_io::read_tokens_at(app.handle()) {
                Ok(tf) => {
                    if let Some(st) = tf.spotify_tokens {
                        *state.tokens.spotify_mut() = Some(st);
                        log::info!("[APP] setup: spotify_tokens loaded into AppState");
                    } else {
                        log::info!("[APP] setup: no spotify_tokens in tokens.json");
                    }
                    if let Some(tt) = tf.teams_tokens {
                        *state.tokens.teams_mut() = Some(tt);
                        log::info!("[APP] setup: teams_tokens loaded into AppState");
                    } else {
                        log::info!("[APP] setup: no teams_tokens in tokens.json");
                    }
                }
                Err(e) => {
                    log::warn!("[APP] setup: failed to load tokens.json: {}", e);
                }
            }

            // Issue #679: `--sync-once` has everything it needs — the app is
            // built windowless in CLI mode, and the state above is loaded — so
            // run its one poll iteration now and exit. Everything below belongs
            // to the GUI: deep-link registration, the tray icon, the app menu.
            // A CLI one-shot must touch none of those surfaces (and would fail
            // on the menu step, which needs the window CLI mode never creates).
            if sync_once {
                return cli_sync_once_iteration(app, state.clone());
            }
            // Issue #865: `--serve[=PORT]` builds the same windowless app and
            // then stays in the event loop. `serve::start_serve` owns the
            // server thread for the lifetime of the process; the runtime
            // loop below keeps the process alive until SIGINT/SIGTERM (or
            // an explicit `app.exit()` from elsewhere — the HTTP layer has
            // no such endpoint by design).
            //
            // Issue #896: `--daemon` shares the same "stays alive in CLI
            // mode" rule but routes to `polling::daemon::run` instead. The
            // dispatcher overloads `serve_port` as a tri-state: `None` is
            // "GUI launch", `Some(None)` is "daemon", `Some(Some(p))` is
            // "serve on port p".
            if let Some(port_opt) = serve_port {
                match port_opt {
                    Some(port) => {
                        let app_handle = app.handle().clone();
                        let state_for_serve = Arc::clone(&state);
                        if let Err(e) =
                            serve::start_serve(state_for_serve, app_handle, port)
                        {
                            log::error!("[APP] setup: --serve failed to start: {}", e);
                            return Err(Box::new(std::io::Error::other(e)));
                        }
                        log::info!(
                            "[APP] setup: --serve bound; runtime loop will keep the process alive"
                        );
                    }
                    None => {
                        // Issue #896: install the supervisor and let the
                        // runtime loop block on the daemon. The supervisor
                        // owns its own signal handlers and stops the
                        // poller on SIGTERM/SIGINT; we exit 0 from
                        // `RunEvent::Exit` when the supervisor returns.
                        let shutdown = Arc::new(std::sync::atomic::AtomicBool::new(false));
                        let app_handle = app.handle().clone();
                        let state_for_daemon = Arc::clone(&state);
                        let shutdown_for_handler = Arc::clone(&shutdown);
                        // `daemon::run` installs its own SIGTERM/SIGINT
                        // handlers; the same `shutdown` flag is also
                        // checked by `RunEvent::Exit` below so a daemon
                        // exit path that bypasses the supervisor (e.g. a
                        // self-exiting poller) still exits cleanly.
                        if let Err(e) = polling::run_daemon(
                            state_for_daemon,
                            app_handle,
                            shutdown_for_handler,
                        ) {
                            log::error!("[APP] setup: --daemon supervisor failed: {}", e);
                            return Err(Box::new(std::io::Error::other(e)));
                        }
                        // The supervisor returned without a panic — exit
                        // 0 immediately; the runtime loop has nothing to
                        // add. We deliberately do not enter `app.run`,
                        // which would block on the Tauri event loop with
                        // no windows / no tray / no menu to drive it.
                        log::info!("[APP] setup: --daemon supervisor returned; exiting 0");
                        std::process::exit(0);
                    }
                }
            }
            // Global shortcuts (issue #676): register the bindings from the
            // config loaded above. This sits BELOW the `--sync-once` early
            // return since issue #769: a CLI one-shot runs windowless and must
            // touch no GUI surface — which includes taking OS-level
            // accelerator grabs — so the registration belongs to the GUI path
            // only. Issue #865: `--serve` shares the same rule — a
            // headless daemon running on a CI host must not steal Ctrl+Alt+M
            // from whatever the operator is using locally.
            //
            // Deliberately NOT inline: every grab goes through the
            // plugin's `run_on_main_thread`, which blocks until the event loop
            // runs the task — and the event loop starts only once this setup
            // hook returns, so registering here would deadlock the app before
            // its first paint (observed under Xvfb: startup stopped right
            // after `config loaded into AppState`). The worker blocks on that
            // hop instead of the main thread; per-slot failures are reported
            // to Settings and are never fatal.
            //
            // Gated on a loaded config for the reason the block used to live
            // inside the `Ok(cfg)` arm: `register_from_config` falls back to
            // the default bindings when AppState holds no config, so an
            // unguarded call after a failed load would grab accelerators the
            // user never configured. Gated on `!cli_mode` so `--serve` and
            // `--sync-once` skip the registration entirely.
            if !cli_mode {
                let shortcuts_config_loaded =
                    app.state::<Arc<AppState>>().config.get().is_some();
                if shortcuts_config_loaded {
                    let shortcut_handle = app.handle().clone();
                    tauri::async_runtime::spawn_blocking(move || {
                        commands::shortcuts::register_from_config(&shortcut_handle);
                    });
                }
            }

            #[cfg(desktop)]
            if !cli_mode {
                use tauri_plugin_deep_link::DeepLinkExt;

                // Issue #66 (further mitigation): re-register the
                // `presencejam://` scheme at every launch so a foreign app
                // that pre-registered the scheme gets clobbered by our
                // last-write. The plugin's `register()` writes
                // `HKCU\Software\Classes\<scheme>` on Windows and
                // `~/.local/share/applications/<scheme>.desktop` plus
                // `xdg-mime default` on Linux; it returns
                // `Err(UnsupportedPlatform)` on macOS, where the claim has to
                // go through CoreServices instead — `macos_deeplink` does that
                // in the error arm below. Startup must not block on either
                // path: PKCE verifier in AppState only (#65) remains the
                // cryptographic mitigation — an interceptor can read the
                // `code` from the callback URL but cannot exchange it for
                // tokens.
                //
                // Issue #865: skipped under `--serve` (and `--sync-once`),
                // so a headless daemon never claims the URL scheme away
                // from a real GUI install on the same machine.
                log::info!("[APP] setup: registering deep links");
                if let Err(e) = app.deep_link().register_all() {
                    #[cfg(target_os = "macos")]
                    {
                        // Issue #66: macOS claims a URL scheme through the app
                        // bundle's `CFBundleURLTypes`, and LaunchServices gives
                        // the *first* claimant priority, so the plugin call
                        // above cannot take `presencejam://` back from an app
                        // that registered it first. CoreServices'
                        // `LSSetDefaultHandlerForURLScheme` writes the user's
                        // preferred handler and does override that. The bundle
                        // id and the scheme list both come from the same
                        // tauri.conf.json the plugin reads, so a scheme added
                        // there is re-claimed automatically. Failure is only
                        // logged — expected under `tauri dev`, where the
                        // process is not an installed bundle.
                        log::warn!(
                            "[APP] setup: deep_link::register_all unsupported on macOS ({e}); \
                             re-claiming the scheme via LSSetDefaultHandlerForURLScheme"
                        );
                        let config = app.config();
                        let schemes = macos_deeplink::configured_schemes(&config.plugins.0);
                        match macos_deeplink::claim(&schemes, &config.identifier) {
                            Ok(true) => log::info!(
                                "[APP] setup: macOS scheme re-claimed via \
                                 LSSetDefaultHandlerForURLScheme for {schemes:?} \
                                 (bundle id {})",
                                config.identifier
                            ),
                            Ok(false) => log::info!(
                                "[APP] setup: macOS scheme re-claim already performed this \
                                 launch; skipping"
                            ),
                            Err(err) => log::warn!(
                                "[APP] setup: macOS scheme re-claim failed ({err}); falling \
                                 back to the #65 PKCE launch-binding defence against scheme \
                                 hijack"
                            ),
                        }
                    }
                    #[cfg(not(target_os = "macos"))]
                    {
                        // #497: name the missing step. The plugin's Linux
                        // branch writes the .desktop file then shells out to
                        // `update-desktop-database` + `xdg-mime default`; an
                        // ENOENT there aborts before the association, so the
                        // file can exist while the scheme stays unassociated.
                        log::error!(
                            "[APP] setup: Failed to register deep links: {e} \
                             (check `update-desktop-database`/`xdg-mime` presence; \
                             if ~/.local/share/applications/presence-jam-handler.desktop \
                             exists but `xdg-mime query default x-scheme-handler/presencejam` \
                             is empty, run `xdg-mime default presence-jam-handler.desktop \
                             x-scheme-handler/presencejam` manually)"
                        );
                        // Best-effort fallback: associate directly when the
                        // database helper is absent (minimal Linux without a
                        // full desktop metapackage). HOME resolves via
                        // directories (BaseDirs); unknown home skips quietly.
                        let apps_dir = directories::BaseDirs::new()
                            .map(|b| b.home_dir().join(".local/share/applications"));
                        let db_missing = match &apps_dir {
                            // #review-5: a non-zero exit is as missing as a
                            // spawn failure — either way the DB was not updated.
                            Some(dir) => !std::process::Command::new("update-desktop-database")
                                .arg(dir)
                                .status()
                                .is_ok_and(|s| s.success()),
                            None => {
                                log::warn!(
                                    "[APP] setup: home dir unknown; skipping deep-link fallback association"
                                );
                                false
                            }
                        };
                        if db_missing {
                            match std::process::Command::new("xdg-mime")
                                .args([
                                    "default",
                                    "presence-jam-handler.desktop",
                                    "x-scheme-handler/presencejam",
                                ])
                                .status()
                            {
                                Ok(s) if s.success() => log::info!(
                                    "[APP] setup: xdg-mime fallback association succeeded"
                                ),
                                Ok(s) => log::warn!(
                                    "[APP] setup: xdg-mime fallback exited with status {s}"
                                ),
                                Err(mime_err) => log::warn!(
                                    "[APP] setup: xdg-mime fallback failed ({mime_err}); \
                                     see SETUP.md Linux prerequisites"
                                ),
                            }
                        }
                    }
                } else {
                    log::info!("[APP] setup: deep links registered successfully");
                }

                // Setup system tray
                log::info!("[APP] setup: setting up system tray");
                if let Err(e) = tray::setup_tray(app) {
                    log::error!("[APP] setup: Failed to setup system tray: {}", e);
                } else {
                    log::info!("[APP] setup: System tray initialized successfully");
                }

                // Setup application menu bar using window menu (not app menu)
                // This ensures click events are properly routed via on_menu_event
                log::info!("[APP] setup: setting up application menu");
                if let Some(window) = app.get_webview_window("main") {
                    if let Err(e) = menu::setup_app_menu(app, &window) {
                        log::error!("[APP] setup: Failed to setup application menu: {}", e);
                    } else {
                        log::info!("[APP] setup: Application menu initialized successfully");
                    }

                    // Register menu event handler on the window
                    // This is critical for macOS - window menus receive click events properly
                    let app_handle = app.handle().clone();
                    log::info!("[APP] setup: registering menu event handler on window");
                    window.on_menu_event(move |_app, event| {
                        let id = event.id().as_ref();
                        log::info!("[APP] window.on_menu_event: id={}", id);
                        menu::handle_app_menu_event(&app_handle, id);
                    });
                } else {
                    log::error!("[APP] setup: could not get main window for menu");
                }

                // Check for deep links on startup
                let start_urls = app.deep_link().get_current();
                log::info!("[APP] setup: checking for start URLs");
                if let Ok(Some(urls)) = start_urls {
                    log::info!("[APP] setup: found {} start URL(s)", urls.len());
                    for url in urls {
                        log::info!("[APP] setup: processing start URL: [REDACTED len {}] prefix={}…", url.as_str().len(), url.as_str().chars().take(4).collect::<String>());
                        handle_deep_link(url.as_str(), app.handle().clone());
                    }
                } else {
                    log::info!("[APP] setup: no start URLs found");
                }

                // Register deep link callback
                let app_handle = app.handle().clone();
                log::info!("[APP] setup: registering on_open_url callback");
                app.deep_link().on_open_url(move |event| {
                    let urls = event.urls();
                    log::info!("[APP] on_open_url: received {} URL(s)", urls.len());
                    for url in urls {
                        log::info!("[APP] on_open_url: processing URL: [REDACTED len {}] prefix={}…", url.as_str().len(), url.as_str().chars().take(4).collect::<String>());
                        handle_deep_link(url.as_str(), app_handle.clone());
                    }
                });
            }

            log::info!("[APP] setup: PresenceJam {} started successfully", env!("CARGO_PKG_VERSION"));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::config::load_config,
            commands::config::save_config,
            commands::config::update_config,
            commands::config::import_working_hours,
            commands::config::set_locale,
            commands::spotify_auth::start_spotify_auth,
            commands::spotify_auth::start_spotify_reconnect,
            commands::spotify_auth::reconnect_spotify_session,
            commands::spotify_auth::complete_spotify_auth_manual,
            commands::spotify_auth::refresh_spotify,
            commands::spotify_auth::is_spotify_client_secret_set,
            commands::teams_auth::start_teams_auth_device_code,
            commands::teams_auth::poll_teams_auth,
            commands::teams_auth::refresh_teams,
            commands::teams_auth::get_teams_granted_scopes,
            commands::teams_auth::cancel_teams_auth_poll,
            commands::sync::start_syncing,
            commands::sync::stop_syncing,
            commands::sync::get_sync_status,
            commands::sync::refresh_status,
            commands::sync::app_exit,
            detach_pane,
            commands::shortcuts::register_shortcuts,
            commands::shortcuts::unregister_shortcuts,
            commands::shortcuts::validate_shortcut,
            commands::window::show_window,
            commands::window::set_autostart_enabled,
            commands::window::open_logs_folder,
            commands::window::open_external_url,
            commands::onboarding::is_onboarding_complete,
            commands::onboarding::complete_onboarding,
            commands::config::export_config,
            commands::config::import_config,
            commands::onboarding::reconnect_spotify,
            commands::onboarding::reconnect_teams,
            commands::misc::preview_status,
            commands::misc::update_tray_menu_state,
            commands::misc::relaunch_app,
            commands::logs::get_recent_logs,
            updater_bg::check_for_update,
            updater_bg::stage_deferred_update,
            updater_bg::clear_failed_update_install,
            updater_bg::cancel_deferred_update,
            commands::playback::get_spotify_granted_scopes,
            diagnostics::get_diagnostics_snapshot,
            diagnostics::save_diagnostics_snapshot,
            commands::status::set_manual_status,
            commands::status::clear_manual_status_command,
            commands::status::load_manual_status_command,
            commands::playback::set_volume,
            commands::playback::seek,
            history::get_presence_history,
            commands::rules::explain_rules,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Issue #585: close-to-tray applies to the main window only.
                // A detached Logs/Settings pane must really close — its
                // badge clears on `tauri://destroyed` — so non-main windows
                // fall through to the platform's normal destroy path.
                if !close_hides_window(window.label()) {
                    log::info!(
                        "[APP] window_event: CloseRequested on detached window '{}', closing it",
                        window.label()
                    );
                    return;
                }
                // Issue #927: a session whose tray failed to initialise has no
                // reachable way back to a hidden window, so close-to-tray must
                // not engage — the close proceeds and the app exits with it.
                if !crate::tray::tray_available() {
                    log::warn!(
                        "[APP] window_event: CloseRequested with no tray — closing instead of hiding"
                    );
                    return;
                }
                log::info!("[APP] window_event: CloseRequested received, hiding window");
                let _ = window.hide();
                // Issue #886: the hide has to reach the tray's visibility mirror,
                // which is what the dedup key is built from.
                crate::tray::note_window_visibility(false);
                api.prevent_close();
            }
        })
        .build(context);
    // Issue #417: a build failure (missing icon, bad capability, plugin
    // init) must not panic the release binary with `.expect` — log the
    // cause and exit non-zero. No panic backtrace, but the OS launcher
    // still sees the failure via the exit code and the log tail.
    let app = match built {
        Ok(app) => app,
        Err(e) => {
            log::error!("[APP] run: failed to build tauri application: {}", e);
            std::process::exit(1);
        }
    };
    app.run(move |app, event| {
        // C3(c) "install on quit": both real exit paths (the shared
        // request_graceful_shutdown in menu.rs backing tray + app-menu
        // Quit, and the app_exit command) funnel into AppHandle::exit,
        // which fires RunEvent::Exit once the event loop has finished —
        // the safe point to apply a staged update (the plugin requires
        // the app to be quitting on Windows).
        // Issue #679: a `--sync-once` run skips both hooks. Installing a
        // staged update is a GUI decision (and the user is not quitting an
        // app), and the presence cleanup would wipe the very status the
        // one-shot was asked to write.
        // Issue #865: a `--serve` run skips the staged update (still a GUI
        // decision) but keeps the presence cleanup — the daemon had an
        // armed presence session and SIGTERM is a clean exit, so the
        // status it advertised on Teams needs the same Paused placeholder
        // every other quit performs.
        #[cfg(desktop)]
        if matches!(event, tauri::RunEvent::Exit) && !sync_once {
            // Order is load-bearing (finding #636, issue #636): the staged
            // update is applied FIRST so a Graph round-trip can never delay an
            // install — and on Windows, where the installer exits the process
            // without returning, an update-driven quit never reaches the
            // presence cleanup at all. On every other exit the cleanup then
            // clears an armed presence session (bounded by its own 3-second
            // client) and replaces a leftover playing status with the
            // short-lived "Paused" placeholder.
            if serve_port.is_none() {
                updater_bg::install_pending_on_exit(app);
            }
            polling::clear_presence_on_exit(app);
        }
    });
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
        let source = include_str!("lib.rs");
        // Find the OnboardingCache struct definition.
        let start = source
            .find("pub struct OnboardingCache")
            .expect("lib.rs must contain OnboardingCache struct");
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
    fn test_polling_sub_struct_lock_and_invalidate() {
        use std::sync::atomic::Ordering;
        use std::sync::mpsc;
        let polling = Polling::new();
        assert!(!polling.is_syncing(Ordering::Acquire));
        assert!(polling.current_track().is_none());
        assert!(polling.handle().is_none());
        assert!(polling.stop_tx().is_none());
        assert!(polling.try_claim());
        assert!(polling.is_syncing(Ordering::Acquire));
        assert!(!polling.try_claim());
        polling.set_syncing(false, Ordering::Release);
        assert!(!polling.is_syncing(Ordering::Acquire));
        let (tx, _rx) = mpsc::channel::<()>();
        *polling.stop_tx_mut() = Some(tx);
        assert!(polling.stop_tx().is_some());
        let handle = std::thread::Builder::new().spawn(|| {}).expect("spawn");
        *polling.handle_mut() = Some(handle);
        assert!(polling.handle().is_some());
    }

    /// Regression guard for issue #66: a future contributor must not
    /// re-gate `app.deep_link().register_all()` to `#[cfg(windows)]`
    /// alone. Per-launch re-registration of the `presencejam://`
    /// scheme is required on Windows AND Linux to defend against a
    /// foreign app pre-registering the scheme. macOS is handled by
    /// `macos_deeplink` inside the call site's error arm (see
    /// `test_macos_deeplink_reclaim_is_wired`) — do NOT reintroduce the
    /// Windows-only gate.
    #[test]
    fn test_register_all_not_gated_to_windows_only() {
        let source = include_str!("lib.rs");
        let needle = "app.deep_link().register_all()";
        // Capture ±10 lines of context around the call site.
        let byte_offset = source.find(needle).unwrap_or_else(|| {
            panic!(
                "no call to `{}` found in lib.rs — the re-registration site \
                 must remain in the desktop setup block",
                needle
            )
        });
        let line_start_byte = source[..byte_offset]
            .rfind('\n')
            .map(|i| i + 1)
            .unwrap_or(0);
        let mut line_start = line_start_byte;
        for _ in 0..10 {
            if line_start == 0 {
                break;
            }
            line_start = source[..line_start - 1]
                .rfind('\n')
                .map(|i| i + 1)
                .unwrap_or(0);
        }
        let match_end = byte_offset + needle.len();
        let mut line_end = match_end;
        for _ in 0..10 {
            if line_end >= source.len() {
                break;
            }
            line_end = source[line_end..]
                .find('\n')
                .map(|i| line_end + i + 1)
                .unwrap_or(source.len());
        }
        let window = &source[line_start..line_end];
        assert!(
            !window.contains("#[cfg(windows)]"),
            "Regression: `{}` is gated to Windows only. Issue #66 \
             requires per-launch re-registration on Windows AND Linux (and \
             the CoreServices re-claim on macOS). Do NOT reintroduce \
             `#[cfg(windows)]` around this call. Offending context:\n{}",
            needle,
            window
        );
    }

    /// Regression guard for the macOS half of issue #66: the CoreServices
    /// re-claim must stay attached to `register_all()`'s failure arm and
    /// stay gated to macOS. Deleting it silently restores the pre-4.6
    /// state where `presencejam://` could be intercepted by whichever app
    /// registered it first, and running it unconditionally would mean
    /// linking CoreServices on Windows/Linux.
    #[test]
    fn test_macos_deeplink_reclaim_is_wired() {
        let source = include_str!("lib.rs");
        assert!(
            source.contains("pub mod macos_deeplink;"),
            "the macos_deeplink module must stay registered in lib.rs"
        );

        let needle = "macos_deeplink::claim(";
        let byte_offset = source.find(needle).unwrap_or_else(|| {
            panic!(
                "no call to `{}` found in lib.rs — the macOS scheme re-claim \
                 must remain in the deep-link setup block",
                needle
            )
        });
        let window_start = byte_offset.saturating_sub(2_000);
        let window_end = (byte_offset + 2_000).min(source.len());
        let window = &source[window_start..window_end];

        assert!(
            window.contains("#[cfg(target_os = \"macos\")]"),
            "the CoreServices re-claim must be `#[cfg(target_os = \"macos\")]` — it \
             pulls in macOS-only dependencies and must not be compiled or linked \
             on other targets. Offending context:\n{}",
            window
        );
        assert!(
            window.contains("app.deep_link().register_all()"),
            "the CoreServices re-claim must be reached from \
             `register_all()`'s failure arm, not from a second call site. \
             Offending context:\n{}",
            window
        );
    }
    #[test]
    fn test_app_state_sub_encapsulation_no_pub_inner_fields() {
        let source = include_str!("lib.rs");
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
    /// Issue #417: `run()` must never `.expect` on the Tauri build in
    /// production — a build failure must log and exit non-zero instead of
    /// panicking the release binary. Brace-counted body isolation
    /// (order-independent): do not anchor on the next fn.
    #[test]
    fn test_run_build_failure_logs_and_exits() {
        let source = include_str!("lib.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("lib.rs has no #[cfg(test)] mod tests block");
        let after_sig = prod_source
            .split("pub fn run()")
            .nth(1)
            .expect("run definition not found");
        let open = after_sig.find('{').expect("run has no opening brace");
        let mut depth = 0usize;
        let mut end = None;
        for (i, ch) in after_sig[open..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(open + i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        let body = &after_sig[open..end.expect("run body never closed")];
        assert!(
            !body.contains(".expect("),
            "run() must not .expect on the Tauri build (issue #417)"
        );
        assert!(
            body.contains("std::process::exit(1)"),
            "run() build failure must exit non-zero"
        );
        assert!(
            body.contains("failed to build tauri application"),
            "run() build failure must log the cause"
        );
    }

    /// Issue #585: only the main window is close-to-tray. Detached panes
    /// clear their `$detachedPanes` badge on `tauri://destroyed`, so hiding
    /// one strands a live-but-invisible window that no `setFocus()` can
    /// bring back.
    #[test]
    fn test_close_hides_window_label_guard() {
        assert!(
            close_hides_window("main"),
            "the main window must stay close-to-tray"
        );
        for detached in ["logs-detached", "settings-detached", "other", ""] {
            assert!(
                !close_hides_window(detached),
                "window `{}` must really close, not hide",
                detached
            );
        }
        // The handler must actually consult the guard before hiding, and still
        // prevent the close for the main window. Anchored on the arm's own
        // statements rather than a fixed byte window: the arm grew with the
        // #927 no-tray guard and the #886 visibility report, and a byte window
        // would silently stop covering `api.prevent_close()` when it does.
        let source = include_str!("lib.rs");
        let arm = source
            .find("tauri::WindowEvent::CloseRequested")
            .expect("lib.rs must handle WindowEvent::CloseRequested");
        let tail = &source[arm..];
        let guard = tail
            .find("close_hides_window(window.label())")
            .expect("the CloseRequested arm must guard on the window label (issue #585)");
        let hide = tail
            .find("window.hide()")
            .expect("the main window must still be hidden (close-to-tray)");
        let prevent = tail
            .find("api.prevent_close()")
            .expect("the main window must still prevent the close (close-to-tray)");
        assert!(
            guard < hide && hide < prevent,
            "the arm must guard on the label, hide, and then prevent the close"
        );
    }

    /// Issue #589: the autostart plugin passes `--minimized`, which must be
    /// parsed rather than silently ignored — and matched exactly, so a
    /// future argument that merely starts with the flag is not mistaken
    /// for it.
    #[test]
    fn test_has_minimized_flag_parses_autostart_arg() {
        assert!(
            has_minimized_flag(vec!["presencejam.exe", "--minimized"]),
            "the autostart argv must be recognised"
        );
        assert!(
            has_minimized_flag(vec!["presencejam".to_string(), MINIMIZED_FLAG.to_string()]),
            "OsString argv elements must be recognised too"
        );
        assert!(
            !has_minimized_flag(vec!["presencejam.exe"]),
            "a plain launch must not start hidden"
        );
        assert!(
            !has_minimized_flag(Vec::<String>::new()),
            "an empty argv must not start hidden"
        );
        assert!(
            !has_minimized_flag(vec!["--minimized-please"]),
            "the flag is matched exactly, never as a prefix"
        );
    }

    /// 4.7.0 (S5, #673): every clamped `keep_files` maps to `KeepSome`, at `1`
    /// included. `KeepOne` looks like the equivalent at 1 but deletes the active
    /// log without ever pruning the dated archives, so `keep_files = 1` kept
    /// every existing archive — the field's documented retention ("archived
    /// log files retained") was not honoured at all.
    #[test]
    fn test_log_rotation_strategy_always_keeps_some() {
        for keep in 1..=20u32 {
            assert!(
                matches!(
                    log_rotation_strategy(keep),
                    tauri_plugin_log::RotationStrategy::KeepSome(n) if n == keep as usize
                ),
                "keep_files={keep} must rotate with KeepSome({keep})"
            );
        }
        // Below the clamp floor the plugin's `keep_count - 1` would underflow.
        assert!(
            matches!(
                log_rotation_strategy(0),
                tauri_plugin_log::RotationStrategy::KeepSome(1)
            ),
            "a 0 that slipped past clamp_logging must still floor at KeepSome(1)"
        );
    }

    /// Brace-counted body isolation for a top-level `fn` in this file (house
    /// style — order-independent, never anchored on the following fn, which
    /// drifts).
    fn body_of<'a>(prod_source: &'a str, sig: &str) -> &'a str {
        let after_sig = prod_source
            .split(sig)
            .nth(1)
            .unwrap_or_else(|| panic!("lib.rs has no `{}`", sig));
        let open = after_sig
            .find('{')
            .unwrap_or_else(|| panic!("{} has no opening brace", sig));
        let mut depth = 0usize;
        let mut end = None;
        for (i, ch) in after_sig[open..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(open + i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        &after_sig[..end.unwrap_or_else(|| panic!("{} body never closed", sig))]
    }

    /// Issue #679: the three CLI flags are recognised, matched exactly (never
    /// as a prefix of something else) and nothing else is. Issue #865
    /// extends the contract with `--serve[=PORT]`.
    #[test]
    fn test_cli_command_matches_only_the_exact_flags() {
        for (argv, expected) in [
            (vec!["presencejam", "--help"], Some(CliCommand::Help)),
            (vec!["presencejam", "--status"], Some(CliCommand::Status)),
            (
                vec!["presencejam", "--sync-once"],
                Some(CliCommand::SyncOnce),
            ),
            // Issue #865: `--serve` defaults the port; `--serve=PORT` carries
            // it in the same argv token.
            (
                vec!["presencejam", "--serve"],
                Some(CliCommand::Serve(None)),
            ),
            (
                vec!["presencejam", "--serve=8649"],
                Some(CliCommand::Serve(Some(8649))),
            ),
            (
                vec!["presencejam", "--serve=1"],
                Some(CliCommand::Serve(Some(1))),
            ),
            // Issue #896: `--daemon` is a bare flag (no `=PORT` form).
            (vec!["presencejam", "--daemon"], Some(CliCommand::Daemon)),
            // Exact match only: a longer argument that merely starts with a
            // flag is not that flag (issue #589's rule, applied here too).
            (vec!["presencejam", "--statuses"], None),
            (vec!["presencejam", "--sync-once-now"], None),
            (vec!["presencejam", "--help-me"], None),
            (vec!["presencejam", "-s"], None),
            (vec!["presencejam", "--status=1"], None),
            // `--serve` typos and edge cases must fall through (GUI launches
            // — a typo is louder than a silent error).
            (vec!["presencejam", "--serve="], None),
            (vec!["presencejam", "--serve=abc"], None),
            (vec!["presencejam", "--serve=0"], None),
            (vec!["presencejam", "--serve=99999"], None),
            (vec!["presencejam", "--server"], None),
            // `--daemon` typos and edge cases must fall through too.
            (vec!["presencejam", "--daemon=8080"], None),
            (vec!["presencejam", "--daemons"], None),
            (Vec::<&str>::new(), None),
        ] {
            assert_eq!(
                cli_command(argv.clone()),
                expected,
                "argv {:?} must parse to {:?}",
                argv,
                expected
            );
        }

        // OsString argv elements must work too — that is what the real process
        // argv hands the parser.
        assert_eq!(
            cli_command(vec![
                std::ffi::OsString::from("presencejam"),
                std::ffi::OsString::from(SYNC_ONCE_FLAG),
            ]),
            Some(CliCommand::SyncOnce),
            "OsString argv elements must be recognised, like has_minimized_flag"
        );
    }

    /// Issue #679: the flags are an alternative to launching the GUI, never a
    /// modifier of it — every argv shape the app already receives (a bare
    /// launch, the autostart plugin's `--minimized`, a `presencejam://` deep
    /// link and the occasional stray argument) must still launch as it did
    /// before, i.e. parse to no CLI command at all.
    #[test]
    fn test_cli_command_leaves_every_gui_launch_alone() {
        for argv in [
            vec!["presencejam"],
            vec!["/usr/bin/presence-jam"],
            vec!["presence-jam.exe", MINIMIZED_FLAG],
            vec![
                "presencejam",
                "--minimized",
                "presencejam://callback?code=abc&state=def",
            ],
            vec!["presencejam", "presencejam://callback"],
            vec!["presencejam", "--some-future-flag", "value"],
            vec!["presencejam", "-"],
        ] {
            assert_eq!(
                cli_command(argv.clone()),
                None,
                "argv {:?} must launch the GUI, not a CLI mode",
                argv
            );
        }
    }

    /// Issue #679: the parser is documented as "first recognised flag wins",
    /// left to right.
    #[test]
    fn test_cli_command_first_recognised_flag_wins() {
        assert_eq!(
            cli_command(vec!["presencejam", SYNC_ONCE_FLAG, STATUS_FLAG]),
            Some(CliCommand::SyncOnce),
            "the leftmost recognised flag decides"
        );
        assert_eq!(
            cli_command(vec!["presencejam", "--unknown", STATUS_FLAG, HELP_FLAG]),
            Some(CliCommand::Status),
            "unknown arguments are skipped, not treated as a choice"
        );
    }

    /// Issue #679: `--help` (and the README/USAGE docs, asserted in the docs
    /// themselves) must document all four flags, and say what happens to
    /// anything else.
    #[test]
    fn test_cli_help_text_documents_every_flag() {
        let help = cli_help_text();
        for flag in [
            STATUS_FLAG,
            SYNC_ONCE_FLAG,
            HELP_FLAG,
            MINIMIZED_FLAG,
            SERVE_FLAG,
            DAEMON_FLAG,
        ] {
            assert!(help.contains(flag), "the usage text must document {}", flag);
        }
        assert!(
            help.contains("exit 0") && help.contains("exit 1"),
            "the usage text must state the exit codes"
        );
        assert!(
            help.to_lowercase().contains("ignored"),
            "the usage text must state that unknown arguments are ignored"
        );
        // Issue #865: the serve surface's two load-bearing claims — the
        // `Authorization: Bearer` requirement and the keychain-stored
        // token — must both appear, so a future copy edit cannot silently
        // regress the security model.
        assert!(
            help.contains("Bearer"),
            "the serve flag must call out the bearer-token requirement"
        );
        assert!(
            help.contains("keychain"),
            "the serve flag must state the token lives in the OS keychain"
        );
        // Issue #896: the daemon's two non-negotiables — SIGTERM → exit 0
        // and the omission of the GUI surfaces — must both appear.
        assert!(
            help.contains("SIGTERM"),
            "the daemon flag must call out SIGTERM as the clean-stop signal"
        );
        assert!(
            help.contains("single-instance"),
            "the daemon flag must call out the single-instance-lock omission"
        );
    }

    /// Issue #679: the `--sync-once` credential gate. This is the seam the
    /// contract's "with credentials it would exit 0" half is proven at: the
    /// credentialed run itself needs a display (the poller works through an
    /// `AppHandle`, which only exists once a GUI runtime does), so the decision
    /// is pinned here instead of being assumed.
    #[test]
    fn test_sync_once_preflight_requires_both_providers() {
        use crate::spotify::SpotifyTokens;
        use crate::teams::TeamsTokens;

        let complete_tokens = token_io::TokensFile {
            spotify_tokens: Some(SpotifyTokens {
                access_token: "at".to_string(),
                refresh_token: "rt".to_string(),
                expires_at: chrono::Utc::now() + chrono::Duration::seconds(3600),
            }),
            teams_tokens: Some(TeamsTokens {
                access_token: "tat".to_string(),
                refresh_token: None,
                expires_at: chrono::Utc::now() + chrono::Duration::seconds(3600),
            }),
        };
        let mut config = crate::config::AppConfig::default();

        // Functionally signed in: the one-shot may run.
        config.spotify.client_id = "client-id".to_string();
        assert_eq!(
            cli_sync_once_preflight(&config, &complete_tokens),
            Ok(()),
            "configured + signed in to both providers must pass the gate"
        );

        // Not configured at all.
        config.spotify.client_id = String::new();
        let reason = cli_sync_once_preflight(&config, &complete_tokens)
            .expect_err("an unconfigured client_id must be rejected");
        assert!(
            reason.contains("client_id"),
            "the reason must name the missing client_id, got: {reason}"
        );

        // Configured but no Spotify session.
        config.spotify.client_id = "client-id".to_string();
        let no_spotify = token_io::TokensFile {
            spotify_tokens: None,
            teams_tokens: complete_tokens.teams_tokens.clone(),
        };
        let reason = cli_sync_once_preflight(&config, &no_spotify)
            .expect_err("no Spotify tokens must be rejected");
        assert!(
            reason.contains("Spotify"),
            "the reason must name Spotify, got: {reason}"
        );

        // Spotify ok, but the Teams status write has no session to use.
        let no_teams = token_io::TokensFile {
            spotify_tokens: complete_tokens.spotify_tokens.clone(),
            teams_tokens: None,
        };
        let reason = cli_sync_once_preflight(&config, &no_teams)
            .expect_err("no Teams tokens must be rejected");
        assert!(
            reason.contains("Teams"),
            "the reason must name Teams, got: {reason}"
        );
    }

    /// Issue #679: the flags must be reachable only as an alternative to the
    /// GUI, and `--sync-once` must reach its one-shot before any GUI surface is
    /// set up. That ordering is what keeps a bare launch on the old path
    /// (asserted by the parser tests above) and a CLI run windowless.
    #[test]
    fn test_cli_flags_short_circuit_before_the_gui_is_built() {
        let source = include_str!("lib.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("lib.rs has no #[cfg(test)] mod tests block");

        let run_body = body_of(prod_source, "pub fn run()");
        let dispatch = run_body
            .find("cli_command(std::env::args_os())")
            .expect("run() must consult the CLI parser (issue #679)");
        let builder = run_body
            .find("tauri::Builder::default()")
            .expect("run() must still build the Tauri app");
        assert!(
            dispatch < builder,
            "the CLI dispatch must precede the Tauri builder: a flag handled \
             after the builder exists has already created a runtime, which is \
             exactly what cannot happen headless (issue #679)"
        );
        assert!(
            run_body.contains("std::process::exit(cli_status_exit_code())"),
            "--status must print and exit without building an app (issue #679)"
        );
        assert!(
            run_body.contains("suppress_config_windows(&mut context)"),
            "--sync-once must suppress the config-declared windows (issue #679)"
        );

        let setup_body = body_of(prod_source, ".setup(move |app|");
        let one_shot = setup_body
            .find("return cli_sync_once_iteration(")
            .expect("setup must hand --sync-once its iteration (issue #679)");
        let tray = setup_body
            .find("tray::setup_tray(")
            .expect("setup must still build the tray for the GUI");
        assert!(
            one_shot < tray,
            "the --sync-once early return must come before the tray/menu setup \
             so a CLI run registers no tray icon (issue #679)"
        );
        assert!(
            !setup_body[..one_shot].contains("register_all()"),
            "a CLI run must not re-register the deep-link scheme (issue #679)"
        );
        // Issue #769: the global-shortcut registration is a GUI surface as
        // well — it takes OS-level accelerator grabs — so it must sit below
        // the `--sync-once` early return too, not merely below the tray.
        let shortcuts = setup_body
            .find("commands::shortcuts::register_from_config(")
            .expect("setup must still register the configured shortcuts for the GUI");
        assert!(
            one_shot < shortcuts,
            "the --sync-once early return must come before the global-shortcut \
             registration so a CLI run grabs no accelerator (issue #769)"
        );
    }

    /// Issue #679: `--sync-once` must run without a window, so the windows
    /// `tauri.conf.json` declares must have their `create` flag cleared before
    /// the app is built (`App::run` builds one webview per `create = true`
    /// entry). Exercised against the real embedded config, so a window added to
    /// `tauri.conf.json` later is covered without touching this test.
    ///
    /// Linux-only: `tauri::generate_context!()` emits a static `EmbedInfo`
    /// whose macOS `_EMBED_INFO_PLIST` / Windows resource symbol collides when
    /// the test binary is linked against the same crate (the production
    /// `run()` already calls it once). The CLI suppression itself is
    /// platform-agnostic and the Linux leg covers it.
    #[cfg(target_os = "linux")]
    #[test]
    fn test_cli_mode_suppresses_every_config_window() {
        let mut context: tauri::Context<tauri::Wry> = tauri::generate_context!();
        let declared = context.config().app.windows.len();
        assert!(
            declared > 0,
            "the app must declare at least one window for this guard to mean anything"
        );
        assert!(
            context.config().app.windows.iter().any(|w| w.create),
            "at least one declared window must be created by default — otherwise \
             the suppression below is vacuous"
        );

        let suppressed = suppress_config_windows(&mut context);
        assert_eq!(
            suppressed, declared,
            "every declared window must be accounted for"
        );
        assert!(
            context.config().app.windows.iter().all(|w| !w.create),
            "no config-declared window may be created in CLI mode (issue #679)"
        );
    }

    /// Issue #679 review round 2: a failed `--sync-once` iteration must report
    /// failure to the shell. The verdict used to be handed to
    /// `AppHandle::exit`, whose code the runtime drops (`RequestExit` →
    /// `ControlFlow::Exit` → `process::exit(0)`), so this pins the mapping the
    /// flag now exits with itself: no captured failure signal is success,
    /// anything the poller announced is 1.
    #[test]
    fn test_sync_once_exit_code_maps_the_verdict() {
        assert_eq!(
            cli_sync_once_exit_code(None),
            0,
            "a completed iteration (no failure signal) must exit 0"
        );
        assert_eq!(
            cli_sync_once_exit_code(Some("spotify: Failed to get currently playing: boom")),
            1,
            "a captured poller failure must exit 1, not report success"
        );
        assert_eq!(
            cli_sync_once_exit_code(Some(
                "reconnect-required: a provider needs to be reconnected"
            )),
            1,
            "a reconnect signal is a failure too"
        );
    }

    /// Issue #922: the detached panes are built from a closed table, so the
    /// label, the in-app URL and the size cannot be steered from the webview —
    /// which is what lets the main window drop the unscoped
    /// `core:webview:allow-create-webview-window` grant.
    #[test]
    fn test_detached_pane_spec_is_a_closed_table() {
        let logs = detached_pane_spec("logs", None).expect("logs is a configured pane");
        assert_eq!(logs.label, "logs-detached");
        assert_eq!(logs.url, "/detached/logs");
        assert!(logs.title.contains("Logs"), "the title must name the pane");
        assert_eq!((logs.width, logs.height), (720.0, 520.0));

        let settings = detached_pane_spec("settings", None).expect("settings is a configured pane");
        assert_eq!(settings.label, "settings-detached");
        assert_eq!(settings.url, "/detached/settings");
        assert_eq!((settings.width, settings.height), (620.0, 720.0));

        // Issue #433: the theme rides on the URL, and only the two values the
        // frontend can read from localStorage are accepted — an unexpected
        // value must not reach the URL.
        for theme in ["dark", "light"] {
            assert_eq!(
                detached_pane_spec("logs", Some(theme))
                    .expect("a stored theme is valid")
                    .url,
                format!("/detached/logs?theme={theme}")
            );
        }
        assert_eq!(
            detached_pane_spec("logs", Some("dark&x=https://evil.example"))
                .expect("an unaccepted theme is ignored, not interpolated")
                .url,
            "/detached/logs"
        );

        // Nothing outside the two configured panes may open a window, and the
        // labels the store already knows are not pane names either.
        for pane in [
            "",
            "main",
            "logs-detached",
            "../logs",
            "LOGS",
            "settings/../logs",
        ] {
            assert!(
                detached_pane_spec(pane, None).is_err(),
                "`{pane}` is not a detached pane and must be refused"
            );
        }

        // The command is the only path, so the store must no longer construct
        // a window itself (issue #922) — that is what the grant was for.
        let store = include_str!("../../src/lib/stores/detach.ts");
        assert!(
            store.contains("invoke('detach_pane'"),
            "the store must open panes through the detach_pane command"
        );
        assert!(
            !store.contains("new WebviewWindow"),
            "the store must not create webview windows from the main window (issue #922)"
        );
    }

    /// Issue #799: one Win/Linux second-instance launch must redeem the
    /// authorization `code` exactly once. The single-instance plugin (built
    /// with the `deep-link` feature) already routes the argv URL through
    /// `handle_cli_arguments` → `deep-link://new-url` → `on_open_url` →
    /// `handle_deep_link`, so the argv scan that used to live in
    /// `forward_launch_to_running_instance` double-dispatched every callback
    /// (the second exchange failed → spurious `spotify-auth-failed`). This
    /// guard fails pre-fix (the body calls `handle_deep_link` on an argv
    /// `presencejam://` URL) and passes post-fix. Brace-counted body
    /// isolation via the shared `body_of` helper (order-independent, never
    /// anchored on the following fn).
    #[test]
    fn test_forward_launch_does_not_redispatch_deep_links() {
        let source = include_str!("lib.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("lib.rs has no #[cfg(test)] mod tests block");
        let body = body_of(prod_source, "fn forward_launch_to_running_instance(");
        assert!(
            !body.contains("handle_deep_link"),
            "forward_launch_to_running_instance must not call handle_deep_link: \
             the single-instance plugin's deep-link feature already routes the \
             argv URL via on_open_url, and a second dispatch redeems the same \
             Spotify code twice (issue #799)"
        );
        assert!(
            !body.contains("presencejam://"),
            "forward_launch_to_running_instance must not scan argv for deep-link \
             URLs (issue #799)"
        );
    }

    /// Issue #799: `handle_deep_link` must pass every callback through the
    /// single-flight gate before spawning the token exchange, so the two
    /// delivery paths for one URL (`get_current` start URL + `on_open_url`
    /// event, or any plugin re-emit) collapse to exactly one exchange.
    #[test]
    fn test_handle_deep_link_claims_single_flight_before_dispatch() {
        let source = include_str!("lib.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("lib.rs has no #[cfg(test)] mod tests block");
        let body = body_of(prod_source, "fn handle_deep_link(");
        assert!(
            body.contains("deep_link_callback_key("),
            "handle_deep_link must hash the (code, state) pair for the \
             single-flight gate (issue #799)"
        );
        assert!(
            body.contains("deep_link_seen.claim("),
            "handle_deep_link must claim the single-flight gate before spawning \
             the token exchange (issue #799)"
        );
    }

    /// Issue #799: the gate behind the single-flight claim. Drives one URL
    /// through both delivery paths (same `(code, state)` key claimed twice)
    /// and asserts exactly one proceeds; a different pair and an expired
    /// window proceed normally so legitimate retries are never wedged.
    /// Fails pre-fix (no gate ⇒ two spawns); passes post-fix.
    #[test]
    fn test_deep_link_dedup_single_flight_per_code_and_state() {
        let key_a = deep_link_callback_key("code-abc", Some("csrf.secret"));
        // Same inputs hash to the same key — the two delivery paths for one
        // URL meet at the gate.
        assert_eq!(
            deep_link_callback_key("code-abc", Some("csrf.secret")),
            key_a,
            "identical (code, state) pairs must map to one dedup key"
        );
        // Either component differs → a different login, not a repeat.
        assert_ne!(
            deep_link_callback_key("code-xyz", Some("csrf.secret")),
            key_a,
            "a different code must not share the dedup key"
        );
        assert_ne!(
            deep_link_callback_key("code-abc", Some("other.secret")),
            key_a,
            "a different state must not share the dedup key"
        );

        // Both delivery paths for one URL: first proceeds, repeat drops.
        let gate = DeepLinkDedup::new();
        let t0 = Instant::now();
        assert!(gate.claim_at(key_a, t0), "first delivery must proceed");
        assert!(
            !gate.claim_at(key_a, t0 + std::time::Duration::from_secs(1)),
            "identical repeat inside the window must be dropped"
        );
        assert!(
            !gate.claim_at(
                key_a,
                t0 + DEEP_LINK_DEDUP_WINDOW - std::time::Duration::from_secs(1)
            ),
            "repeat at the window edge must still be dropped"
        );

        // A different (code, state) pair is a new login, not a repeat.
        let gate = DeepLinkDedup::new();
        assert!(gate.claim_at(key_a, t0), "first delivery must proceed");
        assert!(
            gate.claim_at(
                deep_link_callback_key("code-xyz", Some("csrf.secret")),
                t0 + std::time::Duration::from_secs(1)
            ),
            "a different (code, state) pair must proceed"
        );

        // After the window the same pair is a fresh delivery, so a retried
        // flow can never wedge behind a stale claim.
        let gate = DeepLinkDedup::new();
        assert!(gate.claim_at(key_a, t0), "first delivery must proceed");
        assert!(
            gate.claim_at(
                key_a,
                t0 + DEEP_LINK_DEDUP_WINDOW + std::time::Duration::from_secs(1)
            ),
            "same pair past the window must proceed"
        );
    }
}
