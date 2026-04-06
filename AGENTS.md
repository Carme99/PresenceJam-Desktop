# PresenceJam-Desktop

> Windows desktop app syncing Spotify now-playing to Microsoft Teams status messages via Graph API.

---

## Tech Stack

- **Framework:** Tauri 2 (Rust backend, WebView2 frontend)
- **Frontend:** Svelte 5 + TypeScript + Vite
- **Backend:** Rust (async polling, Spotify PKCE OAuth, Teams Device Code OAuth)
- **Target:** Windows only (portable exe, no UAC required)

---

## Key Packages / Plugins

| Plugin | Purpose |
|--------|---------|
| `tauri-plugin-notification` | Windows toast notifications |
| `tauri-plugin-store` | Persistent token/state storage |
| `tauri-plugin-autostart` | Windows startup registration |
| `tauri-plugin-log` | File-based logging |
| `tauri-plugin-shell` | Open URLs in default browser |
| `tauri-plugin-http` | HTTP requests (Spotify/Graph API) |
| `tauri-plugin-process` | Process exit |
| `tauri-plugin-opener` | Open URLs |

---

## Build Commands

```bash
# Development
npm run tauri dev          # Dev mode with hot reload
npm run tauri build         # Build release binary

# Frontend only
npm run dev                 # Svelte dev server (port 5173)
npm run build               # Svelte static build

# Linting
npm run lint                # ESLint (frontend)
cargo check                 # Rust compiler check
```

---

## Project Structure

```
presence-jam/
├── src/                          # Svelte 5 frontend
│   ├── lib/
│   │   ├── components/
│   │   │   ├── Onboarding.svelte    # 3-step setup wizard
│   │   │   ├── Dashboard.svelte     # Track display + controls
│   │   │   ├── Settings.svelte      # Config panel
│   │   │   └── LogViewer.svelte     # Real-time log viewer
│   │   └── stores/
│   │       ├── config.ts            # AppConfig types + load/save
│   │       ├── spotify.ts          # Spotify tokens + track info
│   │       ├── teams.ts            # Teams tokens
│   │       └── app.ts              # View state + sync state
│   └── routes/
│       └── +page.svelte            # Main router
├── src-tauri/
│   ├── src/
│   │   ├── lib.rs               # App builder + state
│   │   ├── main.rs              # Entry point
│   │   ├── config.rs            # Config types + load/save (%APPDATA%)
│   │   ├── spotify.rs           # PKCE OAuth + Spotify Web API
│   │   ├── teams.rs             # Device code + Graph API (setStatusMessage)
│   │   ├── polling.rs           # Async polling loop
│   │   ├── tray.rs              # System tray icon + menu
│   │   └── commands.rs         # 24 IPC invoke commands
│   ├── Cargo.toml
│   └── tauri.conf.json
├── SPEC.md                          # Technical specification
├── README.md                        # User documentation
└── package.json
```

---

## Key API Reference

```rust
// teams.rs - sets Teams status MESSAGE (not availability dot)
pub fn set_teams_status_message(access_token: &str, message: &str, expiry_datetime: Option<&str>) -> Result<(), String>
// POST https://graph.microsoft.com/v1.0/me/presence/setStatusMessage
// Body: { statusMessage: { message: { content: "...", contentType: "text" }, expiryDateTime: { dateTime: "...", timeZone: "UTC" } } }
```

---

## Commit Style

- **Commits:** `type: description` (feat, fix, docs, test, refactor, chore)
- **Branches:** `feature/name`, `fix/description`, `docs/changes`
- **No direct pushes to main** — PRs for all changes

---

## Notes

- All data stored in user-space (`%APPDATA%/PresenceJam`)
- No UAC required — portable distribution
- Spotify auth uses PKCE + manual URL paste fallback
- Teams auth uses device code flow
- Original PowerShell script preserved in separate `PresenceJam/` repo
