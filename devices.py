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

BREW_BINS = ("/opt/homebrew/bin", "/usr/local/bin")


def _find(name):
    """which() first, then the Homebrew bins — a GUI/.app launch has a minimal
    PATH that omits /opt/homebrew/bin, which is why enumeration came back empty."""
    hit = shutil.which(name)
    if hit:
        return hit
    for d in BREW_BINS:
        p = os.path.join(d, name)
        if os.path.exists(p):
            return p
    return None


def _env():
    """Subprocess env with the Homebrew bins on PATH, so child tools resolve too."""
    e = dict(os.environ)
    e["PATH"] = ":".join(BREW_BINS) + ":" + e.get("PATH", "")
    return e


def _run(args, timeout=10):
    try:
        return subprocess.run(args, capture_output=True, text=True,
                              env=_env(), timeout=timeout).stdout
    except Exception:
        return ""


FFMPEG = _find("ffmpeg")
SWITCH = _find("SwitchAudioSource")

_cache = None  # (cameras, mics)


def _ffmpeg_devices():
    global _cache
    if _cache is not None:
        return _cache
    cameras, mics = [], []
    ffmpeg = _find("ffmpeg")
    if ffmpeg:
        # device list goes to stderr; ffmpeg exits non-zero, which is expected
        try:
            out = subprocess.run(
                [ffmpeg, "-f", "avfoundation", "-list_devices", "true", "-i", ""],
                capture_output=True, text=True, env=_env(), timeout=15).stderr
        except Exception:
            out = ""
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
    sw = _find("SwitchAudioSource")
    if not sw:
        return []
    return [l.strip() for l in _run([sw, "-a", "-t", "output"]).splitlines() if l.strip()]


def current_speaker():
    sw = _find("SwitchAudioSource")
    return _run([sw, "-c", "-t", "output"]).strip() or None if sw else None


def set_speaker(name):
    sw = _find("SwitchAudioSource")
    if sw and name:
        _run([sw, "-t", "output", "-s", name])
