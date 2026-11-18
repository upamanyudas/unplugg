"""Updates from the latest GitHub release, like the macOS app: the release
notes, then Install Update / Skip This Version / Remind Me Later. With
"Install updates automatically" on, it installs without asking, never while armed.

Only the installed Windows build and the AppImage replace themselves. Scoop,
Homebrew, .deb, .rpm and the portable zip belong to their package manager or
to you, so the dialog says how to update instead.
"""
import hashlib
import os
import re
import subprocess
import sys
import tempfile
import threading
from pathlib import Path

import requests
from PySide6.QtWidgets import QPlainTextEdit

from .. import __version__
from ..config import save_config
from . import dialogs
from .utils import WINDOWS, call_after, call_later, open_url

REPO = "upamanyudas/unplugg"
LATEST = f"https://api.github.com/repos/{REPO}/releases/latest"
PAGE = f"https://github.com/{REPO}/releases/latest"
DAY = 24 * 60 * 60
INSTALLER = "unplugg-windows-x64-setup.exe"   # fixed names: the release workflow uploads these
APPIMAGE = "unplugg-linux-x86_64.AppImage"


def schedule(app):
    """Check now (if allowed) and again every day the app keeps running."""
    if app.config["check_updates"]:
        check(app)
    call_later(DAY, schedule, app)


def check(app, manual=False):
    """Ask GitHub off the main thread; answer on it."""
    def fetch():
        try:
            release = requests.get(LATEST, timeout=15,
                                   headers={"Accept": "application/vnd.github+json"}).json()
        except (requests.RequestException, ValueError):
            release = None
        call_after(_offer, app, release, manual)
    threading.Thread(target=fetch, daemon=True).start()


def installed_version():
    """This copy's version, or None from source, where updating makes no sense."""
    return __version__ if getattr(sys, "frozen", False) else None


def _manager():
    """None if this copy updates itself, else how you update it."""
    if WINDOWS:
        exe = Path(sys.executable)
        if "scoop" in (p.lower() for p in exe.parts):
            return "Run: scoop update unplugg"
        return None if (exe.parent / "unins000.exe").exists() else "Download it from GitHub."
    appimage = os.environ.get("APPIMAGE")
    if not appimage:
        return "Download the new .deb or .rpm from GitHub."
    caskroom = Path(os.environ.get("HOMEBREW_PREFIX", "/home/linuxbrew/.linuxbrew")) / "Caskroom" / "unplugg"
    return "Run: brew upgrade unplugg" if caskroom.exists() else None


def _offer(app, release, manual):
    current = installed_version()
    if current is None:
        if manual:
            dialogs.alert("Updates need the released app",
                          "This copy runs from source. Download unplugg from GitHub to get updates.")
        return
    if release is None:
        if manual:
            dialogs.alert("Couldn't check for updates", "GitHub didn't answer. Try again later.")
        return
    version = release.get("tag_name", "").lstrip("v")
    if not _newer(version, current) or (not manual and version == app.config["skipped_version"]):
        if manual:
            dialogs.alert("unplugg is up to date", f"Version {current} is the newest.")
        return
    how = _manager()
    if app.config["auto_update"] and not manual and not how:
        if not app.config["activated"]:
            _install(app, release)
        return
    notes = _notes(release.get("body") or "No release notes.")
    text = f"You have {current}." + (f" {how}" if how else "") + " What's new:"
    first = "Open GitHub" if how and how.startswith("Download") else "OK" if how else "Install Update"
    choice = dialogs.alert(f"unplugg {version} is available", text,
                           first, "Skip This Version", "Remind Me Later", accessory=notes)
    if choice == 0 and first == "Install Update":
        _install(app, release)
    elif choice == 0 and first == "Open GitHub":
        open_url(PAGE)
    elif choice == 1:
        app.config["skipped_version"] = version
        save_config(app.config)


def _newer(tag, current):
    try:
        return tuple(map(int, tag.split("."))) > tuple(map(int, current.split(".")))
    except ValueError:  # not a plain x.y.z tag
        return False


def _notes(text):
    """Scrollable release notes: GitHub's markdown, read as plain text."""
    text = re.sub(r"(?m)^#+\s*", "", text)      # headings
    text = re.sub(r"(?m)^[*-] ", "• ", text)    # bullets
    notes = QPlainTextEdit(text.replace("**", ""))
    notes.setReadOnly(True)
    notes.setMinimumSize(420, 180)
    return notes


def _install(app, release):
    if app.config["activated"]:
        dialogs.alert("Turn off the alarm first", "unplugg can't update while it's armed.")
        return
    name = INSTALLER if WINDOWS else APPIMAGE
    assets = {a["name"]: a["browser_download_url"] for a in release.get("assets", [])}
    if name not in assets or f"{name}.sha256" not in assets:
        dialogs.alert("Couldn't update", "This release has no download for this computer.")
        return

    def download():
        try:
            new = _fetch(assets[name], assets[f"{name}.sha256"], name)
        except (requests.RequestException, OSError, ValueError) as e:
            call_after(dialogs.alert, "Couldn't update", str(e))
            return
        call_after(_swap_and_relaunch, app, new)
    threading.Thread(target=download, daemon=True).start()


def _fetch(url, sha_url, name):
    """The verified download: a temp folder on Windows, beside the AppImage on
    Linux, so the swap is a rename on one filesystem."""
    data = requests.get(url, timeout=300).content
    if hashlib.sha256(data).hexdigest() != requests.get(sha_url, timeout=15).text.split()[0]:
        raise ValueError("The download didn't match its checksum.")
    folder = Path(tempfile.mkdtemp(prefix="unplugg-update-")) if WINDOWS else Path(os.environ["APPIMAGE"]).parent
    path = folder / (name if WINDOWS else f".{name}.new")
    path.write_bytes(data)
    return path


def _swap_and_relaunch(app, new):
    if WINDOWS:
        # the installer waits for this copy to quit, replaces it, and starts the new one
        subprocess.Popen([str(new), "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"],
                         creationflags=subprocess.DETACHED_PROCESS)
    else:
        here = Path(os.environ["APPIMAGE"])
        try:
            new.chmod(0o755)
            os.replace(new, here)  # same filesystem: atomic; else the OSError below
        except OSError as e:
            dialogs.alert("Couldn't update", f"Linux wouldn't replace {here}: {e.strerror}.")
            return
        # reopen once this copy has quit, or the new one finds it still running
        subprocess.Popen(["/bin/sh", "-c", 'sleep 1; exec "$0"', str(here)], start_new_session=True)
    app.quit()
