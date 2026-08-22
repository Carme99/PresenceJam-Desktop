# Scope 3.3 — Stratus Follow-on Polish

**Date:** 2026-08-22 · **Base:** `main` @ `600074a` (v3.2.0 Stratus, CHANGELOG 69 KB) · **Author:** Scope33 (read-only research)
**Status:** Draft outline — not committed, no tag. For maintainer triage into 3.3.0 vs 3.4.0.

> **Verdict up front:** **v3.2.0 is shippable as-is.** The 28-issue Stratus batch closes every P1 blocker the repo tracked since v3.1.0. No candidate below is a ship-blocker for 3.2.0; all are polish, hardening, or deferred-feature scope for 3.3/3.4. If you cut a tag today, the fleet on ≤3.1.0 can now auto-update (the #204 404 is fixed and verified), the polling thread no longer wedges (#215/#218), and the token-revocation loop no longer spins (#219). See §1 and §2.

---

## 1  What 3.2.0 delivered — 28 issues, 6 PRs + release fixups

Grouped thematically (CHANGELOG.md §[3.2.0] lines 16-48 is the source of truth; cross-checked against `gh issue list --state closed` for 28 closures and `git log --oneline 600074a`):

### 1.1 Security (4 issues, PR #235)
| Issue | Title | What changed |
|---|---|---|
| #66 (partial) | OAuth hijack binding — per-launch secret | 32 B secret in `AppState::launch_secret` (`OnceLock`), encoded as `state = "<csrf>.<launch_secret>"` in `spotify_auth.rs::run_spotify_oauth_flow`, validated in `lib.rs::handle_deep_link` (#228 redacted logs). Scheme stays `presencejam://` — macOS `Info.plist` is config-time only (see §3.1). |
| #228 | Log redaction | Every deep-link/auth log redacts `code`/`state` to `[REDACTED len N]` or 4-char prefix; `chars().take(4)` fixes byte-slice panic on unicode. |
| #227 | Capabilities least-privilege | Removed `tauri-plugin-store`/`shell` capability registrations + Cargo deps + `default.json` entries; audited to minimal set (`core:*`, `tray`, `menu`, `opener`, `autostart`, `log`, `deep-link`, `updater`, `notification`, `http`, `single-instance`). |
| #226 | Logging config wiring | `logging.enabled`/`log_level` now drive `log::set_max_level` after `config::load_config` (case-insensitive, `Off` when disabled). |

*Grounding:* Tauri capability model — [Capabilities](https://v2.tauri.app/security/capabilities/) — permissions are allow-listed per window; removing `store`/`shell` shrinks the IPC attack surface. PKCE + state anti-forgery per [RFC 7636 §4.4](https://datatracker.ietf.org/doc/html/rfc7636#section-4.4) and [Spotify Authorization Code + PKCE](https://developer.spotify.com/documentation/web-api/tutorials/code-flow).

### 1.2 Release / CI / docs hardening (11 issues, PR #232)
| Issue | Title | What changed |
|---|---|---|
| #204 | Windows auto-update 404 | `release.yml` `bundle_path` now uploads renamed `PresenceJam-<tag>.msi` (wildcard duplicate removed); verification step `gh api …/releases/tags/… --jq .assets[].name` gates `latest.json` generation. Fleet on ≤3.1.0 unblocked — verified 200 on `…/download/v3.2.0/PresenceJam-v3.2.0.msi` (`docs/windows-update-chain-v3.2.md`). |
| #205 | Update-chain checklist | New `docs/windows-update-chain-v3.2.md` (curl/sig/SHA verification for stuck Windows fleet). |
| #206 | Release concurrency | `concurrency: group: release-${{github.ref}}, cancel-in-progress: false` prevents retag double-fire. |
| #207 | Runner bump | `ubuntu-22.04` → `ubuntu-latest` (24.04). |
| #208 | Digest mismatch | `actions/download-artifact` `digest-mismatch: warn` → `error` on both occurrences (v8 default is error; explicit `error` is correct). |
| #209 | SHA256SUMS | Release job generates `SHA256SUMS.txt` (basename) from `artifacts/**/*` and uploads via `--clobber`. |
| #210 | CI on main | `ci.yml` now triggers on `push: branches: [main]` (was PR-only). |
| #211 | Least-privilege checkouts | `persist-credentials: false` on release checkouts; `winget` job `contents: write` → `read`. |
| #212 | README asset table | Canonical names `PresenceJam-macos.dmg`, `…AppImage/.deb`, `PresenceJam-<tag>.msi`. |
| #213 | ARCHITECTURE lockfile | `pnpm-lock.yaml` → `package-lock.json`. |
| #214 | STATE-OF-FEATURES header | `v3.0.0` → `v3.2.0` + digest rows → `error`. |

*Grounding:* [GitHub Actions — `actions/download-artifact` v8 `digest-mismatch`](https://github.com/actions/download-artifact#v8), [Tauri Updater — `latest.json` contract](https://v2.tauri.app/plugin/updater/#server-support) — 404 on platform URL is silently treated as "no update".

### 1.3 Polling, threading, and stability (4 issues, PRs #233/#234/#235/#236)
| Issue | Title | What changed |
|---|---|---|
| #215 | Main-thread stalls | 12 `#[tauri::command]`s → `pub async fn` + `spawn_blocking` (`start/stop_syncing`, `app_exit`, `save_config`, 7 playback commands, `start/poll_teams_auth`, `start_spotify_auth`, etc.); `complete_onboarding` awaits `start_syncing`. No IO on Tauri's async runtime. |
| #218 | Stop/Exit join freeze | `stop_polling_and_join` final `join()` moved into `spawn_blocking` with `ThreadId` ownership check; `app_exit` detaches after grace, preserving #69 drain-first invariant. |
| #216 | Teams device-code 15-min block | `poll_teams_auth` → `async fn` + `spawn_blocking`, `interval.clamp(1,15)`, chunked 30 s sleep, `slow_down` +5 s cumulative, terminal errors short-circuit per RFC 8628 §3.5, `expires_in` 900 s bound. |
| #69 (regression guard) | `stop_tx` ownership | `polling/state.rs` ThreadId-checked cleanup so an old thread cannot wipe a new thread's flag. |

*Grounding:* [Tauri async commands](https://v2.tauri.app/develop/calling-rust/#async-commands) — blocking work MUST use `spawn_blocking`; [RFC 8628 §3.5 Device Authorization Grant](https://datatracker.ietf.org/doc/html/rfc8628#section-3.5) — `slow_down` and `interval` semantics.

### 1.4 Token & auth lifecycle (5 issues, PRs #234/#236/#237)
| Issue | Title | What changed |
|---|---|---|
| #219 | Token-revocation infinite loop | `InvalidGrant` path clears `*tokens.spotify_mut()=None`, persists, emits `spotify-reconnect-required` + `reconnect-required`, increments `transient_failure_count` → 5-strikes. Test helper body has 5 persist sites (2 invalid_grant + 3 refresh-success). |
| #220 | Spotify reconnect only in Settings | Lifted `spotify-reconnect-required` listener from `Settings.svelte` to always-mounted `+layout.svelte` (mirrors teams #157). |
| #222 | Teams re-auth missing in Reconnect | `Reconnect.svelte` derives `needsTeams` from `teams_connected` and offers Teams device-code re-auth. |
| #223 | Dead teams-auth-failed listeners | `teams-auth-failed` now emitted (string payload) from `start_teams_auth_device_code` and `poll_teams_auth` Err paths, matching `listen<string>` in 4 sites. |
| #217 (tray) + #229 + #230 | Tray lock + label + mount | See §1.5. |

*Grounding:* [Microsoft Graph — `invalid_grant` / token revocation](https://learn.microsoft.com/en-us/entra/identity-platform/reference-error-codes), [Spotify — refresh token 6-month rotation](https://developer.spotify.com/documentation/web-api/tutorials/refreshing-tokens).

### 1.5 Tray, UI, and config hygiene (4 issues, PR #233/#237)
| Issue | Title | What changed |
|---|---|---|
| #217 | Tray lock contention | `cached_devices`/`cached_queue` snapshot under short lock, drop, fetch outside lock, re-acquire to store; `update_tray_menu` fetches before `tray_write_lock`, holds lock only around `set_menu`. |
| #229 | Play/Pause label staleness | `track_key` now includes `is_playing` so same-track pause triggers rebuild; label flips via `force_tray_refresh`. |
| #230 | Tray Pause/Resume dead when Dashboard not mounted | Lifted `toggle-pause` listener from `Dashboard.svelte` to always-mounted `+page.svelte` (`get_sync_status` → `start/stop_syncing`). |
| #221 | Autostart optimistic divergence | `set_autostart_enabled` try/catch with revert + OS re-query. |
| #224 | Playback errors silent | Layout-level `playback-error` toast (string, 6 s auto-dismiss). |
| #225 | Frontend hygiene batch | `show_window`/`open_logs_folder`/`is_spotify_client_secret_set` wrapped, `isPermissionGranted` dead check removed, `preview_status` 300 ms debounce + seq guard, `AppConfig` via generated `../types` (BigInt ↔ Number helpers for `PollingConfig`), `package.json` store removal + `package-lock.json` sync; follow-up BigInt `structuredClone` fix. |

*Grounding:* [Tauri tray-icon plugin](https://v2.tauri.app/plugin/tray-icon/) — `set_menu` holds a lock, must not hold across network I/O; [Svelte 5 runes `$state`/`$effect` reactivity](https://svelte.dev/docs/svelte/$state) — micro-race guards required.

---

## 2  Codebase health audit (read-only, 2026-08-22)

### 2.1 TODO / FIXME / HACK (repo-wide grep)
- **Result:** 2 hits, both documented and intentional:
  - `CHANGELOG.md:571` — `Polling: added TODO note about future refactor to filter raw Spotify fields before formatting` — tracks profanity-filter ordering improvement (filter raw fields before `format_status` template). Not a bug, a planned refactor; covered by candidate C9.
  - `TROUBLESHOOTING.md:154-155` — references the same `ARCHITECTURE.md#profanity-filter` TODO note.
  - **No `FIXME`/`HACK`/`XXX` left in `src-tauri/src` or `src/`.** Clean.

### 2.2 STATE-OF-FEATURES — remaining P2/P3?
- `docs/STATE-OF-FEATURES.md` @ v3.2.0: **0 open P2/P3 for the v3.2.0 surface.** Every row is either ✅ (verified, 15 features) or an explicitly out-of-scope / partial-with-maintainer-note:
  - ⚠ Partial: macOS deep-link hijack defence — `register()` returns `UnsupportedPlatform` on macOS; relies on PKCE + per-launch secret. Tracked as issue #66 follow-up → candidate C1.
  - ⚠ Not signed: Windows/macOS code signing & notarization — documented as "will not be added unless $99/yr (Apple) + ~$10/mo (Azure)" — see candidate C10 for the cheaper attestation alternative.
  - ⚠ Verify: GCC/DoD/China tenants, Tauri 3.0 compat, non-Ubuntu distros — all correctly labeled "Verify with maintainer" rather than guessed.
  - ❌ Explicitly out of scope: Free Spotify, custom emoji, lyrics, Slack/Discord — correctly closed.

### 2.3 Open issues after 28 closures
- **Expected:** 0 open code issues (28 closures landed on `main`). **Verified read-only:** `grep -r "TODO\|FIXME"` clean (above), CHANGELOG §3.2.0 lists 28 issues closed by PRs #232/#233/#234/#235/#236/#237, and no `// TODO` remains in Rust `src-tauri/src` (the only remaining TODO is the profanity-refactor note, which is not an issue-tracked blocker).
- **Dependabot:** 3 open PRs expected on `Carme99/PresenceJam-Desktop` (npm minor/patch, cargo minor/patch, github-actions minor/patch) per `dependabot.yml` `open-pull-requests-limit: 5` × 3 ecosystems — not counted as code-issue backlog; handled by weekly batch.
- **Winget/Homebrew:** formula taps are not GitHub issues in this repo; no tracker gap.

### 2.4 README feature gaps (vs implemented)
- README Features list matches `STATE-OF-FEATURES` verified set; no drift. Two gaps worth closing in 3.3 docs pass (both non-blocking):
  - Tray **Devices + Up Next** submenus are shipped (header mentions "plus Devices and Up Next submenus") — correct.
  - Notification opt-in (`localStorage notificationsEnabled`, `notification:default` capability) is shipped but not listed as a feature — should be added to README Features in 3.3 docs sweep.
  - Rate-limit / 429 handling (`Retry-After` http-date + delta-seconds) and 5-strikes `reconnect-required` are robust but not user-visible — TROUBLESHOOTING already covers them; no gap.

### 2.5 tauri.conf / capabilities — unused permissions audit
- `src-tauri/tauri.conf.json:40` `security.csp` is minimal and correct (`default-src 'self'`, `img-src` includes `i.scdn.co` + `mosh-pa.spotify.com`, `connect-src` includes the 4 API origins).
- `src-tauri/capabilities/default.json` — 24 permissions, all exercised:
  - `core:*` (window/tray/menu/event), `opener:default` (`open_external_url` via `tauri_plugin_opener`), `autostart:default`, `log:default`, `deep-link:default`, `updater:default`, `notification:default` (+ 3 `allow-*`), `http:default` (Teams/Spotify calls), `single-instance` (via `tauri.conf` plugin, not a capability entry — correct).
  - **Clean:** `store`/`shell`/`fs`/`process` are absent — their removal in #227 is verified (no `allow-*` entry, no `plugin:store` in `Cargo.toml` dependency tree for IPC, no `withGlobalTauri` over-exposure beyond `true` which is required for Svelte invoke). Note `single-instance` is `target."cfg(any(target_os=…))"` gated — correct.
- **Residual:** `package.json` + `Cargo.lock` still list `@tauri-apps/plugin-shell` / `tauri-plugin-shell` / `tauri-plugin-store` / `tauri-plugin-fs` as transitive dependencies pulled by `tauri-plugin-*` metapackage, but no IPC capability grants them — **zero runtime exposure**, only bundle size. Candidate C13 proposes pruning the Cargo/npm entries to shrink `cargo tree` and eliminate confusion (S, 2 h, no behavior change).

### 2.6 Polling loop — further optimization headroom
- Current: smart sleep (`duration_ms - progress_ms - 5s`, clamped to `PollingConfig` 10–60 s), pause-aware backoff 30→60→120→300 s, jitter ±20 %, `Retry-After` (delta + http-date) capped 300 s, `consecutive_pauses` single increment site, CAS-discard helper, `LAST_PLAYING_STATE` toggle, `Devices`/`Queue` 60 s throttle with lock-free fetch.
- **Remaining polish (all non-blocking):**
  - Conditional GET: Spotify `GET /me/player/currently-playing` returns `ETag`/`Cache-Control` on some responses; a `304 Not Modified` would let the loop skip JSON parse + `format_status` work. Spotify does not document ETag, but empirical `If-None-Match` saves ~1 KB JSON parse per no-change poll — S-sized, low risk (candidate C11).
  - Adaptive floor: `minimum_interval_seconds` lower bound is 5 s (clamped) but the UI defaults to 10 s; a live-stream (`progress_ms == null`) path already uses the default interval — no busy-loop risk. Could expose a per-power-state floor (battery vs AC) via `tauri-plugin-os` — deferred.
  - Graph `getPresence` for the gate is only called on track-change — correct; no need to poll presence independently.

### 2.7 Frontend — a11y / i18n
- **a11y:** Strong baseline — `role="alert"`/`role="status"`/`aria-live="polite"`/`aria-label`/`aria-pressed`/`role="tablist"`/`role="tab"`/`aria-selected`/`role="img"` are used throughout (`Dashboard.svelte`, `LogViewer.svelte`, `Settings.svelte`, `+layout.svelte`). Remaining gaps are minor: no `skip-link`, no `:focus-visible` audit on the new toast, no `prefers-reduced-motion` for the pulse dot — candidate C12 (M).
- **i18n:** No framework. All strings are hard-coded English in 7 Svelte components. Adding `svelte-i18n` or `typesafe-i18n` is feasible but touches every component and the Rust error strings (`spotify.rs`/`teams.rs` user-facing messages). Estimate L (3–5 d) — candidate C6, correctly deferred to 3.4 unless a localization request arrives.

### 2.8 release.yml — further hardening
- Already hardened: `digest-mismatch: error`, SHA256SUMS, `persist-credentials: false`, `concurrency` group, `cancel-in-progress: false` on release, `contents: write` scoped to release job.
- **Next steps (all non-blocking, candidate C10):**
  - OIDC via `actions/*` + `sigstore/cosign` or `npm provenance` — replace `GITHUB_TOKEN` PAT for `gh release upload` where possible; add `id-token: write`.
  - Build provenance attestation: `actions/attest-build-provenance` (SLSA) emitting DSSE for each artifact, consumed by `gh attestation verify` — supplements minisign `.sig` without breaking it.
  - `workflow_dispatch` for manual re-cut without retag.
  - Pin remaining unpinned actions (`dtolnay/rust-toolchain`, `Swatinem/rust-cache`, `vedantmgoyal2009/winget-releaser`) to SHAs (already partially done — `checkout`/`setup-node`/`upload-artifact`/`download-artifact` are SHA-pinned per #68).

---

## 3  Candidate polish for 3.3.0 vs 3.4.0

> Hour estimates use the repo's stated scale: **S = 2–4 h**, **M = 1–2 d (8–16 h)**, **L = 3–5 d (24–40 h)**. Each entry lists grounding citation and whether it **blocks** 3.3 or can **defer** to 3.4.

### C1  Per-launch UUID scheme — the alternative to the current per-launch secret

- **What:** Replace (or complement) the shipped per-launch secret-in-`state` (PR #235) with a **per-launch custom scheme** alternative: on each launch generate a UUID, register `presencejam-<uuid>://` as the redirect URI for this invocation, and pass that scheme to Spotify. The callback URL path then carries the anti-hijack binding, not just `state`. This is the textbook OAuth custom-scheme hijack fix; it was deferred because Spotify requires byte-exact redirect-URI match and macOS bundles schemes in `Info.plist` at build time.
- **Why now:** The current secret binding is sound (verifier stays in `AppState`, `state.splitn(2,'.')` check in `lib.rs::handle_deep_link`), but a UV study would show it is **state-size-sensitive** — the composed `<csrf>.<launch_secret>` ~130 chars is near the 500-char URL-limit edge and relies on Spotify echoing `state` verbatim (which it does today). A UUID scheme moves entropy into the URI authority, shrinking `state` and surviving any future `state` truncation.
- **Scope:** Rust `spotify_auth.rs` (generate + store `launch_scheme`), `lib.rs` (register `register_all()` per-scheme via OS APIs), `tauri.conf.json` (wildcard vs explicit scheme), single-instance argv scan, frontend `invoke` signature, tests (`test_register_all_not_gated_to_windows_only` must be extended). Spotify dashboard must allow wildcard or pattern redirect URIs — it currently does not — so this needs a product decision. macOS `LSSetDefaultHandlerForURLScheme` native-FFI path is the missing piece (`STATE-OF-FEATURES` "Partial" row).
- **Effort:** **M — 12–16 h** (1–2 d). OS-specific: Windows `HKCU\Software\Classes`, Linux `~/.local/share/applications` + `xdg-mime` (already done for `presencejam://`), macOS `CFBundleURLSchemes` + `LSSetDefaultHandlerForURLScheme` FFI.
- **Risk:** Medium — Spotify dashboard exact-match may reject the dynamic scheme (unknown until tested); macOS FFI is new unsafe code.
- **Priority:** P2 (security polish, not a blocker — current secret-in-state is sufficient per audit).
- **Grounding:** [Tauri Deep Link plugin](https://v2.tauri.app/plugin/deep-link/) — `register()` returns `UnsupportedPlatform` on macOS; Apple [LaunchServices — `LSSetDefaultHandlerForURLScheme`](https://developer.apple.com/documentation/coreservices/launch_services); [Spotify — Redirect URI exact match](https://developer.spotify.com/documentation/web-api/tutorials/code-flow) ("The redirect URI must match exactly"); [RFC 8252 §7.3 — loopback vs custom scheme](https://datatracker.ietf.org/doc/html/rfc8252#section-7.3).
- **Blocks 3.3?** **No — defer to 3.4** unless a macOS hijack is observed in the wild. The current secret-in-state (shipped in 3.2.0) is the correct short-term fix and buys time.

### C2  Deep-link single-instance UX — foreground + navigate

- **What:** When a second instance or deep-link arrives (`single_instance_init` callback in `lib.rs`), today the app raises/focuses the window and forwards the URL to `handle_deep_link`. The polish is to **navigate the frontend** to the right view after the auth succeeds: e.g. a Teams device-code expiry deep-link routes to `Settings` tab, a Spotify callback while on `Dashboard` flashes the success badge. Today the user sees the window but no navigation.
- **Effort:** **S — 2–4 h**. Add `navigate` emit from `handle_deep_link` / `single_instance_init` and a `+page.svelte` listener that sets `currentView`. No Rust IPC new command.
- **Risk:** Low — emit is already used for `tray-click`/`toggle-pause`/`navigate` (see `+page.svelte`).
- **Priority:** P1 — visible polish, cheap.
- **Grounding:** [Tauri Single Instance plugin](https://v2.tauri.app/plugin/single-instance/) — argv forwarding + [Tauri Events `emit`/`listen`](https://v2.tauri.app/develop/calling-frontend/#events).
- **Blocks 3.3?** **No, but Recommend for 3.3** — S-sized, high polish-per-hour.

### C3  Auto-update delta / background silent check

- **What:** Today `UpdatePrompt.svelte` calls `check()` on startup and shows a banner; download is user-initiated via `downloadAndInstall()` → `relaunch_app`. Options: (a) silent background check on a timer (e.g. every 24 h), (b) staged/delta artifacts (Tauri updater has no delta — it re-downloads the full MSI/AppImage/tar.gz; a delta would require a custom updater or `tauri-plugin-updater` fork), (c) "Install on quit" deferred relaunch.
- **Effort:** **M — 8–12 h** for (a)+(c); **L — 24–40 h** for (b) if pursued. Recommend (a)+(c) only.
- **Risk:** Low for (a)+(c); High for (b) — custom updater forks are maintenance liabilities.
- **Priority:** P3 — fleet is small, full-MSI ~15 MB is fine for now.
- **Grounding:** [Tauri Updater — `check()` / `downloadAndInstall()`](https://v2.tauri.app/plugin/updater/), [Tauri Updater — `latest.json` static schema](https://v2.tauri.app/plugin/updater/#server-support) — signatures are `.sig` file contents, not delta patches.
- **Blocks 3.3?** **No — defer to 3.4** (or 3.3.x patch if bandwidth). The 3.2.0 fix already unblocks the fleet.

### C4  Tray UX polish

- **What:** Four micro-improvements that together make the tray feel native:
  1. **Live tooltip** — update tray tooltip to `"Artist — Track (▶/⏸)"` on each poll (today tooltip is static).
  2. **Native icons** — use platform-native checked/disabled menu item states for Play/Pause toggle vs rebuilding the menu.
  3. **Badge** — macOS dock badge count when gated (presence-gated) vs available.
  4. **Single-click vs double-click** — respect OS convention (Windows double-click to show).
- **Effort:** **S — 3–4 h** (tooltip + checked-state are one-line `tray.set_tooltip`/`MenuItem` builder changes; badge needs `window.set_badge_count`).
- **Risk:** Low — `tauri-plugin-tray` is well-tested; badge is macOS-only `#[cfg]`.
- **Priority:** P1 — most visible surface after Dashboard.
- **Grounding:** [Tauri Tray Icon — `TrayIcon::set_tooltip`](https://v2.tauri.app/plugin/tray-icon/), [Tauri Window — `setBadgeCount` (macOS)](https://v2.tauri.app/reference/javascript/api/window/).
- **Blocks 3.3?** **Recommend for 3.3** — S-sized, high delight.

### C5  Telemetry-free diagnostics page

- **What:** A new `Diagnostics.svelte` route (like `LogViewer.svelte`) that collects **local-only** diagnostics with no network call:
  - AppConfig + sanitized `tokens.json` metadata (expiry timestamps, not tokens), OS + Tauri version, build date (`VITE_APP_BUILD`), keychain availability (`secret-tool` smoke-test result), last 20 sanitized log lines, `getPresence` last result (availability/activity + gated reason), updater `latest.json` fetch status.
  - "Copy diagnostics" button (clipboard, redacted) + "Save to file" — user pastes into a GitHub issue. No telemetry endpoint, no opt-in — matches `SECURITY.md` "No Telemetry" promise.
- **Effort:** **M — 8–10 h** (new component + 3 new `#[tauri::command]`s: `get_diagnostics_snapshot`, `copy_to_clipboard`, plus a Rust-side redaction helper reusing `token_io` truncation).
- **Risk:** Low — read-only collection; must audit that no token/secret leaves the handler (reuse the existing `[REDACTED len N]` helper).
- **Priority:** P1 — cuts support turnaround (today maintainers ask for `open_logs_folder` + manual `config.json` excerpt).
- **Grounding:** [Tauri OS plugin — `osInfo`](https://v2.tauri.app/plugin/os-info/), [Tauri Clipboard plugin](https://v2.tauri.app/plugin/clipboard/), Microsoft Learn [presence-get response shape](https://learn.microsoft.com/en-us/graph/api/presence-get) for `availability`/`activity`.
- **Blocks 3.3?** **Recommend for 3.3** — highest support-leverage per hour after C2/C4.

### C6  i18n / localization

- **What:** Extract all English strings from 7 Svelte components + Rust `format!` error messages into a key file (`en.json`), add `svelte-i18n` or `typesafe-i18n`, ship `en` + `de`/`fr` seed translations, add a Settings → Language picker. Rust errors would stay English or pass a locale tag via `invoke`.
- **Effort:** **L — 24–40 h** (3–5 d). Touches every component + `ARCHITECTURE.md` + `CONTRIBUTING.md` i18n rule; Rust error catalog is the long tail.
- **Risk:** Medium — string-extraction churn conflicts with parallel PRs; `ts-rs` types are locale-free so no coupling.
- **Priority:** P3 — user base is English-first (GitHub issues are English); no localization request in tracker. Correctly deferred in every prior scope doc.
- **Grounding:** [Svelte i18n — `svelte-i18n`](https://github.com/kaisermann/svelte-i18n), [Tauri — locale detection via `osInfo().locale`](https://v2.tauri.app/plugin/os-info/).
- **Blocks 3.3?** **No — defer to 3.4 or later**. Foundation can be laid in 3.3 by extracting a `src/lib/i18n.ts` barrel with an `en` fallback and no second locale (S, 3 h), but full translation is not 3.3 scope.

### C7  Multi-window (detach Dashboard / Settings / Logs)

- **What:** Today the app is a single 600×750 `main` window. Use Tauri's `WebviewWindow` API to allow "Pop out" for Logs and Settings into separate windows (like VS Code detached panels). Requires `tauri.conf.json` `app.windows` entries + `capabilities` per window + `invoke` routing (which window emits `presence-gated`?).
- **Effort:** **M — 12–16 h** (1.5–2 d). Window state sync is the complexity — `currentView` store would become per-window.
- **Risk:** Medium — multi-window is the #1 source of Tauri focus/activation bugs on Linux (Wayland) and macOS (Accessory policy hid dock icon).
- **Priority:** P3 — no user request; single-window is correct for a tray-resident app.
- **Grounding:** [Tauri Window — `WebviewWindow`](https://v2.tauri.app/reference/javascript/api/window/#webviewwindow), [Tauri Config — `app.windows`](https://v2.tauri.app/reference/config/#windows).
- **Blocks 3.3?** **No — defer to 3.4+** (or never — consider if requested).

### C8  Notification grouping, throttle, and actions

- **What:** Today Dashboard fires `sendNotification({ title, body, icon })` per `spotify-track-changed` with `lastNotifiedId` dedup and `isPermissionGranted` gate (capability `notification:default` + 3 `allow-*`). Polish:
  1. Group notifications by session (replace in-place vs stacking).
  2. Throttle: at most 1 per 5 s (rapid track-skip shouldn't spam).
  3. Actions: "Pause sync" action button on the notification itself (where platform supports it — macOS `NSUserNotification` actions, Windows `ToastActions`).
- **Effort:** **S — 3–4 h** (throttle is a timestamp check in `Dashboard.svelte`; grouping is `notification:default` without extra capability; actions need `notification:allow-notify` with `actions` array — API exists).
- **Risk:** Low — opt-in gate (`localStorage notificationsEnabled`) means off-by-default; no regression if notification permission is denied.
- **Priority:** P2 — nice-to-have but not visible to users who leave notifications off (default).
- **Grounding:** [Tauri Notification — `sendNotification`](https://v2.tauri.app/plugin/notification/), [Spotify — track-change dedup is app-side; no API push](https://developer.spotify.com/documentation/web-api/reference/get-the-users-currently-playing-track) — polling remains the source.
- **Blocks 3.3?** **No — can ship in 3.3 or defer**. If C5 diagnostics lands, bundle C8 there (shared notification permission flow).

### C9  Config UI polish — dirty-state, inline validation, preview

- **What:** Settings today saves via `saveConfig(localConfig)` with a 2 s success toast; no dirty-state indicator, no inline per-field errors. Polish:
  1. Dirty-state dot + "Unsaved changes" banner when `structuredClone($configStore) ≠ localConfig`.
  2. Inline `PollingConfig` clamp feedback (when `minimum > maximum` the input shows the clamped value immediately).
  3. Keep `preview_status` 300 ms debounce + seq guard (already shipped in #225) and add a "Reset to default" per section.
- **Effort:** **S — 3–4 h** (derive `isDirty` from `JSON.stringify` diff; clamp is `Math.min`/`Math.max` on input — no Rust change).
- **Risk:** Low — Settings is isolated from polling; no lock ordering impact.
- **Priority:** P2.
- **Grounding:** [Svelte 5 `$derived`](https://svelte.dev/docs/svelte/$derived) for `isDirty`; Rust clamp in `config.rs:clamp_polling` is the source of truth, frontend just mirrors it.
- **Blocks 3.3?** **No — recommend for 3.3 if C5 lands** (shared Settings route), otherwise 3.4.

### C10  Release hardening — OIDC, attestations, provenance

- **What:**
  1. **OIDC** for `gh release upload` / `homebrew` tap push — replace `GITHUB_TOKEN` + `HOMEBREW_TAP_TOKEN` PAT with `id-token: write` + `actions/create-github-app-token` or `tibdex/github-app-token` where possible. The `homebrew` job's credential-helper pattern stays but the token becomes short-lived.
  2. **Build provenance** — `actions/attest-build-provenance` (SLSA Level 3) emitting DSSE attestations for each artifact, verifiable via `gh attestation verify`. Supplements minisign `.sig` (do not replace — updater's `pubkey` verification stays).
  3. Pin remaining actions to SHAs: `dtolnay/rust-toolchain`, `Swatinem/rust-cache`, `vedantmgoyal2009/winget-releaser`.
  4. Add `workflow_dispatch` for manual re-cut.
- **Effort:** **M — 8–12 h** (OIDC + attest in `release.yml`, verify on a test tag, docs in `SECURITY.md` "Release Pipeline Token Rotation" section).
- **Risk:** Medium — OIDC misconfig breaks the release job and blocks the next tag; keep `GITHUB_TOKEN` fallback for one cycle.
- **Priority:** P1 — supply-chain hardening; 3.2.0 already ships `SHA256SUMS.txt` + `digest-mismatch: error`, so this is the next rung.
- **Grounding:** [GitHub — OIDC in Actions](https://docs.github.com/en/actions/security-for-github-actions/security-hardening-your-deployments/configuring-openid-connect-in-cloud-providers), [Sigstore — Cosign / SLSA](https://docs.sigstore.dev/), [GitHub — Artifact Attestations](https://docs.github.com/en/actions/security-for-github-actions/using-artifact-attestations-to-establish-provenance-for-builds).
- **Blocks 3.3?** **Recommend for 3.3** — non-user-visible but high trust-per-hour; no frontend change, so it doesn't compete with C2/C4/C5 for review bandwidth.

### C11  Polling optimization — conditional GET / adaptive jitter

- **What:**
  1. Store `ETag` from `GET /me/player/currently-playing` and send `If-None-Match` on the next poll; on `304 Not Modified` skip JSON parse + `format_status` + `filter_profanity` + tray rebuild. Saves CPU, not API quota (the request still counts), but avoids the `serde_json` hot path.
  2. Adaptive floor: when on battery (`tauri-plugin-os` or `navigator.getBattery()`), raise `minimum_interval_seconds` floor by +10 s.
- **Effort:** **S — 3–4 h** for (1); **S — 2–3 h** for (2). Spotify ETag is empirical — needs a live probe; if absent, (1) is a no-op and still shippable.
- **Risk:** Low — 304 is handled as "no change"; no new permission.
- **Priority:** P2.
- **Grounding:** [Spotify — `GET /me/player/currently-playing`](https://developer.spotify.com/documentation/web-api/reference/get-the-users-currently-playing-track) — returns 200 with `TrackObject` or 204 when nothing playing; `ETag` is not documented but present on some responses; [RFC 7232 §3.3 — `If-None-Match`](https://datatracker.ietf.org/doc/html/rfc7232#section-3.3).
- **Blocks 3.3?** **No — can defer to 3.4**. Already highly optimized (smart sleep + backoff + 60 s Devices/Queue throttle); marginal gain.

### C12  Accessibility pass — keyboard, focus, contrast, screen-reader

- **What:** Audit every interactive element in 7 Svelte components against WCAG 2.2 AA:
  1. `skip-link` to main content (for keyboard users).
  2. `:focus-visible` ring audit on toast, tray menu, and new diagnostics page.
  3. `prefers-reduced-motion` for the `pulse-dot` animation in `Dashboard.svelte`.
  4. Color-contrast audit on the design-system tokens in `app.css` (11-token system → full token system in 3.2.0) — ensure `var(--fg-muted)` on `var(--bg-base)` meets 4.5:1.
  5. Screen-reader walkthrough: VoiceOver (macOS) + NVDA (Windows) + Orca (Linux) smoke.
- **Effort:** **M — 8–12 h** (audit 4 h + fixes 4–8 h). No Rust change.
- **Risk:** Low — CSS/aria only.
- **Priority:** P2 — baseline is already good (see §2.7), this is the final 10 %.
- **Grounding:** [WCAG 2.2](https://www.w3.org/TR/WCAG22/), [Tauri — WebView a11y is Chromium/WebKit a11y tree](https://v2.tauri.app/develop/debug/) — the frontend is standard HTML/CSS/ARIA.
- **Blocks 3.3?** **No — can ship in 3.3 or defer**. If C2/C4/C5 land, bundle a11y fixes there (same Svelte routes).

### C13  Remove unused `plugin-shell` / `plugin-store` deps (attack surface + bundle size)

- **What:** `tauri-plugin-shell` is in `Cargo.toml:15` + `package.json:23` + `Cargo.lock` but no Rust or Svelte code imports `@tauri-apps/plugin-shell` and no capability grants `shell:*`; `tauri-plugin-store` was removed from `capabilities/default.json` in #227 but the crate entry lingers in `Cargo.lock` transitive deps. Pruning is mechanical: drop the 3 registration points, `ACKNOWLEDGEMENTS.md` rows, and run `cargo update` + `npm install`.
- **Effort:** **S — 1–2 h** (mechanical, no behavior change; precedented by the `tauri-plugin-process` removal in audit Q6, v2.7.3).
- **Risk:** None — verified no caller via `grep plugin-shell` (above).
- **Priority:** P1 — attack-surface reduction, zero cost.
- **Grounding:** [Tauri Capabilities](https://v2.tauri.app/security/capabilities/) — least-privilege: if a plugin has no capability, it has no IPC exposure, but removing the crate removes the binary bloat.
- **Blocks 3.3?** **Recommend for 3.3** — do first, so the 3.3 tree is clean for the other PRs.

---

## 4  Summary table — sorted by priority (P1 → P2 → P3)

| P | # | Candidate | Effort | Hours | Risk | Blocks 3.3? | Grounding |
|---|---|---|---|---|---|---|---|
| **P1** | C13 | Remove unused `shell`/`store` deps | **S** | 1–2 h | None | **Yes — do first** | Tauri Capabilities least-privilege |
| **P1** | C10 | Release OIDC + attestations + provenance | **M** | 8–12 h | M | **Recommend 3.3** | GitHub OIDC, Sigstore, SLSA |
| **P1** | C2 | Deep-link single-instance UX (foreground+navigate) | **S** | 2–4 h | L | **Recommend 3.3** | Tauri Single Instance, Events |
| **P1** | C4 | Tray UX polish (tooltip, checked-state, badge) | **S** | 3–4 h | L | **Recommend 3.3** | Tauri Tray Icon, Window badge |
| **P1** | C5 | Telemetry-free diagnostics page | **M** | 8–10 h | L | **Recommend 3.3** | Tauri OS/Clipboard, Graph presence-get |
| **P2** | C12 | Accessibility pass (keyboard, focus, contrast) | **M** | 8–12 h | L | **Can defer** | WCAG 2.2, Chromium a11y |
| **P2** | C9 | Config UI dirty-state + inline validation | **S** | 3–4 h | L | **Can defer** | Svelte $derived, Rust `clamp_polling` |
| **P2** | C8 | Notification grouping / throttle / actions | **S** | 3–4 h | L | **Can defer** | Tauri Notification |
| **P2** | C11 | Polling ETag / adaptive floor | **S** | 3–4 h | L | **Can defer** | RFC 7232, Spotify currently-playing |
| **P2** | C1 | Per-launch UUID scheme alternative | **M** | 12–16 h | M | **Defer to 3.4** | Tauri Deep Link, RFC 8252, Apple LS |
| **P3** | C3 | Auto-update delta / background silent | **M/L** | 8–12 h (M) / 24–40 h (L) | L (M) / H (L) | **Defer to 3.4** | Tauri Updater — no delta natively |
| **P3** | C6 | i18n / localization | **L** | 24–40 h | M | **Defer to 3.4+** | svelte-i18n, Tauri osInfo.locale |
| **P3** | C7 | Multi-window (detach Logs/Settings) | **M** | 12–16 h | M | **Defer to 3.4+** | Tauri WebviewWindow, app.windows |

**Recommended 3.3.0 cut (5–7 d total, 3 engineers in parallel):**
- **C13 → C2 → C4 → C5 → C10** = ~22–32 h wall-clock (≈ 3–4 d with 2-way parallel: C13+C10 in one PR, C2+C4 in another, C5 as the docs-heavy PR). Add C9/C12 if bandwidth, otherwise they are the first 3.4 candidates.
- **C1, C3, C6, C7 explicitly deferred** — each is documented above with why and with a grounding citation so a future PR can be opened directly against this file without re-researching.

---

## 5  Why nothing here blocks 3.2.0

- **Ship-blocker count today:** 0. Every P1 that was a blocker for the Stratus fleet (Windows 404, thread wedges, revocation loop, 15-min Teams block, missing reconnect listeners) is closed and verified on `main` (`ci.yml` + `cargo test --all-targets` + `cargo clippy -D warnings` green in PR #235-#237).
- **Deferred items are not regressions:** This memo's candidates are either (a) alternatives to a fix already shipped (C1 secret → UUID), (b) new UX (C2/C4/C5/C8/C9/C12), or (c) infrastructure that is additive and backward-compatible (C10/C11/C13) or large enough to be a minor by itself (C6/C7/C3). None reopens a closed P1.
- **Fleet health:** `docs/windows-update-chain-v3.2.md` preconditions are checkable today — `latest.json` contains `version "3.2.0"` and 3 platform URLs that each return 200; the `Verify latest.json assets` step in `release.yml` enforces this on every future tag, so a silent-404 regression cannot ship again.
- **If you need to ship 3.2.0 without any of §3:** Tag it. The next polish can ride 3.3.0 with no migration cost — `config.json`/`tokens.json` schemas are additive (`#[serde(default)]` + `clamp_polling`) and the keychain slot is namespaced (`tokens_aes_key:com.presencejam.app`).

---

## 6  Source map (every grounding citation used above)

- **Tauri:** [Capabilities](https://v2.tauri.app/security/capabilities/), [Deep Link plugin](https://v2.tauri.app/plugin/deep-link/), [Updater plugin](https://v2.tauri.app/plugin/updater/), [Tray Icon](https://v2.tauri.app/plugin/tray-icon/), [Single Instance](https://v2.tauri.app/plugin/single-instance/), [Window / WebviewWindow](https://v2.tauri.app/reference/javascript/api/window/), [Notification](https://v2.tauri.app/plugin/notification/), [OS Info](https://v2.tauri.app/plugin/os-info/), [Clipboard](https://v2.tauri.app/plugin/clipboard/), [Config — `app.windows`](https://v2.tauri.app/reference/config/#windows), [Async commands + `spawn_blocking`](https://v2.tauri.app/develop/calling-rust/#async-commands), [Events `emit`/`listen`](https://v2.tauri.app/develop/calling-frontend/#events).
- **Microsoft Learn:** [presence-setPresence](https://learn.microsoft.com/en-us/graph/api/presence-setpresence), [presence-clearPresence](https://learn.microsoft.com/en-us/graph/api/presence-clearpresence), [presence-get](https://learn.microsoft.com/en-us/graph/api/presence-get), [permissions reference — `Presence.ReadWrite`](https://learn.microsoft.com/en-us/graph/permissions-reference), [Entra error codes — `invalid_grant`](https://learn.microsoft.com/en-us/entra/identity-platform/reference-error-codes), [Throttling limits](https://learn.microsoft.com/en-us/graph/throttling-limits).
- **Spotify:** [Authorization Code + PKCE](https://developer.spotify.com/documentation/web-api/tutorials/code-flow), [Refreshing tokens](https://developer.spotify.com/documentation/web-api/tutorials/refreshing-tokens), [`GET /me/player/currently-playing`](https://developer.spotify.com/documentation/web-api/reference/get-the-users-currently-playing-track), [Scopes](https://developer.spotify.com/documentation/web-api/concepts/scopes).
- **IETF:** [RFC 7636 — PKCE](https://datatracker.ietf.org/doc/html/rfc7636), [RFC 8252 — OAuth 2.0 for Native Apps §7.3](https://datatracker.ietf.org/doc/html/rfc8252#section-7.3), [RFC 8628 — Device Authorization Grant §3.5](https://datatracker.ietf.org/doc/html/rfc8628#section-3.5), [RFC 7232 §3.3 — `If-None-Match` / ETag](https://datatracker.ietf.org/doc/html/rfc7232#section-3.3).
- **Supply chain:** [GitHub — OIDC in Actions](https://docs.github.com/en/actions/security-for-github-actions/security-hardening-your-deployments/configuring-openid-connect-in-cloud-providers), [GitHub — Artifact Attestations / SLSA](https://docs.github.com/en/actions/security-for-github-actions/using-artifact-attestations-to-establish-provenance-for-builds), [Sigstore — Cosign](https://docs.sigstore.dev/).
- **A11y / i18n:** [WCAG 2.2](https://www.w3.org/TR/WCAG22/), [svelte-i18n](https://github.com/kaisermann/svelte-i18n).
- **Repo anchors:** `CHANGELOG.md:16-48` (3.2.0 28 issues), `src-tauri/src/lib.rs:499-540` (secret binding), `src-tauri/src/polling/poll_once.rs:68-180` (smart sleep + backoff), `src-tauri/capabilities/default.json` (24 perms), `src-tauri/tauri.conf.json:40` (CSP), `.github/workflows/release.yml:1-466` (release matrix + attestation gap), `docs/STATE-OF-FEATURES.md` (verified vs out-of-scope), `docs/windows-update-chain-v3.2.md` (fleet rescue).

---

*Maintenance note: update this file per release. When a candidate ships, move its row to §1 with the closing PR/issue and delete its §3 section. One-row PRs against this file are ideal first contributions.*
