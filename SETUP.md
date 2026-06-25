# Setup

Get PresenceJam running on your machine.

## Prerequisites

- **Windows 10/11** (64-bit) or **macOS** (Apple Silicon)
- A **Spotify Premium** account (required for the Web API)
- A **Microsoft 365 Teams** account (personal or work — both work)

## Step 1 — Install the App

Download the latest release from [GitHub Releases](https://github.com/Carme99/PresenceJam-Desktop/releases):

| OS | File | Notes |
|----|------|-------|
| Windows | `PresenceJam-2.6.0.msi` | Run the installer, follow the prompts |
| macOS | `PresenceJam-2.6.0-macos.dmg` | Drag PresenceJam to Applications |

The installer will create:
- The app in your Applications/start menu
- A `%APPDATA%\PresenceJam\` folder (Windows) or `~/Library/Application Support/PresenceJam/` (macOS) for config and tokens
- A **system tray** icon — the app runs in the background

> **Tip:** When the app first opens, it will show the **Onboarding Wizard** — a 3-step guide that walks you through connecting Spotify and Teams.

---

## Step 2 — Register a Spotify App

PresenceJam uses Spotify's Web API to read your currently playing track. This requires a Spotify Developer app with the correct settings.

### Create the app

1. Go to the [Spotify Developer Dashboard](https://developer.spotify.com/dashboard) and log in
2. Click **Create App**
3. Fill in the details:
   - **App name:** PresenceJam (or anything you like)
   - **App description:** Syncs my Spotify playback to Teams status
4. Under **Redirect URIs**, add: `presencejam://callback`
5. Agree to Spotify's terms and click **Save**

### Copy your credentials

From the app's page, copy:
- **Client ID** — you'll paste this into PresenceJam during onboarding
- **Client Secret** — also pasted into PresenceJam during onboarding

> **Keep your Client Secret private.** It authenticates your app to Spotify. If you accidentally expose it publicly, go to your app's settings and **Reset Client Secret** immediately.

---

## Step 3 — Connect to Teams

Teams uses Microsoft's Device Code flow — no app registration needed on your end, just sign in with your Microsoft account.

During onboarding, PresenceJam will:
1. Show you a short **code** and a verification URL
2. Open your browser to that URL
3. You enter the code and sign in with your Microsoft account
4. PresenceJam picks up the auth token automatically

Make sure you sign in with the **same Microsoft account** you use in Teams.

---

## Step 4 — Configure Your Status

After connecting both services, you'll land on the **Dashboard**. Before starting sync, you can adjust how your Teams status looks in **Settings**.

### Status Format

Customize the text using placeholders:

| Placeholder | Output |
|-------------|--------|
| `{artist}` | Artist name |
| `{track}` | Track name |
| `{album}` | Album name |
| `{emoji}` | 🎵 (playing) or ⏸️ (paused) |

**Default:** `🎵 {artist} - {track} 🎧`

**Example:** `🎵 Daft Punk - One More Time 🎧`

### Profanity Filter

If a track or artist name contains profanity, PresenceJam replaces the entire status with a safe placeholder. You can:
- Toggle the filter on/off
- Customize the placeholder text (supports `{emoji}`)

### Other Settings

| Setting | Description |
|---------|-------------|
| Polling interval | How often to check Spotify (minimum 10s) |
| Clear on pause | Automatically clear Teams status when Spotify pauses |
| Launch at login | Start PresenceJam when your OS boots |

---

## What Gets Installed

```
%APPDATA%\PresenceJam\          (Windows)
~/Library/Application Support/PresenceJam/  (macOS)
├── config.json       # Your settings
├── tokens.json       # Spotify + Teams tokens (encrypted via DPAPI/Keychain)
└── logs\            # Daily rotating application logs
```

No data is sent to any third-party server — all tokens stay on your machine.

---

## Uninstalling

1. **Quit the app** — right-click the tray icon → Quit
2. **Delete the app:**
   - Windows: Settings → Apps → PresenceJam → Uninstall
   - macOS: Drag PresenceJam from Applications to Trash
3. **Delete user data** (optional):
   ```powershell
   # Windows
   Remove-Item -Recurse -Force "$env:APPDATA\PresenceJam"
   ```
   ```bash
   # macOS
   rm -rf ~/Library/Application\ Support/PresenceJam
   ```

Your Spotify app credentials (Client ID/Secret) in the Spotify Developer Dashboard are unaffected — revoke them separately if you want to fully disconnect.

## Linux: System Keyring Required

<a name="linux-keyring"></a>

PresenceJam stores your Spotify `client_secret` in the OS keychain (the same place Firefox/Chromium store website passwords). On Windows and macOS this works automatically; on Linux it requires a system keyring daemon to be installed and running.

If PresenceJam fails to start with an error mentioning "OS keychain is unavailable" or "Failed to open keychain entry", your Linux setup is missing a keyring. Install one of the following:

| Distro / DE | Recommended package |
|---|---|
| GNOME (Ubuntu, Fedora Workstation, etc.) | `gnome-keyring` (usually pre-installed on GNOME desktops) |
| KDE Plasma | `kwallet5` or `kwallet6` |
| systemd-based, no GUI | `systemd-creds` |
| Other / headless | `gnome-keyring` + `libsecret-tools` |

**Linux install command:**

```bash
# Debian / Ubuntu
sudo apt install gnome-keyring libsecret-1-0

# Fedora
sudo dnf install gnome-keyring libsecret

# Arch
sudo pacman -S gnome-keyring libsecret
```

After installing, **log in to a graphical session** (a headless SSH session can't reach the keyring). If you launched PresenceJam from a TTY, launch it from your desktop session instead. Then restart PresenceJam.

Verify the keyring is reachable from your shell:

```bash
secret-tool store --label=test service test user test
secret-tool lookup service test user test  # should echo "test"
secret-tool clear service test user test
```

If `secret-tool` can read/write, PresenceJam's keychain path will work. If not, your distro's Secret Service D-Bus activation is broken — check `systemctl --user status gnome-keyring-daemon` (or the equivalent for `kwalletd`).

There is **no plaintext-on-disk fallback** for the client secret. A working keyring is a hard requirement.
