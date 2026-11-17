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
NOT_LAPTOP = ("unplugg is made for MacBooks",
              "It guards the power cable and the lid. This Mac has no battery or lid, "
              "so the alarm can't be activated here.")


class UnpluggApp(rumps.App):
    def __init__(self):
        super().__init__("unplugg", icon=asset_path(GLYPHS["idle"]), template=True,
                         quit_button=None)
        if not getattr(sys, "frozen", False):  # from source: our icon in alerts, not Python's
            NSApplication.sharedApplication().setApplicationIconImage_(
                NSImage.alloc().initWithContentsOfFile_(asset_path("logo.png")))
        dialogs.install_edit_menu()
        self.laptop = sensors.is_laptop()
        if not self.laptop and dialogs.alert(*NOT_LAPTOP, "Quit", "Open Anyway") == 0:
            sys.exit()
        self.config = load_config()
        self.config["activated"] = False  # never boot already-armed
        self.monitor = Monitor(self._on_unsafe, self.config)
        self.toggle = rumps.MenuItem("Activate", callback=self._toggle_activate)
        self.menu = [
            self.toggle,
            None,
            rumps.MenuItem(SETTINGS, callback=lambda _: open_preferences(self), key=","),
            None,
            rumps.MenuItem("About…", callback=lambda _: dialogs.show_about()),
            rumps.MenuItem("Help…", callback=lambda _: dialogs.show_help()),
            rumps.MenuItem("Quit", callback=self._quit, key="q"),
        ]

    def _toggle_activate(self, _):
        if not self.config["activated"]:
            self._activate()
        elif auth.require("turn off the alarm"):  # macOS shows "unplugg is trying to …"
            self._deactivate()

    def _activate(self):
        if not self.laptop:
            dialogs.alert(*NOT_LAPTOP)
            return
        if self.config["monitor_power"] and not sensors.is_plugged_in():
            dialogs.alert("Plug in first", "Your MacBook should be plugged to power before "
                          "you can activate the alarm. Alternatively, turn off "
                          f'"Power unplugged" in {SETTINGS[:-1]}.')
            return
        self.config["activated"] = True
        self.monitor.start()
        self.toggle.title = "Deactivate"
        self._set_state("armed")
        if self.config["show_overlay"]:
            overlay.show(self._unlock)
        save_config(self.config)

    def _deactivate(self):
        self.config["activated"] = False
        self.monitor.stop()
        alarm.stop_siren()
        overlay.hide()
        self.toggle.title = "Activate"
        self._set_state("idle")
        save_config(self.config)

    def _unlock(self):
        """Someone touched the overlay: owner check, then disarm or keep guarding."""
        if not auth.require("unlock the screen"):
            overlay.listen()  # cancelled or a stranger: the cover stays
            return
        overlay.hide()
        alarm.stop_siren()  # it's the owner
        if dialogs.alert("Deactivate the alarm?", "Keep Armed hides this cover; "
                         "the alarm keeps watching.", "Deactivate", "Keep Armed") == 0:
            self._deactivate()

    def _set_state(self, state):
        self.icon = asset_path(GLYPHS[state])
        # templates follow the light/dark menu bar; the menu bar ignores tint on
        # templates, so the red alarm glyph has to be a plain colour image
        self.template = state != "alarm"

    def _show_alarm(self):
        self._set_state("alarm")
        if self.config["show_overlay"]:
            overlay.show(self._unlock, alarm=True)

    def _on_unsafe(self, reason):
        # Runs on the monitor's background thread; UI work must hit the main thread.
        AppHelper.callAfter(self._show_alarm)
        alarm.trigger(self.config, reason)

    def _quit(self, _):
        if self.config["activated"] and not auth.require("quit while armed"):
            return
        self.monitor.stop()
        alarm.stop_siren()
        overlay.hide()
        rumps.quit_application()
