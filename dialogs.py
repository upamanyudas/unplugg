"""Native AppKit dialogs: alerts, Telegram setup panel, About, Help.

ponytail: About reuses macOS's standard about panel (orderFrontStandardAboutPanel)
instead of a hand-built window. Free, native, correct.

Every dialog passes the app icon explicitly. AppKit's default is the bundle's
icon, which from source is Python's rocket, not the one unplugg.py sets.
"""
import threading

from AppKit import (
    NSAlert, NSView, NSTextField, NSSecureTextField, NSFont, NSColor, NSApplication, NSMenu,
    NSMenuItem, NSButton, NSBezelStyleHelpButton,
    NSAlertFirstButtonReturn, NSAlertStyleCritical,
    NSLinkAttributeName, NSForegroundColorAttributeName, NSFontAttributeName,
)
from Foundation import NSMakeRect, NSAttributedString, NSObject, NSURL
from PyObjCTools import AppHelper

import telegram

TOKEN_HELP = ("How to get a bot token", "https://core.telegram.org/bots/features#botfather")
# The Bot API has no priority flag (only disable_notification, which goes quieter),
# so a loud alert is a setting on the phone.
LOUD_HELP = (
    "Telegram bots can't force a loud alert. Set this up once on your phone.\n\n"
    "iPhone\n"
    "• Settings › Focus › each Focus › Apps › add Telegram.\n"
    "• Telegram › Settings › Notifications and Sounds: give your bot's chat a loud sound.\n"
    "• Silent mode on? Only vibration gets through: "
    "Settings › Sounds & Haptics › Haptics › Always Play.\n\n"
    "Android (menu names vary by phone)\n"
    "• When the first alert arrives, long-press it › Priority.\n"
    "• Settings › Do Not Disturb › People › Conversations › Priority conversations.")
EDIT_ITEMS = [("Undo", "undo:", "z"), ("Redo", "redo:", "Z"), ("Cut", "cut:", "x"),
              ("Copy", "copy:", "c"), ("Paste", "paste:", "v"), ("Select All", "selectAll:", "a")]


def install_edit_menu():
    """Cmd+X/C/V/A only reach text fields through the main menu, and a menu bar
    app has none, so paste into the Telegram fields did nothing."""
    edit = NSMenu.alloc().initWithTitle_("Edit")
    for title, action, key in EDIT_ITEMS:
        edit.addItemWithTitle_action_keyEquivalent_(title, action, key)
    item = NSMenuItem.alloc().init()
    item.setSubmenu_(edit)
    bar = NSMenu.alloc().init()
    bar.addItem_(NSMenuItem.alloc().init())  # slot 0 is always the app menu
    bar.addItem_(item)
    NSApplication.sharedApplication().setMainMenu_(bar)


def alert(title, text, *buttons):
    """Show an alert; returns the index of the button clicked (0 = first)."""
    return _alert(title, text, *buttons).runModal() - NSAlertFirstButtonReturn


def _alert(title, text, *buttons):
    a = NSAlert.alloc().init()
    a.setIcon_(_icon())
    a.setMessageText_(title)
    a.setInformativeText_(text)
    for b in buttons:
        a.addButtonWithTitle_(b)
    # a menu bar app is never frontmost by itself; without this the alert opens behind
    NSApplication.sharedApplication().activateIgnoringOtherApps_(True)
    return a


def _icon():
    return NSApplication.sharedApplication().applicationIconImage()


class _Setup(NSObject):
    """Target for the setup panel's buttons; setup_telegram attaches the fields."""

    def detect_(self, _):
        self.detect.setEnabled_(False)
        self.hint.setStringValue_("Waiting… send your bot any message now.")
        token = _value(self.token)
        # long poll off the main thread so the panel stays responsive
        threading.Thread(target=lambda: AppHelper.callAfter(
            self.detected_, telegram.find_chat_id(token)), daemon=True).start()

    def detected_(self, result):
        chat, err = result
        if chat:
            self.chat.setStringValue_(chat)
        self.hint.setStringValue_("Found your chat." if chat else err)
        self.controlTextDidChange_(None)

    def test_(self, _):
        err = telegram.send_test(_value(self.token), _value(self.chat))
        self.status.setStringValue_(err or "Sent. Check Telegram.")

    def loud_(self, _):
        alert("Make alerts loud on your phone", LOUD_HELP, "Done")

    def controlTextDidChange_(self, _):
        has_token = bool(_value(self.token))
        self.detect.setEnabled_(has_token)
        self.test.setEnabled_(has_token and bool(_value(self.chat)))


def setup_telegram():
    """Show the multi-field setup panel. Returns a destination dict or None."""
    panel = _alert("Add a Telegram destination", "Alerts (with photos and video) are sent here.",
                   "Save", "Cancel")

    ui = _Setup.alloc().init()
    view = NSView.alloc().initWithFrame_(NSMakeRect(0, 0, 360, 270))
    name = _field(view, 230, "Destination name", "e.g. My phone")
    ui.token = _field(view, 160, "Telegram bot token", "123456:ABC-DEF...", help=TOKEN_HELP,
                      secure=True)
    ui.chat = _field(view, 90, "Chat id", "e.g. 987654321", width=250)
    ui.detect = _button(view, "Auto-detect", ui, "detect:", (258, 88))
    ui.hint = _text("Click Auto-detect, then send your bot any message.",
                    NSMakeRect(0, 70, 360, 16), 11, color=NSColor.secondaryLabelColor())
    ui.test = _button(view, "Send Test Message", ui, "test:", (0, 34))
    x = ui.test.frame().size.width + 8
    ui.status = _text("", NSMakeRect(x, 39, 360 - x, 16), 11, color=NSColor.secondaryLabelColor())
    loud = _button(view, "", ui, "loud:", (0, 0))
    loud.setBezelStyle_(NSBezelStyleHelpButton)
    loud.sizeToFit()
    x = loud.frame().size.width + 6
    loud_label = _text("Make alerts ring on silent or Do Not Disturb",
                       NSMakeRect(x, 3, 360 - x, 16), 11, color=NSColor.secondaryLabelColor())
    for v in (ui.hint, ui.status, loud_label):
        view.addSubview_(v)
    for f in (ui.token, ui.chat):
        f.setDelegate_(ui)  # enable the buttons as the fields fill in
    ui.controlTextDidChange_(None)
    panel.setAccessoryView_(view)
    panel.window().setInitialFirstResponder_(name)  # alert builds its panel lazily

    if panel.runModal() != NSAlertFirstButtonReturn:
        return None
    n, t, c = _value(name), _value(ui.token), _value(ui.chat)
    if not (n and t and c):
        return None
    err = telegram.check(t, c)
    if err:
        _error(f"Telegram says: {err}")
        return None
    return {"name": n, "token": t, "chat_id": c}


def _value(field):
    return field.stringValue().strip()


def _field(parent, y, title, placeholder, help=None, secure=False, width=360):
    parent.addSubview_(_text(title, NSMakeRect(0, y + 22, 360, 18), 13, bold=True))
    cls = NSSecureTextField if secure else NSTextField
    tf = cls.alloc().initWithFrame_(NSMakeRect(0, y, width, 22))
    tf.setPlaceholderString_(placeholder)
    parent.addSubview_(tf)
    if help:
        parent.addSubview_(_link(help[0], help[1], NSMakeRect(0, y - 18, 360, 16)))
    return tf


def _button(parent, title, target, action, origin):
    b = NSButton.buttonWithTitle_target_action_(title, target, action)
    b.setFrameOrigin_(origin)
    parent.addSubview_(b)
    return b


def _text(s, rect, size, bold=False, color=None):
    f = NSTextField.alloc().initWithFrame_(rect)
    f.setStringValue_(s)
    f.setBezeled_(False); f.setEditable_(False); f.setSelectable_(False)
    f.setDrawsBackground_(False)
    f.setFont_(NSFont.boldSystemFontOfSize_(size) if bold else NSFont.systemFontOfSize_(size))
    if color:
        f.setTextColor_(color)
    return f


def _link(text, url, rect):
    f = NSTextField.alloc().initWithFrame_(rect)
    f.setBezeled_(False); f.setEditable_(False); f.setDrawsBackground_(False)
    f.setSelectable_(True); f.setAllowsEditingTextAttributes_(True)
    attrs = {
        NSLinkAttributeName: NSURL.URLWithString_(url),
        NSForegroundColorAttributeName: NSColor.linkColor(),
        NSFontAttributeName: NSFont.systemFontOfSize_(11),
    }
    f.setAttributedStringValue_(NSAttributedString.alloc().initWithString_attributes_(text, attrs))
    return f


def _error(msg):
    a = _alert("Setup failed", msg)
    a.setAlertStyle_(NSAlertStyleCritical)
    a.runModal()


def show_about():
    options = {"ApplicationName": "unplugg",
               "ApplicationIcon": _icon(),
               "Credits": NSAttributedString.alloc().initWithString_(
                   "Your Mac, guarded while you step away.")}
    NSApplication.sharedApplication().activateIgnoringOtherApps_(True)
    NSApplication.sharedApplication().orderFrontStandardAboutPanelWithOptions_(options)


def show_help():
    alert("How unplugg works",
          "1. Plug in and add a Telegram destination.\n"
          "2. Choose what counts as unsafe: power unplugged and/or screen moved.\n"
          "3. Click Activate before you step away.\n\n"
          "When the Mac is disturbed: it sirens, captures photo + video, sends a\n"
          "location link, and alerts Telegram — then optionally locks.",
          "Done")
