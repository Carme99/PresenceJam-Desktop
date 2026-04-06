# PresenceJam 2.0 Specification

## Overview
PresenceJam 2.0 is a Windows desktop application that automatically syncs your currently playing Spotify track to your Microsoft Teams status message. Built with Tauri 2 + Svelte 5 as a rewrite of the original PowerShell script.

## Architecture

### Technology Stack
- **Frontend**: Svelte 5 (SPA mode)
- **Backend**: Rust (Tauri 2)
- **Distribution**: Portable executable (no installer, no UAC required)

### Multi-Process Architecture
- **WebView Process**: Renders Svelte UI via system WebView2 (Windows)
- **Core Process**: Rust backend handles all native operations

### Key Modules

#### Rust Backend (`src-tauri/src/`)
| Module | Responsibility |
|--------|---------------|
| `config.rs` | Load/save config.json from %APPDATA%\PresenceJam\ |
| `spotify.rs` | PKCE OAuth, token management, track polling |
| `teams.rs` | Device code OAuth, Microsoft Graph presence API |
| `polling.rs` | Async polling loop, event emission |
| `tray.rs` | System tray icon and menu |
| `commands.rs` | IPC command handlers (invoke API) |

#### Svelte Frontend (`src/`)
| Component | Purpose |
|-----------|---------|
| `Onboarding.svelte` | 3-step setup wizard |
| `Dashboard.svelte` | Track display, status preview, sync controls |
| `Settings.svelte` | Configuration panel |
| `LogViewer.svelte` | Real-time log viewer |

## Configuration

### Config File: `%APPDATA%\PresenceJam\config.json`
```json
{
  "spotify": {
    "client_id": "string",
    "client_secret": "string",
    "redirect_uri": "http://localhost:7890/callback",
    "scopes": ["user-read-currently-playing", "user-read-playback-state"]
  },
  "teams": {
    "status_format": "🎵 {artist} - {track} 🎧",
    "clear_on_pause": true
  },
  "polling": {
    "default_interval_seconds": 30,
    "minimum_interval_seconds": 5,
    "max_interval_seconds": 10,
    "expiry_buffer_seconds": 10
  },
  "logging": {
    "enabled": true,
    "log_level": "Info",
    "retention_days": 30
  }
}
```

### Token Storage
- Spotify and Teams tokens stored via `tauri-plugin-store`
- Stored in: `%APPDATA%\PresenceJam\store.json`

## OAuth Flows

### Spotify PKCE
1. Generate code_verifier (64 random bytes, base64url)
2. Generate code_challenge (SHA256 of verifier, base64url)
3. Open browser to Spotify authorize URL
4. Start local HTTP server on port 7890
5. On callback: exchange code+verifier for tokens
6. Store tokens securely

### Teams Device Code
1. POST to devicecode endpoint → get user_code + verification_url
2. Show user code, open verification URL
3. Poll token endpoint until user completes auth
4. Store tokens

## Presence API
- Endpoint: `POST https://graph.microsoft.com/v1.0/me/presence/setPresence`
- Authentication: Bearer token (device code flow)
- Sets availability to "Available" when playing, "Offline" when paused

## Data Storage (User-Space Only)
| Data | Location |
|------|----------|
| Config | %APPDATA%\PresenceJam\config.json |
| Tokens | %APPDATA%\PresenceJam\store.json |
| Logs | %LOCALAPPDATA%\PresenceJam\logs\ |

## Tauri Plugins
- `tauri-plugin-notification` — Windows toast notifications
- `tauri-plugin-store` — Secure token storage
- `tauri-plugin-autostart` — Launch at login (HKCU)
- `tauri-plugin-log` — File logging
- `tauri-plugin-shell` — Open browser for OAuth
- `tauri-plugin-http` — Spotify/Graph API calls
- `tauri-plugin-process` — Quit/restart

## Build
```
npm run tauri build
# Output: src-tauri/target/release/bundle/
```

## Auto-Start
- Uses HKCU\Software\Microsoft\Windows\CurrentVersion\Run (user-level, no UAC)
- Enabled/disabled via `tauri-plugin-autostart`
