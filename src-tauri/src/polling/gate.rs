//! Quiet-hours, snooze, and gate-recheck predicates (issue #754).
//!
//! Mechanical split of `poll_once.rs`: the out-of-office gate, quiet-hours
//! evaluation and the polling pause, the tray snooze gate, the 304 re-arm
//! block, and the presence-gate re-check cadence. Verbatim motion, no
//! behaviour change.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Local, TimeZone, Utc};

use crate::config::AppConfig;
use crate::AppState;

/// Finding #637 (issue #637): whether the out-of-office reason participates in
/// the gate this iteration.
///
/// Opt-in through `teams.gate_when_out_of_office`, and overridable per rule: a
/// rule that carries its own presence action is an explicit instruction for
/// this track/window, so it wins over the OOO default (finding #634). Busy /
/// Do-Not-Disturb / in-a-call always gate regardless.
pub(crate) fn ooo_gate_enabled(
    config: &Option<std::sync::Arc<AppConfig>>,
    rule_has_presence_action: bool,
) -> bool {
    !rule_has_presence_action
        && config
            .as_ref()
            .map(|c| c.teams.gate_when_out_of_office)
            .unwrap_or(false)
}

/// Issue #432: quiet-hours evaluation. `now_minutes` is local minutes-since-
/// midnight and `weekday` the ISO weekday number 1 (Mon)..=7 (Sun), passed
/// in so the pure predicate stays unit-testable without clock injection.
/// An entry matches when it is enabled and the schedule window (see
/// [`schedule_window_contains`] for the night-owning weekday rule) contains
/// the local clock. Minutes are clamped to 0..=1439 so a hand-edited config
/// can't wedge the comparison.
pub(crate) fn quiet_hours_active(
    rules: &crate::config::StatusRulesConfig,
    now_minutes: u16,
    weekday: u8,
) -> bool {
    matching_quiet_hours(rules, now_minutes, weekday).is_some()
}

/// Issue #432: the first enabled quiet-hours entry whose window contains the
/// given local time, if any. Extracted from [`quiet_hours_active`] (finding
/// #634, issue #634) because the ACTIVE ENTRY — not just the boolean — carries
/// the rule's replacement text and presence action.
///
/// `now_minutes` is local minutes-since-midnight and `weekday` the ISO weekday
/// number 1 (Mon)..=7 (Sun), passed in so the predicate stays unit-testable
/// without clock injection. An entry matches when it is enabled and the
/// schedule window (see [`schedule_window_contains`] for the night-owning
/// weekday rule) contains the local clock. Minutes are clamped to 0..=1439
/// so a hand-edited config can't wedge the comparison.
pub(crate) fn matching_quiet_hours(
    rules: &crate::config::StatusRulesConfig,
    now_minutes: u16,
    weekday: u8,
) -> Option<&crate::config::QuietHoursEntry> {
    rules
        .quiet_hours
        .iter()
        .find(|entry| quiet_entry_contains(entry, now_minutes, weekday))
}

/// Issue #794: the ONE schedule-window matcher both halves of the rules model
/// share — [`quiet_entry_contains`] and [`track_rule_schedule_matches`] route
/// through it, so quiet hours and track rules cannot disagree about what a
/// midnight-crossing window means. All bounds are minutes-since-midnight in
/// `u32` (callers clamp their own field widths before delegating); `weekday`
/// is the ISO weekday 1 (Mon)..=7 (Sun).
///
/// Night-owning semantics: each half of a midnight-crossing window is tested
/// against the day it falls on. The evening half (`now >= start`) belongs to
/// the day it starts on — `days` must contain `weekday` — and the morning
/// half (`now < end`) belongs to the PREVIOUS day — `days` must contain the
/// ISO day before `weekday` (wrapping 1→7). A Monday-only 22:00→07:00 window
/// is therefore active Monday 23:00 and Tuesday 03:00, but NOT Monday 03:00
/// (that morning belongs to the Sunday night) nor Tuesday 23:00 (that
/// evening starts Tuesday's night, and Tuesday is not selected). A
/// non-wrapping window is tested against `weekday` alone, and an empty
/// `days` means every day on both halves. `start == end` matches nothing.
pub(crate) fn schedule_window_contains(
    days: &[u8],
    start: u32,
    end: u32,
    now: u32,
    weekday: u8,
) -> bool {
    if start == end {
        return false;
    }
    if start < end {
        return now >= start && now < end && (days.is_empty() || days.contains(&weekday));
    }
    let evening = now >= start && (days.is_empty() || days.contains(&weekday));
    let previous = if weekday <= 1 { 7 } else { weekday - 1 };
    let morning = now < end && (days.is_empty() || days.contains(&previous));
    evening || morning
}

/// Whether ONE quiet-hours entry is active at the given local time: enabled,
/// and the schedule window (see [`schedule_window_contains`] for the
/// night-owning weekday rule) contains the local clock. Minutes are clamped
/// to 0..=1439 so a hand-edited config cannot wedge the comparison, and a
/// zero-length window (`start == end`) matches nothing. Extracted (S4, issue
/// #672) so the "pause polling" gate can ask the question per entry instead
/// of only of the first match.
pub(crate) fn quiet_entry_contains(
    entry: &crate::config::QuietHoursEntry,
    now_minutes: u16,
    weekday: u8,
) -> bool {
    if !entry.enabled {
        return false;
    }
    let now = u32::from(now_minutes.min(1439));
    let start = u32::from(entry.start_minutes.min(1439));
    let end = u32::from(entry.end_minutes.min(1439));
    schedule_window_contains(&entry.days, start, end, now, weekday)
}

/// Finding PollCore#1 (issue #569): whether the mid-track quiet-hours ENTRY
/// must gate the current track — quiet hours are active and this track has not
/// been gated yet (the `!= Some(track_key)` arm is what keeps the gate
/// idempotent: once recorded, the #380 re-check block owns the decision). Pure
/// so the mid-track entry semantics are testable without an `AppHandle`.
pub(crate) fn quiet_gate_entry_due(
    quiet_active: bool,
    gated_track_key: Option<&str>,
    track_key: &str,
) -> bool {
    quiet_active && gated_track_key != Some(track_key)
}

/// Issue #432: local clock projection for [`quiet_hours_active`].
/// Minute-of-day plus ISO weekday (`number_from_monday`, 1..=7).
pub(crate) fn local_minutes_and_weekday() -> (u16, u8) {
    use chrono::{Datelike, Timelike};
    let now = chrono::Local::now();
    let minutes = (now.hour() as u16 * 60 + now.minute() as u16).min(1439);
    (minutes, now.weekday().number_from_monday() as u8)
}

/// Issue #432 / finding PollCore#2 (issue #570): quiet-hours evaluation on the
/// current local clock — the read-side twin of the hoisted `quiet_active`
/// binding in `process_track`, for the path (`handle_no_track`) that has no
/// track to match a rule against and no hoisted decision to consult.
pub(crate) fn quiet_hours_active_now(
    config: &Option<std::sync::Arc<crate::config::AppConfig>>,
) -> bool {
    let (now_minutes, weekday) = local_minutes_and_weekday();
    // Issue #869: the active profile's `track_rules` overlay does NOT
    // touch `quiet_hours`, so quiet-hours resolution still reads the
    // base config here — but we route through `effective_config` so a
    // future profile overlay that does touch quiet hours lands on the
    // same code path without a second migration step.
    config.as_ref().is_some_and(|c| {
        let effective = crate::config::effective_snapshot(c);
        quiet_hours_active(&effective.status_rules, now_minutes, weekday)
    })
}

/// Whether the previous iteration was skipped by [`quiet_pause_iteration`], so
/// the pause and the resume are each logged exactly once. The DECISION is never
/// cached — it is re-derived from the local clock on every iteration, which is
/// what lets the window end by itself. The latch lives on the session (issue
/// #758) so two sessions never share a pause edge.
pub(crate) fn quiet_pause_iteration(
    session: &super::state::SessionState,
    config: &Option<std::sync::Arc<AppConfig>>,
) -> Option<u64> {
    let (now_minutes, weekday) = local_minutes_and_weekday();
    let decision = quiet_pause_at(config, now_minutes, weekday);
    let was_paused = session
        .quiet_pause_latch()
        .swap(decision.is_some(), Ordering::Relaxed);
    if let Some(line) =
        quiet_pause_log_line(decision.is_some(), was_paused, decision.map(|(_, end)| end))
    {
        log::info!("{}", line);
    }
    decision.map(|(seconds, _)| seconds)
}

/// [`quiet_pause_iteration`] with an explicit clock:
/// `Some((sleep_seconds, window_end_minutes))` when polling must be skipped.
///
/// ANY active quiet-hours entry that sets `pause_polling` can assert the pause —
/// quiet hours are not an ordered list for this decision (first-match-wins is
/// the TRACK rules' contract), so a second overlapping window that asks for
/// "stop polling" is not ignored just because an earlier window owns the
/// replacement text. When more than one pausing window is active the reported
/// end is the LATEST of them, so the "paused until …" log line describes the
/// union of the windows instead of understating it.
///
/// The sleep is the configured ceiling (`polling.max_interval_seconds`, clamped
/// to 5..=300 by `config::clamp_polling`), floored at 1 s so a hand-edited 0
/// cannot spin the thread.
pub(crate) fn quiet_pause_at(
    config: &Option<std::sync::Arc<AppConfig>>,
    now_minutes: u16,
    weekday: u8,
) -> Option<(u64, u16)> {
    // Issue #893: share the base pointer when no profile is active — a
    // `quiet_pause_at` call must not deep-copy the lexicon/rules per iteration.
    let cfg = config.as_ref().map(crate::config::effective_snapshot)?;
    let until = cfg
        .status_rules
        .quiet_hours
        .iter()
        .filter(|entry| entry.pause_polling && quiet_entry_contains(entry, now_minutes, weekday))
        .map(|entry| entry.end_minutes)
        .max()?;
    Some((cfg.polling.max_interval_seconds.max(1), until))
}

/// The log line for a pause/resume transition, or `None` when the state did not
/// change — the "once per transition" contract for both edges.
pub(crate) fn quiet_pause_log_line(
    paused_now: bool,
    was_paused: bool,
    until_minutes: Option<u16>,
) -> Option<String> {
    match (paused_now, was_paused) {
        (true, false) => Some(format!(
            "[POLLING] quiet hours: polling paused until {}",
            format_minutes_of_day(until_minutes.unwrap_or(0))
        )),
        (false, true) => Some("[POLLING] quiet hours: polling resumed".to_string()),
        _ => None,
    }
}

/// `HH:MM` for a minutes-since-midnight value; anything past the end of the day
/// folds back into it so a hand-edited config cannot print `24:00`.
pub(crate) fn format_minutes_of_day(minutes: u16) -> String {
    let minutes = minutes.min(1439);
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

// ---------------------------------------------------------------------------
// 4.7.0 (S9, issue #677): the tray snooze ("Pause sync → 30 minutes / 1 hour /
// until tomorrow").
//
// A snooze is a user-chosen deadline stored in `AppConfig::snooze_until` as an
// RFC3339 UTC instant. While it is in the future the polling driver performs no
// Spotify or Graph work at all and sleeps at `polling.max_interval_seconds` —
// the same shape as the quiet-hours pause above (S4): decided before the write
// clocks are loaded and before `poll_once::run`, with the thread never stopped
// or parked, so the deadline is re-evaluated on every iteration and the snooze
// ends by itself.
//
// The deadline arithmetic (including the LOCAL meaning of "until tomorrow")
// lives with the field it produces, in `crate::config`. What is here is the
// decision: skip, resume, or nothing.
// ---------------------------------------------------------------------------

/// What the polling driver must do with an iteration while a snooze may be
/// stored (4.7.0, S9 / issue #677).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SnoozeGate {
    /// Skip the iteration entirely and sleep for this many seconds
    /// (`polling.max_interval_seconds`).
    Skipped(u64),
    /// The stored deadline has passed: run a normal iteration and clear the
    /// field, so the countdown stops and an expired value never lingers on
    /// disk.
    Expired,
    /// No snooze is stored.
    Inactive,
}

/// Whether the previous iteration was skipped by [`snooze_gate`], so the pause
/// and the resume are each logged exactly once. The DECISION is never cached —
/// it is re-derived from the stored deadline on every iteration, which is what
/// lets the snooze end by itself. The latch lives on the session (issue #758)
/// so two sessions never share a snooze edge.
pub(crate) fn snooze_gate(
    session: &super::state::SessionState,
    config: &Option<std::sync::Arc<AppConfig>>,
) -> SnoozeGate {
    let now = Utc::now();
    if let Some((seconds, deadline)) = snooze_pause_at(config, now) {
        if !session.snooze_latch().swap(true, Ordering::Relaxed) {
            log::info!(
                "{}",
                snooze_pause_log_line(
                    &snooze_deadline_hhmm(deadline, &Local),
                    crate::config::snooze_minutes_left((deadline - now).num_seconds()),
                )
            );
        }
        return SnoozeGate::Skipped(seconds);
    }
    if snooze_expired(config, now) {
        if session.snooze_latch().swap(false, Ordering::Relaxed) {
            log::info!("[POLLING] snooze: polling resumed");
        } else {
            log::info!("[POLLING] snooze: the stored deadline has already passed — clearing it");
        }
        return SnoozeGate::Expired;
    }
    session.snooze_latch().store(false, Ordering::Relaxed);
    SnoozeGate::Inactive
}

/// [`snooze_gate`]'s decision with an explicit clock:
/// `Some((sleep_seconds, deadline))` while a snooze is active. Pure, so the
/// skip decision and the sleep value are unit-testable without a Tauri runtime.
///
/// The sleep is `polling.max_interval_seconds`, floored at 1 s so a hand-edited
/// 0 cannot spin the thread — the same floor [`quiet_pause_at`] applies.
pub(crate) fn snooze_pause_at(
    config: &Option<std::sync::Arc<AppConfig>>,
    now: DateTime<Utc>,
) -> Option<(u64, DateTime<Utc>)> {
    let cfg = config.as_ref()?;
    let status = crate::config::snooze_status(cfg, now)?;
    Some((cfg.polling.max_interval_seconds.max(1), status.deadline))
}

/// Whether a snooze is stored but no longer active — the deadline has passed,
/// or the value cannot be parsed at all (4.7.0, S9 / issue #677).
///
/// Distinct from "no snooze stored": only this state asks the driver to persist
/// a clear. Delegates to `config::snooze_expired_deadline` so the tray, the
/// chip, the load-time report and this gate cannot disagree about what "no
/// longer live" means.
pub(crate) fn snooze_expired(
    config: &Option<std::sync::Arc<AppConfig>>,
    now: DateTime<Utc>,
) -> bool {
    config
        .as_ref()
        .is_some_and(|c| crate::config::snooze_expired_deadline(c, now))
}

/// `HH:MM` of a deadline in the given zone, for the pause log line (4.7.0, S9).
/// Local wall-clock time, because that is the clock the user set the snooze
/// against — the stored value is UTC and would read as an arbitrary hour.
pub(crate) fn snooze_deadline_hhmm<Tz: TimeZone>(deadline: DateTime<Utc>, tz: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    deadline.with_timezone(tz).format("%H:%M").to_string()
}

/// The pause log line: until when, and how long is left (4.7.0, S9).
pub(crate) fn snooze_pause_log_line(local_hhmm: &str, minutes_left: i64) -> String {
    format!(
        "[POLLING] snooze: polling paused until {} ({} min left)",
        local_hhmm, minutes_left
    )
}

/// Clears a stored, already-expired `snooze_until` and persists the config
/// (4.7.0, S9 / issue #677).
///
/// Called by the driver on the iteration that observed [`SnoozeGate::Expired`].
/// Runs once per expiry — the clear removes the value that produced the
/// verdict — and holds the config write guard across the write exactly like
/// `commands::config::update_config`, storing the same value that reached disk
/// (the #297 invariant). A failed write leaves the guard untouched, so the next
/// iteration retries rather than believing a deadline is gone.
pub(crate) fn clear_snooze_if_expired(state: &AppState) {
    let mut guard = state.config.get_mut();
    let Some(current) = guard.as_ref().map(|c| (**c).clone()) else {
        return;
    };
    let mut next = current;
    if !crate::config::clamp_snooze(&mut next, Utc::now()) {
        return;
    }
    crate::config::stamp_schema_version(&mut next);
    match crate::config::save_config(&next) {
        Ok(()) => {
            log::info!("[POLLING] snooze: cleared the expired deadline");
            *guard = Some(Arc::new(next));
        }
        Err(e) => log::warn!(
            "[POLLING] snooze: could not clear the expired deadline ({}); retrying next iteration",
            e
        ),
    }
}

/// Issue #790: whether the presence-gate verdict `process_track` recorded for
/// the track on screen also blocks a re-arm on a bodyless 304.
/// `gated_track_key` holds the track key a suppressed write was recorded
/// against; equality with the key still on screen means the verdict stands. A
/// `None` on either side means no verdict was recorded, and nothing blocks the
/// arm. Pure.
pub(crate) fn gate_blocks_304_rearm(
    gated_track_key: Option<&str>,
    last_track_key: Option<&str>,
) -> bool {
    matches!(
        (gated_track_key, last_track_key),
        (Some(gated), Some(key)) if gated == key
    )
}

/// Issue #380: whether the presence-gate re-check is due — the last gate
/// re-check is at least the re-arm cadence old, or there is no re-check
/// on record. Threaded on its own `last_gate_check` clock so re-checks
/// never shift the debounce + keepalive write windows.
///
/// Issue #867 widens the predicate with a calendar-boundary check: if the
/// Outlook calendar cache reports a meeting boundary at or before `now_wall`,
/// the re-check is due **now** — the un-gate lands within one poll of the
/// meeting end instead of waiting up to `AVAILABILITY_REARM_SECONDS`
/// (4 minutes) for the cadence to elapse. `next_meeting_boundary` is the
/// earliest future wall-clock boundary the [`crate::calendar::CalendarGate`]
/// cached; `None` is a no-op (the cadence alone gates the re-check). The
/// boundary is compared in wall-clock because converting it to `Instant`
/// requires a process-start baseline the polling thread does not have.
pub(crate) fn gate_recheck_due(
    last_gate_check: Option<Instant>,
    now: Instant,
    now_wall: chrono::DateTime<chrono::Utc>,
    next_meeting_boundary: Option<chrono::DateTime<chrono::Utc>>,
) -> bool {
    if let Some(boundary) = next_meeting_boundary {
        if now_wall >= boundary {
            return true;
        }
    }
    match last_gate_check {
        Some(t) => now.duration_since(t).as_secs() >= super::presence::AVAILABILITY_REARM_SECONDS,
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::polling::presence::AVAILABILITY_REARM_SECONDS;

    /// Production source with the test module stripped — the shared preamble
    /// for the structural guards below.
    fn prod_source() -> &'static str {
        include_str!("gate.rs")
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("gate.rs has no #[cfg(test)] mod tests block")
    }

    /// Brace-counted body isolation for a production fn (house style — never
    /// boundary anchors, which drift). Thin wrapper over
    /// [`brace_counted_body`] so an arm inside an already-isolated body can be
    /// isolated with the same routine.
    fn prod_fn_body<'a>(prod: &'a str, sig: &str) -> &'a str {
        brace_counted_body(prod, sig)
    }

    /// Brace-count the block that starts at the first `{` after `anchor`.
    /// Works on a whole production source (`anchor` = a fn signature) or on an
    /// already-isolated body (`anchor` = a match-arm head), which is what lets
    /// a guard pin ONE arm instead of every occurrence in the enclosing fn.
    fn brace_counted_body<'a>(block: &'a str, anchor: &str) -> &'a str {
        let after_anchor = block
            .split(anchor)
            .nth(1)
            .unwrap_or_else(|| panic!("production source has no `{}`", anchor));
        let open = after_anchor
            .find('{')
            .unwrap_or_else(|| panic!("`{}` has no opening brace", anchor));
        let mut depth = 0usize;
        let mut end = None;
        for (i, ch) in after_anchor[open..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(open + i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        &after_anchor[..end.unwrap_or_else(|| panic!("`{}` body never closed", anchor))]
    }

    /// Issue #790: only a verdict recorded for the track STILL on screen blocks
    /// the 304 re-arm. A verdict left over from another track, or none at all,
    /// does not — otherwise a gate that was recorded once would mute the 4-min
    /// cadence for every later 304 of the same play.
    #[test]
    fn test_gate_blocks_304_rearm_scope() {
        assert!(gate_blocks_304_rearm(Some("key"), Some("key")));
        assert!(!gate_blocks_304_rearm(Some("key"), Some("other")));
        assert!(!gate_blocks_304_rearm(None, Some("key")));
        assert!(!gate_blocks_304_rearm(Some("key"), None));
        assert!(
            !gate_blocks_304_rearm(None, None),
            "no verdict on either side means nothing blocks the arm"
        );
    }

    /// Issue #380: the gate re-check follows the re-arm cadence — due
    /// with no re-check on record or a stale one, not due right after one.
    /// Issue #867 adds a calendar-boundary shortcut: a boundary that has
    /// already passed forces the re-check on the same poll, so the un-gate
    /// lands within one poll of the meeting end instead of waiting up to
    /// `AVAILABILITY_REARM_SECONDS` (4 minutes).
    #[test]
    fn test_gate_recheck_due_follows_rearm_cadence() {
        let now = Instant::now();
        let now_wall = chrono::Utc::now();
        assert!(
            gate_recheck_due(None, now, now_wall, None),
            "no re-check on record means the re-check is due"
        );
        assert!(
            gate_recheck_due(
                Some(now - std::time::Duration::from_secs(AVAILABILITY_REARM_SECONDS + 1)),
                now,
                now_wall,
                None,
            ),
            "a re-check older than the re-arm cadence means the re-check is due"
        );
        assert!(
            !gate_recheck_due(Some(now), now, now_wall, None),
            "a fresh re-check must not re-read presence every poll"
        );
        // Issue #867: a boundary in the past overrides the cadence — the
        // meeting just ended, the un-gate must fire on this poll.
        assert!(
            gate_recheck_due(
                Some(now),
                now,
                now_wall,
                Some(now_wall - chrono::Duration::seconds(1))
            ),
            "a meeting boundary that has just passed forces the re-check on the same poll"
        );
        // Issue #867: a boundary in the future leaves the cadence in charge.
        assert!(
            !gate_recheck_due(
                Some(now),
                now,
                now_wall,
                Some(now_wall + chrono::Duration::minutes(30)),
            ),
            "a boundary in the future does not shortcut the cadence"
        );
        // Issue #867: no boundary means no shortcut (cadence alone).
        assert!(
            !gate_recheck_due(Some(now), now, now_wall, None),
            "a fresh re-check with no boundary stays throttled by the cadence"
        );
    }
    /// Issue #432: quiet-hours predicate — plain range, wrap-around,
    /// weekday filter, disabled entry, and degenerate equal bounds.
    #[test]
    fn test_quiet_hours_active_predicate() {
        use crate::config::{QuietHoursEntry, StatusRulesConfig};
        let rules = |entries: Vec<QuietHoursEntry>| StatusRulesConfig {
            quiet_hours: entries,
            track_rules: Vec::new(),
            ..StatusRulesConfig::default()
        };
        let entry = |enabled: bool, start: u16, end: u16, days: Vec<u8>| QuietHoursEntry {
            replacement_status: String::new(),
            enabled,
            start_minutes: start,
            end_minutes: end,
            days,
            ..QuietHoursEntry::default()
        };
        // Plain range 09:00→17:00 on a Wednesday (3).
        let r = rules(vec![entry(true, 540, 1020, vec![])]);
        assert!(quiet_hours_active(&r, 600, 3));
        assert!(!quiet_hours_active(&r, 500, 3));
        assert!(!quiet_hours_active(&r, 1020, 3), "end bound is exclusive");
        // Wrap-around 22:00→07:00.
        let w = rules(vec![entry(true, 1320, 420, vec![])]);
        assert!(quiet_hours_active(&w, 1380, 3));
        assert!(quiet_hours_active(&w, 300, 3));
        assert!(!quiet_hours_active(&w, 600, 3));
        // Weekday filter: Mondays only.
        let d = rules(vec![entry(true, 0, 1439, vec![1])]);
        assert!(quiet_hours_active(&d, 600, 1));
        assert!(!quiet_hours_active(&d, 600, 2));
        // Issue #794: a Monday-only midnight-crossing window is owned by the
        // night it starts on — active Mon 23:00 + Tue 03:00, inactive Mon
        // 03:00 (that morning belongs to the Sunday night) + Tue 23:00
        // (that evening starts the Tuesday night, which is not selected).
        let mon_night = rules(vec![entry(true, 1320, 420, vec![1])]);
        assert!(quiet_hours_active(&mon_night, 1380, 1));
        assert!(quiet_hours_active(&mon_night, 180, 2));
        assert!(!quiet_hours_active(&mon_night, 180, 1));
        assert!(!quiet_hours_active(&mon_night, 1380, 2));
        // Disabled entry never gates; degenerate equal bounds never gate.
        assert!(!quiet_hours_active(
            &rules(vec![entry(false, 0, 1439, vec![])]),
            600,
            3
        ));
        assert!(!quiet_hours_active(
            &rules(vec![entry(true, 600, 600, vec![])]),
            600,
            3
        ));
        // No entries at all.
        assert!(!quiet_hours_active(&rules(vec![]), 600, 3));
    }

    /// S4 (issue #672): the quiet-hours "pause polling" gate. Only the ACTIVE
    /// entry can stop polling, the skip sleeps for the configured ceiling, and
    /// the decision follows the clock — so the window ending resumes polling by
    /// itself, with no thread stop or park.
    #[test]
    fn test_quiet_pause_gate_follows_the_window_and_the_pause_flag() {
        use crate::config::{AppConfig, QuietHoursEntry, StatusRulesConfig};
        let config = |pause_polling: bool, enabled: bool, start: u16, end: u16| {
            std::sync::Arc::new(AppConfig {
                status_rules: StatusRulesConfig {
                    quiet_hours: vec![QuietHoursEntry {
                        enabled,
                        start_minutes: start,
                        end_minutes: end,
                        pause_polling,
                        ..QuietHoursEntry::default()
                    }],
                    track_rules: Vec::new(),
                    ..StatusRulesConfig::default()
                },
                ..AppConfig::default()
            })
        };

        // Inside the window with the flag on: the iteration is skipped, and it
        // sleeps for the configured ceiling (60 s by default) — not the error
        // retry interval.
        let paused = config(true, true, 1320, 420);
        assert_eq!(
            quiet_pause_at(&Some(paused.clone()), 1380, 3),
            Some((60, 420))
        );
        // 07:00 is the exclusive end, so the window is already over.
        assert_eq!(quiet_pause_at(&Some(paused.clone()), 420, 3), None);
        // 22:00 is the inclusive start.
        assert_eq!(
            quiet_pause_at(&Some(paused.clone()), 1320, 3),
            Some((60, 420))
        );
        // The flag off = quiet hours suppress the WRITE, never the poll.
        assert_eq!(
            quiet_pause_at(&Some(config(false, true, 1320, 420)), 1380, 3),
            None
        );
        // A disabled entry is not active at all.
        assert_eq!(
            quiet_pause_at(&Some(config(true, false, 1320, 420)), 1380, 3),
            None
        );
        // Quiet hours are NOT an ordered list for this decision: a window that
        // starts later can assert the pause even though the first match owns the
        // replacement text.
        let mut second_window_pauses = (*config(false, true, 0, 1439)).clone();
        second_window_pauses
            .status_rules
            .quiet_hours
            .push(QuietHoursEntry {
                enabled: true,
                start_minutes: 1200,
                end_minutes: 1380,
                pause_polling: true,
                ..QuietHoursEntry::default()
            });
        assert_eq!(
            quiet_pause_at(&Some(std::sync::Arc::new(second_window_pauses)), 1300, 3),
            Some((60, 1380)),
            "an overlapping second window that asks for the pause must stop polling"
        );

        // The Settings picker saves a quiet window of `00:00 – 00:00` as
        // `start 0, end 1440` (midnight = the end of the day). That must be an
        // ALL-DAY window, not an inert one: pre-mapping the pair was 0..0, which
        // matches nothing, and the user got no feedback.
        assert_eq!(
            quiet_pause_at(&Some(config(true, true, 0, 1440)), 720, 3),
            Some((60, 1440)),
            "a 00:00–00:00 quiet window is the whole day, not a dead window"
        );

        // Two pausing windows, asserted at a time when BOTH are live so the
        // expectation can only come from the union rule: 22:00→07:00 (wrap) and
        // 20:00→23:00 overlap on [22:00, 23:00), so 22:30 is inside both. Their
        // ends are 07:00 (420) and 23:00 (1380) — an implementation taking the
        // FIRST/minimum would report 420 here, so this assertion is not vacuous.
        let mut two_pausing = (*config(true, true, 1320, 420)).clone();
        two_pausing.status_rules.quiet_hours.push(QuietHoursEntry {
            enabled: true,
            start_minutes: 1200,
            end_minutes: 1380,
            pause_polling: true,
            ..QuietHoursEntry::default()
        });
        assert!(quiet_entry_contains(
            &two_pausing.status_rules.quiet_hours[0],
            1350,
            3
        ));
        assert!(quiet_entry_contains(
            &two_pausing.status_rules.quiet_hours[1],
            1350,
            3
        ));
        assert_eq!(
            quiet_pause_at(&Some(std::sync::Arc::new(two_pausing.clone())), 1350, 3),
            Some((60, 1380)),
            "with two live pausing windows the log reports the latest end"
        );

        // The sleep follows the user's ceiling.
        let mut slow = (*paused).clone();
        slow.polling.max_interval_seconds = 300;
        assert_eq!(
            quiet_pause_at(&Some(std::sync::Arc::new(slow)), 1380, 3),
            Some((300, 420))
        );
        // No config loaded: nothing can pause the poll.
        assert_eq!(quiet_pause_at(&None, 1380, 3), None);
    }

    /// S4 (issue #672): the pause and the resume are each logged exactly once —
    /// driven here from explicit minutes instead of the wall clock, so the
    /// transition contract is testable without waiting for a window.
    #[test]
    fn test_quiet_pause_logs_the_transition_once() {
        assert_eq!(
            quiet_pause_log_line(true, false, Some(420)).as_deref(),
            Some("[POLLING] quiet hours: polling paused until 07:00")
        );
        assert_eq!(
            quiet_pause_log_line(false, true, None).as_deref(),
            Some("[POLLING] quiet hours: polling resumed")
        );
        // Steady states repeat nothing.
        assert_eq!(quiet_pause_log_line(true, true, Some(420)), None);
        assert_eq!(quiet_pause_log_line(false, false, None), None);
        assert_eq!(format_minutes_of_day(0), "00:00");
        assert_eq!(format_minutes_of_day(1320), "22:00");
        assert_eq!(format_minutes_of_day(1439), "23:59");
    }

    /// S4 (issue #672) acceptance (c) structural guard: the driver consults the
    /// quiet-hours pause gate BEFORE it loads the write clocks and before it
    /// runs an iteration, so a skipped iteration issues no Spotify/Graph
    /// request and moves no keepalive/debounce clock. The driver itself needs
    /// an `AppHandle`, so the ordering is pinned at the source — the same shape
    /// as the #572/D1 loop guards — while the gate's own behaviour is covered
    /// by the tests above.
    #[test]
    fn test_quiet_pause_gate_precedes_the_clock_load_and_the_iteration() {
        let loop_source = include_str!("loop.rs");
        let gate = loop_source
            .find("quiet_pause_iteration(")
            .expect("polling_loop must consult the quiet-hours pause gate (S4)");
        let clocks = loop_source
            .find("load_write_clocks(&state.session)")
            .expect("polling_loop must still snapshot the shared clocks");
        let run = loop_source
            .find("super::iteration::run(")
            .expect("polling_loop must still dispatch the iteration");
        assert!(
            gate < clocks,
            "the pause gate must run BEFORE the clocks are loaded: a skipped \
             iteration must not move (or discard) a keepalive/debounce clock (S4)"
        );
        assert!(
            gate < run,
            "the pause gate must run BEFORE the iteration: a skipped iteration \
             must issue no Spotify GET (S4)"
        );
        assert_eq!(
            loop_source.matches("quiet_pause_iteration(").count(),
            1,
            "exactly one pause-gate call site is expected in the driver"
        );
        // ...and the pause arm itself: the slice between the gate and the clock
        // load must sleep on the STOP-AWARE receiver (so stop_syncing still
        // interrupts immediately) and `continue` (so the window is re-evaluated
        // on the next iteration instead of the thread ending).
        let skip_arm = &loop_source[gate..clocks];
        assert!(
            skip_arm.contains("recv_timeout"),
            "the paused iteration must sleep on the interruptible receiver (S4)"
        );
        assert!(
            skip_arm.contains("continue"),
            "the paused iteration must re-evaluate the window next iteration (S4)"
        );
        assert!(
            skip_arm.contains("quiet_pause_seconds"),
            "the paused arm must sleep the duration the gate returned (S4)"
        );
    }

    /// Finding PollCore#1 (issue #569): the mid-track quiet-hours ENTRY fires
    /// for any un-gated track while the window is open, never re-fires for a
    /// track it already gated (the #380 re-check owns that decision), and sits
    /// ahead of the first Teams write in `process_track`.
    #[test]
    fn test_quiet_gate_entry_due_mid_track() {
        assert!(
            quiet_gate_entry_due(true, None, "key"),
            "a quiet window opening mid-track must gate the playing track"
        );
        assert!(
            quiet_gate_entry_due(true, Some("another-track"), "key"),
            "a gate recorded for a previous track must not mask this one"
        );
        assert!(
            !quiet_gate_entry_due(true, Some("key"), "key"),
            "an already-gated track must not re-emit presence-gated every poll"
        );
        assert!(
            !quiet_gate_entry_due(false, None, "key"),
            "outside quiet hours nothing is gated"
        );

        // Structural: the mid-track entry check must sit ahead of the first
        // Teams write in `process_track`, and the paused clear must consult
        // the same rule decision (finding PollCore#2).
        // Retargeted per the #754 split: `process_track` / `handle_no_track`
        // now live in `write.rs`, so the guard scans that module's source.
        let write_prod = include_str!("write.rs")
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("write.rs has no #[cfg(test)] mod tests block");
        let track_body = prod_fn_body(write_prod, "pub(crate) fn process_track(");
        let entry = track_body
            .find("quiet_gate_entry_due(")
            .expect("process_track must evaluate the mid-track quiet-hours entry (issue #569)");
        let write = track_body
            .find("set_teams_status_message(")
            .expect("process_track must call set_teams_status_message");
        assert!(
            entry < write,
            "the mid-track quiet-hours entry must precede the status write, otherwise the \
             gate is evaluated after the POST it exists to suppress"
        );
        assert!(
            track_body.contains("rule_gate("),
            "the paused clear must consult the shared rule decision (issue #570)"
        );
        assert!(
            track_body.contains("emit_presence_gated("),
            "gate decisions must be surfaced through the single emitter"
        );

        let no_track_body = prod_fn_body(write_prod, "pub(crate) fn handle_no_track(");
        // Finding #634: `rule_gate` now carries quiet hours too (the matched
        // entry's replacement text and presence pair), so the no-track clear
        // consults ONE decision instead of a separate quiet-hours probe plus a
        // track-rule probe that could disagree.
        assert!(
            no_track_body.contains("rule_gate("),
            "the no-track clear must consult the rule decision (issues #570/#634)"
        );
        assert!(
            no_track_body.contains("suppresses()"),
            "the no-track clear must honor quiet hours AND suppression rules \
             through the shared decision (issues #570/#634)"
        );
        let clear_pos = no_track_body
            .find("clear_teams_status_message(")
            .expect("handle_no_track must call clear_teams_status_message");
        let suppress_pos = no_track_body
            .find("suppression_reason")
            .expect("handle_no_track must compute a suppression reason");
        assert!(
            suppress_pos < clear_pos,
            "the no-track suppression check must precede the clear POST"
        );
    }

    // -----------------------------------------------------------------------
    // S9 (issue #677): the tray snooze gate.
    //
    // The strongest in-tree proof of "a snooze performs no Spotify or Graph
    // work": a real iteration needs an `AppHandle` (tauri's `test` feature is
    // off), so — exactly like the S4 quiet-hours gate — the ORDERING in the
    // driver is pinned at the source, and the decision itself is driven through
    // the pure predicate. What that leaves unproven is stated on the guard.
    // -----------------------------------------------------------------------

    /// A config with a stored `snooze_until`, in the spelling the tray writes.
    fn snooze_config(stored: Option<&str>, max_interval: u64) -> Option<std::sync::Arc<AppConfig>> {
        let mut cfg = AppConfig {
            snooze_until: stored.map(str::to_string),
            ..AppConfig::default()
        };
        cfg.polling.max_interval_seconds = max_interval;
        Some(std::sync::Arc::new(cfg))
    }

    fn stored_in(seconds: i64) -> String {
        crate::config::snooze_store_form(Utc::now() + chrono::TimeDelta::seconds(seconds))
    }

    /// The skip decision and its sleep value: a live deadline skips for the
    /// configured ceiling, everything else polls normally.
    #[test]
    fn snooze_pause_at_skips_only_for_a_live_deadline() {
        let now = Utc::now();
        let live = snooze_config(Some(&stored_in(30 * 60)), 60);
        let (seconds, deadline) =
            snooze_pause_at(&live, now).expect("a future deadline must skip the iteration");
        assert_eq!(seconds, 60, "the skip sleeps the configured ceiling");
        assert!(deadline > now);

        // The ceiling is floored at 1 s so a hand-edited 0 cannot spin the
        // thread — the same floor `quiet_pause_at` applies.
        assert_eq!(
            snooze_pause_at(&snooze_config(Some(&stored_in(60)), 0), now),
            Some((
                1,
                crate::config::snooze_status(&snooze_config(Some(&stored_in(60)), 0).unwrap(), now)
                    .unwrap()
                    .deadline
            ))
        );

        // Expired, unparsable, absent and unloaded configs all poll normally.
        assert!(snooze_pause_at(&snooze_config(Some(&stored_in(-1)), 60), now).is_none());
        assert!(snooze_pause_at(&snooze_config(Some("2026-01-01T00:00:00Z"), 60), now).is_none());
        assert!(snooze_pause_at(&snooze_config(Some("garbage"), 60), now).is_none());
        assert!(snooze_pause_at(&snooze_config(None, 60), now).is_none());
        assert!(snooze_pause_at(&None, now).is_none());
    }

    /// "Expired" is a distinct state from "never snoozed": it is what asks the
    /// driver to persist a clear, and it must not fire for an absent field (or
    /// the driver would rewrite config.json on every idle iteration).
    #[test]
    fn snooze_expired_distinguishes_passed_from_absent() {
        let now = Utc::now();
        assert!(snooze_expired(
            &snooze_config(Some(&stored_in(-1)), 60),
            now
        ));
        assert!(
            snooze_expired(&snooze_config(Some("not a timestamp"), 60), now),
            "an unparsable value is dead weight and must be cleared too"
        );
        assert!(!snooze_expired(
            &snooze_config(Some(&stored_in(300)), 60),
            now
        ));
        assert!(!snooze_expired(&snooze_config(None, 60), now));
        assert!(!snooze_expired(&None, now));
    }

    /// The pause log line states until when and how long is left.
    #[test]
    fn snooze_pause_log_line_reports_the_deadline_and_the_countdown() {
        assert_eq!(
            snooze_pause_log_line("14:32", 29),
            "[POLLING] snooze: polling paused until 14:32 (29 min left)"
        );
        // The `HH:MM` comes from the LOCAL clock the user set the snooze
        // against, whatever zone that is.
        let deadline = Utc::now() + chrono::TimeDelta::minutes(30);
        let east = chrono::FixedOffset::east_opt(2 * 3600).unwrap();
        assert_eq!(
            snooze_deadline_hhmm(deadline, &east),
            deadline.with_timezone(&east).format("%H:%M").to_string()
        );
        assert_eq!(snooze_deadline_hhmm(deadline, &Utc).len(), 5);
    }

    /// S9 acceptance, structural half: the driver consults the snooze gate
    /// BEFORE the quiet-hours gate, before it loads the write clocks and before
    /// it runs an iteration, so a snoozed iteration issues no Spotify/Graph
    /// request and moves no keepalive/debounce clock. It also sleeps on the
    /// STOP-AWARE receiver and `continue`s, so the deadline is re-evaluated
    /// next iteration instead of the thread ending.
    ///
    /// Unproven here (and stated in the PR): the driver itself needs an
    /// `AppHandle`, so this pins the ORDER and the pure decision rather than
    /// executing a real iteration. The remaining runtime evidence is the
    /// `[POLLING] snooze:` lines and the absence of Spotify GETs in the log.
    #[test]
    fn snooze_gate_precedes_the_quiet_gate_the_clock_load_and_the_iteration() {
        let loop_source = include_str!("loop.rs");
        let snooze = loop_source
            .find("snooze_gate(&state.session, &config)")
            .expect("polling_loop must consult the snooze gate (S9)");
        let quiet = loop_source
            .find("quiet_pause_iteration(")
            .expect("the quiet-hours pause must still be consulted (S4)");
        let clocks = loop_source
            .find("load_write_clocks(&state.session)")
            .expect("polling_loop must still snapshot the shared clocks");
        let run = loop_source
            .find("super::iteration::run(")
            .expect("polling_loop must still dispatch the iteration");
        assert!(
            snooze < quiet,
            "an explicit user snooze outranks a scheduled quiet window (S9)"
        );
        assert!(
            snooze < clocks,
            "the snooze gate must run BEFORE the clocks are loaded: a skipped \
             iteration must not move (or discard) a keepalive/debounce clock (S9)"
        );
        assert!(
            snooze < run,
            "the snooze gate must run BEFORE the iteration: a snoozed iteration \
             must issue no Spotify GET (S9)"
        );
        assert_eq!(
            loop_source
                .matches("snooze_gate(&state.session, &config)")
                .count(),
            1,
            "exactly one snooze-gate call site is expected in the driver"
        );

        let skip_arm = &loop_source[snooze..quiet];
        assert!(
            skip_arm.contains("SnoozeGate::Skipped(seconds)"),
            "the gate's skip verdict must be handled (S9)"
        );
        assert!(
            skip_arm.contains("recv_timeout"),
            "the snoozed iteration must sleep on the interruptible receiver (S9)"
        );
        assert!(
            skip_arm.contains("continue"),
            "the snoozed iteration must re-evaluate the deadline next iteration (S9)"
        );
        // The expiry arm must persist the clear rather than silently ignoring
        // it — an expired deadline left in config.json would keep the tray and
        // the Dashboard chip claiming a snooze that is over.
        let expiry_arm = &loop_source[snooze..clocks];
        assert!(
            expiry_arm.contains("SnoozeGate::Expired")
                && expiry_arm.contains("clear_snooze_if_expired("),
            "the expired verdict must clear the stored deadline (S9)"
        );
    }

    /// `clear_snooze_if_expired` must persist what it stores, exactly like the
    /// config commands: hold the write guard, clamp, stamp, save, then adopt.
    /// The behavioural half (that it clears an expired field and leaves a live
    /// one alone) is `config::clamp_snooze`'s unit test — this function writes
    /// to the real config path, so a test must not call it.
    #[test]
    fn clear_snooze_if_expired_persists_before_adopting() {
        let prod = prod_source();
        let body = prod_fn_body(prod, "pub(crate) fn clear_snooze_if_expired(");
        for marker in [
            "state.config.get_mut()",
            "crate::config::clamp_snooze(",
            "crate::config::save_config(&next)",
            "*guard = Some(Arc::new(next))",
        ] {
            assert!(
                body.contains(marker),
                "clear_snooze_if_expired must contain `{}`",
                marker
            );
        }
        assert!(
            body.find("save_config(&next)").unwrap()
                < body.find("*guard = Some(Arc::new(next))").unwrap(),
            "the in-memory config must only be updated after a successful write"
        );
        // A failed write keeps the field, so the next iteration retries instead
        // of leaving an expired deadline on disk that nothing will clear.
        assert!(
            body.contains("retrying next iteration"),
            "a failed clear must say it will retry"
        );
    }
}
