# Contributing

Issues and pull requests are welcome. Keep each change small and focused.

- **Code:** the fewest lines that work, but readable. Comment only what the
  code can't say.
- **Commits:** a short lowercase title, then `*` bullets on what changed and why.
- **Test on a real laptop:** sensors, camera, siren, Touch ID and Windows Hello
  don't run in CI.

## Run from source

```bash
cargo run
```

Install Rust with [rustup](https://rustup.rs). Linux also needs
`libgtk-3-dev libssl-dev libclang-dev nasm` (Ubuntu names). `cargo test` runs
the shared core's tests.

From source on macOS, camera, mic and location belong to your terminal app.
The .app below asks for its own.

## Build

```bash
packaging/mac/build.sh     # dist/unplugg.app and dist/unplugg.dmg, universal
packaging/linux/build.sh   # dist/: the AppImage, .deb and .rpm, on Ubuntu 22.04
```

Windows: `cargo build --release --target x86_64-pc-windows-msvc` (or
`aarch64-pc-windows-msvc`), then
`makensis -DAppVersion=<version> -DArch=x64 packaging/windows/unplugg.nsi`.
CI builds every one of them on each push (`build.yml`).

Camera, mic and location grants belong to the app's signature. An ad-hoc one
changes every build, so macOS asks again. To keep grants across rebuilds, make
a certificate once: **Keychain Access › Certificate Assistant › Create a
Certificate**, name `unplugg cert`, type **Code Signing**. The build signs with
it whenever it exists.

The lid-angle sensor is undocumented HID, so the app runs un-sandboxed (no App Store).

Linux ships FAAC (LGPL) as its own `lib/unplugg/libfaac.so`, so anyone can swap
in their own build. OpenH264 is built in from source.

## Layout

```
Cargo.toml           the version lives here
src/                 what every OS shares:
  main.rs            entry point: mac/, win/ or linux/
  config.rs          settings, JSON in the OS's per-user settings folder
  monitor.rs         polling loop, fires one "unsafe" event
  telegram.rs        Telegram Bot API
  live.rs            live location in Telegram
  updater.rs         latest release, versions, checksums
  mac/               AppKit, AVFoundation, CoreLocation through objc2
  win/               Win32, WinRT, Media Foundation through the windows crate
  linux/             GTK 3, a D-Bus tray, V4L2, PulseAudio, GeoClue
    mp4.rs           a small MP4 writer for H.264 + AAC
assets/              icons, menu bar glyphs, siren
packaging/           mac, Windows (NSIS) and Linux builds
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
| `assets/siren.flac` | the siren, lossless |
| `_design/telegram-avatar.png` | your bot: @BotFather → `/setuserpic` |
| `_design/social-preview.png` | repo **Settings › Social preview** |

Edit the logo, then run `python3 _design/icons.py` (needs Sketch, Xcode 26+,
`brew install imagemagick librsvg oxipng`). oxipng shrinks every PNG losslessly. `_design/unplugg.icon` opens in Icon Composer.

## Releasing

**Actions › Release unplugg › Run workflow**, pick `patch`, `minor` or `major`,
or type an exact version. It runs `build.yml`. Once all pass, it commits the
version to `Cargo.toml`, `Cargo.lock`, `Casks/unplugg.rb` and
`bucket/unplugg.json` with their sha256s, tags it, and attaches, each with a
`.sha256`:

- `unplugg.dmg`, plus the same file as `unplugg-arm64.dmg` and
  `unplugg-x86_64.dmg`, the names 1.x's updater looks for
- `unplugg-windows-{x64,arm64}-setup.exe`, `unplugg-windows-{x64,arm64}.zip`
- `unplugg-linux-{x86_64,arm64}.AppImage`, `unplugg-linux-{amd64,arm64}.deb`,
  `unplugg-linux-{x86_64,arm64}.rpm`

Pull after a release. The updaters find their download by its fixed name.

There's no Apple Developer ID ($99/yr), so releases aren't notarized. So grants
survive updates, releases are signed with the same `unplugg cert`, set once:

1. Keychain Access › My Certificates › `unplugg cert` › **Export**, as a .p12 with a password.
2. `base64 -i unplugg-cert.p12 | pbcopy`
3. Repo **Settings › Secrets and variables › Actions**: add `UNPLUGG_CERT_P12`
   (paste) and `UNPLUGG_CERT_PASSWORD`.

Without them, releases are ad-hoc signed and macOS asks again after each update.
