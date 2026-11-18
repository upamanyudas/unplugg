# Contributing

Issues and pull requests are welcome. Keep each change small and focused.

- **Code:** the fewest lines that work, but readable. Comment only what the
  code can't say.
- **Commits:** a short lowercase title, then `*` bullets on what changed and why.
- **Test on a real MacBook:** sensors, camera, siren and Touch ID don't run in CI.

## Run from source

```bash
python3 -m venv .venv && source .venv/bin/activate
pip install -r requirements.txt
python main.py
```

From source, Location belongs to your terminal app, so Settings shows "Via
terminal". The dev .app below gets its own. `python -m unplugg.sensors` checks
the power, lid and angle readers.

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

## Layout

```
main.py              entry point (py2app needs a script)
setup.py             py2app build
unplugg/
  app.py             menu bar app (rumps)
  preferences.py     Settings window, every setting lives here
  guide.py           setup guide
  updater.py         updates from the latest GitHub release
  config.py          settings, JSON in ~/Library/Application Support/unplugg
  sensors.py         power, lid angle, clamshell
  monitor.py         polling loop, fires one "unsafe" event
  alarm.py           the response: siren, capture, location, Telegram, lock
  capture.py         video with sound + stills (AVFoundation)
  devices.py         cameras, mics (AVFoundation), speakers (CoreAudio)
  permissions.py     camera, mic, location status and requests
  location.py        CoreLocation → maps link
  telegram.py        Telegram Bot API
  overlay.py         full-screen cover while armed
  dialogs.py         alerts, Telegram setup panel, About, Help
  auth.py            Touch ID / password gate
  loginitem.py       launch-at-login LaunchAgent
  utils.py           asset paths, Dock icon, screen lock
assets/              icons, menu bar glyphs, siren
Casks/unplugg.rb     Homebrew cask
_design/             icon sources, icons.py rebuilds them all
```

## Icons

| File | Used by |
|---|---|
| `assets/logo.icns` | macOS 10.15–15 |
| `assets/Assets.car` | macOS 26+ Liquid Glass: default, dark, clear, tinted |
| `assets/menubar*.pdf` | menu bar: outline = idle, filled = armed, red = alarm |
| `_design/telegram-avatar.png` | your bot: @BotFather → `/setuserpic` |
| `_design/social-preview.png` | repo **Settings › Social preview** |

Edit the logo, then run `python3 _design/icons.py` (needs Sketch, Xcode 26+,
`brew install imagemagick librsvg`). `_design/unplugg.icon` opens in Icon Composer.

## Releasing

**Actions › Release unplugg › Run workflow**, pick `patch`, `minor` or `major`,
or type an exact version (the first release: `1.0.0`). It builds on Apple
Silicon and Intel runners. Once both pass, it commits the version to `setup.py`
and `Casks/unplugg.rb` with each dmg's sha256, tags it, and attaches
`unplugg-arm64.dmg` and `unplugg-x86_64.dmg`, each with a `.sha256`. Pull
after a release. The updater picks this Mac's dmg by its fixed name.

There's no Apple Developer ID ($99/yr), so releases aren't notarized. So grants
survive updates, releases are signed with the same `unplugg cert`, set once:

1. Keychain Access › My Certificates › `unplugg cert` › **Export**, as a .p12 with a password.
2. `base64 -i unplugg-cert.p12 | pbcopy`
3. Repo **Settings › Secrets and variables › Actions**: add `UNPLUGG_CERT_P12`
   (paste) and `UNPLUGG_CERT_PASSWORD`.

Without them, releases are ad-hoc signed and macOS asks again after each update.
Builds are per-arch: hidapi, behind the lid-angle sensor, ships no universal2 wheel.
