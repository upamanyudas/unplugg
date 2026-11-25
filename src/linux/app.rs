//! Tray app for Linux. The menu stays glanceable; all settings live in Settings.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use gtk::gio::{self, prelude::*};
use gtk::glib;
use serde_json::{Map, Value};

use super::sensors::LinuxSensors;
use super::{auth, capture, dialogs, guide, location, overlay, power, sensors, siren, tray, ui, updater, utils};
use crate::live;
use crate::monitor::{Monitor, MonitorConfig, Sensors};

const NOT_LAPTOP: (&str, &str) = (
    "unplugg is made for laptops",
    "It guards the power cable and the lid. This computer has no battery, so the alarm can't be activated here.",
);

struct State {
    config: Map<String, Value>,
    // what the alarm thread and the monitor read
    alarm_config: Arc<RwLock<Map<String, Value>>>,
    monitor_config: Arc<RwLock<MonitorConfig>>,
    monitor: Monitor<LinuxSensors>,
    laptop: bool,
    tracking: Option<(location::Source, live::Tracker)>,
    app: gtk::Application,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|state| f(state.borrow_mut().as_mut().expect("started")))
}

pub fn config() -> Map<String, Value> {
    with(|state| state.config.clone())
}

pub fn update_config(key: &str, value: Value) {
    with(|state| {
        state.config.insert(key.into(), value);
        *state.alarm_config.write().unwrap() = state.config.clone();
        *state.monitor_config.write().unwrap() = monitor_config(&state.config);
        let _ = crate::config::save_config(&state.config);
    });
}

fn monitor_config(config: &Map<String, Value>) -> MonitorConfig {
    MonitorConfig {
        monitor_power: config["monitor_power"] == true,
        monitor_lid: config["monitor_lid"] == true,
        monitor_motion: config["monitor_motion"] == true,
        angle_threshold: config["angle_threshold"].as_f64().unwrap_or(5.0),
    }
}

pub fn run() {
    glib::set_prgname(Some("unplugg"));
    // one copy per user: a second launch (the app menu, say) opens Settings in the first, then quits
    let app = gtk::Application::new(Some("com.unplugg.app"), gio::ApplicationFlags::empty());
    app.connect_startup(start);
    static STARTED: AtomicBool = AtomicBool::new(false);
    app.connect_activate(|_| {
        if STARTED.swap(true, Ordering::SeqCst) {
            ui::open_settings();
        }
    });
    app.run_with_args::<&str>(&[]);
}

fn start(app: &gtk::Application) {
    // closing Settings leaves the tray icon
    std::mem::forget(app.hold());
    gtk::Window::set_default_icon_name("unplugg");
    if let Some(icon) = utils::app_icon() {
        gtk::Window::set_default_icon(&icon);
    }
    let mut config = crate::config::load_config().unwrap_or_else(|_| crate::config::defaults());
    // never boot already armed; release() undoes a crash while armed
    config.insert("activated".into(), false.into());
    power::release();
    let laptop = sensors::is_laptop();
    if !laptop && dialogs::alert(NOT_LAPTOP.0, NOT_LAPTOP.1, &["Quit", "Open Anyway"]) == 0 {
        app.quit();
        return;
    }
    let alarm_config = Arc::new(RwLock::new(config.clone()));
    let monitor_config = Arc::new(RwLock::new(monitor_config(&config)));
    let snapshot = alarm_config.clone();
    // the monitor's thread: the alarm thread can block, the rest happens on the main thread
    let monitor = Monitor::new(
        move |reason| {
            let config = snapshot.read().unwrap().clone();
            let reason = reason.to_owned();
            std::thread::spawn(move || alarm(&reason, config));
            glib::idle_add_once(show_alarm);
        },
        monitor_config.clone(),
        LinuxSensors,
    );
    STATE.with(|state| {
        *state.borrow_mut() = Some(State { config, alarm_config, monitor_config, monitor, laptop, tracking: None, app: app.clone() })
    });
    power::watch(|| with(|state| state.monitor.trip("shutdown")));
    tray::start("idle", |id| match id {
        1 => toggle(),
        3 => ui::open_settings(),
        5 => dialogs::show_about(),
        6 => updater::check(true),
        7 => guide::open(),
        8 => dialogs::show_help(),
        9 => quit(),
        _ => (),
    });
    // after launch, so the icon shows first
    glib::timeout_add_seconds_local_once(1, siren::prepare);
    if with(|state| state.config["setup_done"] != true) {
        glib::timeout_add_seconds_local_once(1, guide::open);
    }
    // once launch has settled
    glib::timeout_add_seconds_local_once(10, updater::schedule);
}

fn toggle() {
    if with(|state| state.config["activated"] != true) {
        activate();
    } else if auth::require("turn off the alarm", None) {
        deactivate();
    }
}

fn activate() {
    let (laptop, needs_power) = with(|state| (state.laptop, state.config["monitor_power"] == true));
    if !laptop {
        dialogs::alert(NOT_LAPTOP.0, NOT_LAPTOP.1, &[]);
        return;
    }
    if needs_power && !LinuxSensors.is_plugged_in() {
        dialogs::alert(
            "Plug in first",
            "Your laptop should be plugged in to power before you can activate the alarm. Alternatively, turn off \"Power unplugged\" in Settings.",
            &[],
        );
        return;
    }
    update_config("activated", true.into());
    power::hold();
    with(|state| state.monitor.start());
    tray::set_toggle("Deactivate");
    tray::set_state("armed");
    if with(|state| state.config["show_overlay"] == true) {
        overlay::show(Rc::new(unlock), false, false);
    }
}

fn deactivate() {
    update_config("activated", false.into());
    with(|state| state.monitor.stop());
    power::release();
    siren::stop();
    stop_tracking("the alarm was turned off", Duration::ZERO);
    overlay::hide();
    tray::set_toggle("Activate");
    tray::set_state("idle");
}

/// Someone touched the cover: owner check, then disarm or keep guarding.
fn unlock() {
    if !auth::require("unlock the screen", overlay::window().as_ref()) {
        // cancelled or a stranger: the cover stays
        overlay::listen();
        return;
    }
    overlay::hide();
    // it's the owner
    siren::stop();
    let choice = dialogs::alert("Deactivate the alarm?", "Keep Armed hides this cover; the alarm keeps watching.", &["Deactivate", "Keep Armed"]);
    if choice == 0 {
        deactivate();
    }
}

fn show_alarm() {
    let config = config();
    if config["activated"] != true {
        return;
    }
    if config["play_sound"] == true {
        siren::start(config["speaker_name"].as_str().unwrap_or(""));
    }
    tray::set_state("alarm");
    if config["show_overlay"] == true || config["flash_screen"] == true {
        overlay::show(Rc::new(unlock), true, config["flash_screen"] == true);
    }
    if config["live_location"] == true && with(|state| state.tracking.is_none()) {
        let tracker = live::Tracker::start(&Value::Object(config));
        let updates = tracker.clone();
        let source = location::Source::start(move |fix| updates.update(fix));
        with(|state| state.tracking = Some((source, tracker)));
    }
}

fn stop_tracking(why: &'static str, wait: Duration) {
    if let Some((source, tracker)) = with(|state| state.tracking.take()) {
        source.stop();
        tracker.stop(why, wait);
    }
}

/// Telegram always fires, even if the alarm is turned off while it captures.
fn alarm(reason: &str, config: Map<String, Value>) {
    let config = Value::Object(config);
    if config["lock_device"] == true {
        // first: capture and upload take ~20 s
        utils::lock_screen();
    }
    let (photo, video) = if config["capture_media"] == true { capture::capture(&config) } else { (None, None) };
    let link = if config["send_location"] == true { location::maps_link(Duration::from_secs(8)) } else { None };
    crate::telegram::send_alert(reason, &config, photo.as_deref(), video.as_deref(), link.as_deref());
}

/// Owner check if armed; then stop everything and quit.
pub fn quit() {
    if with(|state| state.config["activated"] == true) && !auth::require("quit while armed", None) {
        return;
    }
    with(|state| state.monitor.stop());
    power::release();
    siren::stop();
    // let telegram hear the live location stop before the app is gone
    stop_tracking("unplugg quit", Duration::from_secs(5));
    overlay::hide();
    with(|state| state.app.quit());
}
