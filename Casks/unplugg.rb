cask "unplugg" do
  arch arm: "arm64", intel: "x86_64"

  # the release workflow writes version and sha256
  version "1.0.0"
  sha256 arm:   "0000000000000000000000000000000000000000000000000000000000000000",
         intel: "0000000000000000000000000000000000000000000000000000000000000000"

  url "https://github.com/upamanyudas/unplugg/releases/download/v#{version}/unplugg-#{arch}.dmg"
  name "unplugg"
  desc "Menu bar alarm that guards a MacBook while you step away"
  homepage "https://github.com/upamanyudas/unplugg"

  auto_updates true
  depends_on :macos

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
