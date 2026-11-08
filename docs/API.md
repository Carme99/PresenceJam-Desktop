# `--serve` — the localhost control API

`presencejam --serve[=PORT]` runs the same poller the GUI runs, but with no
window and no tray, and additionally exposes a small HTTP control surface on
**127.0.0.1**. Default port `8649`.

Implementation: [`src-tauri/src/serve.rs`](../src-tauri/src/serve.rs). Module tag
`[SERVE]`.

```bash
presencejam --serve          # 127.0.0.1:8649
presencejam --serve=9100     # 127.0.0.1:9100
```

The listener binds `127.0.0.1` and **only** `127.0.0.1` — never `0.0.0.0`, and
not the loopback IPv6 address. That is deliberate: the only network surface is
the local UID, but other processes run as that same UID (other apps, shared
containers, an RDP session), so the loopback bind alone is not treated as an
authentication boundary. See [Token](#token) below.

---

## Mutating routes: runtime state, never on-disk config

All four mutating routes change **runtime state only**. None writes
`config.json` or `tokens.json`, so an operator restart returns to whatever was
on disk before the call. That is the deliberate difference between this API and
the tray/GUI: the tray's snooze and profile picker persist, the HTTP routes do
not.

- `POST /pause` / `POST /resume` call the same `stop_syncing` / `start_syncing`
  machinery the tray uses.
- `POST /snooze?minutes=N` publishes a new in-memory `AppConfig` whose
  `snooze_until` is `now + N`. The poller's `snooze_gate` reads that value on its
  next iteration, so the pause takes effect immediately.
- `POST /profile?id=<id>` publishes a new in-memory `AppConfig` whose
  `active_profile` is `id` (or `None` for `base` / `none`). `effective_config`
  resolves the overlay from that pointer on the next poll.

Both of the latter two emit `config-changed` on the app-wide event bus. **The
webview does not currently listen for that event** — an open Dashboard learns
about the change when it next mounts and calls `loadConfig()`, not by being
pushed. `emit_config_changed` had no call site before these routes; they are its
only callers today, so the event is observable but not yet consumed. Treat the
dashboard's freshness as "on next mount", not "live".

**Precision on "runtime-only".** No route *writes* `config.json` — that part is
true and is enforced by a source guard in `serve.rs`. But the value does live in
the same in-memory `AppConfig` that other writers clone before saving, so it can
reach disk *indirectly*. One path is reachable in `--serve` mode:
`clear_snooze_if_expired` (`polling/gate.rs`) clones the live config, clears an
expired deadline, and saves the whole document — carrying any `active_profile`
or `snooze_until` a route set earlier along with it.

In practice this means the restart guarantee is weaker than "never survives":

| Scenario | Survives a restart? |
| --- | --- |
| `/profile` or `/snooze`, then the process exits with no intervening save | **No** |
| `/profile`, then a snooze expires while the daemon keeps running | **Yes** — the expiry sweep persists it |
| `/snooze`, then it expires (the sweep clears the field it was asked to) | **No** — the deadline is what gets cleared |

The GUI-only writers (tray snooze, `update_config`, `window.rs`) are not
reachable from `--serve`, so the expiry sweep is the one that matters. Proper
isolation — keeping runtime overrides in their own layer that a whole-config save
cannot see — is tracked separately rather than half-done here.

**Scope note.** This section used to warn that `/snooze` and `/profile` were
*inert* — they validated input, logged it, and returned `204` without changing
anything. They now apply the change. A previously valid call therefore behaves
differently after this version: it takes effect. An external scheduler that
worked around the old no-op by also driving `/pause` + `/resume` should drop
that workaround, and one that relied on these routes being harmless should
re-read the caller.

---

## Routes

| Method | Path | Auth | Returns |
| --- | --- | --- | --- |
| `GET` | `/status` | none | `200` — `SyncStatus` JSON, byte-for-byte the same shape as `--status` |
| `GET` | `/events` | none | `200` — `text/event-stream` |
| `POST` | `/pause` | **Bearer** | `204` on success, `409` if the poller could not stop |
| `POST` | `/resume` | **Bearer** | `204` on success, `409` if no session is available to start |
| `POST` | `/snooze?minutes=N` | **Bearer** | `204` — pauses sync until `now + N` (runtime-only) |
| `POST` | `/profile?id=<id>` | **Bearer** | `204` — switches the active profile (runtime-only); `404` for an unknown id |

Error responses, in full:

| Status | When | Body |
| --- | --- | --- |
| `400` | `minutes` missing, non-integer, or outside `[1, 1440]` | `minutes must be …` |
| `400` | `id` missing, empty, longer than 64 chars, or outside `[A-Za-z0-9_-]` | `id must be …` |
| `401` | mutating route without a valid `Authorization: Bearer <token>` header | `{"error":"unauthorized", …}` |
| `404` | any other path, **or** `/profile` with an id that names no configured profile | `not found` / `unknown profile: <id>` |
| `405` | a known mutating path reached with a non-`POST` method | `method not allowed` |
| `409` | `/pause` or `/resume` could not complete, or no config is loaded yet | `pause failed: …` / `resume failed: …` / `no config loaded` |
| `500` | `/status` snapshot could not be serialised | `internal error` |

Every error body except `401` is plain text. Do not parse them as JSON.

### `GET /status`

The same `SyncStatus` struct the `--status` CLI flag serialises, produced by
`commands::sync::sync_status_from_state`. Running state, snooze and its
remaining minutes, the manual-status block, the current gate reason, and both
failure counters.

### `GET /events`

A Server-Sent Events stream, open (no token required — the events carry no
secret material). It writes a preamble immediately so a consumer can tell a
working connection from a dead one before any real event has fired:

```
event: ready
data: {}

event: presence-updated
data: {"…"}

event: spotify-track-changed
data: {"…"}

event: presence-gated
data: {"…"}
```

Only those three events are streamed, verbatim from the Tauri event bus. The
internal lifecycle events (`sync-started`, `sync-stopped`,
`polling-thread-panicked`) are deliberately not forwarded — they belong to the
app's own UI, not to a public control surface.

```bash
curl -N http://127.0.0.1:8649/events
```

The connection lives as long as the TCP socket. It closes when the client
hangs up or the server stops; there is no server-side idle timeout.

### `POST /pause` and `POST /resume`

`/pause` calls `stop_syncing`; `/resume` calls `start_syncing` and returns
`409` when the session is missing — the same code the Dashboard's pause and
resume buttons go through. Both are runtime-only: they do not touch
`config.json`.

---

## Token

Mutating routes require `Authorization: Bearer <token>`. The token is 32
random bytes, base64url-encoded (43 characters).

It is generated on first boot of `--serve` and stored in the OS keychain under
`serve_token:com.presencejam.app` — never written to disk in plaintext, never
passed in `argv`, never placed in the environment. Comparison is constant-time;
that helper is the only equality check performed on it, and `redact_sensitive`
scrubs the `Authorization: Bearer …` shape from any log line.

No route writes `config.json` or `tokens.json`. The keychain slot is the only
place the token lives.

### ⚠ There is no supported way to read the token back

This is a known gap in 5.0.0, not an oversight in this page. Nothing in the
CLI prints it: there is no `--serve-token` flag, and `read_or_create_serve_token`
is called only from `--serve`'s own startup path, which keeps the value in
memory.

In practice an operator must read the keychain directly — `secret-tool lookup
service presencejam service serve_token:com.presencejam.app` on Linux, or the
platform equivalent. That works and is what the packaging units' operators have
been doing, but it is not a supported interface and it is not documented
anywhere else in this repository.

Until a supported retrieval path ships, treat `--serve`'s mutating routes as
available only to an operator who is already comfortable reading their own
system keychain.

---

## Startup resilience

If the keychain is locked or unavailable at boot — a systemd unit racing
`gnome-keyring`, for instance — `--serve` retries with exponential backoff
rather than exiting. `token_io::read_or_create_serve_token_with_backoff` makes
6 attempts spaced 1–30 s apart before giving up.

---

## See also

- [`HEADLESS.md`](./HEADLESS.md) — `--daemon` and the packaging units.
- [`../USAGE.md`](../USAGE.md) — the flags in day-to-day use.
- [`../TROUBLESHOOTING.md`](../TROUBLESHOOTING.md) — `[SERVE]` log lines.