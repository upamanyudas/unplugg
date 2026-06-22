"""Small shared helpers: asset paths, screen lock."""
import os
import subprocess
import sys


def asset_path(name: str) -> str:
    """Path to a bundled asset, in dev or inside a .app."""
    if getattr(sys, "frozen", False):
        from Foundation import NSBundle
        return os.path.join(NSBundle.mainBundle().resourcePath(), "assets", name)
    return os.path.join(os.path.dirname(__file__), "assets", name)


def lock_screen() -> None:
    """Lock the Mac (Ctrl+Cmd+Q). The login window enforces the password."""
    subprocess.run([
        "osascript", "-e",
        'tell application "System Events" to key code 12 using {command down, control down}',
    ])
