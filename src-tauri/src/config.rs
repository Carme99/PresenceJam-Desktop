use crate::profanity;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Emitter;

/// Three-way view of the OS-keychain `client_secret` slot (issue #560).
///
/// [`SpotifyConfig::client_secret_set`] cannot express this: it is the
/// `Present`-only projection, so a locked or missing Secret Service collapsed
/// into `false` — the same answer as "the user never configured a secret".
/// Every UI gate that read it then pushed a fully credentialed Linux user
/// through re-onboarding while their secret was still in the keychain,
/// merely unreadable at that moment. This is the type those gates read
/// instead. The keychain-side classification lives in
/// [`crate::keychain::KeychainPresence`]; the conversion below is the single
/// place the two vocabularies meet.
///
/// Serialized lowercase — `present`/`absent`/`unavailable` is the on-the-wire
/// and on-disk spelling the frontend switches on, so `rename_all` is part of
/// the contract, not cosmetics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub enum ClientSecretState {
    /// Stored in the OS keychain and readable right now. The only state a
    /// sign-in flow can complete from.
    Present,
    /// No entry for the slot: the genuine "onboarding needed" answer.
    #[default]
    Absent,
    /// The keychain could not answer (no Secret Service daemon, a locked
    /// keyring, denied storage access). The secret is still there — the UI
    /// must never render this as "not configured".
    Unavailable,
}

impl From<&crate::keychain::KeychainPresence> for ClientSecretState {
    fn from(presence: &crate::keychain::KeychainPresence) -> Self {
        use crate::keychain::KeychainPresence;
        match presence {
            KeychainPresence::Present => ClientSecretState::Present,
            KeychainPresence::Absent => ClientSecretState::Absent,
            KeychainPresence::Unavailable(_) => ClientSecretState::Unavailable,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct SpotifyConfig {
    pub client_id: String,
    /// True iff the Spotify `client_secret` is currently stored in the OS
    /// keychain. This is a derived/display field — it is populated by
    /// `load_config` (and not persisted to disk). The actual secret lives
    /// in the keychain, not in `config.json`. See issue #9.
    #[serde(default)]
    pub client_secret_set: bool,
    /// Tri-state companion of [`Self::client_secret_set`] (issue #560), same
    /// derived/display contract: stamped by [`with_keychain_flags`] on load,
    /// never a durable statement about the keychain. `unavailable` is the
    /// case the bool cannot carry.
    #[serde(default)]
    pub client_secret_state: ClientSecretState,
    #[serde(default = "default_redirect_uri")]
    pub redirect_uri: String,
}

fn default_redirect_uri() -> String {
    "presencejam://callback".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct TeamsConfig {
    #[serde(default = "default_status_format")]
    pub status_format: String,
    #[serde(default = "default_clear_on_pause")]
    pub clear_on_pause: bool,
    #[serde(default = "default_profanity_filter")]
    pub profanity_filter: bool,
    #[serde(default = "default_profanity_placeholder")]
    pub profanity_placeholder: String,
    #[serde(default)]
    pub start_minimized: bool,
    /// P1 (issue #3.0-P1): drive the Teams presence bubble
    /// (Available/Available while a track plays) via Graph
    /// setPresence/clearPresence. OFF by default — it overrides the
    /// user's manual presence bubble.
    #[serde(default = "default_availability_sync")]
    pub availability_sync: bool,
    /// P2 (issue #3.0-P2): before writing a status message, read the
    /// user's presence and skip the write when busy/DND/in a
    /// meeting/in a call/presenting. ON by default.
    #[serde(default = "default_presence_gate")]
    pub presence_gate: bool,
    /// User-supplied extra words for the status profanity filter
    /// (CfgDiag#3(b), issue #538). Normalized once and matched under the
    /// same boundary gates as the built-in lexicon. Empty by default;
    /// bounded to 64 entries of 32 chars by `clamp_teams`.
    #[serde(default)]
    pub profanity_extra_words: Vec<String>,
    /// Finding #635 (issue #635): never overwrite a Teams status message the
    /// user set by hand. ON by default — clobbering a message the user typed
    /// ("In a workshop until 3") is the app taking over something the user
    /// owns, and the read-before-write check reuses the presence sample the
    /// gate already fetches (see `poll_once::manual_status_blocks_write`).
    #[serde(default = "default_respect_manual_status")]
    pub respect_manual_status: bool,
    /// Finding #637 (issue #637): also gate the status write while the user
    /// is marked out of office. OFF by default, matching how
    /// `availability_sync` shipped — 4.5 behaviour is unchanged until the
    /// user opts in.
    #[serde(default = "default_gate_when_out_of_office")]
    pub gate_when_out_of_office: bool,
}

fn default_status_format() -> String {
    "🎵 {artist} - {track} 🎧".to_string()
}

fn default_start_minimized() -> bool {
    false
}

fn default_clear_on_pause() -> bool {
    true
}

fn default_profanity_filter() -> bool {
    true
}

fn default_profanity_placeholder() -> String {
    profanity::safe_placeholder_default().to_string()
}

fn default_availability_sync() -> bool {
    false
}

fn default_presence_gate() -> bool {
    true
}

fn default_respect_manual_status() -> bool {
    true
}

fn default_gate_when_out_of_office() -> bool {
    false
}

#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct PollingConfig {
    #[serde(default = "default_interval_seconds")]
    pub default_interval_seconds: u64,
    #[serde(default = "default_min_interval_seconds")]
    pub minimum_interval_seconds: u64,
    #[serde(default = "default_max_interval_seconds")]
    pub max_interval_seconds: u64,
    #[serde(default = "default_expiry_buffer_seconds")]
    pub expiry_buffer_seconds: u64,
    /// Ceiling for the "paused playback" exponential backoff (CfgDiag#3(c),
    /// issue #538). `pause_backoff` hardcoded 300 s in three places; the
    /// value is now clamped into 60..=3600 by `clamp_polling`.
    #[serde(default = "default_pause_backoff_max")]
    pub pause_backoff_max_seconds: u64,
}

fn default_interval_seconds() -> u64 {
    30
}

fn default_min_interval_seconds() -> u64 {
    10
}

fn default_max_interval_seconds() -> u64 {
    60
}

fn default_expiry_buffer_seconds() -> u64 {
    10
}

fn default_pause_backoff_max() -> u64 {
    300
}
fn clamp_polling(cfg: &mut PollingConfig) {
    cfg.default_interval_seconds = cfg.default_interval_seconds.clamp(5, 300);
    cfg.minimum_interval_seconds = cfg.minimum_interval_seconds.clamp(5, 30);
    cfg.max_interval_seconds = cfg
        .max_interval_seconds
        .clamp(cfg.minimum_interval_seconds, 300);
    // CfgDiag#5 (#540): `default` is pinned to the pair AFTER both ends are
    // clamped, so `minimum <= default <= maximum` always holds. Without this
    // a persisted `{default: 300, minimum: 10, maximum: 30}` was accepted and
    // drove the no-track sleep (poll_once's `pause_backoff`) five minutes
    // past the ceiling the UI was showing as one minute.
    cfg.default_interval_seconds = cfg
        .default_interval_seconds
        .clamp(cfg.minimum_interval_seconds, cfg.max_interval_seconds);
    cfg.expiry_buffer_seconds = cfg.expiry_buffer_seconds.clamp(0, 60);
    // CfgDiag#3(c) (#538): the pause-backoff ceiling is user-configurable,
    // so clamp it into a sane band whatever the file (or the UI) said.
    cfg.pause_backoff_max_seconds = cfg.pause_backoff_max_seconds.clamp(60, 3600);
}

/// Bound the user-supplied profanity lexicon (CfgDiag#3(b), issue #538):
/// at most 64 entries, each at most 32 characters. `clamped_config` is the
/// only normalizer, so this runs on load and on every save.
fn clamp_teams(cfg: &mut TeamsConfig) {
    cfg.profanity_extra_words.truncate(64);
    for word in &mut cfg.profanity_extra_words {
        if word.chars().count() > 32 {
            *word = word.chars().take(32).collect();
        }
    }
}

/// The closed set of `availability`/`activity` pairs the Graph
/// `presence: setPresence` action accepts (finding #634, issue #634).
///
/// Quoted from https://learn.microsoft.com/graph/api/presence-setpresence:
/// "Supported combinations of availability and activity are:
/// Available/Available, Busy/InACall, Busy/InAConferenceCall, Away/Away,
/// DoNotDisturb/Presenting". `DoNotDisturb/DoNotDisturb` appears in the
/// manage-presence-state permutation table but is NOT settable through
/// setPresence, and OutOfOffice/InAMeeting "has no effect" — neither is
/// offered here, so a rule can never contain a pair Graph silently drops.
pub const PRESENCE_COMBINATIONS: [(&str, &str); 5] = [
    ("Available", "Available"),
    ("Busy", "InACall"),
    ("Busy", "InAConferenceCall"),
    ("Away", "Away"),
    ("DoNotDisturb", "Presenting"),
];

/// A validated `setPresence` pair — constructible only through
/// [`normalize_presence_pair`], so an invalid combination cannot exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresencePair {
    pub availability: String,
    pub activity: String,
}

/// Canonicalize a rule's presence pair (finding #634, issue #634).
///
/// Case-insensitive and whitespace-trimmed (a hand-edited `config.json` may
/// say `"doNotDisturb"`), and the ONLY constructor of [`PresencePair`]. Any
/// pair outside [`PRESENCE_COMBINATIONS`] — including a half-filled pair —
/// yields `None`, and [`clamp_rules`] then clears both fields, so an
/// unsupported value is normalized away at the IPC boundary exactly like
/// `clamp_polling` normalizes an out-of-range interval.
pub fn normalize_presence_pair(availability: &str, activity: &str) -> Option<PresencePair> {
    let availability = availability.trim();
    let activity = activity.trim();
    if availability.is_empty() || activity.is_empty() {
        return None;
    }
    PRESENCE_COMBINATIONS
        .iter()
        .find(|(avail, act)| {
            avail.eq_ignore_ascii_case(availability) && act.eq_ignore_ascii_case(activity)
        })
        .map(|(avail, act)| PresencePair {
            availability: (*avail).to_string(),
            activity: (*act).to_string(),
        })
}

/// Upper bound on a rule's status replacement text (finding #634). The text
/// is POSTed verbatim as the Teams status message AND embedded in the #343
/// change key, so it stays a status line rather than an essay; the Settings
/// editor mirrors this with a `maxlength` + counter so the truncation is
/// never silent.
pub const MAX_RULE_STATUS_CHARS: usize = 128;

/// Normalize the rule model (finding #634, issue #634): canonicalize every
/// presence pair and bound every replacement text. Mirrors `clamp_polling` /
/// `clamp_teams`, so it runs on load and on every save through
/// [`clamped_config`].
fn clamp_rules(cfg: &mut StatusRulesConfig) {
    for entry in &mut cfg.quiet_hours {
        clamp_presence_pair(
            &mut entry.presence_availability,
            &mut entry.presence_activity,
        );
        clamp_rule_text(&mut entry.replacement_status);
    }
    for rule in &mut cfg.track_rules {
        clamp_presence_pair(&mut rule.presence_availability, &mut rule.presence_activity);
        clamp_rule_text(&mut rule.replacement_status);
    }
}

/// Rewrite a rule's pair in place to its canonical form, or clear BOTH fields
/// when the pair is not one of [`PRESENCE_COMBINATIONS`] (an empty pair is the
/// documented "don't touch presence" value).
fn clamp_presence_pair(availability: &mut String, activity: &mut String) {
    match normalize_presence_pair(availability, activity) {
        Some(pair) => {
            *availability = pair.availability;
            *activity = pair.activity;
        }
        None => {
            availability.clear();
            activity.clear();
        }
    }
}

/// Truncate a rule's replacement text to [`MAX_RULE_STATUS_CHARS`].
fn clamp_rule_text(text: &mut String) {
    if text.chars().count() > MAX_RULE_STATUS_CHARS {
        *text = text.chars().take(MAX_RULE_STATUS_CHARS).collect();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct LoggingConfig {
    #[serde(default = "default_logging_enabled")]
    pub enabled: bool,
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_logging_enabled() -> bool {
    true
}

fn default_log_level() -> String {
    "Info".to_string()
}

/// Single place the logger's max level is wired from `logging.enabled` /
/// `logging.log_level` (CfgDiag#4, issue #539).
///
/// Called once from the `lib.rs` setup block after the startup config load
/// and again by every config write that can change logging, so a
/// `logging.enabled: false` (or a Debug-to-reproduce-a-bug switch) takes
/// effect immediately instead of at the next launch. An unrecognised level
/// string falls back to Info rather than silently disabling logging.
pub fn apply_log_level(cfg: &LoggingConfig) {
    let level_str = cfg.log_level.to_lowercase();
    let max_level = if !cfg.enabled {
        log::LevelFilter::Off
    } else {
        match level_str.as_str() {
            "off" => log::LevelFilter::Off,
            "error" => log::LevelFilter::Error,
            "warn" => log::LevelFilter::Warn,
            "info" => log::LevelFilter::Info,
            "debug" => log::LevelFilter::Debug,
            "trace" => log::LevelFilter::Trace,
            _ => log::LevelFilter::Info,
        }
    };
    log::set_max_level(max_level);
    log::info!(
        "[CFG] log level applied: {:?} (enabled={})",
        max_level,
        cfg.enabled
    );
}

/// The config schema version THIS binary writes (CfgDiag#1, issue #536).
/// Bump whenever the persisted shape gains or changes a field that needs a
/// migration.
///
/// Deliberately separate from [`default_schema_version`]: a file with no
/// `schema_version` key predates 4.3.0 and is therefore a *v1* file, so the
/// dispatcher must still run for it.
pub const SCHEMA_VERSION: u32 = 2;

fn default_schema_version() -> u32 {
    1
}

/// Make the binary — never the client — authoritative for `schema_version`
/// (CfgDiag#1, issue #536). A stale frontend payload (or a wizard literal
/// that still sends `1`) can no longer erase the record that a migration
/// already ran.
pub fn stamp_schema_version(cfg: &mut AppConfig) {
    cfg.schema_version = SCHEMA_VERSION;
}

/// Version-directed fixups run by `load_config` BEFORE the clamps (issue
/// #536). Fail-safe by construction: a file written by a newer binary keeps
/// its own (higher) version and is passed through untouched, so unknown
/// fields are never relabelled as if this binary had produced them.
fn migrate_config(cfg: &mut AppConfig, from: u32) {
    match from {
        // v1 → v2 (4.6): the three additions of CfgDiag#3 (#538) are all
        // additive with serde defaults, so there is nothing to backfill —
        // the step exists so a future breaking change has a home.
        1 => {}
        n if n > SCHEMA_VERSION => log::warn!(
            "[CFG] config written by a newer binary (schema {} > {}); passing through unknown fields",
            n,
            SCHEMA_VERSION
        ),
        _ => {}
    }
    cfg.schema_version = cfg.schema_version.max(SCHEMA_VERSION);
}

/// One quiet-hours entry for issue #432: status writes are suppressed while
/// the local time falls inside `[start_minutes, end_minutes)` (minutes
/// since midnight; wrap-around ranges like 22:00→07:00 are supported).
/// `days` holds ISO weekday numbers 1 (Mon)..=7 (Sun); empty means every
/// day. All fields `#[serde(default)]` individually so a hand-edited
/// config missing one still loads.
#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct QuietHoursEntry {
    #[serde(default)]
    pub enabled: bool,
    /// Minutes since midnight, clamped to 0..=1439 on read.
    #[serde(default)]
    pub start_minutes: u16,
    /// Minutes since midnight, clamped to 0..=1439 on read.
    #[serde(default = "default_quiet_end")]
    pub end_minutes: u16,
    /// ISO weekday numbers 1..=7; empty = every day.
    #[serde(default)]
    pub days: Vec<u8>,
    /// Optional fixed status posted while this window is active instead of
    /// suppressing the write (CfgDiag#3(a), issue #538) — e.g. "Busy" during
    /// focus hours. Empty = suppress, mirroring
    /// [`TrackRuleEntry::replacement_status`].
    #[serde(default)]
    pub replacement_status: String,
    /// setPresence pair applied while this window is active (finding #634,
    /// issue #634) — e.g. Away/Away outside working hours, so the user is
    /// visibly away instead of merely unheard. Both fields empty (the
    /// default) = don't touch presence; `clamp_rules` normalizes them against
    /// [`PRESENCE_COMBINATIONS`].
    #[serde(default)]
    pub presence_availability: String,
    #[serde(default)]
    pub presence_activity: String,
}

/// Mirrors the serde defaults field-by-field (note `end_minutes` defaults to
/// [`default_quiet_end`], not `u16::default()`), so a test fixture built with
/// `..Default::default()` and a config file missing the same field agree.
impl Default for QuietHoursEntry {
    fn default() -> Self {
        Self {
            enabled: false,
            start_minutes: 0,
            end_minutes: default_quiet_end(),
            days: Vec::new(),
            replacement_status: String::new(),
            presence_availability: String::new(),
            presence_activity: String::new(),
        }
    }
}

fn default_quiet_end() -> u16 {
    420
}

/// One track-matching rule for issue #432: when `artist_substring` /
/// `track_substring` (case-insensitive) both match the current track, the
/// rule suppresses the status write for this track exactly like the
/// presence gate — flowing through the same `gated_track_key`
/// suppression + mid-track re-evaluation path. Empty substrings match
/// everything (so a rule with only one field set still works).
#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct TrackRuleEntry {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub artist_substring: String,
    #[serde(default)]
    pub track_substring: String,
    /// Optional fixed status posted instead of suppressing (issue #432
    /// "busy/focus" alternative). Empty = suppress silently.
    #[serde(default)]
    pub replacement_status: String,
    /// setPresence pair applied while this rule matches (finding #634, issue
    /// #634) — e.g. DoNotDisturb/Presenting for a focus playlist. Both fields
    /// empty (the default) = don't touch presence; `clamp_rules` normalizes
    /// them against [`PRESENCE_COMBINATIONS`].
    #[serde(default)]
    pub presence_availability: String,
    #[serde(default)]
    pub presence_activity: String,
}

/// Mirrors the serde defaults field-by-field — see [`QuietHoursEntry`].
impl Default for TrackRuleEntry {
    fn default() -> Self {
        Self {
            enabled: false,
            artist_substring: String::new(),
            track_substring: String::new(),
            replacement_status: String::new(),
            presence_availability: String::new(),
            presence_activity: String::new(),
        }
    }
}

/// User-defined status rules for issue #432 (quiet hours + track
/// matching). Additive on `AppConfig` with `#[serde(default)]` so
/// pre-4.5 config files load unchanged (issue #379 versioning untouched:
/// `schema_version` stays 1, `extra` retention untouched).
#[derive(Debug, Clone, Default, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct StatusRulesConfig {
    #[serde(default)]
    pub quiet_hours: Vec<QuietHoursEntry>,
    #[serde(default)]
    pub track_rules: Vec<TrackRuleEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct AppConfig {
    #[serde(default)]
    pub spotify: SpotifyConfig,
    #[serde(default)]
    pub teams: TeamsConfig,
    #[serde(default)]
    pub polling: PollingConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub autostart: bool,
    #[serde(default)]
    pub status_rules: StatusRulesConfig,
    /// Config schema version (issue #379). Files written before 4.3.0 carry
    /// no such key and load as version 1.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    /// Unknown / future top-level keys, retained across load→save so a newer
    /// config file is never silently stripped by an older binary (issue #379).
    /// Skipped in the TS export (and omitted from JSON while empty) so
    /// `extra` stays byte-identical when empty. (Note: `schema_version`
    /// serializes on every save, so full-file byte-identity is not claimed
    /// across versions — only `extra` introduces no new bytes.)
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(skip)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl Default for SpotifyConfig {
    fn default() -> Self {
        Self {
            client_id: String::new(),
            client_secret_set: false,
            client_secret_state: ClientSecretState::Absent,
            redirect_uri: default_redirect_uri(),
        }
    }
}

impl Default for TeamsConfig {
    fn default() -> Self {
        Self {
            status_format: default_status_format(),
            clear_on_pause: default_clear_on_pause(),
            profanity_filter: default_profanity_filter(),
            profanity_placeholder: default_profanity_placeholder(),
            start_minimized: default_start_minimized(),
            availability_sync: default_availability_sync(),
            presence_gate: default_presence_gate(),
            profanity_extra_words: Vec::new(),
            respect_manual_status: default_respect_manual_status(),
            gate_when_out_of_office: default_gate_when_out_of_office(),
        }
    }
}

impl Default for PollingConfig {
    fn default() -> Self {
        Self {
            default_interval_seconds: default_interval_seconds(),
            minimum_interval_seconds: default_min_interval_seconds(),
            max_interval_seconds: default_max_interval_seconds(),
            expiry_buffer_seconds: default_expiry_buffer_seconds(),
            pause_backoff_max_seconds: default_pause_backoff_max(),
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            enabled: default_logging_enabled(),
            log_level: default_log_level(),
        }
    }
}

// `Default::default()` can't be derived because `LoggingConfig` uses
// `default_*()` helper functions to seed its fields with non-`Default`
// values (a default log level, a default "enabled" flag). The helper
// calls are intentional, not a candidate for `#[derive(Default)]`.
#[allow(clippy::derivable_impls)]
impl Default for AppConfig {
    fn default() -> Self {
        Self {
            spotify: SpotifyConfig::default(),
            teams: TeamsConfig::default(),
            polling: PollingConfig::default(),
            logging: LoggingConfig::default(),
            autostart: false,
            status_rules: StatusRulesConfig::default(),
            extra: BTreeMap::new(),
            schema_version: default_schema_version(),
        }
    }
}

/// Field-level patch for the `spotify` section (CfgDiag#0, issue #535).
///
/// `client_secret_set` and `client_secret_state` are deliberately absent:
/// both are derived display values filled in by [`with_keychain_flags`] from
/// the OS keychain, so a client must not be able to assert them.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ts_rs::TS)]
pub struct SpotifyPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redirect_uri: Option<String>,
}

/// Field-level patch for the `teams` section (CfgDiag#0, issue #535).
#[derive(Debug, Clone, Default, Serialize, Deserialize, ts_rs::TS)]
pub struct TeamsPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clear_on_pause: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profanity_filter: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profanity_placeholder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_minimized: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub availability_sync: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence_gate: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profanity_extra_words: Option<Vec<String>>,
}

/// Field-level patch for the `polling` section (CfgDiag#0, issue #535). Every
/// value is re-clamped by [`clamped_config`] after the merge.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ts_rs::TS)]
pub struct PollingPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_interval_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_interval_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_interval_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expiry_buffer_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pause_backoff_max_seconds: Option<u64>,
}

/// Field-level patch for the `logging` section (CfgDiag#0, issue #535).
#[derive(Debug, Clone, Default, Serialize, Deserialize, ts_rs::TS)]
pub struct LoggingPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_level: Option<String>,
}

/// Field-level patch for the `status_rules` section (CfgDiag#0, issue #535).
///
/// A named list is replaced wholesale — there is no per-entry addressing, so
/// naming `quiet_hours` means "this is the new list". Omitting it leaves the
/// stored list untouched.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ts_rs::TS)]
pub struct StatusRulesPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quiet_hours: Option<Vec<QuietHoursEntry>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track_rules: Option<Vec<TrackRuleEntry>>,
}

/// Field-level update to [`AppConfig`] for callers that only know part of
/// the document (CfgDiag#0, issue #535).
///
/// `save_config` is a whole-document replace, so every caller had to already
/// hold a complete, current `AppConfig`. A caller that did not — the setup
/// wizard being the first — silently reset everything it omitted, which is
/// the backend half of the #531 config-clobber family.
///
/// **Every field of every nested patch is `Option` and skipped when absent.**
/// That shape is load-bearing, not stylistic: typing a section as the whole
/// `TeamsConfig` would deserialize a patch of `{"teams":
/// {"status_format": "x"}}` into a fully populated `TeamsConfig` whose
/// omitted fields took their *defaults*, and assigning that section would
/// reset the user's `start_minimized`, profanity and presence settings — the
/// very clobber this command exists to prevent, reproduced one level down.
/// An absent key MUST leave the stored value untouched.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ts_rs::TS)]
pub struct ConfigPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spotify: Option<SpotifyPatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub teams: Option<TeamsPatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub polling: Option<PollingPatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logging: Option<LoggingPatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autostart: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_rules: Option<StatusRulesPatch>,
}

/// Merge a patch into `base`, field by field. Only the fields the patch
/// explicitly names are overwritten; everything else — including `extra` and
/// the binary-owned `schema_version` — is left exactly as it was.
///
/// Pure, so the merge guarantee is unit-testable without touching disk.
pub fn apply_patch(base: &mut AppConfig, patch: &ConfigPatch) {
    if let Some(p) = &patch.spotify {
        if let Some(v) = &p.client_id {
            base.spotify.client_id = v.clone();
        }
        if let Some(v) = &p.redirect_uri {
            base.spotify.redirect_uri = v.clone();
        }
    }
    if let Some(p) = &patch.teams {
        if let Some(v) = &p.status_format {
            base.teams.status_format = v.clone();
        }
        if let Some(v) = p.clear_on_pause {
            base.teams.clear_on_pause = v;
        }
        if let Some(v) = p.profanity_filter {
            base.teams.profanity_filter = v;
        }
        if let Some(v) = &p.profanity_placeholder {
            base.teams.profanity_placeholder = v.clone();
        }
        if let Some(v) = p.start_minimized {
            base.teams.start_minimized = v;
        }
        if let Some(v) = p.availability_sync {
            base.teams.availability_sync = v;
        }
        if let Some(v) = p.presence_gate {
            base.teams.presence_gate = v;
        }
        if let Some(v) = &p.profanity_extra_words {
            base.teams.profanity_extra_words = v.clone();
        }
    }
    if let Some(p) = &patch.polling {
        if let Some(v) = p.default_interval_seconds {
            base.polling.default_interval_seconds = v;
        }
        if let Some(v) = p.minimum_interval_seconds {
            base.polling.minimum_interval_seconds = v;
        }
        if let Some(v) = p.max_interval_seconds {
            base.polling.max_interval_seconds = v;
        }
        if let Some(v) = p.expiry_buffer_seconds {
            base.polling.expiry_buffer_seconds = v;
        }
        if let Some(v) = p.pause_backoff_max_seconds {
            base.polling.pause_backoff_max_seconds = v;
        }
    }
    if let Some(p) = &patch.logging {
        if let Some(v) = p.enabled {
            base.logging.enabled = v;
        }
        if let Some(v) = &p.log_level {
            base.logging.log_level = v.clone();
        }
    }
    if let Some(v) = patch.autostart {
        base.autostart = v;
    }
    if let Some(p) = &patch.status_rules {
        if let Some(v) = &p.quiet_hours {
            base.status_rules.quiet_hours = v.clone();
        }
        if let Some(v) = &p.track_rules {
            base.status_rules.track_rules = v.clone();
        }
    }
}

pub fn config_dir() -> Result<PathBuf, String> {
    // Maintained replacement for the unmaintained `dirs` crate (issue #418):
    // `directories::BaseDirs::new()` resolves the same platform config
    // roots (XDG_CONFIG_HOME/~/.config on Linux, ~/Library/Application
    // Support on macOS, %APPDATA% on Windows) and preserves the
    // `<config>/PresenceJam` layout and 0o700 creation below.
    let base_dir = directories::BaseDirs::new()
        .map(|b| b.config_dir().to_path_buf())
        .ok_or_else(|| {
            "Failed to get config directory: BaseDirs::new() returned None".to_string()
        })?;

    let app_dir = base_dir.join("PresenceJam");

    if !app_dir.exists() {
        fs::create_dir_all(&app_dir).map_err(|e| {
            format!(
                "Failed to create config directory '{}': {}",
                app_dir.display(),
                e
            )
        })?;
        #[cfg(unix)]
        {
            let _ = fs::set_permissions(&app_dir, std::fs::Permissions::from_mode(0o700));
        }
        log::info!("[CFG] Created config directory at '{}'", app_dir.display());
    }

    Ok(app_dir)
}

pub fn get_config_path() -> Result<PathBuf, String> {
    let dir = config_dir()?;
    Ok(dir.join("config.json"))
}

/// Set when `load_config` finds a corrupt config.json and quarantines it to
/// `<config>.bak` (issue #379). Diagnostics-visible via
/// [`config_was_quarantined`]; warn-log-only otherwise — no other channel is
/// touched by this slice.
static CONFIG_QUARANTINED: AtomicBool = AtomicBool::new(false);

/// Diagnostics-visible flag: true once this process has quarantined a corrupt
/// config.json to `.bak` and fallen back to defaults (issue #379).
pub fn config_was_quarantined() -> bool {
    CONFIG_QUARANTINED.load(Ordering::SeqCst)
}

/// Backup path alongside the original: `config.json` → `config.json.bak`.
fn quarantine_backup_path(path: &std::path::Path) -> PathBuf {
    let mut backup = path.as_os_str().to_owned();
    backup.push(".bak");
    PathBuf::from(backup)
}

/// Bare file name of the quarantine backup for `path` when one exists,
/// else `None`. Deliberately a bare name and never an absolute path, so the
/// diagnostics snapshot can surface it without breaching the #409
/// no-absolute-path rule (CfgDiag#2, issue #537).
fn quarantine_backup_name_for(path: &std::path::Path) -> Option<String> {
    let backup = quarantine_backup_path(path);
    if backup.is_file() {
        backup
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
    } else {
        None
    }
}

/// [`quarantine_backup_name_for`] against this process's real config path.
/// `None` when nothing was quarantined or the `.bak` has since been removed.
pub fn config_quarantine_backup_name() -> Option<String> {
    quarantine_backup_name_for(&get_config_path().ok()?)
}

/// Rename a corrupt config file alongside itself (`<name>.bak`), raise the
/// diagnostics-visible quarantine flag, and warn. Never fails the load:
/// rename errors are logged and swallowed so the caller falls back to
/// defaults either way (issue #379).
fn quarantine_corrupt_config(path: &std::path::Path, parse_err: impl std::fmt::Display) -> PathBuf {
    let backup = quarantine_backup_path(path);
    match fs::rename(path, &backup) {
        Ok(()) => log::warn!(
            "[CFG] corrupt config '{}' quarantined to '{}': {} — loading defaults",
            path.display(),
            backup.display(),
            parse_err
        ),
        Err(rename_err) => log::warn!(
            "[CFG] corrupt config '{}' failed to parse ({}) and quarantine rename to '{}' failed ({}); loading defaults",
            path.display(),
            parse_err,
            backup.display(),
            rename_err
        ),
    }
    CONFIG_QUARANTINED.store(true, Ordering::SeqCst);
    backup
}

pub fn load_config() -> Result<AppConfig, String> {
    let path = get_config_path()?;

    if !path.exists() {
        log::info!(
            "[CFG] Config file not found at '{}', using defaults",
            path.display()
        );
        return Ok(with_keychain_flags(AppConfig::default()));
    }

    // Issue #135 path A: tighten mode of any pre-existing config.json that
    // was created loose by an older PresenceJam version (default umask 022
    // → 0644). Idempotent on a file that is already 0600. Windows default
    // ACL is user-only, so this is a no-op there.
    #[cfg(unix)]
    {
        let current = fs::metadata(&path)
            .map_err(|e| format!("Failed to stat config file '{}': {}", path.display(), e))?
            .permissions();
        let current_mode = current.mode() & 0o777;
        if current_mode != 0o600 {
            log::warn!(
                "[CFG] Tightening config.json mode from {:o} to 0600 (issue #135)",
                current_mode
            );
            let mut tightened = current;
            tightened.set_mode(0o600);
            fs::set_permissions(&path, tightened).map_err(|e| {
                format!(
                    "Failed to chmod config file '{}' to 0600: {}",
                    path.display(),
                    e
                )
            })?;
        }
    }

    let mut file = fs::File::open(&path)
        .map_err(|e| format!("Failed to open config file '{}': {}", path.display(), e))?;

    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .map_err(|e| format!("Failed to read config file '{}': {}", path.display(), e))?;

    let mut config: AppConfig = match serde_json::from_str(&contents) {
        Ok(cfg) => cfg,
        Err(e) => {
            // Issue #379: never lose the evidence — quarantine the corrupt
            // file to `<config>.bak` alongside the original and boot on
            // defaults. Observable via `config_was_quarantined()`.
            quarantine_corrupt_config(&path, &e);
            return Ok(with_keychain_flags(AppConfig::default()));
        }
    };
    // CfgDiag#1 (#536): the version dispatcher runs BEFORE the clamps, so a
    // migration can never have its rewritten values re-clamped away, and
    // `schema_version` is raised even for a file that was never saved by
    // this binary.
    let from_version = config.schema_version;
    migrate_config(&mut config, from_version);
    clamp_polling(&mut config.polling);
    clamp_teams(&mut config.teams);
    clamp_rules(&mut config.status_rules);

    log::info!("[CFG] Loaded configuration from '{}'", path.display());
    Ok(with_keychain_flags(config))
}

/// Populate derived/display fields that are not persisted to disk.
///
/// Covers the two Spotify keychain views: `client_secret_set` (the pre-#560
/// `Present`-only flag) and `client_secret_state` (the tri-state that can say
/// "the keychain could not answer"). See issues #9 and #560.
///
/// One probe feeds both: `spotify_client_secret_presence` never reads the
/// in-process cache, so it still notices a credential deleted from the OS UI
/// while the app runs — and this function only runs on config load, off the
/// polling hot path (issue #69).
fn with_keychain_flags(mut config: AppConfig) -> AppConfig {
    let presence = crate::keychain::spotify_client_secret_presence();
    config.spotify.client_secret_set =
        matches!(presence, crate::keychain::KeychainPresence::Present);
    config.spotify.client_secret_state = ClientSecretState::from(&presence);
    config
}
/// Frontend event emitted (once per process) when the legacy-plaintext
/// migration finds a *different* secret already in the OS keychain.
///
/// The Settings view should listen for this event and prompt the user to
/// run Settings → Reconnect Spotify. See issue #376.
pub const SPOTIFY_SECRET_CONFLICT_EVENT: &str = "spotify-secret-conflict";
/// One-shot startup migration for the legacy `spotify.client_secret`
/// field (≤ v2.5.0): write it to
/// the OS keychain and strip the plaintext from the file. Idempotent
/// and safe to call on every startup.
///
/// Conflict policy: if the keychain already holds a *different*
/// secret, the migration is a no-op (we don't clobber a working
/// keychain entry with another install's plaintext, and we do NOT delete
/// the plaintext unilaterally — the user may need it). The user resolves
/// the conflict via Settings → Reconnect Spotify. See audit Q3 and
/// issues #9 and #376.
///
/// Bounded notification: the conflict is surfaced exactly once per process
/// (see `migrate_legacy_client_secret_with_app`; a process-wide flag guards
/// the emit) — there is no retry loop or timeout that auto-deletes the
/// plaintext. Manual step: after Reconnect Spotify stores the current
/// secret in the keychain, the next launch either completes the migration
/// (keychain empty / identical value → plaintext stripped) or re-emits
/// this event while the stale plaintext is still present.
/// Log-only variant kept for backward compatibility (no `AppHandle`
/// available at some call sites). Prefer
/// `migrate_legacy_client_secret_with_app`, which additionally surfaces a
/// keychain conflict to the UI via [`SPOTIFY_SECRET_CONFLICT_EVENT`].
pub fn migrate_legacy_client_secret() {
    run_legacy_secret_migration();
}
/// Startup migration with user-visible conflict surfacing (issue #376).
///
/// Runs the same migration as [`migrate_legacy_client_secret`]; when the
/// outcome is [`LegacySecretOutcome::ConflictKeychainDiffers`], emits a
/// one-time [`SPOTIFY_SECRET_CONFLICT_EVENT`] so Settings can prompt
/// Settings → Reconnect Spotify (payload carries the manual step).
/// All other outcomes are silent apart from the usual `[CFG]` logs.
///
/// Wiring note (orchestrator): `lib.rs` setup currently calls the log-only
/// `migrate_legacy_client_secret()`; swap that call site to
/// `config::migrate_legacy_client_secret_with_app(app.handle())` so the
/// conflict becomes user-visible. This file is slice-D owned, so the
/// one-line swap lives outside this change.
pub fn migrate_legacy_client_secret_with_app(app: &tauri::AppHandle) {
    if run_legacy_secret_migration() == LegacySecretOutcome::ConflictKeychainDiffers {
        emit_spotify_secret_conflict_once(app);
    }
}
/// Observable outcome of one [`run_legacy_secret_migration`] pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegacySecretOutcome {
    /// No `spotify.client_secret` plaintext field (or it was empty):
    /// nothing to do. Also returned when the config file is missing,
    /// unreadable, or unparsable, or a keychain write / file rewrite
    /// failed part-way (plaintext left on disk in those cases).
    NoLegacyField,
    /// Plaintext migrated into an empty keychain (or the keychain
    /// already held the identical value) and the strip pass ran.
    Migrated,
    /// Keychain already holds a *different* secret: plaintext deliberately
    /// left on disk. The caller must surface this via
    /// [`SPOTIFY_SECRET_CONFLICT_EVENT`].
    ConflictKeychainDiffers,
}
/// Pure decision step of the migration: given the legacy plaintext (if any)
/// and the current keychain read, decide the outcome without touching disk
/// or the keychain. Unit-tested directly (issue #376).
fn decide_legacy_secret_outcome(
    plaintext: Option<&str>,
    keychain: &Result<String, String>,
) -> LegacySecretOutcome {
    let plaintext = match plaintext {
        Some(s) if !s.is_empty() => s,
        _ => return LegacySecretOutcome::NoLegacyField,
    };
    match keychain {
        Ok(existing) if existing == plaintext => LegacySecretOutcome::Migrated,
        Ok(_) => LegacySecretOutcome::ConflictKeychainDiffers,
        Err(_) => LegacySecretOutcome::Migrated,
    }
}
/// Process-wide guard so the conflict event fires at most once per launch,
/// no matter how often the migration entry points are called.
static CONFLICT_EVENT_SENT: AtomicBool = AtomicBool::new(false);
/// Emit [`SPOTIFY_SECRET_CONFLICT_EVENT`] unless already sent this process.
/// Follows the `let _ = app.emit(...)` pattern used in `poll_once.rs`;
/// the payload tells Settings to prompt Reconnect Spotify. Returns true
/// when this call performed the (single) emit.
fn emit_spotify_secret_conflict_once(app: &tauri::AppHandle) -> bool {
    if CONFLICT_EVENT_SENT.swap(true, Ordering::AcqRel) {
        return false;
    }
    log::warn!(
        "[CFG] migrate_legacy_client_secret: EMIT {} event (prompt Settings → Reconnect Spotify)",
        SPOTIFY_SECRET_CONFLICT_EVENT
    );
    let _ = app.emit(
        SPOTIFY_SECRET_CONFLICT_EVENT,
        serde_json::json!({
            "action": "reconnect-spotify",
            "message": "The Spotify client secret in config.json differs from the one in the OS keychain. Open Settings → Reconnect Spotify to resolve. The legacy plaintext is left untouched until then.",
        }),
    );
    true
}
/// Executes the migration IO and returns its observable outcome.
fn run_legacy_secret_migration() -> LegacySecretOutcome {
    let path = match get_config_path() {
        Ok(p) => p,
        Err(e) => {
            log::warn!(
                "[CFG] migrate_legacy_client_secret: config path unavailable: {}",
                e
            );
            return LegacySecretOutcome::NoLegacyField;
        }
    };
    if !path.exists() {
        return LegacySecretOutcome::NoLegacyField; // Fresh install — nothing to migrate.
    }
    let contents = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("[CFG] migrate_legacy_client_secret: read failed: {}", e);
            return LegacySecretOutcome::NoLegacyField;
        }
    };
    // Parse as raw Value so we can inspect the pre-v2.6.0 nested
    // `spotify.client_secret` field. (`SpotifyConfig` declares no such
    // field, so `serde_json::from_str::<AppConfig>` would discard it
    // before we got a chance to migrate. Top-level unknown keys are
    // retained in `AppConfig::extra` since issue #379, but nested unknown
    // keys are still dropped — hence the raw `Value` here.)
    let mut root: serde_json::Value = match serde_json::from_str(&contents) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("[CFG] migrate_legacy_client_secret: parse failed: {}", e);
            return LegacySecretOutcome::NoLegacyField;
        }
    };
    let plaintext = root
        .get("spotify")
        .and_then(|s| s.get("client_secret"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let keychain_read = crate::keychain::get_spotify_client_secret();
    let outcome = decide_legacy_secret_outcome(plaintext.as_deref(), &keychain_read);
    match (&outcome, &keychain_read) {
        (LegacySecretOutcome::NoLegacyField, _) => {
            log::debug!("[CFG] migrate_legacy_client_secret: no legacy plaintext field");
            return outcome;
        }
        (LegacySecretOutcome::ConflictKeychainDiffers, Ok(existing)) => {
            // Conflict check: if keychain already holds a *different* secret,
            // don't clobber it. Leave the plaintext in place; the user can
            // resolve via Settings → Reconnect Spotify (surfaced via
            // `spotify-secret-conflict`; see `migrate_legacy_client_secret_with_app`).
            let plaintext = plaintext.unwrap_or_default();
            log::warn!(
                "[CFG] migrate_legacy_client_secret: keychain holds a different secret; leaving config.json untouched (user should Reconnect)"
            );
            log::warn!(
                "[CFG] migrate_legacy_client_secret: plaintext.len={}, keychain.len={}",
                plaintext.len(),
                existing.len()
            );
            return outcome;
        }
        _ => {
            // Keychain empty (the typical pre-v2.6.0-upgrader case), or it
            // already holds the identical value (strip-only). Write the
            // plaintext into the keychain only when the keychain is empty.
            if keychain_read.is_err() {
                log::info!("[CFG] migrate_legacy_client_secret: keychain empty, writing plaintext into keychain");
                // `plaintext` is `Some(non-empty)` here: `decide_*` only
                // returns `Migrated` for `Some(non-empty)` input.
                let plaintext = plaintext.unwrap_or_default();
                if let Err(e) = crate::keychain::store_spotify_client_secret(&plaintext) {
                    log::warn!(
                        "[CFG] migrate_legacy_client_secret: keychain write failed: {} (plaintext left in config.json)",
                        e
                    );
                    return LegacySecretOutcome::NoLegacyField;
                }
            } else {
                log::info!(
                    "[CFG] migrate_legacy_client_secret: keychain already holds this value, stripping plaintext only"
                );
            }
        }
    }
    // Strip the plaintext field and re-serialise.
    if let Some(spotify_obj) = root.get_mut("spotify").and_then(|v| v.as_object_mut()) {
        spotify_obj.remove("client_secret");
    }
    let new_contents = match serde_json::to_string_pretty(&root) {
        Ok(s) => s,
        Err(e) => {
            log::warn!(
                "[CFG] migrate_legacy_client_secret: re-serialise failed: {}",
                e
            );
            return LegacySecretOutcome::NoLegacyField;
        }
    };
    if let Err(e) = atomic_write_json(&path, &new_contents) {
        log::warn!(
            "[CFG] migrate_legacy_client_secret: atomic rewrite failed: {} (keychain has the value, plaintext remains on disk)",
            e
        );
    } else {
        log::info!(
            "[CFG] migrate_legacy_client_secret: SUCCESS — plaintext stripped from config.json"
        );
    }
    LegacySecretOutcome::Migrated
}

fn atomic_write_json(path: &std::path::Path, json: &str) -> Result<(), String> {
    let temp_path = path.with_extension("tmp");

    // Issue #135 path A: create the temp file with mode 0600 atomically.
    // Pre-clear any stale sidecar from a previous crash (between temp-write
    // and rename). Without this pre-clear, create_new(true) would error with
    // AlreadyExists on a leftover `.tmp`, turning a one-off crash into a
    // permanent save failure until the user manually deletes the sidecar.
    // Deletion of a non-existent file is fine — we ignore NotFound.
    if let Err(e) = fs::remove_file(&temp_path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            return Err(format!(
                "Failed to remove stale temp file '{}': {}",
                temp_path.display(),
                e
            ));
        }
    }
    // OpenOptions::create_new(true) prevents racing with a leftover sidecar;
    // .mode(0o600) sets the mode at file-creation time (no chmod-after-create
    // window where config.json could briefly sit world-readable). The
    // subsequent rename() preserves the source mode on POSIX. On Windows,
    // the new file inherits the user-only default ACL of the parent.
    #[cfg(unix)]
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp_path)
        .map_err(|e| {
            format!(
                "Failed to create temp file '{}': {}",
                temp_path.display(),
                e
            )
        })?;

    #[cfg(not(unix))]
    let mut file = fs::File::create(&temp_path).map_err(|e| {
        format!(
            "Failed to create temp file '{}': {}",
            temp_path.display(),
            e
        )
    })?;

    file.write_all(json.as_bytes())
        .map_err(|e| format!("Failed to write temp file '{}': {}", temp_path.display(), e))?;

    file.sync_all()
        .map_err(|e| format!("Failed to sync temp file '{}': {}", temp_path.display(), e))?;

    std::fs::rename(&temp_path, path)
        .map_err(|e| format!("Failed to rename temp file to '{}': {}", path.display(), e))?;
    #[cfg(unix)]
    {
        if let Some(parent) = path.parent() {
            if let Ok(dir) = std::fs::File::open(parent) {
                if let Err(e) = dir.sync_all() {
                    log::warn!(
                        "[CFG] Failed to fsync config dir '{}': {}",
                        parent.display(),
                        e
                    );
                }
            }
        }
    }

    Ok(())
}

/// The config as it will actually be persisted: `clamp_polling` applied.
///
/// `save_config` writes a clamped copy, so the caller must store THIS value
/// in `AppState` rather than its own unclamped input — otherwise a value the
/// UI can type (the number inputs' `min`/`max` attributes do not constrain a
/// typed value) lives in memory while a different one sits on disk, and the
/// two silently reconcile only on the next launch. See issue #297.
pub fn clamped_config(config: &AppConfig) -> AppConfig {
    let mut cfg = config.clone();
    clamp_polling(&mut cfg.polling);
    clamp_teams(&mut cfg.teams);
    clamp_rules(&mut cfg.status_rules);
    cfg
}

pub fn save_config(config: &AppConfig) -> Result<(), String> {
    let path = get_config_path()?;

    let mut cfg = clamped_config(config);
    // CfgDiag#1 (#536): the client's `schema_version` is a suggestion, not
    // an instruction — a stale payload can never lower the version.
    stamp_schema_version(&mut cfg);
    let json = serde_json::to_string_pretty(&cfg)
        .map_err(|e| format!("Failed to serialize config to JSON: {}", e))?;

    atomic_write_json(&path, &json)?;

    log::info!("[CFG] Saved configuration to '{}'", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    static QUARANTINE_TEST_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.spotify.redirect_uri, "presencejam://callback");
        assert_eq!(config.teams.status_format, "🎵 {artist} - {track} 🎧");
        assert!(config.teams.clear_on_pause);
        assert!(config.teams.profanity_filter);
        assert_eq!(
            config.teams.profanity_placeholder,
            profanity::safe_placeholder_default()
        );
        // Issue #3.0-P1/P2: availability sync OFF, presence gate ON.
        assert!(!config.teams.availability_sync);
        assert!(config.teams.presence_gate);
        assert_eq!(config.polling.default_interval_seconds, 30);
        assert!(config.logging.enabled);
    }

    /// Issue #560: the whole point of the tri-state is that `Unavailable`
    /// survives the keychain → config → wire boundary as a *distinct* state.
    /// Anything that folded it into `Absent` (or into the `Present`-only
    /// bool) would put a locked-keyring user back in the setup wizard with
    /// their secret still stored.
    #[test]
    fn client_secret_state_maps_keychain_presence() {
        use crate::keychain::KeychainPresence;
        assert_eq!(
            ClientSecretState::from(&KeychainPresence::Present),
            ClientSecretState::Present
        );
        assert_eq!(
            ClientSecretState::from(&KeychainPresence::Absent),
            ClientSecretState::Absent
        );
        assert_eq!(
            ClientSecretState::from(&KeychainPresence::Unavailable("keyring locked".into())),
            ClientSecretState::Unavailable
        );
    }

    /// The frontend switches on these three literals, so the spelling is part
    /// of the wire contract — and both directions must round-trip.
    #[test]
    fn client_secret_state_wire_spelling() {
        for (state, wire) in [
            (ClientSecretState::Present, "\"present\""),
            (ClientSecretState::Absent, "\"absent\""),
            (ClientSecretState::Unavailable, "\"unavailable\""),
        ] {
            assert_eq!(serde_json::to_string(&state).unwrap(), wire);
            assert_eq!(
                serde_json::from_str::<ClientSecretState>(wire).unwrap(),
                state
            );
        }
    }

    /// A `config.json` written before #560 has no `client_secret_state` key;
    /// it must still load, defaulting to the state that does not accuse the
    /// keychain of anything (`with_keychain_flags` re-stamps it on every load
    /// anyway — the default only has to be safe, never authoritative).
    #[test]
    fn pre_4_6_config_json_still_loads() {
        let legacy: SpotifyConfig = serde_json::from_str(
            r#"{"client_id":"abc","client_secret_set":true,"redirect_uri":"presencejam://callback"}"#,
        )
        .expect("a config.json written before #560 must still deserialize");
        assert_eq!(legacy.client_secret_state, ClientSecretState::Absent);
        assert!(legacy.client_secret_set);
    }

    /// Regression guard for issue found in PR review: a redundant
    /// `fs::remove_file(path)` before the final `rename` opened a window
    /// where a process crash leaves config.json missing. Drop the
    /// remove_file; rename() atomically replaces the destination on
    /// POSIX + same-volume Windows renames. Mirrors token_io.rs pattern.
    #[test]
    fn test_atomic_write_json_replaces_existing_file() {
        let dir = std::env::temp_dir().join(format!(
            "pj-test-{}-{}",
            std::process::id(),
            chrono_like_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, b"OLD_CONTENTS_AAA").unwrap();

        atomic_write_json(&path, "NEW_CONTENTS_BBB").expect("write should succeed");

        // After atomic_write_json, the destination must hold the new bytes
        // (no mix with the old), and there must be no leftover .tmp sidecar.
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "NEW_CONTENTS_BBB");
        let sidecar = path.with_extension("tmp");
        assert!(
            !sidecar.exists(),
            "temp sidecar {} must be consumed by rename (rename atomicity)",
            sidecar.display()
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Source-level regression guard for the crash-window bug: a redundant
    /// `fs::remove_file(path)` between temp-write-fsync and `rename()`
    /// breaks the rename-atomicity guarantee (rename atomically replaces
    /// the destination on POSIX + Windows same-volume renames, removing
    /// first leaves a crash window where the destination is gone and the
    /// rename never happens).
    ///
    /// Robust anchor: walk a brace count from the first `{` after the
    /// `fn atomic_write_json(...)` signature. The body's `{`/`}` count is
    /// independent of what other functions are declared around it, so this
    /// test survives reordering / splitting / renaming of adjacent code.
    #[test]
    fn test_atomic_write_json_does_not_remove_destination_first() {
        let src = include_str!("config.rs");
        // Find the function signature (the line that starts the body).
        let sig_idx = src
            .find("fn atomic_write_json(")
            .expect("atomic_write_json must exist");
        // Walk forward until the first `{`, then count braces to find the
        // matching `}`. Robust to whatever comes after the function.
        let brace_open_rel = src[sig_idx..]
            .find('{')
            .expect("atomic_write_json body must have an opening brace");
        let body_start = sig_idx + brace_open_rel;
        let mut depth: u32 = 0;
        let mut i = body_start;
        let body_end = loop {
            let ch = src.as_bytes()[i];
            match ch {
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
                panic!("atomic_write_json body has unbalanced braces");
            }
        };
        let body = &src[body_start + 1..body_end];
        // Allow `remove_file(&temp_path)` (pre-clearing a stale sidecar from
        // a prior crash, introduced by issue #135 path A) but forbid
        // `remove_file(path)` (removing the destination before rename would
        // break the rename-atomicity guarantee). The latter is the original
        // PR #133 regression. We anchor on the destination-path identifier
        // to be string-literal-safe (the brace counter excludes braces
        // inside string contents only by accident, so we rely on the
        // specific `remove_file(path` token rather than free-form
        // `remove_file`).
        assert!(
            !body.contains("remove_file(path"),
            "atomic_write_json must not call remove_file(path) on the destination \
             before rename — rename atomically replaces the destination on POSIX \
             + Windows same-volume renames, and the explicit remove breaks \
             crash-safety. `remove_file(&temp_path)` on the sidecar IS allowed \
             (issue #135 path A: pre-clear stale sidecar from a prior crash). \
             See ARCHITECTURE.md 'Storage' section and PR #133 for the \
             original regression context."
        );
    }

    fn chrono_like_nanos() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }

    /// Regression guard for issue #135: a stale `.tmp` from a previous crash
    /// must not block the next write. Without the pre-clear, the new
    /// create_new(true) on a leftover sidecar would error with AlreadyExists
    /// and turn a one-off crash into a permanent save failure.
    #[test]
    fn test_atomic_write_json_recovers_from_stale_tmp_sidecar() {
        let dir = std::env::temp_dir().join(format!(
            "pj-test-recover-{}-{}",
            std::process::id(),
            chrono_like_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let sidecar = path.with_extension("tmp");

        // Simulate a previous crash that left the sidecar behind.
        std::fs::write(&sidecar, b"PARTIAL_GARBAGE_FROM_CRASH").unwrap();
        assert!(sidecar.exists(), "sidecar must exist before recovery");

        atomic_write_json(&path, "NEW_CONTENTS_AFTER_CRASH")
            .expect("write must succeed despite stale sidecar");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "NEW_CONTENTS_AFTER_CRASH"
        );
        assert!(
            !sidecar.exists(),
            "sidecar must be consumed by rename (no .tmp leftover)"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Issue #376: the keychain/config conflict branch must resolve to a
    /// dedicated outcome (which the `_with_app` entry point turns into a
    /// user-visible `spotify-secret-conflict` event) — never silently to
    /// `Migrated` (which strips the file) or `NoLegacyField` (which stays
    /// log-only). Case from the issue repro: config plaintext "AAA" vs
    /// keychain "BBB".
    #[test]
    fn test_decide_legacy_secret_conflict_keychain_differs() {
        let outcome = decide_legacy_secret_outcome(Some("AAA"), &Ok("BBB".to_string()));
        assert_eq!(outcome, LegacySecretOutcome::ConflictKeychainDiffers);
    }
    #[test]
    fn test_decide_legacy_secret_outcome_matrix() {
        // No plaintext at all (fresh install / already migrated).
        assert_eq!(
            decide_legacy_secret_outcome(None, &Err("empty".to_string())),
            LegacySecretOutcome::NoLegacyField
        );
        // Empty-string field is not a secret.
        assert_eq!(
            decide_legacy_secret_outcome(Some(""), &Err("empty".to_string())),
            LegacySecretOutcome::NoLegacyField
        );
        // Empty keychain (typical pre-v2.6.0 upgrader): migrate + strip.
        assert_eq!(
            decide_legacy_secret_outcome(Some("AAA"), &Err("empty".to_string())),
            LegacySecretOutcome::Migrated
        );
        // Keychain already holds the identical value: strip-only.
        assert_eq!(
            decide_legacy_secret_outcome(Some("AAA"), &Ok("AAA".to_string())),
            LegacySecretOutcome::Migrated
        );
        // Same value with surrounding keychain state must not count as a
        // conflict: equality is exact, so near-misses still conflict.
        assert_eq!(
            decide_legacy_secret_outcome(Some("AAA"), &Ok("AAA ".to_string())),
            LegacySecretOutcome::ConflictKeychainDiffers
        );
    }
    /// The frontend listens on the literal event name, so a rename of the
    /// constant silently breaks Settings without a compile error on either
    /// side. Pin the contract string (issue #376).
    #[test]
    fn test_spotify_secret_conflict_event_name_contract() {
        assert_eq!(SPOTIFY_SECRET_CONFLICT_EVENT, "spotify-secret-conflict");
    }

    /// Issue #379: files written before `schema_version` existed must load
    /// as version 1.
    #[test]
    fn test_schema_version_defaults_to_1_when_absent() {
        let cfg: AppConfig = serde_json::from_str("{}").expect("empty object must parse");
        assert_eq!(cfg.schema_version, 1);
        assert_eq!(AppConfig::default().schema_version, 1);
    }

    /// Issue #379: an explicit schema_version round-trips untouched.
    #[test]
    fn test_schema_version_round_trip() {
        let cfg: AppConfig = serde_json::from_str(r#"{"schema_version": 3}"#).expect("must parse");
        assert_eq!(cfg.schema_version, 3);
        let json = serde_json::to_string(&cfg).expect("must serialize");
        let back: AppConfig = serde_json::from_str(&json).expect("must re-parse");
        assert_eq!(back.schema_version, 3);
    }

    /// Issue #379: unknown top-level keys survive a save round-trip via
    /// `extra` instead of being silently stripped.
    #[test]
    fn test_unknown_future_key_survives_save_round_trip() {
        let cfg: AppConfig =
            serde_json::from_str(r#"{"autostart": true, "future_key": {"nested": [1, 2, 3]}}"#)
                .expect("must parse");
        assert_eq!(
            cfg.extra
                .get("future_key")
                .expect("future_key must be retained"),
            &serde_json::json!({"nested": [1, 2, 3]})
        );
        // `save_config` serialises a `clamped_config` clone with
        // `to_string_pretty`; clamping only touches polling and the
        // profanity lexicon, so this exercises the same serde path as a real
        // save without touching the user's config file.
        let json = serde_json::to_string_pretty(&clamped_config(&cfg)).expect("must serialize");
        assert!(
            json.contains("future_key"),
            "serialised config must still carry the unknown key"
        );
        let back: AppConfig = serde_json::from_str(&json).expect("must re-parse");
        assert_eq!(back.extra.get("future_key"), cfg.extra.get("future_key"));
        assert!(back.autostart);
    }

    /// Issue #379: a corrupt config file is quarantined to `<name>.bak`
    /// alongside the original and the diagnostics-visible flag is raised.
    #[test]
    fn test_corrupt_config_quarantined_to_bak() {
        // Process-wide CONFIG_QUARANTINED is global: serialize the two
        // quarantine tests so parallel save/restore cannot interleave.
        let _guard = QUARANTINE_TEST_LOCK.lock();
        let prev = CONFIG_QUARANTINED.load(Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "pj-test-quarantine-{}-{}",
            std::process::id(),
            chrono_like_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, b"{ NOT VALID JSON !!!").unwrap();
        // Precondition: the file is genuinely unparsable as AppConfig.
        assert!(
            serde_json::from_str::<AppConfig>(&std::fs::read_to_string(&path).unwrap()).is_err()
        );

        let backup = quarantine_corrupt_config(&path, "test corrupt sentinel");
        assert_eq!(backup, dir.join("config.json.bak"));
        assert!(!path.exists(), "corrupt original must be renamed away");
        assert_eq!(
            std::fs::read(&backup).unwrap(),
            b"{ NOT VALID JSON !!!",
            "quarantined copy must preserve the corrupt bytes"
        );
        assert!(
            config_was_quarantined(),
            "diagnostics-visible flag must be raised after quarantine"
        );
        // Defaults remain loadable alongside the quarantine.
        assert_eq!(AppConfig::default().schema_version, 1);

        let _ = std::fs::remove_dir_all(&dir);
        CONFIG_QUARANTINED.store(prev, Ordering::SeqCst);
    }

    /// Issue #379: when the quarantine rename itself fails (e.g. a
    /// directory already occupies the `<name>.bak` target), the load must
    /// still fall back to defaults — the corrupt original is preserved and
    /// the diagnostics-visible flag is raised either way.
    #[test]
    fn test_corrupt_config_quarantine_rename_failure_preserves_original() {
        // Process-wide CONFIG_QUARANTINED is global: serialize the two
        // quarantine tests so parallel save/restore cannot interleave.
        let _guard = QUARANTINE_TEST_LOCK.lock();
        let prev = CONFIG_QUARANTINED.load(Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "pj-test-quarantine-fail-{}-{}",
            std::process::id(),
            chrono_like_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let corrupt_bytes = b"{ NOT VALID JSON !!!";
        std::fs::write(&path, corrupt_bytes).unwrap();
        // Colliding target: a directory at the backup path makes
        // `fs::rename(file, dir)` fail on both POSIX and Windows.
        std::fs::create_dir_all(dir.join("config.json.bak")).unwrap();

        let backup = quarantine_corrupt_config(&path, "test rename-failure sentinel");
        assert_eq!(backup, dir.join("config.json.bak"));
        // Rename failed → the corrupt original must still be in place with
        // its bytes untouched, and the flag is raised so diagnostics still
        // observe the quarantine attempt.
        assert!(
            path.exists(),
            "corrupt original must be preserved when the quarantine rename fails"
        );
        assert_eq!(
            std::fs::read(&path).unwrap(),
            corrupt_bytes,
            "failed quarantine must not truncate or alter the original"
        );
        assert!(
            config_was_quarantined(),
            "diagnostics-visible flag must be raised even when the rename fails"
        );
        // Defaults remain loadable alongside the failed quarantine.
        assert_eq!(AppConfig::default().schema_version, 1);

        let _ = std::fs::remove_dir_all(&dir);
        CONFIG_QUARANTINED.store(prev, Ordering::SeqCst);
    }

    /// Issue #379: `extra` keys serialize in deterministic (sorted) order so
    /// multi-key future payloads do not flap between saves.
    #[test]
    fn test_extra_keys_serialize_in_sorted_order() {
        let mut cfg = AppConfig::default();
        cfg.extra.insert("zeta".to_string(), serde_json::json!(1));
        cfg.extra.insert("alpha".to_string(), serde_json::json!(2));
        cfg.extra.insert("mid".to_string(), serde_json::json!(3));
        let json = serde_json::to_string(&cfg).expect("must serialize");
        let alpha = json.find("\"alpha\"").expect("alpha must serialize");
        let mid = json.find("\"mid\"").expect("mid must serialize");
        let zeta = json.find("\"zeta\"").expect("zeta must serialize");
        assert!(
            alpha < mid && mid < zeta,
            "extra keys must serialize in sorted order, got: {json}"
        );
        let back: AppConfig = serde_json::from_str(&json).expect("must re-parse");
        assert_eq!(back.extra, cfg.extra);
    }

    /// Issue #432: pre-4.5 config files without `status_rules` load with
    /// empty rule lists (serde default), and the #379 versioning is
    /// untouched — schema_version still defaults to 1.
    #[test]
    fn test_status_rules_default_empty_when_absent() {
        let cfg: AppConfig = serde_json::from_str(r#"{"autostart": true}"#).expect("must parse");
        assert!(cfg.status_rules.quiet_hours.is_empty());
        assert!(cfg.status_rules.track_rules.is_empty());
        assert_eq!(cfg.schema_version, 1);
        assert_eq!(AppConfig::default().status_rules.quiet_hours.len(), 0);
    }

    /// Issue #432: rules round-trip through serde with per-field defaults
    /// (a hand-edited entry missing optional fields still loads).
    #[test]
    fn test_status_rules_round_trip() {
        let cfg: AppConfig = serde_json::from_str(
            r#"{"status_rules": {"quiet_hours": [{"enabled": true, "start_minutes": 1320}], "track_rules": [{"enabled": true, "artist_substring": "lofi"}]}}"#,
        )
        .expect("must parse");
        assert_eq!(cfg.status_rules.quiet_hours.len(), 1);
        assert_eq!(cfg.status_rules.quiet_hours[0].start_minutes, 1320);
        // `end_minutes` absent → default_quiet_end (07:00).
        assert_eq!(cfg.status_rules.quiet_hours[0].end_minutes, 420);
        assert!(cfg.status_rules.quiet_hours[0].days.is_empty());
        assert_eq!(cfg.status_rules.track_rules.len(), 1);
        assert!(cfg.status_rules.track_rules[0]
            .replacement_status
            .is_empty());
        let json = serde_json::to_string(&cfg).expect("must serialize");
        let back: AppConfig = serde_json::from_str(&json).expect("must re-parse");
        assert_eq!(back.status_rules.quiet_hours.len(), 1);
        assert_eq!(back.status_rules.track_rules.len(), 1);
    }

    // ---------------------------------------------------------------
    // CfgDiag#5 (#540): `minimum <= default <= maximum` is an invariant.
    // ---------------------------------------------------------------

    /// Hostile-but-persistable inputs (the Settings number inputs' `min`/
    /// `max` attributes do not constrain a typed value, and `clamp_polling`
    /// is the only normalizer) must land on the invariant, not merely on
    /// each field's own range.
    #[test]
    fn test_clamp_polling_pins_default_between_min_and_max() {
        let mut polling = PollingConfig {
            default_interval_seconds: 300,
            minimum_interval_seconds: 10,
            max_interval_seconds: 30,
            expiry_buffer_seconds: 10,
            pause_backoff_max_seconds: 300,
        };
        clamp_polling(&mut polling);
        assert_eq!(polling.max_interval_seconds, 30);
        assert_eq!(polling.minimum_interval_seconds, 10);
        assert_eq!(
            polling.default_interval_seconds, 30,
            "a default above the max must be pulled down to the max"
        );

        // The mirror case: a default below the minimum.
        let mut polling = PollingConfig {
            default_interval_seconds: 5,
            minimum_interval_seconds: 20,
            max_interval_seconds: 60,
            expiry_buffer_seconds: 10,
            pause_backoff_max_seconds: 300,
        };
        clamp_polling(&mut polling);
        assert_eq!(polling.default_interval_seconds, 20);

        // Out-of-range fields still clamp to their own bands first, and the
        // invariant survives the combination.
        let mut polling = PollingConfig {
            default_interval_seconds: 9999,
            minimum_interval_seconds: 9999,
            max_interval_seconds: 9999,
            expiry_buffer_seconds: 9999,
            pause_backoff_max_seconds: 9999,
        };
        clamp_polling(&mut polling);
        assert!(polling.minimum_interval_seconds <= polling.default_interval_seconds);
        assert!(polling.default_interval_seconds <= polling.max_interval_seconds);
        assert_eq!(polling.expiry_buffer_seconds, 60);
        assert_eq!(polling.pause_backoff_max_seconds, 3600);

        // CfgDiag#3(c): the backoff ceiling has a floor too.
        let mut polling = PollingConfig {
            pause_backoff_max_seconds: 1,
            ..PollingConfig::default()
        };
        clamp_polling(&mut polling);
        assert_eq!(polling.pause_backoff_max_seconds, 60);
    }

    // ---------------------------------------------------------------
    // CfgDiag#0 (#535): a partial-knowledge caller cannot lose fields.
    // ---------------------------------------------------------------

    /// A config that differs from `AppConfig::default()` in EVERY field a
    /// patch can name, so an accidental whole-section overwrite shows up as a
    /// changed value instead of coincidentally matching a default.
    fn non_default_config() -> AppConfig {
        let mut cfg = AppConfig {
            autostart: true,
            schema_version: SCHEMA_VERSION,
            ..AppConfig::default()
        };
        cfg.spotify.client_id = "stored-client-id".to_string();
        cfg.spotify.redirect_uri = "presencejam://stored".to_string();
        cfg.teams.status_format = "Stored {artist}".to_string();
        cfg.teams.clear_on_pause = false;
        cfg.teams.profanity_filter = false;
        cfg.teams.profanity_placeholder = "Custom placeholder".to_string();
        cfg.teams.start_minimized = true;
        cfg.teams.availability_sync = true;
        cfg.teams.presence_gate = false;
        cfg.teams.profanity_extra_words = vec!["spam".to_string()];
        cfg.polling.default_interval_seconds = 45;
        cfg.polling.minimum_interval_seconds = 20;
        cfg.polling.max_interval_seconds = 120;
        cfg.polling.expiry_buffer_seconds = 5;
        cfg.polling.pause_backoff_max_seconds = 600;
        cfg.logging.enabled = false;
        cfg.logging.log_level = "Debug".to_string();
        cfg.status_rules.quiet_hours.push(QuietHoursEntry {
            enabled: true,
            start_minutes: 1320,
            end_minutes: 420,
            days: vec![1, 2, 3, 4, 5],
            replacement_status: "Busy".to_string(),
            ..QuietHoursEntry::default()
        });
        cfg.status_rules.track_rules.push(TrackRuleEntry {
            enabled: true,
            artist_substring: "lofi".to_string(),
            track_substring: String::new(),
            replacement_status: "Focus".to_string(),
            presence_availability: "DoNotDisturb".to_string(),
            presence_activity: "Presenting".to_string(),
        });
        cfg.extra
            .insert("future_key".to_string(), serde_json::json!({"a": 1}));
        cfg
    }

    /// Every leaf path (`spotify.client_id`, `status_rules.quiet_hours`, …)
    /// whose value differs between two configs.
    ///
    /// Comparing the whole document this way is what makes "the patch touched
    /// nothing else" exhaustive, instead of a list of fields somebody
    /// remembered to assert.
    fn changed_paths(before: &AppConfig, after: &AppConfig) -> Vec<String> {
        fn join(prefix: &str, key: &str) -> String {
            if prefix.is_empty() {
                key.to_string()
            } else {
                format!("{prefix}.{key}")
            }
        }
        fn walk(a: &serde_json::Value, b: &serde_json::Value, prefix: &str, out: &mut Vec<String>) {
            match (a, b) {
                (serde_json::Value::Object(a_map), serde_json::Value::Object(b_map)) => {
                    for (key, a_value) in a_map {
                        let path = join(prefix, key);
                        match b_map.get(key) {
                            Some(b_value) => walk(a_value, b_value, &path, out),
                            None => out.push(path),
                        }
                    }
                    for key in b_map.keys() {
                        if !a_map.contains_key(key) {
                            out.push(join(prefix, key));
                        }
                    }
                }
                // A list is a leaf: a patch either replaces it or does not
                // name it, there is no per-entry addressing.
                _ => {
                    if a != b {
                        out.push(prefix.to_string());
                    }
                }
            }
        }
        let mut out = Vec::new();
        walk(
            &serde_json::to_value(before).expect("must serialize"),
            &serde_json::to_value(after).expect("must serialize"),
            "",
            &mut out,
        );
        out.sort();
        out
    }

    /// The #531 failure mode in one test: a patch that carries only
    /// `teams.status_format` must leave the stored quiet hours, track rules,
    /// `start_minimized` and logging exactly as they were.
    ///
    /// Asserted through the same `clamped_config` + `stamp_schema_version` +
    /// serde path `save_config` uses — writing the real config.json from a
    /// unit test is not acceptable, and the merge itself is pure.
    #[test]
    fn test_apply_patch_preserves_unpatched_fields() {
        let base = non_default_config();

        let patch: ConfigPatch =
            serde_json::from_str(r#"{"teams": {"status_format": "NEW {track}"}}"#)
                .expect("patch must parse");
        assert!(patch.spotify.is_none());
        assert!(patch.autostart.is_none());

        let mut merged = base.clone();
        apply_patch(&mut merged, &patch);

        // The one patched field took the new value, and it is the ONLY leaf
        // in the whole document that moved.
        assert_eq!(merged.teams.status_format, "NEW {track}");
        assert_eq!(
            changed_paths(&base, &merged),
            vec!["teams.status_format".to_string()],
            "a patch naming one field must not move any other"
        );

        // The losses issue #531 reports, named explicitly.
        assert!(merged.teams.start_minimized);
        assert!(!merged.teams.profanity_filter);
        assert_eq!(merged.teams.profanity_extra_words, vec!["spam".to_string()]);
        assert_eq!(merged.teams.profanity_placeholder, "Custom placeholder");
        assert!(!merged.teams.clear_on_pause);
        assert!(merged.teams.availability_sync);
        assert!(!merged.teams.presence_gate);
        assert_eq!(merged.logging.log_level, "Debug");
        assert!(!merged.logging.enabled);
        assert!(merged.autostart);
        assert_eq!(merged.spotify.client_id, "stored-client-id");
        assert_eq!(merged.spotify.redirect_uri, "presencejam://stored");
        assert_eq!(merged.polling.default_interval_seconds, 45);
        assert_eq!(merged.polling.minimum_interval_seconds, 20);
        assert_eq!(merged.polling.max_interval_seconds, 120);
        assert_eq!(merged.polling.expiry_buffer_seconds, 5);
        assert_eq!(merged.polling.pause_backoff_max_seconds, 600);
        assert_eq!(merged.status_rules.quiet_hours.len(), 1);
        assert_eq!(
            merged.status_rules.quiet_hours[0].replacement_status,
            "Busy"
        );
        assert_eq!(merged.status_rules.track_rules.len(), 1);
        assert_eq!(
            merged.status_rules.track_rules[0].replacement_status,
            "Focus"
        );

        // What actually lands on disk round-trips with the same values.
        let mut persisted = clamped_config(&merged);
        stamp_schema_version(&mut persisted);
        let json = serde_json::to_string_pretty(&persisted).expect("must serialize");
        let back: AppConfig = serde_json::from_str(&json).expect("must re-parse");
        assert_eq!(back.status_rules.quiet_hours.len(), 1);
        assert_eq!(back.status_rules.track_rules.len(), 1);
        assert!(back.teams.start_minimized);
        assert!(!back.teams.profanity_filter);
        assert_eq!(back.teams.status_format, "NEW {track}");
        assert_eq!(back.schema_version, SCHEMA_VERSION);
    }

    /// Naming ONE field inside a section must leave that section's other
    /// fields alone — the case a whole-struct `Option<TeamsConfig>` patch
    /// shape gets wrong, because a partial JSON object deserializes the
    /// omitted fields to their defaults and the section is then assigned
    /// wholesale.
    ///
    /// Table-driven over every patchable field (plus "named a section but no
    /// field", which must be a no-op), asserting the exact set of leaves that
    /// moved.
    #[test]
    fn test_apply_patch_merges_each_section_field_by_field() {
        let cases: &[(&str, &[&str])] = &[
            (
                r#"{"spotify": {"client_id": "new"}}"#,
                &["spotify.client_id"],
            ),
            (
                r#"{"spotify": {"redirect_uri": "presencejam://new"}}"#,
                &["spotify.redirect_uri"],
            ),
            (
                r#"{"teams": {"status_format": "new"}}"#,
                &["teams.status_format"],
            ),
            (
                r#"{"teams": {"clear_on_pause": true}}"#,
                &["teams.clear_on_pause"],
            ),
            (
                r#"{"teams": {"profanity_filter": true}}"#,
                &["teams.profanity_filter"],
            ),
            (
                r#"{"teams": {"profanity_placeholder": "new"}}"#,
                &["teams.profanity_placeholder"],
            ),
            (
                r#"{"teams": {"start_minimized": false}}"#,
                &["teams.start_minimized"],
            ),
            (
                r#"{"teams": {"availability_sync": false}}"#,
                &["teams.availability_sync"],
            ),
            (
                r#"{"teams": {"presence_gate": true}}"#,
                &["teams.presence_gate"],
            ),
            (
                r#"{"teams": {"profanity_extra_words": ["a"]}}"#,
                &["teams.profanity_extra_words"],
            ),
            (
                r#"{"polling": {"default_interval_seconds": 100}}"#,
                &["polling.default_interval_seconds"],
            ),
            (
                r#"{"polling": {"minimum_interval_seconds": 25}}"#,
                &["polling.minimum_interval_seconds"],
            ),
            (
                r#"{"polling": {"max_interval_seconds": 200}}"#,
                &["polling.max_interval_seconds"],
            ),
            (
                r#"{"polling": {"expiry_buffer_seconds": 30}}"#,
                &["polling.expiry_buffer_seconds"],
            ),
            (
                r#"{"polling": {"pause_backoff_max_seconds": 1200}}"#,
                &["polling.pause_backoff_max_seconds"],
            ),
            (r#"{"logging": {"enabled": true}}"#, &["logging.enabled"]),
            (
                r#"{"logging": {"log_level": "Trace"}}"#,
                &["logging.log_level"],
            ),
            (
                r#"{"status_rules": {"quiet_hours": []}}"#,
                &["status_rules.quiet_hours"],
            ),
            (
                r#"{"status_rules": {"track_rules": []}}"#,
                &["status_rules.track_rules"],
            ),
            (r#"{"autostart": false}"#, &["autostart"]),
            // Naming a section without naming a field is a no-op.
            (r#"{}"#, &[]),
            (r#"{"spotify": {}}"#, &[]),
            (r#"{"teams": {}}"#, &[]),
            (r#"{"polling": {}}"#, &[]),
            (r#"{"logging": {}}"#, &[]),
            (r#"{"status_rules": {}}"#, &[]),
        ];

        for (json, expected) in cases {
            let base = non_default_config();
            let patch: ConfigPatch = serde_json::from_str(json)
                .unwrap_or_else(|e| panic!("patch {json} must parse: {e}"));
            let mut merged = base.clone();
            apply_patch(&mut merged, &patch);
            let expected: Vec<String> = expected.iter().map(|path| (*path).to_string()).collect();
            assert_eq!(
                changed_paths(&base, &merged),
                expected,
                "patch {json} moved the wrong set of fields"
            );
        }
    }

    /// A named list is replaced wholesale: the patch value is what lands, not
    /// a merge with the stored list, and the sibling list is untouched.
    #[test]
    fn test_apply_patch_replaces_a_named_list() {
        let mut cfg = non_default_config();
        let patch: ConfigPatch = serde_json::from_str(
            r#"{"status_rules": {"quiet_hours": [{"enabled": false, "replacement_status": "Away"}]}}"#,
        )
        .expect("must parse");

        apply_patch(&mut cfg, &patch);

        assert_eq!(cfg.status_rules.quiet_hours.len(), 1);
        assert!(!cfg.status_rules.quiet_hours[0].enabled);
        assert_eq!(cfg.status_rules.quiet_hours[0].replacement_status, "Away");
        assert_eq!(cfg.status_rules.track_rules.len(), 1);
    }

    /// A patch that names nothing must change nothing (`update_config`'s
    /// base-config path relies on this).
    #[test]
    fn test_apply_patch_is_identity_when_empty() {
        let base = non_default_config();
        let mut merged = base.clone();
        apply_patch(&mut merged, &ConfigPatch::default());
        assert!(
            changed_paths(&base, &merged).is_empty(),
            "an empty patch must be an identity"
        );
    }

    /// `extra` (the #379 forward-compat bucket) is never touched by a patch.
    #[test]
    fn test_apply_patch_leaves_extra_untouched() {
        let base = non_default_config();
        let mut merged = base.clone();
        let patch: ConfigPatch =
            serde_json::from_str(r#"{"autostart": false}"#).expect("must parse");
        apply_patch(&mut merged, &patch);
        assert!(!merged.autostart);
        assert_eq!(
            merged.extra.get("future_key"),
            Some(&serde_json::json!({"a": 1}))
        );
        assert_eq!(base.extra.get("future_key"), merged.extra.get("future_key"));
    }

    // ---------------------------------------------------------------
    // CfgDiag#1 (#536): schema_version is the binary's to set.
    // ---------------------------------------------------------------

    /// A client payload stuck on the pre-4.6 version must not lower the
    /// version that is actually persisted.
    #[test]
    fn test_stamp_schema_version_overrides_stale_client_value() {
        let mut cfg: AppConfig =
            serde_json::from_str(r#"{"schema_version": 1}"#).expect("must parse");
        assert_eq!(cfg.schema_version, 1);
        stamp_schema_version(&mut cfg);
        assert_eq!(cfg.schema_version, SCHEMA_VERSION);
        const { assert!(SCHEMA_VERSION > 1, "4.6 must have bumped the schema") };
    }

    /// The dispatcher raises an old (or absent → v1) file to the current
    /// version and never relabels a file written by a newer binary.
    #[test]
    fn test_migrate_config_raises_old_and_preserves_newer() {
        let mut old = AppConfig {
            schema_version: 1,
            ..AppConfig::default()
        };
        let from = old.schema_version;
        migrate_config(&mut old, from);
        assert_eq!(old.schema_version, SCHEMA_VERSION);

        let mut newer = AppConfig {
            schema_version: 99,
            ..AppConfig::default()
        };
        let from = newer.schema_version;
        migrate_config(&mut newer, from);
        assert_eq!(
            newer.schema_version, 99,
            "a newer file must not be relabelled downward"
        );

        // v0 (a hand-edited or truncated key) is treated as old, not newer.
        let mut zero = AppConfig {
            schema_version: 0,
            ..AppConfig::default()
        };
        let from = zero.schema_version;
        migrate_config(&mut zero, from);
        assert_eq!(zero.schema_version, SCHEMA_VERSION);
    }

    // ---------------------------------------------------------------
    // CfgDiag#4 (#539): logging changes take effect without a restart.
    // ---------------------------------------------------------------

    #[test]
    fn test_apply_log_level_maps_enabled_and_level() {
        // `log::set_max_level` is process-global: serialize this test against
        // itself so a parallel sibling cannot observe the transient value.
        static LOG_LEVEL_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());
        let _guard = LOG_LEVEL_LOCK.lock();

        apply_log_level(&LoggingConfig {
            enabled: false,
            log_level: "Debug".to_string(),
        });
        assert_eq!(
            log::max_level(),
            log::LevelFilter::Off,
            "logging.enabled = false must silence the logger immediately"
        );

        apply_log_level(&LoggingConfig {
            enabled: true,
            log_level: "DEBUG".to_string(),
        });
        assert_eq!(log::max_level(), log::LevelFilter::Debug);

        apply_log_level(&LoggingConfig {
            enabled: true,
            log_level: "not-a-level".to_string(),
        });
        assert_eq!(
            log::max_level(),
            log::LevelFilter::Info,
            "an unrecognised level must never disable logging"
        );

        // Restore the level the rest of the suite runs under.
        apply_log_level(&LoggingConfig::default());
        assert_eq!(log::max_level(), log::LevelFilter::Info);
    }

    // ---------------------------------------------------------------
    // CfgDiag#3 (#538): the three additive 4.6 config fields.
    // ---------------------------------------------------------------

    /// Pre-4.6 files keep loading: every new field carries a serde default.
    #[test]
    fn test_four_six_additions_default_on_old_files() {
        let cfg: AppConfig = serde_json::from_str(
            r#"{"teams": {}, "polling": {}, "status_rules": {"quiet_hours": [{"enabled": true}]}}"#,
        )
        .expect("a pre-4.6 document must still parse");
        assert!(cfg.teams.profanity_extra_words.is_empty());
        assert_eq!(cfg.polling.pause_backoff_max_seconds, 300);
        assert!(cfg.status_rules.quiet_hours[0]
            .replacement_status
            .is_empty());
    }

    /// The new fields round-trip through serde.
    #[test]
    fn test_four_six_additions_round_trip() {
        let cfg: AppConfig = serde_json::from_str(
            r#"{
                "teams": {"profanity_extra_words": ["spam", "spammer"]},
                "polling": {"pause_backoff_max_seconds": 120},
                "status_rules": {"quiet_hours": [{"enabled": true, "replacement_status": "Busy"}]}
            }"#,
        )
        .expect("must parse");
        assert_eq!(cfg.teams.profanity_extra_words.len(), 2);
        assert_eq!(cfg.polling.pause_backoff_max_seconds, 120);
        assert_eq!(cfg.status_rules.quiet_hours[0].replacement_status, "Busy");

        let json = serde_json::to_string(&cfg).expect("must serialize");
        let back: AppConfig = serde_json::from_str(&json).expect("must re-parse");
        assert_eq!(
            back.teams.profanity_extra_words,
            cfg.teams.profanity_extra_words
        );
        assert_eq!(back.polling.pause_backoff_max_seconds, 120);
        assert_eq!(back.status_rules.quiet_hours[0].replacement_status, "Busy");
    }

    /// An oversized user lexicon is bounded, not rejected: the filter must
    /// never take an unbounded amount of work from a hand-edited file.
    #[test]
    fn test_clamp_teams_bounds_extra_words() {
        let mut teams = TeamsConfig {
            profanity_extra_words: (0..100)
                .map(|i| {
                    if i == 0 {
                        "x".repeat(64)
                    } else {
                        format!("w{i}")
                    }
                })
                .collect(),
            ..TeamsConfig::default()
        };
        clamp_teams(&mut teams);
        assert_eq!(teams.profanity_extra_words.len(), 64);
        assert_eq!(teams.profanity_extra_words[0].chars().count(), 32);

        // Multi-byte truncation must stay on a char boundary.
        let mut teams = TeamsConfig {
            profanity_extra_words: vec!["ü".repeat(40)],
            ..TeamsConfig::default()
        };
        clamp_teams(&mut teams);
        assert_eq!(teams.profanity_extra_words[0].chars().count(), 32);
    }

    /// `clamped_config` is applied on load AND on save, so the bounded
    /// lexicon survives both directions.
    #[test]
    fn test_clamped_config_bounds_lexicon_on_save() {
        let mut cfg = AppConfig::default();
        cfg.teams.profanity_extra_words = vec!["y".repeat(50)];
        let json = serde_json::to_string_pretty(&clamped_config(&cfg)).expect("must serialize");
        let back: AppConfig = serde_json::from_str(&json).expect("must re-parse");
        assert_eq!(back.teams.profanity_extra_words[0].chars().count(), 32);
    }

    // ---------------------------------------------------------------
    // Presence findings #634 / #635 / #637: the rule presence model, the
    // manual-status policy flag and the out-of-office gate flag.
    // ---------------------------------------------------------------

    /// Every new field is additive: a 4.5 file loads with the documented
    /// defaults (presence-untouched rules, manual status respected, OOO gating
    /// off).
    #[test]
    fn test_presence_additions_default_on_old_files() {
        let cfg: AppConfig = serde_json::from_str(
            r#"{"teams": {}, "status_rules": {"quiet_hours": [{"enabled": true}], "track_rules": [{"enabled": true}]}}"#,
        )
        .expect("a 4.5 document must still parse");
        assert!(cfg.teams.respect_manual_status);
        assert!(!cfg.teams.gate_when_out_of_office);
        assert!(cfg.status_rules.quiet_hours[0]
            .presence_availability
            .is_empty());
        assert!(cfg.status_rules.quiet_hours[0].presence_activity.is_empty());
        assert!(cfg.status_rules.track_rules[0]
            .presence_availability
            .is_empty());
        assert!(cfg.status_rules.track_rules[0].presence_activity.is_empty());
    }

    /// The canonical pair set is exactly the five documented combinations:
    /// case-insensitive on input, canonical on output, and nothing else.
    #[test]
    fn test_normalize_presence_pair_accepts_only_documented_combinations() {
        assert_eq!(
            normalize_presence_pair("Available", "Available"),
            Some(PresencePair {
                availability: "Available".to_string(),
                activity: "Available".to_string(),
            })
        );
        // Case + whitespace from a hand-edited config normalize to canonical.
        assert_eq!(
            normalize_presence_pair(" doNotDisturb ", "presenting")
                .expect("case-insensitive match")
                .activity,
            "Presenting"
        );
        assert_eq!(
            normalize_presence_pair("Busy", "InAConferenceCall")
                .expect("documented combination")
                .availability,
            "Busy"
        );
        // Unsupported / half-filled pairs are rejected outright — including
        // the two the docs say setPresence does not honour.
        assert_eq!(
            normalize_presence_pair("DoNotDisturb", "DoNotDisturb"),
            None
        );
        assert_eq!(normalize_presence_pair("OutOfOffice", "InAMeeting"), None);
        assert_eq!(normalize_presence_pair("Available", ""), None);
        assert_eq!(normalize_presence_pair("", ""), None);
        assert_eq!(normalize_presence_pair("totally-made-up", "x"), None);
    }

    /// `clamp_rules` is the IPC-boundary normalizer: an unsupported pair is
    /// cleared (not rejected), and an over-long replacement text is truncated
    /// on a char boundary.
    #[test]
    fn test_clamp_rules_normalizes_pairs_and_bounds_text() {
        let mut rules = StatusRulesConfig {
            quiet_hours: vec![QuietHoursEntry {
                enabled: true,
                presence_availability: "donotdisturb".to_string(),
                presence_activity: "presenting".to_string(),
                replacement_status: "ü".repeat(200),
                ..QuietHoursEntry::default()
            }],
            track_rules: vec![TrackRuleEntry {
                enabled: true,
                presence_availability: "DoNotDisturb".to_string(),
                presence_activity: "DoNotDisturb".to_string(),
                replacement_status: "Focus".to_string(),
                ..TrackRuleEntry::default()
            }],
        };
        clamp_rules(&mut rules);
        assert_eq!(rules.quiet_hours[0].presence_availability, "DoNotDisturb");
        assert_eq!(rules.quiet_hours[0].presence_activity, "Presenting");
        assert_eq!(
            rules.quiet_hours[0].replacement_status.chars().count(),
            MAX_RULE_STATUS_CHARS
        );
        // The unsupported pair is cleared on both sides, so the rule means
        // "don't touch presence" instead of sending a pair Graph drops.
        assert!(rules.track_rules[0].presence_availability.is_empty());
        assert!(rules.track_rules[0].presence_activity.is_empty());
        // A supported, already-canonical pair and a short text are untouched.
        assert_eq!(rules.track_rules[0].replacement_status, "Focus");
    }

    /// The rule model is normalized on the SAVE path too (`clamped_config` is
    /// what `save_config` persists and what `AppState` stores).
    #[test]
    fn test_clamped_config_normalizes_rules_on_save() {
        let mut cfg = AppConfig::default();
        cfg.status_rules.track_rules.push(TrackRuleEntry {
            enabled: true,
            presence_availability: "Busy".to_string(),
            presence_activity: "InACall".to_string(),
            ..TrackRuleEntry::default()
        });
        cfg.status_rules.quiet_hours.push(QuietHoursEntry {
            enabled: true,
            presence_availability: "Away".to_string(),
            presence_activity: "Away".to_string(),
            replacement_status: "z".repeat(500),
            ..QuietHoursEntry::default()
        });

        let json = serde_json::to_string_pretty(&clamped_config(&cfg)).expect("must serialize");
        let back: AppConfig = serde_json::from_str(&json).expect("must re-parse");
        assert_eq!(
            back.status_rules.track_rules[0].presence_activity,
            "InACall"
        );
        assert_eq!(back.status_rules.quiet_hours[0].presence_activity, "Away");
        assert_eq!(
            back.status_rules.quiet_hours[0]
                .replacement_status
                .chars()
                .count(),
            MAX_RULE_STATUS_CHARS
        );
    }

    /// Round trip for the new Teams + rule fields (finding #634/#635/#637).
    #[test]
    fn test_presence_additions_round_trip() {
        let cfg: AppConfig = serde_json::from_str(
            r#"{
                "teams": {"respect_manual_status": false, "gate_when_out_of_office": true},
                "status_rules": {"track_rules": [{
                    "enabled": true,
                    "presence_availability": "Away",
                    "presence_activity": "Away"
                }]}
            }"#,
        )
        .expect("must parse");
        assert!(!cfg.teams.respect_manual_status);
        assert!(cfg.teams.gate_when_out_of_office);
        assert_eq!(
            cfg.status_rules.track_rules[0].presence_availability,
            "Away"
        );

        let back: AppConfig =
            serde_json::from_str(&serde_json::to_string(&cfg).expect("must serialize"))
                .expect("must re-parse");
        assert!(!back.teams.respect_manual_status);
        assert!(back.teams.gate_when_out_of_office);
        assert_eq!(back.status_rules.track_rules[0].presence_activity, "Away");
    }

    // ---------------------------------------------------------------
    // CfgDiag#2 (#537): the quarantine backup is name-only.
    // ---------------------------------------------------------------

    /// The diagnostics snapshot must be able to say "config.json.bak"
    /// without leaking the user's home directory (#409).
    #[test]
    fn test_quarantine_backup_name_is_a_bare_file_name() {
        let dir = std::env::temp_dir().join(format!(
            "pj-test-bakname-{}-{}",
            std::process::id(),
            chrono_like_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        assert_eq!(
            quarantine_backup_name_for(&path),
            None,
            "no backup on disk means no name to report"
        );

        std::fs::write(quarantine_backup_path(&path), b"OLD").unwrap();
        let name = quarantine_backup_name_for(&path).expect("backup must be reported");
        assert_eq!(name, "config.json.bak");
        assert!(
            !name.contains('/') && !name.contains('\\'),
            "the reported name must carry no directory component, got {name}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---------------------------------------------------------------
    // IssueTriage#3 (#478): the config.rs log-tag guard.
    // ---------------------------------------------------------------

    /// Extract the argument region of the macro call whose opening `(` is
    /// at `open`, i.e. everything up to the matching `)`. String literals,
    /// char literals and `//` comments are skipped, so a `)` inside a format
    /// string cannot end the region early.
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

    /// IssueTriage#2/#3 (#393/#478): every log macro in this file must carry
    /// the `[CFG]` module tag.
    ///
    /// Macro-aware on purpose: a per-line scan is red on green code, because
    /// ten of the call sites put the format string on the line AFTER the
    /// `log::warn!` opener. Whole argument regions are extracted instead.
    ///
    /// The needles are assembled with `concat!` so this test's own source
    /// never contains the literal it searches for — otherwise an
    /// `include_str!` scan would match the test itself and pass vacuously.
    #[test]
    fn test_config_log_tags_use_cfg_prefix() {
        let src = include_str!("config.rs");
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
                    region.contains("[CFG]"),
                    "config.rs log at byte {i} lacks the [CFG] tag: {}",
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
}
