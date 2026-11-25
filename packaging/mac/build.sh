#!/bin/sh
# macOS build:  packaging/mac/build.sh  ->  dist/unplugg.app and dist/unplugg.dmg
#
# One universal app for Apple Silicon and Intel. The usage descriptions in
# Info.plist are not optional: without them macOS denies the camera and location,
# and kills the app when it asks for the microphone.
set -eu
cd "$(dirname "$0")/../.."

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
APP=dist/unplugg.app
for arch in aarch64 x86_64; do
    cargo build --release --target "$arch-apple-darwin"
done

rm -rf "$APP" dist/dmg dist/unplugg.dmg
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources/assets"
lipo -create -output "$APP/Contents/MacOS/unplugg" \
    target/aarch64-apple-darwin/release/unplugg target/x86_64-apple-darwin/release/unplugg
sed "s/@VERSION@/$VERSION/g" packaging/mac/Info.plist > "$APP/Contents/Info.plist"
# Assets.car is the macOS 26+ Liquid Glass icon, logo.icns the older macOS one
cp assets/Assets.car assets/logo.icns "$APP/Contents/Resources/"
cp assets/menubar.pdf assets/menubar_armed.pdf assets/menubar_alarm.pdf assets/siren.flac \
    "$APP/Contents/Resources/assets/"

# An ad-hoc signature changes every build, and macOS forgets the grants with it.
# A code-signing certificate named "unplugg cert" keeps them (README › Build).
IDENTITY="unplugg cert"
security find-identity -p codesigning | grep -q "\"$IDENTITY\"" || IDENTITY=-
codesign --force --sign "$IDENTITY" "$APP"

# drag the app onto Applications to install. lzma needs macOS 10.15, like the app,
# and HFS+ packs ~190 KB smaller than APFS
mkdir dist/dmg
ditto "$APP" dist/dmg/unplugg.app
ln -s /Applications dist/dmg/Applications
for try in 1 2 3; do  # hdiutil on hosted runners sometimes says "Resource busy"
    hdiutil create -volname unplugg -srcfolder dist/dmg -fs HFS+ -format ULMO dist/unplugg.dmg && break
    [ $try = 3 ] && exit 1
    sleep 5
done
rm -rf dist/dmg
