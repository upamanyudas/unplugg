"""Enumerate capture/output devices for the Preferences pickers.

Cameras + microphones come from ffmpeg's avfoundation device list; the index we
return is the same index ffmpeg/capture.py feeds back in. Speakers come from
SwitchAudioSource. Everything degrades to an empty list if the tool is missing,
so the UI just shows "System default".

ponytail: shell out to the two CLIs already installed instead of binding
AVFoundation + CoreAudio in PyObjC. Enumeration runs once when Preferences opens
— a subprocess is nothing, and it dodges ~100 lines of untested struct marshaling.
"""
import os
import re
import shutil
import subprocess


def _find(name):
    """which() first, then the Homebrew bins — a GUI/.app launch has a minimal
    PATH that omits /opt/homebrew/bin, which is why enumeration came back empty."""
    return shutil.which(name) or next(
        (p for p in (f"/opt/homebrew/bin/{name}", f"/usr/local/bin/{name}")
         if os.path.exists(p)), None)


FFMPEG = _find("ffmpeg")
SWITCH = _find("SwitchAudioSource")

_cache = None  # (cameras, mics)


def _ffmpeg_devices():
    global _cache
    if _cache is not None:
        return _cache
    cameras, mics = [], []
    if FFMPEG:
        out = subprocess.run(
            [FFMPEG, "-f", "avfoundation", "-list_devices", "true", "-i", ""],
            capture_output=True, text=True, timeout=15,
        ).stderr
        target = None
        for line in out.splitlines():
            if "video devices" in line:
                target = cameras
            elif "audio devices" in line:
                target = mics
            elif target is not None:
                m = re.search(r"\[(\d+)\]\s+(.*)$", line)
                if m:
                    target.append(m.group(2).strip())
    _cache = (cameras, mics)
    return _cache


def cameras():
    return _ffmpeg_devices()[0]


def microphones():
    return _ffmpeg_devices()[1]


def speakers():
    if not SWITCH:
        return []
    out = subprocess.run([SWITCH, "-a", "-t", "output"],
                         capture_output=True, text=True).stdout
    return [l.strip() for l in out.splitlines() if l.strip()]


def current_speaker():
    if not SWITCH:
        return None
    return subprocess.run([SWITCH, "-c", "-t", "output"],
                          capture_output=True, text=True).stdout.strip() or None


def set_speaker(name):
    if SWITCH and name:
        subprocess.run([SWITCH, "-t", "output", "-s", name], capture_output=True)
