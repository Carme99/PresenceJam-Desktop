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

/// Emit an `error` event to the frontend with a stable shape:
/// `{ "source": <string>, "message": <string>, "severity": "warning" | "error" }`.
///
/// Centralised so the field shape cannot drift between emit sites
/// (polling.rs had 3 of them, see issue #79). All call sites in
/// `poll_once` route through this helper.
pub(crate) fn emit_error(app: &AppHandle, source: &str, message: String, severity: ErrorSeverity) {
    let severity_str = match severity {
        ErrorSeverity::Warning => "warning",
        ErrorSeverity::Error => "error",
    };
    let _ = app.emit(
        "error",
        serde_json::json!({
            "source": source,
            "message": message,
            "severity": severity_str,
        }),
    );
}
