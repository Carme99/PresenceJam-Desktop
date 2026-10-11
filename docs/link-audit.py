#!/usr/bin/env python3
"""Audit every relative markdown link — and every source citation — in the repo.

Checks, for every `*.md` file outside the build/vendor directories:

* each relative link target exists on disk (directories allowed), and
* each `#anchor` resolves against the target file's headings — including
  explicit `<a name="...">` / `<a id="...">` anchors, which plain
  heading-slug matching would report as dead, and
* each `` `src-tauri/src/**/*.rs` `` / `` `src/lib/**` `` citation in a
  **documentation** file names a file that exists on disk.

Absolute URLs, `mailto:` links and bare fragments of code fences are ignored.

The citation half exists because the #754/#755/#756/#757 module split left ~60
`path:line` citations in the docs pointing at files that no longer exist
(`poll_once.rs`, `config.rs`, `tray.rs`, `lib.rs`) and nothing in CI caught it.
Link targets and anchors were clean the whole time; only the citations drifted.

Usage (from anywhere):

    python3 docs/link-audit.py

Exit status is 1 when at least one link is broken or a citation is unresolvable,
0 otherwise.
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SKIP_DIRS = {".git", "node_modules", "target", "dist", "build", ".svelte-kit"}

LINK = re.compile(r"!?\[[^\]]*\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)")
EXPLICIT_ANCHOR = re.compile(r'<a\s+(?:name|id)="([^"]+)"')

def slug(text):
    """GitHub heading slug: lowercase, alphanumerics/hyphens/underscores kept."""
    text = text.strip().lower()
    kept = [ch for ch in text if ch.isalnum() or ch in "-_" or ch == " "]
    return "".join(kept).replace(" ", "-")


FENCE = re.compile(r"```.*?```|~~~.*?~~~", re.S)


def anchors_of(path):
    """Every anchor a browser can reach in one markdown file."""
    seen, anchors = {}, set()
    with open(path, encoding="utf-8") as fh:
        text = FENCE.sub("", fh.read())
    for line in text.splitlines():
            for explicit in EXPLICIT_ANCHOR.findall(line):
                anchors.add(explicit.lower())
            heading = re.match(r"^#{1,6}\s+(.*?)\s*$", line)
            if not heading:
                continue
            base = slug(heading.group(1))
            count = seen.get(base, 0)
            seen[base] = count + 1
            anchors.add(base if count == 0 else f"{base}-{count}")
    return anchors


DOC_ROOTS = (
    "README.md", "SETUP.md", "USAGE.md", "TROUBLESHOOTING.md", "SECURITY.md",
    "ACKNOWLEDGEMENTS.md", "CONTRIBUTING.md", "ARCHITECTURE.md", "AGENTS.md",
    "CLAUDE.md",
)
DOC_DIRS = ("docs",)
# `docs/audit/` holds audit reports that quote now-dead paths *as the evidence*
# for the defects they describe, so they are excluded from the citation sweep
# (their links are still checked above).
CITATION_SKIP = ("docs/audit/",)
CITATION = re.compile(r"`(src-tauri/src/[A-Za-z0-9_./-]+\.rs|src/lib/[A-Za-z0-9_./-]+\.(?:svelte\.ts|ts|svelte))`")


def citation_problems():
    """Documentation citations naming a source file that does not exist.

    Only the curated documentation set is checked — `examples/` fixtures and
    historical changelog rows legitimately quote paths that have since been
    refactored away.
    """
    problems = []
    for path in markdown_files():
        rel = os.path.relpath(path, ROOT)
        if rel.startswith(CITATION_SKIP):
            continue
        if not (rel in DOC_ROOTS or rel.startswith(DOC_DIRS)):
            continue
        with open(path, encoding="utf-8") as fh:
            for lineno, line in enumerate(fh, 1):
                for cited in CITATION.findall(line):
                    if not os.path.exists(os.path.join(ROOT, cited)):
                        problems.append(f"{rel}:{lineno}: unresolvable citation {cited}")
    return problems


def markdown_files():
    found = []
    for dirpath, dirnames, filenames in os.walk(ROOT):
        dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
        found.extend(
            os.path.join(dirpath, fn) for fn in filenames if fn.endswith(".md")
        )
    return sorted(found)


def main():
    files = markdown_files()
    checked = with_anchor = 0
    problems = []

    for path in files:
        rel = os.path.relpath(path, ROOT)
        with open(path, encoding="utf-8") as fh:
            text = re.sub(r"```.*?```", "", fh.read(), flags=re.S)
        for raw in LINK.findall(text):
            if raw.startswith(("http://", "https://", "mailto:")):
                continue
            checked += 1
            target, _, anchor = raw.partition("#")
            if target:
                resolved = os.path.normpath(
                    os.path.join(os.path.dirname(path), target)
                )
                if not os.path.exists(resolved):
                    problems.append(f"{rel}: missing target {raw}")
                    continue
            else:
                resolved = path
            if anchor:
                with_anchor += 1
                if resolved.endswith(".md") and anchor.lower() not in anchors_of(resolved):
                    problems.append(f"{rel}: dead anchor {raw}")

    citations = citation_problems()
    problems.extend(citations)

    print(f"markdown files scanned : {len(files)}")
    print(f"relative links checked : {checked} ({with_anchor} with #anchors)")
    print(f"source citations       : {len(citations)} unresolvable")
    print(f"broken                 : {len(problems)}")
    for problem in problems:
        print("  BROKEN:", problem)
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
