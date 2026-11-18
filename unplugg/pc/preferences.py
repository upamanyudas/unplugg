"""Settings window, shaped like the macOS one: General / Alerts / Devices /
Permissions tabs, each a stack of grouped rows.

ponytail: stock Qt widgets in each OS's own style, laid out by Qt's layouts.
The page builders are module functions taking (ctrl, layout), so the setup
guide builds the same rows.
"""
from pathlib import Path

from PySide6.QtCore import QEvent, Qt
from PySide6.QtWidgets import (
    QCheckBox, QComboBox, QFileDialog, QFormLayout, QGroupBox, QHBoxLayout, QLabel, QPushButton,
    QTabWidget, QToolButton, QVBoxLayout, QWidget,
)

from ..config import save_config
from .. import telegram
from . import devices, dialogs, loginitem, permissions, sensors
from .utils import WINDOWS, app_icon, open_url

PERMS = [  # key, label, why
    ("camera", "Camera", "Photos and video of a thief."),
    ("microphone", "Microphone", "Sound in that video."),
    ("location", "Location", "A map link to your laptop."),
]
STATUS = {"granted": ("● Allowed", "#34c759"), "denied": ("● Blocked", "#ff3b30")}

_window = None


def open_preferences(app):
    global _window
    if _window is None:
        _window = _Settings(app)
    _window.show()
    _window.raise_()
    _window.activateWindow()


class _Settings(QTabWidget):
    def __init__(self, app):
        super().__init__()
        self.app, self.cfg = app, app.config
        self.setWindowTitle("unplugg Settings")
        self.setWindowIcon(app_icon())
        pages = [("General", general_page), ("Alerts", alerts_page), ("Devices", devices_page)]
        if WINDOWS:  # Linux has no per-app camera, mic or location switches
            pages.append(("Permissions", permissions_page))
        for title, build in pages:
            page = QWidget()
            column = QVBoxLayout(page)
            build(self, column)
            column.addStretch()
            self.addTab(page, title)
        self.setCurrentIndex(min(self.cfg["settings_pane"], self.count() - 1))

    def event(self, e):
        if e.type() == QEvent.Type.WindowActivate:
            refresh_perms(self)  # pick up changes made in Windows Settings
        return super().event(e)

    def closeEvent(self, e):
        global _window
        self.cfg["settings_pane"] = self.currentIndex()
        save_config(self.cfg)
        _window = None
        super().closeEvent(e)


# ---- pages ------------------------------------------------------------------
def general_page(ctrl, column):
    rows = [switch(ctrl, "monitor_power", "Power unplugged"),
            switch(ctrl, "monitor_lid", "Lid closed")]
    if sensors.acceleration() is not None:
        rows.append(switch(ctrl, "monitor_motion", "Moved (motion sensor)"))
    group(column, "Alarm when", rows)
    group(column, "While armed", [switch(ctrl, "show_overlay", "Show overlay"),
                                  switch(ctrl, "lock_device", "Lock when triggered")])
    group(column, None, [switch(ctrl, "launch_at_login", "Launch at login")])
    group(column, "Updates", [switch(ctrl, "check_updates", "Check for updates automatically"),
                              switch(ctrl, "auto_update", "Install updates automatically")])


def alerts_page(ctrl, column):
    group(column, "When triggered", [
        switch(ctrl, "play_sound", "Play siren at full volume"),
        switch(ctrl, "capture_media", "Send photo && video"),
        switch(ctrl, "flash_screen", "Flash screen for photos"),
        switch(ctrl, "record_audio", "Record audio with video"),
        switch(ctrl, "send_location", "Send location")])
    telegram_group(ctrl, column)
    folder = QLabel(_short(ctrl.cfg["media_dir"]))
    folder.setTextInteractionFlags(Qt.TextInteractionFlag.TextSelectableByMouse)

    def choose():
        path = QFileDialog.getExistingDirectory(ctrl, "Save photos and video in", ctrl.cfg["media_dir"])
        if path:
            ctrl.cfg["media_dir"] = path
            save_config(ctrl.cfg)
            folder.setText(_short(path))

    def reveal():
        path = Path(ctrl.cfg["media_dir"]).expanduser()
        path.mkdir(parents=True, exist_ok=True)  # capture makes it lazily
        open_url(str(path))

    group(column, "Saved media", [row(folder, 1, button("Choose…", choose), button("Open", reveal))])


def telegram_group(ctrl, column):
    ctrl.dests = QComboBox()
    ctrl.dests.setMinimumWidth(160)
    add, remove, test = QToolButton(text="+"), QToolButton(text="−"), button("Send Test", None)

    def refresh():
        ctrl.dests.clear()
        names = [d["name"] for d in ctrl.cfg["telegram_destinations"]]
        ctrl.dests.addItems(names or ["None yet"])
        for w in (ctrl.dests, remove, test):
            w.setEnabled(bool(names))

    def added():
        dest = dialogs.setup_telegram(ctrl)
        if dest:
            ctrl.cfg["telegram_destinations"].append(dest)
            save_config(ctrl.cfg)
            refresh()

    def removed():
        del ctrl.cfg["telegram_destinations"][ctrl.dests.currentIndex()]
        save_config(ctrl.cfg)
        refresh()

    def tested():
        dest = ctrl.cfg["telegram_destinations"][ctrl.dests.currentIndex()]
        err = telegram.send_test(dest["token"], dest["chat_id"])
        dialogs.alert("Test failed" if err else "Test sent",
                      f"Telegram says: {err}" if err else f"Check Telegram for {dest['name']}.", parent=ctrl)

    add.clicked.connect(added)
    remove.clicked.connect(removed)
    test.clicked.connect(tested)
    refresh()
    group(column, "Telegram", [row(QLabel("Destinations"), 1, ctrl.dests, add, remove, test)])


def devices_page(ctrl, column):
    form = QFormLayout()
    form.addRow("Camera", picker(ctrl, devices.cameras() or ["Default camera"], "camera",
                                 devices.camera(ctrl.cfg["camera"]).description()))
    form.addRow("Microphone", picker(ctrl, devices.microphones() or ["Default microphone"], "mic",
                                     devices.microphone(ctrl.cfg["mic"]).description()))
    box = QGroupBox("Capture")
    box.setLayout(form)
    column.addWidget(box)
    speaker = QFormLayout()
    speaker.addRow("Speaker", picker(ctrl, ["System default"] + devices.speakers(), "speaker_name",
                                     ctrl.cfg["speaker_name"], default_first=True))
    box = QGroupBox("Siren")
    box.setLayout(speaker)
    column.addWidget(box)


def permissions_page(ctrl, column):
    ctrl.permrows = {}
    rows = []
    for key, label, why in PERMS:
        status, settings = QLabel(), button("Open Settings…", lambda _=False, k=key: permissions.open_settings(k))
        ctrl.permrows[key] = (status, settings)
        text = QLabel(f"{label}<br><small>{why}</small>")
        rows.append(row(text, 1, status, settings))
    group(column, "All optional", rows)
    refresh_perms(ctrl)


def refresh_perms(ctrl):
    for key, (status, settings) in getattr(ctrl, "permrows", {}).items():
        text, color = STATUS[permissions.status(key)]
        status.setText(text)
        status.setStyleSheet(f"color: {color}")
        settings.setHidden(permissions.status(key) == "granted")


# ---- row builders -------------------------------------------------------------
def group(column, title, rows):
    """A titled box of rows; a row is any widget or layout."""
    box = QGroupBox(title or "")
    stack = QVBoxLayout(box)
    for r in rows:
        stack.addLayout(r) if isinstance(r, QHBoxLayout) else stack.addWidget(r)
    column.addWidget(box)


def row(*items):
    """Widgets left to right; a number is that much stretch."""
    line = QHBoxLayout()
    for item in items:
        line.addStretch(item) if isinstance(item, int) else line.addWidget(item)
    return line


def switch(ctrl, key, text):
    box = QCheckBox(text)
    box.setChecked(bool(ctrl.cfg[key]))

    def toggled(on):
        ctrl.cfg[key] = on
        save_config(ctrl.cfg)
        if key == "launch_at_login":
            loginitem.enable() if on else loginitem.disable()
    box.toggled.connect(toggled)
    return box


def picker(ctrl, items, key, selected, default_first=False):
    pop = QComboBox()
    pop.addItems(items)
    pop.setCurrentIndex(items.index(selected) if selected in items else 0)

    def picked(i):
        ctrl.cfg[key] = "" if default_first and i == 0 else pop.itemText(i)
        save_config(ctrl.cfg)
    pop.currentIndexChanged.connect(picked)
    return pop


def button(title, clicked):
    b = QPushButton(title)
    if clicked:
        b.clicked.connect(clicked)
    return b


def _short(path):
    return path.replace(str(Path.home()), "~")
