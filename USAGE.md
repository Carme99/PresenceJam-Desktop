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
- Green — connected and authenticated
- Red — not connected or token expired — follow the Reconnect view to sign in again

**Sync toggle:**
- An icon-only **▶** / **⏸** button in the Dashboard header — its tooltip and screen-reader label read **Resume sync** / **Pause sync**
- **▶** starts polling Spotify and updating your Teams status; **⏸** pauses polling, and your Teams status remains unchanged

**Currently playing card:**
- Shows the active track (artist, track name, album art if available) — or the active podcast/audiobook episode, with the show name where the artist goes and the publisher where the album goes
- Updates in real-time as tracks change
- Shows ⏸️ when nothing is playing, including during adverts, which are never treated as "listening"

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

### Presence

| Setting | Default | Description |
|---------|---------|-------------|
| Show Available while listening | Off | Sets your Teams presence to **Available** while a track plays (re-armed every few minutes, cleared on pause). It shows *Available*, not *Busy* — Microsoft's `setPresence` API only supports the Busy/**InACall** combination, so "busy" would display an in-call bubble to your colleagues. |
| Pause status during meetings/calls/DND | On | Reads your Teams presence before writing a status update and skips the write while you're busy, in a meeting, in a call, or presenting. The status resumes on the next track change once your presence clears. |

> Both toggles need a one-time Teams reconnect if your tokens predate the `Presence.Read` and `profile` scopes — those are only granted on a fresh sign-in (see [SETUP.md — Upgrading from 2.x](./SETUP.md#upgrading-from-2x)).

### Status rules

Quiet hours and track rules suppress the Teams status write. They reuse the same presence-gate path as the meeting/call gate, so a rule that stops matching mid-track posts the status without waiting for the next track.

**Quiet hours** — each entry has an on/off checkbox, a start and an end time, and a weekday picker. Times wrap around midnight (a new entry starts at 22:00 → 07:00). **No weekday ticked means every day.**

**Track rules** — each entry matches case-insensitively on artist and/or track-title substrings, plus an optional replacement status:

| Field | Behaviour |
|-------|-----------|
| Artist contains | Matched against the track's artist. Empty = any artist. |
| Track title contains | Matched against the track title. Empty = any title. |
| Post this instead | Non-empty — this text is posted instead of the formatted status. **Empty — the status write is suppressed entirely** for the matching track. |

New track rules are added **disabled**, so a half-filled rule can't suppress your status by accident. A suppressed track is re-checked every 240 s (the same clock as the presence gate), so clearing the rule or leaving the quiet-hours window posts the status mid-track.

Both lists live in `config.json` under `status_rules` (`quiet_hours[]`, `track_rules[]`); the section is fully additive, so pre-4.5 config files load unchanged (#432).

### Polling

| Setting | Default | Description |
|---------|---------|-------------|
| Default interval | 30s | The baseline poll gap when a track is playing but Spotify reports no playback position (live streams, #165) — slider range 10–60 s. It is also the base for the pause backoff: each consecutive non-playing response doubles it (30 → 60 → 120 s) up to a 300 s cap, resetting on the next playing track. |
| Min interval | 10s | Floor for the track-end smart sleep — while a track plays, PresenceJam never polls sooner than this (5–30 s in Settings). |
| Max interval | 60s | Ceiling for the track-end smart sleep — while a track plays, PresenceJam never sleeps longer than this (up to 300 s in Settings). |

All three are clamped by the backend (`config.rs::clamp_polling`): default 5–300 s, min 5–30 s, and max between min and 300 s. The Settings form previews the clamped values before saving ("Min interval exceeds max interval — max will be saved as {max}s.").

### Notifications

| Setting | Default | Description |
|---------|---------|-------------|
| Desktop notification on track change | Off | Shows a system notification when the track changes. Turn it on here; the first enable asks the OS for notification permission. The flag is stored in `localStorage.notificationsEnabled`, **not** in `config.json`. |

Notifications are throttled to at most one per 5 s, and the same track is never notified twice. Where the platform supports it the newest notification replaces the previous one in place instead of stacking. The listener lives in `Dashboard.svelte`, so notifications are driven by the main window.

### General

| Launch at login | Start PresenceJam automatically when your OS boots |
| Language | Interface language: English, Deutsch (German), or Français (French). Defaults to your browser/OS language; the choice persists. Switching it also retags `<html lang>` for screen readers and applies the locale's number and plural rules (French counts `0` as singular). Detached Logs and Settings windows follow the switch. |
| Start minimized | Open the app minimized to the tray (window hidden on launch). There is no Settings toggle — set `teams.start_minimized` to `true` in `config.json` (consumed at `src-tauri/src/lib.rs`). On macOS, it also switches the app's activation policy to `Accessory`, removing the dock icon and menu-bar app menu — the app becomes a pure tray-resident app. The dock icon reappears when you set the field back to `false` (no restart needed). |

---

## Updates

On startup, PresenceJam checks GitHub Releases for a newer version, then re-checks silently every ~24 hours while the app runs. If a new version is found, a small banner appears at the top of the window: **"Update vX.Y.Z available"** with two choices:

- **Download & Install** — downloads with a progress readout and relaunches into the new version immediately.
- **Install on quit** — opens a confirmation showing the staged version against your current version, with install/skip. The download then runs **in the background with a live percentage in the banner** (`Preparing update — 42%`, or `Preparing…` when the server sends no payload size), so a multi-minute stage is no longer a black box. A **Cancel** button next to the progress readout discards the staged update; a cancel issued while the download is still running cannot stop the transfer already in flight, so it marks the stage abandoned and throws the payload away the moment it lands. Either way the banner returns to its plain offer. The verified update is applied the next time you quit the app (tray → Quit). If the staged update is stale (same version or older than what you're running), it is skipped instead of installed — the banner shows a skipped state with an **Install anyway** override if you really want it (#431). On Windows the installer relaunches the app; on macOS/Linux the new version is picked up on your next launch.

The banner is dismissible, and a failed *check* (offline, unreachable endpoint, mismatched signature key) is silent — it never blocks the UI.

Update payloads are signature-verified against a key baked into the app (minisign), which is independent of OS code signing — so the macOS unsigned/Gatekeeper note in the README applies to updated builds too. Deferred "Install on quit" updates go through the same verification before they're staged, and are held in memory only — cancelling releases them instead of leaving a file behind.

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

**Copy** puts the snapshot on your clipboard; **Save to file** writes it next to your logs. The page makes **no network calls** — nothing leaves your machine unless you paste or attach the snapshot yourself.

If `config.json` was ever unreadable, the page opens with an amber **Settings were reset** banner on top of that snapshot: it says the settings file could not be read, names the `config.json.bak` backup so you can find it, and points at the folder it lives in. The banner also appears on a **later** launch, because the backup on disk outlives the run that produced it. Dismissing it only hides it for this session — nothing on disk is deleted, since the backup is the only copy of the settings you lost (see [TROUBLESHOOTING.md — The app came up with default settings](./TROUBLESHOOTING.md#the-app-came-up-with-default-settings)).

---

## Log Viewer

PresenceJam writes a single log file, `PresenceJam.log`, managed by the logging plugin:

```
%LOCALAPPDATA%\com.presencejam.app\logs\PresenceJam.log   (Windows)
~/Library/Logs/com.presencejam.app/PresenceJam.log        (macOS)
~/.local/share/com.presencejam.app/logs/PresenceJam.log   (Linux)
```

The log directory is the **bundle-identifier folder** (`com.presencejam.app`): Tauri's `app_log_dir()` appends the bundle id to the platform's local data directory, so `PresenceJam.log` does *not* sit next to `config.json` (issue #300). This is the directory tray menu → **Open Logs Folder** opens.

The **Log Viewer** in-app lets you browse these logs without opening the filesystem — and can be popped out into its own window (see *Detachable Windows* above). It opens already filled with the **last 500 lines of the on-disk file** — read off the UI thread and capped at the trailing 256 KiB — so the history is there before the first new line is logged. While you are scrolled away from the bottom the pane holds your place as entries arrive instead of sliding the text under you; **Jump to latest** pins it back to the bottom. **Clear** empties the pane and suppresses that initial backfill for as long as the window stays open. You can also open the folder directly via tray menu → **Open Logs Folder**.

**Log levels:**

| Level | Meaning |
|-------|---------|
| `ERROR` | Something failed — API error, file I/O error |
| `WARN` | Unexpected but recoverable (e.g., slow network) |
| `INFO` | Normal operations (track changed, status updated) |
| `DEBUG` | Verbose — every polling iteration logged |

The current log level is set in `config.json` under `logging.log_level`.

---

## Status Expiry

Teams custom status messages automatically expire. PresenceJam sets the message's expiry (`expiryDateTime`) to the **track's end time + a buffer** (default 10 s; `polling.expiry_buffer_seconds` in `config.json`). This is an app-side choice — the Graph API doesn't shorten it. When playback pauses or stops, PresenceJam replaces the message with a `🎵 Paused` / `🎵 Nothing playing on Spotify` placeholder that **expires 60 s after it is posted** (`placeholder_expiry_str()` in `src-tauri/src/polling/poll_once.rs` sets a fixed now + 60 s, on both the paused and the no-track path) — the pause placeholder does *not* inherit the track-end buffer. Graph has no "clear status message" action, so the short-lived placeholder is the documented clear mechanism: it self-removes ~1 min after the last successful post even if the app quits.

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
