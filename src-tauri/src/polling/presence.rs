//! Presence-session management for the polling loop (split from `poll_once.rs`, issue #754).

use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use tauri::{AppHandle, Emitter};

use crate::config::{AppConfig, PresencePair};
use crate::teams::{
    clear_teams_presence, clear_user_preferred_presence, presence_gate_reason, set_teams_presence,
    set_user_preferred_presence, GATE_REASON_IDLE, GATE_REASON_MANUAL_STATUS,
};
use crate::AppState;

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
pub(crate) const AVAILABILITY_REARM_SECONDS: u64 = 4 * 60;

/// Documented bounds of a `setPresence` `expirationDuration` (finding #636,
/// issue #636): "The valid duration range is from 5 to 240 minutes (PT5M to
/// PT4H)", after which the session becomes `Offline`.
/// (Microsoft Learn: graph/api/presence-setpresence, manage-presence-state)
pub(crate) const PRESENCE_EXPIRATION_MIN_SECONDS: u64 = 5 * 60;
pub(crate) const PRESENCE_EXPIRATION_MAX_SECONDS: u64 = 4 * 60 * 60;

/// True when the Available-presence session should be re-armed (issue
/// #3.0-P1): no arm yet, or the last arm is at least
/// `AVAILABILITY_REARM_SECONDS` old. An `Available` session TIMES OUT
/// after 5 minutes (non-configurable; a distinct clock from
/// `expirationDuration`), so the re-arm cadence must be strictly inside
/// that window (4 min < 5 min).
pub(crate) fn should_rearm_availability(last_arm: Option<Instant>, now: Instant) -> bool {
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
pub(crate) fn should_arm_presence(
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
pub(crate) fn presence_expiration_duration(remaining_ms: Option<u64>) -> String {
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
/// COST. One extra `getPresence` per `AVAILABILITY_REARM_SECONDS` (~4
/// minutes) while a track plays: the ungated mid-track re-check (issue
/// #792) shares the gate's sample, so recording the verdict costs no second
/// call. Only a user who turned the presence gate OFF while leaving this
/// check on pays one extra `getPresence` per gate point, because the read
/// is what makes the decision possible.
pub(crate) fn manual_status_blocks_write(
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
pub(crate) fn presence_gate_decision(
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
pub(crate) fn presence_paused_payload(status: &str) -> crate::events::PresencePaused {
    crate::events::PresencePaused {
        status: status.to_string(),
    }
}

/// The `playback-state-changed` payload (finding D6, issue #689).
///
/// Cross-slice contract: the Dashboard reads `is_playing` and the tray re-seeds
/// from `track_key`. Exactly these two fields (review round 2, item 5).
pub(crate) fn playback_state_changed_payload(
    is_playing: bool,
    track_key: &str,
) -> crate::events::PlaybackStateChanged {
    crate::events::PlaybackStateChanged {
        is_playing,
        track_key: track_key.to_string(),
    }
}

pub(crate) fn emit_presence_gated(
    session: &super::state::SessionState,
    app: &AppHandle,
    reason: &str,
    availability: &str,
    activity: &str,
) {
    let _ = app.emit(
        "presence-gated",
        crate::events::PresenceGated {
            reason: reason.to_string(),
            availability: availability.to_string(),
            activity: activity.to_string(),
            timestamp: Utc::now().to_rfc3339(),
        },
    );
    // Issue #863: publish the gate reason into the shared polling-state slot.
    // This is the single funnel for every announced suppression (~7 call
    // sites), so all presence/rule/quiet/calendar reasons are covered without
    // touching each site. Reason token only — never posted text.
    super::state::record_gate_reason(session, Some(reason.to_string()));
    // Issue #877: append to the bounded decision history. The gate
    // reason is the documented wire shape; the track fingerprint is
    // pulled off the session mirror so an
    // Activity card entry can pin the gate to the track it targeted.
    let track_fingerprint = super::state::current_track_fingerprint(session);
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
#[allow(clippy::too_many_arguments)]
pub(crate) fn arm_presence_session(
    session: &super::state::SessionState,
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
            super::state::record_armed_presence(
                session,
                Some((&pair.availability, &pair.activity, label)),
            );
            let _ = app.emit(
                "presence-availability-updated",
                crate::events::PresenceAvailabilityUpdated {
                    available: true,
                    label: label.to_string(),
                    timestamp: Utc::now().to_rfc3339(),
                },
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
            super::timing::rate_limit_sleep_secs(&e)
        }
    }
}

/// Clear the app's presence session (issues #3.0-P1/#636): `clearPresence`
/// 404 = the session is already gone = success. Returns extra backoff seconds
/// (0 unless Graph throttled the call). No-op when nothing of ours is armed.
pub(crate) fn clear_presence_session(
    session: &super::state::SessionState,
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
            super::state::record_armed_presence(session, None);
            let _ = app.emit(
                "presence-availability-updated",
                crate::events::PresenceAvailabilityUpdated {
                    available: false,
                    label: label.to_string(),
                    timestamp: Utc::now().to_rfc3339(),
                },
            );
            0
        }
        Err(e) => {
            log::error!("[POLLING] failed to clear Teams availability: {}", e);
            super::timing::rate_limit_sleep_secs(&e)
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

/// Issue #758: the preferred-presence slot lives on the session, owned by
/// `AppState`. The arm/clear/expiry helpers below take it explicitly so two
/// sessions never share an arm.
pub(crate) fn load_preferred_presence_session(
    session: &super::state::SessionState,
) -> Option<PreferredPresenceSession> {
    session
        .preferred_presence_slot()
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
}
pub(crate) fn record_preferred_presence_session(
    session: &super::state::SessionState,
    value: Option<PreferredPresenceSession>,
) {
    if let Ok(mut guard) = session.preferred_presence_slot().lock() {
        *guard = value;
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
    session: &super::state::SessionState,
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
    let last = load_preferred_presence_session(session);
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
            record_preferred_presence_session(
                session,
                Some(PreferredPresenceSession {
                    pair: pair.clone(),
                    expires_at,
                    label: label_static,
                }),
            );
            let _ = app.emit(
                "preferred-presence-updated",
                crate::events::PreferredPresenceUpdated {
                    available: true,
                    label: label.to_string(),
                    availability: Some(pair.availability.clone()),
                    activity: Some(pair.activity.clone()),
                    expires_at: Some(expires_at.to_rfc3339()),
                    timestamp: now.to_rfc3339(),
                },
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
                    track_fingerprint: super::state::current_track_fingerprint(session),
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
            super::timing::rate_limit_sleep_secs(&e)
        }
    }
}

/// Issue #866: clear the app's preferred-presence session. Mirrors the
/// 404-as-success contract [`clear_presence_session`] uses — Graph answers
/// 404 when no preferred presence is set, which IS the success case. No-op
/// when nothing of ours is armed.
pub(crate) fn clear_preferred_presence_session(
    session: &super::state::SessionState,
    app: &AppHandle,
    access_token: &str,
    label: &str,
) -> u64 {
    if load_preferred_presence_session(session).is_none() {
        return 0;
    }
    match clear_user_preferred_presence(access_token) {
        Ok(_) => {
            record_preferred_presence_session(session, None);
            let _ = app.emit(
                "preferred-presence-updated",
                crate::events::PreferredPresenceUpdated {
                    available: false,
                    label: label.to_string(),
                    availability: None,
                    activity: None,
                    expires_at: None,
                    timestamp: Utc::now().to_rfc3339(),
                },
            );
            // Issue #877: append a "preferred-presence-cleared" entry so
            // the Activity card can pair each armed entry with its clear.
            crate::history::append(
                crate::history::PresenceHistoryEntry {
                    at: Utc::now(),
                    kind: "preferred-presence-cleared".to_string(),
                    note: format!("cleared ({label})"),
                    track_fingerprint: super::state::current_track_fingerprint(session),
                    posted_status: None,
                    gate_reason: None,
                },
                None,
            );
            0
        }
        Err(e) => {
            log::error!("[POLLING] failed to clear Teams preferred presence: {}", e);
            super::timing::rate_limit_sleep_secs(&e)
        }
    }
}

/// Issue #866: the per-iteration expiry tick. Runs at the head of the poll
/// loop, BEFORE the rule/snooze paths can re-arm — so an expired window
/// always clears before a re-arm can resurrect it.
pub(crate) fn clear_expired_preferred_presence(
    session: &super::state::SessionState,
    app: &AppHandle,
    access_token: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> u64 {
    let Some(armed) = load_preferred_presence_session(session) else {
        return 0;
    };
    if armed.expires_at > now {
        return 0;
    }
    log::info!(
        "[POLLING] preferred presence expired (label={}), clearing",
        armed.label
    );
    clear_preferred_presence_session(session, app, access_token, "Preferred presence expired")
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
pub(crate) fn rule_presence_backoff(
    session: &super::state::SessionState,
    app: &AppHandle,
    access_token: &str,
    config: &Option<std::sync::Arc<AppConfig>>,
    decision: &super::rules::RuleDecision,
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
            session,
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
        session,
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
pub(crate) fn availability_sync_enabled(config: &Option<std::sync::Arc<AppConfig>>) -> bool {
    config
        .as_ref()
        .map(|c| c.teams.availability_sync)
        .unwrap_or(false)
}

/// Issue #790: the default session armed while a track plays and no rule
/// overrides it — `Available`/`Available` is "Listening" in the Graph
/// vocabulary.
pub(crate) fn default_listening_presence() -> PresencePair {
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
pub(crate) fn sync_availability(
    session: &super::state::SessionState,
    app: &AppHandle,
    access_token: &str,
    track_is_playing: bool,
    remaining_ms: Option<u64>,
    rule: &super::rules::RuleDecision,
    config: &Option<std::sync::Arc<AppConfig>>,
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
            session,
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
            session,
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
            session,
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
        session,
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
pub(crate) fn rearm_after_304(tracked: bool, availability_sync: bool, gate_blocked: bool) -> bool {
    tracked && availability_sync && !gate_blocked
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
pub(crate) fn rearm_availability_after_304(
    app: &AppHandle,
    state: &Arc<AppState>,
    last_track_key: &Option<String>,
    last_poll_instant: Instant,
    config: &Option<std::sync::Arc<AppConfig>>,
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
    let Some(teams_tok) = super::write::teams_token_for_write(app, state) else {
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
        &state.session,
        app,
        &teams_tok.access_token,
        &pair,
        &presence_expiration_duration(remaining_ms),
        &label,
        armed_presence,
        last_availability_arm,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::polling::gate::ooo_gate_enabled;

    /// Production source with the test module stripped — the shared preamble
    /// for the structural guards below. Targets `write.rs`: the moved scanner
    /// guards pin call sites inside `process_track`/`handle_no_track`.
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
        let body = prod_fn_body(prod_source(), "pub(crate) fn process_track(");

        let skip_pos = body
            .find("if should_skip_identical_write(")
            .expect("process_track must keep the #384 identical-write guard");
        let after_guard = &body[skip_pos..];
        let skip_return = after_guard
            .find("playing_track_sleep(")
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
        let cfg = Some(std::sync::Arc::new(crate::config::AppConfig::default()));
        assert!(!ooo_gate_enabled(&cfg, true));
        let mut on = crate::config::AppConfig::default();
        on.teams.gate_when_out_of_office = true;
        assert!(ooo_gate_enabled(
            &Some(std::sync::Arc::new(on.clone())),
            false
        ));
        assert!(!ooo_gate_enabled(&Some(std::sync::Arc::new(on)), true));

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

    /// Review round 3, item 3: "no path may observe a manual status without
    /// recording it". The due mid-track re-check calls `presence_gate_decision`
    /// directly (it does not go through `gate_verdict`), so removing its
    /// `observe_presence_sample` call left a gated track + a user-typed status
    /// with a stale exit snapshot — and quitting then replaced the user's own
    /// Teams message with our placeholder.
    #[test]
    fn test_every_presence_read_records_the_manual_status_verdict() {
        let prod = prod_source();
        let own = include_str!("presence.rs")
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("presence.rs has no #[cfg(test)] mod tests block");
        assert!(
            own.contains("fn observe_presence_sample(")
                || prod.contains("observe_presence_sample("),
            "the recording path must exist (review round 3, item 3)"
        );
        // The reads that route through `gate_verdict` are covered by its own
        // call; assert that too, so a future edit cannot drop it quietly.
        let verdict_start = prod
            .find("let gate_verdict = |presence")
            .expect("process_track must keep the shared gate closure");
        let verdict = &prod[verdict_start..(verdict_start + 900).min(prod.len())];
        assert!(
            verdict.contains("observe_presence_sample("),
            "the shared gate closure must record the verdict for its reads"
        );
        // The direct read (due mid-track re-check) must record before deciding.
        let direct = prod
            .find("presence_gate_decision(")
            .expect("the due mid-track re-check must still read presence (the #430 late-post)");
        let window = &prod[direct.saturating_sub(1500)..direct];
        assert!(
            window.contains("observe_presence_sample("),
            "the direct presence read must record the manual-status verdict before deciding \
             (review round 3, item 3) — route it through `gate_verdict`, or call \
             `observe_presence_sample` at the read"
        );
        // Issue #792: the UNGATED mid-track re-check — an ungated track must
        // re-read presence through `gate_verdict` (which records the verdict)
        // before the keepalive can re-POST. Sliced from the gated-track tail
        // to the keepalive so the bounds stay inside the playing branch:
        // pre-fix the slice holds no read and every assert below fails.
        let track_body = prod_fn_body(prod, "pub(crate) fn process_track(");
        let gated_tail = track_body
            .find("track presence-gated, skipping status write")
            .expect("the gated-track early return must exist");
        let after = &track_body[gated_tail..];
        let keepalive = after
            .find("should_skip_identical_write(")
            .expect("the #384 keepalive must still follow the gate");
        let ungated = &after[..keepalive];
        for site in [
            "get_teams_presence(",
            "gate_verdict(",
            "gate_recheck_due(",
            "*gated_track_key = Some",
            "emit_presence_gated",
            "gate engaged mid-track",
        ] {
            assert!(
                ungated.contains(site),
                "issue #792: the ungated mid-track re-check must contain `{site}` before the \
                 keepalive can re-POST — a meeting joined mid-track must suppress within the \
                 `last_gate_check` window"
            );
        }
    }
}
