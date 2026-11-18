"""Launch at login: the Run key on Windows, an XDG autostart entry on Linux."""
import os
import subprocess
import sys
from pathlib import Path

from .utils import WINDOWS


def _command():
    """What to start: this build (an AppImage runs from $APPIMAGE), else this source."""
    if getattr(sys, "frozen", False):
        return [os.environ.get("APPIMAGE", sys.executable)]
    python = sys.executable.replace("python.exe", "pythonw.exe")  # no console window on Windows
    return [python, str(Path(__file__).parents[2] / "main.py")]


if WINDOWS:
    import winreg

    RUN = r"Software\Microsoft\Windows\CurrentVersion\Run"

    def enable() -> None:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN, 0, winreg.KEY_SET_VALUE) as key:
            winreg.SetValueEx(key, "unplugg", 0, winreg.REG_SZ, subprocess.list2cmdline(_command()))

    def disable() -> None:
        try:
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN, 0, winreg.KEY_SET_VALUE) as key:
                winreg.DeleteValue(key, "unplugg")
        except FileNotFoundError:
            pass
else:
    ENTRY = Path(os.environ.get("XDG_CONFIG_HOME") or Path.home() / ".config") / "autostart" / "unplugg.desktop"

    def enable() -> None:
        ENTRY.parent.mkdir(parents=True, exist_ok=True)
        command = " ".join(f'"{arg}"' for arg in _command())  # desktop entries quote with ""
        ENTRY.write_text(f"[Desktop Entry]\nType=Application\nName=unplugg\nExec={command}\nIcon=unplugg\n")

    def disable() -> None:
        ENTRY.unlink(missing_ok=True)
