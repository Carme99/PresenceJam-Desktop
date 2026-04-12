# PresenceJam 2.3.4

[![GitHub release (latest by date)](https://img.shields.io/github/v/release/Carme99/PresenceJam-Desktop?style=flat-square)](https://github.com/Carme99/PresenceJam-Desktop/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=flat-square)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/Rust-000?logo=rust&style=flat-square)](https://www.rust-lang.org/)
[![Tauri](https://img.shields.io/badge/Tauri-2.0-ffc107?logo=tauri&style=flat-square)](https://tauri.app/)
[![Svelte](https://img.shields.io/badge/Svelte-5-ff3e00?logo=svelte&style=flat-square)](https://svelte.dev/)
[![TypeScript](https://img.shields.io/badge/TypeScript-5.6-3178c6?logo=typescript&style=flat-square)](https://www.typescriptlang.org/)
[![macOS](https://img.shields.io/badge/macOS-Apple_Silicon-333333?logo=apple&style=flat-square)](https://github.com/Carme99/PresenceJam-Desktop/releases)

**Sync your Spotify playback to Microsoft Teams status automatically.**

*A solo-dev, vibe-coded desktop app for Windows and macOS.*

## Why?

I had a weekend, some caffeine, and a vague memory of MSN Messenger's
"Now Playing" feature. This is the result. Fully vibe-coded.

## Features

| Feature | Description |
|---------|-------------|
| 🎵 **Spotify Polling** | Real-time track detection via Spotify Web API |
| 📝 **Teams Status** | Sets your Teams custom status message with track info |
| ⏱️ **Smart Polling** | Sleeps until track ends + buffer — no wasted API calls |
| 🗑️ **Auto-Clear** | Automatically clears status when Spotify pauses/stops |
| 🔐 **Secure Auth** | PKCE OAuth for Spotify, Device Code flow for Teams |
| ⚙️ **Configurable** | Custom status format, emoji, polling interval |
| 🖥️ **System Tray** | Runs silently in the background |
| 🚀 **Launch at Login** | Optional auto-start on Windows boot |
| 🛡️ **Profanity Filter** | Auto-detects and replaces profane track names with safe placeholder |

## How It Works

```
Spotify Web API  →  PresenceJam (polling)  →  Microsoft Graph API  →  Teams Status
     ↓                    ↓                        ↓
  "Artist -      Format with           "🎵 Artist -
   Track"          template            Track 🎧"
```

The app polls Spotify every few seconds while a track is playing. When the track changes, it formats a message using your custom template and pushes it to Teams via Microsoft Graph.

## Screenshots
<img width="601" height="414" alt="image" src="https://github.com/user-attachments/assets/dc8317bb-b326-4bc0-ba89-4a28b4d52abf" />
<img width="597" height="686" alt="image" src="https://github.com/user-attachments/assets/404527a5-7d08-487d-abc7-2f39aa25d439" />

> **Want to add screenshots?** PRs welcome! See [CONTRIBUTING.md](./CONTRIBUTING.md#screenshots).

| View | Description |
|------|-------------|
| Dashboard | Shows currently playing track, Teams connection status, and sync controls |
| Onboarding | 3-step wizard: Spotify credentials → Microsoft auth → Customize settings |
| Settings | Adjust status format, polling interval, profanity filter, launch-at-login |
| Log Viewer | Scroll through the daily rotating log files |

## Downloads

Download the latest release from [GitHub Releases](https://github.com/Carme99/PresenceJam-Desktop/releases):

- `PresenceJam-2.3.0.msi` — Windows 10/11 installer (64-bit)
- `PresenceJam-2.3.0-macos.dmg` — macOS installer (Apple Silicon)

## Quickstart

```bash
# 1. Install dependencies
npm install

# 2. Start development mode
npm run tauri dev

# 3. Build for release
npm run tauri build
```

For full setup instructions (Spotify app registration, Teams auth flow), see [CONTRIBUTING.md](./CONTRIBUTING.md).

## Status Format

Customize how your Teams status looks using these placeholders:

| Placeholder | Output |
|-------------|--------|
| `{artist}` | Artist name |
| `{track}` | Track name |
| `{album}` | Album name |
| `{emoji}` | 🎵 (playing) or ⏸️ (paused) |

**Default:** `🎵 {artist} - {track} 🎧`

**Example output:** `🎵 Daft Punk - One More Time 🎧`

## Profanity Filter

If a track or artist name contains profanity, PresenceJam replaces the entire status with a safe placeholder rather than displaying the profane content.

| Setting | Default | Description |
|---------|---------|-------------|
| `profanity_filter` | `true` | Enable/disable the filter |
| `profanity_placeholder` | `Currently Listening to Spotify` | Placeholder text shown when content is filtered. Supports `{emoji}` (🎵 playing / ⏸️ paused) |

**How detection works:**
- 25-word curated profanity list
- Leetspeak normalization: `1→i`, `3→e`, `$→s`, `@→a`, `0→o`, `5→s`, `7→t`, `!→i`, `|→i`
- Repeated-character collapse: `shiiit` → `shiit` (but not excessive repeats)
- Word-boundary detection avoids false positives (e.g., `class`, `cocktail`, `assassin`)
- "fucking", "fucked", "fucker" variants are detected as profanity
- Safe suffix words allow compounds like `cocktail` without blocking `cock`

**Note:** The filter currently operates on the formatted status string. A future refactor may filter raw Spotify metadata before formatting to prevent placeholder injection via custom templates. See [ARCHITECTURE.md](./ARCHITECTURE.md) for details.

## Data & Privacy

| What | Where | How |
|------|-------|-----|
| Spotify tokens | `tauri-plugin-store` | Stored locally in `credentials.json` with atomic writes |
| Teams tokens | `tauri-plugin-store` | Stored locally in `credentials.json` with atomic writes |
| App config | `%APPDATA%\PresenceJam\config.json` | Plain JSON |
| Credentials | `%APPDATA%\PresenceJam\credentials.json` | Plain JSON |
| App logs | `%APPDATA%\PresenceJam\logs\` | Daily rotating, 30-day retention |

- **No telemetry.** Nothing is sent to any server except Spotify and Microsoft directly.
- Tokens are stored locally in your user profile — they never leave your machine except to authenticate with Spotify and Microsoft Graph APIs.

## Troubleshooting

Having issues? Check the [TROUBLESHOOTING](./TROUBLESHOOTING.md) guide for common problems and solutions.

### App closes on X button

The app minimizes to the system tray — this is by design. Right-click the tray icon → **Quit** to fully exit.

### Spotify authorization loop

1. Make sure your Spotify app's Redirect URIs include `presencejam://callback`
2. If auto-detection fails, paste the full redirect URL from your browser manually

### Teams not updating

1. Check that you're signed into the same Microsoft account in Teams and in the app
2. Try disconnecting and reconnecting via Settings
3. Check the log viewer in-app for API errors

## Contributing

Contributions welcome! See [CONTRIBUTING.md](./CONTRIBUTING.md) for development setup, coding standards, and submission guidelines.

**AI-generated contributions are encouraged.** Use whatever tools help you build the best code — just make sure it compiles and follows the project patterns.

## Architecture

Curious how it all works? See [ARCHITECTURE.md](./ARCHITECTURE.md) for deep-dive diagrams and explanation.

## Changelog

See [CHANGELOG.md](./CHANGELOG.md) for version history.

## Acknowledgements

See [ACKNOWLEDGEMENTS.md](./ACKNOWLEDGEMENTS.md) for the open-source dependencies that make this project possible.

## License

MIT License — see [LICENSE](./LICENSE) for details.
