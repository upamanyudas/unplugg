use std::cell::RefCell;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, ProtocolObject};
use objc2::{define_class, msg_send, sel, AllocAnyThread, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAlert, NSAlertStyle, NSApplication, NSBezelStyle, NSButton, NSColor, NSControlTextEditingDelegate,
    NSFont, NSFontAttributeName, NSForegroundColorAttributeName, NSLinkAttributeName,
    NSSecureTextField, NSTextField, NSTextFieldDelegate, NSView,
};
use objc2_foundation::{
    NSAttributedString, NSDictionary, NSNotification, NSObjectProtocol, NSOperationQueue, NSPoint,
    NSRect, NSSize, NSString, NSURL,
};
use serde_json::{json, Value};

use crate::telegram::{self, LOUD_HELP, TOKEN_HELP};

fn rect(x: f64, y: f64, width: f64, height: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(width, height))
}

fn ns(text: &str) -> Retained<NSString> {
    NSString::from_str(text)
}

fn mtm() -> MainThreadMarker {
    MainThreadMarker::new().unwrap()
}

/// An alert with the app's own icon, brought to the front. No buttons means OK.
fn build(title: &str, text: &str, buttons: &[&str]) -> Retained<NSAlert> {
    let alert = NSAlert::new(mtm());
    let application = NSApplication::sharedApplication(mtm());
    unsafe { alert.setIcon(application.applicationIconImage().as_deref()) };
    alert.setMessageText(&ns(title));
    alert.setInformativeText(&ns(text));
    for button in buttons {
        alert.addButtonWithTitle(&ns(button));
    }
    // a menu bar app is never frontmost by itself; without this the alert opens behind
    application.activateIgnoringOtherApps(true);
    alert
}

/// Index of the button clicked, 0 for the first.
pub fn alert(title: &str, text: &str, buttons: &[&str]) -> isize {
    build(title, text, buttons).runModal() - 1000
}

pub fn alert_with(title: &str, text: &str, buttons: &[&str], accessory: &NSView) -> isize {
    let alert = build(title, text, buttons);
    alert.setAccessoryView(Some(accessory));
    alert.runModal() - 1000
}

struct SetupUi {
    token: Retained<NSTextField>,
    chat: Retained<NSTextField>,
    detect: Retained<NSButton>,
    hint: Retained<NSTextField>,
    test: Retained<NSButton>,
    status: Retained<NSTextField>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = RefCell<Option<SetupUi>>]
    struct Setup;

    unsafe impl NSObjectProtocol for Setup {}

    unsafe impl NSControlTextEditingDelegate for Setup {
        #[unsafe(method(controlTextDidChange:))]
        fn text_changed(&self, _: &NSNotification) {
            self.refresh();
        }
    }

    unsafe impl NSTextFieldDelegate for Setup {}

    impl Setup {
        #[unsafe(method(detect:))]
        fn detect(&self, _: &AnyObject) {
            let ui = self.ivars().borrow();
            let ui = ui.as_ref().unwrap();
            ui.detect.setEnabled(false);
            ui.hint.setStringValue(&ns("Waiting… send your bot any message now."));
            let token = value(&ui.token);
            // long poll off the main thread so the panel stays responsive
            std::thread::spawn(move || {
                let found = telegram::find_chat_id(&token, 30);
                let done = RcBlock::new(move || {
                    SETUP.with(|setup| {
                        if let Some(setup) = setup.borrow().as_ref() {
                            setup.detected(found.clone());
                        }
                    })
                });
                unsafe { NSOperationQueue::mainQueue().addOperationWithBlock(&done) };
            });
        }

        #[unsafe(method(test:))]
        fn test(&self, _: &AnyObject) {
            let ui = self.ivars().borrow();
            let ui = ui.as_ref().unwrap();
            let result = telegram::send_test(&value(&ui.token), &value(&ui.chat));
            ui.status.setStringValue(&ns(&result.err().unwrap_or("Sent. Check Telegram.".into())));
        }

        #[unsafe(method(loud:))]
        fn loud(&self, _: &AnyObject) {
            alert("Make alerts loud on your phone", LOUD_HELP, &["Done"]);
        }
    }
);

thread_local! {
    static SETUP: RefCell<Option<Retained<Setup>>> = const { RefCell::new(None) };
}

impl Setup {
    fn refresh(&self) {
        let ui = self.ivars().borrow();
        let ui = ui.as_ref().unwrap();
        let has_token = !value(&ui.token).is_empty();
        ui.detect.setEnabled(has_token);
        ui.test.setEnabled(has_token && !value(&ui.chat).is_empty());
    }

    fn detected(&self, found: Result<String, String>) {
        {
            let ui = self.ivars().borrow();
            let ui = ui.as_ref().unwrap();
            if let Ok(chat) = &found {
                ui.chat.setStringValue(&ns(chat));
            }
            ui.hint.setStringValue(&ns(match &found {
                Ok(_) => "Found your chat.",
                Err(why) => why,
            }));
        }
        self.refresh();
    }
}

/// The multi-field setup panel. Returns a destination, or None if cancelled or rejected.
pub fn setup_telegram() -> Option<Value> {
    let mtm = mtm();
    let panel = build(
        "Add a Telegram destination",
        "Alerts (with photos and video) are sent here.",
        &["Save", "Cancel"],
    );
    let view = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 360.0, 270.0));
    let name = field(&view, 230.0, "Destination name", "e.g. My phone", None, false, 360.0);
    let token = field(&view, 160.0, "Telegram bot token", "123456:ABC-DEF...", Some(TOKEN_HELP), true, 360.0);
    let chat = field(&view, 90.0, "Chat id", "e.g. 987654321", None, false, 250.0);
    let setup: Retained<Setup> = unsafe {
        msg_send![super(Setup::alloc(mtm).set_ivars(RefCell::new(None))), init]
    };
    let detect = button(&view, "Auto-detect", &setup, sel!(detect:), 258.0, 88.0);
    let grey = NSColor::secondaryLabelColor();
    let hint = text("Click Auto-detect, then send your bot any message.", rect(0.0, 70.0, 360.0, 16.0), 11.0, false, Some(&grey));
    let test = button(&view, "Send Test Message", &setup, sel!(test:), 0.0, 34.0);
    let x = test.frame().size.width + 8.0;
    let status = text("", rect(x, 39.0, 360.0 - x, 16.0), 11.0, false, Some(&grey));
    let loud = button(&view, "", &setup, sel!(loud:), 0.0, 0.0);
    loud.setBezelStyle(NSBezelStyle::HelpButton);
    loud.sizeToFit();
    let x = loud.frame().size.width + 6.0;
    let loud_label = text("Make alerts ring on silent or Do Not Disturb", rect(x, 3.0, 360.0 - x, 16.0), 11.0, false, Some(&grey));
    for label in [&hint, &status, &loud_label] {
        view.addSubview(label);
    }
    for input in [&token, &chat] {
        // enable the buttons as the fields fill in
        unsafe { input.setDelegate(Some(ProtocolObject::from_ref(&*setup))) };
    }
    *setup.ivars().borrow_mut() = Some(SetupUi { token, chat, detect, hint, test, status });
    setup.refresh();
    SETUP.with(|cell| *cell.borrow_mut() = Some(setup.clone()));
    panel.setAccessoryView(Some(&view));
    // the alert builds its panel lazily
    panel.window().setInitialFirstResponder(Some(&name));

    let saved = panel.runModal() == 1000;
    SETUP.with(|cell| cell.borrow_mut().take());
    if !saved {
        return None;
    }
    let ui = setup.ivars().borrow();
    let ui = ui.as_ref().unwrap();
    let (name, token, chat) = (value(&name), value(&ui.token), value(&ui.chat));
    if name.is_empty() || token.is_empty() || chat.is_empty() {
        return None;
    }
    if let Err(why) = telegram::check(&token, &chat) {
        let failed = build("Setup failed", &format!("Telegram says: {why}"), &[]);
        failed.setAlertStyle(NSAlertStyle::Critical);
        failed.runModal();
        return None;
    }
    Some(json!({"name": name, "token": token, "chat_id": chat}))
}

fn value(field: &NSTextField) -> String {
    field.stringValue().to_string().trim().to_owned()
}

fn field(
    parent: &NSView,
    y: f64,
    title: &str,
    placeholder: &str,
    help: Option<(&str, &str)>,
    secure: bool,
    width: f64,
) -> Retained<NSTextField> {
    let mtm = mtm();
    parent.addSubview(&text(title, rect(0.0, y + 22.0, 360.0, 18.0), 13.0, true, None));
    let frame = rect(0.0, y, width, 22.0);
    let input = if secure {
        Retained::into_super(NSSecureTextField::initWithFrame(NSSecureTextField::alloc(mtm), frame))
    } else {
        NSTextField::initWithFrame(NSTextField::alloc(mtm), frame)
    };
    input.setPlaceholderString(Some(&ns(placeholder)));
    parent.addSubview(&input);
    if let Some((label, url)) = help {
        parent.addSubview(&link(label, url, rect(0.0, y - 18.0, 360.0, 16.0)));
    }
    input
}

fn button(parent: &NSView, title: &str, target: &AnyObject, action: objc2::runtime::Sel, x: f64, y: f64) -> Retained<NSButton> {
    let button = unsafe { NSButton::buttonWithTitle_target_action(&ns(title), Some(target), Some(action), mtm()) };
    button.setFrameOrigin(NSPoint::new(x, y));
    parent.addSubview(&button);
    button
}

fn text(string: &str, frame: NSRect, size: f64, bold: bool, color: Option<&NSColor>) -> Retained<NSTextField> {
    let label = NSTextField::initWithFrame(NSTextField::alloc(mtm()), frame);
    label.setStringValue(&ns(string));
    label.setBezeled(false);
    label.setEditable(false);
    label.setSelectable(false);
    label.setDrawsBackground(false);
    let font = if bold { NSFont::boldSystemFontOfSize(size) } else { NSFont::systemFontOfSize(size) };
    label.setFont(Some(&font));
    if let Some(color) = color {
        label.setTextColor(Some(color));
    }
    label
}

fn link(string: &str, url: &str, frame: NSRect) -> Retained<NSTextField> {
    let label = NSTextField::initWithFrame(NSTextField::alloc(mtm()), frame);
    label.setBezeled(false);
    label.setEditable(false);
    label.setDrawsBackground(false);
    label.setSelectable(true);
    label.setAllowsEditingTextAttributes(true);
    let target = NSURL::URLWithString(&ns(url)).unwrap();
    let color = NSColor::linkColor();
    let font = NSFont::systemFontOfSize(11.0);
    let attributes = unsafe {
        NSDictionary::from_slices(
            &[NSLinkAttributeName, NSForegroundColorAttributeName, NSFontAttributeName],
            &[target.as_ref() as &AnyObject, color.as_ref(), font.as_ref()],
        )
    };
    let string = unsafe {
        NSAttributedString::initWithString_attributes(NSAttributedString::alloc(), &ns(string), Some(&attributes))
    };
    label.setAttributedStringValue(&string);
    label
}

pub fn show_about() {
    let application = NSApplication::sharedApplication(mtm());
    let credits = NSAttributedString::from_nsstring(&ns("Your Mac, guarded while you step away."));
    let name = ns("unplugg");
    let mut keys = vec![ns("ApplicationName"), ns("Credits")];
    let mut values: Vec<&AnyObject> = vec![name.as_ref(), credits.as_ref()];
    let icon = application.applicationIconImage();
    if let Some(icon) = &icon {
        keys.push(ns("ApplicationIcon"));
        values.push(icon.as_ref());
    }
    let keys: Vec<&NSString> = keys.iter().map(|key| &**key).collect();
    let options = NSDictionary::from_slices(&keys, &values);
    application.activateIgnoringOtherApps(true);
    unsafe { application.orderFrontStandardAboutPanelWithOptions(&options) };
}

pub fn show_help() {
    alert(
        "How unplugg works",
        "1. Plug in and add a Telegram destination.\n\
         2. Choose what counts as unsafe: power unplugged and/or screen moved.\n\
         3. Click Activate before you step away.\n\n\
         When the Mac is disturbed: it sirens, captures photo + video, sends a\n\
         location link, and alerts Telegram — then optionally locks.",
        &["Done"],
    );
}
