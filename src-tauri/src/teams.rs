use chrono::Utc;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use base64::Engine;
use sha2::Digest;
use std::time::Duration as StdDuration;
use std::thread;

pub const MICROSOFT_GRAPH_CLIENT_ID: &str = "14d82eec-204b-4c2f-b7e8-296a70dab67e";

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
    pub refresh_token: String,
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
    verification_url: String,
    device_code: String,
    interval: u64,
    expires_in: u64,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
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
    let client = reqwest::blocking::Client::new();
    log::info!("teams::start_teams_auth_device_code: client created");

    let params = [
        ("client_id", MICROSOFT_GRAPH_CLIENT_ID),
        ("scope", "Presence.ReadWrite User.Read"),
    ];
    log::info!("teams::start_teams_auth_device_code: calling devicecode endpoint");

    let response = client
        .post("https://login.microsoftonline.com/common/oauth2/v2.0/devicecode")
        .form(&params)
        .send()
        .map_err(|e| {
            log::error!("teams::start_teams_auth_device_code: send failed: {}", e);
            format!("Failed to send device code request: {}", e)
        })?;
    log::info!("teams::start_teams_auth_device_code: send succeeded");

    let raw: DeviceCodeResponseRaw = response
        .json()
        .map_err(|e| format!("Failed to parse device code response: {}", e))?;
    log::info!("teams::start_teams_auth_device_code: parsed response");

    let result = DeviceCodeResponse {
        user_code: raw.user_code,
        verification_url: raw.verification_url,
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
    let client = reqwest::blocking::Client::new();
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
            .form(&params)
            .send()
            .map_err(|e| format!("Failed to send token request: {}", e))?;

        let status = response.status();

        if status.is_success() {
            let token_resp: TokenResponse = response
                .json()
                .map_err(|e| format!("Failed to parse token response: {}", e))?;

            let expires_at = chrono::Utc::now() + chrono::Duration::seconds(token_resp.expires_in as i64);

            log::info!("Successfully authenticated with Microsoft Teams");

            return Ok(TeamsTokens {
                access_token: token_resp.access_token,
                refresh_token: token_resp.refresh_token,
                expires_at,
            });
        }

        let error_resp: TokenErrorResponse = response
            .json()
            .map_err(|e| format!("Failed to parse error response: {}", e))?;

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
                return Err("The device code has expired. Please start authentication again.".to_string());
            }
            _ => {
                return Err(format!(
                    "Authentication failed: {} - {}",
                    error_resp.error,
                    error_resp.error_description.unwrap_or_default()
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
    let client = reqwest::blocking::Client::new();

    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", client_id),
        ("code_verifier", code_verifier),
    ];

    let response = client
        .post("https://login.microsoftonline.com/common/oauth2/v2.0/token")
        .form(&params)
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

    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(token_resp.expires_in as i64);

    Ok(TeamsTokens {
        access_token: token_resp.access_token,
        refresh_token: token_resp.refresh_token,
        expires_at,
    })
}

pub fn refresh_teams_token(tokens: &TeamsTokens) -> Result<TeamsTokens, String> {
    let client = reqwest::blocking::Client::new();

    let params = [
        ("grant_type", "refresh_token"),
        ("client_id", MICROSOFT_GRAPH_CLIENT_ID),
        ("refresh_token", &tokens.refresh_token),
    ];

    let response = client
        .post("https://login.microsoftonline.com/common/oauth2/v2.0/token")
        .form(&params)
        .send()
        .map_err(|e| format!("Failed to send refresh token request: {}", e))?;

    if !response.status().is_success() {
        let error_resp: TokenErrorResponse = response
            .json()
            .map_err(|e| format!("Failed to parse error response: {}", e))?;
        return Err(format!(
            "Failed to refresh token: {} - {}",
            error_resp.error,
            error_resp.error_description.unwrap_or_default()
        ));
    }

    let token_resp: TokenResponse = response
        .json()
        .map_err(|e| format!("Failed to parse token response: {}", e))?;

    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(token_resp.expires_in as i64);

    log::info!("Successfully refreshed Microsoft Teams token");

    Ok(TeamsTokens {
        access_token: token_resp.access_token,
        refresh_token: token_resp.refresh_token,
        expires_at,
    })
}

#[derive(Debug, Serialize)]
struct StatusMessageRequest {
    status_message: StatusMessageContent,
}

#[derive(Debug, Serialize)]
struct StatusMessageContent {
    message: MessageContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    expiry_date_time: Option<ExpiryDateTime>,
}

#[derive(Debug, Serialize)]
struct MessageContent {
    content: String,
    content_type: String,
}

#[derive(Debug, Serialize)]
struct ExpiryDateTime {
    date_time: String,
    time_zone: String,
}

pub fn set_teams_status_message(
    access_token: &str,
    message: &str,
    expiry_datetime: Option<&str>,
) -> Result<(), String> {
    let client = reqwest::blocking::Client::new();

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
        let body_text = response.text().unwrap_or_else(|_| "Unknown error".to_string());
        log::error!("Failed to set Teams status message: {} - {}", status, body_text);
        return Err(format!("Failed to set status message: {} - {}", status, body_text));
    }

    log::info!("Successfully set Teams status message: {}", message);
    Ok(())
}

pub fn clear_teams_status_message(access_token: &str) -> Result<(), String> {
    let client = reqwest::blocking::Client::new();

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
        let body_text = response.text().unwrap_or_else(|_| "Unknown error".to_string());
        log::error!("Failed to clear Teams status message: {} - {}", status, body_text);
        return Err(format!("Failed to clear status message: {} - {}", status, body_text));
    }

    log::info!("Successfully cleared Teams status message");
    Ok(())
}
