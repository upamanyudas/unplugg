# Contributing

Issues and pull requests are welcome. Keep each change small and focused.

- **Code:** the fewest lines that work, but readable. Comment only what the
  code can't say.
- **Commits:** a short lowercase title, then `*` bullets on what changed and why.
- **Test on a real laptop:** sensors, camera, siren, Touch ID and Windows Hello
  don't run in CI.

## Run from source

```bash
python3 -m venv .venv && source .venv/bin/activate
pip install -r requirements.txt
python main.py
```

On Windows, activate with `.venv\Scripts\activate`. `requirements.txt` picks
each OS's packages: PyObjC on macOS, Qt everywhere else.

From source on macOS, Location belongs to your terminal app, so Settings shows
"Via terminal". The dev .app below gets its own. `python -m unplugg.sensors`
(macOS) or `python -m unplugg.pc.sensors` (Windows, Linux) checks the readers.

## Build the .app

```bash
# dev: a real .app running your live source, so prompts show unplugg, not Python
python setup.py py2app -A && open dist/unplugg.app

# release: standalone
rm -rf build dist && python setup.py py2app
```

Camera, mic and location grants belong to the app's signature. An ad-hoc one
changes every build, so macOS asks again. To keep grants across rebuilds, make
a certificate once: **Keychain Access › Certificate Assistant › Create a
Certificate**, name `unplugg cert`, type **Code Signing**. The build signs with
it whenever it exists.

The "Modules not found" list at the end of a build is normal. The lid-angle
sensor is undocumented HID, so the app runs un-sandboxed (no App Store).

## Build for Windows or Linux

```bash
python packaging/build.py      # dist/unplugg/, the app and its Python and Qt
```

The release workflow wraps that folder. Windows: `packaging/windows/unplugg.iss`
(Inno Setup) and a zip. Linux: an AppImage, and a .deb and .rpm from
`packaging/linux/nfpm.yaml`. The .deb and .rpm install the polkit action
`com.unplugg.app.disarm`, so disarming asks for your own password.

## Layout

```
main.py              entry point: unplugg/ on macOS, unplugg/pc/ elsewhere
setup.py             py2app build
unplugg/             macOS (PyObjC), plus what every OS shares:
  config.py          settings, JSON in the OS's per-user settings folder
  monitor.py         polling loop, fires one "unsafe" event
  telegram.py        Telegram Bot API
  app.py             menu bar app (rumps)
  preferences.py     Settings window, every setting lives here
  guide.py           setup guide
  updater.py         updates from the latest GitHub release
  sensors.py         power, lid angle, clamshell
  alarm.py           the response: siren, capture, location, Telegram, lock
  capture.py         video with sound + stills (AVFoundation)
  devices.py         cameras, mics (AVFoundation), speakers (CoreAudio)
  permissions.py     camera, mic, location status and requests
  location.py        CoreLocation → maps link
  overlay.py         full-screen cover while armed
  dialogs.py         alerts, Telegram setup panel, About, Help
  auth.py            Touch ID / password gate
  loginitem.py       launch-at-login LaunchAgent
  utils.py           asset paths, Dock icon, screen lock
  pc/                Windows + Linux (Qt), the same modules, plus:
    power.py         stay awake, ignore lid and buttons while armed; shutdown events
assets/              icons, menu bar glyphs, siren
packaging/           build.py (PyInstaller), the Windows installer, Linux packages
bucket/unplugg.json  Scoop manifest: this repo is a Scoop bucket
Casks/unplugg.rb     Homebrew cask, macOS and Linux
_design/             icon sources, icons.py rebuilds them all
```

## Icons

| File | Used by |
|---|---|
| `assets/logo.icns` | macOS 10.15–15 |
| `assets/Assets.car` | macOS 26+ Liquid Glass: default, dark, clear, tinted |
| `assets/menubar*.pdf` | menu bar: outline = idle, filled = armed, red = alarm |
| `assets/menubar*.svg` | Windows and Linux tray, tinted to the taskbar |
| `assets/logo.ico` | Windows exe and installer |
| `assets/logo.png` | Linux, and every OS's windows and dialogs |
| `_design/telegram-avatar.png` | your bot: @BotFather → `/setuserpic` |
| `_design/social-preview.png` | repo **Settings › Social preview** |

Edit the logo, then run `python3 _design/icons.py` (needs Sketch, Xcode 26+,
`brew install imagemagick librsvg`). `_design/unplugg.icon` opens in Icon Composer.

## Releasing

**Actions › Release unplugg › Run workflow**, pick `patch`, `minor` or `major`,
or type an exact version (the first release: `1.0.0`). It builds on Apple
Silicon, Intel, Windows and Linux runners. Once all pass, it commits the
version to `unplugg/__init__.py`, `Casks/unplugg.rb` and `bucket/unplugg.json`
with their sha256s, tags it, and attaches, each with a `.sha256`:

- `unplugg-arm64.dmg`, `unplugg-x86_64.dmg`
- `unplugg-windows-x64-setup.exe`, `unplugg-windows-x64.zip`
- `unplugg-linux-x86_64.AppImage`, `unplugg-linux-amd64.deb`, `unplugg-linux-x86_64.rpm`

Pull after a release. The updaters find their download by its fixed name.

There's no Apple Developer ID ($99/yr), so releases aren't notarized. So grants
survive updates, releases are signed with the same `unplugg cert`, set once:

1. Keychain Access › My Certificates › `unplugg cert` › **Export**, as a .p12 with a password.
2. `base64 -i unplugg-cert.p12 | pbcopy`
3. Repo **Settings › Secrets and variables › Actions**: add `UNPLUGG_CERT_P12`
   (paste) and `UNPLUGG_CERT_PASSWORD`.

Without them, releases are ad-hoc signed and macOS asks again after each update.
Builds are per-arch: hidapi, behind the lid-angle sensor, ships no universal2 wheel.
