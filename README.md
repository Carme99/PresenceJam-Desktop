# PresenceJam 2.0

[![GitHub release (latest by date)](https://img.shields.io/github/v/release/Carme99/PresenceJam-Desktop?style=flat-square)](https://github.com/Carme99/PresenceJam-Desktop/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=flat-square)](https://opensource.org/licenses/MIT)

**Sync your Spotify playback to Microsoft Teams status automatically.**

## Screenshots

[PLACEHOLDER: Add screenshot of the app UI here]

[PLACEHOLDER: Add screenshot of Teams status with Spotify track here]

## Downloads

Download the latest release from GitHub:
- `PresenceJam_2.0.0_x64-setup.exe` — Installer (requires admin)
- `PresenceJam_2.0.0_x64_portable.zip` — Portable (no install, no UAC)

## Features

- Automatically updates Teams status with current Spotify track
- Secure token storage (DPAPI on Windows)
- Optional Windows notifications on track change
- Customizable status format with emoji
- Smart polling (faster near track end)
- Launch at login
- Real-time log viewer

## Setup

### Prerequisites

- Windows 10/11
- Spotify Premium account
- Microsoft 365 account with Teams

### Step 1: Register Spotify App

1. Go to https://developer.spotify.com/dashboard
2. Create a new app
3. Add `http://localhost:7890/callback` to Redirect URIs
4. Copy Client ID and Client Secret

### Step 2: Register Azure AD App (Teams)

1. Go to https://portal.azure.com → Azure Active Directory → App registrations
2. Register a new app
3. Add `Native` platform (no redirect URI needed)
4. Enable: API permissions → Microsoft Graph → Presence.ReadWrite
5. Copy Application (client) ID

### Step 3: Configure PresenceJam

1. Run PresenceJam
2. Enter your Spotify Client ID + Secret
3. Click "Connect Spotify" → authorize in browser
4. Enter your Azure AD Client ID
5. Click "Sign in with Microsoft" → enter the user code
6. Customize your status format
7. Click "Finish"

## Status Format

Use these placeholders in your status format:

- `{artist}` — Artist name
- `{track}` — Track name
- `{album}` — Album name
- `{emoji}` — Auto-selected (🎵 playing, ⏸️ paused)

Default: `🎵 {artist} - {track} 🎧`

## Troubleshooting

### "Spotify not connected"

- Verify your Spotify app redirect URI includes `http://localhost:7890/callback`
- Try pasting the redirect URL manually

### "Teams not updating"

- Ensure the Azure AD app has `Presence.ReadWrite` permission
- Check logs in the app (Logs view)

### App closes on X button

- The app minimizes to system tray, not exit
- Right-click tray icon → Quit to exit

## Data & Privacy

- All data stored locally (no cloud)
- Tokens encrypted via Windows DPAPI
- Nothing is sent to third-party servers except Spotify and Microsoft APIs

## Contributing

Contributions welcome! See [CONTRIBUTING.md](./CONTRIBUTING.md) for development setup and guidelines.

## Acknowledgements

See [ACKNOWLEDGEMENTS.md](./ACKNOWLEDGEMENTS.md) for open-source dependencies.

## License

MIT License — see [LICENSE](./LICENSE) for details.
