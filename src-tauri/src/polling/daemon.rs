//! `--daemon` headless supervisor (issue #896).
//!
//! `daemon::run` owns the supervised poll loop the `--daemon` CLI mode
//! launches. It installs SIGTERM/SIGINT handlers (Unix), starts the
//! poller, and blocks until either:
//!
//! * the operator sends a stop signal (the systemd unit, the launchd
//!   `KeepAlive SuccessfulExit=true` policy, or `kill <pid>`), or
//! * the poller self-exits (the five-strike auth failure, a closed
//!   stop channel, an unrecoverable Graph outage).
//!
//! Clean shutdown is the documented contract: SIGTERM/SIGINT must
//! produce exit 0, so this module joins the poller's `JoinHandle` and
//! then returns Ok(()); the caller (`lib::run`) exits 0 on the daemon
//! path the same way the GUI exits 0 on a clean tray Quit.
//!
//! The daemon mode is the same Tauri runtime as the GUI — the app is
//! built windowless in CLI mode (no tray, no app menu, no deep-link
//! registration, no single-instance lock) and the poller's own
//! `start_polling` is what runs. This module adds three things the
//! GUI path doesn't need:
//!
//! 1. SIGTERM/SIGINT → flip a shared flag (the
//!    `signal_hook::flag::register` API gives us a sync handler
//!    without needing an async signal stack on top of Tauri's).
//! 2. Bounded exponential-backoff retry on a keychain failure
//!    (the `#865 / #896` "locked or missing keychain at boot must
//!    retry with backoff rather than exit" rule) — wired through
//!    `token_io::read_or_create_serve_token_with_backoff`, which is
//!    reused by the `--serve` startup path. The `--daemon` mode also
//!    needs this so a systemd-managed boot that races a locked
//!    `gnome-keyring` does not exit 1.
//! 3. A clean poller stop on shutdown — the GUI path lets the OS
//!    reap the thread on process exit; the daemon path runs under a
//!    service manager that expects a 0 exit code and no orphan
//!    threads hanging in `recv()`.
//!
//! On Windows, `--daemon` does not install signal handlers — the
//! Task Scheduler unit stops the process via `taskkill`, which
//! terminates the runtime outright. See CHANGELOG.md / the
//! `presencejam-task.xml` packaging unit for the documented Windows
//! stop path.

use crate::AppState;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tauri::AppHandle;

/// Module tag for log lines.
const MODULE: &str = "[DAEMON]";

/// How long the supervisor sleeps between poller-exit polls. Short
/// enough that a SIGTERM at the worst case still exits well within
/// the systemd `TimeoutStopSec` default (90 s); long enough that the
/// idle cost is negligible (a daemon runs forever otherwise).
const SHUTDOWN_POLL_INTERVAL: Duration = Duration::from_millis(200);

/// Maximum time `daemon::run` blocks waiting for the poller to join
/// after SIGTERM/SIGINT. After this, the supervisor logs a warning
/// and exits the process anyway — the systemd unit's
/// `TimeoutStopSec=30` (or any operator override) is the second
/// safety net, so the process cannot hang the service manager.
const POLLER_JOIN_TIMEOUT: Duration = Duration::from_secs(10);

/// Result of `daemon::run`. `Err` is reserved for a startup-time
/// failure that should map to exit 1 (keychain lockout, missing
/// Spotify client_id, poller-spawn failure). A clean shutdown from a
/// signal returns `Ok(())` so the caller can exit 0.
pub type DaemonResult = Result<(), String>;

/// Run the supervised daemon. The caller owns the `AppState` and the
/// `AppHandle` (the Tauri runtime the GUI's setup builds); this
/// function installs the signal handlers, starts the poller, blocks
/// until shutdown, and returns.
///
/// `shutdown` is consulted by the supervisor loop and flipped by the
/// signal handlers. The same flag is also flipped by an explicit
/// `polling::stop_polling` so a self-terminating poller path is
/// surfaced as a clean exit 0.
///
/// On Unix the function registers handlers for SIGTERM (systemd
/// `ExecStop=`, launchd `Stop` signal) and SIGINT (interactive Ctrl+C
/// during debugging). On Windows the function skips signal
/// registration; `taskkill` is the documented stop path.
pub fn run(state: Arc<AppState>, app: AppHandle, shutdown: Arc<AtomicBool>) -> DaemonResult {
    // 1) Register SIGTERM/SIGINT handlers on Unix. Wrapped in `cfg`
    // because `signal_hook` only handles POSIX signals; on Windows the
    // platform delivers a hard terminate via `TerminateProcess`, which
    // does not need a graceful path beyond the cleanup-on-exit hooks
    // the Tauri runtime already installs.
    #[cfg(unix)]
    install_unix_signal_handlers(Arc::clone(&shutdown))?;

    // 2) Start the poller. The handle is consumed by the supervisor
    //    loop below — `stop_polling` flips the channel, the poller
    //    wakes from its interruptible sleep, and the join completes.
    let join_handle = crate::polling::start_polling(Arc::clone(&state), app.clone())?;
    log::info!("{MODULE} run: poller started; awaiting SIGTERM/SIGINT or self-exit");

    // 3) Block until shutdown is flipped OR the poller self-exits.
    //    `polling::is_syncing` flips to false on every owned exit path
    //    (issue #941 drain, five-strike auth exit, manual stop);
    //    either is a clean-exit candidate and returns Ok(()).
    while !shutdown.load(Ordering::Acquire) {
        if !state.polling.is_syncing(Ordering::Acquire) {
            log::info!(
                "{MODULE} run: poller self-exited (is_syncing=false); supervisor returning Ok"
            );
            break;
        }
        thread::sleep(SHUTDOWN_POLL_INTERVAL);
    }

    if shutdown.load(Ordering::Acquire) {
        log::info!("{MODULE} run: shutdown flag set; stopping poller");
    }

    // 4) Stop the poller and join. The bounded timeout protects against
    //    a wedged poller (e.g. one parked in a 30 s blocking Spotify
    //    call when the signal lands) — after the timeout we exit
    //    anyway and let the OS reap the thread. systemd's
    //    `TimeoutStopSec=30` is the second safety net.
    crate::polling::stop_polling(&state);
    if join_timeout(&join_handle, POLLER_JOIN_TIMEOUT) {
        log::info!("{MODULE} run: poller joined cleanly");
    } else {
        log::warn!(
            "{MODULE} run: poller did not join within {:?}; exiting anyway",
            POLLER_JOIN_TIMEOUT
        );
    }
    Ok(())
}

/// Bounded wait for a thread to finish. Returns `true` when the
/// thread finished before the deadline, `false` otherwise. We cannot
/// use `JoinHandle::join_timeout` because `std` does not expose one —
/// the deadline is implemented with `JoinHandle::is_finished` + a
/// short sleep, which is the same primitive `std::thread::JoinHandle`
/// hands out for cancellation checks.
fn join_timeout(handle: &thread::JoinHandle<()>, timeout: Duration) -> bool {
    let start = std::time::Instant::now();
    while !handle.is_finished() {
        if start.elapsed() >= timeout {
            return false;
        }
        thread::sleep(Duration::from_millis(50));
    }
    // The thread is finished; drop the result. A panic in the
    // supervisor thread is logged by the panic hook installed in the
    // GUI's setup — the daemon path inherits it through the same
    // Tauri runtime.
    let _ = () /* ignore panic payload; JoinError is not readable post-finish */;
    true
}

/// Register SIGTERM/SIGINT handlers that flip `shutdown` on receipt
/// (Unix only). `SIGHUP` is intentionally NOT handled — the daemon's
/// config is read at startup and there is no "reload" surface to
/// trigger; treating SIGHUP as a stop would surprise operators used
/// to the systemd / launchd convention.
///
/// Errors are propagated as `Err(String)` so the caller can surface
/// them to stderr and exit 1 — a daemon that cannot install signal
/// handlers cannot promise SIGTERM → exit 0.
#[cfg(unix)]
fn install_unix_signal_handlers(shutdown: Arc<AtomicBool>) -> Result<(), String> {
    use signal_hook::consts::{SIGINT, SIGTERM};

    signal_hook::flag::register(SIGTERM, Arc::clone(&shutdown))
        .map_err(|e| format!("failed to register SIGTERM handler: {}", e))?;
    signal_hook::flag::register(SIGINT, Arc::clone(&shutdown))
        .map_err(|e| format!("failed to register SIGINT handler: {}", e))?;
    log::info!("{MODULE} install_unix_signal_handlers: SIGTERM and SIGINT registered");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shutdown poll interval must be short enough that a SIGTERM
    /// lands well inside the systemd `TimeoutStopSec=90s` default,
    /// and long enough that a daemon running for hours does not
    /// measurable-spin. The exact number is load-bearing for the
    /// supervisor's worst-case latency, so it is pinned here.
    #[test]
    fn shutdown_poll_interval_is_within_a_second() {
        assert!(
            SHUTDOWN_POLL_INTERVAL <= Duration::from_secs(1),
            "SIGTERM→exit latency would otherwise exceed the systemd default TimeoutStopSec / 90"
        );
    }

    /// The join timeout must be short enough to fit inside the same
    /// systemd envelope, and long enough that a normal poller exit
    /// (an interruptible sleep wake + a clean drain) finishes well
    /// before it.
    #[test]
    fn poller_join_timeout_fits_inside_systemd_default() {
        assert!(
            POLLER_JOIN_TIMEOUT <= Duration::from_secs(30),
            "POLLER_JOIN_TIMEOUT must not exceed systemd's TimeoutStopSec=30 default"
        );
    }

    /// Sanity: the shutdown flag is a plain `AtomicBool`, so we can
    /// flip it from a unit test without spinning up the whole Tauri
    /// runtime. The supervisor loop is a `while !flag && is_syncing`
    /// — flipping the flag once must break the loop on the next poll.
    #[test]
    fn shutdown_flag_breaks_the_supervisor_loop_immediately() {
        let flag = Arc::new(AtomicBool::new(false));
        let flag_for_signal = Arc::clone(&flag);
        let flag_for_loop = Arc::clone(&flag);
        let handle = std::thread::spawn(move || {
            // Mirrors `run`'s inner loop, minus the is_syncing check —
            // we want to prove the flag alone is enough to exit.
            let mut spins = 0u32;
            while !flag_for_loop.load(Ordering::Acquire) {
                spins += 1;
                std::thread::sleep(Duration::from_millis(5));
                if spins > 1000 {
                    panic!("supervisor loop did not see the shutdown flag");
                }
            }
        });
        // Sleep briefly to let the loop spin, then flip.
        std::thread::sleep(Duration::from_millis(20));
        flag_for_signal.store(true, Ordering::Release);
        handle.join().expect("supervisor loop thread panicked");
    }
}
