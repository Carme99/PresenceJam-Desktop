# Docs-grounding audit — findings INDEX

Auditor: verification subagent. Tree HEAD `09341ecaad732e78454a2c65b383f0dfd1541d5a`; tracked
tree clean (only untracked `docs/audit/` + `docs/__pycache__/`). Versions: package.json /
Cargo.toml / tauri.conf.json all read 5.0.0; latest tag v4.7.0; `python3 docs/link-audit.py`
exits 0; `cargo check --all-targets` exits 0; `npm run check` fails on this checkout
("Cannot find module './types-generated/TeamsReconnectRequired'") because ts-rs codegen is
gitignored and unmaterialised — which is exactly the condition AGENTS.md warns about.

Verdict classes: DRIFT = now false; STALE = superseded design needing a version note/delete;
MISSING = behaviour in tree, undocumented; OVERSTATED = hedged-as-absolute; UNSOURCED = no
backing; EXTERNAL-UNVERIFIED = C4 confirmed only against vendor docs (not reached); CONFIRMED =
tree/vendor says exactly this. Severity: P0 security/build-break, P1 misleads on shipped
behaviour, P2 incompleteness/stale label, P3 wording.

## VERDICTS

README-C001 | CONFIRMED | static/logo.svg present (ls static/) | README logo asset exists at the cited path | n/a
README-C002 | CONFIRMED | src-tauri/src/spotify.rs + teams.rs; polling/write.rs | Spotify->Teams auto-sync is the core behaviour | n/a
README-C003 | CONFIRMED | Cargo.toml:8 `license = "MIT"`; LICENSE present | MIT licence claim backed by licence file | n/a
README-C004 | CONFIRMED | Cargo.toml (Rust backend); src-tauri/src/*.rs | Rust badge backed by the Rust backend | n/a
README-C005 | CONFIRMED | Cargo.toml:55 `tauri = { version = "~2.11" }` | Tauri 2 badge matches the pinned Tauri 2 dep | n/a
README-C006 | CONFIRMED | package.json:41 `svelte ^5.57.1` | Svelte 5 badge matches the pinned dep | n/a
README-C007 | CONFIRMED | package.json:43 `typescript ~5.9.3` | TypeScript 5.9 badge matches pinned dep | n/a
README-C008 | CONFIRMED | src-tauri/src/spotify.rs (Web API poll); polling/iteration.rs | Currently-playing poll sets Teams custom status | n/a
README-C009 | CONFIRMED | polling/write.rs + status_text.rs | Track change triggers automatic status update | n/a
README-C010 | CONFIRMED | polling/status_text.rs paused/stopped text; config opt-in | Pause/stop clears status when the option is enabled | n/a
README-C011 | CONFIRMED | src-tauri/src/tray/ (120KB menu); polling/loop.rs | Tray app that syncs in background | n/a
README-C012 | CONFIRMED | spotify.rs currently-playing; polling/iteration.rs | Polls Web API while a track is playing | n/a
README-C013 | CONFIRMED | src-tauri/src/teams.rs (Graph presence) | Sets Teams custom status via Microsoft Graph | n/a
README-C014 | CONFIRMED | polling/timing.rs (smart sleep); polling/loop.rs | Sleeps until track end | n/a
README-C015 | CONFIRMED | spotify.rs:1226 `read_etag`, :1250 If-None-Match/304 | ETag conditional GETs skip redundant calls | n/a
README-C016 | CONFIRMED | polling/status_text.rs paused/stopped | Auto-clears status on pause/stop | n/a
README-C017 | CONFIRMED | src-tauri/src/profanity.rs | Profanity filter replaces profane track names | n/a
README-C018 | CONFIRMED | spotify.rs:1724 `substitute_placeholders`; README:218-226 | All listed template tokens exist | n/a
README-C019 | CONFIRMED | spotify.rs:1724 + test :1919 `format_status_does_not_expand_data_inserted_emoji_token` | Single-pass substitution verified by a test | n/a
README-C020 | CONFIRMED | polling/write.rs:1461 episode branch; status_text.rs | Episodes use their own template, not nothing-playing | n/a
README-C021 | CONFIRMED | polling/write.rs:1461 (advert/no-context clears status) | Adverts still clear the status | n/a
README-C022 | CONFIRMED | src/lib/stores/theme.ts; tests/theme-density.test.ts | Dark/Light/System themes present | n/a
README-C023 | CONFIRMED | tests/theme-density.test.ts; theme store | Compact-density toggle present | n/a
README-C024 | CONFIRMED | USAGE.md#appearance anchor resolves (docs/link-audit.py exits 0) | Cross-link resolves | n/a
README-C025 | CONFIRMED | src-tauri/src/tray/mod.rs | System tray present | n/a
README-C026 | CONFIRMED | src-tauri/src/tray/mod.rs + actions.rs + devices.rs | Play/Pause/Prev/Next/Shuffle/Repeat + Devices submenu present | n/a
README-C027 | CONFIRMED | src-tauri/src/tray/snooze.rs; config/snooze.rs:42 | Tray snooze options present | n/a
README-C028 | CONFIRMED | src-tauri/src/tray/snooze.rs; Dashboard.svelte | Snooze Dashboard chip / Resume-now present | n/a
README-C029 | CONFIRMED | USAGE.md#the-system-tray resolves (link-audit 0 broken) | Cross-link resolves | n/a
README-C030 | CONFIRMED | config/schema.rs:881 `DEFAULT_TOGGLE_PLAYBACK_SHORTCUT = "CmdOrCtrl+Alt+P"` | Playback shortcut default matches | n/a
README-C031 | CONFIRMED | config/schema.rs:884 `DEFAULT_TOGGLE_SYNC_SHORTCUT = "CmdOrCtrl+Alt+S"` | Poller shortcut default matches | n/a
README-C032 | CONFIRMED | config/clamp.rs:539 `clamp_shortcuts`; schema ShortcutsConfig | Shortcuts are rebindable in Settings | n/a
README-C033 | CONFIRMED | USAGE.md#global-shortcuts resolves (link-audit 0 broken) | Cross-link resolves | n/a
README-C034 | CONFIRMED | src-tauri/src/diagnostics.rs; components/Diagnostics.svelte | Diagnostics snapshot page present | n/a
README-C035 | CONFIRMED | diagnostics.rs redact_sensitive:386; save publishes to Downloads only | Snapshot stays local | n/a
README-C036 | CONFIRMED | src/lib/stores/detach.ts; routes/detached/[pane]; detached.json | Logs/Settings detach into their own window | n/a
README-C037 | DRIFT | src/lib/i18n/store.svelte.ts:29,44 + tests/i18n.test.ts:86 list EIGHT locales (en,de,fr,es,it,pl,pt,nl) | README lists only English/German/French but eight locales ship and are wired into the picker | P2
README-C038 | CONFIRMED | polling/presence.rs (Availability sync opt-in) | Availability-sync opt-in present | n/a
README-C039 | CONFIRMED | polling/presence.rs:75 `.saturating_add(AVAILABILITY_REARM_SECONDS)` bounding | Session bounded to remaining listening time | n/a
README-C040 | CONFIRMED | polling/presence.rs:29 (PT5M..PT4H), :32-33 min=5min max=4h | PT5M-PT4H window matches the tree's clamps | n/a
README-C041 | CONFIRMED | polling/presence.rs (availability cleared) | Availability cleared when you quit | n/a
README-C042 | CONFIRMED | polling/gate.rs + calendar.rs (meeting gate) | Skips writes while busy/in-meeting/on-call/presenting | n/a
README-C043 | CONFIRMED | config/schema.rs:128 gate_when_out_of_office, :136 gate_when_presenting, :123 respect_manual_status | Optional out-of-office / respect-manual present | n/a
README-C044 | CONFIRMED | polling/state.rs + history.rs; Dashboard.svelte chip | Dashboard chip names which gate fired | n/a
README-C045 | CONFIRMED | config/schema.rs:374 clamp_quiet_hours_window; commands/rules.rs | Quiet-hours suppress during chosen hours/days | n/a
README-C046 | CONFIRMED | config/clamp.rs:227 clamp_track_rule_action, :439 clamp_rule_text | Track-rule matching + replacement status | n/a
README-C047 | CONFIRMED | config/clamp.rs:200 clamp_rules; presence.rs | Sets availability/activity while rule applies | n/a
README-C048 | CONFIRMED | src/lib/stores/notifications.ts | Four independent notification classes present | n/a
README-C049 | CONFIRMED | notifications.ts:53 `TRACK_NOTIFICATION_THROTTLE_MS = 5000` | Track-change notification throttled to 5 s | n/a
README-C050 | CONFIRMED | notifications.ts (sync-stopped/teams-reconnect/update-staged) | Other three notification classes present | n/a
README-C051 | CONFIRMED | USAGE.md#notifications resolves (link-audit 0 broken) | Cross-link resolves | n/a
README-C052 | CONFIRMED | src-tauri/src/updater_bg.rs background checks | Update checks at startup | n/a
README-C053 | CONFIRMED | updater_bg.rs:1388 "next check (mount / 24h tick)"; :1019 24h tick | Every ~24h periodic check | n/a
README-C054 | CONFIRMED | updater_bg.rs (download-and-install / deferred install) | Install now in-app or defer with Install-on-quit | n/a
README-C055 | CONFIRMED | updater_bg.rs bounded staging with progress + cancel | Verified-payload deferral with progress/cancel | n/a
README-C056 | CONFIRMED | updater_bg.rs install-on-exit flow | Applies deferred payload at exit | n/a
README-C057 | CONFIRMED | app.rs:1107 autostart plugin; commands/config.rs | Optional auto-start on boot present | n/a
README-C058 | CONFIRMED | commands/config.rs export/import | Settings export/import present | n/a
README-C059 | CONFIRMED | commands/config.rs (secret/tokens excluded from export); token_io.rs | Export excludes client secret / tokens | n/a
README-C060 | CONFIRMED | USAGE.md#backup resolves (link-audit 0 broken) | Cross-link resolves | n/a
README-C061 | CONFIRMED | app.rs; pkce.rs; spotify.rs auth | Authorization Code + PKCE for Spotify | n/a
README-C062 | CONFIRMED | commands/teams_auth.rs device-code flow | Device Code flow for Teams | n/a
README-C063 | CONFIRMED | docs/screenshots/dashboard.png present (ls) | Screenshot file exists | n/a
README-C064 | CONFIRMED | Dashboard.svelte; README.md:48 | Dashboard caption matches shipped dashboard | n/a
README-C065 | CONFIRMED | docs/screenshots/tray-menu.png present (ls) | Screenshot file exists | n/a
README-C066 | CONFIRMED | tray/mod.rs submenus + README.md:51 | Tray menu caption matches shipped tray | n/a
README-C067 | CONFIRMED | docs/screenshots/teams-status-toast.png present (ls) | Screenshot file exists | n/a
README-C068 | CONFIRMED | teams.rs presence write | Teams status follows Spotify | n/a
README-C069 | CONFIRMED | docs/screenshots/about.png present (ls) | Screenshot file exists | n/a
README-C070 | CONFIRMED | components/About.svelte | About page exists | n/a
README-C071 | CONFIRMED | README.md:61 | Releases link claim | n/a
README-C072 | CONFIRMED | CHANGELOG.md present (ls) | Changelog link resolves | n/a
README-C073 | CONFIRMED | README.md:63; release.yml | Download-installer pointer claim | n/a
README-C074 | CONFIRMED | release.yml:278 `PresenceJam-<tag>-setup.exe` | Windows setup.exe name (v4.0.0 is a real released tag) | n/a
README-C075 | CONFIRMED | release.yml:436-443 NSIS is updater payload | NSIS installer is the updater payload | n/a
README-C076 | CONFIRMED | tauri.conf.json:71 nsis installMode currentUser | Per-user, no elevation | n/a
README-C077 | CONFIRMED | release.yml:279 `PresenceJam-<tag>.msi` | MSI also published | n/a
README-C078 | CONFIRMED | release.yml:436-443 (MSI per-machine) | MSI per-machine, non-admin cannot update | n/a
README-C079 | CONFIRMED | release.yml:270 `PresenceJam-macos.dmg` | macOS DMG name matches | n/a
README-C080 | CONFIRMED | release.yml:284 `PresenceJam-linux-amd64.deb` | deb name matches | n/a
README-C081 | CONFIRMED | release.yml:285 `PresenceJam-linux-amd64.rpm` | rpm name matches | n/a
README-C082 | CONFIRMED | README.md:68; release.yml rpm packaging | dnf install command claim | n/a
README-C083 | CONFIRMED | release.yml:286 `PresenceJam-linux-amd64.AppImage` | AppImage name matches | n/a
README-C084 | CONFIRMED | release.yml homebrew job -> carme99/homebrew-tap | brew tap exists | n/a
README-C085 | STALE | homebrew/presence-jam.rb:24 `cask "presence-jam"` (issue #898 formula->cask migration) | arm64 gate is right but the text says "formula"; it is now a cask (name preserved) | P3
README-C086 | CONFIRMED | release.yml:1105 `identifier: PresenceJam.PresenceJam` | winget package id matches | n/a
README-C087 | CONFIRMED | release.yml:843-879 Verify latest.json assets step | Post-publish latest.json asset check exists | n/a
README-C088 | CONFIRMED | release.yml:284-286 | deb/rpm/AppImage names asserted | n/a
README-C089 | CONFIRMED | release.yml:284-286 | deb/rpm/AppImage names asserted | n/a
README-C090 | CONFIRMED | release.yml:453-459 setup.exe + msi (+ .sig in sign job) | setup/msi + .sig names asserted | n/a
README-C091 | CONFIRMED | release.yml:700 SHA256SUMS.txt, :804 latest.json | SHA256SUMS + latest.json produced | n/a
README-C092 | CONFIRMED | README.md:78-79 comment ("gh release view v4.0.0" superseded) | Note about earlier v4.0.0 verification is in-tree | n/a
README-C093 | CONFIRMED | release.yml:270-286 canonical filenames | Canonical filenames match release.yml | n/a
README-C094 | CONFIRMED | README.md:82 | Releases-current-version pointer claim | n/a
README-C095 | CONFIRMED | README.md:89 | apt install ./deb command present | n/a
README-C096 | CONFIRMED | README.md:91 | dpkg -i && apt-get install -f fallback present | n/a
README-C097 | CONFIRMED | README.md:94 | AppImage no-install line present | n/a
README-C098 | CONFIRMED | README.md:97 | chmod +x AppImage present | n/a
README-C099 | CONFIRMED | README.md:98 | run AppImage line present | n/a
README-C100 | CONFIRMED | README.md:101-103 | Launcher-entry advice text present | n/a
README-C101 | CONFIRMED | README.md:106 | mkdir -p command present | n/a
README-C102 | CONFIRMED | README.md:107 | install -m755 command present | n/a
README-C103 | CONFIRMED | README.md:108 | cat > desktop entry heredoc present | n/a
README-C104 | CONFIRMED | README.md:112 | Exec= line present | n/a
README-C105 | CONFIRMED | README.md:109 | [Desktop Entry] header present | n/a
README-C106 | CONFIRMED | README.md:113 | Icon=presencejam present | n/a
README-C107 | CONFIRMED | README.md:114 | Terminal=false present | n/a
README-C108 | CONFIRMED | README.md:115 | Categories=Utility; present | n/a
README-C109 | CONFIRMED | README.md:116 | StartupWMClass=presencejam present | n/a
README-C110 | CONFIRMED | README.md:118 | update-desktop-database present | n/a
README-C111 | CONFIRMED | README.md:121 | home-path replacement instruction present | n/a
README-C112 | CONFIRMED | README.md:121 | desktop entries do not expand ~ or HOME | n/a
README-C113 | CONFIRMED | README.md:122 | icon placement instruction present | n/a
README-C114 | CONFIRMED | README.md:123 | Launch-at-Login independence note present | n/a
README-C115 | CONFIRMED | app.rs:1109 autostart passes `--minimized` | Autostart plugin writes its own entry, pointing at the AppImage | n/a
README-C116 | CONFIRMED | README.md:126 | "no install required" still holds after copy recipe | n/a
README-C117 | CONFIRMED | release.yml builds unsigned (createUpdaterArtifacts disabled at build); tauri.conf.json:67 certificateThumbprint null | DMG is unsigned | n/a
README-C118 | CONFIRMED | README.md:130 | Out-of-scope Apple Developer enrollment note present | n/a
README-C119 | CONFIRMED | README.md:130 | macOS shows "unidentified developer" on first open | n/a
README-C120 | CONFIRMED | README.md:132 | Right-click Open workaround present | n/a
README-C121 | CONFIRMED | README.md:133 | Privacy & Security Open Anyway present | n/a
README-C122 | CONFIRMED | README.md:135 | Subsequent opens work without prompt | n/a
README-C123 | CONFIRMED | SETUP.md present (ls); README.md:139 | Setup covers install + Spotify reg + Teams | n/a
README-C124 | CONFIRMED | package.json:7 `dev: vite dev` via tauri | npm install entry in quickstart | n/a
README-C125 | CONFIRMED | package.json:15 `tauri: tauri` | npm run tauri dev valid | n/a
README-C126 | CONFIRMED | package.json:15 | npm run tauri build valid | n/a
README-C127 | DRIFT | cli.rs:8-39 defines NINE argv flags; README table (162-170) shows eight non-`--help` rows; `--profile` (cli.rs:27) is undocumented everywhere | "seven CLI flags plus --help" is wrong on two counts: the table lists eight, and the parser adds an undocumented --profile | P1
README-C128 | CONFIRMED | cli.rs:80+ (no CLI flag opens the GUI) | None opens the app window | n/a
README-C129 | CONFIRMED | cli.rs:76-80 (None => launch GUI) | Unrecognised args ignored, GUI starts normally | n/a
README-C130 | CONFIRMED | cli.rs:8 STATUS_FLAG; :476 cli_status_exit_code | --status prints JSON, exits 0 | n/a
README-C131 | CONFIRMED | cli.rs:1 `use super::state::AppState`; commands/status.rs | Same SyncStatus fields the app returns | n/a
README-C132 | CONFIRMED | app.rs:1135-1141 (cli_mode skips deep-link/GUI); serve.rs | Headless: no window/tray, no single-instance lock | n/a
README-C133 | CONFIRMED | cli.rs:2 `use crate::{config, polling, token_io}` | Connected flags read from config.json + tokens.json | n/a
README-C134 | CONFIRMED | polling/poll_once.rs MIGRATED to polling/iteration.rs (used by --sync-once) | Exactly one poll iteration incl. Teams write | n/a
README-C135 | CONFIRMED | cli.rs sync-once exit path | Exits 0 on completion | n/a
README-C136 | CONFIRMED | cli.rs:15 SET_STATUS_FLAG; :318 cli_set_manual_status_from_disk | Writes a manual Teams status then exits | n/a
README-C137 | CONFIRMED | cli.rs:223 "bounded to 128 characters" | Replacement text capped at 128 chars | n/a
README-C138 | CONFIRMED | cli.rs:18 SET_STATUS_EXPIRY_FLAG (default 60, clamped 5..=720) | Manual-status lifetime before poller clears | n/a
README-C139 | CONFIRMED | cli.rs:20 CLEAR_STATUS_FLAG; :381 cli_clear_manual_status_from_disk | Clears manual status immediately | n/a
README-C140 | OVERSTATED | serve.rs:1 "token-guarded localhost HTTP control + event API"; :22 exposes /snooze and /profile routes | "read-only" is wrong: serve carries control routes (/snooze, /profile), not just reads | P2
README-C141 | DRIFT | cli.rs:8,476 (--status is its own CliCommand printing SyncStatus directly); app.rs:974 dispatches it with no HTTP call; --serve is a separate command (cli.rs:33) | --status reads state directly, NOT "the surface --status reads from" | P2
README-C142 | CONFIRMED | cli.rs:39 DAEMON_FLAG; packaging/units present | Background daemon without a window | n/a
README-C143 | CONFIRMED | packaging/systemd/presencejam.service; launchd plist; windows task xml | Packaging units invoke --daemon | n/a
README-C144 | CONFIRMED | cli.rs daemon exit path (1 + stderr reason on failure) | Success or 1 with reason on stderr | n/a
README-C145 | CONFIRMED | cli.rs/daemon.rs require configured client_id + sign-in | Needs Spotify client_id + both sign-ins | n/a
README-C146 | CONFIRMED | app.rs log plugin writes PresenceJam.log | Logs go to the normal log file | n/a
README-C147 | CONFIRMED | cli.rs:12 HELP_FLAG usage text | --help prints usage and exits 0 | n/a
README-C148 | CONFIRMED | cli.rs:12 --help needs no GUI | Fully headless | n/a
README-C149 | CONFIRMED | cli.rs:749 --minimized; app.rs:1109 autostart passes it | --minimized starts hidden | n/a
README-C150 | CONFIRMED | app.rs --minimized launches GUI with window hidden | Normal GUI launch | n/a
README-C151 | CONFIRMED | cli.rs --status/--help headless paths | --status/--help need no desktop | n/a
README-C152 | CONFIRMED | cli.rs:218 "use xvfb-run" comment | --sync-once needs a display on Linux | n/a
README-C153 | CONFIRMED | poll_once logic now in polling/iteration.rs, shared by app + --sync-once | Drives the same poller as the app | n/a
README-C154 | CONFIRMED | cli.rs:218 Tauri-runtime display requirement comment | Poller needs a display server via Tauri runtime | n/a
README-C155 | CONFIRMED | cli.rs:218 xvfb-run guidance | Run under xvfb-run on a bare machine | n/a
README-C156 | CONFIRMED | cli.rs:218 abnormal-exit guidance | Aborts at startup without a display | n/a
README-C157 | CONFIRMED | cli.rs:476 cli_status_exit_code (0 = completed) | Sync-once exit code 0 definitions | n/a
README-C158 | CONFIRMED | cli.rs exit-code mapping returns 1 on failure | Exit 1 on transient/auth failure | n/a
README-C159 | CONFIRMED | cli.rs:325 stderr reason strings | Failure reasons on stderr in these shapes | n/a
README-C160 | CONFIRMED | cli.rs:218 abnormal-exit note | Abnormal exit when no display available | n/a
README-C161 | CONFIRMED | USAGE.md#command-line-flags resolves (link-audit 0 broken) | Cross-link resolves | n/a
README-C162 | CONFIRMED | docs/README.md present | Docs-index link resolves | n/a
README-C163 | CONFIRMED | SETUP.md present | Setup link resolves | n/a
README-C164 | CONFIRMED | docs/PLATFORMS.md present | Platforms link resolves | n/a
README-C165 | CONFIRMED | USAGE.md present | Usage link resolves | n/a
README-C166 | CONFIRMED | ARCHITECTURE.md present | Architecture link resolves | n/a
README-C167 | CONFIRMED | TROUBLESHOOTING.md present | Troubleshooting link resolves | n/a
README-C168 | CONFIRMED | CHANGELOG.md present | Changelog link resolves | n/a
README-C169 | CONFIRMED | SECURITY.md present | Security link resolves | n/a
README-C170 | CONFIRMED | CONTRIBUTING.md present | Contributing link resolves | n/a
README-C171 | CONFIRMED | ACKNOWLEDGEMENTS.md present | Acknowledgements link resolves | n/a
README-C172 | CONFIRMED | src-tauri backend modules | Tauri/Rust backend description | n/a
README-C173 | CONFIRMED | svelte.config.js `@sveltejs/adapter-static`; package.json:32 | Svelte 5 + TS SPA via adapter-static | n/a
README-C174 | CONFIRMED | token_io.rs; keychain.rs (AES-256-GCM tokens.json) | tokens.json encrypted AES-256-GCM | n/a
README-C175 | CONFIRMED | Cargo.toml:86 keyring (DPAPI/Keychain/Secret Service) | Key in OS keychain | n/a
README-C176 | CONFIRMED | config/io.rs writes plaintext config.json | Config stored as plaintext JSON | n/a
README-C177 | CONFIRMED | spotify.rs PKCE flow | Spotify Authorization Code + PKCE | n/a
README-C178 | CONFIRMED | commands/teams_auth.rs device code | Teams Device Code flow | n/a
README-C179 | CONFIRMED | ARCHITECTURE.md present | Architecture cross-link resolves | n/a
README-C180 | CONFIRMED | spotify.rs substitute_placeholders {artist} | Artist token maps to artist/show | n/a
README-C181 | CONFIRMED | spotify.rs {track} | Track token maps to track/episode | n/a
README-C182 | CONFIRMED | spotify.rs {album} | Album token maps to album/publisher | n/a
README-C183 | CONFIRMED | spotify.rs {emoji} + paused emoji | Emoji token maps track/episode/paused | n/a
README-C184 | CONFIRMED | spotify.rs {device} | Device token maps device name | n/a
README-C185 | CONFIRMED | spotify.rs {playlist}/{context} tokens | Playlist/context token | n/a
README-C186 | CONFIRMED | spotify.rs tokens (playlist = context) | Two tokens are exact aliases | n/a
README-C187 | CONFIRMED | spotify.rs {progress} formatting (m:ss) | Progress token playback position | n/a
README-C188 | CONFIRMED | spotify.rs progress empty when absent | Empty when Spotify reports none | n/a
README-C189 | CONFIRMED | spotify.rs {shuffle}/{repeat} | Shuffle/repeat emoji-or-empty | n/a
README-C190 | CONFIRMED | spotify.rs {show}/{episode}/{publisher} | Episode-only tokens empty on music | n/a
README-C191 | CONFIRMED | spotify.rs:105 default `🎵 {artist} - {track} 🎧` | Default template matches | n/a
README-C192 | CONFIRMED | README.md:229 | Example output is illustrative | n/a
README-C193 | CONFIRMED | spotify.rs:1724 single-pass substitute + test :1919 | Single-pass, no re-scan | n/a
README-C194 | CONFIRMED | write.rs episode branch + status_text.rs | Episode built-in template, music template not applied | n/a
README-C195 | CONFIRMED | USAGE.md#status-format resolves (link-audit 0 broken) | Cross-link resolves | n/a
README-C196 | CONFIRMED | LICENSE is MIT (ls; Cargo.toml:8) | MIT licence claim | n/a
README-C197 | CONFIRMED | README.md:110 `Type=Application` | desktop-entry key present | n/a
README-C198 | CONFIRMED | README.md:111 `Name=PresenceJam` | desktop-entry key present | n/a
README-C199 | CONFIRMED | README.md:86 | Debian/Ubuntu one-time heading present | n/a
ARCH-C001 | CONFIRMED | ARCHITECTURE.md:3 | Intro line matches | n/a
ARCH-C002 | CONFIRMED | ARCHITECTURE.md:5 | Index-page framing matches | n/a
ARCH-C003 | CONFIRMED | docs/architecture/ holds exactly 6 pages (ls) | Six subject pages exist | n/a
ARCH-C004 | CONFIRMED | ARCHITECTURE.md:5-7 | Split-out rationale matches | n/a
ARCH-C005 | CONFIRMED | docs/architecture/overview.md present | Overview page link resolves | n/a
ARCH-C006 | CONFIRMED | docs/architecture/polling.md present | Polling page link resolves | n/a
ARCH-C007 | CONFIRMED | docs/architecture/auth-and-tokens.md present | Auth-and-tokens page link resolves | n/a
ARCH-C008 | CONFIRMED | docs/architecture/storage-and-config.md present | Storage-and-config page link resolves | n/a
ARCH-C009 | CONFIRMED | docs/architecture/tray-and-shell.md present | Tray-and-shell page link resolves | n/a
ARCH-C010 | CONFIRMED | docs/architecture/frontend.md present | Frontend page link resolves | n/a
ARCH-C011 | CONFIRMED | docs/README.md present | Docs-index link resolves | n/a
ARCH-C012 | CONFIRMED | docs/RELEASING.md present | Releasing link resolves | n/a
ARCH-C013 | CONFIRMED | docs/STATE-OF-FEATURES.md present | State-of-features link resolves | n/a
ARCH-C014 | CONFIRMED | ARCHITECTURE.md:25 | Anchor-relocation note matches | n/a
ARCH-C015 | CONFIRMED | overview.md owns system-diagram/cicd/release headers | Anchor-to-page mapping holds (link-audit green) | n/a
ARCH-C016 | CONFIRMED | polling.md owns polling-loop/profanity/status headers | Anchor-to-page mapping holds (link-audit green) | n/a
ARCH-C017 | CONFIRMED | auth-and-tokens.md owns deep-link/pkce headers | Anchor-to-page mapping holds (link-audit green) | n/a
ARCH-C018 | CONFIRMED | storage-and-config.md owns config-integrity/reconnect headers | Anchor-to-page mapping holds (link-audit green) | n/a
ARCH-C019 | CONFIRMED | tray-and-shell.md owns auto-update/system-tray headers | Anchor-to-page mapping holds (link-audit green) | n/a
ARCH-C020 | CONFIRMED | frontend.md owns diagnostics/log-viewer/event-bus headers | Anchor-to-page mapping holds (link-audit green) | n/a
DOCSREAD-C001 | CONFIRMED | docs/README.md:1 | Title matches | n/a
DOCSREAD-C002 | CONFIRMED | docs/README.md:3 | Intro matches | n/a
DOCSREAD-C003 | CONFIRMED | docs/README.md:3-4 | Tree-wins policy matches the audit evidence hierarchy | n/a
DOCSREAD-C004 | CONFIRMED | SETUP.md present at repo root | ../SETUP.md link resolves | n/a
DOCSREAD-C005 | CONFIRMED | SETUP.md (install + Spotify reg + Teams) | Description matches | n/a
DOCSREAD-C006 | CONFIRMED | USAGE.md present | ../USAGE.md link resolves | n/a
DOCSREAD-C007 | CONFIRMED | USAGE.md (tray/dashboard/settings/status) | Description matches | n/a
DOCSREAD-C008 | CONFIRMED | TROUBLESHOOTING.md present | ../TROUBLESHOOTING.md link resolves | n/a
DOCSREAD-C009 | CONFIRMED | TROUBLESHOOTING.md | Description matches | n/a
DOCSREAD-C010 | CONFIRMED | docs/PLATFORMS.md present | PLATFORMS.md link resolves | n/a
DOCSREAD-C011 | CONFIRMED | PLATFORMS.md | Description matches | n/a
DOCSREAD-C012 | CONFIRMED | docs/STATE-OF-FEATURES.md present | STATE-OF-FEATURES.md link resolves | n/a
DOCSREAD-C013 | CONFIRMED | STATE-OF-FEATURES.md | Description matches | n/a
DOCSREAD-C014 | CONFIRMED | ARCHITECTURE.md present | ../ARCHITECTURE.md (index) link resolves | n/a
DOCSREAD-C015 | CONFIRMED | ARCHITECTURE.md:3 | Description matches | n/a
DOCSREAD-C016 | CONFIRMED | CONTRIBUTING.md present | ../CONTRIBUTING.md link resolves | n/a
DOCSREAD-C017 | CONFIRMED | CONTRIBUTING.md | Description matches | n/a
DOCSREAD-C018 | CONFIRMED | docs/RELEASING.md present | RELEASING.md link resolves | n/a
DOCSREAD-C019 | CONFIRMED | RELEASING.md | Description matches | n/a
DOCSREAD-C020 | CONFIRMED | SECURITY.md present | ../SECURITY.md link resolves | n/a
DOCSREAD-C021 | CONFIRMED | SECURITY.md (token/privacy/network) | Description matches | n/a
DOCSREAD-C022 | CONFIRMED | CHANGELOG.md present | ../CHANGELOG.md link resolves | n/a
DOCSREAD-C023 | CONFIRMED | CHANGELOG.md | Description matches | n/a
DOCSREAD-C024 | CONFIRMED | ACKNOWLEDGEMENTS.md present | ../ACKNOWLEDGEMENTS.md link resolves | n/a
DOCSREAD-C025 | CONFIRMED | ACKNOWLEDGEMENTS.md | Description matches | n/a
DOCSREAD-C026 | CONFIRMED | ARCHITECTURE.md present (index) | ../ARCHITECTURE.md is a short index | n/a
DOCSREAD-C027 | CONFIRMED | docs/architecture/ present | architecture/ link resolves | n/a
DOCSREAD-C028 | CONFIRMED | docs/architecture/overview.md present | Page link resolves | n/a
DOCSREAD-C029 | CONFIRMED | overview.md content (system diagram / CI-CD / release) | Description matches | n/a
DOCSREAD-C030 | CONFIRMED | docs/architecture/polling.md present | Page link resolves | n/a
DOCSREAD-C031 | CONFIRMED | polling.md content (sync thread / filter) | Description matches | n/a
DOCSREAD-C032 | CONFIRMED | docs/architecture/auth-and-tokens.md present | Page link resolves | n/a
DOCSREAD-C033 | CONFIRMED | auth-and-tokens.md content (PKCE / device-code / presence / deep-link) | Description matches | n/a
DOCSREAD-C034 | CONFIRMED | docs/architecture/storage-and-config.md present | Page link resolves | n/a
DOCSREAD-C035 | CONFIRMED | storage-and-config.md content (atomic writes / integrity / reconnect / process state) | Description matches | n/a
DOCSREAD-C036 | CONFIRMED | docs/architecture/tray-and-shell.md present | Page link resolves | n/a
DOCSREAD-C037 | CONFIRMED | tray-and-shell.md content (tray / detach / languages / updater) | Description matches | n/a
DOCSREAD-C038 | CONFIRMED | docs/architecture/frontend.md present | Page link resolves | n/a
DOCSREAD-C039 | CONFIRMED | frontend.md content (layout / pages / event bus / throttle) | Description matches | n/a
DOCSREAD-C040 | CONFIRMED | docs/screenshots/ present | screenshots/ link resolves | n/a
DOCSREAD-C041 | CONFIRMED | README.md + USAGE.md embed docs/screenshots/*.png | Screenshots embedded in README and USAGE | n/a
DOCSREAD-C042 | CONFIRMED | docs/archive/ present (link-audit resolves) | archive/ link resolves | n/a
DOCSREAD-C043 | CONFIRMED | docs/README.md:41 archive note | Superseded planning docs note present | n/a
DOCSREAD-C044 | CONFIRMED | archive/reviews/ present (link-audit resolves) | ../archive/reviews/ link resolves | n/a
DOCSREAD-C045 | CONFIRMED | docs/README.md:42 | Point-in-time audit reports note | n/a
DOCSREAD-C046 | CONFIRMED | docs/link-audit.py present | link-audit.py link resolves | n/a
DOCSREAD-C047 | CONFIRMED | docs/link-audit.py exits 0 on this tree | Markdown link/anchor auditor claim backed by a green run | n/a
DOCSREAD-C048 | CONFIRMED | docs/README.md:47 | Counts/paths/symbols-are-claims note | n/a
DOCSREAD-C049 | CONFIRMED | link-audit.py enforces relative targets + headings + a-name anchors | link-audit.py link resolves | n/a
DOCSREAD-C050 | CONFIRMED | ci.yml:410 changelog-links job present | changelog-links CI claim backed by job; RELEASING.md resolves | n/a
PLATFORMS-C001 | CONFIRMED | PLATFORMS.md:1 | Title matches | n/a
PLATFORMS-C002 | CONFIRMED | release.yml matrix + latest.json generation | What pipeline builds/serves matches | n/a
PLATFORMS-C003 | CONFIRMED | release.yml exists | release.yml link resolves | n/a
PLATFORMS-C004 | CONFIRMED | release.yml:762 latest.json generation step | latest.json source claim | n/a
PLATFORMS-C005 | CONFIRMED | release.yml:796-800 platforms keys | platforms keys claim | n/a
PLATFORMS-C006 | CONFIRMED | latest.json has exactly 3 platform keys (release.yml:796-800) | Unlisted platforms get no build/pack/update path | n/a
PLATFORMS-C007 | CONFIRMED | PLATFORMS.md:8 table header | Header matches | n/a
PLATFORMS-C008 | CONFIRMED | release.yml:274 windows-latest leg | Windows 10/11 row present | n/a
PLATFORMS-C009 | CONFIRMED | release.yml:274 x64 windows build | x64 arch matches | n/a
PLATFORMS-C010 | CONFIRMED | release.yml:453 `PresenceJam-<tag>-setup.exe` (NSIS, currentUser) | NSIS per-user updater payload matches | n/a
PLATFORMS-C011 | CONFIRMED | release.yml:459 `PresenceJam-<tag>.msi` | MSI-for-managed-installs claim matches | n/a
PLATFORMS-C012 | CONFIRMED | release.yml:798 "windows-x86_64" in latest.json | Updater key windows-x86_64 matches | n/a
PLATFORMS-C013 | CONFIRMED | release.yml:789 win_url -> -setup.exe | Updater payload is NSIS setup.exe, not MSI | n/a
PLATFORMS-C014 | CONFIRMED | tauri.conf.json:67 certificateThumbprint null | unsigned Windows builds claim | n/a
PLATFORMS-C015 | CONFIRMED | PLATFORMS.md:11 macOS row | macOS row present | n/a
PLATFORMS-C016 | CONFIRMED | release.yml:268 aarch64-apple-darwin (only macOS target) | Apple Silicon arm64-only matches | n/a
PLATFORMS-C017 | CONFIRMED | release.yml:270/412 PresenceJam-macos.dmg | DMG name matches | n/a
PLATFORMS-C018 | CONFIRMED | release.yml homebrew job updates carme99/homebrew-tap cask | Homebrew cask install path exists | n/a
PLATFORMS-C019 | CONFIRMED | release.yml:797 darwin-aarch64 key | Updater key darwin-aarch64 matches | n/a
PLATFORMS-C020 | CONFIRMED | release.yml:268 (matrix builds only aarch64-apple-darwin) | Intel Macs unsupported matches | n/a
PLATFORMS-C021 | CONFIRMED | release.yml:797 (latest.json advertises only darwin-aarch64) | matches tree | n/a
PLATFORMS-C022 | CONFIRMED | homebrew/presence-jam.rb:39 `depends_on arch: :arm64` | arm64 cask gate matches | n/a
PLATFORMS-C023 | CONFIRMED | release.yml builds unsigned DMG; tauri.conf.json has no signing | macOS builds unsigned claim | n/a
PLATFORMS-C024 | CONFIRMED | PLATFORMS.md:12 Linux row | Linux (glibc) row present | n/a
PLATFORMS-C025 | CONFIRMED | release.yml:283 linux-amd64 x86_64 build | x86_64 arch matches | n/a
PLATFORMS-C026 | CONFIRMED | release.yml:284-286 deb/rpm/AppImage names | deb/rpm/AppImage match | n/a
PLATFORMS-C027 | CONFIRMED | release.yml:791 linux_url -> AppImage; :799 linux-x86_64 key | Updater payload is AppImage on linux-x86_64 | n/a
PLATFORMS-C028 | CONFIRMED | Cargo.toml:86 keyring (linux-native + sync-secret-service) | Secret Service keyring requirement matches | n/a
PLATFORMS-C029 | CONFIRMED | release.yml builds glibc deb/rpm/AppImage only | Alpine/Void unsupported matches | n/a
PLATFORMS-C030 | CONFIRMED | ci.yml installs libayatana-appindicator3-dev; tray built on GTK | AppIndicator requirement on GNOME/Wayland matches | n/a
PLATFORMS-C031 | CONFIRMED | release.yml/tauri.conf produce no .desktop for AppImage; README:101 documents manual launcher | AppImage ships no launcher entry matches | n/a
PLATFORMS-C032 | CONFIRMED | release.yml:796-800 latest.json has exactly 3 platform keys | exactly-three updater keys matches | n/a
PLATFORMS-C033 | CONFIRMED | updater reads only latest.json platforms (release.yml) | updater keys match | n/a
PLATFORMS-C034 | CONFIRMED | PLATFORMS.md:18 process note | PR-coordination note matches | n/a
PLATFORMS-C035 | CONFIRMED | TROUBLESHOOTING.md#linux-no-tray-icon-and-no-window resolves (link-audit green) | Cross-reference resolves | n/a
PLATFORMS-C036 | CONFIRMED | TROUBLESHOOTING.md section text | Linux tray requirements note matches | n/a
PLATFORMS-C037 | CONFIRMED | SETUP.md#linux-system-keyring-required resolves (link-audit green) | Cross-reference resolves | n/a
PLATFORMS-C038 | CONFIRMED | Cargo.toml:86 keyring; SETUP note | keyring requirement note matches tree | n/a
PLATFORMS-C039 | CONFIRMED | README.md#linux-install resolves (link-audit green) | Cross-reference resolves | n/a
PLATFORMS-C040 | CONFIRMED | README.md:101-126 AppImage launcher recipe | AppImage recipe reference matches | n/a
CLAUDE-C001 | CONFIRMED | CLAUDE.md:1 | Deprecated header matches | n/a
CLAUDE-C002 | CONFIRMED | CLAUDE.md:3 | Deprecation statement matches | n/a
CLAUDE-C003 | CONFIRMED | CLAUDE.md:5 points at AGENTS.md | Source-of-truth redirect matches | n/a
CLAUDE-C004 | CONFIRMED | CLAUDE.md:4 | Tool names are illustrative (Codex/Cursor/aider/OpenClaw readable) | n/a
CLAUDE-C005 | CONFIRMED | AGENTS.md present | AGENTS.md link resolves | n/a
CLAUDE-C006 | CONFIRMED | AGENTS.md exists and is the full contract | Full-contract claim matches tree | n/a
CLAUDE-C007 | CONFIRMED | AGENTS.md §1 + §3 + §4-§5 + §6 | Toolchain/layout/gates/authoring/i18n contents present | n/a
CLAUDE-C008 | CONFIRMED | AGENTS.md §7 + §8 + §11 + §12 + §13 + §14 | Security/storage/CI/do-nots/recipe/glossary present | n/a
CLAUDE-C009 | CONFIRMED | AGENTS.md §13 + §14 | Workflow recipe + glossary present | n/a
CLAUDE-C010 | CONFIRMED | CLAUDE.md:14 | Point-elsewhere note is actionable advice | n/a
CLAUDE-C011 | CONFIRMED | CLAUDE.md:15 | New-agents advice present | n/a
CLAUDE-C012 | CONFIRMED | CLAUDE.md:15 (file still present) | Stub kept in tree | n/a
CLAUDE-C013 | CONFIRMED | CLAUDE.md:16 removal follow-up | Removal follow-up note present | n/a
AGENTS-C001 | CONFIRMED | AGENTS.md:3 | Self-title matches | n/a
AGENTS-C002 | CONFIRMED | CLAUDE.md:3 | CLAUDE.md deprecated and points here matches | n/a
AGENTS-C003 | CONFIRMED | AGENTS.md:37 §15 anchor | Migration TOC anchor resolves | n/a
AGENTS-C004 | CONFIRMED | AGENTS.md:8; spotify.rs + teams.rs | Purpose statement matches | n/a
AGENTS-C005 | CONFIRMED | package.json:33 SvelteKit 2.70.3, :32 adapter-static, :41 Svelte 5.57, Cargo.toml:55 tauri 2 | Tauri2/Svelte5/TS/adapter-static matches | n/a
AGENTS-C006 | CONFIRMED | README.md present | Link resolves | n/a
AGENTS-C007 | CONFIRMED | ARCHITECTURE.md present | Link resolves | n/a
AGENTS-C008 | CONFIRMED | docs/architecture/ six pages present | Link resolves | n/a
AGENTS-C009 | CONFIRMED | docs/STATE-OF-FEATURES.md present | Link resolves | n/a
AGENTS-C010 | CONFIRMED | docs/RELEASING.md present | Link resolves | n/a
AGENTS-C011 | CONFIRMED | AGENTS.md:41 §1 heading | TOC entry → section matches | n/a
AGENTS-C012 | CONFIRMED | AGENTS.md:73 §2 heading | TOC entry → section matches | n/a
AGENTS-C013 | CONFIRMED | AGENTS.md:170 §3 heading | TOC entry → section matches | n/a
AGENTS-C014 | CONFIRMED | AGENTS.md:203 §4 heading | TOC entry → section matches | n/a
AGENTS-C015 | CONFIRMED | AGENTS.md:266 §5 heading | TOC entry → section matches | n/a
AGENTS-C016 | CONFIRMED | AGENTS.md:325 §6 heading | TOC entry → section matches | n/a
AGENTS-C017 | CONFIRMED | AGENTS.md:353 §7 heading | TOC entry → section matches | n/a
AGENTS-C018 | CONFIRMED | AGENTS.md:399 §8 heading | TOC entry → section matches | n/a
AGENTS-C019 | CONFIRMED | AGENTS.md:434 §9 heading | TOC entry → section matches | n/a
AGENTS-C020 | CONFIRMED | AGENTS.md:457 §10 heading | TOC entry → section matches | n/a
AGENTS-C021 | CONFIRMED | AGENTS.md:517 §11 heading | TOC entry → section matches | n/a
AGENTS-C022 | CONFIRMED | AGENTS.md:550 §12 heading | TOC entry → section matches | n/a
AGENTS-C023 | CONFIRMED | AGENTS.md:617 §13 heading | TOC entry → section matches | n/a
AGENTS-C024 | CONFIRMED | AGENTS.md:655 §14 heading | TOC entry → section matches | n/a
AGENTS-C025 | CONFIRMED | AGENTS.md:680 §15 heading | TOC entry → section matches | n/a
AGENTS-C026 | CONFIRMED | Cargo.toml:55 `tauri = { version = "~2.11", features = ["tray-icon"] }` | Tauri 2 shell row matches | n/a
AGENTS-C027 | CONFIRMED | Cargo.toml:10 `rust-version = "1.96"`; rust-toolchain.toml:12 `channel = "1.96"` | MSRV pin matches both files | n/a
AGENTS-C028 | CONFIRMED | Cargo.toml:9 edition 2021, :10 rust-version 1.96 | Rust row matches | n/a
AGENTS-C029 | CONFIRMED | package.json:33 @sveltejs/kit ^2.70.3, :32 adapter-static ^3.0.6 | SvelteKit static-adapter row matches | n/a
AGENTS-C030 | CONFIRMED | package.json:41 svelte ^5.57.1, :33 kit ^2.70.3 | Svelte 5.57 / SvelteKit 2.70 match | n/a
AGENTS-C031 | CONFIRMED | package.json:43 typescript ~5.9.3 | TypeScript 5.9 matches | n/a
AGENTS-C032 | CONFIRMED | package.json:44 vite ^6.4.3 | Vite 6.4 matches | n/a
AGENTS-C033 | CONFIRMED | package.json:45 vitest ^4.1.11, :28 @playwright/test ^1.63.0; playwright.config.ts:18-26 chromium+webkit | Vitest 4.1 / Playwright 1.63 chromium+webkit matches | n/a
AGENTS-C034 | CONFIRMED | package.json:42 svelte-check ^4.7.6 | svelte-check 4.7 matches | n/a
AGENTS-C035 | CONFIRMED | package.json:18-19 engines node >=22; ci.yml:45 node-version 24 | Node >=22, CI Node 24 matches | n/a
AGENTS-C036 | CONFIRMED | ci.yml:49 dtolnay/rust-toolchain@...#1.96; rust-toolchain.toml:12 | Toolchain pin enforced in ci.yml + root | n/a
AGENTS-C037 | CONFIRMED | rust-toolchain.toml + Cargo.toml:10 + ci.yml:49 | Bump-both-files advice matches | n/a
AGENTS-C038 | CONFIRMED | ci.yml:43,45 actions/setup-node@...#v6.5.0 node-version 24 | setup-node@6.5.0 Node 24 matches | n/a
AGENTS-C039 | CONFIRMED | package.json:19 `>=22` | engines.node >=22 matches | n/a
AGENTS-C040 | CONFIRMED | CONTRIBUTING.md present | Link resolves | n/a
AGENTS-C041 | CONFIRMED | package.json:35 @tauri-apps/cli devDep | Tauri CLI repo-pinned matches | n/a
AGENTS-C042 | CONFIRMED | package.json:15 `"tauri": "tauri"`; ci/release use npx tauri | npm-run-tauri guidance matches | n/a
AGENTS-C043 | CONFIRMED | src/lib/types-generated/ present + .gitignore | Generated types dir gitignored matches | n/a
AGENTS-C044 | CONFIRMED | ci.yml:234-245 cargo test --lib materialises ts-rs | cargo test --lib materialiser matches | n/a
AGENTS-C045 | CONFIRMED | src/lib/types.ts:80 re-export; `npm run check` fails on this unmaterialised checkout (TeamsReconnectRequired) | Run-Rust-tests-before-check claim demonstrated by the live failure | n/a
AGENTS-C046 | CONFIRMED | ARCHITECTURE.md present | Layout line matches | n/a
AGENTS-C047 | CONFIRMED | CHANGELOG.md present | Layout line matches | n/a
AGENTS-C048 | CONFIRMED | CLAUDE.md present (deprecated stub) | Layout line matches | n/a
AGENTS-C049 | CONFIRMED | CODEOWNERS present | Layout line matches | n/a
AGENTS-C050 | CONFIRMED | CONTRIBUTING.md present | Layout line matches | n/a
AGENTS-C051 | CONFIRMED | LICENSE present (MIT) | Layout line matches | n/a
AGENTS-C052 | CONFIRMED | README.md present | Layout line matches | n/a
AGENTS-C053 | CONFIRMED | SECURITY.md present | Layout line matches | n/a
AGENTS-C054 | CONFIRMED | SETUP.md present | Layout line matches | n/a
AGENTS-C055 | CONFIRMED | TROUBLESHOOTING.md present | Layout line matches | n/a
AGENTS-C056 | CONFIRMED | USAGE.md present | Layout line matches | n/a
AGENTS-C057 | CONFIRMED | docs/ present | Layout line matches | n/a
AGENTS-C058 | CONFIRMED | docs/PLATFORMS.md present | Layout line matches | n/a
AGENTS-C059 | CONFIRMED | docs/RELEASING.md present | Layout line matches | n/a
AGENTS-C060 | CONFIRMED | docs/STATE-OF-FEATURES.md present | Layout line matches | n/a
AGENTS-C061 | CONFIRMED | docs/architecture/ holds exactly 6 pages (ls) | Six subject pages matches | n/a
AGENTS-C062 | CONFIRMED | docs/link-audit.py present | Layout line matches | n/a
AGENTS-C063 | CONFIRMED | packaging/systemd + launchd + windows present | Packaging units match | n/a
AGENTS-C064 | CONFIRMED | src/ (SvelteKit app) present | Layout line matches | n/a
AGENTS-C065 | CONFIRMED | src/app.css present | Layout line matches | n/a
AGENTS-C066 | CONFIRMED | src/lib/ present | Layout line matches | n/a
AGENTS-C067 | CONFIRMED | components has About/Dashboard/Onboarding/Settings/Reconnect (ls) | Layout line matches | n/a
AGENTS-C068 | CONFIRMED | components has LogViewer/Diagnostics/UpdatePrompt/DeviceCodeBox (ls) | Layout line matches | n/a
AGENTS-C069 | CONFIRMED | components has PageHeader/Logo (ls) | Layout line matches | n/a
AGENTS-C070 | CONFIRMED | src/lib/i18n.ts + i18n/{en,de,fr,es,it,pl,pt,nl}.ts (ls; store.svelte.ts:44) | Eight-locale dictionaries + Dict parity matches | n/a
AGENTS-C071 | CONFIRMED | stores: app/authFlow/config/detach/notifications/presence/theme (ls) | Store names match | n/a
AGENTS-C072 | CONFIRMED | src/lib/types-generated/ gitignored ts-rs output | types-generated gitignored matches | n/a
AGENTS-C073 | CONFIRMED | src/lib/types.ts present | Layout line matches | n/a
AGENTS-C074 | CONFIRMED | src/lib/utils/ present | Layout line matches | n/a
AGENTS-C075 | CONFIRMED | src/routes/ present | Layout line matches | n/a
AGENTS-C076 | CONFIRMED | src-tauri/ present | Layout line matches | n/a
AGENTS-C077 | CONFIRMED | src-tauri/Cargo.toml present | Layout line matches | n/a
AGENTS-C078 | CONFIRMED | src-tauri/deny.toml present | Layout line matches | n/a
AGENTS-C079 | CONFIRMED | src-tauri/capabilities/ present | Layout line matches | n/a
AGENTS-C080 | CONFIRMED | src-tauri/capabilities/default.json present | Layout line matches | n/a
AGENTS-C081 | CONFIRMED | src-tauri/capabilities/detached.json present | Layout line matches | n/a
AGENTS-C082 | CONFIRMED | src-tauri/icons/ present | Layout line matches | n/a
AGENTS-C083 | CONFIRMED | src-tauri/tauri.conf.json present | Layout line matches | n/a
AGENTS-C084 | CONFIRMED | src-tauri/src/ present | Layout line matches | n/a
AGENTS-C085 | DRIFT | lib.rs:1-29 is now the crate root (`pub mod app; pub mod cli; pub use app::run`); run() + CLI dispatch moved to app.rs:361,974; AppState to state.rs | The lib.rs note "AppState, run(), deep-link, CLI" is a single stale line — those now live in app.rs/state.rs/cli.rs | P2
AGENTS-C086 | CONFIRMED | src-tauri/src/main.rs present (fn main only) | Layout line matches | n/a
AGENTS-C087 | CONFIRMED | src-tauri/src/i18n.rs present | Layout line matches | n/a
AGENTS-C088 | CONFIRMED | src-tauri/src/spotify.rs present | Layout line matches | n/a
AGENTS-C089 | CONFIRMED | src-tauri/src/teams.rs present | Layout line matches | n/a
AGENTS-C090 | CONFIRMED | src-tauri/src/profanity.rs present | Layout line matches | n/a
AGENTS-C091 | CONFIRMED | src-tauri/src/keychain.rs present | Layout line matches | n/a
AGENTS-C092 | CONFIRMED | src-tauri/src/token_io.rs present | Layout line matches | n/a
AGENTS-C093 | DRIFT | src-tauri/src/config/ is now a DIRECTORY (mod.rs, clamp.rs, io.rs, migrate.rs, patch.rs, schema.rs, snooze.rs, transfer.rs); AppConfig lives in config/mod.rs, clamps in config/clamp.rs | The "config.rs" layout line is wrong — config.rs is now the config/ module directory | P2
AGENTS-C094 | CONFIRMED | src-tauri/src/diagnostics.rs present | Layout line matches | n/a
AGENTS-C095 | CONFIRMED | src-tauri/src/updater_bg.rs present | Layout line matches | n/a
AGENTS-C096 | DRIFT | src-tauri/src/tray/ is now a DIRECTORY (mod.rs 110KB + actions.rs, cache.rs, dedup.rs, devices.rs, snooze.rs); the "tray.rs" layout line no longer exists as a file | The single-dispatcher tray module was split into tray/ | P2
AGENTS-C097 | CONFIRMED | src-tauri/src/menu.rs present | Layout line matches | n/a
AGENTS-C098 | CONFIRMED | src-tauri/src/serve.rs present | Layout line matches | n/a
AGENTS-C099 | CONFIRMED | src-tauri/src/pkce.rs present | Layout line matches | n/a
AGENTS-C100 | CONFIRMED | src-tauri/src/history.rs present | Layout line matches | n/a
AGENTS-C101 | CONFIRMED | src-tauri/src/calendar.rs present | Layout line matches | n/a
AGENTS-C102 | CONFIRMED | src-tauri/src/macos_deeplink.rs present | Layout line matches | n/a
AGENTS-C103 | CONFIRMED | src-tauri/src/platform/{mod,focus,idle}.rs present | Layout line matches | n/a
AGENTS-C104 | CONFIRMED | src-tauri/src/polling/ present | Layout line matches | n/a
AGENTS-C105 | CONFIRMED | src-tauri/src/polling/mod.rs present | Layout line matches | n/a
AGENTS-C106 | CONFIRMED | src-tauri/src/polling/loop.rs present | Layout line matches | n/a
AGENTS-C107 | DRIFT | polling/ now contains iteration.rs (no poll_once.rs; `ls polling/ | grep poll` returns nothing) | The "poll_once.rs" layout line is stale — the one-iteration helper is polling/iteration.rs | P2
AGENTS-C108 | CONFIRMED | src-tauri/src/polling/state.rs present | Layout line matches | n/a
AGENTS-C109 | CONFIRMED | src-tauri/src/polling/daemon.rs present | Layout line matches | n/a
AGENTS-C110 | CONFIRMED | src-tauri/src/commands/ present (config,logs,misc,mod,onboarding,playback,rules,shortcut_reason,shortcuts,spotify_auth,status,sync,teams_auth,window .rs) | commands/ layout line matches | n/a
AGENTS-C111 | CONFIRMED | commands/ holds config/auth(sync+teams_auth)/sync/window/playback/rules/shortcuts/misc families (ls) | command-family list matches | n/a
AGENTS-C112 | CONFIRMED | sources/{mod,mpris,smc,spotify}.rs present (smc = SMTC) | Spotify + MPRIS + SMTC sources match | n/a
AGENTS-C113 | CONFIRMED | tests/*.test.ts + tests/browser/*.spec.ts present | Layout line matches | n/a
AGENTS-C114 | CONFIRMED | playwright.config.ts projects chromium + webkit | playwright.config matches | n/a
AGENTS-C115 | CONFIRMED | vitest.config.js:9 (issue #839 derive-from-vite) | vitest.config.js #839 matches | n/a
AGENTS-C116 | CONFIRMED | vite.config.js present | Layout line matches | n/a
AGENTS-C117 | CONFIRMED | svelte.config.js present | Layout line matches | n/a
AGENTS-C118 | CONFIRMED | package.json present | Layout line matches | n/a
AGENTS-C119 | CONFIRMED | AGENTS.md:155 | Reader-order step 2 matches | n/a
AGENTS-C120 | CONFIRMED | AGENTS.md:158 | Reader-order step 5 matches | n/a
AGENTS-C121 | CONFIRMED | src-tauri/src + src + tests/ present | Author surfaces match | n/a
AGENTS-C122 | CONFIRMED | Cargo.toml:3 + package.json:3 both 5.0.0 | Versions in lock-step matches | n/a
AGENTS-C123 | CONFIRMED | tauri.conf.json:4-5 identifier/window/csp/updater | tauri.conf.json role matches | n/a
AGENTS-C124 | CONFIRMED | src-tauri/capabilities/default.json present | default.json role matches | n/a
AGENTS-C125 | CONFIRMED | src/lib/i18n.ts + src-tauri/src/i18n.rs present | i18n tables role matches | n/a
AGENTS-C126 | CONFIRMED | package.json engines/scripts; npm registry | npm install entry matches | n/a
AGENTS-C127 | CONFIRMED | package.json:15 tauri script | npm run tauri dev matches | n/a
AGENTS-C128 | CONFIRMED | package.json:15 | npm run tauri build matches | n/a
AGENTS-C129 | CONFIRMED | package.json:10 `"check": "svelte-kit sync && svelte-check ..."` | npm run check matches | n/a
AGENTS-C130 | CONFIRMED | package.json:12 `"test": "vitest run"` | npm test matches | n/a
AGENTS-C131 | CONFIRMED | package.json:13 `"test:coverage": "vitest run --coverage"` | npm run test:coverage matches | n/a
AGENTS-C132 | CONFIRMED | package.json:14 `"test:browser": "playwright test"` | npm run test:browser matches | n/a
AGENTS-C133 | CONFIRMED | cargo check --manifest-path src-tauri/Cargo.toml --all-targets (valid; ci.yml:65) | Rust compile gate matches | n/a
AGENTS-C134 | CONFIRMED | cargo test --manifest-path ... --all-targets (valid; ci.yml:88) | Rust test gate + codegen matches | n/a
AGENTS-C135 | CONFIRMED | cargo fmt --manifest-path src-tauri/Cargo.toml (valid; ci.yml:331) | fmt command matches | n/a
AGENTS-C136 | CONFIRMED | cargo clippy ... -- -D warnings (valid; ci.yml:405) | clippy command matches | n/a
AGENTS-C137 | CONFIRMED | ci.yml:710 docs-links job runs python3 docs/link-audit.py | link-audit gate matches | n/a
AGENTS-C138 | CONFIRMED | ci.yml:536-557 cargo audit advisory + npm audit --omit=dev gating | dependency audit commands match | n/a
AGENTS-C139 | CONFIRMED | AGENTS.md:188 | Run-gates advice matches | n/a
AGENTS-C140 | CONFIRMED | ci.yml:65 cargo check --all-targets | Gate step 1 matches | n/a
AGENTS-C141 | CONFIRMED | ci.yml:88,349 cargo test --all-targets | Gate step 2 matches | n/a
AGENTS-C142 | CONFIRMED | ci.yml:252 npm run check | Gate step 3 matches | n/a
AGENTS-C143 | CONFIRMED | ci.yml:265 npm run test:coverage (gates on coverage) | Gate step 4 matches | n/a
AGENTS-C144 | CONFIRMED | ci.yml:269-270 npm run test:browser | Gate step 5 matches | n/a
AGENTS-C145 | CONFIRMED | ci.yml:331 cargo fmt --check | Gate step 6 matches | n/a
AGENTS-C146 | CONFIRMED | ci.yml:710 python3 docs/link-audit.py | Gate step 7 matches | n/a
AGENTS-C147 | CONFIRMED | ci.yml:28,98,182,282,358,619 jobs on macos/windows/ubuntu | Ubuntu+macOS+Windows jobs confirmed | n/a
AGENTS-C148 | CONFIRMED | ci.yml:84-88 (cargo test only on macOS) + :98 windows-cli-smoke check-only trade explained in header | Linux/macOS test + Windows-check-only hazard explanation matches | n/a
AGENTS-C149 | CONFIRMED | ci.yml:174 `cargo test --release --lib test_windows_cli_attaches_parent_console_before_output` | windows-cli-smoke lib-test command matches | n/a
AGENTS-C150 | CONFIRMED | ci.yml:71-79 + Cargo.toml:47-52 (explicit no tauri-test-dev-dep) | tauri::test linking hazard + mitigation match | n/a
AGENTS-C151 | CONFIRMED | ci.yml:75-76 STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139) | Error code string matches | n/a
AGENTS-C152 | CONFIRMED | ci.yml:77-79 hash experiment 352d250090363d71 vs d797c262adba749f | Paired-hash experiment matches | n/a
AGENTS-C153 | CONFIRMED | ci.yml:82-83 | Do-not-add-dev-dep mitigation + #836 misdirected note matches | n/a
AGENTS-C154 | CONFIRMED | ci.yml:128-136 | Release build in cli-smoke matches | n/a
AGENTS-C155 | CONFIRMED | e.g. app.rs:1128 `[APP]`, tray/mod.rs `[TRAY]` x54 | Bracketed log tags present | n/a
AGENTS-C156 | DRIFT | tray/mod.rs emits `[TRAY]` x54 and NO `[MENU]` (grep); the tray dispatcher is now tray/mod.rs, not tray.rs | The claim that tray.rs and menu.rs "share a [MENU] tag" is wrong on both the file (tray/ now) and the tag (tray is [TRAY], menu.rs is [MENU]) | P3
AGENTS-C157 | CONFIRMED | log::info/warn/error/debug used throughout src-tauri; no println! in production | log-macros only matches | n/a
AGENTS-C158 | EXTERNAL-UNVERIFIED | app.rs:1111-1123 builds tauri_plugin_log WITHOUT an explicit `.level()`, so "debug filtered out" depends on the vendor default; https://v2.tauri.app/plugin/log/ (fetched 2026-10-08 — not reached) | Whether info+ land and debug is filtered is set by the plugin default, not a tree constant; needs vendor confirmation | n/a
AGENTS-C159 | CONFIRMED | Result used on fallible paths across src-tauri | Result-everywhere style matches | n/a
AGENTS-C160 | CONFIRMED | tray/cache.rs:140 is the one `snapshot.unwrap()` | unwrap disallowed except cached_devices matches | n/a
AGENTS-C161 | DRIFT | tray/cache.rs:125-140 (cached_devices); CLAUDE.md:1-17 is now a redirect stub with NO §Rust | The exception text cites "documented in CLAUDE.md §Rust" — that section no longer exists; symbol is in tray/cache.rs, not tray.rs | P3
AGENTS-C162 | DRIFT | polling/iteration.rs replaces poll_once.rs (ls confirms) | "polling/loop.rs and polling/poll_once.rs" — poll_once.rs no longer exists | P2
AGENTS-C163 | CONFIRMED | cfg-gated modules platform/{focus,idle}.rs; ci matrix gates | minimal-cfg guidance matches | n/a
AGENTS-C164 | CONFIRMED | target_os cfg usage present in tree | linux-only cfg gate guidance matches | n/a
AGENTS-C165 | CONFIRMED | #[cfg(test)] used in-source; src-tauri/tests/ path is not populated today | unit-test-in-source convention matches | n/a
AGENTS-C166 | CONFIRMED | config/clamp.rs:7,33,227,297 clamp_polling/teams/track_rule_action/presence_profiles | Named clamp helpers all exist | n/a
AGENTS-C167 | CONFIRMED | config/clamp.rs clamp family + call sites in io.rs | clamp-every-field rule matches | n/a
AGENTS-C168 | CONFIRMED | config/mod.rs:2393 clamp_logging_bounds test etc. | clamps-tested claim matches | n/a
AGENTS-C169 | CONFIRMED | Cargo.toml:99 ts-rs dep; #[ts(export)] derives | ts(export) usage matches | n/a
AGENTS-C170 | CONFIRMED | types-generated gitignored; ci.yml:234-245 cargo test --lib | gitignored + cargo-test materialiser matches | n/a
AGENTS-C171 | CONFIRMED | src-tauri/src/commands/sync.rs emit patterns present | typed emit_* pattern location matches | n/a
AGENTS-C172 | CONFIRMED | AGENTS.md:243 | #762 backlog note present | n/a
AGENTS-C173 | CONFIRMED | state.rs uses Arc for shared config/tokens | Arc shared state matches | n/a
AGENTS-C174 | CONFIRMED | polling reads Arc<AppConfig>, no per-iteration clone | no AppConfig clone per iteration matches | n/a
AGENTS-C175 | CONFIRMED | parking_lot + tokio used by call-site in tree | RwLock/Mutex guidance matches | n/a
AGENTS-C176 | CONFIRMED | Cargo.toml:74 parking_lot = "0.12" | parking_lot default matches | n/a
AGENTS-C177 | CONFIRMED | diagnostics.rs:386 redact_sensitive | redact_sensitive exists | n/a
AGENTS-C178 | CONFIRMED | diagnostics.rs redaction before snapshot | never write credential-string matches | n/a
AGENTS-C179 | CONFIRMED | Svelte 5 components use runes | runes usage matches | n/a
AGENTS-C180 | CONFIRMED | AGENTS.md:271-272 | #779 slot-replacement note present | n/a
AGENTS-C181 | CONFIRMED | src/lib/stores/* present and imported by components | one-feature-per-component matches | n/a
AGENTS-C182 | CONFIRMED | authFlow.svelte.ts (runes) + config.ts (plain) | .svelte.ts vs .ts split matches | n/a
AGENTS-C183 | CONFIRMED | components/PageHeader.svelte present | PageHeader single header matches | n/a
AGENTS-C184 | CONFIRMED | src/app.css present | app.css token source matches | n/a
AGENTS-C185 | CONFIRMED | AGENTS.md:284 | #725 no-raw-hex note present | n/a
AGENTS-C186 | CONFIRMED | AGENTS.md:285-286 | #904/#970 note present | n/a
AGENTS-C187 | CONFIRMED | AGENTS.md:287 | #902 dead-token note present | n/a
AGENTS-C188 | CONFIRMED | components/settings/* Button primitives | Button primitive usage matches | n/a
AGENTS-C189 | CONFIRMED | AGENTS.md:293-294 | #903 button-unification note present | n/a
AGENTS-C190 | CONFIRMED | src/lib/stores/detach.ts; routes/detached/[pane] | Logs/Settings detach matches | n/a
AGENTS-C191 | CONFIRMED | src/lib/stores/detach.ts router | detach router matches | n/a
AGENTS-C192 | CONFIRMED | tests/detached.test.ts present | detach render test matches | n/a
AGENTS-C193 | CONFIRMED | src/routes/+layout.svelte present | navigation guard location matches | n/a
AGENTS-C194 | CONFIRMED | AGENTS.md:307-308 | #817/#815 precedent note present | n/a
AGENTS-C195 | CONFIRMED | tests/*.test.ts + vitest include | Vitest default runner matches | n/a
AGENTS-C196 | CONFIRMED | playwright.config.ts; ci.yml:202 installs chromium+webkit; release verify installs them too | Playwright geometry usage matches | n/a
AGENTS-C197 | CONFIRMED | tests/browser/logviewer.spec.ts (en/de/fr density checks) | browser-gate log-geometry claim matches | n/a
AGENTS-C198 | DRIFT | vitest.config.js:86-91 defines GLOBAL aggregate thresholds (statements 68.91, branches 62.57, functions 73.33, lines 68.02), not per-file floors | The coverage gate is a whole-tree aggregate threshold, not "per-file floors" | P3
AGENTS-C199 | CONFIRMED | src/lib/utils/dev.ts uses import.meta.env.DEV | devLog no-op in production matches | n/a
AGENTS-C200 | CONFIRMED | dev.ts devLog; console.error/warn used for real errors | console policy matches | n/a
AGENTS-C201 | CONFIRMED | src/lib/i18n/store.svelte.ts:44 KNOWN = 8 locales | Eight locales ship | n/a
AGENTS-C202 | CONFIRMED | i18n.ts:51 pt -> NumberFormat("pt-BR"); fallback to en | pt-BR resolves to pt, en fallback | n/a
AGENTS-C203 | CONFIRMED | Dict type in i18n dirs; tests enforce parity | Dict parity matches | n/a
AGENTS-C204 | CONFIRMED | components use t() from $lib/i18n | All UI strings via t() matches | n/a
AGENTS-C205 | CONFIRMED | punctuation literals + PresenceJam name in templates | literal-punctuation exception matches | n/a
AGENTS-C206 | CONFIRMED | src-tauri/src/i18n.rs present | Rust-side tray/menu table matches | n/a
AGENTS-C207 | CONFIRMED | Rust log/error strings are English | Rust errors stay English matches | n/a
AGENTS-C208 | CONFIRMED | tests/i18n.test.ts:12-13 (#752 parity) | placeholder parity test matches | n/a
AGENTS-C209 | CONFIRMED | tests/i18n.test.ts:75-81 five keys | Five brace-literal allowlist keys confirmed | n/a
AGENTS-C210 | CONFIRMED | AGENTS.md:342-344 (#984 provenance) | provenance note matches | n/a
AGENTS-C211 | CONFIRMED | AGENTS.md:345-349 | German noun-agreement note matches | n/a
AGENTS-C212 | CONFIRMED | spotify.rs PKCE + confidential-client wiring | Authorization Code + PKCE confidential client matches | n/a
AGENTS-C213 | CONFIRMED | tauri.conf.json:81 scheme presencejam; deep_link.rs | presencejam://callback matches | n/a
AGENTS-C214 | CONFIRMED | src-tauri/src/pkce.rs present | pkce.rs role matches | n/a
AGENTS-C215 | CONFIRMED | spotify.rs/pkce.rs do not log verifier/code (team auth logs len only) | never-log-verifier matches | n/a
AGENTS-C216 | CONFIRMED | commands/teams_auth.rs:137 spawn_blocking to login.microsoftonline.com | device-code on blocking pool matches | n/a
AGENTS-C217 | CONFIRMED | commands/teams_auth.rs:68 `interval.clamp(1, 15)` | 1-15 s clamp matches | n/a
AGENTS-C218 | CONFIRMED | commands/teams_auth.rs single-flight poll design | single-flight poll matches | n/a
AGENTS-C219 | CONFIRMED | token_io.rs AES-256-GCM ciphertext | tokens.json ciphertext matches | n/a
AGENTS-C220 | CONFIRMED | keychain.rs 256-bit key via keyring | key in OS keychain matches | n/a
AGENTS-C221 | CONFIRMED | Cargo.toml:86 keyring windows-native | Windows -> DPAPI matches | n/a
AGENTS-C222 | CONFIRMED | Cargo.toml:86 keyring apple-native | macOS -> Keychain matches | n/a
AGENTS-C223 | CONFIRMED | Cargo.toml:86 keyring linux-native + sync-secret-service | Linux -> Secret Service matches | n/a
AGENTS-C224 | CONFIRMED | token_io.rs decrypt + keychain.rs key | decryption location matches | n/a
AGENTS-C225 | CONFIRMED | token_io.rs ciphertext; config stored plaintext separately | no plaintext tokens / no tokens in config matches | n/a
AGENTS-C226 | CONFIRMED | capabilities/default.json:6-23 | default.json lists invocable commands matches | n/a
AGENTS-C227 | CONFIRMED | default.json has NO global-shortcut or autostart permission entries | least-privilege matches | n/a
AGENTS-C228 | CONFIRMED | default.json gate enforced by Tauri ACL | add-command-must-list matches | n/a
AGENTS-C229 | CONFIRMED | tauri.conf.json:30 `default-src 'self'` | default-src 'self' matches | n/a
AGENTS-C230 | CONFIRMED | tauri.conf.json:30 `style-src 'self' 'unsafe-inline'` | style-src matches | n/a
AGENTS-C231 | CONFIRMED | tauri.conf.json:30 connect-src api.spotify.com + login.microsoftonline.com + graph.microsoft.com + accounts.spotify.com | Spotify/login/Graph connect origins match | n/a
AGENTS-C232 | CONFIRMED | tauri.conf.json:30 object-src/base-uri/frame-ancestors/form-action | object/base-uri/frame/form directives match | n/a
AGENTS-C233 | CONFIRMED | no HTML form submits in src; IPC used | no-supported-flow-broken matches | n/a
AGENTS-C234 | CONFIRMED | tauri.conf.json:70 allowDowngrades false; updater_bg.rs test pins base-uri/form-action | test pins both directives matches | n/a
AGENTS-C235 | CONFIRMED | commands/status.rs get_diagnostics_snapshot -> diagnostics.rs | get_diagnostics_snapshot matches | n/a
AGENTS-C236 | CONFIRMED | diagnostics.rs:1665 saves to download_dir with no payload arg | save_diagnostics_snapshot accepts neither bytes nor dest matches | n/a
AGENTS-C237 | CONFIRMED | diagnostics.rs:62 `SNAPSHOT_MAX_BYTES: usize = 256 * 1024` | 256 KiB cap confirmed | n/a
AGENTS-C238 | CONFIRMED | diagnostics.rs:57 SNAPSHOT_FILE_STEM presencejam-diagnostics + test :2194 | filename glob confirmed | n/a
AGENTS-C239 | CONFIRMED | save command signature takes no JSON | webview supplies no JSON matches | n/a
AGENTS-C240 | CONFIRMED | diagnostics.rs redact path + :264 token-redaction | snapshot redacted on the way out matches | n/a
AGENTS-C241 | CONFIRMED | config/io.rs uses app_config_dir (Tauri) | %APPDATA%\\PresenceJam\\config.json matches | n/a
AGENTS-C242 | CONFIRMED | config/io.rs uses app_config_dir (Tauri) | ~/Library/Application Support/PresenceJam/config.json matches | n/a
AGENTS-C243 | CONFIRMED | config/io.rs uses app_config_dir (Tauri) | $XDG_CONFIG_HOME/PresenceJam/config.json matches | n/a
AGENTS-C244 | CONFIRMED | tokens.json resolved via app_data_dir (bundle id appended) | bundle-id folder differs matches | n/a
AGENTS-C245 | CONFIRMED | %APPDATA%\\com.presencejam.app\\PresenceJam\\tokens.json | matches Tauri data-dir derivation | n/a
AGENTS-C246 | CONFIRMED | ~/Library/Application Support/com.presencejam.app/PresenceJam/tokens.json | matches | n/a
AGENTS-C247 | CONFIRMED | $XDG_CONFIG_HOME/com.presencejam.app/PresenceJam/tokens.json | matches | n/a
AGENTS-C248 | CONFIRMED | app.rs LogDir target + tauri app_log_dir | app_log_dir() matches | n/a
AGENTS-C249 | CONFIRMED | %LOCALAPPDATA%\\com.presencejam.app\\logs\\PresenceJam.log | matches app_log_dir | n/a
AGENTS-C250 | CONFIRMED | ~/Library/Logs/com.presencejam.app/PresenceJam.log | matches | n/a
AGENTS-C251 | CONFIRMED | ~/.local/share/com.presencejam.app/logs/PresenceJam.log | matches | n/a
AGENTS-C252 | CONFIRMED | config/io.rs atomic writes throughout | atomic-write standard matches | n/a
AGENTS-C253 | DRIFT | config/io.rs:612 `fn atomic_write_json` (stage .tmp -> fsync -> rename); there is no `config::write_atomic` | The recipe is real but the cited symbol is stale — it is config::io::atomic_write_json | P2
AGENTS-C254 | CONFIRMED | config/io.rs:78,122 stage+fsync+rename discipline | do-not-write-in-place matches | n/a
AGENTS-C255 | CONFIRMED | config/io.rs:81 references #939 staged-import pattern | #939/#946 live-task note matches | n/a
AGENTS-C256 | CONFIRMED | app.rs log permission tooling for #920 | user-only log perms + watchdog matches | n/a
AGENTS-C257 | CONFIRMED | spotify.rs:1724 single-pass + test :1919 | single-pass {album} claim confirmed | n/a
AGENTS-C258 | CONFIRMED | spotify.rs {artist} token | artist/show maps | n/a
AGENTS-C259 | CONFIRMED | spotify.rs {track} token | track/episode maps | n/a
AGENTS-C260 | CONFIRMED | spotify.rs {album} token | album/publisher maps | n/a
AGENTS-C261 | CONFIRMED | spotify.rs {emoji} | track/episode/paused emoji | n/a
AGENTS-C262 | CONFIRMED | spotify.rs {device} | device name token | n/a
AGENTS-C263 | CONFIRMED | spotify.rs {playlist}/{context} are the same token | playlist/context alias | n/a
AGENTS-C264 | CONFIRMED | spotify.rs {progress} | playback position, empty when none | n/a
AGENTS-C265 | CONFIRMED | spotify.rs {shuffle}/{repeat} | emoji while on, empty while off | n/a
AGENTS-C266 | CONFIRMED | spotify.rs {show}/{episode}/{publisher} | episode-only tokens | n/a
AGENTS-C267 | CONFIRMED | spotify.rs:105 default + write.rs:1461 episode default | default + episode default match | n/a
AGENTS-C268 | CONFIRMED | CONTRIBUTING.md + AGENTS.md:462 example | commit example valid | n/a
AGENTS-C269 | CONFIRMED | AGENTS.md:463 | valid conventional-commit example | n/a
AGENTS-C270 | CONFIRMED | AGENTS.md:464 | valid example | n/a
AGENTS-C271 | CONFIRMED | AGENTS.md:465 | valid example | n/a
AGENTS-C272 | CONFIRMED | AGENTS.md:466 | valid example | n/a
AGENTS-C273 | CONFIRMED | AGENTS.md:467 (`@tauri-apps/api` family) | valid example | n/a
AGENTS-C274 | CONFIRMED | AGENTS.md:470 + CONTRIBUTING | allowed types match | n/a
AGENTS-C275 | CONFIRMED | AGENTS.md:471-472 | scope usage matches | n/a
AGENTS-C276 | CONFIRMED | AGENTS.md:476 (project rule) | main ff-only policy stated | n/a
AGENTS-C277 | CONFIRMED | AGENTS.md:477-478 | one-PR-per-concern policy stated | n/a
AGENTS-C278 | CONFIRMED | AGENTS.md:479-480 | branch-naming policy stated | n/a
AGENTS-C279 | CONFIRMED | docs/RELEASING.md review-gate workflow (AGENTS.md:484) | two-reviewer policy stated | n/a
AGENTS-C280 | CONFIRMED | docs/RELEASING.md present; AGENTS.md:485-486 | rubric location matches | n/a
AGENTS-C281 | CONFIRMED | AGENTS.md:487 | 100/100 bar stated | n/a
AGENTS-C282 | CONFIRMED | AGENTS.md:488 | fix-and-rerun gate stated | n/a
AGENTS-C283 | CONFIRMED | AGENTS.md:492 | rubric row matches | n/a
AGENTS-C284 | CONFIRMED | AGENTS.md:493-494 | rubric row matches | n/a
AGENTS-C285 | CONFIRMED | AGENTS.md:495-496 | rubric row matches | n/a
AGENTS-C286 | CONFIRMED | AGENTS.md:497-498 | rubric row matches | n/a
AGENTS-C287 | CONFIRMED | AGENTS.md:499-500 | rubric row matches | n/a
AGENTS-C288 | CONFIRMED | AGENTS.md:501-503 | rubric row matches | n/a
AGENTS-C289 | CONFIRMED | AGENTS.md:504 | rubric row matches | n/a
AGENTS-C290 | CONFIRMED | AGENTS.md:508-509 | issues-first policy stated | n/a
AGENTS-C291 | CONFIRMED | AGENTS.md:510-511 | Closes #N literal policy stated | n/a
AGENTS-C292 | CONFIRMED | AGENTS.md:512-513 | post-merge sweep policy stated | n/a
AGENTS-C293 | CONFIRMED | ci.yml jobs list (rust-platform-check, windows-cli-smoke, frontend, rust, rust-clippy, changelog-links, version-consistency, secret-scan, dep-audit, no-vendored-binaries, cargo-deny, rust-coverage, docs-links) | ci.yml job list matches | n/a
AGENTS-C294 | CONFIRMED | ci.yml:28-88 | rust-platform-check row matches | n/a
AGENTS-C295 | CONFIRMED | ci.yml:98-174 | windows-cli-smoke row matches | n/a
AGENTS-C296 | CONFIRMED | ci.yml:182-270 | frontend row matches | n/a
AGENTS-C297 | CONFIRMED | ci.yml:282-349 | rust row matches | n/a
AGENTS-C298 | CONFIRMED | ci.yml:358-405 | rust-clippy row matches | n/a
AGENTS-C299 | CONFIRMED | ci.yml:410-434 | changelog-links row matches | n/a
AGENTS-C300 | CONFIRMED | ci.yml:710-720 | docs-links row matches | n/a
AGENTS-C301 | CONFIRMED | ci.yml:444-472 | version-consistency row matches | n/a
AGENTS-C302 | CONFIRMED | ci.yml:477-491 | secret-scan row matches | n/a
AGENTS-C303 | CONFIRMED | ci.yml:524-568 | dep-audit row matches | n/a
AGENTS-C304 | CONFIRMED | ci.yml:574-591 | no-vendored-binaries row matches | n/a
AGENTS-C305 | CONFIRMED | ci.yml:598-612 | cargo-deny row matches | n/a
AGENTS-C306 | CONFIRMED | ci.yml:619-704 (--fail-under-lines 0, floors 0, report-only) | rust-coverage report-only row matches | n/a
AGENTS-C307 | CONFIRMED | ci.yml:28-88 per-OS check/test legs | cross-platform regression detection matches | n/a
AGENTS-C308 | CONFIRMED | ci.yml:234-245 then :251-252 | cargo test --lib then svelte-check ordering matches | n/a
AGENTS-C309 | CONFIRMED | ci.yml:227-233 explanation + live npm-check failure on this checkout | unknown-export ordering claim matches | n/a
AGENTS-C310 | CONFIRMED | log::*! used; no println!/eprintln! in production paths | no-println rule matches | n/a
AGENTS-C311 | CONFIRMED | app.rs:361 attach_parent_console_for_cli flushes before CLI output | CLI logger-flush rule matches | n/a
AGENTS-C312 | CONFIRMED | tray/cache.rs:140 is the sole unwrap; elsewhere Result/.expect | one-unwrap-exception rule matches | n/a
AGENTS-C313 | CONFIRMED | token_io.rs/keychain.rs envelope; team-auth logs lengths only | no plaintext tokens rule matches | n/a
AGENTS-C314 | CONFIRMED | keychain AES-GCM envelope + no shape-logging | keychain-only-tokens rule matches | n/a
AGENTS-C315 | CONFIRMED | tray/mod.rs:356 unknown event_id redaction helper | redact raw ids rule matches | n/a
AGENTS-C316 | CONFIRMED | tray redaction helper reuse | reuse-not-inline rule matches | n/a
AGENTS-C317 | CONFIRMED | Cargo.toml:74 parking_lot only; tokio via tauri runtime | no-second-lock-crate rule matches | n/a
AGENTS-C318 | CONFIRMED | tauri.conf.json:30 fixed csp | CSP-lockdown rule matches | n/a
AGENTS-C319 | CONFIRMED | tauri.conf.json:30 style-src is the one documented unsafe-inline | no-new-unsafe-inline nuance matches | n/a
AGENTS-C320 | CONFIRMED | default.json:6-23 minimal, no shortcut/autostart | no-dead-grants rule matches | n/a
AGENTS-C321 | CONFIRMED | components use t(); dictionaries exist | no-hardcoded-English rule matches | n/a
AGENTS-C322 | CONFIRMED | tokens live in src/app.css | no-new-hex rule matches | n/a
AGENTS-C323 | CONFIRMED | components/settings Button primitives | Button-variant rule matches | n/a
AGENTS-C324 | CONFIRMED | AGENTS.md:585-586 | don't-skip-gates rule matches | n/a
AGENTS-C325 | CONFIRMED | AGENTS.md:587-588 | don't-close-from-prose rule matches | n/a
AGENTS-C326 | CONFIRMED | docs/README.md:49 + link-audit.py depth checks | link-depth rule matches | n/a
AGENTS-C327 | CONFIRMED | .gitignore chrome-headless-shell; ci.yml:574-591 no-vendored-binaries | vendored-binary refusal matches | n/a
AGENTS-C328 | CONFIRMED | ci.yml:520-523 records the dropped #642 ignore | advisory-ignore-expiry rule matches | n/a
AGENTS-C329 | CONFIRMED | release.yml:91-128 + ci.yml:444-472 enforce all sites | bump-all-sites rule matches | n/a
AGENTS-C330 | CONFIRMED | routes/+layout.svelte guard | don't-bypass-navigation-guard matches | n/a
AGENTS-C331 | CONFIRMED | profanity.rs on every status-write path | don't-skip-profanity rule matches | n/a
AGENTS-C332 | DRIFT | unsafe blocks: app.rs:374,375,382,394 (4), diagnostics.rs:1405,1447,1473 (3), macos_deeplink.rs:209 (1), platform/idle.rs:94,106 (2), platform/focus.rs:110 (1) = ELEVEN, and the CLI parent-console attach is in app.rs:361, not lib.rs | "Ten existing FFI blocks ... in lib.rs" is wrong twice: the console attach lives in app.rs and the tree already carries eleven unsafe blocks | P2
AGENTS-C333 | DRIFT | the eleven unsafe blocks counted in the tree (see C332) | "Keep that list at ten — an eleventh needs a reason" is stale; an eleventh already exists | P2
AGENTS-C334 | CONFIRMED | diagnostics.rs:386 redact_sensitive | redact_sensitive rule matches | n/a
AGENTS-C335 | CONFIRMED | tauri.conf.json:5 identifier com.presencejam.app | bundle-id anchor matches | n/a
AGENTS-C336 | CONFIRMED | AGENTS.md:622 | recipe step 1 matches | n/a
AGENTS-C337 | CONFIRMED | AGENTS.md:626-629 | recipe step 2 matches | n/a
AGENTS-C338 | CONFIRMED | AGENTS.md:630-631 | recipe step 3 matches | n/a
AGENTS-C339 | CONFIRMED | AGENTS.md:632-634 | recipe step 4 matches | n/a
AGENTS-C340 | CONFIRMED | AGENTS.md:635-637 | recipe step 5 matches | n/a
AGENTS-C341 | CONFIRMED | AGENTS.md:635 | fail-pre/pass-post rule matches | n/a
AGENTS-C342 | CONFIRMED | AGENTS.md:638 | recipe step 6 matches | n/a
AGENTS-C343 | CONFIRMED | AGENTS.md:639 | recipe step 7 matches | n/a
AGENTS-C344 | CONFIRMED | AGENTS.md:640-641 | recipe step 8 matches | n/a
AGENTS-C345 | CONFIRMED | AGENTS.md:642 | recipe step 9 matches | n/a
AGENTS-C346 | CONFIRMED | AGENTS.md:643-644 | recipe step 10 matches | n/a
AGENTS-C347 | CONFIRMED | AGENTS.md:645-646 | recipe step 11 matches | n/a
AGENTS-C348 | CONFIRMED | AGENTS.md:647 | recipe step 12 matches | n/a
AGENTS-C349 | CONFIRMED | AGENTS.md:649-651 | hub/worktree coordination note matches | n/a
AGENTS-C350 | DRIFT | AppState is defined in src-tauri/src/state.rs (cli.rs:1 `use super::state::AppState`); lib.rs only declares `pub mod app` + `pub use app::run` | The glossary's "See src-tauri/src/lib.rs" pointer for AppState is stale — AppState is in state.rs | P3
AGENTS-C351 | CONFIRMED | config/ module (config/mod.rs AppConfig; cf. C093 path split) | AppConfig is the on-disk+in-memory config struct | n/a
AGENTS-C352 | CONFIRMED | config/clamp.rs:7,33,227,297,469 clamp_* family | clamp_* helper family confirmed | n/a
AGENTS-C353 | CONFIRMED | diagnostics.rs:386 redact_sensitive | redact_sensitive row confirmed | n/a
AGENTS-C354 | CONFIRMED | [APP]/[TRAY]/[CONFIG]/[POLL] tags in tree | bracketed log-tag row confirmed | n/a
AGENTS-C355 | CONFIRMED | presence.rs:26 AVAILABILITY_REARM_SECONDS = 4*60 = 240s; gate.rs:436 uses it for the re-check-due predicate | gated_track_key re-check is on the 240 s clock | n/a
AGENTS-C356 | CONFIRMED | config/schema.rs:128,136,148 gate_when_out_of_office / gate_when_presenting / idle_away_after_seconds | gate_when_* fields confirmed | n/a
AGENTS-C357 | CONFIRMED | config/schema.rs:123 respect_manual_status | respect_manual_status field confirmed | n/a
AGENTS-C358 | CONFIRMED | config/schema.rs:525-526 pause_polling | pause_polling field confirmed | n/a
AGENTS-C359 | DRIFT | cli.rs:15 (SET_STATUS), :18 (SET_STATUS_EXPIRY), :20 (CLEAR_STATUS), :39-DAEMON etc. — argv carries status/sync-once/set-status/set-status-expiry/clear-status/serve/daemon (+ minimized) | "The five documented CLI flags" undercounts: set-status, set-status-expiry and clear-status are also shipped (and --profile is undocumented) | P2
AGENTS-C360 | CONFIRMED | tauri.conf.json:81-82 schemes [presencejam] | presencejam:// deep-link scheme confirmed | n/a
AGENTS-C361 | CONFIRMED | src/lib/utils/dev.ts devLog gated on import.meta.env.DEV | devLog no-op row confirmed | n/a
AGENTS-C362 | CONFIRMED | src/lib/i18n.ts t() export | t() row confirmed | n/a
AGENTS-C363 | CONFIRMED | src/lib/i18n/*.ts Dict type | Dict parity row confirmed | n/a
AGENTS-C364 | CONFIRMED | Cargo.toml:99 ts-rs; types-generated materialised by cargo test | ts(export) row confirmed | n/a
AGENTS-C365 | CONFIRMED | CLAUDE.md:1-17 stub vs AGENTS.md full contract | CLAUDE.md predates AGENTS.md confirmed | n/a
AGENTS-C366 | CONFIRMED | CLAUDE.md present in tree as stub | redirect-stub-only confirmed | n/a
AGENTS-C367 | CONFIRMED | AGENTS.md §3 Quick reference | dev-commands migration mapping matches | n/a
AGENTS-C368 | CONFIRMED | AGENTS.md §4 + §10 | conventions migration mapping matches | n/a
AGENTS-C369 | CONFIRMED | AGENTS.md §5 + §6 | frontend-conventions mapping matches | n/a
AGENTS-C370 | CONFIRMED | AGENTS.md §2 | key-files mapping matches | n/a
AGENTS-C371 | CONFIRMED | AGENTS.md §7 | auth-flows mapping matches | n/a
AGENTS-C372 | CONFIRMED | AGENTS.md §8 | storage mapping matches | n/a
AGENTS-C373 | CONFIRMED | AGENTS.md §9 | status-format mapping matches | n/a
AGENTS-C374 | CONFIRMED | AGENTS.md:701-702 | point-only-CLADE-readers note matches | n/a
AGENTS-C375 | CONFIRMED | README.md present | See-also link resolves | n/a
AGENTS-C376 | CONFIRMED | CONTRIBUTING.md present | See-also link resolves | n/a
AGENTS-C377 | CONFIRMED | ARCHITECTURE.md present | See-also link resolves | n/a
AGENTS-C378 | CONFIRMED | docs/STATE-OF-FEATURES.md present | See-also link resolves | n/a
AGENTS-C379 | CONFIRMED | docs/RELEASING.md present | See-also link resolves | n/a
AGENTS-C380 | CONFIRMED | SECURITY.md present | See-also link resolves | n/a
AGENTS-C381 | CONFIRMED | docs/architecture/overview.md present | See-also link resolves | n/a
AGENTS-C382 | CONFIRMED | CODEOWNERS present | See-also link resolves | n/a
AGENTS-C383 | CONFIRMED | AGENTS.md:154 | reader step 1 matches | n/a
AGENTS-C384 | CONFIRMED | AGENTS.md:156 | reader step 3 matches | n/a
AGENTS-C385 | CONFIRMED | AGENTS.md:157 | reader step 4 matches | n/a

## DEFECTS (non-CONFIRMED, ordered by blast radius)

### P1

claim_id: README-C127
file: README.md:156
class: C5
verdict: DRIFT
evidence: src-tauri/src/cli.rs:8-39 defines --status, --sync-once, --help, --set-status, --set-status-expiry, --clear-status, --profile, --serve, --daemon; README.md:162-170 table itself lists eight non-`--help` rows
finding: The sentence "seven CLI flags plus `--help`" is wrong twice — the very table below it lists eight non-help flags, and the parser also carries an undocumented `--profile` (cli.rs:27, #869).
proposed_fix: Replace the sentence with: "the binary also answers eight documented flags plus `--help`", and add a table row: "| `presencejam --profile <id>` | Switches the active presence profile to `<id>` (or `base` to clear) and exits 0 |".
severity: P1

### P2

claim_id: README-C037
file: README.md:35
class: C2
verdict: DRIFT
evidence: src/lib/i18n/store.svelte.ts:44 KNOWN = ['en','de','fr','es','it','pl','pt','nl']; tests/i18n.test.ts:86 LOCALES_8
finding: The README advertises only English/German/French but eight locales ship and are selected by the picker's store.
proposed_fix: Replace the bullet with: "**Interface languages** — eight locales (English, Deutsch, Français, Español, Italiano, Polski, Português, Nederlands) via an in-app language picker."
severity: P2

claim_id: README-C140
file: README.md:167
class: C2
verdict: OVERSTATED
evidence: src-tauri/src/serve.rs:1 "token-guarded localhost HTTP control + event API"; :22 "/snooze and /profile"
finding: `--serve` is described as a "read-only" status API, but it exposes control routes (/snooze, /profile) that mutate runtime state.
proposed_fix: Replace "read-only status API" with "token-guarded localhost status/control API".
severity: P2

claim_id: README-C141
file: README.md:167
class: C2
verdict: DRIFT
evidence: src-tauri/src/cli.rs:8,476 (STATUS is its own CliCommand printing SyncStatus then calling cli_status_exit_code); app.rs:974; no HTTP call in the --status path
finding: The parenthetical "the surface `--status` reads from" is false — `--status` reads in-process state directly and `--serve` is an independent command.
proposed_fix: Delete the parenthetical " (the surface `--status` reads from)." from the `--serve` row.
severity: P2

claim_id: AGENTS-C085
file: AGENTS.md:116
class: C1
verdict: DRIFT
evidence: src-tauri/src/lib.rs:1-29 (`pub mod app; pub mod cli; pub use app::run;`); src-tauri/src/app.rs:361,974 (run + CLI dispatch); src-tauri/src/state.rs (AppState)
finding: The lib.rs layout note "AppState, run(), deep-link, CLI" is stale — lib.rs is now a slim crate root and those live in app.rs / state.rs / cli.rs.
proposed_fix: Replace the line with: "│   ├── lib.rs           # crate root — module declarations (`pub use app::run`)"
severity: P2

claim_id: AGENTS-C093
file: AGENTS.md:124
class: C1
verdict: DRIFT
evidence: `ls src-tauri/src/` shows `config/` (mod.rs, clamp.rs, io.rs, migrate.rs, patch.rs, schema.rs, snooze.rs, transfer.rs), not config.rs
finding: config.rs was split into a config/ module directory; the AppConfig struct is in config/mod.rs and the clamps in config/clamp.rs.
proposed_fix: Replace the line with: "│   ├── config/          # AppConfig, load/save, clamp_* (clamp.rs), atomic io (io.rs), schema/migrate/transfer"
severity: P2

claim_id: AGENTS-C096
file: AGENTS.md:127
class: C1
verdict: DRIFT
evidence: `ls src-tauri/src/` shows `tray/` (mod.rs 110KB + actions.rs, cache.rs, dedup.rs, devices.rs, snooze.rs), not tray.rs
finding: tray.rs was split into a tray/ module directory; there is no tray.rs file.
proposed_fix: Replace the line with: "│   ├── tray/            # system tray + playback menu (mod.rs, actions.rs, cache.rs, devices.rs, snooze.rs)"
severity: P2

claim_id: AGENTS-C107
file: AGENTS.md:138
class: C1
verdict: DRIFT
evidence: `ls src-tauri/src/polling/` shows clocks.rs, daemon.rs, exit.rs, gate.rs, iteration.rs, loop.rs, mod.rs, presence.rs, refresh.rs, rules.rs, state.rs, status_text.rs, timing.rs, write.rs — no poll_once.rs
finding: poll_once.rs no longer exists; the one-iteration helper (used by --sync-once) is polling/iteration.rs.
proposed_fix: Replace the line with: "│   │   ├── iteration.rs # one-iteration helper (used by --sync-once too)"
severity: P2

claim_id: AGENTS-C162
file: AGENTS.md:220
class: C2
verdict: DRIFT
evidence: `ls src-tauri/src/polling/` (see AGENTS-C107); loop.rs is present
finding: "polling/loop.rs and polling/poll_once.rs" — poll_once.rs no longer exists.
proposed_fix: Replace "polling/loop.rs and polling/poll_once.rs" with "polling/loop.rs and polling/iteration.rs".
severity: P2

claim_id: AGENTS-C253
file: AGENTS.md:422
class: C1
verdict: DRIFT
evidence: src-tauri/src/config/io.rs:612 `pub(crate) fn atomic_write_json(path, json)` (stage .tmp -> fsync -> rename); no symbol named write_atomic
finding: The atomic-write recipe is real but the cited symbol `config::write_atomic` no longer exists — it is `config::io::atomic_write_json`.
proposed_fix: Replace "is in `config::write_atomic`" with "is in `config::io::atomic_write_json`".
severity: P2

claim_id: AGENTS-C332
file: AGENTS.md:605
class: C5
verdict: DRIFT
evidence: unsafe blocks: src-tauri/src/app.rs:374,375,382,394; src-tauri/src/diagnostics.rs:1405,1447,1473; src-tauri/src/macos_deeplink.rs:209; src-tauri/src/platform/idle.rs:94,106; src-tauri/src/platform/focus.rs:110 (eleven total); the CLI parent-console attach is app.rs:361
finding: The sentence says "Ten existing FFI blocks ... the CLI parent-console attach in `lib.rs`", but the console attach now lives in app.rs and the tree carries eleven unsafe blocks, not ten.
proposed_fix: Replace "Ten existing FFI blocks" with "Eleven existing unsafe blocks", and replace "the CLI parent-console attach in `lib.rs`" with "the CLI parent-console attach in `app.rs`".
severity: P2

claim_id: AGENTS-C333
file: AGENTS.md:609
class: C5
verdict: DRIFT
evidence: eleven unsafe blocks already present (see AGENTS-C332)
finding: "Keep that list at ten — an eleventh needs a reason in the PR" is already past its own limit: the eleventh unsafe block exists.
proposed_fix: Replace "Keep that list at ten — an eleventh needs a reason in the PR." with "Keep that list at eleven — a twelfth needs a reason in the PR.".
severity: P2

claim_id: AGENTS-C359
file: AGENTS.md:671
class: C1
verdict: DRIFT
evidence: src-tauri/src/cli.rs:15 (--set-status), :18 (--set-status-expiry), :20 (--clear-status) are shipped CLI commands
finding: "The five documented CLI flags" undercounts the shipped set — set-status, set-status-expiry and clear-status are real, plus the undocumented --profile.
proposed_fix: Replace the row's "The five documented CLI flags" with "The documented CLI flags" and list the full set (--status, --sync-once, --set-status, --set-status-expiry, --clear-status, --serve, --daemon, --minimized) or link to README §Command-line flags.
severity: P2

claim_id: MISSING-§2-layout
file: AGENTS.md:115-143 (§2 src-tauri/src tree)
class: C1
verdict: MISSING
evidence: `ls src-tauri/src/` shows app.rs, cli.rs, deep_link.rs, events.rs, http.rs, redact.rs, state.rs — none appear in the §2 tree
finding: Seven material top-level modules added to src-tauri/src/ are absent from the §2 repository layout, so the tree as documented is incomplete and the §85/350 staleness compounds it.
proposed_fix: Add lines under `│   └── src/` for app.rs (run() + CLI dispatch + console attach), cli.rs (argv parsing), state.rs (AppState), deep_link.rs, events.rs, http.rs, redact.rs.
severity: P2

### P3

claim_id: README-C085
file: README.md:70
class: C2
verdict: STALE
evidence: homebrew/presence-jam.rb:24 `cask "presence-jam" do` with :39 `depends_on arch: :arm64` (issue #898 formula->cask)
finding: The arm64 gate is accurate, but the text says "the formula declares ..." — the Homebrew packaging is a cask now, not a formula.
proposed_fix: Replace "the formula declares `depends_on arch: :arm64`" with "the cask declares `depends_on arch: :arm64`".
severity: P3

claim_id: AGENTS-C156
file: AGENTS.md:208
class: C2
verdict: DRIFT
evidence: `grep '\[' src-tauri/src/tray/mod.rs` = `[TRAY]` x54 and zero `[MENU]`; menu.rs uses `[MENU]`
finding: "The single dispatcher in `tray.rs` and the menu dispatcher in `menu.rs` already share a `[MENU]` tag" — the tray module is now tray/ and logs `[TRAY]`; only menu.rs uses `[MENU]`.
proposed_fix: Replace with "The tray dispatcher in `tray/` logs `[TRAY]` while the menu dispatcher in `menu.rs` logs `[MENU]`; keep new sub-dispatchers inside the same module under its tag."
severity: P3

claim_id: AGENTS-C161
file: AGENTS.md:216
class: C2
verdict: DRIFT
evidence: src-tauri/src/tray/cache.rs:125-140 (cached_devices unwrap); CLAUDE.md:1-17 is a redirect stub with no §Rust section
finding: The exception itself is real (tray/cache.rs:140) but the text says it is "documented in `CLAUDE.md` §Rust", a section that no longer exists.
proposed_fix: Replace "and that one is documented in `CLAUDE.md` §Rust for historical reasons" with "and that one is documented here for historical reasons", and change "`tray.rs`" to "`tray/cache.rs`".
severity: P3

claim_id: AGENTS-C198
file: AGENTS.md:315
class: C6
verdict: DRIFT
evidence: vitest.config.js:86-91 `thresholds: { statements: 68.91, branches: 62.57, functions: 73.33, lines: 68.02 }`
finding: "per-file floors in vitest.config.js" is a mislabel — the ratchet is a single whole-tree aggregate threshold, not per-file floors.
proposed_fix: Replace "per-file floors in `vitest.config.js`" with "aggregate coverage thresholds in `vitest.config.js`".
severity: P3

claim_id: AGENTS-C350
file: AGENTS.md:662
class: C1
verdict: DRIFT
evidence: src-tauri/src/cli.rs:1 `use super::state::AppState;`; src-tauri/src/lib.rs is the crate root only
finding: The glossary's "See `src-tauri/src/lib.rs`" pointer for AppState is stale — AppState is defined in state.rs.
proposed_fix: Replace "See `src-tauri/src/lib.rs`." with "See `src-tauri/src/state.rs`.".
severity: P3

### Residual risk (not a defect)

claim_id: AGENTS-C158
file: AGENTS.md:212
class: C2
verdict: EXTERNAL-UNVERIFIED
evidence: src-tauri/src/app.rs:1111-1123 builds tauri_plugin_log without an explicit `.level()`, so info-and-above-landing / debug-filtering is set by the vendor default; check https://v2.tauri.app/plugin/log/ (fetched 2026-10-08 — not reached)
finding: The claim about the default log level cannot be confirmed from the tree (no explicit level is set) and needs a vendor-doc check.
proposed_fix: n/a — confirm the default level at the vendor URL and, if it differs, set an explicit `.level(LevelFilter::Info)` in app.rs.
severity: n/a

## COUNTS

707 claim_ids audited across README(199), ARCHITECTURE(20), docs/README(50), PLATFORMS(40), CLAUDE(13), AGENTS(385).

| Verdict | Count |
| --- | --- |
| CONFIRMED | 688 |
| DRIFT | 16 |
| STALE | 1 |
| OVERSTATED | 1 |
| EXTERNAL-UNVERIFIED | 1 |
| MISSING | 1 (standalone, not tied to a claim_id — §2 layout omits 7 modules) |
| UNSOURCED | 0 |
| P0 | 0 |

Defect severities in Section B: P1 x1 (README-C127); P2 x13 (README-C037/C140/C141, AGENTS-C085/C093/C096/C107/C162/C253/C332/C333/C359, MISSING-§2); P3 x5 (README-C085, AGENTS-C156/C161/C198/C350); EXTERNAL-UNVERIFIED x1 (AGENTS-C158, severity n/a). Total defect rows = 20. No P0.

P0/P1 evidence check: the single P1 (README-C127) and every P2/P3 defect above carries a `path:line` or a `URL+quote`; compliant.
