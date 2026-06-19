class PresenceJam < Formula
  desc "Spotify to Teams Status Sync"
  homepage "https://github.com/Carme99/PresenceJam-Desktop"
  url "__URL__"
  sha256 "__SHA256__"
  version "__VERSION__"
  license "MIT"

  # Tauri-built macOS DMG. The HFS+ mount layout has been inconsistent
  # across releases and brew versions. The actual HFS+ root on a Linux
  # test-mount of the v2.7.1 DMG contains PresenceJam.app/ directly
  # (verified via dmg2img + hfsplus mount), but brew on Jack's macOS
  # failed both `prefix.install "PresenceJam.app"` and
  # `prefix.install "PresenceJam/PresenceJam.app"` with Errno::ENOENT.
  #
  # This fix probes for the .app at both the documented layouts using
  # absolute paths from `buildpath` (cwd-independent), and if neither
  # matches, prints the actual buildpath contents as a diagnostic.
  # See carme99/homebrew-tap commit 4de3650 for the matched layout.

  def install
    candidates = [
      buildpath/"PresenceJam.app",
      buildpath/"PresenceJam/PresenceJam.app",
    ]
    app_path = candidates.find(&:directory?)
    unless app_path
      odie <<~EOS
        PresenceJam.app not found at #{buildpath}.
        Buildpath contents:
        #{Dir.glob("#{buildpath}/*").map { |p| "  #{p}" }.join("\n")}
        Expected one of:
          #{candidates.join("\n          ")}
      EOS
    end
    prefix.install app_path
  end

  test do
    assert_predicate prefix/"PresenceJam.app", :exist?
  end
end
