"""Setup guide: opens on first launch, and from the menu's Setup Guide….

A Qt wizard with Settings' own rows: how it works, permissions (Windows),
Telegram, launch at login. It's optional: closing it early still counts as seen.
"""
from PySide6.QtWidgets import QLabel, QVBoxLayout, QWizard, QWizardPage

from ..config import save_config
from .preferences import group, permissions_page, switch, telegram_group
from .utils import WINDOWS, app_icon

HOW = [  # title, line
    ("Plug in", "unplugg watches the power cable and the lid."),
    ("Activate", "Click unplugg in the taskbar's tray before you step away."),
    ("If it's unplugged or moved", "A siren sounds. Photos, video and location go to your phone."),
    ("Only you can stop it", "Turning it off asks for Windows Hello or your password." if WINDOWS
     else "Turning it off asks for your password."),
]
TRAY = ("Windows hides new tray icons under the ^ arrow. Drag unplugg onto the taskbar "
        "to keep it in sight." if WINDOWS else
        "On GNOME, tray icons need the AppIndicator extension (Ubuntu has it already).")

_guide = None


def open_guide(app):
    global _guide
    if _guide is None:
        _guide = _Guide(app)
    _guide.show()
    _guide.raise_()
    _guide.activateWindow()


def _how_it_works(ctrl, column):
    group(column, None, [QLabel(f"<b>{title}</b><br>{line}") for title, line in HOW])


def _all_set(ctrl, column):
    group(column, None, [switch(ctrl, "launch_at_login", "Launch at login")])
    tip = QLabel(TRAY)
    tip.setWordWrap(True)
    column.addWidget(tip)


STEPS = [  # title, subtitle, body
    ("Welcome to unplugg", "Your laptop, guarded while you step away.", _how_it_works),
    ("Permissions", "Each one turns on one feature. Skip any you don't need.", permissions_page),
    ("Get alerts on your phone",
     "When your laptop is disturbed, a Telegram bot sends you photos, video and where it is. "
     "Click + to add one.", telegram_group),
    ("You're all set", "Before you step away, click unplugg in the tray and choose Activate.", _all_set),
]


class _Guide(QWizard):
    def __init__(self, app):
        super().__init__()
        self.app, self.cfg = app, app.config
        self.setWindowTitle("unplugg Setup Guide")
        self.setWindowIcon(app_icon())
        self.setPixmap(QWizard.WizardPixmap.LogoPixmap, app_icon().pixmap(64, 64))
        for title, subtitle, body in STEPS:
            if body is permissions_page and not WINDOWS:
                continue
            page = QWizardPage()
            page.setTitle(title)
            page.setSubTitle(subtitle)
            column = QVBoxLayout(page)
            body(self, column)
            column.addStretch()
            self.addPage(page)

    def done(self, result):
        global _guide
        self.cfg["setup_done"] = True
        save_config(self.cfg)
        _guide = None
        super().done(result)
