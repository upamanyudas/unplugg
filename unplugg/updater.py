"""Updates from the latest GitHub release, like Software Update: the release
notes, then Install Update / Skip This Version / Remind Me Later. With
"Install updates automatically" on, it installs without asking, never while armed.

The new copy is downloaded by the app itself, so it isn't quarantined and opens
without Gatekeeper's Open Anyway. Same signing certificate, same grants.
"""
import hashlib
import platform
import re
import subprocess
import sys
import tempfile
import threading
from pathlib import Path

import requests
import rumps
from AppKit import NSBezelBorder, NSFont, NSScrollView, NSTextView, NSViewWidthSizable
from Foundation import NSBundle, NSMakeRect
from PyObjCTools import AppHelper

from . import dialogs
from .config import save_config

REPO = "upamanyudas/unplugg"
LATEST = f"https://api.github.com/repos/{REPO}/releases/latest"
DAY = 24 * 60 * 60


def schedule(app):
    """Check now (if allowed) and again every day the app keeps running."""
    if app.config["check_updates"]:
        check(app)
    AppHelper.callLater(DAY, schedule, app)


def check(app, manual=False):
    """Ask GitHub off the main thread; answer on it."""
    def fetch():
        try:
            release = requests.get(LATEST, timeout=15,
                                   headers={"Accept": "application/vnd.github+json"}).json()
        except (requests.RequestException, ValueError):
            release = None
        AppHelper.callAfter(_offer, app, release, manual)
    threading.Thread(target=fetch, daemon=True).start()


def installed_version():
    """This copy's version, or None where updating makes no sense: from source,
    or the dev alias build that runs the source files."""
    info = NSBundle.mainBundle().infoDictionary()
    if not getattr(sys, "frozen", False) or info.get("PyOptions", {}).get("alias"):
        return None
    return info.get("CFBundleShortVersionString")


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
    if app.config["auto_update"] and not manual:
        if not app.config["activated"]:
            _install(app, release)
        return
    choice = dialogs.alert(f"unplugg {version} is available", f"You have {current}. What's new:",
                           "Install Update", "Skip This Version", "Remind Me Later",
                           accessory=_notes(release.get("body") or "No release notes."))
    if choice == 0:
        _install(app, release)
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
    text = text.replace("**", "")
    scroll = NSScrollView.alloc().initWithFrame_(NSMakeRect(0, 0, 420, 180))
    scroll.setHasVerticalScroller_(True)
    scroll.setBorderType_(NSBezelBorder)
    notes = NSTextView.alloc().initWithFrame_(scroll.contentView().bounds())
    notes.setAutoresizingMask_(NSViewWidthSizable)
    notes.setEditable_(False)
    notes.setFont_(NSFont.systemFontOfSize_(12))
    notes.setTextContainerInset_((6, 6))
    notes.setString_(text)
    scroll.setDocumentView_(notes)
    return scroll


def _install(app, release):
    if app.config["activated"]:
        dialogs.alert("Turn off the alarm first", "unplugg can't update while it's armed.")
        return
    assets = {a["name"]: a["browser_download_url"] for a in release.get("assets", [])}
    dmg = next((name for name in assets if name.endswith(f"-{platform.machine()}.dmg")), None)
    if dmg is None or f"{dmg}.sha256" not in assets:
        dialogs.alert("Couldn't update", "This release has no download for this Mac.")
        return

    def download():
        try:
            new_app = _fetch_app(assets[dmg], assets[f"{dmg}.sha256"])
        except (requests.RequestException, OSError, ValueError, subprocess.CalledProcessError) as e:
            AppHelper.callAfter(dialogs.alert, "Couldn't update", str(e))
            return
        AppHelper.callAfter(_swap_and_relaunch, new_app)
    threading.Thread(target=download, daemon=True).start()


def _fetch_app(dmg_url, sha_url):
    """The verified new unplugg.app, copied out of its disk image."""
    work = Path(tempfile.mkdtemp(prefix="unplugg-update-"))
    data = requests.get(dmg_url, timeout=120).content
    if hashlib.sha256(data).hexdigest() != requests.get(sha_url, timeout=15).text.split()[0]:
        raise ValueError("The download didn't match its checksum.")
    (work / "update.dmg").write_bytes(data)
    mount = work / "mount"
    subprocess.run(["hdiutil", "attach", "-nobrowse", "-readonly", "-mountpoint", str(mount),
                    str(work / "update.dmg")], check=True, capture_output=True)
    try:
        subprocess.run(["ditto", str(mount / "unplugg.app"), str(work / "unplugg.app")], check=True)
    finally:
        subprocess.run(["hdiutil", "detach", str(mount)], capture_output=True)
    return work / "unplugg.app"


def _swap_and_relaunch(new_app):
    """Move this copy aside, put the new one in its place, and reopen it."""
    here = Path(sys.executable).parents[2]  # …/unplugg.app/Contents/MacOS/python
    old = new_app.parent / "old.app"
    try:
        here.rename(old)
        try:
            new_app.rename(here)
        except OSError:
            old.rename(here)  # never leave the Mac without unplugg
            raise
    except OSError as e:
        dialogs.alert("Couldn't update", f"macOS wouldn't replace {here}: {e.strerror}.")
        return
    # reopen once this copy has quit
    subprocess.Popen(["/bin/sh", "-c", f'sleep 1; open "{here}"'], start_new_session=True)
    rumps.quit_application()
