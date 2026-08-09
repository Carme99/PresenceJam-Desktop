# Setup

Get PresenceJam running on your machine.

## Prerequisites

- **Windows 10/11** (64-bit) or **macOS** (Apple Silicon)
- A **Spotify Premium** account (required for the Web API)
- A **Microsoft 365 Teams** account (work or school — the Graph
  presence APIs don't support personal Microsoft accounts)

## Step 1 — Install the App

Download the latest release from the [GitHub Releases page](https://github.com/Carme99/PresenceJam-Desktop/releases/latest):

| OS | File | Notes |
|----|------|-------|
| Windows | see [Releases page](https://github.com/Carme99/PresenceJam-Desktop/releases/latest) | Run the installer, follow the prompts |
| macOS | see [Releases page](https://github.com/Carme99/PresenceJam-Desktop/releases/latest) | Drag PresenceJam to Applications |

Filenames follow the pattern `PresenceJam-<version>-<os>.<ext>` (e.g. `PresenceJam-<version>-linux-amd64.deb`); see the [latest release](https://github.com/Carme99/PresenceJam-Desktop/releases/latest) for the current values.

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
   - Tick the **Developer Terms of Service** checkbox
4. Click **CREATE**

### Configure redirect URIs

The Create dialog has no Redirect URIs field — you add those after creation:

1. On the app overview page, click **Edit Settings**
2. Under **Redirect URIs**, add: `presencejam://callback`
3. Click **SAVE**

### Copy your credentials

From the app overview page, copy:
- **Client ID** — you'll paste this into PresenceJam during onboarding
- **Client Secret** — also pasted into PresenceJam during onboarding

> **Keep your Client Secret private.** It authenticates your app to Spotify. If you accidentally expose it publicly, click **ROTATE** on the app overview page to regenerate it immediately.

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

## Upgrading from 2.x

Upgrading to 3.0 adds new OAuth scopes on both providers, so **both** require a **one-time re-auth** after the upgrade:

- **Spotify** — the new `user-modify-playback-state` scope powers the tray playback controls. Until you reconnect, Settings shows a **"Playback control needs a one-time reconnect"** banner.
- **Teams** — the new `Presence.Read` (meeting/call-aware gating) and `profile` (object-id claim needed by the availability sync) scopes. Until you reconnect, Settings shows a **"Presence features need a one-time Teams reconnect"** banner.

Click **Reconnect** in the banner (or Settings → reconnect the service) — you only need to do this once per provider.

Your `tokens.json` migrates automatically: on first read, v3.0 detects a ≤2.x plaintext file, encrypts it with AES-256-GCM, and rewrites it. No manual step. The existing folder-copy backup advice still applies unchanged — `config.json` and `tokens.json` live together in `%APPDATA%\PresenceJam\` / `~/Library/Application Support/PresenceJam/`.

---

## What Gets Installed

```
%APPDATA%\PresenceJam\          (Windows)
~/Library/Application Support/PresenceJam/  (macOS)
├── config.json       # Your settings
├── tokens.json       # Spotify + Teams tokens (AES-256-GCM ciphertext; decryption key in the OS keychain — see SECURITY.md)
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
