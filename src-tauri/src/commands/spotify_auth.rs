//! Spotify authentication Tauri commands.
//!
//! See issue #76. This module owns the PKCE OAuth flow for both initial
//! Onboarding (`start_spotify_auth`) and Reconnect (`start_spotify_reconnect`),
//! plus the manual-code fallback (`complete_spotify_auth_manual`) and the
//! in-flight token refresher (`refresh_spotify`).

use crate::token_io;
use crate::{AppState, PendingSpotifyAuth};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

/// Log tag prefix for this submodule (issue #79 item 3).
const CMD: &str = "[CMD.SPOTIFY_AUTH]";

/// Space-separated Spotify OAuth scopes requested in the authorize URL.
/// Single source of truth for the requested scope set — `config.spotify.scopes`
/// was removed as dead config (issue #163). The space must be percent-encoded
/// in the query string, hence `urlencoding::encode(SPOTIFY_SCOPES)` (issue
/// #164). `user-modify-playback-state` was added for tray playback control
/// (issue #3.0-P3); devices + queue only need the read scopes already held.
const SPOTIFY_SCOPES: &str =
    "user-read-currently-playing user-read-playback-state user-modify-playback-state";

/// Validates a Spotify client_id (32 alphanumeric chars).
/// See issue #67.
fn validate_spotify_client_id(id: &str) -> Result<(), String> {
    if id.len() != 32 || !id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(format!(
            "Invalid client_id: must be 32 alphanumeric characters (got len={})",
            id.len()
        ));
    }
    Ok(())
}

/// Validates a Spotify client_secret (32-512 ASCII alphanumeric chars).
/// See issue #67 (length floor) and issue #354 (charset check mirroring
/// `validate_spotify_client_id` plus the 512-char IPC cap).
fn validate_spotify_client_secret(secret: &str) -> Result<(), String> {
    if secret.len() < 32 {
        return Err(format!(
            "Invalid client_secret: must be at least 32 characters (got len={})",
            secret.len()
        ));
    }
    if secret.len() > 512 {
        return Err(format!(
            "Invalid client_secret: must be at most 512 characters (got len={})",
            secret.len()
        ));
    }
    if !secret.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(
            "Invalid client_secret: must contain only ASCII alphanumeric characters".to_string(),
        );
    }
    Ok(())
}

/// The only Spotify redirect URI this app registers with the Spotify
/// dashboard. Single source of truth for the IPC allowlist — the frontend
/// (Onboarding/Reconnect) and `config::default_redirect_uri` must send
/// exactly this value. See issue #349.
const SPOTIFY_REDIRECT_URI: &str = "presencejam://callback";

/// Validates the Spotify `redirect_uri` at the IPC boundary (issue #349).
/// Pins the value to `SPOTIFY_REDIRECT_URI` (scheme `presencejam` + fixed
/// path); anything else is rejected before it reaches the authorize URL or
/// pending-auth storage. The error is user-safe: it names only the expected
/// value and never echoes caller input or secrets.
fn validate_spotify_redirect_uri(uri: &str) -> Result<(), String> {
    if uri == SPOTIFY_REDIRECT_URI {
        return Ok(());
    }
    log::error!(
        "{CMD} validate_spotify_redirect_uri: rejected redirect_uri len={}",
        uri.len()
    );
    Err(format!(
        "Invalid redirect_uri: must be exactly '{}'",
        SPOTIFY_REDIRECT_URI
    ))
}

/// Outcome of validating a manual-code paste (issue #351). The peek helper
/// below covers expiry + `state` only and never touches the single-use
/// launch binding, so it is pure and unit-testable without Tauri state;
/// the binding step runs later in the handler, after the peek guard is
/// dropped, so a wrong-state paste never burns the single-use slot.
#[derive(Debug, PartialEq, Eq)]
enum ManualPasteOutcome {
    /// Peek passed; the caller may run the binding check and then `take()`.
    Accept,
    /// Pending expired.
    Expired,
    /// `state` missing or mismatched.
    StateMismatch,
}

/// Pure peek validation for a manual-code paste: expiry, then `state`
/// equality (issue #351). Never consumes the launch binding — the caller
/// runs `validate_and_consume` only on `Accept`, after dropping the peek
/// guard, so a wrong-state paste leaves both the pending and the binding
/// slot intact for the retry.
fn decide_manual_paste_peek(
    pending: &PendingSpotifyAuth,
    oauth_state: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> ManualPasteOutcome {
    if pending.expires_at < now {
        return ManualPasteOutcome::Expired;
    }
    if oauth_state.is_empty() || !crate::pkce::ct_eq(oauth_state, &pending.state) {
        return ManualPasteOutcome::StateMismatch;
    }
    ManualPasteOutcome::Accept
}

/// Common PKCE OAuth flow for Spotify authorization. Builds the auth
/// URL, generates verifier/challenge/state, stores the pending auth
/// in AppState (in-memory only — never persisted), and opens the
/// browser. Does NOT touch the keychain — that's the caller's job.
///
/// Used by both `start_spotify_auth` (initial onboarding, writes
/// secret to keychain) and `start_spotify_reconnect` (keychain
/// already populated, reads from it).
fn run_spotify_oauth_flow(
    client_id: String,
    redirect_uri: String,
    state: &tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let verifier = crate::pkce::generate_verifier();
    log::info!(
        "{CMD} run_spotify_oauth_flow: verifier generated, len={}",
        verifier.len()
    );

    let challenge = crate::pkce::generate_challenge(&verifier);
    log::info!("{CMD} run_spotify_oauth_flow: challenge generated");

    let csrf_state = crate::pkce::generate_verifier();
    // #66 + scope-3.3 §C1: bind per-launch secret into state as
    // `<csrf>.<launch_secret>`. Spotify echoes `state` verbatim, so the
    // callback can validate the secret without extra storage. Additionally the
    // SHA-256 of this flow's PKCE verifier is bound into the launch binding and
    // validated + consumed (single-use) at callback time, so a replayed
    // callback fails closed (RFC 6749 §10.12). On macOS the scheme stays
    // `presencejam://` — hijack still possible but code is useless without
    // secret+verifier.
    let binding = state
        .launch_binding
        .get_or_init(|| crate::pkce::LaunchBinding::new(crate::pkce::generate_launch_secret()));
    binding.bind_verifier(&verifier);
    let csrf_state = format!("{}.{}", csrf_state, binding.launch_secret.clone());
    log::info!(
        "{CMD} run_spotify_oauth_flow: state generated, len={} [REDACTED]",
        csrf_state.len()
    );

    // `show_dialog=true` forces the consent screen even for users who have
    // previously approved this app — with `show_dialog=false` (the default)
    // Spotify can auto-redirect a prior approver without showing any screen,
    // which silently skips the consent step needed to grant the new
    // `user-modify-playback-state` scope (issue #3.0-P3).
    let auth_url = format!(
        "https://accounts.spotify.com/authorize\
         ?client_id={}\
         &response_type=code\
         &redirect_uri={}\
         &code_challenge_method=S256\
         &code_challenge={}\
         &state={}\
         &scope={}\
         &show_dialog=true",
        client_id,
        urlencoding::encode(&redirect_uri),
        urlencoding::encode(&challenge),
        urlencoding::encode(&csrf_state),
        urlencoding::encode(SPOTIFY_SCOPES)
    );
    log::info!(
        "{CMD} run_spotify_oauth_flow: auth_url created, length={}",
        auth_url.len()
    );

    // Spotify authorization codes expire in 10 minutes (600 seconds)
    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(600);

    // Store pending auth in AppState only. We deliberately do NOT persist
    // the PKCE verifier to disk — it's a 10-minute bearer credential and
    // disk persistence leaks it to filesystem-level attackers. See issue
    // #65 / HIGH #3.
    {
        let mut pending = state.pending.spotify_mut();
        *pending = Some(PendingSpotifyAuth {
            verifier,
            state: csrf_state,
            client_id,
            redirect_uri,
            expires_at,
        });
        log::info!(
            "{CMD} run_spotify_oauth_flow: stored pending auth in AppState (in-memory only)"
        );
    }

    if let Err(e) = tauri_plugin_opener::open_url(&auth_url, None::<&str>) {
        log::warn!(
            "{CMD} run_spotify_oauth_flow: Failed to open browser: {}",
            e
        );
    } else {
        log::info!("{CMD} run_spotify_oauth_flow: Browser opened successfully");
    }

    Ok(())
}

#[tauri::command]
pub async fn start_spotify_auth(
    window: tauri::Window,
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    _app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    // Issue #241: detached windows never legitimately start the OAuth flow.
    super::require_main_window(&window)?;
    log::info!(
        "{CMD} start_spotify_auth: ENTRY - client_id.len={}, redirect_uri={}",
        client_id.len(),
        redirect_uri
    );

    if client_id.is_empty() || client_secret.is_empty() {
        log::error!("{CMD} start_spotify_auth: client_id or client_secret is empty");
        return Err("client_id and client_secret are required".to_string());
    }

    // Issue #67: validate the inputs at the IPC boundary, not just in
    // the frontend. The frontend regex was a UX nicety, not a security
    // boundary — a devtools-pasted invoke() with arbitrary strings was
    // accepted before this check.
    validate_spotify_client_id(&client_id)?;
    validate_spotify_client_secret(&client_secret)?;
    // Issue #349: pin redirect_uri at the IPC boundary — a devtools caller
    // can pass any string, so reject anything but presencejam://callback
    // before it reaches the authorize URL or pending-auth storage.
    validate_spotify_redirect_uri(&redirect_uri)?;

    // #215: keychain I/O is blocking (OS keychain + file). Offload to
    // the blocking pool so the async runtime stays responsive, matching
    // the precedent in `commands/onboarding.rs::is_onboarding_complete`.
    let secret_clone = client_secret.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::keychain::store_spotify_client_secret(&secret_clone)
    })
    .await
    .map_err(|e| format!("start_spotify_auth keychain task failed: {}", e))??;
    log::info!("{CMD} start_spotify_auth: client_secret stored in keychain");

    run_spotify_oauth_flow(client_id, redirect_uri, &state)?;

    log::info!("{CMD} start_spotify_auth: SUCCESS - Spotify auth started");
    Ok(())
}

/// Reconnect Spotify by reading the existing `client_secret` from the
/// OS keychain (set during Onboarding). The frontend already verifies
/// the keychain entry via `is_spotify_client_secret_set` before calling
/// this; if it's missing the user is redirected to Onboarding instead.
///
/// Replaces the previous pattern of calling `start_spotify_auth` with
/// `clientSecret: ''`, which the #67 validator correctly rejected
/// (and which would have overwritten the existing keychain entry with
/// an empty string). See issues #9, #67, and the v2.6.4 verifier report.
#[tauri::command]
pub fn start_spotify_reconnect(
    window: tauri::Window,
    client_id: String,
    redirect_uri: String,
    _app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    // Issue #241: detached windows never legitimately start the OAuth flow.
    super::require_main_window(&window)?;
    log::info!(
        "{CMD} start_spotify_reconnect: ENTRY - client_id.len={}, redirect_uri={}",
        client_id.len(),
        redirect_uri
    );

    if client_id.is_empty() {
        log::error!("{CMD} start_spotify_reconnect: client_id is empty");
        return Err("client_id is required".to_string());
    }

    // Read the existing secret from the keychain. This will return an
    // error if the entry is missing (e.g., user cleared the keychain
    // after Onboarding), in which case the frontend should redirect to
    // Onboarding rather than retry. The `_` prefix tells the compiler
    // we intentionally discard the value here — its presence (and the
    // `?` above) proves the keychain entry exists.
    let _client_secret = crate::keychain::get_spotify_client_secret()?;
    log::info!("{CMD} start_spotify_reconnect: client_secret loaded from keychain");

    // #67 validation: client_id format only — we never validate the
    // secret here because it's already in the keychain (validated at
    // Onboarding time).
    validate_spotify_client_id(&client_id)?;
    // Issue #349: same IPC-boundary pin as start_spotify_auth.
    validate_spotify_redirect_uri(&redirect_uri)?;

    run_spotify_oauth_flow(client_id, redirect_uri, &state)?;

    log::info!("{CMD} start_spotify_reconnect: SUCCESS - Spotify reconnect started");
    Ok(())
}

#[tauri::command]
pub async fn complete_spotify_auth_manual(
    window: tauri::Window,
    code: String,
    oauth_state: String,
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    // Issue #241: detached windows never legitimately complete the OAuth flow.
    super::require_main_window(&window)?;
    log::info!(
        "{CMD} complete_spotify_auth_manual: ENTRY - code.len={}, oauth_state.len={} [REDACTED]",
        code.len(),
        oauth_state.len()
    );

    // Issue #351: peek-then-validate-then-take, mirroring the deep-link peek
    // at lib.rs:559-563. Phase 1 peeks under a READ guard (expiry + state
    // only — never the single-use binding, which must not burn on a
    // wrong-state paste). Phase 2 runs `validate_and_consume` on a clone
    // after the guard is dropped. Phase 3 takes the pending and verifies
    // `taken.state == peeked.state`, restoring on mismatch so a concurrent
    // flow's pending is never stolen. A wrong-state paste therefore leaves
    // both the pending and the binding slot intact for the correct retry.
    // Phase 1 — peek under the read guard.
    let peeked = {
        let guard = state.pending.spotify();
        let peeked = guard.as_ref().ok_or_else(|| {
            log::error!("{CMD} complete_spotify_auth_manual: No pending Spotify auth");
            "No pending Spotify auth. Please start auth again.".to_string()
        })?;
        log::info!(
            "{CMD} complete_spotify_auth_manual: pending auth found - verifier.len={}",
            peeked.verifier.len()
        );
        // Re-check expiry at submit time, mirroring handle_spotify_callback in
        // lib.rs (issue #162). Spotify authorization codes expire 10 minutes
        // after creation; a stale pending must not be consumable later.
        // #228: never log raw state values — compare lengths/prefixes only.
        match decide_manual_paste_peek(peeked, &oauth_state, chrono::Utc::now()) {
            ManualPasteOutcome::Accept => {
                log::info!("{CMD} complete_spotify_auth_manual: state verified successfully");
                peeked.clone()
            }
            ManualPasteOutcome::Expired => {
                log::error!(
                    "{CMD} complete_spotify_auth_manual: auth state expired at submit time"
                );
                return Err("Auth state expired — please try signing in again.".to_string());
            }
            ManualPasteOutcome::StateMismatch if oauth_state.is_empty() => {
                log::error!("{CMD} complete_spotify_auth_manual: missing state parameter");
                return Err("Missing state parameter - possible CSRF attack".to_string());
            }
            ManualPasteOutcome::StateMismatch => {
                log::error!(
                    "{CMD} complete_spotify_auth_manual: state mismatch - CSRF attack detected [REDACTED len {} vs {}]",
                    oauth_state.len(),
                    peeked.state.len()
                );
                return Err("State mismatch - possible CSRF attack".to_string());
            }
        }
    };
    // Phase 2 — launch binding on the clone, guard already dropped.
    // scope-3.3 §C1: same binding as the deep-link path (constant-time
    // secret-component compare, PKCE verifier-hash linkage, single-use
    // consumption so a replayed state/code pair fails closed, RFC 6749
    // §10.12). Runs only after the state check passed, so a wrong-state
    // paste never reaches — and never burns — the single-use slot.
    {
        let app_state = state.inner();
        match app_state.launch_binding.get() {
            Some(binding) => {
                let secret_component = oauth_state.rsplit('.').next().unwrap_or("");
                if let Err(reason) =
                    binding.validate_and_consume(secret_component, &peeked.verifier)
                {
                    log::error!(
                        "{CMD} complete_spotify_auth_manual: launch binding rejected ({}) [REDACTED]",
                        reason
                    );
                    return Err(
                        "State binding validation failed - possible CSRF or replay attack"
                            .to_string(),
                    );
                }
            }
            None => {
                log::error!("{CMD} complete_spotify_auth_manual: no launch binding in AppState");
                return Err("No pending auth binding - please start auth again.".to_string());
            }
        }
    }
    // Phase 3 — take + verify: consume the pending and confirm it is still
    // the flow we validated. On mismatch (a concurrent flow replaced it),
    // restore what we took and fail closed — never steal another flow's
    // pending (#351).
    let pending = {
        let mut guard = state.pending.spotify_mut();
        match guard.take() {
            Some(taken) if taken.state == peeked.state => taken,
            Some(taken) => {
                *guard = Some(taken);
                log::error!(
                    "{CMD} complete_spotify_auth_manual: pending changed during validation"
                );
                return Err("No pending Spotify auth. Please start auth again.".to_string());
            }
            None => {
                log::error!("{CMD} complete_spotify_auth_manual: pending consumed concurrently");
                return Err("No pending Spotify auth. Please start auth again.".to_string());
            }
        }
    };
    log::info!("{CMD} complete_spotify_auth_manual: checks passed, consuming pending auth");

    // #215: HTTPS token exchange + keychain I/O are blocking (reqwest::blocking
    // + OS keychain). Offload to the blocking pool so the async runtime stays
    // responsive. Clone owned values into the closure; AppState mutation stays
    // on the async thread after the join.
    let pending_clone = pending.clone();
    let code_clone = code.clone();
    let tokens = tauri::async_runtime::spawn_blocking(move || {
        let client_secret = crate::keychain::get_spotify_client_secret()?;
        crate::spotify::complete_spotify_auth(
            &code_clone,
            &pending_clone.verifier,
            &pending_clone.client_id,
            &client_secret,
            &pending_clone.redirect_uri,
        )
    })
    .await
    .map_err(|e| format!("complete_spotify_auth_manual task failed: {}", e))??;
    log::info!("{CMD} complete_spotify_auth_manual: token exchange successful");

    {
        let mut tokens_guard = state.tokens.spotify_mut();
        *tokens_guard = Some(tokens);
        log::info!("{CMD} complete_spotify_auth_manual: tokens stored in AppState");
    }
    token_io::persist_tokens(state.inner(), &app)?;
    log::info!("{CMD} complete_spotify_auth_manual: tokens persisted atomically");

    // Issue #70: invalidate the onboarding cache.
    state.onboarding_cache.invalidate();
    log::info!("{CMD} complete_spotify_auth_manual: onboarding_cache invalidated");

    log::info!("{CMD} complete_spotify_auth_manual: EMIT spotify-auth-complete event");
    let _ = app.emit("spotify-auth-complete", ());

    log::info!("{CMD} complete_spotify_auth_manual: SUCCESS (manual fallback)");
    Ok(())
}

// See issue #16: the cache-first, store-fallback pattern used to live in
// `crate::token_cache::get_cached_or_load`. Both `get_spotify_tokens` and
// `get_teams_tokens` have been removed — see issue #65. The webview no
// longer has a path to read tokens.

#[tauri::command]
pub fn refresh_spotify(
    window: tauri::Window,
    state: tauri::State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<(), String> {
    // Issue #241: token refresh touches keychain + persisted tokens; the
    // polling loop uses `spotify::refresh_spotify_token` directly and the
    // frontend never invokes this from a detached window.
    super::require_main_window(&window)?;
    log::debug!("{CMD} refresh_spotify: ENTRY");

    // Spotify client_id lives in the config (it's not a secret). The
    // client_secret is in the OS keychain (see issue #9). The previous
    // implementation read client_id from a persistent store — that path
    // is removed as part of issue #65.
    let client_id = {
        let guard = state.config.get();
        guard
            .as_ref()
            .map(|c| c.spotify.client_id.clone())
            .filter(|id| !id.is_empty())
            .ok_or_else(|| {
                log::error!("{CMD} refresh_spotify: Spotify client ID not found in config");
                "Spotify client ID not found".to_string()
            })?
    };
    let client_secret = crate::keychain::get_spotify_client_secret()?;
    log::info!("{CMD} refresh_spotify: credentials loaded (id from config, secret from keychain)");

    let current_tokens = {
        let guard = state.tokens.spotify();
        guard.clone().ok_or_else(|| {
            log::error!("{CMD} refresh_spotify: No Spotify tokens in state");
            "No Spotify tokens to refresh".to_string()
        })?
    };
    log::info!("{CMD} refresh_spotify: current tokens found");

    // `refresh_spotify_token` now returns a typed `SpotifyApiError`
    // (issue #160); stringify it for the IPC boundary, preserving this
    // command's public `Result<(), String>` contract.
    let new_tokens =
        crate::spotify::refresh_spotify_token(&current_tokens, &client_id, &client_secret)
            .map_err(|e| e.to_string())?;
    log::info!("{CMD} refresh_spotify: new tokens received");

    // CAS: only commit if state still holds the access token we refreshed
    // from. If state changed during the refresh (e.g. user clicked
    // Reconnect from another command), discard the result.
    let pre_refresh_access_token = current_tokens.access_token.clone();
    let committed = {
        let mut guard = state.tokens.spotify_mut();
        if guard.as_ref().map(|t| &t.access_token) == Some(&pre_refresh_access_token) {
            *guard = Some(new_tokens.clone());
            true
        } else {
            log::warn!("{CMD} refresh_spotify: state changed during refresh, discarding result");
            false
        }
    };
    if committed {
        token_io::persist_tokens(state.inner(), &app)?;
        log::info!("{CMD} refresh_spotify: SUCCESS (state updated and persisted)");
    } else {
        log::info!("{CMD} refresh_spotify: NOOP (concurrent state change; not persisted)");
    }

    Ok(())
}

/// Returns true iff the Spotify `client_secret` is currently stored in the
/// OS keychain. The frontend uses this to decide whether the user can
/// reconnect (keychain populated) or must re-enter the secret via
/// Onboarding (keychain empty). See issue #9.
#[tauri::command]
pub fn is_spotify_client_secret_set() -> bool {
    crate::keychain::has_spotify_client_secret()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Issue #354: 32+ char non-alphanumeric secrets are rejected, >512 is
    // rejected, and a 32-char alphanumeric secret passes.
    #[test]
    fn secret_validator_rejects_non_alphanumeric_and_overlong() {
        assert!(
            validate_spotify_client_secret(&"a".repeat(32)).is_ok(),
            "32-char alphanumeric must pass"
        );
        let punctuated = format!("{}!", "a".repeat(31));
        assert_eq!(punctuated.len(), 32);
        assert!(
            validate_spotify_client_secret(&punctuated).is_err(),
            "32-char secret with non-alphanumeric must fail"
        );
        assert!(
            validate_spotify_client_secret(&"a".repeat(513)).is_err(),
            ">512 chars must fail"
        );
        assert!(
            validate_spotify_client_secret(&"a".repeat(512)).is_ok(),
            "512-char alphanumeric must pass"
        );
        assert!(
            validate_spotify_client_secret("short").is_err(),
            "short secret must fail"
        );
    }

    // Issue #446: valid 32-char alphanumeric client_ids pass; empty,
    // short/overlong, and illegal-char inputs are rejected. Mirrors
    // `secret_validator_rejects_non_alphanumeric_and_overlong` above.
    #[test]
    fn client_id_validator_accepts_valid_shape_and_rejects_bad() {
        assert!(
            validate_spotify_client_id(&"a".repeat(32)).is_ok(),
            "32-char alphanumeric must pass"
        );
        assert!(
            validate_spotify_client_id("").is_err(),
            "empty client_id must fail"
        );
        assert!(
            validate_spotify_client_id(&"a".repeat(31)).is_err(),
            "31 chars must fail"
        );
        assert!(
            validate_spotify_client_id(&"a".repeat(33)).is_err(),
            "33 chars must fail"
        );
        let punctuated = format!("{}!", "a".repeat(31));
        assert_eq!(punctuated.len(), 32);
        assert!(
            validate_spotify_client_id(&punctuated).is_err(),
            "32-char id with non-alphanumeric must fail"
        );
    }

    // Issue #351: peek-then-validate-then-take. The peek helper never touches
    // the single-use binding, so a wrong-state paste keeps both the pending
    // and the binding slot; a correct paste then still accepts. The handler
    // below mirrors the deep-link peek at lib.rs:559-563 (read guard for
    // peek, binding after the guard drops, take + state-verify last).
    fn sample_pending(state: &str) -> PendingSpotifyAuth {
        PendingSpotifyAuth {
            verifier: crate::pkce::generate_verifier(),
            state: state.to_string(),
            client_id: "cid".to_string(),
            redirect_uri: "presencejam://callback".to_string(),
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(600),
        }
    }

    #[test]
    fn peek_wrong_state_then_correct_state_succeeds() {
        let verifier = crate::pkce::generate_verifier();
        let binding = crate::pkce::LaunchBinding::new(crate::pkce::generate_launch_secret());
        binding.bind_verifier(&verifier);
        // The state must carry THIS binding's launch secret as its
        // `<csrf>.<secret>` tail — otherwise `validate_and_consume` rejects
        // even the correct paste (the secret would belong to no binding).
        let state = format!("csrf.{}", binding.launch_secret);
        let pending = PendingSpotifyAuth {
            verifier,
            state,
            client_id: "cid".to_string(),
            redirect_uri: "presencejam://callback".to_string(),
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(600),
        };
        let now = chrono::Utc::now();
        // Wrong-state paste: peek rejects WITHOUT consuming the binding slot.
        assert_eq!(
            decide_manual_paste_peek(&pending, "wrong-state", now),
            ManualPasteOutcome::StateMismatch
        );
        // Correct paste still passes the peek, then consumes the binding once.
        assert_eq!(
            decide_manual_paste_peek(&pending, &pending.state.clone(), now),
            ManualPasteOutcome::Accept
        );
        let secret_component = pending.state.rsplit('.').next().unwrap_or("");
        assert!(binding
            .validate_and_consume(secret_component, &pending.verifier)
            .is_ok());
    }

    #[test]
    fn peek_rejects_expired_and_empty_state() {
        let mut pending = sample_pending("csrf-state.secret-part");
        let now = chrono::Utc::now();
        assert_eq!(
            decide_manual_paste_peek(&pending, &pending.state.clone(), now),
            ManualPasteOutcome::Accept
        );
        assert_eq!(
            decide_manual_paste_peek(&pending, "", now),
            ManualPasteOutcome::StateMismatch
        );
        pending.expires_at = now - chrono::Duration::seconds(1);
        assert_eq!(
            decide_manual_paste_peek(&pending, &pending.state.clone(), now),
            ManualPasteOutcome::Expired
        );
    }

    // Structural source guard: the handler must peek under a READ guard,
    // run the single-use binding only after the peek, and take() only last
    // with a state re-verify — so no future refactor can reintroduce
    // take-first or burn the binding slot on a wrong-state paste. The body
    // is isolated with the shared literal-aware scanner
    // (`crate::token_io::test_scan`), not a next-function boundary anchor:
    // a naive `{`/`}` byte counter breaks on braces inside string literals
    // (`"{CMD} … {} …"`), so strings, char literals and comments are
    // skipped. This scan is a deliberate, load-bearing proxy for handler
    // ordering that cannot be driven in a unit test without Tauri state —
    // but the guarded behavior itself is covered behaviorally by
    // `peek_wrong_state_then_correct_state_succeeds` (peek-then-bind order
    // through the real helper and binding).
    #[test]
    fn manual_path_takes_pending_only_after_validation() {
        let src = include_str!("spotify_auth.rs");
        let body = crate::token_io::test_scan::fn_body(src, "fn complete_spotify_auth_manual(");
        let peek_pos = body
            .find("state.pending.spotify()")
            .expect("body must peek under the read guard");
        let mismatch_pos = body
            .find("State mismatch - possible CSRF attack")
            .expect("body must keep the state-mismatch rejection");
        // NOTE: anchors must be code-specific call shapes, not bare
        // identifiers — the handler's own header comment names
        // `validate_and_consume` and `taken.state == peeked.state` in
        // backticks, which a bare `find` would hit before the real code.
        let bind_pos = body
            .find("validate_and_consume(secret_component")
            .expect("body must run the launch binding");
        let take_pos = body
            .find("guard.take()")
            .expect("body must take the pending");
        let verify_pos = body
            .find("Some(taken) if taken.state == peeked.state")
            .expect("body must re-verify the taken pending against the peek");
        assert!(
            peek_pos < mismatch_pos,
            "read-guard peek must come before the state-mismatch rejection"
        );
        assert!(
            mismatch_pos < bind_pos,
            "binding consume must come after the state check so a wrong-state paste never burns the slot"
        );
        assert!(
            bind_pos < take_pos && take_pos < verify_pos,
            "take() must come after the binding check and the taken pending must be verified against the peek"
        );
        assert!(
            body.find("decide_manual_paste_peek").is_some(),
            "handler must validate through the peek helper"
        );
    }
}
