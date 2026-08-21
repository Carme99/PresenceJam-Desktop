# Frontend Batch — Hygiene + Layout Lift Report

**Branch:** `fix/frontend-batch` · **Base:** `origin/main` (`ffeee24`)
**Commit:** `fix(svelte): batch hygiene + lift reconnect handlers to layout (#220 #221 #222 #224 #225)`
**Files:** 10 changed incl. `REPORT.md` (9 svelte/ts/json + report)

---

## 1. Changed files

| File | Change kind |
|---|---|
| `src/routes/+layout.svelte` | Lift always-mounted listeners, add toast |
| `src/routes/+page.svelte` | Wrap invokes + micro-race guards |
| `src/lib/components/Settings.svelte` | Remove spotify listener, debounce preview, harden autostart/invokes, fix imports |
| `src/lib/components/Dashboard.svelte` | Remove dead notification prefetch, wrap goToSetup, fix stale comment, micro-race notes |
| `src/lib/components/Reconnect.svelte` | Derive `needsTeams`, offer Teams re-auth button, wrap keychain checks |
| `src/lib/components/LogViewer.svelte` | Wrap `open_logs_folder` |
| `src/lib/components/Onboarding.svelte` | Import `AppConfig` from `../types` |
| `src/lib/stores/config.ts` | Delete hand-written `AppConfig` interfaces, import generated type |
| `package.json` | Remove `@tauri-apps/plugin-store` (dead dep), keep `plugin-shell` |
| `src/lib/utils/useAuthListeners.ts` | Verified unchanged — shared auth listener helper (no hygiene gap); still correctly used by Settings/Reconnect/Onboarding |
| `REPORT.md` | This report |

`package-lock.json` is **intentionally untouched** (contract §2).

---

## 2. Per-issue what / why

### #220 — Lift `spotify-reconnect-required` + steal `playback-error` to `+layout`

**What:**
- `+layout.svelte:62` — new `listen<string>('spotify-reconnect-required', …)` — checks `is_spotify_client_secret_set` + loads `client_id` from `load_config`, redirects to `onboarding` if either missing, otherwise invokes `start_spotify_reconnect`. Toast-level `setSpotifyPhase('waiting'/'error')` + `currentView.set('settings')`.
- `+layout.svelte:94` — new `listen<string>('playback-error', …)` with `showPlaybackError()` toast (6 s auto-dismiss, manual dismiss button, `role="alert" aria-live="polite"`).
- `+layout.svelte:30-37,124-135` — `destroyed` flag + per-listener `.then(u => if(destroyed) u(); else unlistenX = u)` micro-race guard; `onDestroy` + `onMount` cleanup clears `playbackErrorTimeout`.
- `Settings.svelte:117-120` — replace the Settings-owned `spotify-reconnect-required` listener with a `NOTE` explaining layout ownership (avoid double-handle + missed events when Dashboard is mounted).
- `Dashboard.svelte:196` — fix stale comment (`in Settings.svelte` → `in +layout.svelte (issue #220)`).

**Why:** Polling loop emits `spotify-reconnect-required` while the user is on Dashboard (Settings not mounted) — a Settings-only listener drops the event and the reconnect never fires. Layout is always mounted, so moving the handler guarantees delivery. Same rationale already applied to `teams-reconnect-required` (#157); this patch extends it to Spotify and adds `playback-error` (previously un-handled at UI layer).

### #221 — Debounce + seq-guard `preview_status` in Settings

**What:**
- `Settings.svelte:65-88` — add `previewSeq` + `previewDebounce` (300 ms) around `invoke<string>('preview_status')`; async `setTimeout` callback captures `my = ++previewSeq` and early-returns if `my !== previewSeq`. On success `previewText = v`; on error `(preview unavailable)` with same seq check.
- `Settings.svelte:163-166` — `onDestroy` clears `previewDebounce`.

**Why:** `$effect` on `localConfig.teams.status_format` fires per keystroke; the previous fire-and-forget `invoke.then` spammed the Rust backend and displayed stale results when responses returned out of order. Debounce cuts IPC rate; seq guard discards out-of-order replies.

### #222 — Deduplicate `AppConfig` — generate, don't hand-write

**What:**
- `src/lib/stores/config.ts:1-3` — delete 60-line hand-written `SpotifyConfig/TeamsConfig/PollingConfig/LoggingConfig/AppConfig` interfaces; replace with `import type { AppConfig } from '../types'` (re-exports `types-generated/AppConfig.ts` via `ts-rs`). Keep `defaultConfig` + `loadConfig`/`saveConfig` logic.
- `Settings.svelte:7-8` — `import type { AppConfig } from '$lib/types'` (was `from '$lib/stores/config'`).
- `Onboarding.svelte:4-5` — same import migration.
- `Reconnect.svelte:6` — already correct.
- `+layout.svelte:14` — `import type { …, AppConfig }`.
- `package.json` — remove `@tauri-apps/plugin-store` (no JS import remains; Rust never used `tauri_plugin_store`).

**Why:** Hand-written fallback types drift from Rust `config.rs` source-of-truth (GH #13). Generated `types-generated/AppConfig.ts` is canonical; single import eliminates dual-maintenance bug class. `plugin-store` was a leftover dep with zero call sites; removing it shrinks install surface.

### #224 — Harden `autostart` toggle + wrap fallible invokes

**What:**
- `Settings.svelte:499-514` — `onchange` for autostart captures `target.checked`, computes `previous = !enabled`, sets `localConfig.autostart = enabled`, then `try { await invoke('set_autostart_enabled') } catch { revert both `localConfig.autostart` and `target.checked`, show 3 s `saveMessage` }`.
- `Settings.svelte:186-190` — wrap `open_logs_folder` in try/catch.
- `LogViewer.svelte:50-54` — same wrap.
- `+page.svelte:50-54` — `show_window` wrapped (`try { await invoke('show_window') } catch …`).
- `+page.svelte:84-88` — `open_logs_folder` wrapped with `console.error` (folder open is user-visible).
- `Reconnect.svelte:27-28,80-81` — `is_spotify_client_secret_set` wrapped (`try { has=await invoke… } catch { has=false }`) in both `onMount` and `reconnectSpotify`.
- `Dashboard.svelte:252-278` — `goToSetup` wrapped in `try/catch` around `loadConfig` + `is_spotify_client_secret_set`; on failure sets `goToSetupHint` + 4 s disabled hint.

**Why:** `set_autostart_enabled` touches OS autostart entry (registry/LaunchAgent/systemd) and can throw; without revert the checkbox and `localConfig` desync. The other `invoke` wrappers prevent unhandled rejections from surfacing as red console errors / silent failures when keychain or log folder is unavailable.

### #225 — Reconnect `needsTeams` + Teams re-auth + listener micro-race + dead code

**What:**
- `Reconnect.svelte:35-40` — `needsTeams` derived from `invoke<SyncStatus>('get_sync_status').teams_connected` (`needsTeams = !status.teams_connected`, catch → `true`) instead of `hard-coded true`. Badge/class and conditional rendering key off `needsTeams` (`authFlow.teams.phase === 'done' || !needsTeams` → Connected).
- `Reconnect.svelte:100-122` — `reconnectTeams()` starts device-code flow (`start_teams_auth_device_code` → `open_external_url` (non-fatal warn) → `pollTeamsAuth`). `pollTeamsAuth` clears `needsTeams = false` on success.
- `Reconnect.svelte:190-221` — Teams card renders verification URL + code + spinner + "I have signed in — check now" while `phase === 'waiting'`; error/try-again otherwise; idle "Needs reconnect: Reconnect Teams" button.
- `+page.svelte:46-98` — `let destroyed = false` + `.then(fn => if(destroyed) fn(); else …)` for `tray-click`, `app-shutdown`, `navigate`, `open-logs-folder`, `show-about`; `return () => { destroyed=true; unlisten… }`.
- `+layout.svelte:37-125` — same pattern for `teams`, `spotify`, `playback-error`.
- `Dashboard.svelte:59` — remove dead eager `isPermissionGranted().catch(()=>false)` (no-op prefetch that leaked an unhandled promise); legitimate `isPermissionGranted()` inside `spotify-track-changed` handler (opt-in path) remains correctly wrapped.

**Why:** The prior Reconnect view only offered Spotify; Teams device-code failures (`#151/#157`) landed the user with no path to re-auth Teams. Deriving `needsTeams` from sync status makes the card truthful, and the button re-uses the existing device-code flow. Micro-race guards close the `listen().then(u=>unlisten=u)` race where the component unmounts before the promise resolves (leaking listeners). Dead prefetch removed to silence unhandled-rejection lint.

---

## 3. Risks

| Risk | Mitigation | Residual |
|---|---|---|
| Layout Spotify handler redirects to onboarding when keychain empty — user sees onboarding instead of settings | Correct: without secret the reconnect cannot succeed; prior Settings path did the same redirect | Low — existing behavior, now in layout |
| Layout `load_config` for `clientId` races with `loadConfig()` in Settings/Reconnect | Each handler fetches its own `load_config`; single-writer save mutex in `config.ts` unaffected | Low |
| Debounce delays preview by 300 ms | Acceptable per #221 spec; preview is non-blocking hint | Negligible |
| Removed `plugin-store` — possible dynamic import | Grep shows zero JS/RS call sites; build will fail if missed | Zero (build catches) |
| `shell` kept even though `open_external_url` uses opener — double plugin | `src-tauri/src/lib.rs:557` still calls `tauri_plugin_shell::init()`; removing would break build | Kept correctly |
| `needsTeams` flips to `false` only on `teams-auth-complete`/`pollTeamsAuth` success | If user closes Reconnect before poll, next `Reconnect` mount re-derives from `get_sync_status` | Low |
| `package-lock.json` still pins `@tauri-apps/plugin-store@2.4.4` while `package.json` dropped it — `npm ci` fails `ERESOLVE`/`sync-check` until lockfile regenerated (`npm install`) | Intentionally not touched per batch contract §2 (Do NOT touch package-lock.json); drift is expected and must be resolved in follow-up lockfile regeneration | Medium — blocks clean `npm ci` until regenerated |

---

## 4. Grep evidence (selected)

```
# layout owns 3 always-mounted listeners + toast
src/routes/+layout.svelte:39:    listen('teams-reconnect-required', async () => {
src/routes/+layout.svelte:62:    listen<string>('spotify-reconnect-required', async () => {
src/routes/+layout.svelte:67:        const hasSecret = await invoke<boolean>('is_spotify_client_secret_set');
src/routes/+layout.svelte:74:        const cfg = await invoke<AppConfig>('load_config');
src/routes/+layout.svelte:94:    listen<string>('playback-error', (event) => {
src/routes/+layout.svelte:21:  function showPlaybackError(msg: string) {
src/routes/+layout.svelte:37:    let destroyed = false;
src/routes/+layout.svelte:58:      if (destroyed) u();
src/routes/+layout.svelte:90:      if (destroyed) u();
src/routes/+layout.svelte:99:      if (destroyed) u();

# Settings no longer owns spotify handler; note present
src/lib/components/Settings.svelte:117:    // NOTE: `spotify-reconnect-required` is handled by the always-mounted
src/lib/components/Settings.svelte:122:    // NOTE: `teams-reconnect-required` is handled by the always-mounted

# Settings preview debounce+seq
src/lib/components/Settings.svelte:65:  let previewSeq = 0;
src/lib/components/Settings.svelte:66:  let previewDebounce: ReturnType<typeof setTimeout> | null = null;
src/lib/components/Settings.svelte:76:    previewDebounce = setTimeout(async () => {
src/lib/components/Settings.svelte:77:      const my = ++previewSeq;
src/lib/components/Settings.svelte:79:        const v = await invoke<string>('preview_status', { format });
src/lib/components/Settings.svelte:80:        if (my !== previewSeq) return;
src/lib/components/Settings.svelte:163:    if (previewDebounce) {

# config.ts generated import
src/lib/stores/config.ts:3:import type { AppConfig } from '../types';
src/lib/components/Settings.svelte:8:  import type { AppConfig, SyncStatus, TeamsTokens } from '$lib/types';
src/lib/components/Onboarding.svelte:5:  import type { AppConfig } from '$lib/types';
src/routes/+layout.svelte:14:  import type { DeviceCodeResponse, TeamsTokens, AppConfig } from '$lib/types';

# autostart revert + invoke wrapping
src/lib/components/Settings.svelte:505:              await invoke('set_autostart_enabled', { enabled });
src/lib/components/Settings.svelte:506:            } catch (err) {
src/lib/components/Settings.svelte:508:              localConfig.autostart = previous;
src/lib/components/Settings.svelte:187:      await invoke('open_logs_folder');
src/lib/components/LogViewer.svelte:51:      await invoke('open_logs_folder');
src/routes/+page.svelte:51:        await invoke('show_window');
src/lib/components/Reconnect.svelte:28:    try { hasClientSecret = await invoke<boolean>('is_spotify_client_secret_set'); } catch { hasClientSecret = false; }
src/lib/components/Dashboard.svelte:260:      const hasClientSecret = await invoke<boolean>('is_spotify_client_secret_set');

# Reconnect needsTeams + Teams button
src/lib/components/Reconnect.svelte:14:  let needsTeams = $state(false);
src/lib/components/Reconnect.svelte:37:      needsTeams = !status.teams_connected;
src/lib/components/Reconnect.svelte:194:          class:success={authFlow.teams.phase === 'done' || !needsTeams}
src/lib/components/Reconnect.svelte:219:        <button class="btn-full" onclick={reconnectTeams}>Reconnect Teams</button>
src/lib/components/Reconnect.svelte:113:        await invoke('open_external_url', { url: response.verification_url });

# micro-race destroyed guards
src/routes/+page.svelte:46:    let destroyed = false;
src/routes/+page.svelte:79:    }).then(fn => { if (destroyed) fn(); else unlisten.push(fn); });

# package.json: store removed, shell kept
package.json:23:    "@tauri-apps/plugin-shell": "^2.3.5",
# (no plugin-store entry)

# useAuthListeners — verified unchanged, still correctly wired
src/lib/utils/useAuthListeners.ts:11:export async function useAuthListeners(handlers: AuthListeners): Promise<UnlistenFn> {
src/lib/components/Settings.svelte:10:  import { useAuthListeners } from '$lib/utils/useAuthListeners';
src/lib/components/Reconnect.svelte:9:  import { useAuthListeners } from '$lib/utils/useAuthListeners';
src/lib/components/Onboarding.svelte:9:  import { useAuthListeners } from '$lib/utils/useAuthListeners';

# Dashboard dead-code comment fix
src/lib/components/Dashboard.svelte:196:      // spotify-reconnect-required in +layout.svelte (issue #220),
```

---

## 5. Verification

- `git diff --stat` shows 9 svelte/ts/json files + `REPORT.md` = 10 files changed (no lockfile mutation; `package-lock.json` drift noted in §3).
- `src/lib/types.ts` re-exports `AppConfig` from `types-generated/AppConfig` — single source of truth.
- All 3 layout listeners, Settings NOTE, debounce+seq, generated import, `needsTeams` Teams card, autostart revert, and wrapped invokes verified via `grep` above.
- No formatter/gate run per contract (§2).

---

*Generated for review; see commit for full diff.*
