# unplugg

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
| Siren | ✅ macOS `afplay`, forced to full volume on the chosen output |
| Photo + 10s video **with audio** | ✅ ffmpeg (avfoundation); OpenCV video-only fallback |
| Pick camera / mic / speaker | ✅ ffmpeg + SwitchAudioSource enumeration |
| Permissions setup (camera/mic/location/accessibility) | ✅ Preferences → Permissions tab |
| Location in the alert | ✅ **CoreLocation** → Google Maps link |
| "Turn on Find My + live link" | ❌ **No Apple API exists.** Replaced by the CoreLocation link above. |
| Telegram alert with media | ✅ Bot API |
| Overlay + lock | ✅ overlay covers the screen; first input locks the Mac (login window enforces the password) |
| Launch at login | ✅ LaunchAgent (works Catalina → latest) |

The lid-angle sensor is undocumented HID, so the app must run **un-sandboxed**
(direct download, not the App Store).

## Project layout

```
main.py        entry point
unplugg.py     menu-bar app (rumps) — essentials only: Activate, Preferences, About, Help, Quit
preferences.py native Preferences window — every setting lives here
config.py      settings (JSON in ~/Library/Application Support/unplugg)
sensors.py     power / lid-angle / clamshell readers (+ fallback)
monitor.py     polling loop → fires one "unsafe" event
alarm.py       runs the response: siren, capture, location, telegram, lock
capture.py     webcam photos + video (OpenCV)
location.py    CoreLocation → maps link
telegram.py    Telegram Bot API
overlay.py     fullscreen "Protected by unplugg" cover
dialogs.py     native Telegram setup panel, About, Help
loginitem.py   launch-at-login LaunchAgent
utils.py       asset paths, screen lock
assets/        placeholder images + siren — swap in your branded ones
```

## Run from source

```bash
brew install ffmpeg switchaudio-osx        # audio+video capture, speaker routing (optional)
python3 -m venv .venv && source .venv/bin/activate
pip install -r requirements.txt
python main.py
```

First arm grants **Camera** and **Location** permission when prompted. The
lid-angle slider only appears on Macs that have the sensor.

### Telegram in 60 seconds
1. Message **@BotFather** → `/newbot` → copy the **token**.
2. Message your new bot anything, then open
   `https://api.telegram.org/bot<TOKEN>/getUpdates` and copy `chat.id`.
3. Menu → **Preferences…** → Telegram destinations → **Add…**
   (the panel verifies the credentials before saving).

## Build the .app

```bash
pip install -r requirements.txt
python setup.py py2app          # → dist/unplugg.app
```

Drop a real `assets/logo.icns` in first (the placeholders are empty). Unsigned
builds open via **right-click → Open** the first time.

## Releasing (admin-initiated)

GitHub → **Actions → Release unplugg → Run workflow**, type the version. It
builds on Apple Silicon **and** Intel runners and attaches both zips to a
`v<version>` GitHub Release. See `.github/workflows/release.yml` for the
optional Developer-ID signing + notarization steps.

> Per-arch builds, not one universal2 binary: OpenCV and PyObjC don't reliably
> ship universal2 wheels, so a single universal build is flaky. Two native
> builds are honest and actually work.
