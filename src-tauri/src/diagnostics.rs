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

/// Stem of [`LOG_FILE_NAME`]: the plugin is configured with
/// `file_name: Some("PresenceJam")` and appends `.log` to the active file and
/// `_<timestamp>.log` to every archive it rotates (issue #874).
const LOG_FILE_STEM: &str = "PresenceJam";

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
    /// Last [`LOG_TAIL_LINES`] lines of the on-disk log — the active file plus
    /// any rotated archive it needed (issue #874) — each passed through
    /// [`redact_sensitive`] and then [`strip_absolute_paths`] (issue #913), so
    /// neither a credential nor an absolute path can reach a pasted snapshot.
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

/// OS identity for the snapshot: compile-time constants from
/// `std::env::consts` plus the runtime release probed with platform APIs
/// (no new deps; the `tauri-plugin-os` plugin is deliberately not added
/// for this).
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct OsInfo {
    /// `std::env::consts::OS` (e.g. `"windows"` / `"macos"` / `"linux"`).
    pub platform: String,
    /// `std::env::consts::ARCH` (e.g. `"x86_64"` / `"aarch64"`).
    pub arch: String,
    /// `std::env::consts::FAMILY` (e.g. `"unix"` / `"windows"`).
    pub family: String,
    /// Release of the running OS (issue #875), e.g.
    /// `Ubuntu 24.04.1 LTS (7.0.0-31-generic)`, `macOS 14.5` or
    /// `Windows 11 (24H2, build 26100)`. `unknown` when the probe fails;
    /// never the hostname, machine name or a user path.
    pub os_version: String,
    /// How this copy was installed (issue #786), as a lowercase token:
    /// `deb`, `rpm`, `appimage`, `msi`, `nsis`, `dmg` or `app` for a Tauri
    /// bundle, `homebrew` for a Homebrew prefix, else `unknown`. Decides
    /// whether an in-app update can succeed at all (the deb case fails by
    /// design); never the executable path.
    pub install_flavor: String,
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
    /// CfgDiag#2 (issue #537, completing #379): true when this process
    /// found `config.json` unreadable, quarantined it to `<name>.bak` and
    /// booted on `AppConfig::default()`. Every value above is therefore a
    /// factory default, not the user's — without this flag the reset is
    /// invisible (the `[CFG]` warn line sits in a log nobody opens).
    pub config_quarantined: bool,
    /// Bare file name of the quarantine backup (`config.json.bak`) when one
    /// is still on disk, never a path — see [`quarantine_backup_field`] and
    /// the #409 rule it enforces. A `.bak` can outlive the quarantining
    /// launch, so this is also how a later session can point the user at
    /// the settings it lost.
    pub config_quarantine_backup: Option<String>,
    /// Finding #635 gate switch (issue #864): ON by default, and one of the
    /// most common reasons a status write is held back on purpose.
    pub respect_manual_status: bool,
    /// Finding #637 gate switch (issue #864).
    pub gate_when_out_of_office: bool,
    /// Count only, like the rule counts above: the extra words are user
    /// content and never travel (#432 rule, issue #864).
    pub profanity_extra_words_count: usize,
    /// Configured UI locale; `None` is the documented `"en"` default
    /// (issue #864) — needed to reproduce anything from a translated build.
    pub locale: Option<String>,
    /// `stable` / `beta`: which release manifest this install consults
    /// (issue #864).
    pub update_channel: String,
    /// An active snooze right now, and the whole minutes left — the reason a
    /// long silence looks like a hang (issue #864). Derived, not the stored
    /// deadline: [`crate::config::snooze_status`] already applies the
    /// expiry/parse rules, and [`crate::config::snooze_minutes_left`] is the
    /// same rounding the tray and Dashboard render.
    pub snoozed: bool,
    pub snooze_minutes_left: Option<i64>,
    /// Rotation settings from `logging` (issue #874): without them a reader
    /// cannot tell how far back the log above should reach — 64 KiB of a
    /// 10 MB file looks the same as 64 KiB of a 1 MB one.
    pub log_max_file_size_mb: u64,
    pub log_keep_files: u32,
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

/// Quarantine state of the config file, read at the command boundary and
/// carried into [`ConfigSummary`] (CfgDiag#2, issue #537).
///
/// `config::load_config` already quarantines an unreadable `config.json` to
/// `<name>.bak` and boots on defaults, and `config_was_quarantined()` already
/// says so — but nothing observed it, so the user's client id, polling and
/// #432 rules vanished without a word (issue #379's "diagnostics-visible"
/// contract was only claimable from the unit tests).
///
/// Deliberately a plain value rather than a second live read inside
/// [`build_snapshot`]: the assembly boundary stays drivable with a planted
/// quarantine, exactly like the failed-install marker (#603), so the
/// snapshot contract is testable without a real corrupt config on disk.
#[derive(Debug, Clone, Default)]
pub struct ConfigQuarantine {
    /// [`crate::config::config_was_quarantined`] — true once *this* process
    /// renamed a corrupt config aside.
    pub quarantined: bool,
    /// [`crate::config::config_quarantine_backup_name`] — bare file name of
    /// the backup if it is still next to the config, else `None`.
    pub backup_name: Option<String>,
}

impl ConfigQuarantine {
    /// Production read of the process flag plus an existence probe on the
    /// `.bak` sibling of the real config path.
    fn observe() -> Self {
        Self {
            quarantined: crate::config::config_was_quarantined(),
            backup_name: crate::config::config_quarantine_backup_name(),
        }
    }
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

/// Snapshot form of the quarantine backup: the bare file name only, or
/// `None` when there is no name safe enough to publish (issue #537).
///
/// `config::config_quarantine_backup_name` already drops the directory, but
/// this is the boundary that has to *hold* the #409 rule, not a caller that
/// happens to satisfy it: an absolute path is reduced to its last component
/// by the same pass the #603 updater text goes through, and a name that
/// still carries a separator (a relative path, which that pass deliberately
/// leaves alone) is dropped rather than published.
///
/// Interaction with the log-redaction pass in this module: the name is
/// short and separator-free, so [`redact_sensitive`] is the identity on it
/// (no key match, no 32-char opaque run) — pinned by the tests below, since
/// a future redaction change that masked `config.json.bak` would silently
/// remove the one pointer the user has to their lost settings.
fn quarantine_backup_field(name: &str) -> Option<String> {
    let stripped = strip_absolute_paths(name);
    if stripped.is_empty() || stripped.contains('/') || stripped.contains('\\') {
        return None;
    }
    Some(stripped)
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

/// `quarantine` is read by the command (see [`ConfigQuarantine::observe`])
/// and only projected here, so the snapshot shape is fixed in one place.
fn config_summary(
    state: &crate::AppState,
    spotify_client_secret_present: bool,
    quarantine: &ConfigQuarantine,
) -> ConfigSummary {
    let cfg = state.config.get().clone().unwrap_or_default();
    let snooze = crate::config::snooze_status(&cfg, chrono::Utc::now());
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
        config_quarantined: quarantine.quarantined,
        config_quarantine_backup: quarantine
            .backup_name
            .as_deref()
            .and_then(quarantine_backup_field),
        respect_manual_status: cfg.teams.respect_manual_status,
        gate_when_out_of_office: cfg.teams.gate_when_out_of_office,
        profanity_extra_words_count: cfg.teams.profanity_extra_words.len(),
        locale: cfg.locale,
        update_channel: update_channel_token(cfg.updates.channel),
        snoozed: snooze.is_some(),
        snooze_minutes_left: snooze
            .map(|s| crate::config::snooze_minutes_left(s.remaining_seconds)),
        log_max_file_size_mb: cfg.logging.max_file_size_mb,
        log_keep_files: cfg.logging.keep_files,
    }
}

/// Wire spelling of the update channel (issue #864). Mirrors the
/// `#[serde(rename_all = "lowercase")]` contract that the Settings picker and
/// `updates.channel` in `config.json` round-trip, so the snapshot reads the
/// same token as the file it reports. An exhaustive match keeps a future
/// channel from silently missing the snapshot.
fn update_channel_token(channel: crate::config::UpdateChannel) -> String {
    match channel {
        crate::config::UpdateChannel::Stable => "stable",
        crate::config::UpdateChannel::Beta => "beta",
    }
    .to_string()
}

/// Tail the on-disk log written by `tauri_plugin_log`'s `LogDir` target,
/// together with the rotated archives that still hold the minutes before it
/// (issue #874). Returns up to [`LOG_TAIL_LINES`] lines, oldest first, plus a
/// status naming every file they came from (missing file is normal on first
/// run).
///
/// `keep_files` is `logging.keep_files` — how many archives the plugin
/// retains, and so the most this may read.
fn tail_log_file(log_dir: Option<std::path::PathBuf>, keep_files: u32) -> (Vec<String>, String) {
    let Some(dir) = log_dir else {
        return (
            Vec::new(),
            "unavailable: could not resolve app log dir".to_string(),
        );
    };

    // Newest source first: the active file, then the archives.
    let active = dir.join(LOG_FILE_NAME);
    let mut merged: Vec<String> = Vec::new();
    let mut sources: Vec<String> = Vec::new();
    match read_tail_window(&active) {
        Ok(Some(window)) => {
            if !window.is_empty() {
                sources.push(LOG_FILE_NAME.to_string());
            }
            merged = window;
        }
        // No active file yet: a first run, or a rotation that has not written
        // one back. The archives below may still carry history.
        Ok(None) => {}
        Err(e) => {
            // Full path stays in the local log only; the snapshot string
            // carries just the file name (issue #409).
            log::error!(
                "[DIAG] tail_log_file: error reading {}: {}",
                active.display(),
                e
            );
            return (
                Vec::new(),
                format!("error reading {}: {}", LOG_FILE_NAME, e),
            );
        }
    }

    // Only reach further back while the snapshot is short of its line budget.
    for name in rotated_log_names(&dir, keep_files) {
        if merged.len() >= LOG_TAIL_LINES {
            break;
        }
        let path = dir.join(&name);
        match read_tail_window(&path) {
            Ok(Some(window)) if !window.is_empty() => {
                sources.push(name);
                // Older lines belong in front of the newer ones already held.
                let mut group = window;
                group.append(&mut merged);
                merged = group;
            }
            Ok(_) => {}
            Err(e) => {
                // An unreadable archive only costs history; the active file's
                // own failure is the one reported above.
                log::error!(
                    "[DIAG] tail_log_file: error reading {}: {}",
                    path.display(),
                    e
                );
            }
        }
    }

    if sources.is_empty() {
        // Username hygiene (issue #409): the absolute path embeds the OS
        // username — snapshot strings carry only the bare file name.
        return (Vec::new(), format!("no log file yet ({})", LOG_FILE_NAME));
    }

    let total = merged.len();
    // Keep chronological (oldest-first) order; take only the last
    // LOG_TAIL_LINES lines when the merged window is longer.
    //
    // Both hygiene passes the failed-update error gets (issues #603/#409)
    // apply here too (issue #913): `redact_sensitive` alone leaves every
    // absolute path intact — its opaque-run mask needs 32 characters with no
    // separator, so Windows paths split at each backslash and a short
    // `/home/<user>/…` falls under the threshold. `strip_absolute_paths`
    // trims each path to its bare last component, which is what the log line
    // still needs to be useful ("Created config directory at 'PresenceJam'").
    let start = total.saturating_sub(LOG_TAIL_LINES);
    let tail: Vec<String> = merged[start..]
        .iter()
        .map(|l| strip_absolute_paths(&redact_sensitive(l)))
        .collect();
    let status = format!(
        "ok: last {} of {} lines ({})",
        tail.len(),
        total,
        sources.join(" + ")
    );
    (tail, status)
}

/// Last lines of one log file, oldest-first: at most [`LOG_TAIL_LINES`] from
/// its final [`LOG_TAIL_MAX_BYTES`] bytes. `Ok(None)` when the file is not
/// there (a first run, or an archive the plugin has since rotated away) and
/// `Err` when it is there but cannot be read.
fn read_tail_window(path: &std::path::Path) -> Result<Option<Vec<String>>, String> {
    let len = match fs::metadata(path) {
        Ok(md) => md.len(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let start = len - len.min(LOG_TAIL_MAX_BYTES);
    let bytes = read_from_offset(path, start)?;
    let text = String::from_utf8_lossy(&bytes);
    let mut lines: Vec<&str> = text.lines().collect();
    // Seeking mid-file lands inside a line; that fragment is not a log record
    // and would render as a truncated one. A seek that landed on a newline is
    // at a record boundary, where dropping the first line would lose a whole
    // one — so the drop is conditional on the byte before the window (the same
    // rule `commands/logs.rs` applies for issue #824).
    if start > 0 && !lines.is_empty() && read_byte_before(path, start) != Some(b'\n') {
        lines.remove(0);
    }
    // Bound each file before merging: the byte window alone can hold far more
    // lines than the snapshot will ever show.
    let first = lines.len().saturating_sub(LOG_TAIL_LINES);
    Ok(Some(
        lines[first..].iter().map(|l| (*l).to_string()).collect(),
    ))
}

/// The byte at `offset - 1`, or `None` when it cannot be read.
fn read_byte_before(path: &std::path::Path, offset: u64) -> Option<u8> {
    use std::io::{Read, Seek, SeekFrom};
    if offset == 0 {
        return None;
    }
    let mut f = fs::File::open(path).ok()?;
    f.seek(SeekFrom::Start(offset - 1)).ok()?;
    let mut byte = [0u8; 1];
    f.read_exact(&mut byte).ok()?;
    Some(byte[0])
}

/// Names of the rotated archives in `dir`, newest first, at most `keep_files`
/// of them (issue #874).
///
/// The plugin renames the active file to
/// `PresenceJam_<YYYY-MM-DD_HH-MM-SS>.log` (`tauri-plugin-log`'s
/// `LOG_DATE_FORMAT`) and adds a `.bak` twin when that timestamp already
/// existed. The format is fixed-width, so name order is chronological order
/// and no date parsing is needed; `.bak` twins are skipped — they are the
/// older duplicate of a name that is already listed.
fn rotated_log_names(dir: &std::path::Path, keep_files: u32) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let prefix = format!("{LOG_FILE_STEM}_");
    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.starts_with(&prefix) && name.ends_with(".log"))
        .collect();
    names.sort_unstable_by(|a, b| b.cmp(a));
    names.truncate(keep_files as usize);
    names
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
///
/// The quarantine state is a parameter for the same reason as the
/// failed-install marker: the assembly boundary must be drivable with a
/// planted quarantine, and the sanitization that keeps a path out of the
/// payload belongs on this side of it.
fn build_snapshot(
    state: &crate::AppState,
    log_dir: Option<std::path::PathBuf>,
    keychain: KeychainStatus,
    failed_update_install: Option<crate::updater_bg::FailedUpdateInstall>,
    quarantine: ConfigQuarantine,
) -> DiagnosticsSnapshot {
    log::debug!("{CMD} build_snapshot: collecting local diagnostics");
    // Retention bounds how many archives the tail may read (issue #874), and
    // comes from the live config like every other value the summary reports.
    let keep_files = state
        .config
        .get()
        .as_ref()
        .map(|cfg| cfg.logging.keep_files)
        .unwrap_or_else(|| crate::config::LoggingConfig::default().keep_files);
    let (recent_logs, log_source_status) = tail_log_file(log_dir, keep_files);
    DiagnosticsSnapshot {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        tauri_version: tauri::VERSION.to_string(),
        os: OsInfo {
            platform: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            family: std::env::consts::FAMILY.to_string(),
            os_version: os_release(),
            install_flavor: install_flavor(),
        },
        config: config_summary(state, keychain.spotify_client_secret_present, &quarantine),
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

/// Human-readable release of the running OS for `OsInfo::os_version`
/// (issue #875). Whatever the platform probe cannot answer is folded to
/// `unknown` rather than failing the snapshot.
///
/// Kept dependency-free on purpose: see the `OsInfo` doc comment.
fn os_release() -> String {
    #[cfg(target_os = "linux")]
    {
        let pretty = fs::read_to_string("/etc/os-release")
            .ok()
            .and_then(|raw| os_release_pretty_name(&raw));
        let kernel = fs::read_to_string("/proc/sys/kernel/osrelease").ok();
        compose_linux_release(pretty.as_deref(), kernel.as_deref().map(str::trim))
    }
    #[cfg(target_os = "macos")]
    {
        macos_release()
    }
    #[cfg(target_os = "windows")]
    {
        windows_release()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        "unknown".to_string()
    }
}

/// `PRETTY_NAME` out of an `/etc/os-release` body, unquoted; a distribution
/// that omits it falls back to `NAME VERSION_ID`, then to `NAME`.
///
/// Compiled for tests on every host (like [`reg_value`]), so both branches are
/// covered off-Linux.
#[cfg(any(target_os = "linux", test))]
fn os_release_pretty_name(contents: &str) -> Option<String> {
    let value = |key: &str| {
        contents.lines().find_map(|line| {
            let (k, v) = line.split_once('=')?;
            if k.trim() != key {
                return None;
            }
            let v = v.trim().trim_matches('"');
            (!v.is_empty()).then(|| v.to_string())
        })
    };
    value("PRETTY_NAME").or_else(|| {
        let name = value("NAME")?;
        Some(match value("VERSION_ID") {
            Some(id) => format!("{name} {id}"),
            None => name,
        })
    })
}

/// `Ubuntu 24.04.1 LTS (7.0.0-31-generic)`; the kernel alone when
/// `/etc/os-release` had nothing usable, `unknown` when neither did.
///
/// Compiled for tests on every host, like [`os_release_pretty_name`].
#[cfg(any(target_os = "linux", test))]
fn compose_linux_release(pretty_name: Option<&str>, kernel: Option<&str>) -> String {
    let kernel = kernel.filter(|k| !k.is_empty());
    match (pretty_name, kernel) {
        (Some(name), Some(kernel)) => format!("{name} ({kernel})"),
        (Some(name), None) => name.to_string(),
        (None, Some(kernel)) => format!("Linux {kernel}"),
        (None, None) => "unknown".to_string(),
    }
}

/// `macOS 14.5` from `sw_vers -productVersion` — the one release query macOS
/// exposes without a crate (`std::env::consts::OS` is just `"macos"`).
#[cfg(target_os = "macos")]
fn macos_release() -> String {
    let version = std::process::Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|v| !v.is_empty());
    match version {
        Some(v) => format!("macOS {v}"),
        None => "unknown".to_string(),
    }
}

/// `Windows 11 (24H2, build 26100)` from the `CurrentVersion` registry key,
/// which `reg` reads on every supported Windows and — unlike `ver` — without
/// a localised value name.
#[cfg(target_os = "windows")]
fn windows_release() -> String {
    let output = std::process::Command::new("reg")
        .args([
            "query",
            r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion",
        ])
        .output()
        .ok()
        .filter(|out| out.status.success());
    let Some(output) = output else {
        return "unknown".to_string();
    };
    let text = String::from_utf8_lossy(&output.stdout);
    windows_release_token(
        reg_value(&text, "CurrentBuildNumber").as_deref(),
        reg_value(&text, "DisplayVersion").as_deref(),
        reg_value(&text, "ProductName").as_deref(),
    )
}

/// One `<name>  <type>  <value…>` row of `reg query` output.
///
/// The value runs to the end of the line: registry values contain spaces
/// (`ProductName` reads `Windows Server 2022`), so everything after the type
/// field is joined back together.
///
/// Compiled for tests on every host so the parser is covered off-Windows.
#[cfg(any(target_os = "windows", test))]
fn reg_value(text: &str, name: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        if fields.next()? != name {
            return None;
        }
        // `<type>` is `REG_SZ`/`REG_DWORD`; the value follows it.
        fields.next()?;
        let value = fields.collect::<Vec<_>>().join(" ");
        (!value.is_empty()).then_some(value)
    })
}

/// Windows release token from the registry values. The build number decides
/// 10 versus 11 (Microsoft's own rule: 22000 and up is Windows 11) because
/// `ProductName` still says "Windows 10" on Windows 11. Compiled for tests on
/// every host, like [`reg_value`].
#[cfg(any(target_os = "windows", test))]
fn windows_release_token(
    build: Option<&str>,
    display_version: Option<&str>,
    product_name: Option<&str>,
) -> String {
    let Some(build) = build else {
        return match product_name {
            Some(name) => name.to_string(),
            None => "unknown".to_string(),
        };
    };
    let generation = match build.parse::<u32>() {
        Ok(b) if b >= 22_000 => "11",
        Ok(_) => "10",
        Err(_) => "?",
    };
    match display_version {
        Some(dv) => format!("Windows {generation} ({dv}, build {build})"),
        None => format!("Windows {generation} (build {build})"),
    }
}

/// Install flavour token for `OsInfo::install_flavor` (issue #786).
///
/// [`tauri::utils::platform::bundle_type`] names the Tauri bundles; it cannot
/// see a Homebrew install, which is not a bundle, so a Cellar/Caskroom/
/// homebrew prefix on the running executable decides that case. The
/// executable path itself never leaves these functions.
fn install_flavor() -> String {
    let exe = std::env::current_exe().ok();
    install_flavor_of(tauri::utils::platform::bundle_type(), exe.as_deref())
}

/// Mapping from the bundle marker plus the executable path, parameterised so
/// both sources are testable without a real bundle (the marker is a
/// build-time constant).
fn install_flavor_of(
    bundle: Option<tauri::utils::config::BundleType>,
    exe: Option<&std::path::Path>,
) -> String {
    if exe.is_some_and(is_homebrew_path) {
        return "homebrew".to_string();
    }
    match bundle {
        Some(bundle) => bundle.to_string(),
        None => "unknown".to_string(),
    }
}

/// Homebrew formulae live under `Cellar/`, casks under `Caskroom/`, and the
/// prefix itself is `/opt/homebrew/` (Apple silicon) or `/usr/local/Homebrew/`
/// (Intel).
fn is_homebrew_path(exe: &std::path::Path) -> bool {
    let path = exe.to_string_lossy().to_lowercase();
    ["/cellar/", "/caskroom/", "/homebrew/"]
        .iter()
        .any(|marker| path.contains(marker))
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
        // marker record (#603) or a planted quarantine (#537).
        let failed_update_install = crate::updater_bg::read_failed_install_marker();
        let quarantine = ConfigQuarantine::observe();
        build_snapshot(
            &state,
            log_dir,
            probe_keychain(),
            failed_update_install,
            quarantine,
        )
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
        let snapshot = build_snapshot(
            &state,
            Some(dir),
            keychain,
            None,
            ConfigQuarantine::default(),
        );
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
        let (lines, status) = tail_log_file(Some(dir.clone()), 3);
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
        let (lines, status) = tail_log_file(Some(dir.clone()), 3);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "[AUTH] code=[REDACTED len 13]");
        assert_eq!(lines[1], "[AUTH] clean line");
        assert_eq!(
            status,
            format!("ok: last 2 of 2 lines ({LOG_FILE_NAME})"),
            "a single-source tail names only the active file"
        );
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
        let (lines, status) = tail_log_file(Some(dir.clone()), 3);
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

    /// Issue #874: after a rotation the snapshot must span the active file and
    /// the newest archives, and say which files it read.
    #[test]
    fn test_tail_log_file_spans_the_active_file_and_the_newest_archive() {
        let dir = std::env::temp_dir().join(format!("pj-diag-rot-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("create scratch dir");

        let archive = "PresenceJam_2026-09-17_04-00-01.log";
        let active: Vec<String> = (0..3).map(|i| format!("active-{i}")).collect();
        let rotated: Vec<String> = (0..60).map(|i| format!("arch-{i:04}")).collect();
        std::fs::write(dir.join(LOG_FILE_NAME), active.join("\n") + "\n").expect("write active");
        std::fs::write(dir.join(archive), rotated.join("\n") + "\n").expect("write archive");
        // A `.bak` twin of a *different* archive must not be read.
        std::fs::write(
            dir.join("PresenceJam_2026-09-16_04-00-01.log.bak"),
            "bak-0000\nbak-0001\n",
        )
        .expect("write bak twin");

        let (lines, status) = tail_log_file(Some(dir.clone()), 3);

        assert_eq!(
            lines.len(),
            LOG_TAIL_LINES,
            "the budget still caps the tail"
        );
        // The oldest surviving line comes from the archive: merged is the
        // archive's last 50 lines with the 3 active lines appended, and the
        // final 50 of those start 3 lines into the archive.
        assert_eq!(lines[0], "arch-0013");
        assert_eq!(lines[46], "arch-0059", "the archive meets the active file");
        assert_eq!(&lines[47..], &active[..], "the active file stays newest");
        assert!(
            !lines.iter().any(|l| l.starts_with("bak-")),
            "a `.bak` twin is not history: {lines:?}"
        );

        assert!(
            status.starts_with(&format!("ok: last {} of 53 lines (", LOG_TAIL_LINES)),
            "status: {status}"
        );
        assert!(status.contains(LOG_FILE_NAME), "status: {status}");
        assert!(status.contains(archive), "status: {status}");
        assert!(
            !status.contains(dir.to_str().expect("utf-8 dir")),
            "status leaked the absolute log path: {status}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// After a rotation the active file can be absent (the plugin writes a
    /// fresh one only on the next write) while the archives still hold the
    /// history: the tail must come from them, and the status must say so.
    #[test]
    fn test_tail_log_file_reads_archives_when_the_active_file_is_gone() {
        let dir = std::env::temp_dir().join(format!("pj-diag-noactive-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        let archive = "PresenceJam_2026-09-17_04-00-01.log";
        let rotated: Vec<String> = (0..5).map(|i| format!("arch-{i:04}")).collect();
        std::fs::write(dir.join(archive), rotated.join("\n") + "\n").expect("write archive");

        let (lines, status) = tail_log_file(Some(dir.clone()), 3);

        assert_eq!(lines, rotated, "the archive alone is the history");
        assert_eq!(
            status,
            format!("ok: last 5 of 5 lines ({archive})"),
            "the status names only the file the lines came from"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The archive prefix has to be the active file's stem or the tail would
    /// silently stop seeing rotations.
    #[test]
    fn test_log_file_stem_matches_the_active_file_name() {
        assert_eq!(format!("{LOG_FILE_STEM}.log"), LOG_FILE_NAME);
    }

    #[test]
    fn test_rotated_log_names_are_newest_first_and_skip_bak_twins() {
        let dir = std::env::temp_dir().join(format!("pj-diag-names-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        let older = "PresenceJam_2026-09-15_04-00-01.log";
        let newer = "PresenceJam_2026-09-17_04-00-01.log";
        for name in [
            older,
            newer,
            "PresenceJam_2026-09-16_04-00-01.log.bak",
            LOG_FILE_NAME,
            "unrelated.log",
        ] {
            std::fs::write(dir.join(name), "x\n").expect("write candidate");
        }

        assert_eq!(
            rotated_log_names(&dir, 5),
            vec![newer.to_string(), older.to_string()],
            "newest first, `.bak` twins and other files skipped"
        );
        assert_eq!(
            rotated_log_names(&dir, 1),
            vec![newer.to_string()],
            "logging.keep_files bounds how far back the tail may read"
        );
        assert!(rotated_log_names(&dir, 0).is_empty());
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
        let snapshot = build_snapshot(
            &state,
            None,
            keychain,
            Some(record),
            ConfigQuarantine::default(),
        );
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

    // ---------------------------------------------------------------
    // CfgDiag#2 (#537): the quarantine is visible in the snapshot.
    // ---------------------------------------------------------------

    #[test]
    fn test_snapshot_reports_config_quarantine_and_keeps_the_backup_name_bare() {
        // #379 quarantines a corrupt config to `<name>.bak` and boots on
        // defaults; #537 requires the snapshot to say so. A later session
        // has `quarantined == false` (the flag is per-process) but can still
        // point at the backup, so both fields travel independently.
        let state = crate::AppState::default();
        let keychain = KeychainStatus {
            spotify_client_secret_present: false,
            tokens_encryption_key_present: false,
        };
        let snapshot = build_snapshot(
            &state,
            None,
            keychain,
            None,
            ConfigQuarantine {
                quarantined: true,
                backup_name: Some("config.json.bak".to_string()),
            },
        );
        assert!(snapshot.config.config_quarantined);
        assert_eq!(
            snapshot.config.config_quarantine_backup.as_deref(),
            Some("config.json.bak")
        );

        // What the page renders and copies is the serialized payload: both
        // fields must survive it, and neither may carry a directory (the
        // #409 rule) nor anything a redaction pass would eat.
        let json = serde_json::to_string_pretty(&snapshot).expect("serialize snapshot");
        assert!(json.contains("\"config_quarantined\": true"), "{json}");
        assert!(
            json.contains("\"config_quarantine_backup\": \"config.json.bak\""),
            "{json}"
        );
        assert_eq!(
            strip_absolute_paths("config.json.bak"),
            "config.json.bak",
            "the reported name must already satisfy the path-hygiene pass"
        );
        assert_eq!(
            redact_sensitive("config.json.bak"),
            "config.json.bak",
            "log redaction must treat the backup name as an ordinary short file name"
        );
    }

    #[test]
    fn test_quarantine_backup_field_never_publishes_a_path() {
        // The producer (`config::config_quarantine_backup_name`) is already
        // name-only; this boundary holds the #409 rule on its own, so any
        // future route into the field is safe by construction.
        assert_eq!(
            quarantine_backup_field("/home/jack/.config/PresenceJam/config.json.bak").as_deref(),
            Some("config.json.bak")
        );
        assert_eq!(
            quarantine_backup_field(r"C:\Users\jack\AppData\PresenceJam\config.json.bak")
                .as_deref(),
            Some("config.json.bak")
        );
        assert_eq!(
            quarantine_backup_field(r"\\fileserver\share\config.json.bak").as_deref(),
            Some("config.json.bak")
        );
        assert_eq!(
            quarantine_backup_field("config.json.bak").as_deref(),
            Some("config.json.bak")
        );
        // A *relative* path survives `strip_absolute_paths` by design, so
        // the separator check is what stops it reaching a public issue.
        assert_eq!(quarantine_backup_field("PresenceJam/config.json.bak"), None);
        assert_eq!(
            quarantine_backup_field(r"PresenceJam\config.json.bak"),
            None
        );
        assert_eq!(quarantine_backup_field(""), None);

        // And the assembly applies it: a path handed to `build_snapshot`
        // still leaves the payload carrying only the file name.
        let state = crate::AppState::default();
        let keychain = KeychainStatus {
            spotify_client_secret_present: false,
            tokens_encryption_key_present: false,
        };
        let snapshot = build_snapshot(
            &state,
            None,
            keychain,
            None,
            ConfigQuarantine {
                quarantined: true,
                backup_name: Some("/tmp/pj/config.json.bak".to_string()),
            },
        );
        assert_eq!(
            snapshot.config.config_quarantine_backup.as_deref(),
            Some("config.json.bak")
        );
        let json = serde_json::to_string_pretty(&snapshot).expect("serialize snapshot");
        assert!(
            !json.contains("/tmp/pj"),
            "snapshot leaked a config path: {json}"
        );
        assert!(!json.contains("pj/config.json.bak"), "{json}");
    }

    // ---------------------------------------------------------------
    // U10 (#875): the snapshot names the OS *release*, not just the
    // platform token.
    // ---------------------------------------------------------------

    /// Keychain state for snapshot tests — the probes are irrelevant to the
    /// assembly contract and must not touch the real OS keychain.
    fn inert_keychain() -> KeychainStatus {
        KeychainStatus {
            spotify_client_secret_present: false,
            tokens_encryption_key_present: false,
        }
    }

    #[test]
    fn test_os_release_pretty_name_reads_os_release() {
        let body = "NAME=\"Ubuntu\"\nVERSION_ID=\"24.04\"\nPRETTY_NAME=\"Ubuntu 24.04.1 LTS\"\n";
        assert_eq!(
            os_release_pretty_name(body).as_deref(),
            Some("Ubuntu 24.04.1 LTS")
        );
        // Older/minimal distributions may carry only NAME and VERSION_ID.
        assert_eq!(
            os_release_pretty_name("NAME=Alpine\nVERSION_ID=3.20\n").as_deref(),
            Some("Alpine 3.20")
        );
        assert_eq!(
            os_release_pretty_name("NAME=Alpine\n").as_deref(),
            Some("Alpine")
        );
        // An empty PRETTY_NAME is not a name: the fallback still applies.
        assert_eq!(
            os_release_pretty_name("PRETTY_NAME=\"\"\nNAME=Debian\n").as_deref(),
            Some("Debian")
        );
        assert_eq!(os_release_pretty_name("ID=linux\n"), None);
    }

    #[test]
    fn test_compose_linux_release_names_the_kernel_alongside_the_distro() {
        assert_eq!(
            compose_linux_release(Some("Ubuntu 24.04.1 LTS"), Some("6.8.0-45-generic")),
            "Ubuntu 24.04.1 LTS (6.8.0-45-generic)"
        );
        assert_eq!(
            compose_linux_release(Some("Ubuntu 24.04"), None),
            "Ubuntu 24.04"
        );
        // A kernel-only read still beats `unknown`.
        assert_eq!(compose_linux_release(None, Some("6.8.0")), "Linux 6.8.0");
        assert_eq!(compose_linux_release(None, None), "unknown");
    }

    #[test]
    fn test_reg_value_reads_a_reg_query_row() {
        let out = "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\r\n    CurrentBuildNumber    REG_SZ    26100\r\n    DisplayVersion    REG_SZ    24H2\r\n    ProductName    REG_SZ    Windows Server 2022\r\n\r\n";
        assert_eq!(
            reg_value(out, "CurrentBuildNumber").as_deref(),
            Some("26100")
        );
        assert_eq!(reg_value(out, "DisplayVersion").as_deref(), Some("24H2"));
        // The value runs to the end of the line: `ProductName` is multi-word,
        // and stopping at the first space reported "Windows" (D1 review).
        assert_eq!(
            reg_value(out, "ProductName").as_deref(),
            Some("Windows Server 2022")
        );
        assert_eq!(reg_value(out, "Missing"), None);
    }

    #[test]
    fn test_windows_release_token_distinguishes_ten_from_eleven() {
        // Microsoft's own rule — build 22000 and up is Windows 11 — because
        // `ProductName` still says "Windows 10" on Windows 11.
        assert_eq!(
            windows_release_token(Some("26100"), Some("24H2"), Some("Windows 10 Pro")),
            "Windows 11 (24H2, build 26100)"
        );
        assert_eq!(
            windows_release_token(Some("19045"), Some("22H2"), Some("Windows 10 Pro")),
            "Windows 10 (22H2, build 19045)"
        );
        assert_eq!(
            windows_release_token(Some("22631"), None, None),
            "Windows 11 (build 22631)"
        );
        // An unreadable build leaves the product name rather than a guess.
        assert_eq!(
            windows_release_token(None, None, Some("Windows Server 2022")),
            "Windows Server 2022"
        );
        assert_eq!(windows_release_token(None, None, None), "unknown");
    }

    #[test]
    fn test_snapshot_os_version_is_populated_on_this_platform() {
        let snapshot = build_snapshot(
            &crate::AppState::default(),
            None,
            inert_keychain(),
            None,
            ConfigQuarantine::default(),
        );
        assert!(
            !snapshot.os.os_version.is_empty(),
            "the snapshot must name an OS release, not an empty string"
        );
        #[cfg(target_os = "linux")]
        {
            // The probe reads the host, so the release must match the file it
            // reads it from — a stubbed field would not.
            assert_ne!(snapshot.os.os_version, "unknown");
            let kernel =
                fs::read_to_string("/proc/sys/kernel/osrelease").expect("read the kernel release");
            assert!(
                snapshot.os.os_version.contains(kernel.trim()),
                "the kernel release travels with the distro name: {}",
                snapshot.os.os_version
            );
        }
    }

    // ---------------------------------------------------------------
    // U22 (#786): the snapshot names how the copy was installed.
    // ---------------------------------------------------------------

    #[test]
    fn test_install_flavor_distinguishes_bundles_from_a_homebrew_prefix() {
        use tauri::utils::config::BundleType;
        let flavor = |bundle, exe: &str| install_flavor_of(bundle, Some(std::path::Path::new(exe)));

        // The marker decides the bundled cases; the path is irrelevant there.
        assert_eq!(
            flavor(Some(BundleType::Deb), "/usr/bin/presence-jam"),
            "deb"
        );
        assert_eq!(
            flavor(Some(BundleType::AppImage), "/tmp/.mount_pj/presence-jam"),
            "appimage"
        );
        assert_eq!(
            flavor(
                Some(BundleType::Msi),
                r"C:\Program Files\PresenceJam\presence-jam.exe"
            ),
            "msi"
        );
        assert_eq!(
            flavor(
                Some(BundleType::App),
                "/Applications/PresenceJam.app/Contents/MacOS/presence-jam"
            ),
            "app"
        );

        // Homebrew is not a Tauri bundle: the prefix is the only tell, and it
        // also catches a .app that Homebrew staged in the Caskroom.
        assert_eq!(
            flavor(
                Some(BundleType::App),
                "/opt/homebrew/Cellar/presencejam/4.7.0/bin/presence-jam"
            ),
            "homebrew"
        );
        assert_eq!(
            flavor(
                None,
                "/usr/local/Caskroom/presencejam/4.7.0/PresenceJam.app/Contents/MacOS/presence-jam"
            ),
            "homebrew"
        );

        // An unbundled run and an unreadable executable path are `unknown`,
        // never a guess (nor a panic).
        assert_eq!(
            flavor(None, "/home/jack/dev/target/debug/presence-jam"),
            "unknown"
        );
        assert_eq!(install_flavor_of(None, None), "unknown");
    }

    #[test]
    fn test_snapshot_os_install_flavor_matches_this_binary() {
        // The bundle marker is patched into a real bundle at build time; a
        // cargo test binary is not one, so the field must hold the documented
        // fallback for this platform rather than a hardcoded token.
        let snapshot = build_snapshot(
            &crate::AppState::default(),
            None,
            inert_keychain(),
            None,
            ConfigQuarantine::default(),
        );
        let expected = if cfg!(target_os = "macos") {
            "app"
        } else {
            "unknown"
        };
        assert_eq!(
            snapshot.os.install_flavor, expected,
            "an unbundled Linux/Windows binary reports the fallback token"
        );
    }

    // ---------------------------------------------------------------
    // U10 (#864): the config summary carries the switches, locale and
    // channel a support reader needs.
    // ---------------------------------------------------------------

    #[test]
    fn test_config_summary_reports_gating_switches_locale_channel_and_snooze() {
        let state = crate::AppState::default();
        {
            let mut cfg = crate::config::AppConfig::default();
            cfg.teams.respect_manual_status = false;
            cfg.teams.gate_when_out_of_office = true;
            cfg.teams.profanity_extra_words =
                vec!["frobnicate".into(), "wibble".into(), "wobble".into()];
            cfg.locale = Some("de-AT".into());
            cfg.updates.channel = crate::config::UpdateChannel::Beta;
            cfg.logging.max_file_size_mb = 25;
            cfg.logging.keep_files = 7;
            cfg.snooze_until =
                Some((chrono::Utc::now() + chrono::Duration::minutes(30)).to_rfc3339());
            *state.config.get_mut() = Some(cfg);
        }

        let snapshot = build_snapshot(
            &state,
            None,
            inert_keychain(),
            None,
            ConfigQuarantine::default(),
        );
        let config = &snapshot.config;
        assert!(
            !config.respect_manual_status,
            "the switch is reported as set"
        );
        assert!(config.gate_when_out_of_office);
        assert_eq!(config.locale.as_deref(), Some("de-AT"));
        assert_eq!(config.update_channel, "beta");
        assert_eq!(config.profanity_extra_words_count, 3);
        assert_eq!(config.log_max_file_size_mb, 25);
        assert_eq!(config.log_keep_files, 7);
        assert!(config.snoozed, "a running snooze must be visible");
        let minutes = config.snooze_minutes_left.expect("snooze minutes");
        assert!(
            (25..=30).contains(&minutes),
            "minutes left are rounded up from the deadline, got {minutes}"
        );

        // The words themselves are user content and must not travel (#432).
        let json = serde_json::to_string(&snapshot).expect("serialize snapshot");
        assert!(!json.contains("frobnicate"), "{json}");
        assert!(!json.contains("wibble"), "{json}");
    }

    #[test]
    fn test_config_summary_snooze_state_is_absent_without_a_deadline() {
        let state = crate::AppState::default();
        let snapshot = build_snapshot(
            &state,
            None,
            inert_keychain(),
            None,
            ConfigQuarantine::default(),
        );
        assert!(!snapshot.config.snoozed);
        assert_eq!(snapshot.config.snooze_minutes_left, None);
    }

    #[test]
    fn test_update_channel_token_matches_the_serde_spelling() {
        use crate::config::UpdateChannel;
        // The token must be the on-disk spelling, not a Debug rendering.
        assert_eq!(update_channel_token(UpdateChannel::Stable), "stable");
        assert_eq!(update_channel_token(UpdateChannel::Beta), "beta");
    }

    // ---------------------------------------------------------------
    // U10 (#913): no log line may carry an absolute path into the snapshot.
    // ---------------------------------------------------------------

    #[test]
    fn test_snapshot_log_tail_carries_no_absolute_path_or_username() {
        let dir = std::env::temp_dir().join(format!("pj-diag-hyg-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        // The two shapes the app really logs (`[CFG]` at startup and
        // `commands/window.rs` on the Logs button) plus a control line.
        std::fs::write(
            dir.join(LOG_FILE_NAME),
            concat!(
                "[CFG] Loaded configuration from 'C:\\Users\\jack\\AppData\\Roaming\\PresenceJam\\config.json'\n",
                "[CFG] Created config directory at '/home/jack/.config/PresenceJam'\n",
                "[CFG] window open ok\n",
                "[CMD.WINDOW] open_logs_folder: log path=/home/jack/.local/share/com.presencejam.app/logs\n",
            ),
        )
        .expect("write log");

        let snapshot = build_snapshot(
            &crate::AppState::default(),
            Some(dir.clone()),
            inert_keychain(),
            None,
            ConfigQuarantine::default(),
        );
        let json = serde_json::to_string(&snapshot).expect("serialize snapshot");

        // Neither username may survive, on either separator convention.
        assert!(
            !json.contains("jack"),
            "snapshot leaked the OS username: {json}"
        );
        assert!(!json.contains("Users"), "{json}");
        assert!(!json.contains("AppData"), "{json}");
        assert!(!json.contains("/home/"), "{json}");
        assert!(
            !json.contains(dir.to_str().expect("utf-8 dir")),
            "snapshot leaked the absolute log path: {json}"
        );
        // ...while the bare last component keeps the line useful and the
        // unrelated control line is untouched.
        assert!(
            snapshot
                .recent_logs
                .iter()
                .any(|l| l == "[CFG] window open ok"),
            "the control line changed: {:?}",
            snapshot.recent_logs
        );
        assert!(
            json.contains("config.json"),
            "the file name still tells support what failed: {json}"
        );
        let lines = &snapshot.recent_logs;
        assert!(
            lines
                .iter()
                .any(|l| l == "[CFG] Loaded configuration from 'config.json'"),
            "the Windows path is trimmed to its bare file name: {lines:?}"
        );
        // 30 characters, so pass 2's >= 32-char opaque-run mask never fires:
        // this is the line that reached the snapshot verbatim before #913.
        assert!(
            lines
                .iter()
                .any(|l| l == "[CFG] Created config directory at 'PresenceJam'"),
            "a short Unix path survives redaction and needs the hygiene pass: {lines:?}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
