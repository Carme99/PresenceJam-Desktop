#!/usr/bin/env bash
#
# scripts/release/cut.sh — automate the mechanical part of a release cut.
#
# This script automates docs/RELEASING.md §1 (six version literals) and §2
# (CHANGELOG header + link definitions). It deliberately does NOT automate the
# editorial parts: the release-notes prose, the STATE-OF-FEATURES row flips,
# and the decision of which version to cut all stay human. Those are judgement
# calls; the things this script does are not, and each one is a place where a
# forgotten edit either fails CI or, worse, ships a bad artifact.
#
# Why a script and not release-please (issue #1152): the two features that make
# release-please worth adopting here are `skip-github-release` (release.yml
# already owns publishing) and `skip-changelog` (ours is hand-written). Disable
# both and the remaining job is "bump six literals" — which this does in one
# file, instead of a manifest, a config, and a custom plugin for the one thing
# a version-string tool cannot do (prepending a <release> element to the
# AppStream metainfo).
#
# Usage:
#   cut.sh verify              Check the six literals agree with each other.
#   cut.sh verify <version>    Also require them to equal <version>, plus the
#                              metainfo entry, CHANGELOG section + link, and
#                              the STATE-OF-FEATURES header.
#   cut.sh bump <version>      Rewrite the six literals + prepend metainfo.
#   cut.sh changelog <version> Rename [Unreleased], open a fresh one, fix links.
#   cut.sh all <version>       bump + changelog (the full mechanical cut).
#
# Options:
#   --repo-root DIR   Repository root (default: derived from this script).
#   --date YYYY-MM-DD Release date for the metainfo and CHANGELOG headers.
#   --prev VERSION    Previous released version for the new CHANGELOG compare
#                     link. Default: read from the [Unreleased] definition.
#
# Exit codes: 0 ok, 1 verification failed or bad usage, 2 refused.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
TARGET_VERSION=""
TODAY="$(date -u +%F)"
PREV_VERSION=""

BASE_URL="https://github.com/Carme99/PresenceJam-Desktop"
METAINFO_REL="src-tauri/linux/com.presencejam.app.metainfo.xml"
CHANGELOG_REL="CHANGELOG.md"
SOF_REL="docs/STATE-OF-FEATURES.md"

die()    { printf 'error: %s\n' "$*" >&2; exit 1; }
refuse() { printf 'refused: %s\n' "$*" >&2; exit 2; }
note()   { printf '  %-8s %s\n' "$1" "$2"; }

usage() { sed -n '2,${/^#/!q;s/^# \{0,1\}//p;}' "${BASH_SOURCE[0]}"; }

# The command word is consumed FIRST. It used to be swept up by the generic
# argument parser below, which meant `cut.sh verify` ran the help text and
# exited 0 — a silent no-op on the one command meant to be the safety net.
command="${1:-}"
case "$command" in
  verify|bump|changelog|all) shift ;;
  ""|-h|--help) usage; exit 0 ;;
  *) die "unknown command: $command (expected: verify | bump | changelog | all)" ;;
esac

while [ $# -gt 0 ]; do
  case "$1" in
    --repo-root) REPO_ROOT="$2"; shift 2 ;;
    --date)      TODAY="$2"; shift 2 ;;
    --prev)      PREV_VERSION="$2"; shift 2 ;;
    -h|--help)   usage; exit 0 ;;
    -*)          die "unknown option: $1" ;;
    *)
      if [ -n "$TARGET_VERSION" ]; then die "unexpected extra argument: $1"; fi
      TARGET_VERSION="$1"; shift ;;
  esac
done

cd "$REPO_ROOT"
[ -f package.json ] || die "not a repository root (no package.json): $REPO_ROOT"
command -v jq    >/dev/null 2>&1 || die "jq is required but not on PATH"
command -v perl >/dev/null 2>&1 || die "perl is required but not on PATH"

# ------------------------------------------------------------ reading literals

pkg_json_version()      { jq -r '.version' package.json; }
pkg_lock_top_version()  { jq -r '.version' package-lock.json; }
pkg_lock_root_version() { jq -r '.packages[""].version' package-lock.json; }
tauri_conf_version()    { jq -r '.version' src-tauri/tauri.conf.json; }
cargo_toml_version()    { sed -nE 's/^version = "(.*)"/\1/p' src-tauri/Cargo.toml | head -1; }

cargo_lock_version() {
  perl -0777 -ne 'print "$1\n" if /\[\[package\]\]\s*\nname = "presence-jam"\s*\nversion = "([^"]+)"/' \
    src-tauri/Cargo.lock
}

metainfo_version() {
  grep -o -m1 '<release version="[^"]*"' "$METAINFO_REL" \
    | head -1 | sed -E 's/.*version="([^"]*)".*/\1/'
}

valid_semver() {
  printf '%s' "$1" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$'
}

# Count how many lines match an anchored pattern; used to refuse an edit whose
# anchor is ambiguous rather than silently rewriting the wrong line.
count_lines() { grep -c -- "$1" "$2" || true; }

require_single() {
  local pattern="$1" file="$2" label="$3" n
  n=$(count_lines "$pattern" "$file")
  [ "$n" = "1" ] || die "$label: expected exactly 1 line matching /$pattern/, found $n"
}

# ------------------------------------------------------------------- mutations
#
# Values reach perl through the environment (PJV), never by splicing a shell
# variable into a perl program string. Splicing forces quote-escaping inside a
# quote-delimited language inside a shell string, which is how this script's
# first draft ended up unparseable.

set_pkg_json() {
  require_single '^  "version": ' package.json 'package.json'
  PJV="$1" perl -i -pe 's/^(  "version": ")[^"]*(")/${1}$ENV{PJV}${2}/' package.json
  [ "$(pkg_json_version)" = "$1" ] || die "package.json did not take the new version"
}

set_pkg_lock() {
  # THE TRAP (docs/RELEASING.md §1): every node_modules/* entry also puts
  # "version" at six-space indent — 222 of them at the current cut. The top-level
  # literal is the only two-space one; packages[""] is anchored on the
  # packages -> "" block and can never reach a dependency.
  require_single '^  "version": ' package-lock.json 'package-lock.json (top-level)'

  PJV="$1" perl -0777 -i -pe 's/^(\{\s*\n(?:[^{}"]|"[^"]*")*?^\s*"version": ")[^"]*(")/${1}$ENV{PJV}${2}/m' \
    package-lock.json

  PJV="$1" perl -0777 -i -pe 's/("packages"\s*:\s*\{\s*""\s*:\s*\{(?:[^{}"]|"[^"]*")*?"version"\s*:\s*")[^"]*(")/${1}$ENV{PJV}${2}/s' \
    package-lock.json

  local top root
  top=$(pkg_lock_top_version)
  root=$(pkg_lock_root_version)
  [ "$top"  = "$1" ] || die "package-lock.json top-level is '$top', wanted '$1'"
  [ "$root" = "$1" ] || die "package-lock.json packages[\"\"] is '$root', wanted '$1' — the anchor matched a dependency instead"
}

set_tauri_conf() {
  require_single '^  "version": ' src-tauri/tauri.conf.json 'src-tauri/tauri.conf.json'
  PJV="$1" perl -i -pe 's/^(  "version": ")[^"]*(")/${1}$ENV{PJV}${2}/' src-tauri/tauri.conf.json
  [ "$(tauri_conf_version)" = "$1" ] || die "tauri.conf.json did not take the new version"
}

set_cargo_toml() {
  # Exactly one anchored '^version = ' line exists; dependency specs are inline
  # `{ version = "…" }` and never match.
  require_single '^version = ' src-tauri/Cargo.toml 'src-tauri/Cargo.toml'
  PJV="$1" perl -i -pe 's/^(version = ")[^"]*(")/${1}$ENV{PJV}${2}/' src-tauri/Cargo.toml
  [ "$(cargo_toml_version)" = "$1" ] || die "Cargo.toml did not take the new version"
}

set_cargo_lock() {
  require_single '^name = "presence-jam"$' src-tauri/Cargo.lock 'src-tauri/Cargo.lock'
  PJV="$1" perl -0777 -i -pe 's/(\[\[package\]\]\s*\nname = "presence-jam"\s*\nversion = ")[^"]*(")/${1}$ENV{PJV}${2}/' \
    src-tauri/Cargo.lock
  local got
  got=$(cargo_lock_version)
  [ "$got" = "$1" ] || die "Cargo.lock presence-jam version is '$got', wanted '$1'"
}

# Prepend a <release> entry. This is the one operation no version-string tool
# performs: it INSERTS a new element rather than replacing a value, because the
# metainfo is an accumulating history and release.yml asserts the FIRST entry
# names the tag being built.
prepend_metainfo() {
  [ -f "$METAINFO_REL" ] || die "missing $METAINFO_REL"
  if [ "$(metainfo_version)" = "$1" ]; then
    note skip "$METAINFO_REL already has a <release version=\"$1\"> entry"
    return 0
  fi
  grep -q '<releases>' "$METAINFO_REL" || die "$METAINFO_REL has no <releases> block"

  # Build the entry in the shell, then hand it to perl whole. Interpolating XML
  # into perl source invites brace-quoting bugs for no benefit.
  #
  # The trailing "\n" is re-added in perl rather than trusted to the here-doc:
  # command substitution strips trailing newlines, which would glue the new
  # </release> onto the next <release> line.
  local entry
  entry=$(printf '    <release version="%s" date="%s">\n      <description>\n        <p>\n          TODO: replace with the release summary before tagging.\n        </p>\n      </description>\n    </release>' "$1" "$2")

  PJENTRY="$entry" perl -0777 -i -pe 's{(<releases>\s*\n\s*<!--[^>]*-->\s*\n)}{${1}$ENV{PJENTRY}\n}' \
    "$METAINFO_REL"

  local got
  got=$(metainfo_version)
  [ "$got" = "$1" ] || die "$METAINFO_REL: newest <release> is '$got', wanted '$1' (prepend failed)"
}

# Rename [Unreleased] -> [X.Y.Z] - DATE, open a fresh one, and rewrite both link
# definitions. Rename and definition happen in one pass by construction, which
# removes the documented #437/#438 trap where CI run #423 failed on a rename
# whose link definition was forgotten.
rename_changelog_section() {
  [ -f "$CHANGELOG_REL" ] || die "missing $CHANGELOG_REL"
  grep -q '^## \[Unreleased\]$' "$CHANGELOG_REL" \
    || die "$CHANGELOG_REL: no '## [Unreleased]' section to rename"

  # The fresh block is built in the shell, so its newlines and indentation are
  # exact and never inherit perl source formatting.
  local fresh
  fresh=$(printf '## [Unreleased]\n\nEvery change merged after %s lands here. At the next cut this section is\nrenamed to the released version and a fresh empty one is opened on top (see\n`docs/RELEASING.md` §2).\n\n' "$1")

  # 1. rename the existing [Unreleased] header (single line in, single line out)
  PJV="$1" PJD="$2" perl -0777 -i -pe 's{^## \[Unreleased\]$}{## [$ENV{PJV}] - $ENV{PJD}}m' "$CHANGELOG_REL"

  # 2. insert the fresh section. Zero-width lookahead, so nothing is consumed:
  # an earlier draft matched '^## \[' and restored only '## [', silently
  # swallowing the version that followed it. The two newlines are re-added in
  # perl because $( ) strips trailing newlines off $fresh.
  PJFRESH="$fresh" perl -0777 -i -pe 's{^(?=## \[)}{$ENV{PJFRESH}\n\n}m' "$CHANGELOG_REL"

  # 3. the new section gets its compare link; [Unreleased] re-bases onto the new tag
  PJV="$1" PJP="$3" PJB="$BASE_URL" perl -0777 -i -pe 's{^\[Unreleased\]: .*$}{[$ENV{PJV}]: $ENV{PJB}/compare/v$ENV{PJP}...v$ENV{PJV}\n[Unreleased]: $ENV{PJB}/compare/v$ENV{PJV}...HEAD}m' "$CHANGELOG_REL"

  grep -q "^## \[$1\] - $2\$" "$CHANGELOG_REL" \
    || die "$CHANGELOG_REL: released section '## [$1] - $2' not found after rename"
  grep -q "^\[$1\]: " "$CHANGELOG_REL" \
    || die "$CHANGELOG_REL: link definition '[$1]:' missing"
  grep -q "^\[Unreleased\]: $BASE_URL/compare/v$1\.\.\.HEAD\$" "$CHANGELOG_REL" \
    || die "$CHANGELOG_REL: [Unreleased] definition was not re-based onto v$1"
}

# --------------------------------------------------------------------- commands

derive_prev_version() {
  local line
  line=$(grep -m1 '^\[Unreleased\]: ' "$CHANGELOG_REL" || true)
  [ -n "$line" ] || return 0
  printf '%s\n' "$line" | sed -E 's#.*/compare/v##; s#\.\.\.HEAD$##'
}

cmd_verify() {
  local -a names=(
    "package.json"
    "package-lock.json (top-level)"
    "package-lock.json (packages[\"\"])"
    "src-tauri/tauri.conf.json"
    "src-tauri/Cargo.toml"
    "src-tauri/Cargo.lock"
  )
  local -a values=(
    "$(pkg_json_version)"
    "$(pkg_lock_top_version)"
    "$(pkg_lock_root_version)"
    "$(tauri_conf_version)"
    "$(cargo_toml_version)"
    "$(cargo_lock_version)"
  )

  printf 'version literals\n'
  local base="${values[0]}" agree=1 i
  for i in "${!values[@]}"; do
    printf '  %-34s %s\n' "${names[$i]}" "${values[$i]}"
    [ "${values[$i]}" = "$base" ] || agree=0
  done
  local mv; mv=$(metainfo_version)
  printf '  %-34s %s\n' "$METAINFO_REL (newest)" "$mv"
  [ "$mv" = "$base" ] || agree=0

  [ "$agree" = "1" ] || die "version literals disagree — a cut bumps all six at once"

  if [ -z "$TARGET_VERSION" ]; then
    printf '\nok: all six literals and the metainfo agree at %s\n' "$base"
    return 0
  fi

  [ "$base" = "$TARGET_VERSION" ] || die "literals are at '$base', expected '$TARGET_VERSION'"
  grep -q "^## \[$TARGET_VERSION\] - " "$CHANGELOG_REL" \
    || die "$CHANGELOG_REL: no '## [$TARGET_VERSION] - <date>' section (the cut renames [Unreleased] in the same commit)"
  grep -q "^\[$TARGET_VERSION\]: " "$CHANGELOG_REL" \
    || die "$CHANGELOG_REL: link definition '[$TARGET_VERSION]:' missing (issue #437/#438)"

  if [ -f "$SOF_REL" ]; then
    local sof
    sof=$(grep -m1 '^# State of Features' "$SOF_REL" || true)
    printf '  %-34s %s\n' "STATE-OF-FEATURES header" "${sof:-<missing>}"
    case "$sof" in
      *"v$TARGET_VERSION"*) ;;
      *) die "$SOF_REL header is '${sof:-<missing>}', expected it to name v$TARGET_VERSION" ;;
    esac
  fi

  printf '\nok: cut %s is complete and consistent\n' "$TARGET_VERSION"
}

cmd_bump() {
  [ -n "$TARGET_VERSION" ] || die "bump needs a version (e.g. cut.sh bump 5.0.1)"
  valid_semver "$TARGET_VERSION" || refuse "'$TARGET_VERSION' is not a valid semver triple"
  printf 'bumping six literals to %s\n' "$TARGET_VERSION"
  set_pkg_json   "$TARGET_VERSION"; note ok "package.json"
  set_pkg_lock   "$TARGET_VERSION"; note ok "package-lock.json (both literals)"
  set_tauri_conf "$TARGET_VERSION"; note ok "src-tauri/tauri.conf.json"
  set_cargo_toml "$TARGET_VERSION"; note ok "src-tauri/Cargo.toml"
  set_cargo_lock "$TARGET_VERSION"; note ok "src-tauri/Cargo.lock"
  prepend_metainfo "$TARGET_VERSION" "$TODAY"; note ok "$METAINFO_REL (prepended)"
  printf '\nnext: review the metainfo <description> TODO, then\n      cut.sh changelog %s\n' "$TARGET_VERSION"
}

cmd_changelog() {
  [ -n "$TARGET_VERSION" ] || die "changelog needs a version (e.g. cut.sh changelog 5.0.1)"
  valid_semver "$TARGET_VERSION" || refuse "'$TARGET_VERSION' is not a valid semver triple"
  local prev="$PREV_VERSION"
  [ -n "$prev" ] || prev=$(derive_prev_version)
  [ -n "$prev" ] || refuse "could not derive the previous version; pass --prev X.Y.Z"
  [ "$prev" != "$TARGET_VERSION" ] || refuse "previous version equals the new version ($prev)"
  printf 'changelog: [Unreleased] -> [%s] - %s (previous v%s)\n' "$TARGET_VERSION" "$TODAY" "$prev"
  PJB="$BASE_URL" rename_changelog_section "$TARGET_VERSION" "$TODAY" "$prev"
  note ok "renamed section, opened a fresh [Unreleased], rewrote both definitions"
}

cmd_all() { cmd_bump; printf '\n'; cmd_changelog; }

# ------------------------------------------------------------------------ main

case "$command" in
  verify)    cmd_verify "$@" ;;
  bump)      cmd_bump "$@" ;;
  changelog) cmd_changelog "$@" ;;
  all)       cmd_all "$@" ;;
esac