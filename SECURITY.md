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
| Spotify access/refresh tokens | `<app-config-dir>/PresenceJam/tokens.json` (hand-rolled atomic write via `src-tauri/src/token_io.rs`) | **AES-256-GCM ciphertext** (issue #140). The file is `b"PJENC" \| version byte (0x01) \| 12-byte random nonce \| AES-256-GCM ciphertext`; the 256-bit key is generated on first use and held in the OS keychain (Windows Credential Manager, macOS Keychain, Linux Secret Service via the `keyring` crate) under the namespaced slot `tokens_aes_key:com.presencejam.app`. See "Encrypted tokens.json" below. |
| Teams access/refresh tokens | `<app-config-dir>/PresenceJam/tokens.json` (same hand-rolled atomic write) | **AES-256-GCM ciphertext** — same as Spotify above. |
| Spotify OAuth pending state (PKCE verifier, state) | **Not persisted to disk.** Lives in `AppState::PendingSpotifyAuth.state` only (in-memory); the deep-link callback resolves it before the user closes the app. If the process is killed mid-OAuth, the user re-starts the OAuth flow and Spotify issues a fresh code. | N/A — in-memory only. |

**Encrypted tokens.json on disk (v3.0 — issue #140):** OAuth access/refresh
tokens in `tokens.json` are AES-256-GCM ciphertext. The on-disk format is
`b"PJENC" || format-version byte (0x01) || 12-byte random nonce ||
AES-256-GCM ciphertext` (see `src-tauri/src/token_io.rs::encrypt_tokens`
and `decrypt_tokens`). The version byte lets a future cipher change
co-exist with the current format: unknown versions are rejected, never
mis-decrypted. The encryption key is a random 256-bit value generated on
first use (get-or-create) and stored in the OS keychain under the
bundle-identifier-namespaced slot `tokens_aes_key:com.presencejam.app`
(`src-tauri/src/keychain.rs::get_or_create_tokens_aes_key`). The GCM tag
authenticates the file, so bit-flips / tampering fail decryption instead
of yielding garbage tokens. No plaintext JSON ever reaches the disk: the
write path encrypts before creating the temp sidecar, and the temp file +
rename source hold ciphertext only (`src-tauri/src/token_io.rs::write_tokens_atomic`).

**Plaintext → ciphertext migration (v3.0 — issue #140):** every released
version through v2.10.0 stored `tokens.json` as plaintext JSON — under the
pre-v2.6.4 `tauri-plugin-store` write path and the post-#65 `token_io.rs`
hand-rolled atomic-write path (the #65 migration fixed two unrelated bugs
— a webview IPC credential leak and mid-write store corruption — it did
not add encryption). On first read after upgrading,
`src-tauri/src/token_io.rs::read_tokens_at` detects a file starting with
`{` (legacy plaintext JSON), parses it, and immediately re-writes it
encrypted through the atomic write path, which replaces the plaintext file
and pre-clears any stale plaintext temp sidecar. A file that is neither
`PJENC`-encrypted nor plaintext JSON is rejected. A missing keychain key,
corrupt ciphertext, or GCM tag mismatch surfaces as `Err` and drives
re-authentication — the same recovery as the pre-v3.0 corrupt-file path.

**File-mode history.** Prior to v2.8.x, PresenceJam did not explicitly
set a mode on `tokens.json` (or `config.json`); under the typical umask
022, the file ended up at mode 0644 on macOS / Linux, which is
world-readable on the local machine. On Windows, the default ACL
inherited the user-only parent permissions and was not world-readable.
Starting with v2.8.x (issue #135 path A), `tokens.json` and
`config.json` are explicitly set to mode 0600 on Unix-like systems at
write time, and any pre-existing loose file is tightened on first read;
on Windows, the default ACL remains user-only and no explicit ACL
change is required. See "File permissions (v2.8.x — issue #135 path A)"
below for source-of-truth citations, and "Limitations → Token Storage"
for the residual exposure.

Full-disk encryption (BitLocker / FileVault / LUKS) remains the
strongest defense against offline-disk reads of `config.json` and of the
OS keychain itself (which holds the tokens.json decryption key and the
Spotify `client_secret`); revocation at the provider (Spotify / Microsoft
account settings) is the only way to invalidate the credentials once a
local-user compromise is suspected. See `ARCHITECTURE.md` "Storage"
section for the implementation reference.

#### File permissions (v2.8.x — issue #135 path A)

`tokens.json` and `config.json` are explicitly set to mode **0600** (owner
read/write only) on Unix-like systems and inherit the user-only default
ACL on Windows. For `tokens.json` this is defense-in-depth **on top of**
the AES-256-GCM encryption (issue #140): the ciphertext is never
world-readable at any point of the write. For `config.json` (still
plaintext JSON — it holds no credentials, only settings) the mode is the
only file-level protection. Claims tied to the source:

| Claim | Source |
|---|---|
| `tokens.json` temp sidecar is created with mode 0600 atomically and holds only AES-256-GCM ciphertext (no plaintext window) | `src-tauri/src/token_io.rs::write_tokens_atomic` — `OpenOptions::new().create_new(true).mode(0o600)` (Unix); `fs::File::create` (Windows, inherits user-only ACL); payload encrypted by `encrypt_tokens` before the temp file is opened. |
| `tokens.json` live file inherits 0600 from the rename source | POSIX `rename(2)` preserves the source mode; the source is the 0600 tmp. |
| `config.json` temp sidecar is created with mode 0600 atomically | `src-tauri/src/config.rs::atomic_write_json` — `OpenOptions::new().create_new(true).mode(0o600)` (Unix); `fs::File::create` (Windows). |
| Pre-existing loose files are tightened on read (upgrade path) | `src-tauri/src/token_io.rs::read_tokens_at` and `src-tauri/src/config.rs::load_config` — `fs::set_permissions(0o600)` after `fs::metadata` shows a non-0600 mode. Idempotent. |
| Stale `.tmp` sidecar from a prior crash is cleared before create_new (avoids `AlreadyExists` permanent save failure) | `src-tauri/src/token_io.rs::write_tokens_atomic` and `src-tauri/src/config.rs::atomic_write_json` — `fs::remove_file(&temp_path)` with `NotFound` tolerated. |
| Windows does NOT need an explicit DACL change | Windows default ACL on a new file in a user-owned directory inherits user-only access (issue #135 acceptance; verified by reading the existing `Encrypted tokens.json` paragraph above). |

**What this is NOT.** The file-mode tightening does not encrypt
`config.json` — it remains plaintext JSON on disk; only the OS-level file
mode (Unix) or default ACL (Windows) is narrowed for it. `tokens.json` is
*additionally* encrypted (AES-256-GCM, issue #140 — see "Encrypted
tokens.json" above). Full-disk encryption (BitLocker / FileVault / LUKS)
remains the strongest defense against offline-disk reads of `config.json`
and of the OS keychain itself; revocation at the provider (Spotify /
Microsoft account settings) remains the only way to invalidate the
credentials once a local-user compromise is suspected.

**Decision recorded on issue #135:** Path A (file-mode tightening) was
chosen over Path B (full encryption with keychain-stored key) for v2.8.x
because it is a small, low-risk, cross-platform change that closes the
umask-022 → 0644 exposure surface without introducing a new crypto
dependency or breaking the atomic-write guarantees of `token_io.rs` and
`config.rs`. Path B landed in v3.0 for `tokens.json` (issue #140:
AES-256-GCM with a keychain-stored key — see "Encrypted tokens.json"
above). `config.json` intentionally remains plaintext JSON: it holds no
credentials (the Spotify `client_secret` lives in the keychain), only
non-secret settings, and encrypting it would add key-management burden
without closing a credential-exposure path.

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

`tokens.json` contains (AES-256-GCM ciphertext on disk — see "Encrypted
tokens.json" above; the plaintext payload is):

- Spotify access token + refresh token (`SpotifyTokens` JSON object)
- Teams access token + refresh token (`TeamsTokens` JSON object)

**⚠️ The `config.json` file (now) contains no secrets after a successful
v2.6.0+ migration**. As of v2.8.x (issue #135 path A), both `config.json`
and `tokens.json` are explicitly set to mode 0600 on Unix-like systems
(on Windows, the default ACL is already user-only). `config.json` remains
plaintext JSON (file-mode narrowing only); `tokens.json` is *additionally*
encrypted at rest with AES-256-GCM since v3.0 (issue #140 — see
"Encrypted tokens.json" above). Recommendations, ordered by effort:

1. Enable full-disk encryption on the OS (BitLocker on Windows, FileVault
   on macOS, LUKS on Linux). This is the strongest defense against
   physical-disk theft and protects the OS keychain — which holds the
   tokens.json decryption key and the Spotify `client_secret` — against an
   offline read of the disk.
2. Do not run PresenceJam on a machine whose user account is shared with
   untrusted parties; on Unix-like systems, any other process running
   under the same user can read `config.json` and can request the
   tokens.json decryption key from the OS keychain (the keychain is
   unlocked while the user is logged into the graphical session).
3. **Revoke your Spotify and Teams app authorizations** from those
   providers' settings if you suspect the local machine is compromised.
   This is the only way to invalidate the credentials from the provider
   side; revocation is faster than waiting for the tokens to expire
   (Spotify refresh tokens are valid up to 6 months — on `invalid_grant`
   the app discards them and triggers re-auth).

> **Status:** As of v2.6.0, the Spotify `client_secret` is stored in the
> **OS keychain** (macOS Keychain, Windows DPAPI-backed credential store,
> or Linux Secret Service via the `keyring` crate) rather than in
> `config.json`. This supersedes the plaintext-storage approach used
> through v2.5.0 and earlier; on first run after upgrading, users will be
> prompted to re-authenticate Spotify so the secret can be migrated.
> `tokens.json` (access/refresh tokens) was stored **as plaintext JSON
> on disk in every released version through v2.10.0** (see the note
> above) — the v2.6.4 (#65) migration from `tauri-plugin-store` to
> `token_io.rs` was NOT introducing encryption; it was fixing two
> pre-existing bugs (webview IPC leak + crash-corruption) without adding
> encryption at any point.

> **Status:** As of **v3.0 (issue #140)**, `tokens.json` is **AES-256-GCM
> ciphertext** at rest. The 256-bit key is generated on first use and
> stored in the OS keychain under the namespaced slot
> `tokens_aes_key:com.presencejam.app`. Existing plaintext `tokens.json`
> files (≤ v2.10.0) are migrated to ciphertext on first read (parsed,
> then re-written encrypted via the atomic write path). A missing key or
> corrupt/tampered ciphertext causes the app to discard the tokens and
> re-authenticate rather than silently fall back to plaintext.

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

**Token responses are not written to logs (v2.6.3):** Successful Microsoft Graph token responses — which include `access_token` + `refresh_token` (~3.5 KB total, ~77 min lifetime, `Presence.ReadWrite` + `offline_access` + 50+ scopes) — are never written to the log file in full. The `poll_teams_auth` debug log, the `start_teams_auth_device_code` info log, and the user-facing error toasts for the `refresh_teams_token` / `start_teams_auth_device_code` parse-error paths all run the body through the `truncate_for_log` helper, which records only the first 256 chars + a `(…NB total)` byte-count suffix. That's enough to recognise the error envelope shape (e.g. `authorization_pending`, JSON parse errors) without exposing the credential — `slow_down` is also handled, though it is RFC 8628 §3.5-only: Microsoft's device-code error table enumerates only `authorization_pending`, `authorization_declined`, `bad_verification_code`, and `expired_token`. The helper is char-boundary-safe (`body.char_indices().nth(256)`) and unit-tested against the multibyte-UTF-8 case. See [issue #62](https://github.com/Carme99/PresenceJam-Desktop/issues/62).


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
- Scope: `Presence.ReadWrite`, `offline_access` (the device-code sign-in requests exactly `Presence.ReadWrite offline_access`; the unused `User.Read` scope was dropped in the #151 fix)

Review these links to understand how your data is handled by each service.

## Limitations

### Token Storage

Since v3.0 (issue #140), OAuth tokens (`tokens.json`) are **AES-256-GCM
ciphertext on disk** in `<app-config-dir>/PresenceJam/tokens.json`
(encrypted and written atomically by
`src-tauri/src/token_io.rs::write_tokens_atomic`; decrypted by
`read_tokens_at`). The 256-bit key is generated on first use and stored in
the OS keychain (Windows Credential Manager, macOS Keychain, Linux Secret
Service) under the namespaced slot `tokens_aes_key:com.presencejam.app`
(`src-tauri/src/keychain.rs::get_or_create_tokens_aes_key`). Legacy
plaintext files (≤ v2.10.0) are migrated on first read. See "Encrypted
tokens.json" under "Data Storage" for the format and source citations.

**Residual exposure:** the encryption key is protected by the OS keychain,
not by an app-level password — any process running in the same logged-in
OS user session can request the key from the unlocked keychain (on Linux,
the Secret Service is unlocked at graphical login). Tokens are also in
plaintext in the app's memory while it runs. `config.json` remains
plaintext JSON (mode 0600; it holds no credentials — the Spotify
`client_secret` lives in the keychain).

**Mitigation (file-mode tightening, v2.8.x — issue #135 path A):**
`tokens.json` and `config.json` are explicitly set to mode 0600 on
Unix-like systems (and inherit the user-only default ACL on Windows) at
write time, and any pre-existing loose file is tightened on first read.
For `tokens.json` this is defense-in-depth on top of the AES-256-GCM
encryption (the ciphertext is never world-readable); for `config.json` it
is the only file-level protection. See "File permissions (v2.8.x)" under
"Data Storage" for source citations.

**Mitigation (broader):** Use a strong Windows login password/PIN and
enable Windows Hello or BitLocker where possible.

### No Certificate Pinning

The app does not currently implement TLS certificate pinning for API calls. This is a future improvement to consider.

## Best Practices

For a more secure experience:

1. **Revoke access** when not using the app (via Spotify app settings and Microsoft account security page)
2. **Keep Windows updated** to receive DPAPI security patches
3. **Use a password/PIN** on your Windows account — no blank login
4. **Don't share your machine** with untrusted parties while tokens are active
5. **Uninstall the app** and delete `%APPDATA%\PresenceJam` when done
6. **Rotate credentials** if you suspect compromise (Spotify Developer Dashboard → your app overview page → **ROTATE**)


## Release Pipeline

### Supply-chain hardening (SHA-pinned actions)

Every third-party GitHub Action used by `.github/workflows/ci.yml` and
`.github/workflows/release.yml` is pinned to a **full commit SHA** (with the
upstream version noted in a trailing comment), so a compromised or hijacked
tag on the action's own repository cannot change what executes here.
Reviewers should treat any new `uses:` entry that is not SHA-pinned as a
security regression.

### Build provenance attestation

Since v4.0.0, the release workflow generates a **SLSA build provenance
attestation** for every packaged artifact (macOS DMG, Windows MSI, Linux
`.deb`/`.AppImage` and their updater companions) via
[actions/attest-build-provenance](https://github.com/actions/attest-build-provenance)
(itself SHA-pinned). The attestation is a signed DSSE document produced by
GitHub's artifact-attestation infrastructure; it binds the artifact's
SHA-256 digest to the exact workflow run, repository, and commit that built
it. It **supplements** — and does not replace — the minisign `.sig` files
that the Tauri auto-updater verifies. The workflow grants itself only the
minimal scopes needed for this (`id-token: write` + `attestations: write`),
scoped to the build job alone.

**Verifying a downloaded artifact:**

1. Download the artifact from the official release page.
2. Run (requires GitHub CLI `gh` >= 2.63):

   ```sh
   gh attestation verify PresenceJam-v4.0.0.msi --repo Carme99/PresenceJam-Desktop
   ```

`gh attestation verify` fetches all attestations recorded for the file's
SHA-256 digest in the repository's attestation store, verifies the DSSE
signature chain (Sigstore Fulcio certificate with Rekor transparency-log
inclusion), and reports whether the file was produced by an unmodified run
of this repository's release workflow. Exit status 0 means verified; any
digest, signature, or repository mismatch exits non-zero. Example output:

```text
Loaded 1 digest for PresenceJam-v4.0.0.msi ✓
Successfully verified attestations:
Predicate types: [https://slsa.dev/provenance/v1]
```

If verification fails for a file obtained anywhere other than the official
[releases page](https://github.com/Carme99/PresenceJam-Desktop/releases),
treat it as untrusted and re-download from the release page.

## Release Pipeline Token Rotation

The release workflow (`.github/workflows/release.yml`) uses two repository secrets to publish to package managers. Both are personal access tokens (PATs) held by the maintainer and must be rotated on a 90-day cadence to limit blast radius if the token leaks through any other channel (CI logs, tap repo history, developer machine, etc.).

| Secret | Scope | Stored where | Rotation check |
|---|---|---|---|
| `HOMEBREW_TAP_TOKEN` | `contents:write` on `carme99/homebrew-tap` only (fine-grained PAT) | GitHub Actions secrets | When did the token last rotate? If >90 days, generate a new fine-grained PAT with the same scope, update the secret, revoke the old one. |
| `WINGET_TOKEN` | `public_repo` + `workflow` on `Carme99/winget-pkgs` fork only (classic PAT; `komac sync-fork` then PR fork → `microsoft/winget-pkgs`) | GitHub Actions secrets | Same as above. |

**Rotation procedure:**
1. Generate new PATs on GitHub: for `HOMEBREW_TAP_TOKEN` create a **fine-grained PAT** (Settings → Developer settings → Personal access tokens → Fine-grained tokens) scoped to `carme99/homebrew-tap` with `contents:write`; for `WINGET_TOKEN` create a **classic PAT** (Settings → Developer settings → Personal access tokens → Tokens (classic)) with scopes `public_repo` + `workflow` on the `Carme99/winget-pkgs` fork (vedantmgoyal2009/winget-releaser requires classic + workflow scope; fine-grained 422s with "workflow scope required"). See table above for per-secret scope.
2. In the PresenceJam-Desktop repo, go to Settings → Secrets and variables → Actions. Update each secret value to the new token.
3. Revoke the old tokens on GitHub (Settings → Developer settings → Personal access tokens → … → Delete).
4. Trigger a dry-run of the release workflow (push a `v0.0.0-test` tag, then delete it) to confirm the new tokens work.
5. Record the rotation in the repo's release notes / changelog under "Internal / security".

**Why fine-grained where possible, not classic everywhere:** A classic PAT grants the token owner full access to every repository they can see. If `HOMEBREW_TAP_TOKEN` leaks, a classic PAT lets the attacker push to PresenceJam-Desktop, the homebrew tap, and any other repo under the Carme99 account. A fine-grained PAT scoped to a single repo with `contents:write` only leaks the ability to push to that one repo. `WINGET_TOKEN` is the exception: the winget releaser action only supports classic PATs with `workflow` scope (fine-grained returns 422), so it stays classic but is scoped to the `Carme99/winget-pkgs` fork, not `microsoft/winget-pkgs`, and is rotated on the same 90-day cadence.

**Why 90 days:** A compromise window of 90 days balances the operational cost of rotation against the average time-to-detection for token misuse in monitoring (per GitHub's own PAT guidance). Shorter windows (30/60 days) are acceptable if rotation can be automated; longer windows increase the blast radius of any leak.
## Open Source

PresenceJam is open source. You're encouraged to review the code yourself:

- [GitHub Repository](https://github.com/Carme99/PresenceJam-Desktop)
- Key security-sensitive files: `src-tauri/src/spotify.rs`, `src-tauri/src/teams.rs`, `src-tauri/src/polling.rs`, `src-tauri/src/profanity.rs`

Contributions that improve security are welcome.
