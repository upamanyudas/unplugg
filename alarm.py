"""Runs the configured response when the Mac becomes unsafe: siren, capture,
location, Telegram, lock. Telegram always fires; the rest are toggles.

Called from the monitor's background thread, so it can block freely.
"""
import subprocess
from pathlib import Path

import capture
import devices
import location
import telegram
from utils import asset_path, lock_screen

_siren = None          # afplay process
_prev_volume = None    # system volume to restore
_prev_speaker = None   # output device to restore


def trigger(config: dict, event: str):
    if config["play_sound"]:
        _start_siren(config.get("speaker_name", ""))

    photo = video = None
    if config["capture_media"]:
        photo, video = capture.capture(config)

    link = location.maps_link() if config["send_location"] else None
    telegram.send_alert(event, config, photo=photo, video=video, location_url=link)

    if config["lock_device"]:
        lock_screen()


def _start_siren(speaker):
    """Play the siren at full volume on the chosen output, no matter the current
    system volume. Saves prior volume + output device to restore on stop."""
    global _siren, _prev_volume, _prev_speaker
    siren = asset_path("siren.wav")
    if not Path(siren).exists():
        return

    if speaker:
        _prev_speaker = devices.current_speaker()
        devices.set_speaker(speaker)

    _prev_volume = _osascript("output volume of (get volume settings)")
    _osascript("set volume output volume 100 without output muted")

    # ponytail: afplay ships with macOS — no audio dependency to bundle.
    _siren = subprocess.Popen(["afplay", "-v", "1", siren])


def stop_siren():
    global _siren, _prev_volume, _prev_speaker
    if _siren and _siren.poll() is None:
        _siren.terminate()
    _siren = None
    if _prev_volume is not None:
        _osascript(f"set volume output volume {_prev_volume}")
        _prev_volume = None
    if _prev_speaker:
        devices.set_speaker(_prev_speaker)
        _prev_speaker = None


def _osascript(script):
    out = subprocess.run(["osascript", "-e", script], capture_output=True, text=True).stdout.strip()
    return out or None
