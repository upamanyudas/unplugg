cask "unplugg" do
  arch arm: "arm64", intel: "x86_64"

  # the release workflow writes version and sha256
  version "1.2.0"
  sha256 arm:          "b6c95a4d115826e06ac33210bb7faf1e62f4ce7acfb5184889793ff36575e69d",
         intel:        "81c52d44edf493a3a8d0c41283c8e8d473b639b6692013b2dbb3fd0157a3a3b9",
         arm64_linux:  "8ae246a6cbcd68278655251ca019e99afac7a096949543f427b20509e883cb99",
         x86_64_linux: "8ec512eb67bed8706cf14a3c26102a43821c83f5cc60bcce74c2f569ea26fa09"

  on_macos do
    # one universal dmg; arm and intel above carry its one sha256
    url "https://github.com/upamanyudas/unplugg/releases/download/v#{version}/unplugg.dmg"
  end
  on_linux do
    url "https://github.com/upamanyudas/unplugg/releases/download/v#{version}/unplugg-linux-#{arch}.AppImage"
  end

  name "unplugg"
  desc "Alarm that guards a laptop while you step away"
  homepage "https://github.com/upamanyudas/unplugg"

  on_macos do
    auto_updates true

    app "unplugg.app"

    zap trash: [
      "~/Library/Application Support/unplugg",
      "~/Library/LaunchAgents/com.unplugg.launch.plist",
    ]

    caveats <<~EOS
      unplugg isn't notarized. The first time, open it from
      System Settings › Privacy & Security › Open Anyway.
    EOS
  end

  on_linux do
    app_image "unplugg-linux-#{arch}.AppImage"

    zap trash: [
      "~/.config/autostart/unplugg.desktop",
      "~/.config/unplugg",
    ]
  end
end
