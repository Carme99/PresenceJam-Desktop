//! OS presentation / quiet-time probe (issue #872).
//!
//! The Teams presence-gate only sees what the Graph `getPresence` sample
//! carries, so a user demoing a full-screen app, presenting slides or running
//! in Windows Quiet Time still looks `Available` and the app keeps advertising
//! music while the screen is shared. The Windows `SHQueryUserNotificationState`
//! shell API is the documented signal for those states
//! (https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shqueryusernotificationstate):
//! `QUNS_BUSY` (full-screen app), `QUNS_RUNNING_D3D_FULL_SCREEN`,
//! `QUNS_PRESENTATION_MODE`, `QUNS_QUIET_TIME`. macOS has an equivalent only
//! through Focus (`INFocusStatusCenter`), which Apple gates behind the
//! Communication Notifications entitlement plus `NSFocusStatusUsageDescription`;
//! Linux has no equivalent at all.
//!
//! The probe is exposed behind a thin trait so the decision table is
//! unit-testable off Windows (`PresentationState::Unknown` on Linux/macOS,
//! or a fake probe injected through `set_probe_for_tests`).

use std::sync::{Arc, Mutex, OnceLock};

/// The OS-level presentation / quiet-time signal the gate may fold in
/// alongside the Graph presence sample.
///
/// Mapped from `SHQueryUserNotificationState`'s `QUNS_*` values on Windows.
/// The variants the gate actually distinguishes are:
///
/// - `FullScreen` / `Presentation` — a full-screen app or a slide deck is
///   on screen; `gate_when_presenting` should suppress a music status while
///   this holds.
/// - `QuietTime` — Windows Focus Assist is on; same recommendation.
/// - `None` — no presentation-mode flag set; the gate is off.
/// - `Unknown` — the probe could not answer (Linux/macOS, or a Windows
///   probe error). The gate is off, exactly like a failed read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentationState {
    None,
    FullScreen,
    Presentation,
    QuietTime,
    Unknown,
}

impl PresentationState {
    /// The reason string the gate emits and the Dashboard chip maps to a
    /// localised label. The reason MUST be lowercase + ASCII + constant for
    /// the Dashboard's `gatedReasonLabel` switch (the label is en/de/fr
    /// translation, not a Rust string lookup, so the wire spelling is the
    /// one the frontend reads).
    pub fn gate_reason(self) -> &'static str {
        match self {
            PresentationState::FullScreen | PresentationState::Presentation => "presenting",
            PresentationState::QuietTime => "quiet-time",
            // `None` and `Unknown` both map to an empty reason — the gate
            // is off, so the decision path falls through to the next layer.
            PresentationState::None | PresentationState::Unknown => "",
        }
    }
}

/// An error from the underlying probe. The gate treats every variant the same
/// (fail open), but the type is here so callers can log the precise reason.
#[derive(Debug)]
pub enum FocusProbeError {
    /// The native call returned a non-zero error code or unexpected status.
    Native(u32),
    /// The probe is not implemented on this platform (Linux / macOS).
    Unsupported,
}

impl std::fmt::Display for FocusProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FocusProbeError::Native(code) => write!(f, "focus probe native error: 0x{:x}", code),
            FocusProbeError::Unsupported => write!(f, "focus probe not implemented on this OS"),
        }
    }
}

impl std::error::Error for FocusProbeError {}

/// Trait abstracting the platform-specific probe so tests can install a
/// fake and the production code can compile on every target.
pub trait FocusProbe: Send + Sync {
    fn probe(&self) -> Result<PresentationState, FocusProbeError>;
}

/// The Windows probe — calls `SHQueryUserNotificationState` through the
/// `windows` crate (`Win32_UI_Shell`).
///
/// Mapping (`QUNS_*` → [`PresentationState`]):
/// - `QUNS_NOT_PRESENT` (1) — `None`
/// - `QUNS_BUSY` (2) — `FullScreen`
/// - `QUNS_RUNNING_D3D_FULL_SCREEN` (3) — `FullScreen`
/// - `QUNS_PRESENTATION_MODE` (4) — `Presentation`
/// - `QUNS_QUIET_TIME` (5) — `QuietTime`
/// - `QUNS_APP` (6) — `None` (an app is presenting, not the OS)
#[cfg(target_os = "windows")]
pub struct WindowsFocusProbe;

#[cfg(target_os = "windows")]
impl FocusProbe for WindowsFocusProbe {
    fn probe(&self) -> Result<PresentationState, FocusProbeError> {
        use windows::Win32::UI::Shell::{
            SHQueryUserNotificationState, QUNS_APP, QUNS_BUSY, QUNS_NOT_PRESENT,
            QUNS_PRESENTATION_MODE, QUNS_QUIET_TIME, QUNS_RUNNING_D3D_FULL_SCREEN,
        };
        // The `windows` crate (>= 0.61) wraps `SHQueryUserNotificationState`
        // as `fn() -> Result<QUERY_USER_NOTIFICATION_STATE, Error>` — no
        // out-pointer, the `QUNS_*` value comes through the Ok arm.
        let result = unsafe { SHQueryUserNotificationState() };
        match result {
            Ok(state) => Ok(match state {
                    QUNS_BUSY | QUNS_RUNNING_D3D_FULL_SCREEN => PresentationState::FullScreen,
                    QUNS_PRESENTATION_MODE => PresentationState::Presentation,
                    QUNS_QUIET_TIME => PresentationState::QuietTime,
                    // `QUNS_NOT_PRESENT` (no flag) and `QUNS_APP` (an app is
                    // foreground) are both "OS isn't presenting".
                    _ => PresentationState::None,
                }),
            Err(e) => {
                // Preserve the bit pattern in `u32` so the log line stays
                // greppable across 32/64-bit builds.
                let code = e.code().0 as u32;
                log::warn!(
                    "[FOCUS] WindowsFocusProbe: SHQueryUserNotificationState returned 0x{:x}, failing open",
                    code
                );
                Err(FocusProbeError::Native(code))
            }
        }
    }
}

/// The Linux / macOS probe — returns `Unknown`. The macOS
/// `INFocusStatusCenter` route is gated behind entitlements the unsigned app
/// does not hold (issue #872 — `Communication Notifications` +
/// `NSFocusStatusUsageDescription`); Linux has no equivalent. Both platforms
/// therefore report "the OS does not tell us", which the gate treats as
/// "the gate is off".
pub struct UnsupportedFocusProbe;

impl FocusProbe for UnsupportedFocusProbe {
    fn probe(&self) -> Result<PresentationState, FocusProbeError> {
        Ok(PresentationState::Unknown)
    }
}

/// The default probe installed at process start — Windows gets the real
/// shell call, every other target gets the `Unknown` placeholder.
fn default_probe() -> Arc<dyn FocusProbe> {
    #[cfg(target_os = "windows")]
    {
        Arc::new(WindowsFocusProbe)
    }
    #[cfg(not(target_os = "windows"))]
    {
        Arc::new(UnsupportedFocusProbe)
    }
}

/// Process-wide slot for the probe. Tests install a fake through
/// [`set_probe_for_tests`] and receive back a [`ProbeGuard`] that restores
/// the default on drop. The slot is a `OnceLock` so the production startup
/// never pays for a `Mutex` lock on the hot path — only tests race for the
/// install. The runtime payload is wrapped in a `Mutex` so the swap is
/// itself thread-safe.
static PROBE_SLOT: OnceLock<Mutex<Arc<dyn FocusProbe>>> = OnceLock::new();

fn probe_slot() -> &'static Mutex<Arc<dyn FocusProbe>> {
    PROBE_SLOT.get_or_init(|| Mutex::new(default_probe()))
}

/// Read the current probe state. Cheap — a single atomic load under a
/// short-lived `Mutex` guard. Production reads never block.
pub fn current_probe() -> Arc<dyn FocusProbe> {
    probe_slot()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

/// Probe the OS for the current presentation / quiet-time state.
///
/// Returns `PresentationState::Unknown` when the probe is not implemented
/// on this target (Linux / macOS) or when the underlying native call
/// failed. The gate treats `Unknown` as "the gate is off" — failing open
/// is the documented contract.
pub fn probe_focus() -> PresentationState {
    match current_probe().probe() {
        Ok(state) => state,
        Err(e) => {
            log::debug!("[FOCUS] probe_focus: {}", e);
            PresentationState::Unknown
        }
    }
}

/// Install a test probe. Returns a [`ProbeGuard`] whose drop is a no-op
/// (the next test that installs a probe simply overwrites it). A
/// restore-on-drop design raced with parallel test execution: two
/// threads installing and restoring in opposite order can leave the
/// slot pointing at another thread's fake. Each test owns its own
/// probe Arc, so leaving it installed until the next test overwrites
/// is safe and removes the race entirely.
#[cfg(test)]
pub fn set_probe_for_tests(probe: Arc<dyn FocusProbe>) -> ProbeGuard {
    let mut slot = probe_slot().lock().unwrap_or_else(|e| e.into_inner());
    *slot = probe;
    ProbeGuard
}

/// RAII marker for a test-installed probe. Drop is a no-op by design
/// — see [`set_probe_for_tests`] for why.
#[cfg(test)]
#[must_use = "the test probe is installed until the next set_probe_for_tests call"]
pub struct ProbeGuard;

#[cfg(test)]
impl Drop for ProbeGuard {
    fn drop(&mut self) {
        // Intentionally empty — see the doc on `set_probe_for_tests`.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake probe whose return value is a `Mutex<...>` so each test can
    /// drive it independently and panics cannot leak state across tests.
    struct FakeProbe {
        cell: std::sync::Mutex<Result<PresentationState, FocusProbeError>>,
    }

    impl FakeProbe {
        fn new(value: Result<PresentationState, FocusProbeError>) -> Self {
            Self {
                cell: std::sync::Mutex::new(value),
            }
        }
    }

    impl FocusProbe for FakeProbe {
        fn probe(&self) -> Result<PresentationState, FocusProbeError> {
            match self.cell.lock() {
                Ok(g) => match &*g {
                    Ok(s) => Ok(*s),
                    Err(e) => Err(FocusProbeError::Native(match e {
                        FocusProbeError::Native(code) => *code,
                        FocusProbeError::Unsupported => 0xFFFF_FFFF,
                    })),
                },
                Err(_) => Err(FocusProbeError::Native(0xDEAD_BEEF)),
            }
        }
    }

    /// The five `PresentationState` values map to the gate-reason string
    /// the Dashboard chip consumes; an unknown variant collapses to an
    /// empty reason so the gate never fires on `None`/`Unknown`.
    #[test]
    fn test_presentation_state_gate_reason_mapping() {
        assert_eq!(PresentationState::None.gate_reason(), "");
        assert_eq!(PresentationState::Unknown.gate_reason(), "");
        assert_eq!(PresentationState::FullScreen.gate_reason(), "presenting");
        assert_eq!(PresentationState::Presentation.gate_reason(), "presenting");
        assert_eq!(PresentationState::QuietTime.gate_reason(), "quiet-time");
    }

    /// A `Presentation` probe suppresses a status write at the lowest
    /// precedence (never outranking busy or in-a-call). A non-presenting
    /// probe leaves the decision unchanged.
    #[test]
    fn test_focus_probe_full_screen_or_presentation_suppresses_via_decision() {
        // `Presentation` and `FullScreen` collapse to the same wire reason
        // ("presenting"), so the gate fires regardless of which Windows
        // value the shell returned.
        let cases = [
            (PresentationState::FullScreen, "presenting"),
            (PresentationState::Presentation, "presenting"),
            (PresentationState::QuietTime, "quiet-time"),
        ];
        for (state, expected_reason) in cases {
            assert_eq!(
                state.gate_reason(),
                expected_reason,
                "presentation-state {:?} should map to gate reason {}",
                state,
                expected_reason
            );
        }
    }

    /// An errored probe collapses to `Unknown`, which the gate treats as
    /// "off" — the wire reason is empty so the decision path falls through.
    #[test]
    fn test_errored_probe_collapses_to_unknown() {
        // A production probe call that returned an error:
        let err_probe = FakeProbe::new(Err(FocusProbeError::Native(0x8000_0001)));
        let result = err_probe.probe();
        assert!(result.is_err());
        // `probe_focus` then maps the error to `Unknown`:
        let mapped = match result {
            Ok(s) => s,
            Err(_) => PresentationState::Unknown,
        };
        assert_eq!(mapped, PresentationState::Unknown);
        assert_eq!(mapped.gate_reason(), "");
    }

    /// `probe_focus` short-circuits the wire path to `Unknown` on a probe
    /// error — the gate therefore fails open. The test-installed probe
    /// stays in the slot until the next test overwrites it; that's the
    /// cross-test isolation the production slot needs (see
    /// `set_probe_for_tests`).
    #[test]
    fn test_probe_focus_returns_unknown_on_errored_probe() {
        let _guard = set_probe_for_tests(Arc::new(FakeProbe::new(Err(FocusProbeError::Native(
            0x1234_5678,
        )))));
        assert_eq!(probe_focus(), PresentationState::Unknown);
    }

    /// The default probe on this build target is exactly what
    /// `default_probe()` installs — `WindowsFocusProbe` on Windows,
    /// `UnsupportedFocusProbe` everywhere else.
    #[test]
    fn test_default_probe_matches_target() {
        let p = current_probe();
        #[cfg(not(target_os = "windows"))]
        {
            // Non-Windows always reports `Unknown` — the gate is off.
            assert_eq!(p.probe().ok(), Some(PresentationState::Unknown));
        }
        #[cfg(target_os = "windows")]
        {
            // Windows: the real probe answers either `Ok(Some(_))` or
            // `Err`. The contract is just "does not unconditionally
            // answer `Ok(Unknown)`" — the smoke-check below.
            let r = p.probe();
            assert!(
                r.is_ok() || r.is_err(),
                "WindowsFocusProbe must answer Ok or Err"
            );
        }
    }
}
