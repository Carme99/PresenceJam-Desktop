#!/usr/bin/env python3
"""Static guards over .github/workflows/release.yml (#830, #925, #828).

`release.yml` runs only on a tag or a manual re-cut, so a defect in it surfaces
after the release is due, when there is no time left to fix it. These checks run
at PR time from the `release-workflow-guards` job in ci.yml instead.

They are deliberately text-based: the runner image is not assumed to ship `yq`,
and the file's shapes are the ones its own indentation already declares
(two spaces per level, jobs at two, job keys at four, steps at six).

Usage, from the repository root:

    python3 .github/scripts/release-workflow-guards.py            # every check
    python3 .github/scripts/release-workflow-guards.py timeouts   # one check

Each check prints `ok: <name>` or one `::error::` line per problem and the
process exits 1 when any check failed.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = ROOT / ".github" / "workflows" / "release.yml"

JOB = re.compile(r"^  ([A-Za-z0-9_-]+):\s*$")
STEP = re.compile(r"^      - ")
# The one job that legitimately inherits the workflow's `contents: write` grant:
# it creates the Release and uploads the assets.
PRIVILEGED_JOB = "release"
# The updater's trust anchor (#828). `npx tauri build` compiles the whole
# dependency tree, so anything that runs during the build can read these.
SECRETS = ("TAURI_SIGNING_PRIVATE_KEY", "TAURI_SIGNING_PRIVATE_KEY_PASSWORD")


def workflow_lines():
    return WORKFLOW.read_text(encoding="utf-8").splitlines()


def jobs(text):
    """[(name, header_line, [(line_number, text), ...])] under the `jobs:` key."""
    start = next((i for i, line in enumerate(text) if line.rstrip() == "jobs:"), None)
    if start is None:
        raise SystemExit(f"{WORKFLOW}: no `jobs:` mapping found")

    found = []
    for i in range(start + 1, len(text)):
        match = JOB.match(text[i])
        if match:
            found.append((match.group(1), i + 1, []))
        elif found:
            found[-1][2].append((i + 1, text[i]))
    if not found:
        raise SystemExit(f"{WORKFLOW}: no jobs found under `jobs:`")
    return found


def env_names(block, indent):
    """Keys of an `env:` mapping declared at `indent` spaces inside `block`."""
    prefix = " " * indent + "env:"
    for index, (_, line) in enumerate(block):
        if not line.startswith(prefix):
            continue
        # `env: {A: b}` — keys on the same line.
        names = set(re.findall(r"([A-Za-z_][A-Za-z0-9_]*)\s*:", line[len(prefix):]))
        key_indent = indent + 2
        for _, follow in block[index + 1:]:
            if not follow.strip():
                continue
            spaces = len(follow) - len(follow.lstrip(" "))
            if spaces < key_indent:
                break
            if spaces == key_indent:
                names.add(follow.strip().split(":", 1)[0])
        return names
    return set()


def steps(block):
    """[(first_line, [(line_number, text), ...])] for every step in a job block."""
    found = []
    for entry in block:
        if STEP.match(entry[1]):
            found.append((entry[0], []))
        if found:
            found[-1][1].append(entry)
    return found


def check_timeouts(workflow):
    """#830: every job is bounded, so a hang fails instead of burning 6 hours."""
    problems = []
    for name, header, block in jobs(workflow):
        if not any(re.match(r"^    timeout-minutes: \d+\s*$", line) for _, line in block):
            problems.append(f"job `{name}` (line {header}) declares no `timeout-minutes:`")
    return problems


def check_permissions(workflow):
    """#925: no job inherits the workflow-level `contents: write` by accident."""
    problems = []
    for name, header, block in jobs(workflow):
        if name == PRIVILEGED_JOB:
            continue
        if not any(re.match(r"^    permissions:", line) for _, line in block):
            problems.append(
                f"job `{name}` (line {header}) declares no `permissions:` block, so it "
                f"inherits the workflow-level grant (only `{PRIVILEGED_JOB}` may)"
            )
    return problems


def check_signing_env(workflow):
    """#828: the updater key stays out of every `npx tauri build` environment."""
    problems = []
    for name, header, block in jobs(workflow):
        job_env = env_names(block, 4)
        for step_line, step in steps(block):
            text = "\n".join(line for _, line in step)
            if "npx tauri build" not in text:
                continue
            leaked = (job_env | env_names(step, 8)) & set(SECRETS)
            if leaked:
                problems.append(
                    f"job `{name}`, step at line {step_line} runs `npx tauri build` with "
                    f"{', '.join(sorted(leaked))} in its environment"
                )
    return problems


CHECKS = {
    "timeouts": check_timeouts,
    "permissions": check_permissions,
    "signing-env": check_signing_env,
}


def main(argv):
    wanted = argv[1:] or ["all"]
    if wanted == ["all"]:
        wanted = list(CHECKS)
    unknown = [name for name in wanted if name not in CHECKS]
    if unknown:
        raise SystemExit(
            f"unknown check(s): {', '.join(unknown)} — expected one of {', '.join(CHECKS)}"
        )

    text = workflow_lines()
    failed = False
    for name in wanted:
        problems = CHECKS[name](text)
        if problems:
            failed = True
            for problem in problems:
                print(f"::error::{name}: {problem}")
        else:
            print(f"ok: {name}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
