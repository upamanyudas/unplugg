"""Fullscreen "Protected by unplugg" cover on every display while armed.

The first key press, click, scroll or mouse move calls back into the app, which
asks for Touch ID or the login password (LocalAuthentication — we never see the
password; reinventing auth is a bug farm). The cover stays until then.

When the alarm fires it can also flash: plain white at full brightness, so the
webcam sees a lit face. Brightness goes through private DisplayServices (what
Lunar and BetterDisplay use); only the built-in screen answers to it.

Must be called on the main thread.
"""
import ctypes

from AppKit import (
    NSAppearance, NSAppearanceNameDarkAqua, NSApplication,
    NSApplicationDidChangeScreenParametersNotification, NSBackingStoreBuffered, NSColor,
    NSEvent, NSEventMaskFlagsChanged, NSEventMaskKeyDown, NSEventMaskLeftMouseDown,
    NSEventMaskMouseMoved, NSEventMaskOtherMouseDown, NSEventMaskRightMouseDown,
    NSEventMaskScrollWheel, NSFont, NSFontWeightSemibold, NSImageScaleProportionallyUpOrDown,
    NSImageView, NSNotificationCenter, NSOperationQueue, NSScreen, NSScreenSaverWindowLevel,
    NSTextAlignmentCenter, NSTextField, NSView, NSVisualEffectBlendingModeBehindWindow,
    NSVisualEffectMaterialFullScreenUI, NSVisualEffectStateActive, NSVisualEffectView, NSWindow,
    NSWindowCollectionBehaviorCanJoinAllSpaces, NSWindowCollectionBehaviorFullScreenAuxiliary,
    NSWindowCollectionBehaviorStationary, NSWindowStyleMaskBorderless, NSWorkspace,
)
from PyObjCTools import AppHelper
from Quartz import CABasicAnimation

INPUT = (NSEventMaskKeyDown | NSEventMaskFlagsChanged | NSEventMaskMouseMoved
         | NSEventMaskLeftMouseDown | NSEventMaskRightMouseDown | NSEventMaskOtherMouseDown
         | NSEventMaskScrollWheel)

_windows = []
_badges = []    # (dot, text) per display
_monitors = []
_on_input = None
_whites = []      # hidden white sheet per display, shown by the flash
_brightness = {}  # display id -> level before the flash
_alarm = False    # what show() last asked for, redone when displays change
_flashing = False
_observer = None  # display changes, while covered

_DS = ctypes.CDLL("/System/Library/PrivateFrameworks/DisplayServices.framework/DisplayServices")
_DS.DisplayServicesGetBrightness.argtypes = [ctypes.c_uint32, ctypes.POINTER(ctypes.c_float)]
_DS.DisplayServicesSetBrightness.argtypes = [ctypes.c_uint32, ctypes.c_float]


class _Cover(NSWindow):
    def canBecomeKeyWindow(self):
        return True  # borderless windows refuse by default, so keys never reached us


def show(on_input, alarm=False, flash=False):
    """Cover every display, and any plugged in while covered; on later calls just
    flip the badge to the alarm state (and flash). The first input after that
    calls on_input() once."""
    global _on_input, _observer, _alarm, _flashing
    _alarm, _flashing = alarm, _flashing or flash
    if _observer is None:
        _on_input = on_input
        _observer = NSNotificationCenter.defaultCenter().addObserverForName_object_queue_usingBlock_(
            NSApplicationDidChangeScreenParametersNotification, None, NSOperationQueue.mainQueue(),
            _cover_all)
        _cover_all()
        NSApplication.sharedApplication().activateIgnoringOtherApps_(True)
        listen()
    else:
        _set_badges(alarm)
        if flash:
            _flash()


def _cover_all(_note=None):
    """Cover every display afresh: at show(), and when one is added, removed or resized."""
    global _windows
    for win in _windows:
        win.orderOut_(None)
    _badges.clear()
    _whites.clear()
    _windows = [_cover(screen) for screen in NSScreen.screens()]
    for win in _windows[1:]:
        win.orderFrontRegardless()  # only the key window comes forward on its own
    if _windows:  # there may be no display at all for a moment
        _windows[0].makeKeyAndOrderFront_(None)
    _set_badges(_alarm)
    if _flashing:
        _flash()


def _flash():
    for white in _whites:
        white.setHidden_(False)
    for screen in NSScreen.screens():
        display, level = screen.deviceDescription()["NSScreenNumber"], ctypes.c_float()
        if _DS.DisplayServicesGetBrightness(display, ctypes.byref(level)) == 0:  # built-in only
            _brightness.setdefault(display, level.value)
            _DS.DisplayServicesSetBrightness(display, 1.0)


def listen(delay=1.5):
    """Watch for input after `delay` s, so the hand that just clicked Activate
    or closed the Touch ID dialog doesn't set it off again."""
    AppHelper.callLater(delay, _watch)


def _watch():
    if _observer is not None and not _monitors:  # covered, even if no display is right now
        _monitors.extend([
            NSEvent.addLocalMonitorForEventsMatchingMask_handler_(INPUT, _input),
            # input that lands in another app after Cmd-Tab (mouse only: keys need Accessibility)
            NSEvent.addGlobalMonitorForEventsMatchingMask_handler_(INPUT, _input),
        ])


def _unwatch():
    for m in _monitors:
        NSEvent.removeMonitor_(m)
    _monitors.clear()


def _input(event):
    if _monitors:
        _unwatch()
        AppHelper.callAfter(_on_input)  # after this event returns; auth blocks
    return None  # swallow it


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
    global _windows, _observer, _flashing
    _unwatch()
    if _observer is not None:
        NSNotificationCenter.defaultCenter().removeObserver_(_observer)
    _observer, _flashing = None, False
    for display, level in _brightness.items():
        _DS.DisplayServicesSetBrightness(display, level)
    _brightness.clear()
    for win in _windows:
        win.orderOut_(None)
    _windows = []
    _badges.clear()
    _whites.clear()


def _cover(screen):
    frame = screen.frame()
    w, h = frame.size.width, frame.size.height
    win = _Cover.alloc().initWithContentRect_styleMask_backing_defer_(
        frame, NSWindowStyleMaskBorderless, NSBackingStoreBuffered, False)
    win.setLevel_(NSScreenSaverWindowLevel - 1)  # Touch ID's dialog draws at 1000; stay under it
    win.setAcceptsMouseMovedEvents_(True)
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
    blur.addSubview_(_label("Press any key or move the mouse to unlock.", 15,
                            (0, h / 2 - 70, w, 22), NSColor.secondaryLabelColor()))
    _badge(blur, h / 2 - 116)
    white = NSView.alloc().initWithFrame_(((0, 0), (w, h)))
    white.setWantsLayer_(True)
    white.layer().setBackgroundColor_(NSColor.whiteColor().CGColor())
    white.setHidden_(True)
    blur.addSubview_(white)  # last, so it sits on top
    _whites.append(white)
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
