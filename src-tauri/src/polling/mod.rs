//! Polling subsystem — split into five files:
//!
//! - [`loop`]      — the polling driver (`polling_loop`).
//! - [`poll_once`] — the single source of truth for one poll iteration
//!   (CAS refresh, 401-retry, no-track handling, error emission). The
//!   3-branch drift that motivated #72 collapses to one path here.
//! - [`state`]     — thread-lifecycle glue (`start_polling`,
//!   `stop_polling`).
//! - [`daemon`]    — supervised `--daemon` mode (issue #896): SIGTERM/
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
mod daemon;
#[path = "loop.rs"]
mod loop_;
mod poll_once;
mod state;

pub(crate) use poll_once::MUSIC_EMOJI;
pub(crate) use poll_once::{
    cas_refresh_or_discard, clear_presence_on_exit, load_write_clocks, run_oneshot, CasOutcome,
};
// Issue #868: the rule walker (TrackRuleContext, track_rule_hit,
// track_rule_conditions_match, track_rule_schedule_matches) is the
// dry-run tester's source of truth — `commands::rules::explain_rules`
// runs it against a Settings-typed synthetic track, so the same
// walker the live `process_track` path uses feeds the IPC boundary
// too. `matching_track_rule_at_with_ctx` is the public-in-this-crate
// entry point the live path calls; the rule walker pieces are
// re-exported for the IPC path to compose the same evaluation.
pub(crate) use poll_once::{
    track_rule_conditions_match, track_rule_hit, track_rule_schedule_matches, TrackRuleContext,
};
#[cfg(test)]
pub(crate) use state::{global_state_lock, record_manual_status_blocks, reset_exit_snapshot};
pub(crate) use state::{load_exit_snapshot, load_failure_counters, load_gate_reason};
#[cfg(test)]
pub(crate) use state::{record_failure_counters, record_gate_reason, reset_sync_state};
pub use state::{start_polling, stop_polling};
// Issue #896: the supervised `--daemon` mode is a public surface —
// `lib::run` calls `polling::daemon::run` from the setup hook, and
// integration tests (when they land) will exercise it directly.
pub use daemon::run as run_daemon;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Severity tier for `error` events emitted to the frontend.
///
/// Used by `Dashboard.svelte` and other listeners to decide between a
/// transient toast (warning) and a persistent banner (error). See
/// issue #79. A transient error that the polling loop will retry
/// (e.g. a 401 that triggers token refresh, a 429 that triggers
/// back-off) is `warning`; an error that ended the current attempt
/// with no automatic recovery is `error`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ErrorSeverity {
    Warning,
    Error,
}

/// Recovery action advertised by an error event, when the producer has one.
///
/// This is optional because the polling loop's ordinary retry warnings do
/// not carry a provider-specific action. Teams uses it to distinguish a
/// scheduled automatic retry from failures that require user action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ErrorRecovery {
    RetryScheduled,
    ReconnectRequired,
    UserActionRequired,
}

/// Canonical payload for the frontend `error` event.
///
/// All error-event producers go through [`emit_error`], so the wire shape and
/// optional recovery discriminator cannot drift between providers.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ErrorEventPayload {
    pub(crate) source: String,
    pub(crate) message: String,
    pub(crate) severity: ErrorSeverity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) recovery: Option<ErrorRecovery>,
}

/// Emit an `error` event without provider-specific recovery metadata.
pub(crate) fn emit_error(
    app: &AppHandle,
    source: &str,
    message: String,
    severity: ErrorSeverity,
) {
    emit_error_with_recovery(app, source, message, severity, None);
}

/// Emit an `error` event with the canonical payload and optional recovery.
pub(crate) fn emit_error_with_recovery(
    app: &AppHandle,
    source: &str,
    message: String,
    severity: ErrorSeverity,
    recovery: Option<ErrorRecovery>,
) {
    let _ = app.emit(
        "error",
        ErrorEventPayload {
            source: source.to_string(),
            message,
            severity,
            recovery,
        },
    );
}
