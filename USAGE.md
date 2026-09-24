# Usage

Day-to-day guide to running PresenceJam.

## The System Tray

PresenceJam lives in your **system tray** (Windows taskbar or macOS menu bar). The app window is hidden by default to keep your taskbar clean.

**Tray icon behavior:**
- **Left-click** — open/focus the PresenceJam window
- **Right-click** — open the tray menu

**Tray menu options:**

![Tray menu](docs/screenshots/tray-menu.png)

| Option | What it does |
|--------|-------------|
| Show Window | Bring the app window to the foreground |
| Pause / Resume Sync | Stop polling Spotify (Teams status stays as-is) / start it again after a pause |
| Pause sync for… | Snooze the sync: **30 minutes**, **1 hour**, or **until tomorrow** (the next *local* midnight). While a snooze is active the app makes no Spotify or Teams request at all and leaves your existing Teams status untouched. |
| Resume sync now | Shown only while a snooze is active — ends it immediately. The Dashboard's snooze chip has the same button. |
| Play / Pause | Toggle playback on your active Spotify device |
| Previous | Skip to the previous track |
| Next | Skip to the next track |
| Shuffle | Toggle Spotify shuffle. The check mark mirrors the state Spotify reports. |
| Repeat | Cycle repeat mode: **Repeat: Off** → **Repeat: Context** → **Repeat: Track** → back to off. The label always spells the current mode out, because a check mark alone cannot tell *context* from *track*. |
| Devices | List your Spotify devices — picking one transfers playback there and starts it |
| Open Settings | Jump straight to the Settings view |
| Open Logs Folder | Open the log directory in your OS file manager |
| Up Next | Peek at the next tracks or episodes in your queue (up to 3). Rows read `{artist} - {title}`, so an episode shows as `Show name - Episode name`. |
| Quit | Fully exit the app |

> **Tray playback** (Play/Pause, Previous, Next, Shuffle, Repeat, Devices, Up Next) requires a **Spotify Premium** account and a one-time reconnect that adds the `user-modify-playback-state` scope — needed only if your Spotify tokens predate that scope (see [SETUP.md — Upgrading from 2.x](./SETUP.md#upgrading-from-2x)). Until then, Settings shows a "Playback control needs a one-time reconnect" banner. Shuffle and Repeat need an **active device** as well; with none, the action fails and an in-app toast reads "No active playback device - pick one from the tray Devices menu". A non-Premium account fails with "Playback control requires Spotify Premium". A failed toggle leaves the check mark untouched, so the menu never claims a state Spotify refused.

> **Closing the window (X button) doesn't quit the app** — it minimizes to the tray. This is intentional so sync keeps running in the background. Use **Quit** from the tray menu to fully exit.

---

## Dashboard

![Dashboard](docs/screenshots/dashboard.png)

The main screen showing your current sync status.

**Connection status badges:**
- Green — signed in — credentials are stored; the next poll re-checks them
- Red — not connected or token expired — follow the Reconnect view to sign in again

**Sync toggle:**
- An icon-only **▶** / **⏸** button in the Dashboard header — its tooltip and screen-reader label read **Resume sync** / **Pause sync**
- **▶** starts polling Spotify and updating your Teams status; **⏸** pauses polling, and your Teams status remains unchanged

**Currently playing card:**
- Shows the active track (artist, track name, album art if available) — or the active podcast/audiobook episode, with the show name where the artist goes and the publisher where the album goes
- Updates in real-time as tracks change
- Shows ⏸️ when nothing is playing, including during adverts, which are never treated as "listening"

**Suppressed chip:** when a write is being held back, the card shows a chip saying why. The wording is reason-specific for the four rules/policies — *quiet hours are active*, *a track rule matched*, *you set a status message by hand*, *you are out of office* — and falls back to a generic *busy, in a call, or presenting* line for a presence-based verdict (busy / Do Not Disturb / focusing / in a meeting / in a call / presenting). The reason survives a view switch, because it is part of the shared presence state rather than a one-off event payload, so the chip keeps its reason-specific wording.

**Snooze chip:** while a tray snooze is active the Dashboard shows a countdown chip — *Snoozed — 12:34 left (until 14:30)* — with a **Resume now** button, so a snooze started from the tray is visible (and cancellable) in the window too. The tray tooltip leads with its status line and states the remaining time for the same reason.

---

## Settings
Unsaved changes are tracked per section: a **"You have unsaved changes"** banner appears when you edit anything, and each section has a **Reset** button that restores the shipped defaults for that section. Values that fall outside allowed ranges (e.g. polling intervals) are clamped with inline feedback before saving.

### Status Format

Edit the template that formats your Teams status message. The default is `🎵 {artist} - {track} 🎧`.

| Placeholder | Renders |
|-------------|---------|
| `{artist}` | Artist name — on an episode, the **show** name |
| `{track}` | Track name — on an episode, the **episode** name |
| `{album}` | Album name — on an episode, the **publisher** |
| `{emoji}` | 🎵 while a track plays, 🎙️ while an episode plays, ⏸️ when paused |
| `{device}` | The name of the device Spotify is playing on |
| `{playlist}` | The playlist/album/artist/show the item was started from. `{context}` is an exact alias for it. |
| `{progress}` | Playback position (`minutes:seconds` — `3:07`, `90:00`). Empty when Spotify reports no position, e.g. a live stream. |
| `{shuffle}` | 🔀 while shuffle is on, nothing while it is off |
| `{repeat}` | 🔁 while repeat is on (either *context* or *track*), nothing while it is off |
| `{show}` | The podcast/audiobook name — only on an episode, empty on a music track |
| `{episode}` | The episode name — only on an episode, empty on a music track |
| `{publisher}` | The publisher — only on an episode, empty on a music track |

**Example:** `🎵 {artist} - {track} 🎧` → `🎵 Daft Punk - One More Time 🎧`

**Substitution is a single pass.** Tokens are replaced once, left to right, and the text a token produces is never re-scanned — a track literally named `{album}` is shown as `{album}`, not expanded into the album name. Tokens the app does not recognise, and an unclosed `{`, are left exactly as you typed them.

The **live preview** below the field renders against a fixed sample item — device *Kitchen speaker*, playlist *Workout Mix*, position `0:00`, shuffle and repeat on — so the mode tokens are visible before anything is playing.

> **Podcasts and audiobooks use their own template** — `🎙️ {show} - {episode}` — so a 90-minute episode never gets your music template. That template is built into the app; there is no setting for it yet.

### Teams Status

| Setting | Default | Description |
|---------|---------|-------------|
| Clear on pause | On | Clears your Teams status when Spotify pauses or stops. There is no Settings toggle for this — edit `teams.clear_on_pause` in `config.json` directly (consumed at `src-tauri/src/polling/poll_once.rs`). |
| Profanity filter | On | Replaces profane track/artist names with a safe placeholder |
| Profanity placeholder | `Currently Listening to Spotify` | Shown when a track name is filtered. Supports `{emoji}`. |
| Custom words to filter | empty | Extra words/phrases, one per line, bounded to the first **64 entries of 32 characters** and shown with the same clamp feedback as the polling fields. A word you add is matched with the same rules as the built-in list — word boundaries are respected (adding `spam` does not flag `spamalot`), and the usual evasions (`s.p.a.m`, `5pam`) are caught — so the effect is visible immediately in the status preview below. |

### Presence

| Setting | Default | Description |
|---------|---------|-------------|
| Show Available while listening | Off | Sets your Teams presence to **Available** while a track plays (re-armed every few minutes). The requested session length is the **remaining listening time plus one re-arm period**, clamped to Microsoft's documented `PT5M`–`PT4H` window — so a crash or a force-quit no longer leaves you green for four hours when the music stopped. A live/unknown-position stream has no remaining time to bound with, so it still asks for `PT4H`. |
| Pause status during meetings/calls/DND | On | Reads your Teams presence before writing a status update and skips the write while you're busy, in a meeting, in a call, or presenting. The status resumes once your presence clears — the app keeps polling, it is the Teams *write* that is skipped. |
| Never overwrite a status I set by hand | On | If your Teams status message is not one PresenceJam posted (and has not expired), the app leaves it alone rather than replacing it with the track. It reuses the presence read the gate already performs, so it costs no extra request; a message you set that then expires stops blocking, and the next track posts normally. If the presence read itself fails, the write proceeds (nothing is held back on a failed read). |
| Pause while I am out of office | Off | Skips the status update while Teams reports you out of office (either the out-of-office setting or an `outOfOffice` activity). A quiet-hours row or track rule that carries its own presence action overrides it for that track. |

> Both toggles need a one-time Teams reconnect if your tokens predate the `Presence.Read` and `profile` scopes — those are only granted on a fresh sign-in (see [SETUP.md — Upgrading from 2.x](./SETUP.md#upgrading-from-2x)).

> The two policy rows are the newest of the four; they need the same one-time Teams reconnect if your tokens predate the `Presence.Read` scope.

### Status rules

Quiet hours and track rules either **suppress** the Teams status write or **replace** it, and each can also move your Teams presence while it applies. They reuse the same presence-gate path as the meeting/call gate, so a rule that stops matching mid-track posts the status without waiting for the next track.

**Quiet hours** — each entry has an on/off checkbox, a start and an end time, and a weekday picker. Times wrap around midnight (a new entry starts at 22:00 → 07:00). **No weekday ticked means every day.** Each entry also carries a **Stop polling during this window** checkbox (off by default): while such a window is active the app makes no Spotify or Teams request at all, touches no write clock and leaves your Teams status exactly as it is — the poller re-checks the window every iteration and resumes by itself when it ends, and the thread is never stopped or parked. The log shows one transition pair, `[POLLING] quiet hours: polling paused until …` and the resume line.

Each quiet-hours row also has a **Presence while this rule applies** picker. It offers *Don't change my presence* plus the five `(availability, activity)` pairs Microsoft's `setPresence` accepts — **Available/Available**, **Busy/InACall**, **Busy/InAConferenceCall**, **Away/Away** and **DoNotDisturb/Presenting**. Anything else is rejected at the config boundary and cleared rather than guessed at. The action is **inert while "Show Available while listening" (availability sync) is off**, and it never overrides a call, a meeting, or a status you set by hand.
**Track rules** — each entry matches case-insensitively on artist and/or track-title substrings, plus an optional replacement status:

| Field | Behaviour |
|-------|-----------|
| Artist contains | Matched against the track's artist. Empty = any artist. |
| Track title contains | Matched against the track title. Empty = any title. |
| Active days | The weekdays this rule applies on — none ticked means every day. |
| Window start / Window end | The rule's own time window, with the same wrap-over-midnight semantics as quiet hours: an end time of `00:00` means the end of the day, and a start equal to the end never matches. The default `00:00 → 24:00` contains every time. |
| Post this instead | Non-empty — this text is posted instead of the formatted status. **Empty — the status write is suppressed entirely** for the matching track. |
| Presence while this rule applies | *Don't change my presence* (default), or one of the five supported availability/activity pairs — armed while the rule matches, including on the nothing-playing path for time-based quiet-hours windows (a scoped track rule's stale pair still clears when its track ends). |

New track rules are added **disabled**, so a half-filled rule can't suppress your status by accident. Rules are evaluated **top to bottom and the first matching rule wins**, and the card's move-up / move-down buttons change that order — put the narrow rule above the broad one. A rule whose window does not contain the current time, or whose active-day set does not include today, simply does not match. A suppressed track is re-checked every 240 s (the same clock as the presence gate), so clearing the rule or leaving the window posts the status mid-track.

**A rule with "Post this instead" left empty suppresses the write — but it still moves your presence**: that is the point of pairing them ("while my Focus playlist plays, show me Do Not Disturb" needs no status text at all). A rule with replacement text posts that text instead of the formatted status; it still flows through the identical-write skip, so an unchanged replacement is not re-posted every cycle.

**Quiet hours win over track rules.** If a quiet-hours row covers the current time and day, it decides the iteration — the matching track rule is not consulted at all, so its replacement text and its presence pair do not apply.

**Replacement text is capped at 128 characters** and your quiet-hours replacement is posted from both the playing path *and* the stop/pause clear, so a rule you set is what ends up in Teams either way.

**Pause and stop status text.** The two texts posted when playback pauses (`Paused` by default) and when nothing is playing (`Nothing playing on Spotify`) are editable at the bottom of this card — `teams.paused_status_format` / `teams.stopped_status_format` in `config.json`. The 🎵 prefix is added for you, and clearing a field restores the shipped default, so the rendered text is unchanged unless you change it. A matching rule's replacement status still takes precedence over both.

Both lists live in `config.json` under `status_rules` (`quiet_hours[]`, `track_rules[]`), and the 4.7 schedule fields (`days`, `start_minutes`, `end_minutes`), `quiet_hours[].pause_polling` and the two status texts are additive with serde defaults, so a pre-4.7 (or pre-4.5) config file loads unchanged (#432).

### Polling

| Setting | Default | Description |
|---------|---------|-------------|
| Default interval | 30s | The baseline poll gap when a track is playing but Spotify reports no playback position (live streams, #165) — slider range 10–60 s. It is also the base for the pause backoff: each consecutive non-playing response doubles it (30 → 60 → 120 s) up to the paused-backoff ceiling (300 s by default), resetting on the next playing track. |
| Min interval | 10s | Floor for the track-end smart sleep — while a track plays, PresenceJam never polls sooner than this (5–30 s in Settings). |
| Max interval | 60s | Ceiling for the track-end smart sleep — while a track plays, PresenceJam never sleeps longer than this (up to 300 s in Settings). |
| Paused backoff ceiling | 300s | The upper bound for the pause backoff ladder (30 → 60 → 120 s → this value), accepted in the range 60–3600 s with inline clamp feedback. Raising it lets the ladder climb further before settling (fewer idle API calls); lowering it settles sooner. A value below the base interval is floored at the base, so the ladder never shrinks as pauses accumulate. |

All four are clamped by the backend (`config.rs::clamp_polling`): default 5–300 s, min 5–30 s, max between min and 300 s, and the pause ceiling 60–3600 s. The Settings form previews the clamped values before saving ("Min interval exceeds max interval — max will be saved as {max}s.").

### Notifications

| Setting | Default | Description |
|---------|---------|-------------|
| Notify me when the track changes | On | A system notification when the track changes — throttled to at most one per 5 s, and the same track is never notified twice. Where the platform supports it the newest notification replaces the previous one in place instead of stacking. |
| Notify me when syncing stops on its own | On | Says when the poller gave up by itself (an auth failure past the retry limit, or the polling thread ending). A **Pause Sync** you clicked yourself is deliberately *not* reported back to you. |
| Notify me when I have to sign in to Teams again | On | Fires when a Teams session expires and needs a fresh sign-in. A **Reconnect** you just pressed is skipped — only sessions that expire under you notify. |
| Notify me when an update will install on quit | On | Fires once, when a deferred *Install on quit* update finished downloading and its signature verified. |

All four classes are stored in `config.json` under `notifications` (`track_change`, `sync_stopped`, `auth_required`, `update_staged`); an install that still carries the pre-4.7 `notificationsEnabled` flag migrates it into `track_change` once, on the first save. Track changes keep their 5 s throttle and replace-in-place id; the other three notify at most once per occurrence. The track-change toast is dispatched by the Dashboard, the other three by the always-mounted layout, so they arrive whichever view is on screen — and the first one you enable may ask your OS for notification permission.

### Appearance

| Setting | Default | Description |
|---------|---------|-------------|
| Theme | Dark | **Dark**, **Light** or **System**. *System* follows your operating system's appearance live — switching the desktop between light and dark repaints the app immediately, with no restart — while an explicit Dark or Light stays pinned and is never overridden by the OS. The pre-paint bootstrap resolves the stored preference, so a System user never sees a flash of the wrong theme on launch. |
| Compact spacing | Off | Tightens the spacing and type scale (a token-scale override applied before first paint, not a set of component variants). It is independent of the theme, including the System option. |
| Launch at login | Off | Start PresenceJam automatically when your OS boots |
| Language | System | Interface language: English, Deutsch (German), or Français (French). Defaults to your OS/browser language. The choice is stored in `config.json` (`locale`) and is the single source of truth for the window, the tray menu and the native application menu — an unknown value falls back to English. Switching it also retags `<html lang>` for screen readers and applies the locale's number and plural rules (French counts `0` as singular). Detached Logs and Settings windows follow the switch. |
| Start minimized | Off | Open the app minimized to the tray (window hidden on launch). There is no Settings toggle — set `teams.start_minimized` to `true` in `config.json` (consumed at `src-tauri/src/lib.rs`). On macOS, it also switches the app's activation policy to `Accessory`, removing the dock icon and menu-bar app menu — the app becomes a pure tray-resident app. The dock icon reappears when you set the field back to `false` (no restart needed). |

### Logging

| Setting | Default | Description |
|---------|---------|-------------|
| Write a log file | On | Turns the on-disk log off entirely; the in-app Log Viewer still works from the live buffer. Takes effect immediately. |
| Log level | Info | `Off`, `Error`, `Warn`, `Info`, `Debug` or `Trace`. Takes effect immediately. |
| Maximum log file size (MB) | 10 | The live file rotates once it reaches this size. Accepted range 1–500 MB. |
| Archived log files to keep | 3 | How many rotated files are kept. Accepted range 1–20. The live log is kept **in addition** to the archives, so the folder holds at most `keep_files + 1` files — one more than the number in the field. |

The size and retention fields apply from the next launch (the file already being written is not one of the archives); the level and the on/off switch apply at once. **Open logs folder** opens the same directory as tray → **Open Logs Folder**.

### Backup

| Action | What it does |
|--------|--------------|
| Export settings… | Writes a copy of your settings to a file you choose, with a dated, version-stamped name. It **never** contains your Spotify client secret (that lives in the OS keychain; Settings shows only *whether* one is set) or any token material. The resolved path is shown in the card. |
| Import settings… | Replaces your settings from a chosen file, after a confirmation that names the `config.json.bak` backup it keeps. The document is parsed, refused outright if it carries a plaintext `client_secret`, clamped to the same ranges the Settings form enforces, and the running app reloads the imported values. The resolved path is shown in the card. |

### Global shortcuts

Two app-wide bindings, both editable in this card:

| Slot | Default | What it does |
|------|---------|--------------|
| Toggle playback | `CmdOrCtrl+Alt+P` | Play/pause on your active Spotify device — the same refresh-aware path the tray and the Dashboard button use. |
| Pause or resume sync | `CmdOrCtrl+Alt+S` | Starts or stops the poller, exactly like the tray's Pause / Resume Sync. |

They work while the window is hidden. Click a field and press the combination you want — the field records what you press, and the current grab is released while it records so the key is captured instead of fired. **Clear** empties a slot. A combination that cannot be parsed, carries no modifier (see below), or collides with the other slot is refused inline; a desktop that refuses the grab (some Wayland compositors, or a combination another app already owns) shows **Registration failed on this desktop** with the reason, and the other binding — and the rest of the app — keeps working.

A binding needs at least one modifier (Ctrl, Alt, Shift, or Cmd on macOS) — a bare key such as `Escape`, `Enter`, `Space`, or `P` is refused with “needs at least one modifier”, because the grab is system-wide and would swallow that key in every application. Function keys (`F1`–`F24`) and dedicated media keys (`MediaPlayPause`, `MediaStop`, `MediaTrackNext`, …) are the exception: they have no typing role, so they bind bare. While recording, pressing bare `Escape` or `Enter` cancels the capture instead of binding.

---

## Updates

On startup, PresenceJam checks GitHub Releases for a newer version, then re-checks silently every ~24 hours while the app runs. If a new version is found, a small banner appears at the top of the window: **"Update vX.Y.Z available"** with two choices:

- **Download & Install** — downloads with a progress readout and relaunches into the new version immediately.
- **Install on quit** — opens a confirmation showing the staged version against your current version, with install/skip. The download then runs **in the background with a live percentage in the banner** (`Preparing update — 42%`, or `Preparing…` when the server sends no payload size), so a multi-minute stage is no longer a black box. A **Cancel** button next to the progress readout discards the staged update; a cancel issued while the download is still running cannot stop the transfer already in flight, so it marks the stage abandoned and throws the payload away the moment it lands. Either way the banner returns to its plain offer. The verified update is applied the next time you quit the app (tray → Quit). If the staged update is stale (same version or older than what you're running), it is skipped instead of installed — the banner shows a skipped state with an **Install anyway** override if you really want it (#431). On Windows the installer relaunches the app; on macOS/Linux the new version is picked up on your next launch.

The banner is dismissible, and a failed *check* (offline, unreachable endpoint, mismatched signature key) is silent — it never blocks the UI.

Update payloads are signature-verified against a key baked into the app (minisign), which is independent of OS code signing — so the macOS unsigned/Gatekeeper note in the README applies to updated builds too. Deferred "Install on quit" updates go through the same verification before they're staged, and are held in memory only — cancelling releases them instead of leaving a file behind.

**Release channel.** Settings → Updates chooses between **Stable** and **Beta**, and the backend resolves the update candidate from that choice (`updater_bg::check_for_update`) rather than from the plugin's static JS check. Beta builds use the rolling beta feed when available; if that feed is missing or has no newer version, Beta falls back to the stable release. The choice also decides *how* an update installs: on Stable the banner's **Download & Install** works as described above, while on Beta only **Install on quit** is offered, because the immediate download-and-relaunch path is hard-wired to the stable endpoint.

---

## Detachable Windows

The **Logs** and **Settings** views can each be popped out into their own window (and back):

- In the main window, use the **Pop out** control on the Logs or Settings header. The pane opens in its own OS window (`logs-detached` / `settings-detached`).
- In a detached window, the **Pop back in** button returns the pane to the main window and closes the detached window.
- While a pane is detached, the main-window nav shows a dot badge next to it; clicking it focuses the detached window instead of navigating.

Detached windows share live state with the main app — sync keeps running regardless of how the UI is arranged.

---

## Diagnostics Page

The 🩺 button in the Dashboard header opens the **Diagnostics** page: a one-click local support snapshot containing app/Tauri/OS versions, a sanitized config summary, token *metadata* only (expiry timestamps and presence flags — never token values), keychain presence flags, and the last 50 log lines passed through a redaction pass.

**Copy** puts the displayed snapshot on your clipboard. **Save to file** asks the Rust `save_diagnostics_snapshot` command to collect a fresh typed snapshot, keeps it within the independent 256 KiB limit, and atomically writes a timestamped `presencejam-diagnostics-…json` file in the platform Downloads directory; the page supplies neither the file contents nor the destination. Rust tries up to eight generated filenames without replacing an existing file. If the save fails, the page reports **"Save failed — use \"Copy diagnostics\" instead."** and the displayed in-memory snapshot remains available to copy. The page makes **no network calls** — nothing leaves your machine unless you paste or attach the snapshot yourself.

If `config.json` was ever unreadable, the page opens with an amber **Settings were reset** banner on top of that snapshot: it says the settings file could not be read, names the `config.json.bak` backup so you can find it, and points at the folder it lives in. The banner also appears on a **later** launch, because the backup on disk outlives the run that produced it. Dismissing it only hides it for this session — nothing on disk is deleted, since the backup is the only copy of the settings you lost (see [TROUBLESHOOTING.md — The app came up with default settings](./TROUBLESHOOTING.md#the-app-came-up-with-default-settings)).

---

## Log Viewer

PresenceJam writes `PresenceJam.log` — plus the archives it rotates into — managed by the logging plugin:

```
%LOCALAPPDATA%\com.presencejam.app\logs\PresenceJam.log   (Windows)
~/Library/Logs/com.presencejam.app/PresenceJam.log        (macOS)
~/.local/share/com.presencejam.app/logs/PresenceJam.log   (Linux)
```

The live file rotates once it reaches **Settings → Logging → Maximum log file size** (`logging.max_file_size_mb`, default 10 MB) and keeps the number of archives set beside it (`logging.keep_files`, default 3). The live log is kept **in addition** to the archives, so the folder holds at most `keep_files + 1` files.

The log directory is the **bundle-identifier folder** (`com.presencejam.app`): Tauri's `app_log_dir()` appends the bundle id to the platform's local data directory, so `PresenceJam.log` does *not* sit next to `config.json` (issue #300). This is the directory tray menu → **Open Logs Folder** opens.

The **Log Viewer** in-app lets you browse these logs without opening the filesystem — and can be popped out into its own window (see *Detachable Windows* above). It opens already filled with the **last 500 lines of the on-disk file** — read off the UI thread and capped at the trailing 256 KiB — so the history is there before the first new line is logged. While you are scrolled away from the bottom the pane holds your place as entries arrive instead of sliding the text under you; **Jump to latest** pins it back to the bottom. **Clear** empties the pane and suppresses that initial backfill for as long as the window stays open. You can also open the folder directly via tray menu → **Open Logs Folder**.

**Log levels:**

| Level | Meaning |
|-------|---------|
| `ERROR` | Something failed — API error, file I/O error |
| `WARN` | Unexpected but recoverable (e.g., slow network) |
| `INFO` | Normal operations (track changed, status updated) |
| `DEBUG` | Verbose — every polling iteration logged |
| `TRACE` | Very verbose — adds the per-iteration detail a support report needs |
| `OFF` | Silences the logger (nothing is recorded), exactly like clearing **Write a log file** |

The current log level is set in **Settings → Logging** (the same value lives in `config.json` under `logging.log_level`), and that card also turns file logging off entirely and sets the size/retention fields above.

---

## Command-line flags

PresenceJam is a tray app, but the binary also answers three CLI flags — handy from a script, a
cron job or a support session. None of them opens the app window. Any *other* argument is
ignored and the app starts normally, exactly as it always has (that includes the autostart
plugin's `--minimized` and a `presencejam://` deep-link URL, which are both normal GUI
launches).

| Command | What it does |
|---------|--------------|
| `presencejam --status` | Prints the sync status as **JSON on stdout** and exits `0`. The fields are the ones the app's `get_sync_status` command returns: `is_syncing`, `current_track`, `spotify_connected`, `teams_connected`, `last_posted_status`, `presence_gated`, `presence_paused`. It builds no window and no tray icon and takes no single-instance lock, so it works on a **headless machine**. A freshly started process has no poller, so `is_syncing` is `false`, `current_track` is `null` and the presence fields are empty — the values describe *that* process. `spotify_connected` / `teams_connected` are read from the same `config.json` and `tokens.json` the app uses. If either file cannot be read, the JSON is still printed (both providers reported as disconnected) and the reason goes to **stderr**. |
| `presencejam --sync-once` | Runs **exactly one poll iteration** — including the Teams status write — and exits `0` on success or `1` with the reason on **stderr**. Requires a configured Spotify `client_id` and a sign-in to **both** Spotify and Teams; without them it exits `1` before doing anything else. Logs go to the normal log file. |
| `presencejam --help` | Prints the usage text — the three flags above plus `--minimized` — and exits `0`. Fully headless. |
| `presencejam --minimized` | Starts with the window hidden (what the autostart plugin passes at login). This is a normal GUI launch. |

On Windows the release build is a GUI-subsystem executable, so it owns no console of its own:
run the flags from a console — `cmd` or PowerShell, e.g. `cmd /c presencejam --help` — and the
output prints in that window. A redirected stdout/stderr (a pipe or a file) is left alone, so
`presencejam --status | jq …` keeps working; launching the exe with no console at all (for
example by double-clicking it) still starts the GUI normally.

Both `--status` and `--sync-once` read `config.json` and `tokens.json` directly, so the app
does **not** have to be running. `--sync-once` does not take the single-instance lock either,
so it can run next to a running app; the app's own polling keeps going. Like the loop, it honours an active snooze and an active quiet-hours `pause_polling` window by performing no request — there is no override flag. Running a one-shot
while a track plays simply posts the status that track deserves — a later iteration with an
unchanged status is deduplicated exactly as usual.

### Headless use, and what `--sync-once` needs

`--status` and `--help` need no desktop at all: they never build a Tauri app, so they work on a
bare server, in a container, or over SSH.

`--sync-once` is different on Linux: it runs the **same poller the app runs**, and that poller
talks to the app's Tauri runtime, which requires a display server. On a headless machine wrap it
— `xvfb-run -a presencejam --sync-once` — from cron or a CI step. With no display at all the
process aborts during runtime creation (an abnormal exit, not one of the codes below), because
the GUI runtime itself cannot start. The credential gate still runs first, so a machine with no
Spotify/Teams sign-in exits `1` with the reason *before* reaching that point.

Exit codes:

| Code | Meaning |
|------|---------|
| `0` | The iteration completed. No track playing, a deduplicated write, and a gate-suppressed write all count as completions. |
| `1` | A transient or auth failure, with the poller's own reason on **stderr** — e.g. `spotify: Failed to get currently playing: …`, `teams: Failed to update status: …`, `reconnect-required: a provider needs to be reconnected`. Also used before anything else happens when the config has no Spotify `client_id`, or the tokens for Spotify/Teams are missing or unreadable. |
| other | The app could not start at all (no display server, or a Tauri build failure) — check the log file and stderr. |

The first recognised flag wins if you pass more than one.

---

## Status Expiry

Teams custom status messages automatically expire. PresenceJam sets the message's expiry (`expiryDateTime`) to the **track's end time + a buffer** (default 10 s; `polling.expiry_buffer_seconds` in `config.json`). This is an app-side choice — the Graph API doesn't shorten it. When playback pauses or stops, PresenceJam replaces the message with the pause/stop placeholder — by default `🎵 Paused` and `🎵 Nothing playing on Spotify`, both editable in Settings → Status rules → *Pause and stop status text* (`teams.paused_status_format` / `teams.stopped_status_format`; the 🎵 is added for you, and clearing a field restores these defaults) — and that placeholder **expires 60 s after it is posted** (`placeholder_expiry_str()` in `src-tauri/src/polling/poll_once.rs` sets a fixed now + 60 s, on both the paused and the no-track path). The pause placeholder does *not* inherit the track-end buffer. Graph has no "clear status message" action, so the short-lived placeholder is the documented clear mechanism: it self-removes ~1 min after the last successful post even if the app quits.

---

## Token Refresh

Tokens refresh automatically:
- **Spotify tokens** — refreshed by PresenceJam when needed (no action required)
- **Teams tokens** — same. The device-code sign-in requests `offline_access`, so Microsoft issues a refresh token that PresenceJam rotates before the access token expires.

If a token expires mid-session, PresenceJam retries once with a fresh token before asking you to sign in again — since v4.2.0 you only see a reconnect prompt when the refresh itself fails (#428). If your connection drops unexpectedly, PresenceJam opens the Reconnect view. You can also re-authenticate the affected service anytime with its **Reconnect** button in **Settings**.

---

## Switching Accounts

To switch your Spotify or Teams account — there is no Disconnect button:

1. Open PresenceJam → **Settings**
2. Under the service you want to switch, click **Reconnect** — this clears the stored tokens and starts a fresh sign-in
3. Complete the browser sign-in with the other account

If the browser signs you straight back into the old account, quit the app, delete `tokens.json` (`%APPDATA%\com.presencejam.app\PresenceJam\` on Windows, `~/Library/Application Support/com.presencejam.app/PresenceJam/` on macOS, `$XDG_CONFIG_HOME/com.presencejam.app/PresenceJam/` on Linux), restart, and re-onboard.
