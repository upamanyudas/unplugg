"""py2app build config.  Build:  python setup.py py2app

ponytail: usage-description plist keys are NOT optional — without them macOS
silently denies camera/location and the alarm captures nothing. They earn their
place.
"""
from setuptools import setup

APP = ["main.py"]
DATA_FILES = [("assets", [
    "assets/logo.png", "assets/about_logo.png",
    "assets/overlay_bg.png", "assets/siren.wav",
])]

OPTIONS = {
    "argv_emulation": False,
    "iconfile": "assets/logo.icns",
    "packages": ["rumps", "requests", "cv2", "pybooklid"],
    "plist": {
        "CFBundleName": "unplugg",
        "CFBundleDisplayName": "unplugg",
        "CFBundleIdentifier": "com.unplugg.app",
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
    options={"py2app": OPTIONS},
    setup_requires=["py2app"],
)
