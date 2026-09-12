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
}
