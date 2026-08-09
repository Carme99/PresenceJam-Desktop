use chrono::{DateTime, Utc};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Parse the `Retry-After` header (seconds) from a 429 response.
/// Returns `None` when the header is absent or unparseable. See issue #159.
fn parse_retry_after(response: &reqwest::blocking::Response) -> Option<u64> {
    response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
}

/// Extracts the `reason` field from a Spotify API error body. The player
/// endpoints return `{"error":{"status":404,"message":"...","reason":
/// "NO_ACTIVE_DEVICE"}}` when no device is active — see issue #3.0-P3.
fn parse_error_reason(body: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.get("reason")).and_then(|r| r.as_str()))
        .map(str::to_owned)
}

/// True when a response status + body are the player endpoint's
/// "no active device" 404 (`reason: "NO_ACTIVE_DEVICE"`). Split out of
/// `map_player_error` so the mapping is unit-testable without a live
/// `reqwest::blocking::Response`.
fn is_no_active_device_404(status: u16, body: &str) -> bool {
    status == 404 && parse_error_reason(body).as_deref() == Some("NO_ACTIVE_DEVICE")
}

/// Maps a non-success Spotify response to `SpotifyApiError`. Shared by the
/// player control/query functions so the mapping lives in one place:
/// - 401 → `ExpiredToken` (re-auth required)
/// - 403 → `NotPremium` (playback control requires Premium)
/// - 429 → `RateLimited` honouring the `Retry-After` header (issue #159)
/// - 404 with `reason: "NO_ACTIVE_DEVICE"` → `NoActiveDevice` (callers can
///   offer device transfer)
/// - anything else → `Other` with the response body for diagnosis
///
/// Takes the response by value because `Response::text` consumes it; each
/// arm reads the response exactly once.
fn map_player_error(response: reqwest::blocking::Response, context: &str) -> SpotifyApiError {
    let status = response.status().as_u16();
    match status {
        401 => SpotifyApiError::ExpiredToken,
        403 => SpotifyApiError::NotPremium,
        429 => SpotifyApiError::RateLimited(parse_retry_after(&response)),
        _ => {
            let body = response.text().unwrap_or_default();
            if is_no_active_device_404(status, &body) {
                return SpotifyApiError::NoActiveDevice;
            }
            SpotifyApiError::Other(format!("{} request failed: {}", context, body))
        }
    }
}

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

/// A Spotify playback device (GET /v1/me/player/devices).
/// `id` is `Option` because Spotify documents it as "Can be `null`" for
/// some devices; such devices cannot be targeted by transfer/playback
/// commands. See issue #3.0-P3.
#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct DeviceInfo {
    pub id: Option<String>,
    pub name: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub is_active: bool,
    pub is_private_session: bool,
    pub is_restricted: bool,
    pub supports_volume: bool,
}

/// The user's playback queue (GET /v1/me/player/queue), mapped down to
/// the track-shaped subset the app understands — episodes and ads are
/// gated out (same item-type gate as `get_currently_playing`, issue
/// #161). See issue #3.0-P3.
#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct QueueInfo {
    pub currently_playing: Option<TrackInfo>,
    pub up_next: Vec<TrackInfo>,
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
    /// The player endpoint returned a 404 whose error body carries
    /// `reason: "NO_ACTIVE_DEVICE"` — no device is actively playing, so a
    /// device must be selected (transfer) before playback commands work.
    /// See issue #3.0-P3.
    NoActiveDevice,
    /// The player endpoint returned 403 — playback control requires Spotify
    /// Premium, which this account does not have.
    NotPremium,
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
            SpotifyApiError::NoActiveDevice => write!(
                f,
                "No active playback device - start playback on a device or transfer to one"
            ),
            SpotifyApiError::NotPremium => write!(
                f,
                "Playback control requires Spotify Premium"
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

/// The `currently_playing_type` field of the currently-playing response.
/// Typed so the `track` gate can't be broken by a typo; `Unknown` is the
/// explicit catch-all for Spotify's documented "unknown" value and any
/// future item types. Defaults to `Unknown` so an absent field can't
/// hard-fail the parse. See issue #161.
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
enum CurrentlyPlayingType {
    Track,
    Episode,
    Ad,
    #[default]
    #[serde(other)]
    Unknown,
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
                /// `track`, `episode`, `ad` or anything else — the docs say
                /// to check this and to handle new types gracefully. Default
                /// to `Unknown` so an absent field can't hard-fail the parse.
                #[serde(default)]
                currently_playing_type: CurrentlyPlayingType,
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
            if !matches!(playing.currently_playing_type, CurrentlyPlayingType::Track) {
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
            Err(SpotifyApiError::RateLimited(parse_retry_after(&response)))
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

/// Sends a Spotify player-control request (PUT/POST) and maps the response.
/// `device_id` becomes the `device_id` query param when given (playback
/// commands act on the active device when omitted); `body` is the optional
/// JSON payload (used by `player_transfer`). Shared by the four transport
/// commands so the error mapping (404 NO_ACTIVE_DEVICE, 403 non-Premium,
/// 429 Retry-After) lives in exactly one place. See issue #3.0-P3.
fn send_player_command(
    method: reqwest::Method,
    path: &str,
    access_token: &str,
    device_id: Option<&str>,
    body: Option<serde_json::Value>,
    context: &str,
) -> Result<(), SpotifyApiError> {
    let client = Client::new();
    let mut url = format!("https://api.spotify.com/v1{}", path);
    if let Some(id) = device_id {
        url = format!("{}?device_id={}", url, id);
    }
    let mut request = client
        .request(method, &url)
        .header("Authorization", format!("Bearer {}", access_token))
        .timeout(Duration::from_secs(10));
    if let Some(payload) = body {
        request = request.json(&payload);
    }
    let response = request
        .send()
        .map_err(|e| SpotifyApiError::Other(format!("Failed to send {} request: {}", context, e)))?;

    let status = response.status().as_u16();
    if status == 202 || status == 204 {
        Ok(())
    } else {
        Err(map_player_error(response, context))
    }
}

/// Resumes playback. `device_id` targets a specific device; `None` acts on
/// the active device. PUT /v1/me/player/play. See issue #3.0-P3.
pub fn player_play(access_token: &str, device_id: Option<&str>) -> Result<(), SpotifyApiError> {
    send_player_command(
        reqwest::Method::PUT,
        "/me/player/play",
        access_token,
        device_id,
        None,
        "play",
    )
}

/// Pauses playback. `device_id` targets a specific device; `None` acts on
/// the active device. PUT /v1/me/player/pause. See issue #3.0-P3.
pub fn player_pause(access_token: &str, device_id: Option<&str>) -> Result<(), SpotifyApiError> {
    send_player_command(
        reqwest::Method::PUT,
        "/me/player/pause",
        access_token,
        device_id,
        None,
        "pause",
    )
}

/// Skips to the next track. `device_id` targets a specific device; `None`
/// acts on the active device. POST /v1/me/player/next. See issue #3.0-P3.
pub fn player_next(access_token: &str, device_id: Option<&str>) -> Result<(), SpotifyApiError> {
    send_player_command(
        reqwest::Method::POST,
        "/me/player/next",
        access_token,
        device_id,
        None,
        "next",
    )
}

/// Skips to the previous track. `device_id` targets a specific device;
/// `None` acts on the active device. POST /v1/me/player/previous.
/// See issue #3.0-P3.
pub fn player_previous(access_token: &str, device_id: Option<&str>) -> Result<(), SpotifyApiError> {
    send_player_command(
        reqwest::Method::POST,
        "/me/player/previous",
        access_token,
        device_id,
        None,
        "previous",
    )
}

/// Transfers playback to `device_id`, optionally starting playback.
/// The device goes in the JSON body (`device_ids`), not the query string.
/// PUT /v1/me/player. See issue #3.0-P3.
pub fn player_transfer(access_token: &str, device_id: &str, play: bool) -> Result<(), SpotifyApiError> {
    send_player_command(
        reqwest::Method::PUT,
        "/me/player",
        access_token,
        None,
        Some(serde_json::json!({ "device_ids": [device_id], "play": play })),
        "transfer",
    )
}

/// Lists the user's available playback devices.
/// GET /v1/me/player/devices. See issue #3.0-P3.
pub fn get_devices(access_token: &str) -> Result<Vec<DeviceInfo>, SpotifyApiError> {
    let client = Client::new();
    let response = client
        .get("https://api.spotify.com/v1/me/player/devices")
        .header("Authorization", format!("Bearer {}", access_token))
        .timeout(Duration::from_secs(10))
        .send()
        .map_err(|e| SpotifyApiError::Other(format!("Failed to send devices request: {}", e)))?;

    let status = response.status().as_u16();
    if status != 200 {
        return Err(map_player_error(response, "devices"));
    }

    #[derive(Deserialize)]
    struct DevicesResponse {
        devices: Vec<DeviceInfo>,
    }

    let devices: DevicesResponse = response
        .json()
        .map_err(|e| SpotifyApiError::Other(format!("Failed to parse devices response: {}", e)))?;
    Ok(devices.devices)
}

/// Fetches the user's playback queue. Only `track` items are mapped to
/// `TrackInfo` — episodes and ads are gated out with the same item-type
/// gate as `get_currently_playing` (issue #161). GET /v1/me/player/queue.
/// See issue #3.0-P3.
pub fn get_queue(access_token: &str) -> Result<QueueInfo, SpotifyApiError> {
    let client = Client::new();
    let response = client
        .get("https://api.spotify.com/v1/me/player/queue")
        .header("Authorization", format!("Bearer {}", access_token))
        .timeout(Duration::from_secs(10))
        .send()
        .map_err(|e| SpotifyApiError::Other(format!("Failed to send queue request: {}", e)))?;

    let status = response.status().as_u16();
    if status == 204 {
        // 204 No Content — nothing queued; not an error.
        return Ok(QueueInfo {
            currently_playing: None,
            up_next: Vec::new(),
        });
    }
    if status != 200 {
        return Err(map_player_error(response, "queue"));
    }

    #[derive(Deserialize)]
    struct QueueResponse {
        currently_playing: Option<QueueItem>,
        queue: Vec<QueueItem>,
    }

    #[derive(Deserialize)]
    struct QueueItem {
        #[serde(rename = "type")]
        type_: String,
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

    fn map_item(item: QueueItem) -> Option<TrackInfo> {
        // Only `track` items are track-shaped (name/artists/album).
        // Episodes, ads and future item types must not be forced through
        // TrackInfo — same gate as get_currently_playing (issue #161).
        if item.type_ != "track" {
            return None;
        }
        Some(TrackInfo {
            title: item.name,
            artist: item
                .artists
                .iter()
                .map(|a| a.name.clone())
                .collect::<Vec<_>>()
                .join(", "),
            album: item.album.name,
            album_art_url: item
                .album
                .images
                .first()
                .map(|img| img.url.clone())
                .unwrap_or_default(),
            // Queue items are by definition not the currently playing one.
            is_playing: false,
            progress_ms: None,
            duration_ms: item.duration_ms,
        })
    }

    let queue: QueueResponse = response
        .json()
        .map_err(|e| SpotifyApiError::Other(format!("Failed to parse queue response: {}", e)))?;

    Ok(QueueInfo {
        currently_playing: queue.currently_playing.and_then(map_item),
        up_next: queue.queue.into_iter().filter_map(map_item).collect(),
    })
}

/// Base64url-decodes the payload (middle segment) of a Spotify access
/// token JWT and returns the granted `scope` claim split on spaces.
/// Informational only — no signature verification. Returns an empty Vec
/// when the token isn't a decodable JWT with a `scope` claim. Used by the
/// Settings page to detect whether `user-modify-playback-state` is missing
/// (one-time-reconnect banner, issue #3.0-P3).
pub fn decode_spotify_granted_scopes(access_token: &str) -> Vec<String> {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine as _;
    let payload = access_token.split('.').nth(1).unwrap_or_default();
    let scopes = URL_SAFE_NO_PAD
        .decode(payload)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|v| v.get("scope").and_then(|s| s.as_str()).map(str::to_owned))
        .unwrap_or_default();
    if scopes.is_empty() {
        Vec::new()
    } else {
        scopes.split(' ').map(str::to_owned).collect()
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
            Err(SpotifyApiError::RateLimited(parse_retry_after(&response)))
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
    use base64::Engine as _;

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

    // Regression guard for issue #3.0-P3: the player endpoint's 404 body
    // carries `reason: "NO_ACTIVE_DEVICE"` and must surface as a distinct
    // error so callers can offer device transfer instead of a generic
    // failure. The parse helper must also be robust to non-JSON bodies.
    #[test]
    fn parse_error_reason_extracts_no_active_device() {
        let body = r#"{"error":{"status":404,"message":"Player command failed: No active device found","reason":"NO_ACTIVE_DEVICE"}}"#;
        assert_eq!(
            parse_error_reason(body).as_deref(),
            Some("NO_ACTIVE_DEVICE")
        );
        assert_eq!(parse_error_reason("not json").as_deref(), None);
        assert_eq!(parse_error_reason(r#"{"error":{"status":404}}"#).as_deref(), None);
    }

    // Regression guard for issue #3.0-P3: only a 404 whose error body
    // carries `reason: "NO_ACTIVE_DEVICE"` maps to NoActiveDevice — other
    // reasons, other statuses, and non-JSON bodies must not.
    #[test]
    fn is_no_active_device_404_matches_only_no_active_device() {
        let no_active = r#"{"error":{"status":404,"message":"Player command failed: No active device found","reason":"NO_ACTIVE_DEVICE"}}"#;
        assert!(is_no_active_device_404(404, no_active));
        let other_reason = r#"{"error":{"status":404,"message":"Device not found","reason":"DEVICE_NOT_FOUND"}}"#;
        assert!(!is_no_active_device_404(404, other_reason));
        let wrong_status = r#"{"error":{"status":403,"message":"Forbidden","reason":"NO_ACTIVE_DEVICE"}}"#;
        assert!(!is_no_active_device_404(403, wrong_status));
        assert!(!is_no_active_device_404(404, "not json"));
        assert!(!is_no_active_device_404(404, ""));
    }

    // Regression guard for issue #3.0-P3: the Settings reconnect banner
    // reads granted scopes from the JWT payload of the stored access
    // token (base64url, no signature verification). A fake but structurally
    // valid token must decode to the scope list.
    #[test]
    fn decode_spotify_granted_scopes_extracts_scope_claim() {
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
            r#"{"scope":"user-read-currently-playing user-read-playback-state user-modify-playback-state"}"#,
        );
        let token = format!("header.{}.signature", payload);
        assert_eq!(
            decode_spotify_granted_scopes(&token),
            vec![
                "user-read-currently-playing".to_string(),
                "user-read-playback-state".to_string(),
                "user-modify-playback-state".to_string(),
            ]
        );
    }

    // Guard: tokens that aren't JWTs (or whose payload has no scope claim)
    // must yield an empty list — the Settings banner treats that as
    // "scope missing" rather than crashing.
    #[test]
    fn decode_spotify_granted_scopes_empty_when_not_decodable() {
        assert!(decode_spotify_granted_scopes("not-a-jwt").is_empty());
        assert!(decode_spotify_granted_scopes("a.b.c").is_empty());
        let no_scope = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(r#"{"sub":"user123"}"#);
        assert!(decode_spotify_granted_scopes(&format!("h.{}.s", no_scope)).is_empty());
    }
}
