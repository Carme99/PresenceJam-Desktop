# Acknowledgements

PresenceJam uses the following open-source projects. We're grateful to all the maintainers and contributors.

## Rust Crates

| Crate | Version | License | Purpose |
|-------|---------|---------|---------|
| [tauri](https://github.com/tauri-apps/tauri) | 2.x | MIT OR Apache-2.0 | Desktop application framework |
| [tauri-build](https://github.com/tauri-apps/tauri) | 2.x | Apache-2.0 OR MIT | Build-time codegen (`build.rs`): manifest, permissions and capability resolution |
| [tauri-plugin-opener](https://github.com/tauri-apps/tauri) | 2.x | MIT OR Apache-2.0 | URL opening in default browser |
| [tauri-plugin-deep-link](https://github.com/tauri-apps/tauri) | 2.x | MIT OR Apache-2.0 | Custom protocol handling |
| [tauri-plugin-notification](https://github.com/tauri-apps/tauri) | 2.x | MIT OR Apache-2.0 | Windows toast notifications |
| [tauri-plugin-autostart](https://github.com/tauri-apps/tauri) | 2.x | MIT OR Apache-2.0 | Windows startup registration |
| [tauri-plugin-single-instance](https://github.com/tauri-apps/tauri) | 2.x | MIT OR Apache-2.0 | Single-instance enforcement + deep-link forwarding |
| [tauri-plugin-log](https://github.com/tauri-apps/tauri) | 2.x | MIT OR Apache-2.0 | File-based logging |
| [tauri-plugin-updater](https://github.com/tauri-apps/tauri) | 2.x | MIT OR Apache-2.0 | Auto-update with minisign-signed payloads |
| [tauri-plugin-dialog](https://github.com/tauri-apps/tauri) | 2.x | MIT OR Apache-2.0 | Native save/open dialogs for settings export/import (#673) — **Rust-only**: no npm package and no `dialog:*` capability, because the overwrite confirmation runs in Rust |
| [tauri-plugin-global-shortcut](https://github.com/tauri-apps/tauri) | 2.x | MIT OR Apache-2.0 | App-wide hotkeys for playback and sync, registered per binding (#676) |
| [reqwest](https://github.com/seanmonstar/reqwest) | 0.12 | Apache-2.0 OR MIT | HTTP client for Spotify/Graph APIs |
| [serde](https://github.com/serde-rs/serde) | 1.x | Apache-2.0 OR MIT | Serialization framework |
| [serde_json](https://github.com/serde-rs/json) | 1.x | Apache-2.0 OR MIT | JSON parsing |
| [chrono](https://github.com/chronotope/chrono) | 0.4 | Apache-2.0 OR MIT | Date/time handling |
| [sha2](https://github.com/RustCrypto/hashes) | 0.10 | Apache-2.0 OR MIT | SHA256 for PKCE |
| [base64](https://github.com/marshallpierce/rust-base64) | 0.22 | Apache-2.0 OR MIT | Base64 encoding for PKCE |
| [rand](https://github.com/rust-random/rand) | 0.9 | Apache-2.0 OR MIT | Random number generation — PKCE verifier + launch secret, and the AES-256 key / GCM nonce via `try_fill_bytes` |
| [log](https://github.com/rust-lang/log) | 0.4 | Apache-2.0 OR MIT | Logging facade |
| [directories](https://github.com/soc/directories-rs) | 6.x | Apache-2.0 OR MIT | Standard directory locations (maintained replacement for `dirs`, #418) |
| [parking_lot](https://github.com/Amanieu/parking_lot) | 0.12 | Apache-2.0 OR MIT | Synchronization primitives |
| [url](https://github.com/servo/rust-url) | 2.x | Apache-2.0 OR MIT | URL parsing |
| [keyring](https://github.com/hwchen/keyring-rs) | 3.x | MIT OR Apache-2.0 | OS keychain access (client secret + tokens AES key) |
| [aes-gcm](https://github.com/RustCrypto/AEADs) | 0.11 | Apache-2.0 OR MIT | AES-256-GCM encryption for tokens.json at rest |
| [httpdate](https://github.com/de-vri-es/httpdate-rs) | 1.x | Apache-2.0 OR MIT | HTTP date parsing (ETag / Last-Modified handling) |
| [ts-rs](https://github.com/Aleph-Alpha/ts-rs) | 12.x | MIT | TypeScript type generation from Rust structs |
| [urlencoding](https://github.com/nicokoch/rust-urlencoding) | 2.x | Apache-2.0 OR MIT | URL encoding |

## Node.js Packages

| Package | Version | License | Purpose |
|---------|---------|---------|---------|
| [@tauri-apps/api](https://github.com/tauri-apps/tauri) | 2.x | MIT OR Apache-2.0 | Tauri JavaScript API |
| [@tauri-apps/cli](https://github.com/tauri-apps/tauri) | 2.x | Apache-2.0 OR MIT | Tauri CLI — the `tauri` binary behind every `npm run tauri` command |
| [@tauri-apps/plugin-global-shortcut](https://github.com/tauri-apps/tauri) | 2.x | MIT OR Apache-2.0 | JS binding for the global-shortcut crate; declared so the npm package tracks the crate's minor line — the hotkey path itself is driven from Rust |
| [@tauri-apps/plugin-notification](https://github.com/tauri-apps/tauri) | 2.x | MIT OR Apache-2.0 | Notification plugin (`stores/notifications.ts`) |
| [@tauri-apps/plugin-updater](https://github.com/tauri-apps/tauri) | 2.x | MIT OR Apache-2.0 | Updater plugin (`UpdatePrompt.svelte`) |
| [@sveltejs/adapter-static](https://github.com/sveltejs/kit) | 3.x | MIT | Static site adapter |
| [@sveltejs/kit](https://github.com/sveltejs/kit) | 2.x | MIT | Svelte app framework |
| [@sveltejs/vite-plugin-svelte](https://github.com/sveltejs/vite-plugin-svelte) | 5.x | MIT | Vite Svelte plugin |
| [svelte](https://github.com/sveltejs/svelte) | 5.x | MIT | UI framework |
| [svelte-check](https://github.com/sveltejs/language-tools) | 4.x | MIT | Svelte/TypeScript type checking behind `npm run check` |
| [typescript](https://github.com/microsoft/TypeScript) | 5.x | Apache-2.0 | TypeScript language |
| [vite](https://github.com/vitejs/vite) | 6.x | MIT | Build tool |
| [vitest](https://github.com/vitest-dev/vitest) | 4.x | MIT | Frontend unit-test runner behind `npm test` |
| [@vitest/coverage-v8](https://github.com/vitest-dev/vitest) | 4.x | MIT | v8 coverage provider behind `npm run test:coverage`, the frontend coverage ratchet CI runs |
| [jsdom](https://github.com/jsdom/jsdom) | 26.x | MIT | DOM implementation for the vitest environment |
| [@testing-library/svelte](https://github.com/testing-library/svelte-testing-library) | 5.x | MIT | Component-mount helpers for the Svelte test suite |
| [@types/node](https://github.com/DefinitelyTyped/DefinitelyTyped) | 24.x | MIT | Node.js type declarations for config and tooling files |

**Rust-only plugins.** Not every Rust plugin has a JS counterpart here, and the absences are deliberate: `tauri-plugin-dialog` (settings export/import, #673) has no `@tauri-apps/plugin-dialog` dependency and no `dialog:*` capability in `capabilities/default.json` because the confirm dialog is built and answered in Rust, and the log, opener, autostart and deep-link plugins are likewise driven from Rust (`lib.rs`) with no frontend import — so no `@tauri-apps/plugin-log` / `-opener` / `-autostart` / `-deep-link` row exists in the Node table.

## Third-Party Services

PresenceJam communicates with the following third-party APIs:

- **Spotify Web API** — [Spotify Developer](https://developer.spotify.com/) — For currently playing track data
- **Microsoft Graph API** — [Microsoft Azure](https://learn.microsoft.com/en-us/graph/) — For Teams presence status

Neither the app nor its author is affiliated with Spotify AB or Microsoft Corporation.

---

*This list was generated from Cargo.toml and the `dependencies` + `devDependencies` in package.json.*
