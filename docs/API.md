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

## ⚠ Two of the four mutating routes do nothing yet

`POST /snooze` and `POST /profile` validate their input, log it, and return
`204 No Content`. **They do not change any runtime state.** No poller state,
no config, no active profile — nothing.

This is deliberate in the code and documented in its own comments: the route
contract forbids writing `config.json`, and the real snooze and profile
mechanisms are owned elsewhere, so these two routes were landed as
acknowledgement-only ahead of their follow-up slice. `/pause` and `/resume`,
which *do* call `stop_syncing` / `start_syncing`, work today.

The practical trap: an external scheduler that polls these routes sees `204`
and concludes the snooze or the profile switch happened. It did not. Do not
drive a schedule from `/snooze` or `/profile` in 5.0.0. Use `/pause` and
`/resume`, which are real.

---

## Routes

| Method | Path | Auth | Returns |
| --- | --- | --- | --- |
| `GET` | `/status` | none | `200` — `SyncStatus` JSON, byte-for-byte the same shape as `--status` |
| `GET` | `/events` | none | `200` — `text/event-stream` |
| `POST` | `/pause` | **Bearer** | `204` on success, `409` if the poller could not stop |
| `POST` | `/resume` | **Bearer** | `204` on success, `409` if no session is available to start |
| `POST` | `/snooze?minutes=N` | **Bearer** | `204` — **inert**, see above |
| `POST` | `/profile?id=<id>` | **Bearer** | `204` — **inert**, see above |

Error responses, in full:

| Status | When | Body |
| --- | --- | --- |
| `400` | `minutes` missing, non-integer, or outside `[1, 1440]` | `minutes must be …` |
| `400` | `id` missing, empty, longer than 64 chars, or outside `[A-Za-z0-9_-]` | `id must be …` |
| `401` | mutating route without a valid `Authorization: Bearer` header | `{"error":"unauthorized", …}` |
| `404` | any other path | `not found` |
| `405` | a known mutating path reached with a non-`POST` method | `method not allowed` |
| `409` | `/pause` or `/resume` could not complete | `pause failed: …` / `resume failed: …` |
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