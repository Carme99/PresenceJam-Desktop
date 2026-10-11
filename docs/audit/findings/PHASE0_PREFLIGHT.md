# Phase 0 — Preflight record

Captured by the orchestrator before any subagent ran. All values re-verified or
independently re-derived below.

| Item | Value |
| --- | --- |
| Commit SHA | `09341ecaad732e78454a2c65b383f0dfd1541d5a` |
| Working tree | clean (tracked files); untracked paths are audit artifacts only |
| Latest tag | `v4.7.0` (then v4.6.0, v4.5.2, v4.5.1, v4.5.0) |
| `package.json` | `5.0.0` |
| `src-tauri/Cargo.toml` | `5.0.0` |
| `src-tauri/tauri.conf.json` | `5.0.0` |
| `docs/link-audit.py` | **exit 0** — `markdown files scanned : 93` / `relative links checked : 157 (47 with #anchors)` / `broken : 0` |
| `cargo check --all-targets` | **exit 0** |
| `npm run check` | **exit 1** — see below |

## `npm run check` failure

```
Error: Cannot find module './types-generated/TeamsReconnectRequired' or its
corresponding type declarations.
Warn: noninteractive element cannot have nonnegative tabIndex value
svelte-check found 1 error and 1 warning in 2 files
```

First-hand diagnosis (orchestrator, not delegated):

- `src/lib/types.ts:80` re-exports `TeamsReconnectRequired` from
  `./types-generated/TeamsReconnectRequired`.
- The source struct at `src-tauri/src/events.rs:289-293` **does** carry
  `#[ts(export, export_to = "../../src/lib/types-generated/")]`.
- `src/lib/types-generated/` is gitignored and holds 77 files; a scripted
  comparison of every `#[ts(export)]` struct/enum in `src-tauri/src/` against the
  generated directory found **exactly one** gap: `TeamsReconnectRequired`.

Verdict: this is a **stale-codegen artifact**, not a doc defect and not a source
defect. `cargo test --lib` materialises the missing file (AGENTS.md §1 documents
the ordering requirement). It is recorded here because it means the §7 gate set
cannot pass on this tree as it stands, and because the audit brief listed
`npm run check` as a gate that "must exit 0".

**Audit-brief correction:** the §7 gate list is not satisfied on this tree. The
report below states the gate outcome honestly rather than claiming a pass.

## Audit self-inflicted artifact

`docs/audit/claims/` and `docs/audit/findings/` (the audit's own working copies)
themselves break `python3 docs/link-audit.py`:

```
markdown files scanned : 120
relative links checked : 306 (82 with #anchors)
broken                 : 138     <- all inside docs/audit/claims/
```

Every one of the 138 breaks is a root-relative link inside a *copy* of a doc,
resolved against `docs/audit/claims/` instead of the repo root. The 20 in-scope
docs are link-clean in their real locations. Phase 4 resolves this by moving the
working copies out of the tree and leaving only the two deliverables under
`docs/audit/`, re-verified with `link-audit.py`.

## Drift seeds resolved at Phase 0

| # | Hypothesis from the brief | Verdict | First-hand evidence |
| --- | --- | --- | --- |
| 3 | `[Unreleased]` contains **two `### Added` headings** | **REFUTED** | The duplicate is `### Refactor`, at `CHANGELOG.md:48` and `CHANGELOG.md:144`. Full heading order: Added(12) · Changed(28) · Refactor(48) · Fixed(52) · Security(135) · Refactor(144) · Test(149) · Docs(157). 135 bullets total. |
| 4 | Three version sites read `4.7.0` | **REFUTED** | All three read `5.0.0`. CI's `version-consistency` job (`ci.yml:454-470`) checks exactly three sites, and passes. The real defect is AGENTS.md:597-599 listing **five** sites while asserting "all four sites together" — a self-contradiction, and CI checks only three. |
| 8 | `pj-worktrees/v5-pipeline-fix/` duplicates the tree; root `SOUL.md`/`USER.md`/`IDENTITY.md` leaked | **REFUTED (both halves)** | `pj-worktrees/` does not exist; `SOUL.md`, `USER.md`, `IDENTITY.md` do not exist at the repo root. |
| 5 | `docs/link-audit.py` is already in CI (`docs-links`) | **CONFIRMED** | `ci.yml` `docs-links` job runs `python3 docs/link-audit.py` on the whole tree, `ubuntu-latest`, 5-minute timeout, no path filter. |
| 7 | `CLAUDE.md` is a working redirect stub | **CONFIRMED** | 17 lines, points at `./AGENTS.md` with a valid inbound set (AGENTS.md:5, 6, 37, 79, 219, 680, 701; docs/STATE-OF-FEATURES.md:114). No dangling references. |
| — | `AGENTS.md` is "711 lines" per the brief | **DRIFT (brief)** | It is 720 lines. |

## Repo hygiene observed (report only, not actioned)

- `docs/._.DS_Store` and `docs/audit/._.DS_Store` — untracked macOS AppleDouble
  sidecars. `.gitignore:21` ignores `.DS_Store` but **not** `._.DS_Store`, so
  these are untracked and unignored. They are not at the repo root, so the
  `no-vendored-binaries` job (which matches only `^\?\? [^/]*/?$`) does not fail.
- `docs/__pycache__/link-audit.cpython-314.pyc` — untracked bytecode produced by
  running the gate. Not a CI failure (not root-level).
- `archive/` and `docs/archive/` exist and are correctly excluded from this audit.

## Information-architecture input (orchestrator, Phase 4 prep)

22 tracked `.md` files outside `archive/`. Both indexes — `ARCHITECTURE.md:9-16`
and `docs/README.md:27-34` — list all six architecture pages including
`docs/architecture/frontend.md`, so there is **no orphan page**. The audit brief's
file list omitted `frontend.md`; that is a gap in the brief, not in the repo.
`docs/architecture/frontend.md` (288 lines, 251 claims) was extracted and
verified anyway and is flagged `scope_gap: true` in the findings.
