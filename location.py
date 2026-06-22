"""One-shot location via CoreLocation -> a Google Maps link.

This is the honest replacement for "Find My live link": Apple exposes no API to
toggle Find My or mint a shareable link, but CoreLocation gives this Mac's own
coordinates (Wi-Fi triangulation) with the user's consent. Needs Location
Services enabled and NSLocationUsageDescription in the bundle's Info.plist.

ponytail: spins the runloop inline with a timeout instead of an async delegate
dance — we want one fix, then we're done.
"""
import objc
from Foundation import NSRunLoop, NSDate, NSObject

try:
    from CoreLocation import CLLocationManager, kCLLocationAccuracyHundredMeters
    _HAS_CORELOCATION = True
except ImportError:  # pyobjc-framework-CoreLocation not installed
    _HAS_CORELOCATION = False


class _Delegate(NSObject):
    def init(self):
        self = objc.super(_Delegate, self).init()
        self.coord = None
        self.done = False
        return self

    def locationManager_didUpdateLocations_(self, mgr, locs):
        if locs:
            c = locs[-1].coordinate()
            self.coord = (c.latitude, c.longitude)
        self.done = True

    def locationManager_didFailWithError_(self, mgr, err):
        self.done = True


def maps_link(timeout: float = 8.0):
    """Return a Google Maps URL for the Mac's location, or None."""
    if not _HAS_CORELOCATION:
        return None
    mgr = CLLocationManager.alloc().init()
    delegate = _Delegate.alloc().init()
    mgr.setDelegate_(delegate)
    mgr.setDesiredAccuracy_(kCLLocationAccuracyHundredMeters)
    if hasattr(mgr, "requestWhenInUseAuthorization"):
        mgr.requestWhenInUseAuthorization()
    mgr.startUpdatingLocation()

    deadline = NSDate.dateWithTimeIntervalSinceNow_(timeout)
    while not delegate.done and NSDate.date().compare_(deadline) < 0:
        NSRunLoop.currentRunLoop().runUntilDate_(
            NSDate.dateWithTimeIntervalSinceNow_(0.1))
    mgr.stopUpdatingLocation()

    if delegate.coord:
        lat, lon = delegate.coord
        return f"https://maps.google.com/?q={lat:.6f},{lon:.6f}"
    return None
