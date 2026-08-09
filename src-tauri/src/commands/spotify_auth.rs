//! Spotify authentication Tauri commands.
//!
//! See issue #76. This module owns the PKCE OAuth flow for both initial
//! Onboarding (`start_spotify_auth`) and Reconnect (`start_spotify_reconnect`),
//! plus the manual-code fallback (`complete_spotify_auth_manual`) and the
//! in-flight token refresher (`refresh_spotify`).

use crate::spotify::SpotifyTokens;
use crate::token_io;
use crate::{AppState, PendingSpotifyAuth};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};


/// Log tag prefix for this submodule (issue #79 item 3).
const CMD: &str = "[CMD.SPOTIFY_AUTH]";

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

/// Validates a Spotify client_secret (>= 32 chars, non-empty).
/// See issue #67.
fn validate_spotify_client_secret(secret: &str) -> Result<(), String> {
    if secret.len() < 32 {
        return Err(format!(
            "Invalid client_secret: must be at least 32 characters (got len={})",
            secret.len()
        ));
    }
    Ok(())
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
    log::info!(
        "{CMD} run_spotify_oauth_flow: state generated, len={}",
        csrf_state.len()
    );

    let auth_url = format!(
        "https://accounts.spotify.com/authorize\
         ?client_id={}\
         &response_type=code\
         &redirect_uri={}\
         &code_challenge_method=S256\
         &code_challenge={}\
         &state={}\
         &scope=user-read-currently-playing user-read-playback-state",
        client_id,
        urlencoding::encode(&redirect_uri),
        urlencoding::encode(&challenge),
        urlencoding::encode(&csrf_state)
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
pub fn start_spotify_auth(
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    _app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
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

    // Store the client_secret in the OS keychain. This is the only place the
    // secret is persisted from this point forward; it is intentionally NOT
    // included in `pending_spotify_auth` (AppState or store). See issue #9.
    crate::keychain::store_spotify_client_secret(&client_secret)?;
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
    client_id: String,
    redirect_uri: String,
    _app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
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

    run_spotify_oauth_flow(client_id, redirect_uri, &state)?;

    log::info!("{CMD} start_spotify_reconnect: SUCCESS - Spotify reconnect started");
    Ok(())
}

#[tauri::command]
pub fn complete_spotify_auth_manual(
    code: String,
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<SpotifyTokens, String> {
    log::info!(
        "{CMD} complete_spotify_auth_manual: ENTRY - code.len={}",
        code.len()
    );

    // Get pending auth from AppState
    let pending = {
        let mut guard = state.pending.spotify_mut();
        log::info!("{CMD} complete_spotify_auth_manual: taking pending auth from AppState");
        guard.take().ok_or_else(|| {
            log::error!("{CMD} complete_spotify_auth_manual: No pending Spotify auth");
            "No pending Spotify auth. Please start auth again.".to_string()
        })?
    };
    log::info!(
        "{CMD} complete_spotify_auth_manual: pending auth found - verifier.len={}",
        pending.verifier.len()
    );

    // Read the client_secret from the keychain (it was placed there by
    // `start_spotify_auth` — see issue #9).
    let client_secret = crate::keychain::get_spotify_client_secret()?;

    let tokens = crate::spotify::complete_spotify_auth(
        &code,
        &pending.verifier,
        &pending.client_id,
        &client_secret,
        &pending.redirect_uri,
    )?;
    log::info!("{CMD} complete_spotify_auth_manual: token exchange successful");

    {
        let mut tokens_guard = state.tokens.spotify_mut();
        *tokens_guard = Some(tokens.clone());
        log::info!("{CMD} complete_spotify_auth_manual: tokens stored in AppState");
    }
    token_io::persist_tokens(state.inner(), &app)?;
    log::info!("{CMD} complete_spotify_auth_manual: tokens persisted atomically");

    // Issue #70: invalidate the onboarding cache.
    state.onboarding_cache.invalidate();
    log::info!("{CMD} complete_spotify_auth_manual: onboarding_cache invalidated");

    log::info!("{CMD} complete_spotify_auth_manual: EMIT spotify-auth-complete event");
    let _ = app.emit("spotify-auth-complete", &tokens);

    log::info!("{CMD} complete_spotify_auth_manual: SUCCESS (manual fallback)");
    Ok(tokens)
}

// See issue #16: the cache-first, store-fallback pattern used to live in
// `crate::token_cache::get_cached_or_load`. Both `get_spotify_tokens` and
// `get_teams_tokens` have been removed — see issue #65. The webview no
// longer has a path to read tokens.

#[tauri::command]
pub fn refresh_spotify(
    state: tauri::State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<(), String> {
    log::debug!("{CMD} refresh_spotify: ENTRY");

    // Spotify client_id lives in the config (it's not a secret). The
    // client_secret is in the OS keychain (see issue #9). The previous
    // implementation read client_id from `tauri-plugin-store` — that
    // path is removed as part of issue #65.
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
    let new_tokens = crate::spotify::refresh_spotify_token(
        &current_tokens,
        &client_id,
        &client_secret,
    )
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
