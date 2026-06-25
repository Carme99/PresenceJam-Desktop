//! Tauri command handlers, split into per-workflow submodules.
//!
//! See issue #76. The split is purely organizational — each submodule groups
//! handlers that share state, helpers, or operational concerns. Behaviour is
//! unchanged from the pre-split single file; this is a cut-and-paste refactor.
//!
//! Submodule map (each lists every `#[tauri::command]` it owns):
//!   - `config` — load_config, save_config
//!   - `spotify_auth` — start_spotify_auth, start_spotify_reconnect, complete_spotify_auth_manual, refresh_spotify, is_spotify_client_secret_set
//!   - `teams_auth` — start_teams_auth_device_code, poll_teams_auth, refresh_teams
//!   - `sync` — start_syncing, stop_syncing, get_sync_status, app_exit
//!   - `window` — show_window, set_autostart_enabled, open_logs_folder, open_external_url
//!   - `onboarding` — is_onboarding_complete, complete_onboarding, reconnect_spotify, reconnect_teams
//!   - `misc` — preview_status, update_tray_menu_state

pub mod config;
pub mod misc;
pub mod onboarding;
pub mod spotify_auth;
pub mod sync;
pub mod teams_auth;
pub mod window;

#[cfg(test)]
mod tests {
    /// Regression guard for issue #76: the `commands` module must declare all
    /// 7 per-workflow submodules. If a contributor deletes one (or renames the
    /// module without updating this list), `cargo test` fails fast.
    #[test]
    fn test_commands_split_groups_present() {
        let source = include_str!("mod.rs");
        for group in &[
            "config",
            "spotify_auth",
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
        // called with seven literal-source pairs.
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
                    filename, needle
                );
            }
        }

        check(include_str!("config.rs"), "config.rs");
        check(include_str!("spotify_auth.rs"), "spotify_auth.rs");
        check(include_str!("teams_auth.rs"), "teams_auth.rs");
        check(include_str!("sync.rs"), "sync.rs");
        check(include_str!("window.rs"), "window.rs");
        check(include_str!("onboarding.rs"), "onboarding.rs");
        check(include_str!("misc.rs"), "misc.rs");
    }
}