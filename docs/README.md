# Documentation index

What to read for what. Everything here is versioned with the code it
describes; if a page contradicts the tree, the tree wins — open an issue.

## Start here

| If you want to… | Read |
| --- | --- |
| Install the app, register a Spotify Developer app, connect Teams | [`../SETUP.md`](../SETUP.md) |
| Use the app day to day — tray, dashboard, settings, status formats | [`../USAGE.md`](../USAGE.md) |
| Fix something that is not working | [`../TROUBLESHOOTING.md`](../TROUBLESHOOTING.md) |
| Check whether your OS and architecture is supported | [`PLATFORMS.md`](./PLATFORMS.md) |
| Know which features are shipped and verified, row by row | [`STATE-OF-FEATURES.md`](./STATE-OF-FEATURES.md) |
| Understand how it works under the hood | [`../ARCHITECTURE.md`](../ARCHITECTURE.md) (index) |
| Contribute code — dev setup, conventions, PR process | [`../CONTRIBUTING.md`](../CONTRIBUTING.md) |
| Cut a release | [`RELEASING.md`](./RELEASING.md) |
| Review the privacy, token-storage and network posture | [`../SECURITY.md`](../SECURITY.md) |
| See what changed in which version | [`../CHANGELOG.md`](../CHANGELOG.md) |
| Check which open-source projects this app depends on | [`../ACKNOWLEDGEMENTS.md`](../ACKNOWLEDGEMENTS.md) |

## Architecture

[`../ARCHITECTURE.md`](../ARCHITECTURE.md) is a short index; the detail lives in
[`architecture/`](./architecture/):

| Page | What it covers |
| --- | --- |
| [`architecture/overview.md`](./architecture/overview.md) | What the app is, the system diagram, the CI/CD pipeline and the release process |
| [`architecture/polling.md`](./architecture/polling.md) | The sync thread, smart sleep, failure classification, presence gating, status text, the profanity filter |
| [`architecture/auth-and-tokens.md`](./architecture/auth-and-tokens.md) | Spotify PKCE, Teams device-code flow, Graph presence, deep-link routing |
| [`architecture/storage-and-config.md`](./architecture/storage-and-config.md) | Config and token files, atomic writes, config integrity, startup, reconnect, process state |
| [`architecture/tray-and-shell.md`](./architecture/tray-and-shell.md) | Tray menu, detached windows, UI languages, the background updater |
| [`architecture/frontend.md`](./architecture/frontend.md) | Directory layout, in-app pages, event bus, notification throttle |

## Assets and history

| Path | What it is |
| --- | --- |
| [`screenshots/`](./screenshots/) | Screenshots embedded in `../README.md` and `../USAGE.md` |
| [`archive/`](./archive/) | Superseded planning docs, kept for history only — do not plan new work from them |
| [`../archive/reviews/`](../archive/reviews/) | Point-in-time audit/review reports |
| [`link-audit.py`](./link-audit.py) | Markdown link/anchor auditor — run `python3 docs/link-audit.py` from the repo root |

## Conventions

- User-facing counts, file paths and symbol names in a doc are claims about the
  tree: verify them against the current source before changing them.
- Relative links are audited across the repository by
  [`link-audit.py`](./link-audit.py): every relative target must exist and every
  `#anchor` must resolve against the target file's headings (including explicit
  `<a name="…">` anchors). Renaming a heading breaks inbound links — grep for
  the old anchor first.
- The `CHANGELOG.md` section headers and their `[X.Y.Z]:` link definitions are
  enforced by CI (`changelog-links`); see [`RELEASING.md`](./RELEASING.md).
