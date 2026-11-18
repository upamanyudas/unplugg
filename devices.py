"""Enumerate capture/output devices for the Preferences pickers.

Cameras + microphones come from AVFoundation and are saved by name: names
survive an iPhone or AirPods showing up, list indices shift (index 0 was the
iPhone mic). Speakers come from
SwitchAudioSource; without it the list is empty and the UI shows "System default".

ponytail: shell out to SwitchAudioSource instead of binding CoreAudio — it
dodges ~100 lines of untested struct marshaling for a list of names.
"""
import os
import shutil
import subprocess

from AVFoundation import AVCaptureDevice, AVMediaTypeAudio, AVMediaTypeVideo

BREW_BINS = ("/opt/homebrew/bin", "/usr/local/bin")
BUILT_IN = int.from_bytes(b"bltn", "big")  # kIOAudioDeviceTransportTypeBuiltIn


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


SWITCH = _find("SwitchAudioSource")

def _capture(media):
    return list(AVCaptureDevice.devicesWithMediaType_(media))


def cameras():
    return [d.localizedName() for d in _capture(AVMediaTypeVideo)]


def microphones():
    return [d.localizedName() for d in _capture(AVMediaTypeAudio)]


def camera(name):
    return _pick(_capture(AVMediaTypeVideo), name)


def microphone(name):
    return _pick(_capture(AVMediaTypeAudio), name)


def _pick(devs, name):
    """`name` while it's connected, else this Mac's built-in device ("" = built-in),
    so a thief pulling the dock cable can't leave the alarm without a camera."""
    if name in [d.localizedName() for d in devs]:
        return name
    built_in = [d for d in devs if d.transportType() == BUILT_IN] or devs
    return built_in[0].localizedName() if built_in else None


def device(media, name):
    """The connected capture device called `name`, or None."""
    return next((d for d in _capture(media) if d.localizedName() == name), None)


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
