"""Runs the configured response when the laptop becomes unsafe: siren, lock,
capture, location, Telegram. Telegram always fires; the rest are toggles.

The siren lives on the main thread (Qt audio does); trigger() runs on a
background thread and can block.
"""
from PySide6.QtCore import QUrl
from PySide6.QtMultimedia import QSoundEffect

from .. import telegram
from . import capture, devices, location
from .utils import asset_path, lock_screen

_siren = None     # the looping sound while it sounds
_restore = None   # puts the speaker's volume back


def prepare():
    """Load the siren at launch, so it starts at once when the alarm fires."""
    global _siren
    _siren = QSoundEffect()
    _siren.setSource(QUrl.fromLocalFile(asset_path("siren.wav")))
    _siren.setLoopCount(QSoundEffect.Loop.Infinite.value)


def trigger(config: dict, event: str):
    if config["lock_device"]:
        lock_screen()  # first: capture and upload take ~20 s

    photo = video = None
    if config["capture_media"]:
        photo, video = capture.capture(config)

    link = location.maps_link() if config["send_location"] else None
    telegram.send_alert(event, config, photo=photo, video=video, location_url=link)


def start_siren(speaker):
    """Loop the siren at full volume on the chosen speaker; stop_siren() restores it."""
    global _restore
    if _siren is None or _siren.isPlaying():
        return
    device = devices.speaker(speaker)
    _restore = devices.max_volume(device)
    _siren.setAudioDevice(device)
    _siren.play()


def stop_siren():
    global _restore
    if _siren:
        _siren.stop()
    if _restore:
        _restore()
        _restore = None
