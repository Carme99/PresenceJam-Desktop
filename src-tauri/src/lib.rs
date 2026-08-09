use parking_lot::{Mutex, RwLock};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::Arc;
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
        Self { state: Mutex::new(None) }
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
    pub fn spotify(&self) -> parking_lot::RwLockReadGuard<'_, Option<crate::spotify::SpotifyTokens>> {
        self.spotify.read()
    }

    /// Write guard for the Spotify token slot. Use this instead of
    /// touching the `spotify` field directly.
    pub fn spotify_mut(&self) -> parking_lot::RwLockWriteGuard<'_, Option<crate::spotify::SpotifyTokens>> {
        self.spotify.write()
    }

    /// Read guard for the Teams token slot. Use this instead of
    /// touching the `teams` field directly.
    pub fn teams(&self) -> parking_lot::RwLockReadGuard<'_, Option<crate::teams::TeamsTokens>> {
        self.teams.read()
    }

    /// Write guard for the Teams token slot. Use this instead of
    /// touching the `teams` field directly.
    pub fn teams_mut(&self) -> parking_lot::RwLockWriteGuard<'_, Option<crate::teams::TeamsTokens>> {
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
}

impl Polling {
    pub fn new() -> Self {
        Self {
            is_syncing: AtomicBool::new(false),
            handle: RwLock::new(None),
            stop_tx: RwLock::new(None),
            current_track: RwLock::new(None),
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
            .compare_exchange(false, true, std::sync::atomic::Ordering::AcqRel, std::sync::atomic::Ordering::Acquire)
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
    pub fn current_track(&self) -> parking_lot::RwLockReadGuard<'_, Option<crate::spotify::TrackInfo>> {
        self.current_track.read()
    }

    /// Write guard for the last observed track.
    pub fn current_track_mut(&self) -> parking_lot::RwLockWriteGuard<'_, Option<crate::spotify::TrackInfo>> {
        self.current_track.write()
    }
}

impl Default for Polling {
    /// Required by `clippy::new_without_default`. Equivalent to
    /// `Polling::new()`.
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
}

impl AppState {
    pub fn new() -> Self {
        log::info!("[APP_STATE] AppState::new: creating new AppState");
        Self {
            tokens: Tokens::new(),
            polling: Polling::new(),
            pending: PendingAuths::new(),
            config: Config::new(),
            onboarding_cache: OnboardingCache::new(),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        // Routes through `new()` so the AppState creation log line still fires.
        Self::new()
    }
}

pub mod commands;
pub mod config;
pub mod keychain;
pub mod menu;
pub mod pkce;
pub mod polling;
pub mod profanity;
pub mod spotify;
pub mod teams;
pub mod token_io;
pub mod tray;

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

    let pending = {
        let mut guard = app_state.pending.spotify_mut();
        log::info!("[CALLBACK] handle_spotify_callback: taking pending Spotify auth from state");
        guard.take().ok_or_else(|| {
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
        if state_str != pending.state {
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
    // the verifier being in AppState only (#65). A full fix needs per-launch
    // custom-scheme registration (OS-specific) and is tracked in issue #66.
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
    let tokens = tauri::async_runtime::spawn_blocking(move || {
        crate::spotify::complete_spotify_auth(
            &code,
            &verifier,
            &client_id,
            &client_secret,
            &redirect_uri,
        )
    })
    .await
    .map_err(|e| format!("Spotify OAuth callback task failed: {}", e))??;
    log::info!(
        "[CALLBACK] handle_spotify_callback: token exchange successful - access_token.len={}",
        tokens.access_token.len()
    );

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
    log::debug!("[DEEP_LINK] handle_deep_link: ENTRY - url={}", url);

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
                let app_clone = app.clone();
                let code_clone = code_str.clone();
                let state_clone = state_param.clone();

                // Issue #66 (deferred): a per-launch UUID in the redirect
                // URI path would defend against another app pre-registering
                // the `presencejam://` scheme. Spotify requires exact
                // redirect-URI match in the registered app, so a path
                // component breaks the OAuth round-trip. A full fix needs
                // per-launch custom-scheme registration (OS-specific).
                // For now, the verifier-in-memory fix from #65 means an
                // interceptor can read the `code` but cannot exchange it
                // for tokens — the verifier is in our AppState, not on
                // disk and not exposed via IPC.
                log::info!("[DEEP_LINK] handle_deep_link: routing to Spotify callback");
                tauri::async_runtime::spawn(async move {
                    log::info!(
                        "[DEEP_LINK] handle_deep_link: spawning Spotify callback handler"
                    );
                    if let Err(e) =
                        handle_spotify_callback(&code_clone, state_clone.as_deref(), &app_clone)
                            .await
                    {
                        log::error!("[DEEP_LINK] handle_spotify_callback: FAILED - {}", e);
                        log::info!(
                            "[DEEP_LINK] handle_deep_link: EMIT spotify-auth-failed event"
                        );
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    log::info!("[APP] run: ENTRY");

    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        use tauri_plugin_single_instance::init as single_instance_init;

        builder = builder.plugin(single_instance_init(|app, argv, _cwd| {
            // Raise the existing window so the user sees it when a second
            // instance is launched (e.g., double-click the .msi shortcut
            // while the app is running, or a deep-link click from a
            // browser when the app is already open).
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
            // argv[0] is the exe path; scan for a presencejam:// URL
            // (Windows + Linux pass deep links as argv when the scheme
            // is invoked; macOS uses the deep-link plugin's on_open_url
            // callback, which is also wired below).
            for arg in argv.iter().skip(1) {
                if arg.starts_with("presencejam://") {
                    log::info!(
                        "[APP] single_instance: forwarding deep-link argv to handle_deep_link"
                    );
                    handle_deep_link(arg, app.clone());
                }
            }
        }));

        builder = builder.plugin(tauri_plugin_deep_link::init());
        log::info!("[APP] run: deep_link plugin registered");
    }

    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_store::Builder::new().build())
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
            .build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_http::init())
        .setup(|app| {
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
                // (`~/Library/Logs/PresenceJam/`). The `log::error!` above routes through
                // `tauri-plugin-log`, which is the canonical destination for user-visible
                // log lines and the file the `open_logs_folder` command points at. The
                // previous dual-write left a silent failure mode where the panic appeared
                // in a terminal nobody was reading but never in the log file the user could
                // open. See issue #79.
            }));


            log::info!("[APP] setup: ENTRY");

            let state = Arc::new(AppState::new());
            app.manage(state.clone());
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
                    if cfg.teams.start_minimized {
                        log::info!("[APP] setup: start_minimized enabled, hiding window");
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.hide();
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
            // call on every launch; no-op once the field is gone. See audit Q3
            // and issue #9.
            config::migrate_legacy_client_secret();

            // Load persisted tokens (Spotify + Teams) into AppState. We bypass
            // `tauri-plugin-store` for the tokens file and read it directly
            // as JSON from `<app-config-dir>/PresenceJam/tokens.json`. See
            // issue #65.
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
            #[cfg(desktop)]
            {
                use tauri_plugin_deep_link::DeepLinkExt;

                // Issue #66 (further mitigation): re-register the
                // `presencejam://` scheme at every launch so a foreign app
                // that pre-registered the scheme gets clobbered by our
                // last-write. The plugin's `register()` writes
                // `HKCU\Software\Classes\<scheme>` on Windows and
                // `~/.local/share/applications/<scheme>.desktop` plus
                // `xdg-mime default` on Linux; it returns
                // `Err(UnsupportedPlatform)` on macOS — we log that case
                // and continue, since startup must not block on a known
                // platform gap. PKCE verifier in AppState only (#65)
                // remains the macOS mitigation: an interceptor can read
                // the `code` from the callback URL but cannot exchange it
                // for tokens.
                log::info!("[APP] setup: registering deep links");
                if let Err(e) = app.deep_link().register_all() {
                    #[cfg(target_os = "macos")]
                    log::warn!(
                        "[APP] setup: deep_link::register_all unsupported on macOS ({e}); \
                         relying on #65 PKCE-only mitigation for scheme hijack defence"
                    );
                    #[cfg(not(target_os = "macos"))]
                    log::error!("[APP] setup: Failed to register deep links: {}", e);
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
                        log::info!("[APP] setup: processing start URL: {}", url);
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
                        log::info!("[APP] on_open_url: processing URL: {}", url);
                        handle_deep_link(url.as_str(), app_handle.clone());
                    }
                });
            }

            log::info!("[APP] setup: PresenceJam 2.0 started successfully");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::config::load_config,
            commands::config::save_config,
            commands::spotify_auth::start_spotify_auth,
            commands::spotify_auth::start_spotify_reconnect,
            commands::spotify_auth::complete_spotify_auth_manual,
            commands::spotify_auth::refresh_spotify,
            commands::spotify_auth::is_spotify_client_secret_set,
            commands::teams_auth::start_teams_auth_device_code,
            commands::teams_auth::poll_teams_auth,
            commands::teams_auth::refresh_teams,
            commands::sync::start_syncing,
            commands::sync::stop_syncing,
            commands::sync::get_sync_status,
            commands::sync::app_exit,
            commands::window::show_window,
            commands::window::set_autostart_enabled,
            commands::window::open_logs_folder,
            commands::window::open_external_url,
            commands::onboarding::is_onboarding_complete,
            commands::onboarding::complete_onboarding,
            commands::onboarding::reconnect_spotify,
            commands::onboarding::reconnect_teams,
            commands::misc::preview_status,
            commands::misc::update_tray_menu_state,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                log::info!("[APP] window_event: CloseRequested received, hiding window");
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
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
            !struct_body.lines().any(|l| l.trim_start().starts_with("pub state")),
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
        assert!(tokens.spotify().is_none(), "fresh Tokens must have no Spotify token");
        assert!(tokens.teams().is_none(), "fresh Tokens must have no Teams token");

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
        assert!(taken.is_some(), "take() must return the stored Spotify token");
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
    /// foreign app pre-registering the scheme. The macOS path is a
    /// known gap handled inside the call site (logs a warning, does
    /// not crash startup) — the platform gap is documented in #66
    /// and the changelog; do NOT reintroduce the Windows-only gate.
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
        let line_start_byte = source[..byte_offset].rfind('\n').map(|i| i + 1).unwrap_or(0);
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
             requires per-launch re-registration on Windows AND Linux. \
             The macOS gap is handled inside the call site (logs \
             a warning, does not crash) — do NOT reintroduce \
             `#[cfg(windows)]` around this call. Offending context:\n{}",
            needle, window
        );
    }
    #[test]
    fn test_app_state_sub_encapsulation_no_pub_inner_fields() {
        let source = include_str!("lib.rs");
        for name in ["Tokens", "Polling", "PendingAuths", "Config"] {
            let header = format!("pub struct {}", name);
            let start = source.find(&header)
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
}
