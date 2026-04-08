# PresenceJam 2.0

[![GitHub release (latest by date)](https://img.shields.io/github/v/release/Carme99/PresenceJam-Desktop?style=flat-square)](https://github.com/Carme99/PresenceJam-Desktop/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=flat-square)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/Rust-000?logo=rust&style=flat-square)](https://www.rust-lang.org/)
[![Tauri](https://img.shields.io/badge/Tauri-2.0-ffc107?logo=tauri&style=flat-square)](https://tauri.app/)
[![Svelte](https://img.shields.io/badge/Svelte-5-ff3e00?logo=svelte&style=flat-square)](https://svelte.dev/)
[![TypeScript](https://img.shields.io/badge/TypeScript-5.6-3178c6?logo=typescript&style=flat-square)](https://www.typescriptlang.org/)

**Sync your Spotify playback to Microsoft Teams status automatically.**

*A solo-dev, vibe-coded Windows app built for fun.*

## Why?

I had a weekend, some caffeine, and a vague memory of MSN Messenger's 
"Now Playing" feature. This is the result. Fully vibe-coded.

## What It Does

🎵 **Reads your Spotify** — polls the Web API to see what's playing  
📝 **Sets your Teams status** — updates your message with the track info  
⚙️ **Configurable** — pick your own format, interval, and emoji  
🔔 **Notifications** — get a toast when the track changes *(planned)*

## Screenshots

[PLACEHOLDER: Add screenshot of the app UI here]

[PLACEHOLDER: Add screenshot of Teams status with Spotify track here]

## Downloads

Download the latest release from GitHub:
- `PresenceJam_2.0.0_x64_en-US.msi` — Windows installer

## Setup

### Prerequisites

- Windows 10/11
- Spotify Premium account
- Microsoft 365 account with Teams

### Step 1: Register Your Spotify App

1. Go to [Spotify Developer Dashboard](https://developer.spotify.com/dashboard)
2. Create a new app
3. Add `presencejam://callback` to Redirect URIs
4. Copy your Client ID and Client Secret

### Step 2: Connect to Spotify

1. Run PresenceJam
2. Enter your Spotify Client ID + Client Secret
3. Click "Connect Spotify" — authorization opens in your browser
4. If the app doesn't detect authorization automatically, paste the redirect URL from your browser manually

### Step 3: Connect Microsoft Teams

1. Click "Sign in with Microsoft"
2. A code will be displayed — note it down
3. Visit the verification URL shown and enter the code
4. Click "I've completed sign-in"

### Step 4: Customize & Finish

1. Adjust your status format using `{artist}`, `{track}`, `{album}`, `{emoji}`
2. Configure polling interval
3. Click "Finish"

## Status Format

Use these placeholders in your status format:

- `{artist}` — Artist name
- `{track}` — Track name
- `{album}` — Album name
- `{emoji}` — Auto-selected (🎵 playing, ⏸️ paused)

Default: `🎵 {artist} - {track} 🎧`

## Troubleshooting

### "Spotify not connected"

- Verify your Spotify app redirect URI includes `presencejam://callback`
- Try pasting the redirect URL from your browser manually

### "Teams not updating"

- Sign out and sign back in via the app
- Check logs in the app (Logs view)

### App closes on X button

- The app minimizes to system tray, not exit
- Right-click tray icon → Quit to exit

## Data & Privacy

- All data stored locally (no cloud)
- Tokens stored in your user profile
- Nothing is sent to third-party servers except Spotify and Microsoft APIs

## Contributing

Contributions welcome! See [CONTRIBUTING.md](./CONTRIBUTING.md) for development setup and guidelines.

## Acknowledgements

See [ACKNOWLEDGEMENTS.md](./ACKNOWLEDGEMENTS.md) for open-source dependencies.

## License

MIT License — see [LICENSE](./LICENSE) for details.
