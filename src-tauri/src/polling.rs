use chrono::Utc;
use std::sync::Arc;
use std::thread;
use std::time::Duration as StdDuration;
use tauri::{AppHandle, Emitter};
use tauri_plugin_store::StoreExt;

use crate::profanity;
use crate::spotify::{
    format_status, get_currently_playing, is_token_expired, refresh_spotify_token, SpotifyTokens,
};
use crate::teams::{clear_teams_status_message, set_teams_status_message, TeamsTokens};
use crate::AppState;

const DEFAULT_INTERVAL_SECONDS: u64 = 30;
const MAX_INTERVAL_SECONDS: u64 = 60;
const MINIMUM_INTERVAL_SECONDS: u64 = 10;
const ERROR_RETRY_INTERVAL_SECONDS: u64 = 30;

pub fn start_polling(
    state: Arc<AppState>,
    app: AppHandle,
) -> Result<thread::JoinHandle<()>, String> {
    log::info!("[POLLING] start_polling: ENTRY");

    *state.is_syncing.write() = true;
    log::info!("[POLLING] start_polling: is_syncing flag set to true");

    // Clone Arc for the thread
    let state_clone = Arc::clone(&state);
    let app_clone = app.clone();

    let handle = thread::Builder::new()
        .name("presencejam-polling".to_string())
        .stack_size(1024 * 1024) // 1MB stack for safety
        .spawn(move || {
            log::info!("[POLLING] start_polling: thread started");
            polling_loop(state_clone, app_clone);
            log::info!("[POLLING] start_polling: thread ended");
        })
        .map_err(|e| {
            log::error!("[POLLING] start_polling: thread spawn failed - {}", e);
            format!("Failed to spawn polling thread: {}", e)
        })?;

    log::info!("[POLLING] start_polling: SUCCESS - handle returned");
    Ok(handle)
}

fn polling_loop(state: Arc<AppState>, app: AppHandle) {
    log::info!("[POLLING] polling_loop: STARTED");
    let mut last_track_key: Option<String> = None;

    loop {
        log::info!("[POLLING] polling_loop: iteration start");

        // Check if we should stop
        {
            let is_syncing = *state.is_syncing.read();
            log::info!("[POLLING] polling_loop: is_syncing={}", is_syncing);
            if !is_syncing {
                log::info!("[POLLING] polling_loop: is_syncing=false, breaking loop");
                break;
            }
        }

        // Get config
        let config = {
            let guard = state.config.read();
            guard.clone()
        };
        log::info!("[POLLING] polling_loop: config loaded");

        // Get Spotify tokens
        let spotify_tokens = {
            let guard = state.spotify_tokens.read();
            guard.clone()
        };
        log::info!(
            "[POLLING] polling_loop: spotify_tokens: {}",
            if spotify_tokens.is_some() {
                "Some"
            } else {
                "None"
            }
        );

        let spotify_tokens = match spotify_tokens {
            Some(t) => {
                log::info!("[POLLING] polling_loop: using existing Spotify tokens");
                t
            }
            None => {
                log::warn!("[POLLING] polling_loop: No Spotify tokens available, waiting...");
                thread::sleep(StdDuration::from_secs(ERROR_RETRY_INTERVAL_SECONDS));
                continue;
            }
        };

        // Check if token expired
        let token_expired = is_token_expired(&spotify_tokens);
        log::info!("[POLLING] polling_loop: token_expired={}", token_expired);

        // Refresh token if needed
        let spotify_tokens = if token_expired {
            log::info!("[POLLING] polling_loop: Spotify token expired, refreshing...");
            let client_id = config
                .as_ref()
                .map(|c| c.spotify.client_id.as_str())
                .unwrap_or("");
            let client_secret = config
                .as_ref()
                .map(|c| c.spotify.client_secret.as_str())
                .unwrap_or("");
            log::info!(
                "[POLLING] polling_loop: refreshing with client_id.len={}",
                client_id.len()
            );

            match refresh_spotify_token(&spotify_tokens, client_id, client_secret) {
                Ok(new_tokens) => {
                    log::info!("[POLLING] polling_loop: token refresh SUCCESS");
                    *state.spotify_tokens.write() = Some(new_tokens.clone());
                    new_tokens
                }
                Err(e) => {
                    log::error!(
                        "[POLLING] polling_loop: Failed to refresh Spotify token: {}",
                        e
                    );
                    let _ = app.emit(
                        "error",
                        serde_json::json!({
                            "source": "spotify",
                            "message": format!("Token refresh failed: {}", e)
                        }),
                    );
                    log::info!("[POLLING] polling_loop: EMIT error event");
                    thread::sleep(StdDuration::from_secs(ERROR_RETRY_INTERVAL_SECONDS));
                    continue;
                }
            }
        } else {
            spotify_tokens
        };

        let access_token = spotify_tokens.access_token.clone();
        log::info!("[POLLING] polling_loop: calling get_currently_playing");

        // Get currently playing track (blocking call)
        let result = get_currently_playing(&access_token);

        match result {
            Ok(Some(track)) => {
                log::info!(
                    "[POLLING] polling_loop: track found - {} by {}",
                    track.title,
                    track.artist
                );
                let track_key = format!("{} - {}", track.title, track.artist);

                if last_track_key.as_ref() != Some(&track_key) {
                    log::info!("[POLLING] polling_loop: new track detected, updating");
                    last_track_key = Some(track_key.clone());
                    *state.current_track.write() = Some(track.clone());

                    log::info!("[POLLING] polling_loop: EMIT spotify-track-changed event");
                    let _ = app.emit(
                        "spotify-track-changed",
                        serde_json::json!({
                            "title": track.title,
                            "artist": track.artist,
                            "album": track.album,
                            "album_art_url": track.album_art_url,
                            "is_playing": track.is_playing,
                            "progress_ms": track.progress_ms,
                            "duration_ms": track.duration_ms
                        }),
                    );

                    let teams_tokens = {
                        let guard = state.teams_tokens.read();
                        guard.clone()
                    };
                    log::info!(
                        "[POLLING] polling_loop: teams_tokens: {}",
                        if teams_tokens.is_some() {
                            "Some"
                        } else {
                            "None"
                        }
                    );

                    if let Some(teams_tok) = teams_tokens {
                        if track.is_playing {
                            log::info!(
                                "[POLLING] polling_loop: track is playing, updating Teams status"
                            );
                            let status_format = config
                                .as_ref()
                                .map(|c| c.teams.status_format.as_str())
                                .unwrap_or("🎵 {artist} - {track} 🎧");
                            let status_message = format_status(&track, status_format);
                            log::info!("[POLLING] polling_loop: status_message={}", status_message);

                            let profanity_filter_enabled = config
                                .as_ref()
                                .map(|c| c.teams.profanity_filter)
                                .unwrap_or(true);
                            let placeholder = config
                                .as_ref()
                                .map(|c| c.teams.profanity_placeholder.as_str())
                                .unwrap_or("Currently Listening to Spotify");
                            let final_status = if profanity_filter_enabled {
                                profanity::filter_status(
                                    &status_message,
                                    placeholder,
                                    track.is_playing,
                                )
                            } else {
                                status_message.clone()
                            };
                            if final_status != status_message {
                                log::info!(
                                    "[POLLING] profanity filter: replaced status '{}' with placeholder '{}'",
                                    status_message,
                                    final_status
                                );
                            }

                            let remaining_ms = track.duration_ms.saturating_sub(track.progress_ms);
                            let buffer_ms = config
                                .as_ref()
                                .map(|c| c.polling.expiry_buffer_seconds as u64)
                                .unwrap_or(10)
                                * 1000;
                            let expiry = Utc::now()
                                + chrono::Duration::milliseconds(
                                    remaining_ms as i64 + buffer_ms as i64,
                                );
                            let expiry_str = expiry.to_rfc3339();
                            log::info!("[POLLING] polling_loop: expiry={}", expiry_str);

                            match set_teams_status_message(
                                &teams_tok.access_token,
                                &final_status,
                                Some(&expiry_str),
                            ) {
                                Ok(_) => {
                                    log::info!(
                                        "[POLLING] polling_loop: Teams status updated: {}",
                                        final_status
                                    );
                                    let _ = app.emit(
                                        "presence-updated",
                                        serde_json::json!({
                                            "status": final_status,
                                            "timestamp": Utc::now().to_rfc3339()
                                        }),
                                    );
                                    log::info!(
                                        "[POLLING] polling_loop: EMIT presence-updated event"
                                    );
                                }
                                Err(e) => {
                                    log::error!(
                                        "[POLLING] polling_loop: Failed to set Teams status: {}",
                                        e
                                    );
                                    let _ = app.emit(
                                        "error",
                                        serde_json::json!({
                                            "source": "teams",
                                            "message": format!("Failed to update status: {}", e)
                                        }),
                                    );
                                    log::info!("[POLLING] polling_loop: EMIT error event");
                                }
                            }
                        } else {
                            log::info!(
                                "[POLLING] polling_loop: track is paused, clearing Teams status"
                            );
                            match clear_teams_status_message(&teams_tok.access_token) {
                                Ok(_) => {
                                    log::info!("[POLLING] polling_loop: Teams status cleared");
                                    let _ = app.emit(
                                        "presence-cleared",
                                        serde_json::json!({
                                            "timestamp": Utc::now().to_rfc3339()
                                        }),
                                    );
                                    log::info!(
                                        "[POLLING] polling_loop: EMIT presence-cleared event"
                                    );
                                }
                                Err(e) => {
                                    log::error!(
                                        "[POLLING] polling_loop: Failed to clear Teams status: {}",
                                        e
                                    );
                                }
                            }
                        }
                    } else {
                        log::warn!(
                            "[POLLING] polling_loop: No Teams tokens, skipping status update"
                        );
                    }
                } else {
                    log::info!("[POLLING] polling_loop: same track, no update needed");
                }

                // Calculate sleep duration
                let sleep_duration = if track.is_playing {
                    let remaining_ms = track.duration_ms.saturating_sub(track.progress_ms);
                    let buffer_ms = 5000u64;
                    let remaining_secs = remaining_ms / 1000;
                    let sleep_secs = remaining_secs.saturating_sub(buffer_ms / 1000);
                    sleep_secs
                        .max(MINIMUM_INTERVAL_SECONDS)
                        .min(MAX_INTERVAL_SECONDS)
                } else {
                    DEFAULT_INTERVAL_SECONDS
                };
                log::info!(
                    "[POLLING] polling_loop: sleeping for {} seconds",
                    sleep_duration
                );
                thread::sleep(StdDuration::from_secs(sleep_duration));
            }
            Ok(None) => {
                log::info!("[POLLING] polling_loop: no track playing");

                if last_track_key.is_some() {
                    log::info!("[POLLING] polling_loop: was playing before, clearing state");
                    last_track_key = None;
                    *state.current_track.write() = None;

                    let teams_tokens = {
                        let guard = state.teams_tokens.read();
                        guard.clone()
                    };
                    if let Some(teams_tok) = teams_tokens {
                        log::info!("[POLLING] polling_loop: clearing Teams status");
                        match clear_teams_status_message(&teams_tok.access_token) {
                            Ok(_) => {
                                log::info!("[POLLING] polling_loop: EMIT presence-cleared event");
                                let _ = app.emit(
                                    "presence-cleared",
                                    serde_json::json!({
                                        "timestamp": Utc::now().to_rfc3339()
                                    }),
                                );
                            }
                            Err(e) => {
                                log::error!(
                                    "[POLLING] polling_loop: Failed to clear Teams status: {}",
                                    e
                                );
                            }
                        }
                    }
                }
                log::info!(
                    "[POLLING] polling_loop: sleeping for {} seconds (no track)",
                    DEFAULT_INTERVAL_SECONDS
                );
                thread::sleep(StdDuration::from_secs(DEFAULT_INTERVAL_SECONDS));
            }
            Err(e) => {
                log::error!(
                    "[POLLING] polling_loop: Failed to get currently playing track: {}",
                    e
                );
                let _ = app.emit(
                    "error",
                    serde_json::json!({
                        "source": "spotify",
                        "message": format!("Failed to get currently playing: {}", e)
                    }),
                );
                log::info!("[POLLING] polling_loop: EMIT error event");
                thread::sleep(StdDuration::from_secs(ERROR_RETRY_INTERVAL_SECONDS));
            }
        }
    }

    log::info!("[POLLING] polling_loop: ENDED");
}

pub fn stop_polling(state: &AppState) {
    log::info!("[POLLING] stop_polling: ENTRY");
    *state.is_syncing.write() = false;
    log::info!("[POLLING] stop_polling: is_syncing set to false");
}

pub fn save_spotify_tokens(app: &AppHandle, tokens: &SpotifyTokens) -> Result<(), String> {
    log::info!(
        "[POLLING] save_spotify_tokens: ENTRY - access_token.len={}",
        tokens.access_token.len()
    );

    let store = app.store("tokens").map_err(|e| {
        log::error!("[POLLING] save_spotify_tokens: store open failed - {}", e);
        e.to_string()
    })?;
    store.set(
        "spotify_tokens",
        serde_json::to_value(tokens).map_err(|e| {
            log::error!("[POLLING] save_spotify_tokens: serialize failed - {}", e);
            e.to_string()
        })?,
    );
    store.save().map_err(|e| {
        log::error!("[POLLING] save_spotify_tokens: save failed - {}", e);
        e.to_string()
    })?;
    log::info!("[POLLING] save_spotify_tokens: SUCCESS");
    Ok(())
}

pub fn load_spotify_tokens(app: &AppHandle) -> Result<Option<SpotifyTokens>, String> {
    log::info!("[POLLING] load_spotify_tokens: ENTRY");

    let store = app.store("tokens").map_err(|e| {
        log::error!("[POLLING] load_spotify_tokens: store open failed - {}", e);
        e.to_string()
    })?;
    let value = store.get("spotify_tokens");

    match value {
        Some(v) => {
            let tokens: SpotifyTokens = serde_json::from_value(v.clone()).map_err(|e| {
                log::error!("[POLLING] load_spotify_tokens: deserialize failed - {}", e);
                e.to_string()
            })?;
            log::info!("[POLLING] load_spotify_tokens: SUCCESS - tokens loaded");
            Ok(Some(tokens))
        }
        None => {
            log::info!("[POLLING] load_spotify_tokens: no tokens in store");
            Ok(None)
        }
    }
}

pub fn save_teams_tokens(app: &AppHandle, tokens: &TeamsTokens) -> Result<(), String> {
    log::info!(
        "[POLLING] save_teams_tokens: ENTRY - access_token.len={}",
        tokens.access_token.len()
    );

    let store = app.store("tokens").map_err(|e| {
        log::error!("[POLLING] save_teams_tokens: store open failed - {}", e);
        e.to_string()
    })?;
    store.set(
        "teams_tokens",
        serde_json::to_value(tokens).map_err(|e| {
            log::error!("[POLLING] save_teams_tokens: serialize failed - {}", e);
            e.to_string()
        })?,
    );
    store.save().map_err(|e| {
        log::error!("[POLLING] save_teams_tokens: save failed - {}", e);
        e.to_string()
    })?;
    log::info!("[POLLING] save_teams_tokens: SUCCESS");
    Ok(())
}

pub fn load_teams_tokens(app: &AppHandle) -> Result<Option<TeamsTokens>, String> {
    log::info!("[POLLING] load_teams_tokens: ENTRY");

    let store = app.store("tokens").map_err(|e| {
        log::error!("[POLLING] load_teams_tokens: store open failed - {}", e);
        e.to_string()
    })?;
    let value = store.get("teams_tokens");

    match value {
        Some(v) => {
            let tokens: TeamsTokens = serde_json::from_value(v.clone()).map_err(|e| {
                log::error!("[POLLING] load_teams_tokens: deserialize failed - {}", e);
                e.to_string()
            })?;
            log::info!("[POLLING] load_teams_tokens: SUCCESS - tokens loaded");
            Ok(Some(tokens))
        }
        None => {
            log::info!("[POLLING] load_teams_tokens: no tokens in store");
            Ok(None)
        }
    }
}

pub fn clear_spotify_tokens(app: &AppHandle) -> Result<(), String> {
    log::info!("[POLLING] clear_spotify_tokens: ENTRY");

    let store = app.store("tokens").map_err(|e| {
        log::error!("[POLLING] clear_spotify_tokens: store open failed - {}", e);
        e.to_string()
    })?;
    store.delete("spotify_tokens");
    store.delete("spotify_client_id");
    store.delete("spotify_client_secret");
    store.delete("spotify_redirect_uri");
    store.delete("spotify_verifier");
    store.save().map_err(|e| {
        log::error!("[POLLING] clear_spotify_tokens: save failed - {}", e);
        e.to_string()
    })?;
    log::info!("[POLLING] clear_spotify_tokens: SUCCESS - tokens cleared from store");
    Ok(())
}

pub fn clear_teams_tokens(app: &AppHandle) -> Result<(), String> {
    log::info!("[POLLING] clear_teams_tokens: ENTRY");

    let store = app.store("tokens").map_err(|e| {
        log::error!("[POLLING] clear_teams_tokens: store open failed - {}", e);
        e.to_string()
    })?;
    store.delete("teams_tokens");
    store.delete("teams_device_code");
    store.save().map_err(|e| {
        log::error!("[POLLING] clear_teams_tokens: save failed - {}", e);
        e.to_string()
    })?;
    log::info!("[POLLING] clear_teams_tokens: SUCCESS - tokens cleared from store");
    Ok(())
}
