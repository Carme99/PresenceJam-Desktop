# REL + LINKAUDIT — docs-grounding findings

> **Note on link syntax in this file:** every synthetic link probe below is
> written `[text] (target)` — with a space before the parenthesis — so the
> repository's own `docs/link-audit.py` cannot mistake the probe strings for
> real relative links. The syntax is the only thing altered; the targets,
> anchors and verdicts are verbatim as measured.

Audit date: 2026-10-08. HEAD `09341ecaad732e78454a2c65b383f0dfd1541d5a`; tree clean
before this audit's own artifacts (`docs/audit/`, `docs/__pycache__/`) were created.

Claim files verified: `docs/audit/claims/REL.md` (188 claims on `docs/RELEASING.md`)
and `docs/audit/claims/LINKAUDIT.md` (39 claims on `docs/link-audit.py` plus the
`docs-links` CI job). **227 claims total.**

Evidence hierarchy applied: the repository tree on disk first; then allowlisted
vendor docs; `CHANGELOG.md` treated as a claim, not a source; tests over prose,
source over tests. Scope limits honoured — nothing under `pj-worktrees/`,
`archive/`, `docs/archive/`, `node_modules/`, `target/`, `dist/`, `.svelte-kit/`
is read or cited.

Vendor-doc note: no allowlisted vendor page was reachable during this audit, so no
external contract is asserted here. Every verdict rests on the repository tree.

Tree hygiene: the only change this audit made to the working tree is the
`docs/audit/` directory itself. Two byproducts of running the documented
verification command (`docs/__pycache__/`) and of directory listings
(`docs/._.DS_Store`) were moved to `/home/jack/.tmp/audit-strays/` so `git status`
shows only `?? docs/audit/`. No file outside `docs/audit/findings/` was edited;
`docs/link-audit.py` is byte-identical to HEAD (`git diff` empty).

---

## VERDICTS

### REL — `docs/RELEASING.md` (188 claims)

| Claim | Verdict | Evidence | Finding | Sev |
| --- | --- | --- | --- | --- |
| REL-C001 | CONFIRMED | docs/RELEASING.md:1 | `# Releasing PresenceJam` is line 1 verbatim. | — |
| REL-C002 | CONFIRMED | docs/RELEASING.md:3-4 | Tagline matches lines 3-4 verbatim. | — |
| REL-C003 | CONFIRMED | docs/RELEASING.md:5-6; .github/workflows/ci.yml:1; .github/workflows/release.yml:1 | Both cited workflow files exist and are named `CI` / `Release`. | — |
| REL-C004 | CONFIRMED | docs/RELEASING.md:7 | Precedence rule stated; it held for every claim checked against both workflows. | — |
| REL-C005 | CONFIRMED | docs/README.md:1 | `docs/README.md` opens `# Documentation index` — it is the docs index. | — |
| REL-C006 | CONFIRMED | docs/RELEASING.md:9-10 | Both relative links resolve on disk (clean-tree `python3 docs/link-audit.py`: 0 broken). | — |
| REL-C007 | CONFIRMED | docs/RELEASING.md:14 | `## 1. Version-bearing files` present at line 14. | — |
| REL-C008 | CONFIRMED | docs/RELEASING.md:16-26 | Counts check out: 5 files / 6 literals (package-lock contributes two) plus the metainfo as the gated 7th row. | — |
| REL-C009 | CONFIRMED | package.json:3 | `"version": "5.0.0"` is the top-level literal. | — |
| REL-C010 | CONFIRMED | package-lock.json:3 | Top-level `"version": "5.0.0"`. | — |
| REL-C011 | CONFIRMED | package-lock.json:9 | `packages[""].version` = `5.0.0`. | — |
| REL-C012 | CONFIRMED | src-tauri/tauri.conf.json:4 | `"version": "5.0.0"`; tauri.conf.json is the source tauri-bundler stamps into the binary. | — |
| REL-C013 | CONFIRMED | src-tauri/Cargo.toml:2-3 | `name = "presence-jam"`, `version = "5.0.0"` under `[package]`. | — |
| REL-C014 | CONFIRMED | src-tauri/Cargo.lock:3341-3342 | `name = "presence-jam"` / `version = "5.0.0"` block. | — |
| REL-C015 | CONFIRMED | .github/workflows/release.yml:373-391 | The Linux leg runs `appstreamcli validate`, then greps the *first* `<release version=` and fails on mismatch, so a cut must prepend. | — |
| REL-C016 | CONFIRMED | .github/workflows/release.yml:384-390 | True — a missed entry is caught only by that one grep, so the six-literal count silently under-counts. | — |
| REL-C017 | CONFIRMED | docs/RELEASING.md:28 | Sentence present as quoted. | — |
| REL-C018 | CONFIRMED | docs/RELEASING.md:31 | Command present; executed verbatim during this audit. | — |
| REL-C019 | CONFIRMED | docs/RELEASING.md:32 | Command present; executed verbatim during this audit. | — |
| REL-C020 | CONFIRMED | docs/RELEASING.md:33 | Command present; executed verbatim during this audit. | — |
| REL-C021 | CONFIRMED | docs/RELEASING.md:34 | Command present; executed verbatim during this audit. | — |
| REL-C022 | CONFIRMED | docs/RELEASING.md:37 | `### The `package-lock.json` trap` present at line 37. | — |
| REL-C023 | CONFIRMED | package-lock.json:42,56,71,91 | Hundreds of dependency `"version"` literals follow the two root entries. | — |
| REL-C024 | UNSOURCED | package-lock.json:1920 | `cssstyle` is at `4.6.0`, not `5.0.0`, so the "did exactly that at an earlier cut" precedent is not reproducible from this tree. | P3 |
| REL-C025 | CONFIRMED | package-lock.json:3,9 vs 42+ | Exactly two app-owned literals; every other `"version"` sits under a `node_modules/…` key. | — |
| REL-C026 | CONFIRMED | docs/RELEASING.md:42 | Instruction present. | — |
| REL-C027 | CONFIRMED | docs/RELEASING.md:43-44 | Advice is correct — a bare `grep '"version"'` cannot tell root from dependency (both are 5.0.0 here). | — |
| REL-C028 | CONFIRMED | docs/RELEASING.md:47 | Command present; `jq -r '.version, .packages[""].version'` printed `5.0.0` twice. | — |
| REL-C029 | CONFIRMED | docs/RELEASING.md:48 | Command present; `jq -r .version package.json` printed `5.0.0`. | — |
| REL-C030 | CONFIRMED | docs/RELEASING.md:51 | `### How the six literals are gated` present at line 51. | — |
| REL-C031 | CONFIRMED | .github/workflows/ci.yml:454-472; .github/workflows/release.yml:91-128 | The two gates do compare different literal sets (3 vs 6). | — |
| REL-C032 | DRIFT | .github/workflows/ci.yml:463-472 | The gate does not fail on the first disagreement: the loop accumulates `drift=1` for every entry and exits once at the end, so it reports all disagreements before failing. | P3 |
| REL-C033 | CONFIRMED | .github/workflows/release.yml:91-128 | Step `Verify version consistency` in the `resolve-tag` job compares all six literals. | — |
| REL-C034 | CONFIRMED | .github/workflows/release.yml:36,246-247 | `resolve-tag` is a `needs:` of `build`, so the check runs before the matrix. | — |
| REL-C035 | CONFIRMED | .github/workflows/ci.yml:454-472; .github/workflows/release.yml:97-105 | PR-time job reads neither lockfile; tag-time reads both. | — |
| REL-C036 | CONFIRMED | .github/workflows/release.yml:218,226 | `cargo clippy --locked` and `cargo test --locked`; fmt indeed takes no such flag. | — |
| REL-C037 | CONFIRMED | .github/workflows/release.yml:218,226 | Consequence of `--locked`: the lock must land in a PR, not a release build. | — |
| REL-C038 | CONFIRMED | docs/RELEASING.md:70 | "Bump all six literals, every time." present at line 70. | — |
| REL-C039 | CONFIRMED | docs/RELEASING.md:72 | `## 2. CHANGELOG rules` present at line 72. | — |
| REL-C040 | CONFIRMED | CHANGELOG.md:6-7; CHANGELOG.md:13-14,161-162 | Keep-a-Changelog 1.1.0 link present; `Added`/`Changed`/`Fixed`/`Security` all appear as `###` headings. | — |
| REL-C041 | CONFIRMED | CHANGELOG.md:8 | `## [Unreleased]` is the live top section. | — |
| REL-C042 | CONFIRMED | docs/RELEASING.md:78 | "At release time, **in one commit**:" present. | — |
| REL-C043 | CONFIRMED | docs/RELEASING.md:80; CHANGELOG.md:162 | Step 1 present; the real file follows the `## [4.7.0] - 2026-09-17` shape. | — |
| REL-C044 | CONFIRMED | docs/RELEASING.md:81-83; CHANGELOG.md:13-14,161-162 | Step 2 present; each cut does leave `Added`/`Changed`/`Refactor`/`Fixed` headings behind it. | — |
| REL-C045 | CONFIRMED | docs/RELEASING.md:84 | Step 3 present. | — |
| REL-C046 | CONFIRMED | docs/RELEASING.md:87; CHANGELOG.md:1573 | Template present; real definitions use the same `compare/vPREV...vX.Y.Z` shape. | — |
| REL-C047 | CONFIRMED | docs/RELEASING.md:90-91 | Step 4 present and correctly motivates the re-base. | — |
| REL-C048 | CONFIRMED | docs/RELEASING.md:94; CHANGELOG.md:1574 | Template present; the actual `[Unreleased]:` line matches the shape. | — |
| REL-C049 | DRIFT | .github/workflows/ci.yml:428-434 | Same shape as C032: the loop visits every header, records each missing definition, and exits once at the end — it does not stop at the first. | P3 |
| REL-C050 | CONFIRMED | .github/workflows/ci.yml:407-409 | Job header cites issues #437/#438 as the precedent. | — |
| REL-C051 | EXTERNAL-UNVERIFIED | docs/RELEASING.md:99-102 | "CI run **#423** failed on exactly that job" on `release/4.6` is a GitHub Actions run reference; neither the repo tree nor an allowlisted vendor doc can confirm it. | P3 |
| REL-C052 | CONFIRMED | docs/RELEASING.md:102; .github/workflows/ci.yml:420-434 | Advice matches the enforced job. | — |
| REL-C053 | CONFIRMED | docs/RELEASING.md:104 | `## 3. CI gates a release PR must pass` present at line 104. | — |
| REL-C054 | CONFIRMED | .github/workflows/ci.yml:3-7 | `on: push: branches: [main]` and `pull_request: branches: [main]`. | — |
| REL-C055 | CONFIRMED | .github/workflows/ci.yml:19-720 | 13 jobs listed, each with a `name:` that GitHub shows as the PR check context. | — |
| REL-C056 | CONFIRMED | .github/workflows/ci.yml:28-88 | `rust-platform-check` row matches the job exactly. | — |
| REL-C057 | CONFIRMED | .github/workflows/ci.yml:98-174 | `windows-cli-smoke` row matches the job exactly. | — |
| REL-C058 | CONFIRMED | .github/workflows/ci.yml:182-270 | `frontend` row matches the job exactly. | — |
| REL-C059 | CONFIRMED | .github/workflows/ci.yml:282-349 | `rust` row matches the job exactly. | — |
| REL-C060 | CONFIRMED | .github/workflows/ci.yml:358-405 | `rust-clippy` row matches the job exactly. | — |
| REL-C061 | CONFIRMED | .github/workflows/ci.yml:410-434 | `changelog-links` row matches the job exactly. | — |
| REL-C062 | CONFIRMED | .github/workflows/ci.yml:710-720 | `docs-links` row matches the job exactly. | — |
| REL-C063 | CONFIRMED | .github/workflows/ci.yml:444-472 | `version-consistency` row matches the job exactly. | — |
| REL-C064 | CONFIRMED | .github/workflows/ci.yml:477-491 | `secret-scan` row matches the job exactly. | — |
| REL-C065 | CONFIRMED | .github/workflows/ci.yml:524-568 | `dep-audit` row matches the job exactly. | — |
| REL-C066 | CONFIRMED | .github/workflows/ci.yml:574-591 | `no-vendored-binaries` row matches the job exactly. | — |
| REL-C067 | CONFIRMED | .github/workflows/ci.yml:598-612 | `cargo-deny` row matches the job exactly. | — |
| REL-C068 | DRIFT | .github/workflows/ci.yml:670-675,687-704 | The row describes line coverage and per-file floors but omits that every floor is `0` with a calibration TODO, so the job publishes a report and never fails today — it is not a gate a release PR can fail. | P2 |
| REL-C069 | CONFIRMED | .github/workflows/ci.yml:84-88,346-349 | Tests run on macOS (`rust-platform-check`) and Linux (`rust`); the Windows leg runs `cargo check` only. | — |
| REL-C070 | CONFIRMED | .github/workflows/ci.yml:346-349,84-88 | Linux `rust` job runs `cargo test --all-targets`; the macOS leg runs it after its compile check. | — |
| REL-C071 | CONFIRMED | .github/workflows/ci.yml:68-88 | The comment block states the loader failure and `STATUS_ENTRYPOINT_NOT_FOUND`; Windows keeps `cargo check`. | — |
| REL-C072 | CONFIRMED | .github/workflows/ci.yml:65,80-83 | The Windows leg's `cargo check` is described as the cfg-guard. | — |
| REL-C073 | DRIFT | .github/workflows/ci.yml:82-83 | The workflow now says to re-expand "once the dev-dependency hazard is removed, not when an image changes"; the page still tells the reader to wait for the runner image to link the binary cleanly. | P1 |
| REL-C074 | CONFIRMED | docs/RELEASING.md:135 | `## 4. The tag → publish chain (`release.yml`)` present at line 135. | — |
| REL-C075 | CONFIRMED | .github/workflows/release.yml:3-16 | `tags: [v*]` plus `workflow_dispatch` with a `tag` input. | — |
| REL-C076 | CONFIRMED | .github/workflows/release.yml:20-26 | `group: release-${{ github.event_name }}-${{ github.ref }}`, `cancel-in-progress: false`. | — |
| REL-C077 | CONFIRMED | .github/workflows/release.yml:49-70 | `resolve-tag` picks `inputs.tag` on dispatch, else `GITHUB_REF_NAME`. | — |
| REL-C078 | CONFIRMED | .github/workflows/release.yml:60-66 | `case "$TAG" in v*) ;; *) error + exit 1`. | — |
| REL-C079 | CONFIRMED | .github/workflows/release.yml:67-68 | `git ls-remote --exit-code --tags` existence check. | — |
| REL-C080 | CONFIRMED | .github/workflows/release.yml:80-84 | Checkout uses `ref: ${{ steps.resolve.outputs.tag }}`. | — |
| REL-C081 | CONFIRMED | .github/workflows/release.yml:91-128 | All six literals compared against `EXPECTED="${RELEASE_TAG#v}"`. | — |
| REL-C082 | CONFIRMED | .github/workflows/release.yml:72-79; .github/workflows/ci.yml:436-443; src-tauri/src/updater_bg.rs:647,1643,1826 | Drift fails before compile; the updater compares `current_version` against the manifest `version`, so drift re-offers forever (issue #605). | — |
| REL-C083 | CONFIRMED | .github/workflows/release.yml:46-47,173,263,295,600,674,683,932 | Every downstream job consumes `needs.resolve-tag.outputs.tag`. | — |
| REL-C084 | CONFIRMED | .github/workflows/release.yml:135-151 | Step `Verify CHANGELOG section and STATE-OF-FEATURES header` emits `::error::` and fails the run. | — |
| REL-C085 | CONFIRMED | .github/workflows/release.yml:140-143 | `EXPECTED_BASE="${EXPECTED%%-*}"` gates a prerelease tag against the base version's section. | — |
| REL-C086 | CONFIRMED | .github/workflows/release.yml:162-244 | `verify` reruns fmt, clippy, `cargo test --locked`, `npm run check`, `npm run test:coverage`, `npm run test:browser`, with Chromium + Linux system deps. | — |
| REL-C087 | CONFIRMED | .github/workflows/ci.yml:3-7; .github/workflows/release.yml:153-161 | ci.yml is PR/main-only; the verify-job comment states the same rationale. | — |
| REL-C088 | CONFIRMED | .github/workflows/release.yml:155-158 | The comment names the re-cut-of-an-untested-commit worst case. | — |
| REL-C089 | CONFIRMED | .github/workflows/release.yml:246-286 | `needs: [resolve-tag, verify]`, three-OS matrix. | — |
| REL-C090 | CONFIRMED | .github/workflows/release.yml:268-273 | macOS row matches: `aarch64-apple-darwin`, `PresenceJam-<tag>-macos.dmg`, two packaged files. | — |
| REL-C091 | CONFIRMED | .github/workflows/release.yml:274-279 | Windows row matches exactly. | — |
| REL-C092 | CONFIRMED | .github/workflows/release.yml:280-286 | Ubuntu row matches exactly. | — |
| REL-C093 | CONFIRMED | .github/workflows/release.yml:268-269; src-tauri/src/updater_bg.rs:797 | Only `aarch64-apple-darwin` is built and only `darwin-aarch64` is a platform key — Intel Macs get no updater payload. | — |
| REL-C094 | CONFIRMED | .github/workflows/release.yml:436-460,789-798; src-tauri/tauri.conf.json (bundle.windows.nsis.installMode) | NSIS `setup.exe` is the `windows-x86_64` payload (per-user, no elevation); the MSI stays published for managed machines. | — |
| REL-C095 | CONFIRMED | .github/workflows/release.yml:552 | `if-no-files-found: error`. | — |
| REL-C096 | CONFIRMED | .github/workflows/release.yml:538-545 | `Report artifact sizes` prints every `PresenceJam-*` file with its byte count. | — |
| REL-C097 | CONFIRMED | .github/workflows/release.yml:560-563 | `actions/attest-build-provenance` with `subject-path: ${{ matrix.bundle_path }}`. | — |
| REL-C098 | CONFIRMED | .github/workflows/release.yml:325-334,339,345,397 | All three `npx tauri build` steps carry the inline `--config '{"bundle":{"createUpdaterArtifacts":false}}'`; no signing secrets in the job env. | — |
| REL-C099 | CONFIRMED | .github/workflows/release.yml:326-334,571-575; package.json:8 | The compile runs `npm run build` (Vite + every plugin under node_modules) plus the whole dependency tree's build scripts. | — |
| REL-C100 | CONFIRMED | .github/workflows/release.yml:414-434 | The macOS leg tar.gz's the `.app` itself; the comment says tauri's updater bundler no longer runs. | — |
| REL-C101 | CONFIRMED | .github/workflows/release.yml:373-383 | `appstreamcli validate --no-net` runs before `Build Tauri (Linux)`. | — |
| REL-C102 | CONFIRMED | .github/workflows/release.yml:384-390 | `TOP_RELEASE` grep + mismatch error. | — |
| REL-C103 | CONFIRMED | .github/workflows/release.yml:510-530 | Asserts `usr/share/metainfo/com.presencejam.app.metainfo.xml` inside both the `.deb` and the `.rpm`. | — |
| REL-C104 | CONFIRMED | .github/workflows/release.yml:581-628 | `sign`: `needs: [resolve-tag, build]`, `environment: release-signing`, holds `TAURI_SIGNING_PRIVATE_KEY` and its password. | — |
| REL-C105 | CONFIRMED | .github/workflows/release.yml:604-628 | Secrets sit on the `Sign updater payloads` step only, not on the job. | — |
| REL-C106 | CONFIRMED | .github/workflows/release.yml:650-653 | Signs the macOS `.app.tar.gz`, the Windows `setup.exe` and `.msi`, and the Linux `AppImage`. | — |
| REL-C107 | CONFIRMED | .github/workflows/release.yml:657-667 | Uploads a flat `signatures` artifact and attests it. | — |
| REL-C108 | CONFIRMED | .github/workflows/release.yml:586-588 | Comment says the attestation keeps the provenance the build job used to provide. | — |
| REL-C109 | CONFIRMED | .github/workflows/release.yml:577-580 | "add required reviewers to it … Until those reviewers are configured the environment exists but gates nothing." | — |
| REL-C110 | CONFIRMED | .github/workflows/release.yml:669-689 | `needs: [resolve-tag, build, sign]`, checkout at the tag, `digest-mismatch: error`. | — |
| REL-C111 | CONFIRMED | .github/workflows/release.yml:691-708 | `Generate SHA256SUMS.txt` writes `"<sha256>  <filename>"`; the comment says it is unsigned and deliberately not attested. | — |
| REL-C112 | CONFIRMED | .github/workflows/release.yml:715-730 | `notes.md` extraction runs from the `## [VERSION]` header to the next `## [`, with an empty body failing the job. | — |
| REL-C113 | CONFIRMED | .github/workflows/release.yml:732-753 | `ncipollo/release-action` pinned to a SHA, `tag:` explicit, `bodyFile: notes.md`. | — |
| REL-C114 | CONFIRMED | .github/workflows/release.yml:740-753 | `bodyFile` not `generateReleaseNotes`; `allowUpdates: true`; `artifacts: artifacts/**/*`. | — |
| REL-C115 | CONFIRMED | docs/RELEASING.md:221; .github/workflows/release.yml:762 | "It then generates `latest.json`:" at line 221; the step exists. | — |
| REL-C116 | CONFIRMED | .github/workflows/release.yml:765,785 | `--arg version "$VERSION"` where `VERSION="${RELEASE_TAG#v}"`. | — |
| REL-C117 | CONFIRMED | .github/workflows/release.yml:766,786 | `PUB_DATE=$(date -u +"%Y-%m-%dT%H:%M:%SZ")` — UTC RFC3339. | — |
| REL-C118 | CONFIRMED | .github/workflows/release.yml:796 | `platforms: {` key in the jq program. | — |
| REL-C119 | CONFIRMED | .github/workflows/release.yml:787,797 | `darwin-aarch64` → `…/PresenceJam-<tag>.app.tar.gz` + `.sig` contents. | — |
| REL-C120 | CONFIRMED | .github/workflows/release.yml:789,798 | `windows-x86_64` → `…/PresenceJam-<tag>-setup.exe` + `.sig` contents. | — |
| REL-C121 | CONFIRMED | .github/workflows/release.yml:791,799 | `linux-x86_64` → `…/PresenceJam-linux-amd64.AppImage` + `.sig` contents. | — |
| REL-C122 | CONFIRMED | .github/workflows/release.yml:773-782,788-792 | `--arg mac_sig "$(cat "$MAC_SIG")"` etc.; a missing or empty `.sig` exits 1. | — |
| REL-C123 | CONFIRMED | .github/workflows/release.yml:804 | `gh release upload --clobber … latest.json`. | — |
| REL-C124 | CONFIRMED | .github/workflows/release.yml:843-862 | `Verify latest.json assets in release` checks every advertised URL basename against the release asset list. | — |
| REL-C125 | CONFIRMED | .github/workflows/release.yml:868-879 | Pins `.app.tar.gz`, `-setup.exe`, `.deb`, `.rpm`, `.AppImage` on top of the manifest check. | — |
| REL-C126 | CONFIRMED | .github/workflows/release.yml:884-899 | On a beta tag the step resolves the rolling manifest; ci/release comments state a 404 is a silent "no update". | — |
| REL-C127 | CONFIRMED | docs/archive/windows-update-chain-v3.2.md:1-6 | The file exists and documents the v3.1.0 → v3.2.0 Windows incident plus the silent-404 updater behaviour. | — |
| REL-C128 | CONFIRMED | .github/workflows/release.yml:922-948,961-1044 | `homebrew`: `needs: [resolve-tag, release]`, downloads the DMG artifact, `sha256sum`, writes `Casks/presence-jam.rb` in `carme99/homebrew-tap`. | — |
| REL-C129 | CONFIRMED | .github/workflows/release.yml:1017-1027 | Creates the cask if absent; no-ops when version **and** sha256 already match. | — |
| REL-C130 | CONFIRMED | .github/workflows/release.yml:1029-1032 | `git rm` of the stale root `presence-jam.rb`; comment cites the formula → cask migration. | — |
| REL-C131 | CONFIRMED | .github/workflows/release.yml:962-968,990-992 | `HOMEBREW_TAP_TOKEN` secret, required to hold `contents:write` on the tap. | — |
| REL-C132 | CONFIRMED | .github/workflows/release.yml:1046-1108 | `winget` job uses `vedantmgoyal2009/winget-releaser`, `identifier: PresenceJam.PresenceJam`, `fork-user: Carme99`. | — |
| REL-C133 | CONFIRMED | .github/workflows/release.yml:1073-1079 | Comment states `WINGET_TOKEN` must be a classic PAT with `public_repo` AND `workflow` scopes. | — |
| REL-C134 | CONFIRMED | .github/workflows/release.yml:1073-1087 | Fine-grained unsupported; `komac sync-fork` must run before the manifest branch is created. | — |
| REL-C135 | CONFIRMED | .github/workflows/release.yml:1060,1057-1059 | `environment: winget-publish` with the "gates nothing until reviewers are configured" note. | — |
| REL-C136 | CONFIRMED | .github/workflows/release.yml:1050-1056,1090-1095 | Account-wide blast radius is documented for both scopes. | — |
| REL-C137 | CONFIRMED | .github/workflows/release.yml:1088-1095 | "Rotate the PAT every 30 days… revoke it first, then review recent workflow runs and the branch/tag history." | — |
| REL-C138 | CONFIRMED | docs/RELEASING.md:266 | `### Linux install channels` present at line 266. | — |
| REL-C139 | CONFIRMED | src-tauri/tauri.conf.json (bundle.targets); .github/workflows/release.yml:471-488 | One `tauri build` emits `.deb`, `.rpm` and `.AppImage` (targets list contains all three). | — |
| REL-C140 | CONFIRMED | .github/workflows/release.yml:791-799; src-tauri/src/updater_bg.rs:1240-1248 | Only the AppImage is the `linux-x86_64` updater payload; the plugin replaces a running AppImage in place. | — |
| REL-C141 | CONFIRMED | src-tauri/src/updater_bg.rs:1263-1274,1344-1354 | `install_method_for` returns `Deb`/`Rpm` variants pointing at package URLs — those users update via their package manager. | — |
| REL-C142 | UNSOURCED | docs/RELEASING.md:277 | "Flatpak, Snap and AUR are follow-ups (issue #900)" — the in-repo citations of #900 are the AppStream metainfo gate and the `.rpm` target; nothing in the tree corroborates Flatpak/Snap/AUR scope. | P3 |
| REL-C143 | CONFIRMED | docs/RELEASING.md:278-280 | The package-manager-aware-notice requirement is stated (see D4 for its now-stale premise). | — |
| REL-C144 | CONFIRMED | .github/workflows/release.yml:266-286,797-799 | No `ubuntu-24.04-arm` in the matrix and no `linux-aarch64` key in `latest.json`. | — |
| REL-C145 | STALE | src-tauri/src/updater_bg.rs:1238-1275,1344-1354; CHANGELOG.md (Unreleased, #940/#894/#782) | The "package-manager-aware update notice" now exists in-tree: `install_method_for` returns `Deb`/`Rpm` variants with a package URL and the banner shows the package command. "Shipping it before the notice exists" is no longer the reason arm64 Linux is out of the matrix. | P2 |
| REL-C146 | CONFIRMED | docs/RELEASING.md:284-285 | "It is a small matrix change once the notice is in." | — |
| REL-C147 | CONFIRMED | .github/workflows/release.yml:868-879 | The asset check pins every published format. | — |
| REL-C148 | CONFIRMED | docs/RELEASING.md:289 | `### Cutting a beta` present at line 289. | — |
| REL-C149 | CONFIRMED | docs/RELEASING.md:291 | A beta is an ordinary tag carrying a `vX.Y.Z-beta.N` prerelease suffix. | — |
| REL-C150 | CONFIRMED | docs/RELEASING.md:292-308 | Four bulleted differences follow. | — |
| REL-C151 | CONFIRMED | .github/workflows/release.yml:742-748 | `prerelease: ${{ contains(…tag, '-beta') }}`; `/releases/latest/` therefore stays on the newest stable. | — |
| REL-C152 | CONFIRMED | .github/workflows/release.yml:140-143,384-386,715-724 | The CHANGELOG and metainfo gates both strip the `-beta` suffix. | — |
| REL-C153 | CONFIRMED | .github/workflows/release.yml:817-833 | Publishes `latest-beta.json` onto a rolling prerelease tagged `beta`, overwritten with `--clobber`. | — |
| REL-C154 | CONFIRMED | .github/workflows/release.yml:884-897 | Downloads the beta manifest and requires it to name the beta's version. | — |
| REL-C155 | CONFIRMED | src-tauri/src/updater_bg.rs:906-907 | `BETA_ENDPOINT` is exactly `https://github.com/Carme99/PresenceJam-Desktop/releases/download/beta/latest-beta.json`. | — |
| REL-C156 | CONFIRMED | src-tauri/src/updater_bg.rs:2307 | `assert!(!BETA_ENDPOINT.contains("/releases/latest/"))` pins the contract in a test. | — |
| REL-C157 | CONFIRMED | docs/RELEASING.md:316 | `git tag -a vX.Y.Z-beta.1 -m "PresenceJam X.Y.Z-beta.1" <merge-sha>` present. | — |
| REL-C158 | CONFIRMED | docs/RELEASING.md:317 | `git push origin vX.Y.Z-beta.1` present. | — |
| REL-C159 | CONFIRMED | .github/workflows/release.yml:825-833 | The rolling-beta release notes say it must stay a prerelease and must not be deleted. | — |
| REL-C160 | CONFIRMED | docs/RELEASING.md:324 | `## 5. Cutting a release — checklist` present at line 324. | — |
| REL-C161 | CONFIRMED | docs/RELEASING.md:326 | Step 1 present. | — |
| REL-C162 | CONFIRMED | docs/RELEASING.md:327 | Step 2 present. | — |
| REL-C163 | CONFIRMED | docs/RELEASING.md:328-331 | Step 3 present, including the STATE-OF-FEATURES mention. | — |
| REL-C164 | CONFIRMED | docs/STATE-OF-FEATURES.md:1 | `# State of Features — v5.0.0` — the file does carry a version header. | — |
| REL-C165 | CONFIRMED | src-tauri/linux/com.presencejam.app.metainfo.xml:68-70; .github/workflows/release.yml:365-372 | The metainfo carries a `<releases>` block; the release comment says to add an entry per cut (see D2 — the entry is still 4.7.0). | — |
| REL-C166 | CONFIRMED | .github/workflows/release.yml:384-390 | The Linux leg requires the newest entry to name the version being built. | — |
| REL-C167 | CONFIRMED | .github/workflows/release.yml:135-151 | Both `resolve-tag` documentation gates run before any build. | — |
| REL-C168 | CONFIRMED | .github/workflows/ci.yml:19-720 | All 13 named jobs exist in ci.yml. | — |
| REL-C169 | CONFIRMED | docs/RELEASING.md:347-348 | Step 5 present. | — |
| REL-C170 | CONFIRMED | docs/RELEASING.md:351 | `git checkout main && git pull --ff-only` present. | — |
| REL-C171 | CONFIRMED | docs/RELEASING.md:352 | `git tag -a vX.Y.Z -m "PresenceJam X.Y.Z — <codename>" <merge-sha>` present. | — |
| REL-C172 | CONFIRMED | docs/RELEASING.md:353 | `git push origin vX.Y.Z` present. | — |
| REL-C173 | CONFIRMED | docs/RELEASING.md:356 | Step 6 present. | — |
| REL-C174 | CONFIRMED | docs/RELEASING.md:357 | Step 7 present. | — |
| REL-C175 | CONFIRMED | docs/RELEASING.md:360 | `gh release view vX.Y.Z --json tagName,name,assets` present. | — |
| REL-C176 | CONFIRMED | docs/RELEASING.md:361 | `curl -sI …/releases/latest/download/latest.json` present. | — |
| REL-C177 | CONFIRMED | .github/workflows/release.yml:784-801,700-708 | `latest.json` carries `version` plus exactly three platform keys; `SHA256SUMS.txt` is an uploaded artifact. | — |
| REL-C178 | CONFIRMED | docs/STATE-OF-FEATURES.md:138 | `## Release smoke (run once against the published release)` exists; the anchor `#release-smoke-run-once-against-the-published-release` resolves (clean-tree link-audit run: 0 broken). | — |
| REL-C179 | CONFIRMED | docs/STATE-OF-FEATURES.md:147-179 | Smoke item 1 = install the previously published build, stage, quit, relaunch. | — |
| REL-C180 | CONFIRMED | docs/STATE-OF-FEATURES.md:180-206 | Smoke item 2 = sign in, quit, > 1 h closed, relaunch, confirm the session refreshes. | — |
| REL-C181 | CONFIRMED | docs/STATE-OF-FEATURES.md:180-206 | The row is a ⚠ row whose text says to flip it once the smoke is run. | — |
| REL-C182 | CONFIRMED | docs/RELEASING.md:372 | `### Re-cutting an existing tag` present at line 372. | — |
| REL-C183 | CONFIRMED | docs/RELEASING.md:374 | "Use the manual path instead of force-moving the tag:" present. | — |
| REL-C184 | CONFIRMED | docs/RELEASING.md:377; .github/workflows/release.yml:10-16 | `gh workflow run release.yml -f tag=vX.Y.Z` present; the dispatch input is named `tag`. | — |
| REL-C185 | CONFIRMED | .github/workflows/release.yml:60-84,246-247,804,749 | `resolve-tag` validates + checks out; `build` rebuilds; `release` re-uploads with `--clobber` and `allowUpdates: true`. | — |
| REL-C186 | CONFIRMED | .github/workflows/release.yml:7-9,289-295 | The workflow header and the build checkout both frame a re-cut as recovery of an existing tag, not a move to different code. | — |
| REL-C187 | CONFIRMED | .github/workflows/release.yml:739,765 | The tag is what `latest.json`'s `version` is derived from (`VERSION="${RELEASE_TAG#v}"`). | — |
| REL-C188 | DRIFT | .github/workflows/release.yml:96-128 | `resolve-tag` verifies the tag against **six literals in five files** (tauri.conf.json, package.json, both package-lock.json literals, Cargo.toml, Cargo.lock) — not "the three version files". The three-file set is the PR-time `version-consistency` gate. | P2 |

### LINKAUDIT — `docs/link-audit.py` + the `docs-links` CI job (39 claims)

| Claim | Verdict | Evidence | Finding | Sev |
| --- | --- | --- | --- | --- |
| LINKAUDIT-C001 | OVERSTATED | docs/link-audit.py:73,77; CHANGELOG.md:1573-1618 | "Every relative markdown link" is not what runs: the `LINK` regex only matches inline `[text] (target)` forms, so the 46 link-reference definitions in CHANGELOG.md are never audited. It also audits image links (`![…] ()`), which the docstring never mentions. | P2 |
| LINKAUDIT-C002 | CONFIRMED | docs/link-audit.py:4,25 | `SKIP_DIRS` = `.git`, `node_modules`, `target`, `dist`, `build`, `.svelte-kit` — exactly the build/vendor set. | — |
| LINKAUDIT-C003 | CONFIRMED | docs/link-audit.py:82 | `os.path.exists(resolved)` accepts directories, so a directory target passes. | — |
| LINKAUDIT-C004 | CONFIRMED | docs/link-audit.py:7-9,28,43,89 | Explicit `<a name>`/`<a id>` anchors are collected and matched against the fragment. Verified in an isolated tree: `<a name="MyAnchor">` + `[b] (#myanchor)` passes. | — |
| LINKAUDIT-C005 | CONFIRMED | docs/link-audit.py:7-9,42-43 | True — an explicit anchor is not a heading, so a heading-slug-only matcher would report it dead. | — |
| LINKAUDIT-C006 | OVERSTATED | docs/link-audit.py:72 | Only triple-backtick fences are stripped. Verified in an isolated tree: a `~~~`-fenced link, an inline-backtick link and an HTML-comment link are all still audited and flag false positives. | P3 |
| LINKAUDIT-C007 | CONFIRMED | docs/link-audit.py:97,101 | `return 1 if problems else 0` feeds `sys.exit(main())`. Observed exit 1 (working tree, 138 broken) and exit 0 (clean tree). | — |
| LINKAUDIT-C008 | CONFIRMED | docs/link-audit.py:24 | `ROOT` resolves to the repo root; the run prints paths relative to it. | — |
| LINKAUDIT-C009 | CONFIRMED | docs/link-audit.py:25 | Literal set matches, verified by an isolated-tree run that skipped all six names. | — |
| LINKAUDIT-C010 | CONFIRMED | docs/link-audit.py:27 | Regex is verbatim. Verified: `[t] (./a.md "Title")` matches; `[a [b] c] (./nope.md)`, `[t] ()` and `./a file.md` do not. | — |
| LINKAUDIT-C011 | CONFIRMED | docs/link-audit.py:28 | Regex is verbatim. Double quotes only, single `\s+`, one line. | — |
| LINKAUDIT-C012 | CONFIRMED | docs/link-audit.py:30-34 | Verified against `github-slugger@2.0.0` (the package GitHub itself documents as "Generate a slug just like GitHub does for markdown headings"): identical output on 20 headings including `A -- B`, `Über/naïve café`, `🎵 {artist} - {track}`, `§ Tags & more` and `Closed ATX ##`. | — |
| LINKAUDIT-C013 | CONFIRMED | docs/link-audit.py:32 | Line is verbatim. | — |
| LINKAUDIT-C014 | CONFIRMED | docs/link-audit.py:33 | Line is verbatim. | — |
| LINKAUDIT-C015 | CONFIRMED | docs/link-audit.py:34 | Line is verbatim. | — |
| LINKAUDIT-C016 | OVERSTATED | docs/link-audit.py:44,30-34,28 | "Every anchor a browser can reach" is not achieved: setext headings are invisible (the regex is ATX-only), a closed ATX heading yields a trailing hyphen (`closed-atx-`, github-slugger agrees on the raw text but GitHub's renderer strips the closing sequence), `<a name='x'>` / `<a\n name="x">` / `<h2 id="x">` are all missed (verified: `#hid`, `#single`, `#multiline` all reported dead). | P2 |
| LINKAUDIT-C017 | CONFIRMED | docs/link-audit.py:43 | Line is verbatim; the lower-casing here is the cause of defect D9. | — |
| LINKAUDIT-C018 | CONFIRMED | docs/link-audit.py:44 | Regex is verbatim; `#NoSpace` and 4-space-indented `## H` correctly do not match. | — |
| LINKAUDIT-C019 | CONFIRMED | docs/link-audit.py:48-50 | De-dup shape verified: three `## Dup` headings yield `dup`, `dup-1`, `dup-2` — same as github-slugger. | — |
| LINKAUDIT-C020 | CONFIRMED | docs/link-audit.py:56 | Line is verbatim; `os.walk` with default `followlinks=False`. | — |
| LINKAUDIT-C021 | CONFIRMED | docs/link-audit.py:57 | Line is verbatim; pruning is by bare directory name at any depth. | — |
| LINKAUDIT-C022 | CONFIRMED | docs/link-audit.py:59 | Line is verbatim. `endswith(".md")` is case-sensitive: `UPPER.MD`, `x.markdown`, `x.mdx` are invisible (verified). | — |
| LINKAUDIT-C023 | CONFIRMED | docs/link-audit.py:61 | Line is verbatim; output ordering is deterministic. | — |
| LINKAUDIT-C024 | CONFIRMED | docs/link-audit.py:72 | Line is verbatim; non-greedy, paired across the whole file, info strings tolerated. | — |
| LINKAUDIT-C025 | CONFIRMED | docs/link-audit.py:74 | Line is verbatim. Case-sensitive `startswith`, so `HTTPS://…`, `MAILTO:…`, `ftp://…` and `//example.com/x.md` are reported as missing targets. | — |
| LINKAUDIT-C026 | CONFIRMED | docs/link-audit.py:77 | Line is verbatim; splits on the first `#` only. | — |
| LINKAUDIT-C027 | CONFIRMED | docs/link-audit.py:79-81 | Line is verbatim; resolution is per linking file (correct depth) and purely lexical — no `realpath`, no symlink resolution, no containment check. | — |
| LINKAUDIT-C028 | CONFIRMED | docs/link-audit.py:82 | Line is verbatim. | — |
| LINKAUDIT-C029 | CONFIRMED | docs/link-audit.py:86 | Line is verbatim; an empty target falls back to the linking file itself. | — |
| LINKAUDIT-C030 | CONFIRMED | docs/link-audit.py:89 | Line is verbatim. The `endswith(".md")` gate means an anchor on a non-`.md` target (`[x] (README#anchor)`) is never checked. | — |
| LINKAUDIT-C031 | CONFIRMED | docs/link-audit.py:93 | Line is verbatim; observed verbatim in the run below. | — |
| LINKAUDIT-C032 | CONFIRMED | docs/link-audit.py:97 | Line is verbatim. | — |
| LINKAUDIT-C033 | CONFIRMED | docs/link-audit.py:101 | Line is verbatim. | — |
| LINKAUDIT-C034 | CONFIRMED | .github/workflows/ci.yml:706-709 | The comment block is verbatim at lines 706-709 and names the script's two checks plus "No npm ci/node — plain python3". | — |
| LINKAUDIT-C035 | CONFIRMED | .github/workflows/ci.yml:711 | `name: Docs links` at line 711. | — |
| LINKAUDIT-C036 | CONFIRMED | .github/workflows/ci.yml:712 | `runs-on: ubuntu-latest` at line 712. | — |
| LINKAUDIT-C037 | CONFIRMED | .github/workflows/ci.yml:713 | `timeout-minutes: 5` at line 713. | — |
| LINKAUDIT-C038 | CONFIRMED | .github/workflows/ci.yml:716 | `uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1  # v7.0.1` at line 716. | — |
| LINKAUDIT-C039 | CONFIRMED | .github/workflows/ci.yml:719-720 | Step `Audit relative markdown links` runs `python3 docs/link-audit.py` with no path filter, no `working-directory`, no npm/node. | — |

---

## DEFECTS (non-CONFIRMED, by blast radius)

### D1 — REL-C073 · P1 · wrong recovery condition for the Windows test leg

```
claim_id: REL-C073
file: docs/RELEASING.md:131-133
class: C6
verdict: DRIFT
evidence: .github/workflows/ci.yml:82-83
finding: The page still reads "Re-expand the Windows test leg when the runner image
  links the binary cleanly". ci.yml's own comment now says the opposite: "Re-expand
  to both legs once the dev-dependency hazard is removed, not when an image
  changes." The page has not been updated to the corrected cause (the
  `tauri = { features = ["test"] }` dev-dependency that changes the test binary's
  import table, per ci.yml:71-79 and PR #1116's paired binary-hash experiment).
proposed_fix: Replace the sentence with: "Re-expand the Windows test leg once the
  `tauri = { features = [\"test\"] }` dev-dependency hazard is removed, not when a
  runner image changes; record any further platform-specific reduction here rather
  than leaving it implied."
severity: P1
```

### D2 — cross-file · P1 · the 5.0.0 cut cannot pass its own tag gates today

```
claim_id: — (no claim covers this; cross-file consequence of §1/§5 step 3)
file: src-tauri/linux/com.presencejam.app.metainfo.xml:70; CHANGELOG.md:8
class: C5
verdict: DRIFT
evidence: .github/workflows/release.yml:143,387; package.json:3; package-lock.json:3,9;
  src-tauri/tauri.conf.json:4; src-tauri/Cargo.toml:3; src-tauri/Cargo.lock:3342;
  docs/STATE-OF-FEATURES.md:1
finding: All six version literals are already 5.0.0 and STATE-OF-FEATURES.md carries
  `# State of Features — v5.0.0`, but (a) CHANGELOG.md has no `## [5.0.0]` section —
  the top section is `## [Unreleased]`, whose own prose says the premature
  `## [5.0.0] - 2026-10-06` cut was folded back — and (b) the metainfo's newest
  `<release>` entry is still `version="4.7.0" date="2026-09-17"`. A `v5.0.0` tag push
  therefore fails `resolve-tag` at release.yml:143 ("CHANGELOG.md has no
  '## [5.0.0]' section") before anything builds; if that gate were passed, the
  Linux build leg would then fail at release.yml:387 ("metainfo's newest release
  entry says '4.7.0' but this tag builds '5.0.0'"). The version-bump site list in §1
  is complete and correct — the repo simply has not executed §5 step 3 for this cut.
proposed_fix: No doc change needed for §1/§5 (both are accurate). File a release-prep
  task: rename `## [Unreleased]` → `## [5.0.0] - <date>`, add a fresh empty
  `## [Unreleased]` plus both link definitions in the same commit, and prepend a
  `<release version="5.0.0" date="<date>">` entry to the metainfo.
severity: P1
```

### D3 — REL-C188 · P2 · "the three version files" undercounts the tag gate

```
claim_id: REL-C188
file: docs/RELEASING.md:385
class: C2
verdict: DRIFT
evidence: .github/workflows/release.yml:96-128
finding: "resolve-tag verifies it against the three version files before anything
  builds" is wrong. The tag gate compares SIX literals across FIVE files:
  src-tauri/tauri.conf.json, package.json, package-lock.json (top-level and
  packages[""]), src-tauri/Cargo.toml and src-tauri/Cargo.lock's presence-jam
  entry (release.yml:114-120). The three-file set is the PR-time
  `version-consistency` gate, which the same page already documents correctly at
  §1 / REL-C032. Note the same undercount appears in ci.yml's own comment
  (ci.yml:442, "release.yml gates the tag against all three sources").
proposed_fix: Replace "and `resolve-tag` verifies it against the three version files
  before anything builds" with "and `resolve-tag` verifies it against all six
  literals in §1 before anything builds".
severity: P2
```

### D4 — REL-C145 · P2 · the arm64-Linux rationale is stale

```
claim_id: REL-C145
file: docs/RELEASING.md:283
class: C7
verdict: STALE
evidence: src-tauri/src/updater_bg.rs:1238-1275,1344-1354; CHANGELOG.md [Unreleased]
  entry "The update check and the deferred download are now bounded, and
  `.deb`/`.rpm` installs can actually update (#940, #894, #782 — #1133)"
finding: The doc says arm64 Linux is out of the matrix "for that same reason:
  shipping it before the notice exists would repeat the AppImage-payload problem".
  The package-manager-aware update notice does exist now: `install_method_for`
  (updater_bg.rs:1344) returns `UpdateInstall::Deb { package_url }` /
  `UpdateInstall::Rpm { package_url }` from `tauri::utils::platform::bundle_type()`,
  and the doc comment at 1263-1264 says "apt owns the install, so the banner shows
  the package command and no install button". The notice shipped in the unreleased
  5.0.0 work, so the stated blocker no longer applies.
proposed_fix: Reword to: "The arm64 Linux leg (`ubuntu-24.04-arm` plus a
  `linux-aarch64` key in `latest.json`) is not in the matrix yet. It is a small
  matrix change; the package-manager-aware notice (`install_method_for` in
  src-tauri/src/updater_bg.rs) already covers `.deb`/`.rpm`, so arm64 only needs the
  same treatment."
severity: P2
```

### D5 — REL-C068 · P2 · `rust-coverage` is listed as a gate but cannot fail

```
claim_id: REL-C068
file: docs/RELEASING.md:123
class: C6
verdict: DRIFT
evidence: .github/workflows/ci.yml:667-675,687-704
finding: The §3 table is introduced as "CI gates a release PR must pass" and step 4
  of the checklist tells the reader to wait for `rust-coverage`. The job publishes a
  report only: `cargo llvm-cov … --fail-under-lines 0` and every per-file floor is
  `0` with a `TODO(#838): calibrate` comment. AGENTS.md §11 states this explicitly
  ("never fails today — every per-file floor on `main` is `0`, so it is informational
  only"); RELEASING.md does not, so a release engineer waits on a job that can only
  fail on a build/infra error.
proposed_fix: Change the row's What-it-does cell to: "Line coverage and per-file
  floors — **report-only today**: `--fail-under-lines 0` and every floor is `0`
  pending #838 calibration, so the job cannot fail on coverage."
severity: P2
```

### D6 — LINKAUDIT-C001 · P2 · reference-style links are never audited

```
claim_id: LINKAUDIT-C001
file: docs/link-audit.py:2
class: C7
verdict: OVERSTATED
evidence: docs/link-audit.py:73,77; CHANGELOG.md:1573-1618
finding: "Audit every relative markdown link in the repository" is not what runs.
  The `LINK` regex (line 27) only matches inline `[text] (target)` forms, so the 46
  link-reference definitions in CHANGELOG.md are never checked, and neither are
  `[ref][id]` usages. Verified in an isolated tree: `[r1][ref]` with
  `[ref]: ./nope.md` passes silently. In the other direction the script *does* audit
  image links (`![shot] (../missing.png)` → "missing target"), which the docstring
  never mentions, and reports them with the same wording as a broken doc link.
proposed_fix: Reword the docstring line to: "Audit every inline markdown link (and
  image) target in the repository." and add a bullet: "* reference-style link
  definitions (`[id]: target`) and `[text][id]` usages are NOT checked."
severity: P2
```

### D7 — LINKAUDIT-C016 · P2 · four classes of reachable anchor are missed

```
claim_id: LINKAUDIT-C016
file: docs/link-audit.py:38
class: C2
verdict: OVERSTATED
evidence: docs/link-audit.py:28,30-34,44
finding: "Every anchor a browser can reach in one markdown file" misses four
  reachable-anchor classes. Verified in an isolated tree and against
  github-slugger@2.0.0:
  (a) setext headings — `anchors_of` only matches ATX (`^#{1,6}\\s+`), so a
      `Title\\n====` heading yields no anchor; GitHub emits `title`.
  (b) closed ATX headings — `## Closed ATX ##` groups as `Closed ATX ##`, so
      `slug()` returns `closed-atx-` (github-slugger agrees on the raw text, but
      GitHub's renderer strips the closing sequence before slugging), so a link to
      `#closed-atx` is reported dead.
  (c) `<a name='single'>`, `<a\\n name="multiline">` — the regex requires double
      quotes and a single space; `#single` and `#multiline` were reported dead.
  (d) `<h2 id="hid">` — the regex requires the literal `<a`, so `#hid` was reported
      dead even though the anchor is real.
  Also over-permissive: a `## Heading` inside a ``` or ~~~ fence in the *target*
  file IS added to the anchor set, because `anchors_of` does no fence stripping.
proposed_fix: Reword the docstring to "Every ATX heading slug plus explicit
  `<a name=…>`/`<a id=…>` anchors in one markdown file." and add a `Known gaps:`
  note covering setext headings, closed ATX headings, single-quoted or
  newline-broken `<a>` attributes, and `<h2 id=…>`.
severity: P2
```

### D8 — LINKAUDIT-C006 · P2 · code-fence handling is internally inconsistent

```
claim_id: LINKAUDIT-C006
file: docs/link-audit.py:11
class: C7
verdict: OVERSTATED
evidence: docs/link-audit.py:72 vs docs/link-audit.py:37-51
finding: "Absolute URLs, `mailto:` links and bare fragments of code fences are
  ignored" is only half true. `main()` strips triple-backtick fences (line 72);
  `anchors_of()` strips nothing. Verified in an isolated tree:
  - `~~~`-fenced links are still audited → `[dead2] (./gone2.md)` inside `~~~`
    reports "missing target" (false positive).
  - inline-backtick and HTML-comment links are still audited (false positives).
  - an unclosed fence leaves the trailing block unstripped.
  - a `## Heading` inside a ``` or ~~~ fence in the *target* file is accepted as a
    live anchor (under-detection), so the docstring's "code fences are ignored"
    does not cover `anchors_of` at all.
proposed_fix: Reword to: "Absolute URLs, `mailto:` links and links inside
  triple-backtick fences in the scanned file are ignored." and add: "Not ignored:
  `~~~` fences, inline code spans, HTML comments; and fenced headings in a *target*
  file still count as live anchors."
severity: P2
```

### D9 — LINKAUDIT-C017 · P2 · explicit anchors are stored lower-cased but compared case-sensitively

```
claim_id: LINKAUDIT-C017
file: docs/link-audit.py:43
class: C2
verdict: DRIFT
evidence: docs/link-audit.py:43 vs docs/link-audit.py:89
finding: `anchors.add(explicit.lower())` folds the stored anchor to lower case, but
  line 89 compares the raw fragment from the link (`anchor not in anchors_of(...)`)
  case-sensitively. Verified in an isolated tree: `<a name="MyAnchor">` plus
  `[x] (#MyAnchor)` reports `dead anchor #MyAnchor`, while `[x] (#myanchor)` passes.
  The same asymmetry bites any capitalised heading fragment (`#Section-One` vs
  `## Section One`). The repo's only genuine explicit anchor,
  `SETUP.md:221 <a name="linux-keyring">`, is already lower case, so the bug is
  masked in practice — the clean-tree CI run is green.
proposed_fix: Compare case-insensitively at line 89 — e.g.
  `if resolved.endswith(".md") and anchor.lower() not in anchors_of(resolved):`
  — or store both cases in `anchors_of`.
severity: P2
```

### D10 — REL-C032 + REL-C049 · P3 · "fails on the first" is not what either loop does

```
claim_id: REL-C032, REL-C049
file: docs/RELEASING.md:57,97-98
class: C2
verdict: DRIFT
evidence: .github/workflows/ci.yml:463-472,428-434
finding: Both gates accumulate and exit once at the end. `version-consistency`
  sets `drift=1` for every disagreeing entry inside the loop and `exit "$drift"`
  after it; `changelog-links` sets `missing=1` per header and `exit $missing` after
  the loop. Neither stops at the first failure, so a maintainer fixing drift sees
  every disagreement in one run, not one at a time. (ci.yml's own header comment at
  407-409 repeats the same "first missing def" wording, so the page is mirroring a
  stale workflow comment.)
proposed_fix: Change "It fails on the first disagreement" to "It reports every
  disagreement and fails the job"; and "fails on the first one whose `[X]:`
  definition is missing" to "fails the job when any `[X]:` definition is missing".
severity: P3
```

### D11 — REL-C024 · P3 · the `cssstyle` precedent is not reproducible from the tree

```
claim_id: REL-C024
file: docs/RELEASING.md:40
class: C7
verdict: UNSOURCED
evidence: package-lock.json:1920
finding: "a dependency's version is free to coincide with the app's — `cssstyle`
  did exactly that at an earlier cut." `cssstyle` is a transitive dependency in this
  lockfile at version `4.6.0`, i.e. it does not coincide with the app's `5.0.0`
  today, and no git history reference is given. The general point stands (three
  dependencies in the lock are at `5.0.0` today — `data-urls`,
  `w3c-xmlserializer`, `xml-name-validator` — so the trap is live), but the named
  precedent cannot be verified from the repository.
proposed_fix: Either cite the tag/commit where `cssstyle` sat at the app's version,
  or swap the example for one reproducible here: "three dependencies currently sit
  at `5.0.0` in `package-lock.json` — `data-urls`, `w3c-xmlserializer` and
  `xml-name-validator` — so a bare `grep` over that file returns five `5.0.0`
  hits, only two of which are ours."
severity: P3
```

### D12 — REL-C051 · P3 · the `release/4.6` CI-run precedent is externally unverifiable

```
claim_id: REL-C051
file: docs/RELEASING.md:99-102
class: C7
verdict: EXTERNAL-UNVERIFIED
evidence: docs/RELEASING.md:99-102 (claim text); no in-repo or allowlisted-vendor source
finding: "on `release/4.6` the section was renamed without adding the link
  definition in the same commit and CI run **#423** failed on exactly that job" is a
  GitHub Actions run reference. Neither the repository tree nor an allowlisted
  vendor doc (github.com is not on the allowlist) can confirm the run number, the
  branch name, or the failure. The operational advice that follows ("Rename and
  define together, or not at all") is independently correct — it matches the
  enforced job at ci.yml:420-434.
proposed_fix: Either keep it and mark it as a historical note ("historical: a CI run
  on release/4.6 failed on this job"), or drop the run number and keep the rule.
severity: P3
```

### D13 — REL-C142 · P3 · issue #900's scope is only partly corroborated

```
claim_id: REL-C142
file: docs/RELEASING.md:277
class: C7
verdict: UNSOURCED
evidence: .github/workflows/release.yml:365-372,469-470
finding: "Flatpak, Snap and AUR are follow-ups (issue #900)". The in-repo citations
  of #900 are the AppStream metainfo gate (release.yml:365, "AppStream gate (issue
  #900)") and the `.rpm` target (release.yml:469, "issue #900: Fedora, RHEL and
  openSUSE users had no native install path"). Nothing in the tree corroborates that
  #900 also scopes Flatpak, Snap or AUR. github.com is not on the vendor allowlist,
  so the issue body cannot be checked.
proposed_fix: Cite the issue body explicitly, or soften to "Flatpak, Snap and AUR
  are untracked follow-ups" until a tracked issue covers them.
severity: P3
```

---

## COUNTS

### Per-claim-file

| Claim file | Claims | CONFIRMED | DRIFT | STALE | UNSOURCED | OVERSTATED | MISSING | EXTERNAL-UNVERIFIED |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| REL | 188 | 179 | 5 | 1 | 2 | 0 | 0 | 1 |
| LINKAUDIT | 39 | 36 | 0 | 0 | 0 | 3 | 0 | 0 |
| **Total** | **227** | **215** | **5** | **1** | **2** | **3** | **0** | **1** |

215 + 5 + 1 + 2 + 3 + 0 + 1 = **227** ✓

### Non-CONFIRMED claim IDs

| Verdict | Claim IDs |
| --- | --- |
| DRIFT (5) | REL-C032, REL-C049, REL-C068, REL-C073, REL-C188 |
| STALE (1) | REL-C145 |
| UNSOURCED (2) | REL-C024, REL-C142 |
| OVERSTATED (3) | LINKAUDIT-C001, LINKAUDIT-C006, LINKAUDIT-C016 |
| EXTERNAL-UNVERIFIED (1) | REL-C051 |
| MISSING (0) | — |

### Defects by severity

| Severity | Defect IDs | Claim IDs |
| --- | --- | --- |
| P1 | D1, D2 | REL-C073 (+ one cross-file finding) |
| P2 | D3, D4, D5, D6, D7, D8, D9 | REL-C188, REL-C145, REL-C068, LINKAUDIT-C001, LINKAUDIT-C016, LINKAUDIT-C006, LINKAUDIT-C017 |
| P3 | D10, D11, D12, D13 | REL-C032 + REL-C049, REL-C024, REL-C051, REL-C142 |

Every P0/P1 above carries `path:line` evidence: D1 → `.github/workflows/ci.yml:82-83`;
D2 → `.github/workflows/release.yml:143,387` and `src-tauri/linux/com.presencejam.app.metainfo.xml:70`.

---

## APPENDIX A — `docs/link-audit.py` independently re-derived

The 39 LINKAUDIT claims are verdicts above. This appendix records the measurements
behind them, run against this checkout on 2026-10-08.

### A.1 Verbatim run output and exit codes

Working tree (what a developer sees), at repo root:

```
$ python3 docs/link-audit.py
markdown files scanned : 119
relative links checked : 306 (82 with #anchors)
broken                 : 138
  BROKEN: docs/audit/claims/AGENTS.md: dead anchor #15-migration-from-claudemd
  … (137 further lines, every one under docs/audit/claims/)
EXIT_CODE=1
```

Clean `git archive HEAD` tree (what CI sees), extracted to an empty directory:

```
$ python3 docs/link-audit.py
markdown files scanned : 28
relative links checked : 157 (47 with #anchors)
broken                 : 0
EXIT_CODE=0
```

`git ls-files '*.md' | wc -l` = **28**, matching the scanned count exactly — CI sees
every tracked `.md` and nothing else.

### A.2 Discovery coverage

`ROOT` (line 24) is the repo root; `markdown_files()` walks it with
`followlinks=False` and prunes by **bare directory name at any depth**
(`.git`, `node_modules`, `target`, `dist`, `build`, `.svelte-kit`). Selection is
`fn.endswith(".md")`, case-sensitive.

Verified in an isolated tree (a copy of the script under a synthetic root):

| Input | Scanned? |
| --- | --- |
| `root.md`, `src/nested.md`, `sub/deep/deeper/deepest.md` | yes |
| `build/b.md`, `dist/d.md`, `target/t.md`, `.svelte-kit/s.md`, `node_modules/pkg/r.md` | no (pruned) |
| `UPPER.MD`, `other.markdown`, `thing.mdx` | no (not `.md`) |
| `src` reached through a `srclink -> src` symlink | no (`followlinks=False`) |

On-disk today: **no `.md` files exist under `build/`, `dist/`, `target/` or
`.svelte-kit/`**; `node_modules/` holds 229, all correctly skipped. Nothing is lost
to the skip list.

**`.md` files added since the script was written that discovery would MISS: none.**
`docs/link-audit.py` landed in `8afc935` (2026-09-17, "split ARCHITECTURE.md into six
pages, add docs index and RELEASING guide (#712)"). Every `.md` added to the tree
after that commit — `docs/PLATFORMS.md` (`3b5ceaa`, 2026-09-19) and `AGENTS.md`
(`820357c`, 2026-09-23) — sits at a discoverable location and is scanned. The blind
spots are all *content*-shaped, not location-shaped (see A.4).

`.worktrees/` exists and is empty. It is **not** in `SKIP_DIRS`, so once worktrees
are checked out there (AGENTS.md §13's worktree-per-branch convention) the main-root
run will descend into them and resolve *their* links against the main `ROOT`,
producing mass false "missing target" / "dead anchor" reports.

### A.3 Anchor-slug algorithm vs GitHub's slugger

`slug()` (lines 30-34) is `text.strip().lower()`, keep `isalnum() or ch in "-_" or
ch == " "`, then `"".join(kept).replace(" ", "-")`.

Tested against **`github-slugger@2.0.0`** — the package whose own description is
"Generate a slug just like GitHub does for markdown headings" — on 20 headings.
Output was **byte-identical on all 20**, including the required cases:

| Heading | Script slug | github-slugger@2.0.0 |
| --- | --- | --- |
| `A -- B` (the `--` case) | `a----b` | `a----b` |
| `Über/naïve café` (unicode) | `übernaïve-café` | `übernaïve-café` |
| `🎵 {artist} - {track}` (emoji) | `-artist---track` | `-artist---track` |
| `Closed ATX ##` (closed ATX) | `closed-atx-` | `closed-atx-` |
| `§ Tags & more` | `-tags--more` | `-tags--more` |
| `a  double  space` | `a--double--space` | `a--double--space` |
| `3. Quick reference` | `3-quick-reference` | `3-quick-reference` |
| `Fix the \`poll\` bug` | `fix-the-poll-bug` | `fix-the-poll-bug` |
| `i18n contract` | `i18n-contract` | `i18n-contract` |
| `Authoring rules (Rust)` | `authoring-rules-rust` | `authoring-rules-rust` |
| `under_score` / `dot.separated` / `plus+sign` | `under_score` / `dotseparated` / `plussign` | same |
| `Hello, World!` / `CamelCase` / `---` / `x ##` / `Foo # Bar` | `hello-world` / `camelcase` / `---` / `x-` / `foo--bar` | same |

Duplicate de-duplication also matches: three `## Dup` headings yield
`{dup, dup-1, dup-2}` in both.

**Correction to the claim file's implementation note.** The note states that
"GitHub strips the closing sequence and yields `closed-atx`" for `## Closed ATX ##`.
That is a property of GitHub's markdown *renderer* (which strips the ATX closing
sequence before slugging), not of `github-slugger` the library, which returns
`closed-atx-` on the raw text exactly as this script does. The observable defect is
still real — a link to `#closed-atx` **is** reported dead by this script — but the
mechanism is "the script slugs the un-stripped heading text", not "the script's
slugger diverges from github-slugger".

Two further corrections to the claim file's implementation notes:

1. It asserts "`<h2 id="foo">` is matched (the `id` alternative)". It is **not**:
   `EXPLICIT_ANCHOR` requires the literal `<a`, so `<h2 id="foo">` never matches.
   Verified: `<h2 id="hid">` + `[hiddenid] (#hid)` → `dead anchor #hid`.
2. It cites 46 + 2 link-reference definitions. The measured counts are **46 in
   CHANGELOG.md and 0 in docs/RELEASING.md** (RELEASING.md uses only inline links).

### A.4 Behavioural matrix (isolated tree, each case verified)

| Construct | Observed | Note |
| --- | --- | --- |
| `<a name="MyAnchor">` + `[x] (#MyAnchor)` | `dead anchor #MyAnchor` | **case-folding asymmetry** (D9) |
| `<a name="MyAnchor">` + `[x] (#myanchor)` | passes | |
| `<a name='single'>` + `[#single]` | `dead anchor` | single quotes unsupported |
| `<a\n name="multiline">` + `[#multiline]` | `dead anchor` | one-line regex |
| `<h2 id="hid">` + `[#hid]` | `dead anchor` | `<a` required |
| `[t] (./a.md "Title")` | correct | optional title group works |
| `[t] (<./a.md>)` | `missing target <./a.md>` | false positive |
| `[t] (#)` | passes silently | empty anchor skips the `if anchor:` gate |
| `[t] ()` / `[a [b] c] (./nope.md)` / `./a file.md` | not matched at all | silently unchecked |
| `[dir] (./#Target-Doc)` where `adir/` exists | passes | directory target skips the anchor check |
| `HTTPS://x`, `MAILTO:a@b.c`, `ftp://f.md`, `//example.com/x.md` | `missing target` | case-sensitive `startswith` |
| `![shot] (../missing.png)` | `missing target` | images audited, same wording as doc links |
| `[r1][ref]` + `[ref]: ./nope.md` | passes | reference-style never audited (D6) |
| `~~~`-fenced link / inline-backtick link / HTML-comment link | all flagged | fence asymmetry (D8) |
| unclosed ``` fence | trailing block unstripped | |
| `## Fenced Heading` inside ``` or ~~~ in the *target* file | accepted as live anchor | under-detection (D8) |
| setext `Title\n====` | no anchor emitted | (D7) |
| `#NoSpace`, 4-space-indented `## H`, `<!-- ## H -->` | not a heading | correct |
| non-UTF-8 `.md` file (byte `0xff`) | `UnicodeDecodeError` traceback, exit 1, **no summary output at all** | unguarded `open()` in both `main` and `anchors_of` |
| `[esc] (../../../../etc/hostname` | existence-tested against the real filesystem | lexical `normpath`, no containment check |

### A.5 The audit's own self-inflicted artifact

`docs/audit/claims/` (21 `.md`) and `docs/audit/findings/` (3–5 `.md`) are
**untracked but not gitignored** (`git check-ignore docs/audit/claims/REL.md` → no
match). They are therefore:

- scanned by a local `python3 docs/link-audit.py` run (26 files today), contributing
  **all 138** of the working tree's "broken" links — the copies fail because
  root-relative links like `./ARCHITECTURE.md` resolve against
  `docs/audit/claims/`, where they genuinely do not exist (correct per-file-relative
  resolution, noise for docs that were only copied);
- invisible to CI, which checks out only tracked files (28 scanned, 0 broken, exit 0).

So **the local exit code (1) differs from CI's (0) purely because of the audit's own
working copies** plus the 65 gitignored `.tmp/backlog/*.md` scratch issue bodies
(those are clean). `docs/audit/` should be added to `.gitignore` (or the audit run
from a directory outside the repo) so that a maintainer running the documented
command gets CI's answer. Note also that `.tmp/backlog` and `docs/audit/` are both
untracked+unignored nested paths, which the `no-vendored-binaries` job
(ci.yml:586, `grep -E '^\?\? [^/]*/?$'`) does not catch — it only inspects root-level
paths.

---

## APPENDIX B — version-bearing-site reconciliation

Answering the audit brief's explicit question: *how many sites does the doc name,
how many actually carry a version, and does `version-consistency` check all of them
or only three?*

| Site | Value at HEAD | In §1's table? | Checked by `version-consistency` (PR-time)? | Checked by `resolve-tag` (tag-time)? |
| --- | --- | --- | --- | --- |
| `package.json` top-level `version` | `5.0.0` (package.json:3) | yes, row 1 | yes | yes |
| `package-lock.json` top-level `version` | `5.0.0` (package-lock.json:3) | yes, row 2 | **no** | yes |
| `package-lock.json` `packages[""].version` | `5.0.0` (package-lock.json:9) | yes, row 3 | **no** | yes |
| `src-tauri/tauri.conf.json` `version` | `5.0.0` (tauri.conf.json:4) | yes, row 4 | yes | yes |
| `src-tauri/Cargo.toml` `[package] version` | `5.0.0` (Cargo.toml:3) | yes, row 5 | yes | yes |
| `src-tauri/Cargo.lock` `presence-jam` entry | `5.0.0` (Cargo.lock:3342) | yes, row 6 | **no** | yes |
| `src-tauri/linux/com.presencejam.app.metainfo.xml` newest `<release>` | **`4.7.0`** (metainfo.xml:70) | yes, row 7 (gated, not a literal) | **no** | **no** — gated in the Linux build leg instead |
| `docs/STATE-OF-FEATURES.md` version header | `# State of Features — v5.0.0` (line 1) | named in §5 step 3, not in the §1 table | no | yes (release.yml:147) |

**Counts.** The doc names **5 files / 6 literals** plus the metainfo as a gated 7th
row — that is exactly what the tree carries, so §1 is accurate. `version-consistency`
checks **only three** of the six literals (ci.yml:457-459: `tauri.conf.json`,
`package.json`, `Cargo.toml`) and no metainfo; `resolve-tag` checks **all six**
(release.yml:114-120) but not the metainfo either. The metainfo is gated only in the
Linux leg of the build matrix (release.yml:384-390).

**Consequence today.** With all six literals at `5.0.0` and the metainfo still at
`4.7.0`, a `v5.0.0` tag would pass `resolve-tag`'s six-literal check, pass `verify`,
and then fail the Linux `build` leg at release.yml:387. Combined with the missing
`## [5.0.0]` CHANGELOG section (D2), the 5.0.0 cut is currently blocked at
`resolve-tag` before it ever reaches the metainfo gate.

**CHANGELOG state vs the doc's `[Unreleased]` procedure.** §2 step 1 says to rename
`## [Unreleased]` → `## [X.Y.Z] - YYYY-MM-DD` at release time. `CHANGELOG.md` is
still on `## [Unreleased]` (line 8), and its own prose records that a
`## [5.0.0] - 2026-10-06` section was cut on 6 Oct, then folded back because no
`v5.0.0` tag was pushed (CHANGELOG.md:10). So the file is genuinely mid-cycle for
5.0.0 — consistent with §2's procedure, but it means the 5.0.0 release PR has not
run §2 steps 1–4 yet.

**No claim in either claim file asserts a `## [5.0.0]` cut date or section** — the
only version-literal claims are the six rows in §1 and the checklist step. The
5.0.0/4.7.0 mismatch is therefore reported here rather than as a claim verdict.
