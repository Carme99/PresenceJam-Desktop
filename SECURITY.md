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

### Tokens

|| Token Type | Storage Location | Encryption ||
|-----------|----------------|------------|
| Spotify access/refresh tokens | `tauri-plugin-store` (`tokens.json`) | DPAPI on Windows; Keychain on macOS |
| Teams access/refresh tokens | `tauri-plugin-store` (`tokens.json`) | DPAPI on Windows; Keychain on macOS |
| Spotify OAuth pending state (verifier, state) | `tauri-plugin-store` (`tokens.json`) | Same as tokens — cleared after auth completes |

**Pending-state expiry (v2.5.1):** Each pending-auth record carries an `expires_at` timestamp. The expiry is now re-checked at submit time (i.e. when the deep-link callback is delivered), not only at startup when the record is first loaded from disk. A pending-auth record older than the configured TTL is treated as expired and discarded before the code is sent to Spotify, preventing stale OAuth codes from reaching the token endpoint and surfacing as a generic 400.

**DPAPI (Windows):** Tokens are encrypted using Windows Data Protection API, which binds encryption to the current Windows user account. This means tokens cannot be extracted or read by other user accounts on the same machine, or by someone who steals the hard drive but doesn't have your login credentials.

**Keychain (macOS):** Tokens are stored in the macOS Keychain, tied to the current user account.

### Configuration

App settings are stored in plain JSON:

```
%APPDATA%\PresenceJam\config.json
%APPDATA%\PresenceJam\tokens.json
```

These files contain:
- Spotify Client ID and Client Secret (from your Spotify Developer app) — stored **unencrypted** in `config.json`
- Spotify access/refresh tokens — stored **encrypted** (DPAPI/Keychain) in `tokens.json` via tauri-plugin-store
- Teams access/refresh tokens — stored **encrypted** (DPAPI/Keychain) in `tokens.json` via tauri-plugin-store
- Status format template (`status_format`)
- Profanity filter settings (`profanity_filter`, `profanity_placeholder`)
- Polling configuration
- Logging preferences

**⚠️ The `config.json` file is not encrypted.** If you share your machine with untrusted parties, consider revoking your Spotify app credentials when you're done using PresenceJam. The `tokens.json` file is encrypted to your user account via DPAPI (Windows) or Keychain (macOS).

> **Status:** A previous version of this document implied `config.json` was encrypted. As of v2.5.1, the Spotify `client_secret` is still stored in **plaintext** in `config.json`. Migration to OS-keychain storage for the secret is tracked separately in [issue #9](https://github.com/Carme99/PresenceJam-Desktop/issues/9) and is **deferred** to a future release.

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

Logs are written to the `tauri-plugin-log` default log directory. **Log retention/rotation is currently managed by the logging plugin defaults and is not user-configurable.** A previous version of this document claimed logs were "rotated daily and retained for 30 days"; that claim has been removed because no rotation code exists in the application — the v2.5.0 `logging.retention_days` config field was a no-op and has been removed in v2.5.1.


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

Currently, tokens are stored via `tauri-plugin-store` with DPAPI encryption on Windows. There is no password or biometric unlock — the app has access to tokens as soon as you're logged into your Windows session.

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

## Open Source

PresenceJam is open source. You're encouraged to review the code yourself:

- [GitHub Repository](https://github.com/Carme99/PresenceJam-Desktop)
- Key security-sensitive files: `src-tauri/src/spotify.rs`, `src-tauri/src/teams.rs`, `src-tauri/src/polling.rs`, `src-tauri/src/profanity.rs`

Contributions that improve security are welcome.
