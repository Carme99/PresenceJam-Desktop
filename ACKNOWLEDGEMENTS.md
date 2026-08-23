# Acknowledgements

PresenceJam uses the following open-source projects. We're grateful to all the maintainers and contributors.

## Rust Crates

| Crate | Version | License | Purpose |
|-------|---------|---------|---------|
| [tauri](https://github.com/tauri-apps/tauri) | 2.x | MIT OR Apache-2.0 | Desktop application framework |
| [tauri-plugin-opener](https://github.com/tauri-apps/tauri) | 2.x | MIT OR Apache-2.0 | URL opening in default browser |
| [tauri-plugin-deep-link](https://github.com/tauri-apps/tauri) | 2.x | MIT OR Apache-2.0 | Custom protocol handling |
| [tauri-plugin-notification](https://github.com/tauri-apps/tauri) | 2.x | MIT OR Apache-2.0 | Windows toast notifications |
| [tauri-plugin-autostart](https://github.com/tauri-apps/tauri) | 2.x | MIT OR Apache-2.0 | Windows startup registration |
| [tauri-plugin-log](https://github.com/tauri-apps/tauri) | 2.x | MIT OR Apache-2.0 | File-based logging |
| [tauri-plugin-updater](https://github.com/tauri-apps/tauri) | 2.x | MIT OR Apache-2.0 | Auto-update with minisign-signed payloads |
| [tauri-plugin-http](https://github.com/tauri-apps/tauri) | 2.x | MIT OR Apache-2.0 | HTTP client |
| [reqwest](https://github.com/seanmonstar/reqwest) | 0.12 | Apache-2.0 OR MIT | HTTP client for Spotify/Graph APIs |
| [serde](https://github.com/serde-rs/serde) | 1.x | Apache-2.0 OR MIT | Serialization framework |
| [serde_json](https://github.com/serde-rs/json) | 1.x | Apache-2.0 OR MIT | JSON parsing |
| [chrono](https://github.com/chronotope/chrono) | 0.4 | Apache-2.0 OR MIT | Date/time handling |
| [sha2](https://github.com/RustCrypto/hashes) | 0.10 | Apache-2.0 OR MIT | SHA256 for PKCE |
| [base64](https://github.com/marshallpierce/rust-base64) | 0.22 | Apache-2.0 OR MIT | Base64 encoding for PKCE |
| [rand](https://github.com/rust-random/rand) | 0.8 | Apache-2.0 OR MIT | Random number generation |
| [log](https://github.com/rust-lang/log) | 0.4 | Apache-2.0 OR MIT | Logging facade |
| [dirs](https://github.com/dirs-dev/dirs-rs) | 5.x | Apache-2.0 OR MIT | Standard directory locations |
| [parking_lot](https://github.com/Amanieu/parking_lot) | 0.12 | Apache-2.0 OR MIT | Synchronization primitives |
| [once_cell](https://github.com/matklad/once_cell) | 1.x | Apache-2.0 OR MIT | Lazy statics |
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
| [@tauri-apps/plugin-autostart](https://github.com/tauri-apps/tauri) | 2.x | MIT OR Apache-2.0 | Autostart plugin |
| [@tauri-apps/plugin-deep-link](https://github.com/tauri-apps/tauri) | 2.x | MIT OR Apache-2.0 | Deep link plugin |
| [@tauri-apps/plugin-http](https://github.com/tauri-apps/tauri) | 2.x | MIT OR Apache-2.0 | HTTP plugin |
| [@tauri-apps/plugin-log](https://github.com/tauri-apps/tauri) | 2.x | MIT OR Apache-2.0 | Log plugin |
| [@tauri-apps/plugin-updater](https://github.com/tauri-apps/tauri) | 2.x | MIT OR Apache-2.0 | Updater plugin |
| [@tauri-apps/plugin-notification](https://github.com/tauri-apps/tauri) | 2.x | MIT OR Apache-2.0 | Notification plugin |
| [@tauri-apps/plugin-opener](https://github.com/tauri-apps/tauri) | 2.x | MIT OR Apache-2.0 | Opener plugin |
| [@sveltejs/adapter-static](https://github.com/sveltejs/kit) | 3.x | MIT | Static site adapter |
| [@sveltejs/kit](https://github.com/sveltejs/kit) | 2.x | MIT | Svelte app framework |
| [@sveltejs/vite-plugin-svelte](https://github.com/sveltejs/vite-plugin-svelte) | 5.x | MIT | Vite Svelte plugin |
| [svelte](https://github.com/sveltejs/svelte) | 5.x | MIT | UI framework |
| [typescript](https://github.com/microsoft/TypeScript) | 5.x | Apache-2.0 | TypeScript language |
| [vite](https://github.com/vitejs/vite) | 6.x | MIT | Build tool |

## Third-Party Services

PresenceJam communicates with the following third-party APIs:

- **Spotify Web API** — [Spotify Developer](https://developer.spotify.com/) — For currently playing track data
- **Microsoft Graph API** — [Microsoft Azure](https://learn.microsoft.com/en-us/graph/) — For Teams presence status

Neither the app nor its author is affiliated with Spotify AB or Microsoft Corporation.

---

*This list was generated from Cargo.toml and package.json dependencies.*
