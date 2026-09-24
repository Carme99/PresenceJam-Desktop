# AGENTS.md - PresenceJam

> Single source of truth for AI coding agents (Claude Code, Codex, Cursor, aider,
> OpenClaw, omp, etc.). If your tool reads only one file from this repo, this is
> the one. **CLAUDE.md is deprecated** and points here; see
> [§ Migration from CLAUDE.md](#15-migration-from-claudemd) at the bottom.

PresenceJam syncs what you're playing on Spotify into your Microsoft Teams status,
as a Tauri 2 desktop app (Rust backend, Svelte 5 / TypeScript frontend, SvelteKit
static adapter). This document is the agent contract: how the project is laid
out, how to work in it, and what NOT to do.

For *what the app does*, see [README.md](./README.md). For *how it works*, see
[ARCHITECTURE.md](./ARCHITECTURE.md) and the per-topic pages under
[`docs/architecture/`](./docs/architecture/). For *what is shipped*, see
[`docs/STATE-OF-FEATURES.md`](./docs/STATE-OF-FEATURES.md). For *release mechanics*,
see [`docs/RELEASING.md`](./docs/RELEASING.md).

---

## Table of contents

1. [Tech stack and toolchain](#1-tech-stack-and-toolchain)
2. [Repository layout](#2-repository-layout)
3. [Quick reference — commands and gates](#3-quick-reference--commands-and-gates)
4. [Authoring rules (Rust)](#4-authoring-rules-rust)
5. [Authoring rules (frontend)](#5-authoring-rules-frontend)
6. [i18n contract](#6-i18n-contract)
7. [Auth, tokens and security](#7-auth-tokens-and-security)
8. [Storage paths and atomic write contract](#8-storage-paths-and-atomic-write-contract)
9. [Status format placeholders](#9-status-format-placeholders)
10. [Commit, PR and review conventions](#10-commit-pr-and-review-conventions)
11. [CI gates and how to read them](#11-ci-gates-and-how-to-read-them)
12. [Things agents must NOT do](#12-things-agents-must-not-do)
13. [Per-issue workflow (the recipe)](#13-per-issue-workflow-the-recipe)
14. [Glossary of recurring symbols](#14-glossary-of-recurring-symbols)
15. [Migration from CLAUDE.md](#15-migration-from-claudemd)

---

## 1. Tech stack and toolchain

| Layer | Technology | Pinned |
| --- | --- | --- |
| Shell | Tauri 2 | `tauri = "2"` family |
| Backend language | Rust (edition 2021) | MSRV `1.96` (`src-tauri/Cargo.toml` + `rust-toolchain.toml`) |
| Frontend framework | SvelteKit with `@sveltejs/adapter-static` | Svelte `5.57`, SvelteKit `2.70` |
| Frontend language | TypeScript | `5.9` |
| Build / bundler | Vite | `6.4` |
| Test runners | Vitest (`@vitest/coverage-v8`) + Playwright Chromium | `4.1` / `1.63` |
| Type checker | `svelte-check` | `4.7` |
| Node | npm | `engines.node >= 22`; CI installs Node 24 |

The Rust toolchain pin is **enforced** by `dtolnay/rust-toolchain@1.96` in
`ci.yml` and by the root `rust-toolchain.toml`. `cargo fmt` / `cargo check` /
`cargo clippy` cannot silently run on a different toolchain. If you bump the MSRV,
bump both files and `ci.yml` together.

The Node version is enforced by `actions/setup-node@6.5.0` with
`node-version: 24`. `package.json` declares `engines.node >= 22`; if you bump
the floor, document the new minimum in [CONTRIBUTING.md](./CONTRIBUTING.md).

Tauri CLI is **repo-pinned** under `devDependencies` (`@tauri-apps/cli`). Always
use `npm run tauri …` — never `cargo tauri …` or a global install.

Generated types live in `src/lib/types-generated/` (gitignored). They are
materialised by `cargo test --lib` (the `#[ts(export)]` derives run there). Run
Rust tests *before* `npm run check` whenever an exported struct changed, or the
frontend type-check reads stale generated types.

---

## 2. Repository layout

```
.
├── ARCHITECTURE.md          # index of docs/architecture/ — start here
├── CHANGELOG.md             # Keep a Changelog; Unreleased → versioned per release
├── CLAUDE.md                # DEPRECATED → AGENTS.md
├── CODEOWNERS               # reviewers by path
├── CONTRIBUTING.md          # dev setup + standards (mirrors this file)
├── LICENSE                  # MIT
├── README.md                # what the app does + downloads
├── SECURITY.md              # security policy + token storage disclosure
├── SETUP.md                 # first-time Spotify Developer app registration
├── TROUBLESHOOTING.md
├── USAGE.md                 # day-to-day guide
├── docs/                    # long-form docs, per-topic
│   ├── PLATFORMS.md
│   ├── RELEASING.md
│   ├── STATE-OF-FEATURES.md
│   ├── architecture/        # six subject pages behind ARCHITECTURE.md
│   └── link-audit.py
├── packaging/               # OS packaging units (systemd, launchd, Task Scheduler, …)
├── src/                     # SvelteKit app
│   ├── app.css              # the design-token source of truth (see §5)
│   ├── lib/
│   │   ├── components/      # About, Dashboard, Onboarding, Settings, Reconnect,
│   │   │                    #   LogViewer, Diagnostics, UpdatePrompt, DeviceCodeBox,
│   │   │                    #   PageHeader, Logo
│   │   ├── i18n.ts + i18n/   # `t()` barrel, en/de/fr dictionaries, Dict parity type
│   │   ├── stores/          # app, authFlow, config, detach, notifications, presence, theme
│   │   ├── types-generated/ # gitignored; ts-rs output (Rust → TS)
│   │   ├── types.ts
│   │   └── utils/
│   └── routes/
├── src-tauri/
│   ├── Cargo.toml           # version + MSRV + deps
│   ├── deny.toml            # cargo-deny license + source policy
│   ├── capabilities/
│   │   ├── default.json     # main-window capability allowlist
│   │   └── detached.json    # minimal detached-window permissions
│   ├── icons/
│   ├── tauri.conf.json      # bundle id, window size, CSP, updater config
│   └── src/
│       ├── lib.rs           # entry point — AppState, run(), deep-link, CLI
│       ├── main.rs          # `fn main()` only
│       ├── i18n.rs          # Rust-side tray/menu i18n table
│       ├── spotify.rs       # Spotify Web API client (PKCE auth)
│       ├── teams.rs         # Microsoft Graph client (device-code flow)
│       ├── profanity.rs     # filter
│       ├── keychain.rs      # 256-bit AES-GCM key storage (OS keychain)
│       ├── token_io.rs      # AES-GCM encrypt/decrypt of tokens.json
│       ├── config.rs        # AppConfig + load/save + clamp_* helpers
│       ├── diagnostics.rs   # redacted support snapshot
│       ├── updater_bg.rs    # silent background updates + deferred ("Install on quit")
│       ├── tray.rs          # system tray + playback menu (single dispatcher since #804)
│       ├── menu.rs          # macOS app menu
│       ├── serve.rs         # `--serve` token-guarded localhost API
│       ├── pkce.rs          # PKCE verifier/challenge helpers
│       ├── history.rs       # bounded status-decision history ring
│       ├── calendar.rs      # Outlook calendar pre-gate (cached, throttled)
│       ├── macos_deeplink.rs
│       ├── platform/        # focus, idle (Windows-only probes; Unknown elsewhere)
│       ├── polling/
│       │   ├── mod.rs
│       │   ├── loop.rs      # the single sync thread + smart sleep
│       │   ├── poll_once.rs # one-iteration helper (used by --sync-once too)
│       │   ├── state.rs     # polling state machine
│       │   └── daemon.rs    # `--daemon` supervisor
│       ├── commands/        # one file per command family (config, auth, sync,
│       │                    #   window, playback, rules, shortcuts, misc)
│       └── sources/         # Spotify + MPRIS + SMTC (Windows) playback sources
├── tests/                   # Vitest (*.test.ts) + Playwright (tests/browser/*.spec.ts)
├── playwright.config.ts     # Chromium browser test configuration
├── vitest.config.js         # derives from vite.config.js (issue #839)
├── vite.config.js
├── svelte.config.js
└── package.json
```

A **reader** lands on the docs in this order:

1. `README.md` — what it does, downloads.
2. `ARCHITECTURE.md` — index, links to six subject pages.
3. `docs/STATE-OF-FEATURES.md` — row-by-row what's shipped and verified.
4. `docs/RELEASING.md` — release mechanics.
5. `docs/architecture/*.md` — deep dives, on demand.

An **author** lands on these surfaces:

- `src-tauri/src/` for Rust, `src/` for the frontend, `tests/*.test.ts` for Vitest, and `tests/browser/*.spec.ts` for Playwright browser geometry.
- `src-tauri/Cargo.toml` and `package.json` for versions (both kept in lock-step).
- `src-tauri/tauri.conf.json` for bundle id, window geometry, CSP, updater config.
- `src-tauri/capabilities/default.json` for the webview capability allowlist.
- `src/lib/i18n.ts` and `src-tauri/src/i18n.rs` for the bilingual i18n tables.

---

## 3. Quick reference — commands and gates

| Task | Command |
| --- | --- |
| Install dependencies | `npm install` |
| Start dev mode (hot reload) | `npm run tauri dev` |
| Build release | `npm run tauri build` |
| Frontend type-check | `npm run check` |
| Frontend unit tests | `npm test` |
| Frontend tests + coverage (CI ratchet) | `npm run test:coverage` |
| Browser geometry regression | `npm run test:browser` |
| Rust compile gate | `cargo check --manifest-path src-tauri/Cargo.toml --all-targets` |
| Rust tests (also materialises ts-rs codegen) | `cargo test --manifest-path src-tauri/Cargo.toml --all-targets` |
| Format Rust | `cargo fmt --manifest-path src-tauri/Cargo.toml` |
| Lint Rust | `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` |
| Markdown link audit (CI `docs-links` job) | `python3 docs/link-audit.py` |
| Dependency audit | `cargo deny check` and `npm audit --omit=dev --audit-level=high` |

**Always run the gates below before pushing a PR.** See §11 for which CI jobs
fail for which defect.

1. `cargo check --manifest-path src-tauri/Cargo.toml --all-targets`
2. `cargo test --manifest-path src-tauri/Cargo.toml --all-targets`
3. `npm run check`
4. `npm test`
5. `npm run test:browser`
6. `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`
7. `python3 docs/link-audit.py` (only if you edited any markdown)

The CI workflow runs Ubuntu, macOS, and Windows jobs. The Linux and macOS Rust jobs execute `cargo test --all-targets`; the Windows leg does not run the full all-target suite because the lib test binary aborts at loader on the current `windows-latest` image (`STATUS_ENTRYPOINT_NOT_FOUND`), but `windows-cli-smoke` still performs a real release build and runs the focused `test_windows_cli_attaches_parent_console_before_output` regression. Re-expand the full Windows matrix once the runner image links the binary cleanly — track that in issue #836.

---

## 4. Authoring rules (Rust)

### Style

- **Module log tags in square brackets**: `[CONFIG]`, `[POLL]`, `[TEAMS]`, etc.
  The single dispatcher in `tray.rs` and the menu dispatcher in `menu.rs` already
  share a `[MENU]` tag — keep new sub-dispatchers inside the same module under
  the same tag.
- **Logging**: `log::info!`, `log::warn!`, `log::error!`, `log::debug!`. Never
  `println!` or `eprintln!` in production code paths. `log!` macros at `info`
  and above land in `PresenceJam.log`; `debug!` is filtered out by the default
  tauri-plugin-log level.
- **Errors**: `Result<T, E>` everywhere on fallible I/O and parse paths.
  `unwrap()` is **disallowed** on any code that could see production data. The
  sole exception is the `tray.rs` `cached_devices` cache-hit fast path, which
  unwraps a snapshot it just proved is `Some` — and that one is documented in
  `CLAUDE.md` §Rust for historical reasons; do not add new `unwrap()` calls.
- **No panics in hot loops**. `polling/loop.rs` and `polling/poll_once.rs` are
  the long-lived sync thread; classify every error, log it, and back off.
- **`#[cfg(target_os = "…")]`** must be **minimal**. Every cross-platform gate
  in CI is in `ci.yml` — a regression inside `#[cfg(target_os = "macos")]`
  fails the macOS leg by construction, and the same for `#[cfg(windows)]` on
  the Windows leg. Linux-path-only or secret-service-assuming tests must be
  `#[cfg(target_os = "linux")]` gated, not deleted.
- **`#[cfg(test)]`** for unit tests inside source files; integration tests live
  in `src-tauri/tests/` if any are added later.

### Clamps

`config.rs` exposes `clamp_polling`, `clamp_teams`, `clamp_track_rule_action`,
`clamp_presence_profiles`, and friends. **Every** new user-tunable config field
goes through a clamp helper — never let the user save out-of-range values into
`config.json`. Clamps are tested by unit tests next to the helper; add the test
when you add the clamp.

### ts-rs

Exported structs that reach the webview use `#[ts(export)]`. The codegen output
is gitignored and materialised by `cargo test --lib`. New IPC payloads: prefer
the typed `emit_*` helpers over `json!` literals — the type-safe pattern is in
`src-tauri/src/commands/sync.rs`. Track any deviation in the v5 backlog issue
#762 ("type the Rust-side emit payloads with ts-rs instead of json! literals").

### Performance

- `Arc<…>` for shared state in `AppState`. The polling loop never `clone()`s
  `AppConfig` per iteration — see issue #893 ("stop deep-cloning AppConfig on
  every poll iteration"); read the patch before touching the loop.
- `RwLock` for read-heavy state, `Mutex` for write-heavy. `tokio::sync::RwLock`
  is async-friendly; `parking_lot::RwLock` is the synchronous fast path. Pick
  by call-site, not by preference.
- `parking_lot` is the project default for sync locks; do **not** introduce a
  second lock crate without a benchmark and a v5 task.

### Diagnostics

Anything user-visible in `get_diagnostics_snapshot` flows through
`diagnostics::redact_sensitive` (or whatever the unified helper is named after
issue #910 lands — `chore: unify redaction formatting behind one shared helper`).
Never write a credential-shaped string into the snapshot directly.

---

## 5. Authoring rules (frontend)

### Components

- Svelte 5 runes (`$state`, `$derived`, `$effect`, `$props`). Snippets, not
  slots, for any new component; the `refactor: replace the last legacy <slot />`
  task (#779) is in-flight.
- Each component owns one feature. Cross-component state lives in a store under
  `src/lib/stores/`. Do not pass ad-hoc callbacks four layers deep — hoist to
  a store and read it where it's needed.
- Stores are `.svelte.ts` when they hold runes (Svelte 5 reactivity); plain
  `.ts` when they export pure TS objects.
- PageHeader is the single header component — every screen with a heading
  uses it. No bespoke `.header` rules.

### Design tokens

`src/app.css` is the design-token source of truth. **No raw hex literals in
component CSS** outside `app.css` — the v5 polish epic (#725) tracks the
remaining cleanup; theme preview swatches (#904) and the appearance card
reset (#970) are already in. Tokens that exist and have no consumer are
tracked in issue #902 (`remove app.css dead tokens and wire the update banner`)
and removed by that PR — do not reintroduce them.

### Buttons

Every button is a `Button` from `$lib/components/` (or its variant). One-off
button styles are tracked in #903 (`chore: unify one-off button styles on the
shared button primitives`) — do not add new ad-hoc styles, fix the variant.

### Detached panes

Logs and Settings can detach into a separate window. The detached-pane router
in `src/lib/stores/detach.ts` is the only place that decides which view owns
which window. New top-level views that want detach support wire into the
detached router and add a render test (`tests/` covers #857).

### Navigation guard

Programmatic navigation (`show-about`, dirty-Settings back, Onboarding
skip-to-dashboard) goes through the navigation guard in `+layout.svelte`. Do
not call `goto()` directly from a button — see #817 (menu navigation respects
unsaved Settings edits) and #815 (About no longer eats the onboarding wizard)
for the precedent.

### Test surface

Vitest is the default frontend unit/component runner; new tests that do not need real browser layout live in `tests/*.test.ts`. Use Playwright for behavior whose contract is browser geometry: tests live in `tests/browser/*.spec.ts`, run through `npm run test:browser`, and the `frontend` CI job plus release verification both install Chromium and run them. The current browser gate proves every localized LogViewer level badge stays clear of its message in en/de/fr at comfortable and compact densities. Keep that real-layout check in Playwright rather than approximating it in JSDOM.

The Vitest coverage ratchet remains a separate gate: per-file floors in `vitest.config.js` are an absolute floor, not a target — coverage drops fail the `frontend` CI job.

### Debug logging

Use `devLog()` from `$lib/utils/dev` — it is a no-op in production builds.
`console.error` and `console.warn` are fine for real errors that should
always be visible. Plain `console.log` is forbidden in `src/`.

---

## 6. i18n contract

- Three locales ship: **en, de, fr** (`src/lib/i18n/en.ts`, `de.ts`, `fr.ts`).
- The shared `Dict` type enforces key parity at compile time: any missing or
  extra key in any of the three dictionaries fails `npm run check`.
- **All user-facing UI strings MUST go through `t()` from `$lib/i18n`** — no
  hard-coded English strings in components. The single exception is literal
  punctuation and the app name itself.
- The Rust side (`src-tauri/src/i18n.rs`) holds its own tray/menu table.
  Rust-side error strings stay English (documented limitation; tracked in the
  v5 backlog).
- Cross-locale placeholder parity is enforced by `tests/i18n.test.ts` (#752):
  per-key `{param}` sets must match in all three dicts, and every literal
  `t()` / `tCount()` call passes exactly the en placeholders. Five
  brace-literal keys are allowlisted by name; check the list before adding
  a new placeholder.
- When the German translations disagree with the webview terminology, fix
  both surfaces — see #901 (`align the German tray wording with the webview
  terminology`). The Rust and TS tables share the same key *names*, but the
  values must agree on the visible noun (e.g. both say `Logs öffnen` or both
  say `Protokoll öffnen` for the same control).

---

## 7. Auth, tokens and security

### Spotify

Authorization Code + PKCE (confidential client — the app holds the client
secret out of band via a local proxy / environment wiring in CI). Browser
redirects to `presencejam://callback`. `pkce.rs` holds the verifier / challenge
helpers; **never log the verifier or the code**, even at debug.

### Teams

Device Code flow — `commands::teams_auth::start_teams_auth_device_code` calls
`login.microsoftonline.com` on the blocking pool (#878) and polls at the
server-provided interval clamped to 1–15 s, with +5 s RFC 8628 `slow_down`
backoff. The poll is single-flight: a superseded or abandoned device-code
poll cannot report success — see #933.

### Token storage

`tokens.json` is AES-256-GCM ciphertext. The 256-bit key lives in the OS
keychain via `keyring`:

- Windows → DPAPI
- macOS → Keychain
- Linux → Secret Service

Decryption happens in `src-tauri/src/token_io.rs` (and `keychain.rs` for the
key itself). **Never** write plaintext tokens to disk. **Never** include
tokens in `config.json` (which is plaintext JSON by design).

### Capability allowlist

`src-tauri/capabilities/default.json` lists every Tauri command the webview may invoke. The main capability is least-privilege: unused global-shortcut and autostart grants are absent, and the updater/notification grants match the commands the webview actually uses. If you add a new IPC command, the only way the webview can call it is by being listed here.

### CSP

`tauri.conf.json` sets `csp` to an explicit allowlist: `default-src 'self'` governs scripts; Spotify image origins; `style-src 'self' 'unsafe-inline'`; the Spotify, Microsoft login, and Microsoft Graph connect origins; `object-src 'none'`; `base-uri 'self'`; `frame-ancestors 'none'`; and `form-action 'none'`. The app submits state through Tauri IPC rather than HTML forms, so the explicit form ban breaks no supported flow. `updater_bg::test_tauri_conf_disallows_downgrades` pins both `base-uri 'self'` and `form-action 'none'`. Do not loosen any directive without a security review.

### Diagnostics

`get_diagnostics_snapshot` builds a redacted, typed snapshot in memory. The opt-in `save_diagnostics_snapshot` command accepts neither bytes nor a destination: Rust recollects the typed snapshot, serializes it under the independent 256 KiB cap, chooses the `presencejam-diagnostics-*.json` filename, and atomically publishes it in the platform Downloads directory. The webview supplies no JSON to save; do not weaken this into a caller-supplied-content write.

The snapshot is redacted on the way out — never put a token, refresh token, client secret, PKCE verifier, session id, or bearer token into the snapshot.

---

## 8. Storage paths and atomic write contract

`config.json` lives in the OS app config dir:

- Windows: `%APPDATA%\PresenceJam\config.json`
- macOS: `~/Library/Application Support/PresenceJam/config.json`
- Linux: `$XDG_CONFIG_HOME/PresenceJam/config.json`

`tokens.json` lives in the **bundle-id** folder, which **differs** from the
config folder on every OS because Tauri appends the bundle id to
`app_config_dir()` but not to `app_data_dir()` (issue #300):

- Windows: `%APPDATA%\com.presencejam.app\PresenceJam\tokens.json`
- macOS: `~/Library/Application Support/com.presencejam.app/PresenceJam/tokens.json`
- Linux: `$XDG_CONFIG_HOME/com.presencejam.app/PresenceJam/tokens.json`

Logs live in `app_log_dir()`:

- Windows: `%LOCALAPPDATA%\com.presencejam.app\logs\PresenceJam.log`
- macOS: `~/Library/Logs/com.presencejam.app/PresenceJam.log`
- Linux: `~/.local/share/com.presencejam.app/logs/PresenceJam.log`

**Atomic write** is the project standard for every config write. The
recipe — stage to a temp sidecar, fsync, rename — is in `config::write_atomic`.
Do not write the live file in place; do not delete the temp sidecar while the
process still owns it. Issue #939 ("stage an imported config file before
moving the live one aside") and #946 ("run the config import's file replace
inside the write critical section") are the live v5 tasks — read them before
touching the import path.

The log directory and every `PresenceJam*.log*` file are user-only on Unix
(0700 / 0600); a watchdog re-tightens after rotation (#920).

---

## 9. Status format placeholders

The status format is a template substituted in **one** pass; tokens are not
re-scanned, so a track literally named `{album}` is not expanded into the
album name.

| Token | Source | Notes |
| --- | --- | --- |
| `{artist}` | Artist name | On an episode: the show name |
| `{track}` | Track name | On an episode: the episode name |
| `{album}` | Album name | On an episode: the publisher |
| `{emoji}` | `🎵` (track) / `🎙️` (episode) / `⏸️` (paused) | |
| `{device}` | Device name | |
| `{playlist}` / `{context}` | Playlist / album / artist / show | Two tokens, exact aliases |
| `{progress}` | Playback position (`3:07`) | Empty when Spotify reports none |
| `{shuffle}` / `{repeat}` | `🔀` / `🔁` while on, empty while off | |
| `{show}` / `{episode}` / `{publisher}` | Episode-only | Empty on a music track |

Default: `🎵 {artist} - {track} 🎧`. Episode default: `🎙️ {show} - {episode}`
(the music template is not applied to episodes).

---

## 10. Commit, PR and review conventions

### Commits — Conventional Commits

```
feat: add dark mode support
fix: correct token refresh logic
docs: update README
refactor: extract auth module
test: cover the diagnostics save path
chore: bump @tauri-apps/api to 2.11.1
```

Allowed types: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`. Use scope
when it clarifies the change surface (`feat(auth): …`,
`fix(teams): …`, `chore(ci): …`). Body explains *why*; the diff shows *what*.

### Branches

- `main` is protected and fast-forwarded only.
- One PR per concern; stacked PRs are tracked under the stack-pr-playbook
  skill (rare, with explicit coordination).
- Branch names use the issue number when relevant:
  `fix/<short-slug>-<issue>` or `feat/<short-slug>-<issue>`.

### Review bar

Every PR is reviewed by **two** independent reviewer agents before merge —
this is a project rule, not a preference. Each reviewer scores 0–100 against
the rubric in the wave-6 retrospective (`docs/RELEASING.md` §3 + §5). The bar
is **100/100** from each reviewer; the merge is gated on both. If a reviewer
flags a gap, the PR fixes the gap and re-runs the gate; the gate is binary.

The review rubric:

- Correctness — does the change do what the issue says, end to end?
- Tests — does the test fail pre-fix and pass post-fix? Does it assert
  *observable behaviour*, not source text or wiring?
- Security — no new credential-shaped string reaches a log, IPC payload,
  diagnostics snapshot, or webview. The capability allowlist stays minimal.
- Localisation — every new user-facing string is in `t()`; placeholder parity
  is preserved; the German tray/webview disagreement is fixed, not added to.
- Docs — CHANGELOG entry under `[Unreleased]`; `STATE-OF-FEATURES.md` row
  updated; any doc that cites the changed code points at the new line.
- Style — log tags in `[BRACKETS]`, no `println!`, no `unwrap()` on
  fallible paths, no hard-coded hex outside `app.css`, no new one-off
  button styles.
- Verification — the local gate set has been run (`cargo check`, `cargo test --all-targets`, `npm run check`, `npm test`, `npm run test:browser`); the CI matrix outcome is green.

### Issue hygiene

- Issues first, fixes later — log P3s in batches before fixing them (project
  rule from the v5 wave-1 retrospective).
- A PR description that closes issues uses the literal `Closes #N` keyword;
  prose like "closes the remaining sub-items of #N" does NOT auto-close.
- After a wave lands, run a `post-merge-issue-still-open-sweep` to find any
  parent issue that should have closed but didn't.

---

## 11. CI gates and how to read them

`ci.yml` runs the following jobs. Failures on each job point at the same
fix:

| Job | OS | What it does | Common failure modes |
| --- | --- | --- | --- |
| `rust-platform-check` | macOS + Windows | `cargo check --all-targets` on both legs; `cargo test --all-targets` on macOS only | cfg-gated code path breaks on one OS; Linux-only test runs on macOS |
| `windows-cli-smoke` | Windows | Real release build + `cmd /c --help` smoke + regression test | `cfg(windows)` code path regressed; CLI arm does not print |
| `frontend` | Ubuntu | `npm run build`, `cargo test --lib` (materialises ts-rs codegen), `npm run check`, `npm run test:coverage`, `npm run test:browser` | Generated types are stale; new `t()` call missing a key; coverage floor dropped; browser geometry regressed |
| `rust` | Ubuntu | `cargo fmt --check`, `cargo check --all-targets`, `cargo test --all-targets` | compile error, warning, or failing Rust test |
| `rust-clippy` | Ubuntu | `cargo clippy --all-targets -- -D warnings` | new clippy lint |
| `changelog-links` | Ubuntu | Every `## [X]` header has a matching `[X]:` definition | malformed changelog link section |
| `docs-links` | Ubuntu | `python3 docs/link-audit.py` | relative markdown link or anchor is broken |
| `version-consistency` | Ubuntu | The three release-facing manifest versions agree | version drift between `tauri.conf.json`, `package.json`, and `Cargo.toml` |
| `secret-scan` | Ubuntu | Gitleaks scans repository history | credential-shaped content in history |
| `dep-audit` | Ubuntu | npm production audit gates; Cargo audit and the full npm tree are advisory | new production npm advisory or audit step failure |
| `no-vendored-binaries` | Ubuntu | Fails on untracked, unignored root paths | vendored browser/build binary enters the tree |
| `cargo-deny` | Ubuntu | Cargo-deny checks dependency licenses and sources | incompatible license or disallowed source |
| `rust-coverage` | Ubuntu | llvm-cov line coverage and per-file floors | coverage falls below the ratchet |

The `rust-platform-check` job is the one that catches **cross-platform
regressions**: a regression inside `#[cfg(target_os = "macos")]` fails the
macOS leg by construction; the same for `#[cfg(windows)]` on the Windows
leg.

The `frontend` job runs `cargo test --lib` first to materialise the ts-rs
codegen, then `svelte-check` against the freshly generated types. If you
forget this ordering locally, you will see "unknown export" errors that
disappear after `cargo test --lib`.

---

## 12. Things agents must NOT do

This is the hard-won list. If you are about to do any of these, **stop** —
the v5 retrospectives track why each one was forbidden.

- **Do NOT add a new `println!` / `eprint*!` in production code.** Logging is
  `log::*!`. The CLI failure path flushes the logger first because
  tauri-plugin-log buffers — see [§4 Authoring rules (Rust)](#4-authoring-rules-rust).
- **Do NOT add a `unwrap()` on any fallible I/O or parse path.** The one
  documented exception is the `tray.rs` `cached_devices` cache-hit fast
  path; everything else is a `Result` or an `.expect("reason")` with a
  real reason.
- **Do NOT write plaintext tokens, refresh tokens, client secrets, PKCE
  verifiers, session ids, or bearer tokens to disk or to a log.** The
  keychain-backed AES-GCM envelope is the only place tokens live. Logging
  a token shape is forbidden even at debug.
- **Do NOT log raw event ids, menu ids, or device ids at info level.** The
  unknown-event redaction is the helper `tray.rs` uses; reuse it, do not
  inline it.
- **Do NOT introduce a second lock crate** next to `parking_lot`. If you
  think you need `tokio::sync::Mutex` or `std::sync::Mutex`, the answer is
  almost always the existing one with a smaller critical section.
- **Do NOT loosen the CSP** in `tauri.conf.json` without a security review.
  The allowlist is the allowlist; adding `'unsafe-inline'` is a security
  regression.
- **Do NOT add new entries to `capabilities/default.json` for commands the
  webview does not invoke.** The v5 epic is *removing* dead grants; do
  not re-add them.
- **Do NOT hard-code English UI strings** in components. Every new
  user-facing string is a `t()` key in `en.ts`, `de.ts`, `fr.ts`.
- **Do NOT introduce new hex colour literals** outside `src/app.css`. Every
  colour goes through a token; theme preview swatches are token-driven.
- **Do NOT create a new one-off button style.** Use the existing `Button`
  variants; if you need a new look, add a variant, not a style.
- **Do NOT skip the gate set** in §3 before pushing. A green CI matrix
  is the merge contract; locally green is the prerequisite.
- **Do NOT close an issue from prose** ("closes the remaining sub-items
  of #N"). Use the literal `Closes #N` GitHub keyword.
- **Do NOT add raw markdown links across nested READMEs** without
  counting relative depth — `docs/link-audit.py` checks depth explicitly
  and the rule is in the audit script's README.
- **Do NOT commit `chrome-headless-shell/`** or any vendored binary. The
  directory is `.gitignore`d; the CI guard (`no-vendored-binaries`)
  refuses the commit if the directory is staged.
- **Do NOT re-add a `--allow-…` Dependabot advisory** to the ignore list
  past its expiry. Issue #642 tracks the audit-clear backlog.
- **Do NOT bump versions** (in `package.json`, `src-tauri/Cargo.toml`,
  `src-tauri/tauri.conf.json`, `CHANGELOG.md`, `docs/STATE-OF-FEATURES.md`)
  without bumping all four sites together.
- **Do NOT bypass the navigation guard** to "just open the page". Every
  programmatic navigation goes through the guard.
- **Do NOT skip the profanity filter** on any code path that ends at the
  Teams status. The filter is the only thing standing between a crafted
  track title and a Teams status blast.
- **Do NOT write `unsafe`** in production code. There is no `unsafe` in
  this codebase today; do not introduce one.
- **Do NOT bypass `redact_sensitive`** when building a diagnostics payload.
- **Do NOT change the bundle id** (`com.presencejam.app`) — it is the
  on-disk anchor for `tokens.json` and `app_log_dir()`.

---

## 13. Per-issue workflow (the recipe)

This is the canonical agent recipe for a v5 issue. Use it for every issue,
every time. Steps that are not relevant are skipped, not reordered.

1. **Read the issue body.** It has a Summary, Evidence (file:line citations),
   Impact, and Acceptance. If any section is empty, the issue was filed
   before the wave template and the agent needs to backfill the missing
   sections.
2. **Re-verify the file:line citations.** `Read` the cited lines. The
   `doc-claim-verification-before-edit` rule applies — citations go stale
   after every PR. If a citation no longer matches the source, fix the
   issue body before fixing the code.
3. **Decide the slice.** One concern, one PR. If the issue spans two
   concerns, file a follow-up; do not bundle.
4. **Read the surrounding code.** The cited file is a starting point, not
   the whole story. `lsp references` on the symbol you are about to change;
   missing callsites are bugs.
5. **Write the test first** (when the issue is a bug). The test must
   *fail on the pre-fix code* and *pass on the post-fix code*. If you
   cannot write such a test, the issue is not a bug — it is a refactor.
6. **Apply the fix.** Minimal diff. No drive-by refactors.
7. **Run the gate set** in §3.
8. **Re-read the diff.** Self-review against the rubric in §10. If a
   reviewer would flag it, fix it now.
9. **Commit.** Conventional Commit message, scope if it clarifies.
10. **Push and open the PR.** PR body uses the issue template, includes
    the literal `Closes #N`, links the verification you ran.
11. **Wait for the two reviewer scores.** If either reviewer scores < 100,
    fix the named gap, push a follow-up commit, and re-run the gate.
12. **After both reviewers score 100/100, merge** with squash.

If the issue touches shared files (e.g. `config.rs`, `app.css`,
`default.json`), coordinate through `hub` before editing — the wave
playbook requires a worktree-per-branch, not a single shared checkout.

---

## 14. Glossary of recurring symbols

A quick-reference for symbols that recur across the code. If you encounter
one outside this list, treat it as a bug and file an issue.

| Symbol | What it is |
| --- | --- |
| `AppState` | The single Rust state object (config, token slots, tray handles, deep-link state). See `src-tauri/src/lib.rs`. |
| `AppConfig` | The on-disk + in-memory config struct. Clamps live in `config.rs`. |
| `clamp_*` | The family of input-sanitisation helpers in `config.rs`. |
| `redact_sensitive` | The single redaction helper for diagnostics. Tracked by #910. |
| `[TAG]` | Module log tag in square brackets (`[CONFIG]`, `[POLL]`, `[TEAMS]`). |
| `gated_track_key` | The presence-gate re-check key, evaluated on a 240 s mid-track clock. |
| `gate_when_*` | The opt-in presence gates (`gate_when_out_of_office`, `gate_when_presenting`, `idle_away_after_seconds`). |
| `respect_manual_status` | The opt-in that prevents overwriting a hand-set Teams status. |
| `pause_polling` | The "polling paused until…" window. Snooze and resume operate on it. |
| `--status` / `--sync-once` / `--serve` / `--daemon` / `--minimized` | The five documented CLI flags. See README §Command-line flags. |
| `presencejam://` | The deep-link scheme for the Spotify PKCE callback. |
| `devLog()` | The frontend debug logger. No-op in production. |
| `t()` | The i18n lookup from `$lib/i18n`. |
| `Dict` | The shared TS type that enforces en/de/fr key parity. |
| `ts(export)` | The ts-rs derive that materialises `src/lib/types-generated/*.ts`. |

---

## 15. Migration from CLAUDE.md

`CLAUDE.md` predates the cross-tool convention of `AGENTS.md`. As of this
revision, `AGENTS.md` is the single source of truth and `CLAUDE.md` is
deprecated. The deprecated file is kept in the tree only as a redirect stub
and will be removed in a follow-up once every external tool that read it has
been re-pointed.

The contents of the old `CLAUDE.md` are migrated into the relevant sections
of this file:

- Dev commands → §3 Quick reference.
- Conventions (Conventional Commits, Rust `unwrap()` policy, log tags) →
  §4 Authoring rules (Rust) and §10 Commit, PR and review conventions.
- Frontend conventions (Svelte 5, `devLog`, `t()`) → §5 Authoring rules (frontend)
  and §6 i18n contract.
- Key Files table → §2 Repository layout.
- Auth Flows → §7 Auth, tokens and security.
- Storage → §8 Storage paths and atomic write contract.
- Status Format Placeholders → §9 Status format placeholders.

If a tool you are using only reads `CLAUDE.md`, point it at `AGENTS.md`
explicitly. New agents should default to `AGENTS.md` from the start.

---

## See also

- [README.md](./README.md) — what the app is and how to install it.
- [CONTRIBUTING.md](./CONTRIBUTING.md) — dev setup + standards (mirrors §3-§10
  from a human contributor's perspective).
- [ARCHITECTURE.md](./ARCHITECTURE.md) — index of the per-topic architecture
  pages.
- [docs/STATE-OF-FEATURES.md](./docs/STATE-OF-FEATURES.md) — row-by-row
  "shipped and verified" matrix.
- [docs/RELEASING.md](./docs/RELEASING.md) — release mechanics and the
  review-gate workflow.
- [SECURITY.md](./SECURITY.md) — security policy and disclosures.
- [docs/architecture/overview.md](./docs/architecture/overview.md) — the
  system diagram and the CI/CD pipeline.
- [CODEOWNERS](./CODEOWNERS) — reviewer routing by path.
