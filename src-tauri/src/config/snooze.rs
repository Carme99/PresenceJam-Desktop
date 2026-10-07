use super::schema::AppConfig;
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

/// A snooze preset offered by the tray submenu (4.7.0, S9 / issue #677;
/// issue #867 adds the calendar-bound `UntilNextMeetingEnds`).
///
/// The first two are instant offsets (`now + delta`), which no timezone can
/// move. The third is a LOCAL calendar boundary, which is why it is a variant
/// of its own rather than a `Duration` — see [`next_local_midnight_utc`].
/// The fourth (issue #867) reads from the Outlook calendar cache and falls
/// back to "until tomorrow" when no meeting is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnoozePreset {
    ThirtyMinutes,
    OneHour,
    UntilTomorrow,
    UntilNextMeetingEnds,
}

/// The deadline a preset denotes (4.7.0, S9 / issue #677; issue #867).
///
/// Both clocks are arguments rather than read inside, so the "until tomorrow"
/// boundary can be pinned at an exact wall-clock time and timezone in a unit
/// test — the boundary is where a timezone bug would hide.
///
/// `next_meeting_end` (issue #867) is the end of the meeting currently in
/// progress, computed by the tray handler from the [`crate::calendar`] cache.
/// When `preset` is `UntilNextMeetingEnds` and `next_meeting_end` is `None`
/// (no active meeting, or the cache is empty), the deadline falls back to
/// "until tomorrow" — clicking the entry while no meeting is in progress
/// still produces a valid deadline.
pub fn snooze_preset_deadline<Tz: chrono::TimeZone>(
    preset: SnoozePreset,
    now_utc: chrono::DateTime<chrono::Utc>,
    now_local: chrono::DateTime<Tz>,
    next_meeting_end: Option<chrono::DateTime<chrono::Utc>>,
) -> chrono::DateTime<chrono::Utc> {
    match preset {
        SnoozePreset::ThirtyMinutes => now_utc + chrono::TimeDelta::minutes(30),
        SnoozePreset::OneHour => now_utc + chrono::TimeDelta::minutes(60),
        SnoozePreset::UntilTomorrow => next_local_midnight_utc(now_local),
        SnoozePreset::UntilNextMeetingEnds => match next_meeting_end {
            Some(end) => end,
            None => next_local_midnight_utc(now_local),
        },
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
pub(crate) fn next_local_midnight_utc<Tz: chrono::TimeZone>(
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
pub(crate) fn resolve_local_forward<Tz: chrono::TimeZone>(
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
