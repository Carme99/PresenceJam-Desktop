//! Exit-time Teams cleanup (issue #754).
//!
//! Split from `polling::poll_once`: the shutdown presence/status cleanup and
//! its pure plan helper.

use std::sync::Arc;

use tauri::AppHandle;

use crate::teams::{
    clear_teams_presence_quick, clear_teams_status_message_quick,
    clear_user_preferred_presence_quick, is_token_expired as is_teams_token_expired,
};
use crate::AppState;

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
    let config = state.config.snapshot();
    let snapshot = super::state::load_exit_snapshot(&state.session);
    let plan = exit_cleanup_plan(
        &snapshot,
        super::presence::availability_sync_enabled(&config),
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
    if super::presence::load_preferred_presence_session(&state.session).is_some() {
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
        let placeholder = super::status_text::paused_status_placeholder(&config);
        match clear_teams_status_message_quick(
            &tokens.access_token,
            &placeholder,
            Some(&super::timing::placeholder_expiry_str()),
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
        super::state::reset_exit_snapshot(&state.session);
    }
}

/// Finding D1 (issue #684): what the exit cleanup should attempt, derived from
/// the [`super::state::ExitSnapshot`] and the two config flags. Pure so the
/// "the reset clocks must not cancel the cleanup" guarantee is unit-testable —
/// the pre-fix code decided this inline from `clocks.last_availability_arm` /
/// `clocks.last_posted_status`, which the loop's exit tail has already emptied.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ExitCleanupPlan {
    pub(crate) clear_presence: bool,
    pub(crate) post_placeholder: bool,
}

pub(crate) fn exit_cleanup_plan(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::polling::clocks::{load_write_clocks, reset_write_clocks};

    /// Finding D1 (issue #684): `polling_loop`'s exit tail empties the
    /// write-decision clocks BEFORE `RunEvent::Exit` runs the cleanup, so the
    /// cleanup must decide from the exit snapshot. Pre-fix this test's
    /// assertions about the snapshot did not exist and the cleanup read the
    /// (already cold) clocks — a quit mid-song left the music status and the
    /// armed `Available` session live.
    #[test]
    fn test_exit_cleanup_survives_the_loop_exit_tail() {
        let session = crate::polling::SessionState::new();
        crate::polling::state::reset_exit_snapshot(&session);
        // A session that posted a playing status AND armed a listening session.
        crate::polling::state::record_posted_status(&session, Some("\u{1F3B5} A - T \u{1F3A7}"));
        crate::polling::state::record_armed_presence(
            &session,
            Some(("Available", "Available", "Listening (Available)")),
        );

        // The loop's exit tail (what runs before `RunEvent::Exit`).
        reset_write_clocks(&session);

        let snapshot = crate::polling::state::load_exit_snapshot(&session);
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
        let cold = load_write_clocks(&session);
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
        reset_write_clocks(&session);
        assert_eq!(
            crate::polling::state::load_exit_snapshot(&session),
            snapshot,
            "a session boundary must not forget the residue a previous session \
             left on Teams (finding D1)"
        );

        // Only a completed exit cleanup retires it.
        crate::polling::state::reset_exit_snapshot(&session);
        assert_eq!(
            exit_cleanup_plan(
                &crate::polling::state::load_exit_snapshot(&session),
                true,
                true
            ),
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
            body.contains("load_exit_snapshot(&state.session)"),
            "the exit cleanup must read the exit snapshot (finding D1)"
        );
        assert!(
            !body.contains("load_write_clocks"),
            "the exit cleanup must NOT read the clocks: polling_loop's exit tail \
             resets them before RunEvent::Exit runs this cleanup (finding D1)"
        );
        assert!(
            !body.contains("clocks.last_availability_arm"),
            "the pre-fix early-return on default clocks is the D1 defect"
        );
        let loop_source = include_str!("loop.rs");
        assert!(
            loop_source.contains("reset_write_clocks(&state.session)"),
            "polling_loop must still reset the shared clocks when the session ends"
        );
        assert!(
            !loop_source.contains("reset_exit_snapshot("),
            "the loop's exit tail must not reset the exit snapshot — that would \
             resurrect finding D1"
        );
        let state_source = include_str!("state.rs");
        let start_body = prod_fn_body(state_source, "pub fn start_polling(");
        assert!(
            !start_body.contains("reset_exit_snapshot("),
            "a session START must not clear the snapshot: Teams keeps showing the \
             previous session's status, so a stop→start→quit would skip the \
             cleanup (finding D1)"
        );
        assert!(
            body.contains("reset_exit_snapshot(&state.session);"),
            "a completed exit cleanup must retire the snapshot so a repeated \
             RunEvent::Exit is a no-op"
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
        // Post-split (#754): process_track lives in write.rs -- scan its
        // stripped production (test module excluded, as prod_source does).
        let write_prod = stripped(include_str!("write.rs"));
        let track_body = prod_fn_body(write_prod, "pub(crate) fn process_track(");
        assert!(
            track_body.contains("observe_presence_sample("),
            "every gate read must record the manual-status verdict for the exit path \
             (review rounds 2 item 7 / 3 item 3) — the exit path has no Graph sample of \
             its own, so a read that does not record leaves the user's own Teams status \
             exposed on quit"
        );
    }

    /// Production source with the test module stripped -- the shared preamble
    /// for the structural guards below.
    fn prod_source() -> &'static str {
        include_str!("exit.rs")
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("exit.rs has no #[cfg(test)] mod tests block")
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

    /// Production part of a sibling module's source: everything above its
    /// test module (the same strip [`prod_source`] applies to this file).
    fn stripped(source: &str) -> &str {
        source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("sibling module source has no test module")
    }
}
