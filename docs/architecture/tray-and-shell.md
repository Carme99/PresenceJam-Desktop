# Tray and shell

> Native surface: the system tray menu, detached windows, UI languages, and the background auto-updater.
>
> Part of the architecture docs — start at the [architecture index](../../ARCHITECTURE.md).

## Multi-Window Detach (v4.0)

Logs and Settings can each be *popped out* into their own window (and popped
back in), VS Code detached-panel style:

- **Creation is Rust-side:** `src/lib/stores/detach.ts` invokes the
  `detach_pane` command; `src-tauri/src/lib.rs` matches a fixed
  `DetachedPaneSpec` table for the stable labels `logs-detached` /
  `settings-detached`, their `/detached/<pane>` URLs, and their window sizes.
  A SvelteKit route (`src/routes/detached/[pane]/+page.svelte`) renders
  `LogViewer` or `Settings` in detached mode. `tauri.conf.json`'s `app.windows`
  is untouched — the app still boots single-window.
- **Main window stays the source of truth:** `currentView` remains
  main-window-only. Detached panes read and write the same app-global state —
  they call `save_config` / `load_config`, `reconnect_spotify` /
  `reconnect_teams`, `poll_teams_auth` and `open_logs_folder` directly, none
  of which take a `window` argument — so config/polling state is shared by
  construction. The 13 commands that assume the main window (auth *starts*,
  the token refreshes, `relaunch_app`, `app_exit`, `stage_deferred_update`,
  `start_syncing` / `stop_syncing`, …) are rejected by
  `require_main_window` (issue #241). That does not strand a detached user:
  `reconnect_spotify` / `reconnect_teams` are unguarded and emit
  `*-reconnect-required` app-wide, and `+layout.svelte` registers its
  reconnect/update listeners **only** in the main window
  (`if (!isMainWindow) return`), so the main window drives the guarded
  re-auth steps. Popping back in re-mounts Settings via `loadConfig()`
  (backend truth). `src/lib/stores/detach.ts` tracks pane→popped-out state in
  the main window only; dashboard nav shows a dot badge and focuses the child
  instead of navigating while detached.
- **Capabilities:** `src-tauri/capabilities/detached.json` scopes the two
  child labels to a minimal mirrored set (`core/event/log/opener/notification`),
  plus `core:window:allow-close` (#594) — the one window-management permission a
  detached render reaches, because **Pop back in** is `popIn()` →
  `WebviewWindow.close()`, which Tauri resolves against the *calling* webview's
  ACL, and `core:window:default` does not include it. Without that explicit entry
  the close is rejected and the pane is marked not-detached while its window stays
  on screen; a refused close now leaves the badge alone instead of lying
  (`detach.ts::popIn`, with `reconcileDetachedPanes()` on boot). The main
  `default.json` capability has no `core:window:allow-create` or
  `core:webview:allow-create-webview-window` grant; Rust owns window creation.
- **Listener hygiene:** `+layout.svelte` guards its always-mounted
  reconnect/auth/update listeners (and `UpdatePrompt`) behind a window-label
  check so detached windows never double-register handlers.

This stays opt-in per-pane detachment, coexisting with the single-window
rationale for a tray-resident app: no second window at boot, tray
`show_window` targets only `main`, and deep-link/single-instance routing is
unchanged.

### Interface Languages (v4.0)

The UI is localized to **English, German, and French** via the i18n barrel
(`src/lib/i18n.ts` re-exporting `src/lib/i18n/{en,de,fr}.ts` +
`store.svelte.ts`):

- Components import `{ t, i18n }` from `$lib/i18n`. `t(key, params)` resolves
  against the active locale's dictionary (English fallback) and substitutes
  `{name}` placeholders; reading `t(...)` in a template tracks `i18n.locale`,
  so switching locales re-renders reactively.
- All three dictionaries are typed against `Dict = keyof typeof en`, so a
  key present in `en.ts` but missing from `de.ts`/`fr.ts` is a TypeScript
  compile error — translation parity is enforced, not convention.
- **4.7.0 (issue #674): `config.locale` is the source of truth.** The picker
  writes `AppConfig.locale` through the `set_locale` command (which also
  relabels the tray and the native application menu without a restart).
  `localStorage` under `locale` survives only as a pre-paint mirror — it is read
  at module load, because the config arrives over an async IPC round-trip and
  the first frame must already be in the right language — it still converges
  detached windows through the `storage` listener below, and a value found
  there while the config carries none is migrated into the config once. First
  run still defaults to the browser language (`de`/`fr` prefixes), falling back
  to English. The picker lives in Settings → Appearance (there is no General card).
- **Intl formatting (4.6):** the locale's `Intl.NumberFormat` and
  `Intl.PluralRules` are built once per locale and reused (constructing a
  formatter per render would dominate `t()`). Numeric params go through the
  number formatter, and `tCount(key, count)` selects the `${key}_one` /
  `${key}_other` entry from the CLDR category — not from `count === 1`, because
  French puts `0` in `one` ("0 entrée") (#616).
- **`<html lang>` + convergence (4.6):** `app.html` ships the pre-hydration
  `lang="en"` and `applyDocumentLang` retags `<html lang>` on boot and on every
  switch, so screen-reader pronunciation and `:lang()` styling follow the picker.
  Detached Logs/Settings windows own independent locale instances, so a `storage`
  listener converges them on the main window's write (the same pattern as the
  `#423` theme listener, with a same-value guard that stops a write loop) (#620).
- **Native surfaces (4.7.0, #674):** the tray menu (`tray.rs`) and the native
  application menu (`menu.rs`) render from a Rust string table
  (`src-tauri/src/i18n.rs`: one `Strings` field per literal, with `EN`/`DE`/`FR`
  tables). An unknown `locale` falls back to English and is logged, and a Rust
  parity test fails when the three tables drift apart or a label is hard-coded
  back into `tray.rs`/`menu.rs`.
- Known limitation: Rust-side error strings surfaced through `invoke()`
  rejections and event payloads remain English, as does the app name.

## Auto-Update (v3.0)

Updates are delivered through `tauri-plugin-updater` (registered in `lib.rs`).
The main capability grants exactly `updater:allow-check` and
`updater:allow-download-and-install`, plus the notification permissions the
webview uses. The endpoints the app actually uses come from
`updater_bg.rs::update_endpoints(AppConfig.updates.channel)`: Stable is
`https://github.com/Carme99/PresenceJam-Desktop/releases/latest/download/latest.json`;
Beta is the rolling prerelease asset
`https://github.com/Carme99/PresenceJam-Desktop/releases/download/beta/latest-beta.json`
followed by the stable URL, so a missing or non-newer beta manifest falls through
to stable. The manifest is hand-assembled by `release.yml` and maps each platform
to its signed updater artifact on the GitHub Release:

- `darwin-aarch64` → `PresenceJam-<tag>.app.tar.gz` (+ `.sig`)
- `windows-x86_64` → `PresenceJam-<tag>-setup.exe` (+ `.sig`); the `.msi` is
  also published for managed installations (+ `.msi.sig`)
- `linux-x86_64` → `PresenceJam-linux-amd64.AppImage` (+ `.AppImage.sig`); the
  `.deb` and `.rpm` packages are also published

`latest.json` carries the minisign `signature` (the `.sig` file *content*, not
a path), `version` (tag without the leading `v`), and `pub_date`. The build
matrix produces the unsigned artifacts; the separate `sign` job signs updater
payloads with the `TAURI_SIGNING_PRIVATE_KEY` / `_PASSWORD` secrets. The app's
updater pubkey is inlined in `tauri.conf.json`, so the plugin rejects tampered
payloads.

**Flow:** `UpdatePrompt.svelte` invokes the Rust `updater_bg.rs::check_for_update`
on startup (the plugin's JS `check()` cannot take an endpoint list, so the
banner's candidate is resolved in Rust from the configured channel) → if a
newer version exists it shows a dismissible **"Update vX.Y.Z available"**
banner → **Download & Install** runs the JS `downloadAndInstall()` (offered on
Stable only, where the plugin's static endpoint and the channel's resolved
list are the same manifest) with a progress readout → `invoke("relaunch_app")`
(`commands/misc.rs::relaunch_app`, `AppHandle::restart`) restarts the process
into the new version. On Beta that button is replaced by install-on-quit
(`update.betaOnQuitOnly`, issue #678). A failed check (offline, unreachable
endpoint, signature mismatch) is silent — never blocks the UI.

**Silent background checks + install-on-quit (v4.0):**

- *Background checks:* `UpdatePrompt.svelte` repeats the `check_for_update`
  round trip every ~24h while the app runs (the startup check is unchanged). A failed silent check stays
  console-only — it never surfaces a banner or toast, so an offline machine is
  never nagged.
- *Install-on-quit:* the JS-side `downloadAndInstall()` cannot defer (it applies
  the payload immediately on Windows), so `src-tauri/src/updater_bg.rs` exposes
  a `stage_deferred_update` command that performs its own check + download +
  signature verification on the blocking pool and holds the verified bytes in
  managed `PendingUpdate` state. `lib.rs` runs the app via `build().run()` with
  a **`RunEvent::Exit` arm**: when the user quits (tray + menu Quit share a bounded
  graceful-shutdown — `request_graceful_shutdown` emits `app-shutdown`, waits up to an 8 s
  drain acknowledgement, then exits unconditionally — and `app_exit` funnels into
  `AppHandle::exit`), the staged update is applied during exit. The
  Windows installer relaunches automatically; macOS/Linux pick up the replaced
  bundle/AppImage on next launch.

- *Progress + cancel (4.6):* staging is no longer a black box. The download
  streams throttled `update-stage-progress` events carrying
  `StageProgress { downloaded, total }` — the first chunk always emits, then at
  most one event per 250 ms **or** per 5 whole-percent advance
  (`updater_bg::StageProgressThrottle`), so a fast link still shows movement and a
  slow one cannot flood the webview. `cancel_deferred_update` drops the staged
  update on demand (`PendingUpdate` → `None`), which releases the verified
  `Vec<u8>` instead of holding it for the rest of the session. The payload is
  deliberately memory-resident rather than file-backed: the plugin verifies the
  signature inside `Update::download`, so a file could be swapped after the
  verification and before exit-time install.
  Cancellation is request-scoped and final across the asynchronous boundary.
  `stage_deferred_update` begins under the `PendingUpdateState` lock with a
  generated request id and generation. `cancel_deferred_update` either
  invalidates that active generation, removes the same request's
  already-committed payload, or leaves a bounded pre-begin tombstone when
  cancellation reaches IPC first. A full tombstone set backpressures new
  stages rather than evicting an unmatched cancellation. The download can
  therefore still run after Cancel—the Rust transfer is not interrupted—but
  its bytes cannot commit after cancellation, terminal progress/completion are
  suppressed, and exit installation can never observe them. The frontend also
  filters already-queued progress/completion by request id; this last delivery
  guard complements rather than replaces the backend interlock.

  Notification suppression uses the same request id. The always-mounted
  layout reserves bounded request state before stage IPC; Cancel marks that
  request cancelled, so a queued completion is discarded and a notification
  already awaiting OS permission is suppressed by its cancellation predicate.
  Entries drain after a no-stage result or download failure settles, or after
  a successful notification finishes. Only acknowledged entries may be
  evicted; if all bounded slots are unresolved, a new stage is refused rather
  than creating untracked cancellation state.

Payload signing is independent of OS code signing: the updater works on
unsigned builds, and the macOS unsigned/Gatekeeper story (README
"macOS first-run note") applies to updated `.app` builds too. The release
matrix builds **aarch64 macOS only** — Intel Macs never receive updates
(known gap, see [`docs/archive/3.0-release-research.md`](../archive/3.0-release-research.md)).

## System Tray (v4.6)

`tray.rs` builds the menu natively, from in-process state:

- **Shuffle / Repeat are real toggles (#582).** Both are
  `CheckMenuItemBuilder` items; Shuffle's mark reads `LAST_SHUFFLE_STATE` and
  Repeat's reads `LAST_REPEAT_STATE` plus a mode-spelling label
  (`Repeat: Off` / `Repeat: Context` / `Repeat: Track` — a check mark alone cannot
  tell the last two apart). Those atoms are written by `note_playback_modes` from
  the poll body itself (no extra request, no new scope) and optimistically by a
  successful tray toggle, applied *before* `force_tray_refresh` so the rebuild
  paints the state the API just accepted.
- **Clicks are the inverse / the next mode.** Shuffle targets
  `!last_known`; Repeat targets `RepeatState::next()`, which cycles
  `off → context → track → off` to match Spotify's own button. Both run off the
  menu-event thread (blocking HTTP must not stall the tray).
- **Failure never lies.** A rejected command records nothing, so the mark keeps
  showing the last known truth, and the error is emitted on `playback-error`
  (rendered as an in-app toast — there is no OS/tray notification on this path).
  `NoActiveDevice` has its own log line and message pointing at the Devices
  submenu; 403 surfaces "Playback control requires Spotify Premium".
- **The tray never snapshots a raw token for playback (#586).** Every player
  action — including the Play/Pause state read and the device-list re-fetch — goes
  through `commands::playback::player_with_refresh_typed`, so a stale access token
  is refreshed proactively and an `ExpiredToken` response gets one refresh +
  retry. The tray menu *build* still snapshots `state.tokens.spotify()` for the
  Devices/Queue listings; those are display fetches, not playback commands.
- **Refresh cadence:** the polling loop calls `update_tray_menu` after every
  iteration, behind a dedup key built by `tray_snapshot_for`
  (`src-tauri/src/tray.rs::tray_snapshot_for`) from `(is_syncing, is_window_visible,
  "artist|title|is_playing", shuffle, repeat, snooze deadline + minute bucket)`.
  The mode atoms and the snooze key are in the key on purpose: a Shuffle/Repeat
  change made from another Spotify client has to force a rebuild, otherwise the
  marks stayed on the previous mode (#691), and the snooze bucket keeps the
  countdown repainting rather than freezing at the minute it was set (#677).
  The `mode_change_forces_a_tray_rebuild` test pins that behaviour.
