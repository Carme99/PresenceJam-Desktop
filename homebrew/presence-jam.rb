# Homebrew CASK template for carme99/homebrew-tap (rendered to
# Casks/presence-jam.rb there).
#
# The release job (see .github/workflows/release.yml, job `homebrew`)
# substitutes the three placeholders with the version, DMG URL and DMG SHA256,
# so the tap's content stays reviewable in a normal diff instead of living
# inside the workflow YAML.
#
# Why a cask and not a formula (issue #898): the formula copied the DMG's .app
# into its own version-scoped keg under the Homebrew prefix, so nothing landed
# in /Applications. Launchpad and Spotlight never saw the app, macOS had no
# handler for the presencejam:// callback (the Spotify PKCE redirect) until the
# user launched the bundle by hand out of the Cellar, and the keg path changed
# on every upgrade, invalidating any LaunchAgent the app had written.
# `app "PresenceJam.app"` installs into /Applications through Homebrew's cask
# machinery, which is what registers the URL scheme with LaunchServices.
#
# Migrating from the formula: `brew uninstall presence-jam` first, then
# `brew install --cask carme99/tap/presence-jam`. The release job deletes the
# stale root-level presence-jam.rb from the tap in the same commit that writes
# this cask, because a tap holding both a formula and a cask of the same name
# is ambiguous. An autostart entry created while the app ran from the Cellar
# points at that old path and has to be re-created (Settings -> Start at login).
cask "presence-jam" do
  version "__VERSION__"
  sha256 "__SHA256__"

  url "__URL__"
  name "PresenceJam"
  desc "Syncs Spotify playback to your Microsoft Teams status"
  homepage "https://github.com/Carme99/PresenceJam-Desktop"

  # The release matrix builds only aarch64-apple-darwin and latest.json
  # advertises only darwin-aarch64 (see .github/workflows/release.yml), so an
  # Intel Mac can neither run the arm64 bundle nor ever self-heal through the
  # updater. Refuse the install instead of putting a bundle that cannot run
  # into /Applications. See issue #610. Tauri 2 needs macOS 10.15+.
  depends_on arch: :arm64
  depends_on macos: ">= :catalina"

  # The app owns its own updates: tauri-plugin-updater verifies and applies the
  # payload named in latest.json, replacing the bundle in place. Declaring that
  # to Homebrew is what makes a cask the right channel for a self-updating app
  # — there is no versioned keg for the two update paths to fight over.
  auto_updates true

  app "PresenceJam.app"

  zap trash: [
    "~/Library/Application Support/PresenceJam",
    "~/Library/Application Support/com.presencejam.app",
    "~/Library/Caches/com.presencejam.app",
    "~/Library/LaunchAgents/PresenceJam.plist",
    "~/Library/Logs/com.presencejam.app",
    "~/Library/Preferences/com.presencejam.app.plist",
    "~/Library/Saved Application State/com.presencejam.app.savedState",
  ]

  caveats <<~EOS
    PresenceJam updates itself: Settings -> "Check for updates" fetches the
    signed payload named in the project's latest.json. This cask declares
    auto_updates, so `brew upgrade` leaves the bundle in /Applications alone.

    `brew uninstall --zap presence-jam` removes the config directory
    (~/Library/Application Support/PresenceJam), the encrypted tokens
    (~/Library/Application Support/com.presencejam.app) and the logs
    (~/Library/Logs/com.presencejam.app). The AES key that decrypts tokens.json
    lives in your login Keychain under the "presencejam" service, which
    Homebrew cannot remove — delete those entries in Keychain Access if you want
    them gone too.
  EOS
end
