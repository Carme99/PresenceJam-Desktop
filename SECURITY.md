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

| Token Type | Storage Location | Encryption |
|-----------|----------------|------------|
| Spotify access/refresh tokens | `tauri-plugin-store` (tokens store) | DPAPI on Windows |
| Teams access/refresh tokens | `tauri-plugin-store` (tokens store) | DPAPI on Windows |

**DPAPI (Windows):** Tokens are encrypted using Windows Data Protection API, which binds encryption to the current Windows user account. This means tokens cannot be extracted or read by other user accounts on the same machine, or by someone who steals the hard drive but doesn't have your login credentials.

### Configuration

App settings are stored in plain JSON:

```
%APPDATA%\PresenceJam\config.json
%APPDATA%\PresenceJam\credentials.json
```

These files contain:
- Spotify Client ID and Client Secret (from your Spotify Developer app)
- Status format template (`status_format`)
- Profanity filter settings (`profanity_filter`, `profanity_placeholder`)
- Polling configuration
- Logging preferences

**⚠️ These files are not encrypted.** If you share your machine with untrusted parties, consider revoking your Spotify app credentials when you're done using PresenceJam.

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

Logs are **rotated daily** and **retained for 30 days** by default. You can reduce retention in settings.

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
