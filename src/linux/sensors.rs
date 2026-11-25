//! Laptop or not, AC power, lid and motion, from the kernel's sysfs and procfs.
//! There's no lid angle on Linux, so the lid counts as closed or open, like on
//! older Macs. Motion comes from the accelerometer most 2-in-1s carry.

use std::fs;
use std::path::{Path, PathBuf};

use crate::monitor::Sensors;

const G: f64 = 9.80665; // m/s² per g
const SUPPLIES: &str = "/sys/class/power_supply";

fn read(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|text| text.trim().to_owned())
}

/// (type, scope, online, status) of every power supply.
fn supplies() -> Vec<[Option<String>; 4]> {
    let Ok(entries) = fs::read_dir(SUPPLIES) else { return Vec::new() };
    entries
        .flatten()
        .map(|entry| ["type", "scope", "online", "status"].map(|name| read(&entry.path().join(name))))
        .collect()
}

/// The laptop's own batteries, not a mouse's or a headset's.
fn batteries(all: &[[Option<String>; 4]]) -> Vec<&[Option<String>; 4]> {
    all.iter().filter(|s| s[0].as_deref() == Some("Battery") && s[1].as_deref() != Some("Device")).collect()
}

/// Laptops have a battery, desktops don't.
pub fn is_laptop() -> bool {
    !batteries(&supplies()).is_empty()
}

fn accelerometer() -> Option<PathBuf> {
    fs::read_dir("/sys/bus/iio/devices").ok()?.flatten().map(|entry| entry.path()).find(|path| path.join("in_accel_x_raw").exists())
}

/// (x, y, z) in g, or None without an accelerometer. The IIO files are readable by everyone.
pub fn acceleration() -> Option<[f64; 3]> {
    let device = accelerometer()?;
    let value = |name: &str| read(&device.join(name)).and_then(|text| text.parse::<f64>().ok());
    let scale = value("in_accel_scale").or_else(|| value("in_accel_x_scale")).unwrap_or(1.0);
    Some(["x", "y", "z"].map(|axis| value(&format!("in_accel_{axis}_raw")).unwrap_or(0.0) * scale / G))
}

pub struct LinuxSensors;

impl Sensors for LinuxSensors {
    fn is_plugged_in(&self) -> bool {
        let all = supplies();
        if batteries(&all).is_empty() {
            return true; // a desktop is always on power
        }
        let mains: Vec<_> = all.iter().filter(|s| s[0].as_deref() == Some("Mains")).collect();
        if !mains.is_empty() {
            return mains.iter().any(|s| s[2].as_deref() == Some("1"));
        }
        // no adapter listed: the batteries say whether they're draining
        batteries(&all).iter().all(|s| s[3].as_deref() != Some("Discharging"))
    }

    fn is_lid_closed(&self) -> bool {
        let Ok(lids) = fs::read_dir("/proc/acpi/button/lid") else { return false };
        lids.flatten().any(|lid| read(&lid.path().join("state")).is_some_and(|state| state.contains("closed")))
    }

    fn lid_angle(&self) -> Option<f64> {
        None
    }

    fn acceleration(&self) -> Option<[f64; 3]> {
        acceleration()
    }
}
