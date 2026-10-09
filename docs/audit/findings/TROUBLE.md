# Findings — TROUBLESHOOTING.md

Audit of `docs/audit/claims/TROUBLE.md` (214 claims) against the repository tree.

Repo state verified independently: HEAD `09341ecaad732e78454a2c65b383f0dfd1541d5a` (main, "test(sync,scans): reorder-mutation proof + Why comments on every surviving scan (#778) (#1170)"), `git status --porcelain` clean apart from `docs/__pycache__/` and `docs/audit/`, versions 5.0.0 across `src-tauri/Cargo.toml`, `package.json`, `src-tauri/tauri.conf.json`. Vendor URLs quoted below were fetched 2026-10-08.

Evidence hierarchy applied: repository tree on disk (only authority for paths, defaults, clamps, log tags, log levels, commands, CLI flags) > allowlisted vendor docs > CHANGELOG as a claim only > tests > prose.

---

## VERDICTS

TROUBLE-C001 | CONFIRMED | TROUBLESHOOTING.md:9; app.rs:1236 `should_hide_on_close` | Closing the window hides to tray rather than exiting, and right-click tray → Quit exits. | P3
TROUBLE-C002 | CONFIRMED | config/io.rs:19-49 `config_dir()` = `BaseDirs::config_dir()/PresenceJam` | Config folder is `%APPDATA%\PresenceJam\`, `~/Library/Application Support/PresenceJam/`, `$XDG_CONFIG_HOME/PresenceJam/` on the three OSes. | P1
TROUBLE-C003 | CONFIRMED | spotify.rs:667,1289; teams.rs:292,790 | All Spotify/Teams traffic is real HTTPS, so a corporate proxy blocking it breaks sync. | P3
TROUBLE-C004 | CONFIRMED | spotify.rs:552-553 `NotPremium`; teams.rs:790 Graph presence | Playback surface requires Premium and Teams status requires a Teams account; the sentence is accurate. | P3
TROUBLE-C005 | CONFIRMED | release.yml:420-421 "It is unsigned here on purpose"; CHANGELOG.md:1156 | The macOS build is unsigned; Apple Developer Program enrolment is out of scope. | P3
TROUBLE-C006 | CONFIRMED | developer.apple.com "Safely open apps on your Mac": "If you're certain that the app you want to open is from a trustworthy source... you might be able to temporarily override your Mac security settings to open it" | An unsigned build triggers a Gatekeeper alert on first launch. | P3
TROUBLE-C007 | CONFIRMED | developer.apple.com, same page, steps 1-3: "Open System Settings. Click Privacy & Security, scroll down, and click the Open Anyway button" | right-click → Open and System Settings → Privacy & Security → Open Anyway is the documented workaround. | P3
TROUBLE-C008 | CONFIRMED | developer.apple.com: "The app is now saved as an exception to your security settings, and you can open it in the future by double-clicking it" | Subsequent opens carry no prompt once the exception is granted. | P3
TROUBLE-C009 | CONFIRMED | release.yml:420-421 (unsigned on purpose; only the updater .sig is signed) | An update ships an equally unsigned `.app`, so Gatekeeper re-prompts. | P3
TROUBLE-C010 | CONFIRMED | app.rs:37-45 `should_hide_on_close`; app.rs:1218-1244 | On a tray-less Linux session the window closes for real instead of hiding, so the app looks like nothing started. | P2
TROUBLE-C011 | CONFIRMED | GNOME user docs plus app.rs:1218 comment "GNOME without the AppIndicator extension" | GNOME needs the AppIndicator extension to show a tray icon. | P3
TROUBLE-C012 | CONFIRMED | release.yml/comments and Linux packaging; libayatana-appindicator3 is the runtime tray dependency on GTK | The AppIndicator library must be installed for the tray to render. | P2
TROUBLE-C013 | CONFIRMED | app.rs:38-45: "With no tray ... the window must close for real" combined with the tray-not-shown cause | A Wayland session with no extension shows neither window nor tray. | P2
TROUBLE-C014 | CONFIRMED | app.rs:64-68 `notify_no_tray_close` names exactly "Install libayatana-appindicator3 and the GNOME AppIndicator extension" | Fix text matches the in-app notification the app itself emits. | P2
TROUBLE-C015 | CONFIRMED | app.rs:1069-1079 single-instance plugin; app.rs:314-330 `forward_launch_to_running_instance` | A second launch raises the running instance rather than starting a new one. | P2
TROUBLE-C016 | CONFIRMED | app.rs:884-887 "Failed to setup system tray" — the tray error is recorded on AppState, not fatal | Missing tray library makes tray setup fail rather than silently degrade. | P2
TROUBLE-C017 | CONFIRMED | app.rs:1218 "Issue #819 (extends #927)" | Issue #927 is cited in-tree as the graceful-tray-failure task. | P3
TROUBLE-C018 | CONFIRMED | SETUP.md:219 "## Linux: System Keyring Required" | The cross-doc anchor exists and covers the other Linux prerequisite. | P3
TROUBLE-C019 | CONFIRMED | README/SETUP.md:80 "### Configure redirect URIs"; stores/config.ts:44 `redirect_uri: 'presencejam://callback'` | A misconfigured Redirect URI is the documented cause of a failed Spotify connect. | P3
TROUBLE-C020 | CONFIRMED | SETUP.md:16 links the Spotify Developer Dashboard | The dashboard URL is the correct destination for app settings. | P3
TROUBLE-C021 | CONFIRMED | stores/config.ts:44; tauri.conf.json:97-101 `"schemes": ["presencejam"]` | Adding `presencejam://callback` is exactly the redirect URI the app registers. | P2
TROUBLE-C022 | CONFIRMED | lib.rs deep-link registration at startup | Restarting re-registers the scheme handler and retries cleanly. | P3
TROUBLE-C023 | CONFIRMED | commands/spotify_auth.rs:361-364 `tauri_plugin_opener::open_url` — a browser handoff the OS can refuse | Popup blockers / browser preference cause the browser not to open. | P2
TROUBLE-C024 | CONFIRMED | commands/spotify_auth.rs:361-364 | Retrying Connect Spotify hands the authorization URL to the default browser. | P2
TROUBLE-C025 | CONFIRMED | commands/spotify_auth.rs:17 `const CMD: &str = "[CMD.SPOTIFY_AUTH]";` and :362-365 `log::warn!("{CMD} run_spotify_oauth_flow: Failed to open browser: {}", e)` | A failed browser handoff logs a WARN under the `[CMD.SPOTIFY_AUTH]` tag reading `Failed to open browser`. | P1
TROUBLE-C026 | CONFIRMED | TROUBLESHOOTING.md:50 heading "### Paste URL but nothing happens" | The linked anchor exists in this document. | P3
TROUBLE-C027 | CONFIRMED | tauri.conf.json:97-101; stores/config.ts:44; commands/spotify_auth.rs manual-URL path | The browser redirects to `presencejam://callback?code=XXX...`, which the app can also paste manually. | P3
TROUBLE-C028 | CONFIRMED | spotify.rs:169 403→NotPremium; polling/iteration.rs ExpiredToken/InvalidGrant classification | Expired or changed credentials are a real cause of refresh failure. | P2
TROUBLE-C029 | CONFIRMED | SpotifyCard.svelte:131-134 Reconnect button; Onboarding.svelte manual URL | Settings → Reconnect next to Spotify re-runs the browser sign-in. | P2
TROUBLE-C030 | CONFIRMED | spotify.rs:600-612 `InvalidGrant`; polling/iteration.rs:426 `ERROR_RETRY_INTERVAL_SECONDS` retry; timing.rs:304-309 `with_jitter` | Only invalid-grant/revocation needs a reconnect; transient errors retry automatically. | P2
TROUBLE-C031 | CONFIRMED | spotify.rs:110-111,604-607 `NoActiveDevice` = 404 `NO_ACTIVE_DEVICE` | Player commands act on the active device and 404 when none exists. | P2
TROUBLE-C032 | CONFIRMED | tray/mod.rs:715-731 `TrayClickTarget::Device` → `player_transfer(token, &device_id, true)` | Picking a tray device transfers playback there and starts it (`play: true`). | P2
TROUBLE-C033 | CONFIRMED | spotify.rs:604-607 | Starting playback on a device first is the documented alternative. | P3
TROUBLE-C034 | CONFIRMED | developer.spotify.com/documentation/web-api/reference/start-a-users-playback: "This API only works for users who have Spotify Premium" + "Authorization scopes: user-modify-playback-state"; spotify.rs:169 | Playback control is Premium-only and scope-gated; a 403 maps to `NotPremium`. | P2
TROUBLE-C035 | CONFIRMED | commands/spotify_auth.rs:150 scope string includes `user-modify-playback-state`; spotify.rs:1517 | One reconnect re-requests the scope so the new token carries it. | P2
TROUBLE-C036 | CONFIRMED | TROUBLESHOOTING.md:89 heading "### \"Playback control needs a one-time reconnect\" banner" | The referenced "next entry" is the banner section immediately below. | P3
TROUBLE-C037 | CONFIRMED | developer.spotify.com/documentation/web-api/reference/set-repeat-mode-on-users-playback: "This API only works for users who have Spotify Premium" + scope `user-modify-playback-state`; i18n.rs:150-152 | Shuffle/Repeat are real player endpoints with identical Premium + scope requirements. | P2
TROUBLE-C038 | CONFIRMED | developer.spotify.com/documentation/web-api/reference/toggle-shuffle-for-users-playback and set-repeat-mode page, same Premium-only sentences | Playback control in general is a Premium-only Web API surface. | P3
TROUBLE-C039 | CONFIRMED | spotify.rs:605-608 `NotPremium => "Playback control requires Spotify Premium"` | The toast string matches the Rust error text exactly. | P1
TROUBLE-C040 | CONFIRMED | tray/actions.rs:119 and commands/playback.rs:215 `"No active playback device - pick one from the tray Devices menu"`; i18n.rs:1084 | The toast string is emitted verbatim. | P1
TROUBLE-C041 | CONFIRMED | commands/playback.rs:242-250 scope decode + banner; Settings.svelte:256-263 `playbackScopeMissing` | With no toast, the missing `user-modify-playback-state` scope surfaces the one-time reconnect banner. | P2
TROUBLE-C042 | CONFIRMED | TROUBLESHOOTING.md:89 | The referenced banner section exists above. | P3
TROUBLE-C043 | CONFIRMED | i18n.rs:150-152 `"Repeat: Off" | "Repeat: Context" | "Repeat: Track"`; tray/cache.rs:276-281 | The Repeat item spells out its three modes in that order. | P2
TROUBLE-C044 | CONFIRMED | tray/actions.rs:44-52 `note_playback_modes` only on Ok; tray/dedup.rs:195,243 | A refused command leaves the label and check mark unchanged. | P2
TROUBLE-C045 | CONFIRMED | commands/spotify_auth.rs:150 scope string; spotify.rs:1517 "Scope user-modify-playback-state, already requested" | Older tokens lack the scope and cannot control playback until re-auth. | P3
TROUBLE-C046 | CONFIRMED | SpotifyCard.svelte:178-182 banner with Reconnect button | The banner's Reconnect button (and Settings) performs the one-time reconnect. | P2
TROUBLE-C047 | CONFIRMED | config/migrate.rs:167-198 `LEGACY_SECRET_SIDECAR_NAME`, :262 "keychain holds a different secret; leaving config.json untouched (user should Reconnect)" | A legacy plaintext `spotify.client_secret` disagreeing with the keychain is a real tracked state (#376/#803). | P3
TROUBLE-C048 | CONFIRMED | config/migrate.rs:269-292 | The plaintext is deliberately preserved and the user is asked to resolve it. | P2
TROUBLE-C049 | CONFIRMED | config/migrate.rs:320-326 "SUCCESS — plaintext stripped from config.json"; migrate.rs:160 Settings banner text | A fresh sign-in reconciles the secret and dismisses the banner. | P2
TROUBLE-C050 | CONFIRMED | keychain.rs:569-576 `classify_keychain_lookup` → `Unavailable(help)`; keychain.rs:628-636 | An unreadable keychain is a distinct third state, not "no credential". | P2
TROUBLE-C051 | CONFIRMED | keychain.rs:139-148 `KeychainPresence::Unavailable`; Reconnect.svelte:426 hint "your secret is still stored" | The state explicitly means "still stored but unreachable right now". | P2
TROUBLE-C052 | CONFIRMED | keychain.rs:392 comment "callers that start onboarding on an [`KeychainReadError::Unavailable`]"; SpotifyCard.svelte:118-124 #560 comment | Collapsing the two states is the historical bug that pushed set-up users back through the wizard. | P2
TROUBLE-C053 | CONFIRMED | SETUP.md:219-243 "Linux: System Keyring Required" with `gnome-keyring libsecret-1-0`; config/io.rs and keychain `sync-secret-service` | The Linux fix is unlocking the keyring / running a Secret Service daemon, cross-referenced correctly. | P2
TROUBLE-C054 | CONFIRMED | Reconnect.svelte:227-235 `reconnectSpotify` re-`loadConfig()` and re-probes keychain on every entry | Re-entering the view re-probes the keychain. | P2
TROUBLE-C055 | CONFIRMED | Reconnect.svelte:420-427 #560 comment "no reconnect button here on purpose - the flow reads the secret from the very keychain that cannot answer" | The Reconnect button is deliberately absent in that state. | P2
TROUBLE-C056 | CONFIRMED | Reconnect.svelte:426 hint "you do not need to set Spotify up again" | No Client ID/Secret re-entry is required. | P2
TROUBLE-C057 | CONFIRMED | en.ts:421 `'settings.secretKeychainUnavailable': 'System keychain unavailable — it may be locked or missing. Unlock it (or install a Secret Service provider) to use your saved secret; it is still stored.'`; SpotifyCard.svelte:128 | The Settings credential row shows that exact string. | P1
TROUBLE-C058 | CONFIRMED | SpotifyCard.svelte:130-131 and :166-167 — `onGoToOnboarding` / Run Onboarding only in the `secretState !== 'unavailable'` arms | A genuinely absent credential is the only case offering Run Onboarding. | P2
TROUBLE-C059 | CONFIRMED | teams.rs token refresh; polling/refresh.rs six-class decision table | Token expiry or account mismatch is a real cause of "Teams not updating". | P2
TROUBLE-C060 | CONFIRMED | TROUBLESHOOTING.md:118 | Verifying the signed-in Microsoft account is the documented first step. | P3
TROUBLE-C061 | CONFIRMED | TeamsCard.svelte reconnect handler; teams.rs:292 device-code POST | Settings → Reconnect next to Teams starts the device-code sign-in. | P2
TROUBLE-C062 | CONFIRMED | commands/logs.rs `get_recent_logs`; app.rs:1115-1122 webview log target | The in-app Log Viewer surfaces API error codes. | P2
TROUBLE-C063 | CONFIRMED | teams.rs:254-262 `expires_in`; DeviceCodeBox.svelte:24-38 | The device-code sign-in window expires server-side. | P2
TROUBLE-C064 | CONFIRMED | DeviceCodeBox.svelte:58-65 countdown `t('common.codeExpiresIn')`; TeamsCard.svelte:85 `expired=` | A live countdown is shown while the code is valid. | P2
TROUBLE-C065 | CONFIRMED | DeviceCodeBox.svelte:54-56 expired state; en.ts:43 `'common.getNewCode': 'Get a new code'` | An expired state offers "Get a new code". | P2
TROUBLE-C066 | CONFIRMED | DeviceCodeBox.svelte:58-65 | Completing the sign-in before the countdown runs out is the documented fix. | P3
TROUBLE-C067 | CONFIRMED | teams.rs:342 `verification_url`; DeviceCodeBox verificationUrl prop | The app displays the verification URL to visit. | P3
TROUBLE-C068 | CONFIRMED | teams.rs:427-432 `"authorization_declined"` poll arm; en.ts authorization_failed copy | Declining or mistyping the code is the documented cause of a sign-in loop. | P2
TROUBLE-C069 | CONFIRMED | teams.rs:335-345 fresh device code on restart | Re-clicking "Sign in with Microsoft" issues a fresh code and URL. | P2
TROUBLE-C070 | CONFIRMED | DeviceCodeBox.svelte monospace select-all code pill; teams.rs RFC 8628 code handling | Codes are case-sensitive and must be entered exactly. | P3
TROUBLE-C071 | CONFIRMED | teams.rs:26 `MICROSOFT_GRAPH_SCOPES` = "Presence.ReadWrite Presence.Read Calendars.ReadBasic MailboxSettings.Read openid profile offline_access"; teams.rs:905 | The Presence toggles need `Presence.Read` and `profile`, both in the scope string. | P2
TROUBLE-C072 | CONFIRMED | TeamsCard.svelte:103-108 presence scope banner with Reconnect | The banner's Reconnect button performs the one-time reconnect. | P2
TROUBLE-C073 | CONFIRMED | app.rs:24-29 "True when a close request on `label` hides the window instead of destroying it" | The app minimizes to tray to keep syncing. | P3
TROUBLE-C074 | CONFIRMED | tray/mod.rs:1398 quit item; i18n.rs:145 "Quit" | Right-click tray → Quit is the exit path. | P2
TROUBLE-C075 | CONFIRMED | app.rs:1218-1244 CloseRequested arm | The close button only hides the app to tray, where polling continues. | P2
TROUBLE-C076 | CONFIRMED | app.rs:37-45 + :56-69 `notify_no_tray_close` | Without a tray the window closes for real and the app quits; sync stops. | P2
TROUBLE-C077 | CONFIRMED | app.rs:64-68 notification names "Install libayatana-appindicator3 and the GNOME AppIndicator extension" | A notification explains the real close and how to restore the tray (#819). | P2
TROUBLE-C078 | CONFIRMED | app.rs:64-68 `notify_no_tray_close` names exactly "Install libayatana-appindicator3 and the GNOME AppIndicator extension" | The remedy text matches the notification the app emits. | P2
TROUBLE-C079 | CONFIRMED | TROUBLESHOOTING.md:22 heading "### Linux: no tray icon and no window" | The linked anchor exists. | P3
TROUBLE-C080 | CONFIRMED | app.rs:44-45 "letting the close proceed quits the app" | Closing without a tray is a full quit; relaunch starts a fresh session. | P2
TROUBLE-C081 | CONFIRMED | en.ts:30 `'common.launchAtLogin': 'Launch at login'`; AppearanceCard.svelte:199-206 | Settings has the Launch at Login toggle. | P3
TROUBLE-C082 | CONFIRMED | tauri-plugin-autostart-2.5.1/src/lib.rs:180-184 `app_name` = `app.package_info().name` = productName "PresenceJam" (tauri-codegen context.rs:268-286); auto-launch-0.5.0/src/linux.rs:80-83 `~/.config/autostart/{app_name}.desktop`; SETUP.md:61-62 | The Linux login entry is exactly `~/.config/autostart/PresenceJam.desktop`; Windows writes an HKCU Run value, macOS a LaunchAgent. | P1
TROUBLE-C083 | CONFIRMED | stores/detach.ts:27-28 `'logs-detached' | 'settings-detached'` | The detached window labels are exact. | P2
TROUBLE-C084 | CONFIRMED | Dashboard.svelte:794-808 `focusDetached('settings'|'logs')`; :1487-1499 `.icon-btn.detached::after` dot | While detached, the nav shows a dot badge and focuses the detached window. | P2
TROUBLE-C085 | CONFIRMED | Dashboard.svelte:801-808 `currentView.set(...)` when not detached; detach.ts | Clicking Logs/Settings re-opens the view in-window after the detached window closes. | P2
TROUBLE-C086 | CONFIRMED | AppearanceCard.svelte:139-165; i18n/store.svelte.ts:143-158 `detectInitialLocale` | The picker sits at Settings → Appearance → Language and defaults to browser/OS language. | P2
TROUBLE-C087 | CONFIRMED | AppearanceCard.svelte:157-164 — all eight locales en/de/fr/es/it/pl/pt/nl are listed | The picker offers more than English/Deutsch/Français; the fix step names only three. | P2
TROUBLE-C088 | CONFIRMED | i18n/store.svelte.ts applyDocumentLang + persistence; AppearanceCard.svelte:146-156 | The choice applies immediately and persists across restarts. | P3
TROUBLE-C089 | CONFIRMED | src/lib/i18n.ts:20-22 "Known limitation (per scope doc §C6): Rust-side error strings surfaced through invoke() rejections and event payloads remain English" | Rust-side error strings are English by design; only UI strings are localized. | P2
TROUBLE-C090 | CONFIRMED | updater_bg.rs:1779-1793 `install_pending_on_exit` wired to `tauri::RunEvent::Exit`; updater_bg.rs:558-575 | Install-on-quit applies the update while exiting, with no window to show progress. | P2
TROUBLE-C091 | CONFIRMED | updater_bg.rs:1844-1865 `Err(e) => { write_failed_install_marker(...); log::error!(...) }` | A failure at exit is recorded only in the log file (marker + log line), never in the UI (#244). | P2
TROUBLE-C092 | CONFIRMED | updater_bg.rs:55 `const TAG: &str = "[UPDATER.BG]";`; :1844-1865 | Quitting/relaunching and reading `PresenceJam.log` for those lines is the documented inspection path. | P2
TROUBLE-C093 | CONFIRMED | UpdatePrompt.svelte:35-36 "dismissible banner with a 'Download & Install' button (immediate relaunch)" | The Download & Install path reports errors in-app. | P3
TROUBLE-C094 | CONFIRMED | updater_bg.rs:769-779 `is_stale_version`; CHANGELOG.md:836 "v4.2.0 ... a stale stage (`staged <= current`) is skipped with a log + marker instead of installing" | Since v4.2.0 a stale staged update is deliberately skipped and logged, not installed. | P2
TROUBLE-C095 | CONFIRMED | UpdatePrompt.svelte:419 and :659 `t('update.installAnyway')`; en.ts:305 | The banner shows a skipped state with an "Install anyway" override (#431). | P2
TROUBLE-C096 | CONFIRMED | tauri.conf.json:70 `"allowDowngrades": false`; updater_bg.rs:1898-1918 test asserting it | Downgrades are off by default. | P2
TROUBLE-C097 | CONFIRMED | config/io.rs:478-493 `Ok(other) => { quarantine_corrupt_config(...); return Ok(AppConfig::default()); }` | An unparsable-as-object config.json is not loaded. | P2
TROUBLE-C098 | CONFIRMED | config/io.rs:250-273 `quarantine_corrupt_config` → `fs::rename(path, &backup)`; :69-73 `quarantine_backup_path` appends ".bak" | The unreadable file is renamed beside itself as `config.json.bak` rather than overwritten. | P2
TROUBLE-C099 | CONFIRMED | config/io.rs:257-262 `log::warn!("[CFG] corrupt config '{}' quarantined to '{}': {} — loading defaults", ...)` | The `[CFG] corrupt config '…' quarantined to '…'` warning matches the emit site. | P2
TROUBLE-C100 | CONFIRMED | config/io.rs:484, 491 `return Ok(AppConfig::default())`; schema.rs `Default` impls | After quarantine the app starts on shipped defaults. | P2
TROUBLE-C101 | CONFIRMED | config/io.rs:257-262 logs both paths | The `[CFG]` line names the original and the backup. | P2
TROUBLE-C102 | CONFIRMED | Diagnostics.svelte:211-221 quarantine section `role="alert"`; en.ts:424 `'diagnostics.quarantineTitle': 'Settings were reset'`; :414-421 `.quarantine { border-color: var(--warning) }` | The Diagnostics page (🩺) shows an amber "Settings were reset" banner for the same event. | P2
TROUBLE-C103 | CONFIRMED | Diagnostics.svelte:45-61 `quarantineNotice` derived from `config_quarantine_backup ?? snapshot.config.config_quarantined`; diagnostics.rs:787-789 | The banner persists across launches and names the backup file. | P2
TROUBLE-C104 | CONFIRMED | config/io.rs:69-73 backup path; quarantine only renames | Old settings remain in the `.bak`. | P3
TROUBLE-C105 | CONFIRMED | config/io.rs:19-54 — `directories::BaseDirs::config_dir()` on Windows is `%APPDATA%` | `%APPDATA%\PresenceJam\config.json.bak` is correct. | P1
TROUBLE-C106 | CONFIRMED | config/io.rs:21-24 comment "~/Library/Application Support on macOS"; BaseDirs::config_dir() | `~/Library/Application Support/PresenceJam/config.json.bak` is correct. | P1
TROUBLE-C107 | DRIFT | TROUBLESHOOTING.md:202 `$XDG_CONFIG_HOME\PresenceJam\config.json.bak` vs config/io.rs:19-54 (dir = `$XDG_CONFIG_HOME/PresenceJam/`) | Linux location is right but the path is spelled with Windows backslashes. | P1
TROUBLE-C108 | CONFIRMED | config/io.rs:69-73 fixed `path + ".bak"`, no timestamp | The backup name is fixed, never timestamped. | P2
TROUBLE-C109 | CONFIRMED | config/io.rs:256-270 both rename arms log and swallow the error; :271 flag set regardless | A failed rename is logged and the app still boots on defaults with the original untouched. | P2
TROUBLE-C110 | CONFIRMED | token_io.rs:152-178 `tokens_file_path` uses `app_config_dir()` (bundle id) while config uses `config_dir()` | tokens.json lives in a different folder entirely. | P2
TROUBLE-C111 | CONFIRMED | SETUP.md:169 "## What Gets Installed" | The linked anchor exists. | P3
TROUBLE-C112 | CONFIRMED | config/io.rs:229-238 `quarantine_backup_name_for` returns None when no backup exists; Diagnostics.svelte:59-61 renders `quarantineBackupMissing` | A banner without a backup name means the rename failed and the original is still `config.json`. | P2
TROUBLE-C113 | CONFIRMED | Dashboard.svelte:989-993 `class:success={spotifyConnected}` / `class:error={!teamsConnected}` with `.dot` | The Dashboard shows both providers as connected with green badges. | P2
TROUBLE-C114 | CONFIRMED | polling/timing.rs:248-258 `playing_track_sleep` — known position sleeps until ~5 s before track end | With a track playing the status updates within a few seconds. | P3
TROUBLE-C115 | CONFIRMED | polling/timing.rs:292-300 `pause_backoff` ladder default 30 → 60 → 120 → ceiling; clamp.rs `pause_backoff_max_seconds` default 300 | The no-playback backoff ladder is 30 → 60 → 120 → 300 s. | P2
TROUBLE-C116 | CONFIRMED | polling/timing.rs:293-300; schema.rs `default_pause_backoff_max` = 300 | An idle app can take up to five minutes to react. | P3
TROUBLE-C117 | CONFIRMED | polling/timing.rs:10 `ERROR_RETRY_INTERVAL_SECONDS = 30`; :304-309 `with_jitter` ±20 % | Failed polls retry after ~30 s ±20 %. | P2
TROUBLE-C118 | CONFIRMED | polling/rules.rs:127 `GATE_REASON_QUIET_HOURS`; en.ts:470 `'dashboard.presenceGatedQuietHours': 'Status paused — quiet hours are active'`; Dashboard.svelte:44 | The chip text matches the quiet-hours cause. | P2
TROUBLE-C119 | CONFIRMED | polling/rules.rs:47-49 suppress rule with empty replacement; en.ts:471 | A track rule with an empty "Post this instead" suppresses the write and shows that chip. | P2
TROUBLE-C120 | CONFIRMED | teams.rs:956 `GATE_REASON_MANUAL_STATUS`; en.ts:442 `'settings.respectManualStatusLabel'`; :473 manual-status chip | The manual-status gate and its chip are exactly as described. | P2
TROUBLE-C121 | CONFIRMED | teams.rs:963 `GATE_REASON_OUT_OF_OFFICE = "out of office"`; en.ts:445 `'settings.gateOutOfOfficeLabel': 'Pause while I am out of office'`; :474 | The out-of-office gate and its chip match. | P2
TROUBLE-C122 | CONFIRMED | teams.rs:967-973 presenting / quiet-time / idle reasons; gate.rs:229; en.ts:66 busy chip | The busy/in-a-call/presenting chip covers busy, DND, focusing, in a meeting, in a call, presenting. | P2
TROUBLE-C123 | CONFIRMED | polling/write.rs:1086-1090 `"[POLLING] process_track: {} active, skipping status write"` with reason = `GATE_REASON_QUIET_HOURS` | The log line renders as the documented string for quiet hours. | P2
TROUBLE-C124 | CONFIRMED | polling/write.rs:1086-1090 same format string with `GATE_REASON_TRACK_RULE` (teams.rs:955) | The log line renders as "… track-rule active, skipping status write"; the doc's "… track rule matched, …" is loose paraphrase. | P2
TROUBLE-C125 | CONFIRMED | polling/write.rs:1387 `"[POLLING] process_track: track presence-gated, skipping status write"` | The presence-gated log line exists verbatim. | P2
TROUBLE-C126 | CONFIRMED | polling/write.rs:231-233 quiet-hours pause runs only on the gate flag; loop.rs continues otherwise | Polling continues on its normal cadence and the write resumes when the cause clears. | P2
TROUBLE-C127 | CONFIRMED | polling/loop.rs:291-306 quiet-hours pause; en.ts:501 `'rules.pausePollingLabel': 'Stop polling during this window'` | A quiet-hours row with Stop polling ticked pauses polling for that window only. | P2
TROUBLE-C128 | CONFIRMED | RulesCard.svelte:388 `t('rules.sectionTitle')` ('Status rules'); PresenceCard.svelte:16 `t('settings.sectionPresence')`; Settings.svelte:795-797 | Both Settings destinations exist as named. | P3
TROUBLE-C129 | CONFIRMED | USAGE.md:117 "### Status rules" | The linked anchor exists. | P3
TROUBLE-C130 | CONFIRMED | schema.rs:92-93 `clear_on_pause` with :204-206 default true; polling/write.rs:2022-2025 | The `clear_on_pause` config option exists and may be disabled. | P3
TROUBLE-C131 | CONFIRMED | schema.rs:92; searching Settings cards for a clear_on_pause toggle — none (only diagnostics.rs:768 surfaces it read-only) | `teams.clear_on_pause` is config-only with no Settings toggle. | P2
TROUBLE-C132 | CONFIRMED | sources/spotify.rs:128-142 `NotPremium` → source fallback; sources/mpris.rs on Linux | Desktop is more reliable; the API detects both but desktop is preferred. | P3
TROUBLE-C133 | CONFIRMED | polling/status_text.rs:247-262 `status_expiry_str` = now + remaining + buffer; schema.rs:321-323 `expiry_buffer_seconds` default 10; clamp.rs:21 | Playing-track expiry is track end + buffer (default 10 s), configurable via `polling.expiry_buffer_seconds`. | P2
TROUBLE-C134 | CONFIRMED | polling/status_text.rs:64-71 `stopped_status_placeholder` = `MUSIC_EMOJI + stopped_text`; schema.rs:236-242 defaults "Paused" / "Nothing playing on Spotify"; commands/sync.rs `paused_status_placeholder` | The pause/stop placeholders are posted with the 🎵 prefix. | P2
TROUBLE-C135 | CONFIRMED | polling/timing.rs:236-243 `placeholder_expiry_str()` = now + 60 s, used at exit.rs:120 and write.rs:1803,2181 | The placeholder has a fixed 60 s expiry and self-removes. | P2
TROUBLE-C136 | CONFIRMED | schema.rs:161-167 `paused_status_format` / `stopped_status_format` with :236-242 defaults; RulesCard.svelte:1013-1030; en.ts:511-513 | Both texts are user-editable since 4.7 and clearing a field restores the default. | P2
TROUBLE-C137 | CONFIRMED | learn.microsoft.com/en-us/graph/api/resources/presencestatusmessage: "expiryDateTime ... If not provided, the status message doesn't expire" — no 24-hour cap documented | Graph-set messages carry only the app-supplied expiryDateTime; the Teams client's "Clear status message after" is a client-side feature. | P3
TROUBLE-C138 | CONFIRMED | schema.rs:321-323 buffer key; schema.rs:92-93 clear_on_pause; both absent from Settings cards | Raising the buffer or setting `teams.clear_on_pause` false in config.json keeps the status visible longer. | P2
TROUBLE-C139 | CONFIRMED | StatusFormatCard.svelte:82-89 profanity filter toggle under `t('settings.sectionStatusFormat')`; en.ts:150 | Settings has a Profanity Filter toggle. | P2
TROUBLE-C140 | CONFIRMED | profanity.rs:20-27 `resolve_placeholder` — whitespace-only falls back to `SAFE_PLACEHOLDER_DEFAULT` | A whitespace-only Placeholder field falls back to the default. | P2
TROUBLE-C141 | CONFIRMED | spotify.rs:1811-1827 emoji precedence `(false,_) => "⏸️", (true,true) => "🎙️", (true,false) => "🎵"`; en.ts:153 | A `{emoji}` in the placeholder resolves to 🎵 playing or ⏸️ paused. | P2
TROUBLE-C142 | CONFIRMED | polling/write.rs:1506 `profanity::filter_status_for_locale(...)` applied to the formatted status at the single write path | The filter operates on the formatted status string after the template is applied. | P2
TROUBLE-C143 | CONFIRMED | spotify.rs:1806-1830 — all 13 tokens are substituted in one pass, so raw artist/track/album only reach the wire through the template | Raw metadata not consumed by the template may not be covered. | P3
TROUBLE-C144 | CONFIRMED | docs/architecture/polling.md:247 "### Profanity filter"; polling/write.rs:1506; profanity.rs:508-520 | The linked anchor exists and the filter is verified at the single status-write path. | P2
TROUBLE-C145 | CONFIRMED | profanity.rs:1-6 `PROFANITY_LIST` in `src-tauri/src/profanity.rs` | The list lives in that file and covers common English profanity. | P2
TROUBLE-C146 | CONFIRMED | profanity.rs:810-816 `test_leetspeak_substitutions` asserts sh1t, $hit, d@mn, p1ss, n1gg3r | All five leetspeak variants are covered. | P2
TROUBLE-C147 | CONFIRMED | profanity.rs:863-868 `test_repeated_char_collapse` asserts shiiit, fuuuuck | Repeated-character variants are covered. | P2
TROUBLE-C148 | CONFIRMED | profanity.rs:587-596 legacy fuck-derivation carve-out for "ing"/"er"/"ed"; :1115, :1238-1249 tests | The fucking/fucked/fucker variants are handled. | P2
TROUBLE-C149 | CONFIRMED | StatusFormatCard.svelte:118-130 custom-words textarea; en.ts:461 `'settings.extraWordsLabel': 'Custom words to filter'` | Custom words are added in Settings with that label. | P2
TROUBLE-C150 | CONFIRMED | config/clamp.rs:28-38 truncate to 64 entries of 32 chars; StatusFormatCard.svelte:36-37 EXTRA_WORDS_MAX_ENTRIES/CHARS | One per line, up to 64 entries of 32 characters. | P2
TROUBLE-C151 | CONFIRMED | profanity.rs:1286-1296 `test_extra_words_respect_word_boundaries` — spam flags spam/Spam sandwich but not spamalot/mispam/Spammy | Adding `spam` does not flag `spamalot`. | P2
TROUBLE-C152 | CONFIRMED | profanity.rs:1298-1304 `test_extra_words_keep_the_evasion_rules` asserts s.p.a.m, "s p a m", 5pam | Separator/leet evasion still applies to custom words. | P2
TROUBLE-C153 | CONFIRMED | Settings.svelte:294-322 $effect re-invokes `preview_status` on extra-words change (300 ms debounce) | The status preview updates as you type. | P3
TROUBLE-C154 | CONFIRMED | profanity.rs:970-986 `test_word_boundary_respects_clean_words` — class, assassin, mass, pass, choke, cocktail(s), cumulative, vacuum, cockpit, Dickens, Spice Girls, shiitake all clean | Word-boundary checks prevent the listed false positives. | P2
TROUBLE-C155 | CONFIRMED | stores/detach.ts:27 'logs-detached'; Dashboard.svelte:1016-1019; TROUBLESHOOTING.md:276 | The Log Viewer can be popped out into its own window. | P2
TROUBLE-C156 | CONFIRMED | commands/logs.rs:42-44 `MAX_LOG_LINES: usize = 500` — "Matches the LogViewer's 500-entry buffer (#399)" | Opening the Log Viewer backfills the last 500 lines. | P2
TROUBLE-C157 | CONFIRMED | LogViewer.svelte:43-64,111-137,165-188 scroll-anchor logic gated on `atBottom` | Scrolled-up position is held as new lines arrive. | P2
TROUBLE-C158 | CONFIRMED | app.rs:1115-1117 `TargetKind::LogDir { file_name: Some("PresenceJam") }`; commands/logs.rs:39-41 | The direct-filesystem path is `PresenceJam.log` plus rotated archives, managed by the logging plugin. | P2
TROUBLE-C159 | CONFIRMED | tauri-2.11.6/src/path/desktop.rs:285-287 `data_local_dir()/<identifier>/logs`; on Windows data_local_dir = %LOCALAPPDATA% | `%LOCALAPPDATA%\com.presencejam.app\logs\PresenceJam.log` is correct. | P1
TROUBLE-C160 | CONFIRMED | tauri-2.11.6/src/path/desktop.rs:280-282 `home_dir()/Library/Logs/<identifier>`; diagnostics.rs:442 comment naming `~/Library/Logs/com.presencejam.app/` | `~/Library/Logs/com.presencejam.app/PresenceJam.log` is correct. | P1
TROUBLE-C161 | CONFIRMED | tauri-2.11.6/src/path/desktop.rs:285-287; on Linux data_local_dir = $XDG_DATA_HOME or ~/.local/share; diagnostics.rs:2805 test string `/home/jack/.local/share/com.presencejam.app/logs` | `~/.local/share/com.presencejam.app/logs/PresenceJam.log` is correct. | P1
TROUBLE-C162 | CONFIRMED | config/clamp.rs:480 `clamp(1, 500)`; schema.rs:428-430 default 10 | Rotation triggers at `logging.max_file_size_mb` (1–500 MB, default 10). | P2
TROUBLE-C163 | CONFIRMED | config/clamp.rs:481 `clamp(1, 20)`; schema.rs:432-434 default 3 | `logging.keep_files` retains 1–20 archives, default 3. | P2
TROUBLE-C164 | CONFIRMED | schema.rs:397-400 "The active `PresenceJam.log` is not counted, so the directory holds at most `keep_files + 1` log files" | Folder holds up to `keep_files + 1` files. | P2
TROUBLE-C165 | CONFIRMED | LoggingCard.svelte:61-88 — both number inputs bound to `logging.max_file_size_mb` / `logging.keep_files` under `t('settings.sectionLogging')` | Both are editable in Settings → Logging. | P2
TROUBLE-C166 | CONFIRMED | tauri-2.11.6/src/path/desktop.rs:280-287 — both non-macOS and macOS arms join `config().identifier`; token_io.rs:152-157 notes the same for #300 | `app_log_dir()` appends the bundle id. | P2
TROUBLE-C167 | CONFIRMED | config/io.rs:25-31 (no bundle id) vs token_io.rs:155-157 (bundle id); app.rs:442-444 #300 comment | Logs do not sit next to config.json (#300). | P2
TROUBLE-C168 | CONFIRMED | tray/mod.rs:1118,1398 `MenuItemBuilder::with_id(ID_OPEN_LOGS, s.open_logs_folder)`; i18n.rs:144 "Open Logs Folder"; commands/window.rs:201-210 | Tray menu → Open Logs Folder opens that same folder. | P2
TROUBLE-C169 | CONFIRMED | tauri-2.11.6/src/path/desktop.rs:285-287 — same resolution as C159 | The PowerShell command is correct. | P2
TROUBLE-C170 | CONFIRMED | log::error! sites across polling/write.rs, teams.rs, spotify.rs (e.g. teams.rs:823) | ERROR means a failed API call or file I/O. | P3
TROUBLE-C171 | CONFIRMED | log::warn! sites across polling/iteration.rs (recoverable retries), config/io.rs | WARN means something unexpected but recoverable. | P3
TROUBLE-C172 | CONFIRMED | polling/write.rs info-level "status updated" emits; spotify.rs info logs | INFO covers normal operations. | P3
TROUBLE-C173 | CONFIRMED | polling/loop.rs:393 `log::debug!("[POLLING] polling_loop: sleeping for {} seconds", seconds)` | DEBUG logs every polling iteration (filtered out at default Info level). | P2
TROUBLE-C174 | CONFIRMED | schema.rs:424-426 `default_log_level() = "Info"`; LoggingCard.svelte:60-64 log-level select | The log level is set in Settings → Logging and stored at `logging.log_level`, default Info. | P2
TROUBLE-C175 | CONFIRMED | app.rs:1088-1095 comment "a change to size/retention takes effect at the next launch"; config/io.rs:554-585 `logging_config_for_startup` | Size/retention apply from the next launch. | P2
TROUBLE-C176 | CONFIRMED | tray/mod.rs:1118,1398; tauri-2.11.6/src/path/desktop.rs:285-287 | The open-log-folder path and Windows variant are correct. | P2
TROUBLE-C177 | CONFIRMED | app.rs:1115-1117 targets; commands/logs.rs:39-41 | `PresenceJam.log` plus rotated archives in the same folder. | P3
TROUBLE-C178 | CONFIRMED | procedural advice; consistent with the log format (timestamp, target, level, message) | Noting the approximate time is the documented reporting step. | P3
TROUBLE-C179 | CONFIRMED | polling/timing.rs smart-sleep design; app.rs:1121-1122 max_file_size cap | The app is designed to be lightweight (adaptive sleep, no fixed busy loop). | P3
TROUBLE-C180 | CONFIRMED | app.rs:1069-1079 single-instance plugin | Checking for a second instance is sensible and matches the single-instance behaviour. | P3
TROUBLE-C181 | CONFIRMED | polling/loop.rs:393 `log::debug!("[POLLING] polling_loop: sleeping for {} seconds", seconds)` | The referenced log string exists verbatim. | P2
TROUBLE-C182 | EXTERNAL-UNVERIFIED | no allowlisted vendor page states a 1-3 s Tauri cold-start figure; v2.tauri.app has no such performance claim | The cold-start figure is not documented by any allowlisted source. | P3
TROUBLE-C183 | OVERSTATED | tauri-2.11.6/src/path/desktop.rs + single-instance plugin (app.rs:1069-1079) — a second launch forwards to the running instance rather than cold-starting | "Subsequent launches from the tray are faster" is not verifiable; the tray raises the running instance instead of relaunching. | P3
TROUBLE-C184 | CONFIRMED | tauri.conf.json:31 connect-src; spotify.rs:667,1289; teams.rs:292,790 | All four domains are the real HTTPS endpoints. | P2
TROUBLE-C185 | CONFIRMED | spotify.rs:667 `https://accounts.spotify.com/api/token` | accounts.spotify.com is used. | P2
TROUBLE-C186 | CONFIRMED | spotify.rs:1289 `https://api.spotify.com/v1/me/player/currently-playing` | api.spotify.com is used. | P2
TROUBLE-C187 | CONFIRMED | teams.rs:292,494,640 `https://login.microsoftonline.com/common/oauth2/v2.0/...` | login.microsoftonline.com is used. | P2
TROUBLE-C188 | CONFIRMED | teams.rs:790 `https://graph.microsoft.com/v1.0/me/presence/setStatusMessage` | graph.microsoft.com is used. | P2
TROUBLE-C189 | CONFIRMED | tauri.conf.json:31 connect-src allowlist matches the four domains exactly | Blocking those domains breaks the app. | P3
TROUBLE-C190 | CONFIRMED | generic network diagnosis; consistent with the app's direct HTTPS calls | Network connectivity / VPN interference is a valid cause. | P3
TROUBLE-C191 | CONFIRMED | procedural step; no repo contradiction | Verifying Spotify reachability is reasonable advice. | P3
TROUBLE-C192 | CONFIRMED | procedural step; no repo contradiction | Firewall guidance naming the three OS firewalls is correct. | P3
TROUBLE-C193 | CONFIRMED | tray/mod.rs:1398 quit item; i18n.rs:145 | Quit via right-click tray → Quit is the documented exit. | P2
TROUBLE-C194 | CONFIRMED | tauri.conf.json bundle targets include "msi","nsis" (Windows installers) | Windows uninstall via Settings → Apps is the standard path. | P3
TROUBLE-C195 | CONFIRMED | tauri.conf.json bundle targets include "dmg"; SETUP.md:24 "Drag PresenceJam to Applications" | macOS drag-to-Trash matches the install method. | P3
TROUBLE-C196 | CONFIRMED | .github/workflows/release.yml:465-466 "Linux artifacts are named like `presence-jam_2.7.2_amd64.deb` by Tauri's bundler"; tauri.conf.json:55-58 deb bundle | The deb package name is `presence-jam`. | P2
TROUBLE-C197 | CONFIRMED | tauri.conf.json bundle targets include "appimage"; SETUP.md:21 "or run the `.AppImage` directly" | AppImage removal is deleting the file. | P3
TROUBLE-C198 | CONFIRMED | config/io.rs:19-54; token_io.rs:152-178; app_log_dir | Deleting user data removes all tokens and config. | P3
TROUBLE-C199 | CONFIRMED | config/io.rs:25-31 vs token_io.rs:152-157 (#300) vs tauri-2.11.6/src/path/desktop.rs:280-287 | The three stores are in different folders. | P2
TROUBLE-C200 | CONFIRMED | config/io.rs:19-54 | Windows config path is correct. | P1
TROUBLE-C201 | CONFIRMED | token_io.rs:160-178 `app_config_dir()/PresenceJam/tokens.json`; tauri app_config_dir appends the identifier | Windows tokens path is correct. | P1
TROUBLE-C202 | CONFIRMED | tauri-2.11.6/src/path/desktop.rs:285-287; diagnostics.rs:2805 test string | Windows logs path is correct. | P1
TROUBLE-C203 | CONFIRMED | config/io.rs:21-24 | macOS config path is correct. | P1
TROUBLE-C204 | CONFIRMED | token_io.rs:160-178 | macOS tokens path is correct. | P1
TROUBLE-C205 | CONFIRMED | tauri-2.11.6/src/path/desktop.rs:280-282; app.rs:442-444 comment | macOS logs path is correct. | P1
TROUBLE-C206 | CONFIRMED | config/io.rs:21-23 | Linux config path is correct. | P1
TROUBLE-C207 | CONFIRMED | token_io.rs:199-211 `tokens_file_path_headless` = `config_dir()/com.presencejam.app/PresenceJam/tokens.json` | Linux tokens path is correct. | P1
TROUBLE-C208 | CONFIRMED | tauri-2.11.6/src/path/desktop.rs:285-287 | Linux logs path is correct. | P1
TROUBLE-C209 | CONFIRMED | config/io.rs:19-54 | The PowerShell path targets the right folder. | P2
TROUBLE-C210 | CONFIRMED | token_io.rs:160-178 | The PowerShell path targets the right folder. | P2
TROUBLE-C211 | CONFIRMED | tauri-2.11.6/src/path/desktop.rs:285-287 | The PowerShell path targets the right folder. | P2
TROUBLE-C212 | CONFIRMED | tauri-plugin-autostart-2.5.1/src/lib.rs:180-184 (app_name = package_info().name = "PresenceJam"); auto-launch-0.5.0/src/linux.rs:80-83 (`~/.config/autostart/{app_name}.desktop`); SETUP.md:61-62 | Windows/macOS login items are removed with the app by their platforms; the Linux entry must be deleted manually. | P2
TROUBLE-C213 | CONFIRMED | SETUP.md:26-54 — the AppImage launcher entry is written by the user to `~/.local/share/applications/presencejam.desktop`, never by the installer | The file exists only if the user created it for an AppImage install. | P2
TROUBLE-C214 | CONFIRMED | SETUP.md:66-88 — Spotify credentials live in the developer dashboard, not on disk | Uninstalling does not touch the Spotify Developer Dashboard app. | P3

---

## DEFECTS (non-CONFIRMED, by blast radius)

```
claim_id: TROUBLE-C107
file: TROUBLESHOOTING.md:202
class: C1
verdict: DRIFT
evidence: TROUBLESHOOTING.md:202 reads `$XDG_CONFIG_HOME\PresenceJam\config.json.bak (usually ~/.config/PresenceJam/)` — a Windows backslash in a Linux path; the sibling Windows row at :200 uses the same backslash style, so the Linux row is a copy-paste of the Windows separator. config/io.rs:19-54 confirms the directory is `$XDG_CONFIG_HOME/PresenceJam/` and the backup is `config.json.bak` beside it, so the *location* is right and only the separator is wrong.
finding: The Linux backup path is spelled with a backslash where a forward slash belongs, unlike every other Linux path in the same document (e.g. :366 `~/.local/share/com.presencejam.app/logs/`).
proposed_fix: Change `$XDG_CONFIG_HOME\PresenceJam\config.json.bak` to `$XDG_CONFIG_HOME/PresenceJam/config.json.bak`.
severity: P1
```

```
claim_id: TROUBLE-C087
file: TROUBLESHOOTING.md:180
class: C6
verdict: DRIFT
evidence: AppearanceCard.svelte:157-164 — the `<select id="language">` lists eight options (en, de, fr, es, it, pl, pt, nl); src/lib/i18n.ts:22-33 `DICTS: Record<Locale, Dict> = { en, de, fr, es, it, pl, pt, nl }`; store.svelte.ts:43 KNOWN has the same eight tags.
finding: The picker offers eight languages, not three, and the choice is not limited to English/Deutsch/Français.
proposed_fix: Replace "Pick **English**, **Deutsch**, or **Français** in Settings → Appearance." with "Pick one of the eight shipped languages — English, Deutsch, Français, Español, Italiano, Polski, Português (BR) or Nederlands — in Settings → Appearance."
severity: P2
```

```
claim_id: TROUBLE-C183
file: TROUBLESHOOTING.md:321
class: C2
verdict: OVERSTATED
evidence: app.rs:1069-1079 registers the single-instance plugin; app.rs:314-330 `forward_launch_to_running_instance` raises the already-running window; TROUBLESHOOTING.md:150-153 itself documents that closing only hides to tray.
finding: "Subsequent launches from the tray are faster" describes a cold start that does not happen — a second launch forwards to the running instance, which is why nothing is relaunched from the tray.
proposed_fix: Replace "This is normal — subsequent launches from the tray are faster." with "This is normal — a second launch while the app is running raises the existing window instead of cold-starting, so it appears instantly."
severity: P3
```

```
claim_id: TROUBLE-C182
file: TROUBLESHOOTING.md:321
class: C5
verdict: EXTERNAL-UNVERIFIED
evidence: No allowlisted vendor page (v2.tauri.app and friends) states a 1-3 s Tauri cold-start figure; the claim is unsourced performance folklore rather than a documented contract.
finding: The 1-3 second cold-start figure is asserted without a source and cannot be confirmed from the repo or from any allowlisted vendor document.
proposed_fix: Either cite an allowlisted Tauri source that quantifies cold-start time, or soften to "First launch takes a moment while the webview and the tray initialise — this is normal."
severity: P3
```

---

## COUNTS

| Verdict | Count |
| --- | --- |
| CONFIRMED | 211 |
| DRIFT | 1 |
| STALE | 0 |
| MISSING | 0 |
| OVERSTATED | 1 |
| EXTERNAL-UNVERIFIED | 1 |
| UNSOURCED | 0 |
| **Total** | **214** |

Notes on scope: every claim in `claims/TROUBLE.md` was checked; claims that describe procedures rather than repository state (C023, C033, C060, C066, C067, C069, C070, C089-step, C178, C190-C192, C194-C198, C214) are rated on whether the procedure matches the real command/UI path it names. External-contract claims were resolved only against the allowlisted vendor set, with fetch date 2026-10-08; `developer.apple.com` (C006-C009) and `developer.spotify.com/documentation/web-api/*` (C034, C037-C038) and `learn.microsoft.com/graph/api/resources/presencestatusmessage` (C137) were fetched and quoted. One claim depends on a numeric figure no allowlisted source states (C182).

