//! Per-iteration rule decision and track-rule matching (issue #754).
//!
//! Mechanical split of `poll_once.rs`: the `RuleDecision` policy (quiet hours
//! first, then track rules), the decision assemblers, and the track-rule
//! walker (conditions, substring/glob fields, schedule windows). Verbatim
//! motion, no behaviour change.

use crate::config::{AppConfig, PresencePair, TrackRuleMatchKind};
use crate::teams::{GATE_REASON_QUIET_HOURS, GATE_REASON_TRACK_RULE};

/// The per-iteration rule decision, factored out of `process_track` so EVERY
/// status write — the playing write, the paused clear and the no-track clear —
/// consults the same policy (finding PollCore#2, issue #570; findings #634).
///
/// 4.5 could only SUPPRESS: a matched rule with an empty `replacement_status`
/// gated the write exactly like a busy/meeting presence gate. Finding #634
/// widens the same decision into an ACTION: a non-empty replacement posts that
/// text instead, and a validated presence pair moves the user's Teams bubble.
///
/// Issue #866 extends the action with a `preferred_presence` pair: when a
/// matched rule carries no presence pair of its own but the user opted into
/// the preferred-presence feature, that pair rides through the same tail and
/// drives the `setUserPreferredPresence` endpoint instead of the ephemeral
/// `setPresence` session. The two endpoints have different Graph contracts
/// (no `sessionId`, different rate limit, different documented pairs), so the
/// caller routes on which field is `Some`.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct RuleDecision {
    /// The `presence-gated` reason of the matched rule
    /// ([`GATE_REASON_QUIET_HOURS`] / [`GATE_REASON_TRACK_RULE`]); `None` when
    /// no enabled rule matched.
    pub(crate) reason: Option<&'static str>,
    /// Non-empty replacement text to post instead of the formatted status.
    pub(crate) replacement: Option<String>,
    /// The `setPresence` pair the matched rule wants armed (finding #634).
    /// `None` = the rule does not touch presence.
    pub(crate) presence: Option<PresencePair>,
    /// Issue #866: the `setUserPreferredPresence` pair to arm in lieu of
    /// `presence` when the rule does not carry its own pair. `None` when the
    /// feature is off, the user's manual status is in force, or the rule
    /// already names a pair (rule presence wins). The two fields cannot both
    /// be `Some` — `decision_from` keeps the invariant.
    pub(crate) preferred_presence: Option<PresencePair>,
}

impl RuleDecision {
    /// Whether the matched rule suppresses the write: a matched rule with no
    /// replacement text. Suppression is the 4.5 semantics, now shared by quiet
    /// hours and track rules.
    pub(crate) fn suppresses(&self) -> bool {
        self.reason.is_some() && self.replacement.is_none()
    }
}

/// The rule decision for an artist/title pair on the CURRENT local clock. The
/// no-track clear path passes empty strings, so only quiet hours and match-all
/// rules (both substrings empty) can suppress a clear.
pub(crate) fn rule_gate(
    config: &Option<std::sync::Arc<AppConfig>>,
    artist: &str,
    title: &str,
) -> RuleDecision {
    let (now_minutes, weekday) = super::gate::local_minutes_and_weekday();
    rule_gate_at(config, now_minutes, weekday, artist, title)
}

/// [`rule_gate`] with an explicit clock, so the mid-track re-check can
/// re-project it (a long-lived track spans quiet-hours boundaries) without a
/// second live-clock read.
///
/// Precedence is quiet hours first, then track rules — the same order the
/// suppression decision has always used.
pub(crate) fn rule_gate_at(
    config: &Option<std::sync::Arc<AppConfig>>,
    now_minutes: u16,
    weekday: u8,
    artist: &str,
    title: &str,
) -> RuleDecision {
    let ctx = TrackRuleContext {
        artist,
        title,
        ..TrackRuleContext::default()
    };
    rule_gate_at_with_ctx(config, now_minutes, weekday, &ctx)
}

/// Issue #868: the rich-context variant. The legacy `rule_gate_at` is
/// the thin wrapper this delegates to; the live `process_track` calls
/// this so a track / episode's album / show / device / playlist-uri /
/// duration feed into the rule walker.
pub(crate) fn rule_gate_at_with_ctx(
    config: &Option<std::sync::Arc<AppConfig>>,
    now_minutes: u16,
    weekday: u8,
    ctx: &TrackRuleContext<'_>,
) -> RuleDecision {
    let Some(cfg) = config.as_ref() else {
        return RuleDecision::default();
    };
    // Issue #869: the rule walker reads the EFFECTIVE config — the
    // active profile's `track_rules` overlay REPLACES the base list,
    // and its `preferred_presence` overlay feeds the same
    // preferred-presence gate the base config does. A profile switch
    // is a config-shaped change with the same contract as editing the
    // base values mid-track.
    let effective = crate::config::effective_snapshot(cfg);
    // Issue #866: the preferred-presence pair rides the rule decision so the
    // matching tail can route it to `setUserPreferredPresence`. Disabled when
    // the user opted out, the user is in a manual-status window, or the
    // config stored an unsupported pair (clamp clears both fields and turns
    // the feature off; `preferred_presence_pair` mirrors the same logic).
    //
    // The pair is only meaningful when a rule matches — outside the rule and
    // snooze paths the app leaves the user's Teams bubble alone (the default
    // listening session is the only `setPresence` arm). The empty-decision
    // branch intentionally drops `preferred`, mirroring the spec's
    // "rule-gate + snooze" scope.
    let preferred = crate::config::preferred_presence_pair(
        &effective.teams,
        effective.teams.respect_manual_status,
    );
    if let Some(entry) =
        super::gate::matching_quiet_hours(&effective.status_rules, now_minutes, weekday)
    {
        return decision_from(
            GATE_REASON_QUIET_HOURS,
            &entry.replacement_status,
            &entry.presence_availability,
            &entry.presence_activity,
            preferred,
        );
    }
    match matching_track_rule_at_with_ctx(&effective.status_rules, now_minutes, weekday, ctx) {
        Some(rule) => decision_from_rule(rule, preferred),
        // No rule match: preferred presence is scoped to rules and snoozes.
        // The default listening session is the only `setPresence` arm that
        // runs when no rule fires; preferred presence would be a regression
        // outside that scope (issue #866 acceptance criteria).
        None => RuleDecision::default(),
    }
}

/// Issue #868: assemble the rule decision from the matched rule's
/// `action`, NOT just the legacy flat fields — but a `Suppress` rule
/// with a populated legacy `replacement_status` keeps the documented
/// "post this text instead of the track template" behaviour so the
/// pre-#868 Settings UI does not silently lose its rule text. The
/// `action` field's `Replace { status }` / `Presence { availability,
/// activity }` variants are the canonical path; the legacy flat
/// fields are the fallback for users who edited their config (or used
/// the Settings picker) before the `action` enum existed.
pub(crate) fn decision_from_rule(
    rule: &crate::config::TrackRuleEntry,
    preferred: Option<PresencePair>,
) -> RuleDecision {
    match &rule.action {
        crate::config::TrackRuleAction::Suppress => {
            // Legacy flat-field fallback: a `replacement_status` with
            // no `action: Replace` still posts the user's fixed text.
            // Same idea for the presence pair — a populated
            // presence_availability / presence_activity with no
            // `action: Presence` keeps applying the legacy pair.
            decision_from(
                GATE_REASON_TRACK_RULE,
                &rule.replacement_status,
                &rule.presence_availability,
                &rule.presence_activity,
                preferred,
            )
        }
        crate::config::TrackRuleAction::Replace { status } => {
            decision_from(GATE_REASON_TRACK_RULE, status, "", "", preferred)
        }
        crate::config::TrackRuleAction::SnoozeMinutes { .. } => {
            // The snooze is an orthogonal effect the rule walker
            // arms through `TrackRuleAction` — the decision itself
            // is still the "suppress for this track" rule gate, so
            // the existing `presence-gated` emitter does not need to
            // change. The snooze fires when `process_track` consumes
            // the matched rule.
            decision_from(GATE_REASON_TRACK_RULE, "", "", "", preferred)
        }
        crate::config::TrackRuleAction::Profile { .. } => {
            // Same shape as `SnoozeMinutes`: the profile switch is an
            // orthogonal effect; the rule gate decision stays
            // "presence-gated, suppress this track".
            decision_from(GATE_REASON_TRACK_RULE, "", "", "", preferred)
        }
        crate::config::TrackRuleAction::Presence {
            availability,
            activity,
        } => decision_from(
            GATE_REASON_TRACK_RULE,
            "",
            availability,
            activity,
            preferred,
        ),
    }
}

/// Assemble one rule's action. An empty replacement means "suppress"; an empty
/// or unsupported presence pair means "don't touch presence" (the same
/// normalization `config::clamp_rules` applies at the IPC boundary, repeated
/// here so an in-memory config that skipped the clamp can never send an
/// unsupported pair to Graph).
///
/// Issue #866: `preferred` is the fallback used when the matched rule carries
/// no presence pair of its own AND the user opted into the preferred-presence
/// feature. The rule's own pair wins (rule presence IS the user's instruction
/// for this track/window); the preferred pair is the user's standing
/// instruction otherwise.
pub(crate) fn decision_from(
    reason: &'static str,
    replacement_status: &str,
    presence_availability: &str,
    presence_activity: &str,
    preferred: Option<PresencePair>,
) -> RuleDecision {
    RuleDecision {
        reason: Some(reason),
        replacement: (!replacement_status.is_empty()).then(|| replacement_status.to_string()),
        presence: crate::config::normalize_presence_pair(presence_availability, presence_activity),
        preferred_presence: preferred.filter(|_| {
            // Rule presence wins — `decision_from` is the only place the two
            // fields share a call site, so the invariant is local.
            crate::config::normalize_presence_pair(presence_availability, presence_activity)
                .is_none()
        }),
    }
}

/// Issue #868: the inputs the new rule dimensions look at, decoupled from
/// `TrackInfo` so the dry-run tester in `commands::rules::explain_rules`
/// can drive the rule walker with synthetic data (a typed fake track
/// rather than a real Spotify `TrackInfo`). Every field is optional —
/// a rule that doesn't look at album passes `None` for album; a rule
/// that doesn't look at device passes `None` for device; the dry-run
/// tester simply mirrors what the real `process_track` path would
/// provide.
#[derive(Debug, Clone, Default)]
pub(crate) struct TrackRuleContext<'a> {
    pub artist: &'a str,
    pub title: &'a str,
    pub album: &'a str,
    pub show: &'a str,
    pub device: &'a str,
    pub playlist_uri: &'a str,
    pub duration_ms: u64,
}

/// Issue #432 / issue #868: track-rule match. Each non-empty condition
/// must hold (case-insensitive); an empty condition matches anything.
/// `match_kind` only governs `artist_substring` / `track_substring` —
/// the album / show / device / playlist-uri extensions stay substring
/// matches because they are extension surfaces, not primary
/// identifiers, and the Settings UI only exposes a substring field for
/// them. `negate` flips the result so an empty match list still wins
/// when the negation's conditions match. Pure so the matching
/// semantics are unit-testable.
pub(crate) fn track_rule_hit(
    rule: &crate::config::TrackRuleEntry,
    ctx: &TrackRuleContext<'_>,
) -> bool {
    if !rule.enabled {
        return false;
    }
    let matched = track_rule_conditions_match(rule, ctx);
    if rule.negate {
        !matched
    } else {
        matched
    }
}

/// Evaluate the combined conditions WITHOUT applying `negate`. Issue
/// #868: shared between the live walker and the dry-run tester so the
/// "what would fire" projection and the actual firing share one
/// definition of "matched".
pub(crate) fn track_rule_conditions_match(
    rule: &crate::config::TrackRuleEntry,
    ctx: &TrackRuleContext<'_>,
) -> bool {
    // Duration gate (issue #868). `0` disables the gate so the legacy
    // `min_duration_seconds` absent default continues to mean "every
    // duration is OK".
    if rule.min_duration_seconds > 0
        && ctx.duration_ms < u64::from(rule.min_duration_seconds) * 1000
    {
        return false;
    }
    if !track_rule_substring_field_matches(
        ctx.album,
        &rule.album_substring,
        TrackRuleMatchKind::Substring,
    ) {
        return false;
    }
    if !track_rule_substring_field_matches(
        ctx.show,
        &rule.show_substring,
        TrackRuleMatchKind::Substring,
    ) {
        return false;
    }
    if !track_rule_substring_field_matches(
        ctx.device,
        &rule.device_substring,
        TrackRuleMatchKind::Substring,
    ) {
        return false;
    }
    if !track_rule_substring_field_matches(
        ctx.playlist_uri,
        &rule.playlist_uri,
        TrackRuleMatchKind::Substring,
    ) {
        return false;
    }
    let artist_ok =
        track_rule_substring_field_matches(ctx.artist, &rule.artist_substring, rule.match_kind);
    let title_ok =
        track_rule_substring_field_matches(ctx.title, &rule.track_substring, rule.match_kind);
    artist_ok && title_ok
}

/// A single field comparison honouring the rule's `match_kind`. Empty
/// patterns always match — the documented "match anything" behaviour so
/// a rule with only one field set still works.
pub(crate) fn track_rule_substring_field_matches(
    haystack: &str,
    pattern: &str,
    kind: crate::config::TrackRuleMatchKind,
) -> bool {
    if pattern.is_empty() {
        return true;
    }
    match kind {
        crate::config::TrackRuleMatchKind::Substring => {
            haystack.to_lowercase().contains(&pattern.to_lowercase())
        }
        crate::config::TrackRuleMatchKind::Exact => haystack.eq_ignore_ascii_case(pattern),
        crate::config::TrackRuleMatchKind::Glob => glob_match_ignore_ascii_case(pattern, haystack),
    }
}

/// Case-insensitive `glob`-style match: `*` matches any run (including
/// empty), `?` matches exactly one character, all other characters match
/// themselves literally. Anchored on both ends. Issue #868: deliberately
/// simple — no `[abc]` / `[!abc]` / backslash-escape handling — so the
/// Settings UI can preview the pattern without exposing a syntax that
/// the runtime cannot parse. `O(|pattern| * |haystack|)` time, `O(|haystack|)`
/// space — plenty for the 128-char patterns the Settings UI exposes.
pub(crate) fn glob_match_ignore_ascii_case(pattern: &str, haystack: &str) -> bool {
    let pat = pattern.as_bytes();
    let txt = haystack.as_bytes();
    // `prev[j]` = "the pattern so far matched the first `j` chars of
    // haystack". Rolling array lets us reuse one row per pattern char.
    let mut prev: Vec<bool> = vec![false; txt.len() + 1];
    prev[0] = true;
    for (i, &pb) in pat.iter().enumerate() {
        let mut curr = vec![false; txt.len() + 1];
        if pb == b'*' {
            // `*` matches the empty string AND any suffix of every
            // position the previous row already accepted.
            for j in 0..=txt.len() {
                curr[j] = prev[j] || (j > 0 && curr[j - 1]);
            }
        } else {
            for j in 1..=txt.len() {
                let char_matches = pb == b'?' || pb.eq_ignore_ascii_case(&txt[j - 1]);
                curr[j] = prev[j - 1] && char_matches;
            }
        }
        // Sanity: bail out early when nothing in the row is reachable
        // so a long non-matching pattern does not iterate the rest of
        // the haystack. (Cosmetic; the function still terminates
        // without this guard.)
        if !curr.iter().any(|&b| b) {
            return false;
        }
        prev = curr;
        let _ = i;
    }
    prev[txt.len()]
}

/// S4 (issue #672): whether a rule's `days` / `start_minutes` / `end_minutes`
/// window contains the given local time. Delegates to
/// [`schedule_window_contains`] — the same night-owning matcher quiet hours
/// use — so both halves of the rules model agree on midnight-crossing
/// windows: each half is tested against the day it falls on (the morning
/// half against the previous ISO day, wrapping 1→7). An empty `days`
/// applies every day, the window is `[start, end)`, and `start == end`
/// matches nothing. `end_minutes == 1440` is the end of the day, so the
/// default window covers every minute. Issue #868: `pub(crate)` because
/// `commands::rules::explain_rules` runs the same walker the live
/// `process_track` path uses.
pub(crate) fn track_rule_schedule_matches(
    rule: &crate::config::TrackRuleEntry,
    now_minutes: u16,
    weekday: u8,
) -> bool {
    let now = u32::from(now_minutes.min(1439));
    let start = rule
        .start_minutes
        .min(crate::config::TRACK_RULE_DAY_MINUTES);
    let end = rule.end_minutes.min(crate::config::TRACK_RULE_DAY_MINUTES);
    super::gate::schedule_window_contains(&rule.days, start, end, now, weekday)
}

/// Issue #432 + S4 (issue #672) + issue #868: the first enabled track
/// rule whose conditions match this track AND whose schedule contains
/// the given local time. Array order is priority — the first match
/// wins — so the Settings card states that explicitly and offers
/// move-up/move-down controls. The `SyntheticTrack` analogue
/// (`matching_track_rule_at_with_ctx`) is the dry-run hook
/// `commands::rules::explain_rules` calls; this helper is the legacy
/// real-track entry point that builds the rich context from the
/// `TrackInfo` already in scope. `#[cfg(test)]` because the live path
/// now calls [`matching_track_rule_at_with_ctx`] directly with the
/// real `TrackRuleContext`, so the legacy 4-arg wrapper exists only
/// to keep the legacy unit tests below readable.
#[cfg(test)]
fn matching_track_rule_at<'a>(
    rules: &'a crate::config::StatusRulesConfig,
    now_minutes: u16,
    weekday: u8,
    artist: &str,
    title: &str,
) -> Option<&'a crate::config::TrackRuleEntry> {
    let ctx = TrackRuleContext {
        artist,
        title,
        ..TrackRuleContext::default()
    };
    matching_track_rule_at_with_ctx(rules, now_minutes, weekday, &ctx)
}

/// Issue #868: the rich-context variant of [`matching_track_rule_at`].
/// Used by both `process_track` (with the album / show / device /
/// playlist-uri the live path now feeds in) and
/// `commands::rules::explain_rules` (with the synthetic track the
/// Settings dry-run tester types in).
pub(crate) fn matching_track_rule_at_with_ctx<'a>(
    rules: &'a crate::config::StatusRulesConfig,
    now_minutes: u16,
    weekday: u8,
    ctx: &TrackRuleContext<'_>,
) -> Option<&'a crate::config::TrackRuleEntry> {
    rules.track_rules.iter().find(|rule| {
        track_rule_schedule_matches(rule, now_minutes, weekday) && track_rule_hit(rule, ctx)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::teams::{GATE_REASON_QUIET_HOURS, GATE_REASON_TRACK_RULE};

    /// Issue #432: track-rule matching — case-insensitive substrings,
    /// empty-matches-all, disabled rules never hit, first-match wins.
    #[test]
    fn test_track_rule_hit_matching() {
        use crate::config::{StatusRulesConfig, TrackRuleEntry};
        let rule = |enabled: bool, artist: &str, track: &str| TrackRuleEntry {
            enabled,
            artist_substring: artist.to_string(),
            track_substring: track.to_string(),
            replacement_status: String::new(),
            ..TrackRuleEntry::default()
        };
        // Inline the `TrackRuleContext` so the borrow checker does not
        // need to chase a closure's lifetime through every call site.
        assert!(track_rule_hit(
            &rule(true, "lofi", ""),
            &TrackRuleContext {
                artist: "LoFi Girl",
                title: "Anything",
                ..TrackRuleContext::default()
            },
        ));
        assert!(track_rule_hit(
            &rule(true, "", "rain"),
            &TrackRuleContext {
                artist: "Anyone",
                title: "Rain Sounds",
                ..TrackRuleContext::default()
            },
        ));
        assert!(!track_rule_hit(
            &rule(true, "lofi", "rain"),
            &TrackRuleContext {
                artist: "Lofi Girl",
                title: "Sunshine",
                ..TrackRuleContext::default()
            },
        ));
        assert!(!track_rule_hit(
            &rule(false, "", ""),
            &TrackRuleContext {
                artist: "Anyone",
                title: "Anything",
                ..TrackRuleContext::default()
            },
        ));
        let rules = StatusRulesConfig {
            quiet_hours: Vec::new(),
            // First rule disabled (never hits even though empty matches
            // all) so the enabled second rule wins for artist "b".
            track_rules: vec![rule(false, "", ""), rule(true, "b", "")],
            ..StatusRulesConfig::default()
        };
        // S4: the schedule is part of the match — 10:00 on a Monday is inside
        // the default (every day, 0..1440) window, so the substring result is
        // unchanged.
        let hit =
            matching_track_rule_at(&rules, 600, 1, "b", "anything").expect("must hit second rule");
        assert_eq!(hit.artist_substring, "b");
        assert!(matching_track_rule_at(&rules, 600, 1, "a", "zzz").is_none());
    }

    /// S4 (issue #672): a track rule's `days` / `start_minutes` /
    /// `end_minutes` schedule reuses quiet hours' window semantics. Both
    /// boundaries of a same-day window, the empty-`days` case and the
    /// wrap-around pair are pinned here: a rule whose window does not contain
    /// "now" must not match.
    #[test]
    fn test_track_rule_schedule_matching() {
        use crate::config::TrackRuleEntry;
        let rule = |days: Vec<u8>, start: u32, end: u32| TrackRuleEntry {
            enabled: true,
            days,
            start_minutes: start,
            end_minutes: end,
            ..TrackRuleEntry::default()
        };

        // Empty `days` applies every day, and the default window (0, 1440)
        // covers every minute of it.
        let every_day = rule(Vec::new(), 0, crate::config::TRACK_RULE_DAY_MINUTES);
        for weekday in 1..=7 {
            assert!(track_rule_schedule_matches(&every_day, 0, weekday));
            assert!(track_rule_schedule_matches(&every_day, 1439, weekday));
        }

        // A weekday filter excludes every day it does not name.
        let mondays = rule(vec![1], 0, crate::config::TRACK_RULE_DAY_MINUTES);
        assert!(track_rule_schedule_matches(&mondays, 600, 1));
        assert!(!track_rule_schedule_matches(&mondays, 600, 2));

        // A same-day window is `[start, end)` in minutes since midnight.
        let work = rule(Vec::new(), 480, 1020);
        assert!(!track_rule_schedule_matches(&work, 479, 3));
        assert!(track_rule_schedule_matches(&work, 480, 3));
        assert!(track_rule_schedule_matches(&work, 1019, 3));
        assert!(!track_rule_schedule_matches(&work, 1020, 3));

        // A wrap-around window (22:00→07:00) is honoured like quiet hours', and
        // its end boundary is exclusive too.
        let night = rule(Vec::new(), 1320, 420);
        assert!(!track_rule_schedule_matches(&night, 1319, 3));
        assert!(track_rule_schedule_matches(&night, 1320, 3));
        assert!(track_rule_schedule_matches(&night, 1439, 3));
        assert!(track_rule_schedule_matches(&night, 0, 3));
        assert!(track_rule_schedule_matches(&night, 419, 3));
        assert!(!track_rule_schedule_matches(&night, 420, 3));

        // Issue #794: a Monday-only midnight-crossing window is owned by the
        // night it starts on — active Mon 23:00 + Tue 03:00, inactive Mon
        // 03:00 + Tue 23:00.
        let mon_night = rule(vec![1], 1320, 420);
        assert!(track_rule_schedule_matches(&mon_night, 1380, 1));
        assert!(track_rule_schedule_matches(&mon_night, 180, 2));
        assert!(!track_rule_schedule_matches(&mon_night, 180, 1));
        assert!(!track_rule_schedule_matches(&mon_night, 1380, 2));

        // `start == end` is an empty window: it matches nothing, exactly as in
        // quiet hours.
        let empty = rule(Vec::new(), 600, 600);
        assert!(!track_rule_schedule_matches(&empty, 600, 3));
    }

    /// S4 (issue #672): array order is PRIORITY, and this test can OBSERVE it:
    /// the first two rules overlap on `[600, 1020)`, so at 10:00 both match the
    /// same track and only the order decides which action reaches Teams (a
    /// last-match-wins or any-match implementation posts "Evening" instead of
    /// "Morning"). A first rule whose window excludes "now" yields to the later
    /// one, and a same-window pair is decided purely by order too.
    #[test]
    fn test_track_rule_first_match_wins_with_overlapping_windows() {
        use crate::config::{AppConfig, StatusRulesConfig, TrackRuleEntry};
        let rule = |text: &str, days: Vec<u8>, start: u32, end: u32| TrackRuleEntry {
            enabled: true,
            artist_substring: "lofi".to_string(),
            days,
            start_minutes: start,
            end_minutes: end,
            replacement_status: text.to_string(),
            ..TrackRuleEntry::default()
        };
        let with_rules = |rules: Vec<TrackRuleEntry>| {
            Some(std::sync::Arc::new(AppConfig {
                status_rules: StatusRulesConfig {
                    quiet_hours: Vec::new(),
                    track_rules: rules,
                    ..StatusRulesConfig::default()
                },
                ..AppConfig::default()
            }))
        };

        let overlapping = with_rules(vec![
            rule("Morning", Vec::new(), 480, 1020),
            // Overlaps the first rule on [600, 1020).
            rule(
                "Evening",
                Vec::new(),
                600,
                crate::config::TRACK_RULE_DAY_MINUTES,
            ),
        ]);
        assert_eq!(
            rule_gate_at(&overlapping, 600, 3, "Lofi Girl", "Rain Sounds")
                .replacement
                .as_deref(),
            Some("Morning"),
            "both rules cover 10:00, so the FIRST one wins"
        );
        assert_eq!(
            rule_gate_at(&overlapping, 1200, 3, "Lofi Girl", "Rain Sounds")
                .replacement
                .as_deref(),
            Some("Evening"),
            "a rule whose window excludes 'now' yields to the next one"
        );
        assert_eq!(
            rule_gate_at(&overlapping, 60, 3, "Lofi Girl", "Rain Sounds"),
            RuleDecision::default(),
            "outside both windows nothing matches"
        );

        // Same window, different text: nothing but the order can decide.
        let identical_windows = with_rules(vec![
            rule("First", Vec::new(), 600, 1440),
            rule("Second", Vec::new(), 600, 1440),
        ]);
        assert_eq!(
            rule_gate_at(&identical_windows, 700, 3, "Lofi Girl", "Rain Sounds")
                .replacement
                .as_deref(),
            Some("First"),
            "with identical windows the first rule in the array wins"
        );
    }

    /// Finding PollCore#2 (issue #570): one rule decision for every write path.
    #[test]
    fn test_rule_gate_suppress_replace_and_no_rule() {
        use crate::config::{AppConfig, StatusRulesConfig, TrackRuleEntry};
        let rule = |enabled: bool, artist: &str, track: &str, replacement: &str| TrackRuleEntry {
            enabled,
            artist_substring: artist.to_string(),
            track_substring: track.to_string(),
            replacement_status: replacement.to_string(),
            ..TrackRuleEntry::default()
        };
        let config_with = |rules: Vec<TrackRuleEntry>| {
            Some(std::sync::Arc::new(AppConfig {
                status_rules: StatusRulesConfig {
                    quiet_hours: Vec::new(),
                    track_rules: rules,
                    ..StatusRulesConfig::default()
                },
                ..AppConfig::default()
            }))
        };

        assert_eq!(
            rule_gate(
                &config_with(vec![rule(true, "lofi", "", "")]),
                "LoFi Girl",
                "Anything"
            ),
            RuleDecision {
                reason: Some(GATE_REASON_TRACK_RULE),
                ..Default::default()
            },
            "an empty replacement suppresses the write"
        );
        assert_eq!(
            rule_gate(
                &config_with(vec![rule(true, "lofi", "", "Focus time")]),
                "LoFi Girl",
                "Anything"
            ),
            RuleDecision {
                reason: Some(GATE_REASON_TRACK_RULE),
                replacement: Some("Focus time".to_string()),
                ..Default::default()
            },
            "a non-empty replacement becomes the posted text"
        );
        assert_eq!(
            rule_gate(&config_with(vec![rule(false, "", "", "")]), "Anyone", "X"),
            RuleDecision::default(),
            "a disabled rule never gates"
        );
        assert_eq!(
            rule_gate(&None, "Anyone", "X"),
            RuleDecision::default(),
            "no config means no rule"
        );
        assert_eq!(
            rule_gate(&config_with(vec![rule(true, "lofi", "", "")]), "", ""),
            RuleDecision::default(),
            "a scoped rule must not suppress a no-track clear"
        );
        assert_eq!(
            rule_gate(&config_with(vec![rule(true, "", "", "")]), "", ""),
            RuleDecision {
                reason: Some(GATE_REASON_TRACK_RULE),
                ..Default::default()
            },
            "a match-all rule suppresses a no-track clear too"
        );
    }

    // ---------------------------------------------------------------
    // Findings #634/#635/#636/#637 (issues #634/#635/#636/#637): the rule
    // presence action, the manual-status policy, the bounded presence session
    // and the out-of-office gate.
    // ---------------------------------------------------------------

    /// Finding #634: a rule's action travels with the decision — suppression,
    /// replacement text and presence pair — for track rules AND quiet hours.
    #[test]
    fn test_rule_actions_suppress_replace_and_set_presence() {
        use crate::config::{AppConfig, QuietHoursEntry, TrackRuleEntry};
        let cfg = |quiet: Vec<QuietHoursEntry>, rules: Vec<TrackRuleEntry>| {
            let mut c = AppConfig::default();
            c.status_rules.quiet_hours = quiet;
            c.status_rules.track_rules = rules;
            Some(std::sync::Arc::new(c))
        };
        let wy = |avail: &str, act: &str| (avail.to_string(), act.to_string());

        // (a) A track rule with no replacement suppresses and carries its pair.
        let (avail, act) = wy("DoNotDisturb", "Presenting");
        let decision = rule_gate_at(
            &cfg(
                vec![],
                vec![TrackRuleEntry {
                    enabled: true,
                    artist_substring: "lofi".to_string(),
                    presence_availability: avail,
                    presence_activity: act,
                    ..TrackRuleEntry::default()
                }],
            ),
            600,
            3,
            "LoFi Girl",
            "Anything",
        );
        assert!(decision.suppresses(), "empty replacement = suppress");
        assert_eq!(decision.reason, Some(GATE_REASON_TRACK_RULE));
        assert_eq!(
            decision
                .presence
                .expect("the rule must carry its pair")
                .activity,
            "Presenting"
        );

        // (b) A non-empty replacement is NOT a suppression: the text is posted.
        let decision = rule_gate_at(
            &cfg(
                vec![],
                vec![TrackRuleEntry {
                    enabled: true,
                    replacement_status: "Focus time".to_string(),
                    ..TrackRuleEntry::default()
                }],
            ),
            600,
            3,
            "Anyone",
            "Anything",
        );
        assert!(!decision.suppresses());
        assert_eq!(decision.replacement.as_deref(), Some("Focus time"));
        assert!(
            decision.presence.is_none(),
            "no pair = don't touch presence"
        );

        // (c) Quiet hours carry the same three actions, and win over a track
        //     rule (the documented precedence).
        let decision = rule_gate_at(
            &cfg(
                vec![QuietHoursEntry {
                    enabled: true,
                    start_minutes: 540,
                    end_minutes: 1020,
                    replacement_status: "🌙 Back at 09:00".to_string(),
                    presence_availability: "Away".to_string(),
                    presence_activity: "Away".to_string(),
                    ..QuietHoursEntry::default()
                }],
                vec![TrackRuleEntry {
                    enabled: true,
                    replacement_status: "from the track rule".to_string(),
                    ..TrackRuleEntry::default()
                }],
            ),
            600,
            3,
            "Anyone",
            "Anything",
        );
        assert_eq!(decision.reason, Some(GATE_REASON_QUIET_HOURS));
        assert_eq!(decision.replacement.as_deref(), Some("🌙 Back at 09:00"));
        assert_eq!(
            decision.presence.expect("quiet hours pair").availability,
            "Away"
        );

        // (d) An unsupported pair in an unclamped in-memory config is dropped
        //     rather than sent to Graph.
        let decision = rule_gate_at(
            &cfg(
                vec![],
                vec![TrackRuleEntry {
                    enabled: true,
                    presence_availability: "DoNotDisturb".to_string(),
                    presence_activity: "DoNotDisturb".to_string(),
                    ..TrackRuleEntry::default()
                }],
            ),
            600,
            3,
            "",
            "",
        );
        assert!(decision.suppresses());
        assert!(decision.presence.is_none());

        // (e) Outside the quiet window nothing matches.
        let outside = cfg(
            vec![QuietHoursEntry {
                enabled: true,
                start_minutes: 540,
                end_minutes: 1020,
                presence_availability: "Away".to_string(),
                presence_activity: "Away".to_string(),
                ..QuietHoursEntry::default()
            }],
            vec![],
        );
        let decision = rule_gate_at(&outside, 1200, 3, "", "");
        assert!(!decision.suppresses());
        assert!(decision.presence.is_none());
    }

    /// Issue #795: the no-track decision carries the quiet-hours pair (empty
    /// artist/title = the no-track call), so the hoisted `handle_no_track`
    /// presence block arms it instead of clearing; and the pair retires when
    /// window ends.
    #[test]
    fn test_no_track_rule_presence_decision_for_quiet_hours() {
        use crate::config::{AppConfig, QuietHoursEntry};
        let cfg = |quiet: Vec<QuietHoursEntry>| {
            let mut c = AppConfig::default();
            c.status_rules.quiet_hours = quiet;
            c.teams.availability_sync = true;
            Some(std::sync::Arc::new(c))
        };
        let entry = |start: u16, end: u16, avail: &str, act: &str| QuietHoursEntry {
            enabled: true,
            start_minutes: start,
            end_minutes: end,
            presence_availability: avail.to_string(),
            presence_activity: act.to_string(),
            ..QuietHoursEntry::default()
        };
        // Inside the window (empty artist/title = the no-track call): the
        // decision carries the pair, so the no-track presence block takes the
        // `rule_presence_backoff` arm.
        let config = cfg(vec![entry(540, 1020, "Away", "Away")]);
        let decision = rule_gate_at(&config, 600, 3, "", "");
        assert_eq!(decision.reason, Some(GATE_REASON_QUIET_HOURS));
        assert!(
            decision.presence.is_some(),
            "quiet-hours pair must ride the no-track decision so handle_no_track arms it (issue #795)"
        );
        // Outside the window the decision carries nothing: the no-pair case
        // keeps the clear, so the stale pair retires at window end.
        let decision = rule_gate_at(&config, 1200, 3, "", "");
        assert!(
            decision.presence.is_none(),
            "window end must yield no pair so the no-track path clears (issue #795)"
        );
    }

    /// Issue #795: a scoped track rule cannot match the no-track call (empty
    /// artist/title), so its decision carries no pair and the no-track
    /// presence block keeps the clear — the stale pair retires.
    #[test]
    fn test_no_track_scoped_track_rule_carries_no_pair() {
        use crate::config::{AppConfig, TrackRuleEntry};
        let mut c = AppConfig::default();
        c.status_rules.track_rules = vec![TrackRuleEntry {
            enabled: true,
            artist_substring: "lofi".to_string(),
            presence_availability: "DoNotDisturb".to_string(),
            presence_activity: "Presenting".to_string(),
            ..TrackRuleEntry::default()
        }];
        c.teams.availability_sync = true;
        let decision = rule_gate_at(&Some(std::sync::Arc::new(c)), 600, 3, "", "");
        assert!(
            decision.presence.is_none(),
            "a scoped track rule must not match the empty no-track call, so the stale pair still clears (issue #795)"
        );
    }
}
