//! Location from GeoClue2 over the system D-Bus: a one-shot maps link for the
//! alert, and a continuous source for live tracking. GeoClue finds the laptop by
//! Wi-Fi, so it needs the desktop's location switch on.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gtk::gio;
use gtk::glib::{ToVariant, Variant};

use crate::live::Fix;

const SERVICE: &str = "org.freedesktop.GeoClue2";
const CLIENT: &str = "org.freedesktop.GeoClue2.Client";

fn call(bus: &gio::DBusConnection, path: &str, interface: &str, method: &str, args: Option<&Variant>) -> Option<Variant> {
    bus.call_sync(Some(SERVICE), path, interface, method, args, None, gio::DBusCallFlags::NONE, 5000, gio::Cancellable::NONE).ok()
}

fn property(bus: &gio::DBusConnection, path: &str, interface: &str, name: &str) -> Option<Variant> {
    call(bus, path, "org.freedesktop.DBus.Properties", "Get", Some(&(interface, name).to_variant())).map(|reply| reply.child_value(0).as_variant().unwrap_or(reply))
}

/// A started client of our own; CreateClient, so the alert's lookup and live tracking don't share one.
struct Client {
    bus: gio::DBusConnection,
    path: String,
}

impl Client {
    fn start() -> Option<Self> {
        let bus = gio::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE).ok()?;
        let manager = "/org/freedesktop/GeoClue2/Manager";
        let reply = call(&bus, manager, "org.freedesktop.GeoClue2.Manager", "CreateClient", None)
            .or_else(|| call(&bus, manager, "org.freedesktop.GeoClue2.Manager", "GetClient", None))?;
        let path = reply.child_value(0).str()?.to_owned();
        let set = |name: &str, value: Variant| call(&bus, &path, "org.freedesktop.DBus.Properties", "Set", Some(&(CLIENT, name, value).to_variant()));
        // GeoClue asks the desktop about unplugg by its .desktop file
        set("DesktopId", "unplugg".to_variant())?;
        set("RequestedAccuracyLevel", 8u32.to_variant())?; // EXACT
        call(&bus, &path, CLIENT, "Start", None)?;
        Some(Self { bus, path })
    }

    /// The newest fix, and the object it came from, once GeoClue has one.
    fn fix(&self) -> Option<(String, Fix)> {
        let location = property(&self.bus, &self.path, CLIENT, "Location")?.str()?.to_owned();
        if location == "/" {
            return None;
        }
        let read = |name: &str| property(&self.bus, &location, "org.freedesktop.GeoClue2.Location", name)?.get::<f64>();
        let fix = Fix { latitude: read("Latitude")?, longitude: read("Longitude")?, accuracy: read("Accuracy"), observed: Instant::now() };
        Some((location, fix))
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        call(&self.bus, &self.path, CLIENT, "Stop", None);
        let manager = "/org/freedesktop/GeoClue2/Manager";
        let path = gtk::glib::variant::ObjectPath::try_from(self.path.as_str()).ok();
        if let Some(path) = path {
            call(&self.bus, manager, "org.freedesktop.GeoClue2.Manager", "DeleteClient", Some(&(path,).to_variant()));
        }
    }
}

/// A Google Maps link for the laptop's location, or None within `timeout`.
pub fn maps_link(timeout: Duration) -> Option<String> {
    let client = Client::start()?;
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Some((_, fix)) = client.fix() {
            return Some(format!("https://maps.google.com/?q={:.6},{:.6}", fix.latitude, fix.longitude));
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    None
}

/// Fixes as they change, for live tracking, until stop().
pub struct Source {
    stop: Arc<AtomicBool>,
}

impl Source {
    pub fn start(on_fix: impl Fn(Fix) + Send + 'static) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        std::thread::spawn(move || {
            let Some(client) = Client::start() else { return };
            let mut last = String::new();
            while !stopped.load(Ordering::Relaxed) {
                // GeoClue makes a new Location object for each new fix
                if let Some((location, fix)) = client.fix().filter(|(location, _)| *location != last) {
                    last = location;
                    on_fix(fix);
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        });
        Self { stop }
    }

    pub fn stop(self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn property_set_is_ssv() {
        assert_eq!((CLIENT, "DesktopId", "unplugg".to_variant()).to_variant().type_().as_str(), "(ssv)");
    }
}
