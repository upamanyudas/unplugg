"""Windows + Linux build:  python packaging/build.py  ->  dist/unplugg/

One folder with unplugg(.exe) and its Python and Qt. The release workflow wraps
it: an installer and a zip on Windows, an AppImage, .deb and .rpm on Linux.
"""
import os

import PyInstaller.__main__

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ASSETS = ["logo.png", "menubar.svg", "menubar_armed.svg", "menubar_alarm.svg", "siren.wav"]

PyInstaller.__main__.run([
    os.path.join(ROOT, "main.py"),
    "--name", "unplugg",
    "--windowed",  # no console window
    "--noconfirm",
    "--icon", os.path.join(ROOT, "assets", "logo.ico"),
    "--exclude-module", "unplugg.app",  # the macOS app
    "--distpath", os.path.join(ROOT, "dist"),
    "--workpath", os.path.join(ROOT, "build"),
    "--specpath", os.path.join(ROOT, "build"),
    *(arg for name in ASSETS for arg in ("--add-data", f"{os.path.join(ROOT, 'assets', name)}:assets")),
])
