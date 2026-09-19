# Contributing to PresenceJam

Thank you for your interest in contributing!

## Development Setup

### Prerequisites

- **Rust** 1.96 ([rustup](https://rustup.rs/)) — the **declared MSRV**: `src-tauri/Cargo.toml` sets `rust-version = "1.96"`, and the root `rust-toolchain.toml` pins `channel = "1.96"` with the `rustfmt` + `clippy` components, so `cargo fmt` / `cargo check` / `cargo clippy` in this repository cannot silently run on a different toolchain. CI keeps its SHA-pinned `dtolnay/rust-toolchain` action (`ci.yml`).
- **Node.js** 24 ([nodejs.org](https://nodejs.org/)) — what every CI job installs (`node-version: 24` in `ci.yml`); `package.json` requires `">=22"`.
- **npm** 9+
- **Tauri CLI** v2 — provided by repo-pinned local `@tauri-apps/cli`; use `npm run tauri ...`. A global install is unnecessary.

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
| `npm test` | Run frontend unit tests (vitest, `tests/*.test.ts`); run it for any change under `src/`. CI's `frontend` job runs `npm run test:coverage` instead — the same suite through the v8 provider, which fails the job when any of the four measured percentages in `vitest.config.js` drops (the ratchet). |

`cargo test` also regenerates `src/lib/types-generated/` — ts-rs output, gitignored, and produced by the `#[ts(export)]` derives at test time. Run the Rust tests **before** `npm run check` whenever an exported struct changed, or the frontend type-check reads stale generated types (CI does `cargo test --lib` first for the same reason).

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
- Run `cargo fmt --check` (CI `rust` job gates on it) and `cargo clippy --all-targets -- -D warnings` (CI `rust-clippy` job) before committing
- Error handling with `Result` types — no `unwrap()` on fallible I/O or parse paths in production code; the sole exception is the `tray.rs` `cached_devices` cache-hit fast path, which unwraps a snapshot it just proved is `Some`
- Use `log::info!` / `log::debug!` over `println!`
- Prefix module-level log tags in square brackets: `[MODULE]`
- User-Agent and any version-stamped payload must use `env!("CARGO_PKG_VERSION")` — never hardcode the version. `Cargo.toml` is the single source of truth (mirrored into `tauri.conf.json` → `version`).

### Frontend (Svelte + TypeScript)

- Follow existing component patterns
- Use existing stores for state management
- Add TypeScript types for new interfaces
- Use `devLog()` from `$lib/utils/dev` for debug logging — it is a no-op in production builds
- `console.error` and `console.warn` are fine for actual errors that should always be visible
- **All user-facing UI strings must go through `t()` from `$lib/i18n`** — never hardcode English text in components. Add the key to all three dictionaries (`src/lib/i18n/en.ts`, `de.ts`, `fr.ts`); the shared `Dict` type makes a missing translation a compile error. Interpolation uses `{name}` placeholders. Known limitation: Rust-side error strings surfaced through `invoke()` rejections and event payloads stay English.

### Screenshots

If your change affects the UI, please include a screenshot in the PR. Screenshots help reviewers verify the change and are added to the README as needed. Captures that belong in the repo live in [`docs/screenshots/`](./docs/screenshots) — never drop a pasted image at the repository root (`.gitignore` rejects `/image_0*.*`, but the review is the real gate).

## Project Structure

See [the architecture frontend page](./docs/architecture/frontend.md#directory-structure) for the full directory tree.

## Logging

Logs are written by the logging plugin to `PresenceJam.log` in `%LOCALAPPDATA%\com.presencejam.app\logs\` (Windows; see USAGE.md for macOS/Linux paths) — Tauri's `app_log_dir()` appends the bundle identifier to the platform's local data directory, so this is not the same folder as `config.json` (issue #300). The file rotates on size and keeps a bounded number of archives: `logging.max_file_size_mb` (1–500 MB, default 10) and `logging.keep_files` (1–20, default 3) feed `lib.rs::log_rotation_strategy`, and the live log is kept **in addition** to the archives, so the folder holds at most `keep_files + 1` files. Both fields are editable in Settings → Logging.

```powershell
# Open logs folder in Explorer
Start-Process "$env:LOCALAPPDATA\com.presencejam.app\logs"
```

To see verbose output in dev mode, check the terminal where `npm run tauri dev` is running.

## How to Submit Changes

### Bug Fixes

1. Fork the repository
2. Create a branch: `fix/short-description`
3. Make your changes
4. Ensure it compiles: `cargo check && npm run check`
5. Run tests: `cargo test` (+ `npm test` for frontend changes)
6. Commit with a clear message
7. Open a Pull Request

### New Features

1. Fork the repository
2. Create a branch: `feature/short-description`
3. Write your implementation
4. Test in dev mode: `npm run tauri dev`
5. Ensure it compiles: `cargo check && npm run check`
6. Run tests: `cargo test` (+ `npm test` for frontend changes)
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
