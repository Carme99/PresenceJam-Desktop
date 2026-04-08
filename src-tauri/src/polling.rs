use std::sync::Arc;
use tokio::time::{sleep, Duration as TokioDuration};
use tauri::{AppHandle, Emitter};
use tauri_plugin_store::StoreExt;
use chrono::Utc;

use crate::AppState;
use crate::spotify::{SpotifyTokens, get_currently_playing, refresh_spotify_token, is_token_expired, format_status};
use crate::teams::{TeamsTokens, set_teams_status_message, clear_teams_status_message};

const DEFAULT_INTERVAL_SECONDS: u64 = 30;
const MAX_INTERVAL_SECONDS: u64 = 60;
const MINIMUM_INTERVAL_SECONDS: u64 = 10;
const ERROR_RETRY_INTERVAL_SECONDS: u64 = 30;

pub fn start_polling(state: Arc<AppState>, app: AppHandle) -> Result<tokio::task::JoinHandle<()>, String> {
    log::info!("start_polling called");
    *state.is_syncing.write() = true;

    let handle = tokio::spawn(async move {
        log::info!("polling_loop task started");
        polling_loop(state, app).await;
    });
    
    log::info!("start_polling: task spawned successfully");
    Ok(handle)
}

async fn polling_loop(state: Arc<AppState>, app: AppHandle) {
    let mut last_track_key: Option<String> = None;

    loop {
        {
            let is_syncing = *state.is_syncing.read();
            if !is_syncing {
                break;
            }
        }

        // Get config (clone to avoid holding lock across await)
        let config = {
            let guard = state.config.read();
            guard.clone()
        };

        // Get Spotify tokens
        let spotify_tokens = state.spotify_tokens.read().clone();

        let spotify_tokens = match spotify_tokens {
            Some(t) => t,
            None => {
                log::debug!("No Spotify tokens available, waiting...");
                sleep(TokioDuration::from_secs(ERROR_RETRY_INTERVAL_SECONDS)).await;
                continue;
            }
        };

        // Refresh token if needed
        let spotify_tokens = if is_token_expired(&spotify_tokens) {
            log::info!("Spotify token expired, refreshing...");
            // Use credentials from config
            let client_id = config.as_ref().map(|c| c.spotify.client_id.as_str()).unwrap_or("");
            let client_secret = config.as_ref().map(|c| c.spotify.client_secret.as_str()).unwrap_or("");
            match refresh_spotify_token(&spotify_tokens, client_id, client_secret) {
                Ok(new_tokens) => {
                    *state.spotify_tokens.write() = Some(new_tokens.clone());
                    new_tokens
                }
                Err(e) => {
                    log::error!("Failed to refresh Spotify token: {}", e);
                    let _ = app.emit("error", serde_json::json!({
                        "source": "spotify",
                        "message": format!("Token refresh failed: {}", e)
                    }));
                    sleep(TokioDuration::from_secs(ERROR_RETRY_INTERVAL_SECONDS)).await;
                    continue;
                }
            }
        } else {
            spotify_tokens
        };

        let access_token = spotify_tokens.access_token.clone();

        // Get currently playing track
        let result = tokio::task::spawn_blocking(move || {
            get_currently_playing(&access_token)
        }).await;

        match result {
            Ok(Ok(Some(track))) => {
                let track_key = format!("{} - {}", track.title, track.artist);

                if last_track_key.as_ref() != Some(&track_key) {
                    last_track_key = Some(track_key.clone());
                    *state.current_track.write() = Some(track.clone());

                    let _ = app.emit("spotify-track-changed", serde_json::json!({
                        "title": track.title,
                        "artist": track.artist,
                        "album": track.album,
                        "album_art_url": track.album_art_url,
                        "is_playing": track.is_playing,
                        "progress_ms": track.progress_ms,
                        "duration_ms": track.duration_ms
                    }));

                    let teams_tokens = state.teams_tokens.read().clone();

                    if let Some(teams_tok) = teams_tokens {
                        if track.is_playing {
                            // Build status message using config format
                            let status_format = config
                                .as_ref()
                                .map(|c| c.teams.status_format.as_str())
                                .unwrap_or("🎵 {artist} - {track} 🎧");
                            let status_message = format_status(&track, status_format);

                            // Calculate expiry: now + (remaining time) + buffer
                            let remaining_ms = track.duration_ms.saturating_sub(track.progress_ms);
                            let buffer_ms = config
                                .as_ref()
                                .map(|c| c.polling.expiry_buffer_seconds as u64)
                                .unwrap_or(10)
                                * 1000;
                            let expiry = Utc::now() + chrono::Duration::milliseconds(
                                remaining_ms as i64 + buffer_ms as i64
                            );
                            let expiry_str = expiry.to_rfc3339();

                            match set_teams_status_message(&teams_tok.access_token, &status_message, Some(&expiry_str)) {
                                Ok(_) => {
                                    log::info!("Updated Teams status: {}", status_message);
                                    let _ = app.emit("presence-updated", serde_json::json!({
                                        "status": status_message,
                                        "timestamp": Utc::now().to_rfc3339()
                                    }));
                                }
                                Err(e) => {
                                    log::error!("Failed to set Teams status: {}", e);
                                    let _ = app.emit("error", serde_json::json!({
                                        "source": "teams",
                                        "message": format!("Failed to update status: {}", e)
                                    }));
                                }
                            }
                        } else {
                            // Track paused - clear status message
                            match clear_teams_status_message(&teams_tok.access_token) {
                                Ok(_) => {
                                    log::info!("Cleared Teams status (paused)");
                                    let _ = app.emit("presence-cleared", serde_json::json!({
                                        "timestamp": Utc::now().to_rfc3339()
                                    }));
                                }
                                Err(e) => {
                                    log::error!("Failed to clear Teams status: {}", e);
                                }
                            }
                        }
                    }
                }

                // Calculate sleep duration
                let sleep_duration = if track.is_playing {
                    let remaining_ms = track.duration_ms.saturating_sub(track.progress_ms);
                    let buffer_ms = 5000u64;
                    let remaining_secs = remaining_ms / 1000;
                    let sleep_secs = remaining_secs.saturating_sub(buffer_ms / 1000);
                    sleep_secs.max(MINIMUM_INTERVAL_SECONDS).min(MAX_INTERVAL_SECONDS)
                } else {
                    DEFAULT_INTERVAL_SECONDS
                };

                sleep(TokioDuration::from_secs(sleep_duration)).await;
            }
            Ok(Ok(None)) => {
                // No track playing
                if last_track_key.is_some() {
                    last_track_key = None;
                    *state.current_track.write() = None;

                    let teams_tokens = state.teams_tokens.read().clone();
                    if let Some(teams_tok) = teams_tokens {
                        match clear_teams_status_message(&teams_tok.access_token) {
                            Ok(_) => {
                                let _ = app.emit("presence-cleared", serde_json::json!({
                                    "timestamp": Utc::now().to_rfc3339()
                                }));
                            }
                            Err(e) => {
                                log::error!("Failed to clear Teams status: {}", e);
                            }
                        }
                    }
                }
                sleep(TokioDuration::from_secs(DEFAULT_INTERVAL_SECONDS)).await;
            }
            Ok(Err(e)) => {
                log::error!("Failed to get currently playing track: {}", e);
                let _ = app.emit("error", serde_json::json!({
                    "source": "spotify",
                    "message": format!("Failed to get currently playing: {}", e)
                }));
                sleep(TokioDuration::from_secs(ERROR_RETRY_INTERVAL_SECONDS)).await;
            }
            Err(e) => {
                log::error!("Task join error: {}", e);
                sleep(TokioDuration::from_secs(ERROR_RETRY_INTERVAL_SECONDS)).await;
            }
        }
    }

    log::info!("Polling loop ended");
}

pub fn stop_polling(state: &AppState) {
    *state.is_syncing.write() = false;
}

pub fn save_spotify_tokens(app: &AppHandle, tokens: &SpotifyTokens) -> Result<(), String> {
    let store = app.store("tokens").map_err(|e| e.to_string())?;
    store.set(
        "spotify_tokens",
        serde_json::to_value(tokens).map_err(|e| e.to_string())?,
    );
    store.save().map_err(|e| e.to_string())?;
    log::info!("Saved Spotify tokens to store");
    Ok(())
}

pub fn load_spotify_tokens(app: &AppHandle) -> Result<Option<SpotifyTokens>, String> {
    let store = app.store("tokens").map_err(|e| e.to_string())?;
    let value = store.get("spotify_tokens");
    match value {
        Some(v) => {
            let tokens: SpotifyTokens = serde_json::from_value(v.clone())
                .map_err(|e| e.to_string())?;
            log::info!("Loaded Spotify tokens from store");
            Ok(Some(tokens))
        }
        None => Ok(None),
    }
}

pub fn save_teams_tokens(app: &AppHandle, tokens: &TeamsTokens) -> Result<(), String> {
    let store = app.store("tokens").map_err(|e| e.to_string())?;
    store.set(
        "teams_tokens",
        serde_json::to_value(tokens).map_err(|e| e.to_string())?,
    );
    store.save().map_err(|e| e.to_string())?;
    log::info!("Saved Teams tokens to store");
    Ok(())
}

pub fn load_teams_tokens(app: &AppHandle) -> Result<Option<TeamsTokens>, String> {
    let store = app.store("tokens").map_err(|e| e.to_string())?;
    let value = store.get("teams_tokens");
    match value {
        Some(v) => {
            let tokens: TeamsTokens = serde_json::from_value(v.clone())
                .map_err(|e| e.to_string())?;
            log::info!("Loaded Teams tokens from store");
            Ok(Some(tokens))
        }
        None => Ok(None),
    }
}
