# Changelog

All notable changes to PresenceJam are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [2.3.7] - 2026-04-27

### Fixed

- Spotify auth: crash recovery now properly restores pending OAuth state from persistent store on app restart (fixes #1)
- Teams auth: proactively refresh Teams token before use in polling loop to avoid 401 errors mid-session (fixes #4)
- Config: `start_minimized` now properly wired — app window hides on startup when configured (fixes #7)
- Config: `clear_on_pause` now functional — respects user setting when pausing Spotify playback (fixes #6)
- Onboarding: Spotify client ID/secret now validated before initiating auth (min 20 chars) (fixes #19)

### Developer

- Frontend: replaced ~130 verbose `console.log` calls with `devLog()` utility that only outputs in development builds — production builds are no longer polluted with dev traces

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

[Unreleased]: https://github.com/Carme99/PresenceJam-Desktop/compare/v2.3.7...HEAD
[2.3.7]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.3.7
[2.3.4]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.3.4
[2.3.3]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.3.3
[2.3.2]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.3.2
[2.3.1]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.3.1
[2.3.0]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.3.0
[2.2.0]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.2.0
[2.1.0]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.1.0
[2.0.0]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.0.0