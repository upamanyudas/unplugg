"""Full-screen "Protected by unplugg" cover on every display while armed.

The first key press, click, scroll or mouse move calls back into the app, which
asks for the owner (auth.py). The cover stays until then.

When the alarm fires it can also flash: plain white at full brightness, so the
webcam sees a lit face. Brightness goes through WMI on Windows and logind on
Linux (no root needed); only the built-in screen answers to either.

Must be called on the main thread.
"""
import subprocess
from pathlib import Path

from PySide6.QtCore import QPropertyAnimation, Qt
from PySide6.QtGui import QColor, QGuiApplication, QPainter
from PySide6.QtWidgets import QGraphicsOpacityEffect, QHBoxLayout, QLabel, QVBoxLayout, QWidget

from .utils import WINDOWS, app_icon, call_later

_covers = []
_on_input = None   # set while covered
_watching = False
_alarm = False     # what show() last asked for, for displays plugged in later
_flashing = False
_brightness = {}  # built-in display -> level before the flash


class _Cover(QWidget):
    def __init__(self, screen):
        super().__init__(None, Qt.WindowType.FramelessWindowHint | Qt.WindowType.WindowStaysOnTopHint
                         | Qt.WindowType.Tool)  # Tool: no taskbar button
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.setMouseTracking(True)
        self.white = False
        self.content = QWidget(self)
        self.content.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)  # moves reach the cover
        column = QVBoxLayout(self.content)
        column.addStretch()
        logo = QLabel(pixmap=app_icon().pixmap(160, 160))
        title = _label("Protected by unplugg", 30, bold=True)
        hint = _label("Press any key or move the mouse to unlock.", 14, "#bbbbbb")
        self.dot, self.badge = _label("●", 12), _label("", 12, "#bbbbbb")
        badge = QHBoxLayout()
        badge.addStretch()
        badge.addWidget(self.dot)
        badge.addWidget(self.badge)
        badge.addStretch()
        for w in (logo, title, hint):
            column.addWidget(w, alignment=Qt.AlignmentFlag.AlignHCenter)
        column.addLayout(badge)
        column.addStretch()
        QVBoxLayout(self).addWidget(self.content)
        self.home = screen  # Qt moves the cover onto the primary display once this one goes
        self.setScreen(screen)
        self.setGeometry(screen.geometry())
        self.pulse = QPropertyAnimation(QGraphicsOpacityEffect(self.dot), b"opacity")
        self.dot.setGraphicsEffect(self.pulse.targetObject())
        self.pulse.setDuration(2000)
        self.pulse.setKeyValues([(0, 1.0), (0.5, 0.25), (1, 1.0)])
        self.pulse.setLoopCount(-1)
        if not _reduce_motion():
            self.pulse.start()

    def paintEvent(self, _):
        QPainter(self).fillRect(self.rect(), QColor("white") if self.white else QColor(18, 18, 22, 235))

    def flash(self):
        self.white = True
        self.content.hide()
        self.update()

    def keyPressEvent(self, _):
        _input()

    def mousePressEvent(self, _):
        _input()

    def mouseMoveEvent(self, _):
        _input()

    def wheelEvent(self, _):
        _input()


def show(on_input, alarm=False, flash=False):
    """Cover every display, and any plugged in while covered; on later calls just
    flip the badge to the alarm state (and flash). The first input after that
    calls on_input() once."""
    global _on_input, _alarm, _flashing
    _alarm, _flashing = alarm, _flashing or flash
    _sync()
    if _on_input is None:
        _on_input = on_input
        app = QGuiApplication.instance()
        app.screenAdded.connect(_sync)
        app.screenRemoved.connect(_sync)
        if _covers:
            _covers[0].activateWindow()
        listen()
    if flash:
        _brighten()


def _sync(_screen=None):
    """One cover per display, as displays come and go; each shows the alarm state."""
    screens = QGuiApplication.screens()
    for cover in [c for c in _covers if c.home not in screens]:
        _covers.remove(cover)
        cover.close()  # before Qt moves it onto the primary display
        cover.deleteLater()
    for screen in screens:
        if screen not in [c.home for c in _covers]:
            cover = _Cover(screen)
            cover.showFullScreen()
            _covers.append(cover)
    for cover in _covers:
        cover.dot.setStyleSheet(f"color: {'#ff3b30' if _alarm else '#34c759'}")
        cover.badge.setText("Alarm triggered" if _alarm else "Armed")
        if _flashing:
            cover.flash()


def window():
    """The first cover's native handle, for dialogs that must sit above it."""
    return int(_covers[0].winId()) if _covers else None


def listen(delay=1.5):
    """Watch for input after `delay` s, so the hand that just clicked Activate
    or closed the owner check doesn't set it off again."""
    call_later(delay, _watch)


def _watch():
    global _watching
    _watching = _on_input is not None  # covered, even if no display is right now


def _input():
    global _watching
    if _watching:
        _watching = False
        call_later(0, _on_input)  # after this event returns; auth blocks


def hide():
    global _watching, _on_input, _flashing
    _watching = False
    if _on_input is not None:
        app = QGuiApplication.instance()
        app.screenAdded.disconnect(_sync)
        app.screenRemoved.disconnect(_sync)
    _on_input, _flashing = None, False
    if _brightness:
        _set_levels(_brightness)
        _brightness.clear()
    for cover in _covers:
        cover.close()
        cover.deleteLater()
    _covers.clear()


def _brighten():
    """Built-in screen to full for the flash; _sync() turns the covers white."""
    try:
        levels = _levels()
        _brightness.update({k: v for k, v in levels.items() if k not in _brightness})
        _set_levels({k: _full(k) for k in levels})
    except Exception as e:  # no built-in screen, or no logind: the white still helps
        print(f"brightness: {e}")


def _label(text, size, color="white", bold=False):
    label = QLabel(text)
    label.setStyleSheet(f"color: {color}; font-size: {size}pt; font-weight: {600 if bold else 400}")
    return label


if WINDOWS:
    def _wmi():
        import comtypes.client  # comes with pycaw
        return comtypes.client.CoGetObject(r"winmgmts:\\.\root\WMI")

    def _levels():
        return {m.InstanceName: m.CurrentBrightness for m in _wmi().InstancesOf("WmiMonitorBrightness")}

    def _set_levels(levels):
        for m in _wmi().InstancesOf("WmiMonitorBrightnessMethods"):
            if m.InstanceName in levels:
                m.WmiSetBrightness(1, levels[m.InstanceName])  # 1 s timeout

    def _full(_):
        return 100

    def _reduce_motion():
        """Settings › Ease of Access › Display › Show animations in Windows."""
        import ctypes
        on = ctypes.c_bool()
        ctypes.windll.user32.SystemParametersInfoW(0x1042, 0, ctypes.byref(on), 0)  # SPI_GETCLIENTAREAANIMATION
        return not on.value
else:
    BACKLIGHT = Path("/sys/class/backlight")

    def _levels():
        return {d.name: int((d / "brightness").read_text()) for d in BACKLIGHT.glob("*")}

    def _set_levels(levels):
        for name, level in levels.items():
            subprocess.run(["busctl", "call", "org.freedesktop.login1", "/org/freedesktop/login1/session/auto",
                            "org.freedesktop.login1.Session", "SetBrightness", "ssu", "backlight", name,
                            str(level)], capture_output=True)

    def _full(name):
        return int((BACKLIGHT / name / "max_brightness").read_text())

    def _reduce_motion():
        return False
