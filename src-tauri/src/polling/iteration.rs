//! Iteration driver lives here per issue #754 — single source of truth for one polling iteration.
//!
//! Issue #72 documented three near-duplicate API-call branches in the
//! old `polling_loop` that had already drifted:
//!
//! 1. The 401-retry's no-track branch incremented `consecutive_pauses`
//!    in a different order than the main no-track path.
//! 2. The final-failure branch emitted a user-visible `error` event that
//!    the 401-retry path silently skipped.
//! 3. The CAS-discard re-read dance appeared three times (Spotify
//!    proactive refresh, Spotify 401-retry refresh, Teams refresh in
//!    `process_track`) with slightly different log messages.
//!
//! All three collapse to a single function here. See the regression
//! tests at the bottom of this file for invariants.
//! Per #754 the driver (`PollIteration`, `RunMode`, `run`, `run_oneshot`,
//! `run_inner`, `record_no_track_outcome`, `not_modified_iteration`) lives
//! in this module; the clock/write/refresh/gate/timing helpers live in their own modules.

use std::sync::mpsc;
use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use serde_json::json;
use tauri::{AppHandle, Emitter};

use crate::spotify::{is_token_expired, refresh_spotify_token, SpotifyApiError};
use crate::teams::is_token_expired as is_teams_token_expired;
use crate::token_io;
use crate::AppState;

use super::{emit_error, ErrorSeverity};

/// What the driver should do after this iteration.
pub(crate) enum PollIteration {
    Sleep { seconds: u64 },
    Break,
}

/// Execution mode for one poll iteration. `Loop` is the polling-thread
/// path (parking sleeps); `OneShot` is an explicit refresh that must
/// never park a thread on a sleep — every parking sleep site (the
/// `interruptible_sleep` error/no-token/backoff tails) returns `Break`
/// immediately after its usual event/log. Success-path `Sleep` values
/// flow through unchanged but `run_oneshot` discards them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunMode {
    Loop,
    OneShot,
}

pub(crate) fn run(
    state: &Arc<AppState>,
    app: &AppHandle,
    stop_rx: &mpsc::Receiver<()>,
    ps: &mut super::clocks::PollState,
    mode: RunMode,
) -> PollIteration {
    run_inner(state, app, stop_rx, ps, mode)
}

/// One-shot entry: a manual refresh (tray clear, `refresh_status`) runs one
/// iteration against the SAME write-decision clocks the polling loop uses
/// (see [`WriteClocks`], finding PollCore#4 / issue #572) instead of fresh
/// per-call locals. Pre-fix, the fresh `last_availability_arm = None` made
/// `should_rearm_availability` true on every manual refresh — an extra
/// `setPresence` POST even seconds after the loop armed — and the fresh
/// `last_track_key`/`last_posted_status`/`last_teams_update` made the write
/// path force-POST a status the #384 identical-write guard would have
/// skipped. Only the write-decision clocks are shared: `consecutive_pauses`,
/// `last_etag` and `first_iteration` stay local, because a one-shot is not a
/// polling thread (an idle one-shot must stay silent, issue #373) and its
/// verdict is discarded.
///
/// `_tx` is a live binding (not `let _`), so the channel stays connected for
/// the whole call: the top stop-check treats `Disconnected` as Break, and a
/// dropped sender here would make every one-shot a silent no-op. Never
/// collapse this to `let _`. Never parks either: `RunMode::OneShot` turns
/// every parking sleep site into an immediate `Break`.
///
/// S9 (issue #677): a one-shot is an iteration, so it respects an active snooze
/// exactly like the driver's loop — decided HERE, before the shared write clocks
/// are loaded, so a refresh during a snooze issues no Spotify/Graph request and
/// moves no clock. Without this, `refresh_status`, the tray's post-action
/// catch-up and the CLI's `--sync-once` were three silent bypasses of the
/// feature's "no work while snoozed" promise.
///
/// Issue #793: the same holds for the quiet-hours pause (`pause_polling`) —
/// decided HERE, after the snooze gate and before the clocks are loaded, so a
/// refresh inside a pausing window issues no Spotify/Graph request and moves
/// no clock. The CLI's `--sync-once` deliberately gets NO override flag: a
/// documented cron/headless path that silently ignored the pause would break
/// the `pausePollingHint` promise, so all three entry points honour the pause.
pub(crate) fn run_oneshot(state: &Arc<AppState>, app: &AppHandle) {
    // S9 (issue #677): the snooze gate, BEFORE the shared clocks are loaded.
    // Every entry point to an iteration has to honour it, and this is the second
    // one (the driver's loop is the first) — see the fn docs. The expiry case
    // falls through to a normal iteration so an explicit refresh after the
    // deadline still refreshes.
    let gate = {
        // Scoped: the config read guard must not outlive the decision.
        let config = state.config.get();
        super::gate::snooze_gate(&state.session, &config)
    };
    match gate {
        super::gate::SnoozeGate::Skipped(_) => {
            log::info!(
                "[POLLING] run_oneshot: skipped — a snooze is active, so this refresh performed no request"
            );
            return;
        }
        super::gate::SnoozeGate::Expired => super::gate::clear_snooze_if_expired(state),
        super::gate::SnoozeGate::Inactive => {}
    }
    // Issue #793: the quiet-hours pause, AFTER the snooze gate and BEFORE the
    // shared clocks are loaded. Same shape as the driver's loop (S4): a refresh
    // inside a pausing window issues no Spotify/Graph request and moves no
    // clock. `quiet_pause_iteration` already emitted the pause-transition line,
    // exactly like in the loop — this line only explains why the manual
    // refresh did nothing, mirroring the snooze skip message above.
    let paused = {
        // Scoped: the config read guard must not outlive the decision.
        let config = state.config.get();
        super::gate::quiet_pause_iteration(&state.session, &config)
    };
    if paused.is_some() {
        log::info!(
            "[POLLING] run_oneshot: skipped — a quiet-hours pause is active, so this refresh performed no request"
        );
        return;
    }
    // `_tx` is a live binding (not `let _`): the top stop-check treats a
    // `Disconnected` receiver as Break, so a dropped sender would make every
    // one-shot a silent no-op. Never collapse this to `let _`.
    let (_tx, rx) = mpsc::channel::<()>();
    // Issue #862: a one-shot constructs a fresh source — the etag is
    // empty (no persistent source for a one-shot iteration), so the
    // first read is unconditional. The `last_source_kind` field
    // exists for the looping path; a one-shot always starts at
    // `Default::default()` (Auto) and the kind-change check below
    // rebuilds the source on every iteration (cheap — same `Auto`
    // branch). The behaviour is identical to the loop's first
    // iteration.
    // Issue #373 does NOT apply here: a one-shot is an explicit refresh,
    // not a fresh polling thread — an idle one-shot must stay silent
    // instead of POSTing a placeholder on every manual refresh.
    let kind: crate::sources::PlaybackSourceKind = state
        .config
        .get()
        .as_ref()
        .map(|c| c.playback.source)
        .unwrap_or_default();
    let mut ps = super::clocks::PollState {
        clocks: super::clocks::load_write_clocks(&state.session),
        consecutive_pauses: 0,
        transient_failure_count: 0,
        consecutive_network_failures: 0,
        playback_source: crate::sources::build_source(
            state
                .config
                .get()
                .as_ref()
                .map(|c| c.playback.source)
                .unwrap_or_default(),
        )
        .unwrap_or_else(|| Box::new(crate::sources::spotify::SpotifySource::new())),
        last_source_kind: kind,
        first_iteration: false,
    };
    let _ = run_inner(state, app, &rx, &mut ps, RunMode::OneShot);
    super::clocks::store_write_clocks(&state.session, &ps.clocks);
}

fn run_inner(
    state: &Arc<AppState>,
    app: &AppHandle,
    stop_rx: &mpsc::Receiver<()>,
    ps: &mut super::clocks::PollState,
    mode: RunMode,
) -> PollIteration {
    // Issue #754: `PollState` bundles the 17 positional `&mut` out-params;
    // destructured here so the body below stays verbatim.
    let super::clocks::PollState {
        clocks:
            super::clocks::WriteClocks {
                ref mut last_track_key,
                ref mut last_teams_update,
                ref mut last_posted_placeholder,
                ref mut suppressed_placeholder,
                ref mut gated_track_key,
                ref mut last_availability_arm,
                ref mut armed_presence,
                ref mut last_posted_status,
                ref mut last_gate_check,
                ref mut last_idle_verdict,
                ref mut force_resume_write,
                generation: _,
            },
        ref mut consecutive_pauses,
        ref mut transient_failure_count,
        ref mut consecutive_network_failures,
        ref mut playback_source,
        ref mut last_source_kind,
        ref mut first_iteration,
    } = ps;

    match stop_rx.recv_timeout(std::time::Duration::ZERO) {
        Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            log::info!("[POLLING] poll_once: stop signal at top, breaking");
            return PollIteration::Break;
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
    }

    let config = state.config.snapshot();
    log::debug!("[POLLING] poll_once: config loaded");

    // Issue #866: the iteration-head expiry tick for the preferred-presence
    // session. Runs BEFORE any Spotify/Graph round-trip, so a session whose
    // `expires_at` has lapsed is cleared before a rule or snooze transition
    // can resurrect it. The Teams token read here is the same source the
    // rest of the iteration uses; on a missing/expired token the helper is a
    // no-op (the session record is reset regardless so a stale arm does not
    // outlive the run).
    if let Some(teams_tokens) = state.tokens.teams().clone() {
        if !is_teams_token_expired(&teams_tokens) {
            let _ = super::presence::clear_expired_preferred_presence(
                &state.session,
                app,
                &teams_tokens.access_token,
                Utc::now(),
            );
        }
    }
    // Issue #870: the iteration-head expiry tick for the manual status.
    // Mirrors the preferred-presence tick above: the local record clears
    // when the user-picked expiry lapses, so the Dashboard composer stops
    // claiming a manual status is armed. The Graph side already cleared
    // itself via the expiry we POSTed in `set_manual_status_inner`.
    crate::commands::status::tick_manual_status_expiry(app, Utc::now());

    let spotify_tokens = state.tokens.spotify().clone();
    log::debug!(
        "[POLLING] poll_once: spotify_tokens: {}",
        if spotify_tokens.is_some() {
            "Some"
        } else {
            "None"
        }
    );

    let spotify_tokens = match spotify_tokens {
        Some(t) => {
            log::debug!("[POLLING] poll_once: using existing Spotify tokens");
            t
        }
        None => {
            log::warn!("[POLLING] poll_once: No Spotify tokens available, waiting...");
            return super::timing::interruptible_sleep(
                stop_rx,
                super::timing::with_jitter(super::timing::ERROR_RETRY_INTERVAL_SECONDS),
                "no-token sleep",
                mode,
            );
        }
    };

    let token_expired = is_token_expired(&spotify_tokens);
    log::debug!("[POLLING] poll_once: token_expired={}", token_expired);

    let (client_id, client_secret) = super::refresh::get_spotify_credentials(&config);
    // Issue #296: `get_spotify_credentials` reads the secret through the
    // cache-only `keychain::peek_spotify_client_secret()`, so it is empty
    // whenever the startup prime failed (locked Secret Service, headless
    // Linux, entry removed while running). Refreshing with an empty secret
    // can only produce a 400 `invalid_client` — not `InvalidGrant` — so the
    // Err arm below would emit a Warning and sleep *before* the 5-strikes
    // counter, looping forever with no user-visible cause. Classify the
    // decision up front (pure helper, mirroring the 401 path's guard) and
    // route the unavailable case to an actionable reconnect.
    let refresh_plan =
        super::refresh::spotify_refresh_plan(token_expired, &client_id, &client_secret);
    let spotify_tokens = if refresh_plan == super::refresh::SpotifyRefreshPlan::Refresh {
        log::info!("[POLLING] poll_once: Spotify token expired, refreshing...");
        log::info!(
            "[POLLING] poll_once: refreshing with client_id.len={}",
            client_id.len()
        );

        let pre_refresh_access_token = spotify_tokens.access_token.clone();
        match refresh_spotify_token(&spotify_tokens, &client_id, &client_secret) {
            Ok(new_tokens) => {
                log::info!("[POLLING] poll_once: token refresh SUCCESS");
                let cas_outcome = super::refresh::cas_refresh_spotify(
                    state,
                    "spotify",
                    &pre_refresh_access_token,
                    || Ok::<_, SpotifyApiError>(new_tokens.clone()),
                );
                // Issue #180: `cas_refresh_spotify` owns the recovery-marker
                // lock and commits through `AppState`; this caller holds no
                // token-slot write guard. Persist only after the helper returns.
                if matches!(&cas_outcome, super::refresh::CasOutcome::Committed(_)) {
                    if let Err(e) = token_io::persist_tokens(state, app) {
                        log::warn!(
                            "[POLLING] poll_once: failed to persist refreshed spotify tokens: {}",
                            e
                        );
                    }
                }
                match cas_outcome {
                    super::refresh::CasOutcome::Committed(_) => new_tokens,
                    super::refresh::CasOutcome::Discarded { current } => match current {
                        Some(t) => t,
                        None => {
                            log::info!(
                                "[POLLING] poll_once: state cleared during refresh, waiting and re-polling"
                            );
                            return super::timing::interruptible_sleep(
                                stop_rx,
                                super::timing::with_jitter(
                                    super::timing::ERROR_RETRY_INTERVAL_SECONDS,
                                ),
                                "CAS-fail sleep",
                                mode,
                            );
                        }
                    },
                    super::refresh::CasOutcome::RefreshFailed { .. } => {
                        unreachable!("inner refresh_fn is Ok-wrapping")
                    }
                }
            }
            Err(e) => {
                log::error!(
                    "[POLLING] poll_once: Failed to refresh Spotify token: {}",
                    e
                );
                // Issue #160: `invalid_grant` means the refresh token is dead
                // (documented 6-month lifetime, or revoked). Discard it and
                // trigger re-auth instead of retrying forever. The clear helper
                // releases its write guard before this branch persists.
                // Issue #798: only clear when the slot still holds the token
                // this refresh ran from — a mid-flight replacement means the
                // error is about a superseded token and the newer session is
                // alive.
                if matches!(e, SpotifyApiError::InvalidGrant) {
                    let cleared = state
                        .tokens_load
                        .clear_spotify_if_current(&state.tokens, &pre_refresh_access_token);
                    if !cleared {
                        log::warn!("[POLLING] poll_once: Spotify clear superseded by a newer session; no-op");
                        return PollIteration::Sleep {
                            seconds: super::timing::clamp_poll_interval(
                                super::timing::config_default_interval(&config),
                                &config,
                            ),
                        };
                    } else {
                        log::error!("[POLLING] poll_once: Spotify refresh token invalid (invalid_grant), discarding tokens and requiring reconnect");
                        if let Err(persist_err) = token_io::persist_tokens(state, app) {
                            log::warn!(
                                "[POLLING] poll_once: failed to persist cleared Spotify tokens: {}",
                                persist_err
                            );
                        }
                        let _ = app.emit("spotify-reconnect-required", json!(null));
                        let _ = app.emit("reconnect-required", json!(null));
                        return super::timing::interruptible_sleep(
                            stop_rx,
                            super::timing::with_jitter(super::timing::ERROR_RETRY_INTERVAL_SECONDS),
                            "invalid-grant sleep",
                            mode,
                        );
                    }
                }
                emit_error(
                    app,
                    "spotify",
                    format!("Token refresh failed: {}", e),
                    ErrorSeverity::Warning,
                );
                return super::timing::interruptible_sleep(
                    stop_rx,
                    super::timing::with_jitter(super::timing::ERROR_RETRY_INTERVAL_SECONDS),
                    "error retry sleep",
                    mode,
                );
            }
        }
    } else if refresh_plan == super::refresh::SpotifyRefreshPlan::CredentialsUnavailable {
        // Issue #296: the access token is expired but the credential pair
        // needed to refresh it is unavailable. Retrying cannot fix this, so
        // count it toward the existing 5-strikes escape and surface the same
        // actionable reconnect pair as `invalid_grant`.
        //
        // The tokens are deliberately NOT cleared here (the `invalid_grant`
        // path above does): the refresh token itself is still valid, the
        // user's fix is to restore the keychain entry, and keeping it lets
        // this branch be re-entered so `transient_failure_count` can actually
        // reach its threshold — clearing would make the top-of-iteration
        // no-token guard swallow every later iteration and the escape
        // unreachable.
        log::error!(
            "[POLLING] poll_once: Spotify token expired but credentials unavailable (client_id empty: {}, client_secret empty: {}), requiring reconnect",
            client_id.is_empty(),
            client_secret.is_empty()
        );
        *transient_failure_count = transient_failure_count.saturating_add(1);
        // Emit on the first detection only. `spotify-reconnect-required`
        // makes `+layout.svelte` start a real OAuth flow, so repeating it
        // every iteration would be user-hostile; the `invalid_grant` sibling
        // above likewise surfaces the reconnect once (its cleared tokens then
        // short-circuit later iterations).
        if *transient_failure_count == 1 {
            let _ = app.emit("spotify-reconnect-required", json!(null));
            let _ = app.emit("reconnect-required", json!(null));
        }
        if *transient_failure_count >= super::timing::TRANSIENT_FAILURE_EXIT_THRESHOLD {
            log::error!("[POLLING] poll_once: 5 consecutive credential failures, exiting and requiring reconnect");
            return PollIteration::Break;
        }
        return super::timing::interruptible_sleep(
            stop_rx,
            super::timing::with_jitter(super::timing::ERROR_RETRY_INTERVAL_SECONDS),
            "credentials-unavailable sleep",
            mode,
        );
    } else {
        spotify_tokens
    };

    let access_token = spotify_tokens.access_token.clone();
    log::debug!("[POLLING] poll_once: preparing playback source");

    // Issue #862: a `playback.source` change in Settings rebuilds the
    // source on the next iteration. The kind stored in
    // `last_source_kind` is the comparison key — when it differs from
    // `config.playback.source`, the source is rebuilt through
    // `build_source`. The Spotify ETag cache and the system-source
    // singletons are dropped with the old source and re-established
    // on the next iteration. `config` is `Option<AppConfig>` (no config
    // = pre-init / first poll); `.unwrap_or_default()` on the kind is
    // documented `Auto`, which is also what `last_source_kind` boots as
    // in the loop driver.
    let new_kind = config
        .as_ref()
        .map(|c| c.playback.source)
        .unwrap_or_default();
    if new_kind != *last_source_kind {
        log::info!(
            "[POLLING] poll_once: playback source kind changed from {:?} to {:?}, rebuilding",
            *last_source_kind,
            new_kind
        );
        *playback_source = crate::sources::build_source(new_kind)
            .unwrap_or_else(|| Box::new(crate::sources::spotify::SpotifySource::new()));
        *last_source_kind = new_kind;
    }
    // Push the latest access token. The downcast is a `TypeId` check; an
    // `AutoSource` exposes the same `set_spotify_access_token` helper that
    // `SpotifySource` does via its trait `as_any_mut` hook. System
    // sources (SMTC / MPRIS) ignore the token.
    if let Some(spotify_src) = playback_source
        .as_any_mut()
        .downcast_mut::<crate::sources::spotify::SpotifySource>()
    {
        spotify_src.set_access_token(Some(access_token.clone()));
    } else if let Some(auto_src) = playback_source
        .as_any_mut()
        .downcast_mut::<crate::sources::AutoSource>()
    {
        auto_src.set_spotify_access_token(Some(access_token.clone()));
    }
    log::debug!(
        "[POLLING] poll_once: source selected = {:?}",
        playback_source.id()
    );

    let last_poll_instant = Instant::now();

    let result = playback_source.poll();

    match result {
        Ok(Some(np)) => {
            // Issue #862: the trait surface is a flat `NowPlaying`. Convert
            // to the rich `spotify::NowPlaying { media, episode, context }`
            // shape `process_track` already speaks. The Spotify source
            // populated `media`; episode / context metadata only exists for
            // Spotify podcasts, which the trait surface intentionally drops
            // at the boundary. System sources produce a flat track with
            // `episode: None` and the default context, identical to a
            // Spotify music track.
            let now: crate::spotify::NowPlaying = crate::spotify::NowPlaying {
                media: crate::spotify::TrackInfo::from(&np),
                episode: None,
                context: crate::spotify::PlaybackContext::default(),
            };
            state.session.store_now_playing(Some(now.clone()));

            // 304-equivalent path: the Spotify source flags it via
            // `last_poll_was_not_modified` after a 304 round-trip. The
            // system sources always return false (every query is a
            // fresh read), so this branch is Spotify-only in practice.
            if playback_source.last_poll_was_not_modified() {
                if let Some(now_for_rewrite) = super::status_text::config_flip_rewrite_track(
                    &state.session,
                    last_track_key,
                    &config,
                ) {
                    log::info!(
                        "[POLLING] poll_once: 304 but status config changed mid-track, forcing one rewrite"
                    );
                    let sleep = super::write::process_track(
                        app,
                        state,
                        &config,
                        &now_for_rewrite,
                        last_track_key,
                        last_poll_instant,
                        last_teams_update,
                        last_posted_placeholder,
                        suppressed_placeholder,
                        consecutive_pauses,
                        gated_track_key,
                        last_availability_arm,
                        armed_presence,
                        last_posted_status,
                        last_gate_check,
                        last_idle_verdict,
                        force_resume_write,
                    );
                    super::timing::record_success(
                        transient_failure_count,
                        consecutive_network_failures,
                    );
                    return PollIteration::Sleep { seconds: sleep };
                }
                // Issue #790: the 304 fast path skips process_track, so
                // the availability session's own 4-minute clock has to be
                // wound here too — otherwise the steady state of a long
                // episode, DJ set or live stream only ever re-arms on the
                // 5-minute keepalive, at or past the fade boundary.
                let availability_backoff = super::presence::rearm_availability_after_304(
                    app,
                    state,
                    last_track_key,
                    last_poll_instant,
                    &config,
                    super::gate::gate_blocks_304_rearm(
                        gated_track_key.as_deref(),
                        last_track_key.as_deref(),
                    ),
                    armed_presence,
                    last_availability_arm,
                );
                let mut iteration = not_modified_iteration(
                    last_track_key,
                    consecutive_pauses,
                    transient_failure_count,
                    consecutive_network_failures,
                    &config,
                );
                if let PollIteration::Sleep { seconds } = &mut iteration {
                    // Issue #154: a throttled arm extends the next poll
                    // to the server-directed delay.
                    *seconds = (*seconds).max(availability_backoff);
                }
                return iteration;
            }

            // Fresh track (200 with new body, or a system-source read
            // whose key differs from `last_track_key`).
            // Issue #582: the tray's shuffle/repeat toggles are
            // rendered from the state this very body carries — no
            // extra request, no cache. System sources do not surface
            // shuffle/repeat (the spec leaves them unset); the helper
            // is a no-op in that case.
            crate::tray::note_playback_modes(
                &state.caches,
                now.context.shuffle,
                now.context.repeat,
            );
            // Issue #344: debug, not info — title/artist at info
            // level land verbatim in the diagnostics `recent_logs`
            // tail (a paste-able support artifact). No raw track
            // metadata there.
            log::debug!(
                "[POLLING] poll_once: track found - {} by {}",
                now.media.title,
                now.media.artist
            );
            let sleep_duration = super::write::process_track(
                app,
                state,
                &config,
                &now,
                last_track_key,
                last_poll_instant,
                last_teams_update,
                last_posted_placeholder,
                suppressed_placeholder,
                consecutive_pauses,
                gated_track_key,
                last_availability_arm,
                armed_presence,
                last_posted_status,
                last_gate_check,
                last_idle_verdict,
                force_resume_write,
            );
            super::timing::record_success(transient_failure_count, consecutive_network_failures);
            PollIteration::Sleep {
                seconds: sleep_duration,
            }
        }
        Ok(None) => {
            state.session.store_now_playing(None);
            log::info!("[POLLING] poll_once: no track playing");
            let no_track_backoff = super::write::handle_no_track(
                app,
                state,
                last_track_key,
                &config,
                last_posted_placeholder,
                suppressed_placeholder,
                gated_track_key,
                last_availability_arm,
                armed_presence,
                first_iteration,
                last_posted_status,
            );
            super::timing::record_success(transient_failure_count, consecutive_network_failures);
            let mut iteration = record_no_track_outcome(consecutive_pauses, &config);
            if let PollIteration::Sleep { seconds } = &mut iteration {
                // Issue #154: a throttled Teams clear extends the next poll
                // to the server-directed delay.
                *seconds = (*seconds).max(no_track_backoff);
            }
            iteration
        }
        Err(source_err) => {
            log::error!(
                "[POLLING] poll_once: Failed to get currently playing track: {}",
                source_err
            );

            // Issue #862: the trait surface maps Spotify's `ExpiredToken`
            // to `SourceError::Auth(_)` (the only `Auth` variant that
            // should trigger a refresh attempt; `InvalidGrant` /
            // `NotPremium` are also `Auth` but require a different
            // resolution path).
            let mut final_err = source_err;
            let mut backoff_secs =
                super::timing::with_jitter(super::timing::ERROR_RETRY_INTERVAL_SECONDS);

            let token_expired = matches!(final_err, crate::sources::SourceError::Auth(_))
                && final_err
                    .to_string()
                    .contains("spotify access token expired");
            if token_expired && !client_id.is_empty() && !client_secret.is_empty() {
                log::info!("[POLLING] poll_once: token expired, attempting refresh");
                let current_tokens = state.tokens.spotify().clone();
                if let Some(tokens) = current_tokens {
                    let pre_refresh_access_token = tokens.access_token.clone();
                    match refresh_spotify_token(&tokens, &client_id, &client_secret) {
                        Ok(new_tokens) => {
                            let committed = match super::refresh::cas_refresh_spotify(
                                state,
                                "spotify",
                                &pre_refresh_access_token,
                                || Ok::<_, SpotifyApiError>(new_tokens.clone()),
                            ) {
                                super::refresh::CasOutcome::Committed(_) => true,
                                super::refresh::CasOutcome::Discarded { .. } => false,
                                super::refresh::CasOutcome::RefreshFailed { .. } => {
                                    unreachable!("inner refresh_fn is Ok-wrapping")
                                }
                            };
                            if committed {
                                if let Err(e) = token_io::persist_tokens(state, app) {
                                    log::warn!(
                                        "[POLLING] poll_once: failed to persist refreshed spotify tokens: {}",
                                        e
                                    );
                                }
                                let retry_token = new_tokens.access_token.clone();
                                // Push the new token back into the source —
                                // `Box<dyn PlaybackSource>` downcasts to the
                                // concrete Spotify / Auto source so the
                                // retry reads the refreshed credential.
                                if let Some(spotify_src) = playback_source
                                    .as_any_mut()
                                    .downcast_mut::<crate::sources::spotify::SpotifySource>(
                                ) {
                                    spotify_src.set_access_token(Some(retry_token));
                                } else if let Some(auto_src) = playback_source
                                    .as_any_mut()
                                    .downcast_mut::<crate::sources::AutoSource>(
                                ) {
                                    auto_src.set_spotify_access_token(Some(retry_token));
                                }
                                let last_poll_instant_retry = Instant::now();
                                match playback_source.poll() {
                                    Ok(Some(np)) => {
                                        let now = crate::spotify::NowPlaying {
                                            media: crate::spotify::TrackInfo::from(&np),
                                            episode: None,
                                            context: crate::spotify::PlaybackContext::default(),
                                        };
                                        state.session.store_now_playing(Some(now.clone()));
                                        if playback_source.last_poll_was_not_modified() {
                                            if let Some(now_for_rewrite) =
                                                super::status_text::config_flip_rewrite_track(
                                                    &state.session,
                                                    last_track_key,
                                                    &config,
                                                )
                                            {
                                                log::info!(
                                                    "[POLLING] poll_once: 304 but status config changed mid-track, forcing one rewrite"
                                                );
                                                let sleep = super::write::process_track(
                                                    app,
                                                    state,
                                                    &config,
                                                    &now_for_rewrite,
                                                    last_track_key,
                                                    last_poll_instant_retry,
                                                    last_teams_update,
                                                    last_posted_placeholder,
                                                    suppressed_placeholder,
                                                    consecutive_pauses,
                                                    gated_track_key,
                                                    last_availability_arm,
                                                    armed_presence,
                                                    last_posted_status,
                                                    last_gate_check,
                                                    last_idle_verdict,
                                                    force_resume_write,
                                                );
                                                super::timing::record_success(
                                                    transient_failure_count,
                                                    consecutive_network_failures,
                                                );
                                                return PollIteration::Sleep { seconds: sleep };
                                            }
                                            let availability_backoff =
                                                super::presence::rearm_availability_after_304(
                                                    app,
                                                    state,
                                                    last_track_key,
                                                    last_poll_instant_retry,
                                                    &config,
                                                    super::gate::gate_blocks_304_rearm(
                                                        gated_track_key.as_deref(),
                                                        last_track_key.as_deref(),
                                                    ),
                                                    armed_presence,
                                                    last_availability_arm,
                                                );
                                            let mut iteration = not_modified_iteration(
                                                last_track_key,
                                                consecutive_pauses,
                                                transient_failure_count,
                                                consecutive_network_failures,
                                                &config,
                                            );
                                            if let PollIteration::Sleep { seconds } = &mut iteration
                                            {
                                                *seconds = (*seconds).max(availability_backoff);
                                            }
                                            return iteration;
                                        }
                                        crate::tray::note_playback_modes(
                                            &state.caches,
                                            now.context.shuffle,
                                            now.context.repeat,
                                        );
                                        log::debug!(
                                            "[POLLING] poll_once: retry track found - {} by {}",
                                            now.media.title,
                                            now.media.artist
                                        );
                                        let _sleep = super::write::process_track(
                                            app,
                                            state,
                                            &config,
                                            &now,
                                            last_track_key,
                                            last_poll_instant_retry,
                                            last_teams_update,
                                            last_posted_placeholder,
                                            suppressed_placeholder,
                                            consecutive_pauses,
                                            gated_track_key,
                                            last_availability_arm,
                                            armed_presence,
                                            last_posted_status,
                                            last_gate_check,
                                            last_idle_verdict,
                                            force_resume_write,
                                        );
                                        super::timing::record_success(
                                            transient_failure_count,
                                            consecutive_network_failures,
                                        );
                                        return PollIteration::Sleep { seconds: _sleep };
                                    }
                                    Ok(None) => {
                                        state.session.store_now_playing(None);
                                        log::info!("[POLLING] poll_once: retry no track");
                                        let no_track_backoff = super::write::handle_no_track(
                                            app,
                                            state,
                                            last_track_key,
                                            &config,
                                            last_posted_placeholder,
                                            suppressed_placeholder,
                                            gated_track_key,
                                            last_availability_arm,
                                            armed_presence,
                                            first_iteration,
                                            last_posted_status,
                                        );
                                        super::timing::record_success(
                                            transient_failure_count,
                                            consecutive_network_failures,
                                        );
                                        let mut iteration =
                                            record_no_track_outcome(consecutive_pauses, &config);
                                        if let PollIteration::Sleep { seconds } = &mut iteration {
                                            *seconds = (*seconds).max(no_track_backoff);
                                        }
                                        return iteration;
                                    }
                                    Err(retry_err) => {
                                        log::error!(
                                            "[POLLING] poll_once: retry after refresh also failed: {}",
                                            retry_err
                                        );
                                        final_err = retry_err;
                                    }
                                }
                            }
                        }
                        Err(refresh_err) => {
                            log::error!(
                                "[POLLING] poll_once: token refresh failed: {}",
                                refresh_err
                            );
                            // Issue #160: only a dead refresh token
                            // (`invalid_grant`) needs re-auth; other refresh
                            // failures are transient and flow into the
                            // backoff / 5-strikes logic below.
                            // Issue #798: a mid-flight replacement means the error is
                            // about a superseded token, not the live session — skip
                            // the clear AND the reconnect emits, and report the
                            // stale write error (not the refresh error) so the
                            // attempt does not feed the 5-strikes reconnect exit.
                            // Only clear when the slot still holds the token this
                            // refresh ran from.
                            let refresh_superseded = state
                                .tokens
                                .spotify()
                                .as_ref()
                                .map(|t| t.access_token.as_str())
                                != Some(pre_refresh_access_token.as_str());
                            let clear_succeeded = if matches!(
                                refresh_err,
                                SpotifyApiError::InvalidGrant
                            ) {
                                let cleared = state.tokens_load.clear_spotify_if_current(
                                    &state.tokens,
                                    &pre_refresh_access_token,
                                );
                                if cleared {
                                    log::warn!("[POLLING] poll_once: Spotify refresh token invalid (invalid_grant), discarding tokens and requiring reconnect");
                                    if let Err(persist_err) = token_io::persist_tokens(state, app) {
                                        log::warn!(
                                            "[POLLING] poll_once: failed to persist cleared Spotify tokens: {}",
                                            persist_err
                                        );
                                    }
                                    let _ = app.emit("spotify-reconnect-required", json!(null));
                                    let _ = app.emit("reconnect-required", json!(null));
                                }
                                Some(cleared)
                            } else {
                                None
                            };
                            match super::refresh::classify_spotify_refresh_failure(
                                &refresh_err,
                                refresh_superseded,
                                clear_succeeded,
                            ) {
                                super::refresh::SpotifyRefreshFailure::Account(error) => {
                                    final_err = error
                                }
                                super::refresh::SpotifyRefreshFailure::Superseded => {
                                    log::warn!("[POLLING] poll_once: slot replaced mid-refresh, keeping newer Spotify session");
                                    return PollIteration::Sleep {
                                        seconds: super::timing::clamp_poll_interval(
                                            super::timing::config_default_interval(&config),
                                            &config,
                                        ),
                                    };
                                }
                            }
                        }
                    }
                }
            }

            // Issue #159 (finding PollCore#3, issue #571): honor the server's
            // `Retry-After` (floored at the error retry interval so a tiny
            // value can't create a busy loop), and NEVER sleep below it — the
            // jitter applied to a server-directed value is upward-only,
            // because the symmetric ±20% could turn `Retry-After: 300` into a
            // 240s sleep and immediately re-trigger the very rate limit the
            // header exists to avoid. The header-less fallback keeps the
            // symmetric jitter.
            //
            // Issue #862: the source surface is `SourceError`, but a
            // 429 from Spotify still carries its `Retry-After` in the
            // error message — the helper inspects the string and
            // returns the same backoff the pre-v5 Spotify API error
            // path produced.
            if matches!(final_err, crate::sources::SourceError::Transient(_))
                && final_err.to_string().contains("rate limited")
            {
                if let Some(retry_after) =
                    super::timing::extract_retry_after(&final_err.to_string())
                {
                    backoff_secs = super::timing::spotify_backoff_secs_retry_after(retry_after);
                }
            }

            // Finding PollCore#0 (issue #568): only genuinely dead credentials
            // count toward the reconnect exit. Everything else — transport
            // errors, 5xx, JSON parse failures and 429s — is a NETWORK
            // failure when the source returns `SourceError::Transient` or
            // `SourceError::Other` (NOT a Spotify-specific error). A
            // `SourceError::Auth` that is NOT a Spotify invalid-grant is
            // downstream of the existing `Spotify` API surface (the
            // `SpotifyApiError` taxonomy now lives behind the source's
            // error conversion), so the same five-strikes logic still
            // applies — only the in-band classification is different.
            let is_auth = matches!(final_err, crate::sources::SourceError::Auth(_));
            if is_auth {
                *transient_failure_count = transient_failure_count.saturating_add(1);
                if let Some(iteration) = super::timing::transient_outcome(*transient_failure_count)
                {
                    log::error!(
                        "[POLLING] poll_once: {} consecutive auth failures, exiting and requiring reconnect",
                        super::timing::TRANSIENT_FAILURE_EXIT_THRESHOLD
                    );
                    // Issue #389: the exit must carry the provider-specific
                    // signal alongside the generic one — mirror the
                    // `InvalidGrant` arms above, which emit both, so the
                    // frontend can start a real Spotify OAuth flow.
                    let _ = app.emit("spotify-reconnect-required", json!(null));
                    let _ = app.emit("reconnect-required", json!(null));
                    return iteration;
                }
            } else {
                *consecutive_network_failures = consecutive_network_failures.saturating_add(1);
                if *consecutive_network_failures >= super::timing::NETWORK_FAILURE_THRESHOLD {
                    log::warn!(
                        "[POLLING] poll_once: {} consecutive network failures, backing off (polling continues, no reconnect)",
                        *consecutive_network_failures
                    );
                    backoff_secs = backoff_secs.max(super::timing::network_failure_backoff(
                        *consecutive_network_failures,
                    ));
                }
            }

            emit_error(
                app,
                "spotify",
                format!("Failed to get currently playing: {}", final_err),
                ErrorSeverity::Warning,
            );
            super::timing::interruptible_sleep(stop_rx, backoff_secs, "backoff sleep", mode)
        }
    }
}

/// Record a no-track outcome. The ONLY place `consecutive_pauses` is
/// incremented in response to a no-track result.
fn record_no_track_outcome(
    consecutive_pauses: &mut u8,
    config: &Option<std::sync::Arc<crate::config::AppConfig>>,
) -> PollIteration {
    let no_track_sleep = super::timing::pause_backoff(
        *consecutive_pauses,
        super::timing::config_default_interval(config),
        super::timing::config_pause_backoff_max(config),
    );
    *consecutive_pauses = consecutive_pauses.saturating_add(1).min(4);
    log::info!(
        "[POLLING] poll_once: sleeping for {} seconds (no track)",
        no_track_sleep
    );
    PollIteration::Sleep {
        seconds: no_track_sleep,
    }
}

/// Handle a 304 Not Modified from the conditional GET (candidate C11,
/// docs/scope-3.3.md §C11). The response carries no body, so there is
/// nothing to JSON-parse, no status to format/filter and no new state for
/// the tray or frontend — the observable behavior matches the
/// unchanged-track path minus that work: keep every tracked field, reset
/// the transient counter the way an unchanged playing track does, and sleep
/// without duration-derived smart sleep (`progress_ms`, which a bodyless 304
/// cannot provide).
///
/// A 304 with a tracked track mirrors the unchanged-track path: reset the
/// pause counter and sleep the default interval — bounded by the configured
/// `[min, max]` window (finding PollCore#6, issue #573). A 304 with no
/// tracked track means "still nothing playing" (issue #242): the no-track
/// ETag stays valid so idle polling keeps sending conditional GETs, and the
/// pause backoff advances exactly like an unconditional 204 no-track. A later
/// change surfaces as a 200/204 Modified and re-establishes ground truth
/// automatically.
fn not_modified_iteration(
    last_track_key: &Option<String>,
    consecutive_pauses: &mut u8,
    transient_failure_count: &mut u8,
    consecutive_network_failures: &mut u8,
    config: &Option<std::sync::Arc<crate::config::AppConfig>>,
) -> PollIteration {
    log::info!("[POLLING] poll_once: 304 Not Modified, skipping parse/format/tray work");
    super::timing::record_success(transient_failure_count, consecutive_network_failures);
    if last_track_key.is_some() {
        *consecutive_pauses = 0;
        // Finding PollCore#6 (issue #573): this arm dominates idle runtime, so
        // it must honour the configured bounds too — pre-fix it slept the raw
        // `default_interval_seconds`, which `clamp_polling` permits to exceed
        // `max_interval_seconds` (e.g. default 120 / max 60), silently
        // violating the "Max interval (s)" setting on the most common path.
        return PollIteration::Sleep {
            seconds: super::timing::clamp_poll_interval(
                super::timing::config_default_interval(config),
                config,
            ),
        };
    }
    record_no_track_outcome(consecutive_pauses, config)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Production source with the test module stripped — the shared preamble
    /// for the structural guards below.
    fn prod_source() -> &'static str {
        include_str!("iteration.rs")
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("iteration.rs has no #[cfg(test)] mod tests block")
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

    /// Regression guard for issue #72 drift point #1: every no-track
    /// code path (main `Ok(None)` arm, 401-retry `Ok(None)` arm, and —
    /// since issue #242 — the idle 304 arm in `not_modified_iteration`)
    /// must funnel through `record_no_track_outcome` so they cannot
    /// drift apart.
    ///
    /// Note: `process_track`'s paused-but-tracked branch also
    /// increments `consecutive_pauses` (issue #38). That increment is
    /// a separate concern (track found but `is_playing == false`) and
    /// is NOT the no-track drift point — the drift was the *no-track*
    /// increment order differing between the main arm and the 401-retry
    /// arm. We assert the no-track paths share a helper, not that
    /// every increment lives in one place.
    /// Why a source scan survives here (issue #778 allows exactly this
    /// shape): the invariant is the exact call-site COUNT (3 sites + definition); a new no-track path outside the helper is a structural property, so the count is pinned at the source.
    #[test]
    fn test_no_track_paths_share_record_helper() {
        let source = include_str!("iteration.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("iteration.rs has no #[cfg(test)] mod tests block");
        // Occurrences of `record_no_track_outcome(` in production code.
        // The `fn record_no_track_outcome(` definition matches too, so
        // the expected total is 1 definition + one call site per no-track
        // path: main `Ok(None)`, 401-retry `Ok(None)`, and the idle 304
        // (`not_modified_iteration`, issue #242). All three funnel the
        // increment through the same helper; a fourth site outside a
        let call_count = prod_source.matches("record_no_track_outcome(").count();
        // Finding PollCore#8 (issue #574): pin the exact total (the old `>= 4`
        // passed even if a call site — or the shared helper — was deleted).
        assert_eq!(
            call_count, 4,
            "Expected exactly 4 occurrences in production (3 call sites: main \
             Ok(None), 401-retry Ok(None), idle 304 in not_modified_iteration, \
             plus the fn definition). Found {}. A new no-track handling site \
             outside the shared helper lets the increment order drift again; \
             a deleted one loses the pause backoff. See issue #72 drift point #1.",
            call_count
        );
    }

    /// Regression guard for issue #72 drift point #2. Finding PollCore#8
    /// (issue #574): the canonical "Failed to get currently playing" emit is
    /// pinned at EXACTLY one site — the old `>= 1` passed even when the emit
    /// was deleted or duplicated by a new failure path.
    /// Why a source scan survives here (issue #778 allows exactly this
    /// shape): the invariant is exactly ONE canonical emit site; emit placement across failure paths is structural, so the count is pinned at the source.
    #[test]
    fn test_error_event_emitted_in_exactly_one_place_per_failed_poll() {
        let source = include_str!("iteration.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("iteration.rs has no #[cfg(test)] mod tests block");
        let canonical_msg_count = prod_source
            .matches("Failed to get currently playing:")
            .count();
        // Finding PollCore#8 (issue #574): exactly one emit site — the old
        // `>= 1` passed even if the canonical error emit was deleted, or
        // duplicated by a new failure path.
        assert_eq!(
            canonical_msg_count, 1,
            "expected exactly 1 'Failed to get currently playing:' emit_error; found {}",
            canonical_msg_count
        );
    }

    /// Regression guard for issue #79/#117: poll_once.rs must NOT emit raw
    /// "error" events directly.
    /// Why a source scan survives here (issue #778 allows exactly this
    /// shape): the invariant is the ABSENCE of raw `emit("error",` plus the helper call count; emit placement is structural, so the absence is pinned at the source.
    #[test]
    fn test_no_raw_error_emit_in_poll_once() {
        let source = include_str!("iteration.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("iteration.rs has no #[cfg(test)] mod tests block");
        assert!(
            !prod_source.contains(r#"emit("error","#),
            "poll_once.rs must not emit raw \"error\" events directly."
        );
        let helper_call_count = prod_source.matches("emit_error(").count();
        assert!(
            helper_call_count >= 2,
            "emit_error called {} times; need >=2",
            helper_call_count
        );
    }

    /// Candidate C11 (docs/scope-3.3.md §C11): a 304 Not Modified with a
    /// tracked track is a pure no-op iteration — default-interval sleep,
    /// pause/transient counters reset exactly like the unchanged-track path.
    /// The stored ETag survives structurally: the 304 path never touches it,
    /// so the next poll stays conditional.
    #[test]
    fn test_not_modified_keeps_state_and_sleeps_default_interval() {
        let config = Some(std::sync::Arc::new(crate::config::AppConfig::default()));
        let mut consecutive_pauses: u8 = 3;
        let mut transient_failure_count: u8 = 2;
        let mut consecutive_network_failures: u8 = 3;

        let iteration = not_modified_iteration(
            &Some("Artist - Track".to_string()),
            &mut consecutive_pauses,
            &mut transient_failure_count,
            &mut consecutive_network_failures,
            &config,
        );

        let seconds = match iteration {
            PollIteration::Sleep { seconds } => seconds,
            _ => panic!("304 must yield a Sleep iteration"),
        };
        assert_eq!(
            seconds, 30,
            "304 must sleep the configured default interval"
        );
        assert_eq!(consecutive_pauses, 0, "unchanged track resets pauses");
        assert_eq!(
            transient_failure_count, 0,
            "a 304 counts as success for the 5-strikes counter"
        );
        assert_eq!(
            consecutive_network_failures, 0,
            "a 304 counts as success for the network-failure counter too (finding PollCore#0)"
        );
    }

    /// Issue #242: a 304 with no tracked track means "still nothing playing".
    /// It must advance the pause backoff exactly like an unconditional 204
    /// no-track (steady conditional GETs, no 304/drop/unconditional
    /// oscillation, no stalled backoff).
    #[test]
    fn test_not_modified_without_tracked_track_advances_pause_backoff() {
        let config = Some(std::sync::Arc::new(crate::config::AppConfig::default()));
        let mut consecutive_pauses: u8 = 1;
        let mut transient_failure_count: u8 = 1;
        let mut consecutive_network_failures: u8 = 1;

        let iteration = not_modified_iteration(
            &None,
            &mut consecutive_pauses,
            &mut transient_failure_count,
            &mut consecutive_network_failures,
            &config,
        );

        let seconds = match iteration {
            PollIteration::Sleep { seconds } => seconds,
            _ => panic!("304 must yield a Sleep iteration"),
        };
        assert_eq!(
            seconds, 60,
            "idle 304 must sleep the pause backoff (2x default at pauses=1), matching 204 no-track"
        );
        assert_eq!(
            consecutive_pauses, 2,
            "idle 304 must advance the pause counter like a 204 no-track"
        );
        assert_eq!(
            transient_failure_count, 0,
            "a 304 counts as success for the 5-strikes counter"
        );
        assert_eq!(
            consecutive_network_failures, 0,
            "a 304 counts as success for the network-failure counter too (finding PollCore#0)"
        );
    }

    /// Issue #389 (finding PollCore#0, issue #568): the reconnect exit must
    /// emit the provider-specific `spotify-reconnect-required` alongside the
    /// generic `reconnect-required` — mirroring the `InvalidGrant` arms — so
    /// the frontend can start a real Spotify OAuth flow instead of seeing only
    /// the generic banner. And it must be reachable ONLY from an auth failure:
    /// a network blip must not stop the session or open a browser. Structural
    /// guard: the exit window is isolated by its log marker.
    ///
    /// Issue #862: the in-band classification moved from
    /// `is_auth_failure(&final_err)` to a `SourceError::Auth(_)` match —
    /// `SpotifySource::poll` returns `SourceError::Auth(_)` for expired /
    /// invalid-grant tokens and the poll loop counts those toward the
    /// existing 5-strikes exit. The guard now greps for the new pattern.
    /// Why a source scan survives here (issue #778 allows exactly this
    /// shape): the invariant is both reconnect emits sitting INSIDE the auth-gated exit block; the exit needs the live poll loop, so the placement is pinned at the source.
    #[test]
    fn test_five_strikes_exit_emits_spotify_reconnect() {
        let source = include_str!("iteration.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("iteration.rs has no #[cfg(test)] mod tests block");
        let marker = "consecutive auth failures, exiting and requiring reconnect";
        let exit_pos = prod_source
            .find(marker)
            .expect("the 5-strikes auth-exit log line must exist");
        // Issue #862: the classifier is now a `SourceError::Auth(_)`
        // match. The auth-gated block runs from the classifier to the
        // exit's `return iteration;`: both emits must sit INSIDE it.
        let auth_pos = prod_source
            .find("matches!(final_err, crate::sources::SourceError::Auth(_))")
            .expect("the error arm must classify SourceError::Auth (issue #862)");
        let window = &prod_source[exit_pos..];
        let window_end = window
            .find("return iteration;")
            .expect("the auth exit must return");
        let window = &prod_source[auth_pos..exit_pos + window_end];
        assert!(
            window.contains(r#"emit("spotify-reconnect-required""#),
            "the auth-gated exit must emit spotify-reconnect-required (issue #389)"
        );
        assert!(
            window.contains(r#"emit("reconnect-required""#),
            "the auth-gated exit must keep the generic reconnect-required"
        );
        // Finding PollCore#0: the error arm's classification must not
        // bump the reconnect counter on `SourceError::Transient(_)` /
        // `SourceError::Other(_)` — only `SourceError::Auth(_)` is a
        // dead-credential signal. The non-auth branch (the `else`)
        // exists to handle those network / transient / other errors.
        //
        // Issue #862: the `final_err` variable now carries the
        // `SourceError` taxonomy — the `SpotifyApiError::Other(_)`
        // check still applies because the conversion in
        // `sources/spotify.rs::SpotifySource::poll` maps the
        // `SpotifyApiError` arms into the matching `SourceError`
        // variants, and the guard against "network/parse failures
        // counting toward the reconnect exit" is unchanged.
        let arm_start = prod_source
            .find("let mut final_err = source_err;")
            .expect("the error arm must exist");
        let arm = &prod_source[arm_start..exit_pos];
        assert!(
            !arm.contains("SpotifyApiError::Other(_)"),
            "network/parse failures must not count toward the reconnect exit \
             (finding PollCore#0, issue #568)"
        );
    }

    /// Finding PollCore#6 (issue #573): "Max interval (s)" bounds the 304 and
    /// no-track sleeps too. `clamp_polling` permits `default > max` (the
    /// finding's example is default 120 / max 60), which pre-fix leaked
    /// straight onto the two paths that dominate idle runtime.
    #[test]
    fn test_not_modified_sleep_honors_configured_max_interval() {
        let mut config = crate::config::AppConfig::default();
        config.polling.default_interval_seconds = 120;
        config.polling.minimum_interval_seconds = 10;
        config.polling.max_interval_seconds = 60;
        let config = Some(std::sync::Arc::new(config));
        let mut auth = 0u8;
        let mut network = 0u8;

        let mut consecutive_pauses: u8 = 0;
        match not_modified_iteration(
            &Some("Artist - Track".to_string()),
            &mut consecutive_pauses,
            &mut auth,
            &mut network,
            &config,
        ) {
            PollIteration::Sleep { seconds } => assert_eq!(
                seconds, 60,
                "a tracked-track 304 must honor max_interval_seconds (120s pre-fix)"
            ),
            _ => panic!("304 must yield a Sleep iteration"),
        }

        // The no-track arm keeps the documented issue #38 ladder (default →
        // 2× → 4× → 300s cap, an idle-work reduction promised in
        // ARCHITECTURE.md/TROUBLESHOOTING.md), so it may exceed the interval
        // window — deliberately, and identically to the 204 no-track path.
        for (pauses, expected) in [(0u8, 120u64), (1, 240), (2, 300), (3, 300), (4, 300)] {
            let mut counter = pauses;
            match not_modified_iteration(&None, &mut counter, &mut auth, &mut network, &config) {
                PollIteration::Sleep { seconds } => assert_eq!(
                    seconds, expected,
                    "the idle ladder must stay the documented ladder at pauses={}",
                    pauses
                ),
                _ => panic!("304 must yield a Sleep iteration"),
            }
        }
    }

    /// Finding PollCore#4 (issue #572) structural guard: the one-shot entry
    /// must run against the shared clocks rather than fresh per-call locals.
    /// Why a source scan survives here (issue #778 allows exactly this
    /// shape): the invariant is the shared-clocks load plus publish-back with no fresh locals; the one-shot runs a real iteration, so the clock routing is pinned at the source.
    #[test]
    fn test_run_oneshot_uses_shared_write_clocks() {
        let prod = prod_source();
        let body = prod_fn_body(prod, "pub(crate) fn run_oneshot(");
        assert!(
            body.contains("super::clocks::load_write_clocks(&state.session)"),
            "run_oneshot must load the shared write clocks"
        );
        assert!(
            body.contains("super::clocks::store_write_clocks(&state.session, &ps.clocks);"),
            "run_oneshot must publish the clocks it advanced back to the shared slot"
        );
        assert!(
            !body.contains("let mut last_availability_arm: Option<Instant> = None;"),
            "a fresh availability clock on the one-shot path is exactly the issue #572 defect"
        );
    }

    /// S9 (issue #677): EVERY entry point to an iteration honours the snooze —
    /// the driver's loop is pinned by `snooze_gate_precedes_...` above, and
    /// `run_oneshot` is pinned here. `refresh_status`, the tray's post-action
    /// catch-up and the CLI's `--sync-once` all route through it, so a gate that
    /// only the loop consulted left three silent bypasses of the feature's
    /// "no Spotify or Graph work while snoozed" promise.
    /// Why a source scan survives here (issue #778 allows exactly this
    /// shape): the invariant is the snooze-gate-before-clocks-before-inner ORDER; the one-shot runs a real iteration, so the ordering is pinned at the source.
    #[test]
    fn run_oneshot_honours_the_snooze_gate() {
        let prod = prod_source();
        let body = prod_fn_body(prod, "pub(crate) fn run_oneshot(");
        let gate = body
            .find("snooze_gate(&state.session, &config)")
            .expect("run_oneshot must consult the snooze gate (S9)");
        let clocks = body
            .find("load_write_clocks(&state.session)")
            .expect("run_oneshot must still load the shared clocks");
        let inner = body
            .find("run_inner(")
            .expect("run_oneshot must still dispatch the iteration");
        assert!(
            gate < clocks,
            "the one-shot gate must run BEFORE the clocks are loaded: a snoozed \
             refresh must not move (or discard) a keepalive/debounce clock (S9)"
        );
        assert!(
            gate < inner,
            "the one-shot gate must run BEFORE the iteration: a snoozed refresh \
             must issue no Spotify GET (S9)"
        );
        assert_eq!(
            body.matches("snooze_gate(").count(),
            1,
            "exactly one gate call site is expected in the one-shot"
        );
        let skip_arm = &body[gate..clocks];
        assert!(
            skip_arm.contains("SnoozeGate::Skipped(_)") && skip_arm.contains("return;"),
            "the skip verdict must return before any request (S9)"
        );
        assert!(
            skip_arm.contains("SnoozeGate::Expired")
                && skip_arm.contains("clear_snooze_if_expired(state)"),
            "an expired deadline must still be cleared by a manual refresh (S9)"
        );
        assert!(
            skip_arm.contains("skipped — a snooze is active"),
            "a refresh that did nothing must say why (S9)"
        );
    }

    /// Issue #793: EVERY entry point to an iteration honours the quiet-hours
    /// pause — the driver's loop is pinned by
    /// `test_quiet_pause_gate_precedes_the_clock_load_and_the_iteration` above,
    /// and `run_oneshot` is pinned here. `refresh_status` (commands/sync.rs),
    /// the tray's post-action catch-up (tray.rs) and the CLI's `--sync-once`
    /// (lib.rs) all route through `run_oneshot`, so a pause the loop alone
    /// consulted left three silent bypasses of the `pausePollingHint` promise
    /// ("no Spotify query, no status update, no Teams call while paused").
    /// There is deliberately NO `--sync-once` override flag: a documented
    /// cron/headless path that silently ignored the pause would break that
    /// promise, so all three entry points honour it.
    /// Why a source scan survives here (issue #778 allows exactly this
    /// shape): the invariant is the snooze-then-pause-then-clocks-then-inner ORDER; the one-shot runs a real iteration needing live state, so the ordering is pinned at the source.
    #[test]
    fn run_oneshot_honours_the_quiet_pause() {
        let prod = prod_source();
        let body = prod_fn_body(prod, "pub(crate) fn run_oneshot(");
        let snooze = body
            .find("snooze_gate(&state.session, &config)")
            .expect("run_oneshot must still consult the snooze gate (S9)");
        let pause = body
            .find("quiet_pause_iteration(&state.session, &config)")
            .expect("run_oneshot must consult the quiet-hours pause gate (#793)");
        let clocks = body
            .find("load_write_clocks(&state.session)")
            .expect("run_oneshot must still load the shared clocks");
        let inner = body
            .find("run_inner(")
            .expect("run_oneshot must still dispatch the iteration");
        assert!(
            snooze < pause,
            "an explicit user snooze outranks a scheduled quiet window (#793)"
        );
        assert!(
            pause < clocks,
            "the one-shot pause must run BEFORE the clocks are loaded: a paused \
             refresh must not move (or discard) a keepalive/debounce clock (#793)"
        );
        assert!(
            pause < inner,
            "the one-shot pause must run BEFORE the iteration: a paused refresh \
             must issue no Spotify GET and no Teams write (#793)"
        );
        assert_eq!(
            body.matches("quiet_pause_iteration(").count(),
            1,
            "exactly one pause-gate call site is expected in the one-shot"
        );
        let skip_arm = &body[pause..clocks];
        assert!(
            skip_arm.contains("return;"),
            "the pause verdict must return before any request (#793)"
        );
        assert!(
            skip_arm.contains("skipped — a quiet-hours pause is active"),
            "a refresh that did nothing must say why (#793)"
        );
        // All three one-shot entry points route through run_oneshot, so the
        // gate above covers the command, the tray and the CLI with no
        // per-callsite bypass.
        let sync_source = include_str!("../commands/sync.rs");
        assert!(
            sync_source.contains("crate::polling::run_oneshot("),
            "refresh_status must route its one-shot through run_oneshot so the pause applies (#793)"
        );
        let tray_source = include_str!("../tray/actions.rs");
        assert!(
            tray_source.contains("crate::polling::run_oneshot("),
            "the tray catch-up must route its one-shot through run_oneshot so the pause applies (#793)"
        );
        let cli_source = include_str!("../cli.rs");
        assert!(
            cli_source.contains("polling::run_oneshot("),
            "--sync-once must route its one-shot through run_oneshot so the pause applies (#793)"
        );
    }

    /// Issue #893: one poll iteration performs no deep copy of `AppConfig` —
    /// it reads an immutable `Arc` snapshot — and a config save issued while
    /// an iteration is in flight completes without blocking on the poller,
    /// while the in-flight iteration never observes a partially updated
    /// config.
    #[test]
    fn iteration_snapshot_is_shared_and_survives_a_concurrent_save() {
        use std::sync::Arc;
        use std::time::Duration;

        // Shared-pointer identity: the iteration snapshot IS the stored
        // snapshot — no deep copy. A per-iteration `get().clone()` of the
        // document would produce a distinct allocation and fail this.
        let state = Arc::new(crate::AppState::new());
        let mut v1 = crate::config::AppConfig::default();
        v1.teams.status_format = "v1-format".to_string();
        *state.config.get_mut() = Some(Arc::new(v1));
        let stored = state.config.snapshot().expect("config was just stored");
        let iteration = state.config.snapshot().expect("iteration snapshot");
        assert!(
            Arc::ptr_eq(&stored, &iteration),
            "the iteration snapshot must share the stored pointer, not a deep copy (issue #893)"
        );
        // `stored` + `iteration` + the slot itself.
        let strong_before = Arc::strong_count(&stored);
        assert_eq!(
            strong_before, 3,
            "two snapshots of one stored config share one allocation (issue #893)"
        );

        // Hot-path twin: with no profile active the effective snapshot shares
        // the base pointer too (the lexicon/rules must not be cloned per
        // iteration); an active overlay allocates exactly once.
        let effective = crate::config::effective_snapshot(&iteration);
        assert!(
            Arc::ptr_eq(&iteration, &effective),
            "effective_snapshot must share the base pointer when no profile is active (issue #893)"
        );
        assert_eq!(
            Arc::strong_count(&stored),
            strong_before + 1,
            "the effective snapshot must only bump the shared strong count, never allocate a document"
        );
        let mut v_overlay = (*iteration).clone();
        v_overlay.active_profile = Some("missing".to_string());
        let overlaid = crate::config::effective_snapshot(&Arc::new(v_overlay));
        assert_eq!(
            Arc::strong_count(&overlaid),
            1,
            "an active profile overlay allocates exactly one fresh snapshot"
        );

        // Contention: a save racing the in-flight iteration (which holds only
        // its `Arc`, never the read guard) must acquire the write lock
        // promptly. If the iteration held the read guard across its body this
        // try-write would fail.
        // Deterministic save-during-iteration protocol: the writer takes
        // the write lock first and signals while HOLDING it; the main thread
        // (the in-flight iteration, owning only its `Arc`) then proves a
        // competing try-write observes contention — i.e. saves serialize on
        // the write lock — and releases the writer to publish. Had the
        // iteration held the read guard across its body, the writer could
        // never have acquired the guard to signal in the first place.
        let holding = Arc::new(std::sync::Barrier::new(2));
        let release = Arc::new(std::sync::Barrier::new(2));
        let holding_writer = Arc::clone(&holding);
        let release_writer = Arc::clone(&release);
        let state_writer = Arc::clone(&state);
        let snapshot_writer = Arc::clone(&iteration);
        let writer = std::thread::spawn(move || {
            let start = std::time::Instant::now();
            let mut guard = state_writer.config.get_mut();
            holding_writer.wait();
            release_writer.wait();
            let mut v2 = (*snapshot_writer).clone();
            v2.teams.status_format = "v2-format".to_string();
            v2.teams.clear_on_pause = !snapshot_writer.teams.clear_on_pause;
            *guard = Some(Arc::new(v2));
            start.elapsed()
        });
        holding.wait();
        assert!(
            state.config.try_get_mut().is_none(),
            "the racing save must hold the config write lock while the iteration runs on its snapshot (issue #893)"
        );
        release.wait();
        let save_took = writer.join().expect("writer thread must not panic");
        assert!(
            save_took < Duration::from_secs(5),
            "a config save during an in-flight iteration must complete without blocking (issue #893)"
        );
        // The in-flight snapshot is untouched by the concurrent save: no
        // partial update is observable through the old pointer.
        assert_eq!(iteration.teams.status_format, "v1-format");
        // And the new value landed for the NEXT iteration under a new pointer.
        let next = state.config.snapshot().expect("config still stored");
        assert_eq!(next.teams.status_format, "v2-format");
        assert!(
            !Arc::ptr_eq(&iteration, &next),
            "the save must publish a new snapshot"
        );
    }
}
