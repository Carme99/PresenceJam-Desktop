# Releasing PresenceJam

How a release is cut, what enforces the version numbers, and how the tag becomes
a published, auto-updatable build. Everything here is derived from
[`.github/workflows/ci.yml`](../.github/workflows/ci.yml) and
[`.github/workflows/release.yml`](../.github/workflows/release.yml) — if this
page and a workflow disagree, the workflow wins; fix the page.

Index of the docs: [`README.md`](./README.md). Post-release verification lives in
[`STATE-OF-FEATURES.md`](./STATE-OF-FEATURES.md).

---

## 1. Version-bearing files

Five files, **six literals**:

| # | File | Literal | Notes |
| --- | --- | --- | --- |
| 1 | [`package.json`](../package.json) | top-level `"version"` | npm package version |
| 2 | [`package-lock.json`](../package-lock.json) | top-level `"version"` | the lockfile's own version |
| 3 | [`package-lock.json`](../package-lock.json) | `packages[""].version` | the root package entry inside `packages` |
| 4 | [`src-tauri/tauri.conf.json`](../src-tauri/tauri.conf.json) | `"version"` | **the version the built binary self-reports** — the one the updater compares against |
| 5 | [`src-tauri/Cargo.toml`](../src-tauri/Cargo.toml) | `version = "…"` under `[package]` | crate `presence-jam` |
| 6 | [`src-tauri/Cargo.lock`](../src-tauri/Cargo.lock) | `version = "…"` in the `[[package]] name = "presence-jam"` block | the workspace member's locked version |

Find them all (replace `4.6.0` with the version you are leaving):

```bash
grep -n '"version"' package.json package-lock.json src-tauri/tauri.conf.json
grep -n '^version' src-tauri/Cargo.toml
grep -n -A2 '^name = "presence-jam"' src-tauri/Cargo.lock
```

### The `package-lock.json` trap

`package-lock.json` also contains **`"version": "4.6.0"` for the `cssstyle`
dependency** (`node_modules/cssstyle`, resolved from
`https://registry.npmjs.org/cssstyle/-/cssstyle-4.6.0.tgz`). It is an ordinary
dependency version that only happens to match the app's for this release — never
touch it. Confirm you are editing the project's own literals by checking that
your grep hits lines 3 and 9 (the top-level and `packages[""]` entries), not the
`node_modules/cssstyle` block:

```bash
grep -n '4\.6\.0' package-lock.json    # lines 3 and 9 are ours; the rest are deps
```

### Why the two lockfiles are manual

Nothing checks them. `ci.yml`'s `version-consistency` job (`ci.yml`, job
`version-consistency`, ~lines 311-339) reads `jq -r .version` from
`src-tauri/tauri.conf.json`, `package.json` and `sed`'s `src-tauri/Cargo.toml` —
and compares those three with each other. The tag-time check in `release.yml`
("Verify version consistency", ~lines 78-100) compares the **same three** against
the resolved tag. Neither one opens `package-lock.json` or `Cargo.lock`, so a
forgotten lockfile bump passes CI and only surfaces as a `cargo`/`npm`
discrepancy later. Bump all six literals, every time.

## 2. CHANGELOG rules

`CHANGELOG.md` is [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) with
`Added` / `Changed` / `Fixed` / `Security` sections. During development, entries
go under `## [Unreleased]`.

At release time, **in one commit**:

1. rename `## [Unreleased]` → `## [X.Y.Z] - YYYY-MM-DD`,
2. add a fresh, empty `## [Unreleased]` section on top for the next cycle (the
   4.6.0 cut left `Added` / `Changed` / … headings behind it, as the file does
   today), and
3. add the new section's link definition:

   ```markdown
   [X.Y.Z]: https://github.com/Carme99/PresenceJam-Desktop/compare/vPREVIOUS...vX.Y.Z
   ```

4. re-base the `[Unreleased]` definition on the tag you just created, or the new
   empty section links to the wrong diff range:

   ```markdown
   [Unreleased]: https://github.com/Carme99/PresenceJam-Desktop/compare/vX.Y.Z...HEAD
   ```

The `changelog-links` job in `ci.yml` (~lines 282-299) iterates every
`^## [X]` header and fails on the first one whose `[X]:` definition is missing
(precedent: issues #437/#438). **This is the trap that shipped a broken run:**
on `release/4.6` the section was renamed without adding the link definition in
the same commit and CI run **#423** failed on exactly that job. Rename and
define together, or not at all.

## 3. CI gates a release PR must pass

`ci.yml` triggers on pull requests into `main` and pushes to `main`. Failable
jobs (the Name column is the check context shown on the PR's checks list):

| Job | Name | What it does |
| --- | --- | --- |
| `rust-platform-check` | Rust check (macOS + Windows) | `cargo check` on macOS + Windows (platform-gated code compiles) |
| `frontend` | Frontend (npm build + ts-rs codegen) | `npm run build`, `npm run check`, frontend tests |
| `rust` | Rust (cargo check) | fmt, `cargo check`, `cargo test` on Linux |
| `rust-clippy` | Rust clippy | `cargo clippy -- -D warnings` |
| `changelog-links` | CHANGELOG link definitions | every `## [X]` header needs a `[X]:` definition |
| `version-consistency` | Version consistency | `tauri.conf.json` vs `package.json` vs `Cargo.toml` |
| `secret-scan` | Secret scan (gitleaks) | gitleaks over the history |
| `dep-audit` | Dependency audit (cargo + npm) | **advisory only** — `continue-on-error: true` |

## 4. The tag → publish chain (`release.yml`)

Trigger: a push of a `v*` tag, **or** `workflow_dispatch` with the `tag` input
(a manual re-cut of an existing tag; required when dispatching from a non-tag
ref). Concurrency keys on `release-<event>-<ref>`, so a re-cut does not queue
behind the tag-push run.

1. **`resolve-tag`** (~lines 36-100) — resolves the tag (`inputs.tag` for a
   dispatch, else `GITHUB_REF_NAME`), rejects anything that is not `v*`,
   verifies the tag exists with `git ls-remote --exit-code --tags`, checks the
   repository out **at that tag**, then runs the tag/version check: `EXPECTED` =
   the tag without its leading `v`, compared against `tauri.conf.json`,
   `package.json` and `Cargo.toml`. Drift fails the run here — before any
   compile — because a mismatch means the shipped binary self-reports a
   different version than `latest.json` advertises, which makes the updater
   re-offer the same update forever (issue #605). The resolved `tag` output is
   consumed by every downstream job instead of `github.ref_name`.
2. **`verify`** (`needs: resolve-tag`, ~lines 111-178) — checks out the same tag
   and reruns the `ci.yml` gate set there (fmt, clippy, `cargo test`,
   `npm run check`, frontend tests, with Linux system deps). `ci.yml` only runs
   on PRs and `main`, so without this job the exact commit that produces
   user-facing binaries would never be tested — worst case on a re-cut of a
   commit that never saw CI.
3. **`build`** (`needs: [resolve-tag, verify]`, ~lines 180-400) — three-OS
   matrix:

   | OS | Target | Artifact (upload) | Packaged files |
   | --- | --- | --- | --- |
   | `macos-latest` | `aarch64-apple-darwin` | `PresenceJam-<tag>-macos.dmg` | `PresenceJam-macos.dmg`, `PresenceJam-<tag>.app.tar.gz`, `PresenceJam-<tag>.app.tar.gz.sig` |
   | `windows-latest` | default | `PresenceJam-<tag>.msi` | `PresenceJam-<tag>.msi`, `PresenceJam-<tag>.msi.sig` |
   | `ubuntu-latest` | default | `PresenceJam-<tag>-linux-amd64` | `PresenceJam-linux-amd64.deb`, `PresenceJam-linux-amd64.AppImage`, `PresenceJam-<tag>.AppImage.sig` |

   The macOS leg builds **aarch64 only** (Intel Macs never receive updates).
   Uploads fail on missing files (`if-no-files-found: error`), and each packaged
   artifact also gets a SLSA attestation (`actions/attest-build-provenance`,
   `subject-path: matrix.bundle_path`) — supplementary to, not a replacement
   for, the minisign `.sig` files the updater verifies.
4. **`release`** (`needs: [resolve-tag, build]`, ~lines 401-516) — downloads all
   artifacts (`digest-mismatch: error`), writes `SHA256SUMS.txt` (one
   `"<sha256>  <filename>"` line per file; unsigned, and deliberately not covered
   by the build attestation), publishes the Release with
   `ncipollo/release-action` **tagged explicitly with the resolved tag**
   (`allowUpdates: true`, `artifacts: artifacts/**/*`), then generates
   `latest.json`:

   ```json
   {
     "version": "<tag without v>",
     "pub_date": "<UTC RFC3339>",
     "platforms": {
       "darwin-aarch64": { "url": "…/PresenceJam-<tag>.app.tar.gz", "signature": "<.sig contents>" },
       "windows-x86_64": { "url": "…/PresenceJam-<tag>.msi", "signature": "<.sig contents>" },
       "linux-x86_64":   { "url": "…/PresenceJam-linux-amd64.AppImage", "signature": "<.sig contents>" }
     }
   }
   ```

   Signature values are the **contents** of the `.sig` files (minisign output),
   not paths; a missing or empty `.sig` fails the job. `latest.json` is uploaded
   with `gh release upload --clobber`, then "Verify latest.json assets in
   release" lists the published assets and requires all three basenames to be
   present — the updater has no GitHub auto-discovery and treats a 404 on a
   platform URL as "no update" **silently**, so a missing asset would strand
   every client (the v3.1.0 → v3.2.0 Windows incident, see
   [`archive/windows-update-chain-v3.2.md`](./archive/windows-update-chain-v3.2.md)).
5. **`homebrew`** (`needs: [resolve-tag, release]`) — downloads the macOS DMG
   artifact, computes its SHA-256, and updates `presence-jam.rb` in
   `carme99/homebrew-tap` (version/url/sha256), creating the formula if absent
   and no-oping if the version already matches. Requires `HOMEBREW_TAP_TOKEN`
   with `contents:write` on the tap.
6. **`winget`** (`needs: [resolve-tag, release]`) — `vedantmgoyal2009/winget-releaser`
   for `PresenceJam.PresenceJam`, submitting through the fork
   `Carme99/winget-pkgs` (`fork-user`). Requires `WINGET_TOKEN`: a **classic**
   PAT with `public_repo` **and** `workflow` scopes (fine-grained is
   unsupported; the fork must be synced with upstream before the manifest branch
   is created). Rotate it on a ~90-day cadence.

## 5. Cutting a release — checklist

1. `main` is green, and every slice PR for the release is merged.
2. Bump all six literals in §1 (branch `release/X.Y` or a PR into `main`).
3. Update `CHANGELOG.md` per §2 — rename, add the new link definition and re-base
   `[Unreleased]`, all in the same commit. If the release changes shipped behaviour, update
   [`STATE-OF-FEATURES.md`](./STATE-OF-FEATURES.md) too (including its version
   header).
4. Open the release PR and wait for `version-consistency`, `changelog-links`,
   `rust`, `rust-clippy`, `rust-platform-check`, `frontend` and `secret-scan`.
5. Merge, then tag the **merge commit on `main`** (precedent: `v4.6.0` →
   `4e9c6f1d`):

   ```bash
   git checkout main && git pull --ff-only
   git tag -a vX.Y.Z -m "PresenceJam X.Y.Z — <codename>" <merge-sha>
   git push origin vX.Y.Z
   ```

6. Watch the run: `gh run watch`.
7. Verify the published release:

   ```bash
   gh release view vX.Y.Z --json tagName,name,assets
   curl -sI https://github.com/Carme99/PresenceJam-Desktop/releases/latest/download/latest.json
   ```

   `latest.json` must report `X.Y.Z`, carry the three platform keys, and each
   referenced asset URL must return 200; `SHA256SUMS.txt` must be present.
8. Run the post-release smoke checklist in
   [`STATE-OF-FEATURES.md`](./STATE-OF-FEATURES.md#release-smoke-run-once-against-the-published-release): (1) install
   the previously published build, stage the update, quit, relaunch; (2) sign in,
   quit, leave the app closed **> 1 h**, relaunch and confirm the session
   refreshes. Then flip the ⚠ rows with the recorded results.

### Re-cutting an existing tag

Use the manual path instead of force-moving the tag:

```bash
gh workflow run release.yml -f tag=vX.Y.Z
```

`resolve-tag` validates the tag exists and checks it out, `build` re-produces
the artifacts, and the `release` job re-uploads them (`--clobber`, release action
`allowUpdates: true`). This is the recovery path for a failed or partial publish
— it is not a way to move a release onto different code. If the tag itself is
wrong, delete and re-push it deliberately (the tag decides the version written
into `latest.json`, and `resolve-tag` verifies it against the three version
files before anything builds).
