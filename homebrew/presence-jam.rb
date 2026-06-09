class PresenceJam < Formula
  desc "Spotify to Teams Status Sync"
  homepage "https://github.com/Carme99/PresenceJam-Desktop"
  url "__URL__"
  sha256 "__SHA256__"
  version "__VERSION__"
  license "MIT"

  # Tauri-built macOS DMG. The DMG handler exposes the mount's root
  # as the buildpath, so the .app bundle is at buildpath/PresenceJam.app
  # and we copy it into the keg.

  def install
    prefix.install "PresenceJam.app"
  end

  test do
    assert_predicate prefix/"PresenceJam.app", :exist?
  end
end
