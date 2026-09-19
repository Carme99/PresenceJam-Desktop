use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use std::thread;
use std::time::Duration as StdDuration;

/// Log tag prefix for this module (mirrors the `[CFG]` / `[CMD.*]` /
/// `[UPDATER.BG]` pattern). `CLAUDE.md` requires a square-bracket module
/// tag on every log line; `teams::fn_name:` prefixes are kept where they
/// aid diagnosis, but never in place of the tag. Issue #777.
const TAG: &str = "[TEAMS]";

pub const MICROSOFT_GRAPH_CLIENT_ID: &str = "14d82eec-204b-4c2f-b7e8-296a70dab67e";
/// OAuth scopes requested during the Teams device-code flow. The
/// `Calendars.ReadBasic` (issue #867) and `MailboxSettings.Read` (issue #876)
/// scopes are added on top of the presence set; both force a one-time Teams
/// re-consent. Both are *fail-open* in this app — a denied grant reproduces
/// the pre-feature behaviour exactly (no calendar pre-gate, no working-hours
/// import) so a tenant that refuses them never sees an error.
pub const MICROSOFT_GRAPH_SCOPES: &str = "Presence.ReadWrite Presence.Read Calendars.ReadBasic MailboxSettings.Read openid profile offline_access";

/// Truncates a string for safe logging. Returns the body unchanged if it
/// fits in 256 chars; otherwise returns the first 256 chars (cut at a
/// UTF-8 char boundary) plus `(…NB total)` where NB is the byte count.
///
/// Prevents large credential blobs (Microsoft Graph access_token +
/// refresh_token, ~3.5KB, ~77min lifetime) from being written to log
/// files at `debug!` level — see issue #62.
fn truncate_for_log(body: &str) -> String {
    if body.chars().count() > 256 {
        // Find the byte index of the 256th char (char-boundary-safe).
        let cut = body
            .char_indices()
            .nth(256)
            .map(|(i, _)| i)
            .unwrap_or(body.len());
        format!("{}(…{} total)", &body[..cut], body.len())
    } else {
        body.to_string()
    }
}

/// Error type for Teams API operations.
/// Distinguishes permanent auth failures (expired/invalid token, revoked
/// grant, insufficient permission/license) from transient errors (rate
/// limiting, server errors, network failures).
#[derive(Debug, Clone)]
pub enum TeamsApiError {
    /// Token is expired or invalid (401) — requires re-auth
    ExpiredToken(u16),
    /// Access denied (403) — permission/license problem; re-auth won't
    /// help. Carries the response body so `insufficient_claims` (and
    /// other Graph error details) are detectable.
    Forbidden(u16, String),
    /// Rate limited (429) — transient, retry after the parsed
    /// `Retry-After` seconds (None when the header is absent or
    /// unparseable → fall back to exponential backoff).
    RateLimited(Option<u64>),
    /// Refresh token is missing/invalid/revoked (token-endpoint 400
    /// `invalid_grant`) — permanent, re-auth required.
    InvalidGrant,
    /// Network error or other transient failure (5xx, send failure)
    Transient(String),
    /// Other non-retryable error
    Other(u16, String),
}

impl std::fmt::Display for TeamsApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TeamsApiError::ExpiredToken(status) => {
                write!(f, "Access token expired or invalid (HTTP {})", status)
            }
            TeamsApiError::Forbidden(_status, body) => write!(f, "{}", body),
            TeamsApiError::RateLimited(retry_after) => match retry_after {
                Some(secs) => write!(f, "Rate limited (retry after {}s)", secs),
                None => write!(f, "Rate limited"),
            },
            TeamsApiError::InvalidGrant => write!(f, "Refresh token is invalid or revoked"),
            TeamsApiError::Transient(msg) => write!(f, "{}", msg),
            TeamsApiError::Other(_status, body) => write!(f, "{}", body),
        }
    }
}

impl TeamsApiError {
    /// One actionable sentence for the UI (issue #974).
    ///
    /// `Display` stays the raw form for LOGS — bodies included, which is what
    /// makes a rejected Graph call diagnosable — while this is what the
    /// Dashboard banner, Onboarding, Reconnect and Settings show. Each variant
    /// names what the user can do about it: a Graph 403 in particular is a
    /// permission/licence problem that re-authenticating cannot fix, so the
    /// generic "try again" text would send the user round a loop.
    pub fn user_message(&self) -> String {
        match self {
            TeamsApiError::ExpiredToken(_) | TeamsApiError::InvalidGrant => {
                "Your Microsoft Teams sign-in has expired. Reconnect Teams in Settings.".to_string()
            }
            TeamsApiError::Forbidden(_, _) => "Microsoft Teams refused the request: the account may be missing the Teams presence permission (Presence.ReadWrite) or a Microsoft 365 licence that includes Teams. Reconnecting will not fix this — check the account's licence and admin consent.".to_string(),
            TeamsApiError::RateLimited(Some(secs)) => format!(
                "Microsoft Teams is temporarily limiting requests. Retrying in {} seconds.",
                secs
            ),
            TeamsApiError::RateLimited(None) => {
                "Microsoft Teams is temporarily limiting requests. Retrying shortly.".to_string()
            }
            TeamsApiError::Transient(_) => {
                "Microsoft Teams is temporarily unavailable. Retrying shortly.".to_string()
            }
            TeamsApiError::Other(status, _) => format!(
                "Microsoft Teams returned an unexpected error (HTTP {}).",
                status
            ),
        }
    }
}

/// Creates a reqwest blocking client with standard config (user agent +
/// `timeout`). Ensures consistent HTTP client settings across all Teams API
/// calls.
///
/// User-Agent uses `env!("CARGO_PKG_VERSION")` so it tracks `Cargo.toml`
/// (which mirrors `tauri.conf.json` → `version`) automatically on every
/// release. Never hardcode the version — see CONTRIBUTING.md. See audit
/// Q8.
///
/// Only the exit-path cleanup still calls this (issue #884): it is the one
/// caller that needs a timeout other than the shared 10 s, and caching a
/// client per timeout value would defeat the point of the cache.
fn build_teams_client_with_timeout(
    timeout: std::time::Duration,
) -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent(format!("PresenceJam/{}", env!("CARGO_PKG_VERSION")))
        .timeout(timeout)
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))
}

/// The shared Graph client: built once per process and cached (issue #884).
///
/// `reqwest::blocking::Client` is `Arc`-backed, so every later call is a
/// refcount bump over the SAME connection pool instead of a fresh pool per
/// call — the memoization `spotify.rs::build_spotify_client` already has
/// (#576). Eight Teams call sites used to open and discard a pool each: a
/// presence read plus a status POST every poll, i.e. a new TCP+TLS handshake
/// per iteration with no keep-alive reuse.
///
/// The cache memoizes a failed build too: the builder fails only on
/// environmental TLS/runtime init, where a retry would fail identically. The
/// signature is `Result<Client, String>` as before, so the call sites and
/// their error mapping are untouched.
///
/// `pub` for the calendar integration (issue #867) — the calendar's
/// `list_upcoming` shares the same TLS pool so a long-running polling
/// thread doesn't open a fresh connection every fetch.
pub fn build_teams_client() -> Result<reqwest::blocking::Client, String> {
    static CLIENT: LazyLock<Result<reqwest::blocking::Client, String>> = LazyLock::new(|| {
        reqwest::blocking::Client::builder()
            .user_agent(format!("PresenceJam/{}", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))
    });
    CLIENT.as_ref().map(|c| c.clone()).map_err(|e| e.clone())
}

/// Binding budget for the exit-path cleanup (finding #636, issue #636).
///
/// The two best-effort calls run inside `RunEvent::Exit`, so they must never
/// hold the quit open: a dead network costs seconds, not the default 10 s per
/// call.
pub const EXIT_CLEANUP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct TeamsTokens {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    pub expires_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct DeviceCodeResponse {
    pub user_code: String,
    pub verification_url: String,
    pub device_code: String,
    // Tauri IPC crosses the boundary via serde_json, which decodes u64
    // values as JS `number` (f64). Override ts-rs's `bigint` default so
    // the generated `.ts` matches what `invoke()` actually returns at
    // runtime. The OAuth interval/expires-in values are always small,
    // well under 2^53, so no precision is lost in practice.
    #[ts(type = "number")]
    pub interval: u64,
    #[ts(type = "number")]
    pub expires_in: u64,
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponseRaw {
    user_code: String,
    #[serde(alias = "verification_url")]
    verification_uri: String,
    device_code: String,
    interval: u64,
    expires_in: u64,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    expires_in: u64,
}

#[derive(Debug, Deserialize)]
struct TokenErrorResponse {
    error: String,
    error_description: Option<String>,
}

pub fn start_teams_auth_device_code() -> Result<DeviceCodeResponse, String> {
    log::info!("{TAG} start_teams_auth_device_code: starting");

    let client = build_teams_client()?;
    log::info!("{TAG} start_teams_auth_device_code: client created");

    let params = [
        ("client_id", MICROSOFT_GRAPH_CLIENT_ID),
        // `offline_access` is required for Microsoft to issue a
        // refresh_token (device-code flow docs). `Presence.Read` powers the
        // presence-aware status gate (getPresence, issue #3.0-P2).
        // `profile` adds the `oid` claim to the access-token JWT so the
        // setPresence/clearPresence /users/{oid} fallback can resolve the
        // user (docs list only /users/{id}; see issue #3.0-P1), and Microsoft
        // requires `openid` whenever `profile` is requested. `User.Read`
        // stays dropped: no Graph call uses it (least privilege, #151).
        ("scope", MICROSOFT_GRAPH_SCOPES),
    ];
    log::info!("{TAG} start_teams_auth_device_code: calling devicecode endpoint");

    let response = client
        .post("https://login.microsoftonline.com/common/oauth2/v2.0/devicecode")
        .header("Accept", "application/json")
        .form(&params)
        .send()
        .map_err(|e| {
            log::error!("{TAG} start_teams_auth_device_code: send failed: {}", e);
            format!("Failed to send device code request: {}", e)
        })?;
    log::info!("{TAG} start_teams_auth_device_code: send succeeded");

    let status = response.status();
    log::info!(
        "{TAG} start_teams_auth_device_code: response status: {}",
        status
    );

    let raw_body = response.text().map_err(|e| {
        log::error!(
            "{TAG} start_teams_auth_device_code: failed to read body: {}",
            e
        );
        format!("Failed to read response body: {}", e)
    })?;

    if !status.is_success() {
        // Bearer hygiene (issue #348): the body may carry the
        // `device_code` bearer credential, and this Err is error-logged
        // by the caller — report the body length, never its content.
        return Err(format!(
            "Device code request failed with status {} ({}-byte body)",
            status,
            raw_body.len()
        ));
    }

    let raw: DeviceCodeResponseRaw = serde_json::from_str(&raw_body).map_err(|e| {
        format!(
            "Failed to parse device code response: {} ({}-byte body)",
            e,
            raw_body.len()
        )
    })?;
    log::info!(
        "{TAG} start_teams_auth_device_code: received (expires_in={}s, interval={}s)",
        raw.expires_in,
        raw.interval
    );

    let result = DeviceCodeResponse {
        user_code: raw.user_code,
        verification_url: raw.verification_uri,
        device_code: raw.device_code.clone(),
        interval: raw.interval,
        expires_in: raw.expires_in,
    };

    log::info!(
        "{TAG} Device code flow started. User code: {}, verification URL: {}",
        result.user_code,
        result.verification_url
    );

    Ok(result)
}

/// True iff the Teams access token has less than 60 seconds of lifetime
/// remaining. Mirrors `spotify::is_token_expired` so the two providers
/// share the same refresh-window heuristic. See audit PR-3 nit.
pub fn is_token_expired(tokens: &TeamsTokens) -> bool {
    Utc::now() >= tokens.expires_at - chrono::Duration::seconds(60)
}

/// Computes the next polling wait in seconds per RFC 8628 §3.5.
///
/// A `slow_down` error carries no interval of its own; the client MUST
/// increase its polling interval by 5 seconds for this and all
/// subsequent requests. Any other error keeps the current interval.
fn next_poll_wait(current: u64, err: &str) -> u64 {
    if err == "slow_down" {
        current + 5
    } else {
        current
    }
}

/// Parses a `Retry-After` header into an optional delay, supporting both
/// delta-seconds (`120`) and HTTP-date (`Wed, 21 Aug 2026 12:00:00 GMT`)
/// forms per RFC 7231 §7.1.3. Returns None when the header is absent
/// or unparseable — callers then fall back to exponential backoff.
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

/// Failure message for the overall device-code deadline.
const AUTH_TIMEOUT_MSG: &str = "Authentication timed out";

/// What [`poll_teams_auth`] does with one token-endpoint response.
#[derive(Debug, PartialEq, Eq)]
enum PollAction {
    /// Poll again — after `server_wait` seconds when the server sent a
    /// `Retry-After`, else after the loop's current interval.
    Retry {
        server_wait: Option<u64>,
        slow_down: bool,
    },
    /// Stop polling; the string is the user-facing sentence.
    Fail(String),
}

/// Pure decision for one device-code token response (issue #797).
///
/// The poll used to parse every non-success body as the OAuth error envelope
/// with `?`, so a gateway 502 HTML page — or a 429 without the JSON error
/// object — ended sign-in on a serde error the user could not act on, and the
/// whole multi-minute consent step had to be redone. HTTP-level blips (5xx,
/// 429, 408) and any body that is not the OAuth error envelope are now
/// retries, bounded by the caller's overall deadline; only a real verdict on
/// the device code ends the flow. Split out from the network loop so the whole
/// matrix is unit-testable without HTTP.
fn classify_device_code_response(status: u16, retry_after: Option<u64>, body: &str) -> PollAction {
    if (500..=599).contains(&status) || status == 429 || status == 408 {
        return PollAction::Retry {
            server_wait: retry_after,
            slow_down: false,
        };
    }
    let Ok(error_resp) = serde_json::from_str::<TokenErrorResponse>(body) else {
        // An interposed proxy page, an HTML error page or an empty body is not
        // a verdict on the device code: keep the code usable and retry.
        return PollAction::Retry {
            server_wait: retry_after,
            slow_down: false,
        };
    };
    match error_resp.error.as_str() {
        "authorization_pending" => PollAction::Retry {
            server_wait: None,
            slow_down: false,
        },
        "slow_down" => PollAction::Retry {
            server_wait: None,
            slow_down: true,
        },
        "authorization_declined" => {
            PollAction::Fail("Microsoft sign-in was declined in the browser.".to_string())
        }
        "expired_token" => PollAction::Fail(
            "The Microsoft sign-in code expired. Please start sign-in again.".to_string(),
        ),
        // Every other OAuth error code (bad_verification_code,
        // unauthorized_client, …) is terminal for this code. The server's own
        // description is the best actionable text; the raw body is logged by
        // the caller and never shown (issue #974).
        _ => {
            let detail = error_resp
                .error_description
                .as_deref()
                .map(str::trim)
                .filter(|d| !d.is_empty())
                .unwrap_or(error_resp.error.as_str());
            PollAction::Fail(format!("Microsoft sign-in failed: {}", detail))
        }
    }
}

/// Sleeps up to `secs` seconds in 30-second chunks, giving up as soon as the
/// overall device-code deadline passes — so a server-directed `Retry-After`
/// (already clamped to 300 s by [`parse_retry_after_value`]) or a ramped
/// `slow_down` interval can never block the thread past the deadline.
fn sleep_within_deadline(
    start_time: std::time::Instant,
    timeout: StdDuration,
    secs: u64,
) -> Result<(), String> {
    let mut remaining = secs;
    while remaining > 0 {
        if start_time.elapsed() > timeout {
            return Err(AUTH_TIMEOUT_MSG.to_string());
        }
        let chunk = remaining.min(30);
        thread::sleep(StdDuration::from_secs(chunk));
        remaining -= chunk;
    }
    Ok(())
}

pub fn poll_teams_auth(device_code: &str, interval: u64) -> Result<TeamsTokens, String> {
    let client = build_teams_client()?;
    let start_time = std::time::Instant::now();
    let timeout = StdDuration::from_secs(900);

    // RFC 8628 §3.5 + security: server-provided interval is untrusted
    // (devtools could inject u64::MAX). Clamp to a sane range so
    // thread::sleep cannot block the thread for hours.
    let interval = interval.clamp(1, 15);
    let mut wait = interval;

    loop {
        if start_time.elapsed() > timeout {
            return Err(AUTH_TIMEOUT_MSG.to_string());
        }

        let params = [
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("client_id", MICROSOFT_GRAPH_CLIENT_ID),
            ("device_code", device_code),
        ];

        let response = client
            .post("https://login.microsoftonline.com/common/oauth2/v2.0/token")
            .header("Accept", "application/json")
            .form(&params)
            .send()
            .map_err(|e| format!("Failed to send token request: {}", e))?;

        let status = response.status();
        // Read the server's directive before `text()` consumes the response
        // (issue #797): on a throttle it wins over the loop's current interval.
        let retry_after = parse_retry_after(&response);

        let raw_body = response
            .text()
            .map_err(|e| format!("Failed to read response body: {}", e))?;
        log::debug!(
            "{TAG} poll_teams_auth: status={}, body={}",
            status,
            truncate_for_log(&raw_body)
        );

        if status.is_success() {
            let token_resp: TokenResponse = match serde_json::from_str(&raw_body) {
                Ok(parsed) => parsed,
                Err(e) => {
                    // A 2xx body that is not the token envelope is an
                    // interposed proxy/captive-portal page rather than a
                    // verdict on the device code (issue #797): retry within
                    // the deadline instead of ending sign-in on a serde error.
                    log::error!(
                        "{TAG} poll_teams_auth: unparseable 2xx body: {} ({}-byte body)",
                        e,
                        raw_body.len()
                    );
                    sleep_within_deadline(start_time, timeout, wait)?;
                    continue;
                }
            };

            let expires_at =
                chrono::Utc::now() + chrono::Duration::seconds(token_resp.expires_in as i64);

            log::info!("{TAG} Successfully authenticated with Microsoft Teams");

            return Ok(TeamsTokens {
                access_token: token_resp.access_token,
                refresh_token: token_resp.refresh_token,
                expires_at,
            });
        }

        match classify_device_code_response(status.as_u16(), retry_after, &raw_body) {
            PollAction::Fail(message) => {
                // The response body is already logged, truncated, by the
                // `status={}, body={}` debug line above; the user gets the
                // sentence only — never the raw JSON (issues #797/#974).
                log::error!("{TAG} poll_teams_auth: {}", message);
                return Err(message);
            }
            PollAction::Retry {
                server_wait,
                slow_down,
            } => {
                if slow_down {
                    // RFC 8628 §3.5: slow_down carries no interval of its own;
                    // the client must increase its polling interval by 5s for
                    // this and all subsequent requests.
                    wait = next_poll_wait(wait, "slow_down");
                    log::warn!("{TAG} Server requested slow down, waiting {} seconds", wait);
                } else if server_wait.is_none() {
                    log::debug!("{TAG} Authorization pending, waiting {} seconds", wait);
                } else {
                    log::warn!(
                        "{TAG} poll_teams_auth: status {} throttled, waiting up to {} seconds",
                        status,
                        server_wait.unwrap_or_default()
                    );
                }
                let sleep_secs = server_wait.unwrap_or(wait).max(1);
                sleep_within_deadline(start_time, timeout, sleep_secs)?;
                continue;
            }
        }
    }
}

pub fn refresh_teams_token(tokens: &TeamsTokens) -> Result<TeamsTokens, TeamsApiError> {
    let refresh_token = tokens
        .refresh_token
        .as_ref()
        .ok_or(TeamsApiError::InvalidGrant)?;

    let client = build_teams_client().map_err(TeamsApiError::Transient)?;

    let params = [
        ("grant_type", "refresh_token"),
        ("client_id", MICROSOFT_GRAPH_CLIENT_ID),
        ("refresh_token", refresh_token.as_str()),
    ];

    let response = client
        .post("https://login.microsoftonline.com/common/oauth2/v2.0/token")
        .header("Accept", "application/json")
        .form(&params)
        .send()
        .map_err(|e| {
            TeamsApiError::Transient(format!("Failed to send refresh token request: {}", e))
        })?;

    let status = response.status();
    let status_code = status.as_u16();
    let retry_after = parse_retry_after(&response);
    let raw_body = response
        .text()
        .map_err(|e| TeamsApiError::Transient(format!("Failed to read response body: {}", e)))?;

    if !status.is_success() {
        log::error!(
            "{TAG} refresh_teams_token: refresh request failed with status {}: {}",
            status,
            truncate_for_log(&raw_body)
        );
        if status_code == 400 {
            // invalid_grant means the refresh token itself is dead —
            // permanent, re-auth required. Any other 400 body is an
            // "Other" non-retryable error.
            let error_resp: TokenErrorResponse = serde_json::from_str(&raw_body).map_err(|e| {
                TeamsApiError::Other(
                    status_code,
                    format!(
                        "Failed to parse error response: {} (body was: {})",
                        e,
                        truncate_for_log(&raw_body)
                    ),
                )
            })?;
            if error_resp.error == "invalid_grant" {
                return Err(TeamsApiError::InvalidGrant);
            }
            return Err(TeamsApiError::Other(
                status_code,
                format!(
                    "{} - {}",
                    error_resp.error,
                    error_resp.error_description.unwrap_or_default()
                ),
            ));
        }
        return Err(match status_code {
            429 => TeamsApiError::RateLimited(retry_after),
            500..=599 => TeamsApiError::Transient(format!(
                "server error {}: {}",
                status_code,
                truncate_for_log(&raw_body)
            )),
            _ => TeamsApiError::Other(status_code, truncate_for_log(&raw_body)),
        });
    }

    let token_resp: TokenResponse = serde_json::from_str(&raw_body).map_err(|e| {
        TeamsApiError::Other(
            200,
            format!(
                "Failed to parse token response: {} (body was: {})",
                e,
                truncate_for_log(&raw_body)
            ),
        )
    })?;

    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(token_resp.expires_in as i64);

    log::info!("{TAG} Successfully refreshed Microsoft Teams token");

    Ok(TeamsTokens {
        access_token: token_resp.access_token,
        // MS may omit the refresh token on a refresh response; keep the
        // existing one rather than silently dropping refresh capability.
        // Mirrors spotify.rs:142-144. See issue #151.
        refresh_token: token_resp
            .refresh_token
            .or_else(|| tokens.refresh_token.clone()),
        expires_at,
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusMessageRequest {
    status_message: StatusMessageContent,
}

#[derive(Debug, Serialize)]
struct StatusMessageContent {
    message: MessageContent,
    #[serde(skip_serializing_if = "Option::is_none", rename = "expiryDateTime")]
    expiry_date_time: Option<ExpiryDateTime>,
}

#[derive(Debug, Serialize)]
struct MessageContent {
    content: String,
    #[serde(rename = "contentType")]
    content_type: String,
}

#[derive(Debug, Serialize)]
struct ExpiryDateTime {
    #[serde(rename = "dateTime")]
    date_time: String,
    #[serde(rename = "timeZone")]
    time_zone: String,
}

/// Pure status-code to error-variant decision (issue #493): every Graph
/// call site funnels through this so the 401/403/429/5xx discrimination
/// lives in one unit-testable place. Shared by the set and clear paths so
/// both get identical status-code discrimination and `Retry-After`
/// parsing (see issues #153/#154/#820). Takes the already-parsed Retry-After
/// value and the response body text. Callers truncate bodies for log
/// safety before display; the stored body here stays raw for diagnosis.
fn classify_teams_status(status_code: u16, retry_after: Option<u64>, body: &str) -> TeamsApiError {
    match status_code {
        401 => TeamsApiError::ExpiredToken(status_code),
        403 => TeamsApiError::Forbidden(status_code, body.to_string()),
        429 => TeamsApiError::RateLimited(retry_after),
        // Issue #820: a throttled 5xx (Graph answers 503 with `Retry-After`
        // when a backend is unavailable) is still a server DIRECTIVE, not a
        // bare transient — `poll_once::rate_limit_sleep_secs` turns
        // `RateLimited(Some(secs))` into exactly that sleep, the way it
        // already does for a 429 (#154). Collapsing the header away made the
        // app re-poll and re-POST sooner than the server asked and sustained
        // the outage the header exists to ride out.
        500..=599 => match retry_after {
            Some(secs) => TeamsApiError::RateLimited(Some(secs)),
            None => TeamsApiError::Transient(format!("server error {}: {}", status_code, body)),
        },
        _ => TeamsApiError::Other(status_code, body.to_string()),
    }
}

/// Builds the `setStatusMessage` body. `expiry_datetime` is the offset-less
/// UTC `dateTime` the app sends (issue #156).
fn status_message_request(message: &str, expiry_datetime: Option<&str>) -> StatusMessageRequest {
    let expiry = expiry_datetime.map(|dt| ExpiryDateTime {
        date_time: dt.to_string(),
        time_zone: "UTC".to_string(),
    });
    StatusMessageRequest {
        status_message: StatusMessageContent {
            message: MessageContent {
                content: message.to_string(),
                content_type: "text".to_string(),
            },
            expiry_date_time: expiry,
        },
    }
}

/// POSTs a `setStatusMessage` body through a caller-supplied client, so the
/// exit path (finding #636) can bound the call with its own timeout.
fn post_status_message_with(
    client: &reqwest::blocking::Client,
    access_token: &str,
    body: &StatusMessageRequest,
    action: &str,
) -> Result<(), TeamsApiError> {
    let response = client
        .post("https://graph.microsoft.com/v1.0/me/presence/setStatusMessage")
        .header("Authorization", format!("Bearer {}", access_token))
        .header("Content-Type", "application/json")
        .json(body)
        .send()
        .map_err(|e| {
            TeamsApiError::Transient(format!("Failed to send status message request: {}", e))
        })?;

    let status = response.status();
    let status_code = status.as_u16();
    let retry_after = parse_retry_after(&response);
    let body_text = response
        .text()
        .unwrap_or_else(|_| "Unknown error".to_string());

    if !status.is_success() {
        log::error!(
            "{TAG} Failed to {} Teams status message: {} - {}",
            action,
            status,
            body_text
        );
        return Err(classify_teams_status(status_code, retry_after, &body_text));
    }

    Ok(())
}

/// The info-level record for a successful Teams status write: the byte count,
/// never the text.
///
/// The text is the user's artist/track, and the shipping log level keeps
/// info-level records in `PresenceJam.log`, which the Diagnostics page and
/// LogViewer hand out as a support snapshot — so the text itself is logged at
/// `debug!` only and this line replaces it. Length plus the `[POLLING]` /
/// `[TEAMS]` lines already logged is enough to diagnose a rejected or
/// oversized status. Issue #912 (the status-write half of #344).
fn status_set_log_line(message: &str) -> String {
    format!(
        "Successfully set Teams status message ({} bytes)",
        message.len()
    )
}

pub fn set_teams_status_message(
    access_token: &str,
    message: &str,
    expiry_datetime: Option<&str>,
) -> Result<(), TeamsApiError> {
    let client = build_teams_client().map_err(TeamsApiError::Transient)?;
    post_status_message_with(
        &client,
        access_token,
        &status_message_request(message, expiry_datetime),
        "set",
    )?;

    // Issue #912: the posted text is user content (artist + track). The
    // rotating file target keeps info-level records by default, and the
    // Diagnostics / LogViewer snapshots hand the log tail to the user as
    // "safe to publish", so the text goes to `debug!` (the `poll_once.rs`
    // #344 convention) and the info-level record carries the byte count.
    log::debug!("{TAG} Successfully set Teams status message: {}", message);
    log::info!("{TAG} {}", status_set_log_line(message));
    Ok(())
}

pub fn clear_teams_status_message(
    access_token: &str,
    placeholder: &str,
    expiry_datetime: Option<&str>,
) -> Result<(), TeamsApiError> {
    // Graph has no "clear status message" action; the clear path posts a
    // short-lived placeholder whose expiryDateTime removes it. Without an
    // expiry the placeholder never expires (presenceStatusMessage docs).
    // See issue #155.
    let client = build_teams_client().map_err(TeamsApiError::Transient)?;
    clear_teams_status_message_with(&client, access_token, placeholder, expiry_datetime)
}

/// Exit-path variant of [`clear_teams_status_message`] (finding #636, issue
/// #636): identical POST, bounded by [`EXIT_CLEANUP_TIMEOUT`] so a dead network
/// can never hold the quit open.
pub fn clear_teams_status_message_quick(
    access_token: &str,
    placeholder: &str,
    expiry_datetime: Option<&str>,
) -> Result<(), TeamsApiError> {
    let client =
        build_teams_client_with_timeout(EXIT_CLEANUP_TIMEOUT).map_err(TeamsApiError::Transient)?;
    clear_teams_status_message_with(&client, access_token, placeholder, expiry_datetime)
}

fn clear_teams_status_message_with(
    client: &reqwest::blocking::Client,
    access_token: &str,
    placeholder: &str,
    expiry_datetime: Option<&str>,
) -> Result<(), TeamsApiError> {
    post_status_message_with(
        client,
        access_token,
        &status_message_request(placeholder, expiry_datetime),
        "clear",
    )?;

    log::info!("{TAG} Successfully cleared Teams status message");
    Ok(())
}

/// Base64url-decodes the payload (middle segment) of a Teams access token
/// JWT and returns the granted `scp` claim split on spaces. Informational
/// only — no signature verification. Returns an empty Vec when the token
/// isn't a decodable JWT with a `scp` claim. Used by the Settings page to
/// detect whether `Presence.Read` / `profile` are missing (one-time
/// reconnect banner, issue #3.0-P1/P2).
pub fn decode_teams_granted_scopes(access_token: &str) -> Vec<String> {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine as _;
    let payload = access_token.split('.').nth(1).unwrap_or_default();
    let scopes = URL_SAFE_NO_PAD
        .decode(payload)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|v| v.get("scp").and_then(|s| s.as_str()).map(str::to_owned))
        .unwrap_or_default();
    if scopes.is_empty() {
        Vec::new()
    } else {
        scopes.split(' ').map(str::to_owned).collect()
    }
}

/// Extracts the Azure AD `oid` claim (the user's object id) from a Teams
/// access-token JWT payload. Pure function — no signature verification.
///
/// The Graph setPresence/clearPresence docs document only `/users/{id}`
/// (no `/me`); PresenceJam implements `/me` first and falls back to
/// `/users/{oid}` on 404, so the oid must come from the token itself. The
/// claim only appears once `profile` is in the scope string (issue #3.0-P1).
pub fn graph_oid_from_access_token(access_token: &str) -> Result<String, String> {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine as _;
    let payload = access_token
        .split('.')
        .nth(1)
        .ok_or_else(|| "access token is not a JWT (no payload segment)".to_string())?;
    let bytes = URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|e| format!("failed to base64url-decode JWT payload: {}", e))?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| format!("failed to parse JWT payload JSON: {}", e))?;
    value
        .get("oid")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        .ok_or_else(|| "JWT payload has no `oid` claim".to_string())
}

/// `presence-gated` reasons that carry no presence sample — the time- and
/// rule-based gates (issue #432 / findings #634/#635) plus the status-message
/// gate. Shared constants so the polling loop and the Dashboard chip's label
/// mapping (`src/lib/components/Dashboard.svelte`) cannot drift.
pub const GATE_REASON_QUIET_HOURS: &str = "quiet-hours";
pub const GATE_REASON_TRACK_RULE: &str = "track-rule";
pub const GATE_REASON_MANUAL_STATUS: &str = "manual-status";
/// Issue #867: the Outlook-calendar busy-meeting gate. Distinct from the
/// track-rule gate so the Dashboard chip and the snapshot can label the
/// reason precisely — a user that sees "calendar" knows to look at the
/// meeting, not at their status_rules.
pub const GATE_REASON_CALENDAR: &str = "calendar";
/// The out-of-office reason (finding #637) — the only gate reason that is
pub const GATE_REASON_OUT_OF_OFFICE: &str = "out of office";

/// The `statusMessage` half of a Graph presence (finding #635, issue #635).
///
/// `content` is the live status message Teams shows; `expires_at` is the
/// parsed `expiryDateTime` (`None` when Graph omitted it or it did not parse —
/// an unparseable expiry is treated as "no expiry", never as "expired").
/// `publishedDateTime` is deliberately not modelled: authorship is decided by
/// content identity against the text this process last posted (see
/// `poll_once::manual_status_blocks_write`), which subsumes every case a
/// publication-time comparison could distinguish.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PresenceStatusMessage {
    pub content: String,
    pub expires_at: Option<chrono::DateTime<Utc>>,
}

/// Presence returned by the Graph getPresence endpoint (v1.0). The docs
/// enumerate lowercase enum values (`available`, `busy`, …) but real
/// examples return PascalCase (`Available`, `Busy`, `InACall`, …) — parse
/// case-insensitively (issue #3.0-P1/P2).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresenceInfo {
    pub availability: String,
    pub activity: String,
    /// The live status message (finding #635). `None` when Graph sent no
    /// `statusMessage`/`message`/`content`.
    #[serde(default)]
    pub status_message: Option<PresenceStatusMessage>,
    /// `outOfOfficeSettings.isOutOfOffice` (finding #637, issue #637); false
    /// whenever Graph omitted the object.
    #[serde(default)]
    pub out_of_office: bool,
}

/// Parses a Graph getPresence response body into a `PresenceInfo`,
/// normalizing both enum fields to lower-case so callers compare once.
pub fn parse_presence_body(body: &str) -> Result<PresenceInfo, String> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("Failed to parse presence body: {}", e))?;
    let availability = value
        .get("availability")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "presence body has no `availability` string".to_string())?
        .to_lowercase();
    let activity = value
        .get("activity")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "presence body has no `activity` string".to_string())?
        .to_lowercase();
    Ok(PresenceInfo {
        availability,
        activity,
        status_message: parse_status_message(&value),
        // Finding #637: `outOfOfficeSettings.isOutOfOffice` is a plain
        // boolean on the same response; a missing object means "not out of
        // office" (fail-open: a gate must never fire on absent data).
        out_of_office: value
            .get("outOfOfficeSettings")
            .and_then(|v| v.get("isOutOfOffice"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    })
}

/// The `statusMessage` half of a getPresence body (finding #635).
///
/// Graph nests the text under `statusMessage.message.content`
/// (presenceStatusMessage → itemBody), and reports the self-destruct time as a
/// `dateTimeTimeZone` on `statusMessage.expiryDateTime`. `None` when there is
/// no message text at all — an empty live message means "nothing to respect".
fn parse_status_message(value: &serde_json::Value) -> Option<PresenceStatusMessage> {
    let message = value.get("statusMessage")?;
    let content = message
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or_default()
        .to_string();
    let expires_at = message
        .get("expiryDateTime")
        .and_then(parse_datetime_time_zone);
    if content.trim().is_empty() && expires_at.is_none() {
        return None;
    }
    Some(PresenceStatusMessage {
        content,
        expires_at,
    })
}

/// Parse a Graph `dateTimeTimeZone` to UTC, or `None` when it is absent or not
/// trustworthy (fail-open: an unparseable expiry must never be read as
/// "expired", which would let the app clobber a live manual status).
///
/// An offset-bearing `dateTime` is parsed as-is; an offset-less one is only
/// accepted when `timeZone` is `UTC` (or absent), because assuming UTC for a
/// named zone such as "Pacific Standard Time" would silently shift the
/// boundary.
fn parse_datetime_time_zone(value: &serde_json::Value) -> Option<chrono::DateTime<Utc>> {
    let raw = value.get("dateTime")?.as_str()?.trim();
    if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(raw) {
        return Some(parsed.with_timezone(&Utc));
    }
    let zone = value
        .get("timeZone")
        .and_then(|z| z.as_str())
        .unwrap_or("UTC");
    if !zone.is_empty() && !zone.eq_ignore_ascii_case("utc") {
        return None;
    }
    chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S%.f")
        .ok()
        .map(|naive| naive.and_utc())
}

/// Human-readable reason a presence gates a status-message write, or an
/// empty string when it doesn't. Single source of truth for the gating
/// rule — `is_presence_gated` is defined through it so the two cannot
/// drift apart (issue #3.0-P2).
///
/// `gate_when_out_of_office` (finding #637, issue #637) adds the
/// out-of-office reason; it is a parameter rather than a field read because
/// the OPT-IN lives in `teams.gate_when_out_of_office` and a rule that carries
/// its own presence action overrides it for the iteration (see
/// `poll_once::ooo_gate_enabled`).
pub fn presence_gate_reason(presence: &PresenceInfo, gate_when_out_of_office: bool) -> String {
    match presence.activity.to_lowercase().as_str() {
        "inameeting" => return "in a meeting".to_string(),
        "inacall" => return "in a call".to_string(),
        "presenting" => return "presenting".to_string(),
        _ => {}
    }
    let availability_reason = match presence.availability.to_lowercase().as_str() {
        "busy" => "busy".to_string(),
        "donotdisturb" => "Do Not Disturb".to_string(),
        // Issue #254: `focusing` is a documented v1.0 availability value
        // that Teams renders with the same red DND icon (scheduled focus
        // time), so it must gate exactly like Do Not Disturb.
        "focusing" => "focusing".to_string(),
        _ => String::new(),
    };
    if !availability_reason.is_empty() {
        return availability_reason;
    }
    // Finding #637: out-of-office is the LOWEST-precedence gate reason, so a
    // user who is busy or in a call gets that (more specific) explanation.
    // `activity = outOfOffice` and `outOfOfficeSettings.isOutOfOffice` are
    // the two documented signals; either one fires.
    if gate_when_out_of_office
        && (presence.out_of_office || presence.activity.eq_ignore_ascii_case("outofoffice"))
    {
        return GATE_REASON_OUT_OF_OFFICE.to_string();
    }
    String::new()
}

/// True iff a presence should suppress a status-message write: the user is
/// busy or Do-Not-Disturb, or their activity is in a meeting/call or
/// presenting — plus, when opted in, out of office. Case-insensitive — both
/// fields are normalized internally (issue #3.0-P2).
pub fn is_presence_gated(presence: &PresenceInfo, gate_when_out_of_office: bool) -> bool {
    !presence_gate_reason(presence, gate_when_out_of_office).is_empty()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SetPresenceRequest {
    session_id: String,
    availability: String,
    activity: String,
    expiration_duration: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ClearPresenceRequest {
    session_id: String,
}

/// Issue #866: the Graph `setUserPreferredPresence` POST body. Distinct from
/// [`SetPresenceRequest`] — no `sessionId`, no `expirationDuration` upper
/// limit, and the pair can include the documented Busy / DND / BeRightBack /
/// Away values that the `setPresence` table does not offer (e.g.
/// `DoNotDisturb/DoNotDisturb`).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SetUserPreferredPresenceRequest {
    availability: String,
    activity: String,
    expiration_duration: String,
}

/// Issue #870: the expiry the Dashboard composer / `--set-status` CLI flag
/// carries. Wraps `format_expiry` (the offset-less `dateTime` the Graph
/// `statusMessage.expiryDateTime` schema requires — issue #156) so the
/// format cannot drift between the manual path and the polling path.
pub fn manual_status_expiry_rfc3339(expires_at: chrono::DateTime<chrono::Utc>) -> String {
    expires_at.format("%Y-%m-%dT%H:%M:%S%.6f").to_string()
}

/// Issue #870: the short-lived 60-second placeholder expiry the manual
/// status clear path mirrors. Exposed here (instead of borrowing the
/// private `polling::placeholder_expiry_str`) so the manual-status clear
/// cannot accidentally drift from the polling paused/stopped clears.
pub fn placeholder_expiry_rfc3339() -> String {
    let expiry = chrono::Utc::now() + chrono::Duration::seconds(60);
    manual_status_expiry_rfc3339(expiry)
}

/// POSTs a JSON body to a Graph presence endpoint and maps the response to
/// a typed error — the same status-code discrimination and `Retry-After`
/// parsing as `post_status_message` (issues #153/#154). A 404 is surfaced
/// as `Other(404, …)` so callers can retry the documented `/users/{oid}`
/// path (the setPresence/clearPresence docs list only `/users/{id}`; `/me`
/// works in practice but is undocumented).
fn post_presence<T: Serialize>(
    client: &reqwest::blocking::Client,
    access_token: &str,
    url: &str,
    body: &T,
    action: &str,
) -> Result<(), TeamsApiError> {
    let response = client
        .post(url)
        .header("Authorization", format!("Bearer {}", access_token))
        .header("Content-Type", "application/json")
        .json(body)
        .send()
        .map_err(|e| {
            TeamsApiError::Transient(format!("Failed to send {} request: {}", action, e))
        })?;

    let status = response.status();
    let status_code = status.as_u16();
    let retry_after = parse_retry_after(&response);
    let body_text = response
        .text()
        .unwrap_or_else(|_| "Unknown error".to_string());

    if !status.is_success() {
        log::error!(
            "{TAG} Failed to {} Teams presence: {} - {}",
            action,
            status,
            body_text
        );
        return Err(classify_teams_status(status_code, retry_after, &body_text));
    }

    Ok(())
}

/// Sets the user's Teams presence via the Graph setPresence endpoint
/// (issue #3.0-P1). `availability`/`activity` must be a documented combo
/// (e.g. `Available`/`Available`, `Busy`/`InACall`) and
/// `expiration_duration` a `PT5M`-`PT4H` ISO-8601 duration. An
/// `Available` session TIMES OUT after 5 minutes (non-configurable, and a
/// distinct clock from `expirationDuration`), so the app re-arms well
/// inside that window. `sessionId` MUST be the app's Azure AD client
///
/// Implements `/me` first with a `/users/{oid}` fallback on 404: the docs
/// document only `/users/{id}` for setPresence/clearPresence, but `/me`
/// works in practice and is used first (it needs no oid resolution).
pub fn set_teams_presence(
    access_token: &str,
    availability: &str,
    activity: &str,
    expiration_duration: &str,
) -> Result<(), TeamsApiError> {
    let body = SetPresenceRequest {
        session_id: MICROSOFT_GRAPH_CLIENT_ID.to_string(),
        availability: availability.to_string(),
        activity: activity.to_string(),
        expiration_duration: expiration_duration.to_string(),
    };
    let client = build_teams_client().map_err(TeamsApiError::Transient)?;
    match post_presence(
        &client,
        access_token,
        "https://graph.microsoft.com/v1.0/me/presence/setPresence",
        &body,
        "set presence",
    ) {
        Ok(()) => Ok(()),
        Err(TeamsApiError::Other(404, _)) => {
            let oid = graph_oid_from_access_token(access_token).map_err(|e| {
                TeamsApiError::Other(
                    404,
                    format!("failed to resolve oid for /users fallback: {}", e),
                )
            })?;
            post_presence(
                &client,
                access_token,
                &format!(
                    "https://graph.microsoft.com/v1.0/users/{}/presence/setPresence",
                    oid
                ),
                &body,
                "set presence",
            )
        }
        Err(e) => Err(e),
    }
}

/// Clears the app's Teams presence session via the Graph clearPresence
/// endpoint (issue #3.0-P1). A 404 on either path is documented success —
/// the session is already gone (clearPresence docs).
pub fn clear_teams_presence(access_token: &str) -> Result<(), TeamsApiError> {
    let client = build_teams_client().map_err(TeamsApiError::Transient)?;
    clear_teams_presence_with(&client, access_token)
}

/// Exit-path variant of [`clear_teams_presence`] (finding #636, issue #636):
/// identical request, bounded by [`EXIT_CLEANUP_TIMEOUT`] so a dead network can
/// never hold the quit open.
pub fn clear_teams_presence_quick(access_token: &str) -> Result<(), TeamsApiError> {
    let client =
        build_teams_client_with_timeout(EXIT_CLEANUP_TIMEOUT).map_err(TeamsApiError::Transient)?;
    clear_teams_presence_with(&client, access_token)
}

fn clear_teams_presence_with(
    client: &reqwest::blocking::Client,
    access_token: &str,
) -> Result<(), TeamsApiError> {
    let body = ClearPresenceRequest {
        session_id: MICROSOFT_GRAPH_CLIENT_ID.to_string(),
    };
    match post_presence(
        client,
        access_token,
        "https://graph.microsoft.com/v1.0/me/presence/clearPresence",
        &body,
        "clear presence",
    ) {
        Ok(()) => Ok(()),
        Err(TeamsApiError::Other(404, _)) => {
            let oid = graph_oid_from_access_token(access_token).map_err(|e| {
                TeamsApiError::Other(
                    404,
                    format!("failed to resolve oid for /users fallback: {}", e),
                )
            })?;
            match post_presence(
                client,
                access_token,
                &format!(
                    "https://graph.microsoft.com/v1.0/users/{}/presence/clearPresence",
                    oid
                ),
                &body,
                "clear presence",
            ) {
                Ok(()) => Ok(()),
                // 404 = the session is already gone — documented success.
                Err(TeamsApiError::Other(404, _)) => Ok(()),
                Err(e) => Err(e),
            }
        }
        Err(e) => Err(e),
    }
}

/// Reads the user's Teams presence via the Graph getPresence endpoint
/// (issue #3.0-P2). Requires `Presence.Read` (now in the scope string).
pub fn get_teams_presence(access_token: &str) -> Result<PresenceInfo, TeamsApiError> {
    let client = build_teams_client().map_err(TeamsApiError::Transient)?;

    let response = client
        .get("https://graph.microsoft.com/v1.0/me/presence")
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .map_err(|e| TeamsApiError::Transient(format!("Failed to get Teams presence: {}", e)))?;

    let status = response.status();
    let status_code = status.as_u16();
    let retry_after = parse_retry_after(&response);
    let body_text = response
        .text()
        .unwrap_or_else(|_| "Unknown error".to_string());

    if !status.is_success() {
        log::error!(
            "{TAG} Failed to get Teams presence: {} - {}",
            status,
            body_text
        );
        return Err(classify_teams_status(status_code, retry_after, &body_text));
    }

    parse_presence_body(&body_text)
        .map_err(|e| TeamsApiError::Other(200, format!("Failed to parse presence: {}", e)))
}

/// Issue #866: drives the documented Busy / DND / BeRightBack / Away pair on
/// the user's Teams presence via the Graph `setUserPreferredPresence` POST.
///
/// Distinct from [`set_teams_presence`]:
/// - the request body has NO `sessionId` (preferred presence is the user's
///   own setting, not a per-app session),
/// - the supported pairs include `DoNotDisturb/DoNotDisturb` and the
///   standalone `Busy/Busy`, which `setPresence` rejects,
/// - and the call is rate-limited harder (Graph applies the same per-user
///   limits as a Calendar update). The poll loop debounces accordingly
///   (see `poll_once::sync_availability`).
///
/// `expiration_duration` is the ISO-8601 `PT<minutes>M` the docs document;
/// the callers pass the `preferred_presence_expiry_duration` helper so the
/// value can never drift between `set` and the `clear` at expiry.
///
/// `/me` first with the `/users/{oid}` fallback mirrors `set_teams_presence`,
/// and shares its 404-discriminator. A 403 with the documented
/// `Presence.ReadWrite` body (issue #866, national-cloud note) bubbles up
/// through [`classify_teams_status`] so the poller logs a one-shot warning
/// and falls back to the ephemeral `setPresence` session for the run.
pub fn set_user_preferred_presence(
    access_token: &str,
    availability: &str,
    activity: &str,
    expiration_duration: &str,
) -> Result<(), TeamsApiError> {
    let body = SetUserPreferredPresenceRequest {
        availability: availability.to_string(),
        activity: activity.to_string(),
        expiration_duration: expiration_duration.to_string(),
    };
    let client = build_teams_client().map_err(TeamsApiError::Transient)?;
    match post_presence(
        &client,
        access_token,
        "https://graph.microsoft.com/v1.0/me/presence/setUserPreferredPresence",
        &body,
        "set user preferred presence",
    ) {
        Ok(()) => Ok(()),
        Err(TeamsApiError::Other(404, _)) => {
            let oid = graph_oid_from_access_token(access_token).map_err(|e| {
                TeamsApiError::Other(
                    404,
                    format!("failed to resolve oid for /users fallback: {}", e),
                )
            })?;
            post_presence(
                &client,
                access_token,
                &format!(
                    "https://graph.microsoft.com/v1.0/users/{}/presence/setUserPreferredPresence",
                    oid
                ),
                &body,
                "set user preferred presence",
            )
        }
        Err(e) => Err(e),
    }
}

/// Issue #866: clears the preferred presence set by
/// [`set_user_preferred_presence`]. Mirrors the 404-as-success contract
/// `clear_teams_presence` documents.
pub fn clear_user_preferred_presence(access_token: &str) -> Result<(), TeamsApiError> {
    let client = build_teams_client().map_err(TeamsApiError::Transient)?;
    clear_user_preferred_presence_with(&client, access_token)
}

/// Exit-path variant of [`clear_user_preferred_presence`] (issue #866,
/// `RunEvent::Exit` cleanup): identical POST, bounded by
/// [`EXIT_CLEANUP_TIMEOUT`] so a dead network cannot hold the exit open.
pub fn clear_user_preferred_presence_quick(access_token: &str) -> Result<(), TeamsApiError> {
    let client =
        build_teams_client_with_timeout(EXIT_CLEANUP_TIMEOUT).map_err(TeamsApiError::Transient)?;
    clear_user_preferred_presence_with(&client, access_token)
}

fn clear_user_preferred_presence_with(
    client: &reqwest::blocking::Client,
    access_token: &str,
) -> Result<(), TeamsApiError> {
    // The clear endpoint takes an empty JSON body — passing `{}` is what the
    // docs show for a "clear my preferred presence" POST. There is no
    // documented request schema, so the absence of a typed body matches the
    // endpoint shape rather than introducing a new one-off struct.
    match post_presence(
        client,
        access_token,
        "https://graph.microsoft.com/v1.0/me/presence/clearUserPreferredPresence",
        &serde_json::json!({}),
        "clear user preferred presence",
    ) {
        Ok(()) => Ok(()),
        Err(TeamsApiError::Other(404, _)) => Ok(()),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::truncate_for_log;
    use super::{DeviceCodeResponse, TeamsTokens, MICROSOFT_GRAPH_SCOPES};

    #[test]
    fn teams_oauth_profile_scope_also_requests_openid() {
        let scopes: Vec<&str> = MICROSOFT_GRAPH_SCOPES.split_whitespace().collect();
        assert!(scopes.contains(&"profile"));
        assert!(scopes.contains(&"openid"));
    }

    #[test]
    fn test_truncate_under_limit() {
        let body = "short body".to_string();
        assert_eq!(truncate_for_log(&body), body);
    }
    /// Issue #493: the Graph status classifier must map bodies to typed
    /// variants by status code -- a comment or log mentioning "401" must not
    /// change classification, and reintroducing string-sniffing under any
    /// token breaks this test.
    #[test]
    fn test_classify_teams_status_maps_codes_to_variants() {
        use super::classify_teams_status;
        use super::TeamsApiError;
        assert!(matches!(
            classify_teams_status(401, None, "unauthorized noise"),
            TeamsApiError::ExpiredToken(401)
        ));
        assert!(matches!(
            classify_teams_status(403, None, "forbidden noise"),
            TeamsApiError::Forbidden(403, _)
        ));
        assert!(matches!(
            classify_teams_status(429, Some(90), "throttled"),
            TeamsApiError::RateLimited(Some(90))
        ));
        assert!(matches!(
            classify_teams_status(429, None, "throttled"),
            TeamsApiError::RateLimited(None)
        ));
        assert!(matches!(
            classify_teams_status(503, None, "boom"),
            TeamsApiError::Transient(_)
        ));
        // Issue #820: a throttled 5xx keeps the server's directive — the
        // poller reads it back through `rate_limit_sleep_secs`, so a
        // `Retry-After` that reaches this arm must not be dropped (this row
        // fails while the 5xx arm collapses everything to Transient).
        assert!(matches!(
            classify_teams_status(503, Some(120), "service unavailable"),
            TeamsApiError::RateLimited(Some(120))
        ));
        assert!(matches!(
            classify_teams_status(502, Some(30), ""),
            TeamsApiError::RateLimited(Some(30))
        ));
        assert!(matches!(
            classify_teams_status(418, None, "teapot"),
            TeamsApiError::Other(418, _)
        ));
        // Bodies ride through untouched -- no string sniffing: a 403 whose
        // body mentions "401" is still Forbidden, and a 200-range code
        // with an "unauthorized" body is still Other.
        match classify_teams_status(403, None, "error 401 inside body") {
            TeamsApiError::Forbidden(403, b) => assert!(b.contains("401")),
            other => panic!("expected Forbidden, got {:?}", other),
        }
        match classify_teams_status(418, None, "unauthorized words here") {
            TeamsApiError::Other(418, b) => assert!(b.contains("unauthorized")),
            other => panic!("expected Other, got {:?}", other),
        }
    }

    #[test]
    fn test_truncate_ascii_at_boundary() {
        // 256 ASCII chars exactly — at the limit, not over.
        // The helper takes "the first 256 chars", and the body has exactly
        // 256 chars (0-indexed chars 0..=255), so the count check is `> 256`
        // which is false and the body is returned unchanged.
        let body: String = "a".repeat(256);
        assert_eq!(truncate_for_log(&body), body);
    }

    #[test]
    fn test_truncate_handles_multibyte_codepoint_at_boundary() {
        // 255 ASCII chars + 1 four-byte emoji = 259 bytes total.
        // The 256th char is the emoji (char count goes 0..=255 ASCII,
        // index 255 is the emoji). The byte index of the 256th char is
        // 255 (right after the last 'a'), which is the start of the
        // emoji's 4-byte UTF-8 sequence — a char boundary.
        // The old `&body[..256]` implementation would have sliced inside
        // the emoji (byte 255..=258) and panicked on non-ASCII bytes 255.
        let body: String = "a".repeat(255) + "\u{1F600}"; // grinning face
        assert_eq!(body.len(), 259);
        assert_eq!(body.chars().count(), 256);

        let truncated = truncate_for_log(&body);

        // Must not panic. The body is exactly 256 chars, so the
        // `chars().count() > 256` check is false and the body is
        // returned unchanged — this proves the boundary case is
        // handled correctly when the emoji lands at char 256.
        assert_eq!(truncated, body);
    }

    #[test]
    fn test_truncate_cuts_inside_multibyte_sequence() {
        // Body: 1 four-byte emoji followed by 256 ASCII 'a' chars.
        // Total: 260 bytes, 257 chars.
        //
        // Char count (257) exceeds 256, so the helper must truncate.
        // `body.char_indices().nth(256)` is the 257th char (the 256th
        // 'a', 0-indexed), which starts at byte 259 — so `cut = 259`
        // and `&body[..259]` keeps the emoji plus the first 255 'a's
        // (256 chars), then the helper appends the `(…260 total)`
        // byte-count suffix.
        //
        // Note on the test name: the cut lands at an ASCII char
        // boundary *after* the multibyte codepoint, not literally
        // *inside* the multibyte sequence. The old `&body[..256]`
        // implementation would have sliced at byte 256 — also a char
        // boundary in this body (between 'a' chars) — so this exact
        // input would not have panicked under the old code. The test
        // name is preserved for git-blame continuity, but its real
        // value is exercising the truncation path with a multibyte
        // char in the body and asserting the emoji is preserved as a
        // complete char (i.e. the helper does not produce a half-
        // codepoint on this input either).
        let body: String = "\u{1F600}".to_string() + &"a".repeat(256);
        assert_eq!(body.len(), 260);
        assert_eq!(body.chars().count(), 257);

        let truncated = truncate_for_log(&body);

        assert!(truncated.starts_with("\u{1F600}"));
        assert!(truncated.ends_with("(…260 total)"));
    }
    // Regression guard for issue #78: ensure TeamsTokens (and the
    // `Option<String>` refresh_token field that distinguishes it from
    // SpotifyTokens) round-trips through serde_json with field-name
    // parity. The ts-rs-generated TS type at
    // `src/lib/types-generated/TeamsTokens.ts` mirrors these fields
    // exactly — a future field rename or Option/Single swap will
    // break this test before it ships to consumers.
    #[test]
    fn teams_tokens_serde_roundtrip_with_some_refresh() {
        let original = TeamsTokens {
            access_token: "teams-access".to_string(),
            refresh_token: Some("teams-refresh".to_string()),
            expires_at: chrono::Utc::now(),
        };
        let json = serde_json::to_string(&original).expect("serialize");
        let parsed: TeamsTokens = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.access_token, original.access_token);
        assert_eq!(parsed.refresh_token, original.refresh_token);
        assert_eq!(parsed.expires_at, original.expires_at);
    }

    #[test]
    fn teams_tokens_serde_roundtrip_with_none_refresh() {
        // Microsoft endpoint sometimes omits the refresh token; this
        // path is exercised at runtime and the TS shape
        // `refresh_token: string | null` must survive the round-trip
        // as null, not missing-key or undefined.
        let original = TeamsTokens {
            access_token: "teams-access".to_string(),
            refresh_token: None,
            expires_at: chrono::Utc::now(),
        };
        let json = serde_json::to_string(&original).expect("serialize");
        let parsed: TeamsTokens = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.refresh_token, None);
        // Defensive: confirm the wire form actually contains the
        // "refresh_token":null pair (serde default is to emit null,
        // not omit the key, for `Option<String>`).
        assert!(
            json.contains("\"refresh_token\":null"),
            "refresh_token must serialise as null on the wire, got: {}",
            json
        );
    }

    // Regression guard for issue #78: DeviceCodeResponse's u64 fields
    // (interval, expires_in) must serialise as JSON numbers, matching
    // the `#[ts(type = "number")]` override on the Rust side. If a
    // future contributor removes the override, ts-rs will start
    // generating `bigint` for these fields and the TS-side
    // `invoke<DeviceCodeResponse>('start_teams_auth_device_code')` will
    // type-error at consumer sites.
    #[test]
    fn device_code_response_u64_fields_serialize_as_numbers() {
        let resp = DeviceCodeResponse {
            user_code: "ABC-123".to_string(),
            verification_url: "https://microsoft.com/devicelogin".to_string(),
            device_code: "device-code-blob".to_string(),
            interval: 5,
            expires_in: 900,
        };
        let json: serde_json::Value = serde_json::to_value(&resp).expect("to_value");
        assert!(
            json["interval"].is_number(),
            "interval must serialise as JSON number, got {:?}",
            json["interval"]
        );
        assert!(
            json["expires_in"].is_number(),
            "expires_in must serialise as JSON number, got {:?}",
            json["expires_in"]
        );
        assert_eq!(json["interval"].as_u64(), Some(5));
        assert_eq!(json["expires_in"].as_u64(), Some(900));
    }
    /// Issue #490: the backend provides `expires_in` and it must stay on
    /// the wire as a JSON number through the IPC boundary -- the frontend
    /// expiry countdown (`expiresAt = now + expires_in*1000`) depends on
    /// it. Dropping or stringifying the field strands users on a dead
    /// device code with no recovery path.
    #[test]
    fn device_code_response_preserves_expires_in() {
        let resp = DeviceCodeResponse {
            user_code: "X".to_string(),
            verification_url: "https://example.test".to_string(),
            device_code: "d".to_string(),
            interval: 5,
            expires_in: 899,
        };
        // Field survives struct round-trip.
        assert_eq!(resp.expires_in, 899);
        // Field survives serde (the IPC path): number, exact value.
        let json: serde_json::Value = serde_json::to_value(&resp).expect("to_value");
        assert_eq!(json["expires_in"].as_u64(), Some(899));
        let back: DeviceCodeResponse = serde_json::from_value(json).expect("from_value");
        assert_eq!(back.expires_in, 899);
        assert_eq!(back.interval, 5);
    }

    // Issue #152: RFC 8628 §3.5 — `slow_down` carries no interval of its
    // own; the client must increase its polling interval by 5 seconds for
    // this and all subsequent requests. The ramp is cumulative.
    #[test]
    fn next_poll_wait_ramps_on_slow_down() {
        let mut wait = 5;
        wait = super::next_poll_wait(wait, "slow_down");
        assert_eq!(wait, 10);
        wait = super::next_poll_wait(wait, "slow_down");
        assert_eq!(wait, 15);
        // Non-slow_down errors keep the current (already-ramped) interval.
        wait = super::next_poll_wait(wait, "authorization_pending");
        assert_eq!(wait, 15);
    }

    #[test]
    fn next_poll_wait_keeps_interval_for_other_errors() {
        assert_eq!(super::next_poll_wait(7, "authorization_pending"), 7);
        assert_eq!(super::next_poll_wait(7, "expired_token"), 7);
        assert_eq!(super::next_poll_wait(0, "bad_verification_code"), 0);
    }

    // Issue #3.0-P2: parse_presence_body must normalize the Graph enum
    // values case-insensitively — docs list lowercase while real examples
    // return PascalCase — and reject a body without the required fields.
    #[test]
    fn parse_presence_body_normalizes_case() {
        let info =
            super::parse_presence_body(r#"{"availability":"Available","activity":"Available"}"#)
                .expect("PascalCase body must parse");
        assert_eq!(info.availability, "available");
        assert_eq!(info.activity, "available");

        let info = super::parse_presence_body(r#"{"availability":"Busy","activity":"InAMeeting"}"#)
            .expect("mixed-case body must parse");
        assert_eq!(info.availability, "busy");
        assert_eq!(info.activity, "inameeting");
    }

    #[test]
    fn parse_presence_body_requires_fields() {
        assert!(
            super::parse_presence_body(r#"{"availability":"Busy"}"#).is_err(),
            "missing activity must fail"
        );
        assert!(
            super::parse_presence_body(r#"{"activity":"InACall"}"#).is_err(),
            "missing availability must fail"
        );
        assert!(super::parse_presence_body("not json").is_err());
        assert!(super::parse_presence_body(r#"{"availability":42,"activity":"x"}"#).is_err());
    }

    /// Finding #635: the live status message and the out-of-office flag come
    /// from the SAME getPresence response the gate already parses — no extra
    /// request, no extra scope.
    #[test]
    fn parse_presence_body_carries_status_message_and_ooo() {
        let info = super::parse_presence_body(
            r#"{
                "availability": "Available",
                "activity": "Available",
                "statusMessage": {
                    "message": {"contentType": "text", "content": "In a workshop until 3"},
                    "publishedDateTime": "2026-09-16T10:00:00Z",
                    "expiryDateTime": {"dateTime": "2026-09-16T18:00:00.0000000", "timeZone": "UTC"}
                },
                "outOfOfficeSettings": {"message": "OOO", "isOutOfOffice": true}
            }"#,
        )
        .expect("a full presence body must parse");
        assert!(info.out_of_office);
        let message = info.status_message.expect("statusMessage must be parsed");
        assert_eq!(message.content, "In a workshop until 3");
        assert_eq!(
            message.expires_at.expect("expiry must parse").to_rfc3339(),
            "2026-09-16T18:00:00+00:00"
        );

        // No message / no OOO object: both stay absent rather than erroring.
        let bare = super::parse_presence_body(r#"{"availability":"Away","activity":"Away"}"#)
            .expect("a bare presence body must parse");
        assert!(bare.status_message.is_none());
        assert!(!bare.out_of_office);

        // An offset-bearing expiry parses too, and a named non-UTC zone is
        // refused (fail-open: never read as "expired").
        let offset = super::parse_presence_body(
            r#"{"availability":"Available","activity":"Available",
                "statusMessage":{"message":{"content":"x"},
                "expiryDateTime":{"dateTime":"2026-09-16T18:00:00+00:00","timeZone":"UTC"}}}"#,
        )
        .expect("must parse");
        assert!(offset.status_message.unwrap().expires_at.is_some());
        let named_zone = super::parse_presence_body(
            r#"{"availability":"Available","activity":"Available",
                "statusMessage":{"message":{"content":"x"},
                "expiryDateTime":{"dateTime":"2026-09-16T18:00:00.0000000",
                                  "timeZone":"Pacific Standard Time"}}}"#,
        )
        .expect("must parse");
        assert!(named_zone.status_message.unwrap().expires_at.is_none());
    }

    // Issue #3.0-P2: gating rule — busy/DND availability OR
    // in-meeting/in-call/presenting activity, case-insensitive.
    #[test]
    fn is_presence_gated_covers_busy_and_meeting_states() {
        use super::{is_presence_gated, PresenceInfo};
        let info = |availability: &str, activity: &str| PresenceInfo {
            availability: availability.to_string(),
            activity: activity.to_string(),
            ..PresenceInfo::default()
        };
        // Out-of-office gating is opted in via the second argument (finding
        // #637): it changes none of the 4.5 verdicts while it is off.
        let gated = |p: &PresenceInfo| is_presence_gated(p, false);
        assert!(gated(&info("busy", "available")));
        assert!(gated(&info("donotdisturb", "available")));
        assert!(gated(&info("available", "inameeting")));
        assert!(gated(&info("available", "inacall")));
        assert!(gated(&info("available", "presenting")));
        // Issue #254: `focusing` is a documented v1.0 availability value
        // that Teams renders with the same red DND icon (scheduled focus
        // time), so it must gate like Do Not Disturb.
        assert!(gated(&info("focusing", "focusing")));
        // Activity wins even when availability is Available (in-meeting).
        assert!(gated(&info("available", "InAMeeting")));
        assert!(!gated(&info("available", "available")));
        assert!(!gated(&info("away", "away")));
        assert!(!gated(&info("available", "offline")));
        // `presenceUnknown` deliberately stays ungated — the gate already
        // fails safe on a read error, so it is not a "do not disturb".
        assert!(!gated(&info("presenceunknown", "presenceunknown")));
    }

    /// Finding #637: out-of-office gates only when opted in, through either
    /// documented signal, and never outranks a more specific reason.
    #[test]
    fn out_of_office_gates_only_when_opted_in() {
        use super::{
            is_presence_gated, presence_gate_reason, PresenceInfo, GATE_REASON_OUT_OF_OFFICE,
        };
        let ooo_flag = PresenceInfo {
            availability: "available".to_string(),
            activity: "available".to_string(),
            out_of_office: true,
            ..PresenceInfo::default()
        };
        let ooo_activity = PresenceInfo {
            availability: "available".to_string(),
            activity: "outOfOffice".to_string(),
            ..PresenceInfo::default()
        };
        let plain = PresenceInfo {
            availability: "away".to_string(),
            activity: "away".to_string(),
            ..PresenceInfo::default()
        };

        // Opt-in is the whole point: with the flag off nothing changes.
        assert!(!is_presence_gated(&ooo_flag, false));
        assert!(!is_presence_gated(&ooo_activity, false));
        assert!(presence_gate_reason(&ooo_flag, false).is_empty());

        assert!(is_presence_gated(&ooo_flag, true));
        assert!(is_presence_gated(&ooo_activity, true));
        assert_eq!(
            presence_gate_reason(&ooo_flag, true),
            GATE_REASON_OUT_OF_OFFICE
        );
        // Being plain Away is still not "out of office".
        assert!(!is_presence_gated(&plain, true));
        // A busy/in-a-call user gets the more specific reason, not "out of
        // office" — the flag adds a reason, it never masks one.
        let busy_ooo = PresenceInfo {
            availability: "busy".to_string(),
            activity: "available".to_string(),
            out_of_office: true,
            ..PresenceInfo::default()
        };
        assert_eq!(presence_gate_reason(&busy_ooo, true), "busy");
    }

    // Issue #3.0-P2: the human-readable reason must mirror the gating rule
    // (is_presence_gated is defined through it, so they cannot drift).
    #[test]
    fn presence_gate_reason_mirrors_gating_rule() {
        use super::{presence_gate_reason, PresenceInfo};
        let info = |availability: &str, activity: &str| PresenceInfo {
            availability: availability.to_string(),
            activity: activity.to_string(),
            ..PresenceInfo::default()
        };
        // The reason strings carry the 4.5 gating rule verbatim; the
        // out-of-office argument is exercised in
        // `out_of_office_gates_only_when_opted_in`.
        assert_eq!(
            presence_gate_reason(&info("busy", "available"), false),
            "busy"
        );
        assert_eq!(
            presence_gate_reason(&info("donotdisturb", "available"), false),
            "Do Not Disturb"
        );
        assert_eq!(
            presence_gate_reason(&info("available", "inameeting"), false),
            "in a meeting"
        );
        assert_eq!(
            presence_gate_reason(&info("available", "inacall"), false),
            "in a call"
        );
        assert_eq!(
            presence_gate_reason(&info("available", "presenting"), false),
            "presenting"
        );
        assert_eq!(
            presence_gate_reason(&info("focusing", "focusing"), false),
            "focusing"
        );
        assert!(presence_gate_reason(&info("available", "available"), false).is_empty());
    }

    // Issue #3.0-P1: the oid claim (needed for the /users/{oid} fallback)
    // must decode from the JWT payload, and fail cleanly otherwise.
    #[test]
    fn graph_oid_from_access_token_extracts_oid_claim() {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine as _;
        let payload = URL_SAFE_NO_PAD.encode(r#"{"oid":"00000000-0000-0000-0000-000000000000"}"#);
        let token = format!("header.{}.signature", payload);
        assert_eq!(
            super::graph_oid_from_access_token(&token).as_deref(),
            Ok("00000000-0000-0000-0000-000000000000")
        );
    }

    #[test]
    fn graph_oid_from_access_token_errors_cleanly() {
        use base64::Engine as _;
        assert!(super::graph_oid_from_access_token("not-a-jwt").is_err());
        let no_oid =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"sub":"user123"}"#);
        assert!(super::graph_oid_from_access_token(&format!("h.{}.s", no_oid)).is_err());
        let not_json = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode("not json");
        assert!(super::graph_oid_from_access_token(&format!("h.{}.s", not_json)).is_err());
    }

    // Issue #3.0-P1/P2: the Teams JWT `scp` claim (space-separated) must
    // decode for the Settings one-time-reconnect banner.
    #[test]
    fn decode_teams_granted_scopes_extracts_scp_claim() {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine as _;
        let payload =
            URL_SAFE_NO_PAD.encode(format!(r#"{{"scp":"{}"}}"#, super::MICROSOFT_GRAPH_SCOPES));
        let token = format!("h.{}.s", payload);
        let expected: Vec<String> = super::MICROSOFT_GRAPH_SCOPES
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(super::decode_teams_granted_scopes(&token), expected);
    }

    #[test]
    fn decode_teams_granted_scopes_empty_when_not_decodable() {
        use base64::Engine as _;
        assert!(super::decode_teams_granted_scopes("not-a-jwt").is_empty());
        assert!(super::decode_teams_granted_scopes("a.b.c").is_empty());
        let no_scp =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"sub":"user123"}"#);
        assert!(super::decode_teams_granted_scopes(&format!("h.{}.s", no_scp)).is_empty());
    }

    #[test]
    fn parse_retry_after_value_handles_delta_seconds_and_http_date() {
        assert_eq!(super::parse_retry_after_value("120"), Some(120));
        assert_eq!(super::parse_retry_after_value("  42  "), Some(42));
        assert_eq!(super::parse_retry_after_value("9999"), Some(300));
        let future = std::time::SystemTime::now() + std::time::Duration::from_secs(60);
        let http_date = httpdate::fmt_http_date(future);
        let secs = super::parse_retry_after_value(&http_date).expect("http-date must parse");
        assert!(secs <= 60, "future http-date ~60s got {}", secs);
        let far_future = std::time::SystemTime::now() + std::time::Duration::from_secs(10_000);
        let far_date = httpdate::fmt_http_date(far_future);
        assert_eq!(super::parse_retry_after_value(&far_date), Some(300));
        let past = std::time::SystemTime::now() - std::time::Duration::from_secs(60);
        let past_date = httpdate::fmt_http_date(past);
        assert_eq!(super::parse_retry_after_value(&past_date), Some(0));
        assert_eq!(super::parse_retry_after_value("not-a-date"), None);
        assert_eq!(super::parse_retry_after_value(""), None);
    }

    // Issue #348: the device-code endpoint returns a bearer credential
    // (`device_code`), and this function's Err values are error-logged
    // by the caller — so its log/error surface must never carry the
    // response body. Lengths and expiry/interval only. Brace-counted
    // body isolation (order-independent): do not anchor on the next fn.
    #[test]
    fn device_code_flow_logs_no_response_body() {
        let src = include_str!("teams.rs");
        let sig_idx = src
            .find("fn start_teams_auth_device_code()")
            .expect("start_teams_auth_device_code must exist");
        let brace_open_rel = src[sig_idx..]
            .find('{')
            .expect("function body must have an opening brace");
        let body_start = sig_idx + brace_open_rel;
        let mut depth: u32 = 0;
        let mut i = body_start;
        let body_end = loop {
            match src.as_bytes()[i] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        break i;
                    }
                }
                _ => {}
            }
            i += 1;
            if i >= src.len() {
                panic!("unbalanced braces in start_teams_auth_device_code");
            }
        };
        let body = &src[body_start + 1..body_end];
        assert!(
            !body.contains("raw response body"),
            "device-code response body must never be logged"
        );
        assert!(
            !body.contains("truncate_for_log(&raw_body)"),
            "device-code bearer must not reach log/error strings"
        );
        assert!(
            body.contains("expires_in="),
            "device-code receipt must still log expires_in/interval"
        );
        assert!(
            body.contains("User code"),
            "user-code line must stay: the user reads it to sign in"
        );
    }

    /// Extract the argument region of the macro call whose opening `(` is
    /// at `open`, i.e. everything up to the matching `)`. String literals,
    /// char literals and `//` comments are skipped, so a `)` inside a format
    /// string cannot end the region early (mirrors `config.rs::macro_arg_region`).
    fn macro_arg_region(src: &str, open: usize) -> Option<&str> {
        let bytes = src.as_bytes();
        if bytes.get(open) != Some(&b'(') {
            return None;
        }
        let mut depth: i32 = 0;
        let mut i = open;
        while i < bytes.len() {
            match bytes[i] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(&src[open + 1..i]);
                    }
                }
                b'"' => {
                    i += 1;
                    while i < bytes.len() {
                        match bytes[i] {
                            b'\\' => i += 1,
                            b'"' => break,
                            _ => {}
                        }
                        i += 1;
                    }
                }
                b'\'' => {
                    // A char literal (`'x'`, `'\n'`) — a lifetime never
                    // appears in a log macro argument.
                    let mut j = i + 1;
                    if bytes.get(j) == Some(&b'\\') {
                        j += 2;
                    } else {
                        j += 1;
                    }
                    if bytes.get(j) == Some(&b'\'') {
                        i = j;
                    }
                }
                b'/' if bytes.get(i + 1) == Some(&b'/') => {
                    while i < bytes.len() && bytes[i] != b'\n' {
                        i += 1;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        None
    }

    /// Issue #777: every `log::` macro in this module must carry the
    /// `[TEAMS]` module tag, the discipline `config.rs` (`[CFG]`) and
    /// `updater_bg.rs` (`[UPDATER.BG]`) already enforce — a bare
    /// `teams::fn_name:` prefix or no prefix at all makes a Graph line
    /// invisible to a tag grep of `PresenceJam.log`.
    ///
    /// Macro-aware on purpose (region-based, not per-line): eight of the
    /// call sites put the format string on the line AFTER the `log::…!`
    /// opener. The needles are assembled with `concat!` so this test's own
    /// source never contains the literal it searches for — otherwise the
    /// `include_str!` scan would match the test itself and pass vacuously.
    #[test]
    fn teams_log_tags_use_teams_prefix() {
        assert_eq!(super::TAG, "[TEAMS]", "the module tag constant drifted");
        let src = include_str!("teams.rs");
        let needles = [
            concat!("log::", "info!("),
            concat!("log::", "warn!("),
            concat!("log::", "error!("),
            concat!("log::", "debug!("),
            concat!("log::", "trace!("),
        ];
        let mut checked = 0usize;
        for needle in needles {
            let mut from = 0usize;
            while let Some(rel) = src[from..].find(needle) {
                let i = from + rel;
                let open = i + needle.len() - 1;
                let region = macro_arg_region(src, open)
                    .unwrap_or_else(|| panic!("unbalanced macro arguments at byte {i}"));
                assert!(
                    region.contains("{TAG}"),
                    "teams.rs log at byte {i} lacks the [TEAMS] tag: {}",
                    region.replace('\n', " ")
                );
                checked += 1;
                from = open;
            }
        }
        assert!(
            checked >= 20,
            "the scan found only {checked} log macros — the needles are wrong"
        );
    }

    // Issue #912: the info-level status-write line carries the byte count and
    // never the posted text. The text is the user's artist/track, and the
    // rotating file target keeps info by default, so it would ride into the
    // Diagnostics / LogViewer support snapshots as "safe to publish".
    #[test]
    fn status_set_log_line_carries_only_the_byte_count() {
        let marker = "\u{1F3B5} Artist Name - Track Title \u{1F3A7}";
        let line = super::status_set_log_line(marker);
        assert!(
            line.contains(&format!("{} bytes", marker.len())),
            "the size must be reported, got: {line}"
        );
        assert!(!line.contains("Artist Name"), "artist leaked: {line}");
        assert!(!line.contains("Track Title"), "track leaked: {line}");
        assert!(!line.contains(marker), "posted text leaked: {line}");
    }

    /// Issue #912, structural half: the info-level record in
    /// `set_teams_status_message` must be the byte-count helper's output, so
    /// re-inlining the posted text into an info-or-above macro fails here even
    /// if someone deletes the content test above.
    #[test]
    fn set_teams_status_message_info_line_uses_the_byte_count_helper() {
        let src = include_str!("teams.rs");
        let body = fn_body(src, "pub fn set_teams_status_message(");
        // Assembled with `concat!` so this test's own source never contains
        // the macro-opener literal the #777 tag guard scans for — an inline
        // copy of it here leaves that guard's region scan unbalanced.
        let macro_name = concat!("log::", "info!(");
        let at = body
            .find(macro_name)
            .expect("the success line must stay at info level");
        let region = macro_arg_region(body, at + macro_name.len() - 1)
            .expect("the info macro arguments must be balanced");
        assert!(
            region.contains("status_set_log_line("),
            "the info line must log the byte-count helper, got: {region}"
        );
    }

    // ---------------------------------------------------------------
    // Issue #866: setUserPreferredPresence wiring — URL paths, body shape,
    // 404-as-success on the clear.
    // ---------------------------------------------------------------

    /// Issue #866: the preferred-presence URL paths are the documented
    /// `/me/presence/setUserPreferredPresence` and
    /// `/me/presence/clearUserPreferredPresence`. The clear endpoint also
    /// accepts an empty body — a typed struct would diverge from the docs.
    #[test]
    fn preferred_presence_endpoints_use_documented_paths() {
        let src = include_str!("teams.rs");
        assert!(
            src.contains(
                "\"https://graph.microsoft.com/v1.0/me/presence/setUserPreferredPresence\""
            ),
            "setUserPreferredPresence must hit the documented /me path"
        );
        assert!(
            src.contains(
                "\"https://graph.microsoft.com/v1.0/me/presence/clearUserPreferredPresence\""
            ),
            "clearUserPreferredPresence must hit the documented /me path"
        );
        // Mirror `set_teams_presence`'s `/users/{oid}` fallback. The literal
        // `{}` is what `format!` consumes; the test's `oid` placeholder is
        // a separate identifier that would not appear here.
        assert!(
            src.contains("/users/{}/presence/setUserPreferredPresence"),
            "setUserPreferredPresence must mirror setPresence with a /users/oid fallback"
        );
    }

    /// Issue #866: the request body uses the documented `availability` /
    /// `activity` / `expirationDuration` keys (camelCase on the wire). The
    /// clear endpoint posts `{}`, NOT a typed struct, so re-introducing a
    /// struct here would diverge from the docs.
    #[test]
    fn preferred_presence_request_body_is_camel_case() {
        let req = serde_json::json!({
            "availability": "Busy",
            "activity": "Busy",
            "expirationDuration": "PT60M"
        });
        assert_eq!(req["availability"], "Busy");
        assert_eq!(req["activity"], "Busy");
        assert_eq!(req["expirationDuration"], "PT60M");
        // The clear body is the empty object — no schema.
        assert_eq!(serde_json::json!({}).to_string(), "{}");
    }

    /// The body of the function whose signature contains `needle`, isolated by
    /// brace counting from its opening `{` (order-independent — do not anchor
    /// on the next `fn`). Format-string braces are always paired, so counting
    /// stays exact for these bodies.
    fn fn_body<'a>(src: &'a str, needle: &str) -> &'a str {
        let sig = src
            .find(needle)
            .unwrap_or_else(|| panic!("{needle} must exist"));
        let open = sig + src[sig..].find('{').expect("the body must open");
        let mut depth: u32 = 0;
        let mut i = open;
        loop {
            assert!(i < src.len(), "unbalanced braces in {needle}");
            match src.as_bytes()[i] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return &src[open + 1..i];
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }

    // Issue #797: a transient HTTP failure during the multi-minute device-code
    // poll must not end sign-in. The classifier is pure, so the whole matrix is
    // unit-tested without a network.
    #[test]
    fn classify_device_code_response_retries_http_blips() {
        use super::{classify_device_code_response as classify, PollAction};
        // A gateway 502 HTML page — not the OAuth error envelope.
        assert_eq!(
            classify(502, None, "<html><body>502 Bad Gateway</body></html>"),
            PollAction::Retry {
                server_wait: None,
                slow_down: false
            }
        );
        // A 429 without the JSON error object keeps the server's directive.
        assert_eq!(
            classify(429, Some(120), "<html>Too Many Requests</html>"),
            PollAction::Retry {
                server_wait: Some(120),
                slow_down: false
            }
        );
        assert_eq!(
            classify(503, Some(7), ""),
            PollAction::Retry {
                server_wait: Some(7),
                slow_down: false
            }
        );
        assert_eq!(
            classify(408, None, "not json"),
            PollAction::Retry {
                server_wait: None,
                slow_down: false
            }
        );
        // An unparseable 400 is a blip, not a verdict on the device code.
        assert_eq!(
            classify(400, None, "<html>Bad Request</html>"),
            PollAction::Retry {
                server_wait: None,
                slow_down: false
            }
        );
    }

    #[test]
    fn classify_device_code_response_keeps_the_real_verdicts() {
        use super::{classify_device_code_response as classify, PollAction};
        assert_eq!(
            classify(400, None, r#"{"error":"authorization_pending"}"#),
            PollAction::Retry {
                server_wait: None,
                slow_down: false
            }
        );
        assert_eq!(
            classify(400, None, r#"{"error":"slow_down"}"#),
            PollAction::Retry {
                server_wait: None,
                slow_down: true
            }
        );
        // `expired_token` still ends the flow — with a sentence, not a body.
        match classify(
            400,
            None,
            r#"{"error":"expired_token","error_description":"the code expired"}"#,
        ) {
            PollAction::Fail(message) => {
                assert!(
                    message.contains("expired"),
                    "must name the cause: {message}"
                );
                assert!(
                    !message.contains('{'),
                    "a raw JSON body must never reach the user: {message}"
                );
                assert!(
                    !message.contains("raw body"),
                    "the old (raw body: …) suffix must be gone: {message}"
                );
            }
            other => panic!("expired_token must end the flow, got {other:?}"),
        }
        match classify(400, None, r#"{"error":"authorization_declined"}"#) {
            PollAction::Fail(message) => {
                assert!(message.contains("declined"), "got: {message}");
                assert!(!message.contains('{'), "got: {message}");
            }
            other => panic!("authorization_declined must end the flow, got {other:?}"),
        }
        // An unknown code carries the server's description, never the JSON.
        match classify(
            400,
            None,
            r#"{"error":"bad_verification_code","error_description":"The device code is invalid"}"#,
        ) {
            PollAction::Fail(message) => {
                assert!(
                    message.contains("The device code is invalid"),
                    "got: {message}"
                );
                assert!(
                    !message.contains('{'),
                    "a raw JSON body must never reach the user: {message}"
                );
                assert!(
                    !message.contains("raw body"),
                    "the old (raw body: …) suffix must be gone: {message}"
                );
            }
            other => panic!("bad_verification_code must end the flow, got {other:?}"),
        }
        // …and falls back to the code itself when it carries no description.
        match classify(400, None, r#"{"error":"unauthorized_client"}"#) {
            PollAction::Fail(message) => {
                assert!(message.contains("unauthorized_client"), "got: {message}")
            }
            other => panic!("unauthorized_client must end the flow, got {other:?}"),
        }
    }

    /// Issue #797: the retry path is bounded. A server `Retry-After` (clamped
    /// to 300 s) or a ramped `slow_down` interval must never block the thread
    /// past the 900 s deadline, so the sleep gives up as soon as it passes.
    #[test]
    fn sleep_within_deadline_gives_up_after_the_overall_deadline() {
        use std::time::{Duration, Instant};
        // Zero seconds never sleeps and never expires.
        assert_eq!(
            super::sleep_within_deadline(Instant::now(), Duration::from_secs(900), 0),
            Ok(())
        );
        // A deadline already passed: no sleep at all, straight to the error.
        let start = Instant::now();
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(
            super::sleep_within_deadline(start, Duration::from_millis(1), 300),
            Err(super::AUTH_TIMEOUT_MSG.to_string())
        );
    }

    // Issue #974: the UI shows `user_message()`, never `Display`. Display keeps
    // the raw Graph body for logs, but a 403 must reach the user as a sentence
    // naming the permission/licence cause — re-auth cannot fix that one — and
    // no body may leak into a Dashboard banner, an Onboarding error or a
    // Reconnect/Settings message. This test fails while the UI-facing text is
    // the body-printing `Display`.
    #[test]
    fn user_message_is_a_sentence_and_never_a_raw_body() {
        use super::TeamsApiError;
        let body = r#"{"error":{"code":"Forbidden","message":"Insufficient privileges"}}"#;
        let variants = [
            TeamsApiError::ExpiredToken(401),
            TeamsApiError::Forbidden(403, body.to_string()),
            TeamsApiError::RateLimited(Some(120)),
            TeamsApiError::RateLimited(None),
            TeamsApiError::InvalidGrant,
            TeamsApiError::Transient("server error 503: <html>".to_string()),
            TeamsApiError::Other(418, body.to_string()),
        ];
        for variant in &variants {
            let message = variant.user_message();
            assert!(
                !message.contains('{'),
                "raw JSON must never reach the UI ({variant:?}): {message}"
            );
            assert!(
                !message.contains('}'),
                "raw JSON must never reach the UI ({variant:?}): {message}"
            );
            assert!(
                message.ends_with('.'),
                "must read as a sentence ({variant:?}): {message}"
            );
        }

        // The contrast with Display is the point: the body survives for logs…
        for variant in [
            TeamsApiError::Forbidden(403, body.to_string()),
            TeamsApiError::Other(418, body.to_string()),
        ] {
            assert!(
                format!("{variant}").contains("Insufficient privileges"),
                "Display must stay the raw log form: {variant}"
            );
        }
        // …and is absent from what the user reads.
        assert!(!TeamsApiError::Forbidden(403, body.to_string())
            .user_message()
            .contains("Insufficient privileges"));
        assert!(!TeamsApiError::Other(418, body.to_string())
            .user_message()
            .contains("Insufficient privileges"));

        // The 403 remedy names the permission AND the licence, and says why
        // re-authenticating is not the answer.
        let forbidden = TeamsApiError::Forbidden(403, body.to_string()).user_message();
        assert!(
            forbidden.contains("Presence.ReadWrite"),
            "the permission must be named: {forbidden}"
        );
        assert!(
            forbidden.to_lowercase().contains("licen"),
            "the licence must be named: {forbidden}"
        );
        assert!(
            forbidden.contains("Reconnecting will not fix this"),
            "the 403 must rule out re-auth: {forbidden}"
        );

        // Both dead-token variants share the re-auth sentence, and `Other`
        // keeps the status code for diagnosis.
        assert_eq!(
            TeamsApiError::ExpiredToken(401).user_message(),
            TeamsApiError::InvalidGrant.user_message()
        );
        assert!(TeamsApiError::Other(418, body.to_string())
            .user_message()
            .contains("418"));
    }

    /// Issue #884: the shared Graph client must stay memoized. Reintroducing a
    /// builder call in `build_teams_client` would silently restore a fresh
    /// connection pool per Graph call — exactly what the `LazyLock` removed —
    /// and the exit path must keep its own bounded client (3 s, #636).
    #[test]
    fn build_teams_client_uses_one_cached_pool() {
        let src = include_str!("teams.rs");
        // Both needles are assembled with `concat!` so this test's own source
        // never inflates the counts it asserts.
        let builder = concat!("Client::", "builder()");
        let exit_call = concat!("build_teams_client_with_timeout(", "EXIT_CLEANUP_TIMEOUT)");
        assert_eq!(
            src.matches(builder).count(),
            2,
            "exactly two client builders: the cache initializer and the bounded exit-path one"
        );
        let shared = fn_body(src, "fn build_teams_client()");
        // The shared client is built exactly once, and that one builder call
        // must sit inside the process-wide cache: reverting to a per-call
        // client drops this to 0, and adding a second per-call builder beside
        // the cache raises it to 2.
        assert_eq!(
            shared.matches(builder).count(),
            1,
            "the shared client must build once, inside its cache initializer: {shared}"
        );
        assert!(
            shared.contains("static CLIENT: LazyLock"),
            "…and that builder must sit in a process-wide cache: {shared}"
        );
        // The exit path (its own 3 s budget, #636) is the one deliberate
        // exception — a cache keyed by timeout value would defeat the cache.
        let exit_path = fn_body(src, "fn build_teams_client_with_timeout(");
        assert!(
            exit_path.contains(builder),
            "the exit path builds its own bounded client: {exit_path}"
        );
        assert!(
            exit_path.contains(".timeout("),
            "…with an explicit timeout: {exit_path}"
        );
        // Three call sites keep the exit budget (the ephemeral `setPresence`
        // clear, the status-message clear, and the issue #866
        // preferred-presence clear). No other call site may reintroduce a
        // per-call client.
        assert_eq!(
            src.matches(exit_call).count(),
            3,
            "the 3 s client must stay confined to the three exit-path calls"
        );
    }
}
