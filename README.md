# PresenceJam 2.6.0

Sync your Spotify playback to Microsoft Teams status automatically.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=flat-square)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/Rust-000?logo=rust&style=flat-square)](https://www.rust-lang.org/)
[![Tauri](https://img.shields.io/badge/Tauri-2.0-ffc107?logo=tauri&style=flat-square)](https://tauri.app/)
[![Svelte](https://img.shields.io/badge/Svelte-5-ff3e00?logo=svelte&style=flat-square)](https://svelte.dev/)
[![TypeScript](https://img.shields.io/badge/TypeScript-5.6-3178c6?logo=typescript&style=flat-square)](https://www.typescriptlang.org/)

## What it does

PresenceJam polls Spotify's Web API for your currently playing track and sets it as your Microsoft Teams custom status message. When the track changes, your status updates automatically. When you pause or stop, it clears your status (if you've enabled that option).

## Features

| Feature | Description |
|---------|-------------|
| Real-time Spotify detection | Polls Spotify's Web API while a track is playing |
| Teams status sync | Sets your Teams custom status via Microsoft Graph |
| Smart polling | Sleeps until the track ends — no wasted API calls |
| Auto-clear | Clears status when Spotify pauses/stops |
| Profanity filter | Replaces profane track names with a safe placeholder |
| System tray | Runs silently in the background |
| Launch at login | Optional auto-start on boot |
| Secure auth | PKCE OAuth for Spotify, Device Code flow for Teams |

## Downloads

Install the latest release from [GitHub Releases](https://github.com/Carme99/PresenceJam-Desktop/releases):

- `PresenceJam-2.7.1.msi` — Windows 10/11 (64-bit)
- `PresenceJam-2.7.1-macos.dmg` — macOS (Apple Silicon)
- `PresenceJam-2.7.1-linux-amd64.deb` — Debian / Ubuntu / Mint / popOS (64-bit)
- `PresenceJam-2.7.1-linux-amd64.AppImage` — any modern Linux (64-bit, no install)

### Linux install

**Debian / Ubuntu / Mint / popOS** (one-time):

```bash
sudo apt install ./PresenceJam-2.7.1-linux-amd64.deb
# or if apt refuses the local path:
sudo dpkg -i PresenceJam-2.7.1-linux-amd64.deb && sudo apt-get install -f
```

**AppImage** (any distro, no install required):

```bash
chmod +x PresenceJam-2.7.1-linux-amd64.AppImage
./PresenceJam-2.7.1-linux-amd64.AppImage
```

For autostart with an AppImage, see the [Tauri Linux docs](https://tauri.app/distribute/linux/) — a `.desktop` file in `~/.local/share/applications/` plus the binary in `~/.local/bin/` is the standard pattern.

### macOS first-run note

The macOS DMG is currently **unsigned** (Apple Developer Program enrollment is not in scope — see issue #90). On first open, macOS shows "unidentified developer". To open:

- Right-click the app → **Open** (confirms once), or
- System Settings → Privacy & Security → **Open Anyway**

Subsequent opens work without the prompt.

## Quickstart

**First time?** Follow the [Setup Guide](./SETUP.md) — it covers installing the app, registering a Spotify Developer app, and connecting Teams.

**Already set up?** Just run:

```bash
# Install dependencies
npm install

# Start development mode
npm run tauri dev

# Build for release
npm run tauri build
```

## Documentation

| Doc | What it's for |
|-----|---------------|
| [Setup](./SETUP.md) | Installing the app, Spotify app registration, Teams auth |
| [Usage](./USAGE.md) | Day-to-day guide — tray, dashboard, settings |
| [Architecture](./ARCHITECTURE.md) | How it works under the hood |
| [Troubleshooting](./TROUBLESHOOTING.md) | Common problems and fixes |
| [Changelog](./CHANGELOG.md) | Version history |
| [Security](./SECURITY.md) | Token storage, privacy, network |
| [Contributing](./CONTRIBUTING.md) | Dev setup, coding standards, PR process |
| [Acknowledgements](./ACKNOWLEDGEMENTS.md) | Open-source dependencies |

## Architecture

The app is built with:

- **Backend:** Tauri 2 (Rust) — polling thread, API clients, token storage, system tray
- **Frontend:** Svelte 5 + TypeScript — SPA rendered via `@sveltejs/adapter-static`
- **Storage:** `tauri-plugin-store` for tokens (DPAPI on Windows, Keychain on macOS), JSON for config
- **Auth:** Spotify PKCE OAuth + Microsoft Teams Device Code flow

See [ARCHITECTURE.md](./ARCHITECTURE.md) for deep-dive diagrams and explanation.

## Status Format

Customize your Teams status using placeholders:

| Placeholder | Output |
|-------------|--------|
| `{artist}` | Artist name |
| `{track}` | Track name |
| `{album}` | Album name |
| `{emoji}` | 🎵 (playing) or ⏸️ (paused) |

**Default:** `🎵 {artist} - {track} 🎧`  
**Example:** `🎵 Daft Punk - One More Time 🎧`

## License

MIT — see [LICENSE](./LICENSE) for details.
