//! OS-level probes the presence gate folds into its decision.
//!
//! One probe lives here:
//!
//! - [`focus`] — `SHQueryUserNotificationState` on Windows
//!   (`PresentationState::{None, FullScreen, Presentation, QuietTime,
//!   Unknown}`); issue #872. Linux / macOS report `Unknown`.
//!
//! Exposed behind a trait with a swappable test impl, so the decision
//! table (`poll_once::presence_gate_decision`) stays a pure function
//! under unit tests.

pub mod focus;
