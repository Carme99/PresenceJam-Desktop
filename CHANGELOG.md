# Changelog

All notable changes to PresenceJam are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Fixed

- Memory leak: event listeners not cleaned up on component unmount (Onboarding)
- Polling: 401 responses now trigger token refresh instead of silent failure
- Config: default polling interval now correctly less than max interval (was 30 > 10)
- Config: atomic file writes prevent data loss on save failure
- Auth: CSRF state now properly verified on Spotify OAuth callback
- Shutdown: polling thread now properly stopped on app quit
- UI: buttons no longer allow double-click during async operations
- Error messages now properly display instead of "[object Object]"

### Security

- CSRF protection now functional for Spotify OAuth flow
- Credentials file writes now atomic (prevents corruption)

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

[Unreleased]: https://github.com/Carme99/PresenceJam-Desktop/compare/v2.2.0...HEAD
[2.2.0]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.2.0
[2.1.0]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.1.0
[2.0.0]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.0.0