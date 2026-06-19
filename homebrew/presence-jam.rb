class PresenceJam < Formula
  desc "Spotify to Teams Status Sync"
  homepage "https://github.com/Carme99/PresenceJam-Desktop"
  url "__URL__"
  sha256 "__SHA256__"
  version "__VERSION__"
  license "MIT"

  # Tauri-built macOS DMG. The Tauri 2.x bundler wraps the .app bundle
  # in an outer `PresenceJam/` subfolder (along with an `Applications`
  # symlink for the drag-to-install UX), so the mount-root layout is:
  #
  #   /Volumes/PresenceJam/         (the volume, --volname PresenceJam)
  #     PresenceJam/                (outer subfolder, Tauri bundler quirk)
  #       PresenceJam.app/          ← what we want
  #       Applications              (symlink to /Applications)
  #
  # Verified by extracting the v2.7.1 DMG with 7z — the .app is one
  # level deeper than the original formula expected. The original
  # `prefix.install "PresenceJam.app"` failed with `Errno::ENOENT`
  # because it was looking at the buildpath root, not the subfolder.

  def install
    prefix.install "PresenceJam/PresenceJam.app"
  end

  test do
    assert_predicate prefix/"PresenceJam.app", :exist?
  end
end
