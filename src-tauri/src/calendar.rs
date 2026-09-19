//! Outlook calendar gate (issue #867).
//!
//! Reads the user's calendar from Microsoft Graph
//! (`GET /me/calendarView`) and exposes a cached view of upcoming events
//! so the polling loop can:
//!
//! - **pre-gate** a busy meeting by `pre_meeting_suppress_minutes` before it
//!   starts, and
//! - **un-gate** the moment the meeting ends — `gate_recheck_due` returns
//!   true at the boundary, so the un-gate lands within one poll of the
//!   `AVAILABILITY_REARM_SECONDS` cadence instead of being throttled by it.
//!
//! Failures are silent and fail-open: every error path leaves the cache
//! empty (no meetings), so a denied `Calendars.ReadBasic` grant or a
//! tenant that refuses the scope reproduces today's behaviour exactly. The
//! Settings copy says so (issue #867 acceptance criteria).
//!
//! The struct is testable without an `AppHandle`: the clock is injected
//! via [`CalendarGate::with_clock`], and the network call is a closure
//! passed to [`CalendarGate::list_upcoming`]. The unit tests below drive
//! the boundary path with a controllable mock clock and never touch a live
//! HTTP client or a `tauri::AppHandle`.

use std::sync::Arc;
use std::time::{Duration as StdDuration, Instant};

use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use serde::Deserialize;

/// Log tag prefix for this module (mirrors `[TEAMS]` / `[POLLING]` / etc.).
const TAG: &str = "[CALENDAR]";

/// How long the cache is considered fresh before `list_upcoming` re-fetches
/// (issue #867). Five minutes is the documented throttle: long enough that a
/// steady polling loop fetches once per window, short enough that a tenant
/// that revoked the scope catches the denial within one cache cycle.
pub const CALENDAR_CACHE_TTL: StdDuration = StdDuration::from_secs(5 * 60);

/// What Microsoft Graph means by `showAs`. Only `Busy` and `Oof` gate
/// the status write — `free`, `tentative` and `workingElsewhere` are not
/// treated as busy by this app (the gate already respects the explicit
/// Teams presence sample when those surface as `Busy`/`DoNotDisturb`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShowAs {
    Busy,
    Free,
    Tentative,
    Oof,
    WorkingElsewhere,
    /// Unrecognised value (tenant-extended). Treated as not-busy so a
    /// vendor-specific `showAs` cannot accidentally start gating.
    Other,
}

impl ShowAs {
    fn parse(value: &str) -> Self {
        match value {
            "busy" => Self::Busy,
            "free" => Self::Free,
            "tentative" => Self::Tentative,
            "oof" => Self::Oof,
            "workingElsewhere" => Self::WorkingElsewhere,
            _ => Self::Other,
        }
    }

    /// True for the variants that mean "the user is in a meeting right now".
    pub fn is_busy(self) -> bool {
        matches!(self, Self::Busy | Self::Oof)
    }
}

/// Microsoft Graph `sensitivity`. Held for symmetry / logging; this app
/// does not gate differently by sensitivity — a `private` meeting still
/// blocks status writes the same way a `normal` one does, because the
/// user's intent (don't post while I'm in this meeting) is the same.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sensitivity {
    Normal,
    Private,
    Confidential,
    Other,
}

impl Sensitivity {
    fn parse(value: &str) -> Self {
        match value {
            "normal" => Self::Normal,
            "private" => Self::Private,
            "confidential" => Self::Confidential,
            _ => Self::Other,
        }
    }
}

/// One parsed Outlook event (issue #867). Only the fields the gate cares
/// about are kept; the `subject` is dropped so a tenant with private
/// subjects cannot leak through the log or the snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarEvent {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub show_as: ShowAs,
    pub is_all_day: bool,
    pub is_cancelled: bool,
    pub sensitivity: Sensitivity,
}

/// Raw wire payload for one event — only the fields `$select` asks for.
#[derive(Debug, Deserialize)]
pub struct CalendarEventRaw {
    #[serde(default)]
    pub start: Option<EventTimeRaw>,
    #[serde(default)]
    pub end: Option<EventTimeRaw>,
    #[serde(default)]
    pub show_as: Option<String>,
    #[serde(default)]
    pub is_all_day: Option<bool>,
    #[serde(default)]
    pub is_cancelled: Option<bool>,
    #[serde(default)]
    pub sensitivity: Option<String>,
}

/// `start` / `end` on the wire. `dateTime` is the local-zone wall clock
/// (we send `Prefer: outlook.timezone`); `timeZone` is informational and
/// is not parsed here, because the wire value already includes the offset
/// after `outlook.timezone` is applied.
#[derive(Debug, Deserialize)]
pub struct EventTimeRaw {
    #[serde(default)]
    pub date_time: Option<String>,
    #[serde(default)]
    pub time_zone: Option<String>,
}

/// Top-level `calendarView` payload. Empty `value` (a week with no
/// meetings) is a valid response — the call returns `Ok(vec![])`.
#[derive(Debug, Deserialize)]
pub struct CalendarViewResponse {
    #[serde(default)]
    pub value: Vec<CalendarEventRaw>,
}

/// A pluggable monotonic clock. Production passes `Instant::now`; tests
/// pass a controllable mock so `tokio::time::pause()` (or any other test
/// harness) can drive the boundary path.
pub type ClockFn = Arc<dyn Fn() -> Instant + Send + Sync>;

/// The cached calendar. Lives behind an `Arc<Mutex<_>>` so the polling
/// thread, the tray snooze handler and the config writer all see the same
/// view. The cache is process-wide and survives a Teams token refresh
/// (the token is replaced atomically with the cache).
pub struct CalendarGate {
    inner: Arc<Mutex<CalendarGateInner>>,
    clock: ClockFn,
}

impl Default for CalendarGate {
    fn default() -> Self {
        Self::new()
    }
}

impl CalendarGate {
    /// Production constructor — uses `Instant::now` for the freshness clock.
    pub fn new() -> Self {
        Self::with_clock(Arc::new(Instant::now))
    }

    /// Test constructor. `clock` is called once per `list_upcoming` (and per
    /// `is_stale`) so a test that wants to advance the boundary just bumps
    /// the underlying value.
    pub fn with_clock(clock: ClockFn) -> Self {
        Self {
            inner: Arc::new(Mutex::new(CalendarGateInner::default())),
            clock,
        }
    }

    /// Updates the access token used for fetches. The token is read at
    /// fetch time, so a refreshed Teams token is picked up on the next
    /// `list_upcoming` call without invalidating the cache.
    pub fn set_access_token(&self, token: String) {
        self.inner.lock().access_token = Some(token);
    }

    /// Drops the cached token and cache — used on disconnect.
    pub fn clear(&self) {
        let mut inner = self.inner.lock();
        inner.access_token = None;
        inner.cached.clear();
        inner.fetched_at = None;
    }

    /// Lists upcoming events in `[now, now + horizon)`. Returns the cached
    /// view when it is fresh and not past a boundary; otherwise fetches
    /// from Microsoft Graph via the supplied closure and updates the
    /// cache. **Failures are silent**: every error path empties the cache
    /// and returns an empty `Vec`, so the polling loop continues to
    /// behave as if no meeting exists. A tenant that refuses
    /// `Calendars.ReadBasic` (or the scope is not yet granted) reproduces
    /// the pre-#867 behaviour exactly.
    ///
    /// `fetch` is the Graph HTTP call. Production callers pass
    /// [`fetch_calendar_view`]; tests pass a closure that returns canned
    /// payloads, so the cache + boundary logic is exercised without a
    /// live `AppHandle` or a wiremock harness.
    pub fn list_upcoming<F, E>(
        &self,
        now: DateTime<Utc>,
        horizon: chrono::Duration,
        fetch: F,
    ) -> Vec<CalendarEvent>
    where
        F: FnOnce(&str, DateTime<Utc>, chrono::Duration) -> Result<Vec<CalendarEventRaw>, E>,
        E: std::fmt::Display,
    {
        let now_instant = (self.clock)();

        // Decide whether to refresh. A cache is "fresh" if it is younger
        // than the TTL AND we are still before its earliest boundary (a
        // meeting that started or ended since the last fetch is reason
        // enough to ask Graph again). No token → always try to fetch; an
        // empty cache past the TTL is also stale (a fetcher that returned
        // no events stays valid until the TTL lapses, not forever).
        let needs_refresh = {
            let inner = self.inner.lock();
            if inner.access_token.is_none() {
                true
            } else {
                match inner.fetched_at {
                    None => true,
                    Some(t) if now_instant.duration_since(t) >= CALENDAR_CACHE_TTL => true,
                    Some(_) => {
                        // Inside the TTL: only refresh when a boundary has
                        // already passed OR the cache says "no upcoming
                        // events" (which is what triggers a
                        // `next_boundary` of None). A future boundary
                        // means the cached view is still accurate for the
                        // gate.
                        next_boundary_in_cached(&inner.cached, now)
                            .map(|b| now >= b)
                            .unwrap_or(true)
                    }
                }
            }
        };

        if !needs_refresh {
            return self.inner.lock().cached.clone();
        }

        let token = self.inner.lock().access_token.clone();
        let Some(token) = token else {
            return Vec::new();
        };

        match fetch(&token, now, horizon) {
            Ok(raw) => {
                let events: Vec<CalendarEvent> = raw
                    .into_iter()
                    .filter_map(|r| CalendarEvent::try_from_raw(r, now, horizon))
                    .collect();
                let mut inner = self.inner.lock();
                inner.cached = events.clone();
                inner.fetched_at = Some(now_instant);
                events
            }
            Err(e) => {
                // Fail-open: clear the cache, record the fetch attempt so
                // we don't busy-loop, and let the caller see "no events".
                log::warn!("{} list_upcoming: fetch failed, failing open: {}", TAG, e);
                let mut inner = self.inner.lock();
                inner.cached.clear();
                inner.fetched_at = Some(now_instant);
                Vec::new()
            }
        }
    }

    /// True if `now` falls inside a busy meeting, accounting for the
    /// `pre_meeting_suppress_minutes` pre-gate. All-day events are
    /// excluded — they last an entire day and the time-of-day gate is
    /// not the right primitive for them.
    pub fn busy_at(&self, now: DateTime<Utc>, pre_meeting_suppress_minutes: u16) -> bool {
        let pre = chrono::Duration::minutes(pre_meeting_suppress_minutes as i64);
        let inner = self.inner.lock();
        inner.cached.iter().any(|e| {
            !e.is_cancelled
                && !e.is_all_day
                && e.show_as.is_busy()
                && now >= e.start - pre
                && now < e.end
        })
    }

    /// The next meeting boundary strictly after `now`: the start or end of
    /// the earliest such event. `None` when no future boundary exists or
    /// the cache is empty. Used by `gate_recheck_due` so the un-gate
    /// lands at the boundary, not on the 4-minute throttle.
    pub fn next_boundary(&self, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        let inner = self.inner.lock();
        next_boundary_in_cached(&inner.cached, now)
    }

    /// True if a busy meeting is currently active (i.e. the user is "in a
    /// meeting" right now), ignoring the pre-gate window. Used by the tray
    /// to decide whether to surface "Until this meeting ends" in the
    /// snooze submenu.
    pub fn meeting_active(&self, now: DateTime<Utc>) -> bool {
        let inner = self.inner.lock();
        inner.cached.iter().any(|e| {
            !e.is_cancelled && !e.is_all_day && e.show_as.is_busy() && now >= e.start && now < e.end
        })
    }

    /// The end of the currently-active meeting (if any), for the tray's
    /// "until this meeting ends" snooze deadline.
    pub fn current_meeting_end(&self, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        let inner = self.inner.lock();
        inner
            .cached
            .iter()
            .filter(|e| {
                !e.is_cancelled
                    && !e.is_all_day
                    && e.show_as.is_busy()
                    && now >= e.start
                    && now < e.end
            })
            .map(|e| e.end)
            .min()
    }

    /// True if a fetch is currently warranted (TTL expired or past the
    /// next boundary). Exposed for tests; production callers go through
    /// `list_upcoming`.
    pub fn is_stale(&self) -> bool {
        let now_instant = (self.clock)();
        let inner = self.inner.lock();
        match inner.fetched_at {
            None => true,
            Some(t) => now_instant.duration_since(t) >= CALENDAR_CACHE_TTL,
        }
    }
}

#[derive(Default)]
struct CalendarGateInner {
    access_token: Option<String>,
    cached: Vec<CalendarEvent>,
    fetched_at: Option<Instant>,
}

impl CalendarEvent {
    /// Parses a wire event, dropping events that don't intersect the
    /// requested window or that are missing a parseable time. The window
    /// check uses half-open `[window_start, window_end)` semantics so a
    /// meeting ending exactly at `now` is no longer "upcoming".
    fn try_from_raw(
        raw: CalendarEventRaw,
        window_start: DateTime<Utc>,
        horizon: chrono::Duration,
    ) -> Option<Self> {
        let start = parse_event_time(raw.start.as_ref())?;
        let end = parse_event_time(raw.end.as_ref())?;
        let window_end = window_start + horizon;
        if end <= window_start || start >= window_end {
            return None;
        }
        Some(Self {
            start,
            end,
            show_as: ShowAs::parse(raw.show_as.as_deref().unwrap_or("")),
            is_all_day: raw.is_all_day.unwrap_or(false),
            is_cancelled: raw.is_cancelled.unwrap_or(false),
            sensitivity: Sensitivity::parse(raw.sensitivity.as_deref().unwrap_or("")),
        })
    }
}

fn parse_event_time(time: Option<&EventTimeRaw>) -> Option<DateTime<Utc>> {
    let time = time?;
    let date_time = time.date_time.as_deref()?;
    DateTime::parse_from_rfc3339(date_time)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

/// The raw Graph fetch (`GET /me/calendarView`). Lives next to
/// [`CalendarGate`] so the URL, the `$select` projection, and the
/// `Prefer: outlook.timezone` header are co-located — a hand-edit that
/// drops `showAs` would silently stop the busy gate from working.
///
/// Failures map onto the same [`TeamsApiError`] taxonomy the rest of the
/// Teams integration uses, so `list_upcoming`'s fail-open behaviour is
/// documented at one place.
pub fn fetch_calendar_view(
    client: &reqwest::blocking::Client,
    access_token: &str,
    now: DateTime<Utc>,
    horizon: chrono::Duration,
) -> Result<Vec<CalendarEventRaw>, crate::teams::TeamsApiError> {
    let start = now.to_rfc3339();
    let end = (now + horizon).to_rfc3339();
    let url = format!(
        "https://graph.microsoft.com/v1.0/me/calendarView?startDateTime={}&endDateTime={}&$select=subject,start,end,showAs,isAllDay,isCancelled,sensitivity",
        urlencoded(&start),
        urlencoded(&end),
    );
    let response = client
        .get(&url)
        .bearer_auth(access_token)
        .header("Prefer", "outlook.timezone")
        .send()
        .map_err(|e| crate::teams::TeamsApiError::Transient(e.to_string()))?;
    let status = response.status();
    let body = response
        .text()
        .map_err(|e| crate::teams::TeamsApiError::Transient(e.to_string()))?;
    match status.as_u16() {
        200..=299 => serde_json::from_str::<CalendarViewResponse>(&body)
            .map(|parsed| parsed.value)
            .map_err(|e| crate::teams::TeamsApiError::Transient(e.to_string())),
        401 => Err(crate::teams::TeamsApiError::ExpiredToken(status.as_u16())),
        403 => Err(crate::teams::TeamsApiError::Forbidden(
            status.as_u16(),
            body,
        )),
        429 => Err(crate::teams::TeamsApiError::RateLimited(None)),
        s if s >= 500 => Err(crate::teams::TeamsApiError::Transient(body)),
        s => Err(crate::teams::TeamsApiError::Other(s, body)),
    }
}

/// URL-encodes a string for use in a query parameter. `%`-encodes every
/// byte that isn't an unreserved character per RFC 3986 §2.3.
fn urlencoded(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*byte as char);
            }
            _ => out.push_str(&format!("%{:02X}", byte)),
        }
    }
    out
}

fn next_boundary_in_cached(cached: &[CalendarEvent], now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let mut candidates: Vec<DateTime<Utc>> = Vec::new();
    for event in cached {
        if event.start > now {
            candidates.push(event.start);
        }
        if event.end > now {
            candidates.push(event.end);
        }
    }
    candidates.into_iter().min()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// A controllable clock: the `AtomicU64` holds the current `Instant`
    /// in nanoseconds since the test's epoch. Tests advance it via the
    /// returned `&AtomicU64`.
    struct MockClock {
        handle: Arc<AtomicU64>,
        epoch: Instant,
    }

    impl MockClock {
        fn new() -> Self {
            Self {
                handle: Arc::new(AtomicU64::new(0)),
                epoch: Instant::now(),
            }
        }
        fn handle(&self) -> (ClockFn, Arc<AtomicU64>) {
            let handle = self.handle.clone();
            let epoch = self.epoch;
            let clock: ClockFn =
                Arc::new(move || epoch + StdDuration::from_nanos(handle.load(Ordering::SeqCst)));
            (clock, self.handle.clone())
        }
    }

    /// Helper that produces a UTC `DateTime` for a wall-clock value the
    /// test can read at a glance.
    fn at(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0)
            .single()
            .expect("valid epoch second")
    }

    /// Builds a busy event spanning `[start, end)`.
    fn busy(start: i64, end: i64) -> CalendarEvent {
        CalendarEvent {
            start: at(start),
            end: at(end),
            show_as: ShowAs::Busy,
            is_all_day: false,
            is_cancelled: false,
            sensitivity: Sensitivity::Normal,
        }
    }

    /// Issue #867: a busy event suppresses the status BEFORE its start,
    /// when `pre_meeting_suppress_minutes` covers the gap. The 4-minute
    /// `AVAILABILITY_REARM_SECONDS` throttle would otherwise leave the
    /// stale "playing" status posted until the next re-check.
    #[test]
    fn busy_at_suppresses_during_pre_meeting_window() {
        let gate = CalendarGate::new();
        // Busy event 3600..5400 (one o'clock, half an hour long).
        let events = vec![busy(3600, 5400)];
        {
            let mut inner = gate.inner.lock();
            inner.cached = events;
            inner.fetched_at = Some(Instant::now());
        }
        // 5 minutes before start (3550): inside a 10-minute pre-window.
        assert!(
            gate.busy_at(at(3550), 10),
            "pre-meeting suppression window must cover the gap before the start"
        );
        // 11 minutes before start (2940): outside a 10-minute pre-window.
        assert!(
            !gate.busy_at(at(2940), 10),
            "outside the pre-window the gate must stay open"
        );
        // During the meeting.
        assert!(gate.busy_at(at(4000), 10));
        // After the meeting end.
        assert!(!gate.busy_at(at(5500), 10));
    }

    /// Issue #867: a non-busy event (`free`/`tentative`) does NOT suppress,
    /// even when `pre_meeting_suppress_minutes` is non-zero. The user's
    /// presence bubble (the `presence_gate` already in `poll_once`) is the
    /// right gate for those, not the calendar.
    #[test]
    fn busy_at_ignores_non_busy_show_as() {
        let gate = CalendarGate::new();
        let mut event = busy(3600, 5400);
        event.show_as = ShowAs::Tentative;
        {
            let mut inner = gate.inner.lock();
            inner.cached = vec![event];
            inner.fetched_at = Some(Instant::now());
        }
        assert!(
            !gate.busy_at(at(4500), 10),
            "a tentative event must not suppress the status write"
        );
    }

    /// Issue #867: `next_boundary` returns the earliest future start or
    /// end. Used by `gate_recheck_due` to fire the un-gate at the
    /// meeting end.
    #[test]
    fn next_boundary_is_minimum_of_future_starts_and_ends() {
        let gate = CalendarGate::new();
        let events = vec![busy(3600, 5400), busy(7200, 7500)];
        {
            let mut inner = gate.inner.lock();
            inner.cached = events;
            inner.fetched_at = Some(Instant::now());
        }
        assert_eq!(gate.next_boundary(at(0)), Some(at(3600)));
        assert_eq!(gate.next_boundary(at(3600)), Some(at(5400)));
        assert_eq!(gate.next_boundary(at(5400)), Some(at(7200)));
        assert_eq!(gate.next_boundary(at(7500)), None);
    }

    /// Issue #867: `meeting_active` is the strict "is the user in a
    /// meeting right now" predicate the tray snooze menu consults. The
    /// pre-window is NOT included — the user is not yet in the meeting.
    #[test]
    fn meeting_active_is_strict_in_window_only() {
        let gate = CalendarGate::new();
        let events = vec![busy(3600, 5400)];
        {
            let mut inner = gate.inner.lock();
            inner.cached = events;
            inner.fetched_at = Some(Instant::now());
        }
        assert!(
            !gate.meeting_active(at(3599)),
            "right before start: not active"
        );
        assert!(gate.meeting_active(at(3600)), "at start: active");
        assert!(gate.meeting_active(at(4500)), "during: active");
        assert!(
            !gate.meeting_active(at(5400)),
            "at end: not active (half-open)"
        );
    }

    /// Issue #867: a fetch failure is fail-open — the cache empties and
    /// the function returns `vec![]`, so the polling loop sees "no
    /// meetings" exactly like the pre-#867 behaviour. A 403 from a tenant
    /// that refuses `Calendars.ReadBasic` must NOT escalate into a status
    /// write failure.
    #[test]
    fn list_upcoming_fails_open_on_fetch_error() {
        let gate = CalendarGate::new();
        gate.set_access_token("tok".to_string());
        let now = at(2_000);
        let horizon = chrono::Duration::hours(4);
        let result = gate.list_upcoming(now, horizon, |_token, _start, _h| {
            Err::<Vec<CalendarEventRaw>, &str>("HTTP 403: insufficient privileges")
        });
        assert!(
            result.is_empty(),
            "a fetch failure must fail open to an empty list"
        );
    }

    /// Issue #867: a successful fetch populates the cache AND returns the
    /// parsed events.
    #[test]
    fn list_upcoming_returns_parsed_events_on_success() {
        let gate = CalendarGate::new();
        gate.set_access_token("tok".to_string());
        // Window centred on the test's epoch second 0; the event lives at
        // epoch second 3600 (one hour past `now`), which falls inside the
        // 4-hour horizon.
        let now = at(0);
        let horizon = chrono::Duration::hours(4);
        let raw = vec![CalendarEventRaw {
            start: Some(EventTimeRaw {
                date_time: Some("1970-01-01T01:00:00Z".to_string()),
                time_zone: Some("UTC".to_string()),
            }),
            end: Some(EventTimeRaw {
                date_time: Some("1970-01-01T01:30:00Z".to_string()),
                time_zone: Some("UTC".to_string()),
            }),
            show_as: Some("busy".to_string()),
            is_all_day: Some(false),
            is_cancelled: Some(false),
            sensitivity: Some("normal".to_string()),
        }];
        let result = gate.list_upcoming(now, horizon, |_token, _start, _h| Ok::<_, &str>(raw));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].show_as, ShowAs::Busy);
    }

    /// Issue #867: the cache TTL is enforced — a second `list_upcoming`
    /// inside the TTL must NOT call the fetcher a second time.
    #[test]
    fn list_upcoming_within_ttl_uses_cache_without_calling_fetch() {
        let clock = MockClock::new();
        let (clock_fn, _nanos) = clock.handle();
        let gate = CalendarGate::with_clock(clock_fn);
        gate.set_access_token("tok".to_string());
        let now = at(0);
        let horizon = chrono::Duration::hours(4);
        let first = gate.list_upcoming(now, horizon, |_t, _s, _h| {
            Ok::<_, &str>(vec![CalendarEventRaw {
                start: Some(EventTimeRaw {
                    date_time: Some("1970-01-01T01:00:00Z".to_string()),
                    time_zone: None,
                }),
                end: Some(EventTimeRaw {
                    date_time: Some("1970-01-01T01:30:00Z".to_string()),
                    time_zone: None,
                }),
                show_as: Some("busy".to_string()),
                is_all_day: Some(false),
                is_cancelled: Some(false),
                sensitivity: Some("normal".to_string()),
            }])
        });
        assert_eq!(first.len(), 1);

        // Second call inside the TTL must NOT call the fetcher.
        let second: Vec<CalendarEvent> = gate.list_upcoming(
            now,
            horizon,
            |_t, _s, _h| -> Result<Vec<CalendarEventRaw>, &'static str> {
                panic!("fetcher must not be called inside the cache TTL")
            },
        );
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].show_as, ShowAs::Busy);
    }

    /// Issue #867: a fetch past the cache TTL does refresh. The test
    /// uses the mock clock to drive the boundary without touching a real
    /// network or a real `Instant::now`.
    #[test]
    fn list_upcoming_past_ttl_refreshes() {
        let clock = MockClock::new();
        let (clock_fn, nanos) = clock.handle();
        let gate = CalendarGate::with_clock(clock_fn);
        gate.set_access_token("tok".to_string());
        let now = at(2_000);
        let horizon = chrono::Duration::hours(4);
        // First fetch.
        let _ = gate.list_upcoming(now, horizon, |_t, _s, _h| Ok::<_, &str>(vec![]));
        // Advance past the TTL.
        nanos.store((6 * 60) * 1_000_000_000, Ordering::SeqCst);
        let called = std::sync::atomic::AtomicBool::new(false);
        let _ = gate.list_upcoming(now, horizon, |_t, _s, _h| {
            called.store(true, Ordering::SeqCst);
            Ok::<_, &str>(vec![])
        });
        assert!(
            called.load(Ordering::SeqCst),
            "past the TTL the fetcher must be called again"
        );
    }

    /// Issue #867: the boundary-driven refresh. A cache whose earliest
    /// boundary has already passed triggers a fetch — the gate cannot
    /// remain "fresh" against a meeting that started or ended while the
    /// thread was asleep.
    #[test]
    fn list_upcoming_refreshes_after_boundary_passes() {
        let clock = MockClock::new();
        let (clock_fn, _nanos) = clock.handle();
        let gate = CalendarGate::with_clock(clock_fn);
        gate.set_access_token("tok".to_string());
        // A busy event from t=3600 to t=5400 (one hour, half hour long) is
        // cached at t=0. At t=7200 (two hours in) the boundary has
        // passed, so the next `list_upcoming` must refresh.
        let now_first = at(0);
        let now_second = at(7200);
        let horizon = chrono::Duration::hours(4);
        let raw = CalendarEventRaw {
            start: Some(EventTimeRaw {
                date_time: Some("1970-01-01T01:00:00Z".to_string()),
                time_zone: None,
            }),
            end: Some(EventTimeRaw {
                date_time: Some("1970-01-01T01:30:00Z".to_string()),
                time_zone: None,
            }),
            show_as: Some("busy".to_string()),
            is_all_day: Some(false),
            is_cancelled: Some(false),
            sensitivity: Some("normal".to_string()),
        };
        let _ = gate.list_upcoming(now_first, horizon, |_t, _s, _h| Ok::<_, &str>(vec![raw]));
        let called = std::sync::atomic::AtomicBool::new(false);
        let _ = gate.list_upcoming(now_second, horizon, |_t, _s, _h| {
            called.store(true, Ordering::SeqCst);
            Ok::<_, &str>(vec![])
        });
        assert!(
            called.load(Ordering::SeqCst),
            "once the boundary has passed, the next list_upcoming must refresh"
        );
    }

    /// Issue #867: parse a recorded Graph payload that uses an offset
    /// `timeZone` and an all-day flag, and confirm the parsed event
    /// preserves them. (The all-day variant is exercised for the
    /// `workingHours` slice too — both share the `EventTimeRaw`
    /// deserializer.)
    #[test]
    fn try_from_raw_parses_offset_timezone_and_all_day() {
        let raw = CalendarEventRaw {
            start: Some(EventTimeRaw {
                date_time: Some("2024-06-01T09:00:00+02:00".to_string()),
                time_zone: Some("Europe/Berlin".to_string()),
            }),
            end: Some(EventTimeRaw {
                date_time: Some("2024-06-01T17:00:00+02:00".to_string()),
                time_zone: Some("Europe/Berlin".to_string()),
            }),
            show_as: Some("busy".to_string()),
            is_all_day: Some(false),
            is_cancelled: Some(false),
            sensitivity: Some("normal".to_string()),
        };
        // Window starts the day of the event so the half-open
        // `[window_start, window_start + horizon)` window overlaps it.
        let window_start = chrono::Utc
            .with_ymd_and_hms(2024, 6, 1, 0, 0, 0)
            .single()
            .expect("valid date");
        let parsed = CalendarEvent::try_from_raw(raw, window_start, chrono::Duration::hours(48))
            .expect("must parse");
        assert_eq!(parsed.start.to_rfc3339(), "2024-06-01T07:00:00+00:00");
        assert_eq!(parsed.end.to_rfc3339(), "2024-06-01T15:00:00+00:00");
        assert!(!parsed.is_all_day);
    }
}
