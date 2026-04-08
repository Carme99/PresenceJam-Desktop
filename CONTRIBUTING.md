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

```bash
# Build release binary
npm run tauri build

# Check Rust compilation
cargo check

# Lint frontend
npm run lint

# Type check frontend
npm run check
```

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
- Use `cargo fmt` to format code
- Error handling with `Result` types — no `unwrap()` in production code

### Frontend (Svelte + TypeScript)

- Follow existing component patterns
- Use existing stores for state management
- Add TypeScript types for new interfaces

## AI Tools Welcome

**AI-generated contributions are encouraged.**

If you use AI coding tools (GitHub Copilot, Claude, ChatGPT, etc.) to build features or fix bugs, that's great — just make sure the code:

- Compiles and passes `cargo check` / `npm run check`
- Follows the existing code patterns
- Includes any necessary types
- Has no debug code left in (console.log, etc.)

You don't need to disclose that you used AI — just submit the best code you can.

## How to Submit Changes

### Bug Fixes

1. Fork the repository
2. Create a branch: `fix/short-description`
3. Make your changes
4. Ensure code compiles: `cargo check && npm run build`
5. Commit with a clear message
6. Open a Pull Request

### New Features

1. Fork the repository
2. Create a branch: `feature/short-description`
3. Write your implementation
4. Test in dev mode: `npm run tauri dev`
5. Ensure production build works: `npm run tauri build`
6. Commit with a clear message
7. Open a Pull Request

## Reporting Issues

Please use the [issue templates](./.github/ISSUE_TEMPLATE/) when reporting bugs or requesting features. Include:

- Clear description of the problem or feature
- Steps to reproduce (for bugs)
- Expected vs actual behavior
- Screenshots if applicable
- Your environment (Windows version, app version)

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
