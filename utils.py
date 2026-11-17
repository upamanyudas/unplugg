"""Small shared helpers: asset paths, Dock icon, screen lock."""
import ctypes
import os
import subprocess
import sys

from AppKit import (
    NSApplication, NSApplicationActivationPolicyAccessory, NSApplicationActivationPolicyRegular,
    NSImage,
)

LOGIN = "/System/Library/PrivateFrameworks/login.framework/Versions/Current/login"


def asset_path(name: str) -> str:
    """Path to a bundled asset, in dev or inside a .app."""
    if getattr(sys, "frozen", False):
        from Foundation import NSBundle
        return os.path.join(NSBundle.mainBundle().resourcePath(), "assets", name)
    return os.path.join(os.path.dirname(__file__), "assets", name)


def show_in_dock(show: bool) -> None:
    """Regular app (Dock icon, Cmd-Tab, menu bar) or accessory (menu bar only)."""
    app = NSApplication.sharedApplication()
    app.setActivationPolicy_(
        NSApplicationActivationPolicyRegular if show else NSApplicationActivationPolicyAccessory)
    if not getattr(sys, "frozen", False):
        # from source, each switch makes the Dock re-read Python.app's icon
        app.setApplicationIconImage_(NSImage.alloc().initWithContentsOfFile_(asset_path("logo.png")))


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
