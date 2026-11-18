cask "unplugg" do
  arch arm: "arm64", intel: "x86_64"

  # the release workflow writes version and sha256
  version "1.0.0"
  sha256 arm:   "cd2e53eac8c694ef8e27100308f57ba505dbf48f4cfc35ece299a7d917f4b8e5",
         intel: "a8550420d5dd9ba4fd157d2516b9b2082915d0a37d19f90b6e1000224747adc5"

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
