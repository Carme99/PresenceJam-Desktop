//! Write-decision clocks shared between the polling loop and one-shot refreshes (issue #754).

use std::time::Instant;

use crate::config::PresencePair;

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

/// Issue #754: the per-iteration poll state, bundled so `run`/`run_inner`
/// take one `&mut PollState` instead of 17 positional `&mut` out-params.
/// `clocks` holds everything the write path derives per iteration
/// (change detection, debounce/keepalive/gate clocks, presence arm, resume flags);
/// the remaining fields are the driver's own counters and handles, whose
/// lifetime spans iterations but which never touch the shared slot.
pub(crate) struct PollState {
    pub(crate) clocks: WriteClocks,
    pub(crate) consecutive_pauses: u8,
    pub(crate) transient_failure_count: u8,
    pub(crate) consecutive_network_failures: u8,
    pub(crate) playback_source: Box<dyn crate::sources::PlaybackSource>,
    pub(crate) last_source_kind: crate::sources::PlaybackSourceKind,
    pub(crate) first_iteration: bool,
}

/// Snapshot the session's write-decision clocks. A poisoned lock is recovered
/// rather than propagated (`into_inner`): these are dedup heuristics, and
/// losing them costs at most one redundant Graph write.
///
/// The snapshot carries the generation it read, so the matching
/// [`store_write_clocks`] can tell whether it is still the slot's latest view
/// (finding D11, issue #694).
pub(crate) fn load_write_clocks(session: &super::state::SessionState) -> WriteClocks {
    session
        .write_clocks_slot()
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
pub(crate) fn store_write_clocks(session: &super::state::SessionState, clocks: &WriteClocks) {
    let mut slot = session
        .write_clocks_slot()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
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
/// Forget the session's write-decision clocks. Called when a polling session
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
pub(crate) fn reset_write_clocks(session: &super::state::SessionState) {
    let mut slot = session
        .write_clocks_slot()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let next_generation = slot.generation.wrapping_add(1);
    *slot = WriteClocks::default();
    slot.generation = next_generation;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::polling::presence::should_rearm_availability;
    use crate::polling::write::should_skip_identical_write;

    /// Finding PollCore#4 (issue #572): the manual refresh reads the session's
    /// write clocks. Pre-fix its fresh `last_availability_arm = None` re-armed
    /// the availability session on every refresh, and its fresh
    /// `last_track_key`/`last_posted_status`/`last_teams_update` bypassed the
    /// #384 identical-write guard.
    #[test]
    fn test_shared_write_clocks_prevent_one_shot_rearm_and_duplicate_write() {
        let session = crate::polling::SessionState::new();
        let now = Instant::now();
        // Finding D11 (issue #694): the shared slot is generation-checked, so a
        // test snapshot must be the one `load_write_clocks` just handed out —
        // building a `WriteClocks::default()` (generation 0) and storing it
        // would now be discarded as superseded.
        let mut armed = load_write_clocks(&session);
        armed.last_availability_arm = Some(now);
        store_write_clocks(&session, &armed);
        let loaded = load_write_clocks(&session);
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

        reset_write_clocks(&session);
        let cold = load_write_clocks(&session);
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

    /// Finding PollCore#4 (issue #572): a polling session resets the shared
    /// clocks on start and on exit, so a new session (or a refresh issued
    /// while nothing runs) never inherits a dead session's clocks.
    /// Why a source scan survives here (issue #778 allows exactly this
    /// shape): the invariant is the `reset_write_clocks(&state.session)` call in both the loop exit and the session start; the loop needs a live `AppHandle`, so the call sites are pinned at the source.
    #[test]
    fn test_polling_lifecycle_resets_shared_write_clocks() {
        let loop_source = include_str!("loop.rs");
        assert!(
            loop_source.contains("reset_write_clocks(&state.session)"),
            "polling_loop must reset the shared clocks when the session ends"
        );
        let state_source = include_str!("state.rs");
        assert!(
            state_source.contains("reset_write_clocks(&state.session)"),
            "start_polling must reset the shared clocks so a panic-dead session's \
             clocks cannot leak into the next one"
        );
    }

    /// Finding D11 (issue #694): a snapshot whose generation was superseded
    /// must not land — that is the write-back that silently dropped
    /// `gated_track_key` and let a write through mid-meeting.
    #[test]
    fn test_superseded_clock_snapshot_is_discarded() {
        let session = crate::polling::SessionState::new();
        reset_write_clocks(&session);

        // The loop's iteration: loads, records the gate it observed, stores.
        let mut iteration = load_write_clocks(&session);
        iteration.gated_track_key = Some("A - T | filter=true".to_string());
        store_write_clocks(&session, &iteration);
        assert_eq!(
            load_write_clocks(&session).gated_track_key.as_deref(),
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
        store_write_clocks(&session, &stale);

        let landed = load_write_clocks(&session);

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
        reset_write_clocks(&session);
        store_write_clocks(&session, &dead);
        assert_eq!(
            load_write_clocks(&session).generation,
            dead.generation.wrapping_add(1),
            "the reset's generation must not be overwritten by a pre-reset snapshot"
        );
    }
}
