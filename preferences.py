"""Native Preferences window — the one home for every setting, organized into
tabs (Triggers / Actions / Devices / Permissions).

ponytail: tabs instead of one tall scrolling column — keeps every pane under
~350px so it fits a laptop screen, and it's the native System-Settings shape.
Manual top-down layout per page (Auto Layout is ~3x the code for static panes).
PAGE_H is the single layout knob if a control ever clips.

PyObjC note: _Prefs subclasses NSObject, so only its target/action + delegate
methods (all ending in `_`) live on the class; every layout helper is a module
function, so PyObjC never tries to bridge them into selectors.
"""
from AppKit import (
    NSApplication, NSWindow, NSButton, NSSlider, NSTextField, NSPopUpButton,
    NSView, NSFont, NSColor, NSBackingStoreBuffered, NSTabView, NSTabViewItem,
    NSWindowStyleMaskTitled, NSWindowStyleMaskClosable,
)
from Foundation import NSObject, NSMakeRect

import devices
import dialogs
import loginitem
import permissions
import sensors
from config import save_config

_controller = None  # retain while open

W = 380
PAD = 20
PAGE_H = 380  # layout height per tab page

PERMS = [
    ("camera", "Camera"),
    ("microphone", "Microphone"),
    ("location", "Location"),
    ("accessibility", "Accessibility"),
]


def open_preferences(app):
    global _controller
    if _controller is None:
        _controller = _build(app)
    _controller.window.center()
    _controller.window.makeKeyAndOrderFront_(None)
    NSApplication.sharedApplication().activateIgnoringOtherApps_(True)


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
        self.sens.setStringValue_(_sens_text(sender.doubleValue()))
        save_config(self.cfg)

    def cameraPick_(self, sender):
        self.cfg["camera_index"] = sender.indexOfSelectedItem()
        save_config(self.cfg)

    def micPick_(self, sender):
        self.cfg["mic_index"] = sender.indexOfSelectedItem()
        save_config(self.cfg)

    def speakerPick_(self, sender):
        idx = sender.indexOfSelectedItem()
        self.cfg["speaker_name"] = "" if idx == 0 else sender.titleOfSelectedItem()
        save_config(self.cfg)

    def addDest_(self, _):
        dest = dialogs.setup_telegram()
        if dest:
            self.cfg["telegram_destinations"].append(dest)
            save_config(self.cfg)
            _refresh_popup(self)

    def removeDest_(self, _):
        name = self.popup.titleOfSelectedItem()
        self.cfg["telegram_destinations"] = [
            d for d in self.cfg["telegram_destinations"] if d["name"] != name]
        save_config(self.cfg)
        _refresh_popup(self)

    def permGrant_(self, sender):
        key = self.permbtns[sender]
        # A previously-denied permission can't be re-prompted — send them to Settings.
        if permissions.status(key) == "denied":
            permissions.open_settings(key)
        else:
            permissions.request(key)
        self.permlabels[key].setStringValue_(_perm_text(key))

    def windowWillClose_(self, _):
        global _controller
        _controller = None


# ---- construction (module functions, not bridged to ObjC) -----------------
def _build(app):
    ctrl = _Prefs.alloc().init()
    ctrl.app = app
    ctrl.cfg = app.config
    ctrl.checks = {}

    height = 460
    style = NSWindowStyleMaskTitled | NSWindowStyleMaskClosable
    win = NSWindow.alloc().initWithContentRect_styleMask_backing_defer_(
        NSMakeRect(0, 0, W, height), style, NSBackingStoreBuffered, False)
    win.setTitle_("unplugg Preferences")
    win.setReleasedWhenClosed_(False)
    win.setDelegate_(ctrl)
    ctrl.window = win

    tabs = NSTabView.alloc().initWithFrame_(NSMakeRect(10, 10, W - 20, height - 20))
    for title, builder in [("Triggers", _triggers_page), ("Actions", _actions_page),
                           ("Devices", _devices_page), ("Permissions", _permissions_page)]:
        page = NSView.alloc().initWithFrame_(NSMakeRect(0, 0, W - 20, PAGE_H))
        builder(ctrl, page, PAGE_H - PAD)
        item = NSTabViewItem.alloc().initWithIdentifier_(title)
        item.setLabel_(title)
        item.setView_(page)
        tabs.addTabViewItem_(item)
    win.contentView().addSubview_(tabs)
    return ctrl


def _triggers_page(ctrl, view, y):
    y = _heading(view, "Alarm on", y)
    y = _checkbox(ctrl, view, "monitor_power", "Power unplugged", y)
    y = _checkbox(ctrl, view, "monitor_lid", "Screen closed / moved", y)
    if sensors.has_angle_sensor():
        y = _slider(ctrl, view, y)
    y = _heading(view, "When active", y - 8)
    y = _checkbox(ctrl, view, "show_overlay", "Show overlay warning", y)
    y = _checkbox(ctrl, view, "lock_device", "Lock device", y)
    y = _heading(view, "Startup", y - 8)
    _checkbox(ctrl, view, "launch_at_login", "Launch at login", y)


def _actions_page(ctrl, view, y):
    y = _heading(view, "Alarm actions", y)
    y = _checkbox(ctrl, view, "play_sound", "Play siren sound (full volume)", y)
    y = _checkbox(ctrl, view, "send_location", "Send location", y)
    y = _checkbox(ctrl, view, "capture_media", "Send photo & video to Telegram", y)
    y = _checkbox(ctrl, view, "record_audio", "Record audio with video", y)
    _destinations(ctrl, view, y)


def _devices_page(ctrl, view, y):
    y = _heading(view, "Capture devices", y)
    ctrl.cam_popup, y = _popup_row(
        ctrl, view, "Camera (photos & video)",
        devices.cameras() or ["Default camera"], ctrl.cfg["camera_index"], "cameraPick:", y)
    ctrl.mic_popup, y = _popup_row(
        ctrl, view, "Microphone (video audio)",
        devices.microphones() or ["Default microphone"], ctrl.cfg["mic_index"], "micPick:", y)
    spk = ["System default"] + devices.speakers()
    sel = spk.index(ctrl.cfg["speaker_name"]) if ctrl.cfg["speaker_name"] in spk else 0
    ctrl.spk_popup, y = _popup_row(ctrl, view, "Siren output (speaker)", spk, sel, "speakerPick:", y)


def _permissions_page(ctrl, view, y):
    y = _heading(view, "Permissions", y)
    ctrl.permbtns = {}
    ctrl.permlabels = {}
    for key, label in PERMS:
        _perm_row(ctrl, view, key, label, y)
        y -= 32


# ---- row builders ---------------------------------------------------------
def _heading(view, text, y):
    lbl = _label(text, NSMakeRect(PAD, y - 18, W - 2 * PAD, 18), 13, bold=True)
    lbl.setTextColor_(NSColor.secondaryLabelColor())
    view.addSubview_(lbl)
    return y - 28


def _checkbox(ctrl, view, key, label, y):
    b = NSButton.checkboxWithTitle_target_action_(label, ctrl, "toggle:")
    b.setFrame_(NSMakeRect(PAD + 4, y - 22, W - 2 * PAD, 22))
    b.setState_(1 if ctrl.cfg[key] else 0)
    ctrl.checks[b] = key
    view.addSubview_(b)
    return y - 28


def _slider(ctrl, view, y):
    ctrl.sens = _label("", NSMakeRect(PAD + 24, y - 18, W, 16), 11)
    ctrl.sens.setTextColor_(NSColor.secondaryLabelColor())
    s = NSSlider.alloc().initWithFrame_(NSMakeRect(PAD + 24, y - 44, W - 2 * PAD - 24, 22))
    s.setMinValue_(1); s.setMaxValue_(45)
    s.setDoubleValue_(ctrl.cfg["angle_threshold"])
    s.setTarget_(ctrl); s.setAction_("sensitivity:")
    view.addSubview_(ctrl.sens); view.addSubview_(s)
    ctrl.sens.setStringValue_(_sens_text(ctrl.cfg["angle_threshold"]))
    return y - 70


def _popup_row(ctrl, view, caption, items, sel, action, y):
    cap = _label(caption, NSMakeRect(PAD, y - 16, W - 2 * PAD, 16), 11)
    cap.setTextColor_(NSColor.secondaryLabelColor())
    view.addSubview_(cap)
    pop = NSPopUpButton.alloc().initWithFrame_(NSMakeRect(PAD, y - 44, W - 2 * PAD, 25))
    pop.addItemsWithTitles_(items)
    if 0 <= sel < len(items):
        pop.selectItemAtIndex_(sel)
    pop.setTarget_(ctrl); pop.setAction_(action)
    view.addSubview_(pop)
    return pop, y - 56


def _destinations(ctrl, view, y):
    cap = _label("Telegram destinations", NSMakeRect(PAD + 4, y - 18, W, 16), 11)
    cap.setTextColor_(NSColor.secondaryLabelColor())
    view.addSubview_(cap)
    ctrl.popup = NSPopUpButton.alloc().initWithFrame_(
        NSMakeRect(PAD + 4, y - 46, W - 2 * PAD - 150, 25))
    add = NSButton.alloc().initWithFrame_(NSMakeRect(W - PAD - 140, y - 47, 70, 26))
    add.setTitle_("Add…"); add.setBezelStyle_(1)
    add.setTarget_(ctrl); add.setAction_("addDest:")
    rm = NSButton.alloc().initWithFrame_(NSMakeRect(W - PAD - 68, y - 47, 68, 26))
    rm.setTitle_("Remove"); rm.setBezelStyle_(1)
    rm.setTarget_(ctrl); rm.setAction_("removeDest:")
    view.addSubview_(ctrl.popup); view.addSubview_(add); view.addSubview_(rm)
    _refresh_popup(ctrl)


def _perm_row(ctrl, view, key, label, y):
    view.addSubview_(_label(label, NSMakeRect(PAD, y - 20, 120, 18), 12))
    st = _label(_perm_text(key), NSMakeRect(PAD + 120, y - 20, 120, 18), 11)
    st.setTextColor_(NSColor.secondaryLabelColor())
    view.addSubview_(st)
    ctrl.permlabels[key] = st
    btn = NSButton.alloc().initWithFrame_(NSMakeRect(W - PAD - 100, y - 22, 80, 24))
    btn.setTitle_("Grant"); btn.setBezelStyle_(1)
    btn.setTarget_(ctrl); btn.setAction_("permGrant:")
    view.addSubview_(btn)
    ctrl.permbtns[btn] = key


def _refresh_popup(ctrl):
    ctrl.popup.removeAllItems()
    names = [d["name"] for d in ctrl.cfg["telegram_destinations"]]
    ctrl.popup.addItemsWithTitles_(names or ["No destinations yet"])
    ctrl.popup.setEnabled_(bool(names))


def _perm_text(key):
    return {"granted": "✓ Granted", "denied": "✗ Denied",
            "unset": "Not granted", "n/a": "Not available"}[permissions.status(key)]


def _sens_text(deg):
    return f"Screen sensitivity: {int(round(deg))}° of movement"


def _label(s, rect, size, bold=False):
    f = NSTextField.alloc().initWithFrame_(rect)
    f.setStringValue_(s)
    f.setBezeled_(False); f.setEditable_(False); f.setSelectable_(False)
    f.setDrawsBackground_(False)
    f.setFont_(NSFont.boldSystemFontOfSize_(size) if bold else NSFont.systemFontOfSize_(size))
    return f
