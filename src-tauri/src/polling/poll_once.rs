//! Single source of truth for one polling iteration.
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

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;

use chrono::{DateTime, Local, TimeZone, Utc};
use rand::Rng;
use serde_json::json;
use tauri::{AppHandle, Emitter};

use crate::config::{AppConfig, PresencePair, TrackRuleMatchKind};
use crate::profanity;
use crate::spotify::{
    format_status_with_context, is_token_expired, refresh_spotify_token, SpotifyApiError,
};
use crate::teams::{
    clear_teams_presence, clear_teams_presence_quick, clear_teams_status_message,
    clear_teams_status_message_quick, clear_user_preferred_presence,
    clear_user_preferred_presence_quick, get_teams_presence,
    is_token_expired as is_teams_token_expired, presence_gate_reason, refresh_teams_token,
    set_teams_presence, set_teams_status_message, set_user_preferred_presence, TeamsApiError,
    TeamsTokens, GATE_REASON_CALENDAR, GATE_REASON_IDLE, GATE_REASON_MANUAL_STATUS,
    GATE_REASON_QUIET_HOURS, GATE_REASON_TRACK_RULE,
};
use crate::token_io;
use crate::AppState;

use super::{emit_error, ErrorSeverity};

const ERROR_RETRY_INTERVAL_SECONDS: u64 = 30;
const RATE_LIMIT_BACKOFF_SECONDS: u64 = 60;
const DEBOUNCE_MS: u64 = 500;
/// Issue #364: when a track change lands inside the debounce window the
/// change signal must survive — the retry parks here, NOT on the
/// duration-derived sleep (which would stall the pending write until the
/// track nearly ends).
const DEBOUNCE_RETRY_SECONDS: u64 = 1;
/// Issue #384: identical-status writes are skipped while the last write
/// is this fresh; older than this the next poll force-writes a keepalive
/// so the Graph expiry never lapses.
const STATUS_KEEPALIVE_SECONDS: u64 = 5 * 60;
const TRANSIENT_FAILURE_EXIT_THRESHOLD: u8 = 5;
/// Finding PollCore#0 (issue #568): consecutive NETWORK failures — transport
/// errors, 5xx, JSON parse failures and 429s — get their own counter and
/// threshold. They must never end the session nor emit
/// `spotify-reconnect-required`: the frontend turns that event into a real
/// Spotify OAuth window (`+layout.svelte`), which is user-hostile when the
/// tokens on disk are still valid and only the network is down. The counter
/// is reset by any successful iteration.
const NETWORK_FAILURE_THRESHOLD: u8 = 12;
/// Base of the capped exponential backoff applied once
/// `NETWORK_FAILURE_THRESHOLD` consecutive network failures accumulate.
const NETWORK_BACKOFF_BASE_SECONDS: u64 = 30;
/// Ceiling for that backoff: an offline machine slows to this cadence but
/// KEEPS polling (never `PollIteration::Break`).
const NETWORK_BACKOFF_CAP_SECONDS: u64 = 300;
/// Minimum gap between setPresence re-arms while a track plays (issue
/// #3.0-P1). An `Available` session TIMES OUT after 5 minutes when the
/// availability is `Available` — a separate, non-configurable clock from
/// `expirationDuration` (which only bounds the session's absolute life,
/// 5 min–4 h, after which it goes `Offline`). On timeout the state fades
/// in stages: `Available` → `AvailableInactive` → `Away`. So the re-arm
/// must be well inside the 5-minute TIMEOUT, not the expiration window;
/// 4 minutes leaves slack. Raising this toward expiration scale (the
/// `PT4H` the app sends) does NOT extend the green bubble.
/// (Microsoft Learn: cloud-communications-manage-presence-state)
const AVAILABILITY_REARM_SECONDS: u64 = 4 * 60;

/// Documented bounds of a `setPresence` `expirationDuration` (finding #636,
/// issue #636): "The valid duration range is from 5 to 240 minutes (PT5M to
/// PT4H)", after which the session becomes `Offline`.
/// (Microsoft Learn: graph/api/presence-setpresence, manage-presence-state)
const PRESENCE_EXPIRATION_MIN_SECONDS: u64 = 5 * 60;
const PRESENCE_EXPIRATION_MAX_SECONDS: u64 = 4 * 60 * 60;

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

/// Finding PollCore#4 (issue #572): the write-decision clocks.
///
/// These fields decide WHETHER the next Teams/Graph write happens: debounce
/// (#364), change key (#343/#432), presence/rule gate (#3.0-P2/#380/#432),
/// identical-write keepalive (#384) and the availability re-arm clock
/// (#3.0-P1). Teams shows exactly ONE status per app, so the clocks describe
/// process-wide state rather than per-thread state — a second iteration
/// running in parallel (the manual `run_oneshot` refresh spawned by the tray
/// and `refresh_status`) must observe the SAME clocks, otherwise it re-arms
/// the availability session on a fresh clock and re-POSTs text Teams already
/// shows. The polling loop loads this once per iteration and stores it back
/// afterwards; a genuinely cold app reads `None` everywhere and arms normally.
///
/// Finding D11 (issue #694): that load/store pair is NOT atomic, and the
/// consequence of a stale write-back is worse than the "one redundant write"
/// this comment used to claim. `run_oneshot` (a tray/refresh-status "Refresh")
/// loads the slot once around its WHOLE iteration while the loop loads per
/// iteration, so a refresh that stores after the loop recorded a gate
/// (`gated_track_key`) published a PRE-gate snapshot back — silently dropping
/// the gate and letting the next write through mid-meeting. The snapshot
/// therefore carries a [`WriteClocks::generation`]: a store lands only when the
/// slot still holds the generation the snapshot was loaded at, so a superseded
/// writer is discarded (and logged) instead of resurrecting an old decision.
#[derive(Debug, Clone, Default)]
pub(crate) struct WriteClocks {
    /// The change key of the track whose status is currently on Teams
    /// (track identity + status-config fingerprint, #343/#432).
    pub(crate) last_track_key: Option<String>,
    /// Timestamp of the last Teams status write — times the debounce (#364)
    /// and the #384 keepalive.
    pub(crate) last_teams_update: Option<Instant>,
    /// The last placeholder content posted by a clear path, so
    /// byte-identical pause/no-track POSTs are skipped (#155).
    pub(crate) last_posted_placeholder: Option<String>,
    /// The track key whose status write was suppressed by the presence /
    /// quiet-hours / track-rule gate (#3.0-P2/#432).
    pub(crate) gated_track_key: Option<String>,
    /// When the `Available` presence session was last armed via setPresence
    /// (#3.0-P1). Owned here so the manual refresh cannot re-arm it early.
    pub(crate) last_availability_arm: Option<Instant>,
    /// The last playing-track status text posted (#384).
    pub(crate) last_posted_status: Option<String>,
    /// When the presence-gate re-check last ran — its own clock so re-checks
    /// never shift the debounce + keepalive write windows (#380).
    pub(crate) last_gate_check: Option<Instant>,
    /// The `setPresence` pair the app currently has armed (finding #634, issue
    /// #634). `None` = no session of ours is live. Kept next to
    /// `last_availability_arm` so a rule that starts or stops matching can
    /// switch the bubble on the NEXT iteration instead of waiting out the
    /// 4-minute cadence; the pair is also what the exit path clears.
    pub(crate) armed_presence: Option<PresencePair>,
    /// The placeholder text whose write was SUPPRESSED by the presence / rule
    /// gate (findings D3/D4, issues #686/#687). Deliberately distinct from
    /// `last_posted_placeholder`, which only ever holds text Teams actually
    /// shows: a suppressed placeholder must be RETRIED once the gate clears
    /// (re-check due, quiet window over, rule stopped matching), while a posted
    /// one stays deduped. Recording it also keeps the `presence-gated` event to
    /// one per suppression episode instead of one per poll.
    pub(crate) suppressed_placeholder: Option<String>,
    /// Generation of the slot this snapshot was loaded at (finding D11, issue
    /// #694). See the struct docs and [`store_write_clocks`].
    pub(crate) generation: u64,
    /// Issue #873: the desktop-idle gate cleared between this iteration
    /// and the last, so the next status write must happen even when the
    /// text is byte-identical to what Teams already shows — the user
    /// came back, and a stale "listening" status must re-appear exactly
    /// once. Set by the mid-track re-check (or the change-time gate) when
    /// the idle verdict flips from `true` to `false`; consumed by the
    /// write path, which clears it after the forced POST. Distinct from
    /// `gated_track_key` because that one tracks the suppression; this
    /// one tracks the resume.
    pub(crate) force_resume_write: bool,
    /// Issue #873: the previous iteration's idle verdict. Used to detect
    /// the `true`→`false` transition that arms `force_resume_write`.
    /// `None` on the very first iteration of a session (the change from
    /// "unknown" to any verdict is never a "resume").
    pub(crate) last_idle_verdict: Option<bool>,
}

/// Process-wide slot for [`WriteClocks`]. See the struct docs for why these
/// clocks are shared rather than per-thread.
static WRITE_CLOCKS: Mutex<WriteClocks> = Mutex::new(WriteClocks {
    last_track_key: None,
    last_teams_update: None,
    last_posted_placeholder: None,
    gated_track_key: None,
    last_availability_arm: None,
    last_posted_status: None,
    last_gate_check: None,
    armed_presence: None,
    suppressed_placeholder: None,
    generation: 0,
    force_resume_write: false,
    last_idle_verdict: None,
});

/// Snapshot the shared write-decision clocks. A poisoned lock is recovered
/// rather than propagated (`into_inner`): these are dedup heuristics, and
/// losing them costs at most one redundant Graph write.
///
/// The snapshot carries the generation it read, so the matching
/// [`store_write_clocks`] can tell whether it is still the slot's latest view
/// (finding D11, issue #694).
pub(crate) fn load_write_clocks() -> WriteClocks {
    WRITE_CLOCKS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

/// Publish the write-decision clocks back to the shared slot.
///
/// Finding D11 (issue #694): the store is generation-checked. `clocks` came
/// from [`load_write_clocks`] and carries the generation it read; if the slot
/// has moved on, another iteration already published a snapshot derived from a
/// later view of the world and THIS one is stale — applying it would resurrect
/// a pre-gate `gated_track_key` (and its `last_gate_check`) and let a write
/// through mid-meeting, which is exactly the defect the guard exists for.
/// Discard it and log: the lost fields are dedup hints the next iteration
/// re-derives, while a resurrected gate decision is not recoverable.
///
/// Residual, accepted and by design: two writers that loaded the SAME
/// generation are first-publish-wins — the first store lands and moves the
/// generation on, and the second is then discarded wholesale. For most fields
/// that costs dedup precision for one iteration, which the next iteration
/// re-derives; the alternative (letting the loser merge field-by-field) cannot
/// distinguish its own advances from the winner's and would resurrect exactly
/// the stale decisions the guard exists to drop.
///
/// The cost is NOT always one iteration, and the difference is worth stating
/// (review round 3, item 4): for a PRESENCE gate the discarded iteration had
/// just set or cleared, the surviving snapshot may hold the OPPOSITE verdict,
/// and an unchanged playing track does not re-read presence by itself — the
/// mid-track re-check only runs for a track the gate already names, so the gate
/// verdict is otherwise only revisited at the next track change. One status can
/// therefore go through mid-meeting (or be suppressed until the track ends),
/// bounded by the track's remaining duration. The guard is still the right
/// trade: it removes the far more common inversion (a pre-gate snapshot dropping
/// a FRESH gate, which is unbounded while the track plays), and a RULE gate
/// always re-derives on the next iteration because its verdict is recomputed
/// from the clock every time.
pub(crate) fn store_write_clocks(clocks: &WriteClocks) {
    let mut slot = WRITE_CLOCKS.lock().unwrap_or_else(|e| e.into_inner());
    if slot.generation != clocks.generation {
        log::debug!(
            "[POLLING] store_write_clocks: discarding a superseded snapshot (loaded generation {}, slot generation {}); a concurrent iteration already published a newer one",
            clocks.generation,
            slot.generation
        );
        return;
    }
    let next_generation = slot.generation.wrapping_add(1);
    *slot = clocks.clone();
    slot.generation = next_generation;
}

/// Forget the shared write-decision clocks. Called when a polling session
/// starts and when one ends: the clocks describe the status Teams shows for
/// the session that posted it, so a NEW session must start cold — otherwise
/// its first iteration would read a stale `last_track_key` (no
/// `spotify-track-changed` emit, no `current_track` update) or a stale gate.
/// A manual refresh while no session is running therefore behaves exactly like
/// one served by a fresh loop (it re-posts), while a manual refresh DURING a
/// session shares that session's clocks.
///
/// Finding D11 (issue #694): the reset bumps the generation too, so a snapshot
/// loaded before it (by a dead session, or by an in-flight one-shot) can never
/// land on the fresh slot.
pub(crate) fn reset_write_clocks() {
    let mut slot = WRITE_CLOCKS.lock().unwrap_or_else(|e| e.into_inner());
    let next_generation = slot.generation.wrapping_add(1);
    *slot = WriteClocks::default();
    slot.generation = next_generation;
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run(
    state: &Arc<AppState>,
    app: &AppHandle,
    stop_rx: &mpsc::Receiver<()>,
    last_track_key: &mut Option<String>,
    last_teams_update: &mut Option<Instant>,
    last_posted_placeholder: &mut Option<String>,
    suppressed_placeholder: &mut Option<String>,
    consecutive_pauses: &mut u8,
    transient_failure_count: &mut u8,
    consecutive_network_failures: &mut u8,
    gated_track_key: &mut Option<String>,
    last_availability_arm: &mut Option<Instant>,
    armed_presence: &mut Option<PresencePair>,
    // Issue #862: the playback source owns the ETag validator. The
    // driver constructs it once and hands it down so the etag
    // persists across iterations. `last_source_kind` is the comparison
    // key — a kind change rebuilds the source.
    playback_source: &mut Box<dyn crate::sources::PlaybackSource>,
    last_source_kind: &mut crate::sources::PlaybackSourceKind,
    first_iteration: &mut bool,
    last_posted_status: &mut Option<String>,
    last_gate_check: &mut Option<Instant>,
    // Issue #873: see `WriteClocks` for the resume invariant.
    last_idle_verdict: &mut Option<bool>,
    force_resume_write: &mut bool,
) -> PollIteration {
    run_inner(
        state,
        app,
        stop_rx,
        last_track_key,
        last_teams_update,
        last_posted_placeholder,
        suppressed_placeholder,
        consecutive_pauses,
        transient_failure_count,
        consecutive_network_failures,
        gated_track_key,
        last_availability_arm,
        armed_presence,
        playback_source,
        last_source_kind,
        first_iteration,
        last_posted_status,
        last_gate_check,
        last_idle_verdict,
        force_resume_write,
        RunMode::Loop,
    )
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
pub(crate) fn run_oneshot(state: &Arc<AppState>, app: &AppHandle) {
    // S9 (issue #677): the snooze gate, BEFORE the shared clocks are loaded.
    // Every entry point to an iteration has to honour it, and this is the second
    // one (the driver's loop is the first) — see the fn docs. The expiry case
    // falls through to a normal iteration so an explicit refresh after the
    // deadline still refreshes.
    let gate = {
        // Scoped: the config read guard must not outlive the decision.
        let config = state.config.get();
        snooze_gate(&config)
    };
    match gate {
        SnoozeGate::Skipped(_) => {
            log::info!(
                "[POLLING] run_oneshot: skipped — a snooze is active, so this refresh performed no request"
            );
            return;
        }
        SnoozeGate::Expired => clear_snooze_if_expired(state),
        SnoozeGate::Inactive => {}
    }
    // `_tx` is a live binding (not `let _`): the top stop-check treats a
    // `Disconnected` receiver as Break, so a dropped sender would make every
    // one-shot a silent no-op. Never collapse this to `let _`.
    let (_tx, rx) = mpsc::channel::<()>();
    let mut clocks = load_write_clocks();
    let mut consecutive_pauses: u8 = 0;
    let mut transient_failure_count: u8 = 0;
    let mut consecutive_network_failures: u8 = 0;
    // Issue #862: a one-shot constructs a fresh source — the etag is
    // empty (no persistent source for a one-shot iteration), so the
    // first read is unconditional. The `last_source_kind` parameter
    // exists for the looping path; a one-shot always starts at
    // `Default::default()` (Auto) and the kind-change check below
    // rebuilds the source on every iteration (cheap — same `Auto`
    // branch). The behaviour is identical to the loop's first
    // iteration.
    let mut playback_source: Box<dyn crate::sources::PlaybackSource> =
        crate::sources::build_source(
            state
                .config
                .get()
                .as_ref()
                .map(|c| c.playback.source)
                .unwrap_or_default(),
        )
        .unwrap_or_else(|| Box::new(crate::sources::spotify::SpotifySource::new()));
    let mut last_source_kind: crate::sources::PlaybackSourceKind = state
        .config
        .get()
        .as_ref()
        .map(|c| c.playback.source)
        .unwrap_or_default();
    // Issue #373 does NOT apply here: a one-shot is an explicit refresh,
    // not a fresh polling thread — an idle one-shot must stay silent
    // instead of POSTing a placeholder on every manual refresh.
    let mut first_iteration = false;
    let _ = run_inner(
        state,
        app,
        &rx,
        &mut clocks.last_track_key,
        &mut clocks.last_teams_update,
        &mut clocks.last_posted_placeholder,
        &mut clocks.suppressed_placeholder,
        &mut consecutive_pauses,
        &mut transient_failure_count,
        &mut consecutive_network_failures,
        &mut clocks.gated_track_key,
        &mut clocks.last_availability_arm,
        &mut clocks.armed_presence,
        &mut playback_source,
        &mut last_source_kind,
        &mut first_iteration,
        &mut clocks.last_posted_status,
        &mut clocks.last_gate_check,
        &mut clocks.last_idle_verdict,
        &mut clocks.force_resume_write,
        RunMode::OneShot,
    );
    store_write_clocks(&clocks);
}

#[allow(clippy::too_many_arguments)]
fn run_inner(
    state: &Arc<AppState>,
    app: &AppHandle,
    stop_rx: &mpsc::Receiver<()>,
    last_track_key: &mut Option<String>,
    last_teams_update: &mut Option<Instant>,
    last_posted_placeholder: &mut Option<String>,
    suppressed_placeholder: &mut Option<String>,
    consecutive_pauses: &mut u8,
    transient_failure_count: &mut u8,
    consecutive_network_failures: &mut u8,
    gated_track_key: &mut Option<String>,
    last_availability_arm: &mut Option<Instant>,
    armed_presence: &mut Option<PresencePair>,
    // Issue #862: the playback source is the layer the poll loop
    // speaks to. The driver constructs it once and passes it down so
    // the Spotify ETag cache and the system-source singletons persist
    // across iterations. `last_source_kind` is the comparison key —
    // a change in `config.playback.source` rebuilds the source.
    playback_source: &mut Box<dyn crate::sources::PlaybackSource>,
    last_source_kind: &mut crate::sources::PlaybackSourceKind,
    first_iteration: &mut bool,
    last_posted_status: &mut Option<String>,
    last_gate_check: &mut Option<Instant>,
    // Issue #873: see `WriteClocks` for the resume invariant.
    last_idle_verdict: &mut Option<bool>,
    force_resume_write: &mut bool,
    mode: RunMode,
) -> PollIteration {
    log::debug!("[POLLING] poll_once: iteration start");

    match stop_rx.recv_timeout(std::time::Duration::ZERO) {
        Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            log::info!("[POLLING] poll_once: stop signal at top, breaking");
            return PollIteration::Break;
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
    }

    let config = state.config.get().clone();
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
            let _ = clear_expired_preferred_presence(app, &teams_tokens.access_token, Utc::now());
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
            return interruptible_sleep(
                stop_rx,
                with_jitter(ERROR_RETRY_INTERVAL_SECONDS),
                "no-token sleep",
                mode,
            );
        }
    };

    let token_expired = is_token_expired(&spotify_tokens);
    log::debug!("[POLLING] poll_once: token_expired={}", token_expired);

    let (client_id, client_secret) = get_spotify_credentials(&config);
    // Issue #296: `get_spotify_credentials` reads the secret through the
    // cache-only `keychain::peek_spotify_client_secret()`, so it is empty
    // whenever the startup prime failed (locked Secret Service, headless
    // Linux, entry removed while running). Refreshing with an empty secret
    // can only produce a 400 `invalid_client` — not `InvalidGrant` — so the
    // Err arm below would emit a Warning and sleep *before* the 5-strikes
    // counter, looping forever with no user-visible cause. Classify the
    // decision up front (pure helper, mirroring the 401 path's guard) and
    // route the unavailable case to an actionable reconnect.
    let refresh_plan = spotify_refresh_plan(token_expired, &client_id, &client_secret);
    let spotify_tokens = if refresh_plan == SpotifyRefreshPlan::Refresh {
        log::info!("[POLLING] poll_once: Spotify token expired, refreshing...");
        log::info!(
            "[POLLING] poll_once: refreshing with client_id.len={}",
            client_id.len()
        );

        let pre_refresh_access_token = spotify_tokens.access_token.clone();
        match refresh_spotify_token(&spotify_tokens, &client_id, &client_secret) {
            Ok(new_tokens) => {
                log::info!("[POLLING] poll_once: token refresh SUCCESS");
                let cas_outcome = cas_refresh_or_discard(
                    "spotify",
                    &mut *state.tokens.spotify_mut(),
                    &pre_refresh_access_token,
                    // `Ok`-wrapping closure: annotate the error type so `E`
                    // is inferable (this arm never fails, so nothing else
                    // pins it) and matches the sibling `Err` arm's
                    // `SpotifyApiError`.
                    || Ok::<_, SpotifyApiError>(new_tokens.clone()),
                    |t| &t.access_token,
                );
                // Issue #180: the write guard reborrowed above is a temporary
                // that lives only until the end of this statement. Persist in
                // a LATER statement, when the guard is provably dropped —
                // persisting while it is alive would re-lock the same
                // parking_lot RwLock for reading and self-deadlock.
                if matches!(&cas_outcome, CasOutcome::Committed(_)) {
                    if let Err(e) = token_io::persist_tokens(state, app) {
                        log::warn!(
                            "[POLLING] poll_once: failed to persist refreshed spotify tokens: {}",
                            e
                        );
                    }
                }
                match cas_outcome {
                    CasOutcome::Committed(_) => new_tokens,
                    CasOutcome::Discarded { current } => match current {
                        Some(t) => t,
                        None => {
                            log::info!(
                                "[POLLING] poll_once: state cleared during refresh, waiting and re-polling"
                            );
                            return interruptible_sleep(
                                stop_rx,
                                with_jitter(ERROR_RETRY_INTERVAL_SECONDS),
                                "CAS-fail sleep",
                                mode,
                            );
                        }
                    },
                    CasOutcome::RefreshFailed(_) => unreachable!("inner refresh_fn is Ok-wrapping"),
                }
            }
            Err(e) => {
                log::error!(
                    "[POLLING] poll_once: Failed to refresh Spotify token: {}",
                    e
                );
                // Issue #160: `invalid_grant` means the refresh token is dead
                // (documented 6-month lifetime, or revoked). Discard it and
                // trigger re-auth instead of retrying forever. The write guard
                // is dropped before persist_tokens (which re-locks the same
                // RwLock for reading — parking_lot is not reentrant).
                if matches!(e, SpotifyApiError::InvalidGrant) {
                    log::error!("[POLLING] poll_once: Spotify refresh token invalid (invalid_grant), discarding tokens and requiring reconnect");
                    *state.tokens.spotify_mut() = None;
                    if let Err(persist_err) = token_io::persist_tokens(state, app) {
                        log::warn!(
                            "[POLLING] poll_once: failed to persist cleared Spotify tokens: {}",
                            persist_err
                        );
                    }
                    let _ = app.emit("spotify-reconnect-required", json!(null));
                    let _ = app.emit("reconnect-required", json!(null));
                    return interruptible_sleep(
                        stop_rx,
                        with_jitter(ERROR_RETRY_INTERVAL_SECONDS),
                        "invalid-grant sleep",
                        mode,
                    );
                }
                emit_error(
                    app,
                    "spotify",
                    format!("Token refresh failed: {}", e),
                    ErrorSeverity::Warning,
                );
                return interruptible_sleep(
                    stop_rx,
                    with_jitter(ERROR_RETRY_INTERVAL_SECONDS),
                    "error retry sleep",
                    mode,
                );
            }
        }
    } else if refresh_plan == SpotifyRefreshPlan::CredentialsUnavailable {
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
        if *transient_failure_count >= TRANSIENT_FAILURE_EXIT_THRESHOLD {
            log::error!("[POLLING] poll_once: 5 consecutive credential failures, exiting and requiring reconnect");
            return PollIteration::Break;
        }
        return interruptible_sleep(
            stop_rx,
            with_jitter(ERROR_RETRY_INTERVAL_SECONDS),
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
            *LAST_NOW_PLAYING.lock() = Some(now.clone());

            // 304-equivalent path: the Spotify source flags it via
            // `last_poll_was_not_modified` after a 304 round-trip. The
            // system sources always return false (every query is a
            // fresh read), so this branch is Spotify-only in practice.
            if playback_source.last_poll_was_not_modified() {
                if let Some(now_for_rewrite) = config_flip_rewrite_track(last_track_key, &config) {
                    log::info!(
                        "[POLLING] poll_once: 304 but status config changed mid-track, forcing one rewrite"
                    );
                    let sleep = process_track(
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
                    record_success(transient_failure_count, consecutive_network_failures);
                    return PollIteration::Sleep { seconds: sleep };
                }
                // Issue #790: the 304 fast path skips process_track, so
                // the availability session's own 4-minute clock has to be
                // wound here too — otherwise the steady state of a long
                // episode, DJ set or live stream only ever re-arms on the
                // 5-minute keepalive, at or past the fade boundary.
                let availability_backoff = rearm_availability_after_304(
                    app,
                    state,
                    last_track_key,
                    last_poll_instant,
                    &config,
                    gate_blocks_304_rearm(gated_track_key.as_deref(), last_track_key.as_deref()),
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
            crate::tray::note_playback_modes(now.context.shuffle, now.context.repeat);
            // Issue #344: debug, not info — title/artist at info
            // level land verbatim in the diagnostics `recent_logs`
            // tail (a paste-able support artifact). No raw track
            // metadata there.
            log::debug!(
                "[POLLING] poll_once: track found - {} by {}",
                now.media.title,
                now.media.artist
            );
            let sleep_duration = process_track(
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
            record_success(transient_failure_count, consecutive_network_failures);
            PollIteration::Sleep {
                seconds: sleep_duration,
            }
        }
        Ok(None) => {
            *LAST_NOW_PLAYING.lock() = None;
            log::info!("[POLLING] poll_once: no track playing");
            let no_track_backoff = handle_no_track(
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
            record_success(transient_failure_count, consecutive_network_failures);
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
            let mut backoff_secs = with_jitter(ERROR_RETRY_INTERVAL_SECONDS);

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
                            log::info!("[POLLING] poll_once: token refresh SUCCESS, retrying");
                            let committed = match cas_refresh_or_discard(
                                "spotify",
                                &mut *state.tokens.spotify_mut(),
                                &pre_refresh_access_token,
                                || Ok::<_, SpotifyApiError>(new_tokens.clone()),
                                |t| &t.access_token,
                            ) {
                                CasOutcome::Committed(_) => true,
                                CasOutcome::Discarded { .. } => false,
                                CasOutcome::RefreshFailed(_) => {
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
                                        *LAST_NOW_PLAYING.lock() = Some(now.clone());
                                        if playback_source.last_poll_was_not_modified() {
                                            if let Some(now_for_rewrite) =
                                                config_flip_rewrite_track(last_track_key, &config)
                                            {
                                                log::info!(
                                                    "[POLLING] poll_once: 304 but status config changed mid-track, forcing one rewrite"
                                                );
                                                let sleep = process_track(
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
                                                record_success(
                                                    transient_failure_count,
                                                    consecutive_network_failures,
                                                );
                                                return PollIteration::Sleep { seconds: sleep };
                                            }
                                            let availability_backoff = rearm_availability_after_304(
                                                app,
                                                state,
                                                last_track_key,
                                                last_poll_instant_retry,
                                                &config,
                                                gate_blocks_304_rearm(
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
                                            now.context.shuffle,
                                            now.context.repeat,
                                        );
                                        log::debug!(
                                            "[POLLING] poll_once: retry track found - {} by {}",
                                            now.media.title,
                                            now.media.artist
                                        );
                                        let _sleep = process_track(
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
                                        record_success(
                                            transient_failure_count,
                                            consecutive_network_failures,
                                        );
                                        return PollIteration::Sleep { seconds: _sleep };
                                    }
                                    Ok(None) => {
                                        *LAST_NOW_PLAYING.lock() = None;
                                        log::info!("[POLLING] poll_once: retry no track");
                                        let no_track_backoff = handle_no_track(
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
                                        record_success(
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
                            if matches!(refresh_err, SpotifyApiError::InvalidGrant) {
                                log::warn!("[POLLING] poll_once: Spotify refresh token invalid (invalid_grant), discarding tokens and requiring reconnect");
                                *state.tokens.spotify_mut() = None;
                                if let Err(persist_err) = token_io::persist_tokens(state, app) {
                                    log::warn!(
                                        "[POLLING] poll_once: failed to persist cleared Spotify tokens: {}",
                                        persist_err
                                    );
                                }
                                let _ = app.emit("spotify-reconnect-required", json!(null));
                                let _ = app.emit("reconnect-required", json!(null));
                            }
                            final_err = crate::sources::SourceError::Auth(refresh_err.to_string());
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
                if let Some(retry_after) = extract_retry_after(&final_err.to_string()) {
                    backoff_secs = spotify_backoff_secs_retry_after(retry_after);
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
                if let Some(iteration) = transient_outcome(*transient_failure_count) {
                    log::error!(
                        "[POLLING] poll_once: {} consecutive auth failures, exiting and requiring reconnect",
                        TRANSIENT_FAILURE_EXIT_THRESHOLD
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
                if *consecutive_network_failures >= NETWORK_FAILURE_THRESHOLD {
                    log::warn!(
                        "[POLLING] poll_once: {} consecutive network failures, backing off (polling continues, no reconnect)",
                        *consecutive_network_failures
                    );
                    backoff_secs =
                        backoff_secs.max(network_failure_backoff(*consecutive_network_failures));
                }
            }

            emit_error(
                app,
                "spotify",
                format!("Failed to get currently playing: {}", final_err),
                ErrorSeverity::Warning,
            );
            interruptible_sleep(stop_rx, backoff_secs, "backoff sleep", mode)
        }
    }
}

/// Finding PollCore#0 (issue #568): the single place that resets BOTH
/// consecutive-failure counters. One helper so a success can never clear one
/// counter and leave the other primed — a stale network streak would then
/// survive healthy iterations and jump straight to the capped backoff.
fn record_success(transient_failure_count: &mut u8, consecutive_network_failures: &mut u8) {
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
fn is_auth_failure(err: &SpotifyApiError) -> bool {
    matches!(
        err,
        SpotifyApiError::ExpiredToken | SpotifyApiError::InvalidGrant
    )
}

/// Issue #262 (finding PollCore#0, issue #568): the five-strikes reconnect
/// decision, extracted as a pure function of the counter so the threshold
/// semantics are testable without driving the whole `run()` error path.
/// Returns `Some(Break)` exactly when the count has reached
/// `TRANSIENT_FAILURE_EXIT_THRESHOLD`, and `None` below it so the caller
/// keeps retrying after emitting its warning. The counter is only bumped for
/// auth failures (see [`is_auth_failure`]) and is reset by any success.
fn transient_outcome(count: u8) -> Option<PollIteration> {
    if count >= TRANSIENT_FAILURE_EXIT_THRESHOLD {
        Some(PollIteration::Break)
    } else {
        None
    }
}

/// Record a no-track outcome. The ONLY place `consecutive_pauses` is
/// incremented in response to a no-track result.
fn record_no_track_outcome(
    consecutive_pauses: &mut u8,
    config: &Option<crate::config::AppConfig>,
) -> PollIteration {
    let no_track_sleep = pause_backoff(
        *consecutive_pauses,
        config_default_interval(config),
        config_pause_backoff_max(config),
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
    config: &Option<crate::config::AppConfig>,
) -> PollIteration {
    log::info!("[POLLING] poll_once: 304 Not Modified, skipping parse/format/tray work");
    record_success(transient_failure_count, consecutive_network_failures);
    if last_track_key.is_some() {
        *consecutive_pauses = 0;
        // Finding PollCore#6 (issue #573): this arm dominates idle runtime, so
        // it must honour the configured bounds too — pre-fix it slept the raw
        // `default_interval_seconds`, which `clamp_polling` permits to exceed
        // `max_interval_seconds` (e.g. default 120 / max 60), silently
        // violating the "Max interval (s)" setting on the most common path.
        return PollIteration::Sleep {
            seconds: clamp_poll_interval(config_default_interval(config), config),
        };
    }
    record_no_track_outcome(consecutive_pauses, config)
}

fn interruptible_sleep(
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

pub(crate) enum CasOutcome<T, E> {
    Committed(T),
    Discarded { current: Option<T> },
    RefreshFailed(E),
}

/// Generic over the refresh error type `E` so each caller keeps its
/// provider's typed error (`SpotifyApiError` / `TeamsApiError`) for the
/// re-auth policy, instead of a pre-stringified message.
pub(crate) fn cas_refresh_or_discard<T, E, F, G>(
    label: &str,
    lock: &mut Option<T>,
    pre_refresh_access_token: &str,
    refresh_fn: F,
    access_token_of: G,
) -> CasOutcome<T, E>
where
    T: Clone,
    F: FnOnce() -> Result<T, E>,
    G: FnOnce(&T) -> &str,
{
    let new_tokens = match refresh_fn() {
        Ok(t) => t,
        Err(e) => return CasOutcome::RefreshFailed(e),
    };

    // Issue #180: this helper must NEVER persist tokens itself. Callers pass
    // `&mut *state.tokens.X_mut()` — a reborrow of the parking_lot write
    // guard, which stays alive for the whole call statement. Persisting here
    // would re-lock the SAME RwLock for reading (token_io::persist_tokens)
    // while the write guard is still held; parking_lot has no same-thread
    // reentrancy detection, so write→read on the same lock from the same
    // thread parks forever on every successful refresh. The call sites
    // therefore persist in a statement AFTER this call returns, when the
    // guard is provably dropped.
    let committed = {
        if lock.as_ref().map(access_token_of) == Some(pre_refresh_access_token) {
            *lock = Some(new_tokens.clone());
            true
        } else {
            log::warn!(
                "[POLLING] poll_once: cas_refresh_or_discard: {} state changed during refresh, discarding result",
                label
            );
            false
        }
    };

    if committed {
        CasOutcome::Committed(new_tokens)
    } else {
        let current = lock.clone();
        CasOutcome::Discarded { current }
    }
}

fn get_spotify_credentials(config: &Option<crate::config::AppConfig>) -> (String, String) {
    let client_id = config
        .as_ref()
        .map(|c| c.spotify.client_id.clone())
        .unwrap_or_default();
    let client_secret = crate::keychain::peek_spotify_client_secret().unwrap_or_default();
    (client_id, client_secret)
}

/// What the proactive Spotify refresh should do this iteration (issue #296).
#[derive(Debug, PartialEq, Eq)]
enum SpotifyRefreshPlan {
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
fn spotify_refresh_plan(
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
/// `invalid_grant`, or a 401 `ExpiredToken` — means re-auth. `Transient`
/// (network/5xx), `RateLimited`, `Forbidden` and `Other(400, …)` are
/// recoverable states that must keep the session and retry later; a single
/// dropped connection must not end Teams sync.
fn teams_refresh_requires_reauth(e: &TeamsApiError) -> bool {
    matches!(
        e,
        TeamsApiError::InvalidGrant | TeamsApiError::ExpiredToken(_)
    )
}

/// True when the Available-presence session should be re-armed (issue
/// #3.0-P1): no arm yet, or the last arm is at least
/// `AVAILABILITY_REARM_SECONDS` old. An `Available` session TIMES OUT
/// after 5 minutes (non-configurable; a distinct clock from
/// `expirationDuration`), so the re-arm cadence must be strictly inside
/// that window (4 min < 5 min).
fn should_rearm_availability(last_arm: Option<Instant>, now: Instant) -> bool {
    match last_arm {
        Some(arm) => now.duration_since(arm).as_secs() >= AVAILABILITY_REARM_SECONDS,
        None => true,
    }
}

/// Finding #634 (issue #634): whether the presence session must be (re-)armed
/// now. A DIFFERENT desired pair arms immediately — a rule that starts or stops
/// matching must move the bubble on the next iteration, not after the cadence
/// tick — while an unchanged pair keeps the [`should_rearm_availability`]
/// cadence.
fn should_arm_presence(
    armed: Option<&PresencePair>,
    desired: &PresencePair,
    last_arm: Option<Instant>,
    now: Instant,
) -> bool {
    armed != Some(desired) || should_rearm_availability(last_arm, now)
}

/// Finding #636 (issue #636): the `expirationDuration` to request for a session
/// armed now — the remaining listening time plus one re-arm period of slack,
/// clamped into the documented `PT5M..PT4H` window.
///
/// Pre-fix the app always asked for `PT4H`, so a crash, a force-quit or a
/// machine sleep left the user green for four hours after the music stopped.
/// A live/unknown-position stream has no remaining time to bound the session
/// with (issue #165), so it keeps `PT4H`.
fn presence_expiration_duration(remaining_ms: Option<u64>) -> String {
    match remaining_ms {
        None => "PT4H".to_string(),
        Some(remaining) => {
            let seconds = (remaining / 1000)
                .saturating_add(AVAILABILITY_REARM_SECONDS)
                .clamp(
                    PRESENCE_EXPIRATION_MIN_SECONDS,
                    PRESENCE_EXPIRATION_MAX_SECONDS,
                );
            format!("PT{}S", seconds)
        }
    }
}

/// Finding #635 (issue #635): the read-before-write decision — does the status
/// message currently on Teams belong to the USER rather than to us?
///
/// POLICY. A live status message blocks our write when every one of these holds:
///
/// 1. `teams.respect_manual_status` is on (default), and
/// 2. we actually read the presence this iteration — with no sample the check
///    fails OPEN and the write proceeds exactly as in 4.5, and
/// 3. the live content is non-empty, and
/// 4. the message is not already expiring — a message whose `expiryDateTime`
///    has lapsed is stale (our placeholders always carry a near-term expiry,
///    so this is also what stops a leftover from a previous run from blocking
///    forever), and
/// 5. the content is not byte-identical (trimmed) to a message this process
///    last posted — `last_posted_status` for playing/replacement text,
///    `last_posted_placeholder` for the "Paused"/"Nothing playing" clears.
///
/// COST. Zero extra Graph calls under the default configuration: the sample is
/// the one the presence gate already fetches per track change (and every
/// `AVAILABILITY_REARM_SECONDS` mid-track). Only a user who turned the presence
/// gate OFF while leaving this check on pays one extra `getPresence` per gate
/// point, because the read is what makes the decision possible.
fn manual_status_blocks_write(
    respect_manual_status: bool,
    presence: Option<&crate::teams::PresenceInfo>,
    last_posted_status: Option<&str>,
    last_posted_placeholder: Option<&str>,
    now: chrono::DateTime<Utc>,
) -> bool {
    if !respect_manual_status {
        return false;
    }
    let Some(message) = presence.and_then(|p| p.status_message.as_ref()) else {
        return false;
    };
    let content = message.content.trim();
    if content.is_empty() {
        return false;
    }
    if message.expires_at.is_some_and(|expiry| expiry <= now) {
        return false;
    }
    let ours = |candidate: Option<&str>| candidate.is_some_and(|text| text.trim() == content);
    !(ours(last_posted_status) || ours(last_posted_placeholder))
}

/// The single presence-gate decision for one `getPresence` sample (issues
/// #3.0-P2/#635/#637): the reason the write must be suppressed, or `None` when
/// it may proceed.
///
/// ORDER is the precedence the user sees on the Dashboard chip: a
/// busy/meeting/out-of-office presence first (the more specific real-world
/// state), then the OS-level presentation signal (issue #872, lowest of the
/// presence-class reasons but never outranking busy or in-a-call), then a
/// status message the user wrote by hand, then the desktop-idle reading
/// (issue #873, lowest of all — only ever blocks a write that nothing else
/// has already blocked).
#[allow(clippy::too_many_arguments)]
fn presence_gate_decision(
    presence: &crate::teams::PresenceInfo,
    presence_gate_enabled: bool,
    gate_when_out_of_office: bool,
    respect_manual_status: bool,
    last_posted_status: Option<&str>,
    last_posted_placeholder: Option<&str>,
    now: chrono::DateTime<Utc>,
    // Issue #872: OS-level presentation state. `PresentationState::Unknown`
    // (Linux/macOS, or a Windows probe error) collapses to an empty reason
    // — the gate stays off and the rest of the decision runs unchanged.
    presentation_state: crate::platform::focus::PresentationState,
    gate_when_presenting: bool,
    // Issue #873: desktop-idle reading. `None` (Linux/macOS, or a
    // Windows probe error) keeps the idle gate off. The threshold is
    // checked in `process_track`; this function only sees the resolved
    // "the threshold was crossed" boolean.
    idle: bool,
) -> Option<String> {
    if presence_gate_enabled {
        let reason = presence_gate_reason(presence, gate_when_out_of_office);
        if !reason.is_empty() {
            return Some(reason);
        }
        // Issue #872: the OS-level presentation signal sits BELOW
        // `presence_gate_reason` so it can never outrank busy or in-a-call
        // (those are the more specific real-world states the Graph sample
        // already names). An `Unknown` probe answer collapses to an empty
        // reason — failing open. The opt-in is the user's, not the
        // installer's, so the default `false` leaves 4.7 behaviour
        // byte-identical.
        if gate_when_presenting {
            let reason = presentation_state.gate_reason();
            if !reason.is_empty() {
                return Some(reason.to_string());
            }
        }
    }
    if manual_status_blocks_write(
        respect_manual_status,
        Some(presence),
        last_posted_status,
        last_posted_placeholder,
        now,
    ) {
        return Some(GATE_REASON_MANUAL_STATUS.to_string());
    }
    // Issue #873: the idle gate is the LOWEST precedence of all —
    // only ever blocks a write nothing else already blocked. The
    // probe is checked in `process_track`; `idle` here is just the
    // resolved "threshold crossed" boolean. When the threshold is
    // `0` the caller passes `false` and the feature stays off, so
    // an untouched config behaves exactly as today.
    if idle {
        return Some(GATE_REASON_IDLE.to_string());
    }
    None
}

/// Finding #637 (issue #637): whether the out-of-office reason participates in
/// the gate this iteration.
///
/// Opt-in through `teams.gate_when_out_of_office`, and overridable per rule: a
/// rule that carries its own presence action is an explicit instruction for
/// this track/window, so it wins over the OOO default (finding #634). Busy /
/// Do-Not-Disturb / in-a-call always gate regardless.
fn ooo_gate_enabled(config: &Option<AppConfig>, rule_has_presence_action: bool) -> bool {
    !rule_has_presence_action
        && config
            .as_ref()
            .map(|c| c.teams.gate_when_out_of_office)
            .unwrap_or(false)
}

/// Issue #432: quiet-hours evaluation. `now_minutes` is local minutes-since-
/// midnight and `weekday` the ISO weekday number 1 (Mon)..=7 (Sun), passed
/// in so the pure predicate stays unit-testable without clock injection.
/// An entry matches when it is enabled, the weekday filter passes (empty =
/// every day), and the time falls in `[start, end)` — with wrap-around
/// (e.g. 22:00→07:00) handled as `now >= start || now < end`.
/// Minutes are clamped to 0..=1439 so a hand-edited config can't wedge
/// the comparison.
fn quiet_hours_active(
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
/// without clock injection. An entry matches when it is enabled, the weekday
/// filter passes (empty = every day), and the time falls in `[start, end)` —
/// with wrap-around (e.g. 22:00→07:00) handled as `now >= start || now < end`.
/// Minutes are clamped to 0..=1439 so a hand-edited config can't wedge the
/// comparison.
fn matching_quiet_hours(
    rules: &crate::config::StatusRulesConfig,
    now_minutes: u16,
    weekday: u8,
) -> Option<&crate::config::QuietHoursEntry> {
    rules
        .quiet_hours
        .iter()
        .find(|entry| quiet_entry_contains(entry, now_minutes, weekday))
}

/// Whether ONE quiet-hours entry is active at the given local time: enabled, the
/// weekday filter passes (empty = every day), and the time falls in
/// `[start, end)` — with wrap-around (e.g. 22:00→07:00) handled as
/// `now >= start || now < end`. Minutes are clamped to 0..=1439 so a hand-edited
/// config cannot wedge the comparison, and a zero-length window (`start == end`)
/// matches nothing. Extracted (S4, issue #672) so the "pause polling" gate can
/// ask the question per entry instead of only of the first match.
fn quiet_entry_contains(
    entry: &crate::config::QuietHoursEntry,
    now_minutes: u16,
    weekday: u8,
) -> bool {
    let now = now_minutes.min(1439);
    if !entry.enabled {
        return false;
    }
    if !entry.days.is_empty() && !entry.days.contains(&weekday) {
        return false;
    }
    let start = entry.start_minutes.min(1439);
    let end = entry.end_minutes.min(1439);
    if start == end {
        return false;
    }
    if start < end {
        now >= start && now < end
    } else {
        now >= start || now < end
    }
}

/// Finding PollCore#1 (issue #569): whether the mid-track quiet-hours ENTRY
/// must gate the current track — quiet hours are active and this track has not
/// been gated yet (the `!= Some(track_key)` arm is what keeps the gate
/// idempotent: once recorded, the #380 re-check block owns the decision). Pure
/// so the mid-track entry semantics are testable without an `AppHandle`.
fn quiet_gate_entry_due(
    quiet_active: bool,
    gated_track_key: Option<&str>,
    track_key: &str,
) -> bool {
    quiet_active && gated_track_key != Some(track_key)
}

/// Issue #432: local clock projection for [`quiet_hours_active`].
/// Minute-of-day plus ISO weekday (`number_from_monday`, 1..=7).
fn local_minutes_and_weekday() -> (u16, u8) {
    use chrono::{Datelike, Timelike};
    let now = chrono::Local::now();
    let minutes = (now.hour() as u16 * 60 + now.minute() as u16).min(1439);
    (minutes, now.weekday().number_from_monday() as u8)
}

/// Issue #432 / finding PollCore#2 (issue #570): quiet-hours evaluation on the
/// current local clock — the read-side twin of the hoisted `quiet_active`
/// binding in `process_track`, for the path (`handle_no_track`) that has no
/// track to match a rule against and no hoisted decision to consult.
fn quiet_hours_active_now(config: &Option<crate::config::AppConfig>) -> bool {
    let (now_minutes, weekday) = local_minutes_and_weekday();
    // Issue #869: the active profile's `track_rules` overlay does NOT
    // touch `quiet_hours`, so quiet-hours resolution still reads the
    // base config here — but we route through `effective_config` so a
    // future profile overlay that does touch quiet hours lands on the
    // same code path without a second migration step.
    config.as_ref().is_some_and(|c| {
        let effective = crate::config::effective_config(c);
        quiet_hours_active(&effective.status_rules, now_minutes, weekday)
    })
}

/// Whether the previous iteration was skipped by [`quiet_pause_iteration`], so
/// the pause and the resume are each logged exactly once. The DECISION is never
/// cached — it is re-derived from the local clock on every iteration, which is
/// what lets the window end by itself.
static QUIET_PAUSE_ACTIVE: AtomicBool = AtomicBool::new(false);

/// S4 (issue #672): the quiet-hours "pause polling" gate.
///
/// Returns the sleep duration for an iteration the ACTIVE quiet-hours entry —
/// the first one whose window contains the local clock, the same entry
/// [`matching_quiet_hours`] hands the write decision — asks to skip entirely.
/// `None` means poll normally.
///
/// The polling driver consults this BEFORE it loads the write clocks and before
/// it runs an iteration, so a skipped iteration issues no Spotify/Graph request
/// and moves no keepalive/debounce clock. The thread is never stopped or
/// parked: a parked thread could not notice the window ending, so the window is
/// re-evaluated every iteration and the pause/resume transitions are logged
/// once each.
pub(crate) fn quiet_pause_iteration(config: &Option<AppConfig>) -> Option<u64> {
    let (now_minutes, weekday) = local_minutes_and_weekday();
    let decision = quiet_pause_at(config, now_minutes, weekday);
    let was_paused = QUIET_PAUSE_ACTIVE.swap(decision.is_some(), Ordering::Relaxed);
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
fn quiet_pause_at(config: &Option<AppConfig>, now_minutes: u16, weekday: u8) -> Option<(u64, u16)> {
    // Issue #869: route through `effective_config` so a profile
    // overlay that touches quiet hours (or the polling interval)
    // lands on the same code path without a second migration step.
    let cfg = config.as_ref().map(crate::config::effective_config)?;
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
fn quiet_pause_log_line(
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
fn format_minutes_of_day(minutes: u16) -> String {
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
/// lets the snooze end by itself.
static SNOOZE_ACTIVE: AtomicBool = AtomicBool::new(false);

/// S9 (issue #677): the tray snooze gate.
///
/// The polling driver consults this BEFORE the quiet-hours gate, before it
/// loads the write clocks and before it runs an iteration, so a snoozed
/// iteration issues no Spotify/Graph request and moves no keepalive/debounce
/// clock. The thread is never stopped or parked — a parked thread could not
/// notice the deadline — so the gate is re-evaluated every iteration and the
/// pause/resume transitions are logged once each.
///
/// The sleep is the configured ceiling, exactly as the quiet-hours pause uses
/// it, so resuming can overshoot the deadline by at most one interval. That
/// bound is deliberate: a shorter sleep would mean waking (and re-reading the
/// clock) more often than a normal poll, for a deadline the user set in
/// minutes.
pub(crate) fn snooze_gate(config: &Option<AppConfig>) -> SnoozeGate {
    let now = Utc::now();
    if let Some((seconds, deadline)) = snooze_pause_at(config, now) {
        if !SNOOZE_ACTIVE.swap(true, Ordering::Relaxed) {
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
        if SNOOZE_ACTIVE.swap(false, Ordering::Relaxed) {
            log::info!("[POLLING] snooze: polling resumed");
        } else {
            log::info!("[POLLING] snooze: the stored deadline has already passed — clearing it");
        }
        return SnoozeGate::Expired;
    }
    SNOOZE_ACTIVE.store(false, Ordering::Relaxed);
    SnoozeGate::Inactive
}

/// [`snooze_gate`]'s decision with an explicit clock:
/// `Some((sleep_seconds, deadline))` while a snooze is active. Pure, so the
/// skip decision and the sleep value are unit-testable without a Tauri runtime.
///
/// The sleep is `polling.max_interval_seconds`, floored at 1 s so a hand-edited
/// 0 cannot spin the thread — the same floor [`quiet_pause_at`] applies.
fn snooze_pause_at(config: &Option<AppConfig>, now: DateTime<Utc>) -> Option<(u64, DateTime<Utc>)> {
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
fn snooze_expired(config: &Option<AppConfig>, now: DateTime<Utc>) -> bool {
    config
        .as_ref()
        .is_some_and(|c| crate::config::snooze_expired_deadline(c, now))
}

/// `HH:MM` of a deadline in the given zone, for the pause log line (4.7.0, S9).
/// Local wall-clock time, because that is the clock the user set the snooze
/// against — the stored value is UTC and would read as an arbitrary hour.
fn snooze_deadline_hhmm<Tz: TimeZone>(deadline: DateTime<Utc>, tz: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    deadline.with_timezone(tz).format("%H:%M").to_string()
}

/// The pause log line: until when, and how long is left (4.7.0, S9).
fn snooze_pause_log_line(local_hhmm: &str, minutes_left: i64) -> String {
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
    let Some(current) = guard.as_ref().cloned() else {
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
            *guard = Some(next);
        }
        Err(e) => log::warn!(
            "[POLLING] snooze: could not clear the expired deadline ({}); retrying next iteration",
            e
        ),
    }
}

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
struct RuleDecision {
    /// The `presence-gated` reason of the matched rule
    /// ([`GATE_REASON_QUIET_HOURS`] / [`GATE_REASON_TRACK_RULE`]); `None` when
    /// no enabled rule matched.
    reason: Option<&'static str>,
    /// Non-empty replacement text to post instead of the formatted status.
    replacement: Option<String>,
    /// The `setPresence` pair the matched rule wants armed (finding #634).
    /// `None` = the rule does not touch presence.
    presence: Option<PresencePair>,
    /// Issue #866: the `setUserPreferredPresence` pair to arm in lieu of
    /// `presence` when the rule does not carry its own pair. `None` when the
    /// feature is off, the user's manual status is in force, or the rule
    /// already names a pair (rule presence wins). The two fields cannot both
    /// be `Some` — `decision_from` keeps the invariant.
    preferred_presence: Option<PresencePair>,
}

impl RuleDecision {
    /// Whether the matched rule suppresses the write: a matched rule with no
    /// replacement text. Suppression is the 4.5 semantics, now shared by quiet
    /// hours and track rules.
    fn suppresses(&self) -> bool {
        self.reason.is_some() && self.replacement.is_none()
    }
}

/// The rule decision for an artist/title pair on the CURRENT local clock. The
/// no-track clear path passes empty strings, so only quiet hours and match-all
/// rules (both substrings empty) can suppress a clear.
fn rule_gate(config: &Option<AppConfig>, artist: &str, title: &str) -> RuleDecision {
    let (now_minutes, weekday) = local_minutes_and_weekday();
    rule_gate_at(config, now_minutes, weekday, artist, title)
}

/// [`rule_gate`] with an explicit clock, so the mid-track re-check can
/// re-project it (a long-lived track spans quiet-hours boundaries) without a
/// second live-clock read.
///
/// Precedence is quiet hours first, then track rules — the same order the
/// suppression decision has always used.
fn rule_gate_at(
    config: &Option<AppConfig>,
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
fn rule_gate_at_with_ctx(
    config: &Option<AppConfig>,
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
    let effective = crate::config::effective_config(cfg);
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
    if let Some(entry) = matching_quiet_hours(&effective.status_rules, now_minutes, weekday) {
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
fn decision_from_rule(
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
fn decision_from(
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

/// The single emitter for the `presence-gated` event (#3.0-P2/#432/#569/#570),
/// so the payload shape cannot drift between the five sites that gate a write
/// (playing gate, mid-track quiet entry, paused clear, no-track clear, tray).
/// `availability`/`activity` are empty for the time- and rule-based reasons,
/// which carry no Graph presence sample.
/// The `presence-paused` payload (finding D7, issue #690).
///
/// A cross-slice contract: the Dashboard reads `status` to keep the track card
/// and show the paused state, so a rename here is a silent break there. Built by
/// a function rather than inline so the shape can be asserted (review round 2,
/// item 5).
fn presence_paused_payload(status: &str) -> serde_json::Value {
    json!({ "status": status })
}

/// The `playback-state-changed` payload (finding D6, issue #689).
///
/// Cross-slice contract: the Dashboard reads `is_playing` and the tray re-seeds
/// from `track_key`. Exactly these two fields (review round 2, item 5).
fn playback_state_changed_payload(is_playing: bool, track_key: &str) -> serde_json::Value {
    json!({ "is_playing": is_playing, "track_key": track_key })
}

fn emit_presence_gated(app: &AppHandle, reason: &str, availability: &str, activity: &str) {
    let _ = app.emit(
        "presence-gated",
        json!({
            "reason": reason,
            "availability": availability,
            "activity": activity,
            "timestamp": Utc::now().to_rfc3339()
        }),
    );
    // Issue #877: append to the bounded decision history. The gate
    // reason is the documented wire shape; the track fingerprint is
    // pulled off the `state.polling.current_track()` snapshot so an
    // Activity card entry can pin the gate to the track it targeted.
    let track_fingerprint = super::state::current_track_fingerprint();
    crate::history::append(
        crate::history::PresenceHistoryEntry {
            at: Utc::now(),
            kind: "presence-gated".to_string(),
            note: format!(
                "gated ({} {}/{})",
                reason,
                if availability.is_empty() {
                    "-"
                } else {
                    availability
                },
                if activity.is_empty() { "-" } else { activity }
            ),
            track_fingerprint,
            posted_status: None,
            gate_reason: Some(reason.to_string()),
        },
        // The poller hands the AppConfig through the call chain; the
        // helper lives at the call site that owns the snapshot, so the
        // helper does not have it. `None` is the documented OFF mirror
        // path — the helper's `Option<&AppConfig>` parameter already
        // handles it.
        None,
    );
}

/// Arm (or re-arm) a Teams presence session (findings #634/#636, issues #634/
/// #636) through the shared cadence clocks, emitting
/// `presence-availability-updated` on success. Returns extra backoff seconds to
/// fold into the next poll (0 unless Graph throttled the call).
///
/// The single setPresence call site for the polling loop — rules and the
/// default "listening" session both funnel through here, so the pair, the
/// expiration bound and the re-arm cadence cannot drift apart.
fn arm_presence_session(
    app: &AppHandle,
    access_token: &str,
    pair: &PresencePair,
    expiration_duration: &str,
    label: &str,
    armed: &mut Option<PresencePair>,
    last_availability_arm: &mut Option<Instant>,
) -> u64 {
    let now = Instant::now();
    if !should_arm_presence(armed.as_ref(), pair, *last_availability_arm, now) {
        return 0;
    }
    match set_teams_presence(
        access_token,
        &pair.availability,
        &pair.activity,
        expiration_duration,
    ) {
        Ok(_) => {
            *armed = Some(pair.clone());
            *last_availability_arm = Some(now);
            // Finding D1 (issue #684): this session now has a live presence
            // session on Teams — record it in the exit snapshot, which
            // survives the loop's exit-tail clock reset.
            super::state::record_armed_presence(Some((&pair.availability, &pair.activity, label)));
            let _ = app.emit(
                "presence-availability-updated",
                json!({
                    "available": true,
                    "label": label,
                    "timestamp": Utc::now().to_rfc3339()
                }),
            );
            0
        }
        Err(e) => {
            log::error!(
                "[POLLING] failed to set Teams availability ({}): {}",
                label,
                e
            );
            // Issue #154: a throttled set extends the next poll to the
            // server-directed delay.
            rate_limit_sleep_secs(&e)
        }
    }
}

/// Clear the app's presence session (issues #3.0-P1/#636): `clearPresence`
/// 404 = the session is already gone = success. Returns extra backoff seconds
/// (0 unless Graph throttled the call). No-op when nothing of ours is armed.
fn clear_presence_session(
    app: &AppHandle,
    access_token: &str,
    label: &str,
    armed: &mut Option<PresencePair>,
    last_availability_arm: &mut Option<Instant>,
) -> u64 {
    if last_availability_arm.is_none() {
        return 0;
    }
    match clear_teams_presence(access_token) {
        Ok(_) => {
            *armed = None;
            *last_availability_arm = None;
            // Finding D1 (issue #684): no session of ours is armed any more.
            super::state::record_armed_presence(None);
            let _ = app.emit(
                "presence-availability-updated",
                json!({
                    "available": false,
                    "label": label,
                    "timestamp": Utc::now().to_rfc3339()
                }),
            );
            0
        }
        Err(e) => {
            log::error!("[POLLING] failed to clear Teams availability: {}", e);
            rate_limit_sleep_secs(&e)
        }
    }
}

/// Issue #866: the app's preferred-presence session. Distinct from the
/// ephemeral `setPresence` session — `setUserPreferredPresence` has its own
/// per-user rate limit and its own document-mandated pairs. The exit
/// snapshot can carry BOTH at once, so the cleanup arm stays simple.
#[derive(Debug, Clone)]
pub(crate) struct PreferredPresenceSession {
    pub pair: PresencePair,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub label: &'static str,
}

static PREFERRED_PRESENCE_SESSION: std::sync::Mutex<Option<PreferredPresenceSession>> =
    std::sync::Mutex::new(None);

pub(crate) fn load_preferred_presence_session() -> Option<PreferredPresenceSession> {
    PREFERRED_PRESENCE_SESSION
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
}

pub(crate) fn record_preferred_presence_session(session: Option<PreferredPresenceSession>) {
    if let Ok(mut guard) = PREFERRED_PRESENCE_SESSION.lock() {
        *guard = session;
    }
}

/// Issue #866: drive the `setUserPreferredPresence` Graph endpoint with the
/// supplied pair and expiration, recording the session locally so the
/// poll-loop expiry check can `clearUserPreferredPresence` it back to the
/// user's natural bubble. Distinct from [`arm_presence_session`] — different
/// endpoint, no `sessionId`, no `should_arm_presence` debouncing (each POST
/// IS the debounce, see [`PREFERRED_PRESENCE_REARM_SECONDS`]).
///
/// Returns extra backoff seconds (0 unless Graph throttled the call).
pub(crate) fn arm_preferred_presence_session(
    app: &AppHandle,
    access_token: &str,
    pair: &PresencePair,
    expiration_duration: &str,
    label: &str,
) -> u64 {
    let now = chrono::Utc::now();
    // Issue #866: parse the configured `PT<minutes>M` so the in-process
    // session expiry matches what we POST. The clamp guarantees the bound.
    let minutes = expiration_duration
        .trim_start_matches("PT")
        .trim_end_matches('M')
        .parse::<i64>()
        .unwrap_or(60)
        .max(5);
    let expires_at = now + chrono::Duration::minutes(minutes);
    let last = load_preferred_presence_session();
    if let Some(prev) = last.as_ref() {
        if prev.pair == *pair && prev.expires_at > now && prev.label == label {
            // Same pair, still inside the original expiry window — no need
            // to POST again. The poll loop will clear us when the window
            // lapses.
            return 0;
        }
    }
    match set_user_preferred_presence(
        access_token,
        &pair.availability,
        &pair.activity,
        expiration_duration,
    ) {
        Ok(_) => {
            // The label is the rule reason or `"Snooze preferred presence"`
            // — both stable for the lifetime of the arm, so it lives in a
            // `&'static str` and matches the `Eq` arm above.
            let label_static: &'static str = Box::leak(Box::from(label));
            record_preferred_presence_session(Some(PreferredPresenceSession {
                pair: pair.clone(),
                expires_at,
                label: label_static,
            }));
            let _ = app.emit(
                "preferred-presence-updated",
                json!({
                    "available": true,
                    "label": label,
                    "availability": pair.availability,
                    "activity": pair.activity,
                    "expires_at": expires_at.to_rfc3339(),
                    "timestamp": now.to_rfc3339()
                }),
            );
            // Issue #877: append a "preferred-presence-armed" entry so
            // the Dashboard's Activity card can correlate a rule-driven
            // preferred-presence arm with the same track the rule
            // matched.
            crate::history::append(
                crate::history::PresenceHistoryEntry {
                    at: now,
                    kind: "preferred-presence-armed".to_string(),
                    note: format!("armed ({}/{}, {})", pair.availability, pair.activity, label),
                    track_fingerprint: super::state::current_track_fingerprint(),
                    posted_status: None,
                    gate_reason: None,
                },
                None,
            );
            0
        }
        Err(e) => {
            log::error!(
                "[POLLING] failed to set Teams preferred presence ({}): {}",
                label,
                e
            );
            rate_limit_sleep_secs(&e)
        }
    }
}

/// Issue #866: clear the app's preferred-presence session. Mirrors the
/// 404-as-success contract [`clear_presence_session`] uses — Graph answers
/// 404 when no preferred presence is set, which IS the success case. No-op
/// when nothing of ours is armed.
pub(crate) fn clear_preferred_presence_session(
    app: &AppHandle,
    access_token: &str,
    label: &str,
) -> u64 {
    if load_preferred_presence_session().is_none() {
        return 0;
    }
    match clear_user_preferred_presence(access_token) {
        Ok(_) => {
            record_preferred_presence_session(None);
            let _ = app.emit(
                "preferred-presence-updated",
                json!({
                    "available": false,
                    "label": label,
                    "timestamp": Utc::now().to_rfc3339()
                }),
            );
            // Issue #877: append a "preferred-presence-cleared" entry so
            // the Activity card can pair each armed entry with its clear.
            crate::history::append(
                crate::history::PresenceHistoryEntry {
                    at: Utc::now(),
                    kind: "preferred-presence-cleared".to_string(),
                    note: format!("cleared ({label})"),
                    track_fingerprint: super::state::current_track_fingerprint(),
                    posted_status: None,
                    gate_reason: None,
                },
                None,
            );
            0
        }
        Err(e) => {
            log::error!("[POLLING] failed to clear Teams preferred presence: {}", e);
            rate_limit_sleep_secs(&e)
        }
    }
}

/// Issue #866: the per-iteration expiry tick. Runs at the head of the poll
/// loop, BEFORE the rule/snooze paths can re-arm — so an expired window
/// always clears before a re-arm can resurrect it.
fn clear_expired_preferred_presence(
    app: &AppHandle,
    access_token: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> u64 {
    let Some(session) = load_preferred_presence_session() else {
        return 0;
    };
    if session.expires_at > now {
        return 0;
    }
    log::info!(
        "[POLLING] preferred presence expired (label={}), clearing",
        session.label
    );
    clear_preferred_presence_session(app, access_token, "Preferred presence expired")
}

/// Finding #634 (issue #634): apply a matched rule's presence action on the
/// paths that return BEFORE the shared availability block (a playing write
/// suppressed by quiet hours or a track rule). Without this a suppression-only
/// rule — "while my Focus playlist plays, show me Do Not Disturb" — would move
/// nothing, because its whole point is that no status write happens.
///
/// `Some(0)` when the rule carries no presence action or `availability_sync` is
/// off (the rule action is inert then, mirroring the documented hint text).
///
/// Issue #866: a rule decision that carries a `preferred_presence` (no
/// rule-side pair of its own, but the user opted into the feature) drives the
/// `setUserPreferredPresence` arm here. The rule-side path still wins when
/// the rule names a pair of its own (`decision.presence.is_some()`) — that is
/// the user's explicit per-track instruction.
#[allow(clippy::too_many_arguments)]
fn rule_presence_backoff(
    app: &AppHandle,
    access_token: &str,
    config: &Option<AppConfig>,
    decision: &RuleDecision,
    remaining_ms: Option<u64>,
    armed: &mut Option<PresencePair>,
    last_availability_arm: &mut Option<Instant>,
) -> u64 {
    // Issue #866: preferred presence rides here even when `availability_sync`
    // is off — the user opted into a feature that is independent of the
    // per-track listening session, and the Graph endpoints do not share an
    // "off" switch. The decision is still subject to the manual-status gate
    // (resolved at the call site), so a busy/DND/meeting user is never
    // overridden.
    if let Some(preferred) = decision.preferred_presence.as_ref() {
        return arm_preferred_presence_session(
            app,
            access_token,
            preferred,
            &crate::config::preferred_presence_expiry_duration(
                config
                    .as_ref()
                    .map(|c| &c.teams)
                    .unwrap_or(&crate::config::TeamsConfig::default()),
            ),
            &format!(
                "Rule preferred presence ({}/{})",
                preferred.availability, preferred.activity
            ),
        );
    }
    let Some(pair) = decision.presence.as_ref() else {
        return 0;
    };
    if !availability_sync_enabled(config) {
        return 0;
    }
    arm_presence_session(
        app,
        access_token,
        pair,
        &presence_expiration_duration(remaining_ms),
        &format!("Rule presence ({}/{})", pair.availability, pair.activity),
        armed,
        last_availability_arm,
    )
}

/// `teams.availability_sync`, defaulted the same way `process_track` defaults
/// it (off).
fn availability_sync_enabled(config: &Option<AppConfig>) -> bool {
    config
        .as_ref()
        .map(|c| c.teams.availability_sync)
        .unwrap_or(false)
}

/// Issue #790: the default session armed while a track plays and no rule
/// overrides it — `Available`/`Available` is "Listening" in the Graph
/// vocabulary.
fn default_listening_presence() -> PresencePair {
    PresencePair {
        availability: "Available".to_string(),
        activity: "Available".to_string(),
    }
}

/// Issue #790: the shared presence-session tail of one iteration with a track
/// in hand. Extracted from `process_track` so EVERY tail that can be reached
/// while a track plays honours the session's own 4-minute
/// `AVAILABILITY_REARM_SECONDS` clock — including the identical-write skip
/// above, which returns before the shared block ever runs. An `Available`
/// session FADES after 5 minutes whatever this app POSTs, so the re-arm has to
/// ride the poll, not the status write.
///
/// Returns extra backoff seconds for `teams_backoff_secs` (0 unless Graph
/// throttled an arm/clear). No-op when `availability_sync` is off or the
/// presence gate blocked this iteration: the gate is the outer authority, so a
/// busy/DND/meeting user — or one who typed their own status — is never
/// answered with a `setPresence` of ours.
#[allow(clippy::too_many_arguments)]
fn sync_availability(
    app: &AppHandle,
    access_token: &str,
    track_is_playing: bool,
    remaining_ms: Option<u64>,
    rule: &RuleDecision,
    config: &Option<AppConfig>,
    presence_blocked: bool,
    armed_presence: &mut Option<PresencePair>,
    last_availability_arm: &mut Option<Instant>,
) -> u64 {
    // Issue #866: preferred presence is a different Graph endpoint and a
    // different cadence from the per-track `setPresence` session. Route the
    // decision here, return early, and let `arm_preferred_presence_session`
    // own the 4-minute re-arm clock. The user's manual-status window is the
    // only off-switch (already folded into `RuleDecision::preferred_presence`
    // at the `rule_gate_at` site, so by the time we get here it is `None`).
    if let Some(preferred) = rule.preferred_presence.as_ref() {
        return arm_preferred_presence_session(
            app,
            access_token,
            preferred,
            &crate::config::preferred_presence_expiry_duration(
                config
                    .as_ref()
                    .map(|c| &c.teams)
                    .unwrap_or(&crate::config::TeamsConfig::default()),
            ),
            &format!(
                "Rule preferred presence ({}/{})",
                preferred.availability, preferred.activity
            ),
        );
    }
    if !availability_sync_enabled(config) || presence_blocked {
        return 0;
    }
    // A matching rule's own pair takes precedence: it IS the user's
    // instruction for this track/window, and it applies whether the track is
    // playing or paused — leaving a quiet-hours rule clears it again so the
    // user's real state returns.
    if let Some(pair) = rule.presence.as_ref() {
        return arm_presence_session(
            app,
            access_token,
            pair,
            &presence_expiration_duration(remaining_ms),
            &format!("Rule presence ({}/{})", pair.availability, pair.activity),
            armed_presence,
            last_availability_arm,
        );
    }
    if track_is_playing {
        let listening = default_listening_presence();
        return arm_presence_session(
            app,
            access_token,
            &listening,
            &presence_expiration_duration(remaining_ms),
            "Listening (Available)",
            armed_presence,
            last_availability_arm,
        );
    }
    clear_presence_session(
        app,
        access_token,
        "Availability cleared",
        armed_presence,
        last_availability_arm,
    )
}

/// Issue #790: whether a 304 Not Modified iteration owes the availability
/// session a re-arm. A 304 with a tracked track is the steady state of a long
/// episode, DJ set or live stream: `process_track` never runs, so nothing else
/// keeps the session inside the 5-minute fade window. A 304 with no tracked
/// track is "still nothing playing" (issue #242) — nothing of ours is armed and
/// there is nothing to keep alive. Pure, so the contract is unit-testable
/// without an `AppHandle`.
fn rearm_after_304(tracked: bool, availability_sync: bool, gate_blocked: bool) -> bool {
    tracked && availability_sync && !gate_blocked
}

/// Issue #790: whether the presence-gate verdict `process_track` recorded for
/// the track on screen also blocks a re-arm on a bodyless 304.
/// `gated_track_key` holds the track key a suppressed write was recorded
/// against; equality with the key still on screen means the verdict stands. A
/// `None` on either side means no verdict was recorded, and nothing blocks the
/// arm. Pure.
fn gate_blocks_304_rearm(gated_track_key: Option<&str>, last_track_key: Option<&str>) -> bool {
    matches!(
        (gated_track_key, last_track_key),
        (Some(gated), Some(key)) if gated == key
    )
}

/// Issue #790: re-arm the session a 304-with-track steady state already has.
/// The response carries no body, so the desired pair is by definition the one
/// already armed (the default "Listening" pair when nothing is armed yet) and
/// the rule that produced it cannot be re-matched here — arm that same pair on
/// the usual cadence instead of inventing a new one.
///
/// `gate_blocked` is the verdict `process_track` recorded for this track
/// (`gated_track_key`): while it stands, the arm stays suppressed, exactly as
/// the shared tail would. Returns extra backoff seconds for the caller's sleep
/// (0 unless Graph throttled the arm).
#[allow(clippy::too_many_arguments)]
fn rearm_availability_after_304(
    app: &AppHandle,
    state: &Arc<AppState>,
    last_track_key: &Option<String>,
    last_poll_instant: Instant,
    config: &Option<AppConfig>,
    gate_blocked: bool,
    armed_presence: &mut Option<PresencePair>,
    last_availability_arm: &mut Option<Instant>,
) -> u64 {
    if !rearm_after_304(
        last_track_key.is_some(),
        availability_sync_enabled(config),
        gate_blocked,
    ) {
        return 0;
    }
    let Some(teams_tok) = teams_token_for_write(app, state) else {
        return 0;
    };
    // The 304 carries no playback body, so the remaining time comes from the
    // last stored track plus the elapsed poll interval — the same correction
    // `process_track` applies. Without a stored track (or an unknown position,
    // issue #165) it falls back to the unknown-position bound.
    let remaining_ms = state.polling.current_track().as_ref().and_then(|t| {
        let elapsed_ms = last_poll_instant.elapsed().as_millis() as u64;
        t.progress_ms
            .map(|p| t.duration_ms.saturating_sub(p.saturating_add(elapsed_ms)))
    });
    let pair = armed_presence
        .clone()
        .unwrap_or_else(default_listening_presence);
    let label = if pair == default_listening_presence() {
        "Listening (Available)".to_string()
    } else {
        format!("Rule presence ({}/{})", pair.availability, pair.activity)
    };
    arm_presence_session(
        app,
        &teams_tok.access_token,
        &pair,
        &presence_expiration_duration(remaining_ms),
        &label,
        armed_presence,
        last_availability_arm,
    )
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
fn track_rule_substring_field_matches(
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
fn glob_match_ignore_ascii_case(pattern: &str, haystack: &str) -> bool {
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
/// window contains the given local time. The semantics are
/// [`crate::config::QuietHoursEntry`]'s, field for field: an empty `days`
/// applies every day, the window is `[start, end)` with wrap-around
/// (`start > end`, e.g. 22:00→07:00) honoured, and `start == end` matches
/// nothing. `end_minutes == 1440` is the end of the day, so the default window
/// covers every minute. Issue #868: `pub(crate)` because
/// `commands::rules::explain_rules` runs the same walker the live
/// `process_track` path uses.
pub(crate) fn track_rule_schedule_matches(
    rule: &crate::config::TrackRuleEntry,
    now_minutes: u16,
    weekday: u8,
) -> bool {
    if !rule.days.is_empty() && !rule.days.contains(&weekday) {
        return false;
    }
    let now = u32::from(now_minutes.min(1439));
    let start = rule
        .start_minutes
        .min(crate::config::TRACK_RULE_DAY_MINUTES);
    let end = rule.end_minutes.min(crate::config::TRACK_RULE_DAY_MINUTES);
    if start == end {
        return false;
    }
    if start < end {
        now >= start && now < end
    } else {
        now >= start || now < end
    }
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

/// The music-note prefix on both placeholder clears. Kept out of the config
/// fields so their defaults stay plain text (`"Paused"`), which is what the
/// fixed schema in the release contract specifies.
/// The standard music-emoji prefix (`🎵`) the polling path prefixes every
/// status with. Exported `pub(crate)` so the manual-status clear path
/// (issue #870) can mirror it without copying the literal — a future
/// "make the prefix configurable" change should land in one place.
pub(crate) const MUSIC_EMOJI: &str = "\u{1F3B5}";
/// S4 (issue #672): fallbacks used when no config is loaded — the same literals
/// `config.rs` defaults to, so a config-less iteration renders exactly what a
/// default config renders.
const DEFAULT_PAUSED_STATUS_FORMAT: &str = "Paused";
const DEFAULT_STOPPED_STATUS_FORMAT: &str = "Nothing playing on Spotify";

/// S4 (issue #672): the configured paused text, falling back to the contract
/// default when the field is absent OR EMPTY. An empty text would otherwise post
/// a bare music emoji, and clearing a Settings field means "back to the
/// default", not "post nothing that identifies me".
fn paused_status_text(config: &Option<AppConfig>) -> &str {
    config
        .as_ref()
        .map(|c| c.teams.paused_status_format.as_str())
        .filter(|text| !text.is_empty())
        .unwrap_or(DEFAULT_PAUSED_STATUS_FORMAT)
}

/// [`paused_status_text`]'s no-track sibling — same empty-field contract.
fn stopped_status_text(config: &Option<AppConfig>) -> &str {
    config
        .as_ref()
        .map(|c| c.teams.stopped_status_format.as_str())
        .filter(|text| !text.is_empty())
        .unwrap_or(DEFAULT_STOPPED_STATUS_FORMAT)
}

/// S4 (issue #672): the paused-clear placeholder. The emoji is ours; the text is
/// `teams.paused_status_format` (default "Paused"), so the default renders
/// byte-identically to the pre-4.7 literal `"🎵 Paused"`.
fn paused_status_placeholder(config: &Option<AppConfig>) -> String {
    format!("{MUSIC_EMOJI} {}", paused_status_text(config))
}

/// S4 (issue #672): the no-track clear's placeholder — the same emoji contract
/// as [`paused_status_placeholder`], with `teams.stopped_status_format`
/// (default `"Nothing playing on Spotify"`). A matching rule's replacement text
/// still takes precedence over it.
fn stopped_status_placeholder(config: &Option<AppConfig>) -> String {
    format!("{MUSIC_EMOJI} {}", stopped_status_text(config))
}
/// Issue #343: fingerprint of the status-shaping config. Embedded in the
/// track change key so a filter/placeholder/format flip mid-track reads as
/// a change and forces one rewrite on the next poll, instead of leaving
/// the stale status posted until the next track change.
///
/// The `None`-config fallbacks mirror `process_track`'s exactly — a
fn status_config_fingerprint(config: &Option<crate::config::AppConfig>) -> String {
    // Issue #869: the live fingerprint reads the EFFECTIVE config
    // (active profile overlay applied). A profile change must force a
    // status rewrite on the next poll, exactly like any other config
    // flip — the contract is "any config-shaped change mid-track gets
    // one fresh write".
    let effective = config.as_ref().map(crate::config::effective_config);
    let filter = effective
        .as_ref()
        .map(|c| c.teams.profanity_filter)
        .unwrap_or(true);
    let placeholder = effective
        .as_ref()
        .map(|c| c.teams.profanity_placeholder.as_str())
        .unwrap_or(profanity::safe_placeholder_default());
    let format = effective
        .as_ref()
        .map(|c| c.teams.status_format.as_str())
        .unwrap_or("🎵 {artist} - {track} 🎧");
    // S4 (issue #672): the manual-status texts and the rule schedules are part
    // of the same key, so editing a window, a weekday set, `pause_polling` or
    // one of the two placeholder texts mid-track forces the same one-off
    // rewrite.
    // The API accessors, not the raw fields, so an empty field fingerprints the
    // same as an absent one (it renders the same) instead of forcing a rewrite.
    let paused_format = paused_status_text(config);
    let stopped_format = stopped_status_text(config);
    // Issue #432: rule edits flip the key too, so enabling/disabling a
    // rule or quiet-hours entry mid-track forces one rewrite pass instead
    // of leaving the stale gate decision until the next track change.
    // Full CONTENT (not lengths): a same-length text edit must flip the
    // key, otherwise the stale gate decision stands until the next track.
    // (User content in a change key is safe: it stays in-process, is only
    // compared, and never leaves via log/snapshot — ConfigSummary carries
    // counts only.)
    // Issue #869: the rule list reads from the EFFECTIVE config so a
    // profile's rules overlay flips the key (a profile switch is a
    // config-shaped change).
    let rules = effective.as_ref().map(|c| {
        let q: Vec<String> = c
            .status_rules
            .quiet_hours
            .iter()
            .map(|e| {
                // Finding #634: the quiet-hours replacement text and presence
                // pair are part of the decision, so editing either mid-track
                // must flip the key and force one rewrite (issue #432's
                // contract, widened to the new fields).
                format!(
                    "{}:{}-{}:{:?}:{}:{}:{}:{}",
                    e.enabled,
                    e.start_minutes,
                    e.end_minutes,
                    e.days,
                    e.replacement_status,
                    e.presence_availability,
                    e.presence_activity,
                    e.pause_polling
                )
            })
            .collect();
        let t: Vec<String> = c
            .status_rules
            .track_rules
            .iter()
            .map(|r| {
                format!(
                    "{}:{}:{}:{:?}:{}-{}:{}:{}:{}",
                    r.enabled,
                    r.artist_substring,
                    r.track_substring,
                    r.days,
                    r.start_minutes,
                    r.end_minutes,
                    r.replacement_status,
                    r.presence_availability,
                    r.presence_activity
                )
            })
            .collect();
        format!("quiet=[{}] rules=[{}]", q.join(","), t.join(","))
    });
    format!(
        "filter={filter} placeholder={placeholder} format={format} paused={paused_format} stopped={stopped_format} rules={}",
        rules.as_deref().unwrap_or("quiet=[] rules=[]")
    )
}

/// The last observed item in full — media plus episode metadata and playback
/// context (issues #580/#581).
///
/// `AppState::polling` stores only the frozen `TrackInfo`, and the issue
/// #343 config-flip rewrite runs on a 304 body-less response, so without this
/// that forced rewrite would render an episode through the music template and
/// drop every playback-context token. Kept in lockstep with
/// `AppState::polling.current_track` — written on the same genuine track
/// change, cleared by the same no-track clear — so the two can never disagree
/// about what is playing.
static LAST_NOW_PLAYING: std::sync::LazyLock<
    parking_lot::Mutex<Option<crate::spotify::NowPlaying>>,
> = std::sync::LazyLock::new(|| parking_lot::Mutex::new(None));

/// Issue #343: the change key compared against `last_track_key`. Item
/// identity — including the episode marker, so a track and an episode that
/// share a title/artist still re-key (issue #581) — plus the status-shaping
/// config fingerprint.
///
/// Deliberately NOT keyed on `progress_ms` (it advances on every poll, which
/// would rewrite the status continuously) nor on the playback context
/// (device/playlist/shuffle/repeat). A context change the template does not
/// mention still renders byte-identical text and is skipped by the issue #384
/// identical-write check, while one the template DOES mention changes the
/// text and is written by that same check — so keying on it would only add
/// redundant forced writes.
fn status_track_key(
    now: &crate::spotify::NowPlaying,
    config: &Option<crate::config::AppConfig>,
) -> String {
    let kind = match &now.episode {
        Some(episode) => format!("episode:{}:{}", episode.show_name, episode.publisher),
        None => "track".to_string(),
    };
    format!(
        "{} - {} | {} | {}",
        now.media.title,
        now.media.artist,
        kind,
        status_config_fingerprint(config)
    )
}

/// Issue #343: 304 steady-state force-rewrite. A 304 carries no body, so
/// `process_track` never runs and the change key above is never compared —
/// a config flip mid-track would stay stale until the next track change.
/// Returns the last observed item when the stored key no longer matches the
/// current item + config, so `run()` can push one fresh write through
/// `process_track`; `None` otherwise (nothing tracked, or nothing changed).
///
/// Reads `LAST_NOW_PLAYING` rather than the stored `TrackInfo` because the
/// rewrite must render the same template and the same context tokens the
/// live path would have (issue #581): the whole item, not just its media.
fn config_flip_rewrite_track(
    last_track_key: &Option<String>,
    config: &Option<crate::config::AppConfig>,
) -> Option<crate::spotify::NowPlaying> {
    let now = LAST_NOW_PLAYING.lock().clone()?;
    let expected = status_track_key(&now, config);
    if last_track_key.as_ref() != Some(&expected) {
        Some(now)
    } else {
        None
    }
}

/// Issues #370/#388: the single source of truth for a write-ready Teams
/// token — clone the stored tokens, refresh when expired (CAS-commit +
/// persist, dead-credential re-auth policy per issue #295), and hand back
/// `None` when there is nothing usable. Called from BOTH `process_track`
/// and `handle_no_track` (including the clear path) so the no-track clear
/// can no longer sail with a dead token while the track path refreshes.
fn teams_token_for_write(app: &AppHandle, state: &Arc<AppState>) -> Option<TeamsTokens> {
    let teams_tokens = state.tokens.teams().clone();
    if let Some(ref tok) = teams_tokens {
        let expired = is_teams_token_expired(tok);
        if expired {
            log::info!("[POLLING] teams_token_for_write: Teams token expired, refreshing...");

            let pre_refresh_access_token = tok.access_token.clone();

            // The refresh error stays typed (`CasOutcome<T, E>` is generic
            // over `E`) so the re-auth policy below can classify it instead
            // of string-sniffing.
            let teams_refresh_outcome = cas_refresh_or_discard(
                "teams",
                &mut *state.tokens.teams_mut(),
                &pre_refresh_access_token,
                || refresh_teams_token(tok),
                |t| &t.access_token,
            );
            match teams_refresh_outcome {
                CasOutcome::Committed(new_tokens) => {
                    // Issue #180: the write guard reborrowed into the CAS
                    // call above is dropped at the end of that statement.
                    // Persist here so the read lock inside persist_tokens
                    // (same RwLock) cannot self-deadlock.
                    if let Err(e) = token_io::persist_tokens(state, app) {
                        log::warn!(
                            "[POLLING] teams_token_for_write: failed to persist refreshed teams tokens: {}",
                            e
                        );
                    }
                    Some(new_tokens)
                }
                CasOutcome::Discarded { current } => current,
                CasOutcome::RefreshFailed(e) => {
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
                    if teams_refresh_requires_reauth(&e) {
                        log::warn!("[POLLING] teams_token_for_write: Teams refresh token is dead, discarding tokens and requiring reconnect");
                        *state.tokens.teams_mut() = None;
                        // Issue #180: the write guard in the clearing
                        // statement above dies at the end of that statement.
                        // Persist in a LATER statement, when the guard is
                        // provably dropped — persisting while it is alive
                        // would re-lock the same parking_lot RwLock for
                        // reading and self-deadlock.
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
fn debounce_active(changed: bool, last_teams_update: Option<Instant>) -> bool {
    if !changed {
        return false;
    }
    match last_teams_update {
        Some(last_update) => (last_update.elapsed().as_millis() as u64) < DEBOUNCE_MS,
        None => false,
    }
}

/// Issue #373: whether a no-track poll should attempt a Teams clear.
/// Fresh threads start with `last_track_key=None`, so the first no-track
/// poll must attempt one clear (pre-restart status would otherwise stay
/// stale); later nothing-tracked polls stay a no-op. Pure so the
/// exactly-once semantics are unit-testable; the caller consumes the flag.
fn first_no_track_attempts_clear(last_track_key: &Option<String>, first_iteration: bool) -> bool {
    last_track_key.is_some() || first_iteration
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
fn gate_recheck_due(
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
        Some(t) => now.duration_since(t).as_secs() >= AVAILABILITY_REARM_SECONDS,
        None => true,
    }
}

/// Issue #384: skip a byte-identical playing-status write while the last
/// write is still inside the keepalive window. A track/config-fingerprint
/// change (`changed`) always force-writes, as does a lapsed keepalive (so
/// the Graph expiry never lapses with no refresh in flight). Issue #873:
/// `force_resume_write` overrides the dedup exactly once — the first
/// iteration after the idle gate cleared must POST the status even when
/// the text is byte-identical to what Teams already shows, so the resume
/// surfaces to the user.
fn should_skip_identical_write(
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
        Some(t) => now.duration_since(t).as_secs() < STATUS_KEEPALIVE_SECONDS,
        None => false,
    }
}

/// Findings D3/D4 (issues #686/#687): what to do with one placeholder write —
/// the paused-track clear and the no-track clear share it, so the two paths
/// cannot drift again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlaceholderWrite {
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
fn placeholder_write_decision(
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
const NO_TRACK_GATE_KEY: &str = "no-track";

/// The `gated_track_key` transition the no-track path applies: the sentinel
/// while a suppression holds, `None` once nothing is suppressed. One helper so
/// the suppress arm and the two retiring arms cannot drift.
fn no_track_gate_key(blocked: bool) -> Option<&'static str> {
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
fn observe_presence_sample(
    respect_manual_status: bool,
    presence: &crate::teams::PresenceInfo,
    posted: Option<&str>,
    placeholder: Option<&str>,
) {
    super::state::record_manual_status_blocks(manual_status_blocks_write(
        respect_manual_status,
        Some(presence),
        posted,
        placeholder,
        Utc::now(),
    ));
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
fn paused_clear_skips_gate_read(
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
fn playback_state_changed(stored_is_playing: Option<bool>, observed_is_playing: bool) -> bool {
    stored_is_playing != Some(observed_is_playing)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn process_track(
    app: &AppHandle,
    state: &Arc<AppState>,
    config: &Option<crate::config::AppConfig>,
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
    let track_key = status_track_key(now, config);
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
    let quiet_active = quiet_hours_active_now(config);
    let rule = rule_gate(config, &track.artist, &track.title);
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
    let gate_out_of_office = ooo_gate_enabled(config, rule.presence.is_some());
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
        observe_presence_sample(respect_manual_status, presence, posted, placeholder);
        presence_gate_decision(
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
        return DEBOUNCE_RETRY_SECONDS;
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
        super::state::record_current_track_fingerprint(Some(crate::history::TrackFingerprint {
            title: track.title.clone(),
            artist: track.artist.clone(),
        }));
        // Issue #581: the config-flip rewrite path (#343) has no body to
        // re-parse, so the full item — episode metadata and playback context
        // included — is kept alongside the stored `TrackInfo`, which carries
        // neither. Written on the same condition as `current_track`, so the
        // two never disagree about what is playing.
        *LAST_NOW_PLAYING.lock() = Some(now.clone());

        let _ = app.emit(
            "spotify-track-changed",
            json!({
                "title": track.title,
                "artist": track.artist,
                "album": track.album,
                "album_art_url": track.album_art_url,
                "is_playing": track.is_playing,
                "progress_ms": track.progress_ms,
                "duration_ms": track.duration_ms
            }),
        );
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
        super::state::record_current_track_fingerprint(Some(crate::history::TrackFingerprint {
            title: track.title.clone(),
            artist: track.artist.clone(),
        }));
        *LAST_NOW_PLAYING.lock() = Some(now.clone());
        let _ = app.emit(
            "playback-state-changed",
            playback_state_changed_payload(track.is_playing, &track_key),
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
                    emit_presence_gated(app, GATE_REASON_CALENDAR, "", "");
                } else if rule.suppresses() {
                    let reason = rule.reason.unwrap_or(GATE_REASON_QUIET_HOURS);
                    log::info!(
                        "[POLLING] process_track: {} active, skipping status write",
                        reason
                    );
                    *gated_track_key = Some(track_key.clone());
                    *last_gate_check = Some(Instant::now());
                    emit_presence_gated(app, reason, "", "");
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
                                emit_presence_gated(
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
            if quiet_gate_entry_due(quiet_active, gated_track_key.as_deref(), &track_key) {
                *gated_track_key = Some(track_key.clone());
                *last_gate_check = Some(Instant::now());
                let remaining_ms =
                    corrected_progress_ms.map(|c| track.duration_ms.saturating_sub(c));
                if rule.suppresses() {
                    log::info!(
                        "[POLLING] process_track: quiet hours started mid-track, suppressing status write"
                    );
                    emit_presence_gated(
                        app,
                        rule.reason.unwrap_or(GATE_REASON_QUIET_HOURS),
                        "",
                        "",
                    );
                    // Finding #634: the suppression skips the write, but the
                    // rule's presence action still applies.
                    teams_backoff_secs = teams_backoff_secs.max(rule_presence_backoff(
                        app,
                        &teams_tok.access_token,
                        config,
                        &rule,
                        remaining_ms,
                        armed_presence,
                        last_availability_arm,
                    ));
                    return playing_track_sleep(remaining_ms, config).max(teams_backoff_secs);
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
                let (cur_minutes, cur_weekday) = local_minutes_and_weekday();
                // Issue #868: feed the album / show / device /
                // playlist-uri / duration the live path already has into
                // the rule walker so a mid-track re-check honours the
                // new conditions.
                let show_name = now
                    .episode
                    .as_ref()
                    .map(|e| e.show_name.as_str())
                    .unwrap_or("");
                let track_ctx = TrackRuleContext {
                    artist: &track.artist,
                    title: &track.title,
                    album: &track.album,
                    show: show_name,
                    device: &now.context.device,
                    playlist_uri: &now.context.playlist,
                    duration_ms: track.duration_ms,
                };
                let current_rule =
                    rule_gate_at_with_ctx(config, cur_minutes, cur_weekday, &track_ctx);
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
                    if gate_recheck_due(
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
                    return playing_track_sleep(remaining_ms, config).max(teams_backoff_secs);
                }
                if current_rule.suppresses() {
                    if gate_recheck_due(
                        *last_gate_check,
                        Instant::now(),
                        chrono::Utc::now(),
                        next_meeting_boundary,
                    ) {
                        *last_gate_check = Some(Instant::now());
                    }
                    log::debug!("[POLLING] process_track: still rule-gated, keeping suppression");
                    teams_backoff_secs = teams_backoff_secs.max(rule_presence_backoff(
                        app,
                        &teams_tok.access_token,
                        config,
                        &current_rule,
                        remaining_ms,
                        armed_presence,
                        last_availability_arm,
                    ));
                    return playing_track_sleep(remaining_ms, config).max(teams_backoff_secs);
                }
                if !presence_read_needed {
                    *gated_track_key = None;
                } else if gate_recheck_due(
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
                            match presence_gate_decision(
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
                    return playing_track_sleep(remaining_ms, config);
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
                profanity::filter_status(
                    &status_message,
                    placeholder,
                    track.is_playing,
                    extra_words,
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
                teams_backoff_secs = teams_backoff_secs.max(sync_availability(
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
                return playing_track_sleep(remaining_ms, config).max(teams_backoff_secs);
            }

            let remaining_ms = corrected_progress_ms.map(|c| track.duration_ms.saturating_sub(c));
            // Issue #165: live streams have no known remaining time → no
            // `expiryDateTime` on the wire (the status does not self-expire).
            let expiry_str = status_expiry_str(remaining_ms, config);

            match set_teams_status_message(
                &teams_tok.access_token,
                &final_status,
                expiry_str.as_deref(),
            ) {
                Ok(_) => {
                    *last_teams_update = Some(Instant::now());
                    *last_posted_status = Some(final_status.clone());
                    // Finding D1 (issue #684): the playing status is now on
                    // Teams — mirror it in the exit snapshot, which survives the
                    // loop's exit-tail clock reset (the D1 defect).
                    super::state::record_posted_status(Some(&final_status));
                    let _ = app.emit(
                        "presence-updated",
                        json!({
                            "status": final_status,
                            "timestamp": Utc::now().to_rfc3339()
                        }),
                    );
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
                Err(e) => {
                    // Issues #367/#428 (never-re-auth): a 401 here can mean
                    // the token expired mid-sequence (or clock skew) even
                    // though the pre-write expiry check passed. Mirror the
                    // Spotify reactive path at the top of `run`: one
                    // `refresh_teams_token` + CAS-commit + persist + single
                    // write retry before blaming the credential. Only a dead
                    // refresh token surfaces `teams-reconnect-required`.
                    let write_outcome = match e {
                        TeamsApiError::ExpiredToken(status) => {
                            log::info!("[POLLING] process_track: Teams status write hit ExpiredToken, attempting one refresh + retry");
                            let pre_refresh_access_token = teams_tok.access_token.clone();
                            match refresh_teams_token(&teams_tok) {
                                Ok(new_tokens) => {
                                    let committed = match cas_refresh_or_discard(
                                        "teams",
                                        &mut *state.tokens.teams_mut(),
                                        &pre_refresh_access_token,
                                        || Ok::<_, TeamsApiError>(new_tokens.clone()),
                                        |t| &t.access_token,
                                    ) {
                                        CasOutcome::Committed(_) => true,
                                        CasOutcome::Discarded { .. } => false,
                                        CasOutcome::RefreshFailed(_) => {
                                            unreachable!("inner refresh_fn is Ok-wrapping")
                                        }
                                    };
                                    if committed {
                                        // Issue #180: the write guard
                                        // reborrowed into the CAS call above
                                        // is dropped at the end of that
                                        // statement. Persist here — in a
                                        // later statement — so the read lock
                                        // inside persist_tokens (same RwLock)
                                        // cannot self-deadlock.
                                        if let Err(persist_err) =
                                            token_io::persist_tokens(state, app)
                                        {
                                            log::warn!(
                                                "[POLLING] process_track: failed to persist reactively refreshed teams tokens: {}",
                                                persist_err
                                            );
                                        }
                                        match set_teams_status_message(
                                            &new_tokens.access_token,
                                            &final_status,
                                            expiry_str.as_deref(),
                                        ) {
                                            Ok(()) => Ok(new_tokens),
                                            Err(retry_err) => {
                                                log::error!(
                                                    "[POLLING] process_track: Teams status retry after refresh also failed: {}",
                                                    retry_err
                                                );
                                                Err(retry_err)
                                            }
                                        }
                                    } else {
                                        // CAS lost (mirrors the Spotify 401
                                        // path): keep the original error for
                                        // classification below.
                                        Err(TeamsApiError::ExpiredToken(status))
                                    }
                                }
                                Err(refresh_err) => {
                                    log::error!(
                                        "[POLLING] process_track: Teams reactive refresh failed: {}",
                                        refresh_err
                                    );
                                    // Issue #295 policy: only a dead
                                    // credential clears the session; a
                                    // transient refresh failure keeps it and
                                    // flows into the transient branch below
                                    // with no reconnect event. Either way the
                                    // typed refresh error (not the stale
                                    // write error) is what gets classified.
                                    if teams_refresh_requires_reauth(&refresh_err) {
                                        log::warn!("[POLLING] process_track: Teams refresh token is dead, discarding tokens");
                                        *state.tokens.teams_mut() = None;
                                        // Issue #180: the write guard in the
                                        // clearing statement above dies at
                                        // the end of that statement. Persist
                                        // in a LATER statement, when the
                                        // guard is provably dropped.
                                        if let Err(persist_err) =
                                            token_io::persist_tokens(state, app)
                                        {
                                            log::warn!(
                                                "[POLLING] process_track: failed to persist cleared teams tokens: {}",
                                                persist_err
                                            );
                                        }
                                    } else {
                                        log::warn!("[POLLING] process_track: Teams reactive refresh failed (transient), keeping session");
                                    }
                                    Err(refresh_err)
                                }
                            }
                        }
                        other => Err(other),
                    };
                    match write_outcome {
                        Ok(refreshed) => {
                            *last_teams_update = Some(Instant::now());
                            *last_posted_status = Some(final_status.clone());
                            // Finding D1 (issue #684): see the main write arm.
                            super::state::record_posted_status(Some(&final_status));
                            let _ = app.emit(
                                "presence-updated",
                                json!({
                                    "status": final_status,
                                    "timestamp": Utc::now().to_rfc3339()
                                }),
                            );
                            // Adopt the fresh token so the availability
                            // re-arm below does not 401 on the stale one.
                            teams_tok = refreshed;
                        }
                        Err(e) => {
                            log::error!(
                                "[POLLING] process_track: Failed to set Teams status: {}",
                                e
                            );
                            // Issue #974: the Dashboard banner reads `user_message()`,
                            // never `Display`. `Display` carries the raw Graph body for
                            // 403/418 — useful in logs, useless to a user staring at a
                            // five-second banner.
                            emit_error(app, "teams", e.user_message(), ErrorSeverity::Error);
                            // Issue #154: a 429 extends the next poll to the
                            // server-directed delay.
                            teams_backoff_secs = teams_backoff_secs.max(rate_limit_sleep_secs(&e));
                            // Issue #153: classify typed TeamsApiError variants instead
                            // of string-sniffing the error body. Only a dead token
                            // (401 / invalid_grant) means re-auth; 403 is a
                            // permission/license problem re-auth cannot fix.
                            match e {
                                TeamsApiError::ExpiredToken(_) | TeamsApiError::InvalidGrant => {
                                    log::warn!("[POLLING] process_track: Teams auth failure detected, emitting teams-reconnect-required");
                                    let _ = app.emit("teams-reconnect-required", json!(null));
                                }
                                TeamsApiError::Forbidden(_, _) => {
                                    log::error!("[POLLING] process_track: Teams status update forbidden (permission/license) — re-auth cannot fix this; skipping teams-reconnect-required");
                                }
                                TeamsApiError::RateLimited(_)
                                | TeamsApiError::Transient(_)
                                | TeamsApiError::Other(_, _) => {
                                    log::warn!("[POLLING] process_track: Teams status update failed (transient), continuing");
                                }
                            }
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
            let placeholder_text = paused_status_placeholder(config);
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
                && gate_recheck_due(
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
                && !gate_recheck_due(
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
                            emit_presence_gated(app, reason, &availability, &activity);
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
                    let expiry_str = placeholder_expiry_str();
                    match clear_teams_status_message(
                        &teams_tok.access_token,
                        placeholder,
                        Some(&expiry_str),
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
                            super::state::record_posted_status(None);
                            // Finding D7 (issue #690): a PAUSE is not a stop.
                            // The dedicated event lets the Dashboard keep the
                            // track card and show the paused state;
                            // `presence-cleared` stays reserved for the genuine
                            // no-track path.
                            let _ =
                                app.emit("presence-paused", presence_paused_payload(placeholder));
                        }
                        Err(e) => {
                            log::error!(
                                "[POLLING] process_track: Failed to clear Teams status: {}",
                                e
                            );
                            // Issue #154: honor the server's Retry-After on a
                            // throttled clear.
                            teams_backoff_secs = teams_backoff_secs.max(rate_limit_sleep_secs(&e));
                        }
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
        teams_backoff_secs = teams_backoff_secs.max(sync_availability(
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
        playing_track_sleep(remaining_ms, config)
    } else {
        let sleep = pause_backoff(
            *consecutive_pauses,
            config_default_interval(config),
            config_pause_backoff_max(config),
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
    config: &Option<crate::config::AppConfig>,
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
        super::state::record_current_track_fingerprint(None);
        // Cleared in lockstep with `current_track` — the #343 rewrite path
        // reads this cache and must not resurrect a cleared track.
        *LAST_NOW_PLAYING.lock() = None;
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

    // P1 (issue #3.0-P1): availability sync — clear the Graph presence session
    // when nothing is playing (`clearPresence` 404 = session already gone =
    // success). Runs independently of `clear_on_pause`: that toggle governs the
    // placeholder status message only, while availability sync owns the
    // presence bubble. Finding #634: a rule's own presence session is cleared
    // here too — nothing is playing, so the rule's window/track no longer
    // applies and the user's real state must return. `armed_presence` is
    // cleared with it, so the next track re-arms instead of believing a session
    // is live.
    let mut teams_backoff_secs: u64 = 0;
    if availability_sync_enabled(config) {
        teams_backoff_secs = teams_backoff_secs.max(clear_presence_session(
            app,
            &teams_tok.access_token,
            "Availability cleared",
            armed_presence,
            last_availability_arm,
        ));
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
    // that instead of going silent.
    let no_track_rule = rule_gate(config, "", "");
    let placeholder = no_track_rule
        .replacement
        .clone()
        .unwrap_or_else(|| stopped_status_placeholder(config));
    let suppression_reason: Option<&str> = if no_track_rule.suppresses() {
        no_track_rule.reason
    } else {
        None
    };
    // Finding D4 (issue #687): the same class of defect as the paused clear.
    // Pre-fix the byte-identity check above ran BEFORE the suppression verdict,
    // and the suppressed branch recorded the placeholder as POSTED although
    // nothing was sent — so when the quiet window closed (or a match-all rule
    // stopped matching) the dedup skipped the clear and Teams kept showing the
    // stale playing status until the next track. Ask the verdict first, then
    // compare, and record a SUPPRESSION (not a post) when it blocks.
    let already_posted = last_posted_placeholder.as_deref() == Some(placeholder.as_str());
    let already_suppressed = suppressed_placeholder.as_deref() == Some(placeholder.as_str());
    match placeholder_write_decision(
        suppression_reason.is_some(),
        already_posted,
        already_suppressed,
    ) {
        PlaceholderWrite::Suppress { announce } => {
            log::info!(
                "[POLLING] handle_no_track: clear suppressed ({}), keeping Teams status untouched (retried once the decision changes)",
                suppression_reason.unwrap_or(GATE_REASON_QUIET_HOURS)
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
                emit_presence_gated(
                    app,
                    suppression_reason.unwrap_or(GATE_REASON_QUIET_HOURS),
                    "",
                    "",
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

    let expiry_str = placeholder_expiry_str();
    // Issue #455-residual: mirror the process_track ExpiredToken
    // refresh+single-retry (see the write path above) — a 401 here can mean
    // the token expired mid-sequence even though the pre-write expiry check
    // in `teams_token_for_write` passed. Only a dead refresh token surfaces
    // `teams-reconnect-required`.
    let clear_outcome: Result<(), TeamsApiError> = match clear_teams_status_message(
        &teams_tok.access_token,
        &placeholder,
        Some(&expiry_str),
    ) {
        Ok(_) => Ok(()),
        Err(TeamsApiError::ExpiredToken(status)) => {
            log::info!("[POLLING] handle_no_track: Teams clear hit ExpiredToken, attempting one refresh + retry");
            let pre_refresh_access_token = teams_tok.access_token.clone();
            match refresh_teams_token(&teams_tok) {
                Ok(new_tokens) => {
                    let committed = match cas_refresh_or_discard(
                        "teams",
                        &mut *state.tokens.teams_mut(),
                        &pre_refresh_access_token,
                        || Ok::<_, TeamsApiError>(new_tokens.clone()),
                        |t| &t.access_token,
                    ) {
                        CasOutcome::Committed(_) => true,
                        CasOutcome::Discarded { .. } => false,
                        CasOutcome::RefreshFailed(_) => {
                            unreachable!("inner refresh_fn is Ok-wrapping")
                        }
                    };
                    if committed {
                        // Issue #180: the write guard reborrowed into the
                        // CAS call above is dropped at the end of that
                        // statement. Persist here — in a later statement
                        // — so the read lock inside persist_tokens (same
                        // RwLock) cannot self-deadlock.
                        if let Err(persist_err) = token_io::persist_tokens(state, app) {
                            log::warn!(
                                    "[POLLING] handle_no_track: failed to persist reactively refreshed teams tokens: {}",
                                    persist_err
                                );
                        }
                        match clear_teams_status_message(
                            &new_tokens.access_token,
                            &placeholder,
                            Some(&expiry_str),
                        ) {
                            Ok(()) => Ok(()),
                            Err(retry_err) => {
                                log::error!(
                                        "[POLLING] handle_no_track: Teams clear retry after refresh also failed: {}",
                                        retry_err
                                    );
                                Err(retry_err)
                            }
                        }
                    } else {
                        // CAS lost (mirrors the process_track path): keep
                        // the original error for classification below.
                        Err(TeamsApiError::ExpiredToken(status))
                    }
                }
                Err(refresh_err) => {
                    log::error!(
                        "[POLLING] handle_no_track: Teams reactive refresh failed: {}",
                        refresh_err
                    );
                    // Issue #295 policy: only a dead credential clears the
                    // session; a transient refresh failure keeps it. Either
                    // way the typed refresh error (not the stale write
                    // error) is what gets classified.
                    if teams_refresh_requires_reauth(&refresh_err) {
                        log::warn!("[POLLING] handle_no_track: Teams refresh token is dead, discarding tokens");
                        *state.tokens.teams_mut() = None;
                        // Issue #180: the write guard in the clearing
                        // statement above dies at the end of that
                        // statement. Persist in a LATER statement, when
                        // the guard is provably dropped.
                        if let Err(persist_err) = token_io::persist_tokens(state, app) {
                            log::warn!(
                                    "[POLLING] handle_no_track: failed to persist cleared teams tokens: {}",
                                    persist_err
                                );
                        }
                    } else {
                        log::warn!("[POLLING] handle_no_track: Teams reactive refresh failed (transient), keeping session");
                    }
                    Err(refresh_err)
                }
            }
        }
        Err(other) => Err(other),
    };
    match clear_outcome {
        Ok(_) => {
            *last_posted_placeholder = Some(placeholder.to_string());
            // Issue #384: Teams now shows a placeholder, so the recorded
            // playing status is stale.
            *last_posted_status = None;
            // Finding D1 (issue #684): Teams now shows a placeholder, not a
            // playing status — mirror it in the exit snapshot.
            super::state::record_posted_status(None);
            // Findings D4/D11 follow-up: the clear was posted, so no write is
            // being suppressed any more — retire the finished track's gate key
            // (a stale one made `get_sync_status` answer `presence_gated = true`
            // forever after a gated track ended).
            *gated_track_key = no_track_gate_key(false).map(str::to_string);
            let _ = app.emit(
                "presence-cleared",
                json!({ "timestamp": Utc::now().to_rfc3339() }),
            );
            teams_backoff_secs
        }
        Err(e) => {
            log::error!(
                "[POLLING] handle_no_track: Failed to clear Teams status: {}",
                e
            );
            // Issue #154: honor the server's Retry-After on a throttled clear
            // (read before the classifier below moves `e`).
            let backoff = teams_backoff_secs.max(rate_limit_sleep_secs(&e));
            // Mirror the process_track classifier: only a dead token means
            // re-auth; 403 is a permission/license problem re-auth cannot fix.
            match e {
                TeamsApiError::ExpiredToken(_) | TeamsApiError::InvalidGrant => {
                    log::warn!("[POLLING] handle_no_track: Teams auth failure detected, emitting teams-reconnect-required");
                    let _ = app.emit("teams-reconnect-required", json!(null));
                }
                TeamsApiError::Forbidden(_, _) => {
                    log::error!("[POLLING] handle_no_track: Teams clear forbidden (permission/license) — re-auth cannot fix this; skipping teams-reconnect-required");
                }
                TeamsApiError::RateLimited(_)
                | TeamsApiError::Transient(_)
                | TeamsApiError::Other(_, _) => {
                    log::warn!(
                        "[POLLING] handle_no_track: Teams clear failed (transient), continuing"
                    );
                }
            }
            backoff
        }
    }
}

/// Finding #636 (issue #636): best-effort presence cleanup on shutdown.
///
/// Called from the `RunEvent::Exit` arm in `lib.rs`, AFTER
/// `updater_bg::install_pending_on_exit` — so a staged update is never delayed
/// by a Graph round-trip, and on Windows (where the installer exits the process
/// without returning) an update-driven quit never reaches this code at all.
///
/// Without it, a crash/force-quit/machine-sleep leaves the app's presence
/// session armed until its `expirationDuration` lapses, and a leftover
/// "🎵 …" status message keeps advertising music the app is no longer
/// tracking. Two bounded calls, each behind its own config flag:
///
/// * `availability_sync` + something actually armed → `clearPresence`;
/// * `clear_on_pause` + a playing status currently posted → the short-lived
///   "Paused" placeholder (it self-removes after 60 s).
///
/// Never blocks meaningfully: both calls run through a 3-second client
/// ([`crate::teams::EXIT_CLEANUP_TIMEOUT`]) and a failed first call skips the
/// second. Log-only; nothing here can fail the exit.
///
/// Finding D1 (issue #684): the decision is read from the process-wide
/// [`ExitSnapshot`] — NOT from the live write clocks. `polling_loop`'s exit
/// tail calls `reset_write_clocks()` on its way out (finding PollCore#4 / #572)
/// and `RunEvent::Exit` runs after it, so the clocks are already cold exactly
/// when a mid-song quit needs them: `clear_presence_on_exit` found two `None`s,
/// early-returned, and left the music status plus the armed `Available` session
/// live — the very outcome #636 exists to prevent. The snapshot is written by
/// every successful Teams write / presence arm and is deliberately not reset by
/// that exit tail (see [`super::state::ExitSnapshot`]).
pub(crate) fn clear_presence_on_exit(app: &AppHandle) {
    use tauri::Manager;

    let Some(state) = app.try_state::<Arc<AppState>>() else {
        return;
    };
    // Clone out of the read guard before any blocking call: the guard must not
    // be held across a Graph round-trip on the exit path.
    let config = state.config.get().clone();
    let snapshot = super::state::load_exit_snapshot();
    let plan = exit_cleanup_plan(
        &snapshot,
        availability_sync_enabled(&config),
        config
            .as_ref()
            .map(|c| c.teams.clear_on_pause)
            .unwrap_or(true),
    );
    // Nothing of ours is armed or posted: don't touch the user's Teams.
    if !plan.clear_presence && !plan.post_placeholder {
        return;
    }
    let Some(tokens) = state.tokens.teams().clone() else {
        return;
    };
    if is_teams_token_expired(&tokens) {
        log::info!(
            "[POLLING] clear_presence_on_exit: stored Teams token is expired, skipping presence cleanup"
        );
        return;
    }

    let mut cleared = true;
    if plan.clear_presence {
        if let Some((availability, activity, label)) = snapshot.armed_presence.as_ref() {
            log::info!(
                "[POLLING] clear_presence_on_exit: clearing the presence session this app armed as {} / {} ({})",
                availability,
                activity,
                label
            );
        }
        match clear_teams_presence_quick(&tokens.access_token) {
            Ok(_) => log::info!("[POLLING] clear_presence_on_exit: presence session cleared"),
            Err(e) => {
                cleared = false;
                log::warn!(
                    "[POLLING] clear_presence_on_exit: failed to clear presence session: {}",
                    e
                );
            }
        }
    }
    // Issue #866: the preferred-presence session is independent of the
    // ephemeral `setPresence` session above — `availability_sync` being off
    // does not mean the user did not opt into preferred presence, and
    // Graph accepts both. Clear unconditionally when present, using the
    // quick variant so the exit arm cannot hold the close open.
    if load_preferred_presence_session().is_some() {
        match clear_user_preferred_presence_quick(&tokens.access_token) {
            Ok(_) => log::info!("[POLLING] clear_presence_on_exit: preferred presence cleared"),
            Err(e) => log::warn!(
                "[POLLING] clear_presence_on_exit: failed to clear preferred presence: {}",
                e
            ),
        }
    }
    if cleared && plan.post_placeholder {
        // S4 (issue #672): the exit placeholder is the SAME text the paused
        // clear posts, so a configured `teams.paused_status_format` is not
        // replaced by the default on the way out.
        let placeholder = paused_status_placeholder(&config);
        match clear_teams_status_message_quick(
            &tokens.access_token,
            &placeholder,
            Some(&placeholder_expiry_str()),
        ) {
            Ok(_) => log::info!(
                "[POLLING] clear_presence_on_exit: paused placeholder posted; available status replaced"
            ),
            Err(e) => log::warn!(
                "[POLLING] clear_presence_on_exit: failed to post the paused placeholder: {}",
                e
            ),
        }
    }
    if cleared {
        // Nothing of ours is left on Teams, so a repeated `RunEvent::Exit` (or
        // a later session that has not written yet) must not repeat this.
        super::state::reset_exit_snapshot();
    }
}

/// Finding D1 (issue #684): what the exit cleanup should attempt, derived from
/// the [`super::state::ExitSnapshot`] and the two config flags. Pure so the
/// "the reset clocks must not cancel the cleanup" guarantee is unit-testable —
/// the pre-fix code decided this inline from `clocks.last_availability_arm` /
/// `clocks.last_posted_status`, which the loop's exit tail has already emptied.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ExitCleanupPlan {
    pub(crate) clear_presence: bool,
    pub(crate) post_placeholder: bool,
}

fn exit_cleanup_plan(
    snapshot: &super::state::ExitSnapshot,
    availability_sync: bool,
    clear_on_pause: bool,
) -> ExitCleanupPlan {
    ExitCleanupPlan {
        clear_presence: availability_sync && snapshot.armed_presence.is_some(),
        // Review round 2 (item 7): never replace a Teams status the USER owns on
        // the way out — the poller recorded whether one is in force (see
        // `manual_status_blocks_write`). Clearing our OWN armed availability
        // session stays correct either way.
        post_placeholder: clear_on_pause
            && snapshot.last_posted_status.is_some()
            && !snapshot.manual_status_blocks,
    }
}

fn config_default_interval(config: &Option<crate::config::AppConfig>) -> u64 {
    config
        .as_ref()
        .map(|c| c.polling.default_interval_seconds)
        .unwrap_or(30)
}

fn config_minimum_interval(config: &Option<crate::config::AppConfig>) -> u64 {
    config
        .as_ref()
        .map(|c| c.polling.minimum_interval_seconds)
        .unwrap_or(10)
}

/// `polling.pause_backoff_max_seconds` (issue #538), defaulted to the
/// documented 300 s so an untouched config keeps 4.5 behaviour.
fn config_pause_backoff_max(config: &Option<crate::config::AppConfig>) -> u64 {
    config
        .as_ref()
        .map(|c| c.polling.pause_backoff_max_seconds)
        .unwrap_or(300)
}

fn config_maximum_interval(config: &Option<crate::config::AppConfig>) -> u64 {
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
fn spotify_backoff_base(err: &SpotifyApiError) -> u64 {
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
fn spotify_backoff_secs(err: &SpotifyApiError) -> u64 {
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
fn spotify_backoff_secs_retry_after(retry_after: Option<u64>) -> u64 {
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
fn extract_retry_after(msg: &str) -> Option<Option<u64>> {
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
fn rate_limit_sleep_secs(err: &TeamsApiError) -> u64 {
    match err {
        TeamsApiError::RateLimited(Some(secs)) => *secs,
        TeamsApiError::RateLimited(None) => with_jitter(RATE_LIMIT_BACKOFF_SECONDS),
        _ => 0,
    }
}

/// Format a UTC instant as Graph's offset-less `dateTime` with 6 fraction
/// digits (≤ the documented 7). `to_rfc3339()` would embed `+00:00` and up
/// to 9 fraction digits, contradicting the dateTimeTimeZone schema (issue
/// #156).
fn format_expiry(expiry: chrono::DateTime<Utc>) -> String {
    expiry.format("%Y-%m-%dT%H:%M:%S%.6f").to_string()
}

/// Expiry for the short-lived "clear" placeholders (issue #155): now + 60s,
/// so the placeholder self-removes ~1 min after the last successful post even
/// if the app quits.
fn placeholder_expiry_str() -> String {
    let expiry = Utc::now() + chrono::Duration::seconds(60);
    format_expiry(expiry)
}

/// Expiry for a playing-track status message: now + remaining + buffer when
/// the position is known; `None` (no `expiryDateTime` on the wire) for
/// live/unknown-position streams (issue #165).
fn status_expiry_str(
    remaining_ms: Option<u64>,
    config: &Option<crate::config::AppConfig>,
) -> Option<String> {
    remaining_ms.map(|remaining| {
        let buffer_ms = config
            .as_ref()
            .map(|c| c.polling.expiry_buffer_seconds)
            .unwrap_or(10)
            * 1000;
        let expiry =
            Utc::now() + chrono::Duration::milliseconds(remaining as i64 + buffer_ms as i64);
        format_expiry(expiry)
    })
}

/// Sleep decision for a playing track. Known position → sleep until ~5s
/// before the track ends (clamped to the config bounds); unknown position
/// (live stream, issue #165) → the default interval, not a duration-derived
/// one.
fn playing_track_sleep(
    remaining_ms: Option<u64>,
    config: &Option<crate::config::AppConfig>,
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
fn clamp_poll_interval(secs: u64, config: &Option<crate::config::AppConfig>) -> u64 {
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
fn pause_backoff(consecutive_pauses: u8, default_secs: u64, ceiling_secs: u64) -> u64 {
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

fn with_jitter(base_secs: u64) -> u64 {
    let mut rng = rand::rng();
    let jitter_range = base_secs as f64 * 0.2;
    let jitter = rng.random_range(-jitter_range..=jitter_range);
    (base_secs as f64 + jitter).max(1.0) as u64
}

/// Finding PollCore#3 (issue #571): additive-only jitter, `base + 0..=20%`.
/// Used wherever the base is a server directive (`Retry-After`) that must
/// never be undershot.
fn with_upward_jitter(base_secs: u64) -> u64 {
    let mut rng = rand::rng();
    let jitter = rng.random_range(0.0..=(base_secs as f64 * 0.2));
    (base_secs as f64 + jitter) as u64
}

/// Finding PollCore#0 (issue #568): capped exponential backoff for repeated
/// network failures — `min(300, with_jitter(30) * 2^(n-1))` for `n`
/// consecutive failures — so an offline machine slows to the 5-minute ceiling
/// without ever stopping the poller. The doubling exponent is clamped so a
/// saturated counter cannot overflow the shift.
fn network_failure_backoff(count: u8) -> u64 {
    let doublings = count.saturating_sub(1).min(4) as u32;
    with_jitter(NETWORK_BACKOFF_BASE_SECONDS)
        .saturating_mul(1u64 << doublings)
        .min(NETWORK_BACKOFF_CAP_SECONDS)
}

#[cfg(test)]
mod tests {
    use super::*;

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
            config_pause_backoff_max(&Some(crate::config::AppConfig::default())),
            300
        );
        let mut raised = crate::config::AppConfig::default();
        raised.polling.pause_backoff_max_seconds = 900;
        assert_eq!(config_pause_backoff_max(&Some(raised)), 900);
    }

    /// Regression guard for issue #72 drift point #3.
    #[test]
    fn test_cas_discard_block_is_single_source_of_truth() {
        let source = include_str!("poll_once.rs");
        // Scan only production code (above the test module) so the test's
        // own string literals don't inflate the count.
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("poll_once.rs has no #[cfg(test)] mod tests block");
        let discard_count = prod_source
            .matches("state changed during refresh, discarding result")
            .count();
        // Finding PollCore#8 (issue #574): pin the EXACT counts. The old
        // lower-bound assertions (`>= 1`, `>= 3`) passed even when a call site
        // — or the helper itself — was deleted, so the #72 anti-drift
        // guarantee this test exists to provide was vacuous. Update these
        // numbers deliberately whenever a call site is added.
        assert_eq!(
            discard_count, 1,
            "exactly one CAS-discard log line (inside the helper) is expected in \
             production; found {}. A second one means a call site re-implemented \
             the discard dance instead of routing through the helper.",
            discard_count
        );
        let helper_def = prod_source.matches("fn cas_refresh_or_discard").count();
        assert_eq!(helper_def, 1, "helper defined {} times", helper_def);
        let helper_call_count = prod_source.matches("cas_refresh_or_discard(").count();
        // 5 calls: Spotify proactive, Spotify 401-retry, Teams proactive,
        // Teams write-retry (issues #367/#428) and the no-track clear retry
        // (issue #455-residual).
        // The "fn cas_refresh_or_discard(" definition is NOT counted here
        // because the call-shape substring includes the open-paren.
        assert_eq!(
            helper_call_count, 5,
            "cas_refresh_or_discard called {} times in production; expected 5 \
             (Spotify proactive + 401-retry + Teams proactive + Teams write-retry \
             + no-track clear retry)",
            helper_call_count
        );
    }

    /// Issue #180 regression test: refresh-success + persist on the same lock.
    ///
    /// Pre-fix, `cas_refresh_or_discard` persisted the refreshed tokens from
    /// inside the helper while the caller's write guard (a reborrow of
    /// `state.tokens.X_mut()`) was still alive for the whole call statement.
    /// `token_io::persist_tokens` then re-locked the SAME parking_lot RwLock
    /// for reading — write→read on the same lock from the same thread parks
    /// forever (parking_lot has no same-thread reentrancy detection), so
    /// every successful refresh deadlocked the polling thread.
    ///
    /// The fix persists only at the call sites, in a statement AFTER the CAS
    /// call returns, when the write guard is provably dropped. This test runs
    /// the exact production call shape (write guard reborrowed into the CAS
    /// helper) plus the persist step (re-locking the same RwLock for reading,
    /// which is the lock acquisition `token_io::persist_tokens` performs) in
    /// a spawned thread, and asserts completion via `recv_timeout`. The
    /// deadlock would hang CI, so the 10s timeout makes a regression fail
    /// fast instead of hanging the suite.
    #[test]
    fn test_refresh_success_persist_does_not_self_deadlock() {
        use std::thread;
        use std::time::Duration;

        let state = Arc::new(AppState::new());
        {
            let mut guard = state.tokens.spotify_mut();
            *guard = Some(crate::spotify::SpotifyTokens {
                access_token: "pre-refresh-access-token".to_string(),
                refresh_token: "refresh-token".to_string(),
                expires_at: Utc::now() + chrono::Duration::hours(1),
            });
        }

        let (tx, rx) = mpsc::channel();
        let state2 = state.clone();
        let handle = thread::spawn(move || {
            let pre_refresh_access_token = "pre-refresh-access-token".to_string();
            let new_tokens = crate::spotify::SpotifyTokens {
                access_token: "post-refresh-access-token".to_string(),
                refresh_token: "refresh-token".to_string(),
                expires_at: Utc::now() + chrono::Duration::hours(2),
            };
            // Exact production call shape (Spotify proactive refresh): the
            // write guard is a temporary reborrowed into the CAS helper; it
            // stays alive until the end of this statement.
            let outcome = cas_refresh_or_discard(
                "spotify",
                &mut *state2.tokens.spotify_mut(),
                &pre_refresh_access_token,
                || Ok::<_, SpotifyApiError>(new_tokens.clone()),
                |t| &t.access_token,
            );
            let committed = matches!(outcome, CasOutcome::Committed(_));
            if committed {
                // Persist step: re-lock the SAME RwLock for reading, exactly
                // as token_io::persist_tokens does on a successful refresh.
                // If the write guard above were still alive, this parks
                // forever (issue #180).
                let _persisted = state2.tokens.spotify();
            }
            let _ = tx.send(committed);
        });

        let committed = rx.recv_timeout(Duration::from_secs(10)).expect(
            "refresh-success + persist self-deadlocked: the write guard was still \
                 held when the same RwLock was re-locked for reading (issue #180)",
        );
        // The worker only returns after the persist step re-locked the same
        // RwLock successfully; joining surfaces any thread panic as a test
        // failure instead of a silently detached thread.
        handle.join().expect("persist worker thread panicked");
        assert!(committed, "CAS should commit the refreshed tokens");

        let stored = state.tokens.spotify();
        assert_eq!(
            stored.as_ref().map(|t| t.access_token.as_str()),
            Some("post-refresh-access-token"),
            "the refreshed tokens must be stored in AppState"
        );
    }

    /// Issue #180 regression guard: the CAS helper must never persist tokens
    /// itself. Pre-fix it called `token_io::persist_tokens` while the
    /// caller's write guard was still alive (write→read on the same
    /// parking_lot RwLock from the same thread parks forever), so every
    /// successful refresh self-deadlocked. The fix persists only at the
    /// call sites, in a statement AFTER the CAS call returns. If a future
    /// contributor moves a persist call back inside the helper body, the
    /// deadlock returns and this guard fails.
    #[test]
    fn test_cas_helper_body_has_no_persist_and_call_sites_persist() {
        let source = include_str!("poll_once.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("poll_once.rs has no #[cfg(test)] mod tests block");

        // Isolate the helper body by brace counting from its opening `{`
        // (house style — never boundary anchors, which drift). The `{}`
        // format placeholders inside string literals are balanced, so they
        // do not perturb the count.
        let after_sig = prod_source
            .split("fn cas_refresh_or_discard<T, E, F, G>(")
            .nth(1)
            .expect("cas_refresh_or_discard definition not found");
        let open = after_sig
            .find('{')
            .expect("cas_refresh_or_discard has no opening brace");
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
        let body = &after_sig[..end.expect("cas_refresh_or_discard body never closed")];
        assert!(
            !body.contains("persist_tokens("),
            "cas_refresh_or_discard must not persist tokens inside its body (issue #180: \
             the caller's write guard is alive for the whole call, so persist_tokens' \
             read lock on the same RwLock self-deadlocks). Body:\n{}",
            body
        );
        // All persistence must happen at the call sites, after the CAS call
        // returns (guard provably dropped): the three invalid_grant/dead-token
        // clear paths (Spotify proactive, Spotify 401-retry, Teams) plus the
        // four refresh-success call sites (Spotify proactive, Spotify
        // 401-retry, Teams proactive, Teams write-retry for issues #367/#428).
        // The Teams write-retry contributes two sites (refresh-success persist
        // + dead-credential clear persist), and the handle_no_track clear
        // retry (issue #455-residual) contributes two more, so the total is
        // ten.
        let persist_count = prod_source.matches("token_io::persist_tokens(").count();
        assert!(
            persist_count >= 10,
            "expected at least 10 persist_tokens call sites in production (3 provider \
             clear paths + 4 refresh-success call sites + 1 reactive dead-credential \
             clear + 2 no-track reactive clear-retry); found {}. If a call-site \
             persist is removed, refreshed/cleared tokens stop being flushed to disk; \
             if one is added inside cas_refresh_or_discard, the #180 self-deadlock \
             returns.",
            persist_count
        );
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
    #[test]
    fn test_no_track_paths_share_record_helper() {
        let source = include_str!("poll_once.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("poll_once.rs has no #[cfg(test)] mod tests block");
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
    #[test]
    fn test_error_event_emitted_in_exactly_one_place_per_failed_poll() {
        let source = include_str!("poll_once.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("poll_once.rs has no #[cfg(test)] mod tests block");
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

    /// Regression guard: the unified API call site must be invoked
    /// from exactly one place — `SpotifySource::poll` — and nowhere
    /// else (no second spot added by a future contributor). We grep for
    /// the *bound name* of the call site, not the bare
    /// `get_currently_playing(` substring (which would also match the
    /// fn definition site).
    ///
    /// Issue #862: the call moved out of `poll_once.rs` into
    /// `sources/spotify.rs` so the trait surface could own the
    /// conditional-GET round-trip and the ETag cache. The 401-retry
    /// path used to be a second `get_currently_playing(&retry_token,…)`
    /// call in `poll_once.rs`; the trait refactor moved the refresh
    /// into the poll loop (it refreshes the token and re-calls
    /// `playback_source.poll()`, which is the same call site). So the
    /// drift that motivated this guard now reduces to "exactly one
    /// `get_currently_playing` call site in `sources/spotify.rs`".
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
    fn test_start_polling_does_not_claim_is_syncing() {
        let source = include_str!("state.rs");
        let after_sig = source
            .split("pub fn start_polling(")
            .nth(1)
            .expect("state.rs has no `pub fn start_polling(`");
        let open = after_sig
            .find('{')
            .expect("start_polling has no opening brace");
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
        let body = &after_sig[..end.expect("start_polling body never closed")];
        assert!(
            !body.contains(".compare_exchange("),
            "polling::start_polling must not CAS is_syncing. See issue #60."
        );
    }

    /// Regression guard for issue #79/#117: poll_once.rs must NOT emit raw
    /// "error" events directly.
    #[test]
    fn test_no_raw_error_emit_in_poll_once() {
        let source = include_str!("poll_once.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("poll_once.rs has no #[cfg(test)] mod tests block");
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

    /// Issue #159: a Spotify 429 backoff honors the server's Retry-After,
    /// floored at the error retry interval so a tiny value can't busy-loop.
    #[test]
    fn test_spotify_backoff_base_honors_retry_after_floored() {
        use crate::spotify::SpotifyApiError;
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

    /// Issue #165: known position → an expiry exists; live stream (None) →
    /// no expiry so no `expiryDateTime` goes on the wire.
    ///
    /// Issue #264: the VALUE must be `now + remaining + buffer` — asserting
    /// the offset-less shape alone passes if the arithmetic sign flips or
    /// the buffer is dropped. The buffer default is read from the config
    /// type rather than hardcoded so a default change cannot silently
    /// invalidate the expectation.
    #[test]
    fn test_status_expiry_known_and_unknown_position() {
        let config = Some(crate::config::AppConfig::default());
        let buffer_secs = config
            .as_ref()
            .expect("config is Some")
            .polling
            .expiry_buffer_seconds;
        let remaining_ms = 120_000u64;

        let before = chrono::Utc::now();
        let s = status_expiry_str(Some(remaining_ms), &config)
            .expect("known position must yield an expiry");
        assert!(
            !s.contains('+') && !s.contains('Z'),
            "offset leaked into status expiry: {}",
            s
        );

        // The wire shape is offset-less; re-attach UTC to parse it back.
        let parsed = chrono::DateTime::parse_from_rfc3339(&format!("{}+00:00", s))
            .expect("status expiry must round-trip as RFC3339 once UTC is re-attached")
            .with_timezone(&Utc);
        let delta_secs = (parsed - before).num_seconds();
        let expected = (remaining_ms / 1000) as i64 + buffer_secs as i64;
        assert!(
            (delta_secs - expected).abs() <= 2,
            "status expiry must be now + remaining + buffer = {}s; got {}s (delta {}s). \
             A flipped `+ buffer_ms` or a zeroed default buffer lands here. See issue #264.",
            expected,
            delta_secs,
            delta_secs - expected
        );

        assert_eq!(
            status_expiry_str(None, &config),
            None,
            "live streams must not get an expiryDateTime"
        );
    }

    /// Issue #165: sleep falls back to the default interval for live streams
    /// instead of a duration-derived value; known positions sleep until ~5s
    /// before track end, clamped to the config bounds.
    #[test]
    fn test_playing_track_sleep_known_position_and_live_stream() {
        let config = Some(crate::config::AppConfig::default());
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

    // Issue #3.0-P1: the availability re-arm must happen at most every 4
    // minutes — Available sessions FADE after 5 min regardless of
    // `expirationDuration`, so the cadence must be strictly inside that
    // window (240s < 300s).
    #[test]
    fn test_should_rearm_availability_cadence() {
        let now = Instant::now();
        // Never armed → arm immediately.
        assert!(should_rearm_availability(None, now));
        // Armed 1 second ago → don't re-arm.
        let recent = now - std::time::Duration::from_secs(1);
        assert!(!should_rearm_availability(Some(recent), now));
        // Just under the cadence → don't re-arm.
        let under = now - std::time::Duration::from_secs(AVAILABILITY_REARM_SECONDS - 1);
        assert!(!should_rearm_availability(Some(under), now));
        // At/over the cadence → re-arm (strictly < 5 min fade window).
        let at = now - std::time::Duration::from_secs(AVAILABILITY_REARM_SECONDS);
        assert!(should_rearm_availability(Some(at), now));
        let over = now - std::time::Duration::from_secs(AVAILABILITY_REARM_SECONDS + 60);
        assert!(should_rearm_availability(Some(over), now));
        const { assert!(AVAILABILITY_REARM_SECONDS < 300) };
        // Guard above must hold: re-arm cadence strictly inside the
        // 5-minute Available fade window (issue #3.0-P1).
    }

    /// Issue #3.0-P1/P2 regression guard: inside `process_track`, the
    /// presence-gate read (`get_teams_presence`) must precede the status
    /// write (`set_teams_status_message`) so a busy/meeting presence can
    /// suppress it, and the availability call sites (set_teams_presence
    /// re-arm + clear_teams_presence on pause) must exist.
    #[test]
    fn test_presence_gate_precedes_status_write_and_availability_call_sites_exist() {
        let source = include_str!("poll_once.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("poll_once.rs has no #[cfg(test)] mod tests block");

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
        let tail = prod_fn_body(prod_source, "fn sync_availability(");
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
        let helper = prod_fn_body(prod_source, "fn arm_presence_session(");
        assert!(
            helper.contains("set_teams_presence("),
            "the arm helper must be the setPresence call site (issue #3.0-P1)"
        );
        let clear_helper = prod_fn_body(prod_source, "fn clear_presence_session(");
        assert!(
            clear_helper.contains("clear_teams_presence("),
            "the clear helper must be the clearPresence call site (issue #3.0-P1)"
        );
    }

    /// Issue #790: a 304 with a tracked track owes the availability re-arm (it
    /// is the steady state of a long episode/DJ set, which `process_track`
    /// never sees); a 304 with nothing tracked is "still nothing playing"
    /// (issue #242) and has no session of ours to keep alive; and a standing
    /// gate verdict for the track on screen suppresses the arm exactly as the
    /// shared tail does (issue #3.0-P1).
    #[test]
    fn test_rearm_after_304_contract() {
        assert!(
            rearm_after_304(true, true, false),
            "a 304 with a tracked track and availability_sync on must re-arm \
             (issue #790)"
        );
        assert!(
            !rearm_after_304(false, true, false),
            "a 304 with no tracked track has no session of ours to keep alive"
        );
        assert!(
            !rearm_after_304(true, false, false),
            "availability_sync off leaves nothing to arm"
        );
        assert!(
            !rearm_after_304(true, true, true),
            "a gated iteration is never answered with a setPresence of ours \
             (issue #3.0-P1)"
        );
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

    /// Issue #790 regression guard. Teams' `Available` fade is a 5-minute
    /// clock that does not care whether this app POSTed, so the re-arm must
    /// ride the poll tail: the identical-write skip has to reach the shared
    /// availability tail BEFORE it returns, and the 304 path has to surface
    /// the cached track so `process_track` can re-arm it.
    ///
    /// Issue #862: the 304 match arm moved out of `poll_once.rs` into
    /// `sources/spotify.rs::SpotifySource::poll` (the trait surface owns
    /// the conditional-GET cache). The guard now asserts two things in
    /// their respective homes — the `process_track` structural guard still
    /// lives in `poll_once.rs`, and the 304 → cached-track arm lives in
    /// `sources/spotify.rs`. The pre-fix drift ("arms ≥ 2 in
    /// `poll_once.rs`") no longer applies because there is exactly one
    /// 304 arm in the source.
    #[test]
    fn test_identical_write_skip_and_304_arms_rearm_availability() {
        let source = include_str!("poll_once.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("poll_once.rs has no #[cfg(test)] mod tests block");
        let body = prod_fn_body(prod_source, "pub(crate) fn process_track(");

        let skip_pos = body
            .find("if should_skip_identical_write(")
            .expect("process_track must keep the #384 identical-write guard");
        let after_guard = &body[skip_pos..];
        let skip_return = after_guard
            .find("return playing_track_sleep(")
            .expect("the identical-write guard must still return early");
        let sync_pos = after_guard.find("sync_availability(").expect(
            "the identical-write skip must reach the shared availability tail \
             before returning: an Available session fades after 5 minutes \
             whatever this app POSTs (issue #790)",
        );
        assert!(
            sync_pos < skip_return,
            "the availability re-arm must land BEFORE the identical-write early \
             return, or the unchanged-status steady state never re-arms (issue \
             #790)"
        );

        // Issue #862: the 304 match arm now lives in
        // `sources/spotify.rs::SpotifySource::poll`. The contract is the
        // same — a 304 surfaces the cached `NowPlaying` so the poll
        // loop's `last_track_key` keeps recognising the same track, AND
        // sets `last_was_not_modified` so the loop takes the unchanged-
        // track fast path (which runs through `sync_availability`). We
        // grep the source file for the arm instead of `poll_once.rs`.
        let spotify_source = include_str!("../sources/spotify.rs");
        let mut rest = spotify_source;
        let mut arms = 0usize;
        while let Some(pos) = rest.find("Ok(crate::spotify::CurrentlyPlaying::NotModified) =>") {
            let arm = &rest[pos..];
            let open = arm.find('{').expect("a 304 match arm must open a block");
            let mut depth = 0usize;
            let mut end = None;
            for (i, ch) in arm[open..].char_indices() {
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
            let end = end.expect("a 304 match arm must close");
            let arm_body = &arm[..end];
            // The arm must (a) return the cached `NowPlaying` so
            // `last_track_key` keeps recognising the same track, and
            // (b) flag the iteration so the poll loop's unchanged-track
            // fast path reaches `sync_availability` (issue #790).
            assert!(
                arm_body.contains("last_now_playing.clone()"),
                "the 304 arm must surface the cached NowPlaying; arm body was: {}",
                arm_body
            );
            assert!(
                arm_body.contains("last_was_not_modified = true"),
                "the 304 arm must flag the iteration as not-modified so the poll \
                 loop takes the unchanged-track fast path (issue #790); arm body \
                 was: {}",
                arm_body
            );
            arms += 1;
            rest = &arm[end..];
        }
        assert!(
            arms >= 1,
            "SpotifySource::poll must own the 304 → cached-track arm (issue #862); \
             found {}",
            arms
        );
    }

    /// Candidate C11 (docs/scope-3.3.md §C11): a 304 Not Modified with a
    /// tracked track is a pure no-op iteration — default-interval sleep,
    /// pause/transient counters reset exactly like the unchanged-track path.
    /// The stored ETag survives structurally: the 304 path never touches it,
    /// so the next poll stays conditional.
    #[test]
    fn test_not_modified_keeps_state_and_sleeps_default_interval() {
        let config = Some(crate::config::AppConfig::default());
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
        let config = Some(crate::config::AppConfig::default());
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

    /// Candidate C11 regression guard (issue #862): the conditional GET
    /// `If-None-Match` round-trip now lives inside `SpotifySource`
    /// rather than `poll_once`. Verify the `last_etag.as_deref()` pattern
    /// is passed into `get_currently_playing` so the conditional GET
    /// cannot silently degrade to unconditional-only.
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

        let cfg = Some(crate::config::AppConfig::default());

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
    /// Issue #343: the change-key fingerprint must move with each of the
    /// status-shaping config values (filter flag, placeholder, format) —
    /// otherwise a mid-track flip reads as "unchanged" and the stale
    /// status stays posted.
    #[test]
    fn test_status_config_fingerprint_tracks_filter_placeholder_format() {
        let base = Some(crate::config::AppConfig::default());
        let fp = status_config_fingerprint(&base);

        let mut off = crate::config::AppConfig::default();
        off.teams.profanity_filter = false;
        assert_ne!(
            fp,
            status_config_fingerprint(&Some(off)),
            "toggling the filter must change the fingerprint"
        );

        let mut ph = crate::config::AppConfig::default();
        ph.teams.profanity_placeholder = "something else".to_string();
        assert_ne!(
            fp,
            status_config_fingerprint(&Some(ph)),
            "editing the placeholder must change the fingerprint"
        );

        let mut fmt = crate::config::AppConfig::default();
        fmt.teams.status_format = "{track}".to_string();
        assert_ne!(
            fp,
            status_config_fingerprint(&Some(fmt)),
            "editing the format must change the fingerprint"
        );

        assert_eq!(
            fp,
            status_config_fingerprint(&base),
            "identical config must fingerprint identically"
        );

        // Issue #432: enabling a rule or quiet-hours entry must flip the
        // fingerprint so the change takes effect mid-track.
        let mut ruled = crate::config::AppConfig::default();
        ruled
            .status_rules
            .quiet_hours
            .push(crate::config::QuietHoursEntry {
                replacement_status: String::new(),
                enabled: true,
                start_minutes: 0,
                end_minutes: 1439,
                days: Vec::new(),
                ..Default::default()
            });
        assert_ne!(
            fp,
            status_config_fingerprint(&Some(ruled)),
            "adding a quiet-hours entry must change the fingerprint"
        );

        // S4 (issue #672): the rule schedule, `pause_polling` and the two
        // manual-status texts are part of the decision, so editing any of them
        // mid-track must flip the key (and force one rewrite).
        let scheduled = crate::config::AppConfig {
            status_rules: crate::config::StatusRulesConfig {
                quiet_hours: Vec::new(),
                track_rules: vec![crate::config::TrackRuleEntry {
                    enabled: true,
                    days: vec![1],
                    start_minutes: 480,
                    end_minutes: 1020,
                    ..Default::default()
                }],
                ..crate::config::StatusRulesConfig::default()
            },
            ..Default::default()
        };
        let scheduled_fp = status_config_fingerprint(&Some(scheduled.clone()));
        assert_ne!(
            fp, scheduled_fp,
            "adding a scheduled rule must change the fingerprint"
        );

        // The SAME rule with a different window is a different fingerprint: that
        // is what re-evaluates the rule mid-track.
        let mut moved = scheduled.clone();
        moved.status_rules.track_rules[0].end_minutes = 1021;
        assert_ne!(
            scheduled_fp,
            status_config_fingerprint(&Some(moved)),
            "editing a rule's window must change the fingerprint"
        );
        let mut other_days = scheduled.clone();
        other_days.status_rules.track_rules[0].days = vec![2];
        assert_ne!(
            scheduled_fp,
            status_config_fingerprint(&Some(other_days)),
            "editing a rule's weekday set must change the fingerprint"
        );

        let mut pauses = crate::config::AppConfig::default();
        pauses
            .status_rules
            .quiet_hours
            .push(crate::config::QuietHoursEntry {
                enabled: true,
                start_minutes: 0,
                end_minutes: 1439,
                pause_polling: true,
                ..Default::default()
            });
        let pauses_fp = status_config_fingerprint(&Some(pauses));
        assert_ne!(fp, pauses_fp, "`pause_polling` must change the fingerprint");

        let mut paused_text = crate::config::AppConfig::default();
        paused_text.teams.paused_status_format = "BRB".to_string();
        assert_ne!(
            fp,
            status_config_fingerprint(&Some(paused_text)),
            "editing the paused status text must change the fingerprint"
        );

        let mut stopped_text = crate::config::AppConfig::default();
        stopped_text.teams.stopped_status_format = "Idle".to_string();
        assert_ne!(
            fp,
            status_config_fingerprint(&Some(stopped_text)),
            "editing the stopped status text must change the fingerprint"
        );
        // An EMPTY text renders the default, so it must fingerprint like the
        // default — otherwise clearing a field would force a rewrite that
        // changes nothing on Teams.
        let mut cleared_texts = crate::config::AppConfig::default();
        cleared_texts.teams.paused_status_format = String::new();
        cleared_texts.teams.stopped_status_format = String::new();
        assert_eq!(
            fp,
            status_config_fingerprint(&Some(cleared_texts)),
            "an empty status text must fingerprint like the default it renders"
        );
    }

    /// Issue #343: the 304 force-rewrite fires exactly when the stored key
    /// no longer matches the current item + config — nothing tracked, no
    /// rewrite; matching key, no rewrite; flipped config, one rewrite
    /// carrying the last observed item.
    ///
    /// Issue #581: the rewrite carries the WHOLE item, so an episode is
    /// re-rendered through the episode template with its context tokens
    /// instead of being flattened to its media.
    #[test]
    fn test_config_flip_rewrite_track_fires_only_on_mismatch() {
        let config = Some(crate::config::AppConfig::default());
        let now = crate::spotify::NowPlaying {
            media: crate::spotify::TrackInfo {
                title: "T".to_string(),
                artist: "A".to_string(),
                album: String::new(),
                album_art_url: String::new(),
                is_playing: true,
                progress_ms: Some(0),
                duration_ms: 0,
                volume_percent: None,
                supports_volume: None,
                actions: None,
            },
            episode: None,
            context: crate::spotify::PlaybackContext {
                device: "Kitchen speaker".to_string(),
                playlist: "Workout Mix".to_string(),
                shuffle: true,
                repeat: crate::spotify::RepeatState::Context,
            },
        };

        // Nothing tracked → no rewrite. The cache is process-wide, so the
        // test owns both its write and its teardown.
        *LAST_NOW_PLAYING.lock() = None;
        assert!(
            config_flip_rewrite_track(&None, &config).is_none(),
            "nothing tracked means nothing to rewrite"
        );

        *LAST_NOW_PLAYING.lock() = Some(now.clone());
        let key = status_track_key(&now, &config);

        // Matching key → steady-state 304 stays a no-op.
        assert!(
            config_flip_rewrite_track(&Some(key.clone()), &config).is_none(),
            "a matching key must not force a rewrite"
        );

        // Same item, flipped filter → one rewrite carrying the full item.
        let mut flipped = crate::config::AppConfig::default();
        flipped.teams.profanity_filter = false;
        let rewrite = config_flip_rewrite_track(&Some(key), &Some(flipped));
        let rewrite = rewrite.expect("a config flip must force one rewrite");
        assert_eq!(rewrite.media.title, "T");
        assert_eq!(rewrite.media.artist, "A");
        assert_eq!(
            rewrite.context.playlist, "Workout Mix",
            "the rewrite must keep the playback context the live path would render"
        );
        assert_eq!(rewrite.context.repeat, crate::spotify::RepeatState::Context);

        *LAST_NOW_PLAYING.lock() = None;
    }

    /// Issue #581: an episode and a track that happen to share the same
    /// displayed title/show still re-key, so switching track → episode →
    /// track writes a status at each step instead of deduping them into one.
    #[test]
    fn status_track_key_separates_episodes_from_tracks() {
        let config = Some(crate::config::AppConfig::default());
        let media = crate::spotify::TrackInfo {
            title: "Episode 12".to_string(),
            artist: "The Deep Work Show".to_string(),
            album: String::new(),
            album_art_url: String::new(),
            is_playing: true,
            progress_ms: Some(0),
            duration_ms: 0,
            volume_percent: None,
            supports_volume: None,
            actions: None,
        };
        let as_track = status_track_key(
            &crate::spotify::NowPlaying {
                media: media.clone(),
                episode: None,
                context: crate::spotify::PlaybackContext::default(),
            },
            &config,
        );
        let as_episode = status_track_key(
            &crate::spotify::NowPlaying {
                media,
                episode: Some(crate::spotify::EpisodeInfo {
                    show_name: "The Deep Work Show".to_string(),
                    publisher: "Acme Audio".to_string(),
                }),
                context: crate::spotify::PlaybackContext::default(),
            },
            &config,
        );
        assert_ne!(as_track, as_episode);
        // A publisher change on the same episode is also a new key — the
        // `{publisher}` token may be part of the posted text.
        let other_publisher = status_track_key(
            &crate::spotify::NowPlaying {
                media: crate::spotify::TrackInfo {
                    title: "Episode 12".to_string(),
                    artist: "The Deep Work Show".to_string(),
                    album: String::new(),
                    album_art_url: String::new(),
                    is_playing: true,
                    progress_ms: Some(0),
                    duration_ms: 0,
                    volume_percent: None,
                    supports_volume: None,
                    actions: None,
                },
                episode: Some(crate::spotify::EpisodeInfo {
                    show_name: "The Deep Work Show".to_string(),
                    publisher: "Other Audio".to_string(),
                }),
                context: crate::spotify::PlaybackContext::default(),
            },
            &config,
        );
        assert_ne!(as_episode, other_publisher);
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
            DEBOUNCE_RETRY_SECONDS, 1,
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
        let source = include_str!("poll_once.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("poll_once.rs has no #[cfg(test)] mod tests block");
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
            body.contains("return DEBOUNCE_RETRY_SECONDS;"),
            "the debounce branch must park on the short fixed retry, not playing_track_sleep (issue #364)"
        );
        assert_eq!(
            DEBOUNCE_RETRY_SECONDS, 1,
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
        let source = include_str!("poll_once.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("poll_once.rs has no #[cfg(test)] mod tests block");
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
            Some(AppConfig {
                status_rules: StatusRulesConfig {
                    quiet_hours: Vec::new(),
                    track_rules: rules,
                    ..StatusRulesConfig::default()
                },
                ..AppConfig::default()
            })
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

    /// S4 (issue #672): the quiet-hours "pause polling" gate. Only the ACTIVE
    /// entry can stop polling, the skip sleeps for the configured ceiling, and
    /// the decision follows the clock — so the window ending resumes polling by
    /// itself, with no thread stop or park.
    #[test]
    fn test_quiet_pause_gate_follows_the_window_and_the_pause_flag() {
        use crate::config::{AppConfig, QuietHoursEntry, StatusRulesConfig};
        let config = |pause_polling: bool, enabled: bool, start: u16, end: u16| AppConfig {
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
        let mut second_window_pauses = config(false, true, 0, 1439);
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
            quiet_pause_at(&Some(second_window_pauses), 1300, 3),
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
        let mut two_pausing = config(true, true, 1320, 420);
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
            quiet_pause_at(&Some(two_pausing), 1350, 3),
            Some((60, 1380)),
            "with two live pausing windows the log reports the latest end"
        );

        // The sleep follows the user's ceiling.
        let mut slow = paused.clone();
        slow.polling.max_interval_seconds = 300;
        assert_eq!(quiet_pause_at(&Some(slow), 1380, 3), Some((300, 420)));
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

    /// S4 (issue #672): the two manual-status texts replace the hardcoded
    /// literals — a default (or absent) config renders exactly what 4.6 posted,
    /// and a configured text is what gets posted, emoji prefix included.
    #[test]
    fn test_manual_status_placeholders_use_the_configured_text() {
        use crate::config::AppConfig;
        let default = Some(AppConfig::default());
        assert_eq!(paused_status_placeholder(&default), "\u{1F3B5} Paused");
        assert_eq!(
            stopped_status_placeholder(&default),
            "\u{1F3B5} Nothing playing on Spotify"
        );
        // No config loaded: the same fallbacks a default config renders.
        assert_eq!(paused_status_placeholder(&None), "\u{1F3B5} Paused");
        assert_eq!(
            stopped_status_placeholder(&None),
            "\u{1F3B5} Nothing playing on Spotify"
        );

        let mut custom = AppConfig::default();
        custom.teams.paused_status_format = "Back in 5".to_string();
        custom.teams.stopped_status_format = "Idle".to_string();
        assert_eq!(
            paused_status_placeholder(&Some(custom.clone())),
            "\u{1F3B5} Back in 5"
        );
        assert_eq!(stopped_status_placeholder(&Some(custom)), "\u{1F3B5} Idle");
        // An EMPTY field means "back to the default": posting a bare emoji
        // would be a status message that no longer names the state.
        let mut emptied = AppConfig::default();
        emptied.teams.paused_status_format = String::new();
        emptied.teams.stopped_status_format = String::new();
        assert_eq!(
            paused_status_placeholder(&Some(emptied)),
            "\u{1F3B5} Paused"
        );
        assert_eq!(
            stopped_status_placeholder(&Some(AppConfig::default())),
            "\u{1F3B5} Nothing playing on Spotify"
        );
        assert_eq!(
            paused_status_text(&Some(AppConfig::default())),
            DEFAULT_PAUSED_STATUS_FORMAT
        );
        assert_eq!(
            stopped_status_text(&Some(AppConfig::default())),
            DEFAULT_STOPPED_STATUS_FORMAT
        );
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
            .find("load_write_clocks()")
            .expect("polling_loop must still snapshot the shared clocks");
        let run = loop_source
            .find("super::poll_once::run(")
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
            gate_recheck_due(None, now, now_wall, None),
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
        let stale = Some(now - std::time::Duration::from_secs(STATUS_KEEPALIVE_SECONDS + 1));
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
    #[test]
    fn test_five_strikes_exit_emits_spotify_reconnect() {
        let source = include_str!("poll_once.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("poll_once.rs has no #[cfg(test)] mod tests block");
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

    /// Issue #455-residual: the no-track clear path must mirror the
    /// process_track ExpiredToken refresh+single-retry — a 401 on the clear
    /// can mean the token expired mid-sequence even though the pre-write
    /// expiry check passed. Brace-counted `handle_no_track` body isolation
    /// (house style — never boundary anchors, which drift).
    #[test]
    fn test_no_track_clear_retries_expired_token_after_refresh() {
        let source = include_str!("poll_once.rs");
        let prod_source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("poll_once.rs has no #[cfg(test)] mod tests block");
        let after_sig = prod_source
            .split("pub(crate) fn handle_no_track(")
            .nth(1)
            .expect("handle_no_track definition not found");
        let open = after_sig
            .find('{')
            .expect("handle_no_track has no opening brace");
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
        let body = &after_sig[..end.expect("handle_no_track body never closed")];
        assert!(
            body.contains("TeamsApiError::ExpiredToken(status)"),
            "handle_no_track must reactively match ExpiredToken on the clear (issue #455-residual)"
        );
        assert!(
            body.contains("refresh_teams_token(&teams_tok)"),
            "handle_no_track must refresh once before blaming the credential"
        );
        assert!(
            body.contains("clear_teams_status_message("),
            "handle_no_track must retry the clear with the refreshed token"
        );
        assert!(
            body.contains("teams-reconnect-required"),
            "a dead-credential clear must surface teams-reconnect-required"
        );
    }

    /// Production source with the test module stripped — the shared preamble
    /// for the structural guards below.
    fn prod_source() -> &'static str {
        include_str!("poll_once.rs")
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("poll_once.rs has no #[cfg(test)] mod tests block")
    }

    /// Brace-counted body isolation for a production fn (house style — never
    /// boundary anchors, which drift).
    fn prod_fn_body<'a>(prod: &'a str, sig: &str) -> &'a str {
        let after_sig = prod
            .split(sig)
            .nth(1)
            .unwrap_or_else(|| panic!("production source has no `{}`", sig));
        let open = after_sig
            .find('{')
            .unwrap_or_else(|| panic!("`{}` has no opening brace", sig));
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
        &after_sig[..end.unwrap_or_else(|| panic!("`{}` body never closed", sig))]
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
        let config = Some(config);
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
        let narrow = Some(narrow);
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
        let inverted = Some(inverted);
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
        let narrow = Some(narrow);
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
        let prod = prod_source();
        let track_body = prod_fn_body(prod, "pub(crate) fn process_track(");
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

        let no_track_body = prod_fn_body(prod, "pub(crate) fn handle_no_track(");
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
            Some(AppConfig {
                status_rules: StatusRulesConfig {
                    quiet_hours: Vec::new(),
                    track_rules: rules,
                    ..StatusRulesConfig::default()
                },
                ..AppConfig::default()
            })
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

    /// Finding PollCore#4 (issue #572): the manual refresh reads the session's
    /// write clocks. Pre-fix its fresh `last_availability_arm = None` re-armed
    /// the availability session on every refresh, and its fresh
    /// `last_track_key`/`last_posted_status`/`last_teams_update` bypassed the
    /// #384 identical-write guard.
    #[test]
    fn test_shared_write_clocks_prevent_one_shot_rearm_and_duplicate_write() {
        let _guard = global_state_lock();
        let now = Instant::now();
        // Finding D11 (issue #694): the shared slot is generation-checked, so a
        // test snapshot must be the one `load_write_clocks` just handed out —
        // building a `WriteClocks::default()` (generation 0) and storing it
        // would now be discarded as superseded.
        let mut armed = load_write_clocks();
        armed.last_availability_arm = Some(now);
        store_write_clocks(&armed);
        let loaded = load_write_clocks();
        assert_eq!(
            loaded.last_availability_arm,
            Some(now),
            "the one-shot path must observe the session's arm clock"
        );
        assert!(
            !should_rearm_availability(loaded.last_availability_arm, now),
            "a manual refresh must not re-arm a presence session armed seconds ago"
        );

        // The same shared clocks keep #384 effective: the one-shot sees the
        // same track key (not `changed`) and the same posted text, so an
        // unchanged track skips the POST instead of duplicating it.
        let posted = "🎵 A - T 🎧".to_string();
        let key = "A - T | filter=true".to_string();
        let clocks = WriteClocks {
            last_track_key: Some(key.clone()),
            last_posted_status: Some(posted.clone()),
            last_teams_update: Some(now),
            ..WriteClocks::default()
        };
        let changed = clocks.last_track_key.as_ref() != Some(&key);
        assert!(
            !changed,
            "sharing the track key is what makes the one-shot read 'unchanged'"
        );
        assert!(
            should_skip_identical_write(
                changed,
                clocks.last_posted_status.as_deref(),
                &posted,
                clocks.last_teams_update,
                now,
                false,
            ),
            "a manual refresh must honor the #384 identical-write guard"
        );

        reset_write_clocks();
        let cold = load_write_clocks();
        assert!(
            cold.last_availability_arm.is_none() && cold.last_track_key.is_none(),
            "a stopped session must leave cold clocks, so the next session treats its \
             first track as changed (issue #373/#572)"
        );
        assert!(
            should_rearm_availability(cold.last_availability_arm, now),
            "a genuinely cold app must still arm the availability session"
        );
    }

    /// Finding PollCore#4 (issue #572) structural guard: the one-shot entry
    /// must run against the shared clocks rather than fresh per-call locals.
    #[test]
    fn test_run_oneshot_uses_shared_write_clocks() {
        let prod = prod_source();
        let body = prod_fn_body(prod, "pub(crate) fn run_oneshot(");
        assert!(
            body.contains("let mut clocks = load_write_clocks();"),
            "run_oneshot must load the shared write clocks"
        );
        assert!(
            body.contains("store_write_clocks(&clocks);"),
            "run_oneshot must publish the clocks it advanced back to the shared slot"
        );
        assert!(
            !body.contains("let mut last_availability_arm: Option<Instant> = None;"),
            "a fresh availability clock on the one-shot path is exactly the issue #572 defect"
        );
    }

    /// Finding PollCore#4 (issue #572): a polling session resets the shared
    /// clocks on start and on exit, so a new session (or a refresh issued
    /// while nothing runs) never inherits a dead session's clocks.
    #[test]
    fn test_polling_lifecycle_resets_shared_write_clocks() {
        let loop_source = include_str!("loop.rs");
        assert!(
            loop_source.contains("reset_write_clocks()"),
            "polling_loop must reset the shared clocks when the session ends"
        );
        let state_source = include_str!("state.rs");
        assert!(
            state_source.contains("reset_write_clocks()"),
            "start_polling must reset the shared clocks so a panic-dead session's \
             clocks cannot leak into the next one"
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
            Some(c)
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

    /// Finding #635: the read-before-write policy, as a truth table.
    #[test]
    fn test_manual_status_blocks_write_policy() {
        use crate::teams::{PresenceInfo, PresenceStatusMessage};
        let now = Utc::now();
        let sample = |content: &str, expires: Option<chrono::DateTime<Utc>>| PresenceInfo {
            availability: "available".to_string(),
            activity: "available".to_string(),
            status_message: Some(PresenceStatusMessage {
                content: content.to_string(),
                expires_at: expires,
            }),
            ..PresenceInfo::default()
        };

        // A message the user typed blocks the write (this is the fix).
        assert!(manual_status_blocks_write(
            true,
            Some(&sample("In a workshop until 3", None)),
            None,
            None,
            now,
        ));
        // Our own last playing status / placeholder does not.
        assert!(!manual_status_blocks_write(
            true,
            Some(&sample("🎵 A - B 🎧", None)),
            Some("🎵 A - B 🎧"),
            None,
            now,
        ));
        assert!(!manual_status_blocks_write(
            true,
            Some(&sample("🎵 Paused", None)),
            None,
            Some("🎵 Paused"),
            now,
        ));
        // An expired message is stale by definition (our placeholders and
        // status writes both carry a near-term expiryDateTime).
        assert!(!manual_status_blocks_write(
            true,
            Some(&sample(
                "In a workshop until 3",
                Some(now - chrono::Duration::seconds(30))
            )),
            None,
            None,
            now,
        ));
        // Still-live expiry + a different text = authored.
        assert!(manual_status_blocks_write(
            true,
            Some(&sample(
                "In a workshop until 3",
                Some(now + chrono::Duration::hours(1))
            )),
            None,
            None,
            now,
        ));
        // Fail-open paths: the flag is off, there is no sample, or the live
        // message is empty/whitespace.
        assert!(!manual_status_blocks_write(
            false,
            Some(&sample("In a workshop until 3", None)),
            None,
            None,
            now,
        ));
        assert!(!manual_status_blocks_write(true, None, None, None, now));
        assert!(!manual_status_blocks_write(
            true,
            Some(&sample("   ", None)),
            None,
            None,
            now,
        ));
        // Whitespace-only difference is still "ours".
        assert!(!manual_status_blocks_write(
            true,
            Some(&sample(" 🎵 A - B 🎧 ", None)),
            Some("🎵 A - B 🎧"),
            None,
            now,
        ));
    }

    /// Finding #635/#637: the ONE gate decision the read sites share — the
    /// presence reasons outrank the manual-status reason, and each opt-in flag
    /// gates only its own reason. Issue #872 extends the table with the
    /// OS-level presentation signal — at the LOWEST precedence of the
    /// presence-class reasons so it can never outrank busy or in-a-call.
    #[test]
    fn test_presence_gate_decision_precedence_and_opt_ins() {
        use crate::platform::focus::PresentationState;
        use crate::teams::{PresenceInfo, PresenceStatusMessage};
        let now = Utc::now();
        let busy_manual = PresenceInfo {
            availability: "busy".to_string(),
            activity: "available".to_string(),
            status_message: Some(PresenceStatusMessage {
                content: "In a workshop".to_string(),
                expires_at: None,
            }),
            ..PresenceInfo::default()
        };
        let manual = PresenceInfo {
            availability: "available".to_string(),
            activity: "available".to_string(),
            status_message: Some(PresenceStatusMessage {
                content: "In a workshop".to_string(),
                expires_at: None,
            }),
            ..PresenceInfo::default()
        };
        let ooo = PresenceInfo {
            availability: "available".to_string(),
            activity: "available".to_string(),
            out_of_office: true,
            ..PresenceInfo::default()
        };

        // Issue #872: a local helper so the 9-argument call sites stay
        // readable. Default opt-ins (no presentation gate, no idle gate).
        let decide = |presence: &PresenceInfo,
                      gate: bool,
                      ooo: bool,
                      manual_check: bool,
                      ps: PresentationState,
                      gate_pres: bool,
                      idle: bool|
         -> Option<String> {
            presence_gate_decision(
                presence,
                gate,
                ooo,
                manual_check,
                None,
                None,
                now,
                ps,
                gate_pres,
                idle,
            )
        };

        // Busy wins over the manual message (the more specific state).
        assert_eq!(
            decide(
                &busy_manual,
                true,
                false,
                true,
                PresentationState::None,
                false,
                false
            )
            .as_deref(),
            Some("busy")
        );
        // Manual status is reported under its own reason.
        assert_eq!(
            decide(
                &manual,
                true,
                false,
                true,
                PresentationState::None,
                false,
                false
            )
            .as_deref(),
            Some(GATE_REASON_MANUAL_STATUS)
        );
        // Turning the manual check off leaves only the presence gate.
        assert_eq!(
            decide(
                &manual,
                true,
                false,
                false,
                PresentationState::None,
                false,
                false
            ),
            None
        );
        // OOO participates only when opted in.
        assert_eq!(
            decide(
                &ooo,
                true,
                false,
                true,
                PresentationState::None,
                false,
                false
            ),
            None
        );
        assert_eq!(
            decide(
                &ooo,
                true,
                true,
                true,
                PresentationState::None,
                false,
                false
            )
            .as_deref(),
            Some(crate::teams::GATE_REASON_OUT_OF_OFFICE)
        );
        // The manual check survives the presence gate being switched off —
        // that is the one case where it costs an extra Graph read.
        assert_eq!(
            decide(
                &manual,
                false,
                false,
                true,
                PresentationState::None,
                false,
                false
            )
            .as_deref(),
            Some(GATE_REASON_MANUAL_STATUS)
        );

        // Finding #637: a rule with its own presence action overrides the OOO
        // default; a user who is busy is still gated regardless.
        let cfg = Some(crate::config::AppConfig::default());
        assert!(!ooo_gate_enabled(&cfg, true));
        let mut on = crate::config::AppConfig::default();
        on.teams.gate_when_out_of_office = true;
        assert!(ooo_gate_enabled(&Some(on.clone()), false));
        assert!(!ooo_gate_enabled(&Some(on), true));

        // Issue #872: busy STILL outranks the OS-level presentation
        // signal — the Graph sample is the more specific real-world state.
        assert_eq!(
            decide(
                &busy_manual,
                true,
                false,
                false,
                PresentationState::Presentation,
                true,
                false,
            )
            .as_deref(),
            Some("busy"),
            "a busy Graph sample must outrank the OS-level presentation signal"
        );
        // Issue #872: the presentation signal participates only when
        // opted in. The default behaviour (gate_when_presenting=false)
        // leaves an `Available` user + `Presentation` shell state
        // unblocked, exactly like 4.7.
        assert_eq!(
            decide(
                &manual,
                true,
                false,
                false,
                PresentationState::Presentation,
                false,
                false
            ),
            None,
            "without the opt-in, the OS-level presentation signal is silent"
        );
        // Issue #872: with the opt-in on, the presentation signal gates
        // a write that nothing else has blocked.
        assert_eq!(
            decide(
                &manual,
                true,
                false,
                false,
                PresentationState::Presentation,
                true,
                false
            )
            .as_deref(),
            Some(crate::teams::GATE_REASON_PRESENTING)
        );
        // Issue #872: `FullScreen` collapses to the same wire reason.
        assert_eq!(
            decide(
                &manual,
                true,
                false,
                false,
                PresentationState::FullScreen,
                true,
                false
            )
            .as_deref(),
            Some(crate::teams::GATE_REASON_PRESENTING)
        );
        // Issue #872: `QuietTime` is its own wire spelling.
        assert_eq!(
            decide(
                &manual,
                true,
                false,
                false,
                PresentationState::QuietTime,
                true,
                false
            )
            .as_deref(),
            Some(crate::teams::GATE_REASON_QUIET_TIME)
        );
        // Issue #872: an `Unknown` probe (Linux/macOS, or a Windows
        // probe error) collapses to an empty reason — failing open.
        assert_eq!(
            decide(
                &manual,
                true,
                false,
                false,
                PresentationState::Unknown,
                true,
                false
            ),
            None,
            "an Unknown probe must fail open, not block"
        );

        // Issue #873: the idle gate is the LOWEST precedence of all —
        // only ever blocks a write nothing else already blocked.
        assert_eq!(
            decide(
                &manual,
                true,
                false,
                false,
                PresentationState::None,
                false,
                true
            )
            .as_deref(),
            Some(crate::teams::GATE_REASON_IDLE)
        );
        // Issue #873: the manual-status verdict still outranks the idle
        // reading — a user-typed status message must not be clobbered
        // by an idle classification.
        assert_eq!(
            decide(
                &manual,
                true,
                false,
                true,
                PresentationState::None,
                false,
                true
            )
            .as_deref(),
            Some(GATE_REASON_MANUAL_STATUS),
            "manual-status outranks idle"
        );
        // Issue #873: a busy Graph sample still outranks the idle
        // reading — the same precedence contract as #872.
        assert_eq!(
            decide(
                &busy_manual,
                true,
                false,
                false,
                PresentationState::None,
                false,
                true
            )
            .as_deref(),
            Some("busy"),
            "busy outranks idle"
        );
    }

    /// Finding #636: the requested `expirationDuration` is bounded by the real
    /// listening time and clamped into the documented PT5M..PT4H window.
    #[test]
    fn test_presence_expiration_duration_is_bounded_by_listening_time() {
        // Unknown position (live stream, issue #165) keeps the ceiling.
        assert_eq!(presence_expiration_duration(None), "PT4H");
        // A track with 3:30 left: 210s + one re-arm period of slack.
        assert_eq!(
            presence_expiration_duration(Some(210_000)),
            format!("PT{}S", 210 + AVAILABILITY_REARM_SECONDS)
        );
        // A track about to end floors at the documented PT5M — the shortest
        // session Graph accepts (and still inside the 5-minute Available fade).
        assert_eq!(
            presence_expiration_duration(Some(1_000)),
            format!("PT{}S", PRESENCE_EXPIRATION_MIN_SECONDS)
        );
        // A four-hour DJ set ceilings at PT4H.
        assert_eq!(
            presence_expiration_duration(Some(6 * 60 * 60 * 1000)),
            format!("PT{}S", PRESENCE_EXPIRATION_MAX_SECONDS)
        );
        // Every arm therefore stays inside Graph's documented window.
        for remaining in [0_u64, 1_000, 60_000, 900_000, 6 * 60 * 60 * 1000] {
            let value = presence_expiration_duration(Some(remaining));
            let secs: u64 = value
                .trim_start_matches("PT")
                .trim_end_matches('S')
                .parse()
                .expect("the duration is a PT<n>S string");
            assert!(
                (PRESENCE_EXPIRATION_MIN_SECONDS..=PRESENCE_EXPIRATION_MAX_SECONDS).contains(&secs),
                "{} out of the documented PT5M..PT4H window",
                value
            );
        }
    }

    /// Finding #634: a rule that starts or stops matching switches the bubble
    /// on the next iteration; an unchanged pair keeps the 4-minute cadence.
    #[test]
    fn test_should_arm_presence_switches_immediately_and_keeps_cadence() {
        let away = PresencePair {
            availability: "Away".to_string(),
            activity: "Away".to_string(),
        };
        let listening = PresencePair {
            availability: "Available".to_string(),
            activity: "Available".to_string(),
        };
        let now = Instant::now();
        let fresh = Some(now - std::time::Duration::from_secs(30));

        // Cold start: arm.
        assert!(should_arm_presence(None, &away, None, now));
        // Same pair inside the cadence: do not re-POST.
        assert!(!should_arm_presence(Some(&away), &away, fresh, now));
        // A DIFFERENT pair arms immediately, even seconds after the last arm —
        // leaving a quiet-hours rule must not wait out the cadence.
        assert!(should_arm_presence(Some(&away), &listening, fresh, now));
        // Unchanged pair past the cadence re-arms (the Available fade window).
        let stale = Some(now - std::time::Duration::from_secs(AVAILABILITY_REARM_SECONDS));
        assert!(should_arm_presence(Some(&away), &away, stale, now));
    }

    // ---------------------------------------------------------------
    // Findings D1/D3/D4/D6/D7/D11 (issues #684/#686/#687/#689/#690/#694):
    // the exit-time cleanup, the gate/dedup ordering on both placeholder
    // clears, the pause-as-state-change store and the clock generation guard.
    // ---------------------------------------------------------------

    /// Serialises the tests that mutate the process-wide clock / exit-snapshot
    /// statics. ONE lock for both module's tests (it lives in `state.rs`, next
    /// to the snapshot): `cargo test` runs tests in parallel threads, the slots
    /// are shared, and several tests touch both — an interleaved reset would
    /// make the generation / snapshot assertions flaky.
    fn global_state_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::polling::state::global_state_lock()
    }

    /// Finding D1 (issue #684): `polling_loop`'s exit tail empties the
    /// write-decision clocks BEFORE `RunEvent::Exit` runs the cleanup, so the
    /// cleanup must decide from the exit snapshot. Pre-fix this test's
    /// assertions about the snapshot did not exist and the cleanup read the
    /// (already cold) clocks — a quit mid-song left the music status and the
    /// armed `Available` session live.
    #[test]
    fn test_exit_cleanup_survives_the_loop_exit_tail() {
        let _guard = global_state_lock();
        crate::polling::state::reset_exit_snapshot();
        // A session that posted a playing status AND armed a listening session.
        crate::polling::state::record_posted_status(Some("\u{1F3B5} A - T \u{1F3A7}"));
        crate::polling::state::record_armed_presence(Some((
            "Available",
            "Available",
            "Listening (Available)",
        )));

        // The loop's exit tail (what runs before `RunEvent::Exit`).
        reset_write_clocks();

        let snapshot = crate::polling::state::load_exit_snapshot();
        assert_eq!(
            snapshot.last_posted_status.as_deref(),
            Some("\u{1F3B5} A - T \u{1F3A7}"),
            "the posted playing status must survive the clock reset (finding D1)"
        );
        assert_eq!(
            snapshot.armed_presence.as_ref().map(|(a, _, _)| a.as_str()),
            Some("Available"),
            "the armed presence session must survive the clock reset (finding D1)"
        );
        assert_eq!(
            exit_cleanup_plan(&snapshot, true, true),
            ExitCleanupPlan {
                clear_presence: true,
                post_placeholder: true,
            },
            "a quit mid-song must still clear the presence session and replace the \
             playing status (finding D1)"
        );

        // The pre-fix source of truth is provably empty at this point — which
        // is exactly why the cleanup used to skip both calls.
        let cold = load_write_clocks();
        let from_cold_clocks = ExitCleanupPlan {
            clear_presence: cold.last_availability_arm.is_some(),
            post_placeholder: cold.last_posted_status.is_some(),
        };
        assert_eq!(
            from_cold_clocks,
            ExitCleanupPlan::default(),
            "the exit tail empties the clocks, so deciding the cleanup from them \
             (the pre-fix code) does nothing"
        );

        // A session BOUNDARY must not forget it either: stopping and restarting
        // sync does not change what Teams shows, so a stop→start→quit sequence
        // must still clean up (the clocks' own session reset cannot be the
        // snapshot's model).
        reset_write_clocks();
        assert_eq!(
            crate::polling::state::load_exit_snapshot(),
            snapshot,
            "a session boundary must not forget the residue a previous session \
             left on Teams (finding D1)"
        );

        // Only a completed exit cleanup retires it.
        crate::polling::state::reset_exit_snapshot();
        assert_eq!(
            exit_cleanup_plan(&crate::polling::state::load_exit_snapshot(), true, true),
            ExitCleanupPlan::default()
        );
    }

    /// Finding D1 (issue #684) structural guard: the cleanup reads the
    /// snapshot, and the loop's exit tail never resets it.
    #[test]
    fn test_clear_presence_on_exit_reads_the_snapshot_not_the_clocks() {
        let prod = prod_source();
        let body = prod_fn_body(prod, "pub(crate) fn clear_presence_on_exit(");
        assert!(
            body.contains("load_exit_snapshot()"),
            "the exit cleanup must read the exit snapshot (finding D1)"
        );
        assert!(
            !body.contains("load_write_clocks()"),
            "the exit cleanup must NOT read the clocks: polling_loop's exit tail \
             resets them before RunEvent::Exit runs this cleanup (finding D1)"
        );
        assert!(
            !body.contains("clocks.last_availability_arm"),
            "the pre-fix early-return on default clocks is the D1 defect"
        );
        let loop_source = include_str!("loop.rs");
        assert!(
            loop_source.contains("reset_write_clocks()"),
            "polling_loop must still reset the shared clocks when the session ends"
        );
        assert!(
            !loop_source.contains("reset_exit_snapshot()"),
            "the loop's exit tail must not reset the exit snapshot — that would \
             resurrect finding D1"
        );
        let state_source = include_str!("state.rs");
        let start_body = prod_fn_body(state_source, "pub fn start_polling(");
        assert!(
            !start_body.contains("reset_exit_snapshot"),
            "a session START must not clear the snapshot: Teams keeps showing the \
             previous session's status, so a stop→start→quit would skip the \
             cleanup (finding D1)"
        );
        assert!(
            body.contains("reset_exit_snapshot();"),
            "a completed exit cleanup must retire the snapshot so a repeated \
             RunEvent::Exit is a no-op"
        );
    }

    /// Finding D5 (issue #688): a poller that stops itself must announce it.
    /// Structural because the emitter lives in the spawned thread's
    /// ownership-checked exit block, which needs a live `AppHandle`.
    #[test]
    fn test_self_terminating_poller_emits_sync_stopped() {
        let state_source = include_str!("state.rs");
        let body = prod_fn_body(state_source, "pub fn start_polling(");
        // #675: the self-termination marker, pinned once and reused below.
        let self_terminated_emit =
            "app.emit(\"sync-stopped\", json!({ \"self_terminated\": true }))";
        assert!(
            body.contains(self_terminated_emit),
            "the poller's own thread-exit point must emit sync-stopped \
             (finding D5) — otherwise the Dashboard mirror stays on \"Syncing\" \
             and the tray on \"Pause Sync\""
        );
        assert!(
            body.contains("exit_reason"),
            "the self-termination log line must name the exit reason (finding D5)"
        );
        let owner = body
            .find("if is_owner {")
            .expect("start_polling must keep the ownership check");
        let gate = body.find("let stop_requested =").expect(
            "the announce decision must hinge on whether a stop was requested (D5/round 2)",
        );
        let emit = body.find(self_terminated_emit).expect("sync-stopped emit");
        assert!(
            emit > owner,
            "the emit must sit inside the ownership-checked block (finding D5), so a \
             superseded thread's exit cannot report a stop for the live one"
        );
        assert!(
            emit > gate,
            "the emit must be decided by `stop_requested`, not before it"
        );
        assert!(
            !body[gate..emit].contains("is_syncing"),
            "the announce decision must NOT hinge on `is_syncing` (review round 2, item 2): \
             an explicit stop leaves it true until after the join (so gating on it emitted \
             a SECOND sync-stopped) while a self-terminating exit may leave it false (so it \
             emitted NONE). `stop_polling` clearing the stored stop sender is the requested-\
             stop signal, and commands::sync::stop_syncing owns that path's emit."
        );
        // Review round 3, item 1: the SIGNAL itself must be pinned, not just its
        // use — `let stop_requested = false;` (the round-1 double-emit behaviour)
        // otherwise leaves every test green.
        assert!(
            body.contains("let stop_requested = state_for_cleanup.polling.stop_tx().is_none();"),
            "the requested-stop signal must be the stored stop sender being gone: only \
             `stop_polling` clears it, so `is_none()` is exactly 'a stop was requested and \
             commands::sync::stop_syncing owns the emit'. Any other derivation (or a hard-\
             coded false) re-inverts the polarity (review round 3, item 1)."
        );
        // #675: the two emitters must DIFFER on the payload — that difference
        // is the only way a notification consumer can tell the surprise (a
        // self-termination, which toasts) from the stop the user just asked for
        // (which must not). Both markers are pinned so neither can drift back
        // to the old unit payload, which made the two indistinguishable.
        let sync_source: String = include_str!("../commands/sync.rs")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            sync_source.contains(
                "app.emit( \"sync-stopped\", serde_json::json!({ \"self_terminated\": false }), )"
            ),
            "commands::sync::stop_syncing owns the USER-requested stop and must say so \
             (`self_terminated: false`), so #675's sync_stopped notification stays quiet for it"
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
            .find("let placeholder_text = paused_status_placeholder(config);")
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

    /// Finding D7 (issue #690): a pause is not a stop.
    #[test]
    fn test_presence_paused_is_distinct_from_presence_cleared() {
        let prod = prod_source();
        let track_body = prod_fn_body(prod, "pub(crate) fn process_track(");
        assert!(
            track_body.contains("\"presence-paused\""),
            "the paused-clear POST must announce presence-paused (finding D7)"
        );
        assert!(
            !track_body.contains("\"presence-cleared\""),
            "a pause must never be reported as a stop (finding D7)"
        );
        let no_track_body = prod_fn_body(prod, "pub(crate) fn handle_no_track(");
        assert!(
            no_track_body.contains("\"presence-cleared\""),
            "the genuine no-track path keeps presence-cleared"
        );
    }

    /// Finding D11 (issue #694): a snapshot whose generation was superseded
    /// must not land — that is the write-back that silently dropped
    /// `gated_track_key` and let a write through mid-meeting.
    #[test]
    fn test_superseded_clock_snapshot_is_discarded() {
        let _guard = global_state_lock();
        reset_write_clocks();

        // The loop's iteration: loads, records the gate it observed, stores.
        let mut iteration = load_write_clocks();
        iteration.gated_track_key = Some("A - T | filter=true".to_string());
        store_write_clocks(&iteration);
        assert_eq!(
            load_write_clocks().gated_track_key.as_deref(),
            Some("A - T | filter=true"),
            "a snapshot loaded from the current slot must land"
        );

        // A concurrent one-shot that loaded BEFORE that gate decision finishes
        // afterwards and writes its pre-gate view back.
        let mut stale = WriteClocks {
            last_track_key: Some("A - T | filter=true".to_string()),
            last_posted_status: Some("\u{1F3B5} stale \u{1F3A7}".to_string()),
            ..WriteClocks::default()
        };
        stale.generation = iteration.generation;
        store_write_clocks(&stale);

        let landed = load_write_clocks();
        assert!(
            landed.gated_track_key.is_some(),
            "a superseded snapshot must never drop the recorded gate (finding D11)"
        );
        assert_eq!(
            landed.last_posted_status, None,
            "the superseded snapshot must not land at all (finding D11)"
        );

        // A reset also moves the generation on, so a dead session's snapshot
        // cannot resurrect itself afterwards.
        let dead = WriteClocks {
            generation: landed.generation,
            ..WriteClocks::default()
        };
        reset_write_clocks();
        store_write_clocks(&dead);
        assert_eq!(
            load_write_clocks().generation,
            dead.generation.wrapping_add(1),
            "the reset's generation must not be overwritten by a pre-reset snapshot"
        );
    }
    /// Review follow-up on #684: the no-track path owns `gated_track_key`, so a
    /// gate recorded for a track cannot outlive it — while a suppression with
    /// nothing playing stays representable.
    #[test]
    fn test_no_track_path_retires_the_finished_tracks_gate() {
        // A real status key always carries the kind/fingerprint separators, so
        // the sentinel can never collide with one.
        let finished_track_key = status_track_key(&crate::spotify::NowPlaying::default(), &None);
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
            .find("let placeholder_text = paused_status_placeholder(config);")
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

    /// Review round 2, item 5: `presence-paused` and `playback-state-changed`
    /// are cross-slice contracts (the Dashboard reads `status` / `is_playing`,
    /// the tray reads `track_key`), so their serialized shapes are pinned.
    #[test]
    fn test_event_payload_shapes_are_pinned() {
        assert_eq!(
            presence_paused_payload("\u{1F3B5} Paused"),
            json!({ "status": "\u{1F3B5} Paused" }),
            "presence-paused is {{ status }} — the Dashboard renders that text"
        );
        assert_eq!(
            playback_state_changed_payload(false, "A - T | track | f"),
            json!({ "is_playing": false, "track_key": "A - T | track | f" }),
            "playback-state-changed is {{ is_playing, track_key }} — the Dashboard and \
             the tray both consume it"
        );
        let value = playback_state_changed_payload(true, "k");
        let obj = value.as_object().expect("an object payload");
        assert_eq!(
            obj.len(),
            2,
            "exactly is_playing + track_key: a renamed or extra field is a silent \
             break for S2/the tray (review round 2, item 5)"
        );
        assert!(obj.contains_key("is_playing") && obj.contains_key("track_key"));

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

    /// Review round 2, item 7: quitting must not replace a Teams status the user
    /// typed with our "Paused" placeholder — while our own armed availability
    /// session is still cleared.
    #[test]
    fn test_exit_plan_respects_a_manual_teams_status() {
        let snapshot = crate::polling::state::ExitSnapshot {
            last_posted_status: Some("\u{1F3B5} A - T \u{1F3A7}".to_string()),
            armed_presence: Some((
                "Available".to_string(),
                "Available".to_string(),
                "Listening (Available)".to_string(),
            )),
            manual_status_blocks: false,
        };
        assert_eq!(
            exit_cleanup_plan(&snapshot, true, true),
            ExitCleanupPlan {
                clear_presence: true,
                post_placeholder: true,
            },
            "with no manual status observed, the quit cleanup replaces our own status"
        );

        let manual = crate::polling::state::ExitSnapshot {
            manual_status_blocks: true,
            ..snapshot.clone()
        };
        assert_eq!(
            exit_cleanup_plan(&manual, true, true),
            ExitCleanupPlan {
                clear_presence: true,
                post_placeholder: false,
            },
            "a status the USER owns must survive the quit (the shipped 4.6 \
             respect-the-manual-status behaviour), while our own armed presence \
             session is still cleared"
        );

        // The verdict the plan reads is the poller's own predicate, recorded at
        // every gate read (see process_track's `gate_verdict`).
        let prod = prod_source();
        let track_body = prod_fn_body(prod, "pub(crate) fn process_track(");
        assert!(
            track_body.contains("observe_presence_sample("),
            "every gate read must record the manual-status verdict for the exit path \
             (review rounds 2 item 7 / 3 item 3) — the exit path has no Graph sample of \
             its own, so a read that does not record leaves the user's own Teams status \
             exposed on quit"
        );
    }

    /// Review round 3, item 3: "no path may observe a manual status without
    /// recording it". The due mid-track re-check calls `presence_gate_decision`
    /// directly (it does not go through `gate_verdict`), so removing its
    /// `observe_presence_sample` call left a gated track + a user-typed status
    /// with a stale exit snapshot — and quitting then replaced the user's own
    /// Teams message with our placeholder.
    #[test]
    fn test_every_presence_read_records_the_manual_status_verdict() {
        let prod = prod_source();
        assert!(
            prod.contains("fn observe_presence_sample("),
            "the recording path must exist (review round 3, item 3)"
        );
        // The reads that route through `gate_verdict` are covered by its own
        // call; assert that too, so a future edit cannot drop it quietly.
        let verdict_start = prod
            .find("let gate_verdict = |presence:")
            .expect("process_track must keep the shared gate closure");
        let verdict = &prod[verdict_start..(verdict_start + 900).min(prod.len())];
        assert!(
            verdict.contains("observe_presence_sample("),
            "the shared gate closure must record the verdict for its reads"
        );
        // The direct read (due mid-track re-check) must record before deciding.
        let direct = prod
            .find("match presence_gate_decision(")
            .expect("the due mid-track re-check must still read presence (the #430 late-post)");
        let window = &prod[direct.saturating_sub(900)..direct];
        assert!(
            window.contains("observe_presence_sample("),
            "the direct presence read must record the manual-status verdict before deciding \
             (review round 3, item 3) — route it through `gate_verdict`, or call \
             `observe_presence_sample` at the read"
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
    fn snooze_config(stored: Option<&str>, max_interval: u64) -> Option<AppConfig> {
        let mut cfg = AppConfig {
            snooze_until: stored.map(str::to_string),
            ..AppConfig::default()
        };
        cfg.polling.max_interval_seconds = max_interval;
        Some(cfg)
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
            .find("snooze_gate(&config)")
            .expect("polling_loop must consult the snooze gate (S9)");
        let quiet = loop_source
            .find("quiet_pause_iteration(")
            .expect("the quiet-hours pause must still be consulted (S4)");
        let clocks = loop_source
            .find("load_write_clocks()")
            .expect("polling_loop must still snapshot the shared clocks");
        let run = loop_source
            .find("super::poll_once::run(")
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
            loop_source.matches("snooze_gate(&config)").count(),
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
            "*guard = Some(next)",
        ] {
            assert!(
                body.contains(marker),
                "clear_snooze_if_expired must contain `{}`",
                marker
            );
        }
        assert!(
            body.find("save_config(&next)").unwrap() < body.find("*guard = Some(next)").unwrap(),
            "the in-memory config must only be updated after a successful write"
        );
        // A failed write keeps the field, so the next iteration retries instead
        // of leaving an expired deadline on disk that nothing will clear.
        assert!(
            body.contains("retrying next iteration"),
            "a failed clear must say it will retry"
        );
    }

    /// S9 (issue #677): EVERY entry point to an iteration honours the snooze —
    /// the driver's loop is pinned by `snooze_gate_precedes_...` above, and
    /// `run_oneshot` is pinned here. `refresh_status`, the tray's post-action
    /// catch-up and the CLI's `--sync-once` all route through it, so a gate that
    /// only the loop consulted left three silent bypasses of the feature's
    /// "no Spotify or Graph work while snoozed" promise.
    #[test]
    fn run_oneshot_honours_the_snooze_gate() {
        let prod = prod_source();
        let body = prod_fn_body(prod, "pub(crate) fn run_oneshot(");
        let gate = body
            .find("snooze_gate(&config)")
            .expect("run_oneshot must consult the snooze gate (S9)");
        let clocks = body
            .find("load_write_clocks()")
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
}
