# Docs-Grounding Audit — SECURITY.md

Repo: /home/jack/.openclaw/workspace/projects/PresenceJam-Desktop
HEAD: 09341ecaad732e78454a2c65b383f0dfd1541d5a (tree clean, versions 5.0.0, latest tag v4.7.0)
Claim file: docs/audit/claims/SECURITY.md (322 claims)
Verified: 2026-10-08
Evidence hierarchy applied: repo tree on disk > allowlisted vendor docs (learn.microsoft.com/graph/*, developer.spotify.com/documentation/web-api/*, v2.tauri.app) > tests > prose; CHANGELOG treated as a claim, never as a source.

## VERDICTS

Format: CLAIM_ID | VERDICT | evidence | one-sentence finding | severity

C001 | CONFIRMED | src-tauri/src/token_io.rs:53-58, src-tauri/src/keychain.rs:31,807 | The app does handle sensitive authentication credentials (OAuth tokens plus the Spotify client_secret) as the opening sentence states. | -
C002 | CONFIRMED | SECURITY.md:7 | The reporting section exists with the responsible-disclosure framing. | -
C003 | CONFIRMED | .github/workflows/ci.yml (secret-scan job exists), SECURITY.md:11 | A preferred private-reporting channel is offered as described. | -
C004 | CONFIRMED | SECURITY.md:12, git remote origin -> github.com/Carme99/PresenceJam-Desktop | The advisory URL names the correct owner/repo. | -
C005 | CONFIRMED | SECURITY.md:13 | The "Report a vulnerability" step is described. | -
C006 | CONFIRMED | SECURITY.md:14 | Private-advisory visibility is described as maintainer-only. | -
C007 | CONFIRMED | SECURITY.md:15 | A 7-day response expectation is stated. | -
C008 | CONFIRMED | SECURITY.md:17 | The alternative public-disclosure path is offered. | -
C009 | CONFIRMED | SECURITY.md:18 | The `security: ` title prefix plus `security` label are specified. | -
C010 | CONFIRMED | SECURITY.md:19 | The confidential-issue toggle is referenced. | -
C011 | CONFIRMED | SECURITY.md:20 | The "no sensitive details in the body" rule is stated. | -
C012 | CONFIRMED | SECURITY.md:22 | The "Do:" list is present. | -
C013 | CONFIRMED | SECURITY.md:23 | Repro instructions are requested. | -
C014 | CONFIRMED | SECURITY.md:24 | Waiting before public disclosure is requested. | -
C015 | CONFIRMED | SECURITY.md:25 | Affected-version inclusion is requested. | -
C016 | CONFIRMED | SECURITY.md:27 | The "Don't:" list is present. | -
C017 | CONFIRMED | SECURITY.md:28 | Filing a regular public issue is discouraged. | -
C018 | CONFIRMED | SECURITY.md:29 | Compensation demands are discouraged. | -
C019 | CONFIRMED | SECURITY.md:31 | The second statement of the response time reads "within 7 days" — identical to line 15, so the two are consistent. | -
C020 | CONFIRMED | SECURITY.md:15 and SECURITY.md:31 | Both statements say "within 7 days"; the duplication is faithful, not contradictory (see defect D-001 for the redundancy). | -
C021 | CONFIRMED | SECURITY.md:33 | The "Data Storage" section exists. | -
C022 | CONFIRMED | SECURITY.md:35 | The token table header exists as described. | -
C023 | CONFIRMED | src-tauri/src/token_io.rs:161-179, SECURITY.md:37 | Tokens live at `<app-config-dir>/PresenceJam/tokens.json` via token_io.rs. | -
C024 | CONFIRMED | src-tauri/src/token_io.rs:83-103 (issue #140), Cargo.toml aes-gcm | The file is AES-256-GCM ciphertext. | -
C025 | CONFIRMED | src-tauri/src/token_io.rs:63,69,72,75 | Magic `b"PJENC"`, version byte 0x01 and a 12-byte nonce match the documented layout exactly. | -
C026 | CONFIRMED | src-tauri/src/keychain.rs:807,930-945; Cargo.toml:86 keyring = 3 | A random 256-bit key is generated on first use and held in the OS keychain via `keyring`. | -
C027 | CONFIRMED | src-tauri/src/keychain.rs:807 | The namespaced slot is exactly `tokens_aes_key:com.presencejam.app`. | -
C028 | CONFIRMED | SECURITY.md:41-54 | The referenced "Encrypted tokens.json" section exists below. | -
C029 | CONFIRMED | src-tauri/src/teams_auth persist path (commands/teams_auth.rs:253,306), SECURITY.md:38 | Teams tokens land in the same file when persistence succeeds and otherwise stay in AppState until restart. | -
C030 | CONFIRMED | src-tauri/src/token_io.rs:613-616 | Teams tokens use the same AES-256-GCM write path as Spotify. | -
C031 | CONFIRMED | src-tauri/src/state.rs:11-21, src-tauri/src/token_io.rs:30-34 | Pending OAuth state (PKCE verifier + state) is in-memory only in `PendingSpotifyAuth` and never written to disk. | -
C032 | CONFIRMED | src-tauri/src/token_io.rs:27-34 | A killed process loses the pending state, so the user restarts the flow and Spotify issues a fresh code. | -
C033 | CONFIRMED | src-tauri/src/token_io.rs:13,16-25 | The v3.0/issue #140 encrypted-tokens.json statement is grounded in the module docs. | -
C034 | CONFIRMED | src-tauri/src/token_io.rs:83-103 | tokens.json holds AES-256-GCM ciphertext. | -
C035 | CONFIRMED | src-tauri/src/token_io.rs:63,69,72,97-101 | The on-disk format matches byte for byte. | -
C036 | CONFIRMED | src-tauri/src/token_io.rs:83,112 | `encrypt_tokens` and `decrypt_tokens` exist in token_io.rs. | -
C037 | CONFIRMED | src-tauri/src/token_io.rs:130-136 | An unknown version byte is rejected with an Err, never mis-decrypted. | -
C038 | CONFIRMED | src-tauri/src/keychain.rs:862-868,1034-1073 | The key is generated from the OS CSPRNG and stored base64 on first use. | -
C039 | CONFIRMED | src-tauri/src/keychain.rs:807 | The slot is bundle-identifier-namespaced as stated. | -
C040 | CONFIRMED | src-tauri/src/keychain.rs:973 | `get_or_create_tokens_aes_key` exists. | -
C041 | CONFIRMED | src-tauri/src/token_io.rs:141-149 | A tag mismatch fails decryption rather than returning garbage. | -
C042 | OVERSTATED | src-tauri/src/token_io.rs:633-694 vs 29-34, 395-407 | The write path is clean, but the doc never mentions that a headless `--status`/`--sync-once` read parses a legacy plaintext file WITHOUT migrating it, so a plaintext file can persist indefinitely through CLI-only use. | P2
C043 | CONFIRMED | src-tauri/src/token_io.rs:613-616 | `write_tokens_atomic` exists and is the atomic write entry point. | -
C044 | CONFIRMED | src-tauri/src/token_io.rs:27 | The plaintext→ciphertext migration is documented as v3.0/issue #140. | -
C045 | CONFIRMED | src-tauri/src/token_io.rs:4-11,27-34 | Pre-v2.6.4 used the plugin-store path and post-#65 used token_io.rs, both plaintext. | -
C046 | CONFIRMED | CHANGELOG.md:1184 | Issue #65 dropped `store:default` from capabilities and fixed the mid-write corruption, and added no encryption. | -
C047 | CONFIRMED | src-tauri/src/token_io.rs:370-407,505-558 | A leading `{` is parsed and immediately re-written encrypted, with stale plaintext sidecars swept. | -
C048 | CONFIRMED | src-tauri/src/token_io.rs:382-386 | A file that is neither PJENC nor `{`-prefixed is rejected. | -
C049 | CONFIRMED | src-tauri/src/token_io.rs:243-255,146-149; keychain.rs:923-924 | Missing key, corrupt ciphertext and tag mismatch all surface as Err and drive re-auth. | -
C050 | CONFIRMED | src-tauri/src/token_io.rs:293-296 (issue #135 path A comment) | No mode was set explicitly before v2.8.x. | -
C051 | CONFIRMED | src-tauri/src/app.rs:200-202 | The umask-022 premise is stated in the code comments. | -
C052 | CONFIRMED | src-tauri/src/app.rs:200-202, io.rs:457-459 | Under umask 022 the file landed at 0644, world-readable on macOS/Linux. | -
C053 | CONFIRMED | src-tauri/src/token_io.rs:658-660, app.rs:208 | On Windows the default ACL inherits user-only parent permissions. | -
C054 | CONFIRMED | src-tauri/src/token_io.rs:297-325, io.rs:457-461 | v2.8.x/issue #135 path A sets 0600 explicitly at write time. | -
C055 | CONFIRMED | src-tauri/src/token_io.rs:297-325 | A pre-existing loose file is tightened on first read. | -
C056 | CONFIRMED | src-tauri/src/token_io.rs:674-681, io.rs:648-655 | Windows needs no explicit ACL change (File::create inherits the user-only DACL). | -
C057 | CONFIRMED | SECURITY.md:96, 311 | Both referenced sections exist. | -
C058 | CONFIRMED | SECURITY.md:83-85 | Full-disk encryption is recommended as the strongest defense. | -
C059 | CONFIRMED | src-tauri/src/keychain.rs:807,31 | The keychain holds the tokens decryption key and the Spotify client_secret. | -
C060 | CONFIRMED | SECURITY.md:86-88 | Provider-side revocation is presented as the only post-compromise invalidation. | -
C061 | CONFIRMED | docs/architecture/storage-and-config.md:1-3 | The page exists and is described as the implementation reference for file locations, atomic writes and config integrity. | -
C062 | CONFIRMED | docs/architecture/overview.md:20,30 | overview.md names `config.rs::save_config()` -> `atomic_write_json()` and the 0600 modes. | -
C063 | CONFIRMED | SECURITY.md:96 | The file-permissions subsection exists. | -
C064 | CONFIRMED | src-tauri/src/token_io.rs:661-673, io.rs:636-644 | 0600 on Unix, user-only default ACL on Windows, for both files. | -
C065 | CONFIRMED | src-tauri/src/app.rs:210-282 (issue #920) | Log dir is 0700 and every `PresenceJam*.log*` file 0600 on Unix. | -
C066 | STALE | src-tauri/src/app.rs:210,454-479 vs lib.rs (33 lines of `pub mod` only) | The described behaviour is correct but the cited path `lib.rs::tighten_log_permissions` does not resolve — the function lives in app.rs. | P3
C067 | CONFIRMED | src-tauri/src/app.rs:209,460 | The log tightening is `#[cfg(unix)]`; Windows relies on the default ACL. | -
C068 | CONFIRMED | src-tauri/src/token_io.rs:654-673 | The temp file is created with mode(0o600) at creation, so ciphertext is never world-readable mid-write. | -
C069 | CONFIRMED | src-tauri/src/config/io.rs:636-644 | config.json stays plaintext; the 0600 mode is its only file-level protection. | -
C070 | OVERSTATED | src-tauri/src/polling/iteration.rs:588-593 | Track titles and artist names are logged only at `debug!` level, not unconditionally as the "the log holds ..." phrasing implies. | P3
C071 | CONFIRMED | SECURITY.md:111-112 | The claim/source table header exists. | -
C072 | CONFIRMED | src-tauri/src/token_io.rs:643-673 | The temp sidecar is `create_new(true).mode(0o600)` with ciphertext only. | -
C073 | CONFIRMED | src-tauri/src/token_io.rs:687 | POSIX rename preserves the source mode, so the live file inherits 0600. | -
C074 | CONFIRMED | src-tauri/src/config/io.rs:636-644 | config.json's temp sidecar is created with the same 0600 atomic pattern. | -
C075 | CONFIRMED | src-tauri/src/token_io.rs:297-325, io.rs:402-431,457-461 | Both readers tighten non-0600 files idempotently on read. | -
C076 | CONFIRMED | src-tauri/src/token_io.rs:646-653, io.rs:617-629 | A stale `.tmp` is removed before `create_new`, with NotFound tolerated. | -
C077 | CONFIRMED | src-tauri/src/token_io.rs:658-660, io.rs:648-650 | No explicit DACL change is made on Windows. | -
C078 | CONFIRMED | SECURITY.md:113-118 | The table cites live function paths for each row. | -
C079 | CONFIRMED | src-tauri/src/app.rs:210-282,454-479 | The log-permissions row matches the implementation. | -
C080 | CONFIRMED | SECURITY.md:121 | The "What this is NOT" paragraph exists. | -
C081 | CONFIRMED | src-tauri/src/config/io.rs:636-655 | File-mode narrowing does not encrypt config.json. | -
C082 | CONFIRMED | src-tauri/src/token_io.rs:613-616 | tokens.json is additionally encrypted. | -
C083 | CONFIRMED | SECURITY.md:125-126 | The full-disk-encryption recommendation is repeated. | -
C084 | CONFIRMED | SECURITY.md:127-129 | The provider-revocation statement is repeated. | -
C085 | CONFIRMED | src-tauri/src/token_io.rs:293-296 | The issue #135 Path A / Path B decision is reflected in the code comments. | -
C086 | CONFIRMED | src-tauri/src/token_io.rs:644-653, io.rs:617-629 | Path A was implemented without touching the atomic-write guarantees or adding a crypto dependency at that point. | -
C087 | CONFIRMED | src-tauri/src/token_io.rs:13,16-25 | Path B landed in v3.0 for tokens.json with a keychain-stored key. | -
C088 | CONFIRMED | src-tauri/src/config/schema.rs (no client_secret field), keychain.rs:296-331 | config.json holds no credentials; the client_secret lives in the keychain. | -
C089 | CONFIRMED | SECURITY.md:143 | The Configuration section exists. | -
C090 | CONFIRMED | SECURITY.md:145-153 | Two files are listed. | -
C091 | CONFIRMED | v2.tauri.app path reference (configDir -> {FOLDERID_RoamingAppData}), src-tauri/src/config/io.rs:19-51 | `%APPDATA%\PresenceJam\config.json` on Windows. | -
C092 | CONFIRMED | v2.tauri.app path reference, src-tauri/src/config/io.rs:19-51 | `~/Library/Application Support/PresenceJam/config.json` on macOS. | -
C093 | CONFIRMED | v2.tauri.app path reference, src-tauri/src/config/io.rs:19-51 | `$XDG_CONFIG_HOME/PresenceJam/config.json` on Linux, falling back to ~/.config. | -
C094 | CONFIRMED | src-tauri/src/token_io.rs:152-179, tauri.conf.json:5 | `%APPDATA%\com.presencejam.app\PresenceJam\tokens.json` — the bundle-id folder, per issue #300. | -
C095 | CONFIRMED | src-tauri/src/token_io.rs:152-179 | The macOS tokens path is correct. | -
C096 | CONFIRMED | src-tauri/src/token_io.rs:198-211 | The Linux tokens path is correct. | -
C097 | CONFIRMED | SECURITY.md:156-169 | The config.json contents list exists. | -
C098 | CONFIRMED | src-tauri/src/config/schema.rs (SpotifyConfig), SECURITY.md:158 | The Spotify Client ID is stored plaintext in config.json. | -
C099 | CONFIRMED | src-tauri/src/keychain.rs:31,296-331, schema.rs | The Client Secret is in the OS keychain, not config.json, as of v2.6.0. | -
C100 | CONFIRMED | src-tauri/src/config/migrate.rs:87-108,215-274 | Legacy plaintext configs are auto-migrated to the keychain and the plaintext is stripped on first run. | -
C101 | CONFIRMED | src-tauri/src/commands/spotify_auth.rs:154-186 | client_id must be exactly 32 alphanumeric chars; client_secret 32-512 (issues #67/#354). | -
C102 | CONFIRMED | src-tauri/src/commands/spotify_auth.rs:399-400,461,571 | Both values are validated at the IPC boundary before any keychain write or authorize URL. | -
C103 | CONFIRMED | src-tauri/src/config/schema.rs (status_format) | `status_format` is stored in config.json. | -
C104 | CONFIRMED | src-tauri/src/config/schema.rs (profanity_filter/profanity_placeholder) | The profanity settings are stored in config.json. | -
C105 | CONFIRMED | src-tauri/src/config/schema.rs (polling) | Polling configuration is stored in config.json. | -
C106 | CONFIRMED | src-tauri/src/config/schema.rs (logging) | Logging preferences are stored in config.json. | -
C107 | CONFIRMED | src-tauri/src/token_io.rs:50-58 | tokens.json holds a ciphertext envelope whose plaintext payload is the two token objects. | -
C108 | CONFIRMED | src-tauri/src/token_io.rs:54-56, spotify.rs (SpotifyTokens) | Spotify access + refresh tokens are stored as `SpotifyTokens`. | -
C109 | CONFIRMED | src-tauri/src/token_io.rs:56-57, teams.rs (TeamsTokens) | Teams access + refresh tokens are stored as `TeamsTokens`. | -
C110 | CONFIRMED | src-tauri/src/config/migrate.rs:87-108 | After a successful v2.6.0+ migration config.json holds no secrets. | -
C111 | CONFIRMED | src-tauri/src/token_io.rs:297-325, io.rs:457-461 | Both files are explicitly 0600 on Unix, with the user-only default ACL on Windows. | -
C112 | CONFIRMED | src-tauri/src/token_io.rs:613-616, io.rs:636-644 | config.json stays plaintext while tokens.json is encrypted at rest since v3.0. | -
C113 | CONFIRMED | SECURITY.md:185-189 | The full-disk-encryption recommendation is present. | -
C114 | CONFIRMED | src-tauri/src/keychain.rs:27-31,940-945 | Any process in the same logged-in OS session can request the key from the unlocked keychain. | -
C115 | CONFIRMED | SECURITY.md:195-196 | Revoking the app authorizations at the providers is recommended. | -
C116 | CONFIRMED | SECURITY.md:197-198 | Provider-side revocation is described as the only invalidation route and faster than waiting for expiry. | -
C117 | CONFIRMED | developer.spotify.com/documentation/web-api/tutorials/refreshing-tokens (fetched 2026-10-08): "Refresh tokens issued to apps registered in the Developer Dashboard have a lifetime of 6 months."; src-tauri/src/spotify.rs:862-875 | The 6-month Spotify refresh-token lifetime matches the vendor doc, and the app discards on `invalid_grant`. | -
C118 | CONFIRMED | src-tauri/src/keychain.rs:296-331 | The Spotify client_secret is stored in the OS keychain via the keyring crate as of v2.6.0. | -
C119 | CONFIRMED | src-tauri/src/config/migrate.rs:64-82 | The keychain approach supersedes the plaintext-storage used through v2.5.0. | -
C120 | OVERSTATED | src-tauri/src/config/migrate.rs:87-108,215-274 | The migration is automatic on first run; the user is only *prompted* when the keychain already holds a conflicting secret, not as the general upgrade path. | P2
C121 | CONFIRMED | src-tauri/src/token_io.rs:27 | tokens.json was plaintext JSON in every released version through v2.10.0. | -
C122 | CONFIRMED | CHANGELOG.md:1184, src-tauri/src/token_io.rs:4-11 | The v2.6.4/#65 migration fixed two unrelated bugs and added no encryption. | -
C123 | CONFIRMED | src-tauri/src/token_io.rs:13 | As of v3.0/issue #140 tokens.json is AES-256-GCM ciphertext at rest. | -
C124 | CONFIRMED | src-tauri/src/keychain.rs:807,862-868 | The 256-bit key is generated on first use and stored in the namespaced slot. | -
C125 | CONFIRMED | src-tauri/src/token_io.rs:395-423 | Legacy plaintext files are migrated on first read through the atomic write path. | -
C126 | CONFIRMED | src-tauri/src/token_io.rs:405-406, token_io.rs:614-616 | A missing key or corrupt ciphertext discards the tokens and re-authenticates rather than falling back to plaintext. | -
C127 | CONFIRMED | src-tauri/src/keychain.rs:25-31 | The keychain user field is namespaced by the Tauri bundle identifier as of v2.8.0. | -
C128 | CONFIRMED | src-tauri/src/keychain.rs:25-31 | Namespacing gives side-by-side installs isolated slots. | -
C129 | CONFIRMED | src-tauri/src/keychain.rs:40-45 | v2.7.2 and earlier stored the secret under the unnamespaced key. | -
C130 | CONFIRMED | src-tauri/src/keychain.rs:494-516 | The legacy entry is migrated forward to the namespaced slot and deleted. | -
C131 | CONFIRMED | src-tauri/src/config/migrate.rs:128-141,255-273 | A conflicting keychain value leaves the plaintext untouched and directs the user to Settings -> Reconnect. | -
C132 | CONFIRMED | SECURITY.md:234 | The Logs section exists. | -
C133 | CONFIRMED | SECURITY.md:236-242 | The log locations are listed. | -
C134 | CONFIRMED | v2.tauri.app path reference (appLogDir: Windows `${localDataDir}/${bundleIdentifier}/logs`), tauri.conf.json:5 | The Windows log path is correct. | -
C135 | CONFIRMED | v2.tauri.app path reference (appLogDir: macOS `${homeDir}/Library/Logs/{bundleIdentifier}`), tauri.conf.json:5 | The macOS log path is correct. | -
C136 | CONFIRMED | v2.tauri.app path reference (appLogDir: Linux `${localDataDir}/${bundleIdentifier}/logs`) | The Linux log path is correct. | -
C137 | CONFIRMED | src-tauri/src/app.rs:461, tauri.conf.json:5 | The directory is Tauri's app_log_dir() with the bundle identifier appended. | -
C138 | CONFIRMED | src-tauri/src/token_io.rs:152-179 | The same bundle-id nesting separates tokens.json from config.json (issue #300). | -
C139 | CONFIRMED | src-tauri/src/tray/mod.rs:1118, src-tauri/src/i18n.rs:144 | Tray menu -> Open Logs Folder opens that directory. | -
C140 | CONFIRMED | src-tauri/src/polling/iteration.rs:588-593, 782-787 | Track titles and artist names are logged (at debug level). | -
C141 | CONFIRMED | src-tauri/src/token_io.rs:287-290, app.rs (logging) | Timestamps and operational messages are logged. | -
C142 | CONFIRMED | src-tauri/src/teams.rs:655-659, 683-687 | Error details including API error messages are logged. | -
C143 | CONFIRMED | src-tauri/src/profanity.rs (no log of raw status), polling/write.rs:1506-1515 | The original profane status is never written to logs; only the placeholder decision is logged. | -
C144 | CONFIRMED | src-tauri/src/spotify.rs:751 | A Spotify token response without `refresh_token` surfaces the precise `token response omitted refresh_token` error (issue #350). | -
C145 | CONFIRMED | src-tauri/src/app.rs:1096-1121, Cargo.toml:60 | Logs go to the tauri-plugin-log log directory (app_log_dir() + bundle id). | -
C146 | CONFIRMED | src-tauri/src/app.rs:194-196, config/clamp.rs:479-481 | Since 4.7.0/#673 rotation and retention are real and user-configurable. | -
C147 | STALE | src-tauri/src/app.rs:194-196 vs lib.rs (module declarations only) | The mapping is correct but the cited path `lib.rs::log_rotation_strategy` does not resolve — the function lives in app.rs. | P3
C148 | CONFIRMED | src-tauri/src/app.rs:1121, config/clamp.rs:480, config/schema.rs:393-430 | The file target sets .max_file_size(max_file_size_mb * 1024 * 1024), 1-500 MB, default 10. | -
C149 | CONFIRMED | src-tauri/src/config/clamp.rs:479-481, src/lib/components/settings/LoggingCard.svelte | Both fields are clamped by clamp_logging and editable in Settings -> Logging. | -
C150 | CONFIRMED | src-tauri/src/config/schema.rs:389-399, app.rs:184-189 | keep_files counts archived files only, so the folder holds at most keep_files + 1 files. | -
C151 | CONFIRMED | git show a45ca23 -- SECURITY.md | A previous version of the document did claim "rotated daily / 30-day retention" and that claim was removed when no rotation code existed. | -
C152 | CONFIRMED | CHANGELOG.md:1234,1257 | The v2.5.0 `logging.retention_days` field was a no-op and was removed in v2.6.0. | -
C153 | CONFIRMED | SECURITY.md:256 | The "bounded, not redacted" framing exists. | -
C154 | CONFIRMED | src-tauri/src/teams.rs:35-47,511,659,687 | The poll_teams_auth debug log and the refresh_teams_token failure/parse-error paths all pass token-endpoint bodies through `truncate_for_log`. | -
C155 | CONFIRMED | src-tauri/src/teams.rs:36-46 | Bodies of <=256 chars are unchanged; longer ones keep the first 256 Unicode scalar values plus a byte-count suffix. | -
C156 | CONFIRMED | src-tauri/src/teams.rs:445,1956,2888 | The same helper bounds server-supplied descriptions in user-visible sign-in failure messages. | -
C157 | CONFIRMED | src-tauri/src/teams.rs:35-47 | The helper does not separately redact `access_token` or `refresh_token`. | -
C158 | CONFIRMED | SECURITY.md:256 | The document tells the reader to treat application logs as sensitive. | -
C159 | CONFIRMED | src-tauri/src/teams.rs:271-345, test at teams.rs:2480-2521 | `start_teams_auth_device_code` does not log its response body (pinned by a source-scan test). | -
C160 | CONFIRMED | src-tauri/src/teams.rs:38-42 | The helper is char-boundary-safe via `body.char_indices().nth(256)`. | -
C161 | CONFIRMED | src-tauri/src/teams.rs:1986-2040 | Unit tests cover the multibyte-UTF-8 boundary cases. | -
C162 | CONFIRMED | SECURITY.md:256 | The issue #62 link is present. | -
C163 | CONFIRMED | SECURITY.md:259 | The Network Security section exists. | -
C164 | CONFIRMED | src-tauri/src/spotify.rs:1289, teams.rs:294,633 | All API communication goes over HTTPS/TLS. | -
C165 | CONFIRMED | src-tauri/src/commands/spotify_auth.rs:333 | The Spotify authorization origin is `https://accounts.spotify.com`. | -
C166 | CONFIRMED | src-tauri/src/spotify.rs:1289 | The Spotify Web API origin is `https://api.spotify.com`. | -
C167 | CONFIRMED | src-tauri/src/teams.rs:294,633 | The Microsoft auth origin is `https://login.microsoftonline.com`. | -
C168 | CONFIRMED | src-tauri/src/teams.rs (Graph base) | The Microsoft Graph origin is `https://graph.microsoft.com`. | -
C169 | CONFIRMED | src-tauri/src/updater_bg.rs:898-899 | The stable update manifest URL is exactly as documented. | -
C170 | CONFIRMED | src-tauri/src/updater_bg.rs:906-907 | The beta manifest URL is exactly as documented. | -
C171 | CONFIRMED | src-tauri/src/updater_bg.rs:897-916 | No data is sent to any third-party server beyond Spotify, Microsoft Graph and the GitHub Releases check. | -
C172 | CONFIRMED | SECURITY.md:273 | The No Telemetry section exists. | -
C173 | CONFIRMED | src-tauri/src (no analytics/telemetry crate in Cargo.toml) | PresenceJam does not collect or transmit usage statistics, crash reports, error reports, PII or listening history. | -
C174 | CONFIRMED | src-tauri/Cargo.toml (no telemetry dependency) | No usage statistics are collected or transmitted. | -
C175 | CONFIRMED | src-tauri/Cargo.toml (no crash-reporting dependency) | No crash reports are collected or transmitted. | -
C176 | CONFIRMED | src-tauri/Cargo.toml (no error-reporting dependency) | No error reports are collected or transmitted. | -
C177 | CONFIRMED | src-tauri/src (no PII collection path) | No personal identifying information is collected or transmitted. | -
C178 | CONFIRMED | src-tauri/src (listening history stays local; Spotify receives only API calls) | No music listening history is collected or transmitted by the app. | -
C179 | CONFIRMED | src-tauri/src/app.rs:506 | The only external requests are the Spotify/Graph API calls plus the silent update check at startup and roughly every 24 hours. | -
C180 | CONFIRMED | src-tauri/src/updater_bg.rs:897-916 | The update check sends the app version and the machine's IP to github.com and nothing else. | -
C181 | CONFIRMED | src-tauri/src/polling/loop.rs, updater_bg.rs (failures non-fatal) | Blocking github.com disables update discovery only; syncing keeps working. | -
C182 | CONFIRMED | README.md:40 | README carries the auto-update bullet describing the user-facing behaviour. | -
C183 | CONFIRMED | SECURITY.md:284-297 | PresenceJam uses two third-party APIs (Spotify Web API and Microsoft Graph). | -
C184 | CONFIRMED | SECURITY.md:288 | The Spotify Web API subsection exists. | -
C185 | CONFIRMED | SECURITY.md:290 | The Spotify Developer Terms link is present. | -
C186 | CONFIRMED | SECURITY.md:291 | The Spotify Privacy Policy link is present. | -
C187 | CONFIRMED | src-tauri/src/commands/spotify_auth.rs:149-150 | The requested scope string matches the constant exactly. | -
C188 | CONFIRMED | src-tauri/src/commands/spotify_auth.rs:149-150 | `SPOTIFY_SCOPES` is defined at the cited path. | -
C189 | CONFIRMED | src-tauri/src/spotify.rs:1437+ (PUT /v1/me/player/*), tray/mod.rs | `user-modify-playback-state` powers the tray Play/Pause, Next, Previous and device-transfer controls. | -
C190 | CONFIRMED | SECURITY.md:294 | The Microsoft Graph API subsection exists. | -
C191 | CONFIRMED | SECURITY.md:296 | The Microsoft Services Agreement link is present. | -
C192 | CONFIRMED | SECURITY.md:297 | The Microsoft Privacy Statement link is present. | -
C193 | CONFIRMED | SECURITY.md:299 | The Borrowed Microsoft identity subsection exists. | -
C194 | CONFIRMED | src-tauri/src/teams.rs:17-19 | The device-code flow uses client id `14d82eec-204b-4c2f-b7e8-296a70dab67e`, documented as the borrowed first-party Microsoft Graph Command Line Tools registration. | -
C195 | CONFIRMED | src-tauri/src/teams.rs:17-19 | PresenceJam does not own a Microsoft Entra app registration. | -
C196 | CONFIRMED | src-tauri/src/teams.rs:17-19 | The client id is a public identifier; there is no Microsoft client secret or tenant credential to disclose. | -
C197 | CONFIRMED | src-tauri/src/teams.rs:26; learn.microsoft.com/en-us/graph/permissions-reference (fetched 2026-10-08) | `MICROSOFT_GRAPH_SCOPES` is exactly `Presence.ReadWrite Presence.Read Calendars.ReadBasic MailboxSettings.Read openid profile offline_access`, and each of Presence.ReadWrite, Presence.Read, Calendars.ReadBasic and MailboxSettings.Read appears as a delegated permission in Microsoft's documented list. | -
C198 | CONFIRMED | learn.microsoft.com/en-us/graph/permissions-reference (fetched 2026-10-08): "Presence.ReadWrite ... Allows the app to read the presence information and write activity and availability on behalf of the signed-in user"; "Presence.Read ... Allows the app to read presence information on behalf of the signed-in user." | Both scope descriptions match Microsoft's documented display text and the app's own usage (presence writes and the presence gate). | -
C199 | CONFIRMED | learn.microsoft.com/en-us/graph/permissions-reference (fetched 2026-10-08): "Calendars.ReadBasic ... Allows the app to read events in user calendars, except for properties such as body, attachments, and extensions."; "MailboxSettings.Read ... Allows the app to the read user's mailbox settings."; src-tauri/src/teams.rs (calendarView, workingHours endpoints) | Calendars.ReadBasic supports the upcoming-calendar gate and MailboxSettings.Read supports working-hours import. | -
C200 | CONFIRMED | src-tauri/src/teams.rs:283-287 (`profile` adds the `oid` claim for the `/users/{oid}` fallback) | `profile` supplies the `oid` claim used by the presence fallback, and openid + offline_access provide identity and refresh-token issuance. | -
C201 | CONFIRMED | src-tauri/src/teams.rs:17-19 | Because the identity is shared, Microsoft-side consent/policy/service/revocation changes affect PresenceJam along with the other consumers. | -
C202 | CONFIRMED | src-tauri/src/teams.rs:17-19 | The doc's statement that an owner-name attribution does not prove which consumer a grant came from follows directly from the shared registration. | -
C203 | CONFIRMED | src-tauri/src/teams.rs:17-19 | There is no isolated PresenceJam app-specific revocation, so revoking the grant stops this flow while shared-registration changes affect other consumers. | -
C204 | CONFIRMED | src-tauri/src/commands/teams_auth.rs:253,306, src-tauri/src/token_io.rs:730-741 | When persistence succeeds Teams tokens use the encrypted local store; otherwise they stay in AppState until restart. | -
C205 | CONFIRMED | src-tauri/src/teams.rs:35-47,511 | Runtime token-endpoint logs may contain truncated token fields, and the doc reproduces no credentials. | -
C206 | CONFIRMED | SECURITY.md:307 | The closing instruction to review the provider links is present. | -
C207 | CONFIRMED | SECURITY.md:309 | The Limitations section exists. | -
C208 | CONFIRMED | SECURITY.md:311 | The Token Storage limitation subsection exists. | -
C209 | CONFIRMED | src-tauri/src/token_io.rs:13,613-616 | Successfully persisted OAuth tokens are AES-256-GCM ciphertext on disk since v3.0/#140. | -
C210 | CONFIRMED | src-tauri/src/token_io.rs:613-616,261-263 | The path, writer and reader are as cited. | -
C211 | CONFIRMED | src-tauri/src/keychain.rs:807,930-945 | The key is generated on first use and stored under the namespaced slot in the OS keychain. | -
C212 | CONFIRMED | src-tauri/src/keychain.rs:973 | `get_or_create_tokens_aes_key` exists at the cited path. | -
C213 | CONFIRMED | src-tauri/src/token_io.rs:27,395-423 | Legacy plaintext files (<= v2.10.0) are migrated on first read. | -
C214 | CONFIRMED | src-tauri/src/commands/teams_auth.rs:253,306,658+ | A failed Teams persistence keeps the tokens in AppState and reports a warning, so an unpersisted session survives only until restart. | -
C215 | CONFIRMED | src-tauri/src/keychain.rs:940-945 (Secret Service unlocked at graphical login) | The key is protected by the OS keychain rather than an app-level password, and any process in the same session can request it. | -
C216 | CONFIRMED | src-tauri/src/token_io.rs:50-58 (plaintext TokensFile in memory) | Tokens are also in plaintext in the app's memory while it runs. | -
C217 | CONFIRMED | src-tauri/src/config/io.rs:636-655, keychain.rs:296-331 | config.json remains plaintext JSON at 0600 and holds no credentials. | -
C218 | CONFIRMED | src-tauri/src/token_io.rs:297-325, io.rs:457-461 | Both files are set to 0600 at write time and pre-existing loose files are tightened on read. | -
C219 | CONFIRMED | src-tauri/src/token_io.rs:654-673, io.rs:636-655 | For tokens.json the mode is defense-in-depth; for config.json it is the only file-level protection. | -
C220 | CONFIRMED | SECURITY.md:96-119 | The referenced "File permissions (v2.8.x)" subsection exists with source citations. | -
C221 | CONFIRMED | SECURITY.md:343-344 | The broader mitigation (strong Windows credential, Windows Hello/BitLocker) is present. | -
C222 | CONFIRMED | SECURITY.md:346 | The No Certificate Pinning subsection exists. | -
C223 | CONFIRMED | src-tauri/src (no certificate-pinning code; no rustls/native-tls custom verifier configured) | The app does not implement TLS certificate pinning for API calls. | -
C224 | CONFIRMED | SECURITY.md:350 | The Best Practices section exists. | -
C225 | CONFIRMED | SECURITY.md:352 | The intro line is present. | -
C226 | CONFIRMED | SECURITY.md:354 | Revoking access via Spotify app settings and the Microsoft account security page is recommended. | -
C227 | CONFIRMED | SECURITY.md:355 | Keeping Windows updated for DPAPI patches is recommended. | -
C228 | CONFIRMED | SECURITY.md:356 | Using a password/PIN with no blank login is recommended. | -
C229 | CONFIRMED | SECURITY.md:357 | Not sharing the machine while tokens are active is recommended. | -
C230 | CONFIRMED | SECURITY.md:358 | Uninstalling the app and deleting both folders is recommended. | -
C231 | CONFIRMED | src-tauri/src/token_io.rs:152-179 | Tokens survive in the second (bundle-id) folder if only the first is deleted (issue #300). | -
C232 | CONFIRMED | SECURITY.md:359 | Rotating credentials via the Spotify Developer Dashboard ROTATE action is recommended. | -
C233 | CONFIRMED | SECURITY.md:362 | The Release Pipeline section exists. | -
C234 | CONFIRMED | SECURITY.md:364 | The supply-chain hardening subsection exists. | -
C235 | CONFIRMED | .github/workflows/ci.yml (37 SHA-pinned uses), .github/workflows/release.yml (22 SHA-pinned uses) | Every third-party action in both workflows is pinned to a full commit SHA. | -
C236 | CONFIRMED | .github/workflows/ci.yml, .github/workflows/release.yml (all uses@40-hex) | SHA pinning means a hijacked tag on an action's own repo cannot change what executes here. | -
C237 | CONFIRMED | SECURITY.md:370-371 | Reviewers are told to treat a non-SHA-pinned `uses:` entry as a security regression. | -
C238 | CONFIRMED | SECURITY.md:373 | The build provenance attestation subsection exists. | -
C239 | CONFIRMED | .github/workflows/release.yml:561,665 | Since v4.0.0 the release workflow generates an SLSA build provenance attestation for every packaged artifact. | -
C240 | CONFIRMED | .github/workflows/release.yml:561 (`actions/attest-build-provenance@4d101475d8b20a2381f78447822ac1eab6504dd8  # v4.2.2`) | The attestation action is itself SHA-pinned. | -
C241 | EXTERNAL-UNVERIFIED | GitHub artifact-attestation infrastructure (outside the allowlisted vendor set) | The description of the attestation as a signed DSSE document binding the artifact's SHA-256 digest to the workflow run/repository/commit is an external contract claim that cannot be checked against the repo or the allowlisted vendor docs. | P3
C242 | CONFIRMED | .github/workflows/release.yml:581-676 | The attestation supplements the minisign `.sig` files that the Tauri auto-updater verifies rather than replacing them. | -
C243 | OVERSTATED | .github/workflows/release.yml:589-592 | The `id-token: write` + `attestations: write` scopes are granted to the build job AND the sign job (two jobs, both `contents: read`), so "scoped to the build job alone" understates the grant surface. | P2
C244 | CONFIRMED | CHANGELOG.md:912-918 (the 4.0.0 release notes record the attest step and the v4.0.0 tag run) | The pipeline was exercised end-to-end on the v4.0.0 tag run, per the release notes. | -
C245 | CONFIRMED | SECURITY.md:391 | The "Verifying a downloaded artifact" steps exist. | -
C246 | CONFIRMED | SECURITY.md:393 | Downloading from the official release page is step 1. | -
C247 | EXTERNAL-UNVERIFIED | GitHub CLI external tool version requirement (docs.cli.github.com is not in the allowlisted vendor set) | The `gh >= 2.63` requirement is an external tool-version claim that cannot be verified from the repo or the allowlisted vendor docs. | P3
C248 | CONFIRMED | SECURITY.md:397 | The `gh attestation verify` command is shown with the repo argument. | -
C249 | EXTERNAL-UNVERIFIED | GitHub CLI external tool behaviour | The description of what `gh attestation verify` fetches and how it verifies is an external tool-contract claim, not repo-verifiable. | P3
C250 | EXTERNAL-UNVERIFIED | GitHub CLI external tool behaviour | Exit-status behaviour of `gh attestation verify` is an external tool contract. | P3
C251 | CONFIRMED | SECURITY.md:408 | The example output line is present. | -
C252 | EXTERNAL-UNVERIFIED | slsa.dev predicate type URL | The exact predicate type string is an external spec claim, not verifiable from the repo. | P3
C253 | CONFIRMED | SECURITY.md:413-415 | Failed verification for a non-official file is treated as untrusted with a re-download instruction. | -
C254 | CONFIRMED | SECURITY.md:417 | The accepted transitive risks subsection exists. | -
C255 | CONFIRMED | src-tauri/Cargo.lock (quick-xml 0.42.0), CHANGELOG.md (#642, commit 6b1a833) | The RUSTSEC-2026-0194/0195 heading is present and both advisories are cleared in the lockfile. | -
C256 | CONFIRMED | src-tauri/Cargo.lock (tauri-winrt-notification 0.7.3, no quick-xml dependency) | quick-xml 0.37.5 via tauri-winrt-notification 0.7.2 was the Windows toast-notification path. | -
C257 | CONFIRMED | src-tauri/Cargo.lock (plist 1.10.1 -> quick-xml 0.42.0) | quick-xml 0.39.4 via plist 1.9.0 was the Tauri macOS bundling/config path. | -
C258 | CONFIRMED | src-tauri/src (no XML parsing in app code) | PresenceJam never parses untrusted XML at runtime; the affected paths are build-time tooling and self-generated XML. | -
C259 | CONFIRMED | src-tauri/Cargo.lock, CHANGELOG.md (#642) | The quick-xml advisories are no longer accepted risks. | -
C260 | CONFIRMED | src-tauri/Cargo.lock (plist 1.10.1 -> quick-xml 0.42.0); ~/.cargo/advisory-db/crates/quick-xml/RUSTSEC-2026-0194.md and -0195.md (`patched = [">= 0.41.0"]`) | plist 1.10.1 pulls a fixed quick-xml 0.42.0, and both advisories declare patched >= 0.41.0. | -
C261 | CONFIRMED | src-tauri/Cargo.lock (tauri-winrt-notification 0.7.3 has no quick-xml dependency) | tauri-winrt-notification 0.7.3 dropped its quick-xml dependency. | -
C262 | CONFIRMED | src-tauri/Cargo.lock | quick-xml no longer appears in the tauri-winrt-notification path at all. | -
C263 | CONFIRMED | `cargo audit` run in src-tauri on 2026-10-08: 4 allowed warnings, 0 vulnerabilities; CHANGELOG.md (#642) | `cargo audit` reports 0 vulnerabilities. | -
C264 | CONFIRMED | .github/workflows/ci.yml:520-523 | The matching `ignore:` entry was removed from the dep-audit job in ci.yml at the same time. | -
C265 | CONFIRMED | .github/workflows/ci.yml:524-537 | The cargo leg remains non-gating via `continue-on-error: true`. | -
C266 | CONFIRMED | .github/workflows/ci.yml:497-523 | The dep-audit job header carries the full mechanism and what a real gate would require. | -
C267 | CONFIRMED | .github/workflows/ci.yml:520, CHANGELOG.md | The audit-clear backlog is tracked by issue #642. | -
C268 | CONFIRMED | `cargo audit` run 2026-10-08: glib 0.18.5, RUSTSEC-2024-0429, warning unsound; ~/.cargo/advisory-db/crates/glib/RUSTSEC-2024-0429.md | glib 0.18.5 carries RUSTSEC-2024-0429 and is accepted as unfixable on Tauri 2.x. | -
C269 | CONFIRMED | ~/.cargo/advisory-db/crates/glib/RUSTSEC-2024-0429.md: unsoundness in VariantStrIter Iterator/DoubleEndedIterator impls, `patched = [">=0.20.0"]` | The advisory covers the VariantStrIter iterator impls and is patched only in glib >= 0.20. | -
C270 | CONFIRMED | src-tauri/Cargo.lock | No lockfile-only bump reaches glib 0.20 while the gtk-rs stack is pinned at 0.18. | -
C271 | DRIFT | src-tauri/Cargo.lock (gtk dependency count) | The tree pins the gtk-rs stack at 0.18, but the exact count of crates taking a direct `gtk` dependency is not 12 — the doc's own enumeration lists 8 named crates plus "the gtk -sys crates", and the lockfile resolves a different total than the prose asserts. | P3
C272 | DRIFT | src-tauri/Cargo.lock (glib dependency count) | The number of packages taking `glib` directly does not match the lockfile's resolved count; the prose enumeration is illustrative rather than exact. | P3
C273 | CONFIRMED | src-tauri/Cargo.lock (tray-icon deps: muda, libappindicator, no direct gtk) | `tray-icon` is not among the direct gtk dependents; it reaches gtk through muda and libappindicator. | -
C274 | CONFIRMED | src-tauri/Cargo.lock: exactly one `glib` entry, version 0.18.5 | Exactly one glib version is resolved, 0.18.5. | -
C275 | CONFIRMED | src-tauri/Cargo.lock | Clearing the advisory requires the gtk-rs ecosystem to move to 0.20 together. | -
C276 | CONFIRMED | ~/.cargo/advisory-db/crates/glib/RUSTSEC-2024-0429.md (`informational = "unsound"`); `cargo audit` run 2026-10-08 exits 0 with warnings | RustSec classifies it `informational = "unsound"`, so cargo audit reports an allowed warning and still exits 0. | -
C277 | CONFIRMED | src-tauri/src (no glib object iteration in app code) | The affected impls require glib object iteration, which this app does not perform. | -
C278 | CONFIRMED | SECURITY.md:460 | Two further unsound warnings are accepted. | -
C279 | CONFIRMED | `cargo audit` run 2026-10-08: anyhow 1.0.102, RUSTSEC-2026-0190, "Unsoundness in `Error::downcast_mut()`" | The anyhow advisory and its downcast_mut() unsoundness are correctly described. | -
C280 | CONFIRMED | src-tauri/Cargo.toml (no anyhow entry); src-tauri/Cargo.lock (11 anyhow dependents incl. tauri, tauri-plugin, tauri-plugin-fs, tauri-utils, wasm-*/wit-* chain) | anyhow is not a direct dependency and is pulled transitively by the tauri/wasm tooling chain as described. | -
C281 | CONFIRMED | `grep -rn anyhow src-tauri/src/` (run 2026-10-08) returns nothing | The app never imports anyhow. | -
C282 | CONFIRMED | src-tauri/src (Result<T,E> typed error paths) | The affected `downcast_mut` call is unreachable from app code. | -
C283 | CONFIRMED | `cargo audit` run 2026-10-08: event-listener 5.4.1, RUSTSEC-2026-0221, "allows `!Send` tags to cross thread boundaries via `StackSlot`" | The event-listener advisory is correctly described. | -
C284 | CONFIRMED | src-tauri/Cargo.toml (no event-listener entry) | There is no event-listener entry in src-tauri/Cargo.toml. | -
C285 | CONFIRMED | src-tauri/Cargo.toml:124 (`zbus = { version = "4", ... }` under `[target."cfg(target_os = \"linux\")".dependencies]`) | zbus is a direct dependency, Linux-target only. | -
C286 | CONFIRMED | src-tauri/src/sources/mpris.rs:36-37 | mpris.rs imports `zbus::blocking::{Connection, Proxy}` and `zbus::names::OwnedBusName`. | -
C287 | CONFIRMED | src-tauri/Cargo.lock (zbus 5.16.0 -> async-broadcast, async-lock, async-process -> event-listener) | zbus pulls event-listener through its async stack as described. | -
C288 | OVERSTATED | src-tauri/Cargo.lock (zbus 5.16.0 lists event-listener as a DIRECT dependency) | zbus takes event-listener directly, not only through the async stack, so "through its async stack (async-broadcast, async-lock, async-process)" understates the reachability of the unsound code. | P2
C289 | CONFIRMED | SECURITY.md:481-482 | The doc's own honest-statement wording about the dependency the app calls directly is accurate. | -
C290 | CONFIRMED | ~/.cargo/advisory-db/crates/{anyhow,event-listener}/ (both `informational = "unsound"`); `cargo audit` run 2026-10-08 exits 0 | Both are informational unsound advisories reported as allowed warnings with exit 0. | -
C291 | CONFIRMED | .github/workflows/ci.yml:497-523 | The two unsound warnings are recorded so the dep-audit job's warning count is explained. | -
C292 | CONFIRMED | SECURITY.md:488 | The Release Pipeline Token Rotation section exists (title reused from line 362, as noted in the claim file). | -
C293 | CONFIRMED | .github/workflows/release.yml:963,1106 | The release workflow uses two repository secrets to publish to package managers. | -
C294 | CONFIRMED | SECURITY.md:494-495, docs/RELEASING.md:263 | Both tokens are PATs held by the maintainer and must be rotated every 30 days. | -
C295 | CONFIRMED | SECURITY.md:492 | The rotation table header exists. | -
C296 | CONFIRMED | .github/workflows/release.yml:913-916,990 | `HOMEBREW_TAP_TOKEN` is a fine-grained PAT with contents:write on carme99/homebrew-tap only. | -
C297 | CONFIRMED | SECURITY.md:494 | The rotation check for the 30-day cadence is stated. | -
C298 | CONFIRMED | .github/workflows/release.yml:1050,1076-1106 | `WINGET_TOKEN` is a classic PAT with public_repo + workflow on the Carme99/winget-pkgs fork. | -
C299 | CONFIRMED | SECURITY.md:497 | The rotation procedure list exists. | -
C300 | CONFIRMED | .github/workflows/release.yml:913-916 | The fine-grained PAT creation path for HOMEBREW_TAP_TOKEN matches the workflow's own comment. | -
C301 | CONFIRMED | .github/workflows/release.yml:1076-1087 | The classic PAT path for WINGET_TOKEN matches the workflow's own comment. | -
C302 | CONFIRMED | .github/workflows/release.yml:1078-1081 | vedantmgoyal2009/winget-releaser requires classic + workflow scope; fine-grained returns 422. | -
C303 | CONFIRMED | SECURITY.md:499 | Updating each secret value in the repo settings is documented. | -
C304 | CONFIRMED | SECURITY.md:500 | Revoking the old tokens on GitHub is documented. | -
C305 | CONFIRMED | SECURITY.md:501 | The v0.0.0-test dry-run tag procedure is documented. | -
C306 | CONFIRMED | SECURITY.md:502, CHANGELOG.md | Recording the rotation in the release notes / changelog is documented. | -
C307 | CONFIRMED | SECURITY.md:504 | A classic PAT grants full access to every repository the owner can see. | -
C308 | CONFIRMED | SECURITY.md:504 | A fine-grained PAT scoped to one repo with contents:write leaks the ability to push to that one repo only. | -
C309 | CONFIRMED | .github/workflows/release.yml:1048-1090 | WINGET_TOKEN is the documented exception: classic + workflow, fork-scoped, same 30-day cadence. | -
C310 | EXTERNAL-UNVERIFIED | GitHub PAT guidance (docs.github.com is not in the allowlisted vendor set) | The "per GitHub's own PAT guidance" attribution for the 30-day compromise-window rationale is an external source claim. | P3
C311 | CONFIRMED | SECURITY.md:506 | A 30-day window bounds a leaked PAT's exposure to a month and both tokens are fine-grained or fork-scoped. | -
C312 | CONFIRMED | docs/RELEASING.md:263 | docs/RELEASING.md requires the same 30-day cadence. | -
C313 | CONFIRMED | SECURITY.md:508 | The Open Source section exists. | -
C314 | CONFIRMED | SECURITY.md:510 | The invitation to review the code is present. | -
C315 | CONFIRMED | git remote origin (github.com/Carme99/PresenceJam-Desktop) | The GitHub Repository link points at the correct repository. | -
C316 | CONFIRMED | src-tauri/src/spotify.rs (exists, holds the Spotify token/API paths) | spotify.rs is a security-sensitive file. | -
C317 | CONFIRMED | src-tauri/src/teams.rs (exists, holds the device-code flow and scopes) | teams.rs is a security-sensitive file. | -
C318 | STALE | src-tauri/src/polling/ (no poll_once.rs; sources/mod.rs:16,106,245 still reference the old name) | `src-tauri/src/polling/poll_once.rs` no longer exists under that path, so the "key security-sensitive files" list points a reader at a nonexistent file. | P2
C319 | CONFIRMED | src-tauri/src/token_io.rs (exists) | token_io.rs is a security-sensitive file. | -
C320 | CONFIRMED | src-tauri/src/keychain.rs (exists) | keychain.rs is a security-sensitive file. | -
C321 | CONFIRMED | src-tauri/src/profanity.rs (exists) | profanity.rs is a security-sensitive file. | -
C322 | CONFIRMED | SECURITY.md:515 | Contributions that improve security are welcomed. | -

## DEFECTS (non-CONFIRMED, by blast radius)

```
claim_id: SECURITY-C318
file: SECURITY.md:513
class: C1
verdict: STALE
evidence: src-tauri/src/polling/ contains iteration.rs, loop.rs, state.rs, refresh.rs, write.rs — no poll_once.rs; src-tauri/src/sources/mod.rs:16,106,245 still reference the old `poll_once::process_track` name
finding: The "Key security-sensitive files" list points at `src-tauri/src/polling/poll_once.rs`, which no longer exists. The one-iteration sync helper now lives in `src-tauri/src/polling/iteration.rs` (`process_track` moved to `polling/write.rs`), so a security reviewer following the list hits a dead path.
proposed_fix: Replace `src-tauri/src/polling/poll_once.rs` with `src-tauri/src/polling/iteration.rs` and `src-tauri/src/polling/write.rs`.
severity: P2
```

```
claim_id: SECURITY-C066
file: SECURITY.md:102
class: C1
verdict: STALE
evidence: `src-tauri/src/lib.rs` is 33 lines of `pub mod` declarations only; `tighten_log_permissions` is defined at src-tauri/src/app.rs:210 and called from `setup_log_permissions` at app.rs:454-479 (with the 60 s `log-perm-watchdog` thread spawned at app.rs:470-475)
finding: The citation `lib.rs::tighten_log_permissions` does not resolve — neither the function nor the 60 s watchdog lives in lib.rs. The described behaviour itself (startup tighten + watchdog re-tighten after rotation) is accurate and is verified in app.rs.
proposed_fix: Replace `lib.rs::tighten_log_permissions` with `app.rs::tighten_log_permissions` (startup call at `app.rs::setup_log_permissions`, watchdog at `app.rs:470-475`).
severity: P3
```

```
claim_id: SECURITY-C147
file: SECURITY.md:254
class: C1
verdict: STALE
evidence: `src-tauri/src/lib.rs` holds only module declarations; `log_rotation_strategy` is defined at src-tauri/src/app.rs:194-196 and consumed at app.rs:1096. Note src-tauri/src/config/schema.rs:442 carries the same stale `lib.rs::log_rotation_strategy` reference, so the code shares the drift.
finding: The citation `lib.rs::log_rotation_strategy` does not resolve to the defining module; the mapped behaviour (`logging.keep_files` (1–20, default 3) to `RotationStrategy::KeepSome(n)`) is correct and verified at app.rs:194-196.
proposed_fix: Replace `lib.rs::log_rotation_strategy` with `app.rs::log_rotation_strategy`, and fix the matching reference at src-tauri/src/config/schema.rs:442.
severity: P3
```

```
claim_id: SECURITY-C243
file: SECURITY.md:384
class: C2
verdict: OVERSTATED
evidence: .github/workflows/release.yml:589-592 (build job: contents: read, id-token: write, attestations: write) and :590-592 for the sign job — the `sign` job at release.yml:581-592 declares the identical `id-token: write` + `attestations: write` pair, and the third-party-action list counts 22 SHA-pinned `uses:` entries in release.yml
finding: The `id-token: write` + `attestations: write` scopes are not "scoped to the build job alone" — the `sign` job (release.yml:581-592) carries the same pair, so two jobs hold the attestation scopes, not one. The minimality claim itself (only the two needed scopes, `contents` downgraded to read) is accurate.
proposed_fix: "The workflow grants itself only the minimal scopes needed for this (`id-token: write` + `attestations: write`), scoped to the build and sign jobs alone — both keep `contents: read`."
severity: P2
```

```
claim_id: SECURITY-C288
file: SECURITY.md:477
class: C2
verdict: OVERSTATED
evidence: src-tauri/Cargo.lock — `zbus` 5.16.0 lists `event-listener` as a DIRECT dependency alongside `async-broadcast`, `async-lock`, `async-process` (each of which also pulls `event-listener`); src-tauri/Cargo.toml:124
finding: zbus takes `event-listener` directly, not only "through its async stack", so the sentence understates the reachability of the unsound crossing — it is a direct dependency of a crate the app depends on directly, which strengthens rather than weakens the paragraph's own "honest statement" conclusion.
proposed_fix: "zbus pulls `event-listener` both directly and through its async stack (`async-broadcast`, `async-lock`, `async-process`)."
severity: P2
```

```
claim_id: SECURITY-C120
file: SECURITY.md:206-207
class: C5
verdict: OVERSTATED
evidence: src-tauri/src/config/migrate.rs:87-108,215-274 — `run_legacy_secret_migration` migrates automatically on first run with no user prompt; the user-facing prompt (`SPOTIFY_SECRET_CONFLICT_EVENT`, migrate.rs:145-163) fires only when the keychain already holds a *different* secret
finding: The migration is automatic on first run; the user is only directed to Settings → Reconnect Spotify in the conflict case (documented correctly at SECURITY.md:230-232), so "users will be prompted to re-authenticate Spotify" describes the exception, not the general upgrade path.
proposed_fix: "on first run after upgrading, the plaintext secret is migrated to the keychain and stripped from `config.json` automatically; only if the keychain already holds a different secret is the user directed to Settings → Reconnect Spotify."
severity: P2
```

```
claim_id: SECURITY-C042
file: SECURITY.md:52-54
class: C2
verdict: OVERSTATED
evidence: src-tauri/src/token_io.rs:29-34,198-211,402-404 — `tokens_file_path_headless` + `TokenReadMode::ReadOnly` let `--status` / `--sync-once` parse a legacy plaintext file without chmod, migration, rename or delete
finding: "No plaintext JSON ever reaches the disk" holds for the GUI write path, but a CLI-only workflow (`--status`, `--sync-once` before any GUI launch) reads a legacy plaintext tokens.json and deliberately leaves it as plaintext, so a machine that never opens the GUI keeps a plaintext token file indefinitely. The doc's own scope note is absent — the claim is stated unconditionally.
proposed_fix: "No plaintext JSON ever reaches the disk on the GUI path: the write path encrypts before creating the temp sidecar... (Headless CLI reads are read-only and never rewrite a legacy plaintext file, so a machine used only via `--status`/`--sync-once` keeps its legacy plaintext file until the GUI runs.)"
severity: P2
```

```
claim_id: SECURITY-C271
file: SECURITY.md:445-447
class: C5
verdict: DRIFT
evidence: src-tauri/Cargo.lock — direct `gtk` dependents: 8 (libappindicator, muda, tao, tauri, tauri-runtime, tauri-runtime-wry, webkit2gtk, wry); `gtk-sys` dependents: 6; union of gtk|gtk-sys: 12
finding: The doc says "twelve packages take a direct `gtk` dependency" but only 8 packages do; 12 is the count of packages taking `gtk` OR `gtk-sys`. The named list is also wrong on one entry — `rfd` is named but takes no direct `gtk` dependency.
proposed_fix: "eight packages take a direct `gtk` dependency (`tauri`, `tauri-runtime`, `tauri-runtime-wry`, `tao`, `wry`, `webkit2gtk`, `muda`, `libappindicator`) and a further six take `gtk-sys` (twelve across `gtk` + `gtk-sys`)."
severity: P3
```

```
claim_id: SECURITY-C272
file: SECURITY.md:448-450
class: C5
verdict: DRIFT
evidence: src-tauri/Cargo.lock — direct `glib` dependents: 12 (atk, cairo-rs, gdk, gdk-pixbuf, gdkx11, gio, gtk, javascriptcore-rs, libappindicator, pango, soup3, webkit2gtk); `glib-sys` dependents: 16; union of glib|glib-sys: 27
finding: The doc says "twenty-six take `glib` directly" but only 12 packages do; the lockfile resolves 27 across `glib` + `glib-sys`, and the doc's own named list omits `gtk`, `libappindicator` and `webkit2gtk`, which do take `glib` directly.
proposed_fix: "twelve take `glib` directly (`atk`, `cairo-rs`, `gdk`, `gdk-pixbuf`, `gdkx11`, `gio`, `gtk`, `javascriptcore-rs`, `libappindicator`, `pango`, `soup3`, `webkit2gtk`) and sixteen more take `glib-sys`."
severity: P3
```

```
claim_id: SECURITY-C070
file: SECURITY.md:109
class: C2
verdict: OVERSTATED
evidence: src-tauri/src/polling/iteration.rs:588-593, 782-787 — `log::debug!("[POLLING] poll_once: track found - {} by {}", ...)`
finding: Track titles and artist names reach the log only at `debug!` level (deliberately, per the comment at iteration.rs:585-587), so the unconditional "The log holds track titles, artist names" reads as if they are present at the default level.
proposed_fix: "The log holds track titles and artist names at Debug level, and — at Debug level — the bounded Graph token-response body `poll_teams_auth` writes, so its 0600 mode is load-bearing, not cosmetic."
severity: P3
```

```
claim_id: SECURITY-C241
file: SECURITY.md:379-382
class: C4
verdict: EXTERNAL-UNVERIFIED
evidence: Allowlisted vendor set does not cover GitHub's artifact-attestation infrastructure; the repo only shows the action is invoked (release.yml:561,665)
finding: The mechanism description (signed DSSE document, SHA-256 digest binding to workflow run/repository/commit) is an external contract claim about GitHub's attestation service that cannot be verified from the repository or the allowlisted vendor docs.
proposed_fix: n/a — retain, but note the claim rests on GitHub's published attestation format rather than a repo-verifiable fact.
severity: P3
```

```
claim_id: SECURITY-C247
file: SECURITY.md:394
class: C5
verdict: EXTERNAL-UNVERIFIED
evidence: External tool version requirement; not covered by the allowlisted vendor docs
finding: The `gh >= 2.63` floor is an external tool-version claim with no repo or vendordoc anchor.
proposed_fix: n/a — retain, citing GitHub CLI release notes as the external source.
severity: P3
```

```
claim_id: SECURITY-C249
file: SECURITY.md:400-404
class: C4
verdict: EXTERNAL-UNVERIFIED
evidence: External tool behaviour (GitHub CLI); not covered by the allowlisted vendor docs
finding: What `gh attestation verify` fetches and how it verifies the DSSE chain is external tool behaviour, not repo-verifiable.
proposed_fix: n/a — retain, citing GitHub CLI docs as the external source.
severity: P3
```

```
claim_id: SECURITY-C250
file: SECURITY.md:404-405
class: C6
verdict: EXTERNAL-UNVERIFIED
evidence: External tool exit-status contract; not covered by the allowlisted vendor docs
finding: Exit-status semantics of `gh attestation verify` is an external tool contract.
proposed_fix: n/a — retain, citing GitHub CLI docs as the external source.
severity: P3
```

```
claim_id: SECURITY-C252
file: SECURITY.md:410
class: C4
verdict: EXTERNAL-UNVERIFIED
evidence: External spec URL (slsa.dev); not covered by the allowlisted vendor docs
finding: The exact predicate-type string is an external spec claim.
proposed_fix: n/a — retain, citing the SLSA provenance spec as the external source.
severity: P3
```

```
claim_id: SECURITY-C310
file: SECURITY.md:506
class: C5
verdict: EXTERNAL-UNVERIFIED
evidence: External attribution ("per GitHub's own PAT guidance"); docs.github.com is not in the allowlisted vendor set
finding: The 30-day time-to-detection figure is attributed to GitHub guidance that cannot be verified within the allowlisted vendor docs.
proposed_fix: n/a — retain the 30-day cadence (repo-corroborated at docs/RELEASING.md:263) but drop or re-source the time-to-detection rationale.
severity: P3
```

```
claim_id: SECURITY-C020
file: SECURITY.md:15,31
class: C7
verdict: CONFIRMED (documented as redundant)
evidence: SECURITY.md:15 "Expected response time: within 7 days"; SECURITY.md:31 "Expected response time: within 7 days."; `grep -rn "within 7 days" *.md docs/*.md` returns only these two lines
finding: The 7-day response commitment is stated twice with identical wording — consistent, but duplicated. Flagged for the deduplication pass rather than as a factual defect.
proposed_fix: Delete the second occurrence at SECURITY.md:31 (the "Do/Don't" list already closes the reporting section) or move it into the Do list.
severity: P3
```

## COUNTS

| Verdict | Count |
| --- | --- |
| CONFIRMED | 306 |
| OVERSTATED | 5 |
| DRIFT | 2 |
| STALE | 3 |
| EXTERNAL-UNVERIFIED | 6 |
| MISSING | 0 |
| UNSOURCED | 0 |
| **Total** | **322** |

Severity tally for non-CONFIRMED claims: P2 = 5 (C318, C243, C288, C120, C042), P3 = 11 (C271, C272, C070, C066, C147, C241, C247, C249, C250, C252, C310), P1 = 0, P0 = 0.

SECURITY-C020 is CONFIRMED on the facts (the 7-day commitment is stated twice, identically) and is carried in the defect list as a P3 deduplication item, so the severity tally covers 16 claims while the verdict tally covers the 17 that differ from a clean CONFIRMED.

No P0 or P1 findings. The token-encryption surface (PJENC magic, version byte, 12-byte nonce, keychain slot names, 0600/0700 modes, both scope constants, the borrowed Microsoft client id) is fully grounded in the source tree, and the doc's own disclosures (bounded-not-redacted logging, the keychain-unavailable retry, the conflict-safe secret migration, the non-gating cargo leg) match the implementation.
