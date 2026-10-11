# Headless operation

PresenceJam is a tray app, but it also runs **without a window** — one-shot,
as a long-running supervised daemon, or as a localhost control API. This page
covers all three, plus the packaging units in [`packaging/`](../packaging/).

Every mode here uses the same poller and the same `config.json` /
`tokens.json` the GUI uses. Nothing writes a new config format, and no
headless mode can reach a machine other than the one it runs on.

---

## Modes at a glance

| Mode | Command | Window / tray | Syncs continuously | Network surface |
| --- | --- | --- | --- | --- |
| One-shot status dump | `presencejam --status` | none | no, exits immediately | Spotify + Graph reads |
| One poll iteration | `presencejam --sync-once` | none | one iteration, then exits | Spotify + Graph |
| Supervised daemon | `presencejam --daemon` | none | yes | Spotify + Graph |
| Control API | `presencejam --serve[=PORT]` | none | yes | the above **plus** `127.0.0.1` HTTP |
| Normal autostart | `--minimized` (passed by the autostart plugin) | hidden, tray present | yes | Spotify + Graph |

`--status` and `--help` need no desktop at all. `--sync-once` **does** on
Linux: it drives the same Tauri runtime as the GUI and needs a display
server, so run it under `xvfb-run` when cron or CI is the caller.

`--daemon` deliberately registers **no** deep-link handler and takes **no**
single-instance lock, so it cannot steal the `presencejam://callback` scheme
from a running GUI. `--serve`, `--sync-once` and `--daemon` also skip the
per-launch scheme re-registration (`app.rs`) for the same reason.

---

## `--serve` — the token-guarded control API

Binds **`127.0.0.1` only** (default port **8649**, override with
`--serve=PORT`). The loopback interface is still untrusted — other processes
run as the same user, and RDP and shared containers widen that — so every
**mutating** route requires a bearer token.

```
GET    /status              none   same JSON shape as `--status` (SyncStatus)
GET    /events              none   text/event-stream of the three presence events
POST   /pause               bearer  empty — stops the polling thread
POST   /resume              bearer  empty — starts the polling thread
POST   /snooze?minutes=N    bearer  query `minutes` (1..=1440) — runtime-only snooze
POST   /profile?id=<id>     bearer  query `id` (1..=64 chars, [A-Za-z0-9_-])
```

The three events streamed on `/events` are `presence-updated`,
`spotify-track-changed` and `presence-gated`, as Server-Sent Events.

**No route writes `config.json` or `tokens.json`.** `/snooze` and `/profile`
mutate runtime state only, so an operator restart returns to the pre-call
on-disk behaviour — a snooze started over HTTP does **not** survive a restart.
Persistence is what the GUI and `update_config` are for.

### Retrieving the bearer token

The token is **32 random bytes, base64url-encoded (43 characters)**. It is
generated once and stored in the OS keychain under the service
`com.presencejam.app`, account `serve_token:com.presencejam.app`
(`src-tauri/src/keychain.rs:38`). It is **never** written to disk in
plaintext, **never** passed in argv, and **never** placed in the environment.

Rotation is an explicit operator action
(`keychain::rotate_serve_token`) — there is no automatic rotation, so a
deployment that needs a fresh credential must rotate deliberately.

```bash
# macOS — the token is in the login keychain
security find-generic-password -s com.presencejam.app \
  -a 'serve_token:com.presencejam.app' -w

# Linux (Secret Service), via secret-tool
secret-tool lookup service com.presencejam.app \
  account 'serve_token:com.presencejam.app'
```

Token comparison is constant-time, and `redact_sensitive` covers the
credential in any future log line.

### Using it

```bash
curl -s http://127.0.0.1:8649/status
curl -sN http://127.0.0.1:8649/events            # streams

curl -X POST http://127.0.0.1:8649/pause \
  -H "Authorization: Bearer $TOKEN"
curl -X POST 'http://127.0.0.1:8649/snooze?minutes=30' \
  -H "Authorization: Bearer $TOKEN"
```

---

## `--daemon` — supervised headless sync

Runs the same poller the GUI runs, windowless: no tray icon, no app menu, no
deep-link registration, no single-instance lock.

The supervisor (`src-tauri/src/polling/daemon.rs`) installs SIGTERM/SIGINT
handlers on Unix, starts the poller, and blocks on either a stop signal or a
self-exiting poller. **SIGTERM produces exit 0** — this is the contract
systemd's `ExecStop=` and launchd's `Stop` signal depend on.

A locked or missing keychain at boot retries with exponential backoff
(`token_io::read_or_create_serve_token_with_backoff`, 6 attempts, 1–30 s)
rather than exiting 1, so a desktop session that has not unlocked the
keychain yet does not kill the service on first start.

### The three packaging units

Three units ship in [`packaging/`](../packaging/). All three assume the
binary is installed at `/usr/local/bin/presencejam`; adjust `ExecStart` /
`ProgramArguments` to your layout.

#### systemd — `packaging/systemd/presencejam.service`

```ini
Type=simple
ExecStart=/usr/local/bin/presencejam --daemon
Restart=on-failure
RestartSec=5
TimeoutStopSec=30
KillSignal=SIGTERM
WantedBy=default.target
```

`Restart=on-failure` recovers from transient failures (keychain lockout at
boot, a fatal Spotify or Graph outage) without thrashing on a logic bug — a
panic that exits non-zero is exactly the kind of transient worth
supervising. The unit also ships hardening: `NoNewPrivileges`,
`PrivateTmp`, `ProtectSystem=strict`, `ProtectHome=true`.

Adjust `User=` / `Group=`. The default assumes a `presencejam` service user
created at install time.

#### launchd — `packaging/launchd/com.presencejam.daemon.plist`

```xml
<key>ProgramArguments</key>
<array>
    <string>/usr/local/bin/presencejam</string>
    <string>--daemon</string>
</array>
<key>RunAtLoad</key><true/>
<key>KeepAlive</key>
<dict>
    <key>SuccessfulExit</key><false/>
    <key>Crashed</key><true/>
</dict>
<key>ThrottleInterval</key><integer>10</integer>
```

`KeepAlive.SuccessfulExit=false` means launchd restarts only on a non-zero
exit — a clean SIGTERM stop does not loop, a panic or keychain boot failure
does. Install as a per-user LaunchAgent
(`~/Library/LaunchAgents/com.presencejam.daemon.plist`) so the keychain is
reachable; the system-wide `/Library/LaunchDaemons` path needs a
`UserName`/`GroupName` plus a keychain ACL grant.

#### Windows Task Scheduler — `packaging/windows/presencejam-task.xml`

`RunAtLogon` with a `PT30S` delay and `HighestAvailable`, so the daemon
starts after login settles. Import with `schtasks /Create /XML`.

### Logs

Every mode writes to the same `tauri-plugin-log` file the GUI uses:

- Windows — `%LOCALAPPDATA%\com.presencejam.app\logs\PresenceJam.log`
- macOS — `~/Library/Logs/com.presencejam.app/PresenceJam.log`
- Linux — `~/.local/share/com.presencejam.app/logs/PresenceJam.log`

The systemd unit also redirects stdout/stderr to
`/var/log/presencejam/daemon.log` (`StandardOutput=append:`), and the
launchd plist to `/usr/local/var/log/presencejam/`. Both are additive to the
plugin's own file.

---

## `--status` and the manual-status flags

| Flag | Effect |
| --- | --- |
| `--status` | Prints the sync status as JSON on stdout and exits 0. Same fields as the `get_sync_status` IPC command. Builds no window and no tray icon and takes no single-instance lock. |
| `--sync-once` | Runs exactly one poll iteration, including the Teams status write. Exits 0 on success, 1 with the reason on stderr. |
| `--set-status <message>` | Writes a one-off Teams status and exits. Capped at 128 characters (`MAX_RULE_STATUS_CHARS`). |
| `--set-status-expiry <minutes>` | Sets how long that manual status lives before the poller clears it. Clamped 5–720. |
| `--clear-status` | Clears any active manual status immediately. |
| `--profile <id>` | Switches the active presence profile, or back to the base configuration with `--profile base`. |
| `--minimized` | Not a headless flag — starts the GUI hidden to the tray. This is what the autostart plugin passes at login. |

`--status` and `--sync-once` read tokens in `TokenReadMode::ReadOnly`: legacy
plaintext is parsed but **never** migrated, chmod-ed, renamed or rewritten.
A CLI-only machine therefore cannot accidentally upgrade a plaintext
`tokens.json` into the encrypted envelope — see
[`SECURITY.md`](../SECURITY.md).

---

## See also

- [`USAGE.md`](../USAGE.md) § Command-line flags — the operator-facing flag table
- [`SECURITY.md`](../SECURITY.md) — token storage, keychain slots and the disclosure policy
- [`docs/RELEASING.md`](./RELEASING.md) — what ships in a release
- [`packaging/`](../packaging/) — the three service unit files
