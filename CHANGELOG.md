# Changelog

All notable changes to PresenceJam are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- **Track rules can be scheduled, and quiet hours can stop polling (#672):** a
  track rule now carries its own weekday set and time window (empty weekdays =
  every day; the window supports wrap-around like quiet hours, and an end time
  of 00:00 means the end of the day), and the Settings card can reorder rules
  because array order is priority — the first matching rule wins. A quiet-hours
  entry can also carry **Stop polling during this window**: while it is active
  the app makes no Spotify or Teams request at all and touches no write clock,
  then resumes by itself when the window ends (the polling thread is never
  stopped or parked). The paused (`"🎵 Paused"`) and stopped (`"🎵 Nothing
  playing on Spotify"`) status texts are now user-editable in Settings
  (`teams.paused_status_format` / `teams.stopped_status_format`); their defaults
  render exactly as they did in 4.6, and clearing a field restores the default.
  All new fields are additive with serde defaults, so a pre-4.7 config loads
  unchanged.

- **Log rotation and config backup (#673):** the log file is now rotated at a
  configurable size (`logging.max_file_size_mb`, 1–500 MB, default 10) with a
  configurable number of *archived* log files kept (`logging.keep_files`,
  1–20, default 3 — the active log is kept in addition, so the directory holds
  at most `keep_files + 1`), wired into the `tauri-plugin-log` file target and
  editable in a new Settings → Logging card. The same release adds Settings →
  Backup: **Export settings** writes a copy of `config.json` (never the Spotify
  client secret, which is keychain-only) and **Import settings** replaces the
  stored settings from a file — asking first, refusing any document that carries
  a plaintext `client_secret`, clamping out-of-band values, and keeping the
  previous file as `config.json.bak`.

### Changed

### Fixed

- **Quit-time Teams cleanup was cancelled by the poller's own exit tail (#684):**
  `polling_loop` resets the write-decision clocks on its way out, and
  `RunEvent::Exit` runs `clear_presence_on_exit` *after* it — so a cleanup that
  decided from those clocks found two `None`s and returned early. Quitting
  mid-song therefore left the `🎵 …` status message and the armed `Available`
  session on Teams, which is the exact outcome #636 exists to prevent. The
  cleanup now reads a process-wide exit snapshot, written by every successful
  status write and presence arm, which neither the session-end reset nor a new
  session clears — Teams keeps showing that status across a stop/start. The
  snapshot also records the manual-status verdict observed on *every* presence
  read — the change-time gate, the paused clear and the due mid-track re-check
  all funnel through one recorder — so quitting never replaces a status message
  the *user* typed with the "Paused" placeholder (the shipped 4.6
  respect-the-manual-status behaviour) — while the app's own armed availability
  session is still cleared.
- **A gated clear was recorded as posted and could never be retried (#686,
  #687):** on the paused-track and no-track paths the byte-identity check ran
  *before* the gate verdict, and the suppressing branch then recorded the
  placeholder as posted although nothing was sent. A quiet window ending — or a
  meeting ending while the track stayed paused — re-ran the same dedup and
  skipped the clear for good. The verdict now comes first, a suppression is
  recorded as a suppression (never as a post), and the paused clear re-evaluates
  its recorded gate on the same re-check clock the playing branch uses. The
  no-track path also owns the gate state now: a gate recorded for a track is
  retired when that track ends (or when the clear is already showing), so
  `presence_gated` can no longer stay true — and the Dashboard chip keep saying
  "you're busy, in a call, or presenting" — above a "Nothing playing" card. A
  suppression with nothing playing is still reported as gated; it just carries
  the no-track marker instead of the finished track's key, and the two early
  returns (no Teams token, `clear_on_pause` off) retire it as well. The paused
  clear also keeps its pre-4.7 no-read fast path: while the placeholder Teams
  shows is the one the pause wants, no rule suppresses the clear and no recorded
  gate has reached its re-check, the outcome cannot change, so the steady pause
  no longer costs a Graph `/presence` GET per poll.
- **A poller that stopped itself never announced it (#688):** the five-strike
  auth exit (and every other thread exit that is not a `stop_syncing`) emitted
  nothing, so the Dashboard mirror stayed on "Syncing" and the tray on "Pause
  Sync" until a restart. The ownership-checked thread-exit point now emits the
  same `sync-stopped` payload the command does, and logs the exit reason —
  gated on whether a stop was actually *requested* (the stored stop sender being
  gone) rather than on `is_syncing`, which an explicit Stop leaves true until
  after the join (that gating emitted the event twice) and a self-terminating
  exit can leave false (that gating emitted nothing at all). Exactly one
  `sync-stopped` per stop, on both paths.
- **Pausing a track was invisible to everything downstream (#689):** `is_playing`
  is not part of the status change key and the stored track was only written on a
  track *change*, so pressing pause left the tray, the sync status and the
  Dashboard reporting the track as still playing. A playback-state change now
  re-stores the observed item and emits `playback-state-changed`.
- **A pause was reported as a stop (#690):** the paused-track clear emitted
  `presence-cleared`, which the Dashboard reads as "nothing is playing". Pauses
  now emit `presence-paused` with the posted status; `presence-cleared` is kept
  for the genuine no-track path.
- **A concurrent manual refresh could resurrect a pre-gate clock snapshot
  (#694):** `run_oneshot` loads the shared write-decision clocks once around its
  whole iteration while the loop loads per iteration, so a refresh that stored
  later wrote back a snapshot taken before the loop's gate decision — silently
  dropping `gated_track_key` and letting the next write through mid-meeting. The
  shared slot is now generation-checked: a snapshot whose generation was
  superseded is discarded and logged instead of resurrecting an old decision.
- **Presence state no longer dies with the Dashboard (#670, #547):** the
  `presence-updated` / `presence-cleared` / `presence-gated` /
  `presence-availability-updated` listeners (plus `presence-paused`,
  `playback-state-changed`, `sync-started` / `sync-stopped`) now live in the
  always-mounted `+layout.svelte` and write the shared store, so a status, gate
  or pause that lands while Settings/Logs/Diagnostics is on screen is no longer
  dropped with the destroyed Dashboard. A mounting Dashboard seeds the status
  preview, the gate chip and the track card from `get_sync_status`
  (`last_posted_status` / `presence_gated` / `presence_paused`, read from the
  poller's session clocks — never re-derived from a fresh API call), and a
  pause now keeps the track card in its paused state instead of reporting
  "Nothing playing".
- **Tray and log-pane hygiene (#671):** the tray's dedup key now includes the
  shuffle/repeat state, so a mode changed in another Spotify client repaints
  the check marks at the next poll instead of showing the wrong mode until an
  unrelated rebuild; the tray consumes the poller's
  `playback-state-changed` event, so a same-track pause moves the Play/Pause
  mark and the status line off "playing" without waiting for the next track;
  the Logs pane releases a `log://log` subscription that settles after it
  unmounts, instead of leaking one listener per visit; and a Teams session
  that could not be persisted (locked keychain, full disk) now surfaces as an
  amber banner in Settings naming the failure and the reconnect that retries
  it.
### Security

## [4.6.0] - 2026-09-16

The largest PresenceJam release since 4.0: a full repository audit produced 90
tracked findings, and the wave that fixed them reworked the config write path,
the reconnect flow, the token store, the polling loop, the tray remote, the
detached windows, the diagnostics snapshot, the release pipeline and the docs —
then added the features those same paths were missing.

### Added

- **Richer status placeholders (#580):** the status template now also substitutes
  `{device}`, `{playlist}` (alias `{context}`), `{progress}` and `{shuffle}` /
  `{repeat}` — every one of them fed by data the polling loop already receives on
  each 200 response, so no extra request, scope or failure mode. Substitution is
  a single pass over a token table, which fixes the old chained-replace
  re-expansion for templates that themselves contain a token.
- **Podcast and audiobook support (#581, #583):** episodes are no longer treated
  as "nothing playing". An episode renders through its own template
  (`🎙️ {show} - {episode}`, plus `{show}`, `{episode}`, `{publisher}`), the tray
  Up Next peek lists them, and the poll request now sends
  `additional_types=episode`. Adverts and unknown item types still map to
  "nothing playing", as before.
- **Tray Shuffle and Repeat toggles (#582):** both are real check-menu items
  driven by the `shuffle_state`/`repeat_state` already present in the poll
  response; Repeat cycles off → context → track and its label names the current
  mode. A non-Premium account or an inactive device leaves the menu showing the
  truth.
- **Rule-driven Teams availability (#633):** quiet hours and track rules can now
  *set* availability/activity instead of only suppressing the status (quiet hours
  take precedence, and a suppression-only rule still moves the presence bubble);
  a status the user set by hand is respected — the loop skips the Teams write
  while it is in force and the Dashboard chip says why; the Available session is
  bounded to real listening and cleared on quit; and the presence gate
  understands an out-of-office/away state, with the chip naming the reason
  instead of showing one generic string.
- **Settings controls for the rule and polling fields (#538):** quiet hours now
  carry a replacement status, the profanity filter takes extra words, and the
  paused-polling ceiling is configurable — each with clamp feedback that mirrors
  the Rust clamp.
- **Log viewer backfill (#595):** the Logs pane seeds itself from the on-disk
  log tail on mount instead of staying empty until the next live event. The read
  is bounded (500 lines / 256 KiB, read from the end of the file) and runs off
  the UI thread; a backfill that lands after the reader has scrolled away does
  not move the viewport.
- **Live update-staging progress and cancel (#590):** install-on-quit now
  streams throttled `update-stage-progress` events and can be cancelled, which
  releases the verified payload instead of pinning it in memory until the next
  quit.
- **Keychain state surface (#560):** a system keychain that is locked or missing
  is now reported as exactly that — "unlock it or install a Secret Service
  provider, your secret is still stored" — instead of being collapsed into "not
  configured", which used to send a fully-configured user back through first-run
  setup.
- **Config quarantine is visible (#537):** when a corrupt `config.json` is
  quarantined, the diagnostics snapshot and the UI now say so and name the
  backup, rather than silently reverting every setting to its default.
- **macOS deep-link re-claim (#628, #66):** the `presencejam://` scheme is
  re-registered as the default handler on every launch through CoreServices
  `LSSetDefaultHandlerForURLScheme` (the plugin returns `UnsupportedPlatform` on
  macOS), closing the scheme-hijack gap that PKCE alone had to cover.
- **i18n formatting and locale convergence (#616, #620):** plural and number
  formatting goes through `Intl.PluralRules`/`Intl.NumberFormat`, the language
  picker converges across webviews the way the theme already did, and
  `<html lang>` tracks the active locale.

### Changed

- **Config writes merge instead of replace (#535, #536, #542):** `update_config`
  applies a field-level patch, `save_config` is documented as the whole-document
  write, and `schema_version` is server-authoritative. The onboarding wizard now
  prefills from the stored config and patches only the fields it owns — a
  returning user routed back through setup no longer loses their quiet hours,
  track rules, profanity settings, poll bounds or logging level (#531).
- **Reconnect re-authorizes without destroying credentials (#554):** Settings'
  "Reconnect Spotify" used to run the disconnect command, deleting the keychain
  client secret and forcing full setup. The destructive path is now a separate
  `disconnect_spotify`, and the reconnect path clears only the session.
- **PKCE bindings are consumed only after a successful exchange (#555):** a
  transient exchange failure no longer burns the single-use launch binding, so
  the retry works.
- **Polling treats only dead credentials as dead (#568):** transient network and
  parse failures no longer count toward the exit that stops polling and opens a
  browser OAuth window; they back off (`min(300 s, 30 s · 2ⁿ)` with jitter) and
  warn. Quiet hours and track rules are now evaluated mid-track and on the
  paused-clear and no-track clear paths (#569, #570), the 429 backoff can never
  sleep below the server's `Retry-After` (#571), and a manual refresh shares the
  driver's write-decision clocks instead of bypassing them (#572).
- **Token store hardening (#561–#566):** one locked AES-key create, serialised
  token persists with a joint snapshot, a keychain tri-state that keeps a live
  session when the keychain merely cannot answer, persist failures that no
  longer discard a successful sign-in, a recoverable corrupt-key path, and the
  shared CAS refresh on both providers.
- **Detached windows behave (#594, #585):** the child capability grants
  `core:window:allow-close` so "Pop back in" works, and close-to-tray is now
  main-window only — a detached Logs/Settings pane is no longer hidden into an
  unreachable zombie.
- **Tray remote correctness (#586–#589, #591, #592):** playback and device
  actions use the refresh-aware token path (with one refresh + retry), the
  Show/Hide arm no longer performs blocking HTTP on the menu-event thread, the
  Pause/Resume label repaints from backend state, `--minimized` is honoured, and
  the tray derives its state from `AppState` rather than a frontend claim.
- **Dashboard and Settings state (#547–#551):** presence preview and the gate
  chip survive a view remount, the notification opt-in reaches a mounted
  Dashboard and a denied permission no longer renders the toggle as on, leaving
  Settings with unsaved edits prompts instead of discarding them, and the
  availability chip is localized and clears.
- **Diagnostics (#598, #602, #603):** `Authorization: Bearer <token>` is masked
  without a length threshold, absolute POSIX/Windows/UNC paths are stripped from
  the failed-update record, the support snapshot is written by the backend and
  reports its real outcome, and two collected fields are rendered.
- **Profanity filter (#578, #579):** scoped leet folds stop `Cox` and
  `Song (Uncut)` from flagging, and the strong-stem continuation list now
  catches `fuckboy`, `fuckface`, `fuckwad` and `shitpost` while `Fukushima`,
  `shitake`, `cocktail` and `Push It` stay clean.
- **Spotify client cost (#576, #577):** one HTTP client per process instead of
  one per request, and the steady-state 304 path no longer allocates a `String`
  it drops immediately.
- **Frontend listener lifecycle (#615):** `useAuthListeners` returns a
  synchronous disposer and tracks disposal itself, deleting five hand-rolled
  destroyed-flag dances (and giving Dashboard and `+page.svelte` one teardown
  helper for their raw `listen()` sites).

### Fixed (build, CI and release)

- **Tag/version consistency (#605):** the release workflow now fails when the
  pushed tag disagrees with `tauri.conf.json`/`package.json`/`Cargo.toml`, and
  the same three-way agreement runs on every PR — the mismatch that would make
  the updater re-offer one version forever can no longer ship.
- **Tagged commits are verified (#606):** the release pipeline runs
  `cargo fmt --check`, clippy `-D warnings`, `cargo test --all-targets`,
  `npm run check` and `npm test` before it builds, so a tag can no longer publish
  an unverified commit.
- **Frontend tests are a real gate (#607):** `npm test --if-present` is gone.
- **Node 24 (#608):** CI and release moved off the EOL Node 20, with `engines`
  aligned and `@types/node` tracked.
- **Job timeouts and packaging (#609, #610, #611, #612, #613):** the `rust` job's
  timeout is no longer below its own cold build, the Homebrew formula refuses
  Intel Macs (it installs an aarch64-only DMG), `withGlobalTauri` is off now that
  nothing consumes `window.__TAURI__`, dead plugin dependencies and duplicate
  capability grants are pruned, and the MSRV is declared.
- **Dependency advisories (#642):** `vitest`/`@vitest/mocker` bumped to the
  patched 4.1.11. The `glib` advisory is *not* clearable by a lockfile bump —
  `tauri 2.11.5` requires `gtk ^0.18` while glib's fix is 0.20.0 — and remains
  tracked.

### Documentation

- **Truth pass (#622–#626, #630–#632):** the log file is documented at its real
  per-platform location (Tauri's `app_log_dir()` appends the bundle id, and
  Windows uses `%LOCALAPPDATA%`), the v4.5.0 status rules and the opt-in desktop
  notifications are documented, the guarded-command count is corrected, the
  dependency attribution tables are re-derived from the manifests, the stale
  line anchors are gone, and `docs/STATE-OF-FEATURES.md` carries a release-smoke
  recipe for its ⚠ Partial rows.
### Known issues

- The `glib` advisory (GHSA-wrw7-89jp-8q8g) is not clearable by a dependency bump:
  `tauri 2.11.5` requires `gtk ^0.18` while glib's first patched release is
  `0.20.0`, so it needs a tauri/gtk-rs 0.20 migration. Tracked in #642.
- LogViewer virtualization (second half of #434) remains deferred.
- Playlist-id matching for track rules (second half of #432) is out of scope —
  it needs playlist context the status response does not carry.
- The two ⚠ Partial rows in `docs/STATE-OF-FEATURES.md` (sign-in persistence
  across a long idle, install-on-quit updates) are implemented and unit-tested
  but still need one live release cycle to be observed end to end; the smoke
  recipe is in that file.
- The dashboard's presence preview survives a remount via a frontend store
  (#547) rather than by promoting the poller's status/gate state into
  `AppState`; the user-visible defect is fixed, the backend-truth variant is
  still open.

## [4.5.2] - 2026-09-16

Follow-up to 4.5.1: a returning user whose Teams session alone needs a sign-in
no longer gets an unsolicited Spotify OAuth window.

### Fixed
- **Reconnect auto-started Spotify OAuth for a healthy session (#530):** 4.5.1
  routes an incomplete-but-configured install to the Reconnect view, and
  Reconnect's mount started `start_spotify_reconnect` whenever the Spotify
  *credentials* were present — `needsSpotify` never meant "the Spotify session
  is dead". A user whose Microsoft refresh token had lapsed while the Spotify one
  was still alive (idle long enough for the 90-day Teams window, not the 6-month
  Spotify one) therefore got a browser sign-in window they did not need. The
  auto-start now requires `SyncStatus.spotify_connected` to be false, via
  `src/lib/utils/reconnect.ts::shouldAutoStartSpotifyReconnect` (the Spotify card
  and its manual Reconnect button are unchanged).

## [4.5.1] - 2026-09-16

Sign-in persistence fix: relaunching after the app has been closed longer than
an access token's lifetime no longer sends a returning user back through
first-run setup.

### Fixed
- **Expired access token at launch forced full re-onboarding (#530):** the
  launch gate validated a locally-expired access token against the live APIs,
  took the unavoidable 401, and reported the session as dead — so every relaunch
  more than ~1 h after the last poll (the access-token lifetime) opened the
  Onboarding wizard and asked for the Spotify Client ID/Secret plus both OAuth
  flows again, even though nothing had been lost. The gate now spends the
  refresh token under the shared CAS guard and persists the result:
  `invalid_grant` (or unavailable credentials) is the only re-auth signal,
  transient failures keep the pre-existing "a flaky network is not a dead
  session" policy, and a locally-fresh token still short-circuits with no
  network call. The now-dead `validate_spotify_token` / `validate_teams_token`
  stale-bearer probes are removed.
- **A dead session was routed to the first-run wizard (#530):** an incomplete
  verdict with the Spotify Client ID + keychain secret still stored now lands on
  the Reconnect view (`src/lib/utils/boot.ts::bootView`), which re-runs the
  OAuth flow with the credentials already on disk instead of re-asking for
  them.

### Fixed (CI)
- **Dependency-audit job was red on every run (#357):** the npm audit step's
  `run` value was a plain YAML scalar containing ` #357`, so YAML ended the
  value at "see" and bash received an unterminated double quote ("unexpected
  EOF while looking for matching `"`") — the npm leg never executed. The value
  is now a block scalar, so the issue reference stays literal.

### Known issues
- The wizard's `finish()` still overwrites the stored config with defaults when
  it is completed by an existing user — tracked as #531.

## [4.5.0] - 2026-09-16

Feature-packed release: quiet-hours + track-based status rules, pending
status posts when the presence gate clears mid-track, detached windows
with theme sync and self-healing badges, one-click redacted support
snapshot from LogViewer, a 24-issue UX copy + accessibility sweep, a
frontend behavior wave with a real vitest harness, 19 Rust test holes
closed with behavioral tests, and CI/docs/deps modernization
(multi-platform legs, secret scan, dep audit, maintained crates).

### Added
- **Status rules: quiet hours + track rules (#432):** `StatusRulesConfig`
  on `AppConfig` (fully additive, serde defaults — pre-4.5 configs load
  unchanged). Quiet-hours entries support wrap-around ranges and ISO
  weekday selection; track rules match artist/title substrings
  (case-insensitive) with optional replacement status flowing through
  the #384 dedup. Rules flow through the presence-gate path with
  mid-track re-evaluation; Settings gains a rules card with weekday
  picker. New track rules default to disabled (no suppress-the-world
  footgun); frontend deep-backfills nested arrays.
- **Pending status posts on gate-clear (#430):** the #380 re-check branch
  falls through to the single late-post write when the presence gate
  clears mid-track, with debounce/keepalive clocks undisturbed.
- **Detached windows: theme sync + self-healing badges (#433, #422,
  #423):** pre-paint theme bootstrap in `app.html` (`?theme=` override +
  stored key + OS fallback, no hydration flash) plus cross-window
  storage sync; zombie detached flags clear with fall-through
  re-create and concurrent-popOut guard.
- **One-click redacted support snapshot from LogViewer (#434):**
  copy-snapshot button sources solely the backend-redacted
  `recent_logs` + version/platform — the live buffer is never pasted.
- **Frontend unit-test harness (#443):** vitest + jsdom +
  @testing-library/svelte with runtime tests (stores, i18n, LogViewer
  mount) running in CI via `npm test`.
- **Multi-platform CI + supply-chain gates (#355, #356, #357):**
  macOS + Windows check legs, gitleaks secret-scan, cargo/npm
  advisory audit.

### Fixed
- **UX copy sweep (16 issues: #451, #452, #454, #456, #457, #458, #459,
  #460, #461, #462, #463, #465, #467, #469, #471, #473):** tone, jargon,
  first-person, casing, dead-end errors, scope banners, and
  gated-presence wording brought in line across en/de/fr.
- **Accessibility + i18n structure (#381, #382, #385, #387, #390, #412,
  #413, #414):** Back-button i18n, labeled manual-URL input, focus
  management with live announcements, devLog discipline, decorative
  Logo silence, PageHeader action names, UpdatePrompt live-region
  roles.
- **Frontend behavior wave (#420, #422, #423, #424, #425, #426, #488,
  #492, #497, #498, #499, #500):** saveConfig BigInt normalization,
  configStore alias removal, t() runtime-miss degradation with dev
  warning, dead-key removal, neutral Reconnect Teams state, blank-page
  notice outside Tauri, xdg-mime fallback diagnostics, real version
  in logs, LogViewer/i18n behavior tests.
- **Rust behavioral tests (19 issues: #468, #474, #477–#485, #489–#491,
  #493–#496):** pure Teams error classifier funnelling all four Graph
  call sites, break-at-exactly-5 provider-scoped test, device-menu
  id edges + log redaction, unminimize on all Show arms, staged-update
  freshness guard, caller-location guard matrix, device-code expiry
  across IPC; three brittle source-text guards deleted and replaced
  with stronger behavioral tests. Deferred with justification: #393 +
  #478 (need a config.rs prod fix not in 4.5), Svelte halves of #489
  + #491 (covered by the frontend harness).
- **Deep-link fallback port:** the minimal-Linux association fallback
  now resolves HOME via `directories::BaseDirs` following the
  dirs → directories 6 migration (#418, #497).
- **Docs + deps (#358–#363, #365, #368, #369, #371, #372, #374, #418,
  #427):** trigger branch, toolchain/MSRV honesty, SECURITY/ARCHITECTURE/
  TROUBLESHOOTING/SETUP/scope-3.3/STATE/CONTRIBUTING accuracy,
  SHA256SUMS honesty note, tightened npm/Tauri pins, rand 0.9.

### Known limitations carried forward
- #393 (config.rs `[MODULE]` log tags) + #478 (its guard test) need a
  config.rs prod fix — open, low priority.
- LogViewer virtualization (second half of #434) deferred; jumpToLatest
  pre-exists.
- Playlist-id matching (second half of #432) out of scope — needs
  Spotify playlist context + extra API budget.

## [4.4.0] - 2026-09-14

Hardening wave: diagnostics redaction closes 10 secret shapes, profanity
folds shut `ph`/`fuk`/`fux`/z-plural evasions with clean controls intact,
Quit paths share one bounded graceful shutdown, click-path Spotify HTTP
moves off the menu-event thread, device menus pin stable ids with live
re-fetch, polling lifecycle recovers cleanly, playback paths share one
refresh policy, and the frontend auth + boot surfaces shed their races.

### Fixed
- **Diagnostics redaction allowlist gaps (#487):** `SECRET_KEYS` gains `passwd`, `api_key`, `code_challenge` and 7 sibling shapes (10 secret shapes closed); single-quote separators, whitespace-gap separators, and opaque JWT-shaped tokens now redact instead of leaking into the support snapshot.
- **Profanity evasions `ph`/`fuk`/`fux`/z-plurals (#377, #470):** `ph` → `f` pre-fold plus `x` → `ck` / `z` → `s` folds under unchanged boundary gating close `phuck`, `fuk`, `fux`, `niggaz`, `bitchez` (incl. case variants); `skillz`, `phone`, `photo`, `Phoenix`, `Fukushima`, `Jukebox`, `Explicit`, `Zombie` stay clean.
- **Tray + menu Quit shared graceful shutdown (#383, #415):** both Quit paths share `request_graceful_shutdown` with an 8 s bounded `is_syncing` drain and unconditional process exit — no more fixed-sleep race, no more surviving process.
- **Click-path Spotify HTTP off the menu-event thread (#386):** all tray/menu playback and device actions run on worker threads so Spotify network latency never blocks the UI.
- **Stable device-id menu ids with live re-fetch (#388):** tray device selection resolves by stable Spotify device id with live refresh; legacy index-based menu items keep working.
- **Show window unminimizes (#391):** showing the main window restores it from minimized before focusing.
- **Startup build-failure log + exit(1) (#417):** Tauri build failures log and exit non-zero instead of panicking.
- **5-strikes provider-specific reconnect event (#389):** polling exit after five consecutive transient failures emits the provider-matching reconnect event, consistent with the `InvalidGrant` arms.
- **Stop-polling no-handle recovery (#395):** the no-handle branch warn-logs and clears the wedged flag while preserving ownership during in-flight joins.
- **Atomic `get_sync_status` snapshot (#398):** related sync state reads under one critical section so the status surface never mixes generations.
- **No-track clear `ExpiredToken` refresh + retry (#455):** `handle_no_track` mirrors `process_track` with a single Teams refresh + retry before reconnect classification.
- **Playback source guards pin all player paths (#464):** every Spotify playback/query command routes through `player_with_refresh` with concurrent-refresh protection; persist-guard bound 8 → 10.
- **Spotify client builder pins for timeout/UA/`client_id` (#444, #446, #450):** test pins force `accounts.spotify.com` builders through `build_spotify_client` (10 s timeout, `PresenceJam/<version>` UA); `client_id` validator mirrors the secret validator (test-only, no prod change).
- **Frontend auth submit guards (#394):** Onboarding in-flight flags set pre-await plus a Reconnect `spotifyReconnecting` flag, so double-clicks no longer start duplicate auth flows.
- **Shared Teams poll mutex (#396):** one `teamsPollMutex` with finally-release at all 4 poll sites; Check-now stays disabled while held.
- **`expiresAt` plumbing (#397):** device-code expiry passed at the layout start site so layout-started flows keep their countdown.
- **HTTPS-only `verificationUrl` (#410):** `isSafeHttpUrl` gates all 3 verification anchors with a span fallback for unsafe URLs.
- **Honest async teardown with observed rejections (#419):** auth-listener teardown is honestly async with no floating promises.
- **Flow-scoped resets (#421):** `resetSpotifyAuthFlow` / `resetTeamsAuthFlow` at all 7 entries — starting one flow no longer wipes the other.
- **Destroyed guards on listeners (#392):** destroyed guards plus late-resolve release on all 3 listener setups.
- **Boot timeout + retry banner (#405):** bounded boot timeout surfaces a retry banner instead of hanging.
- **Diagnostics retry button (#404):** diagnostics surface gains a retry action on failure.
- **Dismiss guard during download/staging (#402):** update dismiss disabled while a download or stage is in flight.
- **Tracked `goToSetup` timer (#408):** setup-navigation timer tracked for lifecycle cleanup.
- **Live Trace tab (#401):** LogViewer Trace tab mirrors the backend level map.
- **PopOut rejection handling (#403):** catch-and-surface on all popOut/popIn sites.

## [4.3.0] - 2026-09-14

Poll-loop correctness wave: track changes inside the debounce window no
longer vanish, the no-track clear refreshes expired Teams tokens, fresh
threads clear stale status once, gated tracks re-check presence mid-track,
and byte-identical statuses skip the Graph write. Plus auth hardening,
config schema versioning with quarantine, the `sonofabitch` profanity gap,
and a LogViewer render-window with scroll stickiness.

### Added
- **Manual status refresh:** new `refresh_status` command runs one full poll iteration on demand (main window only, no-op while sync is idle) with a Refresh button on the Dashboard track card; successful tray transport actions (play/pause/next/previous/transfer) kick a coalesced delayed refresh so Teams catches up ~2 s after a skip instead of waiting for the next scheduled poll.
- **Config schema versioning with unknown-field retention and corrupt quarantine (#379):** `AppConfig` gains `schema_version` (default 1) plus a flattened `extra` map retained across load→save, so unknown future keys survive settings saves; corrupt `config.json` is quarantined to `.bak` with a diagnostics-visible flag and defaults loaded.

### Fixed
- **Diagnostics snapshot always failed:** `get_diagnostics_snapshot` looked up `AppState` but setup manages `Arc<AppState>` — every call panicked with `state() called before manage()`.
- **Tray next/previous failed with 411 Length Required:** empty-body Spotify player POSTs/PUTs now send `Content-Length: 0` via `.body("")`.
- **Debounced track change recorded but never posted (#364, #383):** the debounce check now runs before every side effect; a change inside the 500 ms window parks untouched on a 1 s fixed retry, so the retry re-detects the change and emits/posts exactly once instead of losing short tracks for up to 60 s.
- **No-track clear never refreshed expired token (#370, #388):** the Teams refresh block is now `teams_token_for_write()`, called from both `process_track` and `handle_no_track` — the clear refreshes instead of failing forever.
- **Fresh thread + no-track skipped Teams clear (#373, #391):** a `first_iteration` flag makes a fresh thread attempt one clear on first no-track poll (pre-restart status no longer survives for hours); later idle polls stay no-ops and one-shot refreshes stay silent when idle.
- **Presence gate never re-evaluated mid-track (#380):** the gated branch re-reads presence at most every 240 s on its own `last_gate_check` clock and posts late if the gate cleared; failed re-reads keep suppression (fail-safe).
- **Identical status re-POSTed every cycle (#384):** `last_posted_status` skips byte-identical writes inside a 5-min keepalive; fingerprint changes and lapsed keepalives force-write so Graph expiry never lapses.
- **Spotify exchange hard-failed without refresh_token (#350):** exchange-path `TokenResponse.refresh_token` is `Option`; omission surfaces a precise `token response omitted refresh_token` error instead of a generic parse failure after the single-use code is consumed.
- **Manual-code path destroyed pending auth before validation (#351):** peek-then-validate-then-take — a typo'd paste no longer burns the pending auth, so retrying works.
- **Legacy-plaintext read failed when keychain locked (#352):** legacy branch parses JSON before touching the keychain; the key is fetched only on parse success, and parse/migration share one helper.
- **Client-secret length-only validation (#354):** `validate_spotify_client_secret` enforces the sibling charset plus a 512-char cap at the IPC boundary.
- **Glued `sonofabitch` bypass (#411, #472, #378):** `is_strong_stem` now matches `shit | fuck | bitch`; carve-outs (`mustard`, `peacock`, `cockpit`, `spicy`, `tardy`) stay clean.
- **LogViewer re-render + scroll theft (#399, #400):** 500-entry buffer kept, newest 100 rendered keyed with a showing-X-of-Y note; stickiness captured before push and inside the rAF snap, filter switches share the helper, floating Jump-to-latest button; new `logs.showingOf` + `logs.jumpToLatest` strings in en/de/fr.
- **Stale polling default in SETUP:** polling-interval default corrected 10s → 30s.
- **Nonexistent Disconnect in TROUBLESHOOTING:** token-refresh and Teams-update fixes now say Reconnect (no Disconnect control exists).
- **Presence-gating prose vs mid-track re-check:** ARCHITECTURE polling flowchart + gating section describe the 4.3.0 helpers (`teams_token_for_write`, debounce, keepalive, `last_gate_check`, `first_iteration`).
- **Missing `spotify-secret-conflict` event row:** ARCHITECTURE event table gains the row; `show-about` row restored.
- **Never-re-auth SOF line ref:** STATE-OF-FEATURES points at `teams_token_for_write` (~line 984); 8 new 4.3.0 rows added.
- **Secret validation coverage:** SECURITY documents IPC charset+cap rules and the precise omitted-refresh error.

> **i18n note:** the new `logs.showingOf` / `logs.jumpToLatest` German and French strings are best-effort and need native-speaker review (the `dashboard.refresh*` strings above likewise).

## [4.2.1] - 2026-09-13

Docs-alignment patch: dead outbound links repaired, drifted claims corrected,
and CHANGELOG link hygiene pinned by CI so the file cannot rot again.

### Fixed
- **Scope-3.3 grounding links (#435):** 9 outbound URLs repaired after Tauri v2
  and GitHub Actions docs restructures (tray-icon, deep-link, window namespace,
  artifact-attestations IA).
- **README/USAGE/SETUP/CONTRIBUTING/SECURITY drift (#436, #439, #366, #440, #441):**
  Linux distribute link, polling-minimum 5s floor in both files, repo-pinned
  Tauri CLI note, three-platform log paths.
- **CHANGELOG link hygiene (#437, #438, #442):** 14 missing version link defs
  added, orphan [3.0.1] def deleted, missing ## [2.3.7] section reconstructed,
  def block sorted descending with [Unreleased] first, CI `changelog-links`
  job fails on any future header-without-def.

## [4.2.0] - 2026-09-12

Trust-wave release: expired-but-refreshable sessions no longer force a full
re-auth, device-code sign-ins show a live expiry countdown with one-click
recovery, quit-time deferred updates ask before installing anything stale,
and two secret-hygiene leaks (device-code bearer in logs, absolute log path
in diagnostics) are closed. Spotify HTTP clients are bounded and the
redirect URI is pinned at the IPC boundary.

### Added
- **Never-re-auth refresh-and-retry (#428, #367, #375):** an expired Teams token on status write now gets one `refresh_teams_token` + CAS-commit + persist + single retry before `teams-reconnect-required` is emitted; all six tray/playback commands share the same proactive-plus-one-retry policy, so only `InvalidGrant` (or a failed refresh) asks the user to sign in again.
- **Device-code expiry countdown with one-click fresh code (#429):** Onboarding, Settings, and Reconnect show a live countdown while a device code is valid and swap to an expired state with a Get-new-code button on the existing sign-in path; expired codes are refused at poll time.
- **Quit-time deferred-update confirmation with version check (#431):** Install-on-quit opens a candidate-vs-current confirm surface with install/skip; a stale stage (`staged <= current`) is skipped with a log + marker instead of installing, and a declined stale candidate shows a skipped state with an explicit Install-anyway (`force:true`) override. `allowDowngrades` is now `false`.
- **Keychain secret-conflict prompt (#376):** a legacy plaintext `spotify.client_secret` in `config.json` that conflicts with the keychain value now surfaces a one-time `spotify-secret-conflict` event with a Settings banner and reconnect action; the plaintext is never deleted.

### Fixed
- **Spotify token exchange + refresh had no HTTP timeout (#347):** all Spotify calls go through a shared `build_spotify_client()` (10 s timeout, `PresenceJam/<version>` User-Agent), matching the Teams client.
- **Device-code bearer was info-logged (#348):** the raw device-code response body and both `Err` paths are length-only in logs and errors; receipt logs carry `expires_in`/`interval` only.
- **`redirect_uri` unvalidated at the Spotify IPC boundary (#349):** exact-match allowlist against `SPOTIFY_REDIRECT_URI` (`presencejam://callback`) enforced in `start_spotify_auth` and `start_spotify_reconnect`.
- **Spotify clients sent no User-Agent (#353):** closed as a drive-by of the shared-client fix above.
- **Diagnostics embedded the absolute log path (#409):** snapshot statuses carry only `PresenceJam.log`; the full path stays in local error logs.
- **Vite config tripped `svelte-check` (#501):** removed the unused `@ts-expect-error` directive; no behavior change.

> **i18n note:** the new German and French strings in this release are best-effort and need native-speaker review (see the individual PR bodies).

## [4.1.1] - 2026-09-12

Profanity-filter correctness pass: the matcher no longer censors innocent
words or misses glued compounds, common evasions (separators, leet, Unicode
confusables) are closed, and the placeholder pipeline is hardened. Boundary
tests now cover phrases, not just bare words (#333).

### Fixed
- **Start-anchored false positives (#328):** `cockpit`, `Dickens`, and `Spice Girls` no longer trip the filter; the short-tail auto-flag is now a first-token boundary check.
- **Whitelist compared the whole remainder (#329):** `cocktail bar` and `cocktails` pass; only the first token after the stem is checked.
- **`head` whitelist immunized insults (#330):** `dickhead`/`shithead`/`fuckhead` flag; the whitelist is scoped to `cock`+`tail`.
- **Glued compounds evaded the filter (#331):** `bullshit`/`dipshit`/`horseshit` flag via strong stems needing only a clean right edge.
- **Duplicate-skip fabricated `shiitake` (#332):** stretched matches now require a right-side word boundary.
- **Separator insertion defeated the filter (#334):** `f*ck`/`f.u.c.k`/`f u c k` flag; separator-spanning matches must start and end at word boundaries, so `Push It` stays clean.
- **Leet gaps (#335):** `6`/`8` to `b`, `9` to `g`, `+` to `t`, `(` to `c`, `\/` to `v`, `2` to `i` (`sh2t` flags).
- **Unicode confusables bypassed the filter (#336):** zero-width/format chars are stripped, fullwidth folds to ASCII, and precomposed Latin is table-folded, with no new dependency.
- **Mixed repeat+leet bypass (#337):** `fuu1uck` flags.
- **Word-list gaps (#338):** `asshole`/`tits`/`twat` added as compounds (bare `ass` stays out to protect `class`/`assassin`).
- **Profane placeholder passed through (#339):** the effective placeholder is re-scanned and falls back to the default on hit.
- **`{emoji}` token was case-sensitive (#340):** placeholder tokens now match case-insensitively.
- **Track metadata expanded `{emoji}` (#341):** `{emoji}` substitutes before data fields, so data-inserted tokens survive verbatim.
- **Placeholder default triple-copied (#342):** `config.ts` is the single frontend source, and the Settings preview routes through `filter_status` with a profane-sample toggle.
- **Mid-track config flips left stale statuses (#343):** the change key fingerprints filter+placeholder+format, and the 304 arms force one rewrite.
- **Track metadata leaked into diagnostics (#344):** the track-found log lines are now debug level.

## [4.1.0] - 2026-09-12

A correctness and hardening pass over the 4.0.0 surface: a startup panic on a
truncated credential file, OAuth tokens no longer crossing into the webview, a
retry/re-auth policy that no longer discards a healthy session on a transient
blip, and a documentation sweep that reconciled drifted claims with the code.

### Security
- **OAuth tokens no longer reach the webview (#299):** the Spotify and Teams auth commands return `()` and their completion events carry no payload, so no `access_token`/`refresh_token` value is handed to a caller or broadcast on the event bus.
- **Single-use OAuth state binding routed through constant-time compares (#239):** full-state comparisons go through `pkce::ct_eq`, closing a timing side channel on the CSRF check.
- **Truncated credentials rejected instead of panicking (#294):** `decrypt_tokens` indexed the version byte before its length guard, so a 5-byte `tokens.json` panicked at startup. A malformed file now returns an error and drives re-authentication.
- **Main-window-only guards on 12 sensitive commands (#241):** detached Logs/Settings panes can no longer invoke config, auth, or window-management commands that assume the main window.
- **Unused `opener:default` permission dropped (#240)** from both the detached and default capability sets.

### Fixed
- **Settings save was completely broken (#285):** `structuredClone` in `toSavePayload` rejected the Svelte 5 `$state` proxy with a DataCloneError, aborting every save before IPC. The config is now snapshotted to a plain object first.
- **Clamped config reached disk but not memory (#297):** `save_config` persisted a clamped copy while `AppState` and the frontend store kept the raw input, so a typed `999` was recorded as `30` on disk but polled at `999` until restart. `clamped_config()` is now the single definition of what is persisted, and the command stores and returns it.
- **A transient Teams refresh failure forced a full re-auth (#295):** the `RefreshFailed` arm matched every error, so one 5xx or dropped connection discarded the session and drove a device-code sign-in. Only `invalid_grant` and a rejected access token now force re-auth.
- **Expired Spotify token with a cold keychain cache looped silently (#296):** the refresh path retried every 30 s with no user-visible escape. Missing credentials are now classified as unavailable, routed to reconnect, and counted toward the five-strike threshold.
- **Idle 304 responses discarded the ETag (#242):** the validator was dropped on every unchanged-track poll, so idle polling alternated conditional and unconditional GETs and doubled the request rate. The validator is retained and the pause backoff advances as it does on the 200-body path.
- **The presence gate omitted `focusing` (#254):** the documented Do-Not-Disturb-class state now suppresses status writes like `busy` and `presenting`.
- **Failed exit-time update installs were invisible (#244):** a marker recorded by the staged-update path is now surfaced in Diagnostics on the next launch.
- **Dashboard leaked all ten event listeners when unmounted mid-`onMount` (#287)**, and **sync-toggle failures became unhandled rejections (#289)**; a failed Spotify reconnect in Settings was likewise silent (#288).
- **Auth phase was set on mount rather than on flow start (#286)**, leaving Onboarding wedged in a permanent spinner when credentials were missing.
- **Release re-cut could publish onto the wrong tag's Release (#265)** and collided with the run it was recovering (#266); the Homebrew formula re-render also no-op'd on a stale version line (#268). The re-cut path now resolves and validates the tag once.
- **Max-interval input accepted values stricter than the backend clamp (#243):** its `min` is derived from `clamp_polling`'s effective minimum rather than hardcoded.

### Changed
- **Dependency refresh:** the npm and Cargo minor/patch groups were bumped (8 and 10 packages); Dependabot subjects no longer emit a doubled scope.
- **CI gates:** `cargo fmt --check` is now enforced alongside clippy and the test suite; CodeRabbit and Sourcery reviews are opt-in.
- **Documentation sweep:** drifted claims were reconciled with the code — OAuth scope sets, the `tokens.json`/`config.json` directory split, the 60 s placeholder expiry, the pause backoff ladder, the device-code cadence, the absence of a Disconnect control, Linux artifact filenames, and the dependency credits.

### Note on the 4.0.0 entry
- The 4.0.0 section says i18n was "landing separately (not yet in this history)". That is inaccurate: both i18n commits (`577b047`, `e205fa0`) are ancestors of the `v4.0.0` tag and are present in the published 4.0.0 installers. The entry is left as shipped rather than rewritten; the i18n language picker is part of v4.0.0.

### Added
- **Regression coverage:** the five-strike transient threshold, `status_expiry_str` arithmetic, a previously vacuous guard, profanity case-insensitivity, and 0600-at-creation for `tokens.json`.

## [4.0.0] - 2026-08-23

The scope-3.3 polish wave beyond Stratus: supply-chain and OAuth hardening, a local diagnostics page, silent background updates with install-on-quit, multi-window detach for Logs/Settings, conditional-GET polling, settings/notification UX polish, and a WCAG 2.2 AA accessibility pass. Landing separately (not yet in this history): i18n with an en/de/fr language picker (scope item C6).

### Security
- **Dependency prune (C13):** dropped the last remaining `@tauri-apps/plugin-shell` entry from `package.json` (+ `package-lock.json`) and pruned `tauri-plugin-shell`/`tauri-plugin-store` from `Cargo.lock`; removed their four ACKNOWLEDGEMENTS.md rows and reworded stale comments naming the store crate. No imports existed in Rust or Svelte code and no capability granted shell/store IPC — bundle-size and attack-surface reduction only, no behavior change.
- **Release attestations + manual re-cut (C10):** the release workflow now attests every packaged artifact via SHA-pinned `actions/attest-build-provenance` (SLSA build provenance; `id-token: write` + `attestations: write` scoped to the build job alone, `contents` downgraded to read there). New `workflow_dispatch` trigger with a `tag` input re-cuts an existing `v*` release without retagging; a `resolve-tag` job validates the tag exists and every downstream job consumes its output instead of `github.ref_name`. Full SHA-pinning audit of all `uses:` entries confirmed clean. Verification steps documented in `SECURITY.md`.
- **OAuth state binding hardened to single-use with constant-time compares (C1, defense-in-depth for #66):** `pkce::LaunchBinding` replaces the bare launch-secret slot — it holds the per-launch secret plus a single-use slot with the SHA-256 of the in-flight flow's PKCE verifier, bound at authorize time and validated at callback time (`handle_deep_link` + `complete_spotify_auth_manual`), tying the echoed `state` to the exact code_verifier presented at token exchange. Single-use consumption (`take()` after validation) makes replayed callbacks fail closed, mirroring what RFC 6749 §10.12 requires for authorization codes. Comparisons go through constant-time `pkce::ct_eq` (manual XOR-fold); strict `<csrf>.<secret>` structure check rejects truncated/malformed states before comparison.
- **deps: h2 bumped to 0.4.18**, clearing RUSTSEC-2026-0258.
- **Accepted transitive risks:** quick-xml 0.37.5 (via `plist`) and 0.39.x (via `tauri-winrt-notification`) carry RUSTSEC-2026-0194/-0195. Exposure here is build-time/self-generated XML only (no untrusted XML is parsed at runtime); held pending upstream fixes — documented under `SECURITY.md` → *Accepted transitive risks*.

### Fixed
- **WCAG 2.2 AA accessibility pass (C12):** added a visually-hidden skip-to-content link targeting `#main-content`; raised `--focus-ring` alphas (dark 0.45→0.75, light 0.35→0.90) so the indicator meets 3:1 non-text contrast; `prefers-reduced-motion` now disables decorative pulse animations (badge dot, Playing indicator); fixed failing contrast pairs in both themes — dark: fg-on-accent white→`#0F1226` on accent green (2.59→7.16:1), danger→`#FB8A8A` (5.29:1); light: fg-subtle→`#5A618F` (5.25:1), accent greens darkened so white text reaches 4.91–8.74:1, success/warning/danger/info/accent-text hues darkened to ≥5:1 on their soft backgrounds; fixed contradictory `role="alert"` + `aria-live="polite"` on the playback toast (kept `role="alert"`); verified `role="status"` regions across Dashboard/Diagnostics/UpdatePrompt.

### Added
- **Telemetry-free local diagnostics page (C5):** new `get_diagnostics_snapshot` command collects a support snapshot entirely locally — app/Tauri/OS versions, sanitized config summary, OAuth token metadata only (RFC3339 expiry timestamps + presence flags, never token values), keychain presence flags for both slots, and the last 50 lines of `PresenceJam.log` passed through a defensive second-pass redaction helper. New `Diagnostics.svelte` page (Copy diagnostics / Save to file, `role="status"` feedback) reachable from a dashboard icon button. No network calls anywhere, matching the No Telemetry promise.
- **Silent background update checks + install-on-quit (C3a+C3c):** `UpdatePrompt` re-checks GitHub Releases every ~24h while running (failed silent checks stay console-only, never banner/toast). New "Install on quit" path: `stage_deferred_update` runs its own check + download + signature verification on the blocking pool and holds the verified bytes in managed `PendingUpdate` state; `lib.rs` swaps to `build().run()` with a `RunEvent::Exit` arm that applies the staged update when quitting via tray Quit or app exit. The Windows installer relaunches automatically; macOS/Linux pick up the replaced bundle/AppImage on next launch.
- **Multi-window detach for Logs & Settings (C7):** each pane gains Pop out / Pop back in (VS Code detached-panel style). Child windows are created JS-side via `WebviewWindow` with stable labels `logs-detached`/`settings-detached`, rendered by a new `/detached/[pane]` SvelteKit route; `src/lib/stores/detach.ts` tracks popped-out state (main-window-only) and dashboard nav shows a dot badge + focuses the child instead of navigating. New `capabilities/detached.json` scopes the two child labels to a minimal mirrored permission set; `default.json` gains the runtime window-creation permissions. `+layout.svelte` guards its always-mounted reconnect/auth/update listeners behind a window-label check so detached windows never double-register. The app still boots single-window; tray/show_window, deep-link, and single-instance routing unchanged.
- **Deep-link navigate UX (C2):** after a validated Spotify deep-link callback token exchange the backend emits `navigate` (`dashboard`); after Teams device-code success it emits `navigate` (`settings`) — the app lands on the right view instead of just raising the window. The frontend listener ignores navigation while Onboarding owns the view.
- **Tray polish (C4):** live tray tooltip `Artist — Track (▶|⏸)` updated on each menu rebuild; Play/Pause is now a native CheckMenuItem (checked state from `LAST_PLAYING_STATE`) instead of label-swapped rebuilds; cfg-gated macOS dock badge helper (`set_presence_gated_badge`) wired into the polling loop so the badge reflects presence-gated sync state.
- **Conditional GET for currently-playing polls (C11):** the loop stores the ETag response header from `GET /me/player/currently-playing` and echoes it as `If-None-Match`; a 304 Not Modified becomes a no-op iteration identical to unchanged-track minus JSON parse/format_status/profanity work. Degrades gracefully: no stored validator → unconditional GET, missing header → next poll unconditional (Spotify does not document ETag support; relies only on RFC 9110 conditional-request semantics and is a behavioral no-op if Spotify never sends one).
- **Settings dirty-state + clamp feedback + reset buttons (C9):** unsaved-changes banner driven by a BigInt-safe deep compare of local config vs saved config; inline clamp feedback on the polling min/max inputs mirroring Rust `clamp_polling` semantics (min clamps to [5,30] first, max clamps to [effMin,300]) showing the effective saved value when min > max; per-section "Reset to default" buttons (Presence, Status format, Polling, Appearance).
- **Notification throttle + grouping (C8):** track-change desktop notifications throttled to at most one per 5 seconds (throttled tracks don't claim `lastNotifiedId`); replace-in-place per session via a stable notification id + group tag where the platform supports it; dedup and permission gates unchanged.

## [3.2.0] - 2026-08-22 — Stratus

Comprehensive hardening and UX polish covering 28 issues. Windows auto-update unblocked, main-thread stalls eliminated, token-revocation loop fixed, event contracts made reliable, and docs/build pipeline tightened.

### Security
- **OAuth hijack binding (#66, PR #235):** per-launch 32B secret bound into OAuth `state` as `<csrf>.<launch_secret>` (stored in `AppState OnceLock`), validated in `handle_deep_link`; mismatch → redacted warn + early return. Scheme stays `presencejam://` (macOS cannot runtime-register). `state` composition ~130 chars within limits.
- **Log redaction (#228, PR #235):** all deep-link/auth logs now redact `code`/`state` to `[REDACTED len N]` or 4-char prefix; byte-slice panic fixed via `chars().take(4)`.
- **Capabilities least-privilege (#227, PR #235):** removed `tauri-plugin-store`/`shell` registrations + Cargo deps + capability entries; audited `default.json` to minimal set; `package.json` store removed (lockfile synced).
- **Logging config wiring (#226, PR #235):** `logging.enabled`/`log_level` now applied via `log::set_max_level` after config load (disabled → `Off`, case-insensitive, default `Info`).

### Fixed
- **Windows auto-update 404 (#204, PR #232):** `release.yml` Windows `bundle_path` now uploads renamed `PresenceJam-<tag>.msi` (removed wildcard duplicate) + verification step checks published asset names via `gh api` before generating `latest.json` (advisory option a). Fleet on ≤3.1.0 can now auto-update to 3.2.0.
- **Update-chain checklist (#205, PR #232):** added `docs/windows-update-chain-v3.2.md` with curl/sig verification steps for the stuck Windows fleet.
- **Release concurrency (#206, PR #232):** `concurrency: group: release-${{ github.ref }}, cancel-in-progress: false` prevents retag double-fire.
- **Runner bump (#207, PR #232):** `ubuntu-22.04` → `ubuntu-latest` (24.04) in `release.yml`.
- **Digest mismatch hardening (#208, PR #232):** `digest-mismatch: warn` → `error` on both `download-artifact` steps.
- **SHA256SUMS (#209, PR #232):** release job now generates `SHA256SUMS.txt` (basename) from `artifacts/**/*` and uploads via `--clobber`.
- **CI on main pushes (#210, PR #232):** `ci.yml` now triggers on `push: branches: [main]`.
- **Least-privilege checkouts (#211, PR #232):** `persist-credentials: false` on release checkouts + `winget` job `contents: write` → `read`.
- **README asset table (#212, PR #232):** canonical names `PresenceJam-macos.dmg`, `PresenceJam-linux-amd64.AppImage/.deb`, `PresenceJam-<tag>.msi`.
- **ARCHITECTURE lockfile (#213, PR #232):** `pnpm-lock.yaml` → `package-lock.json`.
- **STATE-OF-FEATURES header (#214, PR #232):** `v3.0.0` → `v3.2.0` + digest rows updated to `error`.
- **Tray lock contention (#217, PR #233):** `cached_devices`/`cached_queue` now snapshot under short lock, drop, fetch outside lock, re-acquire only to store; `update_tray_menu` fetches before `tray_write_lock`, holds lock only around `set_menu`.
- **Play/Pause label staleness (#229, PR #233):** `track_key` now includes `is_playing` so same-track pause triggers rebuild; label flips immediately via `force_tray_refresh`.
- **Tray Pause/Resume dead when Dashboard not mounted (#230, release):** lifted `toggle-pause` listener from `Dashboard.svelte` to always-mounted `+page.svelte` (checks `get_sync_status` then toggles `start/stop_syncing`).
- **Main-thread stalls (#215, PRs #233 #234 #235 #236):** 12 IO-bound commands → `pub async fn` + `spawn_blocking` (`start/stop_syncing`, `app_exit`, `save_config`, playback 7, `start/poll_teams_auth`, `start_spotify_auth`, etc.); `complete_onboarding` made async to await `start_syncing`.
- **Stop/Exit join freeze (#218, PR #234):** `stop_polling_and_join` moved final `join` into `spawn_blocking` with ThreadId ownership check; `app_exit` detaches after grace, preserving #69 drain-first invariant.
- **Token-revocation infinite loop (#219, PR #234):** 401-retry `InvalidGrant` path now clears `*state.tokens.spotify_mut()=None`, persists, emits both `spotify-reconnect-required` + `reconnect-required`, increments `transient_failure_count` toward 5-strikes. Test `test_cas_helper_body_has_no_persist_and_call_sites_persist` updated to 5 sites (2 invalid_grant + 3 refresh-success).
- **Teams device-code 15-min block (#216, PR #236):** `poll_teams_auth` → `async fn` + `spawn_blocking`, `interval.clamp(1,15)`, chunked sleep max 30s, `slow_down` +5s cumulative, terminal errors short-circuit per RFC 8628 §3.5, `expires_in` 900s bound.
- **Dead teams-auth-failed listeners (#223, PR #236):** `teams-auth-failed` now emitted (string payload) from `start_teams_auth_device_code` and `poll_teams_auth` Err paths, matching `listen<string>` in 4 frontend sites.
- **Spotify reconnect only in Settings (#220, PR #237):** lifted `spotify-reconnect-required` from Settings-only to always-mounted `+layout.svelte` (mirrors teams #157), with keychain check → onboarding fallback.
- **Teams re-auth missing in Reconnect (#222, PR #237):** `Reconnect.svelte` now derives `needsTeams` from `teams_connected` and offers Teams device-code re-auth.
- **Playback errors silent (#224, PR #237):** layout-level `playback-error` toast (string payload, auto-dismiss 6s).
- **Autostart optimistic divergence (#221, PR #237):** `set_autostart_enabled` now try/catch with revert + OS-state re-query.
- **Frontend hygiene batch (#225, PR #237):** `show_window`/`open_logs_folder`/`is_spotify_client_secret_set` wrapped, `isPermissionGranted` dead pre-check removed/handled, `preview_status` debounce 300ms + seq guard, `AppConfig` now imports generated `../types` (BigInt `PollingConfig` fields fixed via `structuredClone` + `BigInt`↔`Number` helpers), listener micro-race `onDestroy` guards, `package.json` store removed + lockfile synced; follow-up BigInt clone fix (structuredClone + toSavePayload) and layout `open_external_url` isolation.

### Changed
- **PollingConfig BigInt handling (PR #237 follow-up):** `PollingConfig` `u64` fields remain `bigint` in `ts-rs` but frontend now uses `structuredClone` and converts via `BigInt()`/`Number()` at load/save boundaries to avoid `JSON.stringify(BigInt)` throws.
- **Clippy hygiene (PR #234):** `state.rs` `clone_on_copy` fixed via deref `*thread_id()`.


## [3.1.0] - 2026-08-20

### Fixed
- **Teams device-code fix (PR #202):** request `openid` whenever `profile` is requested (`MICROSOFT_GRAPH_SCOPES = "Presence.ReadWrite Presence.Read openid profile offline_access"`) — Microsoft Entra returns `AADSTS70011` without it, before Conditional Access. Regression test `teams_oauth_profile_scope_also_requests_openid` + DRY `decode_teams_granted_scopes` via constant. Credit: @BigChiefRick #200 (superseded).
- **Onboarding Teams Step 2 empty-URL startup (PR #202):** `connectTeams` used stale derived `teamsVerificationUrl` (`''` on first click) for `open_external_url('')` which threw `validate_http_url` and was caught as generic "Failed to start Teams sign-in" — now uses `response.verification_url` directly, makes opener non-fatal, surfaces `String(e)`.
- **Onboarding manual URL paste silent failure (PR #202):** pasting a URL without `?code=` or with rejected `state` now sets `validationError` instead of console-only logging.
- **Settings save generic error (PR #202):** `handleSave` now surfaces `String(e).slice(0,180)` instead of bare "Failed to save".
- **Settings preview race (PR #202):** `preview_status` invoke now has `.catch(() => previewText='(preview unavailable)')` to prevent unhandled rejection.
- **Polling config hardening (PR #202):** clamp `PollingConfig` on load + save (`default 5-300`, `minimum 5-30`, `maximum clamp(min,300)`, `expiry 0-60`) to prevent hand-edited `0` busy-loop / 429 storm; align Rust `default_min_interval_seconds` 5→10 with frontend store (`src/lib/stores/config.ts:85`); update `config_minimum_interval` fallback 5→10 and test expectations.
- **Dir permissions + durability (PR #202):** `config_dir()` + `tokens_file_path()` now `chmod 0700` after `create_dir_all` (completes 0600 file-mode promise at directory level, issue #135); `atomic_write_json` + `write_tokens_atomic_with_key` now `fsync` parent directory after `rename` (persists directory entry across power-loss, POSIX durability).

### Added
- **Track-change notifications (3.1.0):** optional desktop notification when the Spotify track changes — uses the dormant `tauri-plugin-notification` (capability `notification:default` + `allow-*`), opt-in via **Settings → Notifications** toggle (`localStorage notificationsEnabled`, default off), deduped by `lastNotifiedId`, permission flow via `isPermissionGranted`/`requestPermission`, `sendNotification` with title/artist — album and `album_art_url` icon. Gated to avoid spam.
- **Retry-After http-date parsing (3.1.0):** `parse_retry_after` in `teams.rs` + `spotify.rs` now supports both delta-seconds (`120`) and HTTP-date (`Wed, 21 Aug 2026 12:00:00 GMT`) per RFC 7231 §7.1.3 via `httpdate = "1"` crate, capped at 300s, shared `parse_retry_after_value` helper, `unwrap_or(0)` clock-skew handling, with two new unit tests.

### Changed
- **Polling fallback alignment (PR #202):** `playing_track_sleep` / `config_minimum_interval` now consistently use 10s minimum (was split 5s Rust fallback vs 10s UI).

### Docs
- **SECURITY.md WINGET_TOKEN (PR #202):** correct `fine-grained PAT on microsoft/winget-pkgs` → `classic PAT public_repo + workflow on Carme99/winget-pkgs fork (komac sync-fork)`, matching `release.yml`.
- **CHANGELOG compare link (PR #202):** `Unreleased` now `compare/v3.1.0...HEAD` + add missing `[3.0.0]` tag link (was `v2.9.0`).
- **CI header (PR #202):** `ci.yml` Rust job comment "Does NOT run tests" → "Runs cargo check + cargo test + clippy".

### Dependencies
- **deps(frontend): svelte 5.56.8 → 5.56.9, svelte-check 4.7.4 → 4.7.6 (PR #201).**

## [3.0.0] - 2026-08-09

### Breaking
- **One-time re-auth required for both providers:** the new scope set is not covered by existing grants. Spotify adds `user-modify-playback-state` (tray playback control); Teams adds `Presence.Read` + `profile` (presence features). After upgrading, reconnect Spotify and Teams once from Settings.
- **tokens.json is now encrypted at rest (PR #184):** the token file is AES-256-GCM encrypted, with the key stored in the OS keychain. Migration to the new format is one-way and happens on first read — **old builds cannot read the new format**, so downgrading requires a fresh re-auth.
- **Presence gating is ON by default:** status writes are suppressed while the Teams status is busy/DND, in a meeting, on a call, or presenting. Toggle it off in Settings if you want unconditional writes (#187).

### Added
- **Availability sync opt-in (PR #187):** the availability sync re-arms to Available when a previously-set Available presence is re-set within 5 minutes, and clears on pause.
- **Presence-aware gating (PR #187):** a `getPresence` gate feeds the gating logic, with new `presence-gated` and `presence-availability-updated` events so the frontend stays in sync.
- **Tray playback controls (PR #185):** Play/Pause/Next/Previous plus **Devices** and **Up Next** submenus in the tray menu.
- **Auto-update (PR #188):** `tauri-plugin-updater` wired to GitHub Releases with an update banner in the UI.
- **tokens.json encryption (PR #184).**

### Fixed
- **Polling refresh self-deadlock (PR #183):** a refresh triggered from within the polling iteration no longer blocks the loop.
- **Dependabot security alerts (PR #186):** postcss and quinn-proto bumped to clear the alerts.

### Security
- **tokens.json encrypted at rest (PR #184):** AES-256-GCM, key in the OS keychain; one-way migration on first read.
- **postcss 8.5.26 override + quinn-proto 0.11.16 (PR #186).**

### Docs
- **3.0 sweep:** release research and scope documented in `docs/3.0-release-research.md`.

## [2.10.0] - 2026-08-09

### Added
- **Teams auto-refresh restored (PR #177):** the device-code flow now requests the `offline_access` scope, so Microsoft issues a refresh token — previously Teams tokens died after the ~1h access-token lifetime and forced manual re-auth on every session. Refresh rotation also preserves the existing refresh token when the response omits a new one (#151).
- **Live-stream support (PR #178):** a null `progress_ms` (live/unknown position) now falls back to the default polling interval and sends no status expiry, instead of re-setting a status that expired in 10s (#165).
- **Spotify error classification (PR #175):** `invalid_grant` on refresh is now detected and triggers re-auth (Spotify refresh tokens expire after 6 months) (#160); 429 responses honor the documented `Retry-After` header (#159).

### Changed
- **Teams re-auth in Settings (PR #175):** the device-code flow is fully wired — user code + verification URL shown, browser auto-opened, polling started; `teams-reconnect-required` events are handled at the layout level so they're no longer lost while on the Dashboard (#152, #157).
- **Status-message expiry (PR #178):** `expiryDateTime.dateTime` is sent offset-less with six fractional digits per the Graph schema; pause/no-track placeholders carry a 60s expiry so they self-remove even on quit, and identical writes are gated (#155, #156).
- **403 handling (PR #177, #178):** Forbidden is classified separately from an expired token — a license/permission failure no longer triggers a re-auth loop (#153).
- **Item-type gate (PR #175):** currently-playing responses are gated on `currently_playing_type == "track"` — podcasts and ads no longer appear as "Nothing playing" (#161).
- **Dead code removed (PR #177):** the unused Teams auth-code path (`complete_teams_auth`, `presencejam://teams-callback`, `PendingTeamsAuth`) is deleted — device-code flow only (#158).
- **Manual Spotify fallback (PR #175):** the pasted-redirect-URL path now validates the OAuth `state` parameter (CSRF) and pending expiry, matching the deep-link path (#162).

### Removed
- **Dead Spotify scopes config (PR #175):** `SpotifyConfig.scopes` was never read by the auth flow — the authorize URL is the single source of truth (#163).
- **Unused `User.Read` scope (PR #177):** no Graph call used it; the device-code request is now `Presence.ReadWrite offline_access` (least privilege).

### Docs
- **MS Learn + Spotify alignment batch (PR #179):** 21 commits closing #166–#174 plus the doc halves of #151/#158/#160 — corrected ARCHITECTURE.md diagrams (POST → 200 OK), work/school-only Teams account requirement, status-expiry attribution to the app's own setting, Spotify app-creation walkthrough matching the current dashboard, "Authorization Code + PKCE (confidential client)" labeling, admin-consent claims, and polling call-math figures. Review report committed at `archive/reviews/mslearn-spotify-docs-alignment.md`.

## [2.9.1] - 2026-07-23

### Dependencies
- **deps(ci): bump `actions/setup-node` 6.4.0 → 6.5.0 (PR #144).** Minor via Dependabot weekly batch. Bundles security bumps underneath: `@actions/cache` 5.1.0, undici + fast-xml-parser security overrides. (CI github-actions group.)
- **deps(backend): bump the cargo-minor-and-patch group with 6 updates (PR #146).** `tauri-plugin-store` 2.4.3 → 2.4.4 (patch, iOS scanner fix), `tauri-plugin-log` 2.8.0 → 2.9.0 (minor, adds opt-in `FileOpenStrategy::Rotate` — default behaviour unchanged), `serde` 1.0.228 → 1.0.229, `serde_json` 1.0.150 → 1.0.151, `rand` 0.8.6 → 0.8.7, `tauri-plugin-single-instance` 2.4.2 → 2.4.3. (Backend cargo group.)
- **deps(frontend): bump the npm-minor-and-patch group with 5 updates (PR #145).** `@tauri-apps/plugin-log` 2.8.0 → 2.9.0, `@tauri-apps/plugin-store` 2.4.3 → 2.4.4 (mirror PR #146's Rust-side bumps — Tauri requires Rust ↔ JS plugin version sync), `@sveltejs/kit` 2.69.1 → 2.70.1 (minor; `defineEnvVars` moved to new `@sveltejs/kit/env` package — no impact since PresenceJam doesn't use `defineEnvVars`, runs in SPA mode via `@sveltejs/adapter-static`), `svelte` 5.56.4 → 5.56.6, `svelte-check` 4.7.1 → 4.7.3. (Frontend npm group.)

Patch-level bump because all three PRs are Dependabot minor/patch bumps with no user-visible behaviour changes.

## [2.9.0] - 2026-07-12

### Changed
- **feat(ui): visual refresh — design system, light & dark themes, new brand mark, refreshed app icons.** Replaces the ad-hoc 11-token CSS with a full token system (color, spacing, radius, type, motion). Adds a `<Logo>` Svelte component (`src/lib/components/Logo.svelte`) used in Dashboard header, About, and Onboarding chrome. Adds light & dark themes with a header toggle (round icon button) and a Settings → Appearance picker; `+layout.svelte` side-effect-imports the theme store so persisted themes apply on cold start regardless of which route mounts first. Status badges (Spotify / Teams / Syncing) gain a leading dot, an animated pulse on the active sync state, and a dedicated accent variant. Focus rings moved to `:focus-visible`. Custom scrollbars (WebKit + Firefox). All six components re-skimmed — card padding & border-radius are now sourced from tokens instead of being copy-pasted, log-level filter replaced with a segmented control, Onboarding gets an inline device-code display and the Spotify step supports a manual URL-paste backchannel.
- **feat(ui): regenerate app icons from a single source-of-truth SVG.** New `static/icon.svg` (Spotify-green EQ inside a Teams-purple "presence" pill on midnight background) is fed to `npx tauri icon` which regenerates the full matrix — PNG/ICO/ICNS/iOS/Android — under `src-tauri/icons/`. The previously-shipped amber/teal yin-yang icon is gone. `static/icon.svg` + `static/logo.svg` (full lockup with wordmark) used in README and as the in-app SVG.

### Dependencies
- **deps(backend): bump tauri (PR #138).** Patch via Dependabot weekly batch. (Backend cargo group.)
- **deps(frontend): bump @sveltejs/kit (PR #137).** Patch via Dependabot weekly batch. (Frontend npm dev-dependency group.)

### Removed
- **chore: drop stale brand artefacts.** `logos/_Design a modern desktop app logo for PresenceJam_.png` and `logos/_Design a modern desktop app logo for PresenceJam_ 2.png` (unused AI drafts) deleted. `static/svelte.svg`, `static/tauri.svg`, `static/vite.svg` sample assets removed. README no longer references non-existent `docs/screenshots/*.png` paths.

### Security
- **fix(security): tighten tokens.json + config.json file mode to 0600 (Unix), user-only ACL (Windows) — issue #135 path A.** `src-tauri/src/token_io.rs::write_tokens_atomic` and `src-tauri/src/config.rs::atomic_write_json` now create the temp sidecar with mode 0600 atomically via `OpenOptions::new().write(true).create_new(true).mode(0o600)` (Unix); the subsequent `rename()` preserves the source mode, so the live file ends up 0600 too. Pre-existing loose files are tightened on first read by `read_tokens_at` / `load_config`. Stale `.tmp` sidecars from a prior crash are pre-cleared so `create_new(true)` does not block the next write. This is **file-mode tightening, not encryption**; the file contents remain plaintext JSON. See `SECURITY.md` "Data Storage → File permissions (v2.8.x)" for source-of-truth citations and the A-vs-B decision. Adds two regression tests: `token_io::tests::recovers_from_stale_tmp_sidecar` and `config::tests::test_atomic_write_json_recovers_from_stale_tmp_sidecar`. Existing `test_atomic_write_json_does_not_remove_destination_first` (PR #133) tightened to forbid `remove_file(path)` on the destination while allowing `remove_file(&temp_path)` on the sidecar.

## [2.8.0] - 2026-07-04

### Security
- **fix(security): re-register presencejam:// scheme at every launch (further mitigates #66).** `src-tauri/src/lib.rs` previously called `tauri-plugin-deep-link`'s `register_all()` only on Windows (`#[cfg(windows)]` gate around the existing call site). The plugin's `register` is a no-op on macOS/Android/iOS (returns `Err(UnsupportedPlatform)`) and an effective re-registration on Windows (writes `HKCU\Software\Classes\<scheme>`) and Linux (writes `~/.local/share/applications/<scheme>.desktop` and runs `xdg-mime default`). This change removes the Windows-only gate so Linux also re-registers on every launch, defending against a foreign app pre-registering `presencejam://` to hijack the Spotify OAuth callback. **Windows + Linux coverage only**; macOS remains partially mitigated by #65 (PKCE verifier in AppState only, never on disk, never exposed via IPC — an interceptor can read the `code` but cannot exchange it for tokens). Native `LSSetDefaultHandlerForURLScheme` work for macOS is tracked separately. Does not modify the OAuth `redirect_uri` or `state` parameter — no Spotify re-registration required.

### CI/Build
- **ci: refresh pinned GitHub Action SHAs across ci.yml + release.yml (PR #130).** Bumps `actions/checkout` v4.3.1 → v7.0.0, `actions/setup-node` v4.4.0 → v6.4.0, `actions/upload-artifact` v4.6.2 → v7.0.1, `actions/download-artifact` v4.3.0 → v8.0.1. SHA pinning (PR #68 policy) preserved. **`actions/download-artifact` v8 introduces `digest-mismatch` (defaults to error) — set explicitly to `warn` in both release.yml occurrences as a safer first try; switch to `error` after one clean release cycle.** `ncipollo/release-action`, `vedantmgoyal2009/winget-releaser`, `dtolnay/rust-toolchain`, `Swatinem/rust-cache` deliberately not bumped (stable).
- **deps(frontend): bump `@tauri-apps/cli` 2.11.3 → 2.11.4 (PR #127).** Patch via Dependabot weekly batch. (Frontend npm group — confirms the minor-and-patch auto-update policy.)

### Documentation
- **docs: README and SETUP install instructions no longer pin a specific release version.** Install commands and download links now reference the [latest release](https://github.com/Carme99/PresenceJam-Desktop/releases/latest) instead of stale `PresenceJam-<v>.msi` filenames. (Landed via #131.)

### Tests
- Regression guard `test_register_all_not_gated_to_windows_only` added; uses `include_str!("lib.rs")` to assert `app.deep_link().register_all()` is not gated to Windows alone.


## [2.7.5] - 2026-06-25

### Refactored
- **refactor(frontend): type the 5 invoke<any> / listen<any> call sites (PR #116, #78 part 1).** Replaces 6 untyped invoke/listen sites with typed equivalents matching the Rust-side return shapes. Closes the silent-drift risk: a Rust-side field rename now produces a TypeScript compile error, not a runtime undefined. New interfaces in `src/lib/types.ts`: `SyncStatus`, `DeviceCodeResponse`, `LogPayload`.
- **refactor: error handling consistency (PR #117, #79 items 1+2).** Three sub-changes: (1) drop the silent-failure `eprintln!` in the panic hook (`lib.rs:407`) — stderr is not connected to the user's log file on macOS release builds. (2) Add a `severity` field to the `error` event payload via a centralised `emit_error(app, source, message, severity)` helper; the polling loop's 3 error emit sites now route through it. (3) Gate the Dashboard.svelte red banner on `severity === 'error'` — warnings (transient 401-retry, backoff) no longer alarm-fatigue the user. Includes a `test_error_event_emits_severity_field` regression guard in `polling.rs`.
- **refactor(commands): split commands.rs into 7 per-workflow modules (PR #122, #76, #79).** Extracts the 24 `#[tauri::command]` handlers + 3 helpers from the single 1113-line `src-tauri/src/commands.rs` into a `src-tauri/src/commands/` directory: `config.rs`, `spotify_auth.rs`, `teams_auth.rs`, `sync.rs`, `window.rs`, `onboarding.rs`, `misc.rs`, plus a thin `mod.rs`. The `tauri::generate_handler!` macro in `lib.rs` now references the 7 submodules via globs. Closes #76. Also includes the [CMD.<GROUP>] log tag namespace rename from #79 item 3: each per-workflow file uses a `const CMD: &str = "[CMD.<GROUP>]"` so the log tag differs by command category, making the existing log_tag sweep work easier to grep.
- **refactor: extract OnboardingCache sub-struct with lock encapsulation (PR #118, #80 step 1).** Pulls the 30s onboarding result cache out of the monolithic `AppState` into its own `OnboardingCache` sub-struct. The `lock()` and `invalidate()` methods encapsulate the inner mutex; the field is private. This is the load-bearing pattern for #80 step 2 (Tokens, Polling, PendingAuths, Config).
- **refactor: extract Tokens/Polling/PendingAuths/Config sub-structs with lock encapsulation (PR #120, #80 step 2).** Continues the #80 split: 4 new sub-structs with private inner fields, lock-acquisition methods (`spotify()`, `teams()`, `handle()`, `handle_mut()`, `try_claim()`, `set_syncing()`, `current_track()`, `current_track_mut()`, `stop_tx()`, `stop_tx_mut()`, `get()`, `get_mut()`), and `Default` impls for clippy. `try_claim()` encapsulates the `compare_exchange(false, true, AcqRel, Acquire)` pattern that was raw atomic on `is_syncing` in step 1. The 38/39 pre-existing tests still pass; 3 new regression tests added.
- **refactor: extract poll_once/state/loop modules (PR #123, #72).** Extracts the 1089-line `src-tauri/src/polling.rs` into 4 files: `polling/loop.rs` (thin driver, ~50 lines), `polling/poll_once.rs` (single source of truth for one iteration, with the unified CAS-discard helper `cas_refresh_or_discard<T>` and the unified 401-retry path), `polling/state.rs` (`start_polling`/`stop_polling` thread lifecycle), `polling/mod.rs` (re-exports + `ErrorSeverity` + `emit_error`). Closes all 3 documented drift points: (1) `consecutive_pauses` is now incremented in exactly one place (the `record_no_track_outcome` helper called by both the main `Ok(None)` and 401-retry `Ok(None)` paths); (2) the `error` event is emitted in exactly one place per failed poll (the unified `Err` arm at the bottom of `poll_once.rs`); (3) the CAS-discard re-read dance is shared between Spotify proactive, Spotify 401-retry, and Teams refresh via the `cas_refresh_or_discard<T>` helper, with one canonical log message. 9 regression tests added covering all 3 drift points.
- **refactor: ts-rs build-time codegen for AppConfig + token/track/sync types (PR #121, #78 part 2).** Adds `ts-rs` v12 (`chrono-impl` feature) as a regular dependency. Derives `ts_rs::TS` and `#[ts(export, export_to = "../../src/lib/types-generated/")]` on the wire-shape structs across `spotify.rs`, `teams.rs`, `commands/sync.rs`, and `config.rs`. The generated `.ts` files land in `src/lib/types-generated/` (gitignored, regenerated by `cargo test`). `src/lib/types.ts` re-exports the generated types so existing component imports (`import type { SpotifyTokens } from '$lib/types'`) keep working unchanged. `u64` fields (`TrackInfo.progress_ms`, `TrackInfo.duration_ms`, `DeviceCodeResponse.interval`, `DeviceCodeResponse.expires_in`) override the ts-rs default `bigint` with `#[ts(type = "number")]` because Tauri's serde_json IPC bridge decodes u64 as JS number (f64) at runtime. 5 round-trip regression tests added.

### Fixed (CI)
- **ci(frontend): run cargo test --lib to materialise ts-rs codegen before svelte-check (PR #124, #125).** The `frontend` CI job now runs `cargo test --lib` (with the generated `src/lib/types-generated/` directory cleared first to avoid stale-cache issues) before `npm run check`, so the ts-rs-generated TypeScript files exist when svelte-check runs the type-re-exports from `src/lib/types.ts`. Without this, the Frontend check fails on PRs that introduce new ts-rs types because the `.ts` files are only produced when the test binary runs.

## [2.7.4] - 2026-06-25

### Security
- **deps(npm): pin cookie >= 0.7.0 via package.json overrides (GHSA-pxg6-pf52-xh8x, PR #113).** The vulnerable `cookie@0.6.0` was pulled transitively via `@sveltejs/kit@2.68.0`. A top-level `overrides` block in `package.json` forces resolution to `^0.7.0` across the entire transitive graph. Dev-only (vite/svelte-kit dev server); no production binary impact.
- **security(ci): switch homebrew job to credential helper, document 90-day PAT rotation (PR #114, #68 finish).** The `homebrew` job in `release.yml` previously cloned the tap with `x-access-token:${HOMEBREW_TAP_TOKEN}@github.com/...` — the token leaked into `git remote -v` output, the process listing, and any error log captured by the job. Replaced with a non-persistent `git config credential.helper` that hands the token to git on demand. Added a new "Release Pipeline Token Rotation" section to `SECURITY.md` documenting the rotation procedure, why fine-grained PATs (not classic), and why 90 days.

### Fixed
- **refactor(frontend): type the 5 invoke<any> / listen<any> call sites (PR #116, #78 part 1).** Replaces 6 untyped invoke/listen sites with typed equivalents matching the Rust-side return shapes. Closes the silent-drift risk: a Rust-side field rename now produces a TypeScript compile error, not a runtime undefined. New interfaces in `src/lib/types.ts`: `SyncStatus`, `DeviceCodeResponse`, `LogPayload`.

### Changed
- **refactor: error handling consistency (PR #117, #79 items 1+2).** Three sub-changes: (1) drop the silent-failure `eprintln!` in the panic hook (`lib.rs:407`) — stderr is not connected to the user's log file on macOS release builds, so the panic was invisible. (2) Add a `severity` field to the `error` event payload via a centralised `emit_error(app, source, message, severity)` helper; the polling loop's 3 error emit sites now route through it. (3) Gate the Dashboard.svelte red banner on `severity === 'error'` — warnings (transient 401-retry, backoff) no longer alarm-fatigue the user. Includes a `test_error_event_emits_severity_field` regression guard in `polling.rs`.

### Refactored
- **refactor: extract OnboardingCache sub-struct with lock encapsulation (PR #118, #80 step 1).** Pulls the 30s onboarding result cache out of the monolithic `AppState` into its own `OnboardingCache` sub-struct. The `lock()` and `invalidate()` methods encapsulate the inner mutex; the field is private. This is the load-bearing pattern for #80 step 2 (Tokens, Polling, PendingAuths, Config). Includes 2 regression tests: `test_onboarding_cache_lock_and_invalidate` (exercises the public API) and `test_onboarding_cache_encapsulation_no_direct_state_access` (grep guard against re-exposing the field).

## [2.7.3] - 2026-06-25

### Security
- **fix: per-install keychain namespacing (audit M2).** `keychain.rs:19-34` — `SPOTIFY_CLIENT_SECRET_USER` is now namespaced by the Tauri bundle identifier (`spotify_client_secret:com.presencejam.app`). Side-by-side installs on the same OS user (prod, dev, beta) now get isolated slots. `get_spotify_client_secret` falls back to the legacy unnamespaced slot used through v2.7.2, migrates the value forward to the namespaced slot, and deletes the legacy entry — so existing v2.7.2 users do not have to re-onboard. `has_spotify_client_secret` and `delete_spotify_client_secret` consult both slots (legacy delete is best-effort).
- **fix: strip plaintext Spotify client_secret from config.json on startup (audit Q3).** `config.rs` adds `migrate_legacy_client_secret()`, called from `lib.rs:441` after `load_config`. If `config.json` contains a plaintext `spotify.client_secret` field (legacy ≤ v2.5.0), the value is written into the OS keychain and the plaintext is atomically stripped from the file. Conflict policy: if the keychain already holds a *different* secret, the migration is a no-op (the user is told to Reconnect via Settings) so a multi-install upgrade cannot clobber a working keychain entry.

### Fixed
- **fix(polling): count `SpotifyApiError::Other` toward `transient_failure_count` (audit M1).** `polling.rs:871-886` — the 5-strikes exit-to-reconnect-required previously only counted `RateLimited` and `ExpiredToken`. A reqwest send failure (DNS, TLS handshake, connection refused) or a non-200/204/401/429 HTTP response is wrapped into `Other`; that variant is now treated as transient so a permanent network outage eventually triggers `reconnect-required` instead of looping forever emitting `error` events.
- **feat: actionable keychain error on Linux (audit Q7).** `keychain.rs` adds `keychain_error_help()` / `map_keychain_err()` helpers that wrap every `Entry::new` / `set_password` / `delete_credential` call site. When the keychain is unavailable (no Secret Service daemon, locked `gnome-keyring`, missing `kwallet`), the returned error message points the user at `SETUP.md#linux-keyring` with install commands for the major distros and a `secret-tool` self-check recipe. `SETUP.md` adds a new "Linux: System Keyring Required" section documenting the dependency and the `secret-tool` smoke test. No encrypted-config fallback is added — a working keyring is a hard requirement, by design.
- **fix(frontend): `TeamsTokens.refresh_token` is `string | null` (audit S2).** `src/lib/types.ts:32` was previously typed as `string`, but the Rust side (`teams.rs:57`) declares `pub refresh_token: Option<String>` with `#[serde(default)]` and no `skip_serializing_if` — so the field is always emitted, defaulting to JSON `null` when the Microsoft token endpoint doesn't return one. TS was lying about the wire shape. Spotify's `refresh_token` is a plain `String` on both sides and remains `string`. No frontend code currently reads `.refresh_token` directly, so this is a latent-bug fix with zero call-site impact.
- **fix: forward deep-link argv via single-instance plugin (audit S5).** `lib.rs:340-362` — the `tauri_plugin_single_instance` callback was a no-op log line. Now it (1) raises, un-minimizes, and focuses the existing main window when a second instance launches, and (2) scans `argv` (skipping argv[0] = exe path) for any `presencejam://` URL and forwards it through `handle_deep_link`. macOS deep-link delivery via the plugin's `on_open_url` callback was already wired and is unaffected.

### Changed
- **chore(deps): drop unused `tauri-plugin-process` (audit Q6).** The plugin was registered in `lib.rs:372` and declared in `Cargo.toml:28` and `package.json:23`, but no frontend code imports `@tauri-apps/plugin-process` and no Rust code calls into the plugin's IPC. All three registration points and the `ACKNOWLEDGEMENTS.md` table entries have been removed. No behaviour change; pure attack-surface reduction.
- **feat(macOS): hide dock icon when Start minimized is on (audit Q4).** `lib.rs` setup now calls `app.set_activation_policy(ActivationPolicy::Accessory)` when `start_minimized` is true, and `commands::save_config` does the same on every save (symmetrically setting `Regular` when the user disables it) so the dock icon and menu-bar app menu disappear for tray-only use. No restart required to re-enable the dock icon. The change is `#[cfg(target_os = "macos")]`-gated; Windows and Linux are unaffected.
- **chore: User-Agent version from `CARGO_PKG_VERSION` (audit Q8).** `teams.rs:52` now uses `format!("PresenceJam/{}", env!("CARGO_PKG_VERSION"))` instead of the hardcoded `"PresenceJam/2.0"`, which had drifted since v2.0.0. `CONTRIBUTING.md` adds the rule "User-Agent and any version-stamped payload must use `env!("CARGO_PKG_VERSION")` — never hardcode the version." Spotify's `reqwest::blocking::Client` is unaffected — it doesn't set a User-Agent at all (pre-existing gap, out of scope).

### Chore
- **chore(nits): three mechanical cleanups.** `tauri.conf.json:30` drops the unused `https://api-secure.spotify.com` from the CSP `connect-src` (no Rust code calls it). `teams.rs:78` drops the redundant `serde(rename = "verification_uri")` on `DeviceCodeResponseRaw.verification_uri` (the field is already named that; only the `alias = "verification_url"` is doing real work). New `teams::is_token_expired(&TeamsTokens)` mirrors `spotify::is_token_expired`; the two inline `Utc::now() [<>]= expires_at - 60s` checks at `polling.rs:131` and `teams.rs:548` now call the helpers (the polling.rs import is aliased `is_token_expired as is_teams_token_expired` to avoid shadowing the Spotify one).

### Verified (no code change)
- **Verified: keychain cache priming at startup is race-free (audit Q1).** `lib.rs:406-415` reads the keychain on startup to populate the `OnceLock<RwLock<Option<String>>>` cache before the polling thread's first iteration. Because `keychain::store_spotify_client_secret` (called from `start_spotify_auth` during onboarding) also writes the cache on success, there is no race window where the polling thread could see a stale empty cache after onboarding completes. Left as-is.
- **Verified: tray Show/Hide label regression is fixed (audit Q5).** `tray.rs:158-211` already includes `is_window_visible` in the dedup key (`TrayStateSnapshot = (bool, bool, Option<String>)` for `(is_syncing, is_window_visible, track_key)`); `update_tray_menu` reads `window.is_visible()` at line 200 before computing the label. Per v2.6.4 #71 fix. No code change needed.

## [2.7.2] - 2026-06-20

### Fixed
- **fix(v2.7.2): verifier-flagged nits + release-hygiene catch (#92).** `Settings.svelte:73` log tag rename (`[SETTINGS] start_spotify_auth failed:` → `[SETTINGS] start_spotify_reconnect failed:`) — the catch block is for the reconnect-required listener (added in v2.7.1), not the original auth. `commands.rs:298` idiomatic `let _ = client_secret;` → `_client_secret` at declaration. `Cargo.lock` presence-jam version 2.7.0 → 2.7.1 (missed by the v2.7.1 release commit 856b613).

### Changed
- **chore(v2.7.2): a11y fix + project-wide rustfmt pass (#93).** `Settings.svelte:195` orphan `<label>` (no associated control — the client secret is stored in the keychain) changed to `<span class="form-label">` with a matching CSS rule. Project-wide rustfmt cleanup (was failing on `lib.rs:389/393/396` trailing whitespace — pre-existing rustfmt 1.9.0 internal bug; once stripped, rustfmt's backlog of legitimate reformatting was unblocked). Mechanical reformatting only: import reordering, long log/if/chain calls broken to multi-line. `cargo fmt --check` is now clean for the first time.

### Refactor
- **refactor: delete frontend dead stores, extract shared types (#91).** `src/lib/stores/spotify.ts` and `src/lib/stores/teams.ts` were writable stores that nothing ever wrote to. They also exported three type definitions (`SpotifyTokens`, `TrackInfo`, `TeamsTokens`) used in 4+ places. Extracted the types to new `src/lib/types.ts` and deleted the dead writables. `Settings.svelte` catch-block fallback (`isConnected = $spotifyConnected`) was always reading the never-written `false` default — replaced with explicit defaults.

### CI/Build
- **feat(ci): add Linux (.deb + .AppImage) to release matrix (#94).** Release workflow now builds Debian/Ubuntu (.deb) and AppImage artifacts alongside the existing macOS DMG and Windows MSI. `ubuntu-22.04` runner; Tauri 2's `tauri build` produces both formats in one invocation. No signing required on Linux (unlike macOS Gatekeeper / Windows SmartScreen). Skipped: .rpm, flatpak, snap, arm64. README updated with install instructions and the macOS unsigned-DMG Gatekeeper workaround note.

## [2.7.1] - 2026-06-19

### Fixed
- **fix: Reconnect Spotify flow was permanently broken (#88).** `Reconnect.svelte` and `Settings.svelte` were calling `start_spotify_auth` with `clientSecret: ''`, expecting the backend to read from the keychain. The #67 validator (≥32 chars) correctly rejected the empty string, so the Reconnect button was non-functional in shipped v2.6.4 / v2.7.0 builds. Fix: new `start_spotify_reconnect(client_id, redirect_uri)` IPC that reads the existing `client_secret` from the OS keychain and runs the same PKCE flow. The shared OAuth flow was extracted into a private `run_spotify_oauth_flow` helper so `start_spotify_auth` (writes secret to keychain) and `start_spotify_reconnect` (reads from keychain) share one implementation.

## [2.7.0] - 2026-06-19

### Refactor
- **refactor: dedup auth-listener setup across 3 Svelte components (#73).** Extracted a shared `useAuthListeners()` helper and an `authFlow` Svelte 5 reactive store to deduplicate the 4-event listener block (spotify-auth-complete/failed, teams-auth-complete/failed) across `Onboarding.svelte`, `Settings.svelte`, and `Reconnect.svelte`.
- **refactor: single source of truth for status-format placeholder substitution (#74).** Moved the placeholder substitution for the status-format template into Rust so the Svelte live preview and the runtime polling loop share one implementation.
- **refactor: extract pkce module, dedup helpers (#75).** Extracted the PKCE challenge/verifier generation logic from `spotify.rs` into a dedicated `pkce.rs` module.

### CI/Build
- **ci: pin third-party action SHAs and document token scoping (#68).** All third-party GitHub Actions are now pinned to full-length commit SHAs instead of version tags. Documented the required token scopes for `HOMEBREW_TAP_TOKEN` and `WINGET_TOKEN`.
- **ci: run cargo test and npm check on PRs (#81).** Added a `ci.yml` workflow that runs `cargo test`, `cargo clippy`, and `npm run check` (Svelte type-check) on every PR.

### Deferred
- **#66 (deep-link hijack)** remains deferred to v2.7.1. Per-launch custom-scheme registration (OS-specific) is required for the full fix. Partial mitigation from #65 (PKCE verifier in `AppState` only) is still in place.

### Follow-ups (not addressed here)
- **Reconnect Spotify flow.** `Reconnect.svelte` and `Settings.svelte` pass `clientSecret: ''` which the #67 validator rejects. Pre-existing bug, not introduced by this release.
- **Frontend dead stores.** `src/lib/stores/spotify.ts` and `src/lib/stores/teams.ts` are still present. Tracked separately.

## [2.6.4] - 2026-06-14

### Security
- **security: tokens.json security boundary (#65).** Dropped `"store:default"` from `capabilities/default.json` — the webview no longer has any path to the tokens file. Deleted the `get_spotify_tokens` and `get_teams_tokens` Tauri commands (registered but unused, and a token-exfil endpoint in waiting). Stopped persisting `pending_spotify_auth.verifier` and `pending_teams_auth.verifier` (PKCE verifier + Teams device code) to disk — both are 10–15 min bearer credentials that filesystem-level attackers could read. Replaced `tauri-plugin-store` token I/O with a new `token_io` module that writes `<app-config-dir>/PresenceJam/tokens.json` **atomically** using temp-file + rename + fsync (mirroring the `config::save_config` pattern). A process kill mid-write can no longer corrupt the tokens file and bounce the user back through Onboarding.
- **security: backend input validation at IPC boundary (#67).** Moved the Spotify `client_id` (`^[A-Za-z0-9]{32}$`) and `client_secret` (≥32 chars) regex checks from the frontend to `start_spotify_auth` — a devtools-pasted `invoke()` with arbitrary strings was previously accepted. Hardened `validate_http_url` to reject URLs with no host and with `userinfo` (`user:pass@`). Deleted the non-manual `complete_spotify_auth` command — it accepted `verifier`/`client_id`/`redirect_uri` from the webview unverified, and the manual variant covers all real flows.
- **security: dead Tauri commands + frontend stores removed (#77).** Deleted 7 commands that were registered but never invoked: `open_external` (alias of `open_external_url`), `hide_window`, `get_autostart_enabled`, `get_recent_logs`, `get_config_dir`, `get_current_track`, `complete_teams_auth_manual`. The frontend stores `spotify.ts` and `teams.ts` (writables that nothing ever wrote) are still present — see follow-up below.

### Fixed
- **fix: polling state machine races (#69).** `start_syncing` now drains the previous polling thread via `stop_polling_and_join` **before** claiming the `is_syncing` flag. A fast Stop+Start cycle (within the 2s stop budget) can no longer leave a stale thread running while a new one starts. The OS keychain is no longer hit on every polling iteration: `keychain::get_spotify_client_secret` now caches the secret in a module-level `OnceLock<RwLock<Option<String>>>`, and the polling thread's hot path uses a new `peek_spotify_client_secret` (cache-only). The cache is primed once at app start, eliminating the macOS keychain prompt mid-poll.
- **fix: onboarding cache was never invalidated (#70).** The 30s result cache for `is_onboarding_complete` is now cleared in every token-mutating command (`complete_spotify_auth_manual`, `poll_teams_auth`, `complete_teams_auth_manual`, `reconnect_spotify`, `reconnect_teams`, `handle_spotify_callback`, `handle_teams_callback`). The user is no longer told "onboarding not complete" for up to 30s after a successful reconnect.
- **fix: tray menu concurrent rebuilds (#71).** `update_tray_menu` now skips the full menu rebuild if `is_syncing` and the current track key haven't changed (the polling thread called this on every successful poll; the menu only needs to change when the state actually changes). A module-level `Mutex<()>` serialises the two writers (polling thread + frontend command) so a `set_menu` from one never interleaves with the other.

### Deferred
- **#66 (deep-link hijack) was deliberately not fully fixed in this release.** A per-launch UUID in the redirect URI path was the proposed defence, but Spotify requires exact redirect-URI match in the registered app — a path component breaks the OAuth round-trip. A full fix needs per-launch custom-scheme registration (OS-specific: Windows registry, macOS `LSSetDefaultHandlerForURLScheme`, Linux XDG MIME). The threat is **partially mitigated** by #65: a foreign app that intercepts `presencejam://callback?code=***` can read the code, but cannot exchange it for tokens — the PKCE verifier is in `AppState` only, not on disk, and not exposed via any IPC. Tracked for a follow-up release.

### Follow-ups (not addressed here)
- Delete `src/lib/stores/spotify.ts` and `src/lib/stores/teams.ts` (writables that nothing writes). The verifier flagged these as still present; they're cosmetic dead code at this point, not a security risk. Tracked separately.
- Update `Settings.svelte:7-8` to drop the `import { spotifyConnected } from '$lib/stores/spotify'` and the dead `isConnected = $spotifyConnected` fallback in the catch block. Tracked separately.

## [2.6.3] - 2026-06-10

### Fixed
- fix(race): drop the double-claim on `is_syncing` between `commands::start_syncing` and `polling::start_polling`. Fresh installs could not complete onboarding — every Finish click after a successful Spotify + Teams auth hit `"Polling is already running"` and rolled back to no-sync state. `commands::start_syncing` is now the **sole claimer** of the flag; `polling::start_polling` is a pure thread-spawner that trusts its caller. A source-grep regression guard (`test_start_polling_does_not_claim_is_syncing`) catches a re-introduced `compare_exchange` inside `start_polling`. Closes #60.
- fix(autostart): gate `disable()` on `is_enabled()` to swallow the "key not present" case. On Windows, `RegDeleteValueW` on a missing Run-key entry returned `os error 2` on every `save_config` call when autostart was never enabled (or was removed externally). The plugin no-op path now returns `Ok(())` with an `info!` log and never calls the registry. Closes #61.
- fix(security): truncate the Microsoft Graph token-poll body in debug logs. The full token response (access_token + refresh_token, ~3.5 KB, ~77 min lifetime, `Presence.ReadWrite` + 50+ scopes) was previously written to the log file at `debug!` level. The `truncate_for_log` helper now records only the first 256 chars + byte count — enough to recognise the error envelope shape, not the credential. The truncation is applied at every `raw_body` interpolation in `teams.rs`: the three success-path debug logs, the three `info!`-level request-body logs, and the eight user-facing error format strings used by token parse failures, device-code failures, and unknown error fallbacks. A second pass hardened the helper against the UTF-8 panic risk at byte 256 (now cuts at a char boundary via `char_indices().nth(256)`), with four unit tests covering the under-limit, ASCII-boundary, multibyte-boundary, and truncation cases. Closes #62.
- fix(ui): make the build version reflect the actual build in both the About panel and the main page footer. The Vite `define` used a bare `__APP_BUILD__` token that esbuild's `define` plugin only matches as a top-level identifier; the consumers read `import.meta.env.__APP_BUILD__` (a member expression), which esbuild never matched against the bare-token define. Result: both surfaces had been hardcoded-fallback'd since 2.6.0 (the v2.6.1 "fix" was a prettier fallback string, not a real fix). Switched to the canonical `import.meta.env.VITE_APP_BUILD` path-based define, swapped `||` for `??` in both consumers (so an empty version doesn't fall through to the dev-build label), and used `'dev build'` as the footer fallback (more honest than the previous hardcoded `'2.6.0'`). The build version is also logged to the DevTools console at boot for faster stale-install triage. Closes #63.

Closes #60 #61 #62 #63

## [2.6.2] - 2026-06-09

### Chore
- chore(release): re-submit to winget. v2.6.1 winget submission got rejected at validation because the manifest pointed to the v2.6.0-named MSI (deleted when the v2.6.1 MSI was uploaded). The winget-releaser action's `komac update` skips on ANY existing PR for the version (open or closed), so closing the broken PR didn't help — only a version bump produces a fresh submission. No source changes; identical binaries as v2.6.1.

## [2.6.1] - 2026-06-09

### Fixed
- fix(ui): show build date in About panel as ISO date instead of Unix epoch milliseconds. The previous `${pkg.version}.${Date.now()}` format produced strings like `2.6.0.1749350400000` in the About panel; now shows `2.6.1 (2026-06-09)`.

### Chore
- chore(docs): condense the stale `retention_days` historical NOTE in `src/lib/stores/config.ts` from 4 lines to 1. Field was removed in v2.6.0 (PR #49, GH #13); the 4-line block was carrying its own weight as removed code rather than a note.

## [2.6.0] - 2026-06-08

### Security
- fix(security): use HTTPS for Authenticode timestampUrl (#28, PR #44)
- chore(security): strip 12+ unused Tauri capabilities (#29, PR #44)

### Fixed
- fix(race): Spotify token-refresh lost-update race (#35, PR #43)
- fix(race): Teams token-refresh lost-update race (#36, PR #43)
- fix(race): apply CAS guard to 401-retry refresh (#PR #43)
- fix(polling): close if_committed block in 401-retry CAS guard (PR #43 follow-up)
- fix(config): wire polling interval config fields; remove dead retention_days (#37, PR #45)
- fix(polling): thread configured default_interval_seconds into pause_backoff (PR #45 follow-up)
- fix(polling): reset consecutive_pauses before debounce early-return on track resume (PR #45 follow-up)
- fix(ux): is_first_poll guard prevents legitimate presence-clearing on app start (#39, PR #45)
- fix(lifecycle): wire polling-thread-panicked and reconnect-required listeners in frontend (#33, PR #44)
- fix(oauth): re-check pending auth expiry at submit time (#34, PR #44)
- fix(perf): use spawn_blocking for OAuth callback HTTP calls (#42, PR #46)
- fix(perf): distinguish panic from cancellation in spawn_blocking error messages (PR #46 follow-up)

### Changed
- refactor(state): use AtomicBool for is_syncing flag (#32, PR #44)
- refactor(cmd): make is_onboarding_complete async with 30s result cache (#41, PR #47)
- perf(api): pause-aware exponential backoff in polling loop (#38, PR #45)
- perf(api): validate_*_token checks local expires_at before network call (#40, PR #45)
- refactor(cmd): extract ONBOARDING_CACHE_TTL as a named constant (PR #47 follow-up)

### Chore
- chore(deps): drop unused tokio dependency (#30, PR #44)
- chore(build): commit Cargo.lock for reproducible builds (#31, PR #44)

### Cleanup (outstanding issues)
- chore(security): migrate Spotify client_secret to OS keychain (closes #9, PR #49)
- chore(logging): demote entry/exit logs to debug level (closes #12, PR #49)
- fix(config): remove retention_days from Rust/TS (closes #13, PR #49)
- refactor(commands): consolidate get_*_tokens via shared helper (closes #16, PR #49)

## [2.5.0] - 2026-05-30

### Fixed

- Teams refresh token rotation — now preserves existing refresh token when Microsoft doesn't return a new one
- Config atomic write on Windows — uses backup-and-rename pattern instead of unsafe remove-then-rename
- Polling config defaults aligned between TypeScript (10s) and Rust (was 5s, now 10s)
- Manual URL paste error now displays to user instead of silently spinning
- Polling config interval fields removed (were dead code — engine uses hardcoded constants)
- Over-permissioned Tauri capabilities — 5 unused permissions removed
- Version string in vite.config.js now reads from package.json dynamically
- Clippy warnings fixed (needless_borrow, manual_clamp x2)
- Quit handler now logs exit failure instead of silently discarding Result
- Stale config.tmp files cleaned up on startup
- Auth callback expiry now checked at callback time (defense-in-depth)
- Token length removed from production logs (downgraded to debug)
- Clear-on-pause now sends "🎵 Paused" placeholder instead of empty string
- Tray menu builds partial menu on item failure instead of aborting entirely
- npm audit: 5 vulnerabilities fixed (2 high, 2 moderate, 1 low)

### Improved

- Tray menu separator logic simplified — no double separators when idle
- User-Agent now reports actual app version from CARGO_PKG_VERSION
- Auth callback expiry check added for both Spotify and Teams
- AssertUnwindSafe in polling loop now has safety comment
- Log retention enforcement added — deletes logs older than configured retention_days

### Removed

- Dead Credentials struct and load/save functions (~132 lines)
- Dead start_spotify_auth() function (unused — command generates its own PKCE)
- Dead frontend token stores (spotifyTokens, teamsConnected) and unused Settings imports

### Security

- Capabilities reduced to minimum required permissions
- Token metadata no longer logged at info level


## [2.4.2] - 2026-05-05

### Fixed

- **is_first_poll guard brace placement** — tightened indentation of the `if !is_first_poll` block around `clear_teams_status_message` in `process_track` so the control flow is readable
- **Inconsistent event payload conventions** — standardised all polling-path event emissions (`reconnect-required`, `spotify-reconnect-required`, `teams-reconnect-required`) to use `serde_json::json!(null)` instead of bare `()` tuples, matching the existing `polling-thread-panicked` payload shape
- **is_syncing wedge on spawn failure** — `start_polling` now resets `is_syncing` and clears `stop_tx` in the `map_err` path when `thread::Builder::spawn` fails, preventing future `start_polling` calls from being permanently rejected

- **B1: Flicker on startup** — `handle_no_track` now skips on first poll to avoid clearing Teams status before any track was ever set; sends "🎵 Nothing playing on Spotify" instead of empty string
- **B3: "Unknown" status message** — `clear_teams_status_message` now receives a human-readable placeholder text instead of empty string
- **B4: Duplicate polling thread** — `start_polling` now guards against spawning a duplicate thread using a `stop_tx.is_some()` check in addition to the `is_syncing` lock
- **is_onboarding_complete: Non-401 errors treated as valid** — RateLimited (429) → valid; all other errors → invalid (was incorrectly treating all errors as valid)
- **complete_onboarding: Missing tokens accepted** — now returns error if Spotify or Teams tokens are missing instead of always succeeding
- **refresh_spotify: State updated after persistence** — AppState tokens now updated before persistence; if persistence fails the error is returned rather than silently consumed
- **onboarding Teams auth: stale error not cleared** — `connectTeams` now clears `teamsAuthError` before starting new auth; `pollTeamsAuth` now guards against empty `teamsDeviceCode`
- **R2: Infinite retry on transient failures** — polling loop now tracks `transient_failure_count` (max 5); after 5 consecutive 5xx/network failures, exits and emits reconnect-required event
- **R5: Polling thread panic kills loop silently** — `start_polling` now wraps the polling loop in `panic::catch_unwind` with proper downcast logging; emits `polling-thread-panicked` event on panic so the frontend can react
- **is_first_poll never flipped on track-playing first poll** — `is_first_poll` flag now reset to `false` on `Ok(Some(track))` path as well, ensuring `clear_on_pause` guard fires correctly from the first poll onward
- **transient_failure_count incremented for non-transient errors** — counter now only increments for `RateLimited` and `ExpiredToken` variants; auth/Other errors are non-transient and do not contribute to the retry limit
- **transient_failure_count not reset on Ok(None)** — counter now resets to 0 in the `Ok(None)` arm so any successful poll (even with no track) breaks the failure streak
- **Auth errors not displayed to user in connectSpotify** — `connectSpotify` catch block now sets `spotifyAuthError` so backend errors like missing credentials are visible to users
- **Missing closing paren in redirect URL placeholder** — `Onboarding.svelte` redirect URL input placeholder now correctly ends with `...)"` instead of truncated `"`
- **R7: Empty client_id/client_secret accepted** — `start_spotify_auth` now rejects empty credentials early with a clear error
- **Redirect URI: wrong URL in onboarding instructions** — onboarding step 3 now says `presencejam://callback` instead of `http://localhost:43210/callback`
- **HTTP body errors silently dropped** — `unwrap_or_default()` on `response.text()` in `spotify.rs` and `teams.rs` now properly propagated as errors instead of discarded
- **Spotify auth errors silently swallowed** — `handleManualUrlPaste` and `connectSpotify` now display errors to user via `spotifyAuthError`; stale errors cleared on retry

### Security (assessed — no code changes)

- **S2: CSP unsafe-inline** — required for Svelte scoped styles; cannot be removed without rearchitecting CSS handling
- **S3: redirect_uri sent from frontend** — always sends `presencejam://callback` which is in the allowlist; defended
- **S4: OAuth scopes hardcoded** — validated by Spotify's OAuth server at runtime; low risk for desktop app
- **L1: Teams device code flow CSRF** — device code flow is CSRF-resistant per NIST 800-63C; not applicable
- **L2: PKCE verifier in tauri-plugin-store** — store uses OS keychain encryption (DPAPI/Keychain); acceptable
- **L3: open_logs_folder file://** — path is `app_log_dir()` (OS-controlled app directory); not user-controlled

### Already correct (confirmed)

- **R3: pending_auth lost on crash** — already wired in `lib.rs`; auth state survives page reload
- **R4: profanity_filter not wired** — already confirmed wired; `process_track` calls `filter_profanity()`
- **R6: polling drift** — both Spotify and Teams polled in same single loop; cannot drift independently

## [2.4.1] - 2026-04-28

### Fixed

- Settings: reconnect buttons now properly trigger auth flow when clicked (were silently failing before)
- Settings: show "Reconnecting..." state while auth is in progress
- Polling: Teams token now auto-refreshes when expired (similar to existing Spotify behavior)
- Dashboard: "Go to Setup" checks for both client_id AND client_secret before routing to reconnect vs onboarding
- Reconnect: checks for both credentials before allowing reconnect; auto-start only when credentials exist
- Version: corrected version strings to 2.4.1

## [2.4.0] - 2026-04-27

### Added

- `SETUP.md` — First-time setup guide: installing the app, registering a Spotify Developer app, connecting Teams, file locations, uninstalling
- `USAGE.md` — Day-to-day guide covering the system tray, dashboard, settings, log viewer, and common tasks

### Changed

- `README.md` rewritten as entry point — shorter, no duplication, links to all docs
- `CONTRIBUTING.md` — project structure section removed (now in ARCHITECTURE.md), references updated
- `CLAUDE.md` — stripped to conventions and key files only; duplicated tech stack and feature list removed

### Removed

- `BUGFIX_TRACKER.md` — moved to GitHub Issues; no longer ships with the repo

### Fixed

- Polling: interruptible stop channel replaces thread::sleep — tray freeze on pause is now instant (fixes #10)
- Polling: `get_sync_status` validates tokens via real API calls, emits reconnect-required events on failure (fixes #11, #12)
- Config: `clear_on_pause` now respected — Teams status only cleared when Spotify pauses if user enabled it (fixes #4)
- Config: `start_minimized` properly wired to Rust `TeamsConfig`, app window hides on startup when set (fixes #5)
- Config: `launch_at_login` moved to `AppConfig.autostart`, binding fixed to `localConfig.autostart`, syncs to OS autostart manager (fixes #6)
- Polling: refreshed Spotify tokens now persisted to tauri-plugin-store so they survive app restarts (fixes #8)
- Deep links: Windows deep link registration now runs in release builds (was debug-only) (fixes #9)
- Onboarding: `is_onboarding_complete` validates tokens via API calls instead of just checking presence (fixes #10, #12)
- Tray: initial tray menu now reflects actual sync state via immediate `update_tray_menu` call (fixes #11)
- Sync: `start_syncing` TOCTOU race fixed by acquiring write lock before checking `is_syncing` (fixes #14)
- OAuth: redundant `client_id` removed from `refresh_spotify_token` form body (was doubled in Basic auth) (fixes #19)
- Tray: tray menu updated after each track change via polling loop call to `update_tray_menu` (fixes #24, #25)
- Tray: `update_tray_menu` errors now logged at warn! level instead of silently ignored (fixes #7)
- Polling: `progress_ms` corrected by elapsed time since last poll to prevent stale position data (fixes #13)
- Onboarding: placeholder OAuth URL replaced with real Spotify app setup instructions (fixes #15)
- Frontend: Dashboard now handles `sync-started`/`sync-stopped` events from backend (fixes #16)
- Polling: 500ms debounce added to prevent Teams status flicker on rapid track changes (fixes #17)
- Quit: redundant quit handler thread removed — `on_window_event(CloseRequested)` handles it (fixes #18)
- LogViewer: wired to `tauri_plugin_log` Webview target, listens on `log://log` event (fixes #21)
- Auth: pending Spotify/Teams auth state persisted to store with expiry, recovered on startup (fixes #22, #23)
- Config: `save_config` now holds config write lock for entire read-modify-write to prevent race (fixes #26)

### Developer

- Frontend: removed unused `isSyncing` store from `app.ts` — stores are actively used, not dead code (note: Bug 20 stores are actually the navigation backbone and were correctly preserved)

## [2.3.7] - 2026-04-27

### Fixed

- Spotify auth: crash recovery now properly restores pending OAuth state from persistent store on app restart (fixes #1)
- Teams auth: proactively refresh Teams token before use in polling loop to avoid 401 errors mid-session (fixes #4)
- Config: `start_minimized` now properly wired — app window hides on startup when configured (fixes #7)
- Config: `clear_on_pause` now functional — respects user setting when pausing Spotify playback (fixes #6)
- Onboarding: Spotify client ID/secret now validated before initiating auth (min 20 chars) (fixes #19)

### Developer

- Frontend: replaced ~130 verbose `console.log` calls with `devLog()` utility that only outputs in development builds — production builds are no longer polluted with dev traces

## [2.3.6] - 2026-04-24

### Added

- Application menu bar with File, Edit, View, Help menus (macOS/Windows)
- Dynamic tray menu that updates based on sync state (Pause/Resume toggles automatically)
- Build number displayed in bottom-right corner of app window (injected at build time)
- About dialog accessible from Help menu

### Fixed

- Duplicate tray icon: removed automatic tray icon from tauri.conf.json (was conflicting with manual setup)
- Tray icon now properly displays in macOS menu bar
- Left-click tray: now shows window via tray-click event (not automatic menu popup)
- Tray menu label now refreshes after show/hide toggle
- Back-to-back separators fixed when no track is playing

### Changed

- `update_menu_state` command renamed to `update_tray_menu_state` for clarity

## [2.3.5] - 2026-04-23

### Fixed

- Teams auth: `pending_teams_auth` now populated during device code flow, fixing `complete_teams_auth_manual` (closes #8)
- Onboarding: `is_onboarding_complete` now checks both Spotify and Teams auth status (closes #11)
- URL validation: `open_external_url` and `open_external` now reject non-http(s) schemes (closes #14)
- Polling: retry intervals now include +/- 20% jitter to prevent thundering herd (closes #17)
- Polling: HTTP requests have 10s timeout, stop_syncing bounded to 2s (partial fix for #10)

### Security

- CSP: replaced broad `*.microsoft.com` wildcard with explicit `login.microsoftonline.com` and `graph.microsoft.com` (closes #15)
- URL commands: only http/https schemes allowed, preventing javascript: and file: attacks

### Documentation

- Token loading duplication in commands.rs documented as intentional (closes #16 - won't fix)

## [2.3.4] - 2026-04-12

### Fixed

- Memory leak: event listeners not cleaned up on component unmount (Onboarding)
- Polling: 401 responses now trigger token refresh instead of silent failure
- Config: default max polling interval now 60s (was incorrectly set to 10s)
- Config: atomic file writes prevent data loss on save failure
- Config: Windows rename fix — removes destination file before rename to prevent failure when file exists
- Auth: CSRF state now properly verified on Spotify OAuth callback
- Auth: CSRF state persisted to store for crash recovery
- Shutdown: polling thread now properly stopped on app quit
- UI: buttons no longer allow double-click during async operations
- UI: error messages now properly display instead of "[object Object]"
- Polling: stale error in retry path now correctly emitted to frontend
- Polling: `get_spotify_credentials()` helper removes credential extraction duplication
- Polling: `SpotifyApiError::RateLimited` errors get 60s backoff instead of 30s

### Security

- CSRF protection now functional for Spotify OAuth flow
- Credentials file writes now atomic (prevents corruption)

## [2.3.3] - 2026-04-12

### Fixed

- Clear validationError when connections complete and fix log placement

## [2.3.2] - 2026-04-12

### Fixed

- No functional changes — re-merged PR #4 (fix/bugfixes-2.3.1) onto updated main

## [2.3.1] - 2026-04-11

### Fixed

- Proper async cleanup of event listeners in Onboarding (prevent memory leaks)
- Validate connections before finishing onboarding
- Correct toggle bindings in Settings.svelte
- Align polling interval defaults with Rust backend
- Display error events to user in Dashboard
- Add validation in extractCodeFromUrl
- Store setTimeout reference for proper cleanup in Settings

## [2.3.0] - 2026-04-12

### Added

- Profanity filter for Spotify track status (`profanity_filter` toggle in Settings)
- Customizable profanity placeholder text (`profanity_placeholder`) with `{emoji}` token support
- Leetspeak normalization (1→i, 3→e, $→s, @→a, 0→o, etc.) and repeated-char collapse
- Word-boundary detection to avoid false positives (class, assassin, cocktail, etc.)
- Safe default placeholder: "Currently Listening to Spotify"

### Changed

- `config.teams.clear_on_pause` field restored (was dropped during serde round-trip)
- Logging: profanity filter no longer logs original profane status (security hardening)
- Polling: added TODO note about future refactor to filter raw Spotify fields before formatting

### Fixed

- `matches_at_pos` boundary tracking: was skipping arbitrary chars instead of only repeated chars
- `at_end` match: now validates left boundary before detection (peacock/cock false positive)
- Config test: uses `profanity::safe_placeholder_default()` instead of hardcoded literal
- Frontend TS config: added NOTE that Rust is canonical source for placeholder default

## [2.2.0] - 2026-04-11

### Added

- macOS support (Apple Silicon build)
- GitHub Actions CI/CD pipeline for automated releases
- Automatic builds for Windows (.msi) and macOS (.app zip) on tag push

### Changed

- Version bumped to 2.2.0 for macOS release

## [2.1.0] - 2026-04-11

### Fixed

- Config persistence: Load saved Spotify/Teams tokens and config on app startup
- Reconnect flow: Added `reconnect_spotify` and `reconnect_teams` commands to re-authenticate
- Reconnect now properly clears persisted tokens from store (not just in-memory state)
- `get_sync_status` now validates client_id presence
- Settings UI: Removed non-functional Teams Client ID field
- Spotify redirect_uri normalized to `presencejam://callback` in frontend config
- Emit error handling in reconnect commands (errors are now logged vs silently ignored)

## [2.0.0] - 2026-04-09

### Added

- Tauri 2 + Svelte 5 + TypeScript rewrite (completely new codebase)
- System tray integration (minimize to tray, tray menu with Show/Pause/Quit)
- Deep link support (`presencejam://callback`) for OAuth flows
- Spotify PKCE OAuth authentication
- Microsoft Teams Device Code flow authentication
- Smart polling that sleeps until track ends (no wasted API calls)
- Automatic status clearing when Spotify pauses/stops
- Token refresh handling for both Spotify and Teams
- Configurable status format with `{artist}`, `{track}`, `{album}`, `{emoji}` placeholders
- Polling interval configuration
- Launch-at-login via Windows registry
- Single-instance enforcement (prevents multiple app windows)
- Log viewer in-app (daily rotating logs, 30-day retention)
- Per-window CSP policy

### Changed

- Completely new desktop app architecture (Rust backend + Svelte frontend)
- Status message expiry set to track end time + 10s buffer
- Teams device code auth instead of browser redirect
### Removed

- PowerShell script version — this is a full rewrite
[Unreleased]: https://github.com/Carme99/PresenceJam-Desktop/compare/v4.6.0...HEAD
[4.6.0]: https://github.com/Carme99/PresenceJam-Desktop/compare/v4.5.2...v4.6.0
[4.5.2]: https://github.com/Carme99/PresenceJam-Desktop/compare/v4.5.1...v4.5.2
[4.5.1]: https://github.com/Carme99/PresenceJam-Desktop/compare/v4.5.0...v4.5.1
[4.5.0]: https://github.com/Carme99/PresenceJam-Desktop/compare/v4.4.0...v4.5.0
[4.4.0]: https://github.com/Carme99/PresenceJam-Desktop/compare/v4.3.0...v4.4.0
[4.3.0]: https://github.com/Carme99/PresenceJam-Desktop/compare/v4.2.1...v4.3.0
[4.2.1]: https://github.com/Carme99/PresenceJam-Desktop/compare/v4.2.0...v4.2.1
[4.2.0]: https://github.com/Carme99/PresenceJam-Desktop/compare/v4.1.1...v4.2.0
[4.1.1]: https://github.com/Carme99/PresenceJam-Desktop/compare/v4.1.0...v4.1.1
[4.1.0]: https://github.com/Carme99/PresenceJam-Desktop/compare/v4.0.0...v4.1.0
[4.0.0]: https://github.com/Carme99/PresenceJam-Desktop/compare/v3.2.0...v4.0.0
[3.2.0]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v3.2.0
[3.1.0]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v3.1.0
[3.0.0]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v3.0.0
[2.10.0]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.10.0
[2.9.1]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.9.1
[2.9.0]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.9.0
[2.8.0]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.8.0
[2.7.5]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.7.5
[2.7.4]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.7.4
[2.7.3]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.7.3
[2.7.2]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.7.2
[2.7.1]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.7.1
[2.7.0]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.7.0
[2.6.4]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.6.4
[2.6.3]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.6.3
[2.6.2]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.6.2
[2.6.1]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.6.1
[2.6.0]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.6.0
[2.5.0]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.5.0
[2.4.2]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.4.2
[2.4.1]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.4.1
[2.4.0]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.4.0
[2.3.7]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.3.7
[2.3.6]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.3.6
[2.3.5]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.3.5
[2.3.4]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.3.4
[2.3.3]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.3.3
[2.3.2]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.3.2
[2.3.1]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.3.1
[2.3.0]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.3.0
[2.2.0]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.2.0
[2.1.0]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.1.0
[2.0.0]: https://github.com/Carme99/PresenceJam-Desktop/releases/tag/v2.0.0