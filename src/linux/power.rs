//! Power policy while armed, through systemd-logind's inhibitor locks: sleep,
//! idle and the lid and power keys are blocked; shutdown is delayed, and its
//! PrepareForShutdown signal fires the alarm.

use std::cell::Cell;
use std::process::{Child, Command};
use std::sync::Mutex;

use gtk::gio;
use gtk::glib::Variant;

static INHIBITORS: Mutex<Vec<Child>> = Mutex::new(Vec::new());

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
}

/// Listens for shutdown for as long as the app runs.
pub fn watch(on_attempt: impl Fn() + 'static) {
    let Ok(bus) = gio::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE) else { return };
    let subscription = bus.signal_subscribe(
        Some("org.freedesktop.login1"),
        Some("org.freedesktop.login1.Manager"),
        Some("PrepareForShutdown"),
        Some("/org/freedesktop/login1"),
        None,
        gio::DBusSignalFlags::NONE,
        move |_, _, _, _, _, args: &Variant| {
            let starting = args.child_value(0).get::<bool>().unwrap_or(false);
            if starting && ARMED.with(Cell::get) {
                on_attempt();
            }
        },
    );
    // the subscription lasts as long as the bus connection, which is the app's
    std::mem::forget((bus, subscription));
}

/// Armed: stay awake, ignore the lid and buttons, delay shutdown.
pub fn hold() {
    ARMED.with(|armed| armed.set(true));
    // tail exits with this process, so even a crash can't leave the laptop sleepless
    let until_we_exit = ["tail".to_owned(), format!("--pid={}", std::process::id()), "-f".into(), "/dev/null".into()];
    let locks = [
        ("sleep:idle:handle-lid-switch:handle-power-key:handle-suspend-key:handle-hibernate-key", "block"),
        ("shutdown", "delay"),
    ];
    let mut inhibitors = INHIBITORS.lock().unwrap();
    for (what, mode) in locks {
        let child = Command::new("systemd-inhibit")
            .args([&format!("--what={what}"), "--who=unplugg", "--why=The alarm is armed", &format!("--mode={mode}")])
            .args(&until_we_exit)
            .spawn();
        match child {
            Ok(child) => inhibitors.push(child),
            Err(_) => return, // no systemd
        }
    }
}

/// Disarmed, or quitting: put everything back.
pub fn release() {
    ARMED.with(|armed| armed.set(false));
    for mut inhibitor in INHIBITORS.lock().unwrap().drain(..) {
        let _ = inhibitor.kill();
        let _ = inhibitor.wait();
    }
}
