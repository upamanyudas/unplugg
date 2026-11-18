<p align="center">
  <img src="assets/logo.png" width="160" alt="unplugg app icon">
</p>
<h1 align="center">unplugg</h1>
<p align="center">Your Mac, guarded while you step away.</p>

A menu bar app for MacBooks. Arm it before you leave your Mac in a café or
library. If someone unplugs the charger or moves the screen, unplugg:

- sounds a siren at full volume
- records photos and a 10-second video with sound
- sends them to your phone on Telegram, with a map link
- locks the Mac, if you want

Only you can turn it off, with Touch ID or your password.

## Install

**Homebrew**

```bash
brew tap upamanyudas/unplugg https://github.com/upamanyudas/unplugg
brew trust --cask upamanyudas/unplugg/unplugg
brew install --cask upamanyudas/unplugg/unplugg
```

**Download:** get the `.dmg` for your Mac from
[Releases](https://github.com/upamanyudas/unplugg/releases/latest)
(`arm64` = Apple Silicon, `x86_64` = Intel), then drag unplugg into Applications.

unplugg isn't notarized; that needs a paid Apple account. The first time you
open it, go to **System Settings › Privacy & Security › Open Anyway**.

Screen-move detection in degrees needs a 2019 or newer MacBook. Older ones
detect the lid closing.

## Set up

A setup guide opens on first launch: permissions, Telegram, launch at login.
Reopen it from the menu bar with **Setup Guide…**. Every permission is optional.

### Telegram in 60 seconds

1. Message **@BotFather** → `/newbot` → copy the **token**.
2. In the setup guide (or **Settings… › Alerts**), click **+** and paste the token.
3. Click **Auto-detect**, then send your bot any message within 30 s.
4. **Send Test Message**, then **Save**.

Give unplugg its own bot. If another app reads the same bot, it takes the
messages and Auto-detect can't see them.

### Make alerts loud on your phone

Bots can't send high-priority messages, so this is a one-time phone setting:

- **iPhone:** Settings › Focus › each Focus › Apps › add Telegram. In Telegram,
  Settings › Notifications and Sounds: give the bot's chat a loud sound. For
  silent mode: Sounds & Haptics › Haptics › Always Play.
- **Android:** long-press the first alert › **Priority**, then allow priority
  conversations in Do Not Disturb.

## Updates

unplugg checks GitHub once a day and shows what's new: install, skip, or later.
**Settings › General** can install updates on their own, never while armed.

## Privacy

No accounts, no analytics, no servers of its own. Photos and video stay in
`~/Pictures/unplugg` and go only to your Telegram bot. Update checks ask GitHub.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[MIT](LICENSE) © 2025 Upa Das
