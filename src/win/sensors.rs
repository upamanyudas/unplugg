use std::sync::atomic::{AtomicBool, Ordering};

use windows::Devices::Sensors::Accelerometer;
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED};

use crate::monitor::Sensors;

static LID_CLOSED: AtomicBool = AtomicBool::new(false);

pub fn set_lid_closed(closed: bool) {
    LID_CLOSED.store(closed, Ordering::Relaxed);
}

fn power_status() -> Option<SYSTEM_POWER_STATUS> {
    let mut status = SYSTEM_POWER_STATUS::default();
    unsafe { GetSystemPowerStatus(&mut status).ok()? };
    Some(status)
}

pub fn is_laptop() -> bool {
    power_status().is_some_and(|status| status.BatteryFlag & 128 == 0)
}

pub fn has_angle_sensor() -> bool {
    false
}

pub fn acceleration() -> Option<[f64; 3]> {
    let initialized = unsafe { RoInitialize(RO_INIT_MULTITHREADED).is_ok() };
    let reading = (|| {
        let reading = Accelerometer::GetDefault().ok()?.GetCurrentReading().ok()?;
        Some([
            reading.AccelerationX().ok()?,
            reading.AccelerationY().ok()?,
            reading.AccelerationZ().ok()?,
        ])
    })();
    if initialized {
        unsafe { RoUninitialize() };
    }
    reading
}

pub struct WinSensors;

impl Sensors for WinSensors {
    fn is_plugged_in(&self) -> bool {
        power_status()
            .is_none_or(|status| status.BatteryFlag & 128 != 0 || status.ACLineStatus == 1)
    }

    fn is_lid_closed(&self) -> bool {
        LID_CLOSED.load(Ordering::Relaxed)
    }

    fn lid_angle(&self) -> Option<f64> {
        None
    }

    fn acceleration(&self) -> Option<[f64; 3]> {
        acceleration()
    }
}
