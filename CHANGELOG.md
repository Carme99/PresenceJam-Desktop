# Changelog

All notable changes to PresenceJam are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

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

[Unreleased]: https://github.com/Carme99/PresenceJam-Desktop/compare/v2.5.0...HEAD
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