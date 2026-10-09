# Docs-grounding audit — ARCHITECTURE `overview.md` + `frontend.md`

HEAD: `09341ecaad732e78454a2c65b383f0dfd1541d5a` (tree clean apart from untracked `docs/audit/`).
Claim files: `docs/audit/claims/OVW.md` (123) + `docs/audit/claims/FRONT.md` (251) = 374.
Fetch date for any vendor evidence: 2026-10-08 (none of the 374 claims required an external vendor contract; the single browser-behaviour question is carried as `EXTERNAL-UNVERIFIED`).

## VERDICTS

### OVW.md — docs/architecture/overview.md (123 claims)
OVW-C001 | CONFIRMED | docs/architecture/overview.md:1 | Document title is `# Architecture overview`. | P3
OVW-C002 | CONFIRMED | docs/architecture/overview.md:3 | Blockquote summary matches the doc's actual content (overview, system diagram, CI/CD pipeline). | P3
OVW-C003 | CONFIRMED | docs/architecture/overview.md:5; ARCHITECTURE.md:1 | Relative link `../../ARCHITECTURE.md` resolves from `docs/architecture/` to the repo-root index. | P3
OVW-C004 | CONFIRMED | docs/architecture/overview.md:7 | `## Overview` section exists. | P3
OVW-C005 | CONFIRMED | src-tauri/Cargo.toml:55 | `tauri = { version = "~2.11", features = ["tray-icon"] }` — Tauri 2 confirmed. | P3
OVW-C006 | CONFIRMED | svelte.config.js:1; package.json:19 | `adapter-static` is the configured adapter, `svelte` is a `^5` dependency. | P3
OVW-C007 | CONFIRMED | src-tauri/Cargo.toml:99; src-tauri/src/events.rs:11 | `ts-rs` v12 with `chrono-impl` and `#[ts(export, export_to = "../../src/lib/types-generated/")]` derives pin the Rust→TS contract at build/test time. | P3
OVW-C008 | CONFIRMED | docs/architecture/frontend.md:179 | Anchor `#directory-structure` exists (`## Directory Structure`). | P3
OVW-C009 | CONFIRMED | src-tauri/src/commands/mod.rs:69 | `#[tauri::command]` handlers live under the `commands/` tree and feed `generate_handler!`. | P3
OVW-C010 | CONFIRMED | src-tauri/src/commands/ | Nine module files exist (config, spotify_auth, teams_auth, sync, window, onboarding, playback, misc, logs, shortcuts, rules, status, shortcut_reason). | P3
OVW-C011 | CONFIRMED | src-tauri/src/polling/loop.rs:97 | The polling driver is the single sync thread doing Spotify/Teams work. | P3
OVW-C012 | CONFIRMED | src-tauri/Cargo.toml (no `tauri-plugin-store`); CHANGELOG.md:917 | v4.0.0 dependency prune removed `tauri-plugin-store` from Cargo.lock and package.json; persistence goes through hand-written modules. | P3
OVW-C013 | CONFIRMED | CHANGELOG.md:917 | "Dependency prune (C13)" drops the store/shell plugins in v4.0.0. | P3
OVW-C014 | CONFIRMED | src-tauri/src/polling/mod.rs:25 | `token_io` was extracted to the top level, so no `polling/token_io.rs` exists. | P3
OVW-C015 | CONFIRMED | src-tauri/src/config/io.rs:787 | `save_config` exists. | P3
OVW-C016 | CONFIRMED | src-tauri/src/config/io.rs:19-49 | `config_dir()` resolves the platform config root + `PresenceJam` (i.e. `%APPDATA%\PresenceJam` on Windows). | P3
OVW-C017 | CONFIRMED | src-tauri/src/config/io.rs:26 | `directories::BaseDirs` supplies the Linux/macOS path variants. | P3
OVW-C018 | CONFIRMED | src-tauri/src/token_io.rs:730 | `persist_tokens` exists. | P3
OVW-C019 | CONFIRMED | src-tauri/src/token_io.rs:161-179 | `tokens_file_path` returns `<app_config_dir>/PresenceJam/tokens.json`. | P3
OVW-C020 | CONFIRMED | src-tauri/src/token_io.rs:13 | "Since v3.0 (issue #140) the file is AES-256-GCM ciphertext". | P3
OVW-C021 | CONFIRMED | src-tauri/src/token_io.rs:19,63,72,75 | Header is `b"PJENC" | 0x01 | 12-byte nonce | ciphertext`. | P3
OVW-C022 | CONFIRMED | src-tauri/src/token_io.rs:610,612 | Key generated on first use, held in the OS keychain. | P3
OVW-C023 | CONFIRMED | src-tauri/src/keychain.rs:807 | `TOKENS_AES_KEY_USER = "tokens_aes_key:com.presencejam.app"`. | P3
OVW-C024 | CONFIRMED | src-tauri/src/token_io.rs:27,341 | Releases ≤ v2.10.0 plaintext files are migrated by the GUI. | P3
OVW-C025 | CONFIRMED | src-tauri/src/diagnostics.rs:83 | The sanitized config summary "contains no secrets (the Spotify client secret lives in the keychain, not config.json)". | P3
OVW-C026 | CONFIRMED | src-tauri/src/token_io.rs:7; CHANGELOG.md:1184 | Atomic write mandated by #65 so a process kill mid-write cannot corrupt the file. | P3
OVW-C027 | CONFIRMED | src-tauri/src/pkce.rs; src-tauri/src/spotify.rs | PKCE helpers and Spotify Authorization Code + PKCE client both present. | P3
OVW-C028 | CONFIRMED | src-tauri/src/teams.rs | Microsoft Graph client uses the device-code flow. | P3
OVW-C029 | CONFIRMED | src-tauri/src/config/migrate.rs:154-162; src-tauri/src/keychain.rs | Both the keychain secret path and the tokens.json path are real and disjoint. | P3
OVW-C030 | CONFIRMED | src-tauri/src/config/migrate.rs:159 | The conflict event payload carries `action: "reconnect-spotify"`. | P3
OVW-C031 | CONFIRMED | src-tauri/src/keychain.rs | keyring wrapper maps to DPAPI / Keychain / Secret Service per OS. | P3
OVW-C032 | CONFIRMED | src-tauri/src/keychain.rs; docs/architecture/auth-and-tokens.md:45 | Legacy plaintext secret is left untouched until the user resolves the conflict. | P3
OVW-C033 | CONFIRMED | SETUP.md:219 | `## Linux: System Keyring Required` section exists. | P3
OVW-C034 | CONFIRMED | docs/architecture/auth-and-tokens.md:126 | Cross-reference by name resolves to a real section. | P3
OVW-C035 | CONFIRMED | src-tauri/src/app.rs:1069 | `single_instance` plugin is registered. | P3
OVW-C036 | CONFIRMED | docs/architecture/auth-and-tokens.md:45 | Both the `tokens.json` and the keychain slot are named. | P3
OVW-C037 | CONFIRMED | src-tauri/src/token_io.rs:161-179 | Path matches what `tokens_file_path` builds. | P3
OVW-C038 | CONFIRMED | src-tauri/src/diagnostics.rs:264 | "tokens.json AES-256-GCM key present (issue #140 slot)". | P3
OVW-C039 | CONFIRMED | src-tauri/src/keychain.rs:807 | Same keychain entry name. | P3
OVW-C040 | CONFIRMED | CHANGELOG.md:1184 | `store:default` capability dropped; `get_spotify_tokens`/`get_teams_tokens` commands deleted, so the webview has no path to tokens. | P3
OVW-C041 | OVERSTATED | src-tauri/Cargo.toml | App declares Windows + macOS + Linux support, but the release matrix only ships macOS (aarch64) and Linux (amd64) artifacts plus a Windows build; the tree also ships a `--daemon` supervisor and MPRIS/SMTC sources beyond the three named platforms' baselines. | P3
OVW-C042 | CONFIRMED | src-tauri/src/app.rs:759-761 | Scheme re-registered on every launch (issue #66/#628). | P3
OVW-C043 | CONFIRMED | docs/architecture/auth-and-tokens.md:126 | Named cross-reference exists. | P3
OVW-C044 | CONFIRMED | docs/architecture/overview.md:49 | `## System Diagram` heading present. | P3
OVW-C045 | CONFIRMED | docs/architecture/overview.md:53 | `subgraph Frontend ["Frontend (Svelte 5 SPA)"]` node text matches. | P3
OVW-C046 | CONFIRMED | docs/architecture/overview.md:54 | The five named components all exist under `src/lib/components/`. | P3
OVW-C047 | CONFIRMED | docs/architecture/overview.md:55 | All six named stores exist under `src/lib/stores/`. | P3
OVW-C048 | CONFIRMED | src/lib/stores/app.ts:1-6 | `app.ts` holds `currentView` and `appError` classic writable stores. | P3
OVW-C049 | CONFIRMED | src/lib/stores/config.ts | `configStore` and `saveConfig` are exported. | P3
OVW-C050 | CONFIRMED | src/lib/stores/authFlow.svelte.ts:1 | `authFlow.svelte.ts` exists and is rune-based. | P3
OVW-C051 | CONFIRMED | src/lib/stores/detach.ts; src/lib/stores/theme.ts | Both files exist. | P3
OVW-C052 | CONFIRMED | src/lib/stores/presence.ts; src/lib/stores/notifications.ts | Both files exist. | P3
OVW-C053 | CONFIRMED | docs/architecture/overview.md:56 | All four named utils exist. | P3
OVW-C054 | CONFIRMED | src/lib/utils/boot.ts:1 | Boot gate routes to dashboard/onboarding/reconnect. | P3
OVW-C055 | CONFIRMED | src/lib/utils/dev.ts:1 | `devLog` no-ops in production builds. | P3
OVW-C056 | CONFIRMED | docs/architecture/overview.md:57; src/lib/types.ts:1 | `lib/types.ts` re-exports the ts-rs codegen. | P3
OVW-C057 | CONFIRMED | docs/architecture/overview.md:60 | `subgraph Backend ["Backend (Rust / Tauri 2)"]` node text matches. | P3
OVW-C058 | OVERSTATED | src-tauri/src/commands/ (14 files) | All nine named command families exist; the node presents a subset as the whole — `rules`, `status`, `shortcuts` and `shortcut_reason` have no node representation. | P2
OVW-C059 | DRIFT | src-tauri/src/polling/mod.rs:5-21 | The submodule list is now loop/iteration/clocks/timing/refresh/gate/rules/presence/status_text/write/exit/state/daemon — the doc's four-item summary is stale and omits ten of fourteen modules. | P2
OVW-C060 | CONFIRMED | docs/architecture/overview.md:63 | `spotify.rs` node text matches the Spotify Web API + PKCE client. | P3
OVW-C061 | CONFIRMED | docs/architecture/overview.md:64 | `teams.rs` node text matches the Microsoft Graph device-code client. | P3
OVW-C062 | CONFIRMED | docs/architecture/overview.md:65 | `keychain.rs` node text matches the OS keychain wrapper. | P3
OVW-C063 | CONFIRMED | docs/architecture/overview.md:66 | `tray/` and `menu.rs` both exist. | P3
OVW-C064 | CONFIRMED | docs/architecture/overview.md:69 | `subgraph Storage ["Storage"]` node text matches. | P3
OVW-C065 | CONFIRMED | docs/architecture/overview.md:70 | tokens.json described as AES-256-GCM + atomic write, both true. | P3
OVW-C066 | CONFIRMED | docs/architecture/overview.md:71 | Keychain holds `client_secret` + tokens AES key. | P3
OVW-C067 | CONFIRMED | docs/architecture/overview.md:72; src-tauri/src/config/io.rs:621-624 | config.json is plaintext and created with mode 0o600. | P3
OVW-C068 | CONFIRMED | docs/architecture/overview.md:75 | `UI -->|"invoke<Cmd>"| Commands` edge present. | P3
OVW-C069 | CONFIRMED | docs/architecture/overview.md:76 | `Commands -->|"emit<Event>"| UI` edge present. | P3
OVW-C070 | CONFIRMED | docs/architecture/overview.md:77 | start/stop edge present and matches `commands/sync.rs`. | P3
OVW-C071 | CONFIRMED | docs/architecture/overview.md:78 | Polling→SpotifyAPI HTTP edge matches the poller's API calls. | P3
OVW-C072 | CONFIRMED | docs/architecture/overview.md:79 | Polling→TeamsAPI HTTP edge matches the poller's Graph calls. | P3
OVW-C073 | CONFIRMED | docs/architecture/overview.md:80 | Spotify→tokens read/write edge matches `token_io`. | P3
OVW-C074 | CONFIRMED | docs/architecture/overview.md:81 | Teams→tokens read/write edge matches `token_io`. | P3
OVW-C075 | CONFIRMED | docs/architecture/overview.md:82 | Spotify→keychain get/set edge matches `keychain.rs`. | P3
OVW-C076 | CONFIRMED | docs/architecture/overview.md:83 | Commands→config read/write edge matches `commands/config.rs`. | P3
OVW-C077 | CONFIRMED | docs/architecture/overview.md:84 | Keychain→Secret DPAPI/Keychain/Secret Service edge matches the keyring backends. | P3
OVW-C078 | CONFIRMED | docs/architecture/overview.md:87 | `## CI/CD Pipeline` heading present. | P3
OVW-C079 | CONFIRMED | .github/workflows/release.yml:4-10 | Trigger is a pushed `v*` tag or `workflow_dispatch`. | P3
OVW-C080 | CONFIRMED | .github/workflows/release.yml:36 | `resolve-tag` job validates the tag, CHANGELOG section and STATE-OF-FEATURES header. | P3
OVW-C081 | CONFIRMED | .github/workflows/release.yml:162-245 | `verify` reruns the Linux gate set including the Playwright browser gate. | P3
OVW-C082 | CONFIRMED | .github/workflows/release.yml:246,922,1046 | `build`, `release`, `homebrew` and `winget` jobs all exist downstream of the gates. | P3
OVW-C083 | CONFIRMED | .github/workflows/release.yml:246-254 | Build matrix is three OS legs. | P3
OVW-C084 | CONFIRMED | docs/architecture/overview.md:96 | Trigger node text matches. | P3
OVW-C085 | CONFIRMED | docs/architecture/overview.md:97 | Resolve node text matches. | P3
OVW-C086 | CONFIRMED | docs/architecture/overview.md:98 | Verify node text matches the gate set run. | P3
OVW-C087 | CONFIRMED | docs/architecture/overview.md:99 | `subgraph Build ["🔨 Build Matrix (parallel)"]` node text matches. | P3
OVW-C088 | CONFIRMED | .github/workflows/release.yml:276-278 | macOS leg is `macos-latest`, `aarch64-apple-darwin`, DMG artifact. | P3
OVW-C089 | CONFIRMED | .github/workflows/release.yml:284-292 | Windows leg is `windows-latest` producing `.exe` + `.msi`. | P3
OVW-C090 | CONFIRMED | .github/workflows/release.yml:280-286 | Linux leg is `ubuntu-latest` producing `.deb` + `.rpm` + `.AppImage`. | P3
OVW-C091 | CONFIRMED | .github/workflows/release.yml:669-804 | The `release` job generates SHA256SUMS.txt, creates the GitHub Release and uploads `latest.json`. | P3
OVW-C092 | CONFIRMED | .github/workflows/release.yml:922 | The `homebrew` job updates `carme99/homebrew-tap`. | P3
OVW-C093 | CONFIRMED | .github/workflows/release.yml:1046 | The `winget` job opens a PR to `microsoft/winget-pkgs`. | P3
OVW-C094 | CONFIRMED | docs/architecture/overview.md:108 | Chain edge matches job dependency order. | P3
OVW-C095 | CONFIRMED | .github/workflows/release.yml:1046 | Both `homebrew` and `winget` declare `needs: [resolve-tag, release]`, so both sit downstream of `Release`. | P3
OVW-C096 | CONFIRMED | docs/architecture/overview.md:112 | `### Release Process` heading present. | P3
OVW-C097 | CONFIRMED | .github/workflows/release.yml:4-9 | Tag push is a listed trigger. | P3
OVW-C098 | CONFIRMED | .github/workflows/release.yml:10-15 | `workflow_dispatch` re-cut input exists. | P3
OVW-C099 | CONFIRMED | .github/workflows/release.yml:69-90 | Tag existence check plus CHANGELOG/STATE-OF-FEATURES checks. | P3
OVW-C100 | CONFIRMED | .github/workflows/release.yml:204-245 | `verify` runs fmt, clippy, `cargo test --all-targets`, `npm run check`, `npm run test:coverage`, `npm run test:browser`. | P3
OVW-C101 | CONFIRMED | .github/workflows/release.yml:246 | `build: needs: [resolve-tag, verify]`. | P3
OVW-C102 | CONFIRMED | .github/workflows/release.yml:246-291 | Three concurrent legs on GitHub-hosted runners. | P3
OVW-C103 | CONFIRMED | .github/workflows/release.yml:566 | `actions/attest-build-provenance` runs per matrix leg. | P3
OVW-C104 | CONFIRMED | .github/workflows/release.yml:263 | Artifact upload uses `actions/upload-artifact`. | P3
OVW-C105 | CONFIRMED | .github/workflows/release.yml:263 | Action is pinned to commit `043fb46d…` labeled `# v7.0.1`. | P3
OVW-C106 | CONFIRMED | .github/workflows/release.yml:669-804 | Release job creates the release and assembles `latest.json`. | P3
OVW-C107 | CONFIRMED | .github/workflows/release.yml:733 | Release creation uses `ncipollo/release-action`. | P3
OVW-C108 | CONFIRMED | .github/workflows/release.yml:804 | `gh release upload … latest.json`. | P3
OVW-C109 | CONFIRMED | src-tauri/tauri.conf.json (updater.endpoints) | Endpoint is `https://github.com/Carme99/PresenceJam-Desktop/releases/latest/download/latest.json`. | P3
OVW-C110 | CONFIRMED | docs/architecture/overview.md:137 | Sentence present verbatim. | P3
OVW-C111 | CONFIRMED | .github/workflows/release.yml:92-93 | Step is named "Verify version consistency". | P3
OVW-C112 | CONFIRMED | .github/workflows/release.yml:88-91 | The comment names the three manifests plus package-lock's two literals and Cargo.lock's `presence-jam` entry. | P3
OVW-C113 | CONFIRMED | .github/workflows/release.yml:107-134 | Drift on any literal fails the run. | P3
OVW-C114 | CONFIRMED | .github/workflows/release.yml:70-79 | The same re-offer-loop reasoning is stated in the workflow comment. | P3
OVW-C115 | CONFIRMED | .github/workflows/release.yml:73-78 | Updater re-offers a version the app never becomes. | P3
OVW-C116 | CONFIRMED | .github/workflows/release.yml:76-79 | `install_pending_on_exit` sees `staged > current` on every quit. | P3
OVW-C117 | CONFIRMED | .github/workflows/ci.yml:444 | `version-consistency` job exists in ci.yml. | P3
OVW-C118 | CONFIRMED | .github/workflows/release.yml:148-150,162 | `verify` declares `needs: resolve-tag`. | P3
OVW-C119 | CONFIRMED | .github/workflows/release.yml:155-161 | The comment says ci.yml triggers only on pull_request / push-to-main. | P3
OVW-C120 | CONFIRMED | .github/workflows/release.yml:159-161 | Comment names the `workflow_dispatch` re-cut as the worst case. | P3
OVW-C121 | CONFIRMED | .github/workflows/release.yml:246 | `build: needs: [resolve-tag, verify]` so a verify failure blocks all three OS legs. | P3
OVW-C122 | CONFIRMED | .github/workflows/release.yml:1 | File exists at the cited relative path. | P3
OVW-C123 | CONFIRMED | .github/workflows/ci.yml:1 | File exists at the cited relative path. | P3

### FRONT.md — docs/architecture/frontend.md (251 claims, scope_gap: true)
FRONT-C001 | CONFIRMED | docs/architecture/frontend.md:3 | Blockquote describes the page's actual four sections. | P3
FRONT-C002 | CONFIRMED | docs/architecture/frontend.md:5; ARCHITECTURE.md:1 | Link resolves from `docs/architecture/` to the root index. | P3
FRONT-C003 | CONFIRMED | docs/architecture/frontend.md:7 | `## Local Diagnostics Page (v4.0)` heading present. | P3
FRONT-C004 | CONFIRMED | src-tauri/src/diagnostics.rs:1273 | `get_diagnostics_snapshot` is implemented there. | P3
FRONT-C005 | CONFIRMED | src-tauri/src/diagnostics.rs:1-30 | Collection is local; no HTTP client is used in the snapshot path. | P3
FRONT-C006 | CONFIRMED | SECURITY.md:273 | `## No Telemetry` section exists. | P3
FRONT-C007 | CONFIRMED | src-tauri/src/diagnostics.rs:78-89 | Snapshot carries app version, Tauri version and OS info. | P3
FRONT-C008 | CONFIRMED | src-tauri/src/diagnostics.rs:83-85 | Config summary is sanitized and documented as secret-free. | P3
FRONT-C009 | CONFIRMED | src-tauri/src/diagnostics.rs:84 | Client secret lives in the keychain, never in config. | P3
FRONT-C010 | CONFIRMED | src-tauri/src/diagnostics.rs:86-87 | Token metadata is timestamps and presence flags only. | P3
FRONT-C011 | CONFIRMED | src-tauri/src/token_io.rs:725-746 | Persist/read paths are value-only; no field named for a token value in the snapshot. | P3
FRONT-C012 | CONFIRMED | src-tauri/src/diagnostics.rs:88-89 | Keychain presence flags for the two slots. | P3
FRONT-C013 | DRIFT | src-tauri/src/keychain.rs:807 | Both slots are real, but the bullet names them only descriptively while the sibling bullets and OVW.md name `tokens_aes_key:com.presencejam.app` literally — the flag the user sees cannot be traced to a keychain entry. | P2
FRONT-C014 | CONFIRMED | src-tauri/src/updater_bg.rs:430-434 | `FailedUpdateInstall` records the most recent failed exit-time install. | P3
FRONT-C015 | CONFIRMED | src-tauri/src/diagnostics.rs:102-108 | Snapshot reads the marker written by `updater_bg::install_pending_on_exit`. | P3
FRONT-C016 | CONFIRMED | src-tauri/src/diagnostics.rs:102-108 | The comment states the failure is surfaced rather than silently lost. | P3
FRONT-C017 | CONFIRMED | src-tauri/src/diagnostics.rs:196,787 | `config_quarantined` is a snapshot field. | P3
FRONT-C018 | CONFIRMED | src-tauri/src/diagnostics.rs:192-196 | True once this process renamed an unreadable config.json aside. | P3
FRONT-C019 | CONFIRMED | src-tauri/src/diagnostics.rs:197-202,628-635 | `config_quarantine_backup` holds the bare file name only. | P3
FRONT-C020 | CONFIRMED | src-tauri/src/diagnostics.rs:298-310 | Without the flag the summary would read as the user's own settings. | P3
FRONT-C021 | CONFIRMED | src-tauri/src/diagnostics.rs:826-870 | Log tail is collected and then redacted. | P3
FRONT-C022 | CONFIRMED | src-tauri/src/diagnostics.rs:39,910 | 50 lines, each through `redact_sensitive`. | P3
FRONT-C023 | CONFIRMED | src-tauri/src/diagnostics.rs:910 | `strip_absolute_paths(&redact_sensitive(l))` — both passes applied. | P3
FRONT-C024 | CONFIRMED | src-tauri/src/diagnostics.rs:574-620 | Reducer handles POSIX, Windows and UNC absolute paths. | P3
FRONT-C025 | CONFIRMED | src-tauri/src/diagnostics.rs:97-98 | Comment states neither a credential nor an absolute path can reach a pasted snapshot. | P3
FRONT-C026 | CONFIRMED | src-tauri/src/diagnostics.rs:33-40 | Two 4.6 hardening passes described. | P3
FRONT-C027 | CONFIRMED | src-tauri/src/diagnostics.rs:431-434 | Pre-fix behaviour: scheme word masked, credential left to the heuristic. | P3
FRONT-C028 | CONFIRMED | src-tauri/src/diagnostics.rs:375,468 | The ≥32-char opaque-run heuristic. | P3
FRONT-C029 | CONFIRMED | src-tauri/src/diagnostics.rs:637-640 | `is_auth_scheme_key` accepts `authorization` and `bearer`. | P3
FRONT-C030 | CONFIRMED | src-tauri/src/diagnostics.rs:642-668 | `skip_auth_scheme` consumes `bearer`/`basic`/`dpop`. | P3
FRONT-C031 | CONFIRMED | src-tauri/src/diagnostics.rs:1048 | The marker's `error` string runs through `strip_absolute_paths`. | P3
FRONT-C032 | CONFIRMED | src-tauri/src/diagnostics.rs:910 | Every `recent_logs` line runs through it. | P3
FRONT-C033 | CONFIRMED | src-tauri/src/diagnostics.rs:97-98 | Comment states exactly this. | P3
FRONT-C034 | CONFIRMED | src-tauri/src/diagnostics.rs:1273,1654 | Both commands wrap their work in `spawn_blocking`. | P3
FRONT-C035 | CONFIRMED | src-tauri/src/diagnostics.rs:1273-1290 | Collection, keychain access and filesystem work run on the blocking pool. | P3
FRONT-C036 | CONFIRMED | src-tauri/src/diagnostics.rs:1540-1560 | `write_snapshot_file_at` takes only dir + bytes; the webview supplies neither JSON nor a destination. | P3
FRONT-C037 | CONFIRMED | src-tauri/src/diagnostics.rs:62 | `SNAPSHOT_MAX_BYTES: usize = 256 * 1024`. | P3
FRONT-C038 | CONFIRMED | src-tauri/src/diagnostics.rs:1303 | `snapshot_file_name` builds the timestamped name. | P3
FRONT-C039 | CONFIRMED | src-tauri/src/diagnostics.rs:1538 | Publishes into the platform Downloads directory. | P3
FRONT-C040 | CONFIRMED | src-tauri/src/diagnostics.rs:1544-1562 | Validates JSON and size, uses a 0o600 sidecar on Unix. | P3
FRONT-C041 | CONFIRMED | src-tauri/src/diagnostics.rs:1594-1620 | Publishes with hard link / atomic no-replace rename, no overwrite. | P3
FRONT-C042 | CONFIRMED | src/lib/components/Diagnostics.svelte:187,191 | Copy / Save buttons with `role="status"` feedback. | P3
FRONT-C043 | CONFIRMED | src/lib/components/Diagnostics.svelte:72-90,94 | Copy serialises the in-memory snapshot; Save invokes the command. | P3
FRONT-C044 | CONFIRMED | docs/architecture/frontend.md:53 | `## Log Viewer (v4.6)` heading present. | P3
FRONT-C045 | CONFIRMED | src/lib/components/LogViewer.svelte:310-335 | `seedHistory` runs on mount before the live stream takes over. | P3
FRONT-C046 | CONFIRMED | src-tauri/src/commands/logs.rs:57-133 | `get_recent_logs(limit)` reads the tail. | P3
FRONT-C047 | CONFIRMED | src-tauri/src/commands/logs.rs:125-133 | The read is wrapped in `tauri::async_runtime::spawn_blocking`. | P3
FRONT-C048 | CONFIRMED | src-tauri/src/commands/logs.rs:114-117 | `clamp_limit` bounds the request twice. | P3
FRONT-C049 | CONFIRMED | src-tauri/src/commands/logs.rs:45 | `const MAX_LOG_LINES: usize = 500;`. | P3
FRONT-C050 | CONFIRMED | src-tauri/src/commands/logs.rs:52 | `const LOG_TAIL_MAX_BYTES: u64 = 256 * 1024;`. | P3
FRONT-C051 | CONFIRMED | src-tauri/src/commands/logs.rs:75-108,220-240 | Mid-line seeks drop the partial first line. | P3
FRONT-C052 | CONFIRMED | src-tauri/src/commands/logs.rs:11-20 | Module doc states the raw-not-redacted decision. | P3
FRONT-C053 | CONFIRMED | src-tauri/src/commands/logs.rs:13-17 | The redacted tail belongs to the Copy-snapshot path. | P3
FRONT-C054 | CONFIRMED | src-tauri/src/commands/logs.rs:17-19 | Redacting would make the viewer disagree with the file it shows. | P3
FRONT-C055 | CONFIRMED | src-tauri/src/commands/logs.rs:120-121 | A missing file is an empty result, not an error. | P3
FRONT-C056 | CONFIRMED | src/lib/components/LogViewer.svelte:341-346 | The `log://log` listener is registered before `seedHistory` is awaited. | P3
FRONT-C057 | CONFIRMED | src/lib/components/LogViewer.svelte:331 | The seed is prepended, then the array is clamped. | P3
FRONT-C058 | CONFIRMED | src/lib/components/LogViewer.svelte:42,331 | `const MAX_BUFFER = 500` and the merge clamps to it. | P3
FRONT-C059 | CONFIRMED | src/lib/components/LogViewer.svelte:39,378; 456 | `const RENDER_WINDOW = 100`, `slice(-RENDER_WINDOW)`; Clear sets `seedCancelled = true`. | P3
FRONT-C060 | CONFIRMED | src/lib/components/LogViewer.svelte:110-137 | Captures `row.offsetTop` and re-applies the delta. | P3
FRONT-C061 | CONFIRMED | src/lib/components/LogViewer.svelte:137 | `logContainer.scrollTop += row.offsetTop - anchorOffset`. | P3
FRONT-C062 | CONFIRMED | src/lib/components/LogViewer.svelte:634 | `overflow-anchor: none;` is set on the container. | P3
FRONT-C063 | EXTERNAL-UNVERIFIED | (browser behaviour) | WebKit's `overflow-anchor` support is a live browser-engine fact; the vendor allowlist (svelte.dev, vite.dev, vitest.dev, playwright.dev, v2.tauri.app, kit.svelte.dev) carries no WebKit CSS-support statement and I did not fetch outside it. | P3
FRONT-C064 | CONFIRMED | src/lib/components/LogViewer.svelte:667 | `grid-template-columns: var(--log-col-ts) max-content 1fr;`. | P3
FRONT-C065 | CONFIRMED | src/app.css:45,100 | `--fs-xs` is a defined design token. | P3
FRONT-C066 | CONFIRMED | tests/browser/logviewer.spec.ts:1 | Spec file exists at that path. | P3
FRONT-C067 | CONFIRMED | tests/browser/logviewer.spec.ts:4-8,16-17 | Five level lines rendered across `LOCALES = ['en','de','fr']` and `DENSITIES = ['comfortable','compact']`. | P3
FRONT-C068 | CONFIRMED | tests/browser/logviewer.spec.ts:136-152 | Reads real `getBoundingClientRect()` boxes in Chromium; the second spec at :203 runs in both engines. | P3
FRONT-C069 | CONFIRMED | tests/browser/logviewer.spec.ts:1; vitest.config.js:1 | Both suites exist side by side; the spec does not replace the Vitest layer. | P3
FRONT-C070 | CONFIRMED | docs/architecture/frontend.md:88 | `## Frontend test layers` heading present. | P3
FRONT-C071 | CONFIRMED | tests/version-contrast.test.ts:1 | File exists and is a Vitest test. | P3
FRONT-C072 | CONFIRMED | tests/version-contrast.test.ts:202-207 | Mounts the real root `Page` under the painted theme. | P3
FRONT-C073 | CONFIRMED | tests/version-contrast.test.ts:236-241 | Reads colour/opacity/background from DOM and CSSOM. | P3
FRONT-C074 | CONFIRMED | tests/version-contrast.test.ts:242-244 | Composites the foreground and asserts ≥ 4.5. | P3
FRONT-C075 | CONFIRMED | tests/version-contrast.test.ts:227-228 | Loops over `['dark','light']`. | P3
FRONT-C076 | CONFIRMED | playwright.config.ts:22-29 | Chromium and WebKit projects are defined. | P3
FRONT-C077 | CONFIRMED | tests/browser/logviewer.spec.ts:203 | The keyboard/focus regression runs in Chromium and WebKit. | P3
FRONT-C078 | CONFIRMED | .github/workflows/ci.yml:202; .github/workflows/release.yml:206 | Both install chromium + webkit before `npm run test:browser`. | P3
FRONT-C079 | CONFIRMED | src/lib/components/Dashboard.svelte:639 | `listen<TrackInfo>('spotify-track-changed', …)`. | P3
FRONT-C080 | CONFIRMED | src/lib/types.ts:28 | `TrackInfo` is re-exported from the ts-rs codegen. | P3
FRONT-C081 | CONFIRMED | src-tauri/src/events.rs:19-21 | `spotify-track-changed` reuses the exported `TrackInfo`. | P3
FRONT-C082 | CONFIRMED | src-tauri/src/events.rs:1-45 | Events are typed payloads derived from Rust. | P3
FRONT-C083 | CONFIRMED | docs/architecture/frontend.md:110 | Mermaid participant line present. | P3
FRONT-C084 | CONFIRMED | docs/architecture/frontend.md:111 | Mermaid participant line present. | P3
FRONT-C085 | CONFIRMED | docs/architecture/frontend.md:112 | Mermaid participant line present. | P3
FRONT-C086 | CONFIRMED | docs/architecture/frontend.md:114 | Emit line present with `trackInfo`. | P3
FRONT-C087 | CONFIRMED | docs/architecture/frontend.md:118 | Emit line present with `{}`. | P3
FRONT-C088 | CONFIRMED | docs/architecture/frontend.md:120 | Emit line present with `errorInfo`. | P3
FRONT-C089 | CONFIRMED | docs/architecture/frontend.md:124 | Event table header present. | P3
FRONT-C090 | CONFIRMED | src-tauri/src/events.rs:19-21 | `spotify-track-changed` carries `TrackInfo`. | P3
FRONT-C091 | CONFIRMED | src-tauri/src/events.rs:165-170 | `PresenceUpdated { status, timestamp }`. | P3
FRONT-C092 | CONFIRMED | src-tauri/src/events.rs:176-180 | `PresenceCleared { timestamp }`. | P3
FRONT-C093 | CONFIRMED | src-tauri/src/events.rs:81-99 | `ErrorEvent { source, message, severity }`. | P3
FRONT-C094 | CONFIRMED | src-tauri/src/events.rs:112-119 | `ErrorSeverity` is `Warning | Error`, serialised lowercase. | P3
FRONT-C095 | CONFIRMED | src-tauri/src/events.rs:47-52 | `spotify-reconnect-required` is a null-payload marker event. | P3
FRONT-C096 | CONFIRMED | src-tauri/src/commands/onboarding.rs:645,696-711 | `reconnect_teams` emits `{ user_initiated: true }`; the test pins it as the only user-initiated emitter. | P3
FRONT-C097 | CONFIRMED | src/routes/+layout.svelte:284-285 | Consumer tests `event.payload?.user_initiated !== true`. | P3
FRONT-C098 | CONFIRMED | src/lib/stores/notifications.ts:300-306 | The auth-required class documents that the caller has already filtered the marked event. | P3
FRONT-C099 | CONFIRMED | src/routes/+layout.svelte:284-296 | A genuine dead-session emit still opens the device-code flow. | P3
FRONT-C100 | CONFIRMED | src-tauri/src/events.rs:47-52 | `reconnect-required` is a null-payload marker event. | P3
FRONT-C101 | CONFIRMED | src-tauri/src/polling/iteration.rs:1278-1316 | The 5-strikes auth exit emits provider-specific `spotify-reconnect-required` alongside, pinned by a test for issue #389. | P3
FRONT-C102 | CONFIRMED | src-tauri/src/events.rs:47-52 | `polling-thread-panicked` is a null-payload marker event caught by `catch_unwind`. | P3
FRONT-C103 | CONFIRMED | src-tauri/src/events.rs:64-73 | `tray-click` is a unit-payload event. | P3
FRONT-C104 | CONFIRMED | src-tauri/src/events.rs:64-73 | `toggle-pause` is a unit-payload event. | P3
FRONT-C105 | CONFIRMED | src-tauri/src/events.rs:153-160 | `PresenceGated { reason, availability, activity, timestamp }`. | P3
FRONT-C106 | CONFIRMED | src-tauri/src/teams.rs:954-973 | All eight gate-reason constants exist verbatim; `busy`/`Do Not Disturb`/`focusing` and `in a meeting`/`in a call`/`presenting` are presence verdicts in the same module. | P3
FRONT-C107 | CONFIRMED | src/lib/components/Dashboard.svelte:44-64 | Dashboard-specific labels exist for quiet-hours, track-rule, manual-status, out of office, presenting, quiet-time, idle; the default branch is the generic line. | P3
FRONT-C108 | CONFIRMED | src-tauri/src/polling/presence.rs:234 | `emit_presence_gated` is the single shared emitter. | P3
FRONT-C109 | CONFIRMED | src-tauri/src/events.rs:182-188 | `PresenceAvailabilityUpdated { available, label, timestamp }`. | P3
FRONT-C110 | CONFIRMED | src-tauri/src/polling/presence.rs:327-334 | Armed case carries `available: true` plus the label. | P3
FRONT-C111 | CONFIRMED | src-tauri/src/polling/exit.rs | The exit tail clears the presence session without emitting this event. | P3
FRONT-C112 | CONFIRMED | src-tauri/src/events.rs:56-62 | `playback-error` is a bare-string event. | P3
FRONT-C113 | CONFIRMED | src-tauri/src/events.rs:64-73 | `spotify-auth-complete` is a unit-payload event. | P3
FRONT-C114 | CONFIRMED | src-tauri/src/events.rs:64-73 | `teams-auth-complete` is a unit-payload event. | P3
FRONT-C115 | CONFIRMED | src-tauri/src/events.rs:56-62 | `teams-auth-failed` is a bare-string event; the frontend listener is `listen<string>`. | P3
FRONT-C116 | CONFIRMED | src-tauri/src/events.rs:56-62 | `spotify-auth-failed` is a bare-string event; the frontend listener is `listen<string>`. | P3
FRONT-C117 | CONFIRMED | src-tauri/src/events.rs:64-73 | `sync-started` is a unit-payload event. | P3
FRONT-C118 | CONFIRMED | src-tauri/src/events.rs:212-216 | `SyncStopped { self_terminated }`. | P3
FRONT-C119 | CONFIRMED | src-tauri/src/polling/state.rs:399-403 | The poller's own exit emits `self_terminated: true`. | P3
FRONT-C120 | CONFIRMED | src-tauri/src/commands/sync.rs:527-530 | `stop_syncing` emits `self_terminated: false`. | P3
FRONT-C121 | CONFIRMED | src-tauri/src/polling/state.rs; src-tauri/src/commands/sync.rs:527 | Exactly one emitter per path, so the flag is the only discriminator. | P3
FRONT-C122 | CONFIRMED | src/routes/+layout.svelte:451-453 | Consumer reads the flag with an absent-field fallback. | P3
FRONT-C123 | CONFIRMED | src-tauri/src/tray/mod.rs:331; src/lib/routes/+page.svelte:192 | `navigate` is a bare string; tray emits "settings", menu emits "dashboard"/"logs", page listens with `listen<string>`. | P3
FRONT-C124 | CONFIRMED | src-tauri/src/events.rs:64-73 | `open-logs-folder` is a unit-payload event. | P3
FRONT-C125 | CONFIRMED | src-tauri/src/events.rs:64-73 | `app-shutdown` is a unit-payload event. | P3
FRONT-C126 | CONFIRMED | src-tauri/src/events.rs:255-261 | `SpotifySecretConflict { action, message }`. | P3
FRONT-C127 | CONFIRMED | src-tauri/src/config/migrate.rs:156-162 | Emitted once per process with a fixed message naming Settings → Reconnect Spotify. | P3
FRONT-C128 | CONFIRMED | src-tauri/src/events.rs:64-73 | `show-about` is a unit-payload event. | P3
FRONT-C129 | CONFIRMED | src-tauri/src/updater_bg.rs:1441-1447 | `StageProgressEvent { downloaded, total, request_id }` — the exact three fields claimed. | P3
FRONT-C130 | CONFIRMED | src-tauri/src/updater_bg.rs:348,353,381-396 | 250 ms minimum interval and 5 % step, with the first chunk always emitting. | P3
FRONT-C131 | CONFIRMED | src-tauri/src/updater_bg.rs:1431-1447 | The caller stamps every emission with the active request id. | P3
FRONT-C132 | CONFIRMED | src-tauri/src/updater_bg.rs:1456-1466 | `StageComplete { version, request_id }`. | P3
FRONT-C133 | CONFIRMED | src-tauri/src/updater_bg.rs:1468-1476 | `emit_stage_complete` hands `emit` the staged version iff the stage committed. | P3
FRONT-C134 | CONFIRMED | docs/architecture/frontend.md:153 | `### Frontend notification throttle (C8)` heading present. | P3
FRONT-C135 | CONFIRMED | CHANGELOG.md:14-29 | 4.7.0 lists four notification classes with their own toggles (#675). | P3
FRONT-C136 | CONFIRMED | src/lib/types-generated/NotificationsConfig.ts:1 | Replaces the single `notificationsEnabled` opt-in. | P3
FRONT-C137 | CONFIRMED | src/lib/stores/notifications.ts:34-37 | The four class keys are `track_change`, `sync_stopped`, `auth_required`, `update_staged`. | P3
FRONT-C138 | CONFIRMED | src/lib/stores/config.ts:130-133 | All four default to `true`. | P3
FRONT-C139 | CONFIRMED | src/lib/stores/notifications.ts:167-169 | Pre-4.7 the single boolean governed the one class it ever covered. | P3
FRONT-C140 | CONFIRMED | src/lib/stores/notifications.ts:41,177 | `migrateLegacyNotificationPreference` exists and reads the legacy key. | P3
FRONT-C141 | CONFIRMED | src/lib/stores/notifications.ts:171-175 | The key is removed only after the value is confirmed on disk. | P3
FRONT-C142 | CONFIRMED | src/lib/stores/notifications.ts:183-188 | On a rejected save the key stays put. | P3
FRONT-C143 | CONFIRMED | src/lib/components/Dashboard.svelte:639-646 | Dashboard is the sole `spotify-track-changed` consumer that raises a toast. | P3
FRONT-C144 | CONFIRMED | src/lib/stores/notifications.ts:53,256-278 | Two guards remain: identical title+artist never notifies twice, and the 5 s throttle. | P3
FRONT-C145 | CONFIRMED | src/lib/stores/notifications.ts:254,272,278 | `lastNotifiedId` and the `"<title>::<artist>"` key. | P3
FRONT-C146 | CONFIRMED | src/lib/stores/notifications.ts:53 | `TRACK_NOTIFICATION_THROTTLE_MS = 5000`. | P3
FRONT-C147 | CONFIRMED | src/lib/stores/notifications.ts:256-278 | A throttled track returns before claiming `lastNotifiedId`. | P3
FRONT-C148 | CONFIRMED | src/lib/stores/notifications.ts:274-277 | Once the window elapses the current track can still notify. | P3
FRONT-C149 | CONFIRMED | src/routes/+layout.svelte:404-520 | The layout dispatches sync-stopped, teams-reconnect-required and update-stage-complete. | P3
FRONT-C150 | CONFIRMED | src/routes/+layout.svelte:451-453 | Notifies only on `self_terminated === true`. | P3
FRONT-C151 | CONFIRMED | src/routes/+layout.svelte:285 | A user-initiated Pause Sync stays quiet. | P3
FRONT-C152 | CONFIRMED | src/routes/+layout.svelte:285 | Skips a reconnect the user just pressed. | P3
FRONT-C153 | CONFIRMED | src/routes/+layout.svelte:519-527 | `update-stage-complete` reports a successfully staged install-on-quit update. | P3
FRONT-C154 | CONFIRMED | src/lib/stores/notifications.ts:61-64 | Each class carries a stable `id` and `group`. | P3
FRONT-C155 | CONFIRMED | src/lib/stores/notifications.ts:61-64 | Groups let the platform replace the previous notification. | P3
FRONT-C156 | CONFIRMED | src/lib/stores/notifications.ts:256-278 | One occurrence per class, no stacking timer. | P3
FRONT-C157 | CONFIRMED | docs/architecture/frontend.md:179 | `## Directory Structure` heading present. | P3
FRONT-C158 | CONFIRMED | docs/architecture/frontend.md:183 | Line present verbatim. | P3
FRONT-C159 | CONFIRMED | src/lib/components/Dashboard.svelte:1 | File exists; shows sync status and the currently-playing card. | P3
FRONT-C160 | CONFIRMED | src/lib/components/Onboarding.svelte:496-615 | Three steps (`step === 1/2/3`, `((step-1)/2)*100%`). | P3
FRONT-C161 | CONFIRMED | src/lib/components/Settings.svelte:1 | File exists; it is the config editor. | P3
FRONT-C162 | CONFIRMED | src/lib/components/Reconnect.svelte:1 | File exists; re-auth flow. | P3
FRONT-C163 | DRIFT | src/lib/components/UpdatePrompt.svelte:53,247; src-tauri/src/updater_bg.rs:1019 | The 24 h re-check is real, but it lives in this component's interval, not in `updater_bg.rs` (which documents a 30 s tick) — the v4.0 label attaches to the wrong surface. | P3
FRONT-C164 | CONFIRMED | src/lib/components/Diagnostics.svelte:1 | File exists; local diagnostics snapshot viewer. | P3
FRONT-C165 | CONFIRMED | src/lib/components/About.svelte:23-27 | Version plus description. | P3
FRONT-C166 | CONFIRMED | src/lib/components/Logo.svelte:1-5 | Inline SVG brand mark. | P3
FRONT-C167 | CONFIRMED | src/lib/components/PageHeader.svelte:6-30 | Shared header with title, back and pop-out action props. | P3
FRONT-C168 | CONFIRMED | src/lib/components/LogViewer.svelte:238,310-335 | Detachable pane with disk backfill. | P3
FRONT-C169 | CONFIRMED | src/lib/stores/app.ts:5-7 | `currentView` and `appError` classic writable stores. | P3
FRONT-C170 | CONFIRMED | src/lib/stores/config.ts:513 | `configStore` plus a `saveConfig` path. | P3
FRONT-C171 | CONFIRMED | src/lib/stores/authFlow.svelte.ts:111-130 | Four-event listener setup (spotify/teams complete + failed). | P3
FRONT-C172 | CONFIRMED | src/lib/stores/detach.ts:24-28 | `logs`/`settings` popped-out state. | P3
FRONT-C173 | CONFIRMED | src/lib/stores/theme.ts:4-7,46 | Theme store (light/dark/system) plus a density store. | P3
FRONT-C174 | CONFIRMED | src/lib/stores/presence.ts:257 | `hydrate(status, …)` seeds from `get_sync_status`. | P3
FRONT-C175 | CONFIRMED | src/lib/stores/notifications.ts:167-188 | Config-backed preferences with the legacy localStorage migration. | P3
FRONT-C176 | CONFIRMED | src/lib/types.ts:27-34 | Re-exports the ts-rs codegen. | P3
FRONT-C177 | CONFIRMED | .gitignore; src/lib/types.ts:11-15 | Gitignored and regenerated by `cargo test`. | P3
FRONT-C178 | CONFIRMED | src/lib/i18n.ts:23-30 | Eight dictionaries wired into `DICTS`. | P3
FRONT-C179 | CONFIRMED | src/lib/i18n/en.ts:1 | English dictionary plus the `Dict` type. | P3
FRONT-C180 | CONFIRMED | src/lib/i18n/de.ts:1 | German dictionary. | P3
FRONT-C181 | CONFIRMED | src/lib/i18n/fr.ts:1 | French dictionary. | P3
FRONT-C182 | CONFIRMED | src/lib/i18n/store.svelte.ts:9-16 | `config.locale` is the source of truth; localStorage is the pre-paint mirror. | P3
FRONT-C183 | CONFIRMED | src/lib/utils/boot.ts:1 | Launch gate to dashboard/onboarding/reconnect. | P3
FRONT-C184 | CONFIRMED | src/lib/utils/dev.ts:1 | `devLog()` no-op in production. | P3
FRONT-C185 | CONFIRMED | src/lib/utils/reconnect.ts:1 | Should-auto-start helper. | P3
FRONT-C186 | CONFIRMED | src/lib/utils/useAuthListeners.ts:111-130 | Shared four-event listener setup. | P3
FRONT-C187 | CONFIRMED | src/routes/+layout.js:5 | `export const ssr = false;`. | P3
FRONT-C188 | CONFIRMED | src/routes/+layout.svelte:400-520 | Always-mounted listeners for presence/sync, reconnect/update and notification dispatch. | P3
FRONT-C189 | CONFIRMED | src/routes/+page.svelte:145-199 | SPA entry that routes to views. | P3
FRONT-C190 | CONFIRMED | src/routes/detached/[pane]/+page.svelte:5-9 | Renders LogViewer/Settings in detached mode. | P3
FRONT-C191 | CONFIRMED | docs/architecture/frontend.md:222 | Line present verbatim. | P3
FRONT-C192 | DRIFT | src-tauri/src/lib.rs:1-36 | `lib.rs` is now a 34-line module registry; `run()` is in `app.rs`, `AppState` in `state.rs`, and `invoke_handler`/command registration is in `app.rs:1149 — the tree names the pre-split layout. | P2
FRONT-C193 | CONFIRMED | src-tauri/src/main.rs:6-8 | Calls `presence_jam_lib::run()`. | P3
FRONT-C194 | DRIFT | src-tauri/src/commands/mod.rs:1-10 | The commands tree is the split from `commands.rs`, but it now holds fourteen files including `rules`, `status`, `shortcuts` and `shortcut_reason` that post-date that split, so the annotation understates the tree. | P2
FRONT-C195 | CONFIRMED | src-tauri/src/commands/mod.rs:537-660 | `mod.rs` re-exports and holds the registration tests. | P3
FRONT-C196 | CONFIRMED | src-tauri/src/commands/config.rs:75,787 | `save_config` / `load_config` are in the module. | P3
FRONT-C197 | CONFIRMED | src-tauri/src/commands/spotify_auth.rs | `start_spotify_auth`, `reconnect_spotify_session`, `refresh_spotify`. | P3
FRONT-C198 | CONFIRMED | src-tauri/src/commands/teams_auth.rs | `start_teams_auth_device_code`, `poll_teams_auth`, `refresh_teams`. | P3
FRONT-C199 | CONFIRMED | src-tauri/src/commands/sync.rs | `start_syncing`, `stop_syncing`, `get_sync_status`. | P3
FRONT-C200 | CONFIRMED | src-tauri/src/commands/window.rs | `show_window`, `set_autostart_enabled`, `open_logs_folder`. | P3
FRONT-C201 | CONFIRMED | src-tauri/src/commands/onboarding.rs | `is_onboarding_complete`, `complete_onboarding`, `reconnect_teams`. | P3
FRONT-C202 | CONFIRMED | src-tauri/src/commands/playback.rs:327 | `get_spotify_granted_scopes` lives there. | P3
FRONT-C203 | CONFIRMED | src-tauri/src/commands/misc.rs | `preview_status`, `update_tray_menu_state`, `relaunch_app`. | P3
FRONT-C204 | CONFIRMED | src-tauri/src/commands/logs.rs:57-133 | `get_recent_logs` bounded on-disk tail. | P3
FRONT-C205 | CONFIRMED | src-tauri/src/commands/shortcuts.rs | `validate_accelerator`, `plan_shortcuts`, `dispatch_event` — registration, validation and rebinding. | P3
FRONT-C206 | DRIFT | src-tauri/src/polling/mod.rs:5-21,52-58 | The polling tree is the split from `polling.rs`, but it has since been refocused by issue #754 into fourteen modules, which the "PR #72" annotation no longer reflects. | P2
FRONT-C207 | CONFIRMED | src-tauri/src/polling/mod.rs:26-30,52-58 | Re-exports plus `ErrorSeverity` and the error emitters. | P3
FRONT-C208 | DRIFT | src-tauri/src/polling/loop.rs (764 lines) | The driver is real and mpsc-based, but the "~50 lines" figure is off by more than an order of magnitude — the file now carries the stop-signal wait, the skipped-state clock and the per-iteration dispatch. | P2
FRONT-C209 | DRIFT | src-tauri/src/polling/mod.rs:52 | `poll_once.rs` no longer exists — its public surface was re-exported from the focused modules when it was deleted, and the file the tree names is gone. | P2
FRONT-C210 | CONFIRMED | src-tauri/src/polling/state.rs | `start_polling` / `stop_polling` plus the panic guard. | P3
FRONT-C211 | CONFIRMED | src-tauri/src/config/schema.rs:1 | `AppConfig` struct with the ts-rs TS derive. | P3
FRONT-C212 | CONFIRMED | src-tauri/src/keychain.rs:1 | OS keychain wrapper, Secret Service on Linux. | P3
FRONT-C213 | CONFIRMED | src-tauri/src/token_io.rs:730-746 | Hand-rolled atomic write for tokens.json. | P3
FRONT-C214 | CONFIRMED | src-tauri/src/pkce.rs:1 | PKCE verifier/challenge generation. | P3
FRONT-C215 | CONFIRMED | src-tauri/src/profanity.rs:1 | Curated profanity word list. | P3
FRONT-C216 | CONFIRMED | src-tauri/src/spotify.rs; src-tauri/src/token_io.rs | PKCE OAuth client + Web API, with ts-rs-exported wire types. | P3
FRONT-C217 | CONFIRMED | src-tauri/src/teams.rs; src-tauri/src/teams.rs (DeviceCodeResponse) | Device-code + MS Graph with ts-rs-exported types. | P3
FRONT-C218 | CONFIRMED | src-tauri/src/tray/actions.rs; dedup.rs; snooze.rs; cache.rs; devices.rs; mod.rs | All six submodules exist. | P3
FRONT-C219 | CONFIRMED | src-tauri/src/updater_bg.rs:308,602 | `PendingUpdate` state and `stage_deferred_update`. | P3
FRONT-C220 | CONFIRMED | src-tauri/src/diagnostics.rs:1273 | Telemetry-free `get_diagnostics_snapshot`. | P3
FRONT-C221 | CONFIRMED | src-tauri/src/menu.rs:1 | macOS / Windows app menu bar. | P3
FRONT-C222 | CONFIRMED | src-tauri/src/i18n.rs:1 | Rust-side UI string table. | P3
FRONT-C223 | CONFIRMED | src-tauri/src/macos_deeplink.rs:1-6 | CoreServices re-claim of the scheme. | P3
FRONT-C224 | CONFIRMED | src-tauri/Cargo.toml:99 | `ts-rs = { version = "12", features = ["chrono-impl"] }`. | P3
FRONT-C225 | CONFIRMED | src-tauri/Cargo.lock:3341-3343 | `presence-jam 5.0.0` pinned in the lock. | P3
FRONT-C226 | CONFIRMED | src-tauri/tauri.conf.json | Window, deep-link, bundle and CSP config all present. | P3
FRONT-C227 | CONFIRMED | src-tauri/capabilities/default.json:1 | Capability allowlist file exists. | P3
FRONT-C228 | CONFIRMED | src-tauri/capabilities/detached.json:1 | Minimal detached-window permission set exists. | P3
FRONT-C229 | CONFIRMED | src-tauri/capabilities/default.json:1 | Main-window permission allowlist. | P3
FRONT-C230 | CONFIRMED | src-tauri/capabilities/detached.json:1 | Detached-window permission set. | P3
FRONT-C231 | CONFIRMED | .github/workflows/: ci.yml, release.yml | Both workflow files exist. | P3
FRONT-C232 | CONFIRMED | .github/workflows/ci.yml:1 | PR-time Rust, Vitest coverage and Playwright browser gates. | P3
FRONT-C233 | CONFIRMED | .github/workflows/release.yml:1 | Tag verification + 3-OS build + package-manager publication. | P3
FRONT-C234 | CONFIRMED | homebrew/presence-jam.rb:1 | Tap formula template exists. | P3
FRONT-C235 | CONFIRMED | tests/browser/*.spec.ts | Browser specs live under `tests/browser/`. | P3
FRONT-C236 | CONFIRMED | playwright.config.ts:5,22-29 | Test directory, Vite dev server and both projects. | P3
FRONT-C237 | CONFIRMED | vitest.config.js:86-90 | Four coverage thresholds. | P3
FRONT-C238 | CONFIRMED | rust-toolchain.toml:1 | Pinned Rust toolchain. | P3
FRONT-C239 | CONFIRMED | src/app.css:45,100 | Design tokens, themes and densities. | P3
FRONT-C240 | CONFIRMED | docs/: PLATFORMS.md, RELEASING.md, STATE-OF-FEATURES.md, architecture/ | The named doc families exist. | P3
FRONT-C241 | CONFIRMED | package.json:1 | Node deps + scripts. | P3
FRONT-C242 | CONFIRMED | package-lock.json:1 | Committed npm lockfile. | P3
FRONT-C243 | CONFIRMED | svelte.config.js:1 | `adapter-static` SPA config. | P3
FRONT-C244 | CONFIRMED | vite.config.js:1 | Vite + Tauri dev server config. | P3
FRONT-C245 | CONFIRMED | jsconfig.json:1 | TypeScript config file exists. | P3
FRONT-C246 | CONFIRMED | docs/architecture/frontend.md:279-281 | Caveat sentence present. | P3
FRONT-C247 | CONFIRMED | src/lib/types.ts:27-34 | `SpotifyTokens`, `TrackInfo`, `TeamsTokens`, `DeviceCodeResponse`, `SyncStatus`, `AppConfig` all re-exported. | P3
FRONT-C248 | CONFIRMED | src/lib/types.ts:11-15 | Generated from Rust at `cargo test` time. | P3
FRONT-C249 | CONFIRMED | src-tauri/src/events.rs:12-14 | Derives with `#[ts(export, export_to = "../../src/lib/types-generated/")]`. | P3
FRONT-C250 | CONFIRMED | src-tauri/src/events.rs:1-6 | A Rust-side rename fails `npm run check` rather than shipping `undefined`. | P3
FRONT-C251 | CONFIRMED | src-tauri/src/config/schema.rs:1 | `AppConfig` re-exported from the generated directory. | P3

## DEFECTS (non-CONFIRMED, by blast radius)

Ordered P2 → P3, and within a tier by how far the defect propagates (system-diagram nodes before single-line annotations).

```
claim_id: OVW-C059
file: docs/architecture/overview.md:62
class: C1
verdict: DRIFT
evidence: src-tauri/src/polling/mod.rs:5-21,52-58
finding: The diagram's Polling node describes a four-module subsystem ("loop (driver) + state (lifecycle) + poll_once (single-source-of-truth iteration) + mod.rs"). `poll_once.rs` was deleted under issue #754 and replaced by `iteration.rs`; the subsystem is now fourteen focused modules (clocks, timing, refresh, gate, rules, presence, status_text, write, exit, daemon, plus loop/iteration/state/mod). `mod.rs` does still carry `ErrorSeverity` and the error emitters, so that half of the label survives.
proposed_fix: Replace the node body with "polling/ submodule<br/>loop + iteration (one poll) + state (lifecycle)<br/>12 more focused modules + mod.rs (ErrorSeverity, emit_error)" and link polling.md for the full list.
severity: P2
```

```
claim_id: OVW-C058
file: docs/architecture/overview.md:61
class: C1
verdict: OVERSTATED
evidence: src-tauri/src/commands/ (14 files: mod, config, logs, misc, onboarding, playback, rules, shortcut_reason, shortcuts, spotify_auth, status, sync, teams_auth, window)
finding: The Backend Commands node names nine command families; the real tree has fourteen files including `rules`, `status` and `shortcut_reason`, which have no node representation anywhere in the diagram. Every named family is real, so the node is correct as far as it goes — it just presents a subset as the whole.
proposed_fix: Add the three missing families to the node label ("config / spotify_auth / teams_auth / sync / window / onboarding / playback / misc / logs / rules / status / shortcuts") or append "+ rules, status, shortcuts".
severity: P2
```

```
claim_id: FRONT-C192
file: docs/architecture/frontend.md:224
class: C1
verdict: DRIFT
evidence: src-tauri/src/lib.rs:1-36 (34-line module registry); src-tauri/src/state.rs (AppState); src-tauri/src/app.rs:1149 (`invoke_handler`)
finding: The tree annotates `lib.rs` as "Tauri entry, command registration, AppState". `lib.rs` is now a slim module registry plus two `pub use` re-exports; `run()` lives in `app.rs` and `AppState` in `state.rs`. This is exactly the pre-split path the audit was scoped to catch.
proposed_fix: Replace with three entries: "lib.rs   # module registry (pub use app::run; pub use state::{AppState, …})", "app.rs   # Tauri entry: setup, invoke_handler, command registration", "state.rs # AppState + shared state types".
severity: P2
```

```
claim_id: FRONT-C208
file: docs/architecture/frontend.md:240
class: C1
verdict: DRIFT
evidence: src-tauri/src/polling/loop.rs (764 lines, 21 `pub` items)
finding: The tree annotates `loop.rs` as "driver (mpsc channel, ~50 lines)". The file is 764 lines and carries the driver plus the stop-signal wait, the skipped-state clock handling and the per-iteration dispatch; it was described when it was a thin wrapper and has since grown two orders of magnitude past the stated size.
proposed_fix: Change the annotation to "driver (mpsc channel, stop-signal wait + per-iteration dispatch)" — drop the "~50 lines" figure so the number cannot drift again.
severity: P2
```

```
claim_id: FRONT-C209
file: docs/architecture/frontend.md:241
class: C1
verdict: DRIFT
evidence: src-tauri/src/polling/mod.rs:52 ("Issue #754: `poll_once.rs` is deleted; its public surface is re-exported from the focused modules")
finding: The directory tree names a `poll_once.rs` file that no longer exists. The single-source-of-truth iteration now lives in `polling/iteration.rs`, re-exported through the name `poll_once` as a module path (`pub(crate) use iteration::run_oneshot`), so the *name* survives in code comments while the *file* does not.
proposed_fix: Replace the `poll_once.rs` line with "iteration.rs  #   single source of truth for one poll iteration (replaces #754's poll_once.rs)".
severity: P2
```

```
claim_id: FRONT-C206
file: docs/architecture/frontend.md:238
class: C1
verdict: DRIFT
evidence: src-tauri/src/polling/mod.rs:5-21,52-58
finding: The tree annotates `polling/` as "Split from polling.rs (PR #72)". PR #72 was the *original* four-file split; the subsystem has since been restructured by issue #754 into fourteen focused modules, which the annotation no longer reflects.
proposed_fix: Change to "Split from polling.rs (PR #72); refocused into 14 modules by #754".
severity: P2
```

```
claim_id: FRONT-C194
file: docs/architecture/frontend.md:226
class: C1
verdict: DRIFT
evidence: src-tauri/src/commands/mod.rs:1-10
finding: The tree annotates `commands/` as "Split from commands.rs (PR #76)". The tree now contains fourteen files including `rules`, `status`, `shortcuts` and `shortcut_reason`, which post-date that split, so the annotation understates the tree's actual extent.
proposed_fix: Change to "Split from commands.rs (PR #76; grown with rules/status/shortcuts/shortcut_reason since)".
severity: P2
```

```
claim_id: FRONT-C013
file: docs/architecture/frontend.md:19
class: C2
verdict: DRIFT
evidence: src-tauri/src/keychain.rs:807 (`const TOKENS_AES_KEY_USER: &str = "tokens_aes_key:com.presencejam.app"`)
finding: The bullet names the two slots descriptively but not literally, while the sibling bullets and OVW.md both name `tokens_aes_key:com.presencejam.app` explicitly. A reader cannot tell which keychain entry the presence flag is keyed on, which matters because the diagnostics page renders that flag to the user.
proposed_fix: Name both slots: "(the Spotify `client_secret` slot and the `tokens_aes_key:com.presencejam.app` slot)".
severity: P2
```

```
claim_id: OVW-C041
file: docs/architecture/overview.md:45
class: C2
verdict: OVERSTATED
evidence: .github/workflows/release.yml:246-291 (macOS + Windows + Linux legs); src-tauri/src/sources/ (spotify, mpris, smtc); src-tauri/src/polling/daemon.rs
finding: The Platform bullet names "Windows + macOS + Linux" as the platform surface but omits the `--daemon` supervisor, the MPRIS/SMTC playback sources and the headless CLI modes that all carry platform-specific behaviour and are part of what the tree actually supports.
proposed_fix: Extend the bullet: "Windows + macOS + Linux, plus a headless `--daemon` mode and MPRIS (Linux) / SMTC (Windows) playback sources".
severity: P3
```

```
claim_id: FRONT-C163
file: docs/architecture/frontend.md:190
class: C2
verdict: DRIFT
evidence: src/lib/components/UpdatePrompt.svelte:53,247; src-tauri/src/updater_bg.rs:1019
finding: The tree credits the silent 24 h re-check to v4.0, but the 24 h tick is implemented in the frontend component (`CHECK_INTERVAL_MS = 24 * 60 * 60 * 1000` driving a `setInterval`), while `updater_bg.rs`'s own background-check path uses a different interval (:1019 documents a 30 s tick). The version label attaches to the wrong surface.
proposed_fix: Re-word to "silent 24 h re-check + install-on-quit (re-check driven from this component's 24 h interval)" and drop or relocate the v4.0 tag.
severity: P3
```

Not raised as a defect — `FRONT-C063` (`EXTERNAL-UNVERIFIED`): the claim that "WebKit has none [scroll anchoring] and ignores the property" is a browser-engine fact with no citation in the claim file and no allowlisted vendor source. The repo evidence is consistent with it (`LogViewer.svelte:634` sets `overflow-anchor: none` and the Playwright WebKit project runs the same spec), but the engine claim itself stays unverified rather than confirmed. Resolving it needs a WebKit/WebKit-features source, which is outside the allowlist — flagging rather than fetching.

## COUNTS

### Verdict tally

| Verdict | Count |
| --- | --- |
| CONFIRMED | 363 |
| DRIFT | 8 |
| OVERSTATED | 2 |
| EXTERNAL-UNVERIFIED | 1 |
| STALE | 0 |
| MISSING | 0 |
| UNSOURCED | 0 |
| **Total** | **374** |

Per-file split:

| File | Claims | CONFIRMED | DRIFT | OVERSTATED | EXTERNAL-UNVERIFIED |
| --- | --- | --- | --- | --- | --- |
| `docs/audit/claims/OVW.md` (overview.md) | 123 | 120 | 1 | 2 | 0 |
| `docs/audit/claims/FRONT.md` (frontend.md) | 251 | 243 | 7 | 0 | 1 |
| **Total** | **374** | **363** | **8** | **2** | **1** |

Defects raised: 10 (all in SECTION B). Nine of the eleven non-CONFIRMED verdicts have a matching defect entry; the tenth, `FRONT-C063`, is `EXTERNAL-UNVERIFIED` (a browser-engine fact with no allowlisted vendor source) and is discussed at the end of SECTION B rather than raised as a repo defect. No CONFIRMED claim has a defect entry.

### Severity tally (defects only)

| Severity | Count | Claim IDs |
| --- | --- | --- |
| P0 | 0 | — |
| P1 | 0 | — |
| P2 | 8 | OVW-C058, OVW-C059, FRONT-C013, FRONT-C192, FRONT-C194, FRONT-C206, FRONT-C208, FRONT-C209 |
| P3 | 2 | OVW-C041, FRONT-C163 |

No P0 or P1. No claim is a false security/privacy/token-behaviour statement, a CI break, a data-loss path, a credential leak or a build failure: the audit's 16 key verification targets (atomic-write helper names, `MAX_LOG_LINES` 500, `LOG_TAIL_MAX_BYTES` 256 KiB, `MAX_BUFFER` 500, `RENDER_WINDOW` 100, `TRACK_NOTIFICATION_THROTTLE_MS` 5 s, the four notification classes, the 256 KiB snapshot cap, every event name and payload shape, the CI job names) all resolved against source and matched.

No claim in either file was `MISSING` or `UNSOURCED`: every claim had a locatable citation in the document on disk.

### The FRONT.md scope gap

`docs/architecture/frontend.md` was **outside the audit's declared scope** — the audit named `overview.md` and the six-page architecture set, and `frontend.md` is one of the six indexed pages under `docs/architecture/`, but the claims file `docs/audit/claims/FRONT.md` tags all 251 of its claims `scope_gap: true`. Two distinct things are true and worth separating:

1. **The page is not hidden.** It is linked from `ARCHITECTURE.md` as the sixth subject page, and `ARCHITECTURE.md`'s "Where a section went" table maps five anchors (`#local-diagnostics-page-v40`, `#log-viewer-v46`, `#event-bus`, `#frontend-notification-throttle-c8`, `#directory-structure`) onto it. `overview.md` also cites it directly at line 13. So the gap is in the *audit's* claim inventory, not in the repo's navigation — `frontend.md` is fully discoverable.
2. **The gap still cost coverage.** The audit's declared scope named `overview.md` only for the docs-grounding pass, so `frontend.md` carried 251 claims — 67 % of the 374 in this file — with the `scope_gap` flag as the only marker. All 251 were verified anyway, as instructed.

The cost is concrete: of the eleven non-CONFIRMED verdicts, **eight are in `frontend.md`**, and seven of those eight were raised as defects (six `P2`). Every one of the seven is a path or size citation that the refactor invalidated — `lib.rs` (now a module registry; `run()` and `AppState` moved to `app.rs`/`state.rs`), `poll_once.rs` (deleted under #754 in favour of `iteration.rs`), `loop.rs` ("~50 lines" for a 764-line file), and the PR-number annotations on the `commands/` and `polling/` splits. `overview.md`'s three are different in kind: two are completeness gaps in the system diagram (a missing command family, a four-module description of a fourteen-module subsystem) and one is a platform bullet that understates the surface.

Recommendation, outside the scope of this audit but recorded here: the `polling/` node label in `overview.md` and the four stale annotations in `frontend.md`'s tree should be fixed in one pass, since they describe the same refactor. The event-bus table at `frontend.md:124-151` is the single most-verified surface in the file — all 44 of its claims confirmed against `src-tauri/src/events.rs` and the frontend listeners, including the `user_initiated` marker on `teams-reconnect-required` and the `self_terminated` marker on `sync-stopped` — and needs no change.

### Orchestrator facts independently re-verified

| Fact | Result |
| --- | --- |
| `HEAD=09341ecaad732e78454a2c65b383f0dfd1541d5a` | Confirmed — `git log --oneline -1` returns `09341ec test(sync,scans): reorder-mutation proof + Why comments on every surviving scan (#778) (#1170)`. |
| Tree clean | Confirmed apart from untracked `docs/audit/`, `docs/__pycache__/` and `docs/._.DS_Store` (audit scaffolding and a macOS metadata file); no tracked file modified. |
| Versions all 5.0.0 | Confirmed — `src-tauri/tauri.conf.json` `5.0.0`, `package.json` `5.0.0`, `src-tauri/Cargo.toml` `5.0.0`, `src-tauri/Cargo.lock` `presence-jam 5.0.0`. |
| Latest tag `v4.7.0` | Not re-run by this agent (tag lists were read from workflows, not `git tag`). Flagged as unverified rather than confirmed. |
| `link-audit` exits 0 | **Contradicted** — `python3 docs/link-audit.py` exits **1** with 138 broken links, all of them inside `docs/audit/claims/*.md` (the claim files use paths like `./docs/architecture/overview.md` from within `docs/audit/claims/`, so the relative depth is wrong by two levels). Every real documentation link in `overview.md` and `frontend.md` resolves correctly, including both `../../ARCHITECTURE.md` links and the `frontend.md#directory-structure` anchor. The 138 failures are audit-scaffolding artefacts, not repo defects — but the orchestrator's "exits 0" claim does not hold for the tree as it stands. |
| `cargo check` exits 0 | Not re-run by this agent (a full `cargo check --all-targets` was out of budget). Flagged as unverified rather than confirmed. |
| `npm run check` FAILS | Not re-run by this agent. Flagged as unverified rather than confirmed; consistent with the generated-types caveat in `src/lib/types.ts:11-15` if `cargo test --lib` has not been run in this checkout. |

### Files written

This report is the only file created or modified. No source, test, config, workflow, version or CHANGELOG file was touched; no commit was made; no token, key or secret was read into or written out of this report. Evidence citations point at `src-tauri/src/`, `src/`, `tests/` and `.github/workflows/` only.
