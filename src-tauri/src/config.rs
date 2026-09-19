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

fn default_paused_status_format() -> String {
    "Paused".to_string()
}

fn default_stopped_status_format() -> String {
    "Nothing playing on Spotify".to_string()
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
    /// issue #538). `pause_backoff` used to hardcode a 300 s cap; it is now
    /// the ladder's ceiling (default 300, so an untouched config is unchanged)
    /// and the value is clamped into 60..=3600 by `clamp_polling`.
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

/// Bound the user-supplied teams text fields: the profanity lexicon
/// (CfgDiag#3(b), issue #538) to at most 64 entries of 32 characters, and the
/// two manual-status texts (S4, issue #672) to
/// [`MAX_RULE_STATUS_CHARS`] like a rule's replacement text.
/// `clamped_config` is the only normalizer, so this runs on load and on every
/// save.
fn clamp_teams(cfg: &mut TeamsConfig) {
    cfg.profanity_extra_words.truncate(64);
    for word in &mut cfg.profanity_extra_words {
        if word.chars().count() > 32 {
            *word = word.chars().take(32).collect();
        }
    }
    // S4 (issue #672): the two manual-status texts are status lines too, so they
    // are bounded exactly like a rule's replacement text. An empty text is left
    // alone — `poll_once` reads it as "use the default".
    clamp_rule_text(&mut cfg.paused_status_format);
    clamp_rule_text(&mut cfg.stopped_status_format);
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

/// S4 (issue #672): minutes in a track rule's day. `end_minutes` may be
/// `TRACK_RULE_DAY_MINUTES` (= the end of the day), which is why the track-rule
/// window uses `u32` while [`QuietHoursEntry`] clamps to `0..=1439`.
pub const TRACK_RULE_DAY_MINUTES: u32 = 1440;

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
        clamp_track_rule_window(rule);
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

/// S4 (issue #672): normalize a track rule's schedule in place.
///
/// Minutes are clamped into `0..=TRACK_RULE_DAY_MINUTES`, so a hand-edited
/// config cannot wedge the comparison, and the weekday filter is reduced to the
/// documented ISO range `1..=7` (deduplicated, so `days` matches the `days`
/// invariant [`QuietHoursEntry`] relies on). The window itself keeps
/// [`QuietHoursEntry`]'s semantics: `[start, end)` with a wrap-around pair
/// (`start > end`, e.g. 22:00→07:00) honoured, and `start == end` matching
/// nothing.
fn clamp_track_rule_window(rule: &mut TrackRuleEntry) {
    // A START of 1440 is unreachable: `now` never exceeds 1439, so such a rule
    // could never match while the picker happily renders it as 00:00. Clamp the
    // start to the last minute of the day instead, and the end to the end of
    // the day (1440), which IS reachable as "until midnight".
    rule.start_minutes = rule.start_minutes.min(TRACK_RULE_DAY_MINUTES - 1);
    rule.end_minutes = rule.end_minutes.min(TRACK_RULE_DAY_MINUTES);
    rule.days.retain(|day| (1..=7).contains(day));
    rule.days.sort_unstable();
    rule.days.dedup();
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
    pub max_file_size_mb: u64,
    /// How many *archived* log files to retain (4.7.0). The active
    /// `PresenceJam.log` is not counted, so the directory holds at most
    /// `keep_files + 1` log files. Clamped to 1..=20 by [`clamp_logging`].
    #[serde(default = "default_keep_files")]
    pub keep_files: u32,
}

fn default_logging_enabled() -> bool {
    true
}

fn default_log_level() -> String {
    "Info".to_string()
}

fn default_max_file_size_mb() -> u64 {
    10
}

fn default_keep_files() -> u32 {
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
fn clamp_logging(cfg: &mut LoggingConfig) {
    cfg.max_file_size_mb = cfg.max_file_size_mb.clamp(1, 500);
    cfg.keep_files = cfg.keep_files.clamp(1, 20);
}

/// Whether a stored snooze deadline is still in the future (4.7.0, S9 /
/// issue #677), as a UTC instant.
///
/// `None` for an absent, unparsable or already-passed value, so a hand-edited
/// `config.json` can never resurrect a snooze. The parse itself is
/// **tolerant of the offset**: RFC3339 accepts `+02:00` as well as `Z` and both
/// denote an instant, which is what a value re-serialized by another tool (or
/// by a future version that writes local time) may carry — only a *past*
/// instant, a malformed string or a bare date is rejected here.
pub fn snooze_deadline(
    stored: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<chrono::DateTime<chrono::Utc>> {
    let deadline = chrono::DateTime::parse_from_rfc3339(stored.trim())
        .ok()?
        .with_timezone(&chrono::Utc);
    (deadline > now).then_some(deadline)
}

/// Whether a config carries a `snooze_until` that is no longer a live deadline
/// (4.7.0, S9 / issue #677) — the non-mutating twin of [`clamp_snooze`], for
/// readers (`load_config`) that must report the state without changing it.
///
/// True for an absent field? No: an absent field is simply "not snoozed", which
/// is not something to report or clean. True for an unparsable value and for an
/// instant that has passed — both are dead weight that a writer should remove.
pub fn snooze_expired_deadline(cfg: &AppConfig, now: chrono::DateTime<chrono::Utc>) -> bool {
    cfg.snooze_until.is_some() && snooze_status(cfg, now).is_none()
}

/// Drops an expired (or unparsable) `snooze_until` (4.7.0, S9 / issue #677).
///
/// Returns `true` when the field was cleared. Pure apart from its `now`
/// argument, so the boundary is unit-testable. Only the WRITE paths call it:
/// [`clamped_config`] (so every save normalizes the value) and the two guarded
/// cleaners that own a config-write guard — `poll_once::clear_snooze_if_expired`
/// on the iteration that observes the expiry, and the tray's startup cleaner.
/// `load_config` deliberately uses [`snooze_expired_deadline`] instead: it is a
/// reader on paths that hold no write guard, and clearing in memory there would
/// hide the expiry from the very cleaners that can fix the file.
pub fn clamp_snooze(cfg: &mut AppConfig, now: chrono::DateTime<chrono::Utc>) -> bool {
    if !snooze_expired_deadline(cfg, now) {
        return false;
    }
    cfg.snooze_until = None;
    true
}

/// A snooze preset offered by the tray submenu (4.7.0, S9 / issue #677).
///
/// The first two are instant offsets (`now + delta`), which no timezone can
/// move. The third is a LOCAL calendar boundary, which is why it is a variant
/// of its own rather than a `Duration` — see [`next_local_midnight_utc`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnoozePreset {
    ThirtyMinutes,
    OneHour,
    UntilTomorrow,
}

/// The deadline a preset denotes (4.7.0, S9 / issue #677).
///
/// Both clocks are arguments rather than read inside, so the "until tomorrow"
/// boundary can be pinned at an exact wall-clock time and timezone in a unit
/// test — the boundary is where a timezone bug would hide.
pub fn snooze_preset_deadline<Tz: chrono::TimeZone>(
    preset: SnoozePreset,
    now_utc: chrono::DateTime<chrono::Utc>,
    now_local: chrono::DateTime<Tz>,
) -> chrono::DateTime<chrono::Utc> {
    match preset {
        SnoozePreset::ThirtyMinutes => now_utc + chrono::TimeDelta::minutes(30),
        SnoozePreset::OneHour => now_utc + chrono::TimeDelta::minutes(60),
        SnoozePreset::UntilTomorrow => next_local_midnight_utc(now_local),
    }
}

/// The start of the next LOCAL calendar day, as a UTC instant — the "until
/// tomorrow" deadline (4.7.0, S9 / issue #677).
///
/// ## Semantics
///
/// "Until tomorrow" means *the next local midnight*, never `now + 24 h`:
///
/// - `snooze_until` is persisted as a UTC instant, but "tomorrow" is read from
///   the machine's local calendar. Computing the deadline as a fixed offset
///   from `now` would silently stretch or shrink the snooze by the UTC offset:
///   at 23:00 in UTC+13 the user means one hour of quiet, not twenty-five, and
///   at 00:05 in UTC−11 they mean almost a full day.
/// - The boundary is the START of the next day. At exactly local midnight the
///   deadline is a full day away; one second later it is one second short of a
///   day. Either way it is strictly in the future, so an "until tomorrow"
///   snooze is always at least one second long.
/// - A DST transition inside the window changes the real duration, never the
///   wall-clock boundary: a fall-back night lasts 25 hours, a spring-forward
///   night 23. Keeping the calendar boundary is what makes the label true.
///
/// ## DST edge cases at local midnight
///
/// - **Ambiguous** — a fall-back repeats local midnight (e.g.
///   `America/Santiago`): the EARLIER of the two instants wins. It is the
///   conservative choice for a deadline (the user asked to stop) and, being
///   derived from the wall clock alone, gives the same answer on every
///   evaluation.
/// - **Gap** — a spring-forward swallows local midnight in a zone whose
///   transition is at 00:00: the first instant that exists on the new day is
///   used, so the deadline lands inside tomorrow instead of on a wall-clock
///   time that never happens.
fn next_local_midnight_utc<Tz: chrono::TimeZone>(
    now_local: chrono::DateTime<Tz>,
) -> chrono::DateTime<chrono::Utc> {
    let midnight = (now_local.date_naive() + chrono::Days::new(1))
        .and_hms_opt(0, 0, 0)
        .expect("00:00:00 is always a valid wall-clock time");
    resolve_local_forward(&now_local.timezone(), midnight)
}

/// The earliest UTC instant at or after the local wall-clock time `naive`
/// (4.7.0, S9 / issue #677).
///
/// `Single` is the normal case, `Ambiguous` takes the earlier instant and
/// `None` (a gap: the wall clock does not exist) walks forward a minute at a
/// time to the first instant that does. A real offset change is minutes, never
/// hours, and the walk is bounded by [`LOCAL_GAP_PROBE_MINUTES`], so it can
/// neither spin nor run long.
fn resolve_local_forward<Tz: chrono::TimeZone>(
    tz: &Tz,
    naive: chrono::NaiveDateTime,
) -> chrono::DateTime<chrono::Utc> {
    let mut probe = naive;
    for _ in 0..LOCAL_GAP_PROBE_MINUTES {
        match tz.from_local_datetime(&probe) {
            chrono::LocalResult::Single(dt) => return dt.with_timezone(&chrono::Utc),
            chrono::LocalResult::Ambiguous(earlier, _) => {
                return earlier.with_timezone(&chrono::Utc)
            }
            chrono::LocalResult::None => probe += chrono::TimeDelta::minutes(1),
        }
    }
    // Unreachable for any real timezone — no DST gap is twelve hours deep.
    // Reading the wall clock as UTC keeps a menu click from panicking over a
    // deadline; the resulting snooze is simply long.
    chrono::DateTime::from_naive_utc_and_offset(naive, chrono::Utc)
}

/// Upper bound on the spring-forward walk in [`resolve_local_forward`]: twelve
/// hours in minutes, far past any real gap (the deepest known is two hours).
const LOCAL_GAP_PROBE_MINUTES: u32 = 720;

/// The persisted spelling of a deadline: RFC3339, UTC, second precision
/// (`2026-09-17T13:45:00Z`) (4.7.0, S9 / issue #677).
///
/// One constructor, so the spelling the tray writes and the spelling
/// [`snooze_deadline`] parses cannot drift. Second precision because the
/// presets are whole minutes and a sub-second deadline would only make two
/// equal states look different.
pub fn snooze_store_form(deadline: chrono::DateTime<chrono::Utc>) -> String {
    deadline.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// A snooze that is currently active, as the tray renders it (4.7.0, S9 /
/// issue #677).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnoozeStatus {
    /// The stored deadline, as the instant it denotes.
    pub deadline: chrono::DateTime<chrono::Utc>,
    /// Whole seconds left; always `>= 1`.
    pub remaining_seconds: i64,
}

/// The active snooze for a config, or `None` when there is none / it has
/// passed / the stored value cannot be parsed (4.7.0, S9 / issue #677).
pub fn snooze_status(cfg: &AppConfig, now: chrono::DateTime<chrono::Utc>) -> Option<SnoozeStatus> {
    let stored = cfg.snooze_until.as_deref()?;
    let deadline = snooze_deadline(stored, now)?;
    Some(SnoozeStatus {
        deadline,
        remaining_seconds: (deadline - now).num_seconds(),
    })
}

/// The countdown the tray and the Dashboard both render: whole minutes left,
/// rounded UP, and never below 1 (4.7.0, S9 / issue #677).
///
/// Rounding up means a freshly-set 30-minute snooze reads "30 min" rather than
/// "29", and the last minute reads "1 min" rather than "0" for a snooze that is
/// still active. `min(1440)` bounds a hand-edited config's absurd deadline
/// without hiding it.
pub fn snooze_minutes_left(remaining_seconds: i64) -> i64 {
    // `i64::div_ceil` is still unstable (`int_roundings`), so round up by hand;
    // the `max(0)` makes the pair exact for every input.
    let seconds = remaining_seconds.max(0);
    // `saturating_add`: a hand-edited absurd deadline must clamp, not panic.
    ((seconds.saturating_add(59)) / 60).clamp(1, 24 * 60)
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

fn default_quiet_end() -> u16 {
    420
}

fn default_pause_polling() -> bool {
    false
}

fn default_track_rule_days() -> Vec<u8> {
    Vec::new()
}

fn default_track_rule_start() -> u32 {
    0
}

/// The contract's default end: the end of the day, so the default window
/// covers every minute (0 → 1440).
fn default_track_rule_end() -> u32 {
    TRACK_RULE_DAY_MINUTES
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
    /// S4 (issue #672): ISO weekday numbers 1 (Mon)..=7 (Sun) this rule
    /// applies on; empty = every day — the same shape and semantics
    /// [`QuietHoursEntry::days`] uses, normalized by `clamp_rules` (see
    /// [`clamp_track_rule_window`]).
    #[serde(default = "default_track_rule_days")]
    pub days: Vec<u8>,
    /// S4 (issue #672): start of the rule's local-time window, in minutes since
    /// midnight. The window is `[start_minutes, end_minutes)` with the same
    /// wrap-around rule as [`QuietHoursEntry`] (22:00→07:00 works); the
    /// default pair (`0`, `1440`) covers every minute of the day.
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
            replacement_status: String::new(),
            presence_availability: String::new(),
            presence_activity: String::new(),
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
        }
    }
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
}

fn default_toggle_playback_shortcut() -> Option<String> {
    Some(DEFAULT_TOGGLE_PLAYBACK_SHORTCUT.to_string())
}

fn default_toggle_sync_shortcut() -> Option<String> {
    Some(DEFAULT_TOGGLE_SYNC_SHORTCUT.to_string())
}

impl Default for ShortcutsConfig {
    fn default() -> Self {
        Self {
            toggle_playback: default_toggle_playback_shortcut(),
            toggle_sync: default_toggle_sync_shortcut(),
        }
    }
}

/// Shared serde default for every [`NotificationsConfig`] flag.
fn default_notification_class() -> bool {
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
    /// Published releases (`releases/latest`): the default, and the only
    /// channel with a published manifest in 4.7.0.
    #[default]
    Stable,
    /// Pre-release builds. 4.7.0 ships the switch only — no
    /// `latest-beta.json` is published yet, so a beta check falls through to
    /// the stable manifest (see `updater_bg::update_endpoints`).
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
fn lenient_update_channel(raw: &serde_json::Value) -> (UpdateChannel, Option<String>) {
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
fn deserialize_update_channel<'de, D>(deserializer: D) -> Result<UpdateChannel, D::Error>
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
    /// Global-shortcut bindings (issue #676). Additive with
    /// `#[serde(default)]`, so a pre-4.7 config file loads with the documented
    /// default accelerators rather than with no shortcuts at all.
    #[serde(default)]
    pub shortcuts: ShortcutsConfig,
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
            paused_status_format: default_paused_status_format(),
            stopped_status_format: default_stopped_status_format(),
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
            max_file_size_mb: default_max_file_size_mb(),
            keep_files: default_keep_files(),
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
            autostart: false,
            notifications: NotificationsConfig::default(),
            locale: None,
            snooze_until: None,
            status_rules: StatusRulesConfig::default(),
            shortcuts: ShortcutsConfig::default(),
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
    /// S4 (issue #672): the user-templatable paused/stopped status texts, part
    /// of the same field-level patch as the rest of the section — a Settings
    /// save that omitted them would leave the stored text untouched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paused_status_format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stopped_status_format: Option<String>,
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
        if let Some(v) = &p.paused_status_format {
            base.teams.paused_status_format = v.clone();
        }
        if let Some(v) = &p.stopped_status_format {
            base.teams.stopped_status_format = v.clone();
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
///
/// Shared with the config-import command (4.7.0, S5), which moves the
/// outgoing file here before an imported document replaces it.
pub(crate) fn quarantine_backup_path(path: &std::path::Path) -> PathBuf {
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

/// Every top-level key [`AppConfig`] has a typed field for (issue #926).
///
/// [`config_from_sections`] reads these one at a time and everything else lands
/// in [`AppConfig::extra`] — the same partition `#[serde(flatten)]` performs
/// when serde parses the document in one call, written out so that one bad
/// section can be replaced by its default without rejecting the rest.
/// `typed_config_keys_match_the_serialized_schema` fails if this list and the
/// struct ever disagree.
const TYPED_CONFIG_KEYS: [&str; 12] = [
    "spotify",
    "teams",
    "polling",
    "logging",
    "updates",
    "autostart",
    "notifications",
    "locale",
    "snooze_until",
    "status_rules",
    "shortcuts",
    "schema_version",
];

/// The JSON type of `value`, for a log line that names the shape of a bad root
/// without echoing a whole (possibly multi-megabyte) document.
fn json_kind(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "a boolean",
        serde_json::Value::Number(_) => "a number",
        serde_json::Value::String(_) => "a string",
        serde_json::Value::Array(_) => "an array",
        serde_json::Value::Object(_) => "an object",
    }
}

/// Deserialize ONE typed field out of a config document, falling back to
/// `fallback` when that field alone does not match the schema (issue #926).
///
/// Per-field, not per-document. A key the file omits takes `fallback`
/// silently — what `#[serde(default)]` has always done — while a key that is
/// PRESENT but invalid takes it with a `[CFG]` warning naming the key, which is
/// the observable replacement for the old all-or-nothing parse. A `null` value
/// is "present but invalid" for every non-`Option` field and a value for an
/// `Option` one, so the field's own type decides, not a special case here.
fn field_or_fallback<T: serde::de::DeserializeOwned>(
    root: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    fallback: T,
) -> T {
    let Some(raw) = root.get(key) else {
        return fallback;
    };
    match serde_json::from_value::<T>(raw.clone()) {
        Ok(value) => value,
        Err(e) => {
            log::warn!(
                "[CFG] config field '{}' is invalid ({}) — using its default, the rest of the config is kept",
                key,
                e
            );
            fallback
        }
    }
}

/// Build an [`AppConfig`] from a config document's root object, one typed field
/// at a time (issue #926).
///
/// A section that no longer matches the schema costs exactly that section — its
/// default, warned about by [`field_or_fallback`] — instead of the whole
/// document, which is what one wrong-typed, out-of-range or misspelled value
/// used to cost (the file was quarantined and the app booted on defaults). The
/// document's scalar fields take the same route, so `"autostart": "yes"` cannot
/// take quiet hours down with it either.
///
/// Unknown top-level keys are still retained in [`AppConfig::extra`] (issue
/// #379), exactly as the `#[serde(flatten)]` field collected them under the
/// single-pass parse.
fn config_from_sections(root: serde_json::Map<String, serde_json::Value>) -> AppConfig {
    let mut config = AppConfig {
        spotify: field_or_fallback(&root, "spotify", Default::default()),
        teams: field_or_fallback(&root, "teams", Default::default()),
        polling: field_or_fallback(&root, "polling", Default::default()),
        logging: field_or_fallback(&root, "logging", Default::default()),
        updates: field_or_fallback(&root, "updates", Default::default()),
        autostart: field_or_fallback(&root, "autostart", Default::default()),
        notifications: field_or_fallback(&root, "notifications", Default::default()),
        locale: field_or_fallback(&root, "locale", Default::default()),
        snooze_until: field_or_fallback(&root, "snooze_until", Default::default()),
        status_rules: field_or_fallback(&root, "status_rules", Default::default()),
        shortcuts: field_or_fallback(&root, "shortcuts", Default::default()),
        schema_version: field_or_fallback(&root, "schema_version", default_schema_version()),
        extra: BTreeMap::new(),
    };
    for (key, value) in root {
        if !TYPED_CONFIG_KEYS.contains(&key.as_str()) {
            config.extra.insert(key, value);
        }
    }
    config
}

/// Tighten a loose `config.json` to 0600 (issue #135 path A), best-effort
/// since issue #802.
///
/// Idempotent on a file that is already 0600. Unix-only: Windows' default ACL
/// is already user-only, so there is nothing to tighten there.
///
/// Best-effort, NOT a precondition: a mode that cannot be READ (EROFS on a
/// read-only or ostree mount, EPERM on a file owned by another user, an
/// ACL-managed path) or cannot be CHANGED is logged and ignored. Both used to
/// be `?`-propagated, so `load_config` failed outright on a perfectly readable
/// file — startup logged "no config found", `AppState.config` stayed `None`,
/// the Settings and Dashboard stores fell back to built-in defaults, and
/// because a Settings save posts the whole document, the next save persisted
/// those defaults over the user's real file. Hardening a file we can already
/// read is a courtesy; refusing to read it is a data-loss path.
#[cfg(unix)]
fn tighten_config_permissions(path: &std::path::Path) {
    let current = match fs::metadata(path) {
        Ok(metadata) => metadata.permissions(),
        Err(e) => {
            log::warn!(
                "[CFG] Could not read the mode of config file '{}': {} — loading it anyway",
                path.display(),
                e
            );
            return;
        }
    };
    let current_mode = current.mode() & 0o777;
    if current_mode == 0o600 {
        return;
    }
    log::warn!(
        "[CFG] Tightening config.json mode from {:o} to 0600 (issue #135)",
        current_mode
    );
    let mut tightened = current;
    tightened.set_mode(0o600);
    if let Err(e) = fs::set_permissions(path, tightened) {
        log::warn!(
            "[CFG] Could not chmod config file '{}' to 0600: {} — loading it anyway",
            path.display(),
            e
        );
    }
}

pub fn load_config() -> Result<AppConfig, String> {
    load_config_from(&get_config_path()?).map(with_keychain_flags)
}

/// Path-taking core of [`load_config`]: the file I/O, the section-by-section
/// parse and the normalization, with the keychain stamping left to the public
/// entry point — so this half is testable against real files with no keychain
/// probe, the same shape [`import_config_document`] uses.
fn load_config_from(path: &std::path::Path) -> Result<AppConfig, String> {
    if !path.exists() {
        log::info!(
            "[CFG] Config file not found at '{}', using defaults",
            path.display()
        );
        return Ok(AppConfig::default());
    }

    // Issue #135 path A: tighten the mode of any pre-existing config.json that
    // was created loose by an older PresenceJam version (default umask 022 →
    // 0644). Best-effort since issue #802 — see `tighten_config_permissions`.
    #[cfg(unix)]
    tighten_config_permissions(path);

    let mut file = fs::File::open(path)
        .map_err(|e| format!("Failed to open config file '{}': {}", path.display(), e))?;

    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .map_err(|e| format!("Failed to read config file '{}': {}", path.display(), e))?;

    let mut config = match serde_json::from_str::<serde_json::Value>(&contents) {
        // Issue #926: the document IS an object — load it field by field, so a
        // section that no longer matches the schema costs exactly that
        // section's default and nothing else.
        Ok(serde_json::Value::Object(root)) => config_from_sections(root),
        // Anything else is not a config: a bare array/string/number/null
        // root, or text that is not JSON at all. Quarantine, exactly as the
        // single-pass parse answered those two shapes before.
        Ok(other) => {
            quarantine_corrupt_config(
                path,
                format!("expected a JSON object, found {}", json_kind(&other)),
            );
            return Ok(AppConfig::default());
        }
        Err(e) => {
            // Issue #379: never lose the evidence — quarantine the corrupt
            // file to `<config>.bak` alongside the original and boot on
            // defaults. Observable via `config_was_quarantined()`.
            quarantine_corrupt_config(path, &e);
            return Ok(AppConfig::default());
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
    clamp_logging(&mut config.logging);
    // 4.7.0 (S9, issue #677): an expired snooze is reported here and REMOVED by
    // the guarded writers below, never by this reader.
    //
    // The distinction is load-bearing twice over. (a) `load_config` runs on
    // paths that hold no config-write guard — the startup load, the
    // `load_config` command, `update_config`'s cold read — so writing the file
    // from here could clobber a concurrent save and break the #297 invariant
    // that the file and the in-memory copy agree. (b) An expired value must stay
    // VISIBLE to the consumer that can persist its removal: `poll_once`'s
    // `SnoozeGate::Expired` arm and the tray's startup cleaner both read the
    // stored field. Clearing it here made the disk drift permanent — the
    // in-memory copy looked clean, so nothing ever rewrote `config.json`, and
    // the line below repeated on every launch.
    if snooze_expired_deadline(&config, chrono::Utc::now()) {
        log::info!("[CFG] snooze: the stored deadline had already passed — it is ignored and cleared on the next write");
    }

    log::info!("[CFG] Loaded configuration from '{}'", path.display());
    Ok(config)
}

/// The persisted `logging` section, read without a full config load
/// (4.7.0, S5).
///
/// The log plugin is registered on the Tauri builder *before* the `setup`
/// hook runs, so the rotating file target needs its size and retention
/// settings before [`load_config`] is reached. Deliberately narrow:
/// no migration, no quarantine side effects, and **no keychain probe** —
/// `load_config` runs moments later and a second probe at startup is a real
/// macOS prompt risk (see [`with_keychain_flags`]).
///
/// A missing, unreadable or unparsable file yields the defaults; the real
/// load still handles the corrupt-file case.
pub fn logging_config_for_startup() -> LoggingConfig {
    let mut logging = match get_config_path()
        .ok()
        .and_then(|path| fs::read_to_string(path).ok())
    {
        Some(contents) => match serde_json::from_str::<AppConfig>(&contents) {
            Ok(cfg) => cfg.logging,
            Err(e) => {
                log::warn!(
                    "[CFG] startup log-rotation read: config unparsable ({}); using log defaults",
                    e
                );
                LoggingConfig::default()
            }
        },
        None => LoggingConfig::default(),
    };
    clamp_logging(&mut logging);
    logging
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

pub(crate) fn atomic_write_json(path: &std::path::Path, json: &str) -> Result<(), String> {
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
    clamp_logging(&mut cfg.logging);
    // 4.7.0 (S9, issue #677): a write that carries an already-expired deadline
    // (a whole-document save from a stale draft, or a resume click that raced
    // its own deadline) normalizes it away, so the in-memory copy, the file on
    // disk and the tray can never disagree about a snooze being active.
    clamp_snooze(&mut cfg, chrono::Utc::now());
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

// ---------------------------------------------------------------------------
// 4.7.0 (S5): config export / import.
//
// The Spotify client secret is keychain-only (issue #9), so an exported
// document is a *shareable* file: every `client_secret` key is stripped on
// the way out and an incoming document that carries one is refused outright,
// rather than silently dropping the plaintext the user asked us to import.
// ---------------------------------------------------------------------------

/// Timestamp suffix of an export file name — `YYYYMMDD-HHMMSS`, UTC.
fn export_timestamp(at: chrono::DateTime<chrono::Utc>) -> String {
    at.format("%Y%m%d-%H%M%S").to_string()
}

/// Name an export is offered under:
/// `presencejam-config-<version>-<YYYYMMDD-HHMMSS>.json`.
pub fn export_file_name(version: &str, at: chrono::DateTime<chrono::Utc>) -> String {
    format!(
        "presencejam-config-{}-{}.json",
        version,
        export_timestamp(at)
    )
}

/// Collect the dotted paths of every `client_secret` key anywhere in `value`.
///
/// Walks nested objects and arrays: a secret cannot hide inside `extra`
/// (the unknown-top-level-key retention map) or a hand-written nested object
/// just because the typed schema has no such field. Only keys are matched —
/// values are irrelevant to the decision.
fn client_secret_paths(value: &serde_json::Value) -> Vec<String> {
    fn walk(value: &serde_json::Value, prefix: &str, out: &mut Vec<String>) {
        match value {
            serde_json::Value::Object(map) => {
                for (key, child) in map {
                    let path = if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", prefix, key)
                    };
                    if key == "client_secret" {
                        out.push(path.clone());
                    }
                    walk(child, &path, out);
                }
            }
            serde_json::Value::Array(items) => {
                for (index, child) in items.iter().enumerate() {
                    walk(child, &format!("{}[{}]", prefix, index), out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(value, "", &mut out);
    out
}

/// Remove every `client_secret` key anywhere in the tree; returns how many
/// were removed.
fn strip_client_secret_keys(value: &mut serde_json::Value) -> usize {
    let mut removed = 0;
    match value {
        serde_json::Value::Object(map) => {
            if map.remove("client_secret").is_some() {
                removed += 1;
            }
            for child in map.values_mut() {
                removed += strip_client_secret_keys(child);
            }
        }
        serde_json::Value::Array(items) => {
            for child in items.iter_mut() {
                removed += strip_client_secret_keys(child);
            }
        }
        _ => {}
    }
    removed
}

/// The document an export writes (4.7.0, S5): the persisted shape of `cfg`,
/// clamped exactly as [`save_config`] would write it, with every
/// `client_secret` key and both derived keychain views stripped.
///
/// The secret strip is a guard, not the normal path — the secret lives in the
/// OS keychain and there is no typed field to carry it — but a secret reaching
/// a file the user is explicitly told to keep or share would undo the keychain
/// migration. Token material never enters `AppConfig` at all.
///
/// `client_secret_set` / `client_secret_state` go too: they are display
/// projections of *this* machine's keychain (issue #560), so an exported file
/// that carried them would describe the exporting machine to whoever imports
/// it. `load_config` re-stamps both from the real keychain on the next read.
pub fn export_document(cfg: &AppConfig) -> Result<String, String> {
    let mut value = serde_json::to_value(clamped_config(cfg))
        .map_err(|e| format!("Failed to serialize config to JSON: {}", e))?;
    let stripped = strip_client_secret_keys(&mut value);
    if stripped > 0 {
        log::warn!(
            "[CFG] export: stripped {} client_secret key(s) from the exported document",
            stripped
        );
    }
    if let Some(spotify) = value
        .get_mut("spotify")
        .and_then(serde_json::Value::as_object_mut)
    {
        spotify.remove("client_secret_set");
        spotify.remove("client_secret_state");
    }
    serde_json::to_string_pretty(&value)
        .map_err(|e| format!("Failed to serialize config to JSON: {}", e))
}

/// An imported document that passed validation and normalization.
#[derive(Debug)]
pub struct PreparedImport {
    /// Migrated and clamped config, with the keychain views neutralized.
    pub config: AppConfig,
    /// The exact JSON [`import_config_document`] writes for it.
    pub document: String,
}

/// Validate and normalize an imported config document (4.7.0, S5).
///
/// Refuses a document carrying a `client_secret` key anywhere (the pre-#560
/// plaintext shape, or a hand-edited file) instead of quietly dropping it:
/// silently discarding a credential the user meant to import is worse than
/// telling them why it will not be imported. Every other field takes the same
/// route a file read off disk takes — the version migration and all the
/// clamps — so an import cannot introduce a value the UI could not have saved.
pub fn prepare_import(raw: &str) -> Result<PreparedImport, String> {
    let value: serde_json::Value =
        serde_json::from_str(raw).map_err(|e| format!("Imported file is not valid JSON: {}", e))?;
    if !value.is_object() {
        return Err(
            "Imported file is not a PresenceJam configuration (expected a JSON object)".to_string(),
        );
    }

    let secrets = client_secret_paths(&value);
    if !secrets.is_empty() {
        return Err(format!(
            "Imported file carries a plaintext client_secret ({}); PresenceJam keeps the Spotify client secret in the OS keychain, never in config.json",
            secrets.join(", ")
        ));
    }
    // No strip pass here: `client_secret_paths` matches the *key* regardless of
    // value, so every shape of it — a string, an explicit null, a nested object
    // — was already refused above. Nothing can reach the document below.

    let mut config: AppConfig = serde_json::from_value(value).map_err(|e| {
        format!(
            "Imported file does not match the PresenceJam configuration schema: {}",
            e
        )
    })?;

    // Same ordering as `load_config`: the version dispatcher runs before the
    // clamps, so a migration's rewritten values are never re-clamped away.
    let from_version = config.schema_version;
    migrate_config(&mut config, from_version);
    clamp_polling(&mut config.polling);
    clamp_teams(&mut config.teams);
    clamp_rules(&mut config.status_rules);
    clamp_logging(&mut config.logging);
    // Deliberately NOT `stamp_schema_version`: `migrate_config` raises the
    // version to the floor and passes a *newer* file through at its own
    // version, exactly as `load_config` does. Stamping would relabel a
    // newer document as if this binary had produced it.

    // The two keychain views describe the machine the file came from, never
    // the importing one — `load_config` re-stamps them from the real keychain
    // as soon as the import lands.
    config.spotify.client_secret_set = false;
    config.spotify.client_secret_state = ClientSecretState::Absent;

    let document = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Failed to serialize imported config to JSON: {}", e))?;
    Ok(PreparedImport { config, document })
}

/// Replace the config at `path` with an imported document (4.7.0, S5).
///
/// `confirm_overwrite` is the user's decision, asked **after** the document has
/// been validated and only when there is a current file to replace: a decline is
/// a clean no-op that leaves the live file byte-identical and writes no `.bak`.
/// The dialog itself lives in the command (it needs an `AppHandle`); this shape
/// keeps the decision itself testable against real files.
///
/// The outgoing file is moved to `<path>.bak` first (the same backup path the
/// corrupt-file quarantine uses), so an import is never a one-way door; a
/// missing current file is not an error (a fresh install has nothing to back up)
/// and does not prompt. Refuses before touching disk, so a rejected import
/// leaves both the live config and the previous `.bak` untouched. `Ok(None)`
/// means the user declined.
pub fn import_config_document(
    raw: &str,
    path: &std::path::Path,
    confirm_overwrite: impl FnOnce() -> bool,
) -> Result<Option<AppConfig>, String> {
    let prepared = prepare_import(raw)?;

    if path.exists() && !confirm_overwrite() {
        log::info!(
            "[CFG] import: DECLINED by the user; '{}' left untouched",
            path.display()
        );
        return Ok(None);
    }
    if path.exists() {
        let backup = quarantine_backup_path(path);
        fs::rename(path, &backup).map_err(|e| {
            format!(
                "Failed to move the current config to '{}': {}",
                backup.display(),
                e
            )
        })?;
        log::info!(
            "[CFG] import: previous config moved to '{}'",
            backup.display()
        );
    }

    atomic_write_json(path, &prepared.document)?;
    log::info!(
        "[CFG] import: configuration imported into '{}'",
        path.display()
    );
    Ok(Some(prepared.config))
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

    /// 4.7.0 / issue #675: `notifications` is a new top-level section, so a
    /// pre-4.7 `config.json` has no such key and every class must come back
    /// ON (the frontend's per-class dispatch reads these flags — a
    /// `false` default would silently disable a class the user never turned
    /// off). The other direction matters too: an explicit `false` is a user
    /// decision and must survive the round-trip, since `save_config` is how
    /// the Settings card persists a toggle.
    #[test]
    fn notifications_default_on_and_round_trip() {
        let legacy: NotificationsConfig =
            serde_json::from_str("{}").expect("a pre-4.7 config must still deserialize");
        assert!(legacy.track_change);
        assert!(legacy.sync_stopped);
        assert!(legacy.auth_required);
        assert!(legacy.update_staged);
        assert_eq!(
            legacy.track_change,
            NotificationsConfig::default().track_change
        );

        let off = r#"{"track_change":false,"sync_stopped":true,"auth_required":true,"update_staged":false}"#;
        let parsed: NotificationsConfig = serde_json::from_str(off).unwrap();
        assert!(!parsed.track_change);
        assert!(!parsed.update_staged);
        assert_eq!(serde_json::to_string(&parsed).unwrap(), off);
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
        cfg.teams.paused_status_format = "Custom paused".to_string();
        cfg.teams.stopped_status_format = "Custom stopped".to_string();
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
            pause_polling: true,
            ..QuietHoursEntry::default()
        });
        cfg.status_rules.track_rules.push(TrackRuleEntry {
            enabled: true,
            artist_substring: "lofi".to_string(),
            track_substring: String::new(),
            days: vec![6, 7],
            start_minutes: 480,
            end_minutes: 1020,
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
                r#"{"teams": {"paused_status_format": "BRB"}}"#,
                &["teams.paused_status_format"],
            ),
            (
                r#"{"teams": {"stopped_status_format": "Idle"}}"#,
                &["teams.stopped_status_format"],
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
            ..LoggingConfig::default()
        });
        assert_eq!(
            log::max_level(),
            log::LevelFilter::Off,
            "logging.enabled = false must silence the logger immediately"
        );

        apply_log_level(&LoggingConfig {
            enabled: true,
            log_level: "DEBUG".to_string(),
            ..LoggingConfig::default()
        });
        assert_eq!(log::max_level(), log::LevelFilter::Debug);

        apply_log_level(&LoggingConfig {
            enabled: true,
            log_level: "not-a-level".to_string(),
            ..LoggingConfig::default()
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

    // ---------------------------------------------------------------
    // S4 (#672): rule schedules, `pause_polling`, manual-status texts.
    // ---------------------------------------------------------------

    /// Pre-4.7 files keep loading: every new field carries a serde default, and
    /// the defaults describe the 4.6 behaviour (a rule with no schedule applies
    /// every day, quiet hours do not pause polling, and the two status texts
    /// render what 4.6 posted).
    #[test]
    fn test_rule_schedule_additions_default_on_old_files() {
        let cfg: AppConfig = serde_json::from_str(
            r#"{
                "teams": {},
                "status_rules": {
                    "quiet_hours": [{"enabled": true}],
                    "track_rules": [{"enabled": true, "artist_substring": "lofi"}]
                }
            }"#,
        )
        .expect("a pre-4.7 document must still parse");

        let rule = &cfg.status_rules.track_rules[0];
        assert!(rule.days.is_empty(), "no weekday set means every day");
        assert_eq!(rule.start_minutes, 0);
        assert_eq!(rule.end_minutes, TRACK_RULE_DAY_MINUTES);
        assert!(!cfg.status_rules.quiet_hours[0].pause_polling);
        assert_eq!(cfg.teams.paused_status_format, "Paused");
        assert_eq!(
            cfg.teams.stopped_status_format,
            "Nothing playing on Spotify"
        );

        // The serde defaults and the hand-written `Default` impls agree, so a
        // fixture built with `..Default::default()` describes the same rule a
        // config file missing the fields does.
        assert_eq!(rule.days, TrackRuleEntry::default().days);
        assert_eq!(rule.start_minutes, TrackRuleEntry::default().start_minutes);
        assert_eq!(rule.end_minutes, TrackRuleEntry::default().end_minutes);
        assert!(!QuietHoursEntry::default().pause_polling);
    }

    /// The new fields round-trip through serde — load, save, load again.
    #[test]
    fn test_rule_schedule_additions_round_trip() {
        let cfg: AppConfig = serde_json::from_str(
            r#"{
                "teams": {
                    "paused_status_format": "Back in 5",
                    "stopped_status_format": "Idle"
                },
                "status_rules": {
                    "quiet_hours": [{"enabled": true, "pause_polling": true}],
                    "track_rules": [{
                        "enabled": true,
                        "days": [1, 3],
                        "start_minutes": 480,
                        "end_minutes": 1020
                    }]
                }
            }"#,
        )
        .expect("must parse");
        assert!(cfg.status_rules.quiet_hours[0].pause_polling);
        assert_eq!(cfg.status_rules.track_rules[0].days, vec![1, 3]);
        assert_eq!(cfg.status_rules.track_rules[0].start_minutes, 480);
        assert_eq!(cfg.status_rules.track_rules[0].end_minutes, 1020);

        let json = serde_json::to_string(&cfg).expect("must serialize");
        let back: AppConfig = serde_json::from_str(&json).expect("must re-parse");
        assert_eq!(back.teams.paused_status_format, "Back in 5");
        assert_eq!(back.teams.stopped_status_format, "Idle");
        assert!(back.status_rules.quiet_hours[0].pause_polling);
        assert_eq!(back.status_rules.track_rules[0].days, vec![1, 3]);
        assert_eq!(back.status_rules.track_rules[0].start_minutes, 480);
        assert_eq!(back.status_rules.track_rules[0].end_minutes, 1020);
    }

    /// `clamp_rules` normalizes the schedule too: the window is bounded to
    /// `0..=1440` and the weekday set to the documented ISO range `1..=7`,
    /// deduplicated — on load and on every save.
    #[test]
    fn test_clamp_rules_normalizes_the_track_rule_window() {
        let mut rules = StatusRulesConfig {
            quiet_hours: Vec::new(),
            track_rules: vec![
                TrackRuleEntry {
                    start_minutes: 5000,
                    end_minutes: 9000,
                    days: vec![0, 7, 1, 9, 1, 8],
                    ..TrackRuleEntry::default()
                },
                TrackRuleEntry {
                    start_minutes: 480,
                    end_minutes: 1020,
                    days: vec![6, 7],
                    ..TrackRuleEntry::default()
                },
            ],
        };
        clamp_rules(&mut rules);
        assert_eq!(
            rules.track_rules[0].start_minutes,
            TRACK_RULE_DAY_MINUTES - 1,
            "a start of 1440 is unreachable, so it clamps to the last minute"
        );
        assert_eq!(rules.track_rules[0].end_minutes, TRACK_RULE_DAY_MINUTES);
        assert_eq!(
            rules.track_rules[0].days,
            vec![1, 7],
            "weekday bytes outside 1..=7 are dropped and duplicates collapse"
        );
        assert_eq!(rules.track_rules[1].start_minutes, 480);
        assert_eq!(rules.track_rules[1].end_minutes, 1020);
        assert_eq!(rules.track_rules[1].days, vec![6, 7]);

        // What is persisted is the normalized rule, not the raw file.
        let json = serde_json::to_string_pretty(&clamped_config(&AppConfig {
            status_rules: rules,
            ..AppConfig::default()
        }))
        .expect("must serialize");
        let back: AppConfig = serde_json::from_str(&json).expect("must re-parse");
        assert_eq!(back.status_rules.track_rules[0].days, vec![1, 7]);
        assert_eq!(
            back.status_rules.track_rules[0].end_minutes,
            TRACK_RULE_DAY_MINUTES
        );
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

    /// S4 (issue #672): the two manual-status texts are status lines, so
    /// `clamp_teams` bounds them like a rule's replacement text — and an EMPTY
    /// text is left empty (the renderer reads that as "use the default").
    #[test]
    fn test_clamp_teams_bounds_the_manual_status_texts() {
        let mut teams = TeamsConfig {
            paused_status_format: "ü".repeat(200),
            stopped_status_format: "z".repeat(200),
            ..TeamsConfig::default()
        };
        clamp_teams(&mut teams);
        assert_eq!(
            teams.paused_status_format.chars().count(),
            MAX_RULE_STATUS_CHARS
        );
        assert_eq!(
            teams.stopped_status_format.chars().count(),
            MAX_RULE_STATUS_CHARS
        );
        // Char-boundary safe (the truncation above would have panicked
        // otherwise) and short texts are untouched.
        let mut short = TeamsConfig {
            paused_status_format: "Back in 5".to_string(),
            stopped_status_format: String::new(),
            ..TeamsConfig::default()
        };
        clamp_teams(&mut short);
        assert_eq!(short.paused_status_format, "Back in 5");
        assert!(short.stopped_status_format.is_empty());
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

    // ---------------------------------------------------------------
    // 4.7.0 (S5): log rotation defaults/clamps + config export/import
    // ---------------------------------------------------------------

    /// The rotation fields are additive and `#[serde(default)]`, so a
    /// `config.json` written before 4.7.0 has neither key and must load with
    /// the shipped defaults — a `0` here would build `KeepSome(0)` and make
    /// the plugin's archive pass underflow.
    #[test]
    fn pre_4_7_logging_config_gets_rotation_defaults() {
        let legacy: LoggingConfig = serde_json::from_str(r#"{"enabled":true,"log_level":"Debug"}"#)
            .expect("a logging section written before 4.7.0 must still deserialize");
        assert_eq!(legacy.max_file_size_mb, 10);
        assert_eq!(legacy.keep_files, 3);
        assert_eq!(LoggingConfig::default().max_file_size_mb, 10);
        assert_eq!(LoggingConfig::default().keep_files, 3);
    }

    /// Out-of-band rotation values — typed past the number inputs' `min`/
    /// `max`, or hand-edited — clamp to the band the UI offers.
    #[test]
    fn clamp_logging_bounds_rotation_fields() {
        let bounded = |max_file_size_mb: u64, keep_files: u32| {
            let mut logging = LoggingConfig {
                enabled: true,
                log_level: "Info".into(),
                max_file_size_mb,
                keep_files,
            };
            clamp_logging(&mut logging);
            (logging.max_file_size_mb, logging.keep_files)
        };
        assert_eq!(bounded(0, 0), (1, 1), "below the floor");
        assert_eq!(bounded(9000, 99), (500, 20), "above the ceiling");
        assert_eq!(bounded(25, 7), (25, 7), "inside the band is untouched");
    }

    /// `clamped_config` is what `save_config` writes, so an out-of-band
    /// rotation value must never reach disk.
    #[test]
    fn clamped_config_bounds_logging() {
        let mut config = AppConfig::default();
        config.logging.keep_files = 0;
        config.logging.max_file_size_mb = 10_000;
        let clamped = clamped_config(&config);
        assert_eq!(clamped.logging.keep_files, 1);
        assert_eq!(clamped.logging.max_file_size_mb, 500);
    }

    /// The export file name carries the app version and a sortable UTC stamp.
    #[test]
    fn export_file_name_is_versioned_and_timestamped() {
        let at = chrono::DateTime::parse_from_rfc3339("2026-09-17T04:05:06Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        assert_eq!(
            export_file_name("4.7.0", at),
            "presencejam-config-4.7.0-20260917-040506.json"
        );
    }

    /// An export must never carry the plaintext secret, wherever a legacy or
    /// hand-edited file parked it — including the unknown-key retention map,
    /// which the typed schema knows nothing about.
    #[test]
    fn export_document_strips_client_secret() {
        let mut config = AppConfig::default();
        config.spotify.client_id = "abc".into();
        config
            .extra
            .insert("client_secret".into(), serde_json::json!("SEKRIT"));

        let json = export_document(&config).unwrap();

        assert!(!json.contains("SEKRIT"), "exported document: {json}");
        assert!(!json.contains("client_secret"), "exported document: {json}");
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["spotify"]["client_id"], "abc");
    }

    /// A plaintext `client_secret` is refused, and the refusal names the path
    /// it found: importing it "successfully" while silently dropping the
    /// credential would look like a completed sign-in.
    #[test]
    fn import_rejects_document_with_client_secret() {
        let err = prepare_import(r#"{"spotify":{"client_id":"a","client_secret":"SEKRIT"}}"#)
            .expect_err("a plaintext client_secret must be refused");
        assert!(
            err.contains("spotify.client_secret"),
            "refusal must name the offending path: {err}"
        );

        // The reject is on the key, not on the value: an explicit null is
        // still a client_secret field.
        assert!(prepare_import(r#"{"spotify":{"client_secret":null}}"#).is_err());

        // Nested inside the unknown-key retention map is the same answer.
        assert!(prepare_import(r#"{"schema_version":2,"client_secret":"SEKRIT"}"#).is_err());

        // A document without the key is not falsely rejected.
        assert!(prepare_import(r#"{"spotify":{"client_id":"a"}}"#).is_ok());
    }

    /// A document that is not a JSON object never reaches the schema parser.
    #[test]
    fn import_rejects_non_object_document() {
        assert!(prepare_import("[1, 2, 3]").is_err());
        assert!(prepare_import("not json at all").is_err());
    }

    /// Export → import keeps every value, including unknown top-level keys,
    /// and re-importing the written document is stable.
    #[test]
    fn export_import_round_trips_a_config() {
        let mut config = AppConfig::default();
        config.spotify.client_id = "abc123".into();
        config.teams.status_format = "🎧 {track}".into();
        config.polling.default_interval_seconds = 45;
        config.logging.max_file_size_mb = 25;
        config.logging.keep_files = 7;
        config
            .extra
            .insert("future_key".into(), serde_json::json!({"nested": [1, 2]}));

        let exported = export_document(&config).unwrap();
        let imported = prepare_import(&exported).unwrap();

        assert_eq!(imported.config.spotify.client_id, "abc123");
        assert_eq!(imported.config.teams.status_format, "🎧 {track}");
        assert_eq!(imported.config.polling.default_interval_seconds, 45);
        assert_eq!(imported.config.logging.max_file_size_mb, 25);
        assert_eq!(imported.config.logging.keep_files, 7);
        assert_eq!(
            imported.config.extra.get("future_key"),
            Some(&serde_json::json!({"nested": [1, 2]}))
        );

        let again = prepare_import(&imported.document).unwrap();
        assert_eq!(
            again.document, imported.document,
            "the written document must be a fixed point of prepare_import"
        );
    }

    /// The whole pipeline the two commands run, minus the file dialogs: export
    /// writes the document with the same crash-safe helper `save_config` uses,
    /// and importing that file lands the same settings with no secret in it.
    #[test]
    fn exported_file_imports_back_to_the_same_settings() {
        let dir = std::env::temp_dir().join(format!(
            "pj-test-export-import-{}-{}",
            std::process::id(),
            chrono_like_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let export_path = dir.join(export_file_name(
            "4.7.0",
            chrono::DateTime::parse_from_rfc3339("2026-09-17T04:05:06Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        ));
        let live_path = dir.join("config.json");

        let mut config = AppConfig::default();
        config.spotify.client_id = "abc123".into();
        config.teams.status_format = "🎧 {track}".into();
        config.logging.max_file_size_mb = 25;
        config.logging.keep_files = 7;
        config
            .extra
            .insert("client_secret".into(), serde_json::json!("SEKRIT"));

        // export_config: serialize, then write through the shared helper.
        atomic_write_json(&export_path, &export_document(&config).unwrap()).unwrap();
        assert_eq!(
            export_path.file_name().unwrap().to_string_lossy(),
            "presencejam-config-4.7.0-20260917-040506.json"
        );
        let exported = std::fs::read_to_string(&export_path).unwrap();
        assert!(!exported.contains("SEKRIT"));

        // import_config: read the chosen file, validate, confirm, replace.
        let imported = import_config_document(&exported, &live_path, || true)
            .unwrap()
            .expect("the overwrite was confirmed");

        assert_eq!(imported.spotify.client_id, "abc123");
        assert_eq!(imported.teams.status_format, "🎧 {track}");
        assert_eq!(imported.logging.max_file_size_mb, 25);
        assert_eq!(imported.logging.keep_files, 7);
        assert_eq!(
            imported.extra.get("client_secret"),
            None,
            "the stripped key must not come back through an import"
        );
        assert!(!std::fs::read_to_string(&live_path)
            .unwrap()
            .contains("SEKRIT"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Declining the overwrite confirmation is a clean no-op: the live file is
    /// left byte-identical and no `.bak` appears. This is the cancel path of the
    /// import confirmation, and it is the reason the prompt is a parameter
    /// rather than a dialog buried inside the command — the decision is
    /// testable against real files without a desktop.
    #[test]
    fn declined_import_touches_nothing() {
        let dir = std::env::temp_dir().join(format!(
            "pj-test-import-decline-{}-{}",
            std::process::id(),
            chrono_like_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let previous = r#"{"spotify":{"client_id":"LIVE"},"logging":{"keep_files":7}}"#;
        std::fs::write(&path, previous).unwrap();

        let outcome =
            import_config_document(r#"{"spotify":{"client_id":"NEW"}}"#, &path, || false).unwrap();

        assert!(
            outcome.is_none(),
            "declining must report 'nothing happened'"
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            previous,
            "the live config must be byte-identical after a decline"
        );
        assert!(
            !dir.join("config.json.bak").exists(),
            "a decline must not quarantine the file it did not replace"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Importing over an existing config moves it to `<path>.bak` (the same
    /// backup the corrupt-file quarantine uses) and writes the imported
    /// document clamped, not raw.
    #[test]
    fn import_quarantines_the_outgoing_config() {
        let dir = std::env::temp_dir().join(format!(
            "pj-test-import-{}-{}",
            std::process::id(),
            chrono_like_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let previous = r#"{"spotify":{"client_id":"OLD"}}"#;
        std::fs::write(&path, previous).unwrap();

        let imported = import_config_document(
            r#"{"spotify":{"client_id":"NEW"},"logging":{"keep_files":900}}"#,
            &path,
            || true,
        )
        .unwrap()
        .expect("the overwrite was confirmed");

        assert_eq!(
            std::fs::read_to_string(dir.join("config.json.bak")).unwrap(),
            previous,
            "the outgoing config must be preserved next to the new one"
        );
        let written: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written["spotify"]["client_id"], "NEW");
        assert_eq!(
            written["logging"]["keep_files"], 20,
            "an imported out-of-range value must land clamped on disk"
        );
        assert_eq!(imported.logging.keep_files, 20);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A refused import must not touch the live config or leave a `.bak`
    /// behind — refusal happens before anything is moved.
    #[test]
    fn rejected_import_leaves_the_config_untouched() {
        let dir = std::env::temp_dir().join(format!(
            "pj-test-import-reject-{}-{}",
            std::process::id(),
            chrono_like_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let previous = r#"{"spotify":{"client_id":"LIVE"}}"#;
        std::fs::write(&path, previous).unwrap();

        let asked = std::cell::Cell::new(false);
        assert!(import_config_document(
            r#"{"spotify":{"client_id":"NEW","client_secret":"SEKRIT"}}"#,
            &path,
            || {
                asked.set(true);
                true
            }
        )
        .is_err());
        assert!(
            !asked.get(),
            "a refused document must be rejected before the user is asked anything"
        );

        assert_eq!(std::fs::read_to_string(&path).unwrap(), previous);
        assert!(!dir.join("config.json.bak").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A fresh install has no `config.json`; an import must create one
    /// rather than fail on the missing backup step.
    #[test]
    fn import_into_a_missing_config_creates_it() {
        let dir = std::env::temp_dir().join(format!(
            "pj-test-import-fresh-{}-{}",
            std::process::id(),
            chrono_like_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");

        let asked = std::cell::Cell::new(false);
        import_config_document(r#"{"spotify":{"client_id":"FRESH"}}"#, &path, || {
            asked.set(true);
            true
        })
        .unwrap()
        .expect("a fresh install has nothing to decline");
        assert!(
            !asked.get(),
            "with no current file there is nothing to overwrite, so nothing to confirm"
        );

        let written: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written["spotify"]["client_id"], "FRESH");
        assert!(!dir.join("config.json.bak").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    // -----------------------------------------------------------------------
    // S9 (issue #677): snooze deadlines.
    //
    // The feature is judged on one thing above all: "until tomorrow" must mean
    // the next LOCAL midnight, so a snooze never silently stretches or shrinks
    // by the machine's UTC offset — nor by a DST transition it spans.
    // -----------------------------------------------------------------------

    /// A fixed-offset zone at `hours` east of UTC. `FixedOffset` is the
    /// timezone-typed stand-in for "the user's machine is here", which is what
    /// makes these boundary tests independent of the test runner's TZ.
    fn zone(hours: i32) -> chrono::FixedOffset {
        chrono::FixedOffset::east_opt(hours * 3600).expect("valid offset")
    }

    fn utc(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> chrono::DateTime<chrono::Utc> {
        chrono::NaiveDate::from_ymd_opt(y, mo, d)
            .unwrap()
            .and_hms_opt(h, mi, s)
            .unwrap()
            .and_utc()
    }

    /// "Until tomorrow" is the START of the next local calendar day, converted
    /// to UTC — never `now + 24 h`.
    ///
    /// Each row pins a boundary: a second before midnight (the old second must
    /// still land on tomorrow), exactly midnight (a full day, not zero) and a
    /// time where the UTC day and the local day differ, in both directions of
    /// offset.
    #[test]
    fn until_tomorrow_is_the_next_local_midnight() {
        // 23:59:59 in UTC+2 → one second later is tomorrow's midnight.
        let late = utc(2026, 1, 5, 21, 59, 59).with_timezone(&zone(2));
        let deadline = next_local_midnight_utc(late);
        assert_eq!(deadline, utc(2026, 1, 5, 22, 0, 0));
        assert_eq!(
            (deadline - late.with_timezone(&chrono::Utc)).num_seconds(),
            1
        );

        // Exactly local midnight: the deadline is the START of the FOLLOWING
        // day (a full day), never "today" (which would already be in the past).
        let midnight = utc(2026, 1, 5, 22, 0, 0).with_timezone(&zone(2));
        assert_eq!(next_local_midnight_utc(midnight), utc(2026, 1, 6, 22, 0, 0));

        // 00:05 in UTC−11: local date 2026-01-05, so the deadline is
        // 2026-01-06T00:00−11:00 = 11:00Z — ~23 h away in real time.
        let early = utc(2026, 1, 5, 11, 5, 0).with_timezone(&zone(-11));
        let far = next_local_midnight_utc(early);
        assert_eq!(far, utc(2026, 1, 6, 11, 0, 0));

        // The same instant seen from UTC+13 is 2026-01-06 00:05 locally, so the
        // deadline is the NEXT midnight — a timezone read of the same UTC value
        // must not reuse the previous day's boundary.
        let east = utc(2026, 1, 5, 11, 5, 0).with_timezone(&zone(13));
        assert_eq!(next_local_midnight_utc(east), utc(2026, 1, 6, 11, 0, 0));
    }

    /// An "until tomorrow" snooze is the local MIDNIGHT that follows `now`, in
    /// every offset — and nothing about `now`'s own wall clock leaks into it.
    ///
    /// Both halves are asserted per row because the second is what makes this
    /// test able to fail: the `(0, 24h]` duration bound alone does NOT kill a
    /// `now + 24 h` implementation (that returns exactly 24 h, which is inside
    /// the bound). The wall-clock assertion does — under `now + 24 h` the
    /// deadline's local time is `now`'s local time, e.g. 12:30, not 00:00.
    /// These rows sit at 23:59 / 00:01 / 12:30 precisely so that a same-wall-
    /// clock deadline is distinguishable from midnight in every row.
    ///
    /// Driven through `snooze_preset_deadline` (the table the tray actually
    /// calls) rather than `next_local_midnight_utc` directly, so the sweep also
    /// covers the DISPATCH arm: a mutant that replaced
    /// `SnoozePreset::UntilTomorrow` with `now + 24 h` used to survive this test
    /// because the test bypassed the arm it lived in.
    #[test]
    fn until_tomorrow_never_shifts_by_the_offset() {
        use chrono::Timelike;
        for hours in [-12, -11, -5, -1, 0, 1, 2, 5, 12, 13, 14] {
            let tz = zone(hours);
            for (h, mi) in [(0, 0), (0, 1), (12, 30), (23, 59)] {
                let now = utc(2026, 3, 4, h, mi, 0).with_timezone(&tz);
                let now_utc = now.with_timezone(&chrono::Utc);
                let deadline = snooze_preset_deadline(SnoozePreset::UntilTomorrow, now_utc, now);
                let secs = (deadline - now_utc).num_seconds();
                assert!(
                    secs > 0 && secs <= 24 * 3600,
                    "offset {}h at {:02}:{:02} produced a {} s snooze",
                    hours,
                    h,
                    mi,
                    secs
                );

                // The boundary itself: tomorrow's local date, at exactly local
                // midnight. `FixedOffset` has no DST, so midnight always exists.
                let local = deadline.with_timezone(&tz);
                assert_eq!(
                    (local.hour(), local.minute(), local.second()),
                    (0, 0, 0),
                    "offset {}h at {:02}:{:02} did not land on local midnight \
                     (a `now + 24 h` deadline would keep the current wall clock)",
                    hours,
                    h,
                    mi
                );
                assert_eq!(
                    local.date_naive(),
                    now.date_naive() + chrono::Days::new(1),
                    "offset {}h at {:02}:{:02} did not land on tomorrow",
                    hours,
                    h,
                    mi
                );
            }
        }
    }

    /// A DST transition inside the window changes the real duration, never the
    /// wall-clock boundary (issue #677): `next_local_midnight_utc` reads the
    /// zone's rules rather than adding 24 h.
    #[test]
    fn until_tomorrow_keeps_the_calendar_boundary_across_dst() {
        // The deadline is the next LOCAL calendar day whatever the zone's
        // offset does in between. `chrono-tz` is not a dependency, so this runs
        // against the machine's own `Local` and asserts only what holds in
        // EVERY zone — including one whose spring-forward swallows local
        // midnight (America/Santiago, Asia/Beirut in some years), where the
        // correct answer is the first instant the new day exists (01:00), not
        // 00:00. Asserting `00:00:00` outright would therefore fail on those
        // machines while passing on a UTC CI runner.
        let now = chrono::Local::now();
        let deadline = next_local_midnight_utc(now);
        let local = deadline.with_timezone(&chrono::Local);
        use chrono::Timelike;

        assert!(
            deadline > now.with_timezone(&chrono::Utc),
            "a deadline in the past would make the snooze end instantly"
        );
        assert_eq!(
            local.date_naive(),
            now.date_naive() + chrono::Days::new(1),
            "the deadline must be TOMORROW's local date"
        );
        if local.hour() == 0 {
            assert_eq!(
                (local.minute(), local.second()),
                (0, 0),
                "a midnight that exists is exactly 00:00:00"
            );
        } else {
            // A gap swallowed midnight: the deadline is the first instant of
            // the new day that exists, which is what `resolve_local_forward`
            // walks to. It must still be well inside the new day and short of
            // its end.
            assert_eq!(
                (local.hour(), local.minute(), local.second()),
                (1, 0, 0),
                "a swallowed midnight resolves to the first existing instant"
            );
        }
        assert!(
            local.date_naive() == now.date_naive() + chrono::Days::new(1),
            "the boundary is the new day's start, never the current day's"
        );
    }

    /// A spring-forward that swallows local midnight (some zones transition at
    /// 00:00) leaves no wall clock to resolve. The walk must land on the first
    /// instant that exists rather than panicking or producing a past deadline.
    ///
    /// `FixedOffset` cannot express a gap, so the branch is pinned through a
    /// minimal stateful zone: offset +1 h before the gap, +2 h after it, with
    /// the local 00:00 of the target day inside the transition.
    #[test]
    fn resolve_local_forward_walks_out_of_a_dst_gap() {
        /// +1 h until 23:00 UTC on 2026-03-28, +2 h from then on (a zone whose
        /// spring-forward is at local midnight).
        #[derive(Debug, Clone, Copy)]
        struct GapZone;
        impl chrono::TimeZone for GapZone {
            type Offset = chrono::FixedOffset;
            fn from_offset(_: &Self::Offset) -> Self {
                GapZone
            }
            fn offset_from_local_date(
                &self,
                _: &chrono::NaiveDate,
            ) -> chrono::LocalResult<Self::Offset> {
                // Never used by the code under test; a single offset keeps the
                // provided `ymd` helpers well-defined.
                chrono::LocalResult::Single(zone(1))
            }
            fn offset_from_local_datetime(
                &self,
                local: &chrono::NaiveDateTime,
            ) -> chrono::LocalResult<Self::Offset> {
                // The gap: local wall clock 2026-03-29T00:00..00:59 never
                // happens.
                let gap_start = chrono::NaiveDate::from_ymd_opt(2026, 3, 29)
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap();
                let gap_end = gap_start + chrono::TimeDelta::minutes(60);
                if *local >= gap_start && *local < gap_end {
                    return chrono::LocalResult::None;
                }
                let offset = if *local < gap_start { zone(1) } else { zone(2) };
                chrono::LocalResult::Single(offset)
            }
            fn offset_from_utc_date(&self, utc: &chrono::NaiveDate) -> Self::Offset {
                if *utc < chrono::NaiveDate::from_ymd_opt(2026, 3, 28).unwrap() {
                    zone(1)
                } else {
                    zone(2)
                }
            }
            fn offset_from_utc_datetime(&self, utc: &chrono::NaiveDateTime) -> Self::Offset {
                if utc.date() < chrono::NaiveDate::from_ymd_opt(2026, 3, 29).unwrap() {
                    zone(1)
                } else {
                    zone(2)
                }
            }
        }

        // 2026-03-28T23:30 local (+1 h) → tomorrow's midnight is inside the gap.
        let now = chrono::NaiveDate::from_ymd_opt(2026, 3, 28)
            .unwrap()
            .and_hms_opt(23, 30, 0)
            .unwrap();
        let deadline = next_local_midnight_utc(
            chrono::TimeZone::from_local_datetime(&GapZone, &now)
                .single()
                .expect("23:30 is not in the gap"),
        );
        // Local 00:00..00:59 on the 29th never happens, so the first instant of
        // the new day is local 01:00 at +2 h — which is 23:00Z on the 28th, i.e.
        // 30 minutes after this `now`. Not panicking, and not a deadline in the
        // past, is the property that matters: the snooze still ends just after
        // the calendar day turns.
        assert_eq!(deadline, utc(2026, 3, 28, 23, 0, 0));
        assert!(deadline > utc(2026, 3, 28, 22, 30, 0), "never in the past");
    }

    /// An ambiguous local midnight (a fall-back repeats it) resolves to the
    /// EARLIER instant: the conservative, shorter snooze.
    #[test]
    fn resolve_local_forward_prefers_the_earlier_ambiguous_instant() {
        #[derive(Debug, Clone, Copy)]
        struct FallBackAtMidnight;
        impl chrono::TimeZone for FallBackAtMidnight {
            type Offset = chrono::FixedOffset;
            fn from_offset(_: &Self::Offset) -> Self {
                FallBackAtMidnight
            }
            fn offset_from_local_date(
                &self,
                _: &chrono::NaiveDate,
            ) -> chrono::LocalResult<Self::Offset> {
                chrono::LocalResult::Single(zone(2))
            }
            fn offset_from_local_datetime(
                &self,
                local: &chrono::NaiveDateTime,
            ) -> chrono::LocalResult<Self::Offset> {
                let ambiguous = chrono::NaiveDate::from_ymd_opt(2026, 10, 25)
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap();
                if *local == ambiguous {
                    return chrono::LocalResult::Ambiguous(zone(1), zone(2));
                }
                chrono::LocalResult::Single(zone(2))
            }
            fn offset_from_utc_date(&self, _: &chrono::NaiveDate) -> Self::Offset {
                zone(2)
            }
            fn offset_from_utc_datetime(&self, _: &chrono::NaiveDateTime) -> Self::Offset {
                zone(2)
            }
        }

        let resolved = resolve_local_forward(
            &FallBackAtMidnight,
            chrono::NaiveDate::from_ymd_opt(2026, 10, 25)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
        );
        assert_eq!(
            resolved,
            utc(2026, 10, 24, 23, 0, 0),
            "the earlier of the two midnights is the shorter, safer snooze"
        );
    }

    /// The preset table: the two duration presets are pure instant offsets (no
    /// timezone can move them), and "until tomorrow" is the local boundary.
    #[test]
    fn snooze_presets_map_to_deadlines() {
        let now_utc = utc(2026, 6, 1, 9, 0, 0);
        let now_local = now_utc.with_timezone(&zone(2));
        assert_eq!(
            snooze_preset_deadline(SnoozePreset::ThirtyMinutes, now_utc, now_local),
            utc(2026, 6, 1, 9, 30, 0)
        );
        assert_eq!(
            snooze_preset_deadline(SnoozePreset::OneHour, now_utc, now_local),
            utc(2026, 6, 1, 10, 0, 0)
        );
        assert_eq!(
            snooze_preset_deadline(SnoozePreset::UntilTomorrow, now_utc, now_local),
            utc(2026, 6, 1, 22, 0, 0),
            "11:00 local → the midnight that follows it, in UTC"
        );
    }

    /// The stored spelling round-trips through the parser the clamp uses, and
    /// the persisted value is UTC regardless of what the machine reads.
    #[test]
    fn snooze_deadline_round_trips_and_survives_an_offset_spelling() {
        let deadline = utc(2026, 9, 17, 13, 45, 0);
        let stored = snooze_store_form(deadline);
        assert_eq!(stored, "2026-09-17T13:45:00Z");
        assert_eq!(
            snooze_deadline(&stored, deadline - chrono::TimeDelta::seconds(1)),
            Some(deadline)
        );

        // A value re-serialized by another tool as +02:00 denotes the same
        // instant and must not be discarded.
        assert_eq!(
            snooze_deadline(
                "2026-09-17T15:45:00+02:00",
                deadline - chrono::TimeDelta::seconds(1)
            ),
            Some(deadline),
            "an offset-shifted spelling of the same instant is still the deadline"
        );
        // Whitespace from a hand edit is tolerated.
        assert_eq!(
            snooze_deadline(
                "  2026-09-17T13:45:00Z ",
                deadline - chrono::TimeDelta::seconds(1)
            ),
            Some(deadline)
        );
    }

    /// The boundaries of "active": the instant of the deadline itself is
    /// already over, and anything unparsable is never a snooze.
    #[test]
    fn snooze_deadline_is_exclusive_at_its_own_instant() {
        let deadline = utc(2026, 9, 17, 13, 45, 0);
        let stored = snooze_store_form(deadline);
        assert!(snooze_deadline(&stored, deadline - chrono::TimeDelta::seconds(1)).is_some());
        assert!(
            snooze_deadline(&stored, deadline).is_none(),
            "at the deadline the snooze is over — otherwise it would never end"
        );
        assert!(snooze_deadline(&stored, deadline + chrono::TimeDelta::seconds(1)).is_none());
        assert!(snooze_deadline("", deadline).is_none());
        assert!(snooze_deadline("not a timestamp", deadline).is_none());
        assert!(snooze_deadline("2026-09-17", deadline).is_none());
    }

    /// The startup clamp: a deadline that passed while the app was closed is
    /// cleared (and reported), a future one and an absent one are untouched.
    #[test]
    fn clamp_snooze_clears_only_expired_deadlines() {
        let now = utc(2026, 9, 17, 13, 45, 0);
        let with = |value: Option<&str>| AppConfig {
            snooze_until: value.map(str::to_string),
            ..AppConfig::default()
        };

        let mut future = with(Some("2026-09-17T14:00:00Z"));
        assert!(!clamp_snooze(&mut future, now));
        assert_eq!(future.snooze_until.as_deref(), Some("2026-09-17T14:00:00Z"));

        let mut expired = with(Some("2026-09-17T13:00:00Z"));
        assert!(
            clamp_snooze(&mut expired, now),
            "an expired snooze is dropped"
        );
        assert_eq!(expired.snooze_until, None);

        let mut garbage = with(Some("yesterday-ish"));
        assert!(
            clamp_snooze(&mut garbage, now),
            "an unparsable value is dropped"
        );
        assert_eq!(garbage.snooze_until, None);

        let mut absent = with(None);
        assert!(!clamp_snooze(&mut absent, now));

        // Idempotent: a second pass finds nothing to clear, which is what keeps
        // the one-line log from repeating on every load and save.
        assert!(!clamp_snooze(&mut expired, now));

        // The reader's twin reports the same states WITHOUT mutating, which is
        // what lets `load_config` log the expiry while leaving the stored value
        // visible to the writers that can correct `config.json`.
        let reported = with(Some("2026-09-17T13:00:00Z"));
        assert!(snooze_expired_deadline(&reported, now));
        assert_eq!(
            reported.snooze_until.as_deref(),
            Some("2026-09-17T13:00:00Z"),
            "the reader must not touch the field"
        );
        assert!(snooze_expired_deadline(&with(Some("yesterday-ish")), now));
        assert!(!snooze_expired_deadline(
            &with(Some("2026-09-17T14:00:00Z")),
            now
        ));
        assert!(
            !snooze_expired_deadline(&with(None), now),
            "an absent field is 'not snoozed', not something to clean or report"
        );
        // …and the writer's verdict agrees with the reader's on every row, so a
        // value the load path reports is always one a save will remove.
        for value in [
            Some("2026-09-17T13:00:00Z"),
            Some("yesterday-ish"),
            Some("2026-09-17T14:00:00Z"),
            None,
        ] {
            let mut cfg = with(value);
            let cleared = clamp_snooze(&mut cfg, now);
            assert_eq!(
                cleared,
                snooze_expired_deadline(&with(value), now),
                "clamp and report must agree for {:?}",
                value
            );
        }
    }

    /// The countdown rounds UP and never reads zero while the snooze is live —
    /// the difference between "30 min left" the moment it is set and "29".
    #[test]
    fn snooze_minutes_left_rounds_up() {
        assert_eq!(snooze_minutes_left(30 * 60), 30);
        assert_eq!(snooze_minutes_left(30 * 60 - 1), 30);
        assert_eq!(snooze_minutes_left(29 * 60), 29);
        assert_eq!(snooze_minutes_left(60), 1);
        assert_eq!(snooze_minutes_left(1), 1);
        // Never zero, never negative — and bounded for an absurd stored value.
        assert_eq!(snooze_minutes_left(0), 1);
        assert_eq!(snooze_minutes_left(-5), 1);
        assert_eq!(snooze_minutes_left(i64::MAX), 24 * 60);
    }

    /// `snooze_status` is the single read the tray and the driver share: it
    /// reports the remaining seconds and refuses everything else.
    #[test]
    fn snooze_status_reports_only_a_live_deadline() {
        let now = utc(2026, 9, 17, 13, 45, 0);
        let cfg = |value: Option<&str>| AppConfig {
            snooze_until: value.map(str::to_string),
            ..AppConfig::default()
        };
        let status = snooze_status(&cfg(Some("2026-09-17T14:15:00Z")), now).expect("live snooze");
        assert_eq!(status.remaining_seconds, 30 * 60);
        assert_eq!(snooze_store_form(status.deadline), "2026-09-17T14:15:00Z");

        assert!(snooze_status(&cfg(Some("2026-09-17T13:45:00Z")), now).is_none());
        assert!(snooze_status(&cfg(Some("nope")), now).is_none());
        assert!(snooze_status(&cfg(None), now).is_none());
    }

    /// A pre-4.7 config file has no `snooze_until` key and must keep loading —
    /// the field is additive with a serde default, and the default is "not
    /// snoozed".
    #[test]
    fn snooze_until_is_additive_for_pre_4_7_configs() {
        let cfg: AppConfig =
            serde_json::from_str(r#"{"spotify":{"client_id":"X"},"schema_version":2}"#).unwrap();
        assert_eq!(cfg.snooze_until, None);
        assert!(snooze_status(&cfg, chrono::Utc::now()).is_none());
        // …and it serializes back out, so a snooze survives a restart.
        let json = serde_json::to_value(AppConfig {
            snooze_until: Some("2026-09-17T14:15:00Z".to_string()),
            ..AppConfig::default()
        })
        .unwrap();
        assert_eq!(json["snooze_until"], "2026-09-17T14:15:00Z");
    }

    /// Issue #676: every config file written before 4.7.0 lacks the
    /// `shortcuts` section entirely, and it must load with the documented
    /// defaults rather than with no global shortcuts at all.
    #[test]
    fn pre_4_7_config_json_still_loads_with_default_shortcuts() {
        let legacy: AppConfig = serde_json::from_str(
            r#"{"spotify":{"client_id":"abc"},"teams":{"status_format":"x"},"autostart":true}"#,
        )
        .expect("a config.json written before #676 must still deserialize");
        assert_eq!(legacy.shortcuts, ShortcutsConfig::default());
        assert_eq!(
            legacy.shortcuts.toggle_playback.as_deref(),
            Some(DEFAULT_TOGGLE_PLAYBACK_SHORTCUT)
        );
        assert_eq!(
            legacy.shortcuts.toggle_sync.as_deref(),
            Some(DEFAULT_TOGGLE_SYNC_SHORTCUT)
        );
        // The rest of the file is untouched by the additive section.
        assert_eq!(legacy.spotify.client_id, "abc");
        assert!(legacy.autostart);
    }

    /// An *absent* key takes the default, an explicit `null` is the user's
    /// deliberate unbinding. Collapsing the two would make a user who cleared
    /// one row find it re-bound at the next launch.
    #[test]
    fn absent_shortcut_takes_the_default_and_null_unbinds() {
        let absent: ShortcutsConfig = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(absent, ShortcutsConfig::default());

        let nulled: ShortcutsConfig = serde_json::from_str(r#"{"toggle_playback":null}"#).unwrap();
        assert_eq!(nulled.toggle_playback, None);
        assert_eq!(
            nulled.toggle_sync.as_deref(),
            Some(DEFAULT_TOGGLE_SYNC_SHORTCUT),
            "clearing one row must not clear the other"
        );
    }

    /// The bindings round-trip through JSON unchanged, blank string included
    /// (the registration planner, not the loader, decides that blank means
    /// "unbound" — a loader that silently rewrote it would make the Settings
    /// field lie about what is stored).
    #[test]
    fn shortcut_bindings_round_trip_through_json() {
        let cfg = ShortcutsConfig {
            toggle_playback: Some("CmdOrCtrl+Shift+P".to_string()),
            toggle_sync: Some("  ".to_string()),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert_eq!(serde_json::from_str::<ShortcutsConfig>(&json).unwrap(), cfg);
    }

    /// Issue #678: a `config.json` written before 4.7.0 has no `updates`
    /// key, and must keep loading on the stable channel (additive serde
    /// default — the schema version does not move for it).
    #[test]
    fn test_updates_channel_defaults_to_stable_for_pre_4_7_configs() {
        let cfg: AppConfig = serde_json::from_str("{}").expect("empty object must parse");
        assert_eq!(cfg.updates.channel, UpdateChannel::Stable);
        assert_eq!(AppConfig::default().updates.channel, UpdateChannel::Stable);
    }

    /// The persisted spelling is what the Settings picker round-trips across
    /// a relaunch, so both directions must hold the lowercase wire form.
    #[test]
    fn test_update_channel_wire_spelling() {
        for (channel, wire) in [
            (UpdateChannel::Stable, "\"stable\""),
            (UpdateChannel::Beta, "\"beta\""),
        ] {
            assert_eq!(serde_json::to_string(&channel).unwrap(), wire);
            assert_eq!(
                serde_json::from_str::<UpdateChannel>(wire).unwrap(),
                channel
            );
        }
        let stored: AppConfig =
            serde_json::from_str(r#"{"updates": {"channel": "beta"}}"#).expect("must parse");
        assert_eq!(stored.updates.channel, UpdateChannel::Beta);
    }

    // The lenient-read tests below assert a `log::warn!`, and no logger is
    // installed in a unit-test process by default. Capturing is process-wide,
    // so they serialise on the same lock the quarantine tests use.
    static LOG_LINES: parking_lot::Mutex<Vec<String>> = parking_lot::Mutex::new(Vec::new());
    static LOGGER: std::sync::Once = std::sync::Once::new();

    struct CapturingLogger;

    impl log::Log for CapturingLogger {
        fn enabled(&self, _metadata: &log::Metadata) -> bool {
            true
        }
        fn log(&self, record: &log::Record) {
            LOG_LINES.lock().push(record.args().to_string());
        }
        fn flush(&self) {}
    }

    /// The channel a value reads as, and the warning to log for it.
    ///
    /// Issue #678: an unknown spelling reads as `stable` and names the
    /// offending value, instead of failing the whole document.
    #[test]
    fn test_unknown_update_channel_names_the_offending_value() {
        let (channel, warning) = lenient_update_channel(&serde_json::json!("nightly"));
        assert_eq!(channel, UpdateChannel::Stable);
        let warning = warning.expect("an unrecognised value must warn");
        assert!(
            warning.contains("nightly"),
            "the warning must name the offending value: {warning}"
        );
        assert!(
            !warning.contains("[CFG]"),
            "the [CFG] tag belongs at the log site, not in the message: {warning}"
        );

        // The known spellings take no fallback path and never warn.
        for (raw, expected) in [
            ("stable", UpdateChannel::Stable),
            ("beta", UpdateChannel::Beta),
        ] {
            let (channel, warning) = lenient_update_channel(&serde_json::json!(raw));
            assert_eq!(channel, expected);
            assert!(warning.is_none(), "{raw} must not warn");
        }
    }

    /// Issue #678: the point of the lenient read — a document carrying a
    /// channel spelling this binary does not know still LOADS, with the rest of
    /// the config intact, and says so in the log.
    ///
    /// Fails before the lenient read: serde rejects the whole document, which
    /// `load_config` answers by quarantining `config.json` to `config.json.bak`
    /// and booting on defaults — so `from_str` returned `Err`, every other
    /// setting was lost, and nothing was logged.
    #[test]
    fn test_unknown_update_channel_keeps_the_rest_of_the_config_and_warns() {
        let _guard = QUARANTINE_TEST_LOCK.lock();
        LOGGER.call_once(|| {
            // Best-effort: another test may have installed a logger first.
            let _ = log::set_boxed_logger(Box::new(CapturingLogger));
            log::set_max_level(log::LevelFilter::Warn);
        });
        LOG_LINES.lock().clear();

        let cfg: AppConfig = serde_json::from_str(
            r#"{"autostart": true, "updates": {"channel": "nightly"},
                "teams": {"status_format": "🎧 {track}"}}"#,
        )
        .expect("an unrecognised channel value must not reject the config document");

        assert_eq!(cfg.updates.channel, UpdateChannel::Stable);
        assert!(
            cfg.autostart,
            "the other settings must survive the fallback"
        );
        assert_eq!(cfg.teams.status_format, "🎧 {track}");

        let logged = LOG_LINES.lock().clone();
        let warned = logged
            .iter()
            .find(|line| line.contains("nightly"))
            .unwrap_or_else(|| panic!("the fallback must be logged: {logged:?}"));
        assert!(
            warned.contains("[CFG]"),
            "the logged line carries the module tag: {warned}"
        );
    }

    // -----------------------------------------------------------------
    // Issue #926: the config document loads field by field, so one bad
    // section cannot take the rest of the document down with it.
    // -----------------------------------------------------------------

    /// Create a unique temp config directory holding a `config.json` with
    /// `contents`; returns `(dir, path)`. The caller removes the dir.
    fn temp_config_file(tag: &str, contents: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "pj-test-{tag}-{}-{}",
            std::process::id(),
            chrono_like_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, contents).unwrap();
        (dir, path)
    }

    /// The list `config_from_sections` partitions on has to match the schema:
    /// a key missing from it would be read BOTH as its typed field and into
    /// `extra`, so a save would write the file's own value back twice.
    #[test]
    fn typed_config_keys_match_the_serialized_schema() {
        let serialized = serde_json::to_value(AppConfig::default()).expect("must serialize");
        let mut keys: Vec<&str> = serialized
            .as_object()
            .expect("the schema serializes to an object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        let mut known = TYPED_CONFIG_KEYS.to_vec();
        known.sort_unstable();
        assert_eq!(
            keys, known,
            "TYPED_CONFIG_KEYS must list exactly the top-level keys AppConfig owns"
        );
    }

    /// Issue #926: sections are independent. One wrong-typed field used to fail
    /// the single `serde_json::from_str::<AppConfig>` call, which `load_config`
    /// answered by quarantining the file and booting on defaults — so a
    /// hand-edited `teams` value cost the user their client id, quiet hours,
    /// polling tuning and everything else too.
    #[test]
    fn test_one_bad_section_keeps_every_other_section() {
        let _guard = QUARANTINE_TEST_LOCK.lock();
        LOGGER.call_once(|| {
            // Best-effort: another test may have installed a logger first.
            let _ = log::set_boxed_logger(Box::new(CapturingLogger));
            log::set_max_level(log::LevelFilter::Warn);
        });
        LOG_LINES.lock().clear();
        let prev = CONFIG_QUARANTINED.load(Ordering::SeqCst);
        CONFIG_QUARANTINED.store(false, Ordering::SeqCst);

        let (dir, path) = temp_config_file(
            "sections",
            r#"{
                "spotify": {"client_id": "abc"},
                "teams": {"status_format": 5},
                "polling": {"default_interval_seconds": 42, "max_interval_seconds": 55},
                "logging": {"log_level": "Debug", "max_file_size_mb": 7},
                "updates": {"channel": "beta"},
                "autostart": true,
                "notifications": {"track_change": false},
                "status_rules": {"quiet_hours": [{"enabled": true, "start_minutes": 1320, "end_minutes": 420, "days": [2]}]},
                "future_top_level": {"kept": true}
            }"#,
        );

        let cfg = load_config_from(&path).expect("a partially invalid document must still load");

        assert_eq!(
            cfg.teams.status_format,
            default_status_format(),
            "the invalid section takes its default"
        );
        assert_eq!(cfg.spotify.client_id, "abc", "a sibling section is kept");
        assert_eq!(cfg.polling.default_interval_seconds, 42);
        assert_eq!(cfg.polling.max_interval_seconds, 55);
        assert_eq!(cfg.logging.log_level, "Debug");
        assert_eq!(cfg.logging.max_file_size_mb, 7);
        assert_eq!(cfg.updates.channel, UpdateChannel::Beta);
        assert!(cfg.autostart);
        assert!(!cfg.notifications.track_change);
        assert_eq!(cfg.status_rules.quiet_hours.len(), 1);
        assert_eq!(cfg.status_rules.quiet_hours[0].start_minutes, 1320);
        assert_eq!(
            cfg.extra.get("future_top_level"),
            Some(&serde_json::json!({"kept": true})),
            "unknown top-level keys still land in `extra`"
        );

        assert!(
            !quarantine_backup_path(&path).exists(),
            "one bad section must not quarantine the whole file"
        );
        assert!(!config_was_quarantined());
        let logged = LOG_LINES.lock().clone();
        assert!(
            logged
                .iter()
                .any(|line| line.contains("[CFG]") && line.contains("teams")),
            "the fallback must name the field it replaced: {logged:?}"
        );

        CONFIG_QUARANTINED.store(prev, Ordering::SeqCst);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Issue #926: `client_id` was the only persisted field without a serde
    /// default, so `{"spotify": {}}` — and equally `"client_id": null` — failed
    /// the whole document. Neither quarantines the file now: the first loads as
    /// the section's defaults, the second as those defaults plus a warning.
    #[test]
    fn test_empty_or_null_client_id_loads_without_quarantining() {
        let _guard = QUARANTINE_TEST_LOCK.lock();
        let prev = CONFIG_QUARANTINED.load(Ordering::SeqCst);
        CONFIG_QUARANTINED.store(false, Ordering::SeqCst);
        for contents in [
            r#"{"spotify": {}, "autostart": true}"#,
            r#"{"spotify": {"client_id": null}, "autostart": true}"#,
        ] {
            let (dir, path) = temp_config_file("client-id", contents);
            let cfg = load_config_from(&path).expect("must load");
            assert_eq!(cfg.spotify.client_id, "", "{contents}");
            assert_eq!(
                cfg.spotify.redirect_uri,
                default_redirect_uri(),
                "{contents}"
            );
            assert!(
                cfg.autostart,
                "the rest of the document is kept: {contents}"
            );
            assert!(!quarantine_backup_path(&path).exists(), "{contents}");
            assert!(!config_was_quarantined(), "{contents}");
            let _ = std::fs::remove_dir_all(&dir);
        }
        CONFIG_QUARANTINED.store(prev, Ordering::SeqCst);
    }

    /// Issue #926: the field-by-field loader is for documents that ARE an
    /// object. Anything else is not a config — a bare array/string/null root,
    /// or text that is not JSON — and is still quarantined to `config.json.bak`
    /// with the app booting on defaults.
    #[test]
    fn test_non_object_root_is_still_quarantined() {
        let _guard = QUARANTINE_TEST_LOCK.lock();
        let prev = CONFIG_QUARANTINED.load(Ordering::SeqCst);
        for contents in ["[1, 2, 3]", "\"spotify\"", "null", "{ NOT VALID JSON !!!"] {
            CONFIG_QUARANTINED.store(false, Ordering::SeqCst);
            let (dir, path) = temp_config_file("non-object", contents);
            let cfg = load_config_from(&path).expect("quarantine still yields defaults");
            assert_eq!(cfg.schema_version, default_schema_version());
            assert!(cfg.spotify.client_id.is_empty());
            assert!(
                quarantine_backup_path(&path).exists(),
                "{contents} must be quarantined"
            );
            assert!(!path.exists(), "{contents}");
            assert!(config_was_quarantined(), "{contents}");
            let _ = std::fs::remove_dir_all(&dir);
        }
        CONFIG_QUARANTINED.store(prev, Ordering::SeqCst);
    }

    // -----------------------------------------------------------------
    // Issue #802: the 0600 hardening is best-effort.
    // -----------------------------------------------------------------

    /// The body of the function `signature` starts — from its opening `{` to
    /// the matching `}` — found by brace counting, so a guard over it survives
    /// reordering / splitting / renaming of the code around it. Panics when the
    /// function itself is gone, which is a failure of the guard's premise.
    fn fn_body<'a>(src: &'a str, signature: &str) -> &'a str {
        let sig_idx = src
            .find(signature)
            .unwrap_or_else(|| panic!("{signature} must exist"));
        let brace_open_rel = src[sig_idx..]
            .find('{')
            .unwrap_or_else(|| panic!("{signature} must have an opening brace"));
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
                panic!("{signature} has unbalanced braces");
            }
        };
        &src[body_start + 1..body_end]
    }

    /// Issue #802: tightening the mode is a courtesy, never a precondition for
    /// reading a config the user's account can open. Both errors on that path
    /// were `?`-propagated, so an unreadable or unwritable mode failed the whole
    /// load — startup logged "no config found", `AppState.config` stayed `None`,
    /// the stores fell back to built-in defaults, and the next whole-document
    /// save persisted those defaults over the real file.
    ///
    /// A `?` cannot appear in a function that has no `Result` to return, so the
    /// guard is exact rather than stylistic. It cannot be replaced by a real
    /// failing `chmod`: that needs a file the test does not own (or an immutable
    /// / read-only mount), and `chmod` is gated on ownership of the file, not on
    /// write access to its directory — so the "0o555 temp dir" shape suggested
    /// in the issue does not deny it, for an unprivileged user or for root.
    #[test]
    fn test_config_mode_tightening_cannot_abort_the_load() {
        let body = fn_body(include_str!("config.rs"), "fn tighten_config_permissions(");
        assert!(
            !body.contains('?'),
            "tighten_config_permissions must not propagate an error: a mode that \
             cannot be read or set must not stop load_config from reading a file it \
             can open (issue #802)"
        );
    }

    /// Issue #802, the happy path: a loose `config.json` is still tightened to
    /// 0600 while it loads, and its stored values come back.
    #[cfg(unix)]
    #[test]
    fn test_loose_config_is_still_tightened_on_load() {
        let (dir, path) = temp_config_file("tighten", r#"{"autostart": true}"#);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let cfg = load_config_from(&path).expect("a readable config must load");
        assert!(cfg.autostart);
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600,
            "the #135 tightening must still run"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
