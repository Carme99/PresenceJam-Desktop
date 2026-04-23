# CLAUDE.md - PresenceJam-Desktop

> Desktop app syncing Spotify playback to Microsoft Teams status. Built with Tauri 2, Svelte 5, TypeScript.

---

## Tech Stack

- **Backend:** Tauri 2 (Rust)
- **Frontend:** Svelte 5, TypeScript 5.6
- **Build Tool:** Vite, Tauri CLI
- **Package Manager:** npm (frontend), Cargo (Rust backend)

---

## Development Commands

```bash
# Install dependencies
npm install

# Start development mode
npm run tauri dev

# Build for release
npm run tauri build
```

---

## Project Structure

```
PresenceJam-Desktop/
├── src/               # Svelte frontend
├── src-tauri/         # Rust backend
│   ├── Cargo.toml     # Rust dependencies
│   └── tauri.conf.json
├── static/            # Static assets
├── build/              # Build outputs
└── .svelte-kit/       # SvelteKit cache
```

---

## Key Features

- System tray integration
- Auto-start on login
- PKCE OAuth for Spotify
- Device Code flow for Teams
- Profanity filter
- Smart polling (sleeps until track ends + buffer)
- Local token storage via tauri-plugin-store

---

## Status Format Placeholders

| Placeholder | Output |
|-------------|--------|
| `{artist}` | Artist name |
| `{track}` | Track name |
| `{album}` | Album name |
| `{emoji}` | 🎵 (playing) or ⏸️ (paused) |

**Default:** `🎵 {artist} - {track} 🎧`
