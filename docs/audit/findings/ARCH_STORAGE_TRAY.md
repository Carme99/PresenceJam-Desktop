# Findings — ARCH / STORAGE / TRAY docs-grounding audit

HEAD verified: 09341ecaad732e78454a2c65b383f0dfd1541d5a. Tree clean except untracked
`docs/audit/` (the audit's own working set) and `docs/__pycache__/`.
`python3 docs/link-audit.py` exits 1 **only** on the untracked `docs/audit/claims/*.md`
files (138 of 138 broken links); zero broken links in the shipped tree.
`cargo check` exits 0. `npm run check` fails (orchestrator-supplied; not re-run).
Vendor docs fetched 2026-10-08: v2.tauri.app/security/runtime-authority/,
v2.tauri.app/plugin/updater/.

## VERDICTS
STORAGE-C001 | CONFIRMED | docs/architecture/storage-and-config.md:1-198 | The page really does cover config.json/tokens.json locations, atomic writes, config integrity, startup loading, reconnect and process state, and nothing else. | -
STORAGE-C002 | CONFIRMED | docs/architecture/storage-and-config.md:5 → ../../ARCHITECTURE.md; ARCHITECTURE.md:14 indexes the page | The relative link resolves from docs/architecture/ to the root index, which links back to this page. | -
STORAGE-C003 | DRIFT | docs/architecture/storage-and-config.md:7; src-tauri/src/config/io.rs:324-384 | The heading exists verbatim, but the "(v4.6)" label is stale: the section documents post-4.6 config-integrity work (#926 per-field fallback, #916 secret stripping, #938 newer-document refusal, #943 revision, #802, #758 slice 2). | P2
STORAGE-C004 | DRIFT | src-tauri/src/config/io.rs:324-384 | "Three layers" undercounts: `field_or_fallback`/`config_from_sections` (issue #926) is a fourth layer that keeps one bad section from destroying the rest of a partial config. | P2
STORAGE-C005 | CONFIRMED | src-tauri/src/config/patch.rs:280-285 | `apply_patch` overwrites only the fields a `ConfigPatch` explicitly names and leaves everything else, including `extra` and `schema_version`, exactly as it was. | -
STORAGE-C006 | CONFIRMED | src-tauri/src/config/patch.rs:281-282; config/mod.rs:1125-1137 | `extra` is never touched by a patch (pinned by `test_apply_patch_leaves_extra_untouched`); `schema_version` is stamped binary-side at save time. | -
STORAGE-C007 | CONFIRMED | src-tauri/src/config/patch.rs:421-427 | A named list (`quiet_hours` / `track_rules`) is assigned wholesale; `test_apply_patch_replaces_a_named_list` pins it. | -
STORAGE-C008 | CONFIRMED | src-tauri/src/commands/config.rs:137-176 (update_config → apply_patch); commands/config.rs:65-121 (save_config whole-document) | The patch API exists precisely because `save_config` is a whole-document replace. | -
STORAGE-C009 | CONFIRMED | src/lib/components/Onboarding.svelte:387-418; src/lib/stores/config.ts:458-479 | `mergeWizardConfig` clones the stored config and writes only the wizard fields, then `saveConfig` sends the merged document. | -
STORAGE-C010 | OVERSTATED | src/lib/stores/config.ts:463-477 | The wizard writes seven keys, not four: client_id, client_secret_set, client_secret_state, status_format, start_minimized, default_interval_seconds, autostart. | P3
STORAGE-C011 | CONFIRMED | src/lib/stores/config.ts:463 | `spotify.client_id` is written from the wizard's `spotify_client_id`. | -
STORAGE-C012 | CONFIRMED | src/lib/stores/config.ts:472 | `teams.status_format` is written from the wizard's `status_format`. | -
STORAGE-C013 | CONFIRMED | src/lib/stores/config.ts:476 | `polling.default_interval_seconds` is written from the wizard's interval. | -
STORAGE-C014 | CONFIRMED | src/lib/stores/config.ts:477 | `autostart` is written from the wizard's launch-at-login toggle. | -
STORAGE-C015 | CONFIRMED | src/lib/components/Onboarding.svelte:422 | The merged result is sent over `saveConfig`. | -
STORAGE-C016 | CONFIRMED | src/lib/stores/config.ts:462,448-452 | `status_rules` survives verbatim via `structuredClone(stored)` and is never named by the wizard. | -
STORAGE-C017 | CONFIRMED | src/lib/stores/config.ts:448-452 | `logging` is carried over verbatim and never written by the wizard. | -
STORAGE-C018 | CONFIRMED | src/lib/stores/config.ts:448-452 | `profanity_*` is carried over verbatim and never written by the wizard. | -
STORAGE-C019 | CONFIRMED | src/lib/stores/config.ts:448-452 | `presence_gate` is carried over verbatim and never written by the wizard. | -
STORAGE-C020 | CONFIRMED | src/lib/stores/config.ts:448-452 | `availability_sync` is carried over verbatim and never written by the wizard. | -
STORAGE-C021 | CONFIRMED | src/lib/stores/config.ts:462 | `extra` survives verbatim inside the `structuredClone`. | -
STORAGE-C022 | CONFIRMED | src/lib/components/Onboarding.svelte:394-408 | The read is a deliberate `invoke('load_config')`, and a failure aborts the save rather than falling back to `defaultConfig`. | -
STORAGE-C023 | CONFIRMED | src-tauri/src/config/io.rs:69-73,250-273; config/mod.rs:470-488 | A config.json that fails to parse is renamed beside itself to the fixed name `config.json.bak`, never timestamped. | -
STORAGE-C024 | CONFIRMED | src-tauri/src/config/io.rs:257-262 | The warning reads `[CFG] corrupt config '…' quarantined to '…': … — loading defaults`. | -
STORAGE-C025 | CONFIRMED | src-tauri/src/config/io.rs:271; state.rs:749,903 | `quarantine_corrupt_config` sets the flag on the per-`AppState` `AppCaches` (issue #758 slice 2), and `config_was_quarantined(&caches)` reads it. | -
STORAGE-C026 | CONFIRMED | src-tauri/src/config/io.rs:484,491 | Both the non-object-root and the parse-error arm return `Ok(AppConfig::default())`, so the app boots on defaults. | -
STORAGE-C027 | CONFIRMED | src-tauri/src/config/io.rs:250-273; config/mod.rs:510-548 | The rename's failure is logged and swallowed, the flag is raised either way, and `test_corrupt_config_quarantine_rename_failure_preserves_original` pins the un-truncated original. | -
STORAGE-C028 | CONFIRMED | src-tauri/src/config/io.rs:516; config/migrate.rs:43-57 | A schema-version mismatch runs `migrate_config` in place (never a quarantine), and a newer-than-this-build version is passed through untouched. | -
STORAGE-C029 | CONFIRMED | src-tauri/src/config/io.rs:824-835,864-866 | `save_config_to` rejects a whole-document payload whose `revision` is behind the file's, then raises `revision` monotonically per accepted save. | -
STORAGE-C030 | CONFIRMED | src-tauri/src/config/schema.rs:1129-1149 | `AppConfig` carries `pub revision: u64`, defaulting to 0 and read by `stored_document_markers`. | -
STORAGE-C031 | DRIFT | src-tauri/src/config/io.rs:768-785; commands/config.rs:419-452; grep of src/ + tests/ for `config-changed` = 0 hits | The constant lives in `config/io.rs` (not `config.rs`), and `emit_config_changed` has NO call site: `after_persist` never emits it and no webview subscribes, so "emits it after every accepted save … and the webview subscribes and reloads" is false. | P1
STORAGE-C032 | CONFIRMED | grep src-tauri/ for flock/file_lock/try_lock_exclusive/fs2 = 0 hits; no "JavaScript-safe terminal boundary" concept anywhere in the tree | No cross-process sidecar lock and no such terminal boundary exist at this checkout. | -
STORAGE-C033 | CONFIRMED | src-tauri/src/cli.rs:41-43,279-295,413-457 | `--profile` is an alternative to launching the GUI (its own `AppState`), writes `config.json` directly with `fs::write`, and never publishes a live change to a running GUI. | -
STORAGE-C034 | CONFIRMED | src-tauri/src/diagnostics.rs:153,196-202 | `ConfigSummary` carries both `config_quarantined: bool` and `config_quarantine_backup: Option<String>`. | -
STORAGE-C035 | CONFIRMED | src-tauri/src/diagnostics.rs:309-324,741-791 | `ConfigQuarantine::observe(&state.caches)` reads the flag plus the bare backup name and injects them at the command boundary, exactly the same injection shape as the #603 failed-install marker. | -
STORAGE-C036 | CONFIRMED | src/lib/components/Diagnostics.svelte:45-52,211-218 | The banner renders from `config_quarantined || quarantineBackup !== null` — either fact — and is dismissible. | -
STORAGE-C037 | CONFIRMED | src/lib/components/Diagnostics.svelte:45-52 | The two-fact trigger exists precisely because the per-process flag vanishes on restart while the `.bak` outlives the launch that produced it. | -
STORAGE-C038 | CONFIRMED | src-tauri/src/diagnostics.rs:628-634,2397-2425 | `quarantine_backup_field` reduces an absolute path to its last component and drops any value that still carries a separator. | -
STORAGE-C039 | CONFIRMED | src/lib/i18n/en.ts:434-435 | The location copy is "Both files live in the PresenceJam folder inside your user configuration folder" — a folder name, not an absolute path. | -
STORAGE-C040 | CONFIRMED | src/lib/components/Diagnostics.svelte:35,218 | Dismiss only sets `quarantineDismissed = true`; it invokes nothing and cannot delete the backup. | -
STORAGE-C041 | CONFIRMED | src-tauri/src/keychain.rs:546-564 | `KeychainPresence` splits `Present` / `Absent` / `Unavailable(String)` with the help text. | -
STORAGE-C042 | CONFIRMED | src-tauri/src/keychain.rs:266-268,566-582 | `NoEntry` is the only error classified `Absent`; `PlatformFailure`/`NoStorageAccess` become `Unavailable` with platform help text. | -
STORAGE-C043 | CONFIRMED | src-tauri/src/config/schema.rs:24-46; config/io.rs:596-611; config/mod.rs:169-184 | The tri-state rides IPC as `ClientSecretState` with lowercase `"present"`/`"absent"`/`"unavailable"` on the wire, stamped by `with_keychain_flags`. | -
STORAGE-C044 | CONFIRMED | src-tauri/src/commands/onboarding.rs:264,306-326 | `record_client_secret_state` mirrors the boot gate's own probe back into the in-memory config so a later save cannot return a stale state. | -
STORAGE-C045 | CONFIRMED | src-tauri/src/commands/onboarding.rs:375-386,165-178 | An `Unavailable` keychain maps to `RefreshFailure::Transient` (session kept); only a positively `Absent` entry maps to `Unavailable` → `ReauthRequired`. | -
STORAGE-C046 | CONFIRMED | src-tauri/src/app.rs:489-634 | `setup_state` / `setup_keychain_cache` / `setup_config` / `setup_secret_migration` / `setup_tokens` load persisted config and tokens into `AppState` on launch. | -
STORAGE-C047 | CONFIRMED | src-tauri/src/app.rs:491-492 | `app.manage(state.clone())` runs after `AppState::new()`. | -
STORAGE-C048 | CONFIRMED | src-tauri/src/app.rs:512-518 | `setup_keychain_cache` primes the Spotify client_secret cache at startup. | -
STORAGE-C049 | CONFIRMED | src-tauri/src/app.rs:518-526 | Both outcomes are logged: secret present, or keychain access failed/empty. | -
STORAGE-C050 | CONFIRMED | src-tauri/src/app.rs:533 | `config::load_config(&state.caches)` is the startup load. | -
STORAGE-C051 | DRIFT | src-tauri/src/config/io.rs:449-455,433-439 | A missing config.json returns `Ok(AppConfig::default())`; `Err` only happens when the config directory cannot be resolved, so "Err (first-launch path)" is wrong. | P3
STORAGE-C052 | DRIFT | src-tauri/src/state.rs:228-271; app.rs:540-541 | `Config` has no `set` method — startup installs the config with `*config_guard = Some(Arc::new(cfg))`, which is the direct-field shape this same doc forbids. | P2
STORAGE-C053 | DRIFT | src-tauri/src/token_io.rs:261-264; app.rs:624-632 | `read_tokens_at` takes `&tauri::AppHandle`, not a directory path; the path-taking variant is the separate `read_tokens_at_path`. | P2
STORAGE-C054 | CONFIRMED | src-tauri/src/token_io.rs:53-58 | `TokensFile { spotify_tokens, teams_tokens }` is exactly the returned shape. | -
STORAGE-C055 | DRIFT | src-tauri/src/state.rs:90-93,657-658,959 | `Tokens.spotify` is a private RwLock; the install goes through `TokensLoadGate::install_loaded` → `*tokens.spotify_mut()`, so the diagram's direct assignment is not valid Rust. | P3
STORAGE-C056 | DRIFT | src-tauri/src/state.rs:90-93,657-658,959 | Same defect as C055 for the Teams slot. | P3
STORAGE-C057 | CONFIRMED | src-tauri/src/app.rs:493-501; deep_link.rs | The deep-link handler resolves callbacks against managed state, replayed right after `manage()`. | -
STORAGE-C058 | CONFIRMED | src-tauri/src/app.rs:955-969 | First launch finds no `spotify_tokens`/`teams_tokens` in tokens.json and logs it. | -
STORAGE-C059 | CONFIRMED | src-tauri/src/app.rs:611-619; grep src-tauri/ for plugin-store = 0 hits | Tokens are read straight from the file, bypassing every plugin store (issue #65). | -
STORAGE-C060 | CONFIRMED | src-tauri/src/commands/spotify_auth.rs:343-357; app.rs:621-623 | Pending auth (PKCE verifier, device code) is `AppState`-only and never persisted, so a mid-OAuth crash forces a restart of the flow. | -
STORAGE-C061 | CONFIRMED | src-tauri/src/commands/spotify_auth.rs:343-346 | The comment pins the PKCE verifier as a 10-minute bearer credential held in memory only. | -
STORAGE-C062 | CONFIRMED | src-tauri/src/commands/onboarding.rs:562-577 | Reconnect clears the token slot in memory and persists the cleared file, forcing re-auth. | -
STORAGE-C063 | CONFIRMED | src/lib/utils/boot.ts:19-24; commands/onboarding.rs:41-85 | `is_onboarding_complete` is the routing decision behind dashboard vs onboarding (and reconnect). | -
STORAGE-C064 | CONFIRMED | src-tauri/src/commands/onboarding.rs:165-178,251-303 | A locally-expired token is refreshed under the CAS guard and persisted; only `invalid_grant`/`Unavailable` report re-auth. | -
STORAGE-C065 | CONFIRMED | src/lib/utils/boot.ts:8-24 | `bootView` routes an incomplete verdict with stored Spotify credentials to `reconnect`, not `onboarding`. | -
STORAGE-C066 | CONFIRMED | src-tauri/src/commands/onboarding.rs:241-242,351-359 | The gate deliberately never asks a credentialed user to re-enter credentials. | -
STORAGE-C067 | CONFIRMED | src-tauri/src/commands/onboarding.rs:547-558 | Clicking Reconnect clears auth state for one provider and triggers re-authentication. | -
STORAGE-C068 | CONFIRMED | src/lib/components/Settings.svelte:541-549 | Settings is the UI that invokes the Spotify reconnect path. | -
STORAGE-C069 | CONFIRMED | src-tauri/src/commands/onboarding.rs:548 | `reconnect_spotify` is a registered `#[tauri::command]`. | -
STORAGE-C070 | DRIFT | src-tauri/src/commands/onboarding.rs:564,621 | The actual clear is `state.tokens_load.clear_spotify(&state.tokens)` / `clear_teams(...)` (issue #935), not a direct `tokens.spotify = None`. | P3
STORAGE-C071 | CONFIRMED | src-tauri/src/commands/onboarding.rs:572,625 | `token_io::persist_tokens(state, app)` rewrites tokens.json after the clear. | -
STORAGE-C072 | CONFIRMED | src-tauri/src/token_io.rs:1-11,602-613; config/io.rs:612-682 | tokens.json is written with the same temp-file + fsync + rename atomic pattern as the config writer. | -
STORAGE-C073 | CONFIRMED | src-tauri/src/commands/onboarding.rs:580,633 | `onboarding_cache.invalidate()` runs on both reconnect paths. | -
STORAGE-C074 | CONFIRMED | src-tauri/src/commands/onboarding.rs:594 | `app.emit("spotify-reconnect-required", ())` is emitted after the clear. | -
STORAGE-C075 | CONFIRMED | src/routes/+layout.svelte:346-367; Settings.svelte:541-549 | The re-auth wizard reads the existing client_secret from the keychain before re-authorising. | -
STORAGE-C076 | CONFIRMED | src-tauri/src/commands/onboarding.rs:562-598 | `reconnect_spotify` clears in-memory + on-disk tokens and emits `spotify-reconnect-required`. | -
STORAGE-C077 | CONFIRMED | src-tauri/src/commands/onboarding.rs:642-647 | `reconnect_teams` clears tokens and emits `teams-reconnect-required` with `user_initiated: true`. | -
STORAGE-C078 | CONFIRMED | src-tauri/src/commands/onboarding.rs:636-641 | The comment states this is the only user-initiated emitter, tied to #675. | -
STORAGE-C079 | CONFIRMED | src-tauri/src/state.rs:829-904 | `AppState` is the struct-of-states from the #80 refactor. | -
STORAGE-C080 | CONFIRMED | src-tauri/src/state.rs:38-93,224-298,1281-1300 | Every sub-struct field is private; access is method-only, and `test_app_state_sub_encapsulation_no_pub_inner_fields` guards it. | -
STORAGE-C081 | DRIFT | src-tauri/src/state.rs:829-904 | The code block lists only 5 of AppState's 15 fields; `calendar`, `launch_binding`, `tray_available`, `deep_link_seen`, `tokens_load`, `secret_conflict`, `last_sync_snapshot`, `session`, `locale` and `caches` are all missing. | P2
STORAGE-C082 | CONFIRMED | src-tauri/src/state.rs:830,90-93 | `pub tokens: Tokens` holds the two independent RwLocks. | -
STORAGE-C083 | CONFIRMED | src-tauri/src/state.rs:831,288-298 | `pub polling: Polling` holds `is_syncing`, `handle`, `stop_tx` (plus `current_track`, `thread_id`). | -
STORAGE-C084 | CONFIRMED | src-tauri/src/state.rs:832,193 | `pub pending: PendingAuths` holds the two pending-auth slots. | -
STORAGE-C085 | CONFIRMED | src-tauri/src/state.rs:833,224-226 | `pub config: Config` holds the `RwLock<Option<Arc<AppConfig>>>`. | -
STORAGE-C086 | CONFIRMED | src-tauri/src/state.rs:834,38-40 | `pub onboarding_cache: OnboardingCache` is the 30 s cache sub-struct. | -
STORAGE-C087 | DRIFT | src-tauri/src/state.rs:42-63,113-117,224-271,335-335 | `config.set()` does not exist — the write path is `get_mut()`; the other four named methods do exist verbatim. | P2
STORAGE-C088 | CONFIRMED | src-tauri/src/state.rs:31-37,49-51 | The `lock_async`-could-replace-`lock` rationale is documented on `OnboardingCache` and is the reason for the `lock()` method. | -
STORAGE-C089 | CONFIRMED | src-tauri/src/commands/sync.rs:247; polling/state.rs:779 | `is_syncing` is claimed only by `try_claim()`, and `test_start_polling_does_not_claim_is_syncing` guards the rule. | -
STORAGE-C090 | CONFIRMED | src-tauri/src/state.rs:314-316 | The poller reads the flag with `Ordering::Acquire`, and the loop exits when it is false. | -
STORAGE-C091 | CONFIRMED | src-tauri/src/polling/state.rs:412,428 | Both the panic-guard path and the spawn-error `map_err` call `set_syncing(false)` so future claims never wedge. | -
STORAGE-C092 | CONFIRMED | src-tauri/src/polling/state.rs:330-334,779 | The claimed hazard is pinned by an explicit regression guard asserting the polling module stays CAS-free. | -
STORAGE-C093 | CONFIRMED | src-tauri/src/state.rs:1281-1300 | `test_app_state_sub_encapsulation_no_pub_inner_fields` panics if any of Tokens/Polling/PendingAuths/Config re-exposes a `pub` field. | -
STORAGE-C094 | CONFIRMED | src-tauri/src/polling/refresh.rs:50-66,248-289 | All three refresh paths route through `cas_refresh_or_discard_unchecked`, the shared CAS guard. | -
STORAGE-C095 | CONFIRMED | src-tauri/src/polling/refresh.rs:50-66; state.rs:662-681 | The guard re-reads under the write lock and commits only when `access_token` matches the pre-refresh snapshot, else discards. | -
STORAGE-C096 | CONFIRMED | src-tauri/src/state.rs:662-681; refresh.rs:504 | The lost-update race is the documented motivation for the commit-if-unchanged rule. | -
STORAGE-C097 | CONFIRMED | src-tauri/src/state.rs:38-40,52; commands/onboarding.rs:20,93 | `parking_lot::Mutex<Option<(Instant, bool)>>` with `ONBOARDING_CACHE_TTL = 30 s`. | -
STORAGE-C098 | CONFIRMED | src-tauri/src/commands/onboarding.rs:47-50,90-102 | `is_onboarding_complete` returns the cached verdict on a hit and only calls upstream APIs on a miss. | -
STORAGE-C099 | CONFIRMED | src-tauri/src/commands/onboarding.rs:580,633; spotify_auth.rs:129,532,987; teams_auth.rs:173 | Every token-mutating command invalidates the cache. | -
STORAGE-C100 | CONFIRMED | src-tauri/src/commands/onboarding.rs:268-273 | The boot gate's Spotify refresh runs through `cas_refresh_spotify`, the shared CAS guard (issue #530). | -
STORAGE-C101 | CONFIRMED | src/lib/stores/app.ts:1-6 | `app.ts` is a classic Svelte store module exporting `currentView` and `appError`. | -
STORAGE-C102 | CONFIRMED | src/lib/stores/app.ts:1-6 | Both are `writable(...)` from `svelte/store` — no runes anywhere in the file. | -
STORAGE-C103 | CONFIRMED | src/lib/stores/config.ts:279,504-524; grep config.ts for localStorage = 0 call sites | `configStore` is a full `AppConfig` store and `saveConfig` round-trips through the Rust atomic-write command. | -
STORAGE-C104 | CONFIRMED | src/lib/stores/authFlow.svelte.ts:3-32,24-32; utils/useAuthListeners.ts:110-133 | Per-provider `phase` + `error`, updated by `setSpotifyPhase`/`setTeamsPhase` from exactly four backend auth events. | -
STORAGE-C105 | CONFIRMED | src/lib/utils/useAuthListeners.ts:100-133 | `useAuthListeners` is the shared helper, consumed by Dashboard, Onboarding and Settings — the consolidation the claim describes. | -
STORAGE-C106 | CONFIRMED | docs/architecture/frontend.md:179,283; src/lib/types.ts | `frontend.md#directory-structure` exists and the "ts-rs generated types" entry lives inside that section. | -
TRAY-C001 | CONFIRMED | docs/architecture/tray-and-shell.md:1-233 | The page covers exactly the native surface: tray menu, detached windows, UI languages and the background auto-updater. | -
TRAY-C002 | CONFIRMED | docs/architecture/tray-and-shell.md:5 → ../../ARCHITECTURE.md; ARCHITECTURE.md:16 | The link resolves to the root index, which links back to this page. | -
TRAY-C003 | CONFIRMED | docs/architecture/tray-and-shell.md:7 | The "## Multi-Window Detach (v4.0)" heading exists verbatim. | -
TRAY-C004 | CONFIRMED | src/lib/stores/detach.ts:1-11,47-118,120-140 | Logs and Settings pop out into `WebviewWindow`s and pop back in, VS Code detached-panel style. | -
TRAY-C005 | CONFIRMED | src/lib/stores/detach.ts:96-98 | `popOutInner` invokes the `detach_pane` command. | -
TRAY-C006 | DRIFT | src-tauri/src/app.rs:71-106,122-151; src-tauri/src/lib.rs:1-33 | The fixed `DetachedPaneSpec` table and the `detach_pane` command live in `app.rs`; `lib.rs` is now a 33-line module registry. detach.ts:19 still names `lib.rs`. | P2
TRAY-C007 | CONFIRMED | src-tauri/src/app.rs:90-91; capabilities/detached.json:windows | The stable labels are `logs-detached` / `settings-detached` on both sides. | -
TRAY-C008 | CONFIRMED | src-tauri/src/app.rs:88-105 | `/detached/<pane>` URLs are built in the table, with sizes 720×520 (logs) and 620×720 (settings). | -
TRAY-C009 | CONFIRMED | src/routes/detached/[pane]/+page.svelte:1-30 | The SvelteKit route exists and is the detached entry point. | -
TRAY-C010 | CONFIRMED | src/routes/detached/[pane]/+page.svelte:21-25 | It renders `<LogViewer detached />` or `<Settings detached />` by pane param. | -
TRAY-C011 | CONFIRMED | src-tauri/tauri.conf.json app.windows | `app.windows` declares exactly one window, labelled `main`, 600×750. | -
TRAY-C012 | CONFIRMED | src/lib/stores/app.ts:5; src/routes/detached/[pane]/+page.svelte (no currentView import) | `currentView` is only imported by the main window's `+page.svelte`; the detached route never touches it. | -
TRAY-C013 | CONFIRMED | src-tauri/src/commands/mod.rs:60-65 | Detached windows share the app-global command surface, so config/polling state is shared by construction. | -
TRAY-C014 | DRIFT | src-tauri/src/commands/mod.rs:199-214; tests/settings.test.ts:306,318 | `reconnect_spotify` has zero `invoke()` call sites — Settings calls `reconnect_spotify_session` — so it is not part of the detached surface the doc describes. | P2
TRAY-C015 | CONFIRMED | src-tauri/src/commands/config.rs:30,75; teams_auth.rs:239; window.rs:198; onboarding.rs:605 | None of `save_config`, `load_config`, `poll_teams_auth`, `open_logs_folder`, `reconnect_teams` takes a `window` argument. | -
TRAY-C016 | DRIFT | src-tauri/src/** (grep require_main_window = 19 call sites: updater_bg 1, status 2, teams_auth 2, sync 4, playback 2, misc 2, onboarding 1, spotify_auth 5) | There are 19 guarded commands, not 13; the doc's count is stale. | P2
TRAY-C017 | DRIFT | src-tauri/src/commands/mod.rs:157-162,199-214 | `reconnect_teams` is unguarded, but `reconnect_spotify` is registered-but-callerless and superseded by the guarded `reconnect_spotify_session`; pairing them as both live and unguarded misleads. | P2
TRAY-C018 | CONFIRMED | src-tauri/src/commands/onboarding.rs:594,642 | Both emits are `app.emit(...)`, which broadcasts app-wide. | -
TRAY-C019 | CONFIRMED | src/routes/+layout.svelte:232 | `if (!isTauriRuntime || !isMainWindow) return;` gates the reconnect/update listener block. | -
TRAY-C020 | CONFIRMED | src/lib/components/Settings.svelte:379,509 | Popping back in re-mounts Settings, which reads backend truth through `loadConfig()`. | -
TRAY-C021 | CONFIRMED | src/lib/stores/detach.ts:31-35 | The pane→popped-out map is documented and used as main-window-only view state; the detached route never imports it. | -
TRAY-C022 | CONFIRMED | src/lib/components/Dashboard.svelte:794-805,1016-1021 | Dashboard nav calls `focusDetached(...)` instead of navigating, and marks the button `class:detached`. | -
TRAY-C023 | DRIFT | src-tauri/capabilities/detached.json:permissions | The mirrored set is `core:default`, `core:window:allow-close`, `core:event:{allow-emit,allow-emit-to,allow-listen}`, `log:default` and `notification:{default,allow-is-permission-granted,allow-request-permission,allow-notify}` — there is no `opener` grant. | P2
TRAY-C024 | CONFIRMED | src-tauri/capabilities/detached.json:permissions | `core:window:allow-close` is present and described as the one window-management permission a detached render reaches (#594). | -
TRAY-C025 | CONFIRMED | src/lib/stores/detach.ts:120-140 | `popIn` → `WebviewWindow.getByLabel` → `win.close()`. | -
TRAY-C026 | CONFIRMED | https://v2.tauri.app/security/runtime-authority/ (fetched 2026-10-08): "the runtime authority receives the invoke request, makes sure that the origin is allowed to actually use the requested command, checks if the origin is part of capabilities"; capabilities/default.json:8 and capabilities/detached.json both list `core:window:allow-close` explicitly under `core:default` | ACLs resolve against the calling webview, and `core:window:default` does not include the close permission. | -
TRAY-C027 | CONFIRMED | src/lib/stores/detach.ts:130-140 | A refused close logs, re-derives the badge from the live window set and leaves it alone. | -
TRAY-C028 | CONFIRMED | src/lib/stores/detach.ts:129-140 | The catch arm calls `reconcileDetachedPanes()` instead of clearing the badge, so a refused close no longer lies. | -
TRAY-C029 | CONFIRMED | src/lib/stores/detach.ts:120-140; src/routes/+layout.svelte:236 | `popIn` is in `detach.ts` and `reconcileDetachedPanes()` runs once at main-window boot. | -
TRAY-C030 | CONFIRMED | src-tauri/capabilities/default.json:permissions | The main capability has no `core:window:allow-create` or `core:webview:allow-create-webview-window` grant. | -
TRAY-C031 | CONFIRMED | src-tauri/src/app.rs:108-151 | Window creation is entirely Rust-side, from the fixed table; an unknown pane name is rejected. | -
TRAY-C032 | CONFIRMED | src/routes/+layout.svelte:55,232,620 | The always-mounted listeners and `UpdatePrompt` are gated behind the window-label check. | -
TRAY-C033 | CONFIRMED | src/routes/+layout.svelte:232,620 | Detached windows return before the listener block and never mount `UpdatePrompt`. | -
TRAY-C034 | CONFIRMED | src-tauri/tauri.conf.json app.windows; detach.ts:47-64 | Detachment is user-initiated per pane and the app boots single-window. | -
TRAY-C035 | CONFIRMED | src-tauri/src/tray/mod.rs:267,332 | The tray's show/window arms call `app.get_webview_window("main")`. | -
TRAY-C036 | CONFIRMED | src-tauri/src/app.rs:493-501; deep_link.rs | Deep-link replay and single-instance routing are untouched by the detach feature. | -
TRAY-C037 | DRIFT | docs/architecture/tray-and-shell.md:56 | The heading's "(v4.0)" label is stale: the section documents 4.6/4.7.0/5.0 work (#616 Intl, #620 convergence, #674 native surfaces, #984 eight locales). | P2
TRAY-C038 | DRIFT | src/lib/i18n/{en,de,fr,es,it,pl,pt,nl}.ts; src/lib/i18n.ts:26 | Eight locales ship, not three; the doc understates the shipped UI by five languages. | P1
TRAY-C039 | DRIFT | src/lib/i18n.ts:20-31 | The barrel re-exports eight dictionaries plus `store.svelte.ts`, not `{en,de,fr}.ts`. | P2
TRAY-C040 | CONFIRMED | src/lib/components/*.svelte (e.g. Dashboard.svelte, Settings.svelte) | Components import `{ t, i18n }` from `$lib/i18n`. | -
TRAY-C041 | CONFIRMED | src/lib/i18n.ts:96-118 | `t(key, params)` resolves against the active dictionary with an English fallback and substitutes `{name}` placeholders. | -
TRAY-C042 | CONFIRMED | src/lib/i18n.ts:96-118; store.svelte.ts:$state | Reading `t(...)` in a template tracks `i18n.locale`, so a switch re-renders reactively. | -
TRAY-C043 | DRIFT | src/lib/i18n.ts:26-31,36 | Eight dictionaries are typed against `Dict = keyof typeof en`, not three. | P2
TRAY-C044 | DRIFT | src/lib/i18n.ts:26-31 | Parity is a compile error across all eight dictionaries; naming only `de`/`fr` understates the enforced set. | P2
TRAY-C045 | CONFIRMED | src/lib/i18n/store.svelte.ts:10-11; commands/config.rs:1046-1071 | `config.locale` is the source of truth, written through `set_locale`. | -
TRAY-C046 | CONFIRMED | src-tauri/src/commands/config.rs:1046-1071 | The picker writes `AppConfig.locale` through `set_locale` (canonicalised via `i18n::resolve_tag`). | -
TRAY-C047 | CONFIRMED | src-tauri/src/commands/config.rs:1017-1024 | `sync_native_locale` installs the table, rebuilds the native app menu and refreshes the tray — all without a restart. | -
TRAY-C048 | DRIFT | src/lib/i18n/store.svelte.ts:31,33-38 | The mirror key is namespaced `presencejam:locale`; the bare `locale` key is the pre-4.7 legacy key folded in once and dropped (#909). | P3
TRAY-C049 | CONFIRMED | src/lib/i18n/store.svelte.ts:11-13,126-142 | The mirror is read at module load because the config arrives over an async IPC round-trip. | -
TRAY-C050 | CONFIRMED | src/lib/i18n/store.svelte.ts:354-378 | A `storage` listener converges detached webviews on the main window's write. | -
TRAY-C051 | CONFIRMED | src/lib/i18n/store.svelte.ts:261-274 | A mirror value found while the config carries none is migrated into the config exactly once. | -
TRAY-C052 | DRIFT | src/lib/i18n/store.svelte.ts:132-142 | First-run browser-language detection resolves all eight known locales, not only `de`/`fr` prefixes. | P3
TRAY-C053 | CONFIRMED | src/lib/components/Settings.svelte:23,824; grep for a General card = 0 hits | The picker lives in Settings → Appearance (`AppearanceCard`) and there is no General card. | -
TRAY-C054 | CONFIRMED | src/lib/i18n.ts:44-68 | `Intl.NumberFormat` and `Intl.PluralRules` are built once per locale in module-level records and reused. | -
TRAY-C055 | CONFIRMED | src/lib/i18n.ts:113-117,134-145 | Numeric params go through the number formatter and `tCount` selects the CLDR category's `${key}_*` entry. | -
TRAY-C056 | CONFIRMED | src/lib/i18n.ts:57-60,123-126 | The comment pins that `fr` puts `0` in `one` ("0 entrée"), so the category is not `count === 1`. | -
TRAY-C057 | CONFIRMED | docs/architecture/tray-and-shell.md:85 | The "**`<html lang>` + convergence (4.6):**" heading exists verbatim. | -
TRAY-C058 | CONFIRMED | src/app.html:2 | `<html lang="en">` ships pre-hydration. | -
TRAY-C059 | CONFIRMED | src/lib/i18n/store.svelte.ts:171-179,192 | `applyDocumentLang` retags on boot (`applyDocumentLang(initialLocale)`) and on every switch inside `applyLocale`. | -
TRAY-C060 | CONFIRMED | src/lib/i18n/store.svelte.ts:354-360 | Detached webviews own independent locale instances and converge through the `storage` listener. | -
TRAY-C061 | CONFIRMED | src/lib/i18n/store.svelte.ts:356-358,371-372 | The listener mirrors the #423 theme pattern and has a same-value guard that stops a write loop. | -
TRAY-C062 | CONFIRMED | src-tauri/src/state.rs:893-897; i18n.rs:665-733 | Native surfaces render from the `AppState`-owned `LocaleState` (issue #758 slice 3). | -
TRAY-C063 | CONFIRMED | src-tauri/src/tray/mod.rs:1059; menu.rs:96 | Both the tray and the native app menu render from `state.locale.strings()`. | -
TRAY-C064 | CONFIRMED | src-tauri/src/i18n.rs:23-137; tray/mod.rs:1059 | One `Strings` field per literal, read through the owned table. | -
TRAY-C065 | CONFIRMED | src-tauri/src/i18n.rs:138,196,254,312,370,428,486,544 | All eight tables `EN`/`DE`/`FR`/`ES`/`IT`/`PL`/`PT`/`NL` exist. | -
TRAY-C066 | CONFIRMED | src-tauri/src/i18n.rs:660-697,731-733; state.rs:897 | `LocaleState` is owned by `AppState`; each state holds its own index, so no process-wide static survives. | -
TRAY-C067 | CONFIRMED | src-tauri/src/i18n.rs:623-643 | An unknown locale logs `[I18N] unknown locale '…' — falling back to 'en'` and resolves to English. | -
TRAY-C068 | CONFIRMED | src-tauri/src/i18n.rs:1134-1169,1356-1378 | `tables_carry_an_identical_field_set` fails on drift and `no_user_visible_literal_stays_hard_coded` scans tray/{mod,cache,dedup,snooze,devices,actions}.rs plus menu.rs. | -
TRAY-C069 | CONFIRMED | src/lib/i18n.ts:20-21; i18n.rs:639-642 | Rust-side error strings and event payloads stay English, as does the app name. | -
TRAY-C070 | CONFIRMED | docs/architecture/tray-and-shell.md:102 | The "## Auto-Update (v3.0)" heading exists verbatim. | -
TRAY-C071 | DRIFT | src-tauri/src/app.rs:1085; src-tauri/src/lib.rs:1-33 | `tauri_plugin_updater::Builder::new().build()` is registered in `app.rs`, not `lib.rs`. | P2
TRAY-C072 | CONFIRMED | src-tauri/capabilities/default.json:14-15; https://v2.tauri.app/plugin/updater/ (fetched 2026-10-08) default-permission list | The capability grants exactly `updater:allow-check` and `updater:allow-download-and-install` — a deliberate least-privilege subset of `updater:default` (which also carries `allow-download` and `allow-install`) — plus the notification permissions. | -
TRAY-C073 | CONFIRMED | src-tauri/src/updater_bg.rs:913-921,1372-1397 | `update_endpoints(channel)` supplies the list `check_for_update` walks. | -
TRAY-C074 | CONFIRMED | src-tauri/src/updater_bg.rs:897-899 | The stable endpoint string matches the doc exactly. | -
TRAY-C075 | CONFIRMED | src-tauri/src/updater_bg.rs:906-907,915-921 | Beta resolves to the rolling `latest-beta.json` asset followed by the stable URL. | -
TRAY-C076 | CONFIRMED | src-tauri/src/updater_bg.rs:966-1008 | `walk_endpoints` keeps the first endpoint offering a newer release, so a missing or non-newer beta manifest falls through to stable. | -
TRAY-C077 | CONFIRMED | .github/workflows/release.yml:762-804 | `Generate latest.json` hand-assembles the manifest with `jq -n` and uploads it to the GitHub Release. | -
TRAY-C078 | CONFIRMED | .github/workflows/release.yml:773,786-787,797 | `darwin-aarch64` maps to `PresenceJam-<tag>.app.tar.gz` and its `.sig` content. | -
TRAY-C079 | CONFIRMED | .github/workflows/release.yml:774,789-790,798 | `windows-x86_64` maps to `PresenceJam-<tag>-setup.exe` and its `.sig` content. | -
TRAY-C080 | CONFIRMED | .github/workflows/release.yml:459,652 | The `.msi` is published and separately signed, producing `.msi.sig`. | -
TRAY-C081 | CONFIRMED | .github/workflows/release.yml:488,775,792-793,799 | `linux-x86_64` maps to `PresenceJam-linux-amd64.AppImage` and its `.AppImage.sig`. | -
TRAY-C082 | CONFIRMED | .github/workflows/release.yml:284-285,486-487 | The `.deb` and `.rpm` are published as separate release assets. | -
TRAY-C083 | CONFIRMED | .github/workflows/release.yml:760-761,784-801; https://v2.tauri.app/plugin/updater/ (fetched 2026-10-08): "`signature` | The content of the generated `.sig` file, which may change with each build. A path or URL does not work!" | `latest.json` carries the `.sig` content (not a path), the version as the tag without the leading `v`, and RFC-3339 `pub_date`. | -
TRAY-C084 | CONFIRMED | .github/workflows/release.yml:339,581-650; https://v2.tauri.app/plugin/updater/ (fetched 2026-10-08): "export TAURI_SIGNING_PRIVATE_KEY=…# optionally also add a password export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=" | The build matrix produces unsigned artifacts (`createUpdaterArtifacts:false`) and the separate gated `sign` job signs updater payloads with both secrets. | -
TRAY-C085 | CONFIRMED | src-tauri/tauri.conf.json plugins.updater.pubkey; https://v2.tauri.app/plugin/updater/ (fetched 2026-10-08): "`pubkey` … It **cannot** be a file path!" | The pubkey is inlined in `tauri.conf.json` so the plugin rejects tampered payloads. | -
TRAY-C086 | CONFIRMED | src/lib/components/UpdatePrompt.svelte:162 | `invoke<UpdateCheckOutcome | null>('check_for_update')` runs on startup. | -
TRAY-C087 | CONFIRMED | src/lib/components/UpdatePrompt.svelte:158-160; https://v2.tauri.app/plugin/updater/ (fetched 2026-10-08): "Setting the URLs that should be requested to check updates at runtime … For security reasons some APIs are only available for Rust." | The JS `check()` cannot take endpoints, so the candidate is resolved in Rust from the configured channel. | -
TRAY-C088 | CONFIRMED | src/lib/i18n/en.ts:288; UpdatePrompt.svelte banner block | The banner copy is `Update v{version} available` and renders only when a candidate exists. | -
TRAY-C089 | CONFIRMED | src/lib/components/UpdatePrompt.svelte:305-338 | The **Download & Install** button runs `downloadAndInstall()` (the plugin JS API). | -
TRAY-C090 | CONFIRMED | src/lib/components/UpdatePrompt.svelte:104-107,616 | The button is gated on `channelResolved && !isBeta`, i.e. Stable only, where both sources resolve to the same manifest. | -
TRAY-C091 | CONFIRMED | src/lib/components/UpdatePrompt.svelte:332; commands/misc.rs:134-154 | `invoke('relaunch_app')` spawns a blocking `app.restart()` (and discards any staged payload first). | -
TRAY-C092 | CONFIRMED | src/lib/components/UpdatePrompt.svelte:556-558,616-625 | On Beta the download button is hidden and the `update.betaOnQuitOnly` span renders instead; note that key is an i18n string (en.ts:570), not a config field. | -
TRAY-C093 | CONFIRMED | src/lib/components/UpdatePrompt.svelte:189-195 | A failed check is `console.error`-only — no banner, no toast, never blocks the UI. | -
TRAY-C094 | CONFIRMED | docs/architecture/tray-and-shell.md:141 | The "**Silent background checks + install-on-quit (v4.0):**" heading exists verbatim. | -
TRAY-C095 | CONFIRMED | src/lib/components/UpdatePrompt.svelte:50-53,247 | `CHECK_INTERVAL_MS = 24 * 60 * 60 * 1000` drives `setInterval(() => checkForUpdate('periodic'), …)`. | -
TRAY-C096 | CONFIRMED | src/lib/components/UpdatePrompt.svelte:189-195 | The periodic failure arm is the same console-only handler; no banner or toast is raised. | -
TRAY-C097 | CONFIRMED | src/lib/components/UpdatePrompt.svelte:342-344; https://v2.tauri.app/plugin/updater/ (fetched 2026-10-08): "On Windows the application is automatically exited when the install step is executed due to a limitation of Windows installers." | The JS `downloadAndInstall()` applies immediately on Windows, which is why staging is Rust-side. | -
TRAY-C098 | CONFIRMED | src-tauri/src/updater_bg.rs:1499-1544,1659-1700 | `stage_deferred_update` does its own check + `update.download(...)` + signature verification inside `spawn_blocking`. | -
TRAY-C099 | CONFIRMED | src-tauri/src/updater_bg.rs:69-75,1710-1724 | The verified `Vec<u8>` is committed into managed `PendingUpdate` state. | -
TRAY-C100 | DRIFT | src-tauri/src/app.rs:1272-1305; src-tauri/src/lib.rs:1-33 | The `build().run()` closure and the `RunEvent::Exit` arm live in `app.rs`; `lib.rs` is a 33-line module registry that only re-exports `app::run`. | P2
TRAY-C101 | CONFIRMED | src-tauri/src/menu.rs:42-48 | Tray + app-menu Quit share `request_graceful_shutdown`, bounded by `SHUTDOWN_GRACE`. | -
TRAY-C102 | CONFIRMED | src-tauri/src/menu.rs:42,51-84 | `request_graceful_shutdown` emits `app-shutdown`, polls for the drain acknowledgement for up to 8 s, then exits unconditionally. | -
TRAY-C103 | CONFIRMED | src-tauri/src/commands/sync.rs:538-559 | `app_exit` stops polling and funnels into `app.exit(0)` (`AppHandle::exit`). | -
TRAY-C104 | CONFIRMED | src-tauri/src/app.rs:1286-1305 | `install_pending_on_exit(app)` runs in the `RunEvent::Exit` arm. | -
TRAY-C105 | CONFIRMED | src-tauri/src/updater_bg.rs:1766-1776; https://v2.tauri.app/plugin/updater/ (fetched 2026-10-08): "Due to a limitation of Windows installers, Tauri will automatically quit your application before installing updates on Windows." | The Windows path exits the process during install and the installer takes over the relaunch. | -
TRAY-C106 | CONFIRMED | https://v2.tauri.app/plugin/updater/ (fetched 2026-10-08): "restarting your app immediately after installing an update is not required and you can choose how to handle the update by either waiting until the user manually restarts the app" | macOS/Linux pick up the replaced bundle/AppImage on next launch. | -
TRAY-C107 | CONFIRMED | docs/architecture/tray-and-shell.md:159 | The "*Progress + cancel (4.6):*" heading exists verbatim. | -
TRAY-C108 | CONFIRMED | src-tauri/src/updater_bg.rs:335-346,1669-1691 | The download streams throttled `update-stage-progress` events carrying `StageProgress { downloaded, total }`. | -
TRAY-C109 | CONFIRMED | src-tauri/src/updater_bg.rs:385-395 | `observe` returns `due = true` when `last_emit` is `None`, so the first chunk always emits. | -
TRAY-C110 | CONFIRMED | src-tauri/src/updater_bg.rs:348,354,386-395 | `STAGE_PROGRESS_MIN_INTERVAL = 250 ms` and `STAGE_PROGRESS_PCT_STEP = 5`, ORed in `StageProgressThrottle::observe`. | -
TRAY-C111 | CONFIRMED | src-tauri/src/updater_bg.rs:600-625,252-264 | `cancel_deferred_update` drops the staged update on demand (`PendingUpdate` → `None`). | -
TRAY-C112 | CONFIRMED | src-tauri/src/updater_bg.rs:69-75,261-264 | Dropping the staged entry releases the verified `Vec<u8>` instead of holding it for the session. | -
TRAY-C113 | CONFIRMED | src-tauri/src/updater_bg.rs:42-50,69-75 | `StagedUpdate` holds `bytes: Vec<u8>` in memory; there is no file-backed payload. | -
TRAY-C114 | CONFIRMED | src-tauri/src/updater_bg.rs:42-50; https://v2.tauri.app/plugin/updater/ (fetched 2026-10-08): "Tauri's updater needs a signature to verify that the update is from a trusted source. This cannot be disabled." | The signature is verified inside `Update::download`, so a file could be swapped between verification and exit-time install. | -
TRAY-C115 | CONFIRMED | src-tauri/src/updater_bg.rs:594-597,252-275 | `cancel_request` invalidates the active generation, so cancellation is request-scoped and final across the async boundary. | -
TRAY-C116 | DRIFT | src-tauri/src/updater_bg.rs:1499-1517,209-215 | Only the *generation* is generated (`advance_generation`); the `request_id` is a caller-supplied `String` parameter (the frontend mints it at UpdatePrompt.svelte:374). | P3
TRAY-C117 | CONFIRMED | src-tauri/src/updater_bg.rs:252-275 | `cancel_request` invalidates the active generation, removes the same request's committed payload, or leaves a bounded pre-begin tombstone. | -
TRAY-C118 | CONFIRMED | src-tauri/src/updater_bg.rs:217-235,1530-1536 | `BeginOutcome::Backpressured` refuses a new stage when the tombstone set is full rather than evicting an unmatched cancellation. | -
TRAY-C119 | CONFIRMED | src-tauri/src/updater_bg.rs:1659-1705 | The Rust `update.download(...)` transfer is never interrupted; the comment states it explicitly. | -
TRAY-C120 | CONFIRMED | src-tauri/src/updater_bg.rs:1707-1724 | `commit(&active, …)` returns false for a cancelled generation, discarding the bytes and suppressing terminal progress/completion. | -
TRAY-C121 | CONFIRMED | src/lib/components/UpdatePrompt.svelte:253-260; src/routes/+layout.svelte:519-525 | Both progress and completion are filtered by `request_id` in the webview, complementing the backend interlock. | -
TRAY-C122 | CONFIRMED | src/routes/+layout.svelte:158-200 | Notification suppression is keyed on the same `requestId`. | -
TRAY-C123 | CONFIRMED | src/routes/+layout.svelte:87-88,126-156 | The always-mounted layout reserves bounded request state (`MAX_TRACKED_UPDATE_STAGES = 32`) before the stage IPC is sent. | -
TRAY-C124 | CONFIRMED | src/routes/+layout.svelte:158-166,186 | Cancel marks the request cancelled, so a queued completion is discarded. | -
TRAY-C125 | CONFIRMED | src/routes/+layout.svelte:197-200 | `notifyUpdateStaged` receives `() => updateNotificationDispatchDestroyed || state.cancelled` as its suppression predicate. | -
TRAY-C126 | CONFIRMED | src/routes/+layout.svelte:204-211; updater_bg.rs:1570-1585 | Entries drain on teardown, and the no-stage / download-failure outcomes call `onStageCancellation(requestId, false)` at UpdatePrompt.svelte:397. | -
TRAY-C127 | CONFIRMED | src/routes/+layout.svelte:101-114,135-148 | Only `completionSeen && !notificationPending` entries are evictable; a saturated map refuses a new stage. | -
TRAY-C128 | CONFIRMED | .github/workflows/release.yml:339,581-650; README.md:128-130 | Updater payload signing is a separate minisign step independent of OS code signing, and the macOS DMG ships unsigned. | -
TRAY-C129 | CONFIRMED | README.md:128-130 | The README's "macOS first-run note" covers the unsigned DMG and applies equally to updated `.app` builds. | -
TRAY-C130 | CONFIRMED | .github/workflows/release.yml:266-283 | The build matrix has exactly one macOS leg, `aarch64-apple-darwin`; Intel Macs receive no updater payload. The referenced `docs/archive/3.0-release-research.md` exists on disk (contents not read — outside audit scope). | -
TRAY-C131 | CONFIRMED | docs/architecture/tray-and-shell.md:198 | The "## System Tray (v4.6)" heading exists verbatim. | -
TRAY-C132 | CONFIRMED | src-tauri/src/tray/mod.rs:883-1008,1054-1412 | The menu is built natively from `AppState` and the owned locale table. | -
TRAY-C133 | CONFIRMED | src-tauri/src/tray/mod.rs:1216-1235 | Both Shuffle and Repeat are `CheckMenuItemBuilder::with_id(...)` items. | -
TRAY-C134 | CONFIRMED | src-tauri/src/tray/mod.rs:1217 | Shuffle's `.checked()` reads `caches.shuffle_flag()`. | -
TRAY-C135 | CONFIRMED | src-tauri/src/tray/mod.rs:1228-1229; tray/dedup.rs:193-199; i18n.rs:150-152 | Repeat's `.checked()` reads `repeat_flag` (via `last_repeat_state`) and its label is the mode-spelling `Repeat: Off` / `Repeat: Context` / `Repeat: Track`. | -
TRAY-C136 | CONFIRMED | src-tauri/src/polling/iteration.rs:580,776; tray/dedup.rs:169-176 | `note_playback_modes` writes both atoms from the poll body, with no extra request or scope. | -
TRAY-C137 | CONFIRMED | src-tauri/src/tray/actions.rs:53-59 | `note_playing_state` / `note_playback_modes` run before `force_tray_refresh_from_app`, so the rebuild paints the accepted state. | -
TRAY-C138 | CONFIRMED | src-tauri/src/tray/mod.rs:438,467; tray/dedup.rs:139-141; spotify.rs:457-463 | Shuffle targets `!last_known`; Repeat targets `last_repeat_state(...).next()`, cycling off → context → track → off. | -
TRAY-C139 | CONFIRMED | src-tauri/src/tray/mod.rs:433,462 | Both click arms wrap the work in `std::thread::spawn`, off the menu-event thread. | -
TRAY-C140 | CONFIRMED | src-tauri/src/tray/actions.rs:112-127 | Neither error arm records anything, so the marks keep showing the last known truth. | -
TRAY-C141 | CONFIRMED | src-tauri/src/tray/actions.rs:123-126; src/routes/+layout.svelte:386-389,615 | The error is emitted on `playback-error` and rendered as an in-app toast; no OS/tray notification is sent on this path. | -
TRAY-C142 | CONFIRMED | src-tauri/src/tray/actions.rs:112-121 | `NoActiveDevice` gets its own `log::warn` and a message pointing at the tray Devices menu. | -
TRAY-C143 | CONFIRMED | src-tauri/src/spotify.rs:169,605-608 | A 403 maps to `NotPremium`, whose `Display` is exactly "Playback control requires Spotify Premium". | -
TRAY-C144 | CONFIRMED | src-tauri/src/tray/actions.rs:44-50 | `run_player_action` routes through the shared refresh-aware policy instead of snapshotting a raw token (issue #586). | -
TRAY-C145 | CONFIRMED | src-tauri/src/tray/mod.rs:371; tray/actions.rs:50; tray/devices.rs:369 | The Play/Pause state read, the player actions and the device-list re-fetch all go through `commands::playback::player_with_refresh_typed`. | -
TRAY-C146 | CONFIRMED | src-tauri/src/commands/playback.rs:174-191 | `refreshed_access_token` pre-emptively refreshes, and an `ExpiredToken` gets one refresh + retry (pinned by a test at playback.rs:356-363). | -
TRAY-C147 | CONFIRMED | src-tauri/src/tray/mod.rs:1024-1028 | The menu build clones the Spotify access token for the Devices/Queue fetches. | -
TRAY-C148 | CONFIRMED | src-tauri/src/tray/mod.rs:1029-1037 | Those tokens feed `devices_for_menu` / `queue_for_menu` — display fetches, not playback commands. | -
TRAY-C149 | CONFIRMED | src-tauri/src/polling/loop.rs:230,382 | The polling loop calls `update_tray_menu` after every iteration. | -
TRAY-C150 | CONFIRMED | src-tauri/src/tray/mod.rs:971-981; tray/dedup.rs:81-110 | The dedup key is built by `tray_snapshot_for` in `tray/dedup.rs`. | -
TRAY-C151 | DRIFT | src-tauri/src/tray/dedup.rs:32-61,81-110 | The key has **nine** inputs, not six: it also carries `devices_bucket`, `queue_bucket` (#805) and `active_profile_key` (#869). | P2
TRAY-C152 | CONFIRMED | src-tauri/src/tray/dedup.rs:20-31 | The mode atoms and the snooze minute bucket are in the key deliberately, with the #691 and #677 reasoning written out. | -
TRAY-C153 | CONFIRMED | src-tauri/src/tray/dedup.rs:20-31,41-53 | The external-mode-change rebuild (#691) and the countdown-repaint bucket (#677) are both documented at the key's definition. | -
TRAY-C154 | CONFIRMED | src-tauri/src/tray/dedup.rs:264 | The `mode_change_forces_a_tray_rebuild` test exists and pins the behaviour. | -

## DEFECTS (non-CONFIRMED, by blast radius)

```
claim_id: STORAGE-C031
file: docs/architecture/storage-and-config.md:38-39
class: C2
verdict: DRIFT
evidence: src-tauri/src/config/io.rs:768-785; src-tauri/src/commands/config.rs:419-452; grep of src-tauri/src + src + tests for `config-changed` = 0 listener hits
finding: The constant is declared in `config/io.rs`, not `config.rs`. Worse, `emit_config_changed` has no call site anywhere: `after_persist` — the shared post-write path used by `save_config`, `update_config` and `set_locale` — runs `apply_log_level`, `sync_native_locale`, the macOS activation policy and the OS autostart sync, and never emits. No webview subscribes to `config-changed`. The revision guard (#943) is fully implemented; only the live-publication half is absent, so the doc's "**are** present … and the webview subscribes and reloads" describes a feature that does not exist at this checkout.
proposed_fix: Replace the sentence with: "`config-changed` publication and live frontend adoption are **not present at this main checkout**: `config/io.rs` declares `CONFIG_CHANGED_EVENT` and `emit_config_changed` (issue #943), but no command calls it — the shared post-write path (`commands/config.rs::after_persist`) never emits — and no webview subscribes. The stale-revision rejection at `config/io.rs::save_config_to` is live and is what the frontend's re-load fallback relies on."
severity: P1
```

```
claim_id: TRAY-C038
file: docs/architecture/tray-and-shell.md:58
class: C1
verdict: DRIFT
evidence: src/lib/i18n/{en,de,fr,es,it,pl,pt,nl}.ts; src/lib/i18n.ts:26-31 (`const DICTS: Record<Locale, Dict> = { en, de, fr, es, it, pl, pt, nl }`)
finding: Eight locales ship, not three. The doc's "localized to **English, German, and French**" understates the shipped product by five languages and contradicts the same page's own C065 bullet, which correctly lists all eight Rust tables (#984).
proposed_fix: Replace with: "The UI is localized to **English, German, French, Spanish, Italian, Polish, Portuguese and Dutch** (`es` / `it` / `pl` / `pt` / `nl` are model-written with human review pending, #984)."
severity: P1
```

```
claim_id: STORAGE-C081
file: docs/architecture/storage-and-config.md:153-160
class: C1
verdict: DRIFT
evidence: src-tauri/src/state.rs:829-904
finding: The code block presents the `AppState` struct as complete but lists 5 of its 15 fields. Also missing: `calendar`, `launch_binding`, `tray_available`, `deep_link_seen`, `tokens_load`, `secret_conflict`, `last_sync_snapshot`, `session`, `locale` and `caches` — the last of which is where the quarantine flag this same page describes (#758 slice 2) actually lives.
proposed_fix: Either mark the block as an excerpt ("the five #80 step-2 sub-structs; `AppState` has since grown `calendar`, `tokens_load`, `deep_link_seen`, `tray_available`, `secret_conflict`, `last_sync_snapshot`, `session`, `locale` and `caches` — see `src-tauri/src/state.rs`") or extend it to the full struct.
severity: P2
```

```
claim_id: TRAY-C006
file: docs/architecture/tray-and-shell.md:13
class: C1
verdict: DRIFT
evidence: src-tauri/src/app.rs:71-106,122-151; src-tauri/src/lib.rs:1-33
finding: `src-tauri/src/lib.rs` is now a 33-line module registry that only re-exports `app::run`. The `DetachedPaneSpec` table and the `detach_pane` command that the doc points at live in `src-tauri/src/app.rs`. A developer following the citation lands on an empty file; `src/lib/stores/detach.ts:19` repeats the same stale `lib.rs` reference.
proposed_fix: Replace "`src-tauri/src/lib.rs` matches a fixed `DetachedPaneSpec` table" with "`src-tauri/src/app.rs::detached_pane_spec` matches a fixed `DetachedPaneSpec` table". Fix the parallel comment in `src/lib/stores/detach.ts:19` in the same change.
severity: P2
```

```
claim_id: TRAY-C014
file: docs/architecture/tray-and-shell.md:21
class: C1
verdict: DRIFT
evidence: src-tauri/src/commands/mod.rs:199-214; tests/settings.test.ts:306,315-319
finding: `reconnect_spotify` has zero `invoke()` call sites in `src/` or `tests/` — it is registered-but-callerless, superseded by the guarded `reconnect_spotify_session` (#554), and pending deletion (#771). Listing it among the commands detached panes "call … directly" describes a surface that does not exist; the live Spotify reconnect from a detached Settings pane is `reconnect_spotify_session`.
proposed_fix: Replace "`reconnect_spotify` / `reconnect_teams`" with "`reconnect_spotify_session` / `reconnect_teams`", and note that the legacy `reconnect_spotify` is callerless and pending deletion.
severity: P2
```

```
claim_id: TRAY-C016
file: docs/architecture/tray-and-shell.md:24
class: C1
verdict: DRIFT
evidence: grep `require_main_window` in src-tauri/src = 19 call sites (updater_bg 1, status 2, teams_auth 2, sync 4, playback 2, misc 2, onboarding 1, spotify_auth 5); src-tauri/src/commands/mod.rs:101-115
finding: The doc pins the guarded set at "The 13 commands". The guard is applied at 19 call sites; the command-family matrix in `commands/mod.rs:101-115` is the current authority and names all of them.
proposed_fix: Replace "The 13 commands" with "The guarded commands (see the caller-location matrix in `src-tauri/src/commands/mod.rs`)", or restate the actual count from that matrix.
severity: P2
```

```
claim_id: TRAY-C017
file: docs/architecture/tray-and-shell.md:28
class: C1
verdict: DRIFT
evidence: src-tauri/src/commands/mod.rs:157-162,199-214; src-tauri/src/commands/onboarding.rs:548-558
finding: Same root cause as C014: `reconnect_teams` is genuinely unguarded, but `reconnect_spotify` is not a live command at all — it has no call site and is superseded by the guarded `reconnect_spotify_session`. Presenting the pair as the stranded-detached-user escape hatch overstates what is reachable.
proposed_fix: Replace "`reconnect_spotify` / `reconnect_teams` are unguarded and emit `*-reconnect-required` app-wide" with "the guarded `reconnect_spotify_session` / unguarded `reconnect_teams` both emit `*-reconnect-required` app-wide (the legacy `reconnect_spotify` is callerless and pending deletion)."
severity: P2
```

```
claim_id: TRAY-C023
file: docs/architecture/tray-and-shell.md:37
class: C3
verdict: DRIFT
evidence: src-tauri/capabilities/detached.json:permissions
finding: The doc names the mirrored set as `core/event/log/opener/notification`. `opener` is not granted — the actual list is `core:default`, `core:window:allow-close`, three `core:event` permissions, `log:default` and four `notification` permissions. A reader auditing the CSP/ACL surface would look for an opener grant that does not exist.
proposed_fix: Replace "(`core/event/log/opener/notification`)" with "(`core:default`, `core:window:allow-close`, `core:event:allow-{emit,emit-to,listen}`, `log:default` and the four notification permissions)".
severity: P2
```

```
claim_id: TRAY-C151
file: docs/architecture/tray-and-shell.md:228
class: C3
verdict: DRIFT
evidence: src-tauri/src/tray/dedup.rs:32-61,77-110
finding: The dedup key has nine inputs, not the six the doc lists. Also present: `devices_bucket`, `queue_bucket` (issue #805 — without them the early return skipped the Spotify fetches and the submenus froze) and `active_profile_key` (issue #869). The doc's own comment "Nine inputs by design" sits directly above `tray_snapshot_for`.
proposed_fix: Replace the tuple with "from `(is_syncing, is_window_visible, "artist|title|is_playing", shuffle, repeat, snooze minute bucket, devices throttle bucket, queue throttle bucket, active profile id)` — nine inputs, one construction site (issue #691)."
severity: P2
```

```
claim_id: TRAY-C039
file: docs/architecture/tray-and-shell.md:59-60
class: C3
verdict: DRIFT
evidence: src/lib/i18n.ts:20-31
finding: The barrel re-exports eight dictionaries plus `store.svelte.ts`, not `src/lib/i18n/{en,de,fr}.ts`.
proposed_fix: Replace "re-exporting `src/lib/i18n/{en,de,fr}.ts` + `store.svelte.ts`" with "re-exporting the eight dictionaries `src/lib/i18n/{en,de,fr,es,it,pl,pt,nl}.ts` + `store.svelte.ts`".
severity: P2
```

```
claim_id: TRAY-C043
file: docs/architecture/tray-and-shell.md:66
class: C1
verdict: DRIFT
evidence: src/lib/i18n.ts:26-31,36,76
finding: "All three dictionaries are typed against `Dict = keyof typeof en`" — there are eight, all held to parity by the same shared type.
proposed_fix: Replace "All three dictionaries" with "All eight dictionaries".
severity: P2
```

```
claim_id: TRAY-C044
file: docs/architecture/tray-and-shell.md:67-68
class: C2
verdict: DRIFT
evidence: src/lib/i18n.ts:26-31
finding: "a key present in `en.ts` but missing from `de.ts`/`fr.ts`" understates the enforced set — parity is checked across all eight tables, and `npm run check` fails on any of them.
proposed_fix: Replace "`de.ts`/`fr.ts`" with "any of the other seven dictionaries".
severity: P2
```

```
claim_id: TRAY-C071
file: docs/architecture/tray-and-shell.md:104
class: C4
verdict: DRIFT
evidence: src-tauri/src/app.rs:1085; src-tauri/src/lib.rs:1-33
finding: `tauri_plugin_updater` is registered in `app.rs`, not `lib.rs`.
proposed_fix: Replace "(registered in `lib.rs`)" with "(registered in `app.rs::run`)".
severity: P2
```

```
claim_id: TRAY-C100
file: docs/architecture/tray-and-shell.md:151
class: C1
verdict: DRIFT
evidence: src-tauri/src/app.rs:1272-1305; src-tauri/src/lib.rs:1-33
finding: The `build().run()` closure and its `RunEvent::Exit` arm are in `app.rs`. `lib.rs` only re-exports `app::run`. The same stale `lib.rs::run` phrasing also appears in `updater_bg.rs:1781`.
proposed_fix: Replace "`lib.rs` runs the app via `build().run()`" with "`app.rs::run` runs the app via `build().run()`".
severity: P2
```

```
claim_id: STORAGE-C004
file: docs/architecture/storage-and-config.md:9
class: C2
verdict: DRIFT
evidence: src-tauri/src/config/io.rs:324-384
finding: "Three layers" undercounts. `config_from_sections` + `field_or_fallback` (issue #926) is a fourth integrity layer: a section that no longer matches the schema costs exactly that section's default (with a `[CFG]` warning) instead of quarantining the whole document. That is squarely "keep a damaged or partial `config.json` from destroying working settings", which is the sentence's own definition of a layer.
proposed_fix: Replace "Three layers keep a damaged or partial `config.json` from destroying working settings:" with "Four layers keep a damaged or partial `config.json` from destroying working settings:" and add a fourth bullet: "**Per-section fallback (#926):** `config/io.rs::config_from_sections` parses the document one typed field at a time; a section that no longer matches the schema is replaced by its default with a `[CFG]` warning, and the rest of the document is kept."
severity: P2
```

```
claim_id: STORAGE-C003
file: docs/architecture/storage-and-config.md:7
class: C5
verdict: DRIFT
evidence: src-tauri/src/config/io.rs:324-384,494-510,824-866; config/mod.rs:21-51
finding: The "(v4.6)" section label is stale — the section describes work that landed well after 4.6: #926 (per-section fallback), #916 (client_secret stripping from extras), #938 (newer-document refusal), #943 (revision + config-changed), #802, and the #758 slice-2 cache ownership.
proposed_fix: Replace "## Config integrity (v4.6)" with "## Config integrity".
severity: P2
```

```
claim_id: STORAGE-C052
file: docs/architecture/storage-and-config.md:91
class: C2
verdict: DRIFT
evidence: src-tauri/src/state.rs:228-271; src-tauri/src/app.rs:540-541
finding: `config.set(cfg)` names a method that does not exist on `Config` (which exposes `get`, `snapshot`, `get_mut`, `try_get_mut`). The real startup write is `*state.config.get_mut() = Some(Arc::new(cfg))`. The pseudocode also contradicts the very next section of this doc, which states the inner mutex is never named at call sites.
proposed_fix: Replace "App->>State: config.set(cfg)" with "App->>State: config.get_mut() = Some(Arc::new(cfg))".
severity: P2
```

```
claim_id: STORAGE-C053
file: docs/architecture/storage-and-config.md:92
class: C4
verdict: DRIFT
evidence: src-tauri/src/token_io.rs:261-264; src-tauri/src/app.rs:624-632
finding: `read_tokens_at` takes `&tauri::AppHandle`, not a directory. The path-taking entry point is the separate `read_tokens_at_path(path, TokenReadMode)`, which is what the CLI and headless paths use. The doc's `read_tokens_at(app_config_dir)` does not compile.
proposed_fix: Replace "App->>TokenIO: read_tokens_at(app_config_dir)" with "App->>TokenIO: read_tokens_at(app.handle())  (CLI: read_tokens_at_path(path, ReadOnly))".
severity: P2
```

```
claim_id: STORAGE-C087
file: docs/architecture/storage-and-config.md:163-164
class: C1
verdict: DRIFT
evidence: src-tauri/src/state.rs:42-63,113-117,224-271,335-343
finding: The method list names `config.set()`, which does not exist; the config sub-struct's write path is `get_mut()`. `tokens.spotify_mut()`, `polling.try_claim()`, `pending.spotify_mut()` and `onboarding_cache.lock()` are all real.
proposed_fix: Replace "`config.set()`" with "`config.get_mut()`".
severity: P2
```

```
claim_id: STORAGE-C055
file: docs/architecture/storage-and-config.md:94
class: C2
verdict: DRIFT
evidence: src-tauri/src/state.rs:90-93,657-658,949-969
finding: `tokens.spotify` is a private RwLock field, so `tokens.spotify = Some(st)` is not valid Rust and is the direct-field style the doc forbids. The real install is `TokensLoadGate::install_loaded` → `*tokens.spotify_mut() = loaded.spotify_tokens`, reached from `apply_token_load_result`.
proposed_fix: Replace "App->>State: tokens.spotify = Some(st)" with "App->>State: tokens_load.install_loaded(tokens, file)". Apply the equivalent change to the Teams line (C056).
severity: P3
```

```
claim_id: STORAGE-C056
file: docs/architecture/storage-and-config.md:95
class: C2
verdict: DRIFT
evidence: src-tauri/src/state.rs:90-93,657-658,949-969
finding: Same defect as C055 for the Teams slot.
proposed_fix: Replace "App->>State: tokens.teams = Some(tt)" with "App->>State: tokens_load.install_loaded(tokens, file)  (both slots)".
severity: P3
```

```
claim_id: STORAGE-C070
file: docs/architecture/storage-and-config.md:130
class: C2
verdict: DRIFT
evidence: src-tauri/src/commands/onboarding.rs:564,621
finding: The reconnect command clears the slot through `state.tokens_load.clear_spotify(&state.tokens)` / `clear_teams(...)` (issue #935 recovery gate), not by assigning `tokens.spotify = None`.
proposed_fix: Replace "Commands->>State: tokens.spotify = None (in-memory)" with "Commands->>State: tokens_load.clear_spotify(tokens) (in-memory)".
severity: P3
```

```
claim_id: STORAGE-C051
file: docs/architecture/storage-and-config.md:90
class: C2
verdict: DRIFT
evidence: src-tauri/src/config/io.rs:449-455,433-439
finding: A missing `config.json` returns `Ok(AppConfig::default())` and logs "Config file not found … using defaults". `Err` is returned only when the config directory cannot be resolved or created. Labelling the `Err` arm "first-launch path" inverts the actual first-launch behaviour.
proposed_fix: Replace "Config-->>App: AppConfig | Err (first-launch path)" with "Config-->>App: AppConfig (defaults on a missing file) | Err (config dir unresolvable)".
severity: P3
```

```
claim_id: STORAGE-C010
file: docs/architecture/storage-and-config.md:19-23
class: C2
verdict: OVERSTATED
evidence: src/lib/stores/config.ts:463-477
finding: `mergeWizardConfig` writes seven keys, not "only the four fields the wizard owns": it also sets `spotify.client_secret_set = true`, `spotify.client_secret_state = 'present'` (both derived display fields, #560) and `teams.start_minimized = fields.autostart`. The last one is a real user-facing setting that a returning user did not expect the wizard to touch.
proposed_fix: Replace "writes only the four fields the wizard owns" with "writes only the wizard's four settings — plus the two derived keychain flags it must keep truthful (#560) and `teams.start_minimized`, which the launch-at-login toggle has always driven".
severity: P3
```

```
claim_id: TRAY-C037
file: docs/architecture/tray-and-shell.md:56
class: C5
verdict: DRIFT
evidence: docs/architecture/tray-and-shell.md:79-90,91-98
finding: The "(v4.0)" heading label is stale; the section that follows describes 4.6 (`Intl` formatting, #616), 4.7.0 (native surfaces, #674) and #984 (the five model-written locales).
proposed_fix: Replace "### Interface Languages (v4.0)" with "### Interface Languages".
severity: P2
```

```
claim_id: TRAY-C048
file: docs/architecture/tray-and-shell.md:72
class: C3
verdict: DRIFT
evidence: src/lib/i18n/store.svelte.ts:31,33-38,144-160
finding: The pre-paint mirror is stored under the namespaced key `presencejam:locale`. The bare `locale` key was the pre-4.7 key and is folded into the namespaced one exactly once at module load (#909), then never consulted again.
proposed_fix: Replace "`localStorage` under `locale`" with "`localStorage` under `presencejam:locale` (the pre-4.7 bare `locale` key is folded in once and dropped, #909)".
severity: P3
```

```
claim_id: TRAY-C052
file: docs/architecture/tray-and-shell.md:77
class: C2
verdict: DRIFT
evidence: src/lib/i18n/store.svelte.ts:44-47,132-142
finding: First-run browser-language detection resolves any of the eight `KNOWN` locales (and the `pt-BR` regional alias), falling back to English — not just `de`/`fr` prefixes.
proposed_fix: Replace "(`de`/`fr` prefixes)" with "(any of the eight supported locale prefixes, plus the `pt-BR` regional tag)".
severity: P3
```

```
claim_id: TRAY-C116
file: docs/architecture/tray-and-shell.md:171
class: C1
verdict: DRIFT
evidence: src-tauri/src/updater_bg.rs:1499-1517,209-215; src/lib/components/UpdatePrompt.svelte:374-380
finding: The `request_id` is not generated by `stage_deferred_update` — it is a caller-supplied `String` parameter, minted in the frontend (`newStageRequestId()`). Only the generation counter is generated (`advance_generation`).
proposed_fix: Replace "begins under the `PendingUpdateState` lock with a generated request id and generation" with "begins under the `PendingUpdateState` lock with the caller's request id and a fresh generation".
severity: P3
```

## COUNTS

| Verdict | Count |
| --- | --- |
| CONFIRMED | 232 |
| DRIFT | 27 |
| OVERSTATED | 1 |
| STALE | 0 |
| MISSING | 0 |
| EXTERNAL-UNVERIFIED | 0 |
| UNSOURCED | 0 |
| **Total** | **260** |

P0: 0 · P1: 2 · P2: 18 · P3: 8 · "-" (no severity, CONFIRMED): 232

All 260 claims carry a verdict. Every P1/P2/P3 above carries `path:line` or a vendor
URL plus a quoted sentence. The only external-contract claims (TRAY-C026, C072,
C083, C084, C085, C087, C097, C105, C106, C114, C128) are all supported by quotes
fetched from v2.tauri.app on 2026-10-08, cross-checked against in-repo
configuration; none needed an EXTERNAL-UNVERIFIED verdict.

### Notes (no claim_id maps to these)

- The startup-loading sequence diagram at storage-and-config.md:78-97 contains 11
  statements, not the 12 the audit brief anticipated; the reconnect diagram at
  lines 120-136 does contain 8 messages. The startup diagram's step 1 is a
  participant declaration, so it is easy to over-count.
- `docs/audit/claims/*.md` (untracked) break `python3 docs/link-audit.py` with 138
  broken relative links, because the claim files quote repo-root-relative paths
  from inside `docs/audit/claims/`. Zero of those 138 are in the shipped tree, so
  the CI `docs-links` gate outcome is unaffected.
- The claim file `docs/audit/claims/AGENTS.md` was supplied in the audit brief but
  is out of this agent's assigned scope (STORAGE + TRAY only, 260 claims); no
  AGENTS.md claim was adjudicated here.
