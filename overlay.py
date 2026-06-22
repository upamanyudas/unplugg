"""Fullscreen "Protected by unplugg" cover shown while armed.

On the first key/mouse the Mac locks (the login window enforces the real
password — we never handle the password ourselves; reinventing auth is a bug
farm). The cover stays until monitoring is deactivated.

Must be called on the main thread.
"""
from AppKit import (
    NSWindow, NSScreen, NSColor, NSImage, NSImageView, NSTextField, NSView,
    NSFont, NSEvent, NSApplication,
    NSWindowStyleMaskBorderless, NSBackingStoreBuffered,
    NSScreenSaverWindowLevel, NSImageScaleProportionallyUpOrDown,
    NSEventMaskKeyDown, NSEventMaskLeftMouseDown, NSEventMaskRightMouseDown,
)

from utils import asset_path, lock_screen

_window = None
_monitor = None


def show():
    global _window, _monitor
    if _window:
        return
    frame = NSScreen.mainScreen().frame()  # ponytail: main screen only; loop NSScreen.screens() for multi-display
    _window = NSWindow.alloc().initWithContentRect_styleMask_backing_defer_(
        frame, NSWindowStyleMaskBorderless, NSBackingStoreBuffered, False)
    _window.setLevel_(NSScreenSaverWindowLevel)
    _window.setBackgroundColor_(NSColor.colorWithCalibratedWhite_alpha_(0.05, 1.0))
    _window.setOpaque_(True)

    content = NSView.alloc().initWithFrame_(frame)
    w, h = frame.size.width, frame.size.height

    logo = NSImageView.alloc().initWithFrame_(((w / 2 - 90, h / 2 - 20), (180, 180)))
    img = NSImage.alloc().initWithContentsOfFile_(asset_path("logo.png"))
    if img:
        logo.setImage_(img)
        logo.setImageScaling_(NSImageScaleProportionallyUpOrDown)
    content.addSubview_(logo)

    label = _label("Protected by unplugg", 34, (w / 2 - 300, h / 2 - 80, 600, 44))
    hint = _label("Press any key or click, then sign in to resume.", 15,
                  (w / 2 - 300, h / 2 - 120, 600, 24), dim=True)
    content.addSubview_(label)
    content.addSubview_(hint)
    _window.setContentView_(content)

    _window.makeKeyAndOrderFront_(None)
    NSApplication.sharedApplication().activateIgnoringOtherApps_(True)

    mask = NSEventMaskKeyDown | NSEventMaskLeftMouseDown | NSEventMaskRightMouseDown
    _monitor = NSEvent.addLocalMonitorForEventsMatchingMask_handler_(mask, _on_input)


def hide():
    global _window, _monitor
    if _monitor:
        NSEvent.removeMonitor_(_monitor)
        _monitor = None
    if _window:
        _window.orderOut_(None)
        _window = None


def _on_input(event):
    lock_screen()
    return None  # swallow the event


def _label(text, size, rect, dim=False):
    (x, y, w, h) = rect
    f = NSTextField.alloc().initWithFrame_(((x, y), (w, h)))
    f.setStringValue_(text)
    f.setBezeled_(False)
    f.setEditable_(False)
    f.setSelectable_(False)
    f.setDrawsBackground_(False)
    f.setAlignment_(2)  # centered
    f.setFont_(NSFont.systemFontOfSize_(size))
    white = 0.6 if dim else 1.0
    f.setTextColor_(NSColor.colorWithCalibratedWhite_alpha_(white, 1.0))
    return f
