"""Menu bar app. Menu stays glanceable; all settings live in Settings."""
import sys

import rumps
from AppKit import NSApplication, NSImage, NSProcessInfo
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

# outline = idle, filled = armed, red = alarm fired
GLYPHS = {"idle": "menubar.pdf", "armed": "menubar_armed.pdf", "alarm": "menubar_alarm.pdf"}
# Ventura renamed Preferences to Settings; say whichever this macOS says
SETTINGS = ("Settings…" if NSProcessInfo.processInfo().isOperatingSystemAtLeastVersion_((13, 0, 0))
            else "Preferences…")


class UnpluggApp(rumps.App):
    def __init__(self):
        super().__init__("unplugg", icon=asset_path(GLYPHS["idle"]), template=True,
                         quit_button=None)
        if not getattr(sys, "frozen", False):  # from source: our icon in alerts, not Python's
            NSApplication.sharedApplication().setApplicationIconImage_(
                NSImage.alloc().initWithContentsOfFile_(asset_path("logo.png")))
        self.config = load_config()
        self.config["activated"] = False  # never boot already-armed
        self.monitor = Monitor(self._on_unsafe, self.config)
        self.menu = [
            rumps.MenuItem("Activate", callback=self._toggle_activate),
            None,
            rumps.MenuItem(SETTINGS, callback=lambda _: open_preferences(self), key=","),
            None,
            rumps.MenuItem("About…", callback=lambda _: dialogs.show_about()),
            rumps.MenuItem("Help…", callback=lambda _: dialogs.show_help()),
            rumps.MenuItem("Quit", callback=self._quit, key="q"),
        ]

    def _toggle_activate(self, sender):
        if not self.config["activated"]:
            if self.config["monitor_power"] and not sensors.is_plugged_in():
                rumps.alert("Plug in first", "Your MacBook should be plugged to power before "
                            "you can activate the alarm. Alternatively, turn off "
                            f'"Power unplugged" in {SETTINGS[:-1]}.')
                return
            self.config["activated"] = True
            self.monitor.start()
            sender.title = "Deactivate"
            self._set_state("armed")
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
            self._set_state("idle")
        save_config(self.config)

    def _set_state(self, state):
        self.icon = asset_path(GLYPHS[state])
        # templates follow the light/dark menu bar; the menu bar ignores tint on
        # templates, so the red alarm glyph has to be a plain colour image
        self.template = state != "alarm"

    def _show_alarm(self):
        self._set_state("alarm")
        if self.config["show_overlay"]:
            overlay.show(alarm=True)

    def _on_unsafe(self, reason):
        # Runs on the monitor's background thread; UI work must hit the main thread.
        AppHelper.callAfter(self._show_alarm)
        alarm.trigger(self.config, reason)

    def _quit(self, _):
        if self.config["activated"] and not auth.require("Authenticate to quit unplugg"):
            return
        self.monitor.stop()
        alarm.stop_siren()
        overlay.hide()
        rumps.quit_application()
