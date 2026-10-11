# V5 Documentation Refactor — Structure Proposal

Companion to `docs/audit/V5-DOCS-GROUNDING.md`. **Structure only — no rewritten
prose.** Every recommendation cites the claim IDs that justify it from the master
report and the per-claim verdict tables in `docs/audit/findings/`.

Status: **proposal, not instructions.** Nothing here has been actioned.

---

## 0. The one-paragraph diagnosis

The documentation is **accurate but incomplete, and structurally behind the
code.** 3,178 of 3,386 recorded claims are CONFIRMED, and the security spine is
clean. But the #754/#755/#756/#757 module split left ≥60 stale `path:line`
citations, and v5 added 20 shipped surfaces that no page describes. The docs are
therefore not wrong so much as *frozen at 4.6* — every page reads as a correct
description of a slightly older program.

That diagnosis drives the proposal: **do not restructure for elegance.** Fix the
facts, split the one page that has outgrown itself, and add the two pages the
changelog already promised. A redesign that moves accurate content risks
breaking the 21 live cross-doc anchor links and the 14 inbound links into
`ARCHITECTURE.md` for no gain.

---

## 1. Per-file verdicts

`KEEP` = accurate, structural changes only. `REWRITE` = content changes, same
page. `SPLIT` = divide into two pages. `MERGE INTO` = fold into another page.
`ARCHIVE` = move to `docs/archive/`.

| File | Verdict | One-line rationale | Justifying claims |
| --- | --- | --- | --- |
| `README.md` | **REWRITE** | The front door carries four P1s, including a malformed table row and a wrong CLI count. | `README-C127`, `README-C140/C141`, `README-C037`, `SETUP-C180` |
| `SETUP.md` | **REWRITE** | The only upgrade section is written for 3.0, two quoted UI strings no longer exist, and the `.desktop` claim is false. | `SETUP-C095`, `C098`, `C104`, `C144`, `C151`, `C032` |
| `USAGE.md` | **REWRITE + SPLIT** | 385 claims and the densest page in the repo; the flag/placeholder reference tables deserve their own page. | `USAGE-C003`, `C010`, `C127`, `C200`, `C213`, `C320`, `C337`, `C084`, `C169`, `C209`, `C367` |
| `TROUBLESHOOTING.md` | **REWRITE** | Genuinely good procedure content (211/214 CONFIRMED); only the locale list and two anchors are wrong. | `TROUBLE-C087`, `C182` |
| `SECURITY.md` | **KEEP** | **The best page in the set** — 306/322 CONFIRMED, zero P0/P1. Minor softenings only. | `SECURITY-C042`, `C120`, `C243`, `C288`, `C318` |
| `ACKNOWLEDGEMENTS.md` | **KEEP** | 59 claims, all essentially CONFIRMED. Regenerate from `Cargo.toml`/`package.json` at release time. | — |
| `CONTRIBUTING.md` | **REWRITE** | One stale `lib.rs` citation; otherwise the command/gate set holds. | `CONTRIB-C063` |
| `ARCHITECTURE.md` | **KEEP** | 14 inbound links — the most-referenced page in the repo. Its anchor-redirection table is load-bearing. | `ARCH-C*` all CONFIRMED |
| `AGENTS.md` | **REWRITE** | 400 claims, 11 DRIFT, and it contradicts `SECURITY.md` on token logging. | `AGENTS-C158` (§2.1), `C085`, `C093`, `C096`, `C107`, `C162`, `C253`, `C332`, `C333`, `C359` |
| `CLAUDE.md` | **KEEP** | A working 17-line redirect stub with no dangling references. Do not touch. | — |
| `docs/README.md` | **REWRITE** | The index needs rows for the new operator page, and its anchor claim overstates `link-audit.py`. | `DOCSREAD` + `LINKAUDIT-C001/C006/C016` |
| `docs/PLATFORMS.md` | **KEEP + FIX** | Accurate matrix; the Linux update-payload claim is superseded by `install_method_for`. | coverage-matrix row 60 |
| `docs/RELEASING.md` | **REWRITE** | 179/190 CONFIRMED and the version-bump list is *more* complete than `AGENTS.md`'s. One P1. | `REL-C073`, `C068`, `C145`, `C188` |
| `docs/STATE-OF-FEATURES.md` | **REWRITE** | 429 claims and 42 DRIFT. The "verified" column is the audit's largest single residual risk. | `STATE-C*` (provisional) |
| `docs/architecture/polling.md` | **REWRITE** | All 171 claims confirmed on substance, but nearly every `path:line` citation names a deleted file. | `POLLING-C004`, `C010`, `C025`, `C074`, `C081`, `C112`, `C119`, `C128`, `C129`, `C132`, `C135`, `C143`, `C147`, `C168` |
| `docs/architecture/auth-and-tokens.md` | **REWRITE** | 19 DRIFT of 137; the Linux desktop-file path is simply wrong, and one registration claim is incomplete. | `AUTH-C079`, `C097`, `C099`, `C116` |
| `docs/architecture/storage-and-config.md` | **REWRITE** | Asserts a `config-changed` live push that has zero call sites. | `STORAGE-C031`, `C003`, `C004`, `C052`, `C053`, `C081`, `C087` |
| `docs/architecture/tray-and-shell.md` | **REWRITE** | 19 DRIFT of 154; the guarded-command count and the dedup-key inputs are both wrong. | `TRAY-C006`, `C014`, `C016`, `C017`, `C023`, `C037`, `C039`, `C043`, `C044`, `C071`, `C100`, `C151` |
| `docs/architecture/overview.md` | **REWRITE (light)** | Diagram labels lag the module split; the CI job names are correct. | `OVW-C058`, `C059` |
| `docs/architecture/frontend.md` | **REWRITE** | The directory tree is the most stale artifact in the repo. | `FRONT-C013`, `C192`, `C194`, `C206`, `C208`, `C209` |
| `docs/link-audit.py` | **KEEP + FIX 3** | Its slugger is GitHub-compatible; three real bugs, one of which is a false-negative on explicit anchors. | `LINKAUDIT-C001`, `C006`, `C016`, `C017` |
| **NEW** `docs/HEADLESS.md` | **CREATE** | The changelog promised `docs/HEADLESS.md` + `docs/API.md`; neither exists. One operator page, not two. | §2.4 of the master report |
| — | **DO NOT CREATE** `docs/API.md` | Fold the HTTP route table into `docs/HEADLESS.md` § routes. Two thin pages are worse than one. | `CHANGELOG.md:22` |

### The one split

`USAGE.md` is 362 lines carrying day-to-day guidance *and* three dense reference
tables (CLI flags, status-format placeholders, config keys with clamps). Split:

- **`USAGE.md`** — tray, dashboard, settings, snooze, rules, logs, diagnostics,
  updates. Narrative, task-shaped.
- **`docs/REFERENCE.md`** (new) — the CLI flag table, the status-format
  placeholder table, and the config-key/default/clamp table. Lookup-shaped.

This is the only structural split recommended, and it is justified by volume
(385 claims, 15 DRIFT) rather than by taste.

---

## 2. Target information architecture

The reader journey the current tree already half-implements, made explicit:

```
ENTER → README.md                     (what it is, downloads, front door)
  │
  ├─ 1. INSTALL ────────────── SETUP.md ── PLATFORMS.md
  │                              (register, connect,      (OS/arch
  │                               upgrade)                 matrix)
  │
  ├─ 2. USE ───────────────── USAGE.md ── REFERENCE.md
  │                              (day to day)    (CLI, placeholders,
  │                                               config keys)
  │
  ├─ 3. WHEN IT BREAKS ────── TROUBLESHOOTING.md ── SECURITY.md
  │                              (symptoms)        (posture, disclosure)
  │
  ├─ 4. CONTRIBUTE ────────── CONTRIBUTING.md ── ARCHITECTURE.md ── docs/architecture/*
  │                              (dev setup)        (index)          (six deep dives)
  │                              ── AGENTS.md (agents only)
  │
  ├─ 5. RELEASE ───────────── docs/RELEASING.md ── STATE-OF-FEATURES.md ── CHANGELOG.md
  │
  └─ 6. OPERATE HEADLESS ──── docs/HEADLESS.md   ← NEW
                                 (--serve, --daemon, packaging units)
```

**What moves where:**

| Content | From | To | Why |
| --- | --- | --- | --- |
| CLI flag table | `USAGE.md:284-321` | `REFERENCE.md` § flags | It is a lookup, not a narrative |
| Status-format placeholder table | `USAGE.md:74-85` | `REFERENCE.md` § placeholders | Already duplicated in `AGENTS.md` §9 and `polling.md`; three homes is two too many |
| Config-key / default / clamp table | `USAGE.md` § settings rows | `REFERENCE.md` § config keys | The single largest factual-drift surface (40 of 148 DRIFT) |
| `--serve` routes, port, token scheme | *(absent)* | `HEADLESS.md` | Changelog-deferred; blocks operators |
| `--daemon` SIGTERM contract, three `packaging/` units | *(absent)* | `HEADLESS.md` | Same |
| The 20 `MISSING` v5 surfaces | *(absent)* | `polling.md`, `USAGE.md`, `HEADLESS.md`, `TROUBLESHOOTING.md` | Per the suggested-home column, master report §2.4 |
| `docs/API.md` content | *(absent)* | `HEADLESS.md` § routes | Avoids a second thin page |

**Deliberately *not* moved:** `SECURITY.md` stays a root-level page (3 inbound
links, and its claim/source table is accurate). `AGENTS.md` stays at the root
(`CLAUDE.md` points at it by relative path). `ARCHITECTURE.md` stays the index
(14 inbound links).

---

## 3. Migration plan

### 3.1 Blast radius

- **21 distinct cross-document `file.md#anchor` links** live across the tracked
  tree (excluding `archive/`). Every one is a potential break.
- **Inbound link counts** (measured): `ARCHITECTURE.md` 14 · `USAGE.md` 13 ·
  `CONTRIBUTING.md` 12 · `SETUP.md` 10 · `docs/STATE-OF-FEATURES.md` 10 ·
  `README.md` 8 · `docs/README.md` 8 · `docs/architecture/frontend.md` 7 ·
  `overview.md` 6 · `storage-and-config.md` 6 · `docs/RELEASING.md` 5 ·
  `TROUBLESHOOTING.md` 5 · `SECURITY.md` 5 · `polling.md` 4 ·
  `docs/PLATFORMS.md` 3 · `auth-and-tokens.md` 3 · `tray-and-shell.md` 3 ·
  `ACKNOWLEDGEMENTS.md` 2 · `AGENTS.md` 1 · `CLAUDE.md` 0.

**Rule: any page with ≥5 inbound links does not get renamed.** That protects
`ARCHITECTURE.md`, `USAGE.md`, `CONTRIBUTING.md`, `SETUP.md`,
`docs/STATE-OF-FEATURES.md`, `README.md`, `docs/README.md`, and the four
architecture pages with ≥6.

### 3.2 Anchors that must be preserved verbatim

Grep each before touching its heading. These are load-bearing for *external*
links and for the repo's own redirect table:

| Anchor | Owner | Referenced from |
| --- | --- | --- |
| `#status-format` | `USAGE.md` | `SETUP.md` ×2, `docs/architecture/*` |
| `#what-gets-installed` | `SETUP.md` | `TROUBLESHOOTING.md` |
| `#linux-keyring` | `SETUP.md` | `docs/PLATFORMS.md`, `docs/architecture/*` |
| `#the-app-came-up-with-default-settings` | `TROUBLESHOOTING.md` | ×2 |
| `#status-rules` | `USAGE.md` | ×2 |
| `#command-line-flags` | `USAGE.md` | `docs/` |
| `#the-system-tray`, `#notifications`, `#global-shortcuts`, `#backup`, `#appearance` | `USAGE.md` | `docs/architecture/frontend.md` |
| `#directory-structure` | `docs/architecture/frontend.md` | ×3 (incl. `CONTRIBUTING.md`) |
| `#profanity-filter` | `docs/architecture/polling.md` | `TROUBLESHOOTING.md` |
| `#release-smoke-run-once-against-the-published-release` | `docs/STATE-OF-FEATURES.md` | `docs/RELEASING.md` |
| the 7 rows of `#overview`, `#polling-loop`, `#authentication-flows`, `#config-integrity-v46`, `#multi-window-detach-v40`, `#local-diagnostics-page-v40`, `#event-bus` … | `ARCHITECTURE.md:28-35` redirect table | external inbound |

`ARCHITECTURE.md`'s redirect table maps ~24 historic anchors to their owning
pages. **If any of those headings is renamed, the table must be updated in the
same commit** — and because the table is the repo's own compatibility promise,
renaming a heading is a breaking change, not a cleanup.

### 3.3 `docs/README.md` after the refactor

```markdown
## Start here
| Install the app, register a Spotify Developer app, connect Teams | ../SETUP.md |
| Use the app day to day                                      | ../USAGE.md |
| Look up a CLI flag, a status-format token, a config key      | ./REFERENCE.md |   ← NEW
| Fix something that is not working                            | ../TROUBLESHOOTING.md |
| Check whether your OS and architecture is supported          | ./PLATFORMS.md |
| Run PresenceJam headless (--serve / --daemon / packaging)     | ./HEADLESS.md |    ← NEW
| Know which features are shipped and verified, row by row      | ./STATE-OF-FEATURES.md |
| Understand how it works under the hood                        | ../ARCHITECTURE.md (index) |
| Contribute code — dev setup, conventions, PR process          | ../CONTRIBUTING.md |
| Cut a release                                                | ./RELEASING.md |
| Review the privacy, token-storage and network posture         | ../SECURITY.md |
| See what changed in which version                            | ../CHANGELOG.md |
| Check which open-source projects this app depends on          | ../ACKNOWLEDGEMENTS.md |
```

### 3.4 Order of operations (content before structure)

1. **Mechanical path sweep.** Fix the ≥60 `path:line` citations naming deleted
   files. Pure find-replace, no judgement, no content change. One commit per
   module family (`poll_once.rs` → `polling/`, `lib.rs` → `app.rs`/`state.rs`/
   `cli.rs`/`deep_link.rs`, `config.rs` → `config/`, `tray.rs` → `tray/`).
2. **Fact corrections.** The 13 P1s from master report §2, one `docs:` commit per
   claim ID group. **No structural change in the same commit.**
3. **New content.** The 20 `MISSING` v5 surfaces, one commit per surface.
4. **`docs/HEADLESS.md`** created; `CHANGELOG.md:22`'s deferral closed.
5. **The `USAGE.md` split** into `USAGE.md` + `docs/REFERENCE.md`, *last*, with
   the anchor grep from §3.2 run first and the `ARCHITECTURE.md` redirect table
   updated in the same commit.
6. **Re-run `python3 docs/link-audit.py`** after every step. It must stay at exit 0.

Steps 1–4 are independent and can land in any order. Step 5 depends on 1–4.

---

## 4. Do not touch

An explicit list so a redesign cannot silently drop content the skeptic could not
verify. **None of these may be restructured, renamed, deduplicated or "cleaned
up" without a fresh audit:**

1. **`archive/**` and `docs/archive/**`** — frozen point-in-time reports.
2. **`CLAUDE.md`'s redirect stub** — external tools read it; it must keep
   pointing at `./AGENTS.md` by relative path.
3. **`ARCHITECTURE.md`'s "Where a section went" table** (`:28-35`) — the repo's
   own anchor-compatibility contract, ~24 entries.
4. **`SECURITY.md`'s claim/source table** (~127 rows) — every security-sensitive
   row was verified against source; it is the only machine-checkable
   security documentation in the repo.
5. **`ACKNOWLEDGEMENTS.md`'s provenance footer** — names the deliberately omitted
   platform-FFI crates and `@playwright/test`; regenerating carelessly would
   re-add them.
6. **The `CHANGELOG.md` released sections** — history is immutable.
7. **The status-format placeholder table's semantics** — the single-pass,
   no-re-scan substitution is pinned by a test
   (`format_status_does_not_expand_data_inserted_emoji_token`).
8. **The `[Unreleased]` section's content** — the audit verified 135 bullets
   against the tree; an editorial restructure must not drop any.
9. **`src-tauri/capabilities/*.json` and the CSP** — security controls, not docs.
10. **The bundle id `com.presencejam.app`** — the on-disk anchor for `tokens.json`
    and `app_log_dir()`.
11. **The `docs/architecture/` page set itself** — six pages is the right
    granularity; splitting further would produce pages thinner than the drift
    they carry.

---

## 5. What this proposal deliberately does *not* do

- **It does not merge the six architecture pages.** They average 200 lines and
  each was verified independently; merging would triple the review surface.
- **It does not split `SECURITY.md`.** 515 lines, but it is the most accurate
  page in the repo. Splitting an accurate page to hit a line-count target is
  churn.
- **It does not rename `AGENTS.md` or `CLAUDE.md`.** Both are externally
  referenced by relative path.
- **It does not create `docs/API.md`.** One operator page beats two thin ones.
- **It does not touch the code.** Every fix in master report §2 is a doc fix
  except two: the `cli_help_text()` gap (§2.10) and the metainfo version mismatch
  (§2.6). Both are flagged as code/doc divergences, not silently absorbed into a
  docs commit.
