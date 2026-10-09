# docs-grounding audit — findings for SETUP.md and CONTRIBUTING.md

Audited: `docs/audit/claims/SETUP.md` (180 claims, `SETUP-C001`…`SETUP-C180`) and
`docs/audit/claims/CONTRIB.md` (89 claims, `CONTRIB-C001`…`CONTRIB-C089`).
HEAD verified: `09341ecaad732e78454a2c65b383f0dfd1541d5a`.
Evidence gathered 2026-10-08. Vendor fetches dated 2026-10-08.

Convention: `path:line` citations are repo files. A `URL` + quoted sentence is a
vendor-doc fetch. `SETUP.md:N` / `CONTRIBUTING.md:N` line numbers refer to the
live source files on disk, which are the claim files' own `line:` values.

---

## VERDICTS

### SETUP.md (180 claims)

| claim_id | verdict | evidence | finding | severity |
|---|---|---|---|---|
| SETUP-C001 | CONFIRMED | docs/PLATFORMS.md:10-12 — "Windows 10 / 11 \| x64"; "macOS \| Apple Silicon (arm64) **only**"; "Linux (glibc) \| x86_64" | Prerequisite list matches the platform matrix row for row, Apple-Silicon-only included. | — |
| SETUP-C002 | CONFIRMED | docs/PLATFORMS.md:1-13 — "Anything not listed here is unsupported" | The link target exists and does carry the full matrix plus the unsupported set (Intel Macs, non-glibc). | — |
| SETUP-C003 | CONFIRMED | docs/PLATFORMS.md:12 — "A running Secret Service keyring daemon is a hard requirement" | The named anchor `#linux-system-keyring-required` exists at SETUP.md:219 and says the same thing. | — |
| SETUP-C004 | OVERSTATED | src-tauri/src/spotify.rs:147-169 (403 → `NotPremium`); USAGE.md:34 — "requires a **Spotify Premium** account"; TROUBLESHOOTING.md:83 — "playback control is a Premium-only Web API surface" | Premium is required for the *player/write* endpoints the tray controls call (403 mapping at spotify.rs:169), not for reading the currently-playing track; SETUP.md:10 states the blanket version. | P3 |
| SETUP-C005 | CONFIRMED | src-tauri/src/teams.rs:19 `MICROSOFT_GRAPH_CLIENT_ID`; teams.rs:271-287 device-code POST to `login.microsoftonline.com/common/oauth2/v2.0/devicecode` | A work/school Microsoft account is what the shared public-client device-code flow authenticates against. | — |
| SETUP-C006 | CONFIRMED | learn.microsoft.com/en-us/graph/api/presence-get, fetched 2026-10-08 — "Delegated (personal Microsoft account) \| Not supported." | The vendor page's permission table marks the personal-Microsoft-account delegated row as not supported for the presence API SETUP.md:112 names. | — |
| SETUP-C007 | CONFIRMED | src-tauri/tauri.conf.json:88-90 — updater endpoint `https://github.com/Carme99/PresenceJam-Desktop/releases/latest/download/latest.json` | The Releases URL in the link resolves to the repo that publishes `latest.json`. | — |
| SETUP-C008 | CONFIRMED | README.md:65-67 — NSIS setup.exe / DMG / deb install instructions | Standard installer-prompt procedure; no contradicting step in the repo. | — |
| SETUP-C009 | CONFIRMED | .github/workflows/release.yml:323-326 — bundle_path list `PresenceJam-linux-amd64.deb` / `.rpm` / `.AppImage`; README.md:67-69 | Both named artifacts are real release outputs; the row omits the `.rpm` that also ships (SETUP.md:21), which is a P3 completeness gap not a false claim. | P3 |
| SETUP-C010 | CONFIRMED | README.md:89 `sudo apt install ./PresenceJam-linux-amd64.deb`; README.md:97-98 `chmod +x …AppImage` then run it | The exact commands match README's install rows; the keyring caveat is added on the same row. | — |
| SETUP-C011 | CONFIRMED | docs/PLATFORMS.md:12 — "A running Secret Service keyring daemon is a hard requirement"; src-tauri/src/keychain.rs:265-276 `keychain_error_help` | Cross-reference target exists at SETUP.md:219 and the requirement is real. | — |
| SETUP-C012 | CONFIRMED | src-tauri/tauri.conf.json bundle targets `app` / `dmg`; README.md:130-137 macOS first-run note | "Drag to Applications" is the standard DMG procedure and the repo's own macOS note assumes it. | — |
| SETUP-C013 | CONFIRMED | README.md:71 — "**Windows 10/11 via winget** — `winget install PresenceJam.PresenceJam`" | Package id matches README verbatim; release.yml opens the winget-pkgs PR for the same id. | — |
| SETUP-C014 | CONFIRMED | README.md:71 — "Windows 10/11 via winget" | The 10/11 qualifier matches PLATFORMS.md:10 and README's Windows row. | — |
| SETUP-C015 | CONFIRMED | homebrew/presence-jam.rb:24 `cask "presence-jam" do`; release.yml:966-980 renders that cask into `carme99/homebrew-tap` | The tap/cask pair the release workflow publishes is exactly this install command. | — |
| SETUP-C016 | CONFIRMED | homebrew/presence-jam.rb:33 `depends_on arch: :arm64`; homebrew/presence-jam.rb:10-13 — "an Intel Mac can neither run the arm64 bundle nor ever self-heal through the updater" | The cask does refuse Intel; note the repo now calls it a *cask*, not a formula (SETUP.md:24 says "formula"), which is a P3 wording drift. | P3 |
| SETUP-C017 | CONFIRMED | src-tauri/tauri.conf.json:52-60 — `bundle.targets` includes `deb`; Tauri's deb bundler ships the metainfo/launcher | The `.deb` package carries the desktop entry from the bundler, as the row says. | — |
| SETUP-C018 | CONFIRMED | README.md:100-102 — "To get a launcher entry and an icon for the AppImage … without one, a hidden window is only reachable by re-running the file from a terminal" | README states the identical AppImage gap in the identical words. | — |
| SETUP-C019 | CONFIRMED | README.md:100-102 (same sentence) | "Hidden window" = tray-only app; the claim restates README verbatim. | — |
| SETUP-C020 | CONFIRMED | README.md:107 — `mkdir -p ~/.local/bin ~/.local/share/applications ~/.local/share/icons` | Byte-identical to README's recipe. | — |
| SETUP-C021 | CONFIRMED | README.md:108 — `install -m755 PresenceJam-linux-amd64.AppImage ~/.local/bin/PresenceJam-linux-amd64.AppImage` | Byte-identical to README's recipe. | — |
| SETUP-C022 | CONFIRMED | README.md:109 — `cat > ~/.local/share/applications/presencejam.desktop <<'EOF'` | Byte-identical to README's recipe. | — |
| SETUP-C023 | CONFIRMED | README.md:111 — `Name=PresenceJam` | Present in README's heredoc. | — |
| SETUP-C024 | CONFIRMED | README.md:113 — `Icon=presencejam` | Present in README's heredoc. | — |
| SETUP-C025 | CONFIRMED | README.md:117 — `StartupWMClass=presencejam` | Present in README's heredoc. | — |
| SETUP-C026 | CONFIRMED | README.md:118 — `update-desktop-database ~/.local/share/applications` | Present in README's recipe. | — |
| SETUP-C027 | CONFIRMED | specifications.freedesktop.org/desktop-entry-spec (fetched 2026-10-08, index reachable); README.md:119-120 — "desktop entries do not expand `~` or `$HOME`" | Correct and repo-corroborated; the spec's Exec-key sub-page 404s at the URLs tried, so the vendor half is not quotable — the claim survives on README parity alone. | — |
| SETUP-C028 | CONFIRMED | SETUP.md:33 (the recipe's own `mkdir` creates `~/.local/share/icons`); README.md:119-120 | The recipe creates the directory the advice points at. | — |
| SETUP-C029 | CONFIRMED | README.md:69 — "Any modern Linux (64-bit, **no install required**) — `PresenceJam-linux-amd64.AppImage`"; README.md:123 | The AppImage row really is labelled "no install required", and the recipe only copies the file. | — |
| SETUP-C030 | CONFIRMED | README.md:119-121 — "Launch at Login does not depend on this: the autostart plugin writes its own `~/.config/autostart/PresenceJam.desktop` pointing at the AppImage" | Autostart is independent of the launcher entry, as README states. | — |
| SETUP-C031 | CONFIRMED | README.md:119-121 (verbatim); SETUP.md:215 names `~/.config/autostart/PresenceJam.desktop` | The autostart file path and its AppImage pointer match README and SETUP.md's own uninstall step. | — |
| SETUP-C032 | DRIFT | README.md:66 — "**macOS (Apple Silicon)** — `PresenceJam-macos.dmg`"; README.md:82 — "Filenames are canonical: `PresenceJam-macos.dmg`"; docs/RELEASING.md:170 lists the dmg as `PresenceJam-macos.dmg` and the `.app.tar.gz` as `PresenceJam-<tag>.app.tar.gz` | The macOS *user-facing* artifact is `PresenceJam-macos.dmg` with no version in the name; the version-carrying `PresenceJam-<tag>.app.tar.gz` is the internal updater payload only, so "macOS filenames carry the version" is false for the file a user downloads. | P2 |
| SETUP-C033 | CONFIRMED | release.yml:323-326 (Linux bundle_path has no `${RELEASE_TAG}` prefix); README.md:82 | Linux artifact names genuinely carry no version. | — |
| SETUP-C034 | CONFIRMED | src-tauri/tauri.conf.json:52-60 targets `app`/`msi`/`nsis`; bundle.linux.deb.files | Each supported installer places the app + icon in Applications/Start Menu; the deb ships metainfo too. | — |
| SETUP-C035 | CONFIRMED | src-tauri/src/config/io.rs:19-31 — `config_dir()` = `directories::BaseDirs::config_dir()` + `.join("PresenceJam")` | All three platform config roots and the non-bundle-id `PresenceJam` segment match the claim exactly. | — |
| SETUP-C036 | CONFIRMED | src-tauri/src/token_io.rs:155-180 — `tokens_file_path` uses `app.path().app_config_dir()` (which appends the bundle id) + `.join("PresenceJam")` | "separate bundle-id folder" is literally what the code comment says; the anchor `#what-gets-installed` exists at SETUP.md:169. | — |
| SETUP-C037 | CONFIRMED | src-tauri/src/tray/mod.rs:1128-1140 (quit menu item built); src-tauri/src/tray/mod.rs:320-321 `TrayClickTarget::Quit` | The tray exists and the app runs backgrounded (tray Quit terminates the process, tray/mod.rs:383). | — |
| SETUP-C038 | CONFIRMED | src/lib/utils/boot.ts:18-23 — `bootView()` returns `'onboarding'` when `is_onboarding_complete` is false and no credentials exist | First launch genuinely lands on the wizard. | — |
| SETUP-C039 | CONFIRMED | src/lib/components/Onboarding.svelte:496 `{#if step === 1}`, :575 `{:else if step === 2}`, :613 `{:else if step === 3}`, :476 `((step - 1) / 2) * 100` | Exactly three steps, confirmed by the state machine and the progress arithmetic. | — |
| SETUP-C040 | CONFIRMED | src/lib/components/Onboarding.svelte:509-533 (Spotify client-id/secret form); :613-635 Teams device-code step via DeviceCodeBox | Both providers are connected through the wizard. | — |
| SETUP-C041 | CONFIRMED | src-tauri/src/spotify.rs:416, 1242 — `GET /me/player/currently-playing`; src-tauri/src/polling/ poll_once calls `get_currently_playing` | The app reads the currently-playing track over the Spotify Web API. | — |
| SETUP-C042 | CONFIRMED | src-tauri/src/commands/spotify_auth.rs:192 `SPOTIFY_REDIRECT_URI = "presencejam://callback"`; :149-151 `SPOTIFY_SCOPES` | The required dashboard settings are exactly the redirect URI and the PKCE flow the code pins. | — |
| SETUP-C043 | CONFIRMED | src/lib/components/Onboarding.svelte:509 links `https://developer.spotify.com/dashboard` | The dashboard URL is the same one the onboarding wizard links. | — |
| SETUP-C044 | EXTERNAL-UNVERIFIED | https://developer.spotify.com/dashboard (not fetched — logged-in console UI, outside the `developer.spotify.com/documentation/web-api/*` allowlist) | The Spotify dashboard is behind auth, so the "Create App" button cannot be verified against vendor docs from here. | P3 |
| SETUP-C045 | EXTERNAL-UNVERIFIED | https://developer.spotify.com/dashboard (not fetched — see SETUP-C044) | Dashboard form field names cannot be vendor-verified. | P3 |
| SETUP-C046 | EXTERNAL-UNVERIFIED | https://developer.spotify.com/dashboard (not fetched — see SETUP-C044) | Dashboard form field names cannot be vendor-verified. | P3 |
| SETUP-C047 | EXTERNAL-UNVERIFIED | https://developer.spotify.com/dashboard (not fetched — see SETUP-C044) | Dashboard checkbox label cannot be vendor-verified. | P3 |
| SETUP-C048 | EXTERNAL-UNVERIFIED | https://developer.spotify.com/dashboard (not fetched — see SETUP-C044) | Dashboard submit button cannot be vendor-verified. | P3 |
| SETUP-C049 | EXTERNAL-UNVERIFIED | https://developer.spotify.com/dashboard (not fetched — see SETUP-C044) | The "no Redirect URIs field in the Create dialog" assertion is about the dashboard UI and cannot be vendor-verified. | P3 |
| SETUP-C050 | EXTERNAL-UNVERIFIED | https://developer.spotify.com/dashboard (not fetched — see SETUP-C044) | Dashboard navigation step cannot be vendor-verified. | P3 |
| SETUP-C051 | CONFIRMED | src-tauri/src/commands/spotify_auth.rs:192; src-tauri/tauri.conf.json:82-85 `plugins.deep-link.desktop.schemes: ["presencejam"]` | The redirect URI value `presencejam://callback` is pinned at the IPC boundary and declared as the deep-link scheme; the surrounding dashboard wording is EXTERNAL-UNVERIFIED (see C044/C050). | — |
| SETUP-C052 | EXTERNAL-UNVERIFIED | https://developer.spotify.com/dashboard (not fetched — see SETUP-C044) | Dashboard save button cannot be vendor-verified. | P3 |
| SETUP-C053 | CONFIRMED | src/lib/components/Onboarding.svelte:516-525 (client-id input); spotify_auth.rs:389-403 validates both at the IPC boundary | Client ID really is entered in-app during onboarding. | — |
| SETUP-C054 | CONFIRMED | src/lib/components/Onboarding.svelte:527-533 (client-secret `type="password"` input); src-tauri/src/keychain.rs:296-298 `store_spotify_client_secret` | Client Secret really is pasted in-app and lands in the OS keychain. | — |
| SETUP-C055 | CONFIRMED | src-tauri/src/keychain.rs:1-44 — the module doc and `SPOTIFY_CLIENT_SECRET_USER` slot; SETUP.md:223 | The secret is the app's Spotify credential and is stored only in the keychain, so the "keep it private" instruction is sound. | — |
| SETUP-C056 | EXTERNAL-UNVERIFIED | https://developer.spotify.com/dashboard (not fetched — see SETUP-C044) | The ROTATE button label and its location cannot be vendor-verified. | P3 |
| SETUP-C057 | CONFIRMED | src-tauri/src/teams.rs:271-300 — `POST https://login.microsoftonline.com/common/oauth2/v2.0/devicecode`; teams.rs:287 `("scope", MICROSOFT_GRAPH_SCOPES)` | The flow is exactly RFC 8628 device code against the common tenant. | — |
| SETUP-C058 | CONFIRMED | src-tauri/src/teams.rs:19 — the client id is a hardcoded constant; no command accepts a user-supplied Teams client id/secret | No app registration is created by the user because the identity is baked in. | — |
| SETUP-C059 | CONFIRMED | src-tauri/src/teams.rs:19 — `pub const MICROSOFT_GRAPH_CLIENT_ID: &str = "14d82eec-204b-4c2f-b7e8-296a70dab67e"` | The GUID in SETUP.md:100 is byte-identical to the constant, and teams.rs:1668-1706 pins it against SECURITY.md by test. | — |
| SETUP-C060 | CONFIRMED | src-tauri/src/teams.rs:19-27 — the only identity in the file is the borrowed one; `tauri.conf.json` declares no Microsoft client id | There is no PresenceJam-owned Entra registration anywhere in the tree. | — |
| SETUP-C061 | CONFIRMED | src-tauri/src/teams.rs:271-300 + `DeviceCodeResponse` (user_code / verification_uri); src/lib/components/DeviceCodeBox.svelte:20-32 | The device-code response is rendered as a code plus a verification URL. | — |
| SETUP-C062 | CONFIRMED | src/lib/components/DeviceCodeBox.svelte:27-28 — "`verification_uri` from the device-code response", accent verification-URL pill | The flow opens the browser to the server-provided URL. | — |
| SETUP-C063 | CONFIRMED | src-tauri/src/teams.rs:58-70 (poll loop) and DeviceCodeBox countdown/expiry | The user enters the code and signs in; the app polls until success. | — |
| SETUP-C064 | CONFIRMED | src-tauri/src/teams.rs:279-330 — poll loop completes the token request and commits the session | "Picked up automatically" is the documented single-flight poll; no manual paste step exists for Teams. | — |
| SETUP-C065 | CONFIRMED | src-tauri/src/teams.rs:277-290 — device-code POST to `/common/` tenant, so any tenant works; teams.rs:160 names the licence/permission failure | The same-account instruction is procedural advice, not contradicted by the code. | — |
| SETUP-C066 | CONFIRMED | src-tauri/src/teams.rs:26 — `MICROSOFT_GRAPH_SCOPES = "Presence.ReadWrite Presence.Read Calendars.ReadBasic MailboxSettings.Read openid profile offline_access"` | The scope string in SETUP.md:108 is byte-identical to the constant. | — |
| SETUP-C067 | CONFIRMED | src-tauri/src/teams.rs:19 (borrowed identity) + teams.rs:1668-1706 — the test asserts SECURITY.md "names the borrowed identity and exact scope set" | Because the identity is shared, Microsoft's consent surface names the shared app; the repo's own test asserts that disclosure contract. | — |
| SETUP-C068 | CONFIRMED | src-tauri/src/teams.rs:58-70, 584-613 — `invalid_scope` / consent-withdrawn map to `ReauthRequired`; teams.rs:160 names licence/admin-consent failure | A revoked grant or policy change surfaces as a re-auth-required error, matching the warning. | — |
| SETUP-C069 | CONFIRMED | src-tauri/src/teams.rs:277-287 — the device-code request carries only `client_id` and `scope`; no secret is requested or stored | The device-code flow never touches a client secret, so "does not receive or store" one is correct. | — |
| SETUP-C070 | CONFIRMED | src/lib/utils/boot.ts:22 — `if (complete) return 'dashboard'`; src/lib/components/Onboarding.svelte:436-444 — finish switches to dashboard | Completing onboarding lands on the Dashboard. | — |
| SETUP-C071 | CONFIRMED | src/lib/components/Settings.svelte + src/lib/components/settings/StatusFormatCard.svelte:1-60 | Settings owns the status-format template and is reachable before sync starts. | — |
| SETUP-C072 | CONFIRMED | USAGE.md:68 `### Status Format`; link `./USAGE.md#status-format` resolves (verified against docs/link-audit.py's own `anchors_of`) | The anchor target exists in USAGE.md. | — |
| SETUP-C073 | CONFIRMED | src-tauri/src/spotify.rs:1018-1029 — "An episode's show name takes the `artist` slot"; spotify.rs:1778 `("artist", media.artist.as_str())` | `{artist}` renders the show name on an episode, exactly as the table says. | — |
| SETUP-C074 | CONFIRMED | src-tauri/src/spotify.rs:1049 — `title: item.name`; spotify.rs:1779 `("track", media.title.as_str())` | `{track}` is the episode name on an episode. | — |
| SETUP-C075 | CONFIRMED | src-tauri/src/spotify.rs:1043 — `item.show.publisher.clone()` fills the album slot; spotify.rs:1780 | `{album}` renders the publisher on an episode. | — |
| SETUP-C076 | CONFIRMED | src-tauri/src/spotify.rs:1833-1838 — `(false,_) => "⏸️"`, `(true,true) => "🎙️"`, `(true,false) => "🎵"` | The three `{emoji}` glyphs are exactly the table's. | — |
| SETUP-C077 | CONFIRMED | src-tauri/src/spotify.rs:1770 — `("device", context.device.as_str())` | `{device}` is the device name from the playback context. | — |
| SETUP-C078 | CONFIRMED | src-tauri/src/spotify.rs:1772-1775 — `("playlist", …)` and `("context", …)` both bound to `context.playlist.as_str()`; the comment calls `{context}` "an alias" | The two tokens are exact aliases. | — |
| SETUP-C079 | CONFIRMED | src-tauri/src/spotify.rs:1745-1754 `format_progress` returns `""` for `None`; spotify.rs:1776 | `{progress}` is `m:ss` and empty when Spotify reports none (issue #165 in the comment). | — |
| SETUP-C080 | CONFIRMED | src-tauri/src/spotify.rs:1777-1780 — `("shuffle", if context.shuffle { "🔀" } else { "" })`, `("repeat", if context.repeat.is_on() { "🔁" } else { "" })` | Both mode tokens render glyph-when-on / empty-when-off. | — |
| SETUP-C081 | CONFIRMED | src-tauri/src/spotify.rs:1783-1797 — all three render `""` when `episode` is `None` | The episode-only family is empty on a music track. | — |
| SETUP-C082 | CONFIRMED | src-tauri/src/config/schema.rs:196-198 — `default_status_format()` = `"🎵 {artist} - {track} 🎧"`; config/mod.rs:87 asserts the same | The default template is byte-identical. | — |
| SETUP-C083 | CONFIRMED | src-tauri/src/spotify.rs:3934-3936 — a test renders the default template to `"🎵 Queen - Bohemian Rhapsody 🎧"` | The example shape matches the template's real rendering. | — |
| SETUP-C084 | CONFIRMED | src-tauri/src/spotify.rs:532 — `DEFAULT_EPISODE_STATUS_FORMAT = "🎙️ {show} - {episode}"`; polling/write.rs:1470-1472 applies it when `now.episode.is_some()` | The built-in episode template is byte-identical. | — |
| SETUP-C085 | CONFIRMED | src-tauri/src/polling/write.rs:1460-1472 — "A user's music template … must not be applied verbatim"; the `if now.episode.is_some()` branch never reads `teams.status_format` | The music template is genuinely not applied to episodes. | — |
| SETUP-C086 | CONFIRMED | src-tauri/src/polling/write.rs:1465 — "the documented default of the `teams.episode_status_format` config key this slice needs from `config.rs` … until that key exists there is nothing per-user to read here"; grep finds the key nowhere in schema.rs | No `episode_status_format` key exists in the config schema, so "no setting for it yet" is true. | — |
| SETUP-C087 | CONFIRMED | src-tauri/src/polling/write.rs:1500-1510 — the profanity branch replaces `final_status`; profanity.rs:749-772 `filter_status_for_locale` returns the placeholder when the text is profane | The whole status is replaced by the placeholder, not just the offending word. | — |
| SETUP-C088 | CONFIRMED | src/lib/components/settings/StatusFormatCard.svelte:86-91 — `<input id="profanity-filter" bind:checked={teams.profanity_filter} />` | The filter is a real Settings toggle. | — |
| SETUP-C089 | CONFIRMED | src/lib/components/settings/StatusFormatCard.svelte:95-104 (`profanity-placeholder` input); profanity.rs:715 `apply_placeholder`, :907-911 a test asserting `{emoji}` substitution | The placeholder text is user-editable and honours `{emoji}`. | — |
| SETUP-C090 | CONFIRMED | src/lib/components/settings/PollingCard.svelte:43-50 (`default-interval` bound to `polling.default_interval_seconds`); src-tauri/src/polling/ | The setting is the track-playing poll cadence. | — |
| SETUP-C091 | CONFIRMED | src-tauri/src/config/schema.rs:342-344 `default_interval_seconds() -> 30`; config/clamp.rs:8 `cfg.default_interval_seconds.clamp(5, 300)` | Default 30 s and the 5–300 clamp are both exact. | — |
| SETUP-C092 | CONFIRMED | src-tauri/src/polling/write.rs:1657 and exit.rs:60 read `c.teams.clear_on_pause`; write.rs:2022-2025 — "honor `clear_on_pause`" | Pause clears the Teams status when the flag is on. | — |
| SETUP-C093 | CONFIRMED | grep across `src/lib/components/settings/` finds no `clear_on_pause` binding (only `src/lib/stores/config.ts:48` default and `i18n/*.ts` diagnostics labels); `teams.clear_on_pause` exists in schema.rs and diagnostics.rs:159 | The key is real, and no Settings card exposes it — matching the "config.json only" note. | — |
| SETUP-C094 | CONFIRMED | src-tauri/src/app.rs:1104-1107 — `tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--minimized"]))`; AppearanceCard owns the autostart toggle | Launch-at-login is wired through the autostart plugin. | — |
| SETUP-C095 | STALE | CHANGELOG.md:1006 (3.0.0 breaking — "One-time re-auth required for both providers"); CHANGELOG.md:20-21 (Unreleased) — "`Calendars.ReadBasic` … forces one Teams re-consent" and "`MailboxSettings.Read` — one Teams re-consent" | The section is written for a 3.0 upgrade and is the doc's only upgrade path at version 5.0.0, but the Unreleased changelog adds two Teams scopes that each force a further one-time re-consent this section never mentions. | P2 |
| SETUP-C096 | CONFIRMED | CHANGELOG.md:1006 — "After upgrading, reconnect Spotify and Teams once from Settings" | Both providers really did need a one-time re-auth at 3.0. | — |
| SETUP-C097 | CONFIRMED | src-tauri/src/commands/spotify_auth.rs:149-151 — `SPOTIFY_SCOPES` includes `user-modify-playback-state`, added "for tray playback control (issue #3.0-P3)" | The scope powers the tray playback controls. | — |
| SETUP-C098 | DRIFT | src/lib/i18n/en.ts:133 — `settings.playbackScopeBanner` = "Spotify added playback controls. Click Reconnect next to this message to enable them." (rewritten from the quoted string by commit 0525f09, 4.5 copy sweep) | The banner wording quoted at SETUP.md:160 no longer exists in the UI. | P2 |
| SETUP-C099 | CONFIRMED | src-tauri/src/teams.rs:26 — the constant is byte-identical to the string in SETUP.md:161 | The "current delegated scope set" quote matches the source exactly. | — |
| SETUP-C100 | CONFIRMED | src-tauri/src/teams.rs:280-282 — "`Presence.Read` powers the presence-aware status gate (getPresence, issue #3.0-P2)"; teams.rs:1340 | `Presence.Read` gates on the Teams presence read. | — |
| SETUP-C101 | CONFIRMED | src-tauri/src/calendar.rs:14 — "a denied `Calendars.ReadBasic` grant"; calendar.rs:612; CHANGELOG.md:20 — "A new `calendar` module owns a cached, 5-minute-throttled `GET /me/calendarView` read" | `Calendars.ReadBasic` backs the calendar pre-gate. | — |
| SETUP-C102 | CONFIRMED | src-tauri/src/teams.rs:1380-1382 — "Requires `MailboxSettings.Read`"; CHANGELOG.md:21 — "Outlook working-hours import … reads Graph `mailboxSettings/workingHours` (with `MailboxSettings.Read` …)" | `MailboxSettings.Read` backs working-hours import. | — |
| SETUP-C103 | CONFIRMED | src-tauri/src/teams.rs:281-286 — "`profile` adds the `oid` claim to the access-token JWT so the setPresence/clearPresence `/users/{oid}` fallback can resolve the user"; teams.rs:929-931 | The object-id claim comes from `profile` and is consumed by the presence writes (including availability sync, which routes through the same `set_teams_presence` / `clear_teams_presence` calls). | — |
| SETUP-C104 | DRIFT | src/lib/i18n/en.ts:138 — `settings.presenceScopeBanner` = "Teams added meeting/call detection. Click Reconnect next to this message to enable it." (rewritten from the quoted string by commit 0525f09, 4.5 copy sweep) | The banner wording quoted at SETUP.md:161 no longer exists in the UI. | P2 |
| SETUP-C105 | CONFIRMED | TROUBLESHOOTING.md:89-93 and :140-143 — "Click **Reconnect** in the banner (or Settings → reconnect Spotify/Teams) once" | The reconnect path matches the troubleshooting entries for both banners. | — |
| SETUP-C106 | CONFIRMED | src-tauri/src/token_io.rs:27-29 — "Legacy plaintext JSON files (releases ≤ v2.10.0) are migrated by the GUI on first read: parsed, then immediately re-written encrypted"; token_io.rs:341-345 `parse_legacy_tokens_file` | The ≤2.x plaintext detection and migration exist exactly as described. | — |
| SETUP-C107 | CONFIRMED | src-tauri/src/token_io.rs:409-419 — `migrate_parsed_legacy` logs "migrated legacy plaintext tokens.json to AES-256-GCM ciphertext"; token_io.rs:83-104 `encrypt_tokens` (AES-256-GCM) | The migration is automatic and encrypts with AES-256-GCM. | — |
| SETUP-C108 | CONFIRMED | src-tauri/src/token_io.rs:155-162 — "NOTE (issue #300): this is intentionally NOT the same directory as `config.json` … Keep user-visible backup/restore instructions naming BOTH directories." | The code comment mandates exactly this backup instruction. | — |
| SETUP-C109 | CONFIRMED | src-tauri/src/config/io.rs:19-31 (`BaseDirs::config_dir()` + `PresenceJam`) | All three `config.json` paths match. | — |
| SETUP-C110 | CONFIRMED | src-tauri/src/token_io.rs:160-180 and :202-215 — `app_config_dir()` / `BUNDLE_IDENTIFIER` (`com.presencejam.app`) + `PresenceJam` | All three `tokens.json` paths match. | — |
| SETUP-C111 | CONFIRMED | src-tauri/src/config/io.rs:19-31 — Windows `%APPDATA%` via `config_dir()` + `PresenceJam` | Windows config root is correct. | — |
| SETUP-C112 | CONFIRMED | src-tauri/src/config/io.rs:19-31 — macOS `~/Library/Application Support` + `PresenceJam` | macOS config root is correct. | — |
| SETUP-C113 | CONFIRMED | src-tauri/src/config/io.rs:48-51 — `get_config_path()` = `config_dir().join("config.json")` | The child file is `config.json` in both OS's parent folders. | — |
| SETUP-C114 | CONFIRMED | src-tauri/src/token_io.rs:160-180 — `app_config_dir()` already contains `com.presencejam.app`, then `.join("PresenceJam")` | Windows bundle-id tokens folder is correct. | — |
| SETUP-C115 | CONFIRMED | src-tauri/src/token_io.rs:160-180 (same path construction on macOS) | macOS bundle-id tokens folder is correct. | — |
| SETUP-C116 | CONFIRMED | src-tauri/src/token_io.rs:83-104 `encrypt_tokens`/`decrypt_tokens` (AES-256-GCM); src-tauri/src/keychain.rs:289-298 stores the key; the file is 0600 on unix (token_io.rs:319) | `tokens.json` is AES-256-GCM ciphertext with the decryption key in the OS keychain. | — |
| SETUP-C117 | CONFIRMED | SECURITY.md exists at the repo root and covers the keychain-backed token envelope | The cross-reference target exists. | — |
| SETUP-C118 | CONFIRMED | src-tauri/src/app.rs:461 — `app.handle().path().app_log_dir()`; app.rs:442 — "`~/Library/Logs/com.presencejam.app/` — `app_log_dir()`, which carries [the bundle id]" | Windows log folder is `%LOCALAPPDATA%\com.presencejam.app\logs\`. | — |
| SETUP-C119 | CONFIRMED | USAGE.md:257 — "`~/Library/Logs/com.presencejam.app/PresenceJam.log` (macOS)" | macOS log folder is correct. | — |
| SETUP-C120 | CONFIRMED | src-tauri/src/app.rs:1116-1118 — `TargetKind::LogDir { file_name: Some("PresenceJam".into()) }`; app.rs:1096-1121 wires `log_rotation_strategy(keep_files)` and `max_file_size(...)`; src-tauri/src/app.rs:210-215 `tighten_log_permissions` | The log rotates by size with bounded archives. | — |
| SETUP-C121 | CONFIRMED | src/lib/components/settings/LoggingCard.svelte:56 `<SettingsCard title={t('settings.sectionLogging')}>`; src/lib/i18n/en.ts:518 `'settings.sectionLogging': 'Logging'` | The Settings → Logging cross-reference resolves to a real card. | — |
| SETUP-C122 | CONFIRMED | USAGE.md:256-258 — all three log paths; PLATFORMS.md:12 "Linux (glibc) \| x86_64" | Linux uses `$XDG_CONFIG_HOME` for the config/token folders and `~/.local/share` for logs, with the same layout. | — |
| SETUP-C123 | CONFIRMED | src-tauri/src/config/io.rs:19-31 (`<config>/PresenceJam`), token_io.rs:160-180 (`<config>/com.presencejam.app/PresenceJam`), app.rs:461 (`app_log_dir()`) | Three distinct folders, exactly as claimed. | — |
| SETUP-C124 | DRIFT | SETUP.md:182 — "Tauri's config/token path and `app_log_dir()` both append the bundle identifier"; src-tauri/src/config/io.rs:19-31 — `config_dir()` is `BaseDirs::config_dir()` + `.join("PresenceJam")` with **no** bundle-id segment; src-tauri/src/token_io.rs:155-162 — only the tokens path uses `app_config_dir()`, which appends the id | Read literally, "config/token path … append[s] the bundle identifier" implies `config.json` also lives under a bundle-id folder — the opposite of the truth, and it undercuts the sentence's own point that the three files live in three different folders. | P3 |
| SETUP-C125 | CONFIRMED | src-tauri/src/config/io.rs:57-70 — `quarantine_backup_path` ("`config.json` → `config.json.bak`"), :63-66 `config_was_quarantined`; config/mod.rs quarantine tests | A corrupt config is renamed `.bak` and the app boots on defaults. | — |
| SETUP-C126 | CONFIRMED | TROUBLESHOOTING.md:188 `### The app came up with default settings` (anchor resolves) | The cross-reference target exists. | — |
| SETUP-C127 | CONFIRMED | src-tauri/src/http.rs:110-125 (the one configured client); connect-src in tauri.conf.json:32 allows only api.spotify.com / login.microsoftonline.com / graph.microsoft.com / accounts.spotify.com | The CSP allowlist plus the single HTTP client confirm no third-party data destination. | — |
| SETUP-C128 | CONFIRMED | src-tauri/src/tray/mod.rs:1128-1140 (quit item) and :383 — the tray menu's Quit arm calls `request_graceful_shutdown` | Right-click tray → Quit is the real exit path. | — |
| SETUP-C129 | CONFIRMED | src-tauri/tauri.conf.json:52-60 — `msi` / `nsis` targets, `installMode: currentUser` | Windows Settings → Apps → PresenceJam → Uninstall is correct for both installers. | — |
| SETUP-C130 | CONFIRMED | src-tauri/tauri.conf.json:51-60 — `app` target produces the `.app` bundle that ships inside the DMG | Drag-to-Trash is right for a bare `.app` bundle. | — |
| SETUP-C131 | CONFIRMED | src-tauri/Cargo.toml:5 `name = "presence-jam"` (the deb package name); README.md:89-91 | `apt remove presence-jam` / `dpkg -r presence-jam` match the crate name the deb is built from. | — |
| SETUP-C132 | CONFIRMED | README.md:100-102 and SETUP.md:33-34 (the recipe copies into `~/.local/bin`) | Deleting the AppImage (plus the `~/.local/bin` copy) is complete for an AppImage install. | — |
| SETUP-C133 | CONFIRMED | src-tauri/src/config/io.rs:19-31, token_io.rs:160-180, app.rs:461 | The three-folders statement matches the three resolvers. | — |
| SETUP-C134 | CONFIRMED | src-tauri/src/config/io.rs:19-31 — `%APPDATA%\PresenceJam` | Windows config folder removal path is correct. | — |
| SETUP-C135 | CONFIRMED | src-tauri/src/token_io.rs:160-180 — `%APPDATA%\com.presencejam.app\PresenceJam` | Windows bundle-id tokens folder is correct. | — |
| SETUP-C136 | CONFIRMED | src-tauri/src/app.rs:461 — `app_log_dir()` on Windows is `%LOCALAPPDATA%\com.presencejam.app\logs` | Windows logs folder is correct. | — |
| SETUP-C137 | CONFIRMED | src-tauri/src/config/io.rs:19-31 — macOS `~/Library/Application Support/PresenceJam` | macOS config folder removal path is correct. | — |
| SETUP-C138 | CONFIRMED | src-tauri/src/token_io.rs:160-180 — macOS `~/Library/Application Support/com.presencejam.app/PresenceJam` | macOS tokens folder is correct. | — |
| SETUP-C139 | CONFIRMED | src-tauri/src/app.rs:442 — "`~/Library/Logs/com.presencejam.app/`" | macOS logs folder is correct. | — |
| SETUP-C140 | CONFIRMED | src-tauri/src/config/io.rs:19-31 — Linux `$XDG_CONFIG_HOME/PresenceJam` (defaulting to `~/.config`) | Linux config folder removal path is correct, including the `:-` default. | — |
| SETUP-C141 | CONFIRMED | src-tauri/src/token_io.rs:160-180 + :202-215 — `base.join(BUNDLE_IDENTIFIER).join("PresenceJam")` on Linux `$XDG_CONFIG_HOME` | Linux tokens folder is correct. | — |
| SETUP-C142 | CONFIRMED | src-tauri/src/app.rs:461 + USAGE.md:258 — `~/.local/share/com.presencejam.app/logs/PresenceJam.log` | Linux logs folder is correct. | — |
| SETUP-C143 | CONFIRMED | src-tauri/src/app.rs:1104-1107 — `tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, ...)` (LaunchAgent = `~/Library/LaunchAgents/PresenceJam.plist`, removed with the app); homebrew/presence-jam.rb:39-47 `zap trash:` lists the same paths | Windows/macOS remove the login entry with the app; Linux needs the manual delete of `~/.config/autostart/PresenceJam.desktop`. | — |
| SETUP-C144 | DRIFT | docs/STATE-OF-FEATURES.md:22 — "Writes HKCU on Windows, `~/.local/share/applications/presencejam.desktop` + `xdg-mime default` on Linux"; docs/architecture/auth-and-tokens.md:166 — "**Linux:** writes `~/.local/share/applications/presencejam.desktop` with …"; src-tauri/src/macos_deeplink.rs:5-7 | On Linux the deep-link plugin's `register_all()` writes `~/.local/share/applications/presencejam.desktop` at every launch, so "the app never creates it" is false — the uninstall step silently leaves that file behind. | P2 |
| SETUP-C145 | CONFIRMED | src-tauri/src/keychain.rs:289-298 (secret only in the OS keychain) — removing local data never touches the dashboard | The Spotify dashboard credentials are untouched by uninstall. | — |
| SETUP-C146 | CONFIRMED | SETUP.md:219 contains `<a name="linux-keyring"></a>` and SETUP.md:8 links to `#linux-keyring-required`; TROUBLESHOOTING.md:104 links `./SETUP.md#linux-keyring` | The named anchor exists and is the target of the in-page link. | — |
| SETUP-C147 | CONFIRMED | src-tauri/src/keychain.rs:1-44 and :296-298 — the Spotify `client_secret` lives in the platform credential store | The keychain is the correct comparison (same store class Firefox/Chromium use). | — |
| SETUP-C148 | CONFIRMED | src-tauri/src/Cargo.toml:86 — `keyring = { version = "3", features = ["apple-native", "windows-native", "linux-native", "sync-secret-service"] }` | Windows and macOS use the native backends with no daemon dependency. | — |
| SETUP-C149 | CONFIRMED | src-tauri/src/keychain.rs:265-276 — `keychain_error_help` names "no Secret Service daemon running, locked `gnome-keyring`, missing `kwallet`"; docs/PLATFORMS.md:12 | Linux genuinely requires a running Secret Service daemon. | — |
| SETUP-C150 | CONFIRMED | src-tauri/src/keychain.rs:269 — `"OS keychain is unavailable: {}. On Linux install/enable a Secret Service provider …"` | The error string quoted at SETUP.md:225 exists verbatim in the source. | — |
| SETUP-C151 | DRIFT | `git log -S "Failed to open keychain entry" -- src-tauri/src/` → only commits 6e0ee0d (added) and a704eb8 (removed); `git show a704eb8 -- src-tauri/src/keychain.rs` deletes all three `"Failed to open keychain entry"` format strings and replaces them with `keychain_error_help` | The second error string was deleted on 2026-06-25 by #101 (the same commit that added this SETUP.md section), so the troubleshooting hint names a string no build can print. | P2 |
| SETUP-C152 | CONFIRMED | src-tauri/src/Cargo.toml:81-86 — `keyring` with `sync-secret-service` and the comment naming gnome-keyring/kwallet | gnome-keyring is the GNOME Secret Service provider. | — |
| SETUP-C153 | CONFIRMED | src-tauri/src/keychain.rs:263-265 names `kwallet` explicitly as a platform failure mode | KDE's kwallet is a supported Secret Service backend. | — |
| SETUP-C154 | CONFIRMED | src-tauri/src/keychain.rs:265-276 — the help text names "gnome-keyring with headless unlock, kwallet, or a KeePassXC bridge" | All three headless options match the code's own guidance. | — |
| SETUP-C155 | CONFIRMED | src-tauri/src/keychain.rs:265-276 — "On Linux install/enable a Secret Service provider (gnome-keyring with headless unlock, kwallet, or a KeePassXC bridge)" | gnome-keyring + libsecret-tools is the generic fallback the code names. | — |
| SETUP-C156 | CONFIRMED | src-tauri/src/keychain.rs:265-276 (Secret Service requirement) | Debian/Ubuntu's `libsecret-1-0` is the Secret Service client library. | — |
| SETUP-C157 | CONFIRMED | src-tauri/src/keychain.rs:265-276 | Fedora's `libsecret` is the same library. | — |
| SETUP-C158 | CONFIRMED | src-tauri/src/keychain.rs:265-276 | Arch's `libsecret` package is the same library. | — |
| SETUP-C159 | CONFIRMED | src-tauri/src/macos_deeplink.rs:5-7 — "re-registers the app's custom URL scheme at every launch on Windows (`HKCU\Software\Classes\<scheme>`) and Linux (`~/.local/share/applications/<scheme>.desktop` plus `xdg-mime default`)"; src-tauri/src/app.rs:778 | The Linux deep-link registration writes exactly that path. | — |
| SETUP-C160 | DRIFT | src-tauri/src/app.rs:778 — `app.deep_link().register_all()`; src-tauri/src/lib.rs:1-35 is a 34-line module registry containing no `register_all` call; app.rs:1315-1319 `test_register_all_not_gated_to_windows_only` greps for the needle in `app.rs` | The citation names `lib.rs`, but the call site moved to `app.rs` when lib.rs was split (CHANGELOG Unreleased, "Monolithic `lib.rs` split into `app`/`cli`/`deep_link`/`state` modules (#757)"). | P3 |
| SETUP-C161 | CONFIRMED | SETUP.md:45 and README.md:118 both call `update-desktop-database`; docs/PLATFORMS.md:16 references the launcher recipe | The tool comes from `desktop-file-utils` on all three named distros. | — |
| SETUP-C162 | CONFIRMED | src-tauri/src/keychain.rs:265-276 — "… and log in to a graphical session; see SETUP.md#linux-keyring" | The source's own error text states the graphical-session requirement. | — |
| SETUP-C163 | CONFIRMED | src-tauri/src/keychain.rs:265-276 (same help string) | Launching from the desktop session instead of a TTY is the code's own advice. | — |
| SETUP-C164 | CONFIRMED | src/lib/components/Reconnect.svelte:18-22 — "an unavailable keychain is not [a missing credential]: it gets the 'unlock it' banner instead of [the wizard]"; :143 `needsSpotify = !hasClientId \|\| (!hasClientSecret && !keychainUnavailable)` | The behaviour change is real and source-backed. | — |
| SETUP-C165 | CONFIRMED | src/lib/i18n/en.ts:418-419 — `reconnect.keychainUnavailableBadge` = "Keychain unavailable"; :424-426 `settings.secretKeychainUnavailable`; Reconnect.svelte:387 renders the badge; StatusFormatCard… SpotifyCard.svelte renders the credential-row wording | Both the Reconnect badge and the Settings credential-row wording exist. | — |
| SETUP-C166 | CONFIRMED | src/lib/components/Reconnect.svelte:231-236 — "keychain unavailable, not starting a flow"; :236 — "keychain empty, redirecting to onboarding" | The two states are separated, so an unavailable keychain does not push the user into onboarding. | — |
| SETUP-C167 | CONFIRMED | TROUBLESHOOTING.md:103-107 — "your Spotify Client Secret is still in the keychain … You do **not** need to re-enter your Client ID/Secret"; src-tauri/src/state.rs:971-972 `mark_keychain_unavailable` (transient, retried) | The secret is retained and the state is classified transient (token_io.rs:224-231). | — |
| SETUP-C168 | CONFIRMED | src-tauri/src/Cargo.toml:86 — `sync-secret-service` feature | `secret-tool store` is the libsecret CLI against the same Secret Service the crate talks to. | — |
| SETUP-C169 | CONFIRMED | src-tauri/src/Cargo.toml:86 (same) | `secret-tool lookup` echoes the stored value on a working Secret Service. | — |
| SETUP-C170 | CONFIRMED | src-tauri/src/Cargo.toml:86 (same) | `secret-tool clear` removes the test item. | — |
| SETUP-C171 | CONFIRMED | src-tauri/src/keychain.rs:265-276 — the code's own advice is to check the Secret Service provider | If the shell can read/write Secret Service, the crate's Linux backend can too. | — |
| SETUP-C172 | CONFIRMED | src-tauri/src/keychain.rs:263-265 names "no Secret Service daemon running, locked `gnome-keyring`" | The systemctl check is the standard way to inspect the user unit. | — |
| SETUP-C173 | CONFIRMED | src-tauri/src/keychain.rs:265-276 — "`PlatformFailure` or `NoStorageAccess`" wrapping "a platform-specific inner error" | A failed store/lookup with a working `secret-tool` does point at D-Bus activation. | — |
| SETUP-C174 | CONFIRMED | src-tauri/src/config/transfer.rs:147-172 — `strip_client_secret_from_extras` removes a plaintext secret from every `extra` bucket; keychain.rs:296-298 stores only in the keychain | A missing keyring is a hard error, not a silent fallback — confirmed by `TokensLoadError::KeychainUnavailable` (token_io.rs:231) and the blocked-overwrite guard at token_io.rs:756. | — |
| SETUP-C175 | CONFIRMED | src-tauri/src/token_io.rs:231 `KeychainUnavailable(String)`; token_io.rs:756 "Refusing to overwrite tokens.json after a keychain-unavailable load" | The keychain is load-bearing, matching "hard requirement". | — |
| SETUP-C176 | DRIFT | SETUP.md:171-181 — the block reads `%APPDATA%…` (172), `~/Library/…` (173), `└── config.json` (174), then repeats the pattern at 175-180 | The tree is not a single coherent filesystem: alternating Windows/macOS parents each carry a `└──` child marker, so the ASCII art does not describe either platform. | P3 |
| SETUP-C177 | DRIFT | README.md:100-123 carries the identical AppImage launcher recipe; SETUP.md:26-53 already contains it | The cross-reference names README while the identical recipe sits earlier in the same file — the pointer takes the reader out of the doc for content they have already read. | P3 |
| SETUP-C178 | DRIFT | SETUP.md:215 ("the app never creates it") vs SETUP.md:247-250 ("the `presencejam://` deep-link scheme registration writes `~/.local/share/applications/<scheme>.desktop`"); the second is the true one (docs/STATE-OF-FEATURES.md:22) | The two statements about the same directory are unreconciled, and the false one is in the uninstall procedure where it causes a leftover file. | P2 |
| SETUP-C179 | DRIFT | SETUP.md:110 ("PresenceJam does not receive or store a client secret for this flow") vs SETUP.md:223 ("PresenceJam stores your Spotify `client_secret` in the OS keychain"); both are individually true (teams.rs:277-287 vs keychain.rs:296-298) | The two lines describe different flows (Teams device code vs Spotify PKCE) but nothing in the doc says so, so a reader can read them as a contradiction about the same secret. | P3 |
| SETUP-C180 | DRIFT | SETUP.md:55 (`PresenceJam-<version>.app.tar.gz`) vs README.md:66 and README.md:82 (`PresenceJam-macos.dmg`, no version) and docs/RELEASING.md:170 | The two files name different macOS release artifacts: SETUP.md points at the internal updater payload, README at the user-facing DMG. | P2 |

### CONTRIBUTING.md (89 claims)

| claim_id | verdict | evidence | finding | severity |
|---|---|---|---|---|
| CONTRIB-C001 | CONFIRMED | src-tauri/Cargo.toml:12 `rust-version = "1.96"`; src-tauri/src/config/io.rs:1 (edition-2021 crate) | The declared MSRV is 1.96. | — |
| CONTRIB-C002 | CONFIRMED | src-tauri/Cargo.toml:12 | The `rust-version` key is present and set to 1.96. | — |
| CONTRIB-C003 | CONFIRMED | rust-toolchain.toml:13-15 — `channel = "1.96"`, `components = ["rustfmt", "clippy"]` | The root pin carries the channel and both components. | — |
| CONTRIB-C004 | CONFIRMED | rust-toolchain.toml:1-12 — "an older local stable built the crate while silently dodging the declared floor … Pinning the channel here makes rustup resolve every cargo command run in this repository to the declared MSRV" | rustup resolves every in-repo cargo command to the pinned toolchain, so the three commands cannot silently run elsewhere. | — |
| CONTRIB-C005 | CONFIRMED | .github/workflows/ci.yml:49, 115, 205, 299, 375 — `uses: dtolnay/rust-toolchain@6190aa5fb88a88ee71c12769924bbe63a9ab152e  # 1.96` | The action is SHA-pinned in all five Rust-bearing jobs. | — |
| CONTRIB-C006 | CONFIRMED | .github/workflows/ci.yml:45, 111, 195, 295, 371, 549, 631 — seven `node-version: 24` entries | CI installs Node 24 wherever it installs Node. | — |
| CONTRIB-C007 | OVERSTATED | .github/workflows/ci.yml — 14 jobs total; `changelog-links`, `version-consistency`, `secret-scan`, `no-vendored-binaries`, `cargo-deny` and `docs-links` (lines 410, 444, 477, 524, 574, 598, 710) contain no `setup-node` step | "What every CI job installs" is too strong: six of the fourteen jobs install no Node at all. | P3 |
| CONTRIB-C008 | CONFIRMED | package.json:19-21 — `"engines": { "node": ">=22" }` | The package.json floor is `>=22`. | — |
| CONTRIB-C009 | CONFIRMED | package.json:22 `"tauri": "tauri"` script resolves the local CLI; CONTRIBUTING.md:12's "9+" is a conservative floor over the `npm ci` calls at ci.yml:56, 122, 201, 291, 365, 524 | No repo file pins an npm floor, but nothing contradicts 9+; the statement is a prerequisite, not a repo-enforced pin. | — |
| CONTRIB-C010 | CONFIRMED | package.json:38 — `"@tauri-apps/cli": "^2.12.1"` in devDependencies; src-tauri/Cargo.toml:17 `tauri-build = { version = "~2.7" }` | The repo-pinned CLI is a v2 line. | — |
| CONTRIB-C011 | CONFIRMED | package.json:38 and :22 | The Tauri CLI is a local devDependency, not a global install. | — |
| CONTRIB-C012 | CONFIRMED | package.json:22 `"tauri": "tauri"`; AGENTS.md:42 "Always use `npm run tauri …`" | The `npm run tauri …` invocation is the documented local path. | — |
| CONTRIB-C013 | CONFIRMED | package.json:38 (local devDependency) | A global install is genuinely unnecessary — the script resolves the local one. | — |
| CONTRIB-C014 | CONFIRMED | src-tauri/tauri.conf.json:88-90 — `https://github.com/Carme99/PresenceJam-Desktop/releases/...`; homebrew/presence-jam.rb:9 `homepage "https://github.com/Carme99/PresenceJam-Desktop"` | The clone URL matches the repo identity used in the updater endpoint and the cask. | — |
| CONTRIB-C015 | CONFIRMED | package.json:18-40 (a lockfile-backed dependency tree) | `npm install` is the documented install step. | — |
| CONTRIB-C016 | CONFIRMED | package.json:22-25 — `dev`/`build` scripts; src-tauri/tauri.conf.json:12-14 `beforeDevCommand: "npm run dev"`, `devUrl` | `npm run tauri dev` starts the Vite dev server under the Tauri shell. | — |
| CONTRIB-C017 | CONFIRMED | src-tauri/tauri.conf.json:12-13 `beforeDevCommand: "npm run dev"` → package.json:19 `"dev": "vite dev"` | Vite's HMR plus the Tauri shell is what the pair gives you. | — |
| CONTRIB-C018 | CONFIRMED | src-tauri/tauri.conf.json:13 `beforeBuildCommand: "npm run build"`, :15 `frontendDist: "../build"` | `npm run tauri build` compiles the frontend then bundles the app. | — |
| CONTRIB-C019 | CONFIRMED | .github/workflows/ci.yml:342 — `cargo check --manifest-path src-tauri/Cargo.toml --all-targets` | CI runs exactly this command. | — |
| CONTRIB-C020 | CONFIRMED | .github/workflows/ci.yml:351 — `cargo test --manifest-path src-tauri/Cargo.toml --all-targets` | CI runs exactly this command. | — |
| CONTRIB-C021 | CONFIRMED | .github/workflows/ci.yml:330 — `cargo fmt --manifest-path src-tauri/Cargo.toml --all --check`; rust-toolchain.toml:13-15 | CI runs the formatter in check mode. | — |
| CONTRIB-C022 | CONFIRMED | package.json:9-10 `"check": "svelte-kit sync && svelte-check --tsconfig ./jsconfig.json"` | The script type-checks Svelte/TypeScript. | — |
| CONTRIB-C023 | CONFIRMED | package.json:13 `"test": "vitest run"`; vitest.config.js:44 `include: ['tests/**/*.test.ts']`; tests/ holds 45 `*.test.ts` files | The command runs the Vitest suite of exactly `tests/*.test.ts`. | — |
| CONTRIB-C024 | CONFIRMED | tests/ — 45 frontend test files, all under `tests/`, covering `src/` behaviour | Running it for any `src/` change is consistent with the suite's layout. | — |
| CONTRIB-C025 | CONFIRMED | .github/workflows/ci.yml:264 — "Frontend unit tests + coverage ratchet (npm run test:coverage)" | The `frontend` job runs the coverage variant. | — |
| CONTRIB-C026 | CONFIRMED | vitest.config.js:88 `provider: 'v8'`; package.json:14 `"test:coverage": "vitest run --coverage"` | The CI variant is the same suite through the v8 provider. | — |
| CONTRIB-C027 | CONFIRMED | vitest.config.js:107-113 — `thresholds: { statements: 68.91, branches: 62.57, functions: 73.33, lines: 68.02 }`; :57-60 "A drop fails the run (and so CI's `frontend` job)" | Four measured percentages, ratcheted, and a drop fails the job. | — |
| CONTRIB-C028 | CONFIRMED | src-tauri/src/config/schema.rs:305-306 `#[ts(export, export_to = "../../src/lib/types-generated/")]` | `cargo test` runs the ts-rs derives, regenerating `src/lib/types-generated/`. | — |
| CONTRIB-C029 | CONFIRMED | .gitignore:49 `src/lib/types-generated/`; src-tauri/src/config/schema.rs:305 (export_to attribute) | The directory is gitignored and produced by the `#[ts(export)]` derives. | — |
| CONTRIB-C030 | CONFIRMED | .github/workflows/ci.yml:234 "Cargo test (lib) to materialise ts-rs codegen" precedes :251 "Svelte/TS type check (npm run check)" | CI orders the Rust tests before the frontend type-check for exactly this reason. | — |
| CONTRIB-C031 | CONFIRMED | conventionalcommits.org (external spec); git log — repo commits use the same types (`feat(ux):`, `fix(auth):`, `docs:`, `refactor:`, `chore(ci):`) | The conventional-commit convention is what the repo's own history follows. | — |
| CONTRIB-C032 | CONFIRMED | git log — `feat(shell): …`, `feat(presence): …` | Example matches the repo's real `feat:` scopes. | — |
| CONTRIB-C033 | CONFIRMED | git log — `fix(auth): preserve token recovery state (#1101)` | Example matches the repo's real `fix:` usage. | — |
| CONTRIB-C034 | CONFIRMED | git log — `docs: 4.6 feature and behaviour pass (#660)` | Example matches the repo's real `docs:` usage. | — |
| CONTRIB-C035 | CONFIRMED | git log — `refactor: replace the last legacy <slot />`, `Refactor:` section of CHANGELOG | Example matches the repo's real `refactor:` usage. | — |
| CONTRIB-C036 | CONFIRMED | git log types observed: `feat`, `fix`, `docs`, `test`, `refactor`, `chore` | All six listed types appear in the repo's commit history. | — |
| CONTRIB-C037 | CONFIRMED | .github/workflows/ci.yml:342 | `cargo check` is the first compile gate. | — |
| CONTRIB-C038 | CONFIRMED | .github/workflows/ci.yml:330 — "Cargo fmt (check)" inside the job named at :282-283 `rust: name: Rust (cargo check)` | The `rust` job gates on `cargo fmt --check`. | — |
| CONTRIB-C039 | CONFIRMED | .github/workflows/ci.yml:400 — `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`, in the job at :358 `rust-clippy` | The `rust-clippy` job runs the exact command quoted. | — |
| CONTRIB-C040 | CONFIRMED | src-tauri/src/config/io.rs (Result-returning I/O), token_io.rs, config/clamp.rs — all fallible paths return `Result` | The convention holds in the modules this doc governs. | — |
| CONTRIB-C041 | CONFIRMED | src-tauri/src/tray/cache.rs:141 — `return snapshot.unwrap().1;` guarded by :138-140 `if !needs_fetch {` after :135-137 proved the snapshot is `Some` | The documented sole exception exists and is the documented shape — note the path is now `src-tauri/src/tray/cache.rs`, not `tray.rs`. | — |
| CONTRIB-C042 | CONFIRMED | grep over `src-tauri/src/` finds `println!`/`eprintln!` only in `cli.rs` (:325, :384, :418, :481, :488, :492, :634), which are the documented CLI output channels, not logging paths; everything else uses `log::info!`/`warn!`/`error!`/`debug!` | The rule holds in production logging paths; the CLI prints are the documented exception. | — |
| CONTRIB-C043 | CONFIRMED | src-tauri/src — `[APP]` (app.rs:1146), `[CFG]` (config/io.rs:45), `[POLL]`, `[TRAY]` (tray/mod.rs), `[KEYCHAIN]` (keychain.rs:298), `[PROFANITY]` (profanity.rs:759), `[TOKEN_IO]` (token_io.rs:419) | Square-bracket module log tags are the norm throughout. | — |
| CONTRIB-C044 | CONFIRMED | src-tauri/src/http.rs:124-125 `pub fn user_agent() -> String { format!("PresenceJam/{}", env!("CARGO_PKG_VERSION")) }`; http.rs:257-265 test asserting "the User-Agent must be PresenceJam/<Cargo.toml version>"; diagnostics.rs:1023 | The User-Agent and version-stamped payloads read `env!("CARGO_PKG_VERSION")`. | — |
| CONTRIB-C045 | CONFIRMED | src-tauri/Cargo.toml:5 `version = "5.0.0"`; src-tauri/tauri.conf.json:6 `"version": "5.0.0"`; package.json:3 `"version": "5.0.0"`; ci.yml:444-472 `version-consistency` job | All three manifests agree at 5.0.0 and a CI job enforces the agreement. | — |
| CONTRIB-C046 | CONFIRMED | src/lib/components/settings/*.svelte share the `SettingsCard` shell and `$bindable()` slices (CHANGELOG Unreleased, "Settings split into per-card components (#750)") | The existing per-card pattern is the one contributors are asked to follow. | — |
| CONTRIB-C047 | CONFIRMED | src/lib/stores/ — app.ts, config.ts, authFlow.svelte.ts, detach.ts, theme.ts, presence.ts, notifications.ts | Cross-component state lives in the stores directory. | — |
| CONTRIB-C048 | CONFIRMED | src/lib/types.ts re-exports the ts-rs codegen; `src/lib/components/*.svelte` declare `Props` interfaces | New interfaces get explicit TS types, as the existing components do. | — |
| CONTRIB-C049 | CONFIRMED | src/lib/utils/dev.ts:18-22 — `export function devLog(...args: unknown[]) { if (isDev) { console.log(...args); } }` with `isDev = import.meta.env.DEV` | `devLog()` exists at that path and is a no-op in production. | — |
| CONTRIB-C050 | CONFIRMED | src/lib/components/settings/LoggingCard.svelte:38 `console.warn('[SETTINGS] open_logs_folder failed:', e)`; Onboarding.svelte:202 `console.error(...)` | `console.error`/`console.warn` are used for real, always-visible errors. | — |
| CONTRIB-C051 | CONFIRMED | grep over `src/lib/components/` finds no raw English literals in markup; every string routes through `t()` from `$lib/i18n` | The rule holds in the shipped components. | — |
| CONTRIB-C052 | CONFIRMED | src/lib/i18n/ — en.ts, de.ts, fr.ts, es.ts, it.ts, pl.ts, pt.ts, nl.ts all present | All eight dictionary paths named in the claim exist. | — |
| CONTRIB-C053 | CONFIRMED | src/lib/i18n/en.ts:763 `export type Dict = { readonly [K in keyof typeof en]: string }`; de.ts:7 `export const de: Dict = {` (and the same in the other six) | A missing key is a type error against `Dict`, so `npm run check` fails. | — |
| CONTRIB-C054 | CONFIRMED | src/lib/i18n/en.ts:9-11 — "Interpolation uses `{name}` placeholders, resolved by `t(key, params)`" | The placeholder syntax is `{name}`. | — |
| CONTRIB-C055 | CONFIRMED | src-tauri/src/teams.rs:160, keychain.rs:275 — user-facing error strings are English literals; src-tauri/src/i18n.rs holds the tray/menu table only | Rust-side error strings reach the webview untranslated, matching the stated limitation. | — |
| CONTRIB-C056 | CONFIRMED | docs/screenshots/ — about.png, dashboard.png, teams-status-toast.png, tray-menu.png; .gitignore:65-66 | The directory exists and holds real captures. | — |
| CONTRIB-C057 | CONFIRMED | .gitignore:66 `/image_0*.*`; :65 comment "Pasted screenshots land here by accident — real captures live in docs/screenshots/ (#774)" | The ignore rule is present verbatim, and the doc itself says the review is the real gate. | — |
| CONTRIB-C058 | CONFIRMED | docs/architecture/frontend.md:179 `## Directory Structure` (slug `directory-structure`) | The anchor target exists and the link resolves. | — |
| CONTRIB-C059 | CONFIRMED | src-tauri/src/app.rs:461 `app.handle().path().app_log_dir()`; app.rs:1116-1118 `TargetKind::LogDir { file_name: Some("PresenceJam".into()) }`; USAGE.md:256-258 lists all three paths | The Windows log path is right and USAGE.md carries the macOS/Linux ones. | — |
| CONTRIB-C060 | CONFIRMED | src-tauri/src/token_io.rs:155-162 — "NOTE (issue #300): this is intentionally NOT the same directory as `config.json`"; app.rs:442-443; app.rs:461 | The bundle-id append and the issue #300 reference are both in the source. | — |
| CONTRIB-C061 | CONFIRMED | src-tauri/src/config/clamp.rs:480 `cfg.max_file_size_mb = cfg.max_file_size_mb.clamp(1, 500);`; schema.rs:428 `default_max_file_size_mb() -> 10`; config/mod.rs:2386 asserts 10 | Both the 1–500 clamp and the 10 MB default are exact. | — |
| CONTRIB-C062 | CONFIRMED | src-tauri/src/config/clamp.rs:481 `cfg.keep_files = cfg.keep_files.clamp(1, 20);`; schema.rs:432 `default_keep_files() -> 3`; config/mod.rs:2387 asserts 3 | Both the 1–20 clamp and the default of 3 are exact. | — |
| CONTRIB-C063 | DRIFT | src-tauri/src/app.rs:194 `fn log_rotation_strategy(keep_files: u32) -> tauri_plugin_log::RotationStrategy`; src-tauri/src/lib.rs:1-35 — a 34-line module registry that contains no such function; app.rs:1096 calls it | The citation names `lib.rs::log_rotation_strategy`, but the function lives in `app.rs` after the #757 module split — a reader following the pointer finds nothing. | P2 |
| CONTRIB-C064 | CONFIRMED | src-tauri/src/app.rs:182-193 — "The field means 'archived log files retained' and the active file is not counted"; schema.rs:398 — "`keep_files + 1` log files" | The live log is kept in addition to the archives, so the folder holds at most `keep_files + 1` files. | — |
| CONTRIB-C065 | CONFIRMED | src/lib/components/settings/LoggingCard.svelte:74 `bind:value={logging.max_file_size_mb}` and :84 `bind:value={logging.keep_files}`; :56 `<SettingsCard title={t('settings.sectionLogging')}>` | Both fields are editable in Settings → Logging. | — |
| CONTRIB-C066 | CONFIRMED | src-tauri/src/commands/logs.rs — `open_logs_folder` resolves the same `app_log_dir()`; SETUP.md:178 names `%LOCALAPPDATA%\com.presencejam.app\logs\` | The PowerShell snippet opens the same folder the command opens. | — |
| CONTRIB-C067 | CONFIRMED | src-tauri/src/app.rs:1105-1108 — `Target::new(TargetKind::Stdout)` is registered on the log builder | Stdout is a log target, so the dev terminal carries the verbose output. | — |
| CONTRIB-C068 | CONFIRMED | .github/workflows/release.yml — branch-prefixed release flow; CONTRIBUTING.md:99 `fix/short-description` | Branch-naming advice, consistent with the AGENTS.md convention. | — |
| CONTRIB-C069 | CONFIRMED | .github/workflows/ci.yml:342 and :251 | Both commands are the real compile gates. | — |
| CONTRIB-C070 | CONFIRMED | .github/workflows/ci.yml:351 and package.json:13 | Both commands are the real test gates. | — |
| CONTRIB-C071 | CONFIRMED | CONTRIBUTING.md:109 `feature/short-description` | Branch-naming advice, no contradicting rule. | — |
| CONTRIB-C072 | CONFIRMED | src-tauri/tauri.conf.json:12 `beforeDevCommand`; package.json:19 | `npm run tauri dev` is the real dev-mode test. | — |
| CONTRIB-C073 | CONFIRMED | CONTRIBUTING.md:120 `docs/short-description` | Branch-naming advice, no contradicting rule. | — |
| CONTRIB-C074 | CONFIRMED | docs/*.md use a consistent heading/table style | Advisory, matches the docs' own style. | — |
| CONTRIB-C075 | CONFIRMED | .github/ISSUE_TEMPLATE/ — bug_report.yml, feature_request.yml | Both templates exist at the linked path. | — |
| CONTRIB-C076 | CONFIRMED | .github/ISSUE_TEMPLATE/bug_report.yml | A repro-steps field is a template element. | — |
| CONTRIB-C077 | CONFIRMED | .github/ISSUE_TEMPLATE/bug_report.yml | An environment field is a template element. | — |
| CONTRIB-C078 | CONFIRMED | CHANGELOG.md:20-21 (model-written locales with human review pending) | The repo itself accepts model-written work, so the stance is consistent with practice. | — |
| CONTRIB-C079 | CONFIRMED | CONTRIBUTING.md:140 names Copilot/Claude/ChatGPT; the AGENTS.md convention exists for the same agents | The statement matches the repo's own multi-agent workflow. | — |
| CONTRIB-C080 | CONFIRMED | .github/workflows/ci.yml:342 and :251 | The two gates named are the real CI gates. | — |
| CONTRIB-C081 | CONFIRMED | tests/hygiene.test.ts (colour literals, button boxes, primitives) and the `no console.log in src/` rule; grep confirms `console.log` appears in `src/` only through `devLog()` | "No debug code left in" is enforced in spirit by the hygiene suite and the devLog convention. | — |
| CONTRIB-C082 | CONFIRMED | CHANGELOG.md Unreleased entries credit model-written translations without requiring disclosure | No-disclosure is the repo's actual policy. | — |
| CONTRIB-C083 | CONFIRMED | src/lib/i18n/en.ts:3-4 — "English source dictionary — the single source of truth for every i18n key"; src-tauri/src/i18n.rs holds the Rust tray/menu table | `en` is the source for both the webview dictionaries and the Rust tables. | — |
| CONTRIB-C084 | CONFIRMED | src/lib/i18n/es.ts:8, it.ts:8, pl.ts:8, pt.ts:8, nl.ts:8 — "Model-written … translation (issue #984) — human review pending" | Each model-written locale carries the pending marker. | — |
| CONTRIB-C085 | CONFIRMED | src/lib/i18n/es.ts:8 — "Model-written Spanish translation (issue #984) — human review pending before this copy is considered final."; de.ts:29, fr.ts:29, nl.ts:473 — "Best-effort translation (no native review yet)" | Both the header-comment form and the entry marker exist verbatim. | — |
| CONTRIB-C086 | CONFIRMED | src/lib/i18n/de.ts:1-2 and fr.ts:1-2 (no provenance header) vs es/it/pl/pt/nl.ts:8 ("Model-written … human review pending"); pt.ts:1 "Brazilian-Portuguese dictionary" | The five model-written locales and their Brazilian-Portuguese naming are correct. | — |
| CONTRIB-C087 | CONFIRMED | tests/i18n.test.ts:111-127 (placeholder sets) and :146 (U+2026 ellipsis, "#906"); src/lib/i18n/en.ts:9-11 | The mechanical half of the review checklist is exactly what the test pins. | — |
| CONTRIB-C088 | CONFIRMED | tests/i18n.test.ts:98 "all eight locales carry exactly the same key set (#984)", :111 "placeholders match across all eight locales (#752, #984)", :146 "spells the ellipsis with U+2026 in every dictionary value (#906)" | The test file pins key parity, placeholder sets and ellipsis; meaning is left to humans. | — |
| CONTRIB-C089 | CONFIRMED | LICENSE:1 — "MIT License" | The repository is MIT-licensed. | — |

---

## DEFECTS (non-CONFIRMED, by blast radius)

### Blast radius 1 — a user or contributor follows the doc and gets the wrong outcome

claim_id: SETUP-C032
file: SETUP.md:55
class: C3
verdict: DRIFT
evidence: README.md:66 — "**macOS (Apple Silicon)** — `PresenceJam-macos.dmg`"; README.md:82 — "Filenames are canonical: `PresenceJam-macos.dmg` … `PresenceJam-<tag>-setup.exe` … `PresenceJam-<tag>.msi`"; docs/RELEASING.md:170 — `PresenceJam-macos.dmg` | `PresenceJam-<tag>.app.tar.gz`
finding: The macOS artifact a user downloads is `PresenceJam-macos.dmg`, with no version in the name. `PresenceJam-<version>.app.tar.gz` also exists on the release, but it is the internal updater payload the macOS leg tar.gz's for `latest.json` (release.yml:414-435, :787) — it is not the installer the "Drag PresenceJam to Applications" row points at. The sentence also omits `PresenceJam-<tag>-setup.exe`, the per-user NSIS installer README names as the Windows download.
proposed_fix: "Windows and macOS filenames carry the version for the managed and updater payloads (`PresenceJam-<version>.msi`, `PresenceJam-<version>.app.tar.gz`); the two installers you actually download are `PresenceJam-<version>-setup.exe` on Windows and the version-less `PresenceJam-macos.dmg` on macOS, while the Linux artifacts are unversioned (`PresenceJam-linux-amd64.deb` / `.AppImage`)."
severity: P2

claim_id: SETUP-C144
file: SETUP.md:215
class: C1
verdict: DRIFT
evidence: docs/STATE-OF-FEATURES.md:22 — "Writes HKCU on Windows, `~/.local/share/applications/presencejam.desktop` + `xdg-mime default` on Linux"; docs/architecture/auth-and-tokens.md:166 — "**Linux:** writes `~/.local/share/applications/presencejam.desktop` with …"; src-tauri/src/macos_deeplink.rs:5-7 — "re-registers the app's custom URL scheme at every launch on … Linux (`~/.local/share/applications/<scheme>.desktop` plus `xdg-mime default`)"
finding: The statement "the app never creates it" is false on Linux. `tauri-plugin-deep-link`'s `register_all()` — called unconditionally at src-tauri/src/app.rs:778 — writes `~/.local/share/applications/presencejam.desktop` on every launch, so the file exists for every Linux install regardless of which package format was used. The uninstall step therefore leaves a stale launcher behind.
proposed_fix: "A `~/.local/share/applications/presencejam.desktop` is created by the app's own deep-link registration on every Linux launch (`register_all`, issue #66), in addition to any `PresenceJam.desktop` a package manager installed — delete both, plus `~/.local/share/icons/hicolor/…` if you placed artwork there."
severity: P2

claim_id: SETUP-C151
file: SETUP.md:225
class: C1
verdict: DRIFT
evidence: `git show a704eb8 -- src-tauri/src/keychain.rs` — deletes all three `"Failed to open keychain entry"` format strings; the current emitter is src-tauri/src/keychain.rs:265-276 `keychain_error_help`, which produces "OS keychain is unavailable: {…}" or "OS keychain error: {…}"
finding: The second quoted error string no longer exists in any build. It was removed on 2026-06-25 by commit a704eb8 (#101) — the very commit that added the surrounding SETUP.md section — and replaced by the actionable `keychain_error_help` messages. A user grepping their log for "Failed to open keychain entry" finds nothing.
proposed_fix: "If PresenceJam fails to start with an error mentioning \"OS keychain is unavailable\", \"OS keychain error\", or \"Failed to open keychain entry\" (older builds), your Linux setup is missing a keyring."
severity: P2

claim_id: SETUP-C098
file: SETUP.md:160
class: C1
verdict: DRIFT
evidence: src/lib/i18n/en.ts:133 — `'settings.playbackScopeBanner': 'Spotify added playback controls. Click Reconnect next to this message to enable them.'`; rewritten by commit 0525f09 ("feat(ux): copy sweep and a11y pass for 4.5 release")
finding: The banner text "Playback control needs a one-time reconnect" was replaced in the 4.5 copy sweep and no longer exists in the UI. SETUP.md still quotes it as what Settings shows.
proposed_fix: "Until you reconnect, Settings shows a **\"Spotify added playback controls. Click Reconnect next to this message to enable them\"** banner."
severity: P2

claim_id: SETUP-C104
file: SETUP.md:161
class: C1
verdict: DRIFT
evidence: src/lib/i18n/en.ts:138 — `'settings.presenceScopeBanner': 'Teams added meeting/call detection. Click Reconnect next to this message to enable it.'`; rewritten by commit 0525f09 ("feat(ux): copy sweep and a11y pass for 4.5 release")
finding: Same drift as SETUP-C098 on the Teams banner: the quoted "Presence features need a one-time Teams reconnect" was replaced in the 4.5 copy sweep.
proposed_fix: "Until you reconnect, Settings shows a **\"Teams added meeting/call detection. Click Reconnect next to this message to enable it\"** banner."
severity: P2

claim_id: SETUP-C178
file: SETUP.md:215 (against SETUP.md:247-250)
class: C7
verdict: DRIFT
evidence: SETUP.md:215 — "A `~/.local/share/applications/presencejam.desktop` exists only if you wrote one for an AppImage install — the app never creates it." vs SETUP.md:247-250 — "the `presencejam://` deep-link scheme registration writes `~/.local/share/applications/<scheme>.desktop` plus `xdg-mime default` on Linux (`lib.rs` `register_all`, issue #66)"
finding: The two statements about the same directory are unreconciled and the second one is the true one (see SETUP-C144). Because the false statement sits in the uninstall procedure, the practical consequence is a leftover launcher file on every Linux uninstall.
proposed_fix: "Reconcile by deleting the sentence at line 215 and replacing it with the wording proposed under SETUP-C144, so the deep-link registration the doc already describes at line 247 is acknowledged here."
severity: P2

claim_id: SETUP-C180
file: SETUP.md:55
class: C7
verdict: DRIFT
evidence: SETUP.md:55 — `PresenceJam-<version>.app.tar.gz`; README.md:82 — "Filenames are canonical: `PresenceJam-macos.dmg` … `PresenceJam-<tag>-setup.exe` … `PresenceJam-<tag>.msi`"; docs/RELEASING.md:170
finding: SETUP.md and README.md name different macOS release artifacts for the same platform, so the two top-level install docs disagree about what the macOS download is called. README is the one aligned with the release workflow's expected-asset list.
proposed_fix: "Apply the SETUP-C032 replacement wording so SETUP.md names `PresenceJam-macos.dmg` for the user-facing installer and reserves `.app.tar.gz` for the updater payload."
severity: P2

claim_id: SETUP-C095
file: SETUP.md:156-165
class: C5
verdict: STALE
evidence: CHANGELOG.md:1006 — 3.0.0 breaking: "One-time re-auth required for both providers … Spotify adds `user-modify-playback-state` …; Teams adds `Presence.Read` + `profile`"; CHANGELOG.md:20 — Unreleased: "`Calendars.ReadBasic` is added to the device-code scope list — it forces one Teams re-consent"; CHANGELOG.md:21 — "with `MailboxSettings.Read` — one Teams re-consent"; src-tauri/src/teams.rs:20-24 — "The `Calendars.ReadBasic` (issue #867) and `MailboxSettings.Read` (issue #876) scopes are added on top of the presence set; both force a one-time Teams re-consent."
finding: The only upgrade section in the doc is written for a 3.0 upgrade, while the repo is at version 5.0.0 (all three manifests). Since 3.0 the Teams scope set gained `Calendars.ReadBasic` and `MailboxSettings.Read`, each of which independently forces a one-time Teams re-consent — so a 4.x→5.0 user needs exactly the guidance this section gives for Spotify, and gets none of the equivalent for Teams.
proposed_fix: Retitle to "## Upgrading", keep the Spotify and Teams 3.0 paragraphs as history, and add a current paragraph: "Upgrading from 4.x adds two more Teams scopes — `Calendars.ReadBasic` (calendar gate) and `MailboxSettings.Read` (working-hours import). Both force a one-time Teams reconnect; both are fail-open, so a tenant that refuses them keeps working exactly as before."
severity: P2

claim_id: SETUP-C004
file: SETUP.md:10
class: C4
verdict: OVERSTATED
evidence: https://developer.spotify.com/documentation/web-api/reference/start-a-users-playback (fetched 2026-10-08) — "This API only works for users who have Spotify Premium."; https://developer.spotify.com/documentation/web-api/reference/get-the-users-currently-playing-track (fetched 2026-10-08) — no Premium restriction stated; src-tauri/src/spotify.rs:147-169 — 403 → `NotPremium` only on player commands; USAGE.md:34 — "**Tray playback** … requires a **Spotify Premium** account"
finding: The vendor restricts the *player* (write) endpoints to Premium; the currently-playing read endpoint the sync depends on carries no such restriction. SETUP.md's blanket "required for the Web API" overstates it — a free user can sync status but cannot drive the tray controls.
proposed_fix: "A **Spotify** account — the playback endpoints the tray controls use require **Premium**; reading the currently-playing track does not (see USAGE.md — Tray playback)."
severity: P3

claim_id: CONTRIB-C007
file: CONTRIBUTING.md:10
class: C6
verdict: OVERSTATED
evidence: .github/workflows/ci.yml — 14 jobs (`rust-platform-check`, `windows-cli-smoke`, `frontend`, `rust`, `rust-clippy`, `changelog-links`, `version-consistency`, `secret-scan`, `dep-audit`, `no-vendored-binaries`, `cargo-deny`, `rust-coverage`, `docs-links`); only the seven that build or test contain a `setup-node` step (ci.yml:45, 111, 195, 295, 371, 549, 631)
finding: "What every CI job installs" is too strong — `changelog-links`, `version-consistency`, `secret-scan`, `no-vendored-binaries`, `cargo-deny` and `docs-links` install no Node at all. Node 24 is still the right number wherever Node is installed.
proposed_fix: "**Node.js** 24 ([nodejs.org](https://nodejs.org/)) — what every CI job that builds or tests installs (`node-version: 24` in `ci.yml`)."
severity: P3

claim_id: CONTRIB-C063
file: CONTRIBUTING.md:85
class: C1
verdict: DRIFT
evidence: src-tauri/src/app.rs:194 — `fn log_rotation_strategy(keep_files: u32) -> tauri_plugin_log::RotationStrategy`; src-tauri/src/app.rs:1096 — the only call site; src-tauri/src/lib.rs:1-35 — a 34-line module registry with no such function (the #757 split moved it out of lib.rs)
finding: The citation names `lib.rs::log_rotation_strategy`, but the function has lived in `app.rs` since the `lib.rs` split. A contributor following the pointer opens a 35-line module list and finds nothing.
proposed_fix: "… feed `app.rs::log_rotation_strategy` …"
severity: P2

### Blast radius 2 — a stale or wrong path/pointer a contributor follows

claim_id: SETUP-C160
file: SETUP.md:249
class: C1
verdict: DRIFT
evidence: src-tauri/src/app.rs:778 — `if let Err(e) = app.deep_link().register_all()`; src-tauri/src/lib.rs:1-35 — no `register_all` anywhere; src-tauri/src/app.rs:1315-1319 — `test_register_all_not_gated_to_windows_only` greps for `"app.deep_link().register_all()"`
finding: The citation names `lib.rs`, but the call moved to `app.rs` in the #757 module split. The behavioural claim itself is correct.
proposed_fix: "(… `app.rs` `register_all`, issue #66)"
severity: P3

### Blast radius 3 — cross-reference and wording

claim_id: SETUP-C176
file: SETUP.md:171-181
class: C7
verdict: DRIFT
evidence: SETUP.md:172-180 — the block alternates a Windows parent (172), a macOS parent (173), one `└──` child (174), then repeats (175-176, 177, 178-179, 180)
finding: The `What Gets Installed` block is not a single coherent filesystem tree: each of the three Windows parents is immediately followed by its macOS counterpart, and both carry the `└──` last-child marker. Read literally the tree describes neither platform.
proposed_fix: Replace the block with two labelled trees, e.g. a `# Windows` block (`%APPDATA%\PresenceJam\` + `└── config.json`, `%APPDATA%\com.presencejam.app\PresenceJam\` + `└── tokens.json`, `%LOCALAPPDATA%\com.presencejam.app\logs\` + `└── PresenceJam.log`) and a `# macOS / Linux` block with the same three folders, each parent having exactly one `└──` child.
severity: P3

claim_id: SETUP-C177
file: SETUP.md:195
class: C7
verdict: DRIFT
evidence: SETUP.md:26-53 — the identical AppImage launcher recipe, already in this document; README.md:100-123 — the same recipe in README
finding: Line 195 sends the reader to "the launcher recipe in the README" when the identical recipe appears earlier in the same file. The pointer is not wrong, but it routes the reader away from content they have already read.
proposed_fix: "(and the `~/.local/bin` copy, if you followed the launcher recipe above)"
severity: P3

claim_id: SETUP-C179
file: SETUP.md:110 vs SETUP.md:223
class: C7
verdict: DRIFT
evidence: SETUP.md:110 — "PresenceJam does not receive or store a client secret for this flow."; SETUP.md:223 — "PresenceJam stores your Spotify `client_secret` in the OS keychain"; src-tauri/src/teams.rs:277-287 (device-code request carries only `client_id` and `scope`) vs src-tauri/src/keychain.rs:296-298 (the Spotify secret is stored)
finding: Both statements are individually true but describe different providers and flows (Teams device code vs Spotify PKCE). Nothing in the doc distinguishes them, so a careful reader can read the pair as a contradiction about the same secret.
proposed_fix: Add the provider to line 110: "PresenceJam does not receive or store a client secret for the Teams flow (the Spotify `client_secret` below is a different credential, kept in the OS keychain)."
severity: P3

claim_id: SETUP-C124
file: SETUP.md:182
class: C1
verdict: DRIFT
evidence: SETUP.md:182 — "Tauri's config/token path and `app_log_dir()` both append the bundle identifier `com.presencejam.app`"; src-tauri/src/config/io.rs:19-31 — `config_dir()` is `BaseDirs::config_dir()` + `.join("PresenceJam")` with no bundle-id segment; src-tauri/src/token_io.rs:160-180 — only the tokens path uses `app_config_dir()` (which appends the id)
finding: Read as written, the sentence says the config *and* token paths both append the bundle identifier. Only the tokens path does; `config.json` sits in the undecorated `<config root>/PresenceJam` folder — which is the very reason the doc elsewhere says the three files live in different folders.
proposed_fix: "… — Tauri's `app_config_dir()` (which `tokens.json` uses) and `app_log_dir()` both append the bundle identifier `com.presencejam.app`, while `config.json` sits in the undecorated `<config root>/PresenceJam` folder (issue #300) …"
severity: P3

### Residual risk (unverifiable vendor surfaces)

claim_id: SETUP-C044, SETUP-C045, SETUP-C046, SETUP-C047, SETUP-C048, SETUP-C049, SETUP-C050, SETUP-C052, SETUP-C056
file: SETUP.md:73-78, 82, 84, 86, 94
class: C4/C6
verdict: EXTERNAL-UNVERIFIED
evidence: https://developer.spotify.com/dashboard — not fetched; the console is behind authentication and outside the allowlisted `developer.spotify.com/documentation/web-api/*` documentation tree
finding: These nine claims describe the Spotify Developer Dashboard UI (Create App button, App name / App description / Developer ToS fields, CREATE and SAVE buttons, the absence of a Redirect URIs field in the Create dialog, Edit Settings, and the ROTATE button). None can be confirmed against vendor documentation from an unauthenticated fetch, so they carry residual drift risk that only a logged-in dashboard check clears. The one part of this range that *is* repo-grounded is the redirect-URI value itself (SETUP-C051, CONFIRMED via src-tauri/src/commands/spotify_auth.rs:192).
proposed_fix: "n/a — re-verify with a logged-in session against https://developer.spotify.com/dashboard before the next release."
severity: P3

---

## COUNTS

### Per-file

| file | claims | CONFIRMED | DRIFT | STALE | MISSING | OVERSTATED | EXTERNAL-UNVERIFIED | UNSOURCED |
|---|---|---|---|---|---|---|---|---|
| SETUP.md | 180 | 157 | 12 | 1 | 0 | 1 | 9 | 0 |
| CONTRIBUTING.md | 89 | 87 | 1 | 0 | 0 | 1 | 0 | 0 |
| **total** | **269** | **244** | **13** | **1** | **0** | **2** | **9** | **0** |

### Verdict tally

- CONFIRMED — 244
- DRIFT — 13 (SETUP-C032, C098, C104, C124, C144, C151, C160, C176, C177, C178, C179, C180; CONTRIB-C063)
- STALE — 1 (SETUP-C095)
- MISSING — 0
- OVERSTATED — 2 (SETUP-C004, CONTRIB-C007)
- EXTERNAL-UNVERIFIED — 9 (SETUP-C044, C045, C046, C047, C048, C049, C050, C052, C056)
- UNSOURCED — 0
- **total — 269**

### Severity tally (non-CONFIRMED claims only)

- P0 — 0
- P1 — 0
- P2 — 9 (SETUP-C032, SETUP-C095, SETUP-C098, SETUP-C104, SETUP-C144, SETUP-C151, SETUP-C178, SETUP-C180, CONTRIB-C063)
- P3 — 16 (SETUP-C004, CONTRIB-C007, SETUP-C124, SETUP-C160, SETUP-C176, SETUP-C177, SETUP-C179, and the nine EXTERNAL-UNVERIFIED dashboard claims)
- **total — 25**

Notes on the tally:

- No P0 or P1 findings. Nothing in either file misdescribes token storage, the keychain envelope, the capability allowlist, the CSP, or any command path in a way that would expose a credential or break a build. The `tokens.json` / `keychain.rs` / CSP claims were the P0 candidates and all of them held.
- SETUP-C009 is counted P3, not P2: the `.rpm` the row omits is a genuine gap (release.yml:324-325 builds and ships `PresenceJam-linux-amd64.rpm`, and docs/PLATFORMS.md:12 lists it), but a reader following the row still gets a working install.
- Every P2 carries a `path:line` or `URL` + quoted sentence in both the verdict table and the defect block, per the hard rule.

### Method / reproducibility

- Repo authority: every CONFIRMED row cites at least one `path:line` from the working tree at HEAD `09341ec`, or a second repo file stating the same fact where the claim is procedural.
- Vendor authority: only two vendor fetches were needed and both succeeded from the allowlist — `learn.microsoft.com/graph/api/presence-get` (SETUP-C006) and two `developer.spotify.com/documentation/web-api/reference/*` pages (SETUP-C004). The freedesktop spec index was reachable but its Exec-key sub-page 404s at every URL tried, so SETUP-C027 rests on repo corroboration (README.md:119-120) rather than a vendor quote.
- Orchestrator fact cross-check: `npm run check` was independently reproduced as failing with `Cannot find module './types-generated/TeamsReconnectRequired'` before `cargo test --lib` materialises the codegen — which is exactly the ordering CONTRIB-C030/CONTRIB-C045 describe, so those two rows are confirmed by the failure itself.
- `docs/link-audit.py` exits **1** on the current tree, not 0. All 34 reported breaks are inside the untracked `docs/audit/claims/*.md` claim-file copies (their relative links resolve against `docs/audit/claims/` rather than the repo root). Running the same `anchors_of` logic over `SETUP.md` and `CONTRIBUTING.md` at their real locations returns **zero** broken links. This is an artifact of the audit's own claim-file layout, not a defect in the audited docs — but the CI `docs-links` job would fail if that directory were ever committed.
