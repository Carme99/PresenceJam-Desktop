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
2. Retry **Connect Spotify** (or **Reconnect** in Settings) — the app hands the authorization URL to your default browser. If that handoff fails you'll see a `Failed to open browser` warning from `[CMD.SPOTIFY_AUTH]` in the in-app **Log Viewer** (see [Logs](#logs))
3. If it still doesn't work, paste the full redirect URL manually when prompted — see [Paste URL but nothing happens](#paste-url-but-nothing-happens)

### Paste URL but nothing happens

**Cause:** The redirect URL might not have been captured automatically.

**Fix:**
1. After authorizing Spotify, your browser will redirect to `presencejam://callback?code=XXX...`
2. Copy the **full URL** from your browser's address bar
3. Paste it into the app's manual URL input field
4. Click Submit

### Token refresh failures

**Cause:** Spotify credentials expired or changed.

**Fix:** In Settings click **Reconnect** next to Spotify and complete the browser sign-in. You only need to reconnect when the refresh itself fails (invalid grant / revoked credentials); transient network/5xx errors retry automatically.

### Tray playback controls do nothing ("no active device")

**Cause:** Spotify has no active playback device — the player commands act on the active device, and there isn't one.

**Fix:** Open the tray menu → **Devices** and pick a device — PresenceJam transfers playback there and starts it. Alternatively, start playback on a device first (e.g. in the Spotify app), then retry.

### Tray playback controls fail with a 403

**Cause:** Playback control requires a **Spotify Premium** subscription (a Spotify platform restriction), or the stored token predates the `user-modify-playback-state` scope.

**Fix:** Check your Spotify plan. If you have Premium but the controls still fail, reconnect Spotify once (see the next entry) so the token carries the new scope.

### Tray Shuffle / Repeat does nothing, or asks for Premium

**Cause:** Both are real Spotify player commands, so they carry exactly the same requirements as Play/Pause: a **Spotify Premium** account, the `user-modify-playback-state` scope (a one-time reconnect for tokens that predate it), and an **active device**.

**Fix:**
1. If an in-app toast reads **"Playback control requires Spotify Premium"**, check your Spotify plan — playback control is a Premium-only Web API surface.
2. If it reads **"No active playback device - pick one from the tray Devices menu"**, open tray → **Devices** and pick one, or start playback on a device first.
3. If nothing happens and no toast appears, the stored token predates the playback scope — click **Reconnect** once (see *"Playback control needs a one-time reconnect" banner* above).

The **Repeat** item spells its mode out — `Repeat: Off` → `Repeat: Context` → `Repeat: Track` → back to off — because a check mark alone cannot distinguish *context* from *track*. A command Spotify rejects leaves the label and check mark where they were, so the menu never claims a state that was refused.

### "Playback control needs a one-time reconnect" banner

**Cause:** Tray playback control needs the `user-modify-playback-state` scope, which older stored tokens don't carry — a token granted before that scope was added can't control playback until you re-auth.

**Fix:** Click **Reconnect** in the banner (or Settings → reconnect Spotify) once. You only need to do this once after upgrading.

### "Spotify secret conflict" banner in Settings

**Cause:** A legacy plaintext `spotify.client_secret` left in `config.json` disagrees with the value stored in the OS keychain (#376). The app keeps the plaintext (nothing is deleted) and asks you to resolve it.

**Fix:** Click **Reconnect** in the banner (or Settings → reconnect Spotify) once — the fresh sign-in reconciles the stored secret and the banner dismisses on reconnect.

### Reconnect shows "Keychain unavailable"

**Cause:** The OS keychain could not be read — typically a locked Secret Service keyring on Linux, or a denied credential store. This is **not** the same as "no stored credential": your Spotify Client Secret is still in the keychain, the app just cannot reach it right now. Treating the two alike is what used to push a fully set-up user back through the Spotify wizard.

**Fix:**
1. On Linux, unlock your keyring and make sure a Secret Service daemon is running — see [SETUP.md — Linux: System Keyring Required](./SETUP.md#linux-keyring). On Windows/macOS an unavailable store usually means the credential manager is locked by policy.
2. Re-enter the Reconnect view: it re-probes the keychain on every entry.
3. There is deliberately **no Reconnect button** while this state is showing — the sign-in flow reads the secret from the same keychain that cannot answer, so it would only open a browser window that fails. You do **not** need to re-enter your Client ID/Secret.

Settings shows the same state on the credential row ("System keychain unavailable — it may be locked or missing…"). A genuine *absent* credential is the only case that offers **Run onboarding**.

## Microsoft Teams

### "Teams not updating" after connecting

**Cause:** Token expired, or the Microsoft account differs from Teams account.

1. Verify you're signed into the same Microsoft account in the app and in Teams
2. Go to Settings → **Reconnect** next to Teams → complete the device-code sign-in
3. Check the in-app log viewer for specific API error codes

### Device code sign-in times out

**Cause:** The sign-in window for entering the code expired.

**Fix:**
1. While the code is still valid, the app shows a live countdown of the remaining time (#429)
2. If the code expired, the app shows an expired state — click **Get new code** (or "Sign in with Microsoft" again) for a fresh code
3. Complete the sign-in before the countdown runs out
4. Make sure you're visiting the correct verification URL

### Sign-in loop / "authorization_declined"

**Cause:** You declined the authorization request, or the code was entered incorrectly.

**Fix:**
1. Click "Sign in with Microsoft" again for a fresh code and URL
2. Make sure to enter the code exactly as shown (uppercase, no spaces)

### "Presence features need a one-time Teams reconnect" banner

**Cause:** The **Settings → Presence** toggles need the `Presence.Read` (presence gating) and `profile` (object-id claim for availability sync) scopes, which older stored tokens don't carry — so the toggles won't take effect until you re-auth.

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

**Fix:** Quit again (or relaunch) and check `PresenceJam.log` for `[UPDATER]` lines. If the staged update keeps failing, use **Download & Install** from the update banner instead — that path reports errors in-app. Note: since v4.2.0, a staged update that is stale (same version or older than your current install) is *deliberately* skipped with an `[UPDATER]` log line, not installed — the banner shows a skipped state with an **Install anyway** override (#431). Downgrades are off by default (`allowDowngrades: false`).

### The app came up with default settings

**Cause:** `config.json` no longer parsed as JSON, so PresenceJam refused to load it. Instead of overwriting a file it cannot read, it **renames the broken file next to itself as `config.json.bak`**, logs a `[CFG] corrupt config '…' quarantined to '…'` warning, and starts on the shipped defaults.

**Fix:**
1. Look for that `[CFG]` line in the log viewer — it names both the original and the backup.
2. Your old settings are all still in the `.bak`. Repair the JSON by hand and put it back as `config.json`, or simply re-apply the settings in the app.
3. Where to look (the backup sits beside the config file):

   | OS | Path |
   |----|------|
   | Windows | `%APPDATA%\PresenceJam\config.json.bak` |
   | macOS | `~/Library/Application Support/PresenceJam/config.json.bak` |
   | Linux | `$XDG_CONFIG_HOME/PresenceJam/config.json.bak` (usually `~/.config/PresenceJam/`) |

The backup name is fixed — `config.json.bak`, never timestamped — and the rename is best-effort: if it fails, the app logs that too and still boots on defaults, leaving your original file untouched. `tokens.json` is unaffected and lives in a different folder entirely (see [SETUP.md — What Gets Installed](./SETUP.md#what-gets-installed)).


### No status appears on Teams

1. Check the Dashboard shows both Spotify and Teams as connected (green badges)
2. Start playing a track on Spotify
3. Wait for the next poll: with a track playing, the status updates within a few seconds; when nothing is playing, the app backs off between polls (30 s → 60 s → 120 s → 300 s), so an idle app can take up to five minutes to react. Failed polls retry after ~30 s (±20%)
4. If still nothing, check the log viewer for API errors

**Suppressed by status rules?** PresenceJam also suppresses the write on purpose: a quiet-hours entry covering the current time and weekday, or an enabled track rule whose *Post this instead* field is left empty. The Dashboard shows its *suppressed* chip for these as well, and the log records which one fired — `[POLLING] process_track: quiet hours active, skipping status write` or `[POLLING] process_track: track rule matched, skipping status write`. Open **Settings → Status rules** and look for an enabled quiet-hours range covering now, and for a rule matching what's playing (see [USAGE.md — Status rules](./USAGE.md#status-rules)).

### Status doesn't clear when Spotify is paused

**Cause:** The `clear_on_pause` config option may be disabled, or you're using Spotify Web Player instead of the desktop app.

**Fix:**
1. Open `config.json` and check `teams.clear_on_pause` is `true` — there is no Settings toggle for it
2. Ensure you're using the Spotify desktop app (not web player) — the API detects both, but desktop is more reliable

### Status shows but disappears quickly

**Cause:** PresenceJam sets a playing track's status expiry (`expiryDateTime`) to the track's end time plus a buffer (default 10 s; `polling.expiry_buffer_seconds` in `config.json`). When the track ends, pauses, or stops, the app replaces the status on the next poll with a `🎵 Paused` / `🎵 Nothing playing on Spotify` placeholder that has its **own fixed 60 s expiry** (`placeholder_expiry_str()`), so it self-removes about a minute after posting. There is no server-side 24-hour cap — the Teams client's "Clear status message after" dropdown (which includes 24 h) affects only messages you set in the Teams UI, not Graph-set messages.

**Fix:**
This is the app's own expiry/clear mechanism, not a Teams limitation. To keep the status visible longer, raise the buffer in `config.json` (`polling.expiry_buffer_seconds`) or set `teams.clear_on_pause` to `false` in `config.json` (there is no Settings toggle for it).

## Profanity Filter

### My custom placeholder isn't showing

1. In **Settings → Teams**, ensure **Profanity Filter** is toggled ON
2. Check that **Placeholder** field is non-empty (whitespace-only falls back to default)
3. If the placeholder contains `{emoji}`, it will be replaced with 🎵 (playing) or ⏸️ (paused)

### A profane track isn't being filtered

**Cause:** The filter currently operates on the formatted status string after the template is applied. If your template uses custom placeholders, the raw track metadata (artist/track/album) may not be fully covered.

**Fix / Workaround:**
This is a known architectural limitation: the filter screens the formatted status string (see [ARCHITECTURE.md](./ARCHITECTURE.md#profanity-filter)), verified against `poll_once.rs` (filter applied at the single status-write path) and `profanity.rs`. A future release will filter raw Spotify fields before formatting.

### What's in the profanity word list?

The list is in `src-tauri/src/profanity.rs` and covers common English profanity. Detection includes:
- Leetspeak variants: `sh1t`, `$hit`, `d@mn`, `p1ss`, `n1gg3r`
- Repeated-character variants: `shiiit`, `fuuuuck`
- "fucking", "fucked", "fucker" variants

False positives are prevented via word-boundary checks — words like `class`, `cocktail`, `assassin`, `vacuum`, `cumulative` are not flagged.

## Logs

### Where to find logs

**In-app:** the **Log Viewer** view (it can be popped out into its own window) lets you scroll through the entries without touching the filesystem. When it opens it is already backfilled with the last 500 lines of the on-disk file, so the history is there before the first new entry is logged; while you are scrolled up it holds your position as new lines arrive.

**Direct filesystem** — a single `PresenceJam.log` file managed by the logging plugin:
```
%LOCALAPPDATA%\com.presencejam.app\logs\PresenceJam.log   (Windows)
~/Library/Logs/com.presencejam.app/PresenceJam.log        (macOS)
~/.local/share/com.presencejam.app/logs/PresenceJam.log   (Linux)
```

The log directory is the **bundle-identifier folder** (`com.presencejam.app`) — `app_log_dir()` appends the bundle id to the platform's local data directory, so the logs do *not* sit next to `config.json` (issue #300). This is the same folder tray menu → **Open Logs Folder** opens.

**From PowerShell:**
```powershell
Start-Process "$env:LOCALAPPDATA\com.presencejam.app\logs"
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

1. Open the log folder: tray menu → **Open Logs Folder** (or `%LOCALAPPDATA%\com.presencejam.app\logs` on Windows)
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
3. **Delete user data** (optional — removes all tokens and config).
   `config.json` and `tokens.json` are NOT in the same folder (issue #300),
   so delete both directories:
   ```
   %APPDATA%\PresenceJam\
   %APPDATA%\com.presencejam.app\PresenceJam\
   ```
   You can also use PowerShell:
   ```powershell
   Remove-Item -Recurse -Force "$env:APPDATA\PresenceJam"
   Remove-Item -Recurse -Force "$env:APPDATA\com.presencejam.app\PresenceJam"
   ```

Note: Your Spotify app credentials in the Spotify Developer Dashboard are unaffected.
