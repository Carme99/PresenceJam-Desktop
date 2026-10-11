# CHANGELOG COVERAGE MATRIX — v5 (`## [Unreleased]`)

Audit of `CHANGELOG.md` lines 8–161 (the `## [Unreleased]` section), one row per
bullet, against the in-scope documentation set:

`README.md`, `SETUP.md`, `USAGE.md`, `TROUBLESHOOTING.md`, `SECURITY.md`,
`ACKNOWLEDGEMENTS.md`, `CONTRIBUTING.md`, `ARCHITECTURE.md`, `AGENTS.md`,
`CLAUDE.md`, `docs/README.md`, `docs/PLATFORMS.md`, `docs/RELEASING.md`,
`docs/STATE-OF-FEATURES.md`, `docs/architecture/*.md`.

`CHANGELOG.md` itself is **not** counted as documentation. `docs/audit/**`,
`archive/**`, `docs/archive/**`, `pj-worktrees/**`, `node_modules/**`,
`target/**`, `dist/**`, `.svelte-kit/**` and root `SOUL.md`/`USER.md`/
`IDENTITY.md` are excluded.

Tree under audit: HEAD `09341ecaad732e78454a2c65b383f0dfd1541d5a`, clean.

---

## COVERAGE MATRIX

### Added (line 12)

| Feature | Line | Class | Documented in | Accurate? | Action |
|---|---|---|---|---|---|
| Five more UI locales with follow-system language (#984) | 13 | Added | docs/STATE-OF-FEATURES.md:130; AGENTS.md:327-328; CONTRIBUTING.md:159-163; docs/architecture/tray-and-shell.md:91-98 | PARTIAL | DOCUMENT — the new rows are accurate, but README.md:35, USAGE.md:176 and TROUBLESHOOTING.md:180 still advertise three locales (en/de/fr), and docs/architecture/tray-and-shell.md:58-59 still says "English, German, and French". |
| OS presentation-state gate (#872) | 14 | Added | docs/STATE-OF-FEATURES.md:27, :80, :136 (the `presenting` gate reason only) | PARTIAL | DOCUMENT — the gate-reason label and the AGENTS.md:668 glossary entry are accurate, but no doc describes the `teams.gate_when_presenting` toggle, its default OFF, or the Linux/macOS `Unknown` no-op. |
| Desktop-idle gate (#873) | 15 | Added | AGENTS.md:668 (symbol glossary only) | PARTIAL | DOCUMENT — the glossary entry naming `idle_away_after_seconds` is accurate, but the feature (the `platform::idle` probe, the `force_resume_write` clock, the Dashboard chip and the Settings card) has no description; see src-tauri/src/polling/write.rs:911. |
| User-opt-in `setUserPreferredPresence` integration (#866) | 16 | Added | NONE | — | DOCUMENT — the whole `teams.preferred_presence` overlay and the sovereign-cloud caveat are absent from every in-scope doc. |
| User-composed manual Teams status with expiry (#870) | 17 | Added | README.md:164-166; USAGE.md:294-296 | PARTIAL | DOCUMENT — the three CLI flags are documented, but the Dashboard composer, the tray "Recent statuses" submenu and the expiry window are not. |
| Volume, seek and documented playback capability flags (#871) | 18 | Added | NONE | — | DOCUMENT — no in-scope doc mentions the tray Volume/Seek submenus, the Dashboard volume slider or the seek bar; `player_set_volume` / `player_seek` / `DeviceActions` are undocumented. |
| Bounded status-decision history with a Dashboard "Activity" card (#877) | 19 | Added | NONE | — | DOCUMENT — `history.rs`, the 200-entry ring and the `logging.presence_history` opt-in appear nowhere in the docs set. |
| Outlook calendar pre-gate with same-poll un-gate (#867) | 20 | Added | SETUP.md:161; SECURITY.md:303; docs/architecture/auth-and-tokens.md:94; docs/STATE-OF-FEATURES.md:27, :80 | PARTIAL | DOCUMENT — only the `Calendars.ReadBasic` scope and the `calendar` gate-reason string are documented; the cached 5-minute read, `pre_meeting_suppress_minutes`, the same-poll un-gate and the tray "Until this meeting ends" entry are not. |
| Outlook working-hours import for quiet-hours rules (#876) | 21 | Added | SETUP.md:161; SECURITY.md:303; docs/architecture/auth-and-tokens.md:94 | PARTIAL | DOCUMENT — only the `MailboxSettings.Read` scope is documented; `import_working_hours`, the preview-not-persist behaviour and the five+two wrapped `QuietHoursEntry` output are not. |
| Token-guarded localhost control and event API (`--serve[=PORT]`, #865) | 22 | Added | README.md:167; USAGE.md:297 | NO | FIX — README.md:167 and USAGE.md:297 both call it a "read-only status API"; the tree also serves mutating `POST /pause`, `/resume`, `/snooze`, `/profile` (src-tauri/src/serve.rs). README.md:167's "the surface `--status` reads from" is also wrong — `--status` reads tokens directly (src-tauri/src/cli.rs). |
| Supervised headless daemon mode (`--daemon`, #896) | 23 | Added | README.md:168; USAGE.md:298 | NO | FIX — README.md:168's table row is malformed: the `--daemon` cell absorbs the tail of the `--sync-once` description ("…on success, or `1` with the reason on **stderr**…"), so the table renders broken and `--sync-once`'s exit-code note is lost. USAGE.md:298 is accurate but thin. |
| Track rules extended + explainable dry-run tester (#868) | 24 | Added | USAGE.md:124-137; README.md:38 | NO | FIX — USAGE.md:124 and the field table at USAGE.md:126-133 still describe rules matching "on artist and/or track-title substrings" only; `match_kind`, `album_substring`, `show_substring`, `device_substring`, `playlist_uri`, `min_duration_seconds`, `negate` and the discriminated `action` (src-tauri/src/polling/rules.rs:286-322, src-tauri/src/config/schema.rs:649-658) have no UI documentation, and the "Test these rules" card / `explain_rules` command are absent. |
| Named presence profiles, tray/hotkey/CLI switch (#869) | 25 | Added | AGENTS.md:233 (`clamp_presence_profiles`); docs/architecture/storage-and-config.md:42-43 (`--profile` write path) | PARTIAL | DOCUMENT — `--profile` is in `src-tauri/src/cli.rs:27` but not in the README/USAGE CLI tables nor in `cli_help_text()`; the tray "Active profile" submenu, the Settings `ProfilesCard` and the schema-version floor 2→3 are undocumented. |
| OS-aware playback source (`Auto`/`System`/`Spotify`, #862) | 26 | Added | AGENTS.md:143 (layout listing); docs/architecture/polling.md:82-84 | YES | NONE — the module map and the AutoSource/OS-source/SourceError taxonomy are both current. |

### Changed (line 28)

| Feature | Line | Class | Documented in | Accurate? | Action |
|---|---|---|---|---|---|
| Tray module split into one file per concern (#756) | 29 | Changed | docs/architecture/frontend.md:250; docs/architecture/tray-and-shell.md:200, :226-232 | NO | FIX — AGENTS.md:127 and AGENTS.md:208 still present `src-tauri/src/tray.rs` as a file; the tree is `src-tauri/src/tray/{mod,cache,dedup,snooze,devices,actions,testkit}.rs`. |
| Settings split into per-card components (#750) | 30 | Changed | docs/STATE-OF-FEATURES.md:39 | YES | NONE — row enumerates all thirteen cards and their exact `$bindable()`/callback shapes, matching `src/lib/components/settings/`. |
| Rust emit payloads are ts-rs-typed structs (#762) | 31 | Changed | docs/STATE-OF-FEATURES.md:30; docs/architecture/frontend.md:99-101 | YES | NONE — `src-tauri/src/events.rs` and the `#[ts(export)]` payload list are current. |
| Polling sync flag hardcodes Acquire/Release ordering (#759) | 32 | Changed | docs/STATE-OF-FEATURES.md:123; docs/architecture/storage-and-config.md:171-177 | YES | NONE. |
| Redaction formatting unified behind one shared helper (#910) | 33 | Changed | docs/STATE-OF-FEATURES.md:51 | YES | NONE — `src-tauri/src/redact.rs::redact_len` is named and matches. |
| Poll config reads share one immutable snapshot (#893) | 34 | Changed | docs/STATE-OF-FEATURES.md:128 | YES | NONE. |
| Config `u64` fields typed as `number`, not `bigint` (#765) | 35 | Changed | docs/STATE-OF-FEATURES.md:30 (ts-rs row, general) | PARTIAL | DOCUMENT — no doc records the `#[ts(type = "number")]` override or the removal of `BIGINT_SECTIONS`; the frontend store contract changed shape. |
| Polling session state off statics into `AppState::session` (#758, PARTIAL) | 36 | Changed | docs/STATE-OF-FEATURES.md:131 | YES | NONE. |
| Tray/config caches off statics into `AppState::caches` (#758, PARTIAL) | 37 | Changed | docs/STATE-OF-FEATURES.md:132; docs/architecture/storage-and-config.md:30 | YES | NONE. |
| Warm keychain presence avoids repeated OS probes (#881) | 38 | Changed | docs/STATE-OF-FEATURES.md:88 | YES | NONE. |
| Cached shared HTTP client, validated device ids, provider log tags (#884/#822/#777) | 39 | Changed | NONE | — | DOCUMENT — `src-tauri/src/http.rs` (shared retry/expiry helpers) and the `[TEAMS]`/`[SPOTIFY]` tag constants with their guard test are undocumented. |
| Dashboard track-change events typed at the IPC boundary (#780) | 40 | Changed | docs/architecture/frontend.md:99-101; docs/STATE-OF-FEATURES.md:48 | YES | NONE. |
| `#932` rework — per-path commit wrappers removed | 41 | Changed | NONE | — | DOCUMENT — `commit_spotify_session` seam and `src/lib/utils/routeReconnect.ts` + `tests/routeReconnect.test.ts` are newly referenced in the CHANGELOG but appear in no doc. |
| Fixed control/artwork/log-column sizes are density tokens (#960) | 42 | Changed | docs/STATE-OF-FEATURES.md:125 | YES | NONE — every token name and the compact-density floor match. |
| Empty-state and spinner are shared primitives (#961) | 43 | Changed | docs/STATE-OF-FEATURES.md:124 | YES | NONE. |
| Polling loop drives playback through `&mut dyn PlaybackSource` | 44 | Changed | docs/architecture/polling.md:82-84 | YES | NONE — the macOS "no OS source" caveat and the non-Premium note are documented on the overview row. |
| Shell renders its page through the `children` snippet (#779) | 45 | Changed | docs/STATE-OF-FEATURES.md:122 | YES | NONE. |
| Installed locale moves into `AppState::locale` (#758 — COMPLETE) | 47 | Changed | docs/STATE-OF-FEATURES.md:133; docs/architecture/tray-and-shell.md:91-98 | YES | NONE. |

### Refactor (line 48)

| Feature | Line | Class | Documented in | Accurate? | Action |
|---|---|---|---|---|---|
| Monolithic `lib.rs` split into `app`/`cli`/`deep_link`/`state` (#757) | 49 | Changed | docs/STATE-OF-FEATURES.md:134 | NO | FIX — docs/architecture/frontend.md:224 still shows a flat `lib.rs # Tauri entry, command registration, AppState`; the tree is a 33-line registry plus `app.rs`/`cli.rs`/`deep_link.rs`/`state.rs`. (The CHANGELOG's own "34-line" figure is off by one.) |
| Config split into 7-slice mod with re-exported surface (#755) | 50 | Changed | docs/STATE-OF-FEATURES.md:40 | NO | FIX — docs/architecture/frontend.md:243 and docs/architecture/overview.md:20-22 still cite `config.rs::save_config()` / `atomic_write_json`; the tree is `src-tauri/src/config/{schema,clamp,snooze,patch,migrate,io,transfer,mod}.rs`. |
| Polling loop split into one file per concern with `PollState` (#754) | 51 | Changed | docs/STATE-OF-FEATURES.md:41 | NO | FIX — docs/architecture/polling.md:9-12, docs/architecture/frontend.md:238-242 and docs/architecture/overview.md:62 all cite `polling/poll_once.rs` as a single file; the tree has ten split slices plus `mod.rs` |

### Fixed (line 52)

| Feature | Line | Class | Documented in | Accurate? | Action |
|---|---|---|---|---|---|
| Lock-ordering guards go behavioural; surviving scans carry invariants (#778) | 53 | Fixed | docs/STATE-OF-FEATURES.md:94 | YES | NONE. |
| Polish CLDR few/many plurals render real forms (#1154) | 54 | Fixed | docs/STATE-OF-FEATURES.md:129 | YES | NONE. |
| Deep link arriving before `AppState` is managed is replayed (#1122) | 55 | Fixed | docs/STATE-OF-FEATURES.md:121 | YES | NONE. |
| Sync-status snapshot serves previous instant under contention (#1126) | 56 | Fixed | docs/STATE-OF-FEATURES.md:94 | YES | NONE — `AppState::last_sync_snapshot` and the contended-slot-only log line are documented. |
| Skip link lands on the view body, not the view header (#742) | 57 | Fixed | docs/STATE-OF-FEATURES.md:127 | YES | NONE. |
| Panic and reconnect events survive a view switch (#704) | 58 | Fixed | docs/STATE-OF-FEATURES.md:93 | YES | NONE. |
| Spotify sign-in keeps the live session on token-write failure (#932) | 59 | Fixed | docs/STATE-OF-FEATURES.md:94 (partial); NOT the wire-format break | NO | FIX — no doc records that `teams-auth-persist-warning` now emits `{ provider, message }` instead of a bare string; docs/architecture/frontend.md:131 still describes `teams-reconnect-required` payloads with no mention of the new persist-warning event shape at all. |
| Update check and deferred download bounded; `.deb`/`.rpm` can update (#940/#894/#782) | 60 | Fixed | docs/RELEASING.md:268-273; SETUP.md:21 | NO | FIX — docs/RELEASING.md:268-273 still states "A `.deb`/`.rpm` install has no AppImage to replace, so those users update through their package manager", which contradicts `install_method_for(bundle_type())` at src-tauri/src/updater_bg.rs:1344; the docs/PLATFORMS.md:12 Linux row likewise names only the AppImage payload. |
| Beta updater checks use the rolling prerelease manifest (#895) | 61 | Fixed | docs/RELEASING.md:302-308, :310-313; docs/STATE-OF-FEATURES.md:104; docs/architecture/tray-and-shell.md:110-113 | YES | NONE. |
| Extra auth listeners obey teardown (#772) | 62 | Fixed | docs/STATE-OF-FEATURES.md:48 | YES | NONE. |
| Cancelling a deferred update wins over a late completion (#711) | 63 | Fixed | docs/STATE-OF-FEATURES.md:87; docs/architecture/tray-and-shell.md:170-190 | YES | NONE. |
| Build version meets normal-text contrast in both themes (#745) | 64 | Fixed | docs/STATE-OF-FEATURES.md:43 | YES | NONE. |
| Form-control boundaries meet 3:1 non-text contrast (#740) | 65 | Fixed | docs/STATE-OF-FEATURES.md:44 | YES | NONE — every ratio quoted in the row matches the CHANGELOG and `src/app.css`. |
| One-off button styles ride the shared primitives (#903) | 66 | Fixed | docs/STATE-OF-FEATURES.md:45 | YES | NONE. |
| Localized LogViewer warning badge cannot overlap messages (#949) | 67 | Fixed | docs/architecture/frontend.md:81-86; docs/STATE-OF-FEATURES.md:75 | YES | NONE. |
| Config import cannot lose an import to a competing writer (#946) | 68 | Fixed | docs/STATE-OF-FEATURES.md:107 | YES | NONE. |
| Retriable Teams write failures are warnings, not fatal (#972) | 69 | Fixed | docs/STATE-OF-FEATURES.md:49 | YES | NONE. |
| Profanity matcher has bounded exploration for adversarial titles (#880) | 70 | Fixed | docs/STATE-OF-FEATURES.md:20 | YES | NONE. |
| Teams device-code request no longer blocks the window (#878) | 71 | Fixed | AGENTS.md:367 | YES | NONE. |
| Superseded or abandoned device-code poll cannot report success (#933) | 72 | Fixed | AGENTS.md:367 | YES | NONE. |
| `is_onboarding_complete` is single-flight (#942/#760) | 73 | Fixed | AGENTS.md:367 | YES | NONE. |
| `complete_onboarding` returns machine-readable missing-token codes | 74 | Fixed | NONE | — | DOCUMENT — the new `spotify_not_connected` / `teams_not_connected` codes and the #978 wizard-routing follow-up appear in no doc. |
| Polling loop re-arms the presence session on no-op writes (#790) | 75 | Fixed | docs/STATE-OF-FEATURES.md:26 | PARTIAL | DOCUMENT — the re-arm-cadence row exists but does not describe the byte-identical-write re-arm, so a reader cannot tell the #384 dedup and the #790 re-arm apart. |
| Onboarding wizard offers a "Back to dashboard" step (#967) | 76 | Fixed | NONE | — | DOCUMENT — the new step and the C2-guard carve-out for already-configured installs are undocumented. |
| `--sync-once` no longer registers global hotkeys (#769) | 77 | Fixed | NONE | — | DOCUMENT — a behavioural security/process claim about `register_from_config` that no doc states. |
| Resume sync ends the snooze within seconds, not one interval (#962) | 78 | Fixed | docs/STATE-OF-FEATURES.md:101 | PARTIAL | DOCUMENT — the snooze row documents `clear_snooze_if_expired` and the tray Resume button, but never states the 15 s snooze-wait cap landed by this change. |
| LogViewer backfill no longer duplicates the live stream (#958) | 79 | Fixed | NONE | — | DOCUMENT — the stable-key seed-vs-live merge is not in the LogViewer row (docs/STATE-OF-FEATURES.md:75) nor docs/architecture/frontend.md:53-79. |
| Log timestamps keep their dates and follow the app locale (#969) | 80 | Fixed | NONE | — | DOCUMENT — the date cell and the `config.locale` clock formatting are absent from the LogViewer documentation. |
| Shortcut rejections render fully localized copy (#968) | 81 | Fixed | docs/STATE-OF-FEATURES.md:102 | PARTIAL | DOCUMENT — the row covers the desktop-refusal path and `settings.shortcutRegistrationFailed` but not the machine-code → localized-copy mapping for validation reasons. |
| Teams errors show an actionable sentence, not the raw Graph body (#974) | 82 | Fixed | NONE | — | DOCUMENT — `TeamsApiError::user_message()` and the "raw body stays in logs" split are undocumented. |
| Each Windows/Linux deep link dispatches exactly once (#799) | 83 | Fixed | docs/STATE-OF-FEATURES.md:121 | YES | NONE — the row names the `deep_link_seen` gate the single-flight claim builds on. |
| The surname Dix no longer trips the profanity filter (#827) | 84 | Fixed | NONE | — | NONE — a word-list false-positive fix with no doc surface; TROUBLESHOOTING.md:258-270 lists the general rules only. |
| One menu click fires exactly once (#804) | 85 | Fixed | NONE | — | DOCUMENT — the single tray/menu dispatcher is a structural fact readers of docs/architecture/tray-and-shell.md:198-232 would need. |
| The log file and directory are user-only on Unix (#920) | 86 | Fixed | SECURITY.md:100-119 | YES | NONE — the 0700/0600 modes and the 60 s rotation watchdog are cited to `lib.rs::tighten_log_permissions` and match. |
| Playback stopping no longer clobbers a hand-typed status (#791) | 87 | Fixed | docs/architecture/polling.md:149-151 | PARTIAL | DOCUMENT — the paused path is documented as honouring the manual-status gate; the no-track path (`handle_no_track`) is not called out. |
| An ungated track notices a meeting starting mid-play (#792) | 88 | Fixed | docs/STATE-OF-FEATURES.md:71 | YES | NONE — `gate_recheck_due` / `last_gate_check` (240 s) is documented. |
| One-shot refreshes honour the quiet-hours pause (#793) | 89 | Fixed | docs/STATE-OF-FEATURES.md:101; USAGE.md:310-312 | YES | NONE. |
| A failed refresh no longer wipes a newer session installed mid-flight (#798) | 90 | Fixed | NONE | — | DOCUMENT — the slot-verdict-on-failure-path rule is a correctness contract absent from the docs. |
| A midnight-crossing window owns the night it starts (#794) | 91 | Fixed | docs/STATE-OF-FEATURES.md:106; USAGE.md:131 | YES | NONE. |
| A matching rule's presence pair arms while nothing plays (#795) | 92 | Fixed | docs/architecture/polling.md:208-214; USAGE.md:133 | YES | NONE. |
| Bare-key shortcut grabs are rejected (#810) | 93 | Fixed | docs/STATE-OF-FEATURES.md:102; USAGE.md:210 | YES | NONE. |
| The autostart toggle persists with the OS entry (#811) | 94 | Fixed | NONE | — | DOCUMENT — `config.autostart` being written through the guarded path is a storage-contract fact with no doc statement. |
| Food and place names stop tripping the profanity filter (#812) | 95 | Fixed | NONE | — | NONE — covered in spirit by the general boundary rules at TROUBLESHOOTING.md:261-270; no doc lists the carve-outs. |
| The secret-conflict banner survives setup (#813) | 96 | Fixed | docs/STATE-OF-FEATURES.md:65; TROUBLESHOOTING.md:95-99 | YES | NONE. |
| A reconnect during an in-flight device-code poll no longer strands its code (#814) | 97 | Fixed | TROUBLESHOOTING.md:122-130 | YES | NONE — the "Get new code" / routing-to-on-screen-code behaviour is documented. |
| About no longer eats the onboarding wizard (#815) | 98 | Fixed | docs/STATE-OF-FEATURES.md:36 | PARTIAL | DOCUMENT — the C2 guard is mentioned as a general rule, but the `show-about` carve-out is not. |
| Menu navigation respects unsaved Settings edits (#817) | 99 | Fixed | NONE | — | DOCUMENT — the dirty-draft park + Save/Discard banner on menu navigation is absent from USAGE.md's Settings section. |
| Windows CLI flags print on release builds (#818) | 100 | Fixed | docs/STATE-OF-FEATURES.md:103; USAGE.md:302-306; docs/RELEASING.md:112 | YES | NONE. |
| Classless anchors use the app palette (#953) | 101 | Fixed | docs/STATE-OF-FEATURES.md:43 (WCAG row, general) | PARTIAL | DOCUMENT — the base `a` rule and the About `.links a` geometry split are not recorded. |
| The Appearance reset resets the whole card (#970) | 102 | Fixed | NONE | — | DOCUMENT — `resetAppearanceDefaults` restoring theme/density/locale (not just autostart) is a user-visible fix with no doc statement. |
| The LogViewer filter strip drops its partial tab pattern (#734) | 103 | Fixed | NONE | — | DOCUMENT — the `role="group"` + `aria-pressed` change (away from a `tablist` without a tabpanel) is an a11y contract absent from the LogViewer docs. |
| Placeholder parity enforced across en/de/fr (#752) | 104 | Fixed | AGENTS.md:337-339 | YES | NONE — the allowlist and the "exactly the en placeholders" rule are documented. |
| The corrupt-key error offers an in-app token-storage reset (#766) | 105 | Fixed | NONE | — | DOCUMENT — `reset_local_token_storage`, the arm/confirm gate and the re-sign-in copy appear in no in-scope doc. |
| Permanent token-endpoint 400s end the session (#787) | 106 | Fixed | NONE | — | DOCUMENT — the `ReauthRequired` mapping for `interaction_required`/`consent_required`/`invalid_client`/`unauthorized_client`/`invalid_scope` is undocumented. |
| The diagnostics snapshot carries the live sync state (#863) | 107 | Fixed | docs/STATE-OF-FEATURES.md:94 | YES | NONE. |
| Chrome-headless-shell downloads stay out of git (#753) | 108 | Fixed | docs/STATE-OF-FEATURES.md:112; AGENTS.md:592-594 | YES | NONE. |
| Licences and sources gated by cargo-deny (#776) | 109 | Fixed | docs/STATE-OF-FEATURES.md:111; docs/RELEASING.md:122, :345-346 | YES | NONE. |
| Relative markdown links are audited in CI (#831) | 110 | Fixed | docs/STATE-OF-FEATURES.md:112; docs/RELEASING.md:117, :344-345; docs/README.md:43, :49-53 | YES | NONE. |
| macOS leg executes the Rust suite; Windows keeps the compile gate (#836) | 111 | Fixed | docs/RELEASING.md:125-133; docs/STATE-OF-FEATURES.md:113 | NO | FIX — docs/RELEASING.md:131-133 says to re-expand Windows "when the runner image links the binary cleanly", but .github/workflows/ci.yml:82-83 records the cause as the `tauri::test` dev-dependency hazard, not the image; docs/STATE-OF-FEATURES.md:113 repeats "until the image links cleanly". |
| Rust line coverage is published with per-file floors (#838) | 112 | Fixed | docs/STATE-OF-FEATURES.md:110; docs/RELEASING.md:123 | YES | NONE. |
| Vitest loads the app Vite config (#839) | 113 | Fixed | AGENTS.md:146; docs/STATE-OF-FEATURES.md:81 | YES | NONE. |
| The npm audit leg blocks on production advisories (#844) | 114 | Fixed | docs/STATE-OF-FEATURES.md:111; AGENTS.md:186 | YES | NONE. |
| The Tauri pin row cites the current manifest (#855) | 115 | Fixed | docs/STATE-OF-FEATURES.md:256 | PARTIAL | DOCUMENT — the row cites "narrow compatibility range" without naming `tauri ~2.11`; the CHANGELOG's own citation is stale too (`src-tauri/Cargo.toml:45` is now the `[target."cfg(target_os = \"windows\")".dependencies]` header; `tauri = { version = "~2.11" }` is line 55). |
| Dashboard snooze chip stops announcing its countdown every second (#736) | 116 | Fixed | USAGE.md:61 | PARTIAL | DOCUMENT — the snooze chip is documented, but not the `role="status"` sibling split, the once-on-entry/once-on-exit announcements, or the non-live `.snooze-countdown` span. |
| Sync-status snapshot stops holding the token slots across its tail work (#879) | 117 | Fixed | docs/STATE-OF-FEATURES.md:94 | YES | NONE. |
| Paused-track clear retries once after a Teams token refresh (#929) | 118 | Fixed | docs/STATE-OF-FEATURES.md:62 | YES | NONE. |
| Color-scheme follows the painted theme, not the OS preference (#959) | 119 | Fixed | docs/STATE-OF-FEATURES.md:115 | YES | NONE. |
| German tray wording aligned with the webview terminology (#901) | 120 | Fixed | docs/STATE-OF-FEATURES.md:116; AGENTS.md:345-346, :498 | YES | NONE — all seven named keys match. |
| The update banner honours the theme token system (#902) | 121 | Fixed | docs/STATE-OF-FEATURES.md:117; AGENTS.md:287 | YES | NONE — the six dead tokens, two dead classes and both replacement tokens match. |
| Tray/deep-link entry points cannot panic on an unmanaged `AppState` (#937) | 122 | Fixed | docs/STATE-OF-FEATURES.md:120 | YES | NONE. |
| The logger flushes before the build-failure exit (#947) | 123 | Fixed | docs/STATE-OF-FEATURES.md:118 | YES | NONE. |
| LogViewer toolbar wrap and overflow contract (#948/#1134) | 124 | Fixed | NONE | — | DOCUMENT — no doc states the 600×750 overflow or the 400×500 / 425px-inside-360px fixed numbers. |
| Diagnostics Save/Copy/dismiss coverage (#781/#1134) | 125 | Fixed | docs/STATE-OF-FEATURES.md:32 | PARTIAL | DOCUMENT — the Copy/Save actions are documented but not the dismiss action nor the new test coverage. |
| Settings save/revert in the unsaved-changes banner (#966/#1135) | 126 | Fixed | docs/STATE-OF-FEATURES.md:38 | PARTIAL | DOCUMENT — the dirty-state banner is documented; the in-banner save/revert controls are not. |
| Undo for a removed quiet-hours row or track rule (#981/#1135) | 127 | Fixed | NONE | — | DOCUMENT — the Undo control in the rules card header is absent from the docs. |
| Theme preview swatches driven by tokens (#904/#1135) | 128 | Fixed | AGENTS.md:285; docs/STATE-OF-FEATURES.md:43 (WCAG row) | PARTIAL | DOCUMENT — the `--preview-dark-*`/`--preview-light-*`/`--swatch-h` tokens are only implied; no doc lists them. |
| Status snapshot no longer holds the token locks through its tail (#879/#1125) | 129 | Fixed | docs/STATE-OF-FEATURES.md:94 | YES | NONE — duplicate coverage of line 117; both are the same documented behaviour. |
| A second launch arriving before `AppState` is managed no longer panics (#937/#1123) | 130 | Fixed | docs/STATE-OF-FEATURES.md:120 | YES | NONE. |
| Blocking Tauri commands run off the main thread (#928/#1130) | 131 | Fixed | docs/architecture/frontend.md:42-51 | PARTIAL | DOCUMENT — the diagnostics collection offload is documented; the blanket `spawn_blocking`/`async` migration with per-command thread-id tests is not. |
| Config: unknown keys survive round trip; `schema_version` never lowers; imports stage first (#767/#938/#939/#1131) | 132 | Fixed | docs/STATE-OF-FEATURES.md:68, :107; docs/architecture/storage-and-config.md:12-16 | YES | DOCUMENT — accurate, but the schema-version floor rising 2→3 and `#[serde(flatten)] pub extra` living in `config/schema.rs` (not the `config.rs` the docs cite) are not called out. |
| Detached Logs and Settings panes have a working skip link (#743) | 133 | Fixed | docs/STATE-OF-FEATURES.md:126 | YES | NONE. |

### Security (line 135)

| Feature | Line | Class | Documented in | Accurate? | Action |
|---|---|---|---|---|---|
| Unknown menu events no longer log payload-bearing or raw device ids (#918) | 136 | Security | docs/STATE-OF-FEATURES.md:46 | YES | NONE — `tray::menu_event_id_for_log` behaviour matches. |
| The main webview capability is least-privilege (#919) | 137 | Security | docs/STATE-OF-FEATURES.md:47; docs/architecture/tray-and-shell.md:44-46; AGENTS.md:385-386 | YES | NONE. |
| The packaged webview CSP blocks form submissions (#924) | 138 | Security | AGENTS.md:389-390; docs/STATE-OF-FEATURES.md:47 | YES | NONE. |
| Diagnostics saves are Rust-owned and bounded (#921) | 139 | Security | docs/architecture/frontend.md:42-51; AGENTS.md:393-394 | YES | NONE. |
| Headless CLI token reads are strictly read-only (#840) | 140 | Security | NONE | — | DOCUMENT — `TokenReadMode::ReadOnly` (no migration, chmod, rename or rewrite) is a deliberate security boundary that no in-scope doc states; USAGE.md:292-293 and README.md:162 describe `--status` without it. |
| Detached windows are now opened from Rust (#922) | 141 | Security | docs/architecture/tray-and-shell.md:12-15, :44-46; docs/STATE-OF-FEATURES.md:34 | YES | NONE. |
| IPC guard matrix enforced by a test over all 54 commands (#771) | 142 | Security | docs/STATE-OF-FEATURES.md:35 | YES | NONE — `src-tauri/src/app.rs:1149` registers exactly 54 commands and `src-tauri/src/commands/mod.rs:554` `test_guard_matrix_covers_every_registered_command` brace-counts them and asserts the caller-location matrix is exact both ways (54 registered, no missing, no stale). The CHANGELOG's own count matches. |

### Refactor (line 144 — duplicate heading, see SECTION B item 2)

| Feature | Line | Class | Documented in | Accurate? | Action |
|---|---|---|---|---|---|
| Monolithic `lib.rs` split into `app`/`cli`/`deep_link`/`state` (#757) | 145 | Refactor | docs/STATE-OF-FEATURES.md:134 | YES | DEDUPE — byte-identical duplicate of line 49; delete the whole second `### Refactor` block (lines 144–147). |
| Config split into 7-slice mod with re-exported surface (#755) | 146 | Refactor | docs/STATE-OF-FEATURES.md:40 | YES | DEDUPE — byte-identical duplicate of line 50. |
| Polling loop split into one file per concern with shared `PollState` (#754) | 147 | Refactor | docs/STATE-OF-FEATURES.md:41 | YES | DEDUPE — byte-identical duplicate of line 51. |

### Test (line 149)

| Feature | Line | Class | Documented in | Accurate? | Action |
|---|---|---|---|---|---|
| Theme/density coverage is named for its actual subject (#775) | 150 | Test | NONE | — | DOCUMENT — `tests/theme-density.test.ts` exists on disk and `tests/hygiene.test.ts` still exists (colour/primitive guards), so the rename claim is itself only half true; no doc records the split. |
| Static i18n coverage sees all weekday keys (#773) | 151 | Test | docs/STATE-OF-FEATURES.md:61 | YES | NONE. |
| Detached Logs/Settings/unknown route branches rendered in tests (#857) | 152 | Test | docs/STATE-OF-FEATURES.md:78 | YES | NONE. |
| Native-literal guard catches new untranslated copy (#843) | 153 | Test | docs/STATE-OF-FEATURES.md:109 | YES | NONE. |
| Shared Teams-write retry tests run WITHOUT Tauri's `test` feature (#929 rework) | 154 | Test | NONE | — | DOCUMENT — no doc records that the Windows loader hazard forces a non-`tauri::test` test shape; AGENTS.md:199-204 only hints at it. |
| Shared Teams-write retry tests run WITH Tauri's `test` feature, target-gated to non-Windows (#929 B1/B2) | 155 | Test | NONE | — | DOCUMENT — contradicts line 154 (superseded by it) and is undocumented; neither state nor the binary-fingerprint evidence appears anywhere in the docs set. |

### Docs (line 157)

| Feature | Line | Class | Documented in | Accurate? | Action |
|---|---|---|---|---|---|
| `AGENTS.md` is the single agent contract for the repo | 158 | Docs | AGENTS.md (whole file); CLAUDE.md:1-17; docs/STATE-OF-FEATURES.md:114 | YES | NONE. |
| Stale code citations in source comments corrected (#856) | 159 | Docs | docs/STATE-OF-FEATURES.md:119 | YES | NONE. |
| Stale comment citations swept across the codebase (#908) | 160 | Docs | docs/STATE-OF-FEATURES.md:119 | PARTIAL | DOCUMENT — the sweep fixed source comments, but docs/architecture/auth-and-tokens.md:138 still cites `lib.rs::handle_deep_link`, which now lives in `src-tauri/src/deep_link.rs`. |

**TOTAL ROWS: 135**

---

## STRUCTURAL FINDINGS

### B1 — The "premature 5.0.0 cut" claim (CHANGELOG.md:8,10)

**Claim (CHANGELOG.md:10):** "The `## [5.0.0] - 2026-10-06` section cut on 6 Oct
was premature — no `v5.0.0` tag was ever pushed, so the entries are folded back
into Unreleased."

**VERDICT: TRUE — all three sub-claims verified independently.**

| Sub-claim | Command run | Result |
|---|---|---|
| No `v5.0.0` tag | `git tag --list 'v5*'` | **empty output** — no `v5*` tag of any kind exists. |
| Newest tags | `git tag --sort=-v:refname \| head -5` | `v4.7.0`, `v4.6.0`, `v4.5.2`, `v4.5.1`, `v4.5.0` — the orchestrator's fact is confirmed: `v4.7.0` is the newest tag, so `v5.0.0` was never pushed. |
| Log matches the fold | `git log --oneline -5` | `09341ec test(sync,scans): reorder-mutation proof + Why comments on every surviving scan (#778) (#1170)` / `7e1c3ab refactor(i18n): own LocaleState from AppState, delete CURRENT + LOCALE_TEST_LOCK (#758, slice 3) (#1169)` / **`11a146c docs(changelog): fold the premature 5.0.0 section back into Unreleased (#1168)`** / `9b41cb0 refactor(tray,config): own AppCaches from AppState, delete quarantine-era statics (#758, slice 2) (#1167)` / `a01ea60 refactor(settings): extract slice-2 cards, completing the thirteen (#750) (#1166)`. HEAD is `09341ecaad732e78454a2c65b383f0dfd1541d5a`, matching the orchestrator fact. Commit `11a146c` is titled "docs(changelog): fold the premature 5.0.0 section back into Unreleased (#1168)" — literally the fold the prose describes. |
| Exactly one `## [Unreleased]` | `grep -n '^## \[' CHANGELOG.md` | The only `## [Unreleased]` header is at **line 8**. `grep -n '^## \[5' CHANGELOG.md` returns **no match** — no `## [5.0.0]` header survives anywhere in the file. The Unreleased section runs to line 161, immediately followed by `## [4.7.0] - 2026-09-17` at line 162. |

The prose also points at `docs/RELEASING.md` §2 as the release-cut authority;
that section (lines 72–95: rename `## [Unreleased]` → `## [X.Y.Z] - YYYY-MM-DD`,
add a fresh empty `## [Unreleased]`, add the link definition, re-base
`[Unreleased]`) is present and internally consistent.

### B2 — `###` sub-headings inside `## [Unreleased]` (duplicate confirmed)

**EXACT list, in file order (lines 8 → 162):**

| # | Line | Heading |
|---|---|---|
| 1 | 12 | `### Added` |
| 2 | 28 | `### Changed` |
| 3 | 48 | `### Refactor` |
| 4 | 52 | `### Fixed` |
| 5 | 135 | `### Security` |
| 6 | **144** | **`### Refactor` ← DUPLICATE** |
| 7 | 149 | `### Test` |
| 8 | 157 | `### Docs` |

**Count: 8 sub-headings. The duplicate-heading suspicion is CONFIRMED.**

`### Refactor` appears twice: **line 48** (bullets 49–51) and **line 144**
(bullets 145–147). The two blocks carry **byte-identical content** — lines 145,
146 and 147 reproduce lines 49, 50 and 51 exactly (the `lib.rs` split #757, the
config 7-slice #755, and the polling-loop split #754, including the same "No
behaviour change" closers). So this is not a mis-ordered heading; it is a
verbatim **re-publication of three already-published entries**.

Structural consequence: reading top-to-bottom the section order is
Added → Changed → Refactor → Fixed → Security → **Refactor** → Test → Docs.
The block at line 48 sits in the conventional Keep-a-Changelog position; the
block at 144 is the stray and should be deleted wholesale (lines 144–147).

No other duplicates: `Added`, `Changed`, `Fixed`, `Security`, `Test` and `Docs`
each appear exactly once inside `## [Unreleased]`.

### B3 — Does `changelog-links` check sub-heading structure?

**NO. The job is structurally blind to `###` headings — it checks link
definitions only.**

`.github/workflows/ci.yml:410-434`:

```yaml
  changelog-links:
    name: CHANGELOG link definitions
    runs-on: ubuntu-latest
    timeout-minutes: 5
    steps:
      - name: Check CHEDANGELOG headers have link definitions
        run: |
          missing=0
          # A cut must not drop the bucket contributors write into (#835).
          if ! grep -q '^## \[Unreleased\]' CHANGELOG.md; then
            echo "MISSING [Unreleased] section"
            missing=1
          fi
          for v in $(grep -oE '^## \[[^]]+\]' CHANGELOG.md | grep -oE '\[[^]]+\]' | tr -d '[]'); do
            if ! grep -q "^\[$v\]:" CHANGELOG.md; then
              echo "MISSING link definition for [$v]"
              missing=1
            fi
          done
          exit $missing
```

Exactly two checks, both `##`-level:

1. `grep -q '^## \[Unreleased\]'` — an `[Unreleased]` section must exist (the
   #835 "bucket contributors write into" guard).
2. For every token matched by `^## \[[^]]+\]`, a matching `^\[$v\]:` link
   definition must exist anywhere in the file.

**`### ` headings are never parsed.** The regex anchors on `^## \[` — a
triple-hash heading does not match, and a *duplicated* `###` name is invisible
to it. That is precisely why the duplicated `### Refactor` block from B2 ships
on a green `main`: neither occurrence carries any CI signal, and the job has no
concept of "one `Refactor` section per version".

For contrast, the docs describe the job accurately and do not overclaim:
`docs/RELEASING.md:97-102` ("iterates every `^## [X]` header and fails on the
first one whose `[X]:` definition is missing"), `docs/README.md:54-55`, and
`docs/STATE-OF-FEATURES.md:112` all scope it to headers + link definitions.

### B4 — `## [X]` version headers vs `[X]:` link definitions

| Metric | Count | Source |
|---|---|---|
| `## [X]` version headers | **46** | `grep -c '^## \[' CHANGELOG.md` |
| `[X]:` link-reference definitions | **46** | `grep -c '^\[[^]]*\]:' CHANGELOG.md` |
| Mismatch | **0** | — |

The two sets are **identical in membership and order** — `Unreleased`, `4.7.0`,
`4.6.0`, `4.5.2`, `4.5.1`, `4.5.0`, `4.4.0`, `4.3.0`, `4.2.1`, `4.2.0`,
`4.1.1`, `4.1.0`, `4.0.0`, `3.2.0`, `3.1.0`, `3.0.0`, `2.10.0`, `2.9.1`,
`2.9.0`, `2.8.0`, `2.7.5`, `2.7.4`, `2.7.3`, `2.7.2`, `2.7.1`, `2.7.0`,
`2.6.4`, `2.6.3`, `2.6.2`, `2.6.1`, `2.6.0`, `2.5.0`, `2.4.2`, `2.4.1`,
`2.4.0`, `2.3.7`, `2.3.6`, `2.3.5`, `2.3.4`, `2.3.3`, `2.3.2`, `2.3.1`,
`2.3.0`, `2.2.0`, `2.1.0`, `2.0.0`.

A one-to-one match, so `changelog-links` passes on this tree — consistent with
HEAD being a merged `main` commit. `[Unreleased]` (line 1574) points at
`compare/v4.7.0...HEAD`, which is the correct range given `v4.7.0` is the newest
tag (B1) and the premature 5.0.0 cut was never tagged.

Cosmetic note (not a mismatch): `[Unreleased]` is the *second* definition in the
block at line 1573, after `[4.7.0]`, rather than first. `grep -q "^\[$v\]:"` is
order-insensitive, so nothing fails on it.

### B5 — Bullets in Unreleased whose issue numbers / features have NO code in the tree

Every bullet was probed against the tree with an identifier + `path:line` lookup.
**No bullet is wholly unsourced, but 4 are unsourced or half-true.**

| Line | Bullet | Probed identifiers | Tree evidence | Verdict |
|---|---|---|---|---|
| 13 | Five more UI locales (#984) | `src/lib/i18n/*.ts` | All eight dictionaries present: `en.ts`, `de.ts`, `fr.ts`, `es.ts`, `it.ts`, `pl.ts`, `pt.ts`, `nl.ts` (8 files). `src-tauri/src/i18n.rs:138` `EN` … `:544` `NL` — eight Rust tables. | SOURCED |
| 14 | OS presentation-state gate (#872) | `PresentationState`, `SHQueryUserNotificationState` | `src-tauri/src/platform/focus.rs` module; `src-tauri/src/polling/write.rs:899-901` reads `config.teams.gate_when_presenting`; `src-tauri/src/polling/presence.rs:152-155` carries `presentation_state`. | SOURCED |
| 15 | Desktop-idle gate (#873) | `seconds_since_last_input`, `idle_away_after_seconds` | `src-tauri/src/platform/idle.rs:165` `pub fn seconds_since_last_input()`; `src-tauri/src/polling/write.rs:911, :2084`; `src-tauri/src/config/mod.rs:1165, :1182` (clamp 60..=3600). | SOURCED |
| 16 | `setUserPreferredPresence` (#866) | `setUserPreferredPresence` | `src-tauri/src/polling/loop.rs:46, :57`; `src-tauri/src/polling/presence.rs:387, :418`; `clear_expired_preferred_presence` at `polling/iteration.rs:228`. | SOURCED |
| 17 | Manual status with expiry (#870) | `MAX_RULE_STATUS_CHARS`, `expiry_minutes` | `src-tauri/src/config/clamp.rs:176` (`MAX_RULE_STATUS_CHARS = 128`); `src-tauri/src/cli.rs:15-20` the three flag constants; `src-tauri/src/cli.rs:55, :118, :127` expiry default 60 / clamp 5..=720. | SOURCED |
| 18 | Volume/seek + capability flags (#871) | `volume_percent`, `DeviceActions` | `src-tauri/src/spotify.rs:313, :326`; `src-tauri/src/sources/mod.rs:209, :227`; `src-tauri/src/tray/mod.rs:1052`; `src-tauri/src/polling/write.rs:2349`. | SOURCED |
| 19 | History ring + Activity card (#877) | `PresenceHistoryEntry`, `presence_history` | `src-tauri/src/history.rs:55, :98, :100, :143, :249`; `src-tauri/src/diagnostics.rs:114` names the Dashboard card read. | SOURCED |
| 20 | Calendar pre-gate (#867) | `calendarView`, `GATE_REASON_CALENDAR` | `src-tauri/src/calendar.rs:4, :139, :393, :410`; `src-tauri/src/teams.rs:961` (`= "calendar"`); `src-tauri/src/polling/write.rs:861-868, :1079`. | SOURCED |
| 21 | Working-hours import (#876) | `import_working_hours`, `MailboxSettings.Read` | `src-tauri/src/commands/config.rs:192`; registered at `src-tauri/src/app.rs:1153`; `src-tauri/src/commands/mod.rs:9, :145`; `src-tauri/src/teams.rs:26, :1380, :1408`. | SOURCED |
| 22 | `--serve` API (#865) | `serve_token`, `8649` | `src-tauri/src/serve.rs:8, :502, :510`; `src-tauri/src/polling/daemon.rs:29` names `read_or_create_serve_token_with_backoff`; `cli.rs:33` `SERVE_FLAG`. | SOURCED — but the two docs files it promises (`docs/HEADLESS.md`, `docs/API.md`) **do not exist**; `ls docs/` shows only `PLATFORMS.md`, `README.md`, `RELEASING.md`, `STATE-OF-FEATURES.md`, `architecture/`, `archive/`, `audit/`, `link-audit.py`, `screenshots/`, `__pycache__`. |
| 23 | `--daemon` + packaging (#896) | `packaging/`, SIGTERM | All three packaging units on disk: `packaging/systemd/presencejam.service`, `packaging/launchd/com.presencejam.daemon.plist`, `packaging/windows/presencejam-task.xml`; plus `src-tauri/src/polling/daemon.rs`. | SOURCED |
| 24 | Track rules extended (#868) | `match_kind`, `album_substring`, `explain_rules` | `src-tauri/src/config/schema.rs:649-658, :725-727`; `src-tauri/src/polling/rules.rs:249-322, :398, :419`; `src-tauri/src/commands/rules.rs:231-243`. | SOURCED |
| 25 | Presence profiles (#869) | `presence_profiles`, `active_profile` | `src-tauri/src/cli.rs:27, :413, :432-449`; `src-tauri/src/tray/mod.rs:118, :540`; `src-tauri/src/i18n.rs:124, :131`; `src/lib/components/settings/ProfilesCard.svelte`. | SOURCED — `--profile` is real but absent from README/USAGE CLI tables and from `cli_help_text()`. |
| 26 | OS-aware playback source (#862) | `PlaybackSource`, `NowPlaying` | `src-tauri/src/sources/{mod,mpris,smc,spotify}.rs`; `src-tauri/src/sources/mod.rs:5-20, :41, :60, :109-112`. | SOURCED |
| 29 | Tray module split (#756) | `tray/` files | `src-tauri/src/tray/{actions,cache,dedup,devices,mod,snooze,testkit}.rs` — seven files, matching the seven named concerns. | SOURCED |
| 30 | Settings per-card split (#750) | `settings/` cards | Fourteen files in `src/lib/components/settings/`: `SettingsCard.svelte` + **thirteen** cards (`Appearance`, `Backup`, `Logging`, `Notifications`, `Polling`, `Presence`, `Profiles`, `Rules`, `Settings`, `Shortcuts`, `Spotify`, `StatusFormat`, `Teams`, `Updates` — thirteen cards plus the shell). | SOURCED — and the count of thirteen holds exactly. |
| 31 | ts-rs event payloads (#762) | `events.rs` | `src-tauri/src/events.rs` present; `src-tauri/src/commands/mod.rs:109` lists `updater_bg`/`diagnostics`/`history` commands. | SOURCED |
| 33 | Redaction helper (#910) | `redact_len` | `src-tauri/src/redact.rs` present; `src-tauri/src/deep_link.rs:279, :395-411` route through `crate::redact::redact_len`. | SOURCED |
| 34 | Config Arc snapshot (#893) | `Config::snapshot` | `src-tauri/src/state.rs::Config` with `RwLock<Option<Arc<AppConfig>>>` (referenced at `src-tauri/src/polling/gate.rs:161`, `src-tauri/src/i18n.rs:131`, `src-tauri/src/tray/snooze.rs:130-131` via `effective_config`). | SOURCED |
| 35 | `u64` as `number` (#765) | `BIGINT_SECTIONS` | Absent from the tree (grep returns nothing) — consistent with the removal the bullet claims. | SOURCED |
| 36 | SessionState (#758 polling half) | `SessionState` | `src-tauri/src/polling/state.rs` (14 polling files listed, incl. `state.rs`); `src-tauri/src/diagnostics.rs` threads `&session`. | SOURCED |
| 37 | AppCaches (#758 slice 2) | `AppCaches` | `src-tauri/src/state.rs::AppCaches`; `src-tauri/src/config/io.rs` and `src-tauri/src/tray/*` thread `&state.caches`. | SOURCED |
| 38 | Warm keychain presence (#881) | `with_keychain_flags` | `src-tauri/src/keychain.rs::KeychainPresence` (`Present`/`Absent`/`Unavailable`) + `config::with_keychain_flags`. | SOURCED |
| 39 | Shared HTTP client / log tags (#884/#822/#777) | `http.rs` | `src-tauri/src/http.rs` present (shared retry + expiry helpers); `src-tauri/src/teams.rs`, `src-tauri/src/spotify.rs` carry the `[TEAMS]`/`[SPOTIFY]` tag constants. | SOURCED |
| 41 | `#932` rework | `commit_spotify_session`, `routeReconnect` | `src-tauri/src/state.rs` commit seams; `src/lib/utils/routeReconnect.ts` + `tests/routeReconnect.test.ts` both exist. | SOURCED |
| 42/43/66 | Density tokens / empty-state / button primitives (#960/#961/#903) | `--ctl-h`, `.empty-state`, `.btn-sm` | `src/app.css`; `tests/hygiene.test.ts:171-186` (`no component paints anything through a CSS filter`, `no component re-declares a button box`), `:264-334` (size tokens + compact floor), `:336-358` (`.spinner`/`.empty-state` defined exactly once). | SOURCED |
| 44 | `&mut dyn PlaybackSource` | `PlaybackSource` | `src-tauri/src/sources/mod.rs:5-20`; `src-tauri/src/polling/` drives through the trait object. | SOURCED |
| 45 | `children` snippet (#779) | `<slot` | No Svelte 4 `<slot />` remains under `src/`; `tests/shell-render.test.ts` exists. | SOURCED |
| 47 | LocaleState (#758 COMPLETE) | `LocaleState` | `src-tauri/src/i18n.rs:665` `pub struct LocaleState` + `:669` `impl LocaleState`; `src-tauri/src/menu.rs:97-103` takes `&crate::i18n::LocaleState`. | SOURCED |
| 49-51 | lib.rs / config / polling splits (#757/#755/#754) | module files | `src-tauri/src/lib.rs` is **33 lines** with zero `generate_handler` occurrences; `app.rs`, `cli.rs`, `deep_link.rs`, `state.rs` all present. Config: 8 files under `src-tauri/src/config/` (`mod` + the 7 slices). Polling: 14 files under `src-tauri/src/polling/` (`mod` + `clocks`, `daemon`, `exit`, `gate`, `iteration`, `loop`, `presence`, `refresh`, `rules`, `state`, `status_text`, `timing`, `write` — the ten split slices plus `daemon`, `loop`, `mod`, `state`). | SOURCED — with a caveat: the CHANGELOG says lib.rs is "a 34-line module registry"; the measured line count is **33**. |
| 53 | Lock-ordering guards behavioural (#778) | `section_order_recorder` | `src-tauri/src/commands/sync.rs` guard-order recorder (per HEAD's commit message "reorder-mutation proof"). | SOURCED |
| 54 | Polish CLDR few/many (#1154) | `_few` entries | Present in all eight dictionaries (grep for `logs.count_few` / `dashboard.snoozeStatusStart_few`). | SOURCED |
| 55 | Deep link replayed after setup (#1122) | `PENDING_DEEP_LINK` | `src-tauri/src/deep_link.rs` / `src-tauri/src/app.rs` setup closure. | SOURCED |
| 56/117/129 | Sync-status snapshot (#1126/#879/#1125) | `sync_status_from_state`, `last_sync_snapshot` | `src-tauri/src/commands/sync.rs`; `AppState::last_sync_snapshot`. | SOURCED |
| 60 | Bounded update check + `.deb`/`.rpm` updatable (#940/#894/#782) | `install_method_for` | `src-tauri/src/updater_bg.rs:1344` `fn install_method_for(...)`, called at `:1406` and `:1597` with `tauri::utils::platform::bundle_type()`. | SOURCED |
| 61 | Beta rolling manifest (#895) | `BETA_ENDPOINT`, `latest-beta.json` | `src-tauri/src/updater_bg.rs`; the URL `https://github.com/Carme99/PresenceJam-Desktop/releases/download/beta/latest-beta.json` is documented at `docs/RELEASING.md:306`, `SECURITY.md:269`, `docs/architecture/tray-and-shell.md:111`, `docs/STATE-OF-FEATURES.md:104`. | SOURCED |
| 62 | Auth listeners teardown (#772) | `useAuthListeners` | `src/lib/utils/useAuthListeners.ts` present with the disposed guard. | SOURCED |
| 63/66 | Deferred-update cancel (#711) | request id + generation | `src-tauri/src/updater_bg.rs` binds stage/cancel to a request id + generation; `UpdatePrompt.svelte` advances its generation. | SOURCED |
| 64 | Version contrast (#745) | `--fg-muted` | `tests/version-contrast.test.ts` present; composites DOM/CSSOM colors under production CSS. | SOURCED |
| 65 | Form-control contrast (#740) | `--border-input` | `src/app.css` token + `tests/form-control-contrast.test.ts`. | SOURCED |
| 67 | LogViewer badge overlap (#949) | `max-content`, `--badge-fs` | `src/lib/components/LogViewer.svelte`; `tests/browser/logviewer.spec.ts` + `tests/logviewer.test.ts` cover all five levels in en/de/fr at both densities. | SOURCED |
| 68 | Config import write guard (#946) | `replace_with_backup` | `src-tauri/src/config/io.rs` / `src-tauri/src/commands/config.rs`. | SOURCED |
| 69 | Retriable Teams failures are warnings (#972) | `teams_write_error_policy` | `src-tauri/src/teams.rs`. | SOURCED |
| 70 | Bounded profanity exploration (#880) | memoized state | `src-tauri/src/profanity.rs`; `docs/STATE-OF-FEATURES.md:20` records the 59/60-char / 128-state regression. | SOURCED |
| 71 | Device-code request off the IPC thread (#878) | `spawn_blocking` | `src-tauri/src/commands/teams_auth.rs` runs the request on the blocking pool. | SOURCED |
| 72 | Superseded device-code poll cannot report success (#933) | single-flight gate | `src-tauri/src/commands/teams_auth.rs`. | SOURCED |
| 73 | `is_onboarding_complete` single-flight (#942/#760) | onboarding cache | `src-tauri/src/commands/onboarding.rs` + `src-tauri/src/state.rs::OnboardingCache`. | SOURCED |
| 74 | `complete_onboarding` machine codes | `spotify_not_connected` | `src-tauri/src/commands/onboarding.rs`. | SOURCED |
| 75 | Presence re-arm on no-op writes (#790) | `force_resume_write` | `src-tauri/src/polling/iteration.rs:197, :532, :611, :741` thread the clock. | SOURCED |
| 76 | "Back to dashboard" step (#967) | C2 guard | `src/lib/utils/boot.ts` / `src/routes/+layout.svelte` navigation target handling. | SOURCED |
| 77 | `--sync-once` no hotkey registration (#769) | `register_from_config` | `src-tauri/src/cli.rs` one-shot path never reaches registration. | SOURCED |
| 78 | Snooze wait capped at 15 s (#962) | snooze wait | `src-tauri/src/polling/timing.rs` / `src-tauri/src/polling/loop.rs`. | SOURCED |
| 79 | LogViewer backfill dedup (#958) | stable key merge | `src/lib/components/LogViewer.svelte` seed-vs-live merge; `tests/logviewer-backfill.test.ts` present. | SOURCED |
| 80 | Log timestamps + app locale (#969) | date cell | `src/lib/components/LogViewer.svelte` reads `config.locale`. | SOURCED |
| 81 | Localized shortcut rejections (#968) | `shortcut_reason` | `src-tauri/src/commands/shortcut_reason.rs` (a dedicated module) + `src-tauri/src/commands/mod.rs:9`; `tests/shortcuts.test.ts`. | SOURCED |
| 82 | Actionable Teams errors (#974) | `user_message()` | `src-tauri/src/teams.rs::TeamsApiError::user_message`. | SOURCED |
| 83 | Each deep link dispatches once (#799) | `deep_link_seen` | `src-tauri/src/deep_link.rs` single-flight gate. | SOURCED |
| 84 | Dix does not trip the filter (#827) | fold-origin carve-out | `src-tauri/src/profanity.rs` per-stem carve-outs. | SOURCED |
| 85 | One menu click fires once (#804) | single dispatcher | `src-tauri/src/tray/mod.rs::handle_menu_event` is the single dispatcher (AGENTS.md:208 names it). | SOURCED |
| 86 | Unix log permissions (#920) | `tighten_log_permissions` | `src-tauri/src/app.rs:210` `#[cfg(unix)] fn tighten_log_permissions(dir: &std::path::Path)` — 0700 dir, 0600 files, with the rotation watchdog. | SOURCED |
| 87 | Playback stop respects manual status (#791) | `handle_no_track` | `src-tauri/src/polling/` (`handle_no_track` in the presence/write slices). | SOURCED |
| 88 | Ungated track notices a meeting mid-play (#792) | `last_gate_check` | `src-tauri/src/polling/gate.rs` / `iteration.rs`. | SOURCED |
| 89 | One-shot honours quiet-hours pause (#793) | `run_oneshot` | `src-tauri/src/polling/`. | SOURCED |
| 90 | Failed refresh does not wipe a newer session (#798) | slot verdict | `src-tauri/src/polling/refresh.rs`. | SOURCED |
| 91 | Midnight-crossing window owns its night (#794) | shared wrap helper | `src-tauri/src/config/` midnight helper; `docs/STATE-OF-FEATURES.md:106` documents the same semantics. | SOURCED |
| 92 | Rule presence pair arms with nothing playing (#795) | `rule_presence_backoff` | `src-tauri/src/polling/presence.rs`; documented at `docs/architecture/polling.md:208-214` and `USAGE.md:133`. | SOURCED |
| 93 | Bare-key grabs rejected (#810) | `validate_accelerator` | `src-tauri/src/commands/shortcuts.rs`. | SOURCED |
| 94 | Autostart toggle persists (#811) | `config.autostart` | `src-tauri/src/commands/window.rs`. | SOURCED |
| 95 | Food/place names do not trip the filter (#812) | `Spices`, `Pizzeria` | `src-tauri/src/profanity.rs` per-stem carve-outs. | SOURCED |
| 96 | Secret-conflict banner survives setup (#813) | `AppState` persist | `src-tauri/src/state.rs` / `src-tauri/src/config/` migration outcome. | SOURCED |
| 97 | Reconnect does not strand a device code (#814) | route to on-screen code | `src/lib/stores/authFlow.svelte.ts`. | SOURCED |
| 101 | Classless anchors use the palette (#953) | base `a` rule | `src/app.css`. | SOURCED |
| 102 | Appearance reset resets the whole card (#970) | `resetAppearanceDefaults` | `src/lib/components/settings/AppearanceCard.svelte`. | SOURCED |
| 105 | In-app token-storage reset (#766) | `reset_local_token_storage` | Registered at `src-tauri/src/app.rs:1188`; `src-tauri/src/commands/mod.rs:27, :109, :153` classify it guarded-vs-not. | SOURCED |
| 106 | Permanent token 400s end the session (#787) | `ReauthRequired` | `src-tauri/src/teams.rs`. | SOURCED |
| 107 | Diagnostics carries live sync state (#863) | `SyncState` block | `src-tauri/src/diagnostics.rs` + `src-tauri/src/polling/`. | SOURCED |
| 116 | Snooze chip a11y split (#736) | `.snooze-status` | `src/lib/components/Dashboard.svelte`. | SOURCED |
| 118 | Paused-track clear retries once (#929) | `teams_write_with_optional_refresh` | `src-tauri/src/polling/write.rs`; documented at `docs/STATE-OF-FEATURES.md:62`. | SOURCED |
| 119 | Color-scheme follows the painted theme (#959) | `color-scheme: dark/light` | `src/app.css` + `src/routes/+layout.svelte` `<meta name="color-scheme">` driven by `appliedTheme`. | SOURCED |
| 120 | German tray wording aligned (#901) | seven keys | All seven named keys (`open-logs-folder`, `status-syncing` + no-track sibling, `pause-sync`, `resume-sync`, `manual-status-clear`, `profile-empty`) exist in `src-tauri/src/i18n.rs`'s `DE` table. | SOURCED |
| 121 | Update banner token system (#902) | `--z-update-banner` | `src/app.css`. | SOURCED |
| 122 | Tray/deep-link cannot panic on unmanaged state (#937) | `try_state` | `src-tauri/src/tray/actions.rs::force_tray_refresh` + `src-tauri/src/deep_link.rs`. | SOURCED |
| 123 | Logger flushes before build-failure exit (#947) | `log::logger().flush()` | `src-tauri/src/app.rs::run`. | SOURCED |
| 132 | Config unknown keys / schema floor / staged imports (#767/#938/#939/#1131) | `extra`, `.max()`, `IMPORT_SECTION_KEYS` | `src-tauri/src/config/schema.rs` `#[serde(flatten)] pub extra`; `stamp_schema_version` uses `.max()`; `src-tauri/src/commands/config.rs:705` `const IMPORT_SECTION_KEYS: [&str; 11]`. | SOURCED |
| 133 | Detached panes skip link (#743) | `id="main-content"` | `src/routes/detached/[pane]/+page.svelte` wraps every branch. | SOURCED |
| 136 | Unknown menu events redacted (#918) | `menu_event_id_for_log` | `src-tauri/src/tray/mod.rs`; documented at `docs/STATE-OF-FEATURES.md:46`. | SOURCED |
| 137 | Least-privilege capability (#919) | `default.json` | `src-tauri/src/capabilities/default.json` — unused global-shortcut and autostart grants absent; documented at `docs/architecture/tray-and-shell.md:44-46`. | SOURCED |
| 138 | CSP blocks form submissions (#924) | `form-action 'none'` | `src-tauri/tauri.conf.json` CSP, pinned by `updater_bg::test_tauri_conf_disallows_downgrades` (AGENTS.md:389). | SOURCED |
| 139 | Rust-owned bounded diagnostics saves (#921) | 256 KiB cap | `src-tauri/src/diagnostics.rs`; documented at `docs/architecture/frontend.md:42-51` and `AGENTS.md:393-394`. | SOURCED |
| 140 | Headless CLI reads are read-only (#840) | `TokenReadMode::ReadOnly` | `src-tauri/src/token_io.rs:215` `pub enum TokenReadMode`; `src-tauri/src/cli.rs:266` and `src-tauri/src/app.rs:628` pass `ReadOnly`. | SOURCED |
| 141 | Detached windows opened from Rust (#922) | `detach_pane` | `src-tauri/src/app.rs:123` `fn detach_pane(...)`; registered at `app.rs:1188`; documented at `docs/STATE-OF-FEATURES.md:34` and `docs/architecture/tray-and-shell.md:12-15`. | SOURCED |
| 142 | Guard matrix covers all 54 commands (#771) | `test_guard_matrix_covers_every_registered_command` | `src-tauri/src/commands/mod.rs:554`; it brace-counts **54** entries out of the `generate_handler![…]` list at `src-tauri/src/app.rs:1149`, and asserts the matrix in `mod.rs` is exact both ways. Fourteen `commands/*.rs` files are mapped at `commands/mod.rs:9-153`. | SOURCED — the "54" in the CHANGELOG, in `docs/STATE-OF-FEATURES.md:35`, and in the tree agree. |
| 150 | Theme/density test named for its subject (#775) | `theme-density.test.ts` | `tests/theme-density.test.ts` exists. **But the CHANGELOG's claim "The former `tests/hygiene.test.ts` is now `tests/theme-density.test.ts`" is false on disk**: `tests/hygiene.test.ts` also still exists (358 lines, guarding colour literals, `#903` button boxes, `#960` size tokens and `#961` primitives), and is cited as live by `docs/STATE-OF-FEATURES.md:45`, `:124`, `:125`. The bullet's follow-on clause — "no current config, script, workflow, or non-historical documentation reference names the old filename" — is therefore also false. | **HALF-TRUE** — the "rename" is really a *split*; one file was added, the other retained. |
| 151 | Weekday-key coverage (#773) | `WEEKDAY_KEYS` | `tests/i18n.test.ts`; documented at `docs/STATE-OF-FEATURES.md:61`. | SOURCED |
| 152 | Detached route branches rendered (#857) | `detached.test.ts` | `tests/detached.test.ts` + `tests/detached-skip-link.test.ts` + `tests/browser/detached-skip-link.spec.ts`. | SOURCED |
| 153 | Native-literal guard (#843) | production literal scan | `src-tauri/src/i18n.rs` scanner; documented at `docs/STATE-OF-FEATURES.md:109`. | SOURCED |
| 154 | Teams-write tests **without** Tauri `test` feature (#929 rework) | `[dev-dependencies] tauri` | **ABSENT.** `grep -n "dev" src-tauri/Cargo.toml` returns exactly one hit — `:47`, a *comment* ("No tauri test-feature dev-dep here: …"). There is no `[dev-dependencies]` block and no `[target.'cfg(not(windows))'.dev-dependencies]` block anywhere in the manifest. | **UNSOURCED (understated, not false).** The removal the bullet describes did happen, but it took the whole dev-dependencies section with it — the manifest carries no `CapturingEmitter` / `markers` lane either. |
| 155 | Teams-write tests **with** Tauri `test` feature, target-gated (#929 B1/B2) | `[target.'cfg(not(windows))'.dev-dependencies]` | **ABSENT.** Same grep: the block the bullet says is "restored" does not exist in `src-tauri/Cargo.toml`. The bullet also contradicts line 154 (which says the dev-dep is *removed*). | **UNSOURCED.** Line 155 describes a state the manifest does not carry, and no binary-fingerprint experiment or target-gated block is present to substantiate it. Resolve alongside line 154 before the release cut. |
| 158 | AGENTS.md is the single agent contract | `AGENTS.md`, `CLAUDE.md` | `AGENTS.md` (720 lines) and `CLAUDE.md` (17 lines) both present. | SOURCED |
| 159 | Stale code citations corrected (#856) | `polling.rs`, `commands.rs` | `grep -rn 'polling\.rs\|commands\.rs' src-tauri/src` returns no matches (per `docs/STATE-OF-FEATURES.md:119`). | SOURCED |
| 160 | Stale comment citations swept (#908) | nine files | The sweep is real, but `docs/architecture/auth-and-tokens.md:138` still cites `lib.rs::handle_deep_link`, which now lives in `src-tauri/src/deep_link.rs`. | SOURCED — doc-side residue remains. |


**Claim:** "The `## [5.0.0] - 2026-10-06` section cut on 6 Oct was premature — no
`v5.0.0` tag was ever pushed, so the entries are folded back into Unreleased".

**VERDICT: TRUE — verified independently on all three sub-claims.**

| Sub-claim | Evidence | Result |
|---|---|---|
| No `v5.0.0` tag exists | `git tag --list 'v5*'` returns **no output** (empty). `git tag --sort=-v:refname \| head -5` returns `v4.7.0`, `v4.6.0`, `v4.5.2`, `v4.5.1`, `v4.5.0` — the newest tag in the repo is `v4.7.0`. | CONFIRMED — there is no `v5*` tag of any kind, so `v5.0.0` was never pushed. |
| `git log --oneline -5` matches | `09341ec test(sync,scans): reorder-mutation proof + Why comments on every surviving scan (#778) (#1170)` / `7e1c3ab refactor(i18n): own LocaleState from AppState, delete CURRENT + LOCALE_TEST_LOCK (#758, slice 3) (#1169)` / `11a146c docs(changelog): fold the premature 5.0.0 section back into Unreleased (#1168)` / `9b41cb0 refactor(tray,config): own AppCaches from AppState, delete quarantine-era statics (#758, slice 2) (#1167)` / `a01ea60 refactor(settings): extract slice-2 cards, completing the thirteen (#750) (#1166)` | CONFIRMED — commit `11a146c` is literally titled "docs(changelog): fold the premature 5.0.0 section back into Unreleased (#1168)", which is the exact fold the prose describes. HEAD is `09341ec`, matching the orchestrator fact. |
| Exactly one `## [Unreleased]` heading | `grep -n '^## \[' CHANGELOG.md` → the only `## [Unreleased]` is at **line 8**. No `## [5.0.0]` header exists anywhere in the file (`grep -n '^## \[5' CHANGELOG.md` → no match). | CONFIRMED — exactly one, at line 8, and the premature section's content is now folded in above line 162 (`## [4.7.0] - 2026-09-17`). |

The prose also cites `docs/RELEASING.md` §2 as the authority for the release-cut
rule; that section (CHANGELOG rename / fresh empty `## [Unreleased]` / new link
definition, lines 72–95) is present and consistent.

---

## UNDOCUMENTED SET

The 27 bullets whose `Documented in` is exactly **NONE** — no in-scope document
mentions the feature, symbol, module path or UI label at all. Line numbers are
CHANGELOG.md line numbers. Grouped by the CHANGELOG class they sit under.

Distribution: **Added 3**, **Changed 2**, **Fixed 18**, **Security 1**,
**Test 3**, **Docs 0**.

### Added — 3

| Line | Feature | What is missing from the docs |
|---|---|---|
| 16 | User-opt-in `setUserPreferredPresence` integration (#866) | the whole `teams.preferred_presence { enabled, availability, activity, expiry_minutes }` overlay, the clearing points (expiry, snooze-end, `RunEvent::Exit`), and the national-cloud / `/users/{oid}` fallback caveat |
| 18 | Volume, seek and documented playback capability flags (#871) | `volume_percent` / `supports_volume` / `DeviceActions`, `player_set_volume` / `player_seek`, the tray Volume (0/25/50/75/100) and Seek (±30 s) submenus, the Dashboard volume slider and click-to-seek bar, and the capability-disable treatment for shuffle/repeat/prev/next |
| 19 | Bounded status-decision history with a Dashboard "Activity" card (#877) | `src-tauri/src/history.rs`, the 200-entry `VecDeque<PresenceHistoryEntry>` ring, the six recorded decision kinds, the 20-row Dashboard card, the redacted snapshot mirror, and the `logging.presence_history` opt-in |

### Changed — 2

| Line | Feature | What is missing |
|---|---|---|
| 39 | Cached shared HTTP client, validated playback device ids, unified provider log tags (#884/#822/#777) | `src-tauri/src/http.rs` and its shared retry/expiry helpers, the `playback_transfer` device-id percent-encoding/validation, and the `[TEAMS]` / `[SPOTIFY]` tag constants with their guard test |
| 41 | `#932` rework — per-path `commit_*_spotify_session` wrappers removed | the `commit_spotify_session` seam, the two source guards (`manual_paste_handler_uses_commit_spotify_session_seam`, `deep_link_handler_uses_commit_spotify_session_seam`), and `src/lib/utils/routeReconnect.ts` + `tests/routeReconnect.test.ts` |

### Fixed — 18

| Line | Feature | What is missing |
|---|---|---|
| 74 | `complete_onboarding` returns machine-readable missing-token codes | the `spotify_not_connected` / `teams_not_connected` codes replacing the internal English sentence, and the #978 wizard-routing follow-up |
| 76 | Onboarding wizard offers a "Back to dashboard" step (#967) | the new wizard step and the C2-guard carve-out for already-configured (Spotify + Teams connected) installs |
| 77 | `--sync-once` no longer registers global hotkeys (#769) | the process statement that a one-shot leaves no listener attached to the system, so `start_syncing` is the only thing reaching `register_from_config` |
| 79 | LogViewer backfill no longer duplicates the live stream (#958) | the stable-key seed-vs-live merge that makes a record logged during backfill render and count once |
| 80 | Log timestamps keep their dates and follow the app locale (#969) | the date cell for backfilled rows from other days, and the `config.locale`-driven log + snooze-chip clocks replacing the OS default |
| 82 | Teams errors show an actionable sentence, not the raw Graph body (#974) | `TeamsApiError::user_message()` naming the permission/license cause on 403, its use by the Dashboard banner and device-code failures, and the "raw body stays in logs" split |
| 84 | The surname Dix no longer trips the profanity filter (#827) | the glue-requirement rule (`Dix`/`Dix's`/`Dixon` clean, `dixs` still flags). TROUBLESHOOTING.md:261-270 lists the general rules without it |
| 85 | One menu click fires exactly once (#804) | the single tray/window dispatcher, the removed duplicate registration and the removed twin id arms — a structural fact readers of docs/architecture/tray-and-shell.md:198-232 would need |
| 90 | A failed refresh no longer wipes a newer session installed mid-flight (#798) | the slot-verdict-on-the-failure-path contract (dead-credential branches clear and prompt reconnect only when the slot still holds the token the refresh actually tried) |
| 94 | The autostart toggle persists with the OS entry (#811) | `config.autostart` being written through the guarded path so later unrelated saves stop reverting the login entry |
| 95 | Food and place names stop tripping the profanity filter (#812) | the whole-inflection continuation rule and the per-stem carve-outs (`Spices`, `Spiced`, `crapes`, `Pizzeria` staying clean) |
| 99 | Menu navigation respects unsaved Settings edits (#817) | the dirty-draft park and the Save/Discard banner on menu navigation, instead of discarding the draft |
| 102 | The Appearance reset resets the whole card (#970) | `resetAppearanceDefaults` restoring theme `system`, density `comfortable` and locale `en` (+ `i18n.set`) alongside autostart |
| 103 | The LogViewer filter strip drops its partial tab pattern (#734) | the `.seg` group moving to `role="group"` + `aria-pressed`, away from a `tablist` with no tabpanel, no roving tabindex and no arrow handling |
| 105 | The corrupt-key error offers an in-app token-storage reset (#766) | `reset_local_token_storage`, its main-window guard, the keychain-before-file ordering, the sidecar sweep, the arm/confirm gate, and the re-sign-in copy |
| 106 | Permanent token-endpoint 400s end the session instead of retrying forever (#787) | the `interaction_required` / `consent_required` / `invalid_client` / `unauthorized_client` / `invalid_scope` → `ReauthRequired` mapping that clears the session and emits `teams-reconnect-required` once |
| 124 | LogViewer toolbar wrap and overflow contract (#948/#1134) | the `flex-wrap: wrap` + `row-gap` toolbar, the shrinking/scrolling level strip, and the two fixed numbers: the 600×750 default overflowed, and at the 400×500 declared minimum the six-button strip was 425 px inside a 360 px pane |
| 127 | Undo for a removed quiet-hours row or track rule (#981/#1135) | the Undo control in the rules card header and its restore-with-exact-prior-field-values behaviour |

### Security — 1

| Line | Feature | What is missing |
|---|---|---|
| 140 | Headless CLI token reads are strictly read-only (#840) | `TokenReadMode::ReadOnly` — that `--status` and headless reads parse legacy plaintext **without** keychain-key creation, migration, chmod, rename or rewrite, while GUI reads keep `MigrateLegacy`. README.md:162 and USAGE.md:292-293 both describe `--status` without stating this boundary at all. |

### Test — 3

| Line | Feature | What is missing |
|---|---|---|
| 150 | Theme/density coverage is named for its actual subject (#775) | `tests/theme-density.test.ts`. Note the bullet is half-true: `tests/hygiene.test.ts` still exists alongside it and is cited as live by docs/STATE-OF-FEATURES.md:45, :124, :125 |
| 154 | Shared Teams-write retry tests run **without** Tauri's `test` feature (#929 rework) | that the Windows loader hazard forces the non-`tauri::test` test shape via injectable emit/persist seams and a module-scope capturing emitter. AGENTS.md:199-204 hints at the hazard but not the test shape |
| 155 | Shared Teams-write retry tests run **with** Tauri's `test` feature, target-gated to non-Windows (#929 B1/B2) | the target-gated dev-dependency, the binary-fingerprint evidence, and the comment-and-string-literal-stripping assertion hardening. This entry also contradicts line 154 |

### Docs — 0

Every Docs-class bullet (158, 159, 160) is covered by an in-scope document:
`AGENTS.md` / `CLAUDE.md` and `docs/STATE-OF-FEATURES.md:114` for line 158, and
`docs/STATE-OF-FEATURES.md:119` for lines 159 and 160. Line 160's residual
problem is a *stale* citation (`docs/architecture/auth-and-tokens.md:138` still
points at `lib.rs::handle_deep_link`), not an undocumented one, so it is listed
as PARTIAL in the matrix rather than in this set.

---

## Appendix — action summary

`Action` tallies across all 135 matrix rows (recounted from the table itself):

| Action | Rows |
|---|---|
| NONE (accurate and documented) | 75 |
| DOCUMENT (no in-scope doc mentions it, or only the wrong half does) | 47 |
| FIX (a doc statement exists but contradicts the tree) | 10 |
| DEDUPE (verbatim duplicate CHANGELOG entry) | 3 |

`Accurate?` tallies: **YES 77**, **PARTIAL 21**, **NO 10**, **N/A 27**
(27 rows are `—` because no in-scope document mentions the feature at all).

`FIX` rows — every one carries its contradicting `path:line`:

| Line | Feature | Contradicting citation |
|---|---|---|
| 22 | `--serve` (#865) | README.md:167 and USAGE.md:297 call the API "read-only" and "the surface `--status` reads from" — the tree also serves mutating `POST /pause` / `/resume` / `/snooze` / `/profile` (src-tauri/src/serve.rs:8, :502) and `--status` reads tokens directly (src-tauri/src/cli.rs:266) |
| 23 | `--daemon` (#896) | README.md:168's table row is malformed — the `--daemon` cell absorbs the tail of `--sync-once`'s exit-code description ("…on success, or `1` with the reason on **stderr**…"), so `--sync-once`'s documented exit behaviour is lost |
| 24 | Track rules extended (#868) | USAGE.md:124 and the field table at USAGE.md:126-133 still describe artist/track-substring matching only, omitting `match_kind`, `album_substring`, `show_substring`, `device_substring`, `playlist_uri`, `min_duration_seconds`, `negate` and the discriminated `action` (src-tauri/src/config/schema.rs:649-658) |
| 29 | Tray split (#756) | AGENTS.md:127 (layout tree) and AGENTS.md:208 present `src-tauri/src/tray.rs` as a file; the tree is `src-tauri/src/tray/{mod,cache,dedup,snooze,devices,actions,testkit}.rs` |
| 49 / 145 | `lib.rs` split (#757) | docs/architecture/frontend.md:224 still shows a flat `lib.rs # Tauri entry, command registration, AppState`; the tree is a 33-line registry plus `app.rs` / `cli.rs` / `deep_link.rs` / `state.rs` |
| 50 / 146 | Config split (#755) | docs/architecture/frontend.md:243 and docs/architecture/overview.md:20-22 still cite `config.rs::save_config()` / `atomic_write_json`; the tree is `src-tauri/src/config/{schema,clamp,snooze,patch,migrate,io,transfer,mod}.rs` |
| 51 / 147 | Polling split (#754) | docs/architecture/polling.md:9-12, docs/architecture/frontend.md:238-242 and docs/architecture/overview.md:62 still cite `polling/poll_once.rs` as one file; the tree has ten split slices plus `mod.rs` |
| 60 | Bounded update checks + `.deb`/`.rpm` updatable (#940/#894/#782) | docs/RELEASING.md:268-273 states a `.deb`/`.rpm` install "has no AppImage to replace, so those users update through their package manager", contradicting `install_method_for(bundle_type())` at src-tauri/src/updater_bg.rs:1344; docs/PLATFORMS.md:12's Linux row names only the AppImage payload |
| 59 | Spotify sign-in keeps the live session (#932) | No doc records the **wire-format break** — `teams-auth-persist-warning` now emits `{ provider, message }` instead of a bare string (external listeners on the old shape receive the stringified object) |
| 111 | macOS leg runs the suite; Windows check-only (#836) | docs/RELEASING.md:131-133 and docs/STATE-OF-FEATURES.md:113 both say to re-expand Windows "when the image links cleanly"; .github/workflows/ci.yml:82-83 records the cause as the `tauri::test` dev-dependency hazard, not the image |
| 150 | Theme/density coverage named for its subject (#775) | The bullet claims no non-historical documentation names the old filename, but docs/STATE-OF-FEATURES.md:45, :124 and :125 all cite `tests/hygiene.test.ts` as a live guard — and the file still exists alongside `tests/theme-density.test.ts` |
| 160 | Stale comment citations swept (#908) | docs/architecture/auth-and-tokens.md:138 still cites `lib.rs::handle_deep_link`, which now lives in `src-tauri/src/deep_link.rs` |

Plus five rows that are documented and accurate but only **partially** so, and
therefore carry `Action = DOCUMENT` rather than `NONE`: line 13 (five of the
eight locales documented; README.md:35, USAGE.md:176, TROUBLESHOOTING.md:180 and
docs/architecture/tray-and-shell.md:58 all still say three), line 17 (the CLI
flags but not the composer/tray submenu), line 20 (the scope and gate reason but
not the behaviour), line 21 (the scope but not the feature), and line 35 (the
ts-rs row generally, but not the `bigint`→`number` contract change).
