# State of Features — v2.8.0

Quick, no-hedge answers to "does this thing actually work in *my* setup?"
Most of the answers below are tied to a code path or a docs file you can read
end-to-end; the few rows that can't be sourced inline are explicitly flagged
**Verify with maintainer** rather than guessed.

> **Maintenance intent:** This file is updated on every release. If you find a
> row that's stale, the right place to flag it is in a PR against this file;
> do not edit the underlying behavior silently.

## Tested in main (verified during the v2.8.0 release cycle)

| Feature                                               | Status | Where it's wired / verified                                                                                             |
|-------------------------------------------------------|--------|--------------------------------------------------------------------------------------------------------------------------|
| Spotify Premium ↔ Teams status (work/school)            | ✅     | Spotify calls hit `accounts.spotify.com/api/token` (Authorization Code + PKCE, confidential client) + `/v1/me/player/currently-playing`; Teams calls hit `graph.microsoft.com/v1.0/me/presence/setStatusMessage`. The OAuth rounds are wired in `src-tauri/src/spotify.rs` and `src-tauri/src/teams.rs`. |
| Smart sleep between polls                              | ✅     | `src-tauri/src/polling/poll_once.rs` — sleeps until `track.duration_ms − track.progress_ms − 5000ms`, clamped to user-configured `min/max_interval_seconds`. ~240 s of silence per 4-min song. |
| Pause-aware backoff (Spotify idle)                     | ✅     | `src-tauri/src/polling/poll_once.rs` — 30 → 60 → 120 → 300 s cap after consecutive non-playing responses (`Ok(None)` or `is_playing == false`); resets to 30 s only on the next *playing* track. At the 30 s default cadence a full day is ~2880 calls; paused it settles at 1/300 s → ~288-291 calls/24 h (~72-75 per 6 h) — a ~10× reduction. |
| Profanity filter on outgoing Teams status              | ✅     | `src-tauri/src/profanity.rs` — 25-word curated list; leetspeak normalization (`1→i, 3→e, 4→a, $→s, @→a, 0→o, 5→s, 7→t, !→i, |→i`); repeated-character collapse; word-boundary safety for `class`, `assassin`, `cocktail`, `vacuum`. Original profane text is **never** logged. |
| App start minimized (cross-platform) + macOS dock-icon toggle  | ✅     | `config.teams.start_minimized` is read in `src-tauri/src/lib.rs` setup; `window.hide()` runs on **all three platforms** (Windows, macOS, Linux). On macOS only (`#[cfg(target_os = "macos")]`), the same code path additionally calls `tauri::ActivationPolicy::Accessory` to hide the dock icon + menu-bar app menu, making PresenceJam a pure tray-resident app in that case. |
| Per-launch deep-link scheme re-registration           | ✅     | `src-tauri/src/lib.rs` calls `tauri-plugin-deep-link`'s `register_all()` on every launch in the desktop `setup` block. Writes HKCU on Windows, `~/.local/share/applications/presencejam.desktop` + `xdg-mime default` on Linux. (Closes v2.7.0–era deferred item from #66.) |
| App config atomic write                                | ✅     | `src-tauri/src/config.rs::atomic_write_json` — temp-file + fsync + `rename()`. Docs in `ARCHITECTURE.md`.                       |
| Tokens.json atomic write                               | ✅     | `src-tauri/src/token_io.rs::write_tokens_atomic` — same temp-file + fsync + rename pattern.                               |
| ts-rs generated TS types from Rust wire structs         | ✅     | `src-tauri/src/spotify.rs`, `teams.rs`, `commands/sync.rs`, `config.rs` derive `#[ts_rs::TS]` with `#[ts(export, export_to = "../../src/lib/types-generated/")]`. Cargo test regenerates. See `ARCHITECTURE.md` Directory Structure.                 |
| Windows / macOS / Linux release matrix builds          | ✅     | `.github/workflows/release.yml` — 3-way job matrix (`macos-latest`, `windows-latest`, `ubuntu-22.04`). Verified for v2.8.0: all 3 jobs succeeded in Release run #28720779185. Linux produces both `.deb` and `.AppImage` from one `tauri build` invocation. |

## Documented gaps (do work; deliberately out of scope for the version tested)

| Area                                               | Status | Notes                                                                                                                                  |
|----------------------------------------------------|--------|----------------------------------------------------------------------------------------------------------------------------------------|
| Spotify **Free** (non-Premium) users                | ❌     | Spotify's Web API requires a Premium subscription for `/me/player/currently-playing`. This is a Spotify platform restriction — PresenceJam cannot work around it. Don't try to install without Premium. |
| macOS deep-link hijack defence (full)              | ⚠ Partial | `tauri-plugin-deep-link`'s `register()` returns `Err(UnsupportedPlatform)` on macOS. v2.8.0 logs a warning and continues; macOS relies on PKCE + `state`-matching alone. The full `LSSetDefaultHandlerForURLScheme` native-FFI path for macOS is tracked separately — see issue #66. |
| `digest-mismatch: error` (instead of `warn`)       | ⚠ Softened | v2.8.0 sets `digest-mismatch: warn` in both `actions/download-artifact` invocations as a safer first-try default (v8 changed `hash-mismatch` defaults to error). After one clean release cycle without checksum-drift events, this can flip to `error`. |
| Code signing & notarization (Windows + macOS)      | ⚠ Not signed | Binary downloads are unsigned (or unsigned-plus-AppleGatekeeper Bypass for macOS). The README documents the `Right-click → Open` workaround for macOS. Microsoft SmartScreen will warn on first install of an unsigned `.msi`. |

## Have not been verified end-to-end (Verify with maintainer before promising to a user)

| Area                                                         | Status | Why this needs explicit verification                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
|--------------------------------------------------------------|--------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **Microsoft 365 GCC / DoD / EduTenants / China tenants**      | ⚠ Verify | The Graph presence APIs are **not supported** for personal Microsoft accounts, so PresenceJam requires a work or school account. For work/school tenants, `Presence.ReadWrite` (delegated) and `offline_access` are both `AdminConsentRequired: No` per the Graph permissions reference; tenants that disable or restrict user consent (Entra user-consent settings) can route any delegated permission — these included — through admin approval. The China (21Vianet) national cloud does not support `setStatusMessage` at all. If you've tested PresenceJam against a tenant-restricted M365 environment, please file an issue or PR against this file with the result — the maintainer has only verified work/school accounts in the public cloud. To check from your end: open `https://developer.microsoft.com/en-us/graph/graph-explorer`, sign in with the relevant account, and confirm `GET /me/presence` returns 200. If it returns 403/401, that tenant won't work out-of-the-box. |
| **Tauri 2 future major (3.0) compatibility**                  | ⚠ Verify | PresenceJam currently pins Tauri 2.x (`tauri = { version = "2", features = ["tray-icon"] }` in `src-tauri/Cargo.toml`). A future Tauri 3 release may break the `tray-icon`, `deep-link`, or `ActivationPolicy` APIs this doc relies on. The next release after a Tauri-3-stable cutover will need to re-verify the v2.8.0 release matrix and the `register_all()` call shape documented in `ARCHITECTURE.md`. |
| **Linux distributions other than Ubuntu 22.04 / Fedora / Arch** | ⚠ Verify | The CI matrix only tests `ubuntu-22.04`. Distros with non-glibc libc (Alpine, Void), unusual default secret-service setups (Pantheon without gnome-keyring), or Wayland-only desktops may hit edge cases (Tray icon visibility on Wayland is a known ecosystem issue, not specific to PresenceJam). See `SETUP.md#linux-keyring` for the Secret Service requirement that all distros must meet. |
| **`actions/download-artifact` v8 checksum-drift behavior in production** | ⚠ Verify | v2.8.0 set `digest-mismatch: warn` for the first release. If you hit a checksum-drift event during a `v2.8.x` release run, that's the signal to flip to `error` (see the Softened row above). |

## Explicitly out of scope (will not be added in any foreseeable release)

- **Internet-archive / fallback authentication.** PresenceJam is local-first; there is no remote auth proxy.
- **Custom Teams status emoji / message-on-pause.** Teams' own `setStatusMessage` API does not support per-track emoji or pause-specific messages; current behavior (status reflects currently-playing track only) is the maximum surface.
- **Lyrics / non-Now-Playing Spotify data.** The Web API surfaces what's documented; we don't scrape unofficial endpoints.
- **Slack / Discord / other IM status sync.** Out of scope — different platforms with different SDKs; would be a separate project.

## Reporting a stale row

If you find an entry here that doesn't match the code on `main`, open a
PR against this file with the correction + a link to the verified
source (a code reference, doc reference, or a CI run URL). One-row
changes are an ideal first contribution.
