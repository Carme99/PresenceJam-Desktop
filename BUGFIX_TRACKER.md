# PresenceJam v2.4.0 — Bug Fix Tracker

Branch: `fix/comprehensive-bugfixes-2.4`

This document tracks all bugs identified for v2.4.0. Each fix should be committed
separately with a reference to its GitHub issue number.

---

## Commits on this branch

| # | Commit | Description | GitHub |
|---|--------|-------------|--------|
| 1 | `5241d6b` | fix: make polling loop interruptible via mpsc channel | #10 |
| 2 | `3c74baf` | fix: validate tokens in get_sync_status and emit reconnect-required events | #11, #12 |

---

## Bug Index

### Critical (must fix before release)

- [x] ~~Bug 1~~ — **Tray freeze: polling thread cannot be cancelled mid-sleep** (#10)
- [x] ~~Bug 2+3~~ — **get_sync_status doesn't validate tokens; polling fails silently** (#11, #12)
- [x] ~~Bug 4~~ — **`clear_on_pause` config option not wired up in process_track**

### High (should fix)

- [x] ~~Bug 5~~ — **`start_minimized` exists in TypeScript config but missing from Rust `TeamsConfig`**
- [x] ~~Bug 6~~ — **`launch_at_login` in Settings binds to `localConfig.teams.launch_at_login` (wrong field)**
- [x] ~~Bug 7~~ — **`update_tray_menu` errors silently ignored; tray state can desync**

### Medium

- [x] ~~Bug 8~~ — **Refreshed Spotify tokens not persisted to tauri-plugin-store**
- [x] ~~Bug 9~~ — **Windows deep link registration only runs in debug builds**
- [x] ~~Bug 10+12~~ — **No token validation on startup; `is_onboarding_complete` returns false positives**
- [x] ~~Bug 11~~ — **Initial tray menu always shows "Start Syncing" regardless of actual sync state**
- [x] ~~Bug 13~~ — **`progress_ms` not corrected by elapsed time**
- [x] ~~Bug 14~~ — **TOCTOU in `start_syncing`**
- [x] ~~Bug 15~~ — **Placeholder OAuth URL in Onboarding.svelte manual auth section**

### Low (nice to have)

- [x] ~~Bug 16~~ — **Frontend doesn't handle `sync-started`/`sync-stopped` events**
- [x] ~~Bug 17~~ — **Teams status can flicker when track changes rapidly (no debounce)**
- [x] ~~Bug 18~~ — **Redundant quit handler thread spawned alongside `on_window_event(CloseRequested)`**
- [x] ~~Bug 19~~ — **Redundant `client_id` field in `refresh_spotify_token` request body (already in Basic auth)**
- [ ] Bug 20 — `src/lib/stores/app.ts`, `spotify.ts`, `teams.ts` are unused dead stores
- [x] ~~Bug 21~~ — **`LogViewer.svelte` not wired to Rust log stream**
- [x] ~~Bug 22+23~~ — **`pending_teams_auth` and `pending_spotify_auth` not persisted to store**
- [x] ~~Bug 24+25~~ — **Initial tray menu shows no track info; tray not updated on track changes**
- [x] ~~Bug 26~~ — **`config.rs` load/save has potential concurrent access race**

---

## Bug Details & Fix Plan

---

### Bug 1 — Polling thread cannot be cancelled mid-sleep [DONE]

**Issue:** #10  
**Symptom:** Right-click tray → Pause Syncing freezes the app entirely. End-task required.  
**Root cause:** `thread::sleep` is not interruptible. `stop_polling` set a bool flag but the thread
was blocked on a 15–30s sleep, so it didn't see the flag until the sleep expired.  

**Fix:** Replaced the sleep-based loop with an `mpsc::Channel`. The stop signal closes the
sender, which immediately unblocks `recv_timeout` on the receiver side.  

**Files:** `src-tauri/src/polling.rs`, `src-tauri/src/lib.rs`

---

### Bug 2+3 — Token validity not checked; polling fails silently [DONE]

**Issues:** #11, #12  
**Symptom:** App shows "Connected" in status bar even when Spotify/Teams are unreachable.
No reconnect prompt ever appears.  
**Root cause (Bug 2):** `get_sync_status` only checked `tokens.is_some()` — presence, not validity.  
**Root cause (Bug 3):** When the polling loop encountered an auth failure after a token refresh,
it only logged and retried silently — no event was ever emitted to the frontend.

**Fix (Bug 2):** Added `validate_spotify_token()` in `spotify.rs` and `validate_teams_token()` in
`teams.rs`. Both make a real API call (`/me/player/currently-playing` and `/me/presence`) to
confirm the token still works. `get_sync_status` now calls these instead of just checking
`is_some()`.  

**Fix (Bug 3):** The polling loop now emits `spotify-reconnect-required` when a token refresh
permanently fails (after the retry attempt). `process_track` emits `teams-reconnect-required`
whenever a Teams API call returns 401/403.

**Files:** `src-tauri/src/spotify.rs`, `src-tauri/src/teams.rs`, `src-tauri/src/commands.rs`,
`src-tauri/src/polling.rs`

---

### Bug 4 — `clear_on_pause` config not wired in process_track

**Severity:** High  
**Symptom:** When Spotify pauses, PresenceJam always clears the Teams status even if the user
has disabled `clear_on_pause` in settings.  

**Root cause:** `process_track` calls `clear_teams_status_message` unconditionally when
`!track.is_playing`. The `config.teams.clear_on_pause` field is never consulted.

**Fix needed:** In `polling.rs` `process_track`, change:

```rust
// CURRENT (wrong):
} else {
    match clear_teams_status_message(&teams_tok.access_token) {

// SHOULD BE:
} else if config.as_ref().map(|c| c.teams.clear_on_pause).unwrap_or(true) {
    match clear_teams_status_message(&teams_tok.access_token) {
```

**Files:** `src-tauri/src/polling.rs`

---

### Bug 5 — `start_minimized` missing from Rust TeamsConfig

**Severity:** High  
**Symptom:** "Start minimized to tray" option in Settings has no effect because the Rust
`TeamsConfig` struct has no `start_minimized` field — the value is saved but never loaded
into the Rust app.

**Fix needed:**  
1. Add `start_minimized: bool` field to `TeamsConfig` in `config.rs`  
2. In `lib.rs` `setup` after config is loaded, check `config.teams.start_minimized` and
   call `window.hide()` if true  
3. Ensure the Settings.svelte binding is correct (it currently uses
   `localConfig.teams.startMinimized` which maps correctly via the serde rename)

**Files:** `src-tauri/src/config.rs`, `src-tauri/src/lib.rs`, `src/lib/components/Settings.svelte`

---

### Bug 6 — `launch_at_login` binds to wrong field

**Severity:** High  
**Symptom:** "Launch at login" checkbox in Settings doesn't work.  

**Root cause:** The binding is `localConfig.teams.launch_at_login` but:
- The field is stored in `AppConfig` (top-level), not `TeamsConfig`
- The Rust struct `AppConfig` has no `autostart` field
- Tauri plugin-autostart expects a specific invocation

**Fix needed:**  
1. Add `autostart: bool` to `AppConfig` in `config.rs`  
2. Bind Settings.svelte "Launch at login" to `localConfig.app.autostart`  
3. In the save handler, call `app.autostart::set_enabled(config.autostart)` when the
   autostart plugin is available (wrapped in `#[cfg(target.os = "macos")]` or platform-gated)  
4. Load autostart state into `config.autostart` on startup

**Files:** `src-tauri/src/config.rs`, `src-tauri/src/commands.rs`, `src/lib/components/Settings.svelte`

---

### Bug 7 — `update_tray_menu` errors silently ignored

**Severity:** High  
**Symptom:** If tray menu update fails (e.g., IPC to tray process), the app continues as
if it succeeded. Tray icon can show stale state indefinitely.  

**Root cause:** `update_tray_menu` returns `Result<()>` but every call site ignores it with
`let _ = update_tray_menu(...)`.

**Fix needed:**  
1. Make `update_tray_menu` return a meaningful error type  
2. At minimum, log errors at `warn!` level even if propagation isn't practical for all callers  
3. The `update_tray_menu_state` command in `commands.rs` should return the `Result` so the
   frontend can display a toast/alert on persistent failure

**Files:** `src-tauri/src/tray.rs`, `src-tauri/src/commands.rs`

---

### Bug 8 — Refreshed Spotify tokens not persisted to store

**Severity:** Medium  
**Symptom:** Token refresh succeeds in the polling loop and the in-memory state is updated,
but after an app restart the old (possibly expired) tokens are loaded from store.

**Root cause:** In `polling.rs` after a successful `refresh_spotify_token`, the new tokens
are written to `state.spotify_tokens` but `save_spotify_tokens` is never called.

**Fix needed:** In `polling.rs` after successfully updating `*state.spotify_tokens.write()`
with new tokens from a refresh, also call `save_spotify_tokens(&app, &new_tokens)`.

**Files:** `src-tauri/src/polling.rs`

---

### Bug 9 — Windows deep link registration skips release builds

**Severity:** Medium  
**Symptom:** `presencejam://` deep links work on macOS and in Windows debug builds, but
silently fail in Windows release builds.

**Root cause:** In `lib.rs`:

```rust
#[cfg(any(windows, all(debug_assertions, windows)))]
```

The `all(debug_assertions, windows)` guard means release builds (where `debug_assertions`
is false) skip the registration entirely.

**Fix needed:**

```rust
#[cfg(windows)]
```

**Files:** `src-tauri/src/lib.rs`

---

### Bug 10+12 — No startup token validation; is_onboarding_complete returns false positives

**Severity:** Medium  
**Bug 10 root cause:** On app startup, tokens are loaded from store but never validated.
A revoked or expired token will cause immediate polling failure with no reconnect prompt.  

**Bug 12 root cause:** `is_onboarding_complete` only checks `!spotify_tokens.is_empty() &&
!teams_tokens.is_empty()` — it doesn't confirm the tokens work.

**Fix needed:**  
1. Add a `validate_all_tokens(state)` helper that calls `validate_spotify_token` and
   `validate_teams_token` on startup  
2. In `is_onboarding_complete`, call this helper and return false if any validation fails  
3. If validation fails on startup, emit `spotify-reconnect-required` and/or
   `teams-reconnect-required` so the frontend can navigate to re-auth

**Files:** `src-tauri/src/commands.rs`, `src-tauri/src/polling.rs`

---

### Bug 11 — Initial tray menu always shows "Start Syncing"

**Severity:** Medium  
**Symptom:** After app launch, right-clicking the tray icon shows "Start Syncing" even if
sync was running before the last app close.

**Root cause:** `build_initial_menu` in `tray.rs` always inserts `ID_START_SYNC` and never
checks `is_syncing` state or calls `update_tray_menu_state`.

**Fix needed:**  
1. Add `ID_RESUME_SYNC` to `build_initial_menu`  
2. Always call `update_tray_menu_state` after building the initial menu so the correct
   option is highlighted/visible  
3. Or better: have `build_initial_menu` accept the current `is_syncing` state and render
   the correct menu

**Files:** `src-tauri/src/tray.rs`, `src-tauri/src/menu.rs`

---

### Bug 13 — `progress_ms` not corrected by elapsed time

**Severity:** Medium  
**Symptom:** When resuming after a pause, the estimated track position in Teams status can
be wrong by up to `DEFAULT_INTERVAL_SECONDS` (30s).

**Root cause:** `track.progress_ms` comes from Spotify's API at poll time. By the time
`process_track` runs, `progress_ms` is stale. There's no correction for elapsed time since
the last poll.

**Fix needed:** In `polling.rs` `polling_loop`, store a `last_poll_instant: Instant` and
when processing a track, add `last_poll_instant.elapsed().as_millis()` as a correction to
`progress_ms`.

**Files:** `src-tauri/src/polling.rs`

---

### Bug 14 — TOCTOU in start_syncing

**Severity:** Medium  
**Symptom:** Calling `start_syncing` concurrently (e.g., rapid button clicks) can spawn
multiple polling threads.

**Root cause:** `start_syncing` reads `is_syncing` under a read lock, then spawns the
thread, then sets `is_syncing = true` under a write lock. Between the read and the write,
another call can also pass the read-check and both paths will proceed to spawn threads.

**Fix needed:** Acquire the write lock first, check `is_syncing` inside it, then spawn:

```rust
// CURRENT (TOCTOU):
let is_syncing = { *state.is_syncing.read() }; // check
if is_syncing { return Ok(()); }
polling::start_polling(...);                    // spawn happens before write lock
*state.is_syncing.write() = true;              // write

// FIXED:
{
    let mut guard = state.is_syncing.write();
    if *guard { return Ok(()); }
    *guard = true;
}
polling::start_polling(...); // safe: is_syncing already true
```

**Files:** `src-tauri/src/commands.rs`

---

### Bug 15 — Placeholder OAuth URL in Onboarding.svelte

**Severity:** Medium  
**Symptom:** The "Enter code manually" flow in Onboarding.svelte shows a placeholder URL
instead of real instructions for creating a Spotify app.

**Root cause:** `manualAuth` section has a placeholder URL.

**Fix needed:** Replace the placeholder with:
- Real instructions explaining how to create a Spotify app at
  https://developer.spotify.com/dashboard
- The redirect URI the user needs to set: `http://localhost:43210/callback`
- Instructions to paste the authorization code from the URL after Spotify redirects

**Files:** `src/lib/components/Onboarding.svelte`

---

### Bug 16 — Frontend doesn't handle sync-started/sync-stopped events

**Severity:** Low  
**Symptom:** The Dashboard UI doesn't react to sync state changes from the backend. The
`use_auto_refresh` store is dead code — it was meant to update the sync toggle button
but nothing emits to it.

**Root cause:** `Dashboard.svelte` has a `syncStatus` store but no `onMount` listener
for `sync-started`/`sync-stopped` events from Tauri. The backend emits these events
(commands `start_syncing` and `stop_syncing`) but the frontend ignores them.

**Fix needed:**  
1. In `+layout.svelte` or `Dashboard.svelte`, add:
   ```js
   import { listen } from '@tauri-apps/api/event';
   onMount(async () => {
     await listen('sync-started', () => syncStatus.set({ ...$syncStatus, isSyncing: true }));
     await listen('sync-stopped', () => syncStatus.set({ ...$syncStatus, isSyncing: false }));
   });
   ```
2. Remove the unused `use_auto_refresh` store and its references

**Files:** `src/lib/components/Dashboard.svelte`, `src/lib/stores/app.ts`

---

### Bug 17 — Teams status can flicker on rapid track changes

**Severity:** Low  
**Symptom:** When skipping through tracks quickly, Teams status briefly shows the previous
track's info before updating to the new one.

**Root cause:** No debounce. Each track change immediately calls `set_teams_status_message`.

**Fix needed:** Add a debounce in `process_track`: if a new track comes in within
`DEBOUNCE_MS` (e.g., 500ms) of the last update, skip the Teams API call and wait for the
next poll cycle.

**Files:** `src-tauri/src/polling.rs`

---

### Bug 18 — Redundant quit handler thread spawn

**Severity:** Low  
**Root cause:** In `lib.rs`, there are two handlers for quit:
1. `on_window_event(CloseRequested)` which calls `hide()` on macOS or `app.exit()` on
   other platforms
2. A `std::thread::spawn` dedicated to handling the quit signal

The spawned thread is redundant — `on_window_event` already handles quit correctly.

**Fix needed:** Remove the `std::thread::spawn` block and the `setup_quit_handler` function
(if it exists), keeping only the `on_window_event` handler.

**Files:** `src-tauri/src/lib.rs`

---

### Bug 19 — Redundant client_id in refresh_spotify_token

**Severity:** Low  
**Root cause:** In `refresh_spotify_token` (`spotify.rs`), the form body includes
`("client_id", client_id)` but the request also uses `.basic_auth(client_id, Some(client_secret))`.
The `client_id` appears twice — once in Basic auth (correct) and once in the form body
(Spotify ignores duplicate `client_id` in form but it's unnecessary).

**Fix needed:** Remove `("client_id", client_id)` from the form `params` array in
`refresh_spotify_token`. Keep it in `complete_spotify_auth` where it's required (PKCE
flow doesn't use Basic auth there).

**Files:** `src-tauri/src/spotify.rs`

---

### Bug 20 — Dead stores in src/lib/stores/

**Severity:** Low  
**Root cause:** `app.ts`, `spotify.ts`, and `teams.ts` in `src/lib/stores/` define Svelte
stores that are never imported or used by any component. They were likely scaffolding
that became obsolete as the component architecture evolved.

**Fix needed:** Delete `src/lib/stores/app.ts`, `src/lib/stores/spotify.ts`,
`src/lib/stores/teams.ts` and confirm no import references remain.

**Files:** `src/lib/stores/app.ts`, `src/lib/stores/spotify.ts`, `src/lib/stores/teams.ts`

---

### Bug 21 — LogViewer not wired to tauri_plugin_log

**Severity:** Low  
**Root cause:** `LogViewer.svelte` renders a static log buffer in component state. It
doesn't subscribe to the Rust-side log stream that `tauri_plugin_log` provides.

**Fix needed:**  
1. Use `log::get()` from `tauri_plugin_log` to get the log entries in Rust  
2. Emit log entries to the frontend via a Tauri event (`log-entry`)  
3. In `LogViewer.svelte`, listen for `log-entry` events and append to a reactive log list

**Files:** `src-tauri/src/lib.rs`, `src/lib/components/LogViewer.svelte`

---

### Bug 22+23 — Pending auth state not persisted

**Severity:** Low  
**Bug 22 root cause:** If the Teams device code flow is started but the user never
completes it, `pending_teams_auth` is lost on app restart — the user sees the "Sign in
with Microsoft" prompt again with no indication they were mid-auth.

**Bug 23 root cause:** Same for Spotify PKCE flow — if the browser is closed before
completing auth, `pending_spotify_auth` (verifier, challenge) is lost.

**Fix needed:**  
1. Persist `pending_teams_auth` (device_code, user_code, expires_at) to
   `tauri-plugin-store` when `start_teams_auth` succeeds  
2. On startup, check for `pending_teams_auth` and if it hasn't expired, skip the
   onboarding step and show "Waiting for Microsoft authentication..."  
3. Same pattern for Spotify `pending_spotify_auth`

**Files:** `src-tauri/src/commands.rs`, `src-tauri/src/polling.rs`,
`src/lib/components/Onboarding.svelte`

---

### Bug 24+25 — Initial tray menu shows no track; not updated on track changes

**Severity:** Low  
**Bug 24 root cause:** `build_initial_menu` constructs the menu without consulting the
current track state. It always uses placeholder text.

**Bug 25 root cause:** `process_track` successfully calls `update_tray_menu` internally but
the call was removed or commented out — tray menu never gets updated with track info.

**Fix needed:**  
1. In `polling.rs` after every successful `process_track`, call
   `update_tray_menu(app.clone())` to refresh the tray menu  
2. In `build_initial_menu`, read the current track from state and include it in the
   menu if available  
3. Or: call `update_tray_menu_state` from `process_track` instead of building a fresh menu

**Files:** `src-tauri/src/polling.rs`, `src-tauri/src/tray.rs`

---

### Bug 26 — Config load/save concurrent access race

**Severity:** Low  
**Root cause:** `config.rs` `load_config` and `save_config` are called from different
threads (polling loop + Tauri command handlers) without synchronization. If a poll
happens to trigger a save while a command handler is reading, the RwLock prevents
corruption, but there's no coordinated locking around the read-modify-write cycle when
updating individual fields.

**Fix needed:** Wrap config loading/saving in a `Mutex<AppConfig>` or ensure all config
mutations go through a single command that holds a lock for the duration of the read-modify-write.

**Files:** `src-tauri/src/config.rs`, `src-tauri/src/commands.rs`

---

## Version Bump Checklist (v2.4.0)

When all bugs are fixed, update these files:

- [ ] `src-tauri/Cargo.toml` — `version = "2.4.0"`
- [ ] `src-tauri/tauri.conf.json` — `version`, `bundle.version`, `productName` version refs
- [ ] `package.json` — `version`
- [ ] `README.md` — version badge and any "current version" mentions
- [ ] `CHANGELOG.md` — add v2.4.0 section with all bug fixes

---

## Architecture Changes (for ARCHITECTURE.md)

After all fixes, the following should be documented:

1. **Interruptible Polling Loop** — `mpsc::Channel` replaces `thread::sleep`. Closing the
   sender (`stop_tx = None`) immediately unblocks all pending `recv_timeout` calls.
   Prevents the up-to-30s freeze when stopping sync from tray.

2. **Token Validation Lifecycle** — `get_sync_status` now calls `validate_spotify_token`
   and `validate_teams_token` (real API calls, not just presence checks). Polling emits
   `spotify-reconnect-required` / `teams-reconnect-required` events on permanent auth
   failure so the frontend can navigate to re-auth without waiting for a timeout.

3. **New Events** — `spotify-reconnect-required`, `teams-reconnect-required`,
   `sync-started`, `sync-stopped` — frontend should subscribe to these for reactive UI.

4. **Smart Sleep** — `process_track` returns a dynamic sleep duration. If track has
   <30s remaining, sleep until it ends + buffer. Otherwise sleep 15–30s with ±20% jitter.
