"""Polls hardware state and fires a single 'unsafe' callback when the laptop's
state breaks from the baseline captured at activation.

Safe baseline = plugged in, lid where it was when you activated, lying still.
Unsafe = unplugged (if monitoring power), lid moved past the threshold / closed
(if monitoring the lid), or moved (if monitoring motion and there's a sensor).
Fires once per activation; deactivate + reactivate to arm again.

The sensors module is the OS's own (unplugg.sensors, unplugg.pc.sensors), all
with the same readers.
"""
import math
import threading
import time

POLL = 0.2   # s between checks; the siren starts within this
MOVE = 0.1   # g of change from the armed reading that counts as moved


class Monitor:
    def __init__(self, on_unsafe, config: dict, sensors):
        self.on_unsafe = on_unsafe
        self.config = config
        self.sensors = sensors
        self._thread = None
        self._running = False
        self._fired = False
        self._lock = threading.Lock()
        self._baseline_angle = None
        self._baseline_motion = None

    def start(self):
        if self._running:
            return
        self._fired = False
        self._baseline_angle = self.sensors.lid_angle()  # None on older Macs, Windows, Linux
        self._baseline_motion = self.sensors.acceleration()  # None without an accelerometer
        self._running = True
        self._thread = threading.Thread(target=self._poll, daemon=True)
        self._thread.start()

    def stop(self):
        self._running = False
        if self._thread:
            self._thread.join(timeout=2)
            self._thread = None

    def trip(self, reason):
        """Fire for an event the poll can't see, such as a shutdown attempt."""
        if self._running:
            self._fire(reason)

    def _fire(self, reason):
        with self._lock:  # the poll and trip() can race
            if self._fired:
                return
            self._fired = True
        self.on_unsafe(reason)

    def _unsafe(self):
        """Return a reason string if state is unsafe, else None."""
        cfg, sensors = self.config, self.sensors
        if cfg["monitor_power"] and not sensors.is_plugged_in():
            return "power_unplugged"
        if cfg["monitor_lid"]:
            if self._baseline_angle is not None:
                now = sensors.lid_angle()
                if now is not None and abs(now - self._baseline_angle) > cfg["angle_threshold"]:
                    return "screen_moved"
            elif sensors.is_lid_closed():
                return "screen_closed"
        if cfg["monitor_motion"] and self._baseline_motion:
            now = sensors.acceleration()
            if now and math.dist(now, self._baseline_motion) > MOVE:
                return "device_moved"
        return None

    def _poll(self):
        while self._running:
            if not self._fired:
                reason = self._unsafe()
                if reason:
                    self._fire(reason)
            time.sleep(POLL)
