use base64::Engine;
use chrono::Utc;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::thread;
use std::time::Duration as StdDuration;

pub const MICROSOFT_GRAPH_CLIENT_ID: &str = "14d82eec-204b-4c2f-b7e8-296a70dab67e";

/// Creates a reqwest blocking client with standard config (user agent + 10s timeout).
/// Ensures consistent HTTP client settings across all Teams API calls.
fn build_teams_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent("PresenceJam/2.0")
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))
}

pub fn pkce_generate_verifier() -> String {
    let mut bytes = [0u8; 64];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub fn pkce_generate_challenge(verifier: &str) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(verifier.as_bytes());
    let hash = hasher.finalize();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hash)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamsTokens {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    pub expires_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCodeResponse {
    pub user_code: String,
    pub verification_url: String,
    pub device_code: String,
    pub interval: u64,
    pub expires_in: u64,
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponseRaw {
    user_code: String,
    #[serde(alias = "verification_url", rename = "verification_uri")]
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
        ("scope", "Presence.ReadWrite User.Read"),
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
        raw_body
    );

    if !status.is_success() {
        return Err(format!(
            "Device code request failed with status {}: {}",
            status, raw_body
        ));
    }

    let raw: DeviceCodeResponseRaw = serde_json::from_str(&raw_body).map_err(|e| {
        format!(
            "Failed to parse device code response: {} (body was: {})",
            e, raw_body
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
        log::debug!("poll_teams_auth: status={}, body={}", status, raw_body);

        if status.is_success() {
            let token_resp: TokenResponse = serde_json::from_str(&raw_body).map_err(|e| {
                format!(
                    "Failed to parse token response: {} (body was: {})",
                    e, raw_body
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
                e, raw_body
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
                    raw_body
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
            raw_body
        );
        return Err(format!("Token request failed: {} - {}", status, raw_body));
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
            e, raw_body
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
            raw_body
        );
        let error_resp: TokenErrorResponse = serde_json::from_str(&raw_body).map_err(|e| {
            format!(
                "Failed to parse error response: {} (body was: {})",
                e, raw_body
            )
        })?;
        return Err(format!(
            "Failed to refresh token: {} - {} (raw body: {})",
            error_resp.error,
            error_resp.error_description.unwrap_or_default(),
            raw_body
        ));
    }

    let token_resp: TokenResponse = serde_json::from_str(&raw_body).map_err(|e| {
        format!(
            "Failed to parse token response: {} (body was: {})",
            e, raw_body
        )
    })?;

    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(token_resp.expires_in as i64);

    log::info!("Successfully refreshed Microsoft Teams token");

    Ok(TeamsTokens {
        access_token: token_resp.access_token,
        refresh_token: token_resp.refresh_token,
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

pub fn clear_teams_status_message(access_token: &str) -> Result<(), String> {
    let client = build_teams_client()?;

    let body = StatusMessageRequest {
        status_message: StatusMessageContent {
            message: MessageContent {
                content: String::new(),
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

/// Validates that a Teams access token is still functional by calling the
/// presence endpoint. Returns Ok(()) if the token works (200), Err(()) for
/// permanent auth failures (401/403).
pub fn validate_teams_token(access_token: &str) -> Result<(), String> {
    let client = build_teams_client()?;
    let response = client
        .get("https://graph.microsoft.com/v1.0/me/presence")
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .map_err(|e| format!("request failed: {}", e))?;

    let status_code = response.status().as_u16();
    match status_code {
        200 => Ok(()),
        401 | 403 => Err(format!("token invalid ({}), reconnect required", status_code)),
        _ => {
            let body = response.text().unwrap_or_default();
            Err(format!("unexpected status {}: {}", status_code, body))
        }
    }
}

