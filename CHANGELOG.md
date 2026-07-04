# Changelog

All notable changes to PresenceJam are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).
## [Unreleased]


## [2.8.0] - 2026-07-04

### Security
- **fix(security): re-register presencejam:// scheme at every launch (further mitigates #66).** `src-tauri/src/lib.rs` previously called `tauri-plugin-deep-link`'s `register_all()` only on Windows (`#[cfg(windows)]` gate around the existing call site). The plugin's `register` is a no-op on macOS/Android/iOS (returns `Err(UnsupportedPlatform)`) and an effective re-registration on Windows (writes `HKCU\Software\Classes\<scheme>`) and Linux (writes `~/.local/share/applications/<scheme>.desktop` and runs `xdg-mime default`). This change removes the Windows-only gate so Linux also re-registers on every launch, defending against a foreign app pre-registering `presencejam://` to hijack the Spotify OAuth callback. **Windows + Linux coverage only**; macOS remains partially mitigated by #65 (PKCE verifier in AppState only, never on disk, never exposed via IPC — an interceptor can read the `code` but cannot exchange it for tokens). Native `LSSetDefaultHandlerForURLScheme` work for macOS is tracked separately. Does not modify the OAuth `redirect_uri` or `state` parameter — no Spotify re-registration required.

### Tests
- Regression guard `test_register_all_not_gated_to_windows_only` added; uses `include_str!("lib.rs")` to assert `app.deep_link().register_all()` is not gated to Windows alone.

## [2.7.5] - 2026-06-25

### Refactored
- **refactor(frontend): type the 5 invoke<any> / listen<any> call sites (PR #116, #78 part 1).** Replaces 6 untyped invoke/listen sites with typed equivalents matching the Rust-side return shapes. Closes the silent-drift risk: a Rust-side field rename now produces a TypeScript compile error, not a runtime undefined. New interfaces in `src/lib/types.ts`: `SyncStatus`, `DeviceCodeResponse`, `LogPayload`.
- **refactor: error handling consistency (PR #117, #79 items 1+2).** Three sub-changes: (1) drop the silent-failure `eprintln!` in the panic hook (`lib.rs:407`) — stderr is not connected to the user's log file on macOS release builds. (2) Add a `severity` field to the `error` event payload via a centralised `emit_error(app, source, message, severity)` helper; the polling loop's 3 error emit sites now route through it. (3) Gate the Dashboard.svelte red banner on `severity === 'error'` — warnings (transient 401-retry, backoff) no longer alarm-fatigue the user. Includes a `test_error_event_emits_severity_field` regression guard in `polling.rs`.
- **refactor(commands): split commands.rs into 7 per-workflow modules (PR #122, #76, #79).** Extracts the 24 `#[tauri::command]` handlers + 3 helpers from the single 1113-line `src-tauri/src/commands.rs` into a `src-tauri/src/commands/` directory: `config.rs`, `spotify_auth.rs`, `teams_auth.rs`, `sync.rs`, `window.rs`, `onboarding.rs`, `misc.rs`, plus a thin `mod.rs`. The `tauri::generate_handler!` macro in `lib.rs` now references the 7 submodules via globs. Closes #76. Also includes the [CMD.<GROUP>] log tag namespace rename from #79 item 3: each per-workflow file uses a `const CMD: &str = "[CMD.<GROUP>]"` so the log tag differs by command category, making the existing log_tag sweep work easier to grep.
- **refactor: extract OnboardingCache sub-struct with lock encapsulation (PR #118, #80 step 1).** Pulls the 30s onboarding result cache out of the monolithic `AppState` into its own `OnboardingCache` sub-struct. The `lock()` and `invalidate()` methods encapsulate the inner mutex; the field is private. This is the load-bearing pattern for #80 step 2 (Tokens, Polling, PendingAuths, Config).
- **refactor: extract Tokens/Polling/PendingAuths/Config sub-structs with lock encapsulation (PR #120, #80 step 2).** Continues the #80 split: 4 new sub-structs with private inner fields, lock-acquisition methods (`spotify()`, `teams()`, `handle()`, `handle_mut()`, `try_claim()`, `set_syncing()`, `current_track()`, `current_track_mut()`, `stop_tx()`, `stop_tx_mut()`, `get()`, `get_mut()`), and `Default` impls for clippy. `try_claim()` encapsulates the `compare_exchange(false, true, AcqRel, Acquire)` pattern that was raw atomic on `is_syncing` in step 1. The 38/39 pre-existing tests still pass; 3 new regression tests added.
- **refactor: extract poll_once/state/loop modules (PR #123, #72).** Extracts the 1089-line `src-tauri/src/polling.rs` into 4 files: `polling/loop.rs` (thin driver, ~50 lines), `polling/poll_once.rs` (single source of truth for one iteration, with the unified CAS-discard helper `cas_refresh_or_discard<T>` and the unified 401-retry path), `polling/state.rs` (`start_polling`/`stop_polling` thread lifecycle), `polling/mod.rs` (re-exports + `ErrorSeverity` + `emit_error`). Closes all 3 documented drift points: (1) `consecutive_pauses` is now incremented in exactly one place (the `record_no_track_outcome` helper called by both the main `Ok(None)` and 401-retry `Ok(None)` paths); (2) the `error` event is emitted in exactly one place per failed poll (the unified `Err` arm at the bottom of `poll_once.rs`); (3) the CAS-discard re-read dance is shared between Spotify proactive, Spotify 401-retry, and Teams refresh via the `cas_refresh_or_discard<T>` helper, with one canonical log message. 9 regression tests added covering all 3 drift points.
- **refactor: ts-rs build-time codegen for AppConfig + token/track/sync types (PR #121, #78 part 2).** Adds `ts-rs` v12 (`chrono-impl` feature) as a regular dependency. Derives `ts_rs::TS` and `#[ts(export, export_to = "../../src/lib/types-generated/")]` on the wire-shape structs across `spotify.rs`, `teams.rs`, `commands/sync.rs`, and `config.rs`. The generated `.ts` files land in `src/lib/types-generated/` (gitignored, regenerated by `cargo test`). `src/lib/types.ts` re-exports the generated types so existing component imports (`import type { SpotifyTokens } from '$lib/types'`) keep working unchanged. `u64` fields (`TrackInfo.progress_ms`, `TrackInfo.duration_ms`, `DeviceCodeResponse.interval`, `DeviceCodeResponse.expires_in`) override the ts-rs default `bigint` with `#[ts(type = "number")]` because Tauri's serde_json IPC bridge decodes u64 as JS number (f64) at runtime. 5 round-trip regression tests added.

### Fixed (CI)
- **ci(frontend): run cargo test --lib to materialise ts-rs codegen before svelte-check (PR #124, #125).** The `frontend` CI job now runs `cargo test --lib` (with the generated `src/lib/types-generated/` directory cleared first to avoid stale-cache issues) before `npm run check`, so the ts-rs-generated TypeScript files exist when svelte-check runs the type-re-exports from `src/lib/types.ts`. Without this, the Frontend check fails on PRs that introduce new ts-rs types because the `.ts` files are only produced when the test binary runs.

## [2.7.4] - 2026-06-25

### Security
- **deps(npm): pin cookie >= 0.7.0 via package.json overrides (GHSA-pxg6-pf52-xh8x, PR #113).** The vulnerable `cookie@0.6.0` was pulled transitively via `@sveltejs/kit@2.68.0`. A top-level `overrides` block in `package.json` forces resolution to `^0.7.0` across the entire transitive graph. Dev-only (vite/svelte-kit dev server); no production binary impact.
- **security(ci): switch homebrew job to credential helper, document 90-day PAT rotation (PR #114, #68 finish).** The `homebrew` job in `release.yml` previously cloned the tap with `x-access-token:${HOMEBREW_TAP_TOKEN}@github.com/...` — the token leaked into `git remote -v` output, the process listing, and any error log captured by the job. Replaced with a non-persistent `git config credential.helper` that hands the token to git on demand. Added a new "Release Pipeline Token Rotation" section to `SECURITY.md` documenting the rotation procedure, why fine-grained PATs (not classic), and why 90 days.

### Fixed
- **refactor(frontend): type the 5 invoke<any> / listen<any> call sites (PR #116, #78 part 1).** Replaces 6 untyped invoke/listen sites with typed equivalents matching the Rust-side return shapes. Closes the silent-drift risk: a Rust-side field rename now produces a TypeScript compile error, not a runtime undefined. New interfaces in `src/lib/types.ts`: `SyncStatus`, `DeviceCodeResponse`, `LogPayload`.

### Changed
- **refactor: error handling consistency (PR #117, #79 items 1+2).** Three sub-changes: (1) drop the silent-failure `eprintln!` in the panic hook (`lib.rs:407`) — stderr is not connected to the user's log file on macOS release builds, so the panic was invisible. (2) Add a `severity` field to the `error` event payload via a centralised `emit_error(app, source, message, severity)` helper; the polling loop's 3 error emit sites now route through it. (3) Gate the Dashboard.svelte red banner on `severity === 'error'` — warnings (transient 401-retry, backoff) no longer alarm-fatigue the user. Includes a `test_error_event_emits_severity_field` regression guard in `polling.rs`.

### Refactored
- **refactor: extract OnboardingCache sub-struct with lock encapsulation (PR #118, #80 step 1).** Pulls the 30s onboarding result cache out of the monolithic `AppState` into its own `OnboardingCache` sub-struct. The `lock()` and `invalidate()` methods encapsulate the inner mutex; the field is private. This is the load-bearing pattern for #80 step 2 (Tokens, Polling, PendingAuths, Config). Includes 2 regression tests: `test_onboarding_cache_lock_and_invalidate` (exercises the public API) and `test_onboarding_cache_encapsulation_no_direct_state_access` (grep guard against re-exposing the field).

## [2.7.3] - 2026-06-25

### Security
- **fix: per-install keychain namespacing (audit M2).** `keychain.rs:19-34` — `SPOTIFY_CLIENT_SECRET_USER` is now namespaced by the Tauri bundle identifier (`spotify_client_secret:com.presencejam.app`). Side-by-side installs on the same OS user (prod, dev, beta) now get isolated slots. `get_spotify_client_secret` falls back to the legacy unnamespaced slot used through v2.7.2, migrates the value forward to the namespaced slot, and deletes the legacy entry — so existing v2.7.2 users do not have to re-onboard. `has_spotify_client_secret` and `delete_spotify_client_secret` consult both slots (legacy delete is best-effort).
- **fix: strip plaintext Spotify client_secret from config.json on startup (audit Q3).** `config.rs` adds `migrate_legacy_client_secret()`, called from `lib.rs:441` after `load_config`. If `config.json` contains a plaintext `spotify.client_secret` field (legacy ≤ v2.5.0), the value is written into the OS keychain and the plaintext is atomically stripped from the file. Conflict policy: if the keychain already holds a *different* secret, the migration is a no-op (the user is told to Reconnect via Settings) so a multi-install upgrade cannot clobber a working keychain entry.

### Fixed
- **fix(polling): count `SpotifyApiError::Other` toward `transient_failure_count` (audit M1).** `polling.rs:871-886` — the 5-strikes exit-to-reconnect-required previously only counted `RateLimited` and `ExpiredToken`. A reqwest send failure (DNS, TLS handshake, connection refused) or a non-200/204/401/429 HTTP response is wrapped into `Other`; that variant is now treated as transient so a permanent network outage eventually triggers `reconnect-required` instead of looping forever emitting `error` events.
- **feat: actionable keychain error on Linux (audit Q7).** `keychain.rs` adds `keychain_error_help()` / `map_keychain_err()` helpers that wrap every `Entry::new` / `set_password` / `delete_credential` call site. When the keychain is unavailable (no Secret Service daemon, locked `gnome-keyring`, missing `kwallet`), the returned error message points the user at `SETUP.md#linux-keyring` with install commands for the major distros and a `secret-tool` self-check recipe. `SETUP.md` adds a new "Linux: System Keyring Required" section documenting the dependency and the `secret-tool` smoke test. No encrypted-config fallback is added — a working keyring is a hard requirement, by design.
- **fix(frontend): `TeamsTokens.refresh_token` is `string | null` (audit S2).** `src/lib/types.ts:32` was previously typed as `string`, but the Rust side (`teams.rs:57`) declares `pub refresh_token: Option<String>` with `#[serde(default)]` and no `skip_serializing_if` — so the field is always emitted, defaulting to JSON `null` when the Microsoft token endpoint doesn't return one. TS was lying about the wire shape. Spotify's `refresh_token` is a plain `String` on both sides and remains `string`. No frontend code currently reads `.refresh_token` directly, so this is a latent-bug fix with zero call-site impact.
- **fix: forward deep-link argv via single-instance plugin (audit S5).** `lib.rs:340-362` — the `tauri_plugin_single_instance` callback was a no-op log line. Now it (1) raises, un-minimizes, and focuses the existing main window when a second instance launches, and (2) scans `argv` (skipping argv[0] = exe path) for any `presencejam://` URL and forwards it through `handle_deep_link`. macOS deep-link delivery via the plugin's `on_open_url` callback was already wired and is unaffected.

### Changed
- **chore(deps): drop unused `tauri-plugin-process` (audit Q6).** The plugin was registered in `lib.rs:372` and declared in `Cargo.toml:28` and `package.json:23`, but no frontend code imports `@tauri-apps/plugin-process` and no Rust code calls into the plugin's IPC. All three registration points and the `ACKNOWLEDGEMENTS.md` table entries have been removed. No behaviour change; pure attack-surface reduction.
- **feat(macOS): hide dock icon when Start minimized is on (audit Q4).** `lib.rs` setup now calls `app.set_activation_policy(ActivationPolicy::Accessory)` when `start_minimized` is true, and `commands::save_config` does the same on every save (symmetrically setting `Regular` when the user disables it) so the dock icon and menu-bar app menu disappear for tray-only use. No restart required to re-enable the dock icon. The change is `#[cfg(target_os = "macos")]`-gated; Windows and Linux are unaffected.
- **chore: User-Agent version from `CARGO_PKG_VERSION` (audit Q8).** `teams.rs:52` now uses `format!("PresenceJam/{}", env!("CARGO_PKG_VERSION"))` instead of the hardcoded `"PresenceJam/2.0"`, which had drifted since v2.0.0. `CONTRIBUTING.md` adds the rule "User-Agent and any version-stamped payload must use `env!("CARGO_PKG_VERSION")` — never hardcode the version." Spotify's `reqwest::blocking::Client` is unaffected — it doesn't set a User-Agent at all (pre-existing gap, out of scope).

### Chore
- **chore(nits): three mechanical cleanups.** `tauri.conf.json:30` drops the unused `https://api-secure.spotify.com` from the CSP `connect-src` (no Rust code calls it). `teams.rs:78` drops the redundant `serde(rename = "verification_uri")` on `DeviceCodeResponseRaw.verification_uri` (the field is already named that; only the `alias = "verification_url"` is doing real work). New `teams::is_token_expired(&TeamsTokens)` mirrors `spotify::is_token_expired`; the two inline `Utc::now() [<>]= expires_at - 60s` checks at `polling.rs:131` and `teams.rs:548` now call the helpers (the polling.rs import is aliased `is_token_expired as is_teams_token_expired` to avoid shadowing the Spotify one).

### Verified (no code change)
- **Verified: keychain cache priming at startup is race-free (audit Q1).** `lib.rs:406-415` reads the keychain on startup to populate the `OnceLock<RwLock<Option<String>>>` cache before the polling thread's first iteration. Because `keychain::store_spotify_client_secret` (called from `start_spotify_auth` during onboarding) also writes the cache on success, there is no race window where the polling thread could see a stale empty cache after onboarding completes. Left as-is.
- **Verified: tray Show/Hide label regression is fixed (audit Q5).** `tray.rs:158-211` already includes `is_window_visible` in the dedup key (`TrayStateSnapshot = (bool, bool, Option<String>)` for `(is_syncing, is_window_visible, track_key)`); `update_tray_menu` reads `window.is_visible()` at line 200 before computing the label. Per v2.6.4 #71 fix. No code change needed.

## [2.7.2] - 2026-06-20

### Fixed
- **fix(v2.7.2): verifier-flagged nits + release-hygiene catch (#92).** `Settings.svelte:73` log tag rename (`[SETTINGS] start_spotify_auth failed:` → `[SETTINGS] start_spotify_reconnect failed:`) — the catch block is for the reconnect-required listener (added in v2.7.1), not the original auth. `commands.rs:298` idiomatic `let _ = client_secret;` → `_client_secret` at declaration. `Cargo.lock` presence-jam version 2.7.0 → 2.7.1 (missed by the v2.7.1 release commit 856b613).

### Changed
- **chore(v2.7.2): a11y fix + project-wide rustfmt pass (#93).** `Settings.svelte:195` orphan `<label>` (no associated control — the client secret is stored in the keychain) changed to `<span class="form-label">` with a matching CSS rule. Project-wide rustfmt cleanup (was failing on `lib.rs:389/393/396` trailing whitespace — pre-existing rustfmt 1.9.0 internal bug; once stripped, rustfmt's backlog of legitimate reformatting was unblocked). Mechanical reformatting only: import reordering, long log/if/chain calls broken to multi-line. `cargo fmt --check` is now clean for the first time.

### Refactor
- **refactor: delete frontend dead stores, extract shared types (#91).** `src/lib/stores/spotify.ts` and `src/lib/stores/teams.ts` were writable stores that nothing ever wrote to. They also exported three type definitions (`SpotifyTokens`, `TrackInfo`, `TeamsTokens`) used in 4+ places. Extracted the types to new `src/lib/types.ts` and deleted the dead writables. `Settings.svelte` catch-block fallback (`isConnected = $spotifyConnected`) was always reading the never-written `false` default — replaced with explicit defaults.

### CI/Build
- **feat(ci): add Linux (.deb + .AppImage) to release matrix (#94).** Release workflow now builds Debian/Ubuntu (.deb) and AppImage artifacts alongside the existing macOS DMG and Windows MSI. `ubuntu-22.04` runner; Tauri 2's `tauri build` produces both formats in one invocation. No signing required on Linux (unlike macOS Gatekeeper / Windows SmartScreen). Skipped: .rpm, flatpak, snap, arm64. README updated with install instructions and the macOS unsigned-DMG Gatekeeper workaround note.

## [2.7.1] - 2026-06-19

### Fixed
- **fix: Reconnect Spotify flow was permanently broken (#88).** `Reconnect.svelte` and `Settings.svelte` were calling `start_spotify_auth` with `clientSecret: ''`, expecting the backend to read from the keychain. The #67 validator (≥32 chars) correctly rejected the empty string, so the Reconnect button was non-functional in shipped v2.6.4 / v2.7.0 builds. Fix: new `start_spotify_reconnect(client_id, redirect_uri)` IPC that reads the existing `client_secret` from the OS keychain and runs the same PKCE flow. The shared OAuth flow was extracted into a private `run_spotify_oauth_flow` helper so `start_spotify_auth` (writes secret to keychain) and `start_spotify_reconnect` (reads from keychain) share one implementation.

## [2.7.0] - 2026-06-19

### Refactor
- **refactor: dedup auth-listener setup across 3 Svelte components (#73).** Extracted a shared `useAuthListeners()` helper and an `authFlow` Svelte 5 reactive store to deduplicate the 4-event listener block (spotify-auth-complete/failed, teams-auth-complete/failed) across `Onboarding.svelte`, `Settings.svelte`, and `Reconnect.svelte`.
- **refactor: single source of truth for status-format placeholder substitution (#74).** Moved the placeholder substitution for the status-format template into Rust so the Svelte live preview and the runtime polling loop share one implementation.
- **refactor: extract pkce module, dedup helpers (#75).** Extracted the PKCE challenge/verifier generation logic from `spotify.rs` into a dedicated `pkce.rs` module.

### CI/Build
- **ci: pin third-party action SHAs and document token scoping (#68).** All third-party GitHub Actions are now pinned to full-length commit SHAs instead of version tags. Documented the required token scopes for `HOMEBREW_TAP_TOKEN` and `WINGET_TOKEN`.
- **ci: run cargo test and npm check on PRs (#81).** Added a `ci.yml` workflow that runs `cargo test`, `cargo clippy`, and `npm run check` (Svelte type-check) on every PR.

### Deferred
- **#66 (deep-link hijack)** remains deferred to v2.7.1. Per-launch custom-scheme registration (OS-specific) is required for the full fix. Partial mitigation from #65 (PKCE verifier in `AppState` only) is still in place.

### Follow-ups (not addressed here)
- **#71 (tray Show/Hide label regression).** The menu label doesn't update after clicking "Hide Window" — the dedup key omits `window.is_visible()`. Tracked separately.
- **Reconnect Spotify flow.** `Reconnect.svelte` and `Settings.svelte` pass `clientSecret: ''` which the #67 validator rejects. Pre-existing bug, not introduced by this release.
- **Frontend dead stores.** `src/lib/stores/spotify.ts` and `src/lib/stores/teams.ts` are still present. Tracked separately.

## [2.6.4] - 2026-06-14

### Security
- **security: tokens.json security boundary (#65).** Dropped `"store:default"` from `capabilities/default.json` — the webview no longer has any path to the tokens file. Deleted the `get_spotify_tokens` and `get_teams_tokens` Tauri commands (registered but unused, and a token-exfil endpoint in waiting). Stopped persisting `pending_spotify_auth.verifier` and `pending_teams_auth.verifier` (PKCE verifier + Teams device code) to disk — both are 10–15 min bearer credentials that filesystem-level attackers could read. Replaced `tauri-plugin-store` token I/O with a new `token_io` module that writes `<app-config-dir>/PresenceJam/tokens.json` **atomically** using temp-file + rename + fsync (mirroring the `config::save_config` pattern). A process kill mid-write can no longer corrupt the tokens file and bounce the user back through Onboarding.
- **security: backend input validation at IPC boundary (#67).** Moved the Spotify `client_id` (`^[A-Za-z0-9]{32}$`) and `client_secret` (≥32 chars) regex checks from the frontend to `start_spotify_auth` — a devtools-pasted `invoke()` with arbitrary strings was previously accepted. Hardened `validate_http_url` to reject URLs with no host and with `userinfo` (`user:pass@`). Deleted the non-manual `complete_spotify_auth` command — it accepted `verifier`/`client_id`/`redirect_uri` from the webview unverified, and the manual variant covers all real flows.
- **security: dead Tauri commands + frontend stores removed (#77).** Deleted 7 commands that were registered but never invoked: `open_external` (alias of `open_external_url`), `hide_window`, `get_autostart_enabled`, `get_recent_logs`, `get_config_dir`, `get_current_track`, `complete_teams_auth_manual`. The frontend stores `spotify.ts` and `teams.ts` (writables that nothing ever wrote) are still present — see follow-up below.

### Fixed
- **fix: polling state machine races (#69).** `start_syncing` now drains the previous polling thread via `stop_polling_and_join` **before** claiming the `is_syncing` flag. A fast Stop+Start cycle (within the 2s stop budget) can no longer leave a stale thread running while a new one starts. The OS keychain is no longer hit on every polling iteration: `keychain::get_spotify_client_secret` now caches the secret in a module-level `OnceLock<RwLock<Option<String>>>`, and the polling thread's hot path uses a new `peek_spotify_client_secret` (cache-only). The cache is primed once at app start, eliminating the macOS keychain prompt mid-poll.
- **fix: onboarding cache was never invalidated (#70).** The 30s result cache for `is_onboarding_complete` is now cleared in every token-mutating command (`complete_spotify_auth_manual`, `poll_teams_auth`, `complete_teams_auth_manual`, `reconnect_spotify`, `reconnect_teams`, `handle_spotify_callback`, `handle_teams_callback`). The user is no longer told "onboarding not complete" for up to 30s after a successful reconnect.
- **fix: tray menu concurrent rebuilds (#71).** `update_tray_menu` now skips the full menu rebuild if `is_syncing` and the current track key haven't changed (the polling thread called this on every successful poll; the menu only needs to change when the state actually changes). A module-level `Mutex<()>` serialises the two writers (polling thread + frontend command) so a `set_menu` from one never interleaves with the other.

### Deferred
- **#66 (deep-link hijack) was deliberately not fully fixed in this release.** A per-launch UUID in the redirect URI path was the proposed defence, but Spotify requires exact redirect-URI match in the registered app — a path component breaks the OAuth round-trip. A full fix needs per-launch custom-scheme registration (OS-specific: Windows registry, macOS `LSSetDefaultHandlerForURLScheme`, Linux XDG MIME). The threat is **partially mitigated** by #65: a foreign app that intercepts `presencejam://callback?code=***` can read the code, but cannot exchange it for tokens — the PKCE verifier is in `AppState` only, not on disk, and not exposed via any IPC. Tracked for a follow-up release.

### Follow-ups (not addressed here)
- Delete `src/lib/stores/spotify.ts` and `src/lib/stores/teams.ts` (writables that nothing writes). The verifier flagged these as still present; they're cosmetic dead code at this point, not a security risk. Tracked separately.
- Update `Settings.svelte:7-8` to drop the `import { spotifyConnected } from '$lib/stores/spotify'` and the dead `isConnected = $spotifyConnected` fallback in the catch block. Tracked separately.

## [2.6.3] - 2026-06-10

### Fixed
- fix(race): drop the double-claim on `is_syncing` between `commands::start_syncing` and `polling::start_polling`. Fresh installs could not complete onboarding — every Finish click after a successful Spotify + Teams auth hit `"Polling is already running"` and rolled back to no-sync state. `commands::start_syncing` is now the **sole claimer** of the flag; `polling::start_polling` is a pure thread-spawner that trusts its caller. A source-grep regression guard (`test_start_polling_does_not_claim_is_syncing`) catches a re-introduced `compare_exchange` inside `start_polling`. Closes #60.
- fix(autostart): gate `disable()` on `is_enabled()` to swallow the "key not present" case. On Windows, `RegDeleteValueW` on a missing Run-key entry returned `os error 2` on every `save_config` call when autostart was never enabled (or was removed externally). The plugin no-op path now returns `Ok(())` with an `info!` log and never calls the registry. Closes #61.
- fix(security): truncate the Microsoft Graph token-poll body in debug logs. The full token response (access_token + refresh_token, ~3.5 KB, ~77 min lifetime, `Presence.ReadWrite` + 50+ scopes) was previously written to the log file at `debug!` level. The `truncate_for_log` helper now records only the first 256 chars + byte count — enough to recognise the error envelope shape, not the credential. The truncation is applied at every `raw_body` interpolation in `teams.rs`: the three success-path debug logs, the three `info!`-level request-body logs, and the eight user-facing error format strings used by token parse failures, device-code failures, and unknown error fallbacks. A second pass hardened the helper against the UTF-8 panic risk at byte 256 (now cuts at a char boundary via `char_indices().nth(256)`), with four unit tests covering the under-limit, ASCII-boundary, multibyte-boundary, and truncation cases. Closes #62.
- fix(ui): make the build version reflect the actual build in both the About panel and the main page footer. The Vite `define` used a bare `__APP_BUILD__` token that esbuild's `define` plugin only matches as a top-level identifier; the consumers read `import.meta.env.__APP_BUILD__` (a member expression), which esbuild never matched against the bare-token define. Result: both surfaces had been hardcoded-fallback'd since 2.6.0 (the v2.6.1 "fix" was a prettier fallback string, not a real fix). Switched to the canonical `import.meta.env.VITE_APP_BUILD` path-based define, swapped `||` for `??` in both consumers (so an empty version doesn't fall through to the dev-build label), and used `'dev build'` as the footer fallback (more honest than the previous hardcoded `'2.6.0'`). The build version is also logged to the DevTools console at boot for faster stale-install triage. Closes #63.

Closes #60 #61 #62 #63

## [2.6.2] - 2026-06-09

### Chore
- chore(release): re-submit to winget. v2.6.1 winget submission got rejected at validation because the manifest pointed to the v2.6.0-named MSI (deleted when the v2.6.1 MSI was uploaded). The winget-releaser action's `komac update` skips on ANY existing PR for the version (open or closed), so closing the broken PR didn't help — only a version bump produces a fresh submission. No source changes; identical binaries as v2.6.1.

## [2.6.1] - 2026-06-09

### Fixed
- fix(ui): show build date in About panel as ISO date instead of Unix epoch milliseconds. The previous `${pkg.version}.${Date.now()}` format produced strings like `2.6.0.1749350400000` in the About panel; now shows `2.6.1 (2026-06-09)`.

### Chore
- chore(docs): condense the stale `retention_days` historical NOTE in `src/lib/stores/config.ts` from 4 lines to 1. Field was removed in v2.6.0 (PR #49, GH #13); the 4-line block was carrying its own weight as removed code rather than a note.

## [2.6.0] - 2026-06-08

### Security
- fix(security): use HTTPS for Authenticode timestampUrl (#28, PR #44)
- chore(security): strip 12+ unused Tauri capabilities (#29, PR #44)

### Fixed
- fix(race): Spotify token-refresh lost-update race (#35, PR #43)
- fix(race): Teams token-refresh lost-update race (#36, PR #43)
- fix(race): apply CAS guard to 401-retry refresh (#PR #43)
- fix(polling): close if_committed block in 401-retry CAS guard (PR #43 follow-up)
- fix(config): wire polling interval config fields; remove dead retention_days (#37, PR #45)
- fix(polling): thread configured default_interval_seconds into pause_backoff (PR #45 follow-up)
- fix(polling): reset consecutive_pauses before debounce early-return on track resume (PR #45 follow-up)
- fix(ux): is_first_poll guard prevents legitimate presence-clearing on app start (#39, PR #45)
- fix(lifecycle): wire polling-thread-panicked and reconnect-required listeners in frontend (#33, PR #44)
- fix(oauth): re-check pending auth expiry at submit time (#34, PR #44)
- fix(perf): use spawn_blocking for OAuth callback HTTP calls (#42, PR #46)
- fix(perf): distinguish panic from cancellation in spawn_blocking error messages (PR #46 follow-up)

### Changed
- refactor(state): use AtomicBool for is_syncing flag (#32, PR #44)
- refactor(cmd): make is_onboarding_complete async with 30s result cache (#41, PR #47)
- perf(api): pause-aware exponential backoff in polling loop (#38, PR #45)
- perf(api): validate_*_token checks local expires_at before network call (#40, PR #45)
- refactor(cmd): extract ONBOARDING_CACHE_TTL as a named constant (PR #47 follow-up)

### Chore
- chore(deps): drop unused tokio dependency (#30, PR #44)
- chore(build): commit Cargo.lock for reproducible builds (#31, PR #44)

### Cleanup (outstanding issues)
- chore(security): migrate Spotify client_secret to OS keychain (closes #9, PR #49)
- chore(logging): demote entry/exit logs to debug level (closes #12, PR #49)
- fix(config): remove retention_days from Rust/TS (closes #13, PR #49)
- refactor(commands): consolidate get_*_tokens via shared helper (closes #16, PR #49)

## [2.5.0] - 2026-05-30

### Fixed

- Teams refresh token rotation — now preserves existing refresh token when Microsoft doesn't return a new one
- Config atomic write on Windows — uses backup-and-rename pattern instead of unsafe remove-then-rename
- Polling config defaults aligned between TypeScript (10s) and Rust (was 5s, now 10s)
- Manual URL paste error now displays to user instead of silently spinning
- Polling config interval fields removed (were dead code — engine uses hardcoded constants)
- Over-permissioned Tauri capabilities — 5 unused permissions removed
- Version string in vite.config.js now reads from package.json dynamically
- Clippy warnings fixed (needless_borrow, manual_clamp x2)
- Quit handler now logs exit failure instead of silently discarding Result
- Stale config.tmp files cleaned up on startup
- Auth callback expiry now checked at callback time (defense-in-depth)
- Token length removed from production logs (downgraded to debug)
- Clear-on-pause now sends "🎵 Paused" placeholder instead of empty string
- Tray menu builds partial menu on item failure instead of aborting entirely
- npm audit: 5 vulnerabilities fixed (2 high, 2 moderate, 1 low)

### Improved

- Tray menu separator logic simplified — no double separators when idle
- User-Agent now reports actual app version from CARGO_PKG_VERSION
- Auth callback expiry check added for both Spotify and Teams
- AssertUnwindSafe in polling loop now has safety comment
- Log retention enforcement added — deletes logs older than configured retention_days

### Removed

- Dead Credentials struct and load/save functions (~132 lines)
- Dead start_spotify_auth() function (unused — command generates its own PKCE)
- Dead frontend token stores (spotifyTokens, teamsConnected) and unused Settings imports

### Security

- Capabilities reduced to minimum required permissions
- Token metadata no longer logged at info level


## [2.4.2] - 2026-05-05

### Fixed

- **is_first_poll guard brace placement** — tightened indentation of the `if !is_first_poll` block around `clear_teams_status_message` in `process_track` so the control flow is readable
- **Inconsistent event payload conventions** — standardised all polling-path event emissions (`reconnect-required`, `spotify-reconnect-required`, `teams-reconnect-required`) to use `serde_json::json!(null)` instead of bare `()` tuples, matching the existing `polling-thread-panicked` payload shape
- **is_syncing wedge on spawn failure** — `start_polling` now resets `is_syncing` and clears `stop_tx` in the `map_err` path when `thread::Builder::spawn` fails, preventing future `start_polling` calls from being permanently rejected

- **B1: Flicker on startup** — `handle_no_track` now skips on first poll to avoid clearing Teams status before any track was ever set; sends "🎵 Nothing playing on Spotify" instead of empty string
- **B3: "Unknown" status message** — `clear_teams_status_message` now receives a human-readable placeholder text instead of empty string
- **B4: Duplicate polling thread** — `start_polling` now guards against spawning a duplicate thread using a `stop_tx.is_some()` check in addition to the `is_syncing` lock
- **is_onboarding_complete: Non-401 errors treated as valid** — RateLimited (429) → valid; all other errors → invalid (was incorrectly treating all errors as valid)
- **complete_onboarding: Missing tokens accepted** — now returns error if Spotify or Teams tokens are missing instead of always succeeding
- **refresh_spotify: State updated after persistence** — AppState tokens now updated before persistence; if persistence fails the error is returned rather than silently consumed
- **onboarding Teams auth: stale error not cleared** — `connectTeams` now clears `teamsAuthError` before starting new auth; `pollTeamsAuth` now guards against empty `teamsDeviceCode`
- **R2: Infinite retry on transient failures** — polling loop now tracks `transient_failure_count` (max 5); after 5 consecutive 5xx/network failures, exits and emits reconnect-required event
- **R5: Polling thread panic kills loop silently** — `start_polling` now wraps the polling loop in `panic::catch_unwind` with proper downcast logging; emits `polling-thread-panicked` event on panic so the frontend can react
- **is_first_poll never flipped on track-playing first poll** — `is_first_poll` flag now reset to `false` on `Ok(Some(track))` path as well, ensuring `clear_on_pause` guard fires correctly from the first poll onward
- **transient_failure_count incremented for non-transient errors** — counter now only increments for `RateLimited` and `ExpiredToken` variants; auth/Other errors are non-transient and do not contribute to the retry limit
- **transient_failure_count not reset on Ok(None)** — counter now resets to 0 in the `Ok(None)` arm so any successful poll (even with no track) breaks the failure streak
- **Auth errors not displayed to user in connectSpotify** — `connectSpotify` catch block now sets `spotifyAuthError` so backend errors like missing credentials are visible to users
- **Missing closing paren in redirect URL placeholder** — `Onboarding.svelte` redirect URL input placeholder now correctly ends with `...)"` instead of truncated `"`
- **R7: Empty client_id/client_secret accepted** — `start_spotify_auth` now rejects empty credentials early with a clear error
- **Redirect URI: wrong URL in onboarding instructions** — onboarding step 3 now says `presencejam://callback` instead of `http://localhost:43210/callback`
- **HTTP body errors silently dropped** — `unwrap_or_default()` on `response.text()` in `spotify.rs` and `teams.rs` now properly propagated as errors instead of discarded
- **Spotify auth errors silently swallowed** — `handleManualUrlPaste` and `connectSpotify` now display errors to user via `spotifyAuthError`; stale errors cleared on retry

### Security (assessed — no code changes)

- **S2: CSP unsafe-inline** — required for Svelte scoped styles; cannot be removed without rearchitecting CSS handling
- **S3: redirect_uri sent from frontend** — always sends `presencejam://callback` which is in the allowlist; defended
- **S4: OAuth scopes hardcoded** — validated by Spotify's OAuth server at runtime; low risk for desktop app
- **L1: Teams device code flow CSRF** — device code flow is CSRF-resistant per NIST 800-63C; not applicable
- **L2: PKCE verifier in tauri-plugin-store** — store uses OS keychain encryption (DPAPI/Keychain); acceptable
- **L3: open_logs_folder file://** — path is `app_log_dir()` (OS-controlled app directory); not user-controlled

### Already correct (confirmed)

- **R3: pending_auth lost on crash** — already wired in `lib.rs`; auth state survives page reload
- **R4: profanity_filter not wired** — already confirmed wired; `process_track` calls `filter_profanity()`
- **R6: polling drift** — both Spotify and Teams polled in same single loop; cannot drift independently

## [2.4.1] - 2026-04-28

### Fixed

- Settings: reconnect buttons now properly trigger auth flow when clicked (were silently failing before)
- Settings: show "Reconnecting..." state while auth is in progress
- Polling: Teams token now auto-refreshes when expired (similar to existing Spotify behavior)
- Dashboard: "Go to Setup" checks for both client_id AND client_secret before routing to reconnect vs onboarding
- Reconnect: checks for both credentials before allowing reconnect; auto-start only when credentials exist
- Version: corrected version strings to 2.4.1

## [2.4.0] - 2026-04-27

### Added

- `SETUP.md` — First-time setup guide: installing the app, registering a Spotify Developer app, connecting Teams, file locations, uninstalling
- `USAGE.md` — Day-to-day guide covering the system tray, dashboard, settings, log viewer, and common tasks

### Changed

- `README.md` rewritten as entry point — shorter, no duplication, links to all docs
- `CONTRIBUTING.md` — project structure section removed (now in ARCHITECTURE.md), references updated
- `CLAUDE.md` — stripped to conventions and key files only; duplicated tech stack and feature list removed

### Removed

- `BUGFIX_TRACKER.md` — moved to GitHub Issues; no longer ships with the repo

### Fixed

- Polling: interruptible stop channel replaces thread::sleep — tray freeze on pause is now instant (fixes #10)
- Polling: `get_sync_status` validates tokens via real API calls, emits reconnect-required events on failure (fixes #11, #12)
- Config: `clear_on_pause` now respected — Teams status only cleared when Spotify pauses if user enabled it (fixes #4)
- Config: `start_minimized` properly wired to Rust `TeamsConfig`, app window hides on startup when set (fixes #5)
- Config: `launch_at_login` moved to `AppConfig.autostart`, binding fixed to `localConfig.autostart`, syncs to OS autostart manager (fixes #6)
- Polling: refreshed Spotify tokens now persisted to tauri-plugin-store so they survive app restarts (fixes #8)
- Deep links: Windows deep link registration now runs in release builds (was debug-only) (fixes #9)
- Onboarding: `is_onboarding_complete` validates tokens via API calls instead of just checking presence (fixes #10, #12)
- Tray: initial tray menu now reflects actual sync state via immediate `update_tray_menu` call (fixes #11)
- Sync: `start_syncing` TOCTOU race fixed by acquiring write lock before checking `is_syncing` (fixes #14)
- OAuth: redundant `client_id` removed from `refresh_spotify_token` form body (was doubled in Basic auth) (fixes #19)
- Tray: tray menu updated after each track change via polling loop call to `update_tray_menu` (fixes #24, #25)
- Tray: `update_tray_menu` errors now logged at warn! level instead of silently ignored (fixes #7)
- Polling: `progress_ms` corrected by elapsed time since last poll to prevent stale position data (fixes #13)
- Onboarding: placeholder OAuth URL replaced with real Spotify app setup instructions (fixes #15)
- Frontend: Dashboard now handles `sync-started`/`sync-stopped` events from backend (fixes #16)
- Polling: 500ms debounce added to prevent Teams status flicker on rapid track changes (fixes #17)
- Quit: redundant quit handler thread removed — `on_window_event(CloseRequested)` handles it (fixes #18)
- LogViewer: wired to `tauri_plugin_log` Webview target, listens on `log://log` event (fixes #21)
- Auth: pending Spotify/Teams auth state persisted to store with expiry, recovered on startup (fixes #22, #23)
- Config: `save_config` now holds config write lock for entire read-modify-write to prevent race (fixes #26)

### Developer

- Frontend: removed unused `isSyncing` store from `app.ts` — stores are actively used, not dead code (note: Bug 20 stores are actually the navigation backbone and were correctly preserved)

## [2.3.6] - 2026-04-24

### Added

- Application menu bar with File, Edit, View, Help menus (macOS/Windows)
- Dynamic tray menu that updates based on sync state (Pause/Resume toggles automatically)
- Build number displayed in bottom-right corner of app window (injected at build time)
- About dialog accessible from Help menu

### Fixed

- Duplicate tray icon: removed automatic tray icon from tauri.conf.json (was conflicting with manual setup)
- Tray icon now properly displays in macOS menu bar
- Left-click tray: now shows window via tray-click event (not automatic menu popup)
- Tray menu label now refreshes after show/hide toggle
- Back-to-back separators fixed when no track is playing

### Changed

- `update_menu_state` command renamed to `update_tray_menu_state` for clarity

## [2.3.5] - 2026-04-23

### Fixed

- Teams auth: `pending_teams_auth` now populated during device code flow, fixing `complete_teams_auth_manual` (closes #8)
- Onboarding: `is_onboarding_complete` now checks both Spotify and Teams auth status (closes #11)
- URL validation: `open_external_url` and `open_external` now reject non-http(s) schemes (closes #14)
- Polling: retry intervals now include +/- 20% jitter to prevent thundering herd (closes #17)
- Polling: HTTP requests have 10s timeout, stop_syncing bounded to 2s (partial fix for #10)

### Security

- CSP: replaced broad `*.microsoft.com` wildcard with explicit `login.microsoftonline.com` and `graph.microsoft.com` (closes #15)
- URL commands: only http/https schemes allowed, preventing javascript: and file: attacks

### Documentation

- Token loading duplication in commands.rs documented as intentional (closes #16 - won't fix)

## [2.3.4] - 2026-04-12

### Fixed

- Memory leak: event listeners not cleaned up on component unmount (Onboarding)
- Polling: 401 responses now trigger token refresh instead of silent failure
- Config: default max polling interval now 60s (was incorrectly set to 10s)
- Config: atomic file writes prevent data loss on save failure
- Config: Windows rename fix — removes destination file before rename to prevent failure when file exists
- Auth: CSRF state now properly verified on Spotify OAuth callback
- Auth: CSRF state persisted to store for crash recovery
- Shutdown: polling thread now properly stopped on app quit
- UI: buttons no longer allow double-click during async operations
- UI: error messages now properly display instead of "[object Object]"
- Polling: stale error in retry path now correctly emitted to frontend
- Polling: `get_spotify_credentials()` helper removes credential extraction duplication
- Polling: `SpotifyApiError::RateLimited` errors get 60s backoff instead of 30s

### Security

- CSRF protection now functional for Spotify OAuth flow
- Credentials file writes now atomic (prevents corruption)

## [2.3.3] - 2026-04-12

### Fixed

- Clear validationError when connections complete and fix log placement

## [2.3.2] - 2026-04-12

### Fixed

- No functional changes — re-merged PR #4 (fix/bugfixes-2.3.1) onto updated main

## [2.3.1] - 2026-04-11

### Fixed

- Proper async cleanup of event listeners in Onboarding (prevent memory leaks)
- Validate connections before finishing onboarding
- Correct toggle bindings in Settings.svelte
- Align polling interval defaults with Rust backend
- Display error events to user in Dashboard
- Add validation in extractCodeFromUrl
- Store setTimeout reference for proper cleanup in Settings

## [2.3.0] - 2026-04-12

### Added

- Profanity filter for Spotify track status (`profanity_filter` toggle in Settings)
- Customizable profanity placeholder text (`profanity_placeholder`) with `{emoji}` token support
- Leetspeak normalization (1→i, 3→e, $→s, @→a, 0→o, etc.) and repeated-char collapse
- Word-boundary detection to avoid false positives (class, assassin, cocktail, etc.)
- Safe default placeholder: "Currently Listening to Spotify"

### Changed

- `config.teams.clear_on_pause` field restored (was dropped during serde round-trip)
- Logging: profanity filter no longer logs original profane status (security hardening)
- Polling: added TODO note about future refactor to filter raw Spotify fields before formatting

### Fixed

- `matches_at_pos` boundary tracking: was skipping arbitrary chars instead of only repeated chars
- `at_end` match: now validates left boundary before detection (peacock/cock false positive)
- Config test: uses `profanity::safe_placeholder_default()` instead of hardcoded literal
- Frontend TS config: added NOTE that Rust is canonical source for placeholder default

## [2.2.0] - 2026-04-11

### Added

- macOS support (Apple Silicon build)
- GitHub Actions CI/CD pipeline for automated releases
- Automatic builds for Windows (.msi) and macOS (.app zip) on tag push

### Changed

- Version bumped to 2.2.0 for macOS release

## [2.1.0] - 2026-04-11

### Fixed

- Config persistence: Load saved Spotify/Teams tokens and config on app startup
- Reconnect flow: Added `reconnect_spotify` and `reconnect_teams` commands to re-authenticate
- Reconnect now properly clears persisted tokens from store (not just in-memory state)
- `get_sync_status` now validates client_id presence
- Settings UI: Removed non-functional Teams Client ID field
- Spotify redirect_uri normalized to `presencejam://callback` in frontend config
- Emit error handling in reconnect commands (errors are now logged vs silently ignored)

## [2.0.0] - 2026-04-09

### Added

- Tauri 2 + Svelte 5 + TypeScript rewrite (completely new codebase)
- System tray integration (minimize to tray, tray menu with Show/Pause/Quit)
- Deep link support (`presencejam://callback`) for OAuth flows
- Spotify PKCE OAuth authentication
- Microsoft Teams Device Code flow authentication
- Smart polling that sleeps until track ends (no wasted API calls)
- Automatic status clearing when Spotify pauses/stops
- Token refresh handling for both Spotify and Teams
- Configurable status format with `{artist}`, `{track}`, `{album}`, `{emoji}` placeholders
- Polling interval configuration
- Launch-at-login via Windows registry
- Single-instance enforcement (prevents multiple app windows)
- Log viewer in-app (daily rotating logs, 30-day retention)
- Per-window CSP policy

### Changed

- Completely new desktop app architecture (Rust backend + Svelte frontend)
- Status message expiry set to track end time + 10s buffer
- Teams device code auth instead of browser redirect

### Removed

- PowerShell script version — this is a full rewrite

[Unreleased]: https://github.com/Carme99/PresenceJam-Desktop/compare/v2.6.2...HEAD
[2.6.2]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.6.2
[2.6.1]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.6.1
[2.6.0]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.6.0
[2.5.0]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.5.0
[2.3.7]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.3.7
[2.3.6]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.3.6
[2.3.5]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.3.5
[2.3.4]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.3.4
[2.3.3]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.3.3
[2.3.2]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.3.2
[2.3.1]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.3.1
[2.3.0]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.3.0
[2.2.0]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.2.0
[2.1.0]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.1.0
[2.0.0]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.0.0