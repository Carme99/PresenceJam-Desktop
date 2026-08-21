# Windows Update Chain — v3.2.0 Rescue Checklist

Stuck fleet: Windows clients on ≤ v3.1.0 that never received an auto-update because the v3.1.0 release uploaded `PresenceJam_3.1.0_x64_en-US.msi` (bundler filename) while `latest.json` referenced `PresenceJam-v3.1.0.msi` (renamed). Tauri's updater treats a 404 on the platform URL as "no update" — silently, no error — so the fleet remained pinned.

v3.2.0 fixes the chain by uploading the renamed MSI (`PresenceJam-<tag>.msi` + `.sig`) and verifying the three updater URLs resolve before publishing `latest.json`.

## Preconditions (all must pass before declaring v3.2.0 healthy)

- [ ] Tag `v3.2.0` exists and the GitHub Release is published (`gh release view v3.2.0` shows Published).
- [ ] `latest.json` is present as a Release asset and the static endpoint returns 200:
  ```bash
  curl -I -s https://github.com/Carme99/PresenceJam-Desktop/releases/latest/download/latest.json | head -n1
  # expect: HTTP/2 200
  gh api repos/Carme99/PresenceJam-Desktop/releases/latest --jq '.assets[].name | select(. == "latest.json")'
  ```
- [ ] Each platform URL in `latest.json` returns 200 (not 404):
  ```bash
  VERSION=v3.2.0
  for url in \
    "https://github.com/Carme99/PresenceJam-Desktop/releases/download/${VERSION}/PresenceJam-${VERSION}.app.tar.gz" \
    "https://github.com/Carme99/PresenceJam-Desktop/releases/download/${VERSION}/PresenceJam-${VERSION}.msi" \
    "https://github.com/Carme99/PresenceJam-Desktop/releases/download/${VERSION}/PresenceJam-linux-amd64.AppImage"; do
    echo -n "$url: "; curl -I -s "$url" | head -n1
  done
  # all three must be 200; a 404 here is a silent no-update for that platform (see note below).
  ```
- [ ] `.sig` contents match the signatures embedded in `latest.json`:
  ```bash
  VERSION=v3.2.0
  gh release download "$VERSION" --repo Carme99/PresenceJam-Desktop --pattern "*.sig" --pattern "latest.json" --dir /tmp/pj-sigs --clobber
  jq -r '.platforms["darwin-aarch64"].signature' /tmp/pj-sigs/latest.json 2>/dev/null || jq -r '.platforms["darwin-aarch64"].signature' latest.json
  # compare each sig file's raw content to the corresponding jq signature field:
  diff <(cat /tmp/pj-sigs/PresenceJam-${VERSION}.app.tar.gz.sig) <(jq -r '.platforms["darwin-aarch64"].signature' /tmp/pj-sigs/latest.json) && echo "darwin sig ok"
  diff <(cat /tmp/pj-sigs/PresenceJam-${VERSION}.msi.sig)        <(jq -r '.platforms["windows-x86_64"].signature' /tmp/pj-sigs/latest.json) && echo "windows sig ok"
  diff <(cat /tmp/pj-sigs/PresenceJam-${VERSION}.AppImage.sig)   <(jq -r '.platforms["linux-x86_64"].signature' /tmp/pj-sigs/latest.json) && echo "linux sig ok"
  ```
- [ ] `SHA256SUMS.txt` is present in the Release and lists all artefacts:
  ```bash
  gh api repos/Carme99/PresenceJam-Desktop/releases/tags/v3.2.0 --jq '.assets[].name' | grep -q SHA256SUMS.txt && echo "SHA256SUMS present"
  ```

## Verification Steps (on a stuck Windows host ≤ v3.1.0)

1. Confirm the installed version is ≤ v3.1.0 (About view or `presencejam --version` if exposed; otherwise check `tokens.json` location or Add/Remove Programs entry).
2. Trigger an update check: restart the app (it calls `check()` on startup) and observe the UpdatePrompt banner. If no banner appears with v3.2.0 live, capture the endpoint response:
   ```bash
   curl -s https://github.com/Carme99/PresenceJam-Desktop/releases/latest/download/latest.json | jq .
   # must contain version "3.2.0" and platforms.windows-x86_64.url ending in PresenceJam-v3.2.0.msi
   ```
3. On the host, verify the MSI URL is fetchable (not 404):
   ```powershell
   Invoke-WebRequest -Method Head -Uri https://github.com/Carme99/PresenceJam-Desktop/releases/download/v3.2.0/PresenceJam-v3.2.0.msi | Select-Object StatusCode
   # expect 200; 404 means latest.json is stale or the Release never received the renamed MSI (re-run the release workflow)
   ```
4. If the banner shows "Update v3.2.0 available", click **Download & Install** and allow the updater to run `downloadAndInstall()` → `relaunch_app`. Confirm the post-restart version is `3.2.0`.
5. If the auto-update still does not appear, manually install `PresenceJam-v3.2.0.msi` from the Release page; this heals the updater chain for all future releases.

## Important — Tauri static JSON 404 behaviour

Tauri's `tauri-plugin-updater` treats a 404 on the configured endpoint (`…/releases/latest/download/latest.json`) or on any platform artifact URL as **"no update available"** — no error is surfaced to the user. This is by design for the static JSON schema but means a broken URL silently pins the fleet. A 404 must never ship. The release workflow's **Verify latest.json assets in release** step (gh api check) enforces this; if it fails, fix the `bundle_path` / rename steps and re-run the workflow before clients poll.

## References

- Release workflow: `.github/workflows/release.yml` (build matrix, SHA256SUMS, latest.json generation + verification).
- Updater config: `src-tauri/tauri.conf.json` → `plugins.updater.endpoints`.
- Issue context: #204 (Windows bundle_path 404), #205 (this checklist).

