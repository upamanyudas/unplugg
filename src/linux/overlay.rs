//! Full-screen "Protected by unplugg" cover on every display while armed.
//! The first key press, click, scroll or mouse move calls back into the app,
//! which asks for the owner. When the alarm fires it can also flash: plain white
//! at full brightness, so the webcam sees a lit face. Brightness goes through
//! logind (no root needed); only the built-in screen answers. Main thread only.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;

use super::utils;

const BACKLIGHT: &str = "/sys/class/backlight";

struct Cover {
    monitor: gdk::Monitor,
    window: gtk::Window,
    content: gtk::Box,
    dot: gtk::Label,
    badge: gtk::Label,
    white: Rc<Cell<bool>>,
}

#[derive(Default)]
struct Overlay {
    covers: Vec<Cover>,
    // set while covered
    on_input: Option<Rc<dyn Fn()>>,
    watching: bool,
    // what show() last asked for, for displays plugged in later
    alarm: bool,
    flashing: bool,
    // built-in display -> level before the flash
    brightness: HashMap<String, u32>,
    signals: Vec<glib::SignalHandlerId>,
    pulse: Option<glib::SourceId>,
}

thread_local! {
    static OVERLAY: RefCell<Overlay> = RefCell::new(Overlay::default());
}

fn label(text: &str, size: u32, color: &str, bold: bool) -> gtk::Label {
    let label = gtk::Label::new(None);
    let weight = if bold { 600 } else { 400 };
    label.set_markup(&format!("<span foreground=\"{color}\" size=\"{}\" weight=\"{weight}\">{}</span>", size * 1024, glib::markup_escape_text(text)));
    label
}

fn cover(monitor: &gdk::Monitor) -> Cover {
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_decorated(false);
    window.set_keep_above(true);
    // no taskbar button
    window.set_skip_taskbar_hint(true);
    window.set_skip_pager_hint(true);
    window.set_app_paintable(true);
    if let Some(visual) = WidgetExt::screen(&window).and_then(|screen| screen.rgba_visual()) {
        window.set_visual(Some(&visual));
    }
    let white = Rc::new(Cell::new(false));
    let lit = white.clone();
    window.connect_draw(move |_, cairo| {
        if lit.get() {
            cairo.set_source_rgb(1.0, 1.0, 1.0);
        } else {
            cairo.set_source_rgba(18.0 / 255.0, 18.0 / 255.0, 22.0 / 255.0, 235.0 / 255.0);
        }
        cairo.set_operator(gtk::cairo::Operator::Source);
        let _ = cairo.paint();
        glib::Propagation::Proceed
    });
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_valign(gtk::Align::Center);
    content.set_halign(gtk::Align::Center);
    if let Some(logo) = utils::app_icon().and_then(|icon| icon.scale_simple(160, 160, gtk::gdk_pixbuf::InterpType::Bilinear)) {
        content.pack_start(&gtk::Image::from_pixbuf(Some(&logo)), false, false, 0);
    }
    content.pack_start(&label("Protected by unplugg", 30, "white", true), false, false, 0);
    content.pack_start(&label("Press any key or move the mouse to unlock.", 14, "#bbbbbb", false), false, false, 0);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    row.set_halign(gtk::Align::Center);
    let (dot, badge) = (gtk::Label::new(None), gtk::Label::new(None));
    row.pack_start(&dot, false, false, 0);
    row.pack_start(&badge, false, false, 0);
    content.pack_start(&row, false, false, 0);
    window.add(&content);
    window.add_events(gdk::EventMask::POINTER_MOTION_MASK | gdk::EventMask::BUTTON_PRESS_MASK | gdk::EventMask::KEY_PRESS_MASK | gdk::EventMask::SCROLL_MASK);
    window.connect_key_press_event(|_, _| input());
    window.connect_button_press_event(|_, _| input());
    window.connect_motion_notify_event(|_, _| input());
    window.connect_scroll_event(|_, _| input());
    let index = monitor.display().and_then(|d| (0..d.n_monitors()).find(|i| d.monitor(*i).as_ref() == Some(monitor))).unwrap_or(0);
    let area = monitor.geometry();
    window.move_(area.x(), area.y());
    window.resize(area.width(), area.height());
    window.show_all();
    window.fullscreen_on_monitor(&WidgetExt::screen(&window).unwrap(), index);
    Cover { monitor: monitor.clone(), window, content, dot, badge, white }
}

fn input() -> glib::Propagation {
    let callback = OVERLAY.with(|overlay| {
        let mut overlay = overlay.borrow_mut();
        let watching = std::mem::replace(&mut overlay.watching, false);
        watching.then(|| overlay.on_input.clone()).flatten()
    });
    if let Some(callback) = callback {
        // after this event returns: the owner check blocks
        glib::idle_add_local_once(move || callback());
    }
    glib::Propagation::Stop
}

/// One cover per display, as displays come and go; each shows the alarm state.
fn sync() {
    let Some(display) = gdk::Display::default() else { return };
    let monitors: Vec<gdk::Monitor> = (0..display.n_monitors()).filter_map(|i| display.monitor(i)).collect();
    let (alarm, flashing, missing) = OVERLAY.with(|overlay| {
        let mut overlay = overlay.borrow_mut();
        let gone: Vec<Cover> = overlay.covers.extract_if(.., |c| !monitors.contains(&c.monitor)).collect();
        for cover in gone {
            unsafe { cover.window.destroy() };
        }
        let missing: Vec<gdk::Monitor> = monitors.iter().filter(|m| !overlay.covers.iter().any(|c| &c.monitor == *m)).cloned().collect();
        (overlay.alarm, overlay.flashing, missing)
    });
    let added: Vec<Cover> = missing.iter().map(cover).collect();
    OVERLAY.with(|overlay| {
        let mut overlay = overlay.borrow_mut();
        overlay.covers.extend(added);
        let (color, text) = if alarm { ("#ff3b30", "Alarm triggered") } else { ("#34c759", "Armed") };
        for cover in &overlay.covers {
            cover.dot.set_markup(&format!("<span foreground=\"{color}\" size=\"{}\">●</span>", 12 * 1024));
            cover.badge.set_markup(&format!("<span foreground=\"#bbbbbb\" size=\"{}\">{text}</span>", 12 * 1024));
            if flashing {
                cover.white.set(true);
                cover.content.hide();
                cover.window.queue_draw();
            }
        }
    });
}

/// The dot fades to a quarter and back every 2 s.
fn pulse() -> glib::SourceId {
    let start = Instant::now();
    glib::timeout_add_local(Duration::from_millis(50), move || {
        let phase = (start.elapsed().as_secs_f64() % 2.0) / 2.0;
        let opacity = 1.0 - 0.75 * (1.0 - (2.0 * phase - 1.0).abs());
        OVERLAY.with(|overlay| overlay.borrow().covers.iter().for_each(|cover| cover.dot.set_opacity(opacity)));
        glib::ControlFlow::Continue
    })
}

/// Covers every display, and any plugged in while covered; on later calls it just
/// flips the badge to the alarm state (and flashes). The first input after that
/// calls on_input() once.
pub fn show(on_input: Rc<dyn Fn()>, alarm: bool, flash: bool) {
    let first = OVERLAY.with(|overlay| {
        let mut overlay = overlay.borrow_mut();
        overlay.alarm = alarm;
        overlay.flashing |= flash;
        let first = overlay.on_input.is_none();
        if first {
            overlay.on_input = Some(on_input);
        }
        first
    });
    sync();
    if first {
        if let Some(display) = gdk::Display::default() {
            let added = display.connect_monitor_added(|_, _| sync());
            let removed = display.connect_monitor_removed(|_, _| sync());
            let pulse = pulse();
            OVERLAY.with(|overlay| {
                let mut overlay = overlay.borrow_mut();
                overlay.signals = vec![added, removed];
                overlay.pulse = Some(pulse);
                if let Some(cover) = overlay.covers.first() {
                    cover.window.present();
                }
            });
        }
        listen();
    }
    if flash {
        brighten();
    }
}

/// The first cover, for dialogs that must sit above it.
pub fn window() -> Option<gtk::Window> {
    OVERLAY.with(|overlay| overlay.borrow().covers.first().map(|cover| cover.window.clone()))
}

/// Watch for input after 1.5 s, so the hand that just clicked Activate or closed
/// the owner check doesn't set it off again.
pub fn listen() {
    glib::timeout_add_local_once(Duration::from_millis(1500), || {
        // covered, even if no display is right now
        OVERLAY.with(|overlay| {
            let mut overlay = overlay.borrow_mut();
            overlay.watching = overlay.on_input.is_some();
        });
    });
}

pub fn hide() {
    let overlay = OVERLAY.with(|overlay| std::mem::take(&mut *overlay.borrow_mut()));
    if let Some(display) = gdk::Display::default() {
        for signal in overlay.signals {
            display.disconnect(signal);
        }
    }
    if let Some(pulse) = overlay.pulse {
        pulse.remove();
    }
    if !overlay.brightness.is_empty() {
        set_levels(&overlay.brightness);
    }
    for cover in overlay.covers {
        unsafe { cover.window.destroy() };
    }
}

fn read_level(path: &Path) -> Option<u32> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// Built-in screen to full for the flash; sync() turns the covers white.
fn brighten() {
    let Ok(entries) = fs::read_dir(BACKLIGHT) else { return };
    let mut full = HashMap::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let (Some(level), Some(max)) = (read_level(&entry.path().join("brightness")), read_level(&entry.path().join("max_brightness"))) else { continue };
        OVERLAY.with(|overlay| {
            overlay.borrow_mut().brightness.entry(name.clone()).or_insert(level);
        });
        full.insert(name, max);
    }
    set_levels(&full);
}

fn set_levels(levels: &HashMap<String, u32>) {
    for (name, level) in levels {
        let _ = Command::new("busctl")
            .args(["call", "org.freedesktop.login1", "/org/freedesktop/login1/session/auto", "org.freedesktop.login1.Session", "SetBrightness", "ssu", "backlight", name, &level.to_string()])
            .output();
    }
}
