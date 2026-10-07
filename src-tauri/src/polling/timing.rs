//! Poll cadence: retry/backoff constants, failure counters, and sleep helpers (issue #754).

use std::sync::mpsc;

use chrono::Utc;
use rand::Rng;

use super::iteration::{PollIteration, RunMode};

pub(crate) const ERROR_RETRY_INTERVAL_SECONDS: u64 = 30;
pub(crate) const RATE_LIMIT_BACKOFF_SECONDS: u64 = 60;
pub(crate) const DEBOUNCE_MS: u64 = 500;
/// Issue #364: when a track change lands inside the debounce window the
/// change signal must survive — the retry parks here, NOT on the
/// duration-derived sleep (which would stall the pending write until the
/// track nearly ends).
pub(crate) const DEBOUNCE_RETRY_SECONDS: u64 = 1;
/// Issue #384: identical-status writes are skipped while the last write
/// is this fresh; older than this the next poll force-writes a keepalive
/// so the Graph expiry never lapses.
pub(crate) const STATUS_KEEPALIVE_SECONDS: u64 = 5 * 60;
pub(crate) const TRANSIENT_FAILURE_EXIT_THRESHOLD: u8 = 5;
/// Finding PollCore#0 (issue #568): consecutive NETWORK failures — transport
/// errors, 5xx, JSON parse failures and 429s — get their own counter and
/// threshold. They must never end the session nor emit
/// `spotify-reconnect-required`: the frontend turns that event into a real
/// Spotify OAuth window (`+layout.svelte`), which is user-hostile when the
/// tokens on disk are still valid and only the network is down. The counter
/// is reset by any successful iteration.
pub(crate) const NETWORK_FAILURE_THRESHOLD: u8 = 12;
/// Base of the capped exponential backoff applied once
/// `NETWORK_FAILURE_THRESHOLD` consecutive network failures accumulate.
pub(crate) const NETWORK_BACKOFF_BASE_SECONDS: u64 = 30;
/// Ceiling for that backoff: an offline machine slows to this cadence but
/// KEEPS polling (never `PollIteration::Break`).
pub(crate) const NETWORK_BACKOFF_CAP_SECONDS: u64 = 300;

/// Finding PollCore#0 (issue #568): the single place that resets BOTH
/// consecutive-failure counters. One helper so a success can never clear one
/// counter and leave the other primed — a stale network streak would then
/// survive healthy iterations and jump straight to the capped backoff.
pub(crate) fn record_success(
    transient_failure_count: &mut u8,
    consecutive_network_failures: &mut u8,
) {
    *transient_failure_count = 0;
    *consecutive_network_failures = 0;
}

/// Finding PollCore#0 (issue #568): the auth-only classification behind the
/// five-strikes reconnect exit. `ExpiredToken` (the 401 Spotify returns for a
/// dead access token) and `InvalidGrant` (a dead refresh token — documented
/// 6-month lifetime, or revoked) are the ONLY errors that mean "the stored
/// credentials are unusable, ask the user to re-authenticate". `Other(_)` is
/// every transport error, 5xx and JSON parse failure (see spotify.rs) and
/// `RateLimited` is a 429: both are recoverable network states that must keep
/// polling with the tokens already on disk.
///
/// `#[cfg(test)]` because the live path is now driven by `PlaybackSource`
/// (issue #862): `SpotifySource::poll` classifies `SpotifyApiError` into a
/// `SourceError` variant and the poll loop reacts to that taxonomy. The
/// classifier exists only to feed the unit tests below.
#[cfg(test)]
pub(crate) fn is_auth_failure(err: &crate::spotify::SpotifyApiError) -> bool {
    matches!(
        err,
        crate::spotify::SpotifyApiError::ExpiredToken
            | crate::spotify::SpotifyApiError::InvalidGrant
    )
}

/// Issue #262 (finding PollCore#0, issue #568): the five-strikes reconnect
/// decision, extracted as a pure function of the counter so the threshold
/// semantics are testable without driving the whole `run()` error path.
/// Returns `Some(Break)` exactly when the count has reached
/// `TRANSIENT_FAILURE_EXIT_THRESHOLD`, and `None` below it so the caller
/// keeps retrying after emitting its warning. The counter is only bumped for
/// auth failures (see [`is_auth_failure`]) and is reset by any success.
pub(crate) fn transient_outcome(count: u8) -> Option<PollIteration> {
    if count >= TRANSIENT_FAILURE_EXIT_THRESHOLD {
        Some(PollIteration::Break)
    } else {
        None
    }
}

pub(crate) fn interruptible_sleep(
    stop_rx: &mpsc::Receiver<()>,
    seconds: u64,
    label: &str,
    mode: RunMode,
) -> PollIteration {
    if mode == RunMode::OneShot {
        log::info!(
            "[POLLING] poll_once: one-shot, skipping {} (returning Break)",
            label
        );
        return PollIteration::Break;
    }
    match stop_rx.recv_timeout(std::time::Duration::from_secs(seconds)) {
        Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            log::info!(
                "[POLLING] poll_once: stop signal during {}, breaking",
                label
            );
            PollIteration::Break
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => PollIteration::Sleep { seconds: 0 },
    }
}

pub(crate) fn config_default_interval(
    config: &Option<std::sync::Arc<crate::config::AppConfig>>,
) -> u64 {
    config
        .as_ref()
        .map(|c| c.polling.default_interval_seconds)
        .unwrap_or(30)
}

pub(crate) fn config_minimum_interval(
    config: &Option<std::sync::Arc<crate::config::AppConfig>>,
) -> u64 {
    config
        .as_ref()
        .map(|c| c.polling.minimum_interval_seconds)
        .unwrap_or(10)
}

/// `polling.pause_backoff_max_seconds` (issue #538), defaulted to the
/// documented 300 s so an untouched config keeps 4.5 behaviour.
pub(crate) fn config_pause_backoff_max(
    config: &Option<std::sync::Arc<crate::config::AppConfig>>,
) -> u64 {
    config
        .as_ref()
        .map(|c| c.polling.pause_backoff_max_seconds)
        .unwrap_or(300)
}

pub(crate) fn config_maximum_interval(
    config: &Option<std::sync::Arc<crate::config::AppConfig>>,
) -> u64 {
    config
        .as_ref()
        .map(|c| c.polling.max_interval_seconds)
        .unwrap_or(60)
}

/// Server-directed sleep base for a Spotify 429 (issue #159): the
/// `Retry-After` seconds when present, else the default rate-limit backoff —
/// floored at the error retry interval so a tiny server value can't create a
/// busy loop.
///
/// `#[cfg(test)]` because the live path is now driven by `PlaybackSource`
/// (issue #862): `SpotifySource::poll` returns a `SourceError::RateLimited`
/// and the poll loop applies its own jitter at the call site. The helper
/// survives here as a unit-test target.
#[cfg(test)]
pub(crate) fn spotify_backoff_base(err: &crate::spotify::SpotifyApiError) -> u64 {
    err.retry_after()
        .unwrap_or(RATE_LIMIT_BACKOFF_SECONDS)
        .max(ERROR_RETRY_INTERVAL_SECONDS)
}

/// Finding PollCore#3 (issue #571): the actual sleep for a Spotify 429. A
/// server-directed `Retry-After` is a FLOOR — jitter may only extend it —
/// while the header-less fallback keeps the symmetric jitter. Pre-fix the
/// symmetric ±20% was applied to both, so `Retry-After: 300` could sleep 240s
/// and re-trigger the rate limit the header exists to avoid.
///
/// `#[cfg(test)]` for the same reason as [`spotify_backoff_base`]: the
/// live path is now `PlaybackSource::poll` → `SourceError::RateLimited` →
/// poll-loop jitter, the helper exists to feed the unit tests.
#[cfg(test)]
pub(crate) fn spotify_backoff_secs(err: &crate::spotify::SpotifyApiError) -> u64 {
    match err.retry_after() {
        Some(_) => with_upward_jitter(spotify_backoff_base(err)),
        None => with_jitter(spotify_backoff_base(err)),
    }
}

/// Issue #862 sibling of [`spotify_backoff_secs`]: the trait surface is
/// `SourceError`, not `SpotifyApiError`, so the poll loop's 429 path
/// inspects the string-form message the Spotify source produced. The
/// `retry_after` value travels in the message via the
/// `retry_after={Some(...)}` debug print; we re-extract it here and
/// fall back to the default backoff when the value is absent.
pub(crate) fn spotify_backoff_secs_retry_after(retry_after: Option<u64>) -> u64 {
    let secs = retry_after.unwrap_or(RATE_LIMIT_BACKOFF_SECONDS);
    let secs = secs.max(ERROR_RETRY_INTERVAL_SECONDS);
    if retry_after.is_some() {
        with_upward_jitter(secs)
    } else {
        with_jitter(secs)
    }
}

/// Pull the `retry_after={Some(N)}` debug form out of a `SourceError`
/// display string. Returns `None` when absent or unparseable.
pub(crate) fn extract_retry_after(msg: &str) -> Option<Option<u64>> {
    let start = msg.find("retry_after=")?;
    let after = &msg[start + "retry_after=".len()..];
    if after.starts_with("Some(") {
        let inner_start = "Some(".len();
        let inner_end = after[inner_start..].find(')')?;
        let n: u64 = after[inner_start..inner_start + inner_end].parse().ok()?;
        Some(Some(n))
    } else if after.starts_with("None") {
        Some(None)
    } else {
        None
    }
}

/// Extra sleep contributed by a failed Teams set/clear (issue #154): a
/// `RateLimited` error with `Retry-After` returns those seconds, without a
/// header falls back to the jittered default backoff, anything else
/// contributes nothing.
pub(crate) fn rate_limit_sleep_secs(err: &crate::teams::TeamsApiError) -> u64 {
    match err {
        crate::teams::TeamsApiError::RateLimited(Some(secs)) => *secs,
        crate::teams::TeamsApiError::RateLimited(None) => with_jitter(RATE_LIMIT_BACKOFF_SECONDS),
        _ => 0,
    }
}

/// Format a UTC instant as Graph's offset-less `dateTime` with 6 fraction
/// digits (≤ the documented 7). `to_rfc3339()` would embed `+00:00` and up
/// to 9 fraction digits, contradicting the dateTimeTimeZone schema (issue
/// #156).
pub(crate) fn format_expiry(expiry: chrono::DateTime<Utc>) -> String {
    expiry.format("%Y-%m-%dT%H:%M:%S%.6f").to_string()
}

/// Expiry for the short-lived "clear" placeholders (issue #155): now + 60s,
/// so the placeholder self-removes ~1 min after the last successful post even
/// if the app quits.
pub(crate) fn placeholder_expiry_str() -> String {
    let expiry = Utc::now() + chrono::Duration::seconds(60);
    format_expiry(expiry)
}

/// Sleep decision for a playing track. Known position → sleep until ~5s
/// before the track ends (clamped to the config bounds); unknown position
/// (live stream, issue #165) → the default interval, not a duration-derived
/// one.
pub(crate) fn playing_track_sleep(
    remaining_ms: Option<u64>,
    config: &Option<std::sync::Arc<crate::config::AppConfig>>,
) -> u64 {
    match remaining_ms {
        Some(remaining) => {
            let buffer_ms = 5000u64;
            let remaining_secs = remaining / 1000;
            clamp_poll_interval(remaining_secs.saturating_sub(buffer_ms / 1000), config)
        }
        None => clamp_poll_interval(config_default_interval(config), config),
    }
}

/// Finding PollCore#6 (issue #573): bound a computed sleep to the user's
/// configured `[minimum_interval_seconds, maximum_interval_seconds]` window.
/// Every sleep the poller takes must respect "Max interval (s)"; pre-fix only
/// `playing_track_sleep` did, while the 304 and no-track paths — the ones that
/// dominate idle runtime — slept raw values the config permits to exceed it.
/// Non-panicking clamp order (`max` then `min`) because a hand-edited config
/// could invert the bounds, which `u64::clamp` would panic on.
pub(crate) fn clamp_poll_interval(
    secs: u64,
    config: &Option<std::sync::Arc<crate::config::AppConfig>>,
) -> u64 {
    let minimum = config_minimum_interval(config);
    let maximum = config_maximum_interval(config).max(minimum);
    secs.max(minimum).min(maximum)
}

/// The documented pause ladder (issue #38: default → 2× → 4× → ceiling, see
/// ARCHITECTURE.md / TROUBLESHOOTING.md — both still describe the 300 s
/// default as the cap) is deliberately NOT bounded by
/// `maximum_interval_seconds`: it is the idle-work reduction the docs promise,
/// and clamping it by the default 60s max would silently multiply idle API
/// traffic. Finding PollCore#6 (issue #573) is therefore fixed at the one path
/// whose sleep was never a ladder rung — the tracked-track 304 (see
/// `not_modified_iteration`).
///
/// Issue #538 / CfgDiag#3(c): the ceiling is `ceiling_secs`, i.e.
/// `polling.pause_backoff_max_seconds` (clamped to 60..=3600 by
/// `config::clamp_polling`, default 300). Pre-fix the literal `300` was
/// hardcoded here, so the config key the Settings card exposes had no effect
/// on the ladder it is named after.
pub(crate) fn pause_backoff(consecutive_pauses: u8, default_secs: u64, ceiling_secs: u64) -> u64 {
    // A ceiling below the base would otherwise produce a ladder that shrinks
    // as the pause count grows.
    let ceiling = ceiling_secs.max(default_secs);
    match consecutive_pauses {
        0 => default_secs,
        1 => default_secs.saturating_mul(2).min(ceiling),
        2 => default_secs.saturating_mul(4).min(ceiling),
        _ => ceiling,
    }
}

pub(crate) fn with_jitter(base_secs: u64) -> u64 {
    let mut rng = rand::rng();
    let jitter_range = base_secs as f64 * 0.2;
    let jitter = rng.random_range(-jitter_range..=jitter_range);
    (base_secs as f64 + jitter).max(1.0) as u64
}

/// Finding PollCore#3 (issue #571): additive-only jitter, `base + 0..=20%`.
/// Used wherever the base is a server directive (`Retry-After`) that must
/// never be undershot.
pub(crate) fn with_upward_jitter(base_secs: u64) -> u64 {
    let mut rng = rand::rng();
    let jitter = rng.random_range(0.0..=(base_secs as f64 * 0.2));
    (base_secs as f64 + jitter) as u64
}

/// Finding PollCore#0 (issue #568): capped exponential backoff for repeated
/// network failures — `min(300, with_jitter(30) * 2^(n-1))` for `n`
/// consecutive failures — so an offline machine slows to the 5-minute ceiling
/// without ever stopping the poller. The doubling exponent is clamped so a
/// saturated counter cannot overflow the shift.
pub(crate) fn network_failure_backoff(count: u8) -> u64 {
    let doublings = count.saturating_sub(1).min(4) as u32;
    with_jitter(NETWORK_BACKOFF_BASE_SECONDS)
        .saturating_mul(1u64 << doublings)
        .min(NETWORK_BACKOFF_CAP_SECONDS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spotify::SpotifyApiError;
    use crate::teams::TeamsApiError;

    /// Issue #538: the ladder with the DOCUMENTED default ceiling (300 s), so
    /// an untouched config behaves exactly as it did in 4.5.
    #[test]
    fn test_pause_backoff_grows_then_caps() {
        assert_eq!(pause_backoff(0, 30, 300), 30);
        assert_eq!(pause_backoff(1, 30, 300), 60);
        assert_eq!(pause_backoff(2, 30, 300), 120);
        assert_eq!(pause_backoff(3, 30, 300), 300);
        assert_eq!(pause_backoff(4, 30, 300), 300);
        assert_eq!(pause_backoff(255, 30, 300), 300);
    }

    #[test]
    fn test_pause_backoff_uses_configured_default() {
        assert_eq!(pause_backoff(0, 45, 300), 45);
        assert_eq!(pause_backoff(1, 45, 300), 90);
        assert_eq!(pause_backoff(2, 45, 300), 180);
        assert_eq!(pause_backoff(3, 45, 300), 300);
    }

    #[test]
    fn test_pause_backoff_caps_with_large_default() {
        assert_eq!(pause_backoff(0, 200, 300), 200);
        assert_eq!(pause_backoff(1, 200, 300), 300);
        assert_eq!(pause_backoff(2, 200, 300), 300);
    }

    /// Issue #538: `polling.pause_backoff_max_seconds` IS the ladder's ceiling
    /// — the config key the Settings card exposes must govern the ladder it is
    /// named after (pre-fix `pause_backoff` hardcoded 300 and the key was
    /// consumed nowhere, so a user's 900 s ceiling changed nothing).
    #[test]
    fn test_pause_backoff_honours_the_configured_ceiling() {
        // A raised ceiling lets the ladder climb past the old 300 s literal.
        assert_eq!(pause_backoff(1, 120, 900), 240);
        assert_eq!(pause_backoff(2, 120, 900), 480);
        assert_eq!(pause_backoff(3, 120, 900), 900);
        assert_eq!(pause_backoff(255, 120, 900), 900);
        // A lowered ceiling caps sooner (clamp_polling's floor is 60).
        assert_eq!(pause_backoff(1, 30, 60), 60);
        assert_eq!(pause_backoff(2, 30, 60), 60);
        assert_eq!(pause_backoff(4, 30, 60), 60);
        // A ceiling below the base cannot invert the ladder.
        assert_eq!(pause_backoff(3, 120, 60), 120);
        // The accessor reads the config, defaulting to the documented 300.
        assert_eq!(config_pause_backoff_max(&None), 300);
        assert_eq!(
            config_pause_backoff_max(&Some(std::sync::Arc::new(
                crate::config::AppConfig::default()
            ))),
            300
        );
        let mut raised = crate::config::AppConfig::default();
        raised.polling.pause_backoff_max_seconds = 900;
        assert_eq!(
            config_pause_backoff_max(&Some(std::sync::Arc::new(raised))),
            900
        );
    }

    /// Issue #159: a Spotify 429 backoff honors the server's Retry-After,
    /// floored at the error retry interval so a tiny value can't busy-loop.
    #[test]
    fn test_spotify_backoff_base_honors_retry_after_floored() {
        assert_eq!(
            spotify_backoff_base(&SpotifyApiError::RateLimited(Some(45))),
            45
        );
        assert_eq!(
            spotify_backoff_base(&SpotifyApiError::RateLimited(Some(10))),
            ERROR_RETRY_INTERVAL_SECONDS,
            "retry-after below the floor must be clamped up"
        );
        assert_eq!(
            spotify_backoff_base(&SpotifyApiError::RateLimited(None)),
            RATE_LIMIT_BACKOFF_SECONDS,
            "header-less 429 falls back to the fixed backoff"
        );
    }

    /// Issue #154: a Teams set/clear failure contributes the server's
    /// Retry-After seconds, the jittered default backoff when the header is
    /// absent, and nothing for non-throttle errors.
    #[test]
    fn test_rate_limit_sleep_secs_teams() {
        assert_eq!(
            rate_limit_sleep_secs(&TeamsApiError::RateLimited(Some(90))),
            90
        );
        assert_eq!(rate_limit_sleep_secs(&TeamsApiError::ExpiredToken(401)), 0);
        assert_eq!(
            rate_limit_sleep_secs(&TeamsApiError::Forbidden(403, "denied".to_string())),
            0
        );
        assert_eq!(rate_limit_sleep_secs(&TeamsApiError::InvalidGrant), 0);
        assert_eq!(
            rate_limit_sleep_secs(&TeamsApiError::Transient("boom".to_string())),
            0
        );
        // Header-less 429 → jittered default backoff (60 ± 20% → [48, 72]).
        let no_header = rate_limit_sleep_secs(&TeamsApiError::RateLimited(None));
        assert!(
            (48..=72).contains(&no_header),
            "jittered backoff out of range: {}",
            no_header
        );
    }

    /// Issue #156: the expiry string must be offset-less with exactly 6
    /// fraction digits (≤ the documented 7) — no `+00:00`, no `Z`, no
    /// 9-digit nanosecond fraction.
    #[test]
    fn test_format_expiry_is_offset_less_with_six_fraction_digits() {
        let fixed = chrono::DateTime::parse_from_rfc3339("2015-02-18T23:16:09.123456789+00:00")
            .unwrap()
            .with_timezone(&Utc);
        let s = format_expiry(fixed);
        assert!(
            !s.contains('+') && !s.contains('Z'),
            "offset leaked into dateTime: {}",
            s
        );
        assert!(
            s.starts_with("2015-02-18T23:16:09."),
            "unexpected shape: {}",
            s
        );
        let fraction = s.split('.').nth(1).unwrap_or("");
        assert_eq!(
            fraction.len(),
            6,
            "expected exactly 6 fraction digits, got '{}'",
            fraction
        );
    }

    /// Issue #155/#156: the clear-path placeholder expiry must be offset-less
    /// with 6 fraction digits.
    #[test]
    fn test_placeholder_expiry_str_is_offset_less() {
        let s = placeholder_expiry_str();
        assert!(
            !s.contains('+') && !s.contains('Z'),
            "offset leaked into placeholder expiry: {}",
            s
        );
        let fraction = s.split('.').nth(1).unwrap_or("");
        assert_eq!(fraction.len(), 6, "got '{}'", fraction);
    }

    /// Issue #165: sleep falls back to the default interval for live streams
    /// instead of a duration-derived value; known positions sleep until ~5s
    /// before track end, clamped to the config bounds.
    #[test]
    fn test_playing_track_sleep_known_position_and_live_stream() {
        let config = Some(std::sync::Arc::new(crate::config::AppConfig::default()));
        // Default config: min 10s, max 60s.
        assert_eq!(playing_track_sleep(Some(30_000), &config), 25);
        assert_eq!(
            playing_track_sleep(Some(120_000), &config),
            60,
            "long remaining time clamps to max interval"
        );
        assert_eq!(
            playing_track_sleep(Some(2_000), &config),
            10,
            "short remaining time clamps to min interval"
        );
        assert_eq!(
            playing_track_sleep(None, &config),
            30,
            "live stream falls back to the default interval"
        );
        // No config → the built-in defaults (30s default, 10s min, 60s max).
        assert_eq!(playing_track_sleep(None, &None), 30);
        assert_eq!(playing_track_sleep(Some(2_000), &None), 10);
    }

    /// Issue #262: the 5-strikes transient-failure counter must break the
    /// polling loop at exactly `TRANSIENT_FAILURE_EXIT_THRESHOLD` — no
    /// sooner (a transient blip must not kill the session) and no later
    /// (a permanently broken token must stop hammering the API).
    ///
    /// Finding PollCore#0 (issue #568): the counter that feeds this decision is
    /// now bumped ONLY by `is_auth_failure` errors (dead access/refresh token)
    /// — a network failure has its own counter and can never reach this exit.
    #[test]
    fn test_transient_outcome_breaks_exactly_at_threshold() {
        // The issue names five strikes explicitly; pin the constant so a
        // future retune cannot silently change the documented contract (the
        // literal assertions below would otherwise follow it).
        // The counter is only reachable through auth failures (finding
        // PollCore#0, issue #568); see
        // `test_auth_failure_classification_is_dead_credentials_only`.
        assert_eq!(
            TRANSIENT_FAILURE_EXIT_THRESHOLD, 5,
            "issue #262 specifies exactly 5 consecutive transient failures"
        );
        assert!(
            transient_outcome(4).is_none(),
            "4 consecutive transient failures must NOT break the loop; the counter is \
             reset by any success, so an early break kills the session on a blip"
        );
        assert!(
            matches!(transient_outcome(5), Some(PollIteration::Break)),
            "5 consecutive transient failures MUST break the loop so the user is asked \
             to reconnect (issue #262)"
        );
        // Saturating-add can reach u8::MAX; the threshold decision must stay
        // stable there (no panic, still Break).
        assert!(
            matches!(transient_outcome(u8::MAX), Some(PollIteration::Break)),
            "a saturated counter must still break"
        );
    }
    /// Issue #477: the 5-strikes threshold is provider-scoped -- five
    /// consecutive transient Spotify failures break the loop so the
    /// caller emits the provider-specific `spotify-reconnect-required`
    /// alongside the generic signal. Below-threshold counts must not
    /// break (a blip must not kill the session).
    #[test]
    fn test_five_strikes_threshold_is_provider_scoped_break() {
        assert_eq!(
            TRANSIENT_FAILURE_EXIT_THRESHOLD, 5,
            "issue #262/#477 specifies exactly 5 consecutive transient failures"
        );
        for count in 0..5u8 {
            assert!(
                transient_outcome(count).is_none(),
                "{} transient failures must NOT break the loop",
                count
            );
        }
        assert!(
            matches!(transient_outcome(5), Some(PollIteration::Break)),
            "5 consecutive transient failures MUST break so the caller emits the provider signal"
        );
    }

    /// Finding PollCore#0 (issue #568): the reconnect exit is reachable ONLY
    /// from genuinely dead credentials. Pre-fix `Other(_)` — every transport
    /// error, 5xx and JSON parse failure — and `RateLimited(_)` counted toward
    /// the five-strikes exit, so five offline polls (~2.5 min) stopped syncing
    /// and made the frontend open a real Spotify OAuth window while perfectly
    /// valid tokens were still on disk.
    #[test]
    fn test_auth_failure_classification_is_dead_credentials_only() {
        assert!(
            is_auth_failure(&SpotifyApiError::ExpiredToken),
            "a rejected access token (401) must count toward the reconnect exit"
        );
        assert!(
            is_auth_failure(&SpotifyApiError::InvalidGrant),
            "a dead refresh token (invalid_grant) must count toward the reconnect exit"
        );
        for network in [
            SpotifyApiError::Other(
                "Failed to send currently playing request: connection refused".to_string(),
            ),
            SpotifyApiError::Other("Failed to parse currently playing response".to_string()),
            SpotifyApiError::Other("Currently playing request failed with 502".to_string()),
            SpotifyApiError::RateLimited(Some(30)),
            SpotifyApiError::RateLimited(None),
        ] {
            assert!(
                !is_auth_failure(&network),
                "a network/parse/429 failure must never count toward the reconnect exit: {:?}",
                network
            );
        }
    }

    /// Finding PollCore#0 (issue #568): repeated network failures double the
    /// backoff up to a hard cap — and, unlike the auth exit, they are warning
    /// material only: no constant in that path can stop the loop.
    #[test]
    fn test_network_failure_backoff_is_capped_and_grows() {
        assert_eq!(
            NETWORK_FAILURE_THRESHOLD, 12,
            "the network threshold must stay well above the auth threshold so an \
             offline blip can never stop the session"
        );
        // Compile-time invariant (clippy: move the constant assertion into a
        // const block) — the network threshold must stay strictly above the
        // auth threshold so no retune can make a network blip reach the exit.
        const { assert!(NETWORK_FAILURE_THRESHOLD > TRANSIENT_FAILURE_EXIT_THRESHOLD) };
        assert_eq!(
            NETWORK_BACKOFF_CAP_SECONDS, 300,
            "the cap is the documented ceiling for a network backoff"
        );
        // n = 1 → with_jitter(30) ∈ [24, 36].
        let first = network_failure_backoff(1);
        assert!(
            (24..=36).contains(&first),
            "unexpected first-rung backoff: {}",
            first
        );
        for count in 1..=u8::MAX {
            let secs = network_failure_backoff(count);
            assert!(
                secs <= NETWORK_BACKOFF_CAP_SECONDS,
                "count {} slept {}s, above the cap {}s",
                count,
                secs,
                NETWORK_BACKOFF_CAP_SECONDS
            );
            assert!(secs >= 1, "a zero-second backoff would busy-loop the API");
        }
        assert_eq!(
            network_failure_backoff(u8::MAX),
            network_failure_backoff(NETWORK_FAILURE_THRESHOLD),
            "a saturated counter must sit at the cap, not overflow the shift"
        );
    }

    /// Finding PollCore#0 (issue #568): one success clears BOTH counters, so a
    /// stale network streak cannot survive healthy iterations and jump
    /// straight to the capped backoff.
    #[test]
    fn test_record_success_resets_both_failure_counters() {
        let mut auth_failures = 4u8;
        let mut network_failures = 11u8;
        record_success(&mut auth_failures, &mut network_failures);
        assert_eq!(
            (auth_failures, network_failures),
            (0, 0),
            "a successful poll must clear both consecutive-failure counters"
        );
        assert!(
            transient_outcome(auth_failures).is_none(),
            "a cleared counter must not immediately exit the loop"
        );
    }

    /// Finding PollCore#3 (issue #571): the 429 sleep never undercuts the
    /// server's `Retry-After`. Pre-fix the symmetric ±20% jitter could turn
    /// `Retry-After: 300` into a 240s sleep and re-trigger the rate limit.
    #[test]
    fn test_spotify_backoff_secs_never_undershoots_retry_after() {
        for retry_after in [30u64, 45, 120, 300] {
            for _ in 0..200 {
                let secs = spotify_backoff_secs(&SpotifyApiError::RateLimited(Some(retry_after)));
                assert!(
                    secs >= retry_after,
                    "429 sleep {}s undercut the server's Retry-After of {}s",
                    secs,
                    retry_after
                );
                assert!(
                    secs <= retry_after + retry_after / 5,
                    "429 sleep {}s overshot Retry-After {}s beyond the +20% jitter budget",
                    secs,
                    retry_after
                );
            }
        }
        // A tiny server value is still floored at the error retry interval.
        for _ in 0..100 {
            let secs = spotify_backoff_secs(&SpotifyApiError::RateLimited(Some(1)));
            assert!(
                secs >= ERROR_RETRY_INTERVAL_SECONDS,
                "a tiny Retry-After must be floored at the error retry interval: {}",
                secs
            );
        }
        // The header-less fallback keeps the symmetric jitter (48..=72).
        for _ in 0..100 {
            let secs = spotify_backoff_secs(&SpotifyApiError::RateLimited(None));
            assert!(
                (48..=72).contains(&secs),
                "header-less 429 backoff out of range: {}",
                secs
            );
        }
        for _ in 0..100 {
            let secs = with_upward_jitter(100);
            assert!(
                (100..=120).contains(&secs),
                "upward jitter must only ever extend the base: {}",
                secs
            );
        }
    }

    /// Finding PollCore#6 (issue #573): the bounded clamp applies to the
    /// interval-derived sleeps it was introduced for, keeps the ladder's rungs
    /// intact (see `pause_backoff`), and a hand-edited config with inverted
    /// bounds clamps instead of panicking like `u64::clamp` would.
    #[test]
    fn test_clamp_poll_interval_bounds_and_inverted_config() {
        let mut narrow = crate::config::AppConfig::default();
        narrow.polling.default_interval_seconds = 30;
        narrow.polling.minimum_interval_seconds = 10;
        narrow.polling.max_interval_seconds = 60;
        let narrow = Some(std::sync::Arc::new(narrow));
        assert_eq!(clamp_poll_interval(120, &narrow), 60);
        assert_eq!(clamp_poll_interval(5, &narrow), 10);
        assert_eq!(clamp_poll_interval(45, &narrow), 45);
        assert_eq!(
            playing_track_sleep(Some(600_000), &narrow),
            60,
            "the playing path keeps its max-interval clamp"
        );

        let mut inverted = crate::config::AppConfig::default();
        inverted.polling.minimum_interval_seconds = 120;
        inverted.polling.max_interval_seconds = 5;
        let inverted = Some(std::sync::Arc::new(inverted));
        assert_eq!(
            clamp_poll_interval(30, &inverted),
            120,
            "inverted bounds must saturate at the minimum, never panic"
        );
        assert_eq!(clamp_poll_interval(1, &None), 10);
        assert_eq!(clamp_poll_interval(9_999, &None), 60);
    }

    /// The documented pause ladder (issue #38) is intentionally NOT clamped by
    /// `maximum_interval_seconds`: ARCHITECTURE.md and TROUBLESHOOTING.md
    /// promise "doubles up to a 5-min cap", and clamping it at the default 60s
    /// max would multiply idle API traffic five-fold.
    #[test]
    fn test_pause_ladder_is_unclamped_by_max_interval() {
        let mut narrow = crate::config::AppConfig::default();
        narrow.polling.default_interval_seconds = 30;
        narrow.polling.minimum_interval_seconds = 10;
        narrow.polling.max_interval_seconds = 60;
        let narrow = Some(std::sync::Arc::new(narrow));
        assert_eq!(
            pause_backoff(
                3,
                config_default_interval(&narrow),
                config_pause_backoff_max(&narrow)
            ),
            300
        );
        assert!(
            pause_backoff(
                3,
                config_default_interval(&narrow),
                config_pause_backoff_max(&narrow)
            ) > config_maximum_interval(&narrow),
            "the ladder's default 5-minute ceiling sits above the default max"
        );
    }
}
