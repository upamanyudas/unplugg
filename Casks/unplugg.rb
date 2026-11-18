cask "unplugg" do
  arch arm: "arm64", intel: "x86_64"

  version :latest # the app updates itself, so the cask never needs a bump
  sha256 :no_check

  url "https://github.com/upamanyudas/unplugg/releases/latest/download/unplugg-#{arch}.dmg"
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
