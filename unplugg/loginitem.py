"""Launch at login via a LaunchAgent plist.

ponytail: a ~/Library/LaunchAgents plist works identically from Catalina to the
latest macOS. The modern SMAppService API is 13+ only and the old LSSharedFileList
API is removed — a plist is the one path that covers your whole version range.
"""
import subprocess
import sys
from pathlib import Path

LABEL = "com.unplugg.launch"
PLIST = Path.home() / "Library" / "LaunchAgents" / f"{LABEL}.plist"


def _program() -> str:
    """The thing to relaunch: the .app bundle when frozen, else this script."""
    if getattr(sys, "frozen", False):
        # sys.executable -> Foo.app/Contents/MacOS/Foo ; open the .app
        return str(Path(sys.executable).parents[2])
    return str(Path(__file__).parents[1] / "main.py")


def enable() -> None:
    prog = _program()
    args = ["/usr/bin/open", prog] if prog.endswith(".app") else [sys.executable, prog]
    items = "".join(f"<string>{a}</string>" for a in args)
    PLIST.parent.mkdir(parents=True, exist_ok=True)
    PLIST.write_text(
        '<?xml version="1.0" encoding="UTF-8"?>'
        '<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" '
        '"http://www.apple.com/DTDs/PropertyList-1.0.dtd">'
        '<plist version="1.0"><dict>'
        f"<key>Label</key><string>{LABEL}</string>"
        f"<key>ProgramArguments</key><array>{items}</array>"
        "<key>RunAtLoad</key><true/>"
        "</dict></plist>"
    )
    subprocess.run(["launchctl", "load", str(PLIST)], capture_output=True)


def disable() -> None:
    subprocess.run(["launchctl", "unload", str(PLIST)], capture_output=True)
    PLIST.unlink(missing_ok=True)
