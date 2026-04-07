# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased] - 2026-04-07

### Status: ⚠️ Teams Auth Blocked - Network Issue

**Issue:** Teams device code OAuth fails with "error decoding response body"

**Workaround:** See [INVESTIGATION_NOTES.md](./INVESTIGATION_NOTES.md) for details and troubleshooting steps.

---

## [2.0.0] - 2026-04-07

### Added

#### Teams Device Code OAuth (Investigation in Progress)
- Added `start_teams_auth_device_code` command for Microsoft device code flow
- Added `poll_teams_auth` command for polling auth completion
- Added `open_external_url` command for opening URLs in default browser
- Added `MICROSOFT_GRAPH_CLIENT_ID` constant (`14d82eec-204b-4c2f-b7e8-296a70dab67e`)
- Added PKCE helper functions (`pkce_generate_verifier`, `pkce_generate_challenge`) for future auth code flow support
- Added comprehensive logging to Teams auth commands

#### Deep Link Support
- Configured `presencejam://` as custom protocol scheme
- Added deep link handling for Spotify (`/callback`) and Teams (`/teams-callback`) callbacks
- MSI installer properly registers protocol handler in Windows registry

#### Security Updates
- Added `microsoft.com` and `*.microsoft.com` to CSP connect-src
- Updated Content Security Policy to allow required Microsoft domains

#### Test/Debug Infrastructure
- Added TEST CLICK button to verify click handling
- Added TEST INVOKE button to test invoke commands
- Added detailed console logging throughout Teams auth flow

### Changed

#### Bundle Configuration
- Changed from NSIS to MSI installer only (NSIS has known deep link registration bug)
- Deep links now work correctly when installed via MSI

#### Permissions
- Updated `capabilities/default.json` with additional shell and opener permissions
- Shell plugin and opener plugin now properly configured

#### Teams Auth Flow
- Migrated from auth code + PKCE flow to device code flow (to match original PowerShell behavior)
- Teams auth no longer requires Azure AD app registration
- Uses Microsoft's well-known Graph PowerShell client ID

### Fixed

#### Event Handler Issues
- Changed Svelte 5 onclick handlers to use arrow function wrapper
- `onclick={() => asyncFunction()}` instead of `onclick={asyncFunction}`

#### Frontend State Management
- Fixed state initialization for Teams auth variables
- Proper handling of `teamsUserCode`, `teamsDeviceCode`, `teamsConnected` states

### Known Issues

- ⚠️ Teams device code flow fails with "error decoding response body" - likely corporate proxy issue
- ⚠️ Dev mode (`npm run tauri dev`) does not register `presencejam://` protocol - use MSI for testing deep links

---

## [1.0.0] - Previous Versions

*No detailed changelog available for earlier versions.*

---

## Migration Notes

### Upgrading from 1.x to 2.0

1. **Spotify Auth:**
   - Requires Spotify Developer App (client ID and secret)
   - Deep link callback: `presencejam://callback`
   - No Azure AD app required

2. **Teams Auth:**
   - No longer requires Azure AD app registration
   - Uses device code flow (like original PowerShell script)
   - User visits `microsoft.com/devicelogin` with short code

3. **Installation:**
   - MSI installer now properly registers protocol handler
   - NSIS installer removed due to deep link bug

---

## Dependencies

### Rust Crates
- `tauri` v2 - Desktop application framework
- `tauri-plugin-opener` v2 - URL opening
- `tauri-plugin-deep-link` v2 - Protocol handling
- `tauri-plugin-notification` v2 - System notifications
- `tauri-plugin-store` v2 - Persistent storage
- `tauri-plugin-autostart` v2 - Windows startup
- `tauri-plugin-log` v2 - Logging
- `tauri-plugin-shell` v2 - Shell operations
- `tauri-plugin-http` v2 - HTTP requests
- `tauri-plugin-process` v2 - Process management
- `reqwest` v0.12 - HTTP client (for Spotify and Teams APIs)
- `chrono` v0.4 - Date/time handling
- `sha2` v0.10 - SHA256 hashing (for PKCE)
- `base64` v0.22 - Base64 encoding (for PKCE)
- `rand` v0.8 - Random number generation (for PKCE)
- `url` v2 - URL parsing
- `urlencoding` v2 - URL encoding

### Frontend
- Svelte 5 - UI framework
- TypeScript - Type safety
- Vite - Build tool
- Tauri API - Rust bridge

---

## Links

- **Repository:** https://github.com/Carme99/PresenceJam-Desktop
- **Documentation:** See README.md
- **Investigation Notes:** See INVESTIGATION_NOTES.md
