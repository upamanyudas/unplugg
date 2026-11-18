"""Setup guide: opens on first launch, and from the menu's Setup Guide….

Four steps with Back / Continue: how it works, permissions, Telegram, launch at
login. The rows are Settings' own (same controller and builders), so they
behave alike. Per the HIG it's optional: closing it early still counts as seen.
"""
from AppKit import (
    NSApplication, NSBackingStoreBuffered, NSColor, NSImageView, NSTextAlignmentCenter,
    NSViewMaxYMargin, NSViewMinYMargin, NSWindow, NSWindowStyleMaskClosable,
    NSWindowStyleMaskFullSizeContentView, NSWindowStyleMaskTitled, NSWindowTitleHidden,
)
from Foundation import NSMakeRect

from config import save_config
from preferences import (
    INSET, PAD, TALL, W, _Page, _Prefs, _button, _group, _info, _label, _permissions_page,
    _refresh_perms, _switch, _telegram_group, _tile,
)
from utils import show_in_dock

_guide = None  # retain while open

ICON = 72
FOOTER = 56
HOW = [  # title, line, SF Symbol, tile colour
    ("Plug in", "unplugg watches the power cable and the lid.", "bolt.fill",
     NSColor.systemGreenColor),
    ("Activate", "Click unplugg in the menu bar before you step away.", "lock.fill",
     NSColor.systemBlueColor),
    ("If it's unplugged or moved", "A siren sounds. Photos, video and location go to your phone.",
     "bell.fill", NSColor.systemRedColor),
    ("Only you can stop it", "Turning it off asks for Touch ID or your password.", "touchid",
     NSColor.systemPinkColor),
]


def _how_it_works(ctrl, view, y):
    width = W - 2 * PAD - 2 * INSET
    return _group(view, y, None, [
        (_info(title, line, _tile(symbol, color()), width), None)
        for title, line, symbol, color in HOW], TALL)


STEPS = [  # title, subtitle, body
    ("Welcome to unplugg", "Your Mac, guarded while you step away.", _how_it_works),
    ("Permissions", "Each one turns on one feature. Skip any you don't need.", _permissions_page),
    ("Get alerts on your phone",
     "When your Mac is disturbed, a Telegram bot sends you photos, video and where it is. "
     "Click + to add one.",
     _telegram_group),
    ("You're all set",
     "Before you step away, click unplugg in the menu bar and choose Activate.",
     lambda ctrl, view, y: _group(view, y, None, [
         ("Launch at login", _switch(ctrl, "launch_at_login"))])),
]


def open_guide(app):
    global _guide
    if _guide is None:
        _guide = _build(app)
        _guide.window.center()
    show_in_dock(True)  # a window is open: Dock icon and Cmd-Tab, like any app
    _guide.window.makeKeyAndOrderFront_(None)
    NSApplication.sharedApplication().activateIgnoringOtherApps_(True)


class _Guide(_Prefs):
    """Settings' controller, plus paging."""

    def next_(self, _):
        if self.step == len(STEPS) - 1:
            self.window.close()
        else:
            _show(self, self.step + 1)

    def back_(self, _):
        _show(self, self.step - 1)

    def windowWillClose_(self, _):
        global _guide
        self.cfg["setup_done"] = True
        save_config(self.cfg)
        show_in_dock(self.cfg["show_in_dock"])
        _guide = None


def _build(app):
    ctrl = _Guide.alloc().init()
    ctrl.app, ctrl.cfg, ctrl.checks = app, app.config, {}
    ctrl.pages = [_page(ctrl, *step) for step in STEPS]
    height = ctrl.pages[0].frame().size.height + FOOTER  # _show fits the window to each step
    root = _Page.alloc().initWithFrame_(NSMakeRect(0, 0, W, height))
    for page in ctrl.pages:
        page.setAutoresizingMask_(NSViewMaxYMargin)  # flipped, so this pins it to the top
        root.addSubview_(page)

    # footer: a dot per step on the left, Back / Continue on the right
    ctrl.dots = [_label("●", 10) for _ in STEPS]
    for i, dot in enumerate(ctrl.dots):
        dot.setFrameOrigin_((PAD + 14 * i, height - (FOOTER + dot.frame().size.height) / 2))
    ctrl.backbtn = _button("Back", ctrl, "back:")
    ctrl.nextbtn = _button("Continue", ctrl, "next:")
    ctrl.nextbtn.setKeyEquivalent_("\r")  # the default button
    for i, btn in enumerate((ctrl.nextbtn, ctrl.backbtn)):
        size = (96, btn.frame().size.height)
        btn.setFrameSize_(size)
        btn.setFrameOrigin_((W - PAD - (size[0] + 8) * i - size[0], height - (FOOTER + size[1]) / 2))
    for v in ctrl.dots + [ctrl.backbtn, ctrl.nextbtn]:
        v.setAutoresizingMask_(NSViewMinYMargin)  # pinned to the bottom
        root.addSubview_(v)

    style = NSWindowStyleMaskTitled | NSWindowStyleMaskClosable | NSWindowStyleMaskFullSizeContentView
    win = NSWindow.alloc().initWithContentRect_styleMask_backing_defer_(
        NSMakeRect(0, 0, W, height), style, NSBackingStoreBuffered, False)
    win.setTitle_("Setup Guide")  # hidden, but named in the Window menu and to VoiceOver
    win.setTitleVisibility_(NSWindowTitleHidden)
    win.setTitlebarAppearsTransparent_(True)
    win.setMovableByWindowBackground_(True)
    win.setContentView_(root)
    win.setReleasedWhenClosed_(False)
    win.setDelegate_(ctrl)
    ctrl.window = win
    _show(ctrl, 0)
    _refresh_perms(ctrl)
    return ctrl


def _page(ctrl, title, subtitle, body):
    """App icon, centred title and subtitle, then the step's rows."""
    page = _Page.alloc().initWithFrame_(NSMakeRect(0, 0, W, 0))
    icon = NSImageView.imageViewWithImage_(NSApplication.sharedApplication().applicationIconImage())
    icon.setFrame_(NSMakeRect((W - ICON) / 2, 36, ICON, ICON))  # clear of the traffic lights
    page.addSubview_(icon)
    y = 36 + ICON + 14
    for text, size, bold, color in ((title, 22, True, None),
                                    (subtitle, 13, False, NSColor.secondaryLabelColor())):
        label = _label(text, size, color, bold, width=W - 4 * PAD)
        label.setAlignment_(NSTextAlignmentCenter)
        label.setFrameOrigin_((2 * PAD, y))
        page.addSubview_(label)
        y += label.frame().size.height + 6
    page.setFrameSize_((W, body(ctrl, page, y + 18)))
    return page


def _show(ctrl, step):
    ctrl.step = step
    for i, (page, dot) in enumerate(zip(ctrl.pages, ctrl.dots)):
        page.setHidden_(i != step)
        dot.setTextColor_(NSColor.controlAccentColor() if i == step else NSColor.tertiaryLabelColor())
    ctrl.backbtn.setHidden_(step == 0)
    ctrl.nextbtn.setTitle_("Done" if step == len(STEPS) - 1 else "Continue")
    # fit the step, keeping the top edge still, as Settings does between panes
    win, frame = ctrl.window, ctrl.window.frame()
    height = ctrl.pages[step].frame().size.height + FOOTER
    win.setFrame_display_animate_(NSMakeRect(
        frame.origin.x, frame.origin.y + frame.size.height - height, W, height), True, win.isVisible())
