"""py2app build config.  Build:  python setup.py py2app

ponytail: usage-description plist keys are NOT optional — without them macOS
silently denies camera/location, and kills the app the moment it asks for the
microphone. They earn their place.

An ad-hoc signature changes every build, and macOS forgets those grants with
it. If this Mac has a code-signing certificate named "unplugg dev", the build
is re-signed with it, so grants survive rebuilds (README › Build the .app).
"""
import subprocess
import sys

from setuptools import setup

APP = ["main.py"]
DEV_IDENTITY = "unplugg dev"
DATA_FILES = [
    "assets/Assets.car",  # macOS 26+ Liquid Glass icon; older macOS uses logo.icns
    ("assets", [
        "assets/logo.png", "assets/menubar.pdf", "assets/menubar_armed.pdf",
        "assets/menubar_alarm.pdf", "assets/siren.wav",
    ]),
]

OPTIONS = {
    "argv_emulation": False,
    "iconfile": "assets/logo.icns",
    "packages": ["rumps", "requests", "pybooklid"],
    "no_report_missing_conditional_import": True,  # ~40 lines of harmless "missing" noise
    "plist": {
        "CFBundleName": "unplugg",
        "CFBundleDisplayName": "unplugg",
        "CFBundleIdentifier": "com.unplugg.app",
        "CFBundleIconName": "unplugg",       # icon inside Assets.car
        "CFBundleVersion": "1.0.0",
        "CFBundleShortVersionString": "1.0.0",
        "LSMinimumSystemVersion": "10.15",   # Catalina
        "LSUIElement": True,                  # menu-bar only, no Dock icon
        "NSCameraUsageDescription":
            "unplugg captures photos and video when your Mac is disturbed.",
        "NSMicrophoneUsageDescription":
            "unplugg records sound with the video when your Mac is disturbed.",
        "NSLocationUsageDescription":
            "unplugg includes your Mac's location in the alert you receive.",
        "NSLocationWhenInUseUsageDescription":
            "unplugg includes your Mac's location in the alert you receive.",
    },
}

setup(
    app=APP,
    data_files=DATA_FILES,
    options={"py2app": OPTIONS},  # py2app comes from requirements.txt; setup_requires is deprecated
)

identities = subprocess.run(["security", "find-identity", "-p", "codesigning"],
                            capture_output=True, text=True).stdout
if "py2app" in sys.argv and f'"{DEV_IDENTITY}"' in identities:
    subprocess.run(["codesign", "--force", "--sign", DEV_IDENTITY, "dist/unplugg.app"], check=True)
