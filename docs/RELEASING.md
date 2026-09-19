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

Find them all (replace `<old-version>` with the version you are leaving):

```bash
grep -n '"version"' package.json package-lock.json src-tauri/tauri.conf.json
grep -n '^version' src-tauri/Cargo.toml
grep -n -A2 '^name = "presence-jam"' src-tauri/Cargo.lock
```

### The `package-lock.json` trap

`package-lock.json` also carries a `"version"` for every dependency entry, and a
dependency's version is free to coincide with the app's — `cssstyle` did exactly
that at an earlier cut. Only **two** literals in that file are ours: the top-level
`version` and `packages[""].version`. Never touch a dependency block. Check the
pair against `package.json` rather than grepping for a version string, which
cannot tell the two apart:

```bash
jq -r '.version, .packages[""].version' package-lock.json   # both must be the new version
jq -r .version package.json                                 # and must match this
```

### How the six literals are gated

Two jobs read **all six** — the three manifests and both lockfiles — and fail
on the first disagreement:

- `ci.yml`, job `version-consistency` — the PR-time gate. `jq -r .version`
  from `src-tauri/tauri.conf.json`, `package.json` and `package-lock.json`,
  `jq -r '.packages[""].version'` for the lockfile's own root entry, `sed` for
  `src-tauri/Cargo.toml`, and an `awk` over the `presence-jam` block of
  `src-tauri/Cargo.lock`; every one of the six is compared with
  `tauri.conf.json`.
- `release.yml`, step "Verify version consistency" in the `resolve-tag`
  job (~lines 86-125) — the same six compared against the tag, before the
  build matrix starts, so a tag cannot publish a lock that disagrees with the
  binary it ships.

The cargo steps also run with `--locked` (fmt takes no such flag), so a
`Cargo.lock` that has drifted from `Cargo.toml` fails the run instead of being
quietly rewritten on the runner. A legitimate dependency update therefore lands
in the PR that changes the lock, never in a release build.

Bump all six literals, every time.

## 2. CHANGELOG rules

`CHANGELOG.md` is [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) with
`Added` / `Changed` / `Fixed` / `Security` sections. During development, entries
go under `## [Unreleased]`.

At release time, **in one commit**:

1. rename `## [Unreleased]` → `## [X.Y.Z] - YYYY-MM-DD`,
2. add a fresh, empty `## [Unreleased]` section on top for the next cycle (each
   cut leaves `Added` / `Changed` / … headings behind it, as the file does
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
| `frontend` | Frontend (npm build + ts-rs codegen) | `npm run build`, `npm run check`, frontend tests + coverage ratchet (`npm run test:coverage`) |
| `rust` | Rust (cargo check) | fmt, `cargo check`, `cargo test` on Linux |
| `rust-clippy` | Rust clippy | `cargo clippy -- -D warnings` |
| `changelog-links` | CHANGELOG link definitions | every `## [X]` header needs a `[X]:` definition |
| `version-consistency` | Version consistency | all six version literals in §1 agree, both lockfiles included |
| `secret-scan` | Secret scan (gitleaks) | gitleaks over the history |
| `dep-audit` | Dependency audit (cargo + npm) | **advisory only** — `continue-on-error: true` |

The Rust test suite runs on **all three** platforms, not just Linux:
`rust-platform-check` executes `cargo test --all-targets` on the macOS and
Windows legs straight after its `cargo check` (that job's `timeout-minutes` was
raised from 20 to 45 for the added compile and link work). Compiling
platform-gated code is not running it — `cargo check` alone would pass while a
regression sat inside the macOS `ActivationPolicy` branch in
`commands/config.rs`, the dock-badge path in `tray.rs`, the macOS deep-link
registration in `lib.rs` or the Windows tray and notification paths. The ubuntu
leg keeps `--all-targets`. If a macOS/Windows leg is ever reduced to
`cargo test --lib` to save runner minutes, record that reduced scope here
rather than leaving it implied.

## 4. The tag → publish chain (`release.yml`)

Trigger: a push of a `v*` tag, **or** `workflow_dispatch` with the `tag` input
(a manual re-cut of an existing tag; required when dispatching from a non-tag
ref). Concurrency keys on `release-<event>-<ref>`, so a re-cut does not queue
behind the tag-push run.

1. **`resolve-tag`** (~lines 36-152) — resolves the tag (`inputs.tag` for a
   dispatch, else `GITHUB_REF_NAME`), rejects anything that is not `v*`,
   verifies the tag exists with `git ls-remote --exit-code --tags`, checks the
   repository out **at that tag**, then runs the tag/version check: `EXPECTED` =
   the tag without its leading `v`, compared against all six literals in §1 —
   `tauri.conf.json`, `package.json`, both `package-lock.json` literals,
   `Cargo.toml` and `Cargo.lock`'s `presence-jam` entry. Drift fails the run
   here — before any compile — because a mismatch means the shipped binary
   self-reports a different version than `latest.json` advertises, which makes
   the updater re-offer the same update forever (issue #605). The resolved
   `tag` output is consumed by every downstream job instead of
   `github.ref_name`. The same job then requires the release's documentation to
   exist *before* anything builds (issue #834): `CHANGELOG.md` must carry a
   `## [X.Y.Z]` section and `docs/STATE-OF-FEATURES.md` a
   `# State of Features — vX.Y.Z` header, or the run fails with an `::error::`
   naming the missing file. A pre-release tag (`vX.Y.Z-beta.N`) is gated
   against the section of the version it will become.
2. **`verify`** (`needs: resolve-tag`, ~lines 162-236) — checks out the same tag
   and reruns the `ci.yml` gate set there (fmt, clippy, `cargo test`,
   `npm run check`, `npm run test:coverage` — the coverage ratchet, not bare
   `npm test`, with Linux system deps). `ci.yml` only runs
   on PRs and `main`, so without this job the exact commit that produces
   user-facing binaries would never be tested — worst case on a re-cut of a
   commit that never saw CI.
3. **`build`** (`needs: [resolve-tag, verify]`, ~lines 238-544) — three-OS matrix:

   | OS | Target | Artifact (upload) | Packaged files |
   | --- | --- | --- | --- |
   | `macos-latest` | `aarch64-apple-darwin` | `PresenceJam-<tag>-macos.dmg` | `PresenceJam-macos.dmg`, `PresenceJam-<tag>.app.tar.gz` |
   | `windows-latest` | default | `PresenceJam-<tag>-windows` | `PresenceJam-<tag>-setup.exe`, `PresenceJam-<tag>.msi` |
   | `ubuntu-latest` | default | `PresenceJam-<tag>-linux-amd64` | `PresenceJam-linux-amd64.deb`, `PresenceJam-linux-amd64.rpm`, `PresenceJam-linux-amd64.AppImage` |

   The macOS leg builds **aarch64 only** (Intel Macs never receive updates).
   The Windows leg ships both installers: the per-user NSIS `setup.exe` (what
   `latest.json` points at, no elevation needed) and the per-machine MSI for
   managed machines. Uploads fail on missing files (`if-no-files-found: error`),
   the build job prints every packaged file with its byte count, and each
   packaged artifact also gets a SLSA attestation
   (`actions/attest-build-provenance`, `subject-path: matrix.bundle_path`).

   **Everything this job produces is unsigned, deliberately** (issue #828): the
   three `npx tauri build` steps run with an inline
   `--config '{"bundle":{"createUpdaterArtifacts":false}}'` and no signing
   secrets in their environment, because the compile executes far more than
   this crate — `npm run build` (Vite plus every plugin under `node_modules`)
   and build scripts for the whole dependency tree all inherit that
   environment, and anyone who can read it can sign an update every installed
   client accepts. The `.app.tar.gz` is tar.gz'd by the macOS leg itself for
   the same reason: with updater artifacts disabled, tauri's own updater
   bundler — the thing that used to create it and sign it — no longer runs.

   The Linux leg also runs `appstreamcli validate` over
   `src-tauri/linux/com.presencejam.app.metainfo.xml` before the compile,
   requires that file's newest `<release>` entry to name the version being cut,
   and asserts afterwards that the metainfo is inside both the `.deb` and the
   `.rpm` — a package without it never appears in GNOME Software or KDE
   Discover.

4. **`sign`** (`needs: [resolve-tag, build]`, `environment: release-signing`, ~lines 546-632) —
   the only job holding `TAURI_SIGNING_PRIVATE_KEY` and its password, exported
   to one step (`Sign updater payloads`) rather than the job, so `npm ci` and
   its lifecycle scripts never see them. It downloads the unsigned payloads and
   runs `npx tauri signer sign` over each: the macOS `.app.tar.gz`, the Windows
   `setup.exe` and `.msi`, and the Linux `AppImage`. Signatures land in a flat
   `signatures` artifact (the release job reads them from there) and are
   attested too, which is the provenance the build job used to give them while
   it produced them. Add required reviewers to the `release-signing`
   environment in the repository settings, or a tag push can sign and publish
   without a human — until then the environment exists but gates nothing.
5. **`release`** (`needs: [resolve-tag, build, sign]`, ~lines 634-885) — checks the tag out (it needs
   `CHANGELOG.md`), downloads all artifacts (`digest-mismatch: error`), writes
   `SHA256SUMS.txt` (one `"<sha256>  <filename>"` line per file; unsigned, and
   deliberately not covered by the build attestation), extracts this version's
   `CHANGELOG.md` section into `notes.md` — from the `## [X.Y.Z]` header to the
   next `## [` header, with an empty body failing the job — and publishes the
   Release with `ncipollo/release-action` **tagged explicitly with the resolved
   tag**, passing that file as `bodyFile`. `generateReleaseNotes` is off on
   purpose: the body users read is the curated section, not GitHub's generated
   PR list (`allowUpdates: true` so a re-cut replaces the body,
   `artifacts: artifacts/**/*`). It then generates `latest.json`:

   ```json
   {
     "version": "<tag without v>",
     "pub_date": "<UTC RFC3339>",
     "platforms": {
       "darwin-aarch64": { "url": "…/PresenceJam-<tag>.app.tar.gz", "signature": "<.sig contents>" },
       "windows-x86_64": { "url": "…/PresenceJam-<tag>-setup.exe", "signature": "<.sig contents>" },
       "linux-x86_64":   { "url": "…/PresenceJam-linux-amd64.AppImage", "signature": "<.sig contents>" }
     }
   }
   ```

   Signature values are the **contents** of the `.sig` files (minisign output),
   not paths, and they come from the `sign` job's `signatures` artifact; a
   missing or empty `.sig` fails the job. `latest.json` is uploaded
   with `gh release upload --clobber`, then "Verify latest.json assets in
   release" requires every URL the manifest advertises to be a published
   asset, pins the `.deb`/`.rpm`/`AppImage`/`setup.exe`/`.app.tar.gz` payload
   names on top of that, and (on a beta tag) resolves the rolling beta
   manifest — the updater has no GitHub auto-discovery and treats a 404 on a
   platform URL as "no update" **silently**, so a missing asset would strand
   every client (the v3.1.0 → v3.2.0 Windows incident, see
   [`archive/windows-update-chain-v3.2.md`](./archive/windows-update-chain-v3.2.md)).
6. **`homebrew`** (`needs: [resolve-tag, release]`, ~lines 887-1009) — downloads the macOS DMG
   artifact, computes its SHA-256, and updates the **cask**
   `Casks/presence-jam.rb` in `carme99/homebrew-tap` (version/url/sha256),
   creating it if absent, no-oping if it already matches, and deleting the
   stale root-level `presence-jam.rb` formula left by the formula → cask
   migration (`brew uninstall presence-jam` first, then
   `brew install --cask carme99/tap/presence-jam`). Requires
   `HOMEBREW_TAP_TOKEN` with `contents:write` on the tap.
7. **`winget`** (`needs: [resolve-tag, release]`, ~lines 1011-1073) — `vedantmgoyal2009/winget-releaser`
   for `PresenceJam.PresenceJam`, submitting through the fork
   `Carme99/winget-pkgs` (`fork-user`). Requires `WINGET_TOKEN`: a **classic**
   PAT with `public_repo` **and** `workflow` scopes (fine-grained is
   unsupported; the fork must be synced with upstream before the manifest branch
   is created). The job runs behind the `winget-publish` GitHub environment —
   configure required reviewers for it, or the gate is decorative. The token is
   account-wide (`public_repo` reaches every public repository the account can
   push to, `workflow` reaches `.github/workflows` in this repository), so
   rotate it every 30 days and treat a leak as a full-repository compromise:
   revoke first, then review recent workflow runs and branch/tag history.

### Linux install channels

One `tauri build` produces three x86_64 formats: `.deb` (Debian, Ubuntu, Mint,
popOS), `.rpm` (Fedora, RHEL, openSUSE) and `.AppImage` (everything else). Only
the AppImage is an updater payload — `latest.json`'s `linux-x86_64` points at it
— because tauri-plugin-updater replaces the running AppImage in place. A
`.deb`/`.rpm` install has no AppImage to replace, so those users update through
their package manager.

Policy for adding a channel:

- Flatpak, Snap and AUR are follow-ups (issue #900). Each must land together
  with the package-manager-aware update notice: without it, a package-managed
  install is offered the AppImage payload and the updater has nothing to
  replace — it either no-ops or leaves a stray AppImage behind.
- The arm64 Linux leg (`ubuntu-24.04-arm` plus a `linux-aarch64` key in
  `latest.json`) is deliberately **not** in the matrix yet, for that same
  reason: shipping it before the notice exists would repeat the
  AppImage-payload problem on a new architecture. It is a small matrix change
  once the notice is in.
- Every published format is asserted in the release job's asset check, so a run
  that silently stops producing one fails instead of shipping.

### Cutting a beta

A beta is an ordinary tag whose version carries a prerelease suffix —
`vX.Y.Z-beta.N`. The same pipeline cuts it, with four differences that keep the
Beta channel from leaking into the stable one:

- `ncipollo/release-action` publishes it with `prerelease: true`, so
  `/releases/latest/` — and therefore the `latest.json` every stable install
  reads — still resolves to the newest stable release.
- The CHANGELOG and AppStream gates are satisfied by the entries for the
  version the beta will become (`## [X.Y.Z]`, `# State of Features — vX.Y.Z`
  and the metainfo's newest `<release>`), so the notes are written once, not
  per beta.
- The job also publishes `latest-beta.json` — same body and signatures as
  `latest.json` — onto a rolling **prerelease tagged `beta`**, overwritten with
  `gh release upload --clobber` on every beta cut.
- The verification step downloads
  `https://github.com/Carme99/PresenceJam-Desktop/releases/download/beta/latest-beta.json`
  and requires it to name the beta's version, so a beta that fails to publish
  its manifest fails the run instead of leaving the channel dangling.

That URL is the contract: `BETA_ENDPOINT` in `src-tauri/src/updater_bg.rs` must
be exactly it, and it must never go through `/releases/latest/`, which
structurally cannot resolve to a prerelease — a beta published that way would
serve its own manifest to every stable user.

```bash
git tag -a vX.Y.Z-beta.1 -m "PresenceJam X.Y.Z-beta.1" <merge-sha>
git push origin vX.Y.Z-beta.1
```

The rolling `beta` release must stay a prerelease and must not be deleted: the
Beta channel downloads `latest-beta.json` from that tag, and a 404 there makes
every Beta-channel check fall through to stable silently.

## 5. Cutting a release — checklist

1. `main` is green, and every slice PR for the release is merged.
2. Bump all six literals in §1 (branch `release/X.Y` or a PR into `main`).
3. Update `CHANGELOG.md` per §2 — rename, add the new link definition and re-base
   `[Unreleased]`, all in the same commit. If the release changes shipped behaviour, update
   [`STATE-OF-FEATURES.md`](./STATE-OF-FEATURES.md) too (including its version
   header).

   The AppStream metainfo carries a `<release>` entry per cut: add the new
   version and its date to `src-tauri/linux/com.presencejam.app.metainfo.xml`
   in the same commit. The Linux leg of `release.yml` requires the newest
   entry to name the version being built, so a forgotten one fails the tag
   instead of shipping a package that advertises an older release.

   Both are enforced at tag time as well: `resolve-tag` fails the run
   before any build when the `## [X.Y.Z]` section or the
   `# State of Features — vX.Y.Z` header is missing, so a forgotten rename
   surfaces at the tag instead of as a published release with an empty body.
4. Open the release PR and wait for `version-consistency`, `changelog-links`,
   `rust`, `rust-clippy`, `rust-platform-check`, `frontend` and `secret-scan`.
5. Merge, then tag the **merge commit on `main`** — tag the commit that is on the
   branch, never the local pre-merge commit:

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
