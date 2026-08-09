# Usage

Day-to-day guide to running PresenceJam.

## The System Tray

PresenceJam lives in your **system tray** (Windows taskbar or macOS menu bar). The app window is hidden by default to keep your taskbar clean.

**Tray icon behavior:**
- **Left-click** — open/focus the PresenceJam window
- **Right-click** — open the tray menu

**Tray menu options:**

| Option | What it does |
|--------|-------------|
| Show Window | Bring the app window to the foreground |
| Pause Syncing | Stop polling Spotify (Teams status stays as-is) |
| Resume Syncing | Start polling again after a pause |
| Play / Pause | Toggle playback on your active Spotify device |
| Previous | Skip to the previous track |
| Next | Skip to the next track |
| Devices | List your Spotify devices — picking one transfers playback there and starts it |
| Up Next | Peek at the next tracks in your queue |
| Quit | Fully exit the app |

> **Tray playback** (Play/Pause, Previous, Next, Devices, Up Next) requires a **Spotify Premium** account and the one-time reconnect that adds the `user-modify-playback-state` scope (see [SETUP.md — Upgrading from 2.x](./SETUP.md#upgrading-from-2x)). Until then, Settings shows a "Playback control needs a one-time reconnect" banner.

> **Closing the window (X button) doesn't quit the app** — it minimizes to the tray. This is intentional so sync keeps running in the background. Use **Quit** from the tray menu to fully exit.

---

## Dashboard

The main screen showing your current sync status.

**Connection status badges:**
- Green — connected and authenticated
- Red — disconnected or token expired — click to reconnect

**Sync toggle:**
- **Start Syncing** — begins polling Spotify and updating your Teams status
- **Stop Syncing** — pauses polling, your Teams status remains unchanged

**Currently playing card:**
- Shows the active track (artist, track name, album art if available)
- Updates in real-time as tracks change
- Shows ⏸️ when nothing is playing

---

## Settings

### Status Format

Edit the template that formats your Teams status message. Supports `{artist}`, `{track}`, `{album}`, and `{emoji}`.

**Example:** `🎵 {artist} - {track} 🎧` → `🎵 Daft Punk - One More Time 🎧`

### Teams Status

| Setting | Default | Description |
|---------|---------|-------------|
| Clear on pause | On | Clears your Teams status when Spotify pauses or stops |
| Profanity filter | On | Replaces profane track/artist names with a safe placeholder |
| Profanity placeholder | `Currently Listening to Spotify` | Shown when a track name is filtered. Supports `{emoji}`. |

### Presence

| Setting | Default | Description |
|---------|---------|-------------|
| Show Available while listening | Off | Sets your Teams presence to **Available** while a track plays (re-armed every few minutes, cleared on pause). It shows *Available*, not *Busy* — Microsoft's `setPresence` API only supports the Busy/**InACall** combination, so "busy" would display an in-call bubble to your colleagues. |
| Pause status during meetings/calls/DND | On | Reads your Teams presence before writing a status update and skips the write while you're busy, in a meeting, in a call, or presenting. The status resumes on the next track change once your presence clears. |

> Both toggles need the one-time Teams reconnect (see [SETUP.md — Upgrading from 2.x](./SETUP.md#upgrading-from-2x)) — the new `Presence.Read` and `profile` scopes are only granted on a fresh sign-in.

### Polling

| Setting | Default | Description |
|---------|---------|-------------|
| Interval | 30s | How often to check Spotify when a track is playing (minimum 10s) |
| Smart sleep | On | Sleep until the track ends instead of polling continuously — no wasted API calls |

### General

| Setting | Description |
| Launch at login | Start PresenceJam automatically when your OS boots |
| Start minimized | Open the app minimized to the tray (window hidden on launch). On macOS, `start_minimized` also switches the app's activation policy to `Accessory`, removing the dock icon and menu-bar app menu — the app becomes a pure tray-resident app. The dock icon reappears when you disable this setting in Settings (no restart needed). (v2.7.3+) |

---

## Updates

On startup, PresenceJam checks GitHub Releases for a newer version. If one is available, a small banner appears at the top of the window: **"Update vX.Y.Z available"** with a **Download & Install** button and a progress readout. Once the download finishes, the app relaunches itself into the new version. The banner is dismissible, and a failed check (offline, unreachable endpoint, mismatched signature key) is silent — it never blocks the UI.

Update payloads are signature-verified against a key baked into the app (minisign), which is independent of OS code signing — so the macOS unsigned/Gatekeeper note in the README applies to updated builds too.

---

## Log Viewer

PresenceJam keeps daily rotating logs at:

```
%APPDATA%\PresenceJam\logs\        (Windows)
~/Library/Application Support/PresenceJam/logs/  (macOS)
```

The **Log Viewer** in-app (accessible from Settings) lets you browse these logs without opening the filesystem.

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

Teams custom status messages automatically expire. PresenceJam sets the message's expiry (`expiryDateTime`) to the **track's end time + a buffer** (default 10 s; `polling.expiry_buffer_seconds` in `config.json`). This is an app-side choice — the Graph API doesn't shorten it. When playback pauses or stops, PresenceJam replaces the message with a non-expiring placeholder.

---

## Token Refresh

Tokens refresh automatically:
- **Spotify tokens** — refreshed by PresenceJam when needed (no action required)
- **Teams tokens** — same. The device-code sign-in requests `offline_access`, so Microsoft issues a refresh token that PresenceJam rotates before the access token expires.

If your connection drops unexpectedly, PresenceJam will emit a reconnect prompt. Go to **Settings → disconnect and reconnect** the affected service.

---

## Switching Accounts

To switch your Spotify or Teams account:

1. Open PresenceJam → **Settings**
2. Under the service you want to switch, click **Disconnect**
3. Run the onboarding step again for that service

This clears the old tokens and prompts fresh OAuth for the new account.
