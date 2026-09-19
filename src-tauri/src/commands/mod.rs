//! Tauri command handlers, split into per-workflow submodules.
//!
//! See issue #76. The split is purely organizational — each submodule groups
//! handlers that share state, helpers, or operational concerns. Behaviour is
//! unchanged from the pre-split single file; this is a cut-and-paste refactor.
//!
//! Submodule map (each lists every `#[tauri::command]` it owns):
//!   - `config` — load_config, save_config, update_config, export_config, import_config
//!   - `spotify_auth` — start_spotify_auth, start_spotify_reconnect, complete_spotify_auth_manual, refresh_spotify, is_spotify_client_secret_set, reconnect_spotify_session
//!   - `playback` — get_spotify_granted_scopes (issue #770 deleted the seven
//!     callerless playback_* / get_playback_* commands; the tray and the global
//!     hotkeys call `playback::player_with_refresh_typed` / `player_with_refresh`
//!     directly, with no IPC hop)
//!   - `teams_auth` — start_teams_auth_device_code, poll_teams_auth, refresh_teams, cancel_teams_auth_poll, get_teams_granted_scopes
//!   - `sync` — start_syncing, stop_syncing, get_sync_status, app_exit
//!   - `window` — show_window, set_autostart_enabled, open_logs_folder, open_external_url
//!   - `onboarding` — is_onboarding_complete, complete_onboarding, reconnect_spotify, reconnect_teams
//!   - `misc` — preview_status, update_tray_menu_state
//!   - `logs` — get_recent_logs (LogViewer history backfill, issue #595)
//!   - `shortcuts` — register_shortcuts, unregister_shortcuts, validate_shortcut (global hotkeys, issue #676)

pub mod config;
pub mod logs;
pub mod misc;
pub mod onboarding;
pub mod playback;
pub mod shortcuts;
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
/// show_window (+page main route), update_tray_menu_state (Dashboard),
/// get_diagnostics_snapshot (Diagnostics-as-main-route), preview_status
/// (Settings preview but read-only pure computation), get_sync_status
/// (Dashboard/Settings status read), get_spotify/teams_granted_scopes
/// (Settings scope readers, no side effect), is_spotify_client_secret_set
/// (Settings/Reconnect presence read), clear_failed_update_install
/// (Diagnostics dismiss; deletes only the marker file),
/// is_onboarding_complete (issue #770: the boot gate in
/// `src/routes/+page.svelte`, a main-window route).
///
/// INTENTIONALLY UNGUARDED -- detached-legit (invoked from popped-out
/// Settings/LogViewer by design): reconnect_spotify, reconnect_teams,
/// poll_teams_auth, set_autostart_enabled, open_logs_folder,
/// open_external_url (Teams verification-URL open during detached
/// device-code flow), save_config (whole-document config write) and
/// update_config (field-level config write, issue #535) — both are reached
/// from Settings, which is one of the two detached-hosting views, as are
/// export_config (reads the config, writes a user-chosen file) and
/// import_config (replaces the config from a user-chosen file) — issue #673;
/// get_recent_logs (LogViewer history backfill, issue #595) — the Logs pane
/// is hosted in either window, and the file it tails is the same local file
/// `open_logs_folder` already exposes to both, unredacted there and here
/// alike (only the paste-able snapshot is redacted, #434).
///
/// set_locale (issue #770) — the language picker lives in Settings, one of
/// the two detached-hosting views (`src/lib/i18n/store.svelte.ts`).
///
/// shortcuts: register_shortcuts, unregister_shortcuts, validate_shortcut
/// (issue #676) — the Settings pane is one of the two detached-hosting views
/// and hosts the hotkey card, so the pane that captures a combo must also be
/// able to (re)register it; the commands act on the persisted config and this
/// process's own OS grabs only.
#[cfg(test)]
mod tests {
    /// Regression guard for issue #76: the `commands` module must declare
    /// every per-workflow submodule. If a contributor deletes one (or renames
    /// the module without updating this list), `cargo test` fails fast.
    /// `logs` joined the list with the #595 LogViewer backfill.
    #[test]
    fn test_commands_split_groups_present() {
        let source = include_str!("mod.rs");
        for group in &[
            "config",
            "logs",
            "spotify_auth",
            "playback",
            "teams_auth",
            "sync",
            "window",
            "onboarding",
            "misc",
            "shortcuts",
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
        check(include_str!("shortcuts.rs"), "shortcuts.rs");
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

    /// Issue #215/#928: the only commands in this slice's files that may stay
    /// synchronous. Each carries a documented "no disk, network or keychain
    /// IO" decision at its definition, so a fast window-manager call or a
    /// pure string substitution is not pushed onto the blocking pool. A new
    /// synchronous command must be added here with its rationale (it is a
    /// deliberate decision, not a default) or made `async`.
    const SYNC_EXCEPTIONS: &[&str] = &["preview_status", "show_window"];

    /// Issue #928: a `#[tauri::command]` body that does network, disk or
    /// keychain work. Comment text is included in the haystack, so the cost
    /// is an occasional false positive — never a false negative (the guard
    /// exists to catch an unconverted command, not to bless one).
    fn touches_blocking_io(body: &str) -> bool {
        ["keychain::", "persist_tokens", "reqwest"]
            .iter()
            .any(|needle| body.contains(needle))
    }

    /// Parse every `#[tauri::command]` item out of a command module's source
    /// into `(name, is_async, body)`. Bodies are brace-counted from the first
    /// `{` after the signature (house style: never boundary anchors), and the
    /// attribute marker means plain helper fns are skipped.
    fn commands_in(src: &str) -> Vec<(String, bool, String)> {
        let mut found = Vec::new();
        let mut rest = src;
        while let Some(attr) = rest.find("#[tauri::command]") {
            let after_attr = &rest[attr..];
            let Some(fn_rel) = after_attr.find("fn ") else {
                break;
            };
            let is_async = after_attr[..fn_rel].contains("async ");
            let after_fn = &after_attr[fn_rel + "fn ".len()..];
            let Some(paren_rel) = after_fn.find('(') else {
                break;
            };
            let name = after_fn[..paren_rel].trim().to_string();
            let Some(brace_rel) = after_fn.find('{') else {
                break;
            };
            let mut depth = 0usize;
            let mut end = None;
            for (i, ch) in after_fn[brace_rel..].char_indices() {
                match ch {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            end = Some(brace_rel + i + 1);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let Some(end) = end else {
                break;
            };
            found.push((name, is_async, after_fn[brace_rel..end].to_string()));
            rest = &after_fn[end..];
        }
        found
    }

    /// Issue #928: Tauri runs a non-async `#[tauri::command]` on the main/UI
    /// thread, so a synchronous command that reads the keychain, does an HTTPS
    /// round trip or fsyncs freezes the window, the tray menu and window
    /// events for the whole call — the class #215 fixed elsewhere and left its
    /// "all commands touching network/disk/keychain declared async" item
    /// unchecked for.
    ///
    /// Scope (orchestrator ruling, 2026-09-19): #928 asked this guard to scan
    /// every `#[tauri::command]` in the tree. Scanning the tree from this
    /// branch would fail on files owned by other slices that are not converted
    /// here, so the three files this slice owns are scanned in full and the
    /// remainder is tracked in `W1-NOTES.md` (`commands/config.rs::load_config`;
    /// `commands/spotify_auth.rs::refresh_spotify`, `start_spotify_reconnect`,
    /// `reconnect_spotify_session`). Wave 2 tightens this to the whole tree
    /// once that remainder is off the main thread.
    #[test]
    fn test_commands_touching_io_are_async_and_offloaded() {
        // Positive control: the detector must fire on a body that does exactly
        // the work this guard is about, or the guard would pass forever.
        let fixture = concat!(
            "#[tauri::command]\n",
            "pub fn probe(state: tauri::State<'_, ()>) -> Result<(), String> {\n",
            "    let _ = keychain::get_spotify_client_secret();\n",
            "    Ok(())\n",
            "}\n",
        );
        let parsed = commands_in(fixture);
        assert_eq!(parsed.len(), 1, "the scanner must find one command");
        assert!(
            !parsed[0].1,
            "the fixture must parse as a synchronous command"
        );
        assert!(
            touches_blocking_io(&parsed[0].2),
            "the detector must fire on a keychain read (issue #928)"
        );

        let mut scanned: Vec<String> = Vec::new();
        for (file, src) in [
            ("commands/sync.rs", include_str!("sync.rs")),
            ("commands/window.rs", include_str!("window.rs")),
            ("commands/misc.rs", include_str!("misc.rs")),
        ] {
            for (name, is_async, body) in commands_in(src) {
                if touches_blocking_io(&body) {
                    assert!(
                        is_async,
                        "{file}::{name} does network/disk/keychain work in a \
                         synchronous command, so Tauri runs it on the main \
                         thread and freezes the UI — make it async and offload \
                         the body (issue #928)"
                    );
                    assert!(
                        body.contains("spawn_blocking"),
                        "{file}::{name} is async but does not offload its \
                         blocking work to the blocking pool (issue #928)"
                    );
                } else {
                    assert!(
                        is_async || SYNC_EXCEPTIONS.contains(&name.as_str()),
                        "{file}::{name} is a new synchronous command: make it \
                         async with a blocking offload, or add it to \
                         SYNC_EXCEPTIONS with its no-IO rationale (issue #928)"
                    );
                }
                scanned.push(name);
            }
        }

        // Scanner sanity + no stale exceptions: both prove the parse above
        // really walked these files rather than finding nothing.
        for expected in [
            "show_window",
            "preview_status",
            "relaunch_app",
            "get_sync_status",
        ] {
            assert!(
                scanned.iter().any(|name| name.as_str() == expected),
                "the scanner must see `{expected}` (issue #928)"
            );
        }
        for allowed in SYNC_EXCEPTIONS {
            assert!(
                scanned.iter().any(|name| name.as_str() == *allowed),
                "SYNC_EXCEPTIONS lists `{allowed}`, which no longer exists — \
                 drop the stale exception (issue #928)"
            );
        }
    }

    /// Issue #806: `relaunch_app` must discard a payload staged for "install
    /// on quit" BEFORE `app.restart()`. The restart fires `RunEvent::Exit`,
    /// which runs `install_pending_on_exit` — without the discard, an
    /// immediate relaunch installs the staged version on top of the one just
    /// installed. The ordering is the whole fix and the command needs a live
    /// `AppHandle` to drive (this crate has no mock runtime), so the guard is
    /// a source-order assertion over the command's own body.
    #[test]
    fn test_relaunch_app_discards_a_staged_update_before_restart() {
        let body = commands_in(include_str!("misc.rs"))
            .into_iter()
            .find(|(name, _, _)| name == "relaunch_app")
            .expect("misc.rs must define the relaunch_app command (issue #806)")
            .2;

        let discard = body
            .find("discard_staged_update(&app)")
            .expect("relaunch_app must discard a staged update (issue #806)");
        let restart = body
            .find("app.restart();")
            .expect("relaunch_app must restart the app");
        assert!(
            discard < restart,
            "the staged-update discard must run before app.restart(), or the \
             restart's install_pending_on_exit reinstalls the staged payload \
             over the version just installed (issue #806)"
        );
    }
}
