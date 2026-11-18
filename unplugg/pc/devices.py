"""Cameras, microphones and speakers for the Settings pickers, saved by name.

Names survive a webcam or headset showing up; list indices shift. A saved
device that's gone falls back to the system default, so a thief pulling a
dock cable can't leave the alarm without a camera.

The siren plays on the chosen speaker directly, and its volume goes to 100%
for as long as it sounds: Core Audio on Windows, PulseAudio/PipeWire's pactl
on Linux.
"""
import re
import subprocess

from PySide6.QtMultimedia import QMediaDevices

from .utils import WINDOWS


def cameras():
    return [d.description() for d in QMediaDevices.videoInputs()]


def microphones():
    return [d.description() for d in QMediaDevices.audioInputs()]


def speakers():
    return [d.description() for d in QMediaDevices.audioOutputs()]


def camera(name):
    return _pick(QMediaDevices.videoInputs(), name, QMediaDevices.defaultVideoInput())


def microphone(name):
    return _pick(QMediaDevices.audioInputs(), name, QMediaDevices.defaultAudioInput())


def speaker(name):
    return _pick(QMediaDevices.audioOutputs(), name, QMediaDevices.defaultAudioOutput())


def _pick(devices, name, default):
    """The device called `name` while it's connected, else the default ("" = default)."""
    return next((d for d in devices if d.description() == name), default)


def max_volume(device):
    """Unmute `device` at 100%; returns a callable that puts it back."""
    device_id = bytes(device.id().data()).decode()
    try:
        return _max_windows(device_id) if WINDOWS else _max_linux(device_id)
    except Exception as e:  # a missing endpoint or no pactl: the siren still plays
        print(f"volume: {e}")
        return lambda: None


def _max_windows(device_id):
    from pycaw.pycaw import AudioUtilities  # Qt's id is the Core Audio endpoint id
    volume = AudioUtilities.CreateDevice(AudioUtilities.GetDeviceEnumerator().GetDevice(device_id)).EndpointVolume
    level, muted = volume.GetMasterVolumeLevelScalar(), volume.GetMute()
    volume.SetMute(0, None)
    volume.SetMasterVolumeLevelScalar(1.0, None)

    def restore():
        volume.SetMasterVolumeLevelScalar(level, None)
        volume.SetMute(muted, None)
    return restore


def _max_linux(sink):
    def pactl(*args):
        return subprocess.run(["pactl", *args], capture_output=True, text=True, check=True).stdout
    level = re.search(r"(\d+)%", pactl("get-sink-volume", sink)).group(1)
    muted = "yes" in pactl("get-sink-mute", sink)
    pactl("set-sink-mute", sink, "0")
    pactl("set-sink-volume", sink, "100%")

    def restore():
        pactl("set-sink-volume", sink, f"{level}%")
        pactl("set-sink-mute", sink, "1" if muted else "0")
    return restore
