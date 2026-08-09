use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::thread;
use std::time::Duration as StdDuration;

pub const MICROSOFT_GRAPH_CLIENT_ID: &str = "14d82eec-204b-4c2f-b7e8-296a70dab67e";

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

/// Creates a reqwest blocking client with standard config (user agent + 10s timeout).
/// Ensures consistent HTTP client settings across all Teams API calls.
///
/// User-Agent uses `env!("CARGO_PKG_VERSION")` so it tracks `Cargo.toml`
/// (which mirrors `tauri.conf.json` → `version`) automatically on every
/// release. Never hardcode the version — see CONTRIBUTING.md. See audit
/// Q8.
fn build_teams_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent(format!("PresenceJam/{}", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))
}

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
    log::info!("teams::start_teams_auth_device_code: starting");

    let client = build_teams_client()?;
    log::info!("teams::start_teams_auth_device_code: client created");

    let params = [
        ("client_id", MICROSOFT_GRAPH_CLIENT_ID),
        // `offline_access` is required for Microsoft to issue a
        // refresh_token (device-code flow docs). `Presence.Read` powers the
        // presence-aware status gate (getPresence, issue #3.0-P2) and
        // `profile` adds the `oid` claim to the access-token JWT so the
        // setPresence/clearPresence /users/{oid} fallback can resolve the
        // user (docs list only /users/{id}; see issue #3.0-P1). `User.Read`
        // stays dropped: no Graph call uses it (least privilege, #151).
        ("scope", "Presence.ReadWrite Presence.Read profile offline_access"),
    ];
    log::info!("teams::start_teams_auth_device_code: calling devicecode endpoint");

    let response = client
        .post("https://login.microsoftonline.com/common/oauth2/v2.0/devicecode")
        .header("Accept", "application/json")
        .form(&params)
        .send()
        .map_err(|e| {
            log::error!("teams::start_teams_auth_device_code: send failed: {}", e);
            format!("Failed to send device code request: {}", e)
        })?;
    log::info!("teams::start_teams_auth_device_code: send succeeded");

    let status = response.status();
    log::info!(
        "teams::start_teams_auth_device_code: response status: {}",
        status
    );

    let raw_body = response.text().map_err(|e| {
        log::error!(
            "teams::start_teams_auth_device_code: failed to read body: {}",
            e
        );
        format!("Failed to read response body: {}", e)
    })?;
    log::info!(
        "teams::start_teams_auth_device_code: raw response body: {}",
        truncate_for_log(&raw_body)
    );

    if !status.is_success() {
        return Err(format!(
            "Device code request failed with status {}: {}",
            status,
            truncate_for_log(&raw_body)
        ));
    }

    let raw: DeviceCodeResponseRaw = serde_json::from_str(&raw_body).map_err(|e| {
        format!(
            "Failed to parse device code response: {} (body was: {})",
            e,
            truncate_for_log(&raw_body)
        )
    })?;
    log::info!("teams::start_teams_auth_device_code: parsed response");

    let result = DeviceCodeResponse {
        user_code: raw.user_code,
        verification_url: raw.verification_uri,
        device_code: raw.device_code.clone(),
        interval: raw.interval,
        expires_in: raw.expires_in,
    };

    log::info!(
        "Device code flow started. User code: {}, verification URL: {}",
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

/// Parses a `Retry-After` header (plain seconds, per Graph throttling
/// docs) into an optional delay. Returns None when the header is absent
/// or unparseable — callers then fall back to exponential backoff.
fn parse_retry_after(response: &reqwest::blocking::Response) -> Option<u64> {
    response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
}

pub fn poll_teams_auth(device_code: &str, interval: u64) -> Result<TeamsTokens, String> {
    let client = build_teams_client()?;
    let start_time = std::time::Instant::now();
    let timeout = StdDuration::from_secs(900);

    // RFC 8628 §3.2/§3.5: wait at least the server-provided `interval`
    // between polls, or 5s when none was provided.
    let mut wait = if interval == 0 { 5 } else { interval };

    loop {
        if start_time.elapsed() > timeout {
            return Err("Authentication timed out".to_string());
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

        let raw_body = response
            .text()
            .map_err(|e| format!("Failed to read response body: {}", e))?;
        log::debug!(
            "poll_teams_auth: status={}, body={}",
            status,
            truncate_for_log(&raw_body)
        );

        if status.is_success() {
            let token_resp: TokenResponse = serde_json::from_str(&raw_body).map_err(|e| {
                format!(
                    "Failed to parse token response: {} (body was: {})",
                    e,
                    truncate_for_log(&raw_body)
                )
            })?;

            let expires_at =
                chrono::Utc::now() + chrono::Duration::seconds(token_resp.expires_in as i64);

            log::info!("Successfully authenticated with Microsoft Teams");

            return Ok(TeamsTokens {
                access_token: token_resp.access_token,
                refresh_token: token_resp.refresh_token,
                expires_at,
            });
        }

        let error_resp: TokenErrorResponse = serde_json::from_str(&raw_body).map_err(|e| {
            format!(
                "Failed to parse error response: {} (body was: {})",
                e,
                truncate_for_log(&raw_body)
            )
        })?;

        match error_resp.error.as_str() {
            "authorization_pending" => {
                log::debug!("Authorization pending, waiting {} seconds", wait);
                thread::sleep(StdDuration::from_secs(wait));
                continue;
            }
            "authorization_declined" => {
                return Err("Authorization was declined by the user".to_string());
            }
            "slow_down" => {
                // RFC 8628 §3.5: slow_down carries no interval; the
                // client must increase its polling interval by 5s for
                // this and all subsequent requests.
                wait = next_poll_wait(wait, error_resp.error.as_str());
                log::warn!("Server requested slow down, waiting {} seconds", wait);
                thread::sleep(StdDuration::from_secs(wait));
                continue;
            }
            "expired_token" => {
                return Err(
                    "The device code has expired. Please start authentication again.".to_string(),
                );
            }
            _ => {
                return Err(format!(
                    "Authentication failed: {} - {} (raw body: {})",
                    error_resp.error,
                    error_resp.error_description.unwrap_or_default(),
                    truncate_for_log(&raw_body)
                ));
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
            "refresh_teams_token: refresh request failed with status {}: {}",
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

    log::info!("Successfully refreshed Microsoft Teams token");

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

/// POSTs a status message body to the Graph setStatusMessage endpoint and
/// maps the response to a typed error. Shared by the set and clear paths so
/// both get identical status-code discrimination (401 vs 403 vs 429 vs 5xx)
/// and `Retry-After` parsing. See issues #153/#154.
fn post_status_message(
    access_token: &str,
    body: &StatusMessageRequest,
    action: &str,
) -> Result<(), TeamsApiError> {
    let client = build_teams_client().map_err(TeamsApiError::Transient)?;

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
        log::error!("Failed to {} Teams status message: {} - {}", action, status, body_text);
        return Err(match status_code {
            401 => TeamsApiError::ExpiredToken(status_code),
            403 => TeamsApiError::Forbidden(status_code, body_text),
            429 => TeamsApiError::RateLimited(retry_after),
            500..=599 => {
                TeamsApiError::Transient(format!("server error {}: {}", status_code, body_text))
            }
            _ => TeamsApiError::Other(status_code, body_text),
        });
    }

    Ok(())
}

pub fn set_teams_status_message(
    access_token: &str,
    message: &str,
    expiry_datetime: Option<&str>,
) -> Result<(), TeamsApiError> {
    let expiry = expiry_datetime.map(|dt| ExpiryDateTime {
        date_time: dt.to_string(),
        time_zone: "UTC".to_string(),
    });

    let body = StatusMessageRequest {
        status_message: StatusMessageContent {
            message: MessageContent {
                content: message.to_string(),
                content_type: "text".to_string(),
            },
            expiry_date_time: expiry,
        },
    };

    post_status_message(access_token, &body, "set")?;

    log::info!("Successfully set Teams status message: {}", message);
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
    let expiry = expiry_datetime.map(|dt| ExpiryDateTime {
        date_time: dt.to_string(),
        time_zone: "UTC".to_string(),
    });

    let body = StatusMessageRequest {
        status_message: StatusMessageContent {
            message: MessageContent {
                content: placeholder.to_string(),
                content_type: "text".to_string(),
            },
            expiry_date_time: expiry,
        },
    };

    post_status_message(access_token, &body, "clear")?;

    log::info!("Successfully cleared Teams status message");
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

/// Presence returned by the Graph getPresence endpoint (v1.0). The docs
/// enumerate lowercase enum values (`available`, `busy`, …) but real
/// examples return PascalCase (`Available`, `Busy`, `InACall`, …) — parse
/// case-insensitively (issue #3.0-P1/P2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresenceInfo {
    pub availability: String,
    pub activity: String,
}

/// Parses a Graph getPresence response body into a `PresenceInfo`,
/// normalizing both enum fields to lower-case so callers compare once.
pub fn parse_presence_body(body: &str) -> Result<PresenceInfo, String> {
    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| format!("Failed to parse presence body: {}", e))?;
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
    })
}

/// Human-readable reason a presence gates a status-message write, or an
/// empty string when it doesn't. Single source of truth for the gating
/// rule — `is_presence_gated` is defined through it so the two cannot
/// drift apart (issue #3.0-P2).
pub fn presence_gate_reason(presence: &PresenceInfo) -> String {
    match presence.activity.to_lowercase().as_str() {
        "inameeting" => return "in a meeting".to_string(),
        "inacall" => return "in a call".to_string(),
        "presenting" => return "presenting".to_string(),
        _ => {}
    }
    match presence.availability.to_lowercase().as_str() {
        "busy" => "busy".to_string(),
        "donotdisturb" => "Do Not Disturb".to_string(),
        _ => String::new(),
    }
}

/// True iff a presence should suppress a status-message write: the user is
/// busy or Do-Not-Disturb, or their activity is in a meeting/call or
/// presenting. Case-insensitive — both fields are normalized internally
/// (issue #3.0-P2).
pub fn is_presence_gated(presence: &PresenceInfo) -> bool {
    !presence_gate_reason(presence).is_empty()
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

/// POSTs a JSON body to a Graph presence endpoint and maps the response to
/// a typed error — the same status-code discrimination and `Retry-After`
/// parsing as `post_status_message` (issues #153/#154). A 404 is surfaced
/// as `Other(404, …)` so callers can retry the documented `/users/{oid}`
/// path (the setPresence/clearPresence docs list only `/users/{id}`; `/me`
/// works in practice but is undocumented).
fn post_presence<T: Serialize>(
    access_token: &str,
    url: &str,
    body: &T,
    action: &str,
) -> Result<(), TeamsApiError> {
    let client = build_teams_client().map_err(TeamsApiError::Transient)?;

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
            "Failed to {} Teams presence: {} - {}",
            action,
            status,
            body_text
        );
        return Err(match status_code {
            401 => TeamsApiError::ExpiredToken(status_code),
            403 => TeamsApiError::Forbidden(status_code, body_text),
            429 => TeamsApiError::RateLimited(retry_after),
            500..=599 => TeamsApiError::Transient(format!(
                "server error {}: {}",
                status_code, body_text
            )),
            _ => TeamsApiError::Other(status_code, body_text),
        });
    }

    Ok(())
}

/// Sets the user's Teams presence via the Graph setPresence endpoint
/// (issue #3.0-P1). `availability`/`activity` must be a documented combo
/// (e.g. `Available`/`Available`, `Busy`/`InACall`) and
/// `expiration_duration` a `PT5M`-`PT4H` ISO-8601 duration (default PT5M;
/// the app re-arms well inside the window because Available sessions FADE
/// after 5 min regardless). `sessionId` MUST be the app's Azure AD client
/// id (Microsoft Learn v1.0 docs).
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
    match post_presence(
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
    let body = ClearPresenceRequest {
        session_id: MICROSOFT_GRAPH_CLIENT_ID.to_string(),
    };
    match post_presence(
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
        .map_err(|e| {
            TeamsApiError::Transient(format!("Failed to get Teams presence: {}", e))
        })?;

    let status = response.status();
    let status_code = status.as_u16();
    let retry_after = parse_retry_after(&response);
    let body_text = response
        .text()
        .unwrap_or_else(|_| "Unknown error".to_string());

    if !status.is_success() {
        log::error!(
            "Failed to get Teams presence: {} - {}",
            status,
            body_text
        );
        return Err(match status_code {
            401 => TeamsApiError::ExpiredToken(status_code),
            403 => TeamsApiError::Forbidden(status_code, body_text),
            429 => TeamsApiError::RateLimited(retry_after),
            500..=599 => TeamsApiError::Transient(format!(
                "server error {}: {}",
                status_code, body_text
            )),
            _ => TeamsApiError::Other(status_code, body_text),
        });
    }

    parse_presence_body(&body_text)
        .map_err(|e| TeamsApiError::Other(200, format!("Failed to parse presence: {}", e)))
}

/// Validates that a Teams access token is still functional.
///
/// Short-circuits on the local `expires_at` field when the token is clearly
/// still good (more than 60s of lifetime remaining), so the typical
/// Onboarding mount doesn't pay for a network round-trip. Only when the
/// token is on the refresh boundary (or already past it) do we make a real
/// HTTP call to confirm.
///
/// Returns Ok(()) if the token works (locally valid OR 200), or
/// Err(TeamsApiError) on failure. Callers should distinguish:
///
/// - `ExpiredToken` → permanent auth failure, re-auth required
/// - `Forbidden` → permission/license problem; re-auth won't help (403)
/// - `RateLimited` / `Transient` → temporary, treat as "valid enough" for onboarding
/// - `Other` → non-retryable but onboarding may still proceed
pub fn validate_teams_token(tokens: &TeamsTokens) -> Result<(), TeamsApiError> {
    // Local pre-check: if the token clearly has plenty of life left, skip
    // the network call. Mirrors the 60s refresh window used elsewhere.
    if !is_token_expired(tokens) {
        return Ok(());
    }

    let client = build_teams_client().map_err(TeamsApiError::Transient)?;
    let response = client
        .get("https://graph.microsoft.com/v1.0/me/presence")
        .header("Authorization", format!("Bearer {}", tokens.access_token))
        .send()
        .map_err(|e| TeamsApiError::Transient(format!("request failed: {}", e)))?;

    let status_code = response.status().as_u16();
    let retry_after = parse_retry_after(&response);
    match status_code {
        200 => Ok(()),
        // 401 = token missing/invalid → re-auth required; 403 = no
        // permission/license (or conditional-access insufficient_claims)
        // → re-auth won't help, surface as its own error. See #153.
        401 => Err(TeamsApiError::ExpiredToken(status_code)),
        403 => {
            let body = response.text().unwrap_or_default();
            Err(TeamsApiError::Forbidden(status_code, body))
        }
        429 => Err(TeamsApiError::RateLimited(retry_after)),
        500..=599 => {
            let body = response.text().unwrap_or_default();
            Err(TeamsApiError::Transient(format!(
                "server error {}: {}",
                status_code, body
            )))
        }
        _ => {
            let body = response.text().unwrap_or_default();
            Err(TeamsApiError::Other(status_code, body))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::truncate_for_log;
    use super::{DeviceCodeResponse, TeamsTokens};

    #[test]
    fn test_truncate_under_limit() {
        let body = "short body".to_string();
        assert_eq!(truncate_for_log(&body), body);
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
        let parsed: TeamsTokens =
            serde_json::from_str(&json).expect("deserialize");
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
        let parsed: TeamsTokens =
            serde_json::from_str(&json).expect("deserialize");
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
        let json: serde_json::Value =
            serde_json::to_value(&resp).expect("to_value");
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
        let info = super::parse_presence_body(
            r#"{"availability":"Available","activity":"Available"}"#,
        )
        .expect("PascalCase body must parse");
        assert_eq!(info.availability, "available");
        assert_eq!(info.activity, "available");

        let info = super::parse_presence_body(
            r#"{"availability":"Busy","activity":"InAMeeting"}"#,
        )
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

    // Issue #3.0-P2: gating rule — busy/DND availability OR
    // in-meeting/in-call/presenting activity, case-insensitive.
    #[test]
    fn is_presence_gated_covers_busy_and_meeting_states() {
        use super::{is_presence_gated, PresenceInfo};
        let info = |availability: &str, activity: &str| PresenceInfo {
            availability: availability.to_string(),
            activity: activity.to_string(),
        };
        assert!(is_presence_gated(&info("busy", "available")));
        assert!(is_presence_gated(&info("donotdisturb", "available")));
        assert!(is_presence_gated(&info("available", "inameeting")));
        assert!(is_presence_gated(&info("available", "inacall")));
        assert!(is_presence_gated(&info("available", "presenting")));
        // Activity wins even when availability is Available (in-meeting).
        assert!(is_presence_gated(&info("available", "InAMeeting")));
        assert!(!is_presence_gated(&info("available", "available")));
        assert!(!is_presence_gated(&info("away", "away")));
        assert!(!is_presence_gated(&info("available", "offline")));
    }

    // Issue #3.0-P2: the human-readable reason must mirror the gating rule
    // (is_presence_gated is defined through it, so they cannot drift).
    #[test]
    fn presence_gate_reason_mirrors_gating_rule() {
        use super::{presence_gate_reason, PresenceInfo};
        let info = |availability: &str, activity: &str| PresenceInfo {
            availability: availability.to_string(),
            activity: activity.to_string(),
        };
        assert_eq!(presence_gate_reason(&info("busy", "available")), "busy");
        assert_eq!(
            presence_gate_reason(&info("donotdisturb", "available")),
            "Do Not Disturb"
        );
        assert_eq!(
            presence_gate_reason(&info("available", "inameeting")),
            "in a meeting"
        );
        assert_eq!(presence_gate_reason(&info("available", "inacall")), "in a call");
        assert_eq!(
            presence_gate_reason(&info("available", "presenting")),
            "presenting"
        );
        assert!(presence_gate_reason(&info("available", "available")).is_empty());
    }

    // Issue #3.0-P1: the oid claim (needed for the /users/{oid} fallback)
    // must decode from the JWT payload, and fail cleanly otherwise.
    #[test]
    fn graph_oid_from_access_token_extracts_oid_claim() {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine as _;
        let payload =
            URL_SAFE_NO_PAD.encode(r#"{"oid":"00000000-0000-0000-0000-000000000000"}"#);
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
        let no_oid = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(r#"{"sub":"user123"}"#);
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
        let payload = URL_SAFE_NO_PAD.encode(
            r#"{"scp":"Presence.ReadWrite Presence.Read profile offline_access"}"#,
        );
        let token = format!("h.{}.s", payload);
        assert_eq!(
            super::decode_teams_granted_scopes(&token),
            vec![
                "Presence.ReadWrite".to_string(),
                "Presence.Read".to_string(),
                "profile".to_string(),
                "offline_access".to_string(),
            ]
        );
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
}