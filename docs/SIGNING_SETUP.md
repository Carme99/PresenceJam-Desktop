# Code signing & notarization setup (issue #54)

This document captures the **one-time setup steps** required to make the
release workflow's signing steps succeed. Until all of these are in place,
the next `v*` tag push will fail at the `Sign and notarize macOS DMG` or
`Sign Windows MSI` step with a clear `::error::` line — the release will
**not** be created.

## 1. Apple Developer ID Application certificate (macOS)

1. In Xcode → Settings → Accounts, add your Apple ID and select your team.
2. Request a **Developer ID Application** certificate
   (Xcode → Manage Certificates → "+" → Developer ID Application).
3. Export the cert from Keychain Access as `developer_id_application.p12`
   (set a password you'll remember — it becomes a secret).
4. Base64-encode it:
   ```bash
   base64 -i developer_id_application.p12 -o developer_id_application.p12.b64
   ```
5. In the GitHub repo: Settings → Secrets and variables → Actions →
   New repository secret. Add the following:
   - `APPLE_CERTIFICATE` — paste the contents of `developer_id_application.p12.b64`
   - `APPLE_CERTIFICATE_PASSWORD` — the password you set on the .p12
   - `APPLE_KEYCHAIN_PASSWORD` — any random string (used to lock the
     temporary keychain the workflow creates on the runner; not the .p12
     password)
   - `APPLE_SIGNING_IDENTITY` — the value shown in
     `security find-identity -p codesigning -v` on your Mac, formatted as
     `Developer ID Application: <Your Name> (<10-char TeamID>)`
6. From `appleid.apple.com` → App-Specific Passwords, generate a password
   scoped to "GitHub Actions (PresenceJam)". Add the secrets:
   - `APPLE_ID` — the email of your Apple Developer account
   - `APPLE_PASSWORD` — the app-specific password string (NOT your Apple ID password)
   - `APPLE_TEAM_ID` — the 10-character Team ID (Apple Developer → Membership)

After all 7 are set, the next macOS release build will sign + notarize
+ staple the DMG. `xcrun notarytool history` will show the submission;
`xcrun stapler validate <dmg>` on a downloaded copy will pass.

## 2. Windows code signing certificate (MSI)

1. Purchase a code-signing cert from a public CA (Sectigo, DigiCert,
   GlobalSign) or use Azure Trusted Signing. **Self-signed won't work**
   for SmartScreen.
2. Export as `codesign.pfx` (set a password).
3. Base64-encode it:
   ```powershell
   [Convert]::ToBase64String([IO.File]::ReadAllBytes('codesign.pfx')) | Set-Content codesign.pfx.b64
   ```
4. Add the secrets:
   - `WINDOWS_CERTIFICATE` — contents of `codesign.pfx.b64`
   - `WINDOWS_CERTIFICATE_PASSWORD` — the password you set on the .pfx

After both are set, the next Windows release build will sign the MSI
with SHA-256 + DigiCert timestamp. SmartScreen will still warn on
fresh certs but quiet down after a reputation window (~a few hundred
downloads or EV cert, which has no warning from day 1).

## Why "fail loudly" is the right behavior

A v* tag push that lands without signing is the **opposite** of the
issue this PR fixes. If you want to ship an unsigned build while you're
still sorting out the certs, branch a `v*` tag off `main` and edit the
`Sign and notarize macOS DMG` step to `if: false` for that one release.
Revert before the next normal release.
