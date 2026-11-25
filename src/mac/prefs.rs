use std::cell::{Cell, RefCell};

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, ProtocolObject, Sel};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadMarker, MainThreadOnly, Message};
use objc2_app_kit::{
    NSApplication, NSBox, NSBoxType, NSButton, NSColor, NSControlSize, NSFont, NSImage,
    NSImageScaling, NSImageView, NSLineBreakMode, NSOpenPanel, NSPopUpButton, NSSegmentSwitchTracking,
    NSSegmentedControl, NSSlider, NSSwitch, NSTabViewController, NSTabViewControllerTabStyle,
    NSTabViewItem, NSTextAlignment, NSTextField, NSTitlePosition, NSView, NSViewController, NSWindow,
    NSWindowDelegate, NSWindowStyleMask, NSWindowToolbarStyle, NSWorkspace,
};
use objc2_foundation::{
    NSArray, NSNotification, NSObjectProtocol, NSOperationQueue, NSPoint, NSProcessInfo, NSRect,
    NSSize, NSString, NSURL,
};
use serde_json::Value;

use super::app::App;
use super::{devices, dialogs, guide, loginitem, permissions, sensors, utils};
use crate::telegram;

pub(super) const W: f64 = 480.0; // pane width
pub(super) const PAD: f64 = 20.0; // pane margin
pub(super) const ROW: f64 = 36.0; // grouped-form row height
pub(super) const TALL: f64 = 56.0; // row with a line of explanation
pub(super) const INSET: f64 = 12.0; // row padding inside a group
const TILE: f64 = 26.0; // icon tile

type Color = fn() -> Retained<NSColor>;

const PERMS: [(&str, &str, &str, &str, Color); 3] = [
    ("camera", "Camera", "Photos and video of a thief.", "camera.fill", NSColor::systemGreenColor),
    ("microphone", "Microphone", "Sound in that video.", "mic.fill", NSColor::systemOrangeColor),
    ("location", "Location", "A map link to your Mac.", "location.fill", NSColor::systemBlueColor),
];

fn status(state: &str) -> (&'static str, Retained<NSColor>) {
    match state {
        "granted" => ("● Granted", NSColor::systemGreenColor()),
        "denied" => ("● Denied", NSColor::systemRedColor()),
        "unset" => ("● Not set", NSColor::secondaryLabelColor()),
        "via-terminal" => ("● Via terminal", NSColor::systemOrangeColor()),
        _ => ("Unavailable", NSColor::tertiaryLabelColor()),
    }
}

pub(super) struct Steps {
    pub pages: Vec<Retained<NSView>>,
    pub dots: Vec<Retained<NSTextField>>,
    pub back: Retained<NSButton>,
    pub next: Retained<NSButton>,
    pub step: Cell<usize>,
}

#[derive(Default)]
pub(super) struct Ui {
    pub checks: Vec<&'static str>,
    pub window: Option<Retained<NSWindow>>,
    tabs: Option<Retained<NSTabViewController>>,
    popup: Option<Retained<NSPopUpButton>>,
    destbtns: Option<Retained<NSSegmentedControl>>,
    testbtn: Option<Retained<NSButton>>,
    sens: Option<Retained<NSTextField>>,
    dirlabel: Option<Retained<NSTextField>>,
    perms: Vec<(&'static str, Retained<NSTextField>, Retained<NSButton>)>,
    pub steps: Option<Steps>,
}

pub(super) struct Ivars {
    pub app: Retained<App>,
    pub ui: RefCell<Ui>,
}

define_class!(
    /// Window + state for Settings, and for the setup guide, which shares its rows.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = Ivars]
    pub(super) struct Prefs;

    unsafe impl NSObjectProtocol for Prefs {}

    unsafe impl NSWindowDelegate for Prefs {
        #[unsafe(method(windowDidBecomeKey:))]
        fn became_key(&self, _: &NSNotification) {
            // pick up changes made in System Settings, and prompt answers
            refresh_perms(self);
            permissions::set_listener(refresh_open);
        }

        #[unsafe(method(windowWillClose:))]
        fn will_close(&self, _: &NSNotification) {
            let app = &self.ivars().app;
            let held = if self.ivars().ui.borrow().steps.is_some() {
                app.set("setup_done", true.into());
                GUIDE.with(|guide| guide.borrow_mut().take())
            } else {
                let pane = self.ivars().ui.borrow().tabs.as_ref().unwrap().selectedTabViewItemIndex();
                app.set("settings_pane", pane.into());
                PREFS.with(|prefs| prefs.borrow_mut().take())
            };
            utils::show_in_dock(app.get("show_in_dock") == true, self.mtm());
            // let this method return before the controller goes away
            let release = RcBlock::new(move || {
                let _ = &held;
            });
            unsafe { NSOperationQueue::mainQueue().addOperationWithBlock(&release) };
        }
    }

    impl Prefs {
        #[unsafe(method(toggle:))]
        fn toggle(&self, sender: &NSSwitch) {
            let key = self.ivars().ui.borrow().checks[sender.tag() as usize];
            let on = sender.state() == 1;
            if key == "block_lid_sleep" && !super::lidsleep::set(on, self.ivars().app.get("activated") == true) {
                sender.setState(if on { 0 } else { 1 });
                return;
            }
            self.ivars().app.set(key, on.into());
            if key == "launch_at_login" {
                let _ = if on { loginitem::enable() } else { loginitem::disable() };
            }
        }

        #[unsafe(method(sensitivity:))]
        fn sensitivity(&self, sender: &NSSlider) {
            let degrees = sender.doubleValue().round() as i64;
            self.ivars().app.set("angle_threshold", degrees.into());
            let ui = self.ivars().ui.borrow();
            ui.sens.as_ref().unwrap().setStringValue(&NSString::from_str(&format!("{degrees}°")));
        }

        #[unsafe(method(cameraPick:))]
        fn camera_pick(&self, sender: &NSPopUpButton) {
            self.ivars().app.set("camera", title(sender).into());
        }

        #[unsafe(method(micPick:))]
        fn mic_pick(&self, sender: &NSPopUpButton) {
            self.ivars().app.set("mic", title(sender).into());
        }

        #[unsafe(method(speakerPick:))]
        fn speaker_pick(&self, sender: &NSPopUpButton) {
            let name = if sender.indexOfSelectedItem() == 0 { String::new() } else { title(sender) };
            self.ivars().app.set("speaker_name", name.into());
        }

        #[unsafe(method(chooseFolder:))]
        fn choose_folder(&self, _: &AnyObject) {
            let panel = NSOpenPanel::openPanel(self.mtm());
            panel.setCanChooseDirectories(true);
            panel.setCanChooseFiles(false);
            panel.setAllowsMultipleSelection(false);
            panel.setCanCreateDirectories(true);
            if panel.runModal() != 1 {
                return;
            }
            let Some(path) = panel.URLs().firstObject().and_then(|url| url.path()) else { return };
            let path = path.to_string();
            self.ivars().app.set("media_dir", path.clone().into());
            self.ivars().ui.borrow().dirlabel.as_ref().unwrap().setStringValue(&NSString::from_str(&short(&path)));
        }

        #[unsafe(method(openFolder:))]
        fn open_folder(&self, _: &AnyObject) {
            let dir = self.ivars().app.get("media_dir");
            let dir = dir.as_str().unwrap_or("");
            let path = match dir.strip_prefix('~') {
                Some(rest) => crate::config::home().join(rest.trim_start_matches('/')),
                None => dir.into(),
            };
            // capture makes it lazily
            let _ = std::fs::create_dir_all(&path);
            let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
            NSWorkspace::sharedWorkspace().openURL(&url);
        }

        #[unsafe(method(destTest:))]
        fn dest_test(&self, _: &AnyObject) {
            let index = self.ivars().ui.borrow().popup.as_ref().unwrap().indexOfSelectedItem();
            let destinations = self.ivars().app.get("telegram_destinations");
            let Some(dest) = destinations.get(index as usize) else { return };
            let field = |key: &str| dest[key].as_str().map(str::to_owned).unwrap_or_else(|| dest[key].to_string());
            match telegram::send_test(&field("token"), &field("chat_id")) {
                Ok(()) => dialogs::alert("Test sent", &format!("Check Telegram for {}.", field("name")), &[]),
                Err(why) => dialogs::alert("Test failed", &format!("Telegram says: {why}"), &[]),
            };
        }

        #[unsafe(method(destEdit:))]
        fn dest_edit(&self, sender: &NSSegmentedControl) {
            let mut destinations = self.ivars().app.get("telegram_destinations").as_array().cloned().unwrap_or_default();
            if sender.selectedSegment() == 0 {
                match dialogs::setup_telegram() {
                    Some(dest) => destinations.push(dest),
                    None => return,
                }
            } else {
                let name = title(self.ivars().ui.borrow().popup.as_ref().unwrap());
                destinations.retain(|dest| dest["name"] != name.as_str());
            }
            self.ivars().app.set("telegram_destinations", destinations.into());
            refresh_destinations(self);
        }

        #[unsafe(method(permGrant:))]
        fn perm_grant(&self, sender: &NSButton) {
            let key = PERMS[sender.tag() as usize].0;
            // a denied permission can't be asked again, so send them to System Settings
            if permissions::status(key) == "denied" {
                permissions::open_settings(key);
            } else {
                permissions::request(key);
            }
            refresh_perms(self);
        }

        #[unsafe(method(next:))]
        fn next(&self, _: &AnyObject) {
            guide::next(self);
        }

        #[unsafe(method(back:))]
        fn back(&self, _: &AnyObject) {
            guide::back(self);
        }
    }
);

thread_local! {
    static PREFS: RefCell<Option<Retained<Prefs>>> = const { RefCell::new(None) };
    pub(super) static GUIDE: RefCell<Option<Retained<Prefs>>> = const { RefCell::new(None) };
}

impl Prefs {
    pub(super) fn new(app: &App) -> Retained<Self> {
        let this = Self::alloc(app.mtm()).set_ivars(Ivars { app: app.retain(), ui: RefCell::new(Ui::default()) });
        unsafe { msg_send![super(this), init] }
    }

    pub(super) fn get(&self, key: &str) -> Value {
        self.ivars().app.get(key)
    }
}

fn refresh_open() {
    for cell in [&PREFS, &GUIDE] {
        cell.with(|ctrl| {
            if let Some(ctrl) = ctrl.borrow().as_ref() {
                refresh_perms(ctrl);
            }
        });
    }
}

pub fn open(app: &App) {
    let ctrl = PREFS.with(|prefs| prefs.borrow().clone()).unwrap_or_else(|| {
        let ctrl = build(app);
        ctrl.ivars().ui.borrow().window.as_ref().unwrap().center();
        PREFS.with(|prefs| *prefs.borrow_mut() = Some(ctrl.clone()));
        ctrl
    });
    front(&ctrl);
}

/// A window is open: Dock icon and Cmd-Tab, like any app.
pub(super) fn front(ctrl: &Prefs) {
    utils::show_in_dock(true, ctrl.mtm());
    ctrl.ivars().ui.borrow().window.as_ref().unwrap().makeKeyAndOrderFront(None);
    NSApplication::sharedApplication(ctrl.mtm()).activateIgnoringOtherApps(true);
}

fn build(app: &App) -> Retained<Prefs> {
    let ctrl = Prefs::new(app);
    let mtm = ctrl.mtm();
    let tabs = NSTabViewController::new(mtm);
    tabs.setTabStyle(NSTabViewControllerTabStyle::Toolbar);
    let panes: [(&str, &str, &str, fn(&Prefs, &NSView, f64) -> f64); 4] = [
        ("General", "gearshape", "NSPreferencesGeneral", general_page),
        ("Alerts", "bell.badge", "NSCaution", alerts_page),
        ("Devices", "video", "NSComputer", devices_page),
        ("Permissions", "hand.raised", "NSUserAccounts", permissions_page),
    ];
    for (name, symbol, fallback, builder) in panes {
        let page = page(mtm, 0.0);
        page.setFrameSize(NSSize::new(W, builder(&ctrl, &page, PAD)));
        let vc = NSViewController::new(mtm);
        vc.setView(&page);
        vc.setTitle(Some(&NSString::from_str(name)));
        vc.setPreferredContentSize(page.frame().size);
        let item = NSTabViewItem::tabViewItemWithViewController(&vc);
        item.setImage(icon(symbol, Some(fallback)).as_deref());
        tabs.addTabViewItem(&item);
    }
    let pane = ctrl.get("settings_pane").as_i64().unwrap_or(0).clamp(0, 3);
    tabs.setSelectedTabViewItemIndex(pane as isize);

    // the title follows the pane
    let window = NSWindow::windowWithContentViewController(&tabs);
    window.setStyleMask(NSWindowStyleMask::Titled | NSWindowStyleMask::Closable);
    if macos(11) {
        window.setToolbarStyle(NSWindowToolbarStyle::Preference);
    }
    unsafe { window.setReleasedWhenClosed(false) };
    window.setDelegate(Some(ProtocolObject::from_ref(&*ctrl)));
    {
        let mut ui = ctrl.ivars().ui.borrow_mut();
        ui.window = Some(window);
        ui.tabs = Some(tabs);
    }
    refresh_perms(&ctrl);
    ctrl
}

fn general_page(ctrl: &Prefs, view: &NSView, y: f64) -> f64 {
    let mut rows = vec![
        ("Power unplugged", switch(ctrl, "monitor_power")),
        ("Screen closed or moved", switch(ctrl, "monitor_lid")),
    ];
    if sensors::has_angle_sensor() {
        rows.push(("Screen sensitivity", sensitivity(ctrl)));
    }
    let y = group(view, y, Some("Alarm when"), rows, ROW);
    let y = group(view, y, Some("While armed"), vec![
        ("Show overlay", switch(ctrl, "show_overlay")),
        ("Lock Mac when triggered", switch(ctrl, "lock_device")),
        ("Stay awake with the lid closed", switch(ctrl, "block_lid_sleep")),
    ], ROW);
    let y = group(view, y, None, vec![
        ("Show in Dock", switch(ctrl, "show_in_dock")),
        ("Launch at login", switch(ctrl, "launch_at_login")),
    ], ROW);
    group(view, y, Some("Updates"), vec![
        ("Check for updates automatically", switch(ctrl, "check_updates")),
        ("Install updates automatically", switch(ctrl, "auto_update")),
    ], ROW)
}

fn alerts_page(ctrl: &Prefs, view: &NSView, y: f64) -> f64 {
    let y = group(view, y, Some("When triggered"), vec![
        ("Play siren at full volume", switch(ctrl, "play_sound")),
        ("Send photo & video", switch(ctrl, "capture_media")),
        ("Flash screen for photos", switch(ctrl, "flash_screen")),
        ("Record audio with video", switch(ctrl, "record_audio")),
        ("Send location", switch(ctrl, "send_location")),
        ("Send live location", switch(ctrl, "live_location")),
    ], ROW);
    let y = telegram_group(ctrl, view, y);
    let dir = ctrl.get("media_dir");
    let dirlabel = label(&short(dir.as_str().unwrap_or("")), 12.0, Some(&NSColor::secondaryLabelColor()), false, None);
    dirlabel.setFrameSize(NSSize::new(150.0, dirlabel.frame().size.height));
    dirlabel.setLineBreakMode(NSLineBreakMode::ByTruncatingMiddle);
    let choose = button("Choose…", ctrl, sel!(chooseFolder:));
    let reveal = button("Open", ctrl, sel!(openFolder:));
    let row = hstack(&[&dirlabel, &choose, &reveal]);
    ctrl.ivars().ui.borrow_mut().dirlabel = Some(dirlabel);
    group(view, y, Some("Saved media"), vec![("Folder", row)], ROW)
}

pub(super) fn telegram_group(ctrl: &Prefs, view: &NSView, y: f64) -> f64 {
    let mtm = ctrl.mtm();
    let popup = popup(ctrl, &[], 0, None, 150.0);
    let images: Vec<_> = ["NSAddTemplate", "NSRemoveTemplate"]
        .iter()
        .filter_map(|name| NSImage::imageNamed(&NSString::from_str(name)))
        .collect();
    let destbtns = unsafe {
        NSSegmentedControl::segmentedControlWithImages_trackingMode_target_action(
            &NSArray::from_retained_slice(&images),
            NSSegmentSwitchTracking::Momentary,
            Some(ctrl),
            Some(sel!(destEdit:)),
            mtm,
        )
    };
    let testbtn = button("Send Test", ctrl, sel!(destTest:));
    let row = hstack(&[&popup, &destbtns, &testbtn]);
    {
        let mut ui = ctrl.ivars().ui.borrow_mut();
        ui.popup = Some(popup);
        ui.destbtns = Some(destbtns);
        ui.testbtn = Some(testbtn);
    }
    refresh_destinations(ctrl);
    group(view, y, Some("Telegram"), vec![("Destinations", row)], ROW)
}

fn devices_page(ctrl: &Prefs, view: &NSView, y: f64) -> f64 {
    let or = |items: Vec<String>, default: &str| if items.is_empty() { vec![default.to_owned()] } else { items };
    let cams = or(devices::cameras(), "Default camera");
    let mics = or(devices::microphones(), "Default microphone");
    let camera = devices::camera(ctrl.get("camera").as_str().unwrap_or(""));
    let mic = devices::microphone(ctrl.get("mic").as_str().unwrap_or(""));
    let cam = popup(ctrl, &cams, index(&cams, camera.as_deref()), Some(sel!(cameraPick:)), 200.0);
    let mic = popup(ctrl, &mics, index(&mics, mic.as_deref()), Some(sel!(micPick:)), 200.0);
    let y = group(view, y, Some("Capture"), vec![("Camera", cam.into_super().into_super().into_super()), ("Microphone", mic.into_super().into_super().into_super())], ROW);
    let mut speakers = vec!["System default".to_owned()];
    speakers.extend(devices::speakers());
    let selected = index(&speakers, ctrl.get("speaker_name").as_str());
    let speaker = popup(ctrl, &speakers, selected, Some(sel!(speakerPick:)), 200.0);
    group(view, y, Some("Siren"), vec![("Speaker", speaker.into_super().into_super().into_super())], ROW)
}

fn index(items: &[String], name: Option<&str>) -> usize {
    name.and_then(|name| items.iter().position(|item| item == name)).unwrap_or(0)
}

pub(super) fn permissions_page(ctrl: &Prefs, view: &NSView, y: f64) -> f64 {
    let mut rows = Vec::new();
    for (tag, (key, name, why, symbol, color)) in PERMS.into_iter().enumerate() {
        let grant = button("Open Settings…", ctrl, sel!(permGrant:));
        grant.setTag(tag as isize);
        let state = label("", 12.0, None, false, None);
        state.setFrameSize(NSSize::new(100.0, state.frame().size.height));
        state.setAlignment(NSTextAlignment::Right);
        let control = hstack(&[&grant, &state]);
        let width = W - 2.0 * PAD - 2.0 * INSET - control.frame().size.width - 8.0;
        rows.push((info(name, why, &tile(symbol, &color()), width), Some(control)));
        ctrl.ivars().ui.borrow_mut().perms.push((key, state, grant));
    }
    group_views(view, y, Some("All optional"), rows, TALL)
}

// ---- row builders ---------------------------------------------------------

pub(super) fn page(mtm: MainThreadMarker, height: f64) -> Retained<NSView> {
    let page: Retained<Page> = unsafe { msg_send![Page::alloc(mtm), initWithFrame: rect(0.0, 0.0, W, height)] };
    Retained::into_super(page)
}

define_class!(
    /// Lays out top-down.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    struct Page;

    impl Page {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }
    }
);

pub(super) fn group(view: &NSView, y: f64, title: Option<&str>, rows: Vec<(&str, Retained<NSView>)>, row: f64) -> f64 {
    let rows = rows
        .into_iter()
        .map(|(text, control)| (Retained::into_super(Retained::into_super(label(text, 13.0, None, false, None))), Some(control)))
        .collect();
    group_views(view, y, title, rows, row)
}

/// Optional heading, then a rounded box of label/control rows. Returns the next y.
pub(super) fn group_views(view: &NSView, mut y: f64, title: Option<&str>, rows: Vec<(Retained<NSView>, Option<Retained<NSView>>)>, row: f64) -> f64 {
    let mtm = view.mtm();
    if let Some(title) = title {
        let heading = label(title, 13.0, None, true, None);
        heading.setFrameOrigin(NSPoint::new(PAD + 2.0, y));
        view.addSubview(&heading);
        y += 22.0;
    }
    let (width, height) = (W - 2.0 * PAD, row * rows.len() as f64);
    let group = NSBox::initWithFrame(NSBox::alloc(mtm), rect(PAD, y, width, height));
    group.setTitlePosition(NSTitlePosition::NoTitle);
    group.setContentViewMargins(NSSize::new(0.0, 0.0));
    let inner = group.contentView().unwrap();
    for (i, (text, control)) in rows.into_iter().enumerate() {
        // the box's own content isn't flipped
        let top = height - row * i as f64;
        if i > 0 {
            let line = NSBox::initWithFrame(NSBox::alloc(mtm), rect(INSET, top, width - 2.0 * INSET, 1.0));
            line.setBoxType(NSBoxType::Separator);
            inner.addSubview(&line);
        }
        text.setFrameOrigin(NSPoint::new(INSET, top - (row + text.frame().size.height) / 2.0));
        inner.addSubview(&text);
        if let Some(control) = control {
            let size = control.frame().size;
            control.setFrameOrigin(NSPoint::new(width - INSET - size.width, top - (row + size.height) / 2.0));
            inner.addSubview(&control);
        }
    }
    view.addSubview(&group);
    y + height + 20.0
}

/// Icon tile, then a title with a short grey line under it.
pub(super) fn info(title: &str, detail: &str, tile: &NSView, width: f64) -> Retained<NSView> {
    let mtm = tile.mtm();
    let name = label(title, 13.0, None, false, None);
    let why = label(detail, 11.0, Some(&NSColor::secondaryLabelColor()), false, Some(width - TILE - 10.0));
    name.setFrameOrigin(NSPoint::new(TILE + 10.0, 0.0));
    why.setFrameOrigin(NSPoint::new(TILE + 10.0, name.frame().size.height + 1.0));
    let height = name.frame().size.height + 1.0 + why.frame().size.height;
    tile.setFrameOrigin(NSPoint::new(0.0, (height - TILE) / 2.0));
    let view = page(mtm, height);
    view.setFrameSize(NSSize::new(width, height));
    view.addSubview(tile);
    view.addSubview(&name);
    view.addSubview(&why);
    view
}

/// White SF Symbol on a coloured rounded square, like System Settings' icons.
pub(super) fn tile(symbol: &str, color: &NSColor) -> Retained<NSView> {
    let mtm = MainThreadMarker::new().unwrap();
    let tile = NSBox::initWithFrame(NSBox::alloc(mtm), rect(0.0, 0.0, TILE, TILE));
    tile.setBoxType(NSBoxType::Custom);
    tile.setTitlePosition(NSTitlePosition::NoTitle);
    tile.setContentViewMargins(NSSize::new(0.0, 0.0));
    tile.setBorderWidth(0.0);
    tile.setCornerRadius(6.0);
    tile.setFillColor(color);
    // no SF Symbols before macOS 11
    if let Some(image) = icon(symbol, None) {
        let glyph = NSImageView::imageViewWithImage(&image, mtm);
        glyph.setFrame(rect(5.0, 5.0, TILE - 10.0, TILE - 10.0));
        glyph.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
        glyph.setContentTintColor(Some(&NSColor::whiteColor()));
        tile.addSubview(&glyph);
    }
    Retained::into_super(tile)
}

pub(super) fn switch(ctrl: &Prefs, key: &'static str) -> Retained<NSView> {
    let switch = NSSwitch::new(ctrl.mtm());
    // per the HIG for grouped-form rows
    switch.setControlSize(NSControlSize::Mini);
    switch.sizeToFit();
    switch.setState(if ctrl.get(key) == true { 1 } else { 0 });
    let mut ui = ctrl.ivars().ui.borrow_mut();
    switch.setTag(ui.checks.len() as isize);
    ui.checks.push(key);
    unsafe {
        switch.setTarget(Some(ctrl));
        switch.setAction(Some(sel!(toggle:)));
    }
    Retained::into_super(Retained::into_super(switch))
}

fn sensitivity(ctrl: &Prefs) -> Retained<NSView> {
    let degrees = ctrl.get("angle_threshold").as_f64().unwrap_or(5.0);
    let slider = unsafe {
        NSSlider::sliderWithValue_minValue_maxValue_target_action(degrees, 1.0, 45.0, Some(ctrl), Some(sel!(sensitivity:)), ctrl.mtm())
    };
    slider.setFrameSize(NSSize::new(150.0, slider.frame().size.height));
    let sens = label(&format!("{degrees}°"), 12.0, Some(&NSColor::secondaryLabelColor()), false, None);
    sens.setFrameSize(NSSize::new(32.0, sens.frame().size.height));
    let row = hstack(&[&slider, &sens]);
    ctrl.ivars().ui.borrow_mut().sens = Some(sens);
    row
}

fn popup(ctrl: &Prefs, items: &[String], selected: usize, action: Option<Sel>, width: f64) -> Retained<NSPopUpButton> {
    let popup = NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(ctrl.mtm()), rect(0.0, 0.0, width, 25.0), false);
    let titles: Vec<_> = items.iter().map(|item| NSString::from_str(item)).collect();
    popup.addItemsWithTitles(&NSArray::from_retained_slice(&titles));
    if selected < items.len() {
        popup.selectItemAtIndex(selected as isize);
    }
    unsafe {
        popup.setTarget(Some(ctrl));
        popup.setAction(action);
    }
    popup
}

pub(super) fn button(title: &str, target: &AnyObject, action: Sel) -> Retained<NSButton> {
    let mtm = MainThreadMarker::new().unwrap();
    unsafe { NSButton::buttonWithTitle_target_action(&NSString::from_str(title), Some(target), Some(action), mtm) }
}

/// Packs controls left-to-right, vertically centred, into one view.
fn hstack(views: &[&NSView]) -> Retained<NSView> {
    let mtm = MainThreadMarker::new().unwrap();
    let height = views.iter().map(|view| view.frame().size.height).fold(0.0, f64::max);
    let row = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 0.0, height));
    let mut x = 0.0;
    for view in views {
        let size = view.frame().size;
        view.setFrameOrigin(NSPoint::new(x, (height - size.height) / 2.0));
        row.addSubview(view);
        x += size.width + 8.0;
    }
    row.setFrameSize(NSSize::new(x - 8.0, height));
    row
}

fn refresh_destinations(ctrl: &Prefs) {
    let ui = ctrl.ivars().ui.borrow();
    let popup = ui.popup.as_ref().unwrap();
    popup.removeAllItems();
    let destinations = ctrl.get("telegram_destinations");
    let names: Vec<_> = destinations
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|dest| dest["name"].as_str())
        .map(NSString::from_str)
        .collect();
    let has = !names.is_empty();
    let names = if has { names } else { vec![NSString::from_str("None yet")] };
    popup.addItemsWithTitles(&NSArray::from_retained_slice(&names));
    popup.setEnabled(has);
    ui.destbtns.as_ref().unwrap().setEnabled_forSegment(has, 1);
    ui.testbtn.as_ref().unwrap().setEnabled(has);
}

pub(super) fn refresh_perms(ctrl: &Prefs) {
    for (key, state, grant) in &ctrl.ivars().ui.borrow().perms {
        let current = permissions::status(key);
        let (text, color) = status(current);
        state.setStringValue(&NSString::from_str(text));
        state.setTextColor(Some(&color));
        grant.setHidden(current == "granted");
        // not "Allow": the HIG keeps that word for the system's own prompt
        grant.setTitle(&NSString::from_str(if current == "unset" { "Request…" } else { "Open Settings…" }));
    }
}

fn title(popup: &NSPopUpButton) -> String {
    popup.titleOfSelectedItem().map(|title| title.to_string()).unwrap_or_default()
}

fn short(path: &str) -> String {
    let home = crate::config::home().to_string_lossy().into_owned();
    path.replace(&home, "~")
}

pub(super) fn macos(major: isize) -> bool {
    NSProcessInfo::processInfo().operatingSystemVersion().majorVersion >= major
}

fn icon(symbol: &str, fallback: Option<&str>) -> Option<Retained<NSImage>> {
    if macos(11) {
        return NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str(symbol), None);
    }
    NSImage::imageNamed(&NSString::from_str(fallback?))
}

/// One line, or wrapped to `width`.
pub(super) fn label(text: &str, size: f64, color: Option<&NSColor>, bold: bool, width: Option<f64>) -> Retained<NSTextField> {
    let mtm = MainThreadMarker::new().unwrap();
    let string = NSString::from_str(text);
    let label = if width.is_some() {
        NSTextField::wrappingLabelWithString(&string, mtm)
    } else {
        NSTextField::labelWithString(&string, mtm)
    };
    let font = if bold { NSFont::boldSystemFontOfSize(size) } else { NSFont::systemFontOfSize(size) };
    label.setFont(Some(&font));
    if let Some(color) = color {
        label.setTextColor(Some(color));
    }
    match width {
        Some(width) => {
            label.setSelectable(false);
            let height = label.cell().unwrap().cellSizeForBounds(rect(0.0, 0.0, width, 1000.0)).height;
            label.setFrameSize(NSSize::new(width, height));
        }
        None => label.sizeToFit(),
    }
    label
}

pub(super) fn rect(x: f64, y: f64, width: f64, height: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(width, height))
}
