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

## App Behavior

### App "closes" on X button but keeps running

**This is by design.** The app minimizes to the system tray to keep syncing in the background.

**To fully quit:**
- Right-click the tray icon → **Quit**
- Or right-click → **Show Window** → close from within the app

**To prevent it from starting with Windows:**
- Settings → disable **Launch at Login**

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

**Cause:** Teams has a limit on how long custom status messages can persist (maximum 24 hours).

**Fix:**
This is a Teams limitation. PresenceJam sets the status to expire at track end time + 10 second buffer, but Teams may shorten this. This is normal Teams behavior.

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

**In-app:**
Settings → **Log Viewer** → scroll through the log entries

**Direct filesystem:**
```
%APPDATA%\PresenceJam\logs\
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

1. Open the log folder: `%APPDATA%\PresenceJam\logs`
2. Find the most recent file (e.g., `spotify-teams-2026-04-09.log`)
3. Attach it to your GitHub issue
4. Note the approximate time the issue occurred

## Performance

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
