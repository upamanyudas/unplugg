#!/bin/sh
# Linux build:  packaging/linux/build.sh  ->  dist/unplugg-linux-<arch>.AppImage, .deb and .rpm
#
# Build on Ubuntu 22.04, so glibc 2.35 is the floor. Needs rust, libgtk-3-dev,
# libssl-dev, libclang-dev, nasm, meson, ninja-build, curl and nfpm on PATH.
# GTK3, OpenSSL 3 and PulseAudio come from the desktop. FAAC (LGPL) ships as its
# own library in lib/unplugg, so anyone can swap in their own build.
set -eu
cd "$(dirname "$0")/../.."

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
case "$(uname -m)" in
    x86_64) ARCH=x86_64 DEB=amd64 ;;
    aarch64) ARCH=arm64 DEB=arm64 ;;
esac
FAAC=faac-2.2
FAAC_SHA256=a93963573907c83e26e8cfabbf80d3a9c360f06ea4ecf1ea6cb74a202494d8d9
APPIMAGETOOL=https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-$(uname -m).AppImage

cargo build --release

rm -rf build/linux && mkdir -p build/linux dist
curl -sSLo build/linux/faac.tar.gz "https://github.com/FreewareAdvancedAudio/faac/archive/refs/tags/$FAAC.tar.gz"
echo "$FAAC_SHA256  build/linux/faac.tar.gz" | sha256sum -c -
tar -xzf build/linux/faac.tar.gz -C build/linux
meson setup build/linux/faac "build/linux/faac-$FAAC" -Dc_std=gnu11 -Ddefault_library=shared --buildtype=minsize
meson compile -C build/linux/faac

# the tree every package installs
ROOT=build/linux/root
install -Dm755 "${CARGO_TARGET_DIR:-target}/release/unplugg" "$ROOT/usr/bin/unplugg"
install -Dm644 build/linux/faac/libfaac/libfaac.so "$ROOT/usr/lib/unplugg/libfaac.so"
strip --strip-unneeded "$ROOT/usr/lib/unplugg/libfaac.so"
for asset in siren.flac menubar.svg menubar_armed.svg menubar_alarm.svg logo.png; do
    install -Dm644 "assets/$asset" "$ROOT/usr/share/unplugg/$asset"
done
install -Dm644 packaging/linux/unplugg.desktop "$ROOT/usr/share/applications/unplugg.desktop"
# the menu's icon is the same file; a .deb or .rpm would carry it twice
mkdir -p "$ROOT/usr/share/pixmaps"
ln -s ../unplugg/logo.png "$ROOT/usr/share/pixmaps/unplugg.png"

# AppImage: the same tree, an entry point, and the desktop file and icon at the top
APPDIR=build/linux/AppDir
cp -r "$ROOT" "$APPDIR"
install -m755 packaging/linux/AppRun "$APPDIR/AppRun"
cp packaging/linux/unplugg.desktop "$APPDIR/"
cp assets/logo.png "$APPDIR/unplugg.png"
curl -sSLo build/linux/appimagetool "$APPIMAGETOOL"
chmod +x build/linux/appimagetool
ARCH=$(uname -m) build/linux/appimagetool --appimage-extract-and-run --comp zstd "$APPDIR" "dist/unplugg-linux-$ARCH.AppImage"

# .deb and .rpm also install the polkit action, so disarming uses the desktop's own dialog
install -Dm644 packaging/linux/com.unplugg.app.policy "$ROOT/usr/share/polkit-1/actions/com.unplugg.app.policy"
export VERSION GOARCH=$DEB
nfpm package -f packaging/linux/nfpm.yaml -p deb -t "dist/unplugg-linux-$DEB.deb"
nfpm package -f packaging/linux/nfpm.yaml -p rpm -t "dist/unplugg-linux-$ARCH.rpm"
