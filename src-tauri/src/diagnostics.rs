//! Telemetry-free local diagnostics snapshot (scope-3.3 candidate C5).
//!
//! Collects a support-oriented snapshot entirely from local state:
//! app/OS version info, a sanitized config summary, token *metadata only*
//! (expiry timestamps + presence flags — never token values), keychain
//! presence flags, and the tail of the on-disk log file with a defensive
//! second-pass redaction applied (`[REDACTED len N]`, same pattern as the
//! #228 auth-log redaction in `pkce::redact_len`).
//!
//! **No network calls. No telemetry endpoint.** This mirrors the
//! SECURITY.md "No Telemetry" promise: everything here can be pasted by
//! the user into a GitHub issue from the Diagnostics page
//! (`src/lib/components/Diagnostics.svelte`).
//!
//! Secret-safety invariant (audited): the returned
//! [`DiagnosticsSnapshot`] struct has no field capable of carrying a
//! token value. Token material lives in `AppState::tokens` (in-memory)
//! and in AES-256-GCM ciphertext at rest; this module only extracts
//! `expires_at` timestamps and presence booleans via the `Tokens`
//! read guards, then drops them before serializing. Regression tests
//! below inject known-fake tokens into an `AppState` and assert the
//! serialized output never contains them.
//!
//! Updater note: the app does not currently persist an updater
//! "last check" timestamp anywhere (frontend updater checks are
//! fire-and-forget on startup), so there is nothing cheaply available to
//! report; the field is intentionally absent rather than stubbed.

use std::fs;

use serde::Serialize;
use tauri::{AppHandle, Manager};

/// Log tag prefix for this module (issue #79 item 3 convention).
const CMD: &str = "[DIAG]";

/// Number of trailing log lines included in the snapshot.
const LOG_TAIL_LINES: usize = 50;

/// Cap on how much of the log file is read (from the end) before
/// splitting lines. Keeps the blocking-pool read bounded for huge logs.
const LOG_TAIL_MAX_BYTES: u64 = 64 * 1024;

/// Name of the log file written by `tauri_plugin_log`'s `LogDir` target
/// (`file_name: Some("PresenceJam")`) — see `lib.rs::run`.
const LOG_FILE_NAME: &str = "PresenceJam.log";

// ---------------------------------------------------------------------
// Snapshot shape (ts-rs exported; regenerated .ts flows through
// `$lib/types` per issue #78)
// ---------------------------------------------------------------------

/// Full local-only diagnostics payload returned by
/// `get_diagnostics_snapshot`. Every field is safe to paste publicly.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct DiagnosticsSnapshot {
    /// Crate version (`CARGO_PKG_VERSION`).
    pub app_version: String,
    /// Tauri crate version.
    pub tauri_version: String,
    pub os: OsInfo,
    /// Sanitized config summary — contains no secrets (the Spotify client
    /// secret lives in the keychain, not config.json; see issue #9).
    pub config: ConfigSummary,
    /// OAuth token metadata — timestamps and presence flags only.
    pub tokens: TokenMetadata,
    /// OS keychain presence flags for the two slots the app uses.
    pub keychain: KeychainStatus,
    /// Last [`LOG_TAIL_LINES`] lines of the on-disk log, each passed
    /// through [`redact_sensitive`].
    pub recent_logs: Vec<String>,
    /// Human-readable status of the log-tail collection (ok/error text).
    pub log_source_status: String,
}

/// Coarse OS identity from `std::env::consts` (no new deps; the
/// `tauri-plugin-os` plugin is deliberately not added for this).
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct OsInfo {
    /// `std::env::consts::OS` (e.g. `"windows"` / `"macos"` / `"linux"`).
    pub platform: String,
    /// `std::env::consts::ARCH` (e.g. `"x86_64"` / `"aarch64"`).
    pub arch: String,
    /// `std::env::consts::FAMILY` (e.g. `"unix"` / `"windows"`).
    pub family: String,
}

/// Non-secret projection of `AppConfig`, flattened field-for-field so a
/// future config addition cannot silently leak into diagnostics without
/// an explicit decision here.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct ConfigSummary {
    /// Spotify client id. Public identifier in the OAuth flow (sent in
    /// every authorize URL), not a credential.
    pub spotify_client_id: String,
    pub redirect_uri: String,
    pub client_secret_set: bool,
    pub clear_on_pause: bool,
    pub profanity_filter: bool,
    pub start_minimized: bool,
    pub availability_sync: bool,
    pub presence_gate: bool,
    pub default_interval_seconds: u64,
    pub minimum_interval_seconds: u64,
    pub maximum_interval_seconds: u64,
    pub expiry_buffer_seconds: u64,
    pub logging_enabled: bool,
    pub log_level: String,
    pub autostart: bool,
}

/// Token metadata ONLY. There is deliberately no field that could carry
/// an access/refresh token value — see the module-level invariant.
#[derive(Debug, Clone, Default, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct TokenMetadata {
    pub spotify_connected: bool,
    /// RFC 3339 expiry timestamp of the Spotify access token.
    pub spotify_expires_at: Option<String>,
    /// True when `expires_at <= now` (i.e. a refresh is due/overdue).
    pub spotify_expired: bool,
    pub teams_connected: bool,
    /// RFC 3339 expiry timestamp of the Teams access token.
    pub teams_expires_at: Option<String>,
    pub teams_expired: bool,
    /// Whether a Teams refresh token exists (Spotify's refresh token is
    /// mandatory whenever connected, so no flag is needed for it).
    pub teams_refresh_token_present: Option<bool>,
}

/// Presence of the two keychain slots the app owns. Booleans only — the
/// values behind them are never read here.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct KeychainStatus {
    /// Spotify `client_secret` present in the OS keychain (issue #9 slot).
    pub spotify_client_secret_present: bool,
    /// `tokens.json` AES-256-GCM key present (issue #140 slot).
    pub tokens_encryption_key_present: bool,
}

// ---------------------------------------------------------------------
// Redaction helper (#228 pattern)
// ---------------------------------------------------------------------

/// Keys whose inline value must never survive into diagnostics. Matched
/// case-insensitively as whole identifiers followed by `=` or `:` (both
/// shell style `code=abc` and JSON style `"code": "abc"`).
const SECRET_KEYS: &[&str] = &[
    "code",
    "state",
    "access_token",
    "refresh_token",
    "client_secret",
    "secret",
    "verifier",
    "device_code",
    "user_code",
    "authorization",
    "bearer",
];

/// Defensive second-pass redaction for one log line, reusing the
/// `[REDACTED len N]` format established by #228 / `pkce::redact_len`.
///
/// The auth/deep-link paths already redact at write time; this catches
/// anything that reaches the log file unredacted (third-party messages,
/// reqwest debug output, future regressions). Two passes:
///
/// 1. **Keyed values** — `<secret-key>` followed by `=` or `:` masks the
///    value up to the next delimiter (whitespace, `&`, `"`, `,`, `}`, or
///    end of line). Handles `code=abc`, `"state": "xyz"`,
///    `Authorization: Bearer abc…`.
/// 2. **Long opaque runs** — any run of ≥ 32 `[A-Za-z0-9_-]` characters
///    (base64/JWT-shaped) is masked regardless of context.
///
/// Conservative by design: over-redaction is acceptable because the
/// page's purpose is human support triage, not log forensics.
pub fn redact_sensitive(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let n = chars.len();
    let mut masked = vec![false; n];
    let lower: Vec<char> = chars.iter().map(|c| c.to_ascii_lowercase()).collect();

    // Pass 1: keyed values.
    for key in SECRET_KEYS {
        let k: Vec<char> = key.chars().collect();
        let mut i = 0;
        while i + k.len() <= n {
            // Whole-identifier match only: reject when flanked by other
            // identifier characters (`codes=` must not match `code`;
            // `_secret=` inside `client_secret=` is handled by its own
            // longer key).
            if lower[i..i + k.len()] == k[..]
                && (i == 0 || !is_ident_char(lower[i - 1]))
                && (i + k.len() == n || !is_ident_char(lower[i + k.len()]))
            {
                // Find the separator: optional whitespace/quotes then '=' or ':'.
                let mut j = i + k.len();
                while j < n && (chars[j].is_whitespace() || chars[j] == '"') {
                    j += 1;
                }
                if j < n && (chars[j] == '=' || chars[j] == ':') {
                    j += 1;
                    // Value starts after optional whitespace and opening quote.
                    while j < n && (chars[j].is_whitespace() || chars[j] == '"') {
                        j += 1;
                    }
                    let value_start = j;
                    while j < n
                        && !chars[j].is_whitespace()
                        && chars[j] != '&'
                        && chars[j] != '"'
                        && chars[j] != ','
                        && chars[j] != '}'
                    {
                        j += 1;
                    }
                    for m in value_start..j {
                        masked[m] = true;
                    }
                    i = j;
                    continue;
                }
            }
            i += 1;
        }
    }

    // Pass 2: long opaque runs.
    let mut run_start: Option<usize> = None;
    for idx in 0..=n {
        let is_opaque = idx < n && is_opaque_char(chars[idx]);
        if is_opaque {
            if run_start.is_none() {
                run_start = Some(idx);
            }
        } else if let Some(start) = run_start {
            if idx - start >= 32 {
                for m in start..idx {
                    masked[m] = true;
                }
            }
            run_start = None;
        }
    }

    // Rebuild: each masked run becomes `[REDACTED len N]` — same pattern
    // as `pkce::redact_len` (#228), inlined here to avoid allocating a
    // dummy string just to measure its length.
    let mut out = String::with_capacity(n);
    let mut idx = 0;
    while idx < n {
        if masked[idx] {
            let run_start = idx;
            while idx < n && masked[idx] {
                idx += 1;
            }
            out.push_str(&format!("[REDACTED len {}]", idx - run_start));
        } else {
            out.push(chars[idx]);
            idx += 1;
        }
    }
    out
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn is_opaque_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

// ---------------------------------------------------------------------
// Collection
// ---------------------------------------------------------------------

/// Extract token metadata from `AppState`. Reads both token slots under
/// their short-lived read guards and copies out only timestamps and
/// presence flags; guards drop before any serialization.
fn token_metadata(state: &crate::AppState) -> TokenMetadata {
    let now = chrono::Utc::now();
    let (spotify_connected, spotify_expires_at, spotify_expired) =
        match state.tokens.spotify().as_ref() {
            Some(t) => (true, Some(t.expires_at.to_rfc3339()), now >= t.expires_at),
            None => (false, None, false),
        };
    let (teams_connected, teams_expires_at, teams_expired, teams_refresh_token_present) =
        match state.tokens.teams().as_ref() {
            Some(t) => (
                true,
                Some(t.expires_at.to_rfc3339()),
                now >= t.expires_at,
                Some(t.refresh_token.is_some()),
            ),
            None => (false, None, false, None),
        };
    TokenMetadata {
        spotify_connected,
        spotify_expires_at,
        spotify_expired,
        teams_connected,
        teams_expires_at,
        teams_expired,
        teams_refresh_token_present,
    }
}

fn config_summary(state: &crate::AppState) -> ConfigSummary {
    let cfg = state.config.get().clone().unwrap_or_default();
    ConfigSummary {
        spotify_client_id: cfg.spotify.client_id,
        redirect_uri: cfg.spotify.redirect_uri,
        client_secret_set: cfg.spotify.client_secret_set,
        clear_on_pause: cfg.teams.clear_on_pause,
        profanity_filter: cfg.teams.profanity_filter,
        start_minimized: cfg.teams.start_minimized,
        availability_sync: cfg.teams.availability_sync,
        presence_gate: cfg.teams.presence_gate,
        default_interval_seconds: cfg.polling.default_interval_seconds,
        minimum_interval_seconds: cfg.polling.minimum_interval_seconds,
        maximum_interval_seconds: cfg.polling.max_interval_seconds,
        expiry_buffer_seconds: cfg.polling.expiry_buffer_seconds,
        logging_enabled: cfg.logging.enabled,
        log_level: cfg.logging.log_level,
        autostart: cfg.autostart,
    }
}

/// Tail the on-disk log file written by `tauri_plugin_log`'s `LogDir`
/// target. Returns up to [`LOG_TAIL_LINES`] redacted lines plus a status
/// string describing what happened (missing file is normal on first run).
fn tail_log_file(log_dir: Option<std::path::PathBuf>) -> (Vec<String>, String) {
    let Some(dir) = log_dir else {
        return (
            Vec::new(),
            "unavailable: could not resolve app log dir".to_string(),
        );
    };
    let path = dir.join(LOG_FILE_NAME);
    if !path.exists() {
        return (Vec::new(), format!("no log file yet at {}", path.display()));
    }
    let collected = (|| -> Result<Vec<String>, String> {
        let len = fs::metadata(&path).map_err(|e| e.to_string())?.len();
        let start = len - len.min(LOG_TAIL_MAX_BYTES);
        let bytes = read_from_offset(&path, start)?;
        let text = String::from_utf8_lossy(&bytes);
        let mut lines: Vec<&str> = text.lines().collect();
        // When we seeked mid-file, drop the (likely partial) first line.
        if start > 0 && !lines.is_empty() {
            lines.remove(0);
        }
        Ok(lines.into_iter().map(|s| s.to_string()).collect())
    })();
    match collected {
        Ok(lines) => {
            let total = lines.len();
            let tail: Vec<String> = lines
                .into_iter()
                .rev()
                .take(LOG_TAIL_LINES)
                .map(|l| redact_sensitive(&l))
                .collect();
            let status = format!("ok: last {} of {} lines", tail.len(), total);
            (tail, status)
        }
        Err(e) => (
            Vec::new(),
            format!("error reading {}: {}", path.display(), e),
        ),
    }
}

fn read_from_offset(path: &std::path::Path, offset: u64) -> Result<Vec<u8>, String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = fs::File::open(path).map_err(|e| e.to_string())?;
    f.seek(SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
}

/// Core assembly, separated from the `#[tauri::command]` wrapper so it
/// stays unit-testable without an `AppHandle` and without touching the
/// OS keychain (tests inject an explicit [`KeychainStatus`]).
fn build_snapshot(
    state: &crate::AppState,
    log_dir: Option<std::path::PathBuf>,
    keychain: KeychainStatus,
) -> DiagnosticsSnapshot {
    log::debug!("{CMD} build_snapshot: collecting local diagnostics");
    let (recent_logs, log_source_status) = tail_log_file(log_dir);
    DiagnosticsSnapshot {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        tauri_version: tauri::VERSION.to_string(),
        os: OsInfo {
            platform: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            family: std::env::consts::FAMILY.to_string(),
        },
        config: config_summary(state),
        tokens: token_metadata(state),
        keychain,
        recent_logs,
        log_source_status,
    }
}

fn probe_keychain() -> KeychainStatus {
    KeychainStatus {
        spotify_client_secret_present: crate::keychain::has_spotify_client_secret(),
        tokens_encryption_key_present: crate::keychain::get_tokens_aes_key().is_ok(),
    }
}

/// Tauri command backing the Diagnostics page. Read-only, local-only,
/// no network calls.
///
/// #215 convention: collection does filesystem IO (log tail) and may
/// touch the OS keychain, so the body runs on the blocking pool.
#[tauri::command]
pub async fn get_diagnostics_snapshot(app: AppHandle) -> Result<DiagnosticsSnapshot, String> {
    log::info!("{CMD} get_diagnostics_snapshot: ENTRY");
    let app_clone = app.clone();
    let snapshot = tauri::async_runtime::spawn_blocking(move || {
        let state = app_clone.state::<crate::AppState>();
        let log_dir = app_clone.path().app_log_dir().ok();
        build_snapshot(&state, log_dir, probe_keychain())
    })
    .await
    .map_err(|e| format!("get_diagnostics_snapshot spawn_blocking panicked: {:?}", e))?;
    log::info!(
        "{CMD} get_diagnostics_snapshot: SUCCESS - {} log lines",
        snapshot.recent_logs.len()
    );
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redact_query_params() {
        assert_eq!(
            redact_sensitive("callback code=abc123&state=xyz99 done"),
            "callback code=[REDACTED len 6]&state=[REDACTED len 5] done"
        );
    }

    #[test]
    fn test_redact_json_style() {
        assert_eq!(
            redact_sensitive(r#"{"access_token": "eyJhbGciOiJIzI1NiJ9", "ok": true}"#),
            r#"{"access_token": "[REDACTED len 19]", "ok": true}"#
        );
    }

    #[test]
    fn test_redact_bearer_header() {
        let out = redact_sensitive("Authorization: Bearer abcdefghijklmnopqrstuvwxyz012345");
        assert!(!out.contains("abcdefghijklmnopqrstuvwxyz012345"));
        assert!(out.contains("[REDACTED"));
    }

    #[test]
    fn test_redact_long_opaque_run_without_key() {
        let long = "a".repeat(40);
        let out = redact_sensitive(&format!("value={}", long));
        assert_eq!(out, format!("value=[REDACTED len {}]", long.len()));
    }

    #[test]
    fn test_keeps_short_benign_values() {
        // Non-keyed short values and ordinary words stay untouched
        // ("interval" is not a secret key; "30s" is below the 32-char
        // opaque-run threshold).
        assert_eq!(
            redact_sensitive("poll interval=30s ok"),
            "poll interval=30s ok"
        );
    }

    #[test]
    fn test_whole_identifier_matching() {
        // `codes=` must not trip the `code` key; `client_secret=` is
        // caught by its own (longer) key entry.
        assert_eq!(
            redact_sensitive("status codes=200"),
            "status codes=[REDACTED len 3]"
        );
        let out = redact_sensitive("client_secret=hunter2 do-not-leak");
        assert!(!out.contains("hunter2"));
    }

    #[test]
    fn test_token_metadata_never_contains_token_values() {
        // Audit test: inject known-fake full tokens into AppState and
        // assert neither value survives serialization.
        let state = crate::AppState::default();
        const FAKE_ACCESS: &str = "fake-access-token-SUPERSECRETVALUE123456";
        const FAKE_REFRESH: &str = "fake-refresh-token-SUPERSECRETVALUE654321";
        const FAKE_TEAMS_ACCESS: &str = "fake-teams-access-EYESONLY0987654321";
        *state.tokens.spotify_mut() = Some(crate::spotify::SpotifyTokens {
            access_token: FAKE_ACCESS.to_string(),
            refresh_token: FAKE_REFRESH.to_string(),
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(3600),
        });
        *state.tokens.teams_mut() = Some(crate::teams::TeamsTokens {
            access_token: FAKE_TEAMS_ACCESS.to_string(),
            refresh_token: None,
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(3600),
        });
        let meta = token_metadata(&state);
        let json = serde_json::to_string(&meta).expect("serialize metadata");
        for secret in [FAKE_ACCESS, FAKE_REFRESH, FAKE_TEAMS_ACCESS] {
            assert!(
                !json.contains(secret),
                "token metadata leaked a token value"
            );
        }
        assert!(meta.spotify_connected);
        assert!(meta.teams_connected);
        assert!(!meta.spotify_expired);
        assert_eq!(meta.teams_refresh_token_present, Some(false));
    }

    #[test]
    fn test_build_snapshot_is_secret_free_with_fake_tokens() {
        let state = crate::AppState::default();
        const FAKE_ACCESS: &str = "audit-canary-access-token-QQWWEERRTTYY";
        *state.tokens.spotify_mut() = Some(crate::spotify::SpotifyTokens {
            access_token: FAKE_ACCESS.to_string(),
            refresh_token: FAKE_ACCESS.to_string(),
            expires_at: chrono::Utc::now() + chrono::Duration::seconds(60),
        });
        let dir = std::env::temp_dir();
        let keychain = KeychainStatus {
            spotify_client_secret_present: true,
            tokens_encryption_key_present: true,
        };
        let snapshot = build_snapshot(&state, Some(dir), keychain);
        let json = serde_json::to_string_pretty(&snapshot).expect("serialize snapshot");
        assert!(
            !json.contains(FAKE_ACCESS),
            "diagnostics snapshot leaked a token value"
        );
        assert_eq!(snapshot.app_version, env!("CARGO_PKG_VERSION"));
        assert!(!snapshot.os.platform.is_empty());
        assert!(snapshot.config.client_secret_set);
    }

    #[test]
    fn test_tail_log_file_missing_and_redacts() {
        let dir = std::env::temp_dir().join(format!("pj-diag-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (lines, status) = tail_log_file(Some(dir.clone()));
        assert!(lines.is_empty());
        assert!(status.contains("no log file yet"));

        let log_path = dir.join(LOG_FILE_NAME);
        std::fs::write(&log_path, "[AUTH] code=hunter2secret\n[AUTH] clean line\n").unwrap();
        let (lines, _) = tail_log_file(Some(dir.clone()));
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "[AUTH] code=[REDACTED len 13]");
        assert_eq!(lines[1], "[AUTH] clean line");
        std::fs::remove_dir_all(&dir).ok();
    }
}
