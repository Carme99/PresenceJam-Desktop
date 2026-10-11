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

> ### ⚠ Two defects in the shipped unit
>
> **1. `ProtectHome=true` will very likely stop this daemon working.**
> The unit sets `ProtectSystem=strict`, `ProtectHome=true`, and
> `ReadWritePaths=/var/log/presencejam`. But the daemon has to read
> `~/.config/com.presencejam.app/` and reach the user's keyring socket to get
> at the stored Spotify and Teams tokens. `ProtectHome=true` makes `/home`,
> `/root` and `/run/user` inaccessible to the service, which is exactly where
> both of those live. The hardening is sound in the abstract and wrong for
> this process — an allow-list of the one config directory the daemon owns is
> what it actually needs. Verify on a real target before shipping this unit to
> anyone; the failure mode is a boot loop, because `Restart=on-failure` keeps
> retrying a process that cannot possibly succeed.
>
> **2. `Documentation=` points at a repository that does not exist.**
> ```
> Documentation=https://github.com/presencejam/presencejam-desktop
> ```
> The repository is `github.com/Carme99/PresenceJam-Desktop`. The lowercase
> `presencejam` org is not this project.

### launchd — `packaging/launchd/com.presencejam.daemon.plist`

```bash
cp packaging/launchd/com.presencejam.daemon.plist \
   ~/Library/LaunchAgents/com.presencejam.daemon.plist
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.presencejam.daemon.plist
```

Per-user agents are the practical choice on macOS: the daemon must run in the
user's GUI session to reach the Keychain. A system-wide `LaunchDaemon` needs a
different `UserName`/`GroupName` and an explicit keychain ACL grant — see
[`../SETUP.md`](../SETUP.md).

`KeepAlive` is a dict with `SuccessfulExit=false` and `Crashed=true`, which is
the correct policy for this contract: a clean `SIGTERM` → exit 0 does **not**
trigger a restart, while a panic or keychain boot failure does. The
surrounding comment describes the key as "`SuccessfulExit` (true)", which is
the inverse of the value beneath it — the value is right, the comment reads
backwards.

Update `StandardOutPath` / `StandardErrorPath`; the defaults point into
`/usr/local/var/log/`, which most macOS installs will not have.

### Windows — `packaging/windows/presencejam-task.xml`

```powershell
schtasks /Create /TN "PresenceJam Daemon" `
         /XML "%ProgramFiles%\PresenceJam\presencejam-task.xml"
```

Runs at logon with a 30-second delay (`PT30S`) so the OS keychain is reachable
before the daemon tries to read it. `MultipleInstancesPolicy=IgnoreNew`,
`RunOnlyIfNetworkAvailable=true`, `ExecutionTimeLimit=PT0S` (unlimited).

> ### ⚠ Two defects in the shipped task
>
> **1. The registered name will not match.** The `<URI>` element is
> `\PresenceJam\Daemon`, but the documented `schtasks` name is
> `"PresenceJam Daemon"` — two different paths to the same task. Whichever
> form `schtasks /End /TN …` is given, it has to be the one that was created.
>
> **2. It runs as `NT AUTHORITY\SYSTEM`, against the wrong user's data.** The
> principal is `S-1-5-18`. The config and the OS keychain are per-user, so
> running as SYSTEM reads a different profile than the signed-in user the Teams
> and Spotify sessions belong to. The file's own comment suggests switching to
> `PrincipalType=InteractiveUser` — that element does not exist in this Task
> Scheduler schema, which uses `<Principal>` with a `<GroupId>` or `<UserId>`
> and an `<LogonType>`. The correct per-user shape is a `<UserId>` for the
> account plus `<LogonType>InteractiveToken`.

---

## Debugging a daemon that will not start

The daemon writes through `tauri-plugin-log` to the platform log directory as
well as whatever the supervisor captures.

| Platform | Log file |
| --- | --- |
| Linux | `~/.local/share/com.presencejam.app/logs/` |
| macOS | `~/Library/Logs/com.presencejam.app/` |
| Windows | `%LOCALAPPDATA%\com.presencejam.app\logs\` |

Look for `[DAEMON]` lines. The common failures in order of likelihood:

1. **Keychain never became available.** You will see six backoff attempts and
   then exit 1. Under systemd this is usually the `ProtectHome` interaction
   described above, or a `gnome-keyring` that has not been unlocked in the
   service session at all.
2. **No Spotify `client_id`.** The daemon needs the configured OAuth client;
   there is no interactive onboarding path to fall back on, so it exits 1
   rather than prompting. Run the GUI once, or export the client id, before
   handing the box to the service manager.
3. **Nothing is ever written even though it runs.** Check that the GUI is not
   also running against the same config — there is no single-instance lock in
   daemon mode, so both will poll and they will fight over the Teams status.

---

## See also

- [`USAGE.md`](../USAGE.md) § Command-line flags — the operator-facing flag table
- [`SECURITY.md`](../SECURITY.md) — token storage, keychain slots and the disclosure policy
- [`docs/RELEASING.md`](./RELEASING.md) — what ships in a release
- [`packaging/`](../packaging/) — the three service unit files
