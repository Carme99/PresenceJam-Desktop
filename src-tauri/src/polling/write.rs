//! Teams write path: 401-retry helper, playing/paused/no-track writes, `process_track` and `handle_no_track` (split from `poll_once.rs`, issue #754).

use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use serde_json::json;
use tauri::{AppHandle, Emitter};

use crate::config::PresencePair;
use crate::profanity;
use crate::spotify::format_status_with_context;
use crate::teams::{
    clear_teams_status_message, emit_teams_write_error, get_teams_presence,
    is_token_expired as is_teams_token_expired, refresh_teams_token, set_teams_status_message,
    TeamsApiError, TeamsTokens, GATE_REASON_CALENDAR, GATE_REASON_QUIET_HOURS,
};
use crate::token_io;
use crate::AppState;

use super::{emit_error, ErrorEventEmitter, ErrorSeverity};

/// Issue #1117: the failure half of the shared Teams write helper's result.
///
/// `Superseded` is deliberately NOT a [`TeamsApiError`]. It means the token
/// slot moved under us while the refresh was in flight (a concurrent login,
/// reconnect or re-auth), so the write never happened and the refresh error
/// is about a token nobody holds any more. The first cut of issue #929
/// folded it into `Transient`, which made every caller treat it as a real
/// failure: a spurious Dashboard banner ("Microsoft Teams is temporarily
/// unavailable. Retrying shortly.") where `process_track` used to show
/// nothing, plus an availability re-arm on the stale token that main never
/// attempted. A typed variant keeps the no-op distinguishable without
/// touching `TeamsApiError` (which is shared with the Graph client).
#[derive(Debug, Clone)]
pub(crate) enum TeamsWriteError {
    /// A newer session owns the slot: this iteration's write is a no-op and
    /// must not be reported to the user as a failure.
    Superseded,
    /// A genuine write / refresh failure, already classified by the helper
    /// (marker emitted for auth, Forbidden logged, transient logged).
    Api(TeamsApiError),
}

/// Issue #1117: how `process_track`'s playing write reacts to a failure, and
/// what the ITERATION then does about it.
///
/// This is the whole arm decision, hoisted out of `process_track` so it is a
/// pure function whose RETURN VALUE is what `process_track` returns. That
/// matters because the superseded case has to end the iteration BEFORE the
/// presence tail re-arms on the stale token (main's pre-#929 `return 0;`),
/// while a genuine failure still logs, banners and honours a server-directed
/// backoff.
///
/// The short-circuit is [`PlayingIterationOutcome::EndIteration`]'s
/// `sleep_secs`, not a `return` statement pasted into the match arm: the first
/// cut of this fix kept the `return 0;` in the arm and only mapped the typed
/// error to an enum variant, which left the P0 unpinned — deleting the
/// `return 0;` restored the stale-token `sync_availability` re-arm plus a
/// backoff and every test stayed green.
#[derive(Debug)]
pub(crate) enum PlayingIterationOutcome {
    /// End the iteration now, returning exactly `sleep_secs`: no error
    /// banner, no backoff, and — crucially — no presence tail, because the
    /// token that just lost the slot must never be re-armed.
    EndIteration { sleep_secs: u64 },
    /// Fall through to the presence tail, carrying the typed error for the
    /// caller's banner and the backoff to raise `teams_backoff_secs` to.
    ContinueWithBackoff {
        backoff_secs: u64,
        error: TeamsApiError,
    },
}

/// Issue #1117: the pure form of `process_track`'s playing-write failure arm.
///
/// `prior_backoff_secs` is `process_track`'s `teams_backoff_secs` as it stands
/// when the write fails. A genuine failure raises it with the server-directed
/// `Retry-After` (issue #154); a superseded slot ignores it entirely and ends
/// the iteration with a zero sleep, because a newer session already owns
/// Teams and there is nothing to wait out or report.
pub(crate) fn playing_write_iteration_outcome(
    failure: TeamsWriteError,
    prior_backoff_secs: u64,
) -> PlayingIterationOutcome {
    match failure {
        // Issue #1117: the pre-#929 `return 0;`, moved here so the caller
        // returns a value this function computed. Deleting the short-circuit
        // (or giving it any non-zero sleep, or letting it honour
        // `prior_backoff_secs`) reintroduces the stale-token re-arm that #929
        // exists to prevent, and the behavioural test fails.
        TeamsWriteError::Superseded => PlayingIterationOutcome::EndIteration { sleep_secs: 0 },
        // Issue #154: a 429 extends the next poll to the server-directed
        // delay. The typed error is handed back unchanged so the caller
        // banners exactly what the shared helper classified.
        TeamsWriteError::Api(error) => PlayingIterationOutcome::ContinueWithBackoff {
            backoff_secs: prior_backoff_secs.max(super::timing::rate_limit_sleep_secs(&error)),
            error,
        },
    }
}

/// Issue #929: the shared 401 retry + classifier for the three Teams write
/// paths (the playing write in `process_track`, the no-track clear in
/// `handle_no_track`, and the paused-track clear in `process_track`).
///
/// `write_fn` is invoked with the supplied access token. On the initial
/// `TeamsApiError::ExpiredToken`, the helper performs one `refresh_teams_token`,
/// then a `cas_refresh_teams` commit, then `token_io::persist_tokens`, then
/// a single retry of `write_fn` using the refreshed token. On the final
/// typed error, the helper classifies: emits `teams-reconnect-required` for
/// auth failures (ExpiredToken, InvalidGrant, or ReauthRequired), logs
/// Forbidden as permission/license-only (re-auth cannot fix it), and logs
/// transient, rate-limited, or other errors as transient. The caller still
/// owns context-specific events (the playing arm's Dashboard banner via
/// `emit_teams_write_error`, the paused arm's `emit_error(Warning)` for
/// transient) and backoff. Returns the (possibly refreshed) tokens on
/// success so the playing arm can use them for the availability re-arm in
/// the same iteration.
///
/// One helper, three call sites: the playing write, no-track clear, and
/// paused clear can no longer drift in their 401-handling shape.
///
/// The body lives in [`teams_write_with_refresh_fn`], which additionally takes
/// injectable refresh + persist closures. [`teams_write_with_optional_refresh`]
/// is the single place the real closures are bound; this seam-bearing entry
/// point is what the test module drives, so no test reaches the Microsoft
/// token endpoint, the OS keychain or `tokens.json` (issue #929 rework,
/// issue #1117).
///
/// Generic over the Tauri runtime so the production call sites retain
/// `&AppHandle<Rt>` for the persist + emit seams (issue #929 rework).
pub(crate) fn teams_write_with_refresh_seams<F, R, P, Rt>(
    app: &AppHandle<Rt>,
    state: &Arc<AppState>,
    teams_tok: &TeamsTokens,
    write_fn: F,
    refresh_fn: R,
    persist: P,
    log_tag: &str,
) -> Result<TeamsTokens, TeamsWriteError>
where
    F: FnMut(&str) -> Result<(), TeamsApiError>,
    R: FnOnce() -> Result<TeamsTokens, TeamsApiError>,
    P: FnMut(&Arc<AppState>) -> Result<(), String>,
    Rt: tauri::Runtime,
{
    teams_write_with_refresh_fn(
        state, teams_tok, write_fn, refresh_fn, persist, app, log_tag,
    )
}

/// Production binding of the shared helper: the real
/// [`refresh_teams_token`] and [`token_io::persist_tokens`].
pub(crate) fn teams_write_with_optional_refresh<F, Rt>(
    app: &AppHandle<Rt>,
    state: &Arc<AppState>,
    teams_tok: &TeamsTokens,
    write_fn: F,
    log_tag: &str,
) -> Result<TeamsTokens, TeamsWriteError>
where
    F: FnMut(&str) -> Result<(), TeamsApiError>,
    Rt: tauri::Runtime,
{
    teams_write_with_refresh_seams(
        app,
        state,
        teams_tok,
        write_fn,
        || refresh_teams_token(teams_tok),
        |s| token_io::persist_tokens(s, app),
        log_tag,
    )
}

/// Issue #929: shared 401-retry + classification core. The production wrapper
/// [`teams_write_with_optional_refresh`] supplies [`refresh_teams_token`]; the
/// injectable `refresh_fn` lets the behavioural tests drive the retry path
/// with fakes (no network, no keychain, observable emit).
///
/// Behaviour contract (executed, not asserted by source text):
/// 1. Try `write_fn` with the current access token.
/// 2. On `Ok`, return the unchanged tokens.
/// 3. On a non-`ExpiredToken` error, classify it and return
///    `TeamsWriteError::Api`.
/// 4. On `ExpiredToken`, call `refresh_fn` exactly once.
///    * If the refresh fails as a dead credential, clear the slot, classify,
///      and return `Api(typed refresh error)` — unless the slot already moved,
///      in which case the write is a no-op and the caller gets `Superseded`
///      (issue #1117).
///    * If the refresh fails transiently, keep the slot and return the typed
///      error — again `Superseded` instead when the slot moved.
///    * On success, CAS-commit the new tokens through `AppState`. A lost
///      CAS keeps the original `ExpiredToken` so the classifier can emit
///      `teams-reconnect-required`. A won CAS persists and re-runs `write_fn`
///      with the NEW access token; a second failure is classified and
///      returned, a second success returns the new tokens so the caller can
///      use them for the availability re-arm in the same iteration.
pub(crate) fn teams_write_with_refresh_fn<F, R, E, P>(
    state: &Arc<AppState>,
    teams_tok: &TeamsTokens,
    mut write_fn: F,
    refresh_fn: R,
    mut persist: P,
    emitter: &E,
    log_tag: &str,
) -> Result<TeamsTokens, TeamsWriteError>
where
    F: FnMut(&str) -> Result<(), TeamsApiError>,
    R: FnOnce() -> Result<TeamsTokens, TeamsApiError>,
    E: ErrorEventEmitter,
    P: FnMut(&Arc<AppState>) -> Result<(), String>,
{
    let pre_refresh_access_token = teams_tok.access_token.clone();
    let first_err = match write_fn(&pre_refresh_access_token) {
        Ok(()) => return Ok(teams_tok.clone()),
        Err(e) => e,
    };
    let status = match first_err {
        TeamsApiError::ExpiredToken(s) => s,
        other => {
            classify_teams_write_failure(emitter, &other, log_tag);
            return Err(TeamsWriteError::Api(other));
        }
    };
    log::info!(
        "[POLLING] {}: Teams write hit ExpiredToken, attempting one refresh + retry",
        log_tag
    );
    let new_tokens = match refresh_fn() {
        Ok(t) => t,
        Err(refresh_err) => {
            log::error!(
                "[POLLING] {}: Teams reactive refresh failed: {}",
                log_tag,
                refresh_err
            );
            let failure = handle_teams_refresh_failure(
                state,
                &mut persist,
                refresh_err,
                &pre_refresh_access_token,
                log_tag,
            );
            // Issue #1117: a superseded slot is not a failure to report. The
            // newer session is alive and already owns Teams, so the helper
            // classifies nothing and the caller's `Superseded` arm stays
            // silent — emitting `teams-reconnect-required` or a Dashboard
            // banner here would send the user chasing a session that works.
            if let TeamsWriteError::Api(error) = &failure {
                classify_teams_write_failure(emitter, error, log_tag);
            }
            return Err(failure);
        }
    };
    let committed =
        match super::refresh::cas_refresh_teams(state, log_tag, &pre_refresh_access_token, || {
            Ok::<_, TeamsApiError>(new_tokens.clone())
        }) {
            super::refresh::CasOutcome::Committed(_) => true,
            super::refresh::CasOutcome::Discarded { .. } => false,
            super::refresh::CasOutcome::RefreshFailed { .. } => {
                unreachable!("inner refresh_fn is Ok-wrapping")
            }
        };
    if !committed {
        // CAS lost (mirrors the Spotify 401 path): keep the original
        // ExpiredToken so the classifier emits `teams-reconnect-required`.
        log::warn!(
            "[POLLING] {}: CAS lost, keeping original 401 for classification",
            log_tag
        );
        let err = TeamsApiError::ExpiredToken(status);
        classify_teams_write_failure(emitter, &err, log_tag);
        return Err(TeamsWriteError::Api(err));
    }
    // Issue #180: `cas_refresh_teams` owns the recovery-marker lock and
    // commits through `AppState`; this caller holds no token-slot write
    // guard. Persist only after it returns.
    if let Err(persist_err) = persist(state) {
        log::warn!(
            "[POLLING] {}: failed to persist reactively refreshed teams tokens: {}",
            log_tag,
            persist_err
        );
    }
    match write_fn(&new_tokens.access_token) {
        Ok(()) => Ok(new_tokens),
        Err(retry_err) => {
            log::error!(
                "[POLLING] {}: Teams retry after refresh also failed: {}",
                log_tag,
                retry_err
            );
            classify_teams_write_failure(emitter, &retry_err, log_tag);
            Err(TeamsWriteError::Api(retry_err))
        }
    }
}

/// Issue #929: typed classifier shared by the three Teams write paths.
/// Mirrors the inlined classification that previously lived in
/// `process_track` and `handle_no_track`: auth failures emit
/// `teams-reconnect-required`; Forbidden logs as permission/license
/// (re-auth cannot fix it); rate-limited / transient / other log as
/// transient.
pub(crate) fn classify_teams_write_failure<E: ErrorEventEmitter>(
    emitter: &E,
    e: &TeamsApiError,
    log_tag: &str,
) {
    match e {
        TeamsApiError::ExpiredToken(_)
        | TeamsApiError::InvalidGrant
        | TeamsApiError::ReauthRequired(_) => {
            log::warn!(
                "[POLLING] {}: Teams auth failure detected, emitting teams-reconnect-required",
                log_tag
            );
            emitter.emit_marker("teams-reconnect-required");
        }
        TeamsApiError::Forbidden(_, _) => {
            log::error!(
                "[POLLING] {}: Teams operation forbidden (permission/license) — re-auth cannot fix this; skipping teams-reconnect-required",
                log_tag
            );
        }
        TeamsApiError::RateLimited(_)
        | TeamsApiError::Transient(_)
        | TeamsApiError::Other(_, _) => {
            log::warn!(
                "[POLLING] {}: Teams operation failed (transient), continuing",
                log_tag
            );
        }
    }
}

/// Issue #929: surface transient / rate-limited / other failures from the
/// paused-track clear to the Dashboard at `ErrorSeverity::Warning`. The shared
/// helper (`teams_write_with_optional_refresh`) already classified the typed
/// error — auth failures emit `teams-reconnect-required`, Forbidden logs as
/// permission/license — so this only fires for the transient / rate-limited /
/// other branch the helper silently logs.
///
/// Behaviour held in a small generic helper so the test module can drive it
/// with a fake [`ErrorEventEmitter`] and assert the actual severity, not the
/// shape of the production match (issue #929 rework: behavioural, not source
/// text).
pub(crate) fn emit_paused_clear_failure<E: ErrorEventEmitter>(emitter: &E, e: &TeamsApiError) {
    match e {
        TeamsApiError::RateLimited(_)
        | TeamsApiError::Transient(_)
        | TeamsApiError::Other(_, _) => {
            emit_error(
                emitter,
                "teams",
                format!("Paused-track clear: {}", e.user_message()),
                ErrorSeverity::Warning,
            );
        }
        TeamsApiError::ExpiredToken(_)
        | TeamsApiError::InvalidGrant
        | TeamsApiError::ReauthRequired(_)
        | TeamsApiError::Forbidden(_, _) => {
            // The shared helper already emitted `teams-reconnect-required`
            // (auth) or logged Forbidden as permission/license.
        }
    }
}

/// Issue #929: shared refresh-failure handler for the three Teams write
/// paths. Issue #295 policy (only a dead credential clears the session;
/// a transient refresh failure keeps it). A transient refresh keeps the
/// session and returns the original refresh error, wrapped in
/// [`TeamsWriteError::Api`] for the caller to classify.
///
/// Issue #1117: a superseded slot returns [`TeamsWriteError::Superseded`], NOT
/// a `Transient`. The newer session is alive, so the caller must treat this
/// as a no-op instead of a failure to surface; folding it into `Transient`
/// is what made `process_track` banner the Dashboard and re-arm presence on
/// the stale token.
pub(crate) fn handle_teams_refresh_failure<P>(
    state: &Arc<AppState>,
    persist: &mut P,
    refresh_err: TeamsApiError,
    pre_refresh_access_token: &str,
    log_tag: &str,
) -> TeamsWriteError
where
    P: FnMut(&Arc<AppState>) -> Result<(), String>,
{
    // Issue #798: only clear when the slot still holds the token this
    // refresh ran from — a mid-flight replacement means the error is
    // about a superseded token and the newer session is alive.
    let refresh_superseded = state
        .tokens
        .teams()
        .as_ref()
        .map(|t| t.access_token.as_str())
        != Some(pre_refresh_access_token);
    if super::refresh::teams_refresh_requires_reauth(&refresh_err) {
        let cleared = state
            .tokens_load
            .clear_teams_if_current(&state.tokens, pre_refresh_access_token);
        if !cleared {
            log::debug!(
                "[POLLING] {}: superseded refresh error discarded: {}",
                log_tag,
                refresh_err
            );
            log::warn!(
                "[POLLING] {}: Teams clear superseded by a newer session; no-op",
                log_tag
            );
            return TeamsWriteError::Superseded;
        }
        log::warn!(
            "[POLLING] {}: Teams refresh token is dead, discarding tokens",
            log_tag
        );
        // Issue #180: the clear helper releases its write guard before
        // return. Persist in this later statement so the read lock cannot
        // overlap that guard.
        if let Err(persist_err) = persist(state) {
            log::warn!(
                "[POLLING] {}: failed to persist cleared teams tokens: {}",
                log_tag,
                persist_err
            );
        }
        return TeamsWriteError::Api(refresh_err);
    }
    if refresh_superseded {
        log::debug!(
            "[POLLING] {}: superseded refresh error discarded: {}",
            log_tag,
            refresh_err
        );
        log::warn!(
            "[POLLING] {}: slot replaced mid-refresh, keeping newer Teams session",
            log_tag
        );
        return TeamsWriteError::Superseded;
    }
    log::warn!(
        "[POLLING] {}: Teams reactive refresh failed (transient), keeping session",
        log_tag
    );
    TeamsWriteError::Api(refresh_err)
}

/// Issues #370/#388: the single source of truth for a write-ready Teams
/// token — clone the stored tokens, refresh when expired (CAS-commit +
/// persist, dead-credential re-auth policy per issue #295), and hand back
/// `None` when there is nothing usable. Called from BOTH `process_track`
/// and `handle_no_track` (including the clear path) so the no-track clear
/// can no longer sail with a dead token while the track path refreshes.
pub(crate) fn teams_token_for_write(app: &AppHandle, state: &Arc<AppState>) -> Option<TeamsTokens> {
    let teams_tokens = state.tokens.teams().clone();
    if let Some(ref tok) = teams_tokens {
        let expired = is_teams_token_expired(tok);
        if expired {
            log::info!("[POLLING] teams_token_for_write: Teams token expired, refreshing...");

            let pre_refresh_access_token = tok.access_token.clone();

            // The refresh error stays typed (`CasOutcome<T, E>` is generic
            // over `E`) so the re-auth policy below can classify it instead
            // of string-sniffing.
            let teams_refresh_outcome = super::refresh::cas_refresh_teams(
                state,
                "teams",
                &pre_refresh_access_token,
                || refresh_teams_token(tok),
            );
            match teams_refresh_outcome {
                super::refresh::CasOutcome::Committed(new_tokens) => {
                    // Issue #180: `cas_refresh_teams` owns the recovery-marker
                    // lock and commits through `AppState`; this caller holds no
                    // token-slot write guard. Persist only after the helper returns.
                    if let Err(e) = token_io::persist_tokens(state, app) {
                        log::warn!(
                            "[POLLING] teams_token_for_write: failed to persist refreshed teams tokens: {}",
                            e
                        );
                    }
                    Some(new_tokens)
                }
                super::refresh::CasOutcome::Discarded { current } => current,
                super::refresh::CasOutcome::RefreshFailed {
                    error: e,
                    replaced: true,
                } => {
                    // Issue #798: the failed refresh never matched the slot
                    // — a newer session was installed mid-flight, so it is
                    // alive; keep it and skip this iteration's Teams work.
                    log::warn!(
                        "[POLLING] teams_token_for_write: slot replaced mid-refresh, keeping newer session: {}",
                        e
                    );
                    state.tokens.teams().clone()
                }
                super::refresh::CasOutcome::RefreshFailed {
                    error: e,
                    replaced: false,
                } => {
                    log::error!(
                        "[POLLING] teams_token_for_write: Failed to refresh Teams token: {}",
                        e
                    );
                    // Issue #295: classify the typed error exactly like the
                    // Teams status-update path below and the Spotify sibling.
                    // Only a dead refresh token (`invalid_grant`) or a
                    // rejected access token (401) means re-auth; `Transient`
                    // (network/5xx), `RateLimited`, `Forbidden` and
                    // `Other(400, …)` keep the session and retry later — a
                    // single dropped connection must not send the user
                    // through a full device-code browser re-auth.
                    if super::refresh::teams_refresh_requires_reauth(&e) {
                        let cleared = state
                            .tokens_load
                            .clear_teams_if_current(&state.tokens, &pre_refresh_access_token);
                        if !cleared {
                            log::warn!(
                                "[POLLING] teams_token_for_write: Teams clear superseded by a newer session; no-op"
                            );
                            return None;
                        }
                        log::warn!("[POLLING] teams_token_for_write: Teams refresh token is dead, discarding tokens and requiring reconnect");
                        // Issue #180: the clear helper releases its write guard
                        // before return. Persist in this later statement so the
                        // read lock cannot overlap that guard.
                        if let Err(persist_err) = token_io::persist_tokens(state, app) {
                            log::warn!(
                                "[POLLING] teams_token_for_write: failed to persist cleared teams tokens: {}",
                                persist_err
                            );
                        }
                        let _ = app.emit("teams-reconnect-required", json!(null));
                        None
                    } else {
                        // Issue #295: a transient refresh failure keeps the
                        // session (the tokens stay in `state`, unlike the
                        // dead-token branch above) and skips this iteration's
                        // Teams work — attempting the status write with a
                        // token we just failed to refresh would only produce
                        // a 401 and force the very re-auth this policy exists
                        // to avoid. The next iteration retries the refresh.
                        log::warn!(
                            "[POLLING] teams_token_for_write: Teams refresh failed (transient), keeping session and retrying later"
                        );
                        None
                    }
                }
            }
        } else {
            teams_tokens
        }
    } else {
        teams_tokens
    }
}

/// Issue #364: the debounce predicate — a change inside the 500ms window
/// after the last Teams write skips this iteration's API call (the caller
/// returns `DEBOUNCE_RETRY_SECONDS` before any side effect, so the retry
/// re-detects the change and emits/posts exactly once).
pub(crate) fn debounce_active(changed: bool, last_teams_update: Option<Instant>) -> bool {
    if !changed {
        return false;
    }
    match last_teams_update {
        Some(last_update) => {
            (last_update.elapsed().as_millis() as u64) < super::timing::DEBOUNCE_MS
        }
        None => false,
    }
}

/// Issue #373: whether a no-track poll should attempt a Teams clear.
/// Fresh threads start with `last_track_key=None`, so the first no-track
/// poll must attempt one clear (pre-restart status would otherwise stay
/// stale); later nothing-tracked polls stay a no-op. Pure so the
/// exactly-once semantics are unit-testable; the caller consumes the flag.
pub(crate) fn first_no_track_attempts_clear(
    last_track_key: &Option<String>,
    first_iteration: bool,
) -> bool {
    last_track_key.is_some() || first_iteration
}
/// Issue #384: skip a byte-identical playing-status write while the last
/// write is still inside the keepalive window. A track/config-fingerprint
/// change (`changed`) always force-writes, as does a lapsed keepalive (so
/// the Graph expiry never lapses with no refresh in flight). Issue #873:
/// `force_resume_write` overrides the dedup exactly once — the first
/// iteration after the idle gate cleared must POST the status even when
/// the text is byte-identical to what Teams already shows, so the resume
/// surfaces to the user.
pub(crate) fn should_skip_identical_write(
    changed: bool,
    last_posted_status: Option<&str>,
    final_status: &str,
    last_write: Option<Instant>,
    now: Instant,
    force_resume_write: bool,
) -> bool {
    if changed || force_resume_write {
        return false;
    }
    if last_posted_status != Some(final_status) {
        return false;
    }
    match last_write {
        Some(t) => now.duration_since(t).as_secs() < super::timing::STATUS_KEEPALIVE_SECONDS,
        None => false,
    }
}

/// Findings D3/D4 (issues #686/#687): what to do with one placeholder write —
/// the paused-track clear and the no-track clear share it, so the two paths
/// cannot drift again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlaceholderWrite {
    /// POST the placeholder (and only then record it as posted).
    Post,
    /// A byte-identical placeholder is already on Teams: stay silent.
    SkipDuplicate,
    /// The presence/rule/manual gate suppressed this write. NOT recorded as
    /// posted, so a later iteration retries it once the gate clears; `announce`
    /// is false when this suppression episode was already surfaced, so the
    /// `presence-gated` event fires once per episode instead of once per poll.
    Suppress { announce: bool },
}

/// Findings D3/D4 (issues #686/#687): the single record/emit decision for a
/// placeholder write.
///
/// The pre-fix code compared `last_posted_placeholder` BEFORE asking the gate
/// whether the write was allowed, and the gated branch then recorded the
/// placeholder as POSTED although nothing was sent. A gated pause-clear or
/// no-track clear was therefore deduped away forever — the meeting could end
/// mid-track and the clear would still never be retried. Splitting "applied"
/// from "suppressed" is what makes the retry possible:
///
/// * `blocked`      ⇒ `Suppress`, regardless of what is currently on Teams (a
///   suppressed write must never look like a posted one);
/// * `already_posted` ⇒ `SkipDuplicate` (#155);
/// * otherwise      ⇒ `Post`.
pub(crate) fn placeholder_write_decision(
    blocked: bool,
    already_posted: bool,
    already_suppressed: bool,
) -> PlaceholderWrite {
    if blocked {
        PlaceholderWrite::Suppress {
            announce: !already_suppressed,
        }
    } else if already_posted {
        PlaceholderWrite::SkipDuplicate
    } else {
        PlaceholderWrite::Post
    }
}

/// The `gated_track_key` value for a suppression with NO track present
/// (findings D4/D11 follow-up). There is no status key to record, but the gate
/// is real — quiet hours or a match-all rule suppress the no-track clear — so
/// the state must stay representable; it must simply never be the finished
/// track's key. A real status key always carries `" | "` separators
/// (see [`status_track_key`]), so this sentinel cannot collide with one.
pub(crate) const NO_TRACK_GATE_KEY: &str = "no-track";

/// The `gated_track_key` transition the no-track path applies: the sentinel
/// while a suppression holds, `None` once nothing is suppressed. One helper so
/// the suppress arm and the two retiring arms cannot drift.
pub(crate) fn no_track_gate_key(blocked: bool) -> Option<&'static str> {
    blocked.then_some(NO_TRACK_GATE_KEY)
}

/// Review rounds 2 (item 7) and 3 (item 3): record the manual-status verdict
/// observed with a presence sample into the [`super::state::ExitSnapshot`], using
/// the SAME predicate the write gate uses.
///
/// The requirement is "no path may OBSERVE a manual status without recording
/// it", because the exit path has no Graph sample of its own and must not replace
/// a Teams status the user typed with our "Paused" placeholder. Every read
/// therefore funnels through here — the shared `gate_verdict` closure for the
/// change-time and paused-clear reads, and the due mid-track re-check (which
/// calls `presence_gate_decision` directly) for its own.
pub(crate) fn observe_presence_sample(
    session: &super::state::SessionState,
    respect_manual_status: bool,
    presence: &crate::teams::PresenceInfo,
    posted: Option<&str>,
    placeholder: Option<&str>,
) {
    super::state::record_manual_status_blocks(
        session,
        super::presence::manual_status_blocks_write(
            respect_manual_status,
            Some(presence),
            posted,
            placeholder,
            Utc::now(),
        ),
    );
}

/// Review round 2 (item 4): whether the paused clear can skip its Graph
/// `/presence` read entirely.
///
/// True exactly when the read cannot change the outcome: the placeholder Teams
/// already shows is the one this pause wants (#155 dedup would skip the POST
/// whatever the verdict says), no rule suppresses the clear (a rule verdict is
/// time/track based and needs no read, and it must still be recorded and
/// announced), and no recorded gate has reached its re-check (a due re-check is
/// what clears a stale gate, so that read must happen).
///
/// What finding D3 (issue #686) actually required was that a suppressed write
/// must not mark itself as POSTED — the ordering fix is what does that, and it
/// does not depend on the read happening first. This helper restores the
/// pre-fix no-read fast path without restoring the poisoning.
pub(crate) fn paused_clear_skips_gate_read(
    already_posted: bool,
    rule_suppresses: bool,
    recorded_gate_due: bool,
) -> bool {
    already_posted && !rule_suppresses && !recorded_gate_due
}

/// Finding D6 (issue #689): whether the observed item represents a playback
/// STATE change for an otherwise-unchanged track.
///
/// `status_track_key` deliberately excludes `is_playing` (it keys track
/// identity + status-shaping config), and the stored `TrackInfo` was only ever
/// written inside `if changed` — so pausing the same track in the Spotify
/// client left the process-wide store claiming it was still playing, and the
/// sync status / tray / Dashboard all reported the wrong playback state. An
/// absent stored track counts as a change (re-store rather than assume).
pub(crate) fn playback_state_changed(
    stored_is_playing: Option<bool>,
    observed_is_playing: bool,
) -> bool {
    stored_is_playing != Some(observed_is_playing)
}

/// The generated `TrackInfo` value carried by `spotify-track-changed`.
/// Device controls are useful to direct state consumers, but this event has
/// always exposed only the seven track and playback fields consumed by the
/// Dashboard. Strip the optional controls from the event copy rather than
/// widening its wire contract.
pub(crate) fn track_event_payload(track: &crate::spotify::TrackInfo) -> crate::spotify::TrackInfo {
    let mut event_track = track.clone();
    event_track.volume_percent = None;
    event_track.supports_volume = None;
    event_track.actions = None;
    event_track
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn process_track(
    app: &AppHandle,
    state: &Arc<AppState>,
    config: &Option<std::sync::Arc<crate::config::AppConfig>>,
    // The whole observed item — media plus episode metadata and playback
    // context (issues #580/#581). `now.media` is the frozen `TrackInfo`
    // every consumer below already speaks.
    now: &crate::spotify::NowPlaying,
    last_track_key: &mut Option<String>,
    last_poll_instant: Instant,
    last_teams_update: &mut Option<Instant>,
    last_posted_placeholder: &mut Option<String>,
    suppressed_placeholder: &mut Option<String>,
    consecutive_pauses: &mut u8,
    gated_track_key: &mut Option<String>,
    last_availability_arm: &mut Option<Instant>,
    armed_presence: &mut Option<PresencePair>,
    last_posted_status: &mut Option<String>,
    last_gate_check: &mut Option<Instant>,
    // Issue #873: see `WriteClocks`. The mid-track re-check watches the
    // `true`→`false` transition of the idle verdict and arms
    // `force_resume_write`; the write path consumes it once and clears.
    last_idle_verdict: &mut Option<bool>,
    force_resume_write: &mut bool,
) -> u64 {
    let track = &now.media;
    let elapsed_ms = last_poll_instant.elapsed().as_millis() as u64;
    // Issue #165: `progress_ms` is `None` for live/unknown-position streams.
    // Keep the Option alive so the duration-derived sleep/expiry below are
    // skipped for streams — they fall back to the default interval and no
    // `expiryDateTime` on the wire respectively.
    let corrected_progress_ms = track.progress_ms.map(|p| p.saturating_add(elapsed_ms));
    // Issue #154: a throttled Teams set/clear (429) extends the next poll to
    // the server-directed delay.
    let mut teams_backoff_secs: u64 = 0;

    // Issue #343: the change key carries the status-shaping config
    // (filter flag + placeholder + format) alongside the item identity, so a
    // relevant config flip mid-track forces one rewrite on the next poll
    // instead of leaving the stale status until the next track.
    let track_key = super::status_text::status_track_key(now, config);
    let changed = last_track_key.as_ref() != Some(&track_key);
    // Finding D6 (issue #689): `status_track_key` excludes `is_playing`, so a
    // pause/resume of the SAME track is not a `changed` iteration — and the
    // stored `TrackInfo` was only written inside `if changed`, leaving every
    // consumer (tray, sync status, Dashboard) reporting the stale playback
    // state. Detect it separately and re-store below.
    let stored_is_playing = state.polling.current_track().as_ref().map(|t| t.is_playing);
    let playing_changed = !changed && playback_state_changed(stored_is_playing, track.is_playing);

    // Issue #867: the calendar-boundary lookup the gate re-check consults
    // when deciding whether the re-arm cadence alone is enough. Computed
    // ONCE here so every `gate_recheck_due` call inside this iteration
    // sees the same boundary — refreshing mid-iteration would let the
    // cache race a meeting end and leave the gate stuck suppressed.
    //
    // The list_upcoming call below drives the actual fetch; the
    // 5-minute TTL plus the boundary-driven refresh (see calendar.rs)
    // mean a steady iteration fires the network call once every
    // `CALENDAR_CACHE_TTL` OR at a meeting start/end, whichever lands
    // first. The closure captures the cached HTTP client so we don't
    // rebuild a TLS stack on every fetch.
    let now_wall = chrono::Utc::now();
    if let Some(teams_token) = state.tokens.teams().as_ref() {
        if !crate::teams::is_token_expired(teams_token) {
            state
                .calendar
                .set_access_token(teams_token.access_token.clone());
            match crate::teams::build_teams_client() {
                Ok(client) => {
                    let _ = state.calendar.list_upcoming(
                        now_wall,
                        chrono::Duration::hours(4),
                        |token, start, end| {
                            crate::calendar::fetch_calendar_view(&client, token, start, end)
                        },
                    );
                }
                Err(e) => {
                    log::debug!(
                        "[CALENDAR] process_track: HTTP client build failed, calendar pre-gate is a no-op: {}",
                        e
                    );
                }
            }
        }
    }
    let next_meeting_boundary = state.calendar.next_boundary(now_wall);

    // Issue #432 / finding PollCore#1 (issue #569) / PollCore#2 (issue #570) /
    // finding #634 (issue #634): the per-iteration rule decision is computed
    // ONCE, here, so EVERY write path can consult it — the playing write, the
    // mid-track gate re-check, the paused clear and the no-track clear. It
    // carries the rule's suppression, its replacement text AND its presence
    // action. Pure computation: no side effects, so it is safe to run ahead of
    // the debounce early-return below.
    let quiet_active = super::gate::quiet_hours_active_now(config);
    let rule = super::rules::rule_gate(config, &track.artist, &track.title);
    // Issue #867: the calendar pre-gate. Active when a busy Outlook event
    // covers `now` (including the `pre_meeting_suppress_minutes` window the
    // user configures in Settings). A `pre_meeting_suppress_minutes` of 0
    // collapses the pre-window to nothing, so a tenant that never consents
    // to `Calendars.ReadBasic` (and stays at 0) reproduces today's
    // behaviour exactly.
    let pre_meeting_suppress_minutes = config
        .as_ref()
        .map(|c| c.teams.pre_meeting_suppress_minutes)
        .unwrap_or(0);
    let calendar_busy = state
        .calendar
        .busy_at(chrono::Utc::now(), pre_meeting_suppress_minutes);
    // The combined suppression predicate the write path consults. The
    // calendar reason flows through the same `gated_track_key` /
    // `last_gate_check` plumbing the rule gate already uses, so a meeting
    // end lands within one poll of `gate_recheck_due` firing at the
    // boundary — see the comment on `gate_recheck_due` above.
    let calendar_suppresses = calendar_busy;
    // Finding #637: a rule with its own presence action overrides the
    // out-of-office default (a track rule cannot override it for the
    // no-track path, where `rule_gate` is fed empty strings).
    let gate_out_of_office = super::gate::ooo_gate_enabled(config, rule.presence.is_some());
    // Findings #3.0-P2/#635/#637: the presence gate and the manual-status check
    // read the SAME sample. `presence_gate` owns the Graph read, so the manual
    // check only ever forces an extra read when the user turned the gate off.
    let presence_gate_enabled = config
        .as_ref()
        .map(|c| c.teams.presence_gate)
        .unwrap_or(true);
    let respect_manual_status = config
        .as_ref()
        .map(|c| c.teams.respect_manual_status)
        .unwrap_or(true);
    // Issue #872: the OS-level presentation gate is opt-in via
    // `teams.gate_when_presenting`. OFF by default, so the 4.7 behaviour
    // is preserved exactly for users who never touch the toggle. The
    // probe runs inside the gate closure, so a `None` config keeps the
    // feature entirely off (no extra shell calls on Linux/macOS).
    let gate_when_presenting = config
        .as_ref()
        .map(|c| c.teams.gate_when_presenting)
        .unwrap_or(false);
    // Issue #873: the idle threshold. `0` disables the gate entirely; the
    // non-zero values are clamped to 60..=3600 by `clamp_teams`, so a
    // hand-edited config cannot put the gate in a state that surprises
    // the user. The probe is consulted once per iteration (hoisted below)
    // so the change-time gate and the mid-track re-check stamp the same
    // reading into `last_idle_verdict`.
    let idle_threshold_secs: u64 = config
        .as_ref()
        .map(|c| c.teams.idle_away_after_seconds)
        .unwrap_or(0);
    let presence_read_needed = presence_gate_enabled || respect_manual_status;

    // Issues #872/#873: the OS probes are called once per iteration
    // (hoisted out of the closure) so the change-time gate and the
    // mid-track re-check stamp the same reading into `last_idle_verdict`
    // and the resume-after-idle transition is detected exactly once.
    let presentation_state = crate::platform::focus::probe_focus();
    let idle_threshold_crossed = if idle_threshold_secs > 0 {
        crate::platform::idle::seconds_since_last_input()
            .is_some_and(|secs| secs.0 >= idle_threshold_secs)
    } else {
        false
    };

    // The gate verdict for one sample — a local closure so the read sites
    // below (track change, mid-track re-check, pause) cannot drift. The two
    // "what we posted" texts are PARAMETERS rather than captures: the write
    // path below mutates them, and a capturing closure would hold a borrow of
    // them for the whole function.
    let gate_verdict = |presence: &crate::teams::PresenceInfo,
                        posted: Option<&str>,
                        placeholder: Option<&str>|
     -> Option<String> {
        observe_presence_sample(
            &state.session,
            respect_manual_status,
            presence,
            posted,
            placeholder,
        );
        super::presence::presence_gate_decision(
            presence,
            presence_gate_enabled,
            gate_out_of_office,
            respect_manual_status,
            posted,
            placeholder,
            Utc::now(),
            presentation_state,
            gate_when_presenting,
            idle_threshold_crossed,
        )
    };

    // Issue #364: debounce BEFORE any side effect. A change inside the
    // window parks on the short fixed retry with every tracked field
    // untouched, so the retry re-detects the change and emits/posts
    // exactly once. (Pre-fix the store/emit/placeholder-clear/gate work
    // below ran first and only the track key was restored, duplicating
    // the `spotify-track-changed` event and the Graph presence read.)
    if debounce_active(changed, *last_teams_update) {
        log::debug!(
            "[POLLING] process_track: debounce active, skipping Teams API call (changed={}, elapsed={}ms)",
            changed,
            last_teams_update.map(|i| i.elapsed().as_millis() as u64).unwrap_or(0)
        );
        return super::timing::DEBOUNCE_RETRY_SECONDS;
    }

    if changed {
        log::info!("[POLLING] process_track: new track detected, updating");
        // Clone: `track_key` is still needed below for the presence-gate
        // comparison (issue #3.0-P2).
        *last_track_key = Some(track_key.clone());
        *state.polling.current_track_mut() = Some(track.clone());
        // Issue #877: mirror the `(title, artist)` pair onto the static
        // the gate emitter reads. Updated on every track change so a
        // subsequent gate entry anchors to the track it targeted, not to
        // the one before it.
        super::state::record_current_track_fingerprint(
            &state.session,
            Some(crate::history::TrackFingerprint {
                title: track.title.clone(),
                artist: track.artist.clone(),
            }),
        );
        // Issue #581: the config-flip rewrite path (#343) has no body to
        // re-parse, so the full item — episode metadata and playback context
        // included — is kept alongside the stored `TrackInfo`, which carries
        // neither. Written on the same condition as `current_track`, so the
        // two never disagree about what is playing.
        state.session.store_now_playing(Some(now.clone()));

        let _ = app.emit("spotify-track-changed", track_event_payload(track));
    } else if playing_changed {
        // Finding D6 (issue #689): a pause is a state change. Re-store the
        // observed item (so `current_track` — and therefore the returned sync
        // status — reports the paused track) and tell the shell about it. The
        // full item is re-stored, not just the flag, because `LAST_NOW_PLAYING`
        // and `current_track` are documented as lockstep (issues #580/#581).
        log::info!(
            "[POLLING] process_track: playback state changed for the same track (is_playing={}), re-storing",
            track.is_playing
        );
        *state.polling.current_track_mut() = Some(track.clone());
        // Issue #877: same-track pause / resume also re-seeds the
        // fingerprint mirror so the gate emitter anchors to the track it
        // targeted even when the gate fires mid-pause.
        super::state::record_current_track_fingerprint(
            &state.session,
            Some(crate::history::TrackFingerprint {
                title: track.title.clone(),
                artist: track.artist.clone(),
            }),
        );
        state.session.store_now_playing(Some(now.clone()));
        let _ = app.emit(
            "playback-state-changed",
            super::presence::playback_state_changed_payload(track.is_playing, &track_key),
        );
    }

    // Issues #370/#388: one shared refresh path — see `teams_token_for_write`.
    let teams_tokens = teams_token_for_write(app, state);

    if let Some(mut teams_tok) = teams_tokens {
        // Findings #634/#635/#637: whether the presence read suppressed this
        // iteration's status work. The presence gate is the OUTER AUTHORITY for
        // the rule engine too, so a rule's `setPresence` action is skipped when
        // the gate — or a status message the user owns — already said no.
        let mut presence_blocked = false;
        if track.is_playing {
            *consecutive_pauses = 0;
            // Issue #155: a real track replaces any placeholder, so the next
            // pause/no-track must post a fresh placeholder again.
            *last_posted_placeholder = None;
            // Findings D3/D4: a real track also retires any SUPPRESSED
            // placeholder record — a fresh pause decides from scratch.
            *suppressed_placeholder = None;

            // P2 (issue #3.0-P2): presence-aware gating. On a track change,
            // read the user's Teams presence; when busy/DND/in a
            // meeting/call/presenting, suppress the status write for the
            // whole track (recorded in `gated_track_key`) and emit
            // `presence-gated`. Runs after the debounce above, so a change
            // inside the window parks untouched and the retry performs the
            // single gate read. Fail-safe: a failed read
            // (network, 403, …) proceeds with the write, logged as a warning.
            // Issue #432: rule-based gating, evaluated alongside the
            // presence gate on every track change. Quiet hours suppress
            // unconditionally (time-based — no presence read needed); a
            // matching track rule with an empty replacement suppresses like
            // the presence gate. Both record into `gated_track_key` so the
            // #380 re-check path below re-evaluates them mid-track
            // (quiet-hours expiry clears like a cleared presence gate) and
            // the write below stays the single late-post path — no
            // duplicate/spam writes beyond #384 dedup. A rule carrying a
            // non-empty `replacement_status` never gates: its text becomes
            // `final_status` below, still flowing through the #384
            // identical-write suppression.
            //
            // Everything comes from the hoisted decision above (`rule`, which
            // now covers quiet hours too): one evaluation per iteration, shared
            // with the paused and the no-track write paths.
            let rule_replacement: Option<String> = rule.replacement.clone();
            if changed {
                if calendar_suppresses {
                    log::info!(
                        "[POLLING] process_track: calendar busy, suppressing status write (pre_meeting_suppress_minutes={})",
                        pre_meeting_suppress_minutes
                    );
                    *gated_track_key = Some(track_key.clone());
                    *last_gate_check = Some(Instant::now());
                    super::presence::emit_presence_gated(
                        &state.session,
                        app,
                        GATE_REASON_CALENDAR,
                        "",
                        "",
                    );
                } else if rule.suppresses() {
                    let reason = rule.reason.unwrap_or(GATE_REASON_QUIET_HOURS);
                    log::info!(
                        "[POLLING] process_track: {} active, skipping status write",
                        reason
                    );
                    *gated_track_key = Some(track_key.clone());
                    *last_gate_check = Some(Instant::now());
                    super::presence::emit_presence_gated(&state.session, app, reason, "", "");
                } else if presence_read_needed {
                    match get_teams_presence(&teams_tok.access_token) {
                        Ok(presence) => match gate_verdict(
                            &presence,
                            last_posted_status.as_deref(),
                            last_posted_placeholder.as_deref(),
                        ) {
                            Some(reason) => {
                                log::info!(
                                    "[POLLING] process_track: gated ({}), skipping status write",
                                    reason
                                );
                                *gated_track_key = Some(track_key.clone());
                                *last_gate_check = Some(Instant::now());
                                super::presence::emit_presence_gated(
                                    &state.session,
                                    app,
                                    &reason,
                                    &presence.availability,
                                    &presence.activity,
                                );
                            }
                            None => {
                                *gated_track_key = None;
                                // Issue #873: a track change mid-resume
                                // also triggers a forced write — the
                                // `track_key` already moves `changed` to
                                // true here, but the #384 dedup still
                                // applies if the new track happens to
                                // template to the same string as the
                                // previous one (rare but legal).
                                if *last_idle_verdict == Some(true) && !idle_threshold_crossed {
                                    *force_resume_write = true;
                                }
                            }
                        },
                        Err(e) => {
                            log::warn!(
                                "[POLLING] process_track: presence gate read failed, proceeding with status write: {}",
                                e
                            );
                            *gated_track_key = None;
                        }
                    }
                    // Issue #873: stamp the verdict on every change-time
                    // gate so a track change records the same state the
                    // mid-track re-check would have. `idle_threshold_crossed`
                    // was already computed once at the top of the loop.
                    *last_idle_verdict = Some(idle_threshold_crossed);
                } else {
                    *gated_track_key = None;
                }
            }
            // Finding PollCore#1 (issue #569): quiet-hour ENTRY must be
            // evaluated mid-track. Pre-fix the entry check lived inside
            // `if changed`, and the #380 re-check block below only ever ran
            // for an already-gated track, so a quiet window that opened while
            // a track played stayed unfelt until the next track change — the
            // #384 keepalive kept (re-)POSTing the status for the track's
            // whole remaining duration. The outcome mirrors the changed-path
            // gate exactly: record the gate (so the exit path below clears it
            // when the window closes), emit `presence-gated`, and return
            // before any write. Only the playing branch is handled here — a
            // paused track's clear is the paused branch's job (finding
            // PollCore#2, issue #570).
            if super::gate::quiet_gate_entry_due(
                quiet_active,
                gated_track_key.as_deref(),
                &track_key,
            ) {
                *gated_track_key = Some(track_key.clone());
                *last_gate_check = Some(Instant::now());
                let remaining_ms =
                    corrected_progress_ms.map(|c| track.duration_ms.saturating_sub(c));
                if rule.suppresses() {
                    log::info!(
                        "[POLLING] process_track: quiet hours started mid-track, suppressing status write"
                    );
                    super::presence::emit_presence_gated(
                        &state.session,
                        app,
                        rule.reason.unwrap_or(GATE_REASON_QUIET_HOURS),
                        "",
                        "",
                    );
                    // Finding #634: the suppression skips the write, but the
                    // rule's presence action still applies.
                    teams_backoff_secs =
                        teams_backoff_secs.max(super::presence::rule_presence_backoff(
                            &state.session,
                            app,
                            &teams_tok.access_token,
                            config,
                            &rule,
                            remaining_ms,
                            armed_presence,
                            last_availability_arm,
                        ));
                    return super::timing::playing_track_sleep(remaining_ms, config)
                        .max(teams_backoff_secs);
                }
                // Finding #634: a quiet-hours window with a replacement text is
                // NOT a suppression — fall through to the write below, which
                // posts the rule's text.
                log::info!(
                    "[POLLING] process_track: quiet hours started mid-track, posting the rule status"
                );
            }
            // Issue #380: a gated track stays gated only until the gate
            // re-check is due — then presence is re-read, and a cleared
            // gate (meeting ended mid-track) falls through to the normal
            // write below instead of suppressing the whole duration.
            // Issue #430 (same late-post, named explicitly): a track
            // suppressed by the presence gate gets its status posted
            // automatically — same poll cycle or next — once the gate
            // clears, without requiring a track change. The cleared branch
            // below (`*gated_track_key = None` + fall-through) IS the #430
            // path: the write further down runs the normal #384 dedup, so
            // no duplicate/spam writes, and a still-gated track posts
            // nothing (early return at the tail of this block).
            // Fail-safe: a failed read keeps the gate (still suppressed).
            // `last_gate_check` throttles the re-reads while gated — never
            // `last_teams_update`, which times the debounce + keepalive write clocks.
            // Issue #432 / finding #634: rule gates re-evaluate here too. The
            // clock is re-projected (a long-lived track can span a quiet-hours
            // boundary) and the rule re-matched — including its replacement
            // text and presence action — so a rule that stopped suppressing
            // falls into the presence re-check below.
            if gated_track_key.as_deref() == Some(track_key.as_str()) {
                let (cur_minutes, cur_weekday) = super::gate::local_minutes_and_weekday();
                // Issue #868: feed the album / show / device /
                // playlist-uri / duration the live path already has into
                // the rule walker so a mid-track re-check honours the
                // new conditions.
                let show_name = now
                    .episode
                    .as_ref()
                    .map(|e| e.show_name.as_str())
                    .unwrap_or("");
                let track_ctx = super::rules::TrackRuleContext {
                    artist: &track.artist,
                    title: &track.title,
                    album: &track.album,
                    show: show_name,
                    device: &now.context.device,
                    playlist_uri: &now.context.playlist,
                    duration_ms: track.duration_ms,
                };
                let current_rule = super::rules::rule_gate_at_with_ctx(
                    config,
                    cur_minutes,
                    cur_weekday,
                    &track_ctx,
                );
                let remaining_ms =
                    corrected_progress_ms.map(|c| track.duration_ms.saturating_sub(c));
                // Issue #867: calendar pre-gate re-evaluates here too. The
                // gate_recheck_due call below returns true at the next
                // meeting boundary, so the calendar busy predicate clears
                // within one poll of the meeting end and the write below
                // posts the late status (the same shape #430 already uses
                // for a presence-gate meeting ending mid-track).
                let current_calendar_busy = state
                    .calendar
                    .busy_at(chrono::Utc::now(), pre_meeting_suppress_minutes);
                if current_calendar_busy {
                    if super::gate::gate_recheck_due(
                        *last_gate_check,
                        Instant::now(),
                        chrono::Utc::now(),
                        next_meeting_boundary,
                    ) {
                        *last_gate_check = Some(Instant::now());
                    }
                    log::debug!(
                        "[POLLING] process_track: still calendar-gated, keeping suppression"
                    );
                    return super::timing::playing_track_sleep(remaining_ms, config)
                        .max(teams_backoff_secs);
                }
                if current_rule.suppresses() {
                    if super::gate::gate_recheck_due(
                        *last_gate_check,
                        Instant::now(),
                        chrono::Utc::now(),
                        next_meeting_boundary,
                    ) {
                        *last_gate_check = Some(Instant::now());
                    }
                    log::debug!("[POLLING] process_track: still rule-gated, keeping suppression");
                    teams_backoff_secs =
                        teams_backoff_secs.max(super::presence::rule_presence_backoff(
                            &state.session,
                            app,
                            &teams_tok.access_token,
                            config,
                            &current_rule,
                            remaining_ms,
                            armed_presence,
                            last_availability_arm,
                        ));
                    return super::timing::playing_track_sleep(remaining_ms, config)
                        .max(teams_backoff_secs);
                }
                if !presence_read_needed {
                    *gated_track_key = None;
                } else if super::gate::gate_recheck_due(
                    *last_gate_check,
                    Instant::now(),
                    chrono::Utc::now(),
                    next_meeting_boundary,
                ) {
                    match get_teams_presence(&teams_tok.access_token) {
                        Ok(presence) => {
                            // Review round 3 (item 3): this read does NOT go
                            // through `gate_verdict`, so it must record the
                            // manual-status verdict itself — otherwise a *gated*
                            // track plus a user-typed status left the exit
                            // snapshot stale and quitting replaced the user's
                            // own Teams message with our placeholder.
                            observe_presence_sample(
                                &state.session,
                                respect_manual_status,
                                &presence,
                                last_posted_status.as_deref(),
                                last_posted_placeholder.as_deref(),
                            );
                            // The same verdict as the change-time gate, so a
                            // manual status that lapses mid-track clears the
                            // gate and late-posts exactly like a meeting ending.
                            // Issue #872/#873: the OS probes are HOISTED
                            // at the top of `process_track` so the
                            // change-time gate and the mid-track re-check
                            // stamp the same reading into
                            // `last_idle_verdict`.
                            match super::presence::presence_gate_decision(
                                &presence,
                                presence_gate_enabled,
                                gate_out_of_office,
                                respect_manual_status,
                                last_posted_status.as_deref(),
                                last_posted_placeholder.as_deref(),
                                Utc::now(),
                                presentation_state,
                                gate_when_presenting,
                                idle_threshold_crossed,
                            ) {
                                Some(_) => {
                                    log::debug!("[POLLING] process_track: still presence-gated, keeping suppression");
                                    *last_gate_check = Some(Instant::now());
                                }
                                None => {
                                    log::info!(
                                        "[POLLING] process_track: presence gate cleared mid-track, posting late"
                                    );
                                    *gated_track_key = None;
                                    // Issue #380: record the re-check on the gate
                                    // clock only — `last_teams_update` (debounce +
                                    // keepalive) stays untouched so the late post
                                    // below is never mistaken for a fresh write.
                                    *last_gate_check = Some(Instant::now());
                                    // Issue #873: the first iteration after
                                    // the idle verdict flipped from
                                    // "threshold crossed" to "active" must
                                    // force exactly one write — the
                                    // byte-identical dedup would otherwise
                                    // skip a resume the user cannot see on
                                    // Teams. The flag is consumed by the
                                    // write path below and cleared once.
                                    if *last_idle_verdict == Some(true) && !idle_threshold_crossed {
                                        *force_resume_write = true;
                                        log::info!(
                                            "[POLLING] process_track: idle gate cleared, forcing resume write"
                                        );
                                    }
                                }
                            }
                            // Issue #873: track the verdict for the next
                            // mid-track re-check's transition detection.
                            // Stamped here (every re-check) so a poll
                            // iteration without a re-read (e.g. a 304) does
                            // not silently pin the verdict.
                            *last_idle_verdict = Some(idle_threshold_crossed);
                        }
                        Err(e) => {
                            log::warn!(
                                "[POLLING] process_track: gate re-read failed, keeping suppression: {}",
                                e
                            );
                            *last_gate_check = Some(Instant::now());
                        }
                    }
                }
                if gated_track_key.as_deref() == Some(track_key.as_str()) {
                    log::debug!(
                        "[POLLING] process_track: track presence-gated, skipping status write"
                    );
                    return super::timing::playing_track_sleep(remaining_ms, config);
                }
            }
            // Issue #792: the UNGATED mid-track re-check. The change-time gate
            // above ran once inside `if changed`, and the #380 block above only
            // re-reads for a recorded gate — so a meeting joined, DND enabled,
            // or hand-typed Teams status mid-track kept getting the music status
            // re-POSTed by the #384 keepalive for the rest of the track. Re-read
            // on the same `last_gate_check` clock the gated path uses (debounce +
            // keepalive windows untouched): on `Some(reason)` record the gate,
            // emit `presence-gated`, and return before the keepalive POSTs; on
            // `None` clear the gate and fall through to the write below.
            // `!changed` skips change iterations — the change-time gate just read
            // presence for those. Fail-safe: a failed read proceeds with the
            // write. The read routes through `gate_verdict`, so the manual-status
            // verdict is recorded for the exit path like every other read.
            if !changed
                && gated_track_key.as_deref() != Some(track_key.as_str())
                && presence_read_needed
                && super::gate::gate_recheck_due(
                    *last_gate_check,
                    Instant::now(),
                    chrono::Utc::now(),
                    next_meeting_boundary,
                )
            {
                match get_teams_presence(&teams_tok.access_token) {
                    Ok(presence) => {
                        match gate_verdict(
                            &presence,
                            last_posted_status.as_deref(),
                            last_posted_placeholder.as_deref(),
                        ) {
                            Some(reason) => {
                                log::info!(
                                    "[POLLING] process_track: gate engaged mid-track ({}), suppressing status write",
                                    reason
                                );
                                *gated_track_key = Some(track_key.clone());
                                *last_gate_check = Some(Instant::now());
                                super::presence::emit_presence_gated(
                                    &state.session,
                                    app,
                                    &reason,
                                    &presence.availability,
                                    &presence.activity,
                                );
                                let remaining_ms = corrected_progress_ms
                                    .map(|c| track.duration_ms.saturating_sub(c));
                                return super::timing::playing_track_sleep(remaining_ms, config)
                                    .max(teams_backoff_secs);
                            }
                            None => {
                                *gated_track_key = None;
                                *last_gate_check = Some(Instant::now());
                            }
                        }
                        // Issue #873: stamp the verdict like the change-time gate
                        // so the next re-check detects the resume transition.
                        *last_idle_verdict = Some(idle_threshold_crossed);
                    }
                    Err(e) => {
                        log::warn!(
                            "[POLLING] process_track: mid-track gate read failed, proceeding with status write: {}",
                            e
                        );
                        *last_gate_check = Some(Instant::now());
                    }
                }
            }

            // Issue #581: an episode uses its own template. A user's music
            // template ("🎵 {artist} - {track} 🎧") must not be applied
            // verbatim to a 90-minute podcast — the episode tokens
            // ({show}/{episode}/{publisher}) and the 🎙️ glyph exist for that
            // case. The built-in default IS the documented default of the
            // `teams.episode_status_format` config key this slice needs from
            // `config.rs` (see the report note): until that key exists there
            // is nothing per-user to read here.
            let status_format = if now.episode.is_some() {
                crate::spotify::DEFAULT_EPISODE_STATUS_FORMAT
            } else {
                config
                    .as_ref()
                    .map(|c| c.teams.status_format.as_str())
                    .unwrap_or("\u{1F3B5} {artist} - {track} \u{1F3A7}")
            };
            // Issues #580/#581: the same formatter the Settings preview uses,
            // now fed the playback context and episode metadata parsed from
            // this poll body.
            let status_message = format_status_with_context(
                track,
                now.episode.as_ref(),
                &now.context,
                status_format,
            );
            let profanity_filter_enabled = config
                .as_ref()
                .map(|c| c.teams.profanity_filter)
                .unwrap_or(true);
            let placeholder = config
                .as_ref()
                .map(|c| c.teams.profanity_placeholder.as_str())
                .unwrap_or(profanity::safe_placeholder_default());
            // Issue #432: a matching rule's non-empty `replacement_status`
            // becomes the posted text (the "busy/focus" alternative to
            // suppression). It still flows through the #384 identical-write
            // suppression below — a byte-identical replacement inside the
            // keepalive window skips the write exactly like normal text.
            let final_status = if let Some(replacement) = rule_replacement.as_deref() {
                replacement.to_string()
            } else if profanity_filter_enabled {
                // Issue #538: the user's own lexicon takes part in the filter.
                let extra_words: &[String] = config
                    .as_ref()
                    .map(|c| c.teams.profanity_extra_words.as_slice())
                    .unwrap_or(&[]);
                profanity::filter_status_for_locale(
                    &status_message,
                    placeholder,
                    track.is_playing,
                    extra_words,
                    super::status_text::config_locale(config),
                )
            } else {
                status_message.clone()
            };
            // Issue #384: byte-identical re-POSTs every cycle are pure
            // noise. Skip the write when the status is unchanged and the
            // last write is still inside the keepalive window. A
            // fingerprint change forces `changed` above, so it always
            // force-writes; a lapsed keepalive force-writes so the Graph
            // expiry never lapses.
            if should_skip_identical_write(
                changed,
                last_posted_status.as_deref(),
                &final_status,
                *last_teams_update,
                Instant::now(),
                *force_resume_write,
            ) {
                log::debug!("[POLLING] process_track: status identical and keepalive fresh, skipping Teams write");
                // Issue #790: the skipped POST must not skip the presence
                // session's own clock. An `Available` session fades after 5
                // minutes whatever this app POSTs, and the keepalive that
                // eventually does fire lands at or past that boundary — so the
                // re-arm winds through the shared tail here, on the 4-minute
                // cadence, exactly as it would after a real write.
                teams_backoff_secs = teams_backoff_secs.max(super::presence::sync_availability(
                    &state.session,
                    app,
                    &teams_tok.access_token,
                    track.is_playing,
                    corrected_progress_ms.map(|c| track.duration_ms.saturating_sub(c)),
                    &rule,
                    config,
                    presence_blocked,
                    armed_presence,
                    last_availability_arm,
                ));
                let remaining_ms =
                    corrected_progress_ms.map(|c| track.duration_ms.saturating_sub(c));
                return super::timing::playing_track_sleep(remaining_ms, config)
                    .max(teams_backoff_secs);
            }

            let remaining_ms = corrected_progress_ms.map(|c| track.duration_ms.saturating_sub(c));
            // Issue #165: live streams have no known remaining time → no
            // `expiryDateTime` on the wire (the status does not self-expire).
            let expiry_str = super::status_text::status_expiry_str(remaining_ms, config);

            // Issues #367/#428 (never-re-auth): a 401 here can mean the
            // token expired mid-sequence (or clock skew) even though the
            // pre-write expiry check passed. Issue #929 folds the reactive
            // refresh + CAS-commit + persist + single retry + typed
            // classification into `teams_write_with_optional_refresh` so the
            // playing write, no-track clear, and paused clear share one
            // 401-handling shape.
            let write_outcome = teams_write_with_optional_refresh(
                app,
                state,
                &teams_tok,
                |access_token| {
                    set_teams_status_message(access_token, &final_status, expiry_str.as_deref())
                },
                "process_track",
            );
            match write_outcome {
                Ok(refreshed) => {
                    *last_teams_update = Some(Instant::now());
                    *last_posted_status = Some(final_status.clone());
                    // Finding D1 (issue #684): the playing status is now on
                    // Teams — mirror it in the exit snapshot, which survives the
                    // loop's exit-tail clock reset (the D1 defect).
                    super::state::record_posted_status(&state.session, Some(&final_status));
                    let _ = app.emit(
                        "presence-updated",
                        crate::events::PresenceUpdated {
                            status: final_status.clone(),
                            timestamp: Utc::now().to_rfc3339(),
                        },
                    );
                    // Adopt the fresh token so the availability
                    // re-arm below does not 401 on the stale one.
                    teams_tok = refreshed;
                    // Issue #877: append to the bounded decision history.
                    // The track fingerprint pins the entry to the same
                    // (title, artist) pair the status write key uses, so a
                    // follow-up rewrite on the same track collapses cleanly
                    // when the Dashboard renders the Activity card.
                    crate::history::append(
                        crate::history::PresenceHistoryEntry {
                            at: Utc::now(),
                            kind: "presence-updated".to_string(),
                            note: if rule.reason.is_some() {
                                "posted (rule)".to_string()
                            } else {
                                "posted".to_string()
                            },
                            track_fingerprint: Some(crate::history::TrackFingerprint {
                                title: track.title.clone(),
                                artist: track.artist.clone(),
                            }),
                            posted_status: Some(final_status.clone()),
                            gate_reason: rule.reason.map(|s| s.to_string()),
                        },
                        config.as_ref(),
                    );
                }
                // Issue #1117 (option 2 of the issue's two options): the whole
                // arm decision — including the sleep this iteration returns —
                // is computed by `playing_write_iteration_outcome`, so the
                // short-circuit is a return VALUE the unit tests assert on
                // rather than a `return 0;` statement nothing could pin. No
                // write happened, `teams_tok` is stale, and the newer session
                // owns Teams — so there is nothing to banner, no error to back
                // off for, and the presence tail below must not re-arm on a
                // token that just 401'd. The next iteration picks the new
                // session up through `teams_token_for_write`.
                Err(failure) => {
                    match playing_write_iteration_outcome(failure, teams_backoff_secs) {
                        PlayingIterationOutcome::EndIteration { sleep_secs } => return sleep_secs,
                        PlayingIterationOutcome::ContinueWithBackoff {
                            backoff_secs,
                            error,
                        } => {
                            log::error!(
                                "[POLLING] process_track: Failed to set Teams status: {}",
                                error
                            );
                            // Issue #974: the Dashboard banner reads
                            // `user_message()`, never `Display`. `Display` carries
                            // the raw Graph body for 403/418 — useful in logs,
                            // useless to a user staring at a five-second banner.
                            emit_teams_write_error(app, &error);
                            // Issue #154: a 429 extends the next poll to the
                            // server-directed delay.
                            teams_backoff_secs = backoff_secs;
                            // The shared helper already classified the
                            // typed error (issue #929): dead credential →
                            // `teams-reconnect-required`; Forbidden logged
                            // as permission/license; transient logged.
                        }
                    }
                }
            }
        } else if config
            .as_ref()
            .map(|c| c.teams.clear_on_pause)
            .unwrap_or(true)
        {
            // Issue #155: the clear path posts a short-lived placeholder
            // (Graph has no "clear status message" action) and skips
            // byte-identical repeat posts.
            let placeholder_text = super::status_text::paused_status_placeholder(config);
            let placeholder = placeholder_text.as_str();
            // P2 (issue #3.0-P2): gate the paused-clear the same way as
            // the playing write — don't replace a busy/meeting presence
            // with a "Paused" placeholder. `gated_track_key` carries the
            // change-time decision from the playing path; re-read
            // presence only when this track wasn't gated there.
            //
            // Finding PollCore#2 (issue #570): the clear IS a status
            // write, so quiet hours and suppression rules apply to it too
            // (en.ts 'rules.sectionHint'). Pre-fix only the presence gate
            // was consulted here, so a quiet window or a matching
            // suppression rule was bypassed on every pause.
            // Finding #635: the gate covers the manual-status verdict too —
            // never replace a message the user typed with "Paused".
            let rule_suppression_reason: Option<&str> =
                if rule.suppresses() { rule.reason } else { None };
            // Findings D3 (issue #686): ask the GATE first and compare against
            // what Teams shows SECOND. Pre-fix the byte-identity check ran
            // before the verdict, and the gated branch recorded the placeholder
            // as POSTED although nothing was sent — so once `gate_recheck_due`
            // let the gate clear, the dedup skipped the clear forever and the
            // meeting-ending mid-pause never reached Teams.
            //
            // A gate recorded by the playing path is therefore re-decided as
            // soon as its re-check is due (the #380 clock, exactly like the
            // playing branch), instead of suppressing the pause indefinitely.
            //
            // Review round 2 (item 4): the ORDER of the verdict and the
            // byte-identity comparison does not by itself justify a Graph read
            // per paused poll — the pre-fix code short-circuited here, and a
            // steady pause must not pay for a `/presence` GET it cannot act on.
            // The read is skipped exactly where its result cannot matter:
            let already_posted = last_posted_placeholder.as_deref() == Some(placeholder);
            let recorded_gate_due = gated_track_key.as_deref() == Some(track_key.as_str())
                && super::gate::gate_recheck_due(
                    *last_gate_check,
                    Instant::now(),
                    chrono::Utc::now(),
                    next_meeting_boundary,
                );
            let mut gate_blocked = false;
            let mut gate_reason: Option<String> = None;
            let mut gate_sample: Option<(String, String)> = None;
            if paused_clear_skips_gate_read(
                already_posted,
                rule_suppression_reason.is_some(),
                recorded_gate_due,
            ) {
                // Every outcome of a fresh read is the same no-write
                // `SkipDuplicate` (#155) here, so nothing is pending — which also
                // means a suppression marker left over from an earlier episode
                // would only mute a future announced suppression.
                log::debug!(
                    "[POLLING] process_track: paused placeholder unchanged, skipping gate read and clear POST"
                );
                *suppressed_placeholder = None;
            } else if let Some(reason) = rule_suppression_reason {
                log::info!(
                    "[POLLING] process_track: paused-clear suppressed ({}), keeping presence untouched",
                    reason
                );
                gate_blocked = true;
                gate_reason = Some(reason.to_string());
            } else if gated_track_key.as_deref() == Some(track_key.as_str())
                && !super::gate::gate_recheck_due(
                    *last_gate_check,
                    Instant::now(),
                    chrono::Utc::now(),
                    next_meeting_boundary,
                )
            {
                // Inside the re-check window: keep the recorded verdict (it was
                // surfaced when it was taken, so nothing to emit).
                gate_blocked = true;
            } else if presence_read_needed {
                match get_teams_presence(&teams_tok.access_token) {
                    Ok(presence) => match gate_verdict(
                        &presence,
                        last_posted_status.as_deref(),
                        last_posted_placeholder.as_deref(),
                    ) {
                        Some(reason) => {
                            *gated_track_key = Some(track_key.clone());
                            *last_gate_check = Some(Instant::now());
                            presence_blocked = true;
                            gate_blocked = true;
                            gate_reason = Some(reason);
                            gate_sample =
                                Some((presence.availability.clone(), presence.activity.clone()));
                        }
                        None => {
                            // The gate cleared: fall through to the write
                            // decision below, which posts the placeholder the
                            // suppressed iteration never sent (finding D3).
                            *gated_track_key = None;
                            *last_gate_check = Some(Instant::now());
                        }
                    },
                    Err(e) => {
                        // Fail-safe: proceed with the clear.
                        log::warn!(
                            "[POLLING] process_track: presence gate read failed, proceeding with paused clear: {}",
                            e
                        );
                    }
                }
            }

            let already_suppressed = suppressed_placeholder.as_deref() == Some(placeholder);
            match placeholder_write_decision(gate_blocked, already_posted, already_suppressed) {
                PlaceholderWrite::Suppress { announce } => {
                    log::info!(
                        "[POLLING] process_track: paused-clear gated, keeping presence untouched (retried when the gate clears)"
                    );
                    // Findings D3/D4: record the suppression, never a post. The
                    // marker is what lets a later iteration (re-check due, quiet
                    // window over, rule stopped matching) POST the placeholder
                    // instead of deduping it away.
                    *suppressed_placeholder = Some(placeholder.to_string());
                    if announce {
                        if let Some(reason) = gate_reason.as_deref() {
                            let (availability, activity) = gate_sample.unwrap_or_default();
                            super::presence::emit_presence_gated(
                                &state.session,
                                app,
                                reason,
                                &availability,
                                &activity,
                            );
                        }
                    }
                }
                PlaceholderWrite::SkipDuplicate => {
                    log::debug!(
                        "[POLLING] process_track: paused placeholder unchanged, skipping clear POST"
                    );
                }
                PlaceholderWrite::Post => {
                    *suppressed_placeholder = None;
                    let expiry_str = super::timing::placeholder_expiry_str();
                    // Issue #929: route the paused clear through the same
                    // shared 401-retry helper the playing write and no-track
                    // clear use. A 401 the local expiry check did not
                    // predict (server-side revocation, clock skew) used to
                    // fail silently here; it now retries once and surfaces
                    // `teams-reconnect-required` for auth failures exactly
                    // like the playing arm.
                    match teams_write_with_optional_refresh(
                        app,
                        state,
                        &teams_tok,
                        |access_token| {
                            clear_teams_status_message(access_token, placeholder, Some(&expiry_str))
                        },
                        "process_track: paused clear",
                    ) {
                        Ok(_) => {
                            *last_teams_update = Some(Instant::now());
                            *last_posted_placeholder = Some(placeholder.to_string());
                            // Issue #384: Teams now shows a placeholder, so
                            // the recorded playing status is stale.
                            *last_posted_status = None;
                            // Finding D1 (issue #684): mirror it in the exit
                            // snapshot, which the loop's exit-tail clock reset
                            // cannot erase.
                            super::state::record_posted_status(&state.session, None);
                            // Finding D7 (issue #690): a PAUSE is not a stop.
                            // The dedicated event lets the Dashboard keep the
                            // track card and show the paused state;
                            // `presence-cleared` stays reserved for the genuine
                            // no-track path.
                            let _ = app.emit(
                                "presence-paused",
                                super::presence::presence_paused_payload(placeholder),
                            );
                        }
                        Err(failure) => match failure {
                            // Issue #1117: a newer session won the slot, so the
                            // clear is a no-op — no error log, no Dashboard
                            // banner, no backoff bump. Unlike the playing arm
                            // this arm does NOT return early: pre-#929 it had
                            // no reactive refresh at all and always fell
                            // through to the presence tail, so short-circuiting
                            // here would change the pause-backoff cadence for a
                            // condition main never saw. The next iteration
                            // re-reads the slot through `teams_token_for_write`.
                            TeamsWriteError::Superseded => {
                                log::debug!(
                                    "[POLLING] process_track: paused clear superseded by a newer session; no-op"
                                );
                            }
                            TeamsWriteError::Api(e) => {
                                log::error!(
                                    "[POLLING] process_track: Failed to clear Teams status: {}",
                                    e
                                );
                                // Issue #154: honor the server's Retry-After on a
                                // throttled clear.
                                teams_backoff_secs = teams_backoff_secs
                                    .max(super::timing::rate_limit_sleep_secs(&e));
                                // Issue #929: surface transient / rate-limited /
                                // other failures to the Dashboard so the user
                                // sees a Warning instead of a silent stuck
                                // status. The shared helper already classified
                                // the typed error (auth → `teams-reconnect-required`,
                                // Forbidden → permission/license logged), so the
                                // remaining branches only fire on transient /
                                // rate-limited / other. Behaviour is held in
                                // [`emit_paused_clear_failure`] for testability.
                                emit_paused_clear_failure(app, &e);
                            }
                        },
                    }
                }
            }
        }

        // P1 (issue #3.0-P1) + finding #634 (issue #634) + issue #790:
        // presence-session maintenance for this iteration. It lives in one
        // helper shared with the identical-write early return above and the
        // 304-with-track steady state, so the arm/clear cadence cannot drift
        // between the iterations that POST a status and the ones that do not
        // (an `Available` session fades after 5 minutes regardless of
        // `expirationDuration`). Emits `presence-availability-updated` on each
        // arm/clear.
        teams_backoff_secs = teams_backoff_secs.max(super::presence::sync_availability(
            &state.session,
            app,
            &teams_tok.access_token,
            track.is_playing,
            corrected_progress_ms.map(|c| track.duration_ms.saturating_sub(c)),
            &rule,
            config,
            presence_blocked,
            armed_presence,
            last_availability_arm,
        ));
    }

    if track.is_playing {
        let remaining_ms = corrected_progress_ms.map(|c| track.duration_ms.saturating_sub(c));
        super::timing::playing_track_sleep(remaining_ms, config)
    } else {
        let sleep = super::timing::pause_backoff(
            *consecutive_pauses,
            super::timing::config_default_interval(config),
            super::timing::config_pause_backoff_max(config),
        );
        *consecutive_pauses = consecutive_pauses.saturating_add(1).min(4);
        sleep
    }
    .max(teams_backoff_secs)
}
/// Handle a no-track poll result. Clears the tracked state and, when
/// `clear_on_pause` allows it (issue #155), posts a short-lived "Nothing
/// playing" placeholder. Returns extra backoff seconds to fold into the
/// next poll when the clear was throttled (issue #154).
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_no_track(
    app: &AppHandle,
    state: &Arc<AppState>,
    last_track_key: &mut Option<String>,
    config: &Option<std::sync::Arc<crate::config::AppConfig>>,
    last_posted_placeholder: &mut Option<String>,
    suppressed_placeholder: &mut Option<String>,
    gated_track_key: &mut Option<String>,
    last_availability_arm: &mut Option<Instant>,
    armed_presence: &mut Option<PresencePair>,
    first_iteration: &mut bool,
    last_posted_status: &mut Option<String>,
) -> u64 {
    // Findings D4/D11 follow-up: `gated_track_key` must describe the CURRENT
    // suppression, so the no-track path owns it too — the track's key must not
    // survive the track (a stale key makes the Dashboard chip claim "you're
    // busy, in a call, or presenting" over a "Nothing playing" card forever),
    // while a suppression with no track present (quiet hours / a match-all
    // rule) IS a real gated state and has to stay representable.
    // `no_track_gate_key` is the transition, in one place.
    // Issue #373: consume the fresh-thread flag exactly once (see
    // `first_no_track_attempts_clear`). A fresh thread starts with
    // `last_track_key=None`, so the first no-track poll falls through
    // and attempts one clear instead of leaving pre-restart status
    // stale; later nothing-tracked polls stay a no-op.
    let is_first = *first_iteration;
    *first_iteration = false;
    if first_no_track_attempts_clear(last_track_key, is_first) {
        *last_track_key = None;
        *state.polling.current_track_mut() = None;
        // Issue #877: clear the fingerprint mirror alongside the live
        // track — a gate that fires on a "no track" decision must not
        // anchor to the track the poller just stopped observing.
        super::state::record_current_track_fingerprint(&state.session, None);
        // Cleared in lockstep with `current_track` — the #343 rewrite path
        // reads this cache and must not resurrect a cleared track.
        state.session.store_now_playing(None);
    } else {
        return 0;
    }

    // Issues #370/#388: refresh before the clear, exactly like the track
    // path — one shared helper, no cloned-without-expiry token.
    let teams_tok = match teams_token_for_write(app, state) {
        Some(t) => t,
        None => {
            // Review round 2, item 3: this early return cannot post a clear, so
            // the finished track's gate must not survive it either — leaving it
            // makes `get_sync_status` answer `presence_gated = true` over a
            // "Nothing playing" card forever.
            *gated_track_key = no_track_gate_key(false).map(str::to_string);
            return 0;
        }
    };

    // P1 (issue #3.0-P1): availability sync owns the presence bubble while
    // nothing is playing, independently of `clear_on_pause` (that toggle
    // governs the placeholder status message only). The rule decision is
    // evaluated HERE — hoisted above the presence block (it used to live
    // below, feeding only the status-write verdict) — because the presence
    // half needs it too. The decision is pure (config + clock, no Graph I/O),
    // so hoisting changes nothing for the verdict below, which keeps
    // consuming the same value. With nothing playing there is no
    // artist/title, so only quiet hours and match-all rules can match.
    //
    // Issue #795: a time-based quiet-hours window carrying a presence pair is
    // still in force while nothing plays, so its pair is armed through
    // `rule_presence_backoff` (mirroring the playing-path quiet entry).
    // The no-pair case keeps the `clearPresence` clear (404 = session already
    // gone = success), so a track rule's stale pair still retires when its
    // track ends — finding #634's "the window/track no longer applies"
    // rationale holds for track rules, not for quiet hours. The pair arm also
    // updates `armed_presence`, so the next track re-arms instead of
    // believing a session is live.
    let no_track_rule = super::rules::rule_gate(config, "", "");
    let mut teams_backoff_secs: u64 = 0;
    if super::presence::availability_sync_enabled(config) {
        if no_track_rule.presence.is_some() {
            teams_backoff_secs = teams_backoff_secs.max(super::presence::rule_presence_backoff(
                &state.session,
                app,
                &teams_tok.access_token,
                config,
                &no_track_rule,
                None,
                armed_presence,
                last_availability_arm,
            ));
        } else {
            teams_backoff_secs = teams_backoff_secs.max(super::presence::clear_presence_session(
                &state.session,
                app,
                &teams_tok.access_token,
                "Availability cleared",
                armed_presence,
                last_availability_arm,
            ));
        }
    }

    // Issue #155: honor `clear_on_pause` like the paused-track branch.
    if !config
        .as_ref()
        .map(|c| c.teams.clear_on_pause)
        .unwrap_or(true)
    {
        // Review round 2, item 3: the user's config — not a gate — is what
        // suppresses this clear, so no gate is in force; retire the finished
        // track's key rather than leave a stale one on the clock.
        *gated_track_key = no_track_gate_key(false).map(str::to_string);
        return teams_backoff_secs;
    }

    // Finding PollCore#2 (issue #570): the no-track clear is a status write
    // too, so quiet hours and suppression rules govern it — the documented
    // contract is that rules suppress the Teams status write, not just the
    // playing branch of it (en.ts 'rules.sectionHint'). With nothing playing
    // there is no artist/title to match a scoped rule against, so only quiet
    // hours and match-all rules (both substrings empty) can suppress here.
    //
    // Finding #634 (issue #634): the decision now also carries the rule's
    // replacement text, so a quiet-hours entry saying "🌙 Back at 09:00" posts
    // that instead of going silent. (`no_track_rule` was evaluated above for
    // the presence half and is reused here — the decision is pure, so the
    // verdict is unchanged.)
    let placeholder = no_track_rule
        .replacement
        .clone()
        .unwrap_or_else(|| super::status_text::stopped_status_placeholder(config));
    let suppression_reason: Option<&str> = if no_track_rule.suppresses() {
        no_track_rule.reason
    } else {
        None
    };
    // Issue #791: the no-track clear is a status write too, so the presence
    // gate and the manual-status verdict govern it — pre-fix playback stopping
    // replaced a status message the user typed with our placeholder. Mirrors
    // the paused-clear read: a failed read fails open and the write proceeds.
    // A rule verdict needs no read, so it short-circuits first (paused path).
    let mut presence_reason: Option<String> = None;
    let mut presence_sample: Option<(String, String)> = None;
    if suppression_reason.is_none() {
        let presence_gate_enabled = config
            .as_ref()
            .map(|c| c.teams.presence_gate)
            .unwrap_or(true);
        let respect_manual_status = config
            .as_ref()
            .map(|c| c.teams.respect_manual_status)
            .unwrap_or(true);
        if presence_gate_enabled || respect_manual_status {
            match get_teams_presence(&teams_tok.access_token) {
                Ok(presence) => {
                    observe_presence_sample(
                        &state.session,
                        respect_manual_status,
                        &presence,
                        last_posted_status.as_deref(),
                        last_posted_placeholder.as_deref(),
                    );
                    let idle_threshold_secs: u64 = config
                        .as_ref()
                        .map(|c| c.teams.idle_away_after_seconds)
                        .unwrap_or(0);
                    let idle_threshold_crossed = if idle_threshold_secs > 0 {
                        crate::platform::idle::seconds_since_last_input()
                            .is_some_and(|secs| secs.0 >= idle_threshold_secs)
                    } else {
                        false
                    };
                    if let Some(reason) = super::presence::presence_gate_decision(
                        &presence,
                        presence_gate_enabled,
                        super::gate::ooo_gate_enabled(config, no_track_rule.presence.is_some()),
                        respect_manual_status,
                        last_posted_status.as_deref(),
                        last_posted_placeholder.as_deref(),
                        Utc::now(),
                        crate::platform::focus::probe_focus(),
                        config
                            .as_ref()
                            .map(|c| c.teams.gate_when_presenting)
                            .unwrap_or(false),
                        idle_threshold_crossed,
                    ) {
                        presence_sample =
                            Some((presence.availability.clone(), presence.activity.clone()));
                        presence_reason = Some(reason);
                    }
                }
                Err(e) => {
                    log::warn!(
                        "[POLLING] handle_no_track: presence gate read failed, proceeding with no-track clear: {}",
                        e
                    );
                }
            }
        }
    }
    // The combined verdict: a rule suppression or a presence/manual-status gate
    // both block the clear. Either one records a suppression (never a post) so
    // the decision can flip back, and the event fires once per episode.
    let gate_blocked = suppression_reason.is_some() || presence_reason.is_some();
    // Finding D4 (issue #687): the same class of defect as the paused clear.
    // Pre-fix the byte-identity check above ran BEFORE the suppression verdict,
    // and the suppressed branch recorded the placeholder as POSTED although
    // nothing was sent — so when the quiet window closed (or a match-all rule
    // stopped matching) the dedup skipped the clear and Teams kept showing the
    // stale playing status until the next track. Ask the verdict first, then
    // compare, and record a SUPPRESSION (not a post) when it blocks.
    let already_posted = last_posted_placeholder.as_deref() == Some(placeholder.as_str());
    let already_suppressed = suppressed_placeholder.as_deref() == Some(placeholder.as_str());
    match placeholder_write_decision(gate_blocked, already_posted, already_suppressed) {
        PlaceholderWrite::Suppress { announce } => {
            let reason = suppression_reason
                .or(presence_reason.as_deref())
                .unwrap_or(GATE_REASON_QUIET_HOURS);
            log::info!(
                "[POLLING] handle_no_track: clear suppressed ({}), keeping Teams status untouched (retried once the decision changes)",
                reason
            );
            // Findings D4 (issue #687): recorded as SUPPRESSED so the decision
            // can flip back — the next iteration where the rule no longer
            // suppresses falls into the POST arm below instead of being
            // deduped. The marker also keeps the event to one per suppression
            // episode.
            *suppressed_placeholder = Some(placeholder.clone());
            // Findings D4/D11 follow-up: a suppressed clear with nothing
            // playing IS a real gate (quiet hours / a match-all rule suppress
            // the write), so `presence_gated` stays true — but the finished
            // track's key must not survive it, or the Dashboard would keep
            // naming a track that has ended. Record the no-track sentinel.
            *gated_track_key = no_track_gate_key(true).map(str::to_string);
            if announce {
                let (availability, activity) = presence_sample.unwrap_or_default();
                super::presence::emit_presence_gated(
                    &state.session,
                    app,
                    reason,
                    &availability,
                    &activity,
                );
            }
            return teams_backoff_secs;
        }
        PlaceholderWrite::SkipDuplicate => {
            log::debug!(
                "[POLLING] handle_no_track: no-track placeholder unchanged, skipping clear POST"
            );
            // A suppressing verdict would have won above, so nothing is gated
            // now: retire whatever key the finished track left behind.
            *gated_track_key = no_track_gate_key(false).map(str::to_string);
            return teams_backoff_secs;
        }
        PlaceholderWrite::Post => {
            *suppressed_placeholder = None;
        }
    }

    let expiry_str = super::timing::placeholder_expiry_str();
    // Issue #455-residual: mirror the process_track ExpiredToken
    // refresh+single-retry. Issue #929 folds both paths (and the paused
    // clear in `process_track`) into `teams_write_with_optional_refresh`,
    // so a 401 here now shares one 401-handling shape with its siblings.
    let clear_outcome = teams_write_with_optional_refresh(
        app,
        state,
        &teams_tok,
        |access_token| clear_teams_status_message(access_token, &placeholder, Some(&expiry_str)),
        "handle_no_track",
    );
    match clear_outcome {
        Ok(_) => {
            *last_posted_placeholder = Some(placeholder.to_string());
            // Issue #384: Teams now shows a placeholder, so the recorded
            // playing status is stale.
            *last_posted_status = None;
            // Finding D1 (issue #684): Teams now shows a placeholder, not a
            // playing status — mirror it in the exit snapshot.
            super::state::record_posted_status(&state.session, None);
            // Findings D4/D11 follow-up: the clear was posted, so no write is
            // being suppressed any more — retire the finished track's gate key
            // (a stale one made `get_sync_status` answer `presence_gated = true`
            // forever after a gated track ended).
            *gated_track_key = no_track_gate_key(false).map(str::to_string);
            let _ = app.emit(
                "presence-cleared",
                crate::events::PresenceCleared {
                    timestamp: Utc::now().to_rfc3339(),
                },
            );
            teams_backoff_secs
        }
        Err(failure) => match failure {
            // Issue #1117: the newer session already owns Teams and no clear
            // was posted, so this is a no-op — no error log and, crucially, no
            // `presence-cleared` emit (the Dashboard must not be told the
            // status was cleared when it was not) and no backoff. The next
            // iteration re-reads the slot through `teams_token_for_write`.
            TeamsWriteError::Superseded => {
                log::debug!(
                    "[POLLING] handle_no_track: clear superseded by a newer session; no-op"
                );
                teams_backoff_secs
            }
            TeamsWriteError::Api(e) => {
                log::error!(
                    "[POLLING] handle_no_track: Failed to clear Teams status: {}",
                    e
                );
                // Issue #154: honor the server's Retry-After on a throttled clear.
                let backoff = teams_backoff_secs.max(super::timing::rate_limit_sleep_secs(&e));
                // The shared helper already classified the typed error
                // (issue #929): dead credential → `teams-reconnect-required`;
                // Forbidden logged as permission/license; transient logged.
                backoff
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::polling::rules::matching_track_rule_at_with_ctx;
    use crate::polling::status_text::status_track_key;
    use crate::polling::ErrorEventPayload;
    use crate::teams::GATE_REASON_MANUAL_STATUS;

    /// Records every event the production emitters push, with no GUI runtime
    /// and without linking Tauri's `test` module — which breaks the Windows
    /// test binary's load (issue #929 rework). Module-scoped so both the
    /// reconnect-marker and the severity tests can drive it.
    struct CapturingEmitter {
        events: std::sync::Mutex<Vec<(String, ErrorEventPayload)>>,
        markers: std::sync::Mutex<Vec<String>>,
    }
    impl ErrorEventEmitter for CapturingEmitter {
        fn emit_error_event(&self, event: &str, payload: ErrorEventPayload) {
            self.events
                .lock()
                .expect("capturing-emitter mutex must not be poisoned")
                .push((event.to_string(), payload));
        }
        fn emit_marker(&self, event: &str) {
            self.markers
                .lock()
                .expect("capturing-emitter mutex must not be poisoned")
                .push(event.to_string());
        }
    }
    impl CapturingEmitter {
        fn new() -> Self {
            Self {
                events: std::sync::Mutex::new(Vec::new()),
                markers: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    /// Production source with the test module stripped — the shared preamble
    /// for the structural guards below.
    fn prod_source() -> &'static str {
        include_str!("write.rs")
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("write.rs has no #[cfg(test)] mod tests block")
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

    #[test]
    fn track_changed_event_serializes_exactly_the_seven_field_contract() {
        let track = crate::spotify::TrackInfo {
            title: "A Track".to_string(),
            artist: "An Artist".to_string(),
            album: "An Album".to_string(),
            album_art_url: "https://example.com/album-art.jpg".to_string(),
            is_playing: true,
            progress_ms: Some(42_000),
            duration_ms: 240_000,
            volume_percent: Some(80),
            supports_volume: Some(true),
            actions: Some(crate::spotify::DeviceActions {
                seeking: true,
                ..Default::default()
            }),
        };

        assert_eq!(
            serde_json::to_value(track_event_payload(&track)).unwrap(),
            json!({
                "title": "A Track",
                "artist": "An Artist",
                "album": "An Album",
                "album_art_url": "https://example.com/album-art.jpg",
                "is_playing": true,
                "progress_ms": 42_000,
                "duration_ms": 240_000
            })
        );

        let process_track = prod_fn_body(prod_source(), "pub(crate) fn process_track(");
        assert!(
            process_track
                .contains(r#"app.emit("spotify-track-changed", track_event_payload(track))"#),
            "spotify-track-changed must emit the serialized seven-field event value"
        );
    }

    /// Issue #3.0-P1/P2 regression guard: inside `process_track`, the
    /// presence-gate read (`get_teams_presence`) must precede the status
    /// write (`set_teams_status_message`) so a busy/meeting presence can
    /// suppress it, and the availability call sites (set_teams_presence
    /// re-arm + clear_teams_presence on pause) must exist.
    #[test]
    fn test_presence_gate_precedes_status_write_and_availability_call_sites_exist() {
        let source = include_str!("write.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("write.rs has no #[cfg(test)] mod tests block");

        // Isolate the process_track body by brace counting from its opening
        // `{` (house style — never boundary anchors, which drift). The
        // json!({...}) braces and `\u{...}` escapes inside string literals
        // are balanced, so they do not perturb the count.
        let after_sig = prod_source
            .split("pub(crate) fn process_track(")
            .nth(1)
            .expect("process_track definition not found");
        let open = after_sig
            .find('{')
            .expect("process_track has no opening brace");
        let mut depth = 0usize;
        let mut end = None;
        for (i, ch) in after_sig[open..].char_indices() {
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
        let body = &after_sig[..end.expect("process_track body never closed")];

        let gate_pos = body
            .find("get_teams_presence(")
            .expect("process_track must call get_teams_presence (presence gate, issue #3.0-P2)");
        let write_pos = body
            .find("set_teams_status_message(")
            .expect("process_track must call set_teams_status_message");
        assert!(
            gate_pos < write_pos,
            "the presence-gate read must precede the status write in process_track \
             so a busy/meeting presence can suppress it (issue #3.0-P2)"
        );
        // Finding #634 / issue #790: the setPresence/clearPresence calls moved
        // into the shared `sync_availability` tail, which process_track reaches
        // from BOTH of its tails (the identical-write skip and the end of the
        // branch), so the source-level contract is now "process_track maintains
        // the session through sync_availability" — the Graph calls themselves
        // are pinned in the helper bodies below.
        assert!(
            body.contains("sync_availability("),
            "process_track must maintain the presence session through the shared \
             tail (issues #3.0-P1/#634/#790)"
        );
        let presence_source = include_str!("presence.rs")
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("presence.rs has no #[cfg(test)] mod tests block");
        let tail = prod_fn_body(presence_source, "fn sync_availability(");
        assert!(
            tail.contains("arm_presence_session("),
            "the shared tail must re-arm a presence session while playing \
             (issues #3.0-P1/#634/#790)"
        );
        assert!(
            tail.contains("clear_presence_session("),
            "the shared tail must clear the presence session on pause \
             (issues #3.0-P1/#634/#790)"
        );
        let helper = prod_fn_body(presence_source, "fn arm_presence_session(");
        assert!(
            helper.contains("set_teams_presence("),
            "the arm helper must be the setPresence call site (issue #3.0-P1)"
        );
        let clear_helper = prod_fn_body(presence_source, "fn clear_presence_session(");
        assert!(
            clear_helper.contains("clear_teams_presence("),
            "the clear helper must be the clearPresence call site (issue #3.0-P1)"
        );
    }

    /// Issue #364: the debounce predicate fires only for a change inside
    /// the 500ms window — unchanged polls, first writes, and changes past
    /// the window all post.
    #[test]
    fn test_debounce_active_only_for_change_inside_window() {
        assert!(
            !debounce_active(false, Some(Instant::now())),
            "unchanged polls never debounce"
        );
        assert!(
            !debounce_active(true, None),
            "no prior write means nothing to debounce against"
        );
        assert!(
            debounce_active(true, Some(Instant::now())),
            "a change right after a write must debounce"
        );
        assert!(
            !debounce_active(
                true,
                Some(Instant::now() - std::time::Duration::from_secs(10))
            ),
            "a change past the window must post"
        );
        assert_eq!(
            crate::polling::timing::DEBOUNCE_RETRY_SECONDS,
            1,
            "the debounce retry parks ~1s, not on the duration-derived sleep"
        );
    }

    /// Issue #364 ordering guard: the debounce early return runs BEFORE any
    /// side effect, so the retry re-detects the change and emits/posts
    /// exactly once. Pre-fix the store/emit/placeholder-clear/gate work ran
    /// first and only the track key was restored, duplicating the
    /// `spotify-track-changed` event and the Graph presence read on retry.
    #[test]
    fn test_debounce_branch_restores_previous_key_and_sleeps_short() {
        let source = include_str!("write.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("write.rs has no #[cfg(test)] mod tests block");
        let after_sig = prod_source
            .split("pub(crate) fn process_track(")
            .nth(1)
            .expect("process_track definition not found");
        let open = after_sig
            .find('{')
            .expect("process_track has no opening brace");
        let mut depth = 0usize;
        let mut end = None;
        for (i, ch) in after_sig[open..].char_indices() {
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
        let body = &after_sig[open..end.expect("process_track body never closed")];
        let debounce_pos = body
            .find("if debounce_active")
            .expect("process_track must gate on debounce_active (issue #364)");
        for marker in [
            "*last_track_key =",
            "current_track_mut",
            "\"spotify-track-changed\",",
            "teams_token_for_write",
            "*gated_track_key =",
        ] {
            let pos = body
                .find(marker)
                .unwrap_or_else(|| panic!("process_track body must contain {}", marker));
            assert!(
                debounce_pos < pos,
                "debounce check must precede '{}' so the retry re-detects the change exactly once (issue #364)",
                marker
            );
        }
        assert!(
            body.contains("return super::timing::DEBOUNCE_RETRY_SECONDS;"),
            "the debounce branch must park on the short fixed retry, not playing_track_sleep (issue #364)"
        );
        assert_eq!(
            crate::polling::timing::DEBOUNCE_RETRY_SECONDS,
            1,
            "the debounce retry parks ~1s, not on the duration-derived sleep"
        );
        assert!(
            debounce_active(true, Some(Instant::now())),
            "a change right after a write must debounce"
        );
        assert!(
            !debounce_active(false, Some(Instant::now())),
            "unchanged polls never debounce"
        );
    }

    /// Issues #370/#388 structural guard: the Teams refresh lives in one
    /// shared helper called from BOTH write paths. Pre-fix
    /// `handle_no_track` cloned the stored token with no expiry/refresh.
    #[test]
    fn test_teams_token_refresh_is_single_shared_helper() {
        let source = include_str!("write.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("write.rs has no #[cfg(test)] mod tests block");
        assert!(
            prod_source.matches("fn teams_token_for_write(").count() >= 1,
            "teams_token_for_write must be defined"
        );
        // Definition + process_track + handle_no_track call sites.
        assert!(
            prod_source.matches("teams_token_for_write(").count() >= 3,
            "expected def + 2 call sites (process_track, handle_no_track); found a drift"
        );
        assert!(
            !prod_source.contains("let teams_tok = match teams_tokens"),
            "handle_no_track must not clone the stored token without refresh (issue #370)"
        );
    }

    /// Issue #373: the first no-track poll attempts one clear (fresh
    /// thread, stale pre-restart status); tracked clears always run;
    /// later nothing-tracked polls stay a no-op.
    #[test]
    fn test_first_no_track_attempts_clear_exactly_once() {
        assert!(
            first_no_track_attempts_clear(&Some("key".to_string()), false),
            "a tracked track must always attempt the clear"
        );
        assert!(
            first_no_track_attempts_clear(&None, true),
            "a fresh thread must attempt one clear even with nothing tracked"
        );
        assert!(
            !first_no_track_attempts_clear(&None, false),
            "later idle polls must stay a no-op"
        );
        assert!(
            first_no_track_attempts_clear(&Some("key".to_string()), true),
            "first iteration with a tracked track still clears"
        );
    }

    /// Issues #380/#430 behavioral late-post contract: a gated track whose
    /// gate clears re-ENTERs the write path exactly once — the gate state
    /// machine (gated → cleared → `None`) combined with #384 dedup
    /// (`should_skip_identical_write`) is what guarantees it. This test
    /// pins the contract WITHOUT network: it drives the pure predicates
    /// `process_track` itself consults, in the order it consults them.
    /// Pre-fix (#380 era) a gated track stayed gated for the whole
    /// duration — there was no re-check branch at all.
    #[test]
    fn test_gated_branch_rechecks_presence_and_clears_gate() {
        use crate::teams::PresenceInfo;
        // 1. The gate classifies a meeting presence as gated, an
        //    available one as cleared — the two states the re-check
        //    discriminates (mocked presence, no network).
        let gated = PresenceInfo {
            availability: "busy".to_string(),
            activity: "inAMeeting".to_string(),
            ..Default::default()
        };
        let cleared = PresenceInfo {
            availability: "available".to_string(),
            activity: "available".to_string(),
            ..Default::default()
        };
        assert!(
            crate::teams::is_presence_gated(&gated, false),
            "a meeting presence must gate (issue #430 precondition)"
        );
        assert!(
            !crate::teams::is_presence_gated(&cleared, false),
            "an available presence must clear the gate (issue #430 trigger)"
        );
        // 2. The re-check is throttled on its own clock (no per-poll
        //    presence storm), and a cleared gate falls through to a write
        //    the #384 dedup still governs — a fresh (never-posted) late
        //    status always writes, a byte-identical one inside the
        //    keepalive does not (no spam).
        let now = Instant::now();
        let now_wall = chrono::Utc::now();
        assert!(
            crate::polling::gate::gate_recheck_due(None, now, now_wall, None),
            "first re-check must be due so the late post can fire"
        );
        assert!(
            !should_skip_identical_write(false, None, "late post", Some(now), now, false),
            "a never-posted late status must write (issue #430 posts it)"
        );
        assert!(
            should_skip_identical_write(
                false,
                Some("late post"),
                "late post",
                Some(now),
                now,
                false
            ),
            "a byte-identical late post inside the keepalive must not re-POST (no spam)"
        );
    }

    /// Issue #384: byte-identical writes skip while the keepalive is
    /// fresh, but a fingerprint/track change or a lapsed keepalive
    /// force-writes.
    #[test]
    fn test_identical_write_skipped_until_fingerprint_change_or_keepalive() {
        let now = Instant::now();
        let fresh = Some(now);
        assert!(
            should_skip_identical_write(false, Some("status"), "status", fresh, now, false),
            "identical status inside the keepalive must skip the write"
        );
        assert!(
            !should_skip_identical_write(false, Some("old"), "new", fresh, now, false),
            "changed text must write"
        );
        assert!(
            !should_skip_identical_write(false, None, "status", fresh, now, false),
            "nothing posted yet must write"
        );
        assert!(
            !should_skip_identical_write(true, Some("status"), "status", fresh, now, false),
            "a fingerprint/track change must force-write even identical text"
        );
        let stale = Some(
            now - std::time::Duration::from_secs(
                crate::polling::timing::STATUS_KEEPALIVE_SECONDS + 1,
            ),
        );
        assert!(
            !should_skip_identical_write(false, Some("status"), "status", stale, now, false),
            "a lapsed keepalive must force-write so the expiry never lapses"
        );
        assert!(
            !should_skip_identical_write(false, Some("status"), "status", None, now, false),
            "no write on record must write"
        );
        // Issue #873: a forced resume write bypasses the dedup
        // exactly once — the first iteration after the idle gate
        // cleared must surface the status to the user even when the
        // text is byte-identical to what Teams already shows.
        assert!(
            !should_skip_identical_write(false, Some("status"), "status", fresh, now, true),
            "a resume-after-idle must force-write even identical text"
        );
    }

    /// Issue #455-residual + #929 rework: behavioural proof that the shared
    /// 401-retry helper actually retries the write with the NEW access token
    /// after a successful refresh, instead of asserting on source text.
    /// Drives [`teams_write_with_refresh_fn`] with fakes:
    /// * `write_fn` records the access token it was handed and returns
    ///   `TeamsApiError::ExpiredToken` on the first call and `Ok(())` on the
    ///   second — modelling the pre-fix silent 401 followed by a retry after
    ///   refresh.
    /// * `refresh_fn` returns tokens whose `access_token` differs from the
    ///   original, with `refresh_calls` counter incremented for the side
    ///   effect (mirrors the real `refresh_teams_token` exactly once on a
    ///   401).
    ///
    /// Asserts the helper's observable behaviour: the write closure is called
    /// exactly twice; the second call receives the NEW access token; the
    /// helper returns `Ok(new_tokens)`. Source-text wiring alone (no
    /// execution) would let a closure that no-ops the refresh slip through;
    /// executing the helper proves the retry-after-refresh contract.
    #[test]
    fn test_teams_write_with_refresh_fn_retries_after_refresh_uses_new_token() {
        let state = Arc::new(AppState::new());
        let teams_tok = TeamsTokens {
            access_token: "old-access".to_string(),
            refresh_token: Some("teams-refresh".to_string()),
            expires_at: Utc::now() + chrono::Duration::hours(1),
        };
        // Production precondition: the slot already holds the pre-refresh
        // access token so the CAS inside the helper commits the new pair
        // instead of discarding.
        state
            .tokens_load
            .commit_teams(&state.tokens, teams_tok.clone());
        let new_tokens = TeamsTokens {
            access_token: "new-access".to_string(),
            refresh_token: Some("teams-refresh".to_string()),
            expires_at: Utc::now() + chrono::Duration::hours(1),
        };
        let mut write_calls: Vec<String> = Vec::new();
        let mut refresh_calls: u32 = 0;
        let emitter = CapturingEmitter::new();
        let persist_calls = std::sync::Arc::new(std::sync::Mutex::new(0u32));
        let result = {
            let persist_calls = persist_calls.clone();
            teams_write_with_refresh_fn(
                &state,
                &teams_tok,
                |access_token| {
                    write_calls.push(access_token.to_string());
                    if write_calls.len() == 1 {
                        Err(TeamsApiError::ExpiredToken(401))
                    } else {
                        Ok(())
                    }
                },
                || {
                    refresh_calls += 1;
                    Ok(new_tokens.clone())
                },
                |_s| {
                    *persist_calls
                        .lock()
                        .expect("persist-call mutex must not be poisoned") += 1;
                    Ok(())
                },
                &emitter,
                "test: retry-after-refresh",
            )
        };
        let returned = result.expect("retry after refresh must succeed");
        assert_eq!(
            refresh_calls, 1,
            "the refresh closure must be called exactly once on a 401"
        );
        assert_eq!(
            write_calls.len(),
            2,
            "the write closure must be called twice (initial + retry)"
        );
        assert_eq!(
            write_calls[0], "old-access",
            "the first write must use the pre-refresh access token"
        );
        assert_eq!(
            write_calls[1], "new-access",
            "the second write must use the refresh-supplied access token"
        );
        assert_eq!(
            returned.access_token, "new-access",
            "the helper must return the refreshed tokens so the caller can \
             adopt them for the availability re-arm in the same iteration \
             (issue #929)"
        );
        // Success path: persist runs exactly once (post-CAS, pre-retry).
        // No reconnect marker, no typed-error payload.
        assert_eq!(
            *persist_calls
                .lock()
                .expect("persist-call mutex must not be poisoned"),
            1,
            "the success path must persist the refreshed tokens exactly once"
        );
        assert!(
            emitter
                .markers
                .lock()
                .expect("capturing-emitter mutex must not be poisoned")
                .is_empty(),
            "the success path must not emit `teams-reconnect-required`"
        );
        assert!(
            emitter
                .events
                .lock()
                .expect("capturing-emitter mutex must not be poisoned")
                .is_empty(),
            "the success path must not emit a typed error event"
        );
    }

    /// Issue #929 rework: behavioural proof that a dead refresh credential
    /// causes the helper to surface `teams-reconnect-required` so the
    /// Dashboard can prompt the user. No GUI runtime, no Tauri event
    /// channel — the module-scope [`CapturingEmitter`] captures the
    /// marker directly and a recording closure counts the persist attempt.
    ///
    /// `write_fn` returns `ExpiredToken` on every call (the initial 401 and
    /// any retry the helper might attempt) so the helper cannot accidentally
    /// succeed via the retry-after-refresh path; `refresh_fn` returns the
    /// typed `InvalidGrant` dead-credential signal the refresh endpoint emits.
    /// The helper also returns the typed error so the caller's own
    /// classifier can react.
    #[test]
    fn test_teams_write_with_refresh_fn_emits_reconnect_on_dead_credential() {
        let state = Arc::new(AppState::new());
        let teams_tok = TeamsTokens {
            access_token: "old-access".to_string(),
            refresh_token: Some("dead-refresh".to_string()),
            expires_at: Utc::now() + chrono::Duration::hours(1),
        };
        // Production precondition: the slot holds the pre-refresh token so
        // the refresh-failure handler's clear-when-current path runs and the
        // typed error is returned for the classifier to emit on.
        state
            .tokens_load
            .commit_teams(&state.tokens, teams_tok.clone());
        let emitter = CapturingEmitter::new();
        let mut persist_calls: u32 = 0;
        let result = teams_write_with_refresh_fn(
            &state,
            &teams_tok,
            |_| Err(TeamsApiError::ExpiredToken(401)),
            || Err(TeamsApiError::InvalidGrant),
            |_s| {
                persist_calls += 1;
                Ok(())
            },
            &emitter,
            "test: dead-credential",
        );
        assert!(
            matches!(
                result,
                Err(TeamsWriteError::Api(TeamsApiError::InvalidGrant))
            ),
            "the helper must surface the typed refresh error so callers can \
             classify it (issue #929): got {:?}",
            result
        );
        assert_eq!(
            persist_calls, 1,
            "the cleared-token persist must be attempted after a dead-credential \
             clear, otherwise the discarded tokens come back on next launch"
        );
        let markers = emitter
            .markers
            .lock()
            .expect("capturing-emitter mutex must not be poisoned");
        assert!(
            markers.iter().any(|s| s == "teams-reconnect-required"),
            "the helper must emit `teams-reconnect-required` on a dead \
             credential so the Dashboard opens the reconnect prompt \
             (issue #929): observed markers = {:?}",
            *markers
        );
        assert_eq!(
            markers.len(),
            1,
            "a dead-credential failure must emit exactly one marker, never a \
             duplicate the Dashboard would render twice (issue #929): {:?}",
            *markers
        );
        let events = emitter
            .events
            .lock()
            .expect("capturing-emitter mutex must not be poisoned");
        assert!(
            events.is_empty(),
            "the reconnect marker must NOT also raise an `error` event — that \
             would double-report the same failure to the user (issue #929): \
             events = {:?}",
            *events
        );
    }

    /// Issue #1117 (review P0): a token slot that MOVES while the reactive
    /// refresh is in flight must come back as [`TeamsWriteError::Superseded`],
    /// not as a typed API error. The first cut of #929 returned
    /// `TeamsApiError::Transient("slot replaced before clear; …")`, which every
    /// caller then handled as a real failure: `process_track` logged at error,
    /// pushed a Dashboard banner ("Microsoft Teams is temporarily unavailable.
    /// Retrying shortly.") where main had shown nothing, and fell through to
    /// `sync_availability` with the stale token.
    ///
    /// Drives the core with fakes and reproduces the interleaving — the slot
    /// holds the pre-refresh token, the write 401s, and a newer session is
    /// committed while the refresh runs. Both supersede conditions are
    /// covered: a dead credential whose clear-when-current loses the CAS (the
    /// branch that logged "no-op" while no-op-ing nothing), and a transient
    /// refresh that finds the slot already replaced. Contrast: with the slot
    /// unmoved the same dead credential yields `Api(InvalidGrant)` plus the
    /// `teams-reconnect-required` marker
    /// (`test_teams_write_with_refresh_fn_emits_reconnect_on_dead_credential`),
    /// so the supersede detection is the only thing under test here.
    #[test]
    fn test_superseded_slot_returns_no_op_instead_of_an_api_error() {
        let teams_tok = TeamsTokens {
            access_token: "old-access".to_string(),
            refresh_token: Some("stale-refresh".to_string()),
            expires_at: Utc::now() + chrono::Duration::hours(1),
        };
        let new_session = TeamsTokens {
            access_token: "newer-access".to_string(),
            refresh_token: Some("newer-refresh".to_string()),
            expires_at: Utc::now() + chrono::Duration::hours(1),
        };

        for (case, refresh_err) in [
            (
                "dead credential whose clear-when-current loses the slot",
                TeamsApiError::InvalidGrant,
            ),
            (
                "transient refresh that finds the slot already replaced",
                TeamsApiError::Transient("connection reset".to_string()),
            ),
        ] {
            let state = Arc::new(AppState::new());
            state
                .tokens_load
                .commit_teams(&state.tokens, teams_tok.clone());
            let emitter = CapturingEmitter::new();
            let persist_calls = std::sync::Arc::new(std::sync::Mutex::new(0u32));
            let result = {
                let persist_calls = persist_calls.clone();
                let new_session = new_session.clone();
                teams_write_with_refresh_fn(
                    &state,
                    &teams_tok,
                    |_| Err(TeamsApiError::ExpiredToken(401)),
                    || {
                        // A concurrent login lands mid-refresh: the slot now
                        // holds a session this writer never saw.
                        state
                            .tokens_load
                            .commit_teams(&state.tokens, new_session.clone());
                        Err(refresh_err.clone())
                    },
                    move |_s| {
                        *persist_calls
                            .lock()
                            .expect("persist-call mutex must not be poisoned") += 1;
                        Ok(())
                    },
                    &emitter,
                    "test: superseded-slot",
                )
            };

            assert!(
                matches!(result, Err(TeamsWriteError::Superseded)),
                "{case}: a replaced slot must surface as `Superseded` so the \
                 caller no-ops — never as a `TeamsApiError` the caller would \
                 banner and back off for (issue #1117): got {result:?}"
            );
            assert_eq!(
                *persist_calls
                    .lock()
                    .expect("persist-call mutex must not be poisoned"),
                0,
                "{case}: nothing changed on this path, so the persist seam must \
                 not run (issue #1117)"
            );
            assert!(
                emitter
                    .markers
                    .lock()
                    .expect("capturing-emitter mutex must not be poisoned")
                    .is_empty(),
                "{case}: a superseded write must NOT emit \
                 `teams-reconnect-required` — the newer session works, so \
                 sending the user to re-auth would be wrong (issue #1117)"
            );
            assert!(
                emitter
                    .events
                    .lock()
                    .expect("capturing-emitter mutex must not be poisoned")
                    .is_empty(),
                "{case}: a superseded write must NOT emit a typed `error` event \
                 (issue #929 folded this into a `Transient`, which produced a \
                 spurious Dashboard banner)"
            );
            assert_eq!(
                state
                    .tokens
                    .teams()
                    .as_ref()
                    .map(|t| t.access_token.as_str()),
                Some("newer-access"),
                "{case}: the newer session must survive untouched (issue #798)"
            );
        }
    }

    /// Issue #1117: the caller-side half of the same fix, pinned on the
    /// RETURN VALUE. `process_track`'s playing write must turn a superseded
    /// failure into "end the iteration with a zero sleep" — no error log, no
    /// Dashboard banner, no backoff, and (because it returns before the
    /// presence tail) no `sync_availability` call against the stale token that
    /// just lost the slot. Main's pre-#929 inline handler did exactly this with
    /// `return 0;`.
    ///
    /// Why this asserts the whole [`PlayingIterationOutcome`] and not just a
    /// mapper: the first cut kept `return 0;` as a statement in the match arm
    /// and only mapped the typed error to an enum variant. Mutation testing
    /// showed the mapper test stayed green with that `return 0;` replaced by
    /// `() => {}`, which restored the stale-token `sync_availability` re-arm
    /// plus a backoff — the exact P0 #929 exists to fix, unpinned. The arm
    /// decision now lives in [`playing_write_iteration_outcome`], so the
    /// short-circuit IS the value this test checks, and deleting it fails
    /// here.
    ///
    /// The `prior_backoff_secs` sweep is load-bearing: it pins the superseded
    /// arm at a zero sleep even when a backoff is already pending, which is
    /// the "backoff instead of 0" half of that regression.
    #[test]
    fn test_playing_write_iteration_outcome_ends_only_a_superseded_write() {
        for prior_backoff_secs in [0u64, 1, 30, 300, 900] {
            assert!(
                matches!(
                    playing_write_iteration_outcome(
                        TeamsWriteError::Superseded,
                        prior_backoff_secs
                    ),
                    PlayingIterationOutcome::EndIteration { sleep_secs: 0 }
                ),
                "a superseded slot must end the iteration with a ZERO sleep \
                 (pre-#929 `return 0;`) so no banner fires and the presence \
                 tail never re-arms on the stale token — and it must not honour \
                 the pending backoff of {prior_backoff_secs}s, because a newer \
                 live session already owns Teams (issue #1117)"
            );
        }

        let genuine = [
            TeamsApiError::Transient("connection reset".to_string()),
            TeamsApiError::RateLimited(Some(30)),
            TeamsApiError::RateLimited(None),
            TeamsApiError::Forbidden(403, "insufficient_claims".to_string()),
            TeamsApiError::InvalidGrant,
            TeamsApiError::ExpiredToken(401),
        ];
        for error in genuine.clone() {
            for prior_backoff_secs in [0u64, 30, 300] {
                match playing_write_iteration_outcome(
                    TeamsWriteError::Api(error.clone()),
                    prior_backoff_secs,
                ) {
                    PlayingIterationOutcome::ContinueWithBackoff {
                        backoff_secs,
                        error: surfaced,
                    } => {
                        assert_eq!(
                            surfaced.to_string(),
                            error.to_string(),
                            "a genuine Teams failure must reach the caller's error \
                             path with the SAME typed error so the banner and backoff \
                             are unchanged (issue #1117): {error:?}"
                        );
                        // Issue #154: the only backoff that may exceed the pending
                        // one is a server-directed delay, and the pending one may
                        // never be lowered. A 429 WITHOUT `Retry-After` falls back
                        // to a jittered 60s hold, so only a `Some(secs)` arm can be
                        // pinned exactly; every other variant must merely land at
                        // or above the prior.
                        match &error {
                            TeamsApiError::RateLimited(Some(secs)) => assert_eq!(
                                backoff_secs,
                                prior_backoff_secs.max(*secs),
                                "a server-directed delay may raise the backoff but \
                                 never lower it: error = {error:?}, prior = \
                                 {prior_backoff_secs}, got {backoff_secs} (issue #154)"
                            ),
                            _ => assert!(
                                backoff_secs >= prior_backoff_secs,
                                "the backoff must never be lowered: error = {error:?}, \
                                 prior = {prior_backoff_secs}, got {backoff_secs} \
                                 (issue #154)"
                            ),
                        };
                    }
                    other => panic!(
                        "a genuine Teams failure must NOT be swallowed as a \
                         no-op — the Dashboard would show a stuck status with no \
                         banner, and a rate limit would be ignored (issue #1117): \
                         {other:?} for {error:?} with prior {prior_backoff_secs}"
                    ),
                }
            }
        }

        // A 429 with no `Retry-After` still has to hold the poll off, so the
        // helper falls back to jittered `rate_limit_sleep_secs` over its 60s base.
        // With nothing pending (prior 0) the fallback wins `max` outright, so
        // the observable value must be a plausible jittered-60s result: pin
        // the band, not a fixed value (which would overfit the RNG). The
        // neighbouring matrix already proves the backoff is never lowered, so
        // this block proves the complementary half — the fallback actually
        // fires at its documented base.
        match playing_write_iteration_outcome(
            TeamsWriteError::Api(TeamsApiError::RateLimited(None)),
            0,
        ) {
            PlayingIterationOutcome::ContinueWithBackoff { backoff_secs, .. } => assert!(
                (48..=72).contains(&backoff_secs),
                "the jittered rate-limit fallback must land near its 60s base \
                 (a 429 without Retry-After may only hold the poll off): got \
                 {backoff_secs} (issue #154)"
            ),
            other => panic!(
                "a 429 without `Retry-After` must still reach the caller's error \
                 path (issue #1117): {other:?}"
            ),
        }
    }

    /// Issue #929 rework (B1 closure, `tauri::test`-free): behavioural proof
    /// that the PRODUCTION binding still wires the app emitter through to the
    /// reconnect path. The `teams_write_with_refresh_fn` core-helper tests prove
    /// the shared core emits `teams-reconnect-required` on a dead credential;
    /// this test proves the one remaining choice the core tests do NOT cover —
    /// that `teams_write_with_optional_refresh`, the only place the real
    /// `refresh_teams_token` / `token_io::persist_tokens` closures are bound,
    /// still binds them, and still hands `app` through as the emitter. No
    /// mock runtime: there is no `tauri::test` in this crate's dev-deps (the
    /// feature breaks the Windows loader and skews the macOS build), so this
    /// is proved by pinning the binding source text at its function body
    /// rather than by executing a GUI bus.
    ///
    /// If someone moves the real-closure binding or stops passing `app`, this
    /// fails. The core path it guards is executed by
    /// `test_teams_write_with_refresh_fn_emits_reconnect_on_dead_credential`,
    /// so together the two tests cover behaviour + binding.
    #[test]
    fn test_wrapper_still_binds_real_refresh_and_app_emitter() {
        let prod_source = prod_source();
        // Same anchor convention as the anti-decoy persist guard (line 6473):
        // `prod_fn_body` over the full generic signature.
        let body = prod_fn_body(prod_source, "fn teams_write_with_optional_refresh<F, Rt>(");
        for wanted in [
            "refresh_teams_token(teams_tok)",
            "token_io::persist_tokens(s, app)",
        ] {
            assert!(
                body.contains(wanted),
                "the production wrapper must still bind the real closures;                  `{wanted}` is missing from its body (issue #929 rework): {body}"
            );
        }
        assert!(
            !body.contains("tauri::test"),
            "the production wrapper must not reach for a mock runtime (issue #929 rework)"
        );
    }

    /// Issue #929 rework: behavioural proof that the paused-track clear arm
    /// surfaces transient / rate-limited / other failures to the Dashboard at
    /// `ErrorSeverity::Warning` — matching the playing arm's coverage so the
    /// user sees a warning instead of a silent stuck status. Asserts the
    /// actual severity, not the shape of the production match.
    ///
    /// Drives [`emit_paused_clear_failure`] with a fake [`ErrorEventEmitter`]
    /// (the production `&AppHandle` impl is not used here — the trait exists
    /// exactly for this kind of testability). Covers all three transient
    /// variants so a future regression that reclassifies one as `Error` is
    /// caught.
    #[test]
    fn test_paused_clear_transient_failure_emits_warning_severity() {
        for transient in [
            TeamsApiError::RateLimited(Some(30)),
            TeamsApiError::RateLimited(None),
            TeamsApiError::Transient("connection refused".to_string()),
            TeamsApiError::Other(502, "Bad Gateway".to_string()),
        ] {
            let emitter = CapturingEmitter::new();
            emit_paused_clear_failure(&emitter, &transient);
            let events = emitter
                .events
                .lock()
                .expect("capturing-emitter mutex must not be poisoned");
            assert_eq!(
                events.len(),
                1,
                "a transient variant must emit exactly one `error` event \
                 (issue #929): variant = {:?}, events = {:?}",
                transient,
                *events
            );
            assert_eq!(events[0].0, "error", "the event name must be `error`");
            assert_eq!(
                events[0].1.severity,
                ErrorSeverity::Warning,
                "a transient paused-clear failure must surface at \
                 `ErrorSeverity::Warning`, not `Error` (issue #929): \
                 variant = {:?}",
                transient
            );
            assert_eq!(
                events[0].1.source, "teams",
                "the error source must be `teams` so the Dashboard scopes the banner"
            );
            assert!(
                events[0].1.message.starts_with("Paused-track clear: "),
                "the message must be prefixed so the Dashboard can identify \
                 the paused-clear origin (issue #929): variant = {:?}",
                transient
            );
        }

        // Auth + Forbidden variants must NOT emit an error event here — the
        // shared helper already emitted `teams-reconnect-required` (auth) or
        // logged Forbidden as permission/license. Re-emitting would be
        // double-reporting (issue #929 review).
        for classified in [
            TeamsApiError::ExpiredToken(401),
            TeamsApiError::InvalidGrant,
            TeamsApiError::ReauthRequired("interaction_required".to_string()),
            TeamsApiError::Forbidden(403, "insufficient_claims".to_string()),
        ] {
            let emitter = CapturingEmitter::new();
            emit_paused_clear_failure(&emitter, &classified);
            let events = emitter
                .events
                .lock()
                .expect("capturing-emitter mutex must not be poisoned");
            assert!(
                events.is_empty(),
                "an already-classified variant must not re-emit an error event \
                 here (issue #929): variant = {:?}, events = {:?}",
                classified,
                *events
            );
        }
    }

    /// Narrow wiring guard (NOT a behaviour test): `process_track`'s paused
    /// `PlaceholderWrite::Post` arm must route through
    /// [`teams_write_with_optional_refresh`].
    ///
    /// Why a source guard survives here (issue #778 allows exactly this
    /// shape): the invariant is *which function the arm calls*. Driving the
    /// helper behaviourally cannot observe it — the arm lives inside
    /// `process_track`, whose other inputs are a real `AppHandle`, real
    /// presence reads and a Graph round-trip, none of which a hermetic unit
    /// test may reach. Extracting the arm into a seam-bearing function would
    /// only move the hole: the test would then drive the extracted function
    /// while a reverted arm stayed green.
    ///
    /// Scoped to the arm (issue #1117): the first cut brace-counted the WHOLE
    /// `process_track`, which contains two helper call sites (the playing
    /// write and this paused clear). Reverting only this arm to a direct
    /// `clear_teams_status_message` left the playing write's occurrence, so
    /// the guard could not fail for its stated purpose. The body is now
    /// brace-counted from the arm head, and the pre-#929 direct-call shape
    /// (`match clear_teams_status_message(`) is asserted absent, so the arm
    /// must both call the helper and not call Graph directly.
    #[test]
    fn test_paused_clear_arm_routes_through_helper_wiring_guard() {
        let track_body = prod_fn_body(prod_source(), "pub(crate) fn process_track(");
        let arm_start = track_body
            .find("PlaceholderWrite::Post =>")
            .expect("process_track must keep the paused-clear POST arm (issue #155)");
        let arm = brace_counted_body(&track_body[arm_start..], "PlaceholderWrite::Post =>");
        assert_eq!(
            arm.matches("teams_write_with_optional_refresh(").count(),
            1,
            "the paused-clear POST arm must route through \
             teams_write_with_optional_refresh exactly once (wiring guard, \
             issue #929): arm =\n{}",
            arm
        );
        assert!(
            !arm.contains("match clear_teams_status_message("),
            "the paused-clear POST arm must not call Graph directly — a direct \
             call is what pre-#929 did, and it left a 401 on the paused clear \
             with no refresh and no retry (issue #929). Arm =\n{}",
            arm
        );
    }

    /// Issue #455-residual: the no-track clear path must mirror the
    /// process_track ExpiredToken refresh+single-retry — a 401 on the clear
    /// can mean the token expired mid-sequence even though the pre-write
    /// expiry check passed.
    ///
    /// Issue #929 reshapes the three Teams write sites (playing write,
    /// no-track clear, paused clear) around the shared helper
    /// `teams_write_with_optional_refresh`. `handle_no_track` no longer
    /// inlines the ExpiredToken match; it delegates to the helper, and the
    /// helper owns the refresh, CAS-commit, persist, single retry, and
    /// typed classification (teams-reconnect-required emit on auth, Forbidden
    /// log-only, transient log-warn).
    ///
    /// Issue #929 rework: the retry-after-refresh behaviour for the no-track
    /// clear is the SAME helper as the paused clear, so the four behavioural
    /// tests above already cover the no-track call site in spirit. This test
    /// is kept as a narrow wiring guard for that one specific call site so
    /// a future revert of `handle_no_track`'s delegation is caught. NOT a
    /// behaviour test.
    #[test]
    fn test_no_track_clear_arm_routes_through_helper_wiring_guard() {
        let body = prod_fn_body(prod_source(), "pub(crate) fn handle_no_track(");
        assert!(
            body.contains("teams_write_with_optional_refresh("),
            "handle_no_track must delegate the Teams clear to the shared \
             401-retry helper (wiring guard, issue #929)"
        );
    }

    /// Findings D3/D4 (issues #686/#687): a SUPPRESSED placeholder write is
    /// never recorded as posted, so the clear is retried once the gate clears;
    /// a genuinely posted one still dedups (#155).
    #[test]
    fn test_placeholder_write_decision_retries_a_suppressed_write() {
        assert_eq!(
            placeholder_write_decision(false, false, false),
            PlaceholderWrite::Post
        );
        assert_eq!(
            placeholder_write_decision(false, true, false),
            PlaceholderWrite::SkipDuplicate,
            "the issue #155 dedup still holds for a placeholder Teams shows"
        );
        assert_eq!(
            placeholder_write_decision(true, false, false),
            PlaceholderWrite::Suppress { announce: true },
            "a gated write is suppressed, never posted (findings D3/D4)"
        );
        assert_eq!(
            placeholder_write_decision(true, true, false),
            PlaceholderWrite::Suppress { announce: true },
            "even when a stale post marker is present, a gated write is not a post"
        );
        assert_eq!(
            placeholder_write_decision(true, false, true),
            PlaceholderWrite::Suppress { announce: false },
            "one presence-gated event per suppression episode, not one per poll"
        );
        assert_eq!(
            placeholder_write_decision(false, false, true),
            PlaceholderWrite::Post,
            "the gate clearing must RETRY the suppressed clear (findings D3/D4) — \
             pre-fix the helper did not exist and the gated branch marked the \
             placeholder as posted, so `gate_recheck_due` could never re-post it"
        );
    }

    /// Finding D3 (issue #686) structural guard: in the paused-clear branch the
    /// gate verdict is computed BEFORE the byte-identity comparison, and the
    /// suppressing arm records a suppression instead of a post.
    #[test]
    fn test_paused_clear_asks_the_gate_before_the_dedup() {
        let prod = prod_source();
        let track_body = prod_fn_body(prod, "pub(crate) fn process_track(");
        let paused_start = track_body
            .find("let placeholder_text = super::status_text::paused_status_placeholder(config);")
            .expect("process_track must keep the config-driven paused placeholder (S4)");
        let paused = &track_body[paused_start..];
        let verdict = paused
            .find("let mut gate_blocked = false;")
            .expect("the paused branch must compute a gate verdict (finding D3)");
        let decision = paused
            .find("placeholder_write_decision(gate_blocked,")
            .expect("the paused branch must decide from the verdict (finding D3)");
        assert!(
            verdict < decision,
            "the gate verdict must be computed before the outcome is decided: pre-fix \
             the byte-identity comparison ran first and a suppressed write claimed it \
             had been posted (finding D3). `already_posted` may be computed earlier — \
             it also feeds the no-read fast path (review round 2, item 4) — but it is \
             only an INPUT to the decision, never the decision itself."
        );
        assert!(
            !paused.contains("if last_posted_placeholder.as_deref() == Some(placeholder) {"),
            "the paused branch must not branch on the dedup directly: that is the \
             pre-fix shape that let a suppressed write look posted (finding D3)"
        );
        let suppress_arm_start = paused
            .find("PlaceholderWrite::Suppress")
            .expect("the paused branch must use the shared decision (findings D3/D4)");
        let post_arm_start = paused
            .find("PlaceholderWrite::Post")
            .expect("the paused branch must keep the POST arm");
        let suppress_arm = &paused[suppress_arm_start..post_arm_start];
        assert!(
            !suppress_arm.contains("last_posted_placeholder = Some"),
            "a suppressed paused-clear must not claim the placeholder was posted \
             (finding D3)"
        );
        assert!(
            suppress_arm.contains("suppressed_placeholder = Some"),
            "a suppressed paused-clear must record the suppression (finding D3)"
        );
        assert!(
            paused.contains("*suppressed_placeholder = None;"),
            "an allowed clear retires the suppression marker"
        );
        // The pause is re-decided once its re-check is due, like the playing
        // branch — otherwise the gate could never clear on the pause path.
        // Issue #867 widens the call with `chrono::Utc::now()` and the
        // calendar boundary so the boundary-driven un-gate works there too;
        // the assertion only needs to prove the call still happens here.
        assert!(
            paused.contains("gate_recheck_due(")
                && paused.contains("*last_gate_check,")
                && paused.contains("Instant::now(),"),
            "the paused-clear gate must be re-evaluated once the re-check is due \
             (finding D3)"
        );
    }

    /// Finding D4 (issue #687) structural guard: the same ordering on the
    /// no-track clear.
    #[test]
    fn test_no_track_clear_asks_the_rule_before_the_dedup() {
        let prod = prod_source();
        let body = prod_fn_body(prod, "pub(crate) fn handle_no_track(");
        let verdict = body
            .find("if no_track_rule.suppresses()")
            .expect("handle_no_track must ask the rule decision");
        let dedup = body
            .find("let already_posted =")
            .expect("handle_no_track must compare against what Teams shows");
        let clear = body
            .find("clear_teams_status_message(")
            .expect("handle_no_track must keep the clear POST");
        assert!(
            verdict < dedup && dedup < clear,
            "the no-track rule verdict must precede the byte-identity comparison, and \
             the comparison must precede the POST (finding D4)"
        );
        let suppress_arm_start = body
            .find("PlaceholderWrite::Suppress")
            .expect("the no-track clear must use the shared decision (finding D4)");
        let post_arm_start = body
            .find("PlaceholderWrite::Post")
            .expect("the no-track clear must keep the POST arm");
        let suppress_arm = &body[suppress_arm_start..post_arm_start];
        assert!(
            !suppress_arm.contains("last_posted_placeholder = Some"),
            "a suppressed no-track clear must not claim the placeholder was posted \
             (finding D4)"
        );
        assert!(
            suppress_arm.contains("suppressed_placeholder = Some"),
            "a suppressed no-track clear must record the suppression (finding D4)"
        );
    }

    /// Finding D6 (issue #689): pausing the SAME track is a state change.
    #[test]
    fn test_playback_state_change_is_detected_for_the_same_track() {
        assert!(
            playback_state_changed(Some(true), false),
            "pausing the same track must be a change (finding D6) — the status key \
             excludes `is_playing`, so nothing else notices"
        );
        assert!(
            playback_state_changed(Some(false), true),
            "resuming must be a change too (finding D6)"
        );
        assert!(!playback_state_changed(Some(true), true));
        assert!(!playback_state_changed(Some(false), false));
        assert!(
            playback_state_changed(None, false),
            "an absent stored track counts as a change (re-store, never assume)"
        );

        let prod = prod_source();
        let track_body = prod_fn_body(prod, "pub(crate) fn process_track(");
        assert!(
            track_body.contains("let playing_changed ="),
            "process_track must derive the playback-state change (finding D6)"
        );
        let arm = track_body
            .find("} else if playing_changed {")
            .expect("the playback-state change needs its own arm (finding D6)");
        let arm_body = &track_body[arm..];
        assert!(
            arm_body.contains("current_track_mut()")
                && arm_body.contains("\"playback-state-changed\"")
                && arm_body.contains("playback_state_changed_payload("),
            "the arm must re-store the observed track — so the sync status reports \
             the paused track — and emit the playback-state-changed payload (finding D6)"
        );
        // Review round 2, item 1: the arm's guard must be the REAL predicate.
        // Disabling the feature with `let playing_changed = false;` leaves every
        // other assertion in this test passing, so the derivation itself is what
        // has to be pinned — process_track needs a live AppHandle and a Graph
        // read, so there is no unit-level way to observe the arm's effect.
        assert!(
            track_body.contains(
                "let playing_changed = !changed && playback_state_changed(stored_is_playing, track.is_playing);"
            ),
            "the playback-state arm must be driven by playback_state_changed(...) on the \
             observations (finding D6): forcing `playing_changed` to false silently \
             disables the whole fix (review round 2, item 1)"
        );
    }

    /// Review follow-up on #684: the no-track path owns `gated_track_key`, so a
    /// gate recorded for a track cannot outlive it — while a suppression with
    /// nothing playing stays representable.
    #[test]
    fn test_no_track_path_retires_the_finished_tracks_gate() {
        // A real status key always carries the kind/fingerprint separators, so
        // the sentinel can never collide with one.
        let finished_track_key = crate::polling::status_text::status_track_key(
            &crate::spotify::NowPlaying::default(),
            &None,
        );
        assert!(
            finished_track_key.contains(" | "),
            "a status key is <title> - <artist> | <kind> | <config fingerprint>"
        );
        assert_ne!(finished_track_key, NO_TRACK_GATE_KEY);

        // A gated track ends (the clock still names it) and the no-track clear
        // is suppressed by quiet hours / a match-all rule.
        let decision = placeholder_write_decision(true, false, false);
        assert_eq!(decision, PlaceholderWrite::Suppress { announce: true });
        let suppressed_gate = no_track_gate_key(true).map(str::to_string);
        assert_eq!(
            suppressed_gate.as_deref(),
            Some(NO_TRACK_GATE_KEY),
            "a gate with no track present is real and must stay representable, otherwise \
             the Dashboard chip cannot say why the clear is suppressed"
        );
        assert_ne!(
            suppressed_gate.as_deref(),
            Some(finished_track_key.as_str()),
            "the gate must stop naming the track that has ended"
        );

        // The suppression lifts and the clear is posted: nothing is gated.
        assert_eq!(
            no_track_gate_key(false).map(str::to_string),
            None,
            "a posted clear must leave get_sync_status answering presence_gated = false"
        );
    }

    /// Review follow-up on #684, structural guard (the head of #702 failed
    /// exactly here): `handle_no_track` must retire the gate when its clear
    /// succeeds and record the no-track sentinel while a rule suppresses it.
    /// Pre-fix it never touched `gated_track_key`, so a gated track's key
    /// survived the track and `get_sync_status` kept reporting
    /// `presence_gated = true` over a "Nothing playing" card.
    #[test]
    fn test_no_track_path_owns_the_gate_state() {
        let prod = prod_source();
        let body = prod_fn_body(prod, "pub(crate) fn handle_no_track(");
        assert!(
            body.contains("*gated_track_key = "),
            "handle_no_track must write clocks.gated_track_key: it is the only owner of \
             that state once no track is playing (review follow-up on #684)"
        );
        let suppress_start = body
            .find("PlaceholderWrite::Suppress")
            .expect("the no-track clear must use the shared decision");
        let skip_start = body
            .find("PlaceholderWrite::SkipDuplicate")
            .expect("the no-track clear must keep the dedup arm");
        let suppress_arm = &body[suppress_start..skip_start];
        assert!(
            suppress_arm.contains("no_track_gate_key(true)"),
            "a suppressed no-track clear IS gated: record the no-track sentinel, not the \
             finished track's key (review follow-up on #684)"
        );
        let post_start = body
            .find("PlaceholderWrite::Post")
            .expect("the no-track clear must keep the POST arm");
        let skip_arm = &body[skip_start..post_start];
        assert!(
            skip_arm.contains("no_track_gate_key(false)"),
            "an already-correct placeholder leaves nothing gated: retire the key"
        );
        assert!(
            body.matches("no_track_gate_key(false)").count() >= 2,
            "both non-suppressing outcomes (dedup and the posted clear) must retire the \
             gate, so no path leaves a stale one behind"
        );
    }

    /// Review round 2, item 4: the paused clear must not spend a Graph
    /// `/presence` GET per poll on a steady pause — and the fast path may not
    /// come back at the price of the D3 poisoning (a suppressed write claiming
    /// it was posted).
    #[test]
    fn test_paused_clear_skips_the_gate_read_only_when_it_cannot_matter() {
        // The steady pause: the placeholder is on Teams, no rule suppresses,
        // no gate is due for re-check -> skip the read (#155 dedup decides).
        assert!(paused_clear_skips_gate_read(true, false, false));
        // A rule suppression is computed without a read and must still be
        // recorded and announced (the Dashboard chip), so take the full path.
        assert!(!paused_clear_skips_gate_read(true, true, false));
        // A recorded gate that reached its re-check is exactly what a read
        // clears, so it must happen.
        assert!(!paused_clear_skips_gate_read(true, false, true));
        // Nothing posted yet: the clear may have to happen.
        assert!(!paused_clear_skips_gate_read(false, false, false));
        assert!(!paused_clear_skips_gate_read(false, true, true));

        // Structural: the skip decision is taken BEFORE the read, and the arm
        // that takes it never records a post (the D3 poisoning).
        let prod = prod_source();
        let track_body = prod_fn_body(prod, "pub(crate) fn process_track(");
        let paused_start = track_body
            .find("let placeholder_text = super::status_text::paused_status_placeholder(config);")
            .expect("process_track must keep the config-driven paused placeholder (S4)");
        let paused = &track_body[paused_start..];
        let skip_check = paused
            .find("if paused_clear_skips_gate_read(")
            .expect("the paused branch must take the no-read fast path (review round 2, item 4)");
        let read = paused
            .find("get_teams_presence(")
            .expect("the paused branch keeps its gate read");
        assert!(
            skip_check < read,
            "the skip decision must precede the Graph read it exists to avoid"
        );
        // Review round 3, item 2: pin the ARGUMENTS, not just the predicate —
        // hard-coding `recorded_gate_due` to false at the call site silently
        // re-breaks the retry (a presence gate's clear is then never retried and
        // a rule gate stays stuck) while the predicate test stays green.
        let call_end = paused[skip_check..]
            .find(") {")
            .expect("the fast-path call must close")
            + skip_check;
        let call = &paused[skip_check..call_end];
        assert!(
            call.contains("already_posted")
                && call.contains("rule_suppression_reason.is_some()")
                && call.contains("recorded_gate_due"),
            "the fast-path call must pass the three observations (placeholder posted, rule \
             suppressing, recorded gate due): {:?}",
            call
        );
        assert!(
            !call.contains("false"),
            "no argument of the fast-path call may be a hard-coded literal — a literal \
             `false` for `recorded_gate_due` disables the due-recheck retry (review round \
             3, item 2)"
        );
        let skip_arm_end = paused[skip_check..]
            .find("} else if let Some(reason) = rule_suppression_reason {")
            .expect("the fast path must be the FIRST arm of the verdict chain")
            + skip_check;
        let skip_arm = &paused[skip_check..skip_arm_end];
        assert!(
            !skip_arm.contains("last_posted_placeholder = Some"),
            "the fast path must never claim the placeholder was posted — that is the \
             finding D3 defect this ordering fix exists for"
        );
        assert!(
            !skip_arm.contains("suppressed_placeholder = Some"),
            "and it must not record a suppression it did not observe"
        );
    }

    /// Issue #791: with `respect_manual_status` on and a user-typed status,
    /// playback stopping must leave the status untouched — the manual-status
    /// verdict turns the no-track decision into `Suppress`.
    #[test]
    fn test_no_track_manual_status_verdict_suppresses_the_clear() {
        use crate::platform::focus::PresentationState;
        use crate::teams::{PresenceInfo, PresenceStatusMessage};
        let manual = PresenceInfo {
            availability: "available".to_string(),
            activity: "available".to_string(),
            status_message: Some(PresenceStatusMessage {
                content: "In a workshop".to_string(),
                expires_at: None,
            }),
            ..PresenceInfo::default()
        };
        let verdict = crate::polling::presence::presence_gate_decision(
            &manual,
            false,
            false,
            true,
            None,
            None,
            Utc::now(),
            PresentationState::None,
            false,
            false,
        );
        assert_eq!(
            verdict.as_deref(),
            Some(GATE_REASON_MANUAL_STATUS),
            "a user-typed status blocks even with no track and no other gate (issue #791)"
        );
        assert_eq!(
            placeholder_write_decision(verdict.is_some(), false, false),
            PlaceholderWrite::Suppress { announce: true },
            "the manual-status verdict turns the no-track decision into Suppress, \
             leaving the user's status untouched and recording the presence-gated reason"
        );
        assert!(
            !crate::polling::presence::manual_status_blocks_write(
                false,
                Some(&manual),
                None,
                None,
                Utc::now()
            ),
            "with respect_manual_status off the same status must not block (fail-open opt-out)"
        );
    }

    /// Issue #791 structural guard: `handle_no_track` must consult the
    /// presence/manual-status verdict before the clear POST.
    #[test]
    fn test_no_track_clear_consults_presence_verdict_before_post() {
        let prod = prod_source();
        let body = prod_fn_body(prod, "pub(crate) fn handle_no_track(");
        let read = body
            .find("get_teams_presence(")
            .expect("handle_no_track must read presence for the gate (issue #791)");
        let record = body.find("observe_presence_sample(").expect(
            "handle_no_track must record the manual-status verdict (review round 3, item 3)",
        );
        let verdict = body
            .find("presence_gate_decision(")
            .expect("handle_no_track must consult the presence/manual-status verdict (issue #791)");
        let decision = body
            .find("placeholder_write_decision(gate_blocked,")
            .expect("handle_no_track must decide from the combined verdict (issue #791)");
        let clear = body
            .find("clear_teams_status_message(")
            .expect("handle_no_track must keep the clear POST");
        assert!(
            read < verdict && record < verdict && verdict < decision && decision < clear,
            "the presence read, its recording, the verdict, and the combined decision \
             must all precede the clear POST: pre-fix the no-track path posted the \
             placeholder over a status message the user typed (issue #791)"
        );
        // The blocked path must announce the presence/manual-status reason with
        // the Graph sample, mirroring the paused path's once-per-episode emit.
        let suppress_arm_start = body
            .find("PlaceholderWrite::Suppress")
            .expect("the no-track clear must use the shared decision");
        let post_arm_start = body
            .find("PlaceholderWrite::Post")
            .expect("the no-track clear must keep the POST arm");
        let suppress_arm = &body[suppress_arm_start..post_arm_start];
        assert!(
            suppress_arm.contains("presence_reason"),
            "the no-track suppress arm must surface the presence/manual-status reason, \
             not only the rule verdict (issue #791)"
        );
    }

    /// Review round 2, item 3: `handle_no_track`'s early returns cannot post a
    /// clear, so they must retire the finished track's gate instead of leaving
    /// `get_sync_status` answering `presence_gated = true` forever.
    #[test]
    fn test_no_track_early_returns_retire_the_gate() {
        let prod = prod_source();
        let body = prod_fn_body(prod, "pub(crate) fn handle_no_track(");
        let token_site = body
            .find("teams_token_for_write(app, state)")
            .expect("handle_no_track must try to refresh its Teams token");
        let token_region = &body[token_site..(token_site + 700).min(body.len())];
        assert!(
            token_region.contains("no_track_gate_key(false)"),
            "the no-Teams-token early return must retire the gate (review round 2, item 3)"
        );
        let pause_site = body
            .find("c.teams.clear_on_pause")
            .expect("handle_no_track must honor clear_on_pause");
        let pause_region = &body[pause_site..(pause_site + 500).min(body.len())];
        assert!(
            pause_region.contains("no_track_gate_key(false)"),
            "with clear_on_pause off nothing is gated — retire the finished track's key \
             (review round 2, item 3)"
        );
        assert!(
            body.matches("no_track_gate_key(false)").count() >= 4,
            "all four no-write outcomes (two early returns, the dedup, and the posted \
             clear) retire the gate"
        );
    }

    #[test]
    fn test_fake_playback_source_drives_unchanged_status_write_path() {
        use crate::sources::tests::FakePlaybackSource;
        use crate::sources::{NowPlaying, PlaybackSource};

        // Script two responses — one real track, one empty — so the test
        // covers both arms of the trait's `Ok(Some(_))` and `Ok(None)`
        // surface that the poll loop dispatches to `process_track` and
        // `handle_no_track` respectively.
        let fake = FakePlaybackSource::new(crate::sources::PlaybackSourceId::Spotify);
        fake.push(Ok(Some(NowPlaying {
            title: "Bohemian Rhapsody".into(),
            artist: "Queen".into(),
            album: "A Night at the Opera".into(),
            album_art_url: "https://example.com/art.jpg".into(),
            is_playing: true,
            progress_ms: Some(42_000),
            duration_ms: 355_000,
        })));
        fake.push(Ok(None));

        let mut boxed: Box<dyn PlaybackSource> = Box::new(fake);

        // First poll: a real track. The trait surface maps 1:1 onto the
        // existing `spotify::NowPlaying { media: TrackInfo, ... }` shape,
        // so the downstream `status_track_key` /
        // `matching_track_rule_at_with_ctx` / `format_status_with_context`
        // consumers need no change.
        let np = boxed
            .poll()
            .expect("scripted Some track")
            .expect("scripted Some value");
        let track: crate::spotify::TrackInfo = (&np).into();
        let now = crate::spotify::NowPlaying {
            media: track.clone(),
            episode: None,
            context: crate::spotify::PlaybackContext::default(),
        };

        let cfg = Some(std::sync::Arc::new(crate::config::AppConfig::default()));

        // Change-detection path — the key the poll loop compares
        // `last_track_key` against before posting another status.
        let key = status_track_key(&now, &cfg);
        assert!(key.contains("Bohemian Rhapsody"));
        assert!(key.contains("Queen"));

        // Rules path — the same matcher `process_track` walks must accept
        // the flattened `TrackInfo` field-for-field.
        let rule_ctx = crate::polling::TrackRuleContext {
            artist: &now.media.artist,
            title: &now.media.title,
            ..Default::default()
        };
        // No rules configured by default; the helper returns None.
        let rules_cfg = &cfg.as_ref().unwrap().status_rules;
        assert!(
            matching_track_rule_at_with_ctx(rules_cfg, 0, 0, &rule_ctx).is_none(),
            "the empty default rules must not match this track"
        );

        // Formatter path — the same `format_status_with_context` the
        // live poll calls must produce the documented status text.
        let formatted = crate::spotify::format_status_with_context(
            &track,
            now.episode.as_ref(),
            &now.context,
            "🎵 {artist} - {track} 🎧",
        );
        assert_eq!(formatted, "🎵 Queen - Bohemian Rhapsody 🎧");

        // Second poll: `Ok(None)` — the no-track / clear arm. The trait
        // surface carries no body, so the downstream `handle_no_track`
        // branch sees an empty body — exactly what the pre-#862 path
        // produced when Spotify returned 204.
        let cleared = boxed.poll().expect("scripted None");
        assert!(
            cleared.is_none(),
            "the second scripted response must surface as Ok(None) so handle_no_track runs"
        );
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
    fn test_conditional_get_round_trip_is_preserved() {
        let spotify_src = include_str!("../sources/spotify.rs");
        let conditional = spotify_src.matches("last_etag.as_deref()").count();
        assert!(
            conditional >= 1,
            "expected SpotifySource::poll to pass last_etag.as_deref() to get_currently_playing so the conditional GET round-trip is preserved; found {}",
            conditional
        );
    }

    /// Issue #862 acceptance test: a fake `PlaybackSource` returning a
    /// track drives the unchanged status write path end-to-end. The trait
    /// surface (`NowPlaying`) and the rich `spotify::NowPlaying` must
    /// round-trip through every downstream consumer (`status_track_key`
    /// for change detection, `matching_track_rule_at_with_ctx` for rules,
    /// `format_status_with_context` for the Teams text) so the move from
    /// a direct `get_currently_playing` call to a `&mut dyn
    /// PlaybackSource` did not silently lose the contract `process_track`
    /// expects.
    ///
    /// The test uses the `FakePlaybackSource` test helper in
    /// `sources::tests`, which scripts a sequence of
    /// `Result<Option<NowPlaying>, SourceError>` answers. The fake is
    /// polled twice: the first answer carries a real track (proves the
    /// fresh-track path), the second carries `None` (proves the
    /// no-track / clear path). Both answers flow through the trait's
    /// `TrackInfo::from(&NowPlaying)` conversion so the existing
    /// `process_track` signature stays unchanged.

    #[test]
    fn test_single_top_level_get_currently_playing_match() {
        let source = include_str!("../sources/spotify.rs");
        let top_level = source
            .matches("crate::spotify::get_currently_playing(")
            .count();
        assert_eq!(
            top_level, 1,
            "SpotifySource::poll must own exactly one get_currently_playing call \
             site (issue #862 — the trait surface owns the conditional-GET \
             round-trip and the 401-retry is a re-call of the same site); found \
             {}",
            top_level
        );
    }

    /// Regression guard for issue #60: `start_polling`'s caller
    /// (`commands::start_syncing`) has already claimed `is_syncing`; a
    /// second compare-exchange here would always lose and surface
    /// "Polling is already running" after every fresh install.
    ///
    /// The body is isolated by brace counting from `start_polling`'s
    /// opening `{` (house style — never boundary anchors/log-line
    /// anchors, which silently drift and leave the assertion vacuous).

    #[test]
    fn test_no_track_presence_routes_pair_through_rule_backoff() {
        let prod = prod_source();
        let body = prod_fn_body(prod, "pub(crate) fn handle_no_track(");
        assert!(
            body.contains("rule_presence_backoff("),
            "handle_no_track must arm a matching rule's pair through rule_presence_backoff (issue #795)"
        );
        assert!(
            body.contains("clear_presence_session("),
            "handle_no_track must keep the clear for the no-pair case (issue #795)"
        );
    }

    /// Finding #635: the read-before-write policy, as a truth table.

    #[test]
    fn test_event_payload_shapes_are_pinned() {
        // Issue #762: the builders now return ts-rs-typed structs, so the
        // exact-JSON assertions below pin the wire shape THROUGH the type —
        // renaming or dropping a field changes the serialized JSON and fails
        // here before it can reach the UI as `undefined`.
        assert_eq!(
            serde_json::to_value(crate::polling::presence::presence_paused_payload(
                "\u{1F3B5} Paused"
            ))
            .expect("PresencePaused must serialize"),
            json!({ "status": "\u{1F3B5} Paused" }),
            "presence-paused is {{ status }} — the Dashboard renders that text"
        );
        assert_eq!(
            serde_json::to_value(crate::polling::presence::playback_state_changed_payload(
                false,
                "A - T | track | f"
            ))
            .expect("PlaybackStateChanged must serialize"),
            json!({ "is_playing": false, "track_key": "A - T | track | f" }),
            "playback-state-changed is {{ is_playing, track_key }} — the Dashboard and \
             the tray both consume it"
        );
        let value = serde_json::to_value(crate::polling::presence::playback_state_changed_payload(
            true, "k",
        ))
        .expect("PlaybackStateChanged must serialize");
        let obj = value.as_object().expect("an object payload");
        assert_eq!(
            obj.len(),
            2,
            "exactly is_playing + track_key: a renamed or extra field is a silent \
             break for S2/the tray (review round 2, item 5)"
        );
        assert!(obj.contains_key("is_playing") && obj.contains_key("track_key"));
        // The `error` envelope had NO shape assertion at all (issue #762):
        // a rename of `source`, `message` or `severity` compiled, passed the
        // suite, and reached the UI as `undefined`. The canonical builder
        // path (`emit_error` → `ErrorEvent`) now pins all three plus the
        // optional `recovery` discriminator.
        use crate::polling::{emit_error_with_recovery, ErrorRecovery};
        let recorder = CapturingEmitter::new();
        emit_error(
            &recorder,
            "spotify",
            "Retrying after a transient failure".to_string(),
            ErrorSeverity::Warning,
        );
        emit_error_with_recovery(
            &recorder,
            "teams",
            "Reconnect Teams in Settings".to_string(),
            ErrorSeverity::Error,
            Some(ErrorRecovery::ReconnectRequired),
        );
        let events = recorder
            .events
            .lock()
            .expect("capturing-emitter mutex must not be poisoned");
        let serialized = events
            .iter()
            .map(|(event, payload)| {
                (
                    event.as_str(),
                    serde_json::to_value(payload).expect("ErrorEvent must serialize"),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            serialized,
            vec![
                (
                    "error",
                    serde_json::json!({
                        "source": "spotify",
                        "message": "Retrying after a transient failure",
                        "severity": "warning"
                    })
                ),
                (
                    "error",
                    serde_json::json!({
                        "source": "teams",
                        "message": "Reconnect Teams in Settings",
                        "severity": "error",
                        "recovery": "reconnect_required"
                    })
                ),
            ],
            "error is {{ source, message, severity, recovery? }} — a renamed or \
             dropped field is a silent break for the Dashboard banner (issue #762)"
        );

        // ...and the emit sites must go through those builders.
        let prod = prod_source();
        let track_body = prod_fn_body(prod, "pub(crate) fn process_track(");
        assert!(
            track_body.contains("presence_paused_payload("),
            "the paused clear must emit through the pinned builder"
        );
        assert!(
            track_body.contains("playback_state_changed_payload("),
            "the playback-state arm must emit through the pinned builder"
        );
        assert!(
            !track_body.contains("\"presence-paused\", json!")
                && !track_body.contains("\"playback-state-changed\", json!"),
            "an inline json! at the emit site would bypass the pinned shape"
        );
    }
}
