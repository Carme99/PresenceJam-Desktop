//! Tauri command handlers, split into per-workflow submodules.
//!
//! See issue #76. The split is purely organizational — each submodule groups
//! handlers that share state, helpers, or operational concerns. Behaviour is
//! unchanged from the pre-split single file; this is a cut-and-paste refactor.
//!
//! Submodule map (each lists every `#[tauri::command]` it owns):
//!   - `config` — `load_config`, `save_config`, `update_config`,
//!     `import_working_hours`, `export_config`, `import_config`, `set_locale`
//!   - `spotify_auth` — `start_spotify_auth`, `start_spotify_reconnect`,
//!     `reconnect_spotify_session`, `complete_spotify_auth_manual`,
//!     `refresh_spotify`, `is_spotify_client_secret_set`
//!   - `playback` — `set_volume`, `seek`, `get_spotify_granted_scopes`
//!     (issue #770 deleted the seven callerless playback_* /
//!     get_playback_* commands; the tray and the global hotkeys call
//!     `playback::player_with_refresh_typed` / `player_with_refresh`
//!     directly, with no IPC hop)
//!   - `teams_auth` — `start_teams_auth_device_code`, `poll_teams_auth`,
//!     `cancel_teams_auth_poll`, `refresh_teams`, `get_teams_granted_scopes`
//!   - `sync` — `start_syncing`, `stop_syncing`, `get_sync_status`,
//!     `refresh_status`, `app_exit`
//!   - `window` — `show_window`, `set_autostart_enabled`, `open_logs_folder`,
//!     `open_external_url`
//!   - `onboarding` — `is_onboarding_complete`, `complete_onboarding`,
//!     `reconnect_spotify`, `reconnect_teams`
//!   - `misc` — `preview_status`, `update_tray_menu_state`, `relaunch_app`,
//!     `reset_local_token_storage`
//!   - `logs` — `get_recent_logs` (LogViewer history backfill, issue #595)
//!   - `shortcuts` — `register_shortcuts`, `unregister_shortcuts`,
//!     `validate_shortcut` (global hotkeys, issue #676)
//!   - `status` — `set_manual_status`, `clear_manual_status_command`,
//!     `load_manual_status_command` (issue #870, user-composed Teams status
//!     with an expiry, mirrored by a Dashboard composer, a tray "Recent
//!     statuses" submenu, and a `--set-status` CLI flag)
//!   - `rules` — `explain_rules` (issue #868, track-rule dry-run tester; a
//!     pure projection that runs the same walker the live `process_track`
//!     path uses, against a Settings-typed synthetic track)
//!   - `updater_bg` (in crate root, not `commands/`) —
//!     `check_for_update`, `stage_deferred_update`,
//!     `clear_failed_update_install`, `cancel_deferred_update`
//!   - `diagnostics` (in crate root) — `get_diagnostics_snapshot`,
//!     `save_diagnostics_snapshot`
//!   - `history` (in crate root) — `get_presence_history`
//!   - `lib.rs` (crate root) — `detach_pane`

pub mod config;
pub mod logs;
pub mod misc;
pub mod onboarding;
pub mod playback;
pub mod rules;
pub mod shortcut_reason;
pub mod shortcuts;
pub mod spotify_auth;
pub mod status;
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

/// Issue #485 caller-location matrix: which commands are guarded by the
/// main-window guard, which are intentionally unguarded, and why.
/// Commands without a `tauri::Window` param cannot call the guard (it
/// needs the caller label); their main-only status is justified by caller
/// location instead -- every frontend call site lives in a main-window-only
/// view (Dashboard, +page, UpdatePrompt, Diagnostics-as-main-route).
/// Detached windows (`logs-detached` / `settings-detached`) host only
/// Settings + LogViewer, whose invokes are the allowlist below.
/// GUARDED (take `window` and reject non-main first):
/// `start_syncing`, `stop_syncing`, `app_exit`, `refresh_status` (sync.rs),
/// `start_spotify_auth`, `start_spotify_reconnect`, `reconnect_spotify_session`
/// (issue #771: the Settings Spotify reconnect card calls it, and Settings
/// is a detached-hosting view, so caller location cannot justify it --
/// `complete_spotify_auth_manual`, `refresh_spotify` (spotify_auth.rs),
/// `start_teams_auth_device_code`, `refresh_teams` (teams_auth.rs),
/// `complete_onboarding` (onboarding.rs), `relaunch_app`,
/// `reset_local_token_storage` (misc.rs), `set_volume`, `seek` (playback.rs,
/// issue #871: the Dashboard slider/progress bar and the tray Volume/Seek
/// submenus all run in the main window, and the tray path must not be
/// reachable from a popped-out pane), `set_manual_status`,
/// `clear_manual_status_command` (status.rs, issue #870: the Dashboard
/// composer and the tray "Set/Clear manual status" submenu handlers run in
/// the main window only), `stage_deferred_update` (updater_bg.rs).
///
/// MAIN-ONLY BY CALLER LOCATION (no `window` param, so the guard cannot
/// run; every `invoke()` call site lives in a main-window-only view):
/// `show_window` (+page main route), `update_tray_menu_state` (Dashboard),
/// `get_diagnostics_snapshot` (Diagnostics-as-main-route),
/// `save_diagnostics_snapshot` (issue #771: Diagnostics `saveToFile()` in
/// `src/lib/components/Diagnostics.svelte`, a main-route-only view --
/// read-only `get_diagnostics_snapshot` shares the same route),
/// `preview_status` (Settings preview but read-only pure computation),
/// `get_sync_status` (Dashboard/Settings status read),
/// `get_spotify_granted_scopes`/`get_teams_granted_scopes` (Settings scope
/// readers, no side effect), `clear_failed_update_install` (Diagnostics
/// dismiss; deletes only the marker file), `is_onboarding_complete`
/// (issue #770: the boot gate in `src/routes/+page.svelte`, a main-window
/// route; Onboarding.svelte's remount probe runs in the same main-window
/// view), `check_for_update` + `cancel_deferred_update` (issue #771:
/// UpdatePrompt is mounted only under `{#if isMainWindow}` in
/// `src/routes/+layout.svelte`, the sole `invoke()` call sites for both;
/// neither takes a `window` param, so the guard cannot run -- a detached
/// pane has no UpdatePrompt instance and therefore never invokes them;
/// adding a `window` param + guard is the defence-in-depth follow-up
/// if a second call site ever appears), `get_presence_history` (Dashboard
/// "Activity" card, `src/lib/components/Dashboard.svelte`),
/// `load_manual_status_command` (Dashboard composer re-fetch,
/// `src/lib/components/Dashboard.svelte`), `explain_rules` (Dashboard +
/// Settings dry-run testers, `src/lib/components/Dashboard.svelte` +
/// `src/lib/components/Settings.svelte` -- Settings is a detached-hosting
/// view, but this command is a pure projection over a synthetic track with
/// no keychain/token/config/process side effect, so caller reachability
/// from a popped-out Settings pane is safe), `import_working_hours`
/// (Settings quiet-hours import preview,
/// `src/lib/components/Settings.svelte` -- same pure-preview rationale as
/// `explain_rules`: it GETs Graph working hours and returns a preview, and
/// only `update_config` persists), `detach_pane` (issue #771:
/// the `popOut()` helper in `src/lib/stores/detach.ts`, called from the
/// main window's Settings/LogViewer "Pop out" buttons; creating a
/// same-label window twice is a Tauri-level no-op focus, so a detached
/// caller gains nothing). `reset_local_token_storage` is NOT in this list:
/// it deletes the keychain key plus tokens.json and its sidecars, so it
/// takes `window` and is guarded (issue #766).
///
/// INTENTIONALLY UNGUARDED -- detached-legit (invoked from popped-out
/// Settings/LogViewer by design): `reconnect_teams` (the Settings Spotify
/// reconnect card's Teams counterpart, `src/lib/components/Settings.svelte`
/// -- same detached-hosting view as the guarded `reconnect_spotify_session`,
/// but it carries no `window` param, so the guard cannot run; it only
/// clears the Teams token state and re-opens the device-code flow),
/// `poll_teams_auth` (device-code poll from `src/lib/stores/authFlow.svelte.ts`,
/// used by the main-window Onboarding/DeviceCodeBox/Reconnect views AND the
/// detached Settings device-code flow -- same dual-window rationale as
/// `open_external_url` below), `cancel_teams_auth_poll` (issue #771: the
/// abort arm of the same device-code flow -- `resetTeamsAuthFlow()` in
/// `src/lib/stores/authFlow.svelte.ts` invokes it unconditionally, and the
/// detached-hosting Settings view calls that reset (`Settings.svelte:1461`),
/// so a popped-out Settings pane aborts through the same path),
/// `set_autostart_enabled` (Settings autostart
/// toggle, `src/lib/components/Settings.svelte` -- a detached-hosting view),
/// `open_logs_folder` (Settings + LogViewer, `Settings.svelte` +
/// `LogViewer.svelte` -- the Logs pane is hosted in either window),
/// `open_external_url` (Teams verification-URL open during detached
/// device-code flow, `Onboarding.svelte` + `Reconnect.svelte` +
/// `src/routes/+layout.svelte`), `load_config` (issue #771: `loadConfig()`
/// in `src/lib/stores/config.ts`, called from Settings, Dashboard,
/// Onboarding and the boot probe -- Settings is a detached-hosting view,
/// so a popped-out Settings pane re-reads through the same path),
/// `save_config` (whole-document config write) and `update_config`
/// (field-level config write, issue #535) — both are reached from
/// Settings, which is one of the two detached-hosting views, as are
/// `export_config` (reads the config, writes a user-chosen file) and
/// `import_config` (replaces the config from a user-chosen file) —
/// issue #673; `get_recent_logs` (LogViewer history backfill, issue #595)
/// — the Logs pane is hosted in either window, and the file it tails is
/// the same local file `open_logs_folder` already exposes to both,
/// unredacted there and here alike (only the paste-able snapshot is
/// redacted, #434); `set_locale` (issue #771: the language picker lives in
/// Settings via `src/lib/i18n/store.svelte.ts`, one of the two
/// detached-hosting views, so a popped-out Settings pane must be able to
/// switch language); `register_shortcuts`, `unregister_shortcuts`,
/// `validate_shortcut` (issue #676: the Settings hotkey card lives in a
/// detached-hosting view, and the pane that captures a combo must also be
/// able to (re)register it -- the commands act on the persisted config and
/// this process's own OS grabs only).
///
/// REGISTERED BUT CALLERLESS (issue #771: REGISTERED in `generate_handler!`
/// today, so the test above requires a matrix entry -- but with zero
/// `invoke()` call sites in `src/` or `tests/` (only comments plus tests
/// asserting their absence and one browser-spec mock), so neither is
/// "main-only by caller location" nor "detached-legit". Both are pending
/// deletion, the same treatment the seven playback wrappers got in #770):
/// `is_spotify_client_secret_set` (read-only keychain-presence bool with no
/// error channel -- a locked keyring reads as `false`, which is why #560
/// replaced it with the config-carried tri-state; Reconnect.svelte and the
/// boot gate document the bypass) and `reconnect_spotify`
/// (`src-tauri/src/commands/onboarding.rs` -- superseded by the guarded
/// `reconnect_spotify_session` (#554); the Settings Spotify card calls the
/// successor, pinned by `tests/settings.test.ts`, which asserts the legacy
/// name is never invoked. Note it clears tokens + deletes the keychain
/// secret, so it must NOT be blessed as intended detached surface -- when
/// its deletion lands, drop both entries here).
#[cfg(test)]
mod tests {
    /// Regression guard for issue #76: the `commands` module must declare
    /// every per-workflow submodule. If a contributor deletes one (or renames
    /// the module without updating this list), `cargo test` fails fast.
    /// `logs` joined the list with the #595 LogViewer backfill, and
    /// `shortcut_reason` with the #968 machine-readable Settings reasons.
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
            "shortcut_reason",
            "status",
            "rules",
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
    /// Scope: this grep-style scan covers the three files its own wave
    /// converted (`sync.rs`, `window.rs`, `misc.rs`). It is **not** the
    /// coverage for the rest of the tree, and deliberately so — source-text
    /// guards are rejected in this repo (issue #778, PR #1116), and widening
    /// this one to every command file would also misfire on bodies that
    /// legitimately offload inside a `*_core` seam (`poll_teams_auth` is the
    /// live example: it delegates to `poll_teams_auth_core`, so no
    /// `spawn_blocking` literal appears in the command body).
    ///
    /// The commands this slice owns are covered **behaviourally** instead,
    /// each by a test that drives the command's own offloaded entry point and
    /// asserts the recorded thread id differs from the awaiting one:
    /// `commands/config.rs::load_config` (`load_config_reads_the_document_off_the_calling_thread`),
    /// `commands/spotify_auth.rs::{refresh_spotify, start_spotify_reconnect,
    /// reconnect_spotify_session, is_spotify_client_secret_set}` and
    /// `commands/teams_auth.rs::refresh_teams`. A thread id is observable at
    /// runtime; a grep over a signature is not evidence the body moved.
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

    /// Issue #771: the caller-location matrix above must match the
    /// registered command surface. The handler list in `lib.rs`'s
    /// `generate_handler![...]` is the registered set; the backticked names
    /// inside the matrix comment block (the `GUARDED` / `MAIN-ONLY` /
    /// `INTENTIONALLY UNGUARDED` doc lines above this module) are the
    /// documented set. A command registered without a matrix entry, or
    /// listed in the matrix without being registered, fails this test.
    ///
    /// Both scans anchor from a marker (`generate_handler![` for the handler
    /// list, the matrix block between its `GUARDED` marker and the
    /// `#[cfg(test)]` line that closes the matrix comment for the matrix),
    /// in the style of the `commands_in` source guards above -- never
    /// boundary anchors. The matrix block lists every command backticked,
    /// so the scan keeps every backticked token there verbatim --
    /// including `seek`, which has no underscore and would be lost to any
    /// underscore filter (the only prose exclusion is the lone `window`).
    #[test]
    fn test_guard_matrix_covers_every_registered_command() {
        // Registered set: brace-count the `generate_handler![...]` list
        // out of lib.rs, then take the command name after the last `::`
        // (`commands::config::load_config` -> `load_config`; `detach_pane`
        // has no module prefix and is kept whole).
        fn registered_commands(lib: &str) -> Vec<String> {
            let anchor = "generate_handler![";
            let start = lib
                .find(anchor)
                .unwrap_or_else(|| panic!("lib.rs must contain `{anchor}` (issue #771)"));
            let after = &lib[start + anchor.len()..];
            let mut depth = 1usize;
            let mut end = None;
            for (i, ch) in after.char_indices() {
                match ch {
                    '[' => depth += 1,
                    ']' => {
                        depth -= 1;
                        if depth == 0 {
                            end = Some(i);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let list = &after[..end.unwrap_or_else(|| {
                panic!("generate_handler![...] must be bracket-balanced (issue #771)")
            })];
            list.split(',')
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .map(|entry| {
                    entry
                        .rsplit("::")
                        .next()
                        .unwrap_or_else(|| panic!("handler entry `{entry}` must name a command"))
                        .trim()
                        .to_string()
                })
                .collect()
        }

        // Documented set: backticked tokens inside the matrix comment block
        // only. The block starts at the `GUARDED` marker and ends at the
        // `#[cfg(test)]` line that closes the matrix comment; that span holds
        // all four command lists (guarded, main-only, detached-legit,
        // callerless) and nothing else backticked except prose quotes
        // (`invoke()`, file paths, `{#if isMainWindow}`, `generate_handler!`),
        // which the token filter below already drops (spaces, slashes, dots,
        // parens, braces, bangs), plus the lone prose `window` (the
        // `tauri::Window` param), which is
        // excluded by name -- every command name but `seek` carries an
        // underscore. A non-command backtick added to the block in command
        // shape would fail loudly on the stale side instead of passing
        // silently.
        fn matrix_commands(source: &str) -> Vec<String> {
            let start_marker = "GUARDED (take `window`";
            let end_marker = "#[cfg(test)]";
            let start = source.find(start_marker).unwrap_or_else(|| {
                panic!("commands/mod.rs must contain the guard-matrix GUARDED marker (issue #771)")
            });
            let after = &source[start..];
            let end_rel = after.find(end_marker).unwrap_or_else(|| {
                panic!("commands/mod.rs must contain the guard-matrix end marker (issue #771)")
            });
            let block = &after[..end_rel];
            let mut found = Vec::new();
            let mut rest = block;
            while let Some(open) = rest.find('`') {
                let after_open = &rest[open + 1..];
                let Some(close) = after_open.find('`') else {
                    break;
                };
                let token = after_open[..close].trim().to_string();
                // `window` is prose (the `tauri::Window` param), not a
                // command; every command name is multi-word except `seek`,
                // so single-word tokens other than `seek` are prose.
                if (token == "seek" || token.contains('_'))
                    && !token.contains(' ')
                    && !token.contains('/')
                    && !token.contains('.')
                    && !token.contains('(')
                    && !token.contains('!')
                    && !token.contains('{')
                    && token != "window"
                {
                    found.push(token);
                }
                rest = &after_open[close + 1..];
            }
            found.sort();
            found.dedup();
            found
        }

        let registered = registered_commands(include_str!("../app.rs"));
        let matrix = matrix_commands(include_str!("mod.rs"));

        // Scanner sanity: the parse must really walk the handler list
        // rather than finding nothing, and the exact registered count is
        // pinned so a quiet add/remove cannot slip past review.
        assert_eq!(
            registered.len(),
            54,
            "generate_handler! must register exactly 54 commands -- if this \
             changed, update the matrix, the submodule map, and this count \
             together (issue #771)"
        );
        for probe in ["load_config", "detach_pane", "explain_rules", "seek"] {
            assert!(
                registered.contains(&probe.to_string()),
                "the handler scan must see `{probe}` (issue #771)"
            );
        }

        let mut missing: Vec<&String> = registered
            .iter()
            .filter(|name| !matrix.contains(name))
            .collect();
        missing.sort();
        assert!(
            missing.is_empty(),
            "commands registered in lib.rs but missing from the guard matrix \
             in commands/mod.rs: {missing:?} -- add each with its \
             guarded/main-only/detached-legit justification (issue #771)"
        );

        let mut stale: Vec<&String> = matrix
            .iter()
            .filter(|name| !registered.contains(name))
            .collect();
        stale.sort();
        assert!(
            stale.is_empty(),
            "commands listed in the guard matrix but not registered in \
             lib.rs generate_handler!: {stale:?} -- drop the stale entry \
             (issue #771)"
        );
    }
}
