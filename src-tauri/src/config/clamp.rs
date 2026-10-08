use super::schema::{
    clamp_quiet_hours_window, AppConfig, LoggingConfig, PollingConfig, PreferredPresenceConfig,
    PresenceProfile, ShortcutsConfig, StatusRulesConfig, TeamsConfig, TrackRuleAction,
    TrackRuleEntry,
};
use serde::Deserialize;
pub(crate) fn clamp_polling(cfg: &mut PollingConfig) {
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
pub(crate) fn clamp_teams(cfg: &mut TeamsConfig) {
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
    // Issue #866: the preferred-presence pair rides the same
    // normalizer — `normalize_presence_pair` already clears both
    // fields when they fail to match `PRESENCE_COMBINATIONS`, so
    // disabling an unsupported config is automatic.
    clamp_preferred_presence(&mut cfg.preferred_presence);
    // Issue #873: the idle-away threshold. `0` disables (no clamp), any
    // other value is clamped into 60..=3600 so a hand-edited config
    // cannot put the gate in a state that surprises the user (a 1 s
    // threshold would have every normal typing pause fire the gate).
    if cfg.idle_away_after_seconds != 0 {
        cfg.idle_away_after_seconds = cfg.idle_away_after_seconds.clamp(60, 3600);
    }

    // Issue #867: cap the pre-meeting suppression window at 60 minutes —
    // anything larger is almost certainly a hand-edited mistake, and a
    // longer window only widens the blast radius of a flaky calendar.
    cfg.pre_meeting_suppress_minutes = cfg.pre_meeting_suppress_minutes.min(60);
}

/// Issue #866: bound the preferred-presence config the same way `clamp_rules`
/// bounds rule pairs. The expiry is clamped to `5..=720` minutes (Graph's
/// `expirationDuration` accepts anything but the app's clear-at-expiry logic
/// needs a sane cadence); an unsupported pair clears BOTH fields and disables
/// the feature — a Graph POST with a pair outside `PRESENCE_COMBINATIONS`
/// would 4xx every call, and the user would never see a presence move.
pub(crate) fn clamp_preferred_presence(cfg: &mut PreferredPresenceConfig) {
    cfg.expiry_minutes = cfg.expiry_minutes.clamp(5, 720);
    match normalize_presence_pair(&cfg.availability, &cfg.activity) {
        Some(pair) => {
            cfg.availability = pair.availability;
            cfg.activity = pair.activity;
        }
        None => {
            cfg.availability.clear();
            cfg.activity.clear();
            cfg.enabled = false;
        }
    }
}

/// Issue #866: resolve the preferred-presence config into the validated
/// `PresencePair` the Graph POST needs — `None` when the feature is off, the
/// pair is empty, or the user's `respect_manual_status` setting wins the
/// decision. Pure so the gating tests do not need a Tauri runtime.
pub fn preferred_presence_pair(
    teams: &TeamsConfig,
    respect_manual_status: bool,
) -> Option<PresencePair> {
    if !teams.preferred_presence.enabled || respect_manual_status {
        return None;
    }
    normalize_presence_pair(
        &teams.preferred_presence.availability,
        &teams.preferred_presence.activity,
    )
}

/// Issue #866: the `expirationDuration` the Graph
/// `setUserPreferredPresence` POST carries. Pure so the same shape that
/// goes to `setPresence` can be tested in isolation.
pub fn preferred_presence_expiry_duration(teams: &TeamsConfig) -> String {
    let minutes = teams.preferred_presence.expiry_minutes.max(5);
    format!("PT{}M", minutes)
}

/// Issue #870: borrow the user's lexicon (`teams.profanity_extra_words`,
/// issue #538) into the slice shape `profanity::filter_status_for_locale` expects. The
/// function is `None`-aware — a hand-edited config that lacks the section
/// reads as the empty slice, reproducing the pre-#538 behaviour exactly.
pub fn profanity_extra_words_for_filter(config: Option<&std::sync::Arc<AppConfig>>) -> &[String] {
    config
        .map(|c| c.teams.profanity_extra_words.as_slice())
        .unwrap_or(&[])
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

/// S4 (issue #672): minutes in a rule's day. `end_minutes` may be
/// `TRACK_RULE_DAY_MINUTES` (= the end of the day), which is why the track-rule
/// window uses `u32` while [`QuietHoursEntry`] carries the same value in the
/// `u16` [`QUIET_HOURS_DAY_MINUTES`].
pub const TRACK_RULE_DAY_MINUTES: u32 = 1440;

/// The same 24-hour day as [`TRACK_RULE_DAY_MINUTES`], in the `u16` width
/// [`QuietHoursEntry`]'s minute fields carry (issue #821). One value written
/// twice, once per field width; both schedule windows draw their bounds from it,
/// so the two halves of the rules model cannot disagree about where the day
/// ends.
pub(crate) const QUIET_HOURS_DAY_MINUTES: u16 = 1440;

/// Normalize the rule model (finding #634, issue #634): canonicalize every
/// presence pair, bound every replacement text, and normalize both schedule
/// windows ([`clamp_quiet_hours_window`] for quiet hours, issue #821;
/// [`clamp_track_rule_window`] for track rules, issue #672). Mirrors
/// `clamp_polling` / `clamp_teams`, so it runs on load and on every save through
/// [`clamped_config`]. Issue #868 adds the action's nested text / value / id
/// normalization through [`clamp_track_rule_action`] — the SINGLE
/// normalizer the spec mandates, so a hand-edited config cannot smuggle a
/// 200-character status or a 100 000-minute snooze past the IPC boundary.
pub(crate) fn clamp_rules(cfg: &mut StatusRulesConfig) {
    for entry in &mut cfg.quiet_hours {
        clamp_presence_pair(
            &mut entry.presence_availability,
            &mut entry.presence_activity,
        );
        clamp_rule_text(&mut entry.replacement_status);
        clamp_quiet_hours_window(entry);
    }
    for rule in &mut cfg.track_rules {
        clamp_presence_pair(&mut rule.presence_availability, &mut rule.presence_activity);
        clamp_rule_text(&mut rule.replacement_status);
        clamp_track_rule_window(rule);
        clamp_track_rule_action(rule);
    }
}

/// Normalize a track rule's `action` field (issue #868). The legacy
/// flat fields (`replacement_status`, `presence_availability` /
/// `presence_activity`) are also mirrored INTO the action so the rule
/// walker and the dry-run tester share one projection — a rule that
/// sets `replacement_status` but keeps `action: Suppress` continues to
/// behave like the legacy "post this fixed text" replacement, but a
/// rule that sets `action: Replace { status: "…" }` now uses the new
/// field verbatim. The `min_duration_seconds` cap mirrors the same
/// paranoia as the `clamp_teams` caps — a hand-edited config cannot
/// put the duration gate in a permanently-firing state.
pub(crate) fn clamp_track_rule_action(rule: &mut TrackRuleEntry) {
    rule.min_duration_seconds = rule.min_duration_seconds.min(MAX_TRACK_RULE_DURATION_SECS);
    match &mut rule.action {
        TrackRuleAction::Suppress => {
            // No fields to clamp.
        }
        TrackRuleAction::Replace { status } => {
            clamp_rule_text(status);
        }
        TrackRuleAction::SnoozeMinutes { value } => {
            // 1..=1440 minutes (24 hours); an empty / zero value falls
            // back to the legacy SnoozePreset::ForMinutes(15) default
            // when the rule fires, so the gate can still act.
            if *value == 0 {
                *value = 15;
            }
            *value = (*value).clamp(1, MAX_TRACK_RULE_SNOOZE_MINUTES);
        }
        TrackRuleAction::Profile { id } => {
            clamp_profile_id(id);
        }
        TrackRuleAction::Presence {
            availability,
            activity,
        } => {
            clamp_presence_pair(availability, activity);
        }
    }
}

/// Upper bound on `min_duration_seconds` (issue #868): 24 h. Mirrors
/// the 60 minute pre-meeting cap and the `clamp_teams` upper bounds so
/// a hand-edited config cannot wedge the duration gate in a
/// permanently-matching state.
pub const MAX_TRACK_RULE_DURATION_SECS: u32 = 24 * 60 * 60;

/// Upper bound on `SnoozeMinutes.value` (issue #868): 24 h.
pub const MAX_TRACK_RULE_SNOOZE_MINUTES: u32 = 24 * 60;

/// Issue #869: shared cap on a presence profile's `id`. Also reused by
/// `clamp_track_rule_action` for `TrackRuleAction::Profile { id }`.
pub const MAX_PROFILE_ID_CHARS: usize = 32;

/// Issue #869: trim / cap a presence profile id (and the matching
/// `TrackRuleAction::Profile { id }`). Whitespace is stripped from the
/// edges; the result is truncated to [`MAX_PROFILE_ID_CHARS`]; an empty
/// id stays empty so the rule walker treats it as `Suppress`.
pub fn clamp_profile_id(id: &mut String) {
    let trimmed = id.trim().to_string();
    if trimmed.chars().count() > MAX_PROFILE_ID_CHARS {
        *id = trimmed.chars().take(MAX_PROFILE_ID_CHARS).collect();
    } else {
        *id = trimmed;
    }
}

/// Issue #869: normalize the presence-profile list (issue #869).
///
/// - Names are trimmed + truncated to [`MAX_PROFILE_ID_CHARS`] and
///   must be unique (case-sensitive); a duplicate is dropped so a
///   hand-edited config cannot smuggle two profiles under the same id
///   and confuse the tray / hotkey / CLI.
/// - A profile whose name normalises to empty is dropped for the same
///   reason `clamp_profile_id` clears empty ids.
/// - The active-profile pointer is cleared if its name no longer
///   matches any surviving profile (Settings just deleted it; a hand
///   edit typo'd it; an upgrade dropped the whole list). The pointer
///   is `Option<&mut Option<String>>` so the caller can pass either
///   `&mut config.active_profile` or a local — both paths share one
///   definition of "the pointer is invalid, so it must be cleared".
pub fn clamp_presence_profiles(profiles: &mut Vec<PresenceProfile>, active: &mut Option<String>) {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    profiles.retain_mut(|profile| {
        clamp_profile_id(&mut profile.name);
        if profile.name.is_empty() {
            log::warn!(
                "[CFG] presence_profiles: dropped a profile with an empty name (issue #869)"
            );
            return false;
        }
        if !seen.insert(profile.name.clone()) {
            log::warn!(
                "[CFG] presence_profiles: dropped a duplicate profile named {:?} (issue #869)",
                profile.name
            );
            return false;
        }
        // Cap the idle threshold so a hand-edited config cannot put
        // the idle gate in a permanently-firing state.
        if let Some(value) = profile.idle_away_after_seconds.as_mut() {
            *value = (*value).min(86_400);
        }
        // A profile's `track_rules` overlay, when present, is itself
        // a Vec<TrackRuleEntry> — re-run `clamp_rules` semantics on
        // it so a profile stored before the rule extensions existed
        // gets the same normalization every other rule path gets.
        if let Some(rules) = profile.track_rules.as_mut() {
            for rule in rules.iter_mut() {
                clamp_track_rule_window(rule);
                clamp_track_rule_action(rule);
            }
        }
        true
    });
    if let Some(name) = active.as_ref() {
        let still_present = profiles.iter().any(|p| &p.name == name);
        if !still_present {
            log::warn!(
                "[CFG] active_profile: the stored profile {:?} no longer exists — cleared (issue #869)",
                name
            );
            *active = None;
        }
    }
}

/// Issue #869: resolve the active profile overlay onto the base
/// configuration at READ time. `effective_config` is the single
/// non-mutating overlay path the tray / hotkey / CLI / Settings all
/// share; it MUST NOT mutate the input (the spec calls this out
/// explicitly — a "switch to profile X" call is a runtime state
/// change, not a config rewrite).
///
/// Resolution rules:
/// - `active_profile == None` → the input is returned unchanged.
/// - `active_profile == Some(name)` but `name` does not match any
///   profile → the input is returned unchanged (defensive parity
///   with `clamp_presence_profiles`, which would have cleared the
///   pointer; the runtime side keeps the read-only contract even if a
///   caller forgot to clamp first).
/// - Otherwise, every `Some(_)` field on the matched profile wins
///   over the base field. `None` overlay fields fall through to the
///   base unchanged. The `track_rules` overlay, when present, REPLACES
///   the base rules list — the spec's documented "rules subset"
///   semantics — so `Some(vec![])` is a legitimate "no rules while
///   this profile is active" shape.
pub fn effective_config(config: &AppConfig) -> AppConfig {
    let Some(active_name) = config.active_profile.as_ref() else {
        return config.clone();
    };
    let Some(profile) = config
        .presence_profiles
        .iter()
        .find(|p| &p.name == active_name)
    else {
        // Defensive: the clamp normally clears this case, but the
        // runtime side keeps the read-only contract. Return the base
        // unchanged rather than panic / silently pick a wrong profile.
        return config.clone();
    };
    let mut out = config.clone();
    if let Some(v) = &profile.status_format {
        out.teams.status_format = v.clone();
    }
    if let Some(v) = profile.clear_on_pause {
        out.teams.clear_on_pause = v;
    }
    if let Some(v) = profile.availability_sync {
        out.teams.availability_sync = v;
    }
    if let Some(v) = profile.gate_when_out_of_office {
        out.teams.gate_when_out_of_office = v;
    }
    if let Some(v) = profile.gate_when_presenting {
        out.teams.gate_when_presenting = v;
    }
    if let Some(v) = profile.idle_away_after_seconds {
        out.teams.idle_away_after_seconds = v;
    }
    if let Some(pp) = &profile.preferred_presence {
        out.teams.preferred_presence = pp.clone();
    }
    if let Some(rules) = &profile.track_rules {
        out.status_rules.track_rules = rules.clone();
    }
    if let Some(notifications) = &profile.notifications {
        out.notifications = notifications.clone();
    }
    out
}

/// Issue #893: the hot-path twin of [`effective_config`]. When no profile is
/// active (the common case) the base pointer is shared — no deep copy — and
/// only an active overlay allocates. Poll-iteration readers take their
/// snapshot through this so one iteration performs no `AppConfig` clone.
pub fn effective_snapshot(config: &std::sync::Arc<AppConfig>) -> std::sync::Arc<AppConfig> {
    if config.active_profile.is_none() {
        return std::sync::Arc::clone(config);
    }
    std::sync::Arc::new(effective_config(config))
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
/// Issue #870: the text-length bound shared by [`clamp_rule_text`] and the
/// Dashboard composer / `--set-status` CLI flag. Public so the manual
/// status path and the rule-replacement-text path share one anchor.
pub fn clamp_rule_text(text: &mut String) {
    if text.chars().count() > MAX_RULE_STATUS_CHARS {
        *text = text.chars().take(MAX_RULE_STATUS_CHARS).collect();
    }
}

/// Normalize a rule's weekday list in place (issue #821): keep only the
/// documented ISO range `1..=7`, then sort and deduplicate.
///
/// The ONE normalization of `days` for both halves of the rules model — the
/// quiet-hours window and the track rule — so the two cannot drift on what
/// load-time normalization means. A list that ends up empty means "every day",
/// so dropping an out-of-range value can only widen a rule, never leave it
/// matching nothing.
pub(crate) fn normalize_rule_days(days: &mut Vec<u8>) {
    days.retain(|day| (1..=7).contains(day));
    days.sort_unstable();
    days.dedup();
}

/// S4 (issue #672): normalize a track rule's schedule in place.
///
/// Minutes are clamped into `0..=TRACK_RULE_DAY_MINUTES`, so a hand-edited
/// config cannot wedge the comparison, and `days` goes through
/// [`normalize_rule_days`] — the documented ISO range `1..=7`, sorted and
/// deduplicated — so it matches the invariant [`QuietHoursEntry`] relies on.
/// The window itself keeps
/// [`QuietHoursEntry`]'s semantics: `[start, end)` with a wrap-around pair
/// (`start > end`, e.g. 22:00→07:00) honoured, and `start == end` matching
/// nothing.
pub(crate) fn clamp_track_rule_window(rule: &mut TrackRuleEntry) {
    // A START of 1440 is unreachable: `now` never exceeds 1439, so such a rule
    // could never match while the picker happily renders it as 00:00. Clamp the
    // start to the last minute of the day instead, and the end to the end of
    // the day (1440), which IS reachable as "until midnight".
    rule.start_minutes = rule.start_minutes.min(TRACK_RULE_DAY_MINUTES - 1);
    rule.end_minutes = rule.end_minutes.min(TRACK_RULE_DAY_MINUTES);
    normalize_rule_days(&mut rule.days);
}

pub(crate) fn clamp_logging(cfg: &mut LoggingConfig) {
    cfg.max_file_size_mb = cfg.max_file_size_mb.clamp(1, 500);
    cfg.keep_files = cfg.keep_files.clamp(1, 20);
}

/// Canonicalise the stored UI locale to a tag the app can actually render
/// (issue #767 — the clamp for the new `ConfigPatch::locale` field).
///
/// `None` stays `None`: that is the documented pre-4.7 state, and it means
/// "follow the OS", not "English". A present tag is resolved onto one of the
/// shipped dictionaries through
/// [`crate::i18n::resolve_tag`], which is the same function the `set_locale`
/// command canonicalises with — so a patch, a `set_locale` call and a
/// hand-edited file all converge on the identical stored value. Without it a
/// patch could persist `"de-AT-x-priv"` and the picker would render a tag
/// that resolves to English while the file claims German.
pub(crate) fn clamp_locale(cfg: &mut AppConfig) {
    let Some(tag) = cfg.locale.as_deref() else {
        return;
    };
    cfg.locale = Some(crate::i18n::resolve_tag(Some(tag)).to_string());
}

/// Read a three-state `Option<Option<String>>` patch field: absent leaves the
/// stored value untouched, `null` clears it, a string sets it (issue #767).
///
/// Serde cannot do this unaided. For `Option<Option<T>>` both a MISSING key and
/// an explicit `null` deserialize to `None`, so the two states that the field
/// exists to distinguish collapse into one — `{"locale": null}` would read as
/// "this patch says nothing about the locale" and the caller's intent to clear
/// it would be silently dropped. This maps the two JSON spellings onto the two
/// distinct `Option` layers, with `#[serde(default)]` still supplying the
/// missing-key case.
pub(crate) fn deserialize_optional_tag<'de, D>(
    deserializer: D,
) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer).map(Some)
}

/// Bound the length of the two shortcut bindings (issue #767 — the clamp for
/// the new `ConfigPatch::shortcuts` field).
///
/// A binding is user-supplied text that goes straight into `config.json`, and
/// the Settings number/text inputs do not constrain a typed value, so an
/// unbounded string is the same class of hazard `clamp_rule_text` bounds for a
/// rule's replacement text. It is capped here rather than validated because
/// whether an accelerator PARSES is `commands::shortcuts::validate_accelerator`'s
/// job, and its failure is surfaced to the user as a `ShortcutReason` rather
/// than silently unbound (issue #810). A second, quieter rule here would make
/// two places answer "is this binding usable?".
///
/// A blank binding is deliberately LEFT ALONE — `Some("  ")` is not rewritten
/// to `None`, and nothing is trimmed away. `configured_binding` already treats
/// blank as unbound when it plans a registration, and the Settings field
/// renders exactly what is stored; a writer that normalised it away would make
/// that field lie about the document on disk (pinned by
/// `shortcut_bindings_round_trip_through_json`).
pub(crate) fn clamp_shortcuts(cfg: &mut ShortcutsConfig) {
    fn bound(slot: &mut Option<String>) {
        let Some(value) = slot.as_deref() else {
            return;
        };
        if value.chars().count() > MAX_SHORTCUT_BINDING_CHARS {
            *slot = Some(value.chars().take(MAX_SHORTCUT_BINDING_CHARS).collect());
        }
    }
    bound(&mut cfg.toggle_playback);
    bound(&mut cfg.toggle_sync);
}

/// Longest accelerator spelling `clamp_shortcuts` will store (issue #767).
///
/// Generous next to any real binding — `CmdOrCtrl+Alt+Shift+F12` is 23
/// characters — and small enough that a pasted paragraph cannot become the
/// stored document. The registrar still rejects anything that does not parse
/// (`validate_accelerator`); this only bounds the text on the way to disk.
pub(crate) const MAX_SHORTCUT_BINDING_CHARS: usize = 128;
