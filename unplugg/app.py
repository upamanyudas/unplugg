"""Menu bar app. Menu stays glanceable; all settings live in Settings."""
import sys

import objc
import rumps
from AppKit import NSProcessInfo, NSTerminateCancel, NSTerminateNow
from PyObjCTools import AppHelper

from . import alarm, auth, dialogs, overlay, sensors, updater
from .config import load_config, save_config
from .guide import open_guide
from .monitor import Monitor
from .preferences import open_preferences
from .utils import asset_path, show_in_dock

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
        self.config = load_config()
        self.config["activated"] = False  # never boot already-armed
        show_in_dock(self.config["show_in_dock"])  # also sets our icon when run from source
        dialogs.install_main_menu()
        self.laptop = sensors.is_laptop()
        if not self.laptop and dialogs.alert(*NOT_LAPTOP, "Quit", "Open Anyway") == 0:
            sys.exit()
        UnpluggApp.instance = self
        self.monitor = Monitor(self._on_unsafe, self.config, sensors)
        AppHelper.callLater(1, alarm.prepare)  # after launch, so the icon shows first
        if not self.config["setup_done"]:
            AppHelper.callLater(1, open_guide, self)
        AppHelper.callLater(10, updater.schedule, self)  # once launch has settled
        self.toggle = rumps.MenuItem("Activate", callback=self._toggle_activate)
        self.menu = [
            self.toggle,
            None,
            rumps.MenuItem(SETTINGS, callback=lambda _: open_preferences(self), key=","),
            None,
            rumps.MenuItem("About…", callback=lambda _: dialogs.show_about()),
            rumps.MenuItem("Check for Updates…", callback=lambda _: updater.check(self, manual=True)),
            rumps.MenuItem("Setup Guide…", callback=lambda _: open_guide(self)),
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
        if self.config["play_sound"]:
            alarm.start_siren(self.config["speaker_name"])
        self._set_state("alarm")
        if self.config["show_overlay"] or self.config["flash_screen"]:
            overlay.show(self._unlock, alarm=True, flash=self.config["flash_screen"])

    def _on_unsafe(self, reason):
        # Runs on the monitor's background thread; UI work must hit the main thread.
        AppHelper.callAfter(self._show_alarm)
        alarm.trigger(self.config, reason)

    def _quit(self, _):
        rumps.quit_application()  # → applicationShouldTerminate_ below

    def allow_quit(self):
        """Touch ID if armed; on yes, stop everything. Returns whether to quit."""
        if self.config["activated"] and not auth.require("quit while armed"):
            return False
        self.monitor.stop()
        alarm.stop_siren()
        overlay.hide()
        return True


class NSApp(objc.Category(rumps.rumps.NSApp)):
    """Every quit (menu, Cmd+Q, the Dock, logout) asks rumps' app delegate first,
    so a Dock icon can't become a way around Touch ID."""

    def applicationShouldTerminate_(self, _):
        return NSTerminateNow if UnpluggApp.instance.allow_quit() else NSTerminateCancel
