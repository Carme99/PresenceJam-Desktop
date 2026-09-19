//! Track-rule dry-run tester (issue #868).
//!
//! The Settings UI exposes a "test this rule" form. The user types a
//! synthetic track (artist / track / album / show / device /
//! playlist-uri / duration), picks a weekday + minute-of-day, and the
//! `explain_rules` Tauri command returns the matched rule index and a
//! full reason chain — one entry per rule showing the per-field
//! decision (matched / not matched / skipped because disabled /
//! suppressed by `negate`) so the user can debug "why did this fire?" /
//! "why did this NOT fire?" without waiting for a real Spotify poll.

use crate::config::{AppConfig, TrackRuleAction, TrackRuleEntry, TrackRuleMatchKind};
use crate::polling::{
    track_rule_conditions_match, track_rule_hit, track_rule_schedule_matches, TrackRuleContext,
};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

/// Log tag prefix for this submodule (issue #79 item 3).
const CMD: &str = "[CMD.RULES]";

/// The dry-run tester input — exactly the dimensions the live
/// `process_track` path now feeds into the rule walker, exposed at the
/// IPC boundary so the Settings UI can probe without a real Spotify
/// poll. Every field is optional: an empty `album` means "the user did
/// not type one" and is matched by the same "empty substring matches
/// anything" rule every other field uses.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SyntheticTrack {
    pub artist: String,
    pub track: String,
    pub album: String,
    pub show: String,
    pub device: String,
    pub playlist_uri: String,
    pub duration_ms: u64,
}

/// One per-rule evaluation step the tester reports back. Mirrors the
/// fields `TrackRuleEntry` cares about so the UI can render "rule N
/// matched because X" without round-tripping the rule itself.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RuleEvaluationStep {
    pub index: usize,
    pub enabled: bool,
    pub matched: bool,
    /// Why the rule matched / did not match. `None` means "matched
    /// because every condition held" (or "skipped because `enabled`
    /// was false"). A populated string names the first failing
    /// dimension so the UI can show "rule 2 did NOT match — track
    /// substring "rain" not found in "Sunshine"".
    pub reason: Option<String>,
    /// Whether `negate` flipped the underlying condition result.
    /// Lets the UI render "rule matched because the conditions DID
    /// NOT hold AND negate is true".
    pub negated: bool,
    /// The rule's effect when it matches — copied through so the
    /// tester can render "rule 3 would Replace with: …" without
    /// reading the rule again.
    pub action: TrackRuleAction,
    /// `match_kind` is captured too so the UI can render the "Exact"
    /// / "Glob" badge next to the matching step.
    pub match_kind: TrackRuleMatchKind,
}

/// The full dry-run result. The Settings UI renders
/// `matched_index` next to the form, `reason_chain` as the expanded
/// per-rule detail, and `summary` as a one-line "rule N would fire:
/// Replace" / "no rule fired" headline.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RulesExplanation {
    pub matched_index: Option<usize>,
    pub summary: String,
    pub reason_chain: Vec<RuleEvaluationStep>,
    /// A clone of the input that produced the result, so the
    /// frontend can render its form values without tracking them
    /// separately. Issue #868 acceptance criterion: a tester that
    /// reports `matched_index` and "the winning rule was rule N" —
    /// the Svelte UI uses `synthetic_track.artist` etc. for the
    /// "what you typed" echo.
    pub synthetic_track: SyntheticTrack,
    pub now_minutes: u16,
    pub weekday: u8,
}

/// The dry-run Tauri command. Returns the same `RulesExplanation` for
/// the supplied synthetic track; the live process_track path uses the
/// SAME walker (`matching_track_rule_at_with_ctx`) so the tester's
/// verdict matches what a real poll would have done.
#[tauri::command]
pub fn explain_rules(
    _app: AppHandle,
    now_minutes: u16,
    weekday: u8,
    synthetic_track: SyntheticTrack,
) -> RulesExplanation {
    log::debug!(
        "{CMD} explain_rules: now_minutes={now_minutes} weekday={weekday} \
         artist=\"{}\" track=\"{}\"",
        synthetic_track.artist,
        synthetic_track.track,
    );
    let config = crate::config::load_config().ok();
    explain_rules_with_config(&config, now_minutes, weekday, &synthetic_track)
}

/// The pure evaluator. `commands::rules::explain_rules` is the IPC
/// wrapper; tests exercise this directly with a hand-built `AppConfig`
/// so the verdict can be pinned without touching disk.
pub fn explain_rules_with_config(
    config: &Option<AppConfig>,
    now_minutes: u16,
    weekday: u8,
    track: &SyntheticTrack,
) -> RulesExplanation {
    let ctx = TrackRuleContext {
        artist: &track.artist,
        title: &track.track,
        album: &track.album,
        show: &track.show,
        device: &track.device,
        playlist_uri: &track.playlist_uri,
        duration_ms: track.duration_ms,
    };
    let empty_rules = crate::config::StatusRulesConfig::default();
    let rules = config
        .as_ref()
        .map(|c| &c.status_rules)
        .unwrap_or(&empty_rules);
    let mut chain = Vec::with_capacity(rules.track_rules.len());
    let mut matched_index: Option<usize> = None;
    for (idx, rule) in rules.track_rules.iter().enumerate() {
        let step = evaluate_step(idx, rule, &ctx, now_minutes, weekday);
        if step.matched && matched_index.is_none() {
            matched_index = Some(idx);
        }
        chain.push(step);
    }
    let summary = match matched_index {
        Some(i) => format_rule_summary(&rules.track_rules[i]),
        None => "no track rule matched".to_string(),
    };
    RulesExplanation {
        matched_index,
        summary,
        reason_chain: chain,
        synthetic_track: track.clone(),
        now_minutes,
        weekday,
    }
}

fn evaluate_step(
    index: usize,
    rule: &TrackRuleEntry,
    ctx: &TrackRuleContext<'_>,
    now_minutes: u16,
    weekday: u8,
) -> RuleEvaluationStep {
    let enabled = rule.enabled;
    if !enabled {
        return RuleEvaluationStep {
            index,
            enabled,
            matched: false,
            reason: Some("disabled".to_string()),
            negated: rule.negate,
            action: rule.action.clone(),
            match_kind: rule.match_kind,
        };
    }
    if !track_rule_schedule_matches(rule, now_minutes, weekday) {
        return RuleEvaluationStep {
            index,
            enabled,
            matched: false,
            reason: Some("schedule does not contain now".to_string()),
            negated: rule.negate,
            action: rule.action.clone(),
            match_kind: rule.match_kind,
        };
    }
    let raw = track_rule_conditions_match(rule, ctx);
    let mut reason: Option<String> = None;
    if !raw {
        reason = Some(first_failing_dimension(rule, ctx));
    }
    let matched = if rule.negate { !raw } else { raw };
    // Defensive — the live walker (`track_rule_hit`) is the same
    // function we call here; we re-derive so the per-step projection
    // and the headline verdict cannot drift.
    debug_assert_eq!(
        matched,
        track_rule_hit(rule, ctx),
        "rule_step.matched must agree with track_rule_hit"
    );
    RuleEvaluationStep {
        index,
        enabled,
        matched,
        reason,
        negated: rule.negate,
        action: rule.action.clone(),
        match_kind: rule.match_kind,
    }
}

/// First dimension the rule's combined conditions failed on — used
/// as the human-readable "why didn't this match" string. Order matches
/// `track_rule_conditions_match` so the message names the dimension the
/// walker evaluated first.
fn first_failing_dimension(rule: &TrackRuleEntry, ctx: &TrackRuleContext<'_>) -> String {
    if rule.min_duration_seconds > 0
        && ctx.duration_ms < u64::from(rule.min_duration_seconds) * 1000
    {
        return format!(
            "track duration {} ms is below min_duration_seconds {} s",
            ctx.duration_ms, rule.min_duration_seconds,
        );
    }
    if !field_match_msg(ctx.album, &rule.album_substring, "album") {
        return format!(
            "album \"{}\" does not contain \"{}\"",
            ctx.album, rule.album_substring,
        );
    }
    if !field_match_msg(ctx.show, &rule.show_substring, "show") {
        return format!(
            "show \"{}\" does not contain \"{}\"",
            ctx.show, rule.show_substring,
        );
    }
    if !field_match_msg(ctx.device, &rule.device_substring, "device") {
        return format!(
            "device \"{}\" does not contain \"{}\"",
            ctx.device, rule.device_substring,
        );
    }
    if !field_match_msg(ctx.playlist_uri, &rule.playlist_uri, "playlist_uri") {
        return format!(
            "playlist_uri \"{}\" does not contain \"{}\"",
            ctx.playlist_uri, rule.playlist_uri,
        );
    }
    if !field_match_msg(ctx.artist, &rule.artist_substring, "artist") {
        return match rule.match_kind {
            TrackRuleMatchKind::Substring => format!(
                "artist \"{}\" does not contain \"{}\"",
                ctx.artist, rule.artist_substring,
            ),
            TrackRuleMatchKind::Exact => format!(
                "artist \"{}\" does not equal \"{}\"",
                ctx.artist, rule.artist_substring,
            ),
            TrackRuleMatchKind::Glob => format!(
                "artist \"{}\" does not match glob \"{}\"",
                ctx.artist, rule.artist_substring,
            ),
        };
    }
    format!(
        "track \"{}\" does not contain \"{}\"",
        ctx.title, rule.track_substring,
    )
}

/// Whether `pattern` matches `haystack` under the legacy
/// case-insensitive-substring rules. Album / show / device /
/// playlist-uri always use substring semantics; the per-field reason
/// message uses substring wording too.
fn field_match_msg(haystack: &str, pattern: &str, _label: &str) -> bool {
    pattern.is_empty() || haystack.to_lowercase().contains(&pattern.to_lowercase())
}

fn format_rule_summary(rule: &TrackRuleEntry) -> String {
    match &rule.action {
        TrackRuleAction::Suppress => "would suppress this track".to_string(),
        TrackRuleAction::Replace { status } => {
            format!("would Replace status with: \"{status}\"")
        }
        TrackRuleAction::SnoozeMinutes { value } => {
            format!("would Snooze sync for {value} minutes")
        }
        TrackRuleAction::Profile { id } => {
            if id.is_empty() {
                "would Suppress (profile id was empty)".to_string()
            } else {
                format!("would switch to Profile \"{id}\"")
            }
        }
        TrackRuleAction::Presence {
            availability,
            activity,
        } => {
            if availability.is_empty() && activity.is_empty() {
                "would Suppress (presence pair was empty)".to_string()
            } else {
                format!("would set Presence {availability}/{activity}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{StatusRulesConfig, TrackRuleAction, TrackRuleEntry, TrackRuleMatchKind};

    fn cfg_with_rules(rules: Vec<TrackRuleEntry>) -> Option<AppConfig> {
        Some(AppConfig {
            status_rules: StatusRulesConfig {
                quiet_hours: Vec::new(),
                track_rules: rules,
                ..StatusRulesConfig::default()
            },
            ..AppConfig::default()
        })
    }

    fn synthetic(artist: &str, track: &str) -> SyntheticTrack {
        SyntheticTrack {
            artist: artist.to_string(),
            track: track.to_string(),
            ..SyntheticTrack::default()
        }
    }

    /// Issue #868 acceptance: two overlapping rules + a synthetic
    /// track assert the winning index — array order is priority.
    #[test]
    fn explain_rules_returns_winning_index_for_overlapping_rules() {
        let rules = vec![
            TrackRuleEntry {
                enabled: true,
                artist_substring: "lofi".to_string(),
                track_substring: String::new(),
                action: TrackRuleAction::Replace {
                    status: "Focus time".to_string(),
                },
                ..TrackRuleEntry::default()
            },
            TrackRuleEntry {
                enabled: true,
                artist_substring: "lofi".to_string(),
                track_substring: String::new(),
                action: TrackRuleAction::Suppress,
                ..TrackRuleEntry::default()
            },
        ];
        let cfg = cfg_with_rules(rules);
        let out = explain_rules_with_config(&cfg, 600, 3, &synthetic("Lofi Girl", "Rain Sounds"));
        assert_eq!(out.matched_index, Some(0));
        assert_eq!(out.summary, "would Replace status with: \"Focus time\"");
        assert_eq!(out.reason_chain.len(), 2);
        assert!(out.reason_chain[0].matched);
        // The second rule also matches (its conditions are the same)
        // — array order is recorded as priority, NOT exclusivity.
        // The dry-run tester reports every per-rule decision so the
        // user can see "rule 1 would ALSO have fired".
        assert!(out.reason_chain[1].matched);
    }

    /// Issue #868 acceptance: a negated device match asserts the
    /// "suppress unless device name contains X" pattern — the rule
    /// fires when the negation's conditions do NOT hold.
    #[test]
    fn explain_rules_negated_device_match_fires_when_device_absent() {
        let rule = TrackRuleEntry {
            enabled: true,
            artist_substring: String::new(),
            track_substring: String::new(),
            device_substring: "office".to_string(),
            negate: true,
            action: TrackRuleAction::Suppress,
            ..TrackRuleEntry::default()
        };
        let cfg = cfg_with_rules(vec![rule.clone()]);
        // Device absent — `conditions_match` is false → `negate` flips
        // it to true → the rule fires.
        let absent = explain_rules_with_config(
            &cfg,
            600,
            3,
            &SyntheticTrack {
                device: "Kitchen speaker".to_string(),
                ..synthetic("", "")
            },
        );
        assert_eq!(absent.matched_index, Some(0));
        assert!(absent.reason_chain[0].matched);
        assert!(absent.reason_chain[0].negated);
        // Device present — `conditions_match` is true → `negate`
        // flips it to false → the rule does NOT fire.
        let present = explain_rules_with_config(
            &cfg,
            600,
            3,
            &SyntheticTrack {
                device: "Office speaker".to_string(),
                ..synthetic("", "")
            },
        );
        assert_eq!(present.matched_index, None);
        assert!(!present.reason_chain[0].matched);
        let _ = rule;
    }

    /// Issue #868 acceptance: array-order priority. The disabled
    /// rule at index 0 cannot win even though its conditions would
    /// otherwise match — the enabled rule at index 1 does.
    #[test]
    fn explain_rules_disabled_rules_never_match() {
        let rules = vec![
            TrackRuleEntry {
                enabled: false,
                artist_substring: "lofi".to_string(),
                action: TrackRuleAction::Suppress,
                ..TrackRuleEntry::default()
            },
            TrackRuleEntry {
                enabled: true,
                artist_substring: "lofi".to_string(),
                action: TrackRuleAction::Replace {
                    status: "Focus".to_string(),
                },
                ..TrackRuleEntry::default()
            },
        ];
        let cfg = cfg_with_rules(rules);
        let out = explain_rules_with_config(&cfg, 600, 3, &synthetic("Lofi Girl", "Anything"));
        assert_eq!(out.matched_index, Some(1));
        assert!(!out.reason_chain[0].matched);
        assert_eq!(out.reason_chain[0].reason.as_deref(), Some("disabled"));
    }

    /// Issue #868: a rule with the `min_duration_seconds` gate set
    /// must NOT fire on a track shorter than the threshold.
    #[test]
    fn explain_rules_min_duration_gate_skips_short_tracks() {
        let rule = TrackRuleEntry {
            enabled: true,
            artist_substring: "lofi".to_string(),
            min_duration_seconds: 120,
            action: TrackRuleAction::Suppress,
            ..TrackRuleEntry::default()
        };
        let cfg = cfg_with_rules(vec![rule]);
        let short = explain_rules_with_config(
            &cfg,
            600,
            3,
            &SyntheticTrack {
                duration_ms: 60_000,
                ..synthetic("Lofi Girl", "Anything")
            },
        );
        assert_eq!(short.matched_index, None);
        assert!(short.reason_chain[0]
            .reason
            .as_deref()
            .unwrap_or("")
            .contains("duration"));
        let long = explain_rules_with_config(
            &cfg,
            600,
            3,
            &SyntheticTrack {
                duration_ms: 180_000,
                ..synthetic("Lofi Girl", "Anything")
            },
        );
        assert_eq!(long.matched_index, Some(0));
    }

    /// Issue #868: `MatchKind::Exact` distinguishes from the legacy
    /// substring behaviour — an exact rule requires a full
    /// case-insensitive equality.
    #[test]
    fn explain_rules_exact_match_kind_does_not_substring() {
        let rules = vec![
            TrackRuleEntry {
                enabled: true,
                artist_substring: "lofi girl".to_string(),
                match_kind: TrackRuleMatchKind::Exact,
                action: TrackRuleAction::Suppress,
                ..TrackRuleEntry::default()
            },
            TrackRuleEntry {
                enabled: true,
                artist_substring: "lofi".to_string(),
                match_kind: TrackRuleMatchKind::Substring,
                action: TrackRuleAction::Suppress,
                ..TrackRuleEntry::default()
            },
        ];
        let cfg = cfg_with_rules(rules);
        let out = explain_rules_with_config(&cfg, 600, 3, &synthetic("Lofi Girl", "Anything"));
        // The exact rule on "lofi girl" does NOT match "Lofi Girl"
        // (case-insensitive equal — it IS equal) but the
        // substring rule on "lofi" does. The exact rule actually
        // matches because `Lofi Girl`.eq_ignore_ascii_case("lofi
        // girl") is true. The substring rule ALSO matches because
        // "Lofi Girl".contains("lofi"). First-match wins, so index 0.
        assert_eq!(out.matched_index, Some(0));
        // A different artist with a substring match but not an exact
        // match: the substring rule wins.
        let partial =
            explain_rules_with_config(&cfg, 600, 3, &synthetic("Lofi Girl Plus", "Anything"));
        assert_eq!(partial.matched_index, Some(1));
    }

    /// Issue #868: a no-rules config returns `matched_index: None`
    /// and a one-line summary the UI can render verbatim.
    #[test]
    fn explain_rules_empty_rule_set() {
        let cfg = cfg_with_rules(Vec::new());
        let out = explain_rules_with_config(&cfg, 600, 3, &synthetic("Anyone", "Anything"));
        assert_eq!(out.matched_index, None);
        assert_eq!(out.summary, "no track rule matched");
        assert!(out.reason_chain.is_empty());
    }

    /// Issue #868: the `PresencePair` is rebuilt from
    /// `availability` / `activity` so an action's presence pair
    /// survives the round-trip; the summary reads `set Presence`.
    #[test]
    fn explain_rules_action_presence_summary_reads_pair() {
        let rule = TrackRuleEntry {
            enabled: true,
            artist_substring: "lofi".to_string(),
            action: TrackRuleAction::Presence {
                availability: "DoNotDisturb".to_string(),
                activity: "Presenting".to_string(),
            },
            ..TrackRuleEntry::default()
        };
        let cfg = cfg_with_rules(vec![rule]);
        let out = explain_rules_with_config(&cfg, 600, 3, &synthetic("Lofi Girl", "Rain Sounds"));
        assert_eq!(out.matched_index, Some(0));
        assert!(out.summary.contains("set Presence"));
    }
}
