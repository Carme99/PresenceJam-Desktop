use chrono::{DateTime, Utc};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use std::time::Duration;

/// Parse the `Retry-After` header from a 429 response, supporting both
/// delta-seconds (`120`) and HTTP-date (`Wed, 21 Aug 2026 12:00:00 GMT`)
/// forms per RFC 7231 §7.1.3. Returns `None` when the header is absent
/// or unparseable. See issue #159.
fn parse_retry_after_value(s: &str) -> Option<u64> {
    let s = s.trim();
    if let Ok(secs) = s.parse::<u64>() {
        return Some(secs.min(300));
    }
    if let Ok(date) = httpdate::parse_http_date(s) {
        let secs = date
            .duration_since(std::time::SystemTime::now())
            .unwrap_or(std::time::Duration::from_secs(0))
            .as_secs()
            .min(300);
        return Some(secs);
    }
    None
}

fn parse_retry_after(response: &reqwest::blocking::Response) -> Option<u64> {
    response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_retry_after_value)
}

/// Extracts the `reason` field from a Spotify API error body. The player
/// endpoints return `{"error":{"status":404,"message":"...","reason":
/// "NO_ACTIVE_DEVICE"}}` when no device is active — see issue #3.0-P3.
fn parse_error_reason(body: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v.get("error")
                .and_then(|e| e.get("reason"))
                .and_then(|r| r.as_str())
                .map(str::to_owned)
        })
}

/// True when a response status + body are the player endpoint's
/// "no active device" 404 (`reason: "NO_ACTIVE_DEVICE"`). Split out of
/// `map_player_error` so the mapping is unit-testable without a live
/// `reqwest::blocking::Response`.
fn is_no_active_device_404(status: u16, body: &str) -> bool {
    status == 404 && parse_error_reason(body).as_deref() == Some("NO_ACTIVE_DEVICE")
}

/// Classifies a non-success Spotify response into the typed error every
/// endpoint shares (issue #749).
///
/// Split out of [`map_player_error`] so variant selection is unit-testable
/// without a live `reqwest::blocking::Response`, exactly like
/// [`is_no_active_device_404`]:
/// - 401 → `ExpiredToken` (re-auth required)
/// - 403 → `NotPremium` (playback control requires Premium)
/// - 429 → `RateLimited` honouring the parsed `Retry-After` (issue #159)
/// - 404 with `reason: "NO_ACTIVE_DEVICE"` → `NoActiveDevice` (callers can
///   offer device transfer)
/// - 5xx → `Transient` (the request is worth retrying)
/// - anything else → `Http` carrying the status
///
/// `context` names the endpoint in the user-facing message; `body` is kept on
/// the error for logging only (issue #796).
fn classify_spotify_status(
    status: u16,
    retry_after: Option<u64>,
    context: &'static str,
    body: &str,
) -> SpotifyApiError {
    match status {
        401 => SpotifyApiError::ExpiredToken,
        403 => SpotifyApiError::NotPremium,
        429 => SpotifyApiError::RateLimited(retry_after),
        _ if is_no_active_device_404(status, body) => SpotifyApiError::NoActiveDevice,
        500..=599 => SpotifyApiError::Transient {
            status,
            context,
            body: body.to_string(),
        },
        _ => SpotifyApiError::Http {
            status,
            context,
            body: body.to_string(),
        },
    }
}

/// Maps a non-success Spotify response to `SpotifyApiError` through
/// [`classify_spotify_status`]. Shared by every endpoint — the player
/// commands, devices, queue and the currently-playing GET — so one HTTP status
/// yields one variant and one user-facing message everywhere.
///
/// Takes the response by value because `Response::text` consumes it; the
/// status and the `Retry-After` header are read before the body.
fn map_player_error(
    response: reqwest::blocking::Response,
    context: &'static str,
) -> SpotifyApiError {
    let status = response.status().as_u16();
    let retry_after = parse_retry_after(&response);
    let body = response.text().unwrap_or_default();
    classify_spotify_status(status, retry_after, context, &body)
}

#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct SpotifyTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: DateTime<Utc>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, ts_rs::TS)]
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

/// The user's playback queue (GET /v1/me/player/queue), mapped down to the
/// app's track-shaped `TrackInfo`. Tracks AND podcast/audiobook episodes are
/// mapped (their `type` field decides — issue #581); only ads and item types
/// the client does not know are dropped. See issues #161 and #3.0-P3.
#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct QueueInfo {
    pub currently_playing: Option<TrackInfo>,
    pub up_next: Vec<TrackInfo>,
}

/// Repeat mode — the three values `repeat_state` may carry (issue #582).
///
/// Grounding:
/// https://developer.spotify.com/documentation/web-api/reference/get-the-users-currently-playing-track
/// documents `repeat_state` as `off`, `track` or `context`; the same three
/// values are what `PUT /me/player/repeat` accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RepeatState {
    #[default]
    Off,
    Context,
    Track,
}

impl RepeatState {
    /// Normalises a wire `repeat_state`. Anything else — a value Spotify adds
    /// later, or a body that omitted the field — degrades to
    /// [`RepeatState::Off`], so neither the status text nor the tray toggle
    /// ever claims a mode the API did not report.
    pub fn from_api(state: &str) -> Self {
        match state.to_ascii_lowercase().as_str() {
            "track" => Self::Track,
            "context" => Self::Context,
            _ => Self::Off,
        }
    }

    /// The wire value for `PUT /me/player/repeat`.
    pub fn as_api(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Context => "context",
            Self::Track => "track",
        }
    }

    /// True while any repeat mode is active — drives the tray check mark and
    /// the `{repeat}` token.
    pub fn is_on(self) -> bool {
        !matches!(self, Self::Off)
    }

    /// The next mode in the cycle Spotify's own player uses:
    /// `off` → `context` → `track` → `off` (issue #582).
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Context,
            Self::Context => Self::Track,
            Self::Track => Self::Off,
        }
    }
}

/// Episode-only metadata for a podcast/audiobook item. Every field is empty
/// when the body omitted it (`show` is absent on some items and
/// `publisher` is not guaranteed), so the episode mapping degrades cleanly
/// instead of failing the poll (issue #581).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EpisodeInfo {
    /// `show.name` — the podcast/audiobook the episode belongs to.
    pub show_name: String,
    /// `show.publisher`.
    pub publisher: String,
}

/// The playback context a currently-playing poll body carries next to the
/// item itself: which device is playing, which context the item was started
/// from, and the shuffle/repeat modes (issue #580). Every field comes from
/// the SAME poll response — no extra request, no new scope.
#[derive(Debug, Clone, Default)]
pub struct PlaybackContext {
    /// `device.name`, `""` when the body carried no device.
    pub device: String,
    /// `context.display_name` — the playlist/album/artist/show the item plays
    /// from. `""` for ad-hoc playback (a search result or a queue has no
    /// context object).
    pub playlist: String,
    /// `shuffle_state`, `false` when the body omitted it.
    pub shuffle: bool,
    /// `repeat_state`, normalised through [`RepeatState::from_api`].
    pub repeat: RepeatState,
}

impl PlaybackContext {
    /// The sample context the Svelte Settings preview renders against, so a
    /// user editing `{device}`/`{playlist}` sees a value instead of a hole.
    /// The runtime path always supplies the context parsed from the poll body.
    pub fn sample() -> Self {
        Self {
            device: "Kitchen speaker".to_string(),
            playlist: "Workout Mix".to_string(),
            shuffle: true,
            repeat: RepeatState::Context,
        }
    }
}

/// One observed playing item: the media in the app's frozen `TrackInfo` shape
/// plus the episode metadata and playback context the same poll body carries
/// (issues #580/#581).
///
/// `TrackInfo` itself stays exactly as it is: it is the ts-rs-exported IPC
/// shape consumed by `SyncStatus`, the Dashboard and the tray, and it is
/// built with exhaustive struct literals outside this module — widening it
/// would be a breaking wire change with no consumer that needs it.
#[derive(Debug, Clone, Default)]
pub struct NowPlaying {
    pub media: TrackInfo,
    /// `Some` for a podcast/audiobook episode (`item.type == "episode"`),
    /// `None` for a music track.
    pub episode: Option<EpisodeInfo>,
    pub context: PlaybackContext,
}

/// Default status template for episodes (issue #581): a user's music template
/// must not be applied verbatim to a 90-minute episode. This value is the
/// documented default of the `teams.episode_status_format` config key the
/// episode slice needs from `config.rs` (see the report note in the commit
/// body) — until that key exists, the built-in default IS what episodes use.
pub const DEFAULT_EPISODE_STATUS_FORMAT: &str = "🎙️ {show} - {episode}";

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
    /// The endpoint returned 403 — playback control requires Spotify
    /// Premium, which this account does not have.
    NotPremium,
    /// A 5xx response: the service failed on its side, so the same request is
    /// worth retrying — the Spotify counterpart of `TeamsApiError::Transient`
    /// (issue #749). `context` names the endpoint that failed; `body` is the
    /// response body **for logging only** and never reaches `Display`
    /// (issue #796).
    Transient {
        status: u16,
        context: &'static str,
        body: String,
    },
    /// Any other status the client does not classify (issue #749). Carrying
    /// the status is what lets a caller tell a permanent 4xx from a transient
    /// 5xx; `Other(String)` alone could not. `body` is for logging only.
    Http {
        status: u16,
        context: &'static str,
        body: String,
    },
    /// A failure with no HTTP status at all: client construction, transport
    /// errors and response-parse errors raised inside this module.
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
            // The response body is deliberately not interpolated (issue #796):
            // a raw CDN error page or JSON envelope in a toast is not
            // actionable, and the same string is written to the log.
            SpotifyApiError::Transient {
                status, context, ..
            } => write!(
                f,
                "{} request failed (HTTP {}) - Spotify reported a temporary problem, try again shortly",
                context, status
            ),
            SpotifyApiError::Http {
                status, context, ..
            } => write!(f, "{} request failed (HTTP {})", context, status),
            SpotifyApiError::Other(s) => write!(f, "{}", s),
        }
    }
}

/// Creates a reqwest blocking client with standard config (user agent + 10s timeout).
/// Ensures consistent HTTP client settings across all Spotify API calls
/// (mirrors `teams.rs::build_teams_client`).
///
/// The 10s timeout bounds the token exchange and refresh paths, which
/// previously built a bare `Client::new()` with no timeout (issue #347).
/// The User-Agent closes the #353 UA gap as a drive-by.
///
/// User-Agent uses `env!("CARGO_PKG_VERSION")` so it tracks `Cargo.toml`
/// automatically on every release — never hardcode the version.
///
/// The client is built once per process and cached (#576):
/// `reqwest::blocking::Client` is `Arc`-backed, so every later call returns a
/// refcount bump over the same connection pool instead of a fresh pool per
/// poll iteration (a new TCP+TLS handshake every 30–60 s, ~2880 discarded
/// pools per day at the default cadence). The cache also memoizes a failed
/// build: `ClientBuilder::build` fails only on environmental TLS/runtime
/// init, where a retry would fail identically. The signature stays
/// `Result<Client, String>` so the existing call sites and their error
/// mapping are untouched.
fn build_spotify_client() -> Result<Client, String> {
    static CLIENT: LazyLock<Result<Client, String>> = LazyLock::new(|| {
        Client::builder()
            .user_agent(format!("PresenceJam/{}", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))
    });
    CLIENT.as_ref().map(|c| c.clone()).map_err(|e| e.clone())
}

pub fn complete_spotify_auth(
    code: &str,
    code_verifier: &str,
    client_id: &str,
    client_secret: &str,
    redirect_uri: &str,
) -> Result<SpotifyTokens, String> {
    let client = build_spotify_client()?;

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

    // Issue #350: the exchange body is parsed by `parse_exchange_token_response`
    // below so the mapping is unit-testable without a live HTTP response.
    // `refresh_token` is `Option` there (a `String` would collapse "field
    // absent" into "Failed to parse token response"); a missing field now
    // surfaces as `token response omitted refresh_token`. Spotify always
    // sends a refresh token on the authorization_code grant, so its absence
    // means the response is unusable for persistent auth.
    let body = response
        .text()
        .map_err(|e| format!("Failed to read token response: {}", e))?;
    parse_exchange_token_response(&body)
}

/// Shortest access-token lifetime the app accepts from the token endpoint,
/// in seconds. Spotify documents 3600 s; anything below this floor is treated
/// as malformed rather than believed (issue #931).
const MIN_TOKEN_LIFETIME_SECS: u64 = 30;

/// Longest access-token lifetime the app accepts from the token endpoint, in
/// seconds (24 h). Anything above is treated as malformed (issue #931).
const MAX_TOKEN_LIFETIME_SECS: u64 = 86_400;

/// The shared shape of Spotify's token-endpoint success body, used by both the
/// authorization_code exchange and the refresh_token grant (issue #931; the two
/// paths previously carried byte-identical local copies of this struct).
#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
    #[allow(dead_code)]
    token_type: String,
}

/// `expires_at` for a token whose response claimed a lifetime of `expires_in`
/// seconds, clamped to
/// [`MIN_TOKEN_LIFETIME_SECS`]..=[`MAX_TOKEN_LIFETIME_SECS`] (issue #931).
///
/// `expires_in` is server-controlled. Casting it straight through `as i64` and
/// `chrono::Duration::seconds` panicked for values above `i64::MAX / 1000` and
/// wrapped to a negative offset for values that overflow the cast, producing an
/// `expires_at` in the past — an instantly-expired token that made
/// `is_token_expired()` permanently true, so every poll iteration and every tray
/// click refreshed again. `try_seconds` cannot panic; the clamp keeps a
/// malformed response inside the range the app can reason about, falling back to
/// one hour if the duration is somehow still out of range.
fn token_expiry(expires_in: u64, now: DateTime<Utc>) -> DateTime<Utc> {
    let lifetime = expires_in.clamp(MIN_TOKEN_LIFETIME_SECS, MAX_TOKEN_LIFETIME_SECS);
    let offset = chrono::Duration::try_seconds(lifetime as i64)
        .unwrap_or_else(|| chrono::Duration::hours(1));
    now + offset
}

/// Parse an OAuth authorization_code exchange body into [`SpotifyTokens`].
/// Split out of `complete_spotify_auth` so the mapping is unit-testable
/// without a live `reqwest::blocking::Response` (issue #350).
fn parse_exchange_token_response(body: &str) -> Result<SpotifyTokens, String> {
    let token_resp: TokenResponse =
        serde_json::from_str(body).map_err(|e| format!("Failed to parse token response: {}", e))?;
    let refresh_token = token_resp.refresh_token.ok_or_else(|| {
        "token response omitted refresh_token - please try signing in again.".to_string()
    })?;

    let expires_at = token_expiry(token_resp.expires_in, Utc::now());

    Ok(SpotifyTokens {
        access_token: token_resp.access_token,
        refresh_token,
        expires_at,
    })
}

/// Process-wide serialization for the Spotify token refresh (issue #930).
///
/// The poll thread and every tray/playback command refresh independently, so a
/// tray click landing while the poller refreshes the same expired token put two
/// POSTs carrying the same refresh token in flight at once: duplicated load on
/// the endpoint that rate-limits, plus a second failure path (`invalid_grant`,
/// HTTP 429) that can still reach the user even though the other caller
/// succeeded. This mutex allows one refresh POST at a time; the cache behind it
/// lets the callers that waited on the lock reuse the fresh token the winner
/// just obtained instead of POSTing again.
static REFRESH_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

/// The last successful refresh, keyed by the refresh token it was performed
/// with — both the token that went in and the one that came back, since Spotify
/// may rotate the refresh token. See [`REFRESH_LOCK`].
static REFRESHED: LazyLock<parking_lot::Mutex<Option<(String, SpotifyTokens)>>> =
    LazyLock::new(|| parking_lot::Mutex::new(None));

/// The cached token pair for `refresh_token`, when a previous refresh used the
/// same credential and the cached access token is still fresh.
///
/// Callers hold [`REFRESH_LOCK`] for the whole read-modify-write, so the only
/// lock order in the process is `REFRESH_LOCK` → `REFRESHED`.
fn cached_refresh(refresh_token: &str, now: DateTime<Utc>) -> Option<SpotifyTokens> {
    let guard = REFRESHED.lock();
    let (key, tokens) = guard.as_ref()?;
    if key != refresh_token && tokens.refresh_token != refresh_token {
        return None;
    }
    (tokens.expires_at > now).then(|| tokens.clone())
}

/// Runs `fetch` under [`REFRESH_LOCK`], returning the cached token pair for
/// `refresh_token` when another caller already refreshed it while this caller
/// waited for the lock. Exactly one `fetch` runs per burst of concurrent
/// refreshes of the same credential (issue #930).
///
/// Split out of [`refresh_spotify_token`] so the one-POST-per-burst contract is
/// unit-testable without a live token endpoint.
fn refresh_serialized<F>(
    refresh_token: &str,
    now: DateTime<Utc>,
    fetch: F,
) -> Result<SpotifyTokens, SpotifyApiError>
where
    F: FnOnce() -> Result<SpotifyTokens, SpotifyApiError>,
{
    let _guard = REFRESH_LOCK.lock();
    if let Some(tokens) = cached_refresh(refresh_token, now) {
        log::debug!("[SPOTIFY] refresh_spotify_token: reusing the token another caller just refreshed");
        return Ok(tokens);
    }
    let tokens = fetch()?;
    *REFRESHED.lock() = Some((refresh_token.to_string(), tokens.clone()));
    Ok(tokens)
}

pub fn refresh_spotify_token(
    tokens: &SpotifyTokens,
    client_id: &str,
    client_secret: &str,
) -> Result<SpotifyTokens, SpotifyApiError> {
    refresh_serialized(&tokens.refresh_token, Utc::now(), || {
        request_refreshed_token(tokens, client_id, client_secret)
    })
}

/// The single refresh POST behind [`refresh_spotify_token`]. Never call this
/// directly: it has no serialization and is not idempotent for the caller.
fn request_refreshed_token(
    tokens: &SpotifyTokens,
    client_id: &str,
    client_secret: &str,
) -> Result<SpotifyTokens, SpotifyApiError> {
    let client = build_spotify_client().map_err(SpotifyApiError::Other)?;

    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", &tokens.refresh_token),
    ];

    let response = client
        .post("https://accounts.spotify.com/api/token")
        .form(&params)
        .basic_auth(client_id, Some(client_secret))
        .send()
        .map_err(|e| SpotifyApiError::Other(format!("Failed to send refresh request: {}", e)))?;

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

    let token_resp: TokenResponse = response
        .json()
        .map_err(|e| SpotifyApiError::Other(format!("Failed to parse refresh response: {}", e)))?;

    let expires_at = token_expiry(token_resp.expires_in, Utc::now());

    Ok(SpotifyTokens {
        access_token: token_resp.access_token,
        refresh_token: token_resp
            .refresh_token
            .unwrap_or_else(|| tokens.refresh_token.clone()),
        expires_at,
    })
}

/// The `type` field of an item, or of the envelope's
/// `currently_playing_type`. Typed so the item gate can't be broken by a
/// typo; `Unknown` is the explicit catch-all for Spotify's documented
/// "unknown" value and any future item types. Defaults to `Unknown` so an
/// absent field can't hard-fail the parse. See issues #161 and #581.
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

impl CurrentlyPlayingType {
    /// Classifies a raw `type` string. An item's own `type` is read as a
    /// plain string because the docs direct clients to check it themselves —
    /// "make sure that your client properly handles cases of new types in
    /// the future by checking against the `type` field of each object" — and
    /// an unrecognised value must degrade, not fail the whole parse.
    fn from_wire(raw: &str) -> Self {
        match raw {
            "track" => Self::Track,
            "episode" => Self::Episode,
            "ad" => Self::Ad,
            _ => Self::Unknown,
        }
    }
}

/// The documented `oneOf(TrackObject, EpisodeObject)` item shared by the
/// currently-playing and queue bodies. Every field is `#[serde(default)]` so
/// a body that omits one degrades instead of failing the whole parse.
#[derive(Debug, Deserialize, Default)]
struct MediaItem {
    #[serde(rename = "type", default)]
    type_: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    images: Vec<ItemImage>,
    #[serde(default)]
    artists: Vec<ItemName>,
    #[serde(default)]
    album: ItemAlbum,
    #[serde(default)]
    show: ItemShow,
    #[serde(default)]
    duration_ms: u64,
}

#[derive(Debug, Deserialize, Default)]
struct ItemName {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Deserialize, Default)]
struct ItemAlbum {
    #[serde(default)]
    name: String,
    #[serde(default)]
    images: Vec<ItemImage>,
}

/// `item.show` — the podcast/audiobook an episode belongs to. Absent on
/// tracks and on bodies that omit it, hence the `Default`.
#[derive(Debug, Deserialize, Default)]
struct ItemShow {
    #[serde(default)]
    name: String,
    #[serde(default)]
    publisher: String,
    #[serde(default)]
    images: Vec<ItemImage>,
}

#[derive(Debug, Deserialize, Default)]
struct ItemImage {
    #[serde(default)]
    url: String,
}

fn first_image_url(images: &[ItemImage]) -> String {
    images
        .first()
        .map(|img| img.url.clone())
        .unwrap_or_default()
}

/// Maps one wire item onto the app's frozen `TrackInfo`, plus the episode
/// metadata when the item is a podcast/audiobook episode (issues #580/#581).
///
/// The item's own `type` decides which branch of the documented
/// `oneOf(track, episode)` union to read: a track reads `artists`/`album`, an
/// episode reads `show`. An episode's show name takes the `artist` slot so
/// the Dashboard, the desktop notification and the tray keep rendering one
/// shape, and its publisher takes the `album` slot — a second copy of the
/// show name on the Dashboard's album line would read as a duplicate, and an
/// empty line reads as a bug. `ad` and every unknown type return `None`,
/// i.e. "nothing playing", exactly as the pre-#581 gate did.
fn map_media_item(
    item: MediaItem,
    is_playing: bool,
    progress_ms: Option<u64>,
) -> Option<(TrackInfo, Option<EpisodeInfo>)> {
    let (artist, album, album_art_url, episode) = match CurrentlyPlayingType::from_wire(&item.type_)
    {
        CurrentlyPlayingType::Track => (
            item.artists
                .iter()
                .map(|a| a.name.clone())
                .collect::<Vec<_>>()
                .join(", "),
            item.album.name.clone(),
            first_image_url(&item.album.images),
            None,
        ),
        CurrentlyPlayingType::Episode => (
            item.show.name.clone(),
            item.show.publisher.clone(),
            // The episode's own cover art is what the Dashboard and the
            // notifications show; the show's cover is the fallback for
            // items that carry no images of their own.
            if item.images.is_empty() {
                first_image_url(&item.show.images)
            } else {
                first_image_url(&item.images)
            },
            Some(EpisodeInfo {
                show_name: item.show.name.clone(),
                publisher: item.show.publisher.clone(),
            }),
        ),
        CurrentlyPlayingType::Ad | CurrentlyPlayingType::Unknown => return None,
    };
    Some((
        TrackInfo {
            title: item.name,
            artist,
            album,
            album_art_url,
            is_playing,
            progress_ms,
            duration_ms: item.duration_ms,
        },
        episode,
    ))
}

/// Resolves the human-readable name of the playback context (`{playlist}`).
///
/// `context.display_name` is read first — Spotify's newer responses carry it
/// — but the reference documents only `type`/`uri` on the context object, so
/// when it is missing the name is derived from the item for the context
/// types whose name the item already contains (an `album` context's name is
/// the item's album, an `artist` context's name is its artist, a `show`
/// context's name is the episode's show). A *playlist* context has no
/// derivable name — that needs `GET /playlists/{id}`, i.e. a scope this app
/// does not request — so `{playlist}` renders empty there instead of
/// inventing one.
fn context_display_name(context: Option<&ContextObject>, item: Option<&MediaItem>) -> String {
    if let Some(name) = context.and_then(|c| c.display_name.as_deref()) {
        if !name.is_empty() {
            return name.to_string();
        }
    }
    let Some(item) = item else {
        return String::new();
    };
    match context.map(|c| c.type_.as_str()).unwrap_or_default() {
        "album" => item.album.name.clone(),
        "artist" => item
            .artists
            .first()
            .map(|a| a.name.clone())
            .unwrap_or_default(),
        "show" => item.show.name.clone(),
        _ => String::new(),
    }
}

/// `context` of the currently-playing body: `type` plus the undocumented-but-
/// present `display_name`.
#[derive(Debug, Deserialize, Default)]
struct ContextObject {
    #[serde(rename = "type", default)]
    type_: String,
    #[serde(default)]
    display_name: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct DeviceName {
    #[serde(default)]
    name: String,
}

/// Parses a 200 body of `GET /me/player/currently-playing`.
///
/// Split out of `get_currently_playing` so the whole mapping — track vs
/// episode, ad gating, playback context, missing fields — is unit-testable
/// without a live response. `Err` carries the message for
/// `SpotifyApiError::Other`; `Ok(None)` means "nothing to report" (204-style
/// empty body, an ad, an unknown item type, or no item at all).
fn parse_currently_playing_body(body: &str) -> Result<Option<NowPlaying>, String> {
    #[derive(Debug, Deserialize, Default)]
    struct CurrentlyPlayingResponse {
        #[serde(default)]
        item: Option<MediaItem>,
        #[serde(default)]
        is_playing: bool,
        #[serde(default)]
        progress_ms: Option<u64>,
        /// `track`, `episode`, `ad` or anything else — the docs say to check
        /// this and to handle new types gracefully. Defaults to `Unknown` so
        /// an absent field can't hard-fail the parse.
        #[serde(default)]
        currently_playing_type: CurrentlyPlayingType,
        #[serde(default)]
        device: Option<DeviceName>,
        #[serde(default)]
        context: Option<ContextObject>,
        #[serde(default)]
        shuffle_state: bool,
        #[serde(default)]
        repeat_state: String,
    }

    let playing: CurrentlyPlayingResponse = serde_json::from_str(body)
        .map_err(|e| format!("Failed to parse currently playing response: {}", e))?;

    // An ad is never "listening": the envelope's type says so even when the
    // item looks like a track, and the pre-#581 gate dropped it the same way.
    if matches!(playing.currently_playing_type, CurrentlyPlayingType::Ad) {
        return Ok(None);
    }

    // Resolved before `item` is moved into the mapper (needs both halves).
    let playlist = context_display_name(playing.context.as_ref(), playing.item.as_ref());

    let Some((media, episode)) = playing
        .item
        .and_then(|item| map_media_item(item, playing.is_playing, playing.progress_ms))
    else {
        return Ok(None);
    };

    Ok(Some(NowPlaying {
        media,
        episode,
        context: PlaybackContext {
            device: playing.device.map(|d| d.name).unwrap_or_default(),
            playlist,
            shuffle: playing.shuffle_state,
            repeat: RepeatState::from_api(&playing.repeat_state),
        },
    }))
}

/// Reads the `ETag` response header as an owned validator. Only the arms
/// that carry a representation (200/204) call this: a 304 refreshes
/// nothing, because the stored validator stays authoritative
/// (RFC 9110 §13.1.2), so the steady state of the conditional-GET feature
/// allocates no String it would immediately drop (#577).
fn read_etag(response: &reqwest::blocking::Response) -> Option<String> {
    response
        .headers()
        .get("ETag")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
}

/// Conditional GET of the currently-playing track (candidate C11(1),
/// docs/scope-3.3.md §C11).
///
/// Outcome is [`CurrentlyPlaying`]: a parsed 200/204 body plus the
/// response's `ETag` validator, or a 304 Not Modified with no body.
///
/// Grounding note: Spotify's official reference for
/// `GET /me/player/currently-playing`
/// (https://developer.spotify.com/documentation/web-api/reference/get-the-users-currently-playing-track)
/// documents only 200/401/403/429 responses and does NOT document an
/// `ETag` response header or `If-None-Match`/304 handling. ETag support
/// is therefore EMPIRICAL, relying only on standard RFC 9110 semantics
/// (§8.8.3 `ETag`, §13 conditional requests, §15.4.5 `304 Not
/// Modified`). Every step degrades gracefully: no stored ETag ⇒ the
/// request goes out unconditional; no `ETag` in a response ⇒ the next
/// poll is unconditional; any other status keeps the pre-existing error
/// paths. If Spotify never sends an ETag this whole feature is a
/// behavioral no-op.
// The `Modified` payload is the whole observed item (issues #580/#581) and is
// deliberately held by value: it is built once per poll and moved a couple of
// times, so a `Box` would buy nothing and cost one heap allocation on every
// poll — the same per-poll waste #577 removed from this call path.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum CurrentlyPlaying {
    /// 200/204 — the full response was parsed. `now` is `None` for a 204, a
    /// missing `item`, an ad, or an item type the client does not know
    /// (issues #161/#581).
    Modified {
        now: Option<NowPlaying>,
        /// The `ETag` response header when present; echo it back as
        /// `If-None-Match` on the next poll. `None` ⇒ the next poll is
        /// unconditional (RFC 9110 §13.1.2).
        etag: Option<String>,
    },
    /// 304 Not Modified — body absent; the caller keeps its prior state
    /// (including the stored `ETag` validator, which a 304 leaves
    /// authoritative per RFC 9110 §13.1.2) and skips JSON parse /
    /// status-format work.
    NotModified,
}
pub fn get_currently_playing(
    access_token: &str,
    if_none_match: Option<&str>,
) -> Result<CurrentlyPlaying, SpotifyApiError> {
    let client = build_spotify_client().map_err(SpotifyApiError::Other)?;

    // Issue #581: `additional_types=episode` makes the endpoint return the
    // podcast/audiobook item instead of ignoring it — the reference: "A
    // comma-separated list of item types that your client supports besides
    // the default `track` type. Valid types are: `track` and `episode`."
    let mut request = client
        .get("https://api.spotify.com/v1/me/player/currently-playing?additional_types=episode")
        .header("Authorization", format!("Bearer {}", access_token));
    // Conditional GET (candidate C11): a stored ETag goes out as
    // If-None-Match so an unchanged resource can answer 304 without a
    // body. No stored ETag (first poll, or the server stopped sending
    // one) ⇒ unconditional GET, exactly the pre-C11 behavior.
    if let Some(etag) = if_none_match {
        request = request.header("If-None-Match", etag);
    }
    let response = request
        .timeout(Duration::from_secs(10))
        .send()
        .map_err(|e| {
            SpotifyApiError::Other(format!("Failed to send currently playing request: {}", e))
        })?;

    // No `ETag` read here. A 304 means the representation is unchanged, so
    // the validator already stored by the caller stays authoritative
    // (RFC 9110 §13.1.2) — and this is the steady state of the whole
    // conditional-GET feature, so allocating a String here only to drop it
    // was pure per-poll waste (#577). The 200/204 arms read it instead.

    match response.status().as_u16() {
        304 => Ok(CurrentlyPlaying::NotModified),
        200 => {
            // Read before `.text()` consumes the response.
            let response_etag = read_etag(&response);
            let body = response.text().map_err(|e| {
                SpotifyApiError::Other(format!("Failed to read currently playing body: {}", e))
            })?;
            let now = parse_currently_playing_body(&body).map_err(SpotifyApiError::Other)?;
            Ok(CurrentlyPlaying::Modified {
                now,
                etag: response_etag,
            })
        }
        204 => Ok(CurrentlyPlaying::Modified {
            now: None,
            etag: read_etag(&response),
        }),
        _ => Err(map_player_error(response, "Currently playing")),
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
    context: &'static str,
) -> Result<(), SpotifyApiError> {
    let client = build_spotify_client().map_err(SpotifyApiError::Other)?;
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
    } else {
        // Empty-body POST/PUT (next/previous/play/pause): Spotify's edge
        // 411s without an explicit length. `.body("")` forces reqwest to
        // emit `Content-Length: 0`; the manual header is belt-and-braces
        // in case a layer strips one form.
        request = request.body("").header("Content-Length", "0");
    }
    let response = request.send().map_err(|e| {
        SpotifyApiError::Other(format!("Failed to send {} request: {}", context, e))
    })?;

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
pub fn player_transfer(
    access_token: &str,
    device_id: &str,
    play: bool,
) -> Result<(), SpotifyApiError> {
    send_player_command(
        reqwest::Method::PUT,
        "/me/player",
        access_token,
        None,
        Some(serde_json::json!({ "device_ids": [device_id], "play": play })),
        "transfer",
    )
}

/// Turns shuffle on or off. `device_id` targets a specific device; `None`
/// acts on the active device. PUT /v1/me/player/shuffle with the documented
/// `{"state": <bool>}` body — the reference documents `state` as
/// "**true** : Shuffle user's playback." and the response as 204/401/403/429.
/// Scope `user-modify-playback-state`, already requested. See issue #582.
pub fn player_set_shuffle(
    access_token: &str,
    state: bool,
    device_id: Option<&str>,
) -> Result<(), SpotifyApiError> {
    send_player_command(
        reqwest::Method::PUT,
        "/me/player/shuffle",
        access_token,
        device_id,
        Some(serde_json::json!({ "state": state })),
        "shuffle",
    )
}

/// Sets the repeat mode. `device_id` targets a specific device; `None` acts
/// on the active device. PUT /v1/me/player/repeat with the documented
/// `{"state": "off" | "track" | "context"}` body. Scope
/// `user-modify-playback-state`, already requested. See issue #582.
pub fn player_set_repeat(
    access_token: &str,
    state: RepeatState,
    device_id: Option<&str>,
) -> Result<(), SpotifyApiError> {
    send_player_command(
        reqwest::Method::PUT,
        "/me/player/repeat",
        access_token,
        device_id,
        Some(serde_json::json!({ "state": state.as_api() })),
        "repeat",
    )
}

/// Lists the user's available playback devices.
/// GET /v1/me/player/devices. See issue #3.0-P3.
pub fn get_devices(access_token: &str) -> Result<Vec<DeviceInfo>, SpotifyApiError> {
    let client = build_spotify_client().map_err(SpotifyApiError::Other)?;
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

/// Fetches the user's playback queue. Items are mapped through the same
/// `oneOf(track, episode)` mapper as `get_currently_playing`, so podcast and
/// audiobook episodes reach the tray's Up Next submenu instead of being
/// dropped (issue #583); ads and unknown item types are still gated out
/// (issue #161). GET /v1/me/player/queue. See issue #3.0-P3.
pub fn get_queue(access_token: &str) -> Result<QueueInfo, SpotifyApiError> {
    let client = build_spotify_client().map_err(SpotifyApiError::Other)?;
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

    let queue_body = response
        .text()
        .map_err(|e| SpotifyApiError::Other(format!("Failed to read queue response: {}", e)))?;
    parse_queue_body(&queue_body).map_err(SpotifyApiError::Other)
}

/// Parses a 200 body of `GET /me/player/queue`.
///
/// Split out of `get_queue` for the same reason as
/// [`parse_currently_playing_body`]: the item gate and the track/episode
/// mapping are what the tray's Up Next submenu renders, and both are worth
/// pinning without a live response.
fn parse_queue_body(body: &str) -> Result<QueueInfo, String> {
    #[derive(Debug, Deserialize, Default)]
    struct QueueResponse {
        #[serde(default)]
        currently_playing: Option<MediaItem>,
        #[serde(default)]
        queue: Vec<MediaItem>,
    }

    fn map_item(item: MediaItem) -> Option<TrackInfo> {
        // Queue items are by definition not the currently playing one, so
        // there is no playing state or position to report.
        map_media_item(item, false, None).map(|(media, _episode)| media)
    }

    let queue: QueueResponse =
        serde_json::from_str(body).map_err(|e| format!("Failed to parse queue response: {}", e))?;
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

/// Substitutes one `{token}` per pass and never re-scans what it inserted
/// (issue #341): a value that arrives from Spotify containing a literal
/// expanded by a later token. The previous chained-`replace` implementation
/// documented that rule but only honoured it for `{emoji}` — a track titled
/// `{album}` was re-expanded by the later `{album}` pass. Substituting
/// against a fixed token table in one pass makes the rule true for every
/// token and drops the intermediate strings.
fn substitute_placeholders(format: &str, tokens: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(format.len());
    let mut rest = format;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let candidate = &rest[open..];
        let Some(close) = candidate.find('}') else {
            // Unterminated brace: copy the tail verbatim.
            out.push_str(candidate);
            return out;
        };
        let name = &candidate[1..close];
        match tokens.iter().find(|(token, _)| *token == name) {
            Some((_, value)) => out.push_str(value),
            // Unknown placeholder: keep it exactly as written.
            None => out.push_str(&candidate[..=close]),
        }
        rest = &candidate[close + 1..];
    }
    out.push_str(rest);
    out
}

/// `mm:ss` for a progress position, `""` when Spotify reported none (a
/// live/unknown-position stream — issue #165).
fn format_progress(progress_ms: Option<u64>) -> String {
    match progress_ms {
        Some(ms) => {
            let total_secs = ms / 1000;
            format!("{}:{:02}", total_secs / 60, total_secs % 60)
        }
        None => String::new(),
    }
}

/// The placeholder vocabulary shared by every render path. Values are the
/// *rendered* text for one item, so the table is built here once per render
/// and reused for whichever template is being filled in.
fn placeholder_values<'a>(
    media: &'a TrackInfo,
    episode: Option<&'a EpisodeInfo>,
    context: &'a PlaybackContext,
    progress: &'a str,
    emoji: &'a str,
) -> Vec<(&'static str, &'a str)> {
    vec![
        ("emoji", emoji),
        ("artist", media.artist.as_str()),
        ("track", media.title.as_str()),
        ("album", media.album.as_str()),
        ("device", context.device.as_str()),
        // `{context}` is an alias: the token names differ in how much the
        // user knows about where the item was started from.
        ("playlist", context.playlist.as_str()),
        ("context", context.playlist.as_str()),
        ("progress", progress),
        // Icon-only tokens for the two playback modes (issue #580): an
        // "on"/"off" word would wreck a status that reads as a sentence, and
        // a template author can put them behind a literal space.
        ("shuffle", if context.shuffle { "🔀" } else { "" }),
        ("repeat", if context.repeat.is_on() { "🔁" } else { "" }),
        ("show", episode.map(|e| e.show_name.as_str()).unwrap_or("")),
        // `{episode}` is the episode's own name, and renders only for an
        // episode — the whole episode token family stays empty on a music
        // track, so a template that mentions one never prints the track
        // title by accident.
        (
            "episode",
            if episode.is_some() {
                media.title.as_str()
            } else {
                ""
            },
        ),
        (
            "publisher",
            episode.map(|e| e.publisher.as_str()).unwrap_or(""),
        ),
    ]
}

/// Single source of truth for status-format placeholder substitution.
///
/// Renders `format` against one observed item: its media (`{artist}`,
/// `{track}`, `{album}`), its episode metadata (`{show}`, `{episode}`,
/// `{publisher}`) and the playback context the same poll body carried
/// (`{device}`, `{playlist}`/`{context}`, `{progress}`, `{shuffle}`,
/// `{repeat}`). `{emoji}` is `🎵` for a playing track, `🎙️` for a playing
/// episode and `⏸️` when paused. The runtime polling loop
/// (`polling::poll_once`) calls this with the parsed body — issues
/// #580/#581.
pub fn format_status_with_context(
    media: &TrackInfo,
    episode: Option<&EpisodeInfo>,
    context: &PlaybackContext,
    format: &str,
) -> String {
    let emoji = match (media.is_playing, episode.is_some()) {
        (false, _) => "⏸️",
        (true, true) => "🎙️",
        (true, false) => "🎵",
    };
    let progress = format_progress(media.progress_ms);
    let tokens = placeholder_values(media, episode, context, &progress, emoji);
    substitute_placeholders(format, &tokens)
}

/// The Settings-preview entry point: renders `format` against the sample
/// media and the sample playback context ([`PlaybackContext::sample`]).
///
/// Kept as a 2-argument surface because `commands::misc::preview_status`'s
/// profanity branch builds its own sample media and has no playback context
/// to offer; sharing one sample context here is what makes that branch and
/// `preview_status_with_sample` render the new tokens identically. The
/// runtime polling loop must NOT use this — it calls
/// [`format_status_with_context`] with the context parsed from the poll
/// body. See issues #74 and #580.
pub fn format_status(track: &TrackInfo, format: &str) -> String {
    format_status_with_context(track, None, &PlaybackContext::sample(), format)
}

/// The sample item the Settings preview renders. Shared with
/// `format_status`'s fallback context so both preview branches agree.
fn sample_track() -> TrackInfo {
    TrackInfo {
        title: "Sample Track".to_string(),
        artist: "Sample Artist".to_string(),
        album: "Sample Album".to_string(),
        album_art_url: String::new(),
        is_playing: true,
        // `Some(0)` mirrors the sample `commands::misc::preview_status`
        // builds on the profanity path, so `{progress}` renders "0:00" on
        // both sides of that toggle instead of disagreeing.
        progress_ms: Some(0),
        duration_ms: 0,
    }
}

/// Renders `format` against a sample item so the Svelte Settings page can
/// show a live preview without holding a real playing track. Picked up by
/// the `preview_status` Tauri command. See issue #74.
pub fn preview_status_with_sample(format: &str) -> String {
    format_status_with_context(&sample_track(), None, &PlaybackContext::sample(), format)
}

pub fn is_token_expired(tokens: &SpotifyTokens) -> bool {
    Utc::now() >= tokens.expires_at - chrono::Duration::seconds(60)
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
    // Issue #341: track metadata containing a literal "{emoji}" must
    // survive verbatim. `{emoji}` substitutes before the data fields, so
    // the data-inserted token is never re-scanned and re-expanded.
    #[test]
    fn format_status_does_not_expand_data_inserted_emoji_token() {
        let track = make_track("x", "{emoji}", "z", true);
        let result = format_status(&track, "{artist} - {track}");
        assert_eq!(result, "{emoji} - x");
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

    // Issue #580: the Settings preview has two Rust branches. With the
    // profanity filter on, `commands::misc::preview_status` builds its own
    // sample media and calls `format_status`; with it off, the same command
    // calls `preview_status_with_sample`. Both must render the new context
    // tokens identically, or toggling that filter would silently change
    // which placeholders the preview appears to support.
    #[test]
    fn preview_branches_render_the_same_placeholders() {
        let format = "{emoji} {artist} - {track} ({album}) {device} {playlist} {progress} {shuffle} {repeat}";
        let filter_branch_sample = TrackInfo {
            title: "Sample Track".to_string(),
            artist: "Sample Artist".to_string(),
            album: "Sample Album".to_string(),
            album_art_url: String::new(),
            is_playing: true,
            progress_ms: Some(0),
            duration_ms: 0,
        };
        assert_eq!(
            format_status(&filter_branch_sample, format),
            preview_status_with_sample(format)
        );
    }

    fn make_context() -> PlaybackContext {
        PlaybackContext {
            device: "Kitchen speaker".to_string(),
            playlist: "Workout Mix".to_string(),
            shuffle: true,
            repeat: RepeatState::Context,
        }
    }

    fn make_episode(show: &str) -> EpisodeInfo {
        EpisodeInfo {
            show_name: show.to_string(),
            publisher: "Acme Audio".to_string(),
        }
    }

    // Issue #580: the tokens the poll body already carries render from the
    // parsed context. Fails pre-fix — `{device}`/`{playlist}`/`{progress}`/
    // `{shuffle}`/`{repeat}` stayed in the output as literal placeholders.
    #[test]
    fn format_status_renders_playback_context_tokens() {
        let mut track = make_track("Karma Police", "Radiohead", "OK Computer", true);
        track.progress_ms = Some(83_000);
        let context = make_context();
        let result = format_status_with_context(
            &track,
            None,
            &context,
            "{device}|{playlist}|{context}|{progress}|{shuffle}|{repeat}",
        );
        assert_eq!(result, "Kitchen speaker|Workout Mix|Workout Mix|1:23|🔀|🔁");
    }

    // The mode tokens are icon-only: off must leave nothing behind, so a
    // template like "🎵 {artist} - {track}{repeat}" stays a clean sentence.
    #[test]
    fn format_status_omits_mode_tokens_when_off() {
        let track = make_track("Karma Police", "Radiohead", "OK Computer", true);
        let context = PlaybackContext {
            device: "Kitchen speaker".to_string(),
            playlist: String::new(),
            shuffle: false,
            repeat: RepeatState::Off,
        };
        let result = format_status_with_context(&track, None, &context, "a{shuffle}b{repeat}c");
        assert_eq!(result, "abc");
    }

    // Issue #165: no known position ⇒ `{progress}` renders empty rather than
    // "0:00", so a live stream's status does not claim to be at the start.
    #[test]
    fn format_progress_matches_spotify_position_semantics() {
        assert_eq!(format_progress(None), "");
        assert_eq!(format_progress(Some(0)), "0:00");
        assert_eq!(format_progress(Some(59_999)), "0:59");
        assert_eq!(format_progress(Some(60_000)), "1:00");
        assert_eq!(format_progress(Some(3_599_000)), "59:59");
        // A 90-minute podcast is 90 minutes, not an hour-and-a-half rollover.
        assert_eq!(format_progress(Some(5_400_000)), "90:00");
    }

    // Issue #581: episodes render through the same formatter, with their own
    // tokens and their own play glyph.
    #[test]
    fn format_status_renders_episode_tokens_and_episode_emoji() {
        let media = make_track(
            "Episode 12: Focus",
            "The Deep Work Show",
            "Acme Audio",
            true,
        );
        let episode = make_episode("The Deep Work Show");
        let result = format_status_with_context(
            &media,
            Some(&episode),
            &PlaybackContext::default(),
            "{emoji} {show} - {episode} ({publisher})",
        );
        assert_eq!(
            result,
            "🎙️ The Deep Work Show - Episode 12: Focus (Acme Audio)"
        );
    }

    // An episode template's `{show}`/`{episode}`/`{publisher}` must render
    // empty for a music track instead of leaking the previous episode.
    #[test]
    fn format_status_episode_tokens_empty_for_track() {
        let track = make_track("Karma Police", "Radiohead", "OK Computer", true);
        let result = format_status_with_context(
            &track,
            None,
            &PlaybackContext::default(),
            "[{show}|{episode}|{publisher}]",
        );
        assert_eq!(result, "[||]");
    }

    #[test]
    fn format_status_paused_episode_uses_pause_emoji() {
        let media = make_track("Episode 12", "Show", "Publisher", false);
        let episode = make_episode("Show");
        let result = format_status_with_context(
            &media,
            Some(&episode),
            &PlaybackContext::default(),
            "{emoji}",
        );
        assert_eq!(result, "⏸️");
    }

    // Issue #341, extended from `{emoji}` to every token: a value that came
    // from Spotify is never re-scanned, so metadata containing a token is
    // copied verbatim. Fails pre-fix for the data tokens — the chained
    // `replace` calls re-expanded a title of "{album}" into the album name.
    #[test]
    fn format_status_does_not_expand_data_inserted_tokens() {
        let track = make_track("{album} - {device}", "{artist}", "OK Computer", true);
        let context = make_context();
        let result = format_status_with_context(&track, None, &context, "{artist} - {track}");
        assert_eq!(result, "{artist} - {album} - {device}");
    }

    #[test]
    fn format_status_keeps_unterminated_brace_verbatim() {
        let track = make_track("x", "y", "z", true);
        let result = format_status_with_context(
            &track,
            None,
            &PlaybackContext::default(),
            "y {artist} {unclosed",
        );
        assert_eq!(result, "y y {unclosed");
    }

    #[test]
    fn repeat_state_normalises_the_documented_wire_values() {
        assert_eq!(RepeatState::from_api("off"), RepeatState::Off);
        assert_eq!(RepeatState::from_api("track"), RepeatState::Track);
        assert_eq!(RepeatState::from_api("context"), RepeatState::Context);
        assert_eq!(RepeatState::from_api("CONTEXT"), RepeatState::Context);
        // Anything Spotify adds later degrades to Off rather than guessing.
        assert_eq!(RepeatState::from_api("party"), RepeatState::Off);
        assert_eq!(RepeatState::from_api(""), RepeatState::Off);
    }

    #[test]
    fn repeat_state_cycles_off_context_track_and_round_trips_to_the_api() {
        assert_eq!(RepeatState::Off.next(), RepeatState::Context);
        assert_eq!(RepeatState::Context.next(), RepeatState::Track);
        assert_eq!(RepeatState::Track.next(), RepeatState::Off);
        assert_eq!(RepeatState::Off.as_api(), "off");
        assert_eq!(RepeatState::Context.as_api(), "context");
        assert_eq!(RepeatState::Track.as_api(), "track");
        assert!(!RepeatState::Off.is_on());
        assert!(RepeatState::Context.is_on());
        assert!(RepeatState::Track.is_on());
    }

    /// A realistic `currently-playing` body for a podcast episode, verbatim
    /// from the shapes the reference documents: `item.type = "episode"`,
    /// `show` with `name`/`publisher`/`images`, `resume_point`,
    /// `is_externally_hosted`, plus the playback context fields.
    const EPISODE_BODY: &str = r#"{
        "device": {"id": "dev1", "is_active": true, "name": "Kitchen speaker", "type": "computer", "volume_percent": 59},
        "repeat_state": "context",
        "shuffle_state": true,
        "context": {"type": "show", "href": "https://api.spotify.com/v1/shows/s1", "uri": "spotify:show:s1"},
        "timestamp": 1758000000000,
        "progress_ms": 90000,
        "is_playing": true,
        "item": {
            "type": "episode",
            "id": "e1",
            "name": "Episode 12: Focus",
            "duration_ms": 5400000,
            "is_externally_hosted": true,
            "images": [{"url": "https://i.scdn.co/image/ep1"}],
            "resume_point": {"fully_played": false, "resume_position_ms": 90000},
            "show": {
                "name": "The Deep Work Show",
                "publisher": "Acme Audio",
                "images": [{"url": "https://i.scdn.co/image/show1"}]
            }
        },
        "currently_playing_type": "episode"
    }"#;

    // Issue #581: an episode must produce a real item — pre-fix the whole
    // body mapped to `None` ("Nothing playing on Spotify") and a podcast
    // morning looked like a broken app.
    #[test]
    fn parse_episode_body_maps_title_show_and_publisher() {
        let now = parse_currently_playing_body(EPISODE_BODY)
            .expect("episode body must parse")
            .expect("episode body must yield an item");
        assert_eq!(now.media.title, "Episode 12: Focus");
        // The show takes the artist slot so the Dashboard/notification/tray
        // keep rendering one shape; the publisher fills the subtitle slot.
        assert_eq!(now.media.artist, "The Deep Work Show");
        assert_eq!(now.media.album, "Acme Audio");
        assert_eq!(now.media.album_art_url, "https://i.scdn.co/image/ep1");
        assert_eq!(now.media.duration_ms, 5_400_000);
        assert_eq!(now.media.progress_ms, Some(90_000));
        assert!(now.media.is_playing);
        let episode = now.episode.expect("episode metadata must be present");
        assert_eq!(episode.show_name, "The Deep Work Show");
        assert_eq!(episode.publisher, "Acme Audio");
    }

    #[test]
    fn parse_episode_body_maps_the_playback_context() {
        let now = parse_currently_playing_body(EPISODE_BODY)
            .expect("episode body must parse")
            .expect("episode body must yield an item");
        assert_eq!(now.context.device, "Kitchen speaker");
        // A `show` context carries no display_name in the documented shape,
        // so the name is derived from the episode's own show.
        assert_eq!(now.context.playlist, "The Deep Work Show");
        assert!(now.context.shuffle);
        assert_eq!(now.context.repeat, RepeatState::Context);
    }

    // The episode mapping must degrade, not drop: `show` absent leaves the
    // show/publisher fields empty while the episode still reports playing.
    #[test]
    fn parse_episode_without_show_degrades_cleanly() {
        let body = r#"{
            "is_playing": true,
            "progress_ms": null,
            "currently_playing_type": "episode",
            "item": {"type": "episode", "name": "Some Episode", "duration_ms": 120000}
        }"#;
        let now = parse_currently_playing_body(body)
            .expect("episode body must parse")
            .expect("episode must still be an item");
        assert_eq!(now.media.title, "Some Episode");
        assert_eq!(now.media.artist, "");
        assert_eq!(now.media.album, "");
        assert_eq!(now.media.progress_ms, None);
        let episode = now.episode.expect("episode metadata is present-but-empty");
        assert_eq!(episode.show_name, "");
        assert_eq!(episode.publisher, "");
    }

    /// A `currently-playing` body for a music track started from a playlist,
    /// with the documented `context` shape (no `display_name`).
    const TRACK_BODY: &str = r#"{
        "device": {"id": "dev2", "is_active": true, "name": "Office PC", "type": "computer"},
        "repeat_state": "off",
        "shuffle_state": false,
        "context": {"type": "playlist", "href": "https://api.spotify.com/v1/playlists/p1", "uri": "spotify:playlist:p1"},
        "progress_ms": 12345,
        "is_playing": true,
        "item": {
            "type": "track",
            "name": "Karma Police",
            "duration_ms": 261000,
            "artists": [{"name": "Radiohead"}],
            "album": {"name": "OK Computer", "images": [{"url": "https://i.scdn.co/image/okc"}]}
        },
        "currently_playing_type": "track"
    }"#;

    #[test]
    fn parse_track_body_maps_artists_album_and_modes() {
        let now = parse_currently_playing_body(TRACK_BODY)
            .expect("track body must parse")
            .expect("track body must yield an item");
        assert!(now.episode.is_none());
        assert_eq!(now.media.title, "Karma Police");
        assert_eq!(now.media.artist, "Radiohead");
        assert_eq!(now.media.album, "OK Computer");
        assert_eq!(now.media.album_art_url, "https://i.scdn.co/image/okc");
        assert_eq!(now.media.progress_ms, Some(12_345));
        assert_eq!(now.context.device, "Office PC");
        assert!(!now.context.shuffle);
        assert_eq!(now.context.repeat, RepeatState::Off);
        // A playlist context has no derivable name (that needs
        // GET /playlists/{id}, i.e. a scope this app does not request), so
        // `{playlist}` renders empty rather than a URI fragment.
        assert_eq!(now.context.playlist, "");
    }

    // `display_name` is honoured when a response does carry it.
    #[test]
    fn context_display_name_prefers_the_response_value() {
        let body = r#"{
            "is_playing": true,
            "currently_playing_type": "track",
            "context": {"type": "playlist", "display_name": "Workout Mix"},
            "item": {"type": "track", "name": "X", "duration_ms": 1000, "artists": [{"name": "A"}]}
        }"#;
        let now = parse_currently_playing_body(body)
            .expect("body must parse")
            .expect("item expected");
        assert_eq!(now.context.playlist, "Workout Mix");
    }

    // Contexts whose name the item already contains are derived from it, so
    // `{playlist}` still renders something useful without an extra request.
    #[test]
    fn context_display_name_derives_from_the_item_when_the_response_omits_it() {
        let album = r#"{
            "is_playing": true,
            "currently_playing_type": "track",
            "context": {"type": "album", "uri": "spotify:album:a1"},
            "item": {"type": "track", "name": "X", "duration_ms": 1000, "artists": [{"name": "Radiohead"}], "album": {"name": "OK Computer"}}
        }"#;
        let now = parse_currently_playing_body(album)
            .expect("body must parse")
            .expect("item expected");
        assert_eq!(now.context.playlist, "OK Computer");

        let artist = r#"{
            "is_playing": true,
            "currently_playing_type": "track",
            "context": {"type": "artist", "uri": "spotify:artist:a-r"},
            "item": {"type": "track", "name": "X", "duration_ms": 1000, "artists": [{"name": "Radiohead"}], "album": {"name": "OK Computer"}}
        }"#;
        let now = parse_currently_playing_body(artist)
            .expect("body must parse")
            .expect("item expected");
        assert_eq!(now.context.playlist, "Radiohead");
    }

    // An ad is never "listening" (issue #161): the envelope type drops it
    // even though the item looks playable.
    #[test]
    fn parse_ad_body_is_nothing_playing() {
        let body = r#"{
            "is_playing": true,
            "progress_ms": 1000,
            "currently_playing_type": "ad",
            "item": {"type": "track", "name": "Sponsor", "duration_ms": 30000, "artists": [{"name": "Ad"}]}
        }"#;
        assert!(parse_currently_playing_body(body)
            .expect("ad body must parse")
            .is_none());
    }

    // Doc guidance for future item types: check the item's own `type` and
    // degrade to "nothing playing" instead of guessing a shape.
    #[test]
    fn parse_unknown_item_type_is_nothing_playing() {
        let body = r#"{
            "is_playing": true,
            "currently_playing_type": "unknown",
            "item": {"type": "hologram", "name": "Future", "duration_ms": 1000}
        }"#;
        assert!(parse_currently_playing_body(body)
            .expect("unknown item body must parse")
            .is_none());
    }

    #[test]
    fn parse_body_without_item_is_nothing_playing() {
        let body = r#"{"is_playing": false, "currently_playing_type": "track"}"#;
        assert!(parse_currently_playing_body(body)
            .expect("empty body parses")
            .is_none());
    }

    #[test]
    fn parse_malformed_body_reports_a_parse_error() {
        let err = parse_currently_playing_body("not json").expect_err("malformed body must fail");
        assert!(
            err.contains("Failed to parse currently playing response"),
            "unexpected error: {err}"
        );
    }

    // Issue #583: the Up Next pane must tell the same story as the status, so
    // queue items go through the same track/episode mapper. Pre-fix every
    // episode in the queue was dropped and the pane read "(queue empty)".
    #[test]
    fn parse_queue_body_maps_episodes_and_drops_ads() {
        let body = r#"{
            "currently_playing": {"type": "track", "name": "Now", "duration_ms": 1000, "artists": [{"name": "A"}]},
            "queue": [
                {"type": "track", "name": "Next Track", "duration_ms": 2000, "artists": [{"name": "B"}], "album": {"name": "Album B", "images": [{"url": "https://i.scdn.co/image/b"}]}},
                {"type": "episode", "name": "Next Episode", "duration_ms": 600000, "images": [{"url": "https://i.scdn.co/image/e"}], "show": {"name": "The Show", "publisher": "Acme Audio"}},
                {"type": "ad", "name": "Sponsor", "duration_ms": 30000}
            ]
        }"#;
        let queue = parse_queue_body(body).expect("queue body must parse");
        assert_eq!(queue.currently_playing.expect("now playing").title, "Now");
        let titles: Vec<&str> = queue.up_next.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, vec!["Next Track", "Next Episode"]);
        // Episodes arrive in the same frozen shape, with the show in the
        // artist slot so the submenu label reads "Show - Episode".
        assert_eq!(queue.up_next[1].artist, "The Show");
        assert_eq!(queue.up_next[1].album_art_url, "https://i.scdn.co/image/e");
    }

    #[test]
    fn parse_queue_body_tolerates_missing_fields() {
        let queue = parse_queue_body("{}").expect("empty body must parse");
        assert!(queue.currently_playing.is_none());
        assert!(queue.up_next.is_empty());
    }

    // Issue #581: the episode template a user's music template must not
    // replace. Pinned as a rendered string, not as the raw constant, so a
    // formatter regression (a token that stopped resolving) fails here.
    #[test]
    fn default_episode_status_format_renders_the_episode() {
        let media = make_track("Episode 12", "The Deep Work Show", "Acme Audio", true);
        let episode = make_episode("The Deep Work Show");
        let result = format_status_with_context(
            &media,
            Some(&episode),
            &PlaybackContext::default(),
            DEFAULT_EPISODE_STATUS_FORMAT,
        );
        assert_eq!(result, "🎙️ The Deep Work Show - Episode 12");
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
        let parsed: SpotifyTokens = serde_json::from_str(&json).expect("deserialize");
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
        let json: serde_json::Value = serde_json::to_value(&track).expect("to_value");
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
        assert_eq!(
            parse_error_reason(r#"{"error":{"status":404}}"#).as_deref(),
            None
        );
    }

    // Regression guard for issue #3.0-P3: only a 404 whose error body
    // carries `reason: "NO_ACTIVE_DEVICE"` maps to NoActiveDevice — other
    // reasons, other statuses, and non-JSON bodies must not.
    #[test]
    fn is_no_active_device_404_matches_only_no_active_device() {
        let no_active = r#"{"error":{"status":404,"message":"Player command failed: No active device found","reason":"NO_ACTIVE_DEVICE"}}"#;
        assert!(is_no_active_device_404(404, no_active));
        let other_reason =
            r#"{"error":{"status":404,"message":"Device not found","reason":"DEVICE_NOT_FOUND"}}"#;
        assert!(!is_no_active_device_404(404, other_reason));
        let wrong_status =
            r#"{"error":{"status":403,"message":"Forbidden","reason":"NO_ACTIVE_DEVICE"}}"#;
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
        let no_scope =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"sub":"user123"}"#);
        assert!(decode_spotify_granted_scopes(&format!("h.{}.s", no_scope)).is_empty());
    }

    #[test]
    fn parse_retry_after_value_handles_delta_seconds_and_http_date() {
        // Plain delta-seconds still primary.
        assert_eq!(super::parse_retry_after_value("120"), Some(120));
        assert_eq!(super::parse_retry_after_value("  42  "), Some(42));
        // Capped at 300.
        assert_eq!(super::parse_retry_after_value("9999"), Some(300));
        // HTTP-date ~60s in future -> small positive delay, not None.
        let future = std::time::SystemTime::now() + std::time::Duration::from_secs(60);
        let http_date = httpdate::fmt_http_date(future);
        let secs = super::parse_retry_after_value(&http_date).expect("http-date must parse");
        assert!(secs <= 60, "future http-date ~60s got {}", secs);
        // Far-future HTTP-date capped at 300.
        let far_future = std::time::SystemTime::now() + std::time::Duration::from_secs(10_000);
        let far_date = httpdate::fmt_http_date(far_future);
        assert_eq!(super::parse_retry_after_value(&far_date), Some(300));
        // Past HTTP-date -> 0 (max(0, date-now)).
        let past = std::time::SystemTime::now() - std::time::Duration::from_secs(60);
        let past_date = httpdate::fmt_http_date(past);
        assert_eq!(super::parse_retry_after_value(&past_date), Some(0));
        // Unparseable stays None (callers fall back to exponential backoff).
        assert_eq!(super::parse_retry_after_value("not-a-date"), None);
        assert_eq!(super::parse_retry_after_value(""), None);
    }

    // Issue #350: a token response without `refresh_token` must yield the
    // precise `omitted refresh_token` error, not a generic parse failure;
    // a response carrying it must store it.
    #[test]
    fn exchange_without_refresh_token_yields_precise_error() {
        let body = r#"{"access_token":"at","expires_in":3600,"token_type":"Bearer"}"#;
        let err =
            super::parse_exchange_token_response(body).expect_err("missing refresh must fail");
        assert!(
            err.contains("token response omitted refresh_token"),
            "precise error expected, got: {}",
            err
        );
        assert!(
            !err.contains("Failed to parse token response"),
            "must not be the generic parse error, got: {}",
            err
        );
    }

    #[test]
    fn exchange_with_refresh_token_stores_it() {
        let body =
            r#"{"access_token":"at","refresh_token":"rt","expires_in":3600,"token_type":"Bearer"}"#;
        let tokens = super::parse_exchange_token_response(body).expect("full body must parse");
        assert_eq!(tokens.access_token, "at");
        assert_eq!(tokens.refresh_token, "rt");
    }

    // Issues #444/#446/#450: every accounts.spotify.com token request must
    // go through `build_spotify_client` (10s timeout + PresenceJam UA), so
    // re-adding a bare `Client::new()` token request fails the suite. The
    // builder body is isolated with the shared literal-aware scanner
    // (`crate::token_io::test_scan`), not a next-function boundary anchor.
    #[test]
    fn token_requests_go_through_shared_client_builder() {
        let src = include_str!("spotify.rs");
        let prod = src
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("spotify.rs has no #[cfg(test)] mod tests block");
        // Strip line/doc comments: the builder's own docs name the
        // historical bare `Client::new()` (issue #347), which must not trip
        // the guard — only live code counts.
        let code_lines: Vec<&str> = prod
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect();
        let code = code_lines.join("\n");
        assert!(
            !code.contains("Client::new()"),
            "prod code must not build a bare Client::new(): all token requests go through build_spotify_client (issue #444)"
        );
        let token_posts = prod.matches("accounts.spotify.com").count();
        assert_eq!(
            token_posts, 2,
            "expected exactly the exchange + refresh token posts, got {}",
            token_posts
        );
        let builder_uses = prod.matches("build_spotify_client()").count();
        assert!(
            builder_uses >= token_posts,
            "every accounts.spotify.com request must go through build_spotify_client: {} posts but {} builder uses",
            token_posts,
            builder_uses
        );
        let body = crate::token_io::test_scan::fn_body(src, "fn build_spotify_client(");
        assert!(
            body.contains("Duration::from_secs(10)"),
            "builder must set a 10s timeout (issue #444)"
        );
        assert!(
            body.contains("user_agent"),
            "builder must set a User-Agent (issue #450)"
        );
        assert!(
            body.contains("PresenceJam/"),
            "builder User-Agent must be PresenceJam/<version> (issue #450)"
        );
        assert!(
            body.contains("CARGO_PKG_VERSION"),
            "builder User-Agent version must track Cargo.toml via env! (issue #450)"
        );
    }

    // Issue #576: one Spotify client — and therefore one connection pool —
    // per process. `reqwest::blocking::Client` exposes no handle identity and
    // pooling lives behind a background runtime, so a behavioural assertion
    // would need a live keep-alive server; the invariant is pinned the way
    // this module already pins builder *configuration* (see
    // `token_requests_go_through_shared_client_builder`): against the
    // production source. Regression this defends: a fresh `Client::builder()
    // .build()` per call — the pre-#576 shape, which paid a new TCP+TLS
    // handshake on every poll.
    #[test]
    fn build_spotify_client_is_memoized_per_process() {
        let src = include_str!("spotify.rs");
        let body = crate::token_io::test_scan::fn_body(src, "fn build_spotify_client(");
        assert!(
            body.contains("static CLIENT"),
            "build_spotify_client must memoize its client in a process-wide static (issue #576)"
        );
        assert!(
            body.contains("LazyLock") || body.contains("OnceLock"),
            "the cached client must live in a std sync cell (issue #576)"
        );
        assert_eq!(
            body.matches("Client::builder()").count(),
            1,
            "the client must be built exactly once (inside the cache initializer), not per call (issue #576)"
        );
        assert!(
            body.contains(".map(|c| c.clone())"),
            "callers must receive a refcount-bumped clone of the one cached client (issue #576)"
        );
    }

    // Issue #930: the poll thread and every tray/playback command refreshed
    // independently, so a tray click landing during a poller refresh put two
    // token POSTs carrying the same refresh token in flight at once.
    // `refresh_serialized` is the lock + cache core of `refresh_spotify_token`
    // with the POST injected, so the contract — one request per burst, and the
    // callers that waited get the fresh token — is asserted without a live
    // token endpoint. The count is interleaving-independent: whichever thread
    // takes the lock first does the one fetch and the other reads the cache.
    #[test]
    fn concurrent_refreshes_of_the_same_token_make_exactly_one_request() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Barrier};

        let calls = Arc::new(AtomicUsize::new(0));
        let start = Arc::new(Barrier::new(2));
        let now = Utc::now();

        let spawn = |n: usize| {
            let calls = Arc::clone(&calls);
            let start = Arc::clone(&start);
            std::thread::spawn(move || {
                start.wait();
                refresh_serialized("refresh-930-concurrent", Utc::now(), || {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(SpotifyTokens {
                        access_token: format!("access-930-{}", n),
                        refresh_token: "refresh-930-concurrent".to_string(),
                        expires_at: now + chrono::Duration::hours(1),
                    })
                })
                .expect("the stub fetch cannot fail")
            })
        };

        let a = spawn(1);
        let b = spawn(2);
        let a = a.join().expect("refresh thread a must not panic");
        let b = b.join().expect("refresh thread b must not panic");

        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "two concurrent refreshes of the same token must produce one token request (issue #930)"
        );
        assert_eq!(
            a.access_token, b.access_token,
            "the caller that waited on the lock must receive the token the winner fetched"
        );
    }

    // The cache is keyed by the refresh token: a *different* credential must
    // never be served the cached pair, or a re-auth (or account switch) would
    // hand the caller the previous session's access token.
    #[test]
    fn refresh_cache_does_not_serve_a_different_refresh_token() {
        let now = Utc::now();
        let mut calls = 0usize;
        for (key, expected) in [("refresh-930-a", "access-930-a"), ("refresh-930-b", "access-930-b")]
        {
            let tokens = refresh_serialized(key, now, || {
                calls += 1;
                Ok(SpotifyTokens {
                    access_token: expected.to_string(),
                    refresh_token: key.to_string(),
                    expires_at: now + chrono::Duration::hours(1),
                })
            })
            .expect("the stub fetch cannot fail");
            assert_eq!(tokens.access_token, expected);
        }
        assert_eq!(
            calls, 2,
            "a second refresh token must not be served the first one's cached access token (issue #930)"
        );
    }

    // The cache is a convenience, not a source of truth: once the stored access
    // token has expired the next caller must POST again.
    #[test]
    fn refresh_cache_is_not_used_once_the_stored_token_expires() {
        let now = Utc::now();
        let mut calls = 0usize;
        let stale = refresh_serialized("refresh-930-stale", now, || {
            calls += 1;
            Ok(SpotifyTokens {
                access_token: "stale-930".to_string(),
                refresh_token: "refresh-930-stale".to_string(),
                expires_at: now - chrono::Duration::seconds(5),
            })
        })
        .expect("the stub fetch cannot fail");
        assert_eq!(stale.access_token, "stale-930");

        let fresh = refresh_serialized("refresh-930-stale", now, || {
            calls += 1;
            Ok(SpotifyTokens {
                access_token: "fresh-930".to_string(),
                refresh_token: "refresh-930-stale".to_string(),
                expires_at: now + chrono::Duration::hours(1),
            })
        })
        .expect("the stub fetch cannot fail");

        assert_eq!(
            fresh.access_token, "fresh-930",
            "an expired cached token must not be handed to a later caller"
        );
        assert_eq!(calls, 2, "the expired entry must not suppress the request");
    }

    // Issue #931: `expires_in` is server-controlled and was cast straight
    // through `as i64` into `chrono::Duration::seconds`, which panics for
    // absurd lifetimes and wraps negative for values that overflow the cast —
    // the wrap stored an `expires_at` in the past, so the token was expired the
    // moment it was saved and every poll iteration refreshed again.
    #[test]
    fn token_expiry_clamps_a_malformed_expires_in_to_a_sane_future_instant() {
        let now = Utc::now();
        let floor = now + chrono::Duration::seconds(MIN_TOKEN_LIFETIME_SECS as i64);
        let ceiling = now + chrono::Duration::seconds(MAX_TOKEN_LIFETIME_SECS as i64);

        for (expires_in, label) in [
            (u64::MAX, "u64::MAX panicked the old cast"),
            (0, "zero must not expire instantly"),
            (1, "below the floor"),
            (u64::MAX / 1000 + 1, "the old panic threshold"),
            (MAX_TOKEN_LIFETIME_SECS * 2, "above the ceiling"),
        ] {
            let expires_at = token_expiry(expires_in, now);
            assert!(
                expires_at >= floor,
                "expires_in={} ({}) must expire no sooner than the floor, got {}",
                expires_in,
                label,
                expires_at
            );
            assert!(
                expires_at <= ceiling,
                "expires_in={} ({}) must expire no later than the ceiling, got {}",
                expires_in,
                label,
                expires_at
            );
        }

        assert_eq!(
            token_expiry(3600, now),
            now + chrono::Duration::hours(1),
            "the lifetime Spotify documents must pass through unchanged"
        );
    }

    // Same contract through the real parse path: a hostile or broken token
    // endpoint answering with a huge `expires_in` must leave the app with a
    // usable token, not a panic and not an already-expired one.
    #[test]
    fn exchange_parse_survives_a_malformed_expires_in() {
        let body = r#"{"access_token":"at","refresh_token":"rt","token_type":"Bearer","expires_in":18446744073709551615}"#;
        let tokens =
            parse_exchange_token_response(body).expect("a huge expires_in must still parse");
        assert!(
            tokens.expires_at > Utc::now(),
            "the stored token must not already be expired, got {}",
            tokens.expires_at
        );
        assert!(
            tokens.expires_at <= Utc::now() + chrono::Duration::seconds(MAX_TOKEN_LIFETIME_SECS as i64 + 1),
            "the stored expiry must stay inside the accepted range, got {}",
            tokens.expires_at
        );
    }

    // Issue #749: `Other(String)` could not tell a retryable 5xx from a
    // permanent 4xx, and 403 mapped to `NotPremium` on the player commands
    // while the currently-playing GET sent the same status into the generic
    // body arm — one status, two messages. `classify_spotify_status` is the pure
    // mapping every endpoint now shares.
    #[test]
    fn classify_spotify_status_selects_the_variant_for_each_status() {
        const NO_DEVICE: &str = r#"{"error":{"status":404,"reason":"NO_ACTIVE_DEVICE"}}"#;
        const OTHER_404: &str = r#"{"error":{"status":404,"reason":"NOT_FOUND"}}"#;
        let err = |status, retry_after, body| {
            classify_spotify_status(status, retry_after, "play", body)
        };

        assert!(matches!(err(401, None, "{}"), SpotifyApiError::ExpiredToken));
        assert!(matches!(err(403, None, "{}"), SpotifyApiError::NotPremium));
        assert!(matches!(
            err(429, Some(7), "{}"),
            SpotifyApiError::RateLimited(Some(7))
        ));
        assert!(matches!(
            err(429, None, "{}"),
            SpotifyApiError::RateLimited(None)
        ));
        assert!(matches!(
            err(404, None, NO_DEVICE),
            SpotifyApiError::NoActiveDevice
        ));
        assert!(matches!(
            err(503, None, "{}"),
            SpotifyApiError::Transient { status: 503, .. }
        ));
        assert!(matches!(
            err(500, None, "{}"),
            SpotifyApiError::Transient { status: 500, .. }
        ));
        assert!(matches!(
            err(400, None, "{}"),
            SpotifyApiError::Http { status: 400, .. }
        ));
        assert!(
            matches!(
                err(404, None, OTHER_404),
                SpotifyApiError::Http { status: 404, .. }
            ),
            "a 404 without NO_ACTIVE_DEVICE is not a device problem"
        );
    }

    // The distinction a 5xx/4xx-blind `Other(String)` could not express.
    #[test]
    fn a_5xx_is_distinguishable_from_a_4xx_at_the_type_level() {
        let transient = classify_spotify_status(500, None, "play", "upstream boom");
        let permanent = classify_spotify_status(400, None, "play", "upstream boom");

        assert!(matches!(transient, SpotifyApiError::Transient { .. }));
        assert!(!matches!(permanent, SpotifyApiError::Transient { .. }));
        assert!(transient.to_string().contains("500"), "got {}", transient);
        assert!(permanent.to_string().contains("400"), "got {}", permanent);
        assert_ne!(transient.to_string(), permanent.to_string());
    }

    // Acceptance criterion for issue #749: the same status must yield the same
    // variant and the same user-facing message regardless of which endpoint
    // produced it. The player commands and the currently-playing GET both map
    // through `map_player_error` now, so their contexts differ while the
    // message does not.
    #[test]
    fn a_403_reads_the_same_on_a_player_command_and_on_currently_playing() {
        let player = classify_spotify_status(403, None, "pause", "{}");
        let currently_playing = classify_spotify_status(403, None, "Currently playing", "{}");

        assert!(matches!(player, SpotifyApiError::NotPremium));
        assert!(matches!(currently_playing, SpotifyApiError::NotPremium));
        assert_eq!(player.to_string(), currently_playing.to_string());
        assert_eq!(player.to_string(), "Playback control requires Spotify Premium");
    }

    // Issue #796's contract, pinned at the type's own boundary: whatever the
    // endpoint, an unclassified status names the HTTP status and never leaks the
    // response body into text the UI renders.
    #[test]
    fn unclassified_statuses_never_leak_the_response_body_into_display() {
        const BODY: &str = "<html>edge refused: request blocked by CDN rule 12345</html>";

        for status in [400u16, 404, 500, 503] {
            let text = classify_spotify_status(status, None, "Currently playing", BODY).to_string();
            assert!(
                text.contains(&status.to_string()),
                "the message must name the status, got {}",
                text
            );
            assert!(
                !text.contains("CDN rule") && !text.contains("<html>") && !text.contains(BODY),
                "the response body must not reach user-facing text, got {}",
                text
            );
        }
    }
}
