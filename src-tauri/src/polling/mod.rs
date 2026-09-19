//! Polling subsystem — split into four files:
//!
//! - [`loop`]      — the polling driver (`polling_loop`).
//! - [`poll_once`] — the single source of truth for one poll iteration
//!   (CAS refresh, 401-retry, no-track handling, error emission). The
//!   3-branch drift that motivated #72 collapses to one path here.
//! - [`state`]     — thread-lifecycle glue (`start_polling`,
//!   `stop_polling`).
//!
//! `token_io` was historically part of `polling.rs` per the #72 issue
//! body; that surface was already extracted to the top-level
//! `crate::token_io` module in a prior PR (see issue #65), so no
//! `polling/token_io.rs` file is created here.
//!
//! `ErrorSeverity` + `emit_error` live in this file (issue #117 / #79)
//! because they are the single canonical shape for the `error` event
//! and must be reachable from every submodule.

// `loop` is a Rust keyword so the module identifier is `loop_`; the file is
// still named `loop.rs` per the #72 issue spec via the `#[path]` attribute.
#[path = "loop.rs"]
mod loop_;
mod poll_once;
mod state;

pub(crate) use poll_once::{
    cas_refresh_or_discard, clear_presence_on_exit, load_write_clocks, run_oneshot, CasOutcome,
};
pub use state::{start_polling, stop_polling};

use tauri::{AppHandle, Emitter};

/// Severity tier for `error` events emitted to the frontend.
///
/// Used by `Dashboard.svelte` and other listeners to decide between a
/// transient toast (warning) and a persistent banner (error). See
/// issue #79. A transient error that the polling loop will retry
/// (e.g. a 401 that triggers token refresh, a 429 that triggers
/// back-off) is `warning`; an error that ended the current attempt
/// with no automatic recovery is `error`.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ErrorSeverity {
    Warning,
    Error,
}

impl ErrorSeverity {
    /// The wire spelling of this tier. `Dashboard.svelte` gates its red banner
    /// on the literal `"error"`, so the two strings are a frontend contract,
    /// not an internal detail (see issue #79 and `ErrorEventPayload` in
    /// `src/lib/types.ts`).
    fn as_str(self) -> &'static str {
        match self {
            ErrorSeverity::Warning => "warning",
            ErrorSeverity::Error => "error",
        }
    }
}

/// The `error` event payload, as the frontend receives it:
/// `{ "source": <string>, "message": <string>, "severity": "warning" | "error" }`.
///
/// Pure so the shape is asserted by a test that runs it (issue #761):
/// centralised here so the field names cannot drift between emit sites
/// (polling.rs had 3 of them, see issue #79). All call sites in `poll_once`
/// route through [`emit_error`].
fn error_payload(source: &str, message: String, severity: ErrorSeverity) -> serde_json::Value {
    serde_json::json!({
        "source": source,
        "message": message,
        "severity": severity.as_str(),
    })
}

/// Emit an `error` event to the frontend in the [`error_payload`] shape.
///
/// Needs a live `AppHandle`, so the payload itself is built by the pure
/// helper above and asserted there; this wrapper only hands it to the emitter.
pub(crate) fn emit_error(app: &AppHandle, source: &str, message: String, severity: ErrorSeverity) {
    let _ = app.emit("error", error_payload(source, message, severity));
}

#[cfg(test)]
mod tests {
    use super::{error_payload, ErrorSeverity};
    use serde_json::json;

    /// Issue #761: the `error` envelope is the app's single canonical error
    /// shape, and its field names are what every listener reads — `source`
    /// (which subsystem), `message` (what happened) and `severity` (whether
    /// the Dashboard pops the red banner or only logs it). A rename drops a
    /// field to `undefined` in the listener with no type error on the Rust
    /// side, so the three names are pinned here by running the builder.
    #[test]
    fn error_payload_carries_source_message_and_severity() {
        let payload = error_payload(
            "poll_once",
            "token refresh failed".to_string(),
            ErrorSeverity::Error,
        );

        assert_eq!(
            payload,
            json!({
                "source": "poll_once",
                "message": "token refresh failed",
                "severity": "error",
            }),
            "the error envelope is the frontend's contract"
        );
    }

    /// Both tiers have to keep their exact spelling: `Dashboard.svelte` treats
    /// `"error"` as the persistent-banner case and anything else as log-only,
    /// so a reworded literal would silently downgrade every hard failure.
    #[test]
    fn error_severity_spells_both_tiers_for_the_frontend() {
        assert_eq!(ErrorSeverity::Warning.as_str(), "warning");
        assert_eq!(ErrorSeverity::Error.as_str(), "error");
    }
}
