"""One-shot location -> a Google Maps link, via Qt Positioning: Windows'
Geolocator (Wi-Fi positioning) on Windows, GeoClue on Linux. Needs location
services on, and on Windows, desktop apps allowed to use them.

Qt's position source lives on the main thread; maps_link() is called from the
alarm's background thread and waits for it.
"""
import threading

from PySide6.QtPositioning import QGeoPositionInfoSource

from .utils import call_after

_sources = []  # keep each source alive until it answers


def maps_link(timeout: float = 8.0):
    """Return a Google Maps URL for the laptop's location, or None."""
    done = threading.Event()
    found = {}
    call_after(_request, timeout, found, done)
    done.wait(timeout + 2)
    if "coord" not in found:
        return None
    lat, lon = found["coord"]
    return f"https://maps.google.com/?q={lat:.6f},{lon:.6f}"


def _request(timeout, found, done):
    source = QGeoPositionInfoSource.createDefaultSource(None)
    if source is None:  # no backend, or location services off
        done.set()
        return
    _sources.append(source)

    def answered(info=None):
        if info is not None and info.isValid():
            found["coord"] = (info.coordinate().latitude(), info.coordinate().longitude())
        if source in _sources:  # an error can follow an answer
            _sources.remove(source)
        done.set()

    source.positionUpdated.connect(answered)
    source.errorOccurred.connect(lambda _: answered())
    source.requestUpdate(int(timeout * 1000))
