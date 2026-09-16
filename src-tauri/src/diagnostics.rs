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

/// File-name stem for [`save_diagnostics_snapshot`] (issue #598): the
/// snapshot is written into the platform downloads directory, which is
/// where the old synthetic `<a download>` click claimed to put it.
const SNAPSHOT_FILE_STEM: &str = "presencejam-diagnostics";

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
    /// Exit-time update install that failed on a previous run (issue #244),
    /// read from the marker written by `updater_bg::install_pending_on_exit`
    /// and sanitized at collection time (issue #603): `error` is passed
    /// through [`redact_sensitive`], so a credential-shaped run or a
    /// username-bearing path in the raw updater text cannot reach a public
    /// issue. `None` when the last exit-time install succeeded or none was
    /// attempted.
    pub failed_update_install: Option<crate::updater_bg::FailedUpdateInstall>,
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
    /// Issue #432: rule counts only (substrings/replacements are user
    /// content — never snapshot them). Keeps the "explicit field per
    /// config value" invariant without leaking rule text.
    pub quiet_hours_count: usize,
    pub quiet_hours_enabled_count: usize,
    pub track_rules_count: usize,
    pub track_rules_enabled_count: usize,
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
/// shell style `code=abc` and JSON style `"code": "abc"`, either quote
/// style). Compound keys (`id_token`, `code_verifier`, `code_challenge`) are listed
/// explicitly because the whole-identifier check rejects `_`-flanked
/// substrings, so bare `token`/`code`/`verifier` never match inside them
/// (and vice versa: bare `token` cannot match inside `access_token`).
const SECRET_KEYS: &[&str] = &[
    "api_key",
    "code",
    "state",
    "access_token",
    "refresh_token",
    "id_token",
    "token",
    "client_secret",
    "secret",
    "password",
    "passwd",
    "verifier",
    "code_challenge",
    "code_verifier",
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
/// 1. **Keyed values** — `<secret-key>` followed by `=` or `:` (either
///    quote style) masks the value up to the next delimiter
///    (whitespace, `&`, `"`, `'`, `,`, `}`, or end of line). Handles
///    `code=abc`, `"state": "xyz"`, `'token': 'abc'`,
///    `Authorization: Bearer abc…`. The device-code keys (`user_code`,
///    `device_code`) additionally accept a bare-whitespace gap
///    (`user_code XXXX-XXXX`).
/// 2. **Long opaque runs** — any run of ≥ 32
///    `[A-Za-z0-9_./+-]` characters (base64/JWT-shaped, including
///    dotted segments) is masked regardless of context. `=` joins the
///    run unless it acts as a `key=value` separator, so `value=<long>`
///    keeps its key name while `a.b/c+d=e…` masks whole.
///
/// Conservative by design: over-redaction is acceptable because the
/// page's purpose is human support triage, not log forensics.
// Index-based pairwise masking with lookahead/lookback ranges; iterator
// rewrites obscure the span arithmetic the tests pin down.
#[allow(clippy::needless_range_loop)]
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
                // Find the separator: optional whitespace/quotes (either
                // quote style) then '=' or ':'. Device-code style
                // `user_code XXXX-XXXX` has no separator at all, so the
                // device-code keys additionally accept a bare-whitespace
                // gap. Other keys require '=' or ':' — otherwise prose
                // like `token expired` would mask `expired`.
                let mut j = i + k.len();
                let mut saw_gap = false;
                while j < n && (chars[j].is_whitespace() || chars[j] == '"' || chars[j] == '\'') {
                    if chars[j].is_whitespace() {
                        saw_gap = true;
                    }
                    j += 1;
                }
                let whitespace_gap_ok = saw_gap && (*key == "user_code" || *key == "device_code");
                if j < n && (chars[j] == '=' || chars[j] == ':') {
                    j += 1;
                    // Value starts after optional whitespace and opening quote.
                    while j < n && (chars[j].is_whitespace() || chars[j] == '"' || chars[j] == '\'')
                    {
                        j += 1;
                    }
                    // Issue #602: the two auth header keys carry their
                    // credential behind a scheme word (`Authorization:
                    // Bearer <token>`), so without skipping it the matcher
                    // masked the scheme and left the credential to pass 2's
                    // >=32-char heuristic — i.e. `Authorization: Bearer
                    // abc123` printed the token in full.
                    if is_auth_scheme_key(key) {
                        if let Some(credential_start) = skip_auth_scheme(&chars, j) {
                            j = credential_start;
                        }
                    }
                } else if !(whitespace_gap_ok && j < n && is_value_char(chars[j])) {
                    i += 1;
                    continue;
                }
                let value_start = j;
                while j < n && is_value_char(chars[j]) {
                    j += 1;
                }
                for m in value_start..j {
                    masked[m] = true;
                }
                i = j;
                continue;
            }
            i += 1;
        }
    }

    // Pass 2: long opaque runs. `=` joins a run unless it acts as a
    // `key=value` separator (see `is_kv_separator`), so `value=<40 chars>`
    // keeps its key name while `a.b/c+d=e…` masks as one run.
    let mut run_start: Option<usize> = None;
    for idx in 0..=n {
        let is_opaque = idx < n
            && (is_opaque_char(chars[idx]) || (chars[idx] == '=' && !is_kv_separator(&chars, idx)));
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
    c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' || c == '/' || c == '+'
}

/// Value characters for a keyed secret: everything up to the next
/// delimiter (whitespace, `&`, either quote style, `,`, or `}`).
fn is_value_char(c: char) -> bool {
    !c.is_whitespace() && c != '&' && c != '"' && c != '\'' && c != ',' && c != '}'
}

/// True when `chars[idx] == '='` acts as a `key=value` separator: an
/// identifier character immediately to the left and a non-`=` value
/// character immediately to the right. Base64 padding (`…e=`, trailing or
/// mid-run) never has an identifier run ending exactly at the `=`, so it
/// stays part of the opaque run.
fn is_kv_separator(chars: &[char], idx: usize) -> bool {
    if chars[idx] != '=' || idx == 0 || idx + 1 >= chars.len() {
        return false;
    }
    // Left: end of an identifier run (`value=…`).
    if !is_ident_char(chars[idx - 1]) {
        return false;
    }
    let mut back = idx - 1;
    while back > 0 && is_ident_char(chars[back - 1]) {
        back -= 1;
    }
    // …that starts at a word boundary (not mid-run base64 like `a.b/c+d`).
    if back > 0 && is_opaque_char(chars[back - 1]) {
        return false;
    }
    is_value_char(chars[idx + 1]) && chars[idx + 1] != '='
}

/// Terminators for a path run: whitespace plus the punctuation that closes
/// a wrapped or quoted path inside a real error message.
fn is_path_end_char(c: char) -> bool {
    c.is_whitespace() || matches!(c, ')' | ']' | '}' | '"' | '\'' | ',' | ';' | '>')
}

/// True when a path run starts at `i`: a POSIX root, a UNC `\\` prefix, or
/// a Windows `C:\` / `C:/` drive prefix — and only at a token boundary, so
/// the `//host` of a URL and the `/` inside a relative path stay intact.
fn is_absolute_path_start(chars: &[char], i: usize) -> bool {
    let boundary = i == 0
        || !(chars[i - 1].is_ascii_alphanumeric()
            || matches!(chars[i - 1], '_' | '/' | '\\' | ':' | '.'));
    if !boundary {
        return false;
    }
    match chars[i] {
        '/' | '\\' => true,
        c if c.is_ascii_alphabetic() => {
            i + 2 < chars.len() && chars[i + 1] == ':' && matches!(chars[i + 2], '\\' | '/')
        }
        _ => false,
    }
}

/// Path-hygiene pass applied to strings the snapshot carries verbatim
/// (issue #603 enforcing the #409 rule): every absolute filesystem path —
/// POSIX (`/home/…`), Windows (`C:\Users\…`) or UNC (`\\server\share\…`) —
/// is reduced to its bare trailing component, so a raw OS/updater message
/// can never publish the OS username or the directory the payload was
/// staged in.
///
/// Deliberately separator-agnostic: [`redact_sensitive`]'s opaque-run pass
/// only joins `/`-delimited runs and only masks runs of 32+ characters, so
/// `C:\Users\<user>\Temp\app.msi` (runs split at every `\`) and any short
/// absolute path would otherwise travel untouched.
fn strip_absolute_paths(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut out = String::with_capacity(n);
    let mut i = 0;
    while i < n {
        if !is_absolute_path_start(&chars, i) {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let mut end = i;
        while end < n && !is_path_end_char(chars[end]) {
            end += 1;
        }
        // Trailing separators are punctuation, not part of the last name.
        let mut cut = end;
        while cut > i && (chars[cut - 1] == '/' || chars[cut - 1] == '\\') {
            cut -= 1;
        }
        if cut == i {
            // A bare `/` or `\` is not a path with a name to keep.
            out.push(chars[i]);
            i += 1;
            continue;
        }
        match chars[i..cut].iter().rposition(|c| *c == '/' || *c == '\\') {
            Some(sep) => out.extend(chars[i + sep + 1..cut].iter()),
            None => out.extend(chars[i..cut].iter()),
        }
        i = cut;
    }
    out
}

/// Keys whose value may be preceded by an RFC 7235 auth scheme.
fn is_auth_scheme_key(key: &str) -> bool {
    key == "authorization" || key == "bearer"
}

/// Auth-scheme words that can sit between an auth-header key and its
/// credential (`Bearer`, `Basic`, and the `DPoP` scheme Teams uses).
const AUTH_SCHEMES: &[&str] = &["bearer", "basic", "dpop"];

/// Consume one optional scheme word (case-insensitive) plus the whitespace
/// after it and return the index where the credential starts. `None` when
/// no scheme word is present, so `Bearer: abc` / `authorization=abc` keep
/// the plain `key: value` shape.
fn skip_auth_scheme(chars: &[char], start: usize) -> Option<usize> {
    let n = chars.len();
    for scheme in AUTH_SCHEMES {
        let end = start + scheme.len();
        // `end < n` also guarantees the whitespace that must follow the
        // scheme word is present.
        if end < n
            && chars[start..end]
                .iter()
                .zip(scheme.chars())
                .all(|(c, s)| c.to_ascii_lowercase() == s)
            && chars[end].is_whitespace()
        {
            let mut j = end;
            while j < n && chars[j].is_whitespace() {
                j += 1;
            }
            return Some(j);
        }
    }
    None
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

fn config_summary(state: &crate::AppState, spotify_client_secret_present: bool) -> ConfigSummary {
    let cfg = state.config.get().clone().unwrap_or_default();
    ConfigSummary {
        spotify_client_id: cfg.spotify.client_id,
        redirect_uri: cfg.spotify.redirect_uri,
        // Report the live keychain probe rather than the load-time flag:
        // `config::with_keychain_flags` only stamps this when the config
        // file is loaded, so it can go stale within a session.
        client_secret_set: spotify_client_secret_present,
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
        quiet_hours_count: cfg.status_rules.quiet_hours.len(),
        quiet_hours_enabled_count: cfg
            .status_rules
            .quiet_hours
            .iter()
            .filter(|e| e.enabled)
            .count(),
        track_rules_count: cfg.status_rules.track_rules.len(),
        track_rules_enabled_count: cfg
            .status_rules
            .track_rules
            .iter()
            .filter(|r| r.enabled)
            .count(),
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
        // Username hygiene (issue #409): the absolute path embeds the OS
        // username — snapshot strings carry only the bare file name.
        return (Vec::new(), format!("no log file yet ({})", LOG_FILE_NAME));
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
            // Keep chronological (oldest-first) order; take only the last
            // LOG_TAIL_LINES lines when the file is longer.
            let start = total.saturating_sub(LOG_TAIL_LINES);
            let tail: Vec<String> = lines[start..].iter().map(|l| redact_sensitive(l)).collect();
            let status = format!("ok: last {} of {} lines", tail.len(), total);
            (tail, status)
        }
        Err(e) => {
            // Full path stays in the local log only; the snapshot string
            // carries just the file name (issue #409).
            log::error!(
                "[DIAG] tail_log_file: error reading {}: {}",
                path.display(),
                e
            );
            (
                Vec::new(),
                format!("error reading {}: {}", LOG_FILE_NAME, e),
            )
        }
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
    failed_update_install: Option<crate::updater_bg::FailedUpdateInstall>,
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
        config: config_summary(state, keychain.spotify_client_secret_present),
        tokens: token_metadata(state),
        keychain,
        recent_logs,
        log_source_status,
        // Issue #603: the marker is read (and handed in) by the caller, but
        // sanitized here so no code path can put the raw updater error --
        // download URL, staged path under the OS profile -- into a payload
        // whose whole purpose is to be pasted into a public issue.
        failed_update_install: failed_update_install.map(|mut f| {
            f.error = redact_sensitive(&f.error);
            // #409 applied to #603: the raw updater error names the staged
            // file (and the OS profile it sits under); redaction alone only
            // catches long `/`-joined runs, so the path itself is scrubbed
            // down to its bare file name.
            f.error = strip_absolute_paths(&f.error);
            f
        }),
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
        let state = app_clone.state::<std::sync::Arc<crate::AppState>>();
        let log_dir = app_clone.path().app_log_dir().ok();
        // Read here, like `log_dir`, so `build_snapshot` stays an assembly
        // plus sanitization boundary that tests can drive with a planted
        // marker record (#603).
        let failed_update_install = crate::updater_bg::read_failed_install_marker();
        build_snapshot(&state, log_dir, probe_keychain(), failed_update_install)
    })
    .await
    .map_err(|e| format!("get_diagnostics_snapshot spawn_blocking panicked: {:?}", e))?;
    log::info!(
        "{CMD} get_diagnostics_snapshot: SUCCESS - {} log lines",
        snapshot.recent_logs.len()
    );
    Ok(snapshot)
}

/// Timestamped file name for a saved snapshot (issue #598).
///
/// UTC, millisecond precision: no `:` (illegal in Windows file names) and
/// two saves inside the same second cannot silently overwrite one another.
/// Pure, so the naming contract is unit-testable without touching disk.
fn snapshot_file_name(now: chrono::DateTime<chrono::Utc>) -> String {
    format!(
        "{SNAPSHOT_FILE_STEM}-{}.json",
        now.format("%Y%m%d-%H%M%S%3f")
    )
}

/// Writes `json` into `dir` under a fresh timestamped name and returns the
/// file that now exists on disk. Path-parameterised so the write contract
/// is unit-testable without an `AppHandle` (same shape as
/// `updater_bg::write_failed_install_marker_at`).
fn write_snapshot_file(dir: &std::path::Path, json: &str) -> Result<std::path::PathBuf, String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let file = dir.join(snapshot_file_name(chrono::Utc::now()));
    fs::write(&file, json.as_bytes()).map_err(|e| e.to_string())?;
    Ok(file)
}

/// Tauri command behind the Diagnostics page's "Save to file" button.
///
/// Writes the snapshot JSON the frontend hands over into the platform
/// downloads directory and returns the absolute path of the file now on
/// disk (issue #598). The previous frontend clicked a synthetic anchor on
/// a `blob:` URL and reported success unconditionally: no download handler
/// is registered anywhere in the app, so on engines that ignore an
/// unhandled download the click wrote nothing while the user was told the
/// snapshot had been saved. Success is reported only from a completed
/// write.
///
/// #485: like `clear_failed_update_install`, this has a local side effect
/// but reads no keychain/token/config state, and its only caller is the
/// Diagnostics page (a main-window route), so it stays unguarded.
/// #215: filesystem IO, so the write runs on the blocking pool.
#[tauri::command]
pub async fn save_diagnostics_snapshot(app: AppHandle, json: String) -> Result<String, String> {
    log::info!(
        "{CMD} save_diagnostics_snapshot: ENTRY - {} bytes",
        json.len()
    );
    // The command sits on the app-global `invoke` surface and the file it
    // creates is one a user may attach to a public issue: refuse a payload
    // that is not the snapshot JSON at all.
    serde_json::from_str::<serde_json::Value>(&json).map_err(|e| {
        log::error!("{CMD} save_diagnostics_snapshot: payload is not valid JSON - {e}");
        format!("snapshot payload is not valid JSON: {e}")
    })?;
    let bytes = json.len();

    let app_clone = app.clone();
    let path = tauri::async_runtime::spawn_blocking(move || {
        let dir = app_clone.path().download_dir().map_err(|e| {
            log::error!("{CMD} save_diagnostics_snapshot: no downloads dir - {e}");
            e.to_string()
        })?;
        write_snapshot_file(&dir, &json)
    })
    .await
    .map_err(|e| format!("save_diagnostics_snapshot spawn_blocking panicked: {:?}", e))??;

    let path_str = path.to_string_lossy().to_string();
    log::info!(
        "{CMD} save_diagnostics_snapshot: SUCCESS - wrote {} bytes to the downloads folder",
        bytes
    );
    Ok(path_str)
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
        assert_eq!(redact_sensitive("status codes=200"), "status codes=200");
        let out = redact_sensitive("client_secret=hunter2 do-not-leak");
        assert!(!out.contains("hunter2"));
    }

    #[test]
    fn test_redact_password_value() {
        let out = redact_sensitive("login password=hunter2 failed");
        assert!(!out.contains("hunter2"));
        assert!(out.contains("password=[REDACTED"));
    }

    #[test]
    fn test_redact_bare_token_value() {
        // Bare `token` is its own whole identifier: it must redact
        // `token=abc123` without disturbing `access_token`/`refresh_token`.
        let out = redact_sensitive("got token=abc123 done");
        assert!(!out.contains("abc123"));
        assert!(out.contains("token=[REDACTED len 6]"));
        let access = redact_sensitive("got access_token=abc123 done");
        assert!(!access.contains("abc123"));
        assert!(access.contains("access_token=[REDACTED len 6]"));
        assert!(!access.contains("access_token=[REDACTED len 6][REDACTED"));
        let refresh = redact_sensitive("got refresh_token=abc123 done");
        assert!(!refresh.contains("abc123"));
        assert!(refresh.contains("refresh_token=[REDACTED len 6]"));
    }

    #[test]
    fn test_redact_id_token_value() {
        let out = redact_sensitive("login id_token=shortsecret1 ok");
        assert!(!out.contains("shortsecret1"));
        assert!(out.contains("id_token=[REDACTED len 12]"));
    }

    #[test]
    fn test_redact_code_verifier_value() {
        // `verifier` alone does NOT match inside `code_verifier`
        // (whole-identifier check rejects `_`-flanked substrings), so the
        // compound key must be listed explicitly. The 13-char canary is
        // well under the 32-char opaque-run threshold, so only the keyed
        // pass can mask it.
        let out = redact_sensitive("pkce code_verifier=shortsecret00 ok");
        assert!(!out.contains("shortsecret00"));
        assert!(out.contains("code_verifier=[REDACTED len 13]"));
    }

    #[test]
    fn test_redact_passwd_value() {
        let out = redact_sensitive("login passwd=hunter2 failed");
        assert!(!out.contains("hunter2"));
        assert!(out.contains("passwd=[REDACTED"));
    }

    #[test]
    fn test_redact_api_key_value() {
        // Bare `api_key` is its own whole identifier: it redacts
        // `api_key=...` without disturbing the other keys.
        let out = redact_sensitive("call api_key=shortsecret1 ok");
        assert!(!out.contains("shortsecret1"));
        assert!(out.contains("api_key=[REDACTED len 12]"));
    }

    #[test]
    fn test_redact_code_challenge_value() {
        // Like `code_verifier`, the compound key must be listed
        // explicitly: the whole-identifier check rejects `_`-flanked
        // substrings, so bare `code` never matches inside
        // `code_challenge`. The 13-char canary is well under the 32-char
        // opaque-run threshold, so only the keyed pass can mask it.
        let out = redact_sensitive("pkce code_challenge=shortsecret00 ok");
        assert!(!out.contains("shortsecret00"));
        assert!(out.contains("code_challenge=[REDACTED len 13]"));
    }

    #[test]
    fn test_redact_single_quoted_pair() {
        let out = redact_sensitive("'token': 'abc' done");
        assert!(!out.contains("abc"));
        assert!(out.contains("[REDACTED len 3]"));
    }

    #[test]
    fn test_redact_user_code_whitespace_gap() {
        let out = redact_sensitive("enter user_code XXXX-XXXX now");
        assert!(!out.contains("XXXX-XXXX"));
        assert!(out.contains("user_code [REDACTED"));
    }

    #[test]
    fn test_redact_jwt_shaped_run() {
        // Dotted-base64/JWT with `/`, `+`, and `=` inside: the whole run
        // masks as one regardless of context. Every `/`-delimited chunk
        // is under the 32-char threshold on its own, so pre-fix code
        // (which splits on `/`, `+`, `=`) leaves the secret visible.
        let jwt = "eyJh.bGc-ab/CD+ef.SflKx-wRJSMeKKF2QT4fwpMeJf36P.Ok6yJVadQssw5c=";
        let out = redact_sensitive(&format!("bearer {}", jwt));
        assert!(!out.contains("SflKx-wRJSMeKKF2QT4fwpMeJf36P"));
        assert!(out.contains(&format!("[REDACTED len {}]", jwt.len())));
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
        let snapshot = build_snapshot(&state, Some(dir), keychain, None);
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
        // Issue #409: the status string must not embed the absolute dir
        // (it carries the OS username); only the bare file name travels.
        assert!(status.contains(LOG_FILE_NAME));
        assert!(
            !status.contains(dir.to_str().unwrap()),
            "log source status leaked the absolute log path"
        );

        let log_path = dir.join(LOG_FILE_NAME);
        std::fs::write(&log_path, "[AUTH] code=hunter2secret\n[AUTH] clean line\n").unwrap();
        let (lines, _) = tail_log_file(Some(dir.clone()));
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "[AUTH] code=[REDACTED len 13]");
        assert_eq!(lines[1], "[AUTH] clean line");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_tail_log_file_error_status_has_no_absolute_path() {
        // Issue #409: force the read-error branch by planting a
        // directory where the log file should be (metadata succeeds,
        // the byte read fails), then assert the snapshot status names
        // only the file — never the username-bearing absolute path.
        let dir = std::env::temp_dir().join(format!("pj-diag-err-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(LOG_FILE_NAME)).unwrap();
        let (lines, status) = tail_log_file(Some(dir.clone()));
        assert!(lines.is_empty());
        assert!(status.contains("error reading"));
        assert!(status.contains(LOG_FILE_NAME));
        assert!(
            !status.contains(dir.to_str().unwrap()),
            "error status leaked the absolute log path: {}",
            status
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_redact_auth_scheme_with_short_credential() {
        // Issue #602: the keyed `authorization`/`bearer` entries used to
        // mask the scheme word and leave the credential to the >=32-char
        // heuristic, so `Authorization: Bearer abc123` printed the token in
        // full. The scheme word is not a secret; the credential always is.
        let header = redact_sensitive("Authorization: Bearer abc123");
        assert_eq!(header, "Authorization: Bearer [REDACTED len 6]");

        let basic = redact_sensitive("Authorization: Basic dXNlcjpwYXNz");
        assert!(!basic.contains("dXNlcjpwYXNz"));
        assert!(basic.contains("[REDACTED len 12]"));

        let dpop = redact_sensitive("authorization: DPoP short1");
        assert!(!dpop.contains("short1"));
        assert!(dpop.contains("[REDACTED len 6]"));

        // `bearer:` as the key itself (no scheme word behind it).
        let scheme_key = redact_sensitive("bearer: abc123 done");
        assert!(!scheme_key.contains("abc123"));
        assert!(scheme_key.contains("[REDACTED len 6]"));
    }

    #[test]
    fn test_failed_update_install_error_is_redacted() {
        // Issue #603: the failed-install marker was copied into the
        // snapshot verbatim, so a raw updater error -- download URL, staged
        // file path under the OS profile -- reached the payload whose whole
        // purpose is to be pasted into a public issue. `error` must lose
        // both the credential-shaped run and every absolute path, on either
        // separator style (the old test only used a long `/` path, which
        // pass 2 happened to mask).
        let state = crate::AppState::default();
        const OPAQUE: &str = "b3BlbnF1ZXVlLXNlY3JldC1jYW5hcnktMTIzNDU2Nzg5MA";
        const LONG_POSIX: &str = "/home/jack/.local/share/PresenceJam/staged/presencejam-4.6.0.msi";
        const SHORT_POSIX: &str = "/tmp/a/presencejam-staged.msi";
        const WINDOWS: &str = r"C:\Users\jack\AppData\Local\Temp\pj\app.msi";
        const UNC: &str = r"\\fileserver\share\rel\presencejam.msi";
        let record = crate::updater_bg::FailedUpdateInstall {
            version: "4.6.0".to_string(),
            error: format!(
                "download failed for {LONG_POSIX} then {SHORT_POSIX} then {WINDOWS} then {UNC} (token {OPAQUE})"
            ),
            timestamp: "2026-09-16T10:00:00+00:00".to_string(),
        };
        let keychain = KeychainStatus {
            spotify_client_secret_present: false,
            tokens_encryption_key_present: false,
        };
        let snapshot = build_snapshot(&state, None, keychain, Some(record));
        let json = serde_json::to_string_pretty(&snapshot).expect("serialize snapshot");
        let failed = snapshot
            .failed_update_install
            .expect("failed-install record survives collection");

        assert!(
            !json.contains(OPAQUE),
            "opaque credential reached the diagnostics snapshot"
        );
        for path in [LONG_POSIX, SHORT_POSIX, WINDOWS, UNC] {
            assert!(
                !json.contains(path),
                "absolute path reached the diagnostics snapshot: {path}"
            );
        }
        // Path hygiene is separator-agnostic: the OS username and the
        // staging directories are gone from the error text whatever
        // separator the platform uses. LONG_POSIX is a single opaque run
        // above the 32-char threshold, so pass 2 redacts it whole; the
        // shorter paths are scrubbed down to their bare file names.
        assert!(!failed.error.contains("jack"));
        assert!(!failed.error.contains("AppData"));
        assert!(!failed.error.contains("/home/"));
        assert!(!failed.error.contains("PresenceJam/staged"));
        assert!(!failed.error.contains("/tmp/"));
        assert!(!failed.error.contains(r"\\"));
        // ...while the surviving file names still tell support what failed.
        assert!(failed.error.contains("presencejam-staged.msi"));
        assert!(failed.error.contains("app.msi"));
        assert!(failed.error.contains("presencejam.msi"));
        // The triage fields the page renders must survive sanitization.
        assert_eq!(failed.version, "4.6.0");
        assert_eq!(failed.timestamp, "2026-09-16T10:00:00+00:00");
    }

    #[test]
    fn test_strip_absolute_paths_keeps_last_component() {
        assert_eq!(strip_absolute_paths("/home/a/b/c.msi"), "c.msi");
        assert_eq!(strip_absolute_paths(r"C:\Users\a\b.msi"), "b.msi");
        assert_eq!(strip_absolute_paths(r"C:/Users/a/b.msi"), "b.msi");
        assert_eq!(strip_absolute_paths(r"\\server\share\b.msi"), "b.msi");
        // Embedded in prose / wrapped in punctuation / behind `key=`.
        assert_eq!(
            strip_absolute_paths("see (/home/jack/x.msi) now"),
            "see (x.msi) now"
        );
        assert_eq!(
            strip_absolute_paths(r"staged=C:\Users\jack\app.msi failed"),
            "staged=app.msi failed"
        );
        assert_eq!(
            strip_absolute_paths("dir /home/jack/x/ done"),
            "dir x/ done"
        );
        // Relative tokens, URLs and bare separators are not paths.
        assert_eq!(strip_absolute_paths("version 4.6.0"), "version 4.6.0");
        assert_eq!(
            strip_absolute_paths("GET https://host/p/a ok"),
            "GET https://host/p/a ok"
        );
        assert_eq!(strip_absolute_paths("either / or"), "either / or");
    }

    #[test]
    fn test_snapshot_file_name_is_windows_safe() {
        // Issue #598: the saved file name must not contain a `:` and must
        // carry a millisecond stamp, so two saves in one second do not
        // silently overwrite each other.
        let ts = chrono::DateTime::parse_from_rfc3339("2026-09-16T10:11:12.345Z")
            .expect("parse timestamp")
            .with_timezone(&chrono::Utc);
        let name = snapshot_file_name(ts);
        assert_eq!(name, "presencejam-diagnostics-20260916-101112345.json");
        assert!(!name.contains(':'));
        let other = snapshot_file_name(ts + chrono::Duration::milliseconds(1));
        assert_ne!(name, other);
    }

    #[test]
    fn test_write_snapshot_file_reports_a_file_that_exists() {
        // Issue #598: the frontend now claims success only from this
        // result, so a returned path must be a completed write.
        let dir = std::env::temp_dir().join(format!("pj-diag-save-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let payload = r#"{"app_version":"4.6.0"}"#;
        let path = write_snapshot_file(&dir, payload).expect("write snapshot");
        assert!(path.exists(), "save reported a file that does not exist");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), payload);
        let name = path
            .file_name()
            .expect("named file")
            .to_string_lossy()
            .to_string();
        assert!(name.starts_with("presencejam-diagnostics-"));
        assert!(name.ends_with(".json"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
