# Security

PresenceJam handles sensitive authentication credentials. Here's how it manages security.

## Reporting Security Issues

If you find a security vulnerability, please report it responsibly:

**How to report:**

1. **Preferred — GitHub Security Advisories** (private, no public disclosure):
   - Go to: [https://github.com/Carme99/PresenceJam-Desktop/security/advisories](https://github.com/Carme99/PresenceJam-Desktop/security/advisories)
   - Click **"Report a vulnerability"**
   - This creates a private advisory that only you and the repo maintainers can see
   - Expected response time: within 7 days

2. **Alternative — responsible public disclosure**:
   - File a GitHub Issue with the title prefix `security: ` and the label `security`
   - Mark the issue as ** confidential** (look for the "Keep this issue confidential" toggle in the issue form)
   - Do not include sensitive details in the issue body — just a summary, and note that full details will be shared privately via discussion

**Do:**
- Give a clear description of the vulnerability and how to reproduce it
- Wait for a response before disclosing publicly
- Include the affected version if known

**Don't:**
- File a regular public GitHub issue for security bugs
- Ask for compensation in exchange for reporting

Expected response time: within 7 days.

## Data Storage

| Token Type | Storage Location | Encryption |
|-----------|----------------|------------|
| Spotify access/refresh tokens | `<app-config-dir>/PresenceJam/tokens.json` (hand-rolled atomic write via `src-tauri/src/token_io.rs`) | **Plaintext JSON.** No encryption is applied to this file; the app does NOT call the OS keychain or DPAPI for OAuth tokens. See "Plaintext tokens.json" note below for the actual protection model. |
| Teams access/refresh tokens | `<app-config-dir>/PresenceJam/tokens.json` (same hand-rolled atomic write) | **Plaintext JSON** — same as Spotify above. |
| Spotify OAuth pending state (PKCE verifier, state) | **Not persisted to disk.** Lives in `AppState::PendingSpotifyAuth.state` only (in-memory); the deep-link callback resolves it before the user closes the app. If the process is killed mid-OAuth, the user re-starts the OAuth flow and Spotify issues a fresh code. | N/A — in-memory only. |

**Plaintext tokens.json on disk (v2.6.4 to present — issue #65):** As of
v2.6.4, `token_io.rs` writes OAuth access/refresh tokens as plaintext JSON
to `<app-config-dir>/PresenceJam/tokens.json`. This was a deliberate change
during the security hardening of #65 — the previous `tauri-plugin-store`
path was found to leak credentials to the webview via the IPC bridge. The
OS keychain is reserved **only** for the Spotify `client_secret` (see
"Configuration" below). The only protection for `tokens.json` is the
file permissions that the OS grants the new file at creation time
(PresenceJam does not explicitly set a mode — under the typical umask
022, `tokens.json` ends up mode 0644 on macOS / Linux, which is
world-readable on the local machine; on Windows the default ACL
inherits the user-only parent permissions and is not world-readable).
This is **the real-world soft exposure**. Mitigations to consider in a
future release: explicitly `chmod 0600` on Unix, and explicit user-only
DACLs on Windows; full-disk encryption (BitLocker / FileVault / LUKS)
is the strongest defense today against offline-disk reads. See
`ARCHITECTURE.md` "Storage" section for the implementation reference.

### Configuration

App settings are stored in two files:

```
%APPDATA%\PresenceJam\config.json   (Windows)
~/Library/Application Support/PresenceJam/config.json   (macOS)
$XDG_CONFIG_HOME/PresenceJam/config.json   (Linux; falls back to ~/.config/)
%APPDATA%\PresenceJam\tokens.json   (Windows; same dir as config.json)
~/Library/Application Support/PresenceJam/tokens.json   (macOS)
$XDG_CONFIG_HOME/PresenceJam/tokens.json   (Linux)
```

`config.json` contains:

- Spotify Client ID — stored **plaintext** in `config.json`
- Spotify **Client Secret** is **NOT** in `config.json` as of v2.6.0 — it
  lives in the **OS keychain** (see "Status" note below). Older v2.5.0 and
  earlier configs that still carry the plaintext secret are auto-migrated
  to the keychain on first run after upgrade and the plaintext is stripped.
- Status format template (`status_format`)
- Profanity filter settings (`profanity_filter`, `profanity_placeholder`)
- Polling configuration
- Logging preferences

`tokens.json` contains (plaintext — see "Plaintext tokens.json" note above):

- Spotify access token + refresh token (`SpotifyTokens` JSON object)
- Teams access token + refresh token (`TeamsTokens` JSON object)

**⚠️ The `config.json` file (now) contains no secrets after a successful
v2.6.0+ migration**, but the plaintext tokens in `tokens.json` are exposed
to any process running under the same OS user (on Unix-like systems,
under the typical umask 022, the file is created 0644; PresenceJam does
not change this). Recommendations, ordered by effort:

1. Enable full-disk encryption on the OS (BitLocker on Windows, FileVault
   on macOS, LUKS on Linux). This is the strongest defense against
   physical-disk theft and the only defense that protects the plaintext
   `tokens.json` against an offline read of the disk.
2. Do not run PresenceJam on a machine whose user account is shared with
   untrusted parties; on Unix-like systems, any other process running
   under the same user can read `tokens.json`.
3. **Revoke your Spotify and Teams app authorizations** from those
   providers' settings if you suspect the local machine is compromised.
   This is the only way to invalidate the credentials stored in
   `tokens.json` from the provider side; revocation is faster than
   waiting for the tokens to expire (Spotify refresh token TTL is long).

> **Status:** As of v2.6.0, the Spotify `client_secret` is stored in the
> **OS keychain** (macOS Keychain, Windows DPAPI-backed credential store,
> or Linux Secret Service via the `keyring` crate) rather than in
> `config.json`. This supersedes the plaintext-storage approach used
> through v2.5.0 and earlier; on first run after upgrading, users will be
> prompted to re-authenticate Spotify so the secret can be migrated.
> `tokens.json` (access/refresh tokens) is currently **plaintext on disk**
> (see the note above) — the v2.6.4 (#65) migration from
> `tauri-plugin-store` to `token_io.rs` explicitly chose crash-safe
> atomic writes over encryption, on the basis that the tokens are
> short-lived, refreshable, and revocable.

> **Status:** As of v2.8.0, the keychain user field is **namespaced by the
> Tauri bundle identifier** (`spotify_client_secret:com.presencejam.app`)
> so side-by-side installs on the same OS user (prod, dev, beta) get
> isolated slots. v2.7.2 and earlier stored the secret under the
> unnamespaced key `spotify_client_secret`; on first read after upgrading,
> the old entry is automatically migrated forward to the namespaced slot
> and deleted. The migration is conflict-safe: if the keychain already
> holds a *different* secret, the legacy plaintext (if any) is left
> untouched and the user is directed to Settings → Reconnect to resolve.

### Logs

Application logs are written to:

```
%APPDATA%\PresenceJam\logs\
```

Logs may contain:
- Track titles and artist names (from Spotify API responses)
- Timestamps and operational messages
- Error details (including API error messages)
- Redacted profanity filter events (the original profane status is **never** written to logs)

Logs are written to the `tauri-plugin-log` default log directory. **Log retention/rotation is currently managed by the logging plugin defaults and is not user-configurable.** A previous version of this document claimed logs were "rotated daily and retained for 30 days"; that claim has been removed because no rotation code exists in the application — the v2.5.0 `logging.retention_days` config field was a no-op and has been removed in v2.6.0.

**Token responses are not written to logs (v2.6.3):** Successful Microsoft Graph token responses — which include `access_token` + `refresh_token` (~3.5 KB total, ~77 min lifetime, `Presence.ReadWrite` + 50+ scopes) — are never written to the log file in full. The `poll_teams_auth` debug log, the `start_teams_auth_device_code` info log, and the user-facing error toasts for the `complete_teams_auth` / `refresh_teams_token` / `start_teams_auth_device_code` parse-error paths all run the body through the `truncate_for_log` helper, which records only the first 256 chars + a `(…NB total)` byte-count suffix. That's enough to recognise the error envelope shape (e.g. `authorization_pending`, `slow_down`, JSON parse errors) without exposing the credential. The helper is char-boundary-safe (`body.char_indices().nth(256)`) and unit-tested against the multibyte-UTF-8 case. See [issue #62](https://github.com/Carme99/PresenceJam-Desktop/issues/62).


## Network Security

All API communication happens over **HTTPS/TLS**:

| API | Endpoint |
|-----|----------|
| Spotify Authorization | `https://accounts.spotify.com` |
| Spotify Web API | `https://api.spotify.com` |
| Microsoft Auth | `https://login.microsoftonline.com` |
| Microsoft Graph | `https://graph.microsoft.com` |

No data is sent to any third-party server other than Spotify and Microsoft Graph APIs.

## No Telemetry

PresenceJam does **not** collect or transmit:
- Usage statistics
- Crash reports
- Error reports
- Personal identifying information
- Your music listening history

The only external network requests are the Spotify and Microsoft Graph API calls required for the app to function.

## Third-Party APIs

PresenceJam uses two third-party APIs:

### Spotify Web API

- [Spotify Developer Terms](https://developer.spotify.com/terms/)
- [Spotify Privacy Policy](https://www.spotify.com/legal/privacy-policy/)
- Scope: `user-read-currently-playing`, `user-read-playback-state`

### Microsoft Graph API

- [Microsoft Services Agreement](https://www.microsoft.com/servicesagreement/)
- [Microsoft Privacy Statement](https://privacy.microsoft.com/privacystatement/)
- Scope: `Presence.ReadWrite`, `User.Read`

Review these links to understand how your data is handled by each service.

## Limitations

### Token Storage

Currently, OAuth tokens (`tokens.json`) are **plaintext JSON on disk** in `<app-config-dir>/PresenceJam/tokens.json` (written atomically by `src-tauri/src/token_io.rs`). The file is not encrypted by the app — protection is whatever the OS grants the file at creation time (typically umask 022 → 0644 on Unix-like systems; user-only default ACL on Windows). The app has access to tokens as soon as you’re logged into your OS session — there is no additional password or biometric unlock layer. See "Plaintext tokens.json" note above.

**Mitigation:** Use a strong Windows login password/PIN and enable Windows Hello or BitLocker where possible.

### No Certificate Pinning

The app does not currently implement TLS certificate pinning for API calls. This is a future improvement to consider.

## Best Practices

For a more secure experience:

1. **Revoke access** when not using the app (via Spotify app settings and Microsoft account security page)
2. **Keep Windows updated** to receive DPAPI security patches
3. **Use a password/PIN** on your Windows account — no blank login
4. **Don't share your machine** with untrusted parties while tokens are active
5. **Uninstall the app** and delete `%APPDATA%\PresenceJam` when done
6. **Rotate credentials** if you suspect compromise (Spotify Developer Dashboard → your app → Client Secrets → Reset)


## Release Pipeline Token Rotation

The release workflow (`.github/workflows/release.yml`) uses two repository secrets to publish to package managers. Both are personal access tokens (PATs) held by the maintainer and must be rotated on a 90-day cadence to limit blast radius if the token leaks through any other channel (CI logs, tap repo history, developer machine, etc.).

| Secret | Scope | Stored where | Rotation check |
|---|---|---|---|
| `HOMEBREW_TAP_TOKEN` | `contents:write` on `carme99/homebrew-tap` only (fine-grained PAT) | GitHub Actions secrets | When did the token last rotate? If >90 days, generate a new fine-grained PAT with the same scope, update the secret, revoke the old one. |
| `WINGET_TOKEN` | `contents:write` + `pull_requests:write` on `microsoft/winget-pkgs` only (fine-grained PAT) | GitHub Actions secrets | Same as above. |

**Rotation procedure:**
1. Generate a new fine-grained PAT on GitHub (Settings → Developer settings → Personal access tokens → Fine-grained tokens). Scope it to the single repository that needs write access; set the minimum required permissions (`contents:write` for the tap, plus `pull_requests:write` for winget-pkgs).
2. In the PresenceJam-Desktop repo, go to Settings → Secrets and variables → Actions. Update the secret value to the new token.
3. Revoke the old token on GitHub (Settings → Developer settings → Personal access tokens → … → Delete).
4. Trigger a dry-run of the release workflow (push a `v0.0.0-test` tag, then delete it) to confirm the new token works.
5. Record the rotation in the repo's release notes / changelog under "Internal / security".

**Why fine-grained, not classic:** A classic PAT grants the token owner full access to every repository they can see. If `HOMEBREW_TAP_TOKEN` leaks, a classic PAT lets the attacker push to PresenceJam-Desktop, the homebrew tap, and any other repo under the Carme99 account. A fine-grained PAT scoped to a single repo with `contents:write` only leaks the ability to push to that one repo.

**Why 90 days:** A compromise window of 90 days balances the operational cost of rotation against the average time-to-detection for token misuse in monitoring (per GitHub's own PAT guidance). Shorter windows (30/60 days) are acceptable if rotation can be automated; longer windows increase the blast radius of any leak.
## Open Source

PresenceJam is open source. You're encouraged to review the code yourself:

- [GitHub Repository](https://github.com/Carme99/PresenceJam-Desktop)
- Key security-sensitive files: `src-tauri/src/spotify.rs`, `src-tauri/src/teams.rs`, `src-tauri/src/polling.rs`, `src-tauri/src/profanity.rs`

Contributions that improve security are welcome.
