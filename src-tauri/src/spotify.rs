use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Utc};
use rand::RngCore;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpotifyTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackInfo {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_art_url: String,
    pub is_playing: bool,
    pub progress_ms: u64,
    pub duration_ms: u64,
}

#[derive(Debug)]
pub enum SpotifyApiError {
    ExpiredToken,
    RateLimited,
    Other(String),
}

impl std::fmt::Display for SpotifyApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpotifyApiError::ExpiredToken => write!(f, "Access token expired"),
            SpotifyApiError::RateLimited => write!(f, "Rate limited"),
            SpotifyApiError::Other(s) => write!(f, "{}", s),
        }
    }
}

pub fn pkce_generate_verifier() -> String {
    let mut bytes = [0u8; 64];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn pkce_generate_challenge(verifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let hash = hasher.finalize();
    URL_SAFE_NO_PAD.encode(hash)
}

pub fn start_spotify_auth(client_id: &str, redirect_uri: &str) -> Result<(String, String), String> {
    let verifier = pkce_generate_verifier();
    let challenge = pkce_generate_challenge(&verifier);

    let state = pkce_generate_verifier();

    let auth_url = format!(
        "https://accounts.spotify.com/authorize?\
         client_id={}\
         &response_type=code\
         &redirect_uri={}\
         &code_challenge_method=S256\
         &code_challenge={}\
         &state={}\
         &scope=user-read-currently-playing user-read-playback-state",
        client_id,
        urlencoding::encode(redirect_uri),
        urlencoding::encode(&challenge),
        urlencoding::encode(&state)
    );

    log::info!("Opening Spotify auth URL: {}", auth_url);
    tauri_plugin_opener::open_url(&auth_url, None::<&str>)
        .map_err(|e| format!("Failed to open browser: {}", e))?;

    Ok((challenge, verifier))
}

pub fn complete_spotify_auth(
    code: &str,
    code_verifier: &str,
    client_id: &str,
    client_secret: &str,
    redirect_uri: &str,
) -> Result<SpotifyTokens, String> {
    let client = Client::new();

    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", client_id),
        ("code_verifier", code_verifier),
    ];

    let response = client
        .post("https://accounts.spotify.com/api/token")
        .form(&params)
        .basic_auth(client_id, Some(client_secret))
        .send()
        .map_err(|e| format!("Failed to send token request: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(format!("Token request failed: {} - {}", status, body));
    }

    #[derive(Deserialize)]
    struct TokenResponse {
        access_token: String,
        refresh_token: String,
        expires_in: u64,
        #[allow(dead_code)]
        token_type: String,
    }

    let token_resp: TokenResponse = response
        .json()
        .map_err(|e| format!("Failed to parse token response: {}", e))?;

    let expires_at = Utc::now() + chrono::Duration::seconds(token_resp.expires_in as i64);

    Ok(SpotifyTokens {
        access_token: token_resp.access_token,
        refresh_token: token_resp.refresh_token,
        expires_at,
    })
}

pub fn refresh_spotify_token(
    tokens: &SpotifyTokens,
    client_id: &str,
    client_secret: &str,
) -> Result<SpotifyTokens, String> {
    let client = Client::new();

    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", &tokens.refresh_token),
        ("client_id", client_id),
    ];

    let response = client
        .post("https://accounts.spotify.com/api/token")
        .form(&params)
        .basic_auth(client_id, Some(client_secret))
        .send()
        .map_err(|e| format!("Failed to send refresh request: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(format!("Refresh request failed: {} - {}", status, body));
    }

    #[derive(Deserialize)]
    struct TokenResponse {
        access_token: String,
        refresh_token: Option<String>,
        expires_in: u64,
        #[allow(dead_code)]
        token_type: String,
    }

    let token_resp: TokenResponse = response
        .json()
        .map_err(|e| format!("Failed to parse refresh response: {}", e))?;

    let expires_at = Utc::now() + chrono::Duration::seconds(token_resp.expires_in as i64);

    Ok(SpotifyTokens {
        access_token: token_resp.access_token,
        refresh_token: token_resp
            .refresh_token
            .unwrap_or_else(|| tokens.refresh_token.clone()),
        expires_at,
    })
}

pub fn get_currently_playing(access_token: &str) -> Result<Option<TrackInfo>, SpotifyApiError> {
    let client = Client::new();

    let response = client
        .get("https://api.spotify.com/v1/me/player/currently-playing")
        .header("Authorization", format!("Bearer {}", access_token))
        .timeout(Duration::from_secs(10))
        .send()
        .map_err(|e| {
            SpotifyApiError::Other(format!("Failed to send currently playing request: {}", e))
        })?;

    match response.status().as_u16() {
        200 => {
            #[derive(Deserialize)]
            struct CurrentlyPlayingResponse {
                item: Option<CurrentlyPlayingItem>,
                is_playing: bool,
                progress_ms: Option<u64>,
            }

            #[derive(Deserialize)]
            struct CurrentlyPlayingItem {
                name: String,
                artists: Vec<Artist>,
                album: Album,
                duration_ms: u64,
            }

            #[derive(Deserialize)]
            struct Artist {
                name: String,
            }

            #[derive(Deserialize)]
            struct Album {
                name: String,
                images: Vec<AlbumImage>,
            }

            #[derive(Deserialize)]
            struct AlbumImage {
                url: String,
            }

            let playing: CurrentlyPlayingResponse = response.json().map_err(|e| {
                SpotifyApiError::Other(format!("Failed to parse currently playing response: {}", e))
            })?;

            if let Some(item) = playing.item {
                let artist = item
                    .artists
                    .iter()
                    .map(|a| a.name.clone())
                    .collect::<Vec<_>>()
                    .join(", ");

                let album_art_url = item
                    .album
                    .images
                    .first()
                    .map(|img| img.url.clone())
                    .unwrap_or_default();

                Ok(Some(TrackInfo {
                    title: item.name,
                    artist,
                    album: item.album.name,
                    album_art_url,
                    is_playing: playing.is_playing,
                    progress_ms: playing.progress_ms.unwrap_or(0),
                    duration_ms: item.duration_ms,
                }))
            } else {
                Ok(None)
            }
        }
        204 => Ok(None),
        401 => Err(SpotifyApiError::ExpiredToken),
        429 => Err(SpotifyApiError::RateLimited),
        _ => {
            let body = response.text().unwrap_or_default();
            Err(SpotifyApiError::Other(format!(
                "Currently playing request failed: {}",
                body
            )))
        }
    }
}

pub fn format_status(track: &TrackInfo, format: &str) -> String {
    let emoji = if track.is_playing { "🎵" } else { "⏸️" };

    format
        .replace("{artist}", &track.artist)
        .replace("{track}", &track.title)
        .replace("{album}", &track.album)
        .replace("{emoji}", emoji)
}

pub fn is_token_expired(tokens: &SpotifyTokens) -> bool {
    Utc::now() >= tokens.expires_at - chrono::Duration::seconds(60)
}
