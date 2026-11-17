"""Settings window, shaped like System Settings: toolbar panes (General / Alerts /
Devices / Permissions), each a stack of grouped rows — label on the left, mini
switch or popup on the right.

ponytail: stock AppKit only (NSTabViewController toolbar, NSBox, NSSwitch), so
each macOS version draws it in its own style, Catalina through Liquid Glass.
Manual top-down layout in a flipped view (Auto Layout is ~3x the code here).

PyObjC note: _Prefs subclasses NSObject, so only its target/action + delegate
methods (all ending in `_`) live on the class; every layout helper is a module
function, so PyObjC never tries to bridge them into selectors.
"""
from pathlib import Path

from AppKit import (
    NSApplication, NSBox, NSBoxSeparator, NSButton, NSColor, NSControlSizeMini, NSFont, NSImage,
    NSLineBreakByTruncatingMiddle, NSNoTitle, NSOpenPanel, NSPopUpButton, NSSegmentedControl,
    NSSegmentSwitchTrackingMomentary, NSSlider, NSSwitch, NSTabViewController,
    NSTabViewControllerTabStyleToolbar, NSTabViewItem, NSTextAlignmentRight, NSTextField, NSView,
    NSViewController, NSWindow, NSWindowStyleMaskClosable, NSWindowStyleMaskTitled,
    NSWindowToolbarStylePreference, NSWorkspace,
)
from Foundation import NSMakeRect, NSObject, NSURL

import devices
import dialogs
import loginitem
import permissions
import sensors
import telegram
from config import save_config
from utils import show_in_dock

_controller = None  # retain while open

W = 480      # pane width
PAD = 20     # pane margin
ROW = 36     # grouped-form row height
INSET = 12   # row padding inside a group

PERMS = [
    ("camera", "Camera"),
    ("microphone", "Microphone"),
    ("location", "Location"),
]

STATUS = {  # permission state → (label, colour)
    "granted": ("● Granted", NSColor.systemGreenColor),
    "denied": ("● Denied", NSColor.systemRedColor),
    "unset": ("● Not set", NSColor.secondaryLabelColor),
    "n/a": ("Unavailable", NSColor.tertiaryLabelColor),
    "app-only": ("● Needs the .app", NSColor.systemOrangeColor),
}


def open_preferences(app):
    global _controller
    if _controller is None:
        _controller = _build(app)
        _controller.window.center()
    show_in_dock(True)  # a window is open: Dock icon and Cmd-Tab, like any app
    _controller.window.makeKeyAndOrderFront_(None)
    NSApplication.sharedApplication().activateIgnoringOtherApps_(True)


class _Page(NSView):
    def isFlipped(self):
        return True  # lay out top-down


class _Prefs(NSObject):
    """Holds window + state; only ObjC-callable methods (trailing `_`)."""

    def toggle_(self, sender):
        key = self.checks[sender]
        self.cfg[key] = bool(sender.state())
        save_config(self.cfg)
        if key == "launch_at_login":
            loginitem.enable() if self.cfg[key] else loginitem.disable()

    def sensitivity_(self, sender):
        self.cfg["angle_threshold"] = int(round(sender.doubleValue()))
        self.sens.setStringValue_(f"{self.cfg['angle_threshold']}°")
        save_config(self.cfg)

    def cameraPick_(self, sender):
        self.cfg["camera"] = sender.titleOfSelectedItem()
        save_config(self.cfg)

    def micPick_(self, sender):
        self.cfg["mic"] = sender.titleOfSelectedItem()
        save_config(self.cfg)

    def speakerPick_(self, sender):
        idx = sender.indexOfSelectedItem()
        self.cfg["speaker_name"] = "" if idx == 0 else sender.titleOfSelectedItem()
        save_config(self.cfg)

    def chooseFolder_(self, _):
        panel = NSOpenPanel.openPanel()
        panel.setCanChooseDirectories_(True)
        panel.setCanChooseFiles_(False)
        panel.setAllowsMultipleSelection_(False)
        panel.setCanCreateDirectories_(True)
        if panel.runModal() == 1:  # NSModalResponseOK
            path = panel.URLs()[0].path()
            self.cfg["media_dir"] = path
            save_config(self.cfg)
            self.dirlabel.setStringValue_(_short(path))

    def openFolder_(self, _):
        path = Path(self.cfg["media_dir"]).expanduser()
        path.mkdir(parents=True, exist_ok=True)  # capture makes it lazily
        NSWorkspace.sharedWorkspace().openURL_(NSURL.fileURLWithPath_(str(path)))

    def destTest_(self, _):
        dest = self.cfg["telegram_destinations"][self.popup.indexOfSelectedItem()]
        err = telegram.send_test(dest["token"], dest["chat_id"])
        dialogs.alert("Test failed" if err else "Test sent",
                      f"Telegram says: {err}" if err else f"Check Telegram for {dest['name']}.")

    def destEdit_(self, sender):
        if sender.selectedSegment() == 0:
            dest = dialogs.setup_telegram()
            if dest:
                self.cfg["telegram_destinations"].append(dest)
        else:
            name = self.popup.titleOfSelectedItem()
            self.cfg["telegram_destinations"] = [
                d for d in self.cfg["telegram_destinations"] if d["name"] != name]
        save_config(self.cfg)
        _refresh_destinations(self)

    def permGrant_(self, sender):
        key = self.permbtns[sender]
        # A previously-denied permission can't be re-prompted — send them to Settings.
        if permissions.status(key) == "denied":
            permissions.open_settings(key)
        else:
            permissions.request(key)
        _refresh_perms(self)

    def windowDidBecomeKey_(self, _):
        _refresh_perms(self)  # pick up changes made in System Settings

    def windowWillClose_(self, _):
        global _controller
        self.cfg["settings_pane"] = self.tabs.selectedTabViewItemIndex()
        save_config(self.cfg)
        show_in_dock(self.cfg["show_in_dock"])
        _controller = None


# ---- construction (module functions, not bridged to ObjC) -----------------
def _build(app):
    ctrl = _Prefs.alloc().init()
    ctrl.app = app
    ctrl.cfg = app.config
    ctrl.checks = {}
    ctrl.tabs = NSTabViewController.alloc().init()
    ctrl.tabs.setTabStyle_(NSTabViewControllerTabStyleToolbar)

    # (title, SF Symbol on macOS 11+, named-image fallback on Catalina, builder)
    for title, symbol, fallback, builder in [
        ("General", "gearshape", "NSPreferencesGeneral", _general_page),
        ("Alerts", "bell.badge", "NSCaution", _alerts_page),
        ("Devices", "video", "NSComputer", _devices_page),
        ("Permissions", "hand.raised", "NSUserAccounts", _permissions_page),
    ]:
        page = _Page.alloc().initWithFrame_(NSMakeRect(0, 0, W, 0))
        page.setFrameSize_((W, builder(ctrl, page, PAD)))
        vc = NSViewController.alloc().init()
        vc.setView_(page)
        vc.setTitle_(title)
        vc.setPreferredContentSize_(page.frame().size)
        item = NSTabViewItem.tabViewItemWithViewController_(vc)
        item.setImage_(_icon(symbol, fallback))
        ctrl.tabs.addTabViewItem_(item)
    ctrl.tabs.setSelectedTabViewItemIndex_(min(ctrl.cfg["settings_pane"], 3))

    win = NSWindow.windowWithContentViewController_(ctrl.tabs)  # title follows the pane
    win.setStyleMask_(NSWindowStyleMaskTitled | NSWindowStyleMaskClosable)
    if win.respondsToSelector_("setToolbarStyle:"):
        win.setToolbarStyle_(NSWindowToolbarStylePreference)  # macOS 11+
    win.setReleasedWhenClosed_(False)
    win.setDelegate_(ctrl)
    ctrl.window = win
    _refresh_perms(ctrl)
    return ctrl


def _general_page(ctrl, view, y):
    rows = [("Power unplugged", _switch(ctrl, "monitor_power")),
            ("Screen closed or moved", _switch(ctrl, "monitor_lid"))]
    if sensors.has_angle_sensor():
        rows.append(("Screen sensitivity", _sensitivity(ctrl)))
    y = _group(view, y, "Alarm when", rows)
    y = _group(view, y, "While armed", [
        ("Show overlay", _switch(ctrl, "show_overlay")),
        ("Lock Mac when triggered", _switch(ctrl, "lock_device"))])
    return _group(view, y, None, [("Show in Dock", _switch(ctrl, "show_in_dock")),
                                  ("Launch at login", _switch(ctrl, "launch_at_login"))])


def _alerts_page(ctrl, view, y):
    y = _group(view, y, "When triggered", [
        ("Play siren at full volume", _switch(ctrl, "play_sound")),
        ("Send photo & video", _switch(ctrl, "capture_media")),
        ("Record audio with video", _switch(ctrl, "record_audio")),
        ("Send location", _switch(ctrl, "send_location"))])
    ctrl.popup = _popup(ctrl, [], 0, None, 150)
    ctrl.destbtns = NSSegmentedControl.segmentedControlWithImages_trackingMode_target_action_(
        [NSImage.imageNamed_("NSAddTemplate"), NSImage.imageNamed_("NSRemoveTemplate")],
        NSSegmentSwitchTrackingMomentary, ctrl, "destEdit:")
    ctrl.testbtn = _button("Send Test", ctrl, "destTest:")
    _refresh_destinations(ctrl)
    y = _group(view, y, "Telegram", [
        ("Destinations", _hstack(ctrl.popup, ctrl.destbtns, ctrl.testbtn))])
    ctrl.dirlabel = _label(_short(ctrl.cfg["media_dir"]), 12, NSColor.secondaryLabelColor())
    ctrl.dirlabel.setFrameSize_((150, ctrl.dirlabel.frame().size.height))
    ctrl.dirlabel.setLineBreakMode_(NSLineBreakByTruncatingMiddle)
    choose = _button("Choose…", ctrl, "chooseFolder:")
    reveal = _button("Open", ctrl, "openFolder:")
    return _group(view, y, "Saved media", [("Folder", _hstack(ctrl.dirlabel, choose, reveal))])


def _devices_page(ctrl, view, y):
    cams = devices.cameras() or ["Default camera"]
    mics = devices.microphones() or ["Default microphone"]
    cam = _popup(ctrl, cams, _index(cams, devices.camera(ctrl.cfg["camera"])), "cameraPick:")
    mic = _popup(ctrl, mics, _index(mics, devices.microphone(ctrl.cfg["mic"])), "micPick:")
    y = _group(view, y, "Capture", [("Camera", cam), ("Microphone", mic)])
    spk = ["System default"] + devices.speakers()
    sel = _index(spk, ctrl.cfg["speaker_name"])
    return _group(view, y, "Siren", [("Speaker", _popup(ctrl, spk, sel, "speakerPick:"))])


def _index(items, name):
    return items.index(name) if name in items else 0


def _permissions_page(ctrl, view, y):
    ctrl.permbtns, ctrl.permrows = {}, {}
    rows = []
    for key, label in PERMS:
        btn = _button("Open Settings…", ctrl, "permGrant:")
        status = _label("", 12)
        status.setFrameSize_((90, status.frame().size.height))
        status.setAlignment_(NSTextAlignmentRight)
        ctrl.permbtns[btn] = key
        ctrl.permrows[key] = (status, btn)
        rows.append((label, _hstack(btn, status)))
    return _group(view, y, "unplugg needs", rows)


# ---- row builders ---------------------------------------------------------
def _group(view, y, title, rows):
    """Optional heading, then a rounded box of label/control rows. Returns the next y."""
    if title:
        heading = _label(title, 13, bold=True)
        heading.setFrameOrigin_((PAD + 2, y))
        view.addSubview_(heading)
        y += 22
    width, height = W - 2 * PAD, ROW * len(rows)
    box = NSBox.alloc().initWithFrame_(NSMakeRect(PAD, y, width, height))
    box.setTitlePosition_(NSNoTitle)
    box.setContentViewMargins_((0, 0))
    inner = box.contentView()
    for i, (text, control) in enumerate(rows):
        top = height - ROW * i  # the box's own content isn't flipped
        if i:
            line = NSBox.alloc().initWithFrame_(NSMakeRect(INSET, top, width - 2 * INSET, 1))
            line.setBoxType_(NSBoxSeparator)
            inner.addSubview_(line)
        size = control.frame().size
        control.setFrameOrigin_((width - INSET - size.width, top - (ROW + size.height) / 2))
        label = _label(text, 13)
        label.setFrameOrigin_((INSET, top - (ROW + label.frame().size.height) / 2))
        inner.addSubview_(label)
        inner.addSubview_(control)
    view.addSubview_(box)
    return y + height + 20


def _switch(ctrl, key):
    s = NSSwitch.alloc().init()
    s.setControlSize_(NSControlSizeMini)  # per HIG for grouped-form rows
    s.sizeToFit()
    s.setState_(1 if ctrl.cfg[key] else 0)
    s.setTarget_(ctrl)
    s.setAction_("toggle:")
    ctrl.checks[s] = key
    return s


def _sensitivity(ctrl):
    s = NSSlider.sliderWithValue_minValue_maxValue_target_action_(
        ctrl.cfg["angle_threshold"], 1, 45, ctrl, "sensitivity:")
    s.setFrameSize_((150, s.frame().size.height))
    ctrl.sens = _label(f"{ctrl.cfg['angle_threshold']}°", 12, NSColor.secondaryLabelColor())
    ctrl.sens.setFrameSize_((32, ctrl.sens.frame().size.height))
    return _hstack(s, ctrl.sens)


def _popup(ctrl, items, sel, action, width=200):
    pop = NSPopUpButton.alloc().initWithFrame_pullsDown_(NSMakeRect(0, 0, width, 25), False)
    pop.addItemsWithTitles_(items)
    if 0 <= sel < len(items):
        pop.selectItemAtIndex_(sel)
    pop.setTarget_(ctrl)
    pop.setAction_(action)
    return pop


def _button(title, target, action):
    return NSButton.buttonWithTitle_target_action_(title, target, action)


def _hstack(*views):
    """Pack controls left-to-right, vertically centred, into one view."""
    height = max(v.frame().size.height for v in views)
    row = NSView.alloc().initWithFrame_(NSMakeRect(0, 0, 0, height))
    x = 0
    for v in views:
        size = v.frame().size
        v.setFrameOrigin_((x, (height - size.height) / 2))
        row.addSubview_(v)
        x += size.width + 8
    row.setFrameSize_((x - 8, height))
    return row


def _refresh_destinations(ctrl):
    ctrl.popup.removeAllItems()
    names = [d["name"] for d in ctrl.cfg["telegram_destinations"]]
    ctrl.popup.addItemsWithTitles_(names or ["None yet"])
    ctrl.popup.setEnabled_(bool(names))
    ctrl.destbtns.setEnabled_forSegment_(bool(names), 1)
    ctrl.testbtn.setEnabled_(bool(names))


def _refresh_perms(ctrl):
    for key, (status, btn) in ctrl.permrows.items():
        state = permissions.status(key)
        text, color = STATUS[state]
        status.setStringValue_(text)
        status.setTextColor_(color())
        btn.setHidden_(state in ("granted", "app-only"))
        btn.setTitle_("Grant…" if state == "unset" else "Open Settings…")


def _short(path):
    return path.replace(str(Path.home()), "~")


def _icon(symbol, fallback):
    if NSImage.respondsToSelector_("imageWithSystemSymbolName:accessibilityDescription:"):
        return NSImage.imageWithSystemSymbolName_accessibilityDescription_(symbol, None)
    return NSImage.imageNamed_(fallback)


def _label(text, size, color=None, bold=False):
    f = NSTextField.labelWithString_(text)
    f.setFont_(NSFont.boldSystemFontOfSize_(size) if bold else NSFont.systemFontOfSize_(size))
    if color:
        f.setTextColor_(color)
    f.sizeToFit()
    return f
