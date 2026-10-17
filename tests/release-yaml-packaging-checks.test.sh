#!/usr/bin/env bash
# tests/release-yaml-packaging-checks.test.sh
#
# Regression tests for the SIGPIPE-on-pipefail bug fixed in
# `.github/workflows/release.yml`, "Package Linux artifacts" step.
#
# The original pipeline (e.g. `dpkg-deb --fsys-tarfile X.deb | tar -t |
# grep -Fqx ./usr/share/metainfo/...`) combined with `set -euo pipefail`
# at the top of the step was wrong: once `grep -Fqx` matched the
# metainfo path it early-exits 0 and closed its stdin; the upstream
# `tar` (or `cpio`) was killed with SIGPIPE on its next write of a
# later path and exited 141; pipefail surfaced 141; `if !` flipped
# that to 0 — so the gate passed exactly when the metainfo was
# present and the listing had any later entries (the common case).
#
# The fix buffers each listing into a temp file via shell redirection,
# then runs `grep -Fqx` against that file. The upstream tool no longer
# has a downstream pipe to be killed by, so SIGPIPE is impossible and
# grep's exit code reflects ONLY whether the metainfo is in the
# listing.
#
# These tests exercise the FIXED shell shape with `cat` as the upstream
# stand-in. The bug is in the pipeline shape, not in dpkg-deb / tar /
# rpm2cpio / cpio; the upstream tool is interchangeable from the test's
# point of view. Each fixture mimics the `tar -t` / `cpio -t` output
# style (paths prefixed with `./`).

set -euo pipefail

PASS=0
FAIL=0
TMPDIR_TEST="$(mktemp -d)"
trap 'rm -rf "$TMPDIR_TEST"' EXIT

PATTERN="./usr/share/metainfo/com.presencejam.app.metainfo.xml"

# Build a synthetic listing that mimics `tar -t` / `cpio -t` output.
# The metainfo line uses the `./` prefix that those tools produce.
# Many additional entries ensure that, under the OLD pipeline shape,
# `grep -Fqx` would have early-exited and SIGPIPE'd the upstream.
build_listing() {
  local f="$1" with_metainfo="$2"
  : > "$f"
  {
    echo "./"
    echo "./usr/"
    if [ "$with_metainfo" = "yes" ]; then
      echo "./usr/share/metainfo/com.presencejam.app.metainfo.xml"
    fi
    echo "./usr/bin/presence-jam"
    for i in $(seq 1 1000); do
      echo "./usr/lib/junk/file_${i}.bin"
    done
  } >> "$f"
}

# Replicate the FIXED pattern from release.yml verbatim. The upstream
# in release.yml is `dpkg-deb --fsys-tarfile X.deb | tar -t` (or
# `rpm2cpio X.rpm | cpio -t --quiet`); in the test we use `cat` for
# brevity and because the bug is in the shell pipeline shape, not in
# the upstream tool.
fixed_check() {
  local listing="$1" pattern="$2"
  local buf
  buf="$(mktemp)"
  trap "rm -f '$buf'" RETURN
  cat "$listing" > "$buf"
  grep -Fqx "$pattern" "$buf"
}

# Capture the exit code of a command without tripping `set -e`.
capture_exit() {
  set +e
  "$@"
  local rc=$?
  set -e
  echo "$rc"
}

# ---- fixtures ----

WITH="$TMPDIR_TEST/with.txt"
WITHOUT="$TMPDIR_TEST/without.txt"
EMPTY="$TMPDIR_TEST/empty.txt"
: > "$EMPTY"
build_listing "$WITH"    "yes"
build_listing "$WITHOUT" "no"

# ---- cases ----

echo "==> fixed_check: metainfo present in listing"
rc=$(capture_exit fixed_check "$WITH" "$PATTERN")
if [ "$rc" = 0 ]; then
  echo "  ok    exit=0 (gate passes correctly)"
  PASS=$((PASS + 1))
else
  echo "  FAIL  expected 0, got $rc"
  FAIL=$((FAIL + 1))
fi

echo
echo "==> fixed_check: metainfo absent in listing"
rc=$(capture_exit fixed_check "$WITHOUT" "$PATTERN")
if [ "$rc" = 1 ]; then
  echo "  ok    exit=1 (gate fails correctly)"
  PASS=$((PASS + 1))
else
  echo "  FAIL  expected 1, got $rc"
  FAIL=$((FAIL + 1))
fi

echo
echo "==> fixed_check: empty listing"
rc=$(capture_exit fixed_check "$EMPTY" "$PATTERN")
if [ "$rc" = 1 ]; then
  echo "  ok    exit=1 (gate fails correctly on empty input)"
  PASS=$((PASS + 1))
else
  echo "  FAIL  expected 1, got $rc"
  FAIL=$((FAIL + 1))
fi

echo
echo "==> fixed_check: metainfo early in long listing (1000 trailing entries)"
echo "    Under the OLD pipeline shape, grep -Fqx would have early-exited"
echo "    on the metainfo match and SIGPIPE'd the upstream `cat`. The"
echo "    FIXED shape buffers to a temp file so SIGPIPE is impossible."
rc=$(capture_exit fixed_check "$WITH" "$PATTERN")
if [ "$rc" = 0 ]; then
  echo "  ok    exit=0 (no SIGPIPE-induced false-positive failure)"
  PASS=$((PASS + 1))
else
  echo "  FAIL  expected 0, got $rc"
  FAIL=$((FAIL + 1))
fi

echo
echo "==> release.yml cross-check: the fixed buffered shape is still in place"
# Refreshed after #1177 ("the .deb metainfo gate asserted a './' prefix tar
# never emits"). That fix kept the SIGPIPE fix -- listings are still
# buffered to a temp file -- but replaced the literal
# `grep -Fqx "./${METAINFO_PATH}"` with a `METAINFO_RE` that tolerates
# BOTH the `./`-prefixed and unprefixed forms tauri-bundler may emit. The
# cross-check below therefore asserts the CURRENT fixed shape:
#   1. each listing is redirected into its temp file (no grep in the pipe),
#   2. grep then runs `-Eq "$METAINFO_RE"` against the buffered file.
# It also asserts the ORIGINAL buggy shape (`listing | grep`) has not
# crept back, which is the actual regression this file exists to catch.
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
if [ -f "$REPO_ROOT/.github/workflows/release.yml" ]; then
  REL="$REPO_ROOT/.github/workflows/release.yml"
  cross_ok=1
  if grep -qF 'grep -Eq "$METAINFO_RE" "$DEB_LISTING"' "$REL" \
     && grep -qF 'grep -Eq "$METAINFO_RE" "$RPM_LISTING"' "$REL"; then
    echo "  ok    release.yml greps the buffered DEB_LISTING + RPM_LISTING"
    PASS=$((PASS + 1))
  else
    echo "  FAIL  release.yml does not grep METAINFO_RE against the buffered listings"
    cross_ok=0
    FAIL=$((FAIL + 1))
  fi

  # Each upstream tool must redirect into its temp file rather than pipe
  # straight into grep -- a pipe is what let SIGPIPE turn a match into a
  # false failure under `set -o pipefail`.
  if grep -qE 'tar -t > "\$DEB_LISTING"' "$REL" \
     && grep -qE 'rpm -qp --list .* > "\$RPM_LISTING"' "$REL"; then
    echo "  ok    release.yml buffers each listing to a temp file (no SIGPIPE path)"
    PASS=$((PASS + 1))
  else
    echo "  FAIL  release.yml does not buffer a listing into its temp file"
    cross_ok=0
    FAIL=$((FAIL + 1))
  fi

  # Regression guard: the buggy shape pipes an upstream straight into grep.
  # Full-line comments are stripped first -- release.yml deliberately
  # documents the OLD buggy shape in prose (the `#dpkg-deb ... | grep`
  # comment block), and those prose lines must not be mistaken for live code.
  if grep -vE '^[[:space:]]*#' "$REL" | grep -qE '\| *(grep|tar -t \| grep)'; then
    echo "  FAIL  release.yml pipes a listing into grep again (SIGPIPE regression)"
    FAIL=$((FAIL + 1))
  else
    echo "  ok    release.yml has no listing-into-grep pipe"
    PASS=$((PASS + 1))
  fi
else
  echo "  SKIP  release.yml not found at $REPO_ROOT/.github/workflows/release.yml"
fi

echo
echo "Summary: $PASS passed, $FAIL failed"
[ "$FAIL" = 0 ]
