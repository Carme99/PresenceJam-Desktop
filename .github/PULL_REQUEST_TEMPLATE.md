## Description

[Describe what this PR does and why]

## Type of Change

- [ ] Bug fix
- [ ] New feature
- [ ] Documentation update
- [ ] Refactoring
- [ ] Other: [description]

## Testing

[Describe how you tested this change — e.g. ran the app, checked logs, tested specific auth flows]

## Gate evidence

[Paste the exact commands you ran and their final status — the gate lines in
this order, plus test counts:

```
cd src-tauri && cargo fmt --all --check && cargo check --all-targets && cargo clippy --all-targets -- -D warnings && cargo test --all-targets
cd .. && npm run build && npm run check && npm test
cd src-tauri && cargo deny check licenses bans sources
```

Example: `cargo test --all-targets` → 412 passed, 0 failed; `npm test` → 68
passed. "CI will tell us" is not evidence.]

## Runtime smoke

[What you actually launched and what you observed — the binary/app, the exact
steps, and the result (logs, screenshots, output). If a runtime check was not
possible, write `not run — <reason>`; never claim a check you did not perform.]

## Screenshots

[PLACEHOLDER: Add screenshot here if your change affects the UI]

## Checklist

- [ ] Code compiles (`cargo check` passes)
- [ ] Code is formatted (`cargo fmt --all --check` passes)
- [ ] TypeScript type-checks (`npm run check` passes)
- [ ] Commit messages follow [conventional commits](https://www.conventionalcommits.org/) format
- [ ] PR title matches commit format (e.g. `feat: add dark mode`)

## Changelog

- [ ] I have added an entry to [CHANGELOG.md](../CHANGELOG.md) under `[Unreleased]`
- [ ] This change does not require a changelog entry (e.g. typo fix in docs)

## Additional Context

[Any other relevant information — related issues, notes for reviewers, etc.]

## Reviewer verdict

[Reviewer only: score /100 against the change's acceptance criteria, plus every
gap you found by name. Merge requires ≥ 95 with all named gaps fixed on this
branch.]
