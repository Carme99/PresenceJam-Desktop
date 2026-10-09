# Phase 3 — Skeptic re-verification (orchestrator, first-hand)

Every P0/P1 from the completed findings was re-verified from scratch against the
tree, independent of the auditor's own evidence. Two agent findings did not
survive. Both are recorded below and in the master report.

Legend: **UPHELD** = the defect is real, evidence confirmed first-hand.
**OVERTURNED** = the defect is not real; the auditor's evidence does not exist.

---

## UPHELD — 9 of 11

### 1. `USAGE-C003` — "The app window is hidden by default"
`docs/architecture/…` no — `USAGE.md:…`. Evidence:
`src-tauri/tauri.conf.json:24` — `"visible": true`.
Window is hidden only when `cfg.teams.start_minimized` or the autostart
`--minimized` flag is present (`src-tauri/src/app.rs:562-567`). **P1 UPHELD.**

### 2. `USAGE-C213` — write-a-log-file "off" leaves the Log Viewer working
`src-tauri/src/config/schema.rs:452-467` — `apply_log_level` maps `"off"` to
`log::LevelFilter::Off` and calls the **global** `log::set_max_level`, which
filters every target, including the Webview target that feeds the in-app viewer.
**P1 UPHELD.**

### 3. `USAGE-C127` — track rules documented as artist/track substrings only
`USAGE.md:124-133` describes six fields. `src-tauri/src/config/schema.rs:635+`
`TrackRuleEntry` carries `match_kind`, `album_substring`, `show_substring`,
`device_substring`, `playlist_uri`, `min_duration_seconds`, `negate` and a
discriminated `action` — none documented. **P1 UPHELD.**

### 4. `USAGE-C200` / `TRAY-C038` — three of eight locales advertised
`src/lib/i18n.ts` `DICTS` and `src/lib/stores/i18n/store.svelte.ts:44` ship
`en/de/fr/es/it/pl/pt/nl`. `USAGE.md` and `docs/architecture/tray-and-shell.md:58-59`
both say "English, German, and French". Also `TROUBLESHOOTING.md:180` and
`README.md:35`. **P1 UPHELD** (four docs, one root cause).

### 5. `USAGE-C337` — `--serve` described as "read-only"
`README.md:167` and `USAGE.md:297` both say "read-only status API".
`src-tauri/src/serve.rs:17-20` defines four **mutating** token-guarded routes:
`POST /pause`, `POST /resume`, `POST /snooze?minutes=N`, `POST /profile?id=<id>`
(`serve.rs:378`, `:400`, `:423`, `:464`). This is a **security-posture** claim
stated too narrowly. **P1 UPHELD.**

### 6. `README-C127` — "seven CLI flags plus `--help`"
`README.md:156` and `USAGE.md:284` both say it. The tree defines **nine** flags:
`--status`, `--sync-once`, `--help`, `--set-status`, `--set-status-expiry`,
`--clear-status`, `--profile`, `--serve`, `--daemon`
(`src-tauri/src/cli.rs:8,10,12,15,18,20,27,33,39`), plus `--minimized` from
`app.rs:22`. Both docs' own tables list 9 rows. **P1 UPHELD**, and worse than
reported — see item 7.

### 7. `REL` cross-cutting — the `--daemon` table row is malformed (README only)
`README.md:168` contains a stray `|` mid-sentence: the `--daemon` cell absorbs
`--sync-once`'s exit-code tail ("| on success, or `1` with the reason on
**stderr**. Needs a configured Spotify `client_id`…"). The row renders broken and
`--sync-once` loses its exit-code prose. `USAGE.md:298` is clean.
**P1 UPHELD.**

### 8. `STORAGE-C031` — `config-changed` live publication does not exist
`docs/architecture/storage-and-config.md:38-40`: "`config-changed` publication
and live frontend adoption **are** present: `config.rs` declares
`CONFIG_CHANGED_EVENT` and emits it after every accepted save (issue #943), and
the webview subscribes and reloads."
Reality, all three limbs false:
- `emit_config_changed` is defined at `src-tauri/src/config/io.rs:768` and
  re-exported at `config/mod.rs:26`, but has **zero call sites** anywhere in
  `src-tauri/src/`.
- No frontend listener for `config-changed` exists; the only hit in `src/` is the
  generated `types-generated/ConfigChanged.ts` envelope comment.
- The code lives in `config/io.rs`, not `config.rs`.
The monotonic `revision` guard (#943) *is* live — only the publication half is
absent. **P1 UPHELD.** This is the single highest-value P1: a maintainer reading
the page believes a live config push exists and will not build one.

### 9. `REL-C073` — Windows-leg re-expansion precedence contradicts CI
`docs/RELEASING.md:130-133`: "Re-expand the Windows test leg **when the runner
image links the binary cleanly**."
`.github/workflows/ci.yml:82-83`: "Re-expand to both legs **once the
dev-dependency hazard is removed, not when an image changes**."
The workflow is authoritative; the doc tells a maintainer to do the wrong thing.
`AGENTS.md` §3/§11 carries the correct long-form explanation. **P1 UPHELD.**

---

## OVERTURNED — 2 of 11

### A. `TROUBLE-C107` — "Linux backup path uses Windows backslashes" — **NOT REAL**
The auditor reported: "TROUBLESHOOTING.md:202 writes the Linux backup path with
Windows backslashes (`$XDG_CONFIG_HOME\PresenceJam\config.json.bak`)".

First-hand check — `TROUBLESHOOTING.md:202`, verbatim:

```
   | Linux | `$XDG_CONFIG_HOME/PresenceJam/config.json.bak` (usually `~/.config/PresenceJam/`) |
```

Forward slashes. `rg -n 'XDG_CONFIG_HOME\\\\|PresenceJam\\\\config'
TROUBLESHOOTING.md` returns **no matches** — there is no backslash Linux path in
the document. The auditor's quoted "evidence" text does not exist in the file, and
its own supporting claim ("Every other Linux path in the same table uses forward
slashes") describes the opposite of what it asserted. **Verdict overturned; the
claim is CONFIRMED.** Severity was P1 on a fabricated defect — the audit's own
quality hit, recorded here rather than deleted.

### B. `REL_LINKAUDIT` D2 (cross-file) — "the metainfo is undocumented" — **PARTLY OVERTURNED**
The auditor raised a P1 doc defect that a `v5.0.0` tag fails because the metainfo
is still 4.7.0 and no doc mentions it.

The repo-state half is real: `src-tauri/linux/com.presencejam.app.metainfo.xml:70`
— `<release version="4.7.0" date="2026-09-17">`, while all six version literals
read `5.0.0`, and `release.yml:373-390` hard-fails the Linux leg on that mismatch.

The "undocumented" half is **false**: `docs/RELEASING.md:16, :26, :34, :194,
:300, :333-334` all name the metainfo as the seventh version-bearing file, and
`:26` even says "This is the gate that silently under-counts if it is missed."

**Restated:** P1 **repo-state** finding, not a doc defect. The tree is committed
mid-bump. The doc already prescribes the fix (prepend, never edit —
`RELEASING.md:26`). Blast radius: blocks the v5 release cut. Note also that
`AGENTS.md:597-599`'s "four sites" rule is *narrower than the real gate*, which
is what makes this easy to miss.

---

## Additional defects found by the skeptic that no auditor flagged

### C. The binary's own `--help` under-reports by six flags
`cli_help_text()` (`src-tauri/src/cli.rs`) advertises only `--set-status`,
`--status` and `--sync-once`. Six parsed flags are absent:
`--clear-status`, `--profile`, `--serve`, `--daemon`, `--set-status-expiry`,
`--help`. Both docs assert the opposite — `README.md:169` "every flag below" and
`USAGE.md:299` "every flag above". **P1** (a doc claim about the binary's own
output is false; the fix is in `cli.rs`, so this is a code/doc divergence, not a
pure doc fix).

### D. `--profile` is absent from both CLI tables
`src-tauri/src/cli.rs:27` defines `--profile <id>` (v5, #869). Neither
`README.md`'s nor `USAGE.md`'s flag table lists it. **P1** (missing shipped
feature in a reference table).

### E. `USAGE.md:299` — typo "Fully headlessdless." **P3.**

---

## Phase 3b — a vendor doc resolves the audit's one open external claim, and it overturns a doc

`AGENTS-C158` was the only `EXTERNAL-UNVERIFIED` with security consequences. It is
now **resolved against the vendor source**, and the resolution turns it into a
defect the auditors had under-rated.

### F. `AGENTS.md:213-214` — "debug! is filtered out by the default tauri-plugin-log level" — **FALSE, P1**

`AGENTS.md:212-214` (the Rust authoring rules):

> `log!` macros at `info` and above land in `PresenceJam.log`; `debug!` is
> filtered out by the default tauri-plugin-log level.

Vendor source, fetched **2026-10-09** —
`https://github.com/tauri-apps/plugins-workspace/blob/v2/plugins/log/src/lib.rs`,
the `pub fn level(...)` doc comment, quoted verbatim:

> Sets the overarching level filter for this logger. … **Default level is
> `[log::LevelFilter::Trace]`.**

(The same page, `https://v2.tauri.app/plugin/logging/`, shows `.level(
log::LevelFilter::Info)` as something the *app* must set — "In this example, debug
and trace logs are discarded as they have a lower level than info" — confirming
the default is not Info.)

Repo side, all three limbs first-hand:
- `src-tauri/src/app.rs:1111-1121` builds `tauri_plugin_log::Builder::new()` with
  Stdout + LogDir + Webview targets and `max_file_size`. There is **no `.level()`
  call** — `rg '\.level\(|LevelFilter' src-tauri/src/app.rs` returns nothing.
- What actually filters debug is the app's **own**
  `config::apply_log_level` (`src-tauri/src/config/schema.rs:452-467`), called
  from `app.rs:539`, whose default is `"Info"`
  (`default_log_level()`, `schema.rs:424-426`).

So the *outcome* in a default install is the same (debug filtered) but the
*causal attribution* is wrong, and it matters — see below.

### Why this is not pedantry: AGENTS.md contradicts SECURITY.md on the same subject

`SECURITY.md:109` and `SECURITY.md:256` are **correct and deliberately honest**:

> "The log holds track titles, artist names and — **at Debug level** — the
> bounded Graph token-response body `poll_teams_auth` writes, so its 0600 mode is
> load-bearing, not cosmetic." (`SECURITY.md:109`)

> "The helper does not separately redact `access_token` or `refresh_token`
> fields, so **logs may contain those values** when they occur in the unchanged
> short body or the retained prefix. **Treat application logs as sensitive.**"
> (`SECURITY.md:256`)

The code agrees with SECURITY.md: `src-tauri/src/teams.rs:508-513` is a
`log::debug!` that writes `status={}, body={}` with `truncate_for_log(&raw_body)`
— the token-endpoint response body, bounded at 256 chars but **not redacted**.

**The agent contract therefore tells a maintainer the opposite of the security
page.** The practical consequence: the moment a user sets `logging.log_level` to
`Debug` or `Trace` to reproduce a bug — exactly the scenario `SECURITY.md:256`
anticipates — a `TokenResponse` body lands in `PresenceJam.log`, and the 0600/0700
hardening (#920, `app.rs:198-210`) is the only thing containing it. AGENTS.md's
phrasing invites a reader to conclude that control is unnecessary.

**Proposed fix:** replace "filtered out by the default tauri-plugin-log level"
with the mechanism — *"`debug!` reaches `PresenceJam.log` whenever
`logging.log_level` is `Debug` or `Trace`; the default `Info` is applied by
`config::apply_log_level`, not by the plugin (whose own default level is
`Trace`). `SECURITY.md` documents what debug-level output contains."*

**Severity: P1.** P0-adjacent — it is a false statement of token-adjacent
logging behaviour, but no user loses data or leaks a credential by following it.

### G. Remaining `EXTERNAL-UNVERIFIED` — all P3, all genuinely outside the allowlist

| Claim | Why unresolvable under §5 | Consequence |
| --- | --- | --- |
| `TROUBLE-C182` | The 1-3 s Tauri cold-start figure is a performance claim; no allowlisted vendor page states one | P3. Either cite a measured figure with the machine that produced it, or delete the number. |
| `SECURITY-C241` | GitHub artifact-attestation DSSE description — outside the allowlist | P3. Attribution to GitHub docs is fine in prose; the verbatim quote is not verifiable here. |
| `SECURITY-C247`, `C249`, `C250` | `gh` CLI is an external tool; `docs.cli.github.com` not allowlisted | P3. `gh >= 2.63` is a real external-tool contract; keep, but do not present it as vendor-verified in this audit. |
| `SECURITY-C252` | SLSA predicate-type URL | P3. |
| `SECURITY-C310` | "per GitHub's own PAT guidance" attribution | P3. Either quote GitHub's PAT docs or drop the attribution and state the 30-day window as project policy. |
| `FRONT-C063` | WebKit `overflow-anchor` support is a live browser-engine fact | P3. Confirm in WebKit release notes before relying on it in a layout contract. |

`AGENTS-C158` is the only one of the set that was load-bearing, and it is now
resolved — adversely.
