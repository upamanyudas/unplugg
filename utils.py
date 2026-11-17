"""Small shared helpers: asset paths, screen lock."""
import ctypes
import os
import subprocess
import sys

LOGIN = "/System/Library/PrivateFrameworks/login.framework/Versions/Current/login"


def asset_path(name: str) -> str:
    """Path to a bundled asset, in dev or inside a .app."""
    if getattr(sys, "frozen", False):
        from Foundation import NSBundle
        return os.path.join(NSBundle.mainBundle().resourcePath(), "assets", name)
    return os.path.join(os.path.dirname(__file__), "assets", name)


def lock_screen() -> None:
    """Lock the Mac now; the login window enforces the password.
    SACLockScreenImmediate (private login.framework, what Hammerspoon calls)
    needs no permission. The Ctrl+Cmd+Q keystroke is the fallback, and macOS
    asks for Accessibility the first time it runs."""
    try:
        ctypes.CDLL(LOGIN).SACLockScreenImmediate()
        return
    except (OSError, AttributeError):  # framework or symbol gone in a future macOS
        pass
    subprocess.run([
        "osascript", "-e",
        'tell application "System Events" to key code 12 using {command down, control down}',
    ])
