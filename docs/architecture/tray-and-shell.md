# Tray and shell

> Native surface: the system tray menu, detached windows, UI languages, and the background auto-updater.
>
> Part of the architecture docs — start at the [architecture index](../../ARCHITECTURE.md).

## Multi-Window Detach (v4.0)

Logs and Settings can each be *popped out* into their own window (and popped
back in), VS Code detached-panel style:

- **Creation is JS-side:** the main window constructs child windows via the
  `@tauri-apps/api/webviewWindow` constructor with stable labels
  `logs-detached` / `settings-detached` and URL `/detached/<pane>`; a SvelteKit
  route (`src/routes/detached/[pane]/+page.svelte`) renders `LogViewer` or
  `Settings` in detached mode. `tauri.conf.json`'s `app.windows` is untouched —
  the app still boots single-window.
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
  the close rejected and the pane was marked not-detached while its window stayed
  on screen; a refused close now leaves the badge alone instead of lying
  (`detach.ts::popIn`, with `reconcileDetachedPanes()` on boot). `default.json`
  gains `core:window:allow-create` + `core:webview:allow-create-webview-window`
  for runtime creation.
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
- The locale persists to `localStorage` under `locale`; first run defaults
  to the browser language (`de`/`fr` prefixes), falling back to English.
  The picker lives in Settings → General.
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
- Known limitation: Rust-side error strings surfaced through `invoke()`
  rejections and event payloads remain English. Tray menu labels are English
  literals too — they are built in Rust (`tray.rs`) and never route through `t()`.

## Auto-Update (v3.0)

Updates are delivered through `tauri-plugin-updater` (registered in
`lib.rs`; `updater:default` in `capabilities/default.json`). The endpoint
(`tauri.conf.json`) is
`https://github.com/Carme99/PresenceJam-Desktop/releases/latest/download/latest.json`
— a hand-assembled manifest (`release.yml`) mapping each platform to its
signed artifact on the GitHub Release:

- `darwin-aarch64` → `PresenceJam-<tag>.app.tar.gz` (+ `.sig`)
- `windows-x86_64` → `PresenceJam-<tag>.msi` (+ `.msi.sig`)
- `linux-x86_64` → `PresenceJam-linux-amd64.AppImage` (+ `.AppImage.sig`)

`latest.json` carries the minisign `signature` (the `.sig` file
*content*, not a path), `version` (tag without the leading `v`), and
`pub_date`. The build matrix signs artifacts via the
`TAURI_SIGNING_PRIVATE_KEY` / `_PASSWORD` secrets; the app's updater
pubkey is inlined in `tauri.conf.json`, so the plugin rejects tampered
payloads.

**Flow:** `UpdatePrompt.svelte` calls `check()` on startup → if a newer
version exists it shows a dismissible **"Update vX.Y.Z available"** banner
→ **Download & Install** runs `downloadAndInstall()` with a progress
readout → `invoke("relaunch_app")` (`commands/misc.rs::relaunch_app`,
`AppHandle::restart`) restarts the process into the new version. A failed
check (offline, unreachable endpoint, signature mismatch) is silent —
never blocks the UI.

**Silent background checks + install-on-quit (v4.0):**

- *Background checks:* `UpdatePrompt.svelte` repeats `check()` every ~24h while
  the app runs (the startup check is unchanged). A failed silent check stays
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
  verification and before the exit-time install. A cancel issued while the
  download is still in flight **cannot** interrupt the Rust transfer — the banner
  marks the stage abandoned and discards the payload the moment it lands
  (`UpdatePrompt.svelte::cancelStage` → `stageAbort` → `stageForQuit`), and after a
  successful cancel the banner simply returns to its plain offer.

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
  iteration, behind a dedup key of `(is_syncing, window_visible,
  "artist|title|is_playing")`. The key is deliberately track-scoped, so a mode
  changed from another Spotify client is picked up at the next rebuild rather
  than forcing one.
