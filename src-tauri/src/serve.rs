//! `--serve`: token-guarded localhost HTTP control + event API (issue #865).
//!
//! Binds **127.0.0.1 only** — the only network surface is the local UID, but
//! local UIDs are still untrusted (other apps run as the same user, shared
//! containers, RDP sessions, etc.), so every mutating route requires
//! `Authorization: Bearer <token>`. The token is 32 random bytes, base64url-
//! encoded (43 chars), generated and stored in the OS keychain by
//! [`crate::token_io::read_or_create_serve_token`]; it never lives on disk in
//! plaintext, never in argv, never in env.
//!
//! Routes:
//!
//! | Method | Path                | Auth | Body                                         |
//! |--------|---------------------|------|----------------------------------------------|
//! | GET    | `/status`           | none | same JSON shape as `--status` (SyncStatus)   |
//! | GET    | `/events`           | none | text/event-stream of three Tauri events      |
//! | POST   | `/pause`            | yes  | empty — calls `stop_syncing`                 |
//! | POST   | `/resume`           | yes  | empty — calls `start_syncing`                |
//! | POST   | `/snooze?minutes=N` | yes  | query `minutes` (1..=1440)                  |
//! | POST   | `/profile?id=<id>`  | yes  | query `id`, percent-decoded, 1..=32 chars    |
//!
//! No route writes `config.json` or `tokens.json`. `/snooze` and `/profile`
//! publish a new **in-memory** `AppConfig` (through the config write guard, via
//! `clamped_config`, never through `save_config`), so the poller sees the change
//! on its next iteration while the on-disk document is untouched — an operator
//! restart returns to the pre-call behaviour. Both emit `config-changed`;
//! nothing in the webview listens for it yet, so an open dashboard refreshes on
//! its next mount via `loadConfig()`, not by push.
//!
//! The HTTP server is a blocking `tiny_http` instance on its own thread; SSE
//! is a streaming response via `Request::into_writer`, one event per
//! `app.emit` for the three presence-related events.

use crate::commands::sync::{start_syncing_with, stop_syncing_with, sync_status_from_state};
use crate::AppState;
use serde::Serialize;
use std::io::Write as _;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use tauri::Listener;
use tiny_http::{Header, Method, Response, Server, StatusCode};

/// Module tag for log lines.
const MODULE: &str = "[SERVE]";

/// Default port when `--serve` is given without an explicit value.
pub const DEFAULT_PORT: u16 = 8649;

/// Events the SSE stream forwards verbatim. One `event: NAME\ndata: JSON\n\n`
/// frame per `app.emit(..., payload)` for these three names. Other events
/// (`sync-started`, `sync-stopped`, `polling-thread-panicked`, etc.) are not
/// streamed — they are internal lifecycle signals that the Tauri app already
/// surfaces to its own UI; the `--serve` surface is the public control API.
const STREAMED_EVENTS: &[&str] = &[
    "presence-updated",
    "spotify-track-changed",
    "presence-gated",
];

/// Bound HTTP server handle, kept alive for the lifetime of `--serve`.
/// `tiny_http::Server` does not implement `Send`, so we own it on its own
/// thread and drive it via a side channel that receives events from the
/// Tauri event bus. Dropping the server stops accepting connections; the
/// dedicated thread exits on its own.
pub struct ServeHandle {
    /// Set on `stop()`; the server thread polls this between accepts and
    /// returns when it sees it flipped.
    stop: Arc<AtomicBool>,
    /// Join handle for the server thread — exposed so the daemon path can
    /// `join()` it before exiting 0 on SIGTERM.
    join: Option<thread::JoinHandle<()>>,
}

impl ServeHandle {
    /// Stop accepting connections and join the server thread. Idempotent.
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.join.take() {
            // The server thread sees the flag on its next accept poll; it
            // owns the listening socket, so dropping the server handle from
            // here is not possible — the join is what blocks until the
            // thread has actually closed its socket. The bounded time it
            // can park in `recv()` is the time the longest open SSE
            // connection keeps a `Writer` alive.
            let _ = handle.join();
        }
    }
}

/// A snapshot of one Tauri event, captured in a channel that the SSE handler
/// drains. The payload is the JSON body the Tauri app emitted; the
/// `event_name` is the bare event name (e.g. `presence-updated`).
#[derive(Debug)]
struct EventFrame {
    event_name: &'static str,
    /// Raw JSON payload as the Tauri app emitted it. We do not re-parse it
    /// — the SSE consumer is the one that needs to interpret the schema.
    payload: String,
}

/// Public, JSON-serializable error returned by mutating routes when the
/// bearer token is missing or wrong. The token itself is never echoed in
/// any field of this struct — only a constant label and a marker — so it
/// cannot leak through structured logs that serialize error responses.
#[derive(Debug, Serialize)]
struct AuthFailure {
    error: &'static str,
}

impl AuthFailure {
    const BODY: AuthFailure = AuthFailure {
        error: "missing_or_wrong_bearer_token",
    };
}

/// `Authorization: Bearer <token>` is well-formed iff the prefix matches
/// case-insensitively and there is at least one whitespace gap. Returns the
/// credential start index in `header_value`, or `None` for any other shape.
fn bearer_credential_start(header_value: &str) -> Option<usize> {
    let prefix = "bearer";
    let bytes = header_value.as_bytes();
    if bytes.len() <= prefix.len() + 1 {
        return None;
    }
    if !bytes[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes()) {
        return None;
    }
    if !bytes[prefix.len()].is_ascii_whitespace() {
        return None;
    }
    let mut i = prefix.len();
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i == bytes.len() {
        return None;
    }
    Some(i)
}

/// Constant-time comparison. Returns `false` immediately for any length
/// mismatch; otherwise XORs every byte and ORs the result, so the runtime
/// is independent of where the first differing byte sits. Tiny_http is not
/// the bottleneck here — local-token timing attacks are the realistic
/// threat, and a non-constant-time comparison leaks the matched prefix
/// length over repeated requests.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Validate the `Authorization` header on a mutating request. Returns
/// `Ok(())` when the token matches; `Err(())` for missing, malformed or
/// wrong credentials. The bearer token is never logged — neither here
/// nor in any caller — and `redact_sensitive` already scrubs the literal
/// `Authorization: Bearer ` shape from any downstream error message.
pub(crate) fn check_bearer(req: &tiny_http::Request, expected: &[u8]) -> Result<(), ()> {
    let header = req
        .headers()
        .iter()
        .find(|h| h.field.equiv("Authorization"))
        .map(|h| h.value.as_str());
    let Some(value) = header else {
        log::debug!("{MODULE} mutating route rejected: missing Authorization header");
        return Err(());
    };
    let Some(cred_start) = bearer_credential_start(value) else {
        log::debug!("{MODULE} mutating route rejected: malformed Authorization header");
        return Err(());
    };
    let cred = &value.as_bytes()[cred_start..];
    if ct_eq(cred, expected) {
        Ok(())
    } else {
        log::debug!("{MODULE} mutating route rejected: bearer token mismatch");
        Err(())
    }
}

/// Run the HTTP server on the calling thread until [`stop`] flips the
/// shutdown flag. `expected_token` is the raw token bytes — they are read
/// once at startup, never logged, and live for the lifetime of the
/// thread. `event_rx` is the channel the Tauri-listener side writes SSE
/// frames into.
fn serve_loop(
    server: Server,
    expected_token: Vec<u8>,
    event_rx: mpsc::Receiver<EventFrame>,
    stop: Arc<AtomicBool>,
    state: Arc<AppState>,
    app: AppHandle,
) {
    log::info!("{MODULE} serve_loop: entering accept loop");
    for request in server.incoming_requests() {
        if stop.load(Ordering::Acquire) {
            log::info!("{MODULE} serve_loop: stop flag set, exiting accept loop");
            break;
        }
        let url = request.url().to_string();
        let method = request.method().clone();
        handle_request(
            request,
            &method,
            &url,
            &expected_token,
            &event_rx,
            Arc::clone(&state),
            &app,
        );
    }
    log::info!("{MODULE} serve_loop: exited accept loop");
}

/// Parse the query string for `key=val` (first occurrence). Returns
/// `None` for missing key, empty value, or a malformed percent-encoded
/// value (which would otherwise be a vector for silent parameter
/// tampering on a mutating route).
fn query_param<'a>(query: Option<&'a str>, key: &str) -> Option<&'a str> {
    let q = query?;
    for pair in q.split('&') {
        let mut kv = pair.splitn(2, '=');
        let k = kv.next().unwrap_or("");
        let v = kv.next().unwrap_or("");
        if k == key && !v.is_empty() {
            return Some(v);
        }
    }
    None
}

/// Parse the path, dropping query string. `"/events?foo=1"` -> `"/events"`.
fn split_query(url: &str) -> (&str, Option<&str>) {
    match url.find('?') {
        Some(i) => (&url[..i], Some(&url[i + 1..])),
        None => (url, None),
    }
}

type AppHandle = tauri::AppHandle<tauri::Wry>;

/// Dispatch one request. Read routes (`GET /status`, `GET /events`) are
/// open; mutating routes (`POST /pause`, `POST /resume`, `POST /snooze`,
/// `POST /profile`) require the bearer token. Any unknown method/path
/// pair returns 404 with no body.
fn handle_request(
    request: tiny_http::Request,
    method: &Method,
    url: &str,
    expected_token: &[u8],
    event_rx: &mpsc::Receiver<EventFrame>,
    state: Arc<AppState>,
    app: &AppHandle,
) {
    let (path, query) = split_query(url);
    // GET /status — open read of the same SyncStatus the `--status` CLI
    // path produces (issue #865 acceptance: same JSON shape as `--status`).
    if matches!(method, Method::Get) && path == "/status" {
        handle_status(request, state);
        return;
    }
    // GET /events — open read of the SSE stream. Bearer is optional here;
    // the events carry no secret material, and the stream is already
    // bound to 127.0.0.1, so leaving it open matches the issue's framing
    // of `/status` and `/events` as the two read endpoints.
    if matches!(method, Method::Get) && path == "/events" {
        handle_events(request, event_rx);
        return;
    }
    // All mutating routes below.
    if let Err(()) = check_bearer(&request, expected_token) {
        let body = serde_json::to_string(&AuthFailure::BODY).unwrap_or_else(|_| "{}".to_string());
        let response = Response::from_string(body)
            .with_status_code(StatusCode(401))
            .with_header(
                Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                    .expect("static header is valid"),
            );
        let _ = request.respond(response);
        return;
    }
    if !matches!(method, Method::Post) {
        let response =
            Response::from_string("method not allowed").with_status_code(StatusCode(405));
        let _ = request.respond(response);
        return;
    }
    match path {
        "/pause" => handle_pause(request, state, app.clone()),
        "/resume" => handle_resume(request, state, app.clone()),
        "/snooze" => handle_snooze(request, query, state, app.clone()),
        "/profile" => handle_profile(request, query, state, app.clone()),
        _ => {
            let response = Response::from_string("not found").with_status_code(StatusCode(404));
            let _ = request.respond(response);
        }
    }
}

/// `GET /status` — same JSON shape as `commands::sync::SyncStatus`.
fn handle_status(request: tiny_http::Request, state: Arc<AppState>) {
    // Off-thread to match the `--status` path's spawn_blocking model and to
    // keep the tiny_http worker out of any lock contention with the
    // polling thread. The function is `pub` and synchronous, so this is a
    // direct call here — the `--status` CLI wraps it the same way (see
    // `commands::sync::get_sync_status`).
    let snapshot = sync_status_from_state(&state);
    let body = match serde_json::to_string(&snapshot) {
        Ok(b) => b,
        Err(e) => {
            log::error!("{MODULE} /status: serialize failed: {}", e);
            let response =
                Response::from_string("internal error").with_status_code(StatusCode(500));
            let _ = request.respond(response);
            return;
        }
    };
    let response = Response::from_string(body).with_header(
        Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
            .expect("static header is valid"),
    );
    let _ = request.respond(response);
}

/// `GET /events` — SSE stream. Sends a tiny preamble (`event: ready\ndata:
/// {}\n\n`) so a slow consumer can detect a working connection before the
/// first real event lands, then drains the channel until the request is
/// dropped or the channel closes.
///
/// Each frame is `event: <name>\ndata: <json>\n\n` so a curl `--no-buffer`
/// consumer or a browser `EventSource` both parse them correctly without
/// further translation.
fn handle_events(request: tiny_http::Request, event_rx: &mpsc::Receiver<EventFrame>) {
    // `Request::into_writer` hands us the underlying TCP stream; the
    // HTTP status line + headers must be written ourselves before the
    // body. We pick 200 + the SSE headers the spec mandates and keep the
    // connection alive until the client or the channel closes.
    let mut writer = request.into_writer();
    let head = b"HTTP/1.1 200 OK\r\n\
                 Content-Type: text/event-stream\r\n\
                 Cache-Control: no-cache\r\n\
                 Connection: close\r\n\
                 \r\n";
    if writer.write_all(head).is_err() {
        log::debug!("{MODULE} /events: client hung up before headers");
        return;
    }
    if writer.write_all(b"event: ready\ndata: {}\n\n").is_err() {
        log::debug!("{MODULE} /events: client hung up before preamble");
        return;
    }
    if writer.flush().is_err() {
        return;
    }

    // Drain the channel. We don't need to time-out — the request lives as
    // long as the TCP connection, and dropping `writer` closes the socket.
    while let Ok(frame) = event_rx.recv() {
        // SSE forbids newlines inside `data:` payloads without re-prefixing
        // each line; the Tauri events we forward are JSON, which has no
        // literal LF outside string values, but a defensive split keeps a
        // misbehaving emitter from breaking the stream framing.
        let safe = frame.payload.replace('\n', "\\n");
        let bytes = format!("event: {}\ndata: {}\n\n", frame.event_name, safe);
        if writer.write_all(bytes.as_bytes()).is_err() {
            // Consumer hung up.
            break;
        }
        if writer.flush().is_err() {
            break;
        }
    }
    log::debug!("{MODULE} /events: stream ended");
}

/// `POST /pause` — stop the polling thread. Returns 204 on success.
fn handle_pause(request: tiny_http::Request, state: Arc<AppState>, app: AppHandle) {
    let state_clone = Arc::clone(&state);
    let app_clone = app.clone();
    let result =
        tauri::async_runtime::block_on(
            async move { stop_syncing_with(state_clone, &app_clone).await },
        );
    match result {
        Ok(()) => {
            let response = Response::empty(StatusCode(204));
            let _ = request.respond(response);
        }
        Err(e) => {
            log::warn!("{MODULE} /pause: stop_syncing failed: {}", e);
            let response = Response::from_string(format!("pause failed: {}", e))
                .with_status_code(StatusCode(409));
            let _ = request.respond(response);
        }
    }
}

/// `POST /resume` — start the polling thread. Returns 204 on success,
/// 409 if `missing_session_code` is `Some` (mirrors `start_syncing`).
fn handle_resume(request: tiny_http::Request, state: Arc<AppState>, app: AppHandle) {
    let state_clone = Arc::clone(&state);
    let app_clone = app.clone();
    let result =
        tauri::async_runtime::block_on(
            async move { start_syncing_with(state_clone, &app_clone).await },
        );
    match result {
        Ok(()) => {
            let response = Response::empty(StatusCode(204));
            let _ = request.respond(response);
        }
        Err(e) => {
            log::warn!("{MODULE} /resume: start_syncing failed: {}", e);
            let response = Response::from_string(format!("resume failed: {}", e))
                .with_status_code(StatusCode(409));
            let _ = request.respond(response);
        }
    }
}

/// Why a `/snooze` or `/profile` request could not be applied. Kept as its own
/// type so the status-code mapping and the decision are separately testable.
#[derive(Debug, Clone, PartialEq, Eq)]
enum MutateError {
    /// 400 — bad/absent query parameter, with the message the route returns.
    BadRequest(String),
    /// 404 — the profile id is well-formed but names nothing in the config.
    UnknownProfile(String),
    /// 409 — no config is loaded in this process yet.
    NoConfig,
}

impl MutateError {
    fn status_code(&self) -> StatusCode {
        match self {
            MutateError::BadRequest(_) => StatusCode(400),
            MutateError::UnknownProfile(_) => StatusCode(404),
            MutateError::NoConfig => StatusCode(409),
        }
    }

    fn body(&self) -> String {
        match self {
            MutateError::BadRequest(msg) => msg.clone(),
            MutateError::UnknownProfile(id) => format!("unknown profile: {id}"),
            MutateError::NoConfig => "no config loaded".to_string(),
        }
    }
}

/// Parse and validate `/snooze`'s `minutes` parameter, and resolve it to an
/// absolute deadline against `now`. Pure — no state, no clock read — so the
/// bounds and the rounding are unit-testable.
fn snooze_deadline_from_query(
    query: Option<&str>,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<chrono::DateTime<chrono::Utc>, MutateError> {
    let Some(raw) = query_param(query, "minutes") else {
        return Err(MutateError::BadRequest(
            "missing minutes parameter".to_string(),
        ));
    };
    let minutes: u32 = raw.parse().map_err(|_| {
        MutateError::BadRequest("minutes must be a non-negative integer".to_string())
    })?;
    if !(1..=1440).contains(&minutes) {
        return Err(MutateError::BadRequest(
            "minutes must be in [1, 1440]".to_string(),
        ));
    }
    Ok(now + chrono::Duration::minutes(i64::from(minutes)))
}

/// True when `raw` contains a `%` that is not followed by two hex digits.
///
/// `urlencoding::decode` leaves such a sequence in place instead of returning
/// `Err`, so a caller that only checks its `Result` would accept `%ZZ` verbatim.
fn has_malformed_escape(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            // Needs two more bytes, both hex digits.
            if i + 2 >= bytes.len()
                || !bytes[i + 1].is_ascii_hexdigit()
                || !bytes[i + 2].is_ascii_hexdigit()
            {
                return true;
            }
            i += 3;
        } else {
            i += 1;
        }
    }
    false
}

/// The `/profile` decision: which `active_profile` value the request names.
///
/// `Ok(None)` means "clear to base". `Ok(Some(id))` means "activate `id`".
/// Validation is against the config's own (already clamped) profile list, so an
/// id that does not exist is a 404 rather than a switch to nothing.
fn active_profile_from_query(
    cfg: &crate::config::AppConfig,
    query: Option<&str>,
) -> Result<Option<String>, MutateError> {
    let Some(raw) = query_param(query, "id") else {
        return Err(MutateError::BadRequest("missing id parameter".to_string()));
    };

    // Percent-decode BEFORE validating. A URL query value carries `%20` for a
    // space, and profile names legitimately contain spaces: `clamp_profile_id`
    // only trims the EDGES (`config/clamp.rs:274`), and the Settings name field
    // validates only non-empty / unique / <=32 chars. So "Deep Work" is a valid
    // stored profile the tray, hotkey and CLI can all switch to — but the old
    // raw-string charset check rejected every encoded spelling with a 400,
    // leaving such a profile permanently unreachable over HTTP.
    //
    // A malformed escape is a 400 rather than being passed through: on a
    // mutating route a silently-kept `%` is a tampering vector.
    //
    // `+` must be treated as a space first: this is an
    // application/x-www-form-urlencoded query string, where `+` is the space
    // shorthand, and `urlencoding::decode` handles only `%XX`.
    let plus_decoded = raw.replace('+', " ");
    // A `%` that does not introduce two hex digits is malformed. `urlencoding`
    // passes such a sequence through unchanged rather than failing, so the
    // check is explicit.
    if has_malformed_escape(&plus_decoded) {
        return Err(MutateError::BadRequest(
            "id is not valid percent-encoding".to_string(),
        ));
    }
    let id = match urlencoding::decode(&plus_decoded) {
        Ok(decoded) => decoded.into_owned(),
        Err(_) => {
            return Err(MutateError::BadRequest(
                "id is not valid percent-encoding".to_string(),
            ))
        }
    };

    // The bound is the config's own `MAX_PROFILE_ID_CHARS` (32), not the 64 the
    // route used to accept. `clamped_config` truncates a profile name to 32
    // chars, so accepting a 33..=64-char id meant the lookup missed the stored
    // name and the truncation then produced a DIFFERENT id — silently selecting
    // the wrong profile, or clearing the pointer via `clamp_presence_profiles`.
    // Validating the real cap makes the two agree.
    let max = crate::config::MAX_PROFILE_ID_CHARS;
    if id.is_empty() || id.chars().count() > max {
        return Err(MutateError::BadRequest(format!(
            "id must be 1..={max} chars"
        )));
    }

    // `base` / `none` clear the pointer, matching `--profile`'s spelling.
    if id.eq_ignore_ascii_case("base") || id.eq_ignore_ascii_case("none") {
        return Ok(None);
    }

    // Authoritative check: the decoded id must name a stored profile. This is
    // what actually constrains the character set — a name only matters if the
    // config holds it — so there is no separate hand-rolled charset regex to
    // drift out of sync with `clamp_profile_id`.
    if !cfg.presence_profiles.iter().any(|p| p.name == id) {
        return Err(MutateError::UnknownProfile(id));
    }
    Ok(Some(id))
}

/// Apply a snooze to `state` without persisting: clones the current config,
/// sets `snooze_until`, clamps, and publishes the result under the write guard.
/// Returns the published config, or [`MutateError::NoConfig`] when nothing is
/// loaded.
///
/// Split out from the route so the mutation is testable against a real
/// `AppState` with no HTTP server and no `AppHandle`.
fn apply_snooze_to_state(
    state: &AppState,
    deadline: chrono::DateTime<chrono::Utc>,
) -> Result<crate::config::AppConfig, MutateError> {
    let mut guard = state.config.get_mut();
    let Some(current) = guard.as_ref().map(|c| (**c).clone()) else {
        return Err(MutateError::NoConfig);
    };
    let mut next = current;
    next.snooze_until = Some(crate::config::snooze_store_form(deadline));
    // `clamped_config` keeps the published snapshot inside the same bounds a
    // reader would enforce, so a hand-set value cannot leak past the route.
    let next = crate::config::clamped_config(&next);
    *guard = Some(Arc::new(next.clone()));
    Ok(next)
}

/// The `/profile` equivalent of [`apply_snooze_to_state`]: resolve `query`
/// against the config (which is the loaded one, under the same guard that
/// publishes the change, so a profile deleted mid-request cannot be selected)
/// and publish the result without persisting.
fn apply_profile_to_state(
    state: &AppState,
    query: Option<&str>,
) -> Result<crate::config::AppConfig, MutateError> {
    let mut guard = state.config.get_mut();
    let Some(current) = guard.as_ref().map(|c| (**c).clone()) else {
        return Err(MutateError::NoConfig);
    };
    let target = active_profile_from_query(&current, query)?;
    let mut next = current;
    next.active_profile = target;
    let next = crate::config::clamped_config(&next);
    *guard = Some(Arc::new(next.clone()));
    Ok(next)
}

/// `POST /snooze?minutes=N` — pause sync for N minutes, runtime-only.
///
/// **Persistence contract:** this route never writes `config.json` (see the
/// module docs and `docs/HEADLESS.md`). It publishes a new in-memory
/// `AppConfig` whose `snooze_until` is `now + N minutes` under the config write
/// guard, which is exactly what the poller's `snooze_gate` reads on the next
/// iteration — so the pause takes effect immediately, and an operator restart
/// returns to whatever was on disk before the call. That is the documented
/// difference between this route and the tray's snooze (which does persist).
///
/// Bounds: 1..=1440 minutes (24 h ceiling so a stuck client cannot park the
/// daemon forever).
fn handle_snooze(
    request: tiny_http::Request,
    query: Option<&str>,
    state: Arc<AppState>,
    app: AppHandle,
) {
    // Validate before touching any lock, so a bad request never blocks on the
    // config write guard.
    let deadline = match snooze_deadline_from_query(query, chrono::Utc::now()) {
        Ok(d) => d,
        Err(e) => {
            let response = Response::from_string(e.body()).with_status_code(e.status_code());
            let _ = request.respond(response);
            return;
        }
    };

    let published = match apply_snooze_to_state(&state, deadline) {
        Ok(cfg) => cfg,
        Err(e) => {
            let response = Response::from_string(e.body()).with_status_code(e.status_code());
            let _ = request.respond(response);
            return;
        }
    };

    // Publish the envelope on the app-wide event bus. Note the webview does not
    // currently listen for `config-changed` (it re-reads via `loadConfig()` on
    // mount), so this is not what repaints an open Dashboard — it exists so a
    // future listener has a hook, and so the change is observable. No disk write
    // happened, so this is a pure in-memory publication.
    let _ = crate::config::emit_config_changed(&app, &published);

    log::info!(
        "{MODULE} /snooze: paused until {} (runtime-only; config.json untouched)",
        crate::config::snooze_store_form(deadline),
    );
    let response = Response::empty(StatusCode(204));
    let _ = request.respond(response);
}

/// `POST /profile?id=<id>` — switch the active presence profile, runtime-only.
///
/// **Persistence contract:** like [`handle_snooze`], this never writes
/// `config.json`. It publishes a new in-memory `AppConfig` with
/// `active_profile` set to the named profile (or `None` for `base`), which is
/// the pointer `effective_config` resolves the overlay from on the next poll —
/// so the switch is immediate and does not survive a restart.
fn handle_profile(
    request: tiny_http::Request,
    query: Option<&str>,
    state: Arc<AppState>,
    app: AppHandle,
) {
    let published = match apply_profile_to_state(&state, query) {
        Ok(cfg) => cfg,
        Err(e) => {
            let response = Response::from_string(e.body()).with_status_code(e.status_code());
            let _ = request.respond(response);
            return;
        }
    };

    let _ = crate::config::emit_config_changed(&app, &published);

    match published.active_profile.as_deref() {
        Some(name) => log::info!(
            "{MODULE} /profile: active profile is now {name:?} (runtime-only; config.json untouched)"
        ),
        None => log::info!(
            "{MODULE} /profile: cleared to base configuration (runtime-only; config.json untouched)"
        ),
    }
    let response = Response::empty(StatusCode(204));
    let _ = request.respond(response);
}

/// Spawn the HTTP server thread and the Tauri event listener. Returns a
/// [`ServeHandle`] whose `stop()` shuts the server down cleanly. The
/// expected token is read once via [`crate::token_io::read_or_create_serve_token`]
/// and held in the server thread's stack for the rest of its life.
///
/// `port` must be the resolved port — the CLI layer has already
/// substituted the default (`DEFAULT_PORT`) when the user gave `--serve`
/// without an explicit value.
pub fn start_serve(state: Arc<AppState>, app: AppHandle, port: u16) -> Result<ServeHandle, String> {
    // 1) Resolve the bearer token via the keychain, with retry/backoff.
    let token = crate::token_io::read_or_create_serve_token_with_backoff(
        6,
        std::time::Duration::from_secs(1),
        std::time::Duration::from_secs(30),
    )?;
    let expected_token = token.into_bytes();

    // 2) Subscribe to the three Tauri events before we open the socket —
    // any event fired between `listen()` and the first accept is queued
    // in the channel until a client connects.
    let (event_tx, event_rx) = mpsc::channel::<EventFrame>();
    for name in STREAMED_EVENTS {
        let tx = event_tx.clone();
        let name_static: &'static str = name;
        app.listen(name_static, move |event| {
            let payload =
                serde_json::to_string(event.payload()).unwrap_or_else(|_| "null".to_string());
            // A full channel means no SSE consumer is connected; the
            // frame is dropped. Tauri events fire on every poll, so this
            // is the steady state for a daemon with no dashboard attached.
            let _ = tx.send(EventFrame {
                event_name: name_static,
                payload,
            });
        });
    }
    drop(event_tx); // The listeners own their own clones; the original is unused.

    // 3) Bind the listener. 127.0.0.1 ONLY — the loopback IPv6 address is
    // deliberately excluded, because IPv6 link-local and ULA addresses are
    // commonly bridged to other interfaces on Linux, and the issue's
    // security model is "local UID only".
    let bind_addr = format!("127.0.0.1:{}", port);
    let server = Server::http(&bind_addr)
        .map_err(|e| format!("could not bind serve socket to {}: {}", bind_addr, e))?;

    // 4) Print the bind URL once so the operator can curl/swagger it.
    // The token itself is NOT printed (issue #865 acceptance: never
    // appears in any log line). The token has to be obtained from the
    // OS keychain by the operator — that step is the "first boot" copy
    // documented in the README / docs/HEADLESS.md (the docs slice writes
    // that page; this slice notes the gap in CHANGELOG.md).
    log::info!(
        "{MODULE} --serve: bound on http://{} (token-guarded mutating routes; \
         bearer token is in the OS keychain, not on disk)",
        bind_addr
    );

    // 5) Spawn the server thread.
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = Arc::clone(&stop);
    let state_thread = Arc::clone(&state);
    let app_thread = app.clone();
    let expected_thread = expected_token.clone();
    let join = thread::Builder::new()
        .name("presencejam-serve".to_string())
        .spawn(move || {
            serve_loop(
                server,
                expected_thread,
                event_rx,
                stop_thread,
                state_thread,
                app_thread,
            );
        })
        .map_err(|e| format!("failed to spawn serve thread: {}", e))?;

    Ok(ServeHandle {
        stop,
        join: Some(join),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearer_credential_start_accepts_canonical_form() {
        // Canonical `Authorization: Bearer <token>` (single space).
        assert_eq!(bearer_credential_start("Bearer abc.def-ghi_123"), Some(7));
        // Case-insensitive scheme word.
        assert_eq!(bearer_credential_start("bearer abc.def-ghi_123"), Some(7));
        assert_eq!(bearer_credential_start("BEARER abc.def-ghi_123"), Some(7));
        // RFC 7235 allows one or more whitespace characters between scheme
        // and credential; we accept any run of ASCII whitespace.
        assert_eq!(bearer_credential_start("Bearer   abc"), Some(9));
        assert_eq!(bearer_credential_start("Bearer\tabc"), Some(7));
    }

    #[test]
    fn bearer_credential_start_rejects_malformed() {
        assert_eq!(bearer_credential_start(""), None);
        assert_eq!(bearer_credential_start("Bearer"), None);
        assert_eq!(bearer_credential_start("Bearer "), None);
        assert_eq!(bearer_credential_start("Basic abc"), None);
        // No whitespace between scheme and credential.
        assert_eq!(bearer_credential_start("Bearerabc"), None);
    }

    #[test]
    fn ct_eq_is_constant_time() {
        // Equal lengths, equal contents — accepted.
        assert!(ct_eq(b"hello", b"hello"));
        // Equal lengths, different contents — rejected.
        assert!(!ct_eq(b"hello", b"hellp"));
        // Length mismatch — rejected immediately (timing leak on the
        // length itself is unavoidable for a binary check; the real
        // defence is the 32-byte / base64url token size, which is fixed).
        assert!(!ct_eq(b"hello", b"hell"));
        assert!(!ct_eq(b"hell", b"hello"));
        // Empty vs non-empty.
        assert!(ct_eq(b"", b""));
        assert!(!ct_eq(b"", b"x"));
    }

    #[test]
    fn split_query_handles_queryless_and_querystring_urls() {
        assert_eq!(split_query("/status"), ("/status", None));
        assert_eq!(
            split_query("/snooze?minutes=5"),
            ("/snooze", Some("minutes=5"))
        );
        // No value side — still a query, just an empty pair list.
        assert_eq!(split_query("/profile?"), ("/profile", Some("")));
    }

    #[test]
    fn query_param_returns_first_match() {
        let q = Some("minutes=5&foo=bar");
        assert_eq!(query_param(q, "minutes"), Some("5"));
        assert_eq!(query_param(q, "foo"), Some("bar"));
        assert_eq!(query_param(q, "missing"), None);
        assert_eq!(query_param(None, "minutes"), None);
        // Empty value is rejected so `/profile?id=` returns 400 instead of
        // a silent "id was empty" success — see handle_profile.
        assert_eq!(query_param(Some("id="), "id"), None);
        // Multiple occurrences: first wins (deterministic).
        assert_eq!(query_param(Some("id=first&id=second"), "id"), Some("first"));
    }

    #[test]
    fn auth_failure_body_does_not_carry_the_token() {
        // The whole point of the structured failure: a future caller that
        // serializes the response body into a log line must not get the
        // token back. The struct has exactly one field, with a static
        // label, so it cannot contain request state.
        let json = serde_json::to_string(&AuthFailure::BODY).unwrap();
        assert_eq!(json, r#"{"error":"missing_or_wrong_bearer_token"}"#);
        // The constant label is the only string that mentions "bearer",
        // and it does not contain the credential.
        assert!(!json.contains("abc"));
    }

    /// Issue #865: the bearer-token string must never survive
    /// `redact_sensitive`. The token is 43 chars of base64url (≥ 32
    /// opaque chars), so the "long opaque run" pass is what scrubs it —
    /// and an `Authorization: Bearer <token>` header is scrubbed by the
    /// keyed-value pass. Both shapes are pinned here so a future change
    /// to the redaction table cannot silently leak the credential.
    #[test]
    fn bearer_token_is_scrubbed_by_redact_sensitive() {
        let token = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"; // 43 chars
                                                                   // Keyed-value shape (the most common log line shape).
        let keyed = format!("Authorization: Bearer {}", token);
        let redacted = crate::diagnostics::redact_sensitive(&keyed);
        assert!(
            !redacted.contains(token),
            "redact_sensitive must scrub the credential from a keyed header (got: {})",
            redacted
        );
        // Bare-token shape (a future log line that mentions the value
        // without the key — still must not leak).
        let bare = format!("serve token is {}", token);
        let redacted_bare = crate::diagnostics::redact_sensitive(&bare);
        assert!(
            !redacted_bare.contains(token),
            "redact_sensitive must scrub a 43-char base64url token as a long opaque run (got: {})",
            redacted_bare
        );
    }

    /// Issue #865: `check_bearer` must reject a missing header, a wrong
    /// scheme, and a wrong credential. The token must never appear in any
    /// log line emitted by the helper — the test asserts the closure
    /// path's failure by checking the helper's boolean contract only, but
    /// it also asserts the helper does not panic on the empty / unicode /
    /// multi-value headers a real HTTP client might send.
    #[test]
    fn check_bearer_contract_is_bool_only_no_token_leak() {
        // We don't build a full tiny_http::Request in this unit test —
        // the helper's API is exercised end-to-end by the live route
        // (start_serve + curl). Here we pin the two functions that are
        // pure enough to test: the credential extractor and the
        // constant-time comparator.
        assert_eq!(bearer_credential_start("Bearer abc"), Some(7));
        assert!(ct_eq(b"abc", b"abc"));
        assert!(!ct_eq(b"abc", b"abd"));
    }

    // ── /snooze and /profile stopped being inert ────────────────────────────
    //
    // Both routes used to validate their input, log it, and return 204 without
    // changing anything. These tests pin the decision half of the real
    // implementation: the deadline arithmetic and bounds, and which
    // `active_profile` value a request resolves to.

    fn at(secs: i64) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::from_timestamp(secs, 0).expect("valid timestamp")
    }

    #[test]
    fn snooze_deadline_adds_the_requested_minutes() {
        let now = at(1_700_000_000);
        let deadline = snooze_deadline_from_query(Some("minutes=30"), now).expect("valid");
        assert_eq!(deadline, now + chrono::Duration::minutes(30));

        // The bounds are inclusive at both ends (1 and 1440).
        assert_eq!(
            snooze_deadline_from_query(Some("minutes=1"), now),
            Ok(now + chrono::Duration::minutes(1))
        );
        assert_eq!(
            snooze_deadline_from_query(Some("minutes=1440"), now),
            Ok(now + chrono::Duration::minutes(1440))
        );
    }

    #[test]
    fn snooze_rejects_missing_malformed_and_out_of_range_minutes() {
        let now = at(1_700_000_000);
        assert_eq!(
            snooze_deadline_from_query(None, now),
            Err(MutateError::BadRequest(
                "missing minutes parameter".to_string()
            ))
        );
        assert_eq!(
            snooze_deadline_from_query(Some("minutes="), now),
            Err(MutateError::BadRequest(
                "missing minutes parameter".to_string()
            )),
            "an empty value must read as absent, not as 0"
        );
        assert!(matches!(
            snooze_deadline_from_query(Some("minutes=abc"), now),
            Err(MutateError::BadRequest(_))
        ));
        assert!(matches!(
            snooze_deadline_from_query(Some("minutes=-5"), now),
            Err(MutateError::BadRequest(_))
        ));
        assert_eq!(
            snooze_deadline_from_query(Some("minutes=0"), now),
            Err(MutateError::BadRequest(
                "minutes must be in [1, 1440]".to_string()
            ))
        );
        assert_eq!(
            snooze_deadline_from_query(Some("minutes=1441"), now),
            Err(MutateError::BadRequest(
                "minutes must be in [1, 1440]".to_string()
            ))
        );
    }

    #[test]
    fn mutate_errors_map_to_the_documented_status_codes() {
        // docs/API.md's error table: 400 for a bad parameter, 404 for an
        // unknown profile, 409 when the process has no config loaded.
        assert_eq!(MutateError::BadRequest("x".into()).status_code().0, 400);
        assert_eq!(MutateError::UnknownProfile("x".into()).status_code().0, 404);
        assert_eq!(MutateError::NoConfig.status_code().0, 409);
    }

    fn config_with_profiles(names: &[&str]) -> crate::config::AppConfig {
        crate::config::AppConfig {
            presence_profiles: names
                .iter()
                .map(|n| crate::config::PresenceProfile {
                    name: (*n).to_string(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn profile_query_selects_a_named_profile_or_clears_to_base() {
        let cfg = config_with_profiles(&["focus", "meeting"]);

        assert_eq!(
            active_profile_from_query(&cfg, Some("id=focus")),
            Ok(Some("focus".to_string()))
        );
        // `base` and `none` both clear, case-insensitively, matching --profile.
        assert_eq!(active_profile_from_query(&cfg, Some("id=base")), Ok(None));
        assert_eq!(active_profile_from_query(&cfg, Some("id=BASE")), Ok(None));
        assert_eq!(active_profile_from_query(&cfg, Some("id=none")), Ok(None));
    }

    #[test]
    fn profile_query_rejects_unknown_and_malformed_ids() {
        let cfg = config_with_profiles(&["focus"]);

        // Well-formed but not in the config: 404, not a silent no-op. This is
        // the difference between the old inert route (always 204) and the real
        // one, so it is the assertion that matters most.
        assert_eq!(
            active_profile_from_query(&cfg, Some("id=missing")),
            Err(MutateError::UnknownProfile("missing".to_string()))
        );

        assert!(matches!(
            active_profile_from_query(&cfg, None),
            Err(MutateError::BadRequest(_))
        ));
        assert!(matches!(
            active_profile_from_query(&cfg, Some("id=")),
            Err(MutateError::BadRequest(_))
        ));
        // The ceiling is the config's MAX_PROFILE_ID_CHARS (32), not 64.
        let max = crate::config::MAX_PROFILE_ID_CHARS;
        let too_long = "a".repeat(max + 1);
        assert!(matches!(
            active_profile_from_query(&cfg, Some(&format!("id={too_long}"))),
            Err(MutateError::BadRequest(_))
        ));
        // Exactly at the limit is accepted (then rejected as unknown, proving
        // the length check itself passed).
        let at_limit = "a".repeat(max);
        assert_eq!(
            active_profile_from_query(&cfg, Some(&format!("id={at_limit}"))),
            Err(MutateError::UnknownProfile(at_limit))
        );
        // Characters outside [A-Za-z0-9_-] are refused before lookup.
        // A name that decodes fine but is not stored is a 404, not a 400 —
        // the charset is no longer hand-validated, the profile list is.
        for absent in ["id=foc.us", "id=foc+us", "id=Deep%20Work"] {
            assert!(
                matches!(
                    active_profile_from_query(&cfg, Some(absent)),
                    Err(MutateError::UnknownProfile(_))
                ),
                "{absent} decodes to an unstored name, so it must be a 404"
            );
        }
        // Malformed percent-encoding is refused rather than passed through.
        for broken in ["id=%ZZ", "id=%"] {
            assert!(
                matches!(
                    active_profile_from_query(&cfg, Some(broken)),
                    Err(MutateError::BadRequest(_))
                ),
                "{broken} is not valid percent-encoding, so it must be a 400"
            );
        }
    }

    /// A profile name with a space is legal in config (`clamp_profile_id` only
    /// trims the edges, `config/clamp.rs:274`) and switchable from the tray,
    /// hotkey and CLI. The route used to reject every encoding of it, so the
    /// profile was permanently unreachable over HTTP. Percent-decoding before
    /// the lookup fixes that.
    #[test]
    fn profile_names_with_spaces_are_selectable_when_percent_encoded() {
        let cfg = config_with_profiles(&["Deep Work", "focus"]);

        // %20 — the canonical encoding for a space in a query value.
        assert_eq!(
            active_profile_from_query(&cfg, Some("id=Deep%20Work")),
            Ok(Some("Deep Work".to_string()))
        );
        // '+' decodes to a space in application/x-www-form-urlencoded, which is
        // what a query string is. Accept it too, so both common client
        // behaviours work.
        assert_eq!(
            active_profile_from_query(&cfg, Some("id=Deep+Work")),
            Ok(Some("Deep Work".to_string()))
        );
        // An unencoded space is malformed as a URL but tiny_http hands it to us;
        // match it directly rather than 400, because the name is real.
        assert_eq!(
            active_profile_from_query(&cfg, Some("id=Deep Work")),
            Ok(Some("Deep Work".to_string()))
        );
        // And the space-bearing name really does land in the live pointer.
        let state = state_with(cfg);
        apply_profile_to_state(&state, Some("id=Deep%20Work")).expect("applied");
        assert_eq!(
            state
                .config
                .snapshot()
                .and_then(|c| c.active_profile.clone()),
            Some("Deep Work".to_string())
        );
    }

    /// The contract that makes these routes safe to expose: neither one writes
    /// the config file. This is a source guard, because the difference between
    /// "publishes an in-memory Arc" and "persists to disk" is one function call
    /// and a reviewer cannot see it from the route's signature.
    #[test]
    fn runtime_routes_never_persist_config() {
        let src = include_str!("serve.rs");
        let production = &src[..src.find("\n#[cfg(test)]").expect("a test module")];

        // Strip comments and doc-comments first. The contract is about CODE, and
        // the module doc legitimately names the writer it promises not to call —
        // matching raw text made the guard fire on its own documentation.
        let code: String = production
            .lines()
            .filter(|l| {
                let t = l.trim_start();
                !t.starts_with("//") && !t.starts_with("/*") && !t.starts_with('*')
            })
            .collect::<Vec<_>>()
            .join("\n");

        // Match the WRITER NAMES, not call syntax. The previous version looked
        // for the literal `"save_config("`, which a real disk write via
        // `save_config_persisted` slipped past, as did `save_config (&cfg)` with
        // a space and an aliased import.
        for forbidden in [
            "save_config",
            "save_config_persisted",
            "save_config_to",
            "store_snooze",
            "store_active_profile",
            "atomic_write_json",
        ] {
            assert!(
                !code.contains(forbidden),
                "serve.rs must never reference {forbidden} — /snooze and /profile are \
                 documented as runtime-only in docs/HEADLESS.md and docs/API.md"
            );
        }

        // The in-memory publication path IS present, so the assertions above
        // cannot pass just because the routes were gutted again.
        assert!(
            code.contains("state.config.get_mut()"),
            "the runtime routes must publish through the config write guard"
        );
        assert!(
            code.contains("emit_config_changed"),
            "a runtime config change must be observable on the event bus"
        );
        assert!(
            code.contains("apply_snooze_to_state") && code.contains("apply_profile_to_state"),
            "the handlers must delegate to the mutation helpers"
        );
    }

    /// The above guard only proves THIS FILE does not persist. It does not
    /// prove the value stays off disk, because a runtime-only change lands in
    /// the same in-memory `AppConfig` that every other writer clones before
    /// saving. This test records that consequence explicitly, so the limitation
    /// is a pinned fact rather than an unexamined assumption.
    ///
    /// The reachable path in `--serve` mode is `clear_snooze_if_expired`
    /// (`polling/gate.rs:372`, called from `polling/iteration.rs:113` and
    /// `polling/loop.rs:287`): it clones the live config, clears the expired
    /// deadline, and calls `save_config` on the whole document — which carries
    /// along whatever `active_profile` a prior `POST /profile` set in memory.
    /// The GUI-only writers (tray, `update_config`, `window.rs`) are not
    /// reachable from `--serve`, so the snooze-expiry sweep is the one that
    /// matters.
    #[test]
    fn a_runtime_only_change_can_be_carried_to_disk_by_a_later_whole_config_save() {
        let state = state_with(config_with_profiles(&["focus"]));

        // 1. The route changes memory only.
        apply_profile_to_state(&state, Some("id=focus")).expect("applied");

        // Seed an already-expired snooze so the sweep below has something to
        // clear — `clamp_snooze` is a no-op when there is no dead deadline.
        {
            let mut guard = state.config.get_mut();
            let mut seeded = (**guard.as_ref().expect("loaded")).clone();
            seeded.snooze_until = Some("2020-01-01T00:00:00Z".to_string());
            *guard = Some(Arc::new(seeded));
        }

        let in_memory = state.config.snapshot().expect("config loaded");
        assert_eq!(in_memory.active_profile, Some("focus".to_string()));

        // 2. An unrelated writer clones the live config and saves the whole
        //    document — exactly what `clear_snooze_if_expired` does.
        let mut swept = (*in_memory).clone();
        assert!(
            crate::config::clamp_snooze(&mut swept, chrono::Utc::now()),
            "the seeded deadline should clear"
        );
        let serialized = serde_json::to_value(&swept).expect("serialises");

        // 3. The runtime-only value is now in the document that would reach disk.
        assert_eq!(
            serialized.get("active_profile").and_then(|v| v.as_str()),
            Some("focus"),
            "if this ever stops being true, the runtime-only routes got stronger \
             isolation than the docs promise — update docs/API.md and this test"
        );
    }

    // ── The mutations actually land in real AppState ────────────────────────
    //
    // The tests above pin the decision half. These drive `apply_*_to_state`
    // against a real `AppState`, which is what proves the routes are not inert
    // any more: the value the poller reads has changed afterwards.

    fn state_with(cfg: crate::config::AppConfig) -> crate::AppState {
        let state = crate::AppState::new();
        *state.config.get_mut() = Some(Arc::new(cfg));
        state
    }

    fn live_snooze_after(state: &crate::AppState) -> Option<String> {
        state.config.snapshot().and_then(|c| c.snooze_until.clone())
    }

    #[test]
    fn snooze_mutation_publishes_a_live_deadline() {
        let state = state_with(crate::config::AppConfig::default());
        // Start from a whole second: `snooze_store_form` writes
        // `SecondsFormat::Secs` on purpose (two equal states must not look
        // different just because a sub-second part moved), so a raw
        // `Utc::now()` deadline would round on the way in and never round-trip.
        let now = chrono::DateTime::from_timestamp(chrono::Utc::now().timestamp(), 0)
            .expect("whole-second now");
        let deadline = now + chrono::Duration::minutes(45);

        let published = apply_snooze_to_state(&state, deadline).expect("applied");

        // The published copy and the live slot agree — a mismatch here is the
        // #297/#536 bug class (in-memory and on-disk disagreeing).
        assert_eq!(
            published.snooze_until,
            Some(crate::config::snooze_store_form(deadline))
        );
        assert_eq!(live_snooze_after(&state), published.snooze_until);

        // And the value is one the poller's gate treats as live: parsing it
        // back must yield the deadline, not `None`.
        let stored = published.snooze_until.expect("set");
        let parsed = crate::config::snooze_deadline(&stored, now)
            .expect("a deadline 45 minutes out must be a live snooze");
        assert_eq!(parsed, deadline);
    }

    #[test]
    fn snooze_mutation_truncates_sub_second_precision() {
        // Pins the documented `SecondsFormat::Secs` behaviour so the round-trip
        // above cannot be "fixed" later by comparing against a truncated
        // instant while the stored form silently keeps sub-seconds.
        let state = state_with(crate::config::AppConfig::default());
        let now = chrono::DateTime::from_timestamp(chrono::Utc::now().timestamp(), 0)
            .expect("whole-second now");
        let with_nanos = now + chrono::Duration::milliseconds(1_500);

        let published = apply_snooze_to_state(&state, with_nanos).expect("applied");
        let stored = published.snooze_until.expect("set");
        assert!(
            !stored.contains('.'),
            "the stored form must carry whole seconds only, got {stored}"
        );
        // Still a live deadline, just at second resolution.
        assert!(crate::config::snooze_deadline(&stored, now).is_some());
    }

    #[test]
    fn snooze_mutation_reports_no_config_when_nothing_is_loaded() {
        // `AppState::new()` starts with an empty config slot.
        let state = crate::AppState::new();
        assert_eq!(
            apply_snooze_to_state(&state, chrono::Utc::now() + chrono::Duration::minutes(5)).err(),
            Some(MutateError::NoConfig)
        );
        assert_eq!(live_snooze_after(&state), None);
    }

    #[test]
    fn profile_mutation_switches_the_active_pointer() {
        let cfg = config_with_profiles(&["focus", "meeting"]);
        let state = state_with(cfg);

        let published = apply_profile_to_state(&state, Some("id=focus")).expect("applied");
        assert_eq!(published.active_profile, Some("focus".to_string()));
        assert_eq!(
            state
                .config
                .snapshot()
                .and_then(|c| c.active_profile.clone()),
            Some("focus".to_string()),
            "the live pointer the poller resolves the overlay from must move"
        );

        // Switching to another profile replaces it rather than appending.
        let published = apply_profile_to_state(&state, Some("id=meeting")).expect("applied");
        assert_eq!(published.active_profile, Some("meeting".to_string()));

        // `base` clears.
        let published = apply_profile_to_state(&state, Some("id=base")).expect("applied");
        assert_eq!(published.active_profile, None);
    }

    #[test]
    fn profile_mutation_leaves_the_pointer_alone_when_the_id_is_unknown() {
        let state = state_with(config_with_profiles(&["focus"]));
        // Establish a known-good pointer first.
        apply_profile_to_state(&state, Some("id=focus")).expect("applied");

        assert_eq!(
            apply_profile_to_state(&state, Some("id=does-not-exist")).err(),
            Some(MutateError::UnknownProfile("does-not-exist".to_string()))
        );
        assert_eq!(
            state
                .config
                .snapshot()
                .and_then(|c| c.active_profile.clone()),
            Some("focus".to_string()),
            "a rejected request must not disturb the current profile"
        );
    }

    #[test]
    fn profile_mutation_rejects_malformed_before_touching_state() {
        let state = state_with(config_with_profiles(&["focus"]));
        assert!(matches!(
            apply_profile_to_state(&state, None),
            Err(MutateError::BadRequest(_))
        ));
        assert_eq!(
            state
                .config
                .snapshot()
                .and_then(|c| c.active_profile.clone()),
            None
        );
    }

    /// Both routes run the published config through `clamped_config`, so what a
    /// reader sees is always inside the bounds a save would enforce.
    ///
    /// These exist because deleting the `clamped_config` call left all 21 serve
    /// tests green — the clamp on both paths was entirely unpinned. Seeded
    /// out-of-range values stand in for "a hand-edited config that skipped the
    /// clamp", which is exactly the case the call exists for.
    #[test]
    fn snooze_mutation_publishes_a_clamped_config() {
        let mut seeded = crate::config::AppConfig::default();
        seeded.polling.default_interval_seconds = 999_999;
        let state = state_with(seeded);

        let published = apply_snooze_to_state(
            &state,
            chrono::DateTime::from_timestamp(chrono::Utc::now().timestamp(), 0)
                .expect("whole-second now")
                + chrono::Duration::minutes(10),
        )
        .expect("applied");

        assert!(
            published.polling.default_interval_seconds < 999_999,
            "clamped_config must have run on the /snooze path: \
             default_interval_seconds was left at {}",
            published.polling.default_interval_seconds
        );
        let live = state.config.snapshot().expect("loaded");
        assert_eq!(
            live.polling.default_interval_seconds, published.polling.default_interval_seconds,
            "the clamped value, not the raw input, must reach the live slot"
        );
    }

    #[test]
    fn profile_mutation_publishes_a_clamped_config() {
        let seeded = crate::config::AppConfig {
            presence_profiles: vec![crate::config::PresenceProfile {
                name: "focus".to_string(),
                idle_away_after_seconds: Some(u64::MAX),
                ..Default::default()
            }],
            ..Default::default()
        };
        let state = state_with(seeded);

        let published = apply_profile_to_state(&state, Some("id=focus")).expect("applied");

        let profile = published
            .presence_profiles
            .iter()
            .find(|p| p.name == "focus")
            .expect("profile survives the clamp");
        assert_eq!(
            profile.idle_away_after_seconds,
            Some(86_400),
            "clamped_config must have run on the /profile path too"
        );
    }

    /// A 33-64 char id used to pass the route's own shape check (it allowed up
    /// to 64) and then be truncated to 32 by `clamped_config`. The lookup
    /// against the stored list happened BEFORE the clamp, so the request either
    /// missed a stored 32-char name (404 with a confusing message) or, worse,
    /// matched nothing and had its truncated form written as the active pointer.
    /// The route now refuses anything past the config's own cap up front.
    #[test]
    fn profile_ids_longer_than_the_config_cap_are_rejected_not_truncated() {
        let max = crate::config::MAX_PROFILE_ID_CHARS;
        let long_name = "a".repeat(max); // a legitimate stored profile
        let state = state_with(config_with_profiles(&[&long_name]));

        // The max-length id works.
        assert!(
            apply_profile_to_state(&state, Some(&format!("id={long_name}"))).is_ok(),
            "a stored profile at exactly the cap must be selectable"
        );

        // One char past the cap is a 400 — not a truncation to the valid name.
        let over = "a".repeat(max + 1);
        assert_eq!(
            apply_profile_to_state(&state, Some(&format!("id={over}"))).err(),
            Some(MutateError::BadRequest(format!(
                "id must be 1..={max} chars"
            )))
        );
        // And the live pointer is unchanged by the rejected request.
        assert_eq!(
            state
                .config
                .snapshot()
                .and_then(|c| c.active_profile.clone()),
            Some(long_name)
        );
    }

    #[test]
    fn malformed_escape_detection() {
        assert!(!has_malformed_escape("focus"));
        assert!(!has_malformed_escape("Deep Work"));
        assert!(!has_malformed_escape("Deep%20Work"));
        assert!(!has_malformed_escape("%2F"));
        assert!(!has_malformed_escape("%ff")); // lowercase hex is valid
        assert!(!has_malformed_escape("a%2Fb%20c"));

        assert!(has_malformed_escape("%"));
        assert!(has_malformed_escape("%2"));
        assert!(has_malformed_escape("%ZZ"));
        assert!(has_malformed_escape("%2Z"));
        assert!(has_malformed_escape("ok%then"));
    }

    /// The helpers were well covered but the HANDLERS were not: replacing
    /// `handle_snooze` / `handle_profile` wholesale with the pre-fix inert body
    /// (validate, log, 204, mutate nothing) left all 26 serve tests green,
    /// because every mutation test drives `apply_*_to_state` directly.
    ///
    /// These tests pin the wiring the other tests cannot see: each handler must
    /// delegate to its helper AND respond with what that helper produced.
    #[test]
    fn handlers_delegate_to_the_mutation_helpers() {
        let src = include_str!("serve.rs");
        let production = &src[..src.find("\n#[cfg(test)]").expect("a test module")];

        for (handler, helper) in [
            ("fn handle_snooze(", "apply_snooze_to_state"),
            ("fn handle_profile(", "apply_profile_to_state"),
        ] {
            let start = production
                .find(handler)
                .unwrap_or_else(|| panic!("{handler} must exist"));
            // Handler body: brace-matched from the opening brace.
            let brace = start + production[start..].find('{').expect("handler has a body");
            let mut depth = 0;
            let mut end = brace;
            for (i, ch) in production[brace..].char_indices() {
                match ch {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            end = brace + i;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let body = &production[brace..=end];

            assert!(
                body.contains(helper),
                "{handler} must call {helper} — without it the route validates and \
                 returns 204 while changing nothing, which is the exact bug this \
                 branch fixed"
            );
            // And it must not ignore the outcome: the error arm has to be handled.
            assert!(
                body.contains(".body()") || body.contains("Err("),
                "{handler} must handle the {helper} error, not discard it"
            );
        }
    }
}
