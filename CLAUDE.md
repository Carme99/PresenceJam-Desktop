# CLAUDE.md - PresenceJam-Desktop

Desktop app syncing Spotify playback to Microsoft Teams status. Built with Tauri 2, Svelte 5, TypeScript.

---

## Dev Commands

```bash
npm install           # Install dependencies
npm run tauri dev     # Start development mode (hot reload)
npm run tauri build   # Build release binary
cargo check           # Check Rust compilation
cargo test            # Run Rust unit tests
cargo fmt             # Format Rust code
npm run check         # Type-check Svelte/TypeScript
```

---

## Conventions

### Commits

Follow [Conventional Commits](https://www.conventionalcommits.org/):
- `feat: add dark mode support`
- `fix: correct token refresh logic`
- `docs: update README`
- `refactor: extract auth module`

Types: `feat`, `fix`, `docs`, `test`, `refactor`, `chore`

### Rust

- Run `cargo check` before committing
- Use `cargo fmt` to format code before committing
- Error handling with `Result` types — no `unwrap()` in production code
- Use `log::info!` / `log::debug!` over `println!`
- Prefix module-level log tags in square brackets: `[MODULE]`

### Frontend (Svelte + TypeScript)

- Follow existing component patterns
- Use existing stores for state management
- Add TypeScript types for new interfaces
- Use `devLog()` from `$lib/utils/dev` for debug logging — it is a no-op in production builds
- `console.error` and `console.warn` are fine for actual errors that should always be visible

---

## Key Files

| File | Purpose |
|------|---------|
| `src-tauri/src/lib.rs` | Tauri entry, command registration, AppState setup |
| `src-tauri/src/commands.rs` | All invoke() command handlers |
| `src-tauri/src/polling.rs` | Polling loop, token refresh, crash recovery |
| `src-tauri/src/spotify.rs` | Spotify Web API client (PKCE auth) |
| `src-tauri/src/teams.rs` | Microsoft Graph API client (device code flow) |
| `src-tauri/src/profanity.rs` | Profanity filter |
| `src-tauri/src/config.rs` | AppConfig struct, JSON load/save |
| `src-tauri/src/tray.rs` | System tray setup |
| `src/lib/components/` | Svelte components (Dashboard, Onboarding, Settings, LogViewer) |

---

## Auth Flows

- **Spotify:** PKCE OAuth — `code_verifier` generated, `code_challenge` sent to Spotify, browser redirects to `presencejam://callback`
- **Teams:** Device Code flow — app polls `login.microsoftonline.com` every 5s while user completes browser auth

---

## Storage

- Tokens stored via `tauri-plugin-store` (DPAPI on Windows, Keychain on macOS)
- Config stored as plain JSON in `%APPDATA%\PresenceJam\config.json` (Windows) or `~/Library/Application Support/PresenceJam/` (macOS)
- Logs: daily rotating, 30-day retention in `logs/` subdirectory

---

## Status Format Placeholders

`{artist}`, `{track}`, `{album}`, `{emoji}` — default: `🎵 {artist} - {track} 🎧`
