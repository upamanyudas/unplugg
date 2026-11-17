"""py2app build config.  Build:  python setup.py py2app

ponytail: usage-description plist keys are NOT optional — without them macOS
silently denies camera/location and the alarm captures nothing. They earn their
place.
"""
from setuptools import setup

APP = ["main.py"]
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
    "packages": ["rumps", "requests", "cv2", "pybooklid"],
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
