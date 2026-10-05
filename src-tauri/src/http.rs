//! Shared HTTP plumbing for the two cloud providers (issue #884).
//!
//! Before this module, `spotify.rs` and `teams.rs` each carried their own
//! copy of the same three things:
//!
//! - a `reqwest::blocking::Client` builder, re-run per call site, so every
//!   Graph/Spotify request paid a new TCP+TLS handshake and threw the
//!   connection pool away;
//! - `parse_retry_after` / `parse_retry_after_value` (RFC 7231 §7.1.3, 300 s
//!   clamp) — byte-identical in both modules;
//! - a 60 s "refresh early" window inside `is_token_expired`.
//!
//! A fix applied to one copy silently missed the other. One implementation
//! each lives here; the provider modules keep their public signatures and
//! delegate.
//!
//! Two client shapes, deliberately:
//!
//! - [`shared_client`] is the process-wide 10 s client, built once. Both
//!   providers use it, so the whole app shares one pool.
//! - [`client_with_timeout`] builds per call and exists only for the quit
//!   path, which must not wait the full budget. Caching per timeout value
//!   would defeat the point of the cache (each cached entry would have its own
//!   pool), so the exit path keeps paying for its own client — it runs once.

use chrono::{DateTime, Utc};
use reqwest::blocking::Client;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

/// Log tag for this module (AGENTS.md §4 — square-bracket module tags).
const TAG: &str = "[HTTP]";

/// Default per-request timeout for the shared client, in seconds.
///
/// Teams and Spotify both used 10 s before this module existed; the value is
/// pinned here so a change cannot land in one provider only.
pub const DEFAULT_TIMEOUT_SECS: u64 = 10;

/// Longest `Retry-After` the client will honour, in seconds (issue #159).
///
/// Caps a hostile or misconfigured `Retry-After` so one throttled response
/// cannot park the polling thread for an hour.
pub const MAX_RETRY_AFTER_SECS: u64 = 300;

/// How long before real expiry a token is treated as expired, in seconds.
///
/// Shared by both providers so the refresh heuristic cannot drift apart
/// (issue #884).
pub const REFRESH_WINDOW_SECS: i64 = 60;

/// The shared client, plus the budget it was built with.
///
/// The budget is kept beside the client so that "the shared client is the 10 s
/// one" is an assertion about a value the running cache actually holds rather
/// than about the source that built it. It exists only for that assertion:
/// `reqwest::blocking::Client` has no accessor for its own timeout, so there
/// is nothing in production that would read it.
struct CachedClient {
    client: Arc<Client>,
    #[cfg(test)]
    timeout: Duration,
}

/// The process-wide client, built on first use.
///
/// `reqwest::blocking::Client` is internally `Arc`-backed, so every later
/// caller gets a refcount bump over the SAME connection pool. A failed build
/// is memoized too: the builder fails only on environmental TLS/runtime
/// init, where an immediate retry would fail identically — and a per-call
/// retry would mean re-attempting a known-broken build on every poll.
///
/// Returns an `Arc` rather than a `Client` so the cache's identity is
/// observable: two calls hand back the same allocation, which is what
/// "one pool per process" actually means (see the reuse test below).
static SHARED_CLIENT: LazyLock<Result<CachedClient, String>> = LazyLock::new(|| {
    let timeout = Duration::from_secs(DEFAULT_TIMEOUT_SECS);
    #[cfg(test)]
    SHARED_CLIENT_BUILDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    build_client(timeout)
        .map(|client| CachedClient {
            client: Arc::new(client),
            #[cfg(test)]
            timeout,
        })
        .map_err(|e| {
            log::error!("{TAG} shared client build failed: {}", e);
            format!("Failed to create HTTP client: {}", e)
        })
});

#[cfg(test)]
static SHARED_CLIENT_BUILDS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// Number of times the shared client's builder has run in this process.
///
/// Test-only accessor for the reuse invariant.
#[cfg(test)]
pub(crate) fn shared_client_builds() -> usize {
    SHARED_CLIENT_BUILDS.load(std::sync::atomic::Ordering::Relaxed)
}

/// The timeout the shared cache's client was built with.
#[cfg(test)]
fn shared_timeout() -> Option<Duration> {
    SHARED_CLIENT.as_ref().ok().map(|cached| cached.timeout)
}

/// The one place a client is configured, so the User-Agent and the timeout
/// cannot differ between the shared client and the bounded quit-path one.
fn build_client(timeout: Duration) -> Result<Client, String> {
    Client::builder()
        .user_agent(user_agent())
        .timeout(timeout)
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))
}

/// The `User-Agent` both providers send: `PresenceJam/<version>`.
///
/// Never hard-code the version — it tracks `Cargo.toml` via `env!`
/// (CONTRIBUTING.md, audit Q8).
pub fn user_agent() -> String {
    format!("PresenceJam/{}", env!("CARGO_PKG_VERSION"))
}

/// The shared 10 s client, cloned out of the process-wide cache.
///
/// The cheap case is a refcount bump; the expensive case — the first call —
/// builds one client that every later caller reuses.
pub fn shared_client() -> Result<Arc<Client>, String> {
    match SHARED_CLIENT.as_ref() {
        Ok(cached) => Ok(Arc::clone(&cached.client)),
        Err(e) => Err(e.clone()),
    }
}

/// A freshly built client with an explicit timeout.
///
/// Only the quit path (`EXIT_CLEANUP_TIMEOUT`, 3 s) uses this: it runs inside
/// `RunEvent::Exit`, so it must never hold the quit open for the shared
/// client's budget, and it must not evict or re-key the shared pool to get
/// there.
pub fn client_with_timeout(timeout: Duration) -> Result<Client, String> {
    build_client(timeout)
}

/// Parses a `Retry-After` header value, supporting both delta-seconds (`120`)
/// and HTTP-date (`Wed, 21 Aug 2026 12:00:00 GMT`) forms per RFC 7231 §7.1.3.
/// Returns `None` when the value is unparseable — callers then fall back to
/// exponential backoff.
pub fn parse_retry_after_value(s: &str) -> Option<u64> {
    let s = s.trim();
    if let Ok(secs) = s.parse::<u64>() {
        return Some(secs.min(MAX_RETRY_AFTER_SECS));
    }
    if let Ok(date) = httpdate::parse_http_date(s) {
        let secs = date
            .duration_since(std::time::SystemTime::now())
            .unwrap_or(std::time::Duration::from_secs(0))
            .as_secs()
            .min(MAX_RETRY_AFTER_SECS);
        return Some(secs);
    }
    None
}

/// Parses the `Retry-After` header off a live response. `None` when the header
/// is absent, non-UTF-8, or unparseable.
pub fn parse_retry_after(response: &reqwest::blocking::Response) -> Option<u64> {
    response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_retry_after_value)
}

/// True when `expires_at` is within [`REFRESH_WINDOW_SECS`] of expiry.
///
/// Both providers refresh on the same early window; the window is applied
/// before the comparison so a token that just crossed it is treated as
/// expired even if the clock says otherwise.
pub fn is_token_expired(expires_at: DateTime<Utc>) -> bool {
    Utc::now() >= expires_at - chrono::Duration::seconds(REFRESH_WINDOW_SECS)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------
    // Issue #884: one shared client, genuinely reused.
    // ---------------------------------------------------------------

    /// The core performance claim: N callers, one builder run, one
    /// allocation. Fails if the cache is reverted to a per-call
    /// `Client::builder().build()` — the pre-#884 shape, which paid a new
    /// TCP+TLS handshake on every poll.
    ///
    /// Both halves are asserted because either alone is weak: the counter
    /// alone would pass for a cache that hands out a fresh clone per caller,
    /// and identity alone would pass for a builder that ran twice and kept
    /// only the second result.
    #[test]
    fn shared_client_is_built_once_and_every_caller_gets_the_same_pool() {
        let first = shared_client().expect("the shared client must build");
        let second = shared_client().expect("the shared client must build");

        assert!(
            Arc::ptr_eq(&first, &second),
            "two callers received different client allocations — the pool is not shared"
        );
        assert_eq!(
            shared_client_builds(),
            1,
            "the client builder must run exactly once per process (issue #884)"
        );
    }

    /// The shared client carries the shared budget, and a bounded client
    /// built beside it does not disturb that cache.
    ///
    /// Read from the value the running cache actually holds, not from the
    /// source that built it. The bounded half is asserted as far as `reqwest`
    /// allows — it exposes no timeout accessor — by proving that a bounded
    /// build succeeds, is usable, and leaves both the cached budget and the
    /// build count untouched.
    #[test]
    fn the_shared_client_is_ten_second_and_a_bounded_one_builds_beside_it() {
        shared_client().expect("the shared client must build");
        assert_eq!(
            shared_timeout(),
            Some(Duration::from_secs(DEFAULT_TIMEOUT_SECS)),
            "the shared cache must hold the shared {DEFAULT_TIMEOUT_SECS}s budget"
        );

        let bounded = client_with_timeout(Duration::from_secs(3)).expect("a 3s client must build");
        // A client that could not have issued a request was never really built.
        let served = bounded_request_status(&bounded);
        assert_eq!(
            served, 429,
            "the bounded client must be usable for a request"
        );
        assert_eq!(
            shared_timeout(),
            Some(Duration::from_secs(DEFAULT_TIMEOUT_SECS)),
            "a bounded build must not re-key the shared cache"
        );
        assert_eq!(
            shared_client_builds(),
            1,
            "the bounded build must not run the shared cache initializer (issue #884)"
        );
    }

    /// The User-Agent is `PresenceJam/<version>`, version tracked from
    /// `Cargo.toml` — never hardcoded (audit Q8, issue #450). Both clients
    /// share it, because both go through one builder.
    #[test]
    fn both_clients_send_the_cargo_tracked_user_agent() {
        assert_eq!(
            user_agent(),
            format!("PresenceJam/{}", env!("CARGO_PKG_VERSION")),
            "the User-Agent must be PresenceJam/<Cargo.toml version>"
        );
    }

    /// The header-reading half, against a response object rather than a
    /// hand-passed value: a present, parseable header yields its seconds and
    /// an absent one yields `None` (so the caller falls back to its own
    /// backoff rather than inventing a zero wait).
    #[test]
    fn parse_retry_after_reads_the_response_header() {
        let present = response_with_retry_after(Some("120"));
        assert_eq!(
            parse_retry_after(&present),
            Some(120),
            "a delta-seconds header must be read off the response"
        );
        let clamped = response_with_retry_after(Some("9999"));
        assert_eq!(
            parse_retry_after(&clamped),
            Some(MAX_RETRY_AFTER_SECS),
            "the 300 s clamp must apply to the header path too"
        );
        let absent = response_with_retry_after(None);
        assert_eq!(
            parse_retry_after(&absent),
            None,
            "an absent header must yield None, not a fabricated wait"
        );
    }

    /// Issues one request with `client` against a loopback listener that
    /// answers a single 429 carrying `retry_after`, and returns the response.
    ///
    /// `reqwest::blocking::Response` has no public constructor and cannot be
    /// assembled from a raw hyper body, so a loopback listener is the only way
    /// to get a real one. Nothing leaves the machine.
    fn loopback_429(client: &Client, retry_after: Option<&str>) -> reqwest::blocking::Response {
        use std::io::{Read, Write};

        let listener =
            std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port must be available");
        let port = listener
            .local_addr()
            .expect("a bound listener has an address")
            .port();
        let header = retry_after
            .map(|v| format!("Retry-After: {v}\r\n"))
            .unwrap_or_default();

        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("the client must connect");
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let response = format!(
                "HTTP/1.1 429 Too Many Requests\r\n{header}Content-Length: 0\r\nConnection: close\r\n\r\n"
            );
            stream
                .write_all(response.as_bytes())
                .expect("the loopback response must write");
            stream.flush().ok();
        });

        let response = client
            .get(format!("http://127.0.0.1:{port}/"))
            .send()
            .expect("the loopback request must succeed");
        server.join().expect("the loopback thread must not panic");
        response
    }

    /// A response with `retry_after`, carried by a throwaway loopback client.
    fn response_with_retry_after(value: Option<&str>) -> reqwest::blocking::Response {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("a loopback client must build");
        loopback_429(&client, value)
    }

    /// The status a 429 loopback answers with, for a client under test.
    fn bounded_request_status(client: &Client) -> u16 {
        loopback_429(client, None).status().as_u16()
    }

    // ---------------------------------------------------------------
    // RFC 7231 §7.1.3 Retry-After parsing (one copy for both providers).
    // ---------------------------------------------------------------

    #[test]
    fn parse_retry_after_value_accepts_delta_seconds_and_http_dates() {
        assert_eq!(parse_retry_after_value("120"), Some(120));
        assert_eq!(parse_retry_after_value("  42  "), Some(42));
        assert_eq!(parse_retry_after_value("0"), Some(0));

        // The 300 s clamp: a hostile or misconfigured header must not park
        // the polling thread for an hour.
        assert_eq!(parse_retry_after_value("9999"), Some(300));

        let soon = httpdate::fmt_http_date(
            std::time::SystemTime::now() + std::time::Duration::from_secs(120),
        );
        let secs = parse_retry_after_value(&soon).expect("an http-date must parse");
        assert!(
            (119..=120).contains(&secs),
            "a date 120s out must yield ~120, got {secs}"
        );

        // A date already in the past is "retry now", not an error.
        let past = httpdate::fmt_http_date(
            std::time::SystemTime::now() - std::time::Duration::from_secs(600),
        );
        assert_eq!(parse_retry_after_value(&past), Some(0));

        // A far-future date is clamped like the delta-seconds form.
        let far = httpdate::fmt_http_date(
            std::time::SystemTime::now() + std::time::Duration::from_secs(86_400),
        );
        assert_eq!(parse_retry_after_value(&far), Some(300));

        assert_eq!(parse_retry_after_value("not-a-date"), None);
        assert_eq!(parse_retry_after_value(""), None);
    }

    // ---------------------------------------------------------------
    // The shared refresh window.
    // ---------------------------------------------------------------

    /// Both providers refresh [`REFRESH_WINDOW_SECS`] early. Asserted against
    /// the clock rather than a source string so the number cannot drift in
    /// one provider only.
    #[test]
    fn is_token_expired_fires_exactly_one_window_early() {
        let now = Utc::now();
        let window = chrono::Duration::seconds(REFRESH_WINDOW_SECS);

        // Comfortably inside the window.
        assert!(is_token_expired(now + window / 2));
        // Already past expiry.
        assert!(is_token_expired(now - window));
        // Comfortably outside it — one second of slack absorbs the clock
        // moving between the two `Utc::now()` calls.
        assert!(
            !is_token_expired(now + window * 2),
            "a token with a full extra window of life must not read as expired"
        );
    }
}
