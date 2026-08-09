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
///区分 expired/unauthorized tokens vs transient errors.
#[derive(Debug, Clone)]
pub enum TeamsApiError {
    /// Token is expired or invalid (401/403) — requires re-auth
    ExpiredToken(u16),
    /// Rate limited (429) — transient, retry after backoff
    RateLimited,
    /// Network error or other transient failure (5xx, send failure)
    Transient(String),
    /// Other non-retryable error
    Other(u16, String),
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
    interval: Option<u64>,
}

pub fn start_teams_auth_device_code() -> Result<DeviceCodeResponse, String> {
    log::info!("teams::start_teams_auth_device_code: starting");

    let client = build_teams_client()?;
    log::info!("teams::start_teams_auth_device_code: client created");

    let params = [
        ("client_id", MICROSOFT_GRAPH_CLIENT_ID),
        // `offline_access` is required for Microsoft to issue a
        // refresh_token (device-code flow docs). `User.Read` is dropped:
        // no Graph call in the app uses it (least privilege, see #151).
        ("scope", "Presence.ReadWrite offline_access"),
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

pub fn poll_teams_auth(device_code: &str) -> Result<TeamsTokens, String> {
    let client = build_teams_client()?;
    let start_time = std::time::Instant::now();
    let timeout = StdDuration::from_secs(900);

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
                log::debug!("Authorization pending, waiting for user to complete login...");
                thread::sleep(StdDuration::from_secs(5));
                continue;
            }
            "authorization_declined" => {
                return Err("Authorization was declined by the user".to_string());
            }
            "slow_down" => {
                let interval = error_resp.interval.unwrap_or(5);
                log::warn!("Server requested slow down, waiting {} seconds", interval);
                thread::sleep(StdDuration::from_secs(interval));
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

pub fn complete_teams_auth(
    code: &str,
    code_verifier: &str,
    client_id: &str,
    redirect_uri: &str,
) -> Result<TeamsTokens, String> {
    let client = build_teams_client()?;

    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", client_id),
        ("code_verifier", code_verifier),
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

    if !status.is_success() {
        log::error!(
            "complete_teams_auth: token request failed with status {}: {}",
            status,
            truncate_for_log(&raw_body)
        );
        return Err(format!(
            "Token request failed: {} - {}",
            status,
            truncate_for_log(&raw_body)
        ));
    }

    #[derive(Deserialize)]
    struct TokenResponse {
        access_token: String,
        #[serde(default)]
        refresh_token: Option<String>,
        expires_in: u64,
        #[allow(dead_code)]
        token_type: String,
    }

    let token_resp: TokenResponse = serde_json::from_str(&raw_body).map_err(|e| {
        format!(
            "Failed to parse token response: {} (body was: {})",
            e,
            truncate_for_log(&raw_body)
        )
    })?;

    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(token_resp.expires_in as i64);

    Ok(TeamsTokens {
        access_token: token_resp.access_token,
        refresh_token: token_resp.refresh_token,
        expires_at,
    })
}

pub fn refresh_teams_token(tokens: &TeamsTokens) -> Result<TeamsTokens, String> {
    let refresh_token = tokens
        .refresh_token
        .as_ref()
        .ok_or("No refresh token available. Please sign in again.")?;

    let client = build_teams_client()?;

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
        .map_err(|e| format!("Failed to send refresh token request: {}", e))?;

    let status = response.status();
    let raw_body = response
        .text()
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    if !status.is_success() {
        log::error!(
            "refresh_teams_token: refresh request failed with status {}: {}",
            status,
            truncate_for_log(&raw_body)
        );
        let error_resp: TokenErrorResponse = serde_json::from_str(&raw_body).map_err(|e| {
            format!(
                "Failed to parse error response: {} (body was: {})",
                e,
                truncate_for_log(&raw_body)
            )
        })?;
        return Err(format!(
            "Failed to refresh token: {} - {} (raw body: {})",
            error_resp.error,
            error_resp.error_description.unwrap_or_default(),
            truncate_for_log(&raw_body)
        ));
    }

    let token_resp: TokenResponse = serde_json::from_str(&raw_body).map_err(|e| {
        format!(
            "Failed to parse token response: {} (body was: {})",
            e,
            truncate_for_log(&raw_body)
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

pub fn set_teams_status_message(
    access_token: &str,
    message: &str,
    expiry_datetime: Option<&str>,
) -> Result<(), String> {
    let client = build_teams_client()?;

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

    let response = client
        .post("https://graph.microsoft.com/v1.0/me/presence/setStatusMessage")
        .header("Authorization", format!("Bearer {}", access_token))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .map_err(|e| format!("Failed to send status message request: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body_text = response
            .text()
            .unwrap_or_else(|_| "Unknown error".to_string());
        log::error!(
            "Failed to set Teams status message: {} - {}",
            status,
            body_text
        );
        return Err(format!(
            "Failed to set status message: {} - {}",
            status, body_text
        ));
    }

    log::info!("Successfully set Teams status message: {}", message);
    Ok(())
}

pub fn clear_teams_status_message(access_token: &str, placeholder: &str) -> Result<(), String> {
    let client = build_teams_client()?;

    let body = StatusMessageRequest {
        status_message: StatusMessageContent {
            message: MessageContent {
                content: placeholder.to_string(),
                content_type: "text".to_string(),
            },
            expiry_date_time: None,
        },
    };

    let response = client
        .post("https://graph.microsoft.com/v1.0/me/presence/setStatusMessage")
        .header("Authorization", format!("Bearer {}", access_token))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .map_err(|e| format!("Failed to send clear status message request: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body_text = response
            .text()
            .unwrap_or_else(|_| "Unknown error".to_string());
        log::error!(
            "Failed to clear Teams status message: {} - {}",
            status,
            body_text
        );
        return Err(format!(
            "Failed to clear status message: {} - {}",
            status, body_text
        ));
    }

    log::info!("Successfully cleared Teams status message");
    Ok(())
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
    match status_code {
        200 => Ok(()),
        401 | 403 => Err(TeamsApiError::ExpiredToken(status_code)),
        429 => Err(TeamsApiError::RateLimited),
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
}