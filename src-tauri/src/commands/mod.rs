//! Tauri command handlers, split into per-workflow submodules.
//!
//! See issue #76. The split is purely organizational — each submodule groups
//! handlers that share state, helpers, or operational concerns. Behaviour is
//! unchanged from the pre-split single file; this is a cut-and-paste refactor.
//!
//! Submodule map (each lists every `#[tauri::command]` it owns):
//!   - `config` — load_config, save_config
//!   - `spotify_auth` — start_spotify_auth, start_spotify_reconnect, complete_spotify_auth_manual, refresh_spotify, is_spotify_client_secret_set
//!   - `playback` — playback_play, playback_pause, playback_next, playback_previous, playback_transfer, get_playback_devices, get_playback_queue, get_spotify_granted_scopes
//!   - `teams_auth` — start_teams_auth_device_code, poll_teams_auth, refresh_teams, get_teams_granted_scopes
//!   - `sync` — start_syncing, stop_syncing, get_sync_status, app_exit
//!   - `window` — show_window, set_autostart_enabled, open_logs_folder, open_external_url
//!   - `onboarding` — is_onboarding_complete, complete_onboarding, reconnect_spotify, reconnect_teams
//!   - `misc` — preview_status, update_tray_menu_state

pub mod config;
pub mod misc;
pub mod onboarding;
pub mod playback;
pub mod spotify_auth;
pub mod sync;
pub mod teams_auth;
pub mod window;

/// Detached-window isolation (issue #241): the Logs/Settings pop-out windows
/// (`logs-detached` / `settings-detached`) share the app-global command
/// surface, so sensitive app commands must reject non-main callers Rust-side
/// before touching keychain, tokens, config, or process state. The Tauri
/// capability split (`capabilities/detached.json`) only gates webview APIs —
/// it is cosmetic for `invoke()` — hence this runtime guard.
///
/// The main window label is exactly `"main"` (see `tauri.conf.json`); every
/// other label is rejected. `window: tauri::Window` injection is transparent
/// to `invoke_handler` registration, so guarded commands need no wiring
/// change in `lib.rs`.
///
/// Pure predicate over the window label: true only for exactly `"main"`.
pub fn is_main_window_label(label: &str) -> bool {
    label == "main"
}

/// Rejects invocations from any window other than the main one. Call this
/// first in every sensitive command, before any keychain/token/config read
/// or side effect.
pub fn require_main_window(window: &tauri::Window) -> Result<(), String> {
    let label = window.label();
    if is_main_window_label(label) {
        Ok(())
    } else {
        log::error!(
            "[CMD.GUARD] require_main_window: rejected caller from window '{}' — sensitive app commands are only available in the main window",
            label
        );
        Err("This command is only available in the main window.".to_string())
    }
}

/// Issue #485 caller-location matrix: which commands are guarded by
/// `require_main_window`, which are intentionally unguarded, and why.
/// Commands without a `tauri::Window` param cannot call the guard (it
/// needs the caller label); their main-only status is justified by caller
/// location instead -- every frontend call site lives in a main-window-only
/// view (Dashboard, +page, UpdatePrompt, Diagnostics-as-main-route).
/// Detached windows (`logs-detached` / `settings-detached`) host only
/// Settings + LogViewer, whose invokes are the allowlist below.
///
/// GUARDED (take `window` and reject non-main first):
/// start_syncing, stop_syncing, app_exit, refresh_status (sync.rs),
/// start_spotify_auth, start_spotify_reconnect, complete_spotify_auth_manual,
/// refresh_spotify (spotify_auth.rs), start_teams_auth_device_code,
/// refresh_teams (teams_auth.rs), complete_onboarding (onboarding.rs),
/// relaunch_app (misc.rs), stage_deferred_update (updater_bg.rs).
///
/// INTENTIONALLY UNGUARDED -- main-only by caller location (no Window param):
/// playback_play/pause/next/previous/transfer, get_playback_devices/queue
/// (Dashboard tray-adjacent controls + tray worker; Dashboard is main-only),
/// show_window (+page main route), update_tray_menu_state (Dashboard),
/// get_diagnostics_snapshot (Diagnostics-as-main-route), preview_status
/// (Settings preview but read-only pure computation), get_sync_status
/// (Dashboard/Settings status read), get_spotify/teams_granted_scopes
/// (Settings scope readers, no side effect), is_spotify_client_secret_set
/// (Settings/Reconnect presence read), clear_failed_update_install
/// (Diagnostics dismiss; deletes only the marker file).
///
/// INTENTIONALLY UNGUARDED -- detached-legit (invoked from popped-out
/// Settings/LogViewer by design): reconnect_spotify, reconnect_teams,
/// poll_teams_auth, set_autostart_enabled, open_logs_folder,
/// open_external_url (Teams verification-URL open during detached
/// device-code flow).
#[cfg(test)]
mod tests {
    /// Regression guard for issue #76: the `commands` module must declare all
    /// 8 per-workflow submodules. If a contributor deletes one (or renames the
    /// module without updating this list), `cargo test` fails fast.
    #[test]
    fn test_commands_split_groups_present() {
        let source = include_str!("mod.rs");
        for group in &[
            "config",
            "spotify_auth",
            "playback",
            "teams_auth",
            "sync",
            "window",
            "onboarding",
            "misc",
        ] {
            let needle_pub = format!("pub mod {};", group);
            let needle_priv = format!("mod {};", group);
            assert!(
                source.contains(&needle_pub) || source.contains(&needle_priv),
                "commands/mod.rs must declare submodule `{}` (issue #76 split)",
                group
            );
        }
    }

    /// Regression guard for the bonus log-tag sweep (issue #79 item 3):
    /// the legacy un-namespaced `[CMD]` prefix must no longer appear in
    /// any of the per-group command files. Each group should use its
    /// own `[CMD.<GROUP>]` constant.
    #[test]
    fn test_log_tags_use_namespaced_prefix() {
        // `include_str!` requires a literal path, so this is one helper fn
        // called with eight literal-source pairs.
        fn check(source: &str, filename: &str) {
            for needle in &[
                "log::debug!(\"[CMD] ",
                "log::info!(\"[CMD] ",
                "log::warn!(\"[CMD] ",
                "log::error!(\"[CMD] ",
            ] {
                assert!(
                    !source.contains(needle),
                    "commands/{} still contains legacy {} ...\") prefix \
                     — issue #79 item 3 requires the file to use its own \
                     [CMD.<GROUP>] constant",
                    filename,
                    needle
                );
            }
        }

        check(include_str!("config.rs"), "config.rs");
        check(include_str!("spotify_auth.rs"), "spotify_auth.rs");
        check(include_str!("playback.rs"), "playback.rs");
        check(include_str!("teams_auth.rs"), "teams_auth.rs");
        check(include_str!("sync.rs"), "sync.rs");
        check(include_str!("window.rs"), "window.rs");
        check(include_str!("onboarding.rs"), "onboarding.rs");
        check(include_str!("misc.rs"), "misc.rs");
    }

    /// Detached-window guard predicate (issue #241): only exactly `"main"`
    /// passes; detached labels, empty, and case/whitespace-tampered labels
    /// are rejected.
    #[test]
    fn test_is_main_window_label() {
        assert!(super::is_main_window_label("main"));
        for rejected in &["logs-detached", "settings-detached", "", "Main", "main "] {
            assert!(
                !super::is_main_window_label(rejected),
                "label {:?} must not pass the main-window guard (issue #241)",
                rejected
            );
        }
    }

    /// Issue #485: the guard predicate is the enforcement primitive every
    /// guarded command funnels through -- pin its exact accept/reject
    /// boundary behaviorally (not by grepping call sites, which pins
    /// implementation text). The caller-location matrix above documents
    /// which commands are guarded vs intentionally unguarded and why;
    /// this test pins the primitive the matrix relies on.
    #[test]
    fn test_guard_matrix_primitive_accepts_only_main() {
        use super::is_main_window_label;
        // Accept: exactly "main".
        assert!(is_main_window_label("main"));
        // Reject: detached labels, empty, case/whitespace tampering, and
        // near-miss prefixes/suffixes a confused deputy might present.
        for rejected in &[
            "logs-detached",
            "settings-detached",
            "",
            "Main",
            "MAIN",
            "main ",
            " main",
            "main-window",
            "mainwindow",
            "detached",
        ] {
            assert!(
                !is_main_window_label(rejected),
                "label {:?} must not pass the main-window guard (issue #485)",
                rejected
            );
        }
    }
}
