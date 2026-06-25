//! Onboarding Tauri commands: completion check, transition, and disconnect/reconnect.
//!
//! See issue #76. Owns `is_onboarding_complete_impl` (the spawn_blocking body
//! for the async cache-first check) and the `ONBOARDING_CACHE_TTL` constant.

use crate::config;
use crate::token_io;
use crate::AppState;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// TTL for the `is_onboarding_complete` result cache. The front-end remounts this
/// command on every Onboarding view enter, and the upstream HTTPS calls can take
/// up to 20s in the worst case (token validation against Spotify/Graph APIs), so
/// a short cache is needed to avoid hammering the upstream APIs.
const ONBOARDING_CACHE_TTL: Duration = Duration::from_secs(30);

/// Log tag prefix for this submodule (issue #79 item 3).
const CMD: &str = "[CMD.ONBOARDING]";

/// Onboarding check: `true` if both Spotify and Teams are configured and have a non-expired
/// token. Network errors (5xx, 429) are treated as "still valid" (transient) so a flaky
/// network doesn't bounce the user back into the onboarding flow.
///
/// Result is cached on `AppState.onboarding_cache` for [`ONBOARDING_CACHE_TTL`] —
/// the front-end remounts this command on every Onboarding view enter, and the
/// upstream HTTPS calls can take up to 20s in the worst case (token validation
/// against Spotify/Graph APIs).
#[tauri::command]
pub async fn is_onboarding_complete(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<bool, String> {
    log::debug!("{CMD} is_onboarding_complete: ENTRY");

    // Cache hit — return immediately.
    {
        let guard = state.onboarding_cache.lock();
        if let Some((ts, result)) = *guard {
            if ts.elapsed() < ONBOARDING_CACHE_TTL {
                log::info!(
                    "{CMD} is_onboarding_complete: cache HIT (age={:.2}s, result={})",
                    ts.elapsed().as_secs_f32(),
                    result
                );
                return Ok(result);
            }
        }
    }

    // Cache miss — run the actual validation on a blocking thread (HTTPS round-trips).
    let state_clone: Arc<AppState> = Arc::clone(&state);
    let result =
        tauri::async_runtime::spawn_blocking(move || is_onboarding_complete_impl(&state_clone))
            .await
            .map_err(|e| format!("is_onboarding_complete task panicked: {}", e))??;

    // Store result in cache. We cache both `true` and `false` outcomes — a recent "complete"
    // result is just as valid as a recent "incomplete" one for the 30s window.
    *state.onboarding_cache.lock() = Some((Instant::now(), result));
    log::info!(
        "{CMD} is_onboarding_complete: cache MISS, stored fresh result={}",
        result
    );
    Ok(result)
}

/// Blocking implementation of the onboarding check. Run via `spawn_blocking` from
/// `is_onboarding_complete` so the async runtime can keep serving other commands while
/// the HTTPS round-trips to Spotify/Graph complete.
fn is_onboarding_complete_impl(state: &Arc<AppState>) -> Result<bool, String> {
    let config = config::load_config()?;
    let spotify_configured = !config.spotify.client_id.is_empty();

    // Check Teams tokens — only ExpiredToken (401/403) means invalid.
    // RateLimited (429) and Transient (5xx, network) are temporary → treat as valid.
    let (teams_configured, teams_valid) = {
        let guard = state.tokens.teams();
        match guard.as_ref() {
            Some(tokens) => {
                let valid = match crate::teams::validate_teams_token(tokens) {
                    Ok(()) => true,
                    Err(crate::teams::TeamsApiError::ExpiredToken(_)) => false,
                    Err(_) => true, // transient — still valid for onboarding
                };
                (true, valid)
            }
            None => (false, false),
        }
    };

    // Check Spotify tokens — only ExpiredToken means invalid.
    // RateLimited and Other are transient → treat as valid.
    let (spotify_valid, _spotify_token) = {
        let guard = state.tokens.spotify();
        match guard.as_ref() {
            Some(tokens) => {
                let valid = match crate::spotify::validate_spotify_token(tokens) {
                    Ok(()) => true,
                    Err(crate::spotify::SpotifyApiError::ExpiredToken) => false,
                    Err(_) => true, // transient — still valid for onboarding
                };
                (valid, Some(tokens.clone()))
            }
            None => (false, None),
        }
    };

    // Onboarding is complete only if:
    // 1. Spotify is configured AND token is not permanently expired
    // 2. Teams is configured AND token is not permanently expired
    let complete = spotify_configured && spotify_valid && teams_configured && teams_valid;
    log::info!(
        "{CMD} is_onboarding_complete: result={} (spotify_configured={}, spotify_valid={}, teams_configured={}, teams_valid={})",
        complete,
        spotify_configured,
        spotify_valid,
        teams_configured,
        teams_valid
    );

    Ok(complete)
}

#[tauri::command]
pub fn complete_onboarding(
    state: tauri::State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<(), String> {
    log::debug!("{CMD} complete_onboarding: ENTRY");

    let has_spotify = {
        let guard = state.tokens.spotify();
        guard.is_some()
    };

    let has_teams = {
        let guard = state.tokens.teams();
        guard.is_some()
    };

    log::info!(
        "{CMD} complete_onboarding: has_spotify={}, has_teams={}",
        has_spotify,
        has_teams
    );

    if has_spotify && has_teams {
        log::info!("{CMD} complete_onboarding: both tokens present, starting sync");
        super::sync::start_syncing(state, app)?;
        log::info!("{CMD} complete_onboarding: sync started successfully");
    } else {
        log::error!(
            "{CMD} complete_onboarding: missing tokens, cannot start sync (spotify={}, teams={})",
            has_spotify,
            has_teams
        );
        return Err(format!(
            "Missing tokens: spotify={}, teams={}",
            has_spotify, has_teams
        ));
    }

    log::info!("{CMD} complete_onboarding: SUCCESS");
    Ok(())
}

#[tauri::command]
pub fn reconnect_spotify(
    state: tauri::State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<(), String> {
    log::debug!("{CMD} reconnect_spotify: ENTRY");

    // Clear Spotify tokens from state
    *state.tokens.spotify_mut() = None;
    log::info!("{CMD} reconnect_spotify: cleared spotify_tokens");

    // Clear pending Spotify auth
    *state.pending.spotify_mut() = None;
    log::info!("{CMD} reconnect_spotify: cleared pending_spotify_auth");

    // Persist the cleared state to disk atomically.
    if let Err(e) = token_io::persist_tokens(state.inner(), &app) {
        log::warn!(
            "{CMD} reconnect_spotify: failed to persist cleared state - {}",
            e
        );
    }

    // Issue #70: invalidate the onboarding cache so the UI sees the cleared state.
    state.onboarding_cache.invalidate();
    log::info!("{CMD} reconnect_spotify: onboarding_cache invalidated");

    // Clear the client_secret from the OS keychain (see issue #9).
    // Best-effort: don't fail the disconnect if the keychain entry is
    // already gone or unavailable.
    if let Err(e) = crate::keychain::delete_spotify_client_secret() {
        log::warn!(
            "{CMD} reconnect_spotify: failed to clear keychain entry - {}",
            e
        );
    }

    // Emit event so UI can show re-auth flow
    if let Err(e) = app.emit("spotify-reconnect-required", ()) {
        log::error!("{CMD} reconnect_spotify: failed to emit event - {}", e);
    } else {
        log::info!("{CMD} reconnect_spotify: EMIT spotify-reconnect-required event");
    }

    log::info!("{CMD} reconnect_spotify: SUCCESS");
    Ok(())
}

#[tauri::command]
pub fn reconnect_teams(
    state: tauri::State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<(), String> {
    log::debug!("{CMD} reconnect_teams: ENTRY");

    // Clear Teams tokens from state
    *state.tokens.teams_mut() = None;
    log::info!("{CMD} reconnect_teams: cleared teams_tokens");

    // Clear pending Teams auth
    *state.pending.teams_mut() = None;
    log::info!("{CMD} reconnect_teams: cleared pending_teams_auth");

    // Persist the cleared state to disk atomically.
    if let Err(e) = token_io::persist_tokens(state.inner(), &app) {
        log::warn!(
            "{CMD} reconnect_teams: failed to persist cleared state - {}",
            e
        );
    }

    // Issue #70: invalidate the onboarding cache.
    state.onboarding_cache.invalidate();
    log::info!("{CMD} reconnect_teams: onboarding_cache invalidated");

    // Emit event so UI can show re-auth flow
    if let Err(e) = app.emit("teams-reconnect-required", ()) {
        log::error!("{CMD} reconnect_teams: failed to emit event - {}", e);
    } else {
        log::info!("{CMD} reconnect_teams: EMIT teams-reconnect-required event");
    }

    log::info!("{CMD} reconnect_teams: SUCCESS");
    Ok(())
}