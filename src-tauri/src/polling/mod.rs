//! Polling subsystem — split into focused modules (issue #754):
//!
//! - [`loop_`]      — the polling driver (`polling_loop`).
//! - [`iteration`]  — the single source of truth for one poll iteration
//!   (CAS refresh, 401-retry, no-track handling, error emission). The
//!   3-branch drift that motivated #72 collapses to one path here.
//! - [`clocks`]     — the shared write-decision clocks (`WriteClocks`,
//!   load/store/reset).
//! - [`timing`]     — backoff, jitter, debounce and interval helpers.
//! - [`refresh`]    — the Spotify/Teams CAS refresh path.
//! - [`gate`]       — quiet-hours, snooze and gate-recheck predicates.
//! - [`rules`]      — the track-rule engine (`RuleDecision`, walker).
//! - [`presence`]   — the Teams presence session.
//! - [`status_text`]— the status-text builder.
//! - [`write`]      — the Teams write path (`process_track`,
//!   `handle_no_track`, debounce/keepalive decisions).
//! - [`exit`]       — the loop-exit cleanup tail.
//! - [`state`]      — thread-lifecycle glue (`start_polling`,
//!   `stop_polling`).
//! - [`daemon`]     — supervised `--daemon` mode (issue #896): SIGTERM/
//!   SIGINT handlers, bounded join, clean shutdown.
//!
//! `token_io` was historically part of `polling/loop.rs` per the #72 issue
//! body; that surface was already extracted to the top-level
//! `crate::token_io` module in a prior PR (see issue #65), so no
//! `polling/token_io.rs` file is created here.
//!
//! `ErrorSeverity`, `ErrorRecovery`, `ErrorEventPayload`, and the error
//! emitters live in this file (issue #117 / #79) because they are the single
//! canonical contract for the `error` event and must be reachable from every
//! submodule.

// `loop` is a Rust keyword so the module identifier is `loop_`; the file is
// still named `loop.rs` per the #72 issue spec via the `#[path]` attribute.
pub(crate) mod clocks;
mod daemon;
pub(crate) mod exit;
pub(crate) mod gate;
pub(crate) mod iteration;
#[path = "loop.rs"]
mod loop_;
pub(crate) mod presence;
pub(crate) mod refresh;
pub(crate) mod rules;
pub(crate) mod state;
pub(crate) mod status_text;
pub(crate) mod timing;
pub(crate) mod write;

// Issue #754: `poll_once.rs` is deleted; its public surface is re-exported
// from the focused modules so existing call sites do not move. Only the
// names used outside `polling/` are re-exported here — sibling modules
// reach each other through `super::<module>::` paths directly.
pub(crate) use clocks::load_write_clocks;
pub(crate) use exit::clear_presence_on_exit;
pub(crate) use iteration::run_oneshot;
pub(crate) use refresh::{cas_refresh_spotify, cas_refresh_teams, CasOutcome};
pub(crate) use status_text::MUSIC_EMOJI;

// Issue #868: the rule walker (TrackRuleContext, track_rule_hit,
// track_rule_conditions_match, track_rule_schedule_matches) is the
// dry-run tester's source of truth — `commands::rules::explain_rules`
// runs it against a Settings-typed synthetic track, so the same
// walker the live `process_track` path uses feeds the IPC boundary
// too. `matching_track_rule_at_with_ctx` is the public-in-this-crate
// entry point the live path calls; the rule walker pieces are
// re-exported for the IPC path to compose the same evaluation.
pub(crate) use rules::{
    track_rule_conditions_match, track_rule_hit, track_rule_schedule_matches, TrackRuleContext,
};
#[cfg(test)]
pub(crate) use state::record_manual_status_blocks;
pub(crate) use state::SessionState;
pub(crate) use state::{load_exit_snapshot, load_failure_counters, load_gate_reason};
#[cfg(test)]
pub(crate) use state::{record_failure_counters, record_gate_reason, reset_sync_state};
pub use state::{start_polling, stop_polling};
// Issue #896: the supervised `--daemon` mode is a public surface —
// `lib::run` calls `polling::daemon::run` from the setup hook, and
// integration tests (when they land) will exercise it directly.
pub use daemon::run as run_daemon;

use tauri::{AppHandle, Emitter};

/// Typed `error`-event surface (issue #762). The canonical payload and its
/// enums live in [`crate::events`] so ts-rs can export them; the names are
/// re-exported here so every existing `polling::ErrorX` path keeps working.
/// `ErrorEventPayload` stays as the alias the emitters and tests already use.
pub(crate) use crate::events::ErrorEvent as ErrorEventPayload;
pub(crate) use crate::events::{ErrorRecovery, ErrorSeverity};

/// Event sink used by the canonical error emitters. Production delegates to
/// Tauri's emitter; tests capture the same production payload without a GUI
/// runtime, and without linking Tauri's `test` module (which breaks the
/// Windows test binary's load — issue #929 rework).
pub(crate) trait ErrorEventEmitter {
    fn emit_error_event(&self, event: &str, payload: ErrorEventPayload);
    /// Emit a marker event with a `null` payload — the shape the reconnect
    /// prompts use. Split from [`Self::emit_error_event`] because those
    /// carry a structured `ErrorEventPayload`, and a single trait cannot
    /// express both without erasing the distinction the tests assert on.
    fn emit_marker(&self, event: &str);
}

impl<R: tauri::Runtime> ErrorEventEmitter for AppHandle<R> {
    fn emit_error_event(&self, event: &str, payload: ErrorEventPayload) {
        let _ = self.emit(event, payload);
    }

    fn emit_marker(&self, event: &str) {
        let _ = self.emit(event, serde_json::json!(null));
    }
}

/// Emit an `error` event without provider-specific recovery metadata.
pub(crate) fn emit_error<E: ErrorEventEmitter>(
    emitter: &E,
    source: &str,
    message: String,
    severity: ErrorSeverity,
) {
    emit_error_with_recovery(emitter, source, message, severity, None);
}

/// Emit an `error` event with the canonical payload and optional recovery.
pub(crate) fn emit_error_with_recovery<E: ErrorEventEmitter>(
    emitter: &E,
    source: &str,
    message: String,
    severity: ErrorSeverity,
    recovery: Option<ErrorRecovery>,
) {
    emitter.emit_error_event(
        "error",
        ErrorEventPayload::new(source, message, severity, recovery),
    );
}

#[cfg(test)]
mod tests {
    use super::{emit_error, ErrorEventEmitter, ErrorEventPayload, ErrorSeverity};

    #[derive(Default)]
    struct RecordingErrorEmitter {
        events: parking_lot::Mutex<Vec<(String, ErrorEventPayload)>>,
    }

    impl ErrorEventEmitter for RecordingErrorEmitter {
        fn emit_error_event(&self, event: &str, payload: ErrorEventPayload) {
            self.events.lock().push((event.to_string(), payload));
        }
        fn emit_marker(&self, event: &str) {
            self.events.lock().push((
                event.to_string(),
                ErrorEventPayload::new("test", String::new(), ErrorSeverity::Warning, None),
            ));
        }
    }

    #[test]
    fn emit_error_builds_the_canonical_lowercase_severity_payloads() {
        let recorder = RecordingErrorEmitter::default();

        emit_error(
            &recorder,
            "spotify",
            "Retrying after a transient failure".to_string(),
            ErrorSeverity::Warning,
        );
        emit_error(
            &recorder,
            "teams",
            "Reconnect Teams in Settings".to_string(),
            ErrorSeverity::Error,
        );

        let events = recorder.events.lock();
        let serialized = events
            .iter()
            .map(|(event, payload)| {
                (
                    event.as_str(),
                    serde_json::to_value(payload).expect("serialize canonical error payload"),
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
                        "severity": "error"
                    })
                ),
            ]
        );
    }
}
