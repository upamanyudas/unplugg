"""Runs the configured response when the Mac becomes unsafe: siren, lock,
capture, location, Telegram. Telegram always fires; the rest are toggles.

The siren runs on the main thread (NSSound and NSAppleScript are main-thread
classes); trigger() runs on the monitor's background thread and can block.
"""
from AppKit import NSSound
from Foundation import NSAppleScript

import capture
import devices
import location
import telegram
from utils import asset_path, lock_screen

GET_VOLUME = "output volume of (get volume settings)"
MAX_VOLUME = "set volume output volume 100 without output muted"

_scripts = {}          # compiled by prepare(): ~0.3 ms a run, osascript took ~360 ms
_siren = None          # the looping NSSound while it sounds
_prev_volume = None    # system volume to restore
_prev_speaker = None   # output device to restore


def prepare():
    """Warm up AppleScript and audio at launch. Cold, each costs ~0.25-0.5 s;
    warm, the siren starts within ~20 ms of the trigger."""
    for src in (GET_VOLUME, MAX_VOLUME):
        _scripts[src] = NSAppleScript.alloc().initWithSource_(src)
        _scripts[src].compileAndReturnError_(None)
    _applescript(GET_VOLUME)  # loads the scripting additions
    warm = _sound()
    if warm:
        warm.setVolume_(0)
        warm.play()
        warm.stop()


def trigger(config: dict, event: str):
    # lock at once (capture + upload take ~20 s), unless the flash needs the screen
    lock_after_photos = config["flash_screen"] and config["capture_media"]
    if config["lock_device"] and not lock_after_photos:
        lock_screen()

    photo = video = None
    if config["capture_media"]:
        photo, video = capture.capture(config)
    if config["lock_device"] and lock_after_photos:
        lock_screen()

    link = location.maps_link() if config["send_location"] else None
    telegram.send_alert(event, config, photo=photo, video=video, location_url=link)


def start_siren(speaker):
    """Loop the siren at full volume on the chosen output, no matter the current
    system volume. Saves prior volume + output device to restore on stop."""
    global _siren, _prev_volume, _prev_speaker
    if _siren:
        return
    if speaker:
        _prev_speaker = devices.current_speaker()
        devices.set_speaker(speaker)
    _prev_volume = _applescript(GET_VOLUME)
    _applescript(MAX_VOLUME)
    _siren = _sound()  # made after the switch, so it plays on that speaker
    if _siren:
        _siren.setLoops_(True)  # seamless, unlike re-spawning afplay
        _siren.play()


def stop_siren():
    global _siren, _prev_volume, _prev_speaker
    if _siren:
        _siren.stop()
        _siren = None
    if _prev_volume is not None:
        _applescript(f"set volume output volume {_prev_volume}")
        _prev_volume = None
    if _prev_speaker:
        devices.set_speaker(_prev_speaker)
        _prev_speaker = None


def _sound():
    return NSSound.alloc().initWithContentsOfFile_byReference_(asset_path("siren.wav"), True)


def _applescript(src):
    """Run AppleScript in-process; returns its result as text, or None."""
    script = _scripts.get(src) or NSAppleScript.alloc().initWithSource_(src)
    result, _ = script.executeAndReturnError_(None)
    return result.stringValue() if result else None
