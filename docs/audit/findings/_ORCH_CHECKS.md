# Orchestrator first-hand verifications and corrections

Phase 2/3 cross-checks run by the orchestrator against the tree directly. These
are independent of the subagent findings and are the seed of the Phase 3
skeptic comparison. Anything here that contradicts a subagent finding is
resolved in the master report's "Overturned / corrected" section.

---

## Confirmed first-hand (orchestrator, 2026-10-09)

### 1. `npm run check` fails on stale ts-rs codegen
`src/lib/types.ts:80` re-exports `TeamsReconnectRequired` from
`./types-generated/TeamsReconnectRequired`. The struct at
`src-tauri/src/events.rs:289-293` **does** carry
`#[ts(export, export_to = "../../src/lib/types-generated/")]`. A scripted
comparison of every `#[ts(export)]` item in `src-tauri/src/` against the 77
files in `src/lib/types-generated/` found exactly one gap. Verdict: stale
codegen, `cargo test --lib` materialises it. Not a doc defect. See
`PHASE0_PREFLIGHT.md`.

### 2. Window is NOT hidden by default
`src-tauri/tauri.conf.json:24` — `"visible": true`. Hidden only when
`cfg.teams.start_minimized` or the autostart `--minimized` flag is present:
`src-tauri/src/app.rs:562-567`. Confirms USAGE-C003 DRIFT.

### 3. Log-level "off" silences the in-app Log Viewer too
`src-tauri/src/config/schema.rs:452-467` — `apply_log_level` maps `"off"` to
`log::LevelFilter::Off` and calls the **global** `log::set_max_level`, which
filters every target including the Webview target that feeds the in-app viewer.
Confirms USAGE-C213 DRIFT.

### 4. CSP string matches AGENTS.md's description exactly
`src-tauri/tauri.conf.json:30` —
`default-src 'self'; img-src 'self' https://i.scdn.co https://mosh-pa.spotify.com;
style-src 'self' 'unsafe-inline'; connect-src 'self' https://api.spotify.com
https://login.microsoftonline.com https://graph.microsoft.com
https://accounts.spotify.com; object-src 'none'; base-uri 'self';
frame-ancestors 'none'; form-action 'none'`. No `frame-src`, so frames fall back
to `default-src 'self'`. AGENTS.md §7 is accurate.

### 5. `AGENTS.md` version-bump rule is self-contradictory
`AGENTS.md:597-599` lists **five** files (`package.json`,
`src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `CHANGELOG.md`,
`docs/STATE-OF-FEATURES.md`) then says "without bumping all **four** sites
together". CI's `version-consistency` (`ci.yml:454-470`) checks only **three**.
A fifth/sixth/seventh site exists in reality — see item 6. **P2 editorial with
P1 consequence** (a maintainer following the rule under-bumps).

### 6. `docs/RELEASING.md` §1 is MORE complete than AGENTS.md says
`docs/RELEASING.md:16-26` names **five files / six literals** plus a seventh
file (`com.presencejam.app.metainfo.xml`) that is "not a literal but is gated
the same way", and even names it as "the gate that silently under-counts if it
is missed". `AGENTS.md` §12's "four sites" rule and the CI's three-site check
are both narrower than the real release gate.

---

## Corrections to subagent findings (orchestrator overrules)

### A. `REL_LINKAUDIT` D2 — "the metainfo is not documented" → **overstated**
The agent reported as a P1 doc defect that `CHANGELOG.md` has no `## [5.0.0]`
section and the metainfo is still 4.7.0 while the manifests read 5.0.0. The
repo-state half is true (`src-tauri/linux/com.presencejam.app.metainfo.xml:70`
— `<release version="4.7.0" date="2026-09-17">`), but the "not documented" half
is **false**: `docs/RELEASING.md:16, :26, :34, :194, :300, :333-334` all name it,
and `release.yml:373-390` is the gate that enforces it.

**Restated as a P1 repo-state finding, not a doc defect:** the tree is committed
mid-bump — all six version literals read `5.0.0` (and `ci.yml`'s three-site gate
passes) while the metainfo's newest `<release>` is `4.7.0`. A `v5.0.0` tag cut
fails `release.yml:387-390` on the Linux leg. The doc already prescribes the fix
(prepend, do not edit — `RELEASING.md:26`). Blast radius: blocks the v5 release.

### B. `REL_LINKAUDIT` D1 — Windows-leg precedence contradiction → **CONFIRMED**
`docs/RELEASING.md:130-133`: "Re-expand the Windows test leg **when the runner
image links the binary cleanly**".
`.github/workflows/ci.yml:82-83`: "Re-expand to both legs **once the
dev-dependency hazard is removed, not when an image changes**."
The workflow is authoritative; the doc tells a maintainer to do the wrong thing.
**P1.** `AGENTS.md` §3/§11 carries the correct long-form explanation.

### C. Coverage matrix line 168 — `docs/COMMANDS.md` → **agent self-corrected, confirmed absent**
The agent cited `docs/COMMANDS.md` as documentation for the 54-command guard
matrix, then struck it. Verified: `ls docs/COMMANDS.md` → no such file;
`git ls-files docs/` lists only PLATFORMS/README/RELEASING/STATE-OF-FEATURES,
the six architecture pages, three archive docs, link-audit.py and three
screenshots. The strike was correct. Consequence: the guard matrix is documented
**only** in `docs/STATE-OF-FEATURES.md:35` and in the test itself.

### D. Coverage matrix line 184 — the #775 test-rename claim → **CONFIRMED DEFECT**
`CHANGELOG.md:150` claims "The former `tests/hygiene.test.ts` is now
`tests/theme-density.test.ts`; no current config, script, workflow, or
non-historical documentation reference names the old filename." Both halves are
false:
- `tests/hygiene.test.ts` (16,376 bytes, mtime 2026-10-08) still exists, header
  `#904 — the design-token hygiene guard`.
- `tests/theme-density.test.ts` (13,242 bytes) also exists, header
  `#680 — follow-system appearance + compact density`.
- `CHANGELOG.md:42, :43, :66` — three bullets in the **same Unreleased
  section** — cite `tests/hygiene.test.ts`.
This was a new-file split, not a rename. **P2 changelog inaccuracy.**

### E. Coverage matrix line 138 — Tauri pin citation line number → **CONFIRMED DEFECT**
`CHANGELOG.md:115` (#855) says `docs/STATE-OF-FEATURES.md` quotes `tauri ~2.11`
"at `src-tauri/Cargo.toml:45`". `src-tauri/Cargo.toml:55` is
`tauri = { version = "~2.11", features = ["tray-icon"] }`; line 45 is inside the
`[target."cfg(target_os = \"windows\")".dependencies]` block. **P2 stale
file:line citation**, and one the changelog asserts as fixed.

---

## Repo hygiene observed (raised, not actioned)

- `docs/._.DS_Store` and `docs/audit/._.DS_Store` — untracked macOS AppleDouble
  sidecars. `.gitignore:21` ignores `.DS_Store` but not `._.DS_Store`.
- `docs/__pycache__/link-audit.cpython-314.pyc` — untracked bytecode from
  running the gate.
- `docs/audit/` — untracked and unignored; `REL_LINKAUDIT` recommends gitignoring
  it. Phase 4 instead moves the working copies out of the tree, which fixes
  `link-audit.py` (138 self-inflicted breaks) without touching `.gitignore`.
- `pj-worktrees/`, root `SOUL.md` / `USER.md` / `IDENTITY.md` — **do not exist**.
  Brief hygiene seed #8 is refuted.
- 22 tracked `.md` files outside `archive/`; both indexes list all six
  architecture pages including `frontend.md`, so there is **no orphan page**.
  The brief's file list omitted `docs/architecture/frontend.md` — a gap in the
  brief, not in the repo. It was extracted and verified anyway.
