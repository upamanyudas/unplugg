<p align="center">
  <img src="assets/logo.png" width="160" alt="unplugg app icon">
</p>
<h1 align="center">unplugg</h1>
<p align="center">Your Mac, guarded while you step away.</p>

A menu-bar app that safeguards your Mac when you step away in a café or library.
While armed, it watches the **power adapter** and the **screen**; if either is
disturbed it sirens, captures photo + video from the webcam, grabs a location
link, and pushes a Telegram alert — then optionally locks the Mac.

## What's real (and what isn't)

| Requirement | Status |
|---|---|
| Power-unplugged detection | ✅ `pmset` |
| Screen-moved detection (degrees) | ✅ on **2019+ Macs** via the hidden lid-angle sensor |
| Screen on older Macs | ✅ falls back to closed/open detection (no degrees) |
| Siren | ✅ seamless NSSound loop at full volume on the chosen output, within ~0.25 s |
| Photo + 10s video **with audio** | ✅ AVFoundation, built in: nothing to install |
| Pick camera / mic / speaker | ✅ AVFoundation + SwitchAudioSource; built-in camera and mic by default |
| Permissions setup (camera/mic/location) | ✅ setup guide on first launch, then Settings → Permissions |
| Location in the alert | ✅ **CoreLocation** → Google Maps link |
| "Turn on Find My + live link" | ❌ **No Apple API exists.** Replaced by the CoreLocation link above. |
| Telegram alert with media | ✅ Bot API |
| Screen flash for photos | ✅ built-in screen to full brightness (lit even behind the lock screen) + white cover; restored on unlock |
| Overlay | ✅ covers every screen; any key or mouse move asks for Touch ID / password, then whether to deactivate |
| Launch at login | ✅ LaunchAgent (works Catalina → latest) |
| Menu bar only, or Dock too | ✅ Settings → General → Show in Dock; every quit path asks for Touch ID while armed |

The lid-angle sensor is undocumented HID, so the app must run **un-sandboxed**
(direct download, not the App Store).

## Project layout

```
main.py        entry point
unplugg.py     menu-bar app (rumps) — essentials only: Activate, Settings, About, Setup Guide, Help, Quit
preferences.py Settings window (System Settings style) — every setting lives here
guide.py       setup guide: how it works, permissions, Telegram, launch at login
config.py      settings (JSON in ~/Library/Application Support/unplugg)
sensors.py     power / lid-angle / clamshell readers (+ fallback)
monitor.py     polling loop → fires one "unsafe" event
alarm.py       runs the response: siren, capture, location, telegram, lock
capture.py     webcam video with sound + stills (AVFoundation)
location.py    CoreLocation → maps link
telegram.py    Telegram Bot API
overlay.py     fullscreen "Protected by unplugg" cover
dialogs.py     native Telegram setup panel, About, Help
loginitem.py   launch-at-login LaunchAgent
utils.py       asset paths, screen lock
assets/        app icons, menu bar glyphs, siren
_design/       icon sources + icons.py (rebuilds every icon)
```

## Icons

| File | Used by |
|---|---|
| `assets/logo.icns` | macOS 10.15–15 (refit to Apple's squircle grid) |
| `assets/Assets.car` | macOS 26+ Liquid Glass: default, dark, clear, tinted |
| `assets/menubar*.pdf` | menu bar: outline = idle, filled = armed, red = alarm |
| `_design/telegram-avatar.png` | your alert bot: @BotFather → `/setuserpic`, send as a photo |
| `_design/social-preview.png` | repo **Settings → Social preview** |

Edit the logo, then run `python3 _design/icons.py` (needs Sketch, Xcode 26+,
`brew install imagemagick librsvg`). The Liquid Glass source
`_design/unplugg.icon` opens in Icon Composer.

## Run from source

```bash
brew install switchaudio-osx               # siren speaker routing (optional)
python3 -m venv .venv && source .venv/bin/activate
pip install -r requirements.txt
python main.py
```

The setup guide opens on first launch; reopen it from the menu's **Setup
Guide…**. Every permission is optional. From source,
**Location** belongs to your terminal app, so Settings shows "Via terminal";
the dev .app (see Build) gets its own. The
lid-angle slider only appears on Macs that have the sensor.

### Telegram in 60 seconds
1. Message **@BotFather** → `/newbot` → copy the **token**.
2. Menu → **Settings…** → **Alerts** → Telegram → **+**, paste the token.
3. Click **Auto-detect**, then send your bot any message within 30 s.
4. **Send Test Message**, then **Save** (it checks the details with Telegram).

Use a bot only unplugg talks to: if another app polls the same bot, it eats
the messages and Auto-detect can't see them.

### Make alerts loud on your phone
Bots can't mark a message high priority; the Bot API only has a *silent* flag.
So it's a one-time phone setting (also under **?** in the setup panel):

- **iPhone:** Settings › Focus › each Focus › Apps › add Telegram. In Telegram,
  Settings › Notifications and Sounds: give the bot's chat a loud sound. Silent
  mode only lets vibration through: Sounds & Haptics › Haptics › Always Play.
- **Android:** long-press the first alert › **Priority**, then allow priority
  conversations in Do Not Disturb.

## Build the .app

```bash
pip install -r requirements.txt

# dev: a real .app that runs your live source files, so Touch ID and
# permission prompts show unplugg's icon and name instead of Python's
python setup.py py2app -A && open dist/unplugg.app

# release: standalone app
rm -rf build dist
python setup.py py2app          # → dist/unplugg.app
```

Camera, mic and location grants belong to the app's signature. An ad-hoc one
changes every build, so macOS asks again. To keep them across rebuilds, make a
certificate once: **Keychain Access › Certificate Assistant › Create a
Certificate**, name `unplugg dev`, type **Code Signing**. The build signs with
it whenever it exists.

The "Modules not found" list at the end is normal: optional imports inside
bundled libraries. A downloaded unsigned build opens via **System Settings ›
Privacy & Security › Open Anyway** (macOS 15+ dropped right-click → Open).

## Releasing (admin-initiated)

GitHub → **Actions → Release unplugg → Run workflow**, type the version. It
builds on Apple Silicon **and** Intel runners and attaches both zips to a
`v<version>` GitHub Release. See `.github/workflows/release.yml` for the
optional Developer-ID signing + notarization steps.

> Per-arch builds, not one universal2 binary: OpenCV and PyObjC don't reliably
> ship universal2 wheels, so a single universal build is flaky. Two native
> builds are honest and actually work.
