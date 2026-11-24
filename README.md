<p align="center">
  <img src="assets/logo.png" width="160" alt="unplugg app icon">
</p>
<h1 align="center">unplugg</h1>
<p align="center">A laptop guard while you step away.</p>

A menu bar and tray app for mac, windows 10+ and linux laptops.
Arm it before you leave your laptop unattended in a café or library. If someone unplugs
the charger, moves the screen or closes the lid, unplugg:

- sounds a siren at full volume
- records photos and a 10-second video with sound
- sends them to your phone on Telegram, with a map link
- locks the laptop, if you want

Only you can turn it off: Touch ID, Windows Hello or your password.

While armed, the laptop stays awake. On Windows and Linux, the lid and power
button stop sleeping it, and a shutdown attempt sets off the alarm.

## Install

### macOS

**Homebrew (recommended)**

```bash
brew tap upamanyudas/unplugg https://github.com/upamanyudas/unplugg
brew trust --cask upamanyudas/unplugg/unplugg
brew install --cask upamanyudas/unplugg/unplugg
```

**Download:** get the `.dmg` for your Mac from
[Releases](https://github.com/upamanyudas/unplugg/releases/latest)
(`arm64` = Apple Silicon, `x86_64` = Intel), then drag unplugg into Applications.

unplugg isn't notarized. The first time you
open it, go to **System Settings › Privacy & Security › Open Anyway**.

Screen-move detection needs a 2019 or newer MacBook.

### Windows 10 and 11

**Scoop (recommended)**

```powershell
scoop bucket add unplugg https://github.com/upamanyudas/unplugg
scoop install unplugg/unplugg
```

**Download:** `unplugg-windows-x64-setup.exe` from
[Releases](https://github.com/upamanyudas/unplugg/releases/latest). No admin
needed. Or `unplugg-windows-x64.zip` to run without installing. ARM laptops
(Snapdragon): the `arm64` files.

unplugg isn't signed. The first time, SmartScreen says "Windows protected
your PC": click **More info › Run anyway**.

New tray icons hide under the **^** arrow. Drag unplugg onto the taskbar.

### Linux

**Homebrew (recommended)**

```bash
brew tap upamanyudas/unplugg https://github.com/upamanyudas/unplugg
brew trust --cask upamanyudas/unplugg/unplugg
brew install --cask upamanyudas/unplugg/unplugg
```

**Download** from [Releases](https://github.com/upamanyudas/unplugg/releases/latest):

- `unplugg-linux-x86_64.AppImage`: any distro, no install. `chmod +x` it, then run it.
- `unplugg-linux-amd64.deb`: Ubuntu, Debian. `sudo apt install ./unplugg-linux-amd64.deb`
- `unplugg-linux-x86_64.rpm`: Fedora, openSUSE. `sudo dnf install ./unplugg-linux-x86_64.rpm`

ARM laptops: the `arm64` files. x86_64 needs Ubuntu 22.04, Debian 12, Fedora
36 or newer. ARM64 needs Ubuntu 24.04 or newer.

- **GNOME:** tray icons need the AppIndicator extension. Ubuntu has it already.
- **Disarming:** the .deb and .rpm use your desktop's own password dialog. The
  AppImage and Homebrew ask in unplugg's own box.
- **Siren volume** needs `pactl` (`pulseaudio-utils`). Most desktops have it.
- **AppImage on Ubuntu 24.04+ (X11):** `sudo apt install libxcb-cursor0` if it won't start.

Windows and Linux detect the lid closing. 2-in-1s with a motion sensor also
alarm when picked up.

## Set up

A setup guide opens on first launch: permissions, Telegram, launch at login.
Reopen it from the menu with **Setup Guide…**. Every permission is optional.

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

The Mac app, the Windows installer and the AppImage update themselves. Scoop,
Homebrew on Linux, .deb and .rpm update the usual way: `scoop update unplugg`,
`brew upgrade unplugg`, or the new package from Releases.

## Privacy

No accounts, no analytics, no servers of its own. Photos and video stay in
`Pictures/unplugg` in your home folder and go only to your Telegram bot.
Update checks ask GitHub.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[MIT](LICENSE) © 2025 Upa Das
