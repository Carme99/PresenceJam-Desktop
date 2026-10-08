//! Token-refresh policy for the Spotify and Teams sessions (issue #754).
//!
//! Split from `polling::poll_once`: the CAS compare-and-swap refresh helpers,
//! the Spotify refresh-failure classifier, the proactive-refresh planner and
//! the Teams re-auth policy.

use crate::spotify::SpotifyApiError;
use crate::teams::TeamsApiError;
use crate::AppState;

#[derive(Debug)]
pub(crate) enum SpotifyRefreshFailure {
    Account(crate::sources::SourceError),
    Superseded,
}

pub(crate) fn classify_spotify_refresh_failure(
    error: &SpotifyApiError,
    refresh_superseded: bool,
    clear_succeeded: Option<bool>,
) -> SpotifyRefreshFailure {
    if refresh_superseded || clear_succeeded == Some(false) {
        SpotifyRefreshFailure::Superseded
    } else {
        SpotifyRefreshFailure::Account(crate::sources::SourceError::Auth(error.to_string()))
    }
}

pub(crate) enum CasOutcome<T, E> {
    Committed(T),
    Discarded {
        current: Option<T>,
    },
    /// Issue #798: the refresh failed, but the slot verdict still travels
    /// with the error — `replaced` is true when the slot no longer holds
    /// `pre_refresh_access_token`, i.e. a newer session was installed while
    /// this refresh was in flight and the error is about a superseded
    /// token. Dead-credential branches must only clear, persist and emit
    /// `reconnect-required` when `replaced` is false.
    RefreshFailed {
        error: E,
        replaced: bool,
    },
}

/// Generic over the refresh error type `E` so each caller keeps its
/// provider's typed error (`SpotifyApiError` / `TeamsApiError`) for the
/// re-auth policy, instead of a pre-stringified message.
#[cfg(test)]
pub(crate) fn cas_refresh_or_discard_unchecked<T, E, F, G>(
    label: &str,
    lock: &mut Option<T>,
    pre_refresh_access_token: &str,
    refresh_fn: F,
    access_token_of: G,
) -> CasOutcome<T, E>
where
    T: Clone,
    F: FnOnce() -> Result<T, E>,
    G: Fn(&T) -> &str,
{
    let result = refresh_fn();
    let replaced = lock.as_ref().map(access_token_of) != Some(pre_refresh_access_token);
    if replaced {
        log::warn!(
            "[POLLING] poll_once: cas_refresh_or_discard: {} state changed during refresh, discarding result",
            label
        );
    }
    match result {
        Err(e) => CasOutcome::RefreshFailed { error: e, replaced },
        Ok(new_tokens) => {
            if !replaced {
                *lock = Some(new_tokens.clone());
                CasOutcome::Committed(new_tokens)
            } else {
                let current = lock.clone();
                CasOutcome::Discarded { current }
            }
        }
    }
}

/// Refresh and commit through the recovery marker mutex. The network call
/// runs first; the marker mutex then owns the CAS compare and `Some` write, so
/// a fresh commit cannot race a recovery hydration or explicit clear.
pub(crate) fn cas_refresh_spotify<E, F>(
    state: &AppState,
    label: &str,
    pre_refresh_access_token: &str,
    refresh_fn: F,
) -> CasOutcome<crate::spotify::SpotifyTokens, E>
where
    F: FnOnce() -> Result<crate::spotify::SpotifyTokens, E>,
{
    let result = refresh_fn();
    match result {
        Err(error) => {
            let replaced = !state
                .tokens_load
                .provider_matches_spotify(&state.tokens, pre_refresh_access_token);
            if replaced {
                log::warn!(
                    "[POLLING] poll_once: {} state changed during refresh",
                    label
                );
            }
            CasOutcome::RefreshFailed { error, replaced }
        }
        Ok(new_tokens) => match state.tokens_load.cas_spotify(
            &state.tokens,
            pre_refresh_access_token,
            new_tokens.clone(),
        ) {
            crate::TokenCommitOutcome::Committed(_) => CasOutcome::Committed(new_tokens),
            crate::TokenCommitOutcome::Discarded(current) => {
                log::warn!(
                    "[POLLING] poll_once: {} state changed during refresh",
                    label
                );
                CasOutcome::Discarded { current }
            }
        },
    }
}

pub(crate) fn cas_refresh_teams<E, F>(
    state: &AppState,
    label: &str,
    pre_refresh_access_token: &str,
    refresh_fn: F,
) -> CasOutcome<crate::teams::TeamsTokens, E>
where
    F: FnOnce() -> Result<crate::teams::TeamsTokens, E>,
{
    let result = refresh_fn();
    match result {
        Err(error) => {
            let replaced = !state
                .tokens_load
                .provider_matches_teams(&state.tokens, pre_refresh_access_token);
            if replaced {
                log::warn!(
                    "[POLLING] poll_once: {} state changed during refresh",
                    label
                );
            }
            CasOutcome::RefreshFailed { error, replaced }
        }
        Ok(new_tokens) => match state.tokens_load.cas_teams(
            &state.tokens,
            pre_refresh_access_token,
            new_tokens.clone(),
        ) {
            crate::TokenCommitOutcome::Committed(_) => CasOutcome::Committed(new_tokens),
            crate::TokenCommitOutcome::Discarded(current) => {
                log::warn!(
                    "[POLLING] poll_once: {} state changed during refresh",
                    label
                );
                CasOutcome::Discarded { current }
            }
        },
    }
}

pub(crate) fn get_spotify_credentials(
    config: &Option<std::sync::Arc<crate::config::AppConfig>>,
) -> (String, String) {
    let client_id = config
        .as_ref()
        .map(|c| c.spotify.client_id.clone())
        .unwrap_or_default();
    let client_secret = crate::keychain::peek_spotify_client_secret().unwrap_or_default();
    (client_id, client_secret)
}

/// What the proactive Spotify refresh should do this iteration (issue #296).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SpotifyRefreshPlan {
    /// The access token is still inside its refresh window — use it as-is.
    Fresh,
    /// The access token expired and the client_id/secret needed to refresh it
    /// are both available.
    Refresh,
    /// The access token expired but the credential pair is unavailable
    /// (cold keychain cache: the startup prime failed, e.g. locked Secret
    /// Service / headless Linux / entry removed while running). Refreshing
    /// with an empty secret can only produce a 400 `invalid_client` — not
    /// `InvalidGrant` — so retrying is pointless; the user must re-auth or
    /// restore the keychain entry.
    CredentialsUnavailable,
}

/// Classify the proactive-refresh decision. Pure and total so the policy is
/// unit-testable without an `AppHandle`; the caller owns the side effects
/// (emitting events, persisting, the 5-strikes counter).
pub(crate) fn spotify_refresh_plan(
    token_expired: bool,
    client_id: &str,
    client_secret: &str,
) -> SpotifyRefreshPlan {
    if !token_expired {
        SpotifyRefreshPlan::Fresh
    } else if client_id.is_empty() || client_secret.is_empty() {
        SpotifyRefreshPlan::CredentialsUnavailable
    } else {
        SpotifyRefreshPlan::Refresh
    }
}

/// True when a failed Teams token refresh must force re-auth (issue #295).
/// The policy mirrors the Teams status-update classifier and the Spotify
/// sibling: only a genuinely dead credential — token-endpoint
/// `invalid_grant`, the interaction-gated 400s (`ReauthRequired`, issue
/// #787), or a 401 `ExpiredToken` — means re-auth. `Transient`
/// (network/5xx/unclassified-400/unparseable-body), `RateLimited`,
/// `Forbidden` and `Other(400, …)` are recoverable states that must keep
/// the session and retry later; a single dropped connection must not end
/// Teams sync.
pub(crate) fn teams_refresh_requires_reauth(e: &TeamsApiError) -> bool {
    matches!(
        e,
        TeamsApiError::InvalidGrant
            | TeamsApiError::ReauthRequired(_)
            | TeamsApiError::ExpiredToken(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    /// Issue #798: a failed refresh whose slot no longer holds the token it
    /// ran from must report `replaced: true`. Pre-fix the helper returned
    /// `RefreshFailed(e)` before inspecting the slot, so the error carried no
    /// verdict and every dead-credential branch cleared the newer session
    /// unconditionally. The swapped slot models a concurrent sign-in that
    /// landed while the (failed) refresh was in flight — the helper computes
    /// its verdict after `refresh_fn` returns, so a pre-call swap pins the
    /// same observable as a mid-call one.
    #[test]
    fn test_cas_failed_refresh_reports_mid_flight_replacement() {
        // `replaced: false` — slot untouched: the error is about the stored
        // session, so callers may clear.
        let mut slot: Option<String> = Some("old-access".to_string());
        let outcome: CasOutcome<String, &str> = cas_refresh_or_discard_unchecked(
            "test",
            &mut slot,
            "old-access",
            || Err("invalid_grant"),
            |t| t.as_str(),
        );
        assert!(
            matches!(
                outcome,
                CasOutcome::RefreshFailed {
                    replaced: false,
                    ..
                }
            ),
            "an unmatched failed refresh must report replaced: false"
        );

        // `replaced: true` — the slot moved while the refresh was in flight:
        // the error is about a superseded token, so callers must NOT clear.
        let mut slot: Option<String> = Some("new-access".to_string());
        let outcome: CasOutcome<String, &str> = cas_refresh_or_discard_unchecked(
            "test",
            &mut slot,
            "old-access",
            || Err("invalid_grant"),
            |t| t.as_str(),
        );
        assert!(
            matches!(outcome, CasOutcome::RefreshFailed { replaced: true, .. }),
            "a failed refresh whose slot moved mid-flight must report replaced: true"
        );
        assert_eq!(
            slot.as_deref(),
            Some("new-access"),
            "the helper must leave the newer session in the slot"
        );

        // Success path keeps its CAS verdict too: racing a replacement
        // discards the winner instead of clobbering it.
        let mut slot: Option<String> = Some("new-access".to_string());
        let outcome: CasOutcome<String, &str> = cas_refresh_or_discard_unchecked(
            "test",
            &mut slot,
            "old-access",
            || Ok("refreshed".to_string()),
            |t| t.as_str(),
        );
        assert!(
            matches!(
                outcome,
                CasOutcome::Discarded { current } if current.as_deref() == Some("new-access")
            ),
            "a successful refresh racing a replacement must discard"
        );
    }

    /// Why a source scan survives here (issue #778 allows exactly this
    /// shape): the invariant is the exact helper-definition and call-site COUNTS across the polling package; call-site placement is structural, so the counts are pinned at the source.
    #[test]
    fn test_cas_discard_block_is_single_source_of_truth() {
        // Post-split (#754): the CAS helpers live in refresh.rs and their call
        // sites in iteration.rs / write.rs -- scan the stripped production of
        // all three, so the counts still cover the whole package. Test
        // modules are stripped so their string literals don't inflate the
        // count.
        let prod_source = polling_prod();
        let helper_def = prod_source.matches("fn cas_refresh_spotify").count()
            + prod_source.matches("fn cas_refresh_teams").count();
        assert_eq!(helper_def, 2, "typed helpers defined {} times", helper_def);
        assert_eq!(
            prod_source.matches("fn cas_refresh_or_discard<").count(),
            0,
            "the removed generic helper must not remain in production code"
        );
        let helper_call_count = prod_source.matches("cas_refresh_spotify(").count()
            + prod_source.matches("cas_refresh_teams(").count();
        // 4 calls: Spotify proactive + 401-retry (run_inner), Teams
        // proactive (`teams_token_for_write`), and the single shared
        // Teams-write retry site (`teams_write_with_optional_refresh`,
        // issue #929 — the playing write, no-track clear, and paused
        // clear all funnel through this one helper so the three sites
        // cannot drift in their 401-handling shape).
        // The typed production helpers are the only refresh entry points;
        // the test-only unchecked helper is intentionally separate.
        assert_eq!(
            helper_call_count, 4,
            "typed refresh helpers called {} times in production; expected 4 \
             (Spotify proactive + 401-retry + Teams proactive + the shared \
             Teams-write retry helper `teams_write_with_optional_refresh` \
             that serves the playing write, no-track clear, and paused clear)",
            helper_call_count
        );
    }

    /// Issue #180 regression guard: a typed CAS caller must not retain a
    /// token-slot write guard while it invokes the helper and then persists.
    ///
    /// The old deadlock test modelled the pre-typed-helper shape by passing a
    /// `&mut Option<T>` guard into a CAS call. The production helpers now own
    /// the recovery-marker lock and compare/commit through `AppState`; callers
    /// pass only the access-token snapshot. A caller that reacquires
    /// `spotify_mut()` or `teams_mut()` around that call can reintroduce the
    /// write→read lock cycle when `persist_tokens` runs afterwards. This
    /// structural guard covers every current typed CAS caller and fails on
    /// either provider's write accessor, without inventing a fake runtime
    /// lock shape that production no longer has.
    /// Why a source scan survives here (issue #778 allows exactly this
    /// shape): the invariant is no `spotify_mut()`/`teams_mut()` guard retained around any typed CAS call; the callers run the live refresh against real tokens, so the guard discipline is pinned at the source.
    #[test]
    fn test_typed_cas_callers_do_not_retain_slot_write_guard() {
        // Post-split (#754): the single-file call sites now live in the
        // named sibling modules -- scan each owner's stripped production.
        let iteration = stripped(include_str!("iteration.rs"));
        let write_prod = stripped(include_str!("write.rs"));
        let cases = [
            (
                "polling/iteration.rs::run_inner",
                iteration,
                "fn run_inner(",
                "cas_refresh_spotify(",
            ),
            (
                "polling/write.rs::teams_token_for_write",
                write_prod,
                "fn teams_token_for_write(",
                "cas_refresh_teams(",
            ),
            (
                // Issue #1117: the production wrapper delegates to the
                // seam-bearing entry point, which is where the emitter and
                // the injected seams are bound and which in turn owns the
                // core call. Both halves stay in this guard; the total is
                // unchanged (the wrapper still makes exactly one delegated
                // call).
                "polling/write.rs::teams_write_with_refresh_seams",
                write_prod,
                "fn teams_write_with_refresh_seams",
                "teams_write_with_refresh_fn(",
            ),
            (
                "polling/write.rs::teams_write_with_refresh_fn",
                write_prod,
                "fn teams_write_with_refresh_fn",
                "cas_refresh_teams(",
            ),
            (
                "commands/onboarding.rs::spotify_session_verdict",
                include_str!("../commands/onboarding.rs"),
                "fn spotify_session_verdict(",
                "cas_refresh_spotify(",
            ),
            (
                "commands/onboarding.rs::teams_session_verdict",
                include_str!("../commands/onboarding.rs"),
                "fn teams_session_verdict(",
                "cas_refresh_teams(",
            ),
            (
                // Issue #928 moved this CAS into the command's blocking
                // impl so the command can offload it to the blocking pool;
                // this points at that impl, matching the Teams twin
                // (`refresh_teams_impl`) directly below.
                "commands/spotify_auth.rs::refresh_spotify_impl",
                include_str!("../commands/spotify_auth.rs"),
                "fn refresh_spotify_impl(",
                "crate::polling::cas_refresh_spotify(",
            ),
            (
                "commands/teams_auth.rs::refresh_teams_impl",
                include_str!("../commands/teams_auth.rs"),
                "fn refresh_teams_impl(",
                "cas_refresh_teams(",
            ),
        ];

        let mut cas_calls = 0;
        for (caller, source, function, helper) in cases {
            let body = prod_fn_body(source, function);
            let calls = body.matches(helper).count();
            assert!(
                calls > 0,
                "{caller} must invoke {helper}; the structural guard lost its callsite"
            );
            cas_calls += calls;
            for accessor in ["spotify_mut", "teams_mut"] {
                assert!(
                    !body.contains(accessor),
                    "{caller} must not acquire {accessor} around {helper}; \
                     persist_tokens would re-lock the same RwLock while the guard is live"
                );
            }
        }
        // Issue #929 folded the three Teams write sites (playing write in
        // `process_track`, no-track clear in `handle_no_track`, paused clear in
        // `process_track`) into the shared helper
        // `teams_write_with_optional_refresh`, so the helper is now the
        // single typed-CAS caller for Teams writes. The playing write and
        // no-track clear are no longer direct CAS callers; the helper owns
        // the guard invariant instead.
        // Issue #929 rework: the helper is split into a thin production
        // wrapper (`teams_write_with_optional_refresh`), a seam-bearing entry
        // point (`teams_write_with_refresh_seams`) and a generic core
        // (`teams_write_with_refresh_fn`) so the test module can drive the
        // retry behaviour without a network round-trip, a keychain read or a
        // `tokens.json` write. The wrapper delegates to the seam entry point,
        // which delegates to the core; the core holds the single
        // `cas_refresh_teams(` call. Both delegating halves stay in this guard
        // so neither can drift: the seam entry catches a future revert that
        // inlines the core (it would lose the `teams_write_with_refresh_fn(`
        // substring), and the core entry catches a future removal of the CAS
        // call. Issue #1117 re-anchored the wrapper entry onto the seam entry
        // rather than dropping it, so the total is UNCHANGED at 9 — one
        // delegating caller plus one core, exactly as before.
        assert_eq!(
            cas_calls, 9,
            "expected all nine current typed CAS callsites; a new caller must be \
             added to this guard before it can hold a slot write guard \
             (issue #929 — the three Teams write sites now share \
             `teams_write_with_optional_refresh`, which delegates through \
             `teams_write_with_refresh_seams` to `teams_write_with_refresh_fn`; \
             both stay in the guard; `process_track` and `handle_no_track` no \
             longer call `cas_refresh_teams` directly)"
        );
    }

    /// Issue #180 regression guard: the CAS helper must never persist tokens
    /// itself. Pre-fix it called `token_io::persist_tokens` while the
    /// caller's write guard was still alive (write→read on the same
    /// parking_lot RwLock from the same thread parks forever), so every
    /// successful refresh self-deadlocked. The typed helpers now own the
    /// compare/commit lock; call sites persist only after they return. If a
    /// future contributor moves a persist call back inside a helper body, the
    /// deadlock returns and this guard fails.
    /// Why a source scan survives here (issue #778 allows exactly this
    /// shape): the invariant is the ABSENCE of `persist_tokens(` inside both helper bodies plus persistence at the call sites; the helpers run the live refresh, so the split is pinned at the source.
    #[test]
    fn test_cas_helper_body_has_no_persist_and_call_sites_persist() {
        let prod_source = prod_source();
        let write_prod = stripped(include_str!("write.rs"));

        // Isolate both typed helper bodies by brace counting from each
        // function's opening `{` (house style — never boundary anchors).
        for (helper, signature) in [
            ("cas_refresh_spotify", "fn cas_refresh_spotify<E, F>("),
            ("cas_refresh_teams", "fn cas_refresh_teams<E, F>("),
        ] {
            let body = prod_fn_body(prod_source, signature);
            assert!(
                !body.contains("persist_tokens("),
                "{helper} must not persist tokens inside its body (issue #180). Body:\n{}",
                body
            );
        }
        // All persistence must happen at the call sites, after the CAS call
        // returns (guard provably dropped). Pre-#929 the breakdown was:
        // three invalid_grant/dead-token clear paths (Spotify proactive,
        // Spotify 401-retry, Teams in `teams_token_for_write`), four
        // refresh-success call sites (Spotify proactive, Spotify 401-retry,
        // Teams proactive, Teams write-retry for issues #367/#428), the
        // playing write's reactive dead-credential clear, and the
        // no-track clear's two reactive persist sites — ten in total.
        //
        // Issue #929 folds the three Teams write sites (playing write,
        // no-track clear, paused clear) into `teams_write_with_optional_refresh`,
        // so the Teams-write persist and dead-credential clear persist
        // each collapse from N inline sites (2 before: process_track +
        // handle_no_track) into a single site inside the helper. Total
        // drops from 10 to 8: the four Spotify sites + Teams proactive
        // clear + Teams proactive success + the two sites inside the
        // shared helper (refresh-success persist + dead-credential clear
        // persist).
        //
        // Issue #929 rework: the helper now takes an *injected* persist
        // closure, so both call sites inside `teams_write_with_refresh_fn`
        // and `handle_teams_refresh_failure` route through ONE closure
        // whose body lives in the wrapper `teams_write_with_optional_refresh`.
        // The literal `token_io::persist_tokens(` count drops by one (the
        // two inline sites collapse into one closure body), so the lower
        // bound is 7 — the four Spotify sites + two Teams proactive sites
        // + one closure-injected site in the wrapper. The companion
        // assertion below checks the closure itself is wired, so a future
        // regression that drops the closure for, say, `idle persist`
        // cannot silently slip past the count check alone.
        // Post-split (#754): the persist sites moved with their call sites
        // (Spotify sites to iteration.rs, Teams sites to write.rs) -- count
        // the stripped production of all three so the lower bound still
        // covers the whole package.
        let persist_count = polling_prod().matches("token_io::persist_tokens(").count();
        assert!(
            persist_count >= 7,
            "expected at least 7 persist_tokens call sites in production (4 Spotify \
             sites + 2 Teams proactive sites + 1 closure-injected site inside \
             `teams_write_with_optional_refresh` after issue #929 rework collapsed the \
             helper's two persist sites into the wrapper's persist closure); found \
             {}. If a call-site persist is removed, refreshed/cleared tokens stop being \
             flushed to disk; if one is added inside a CAS helper, the #180 \
             self-deadlock returns.",
            persist_count
        );

        // Companion guard for the issue #929 rework closure shape: the
        // wrapper body must inject `token_io::persist_tokens(s, app)` as
        // a closure OUTSIDE any string literal so the helper stays
        // seam-driven. The naive `contains(...)` check the original
        // rework shipped was defeated with one line:
        //     let _decoy: &str = "|s| token_io::persist_tokens(s, app)";
        // — a reviewer dropped it in while the real closure was
        // `|_| Ok(())`, both checks passed, and the runtime silently
        // stopped flushing refreshed/cleared tokens to disk. This guard
        // builds the candidate by stripping `//`/`///` line comments AND
        // the contents of every `"…"` string literal; the decoy line has
        // quotes and is dropped, the real closure
        // `|s| token_io::persist_tokens(s, app),` survives. A future
        // regression that drops the closure entirely (or relocates it
        // inside a string) fails this test.
        let wrapper_body = prod_fn_body(write_prod, "fn teams_write_with_optional_refresh<F, Rt>(");
        let closure_pattern = "|s| token_io::persist_tokens(s, app)";
        let mut candidate = String::new();
        let mut in_string = false;
        for line in wrapper_body.lines() {
            // Drop line-level Rust comments (`//`, `///`, `//!`, `    // …`).
            // A future contributor could put the real closure on a comment
            // line and skip the guard; this filter blocks that.
            if line.trim_start().starts_with("//") {
                continue;
            }
            // Walk the line char-by-char, suppressing string-literal
            // contents so a decoy like
            // `let _decoy: &str = "<pattern>";` contributes nothing to
            // the candidate. Escape sequences (`\"`, `\\`, …) are honoured
            // — the `\X` form skips the next char without toggling the
            // string state. Multi-line strings are handled: the `in_string`
            // state persists across lines, and a closing `"` on a later
            // line ends the suppression. The wrapper body never uses raw
            // strings (`r"…"`, `r#"…"#`); a future contributor adding one
            // is a separate concern that this guard does not pretend to
            // handle.
            let mut stripped = String::new();
            let mut chars = line.chars().peekable();
            while let Some(c) = chars.next() {
                if in_string {
                    if c == '\\' {
                        // Skip the escaped char (the `\"` form, etc.).
                        chars.next();
                    } else if c == '"' {
                        in_string = false;
                    }
                    continue;
                }
                if c == '"' {
                    in_string = true;
                    continue;
                }
                stripped.push(c);
            }
            candidate.push_str(&stripped);
            candidate.push('\n');
        }
        assert!(
            candidate.contains(closure_pattern),
            "the wrapper `teams_write_with_optional_refresh` must inject the \
             production persist as a closure (`{closure_pattern}`) OUTSIDE any \
             string literal, so a future contributor cannot defeat the guard \
             with `let _decoy: &str = \"<pattern>\";`. The wrapper body had no \
             occurrence outside string contexts — either the closure was removed, \
             or every match is inside a string. Wrapper body:\n{}",
            wrapper_body
        );
    }

    /// Issue #295 regression guard: only a genuinely dead Teams credential
    /// forces re-auth. Pre-fix the `RefreshFailed` arm matched every error
    /// unconditionally, so a single 5xx/dropped connection discarded the
    /// session and drove a full device-code re-auth.
    #[test]
    fn test_teams_refresh_reauth_policy_is_dead_token_only() {
        assert!(
            teams_refresh_requires_reauth(&TeamsApiError::InvalidGrant),
            "a dead refresh token (invalid_grant) must force re-auth"
        );
        // Issue #787: the interaction-gated 400s end the session once via
        // `ReauthRequired` — no per-poll retry loop.
        assert!(
            teams_refresh_requires_reauth(&TeamsApiError::ReauthRequired(
                "interaction_required - user interaction needed".to_string()
            )),
            "an interaction-gated 400 must force re-auth"
        );
        assert!(
            teams_refresh_requires_reauth(&TeamsApiError::ExpiredToken(401)),
            "a rejected access token (401) must force re-auth"
        );
        for transient in [
            TeamsApiError::Transient("Failed to send refresh token request: boom".to_string()),
            TeamsApiError::RateLimited(Some(30)),
            TeamsApiError::RateLimited(None),
            TeamsApiError::Other(400, "invalid_client".to_string()),
            TeamsApiError::Forbidden(403, "denied".to_string()),
        ] {
            assert!(
                !teams_refresh_requires_reauth(&transient),
                "transient Teams refresh failure must keep the session: {:?}",
                transient
            );
        }
    }

    /// Issue #296 regression guard: an expired access token with a cold
    /// keychain cache (empty secret) must be classified as
    /// `CredentialsUnavailable` rather than attempted — and the guard must
    /// not fire when the token is still fresh or both credentials are
    /// present.
    #[test]
    fn test_spotify_refresh_plan_requires_non_empty_credentials() {
        assert_eq!(
            spotify_refresh_plan(true, "client-id", ""),
            SpotifyRefreshPlan::CredentialsUnavailable,
            "an empty client_secret (cold keychain cache) must not be attempted"
        );
        assert_eq!(
            spotify_refresh_plan(true, "", "client-secret"),
            SpotifyRefreshPlan::CredentialsUnavailable,
            "an empty client_id must not be attempted"
        );
        assert_eq!(
            spotify_refresh_plan(true, "client-id", "client-secret"),
            SpotifyRefreshPlan::Refresh,
            "expired token + both credentials present must refresh"
        );
        assert_eq!(
            spotify_refresh_plan(false, "", ""),
            SpotifyRefreshPlan::Fresh,
            "a token still inside its window is used as-is regardless of credentials"
        );
    }

    #[test]
    fn polling_dead_refresh_clear_uses_replacement_safe_authority() {
        let state = AppState::new();
        let spotify = |access: &str| crate::spotify::SpotifyTokens {
            access_token: access.to_string(),
            refresh_token: "spotify-refresh".to_string(),
            expires_at: Utc::now() + chrono::Duration::hours(1),
        };
        let teams = |access: &str| crate::teams::TeamsTokens {
            access_token: access.to_string(),
            refresh_token: Some("teams-refresh".to_string()),
            expires_at: Utc::now() + chrono::Duration::hours(1),
        };

        state
            .tokens_load
            .commit_spotify(&state.tokens, spotify("old"));
        state
            .tokens_load
            .commit_spotify(&state.tokens, spotify("new"));
        assert!(!state
            .tokens_load
            .clear_spotify_if_current(&state.tokens, "old"));
        assert_eq!(
            state
                .tokens
                .spotify()
                .as_ref()
                .map(|tokens| tokens.access_token.as_str()),
            Some("new")
        );
        assert!(state
            .tokens_load
            .clear_spotify_if_current(&state.tokens, "new"));
        assert!(state.tokens.spotify().is_none());

        state.tokens_load.commit_teams(&state.tokens, teams("old"));
        state.tokens_load.commit_teams(&state.tokens, teams("new"));
        assert!(!state
            .tokens_load
            .clear_teams_if_current(&state.tokens, "old"));
        assert_eq!(
            state
                .tokens
                .teams()
                .as_ref()
                .map(|tokens| tokens.access_token.as_str()),
            Some("new")
        );
        assert!(state
            .tokens_load
            .clear_teams_if_current(&state.tokens, "new"));
        assert!(state.tokens.teams().is_none());
    }

    /// Production source with the test module stripped -- the shared preamble
    /// for the structural guards below.
    fn prod_source() -> &'static str {
        include_str!("refresh.rs")
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("refresh.rs has no #[cfg(test)] mod tests block")
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

    /// Stripped production of the polling package post-split (#754):
    /// refresh.rs holds the CAS helpers, iteration.rs and write.rs their call
    /// sites. Test modules are excluded so their string literals cannot
    /// inflate the structural counts below.
    fn polling_prod() -> String {
        [
            stripped(include_str!("refresh.rs")).to_string(),
            stripped(include_str!("iteration.rs")).to_string(),
            stripped(include_str!("write.rs")).to_string(),
        ]
        .concat()
    }

    /// Production part of a sibling module's source: everything above its
    /// test module (the same strip [`prod_source`] applies to this file).
    fn stripped(source: &str) -> &str {
        source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("sibling module source has no test module")
    }
}
