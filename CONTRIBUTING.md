# Contributing to PresenceJam

Thank you for your interest in contributing!

## Development Setup

### Prerequisites

- **Rust** 1.75+ ([rustup](https://rustup.rs/))
- **Node.js** 18+ ([nodejs.org](https://nodejs.org/))
- **npm** 9+
- **Tauri CLI** v2 (`npm install -g @tauri-apps/cli@^2`)

### Getting Started

```bash
# Clone the repo
git clone https://github.com/Carme99/PresenceJam-Desktop.git
cd PresenceJam-Desktop

# Install dependencies
npm install

# Start development mode
npm run tauri dev
```

### Build Commands

| Command | What it does |
|---------|--------------|
| `npm run tauri dev` | Start dev mode with hot reload |
| `npm run tauri build` | Build release binary |
| `cargo check` | Check Rust compilation |
| `cargo test` | Run Rust unit tests |
| `cargo fmt` | Format Rust code |
| `npm run check` | Type-check Svelte/TypeScript |
| `npm run lint` | Lint Svelte/TypeScript (if configured) |

## Coding Standards

### Commits

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
feat: add dark mode support
fix: correct token refresh logic
docs: update README
refactor: extract auth module
```

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
- No `console.log` in production code

### Screenshots

If your change affects the UI, please include a screenshot in the PR. Screenshots help with review and will eventually be added to the README.

## Project Structure

```
PresenceJam-Desktop/
├── src/                          # Svelte frontend
│   ├── lib/
│   │   ├── components/           # UI components (Dashboard, Onboarding, Settings, LogViewer)
│   │   └── stores/               # Svelte stores (app state, config, spotify)
│   └── routes/                   # SvelteKit routes (+page.svelte)
├── src-tauri/
│   ├── src/
│   │   ├── lib.rs                # Tauri entry, command registration
│   │   ├── commands.rs            # Invoke handlers
│   │   ├── config.rs             # Config loading/saving
│   │   ├── polling.rs            # Polling loop
│   │   ├── spotify.rs            # Spotify API client (PKCE auth, token refresh)
│   │   ├── teams.rs              # Microsoft Teams API client (device code flow)
│   │   └── tray.rs               # System tray setup
│   ├── Cargo.toml
│   └── tauri.conf.json
├── package.json
├── svelte.config.js
├── vite.config.js
├── CONTRIBUTING.md               # You are here
├── CHANGELOG.md
└── README.md
```

## Logging

Logs are stored in `%APPDATA%\PresenceJam\logs\` as daily rotating files (`spotify-teams-YYYY-MM-DD.log`).

To access logs from the app, use the **Log Viewer** view in the app. To access them directly:

```powershell
# Open logs folder in Explorer
Start-Process "$env:APPDATA\PresenceJam\logs"
```

To see verbose output in dev mode, check the terminal where `npm run tauri dev` is running.

## How to Submit Changes

### Bug Fixes

1. Fork the repository
2. Create a branch: `fix/short-description`
3. Make your changes
4. Ensure it compiles: `cargo check && npm run check`
5. Run tests: `cargo test`
6. Commit with a clear message
7. Open a Pull Request

### New Features

1. Fork the repository
2. Create a branch: `feature/short-description`
3. Write your implementation
4. Test in dev mode: `npm run tauri dev`
5. Ensure it compiles: `cargo check && npm run check`
6. Run tests: `cargo test`
7. Commit with a clear message
8. Open a Pull Request

### Documentation

1. Fork the repository
2. Create a branch: `docs/short-description`
3. Make your doc changes
4. Ensure formatting is consistent with existing docs
5. Commit with a clear message
6. Open a Pull Request

## Reporting Issues

Please use the [issue templates](./.github/ISSUE_TEMPLATE/) when reporting bugs or requesting features. Include:

- Clear description of the problem or feature
- Steps to reproduce (for bugs)
- Expected vs actual behavior
- Screenshots if applicable
- Your environment (Windows version, app version)

## AI Tools Welcome

**AI-generated contributions are encouraged.**

If you use AI coding tools (GitHub Copilot, Claude, ChatGPT, etc.) to build features or fix bugs, that's great — just make sure the code:

- Compiles and passes `cargo check` / `npm run check`
- Follows the existing code patterns
- Includes any necessary types
- Has no debug code left in (`console.log`, `println!`, etc.)

You don't need to disclose that you used AI — just submit the best code you can.

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
