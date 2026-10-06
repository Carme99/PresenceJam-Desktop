use super::clamp::{normalize_rule_days, QUIET_HOURS_DAY_MINUTES, TRACK_RULE_DAY_MINUTES};
use super::migrate::default_schema_version;
use crate::profanity;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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
    /// Spotify app client id. `#[serde(default)]` since issue #926: this was
    /// the only persisted field without one, so a spotify section that omitted
    /// it — or spelled it `null` — failed the whole document, which
    /// `load_config` answered by quarantining the file and booting on
    /// defaults, costing the user every other setting too.
    #[serde(default)]
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
    /// Unknown / future keys NESTED inside this section, retained across
    /// load→save so a section written by a newer binary is not silently
    /// stripped by an older one (issue #938 — the section-level companion of
    /// [`AppConfig::extra`]). Omitted from JSON while empty and skipped in the
    /// TypeScript export, so an untouched config gains no bytes and the
    /// generated TypeScript is unchanged.
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(skip)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

pub(crate) fn default_redirect_uri() -> String {
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
    /// Issue #872: also gate the status write while the OS reports a
    /// full-screen app, presentation mode, or Quiet Time. OFF by default
    /// — a hand-edited config flips it on; the GUI does too. Linux/macOS
    /// always report `Unknown` (`platform::focus`), so the toggle is a
    /// no-op on those targets. Fails open on a Windows probe error so a
    /// transient shell-API failure cannot lock the gate.
    #[serde(default = "default_gate_when_presenting")]
    pub gate_when_presenting: bool,
    /// Issue #873: stop advertising listening once the OS reports no
    /// keyboard/mouse input for this many seconds. `0` (the default)
    /// disables the feature — 4.7 behaviour is unchanged until the user
    /// opts in. Clamped to 60..=3600 by `clamp_teams` so a hand-edited
    /// config cannot put the gate in a state that surprises the user
    /// (a 1 s threshold would fire on every typing pause). Linux/macOS
    /// always report `None` (`platform::idle`), so the toggle is a no-op
    /// on those targets.
    #[serde(default)]
    #[ts(type = "number")]
    pub idle_away_after_seconds: u64,
    /// Issue #867: minutes before a busy Outlook calendar event starts that
    /// the status write is suppressed. `0` means suppress only during the
    /// meeting itself (the same behaviour as the presence-gate today);
    /// `>0` lets a user pre-gate so a track that started ten minutes before
    /// the meeting is also caught. Capped at 60 minutes by `clamp_teams`.
    #[serde(default)]
    pub pre_meeting_suppress_minutes: u16,
    /// S4 (issue #672): the text posted as the Teams status message while
    /// playback is paused — the user-templatable form of the literal the
    /// paused clear used to hardcode (`"🎵 Paused"`, emoji included by
    /// `poll_once`). Defaults to that literal's text, so an existing config
    /// renders byte-identically.
    #[serde(default = "default_paused_status_format")]
    pub paused_status_format: String,
    /// S4 (issue #672): the text posted when nothing is playing — the
    /// user-templatable form of the no-track clear's hardcoded
    /// `"🎵 Nothing playing on Spotify"`. Defaults to that literal's text.
    #[serde(default = "default_stopped_status_format")]
    pub stopped_status_format: String,
    /// Issue #866: a long-lived "preferred presence" the app sets on the user's
    /// behalf via Graph `setUserPreferredPresence`, applying the documented
    /// Busy / DND / BeRightBack / Away pairs while a rule or snooze wants
    /// presence moved. The user's own Teams bubble wins — `respect_manual_status`
    /// suppresses the call — and the user can clear it from the Settings pane
    /// or by quitting the app (the `RunEvent::Exit` arm invokes the Graph
    /// `clearUserPreferredPresence` counterpart).
    ///
    /// National-cloud note: `setUserPreferredPresence` is a commercial-Graph
    /// surface. The free `graph.microsoft.com` endpoint used by `setPresence`
    /// is the same on every cloud, but sovereign clouds (US Gov / DoD, China,
    /// Germany) have historically rejected preferred-presence POSTs. The app
    /// always prefers `setUserPreferredPresence` when enabled, and logs a
    /// one-shot warning the first time the endpoint answers with the
    /// documented 4xx shape.
    #[serde(default)]
    pub preferred_presence: PreferredPresenceConfig,
    /// Unknown / future keys NESTED inside this section, retained across
    /// load→save so a section written by a newer binary is not silently
    /// stripped by an older one (issue #938 — the section-level companion of
    /// [`AppConfig::extra`]). Omitted from JSON while empty and skipped in the
    /// TypeScript export, so an untouched config gains no bytes and the
    /// generated TypeScript is unchanged.
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(skip)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

pub(crate) fn default_status_format() -> String {
    "🎵 {artist} - {track} 🎧".to_string()
}

pub(crate) fn default_start_minimized() -> bool {
    false
}

pub(crate) fn default_clear_on_pause() -> bool {
    true
}

pub(crate) fn default_profanity_filter() -> bool {
    true
}

pub(crate) fn default_profanity_placeholder() -> String {
    profanity::safe_placeholder_default().to_string()
}

pub(crate) fn default_availability_sync() -> bool {
    false
}

pub(crate) fn default_presence_gate() -> bool {
    true
}

pub(crate) fn default_respect_manual_status() -> bool {
    true
}

pub(crate) fn default_gate_when_out_of_office() -> bool {
    false
}

pub(crate) fn default_gate_when_presenting() -> bool {
    false
}

pub(crate) fn default_paused_status_format() -> String {
    "Paused".to_string()
}

pub(crate) fn default_stopped_status_format() -> String {
    "Nothing playing on Spotify".to_string()
}

/// Issue #866: the user-configurable "preferred presence" the app drives on
/// the user's behalf via Graph `setUserPreferredPresence`. Distinct from the
/// ephemeral [`Self::availability_sync`] `setPresence` session — preferred
/// presence is the documented Busy / DND / BeRightBack / Away vehicle and
/// survives across processes the user did not start themselves.
///
/// `expiry_minutes` is bound by [`clamp_preferred_presence`] into
/// `5..=720`. The pair is bound by the same [`normalize_presence_pair`] the
/// rules use — a hand-edited file that names a pair Graph silently drops is
/// normalized away at the IPC boundary exactly like the rule pairs.
#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct PreferredPresenceConfig {
    /// OFF by default — preferred presence is opt-in, mirroring how
    /// `availability_sync` shipped (it overrides the user's manual bubble).
    #[serde(default)]
    pub enabled: bool,
    /// Graph availability token (`Busy`, `DoNotDisturb`, `BeRightBack`,
    /// `Away`). Normalized through [`normalize_presence_pair`] on load and on
    /// every save; an unsupported value clears the pair and disables the
    /// feature (the call would never land anyway).
    #[serde(default)]
    pub availability: String,
    /// Graph activity token (`Busy`, `DoNotDisturb`, `Away`, `BeRightBack`,
    /// or — for `Busy` — `InACall`/`InAConferenceCall`/`Presenting`). Same
    /// normalizer as `availability`.
    #[serde(default)]
    pub activity: String,
    /// How long the preferred presence survives a successful
    /// `setUserPreferredPresence` before the app clears it at expiry (the
    /// same expiry the rule+snooze path observed, and the same `RunEvent::Exit`
    /// arm clears on quit). Default: 60 minutes.
    #[serde(default = "default_preferred_presence_expiry")]
    pub expiry_minutes: u32,
    /// Unknown / future keys NESTED inside this section, retained across
    /// load→save so a section written by a newer binary is not silently
    /// stripped by an older one (issue #938 — this object is a SECTION in its
    /// own right, reachable at `teams.preferred_presence`, and it was the one
    /// nested object still missing the retention map its siblings all carry).
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(skip)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

pub(crate) fn default_preferred_presence_expiry() -> u32 {
    60
}

impl Default for PreferredPresenceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            availability: String::new(),
            activity: String::new(),
            expiry_minutes: default_preferred_presence_expiry(),
            extra: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct PollingConfig {
    // Issue #765: Tauri IPC crosses the boundary via serde_json, which decodes
    // `u64` values as JS `number` (f64). Override ts-rs's `bigint` default so
    // the generated `.ts` matches what `invoke()` actually returns at
    // runtime — `bigint` would type-lie about the wire shape. All five values
    // are small (seconds, clamped to <= 3600), well under 2^53.
    #[serde(default = "default_interval_seconds")]
    #[ts(type = "number")]
    pub default_interval_seconds: u64,
    #[serde(default = "default_min_interval_seconds")]
    #[ts(type = "number")]
    pub minimum_interval_seconds: u64,
    #[serde(default = "default_max_interval_seconds")]
    #[ts(type = "number")]
    pub max_interval_seconds: u64,
    #[serde(default = "default_expiry_buffer_seconds")]
    #[ts(type = "number")]
    pub expiry_buffer_seconds: u64,
    /// Ceiling for the "paused playback" exponential backoff (CfgDiag#3(c),
    /// issue #538). `pause_backoff` used to hardcode a 300 s cap; it is now
    /// the ladder's ceiling (default 300, so an untouched config is unchanged)
    /// and the value is clamped into 60..=3600 by `clamp_polling`.
    #[serde(default = "default_pause_backoff_max")]
    #[ts(type = "number")]
    pub pause_backoff_max_seconds: u64,
    /// Unknown / future keys NESTED inside this section, retained across
    /// load→save so a section written by a newer binary is not silently
    /// stripped by an older one (issue #938 — the section-level companion of
    /// [`AppConfig::extra`]). Omitted from JSON while empty and skipped in the
    /// TypeScript export, so an untouched config gains no bytes and the
    /// generated TypeScript is unchanged.
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(skip)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

pub(crate) fn default_interval_seconds() -> u64 {
    30
}

pub(crate) fn default_min_interval_seconds() -> u64 {
    10
}

pub(crate) fn default_max_interval_seconds() -> u64 {
    60
}

pub(crate) fn default_expiry_buffer_seconds() -> u64 {
    10
}

pub(crate) fn default_pause_backoff_max() -> u64 {
    300
}
/// Normalize a quiet-hours window in place (issue #821), mirroring
/// [`clamp_track_rule_window`]: the minutes into the range [`QuietHoursEntry`]
/// documents, and `days` through [`normalize_rule_days`].
///
/// Without this, an out-of-range weekday loaded unchanged and matched NO weekday
/// at all, so a hand-edited or other-build `days: [0]` window — its
/// `pause_polling` arm included — silently never fired, with no error anywhere.
/// The Settings day picker only ever writes `1..=7`, so the trigger is exactly
/// the hand-edited/foreign document this load-time normalizer exists for.
///
/// The window itself keeps [`QuietHoursEntry`]'s semantics: `[start, end)` with
/// a wrap-around pair (`start > end`, e.g. 22:00→07:00) honoured, and
/// `start == end` matching nothing.
pub(crate) fn clamp_quiet_hours_window(entry: &mut QuietHoursEntry) {
    // Same reasoning as the track rule above: a START of 1440 is unreachable
    // (`now` never exceeds 1439), while an END of 1440 is the end of the day.
    entry.start_minutes = entry.start_minutes.min(QUIET_HOURS_DAY_MINUTES - 1);
    entry.end_minutes = entry.end_minutes.min(QUIET_HOURS_DAY_MINUTES);
    normalize_rule_days(&mut entry.days);
}

#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct LoggingConfig {
    #[serde(default = "default_logging_enabled")]
    pub enabled: bool,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    /// Rotation ceiling for the log file, in mebibytes (4.7.0). The
    /// rotating file target renames the active log and starts a fresh one
    /// once it would exceed this size. Clamped to 1..=500 by
    /// [`clamp_logging`].
    #[serde(default = "default_max_file_size_mb")]
    #[ts(type = "number")]
    pub max_file_size_mb: u64,
    /// How many *archived* log files to retain (4.7.0). The active
    /// `PresenceJam.log` is not counted, so the directory holds at most
    /// `keep_files + 1` log files. Clamped to 1..=20 by [`clamp_logging`].
    #[serde(default = "default_keep_files")]
    pub keep_files: u32,
    /// Issue #877: opt-in JSONL mirror of the bounded status-decision
    /// history. OFF by default — a noisy rule set could otherwise grow
    /// the log without bound — and writes only when the user opts in.
    /// The mirror lives in the same `app_log_dir()` folder
    /// `tauri-plugin-log` already targets; the file is `presence-history.jsonl`
    /// and one line per decision appends.
    #[serde(default)]
    pub presence_history: bool,
    /// Unknown / future keys NESTED inside this section, retained across
    /// load→save so a section written by a newer binary is not silently
    /// stripped by an older one (issue #938 — the section-level companion of
    /// [`AppConfig::extra`]). Omitted from JSON while empty and skipped in the
    /// TypeScript export, so an untouched config gains no bytes and the
    /// generated TypeScript is unchanged.
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(skip)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

pub(crate) fn default_logging_enabled() -> bool {
    true
}

pub(crate) fn default_log_level() -> String {
    "Info".to_string()
}

pub(crate) fn default_max_file_size_mb() -> u64 {
    10
}

pub(crate) fn default_keep_files() -> u32 {
    3
}

/// Bound the log-rotation settings (4.7.0, S5). Mirrors [`clamp_polling`]:
/// the Settings number inputs' `min`/`max` attributes do not constrain a
/// typed value and a hand-edited `config.json` is not policed by anyone
/// else, so this is the only normalizer — it runs on load and on every save.
///
/// `keep_files >= 1` matters beyond taste: the rotating target is built as
/// `KeepSome(keep_files)` (see `lib.rs::log_rotation_strategy`) and the
/// plugin's archive pass computes `keep_count - 1`.
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
/// One quiet-hours entry for issue #432: status writes are suppressed while
/// the local time falls inside `[start_minutes, end_minutes)` (minutes
/// since midnight; wrap-around ranges like 22:00→07:00 are supported).
/// `days` holds ISO weekday numbers 1 (Mon)..=7 (Sun); empty means every
/// day. A midnight-crossing window is NIGHT-OWNING (issue #794): each half
/// is tested against the day it falls on — the evening half (`now >= start`)
/// against `weekday`, the morning half (`now < end`) against the previous
/// ISO day (wrapping 1→7) — so a Monday-only 22:00→07:00 window covers
/// Monday night into Tuesday morning, not Sunday night. All fields
/// `#[serde(default)]` individually so a hand-edited config missing one
/// still loads, and load-time normalization of `days` and the two minutes
/// lives in one place: [`clamp_quiet_hours_window`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct QuietHoursEntry {
    #[serde(default)]
    pub enabled: bool,
    /// Minutes since midnight, normalized into `0..=1439` by
    /// [`clamp_quiet_hours_window`] (a start of 1440 is unreachable — the
    /// clock never reads it).
    #[serde(default)]
    pub start_minutes: u16,
    /// Minutes since midnight, normalized into `0..=1440` by
    /// [`clamp_quiet_hours_window`]; `1440` is the end of the day.
    #[serde(default = "default_quiet_end")]
    pub end_minutes: u16,
    /// ISO weekday numbers 1..=7; empty = every day. Normalized by
    /// [`clamp_quiet_hours_window`] (out-of-range days dropped, then sorted and
    /// deduplicated) exactly like [`TrackRuleEntry::days`].
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
    /// S4 (issue #672): also stop POLLING while this window is active, not
    /// just the status write — no Spotify GET, no Graph work, and no clock
    /// movement for the duration (the polling driver re-evaluates the window
    /// every iteration, so it resumes by itself). OFF by default: 4.6
    /// behaviour is unchanged until the user opts in.
    #[serde(default = "default_pause_polling")]
    pub pause_polling: bool,
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
            pause_polling: default_pause_polling(),
        }
    }
}

pub(crate) fn default_quiet_end() -> u16 {
    420
}

pub(crate) fn default_pause_polling() -> bool {
    false
}

pub(crate) fn default_track_rule_days() -> Vec<u8> {
    Vec::new()
}

pub(crate) fn default_track_rule_start() -> u32 {
    0
}

/// The contract's default end: the end of the day, so the default window
/// covers every minute (0 → 1440).
pub(crate) fn default_track_rule_end() -> u32 {
    TRACK_RULE_DAY_MINUTES
}

/// Issue #868: how `artist_substring` / `track_substring` are compared
/// against the playing track. Substring is the legacy behaviour (case-
/// insensitive `contains`); Exact requires a full case-insensitive
/// equality; Glob treats the two substrings as case-insensitive glob
/// patterns (`*` matches any run, `?` matches one character) evaluated
/// independently. Album / show / device / playlist-uri remain substring
/// matches regardless of `match_kind` — they are extension surfaces, not
/// primary identifiers, and the Settings UI exposes only the substring
/// field for them.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, ts_rs::TS, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub enum TrackRuleMatchKind {
    #[default]
    Substring,
    Exact,
    Glob,
}

/// Issue #868: what happens when a track rule matches. `Suppress` is the
/// documented "write nothing" default; `Replace` posts a fixed status text
/// instead of the track template; `SnoozeMinutes { value }` arms the snooze
/// for `value` minutes; `Profile { id }` switches the active presence
/// profile for the duration of the track; `Presence { availability,
/// activity }` applies a Teams presence pair while the track plays. The
/// legacy `replacement_status` + `presence_availability` /
/// `presence_activity` fields continue to feed the `Replace` / `Presence`
/// variants during the transition — see `explain_rules` for the canonical
/// "what would fire" projection.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ts_rs::TS, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "lowercase")]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub enum TrackRuleAction {
    /// Default: write nothing for this track (suppress the status update).
    #[default]
    Suppress,
    /// Post a fixed status text instead of the track template.
    Replace { status: String },
    /// Snooze sync for `value` minutes (clamped into 1..=1440 by
    /// `clamp_track_rule_action`).
    SnoozeMinutes { value: u32 },
    /// Switch the active presence profile for this track. `id` is the
    /// profile name from `AppConfig::presence_profiles`; a missing id is
    /// treated as `Suppress` by the rule walker.
    Profile { id: String },
    /// Apply a Teams presence pair for the duration of the track. The pair
    /// is normalized against [`PRESENCE_COMBINATIONS`] by `clamp_rules`
    /// exactly like the legacy `presence_availability` /
    /// `presence_activity` fields.
    Presence {
        availability: String,
        activity: String,
    },
}

/// One track-matching rule for issue #432 / issue #868: when the
/// substring conditions AND the album / show / device / playlist-uri
/// extensions AND the duration gate all match, the rule's `action` runs.
/// Issue #868 also adds `negate` so an empty match list still wins when
/// the negation's conditions match (a "suppress on the absence of a
/// device substring" pattern), plus `match_kind` and the new
/// `action` enum. Empty substrings match everything (so a rule with
/// only one field set still works); `min_duration_seconds == 0` skips the
/// duration gate.
#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct TrackRuleEntry {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub artist_substring: String,
    #[serde(default)]
    pub track_substring: String,
    /// Issue #868: how `artist_substring` / `track_substring` are compared.
    /// Defaults to `Substring` (the legacy case-insensitive `contains`).
    #[serde(default)]
    pub match_kind: TrackRuleMatchKind,
    /// Issue #868: substring matched against the track's album title
    /// (empty = match any album).
    #[serde(default)]
    pub album_substring: String,
    /// Issue #868: substring matched against the episode's show name when
    /// the playing item is a podcast episode (empty = match any show or any
    /// track).
    #[serde(default)]
    pub show_substring: String,
    /// Issue #868: substring matched against the active Spotify device's
    /// name (empty = match any device).
    #[serde(default)]
    pub device_substring: String,
    /// Issue #868: substring matched against the playing context URI
    /// (e.g. `spotify:playlist:abc…`). Empty = match any context.
    #[serde(default)]
    pub playlist_uri: String,
    /// Issue #868: minimum track / episode duration, in seconds, for this
    /// rule to match. `0` (the default) disables the gate. Capped at 86 400
    /// (24 h) by `clamp_track_rule_action` so a hand-edited config cannot
    /// put the gate in a permanently-firing state.
    #[serde(default)]
    pub min_duration_seconds: u32,
    /// Issue #868: when `true`, the rule matches the NEGATION of the
    /// combined conditions (the "suppress unless something matches"
    /// pattern). `false` (the default) keeps the legacy "match if the
    /// conditions hold" semantics.
    #[serde(default)]
    pub negate: bool,
    /// Optional fixed status posted instead of suppressing (issue #432
    /// "busy/focus" alternative, retained for the legacy
    /// `TrackRuleAction::Replace { status: … }` projection).
    /// Empty = suppress silently.
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
    /// Issue #868: the rule's effect. Defaults to `Suppress`, the legacy
    /// "write nothing for this track" outcome. `clamp_rules` normalizes
    /// the inner text / value / id fields into their canonical forms.
    #[serde(default)]
    pub action: TrackRuleAction,
    /// S4 (issue #672): ISO weekday numbers 1 (Mon)..=7 (Sun) this rule
    /// applies on; empty = every day — the same shape and semantics
    /// [`QuietHoursEntry::days`] uses, normalized by `clamp_rules` (see
    /// [`clamp_track_rule_window`]).
    #[serde(default = "default_track_rule_days")]
    pub days: Vec<u8>,
    /// S4 (issue #672): start of the rule's local-time window, in minutes since
    /// midnight. The window is `[start_minutes, end_minutes)` with the same
    /// wrap-around rule as [`QuietHoursEntry`] (22:00→07:00 works); the
    /// default pair (`0`, `1440`) covers every minute of the day. A
    /// midnight-crossing window is NIGHT-OWNING (issue #794): the evening
    /// half (`now >= start`) is tested against the selected day, the morning
    /// half (`now < end`) against the previous ISO day (wrapping 1→7) — a
    /// Monday-only 22:00→07:00 rule covers Monday night into Tuesday
    /// morning.
    #[serde(default = "default_track_rule_start")]
    pub start_minutes: u32,
    /// S4 (issue #672): end of the rule's local-time window, in minutes since
    /// midnight; `1440` is the end of the day.
    #[serde(default = "default_track_rule_end")]
    pub end_minutes: u32,
}

/// Mirrors the serde defaults field-by-field — see [`QuietHoursEntry`].
impl Default for TrackRuleEntry {
    fn default() -> Self {
        Self {
            enabled: false,
            artist_substring: String::new(),
            track_substring: String::new(),
            match_kind: TrackRuleMatchKind::default(),
            album_substring: String::new(),
            show_substring: String::new(),
            device_substring: String::new(),
            playlist_uri: String::new(),
            min_duration_seconds: 0,
            negate: false,
            replacement_status: String::new(),
            presence_availability: String::new(),
            presence_activity: String::new(),
            action: TrackRuleAction::default(),
            days: default_track_rule_days(),
            start_minutes: default_track_rule_start(),
            end_minutes: default_track_rule_end(),
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
    /// Unknown / future keys NESTED inside this section, retained across
    /// load→save so a section written by a newer binary is not silently
    /// stripped by an older one (issue #938 — the section-level companion of
    /// [`AppConfig::extra`]). Omitted from JSON while empty and skipped in the
    /// TypeScript export, so an untouched config gains no bytes and the
    /// generated TypeScript is unchanged.
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(skip)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// Which desktop-notification classes the app may show (4.7.0 / issue #675).
///
/// Replaces the single `notificationsEnabled` localStorage opt-in, which only
/// ever governed track changes, with one toggle per class. All four are ON by
/// default (matching the 4.7.0 schema table); a class is only ever dispatched
/// when its flag is true *and* the OS granted notification permission.
/// Additive with serde defaults, so a pre-4.7 config file loads unchanged —
/// including one that only ever carried the legacy key, which the frontend
/// migrates into `track_change` on first launch.
#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct NotificationsConfig {
    /// System notification when the playing track changes (the pre-4.7 class).
    #[serde(default = "default_notification_class")]
    pub track_change: bool,
    /// The poller stopped on its own — an auth failure or a self-terminating
    /// loop — so the Dashboard mirror can no longer report "Syncing".
    #[serde(default = "default_notification_class")]
    pub sync_stopped: bool,
    /// A stored Teams session is no longer usable and a sign-in is required.
    #[serde(default = "default_notification_class")]
    pub auth_required: bool,
    /// An update finished staging and will install on quit.
    #[serde(default = "default_notification_class")]
    pub update_staged: bool,
    /// Unknown / future keys NESTED inside this section, retained across
    /// load→save so a section written by a newer binary is not silently
    /// stripped by an older one (issue #938 — the section-level companion of
    /// [`AppConfig::extra`]). Omitted from JSON while empty and skipped in the
    /// TypeScript export, so an untouched config gains no bytes and the
    /// generated TypeScript is unchanged.
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(skip)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// Mirrors the serde defaults field-by-field ([`QuietHoursEntry`]'s pattern):
/// every class defaults to ON, so a config file missing the section and one
/// built with `Default::default()` agree.
impl Default for NotificationsConfig {
    fn default() -> Self {
        Self {
            track_change: default_notification_class(),
            sync_stopped: default_notification_class(),
            auth_required: default_notification_class(),
            update_staged: default_notification_class(),
            extra: BTreeMap::new(),
        }
    }
}

/// Issue #869: one named presence profile — a typed overlay of the
/// base `AppConfig` the user can switch from the tray, a hotkey, or
/// `presencejam --profile <id>`. Every overlay field is `Option<_>`
/// so a profile can carry JUST a status format (a one-line tweak) or a
/// full rules replacement (a "Focus" mode that suppresses every track
/// except the user's whitelist). Names are unique and at most 32
/// characters — [`clamp_presence_profiles`] enforces both at load and
/// on every save. Profile overlays are resolved at READ time through
/// [`effective_config`], so the on-disk base values stay untouched
/// even while a non-default profile is active.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct PresenceProfile {
    /// Profile name, the id the tray / hotkey / CLI use to switch.
    /// Case-sensitive, unique within `presence_profiles`, ≤ 32
    /// characters after trim (`MAX_PROFILE_ID_CHARS`). A profile whose
    /// name normalises to empty is dropped by `clamp_presence_profiles`.
    pub name: String,
    /// `None` keeps the base `teams.status_format`; `Some` overrides it
    /// while the profile is active.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clear_on_pause: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub availability_sync: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate_when_out_of_office: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate_when_presenting: Option<bool>,
    /// Capped at 86 400 (24 h) by `clamp_presence_profiles` so a hand-
    /// edited config cannot put the idle gate in a permanently-firing
    /// state. `None` keeps the base value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "number | null")]
    pub idle_away_after_seconds: Option<u64>,
    /// `None` keeps the base `teams.preferred_presence`; `Some` overlays
    /// the whole `PreferredPresenceConfig` (enabled, availability,
    /// activity, expiry) while the profile is active.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_presence: Option<PreferredPresenceConfig>,
    /// `None` keeps the base `status_rules.track_rules`; `Some`
    /// replaces the whole list while the profile is active. The
    /// `Some(vec![])` shape is a legitimate "no rules" overlay (a
    /// silent profile that only flips the status format).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track_rules: Option<Vec<TrackRuleEntry>>,
    /// `None` keeps the base `notifications`; `Some` overlays each
    /// notification class while the profile is active.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notifications: Option<NotificationsConfig>,
    /// Unknown / future keys NESTED inside this section, retained across
    /// load→save so a section written by a newer binary is not silently
    /// stripped by an older one (issue #938 — the section-level companion of
    /// [`AppConfig::extra`]).
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(skip)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// Default accelerator for the playback toggle (issue #676).
///
/// `CmdOrCtrl` is the plugin parser's platform-portable "primary modifier"
/// spelling (`global_hotkey::hotkey::CMD_OR_CTRL` — SUPER on macOS, CONTROL
/// elsewhere), so a `config.json` carried between platforms keeps the user's
/// intent instead of pinning Command on one machine and Ctrl on another.
pub const DEFAULT_TOGGLE_PLAYBACK_SHORTCUT: &str = "CmdOrCtrl+Alt+P";

/// Default accelerator for the sync pause/resume toggle (issue #676).
pub const DEFAULT_TOGGLE_SYNC_SHORTCUT: &str = "CmdOrCtrl+Alt+S";

/// Global-shortcut bindings (issue #676).
///
/// `None` — and the blank string a hand-edited file can carry — means
/// "unbound": the slot registers nothing and never disturbs the other slot.
/// A key that is *absent* from the file takes the documented default, while an
/// explicit `null` is a deliberate unbinding, because serde applies
/// `default = "…"` only when the key is missing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct ShortcutsConfig {
    #[serde(default = "default_toggle_playback_shortcut")]
    pub toggle_playback: Option<String>,
    #[serde(default = "default_toggle_sync_shortcut")]
    pub toggle_sync: Option<String>,
    /// Unknown / future keys NESTED inside this section, retained across
    /// load→save so a section written by a newer binary is not silently
    /// stripped by an older one (issue #938 — the section-level companion of
    /// [`AppConfig::extra`]). Omitted from JSON while empty and skipped in the
    /// TypeScript export, so an untouched config gains no bytes and the
    /// generated TypeScript is unchanged.
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(skip)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

pub(crate) fn default_toggle_playback_shortcut() -> Option<String> {
    Some(DEFAULT_TOGGLE_PLAYBACK_SHORTCUT.to_string())
}

pub(crate) fn default_toggle_sync_shortcut() -> Option<String> {
    Some(DEFAULT_TOGGLE_SYNC_SHORTCUT.to_string())
}

impl Default for ShortcutsConfig {
    fn default() -> Self {
        Self {
            toggle_playback: default_toggle_playback_shortcut(),
            toggle_sync: default_toggle_sync_shortcut(),
            extra: BTreeMap::new(),
        }
    }
}

/// Shared serde default for every [`NotificationsConfig`] flag.
pub(crate) fn default_notification_class() -> bool {
    true
}

/// Which release manifest the updater consults (4.7.0, issue #678).
///
/// Serialized lowercase — `stable`/`beta` is the on-disk and on-the-wire
/// spelling the Settings picker round-trips, so `rename_all` is part of the
/// contract, not cosmetics (same convention as [`ClientSecretState`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub enum UpdateChannel {
    /// Published stable releases (`releases/latest`): the default channel.
    #[default]
    Stable,
    /// Rolling prerelease builds (`releases/download/beta`). A missing or
    /// non-newer beta manifest falls back to the stable manifest through
    /// `updater_bg::update_endpoints`.
    Beta,
}

/// Updater settings (4.7.0, issue #678). Additive on `AppConfig` with
/// `#[serde(default)]`, so a pre-4.7 config loads as the stable channel.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct UpdatesConfig {
    /// Missing key keeps the serde default (`stable`); an unrecognised value
    /// is read leniently — see [`deserialize_update_channel`].
    #[serde(default, deserialize_with = "deserialize_update_channel")]
    pub channel: UpdateChannel,
    /// Unknown / future keys NESTED inside this section, retained across
    /// load→save so a section written by a newer binary is not silently
    /// stripped by an older one (issue #938 — the section-level companion of
    /// [`AppConfig::extra`]). Omitted from JSON while empty and skipped in the
    /// TypeScript export, so an untouched config gains no bytes and the
    /// generated TypeScript is unchanged.
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(skip)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// Playback source selection (5.0.0, issue #862).
///
/// `Auto` is the documented default — the poll loop tries the OS media-
/// session source first (SMTC on Windows, MPRIS on Linux) and falls back
/// to the Spotify Web API source when the session is empty. `Spotify`
/// reproduces the pre-5.0 behaviour exactly. `System` forces the OS
/// source; on macOS there is no OS source so the poll loop reports
/// "no track" and the user is told to switch back to Spotify in
/// `Onboarding.svelte`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/lib/types-generated/")]
pub struct PlaybackConfig {
    #[serde(default)]
    pub source: crate::sources::PlaybackSourceKind,
    /// Unknown / future keys NESTED inside this section, retained across
    /// load→save so a section written by a newer binary is not silently
    /// stripped by an older one (issue #938 — the section-level companion
    /// of [`AppConfig::extra`]). Omitted from JSON while empty and
    /// skipped in the TypeScript export.
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(skip)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// Lenient read of one `updates.channel` value (4.7.0, issue #678): the
/// channel, plus the warning to log when the document carried a spelling this
/// binary does not know.
///
/// Why lenient: `UpdateChannel` is new in 4.7.0, and a plain enum field makes
/// serde reject the WHOLE document — which `load_config` answers by
/// quarantining `config.json` to `config.json.bak` and booting on defaults, so
/// one unrecognised spelling would cost the user every setting they have (a
/// config written by a newer binary, or a hand-edit, would do it). The rest of
/// the document survives instead.
///
/// Deliberately scoped to this field: the pre-existing enums
/// ([`SpotifyConfig::client_secret_state`] and friends) still reject the whole
/// document, so that inconsistency stays visible rather than half-fixed here.
pub(crate) fn lenient_update_channel(raw: &serde_json::Value) -> (UpdateChannel, Option<String>) {
    // The happy path goes through the enum's own `Deserialize`, so the
    // lowercase wire spelling keeps living in `#[serde(rename_all)]` alone.
    match serde_json::from_value::<UpdateChannel>(raw.clone()) {
        Ok(channel) => (channel, None),
        Err(_) => (
            UpdateChannel::Stable,
            Some(format!(
                "updates.channel: unrecognised value {raw}; using \"stable\" (the rest of the \
                 config is kept)"
            )),
        ),
    }
}

/// `deserialize_with` for [`UpdatesConfig::channel`]: [`lenient_update_channel`]
/// plus its warning. A MISSING key never reaches here — `#[serde(default)]` on
/// the field still supplies the default channel.
pub(crate) fn deserialize_update_channel<'de, D>(deserializer: D) -> Result<UpdateChannel, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = serde_json::Value::deserialize(deserializer)?;
    let (channel, warning) = lenient_update_channel(&raw);
    if let Some(warning) = warning {
        // The `[CFG]` prefix belongs at the log site (`test_config_log_tags_…`
        // scans for it there, and the message is reused verbatim below).
        log::warn!("[CFG] {warning}");
    }
    Ok(channel)
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
    /// Release channel the updater reads (issue #678).
    #[serde(default)]
    pub updates: UpdatesConfig,
    /// Playback source selection (5.0.0, issue #862). `Auto` is the
    /// documented default — try the OS media-session source first,
    /// fall back to Spotify. Additive on `AppConfig` with
    /// `#[serde(default)]`, so a pre-5.0 config file loads as `Auto`
    /// and the on-disk byte shape does not change.
    #[serde(default)]
    pub playback: PlaybackConfig,
    #[serde(default)]
    pub autostart: bool,
    /// Desktop-notification classes (4.7.0 / issue #675). Additive on
    /// `AppConfig` with a serde default so pre-4.7 config files load
    /// unchanged (`schema_version` untouched, `extra` retention untouched).
    #[serde(default)]
    pub notifications: NotificationsConfig,
    /// UI locale for the native surfaces (tray + application menu) and the
    /// webview dictionaries (4.7.0, issue #674). `None` — the documented
    /// pre-4.7 state and every config file written before this release —
    /// reads as `"en"`. Only `en`/`de`/`fr` (or a `de-AT`-style variant of
    /// one) are meaningful; an unknown tag falls back to English and is
    /// logged (see `crate::i18n::resolve_tag`). The frontend mirrors this
    /// value as the single source of truth for the language picker.
    #[serde(default)]
    pub locale: Option<String>,
    /// Persisted "pause sync" deadline set from the tray's snooze submenu
    /// (4.7.0, S9 / issue #677). RFC3339 **in UTC** (`2026-09-17T13:45:00Z`):
    /// the value has to survive a relaunch, a timezone change and a DST
    /// transition without moving, so the three presets are computed from the
    /// local clock and stored as that instant
    /// (`crate::polling::poll_once::snooze`). `None` — the documented default
    /// and every config file written before this release — means polling runs
    /// normally.
    ///
    /// An expired (or unparsable) value is INERT everywhere — [`snooze_status`]
    /// and therefore the tray, the chip and the poller's gate all treat "no
    /// longer a live deadline" as "not snoozed" — and it is removed by the first
    /// WRITE that touches the document: [`clamp_snooze`] through
    /// [`clamped_config`] on any save, `poll_once::clear_snooze_if_expired` on
    /// the iteration that observes the expiry, or the tray's startup cleaner.
    /// `load_config` only reports it (see [`snooze_expired_deadline`]), because
    /// a reader that fixes the field in memory would hide the expiry from the
    /// writers that can actually correct `config.json`.
    #[serde(default)]
    pub snooze_until: Option<String>,
    #[serde(default)]
    pub status_rules: StatusRulesConfig,
    /// Issue #869: the named presence profiles the user can switch from the
    /// tray, a hotkey or `presencejam --profile <id>`. Each profile is a
    /// typed overlay of a SUBSET of the base config — `status_format`,
    /// `clear_on_pause`, `availability_sync`, the gate flags, the preferred
    /// presence, a rules subset and notifications. Switching resolves at
    /// READ time through [`effective_config`], so a profile change NEVER
    /// rewrites the on-disk base values. Empty by default, additive with
    /// `#[serde(default)]` so a pre-5.0 config file loads with no profiles
    /// and the `effective_config` overlay is a no-op.
    #[serde(default)]
    pub presence_profiles: Vec<PresenceProfile>,
    /// Issue #869: the name of the active profile. `None` — the documented
    /// pre-5.0 default — means "use the base configuration". A value that
    /// does not match any profile name is treated as `None` by
    /// [`clamp_presence_profiles`] (the active id is cleared at the IPC
    /// boundary) so a hand-edited config cannot silently land on a phantom
    /// profile and never resolve.
    #[serde(default)]
    pub active_profile: Option<String>,
    /// Global-shortcut bindings (issue #676). Additive with
    /// `#[serde(default)]`, so a pre-4.7 config file loads with the documented
    /// default accelerators rather than with no shortcuts at all.
    #[serde(default)]
    pub shortcuts: ShortcutsConfig,
    /// Config schema version (issue #379). Files written before 4.3.0 carry
    /// no such key and load as version 1.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    /// The document's revision, raised once per accepted save (issue #943).
    ///
    /// Settings renders in both the main window and the detached pane, each
    /// webview holding its own copy loaded once, so two writers can be a save
    /// apart. A save whose payload is OLDER than the revision on disk is
    /// rejected instead of silently reverting the other window's change (see
    /// [`STALE_REVISION_MARKER`]), and [`emit_config_changed`] carries the new
    /// revision so every window can adopt the document that was actually
    /// persisted.
    ///
    /// `0` for a fresh install, for every file written before this field
    /// existed, and for a client that does not send the field yet — which is why
    /// `0` is stamped upward rather than treated as stale. The first save from
    /// such a payload stores `1`.
    ///
    /// Skipped in the TS export: the field is the Rust-side guard until the
    /// store half of #943 ships, and exporting it would make the generated
    /// `AppConfig` require a member the frontend does not construct yet.
    #[serde(default)]
    #[ts(skip)]
    pub revision: u64,
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
            extra: BTreeMap::new(),
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
            gate_when_presenting: default_gate_when_presenting(),
            // Issue #873: `0` = off (the default; an untouched config
            // behaves exactly as today). The clamp runs through
            // `clamp_teams` so any non-zero value lands in 60..=3600.
            idle_away_after_seconds: 0,
            // Issue #867: 0 disables the pre-meeting suppression window
            // (the previous behaviour, which also matches the documented
            // default); any non-zero value is capped at 60 by `clamp_teams`.
            pre_meeting_suppress_minutes: 0,
            paused_status_format: default_paused_status_format(),
            stopped_status_format: default_stopped_status_format(),
            preferred_presence: PreferredPresenceConfig::default(),
            extra: BTreeMap::new(),
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
            extra: BTreeMap::new(),
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            enabled: default_logging_enabled(),
            log_level: default_log_level(),
            max_file_size_mb: default_max_file_size_mb(),
            keep_files: default_keep_files(),
            // Issue #877: opt-in. A user with a busy rule set could
            // otherwise grow the log without bound; the Dashboard's
            // "Activity" card is the always-on reading surface.
            presence_history: false,
            extra: BTreeMap::new(),
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
            updates: UpdatesConfig::default(),
            playback: PlaybackConfig::default(),
            autostart: false,
            notifications: NotificationsConfig::default(),
            locale: None,
            snooze_until: None,
            status_rules: StatusRulesConfig::default(),
            presence_profiles: Vec::new(),
            active_profile: None,
            shortcuts: ShortcutsConfig::default(),
            extra: BTreeMap::new(),
            schema_version: default_schema_version(),
            revision: 0,
        }
    }
}
