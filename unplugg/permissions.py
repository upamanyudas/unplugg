"""Permission status + request + open-Settings, for Settings and the setup guide.

Camera/Microphone use AVFoundation's authorization API; Location uses
CoreLocation. Anything whose framework is missing degrades to "n/a" with an
Open-Settings button — we never block import on an optional framework.
"""
from AppKit import NSWorkspace
from Foundation import NSBundle, NSObject, NSURL
from PyObjCTools import AppHelper

_SETTINGS = {
    "camera": "Privacy_Camera",
    "microphone": "Privacy_Microphone",
    "location": "Privacy_LocationServices",
}

listener = None  # the key window's refresh, run on the main thread after any answer

# AVFoundation: camera + mic ------------------------------------------------
try:
    from AVFoundation import AVCaptureDevice, AVMediaTypeVideo, AVMediaTypeAudio
    _MEDIA = {"camera": AVMediaTypeVideo, "microphone": AVMediaTypeAudio}
except Exception:
    _MEDIA = {}

# CoreLocation --------------------------------------------------------------
# Until a live manager's delegate has heard back, CoreLocation says "not
# determined" even when allowed, so one manager lives as long as the app.
try:
    from CoreLocation import CLLocationManager
except Exception:
    CLLocationManager = None
_loc_status = None


class _LocationDelegate(NSObject):
    def locationManager_didChangeAuthorizationStatus_(self, _, status):  # also on creation
        global _loc_status
        _loc_status = status
        _changed()


if CLLocationManager:
    _loc_delegate = _LocationDelegate.alloc().init()  # the manager only holds it weakly
    _loc_mgr = CLLocationManager.alloc().init()
    _loc_mgr.setDelegate_(_loc_delegate)


def _changed():
    if listener:
        listener()


def status(key) -> str:
    """One of: 'granted', 'denied', 'unset', 'n/a'."""
    if key in _MEDIA:
        return {3: "granted", 2: "denied", 1: "denied"}.get(
            AVCaptureDevice.authorizationStatusForMediaType_(_MEDIA[key]), "unset")
    if key == "location" and CLLocationManager:
        # from source, Location belongs to the terminal that launched Python and
        # CoreLocation can't report it (Python.app has no usage description)
        if not NSBundle.mainBundle().objectForInfoDictionaryKey_("NSLocationUsageDescription"):
            return "via-terminal"
        return {3: "granted", 4: "granted", 2: "denied", 1: "denied"}.get(_loc_status, "unset")
    return "n/a"


def request(key):
    """Trigger the OS prompt where we can; otherwise open System Settings."""
    if key in _MEDIA:
        # the answer arrives on a background queue
        AVCaptureDevice.requestAccessForMediaType_completionHandler_(
            _MEDIA[key], lambda granted: AppHelper.callAfter(_changed))
        return
    if key == "location" and CLLocationManager:
        _loc_mgr.requestWhenInUseAuthorization()  # the delegate hears the answer
        return
    open_settings(key)


def open_settings(key):
    pane = _SETTINGS.get(key, "Privacy")
    url = NSURL.URLWithString_(
        f"x-apple.systempreferences:com.apple.preference.security?{pane}")
    NSWorkspace.sharedWorkspace().openURL_(url)
