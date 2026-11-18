"""Small shared helpers: main-thread calls, asset paths, icons, screen lock."""
import ctypes
import os
import subprocess
import sys

from PySide6 import QtSvg  # noqa: F401  the SVG icon engine, so the build bundles it
from PySide6.QtCore import QObject, QTimer, QUrl, Signal
from PySide6.QtGui import QDesktopServices, QGuiApplication, QIcon, QPainter, Qt

WINDOWS = sys.platform == "win32"
# outline = idle, filled = armed, red = alarm fired
GLYPHS = {"idle": "menubar.svg", "armed": "menubar_armed.svg", "alarm": "menubar_alarm.svg"}


class _Main(QObject):
    call = Signal(object)


_main = _Main()  # made on import, from the main thread, so it lives there
_main.call.connect(lambda fn: fn())


def call_after(fn, *args):
    """Run fn(*args) on the main thread, from any thread."""
    _main.call.emit(lambda: fn(*args))


def call_later(seconds, fn, *args):
    """Run fn(*args) on the main thread after `seconds`. Main thread only."""
    QTimer.singleShot(int(seconds * 1000), lambda: fn(*args))


def asset_path(name: str) -> str:
    """Path to a bundled asset, from source or inside a PyInstaller build."""
    root = getattr(sys, "_MEIPASS", None) or os.path.dirname(os.path.dirname(os.path.dirname(__file__)))
    return os.path.join(root, "assets", name)


def app_icon():
    return QIcon(asset_path("logo.png"))


def tray_icon(state):
    """The menu bar glyph, tinted to read on the taskbar or panel; the alarm stays red."""
    icon = QIcon(asset_path(GLYPHS[state]))
    if state == "alarm":
        return icon
    pixmap = icon.pixmap(64, 64)
    painter = QPainter(pixmap)
    painter.setCompositionMode(QPainter.CompositionMode.CompositionMode_SourceIn)
    painter.fillRect(pixmap.rect(), Qt.GlobalColor.black if _light_bar() else Qt.GlobalColor.white)
    painter.end()
    return QIcon(pixmap)


def _light_bar():
    if WINDOWS:  # the taskbar has its own light/dark setting, apart from apps
        import winreg
        try:
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER,
                                r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize") as key:
                return winreg.QueryValueEx(key, "SystemUsesLightTheme")[0] == 1
        except OSError:
            return False
    if "GNOME" in os.environ.get("XDG_CURRENT_DESKTOP", ""):
        return False  # GNOME's top bar is dark in every theme
    return QGuiApplication.styleHints().colorScheme() == Qt.ColorScheme.Light


def open_url(url):
    QDesktopServices.openUrl(QUrl(url) if "://" in url or url.startswith("ms-settings:")
                             else QUrl.fromLocalFile(url))


def lock_screen() -> None:
    """Lock now; the OS's lock screen enforces the password."""
    if WINDOWS:
        ctypes.windll.user32.LockWorkStation()
        return
    # logind tells the desktop's locker; xdg-screensaver covers sessions without systemd
    for cmd in (["loginctl", "lock-session"], ["xdg-screensaver", "lock"]):
        try:
            if subprocess.run(cmd, capture_output=True).returncode == 0:
                return
        except FileNotFoundError:
            continue
