"""Menu bar app. Menu stays glanceable; all settings live in Preferences."""
import os

import rumps
from PyObjCTools import AppHelper

import alarm
import auth
import dialogs
import overlay
import sensors
from config import load_config, save_config
from monitor import Monitor
from preferences import open_preferences
from utils import asset_path

IDLE_TITLE = "🔌"
ARMED_TITLE = "🛡️"


class UnpluggApp(rumps.App):
    def __init__(self):
        logo = asset_path("logo.png")
        icon = logo if os.path.exists(logo) and os.path.getsize(logo) > 0 else None
        super().__init__("unplugg", title=IDLE_TITLE, icon=icon, template=True, quit_button=None)
        self.config = load_config()
        self.config["activated"] = False  # never boot already-armed
        self.monitor = Monitor(self._on_unsafe, self.config)
        self.menu = [
            rumps.MenuItem("Activate", callback=self._toggle_activate),
            None,
            rumps.MenuItem("Preferences…", callback=lambda _: open_preferences(self)),
            None,
            rumps.MenuItem("About…", callback=lambda _: dialogs.show_about()),
            rumps.MenuItem("Help…", callback=lambda _: dialogs.show_help()),
            rumps.MenuItem("Quit", callback=self._quit),
        ]

    def _toggle_activate(self, sender):
        if not self.config["activated"]:
            if self.config["monitor_power"] and not sensors.is_plugged_in():
                rumps.alert("Plug in first", "Your MacBook should be plugged to power before "
                            "you can activate the alarm. Alternatively, uncheck the "
                            '"Power Unplugged" option.')
                return
            self.config["activated"] = True
            self.monitor.start()
            sender.title = "Deactivate"
            self.title = ARMED_TITLE
            if self.config["show_overlay"]:
                overlay.show()
        else:
            if not auth.require("Authenticate to deactivate unplugg"):
                return
            self.config["activated"] = False
            self.monitor.stop()
            alarm.stop_siren()
            overlay.hide()
            sender.title = "Activate"
            self.title = IDLE_TITLE
        save_config(self.config)

    def _on_unsafe(self, reason):
        # Runs on the monitor's background thread.
        if self.config["show_overlay"]:
            AppHelper.callAfter(overlay.show)  # UI work must hit the main thread
        alarm.trigger(self.config, reason)

    def _quit(self, _):
        if self.config["activated"] and not auth.require("Authenticate to quit unplugg"):
            return
        self.monitor.stop()
        alarm.stop_siren()
        overlay.hide()
        rumps.quit_application()
