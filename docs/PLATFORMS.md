# Supported platforms

What the release pipeline actually builds and serves — the build matrix in
`.github/workflows/release.yml` and the `platforms` keys in the generated
`latest.json`. Anything not listed here is unsupported: it may run, but no
build, package or update path is produced for it.

| Platform | Architecture | Shipped packages | Update path | Caveats |
|----------|--------------|------------------|-------------|---------|
| Windows 10 / 11 | x64 | `PresenceJam-<tag>.msi` (per-user install) | in-app updater, `windows-x86_64` | Builds are unsigned — SmartScreen warns on first install. |
| macOS | Apple Silicon (arm64) **only** | `PresenceJam-macos.dmg`; Homebrew `brew install carme99/tap/presence-jam` | in-app updater, `darwin-aarch64` | Intel Macs are **unsupported**: the matrix builds only `aarch64-apple-darwin` and `latest.json` advertises only `darwin-aarch64`, so an Intel Mac can neither launch the build nor self-heal through the updater. `homebrew/presence-jam.rb` declares `depends_on arch: :arm64` and refuses the install rather than copying a bundle that cannot run. Builds are unsigned — see README's macOS first-run note. |
| Linux (glibc) | x86_64 | `PresenceJam-linux-amd64.deb`, `PresenceJam-linux-amd64.AppImage` | in-app updater, `linux-x86_64` (the AppImage payload) | A running Secret Service keyring daemon is a hard requirement. Non-glibc distributions (Alpine, Void) are unsupported. GNOME/Wayland needs the AppIndicator extension for the tray icon, and the AppImage ships no launcher entry of its own. |

Notes:

- The three updater keys in the table are the only ones published. A platform
  missing from `latest.json` cannot self-update even if a build exists for it.
- Changing the build matrix or the `platforms` block means updating this page in
  the same pull request, so the two cannot drift apart.
- Linux tray requirements are in
  [TROUBLESHOOTING.md — Linux: no tray icon and no window](../TROUBLESHOOTING.md#linux-no-tray-icon-and-no-window),
  the keyring requirement in [SETUP.md — Linux: System Keyring Required](../SETUP.md#linux-system-keyring-required),
  and the AppImage launcher recipe in [README.md](../README.md#linux-install).
