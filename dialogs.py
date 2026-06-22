"""Native AppKit dialogs: Telegram setup panel, About, Help.

ponytail: About reuses macOS's standard about panel (orderFrontStandardAboutPanel)
instead of a hand-built window. Free, native, correct.
"""
from AppKit import (
    NSAlert, NSView, NSTextField, NSSecureTextField, NSImage, NSImageView,
    NSFont, NSColor, NSApplication, NSImageScaleProportionallyUpOrDown,
    NSAlertFirstButtonReturn,
    NSLinkAttributeName, NSForegroundColorAttributeName, NSFontAttributeName,
)
from Foundation import NSMakeRect, NSAttributedString, NSURL

import telegram
from utils import asset_path

TOKEN_HELP = ("How to get a bot token", "https://core.telegram.org/bots/features#botfather")
CHAT_HELP = ("How to get your chat id", "https://docs.unplugg.app/chat-id")  # placeholder


def setup_telegram():
    """Show the multi-field setup panel. Returns a destination dict or None."""
    alert = NSAlert.alloc().init()
    alert.setMessageText_("Add a Telegram destination")
    alert.setInformativeText_("Alerts (with photos and video) are sent here.")
    icon = NSImage.alloc().initWithContentsOfFile_(asset_path("logo.png"))
    if icon:
        alert.setIcon_(icon)
    alert.addButtonWithTitle_("Save")
    alert.addButtonWithTitle_("Cancel")

    view = NSView.alloc().initWithFrame_(NSMakeRect(0, 0, 360, 250))
    name = _field(view, 210, "Destination name", "e.g. My phone")
    token = _field(view, 120, "Telegram bot token", "123456:ABC-DEF...", help=TOKEN_HELP, secure=True)
    chat = _field(view, 30, "Chat id", "e.g. 987654321", help=CHAT_HELP)
    alert.setAccessoryView_(view)
    alert.window().setInitialFirstResponder_(name)  # alert builds its panel lazily

    if alert.runModal() != NSAlertFirstButtonReturn:
        return None
    n, t, c = name.stringValue().strip(), token.stringValue().strip(), chat.stringValue().strip()
    if not (n and t and c):
        return None
    if not telegram.test_credentials(t, c):
        _error("Couldn't reach Telegram with those details. Check the token and chat id.")
        return None
    return {"name": n, "token": t, "chat_id": c}


def _field(parent, y, title, placeholder, help=None, secure=False):
    parent.addSubview_(_text(title, NSMakeRect(0, y + 22, 360, 18), 13, bold=True))
    cls = NSSecureTextField if secure else NSTextField
    tf = cls.alloc().initWithFrame_(NSMakeRect(0, y, 360, 22))
    tf.setPlaceholderString_(placeholder)
    parent.addSubview_(tf)
    if help:
        parent.addSubview_(_link(help[0], help[1], NSMakeRect(0, y - 18, 360, 16)))
    return tf


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
    a = NSAlert.alloc().init()
    a.setMessageText_("Setup failed")
    a.setInformativeText_(msg)
    a.runModal()


def show_about():
    img = NSImage.alloc().initWithContentsOfFile_(asset_path("about_logo.png"))
    options = {"ApplicationName": "unplugg",
               "Credits": NSAttributedString.alloc().initWithString_(
                   "Safeguard your Mac in public spaces.")}
    if img:
        options["ApplicationIcon"] = img
    NSApplication.sharedApplication().orderFrontStandardAboutPanelWithOptions_(options)


def show_help():
    alert = NSAlert.alloc().init()
    alert.setMessageText_("How unplugg works")
    alert.setInformativeText_(
        "1. Plug in and add a Telegram destination.\n"
        "2. Choose what counts as unsafe: power unplugged and/or screen moved.\n"
        "3. Click Activate before you step away.\n\n"
        "When the Mac is disturbed: it sirens, captures photo + video, sends a\n"
        "location link, and alerts Telegram — then optionally locks.")
    img = NSImage.alloc().initWithContentsOfFile_(asset_path("about_logo.png"))
    if img:
        alert.setIcon_(img)
    alert.addButtonWithTitle_("Done")
    alert.runModal()
