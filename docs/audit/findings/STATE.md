# STATE-OF-FEATURES.md — docs-grounding audit findings

Audited: `docs/STATE-OF-FEATURES.md` (272 lines) against the on-disk tree at
HEAD `09341ecaad732e78454a2c65b383f0dfd1541d5a`.

Evidence method: `grep`/`cargo test --lib -- --list`/`npm test`/`svelte-check`
over `src-tauri/src/`, `src/`, `tests/`, `.github/workflows/`,
`src-tauri/tauri.conf.json`, `src-tauri/capabilities/*.json`,
`src-tauri/Cargo.{toml,lock}`, `package.json`, `package-lock.json`,
`CHANGELOG.md`, and the `v4.7.0` tag tree.

Two orchestrator "facts" do not reproduce on this tree and are recorded here
for the parent audit (they are not STATE claims, so they are not in the
verdict table):

- **`npm run check` does NOT fail.** `npm run check` exits **0** with
  "svelte-check found 0 errors and 1 warning in 1 file"
  (`src/lib/components/LogViewer.svelte:523:5`,
  `a11y_no_noninteractive_tabindex`). `npx vitest run` is 456/456 green,
  `npm run test:coverage` is 74.88/65.54/78.97/73.56 (above the
  68.91/62.57/73.33/68.02 ratchet), `cargo test --lib` is 915/915 green, and
  `cargo check --all-targets` is clean.
- **`python3 docs/link-audit.py` does NOT exit 0.** It exits **1** with 138
  broken links — but **all 138** are inside the audit's own scaffolding
  (`docs/audit/claims/*.md`, where the extracted claim files sit one directory
  deeper than the docs they quote, so their relative links and `#anchor`s
  resolve against the wrong base). Excluding `docs/audit/`, the audit reports
  **0** broken links across 119 files / 306 relative links / 82 anchors. The
  gate is clean on the real tree; the failures are an artifact of claim
  extraction.

Also recorded for the parent, because it is load-bearing for several STATE
rows: the tree was refactored after these rows were written.
`src-tauri/src/lib.rs` is now a 33-line module registry
(`src-tauri/src/lib.rs:1-33`); `config.rs` is the `src-tauri/src/config/`
directory (`config/{mod,schema,clamp,snooze,patch,migrate,io,transfer}.rs`,
re-exported from `config/mod.rs:6-50`); `polling/poll_once.rs` is deleted and
replaced by `polling/{clocks,iteration,refresh,gate,rules,presence,status_text,write,timing,exit,state,loop_,daemon}.rs`;
`tray.rs` is `src-tauri/src/tray/{mod,actions,cache,dedup,devices,snooze,testkit}.rs`.


## VERDICTS

CLAIM_ID | VERDICT | evidence | one-sentence finding | severity
---|---|---|---|---
STATE-C001 | CONFIRMED | docs/STATE-OF-FEATURES.md:1; src-tauri/Cargo.toml:3; package.json:3; src-tauri/tauri.conf.json:4 | Title says v5.0.0 and all three manifests are 5.0.0. | —
STATE-C002 | CONFIRMED | docs/STATE-OF-FEATURES.md:3 | Tagline prose present verbatim. | —
STATE-C003 | CONFIRMED | docs/STATE-OF-FEATURES.md:251-257 | The "Have not been verified end-to-end (Verify with maintainer…)" section backs the no-guess claim. | —
STATE-C004 | CONFIRMED | CHANGELOG.md:8-10; docs/STATE-OF-FEATURES.md:8 | The Unreleased block is maintained, so "updated on every release" holds in practice. | —
STATE-C005 | CONFIRMED | docs/STATE-OF-FEATURES.md:9-10 | Maintenance-intent prose present verbatim. | —
STATE-C006 | CONFIRMED | docs/STATE-OF-FEATURES.md:12 | Heading present verbatim. | —
STATE-C007 | CONFIRMED | docs/STATE-OF-FEATURES.md:14 | Table header row present verbatim. | —
STATE-C008 | CONFIRMED | src-tauri/src/spotify.rs:667,845; src-tauri/src/spotify.rs:416 | Both `https://accounts.spotify.com/api/token` POSTs and the documented currently-playing endpoint are present. | —
STATE-C009 | CONFIRMED | src-tauri/src/teams.rs:790 | `POST https://graph.microsoft.com/v1.0/me/presence/setStatusMessage` is present. | —
STATE-C010 | CONFIRMED | src-tauri/src/spotify.rs:649; src-tauri/src/teams.rs | The PKCE token exchange and the device-code rounds live in those two modules. | —
STATE-C011 | CONFIRMED | docs/STATE-OF-FEATURES.md:16 | Row label present verbatim. | —
STATE-C012 | CONFIRMED | docs/STATE-OF-FEATURES.md:17 | Row label and its ⚠ Partial marker present verbatim. | —
STATE-C013 | CONFIRMED | src-tauri/src/commands/onboarding.rs:167,455,459 | `session_verdict`, `spotify_session_verdict` and `teams_session_verdict` all exist in `commands/onboarding.rs`. | —
STATE-C014 | CONFIRMED | src-tauri/src/commands/onboarding.rs:167-179 | `session_verdict` refreshes only for a locally-expired token and maps `Dead`/`Unavailable` to `ReauthRequired`; transient failures stay `Valid`. | —
STATE-C015 | CONFIRMED | src/lib/utils/boot.ts:18-24 | `bootView(complete, hasSpotifyCredentials)` returns `reconnect` for an incomplete-but-configured install. | —
STATE-C016 | CONFIRMED | src/lib/utils/reconnect.ts; src/lib/components/Reconnect.svelte | `shouldAutoStartSpotifyReconnect` exists and is consumed by Reconnect.svelte. | —
STATE-C017 | CONFIRMED | src-tauri/src/commands/onboarding.rs:160-179; tests/onboarding.test.ts; tests/reconnect.test.ts | Decision-table tests exist; the > 1 h relaunch leg is external to the tree. | —
STATE-C018 | CONFIRMED | docs/STATE-OF-FEATURES.md:210,231-237 | The 4.7.0 record explicitly states the smoke was not run for this row. | —
STATE-C019 | CONFIRMED | docs/STATE-OF-FEATURES.md:213-229 | The record lists startup, `--status` and `--sync-once` exit codes as exercised, plus unit tests. | —
STATE-C020 | DRIFT | src-tauri/src/polling/timing.rs:254 | `poll_once.rs` is deleted; the 5000 ms buffer now lives in `polling/timing.rs`. | P2
STATE-C021 | CONFIRMED | src-tauri/src/polling/timing.rs:254 | ~235 s per 4-min song, consistent with the "~240 s" approximation. | —
STATE-C022 | DRIFT | src-tauri/src/polling/timing.rs:292-302 | `pause_backoff` exists with the exact cited signature and 30→60→120→ceiling ladder, but in `polling/timing.rs`, not the deleted `polling/poll_once.rs`. | P2
STATE-C023 | CONFIRMED | src-tauri/src/polling/iteration.rs:1039 | `not_modified_iteration` resets `consecutive_pauses = 0`, matching the 304 reset claim. | —
STATE-C024 | CONFIRMED | src-tauri/src/config/clamp.rs:24; src-tauri/src/polling/timing.rs:295 | Ceiling clamped to 60–3600 and floored at the base interval by `ceiling_secs.max(default_secs)`. | —
STATE-C025 | CONFIRMED | arithmetic over src-tauri/src/polling/timing.rs:292 | 2880 calls/day at 30 s and ~288-291 at a 300 s ceiling reproduce the stated ~10× reduction. | —
STATE-C026 | CONFIRMED | src-tauri/src/profanity.rs | Normalization, leetspeak, separator, Unicode, compound and boundary rules are all present. | —
STATE-C027 | CONFIRMED | src-tauri/src/profanity.rs:368-415 | `MatchMemo` keys on the full `(text index, word index, stretched, separator-skipped)` failed state. | —
STATE-C028 | CONFIRMED | src-tauri/src/profanity.rs:885-891 | The adversarial regression asserts `memo.explored() <= 128`. | —
STATE-C029 | DRIFT | src-tauri/src/app.rs:562-581 | `start_minimized` is read in `app.rs::setup_config`, not the `lib.rs` setup block. | P2
STATE-C030 | CONFIRMED | src-tauri/src/app.rs:579-581 | The `#[cfg(target_os = "macos")]` `ActivationPolicy::Accessory` call is present in that path. | —
STATE-C031 | DRIFT | src-tauri/src/app.rs:756-778 | `register_all()` is called in `app.rs::setup_deep_links`, not in `lib.rs`. | P2
STATE-C032 | CONFIRMED | src-tauri/src/app.rs:840-873 | The Linux `xdg-mime` fallback association is present alongside the Windows HKCU write. | —
STATE-C033 | CONFIRMED | src-tauri/src/macos_deeplink.rs:209 | `LSSetDefaultHandlerForURLScheme` call is present and latched. | —
STATE-C034 | CONFIRMED | src-tauri/src/app.rs:766-799; src-tauri/src/deep_link.rs:64 | The #66 re-claim wiring and its comment chain are present. | —
STATE-C035 | DRIFT | src-tauri/src/config/io.rs; src-tauri/src/config/mod.rs:22 | `atomic_write_json` lives in `config/io.rs`; `config.rs` no longer exists as a file. | P2
STATE-C036 | CONFIRMED | docs/architecture/overview.md:20 | The storage bullet naming `config.rs::save_config()` → `atomic_write_json()` is present verbatim. | —
STATE-C037 | CONFIRMED | docs/architecture/storage-and-config.md:38 | The doc exists and discusses `save_config` / atomic-write semantics. | —
STATE-C038 | CONFIRMED | src-tauri/src/token_io.rs:613-615 | `write_tokens_atomic` exists with the temp-file + rename pattern. | —
STATE-C039 | CONFIRMED | src-tauri/src/token_io.rs:19,63-74 | The on-disk layout `b"PJENC" + version byte 0x01 + 12-byte random nonce + AES-256-GCM ciphertext` matches exactly. | —
STATE-C040 | CONFIRMED | src-tauri/src/keychain.rs; src-tauri/src/token_io.rs:66 | `tokens_aes_key:com.presencejam.app` and `get_or_create_tokens_aes_key` are present. | —
STATE-C041 | CONFIRMED | src-tauri/src/token_io.rs (test `plaintext_migrates_to_ciphertext_on_read`) | The ≤ v2.10.0 plaintext migration on first read is present and tested. | —
STATE-C042 | CONFIRMED | src-tauri/src/config/mod.rs:95 | `test_default_config` asserts `!config.teams.availability_sync`. | —
STATE-C043 | CONFIRMED | src-tauri/src/teams.rs:1253,1235 | `setPresence` POST with the `/users/{id}` fallback is present. | —
STATE-C044 | EXTERNAL-UNVERIFIED | (vendor: learn.microsoft.com/graph/*) | "An `Available` session times out after 5 min" is a Graph contract claim; the allowlisted Graph page was not fetched, so it is unverified here. | P3
STATE-C045 | EXTERNAL-UNVERIFIED | (vendor: learn.microsoft.com/graph/*) | The `PT5M`–`PT4H` `expirationDuration` bounds are a Graph contract claim; not fetched here. | P3
STATE-C046 | CONFIRMED | src-tauri/src/polling/presence.rs; src-tauri/src/app.rs:1298 | `presence_expiration_duration` and the `RunEvent::Exit` clear are present. | —
STATE-C047 | OVERSTATED | src-tauri/src/polling/exit.rs; src-tauri/src/polling/state.rs | "`clear_presence_on_exit` has no test at all" is false — `test_clear_presence_on_exit_reads_the_snapshot_not_the_clocks` and `test_exit_cleanup_survives_the_loop_exit_tail` both exist and pass. | P1
STATE-C048 | CONFIRMED | src-tauri/src/app.rs:1287-1291 | The Windows update-driven quit never reaching the presence cleanup is documented in that arm. | —
STATE-C049 | CONFIRMED | src-tauri/src/config/mod.rs:96 | `test_default_config` asserts `config.teams.presence_gate`. | —
STATE-C050 | CONFIRMED | src-tauri/src/teams.rs; src-tauri/src/polling/presence.rs | `presence_gate_reason` covers busy/DND/focusing/in-a-meeting/in-a-call/presenting. | —
STATE-C051 | CONFIRMED | src-tauri/src/polling/presence.rs; src-tauri/src/config/schema.rs | `respect_manual_status`, `gate_when_out_of_office` and `presence_gate_decision` are all present. | —
STATE-C052 | CONFIRMED | src/lib/components/Dashboard.svelte:43-65 | All seven specific labels plus the generic fallback are present in `gatedReasonLabel`. | —
STATE-C053 | CONFIRMED | cargo test --lib (tests named in doc) | `test_manual_status_blocks_write_policy`, `test_presence_gate_decision_precedence_and_opt_ins` and `out_of_office_gates_only_when_opted_in` all exist and pass. | —
STATE-C054 | CONFIRMED | src-tauri/src/tray/actions.rs; src-tauri/src/tray/devices.rs; src-tauri/src/commands/playback.rs | Play/Pause, Previous, Next, Shuffle and Repeat actions plus the Devices submenu are present. | —
STATE-C055 | CONFIRMED | src-tauri/src/tray/devices.rs; src-tauri/src/tray/mod.rs | The Devices submenu and the ≤3-track Up Next peek are present. | —
STATE-C056 | CONFIRMED | src-tauri/src/sources/spotify.rs:131-141 | `NotPremium` maps into the auth/reconnect path. | —
STATE-C057 | CONFIRMED | src-tauri/src/commands/playback.rs; src-tauri/src/tray/mod.rs | `player_with_refresh_typed` is the shared typed refresh helper. | —
STATE-C058 | DRIFT | src-tauri/src/app.rs:1085 | `tauri_plugin_updater` is registered in `app.rs::run`, not `lib.rs`. | P2
STATE-C059 | CONFIRMED | src-tauri/src/updater_bg.rs:907,915 | `update_endpoints` lists the beta asset before the stable fallback. | —
STATE-C060 | CONFIRMED | .github/workflows/release.yml:581-592,669-698 | The hand-assembled `latest.json` and the independent `sign` job are present. | —
STATE-C061 | DRIFT | src-tauri/src/config/patch.rs; src-tauri/src/config/schema.rs; src-tauri/src/config/mod.rs | `src-tauri/src/config.rs` is now the `config/` directory; the `#[ts(export)]` derives live in `config/patch.rs` and `config/schema.rs`, not a `config.rs` file. | P2
STATE-C062 | CONFIRMED | src-tauri/src/events.rs:84-300 | All named payload structs are `#[ts(export)]` in `events.rs`. | —
STATE-C063 | CONFIRMED | src-tauri/src/events.rs:8; src-tauri/src/config/patch.rs:12 | The `#[ts(export, export_to = "../../src/lib/types-generated/")]` derive and its cargo-test codegen are present. | —
STATE-C064 | CONFIRMED | docs/architecture/frontend.md:179 | The `## Directory Structure` heading exists, so `#directory-structure` resolves. | —
STATE-C065 | CONFIRMED | .github/workflows/release.yml:246,266-284 | The three-way `macos-latest`/`windows-latest`/`ubuntu-latest` matrix is present. | —
STATE-C066 | CONFIRMED | .github/workflows/release.yml:270-284 | dmg on macOS, exe+msi on Windows, deb+rpm+AppImage on Linux — matches the matrix. | —
STATE-C067 | CONFIRMED | src-tauri/src/diagnostics.rs:39,826,1021 | `get_diagnostics_snapshot`, `LOG_TAIL_LINES = 50` and the redact/strip pipeline are all present. | —
STATE-C068 | CONFIRMED | src-tauri/src/diagnostics.rs:578,1048 | `strip_absolute_paths` handles POSIX/Windows/UNC paths and also covers the failed-install marker. | —
STATE-C069 | CONFIRMED | src-tauri/src/diagnostics.rs:196,202 | `config_quarantined` and bare-name `config_quarantine_backup` are present. | —
STATE-C070 | CONFIRMED | src-tauri/src/diagnostics.rs (test `test_build_snapshot_is_secret_free_with_fake_tokens`) | A regression test asserts injected token values never survive serialization. | —
STATE-C071 | DRIFT | src-tauri/src/polling/iteration.rs; src-tauri/src/polling/loop.rs; src-tauri/src/sources/spotify.rs | `poll_once.rs` is deleted; the ETag store/echo and 304 short-circuit now live in `sources/spotify.rs` and `polling/iteration.rs`. | P2
STATE-C072 | CONFIRMED | src-tauri/src/spotify.rs:416 | The row carries an explicit empirical-evidence note naming the vendor doc URL. | —
STATE-C073 | CONFIRMED | src/routes/detached/[pane]/+page.svelte; tests/about-build.test.ts:108 | The logs/settings/unknown-pane branches render and are tested. | —
STATE-C074 | CONFIRMED | src/lib/stores/detach.ts:97; src-tauri/capabilities/default.json | `detach_pane` is invoked from the store, and the main capability grants no webview-window creation. | —
STATE-C075 | CONFIRMED | src-tauri/capabilities/detached.json | `core:window:allow-close` plus the mirrored minimal set are present. | —
STATE-C076 | CONFIRMED | src/routes/+layout.svelte:55 | Listeners are window-label-guarded by `isMainWindow`. | —
STATE-C077 | CONFIRMED | src-tauri/src/app.rs:1149-1203 (54 entries); src-tauri/src/commands/mod.rs | The `generate_handler!` list holds exactly 54 commands and the matrix test exists. | —
STATE-C078 | DRIFT | src-tauri/src/app.rs:1149; src-tauri/src/commands/mod.rs:560 | The handler list is brace-counted out of `app.rs`, not `lib.rs` — `lib.rs` no longer contains it. | P2
STATE-C079 | CONFIRMED | src/routes/+layout.svelte:620; src/lib/components/UpdatePrompt.svelte:162,475 | `UpdatePrompt` mounts only under `{#if isMainWindow}` and both commands are invoked only there. | —
STATE-C080 | CONFIRMED | src-tauri/src/menu.rs:315; src-tauri/src/tray/mod.rs:331; src-tauri/src/commands/teams_auth.rs:100 | `navigate` is emitted from the menu, the tray and Teams auth success with `dashboard`/`settings` payloads. | —
STATE-C081 | CONFIRMED | src-tauri/src/tray/mod.rs; src-tauri/src/tray/dedup.rs | Live tooltip on rebuild and the CheckMenuItems are present. | —
STATE-C082 | CONFIRMED | src-tauri/src/tray/dedup.rs (test `playback_modes_feed_both_toggle_items`) | Both toggles are driven by the `playing_flag`/`shuffle_flag`/`repeat_flag` mirrors. | —
STATE-C083 | UNSOURCED | (no `b82f515` reference resolvable in-tree) | The commit SHA `b82f515` is not reachable/cited by any source line in the tree, so the dock-badge row's evidence is not verifiable here. | P3
STATE-C084 | CONFIRMED | src-tauri/src/tray/mod.rs; src-tauri/src/tray/snooze.rs | `ID_PAUSE_SYNC`/`ID_RESUME_SYNC` and the `ID_SNOOZE_*` submenu are present. | —
STATE-C085 | CONFIRMED | src-tauri/src/i18n.rs:218; src-tauri/src/tray/actions.rs (test `sync_status_line_reports_backend_state_in_every_locale`) | `status_syncing` / `status_syncing_no_track` and the composed status line are present. | —
STATE-C086 | CONFIRMED | src-tauri/src/i18n.rs; src-tauri/src/tray/snooze.rs (test `snooze_status_line_counts_down_in_every_locale`) | The words come from `i18n::Strings` and the snooze line is a separate entry. | —
STATE-C087 | CONFIRMED | src/lib/components/Settings.svelte:51-66 | Unsaved-changes tracking and per-section reset are present; the deep compare is now number-safe post-#765. | —
STATE-C088 | CONFIRMED | src/app.css:494; src/lib/components/settings/PollingCard.svelte; src/lib/components/settings/StatusFormatCard.svelte; src/lib/components/settings/RulesCard.svelte | `.clamp-hint` is the shared rule and all three cards carry clamp feedback. | —
STATE-C089 | CONFIRMED | src/lib/components/settings/ (14 .svelte files) | `SettingsCard` shell plus exactly thirteen cards are present. | —
STATE-C090 | CONFIRMED | src/lib/components/settings/*.svelte; CHANGELOG.md | The per-card `$bindable()`/`onreset`/`actions` split matches the tree. | —
STATE-C091 | CONFIRMED | src/lib/components/Settings.svelte:57-66,476-511,623 | Draft state, save/discard, `pendingNav` and the footer are present. | —
STATE-C092 | CONFIRMED | src-tauri/src/config/mod.rs:6-50 | `schema`/`clamp`/`snooze`/`patch`/`migrate`/`io`/`transfer` are declared and re-exported. | —
STATE-C093 | CONFIRMED | src-tauri/src/config/mod.rs (121 `#[test]`); config slices (0 each) | 121 config tests centralized in `mod.rs`, zero in the slices. | —
STATE-C094 | CONFIRMED | src-tauri/src/redact.rs:111-119; src-tauri/src/config/schema.rs | `redact.rs` aggregates the 8 config slices through one `concat!`, and `LoggingConfig` lives only in `schema.rs`. | —
STATE-C095 | DRIFT | cargo test --lib → 915 passed | "config 121/121" is right but "full 910/910" is stale — the tree runs 915/915. | P2
STATE-C096 | DRIFT | src-tauri/src/polling/mod.rs:35-48 | `poll_once.rs` is deleted as claimed, but `mod.rs` declares 14 modules (adding `daemon` and `state`), not the 10 named; `PollState` exists (`polling/clocks.rs:92`). | P2
STATE-C097 | DRIFT | src-tauri/src/polling/iteration.rs:1531 | 121 polling tests confirmed, but the `../tray/actions.rs` scanner is in `polling/iteration.rs:1531`, not the deleted `poll_once.rs:11874`. | P2
STATE-C098 | CONFIRMED | src/lib/stores/notifications.ts:53,61,274 | 5 s throttle and the stable replace-in-place id/group are present. | —
STATE-C099 | CONFIRMED | src/lib/stores/notifications.ts:168-180 | The legacy `localStorage.notificationsEnabled` → `track_change` migration lands only after the write. | —
STATE-C100 | CONFIRMED | src/lib/stores/notifications.ts:53,274 | `TRACK_NOTIFICATION_THROTTLE_MS` = 5000 and the stable id are both present. | —
STATE-C101 | CONFIRMED | src/app.css:619; src/lib/components/Dashboard.svelte:1788 | `prefers-reduced-motion` guards are present; skip link and darkened tokens ship. | —
STATE-C102 | CONFIRMED | src/lib/components/About.svelte:24,107; tests/version-contrast.test.ts | The version label paints with `var(--fg-muted)` and the contrast test exists. | —
STATE-C103 | CONFIRMED | src/app.css:144,435 | The dedicated `--border-input` token paints the shared input/textarea/select border. | —
STATE-C104 | CONFIRMED | src/app.css:144; tests/form-control-contrast.test.ts | The dark-theme ratios are stated in `app.css:144` and the test asserts the computed ratio per theme. | —
STATE-C105 | CONFIRMED | src/app.css (decorative `--border` unchanged) | The decorative divider token is untouched by the `--border-input` change. | —
STATE-C106 | CONFIRMED | tests/form-control-contrast.test.ts | The test resolves the painted token per theme and asserts the ratio. | —
STATE-C107 | CONFIRMED | src/app.css:341,351 | `.btn-sm` and `.btn-quiet` sit beside `.btn-secondary`. | —
STATE-C108 | CONFIRMED | src/lib/components/Dashboard.svelte:1157,1258; src/lib/components/UpdatePrompt.svelte:619,629 | Dashboard refresh/snooze-resume and the banner actions all use the shared classes. | —
STATE-C109 | CONFIRMED | src/lib/components/Dashboard.svelte:1781,1788; src/lib/components/UpdatePrompt.svelte:815 | No component CSS sets `filter:` (comment-only mentions) and the reduced-motion override is present. | —
STATE-C110 | CONFIRMED | tests/hygiene.test.ts; tests/browser/button-primitives.spec.ts; playwright.config.ts:20,24 | Both test files exist and the browser gate runs Chromium + WebKit. | —
STATE-C111 | CONFIRMED | src-tauri/src/tray/mod.rs:130-160 | `menu_event_id_for_log` preserves known IDs and reduces payload-bearing/unknown IDs. | —
STATE-C112 | CONFIRMED | src-tauri/src/menu.rs; tests: `menu_event_log_redacts_payload_and_unknown_ids`, `unknown_app_menu_event_record_redacts_device_id`, `dispatched_menu_event_record_redacts_device_id` | The app-menu branch uses the same helper and both formatter and dispatcher records are tested. | —
STATE-C113 | CONFIRMED | src-tauri/capabilities/default.json | The main-window set contains only core/window/event plus updater/notification grants; no global-shortcut or autostart grant. | —
STATE-C114 | CONFIRMED | src-tauri/tauri.conf.json (csp); src-tauri/src/updater_bg.rs:1906-1930 | Both `base-uri 'self'` and `form-action 'none'` ship and are pinned by a config test. | —
STATE-C115 | CONFIRMED | src/lib/components/Dashboard.svelte; src/lib/utils/useAuthListeners.ts | `spotify-track-changed` is consumed as `TrackInfo` and forwarded to notifications. | —
STATE-C116 | CONFIRMED | src/lib/utils/useAuthListeners.ts:43-84 | The disposed guard covers late registrations and teardown releases them. | —
STATE-C117 | CONFIRMED | src-tauri/src/teams.rs:90-120 | `teams_write_error_policy` classifies retryable/permission/reconnect exactly as described. | —
STATE-C118 | CONFIRMED | src/lib/components/Dashboard.svelte; src/lib/utils/useAuthListeners.ts | The retry banner renders only for a Teams warning carrying `retry_scheduled`. | —
STATE-C119 | CONFIRMED | src-tauri/src/pkce.rs; src-tauri/src/deep_link.rs:52 | `LaunchBinding`/`validate_and_consume` and the constant-time `ct_eq` compare are present. | —
STATE-C120 | CONFIRMED | src-tauri/src/pkce.rs (12 tests) | Replay/wrong/truncated/malformed states are unit-tested. | —
STATE-C121 | CONFIRMED | src-tauri/src/redact.rs:21-30,111-119 | `redact_len` owns the `[REDACTED len N]` construction and all cited sites route through it. | —
STATE-C122 | CONFIRMED | src-tauri/src/redact.rs:7-12 | The 4-char prefix is documented as deleted; no public function prints a secret prefix. | —
STATE-C123 | DRIFT | src-tauri/src/redact.rs:82-140 | `redaction_literal_is_built_only_in_redact_rs` exists, but `call_site_output_matches_helper_format` does not — the equivalent test is `diagnostics_redaction_embeds_helper_output_for_keyed_value`. | P2
STATE-C124 | CONFIRMED | src/lib/components/UpdatePrompt.svelte:50-53 | `CHECK_INTERVAL_MS = 24 * 60 * 60 * 1000` and the console-only failure path are present. | —
STATE-C125 | CONFIRMED | src-tauri/src/updater_bg.rs:1499,1832 | `stage_deferred_update` stages in memory and `install_pending_on_exit` applies it. | —
STATE-C126 | CONFIRMED | src-tauri/src/updater_bg.rs:1714; tests in updater_bg | Request-scoped cancellation discards post-cancel completions. | —
STATE-C127 | CONFIRMED | docs/STATE-OF-FEATURES.md:53,231-237 | Both the row and the 4.7.0 record state the published smoke is still pending. | —
STATE-C128 | UNSOURCED | .github/workflows/release.yml:561,665 | `release.yml` does use SHA-pinned `actions/attest-build-provenance`, but no run URL or repo evidence substantiates "exercised live on the v4.0.0 and v4.1.0 tag runs". | P3
STATE-C129 | CONFIRMED | .github/workflows/release.yml:11-16 | `workflow_dispatch` takes a `tag` input allowing re-cuts of existing `v*` tags. | —
STATE-C130 | CONFIRMED | src-tauri/src/token_io.rs (tests `keychain_unavailable_retry_*`, `persist_holds_both_token_guards_before_cloning`) | Blocked ciphertext, corrupt-retry snapshot, tombstones and dual-slot guards are all tested. | —
STATE-C131 | CONFIRMED | src-tauri/src/token_io.rs | `clear_*_if_current` semantics are implemented and tested. | —
STATE-C132 | DRIFT | src-tauri/src/commands/misc.rs; src-tauri/src/commands/sync.rs; src-tauri/src/polling/status_text.rs | Locale-resolved paused/stopped fallbacks are wired through all three surfaces, but the stopped fallback lives in `polling/status_text.rs`, not the cited `polling/poll_once.rs`. | P2
STATE-C133 | CONFIRMED | src/lib/i18n/store.svelte.ts:206-207 | Locale writes are serialized latest-wins with canonical default provenance. | —
STATE-C134 | CONFIRMED | src-tauri/src/commands/teams_auth.rs; src-tauri/src/polling/mod.rs:57 | Device-code polling and CAS seams are split into testable cores and exposed as typed seams. | —
STATE-C135 | CONFIRMED | src-tauri/src/commands/teams_auth.rs (9 tests); src-tauri/src/polling/refresh.rs | Behavioral tests cover interval clamping, persistence warnings and commit ordering. | —
STATE-C136 | CONFIRMED | src-tauri/src/diagnostics.rs:135-160; .github/workflows/ci.yml:202; .github/workflows/release.yml:205 | OS release + install flavour, the focusable LogViewer viewport, and Chromium + WebKit installs in both workflows are present. | —
STATE-C137 | CONFIRMED | package.json (deps); src-tauri/Cargo.toml; src-tauri/Cargo.lock; package-lock.json | No `tauri-plugin-shell`/`tauri-plugin-store` in any lockfile or manifest. | —
STATE-C138 | CONFIRMED | src-tauri/capabilities/*.json; src-tauri/src/ | No shell/store imports or capability grants exist. | —
STATE-C139 | CONFIRMED | src-tauri/Cargo.lock:1831-1833; git commit ed88008 | `h2 0.4.18` is locked and `ed88008` is "fix(deps): bump h2 to 0.4.18 (RUSTSEC-2026-0258)". | —
STATE-C140 | CONFIRMED | src/lib/i18n/ (8 .ts + store.svelte.ts); src/lib/components/settings/AppearanceCard.svelte | The `Dict`-typed dictionaries and the language picker persisted to `config.locale` are present. | —
STATE-C141 | CONFIRMED | src-tauri/src/i18n.rs:603; src/lib/i18n/store.svelte.ts | The Rust `LOCALES` list and the webview store agree on the eight locales; `localStorage.locale` is the pre-paint mirror. | —
STATE-C142 | CONFIRMED | src/lib/i18n/store.svelte.ts; tests/i18n.test.ts:518,589,602 | CLDR `tCount`/number-format retagging and cross-window convergence are all present and tested. | —
STATE-C143 | CONFIRMED | tests/i18n.test.ts:241 (26 tests total) | The typed `WEEKDAY_KEYS` registry is pinned by a named test. | —
STATE-C144 | CONFIRMED | src-tauri/src/i18n.rs | Rust-side error strings remain English; the row states it as a documented limitation. | —
STATE-C145 | DRIFT | src-tauri/src/polling/write.rs; src-tauri/src/polling/refresh.rs | `teams_write_with_optional_refresh` exists, but in `polling/write.rs`, not the deleted `polling/poll_once.rs`. | P2
STATE-C146 | CONFIRMED | src-tauri/src/polling/write.rs; src-tauri/src/polling/refresh.rs | The refresh + CAS-commit + persist + single retry sequence and its error classification are present. | —
STATE-C147 | CONFIRMED | src-tauri/src/polling/write.rs | The paused arm emits `emit_error(Warning)` for transient failures. | —
STATE-C148 | CONFIRMED | src-tauri/src/commands/playback.rs | `player_with_refresh` is the shared helper for the player commands plus devices/queue. | —
STATE-C149 | CONFIRMED | src-tauri/src/tray/mod.rs; src-tauri/src/commands/playback.rs; tests `tray_player_actions_use_refresh_aware_token` | The tray uses the typed `player_with_refresh_typed` core. | —
STATE-C150 | CONFIRMED | src/lib/stores/authFlow.svelte.ts | `expiresAtFromResponse` and `formatCountdownMs` are both present. | —
STATE-C151 | CONFIRMED | src/lib/components/Onboarding.svelte; src/lib/components/DeviceCodeBox.svelte; src/lib/components/Reconnect.svelte; src/lib/components/settings/TeamsCard.svelte | All three countdown surfaces exist with the expired state and Get-new-code button. | —
STATE-C152 | CONFIRMED | src-tauri/src/updater_bg.rs:1637 | `install_pending_on_exit` skips `v{} <= current v{}` when not forced. | —
STATE-C153 | CONFIRMED | src/lib/components/UpdatePrompt.svelte | `stageForQuit(force)` with candidate-vs-current and the Install-anyway override is present. | —
STATE-C154 | CONFIRMED | src-tauri/tauri.conf.json:70; src-tauri/src/updater_bg.rs:1906 | `allowDowngrades: false` is set and pinned by a test. | —
STATE-C155 | DRIFT | src-tauri/src/config/migrate.rs; src-tauri/src/app.rs:596 | `decide_legacy_secret_outcome` lives in `config/migrate.rs` and is wired from `app.rs::setup_secret_migration`, not `lib.rs`. | P2
STATE-C156 | CONFIRMED | src-tauri/src/config/migrate.rs; src-tauri/src/app.rs:596-610 | The one-time `spotify-secret-conflict` event and plaintext-preserving migration are present. | —
STATE-C157 | DRIFT | src-tauri/src/spotify.rs:645-647; src-tauri/src/http.rs:112-127 | `build_spotify_client` no longer sets the 10 s timeout or the User-Agent itself — it delegates to `http::shared_client()`; the timeout and `PresenceJam/<version>` UA now live in `http.rs::build_client`. | P2
STATE-C158 | CONFIRMED | src-tauri/src/commands/spotify_auth.rs:199-211 | `validate_spotify_redirect_uri` is an exact match against `presencejam://callback`. | —
STATE-C159 | CONFIRMED | src-tauri/src/teams.rs | Device-code error/parse paths log lengths only and receipts carry `expires_in`/`interval`. | —
STATE-C160 | CONFIRMED | src-tauri/src/diagnostics.rs:826,900-910; test `test_tail_log_file_error_status_has_no_absolute_path_or_username` | Snapshot statuses carry only the file name; full paths stay local. | —
STATE-C161 | CONFIRMED | src-tauri/src/config/schema.rs; src-tauri/src/config/migrate.rs | `schema_version` (default 1) and the flattened `extra` retention are present. | —
STATE-C162 | CONFIRMED | src-tauri/src/config/patch.rs; src/lib/stores/config.ts; src/lib/components/Onboarding.svelte | `apply_patch`/`update_config` and `mergeWizardConfig` are both present. | —
STATE-C163 | CONFIRMED | src-tauri/src/config/io.rs | Fixed-name `config.json.bak` quarantine (never timestamped) plus in-place schema migration is present. | —
STATE-C164 | CONFIRMED | src-tauri/src/diagnostics.rs:196-202; src/lib/components/Diagnostics.svelte | Both quarantine fields and the dismissible amber banner are present. | —
STATE-C165 | DRIFT | src-tauri/src/polling/write.rs | `debounce_active`/`DEBOUNCE_RETRY_SECONDS` exist, but in `polling/write.rs`, not `polling/poll_once.rs`. | P2
STATE-C166 | DRIFT | src-tauri/src/polling/write.rs; src-tauri/src/polling/clocks.rs | `should_skip_identical_write` lives in `polling/write.rs`, not the deleted `poll_once.rs`. | P2
STATE-C167 | DRIFT | src-tauri/src/polling/gate.rs | `gate_recheck_due`/`last_gate_check` live in `polling/gate.rs`, not the deleted `poll_once.rs`. | P2
STATE-C168 | DRIFT | src-tauri/src/polling/write.rs; src-tauri/src/polling/refresh.rs | `teams_token_for_write` lives in `polling/write.rs`, not the deleted `poll_once.rs`. | P2
STATE-C169 | CONFIRMED | src-tauri/src/spotify.rs:751,2548 | The exact string `token response omitted refresh_token - please try signing in again.` is surfaced. | —
STATE-C170 | CONFIRMED | src-tauri/src/commands/spotify_auth.rs:167-185 | 32-char lower bound, 512 upper bound and ASCII-alphanumeric-only validation are all present. | —
STATE-C171 | CONFIRMED | src-tauri/src/commands/spotify_auth.rs (21 tests) | Manual-code peek-then-take and legacy parse-before-keychain are covered. | —
STATE-C172 | CONFIRMED | src/lib/components/LogViewer.svelte:39-50,238 | `MAX_BUFFER = 500`, `RENDER_WINDOW = 100`, `SCROLL_THRESHOLD = 48`, and on-disk backfill are all present. | —
STATE-C173 | DRIFT | src/lib/components/LogViewer.svelte:667; src/app.css:80 | The grid is `var(--log-col-ts) max-content 1fr`, not the literal `88px max-content 1fr` — `--log-col-ts` resolves to `88px` in `app.css`. | P3
STATE-C174 | CONFIRMED | tests/logviewer.test.ts; tests/browser/logviewer.spec.ts | Both test files exist as cited. | —
STATE-C175 | DRIFT | src-tauri/src/config/schema.rs; src-tauri/src/polling/rules.rs | `StatusRulesConfig` lives in `config/schema.rs` and the rule hooks in `polling/rules.rs`, not the cited pre-split paths. | P2
STATE-C176 | CONFIRMED | src-tauri/src/polling/gate.rs; src-tauri/src/polling/write.rs | `quiet_hours_active_now` and `quiet_gate_entry_due` are present. | —
STATE-C177 | CONFIRMED | docs/STATE-OF-FEATURES.md:76 | The row is explicitly marked ⚠ Partial with the live-tenant caveat stated. | —
STATE-C178 | DRIFT | src-tauri/src/polling/write.rs; src-tauri/src/polling/presence.rs | The gate re-check branch lives in `polling/write.rs`, not the deleted `poll_once.rs`. | P2
STATE-C179 | CONFIRMED | src/app.html; src/lib/stores/theme.ts:103; src/lib/i18n/store.svelte.ts:360 | Pre-paint bootstrap plus `storage` listeners for theme and locale are present. | —
STATE-C180 | CONFIRMED | src/lib/stores/detach.ts | Zombie-flag clearing and the refused-Pop-back-in behaviour are present. | —
STATE-C181 | CONFIRMED | tests/about-build.test.ts:108-118 | The detached logs/settings/unknown branches are rendered and asserted. | —
STATE-C182 | CONFIRMED | src/lib/components/LogViewer.svelte (copySnapshot) | The snapshot copies solely from `get_diagnostics_snapshot.recent_logs` plus version/platform. | —
STATE-C183 | CONFIRMED | src/lib/components/LogViewer.svelte | The live buffer is never pasted; virtualization is deferred. | —
STATE-C184 | CONFIRMED | src/lib/components/Dashboard.svelte:43-65 | All seven specific reasons plus the generic-label fallbacks are present. | —
STATE-C185 | CONFIRMED | src/lib/stores/presence.ts:82-96; src/lib/components/Dashboard.svelte:35 | `gatedReason` is component-local while `gated` is store-held, making the empty-reason path reachable. | —
STATE-C186 | OVERSTATED | tests/dashboard.test.ts:266 | The test asserts the per-reason string `t('dashboard.presenceGatedQuietHours')`, so "covers the chip's presence/absence, not the per-reason strings" understates what the suite checks. | P3
STATE-C187 | CONFIRMED | vitest.config.js; playwright.config.ts; package.json:12-14 | Both runners and their commands are wired as described. | —
STATE-C188 | CONFIRMED | .github/workflows/ci.yml:202,269; .github/workflows/release.yml:205,229 | PR CI and release verification both install the browser and run the geometry gate. | —
STATE-C189 | CONFIRMED | tests/version-contrast.test.ts | The contrast regression remains Vitest while evaluating production CSS in both themes. | —
STATE-C190 | CONFIRMED | .github/workflows/ci.yml:28-35,477,524 | macOS + Windows check legs, gitleaks secret-scan and cargo/npm advisory jobs are all present. | —
STATE-C191 | CONFIRMED | src-tauri/Cargo.toml:72,78,88 | `directories = "6"`, `rand = "0.9"` with `try_fill_bytes`, and the single keyring feature set are all present. | —
STATE-C192 | CONFIRMED | src-tauri/src/spotify.rs:1762-1806 | `placeholder_values` builds exactly 13 tokens and `substitute_placeholders` runs a single pass. | —
STATE-C193 | CONFIRMED | src-tauri/src/spotify.rs:1769-1804 | `{context}` aliases `{playlist}`; shuffle/repeat are icon-only; progress is `M:SS` and empty when `None`. | —
STATE-C194 | CONFIRMED | src/lib/i18n/en.ts:148,348 | Both `settings.placeholdersHint` and `onboarding.placeholdersHint` exist and list the music tokens. | —
STATE-C195 | CONFIRMED | src-tauri/src/spotify.rs | `map_media_item` reads the item's own `type` against the documented track/episode union and fills `show.name`/`show.publisher` into the artist/album slots. | —
STATE-C196 | DRIFT | src-tauri/src/polling/write.rs; src-tauri/src/spotify.rs | `DEFAULT_EPISODE_STATUS_FORMAT` exists (in `spotify.rs`, consumed in `polling/write.rs`), but the cited `poll_once::process_track` path no longer exists. | P2
STATE-C197 | CONFIRMED | src-tauri/src/spotify.rs; src-tauri/src/tray/devices.rs | The `Ad` / `Unknown` mapper gate returning "nothing playing" and the ad-dropping queue mapper are present. | —
STATE-C198 | CONFIRMED | src-tauri/src/config/schema.rs (no such key) | There is no `teams.episode_status_format` config key; the episode template is a built-in constant. | —
STATE-C199 | CONFIRMED | src-tauri/src/tray/dedup.rs; src-tauri/src/polling/iteration.rs | `note_playback_modes` writes the shuffle/repeat flags from the poll body with no extra request. | —
STATE-C200 | CONFIRMED | src-tauri/src/tray/cache.rs (test `repeat_menu_label_spells_out_the_mode`) | The inverse-shuffle and `RepeatState::next()` targets plus the spelled-out label are present. | —
STATE-C201 | CONFIRMED | src-tauri/src/tray/actions.rs; src-tauri/src/tray/dedup.rs | A rejected command records nothing and surfaces on `playback-error`. | —
STATE-C202 | CONFIRMED | src-tauri/src/updater_bg.rs | Progress, completion and cancellation are bound to the exact stage request id and generation. | —
STATE-C203 | CONFIRMED | src-tauri/src/updater_bg.rs | All three cancel arms (in-flight, committed, pre-begin) are implemented and tested. | —
STATE-C204 | CONFIRMED | src-tauri/src/keychain.rs:139-148 | `KeychainPresence::Present`/`Absent`/`Unavailable(help)` are all preserved as distinct states. | —
STATE-C205 | CONFIRMED | src-tauri/src/keychain.rs:50,157-165 | A fresh `Present` observation is reused for 30 s (`SPOTIFY_CLIENT_SECRET_PRESENCE_TTL`); cold/expired falls back to the direct probe. | —
STATE-C206 | CONFIRMED | src-tauri/src/commands/logs.rs:45,52,132 | `MAX_LOG_LINES 500`, `LOG_TAIL_MAX_BYTES 256 KiB`, `spawn_blocking` and the partial-first-line drop are all present. | —
STATE-C207 | CONFIRMED | src/lib/components/LogViewer.svelte:203-206,238,331 | The listener registers before the seed, which is then prepended and re-clamped to `MAX_BUFFER`. | —
STATE-C208 | CONFIRMED | src/lib/components/LogViewer.svelte (seedCancelled) | The raw-seed decision and the `seedCancelled` Clear path are present. | —
STATE-C209 | DRIFT | src-tauri/src/app.rs:778-800 | `macos_deeplink::claim` is reached from `app.rs::setup_deep_links`, not `lib.rs`'s setup block. | P2
STATE-C210 | CONFIRMED | src-tauri/src/app.rs:798 | The scheme list is read from `tauri.conf.json` via `macos_deeplink::configured_schemes`. | —
STATE-C211 | CONFIRMED | src-tauri/src/macos_deeplink.rs | The claim is latched once per process and failures are log-only. | —
STATE-C212 | CONFIRMED | .github/workflows/release.yml:78-160 | `resolve-tag` validates tag existence and all six version literals including lockfiles. | —
STATE-C213 | CONFIRMED | .github/workflows/release.yml:162-230,246-247 | The `verify` job reruns the full Linux gate set and `build` declares `needs: [resolve-tag, verify]`. | —
STATE-C214 | CONFIRMED | .github/workflows/ci.yml:444-475 | The PR-time version-consistency twin job is present. | —
STATE-C215 | DRIFT | src-tauri/src/polling/iteration.rs; src-tauri/src/polling/loop.rs | The `SourceError::Auth` classification lives in `polling/iteration.rs`, not the deleted `poll_once.rs`. | P2
STATE-C216 | CONFIRMED | src-tauri/src/polling/timing.rs:30,35 | `NETWORK_FAILURE_THRESHOLD = 12` and `NETWORK_BACKOFF_CAP_SECONDS = 300` are exact. | —
STATE-C217 | CONFIRMED | src-tauri/src/polling/timing.rs:42-48 | `record_success` resets both the transient and network failure counters. | —
STATE-C218 | CONFIRMED | src/lib/stores/presence.ts:1-96 | A module-level store holds the posted status / gate flag / availability; the view is destroyed per switch. | —
STATE-C219 | CONFIRMED | src/lib/components/Dashboard.svelte:35 | `statusPreview` is derived from `$presence.postedStatus`. | —
STATE-C220 | CONFIRMED | src-tauri/src/commands/sync.rs:143-163; src/routes/+layout.svelte:620; src/lib/stores/presence.ts:257-265 | The three `SyncStatus` fields, the always-mounted layout listeners, and the revision-guarded `hydrate()` are all present. | —
STATE-C221 | CONFIRMED | src/routes/+layout.svelte:55,232,564-577 | The two remaining listeners are handled in the always-mounted layout behind `isMainWindow`. | —
STATE-C222 | CONFIRMED | src-tauri/src/commands/sync.rs | `sync_status_from_state` holds the four guards in the documented order and releases them before the tail work. | —
STATE-C223 | CONFIRMED | src-tauri/src/state.rs:886,929; src-tauri/src/commands/sync.rs:669,762 | `last_sync_snapshot` is published by the fresh path only and names only the contended slot. | —
STATE-C224 | CONFIRMED | tests `test_sync_status_offload_yields_instead_of_parking_the_command_thread`, `test_snapshot_serves_the_previous_instant_while_a_token_slot_is_held` | The sibling test holds the Teams write guard and asserts a bounded, field-for-field-equal answer. | —
STATE-C225 | CONFIRMED | test `test_sync_status_section_acquires_guards_in_documented_order` | The acquisition order is pinned behaviourally, and the prose comment is rewordable without failing the suite. | —
STATE-C226 | CONFIRMED | src-tauri/src/diagnostics.rs:317-324,787-788 | `ConfigQuarantine::observe` reads the process flag plus a `.bak` existence probe and injects both at the command boundary. | —
STATE-C227 | CONFIRMED | src/lib/components/Diagnostics.svelte; src/lib/i18n/en.ts | The amber banner renders from either fact, names the bare backup file and is dismissible. | —
STATE-C228 | CONFIRMED | tests/diagnostics-quarantine.test.ts | The quarantine banner is driven by a dedicated test file. | —
STATE-C229 | DRIFT | src-tauri/src/config/clamp.rs | `PRESENCE_COMBINATIONS` and `normalize_presence_pair` live in `config/clamp.rs`, not `config.rs`. | P2
STATE-C230 | DRIFT | src-tauri/src/polling/rules.rs; src-tauri/src/polling/mod.rs:68-70 | `rule_gate_at` and `RuleDecision` live in `polling/rules.rs`, not `poll_once.rs`. | P2
STATE-C231 | CONFIRMED | src-tauri/src/polling/presence.rs | `arm_presence_session` is the single `setPresence` call site and `should_arm_presence` arms on a changed pair. | —
STATE-C232 | CONFIRMED | tests `test_rule_actions_suppress_replace_and_set_presence`, `test_should_arm_presence_switches_immediately_and_keeps_cadence`, `test_normalize_presence_pair_accepts_only_documented_combinations` | All three cited unit tests exist and pass; the live-tenant caveat is stated. | —
STATE-C233 | DRIFT | src-tauri/src/polling/presence.rs | `manual_status_blocks_write` lives in `polling/presence.rs`, not `poll_once.rs`. | P2
STATE-C234 | CONFIRMED | src-tauri/src/polling/presence.rs | Authorship is decided by content identity; `publishedDateTime` is not modelled. | —
STATE-C235 | CONFIRMED | src-tauri/src/polling/presence.rs | A failed read fails open and the guard protects the status message only. | —
STATE-C236 | CONFIRMED | src-tauri/src/polling/presence.rs; src-tauri/src/polling/timing.rs | The gated branch returns `playing_track_sleep(...)` and the loop keeps polling. | —
STATE-C237 | CONFIRMED | tests `test_manual_status_blocks_write_policy`, `test_presence_gate_decision_precedence_and_opt_ins` | The truth-table tests exist; the live-tenant caveat is stated. | —
STATE-C238 | CONFIRMED | src-tauri/src/config/schema.rs (`default_gate_when_out_of_office`) | The field exists with the documented default-off helper. | —
STATE-C239 | CONFIRMED | src-tauri/src/teams.rs | Both `outOfOfficeSettings.isOutOfOffice` and the `outOfOffice` activity are parsed, case-insensitively and fail-safe on a missing object. | —
STATE-C240 | CONFIRMED | src-tauri/src/polling/presence.rs; src-tauri/src/polling/write.rs | `ooo_gate_enabled(config, rule.presence.is_some())` lets a rule's own pair override the OOO gate. | —
STATE-C241 | CONFIRMED | tests `out_of_office_gates_only_when_opted_in`, `parse_presence_body_carries_status_message_and_ooo` | Both cited unit tests exist and pass. | —
STATE-C242 | DRIFT | src-tauri/src/polling/presence.rs | `presence_expiration_duration` lives in `polling/presence.rs`, not `poll_once.rs`. | P2
STATE-C243 | DRIFT | src-tauri/src/app.rs:1286-1298 | The `RunEvent::Exit` arm lives in `app.rs::run`, not `lib.rs`. | P2
STATE-C244 | CONFIRMED | src-tauri/src/polling/state.rs; tests `test_exit_cleanup_survives_the_loop_exit_tail`, `test_clear_presence_on_exit_reads_the_snapshot_not_the_clocks` | The `ExitSnapshot` decision source and both pinning tests exist. | —
STATE-C245 | CONFIRMED | src-tauri/src/app.rs:1287-1291 | The Windows update-driven quit never reaching the clear is documented at the arm. | —
STATE-C246 | CONFIRMED | src/lib/components/settings/*.svelte; src-tauri/src/config/*.rs | All three fields are exposed in Settings with clamp feedback and consumed by the poller. | —
STATE-C247 | CONFIRMED | src-tauri/src/config/clamp.rs (`MAX_RULE_STATUS_CHARS`); src-tauri/src/polling/write.rs | The replacement is posted from both clear paths, capped at 160, and is part of the change fingerprint. | —
STATE-C248 | DRIFT | src-tauri/src/profanity.rs:749 | The production function is `filter_status_for_locale(text, placeholder, is_playing, extra_words, locale)`; the bare 4-arg `filter_status` cited is a `#[cfg(test)]` helper only. | P2
STATE-C249 | CONFIRMED | src-tauri/src/config/schema.rs; src-tauri/src/config/clamp.rs | `polling.pause_backoff_max_seconds` is the pause-ladder ceiling as described. | —
STATE-C250 | CONFIRMED | src/lib/i18n/en.ts:462,468 | `settings.extraWordsHint` and `settings.pauseBackoffClampHint` both exist and describe the implemented behaviour. | —
STATE-C251 | CONFIRMED | src-tauri/src/tray/snooze.rs; src-tauri/src/config/snooze.rs | The submenu IDs, the RFC3339 UTC `snooze_until`, and `clear_expired_snooze_at_startup` are all present. | —
STATE-C252 | DRIFT | src-tauri/src/polling/gate.rs | `clear_snooze_if_expired` lives in `polling/gate.rs`, not `poll_once.rs`. | P2
STATE-C253 | CONFIRMED | src-tauri/src/config/io.rs; src-tauri/src/config/snooze.rs | `load_config` reports `snooze_expired_deadline` without holding a write guard. | —
STATE-C254 | DRIFT | src-tauri/src/polling/gate.rs | `snooze_gate` lives in `polling/gate.rs`, not `poll_once.rs`. | P2
STATE-C255 | CONFIRMED | src-tauri/src/config/snooze.rs (`next_local_midnight_utc`) | "Until tomorrow" is the next local midnight, never `now + 24 h`. | —
STATE-C256 | CONFIRMED | src/lib/components/Dashboard.svelte | The countdown chip with the **Resume now** action is present. | —
STATE-C257 | CONFIRMED | src-tauri/src/commands/shortcuts.rs:38-45; src-tauri/src/config/schema.rs:881,884 | Two slots with exactly the documented default accelerators, stored in `ShortcutsConfig`. | —
STATE-C258 | DRIFT | src-tauri/src/commands/shortcuts.rs; src-tauri/src/Cargo.toml:100 | `apply_plan` registers through `tauri-plugin-global-shortcut` from `commands/shortcuts.rs`; `lib.rs` is only the module registry and holds no registration. | P2
STATE-C259 | CONFIRMED | src-tauri/src/commands/shortcuts.rs; src/lib/i18n/en.ts (`settings.shortcutRegistrationFailed`) | Per-slot non-fatal registration and the Settings reason string are present. | —
STATE-C260 | OVERSTATED | docs/STATE-OF-FEATURES.md:102 | The row's text is truncated mid-sentence ("`dispatch_event` routes playback thr…"), so the claim is incomplete and unverifiable as written. | P2
STATE-C261 | CONFIRMED | src-tauri/src/cli.rs:8-12,745-760; src-tauri/src/commands/sync.rs:143 | All three flags parse and behave as described, including the exit codes. | —
STATE-C262 | CONFIRMED | src-tauri/src/cli.rs:88-120,500-520 | `--status`/`--help` run before any Tauri app is built; `--sync-once` needs the poller's runtime. | —
STATE-C263 | CONFIRMED | src-tauri/src/cli.rs:76-90 | Unrecognised arguments are ignored and fall through to the GUI, as do `--minimized` and a `presencejam://` URL. | —
STATE-C264 | CONFIRMED | src-tauri/src/app.rs:374-394; .github/workflows/ci.yml:152,171 | The Windows parent-console attach is present and `windows-cli-smoke` asserts `--help` via `cmd /c`. | —
STATE-C265 | DRIFT | src-tauri/src/config/schema.rs (`lenient_update_channel`); src-tauri/src/updater_bg.rs:915 | `UpdateChannel::{Stable, Beta}` is lowercase-serialized and lenient to `Stable` on an unknown value, but it lives in `config/schema.rs`, not the cited `config.rs`. | P2
STATE-C266 | CONFIRMED | src-tauri/src/updater_bg.rs:907,915-930 | The beta asset is listed before the stable fallback exactly as described. | —
STATE-C267 | CONFIRMED | src-tauri/src/updater_bg.rs:1372; src/lib/components/UpdatePrompt.svelte | `check_for_update` resolves the candidate in Rust and the Beta path replaces download-and-relaunch with install-on-quit. | —
STATE-C268 | DRIFT | src-tauri/src/app.rs:194 | `log_rotation_strategy` lives in `app.rs`, not `lib.rs`. | P2
STATE-C269 | CONFIRMED | src-tauri/src/config/clamp.rs (`clamp_logging`); src-tauri/src/config/schema.rs | `max_file_size_mb` 1–500 (default 10) and `keep_files` 1–20 (default 3) are both present. | —
STATE-C270 | CONFIRMED | src-tauri/src/app.rs:194-210; src/lib/components/settings/LoggingCard.svelte | `KeepSome(n)` counts archived files only, leaving at most `keep_files + 1`, and the card states this. | —
STATE-C271 | CONFIRMED | src/lib/components/settings/LoggingCard.svelte | The card carries `enabled`, `log_level` and the open-folder action. | —
STATE-C272 | DRIFT | src-tauri/src/config/schema.rs; src-tauri/src/config/clamp.rs | `days`/`start_minutes`/`end_minutes` with the documented defaults and `clamp_track_rule_window` are present, but in `config/schema.rs` and `config/clamp.rs`, not the cited `config.rs`. | P2
STATE-C273 | DRIFT | src-tauri/src/polling/rules.rs | The first-match array-order walk lives in `polling/rules.rs`, not `poll_once.rs`. | P2
STATE-C274 | CONFIRMED | src-tauri/src/config/clamp.rs (`TRACK_RULE_DAY_MINUTES`); tests in polling/rules.rs | Midnight-crossing windows are evaluated per starting day. | —
STATE-C275 | DRIFT | src-tauri/src/config/schema.rs; src-tauri/src/polling/gate.rs; src-tauri/src/polling/iteration.rs | `QuietHoursEntry.pause_polling` defaults false and skips the iteration without touching clocks or parking the thread, but it lives in `config/schema.rs`, not the cited `config.rs`. | P2
STATE-C276 | CONFIRMED | src-tauri/src/config/transfer.rs | Import validation, plaintext-`client_secret` rejection and the `.import.tmp` sidecar are present. | —
STATE-C277 | CONFIRMED | src-tauri/src/config/transfer.rs; src-tauri/src/config/io.rs | Live-file move to `config.json.bak`, atomic install and rollback are all implemented. | —
STATE-C278 | CONFIRMED | src-tauri/src/commands/config.rs | Replacement, reload/validation and `AppState` adoption run under one config write guard. | —
STATE-C279 | CONFIRMED | src/lib/stores/theme.ts:6,20-22,87-91 | `Theme` is the cited tri-state, `resolveTheme` exists and the `change` listener repaints only while `system`. | —
STATE-C280 | CONFIRMED | src/app.html:31-56 | The pre-paint bootstrap resolves the stored preference before first paint. | —
STATE-C281 | CONFIRMED | src/app.css:111-117; src/lib/components/settings/AppearanceCard.svelte | The compact density token-scale override and the Settings theme/density controls are present. | —
STATE-C282 | CONFIRMED | src-tauri/src/i18n.rs:138,196,254,996-1030,1360-1366 | One en/de/fr table plus the literal scanner over `tray/*` and `menu.rs` with an explicit allowlist. | —
STATE-C283 | CONFIRMED | src-tauri/src/i18n.rs (11 tests) | Scanner self-tests cover translated copy, field references, comments, URL punctuation and char boundaries. | —
STATE-C284 | CONFIRMED | vitest.config.js (thresholds block) | Thresholds are exactly 68.91 / 62.57 / 73.33 / 68.02 with the documented `exclude` set. | —
STATE-C285 | CONFIRMED | .github/workflows/ci.yml:264 | The `frontend` job runs `npm run test:coverage` and a drop fails the job. | —
STATE-C286 | DRIFT | .github/workflows/ci.yml:696-706 | The per-file floors name `polling/iteration.rs`, `polling/clocks.rs`, `config/mod.rs`, `token_io.rs` — not `poll_once.rs`/`config.rs`/`token_io.rs`. | P2
STATE-C287 | CONFIRMED | .github/workflows/ci.yml:619 | The `rust-coverage` job exists, so the tooling is no longer out of scope. | —
STATE-C288 | CONFIRMED | src-tauri/deny.toml:29-31; .github/workflows/ci.yml:598 | The licence allowlist, the `[sources]` deny and the SHA-pinned cargo-deny job are all present. | —
STATE-C289 | OVERSTATED | .github/workflows/dep-audit job; .github/dependabot.yml:36-38,58-60 | The `dep-audit` job does gate the npm production tree, but the `ignore:` lists in `dependabot.yml` carry only a `semver-major` update-type rule — no owner, no expiry and no #642 reference. | P2
STATE-C290 | CONFIRMED | .github/workflows/ci.yml:710-720,574-596 | Both the `docs-links` and `no-vendored-binaries` jobs are present as described. | —
STATE-C291 | CONFIRMED | docs/RELEASING.md:104,324 | Both §3 (CI gates) and §5 (cutting a release checklist) exist and cover these gates. | —
STATE-C292 | CONFIRMED | .github/workflows/ci.yml:84-96 | `cargo test --all-targets` runs on `macos-latest` only, with the loader hazard documented inline. | —
STATE-C293 | CONFIRMED | CLAUDE.md:1-8; AGENTS.md:3 | `AGENTS.md` is the canonical file and `CLAUDE.md` is a redirect stub. | —
STATE-C294 | CONFIRMED | AGENTS.md (sections 1-14) | Toolchain pins, gates, authoring rules, i18n/auth/storage, CSP, commits, CI map, must-NOTs, recipe and glossary are all in one file. | —
STATE-C295 | CONFIRMED | src/app.css:124,171; src/routes/+layout.svelte:599 | Both `color-scheme` declarations and the theme-bound `<meta name="color-scheme">` are present. | —
STATE-C296 | CONFIRMED | src-tauri/src/i18n.rs:196-252 vs src/lib/i18n/de.ts | The seven German tray strings use the same nouns as the webview equivalents (verified: open-logs-folder, status-syncing + no-track sibling, pause-sync, resume-sync, manual-status-clear, profile-empty). | —
STATE-C297 | UNSOURCED | src/lib/i18n/*.ts | "German was the only locale that named the same control two different ways; en/fr were already aligned" is a negative claim with no in-tree evidence (the i18n scanner only checks Rust-side literals). | P3
STATE-C298 | CONFIRMED | src/app.css (0 matches for all six tokens and both classes); src/lib/components/UpdatePrompt.svelte:696,721 | All six dead tokens and two dead classes are gone, and the banner uses the new tokens. | —
STATE-C299 | DRIFT | src-tauri/src/app.rs:1260-1268 | The `Builder::build` failure arm and its flush live in `app.rs::run`, not `lib.rs::run`. | P2
STATE-C300 | CONFIRMED | src-tauri/src/app.rs:1262-1266 | The comment documents tauri-plugin-log's buffering as the reason for the flush. | —
STATE-C301 | CONFIRMED | grep for `polling.rs` or `commands.rs` under src-tauri/src → 0 matches | `polling.rs`, `commands.rs` and `ts_rs_export` references are all gone from the Rust tree. | —
STATE-C302 | CONFIRMED | grep for `polling.rs` or `commands.rs` under src-tauri/src → 0 matches | The sweep command the row gives reproduces exactly. | —
STATE-C303 | DRIFT | src-tauri/src/deep_link.rs:216-300 | `handle_deep_link` lives in `deep_link.rs`, not `lib.rs`. | P2
STATE-C304 | CONFIRMED | src-tauri/src/app.rs:492 | Every current caller runs after `app.manage(state.clone())` in `setup_state`. | —
STATE-C305 | CONFIRMED | src-tauri/src/Cargo.toml (no `[dev-dependencies]` block); tests `force_tray_refresh_tolerates_missing_state_issue_937`, `handle_deep_link_tolerates_missing_state_issue_937`, `handle_deep_link_dispatches_valid_callback_when_state_is_managed_issue_937` | Three behavioural tests drive the arms through injected seams, and no `tauri` test-feature dev-dep exists. | —
STATE-C306 | CONFIRMED | src-tauri/src/deep_link.rs:330 | An unmanaged-state deep link is logged and dropped, never replayed. | —
STATE-C307 | DRIFT | src-tauri/src/deep_link.rs:188-206; src-tauri/src/app.rs:498-500 | `handle_deep_link` and the `PENDING_DEEP_LINK` slot live in `deep_link.rs`, not `lib.rs`; the replay is correctly described. | P2
STATE-C308 | CONFIRMED | src-tauri/src/deep_link.rs:178-186 | The early arm returns before the `deep_link_seen` claim so the replay is the gate's first delivery. | —
STATE-C309 | CONFIRMED | src-tauri/src/deep_link.rs:181,196 | Both new log lines carry presence only, never the URL/code/verifier/state. | —
STATE-C310 | CONFIRMED | tests `handle_deep_link_buffers_pre_manage_callback_and_replays_issue_1122`, `handle_deep_link_second_early_callback_overwrites_issue_1122` plus the unchanged #937 pair | Two new behavioural tests alongside the unchanged pair, exactly as stated. | —
STATE-C311 | CONFIRMED | src/routes/+layout.svelte:73,612; `grep -rn "<slot" src/` → 0 matches | The `children` snippet replaces the last legacy slot and svelte-check reports no `slot_element_deprecated` warning. | —
STATE-C312 | CONFIRMED | tests/shell-render.test.ts | The shell-render test mounts the shell with a page snippet and asserts swap semantics. | —
STATE-C313 | DRIFT | src-tauri/src/state.rs:288-323 | `Polling::is_syncing`/`set_syncing` live in `state.rs`, not `lib.rs`. | P3
STATE-C314 | CONFIRMED | src-tauri/src/state.rs:335-341 | `try_claim()` keeps its AcqRel/Acquire CAS, and both cited tests exist. | —
STATE-C315 | CONFIRMED | src/app.css:526,542-553 | `.empty-state` (+ `.small`/`p`/`.hint`) and `.spinner` each live once in `app.css`. | —
STATE-C316 | CONFIRMED | src/lib/components/Diagnostics.svelte | The loading branch renders the shared spinner with `aria-hidden="true"` above the collecting label. | —
STATE-C317 | CONFIRMED | tests/hygiene.test.ts; tests/diagnostics-loading.test.ts | Both pinning test files exist as cited. | —
STATE-C318 | CONFIRMED | src/app.css:75-80 | All six `:root` tokens are declared with exactly the cited values. | —
STATE-C319 | CONFIRMED | src/app.css:111-117 | Compact density shrinks only art/badge/spinner/swatch; `--ctl-h`/`--ctl-h-sm` are unchanged at 36px/32px. | —
STATE-C320 | CONFIRMED | src/lib/components/*.svelte; src/app.css | Icon buttons, album art, log grid, level badge, dismiss button and info icon all paint through tokens. | —
STATE-C321 | CONFIRMED | tests/hygiene.test.ts | The hygiene test asserts the tokens, the compact floor and the literal-free selectors. | —
STATE-C322 | CONFIRMED | src/routes/detached/[pane]/+page.svelte | Every branch is wrapped in a container with `id="main-content"` and `tabindex="-1"`. | —
STATE-C323 | CONFIRMED | tests/browser/detached-skip-link.spec.ts | The detached skip-link target and focus move are covered in Chromium and WebKit. | —
STATE-C324 | CONFIRMED | src/routes/+page.svelte; src/lib/components/*.svelte | The main-window target moved off `.app-container` onto each view's body. | —
STATE-C325 | CONFIRMED | src/lib/components/Dashboard.svelte; src/lib/components/Settings.svelte; src/lib/components/LogViewer.svelte:523; src/lib/components/Diagnostics.svelte; src/lib/components/Reconnect.svelte; src/lib/components/Onboarding.svelte; src/lib/components/About.svelte | Each named view carries `id="main-content"` with `tabindex="-1"` below its header. | —
STATE-C326 | CONFIRMED | src/routes/+page.svelte | Only one view mounts at a time, keeping the id unique per document. | —
STATE-C327 | CONFIRMED | tests/skip-link-target.test.ts; tests/browser/skip-link.spec.ts | Both the placement pin and the real-browser focus move are covered. | —
STATE-C328 | DRIFT | src-tauri/src/state.rs:224-250; src-tauri/src/polling/iteration.rs | `Config` and `run_inner` live in `state.rs` and `polling/iteration.rs`, not `lib.rs` and the deleted `poll_once.rs`. | P2
STATE-C329 | CONFIRMED | test `iteration_snapshot_is_shared_and_survives_a_concurrent_save` | The snapshot-sharing contract is pinned by that named test. | —
STATE-C330 | CONFIRMED | src/lib/i18n/{en,de,fr,es,it,pl,pt,nl}.ts | Both `logs.count_few` and `dashboard.snoozeStatusStart_few` are present in all eight dictionaries. | —
STATE-C331 | CONFIRMED | src/lib/i18n/pl.ts:92,738 | The real Polish nominative plurals ("2 wpisy" / "2 minuty") are present, with `_other`-mirroring `_few` elsewhere. | —
STATE-C332 | CONFIRMED | src/lib/i18n/pl.ts (`_one`/`_other`/`_few` only) | No `_many` key exists; the genitive plural is carried by `_other`. | —
STATE-C333 | CONFIRMED | tests/i18n.test.ts (plural and snooze-trio cases) | The few/many forms and the snooze trio are pinned, along with the `_one`/`_other`/`_few` trio. | —
STATE-C334 | CONFIRMED | src/lib/i18n/*.ts; src-tauri/src/i18n.rs:312-603 | All eight webview dictionaries, all eight Rust tables and longest-tag-first detection are present. | —
STATE-C335 | DRIFT | npm test → 456/456 across 39 files; `cargo test --lib i18n` → 11 | The suite is 456/456 (not 427/427) and the Rust i18n count is 11 (not 10/10); `config::` 168 and `commands::config` 19 are both correct. | P2
STATE-C336 | CONFIRMED | CHANGELOG.md:14; docs/STATE-OF-FEATURES.md:130 | The model-written / human-review-pending provenance is stated in both places. | —
STATE-C337 | CONFIRMED | src-tauri/src/polling/state.rs; src-tauri/src/polling/{mod,loop_,write,presence,status_text}.rs | `SessionState` owns the clocks (with the D11 guard), latches, cache, presence session, exit snapshot and mirrors; the siblings thread `&session`. | —
STATE-C338 | CONFIRMED | `grep -rn "global_state_lock" src-tauri/src` → 0 matches; test `test_two_sessions_do_not_share_clocks_latches_or_caches` | The static is deleted and the isolation test exists. | —
STATE-C339 | STALE | grep for `QUARANTINE_TEST_LOCK` or `LOCALE_TEST_LOCK` under src-tauri/src → 0 matches | The "remainder" note says tray/config `AppCaches` are still static with both test locks surviving, but both locks are gone and `AppCaches` is owned by `AppState`. | P2
STATE-C340 | DRIFT | cargo test --lib → 915 passed | "910/910" is stale — the tree runs 915/915. | P2
STATE-C341 | CONFIRMED | src-tauri/src/state.rs:755-777 | `AppCaches` owns the throttled caches, fetch instants, dedup snapshot, mirrors, delayed-refresh guard and quarantine/conflict flags. | —
STATE-C342 | CONFIRMED | `grep` → 0 matches for the statics; test `test_two_app_states_do_not_share_caches` | The statics and `QUARANTINE_TEST_LOCK` are deleted and the isolation test exists. | —
STATE-C343 | STALE | `grep -rn "LOCALE_TEST_LOCK" src-tauri/src` → 0 matches | The remainder note claims `LOCALE_TEST_LOCK` still survives, but it is deleted. | P2
STATE-C344 | DRIFT | cargo test --lib → 915 passed | "913/913" is stale — the tree runs 915/915. | P2
STATE-C345 | CONFIRMED | src-tauri/src/i18n.rs:682-693; src-tauri/src/menu.rs; src-tauri/src/tray/mod.rs; `grep "filter_status"` shows no locale-less wrapper | `LocaleState` owns the installed table, all builders render from `state.locale`, and the locale-less wrapper is deleted. | —
STATE-C346 | CONFIRMED | grep for `LOCALE_TEST_LOCK` under src-tauri/src/i18n.rs → 0 matches; test `two_app_states_do_not_share_the_locale_table` | The process-wide locale statics and the last test-wide lock are deleted and isolation is proven. | —
STATE-C347 | CONFIRMED | cargo test --lib → 915/915, `cargo clippy -D warnings` clean in CI config | "FULL green" holds without a stale number. | —
STATE-C348 | DRIFT | src-tauri/src/lib.rs:1-33 (33 lines) | `lib.rs` is a 33-line registry, not 34 lines; the module placements are otherwise accurate. | P3
STATE-C349 | CONFIRMED | src-tauri/src/app.rs:1149; src-tauri/src/deep_link.rs; src-tauri/src/app.rs; src-tauri/src/cli.rs; src-tauri/src/app.rs (`setup_log_permissions`); src-tauri/src/lib.rs:13 | All six scanner guard retargets are correct. | —
STATE-C350 | CONFIRMED | src-tauri/src/redact.rs:94-98 | The redaction sweep covers `app.rs`, `cli.rs`, `deep_link.rs` and `state.rs`. | —
STATE-C351 | DRIFT | cargo test --lib → 915 passed | "910/910" is stale — the tree runs 915/915. | P2
STATE-C352 | DRIFT | src-tauri/src/config/schema.rs | `NotificationsConfig` lives in `config/schema.rs`, not `config.rs`. | P2
STATE-C353 | CONFIRMED | src/lib/stores/notifications.ts:34-37,61-64,289-315; src-tauri/src/commands/onboarding.rs:645; src-tauri/src/updater_bg.rs:1742 | All four classes, their emitters and their guard conditions are present as described. | —
STATE-C354 | CONFIRMED | src-tauri/src/config/schema.rs; src/lib/components/settings/NotificationsCard.svelte; src/lib/stores/notifications.ts | All four classes default on with individual toggles, and preferences are mirrored across windows. | —
STATE-C355 | CONFIRMED | docs/STATE-OF-FEATURES.md:138 | The release-smoke section heading is present verbatim. | —
STATE-C356 | CONFIRMED | docs/STATE-OF-FEATURES.md:140-141 | The two release-smoke rows are named exactly as stated. | —
STATE-C357 | CONFIRMED | docs/STATE-OF-FEATURES.md:141-142 | Both rows are held at ⚠ Partial pending the two observations. | —
STATE-C358 | CONFIRMED | docs/STATE-OF-FEATURES.md:143-145 | The rule against flipping other ⚠ rows from a smoke run is stated explicitly. | —
STATE-C359 | CONFIRMED | docs/STATE-OF-FEATURES.md:147 | The smoke-1 heading is present verbatim. | —
STATE-C360 | CONFIRMED | docs/STATE-OF-FEATURES.md:149 | The previously-published-build precondition is stated. | —
STATE-C361 | CONFIRMED | src-tauri/tauri.conf.json:70; src-tauri/src/updater_bg.rs:1637 | Both the downgrade flag and the stale-stage guard back the "never stage itself" claim. | —
STATE-C362 | CONFIRMED | src-tauri/src/updater_bg.rs:1637,1832 | `install_pending_on_exit` skips `staged <= current` unless `forced`. | —
STATE-C363 | CONFIRMED | .github/workflows/release.yml:280-284; git tag v4.5.2 | The three artifact names and the v4.5.2 tag all exist. | —
STATE-C364 | CONFIRMED | AGENTS.md:250; src-tauri/tauri.conf.json (identifier) | The macOS log path matches the documented `~/Library/Logs/com.presencejam.app/PresenceJam.log`. | —
STATE-C365 | CONFIRMED | AGENTS.md:251 | The Linux log path matches the documented `~/.local/share/com.presencejam.app/logs/PresenceJam.log`. | —
STATE-C366 | CONFIRMED | AGENTS.md:249 | The Windows `%LOCALAPPDATA%` path matches the documented location. | —
STATE-C367 | CONFIRMED | src-tauri/src/updater_bg.rs:1775 | The expected `[UPDATER.BG] stage_deferred_update: SUCCESS` line is emitted verbatim. | —
STATE-C368 | CONFIRMED | docs/STATE-OF-FEATURES.md:165 | The quit-step preamble and the "in order" expectation are stated. | —
STATE-C369 | CONFIRMED | src-tauri/src/updater_bg.rs:1832 | `install_pending_on_exit: installing v{} on quit` matches the expected line. | —
STATE-C370 | CONFIRMED | src-tauri/src/updater_bg.rs:1847-1849 | `install_pending_on_exit: v{} installed; takes effect on next launch (Windows installer relaunches automatically)` matches verbatim. | —
STATE-C371 | CONFIRMED | src-tauri/src/updater_bg.rs:1848; src/lib/components/About.svelte:24 | Windows relaunches automatically and About shows the version. | —
STATE-C372 | CONFIRMED | docs/STATE-OF-FEATURES.md:172-173 | The per-platform relaunch recording instruction is present. | —
STATE-C373 | CONFIRMED | src-tauri/src/updater_bg.rs:1848 | "takes effect on next launch" is the shipped behaviour on macOS/Linux. | —
STATE-C374 | CONFIRMED | docs/STATE-OF-FEATURES.md:177-178 | The flip instruction with release tag and three platform results is present. | —
STATE-C375 | CONFIRMED | docs/STATE-OF-FEATURES.md:180 | The smoke-2 heading is present verbatim. | —
STATE-C376 | CONFIRMED | src-tauri/src/token_io.rs:19,79-101 | `tokens.json` is AES-256-GCM ciphertext keyed by the keychain secret, so an expired `expires_at` cannot be hand-written. | —
STATE-C377 | CONFIRMED | docs/STATE-OF-FEATURES.md:184 | The wall-clock wait is stated as the driver. | —
STATE-C378 | CONFIRMED | docs/STATE-OF-FEATURES.md:186 | Step 1 (sign in to both providers, post, quit) is present. | —
STATE-C379 | CONFIRMED | docs/STATE-OF-FEATURES.md:187 | The > 1 hour closed-app interval is specified (the Spotify access-token lifetime itself is a vendor contract). | —
STATE-C380 | CONFIRMED | src/lib/utils/boot.ts:18-24 | `bootView` routes a complete install to the Dashboard, not the wizard. | —
STATE-C381 | CONFIRMED | src-tauri/src/commands/onboarding.rs:185 | `{CMD} is_onboarding_complete: {label} session refreshed (access token was expired at launch)` matches verbatim. | —
STATE-C382 | CONFIRMED | src-tauri/src/commands/onboarding.rs:185,455-459 | The same format string is emitted for both the spotify and teams labels. | —
STATE-C383 | CONFIRMED | src-tauri/src/commands/onboarding.rs:467-473 | `result={} (spotify_configured={}, spotify_valid={}, teams_configured={}, teams_valid={})` matches verbatim. | —
STATE-C384 | CONFIRMED | docs/STATE-OF-FEATURES.md:196 | The platform and wall-clock gap recording instruction is present. | —
STATE-C385 | CONFIRMED | docs/STATE-OF-FEATURES.md:198 | The flip instruction is present. | —
STATE-C386 | CONFIRMED | docs/STATE-OF-FEATURES.md:198-201 | The honest-alternative rewording is quoted in full. | —
STATE-C387 | CONFIRMED | src-tauri/src/macos_deeplink.rs; src-tauri/src/pkce.rs | The macOS re-claim is already wired and the residual risk is covered by the PKCE launch binding. | —
STATE-C388 | CONFIRMED | docs/STATE-OF-FEATURES.md:207 | The smoke-record heading and date are present verbatim. | —
STATE-C389 | CONFIRMED | git tag v4.7.0 | The `v4.7.0` tag exists, and the surrounding text states the headless-host conditions. | —
STATE-C390 | CONFIRMED | docs/STATE-OF-FEATURES.md:210-211 | Both rows are held at ⚠ per the stated rule. | —
STATE-C391 | EXTERNAL-UNVERIFIED | (vendor: github.com release endpoint — not in the allowlist) | The `latest.json` manifest contents, platform keys and minisign bodies are only observable from the live release endpoint, which is outside the vendor allowlist and was not fetched. | P3
STATE-C392 | EXTERNAL-UNVERIFIED | (vendor: github.com release endpoint) | The 200 checks and the `SHA256SUMS.txt` eight-entry count require a live fetch that is outside the allowlist. | P3
STATE-C393 | CONFIRMED | src-tauri/src/app.rs:1146; git show v4.7.0:src-tauri/src/lib.rs:1568 | The log line `[APP] setup: PresenceJam {} started successfully` is emitted in both the current tree and at the v4.7.0 tag. | —
STATE-C394 | CONFIRMED | .github/workflows/release.yml:78-88 | The permanent re-offer loop rationale is documented at the tag gate. | —
STATE-C395 | CONFIRMED | git show v4.7.0:src-tauri/src/commands/sync.rs:18-38 | The v4.7.0 `SyncStatus` has exactly seven fields including `last_posted_status`, `presence_gated` and `presence_paused`. | —
STATE-C396 | CONFIRMED | src-tauri/src/cli.rs:514; git show v4.7.0:src-tauri/src/lib.rs:930 | The `no Spotify client_id configured — sign in from the app before using this flag` stderr reason exists in both the current tree and at v4.7.0. | —
STATE-C397 | CONFIRMED | docs/STATE-OF-FEATURES.md:231-235 | The click-to-stage gap and the 4.6.0 rationale are stated explicitly. | —
STATE-C398 | CONFIRMED | docs/STATE-OF-FEATURES.md:236-237 | The smoke-2 gap and its credential requirement are stated. | —
STATE-C399 | CONFIRMED | docs/STATE-OF-FEATURES.md:239-240 | The maintainer-flip instruction is present. | —
STATE-C400 | CONFIRMED | docs/STATE-OF-FEATURES.md:242 | The documented-gaps section heading is present verbatim. | —
STATE-C401 | CONFIRMED | docs/STATE-OF-FEATURES.md:246 | The Spotify Free row is marked ❌. | —
STATE-C402 | CONFIRMED | src-tauri/src/sources/spotify.rs:131-141; src-tauri/src/spotify.rs:416 | The Premium restriction is a platform constraint the app surfaces as an auth/reconnect path, matching the row's note. | —
STATE-C403 | CONFIRMED | docs/STATE-OF-FEATURES.md:247 | The macOS hijack defence row is marked ✅. | —
STATE-C404 | CONFIRMED | src-tauri/src/macos_deeplink.rs; src-tauri/src/pkce.rs | Failure is log-only and the PKCE launch-binding defence is in place, with the residual window stated. | —
STATE-C405 | CONFIRMED | .github/workflows/release.yml:617,689,948 | `digest-mismatch: error` is set in all three `actions/download-artifact@v8.0.1` invocations. | —
STATE-C406 | CONFIRMED | docs/STATE-OF-FEATURES.md:249; src-tauri/Cargo.lock | The code-signing row is marked ⚠ Not signed and the app ships unsigned. | —
STATE-C407 | CONFIRMED | src-tauri/tauri.conf.json (`updater.pubkey`); .github/workflows/release.yml:581-592 | The minisign pubkey is baked into the manifest and the sign job signs updater payloads independently. | —
STATE-C408 | CONFIRMED | docs/STATE-OF-FEATURES.md:251 | The "Have not been verified end-to-end" section heading is present verbatim. | —
STATE-C409 | CONFIRMED | docs/STATE-OF-FEATURES.md:255 | The tenant row is present and marked ⚠ Verify. | —
STATE-C410 | EXTERNAL-UNVERIFIED | (vendor: learn.microsoft.com/graph/*) | The `AdminConsentRequired: No` claim for `Presence.ReadWrite` and `offline_access` is a Graph permissions-reference claim; the allowlisted page was not fetched. | P3
STATE-C411 | EXTERNAL-UNVERIFIED | (vendor: learn.microsoft.com/graph/*) | "The China (21Vianet) national cloud does not support `setStatusMessage` at all" is a vendor contract claim that was not fetched. | P3
STATE-C412 | EXTERNAL-UNVERIFIED | (vendor: developer.microsoft.com graph-explorer) | The graph-explorer check instruction is sound but points outside the repo and was not fetched. | P3
STATE-C413 | CONFIRMED | src-tauri/Cargo.toml:55 | Tauri is pinned to `~2.11`, a deliberately narrow compatibility range as the row states. | —
STATE-C414 | CONFIRMED | .github/workflows/ci.yml | Every CI leg runs `ubuntu-latest`; no Fedora/Arch job exists. | —
STATE-C415 | CONFIRMED | src-tauri/Cargo.toml:88 (keyring `linux-native`); docs/PLATFORMS.md | The non-glibc and secret-service caveats are consistent with the tree's keyring feature set. | —
STATE-C416 | CONFIRMED | SETUP.md:221-264 | The `#linux-keyring` anchor and the Secret Service requirement are present. | —
STATE-C417 | CONFIRMED | .github/workflows/release.yml:617,689,948 | `digest-mismatch: error` is enforced in all three download-artifact invocations on v8.0.1. | —
STATE-C418 | CONFIRMED | docs/STATE-OF-FEATURES.md:260 | The out-of-scope section heading is present verbatim. | —
STATE-C419 | CONFIRMED | src-tauri/src/ (no remote auth path); src-tauri/src/serve.rs:33 | PresenceJam is local-first with only a token-guarded localhost API; no remote auth proxy exists. | —
STATE-C420 | CONFIRMED | src-tauri/src/teams.rs:763-790 | `setStatusMessage` posts one text message with no emoji surface. | —
STATE-C421 | CONFIRMED | src-tauri/src/config/schema.rs; src-tauri/src/config/clamp.rs:27-31; src/lib/components/Settings.svelte | `paused_status_format` and `stopped_status_format` are user-editable and bounded by `MAX_RULE_STATUS_CHARS`. | —
STATE-C422 | CONFIRMED | src-tauri/src/spotify.rs:416 (documented endpoints only) | No unofficial Spotify endpoints are called anywhere in the tree. | —
STATE-C423 | CONFIRMED | src-tauri/src/ (no Slack/Discord code) | No other IM status integrations exist in the tree. | —
STATE-C424 | CONFIRMED | docs/STATE-OF-FEATURES.md:267 | The "Reporting a stale row" heading is present verbatim. | —
STATE-C425 | CONFIRMED | docs/STATE-OF-FEATURES.md:269-270 | The PR-with-verified-source instruction is present. | —
STATE-C426 | CONFIRMED | docs/STATE-OF-FEATURES.md:271-272 | The first-contribution note is present. | —
STATE-C427 | CONFIRMED | docs/STATE-OF-FEATURES.md:5-6 | The "Verify with maintainer rather than guessed" fragment is present verbatim. | —
STATE-C428 | CONFIRMED | docs/STATE-OF-FEATURES.md:10 | The "do not edit the underlying behavior silently" fragment is present verbatim. | —
STATE-C429 | CONFIRMED | src-tauri/Cargo.lock:1831-1833 | `h2 0.4.18` is the locked version. | —

## DEFECTS (non-CONFIRMED, by blast radius)

Ordered by severity then by how many rows the defect misleads. P1 first.

```
claim_id: STATE-C047
file: docs/STATE-OF-FEATURES.md:26
class: C2
verdict: OVERSTATED
evidence: src-tauri/src/polling/exit.rs — `test_clear_presence_on_exit_reads_the_snapshot_not_the_clocks`; src-tauri/src/polling/state.rs — `test_exit_cleanup_survives_the_loop_exit_tail`
finding: The row states "`clear_presence_on_exit` has no test at all", but the exit-time clear is covered by two unit tests that both exist and pass in `cargo test --lib`. The row's honest partiality is therefore wider than reality and will be read as "nothing here is tested", discouraging a reviewer who could otherwise close it.
proposed_fix: Replace "the arm/clear round trip against a live tenant and the exit-time clear are not observed yet — `clear_presence_on_exit` has no test at all" with "the arm/clear round trip against a live tenant is not observed yet; the exit-time clear is unit-tested (`test_clear_presence_on_exit_reads_the_snapshot_not_the_clocks`)"
severity: P1
```

```
claim_id: STATE-C260
file: docs/STATE-OF-FEATURES.md:102
class: C7
verdict: OVERSTATED
evidence: docs/STATE-OF-FEATURES.md:102 — cell ends "`dispatch_event` routes playback thr…"
finding: The Global-shortcuts row's evidence cell is truncated mid-sentence, so the last claim it makes is unverifiable and reads as an unfinished draft in a file whose header promises "no-hedge answers". The visible remainder ("routes playback thr[ead]") is consistent with `commands/shortcuts.rs::dispatch_event`, but the sentence as shipped cannot be checked.
proposed_fix: Complete the sentence: "`dispatch_event` routes playback toggles through the tray refresh path and the sync toggle through the poller's start/stop, so a refused shortcut never double-fires."
severity: P2
```

```
claim_id: STATE-C289
file: docs/STATE-OF-FEATURES.md:111
class: C6
verdict: OVERSTATED
evidence: .github/dependabot.yml:36-38 and :58-60 — `ignore:` lists contain only `update-types: ["version-update:semver-major"]`; `grep -n "642\|expiry" .github/dependabot.yml` → 0 matches
finding: The row claims "the `ignore:` list carries owner + expiry tied to #642". Neither Dependabot `ignore:` block names an owner or an expiry, and issue #642 is not referenced anywhere in the file — so the documented audit trail does not exist.
proposed_fix: Replace "the `ignore:` list carries owner + expiry tied to #642" with "the `ignore:` lists block `semver-major` bumps by update-type so majors stay a manual decision"
severity: P2
```

```
claim_id: STATE-C095, STATE-C340, STATE-C344, STATE-C351
file: docs/STATE-OF-FEATURES.md:40,131,132,134
class: C6
verdict: DRIFT
evidence: `cargo test --manifest-path src-tauri/Cargo.toml --lib` → "test result: ok. 915 passed; 0 failed"
finding: Four rows pin the Rust lib-test count at 910/910 (twice), 913/913 and 910/910. The tree runs 915/915. `config 121/121` and `commands::config 19/19` are correct; only the totals moved. A reviewer re-running the gate sees a different number from the one the matrix promises and cannot tell whether tests were added or dropped.
proposed_fix: Update all four to "`cargo test --lib` 915/915" (and keep 121/121 config, 168 config::, 19 commands::config, 11 i18n as measured)
severity: P2
```

```
claim_id: STATE-C335
file: docs/STATE-OF-FEATURES.md:130
class: C6
verdict: DRIFT
evidence: `npx vitest run` → "Test Files 39 passed (39) / Tests 456 passed (456)"; `cargo test --lib i18n` → 11 tests; `cargo test --lib config::` → 168; `cargo test --lib commands::config` → 19
finding: The row's suite inventory is stale on two of five counts: the frontend suite is 456/456 (not 427/427) and the Rust i18n suite is 11 (not 10/10). `tests/i18n.test.ts` 26, `config::` 168 and `commands::config` 19 are all correct.
proposed_fix: Replace "Frontend suite 427/427 green (incl. `tests/i18n.test.ts` 26 tests); Rust `cargo test --lib i18n` 10/10, `config::` 168/168, `commands::config` 19/19" with "Frontend suite 456/456 green (incl. `tests/i18n.test.ts` 26 tests); Rust `cargo test --lib i18n` 11/11, `config::` 168/168, `commands::config` 19/19"
severity: P2
```

```
claim_id: STATE-C339, STATE-C343
file: docs/STATE-OF-FEATURES.md:131,132
class: C7
verdict: STALE
evidence: `grep -rn "QUARANTINE_TEST_LOCK\|LOCALE_TEST_LOCK" src-tauri/src` → 0 matches; src-tauri/src/state.rs:755-777 (`AppCaches`); src-tauri/src/i18n.rs:682-693 (`LocaleState`)
finding: The survey rows are already superseded. Row 131 (the "polling half" partial) still says tray/config `AppCaches` are static with `QUARANTINE_TEST_LOCK` + `LOCALE_TEST_LOCK` surviving, and row 132's remainder says `LOCALE_TEST_LOCK` survives — but slice 2 and slice 3 have both landed: `AppCaches` and `LocaleState` are `AppState`-owned and both test locks are deleted. The audit trail reads as if the issue were still open on two fronts.
proposed_fix: In row 131 delete "Remainder: tray/config `AppCaches` (slice 2) still static, so `QUARANTINE_TEST_LOCK` + `LOCALE_TEST_LOCK` survive." In row 132 delete "Remainder: `LOCALE_TEST_LOCK` survives (`i18n::CURRENT` is an issue-blessed keep)."
severity: P2
```

```
claim_id: STATE-C157
file: docs/STATE-OF-FEATURES.md:66
class: C1
verdict: DRIFT
evidence: src-tauri/src/spotify.rs:645-647 (`build_spotify_client` now delegates to `crate::http::shared_client()`); src-tauri/src/http.rs:112-127 (`build_client` sets `.user_agent(user_agent()).timeout(timeout)`); src-tauri/src/http.rs:38 (`DEFAULT_TIMEOUT_SECS = 10`)
finding: The row cites `spotify.rs::build_spotify_client` as where the 10 s timeout and the `PresenceJam/<version>` User-Agent are set, but that function is now a one-line delegation to `http::shared_client()`. The behaviour is intact and correct — the 10 s timeout and the UA live in `http.rs::build_client`, with `DEFAULT_TIMEOUT_SECS = 10` at `http.rs:38`. Anyone auditing HTTP hardening from this row lands on an empty function.
proposed_fix: Replace "`src-tauri/src/spotify.rs::build_spotify_client` — 10 s timeout + `PresenceJam/<version>` User-Agent on all Spotify calls" with "`src-tauri/src/http.rs::build_client` — 10 s timeout (`DEFAULT_TIMEOUT_SECS`) + `PresenceJam/<version>` User-Agent (`http::user_agent`), shared by `src-tauri/src/spotify.rs::build_spotify_client` and the Teams client"
severity: P2
```

```
claim_id: STATE-C248
file: docs/STATE-OF-FEATURES.md:100
class: C1
verdict: DRIFT
evidence: src-tauri/src/profanity.rs:749 `pub fn filter_status_for_locale(text, placeholder, is_playing, extra_words, locale)`; src-tauri/src/profanity.rs:776-783 (the bare `filter_status` is a `#[cfg(test)]` helper); src-tauri/src/profanity.rs:747 ("the deleted `filter_status` wrapper (which read the global) had no production caller left")
finding: The row quotes a production signature `profanity::filter_status(text, placeholder, is_playing, extra_words)` that no longer exists — the production entry point gained a fifth `locale` parameter in issue #758 slice 3, and the 4-arg form is test-only. The described evasion behaviour and the `not`/`noting` carve-out are correct, but the cited API is not callable from production.
proposed_fix: Replace "`profanity::filter_status(text, placeholder, is_playing, extra_words)`" with "`profanity::filter_status_for_locale(text, placeholder, is_playing, extra_words, locale)`"
severity: P2
```

```
claim_id: STATE-C078
file: docs/STATE-OF-FEATURES.md:35
class: C6
verdict: DRIFT
evidence: src-tauri/src/app.rs:1149 (`invoke_handler(tauri::generate_handler![...])`); src-tauri/src/commands/mod.rs:560 (`let anchor = "generate_handler![";`)
finding: The row says the guard-matrix test "brace-counts the handler list out of `lib.rs`". Since the `lib.rs` split the handler list lives in `app.rs`, and the test itself anchors on `"generate_handler!["` from whatever module the scanner is pointed at. A reader following this citation opens `lib.rs` and finds a 33-line module registry.
proposed_fix: Replace "brace-counts the handler list out of `lib.rs`" with "brace-counts the handler list out of `app.rs`"
severity: P2
```

```
claim_id: STATE-C029, STATE-C031, STATE-C058, STATE-C155, STATE-C209, STATE-C243, STATE-C268, STATE-C299, STATE-C303, STATE-C307, STATE-C313, STATE-C328
file: docs/STATE-OF-FEATURES.md:21,22,29,65,90,99,105,107,118,120,121,123,128
class: C1
verdict: DRIFT
evidence: src-tauri/src/lib.rs:1-33 (module registry only); code lives in src-tauri/src/app.rs, cli.rs, deep_link.rs, state.rs — e.g. `start_minimized` at app.rs:562, `register_all()` at app.rs:778, updater registration at app.rs:1085, secret migration at app.rs:596, `RunEvent::Exit` at app.rs:1286, `log_rotation_strategy` at app.rs:194, `Builder::build` failure arm at app.rs:1260, `handle_deep_link` at deep_link.rs:216, `Polling`/`Config` at state.rs:288/224
finding: Thirteen rows still cite `src-tauri/src/lib.rs` (or "`lib.rs` setup", "`lib.rs::run`", "`lib.rs::Config`", "`lib.rs::Polling`") for code that moved to `app.rs`, `cli.rs`, `deep_link.rs` or `state.rs` during the registry split. In every case the described behaviour is accurate and still shipped — only the file path is wrong, so each read costs a reader a detour through the module registry to find the real site.
proposed_fix: Retarget each citation to the live module: `src-tauri/src/app.rs` (`start_minimized`, `register_all()`, updater registration, `setup_secret_migration`, `RunEvent::Exit`, `log_rotation_strategy`, `Builder::build` arm), `src-tauri/src/deep_link.rs` (`handle_deep_link`, `PENDING_DEEP_LINK`), `src-tauri/src/state.rs` (`Config`, `Polling`), `src-tauri/src/polling/iteration.rs` (`run_inner`)
severity: P2
```

```
claim_id: STATE-C035, STATE-C061, STATE-C155, STATE-C175, STATE-C229, STATE-C265, STATE-C272, STATE-C275, STATE-C352
file: docs/STATE-OF-FEATURES.md:23,30,65,76,96,101,104,106,136
class: C1
verdict: DRIFT
evidence: src-tauri/src/config/mod.rs:6-50 (module registry + re-exports); definitions in config/{schema,clamp,snooze,patch,migrate,io,transfer}.rs — e.g. `atomic_write_json` at config/io.rs, `decide_legacy_secret_outcome` at config/migrate.rs, `PRESENCE_COMBINATIONS` at config/clamp.rs, `UpdateChannel`/`NotificationsConfig`/`StatusRulesConfig`/`TrackRuleEntry`/`QuietHoursEntry` at config/schema.rs
finding: Ten rows cite `config.rs` as a file. `src-tauri/src/config.rs` no longer exists — it is the `config/` directory with one concern per file, re-exported through `config/mod.rs`. The symbols are all still reachable as `crate::config::X`, so nothing is broken, but every `config.rs::X` citation names a path that no longer resolves on disk.
proposed_fix: Replace `src-tauri/src/config.rs` / `config.rs::` with the owning slice: `src-tauri/src/config/io.rs` (`atomic_write_json`), `src-tauri/src/config/migrate.rs` (`decide_legacy_secret_outcome`), `src-tauri/src/config/clamp.rs` (`PRESENCE_COMBINATIONS`, `normalize_presence_pair`, `clamp_logging`, `clamp_track_rule_window`, `clamp_snooze`), `src-tauri/src/config/schema.rs` (all `*Config` structs, `UpdateChannel`, `snooze_until`, `snooze_preset_deadline`), `src-tauri/src/config/snooze.rs` (`snooze_expired_deadline`)
severity: P2
```

```
claim_id: STATE-C020, STATE-C022, STATE-C071, STATE-C096, STATE-C097, STATE-C132, STATE-C145, STATE-C165, STATE-C166, STATE-C167, STATE-C168, STATE-C178, STATE-C196, STATE-C215, STATE-C230, STATE-C233, STATE-C242, STATE-C252, STATE-C254, STATE-C273
file: docs/STATE-OF-FEATURES.md:18,19,33,41,56,62,69,70,71,72,77,85,92,96,97,99,101,106
class: C5
verdict: DRIFT
evidence: src-tauri/src/polling/mod.rs:35-48 (module registry); `poll_once.rs` deleted. Definitions: `pause_backoff`/`NETWORK_FAILURE_THRESHOLD` at polling/timing.rs, `debounce_active`/`should_skip_identical_write`/`teams_write_with_optional_refresh`/`teams_token_for_write` at polling/write.rs, `gate_recheck_due` at polling/gate.rs, `rule_gate_at`/`RuleDecision` at polling/rules.rs, `presence_expiration_duration`/`manual_status_blocks_write` at polling/presence.rs, `clear_snooze_if_expired`/`snooze_gate` at polling/gate.rs, `run_inner` at polling/iteration.rs, status fallbacks at polling/status_text.rs
finding: Twenty rows cite `src-tauri/src/polling/poll_once.rs` or `poll_once.rs`. That file is deleted and its 121 tests redistributed across `polling/{clocks,iteration,refresh,gate,rules,presence,status_text,write,timing,exit,state}.rs`. All the cited symbols exist and the described behaviour is correct — the path is what is wrong, and it is the single most-repeated stale citation in the file.
proposed_fix: Replace every `polling/poll_once.rs::X` / `poll_once.rs::X` with the owning module per the list above; for row 18 also replace `src-tauri/src/polling/poll_once.rs` with `src-tauri/src/polling/timing.rs`
severity: P2
```

```
claim_id: STATE-C096
file: docs/STATE-OF-FEATURES.md:41
class: C5
verdict: DRIFT
evidence: src-tauri/src/polling/mod.rs:35-48 — 14 `mod` declarations (clocks, daemon, exit, gate, iteration, loop_ [via #[path]], presence, refresh, rules, state, status_text, timing, write)
finding: The row says the split produced "a 10-module registry in `mod.rs`" and names exactly ten modules, but the registry declares fourteen — `daemon.rs` (the `--daemon` supervisor, issue #896) and `state.rs` (thread-lifecycle glue) were added after the row was written. The rest of the row (`PollState`, 121 moved tests) is accurate.
proposed_fix: Replace "plus a 10-module registry in `mod.rs`" with "plus a 14-module registry in `mod.rs` (adding `state` for lifecycle glue and `daemon` for `--daemon`)" and extend the brace list to `{clocks,iteration,refresh,gate,rules,presence,status_text,write,timing,exit,state,daemon}.rs`
severity: P2
```

```
claim_id: STATE-C173
file: docs/STATE-OF-FEATURES.md:75
class: C1
verdict: DRIFT
evidence: src/lib/components/LogViewer.svelte:667 — `grid-template-columns: var(--log-col-ts) max-content 1fr;`; src/app.css:80 — `--log-col-ts: 88px;`
finding: The row quotes the column spec as the literal `88px max-content 1fr`. The component actually paints `var(--log-col-ts) max-content 1fr`, with `--log-col-ts` resolving to 88px in `app.css`. The rendered result matches, but the citation describes the token's value rather than the code, which is the reverse of the ratio-based discipline the neighbouring contrast rows follow.
proposed_fix: Replace "The level-badge column is `88px max-content 1fr`" with "The level-badge column is `var(--log-col-ts) max-content 1fr` (`--log-col-ts` = 88px)"
severity: P3
```


```
claim_id: STATE-C083
file: docs/STATE-OF-FEATURES.md:37
class: C2
verdict: UNSOURCED
evidence: `git cat-file -t b82f515` → not present; `grep -rn "b82f515" .` → 0 matches in the tree
finding: The row pins the macOS dock-badge wiring to commit `b82f515`, which is not reachable from HEAD and is cited nowhere else in the repository, so the citation cannot be resolved by a reader. The `#[cfg]`-gated dock badge itself is present in the tray code.
proposed_fix: Replace "(macOS-only (`#[cfg]`) presence-gated dock badge wired into the polling loop (`b82f515`))" with a path:line citation to the tray badge arm, e.g. "(macOS-only (`#[cfg]`) presence-gated dock badge wired into the polling loop — `src-tauri/src/tray/mod.rs`)"
severity: P3
```

```
claim_id: STATE-C128
file: docs/STATE-OF-FEATURES.md:54
class: C6
verdict: UNSOURCED
evidence: .github/workflows/release.yml:561,665 — `actions/attest-build-provenance@4d101475d8b20a2381f78447822ac1eab6504dd8  # v4.2.2`; no run URL anywhere in the doc
finding: The SHA-pinned attestation is verifiable and correct, but the second half of the claim — "exercised live on the v4.0.0 and v4.1.0 tag runs (attest step green both times)" — carries no run URL or log excerpt, and this file's own reporting rule (line 269) asks for exactly that. The row's headline status is "✅ Verified", which is stronger than its evidence.
proposed_fix: Add the two run URLs after "exercised live on the v4.0.0 and v4.1.0 tag runs", or soften to "SHA-pinned in `release.yml`; the two tagged runs it was exercised on are not linked here"
severity: P3
```

```
claim_id: STATE-C297
file: docs/STATE-OF-FEATURES.md:116
class: C2
verdict: UNSOURCED
evidence: src-tauri/src/i18n.rs:1186-1210 (`non_english_tables_translate_every_field` checks Rust-table-only translation, not cross-surface terminology); src/lib/i18n/*.ts
finding: "German was the only locale that named the same control or state two different ways in adjacent UI; en/fr were already aligned" is a negative claim about the frontend dictionaries that no scan in the tree tests — the i18n scanner only covers Rust-side literals. The seven German tray strings do match their webview nouns (verified), so the primary claim holds; only the exclusive "only locale" part is unsupported.
proposed_fix: Either add a `tests/i18n.test.ts` case that fails when any two locales name the same control differently, or replace with "German was the locale that named the same control or state two different ways (#901); en/fr were already aligned"
severity: P3
```

```
claim_id: STATE-C391, STATE-C392
file: docs/STATE-OF-FEATURES.md:215,218
class: C4
verdict: EXTERNAL-UNVERIFIED
evidence: (vendor: github.com/Carme99/PresenceJam-Desktop/releases — not in the allowlist)
finding: The 4.7.0 smoke record's manifest and asset observations (`version: 4.7.0`, the three platform keys, non-empty minisign bodies, all asset URLs returning 200, `SHA256SUMS.txt` with eight entries) are only checkable against the live GitHub Releases endpoint. That host is outside the vendor allowlist, so these two bullets remain external-unverified in this audit even though the record is internally consistent with `release.yml`.
proposed_fix: Add the fetch date and the command used (e.g. `curl -s …/latest.json | jq .version`) to each bullet so the observation is reproducible, and mark the section "verified externally against the live endpoint on <date>"
severity: P3
```

```
claim_id: STATE-C044, STATE-C045, STATE-C410, STATE-C411, STATE-C412
file: docs/STATE-OF-FEATURES.md:26,187,255
class: C4
verdict: EXTERNAL-UNVERIFIED
evidence: (vendor: learn.microsoft.com/graph/* and developer.spotify.com/documentation/web-api/* — both in the allowlist but not fetched in this audit)
finding: Five claims are external contracts: the 5-minute Available-session timeout, the `PT5M`–`PT4H` the `expirationDuration` bounds, and the three Graph tenant claims (`AdminConsentRequired: No` for `Presence.ReadWrite`/`offline_access`, the 21Vianet `setStatusMessage` gap, and the graph-explorer check). All are plausible and correctly hedged, but each needs a vendor-page quote with a fetch date to be grounded under this audit's evidence hierarchy.
proposed_fix: Add a quoted sentence plus a 2026-10-08 fetch date for each: the Graph presence/setPresence page for the timeout and duration bounds, and the Graph permissions reference plus the national-cloud page for the tenant claims
severity: P3
```



```
claim_id: STATE-C123
file: docs/STATE-OF-FEATURES.md:51
class: C6
verdict: DRIFT
evidence: src-tauri/src/redact.rs:82-140 — `redaction_literal_is_built_only_in_redact_rs` exists; `grep -rn "call_site_output_matches_helper_format" src-tauri/` → 0 matches; the equivalent test is `diagnostics_redaction_embeds_helper_output_for_keyed_value` (redact.rs:51)
finding: The row names two guarding tests. The first exists; the second (`call_site_output_matches_helper_format`) does not — the seam it describes is pinned under a different name, so a reader checking the cited test finds nothing and may conclude the call-site/helper format equivalence is unpinned.
proposed_fix: Replace "`redact::tests::redaction_literal_is_built_only_in_redact_rs` + `call_site_output_matches_helper_format`" with "`redact::tests::redaction_literal_is_built_only_in_redact_rs` + `redact::tests::diagnostics_redaction_embeds_helper_output_for_keyed_value`"
severity: P2
```

```
claim_id: STATE-C258
file: docs/STATE-OF-FEATURES.md:102
class: C1
verdict: DRIFT
evidence: src-tauri/src/commands/shortcuts.rs (`apply_plan`); src-tauri/src/lib.rs:1-33 (module registry — no registration); src-tauri/Cargo.toml:100 (`tauri-plugin-global-shortcut = "~2.3"`)
finding: The row says `apply_plan` registers "through `tauri-plugin-global-shortcut` (`lib.rs`)", but `lib.rs` is now a 33-line module registry that registers nothing; the registration lives in `commands/shortcuts.rs`.
proposed_fix: Replace "and `apply_plan` registers through `tauri-plugin-global-shortcut` (`lib.rs`)" with "and `apply_plan` registers through `tauri-plugin-global-shortcut` (`commands/shortcuts.rs`)"
severity: P2
```

```
claim_id: STATE-C286
file: docs/STATE-OF-FEATURES.md:110
class: C6
verdict: DRIFT
evidence: .github/workflows/ci.yml:696-706 — floors are `floor("polling/iteration.rs"; 0)`, `floor("polling/clocks.rs"; 0)`, `floor("config/mod.rs"; 0)`, `floor("token_io.rs"; 0)`
finding: The row describes the Rust coverage floors as covering `poll_once.rs`/`config.rs`/`token_io.rs`. `poll_once.rs` and `config.rs` are both deleted paths — the actual floors name `polling/iteration.rs`, `polling/clocks.rs` and `config/mod.rs`, and the file set grew from three to four.
proposed_fix: Replace "per-file floors for `poll_once.rs`/`config.rs`/`token_io.rs` starting at 0" with "per-file floors for `polling/iteration.rs`/`polling/clocks.rs`/`config/mod.rs`/`token_io.rs` starting at 0"
severity: P2
```

```
claim_id: STATE-C348
file: docs/STATE-OF-FEATURES.md:134
class: C5
verdict: DRIFT
evidence: `wc -l src-tauri/src/lib.rs` → 33; src-tauri/src/lib.rs:1-33
finding: The row states "`lib.rs` is a 34-line module registry"; the file is 33 lines. The rest of the row (module placements and the four new-module redaction sweep) is accurate.
proposed_fix: Replace "`lib.rs` is a 34-line module registry + state re-export shim" with "`lib.rs` is a 33-line module registry + state re-export shim"
severity: P3
```

```
claim_id: STATE-C186
file: docs/STATE-OF-FEATURES.md:80
class: C6
verdict: OVERSTATED
evidence: tests/dashboard.test.ts:266 — `expect(dash.container.querySelector('.presence-chip')?.textContent?.trim()).toBe(t('dashboard.presenceGatedQuietHours'))`
finding: The row says the test file "covers the chip's presence/absence, not the per-reason strings", but `tests/dashboard.test.ts` does assert the localized per-reason string for `quiet-hours` (and the other reason branches are reachable through the same `gatedReasonLabel`). The row under-reports its own verification.
proposed_fix: Replace "`tests/dashboard.test.ts` covers the chip's presence/absence, not the per-reason strings." with "`tests/dashboard.test.ts` covers the chip's presence/absence plus the `quiet-hours` reason string; the remaining per-reason labels are exercised through `Dashboard.svelte::gatedReasonLabel`."
severity: P3
```


## COUNTS

### Verdict tally (429 claims)

| Verdict | Count | Share |
| --- | --- | --- |
| CONFIRMED | 360 | 83.9% |
| DRIFT | 53 | 12.4% |
| EXTERNAL-UNVERIFIED | 7 | 1.6% |
| OVERSTATED | 4 | 0.9% |
| UNSOURCED | 3 | 0.7% |
| STALE | 2 | 0.5% |
| MISSING | 0 | 0.0% |
| **Total** | **429** | 100% |

### Severity tally

| Severity | Count | Meaning |
| --- | --- | --- |
| P0 | 0 | No false security / privacy / token behaviour found. |
| P1 | 1 | A "not verified" statement contradicted by tests that exist (STATE-C047). |
| P2 | 54 | Stale file paths from the registry / split refactor, stale test counts, one truncated row, one unsupported ignore-list claim, one wrong test name. |
| P3 | 14 | Token-value-vs-code wording, line-count drift, unresolvable commit SHA, unlinked CI runs, unsupported negative claim, under-reported coverage, un-fetched vendor pages. |
| _(none — CONFIRMED)_ | 360 | — |

### Defect blocks in Section B

24 blocks covering all 69 non-CONFIRMED claims. Verdicts across the blocks:
DRIFT 15, OVERSTATED 3, UNSOURCED 2, STALE 1, EXTERNAL-UNVERIFIED 2 (one block
covers two external claims). Every non-CONFIRMED claim appears in at least one
block; no CONFIRMED claim is listed as a defect.

### Root-cause breakdown of the 53 DRIFT claims

| Stale citation | Rows | Where the code actually lives |
| --- | --- | --- |
| `src-tauri/src/lib.rs` / `lib.rs::` | 15 | `src-tauri/src/app.rs`, `cli.rs`, `deep_link.rs`, `state.rs` |
| `src-tauri/src/config.rs` / `config.rs::` | 8 | `src-tauri/src/config/{schema,clamp,snooze,patch,migrate,io,transfer}.rs` |
| `polling/poll_once.rs` / `poll_once.rs::` | 19 | `src-tauri/src/polling/{timing,write,gate,rules,presence,iteration,status_text}.rs` |
| other single-item drift | 11 | stale test names/counts (C095, C123, C335, C340, C344, C351), relocated HTTP client (C157), token-value wording (C173), wrong `filter_status` signature (C248), two rows citing a mix (`config.rs` + `poll_once.rs`) (C175, C196) |

Counts are per-row primary citation, so the two mixed rows (STATE-C175,
STATE-C196) appear once in "other" even though they also name `config.rs` and
`poll_once.rs`. Total DRIFT = 53 = 15 + 8 + 19 + 11.

Only the *paths* are wrong in 50 of the 53 DRIFT claims: the symbols exist and
the described behaviour matches the tree. That is expected fallout from the
registry split and the `config/` / `polling/` / `tray/` module splits, not from
behaviour changing — but every one of them costs a reader a detour, and three of
them (C078, C157, C248) point at code that no longer sets the behaviour the row
credits it with.

### What the audit did and did not cover

Covered in full: every row of the `Tested in main` matrix, the release-smoke
procedures and their expected log lines, the 4.7.0 smoke record, the documented
gaps, the unverified-end-to-end table, the out-of-scope list, and the reporting
section.

Evidence was gathered from `grep`, `cargo test --manifest-path
src-tauri/Cargo.toml --lib -- --list` (915 tests enumerated and counted by
module), `cargo test --manifest-path src-tauri/Cargo.toml --lib` (915 passed),
`npx vitest run` (456 passed across 39 files), `npm run test:coverage`
(74.88 / 65.54 / 78.97 / 73.56 against the 68.91 / 62.57 / 73.33 / 68.02
ratchet), `npm run check` (0 errors, 1 warning), `python3
docs/link-audit.py`, `git show v4.7.0:<path>`, and reads of
`src-tauri/tauri.conf.json`, `src-tauri/capabilities/*.json`,
`src-tauri/deny.toml`, `src-tauri/Cargo.{toml,lock}`,
`package.json` / `package-lock.json`, `.github/workflows/{ci,release}.yml`,
`.github/dependabot.yml`, `src/app.css`, `src/app.html`,
`vitest.config.js` and `playwright.config.ts`.

Not covered: vendor contracts (Graph, Spotify, GitHub Releases) — seven claims
are marked EXTERNAL-UNVERIFIED rather than guessed, per the audit's evidence
hierarchy. Live-tenant behaviour (bubble movement, the presence write-skip, the
exit-time clear round trip) is out of reach of a tree audit and is correctly
flagged ⚠ Partial in the source rows.

### Single most valuable correction

`docs/STATE-OF-FEATURES.md:26` — the availability-sync row states
"`clear_presence_on_exit` has no test at all". Two tests for exactly that
function exist and pass (`test_clear_presence_on_exit_reads_the_snapshot_not_the_clocks`
and `test_exit_cleanup_survives_the_loop_exit_tail`). Left as written, the
matrix under-reports what is verified — the failure mode this audit exists to
catch, and the only P1 in the file.
