"""Polls hardware state and fires a single 'unsafe' callback when the Mac's
state breaks from the baseline captured at activation.

Safe baseline = plugged in, lid where it was when you activated. Unsafe =
unplugged (if monitoring power) or lid moved past the threshold / closed (if
monitoring the lid). Fires once per activation; deactivate + reactivate to arm
again.
"""
import threading
import time

from . import sensors

POLL = 0.2  # s between checks; the siren starts within this


class Monitor:
    def __init__(self, on_unsafe, config: dict):
        self.on_unsafe = on_unsafe
        self.config = config
        self._thread = None
        self._running = False
        self._fired = False
        self._baseline_angle = None

    def start(self):
        if self._running:
            return
        self._fired = False
        self._baseline_angle = sensors.lid_angle()  # None on older Macs
        self._running = True
        self._thread = threading.Thread(target=self._poll, daemon=True)
        self._thread.start()

    def stop(self):
        self._running = False
        if self._thread:
            self._thread.join(timeout=2)
            self._thread = None

    def _unsafe(self):
        """Return a reason string if state is unsafe, else None."""
        cfg = self.config
        if cfg["monitor_power"] and not sensors.is_plugged_in():
            return "power_unplugged"
        if cfg["monitor_lid"]:
            if self._baseline_angle is not None:
                now = sensors.lid_angle()
                if now is not None and abs(now - self._baseline_angle) > cfg["angle_threshold"]:
                    return "screen_moved"
            elif sensors.is_lid_closed():
                return "screen_closed"
        return None

    def _poll(self):
        while self._running:
            if not self._fired:
                reason = self._unsafe()
                if reason:
                    self._fired = True
                    self.on_unsafe(reason)
            time.sleep(POLL)
