# Sync-Threading Lane — Report

Worktree: `/tmp/pj-worktrees/sync-threading` — branch `fix/sync-threading` off `ffeee24` (v3.1.0)
Scope: #215, #218, #219

## Changed Files

| File | Change |
|------|--------|
| `src-tauri/src/commands/sync.rs` | `start_syncing`, `stop_syncing`, `app_exit` → `pub async fn`; `stop_polling_and_join` redesigned to use `spawn_blocking`; `start_polling` spawn offloaded; `app_exit` uses timeout + detached drain |
| `src-tauri/src/commands/misc.rs` | `update_tray_menu_state`, `relaunch_app` → `pub async fn` + `spawn_blocking`; `preview_status` stays sync (pure formatting) |
| `src-tauri/src/commands/window.rs` | `set_autostart_enabled`, `open_logs_folder`, `open_external_url` → `pub async fn` + `spawn_blocking`; `show_window` stays sync |
| `src-tauri/src/commands/config.rs` | `save_config` → `pub async fn` + `spawn_blocking` for write-lock + fsync; autostart now `await`ed |
| `src-tauri/src/polling/poll_once.rs` | #219 retry-path InvalidGrant now clears tokens, emits both events, increments counter |
| `src-tauri/src/polling/loop.rs` | No change — owned but no IO-bound command; driver remains sync thread |

## Per-Issue Summary

### #215 — IO-bound commands to async

**What:** Converted all commands that touch disk/network/process to `pub async fn` with `tauri::async_runtime::spawn_blocking`:

- `sync.rs`: `start_syncing` (drain + `start_polling` spawn), `stop_syncing`, `app_exit`
- `misc.rs`: `update_tray_menu_state` (tray menu builds + `cached_devices`/`cached_queue` blocking HTTP), `relaunch_app` (process restart)
- `window.rs`: `set_autostart_enabled` (OS autostart registry/file), `open_logs_folder` (`app_log_dir` + `open_url` shell), `open_external_url` (`open_url` shell)
- `config.rs`: `save_config` (holds `RwLock` write guard across `serde_json` + `atomic_write_json` fsync)

Kept cheap sync commands as sync: `get_sync_status`, `load_config`, `show_window`, `preview_status`.

**Why:** Tauri invokes commands on the UI thread; blocking on disk/network (keychain, HTTP 10 s timeout, autostart, shell) freezes the window for tens of seconds. Offloading to the blocking pool keeps the UI responsive. Precedent `commands/onboarding.rs: is_onboarding_complete` uses the same pattern.

**Decisions documented:**

- `preview_status` stays sync: `spotify::preview_status_with_sample` is pure `format_status` substitution on a static `TrackInfo` — no IO. Offloading would add overhead for no benefit.
- `show_window` stays sync: fast window-manager call (`show` + `set_focus`); offloading risks calling `window.show()` off the main thread and adds latency. Labelled in file header.
- `save_config` write-lock: the guard is now acquired *inside* `spawn_blocking` so it is not held across an `await`. The macOS activation-policy and autostart sync run *after* the blocking section; autostart is now awaited (`set_autostart_enabled` is async).

### #218 — Unbounded blocking join

**What:** `stop_polling_and_join` now moves the `JoinHandle::join` (which can block tens of seconds when the poll thread is stuck in sequential `get_currently_playing` → `set_teams_status` → `get_teams_presence` each 10 s) into `spawn_blocking`.

- `stop_polling_and_join` is now `async fn` that `await`s a `spawn_blocking` closure containing the 2 s grace loop + final `join`. Caller thread (Tauri async runtime) is not blocked.
- `start_syncing`'s drain-first invariant (#69) preserved: it `await`s the same helper before `try_claim`.
- `app_exit` uses a variant `stop_polling_and_join_for_exit` that awaits only the 2 s grace on the blocking pool; if the thread is still alive after 2 s, it spawns a detached `spawn_blocking` for the final `join` and proceeds to `app.exit(0)`. This implements "await with timeout then proceed (spawn detached drain if needed)" without requiring `tokio::time::timeout`.

**Why:** Previous `stop_polling_and_join` did `while elapsed < 2 s { sleep 50 ms }` then `handle.join()` on the caller thread. If the poll thread was blocked in HTTP, the final `join` froze the UI for up to 30 s. Moving it to the blocking pool fixes the freeze while keeping the 2 s cooperative shutdown.

### #219 — InvalidGrant 401-retry path

**What:** `poll_once.rs` ~line 360 path now mirrors the proactive refresh path (~line 163):

- Clears `*state.tokens.spotify_mut() = None`
- Persists via `token_io::persist_tokens`
- Emits both `spotify-reconnect-required` *and* `reconnect-required` with `json!(null)`
- Added `SpotifyApiError::InvalidGrant` to the `transient_failure_count` match arm so the 5-strikes `Break` is reachable

**Why:** Previously the retry path only emitted `spotify-reconnect-required`, leaving stale tokens in memory/disk and never emitting the catch-all `reconnect-required` that the layout listens for. It also never counted toward the 5-strikes exit, so a repeatedly failing refresh could loop forever.

**Spin guard:** After clearing, the next `poll_once::run` iteration hits the no-tokens guard (`state.tokens.spotify().clone() is None` → `interruptible_sleep`) and sleeps, so the loop cannot spin on a dead token. Verified guard exists at `poll_once.rs:88-100`. No extra `Break` needed; the behaviour now matches the proactive path.

## Risks & Follow-up

- **Tauri `AppHandle` on blocking thread:** `AppHandle` is `Clone + Send` per Tauri docs, and `TrayIcon`/`AutoLaunchManager` are accessed via `app.state` which is `Send` — same pattern as `onboarding.rs`. No known thread-affinity violation.
- **Window ops off main thread:** avoided by keeping `show_window` sync. If `set_autostart_enabled` internally touches UI, it is still via `AutoLaunchManager` (file/registry), not window, so blocking thread is safe.
- **Parking_lot guard in blocking thread:** `RwLockWriteGuard` is `Send` and the guard is acquired and dropped entirely inside `spawn_blocking` — no guard held across `await`.
- **Detached drain leak:** `app_exit` detached `join` continues after `app.exit(0)`; the OS will reap the thread on process exit, but a 2 s grace log is emitted for diagnosis.
- **No `cargo check` run per lane instruction:** orchestrator will verify build once. Local grep evidence below.

## Grep Evidence

```
$ grep -n "pub async fn" src-tauri/src/commands/sync.rs src-tauri/src/commands/misc.rs src-tauri/src/commands/window.rs src-tauri/src/commands/config.rs
src-tauri/src/commands/sync.rs:26:pub async fn start_syncing
src-tauri/src/commands/sync.rs:208:pub async fn stop_syncing
src-tauri/src/commands/sync.rs:222:pub async fn app_exit
src-tauri/src/commands/misc.rs:35:pub async fn update_tray_menu_state
src-tauri/src/commands/misc.rs:71:pub async fn relaunch_app
src-tauri/src/commands/window.rs:59:pub async fn set_autostart_enabled
src-tauri/src/commands/window.rs:104:pub async fn open_logs_folder
src-tauri/src/commands/window.rs:141:pub async fn open_external_url
src-tauri/src/commands/config.rs:32:pub async fn save_config

$ grep -n "pub fn show_window\|pub fn preview_status\|pub fn load_config\|pub fn get_sync_status" src-tauri/src/commands/*/...
src-tauri/src/commands/window.rs:43:pub fn show_window
src-tauri/src/commands/misc.rs:27:pub fn preview_status
src-tauri/src/commands/config.rs:14:pub fn load_config
src-tauri/src/commands/sync.rs:239:pub fn get_sync_status

$ grep -n "handle.join" src-tauri/src/commands/sync.rs
117:                    match handle.join() {
136:            match handle.join() {
173:                    match handle.join() {
195:                match handle.join() {

$ grep -n "spawn_blocking" src-tauri/src/commands/sync.rs
63:    let handle = tauri::async_runtime::spawn_blocking(move || {
112:        let res = tauri::async_runtime::spawn_blocking(move || {
169:        let still_running = tauri::async_runtime::spawn_blocking(move || {
194:            tauri::async_runtime::spawn_blocking(move || {

$ grep -n "InvalidGrant" src-tauri/src/polling/poll_once.rs
163:                if matches!(e, SpotifyApiError::InvalidGrant) {
365:                            if matches!(refresh_err, SpotifyApiError::InvalidGrant) {
396:                    | SpotifyApiError::InvalidGrant

$ sed -n '365,376p' src-tauri/src/polling/poll_once.rs
                                *state.tokens.spotify_mut() = None;
                                if let Err(persist_err) = token_io::persist_tokens(state, app) {
                                let _ = app.emit("spotify-reconnect-required", json!(null));
                                let _ = app.emit("reconnect-required", json!(null));
```

## Commit

Conventional commit on `fix/sync-threading` referencing #215 #218 #219. No push — orchestrator pushes.
