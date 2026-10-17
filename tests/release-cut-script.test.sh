#!/usr/bin/env bash
#
# tests/release-cut-script.test.sh
#
# Regression tests for scripts/release/cut.sh (issue #1152).
#
# The script edits five files at once, and three of the edits are load-bearing
# in ways a casual reader would not guess:
#
#   1. package-lock.json carries a "version" for every dependency at the SAME
#      six-space indent as packages[""].version. At the current cut there are
#      222 dependency version lines. An indent-based anchor silently rewrites
#      dependency blocks -- and a dependency's version may legitimately equal
#      the app's, so nothing looks wrong afterwards. docs/RELEASING.md §1
#      records an earlier cut where `cssstyle` did exactly that. The fixtures
#      below plant a dependency whose version equals the app's on purpose, so
#      the trap is live rather than theoretical.
#
#   2. com.presencejam.app.metainfo.xml is an accumulating history, so a cut
#      INSERTS a <release> element rather than replacing a version string. The
#      new entry must land FIRST; release.yml greps the first <release version=
#      and fails the build when it is not the version being tagged.
#
#   3. The CHANGELOG rename and its link definition must land together. Issue
#      #437/#438: on release/4.6 the section was renamed without the definition
#      and CI run #423 failed on the changelog-links job.
#
# Every case runs against a throwaway fixture, never the repository.

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/.." && pwd)"
CUT_SH="$REPO/scripts/release/cut.sh"
TMPROOT="$(mktemp -d)"
trap 'rm -rf "$TMPROOT"' EXIT

PASS=0
FAIL=0

ok()   { PASS=$((PASS + 1)); printf '  ok    %s\n' "$1"; }
bad()  { FAIL=$((FAIL + 1)); printf '  FAIL  %s\n' "$1"; [ $# -gt 1 ] && printf '        %s\n' "$2"; }
check() { if [ "$2" = "$3" ]; then ok "$1"; else bad "$1" "expected '$3', got '$2'"; fi; }

# --------------------------------------------------------------- fixture build

# $1 = destination dir. Layouts scripts/release/cut.sh at the real relative
# path so the script's default repo-root derivation is exercised, exactly as it
# is in production.
build_fixture() {
  local d="$1"
  rm -rf "$d"
  mkdir -p "$d/src-tauri/linux" "$d/docs" "$d/scripts/release"
  cp "$CUT_SH" "$d/scripts/release/cut.sh"
  chmod +x "$d/scripts/release/cut.sh"

  cat > "$d/package.json" <<'JSON'
{
  "name": "presence-jam",
  "version": "5.0.0",
  "type": "module"
}
JSON

  # The trap: `sibling` and `version-collision` both sit at 5.0.0, the same as
  # the app. `version-collision` exists purely to be mis-bumped.
  cat > "$d/package-lock.json" <<'JSON'
{
  "name": "presence-jam",
  "version": "5.0.0",
  "lockfileVersion": 3,
  "packages": {
    "": {
      "name": "presence-jam",
      "version": "5.0.0",
      "license": "MIT"
    },
    "node_modules/sibling": {
      "version": "5.0.0",
      "resolved": "https://registry.npmjs.org/sibling/-/sibling-5.0.0.tgz"
    },
    "node_modules/version-collision": {
      "version": "5.0.0",
      "resolved": "https://registry.npmjs.org/version-collision/-/version-collision-5.0.0.tgz"
    }
  }
}
JSON

  cat > "$d/src-tauri/tauri.conf.json" <<'JSON'
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "PresenceJam",
  "version": "5.0.0"
}
JSON

  cat > "$d/src-tauri/Cargo.toml" <<'TOML'
[package]
name = "presence-jam"
version = "5.0.0"
edition = "2021"

[dependencies]
tauri = { version = "~2.12", features = ["tray-icon"] }
TOML

  cat > "$d/src-tauri/Cargo.lock" <<'TOML'
version = 4

[[package]]
name = "presence-jam"
version = "5.0.0"
dependencies = [
 "tauri",
]

[[package]]
name = "tauri"
version = "2.12.2"
TOML

  cat > "$d/src-tauri/linux/com.presencejam.app.metainfo.xml" <<'XML'
<component type="desktop-application">
  <releases>
    <!-- Newest first. release.yml asserts the top entry matches the tag. -->
    <release version="5.0.0" date="2026-10-11">
      <description>
        <p>Prior release summary.</p>
      </description>
    </release>
    <release version="4.7.0" date="2026-09-17">
      <description>
        <p>Older release summary.</p>
      </description>
    </release>
  </releases>
</component>
XML

  cat > "$d/CHANGELOG.md" <<'MD'
# Changelog

All notable changes are documented in this file.

## [Unreleased]

Every change merged after 5.0.0 lands here.

### Fixed

- A fix that has not shipped yet.

## [5.0.0] - 2026-10-11

The 5.0.0 summary.

[5.0.0]: https://github.com/Carme99/PresenceJam-Desktop/compare/v4.7.0...v5.0.0
[Unreleased]: https://github.com/Carme99/PresenceJam-Desktop/compare/v5.0.0...HEAD
[4.7.0]: https://github.com/Carme99/PresenceJam-Desktop/compare/v4.6.0...v4.7.0
MD

  cat > "$d/docs/STATE-OF-FEATURES.md" <<'MD'
# State of Features — v5.0.0

| Row | Status |
MD
}

run_cut() { local d="$1"; shift; ( cd "$d" && ./scripts/release/cut.sh "$@" ) 2>&1; }

# ------------------------------------------------------------------- the cases

echo "==> cut.sh exists and parses"
if bash -n "$CUT_SH" 2>/dev/null; then ok "bash -n passes"; else bad "bash -n fails"; fi

echo
echo "==> verify: accepts a consistent tree"
D="$TMPROOT/a"; build_fixture "$D"
out=$(run_cut "$D" verify); rc=$?
check "exit 0 on a consistent fixture" "$rc" "0"
case "$out" in *"agree at 5.0.0"*) ok "reports the agreed version" ;; *) bad "no agreed-version line" ;; esac

echo
echo "==> verify: REFUSES a tree whose literals disagree"
D="$TMPROOT/b"; build_fixture "$D"
# desynchronise one manifest, the way a hand-edited cut does
perl -0777 -i -pe 's/^version = "5\.0\.0"/version = "5.0.0"/m' "$D/src-tauri/Cargo.toml"
perl -0777 -i -pe 's/^version = "5\.0\.0"$/version = "5.1.0"/m' "$D/src-tauri/Cargo.toml"
out=$(run_cut "$D" verify); rc=$?
check "exit 1 when a literal drifts" "$rc" "1"
case "$out" in *"disagree"*) ok "says which check failed" ;; *) bad "missing disagreement message" ;; esac

echo
echo "==> bump: rewrites all six literals to the new version"
D="$TMPROOT/c"; build_fixture "$D"
run_cut "$D" bump 5.0.1 --date 2026-10-20 >/dev/null 2>&1
check "package.json"              "$(jq -r .version "$D/package.json")"                 "5.0.1"
check "package-lock top-level"    "$(jq -r .version "$D/package-lock.json")"             "5.0.1"
check "package-lock packages[\"\"]" "$(jq -r '.packages[""].version' "$D/package-lock.json")" "5.0.1"
check "tauri.conf.json"           "$(jq -r .version "$D/src-tauri/tauri.conf.json")"     "5.0.1"
check "Cargo.toml"                "$(sed -nE 's/^version = "(.*)"/\1/p' "$D/src-tauri/Cargo.toml" | head -1)" "5.0.1"
check "Cargo.lock"                "$(perl -0777 -ne 'print "$1" if /\[\[package\]\]\s*\nname = "presence-jam"\s*\nversion = "([^"]+)"/' "$D/src-tauri/Cargo.lock")" "5.0.1"

echo
echo "==> bump: does NOT rewrite dependency blocks in package-lock.json"
check "sibling stays 5.0.0"        "$(jq -r '.packages["node_modules/sibling"].version' "$D/package-lock.json")"              "5.0.0"
check "version-collision stays"     "$(jq -r '.packages["node_modules/version-collision"].version' "$D/package-lock.json")" "5.0.0"
check "sibling integrity stays"     "$(jq -r '.packages["node_modules/sibling"].resolved' "$D/package-lock.json")" "https://registry.npmjs.org/sibling/-/sibling-5.0.0.tgz"
# Only the two owned literals may differ from a freshly built fixture. Anything
# else -- a dependency bumped, a reflowed file -- shows up here as extra lines.
D2="$TMPROOT/c2"; build_fixture "$D2"
ndiff=$(diff "$D2/package-lock.json" "$D/package-lock.json" | grep -c '^[<>]')
check "exactly 2 changed values (4 diff lines)" "$ndiff" "4"

echo
echo "==> bump: Cargo.toml dependency specs are untouched"
check "tauri dep spec unchanged" "$(grep -c 'tauri = { version = "~2.12"' "$D/src-tauri/Cargo.toml")" "1"

echo
echo "==> bump: metainfo entry is PREPENDED, newest first"
first_release=$(grep -o -m1 '<release version="[^"]*"' "$D/src-tauri/linux/com.presencejam.app.metainfo.xml" | sed -E 's/.*version="([^"]*)".*/\1/')
check "newest <release> is the new version" "$first_release" "5.0.1"
check "prior entry retained"                "$(grep -c '<release version="5.0.0"' "$D/src-tauri/linux/com.presencejam.app.metainfo.xml")" "1"
check "older entry retained"                "$(grep -c '<release version="4.7.0"' "$D/src-tauri/linux/com.presencejam.app.metainfo.xml")" "1"
check "new entry carries the date"          "$(grep -c '<release version="5.0.1" date="2026-10-20">' "$D/src-tauri/linux/com.presencejam.app.metainfo.xml")" "1"
# the release.yml gate greps the FIRST entry; confirm it would see 5.0.1
if grep -q '<release version="5.0.1"' "$D/src-tauri/linux/com.presencejam.app.metainfo.xml"; then
  ok "release.yml's newest-entry gate would pass"
else
  bad "newest-entry gate would fail"
fi

echo
echo "==> bump: is idempotent for the metainfo (re-run does not duplicate)"
run_cut "$D" bump 5.0.1 --date 2026-10-20 >/dev/null 2>&1
check "still exactly one 5.0.1 entry" "$(grep -c '<release version="5.0.1"' "$D/src-tauri/linux/com.presencejam.app.metainfo.xml")" "1"

echo
echo "==> bump: refuses a non-semver version"
D="$TMPROOT/d"; build_fixture "$D"
run_cut "$D" bump 5.0 >/dev/null 2>&1; rc=$?
check "exit 2 on a bad version" "$rc" "2"
check "nothing was written"     "$(jq -r .version "$D/package.json")" "5.0.0"

echo
echo "==> changelog: renames, opens a fresh section, and fixes both links"
D="$TMPROOT/e"; build_fixture "$D"
run_cut "$D" changelog 5.0.1 --date 2026-10-20 >/dev/null 2>&1; rc=$?
check "exit 0" "$rc" "0"
check "released header written"        "$(grep -c '^## \[5\.0\.1\] - 2026-10-20$' "$D/CHANGELOG.md")" "1"
check "a fresh [Unreleased] was opened" "$(grep -c '^## \[Unreleased\]$' "$D/CHANGELOG.md")" "1"
check "new section link definition"    "$(grep -c '^\[5\.0\.1\]: https://github.com/Carme99/PresenceJam-Desktop/compare/v5\.0\.0\.\.\.v5\.0\.1$' "$D/CHANGELOG.md")" "1"
check "[Unreleased] re-based"          "$(grep -c '^\[Unreleased\]: https://github.com/Carme99/PresenceJam-Desktop/compare/v5\.0\.1\.\.\.HEAD$' "$D/CHANGELOG.md")" "1"
check "prior link definition kept"     "$(grep -c '^\[5\.0\.0\]: ' "$D/CHANGELOG.md")" "1"
check "shipped entry moved under the release" "$(grep -c '^- A fix that has not shipped yet\.$' "$D/CHANGELOG.md")" "1"
# the section must come AFTER the fresh Unreleased one
first_unrel=$(grep -n '^## \[Unreleased\]$' "$D/CHANGELOG.md" | cut -d: -f1)
rel_line=$(grep -n '^## \[5\.0\.1\] - 2026-10-20$' "$D/CHANGELOG.md" | cut -d: -f1)
if [ "$first_unrel" -lt "$rel_line" ]; then ok "fresh [Unreleased] sits above the released section"; else bad "section order is wrong"; fi

echo
echo "==> changelog: refuses when there is no [Unreleased] to rename"
D="$TMPROOT/f"; build_fixture "$D"
perl -i -pe 's/^## \[Unreleased\]$/## [5.0.2] - 2026-10-25/' "$D/CHANGELOG.md"
run_cut "$D" changelog 5.0.1 >/dev/null 2>&1; rc=$?
check "exit 1 with no [Unreleased]" "$rc" "1"

echo
echo "==> verify <version>: requires the CHANGELOG section AND its link"
D="$TMPROOT/g"; build_fixture "$D"
run_cut "$D" bump 5.0.1 --date 2026-10-20 >/dev/null 2>&1
out=$(run_cut "$D" verify 5.0.1); rc=$?
check "exit 1 before the changelog step" "$rc" "1"
run_cut "$D" changelog 5.0.1 --date 2026-10-20 >/dev/null 2>&1
perl -i -pe 's/^# State of Features — v5\.0\.0$/# State of Features — v5.0.0/' "$D/docs/STATE-OF-FEATURES.md"
out=$(run_cut "$D" verify 5.0.1); rc=$?
check "exit 1 while STATE-OF-FEATURES still names v5.0.0" "$rc" "1"
perl -i -pe 's/^# State of Features — v5\.0\.0$/# State of Features — v5.0.1/' "$D/docs/STATE-OF-FEATURES.md"
run_cut "$D" verify 5.0.1 >/dev/null 2>&1; rc=$?
check "exit 0 once every file names 5.0.1" "$rc" "0"

echo
echo "==> all: bump + changelog reaches a verified cut"
D="$TMPROOT/h"; build_fixture "$D"
run_cut "$D" all 5.0.1 --date 2026-10-20 >/dev/null 2>&1
perl -i -pe 's/^# State of Features — v5\.0\.0$/# State of Features — v5.0.1/' "$D/docs/STATE-OF-FEATURES.md"
run_cut "$D" verify 5.0.1 >/dev/null 2>&1; rc=$?
check "verified cut passes end to end" "$rc" "0"

echo
echo "Summary: $PASS passed, $FAIL failed"
[ "$FAIL" = 0 ]