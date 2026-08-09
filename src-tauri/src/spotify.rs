use chrono::{DateTime, Utc};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct SpotifyTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: DateTime<Utc>,
}
#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct TrackInfo {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_art_url: String,
    pub is_playing: bool,
    // Tauri IPC crosses the boundary via serde_json, which decodes u64
    // values as JS `number` (f64). Override ts-rs's `bigint` default so
    // the generated `.ts` matches what `invoke()` actually returns at
    // runtime — `bigint` would type-lie about the wire shape.
    // `progress_ms` is `Option` because Spotify documents it as "Can be
    // `null`" (live/unknown position) — see issue #165.
    #[ts(type = "number | null")]
    pub progress_ms: Option<u64>,
    #[ts(type = "number")]
    pub duration_ms: u64,
}

#[derive(Debug)]
pub enum SpotifyApiError {
    ExpiredToken,
    /// 429 rate limited. Carries the `Retry-After` header value in seconds
    /// when present and parseable, `None` when the header was absent or
    /// unparseable. See issue #159.
    RateLimited(Option<u64>),
    /// The token endpoint returned `{"error":"invalid_grant"}` — the refresh
    /// token is expired, revoked, or otherwise invalid and the app must
    /// discard it and re-run the authorization flow instead of retrying.
    /// See issue #160.
    InvalidGrant,
    Other(String),
}

impl SpotifyApiError {
    /// Retry-after seconds carried by a `RateLimited` (429) error, if the
    /// server sent a parseable `Retry-After` header. `None` when the header
    /// was absent or unparseable, or when the error is not a 429.
    pub fn retry_after(&self) -> Option<u64> {
        match self {
            SpotifyApiError::RateLimited(secs) => *secs,
            _ => None,
        }
    }
}

impl std::fmt::Display for SpotifyApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpotifyApiError::ExpiredToken => write!(f, "Access token expired"),
            SpotifyApiError::RateLimited(retry_after) => match retry_after {
                Some(secs) => write!(f, "Rate limited (retry after {}s)", secs),
                None => write!(f, "Rate limited"),
            },
            SpotifyApiError::InvalidGrant => write!(
                f,
                "Invalid grant: refresh token is expired, revoked, or otherwise invalid - re-authentication required"
            ),
            SpotifyApiError::Other(s) => write!(f, "{}", s),
        }
    }
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
) -> Result<SpotifyTokens, SpotifyApiError> {
    let client = Client::new();

    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", &tokens.refresh_token),
    ];

    let response = client
        .post("https://accounts.spotify.com/api/token")
        .form(&params)
        .basic_auth(client_id, Some(client_secret))
        .send()
        .map_err(|e| {
            SpotifyApiError::Other(format!("Failed to send refresh request: {}", e))
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        // Spotify returns `{"error":"invalid_grant"}` when the refresh token
        // is expired, revoked, or otherwise invalid. The docs say to discard
        // the refresh token and start the authorization code flow again
        // rather than retrying — see issue #160.
        let error_field = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_owned));
        if error_field.as_deref() == Some("invalid_grant") {
            return Err(SpotifyApiError::InvalidGrant);
        }
        return Err(SpotifyApiError::Other(format!(
            "Refresh request failed: {} - {}",
            status, body
        )));
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
        .map_err(|e| SpotifyApiError::Other(format!("Failed to parse refresh response: {}", e)))?;

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
                /// `track`, `episode`, `ad` or `unknown` — the docs say to
                /// check this and to handle new types gracefully. Default to
                /// empty so an absent field can't hard-fail the parse.
                #[serde(default)]
                currently_playing_type: String,
            }

            #[derive(Deserialize)]
            struct CurrentlyPlayingItem {
                name: String,
                #[serde(default)]
                artists: Vec<Artist>,
                #[serde(default)]
                album: Album,
                duration_ms: u64,
            }

            #[derive(Deserialize)]
            struct Artist {
                name: String,
            }

            #[derive(Deserialize, Default)]
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

            // Only `track` items are track-shaped (name/artists/album).
            // Episodes, ads and future item types must not be forced through
            // TrackInfo — treat them as "nothing playing" instead of erroring.
            // See issue #161.
            if playing.currently_playing_type != "track" {
                return Ok(None);
            }

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
                    progress_ms: playing.progress_ms,
                    duration_ms: item.duration_ms,
                }))
            } else {
                Ok(None)
            }
        }
        204 => Ok(None),
        401 => Err(SpotifyApiError::ExpiredToken),
        429 => {
            // Spotify's rate-limit docs: the 429 response normally includes a
            // `Retry-After` header in seconds — honor it instead of a fixed
            // backoff. `None` when the header is absent/unparseable. See
            // issue #159.
            let retry_after = response
                .headers()
                .get("Retry-After")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse::<u64>().ok());
            Err(SpotifyApiError::RateLimited(retry_after))
        }
        _ => {
            let body = response.text().unwrap_or_default();
            Err(SpotifyApiError::Other(format!(
                "Currently playing request failed: {}",
                body
            )))
        }
    }
}

/// Single source of truth for status-format placeholder substitution.
///
/// Substitutes `{artist}`, `{track}`, `{album}`, and `{emoji}` in the
/// given `format` string. The `{emoji}` placeholder resolves to `🎵` when
/// the track is playing and `⏸️` when paused.
///
/// Both the runtime polling loop (`polling::poll_once`) and the Svelte
/// live preview (Settings.svelte via the `preview_status` Tauri command)
/// must call this — see issue #74.
pub fn format_status(track: &TrackInfo, format: &str) -> String {
    let emoji = if track.is_playing { "🎵" } else { "⏸️" };

    format
        .replace("{artist}", &track.artist)
        .replace("{track}", &track.title)
        .replace("{album}", &track.album)
        .replace("{emoji}", emoji)
}

/// Renders `format` against a sample TrackInfo so the Svelte Settings page
/// can show a live preview without holding a real playing track. Picked up
/// by the `preview_status` Tauri command. See issue #74.
pub fn preview_status_with_sample(format: &str) -> String {
    let sample = TrackInfo {
        title: "Sample Track".to_string(),
        artist: "Sample Artist".to_string(),
        album: "Sample Album".to_string(),
        album_art_url: String::new(),
        is_playing: true,
        progress_ms: Some(0),
        duration_ms: 0,
    };
    format_status(&sample, format)
}

pub fn is_token_expired(tokens: &SpotifyTokens) -> bool {
    Utc::now() >= tokens.expires_at - chrono::Duration::seconds(60)
}

/// Validates that a Spotify access token is still functional.
///
/// Short-circuits on the local `expires_at` field when the token is clearly
/// still good (more than 60s of lifetime remaining), so the typical
/// Onboarding mount doesn't pay for a network round-trip. Only when the
/// token is on the refresh boundary (or already past it) do we make a real
/// HTTP call to confirm.
///
/// Returns `Result<(), SpotifyApiError>`:
/// - `Ok(())` — token works (locally valid OR 200/204)
/// - `Err(SpotifyApiError::ExpiredToken)` — permanent auth failure (401); re-auth required
/// - `Err(SpotifyApiError::RateLimited)` — transient (429); treat as valid, retry after backoff
/// - `Err(SpotifyApiError::Other)` — transient or non-retryable; treat as valid for onboarding
///
/// Callers should inspect `SpotifyApiError` variants to distinguish permanent failures
/// (ExpiredToken) from transient errors (RateLimited, Other).
pub fn validate_spotify_token(tokens: &SpotifyTokens) -> Result<(), SpotifyApiError> {
    // Local pre-check: if the token clearly has plenty of life left, skip
    // the network call. is_token_expired() uses the same 60s window the
    // refresh path uses, so we mirror that exact heuristic here.
    if !is_token_expired(tokens) {
        return Ok(());
    }

    let client = Client::new();
    let response = client
        .get("https://api.spotify.com/v1/me/player/currently-playing")
        .header("Authorization", format!("Bearer {}", tokens.access_token))
        .timeout(Duration::from_secs(10))
        .send()
        .map_err(|e| SpotifyApiError::Other(format!("request failed: {}", e)))?;

    match response.status().as_u16() {
        200 | 204 => Ok(()),
        401 => Err(SpotifyApiError::ExpiredToken),
        429 => {
            // Honor the documented `Retry-After` header (seconds) so the
            // caller can wait out the server's window instead of a fixed
            // backoff. `None` when absent/unparseable. See issue #159.
            let retry_after = response
                .headers()
                .get("Retry-After")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse::<u64>().ok());
            Err(SpotifyApiError::RateLimited(retry_after))
        }
        _ => Err(SpotifyApiError::Other(format!(
            "unexpected status {}",
            response.status()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_track(title: &str, artist: &str, album: &str, is_playing: bool) -> TrackInfo {
        TrackInfo {
            title: title.to_string(),
            artist: artist.to_string(),
            album: album.to_string(),
            album_art_url: String::new(),
            is_playing,
            progress_ms: Some(0),
            duration_ms: 0,
        }
    }

    #[test]
    fn format_status_substitutes_all_placeholders_when_playing() {
        let track = make_track("Karma Police", "Radiohead", "OK Computer", true);
        let result = format_status(&track, "{emoji} {artist} - {track} ({album}) {emoji}");
        assert_eq!(result, "🎵 Radiohead - Karma Police (OK Computer) 🎵");
    }

    #[test]
    fn format_status_uses_pause_emoji_when_paused() {
        let track = make_track("Karma Police", "Radiohead", "OK Computer", false);
        let result = format_status(&track, "{artist} - {track} {emoji}");
        assert_eq!(result, "Radiohead - Karma Police ⏸️");
    }

    #[test]
    fn format_status_leaves_unrecognized_placeholders_alone() {
        let track = make_track("x", "y", "z", true);
        let result = format_status(&track, "{artist} {not_a_placeholder} {track}");
        assert_eq!(result, "y {not_a_placeholder} x");
    }

    #[test]
    fn format_status_works_with_no_placeholders() {
        let track = make_track("x", "y", "z", true);
        let result = format_status(&track, "Static text only");
        assert_eq!(result, "Static text only");
    }

    #[test]
    fn format_status_empty_format_returns_empty() {
        let track = make_track("x", "y", "z", true);
        assert_eq!(format_status(&track, ""), "");
    }

    #[test]
    fn preview_status_with_sample_uses_sample_values_and_playing_emoji() {
        let result = preview_status_with_sample("{emoji} {artist} - {track} ({album}) {emoji}");
        assert_eq!(result, "🎵 Sample Artist - Sample Track (Sample Album) 🎵");
    }

    // Regression guard for issue #78: ensure the SpotifyTokens struct
    // round-trips through serde_json with field-name parity. The
    // ts-rs-generated TS type in `src/lib/types-generated/SpotifyTokens.ts`
    // mirrors these field names exactly; a future field rename that
    // updates only one side will break this test (proving the drift
    // before it ships to consumers).
    #[test]
    fn spotify_tokens_serde_roundtrip() {
        let original = SpotifyTokens {
            access_token: "access-abc".to_string(),
            refresh_token: "refresh-xyz".to_string(),
            expires_at: Utc::now(),
        };
        let json = serde_json::to_string(&original).expect("serialize");
        let parsed: SpotifyTokens =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.access_token, original.access_token);
        assert_eq!(parsed.refresh_token, original.refresh_token);
        assert_eq!(parsed.expires_at, original.expires_at);
    }

    // Regression guard for issue #78: ensure TrackInfo's u64 fields
    // serialise as plain JSON numbers (not strings), so the Tauri IPC
    // bridge delivers them to JS as `number` (f64). The matching TS
    // override is `#[ts(type = "number")]` on duration_ms and
    // `#[ts(type = "number | null")]` on progress_ms (Option<u64> — see
    // issue #165).
    #[test]
    fn track_info_u64_fields_serialize_as_numbers() {
        let track = TrackInfo {
            title: "Test Track".to_string(),
            artist: "Test Artist".to_string(),
            album: "Test Album".to_string(),
            album_art_url: "https://example.com/art.jpg".to_string(),
            is_playing: true,
            progress_ms: Some(123_456),
            duration_ms: 240_000,
        };
        let json: serde_json::Value =
            serde_json::to_value(&track).expect("to_value");
        // u64 must round-trip as a JSON number, not a string. `Some(v)`
        // serialises as the bare number; `None` would serialise as `null`.
        assert!(
            json["progress_ms"].is_number(),
            "progress_ms must serialise as a JSON number, got {:?}",
            json["progress_ms"]
        );
        assert!(
            json["duration_ms"].is_number(),
            "duration_ms must serialise as a JSON number, got {:?}",
            json["duration_ms"]
        );
        assert_eq!(json["progress_ms"].as_u64(), Some(123_456));
        assert_eq!(json["duration_ms"].as_u64(), Some(240_000));
    }
}
