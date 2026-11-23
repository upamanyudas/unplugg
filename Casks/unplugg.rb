cask "unplugg" do
  arch arm: "arm64", intel: "x86_64"

  # the release workflow writes version and sha256
  version "1.1.0"
  sha256 arm:          "b79d29c2832f08f722a3b2d9047ae79da3afd3a6efc1c748d868efd6dcde8aca",
         intel:        "ee51ab62c0958282d22b5c135a14b3e95bbbefaffbcaf73e08e70cef2a3f940a",
         arm64_linux:  "dcefcb0ba3ada1fd0a9dc8e27c867f62fa3d0704b2b6e00a93307305f4dd0f45",
         x86_64_linux: "2be40fa48c542929b47466e040fe16342dff13f50538ec399a559b2cbadb5f17"

  on_macos do
    url "https://github.com/upamanyudas/unplugg/releases/download/v#{version}/unplugg-#{arch}.dmg"
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
