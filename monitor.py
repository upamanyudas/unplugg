import subprocess
import threading
import time
from typing import Callable

class Monitor:
    def __init__(self, on_trigger: Callable[[str], None], config: dict):
        self.on_trigger = on_trigger
        self.config = config
        self.running = False
        self.thread = None
        self.last_power_state = self._get_power_state()
        self.last_lid_state = self._get_lid_state()

    def _get_power_state(self) -> bool:
        """True if plugged in (AC connected)."""
        result = subprocess.run(["pmset", "-g", "ac"], capture_output=True, text=True)
        return "AC Power" in result.stdout

    def _get_lid_state(self) -> bool:
        """True if lid closed (Clamshell Yes)."""
        result = subprocess.run(["ioreg", "-r", "-k", "AppleClamshellState", "-d", "4"], capture_output=True, text=True)
        return "Yes" in result.stdout

    def _poll(self):
        while self.running:
            if self.config["activated"]:
                current_power = self._get_power_state()
                current_lid = self._get_lid_state()

                if self.config["monitor_power"] and current_power != self.last_power_state and not current_power:
                    self.on_trigger("power_unplugged")
                self.last_power_state = current_power

                if self.config["monitor_lid"] and current_lid != self.last_lid_state and current_lid:
                    self.on_trigger("lid_closed")
                self.last_lid_state = current_lid
            time.sleep(2)  # Poll every 2s

    def start(self):
        if not self.running:
            self.running = True
            self.thread = threading.Thread(target=self._poll, daemon=True)
            self.thread.start()

    def stop(self):
        self.running = False
        if self.thread:
            self.thread.join(timeout=1)
