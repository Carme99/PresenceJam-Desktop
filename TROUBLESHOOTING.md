# Troubleshooting

Common problems and how to fix them.

## Quick Checks

Before diving in, check these basics:

- The app is minimized to **system tray**, not closed — right-click tray icon to quit
- Your Windows user profile has **write access** to `%APPDATA%\PresenceJam\`
- You're connected to the **same network** (no corporate proxy blocking Spotify/Teams APIs)
- Both **Spotify Premium** and **Microsoft 365 Teams** accounts are active

## Spotify

### "Spotify not connected" after entering credentials

**Cause:** The Redirect URI in your Spotify app isn't configured correctly.

**Fix:**
1. Go to [Spotify Developer Dashboard](https://developer.spotify.com/dashboard)
2. Select your app → **Edit Settings**
3. Under **Redirect URIs**, add: `presencejam://callback`
4. Save settings
5. Restart the app and try again

### Authorization browser doesn't open

**Cause:** Popup blocker or browser preference.

**Fix:**
1. Check if a popup was blocked in your browser
2. Try opening the URL manually — the auth URL will be logged in the terminal
3. If it still doesn't work, paste the redirect URL manually when prompted

### Paste URL but nothing happens

**Cause:** The redirect URL might not have been captured automatically.

**Fix:**
1. After authorizing Spotify, your browser will redirect to `presencejam://callback?code=XXX...`
2. Copy the **full URL** from your browser's address bar
3. Paste it into the app's manual URL input field
4. Click Submit

### Token refresh failures

**Cause:** Spotify credentials expired or changed.

**Fix:**
1. Go to Settings in the app
2. Disconnect Spotify
3. Re-run the onboarding Spotify step

### Tray playback controls do nothing ("no active device")

**Cause:** Spotify has no active playback device — the player commands act on the active device, and there isn't one.

**Fix:** Open the tray menu → **Devices** and pick a device — PresenceJam transfers playback there and starts it. Alternatively, start playback on a device first (e.g. in the Spotify app), then retry.

### Tray playback controls fail with a 403

**Cause:** Playback control requires a **Spotify Premium** subscription (a Spotify platform restriction), or the stored token predates the `user-modify-playback-state` scope.

**Fix:** Check your Spotify plan. If you have Premium but the controls still fail, reconnect Spotify once (see the next entry) so the token carries the new scope.

### "Playback control needs a one-time reconnect" banner

**Cause:** v3.0 added the `user-modify-playback-state` scope; tokens granted before the upgrade don't have it, so the tray playback controls won't work until you re-auth.

**Fix:** Click **Reconnect** in the banner (or Settings → reconnect Spotify) once. You only need to do this once after upgrading.

## Microsoft Teams

### "Teams not updating" after connecting

**Cause:** Token expired, or the Microsoft account differs from Teams account.

**Fix:**
1. Verify you're signed into the same Microsoft account in the app and in Teams
2. Go to Settings → Disconnect Teams → reconnect
3. Check the in-app log viewer for specific API error codes

### Device code sign-in times out

**Cause:** The 15-minute window for entering the code expired.

**Fix:**
1. Click "Sign in with Microsoft" again to get a fresh code
2. Complete the sign-in within 15 minutes
3. Make sure you're visiting the correct verification URL

### Sign-in loop / "authorization_declined"

**Cause:** You declined the authorization request, or the code was entered incorrectly.

**Fix:**
1. Click "Sign in with Microsoft" again for a fresh code and URL
2. Make sure to enter the code exactly as shown (uppercase, no spaces)

### "Presence features need a one-time Teams reconnect" banner

**Cause:** v3.0 added the `Presence.Read` (presence gating) and `profile` (object-id claim for availability sync) scopes; tokens granted before the upgrade don't carry them, so the **Settings → Presence** toggles won't take effect until you re-auth.

**Fix:** Click **Reconnect** in the banner (or Settings → reconnect Teams) once. You only need to do this once after upgrading.

## App Behavior

### App "closes" on X button but keeps running

**This is by design.** The app minimizes to the system tray to keep syncing in the background.

**To fully quit:**
- Right-click the tray icon → **Quit**
- Or right-click → **Show Window** → close from within the app

**To prevent it from starting with Windows:**
- Settings → disable **Launch at Login**

### A detached Logs/Settings window disappeared

**Cause:** The pane was popped out into its own window (`logs-detached` / `settings-detached`) and the window was closed or lost behind others.

**Fix:** The main window's nav still shows the pane with a dot badge while it's detached — clicking it focuses the detached window. If the detached window was closed entirely, click **Logs**/**Settings** in the main nav to re-open the view in-window, then pop it out again if you want.

### The interface is in the wrong language

**Cause:** The language picker (Settings → General → Language) defaults to your browser/OS language and persists the choice.

**Fix:** Pick **English**, **Deutsch**, or **Français** in Settings → General. The choice applies immediately and persists across restarts. Rust-side error strings surfaced by the backend remain English by design — only UI strings are localized.

### "Install on quit" seemed to do nothing

**Cause:** With *Install on quit*, the update is applied while the app is exiting — there is no window left to show progress or an error in. If staging or applying fails at that point, the failure is visible **only in the log file** (`PresenceJam.log`), not in the UI (#244).

**Fix:** Quit again (or relaunch) and check `PresenceJam.log` for `[UPDATER]` lines. If the staged update keeps failing, use **Download & Install** from the update banner instead — that path reports errors in-app.


### No status appears on Teams

1. Check the Dashboard shows both Spotify and Teams as connected (green badges)
2. Start playing a track on Spotify
3. Wait 10-30 seconds for the polling interval
4. If still nothing, check the log viewer for API errors

### Status doesn't clear when Spotify is paused

**Cause:** The `clear_on_pause` config option may be disabled, or you're using Spotify Web Player instead of the desktop app.

**Fix:**
1. Check Settings → polling config
2. Ensure you're using the Spotify desktop app (not web player) — the API detects both, but desktop is more reliable

### Status shows but disappears quickly

**Cause:** PresenceJam sets the status message's expiry (`expiryDateTime`) to the track's end time plus a buffer (default 10 s; `polling.expiry_buffer_seconds` in `config.json`). When the track ends, pauses, or stops, the app expires or clears/replaces the status on the next poll. There is no server-side 24-hour cap — the Teams client's "Clear status message after" dropdown (which includes 24 h) affects only messages you set in the Teams UI, not Graph-set messages.

**Fix:**
This is the app's own expiry/clear mechanism, not a Teams limitation. To keep the status visible longer, raise the buffer in `config.json` (`polling.expiry_buffer_seconds`) or disable Clear on pause.

## Profanity Filter

### My custom placeholder isn't showing

1. In **Settings → Teams**, ensure **Profanity Filter** is toggled ON
2. Check that **Placeholder** field is non-empty (whitespace-only falls back to default)
3. If the placeholder contains `{emoji}`, it will be replaced with 🎵 (playing) or ⏸️ (paused)

### A profane track isn't being filtered

**Cause:** The filter currently operates on the formatted status string after the template is applied. If your template uses custom placeholders, the raw track metadata (artist/track/album) may not be fully covered.

**Fix / Workaround:**
This is a known architectural limitation. See the TODO note in [ARCHITECTURE.md](./ARCHITECTURE.md#profanity-filter). A future release will filter raw Spotify fields before formatting.

### What's in the profanity word list?

The list is in `src-tauri/src/profanity.rs` and covers common English profanity. Detection includes:
- Leetspeak variants: `sh1t`, `$hit`, `d@mn`, `p1ss`, `n1gg3r`
- Repeated-character variants: `shiiit`, `fuuuuck`
- "fucking", "fucked", "fucker" variants

False positives are prevented via word-boundary checks — words like `class`, `cocktail`, `assassin`, `vacuum`, `cumulative` are not flagged.

## Logs

### Where to find logs

**In-app:** the **Log Viewer** view (it can be popped out into its own window) lets you scroll through the entries without touching the filesystem.

**Direct filesystem** — a single `PresenceJam.log` file managed by the logging plugin:
```
%APPDATA%\PresenceJam\logs\PresenceJam.log        (Windows)
~/Library/Logs/PresenceJam/PresenceJam.log        (macOS)
~/.local/share/PresenceJam/logs/PresenceJam.log   (Linux)
```

**From PowerShell:**
```powershell
Start-Process "$env:APPDATA\PresenceJam\logs"
```

### How to read log levels

| Level | What it means |
|-------|---------------|
| `ERROR` | Something failed — API returned error, file I/O failed |
| `WARN` | Something unexpected but recoverable (e.g., slow network) |
| `INFO` | Normal operations (track changed, status updated) |
| `DEBUG` | Verbose — every polling iteration logged |

The current log level is set in your `config.json` under `logging.log_level` (default: `Info`).

### Attaching logs to bug reports

1. Open the log folder: tray menu → **Open Logs Folder** (or `%APPDATA%\PresenceJam\logs` on Windows)
2. Attach `PresenceJam.log`
3. Note the approximate time the issue occurred

### High CPU or memory usage

PresenceJam is designed to be lightweight. If you're seeing high resource usage:

1. Check only one instance is running (see system tray)
2. Verify the polling isn't stuck in a fast loop (logs will show `polling_loop: sleeping for X seconds`)
3. Close other resource-heavy applications and test again

### App starts slowly

Tauri apps have a cold-start time of 1-3 seconds on first launch. This is normal — subsequent launches from the tray are faster.

## Networking

### Blocked by corporate proxy

PresenceJam makes HTTPS requests directly to:
- `accounts.spotify.com`
- `api.spotify.com`
- `login.microsoftonline.com`
- `graph.microsoft.com`

If you're on a corporate network that blocks these domains, the app won't work. Check with your IT administrator.

### "Connection refused" errors

**Cause:** Network connectivity issue or VPN interference.

**Fix:**
1. Try disabling VPN temporarily
2. Verify you can reach Spotify.com in your browser
3. Check Windows Firewall hasn't blocked the app

## Uninstalling

To fully remove PresenceJam:

1. **Quit the app** (right-click tray → Quit)
2. **Delete the app:**
   - Windows Settings → Apps → PresenceJam → Uninstall
3. **Delete user data** (optional — removes all tokens and config):
   ```
   %APPDATA%\PresenceJam\
   ```
   You can also use PowerShell:
   ```powershell
   Remove-Item -Recurse -Force "$env:APPDATA\PresenceJam"
   ```

Note: Your Spotify app credentials in the Spotify Developer Dashboard are unaffected.
