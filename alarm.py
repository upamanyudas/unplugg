"""Runs the configured response when the Mac becomes unsafe: siren, capture,
location, Telegram, lock. Telegram always fires; the rest are toggles.

Called from the monitor's background thread, so it can block freely.
"""
import subprocess
import threading
from pathlib import Path

import capture
import devices
import location
import telegram
from utils import asset_path, lock_screen

_siren_stop = threading.Event()
_siren_thread = None   # loops afplay until stopped
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
    """Loop the siren at full volume on the chosen output, no matter the current
    system volume. Saves prior volume + output device to restore on stop."""
    global _siren_thread, _prev_volume, _prev_speaker
    siren = asset_path("siren.wav")
    if not Path(siren).exists() or (_siren_thread and _siren_thread.is_alive()):
        return

    if speaker:
        _prev_speaker = devices.current_speaker()
        devices.set_speaker(speaker)

    _prev_volume = _osascript("output volume of (get volume settings)")
    _osascript("set volume output volume 100 without output muted")

    _siren_stop.clear()
    _siren_thread = threading.Thread(target=_siren_loop, args=(siren,), daemon=True)
    _siren_thread.start()


def _siren_loop(siren):
    # ponytail: afplay ships with macOS. Re-spawn it each pass; the tiny gap
    # between loops is fine for an alarm — a seamless siren needs a longer wav.
    while not _siren_stop.is_set():
        p = subprocess.Popen(["afplay", "-v", "1", siren])
        while p.poll() is None:
            if _siren_stop.wait(0.2):
                p.terminate()
                return


def stop_siren():
    global _prev_volume, _prev_speaker
    _siren_stop.set()
    if _siren_thread:
        _siren_thread.join(timeout=2)
    if _prev_volume is not None:
        _osascript(f"set volume output volume {_prev_volume}")
        _prev_volume = None
    if _prev_speaker:
        devices.set_speaker(_prev_speaker)
        _prev_speaker = None


def _osascript(script):
    out = subprocess.run(["osascript", "-e", script], capture_output=True, text=True).stdout.strip()
    return out or None
