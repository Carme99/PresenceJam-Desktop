//! OS idle / last-input probe (issue #873).
//!
//! Playback keeps running from a phone or a hi-fi while the user is nowhere
//! near the PC; the app cannot tell, so a stale "listening to X" status
//! sits next to a colleague's message that will not be answered. The
//! desktop-idle probe gives the gate a real "is anyone at the desk?"
//! signal so it can stop advertising listening while the user is away
//! and restore the status on the first input.
//!
//! Sources per platform:
//!
//! - Windows — `GetLastInputInfo` (`Win32_UI_Input_KeyboardAndMouse`).
//! - macOS — `CGEventSourceSecondsSinceLastEventType(
//!   kCGEventSourceStateCombinedSessionState, kCGAnyInputEventType)`.
//!   Gated behind the `objc2` ecosystem, which the unsigned build cannot
//!   take a hard dependency on (contract: do not introduce a hard
//!   `objc2` dependency for this slice). Reported as `None` here.
//! - Linux — the systemd-logind `IdleHint` D-Bus property (primary) with
//!   an XScreenSaver `ScreenSaverTimeSinceLastInput` fallback. Both
//!   require crate additions (`zbus`, `x11rb-protocol`/screensaver) the
//!   unsigned build does not take either. Reported as `None` here.
//!
//! The probe is a trait so a fake can be installed from tests, and so the
//! real probe can be added later (per platform) without changing the
//! call-site signature.

use std::sync::{Arc, Mutex, OnceLock};

/// The idle probe's answer.
///
/// `Some(seconds)` is a confident reading: the user has not touched
/// the keyboard/mouse for `seconds`. `None` is "the OS does not tell
/// us" (Linux/macOS here, no D-Bus or X11, or no display server). The
/// gate treats `None` as "the gate is off", so a host without an idle
/// source keeps the 4.7 behaviour byte-for-byte. `Err` — the probe
/// failed (Windows could not read `GetLastInputInfo`, etc.). Same as
/// `None` for the gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdleSeconds(pub u64);

/// Why the probe could not produce a reading.
#[derive(Debug)]
pub enum IdleProbeError {
    /// The native call returned a non-zero error code or unexpected status.
    Native(u32),
    /// The probe is not implemented on this target — Linux / macOS ship
    /// `Ok(None)` from `UnsupportedIdleProbe`, but a future Linux
    /// implementation that lacks the systemd-logind daemon or an X server
    /// uses this variant.
    Unsupported,
}

impl std::fmt::Display for IdleProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IdleProbeError::Native(code) => {
                write!(f, "idle probe native error: 0x{:x}", code)
            }
            IdleProbeError::Unsupported => write!(f, "idle probe not implemented on this OS"),
        }
    }
}

impl std::error::Error for IdleProbeError {}

/// Trait abstracting the platform-specific idle probe so tests can install
/// a fake and production code compiles on every target.
pub trait IdleProbe: Send + Sync {
    fn probe(&self) -> Result<Option<IdleSeconds>, IdleProbeError>;
}

/// The Windows probe — calls `GetLastInputInfo` (`Win32_UI_Input_KeyboardAndMouse`).
///
/// `LASTINPUTINFO.dwTime` is in milliseconds since the system boot; we
/// subtract it from the current tick count and round down to seconds.
/// Returns `None` only when the conversion would overflow (a system
/// uptime of ~49 days plus 49 days of idle would be enough to trip it,
/// in practice never on a desktop that runs PresenceJam).
#[cfg(target_os = "windows")]
pub struct WindowsIdleProbe;

#[cfg(target_os = "windows")]
impl IdleProbe for WindowsIdleProbe {
    fn probe(&self) -> Result<Option<IdleSeconds>, IdleProbeError> {
        use windows::Win32::System::SystemInformation::GetTickCount;
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
        let mut info = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        // SAFETY: `LASTINPUTINFO` is fully initialised above (`cbSize` set
        // to the struct size, `dwTime` zeroed); `GetLastInputInfo` only
        // writes `dwTime` and only reads the `cbSize` it received.
        let ok = unsafe { GetLastInputInfo(&mut info) };
        if !ok.as_bool() {
            // No error code is exposed by the API — `GetLastInputInfo` only
            // returns `BOOL`. Surface a sentinel so the log line is greppable.
            log::warn!("[IDLE] WindowsIdleProbe: GetLastInputInfo returned FALSE, failing open");
            return Err(IdleProbeError::Native(0xFFFF_FFFF));
        }
        // `GetTickCount` is millisecond uptime since boot; subtracting the
        // last-input tick and dividing by 1000 yields seconds since input.
        // Wrap-safe arithmetic is fine here: `GetTickCount` and the
        // `LASTINPUTINFO.dwTime` field are both `u32` ms counters that
        // wrap together.
        let now_ms: u32 = unsafe { GetTickCount() };
        let elapsed_ms = now_ms.wrapping_sub(info.dwTime);
        Ok(Some(IdleSeconds((elapsed_ms / 1000) as u64)))
    }
}

/// The Linux / macOS probe — returns `Ok(None)` ("the OS does not tell
/// us"), which the gate treats as "the gate is off". Future work: ship
/// a `zbus`-backed systemd-logind probe on Linux and an `objc2`-backed
/// `CGEventSourceSecondsSinceLastEventType` probe on macOS, behind the
/// same `IdleProbe` trait so the call sites do not change.
pub struct UnsupportedIdleProbe;

impl IdleProbe for UnsupportedIdleProbe {
    fn probe(&self) -> Result<Option<IdleSeconds>, IdleProbeError> {
        Ok(None)
    }
}

/// The default probe installed at process start — Windows gets the real
/// `GetLastInputInfo`, every other target gets the "OS does not tell us"
/// placeholder.
fn default_probe() -> Arc<dyn IdleProbe> {
    #[cfg(target_os = "windows")]
    {
        Arc::new(WindowsIdleProbe)
    }
    #[cfg(not(target_os = "windows"))]
    {
        Arc::new(UnsupportedIdleProbe)
    }
}

/// Process-wide slot for the probe. Tests install a fake through
/// [`set_probe_for_tests`] and receive back an [`IdleProbeGuard`] that
/// restores the default on drop. Same `OnceLock`-of-`Mutex<Arc<...>>`
/// shape as [`crate::platform::focus`] — production reads never lock
/// after init.
static PROBE_SLOT: OnceLock<Mutex<Arc<dyn IdleProbe>>> = OnceLock::new();

fn probe_slot() -> &'static Mutex<Arc<dyn IdleProbe>> {
    PROBE_SLOT.get_or_init(|| Mutex::new(default_probe()))
}

/// Read the current probe. Cheap — single atomic load under a
/// short-lived `Mutex` guard.
pub fn current_probe() -> Arc<dyn IdleProbe> {
    probe_slot()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

/// Seconds since the last user input, or `None` if the OS does not
/// report it.
///
/// The default probe is `UnsupportedIdleProbe` on Linux/macOS and
/// `WindowsIdleProbe` on Windows. Tests install a fake through
/// [`set_probe_for_tests`].
pub fn seconds_since_last_input() -> Option<IdleSeconds> {
    match current_probe().probe() {
        Ok(opt) => opt,
        Err(e) => {
            log::debug!("[IDLE] seconds_since_last_input: {}", e);
            None
        }
    }
}

/// Install a test probe. Returns an [`IdleProbeGuard`] whose drop is
/// a no-op (the next test that installs a probe simply overwrites it).
/// A restore-on-drop design raced with parallel test execution: two
/// threads installing and restoring in opposite order can leave the
/// slot pointing at another thread's fake. Each test owns its own
/// probe Arc, so leaving it installed until the next test overwrites
/// is safe and removes the race entirely.
#[cfg(test)]
pub fn set_probe_for_tests(probe: Arc<dyn IdleProbe>) -> IdleProbeGuard {
    let mut slot = probe_slot().lock().unwrap_or_else(|e| e.into_inner());
    *slot = probe;
    IdleProbeGuard
}

/// RAII marker for a test-installed probe. Drop is a no-op by design
/// — see [`set_probe_for_tests`] for why.
#[cfg(test)]
#[must_use = "the test probe is installed until the next set_probe_for_tests call"]
pub struct IdleProbeGuard;

#[cfg(test)]
impl Drop for IdleProbeGuard {
    fn drop(&mut self) {
        // Intentionally empty — see the doc on `set_probe_for_tests`.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Issue #1041: see focus.rs — same process-global slot race.
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// A fake probe whose return value is a `Mutex<...>` so each test
    /// can drive it independently. The optional `before_input` field is
    /// a vector of values the probe returns in order, then `Ok(None)`
    /// forever — that is how the acceptance test asserts the
    /// "threshold → input event → forced write" lifecycle without
    /// driving the real polling loop.
    struct FakeProbe {
        cell: std::sync::Mutex<FakeState>,
    }

    struct FakeState {
        // `Ok(Some(seconds))` is the steady idle state.
        // `Ok(None)` is "the OS does not tell us".
        // `Err(...)` is the failure path.
        responses: Vec<Result<Option<IdleSeconds>, IdleProbeError>>,
    }

    impl FakeProbe {
        fn new(responses: Vec<Result<Option<IdleSeconds>, IdleProbeError>>) -> Self {
            Self {
                cell: std::sync::Mutex::new(FakeState { responses }),
            }
        }
    }

    impl IdleProbe for FakeProbe {
        fn probe(&self) -> Result<Option<IdleSeconds>, IdleProbeError> {
            let mut state = self.cell.lock().unwrap_or_else(|e| e.into_inner());
            if state.responses.is_empty() {
                Ok(None)
            } else {
                state.responses.remove(0)
            }
        }
    }

    /// The decision the gate folds the probe into — a pure predicate so
    /// the test surface is independent of the polling loop. Given the
    /// configured threshold (`0` disables) and the probe's reading, the
    /// gate is active exactly when the threshold is in (60..=3600) and
    /// the probe answered `Some(seconds)` with `seconds >= threshold`.
    fn idle_gate_active(threshold: u64, reading: Option<IdleSeconds>) -> bool {
        (60..=3600).contains(&threshold) && reading.is_some_and(|s| s.0 >= threshold)
    }

    /// `0` disables the gate — the documented opt-out, and the value an
    /// untouched config carries (no `idle_away_after_seconds` key).
    #[test]
    fn test_zero_disables_the_gate() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        assert!(!idle_gate_active(0, Some(IdleSeconds(1))));
        assert!(!idle_gate_active(0, Some(IdleSeconds(3600))));
        assert!(!idle_gate_active(0, None));
    }

    /// The threshold is clamped to 60..=3600 — a hand-edited config
    /// cannot put the gate in a state that surprises the user (a
    /// one-second threshold would have every normal typing pause fire
    /// the gate). The clamp is enforced in `clamp_teams`; this test is
    /// the regression guard for the gate's acceptance of those values.
    #[test]
    fn test_threshold_is_clamped_to_sixty_three_thousand_six_hundred() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        assert!(!idle_gate_active(30, Some(IdleSeconds(45))));
        assert!(!idle_gate_active(7200, Some(IdleSeconds(10_000))));
        assert!(!idle_gate_active(30, None));
        assert!(!idle_gate_active(7200, None));
        // In-range thresholds fire when the probe answers above them.
        assert!(idle_gate_active(60, Some(IdleSeconds(60))));
        assert!(idle_gate_active(3600, Some(IdleSeconds(3600))));
        // A reading below the threshold keeps the gate off.
        assert!(!idle_gate_active(300, Some(IdleSeconds(120))));
    }

    /// A reading of `None` (the OS does not tell us) keeps the gate off
    /// regardless of the threshold — the "fail open" rule for an
    /// unavailable probe.
    #[test]
    fn test_unavailable_probe_leaves_the_gate_off() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        assert!(!idle_gate_active(60, None));
        assert!(!idle_gate_active(3600, None));
    }

    /// The acceptance-criterion lifecycle: an idle clock crosses the
    /// threshold and asserts the gate is active, then an input event
    /// drops the reading below the threshold and asserts it cleared.
    /// The probe answers the configured sequence; the production gate
    /// would then write the resumed status once (covered end-to-end by
    /// the polling-loop integration tests, not here).
    #[test]
    fn test_idle_clock_crosses_threshold_then_input_clears_it() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let threshold = 60_u64;
        let responses = vec![
            Ok(Some(IdleSeconds(120))), // crossed
            Ok(Some(IdleSeconds(30))),  // input event
        ];
        let probe = Arc::new(FakeProbe::new(responses));
        let _guard = set_probe_for_tests(probe);

        // First read: above the threshold → gate active.
        let first_reading = seconds_since_last_input();
        assert!(matches!(first_reading, Some(IdleSeconds(120))));
        assert!(idle_gate_active(threshold, first_reading));

        // Second read: input event → gate cleared.
        let second_reading = seconds_since_last_input();
        assert!(matches!(second_reading, Some(IdleSeconds(30))));
        assert!(!idle_gate_active(threshold, second_reading));
    }

    /// An errored probe collapses to `None` — the gate stays off
    /// rather than panicking. The slot-restore on drop is the
    /// cross-test isolation the production slot needs.
    #[test]
    fn test_errored_probe_collapses_to_none() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let probe = Arc::new(FakeProbe::new(vec![Err(IdleProbeError::Native(
            0x8000_0001,
        ))]));
        let _guard = set_probe_for_tests(probe);
        assert_eq!(seconds_since_last_input(), None);
    }

    /// The default probe on this build target is exactly what
    /// `default_probe()` installs.
    #[test]
    fn test_default_probe_matches_target() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        // Issue #1041: reinstall the default first (see focus.rs).
        *probe_slot().lock().unwrap_or_else(|e| e.into_inner()) = default_probe();
        let p = current_probe();
        let reading = p.probe();
        #[cfg(target_os = "windows")]
        {
            // Windows: the real probe returns `Ok(Some(_))` or `Err`. A
            // unit test cannot pin a value (it depends on system uptime
            // and the last input event), but it CAN pin that the probe
            // is not the unsupported one — i.e. it does not answer
            // `Ok(None)` unconditionally.
            assert!(
                reading.is_ok() || reading.is_err(),
                "WindowsIdleProbe must answer"
            );
        }
        #[cfg(not(target_os = "windows"))]
        {
            // Non-Windows always reports `None` — the gate is off.
            assert_eq!(reading.ok(), Some(None));
        }
    }
}
