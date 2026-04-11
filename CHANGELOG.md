# Changelog

All notable changes to PresenceJam are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

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

[Unreleased]: https://github.com/Carme99/PresenceJam-Desktop/compare/v2.1.0...HEAD
[2.1.0]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.1.0
[2.0.0]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.0.0