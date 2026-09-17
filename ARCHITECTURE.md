# Architecture

A deep-dive into how PresenceJam works under the hood.

This page is the index. The detail lives in six subject pages under
[`docs/architecture/`](./docs/architecture/), split out of this file so each
subject can be read (and reviewed) on its own:

| Page | What it covers |
| --- | --- |
| [Overview](./docs/architecture/overview.md) | What the app is, the system diagram, the CI/CD pipeline and the release process |
| [Polling loop](./docs/architecture/polling.md) | The single sync thread — intervals and smart sleep, failure classification, presence gating and status rules, status-text formatting, and the profanity filter |
| [Auth and tokens](./docs/architecture/auth-and-tokens.md) | Spotify PKCE, the Teams device-code flow, the Graph presence calls, and `presencejam://` deep-link routing |
| [Storage and config](./docs/architecture/storage-and-config.md) | `config.json` / `tokens.json` locations and atomic writes, config integrity, startup loading, reconnect flow, and process state |
| [Tray and shell](./docs/architecture/tray-and-shell.md) | The system tray menu, detached windows, UI languages, and the background auto-updater |
| [Frontend](./docs/architecture/frontend.md) | Directory layout, the diagnostics and log-viewer pages, the Rust→frontend event bus, and the notification throttle |

Other entry points: [docs/README.md](./docs/README.md) (documentation index),
[docs/RELEASING.md](./docs/RELEASING.md) (cutting a release) and
[docs/STATE-OF-FEATURES.md](./docs/STATE-OF-FEATURES.md) (what is shipped and
verified, row by row).

## Where a section went

Anchors that used to resolve against this file now resolve against the page
that owns the section:

| Anchor | Now lives in |
| --- | --- |
| `#overview`, `#system-diagram`, `#cicd-pipeline`, `#release-process` | [overview.md](./docs/architecture/overview.md) |
| `#polling-loop`, `#smart-sleep--pause-aware-backoff-pr-45`, `#failure-classification-46`, `#presence-gating--availability-sync-v30`, `#user-editable-config-surface-46-538`, `#profanity-filter`, `#status-formatting-46` | [polling.md](./docs/architecture/polling.md) |
| `#authentication-flows`, `#spotify-oauth-authorization-code--pkce-confidential-client`, `#microsoft-teams-device-code-flow`, `#teams-presence-apis-v30`, `#deep-link-routing`, `#routing-flow`, `#state-parameter-is-both-csrf-and-anti-hijack-binding`, `#per-launch-scheme-re-registration-further-mitigates-66` | [auth-and-tokens.md](./docs/architecture/auth-and-tokens.md) |
| `#config-integrity-v46`, `#startup-loading`, `#reconnect-flow`, `#commands`, `#state-management`, `#rust-state--appstate-struct-of-states-v275-refactor-pr-80`, `#frontend-state` | [storage-and-config.md](./docs/architecture/storage-and-config.md) |
| `#multi-window-detach-v40`, `#interface-languages-v40`, `#auto-update-v30`, `#system-tray-v46` | [tray-and-shell.md](./docs/architecture/tray-and-shell.md) |
| `#local-diagnostics-page-v40`, `#log-viewer-v46`, `#event-bus`, `#frontend-notification-throttle-c8`, `#directory-structure` | [frontend.md](./docs/architecture/frontend.md) |
