"""Tray app for Windows and Linux. Menu stays glanceable; all settings live in Settings."""
import getpass
import sys
import threading

from PySide6.QtGui import QAction, QCursor
from PySide6.QtNetwork import QLocalServer, QLocalSocket
from PySide6.QtWidgets import QApplication, QMenu, QSystemTrayIcon

from ..config import load_config, save_config
from ..monitor import Monitor
from . import alarm, auth, dialogs, overlay, power, sensors, updater
from .guide import open_guide
from .preferences import open_preferences
from .utils import app_icon, call_after, call_later, tray_icon

NOT_LAPTOP = ("unplugg is made for laptops",
              "It guards the power cable and the lid. This computer has no battery, "
              "so the alarm can't be activated here.")


class UnpluggApp:
    def __init__(self):
        self.qt = QApplication(sys.argv)
        self.qt.setApplicationName("unplugg")
        self.qt.setDesktopFileName("unplugg")  # Linux: GeoClue and the taskbar know us by it
        self.qt.setWindowIcon(app_icon())
        self.qt.setQuitOnLastWindowClosed(False)  # closing Settings leaves the tray icon
        if self._already_running():
            sys.exit()
        self.config = load_config()
        self.config["activated"] = False  # never boot already-armed
        power.release(self.config)  # undoes a crash while armed
        self.laptop = sensors.is_laptop()
        if not self.laptop and dialogs.alert(*NOT_LAPTOP, "Quit", "Open Anyway") == 0:
            sys.exit()
        self.monitor = Monitor(self._on_unsafe, self.config, sensors)
        power.watch(self.qt, lambda: self.monitor.trip("shutdown"))

        self.toggle = QAction("Activate", triggered=self._toggle_activate)
        menu = QMenu()
        menu.addAction(self.toggle)
        menu.addSeparator()
        menu.addAction("Settings…", lambda: open_preferences(self))
        menu.addSeparator()
        menu.addAction("About…", dialogs.show_about)
        menu.addAction("Check for Updates…", lambda: updater.check(self, manual=True))
        menu.addAction("Setup Guide…", lambda: open_guide(self))
        menu.addAction("Help…", dialogs.show_help)
        menu.addAction("Quit", self.quit)
        self.menu = menu
        self.tray = QSystemTrayIcon(tray_icon("idle"))
        self.tray.setToolTip("unplugg")
        self.tray.setContextMenu(menu)
        # Windows opens the menu on a right click; a left click should too
        self.tray.activated.connect(lambda reason: reason == QSystemTrayIcon.ActivationReason.Trigger
                                    and menu.popup(QCursor.pos()))
        self.tray.show()

        call_later(1, alarm.prepare)  # after launch, so the icon shows first
        if not self.config["setup_done"]:
            call_later(1, open_guide, self)
        call_later(10, updater.schedule, self)  # once launch has settled

    def run(self):
        sys.exit(self.qt.exec())

    def _already_running(self):
        """One copy per user. A second launch (the Start menu, say) asks the first
        to open Settings, then quits."""
        name = f"unplugg-{getpass.getuser()}"
        socket = QLocalSocket()
        socket.connectToServer(name)
        if socket.waitForConnected(500):
            return True
        QLocalServer.removeServer(name)  # a stale socket left by a crash
        self.server = QLocalServer()
        self.server.newConnection.connect(lambda: (self.server.nextPendingConnection(),
                                                   open_preferences(self)))
        self.server.listen(name)
        return False

    def _toggle_activate(self):
        if not self.config["activated"]:
            self._activate()
        elif auth.require("turn off the alarm"):
            self._deactivate()

    def _activate(self):
        if not self.laptop:
            dialogs.alert(*NOT_LAPTOP)
            return
        if self.config["monitor_power"] and not sensors.is_plugged_in():
            dialogs.alert("Plug in first", "Your laptop should be plugged in to power before you "
                          'can activate the alarm. Alternatively, turn off "Power unplugged" in Settings.')
            return
        self.config["activated"] = True
        power.hold(self.config)
        self.monitor.start()
        self.toggle.setText("Deactivate")
        self._set_state("armed")
        if self.config["show_overlay"]:
            overlay.show(self._unlock)
        save_config(self.config)

    def _deactivate(self):
        self.config["activated"] = False
        self.monitor.stop()
        power.release(self.config)
        alarm.stop_siren()
        overlay.hide()
        self.toggle.setText("Activate")
        self._set_state("idle")
        save_config(self.config)

    def _unlock(self):
        """Someone touched the overlay: owner check, then disarm or keep guarding."""
        if not auth.require("unlock the screen", overlay.window()):
            overlay.listen()  # cancelled or a stranger: the cover stays
            return
        overlay.hide()
        alarm.stop_siren()  # it's the owner
        if dialogs.alert("Deactivate the alarm?", "Keep Armed hides this cover; "
                         "the alarm keeps watching.", "Deactivate", "Keep Armed") == 0:
            self._deactivate()

    def _set_state(self, state):
        self.tray.setIcon(tray_icon(state))

    def _show_alarm(self):
        if self.config["play_sound"]:
            alarm.start_siren(self.config["speaker_name"])
        self._set_state("alarm")
        if self.config["show_overlay"] or self.config["flash_screen"]:
            overlay.show(self._unlock, alarm=True, flash=self.config["flash_screen"])

    def _on_unsafe(self, reason):
        # the monitor's thread, or the main thread for a shutdown attempt; trigger() blocks
        call_after(self._show_alarm)
        threading.Thread(target=alarm.trigger, args=(self.config, reason), daemon=True).start()

    def quit(self):
        """Owner check if armed; on yes, stop everything and quit."""
        if self.config["activated"] and not auth.require("quit while armed"):
            return
        self.monitor.stop()
        power.release(self.config)
        alarm.stop_siren()
        overlay.hide()
        self.qt.quit()
