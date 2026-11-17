"""Permission status + request + open-Settings, for the Preferences panel.

Camera/Microphone use AVFoundation's authorization API; Location uses
CoreLocation. Anything whose framework is missing degrades to "n/a" with an
Open-Settings button — we never block import on an optional framework.
"""
from AppKit import NSWorkspace
from Foundation import NSURL

_SETTINGS = {
    "camera": "Privacy_Camera",
    "microphone": "Privacy_Microphone",
    "location": "Privacy_LocationServices",
}

# AVFoundation: camera + mic ------------------------------------------------
try:
    from AVFoundation import AVCaptureDevice, AVMediaTypeVideo, AVMediaTypeAudio
    _MEDIA = {"camera": AVMediaTypeVideo, "microphone": AVMediaTypeAudio}
except Exception:
    _MEDIA = {}

# CoreLocation --------------------------------------------------------------
try:
    from CoreLocation import CLLocationManager
except Exception:
    CLLocationManager = None
_loc_mgr = None  # keep a ref so the prompt isn't GC'd


def status(key) -> str:
    """One of: 'granted', 'denied', 'unset', 'n/a'."""
    if key in _MEDIA:
        return {3: "granted", 2: "denied", 1: "denied"}.get(
            AVCaptureDevice.authorizationStatusForMediaType_(_MEDIA[key]), "unset")
    if key == "location" and CLLocationManager:
        return {3: "granted", 4: "granted", 2: "denied", 1: "denied"}.get(
            CLLocationManager.authorizationStatus(), "unset")
    return "n/a"


def request(key):
    """Trigger the OS prompt where we can; otherwise open System Settings."""
    if key in _MEDIA:
        AVCaptureDevice.requestAccessForMediaType_completionHandler_(
            _MEDIA[key], lambda granted: None)
        return
    if key == "location" and CLLocationManager:
        global _loc_mgr
        _loc_mgr = CLLocationManager.alloc().init()
        _loc_mgr.requestWhenInUseAuthorization()
        return
    open_settings(key)


def open_settings(key):
    pane = _SETTINGS.get(key, "Privacy")
    url = NSURL.URLWithString_(
        f"x-apple.systempreferences:com.apple.preference.security?{pane}")
    NSWorkspace.sharedWorkspace().openURL_(url)
