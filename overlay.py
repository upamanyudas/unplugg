"""Fullscreen "Protected by unplugg" cover on every display while armed.

On the first key/mouse the Mac locks (the login window enforces the real
password — we never handle the password ourselves; reinventing auth is a bug
farm). The cover stays until monitoring is deactivated.

Must be called on the main thread.
"""
from AppKit import (
    NSAppearance, NSAppearanceNameDarkAqua, NSApplication, NSBackingStoreBuffered, NSColor,
    NSEvent, NSEventMaskKeyDown, NSEventMaskLeftMouseDown, NSEventMaskRightMouseDown, NSFont,
    NSFontWeightSemibold, NSImageScaleProportionallyUpOrDown, NSImageView, NSScreen,
    NSScreenSaverWindowLevel, NSTextAlignmentCenter, NSTextField, NSView,
    NSVisualEffectBlendingModeBehindWindow,
    NSVisualEffectMaterialFullScreenUI, NSVisualEffectStateActive, NSVisualEffectView, NSWindow,
    NSWindowCollectionBehaviorCanJoinAllSpaces, NSWindowCollectionBehaviorFullScreenAuxiliary,
    NSWindowCollectionBehaviorStationary, NSWindowStyleMaskBorderless, NSWorkspace,
)
from Quartz import CABasicAnimation

from utils import lock_screen

_windows = []
_badges = []   # (dot, text) per display
_monitor = None


def show(alarm=False):
    """Cover every display; on later calls just flip the badge to the alarm state."""
    global _windows, _monitor
    if not _windows:
        _windows = [_cover(screen) for screen in NSScreen.screens()]
        _windows[0].makeKeyAndOrderFront_(None)
        NSApplication.sharedApplication().activateIgnoringOtherApps_(True)
        mask = NSEventMaskKeyDown | NSEventMaskLeftMouseDown | NSEventMaskRightMouseDown
        _monitor = NSEvent.addLocalMonitorForEventsMatchingMask_handler_(mask, _on_input)
    _set_badges(alarm)


def _set_badges(alarm):
    color = NSColor.systemRedColor() if alarm else NSColor.systemGreenColor()
    for dot, text in _badges:
        dot.layer().setBackgroundColor_(color.CGColor())
        text.setStringValue_("Alarm triggered" if alarm else "Armed")
        text.sizeToFit()
        # keep "● word" centred as one unit
        x = (text.superview().frame().size.width - text.frame().size.width - 16) / 2
        dot.setFrameOrigin_((x, dot.frame().origin.y))
        text.setFrameOrigin_((x + 16, text.frame().origin.y))


def hide():
    global _windows, _monitor
    if _monitor:
        NSEvent.removeMonitor_(_monitor)
        _monitor = None
    for win in _windows:
        win.orderOut_(None)
    _windows = []
    _badges.clear()


def _on_input(event):
    lock_screen()
    return None  # swallow the event


def _cover(screen):
    frame = screen.frame()
    w, h = frame.size.width, frame.size.height
    win = NSWindow.alloc().initWithContentRect_styleMask_backing_defer_(
        frame, NSWindowStyleMaskBorderless, NSBackingStoreBuffered, False)
    win.setLevel_(NSScreenSaverWindowLevel)
    # follow every Space and full-screen app, so swiping desktops can't escape it
    win.setCollectionBehavior_(NSWindowCollectionBehaviorCanJoinAllSpaces
                               | NSWindowCollectionBehaviorStationary
                               | NSWindowCollectionBehaviorFullScreenAuxiliary)
    win.setAppearance_(NSAppearance.appearanceNamed_(NSAppearanceNameDarkAqua))
    win.setOpaque_(False)
    win.setBackgroundColor_(NSColor.clearColor())

    blur = NSVisualEffectView.alloc().initWithFrame_(((0, 0), (w, h)))
    blur.setMaterial_(NSVisualEffectMaterialFullScreenUI)
    blur.setBlendingMode_(NSVisualEffectBlendingModeBehindWindow)
    blur.setState_(NSVisualEffectStateActive)
    win.setContentView_(blur)

    logo = NSImageView.alloc().initWithFrame_(((w / 2 - 80, h / 2 + 20), (160, 160)))
    logo.setImage_(NSApplication.sharedApplication().applicationIconImage())
    logo.setImageScaling_(NSImageScaleProportionallyUpOrDown)
    blur.addSubview_(logo)
    blur.addSubview_(_label("Protected by unplugg", 34, (0, h / 2 - 36, w, 44), bold=True))
    blur.addSubview_(_label("Press any key or click, then sign in to resume.", 15,
                            (0, h / 2 - 70, w, 22), NSColor.secondaryLabelColor()))
    _badge(blur, h / 2 - 116)
    return win


def _badge(parent, y):
    """Pulsing status dot + word; show() sets its colour, text and position."""
    text = _label("", 13, (0, y, 0, 18), NSColor.secondaryLabelColor(), center=False)
    dot = NSView.alloc().initWithFrame_(((0, y + 4), (10, 10)))
    dot.setWantsLayer_(True)
    dot.layer().setCornerRadius_(5)
    if not NSWorkspace.sharedWorkspace().accessibilityDisplayShouldReduceMotion():
        pulse = CABasicAnimation.animationWithKeyPath_("opacity")
        pulse.setFromValue_(1.0)
        pulse.setToValue_(0.25)
        pulse.setDuration_(1.0)
        pulse.setAutoreverses_(True)
        pulse.setRepeatCount_(float("inf"))
        dot.layer().addAnimation_forKey_(pulse, "pulse")
    parent.addSubview_(dot)
    parent.addSubview_(text)
    _badges.append((dot, text))


def _label(text, size, rect, color=None, bold=False, center=True):
    (x, y, w, h) = rect
    f = NSTextField.labelWithString_(text)
    f.setFrame_(((x, y), (w, h)))
    if center:
        f.setAlignment_(NSTextAlignmentCenter)
    f.setFont_(NSFont.systemFontOfSize_weight_(size, NSFontWeightSemibold) if bold
               else NSFont.systemFontOfSize_(size))
    if color:
        f.setTextColor_(color)
    return f
