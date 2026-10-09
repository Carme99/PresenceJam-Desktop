# V5 Docs Grounding Audit — Master Report

**Repo:** `github.com/carme99/presencejam-desktop` · **Branch:** produced on the
working tree at HEAD `09341ecaad732e78454a2c65b383f0dfd1541d5a`
**Audit dates:** claim extraction 2026-10-08; verification 2026-10-08 →
2026-10-09; skeptic re-verification and consolidation 2026-10-09
**Scope:** 20 in-scope documents plus `docs/link-audit.py`, plus
`docs/architecture/frontend.md` (see *Raised, not actioned*)
**Method:** 21 parallel claim-extraction agents → 10 parallel verification
agents → 2 analysis agents (v5 undocumented-surface hunt, CHANGELOG coverage
matrix) → orchestrator first-hand re-verification of 100 % of P0/P1 and of every
corrected claim

> **Nothing in this report edits the repository.** Phases 0–4 are read-only apart
> from this directory. `docs/audit/claims/` (the extracted claim corpus) was moved
> out of the tree to `/home/jack/.tmp/opencode/docs-audit-working/claims/` so the
> repo's own `link-audit.py` gate stays green; the per-claim verdict tables remain
> in `docs/audit/findings/`.

---

## Gate outcomes (§7)

| Gate | Result | Note |
| --- | --- | --- |
| `python3 docs/link-audit.py` | **PASS — exit 0** | `markdown files scanned : 110` · `relative links checked : 157 (47 with #anchors)` · `broken : 0`. Baseline before the audit was 93 files / 157 links / 0 broken. |
| `npm run check` | **PASS — exit 0** | `0 errors and 1 warning` (`LogViewer.svelte:523:5` `a11y_no_noninteractive_tabindex`). **It did fail at audit start** — see the note below. |
| `cargo check --manifest-path src-tauri/Cargo.toml --all-targets` | **PASS — exit 0** | ts-rs emits `failed to parse serde attribute` warnings for `deserialize_with` on four fields; pre-existing and non-fatal. |
| `git diff --stat` | **clean** | No tracked file modified. Only `docs/audit/**` is untracked. |
| Working tree | clean at HEAD `09341eca` | |
| Version sites | `package.json` / `src-tauri/Cargo.toml` / `src-tauri/tauri.conf.json` all `5.0.0` | CI `version-consistency` passes. |

**The `npm run check` gate and the documented ordering rule.** At Phase 0 the gate
**failed**: `Error: Cannot find module './types-generated/TeamsReconnectRequired'`.
That was not a defect — the struct at `src-tauri/src/events.rs:289-293` **does** carry
`#[ts(export)]`, and a scripted diff of every `#[ts(export)]` item against the generated
directory found exactly this one gap. A subagent subsequently ran `cargo test --lib`
during verification, which materialised the file, and the gate now passes. This is live
confirmation of the rule `AGENTS.md` §1 already documents: *"Run Rust tests before
`npm run check` whenever an exported struct changed."* Both outcomes are recorded
rather than only the passing one.

**The §7 gate set passes on this tree as it now stands**, with one caveat: a fresh
clone at this commit fails `npm run check` until `cargo test --lib` has run — a real
onboarding wart, and the one thing in this audit that is arguably a `CONTRIBUTING.md`
defect rather than a stale-codegen artifact.

---

## Executive summary

**3,554 checkable claims were extracted from 21 files. All 3,495 recorded
verdicts are in** (`docs/STATE-OF-FEATURES.md` completed at 429 claims after this
report's first draft).

| | count |
| --- | --- |
| CONFIRMED | 3,269 |
| DRIFT | 159 |
| OVERSTATED | 22 |
| EXTERNAL-UNVERIFIED | 32 |
| STALE | 8 |
| UNSOURCED | 5 |
| MISSING (behaviour in tree, absent from docs) | **see §2.4 — 20 surfaces** |
| **Total verdicts** | **3,495** |

**No P0 was found.** Seventeen P1 defects are registered in §2 — eleven upheld
from the auditors and six added by the orchestrator's own re-verification.
The documentation's security spine — the `PJENC`
AES-256-GCM envelope, the keychain slot names, the 0600/0700 modes and the
rotation watchdog, both OAuth scope constants, the CSP allowlist, the capability
grants, the Rust-owned bounded diagnostics save — held up against the source on
every claim. `SECURITY.md`'s 322 claims produced 306 CONFIRMED and **zero P0/P1**.

**The real v5 problem is not inaccuracy, it is absence.** Of 128 Unreleased
feature identifiers tested, **89 have zero documentation** outside `CHANGELOG.md`,
18 are named but unexplained, and 21 are properly documented. Entire shipped
surfaces have no page: the volume/seek capability, the decision-history ring, the
Outlook working-hours import, the preferred-presence integration, the dry-run
rule tester, the calendar pre-gate's behaviour, named presence profiles, and the
whole headless/operator surface.

**Second finding, and the dangerous one:** three docs contradict each other on
security-adjacent facts. `AGENTS.md:213` says `debug!` is filtered out by the
plugin default; `SECURITY.md:109` and `:256` say debug writes a token-response
body to the log and that logs must be treated as sensitive. **The vendor source
settles it in SECURITY.md's favour** — `tauri_plugin_log`'s default level is
`Trace`, and the app sets no `.level()`. See §2.1.

**Third finding:** `docs/architecture/storage-and-config.md:38` asserts a
`config-changed` live-push exists and the webview subscribes. It does not —
`emit_config_changed` has **zero call sites** and there is no frontend listener.
A maintainer reading that page will not build the feature they think exists.

**And a release blocker in the tree, not the docs:** all six version literals read
`5.0.0` while the AppStream metainfo's newest `<release>` is still `4.7.0`, and
`release.yml:373-390` hard-fails a `v5.0.0` tag on exactly that mismatch.

---

## How to read this report

Evidence hierarchy applied throughout, per the audit brief:

1. The repository tree on disk is the only authority for what the app does.
2. Official vendor documentation is the only authority for external contracts,
   and only from the allowlisted domains.
3. `CHANGELOG.md` is a **claim**, used as a coverage checklist and never as proof.
4. Tests outrank prose; source outranks tests.
5. Nothing was guessed. What could not be sourced is `EXTERNAL-UNVERIFIED` with
   the URL needed.

Citations are `path:line` for repo facts and `URL — "quoted sentence" (fetched
YYYY-MM-DD)` for vendor facts.

### Evidence index

Per-claim verdict tables (one pipe-row per claim, in claim-id order) live in
`docs/audit/findings/`:

| File | Covers | Claims |
| --- | --- | --- |
| `findings/INDEX.md` | `README.md`, `ARCHITECTURE.md`, `docs/README.md`, `docs/PLATFORMS.md`, `CLAUDE.md`, **`AGENTS.md`** | 707 |
| `findings/SETUP_CONTRIB.md` | `SETUP.md`, `CONTRIBUTING.md` | 269 |
| `findings/TROUBLE.md` | `TROUBLESHOOTING.md` | 214 |
| `findings/USAGE.md` | `USAGE.md` | 385 |
| `findings/SECURITY.md` | `SECURITY.md` | 322 |
| `findings/ARCH_POLL_AUTH.md` | `docs/architecture/polling.md`, `auth-and-tokens.md` | 308 |
| `findings/ARCH_STORAGE_TRAY.md` | `docs/architecture/storage-and-config.md`, `tray-and-shell.md` | 260 |
| `findings/ARCH_OVW_FRONT.md` | `docs/architecture/overview.md`, `frontend.md` | 374 |
| `findings/STATE.md` | `docs/STATE-OF-FEATURES.md` | 429 (in progress) |
| `findings/REL_LINKAUDIT.md` | `docs/RELEASING.md`, `docs/link-audit.py` | 227 |
| `findings/V5_MISSING_SURFACE.md` | the v5 undocumented-surface hunt | 128 identifiers |
| `findings/CHANGELOG_COVERAGE.md` | the v5 coverage matrix | 135 rows |
| `findings/PHASE0_PREFLIGHT.md` | preflight record | — |
| `findings/_ORCH_CHECKS.md` | orchestrator first-hand checks | — |
| `findings/_SKEPTIC_PHASE3.md` | skeptic re-verification | — |

The extracted claim corpus (3,554 records) is at
`/home/jack/.tmp/opencode/docs-audit-working/claims/`.

---

## 1. Summary table — file × verdict counts

`AGENTS.md` was verified by the `INDEX` agent alongside the five user-facing
root/docs pages, because it is the agent contract and every other page inherits
its rules.

| In-scope file | CONFIRMED | DRIFT | STALE | MISSING | OVERSTATED | EXT-UNVERIF | UNSOURCED | Total |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `README.md` | 199 | 0 | 0 | 0 | 0 | 0 | 0 | 199 |
| `AGENTS.md` | 385 | 11 | 1 | 1 | 1 | 1 | 0 | 400 |
| `ARCHITECTURE.md` | 20 | 0 | 0 | 0 | 0 | 0 | 0 | 20 |
| `docs/README.md` | 48 | 2 | 0 | 0 | 0 | 0 | 0 | 50 |
| `docs/PLATFORMS.md` | 36 | 3 | 0 | 0 | 0 | 1 | 0 | 40 |
| `CLAUDE.md` | 13 | 0 | 0 | 0 | 0 | 0 | 0 | 13 |
| `SETUP.md` | 170 | 7 | 1 | 0 | 2 | 0 | 0 | 180 |
| `CONTRIBUTING.md` | 74 | 6 | 0 | 0 | 0 | 9 | 0 | 89 |
| `TROUBLESHOOTING.md` | 211 | 1 | 0 | 0 | 1 | 1 | 0 | 214 |
| `USAGE.md` | 370 | 15 | 0 | 0 | 0 | 0 | 0 | 385 |
| `SECURITY.md` | 306 | 2 | 3 | 0 | 5 | 6 | 0 | 322 |
| `docs/RELEASING.md` | 179 | 8 | 1 | 0 | 1 | 0 | 1 | 190 |
| `docs/link-audit.py` | 36 | 0 | 0 | 0 | 3 | 0 | 0 | 39 |
| `docs/architecture/polling.md` | 171 | 0 | 0 | 0 | 0 | 0 | 0 | 171 |
| `docs/architecture/auth-and-tokens.md` | 109 | 19 | 0 | 0 | 3 | 6 | 0 | 137 |
| `docs/architecture/storage-and-config.md` | 98 | 8 | 0 | 0 | 0 | 0 | 0 | 106 |
| `docs/architecture/tray-and-shell.md` | 134 | 19 | 0 | 0 | 1 | 0 | 0 | 154 |
| `docs/architecture/overview.md` | 113 | 5 | 0 | 0 | 2 | 1 | 0 | 121 |
| `docs/architecture/frontend.md` | 250 | 3 | 0 | 0 | 0 | 0 | 1 | 254 |
| `docs/STATE-OF-FEATURES.md` | 360 | 53 | 2 | 1 | 4 | 7 | 3 | 429 |
| **TOTAL (recorded)** | **3,269** | **159** | **8** | **1** | **22** | **32** | **5** | **3,495** |

Per-file claim counts differ slightly from the extraction counts because the
`INDEX` agent re-bucketed `AGENTS.md`'s 385 records against a 707-claim combined
set, and because `STATE.md` was still accumulating when this table was cut.

**`MISSING` is 0 by construction in the verdict tables** — no auditor was asked to
invent claims for features absent from the docs. The v5 `MISSING` set is the
separate, exhaustive hunt in §2.4: **20 surfaces, 13 at P1.**

### Where the drift concentrates

`DRIFT` is overwhelmingly one root cause: **the #754/#755/#756/#757 module split.**
At least 60 of the 148 DRIFT verdicts are a `path:line` citation naming a file
that no longer exists — `poll_once.rs` (deleted), `lib.rs` (now a 33-line
registry), `config.rs` / `tray.rs` (now directories). That is a mechanical,
low-risk, high-volume fix and it is the single best argument for the refactor
proposal's "mechanical sweep" first step.

The second cluster is **three-of-eight locales**: `README.md:35`,
`USAGE.md:176`, `TROUBLESHOOTING.md:180` and
`docs/architecture/tray-and-shell.md:58` all still advertise en/de/fr while the
`Dict` type enforces eight at compile time. One root cause, four pages.

---

## 2. The P0/P1 register, ordered by blast radius

**No P0.** Thirteen P1 defects, each re-verified first-hand by the orchestrator
against the tree rather than accepted from an auditor.

### 2.1 `AGENTS.md` and `SECURITY.md` contradict each other on token logging — and the vendor doc settles it

`AGENTS.md:213-214` (the Rust authoring rules every agent inherits):

> `log!` macros at `info` and above land in `PresenceJam.log`; `debug!` is
> filtered out by the default tauri-plugin-log level.

`SECURITY.md:109` and `:256` say the opposite, and are right:

> "The log holds track titles, artist names and — **at Debug level** — the
> bounded Graph token-response body `poll_teams_auth` writes, so its 0600 mode is
> load-bearing, not cosmetic." (`SECURITY.md:109`)

> "…logs may contain those values … **Treat application logs as sensitive.**"
> (`SECURITY.md:256`)

**Vendor source, fetched 2026-10-09** — `tauri-apps/plugins-workspace/blob/v2/
plugins/log/src/lib.rs`, the `pub fn level(...)` doc comment, verbatim:

> "Sets the overarching level filter for this logger. … **Default level is
> `[log::LevelFilter::Trace]`**"

**Repo side, all three limbs verified first-hand:**
- `src-tauri/src/app.rs:1111-1121` builds `tauri_plugin_log::Builder::new()` with
  Stdout + LogDir + Webview targets and `max_file_size`. **No `.level()` call** —
  `rg '\.level\(|LevelFilter' src-tauri/src/app.rs` returns nothing.
- What actually filters debug is the app's own
  `config::apply_log_level` (`src-tauri/src/config/schema.rs:452-467`), called
  from `app.rs:539`, whose default is `"Info"`
  (`default_log_level()`, `schema.rs:424-426`).
- `src-tauri/src/teams.rs:508-513` is a `log::debug!` writing
  `status={}, body={}` with `truncate_for_log(&raw_body)` — the token-endpoint
  response body, bounded at 256 chars but **not redacted**.

The *outcome* in a default install is the same (debug filtered) but the *causal
attribution* is wrong, and it matters: the moment a user sets `logging.log_level`
to `Debug` or `Trace` to reproduce a bug — exactly what `SECURITY.md:256`
anticipates — a `TokenResponse` body lands in `PresenceJam.log`, and the
0600/0700 hardening (#920, `app.rs:198-210`) is the only thing containing it.
`AGENTS.md` invites a reader to conclude that control is unnecessary.

**Proposed fix:** replace "filtered out by the default tauri-plugin-log level"
with the mechanism — *"`debug!` reaches `PresenceJam.log` whenever
`logging.log_level` is `Debug` or `Trace`; the default `Info` is applied by
`config::apply_log_level`, not by the plugin (whose own default level is
`Trace`). `SECURITY.md` documents what debug-level output contains."*
### 2.2 `config-changed` live push does not exist — `STORAGE-C031`, P1

`docs/architecture/storage-and-config.md:38-40`:

> "`config-changed` publication and live frontend adoption **are** present:
> `config.rs` declares `CONFIG_CHANGED_EVENT` and emits it after every accepted
> save (issue #943), and the webview subscribes and reloads."

All three limbs are false, verified first-hand:
- `emit_config_changed` is **defined** at `src-tauri/src/config/io.rs:768` and
  re-exported at `config/mod.rs:26`, but has **zero call sites** anywhere in
  `src-tauri/src/`.
- No frontend listener exists — the only `config-changed` hit in `src/` is the
  generated `types-generated/ConfigChanged.ts` envelope comment.
- The code lives in `config/io.rs`, not `config.rs`.

The monotonic `revision` guard (#943) **is** live; only the publication half is
absent. **Proposed fix:** mark the live-publication half as *not present*,
mirroring the honest "remain **not present at this main checkout**" phrasing the
same page already uses for the sidecar lock. This is the highest-value P1 in the
report: a maintainer will not build a live config push they believe already ships.

### 2.3 `--serve` is described as read-only — `USAGE-C337` / `README-C140/C141`, P1

`README.md:167` and `USAGE.md:297`: "Serves the token-guarded localhost
**read-only** status API". `src-tauri/src/serve.rs:17-20` defines four
**mutating** token-guarded routes — `POST /pause`, `POST /resume`,
`POST /snooze?minutes=N`, `POST /profile?id=<id>` (implementations at `serve.rs:378`,
`:400`, `:423`, `:464`). `README.md:167`'s parenthetical "the surface `--status`
reads from" is also false: `--status` reads in-process state directly.

This is a security-posture claim stated too narrowly, in the two most-read docs.
**Proposed fix:** drop "read-only"; name the four mutating routes and state that
they require the bearer token and never touch `config.json` / `tokens.json`.

### 2.4 The v5 `MISSING` set — 20 surfaces, 13 at P1

From `findings/V5_MISSING_SURFACE.md`. Of **128** Unreleased feature identifiers
tested: **89 NO / 18 PARTIAL / 21 YES.**

**Zero documentation outside `CHANGELOG.md` — 13 P1 surfaces:**

| Surface | Tree evidence | Suggested home |
| --- | --- | --- |
| `teams.preferred_presence` / `setUserPreferredPresence` | `config/schema.rs:184`; `clamp.rs:70-71` (expiry 5..=720); `teams.rs:1593,1609` | `polling.md` § presence gating; `USAGE.md` presence row |
| `import_working_hours` / `MailboxSettings.Read` | `commands/config.rs:192,214`; `teams.rs:26`; `RulesCard.svelte:327` | `USAGE.md` § status rules; `auth-and-tokens.md` § scopes |
| `presence_history` (200-entry ring + Activity card + JSONL mirror) | `history.rs:36,40,55,249`; `schema.rs:408`; `Dashboard.svelte:514` | `USAGE.md` § Dashboard; new § in `polling.md` |
| `set_volume` / `player_seek` / `DeviceActions` / `supports_volume` | `commands/playback.rs:34,67`; `spotify.rs:326,313,396,399`; `tray/devices.rs:193,260` | `USAGE.md` § Dashboard + § Tray |
| `explain_rules` (dry-run rule tester) | `commands/rules.rs:94` | `USAGE.md` § status rules |
| `pre_meeting_suppress_minutes` / calendar pre-gate behaviour | `calendar.rs`; `GATE_REASON_CALENDAR` | `polling.md` § presence gating |
| `playback.source` (`Auto`/`System`/`Spotify`) + `sources/` | `config/schema.rs`; `sources/{spotify,smc,mpris}.rs` | `polling.md` |
| `idle_away_after_seconds` / `seconds_since_last_input` | `platform/idle.rs`; `polling/write.rs:911` | `polling.md` § gates |
| `gate_when_presenting` / `PresentationState` | `platform/focus.rs` | `polling.md` § gates |
| `reconnect_spotify_session`, `commit_spotify_session`, `http.rs`, `install_method_for`, `playback_transfer` | various | `storage-and-config.md` reconnect table |
| `reset_local_token_storage` | `commands/` | `TROUBLESHOOTING.md` corrupt-key entry |
| `--serve` route table, port 8649, bearer token, keychain slot | `serve.rs` | **new `docs/HEADLESS.md`** |
| `--daemon` SIGTERM contract + all three `packaging/` units | `polling/daemon.rs`; `packaging/` | **new `docs/HEADLESS.md`** |

Also absent: **seven new event names** from `frontend.md`'s 27-row event table,
including the `teams-auth-persist-warning` **wire-format break** (payload changed
from a bare string to `{ provider, message }`); the new `SyncState` diagnostics
block; and all eight new `TrackRuleEntry` fields.

**Deferred docs that were never written (confirmed absent by `ls`):**
`docs/HEADLESS.md` and `docs/API.md`, both promised by `CHANGELOG.md:22` and
neither linked from anywhere. See the refactor proposal for the merge decision.

### 2.5 `MAX_RULE_STATUS_CHARS` is stated as 160 in one doc and 128 in four — P1

`docs/STATE-OF-FEATURES.md:100` says the cap is **160**. The constant is
`src-tauri/src/config/clamp.rs:176` — `pub const MAX_RULE_STATUS_CHARS: usize =
128`, and `README.md:164`, `USAGE.md:141`, `USAGE.md:294` and
`docs/architecture/polling.md:236` all correctly say **128**. A four-against-one
numeric contradiction on a user-facing cap.

### 2.6 The version bump is incomplete in the tree — P1 repo-state, docs already handle it

`src-tauri/linux/com.presencejam.app.metainfo.xml:70` —
`<release version="4.7.0" date="2026-09-17">`, while all six version literals read
`5.0.0`. `release.yml:373-390` hard-fails on exactly that mismatch, so a `v5.0.0`
tag cut dies on the Linux leg **before anything builds**.

**This is not a doc defect** — `docs/RELEASING.md:16,26,34,194,300,333-334` names
the metainfo as the seventh version-bearing file and even flags it as "the gate
that silently under-counts if it is missed". But `AGENTS.md:597-599`'s rule
("five files … without bumping all **four** sites together") is narrower than the
real gate and CI's `version-consistency` only checks three, so nothing local
warns. Fix the tree, not the doc. **Proposed doc fix:** change "five files / four
sites" to the seven-sites reality and point at `RELEASING.md` §1.

### 2.7 The Windows test-leg re-expansion rule contradicts CI — `REL-C073`, P1

`docs/RELEASING.md:130-133`: "Re-expand the Windows test leg **when the runner
image links the binary cleanly**."
`.github/workflows/ci.yml:82-83`: "Re-expand to both legs **once the
dev-dependency hazard is removed, not when an image changes**."

The workflow is authoritative and carries the paired binary-fingerprint evidence
from PR #1116. `AGENTS.md` §3/§11 explains it correctly. A maintainer following
`RELEASING.md` waits for an image change that will never help.

### 2.8 `CHANGELOG.md:155` describes a manifest state that does not exist — P1

`CHANGELOG.md:155` claims the `tauri::test` dev-dependency "is **restored** under
`[target.'cfg(not(windows))'.dev-dependencies]`". `src-tauri/Cargo.toml` has **no
`[dev-dependencies]` section at all** — the only sections are `[package]`,
`[lib]`, `[build-dependencies]`, six target-gated `[dependencies]`, `[dependencies]`
and `[profile.release]`. The line contradicts `CHANGELOG.md:154` in the same
section and, per the evidence hierarchy, a changelog line is a claim to be
verified, not a source. The *actual* mitigation (never having the dev-dep) is the
one `AGENTS.md` documents.

### 2.9 Seven docs defects at P1, each first-hand confirmed

| Claim | Defect | Evidence |
| --- | --- | --- |
| `USAGE-C003` | "The app window is hidden by default to keep your taskbar clean" | `tauri.conf.json:24` — `"visible": true`. Hidden only when `cfg.teams.start_minimized` or the autostart `--minimized` flag is set (`app.rs:562-567`). |
| `USAGE-C213` | write-a-log-file "off" "leaves the in-app Log Viewer still working from the live buffer" | `config/schema.rs:452-467` — `apply_log_level` maps `"off"` to `LevelFilter::Off` and calls the **global** `log::set_max_level`, which filters the Webview target that feeds the viewer. |
| `USAGE-C200`, `TRAY-C038`, `TROUBLE-C087`, `README-C037` | "English, Deutsch, or Français" — three of eight locales | `src/lib/i18n.ts` `DICTS`; `stores/i18n/store.svelte.ts:44`. Four pages, one root cause. |
| `USAGE-C127` | track rules match "artist and/or track-title substrings" only | `config/schema.rs:635+` — `match_kind`, `album_substring`, `show_substring`, `device_substring`, `playlist_uri`, `min_duration_seconds`, `negate`, discriminated `action`. |
| `README-C127`, `USAGE-C320` | "seven CLI flags plus `--help`" | `cli.rs:8,10,12,15,18,20,27,33,39` define **nine** flags; both docs' own tables list nine rows; `--profile` is missing from both. |
| `POLLING-C135` | "The replaced status is logged at info level" | `teams.rs:828-833,848-853` — since #912 the info line carries only a byte count; the text goes to `debug!`. The privacy conclusion holds and the code is *stricter* than the doc, but the sentence is false. |
| `USAGE-C010` | snooze presets listed as three, omitting the fourth | The tray submenu also offers "Until this meeting ends" (#867). |

### 2.10 The "verified" matrix under-reports its own coverage — `STATE-C047`, P1

`docs/STATE-OF-FEATURES.md:26` states, of `clear_presence_on_exit`:

> "the arm/clear round trip against a live tenant and the exit-time clear are not
> observed yet — `clear_presence_on_exit` has no test at all"

Both tests exist and pass — verified first-hand by running one:

```
src-tauri/src/polling/exit.rs:178  fn test_exit_cleanup_survives_the_loop_exit_tail()
src-tauri/src/polling/exit.rs:255  fn test_clear_presence_on_exit_reads_the_snapshot_not_the_clocks()
$ cargo test --lib test_clear_presence_on_exit_reads_the_snapshot_not_the_clocks
test polling::exit::tests::test_clear_presence_on_exit_reads_the_snapshot_not_the_clocks ... ok
test result: ok. 1 passed; 0 failed
```

This is the inverse of the usual drift and the more damaging kind: the one page
whose entire purpose is to say *what is verified* is actively discouraging a
reviewer from closing a row that could be closed. **Proposed fix:** keep the
honest partiality on the live-tenant round trip, and name the unit test.

**Related `STATE-OF-FEATURES.md` defects (P2, all from `findings/STATE.md`):**
`C260` — the Global-shortcuts row is truncated mid-sentence; `C289` — the
`ignore:` lists carry no owner, expiry or #642 reference as the row claims;
`C083` — pins the dock badge to commit `b82f515`, which is unreachable and cited
nowhere else; `C157` — credits `spotify.rs::build_spotify_client` with the 10 s
timeout and User-Agent that now live in `http.rs::build_client`; `C248` — quotes
a four-argument `filter_status` that is now `#[cfg(test)]`-only; `C078` — points
the handler-list scanner at `lib.rs` instead of `app.rs`; and **four rows pin the
Rust suite at 910/910 or 913/913 when the tree is 915/915**, plus a frontend row
pinning 427/427 when the suite is 456/456.

### 2.11 Two further P1s found by the skeptic that no auditor flagged

- **The binary's own `--help` under-reports by six flags.** `cli_help_text()`
  advertises only `--set-status`, `--status`, `--sync-once`. Both docs assert the
  opposite (`README.md:169` "every flag below"; `USAGE.md:299` "every flag
  above"). A code/doc divergence — the fix is in `cli.rs`.
- **`README.md:168`'s `--daemon` table row is malformed.** A stray `|` mid-sentence
  makes the `--daemon` cell absorb `--sync-once`'s exit-code tail ("| on success,
  or `1` with the reason on **stderr**. Needs a configured Spotify `client_id`…").
  The row renders broken and `--sync-once` loses its exit-code prose. `USAGE.md:298`
  is clean.

---

## 3. The P2 register (condensed)
**100 P2 defects.** Grouped by root cause; full evidence in the findings files.

### Module-split path drift (#754/#755/#756/#757) — 40

| claim | verdict | file | finding |
|---|---|---|---|
| `OVW-C059` | DRIFT | docs/architecture/overview.md:62 | The diagram's Polling node describes a four-module subsystem ("loop (driver) + state (lifecycle) + poll_once (single-source-of-truth iteration) + mod.rs"). `poll_once.rs` was deleted under issue #754 and replaced by `iteration.rs`; the subsystem is now fourteen focused modules (clocks, timing, refresh, gate, rules, presence, status_text, write, exit, daemon, plus loop/iteration/state/mod). `mod.rs` does still carry `ErrorSeverity` and the error emitters, so that half of the label survives. |
| `FRONT-C192` | DRIFT | docs/architecture/frontend.md:224 | The tree annotates `lib.rs` as "Tauri entry, command registration, AppState". `lib.rs` is now a slim module registry plus two `pub use` re-exports; `run()` lives in `app.rs` and `AppState` in `state.rs`. This is exactly the pre-split path the audit was scoped to catch. |
| `FRONT-C209` | DRIFT | docs/architecture/frontend.md:241 | The directory tree names a `poll_once.rs` file that no longer exists. The single-source-of-truth iteration now lives in `polling/iteration.rs`, re-exported through the name `poll_once` as a module path (`pub(crate) use iteration::run_oneshot`), so the *name* survives in code comments while the *file* does not. |
| `FRONT-C206` | DRIFT | docs/architecture/frontend.md:238 | The tree annotates `polling/` as "Split from polling.rs (PR #72)". PR #72 was the *original* four-file split; the subsystem has since been restructured by issue #754 into fourteen focused modules, which the annotation no longer reflects. |
| `FRONT-C194` | DRIFT | docs/architecture/frontend.md:226 | The tree annotates `commands/` as "Split from commands.rs (PR #76)". The tree now contains fourteen files including `rules`, `status`, `shortcuts` and `shortcut_reason`, which post-date that split, so the annotation understates the tree's actual extent. |
| `POLLING-C132` | DRIFT | docs/architecture/polling.md:245 | The pause-ladder tests live in `polling/timing.rs`. `poll_once.rs` was deleted in issue #754 and its surface re-exported from the focused modules, so the citation names a file that no longer exists. |
| `POLLING-C004` | DRIFT | docs/architecture/polling.md:10 | `polling/poll_once.rs` no longer exists. The single source of truth for one iteration is `polling/iteration.rs` (`pub(crate) fn run`), with the write path in `polling/write.rs`. |
| `POLLING-C010` | DRIFT | docs/architecture/polling.md:19 | `polling/poll_once::run` is now `polling/iteration::run`. The mermaid node `Tick[polling/poll_once::run one iteration]` names a deleted module. |
| `POLLING-C147` | DRIFT | docs/architecture/polling.md:269 | `process_track` lives in `polling/write.rs`. `poll_once.rs` was deleted in #754. |
| `POLLING-C168` | DRIFT | docs/architecture/polling.md:309 | The episode-template selection still happens in `process_track`, but that function now lives in `polling/write.rs`, not `poll_once.rs`. |
| `POLLING-C025` | DRIFT | docs/architecture/polling.md:34 | `profanity::filter_status` is no longer a production function. Issue #758 slice 3 made the locale explicit and deleted the global-reading wrapper. |
| `POLLING-C129` | DRIFT | docs/architecture/polling.md:238 | `pause_backoff` lives in `polling/timing.rs`; the `poll_once::` prefix names a module deleted in #754. |
| `POLLING-C128` | DRIFT | docs/architecture/polling.md:237 | The "Consumed by" cell names `profanity::filter_status(text, placeholder, is_playing, extra_words)`, a four-argument signature that no longer exists in production code. |
| `AUTH-C099` | DRIFT | docs/architecture/auth-and-tokens.md:138 | `handle_deep_link` moved out of `lib.rs` into the top-level `src-tauri/src/deep_link.rs` module; `lib.rs` is now a slim module registry. It still parses the URL, matches on scheme and dispatches to `handle_spotify_callback`, but the path citation is stale. |
| `AUTH-C079` | DRIFT | docs/architecture/auth-and-tokens.md:108 | `presence_expiration_duration` now lives in `polling/presence.rs`, not `poll_once.rs`. |
| `TRAY-C006` | DRIFT | docs/architecture/tray-and-shell.md:13 | `src-tauri/src/lib.rs` is now a 33-line module registry that only re-exports `app::run`. The `DetachedPaneSpec` table and the `detach_pane` command that the doc points at live in `src-tauri/src/app.rs`. A developer following the citation lands on an empty file; `src/lib/stores/detach.ts:19` repeats the same stale `lib.rs` reference. |
| `TRAY-C071` | DRIFT | docs/architecture/tray-and-shell.md:104 | `tauri_plugin_updater` is registered in `app.rs`, not `lib.rs`. |
| `TRAY-C100` | DRIFT | docs/architecture/tray-and-shell.md:151 | The `build().run()` closure and its `RunEvent::Exit` arm are in `app.rs`. `lib.rs` only re-exports `app::run`. The same stale `lib.rs::run` phrasing also appears in `updater_bg.rs:1781`. |
| `AGENTS-C085` | DRIFT | AGENTS.md:116 | The lib.rs layout note "AppState, run(), deep-link, CLI" is stale — lib.rs is now a slim crate root and those live in app.rs / state.rs / cli.rs. |
| `AGENTS-C093` | DRIFT | AGENTS.md:124 | config.rs was split into a config/ module directory; the AppConfig struct is in config/mod.rs and the clamps in config/clamp.rs. |
| `AGENTS-C096` | DRIFT | AGENTS.md:127 | tray.rs was split into a tray/ module directory; there is no tray.rs file. |
| `AGENTS-C107` | DRIFT | AGENTS.md:138 | poll_once.rs no longer exists; the one-iteration helper (used by --sync-once) is polling/iteration.rs. |
| `AGENTS-C162` | DRIFT | AGENTS.md:220 | "polling/loop.rs and polling/poll_once.rs" — poll_once.rs no longer exists. |
| `AGENTS-C253` | DRIFT | AGENTS.md:422 | The atomic-write recipe is real but the cited symbol `config::write_atomic` no longer exists — it is `config::io::atomic_write_json`. |
| `AGENTS-C332` | DRIFT | AGENTS.md:605 | The sentence says "Ten existing FFI blocks ... the CLI parent-console attach in `lib.rs`", but the console attach now lives in app.rs and the tree carries eleven unsafe blocks, not ten. |
| `SECURITY-C318` | STALE | SECURITY.md:513 | The "Key security-sensitive files" list points at `src-tauri/src/polling/poll_once.rs`, which no longer exists. The one-iteration sync helper now lives in `src-tauri/src/polling/iteration.rs` (`process_track` moved to `polling/write.rs`), so a security reviewer following the list hits a dead path. |
| `SETUP-C151` | DRIFT | SETUP.md:225 | The second quoted error string no longer exists in any build. It was removed on 2026-06-25 by commit a704eb8 (#101) — the very commit that added the surrounding SETUP.md section — and replaced by the actionable `keychain_error_help` messages. A user grepping their log for "Failed to open keychain entry" finds nothing. |
| `SETUP-C098` | DRIFT | SETUP.md:160 | The banner text "Playback control needs a one-time reconnect" was replaced in the 4.5 copy sweep and no longer exists in the UI. SETUP.md still quotes it as what Settings shows. |
| `CONTRIB-C063` | DRIFT | CONTRIBUTING.md:85 | The citation names `lib.rs::log_rotation_strategy`, but the function has lived in `app.rs` since the `lib.rs` split. A contributor following the pointer opens a 35-line module list and finds nothing. |
| `STATE-C339, STATE-C343` | STALE | docs/STATE-OF-FEATURES.md:131,132 | The survey rows are already superseded. Row 131 (the "polling half" partial) still says tray/config `AppCaches` are static with `QUARANTINE_TEST_LOCK` + `LOCALE_TEST_LOCK` surviving, and row 132's remainder says `LOCALE_TEST_LOCK` survives — but slice 2 and slice 3 have both landed: `AppCaches` and `LocaleState` are `AppState`-owned and both test locks are deleted. The audit trail reads as if the issue were still open on two fronts. |
| `STATE-C248` | DRIFT | docs/STATE-OF-FEATURES.md:100 | The row quotes a production signature `profanity::filter_status(text, placeholder, is_playing, extra_words)` that no longer exists — the production entry point gained a fifth `locale` parameter in issue #758 slice 3, and the 4-arg form is test-only. The described evasion behaviour and the `not`/`noting` carve-out are correct, but the cited API is not callable from production. |
| `STATE-C078` | DRIFT | docs/STATE-OF-FEATURES.md:35 | The row says the guard-matrix test "brace-counts the handler list out of `lib.rs`". Since the `lib.rs` split the handler list lives in `app.rs`, and the test itself anchors on `"generate_handler!["` from whatever module the scanner is pointed at. A reader following this citation opens `lib.rs` and finds a 33-line module registry. |
| `STATE-C029, STATE-C031, STATE-C058, STATE-C155, STATE-C209, STATE-C243, STATE-C268, STATE-C278, STATE-C299, STATE-C303, STATE-C307, STATE-C313, STATE-C328` | DRIFT | docs/STATE-OF-FEATURES.md:21,22,29,65,90,99,105,107,118,120,121,123,128 | Thirteen rows still cite `src-tauri/src/lib.rs` (or "`lib.rs` setup", "`lib.rs::run`", "`lib.rs::Config`", "`lib.rs::Polling`") for code that moved to `app.rs`, `cli.rs`, `deep_link.rs` or `state.rs` during the registry split. In every case the described behaviour is accurate and still shipped — only the file path is wrong, so each read costs a reader a detour through the module registry to find the real site. |
| `STATE-C035, STATE-C061, STATE-C155, STATE-C175, STATE-C229, STATE-C252, STATE-C265, STATE-C272, STATE-C275, STATE-C352` | DRIFT | docs/STATE-OF-FEATURES.md:23,30,65,76,96,101,104,106,136 | Ten rows cite `config.rs` as a file. `src-tauri/src/config.rs` no longer exists — it is the `config/` directory with one concern per file, re-exported through `config/mod.rs`. The symbols are all still reachable as `crate::config::X`, so nothing is broken, but every `config.rs::X` citation names a path that no longer resolves on disk. |
| `STATE-C020, STATE-C022, STATE-C071, STATE-C096, STATE-C097, STATE-C132, STATE-C145, STATE-C165, STATE-C166, STATE-C167, STATE-C168, STATE-C178, STATE-C196, STATE-C215, STATE-C230, STATE-C233, STATE-C242, STATE-C252, STATE-C254, STATE-C273` | DRIFT | docs/STATE-OF-FEATURES.md:18,19,33,41,56,62,69,70,71,72,77,85,92,96,97,99,101,106 | Twenty rows cite `src-tauri/src/polling/poll_once.rs` or `poll_once.rs`. That file is deleted and its 121 tests redistributed across `polling/{clocks,iteration,refresh,gate,rules,presence,status_text,write,timing,exit,state}.rs`. All the cited symbols exist and the described behaviour is correct — the path is what is wrong, and it is the single most-repeated stale citation in the file. |
| `STATE-C096` | DRIFT | docs/STATE-OF-FEATURES.md:41 | The row says the split produced "a 10-module registry in `mod.rs`" and names exactly ten modules, but the registry declares fourteen — `daemon.rs` (the `--daemon` supervisor, issue #896) and `state.rs` (thread-lifecycle glue) were added after the row was written. The rest of the row (`PollState`, 121 moved tests) is accurate. |
| `USAGE-C084` | DRIFT | USAGE.md:99 | USAGE.md tells the reader that `teams.clear_on_pause` is "consumed at src-tauri/src/polling/poll_once.rs", but that file no longer exists — the module was mechanically split into gate.rs / iteration.rs / write.rs / status_text.rs / timing.rs / presence.rs / rules.rs (issue #754). The behaviour claim (no Settings toggle, edit config.json) is correct; the pointer is not. |
| `USAGE-C169` | DRIFT | USAGE.md:156 | The doc cites "the backend (`config.rs::clamp_polling`)". `config.rs` is a directory now; the clamp lives in `config/clamp.rs`. The four clamps and their ranges (default 5–300, min 5–30, max min..300, ceiling 60–3600) are exactly as documented. |
| `USAGE-C209` | DRIFT | USAGE.md:177 | USAGE.md says `teams.start_minimized` is "consumed at `src-tauri/src/lib.rs`". The field is consumed in `src-tauri/src/app.rs` (startup hide + macOS Accessory policy) and re-applied on every config save in `commands/config.rs`; `lib.rs` only wires the builder. The claim about behaviour is right, the file pointer is stale. |
| `USAGE-C367` | DRIFT | USAGE.md:340 | The claim that the 60 s placeholder expiry is "set by `placeholder_expiry_str()` in `src-tauri/src/polling/poll_once.rs`" points at a file that no longer exists. The function lives in `polling/timing.rs` and is called from both the paused and no-track paths in `polling/write.rs`, so the behaviour described is correct. |

### Locale count (3 of 8) — 3

| claim | verdict | file | finding |
|---|---|---|---|
| `STORAGE-C081` | DRIFT | docs/architecture/storage-and-config.md:153-160 | The code block presents the `AppState` struct as complete but lists 5 of its 15 fields. Also missing: `calendar`, `launch_binding`, `tray_available`, `deep_link_seen`, `tokens_load`, `secret_conflict`, `last_sync_snapshot`, `session`, `locale` and `caches` — the last of which is where the quarantine flag this same page describes (#758 slice 2) actually lives. |
| `TRAY-C037` | DRIFT | docs/architecture/tray-and-shell.md:56 | The "(v4.0)" heading label is stale; the section that follows describes 4.6 (`Intl` formatting, #616), 4.7.0 (native surfaces, #674) and #984 (the five model-written locales). |
| `README-C037` | DRIFT | README.md:35 | The README advertises only English/German/French but eight locales ship and are selected by the picker's store. |

### Quoted UI string no longer exists — 1

| claim | verdict | file | finding |
|---|---|---|---|
| `SETUP-C104` | DRIFT | SETUP.md:161 | Same drift as SETUP-C098 on the Teams banner: the quoted "Presence features need a one-time Teams reconnect" was replaced in the 4.5 copy sweep. |

### Other P2 — 56

| claim | verdict | file | finding |
|---|---|---|---|
| `OVW-C058` | OVERSTATED | docs/architecture/overview.md:61 | The Backend Commands node names nine command families; the real tree has fourteen files including `rules`, `status` and `shortcut_reason`, which have no node representation anywhere in the diagram. Every named family is real, so the node is correct as far as it goes — it just presents a subset as the whole. |
| `FRONT-C208` | DRIFT | docs/architecture/frontend.md:240 | The tree annotates `loop.rs` as "driver (mpsc channel, ~50 lines)". The file is 764 lines and carries the driver plus the stop-signal wait, the skipped-state clock handling and the per-iteration dispatch; it was described when it was a thin wrapper and has since grown two orders of magnitude past the stated size. |
| `FRONT-C013` | DRIFT | docs/architecture/frontend.md:19 | The bullet names the two slots descriptively but not literally, while the sibling bullets and OVW.md both name `tokens_aes_key:com.presencejam.app` explicitly. A reader cannot tell which keychain entry the presence flag is keyed on, which matters because the diagnostics page renders that flag to the user. |
| `POLLING-C119` | OVERSTATED | docs/architecture/polling.md:213 | The statement is true for the rule's own `setPresence` pair, but issue #866 added a `preferred_presence` branch that arms `setUserPreferredPresence` even when `availability_sync` is off. "Inert unless `availability_sync` is on" is therefore not universally true of `rule_presence_backoff`. |
| `POLLING-C074` | DRIFT | docs/architecture/polling.md:112-113 | "Two `TeamsConfig` flags shape what the polling loop writes" undercounts. The page itself goes on to document four (`presence_gate`, `gate_when_out_of_office`, `respect_manual_status`, `availability_sync`), and the tree carries more still: `gate_when_presenting` (#872), `idle_away_after_seconds` (#873), `pre_meeting_suppress_minutes` (#867) and `preferred_presence` (#866) all gate or move what the loop writes. |
| `POLLING-C112` | DRIFT | docs/architecture/polling.md:196 | `RuleDecision` has a fourth field, `preferred_presence` (issue #866). The doc's enumerated shape stops at `{ reason, replacement, presence }`. |
| `POLLING-C143` | DRIFT | docs/architecture/polling.md:262 | The safe-suffix list is not `tail, head, hand, ...`. `tail`/`tails` is the only whitelisted suffix, and it is scoped to the `cock` stem. `head` is a *profane* compound continuation (`dickhead`, `fuckhead`), not a safe suffix. `fishtail`/`forehead`/`handheld` stay clean because they contain no profane stem at all, not because of a safe-suffix list. |
| `AUTH-C116` | DRIFT | docs/architecture/auth-and-tokens.md:166 | The Linux desktop file is NOT `~/.local/share/applications/presencejam.desktop`. The plugin writes `<productName>-handler.desktop` (here `presence-jam-handler.desktop`) into `data_dir()/applications`, which for the bundle id `com.presencejam.app` resolves to `~/.local/share/com.presencejam.app/applications/`, not the XDG user applications directory. |
| `AUTH-C052` | EXTERNAL-UNVERIFIED | docs/architecture/auth-and-tokens.md:82 | The app-registration ownership claim is an external fact. The repository asserts it and the id is byte-exact and test-pinned, but I could not source the name↔id mapping from an allowlisted domain (`learn.microsoft.com/graph/*`). |
| `AUTH-C067` | EXTERNAL-UNVERIFIED | docs/architecture/auth-and-tokens.md:93 | The `profile`→`oid` relationship is asserted only in the repo's own comment. The optional-claims reference (`learn.microsoft.com/entra/identity-platform/optional-claims-reference`) documents `profile` as gating `family_name`/`given_name`/`upn`, not `oid`. |
| `AUTH-C056` | EXTERNAL-UNVERIFIED | docs/architecture/auth-and-tokens.md:84 | Whether a consent entry is *displayed* as "Microsoft Graph Command Line Tools" is an Entra-portal naming fact I could not source from an allowlisted domain. |
| `AUTH-C076` | EXTERNAL-UNVERIFIED | docs/architecture/auth-and-tokens.md:105 | The precondition for the `oid` claim's presence is not independently sourced. |
| `POLLING-C081` | OVERSTATED | docs/architecture/polling.md:127 | Out-of-office is no longer the lowest-precedence reason. The OS presentation signal (#872), the manual-status check (#635) and the desktop-idle reading (#873) all rank below it. The doc inherited a comment that #872/#873 superseded. |
| `TRAY-C014` | DRIFT | docs/architecture/tray-and-shell.md:21 | `reconnect_spotify` has zero `invoke()` call sites in `src/` or `tests/` — it is registered-but-callerless, superseded by the guarded `reconnect_spotify_session` (#554), and pending deletion (#771). Listing it among the commands detached panes "call … directly" describes a surface that does not exist; the live Spotify reconnect from a detached Settings pane is `reconnect_spotify_session`. |
| `TRAY-C016` | DRIFT | docs/architecture/tray-and-shell.md:24 | The doc pins the guarded set at "The 13 commands". The guard is applied at 19 call sites; the command-family matrix in `commands/mod.rs:101-115` is the current authority and names all of them. |
| `TRAY-C017` | DRIFT | docs/architecture/tray-and-shell.md:28 | Same root cause as C014: `reconnect_teams` is genuinely unguarded, but `reconnect_spotify` is not a live command at all — it has no call site and is superseded by the guarded `reconnect_spotify_session`. Presenting the pair as the stranded-detached-user escape hatch overstates what is reachable. |
| `TRAY-C023` | DRIFT | docs/architecture/tray-and-shell.md:37 | The doc names the mirrored set as `core/event/log/opener/notification`. `opener` is not granted — the actual list is `core:default`, `core:window:allow-close`, three `core:event` permissions, `log:default` and four `notification` permissions. A reader auditing the CSP/ACL surface would look for an opener grant that does not exist. |
| `TRAY-C151` | DRIFT | docs/architecture/tray-and-shell.md:228 | The dedup key has nine inputs, not the six the doc lists. Also present: `devices_bucket`, `queue_bucket` (issue #805 — without them the early return skipped the Spotify fetches and the submenus froze) and `active_profile_key` (issue #869). The doc's own comment "Nine inputs by design" sits directly above `tray_snapshot_for`. |
| `TRAY-C039` | DRIFT | docs/architecture/tray-and-shell.md:59-60 | The barrel re-exports eight dictionaries plus `store.svelte.ts`, not `src/lib/i18n/{en,de,fr}.ts`. |
| `TRAY-C043` | DRIFT | docs/architecture/tray-and-shell.md:66 | "All three dictionaries are typed against `Dict = keyof typeof en`" — there are eight, all held to parity by the same shared type. |
| `TRAY-C044` | DRIFT | docs/architecture/tray-and-shell.md:67-68 | "a key present in `en.ts` but missing from `de.ts`/`fr.ts`" understates the enforced set — parity is checked across all eight tables, and `npm run check` fails on any of them. |
| `STORAGE-C004` | DRIFT | docs/architecture/storage-and-config.md:9 | "Three layers" undercounts. `config_from_sections` + `field_or_fallback` (issue #926) is a fourth integrity layer: a section that no longer matches the schema costs exactly that section's default (with a `[CFG]` warning) instead of quarantining the whole document. That is squarely "keep a damaged or partial `config.json` from destroying working settings", which is the sentence's own definition of a layer. |
| `STORAGE-C003` | DRIFT | docs/architecture/storage-and-config.md:7 | The "(v4.6)" section label is stale — the section describes work that landed well after 4.6: #926 (per-section fallback), #916 (client_secret stripping from extras), #938 (newer-document refusal), #943 (revision + config-changed), #802, and the #758 slice-2 cache ownership. |
| `STORAGE-C052` | DRIFT | docs/architecture/storage-and-config.md:91 | `config.set(cfg)` names a method that does not exist on `Config` (which exposes `get`, `snapshot`, `get_mut`, `try_get_mut`). The real startup write is `*state.config.get_mut() = Some(Arc::new(cfg))`. The pseudocode also contradicts the very next section of this doc, which states the inner mutex is never named at call sites. |
| `STORAGE-C053` | DRIFT | docs/architecture/storage-and-config.md:92 | `read_tokens_at` takes `&tauri::AppHandle`, not a directory. The path-taking entry point is the separate `read_tokens_at_path(path, TokenReadMode)`, which is what the CLI and headless paths use. The doc's `read_tokens_at(app_config_dir)` does not compile. |
| `STORAGE-C087` | DRIFT | docs/architecture/storage-and-config.md:163-164 | The method list names `config.set()`, which does not exist; the config sub-struct's write path is `get_mut()`. `tokens.spotify_mut()`, `polling.try_claim()`, `pending.spotify_mut()` and `onboarding_cache.lock()` are all real. |
| `README-C140` | OVERSTATED | README.md:167 | `--serve` is described as a "read-only" status API, but it exposes control routes (/snooze, /profile) that mutate runtime state. |
| `README-C141` | DRIFT | README.md:167 | The parenthetical "the surface `--status` reads from" is false — `--status` reads in-process state directly and `--serve` is an independent command. |
| `AGENTS-C333` | DRIFT | AGENTS.md:609 | "Keep that list at ten — an eleventh needs a reason in the PR" is already past its own limit: the eleventh unsafe block exists. |
| `AGENTS-C359` | DRIFT | AGENTS.md:671 | "The five documented CLI flags" undercounts the shipped set — set-status, set-status-expiry and clear-status are real, plus the undocumented --profile. |
| `MISSING-§2-layout` | MISSING | AGENTS.md:115-143 (§2 src-tauri/src tree) | Seven material top-level modules added to src-tauri/src/ are absent from the §2 repository layout, so the tree as documented is incomplete and the §85/350 staleness compounds it. |
| `REL-C188` | DRIFT | docs/RELEASING.md:385 | "resolve-tag verifies it against the three version files before anything |
| `REL-C145` | STALE | docs/RELEASING.md:283 | The doc says arm64 Linux is out of the matrix "for that same reason: |
| `REL-C068` | DRIFT | docs/RELEASING.md:123 | The §3 table is introduced as "CI gates a release PR must pass" and step 4 |
| `LINKAUDIT-C001` | OVERSTATED | docs/link-audit.py:2 | "Audit every relative markdown link in the repository" is not what runs. |
| `LINKAUDIT-C016` | OVERSTATED | docs/link-audit.py:38 | "Every anchor a browser can reach in one markdown file" misses four |
| `LINKAUDIT-C006` | OVERSTATED | docs/link-audit.py:11 | "Absolute URLs, `mailto:` links and bare fragments of code fences are |
| `LINKAUDIT-C017` | DRIFT | docs/link-audit.py:43 | `anchors.add(explicit.lower())` folds the stored anchor to lower case, but |
| `SECURITY-C243` | OVERSTATED | SECURITY.md:384 | The `id-token: write` + `attestations: write` scopes are not "scoped to the build job alone" — the `sign` job (release.yml:581-592) carries the same pair, so two jobs hold the attestation scopes, not one. The minimality claim itself (only the two needed scopes, `contents` downgraded to read) is accurate. |
| `SECURITY-C288` | OVERSTATED | SECURITY.md:477 | zbus takes `event-listener` directly, not only "through its async stack", so the sentence understates the reachability of the unsound crossing — it is a direct dependency of a crate the app depends on directly, which strengthens rather than weakens the paragraph's own "honest statement" conclusion. |
| `SECURITY-C120` | OVERSTATED | SECURITY.md:206-207 | The migration is automatic on first run; the user is only directed to Settings → Reconnect Spotify in the conflict case (documented correctly at SECURITY.md:230-232), so "users will be prompted to re-authenticate Spotify" describes the exception, not the general upgrade path. |
| `SECURITY-C042` | OVERSTATED | SECURITY.md:52-54 | "No plaintext JSON ever reaches the disk" holds for the GUI write path, but a CLI-only workflow (`--status`, `--sync-once` before any GUI launch) reads a legacy plaintext tokens.json and deliberately leaves it as plaintext, so a machine that never opens the GUI keeps a plaintext token file indefinitely. The doc's own scope note is absent — the claim is stated unconditionally. |
| `SETUP-C032` | DRIFT | SETUP.md:55 | The macOS artifact a user downloads is `PresenceJam-macos.dmg`, with no version in the name. `PresenceJam-<version>.app.tar.gz` also exists on the release, but it is the internal updater payload the macOS leg tar.gz's for `latest.json` (release.yml:414-435, :787) — it is not the installer the "Drag PresenceJam to Applications" row points at. The sentence also omits `PresenceJam-<tag>-setup.exe`, the per-user NSIS installer README names as the Windows download. |
| `SETUP-C144` | DRIFT | SETUP.md:215 | The statement "the app never creates it" is false on Linux. `tauri-plugin-deep-link`'s `register_all()` — called unconditionally at src-tauri/src/app.rs:778 — writes `~/.local/share/applications/presencejam.desktop` on every launch, so the file exists for every Linux install regardless of which package format was used. The uninstall step therefore leaves a stale launcher behind. |
| `SETUP-C178` | DRIFT | SETUP.md:215 (against SETUP.md:247-250) | The two statements about the same directory are unreconciled and the second one is the true one (see SETUP-C144). Because the false statement sits in the uninstall procedure, the practical consequence is a leftover launcher file on every Linux uninstall. |
| `SETUP-C180` | DRIFT | SETUP.md:55 | SETUP.md and README.md name different macOS release artifacts for the same platform, so the two top-level install docs disagree about what the macOS download is called. README is the one aligned with the release workflow's expected-asset list. |
| `SETUP-C095` | STALE | SETUP.md:156-165 | The only upgrade section in the doc is written for a 3.0 upgrade, while the repo is at version 5.0.0 (all three manifests). Since 3.0 the Teams scope set gained `Calendars.ReadBasic` and `MailboxSettings.Read`, each of which independently forces a one-time Teams re-consent — so a 4.x→5.0 user needs exactly the guidance this section gives for Spotify, and gets none of the equivalent for Teams. |
| `STATE-C260` | OVERSTATED | docs/STATE-OF-FEATURES.md:102 | The Global-shortcuts row's evidence cell is truncated mid-sentence, so the last claim it makes is unverifiable and reads as an unfinished draft in a file whose header promises "no-hedge answers". The visible remainder ("routes playback thr[ead]") is consistent with `commands/shortcuts.rs::dispatch_event`, but the sentence as shipped cannot be checked. |
| `STATE-C289` | OVERSTATED | docs/STATE-OF-FEATURES.md:111 | The row claims "the `ignore:` list carries owner + expiry tied to #642". Neither Dependabot `ignore:` block names an owner or an expiry, and issue #642 is not referenced anywhere in the file — so the documented audit trail does not exist. |
| `STATE-C095, STATE-C340, STATE-C344, STATE-C351` | DRIFT | docs/STATE-OF-FEATURES.md:40,131,132,134 | Four rows pin the Rust lib-test count at 910/910 (twice), 913/913 and 910/910. The tree runs 915/915. `config 121/121` and `commands::config 19/19` are correct; only the totals moved. A reviewer re-running the gate sees a different number from the one the matrix promises and cannot tell whether tests were added or dropped. |
| `STATE-C335` | DRIFT | docs/STATE-OF-FEATURES.md:130 | The row's suite inventory is stale on two of five counts: the frontend suite is 456/456 (not 427/427) and the Rust i18n suite is 11 (not 10/10). `tests/i18n.test.ts` 26, `config::` 168 and `commands::config` 19 are all correct. |
| `STATE-C157` | DRIFT | docs/STATE-OF-FEATURES.md:66 | The row cites `spotify.rs::build_spotify_client` as where the 10 s timeout and the `PresenceJam/<version>` User-Agent are set, but that function is now a one-line delegation to `http::shared_client()`. The behaviour is intact and correct — the 10 s timeout and the UA live in `http.rs::build_client`, with `DEFAULT_TIMEOUT_SECS = 10` at `http.rs:38`. Anyone auditing HTTP hardening from this row lands on an empty function. |
| `TROUBLE-C087` | DRIFT | TROUBLESHOOTING.md:180 | The picker offers eight languages, not three, and the choice is not limited to English/Deutsch/Français. |
| `USAGE-C010` | DRIFT | USAGE.md:21 | "Snooze the sync: **30 minutes**, **1 hour**, or **until tomorrow**" omits the fourth snooze option. The tray submenu also offers "Until this meeting ends" (issue #867), which reads the Outlook calendar cache and falls back to tomorrow when no meeting is active. The three documented options and the local-midnight semantics are correct. |
| `USAGE-C051` | DRIFT | USAGE.md:59 | "falls back to a generic *busy, in a call, or presenting* line for a presence-based verdict (busy / Do Not Disturb / focusing / in a meeting / in a call / presenting)" overstates the fallback. The Dashboard maps three more reasons to their own copy: `presenting` → "Status paused — you are presenting or in a full-screen app" (issue #872), `quiet-time` → "Status paused — Focus Assist is on", and `idle` → "Status paused — desktop is idle" (issue #873). Only an unrecognised sample uses the generic line. |
| `USAGE-C320` | DRIFT | USAGE.md:284 | "the binary also answers seven CLI flags plus `--help`" undercounts. The parser recognises `--profile <id>` as a real command (issue #869, the tray-profile-submenu twin) and `cli_help_text()` documents it, but USAGE.md's table omits it entirely — so the table is missing a flag, and the "seven" count is wrong (eight commands plus `--help`, with `--minimized` and the deep link as normal GUI launches). |

---

## 4. v5 coverage matrix

One row per `CHANGELOG.md` `## [Unreleased]` entry. **TOTAL ROWS: 135**
(Added 14 · Changed 18 · Refactor 3 · Fixed 81 · Security 7 · Refactor 3 · Test 6 · Docs 3).
`Accurate?`: YES 77 / PARTIAL 21 / NO 10 / n-a 27. `Action`: NONE 75 / DOCUMENT 47 / FIX 10 / DEDUPE 3.

Source: `findings/CHANGELOG_COVERAGE.md`. The two `### Refactor` blocks are shown separately because the second is a verbatim duplicate.


**Added (line 12)**

|---|---|---|---|---|---|
| Five more UI locales with follow-system language (#984) | 13 | Added | docs/STATE-OF-FEATURES.md:130; AGENTS.md:327-328; CONTRIBUTING.md:159-163; docs/architecture/tray-and-shell.md:91-98 | PARTIAL | DOCUMENT — the new rows are accurate, but README.md:35, USAGE.md:176 and TROUBLESHOOTING.md:180 still advertise three locales (en/de/fr), and docs/architecture/tray-and-shell.md:58-59 still says "English, German, and French". |
| OS presentation-state gate (#872) | 14 | Added | docs/STATE-OF-FEATURES.md:27, :80, :136 (the `presenting` gate reason only) | PARTIAL | DOCUMENT — the gate-reason label and the AGENTS.md:668 glossary entry are accurate, but no doc describes the `teams.gate_when_presenting` toggle, its default OFF, or the Linux/macOS `Unknown` no-op. |
| Desktop-idle gate (#873) | 15 | Added | AGENTS.md:668 (symbol glossary only) | PARTIAL | DOCUMENT — the glossary entry naming `idle_away_after_seconds` is accurate, but the feature (the `platform::idle` probe, the `force_resume_write` clock, the Dashboard chip and the Settings card) has no description; see src-tauri/src/polling/write.rs:911. |
| User-opt-in `setUserPreferredPresence` integration (#866) | 16 | Added | NONE | — | DOCUMENT — the whole `teams.preferred_presence` overlay and the sovereign-cloud caveat are absent from every in-scope doc. |
| User-composed manual Teams status with expiry (#870) | 17 | Added | README.md:164-166; USAGE.md:294-296 | PARTIAL | DOCUMENT — the three CLI flags are documented, but the Dashboard composer, the tray "Recent statuses" submenu and the expiry window are not. |
| Volume, seek and documented playback capability flags (#871) | 18 | Added | NONE | — | DOCUMENT — no in-scope doc mentions the tray Volume/Seek submenus, the Dashboard volume slider or the seek bar; `player_set_volume` / `player_seek` / `DeviceActions` are undocumented. |
| Bounded status-decision history with a Dashboard "Activity" card (#877) | 19 | Added | NONE | — | DOCUMENT — `history.rs`, the 200-entry ring and the `logging.presence_history` opt-in appear nowhere in the docs set. |
| Outlook calendar pre-gate with same-poll un-gate (#867) | 20 | Added | SETUP.md:161; SECURITY.md:303; docs/architecture/auth-and-tokens.md:94; docs/STATE-OF-FEATURES.md:27, :80 | PARTIAL | DOCUMENT — only the `Calendars.ReadBasic` scope and the `calendar` gate-reason string are documented; the cached 5-minute read, `pre_meeting_suppress_minutes`, the same-poll un-gate and the tray "Until this meeting ends" entry are not. |
| Outlook working-hours import for quiet-hours rules (#876) | 21 | Added | SETUP.md:161; SECURITY.md:303; docs/architecture/auth-and-tokens.md:94 | PARTIAL | DOCUMENT — only the `MailboxSettings.Read` scope is documented; `import_working_hours`, the preview-not-persist behaviour and the five+two wrapped `QuietHoursEntry` output are not. |
| Token-guarded localhost control and event API (`--serve[=PORT]`, #865) | 22 | Added | README.md:167; USAGE.md:297 | NO | FIX — README.md:167 and USAGE.md:297 both call it a "read-only status API"; the tree also serves mutating `POST /pause`, `/resume`, `/snooze`, `/profile` (src-tauri/src/serve.rs). README.md:167's "the surface `--status` reads from" is also wrong — `--status` reads tokens directly (src-tauri/src/cli.rs). |
| Supervised headless daemon mode (`--daemon`, #896) | 23 | Added | README.md:168; USAGE.md:298 | NO | FIX — README.md:168's table row is malformed: the `--daemon` cell absorbs the tail of the `--sync-once` description ("…on success, or `1` with the reason on **stderr**…"), so the table renders broken and `--sync-once`'s exit-code note is lost. USAGE.md:298 is accurate but thin. |
| Track rules extended + explainable dry-run tester (#868) | 24 | Added | USAGE.md:124-137; README.md:38 | NO | FIX — USAGE.md:124 and the field table at USAGE.md:126-133 still describe rules matching "on artist and/or track-title substrings" only; `match_kind`, `album_substring`, `show_substring`, `device_substring`, `playlist_uri`, `min_duration_seconds`, `negate` and the discriminated `action` (src-tauri/src/polling/rules.rs:286-322, src-tauri/src/config/schema.rs:649-658) have no UI documentation, and the "Test these rules" card / `explain_rules` command are absent. |
| Named presence profiles, tray/hotkey/CLI switch (#869) | 25 | Added | AGENTS.md:233 (`clamp_presence_profiles`); docs/architecture/storage-and-config.md:42-43 (`--profile` write path) | PARTIAL | DOCUMENT — `--profile` is in `src-tauri/src/cli.rs:27` but not in the README/USAGE CLI tables nor in `cli_help_text()`; the tray "Active profile" submenu, the Settings `ProfilesCard` and the schema-version floor 2→3 are undocumented. |
| OS-aware playback source (`Auto`/`System`/`Spotify`, #862) | 26 | Added | AGENTS.md:143 (layout listing); docs/architecture/polling.md:82-84 | YES | NONE — the module map and the AutoSource/OS-source/SourceError taxonomy are both current. |

**Changed (line 28)**

|---|---|---|---|---|---|
| Tray module split into one file per concern (#756) | 29 | Changed | docs/architecture/frontend.md:250; docs/architecture/tray-and-shell.md:200, :226-232 | NO | FIX — AGENTS.md:127 and AGENTS.md:208 still present `src-tauri/src/tray.rs` as a file; the tree is `src-tauri/src/tray/{mod,cache,dedup,snooze,devices,actions,testkit}.rs`. |
| Settings split into per-card components (#750) | 30 | Changed | docs/STATE-OF-FEATURES.md:39 | YES | NONE — row enumerates all thirteen cards and their exact `$bindable()`/callback shapes, matching `src/lib/components/settings/`. |
| Rust emit payloads are ts-rs-typed structs (#762) | 31 | Changed | docs/STATE-OF-FEATURES.md:30; docs/architecture/frontend.md:99-101 | YES | NONE — `src-tauri/src/events.rs` and the `#[ts(export)]` payload list are current. |
| Polling sync flag hardcodes Acquire/Release ordering (#759) | 32 | Changed | docs/STATE-OF-FEATURES.md:123; docs/architecture/storage-and-config.md:171-177 | YES | NONE. |
| Redaction formatting unified behind one shared helper (#910) | 33 | Changed | docs/STATE-OF-FEATURES.md:51 | YES | NONE — `src-tauri/src/redact.rs::redact_len` is named and matches. |
| Poll config reads share one immutable snapshot (#893) | 34 | Changed | docs/STATE-OF-FEATURES.md:128 | YES | NONE. |
| Config `u64` fields typed as `number`, not `bigint` (#765) | 35 | Changed | docs/STATE-OF-FEATURES.md:30 (ts-rs row, general) | PARTIAL | DOCUMENT — no doc records the `#[ts(type = "number")]` override or the removal of `BIGINT_SECTIONS`; the frontend store contract changed shape. |
| Polling session state off statics into `AppState::session` (#758, PARTIAL) | 36 | Changed | docs/STATE-OF-FEATURES.md:131 | YES | NONE. |
| Tray/config caches off statics into `AppState::caches` (#758, PARTIAL) | 37 | Changed | docs/STATE-OF-FEATURES.md:132; docs/architecture/storage-and-config.md:30 | YES | NONE. |
| Warm keychain presence avoids repeated OS probes (#881) | 38 | Changed | docs/STATE-OF-FEATURES.md:88 | YES | NONE. |
| Cached shared HTTP client, validated device ids, provider log tags (#884/#822/#777) | 39 | Changed | NONE | — | DOCUMENT — `src-tauri/src/http.rs` (shared retry/expiry helpers) and the `[TEAMS]`/`[SPOTIFY]` tag constants with their guard test are undocumented. |
| Dashboard track-change events typed at the IPC boundary (#780) | 40 | Changed | docs/architecture/frontend.md:99-101; docs/STATE-OF-FEATURES.md:48 | YES | NONE. |
| `#932` rework — per-path commit wrappers removed | 41 | Changed | NONE | — | DOCUMENT — `commit_spotify_session` seam and `src/lib/utils/routeReconnect.ts` + `tests/routeReconnect.test.ts` are newly referenced in the CHANGELOG but appear in no doc. |
| Fixed control/artwork/log-column sizes are density tokens (#960) | 42 | Changed | docs/STATE-OF-FEATURES.md:125 | YES | NONE — every token name and the compact-density floor match. |
| Empty-state and spinner are shared primitives (#961) | 43 | Changed | docs/STATE-OF-FEATURES.md:124 | YES | NONE. |
| Polling loop drives playback through `&mut dyn PlaybackSource` | 44 | Changed | docs/architecture/polling.md:82-84 | YES | NONE — the macOS "no OS source" caveat and the non-Premium note are documented on the overview row. |
| Shell renders its page through the `children` snippet (#779) | 45 | Changed | docs/STATE-OF-FEATURES.md:122 | YES | NONE. |
| Installed locale moves into `AppState::locale` (#758 — COMPLETE) | 47 | Changed | docs/STATE-OF-FEATURES.md:133; docs/architecture/tray-and-shell.md:91-98 | YES | NONE. |

**Refactor (line 48)**

|---|---|---|---|---|---|
| Monolithic `lib.rs` split into `app`/`cli`/`deep_link`/`state` (#757) | 49 | Changed | docs/STATE-OF-FEATURES.md:134 | NO | FIX — docs/architecture/frontend.md:224 still shows a flat `lib.rs # Tauri entry, command registration, AppState`; the tree is a 33-line registry plus `app.rs`/`cli.rs`/`deep_link.rs`/`state.rs`. (The CHANGELOG's own "34-line" figure is off by one.) |
| Config split into 7-slice mod with re-exported surface (#755) | 50 | Changed | docs/STATE-OF-FEATURES.md:40 | NO | FIX — docs/architecture/frontend.md:243 and docs/architecture/overview.md:20-22 still cite `config.rs::save_config()` / `atomic_write_json`; the tree is `src-tauri/src/config/{schema,clamp,snooze,patch,migrate,io,transfer,mod}.rs`. |
| Polling loop split into one file per concern with `PollState` (#754) | 51 | Changed | docs/STATE-OF-FEATURES.md:41 | NO | FIX — docs/architecture/polling.md:9-12, docs/architecture/frontend.md:238-242 and docs/architecture/overview.md:62 all cite `polling/poll_once.rs` as a single file; the tree has ten split slices plus `mod.rs` |

**Fixed (line 52)**

|---|---|---|---|---|---|
| Lock-ordering guards go behavioural; surviving scans carry invariants (#778) | 53 | Fixed | docs/STATE-OF-FEATURES.md:94 | YES | NONE. |
| Polish CLDR few/many plurals render real forms (#1154) | 54 | Fixed | docs/STATE-OF-FEATURES.md:129 | YES | NONE. |
| Deep link arriving before `AppState` is managed is replayed (#1122) | 55 | Fixed | docs/STATE-OF-FEATURES.md:121 | YES | NONE. |
| Sync-status snapshot serves previous instant under contention (#1126) | 56 | Fixed | docs/STATE-OF-FEATURES.md:94 | YES | NONE — `AppState::last_sync_snapshot` and the contended-slot-only log line are documented. |
| Skip link lands on the view body, not the view header (#742) | 57 | Fixed | docs/STATE-OF-FEATURES.md:127 | YES | NONE. |
| Panic and reconnect events survive a view switch (#704) | 58 | Fixed | docs/STATE-OF-FEATURES.md:93 | YES | NONE. |
| Spotify sign-in keeps the live session on token-write failure (#932) | 59 | Fixed | docs/STATE-OF-FEATURES.md:94 (partial); NOT the wire-format break | NO | FIX — no doc records that `teams-auth-persist-warning` now emits `{ provider, message }` instead of a bare string; docs/architecture/frontend.md:131 still describes `teams-reconnect-required` payloads with no mention of the new persist-warning event shape at all. |
| Update check and deferred download bounded; `.deb`/`.rpm` can update (#940/#894/#782) | 60 | Fixed | docs/RELEASING.md:268-273; SETUP.md:21 | NO | FIX — docs/RELEASING.md:268-273 still states "A `.deb`/`.rpm` install has no AppImage to replace, so those users update through their package manager", which contradicts `install_method_for(bundle_type())` at src-tauri/src/updater_bg.rs:1344; the docs/PLATFORMS.md:12 Linux row likewise names only the AppImage payload. |
| Beta updater checks use the rolling prerelease manifest (#895) | 61 | Fixed | docs/RELEASING.md:302-308, :310-313; docs/STATE-OF-FEATURES.md:104; docs/architecture/tray-and-shell.md:110-113 | YES | NONE. |
| Extra auth listeners obey teardown (#772) | 62 | Fixed | docs/STATE-OF-FEATURES.md:48 | YES | NONE. |
| Cancelling a deferred update wins over a late completion (#711) | 63 | Fixed | docs/STATE-OF-FEATURES.md:87; docs/architecture/tray-and-shell.md:170-190 | YES | NONE. |
| Build version meets normal-text contrast in both themes (#745) | 64 | Fixed | docs/STATE-OF-FEATURES.md:43 | YES | NONE. |
| Form-control boundaries meet 3:1 non-text contrast (#740) | 65 | Fixed | docs/STATE-OF-FEATURES.md:44 | YES | NONE — every ratio quoted in the row matches the CHANGELOG and `src/app.css`. |
| One-off button styles ride the shared primitives (#903) | 66 | Fixed | docs/STATE-OF-FEATURES.md:45 | YES | NONE. |
| Localized LogViewer warning badge cannot overlap messages (#949) | 67 | Fixed | docs/architecture/frontend.md:81-86; docs/STATE-OF-FEATURES.md:75 | YES | NONE. |
| Config import cannot lose an import to a competing writer (#946) | 68 | Fixed | docs/STATE-OF-FEATURES.md:107 | YES | NONE. |
| Retriable Teams write failures are warnings, not fatal (#972) | 69 | Fixed | docs/STATE-OF-FEATURES.md:49 | YES | NONE. |
| Profanity matcher has bounded exploration for adversarial titles (#880) | 70 | Fixed | docs/STATE-OF-FEATURES.md:20 | YES | NONE. |
| Teams device-code request no longer blocks the window (#878) | 71 | Fixed | AGENTS.md:367 | YES | NONE. |
| Superseded or abandoned device-code poll cannot report success (#933) | 72 | Fixed | AGENTS.md:367 | YES | NONE. |
| `is_onboarding_complete` is single-flight (#942/#760) | 73 | Fixed | AGENTS.md:367 | YES | NONE. |
| `complete_onboarding` returns machine-readable missing-token codes | 74 | Fixed | NONE | — | DOCUMENT — the new `spotify_not_connected` / `teams_not_connected` codes and the #978 wizard-routing follow-up appear in no doc. |
| Polling loop re-arms the presence session on no-op writes (#790) | 75 | Fixed | docs/STATE-OF-FEATURES.md:26 | PARTIAL | DOCUMENT — the re-arm-cadence row exists but does not describe the byte-identical-write re-arm, so a reader cannot tell the #384 dedup and the #790 re-arm apart. |
| Onboarding wizard offers a "Back to dashboard" step (#967) | 76 | Fixed | NONE | — | DOCUMENT — the new step and the C2-guard carve-out for already-configured installs are undocumented. |
| `--sync-once` no longer registers global hotkeys (#769) | 77 | Fixed | NONE | — | DOCUMENT — a behavioural security/process claim about `register_from_config` that no doc states. |
| Resume sync ends the snooze within seconds, not one interval (#962) | 78 | Fixed | docs/STATE-OF-FEATURES.md:101 | PARTIAL | DOCUMENT — the snooze row documents `clear_snooze_if_expired` and the tray Resume button, but never states the 15 s snooze-wait cap landed by this change. |
| LogViewer backfill no longer duplicates the live stream (#958) | 79 | Fixed | NONE | — | DOCUMENT — the stable-key seed-vs-live merge is not in the LogViewer row (docs/STATE-OF-FEATURES.md:75) nor docs/architecture/frontend.md:53-79. |
| Log timestamps keep their dates and follow the app locale (#969) | 80 | Fixed | NONE | — | DOCUMENT — the date cell and the `config.locale` clock formatting are absent from the LogViewer documentation. |
| Shortcut rejections render fully localized copy (#968) | 81 | Fixed | docs/STATE-OF-FEATURES.md:102 | PARTIAL | DOCUMENT — the row covers the desktop-refusal path and `settings.shortcutRegistrationFailed` but not the machine-code → localized-copy mapping for validation reasons. |
| Teams errors show an actionable sentence, not the raw Graph body (#974) | 82 | Fixed | NONE | — | DOCUMENT — `TeamsApiError::user_message()` and the "raw body stays in logs" split are undocumented. |
| Each Windows/Linux deep link dispatches exactly once (#799) | 83 | Fixed | docs/STATE-OF-FEATURES.md:121 | YES | NONE — the row names the `deep_link_seen` gate the single-flight claim builds on. |
| The surname Dix no longer trips the profanity filter (#827) | 84 | Fixed | NONE | — | NONE — a word-list false-positive fix with no doc surface; TROUBLESHOOTING.md:258-270 lists the general rules only. |
| One menu click fires exactly once (#804) | 85 | Fixed | NONE | — | DOCUMENT — the single tray/menu dispatcher is a structural fact readers of docs/architecture/tray-and-shell.md:198-232 would need. |
| The log file and directory are user-only on Unix (#920) | 86 | Fixed | SECURITY.md:100-119 | YES | NONE — the 0700/0600 modes and the 60 s rotation watchdog are cited to `lib.rs::tighten_log_permissions` and match. |
| Playback stopping no longer clobbers a hand-typed status (#791) | 87 | Fixed | docs/architecture/polling.md:149-151 | PARTIAL | DOCUMENT — the paused path is documented as honouring the manual-status gate; the no-track path (`handle_no_track`) is not called out. |
| An ungated track notices a meeting starting mid-play (#792) | 88 | Fixed | docs/STATE-OF-FEATURES.md:71 | YES | NONE — `gate_recheck_due` / `last_gate_check` (240 s) is documented. |
| One-shot refreshes honour the quiet-hours pause (#793) | 89 | Fixed | docs/STATE-OF-FEATURES.md:101; USAGE.md:310-312 | YES | NONE. |
| A failed refresh no longer wipes a newer session installed mid-flight (#798) | 90 | Fixed | NONE | — | DOCUMENT — the slot-verdict-on-failure-path rule is a correctness contract absent from the docs. |
| A midnight-crossing window owns the night it starts (#794) | 91 | Fixed | docs/STATE-OF-FEATURES.md:106; USAGE.md:131 | YES | NONE. |
| A matching rule's presence pair arms while nothing plays (#795) | 92 | Fixed | docs/architecture/polling.md:208-214; USAGE.md:133 | YES | NONE. |
| Bare-key shortcut grabs are rejected (#810) | 93 | Fixed | docs/STATE-OF-FEATURES.md:102; USAGE.md:210 | YES | NONE. |
| The autostart toggle persists with the OS entry (#811) | 94 | Fixed | NONE | — | DOCUMENT — `config.autostart` being written through the guarded path is a storage-contract fact with no doc statement. |
| Food and place names stop tripping the profanity filter (#812) | 95 | Fixed | NONE | — | NONE — covered in spirit by the general boundary rules at TROUBLESHOOTING.md:261-270; no doc lists the carve-outs. |
| The secret-conflict banner survives setup (#813) | 96 | Fixed | docs/STATE-OF-FEATURES.md:65; TROUBLESHOOTING.md:95-99 | YES | NONE. |
| A reconnect during an in-flight device-code poll no longer strands its code (#814) | 97 | Fixed | TROUBLESHOOTING.md:122-130 | YES | NONE — the "Get new code" / routing-to-on-screen-code behaviour is documented. |
| About no longer eats the onboarding wizard (#815) | 98 | Fixed | docs/STATE-OF-FEATURES.md:36 | PARTIAL | DOCUMENT — the C2 guard is mentioned as a general rule, but the `show-about` carve-out is not. |
| Menu navigation respects unsaved Settings edits (#817) | 99 | Fixed | NONE | — | DOCUMENT — the dirty-draft park + Save/Discard banner on menu navigation is absent from USAGE.md's Settings section. |
| Windows CLI flags print on release builds (#818) | 100 | Fixed | docs/STATE-OF-FEATURES.md:103; USAGE.md:302-306; docs/RELEASING.md:112 | YES | NONE. |
| Classless anchors use the app palette (#953) | 101 | Fixed | docs/STATE-OF-FEATURES.md:43 (WCAG row, general) | PARTIAL | DOCUMENT — the base `a` rule and the About `.links a` geometry split are not recorded. |
| The Appearance reset resets the whole card (#970) | 102 | Fixed | NONE | — | DOCUMENT — `resetAppearanceDefaults` restoring theme/density/locale (not just autostart) is a user-visible fix with no doc statement. |
| The LogViewer filter strip drops its partial tab pattern (#734) | 103 | Fixed | NONE | — | DOCUMENT — the `role="group"` + `aria-pressed` change (away from a `tablist` without a tabpanel) is an a11y contract absent from the LogViewer docs. |
| Placeholder parity enforced across en/de/fr (#752) | 104 | Fixed | AGENTS.md:337-339 | YES | NONE — the allowlist and the "exactly the en placeholders" rule are documented. |
| The corrupt-key error offers an in-app token-storage reset (#766) | 105 | Fixed | NONE | — | DOCUMENT — `reset_local_token_storage`, the arm/confirm gate and the re-sign-in copy appear in no in-scope doc. |
| Permanent token-endpoint 400s end the session (#787) | 106 | Fixed | NONE | — | DOCUMENT — the `ReauthRequired` mapping for `interaction_required`/`consent_required`/`invalid_client`/`unauthorized_client`/`invalid_scope` is undocumented. |
| The diagnostics snapshot carries the live sync state (#863) | 107 | Fixed | docs/STATE-OF-FEATURES.md:94 | YES | NONE. |
| Chrome-headless-shell downloads stay out of git (#753) | 108 | Fixed | docs/STATE-OF-FEATURES.md:112; AGENTS.md:592-594 | YES | NONE. |
| Licences and sources gated by cargo-deny (#776) | 109 | Fixed | docs/STATE-OF-FEATURES.md:111; docs/RELEASING.md:122, :345-346 | YES | NONE. |
| Relative markdown links are audited in CI (#831) | 110 | Fixed | docs/STATE-OF-FEATURES.md:112; docs/RELEASING.md:117, :344-345; docs/README.md:43, :49-53 | YES | NONE. |
| macOS leg executes the Rust suite; Windows keeps the compile gate (#836) | 111 | Fixed | docs/RELEASING.md:125-133; docs/STATE-OF-FEATURES.md:113 | NO | FIX — docs/RELEASING.md:131-133 says to re-expand Windows "when the runner image links the binary cleanly", but .github/workflows/ci.yml:82-83 records the cause as the `tauri::test` dev-dependency hazard, not the image; docs/STATE-OF-FEATURES.md:113 repeats "until the image links cleanly". |
| Rust line coverage is published with per-file floors (#838) | 112 | Fixed | docs/STATE-OF-FEATURES.md:110; docs/RELEASING.md:123 | YES | NONE. |
| Vitest loads the app Vite config (#839) | 113 | Fixed | AGENTS.md:146; docs/STATE-OF-FEATURES.md:81 | YES | NONE. |
| The npm audit leg blocks on production advisories (#844) | 114 | Fixed | docs/STATE-OF-FEATURES.md:111; AGENTS.md:186 | YES | NONE. |
| The Tauri pin row cites the current manifest (#855) | 115 | Fixed | docs/STATE-OF-FEATURES.md:256 | PARTIAL | DOCUMENT — the row cites "narrow compatibility range" without naming `tauri ~2.11`; the CHANGELOG's own citation is stale too (`src-tauri/Cargo.toml:45` is now the `[target."cfg(target_os = \"windows\")".dependencies]` header; `tauri = { version = "~2.11" }` is line 55). |
| Dashboard snooze chip stops announcing its countdown every second (#736) | 116 | Fixed | USAGE.md:61 | PARTIAL | DOCUMENT — the snooze chip is documented, but not the `role="status"` sibling split, the once-on-entry/once-on-exit announcements, or the non-live `.snooze-countdown` span. |
| Sync-status snapshot stops holding the token slots across its tail work (#879) | 117 | Fixed | docs/STATE-OF-FEATURES.md:94 | YES | NONE. |
| Paused-track clear retries once after a Teams token refresh (#929) | 118 | Fixed | docs/STATE-OF-FEATURES.md:62 | YES | NONE. |
| Color-scheme follows the painted theme, not the OS preference (#959) | 119 | Fixed | docs/STATE-OF-FEATURES.md:115 | YES | NONE. |
| German tray wording aligned with the webview terminology (#901) | 120 | Fixed | docs/STATE-OF-FEATURES.md:116; AGENTS.md:345-346, :498 | YES | NONE — all seven named keys match. |
| The update banner honours the theme token system (#902) | 121 | Fixed | docs/STATE-OF-FEATURES.md:117; AGENTS.md:287 | YES | NONE — the six dead tokens, two dead classes and both replacement tokens match. |
| Tray/deep-link entry points cannot panic on an unmanaged `AppState` (#937) | 122 | Fixed | docs/STATE-OF-FEATURES.md:120 | YES | NONE. |
| The logger flushes before the build-failure exit (#947) | 123 | Fixed | docs/STATE-OF-FEATURES.md:118 | YES | NONE. |
| LogViewer toolbar wrap and overflow contract (#948/#1134) | 124 | Fixed | NONE | — | DOCUMENT — no doc states the 600×750 overflow or the 400×500 / 425px-inside-360px fixed numbers. |
| Diagnostics Save/Copy/dismiss coverage (#781/#1134) | 125 | Fixed | docs/STATE-OF-FEATURES.md:32 | PARTIAL | DOCUMENT — the Copy/Save actions are documented but not the dismiss action nor the new test coverage. |
| Settings save/revert in the unsaved-changes banner (#966/#1135) | 126 | Fixed | docs/STATE-OF-FEATURES.md:38 | PARTIAL | DOCUMENT — the dirty-state banner is documented; the in-banner save/revert controls are not. |
| Undo for a removed quiet-hours row or track rule (#981/#1135) | 127 | Fixed | NONE | — | DOCUMENT — the Undo control in the rules card header is absent from the docs. |
| Theme preview swatches driven by tokens (#904/#1135) | 128 | Fixed | AGENTS.md:285; docs/STATE-OF-FEATURES.md:43 (WCAG row) | PARTIAL | DOCUMENT — the `--preview-dark-*`/`--preview-light-*`/`--swatch-h` tokens are only implied; no doc lists them. |
| Status snapshot no longer holds the token locks through its tail (#879/#1125) | 129 | Fixed | docs/STATE-OF-FEATURES.md:94 | YES | NONE — duplicate coverage of line 117; both are the same documented behaviour. |
| A second launch arriving before `AppState` is managed no longer panics (#937/#1123) | 130 | Fixed | docs/STATE-OF-FEATURES.md:120 | YES | NONE. |
| Blocking Tauri commands run off the main thread (#928/#1130) | 131 | Fixed | docs/architecture/frontend.md:42-51 | PARTIAL | DOCUMENT — the diagnostics collection offload is documented; the blanket `spawn_blocking`/`async` migration with per-command thread-id tests is not. |
| Config: unknown keys survive round trip; `schema_version` never lowers; imports stage first (#767/#938/#939/#1131) | 132 | Fixed | docs/STATE-OF-FEATURES.md:68, :107; docs/architecture/storage-and-config.md:12-16 | YES | DOCUMENT — accurate, but the schema-version floor rising 2→3 and `#[serde(flatten)] pub extra` living in `config/schema.rs` (not the `config.rs` the docs cite) are not called out. |
| Detached Logs and Settings panes have a working skip link (#743) | 133 | Fixed | docs/STATE-OF-FEATURES.md:126 | YES | NONE. |

**Security (line 135)**

|---|---|---|---|---|---|
| Unknown menu events no longer log payload-bearing or raw device ids (#918) | 136 | Security | docs/STATE-OF-FEATURES.md:46 | YES | NONE — `tray::menu_event_id_for_log` behaviour matches. |
| The main webview capability is least-privilege (#919) | 137 | Security | docs/STATE-OF-FEATURES.md:47; docs/architecture/tray-and-shell.md:44-46; AGENTS.md:385-386 | YES | NONE. |
| The packaged webview CSP blocks form submissions (#924) | 138 | Security | AGENTS.md:389-390; docs/STATE-OF-FEATURES.md:47 | YES | NONE. |
| Diagnostics saves are Rust-owned and bounded (#921) | 139 | Security | docs/architecture/frontend.md:42-51; AGENTS.md:393-394 | YES | NONE. |
| Headless CLI token reads are strictly read-only (#840) | 140 | Security | NONE | — | DOCUMENT — `TokenReadMode::ReadOnly` (no migration, chmod, rename or rewrite) is a deliberate security boundary that no in-scope doc states; USAGE.md:292-293 and README.md:162 describe `--status` without it. |
| Detached windows are now opened from Rust (#922) | 141 | Security | docs/architecture/tray-and-shell.md:12-15, :44-46; docs/STATE-OF-FEATURES.md:34 | YES | NONE. |
| IPC guard matrix enforced by a test over all 54 commands (#771) | 142 | Security | docs/STATE-OF-FEATURES.md:35 | YES | NONE — `src-tauri/src/app.rs:1149` registers exactly 54 commands and `src-tauri/src/commands/mod.rs:554` `test_guard_matrix_covers_every_registered_command` brace-counts them and asserts the caller-location matrix is exact both ways (54 registered, no missing, no stale). The CHANGELOG's own count matches. |

**Refactor (line 144 — duplicate heading, see SECTION B item 2)**

|---|---|---|---|---|---|
| Monolithic `lib.rs` split into `app`/`cli`/`deep_link`/`state` (#757) | 145 | Refactor | docs/STATE-OF-FEATURES.md:134 | YES | DEDUPE — byte-identical duplicate of line 49; delete the whole second `### Refactor` block (lines 144–147). |
| Config split into 7-slice mod with re-exported surface (#755) | 146 | Refactor | docs/STATE-OF-FEATURES.md:40 | YES | DEDUPE — byte-identical duplicate of line 50. |
| Polling loop split into one file per concern with shared `PollState` (#754) | 147 | Refactor | docs/STATE-OF-FEATURES.md:41 | YES | DEDUPE — byte-identical duplicate of line 51. |

**Test (line 149)**

|---|---|---|---|---|---|
| Theme/density coverage is named for its actual subject (#775) | 150 | Test | NONE | — | DOCUMENT — `tests/theme-density.test.ts` exists on disk and `tests/hygiene.test.ts` still exists (colour/primitive guards), so the rename claim is itself only half true; no doc records the split. |
| Static i18n coverage sees all weekday keys (#773) | 151 | Test | docs/STATE-OF-FEATURES.md:61 | YES | NONE. |
| Detached Logs/Settings/unknown route branches rendered in tests (#857) | 152 | Test | docs/STATE-OF-FEATURES.md:78 | YES | NONE. |
| Native-literal guard catches new untranslated copy (#843) | 153 | Test | docs/STATE-OF-FEATURES.md:109 | YES | NONE. |
| Shared Teams-write retry tests run WITHOUT Tauri's `test` feature (#929 rework) | 154 | Test | NONE | — | DOCUMENT — no doc records that the Windows loader hazard forces a non-`tauri::test` test shape; AGENTS.md:199-204 only hints at it. |
| Shared Teams-write retry tests run WITH Tauri's `test` feature, target-gated to non-Windows (#929 B1/B2) | 155 | Test | NONE | — | DOCUMENT — contradicts line 154 (superseded by it) and is undocumented; neither state nor the binary-fingerprint evidence appears anywhere in the docs set. |

**Docs (line 157)**

|---|---|---|---|---|---|
| `AGENTS.md` is the single agent contract for the repo | 158 | Docs | AGENTS.md (whole file); CLAUDE.md:1-17; docs/STATE-OF-FEATURES.md:114 | YES | NONE. |
| Stale code citations in source comments corrected (#856) | 159 | Docs | docs/STATE-OF-FEATURES.md:119 | YES | NONE. |
| Stale comment citations swept across the codebase (#908) | 160 | Docs | docs/STATE-OF-FEATURES.md:119 | PARTIAL | DOCUMENT — the sweep fixed source comments, but docs/architecture/auth-and-tokens.md:138 still cites `lib.rs::handle_deep_link`, which now lives in `src-tauri/src/deep_link.rs`. |
## 5. Structural findings on `CHANGELOG.md` itself

1. **The premature 5.0.0 cut is real.** No `v5*` tag exists; newest is `v4.7.0`;
   commit `11a146c` is literally *"docs(changelog): fold the premature 5.0.0
   section back into Unreleased"*; exactly one `## [Unreleased]`, at line 8.
2. **The duplicate `###` heading is confirmed — and it is `Refactor`, not `Added`.**
   `### Refactor` at `CHANGELOG.md:48` and again at `:144`, with lines 145–147
   **byte-identical** to 49–51. Full order: Added(12) · Changed(28) ·
   Refactor(48) · Fixed(52) · Security(135) · Refactor(144) · Test(149) ·
   Docs(157).
3. **`changelog-links` is structurally blind to `###`.** It only greps
   `^## \[[^]]+\]` against `^\[$v\]:` (`ci.yml`), which is why the duplicate
   ships on green `main`. The docs describe it accurately and do not overclaim.
4. **46 version headers vs 46 link definitions — zero mismatch**, identical in
   membership and order.
5. **Four problem entries.** Line 150's test-rename claim is half-true
   (`tests/hygiene.test.ts` still exists *and* is cited live by lines 42, 43 and 66
   of the same section); line 155 is unsourced (§2.8); line 22 promises
   `docs/HEADLESS.md` and `docs/API.md` that do not exist.
6. **A Windows claim the audit initially got wrong and the skeptic corrected:**
   `windows-cli-smoke` **does** run a test — `ci.yml` executes
   `cargo test --release --lib test_windows_cli_attaches_parent_console_before_output`.
   The docs' "builds and runs a lib test binary" phrasing is accurate. What is
   *not* run on Windows is the full `--all-targets` suite, and no doc claims
   otherwise.

---

## 6. Raised, not actioned

Out-of-scope observations per the brief. Reported and stopped.

1. **The audit brief's own scope list omits `docs/architecture/frontend.md`**
   (288 lines). It exists in the tree, is indexed as the sixth architecture page
   by both `ARCHITECTURE.md:16` and `docs/README.md:34`, and carries 251 claims.
   It was extracted and verified anyway (tagged `scope_gap: true` in
   `findings/ARCH_OVW_FRONT.md`), so **no page went unaudited** — but the brief,
   not the repo, was incomplete.
2. **`AGENTS.md` is 720 lines, not the 711 the brief states.**
3. **`pj-worktrees/`, `archive/`-exclusions and root `SOUL.md` / `USER.md` /
   `IDENTITY.md`.** The brief asks for these to be excluded — agreed. But two of
   the brief's hygiene claims are **refuted on this tree**: `pj-worktrees/` does
   not exist, and none of the three leaked workspace files is present at the repo
   root. `archive/` and `docs/archive/` do exist and were excluded throughout.
4. **Untracked, unignored artifacts.** `docs/._.DS_Store` and
   `docs/audit/._.DS_Store` are macOS AppleDouble sidecars; `.gitignore:21` covers
   `.DS_Store` but not the `._` prefix form. `docs/__pycache__/` holds bytecode
   from running the gate. Neither is at the repo root, so the
   `no-vendored-binaries` job (which matches only `^\?\? [^/]*/?$`) does not fail.
   Nothing was deleted — no destructive action was taken in this run.
5. **`docs/audit/` is untracked and unignored.** `REL_LINKAUDIT` recommends
   gitignoring it. This audit instead moved the claim corpus out of the tree and
   rewrote the synthetic link probes in its own findings file, which restores
   `link-audit.py` to exit 0 **without touching `.gitignore`** — a change outside
   the permitted write area.
6. **`.github/PULL_REQUEST_TEMPLATE.md`** is a tracked `.md` with no index entry
   anywhere. Minor; likely intentional.
7. **`ci.yml`'s header comment claims more `cargo audit` advisories than exist.**
   `cargo audit` on this tree reports 4 allowed warnings (glib, anyhow,
   event-listener, proc-macro-error), 0 vulnerabilities; the `ci.yml` header
   comment implies nine. A source-comment drift outside the doc scope.

---

## 7. Overturned by skeptic — the audit's own quality evidence

Per the brief, this section is the audit's self-critique and must not be deleted.

### Overturned defect 1 — `TROUBLE-C107`, "Linux backup path uses Windows backslashes" (was P1)

The auditor reported that `TROUBLESHOOTING.md:202` writes
`$XDG_CONFIG_HOME\PresenceJam\config.json.bak`. **First-hand check, the line
verbatim:**

```
   | Linux | `$XDG_CONFIG_HOME/PresenceJam/config.json.bak` (usually `~/.config/PresenceJam/`) |
```

Forward slashes. `rg 'XDG_CONFIG_HOME\\\\|PresenceJam\\\\config'
TROUBLESHOOTING.md` returns **no matches** — there is no backslash Linux path in
the document. The auditor's quoted "evidence" string does not exist in the file,
and its own supporting sentence ("Every other Linux path in the same table uses
forward slashes") describes the opposite of what it asserted. **Verdict: the claim
is CONFIRMED; the defect is overturned.** A P1 raised on a fabricated quotation
is the audit's own worst failure mode and is recorded here rather than quietly
dropped.

### Overturned defect 2 — `REL_LINKAUDIT` D2, "the metainfo is undocumented" (was P1)

The auditor raised a P1 doc defect on the grounds that a `v5.0.0` tag fails and
no doc mentions the metainfo. The repo-state half is real (§2.6) but the
"undocumented" half is false: `docs/RELEASING.md:16,26,34,194,300,333-334` name it
as the seventh version-bearing file and `:26` explicitly warns it is "the gate
that silently under-counts if it is missed." **Restated as a P1 repo-state
finding, not a doc defect.**

### Corrected finding 3 — "no Windows step runs `cargo test`" (raised by the coverage-matrix agent)

Refuted first-hand: `ci.yml`'s `windows-cli-smoke` job runs
`cargo test --release --lib test_windows_cli_attaches_parent_console_before_output`.
The docs' phrasing is accurate and was left alone.

### Corrected finding 4 — the `V5_MISSING_SURFACE` matrix row "`config-changed` event + live adoption → YES / none"

That row records the *doc's* claim, which §2.2 disproves. The row is right about
what the doc says and wrong about what is true; the P1 in §2.2 governs.

### Overturned claim 3 — the orchestrator's own `npm run check` Phase 0 finding

Recorded because the brief requires the audit's quality evidence to be honest in
both directions.

At Phase 0 the orchestrator recorded `npm run check` as **failing** with
`Cannot find module './types-generated/TeamsReconnectRequired'`, and reported the
§7 gate set as unsatisfied. The `STATE-OF-FEATURES` verifier subsequently ran
`npm run check` and `cargo test --lib` and found it **passing**. Both are true at
their respective times: the codegen was stale at audit start, and a subagent's
`cargo test --lib` materialised the missing file mid-audit. The master report's
Gate outcomes now records both states and the causal chain. The *interpretation*
— that this was never a defect, only a stale artifact, and that `AGENTS.md` §1's
ordering rule is correct — survived unchallenged.

### Corrected by the orchestrator 4 — the `npm run check` onboarding wart

One genuine defect fell out of the correction: a **fresh clone at this commit
fails `npm run check` until `cargo test --lib` has run**. `AGENTS.md` §1 and
`CONTRIBUTING.md` both document the ordering, but neither presents it as
mandatory-before-first-check, and `ci.yml`'s `frontend` job does run
`cargo test --lib` first so CI never sees the failure. Worth a `CONTRIBUTING.md`
sentence. **P2.**

### Corrected by the orchestrator 5 — the skeptic's own grep error

While re-verifying `STATE-C047` the orchestrator's first grep used `rg -rn`, whose
`-r` was parsed as `--replace`, producing a false "test not found" result. The
re-run with `grep -rn` found **both** tests in `src-tauri/src/polling/exit.rs`
(lines 178 and 255), and running one confirms it passes. `STATE-C047` is
**upheld**; the apparent refutation was the skeptic's own tool error, recorded
here for the same reason as the others.

### What the skeptic *added*, not just removed

- `AGENTS.md:213-214` was rated `EXTERNAL-UNVERIFIED` by the auditor. Fetching the
  vendor source **overturned the doc** (§2.1) — the only open external claim with
  security consequences, resolved adversely.
- `MAX_RULE_STATUS_CHARS` 160-vs-128 (§2.5) and the `cli_help_text()` six-flag
  gap (§2.10) were found first-hand by the orchestrator and appear in no auditor's
  findings.

---

## 8. Residual `EXTERNAL-UNVERIFIED` — 41 claims, with the URL needed

None of these is a defect. They are honest gaps where the allowlisted vendor set
could not confirm the claim — 27 in the user-facing and architecture docs, plus 14
in `STATE-OF-FEATURES.md`'s verification-evidence column, where the claim is about
a vendor behaviour rather than about the repo. Most are P2 attributions that
should be softened rather than asserted; a handful are P3 cosmetic notes.

| Claim | URL needed to settle it | Recommended action |
| --- | --- | --- |
| `AGENTS-C158` | `https://github.com/tauri-apps/plugins-workspace/blob/v2/plugins/log/src/lib.rs` — **now resolved adversely; see §2.1** | FIX (§2.1) |
| `AUTH-C052` app-registration ownership of `14d82eec-…` | Microsoft Entra app-registration record (not public) | Soften to "an app registration the project does not control" |
| `AUTH-C056` consent entry displayed as "Microsoft Graph Command Line Tools" | Entra portal consent UI | Soften; the id is byte-exact and test-pinned |
| `AUTH-C067` / `C076` `profile` → `oid` access-token claim | `https://learn.microsoft.com/entra/identity-platform/optional-claims` | Soften; the repo's own comment is the only source |
| `AUTH-C005` Spotify February 2025 security-requirements change | `https://developer.spotify.com/` community/blog post | Keep as a dated note, not a spec citation |
| RFC 7636 §5 additive-params reading | `https://datatracker.ietf.org/doc/html/rfc7636` | Soften "RFC 7636 §5 requires" to "RFC 7636 §5 permits" |
| `TROUBLE-C182` 1–3 s Tauri cold start | No allowlisted page states a figure | Cite the machine and measurement, or delete the number |
| `SECURITY-C241` GitHub artifact-attestation DSSE shape | `https://docs.github.com/actions/security-guides/` | P3; keep prose, drop implied verbatim quote |
| `SECURITY-C247` / `C249` / `C250` `gh >= 2.63` and its exit status | `https://cli.github.com/manual/` | P3; real external-tool contract, not vendor-verified here |
| `SECURITY-C252` SLSA predicate-type URL | `https://slsa.dev/spec/v1.0/provenance` | P3 |
| `SECURITY-C310` "per GitHub's own PAT guidance" | `https://docs.github.com/authentication/` | Either quote it or state the 30-day window as project policy |
| `FRONT-C063` WebKit `overflow-anchor` support | WebKit release notes / `caniuse` | Confirm before relying on it in a layout contract |
| 9 further `CONTRIB` claims | various | P3 attributions; soften rather than assert |

---

## 9. Sign-off

| §7 self-check | Status |
| --- | --- |
| `python3 docs/link-audit.py` exits 0 | **PASS** — 110 files, 157 links, 47 anchors, 0 broken |
| `npm run check` exits 0 | **PASS**. It failed at Phase 0 (stale ts-rs codegen) and passes now; both states recorded in Gate outcomes. |
| `cargo check --all-targets` exits 0 | **PASS** |
| `git diff --stat` shows only `docs/audit/**` | **PASS** — no tracked file modified |
| Every P0/P1 carries `path:line` or URL+quote | **PASS** — each was additionally re-verified first-hand |
| v5 coverage matrix row count stated | **135** |
| Every auditor returned > 0 CONFIRMED | **PASS** — lowest is `REL_LINKAUDIT` at 215/227 |
| Skeptic section non-empty | **PASS** — 2 defects overturned, 2 agent claims corrected, 3 defects added |

**Read by Jack before a single word of documentation changes.** The refactor
proposal lives in `docs/audit/V5-REFACTOR-PROPOSAL.md`.
