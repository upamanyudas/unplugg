use std::cell::RefCell;
use std::fs::{self, File, OpenOptions};
use std::sync::{mpsc, Arc, RwLock};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject, Sel};
use objc2::{
    define_class, msg_send, sel, AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly,
};
use objc2_app_kit::{
    NSAlert, NSApplication, NSApplicationDelegate, NSApplicationTerminateReply, NSImage, NSMenu,
    NSMenuItem, NSStatusBar, NSStatusItem, NSVariableStatusItemLength,
};
use objc2_foundation::{NSProcessInfo, NSString, NSTimer};
use serde_json::{Map, Value};

use super::{auth, sensors, utils};
use crate::monitor::{Monitor, MonitorConfig, Sensors};

struct State {
    config: Map<String, Value>,
    laptop: bool,
    monitor: Monitor<sensors::MacSensors>,
    events: mpsc::Receiver<String>,
    item: Retained<NSStatusItem>,
    toggle: Retained<NSMenuItem>,
}

struct AppIvars(RefCell<Option<State>>);

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = objc2::MainThreadOnly]
    #[ivars = AppIvars]
    struct App;

    unsafe impl NSObjectProtocol for App {}

    unsafe impl NSApplicationDelegate for App {
        #[unsafe(method(applicationShouldTerminate:))]
        fn should_terminate(&self, _: &NSApplication) -> NSApplicationTerminateReply {
            let armed = self.ivars().0.borrow().as_ref().unwrap().config["activated"] == true;
            if armed && !auth::require("quit while armed") {
                return NSApplicationTerminateReply::TerminateCancel;
            }
            let mut state = self.ivars().0.borrow_mut();
            let state = state.as_mut().unwrap();
            state.monitor.stop();
            stop_alarm();
            hide_overlay();
            NSApplicationTerminateReply::TerminateNow
        }
    }

    impl App {
        #[unsafe(method(toggle:))]
        fn toggle(&self, _: &AnyObject) {
            let armed = self.ivars().0.borrow().as_ref().unwrap().config["activated"] == true;
            if armed {
                if auth::require("turn off the alarm") { self.deactivate(); }
                return;
            }
            let (laptop, monitor_power) = {
                let state = self.ivars().0.borrow();
                let state = state.as_ref().unwrap();
                (state.laptop, state.config["monitor_power"] == true)
            };
            if !laptop {
                alert("unplugg is made for MacBooks", "It guards the power cable and the lid. This Mac has no battery or lid, so the alarm can't be activated here.", &["OK"]);
                return;
            }
            if monitor_power && !sensors::MacSensors.is_plugged_in() {
                let settings = settings_name();
                alert("Plug in first", &format!("Your MacBook should be plugged to power before you can activate the alarm. Alternatively, turn off \"Power unplugged\" in {}.", settings.trim_end_matches('…')), &["OK"]);
                return;
            }
            let mut state = self.ivars().0.borrow_mut();
            let state = state.as_mut().unwrap();
            state.config["activated"] = true.into();
            state.monitor.start();
            state.toggle.setTitle(&NSString::from_str("Deactivate"));
            set_icon(&state.item, "menubar_armed.pdf", true);
            if state.config["show_overlay"] == true { show_overlay(self, false, false); }
            let _ = crate::config::save_config(&state.config);
        }

        #[unsafe(method(preferences:))]
        fn preferences(&self, _: &AnyObject) { open_preferences(); }
        #[unsafe(method(about:))]
        fn about(&self, _: &AnyObject) { show_about(); }
        #[unsafe(method(updates:))]
        fn updates(&self, _: &AnyObject) { check_updates(); }
        #[unsafe(method(guide:))]
        fn guide(&self, _: &AnyObject) { open_guide(); }
        #[unsafe(method(help:))]
        fn help(&self, _: &AnyObject) { show_help(); }
        #[unsafe(method(quit:))]
        fn quit(&self, _: &AnyObject) {
            NSApplication::sharedApplication(self.mtm()).terminate(None);
        }
        #[unsafe(method(launch:))]
        fn launch(&self, _: &AnyObject) {
            prepare_alarm();
            if self.ivars().0.borrow().as_ref().unwrap().config["setup_done"] != true { open_guide(); }
        }
        #[unsafe(method(scheduleUpdates:))]
        fn schedule_updates(&self, _: &AnyObject) { schedule_updates(); }
        #[unsafe(method(poll:))]
        fn poll(&self, _: &AnyObject) {
            let mut state = self.ivars().0.borrow_mut();
            let state = state.as_mut().unwrap();
            if state.events.try_recv().is_ok() && state.config["activated"] == true {
                if state.config["play_sound"] == true { start_alarm(); }
                set_icon(&state.item, "menubar_alarm.pdf", false);
                if state.config["show_overlay"] == true || state.config["flash_screen"] == true {
                    show_overlay(self, true, state.config["flash_screen"] == true);
                }
            }
        }
    }
);

impl App {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(AppIvars(RefCell::new(None)));
        unsafe { msg_send![super(this), init] }
    }

    fn deactivate(&self) {
        let mut state = self.ivars().0.borrow_mut();
        let state = state.as_mut().unwrap();
        state.config["activated"] = false.into();
        state.monitor.stop();
        while state.events.try_recv().is_ok() {}
        stop_alarm();
        hide_overlay();
        state.toggle.setTitle(&NSString::from_str("Activate"));
        set_icon(&state.item, "menubar.pdf", true);
        let _ = crate::config::save_config(&state.config);
    }

    fn unlock(&self) {
        if !auth::require("unlock the screen") {
            listen_overlay();
            return;
        }
        hide_overlay();
        stop_alarm();
        if alert(
            "Deactivate the alarm?",
            "Keep Armed hides this cover; the alarm keeps watching.",
            &["Deactivate", "Keep Armed"],
        ) == 0
        {
            self.deactivate();
        }
    }
}

fn alert(title: &str, message: &str, buttons: &[&str]) -> isize {
    let mtm = MainThreadMarker::new().unwrap();
    let alert = NSAlert::new(mtm);
    let application = NSApplication::sharedApplication(mtm);
    unsafe { alert.setIcon(application.applicationIconImage().as_deref()) };
    alert.setMessageText(&NSString::from_str(title));
    alert.setInformativeText(&NSString::from_str(message));
    for title in buttons {
        alert.addButtonWithTitle(&NSString::from_str(title));
    }
    application.activateIgnoringOtherApps(true);
    alert.runModal() - 1000
}

fn settings_name() -> &'static str {
    if NSProcessInfo::processInfo()
        .operatingSystemVersion()
        .majorVersion
        >= 13
    {
        "Settings…"
    } else {
        "Preferences…"
    }
}

fn set_icon(item: &NSStatusItem, name: &str, template: bool) {
    let path = utils::asset_path(name);
    if let Some(image) = NSImage::initWithContentsOfFile(
        NSImage::alloc(),
        &NSString::from_str(&path.to_string_lossy()),
    ) {
        image.setTemplate(template);
        if let Some(button) = item.button(MainThreadMarker::new().unwrap()) {
            button.setImage(Some(&image));
        }
    }
}

fn menu_item(
    title: &str,
    action: Sel,
    key: &str,
    target: &App,
    mtm: MainThreadMarker,
) -> Retained<NSMenuItem> {
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            Some(action),
            &NSString::from_str(key),
        )
    };
    unsafe {
        item.setTarget(Some(target));
    }
    item
}

fn install_main_menu(application: &NSApplication, mtm: MainThreadMarker) {
    let bar = NSMenu::new(mtm);
    for (title, entries) in [
        (
            "unplugg",
            vec![
                ("Hide unplugg", sel!(hide:), "h"),
                ("Quit unplugg", sel!(terminate:), "q"),
            ],
        ),
        (
            "Edit",
            vec![
                ("Undo", sel!(undo:), "z"),
                ("Redo", sel!(redo:), "Z"),
                ("Cut", sel!(cut:), "x"),
                ("Copy", sel!(copy:), "c"),
                ("Paste", sel!(paste:), "v"),
                ("Select All", sel!(selectAll:), "a"),
            ],
        ),
    ] {
        let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(title));
        for (name, action, key) in entries {
            unsafe {
                menu.addItemWithTitle_action_keyEquivalent(
                    &NSString::from_str(name),
                    Some(action),
                    &NSString::from_str(key),
                );
            }
        }
        let item = NSMenuItem::new(mtm);
        item.setSubmenu(Some(&menu));
        bar.addItem(&item);
    }
    application.setMainMenu(Some(&bar));
}

fn single_instance() -> Option<File> {
    let path = crate::config::config_file();
    let dir = path.parent()?;
    fs::create_dir_all(dir).ok()?;
    let lock = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join("instance.lock"))
        .ok()?;
    lock.try_lock().ok()?;
    Some(lock)
}

pub fn run() {
    let mtm = MainThreadMarker::new().expect("main thread");
    let Some(_lock) = single_instance() else {
        return;
    };
    let application = NSApplication::sharedApplication(mtm);
    super::permissions::init();
    install_main_menu(&application, mtm);
    let mut config = crate::config::load_config().unwrap_or_else(|_| crate::config::defaults());
    config["activated"] = false.into();
    utils::show_in_dock(config["show_in_dock"] == true, mtm);
    let laptop = sensors::is_laptop();
    if !laptop && alert("unplugg is made for MacBooks", "It guards the power cable and the lid. This Mac has no battery or lid, so the alarm can't be activated here.", &["Quit", "Open Anyway"]) == 0 { return; }
    let app = App::new(mtm);
    let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
    let menu = NSMenu::new(mtm);
    let toggle = menu_item("Activate", sel!(toggle:), "", &app, mtm);
    menu.addItem(&toggle);
    menu.addItem(&NSMenuItem::separatorItem(mtm));
    menu.addItem(&menu_item(
        settings_name(),
        sel!(preferences:),
        ",",
        &app,
        mtm,
    ));
    menu.addItem(&NSMenuItem::separatorItem(mtm));
    for (title, action, key) in [
        ("About…", sel!(about:), ""),
        ("Check for Updates…", sel!(updates:), ""),
        ("Setup Guide…", sel!(guide:), ""),
        ("Help…", sel!(help:), ""),
        ("Quit", sel!(quit:), "q"),
    ] {
        menu.addItem(&menu_item(title, action, key, &app, mtm));
    }
    item.setMenu(Some(&menu));
    set_icon(&item, "menubar.pdf", true);
    let monitor_config = MonitorConfig {
        monitor_power: config["monitor_power"] == true,
        monitor_lid: config["monitor_lid"] == true,
        monitor_motion: config["monitor_motion"] == true,
        angle_threshold: config["angle_threshold"].as_f64().unwrap_or(5.0),
    };
    let (sender, events) = mpsc::channel();
    let monitor = Monitor::new(
        move |reason| {
            let _ = sender.send(reason.to_owned());
            trigger_alarm(reason);
        },
        Arc::new(RwLock::new(monitor_config)),
        sensors::MacSensors,
    );
    *app.ivars().0.borrow_mut() = Some(State {
        config,
        laptop,
        monitor,
        events,
        item,
        toggle,
    });
    application.setDelegate(Some(ProtocolObject::from_ref(&*app)));
    unsafe {
        NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
            1.0,
            &app,
            sel!(launch:),
            None,
            false,
        );
        NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
            10.0,
            &app,
            sel!(scheduleUpdates:),
            None,
            false,
        );
        NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
            0.1,
            &app,
            sel!(poll:),
            None,
            true,
        );
    }
    application.run();
}

fn open_preferences() {}
fn show_about() {}
fn check_updates() {}
fn schedule_updates() {}
fn open_guide() {}
fn show_help() {}
fn prepare_alarm() {}
fn start_alarm() {}
fn stop_alarm() {}
fn trigger_alarm(_reason: &str) {}
fn show_overlay(_app: &App, _alarm: bool, _flash: bool) {}
fn listen_overlay() {}
fn hide_overlay() {}
