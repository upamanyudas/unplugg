use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use windows::Devices::Geolocation::{Geolocator, Geoposition, PositionChangedEventArgs};
use windows::Foundation::{TimeSpan, TypedEventHandler};
use windows::Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED};

use crate::live::Fix;

fn fix(position: &Geoposition) -> Option<Fix> {
    let coordinate = position.Coordinate().ok()?;
    let point = coordinate.Point().ok()?.Position().ok()?;
    let accuracy = coordinate.Accuracy().ok()?;
    let timestamp = coordinate.Timestamp().ok()?.UniversalTime;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?;
    let now_ticks = i128::try_from(now.as_nanos() / 100).ok()? + 116_444_736_000_000_000;
    let age_ticks = now_ticks - i128::from(timestamp);
    if !(0..=600_000_000).contains(&age_ticks)
        || !accuracy.is_finite()
        || accuracy < 0.0
        || !(-90.0..=90.0).contains(&point.Latitude)
        || !(-180.0..=180.0).contains(&point.Longitude)
    {
        return None;
    }
    let age = Duration::from_nanos((age_ticks.clamp(0, 600_000_000) as u64) * 100);
    Some(Fix {
        latitude: point.Latitude,
        longitude: point.Longitude,
        accuracy: Some(accuracy),
        observed: Instant::now().checked_sub(age).unwrap_or_else(Instant::now),
    })
}

pub struct Source {
    stop: Arc<AtomicBool>,
}

impl Source {
    pub fn start(callback: impl Fn(Fix) + Send + 'static) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let done = stop.clone();
        thread::spawn(move || {
            if unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.is_err() {
                return;
            }
            if let Ok(locator) = Geolocator::new() {
                let handler = TypedEventHandler::<Geolocator, PositionChangedEventArgs>::new(
                    move |_, args| {
                        if let Some(fix) = args
                            .as_ref()
                            .and_then(|args| args.Position().ok())
                            .and_then(|position| fix(&position))
                        {
                            callback(fix);
                        }
                        Ok(())
                    },
                );
                if let Ok(token) = locator.PositionChanged(&handler) {
                    while !done.load(Ordering::Relaxed) {
                        thread::sleep(Duration::from_millis(250));
                    }
                    let _ = locator.RemovePositionChanged(token);
                }
            }
            unsafe { RoUninitialize() };
        });
        Self { stop }
    }

    pub fn stop(self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

pub fn maps_link(timeout: Duration) -> Option<String> {
    if unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.is_err() {
        return None;
    }
    let result = (|| {
        let locator = Geolocator::new().ok()?;
        let ticks = i64::try_from(timeout.as_nanos() / 100).ok()?;
        let operation = locator
            .GetGeopositionAsyncWithAgeAndTimeout(
                TimeSpan { Duration: 0 },
                TimeSpan { Duration: ticks },
            )
            .ok()?;
        let deadline = Instant::now() + timeout;
        while operation.Status().ok()?.0 == 0 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(50));
        }
        if operation.Status().ok()?.0 != 1 {
            let _ = operation.Cancel();
            return None;
        }
        let position = operation
            .GetResults()
            .ok()?
            .Coordinate()
            .ok()?
            .Point()
            .ok()?
            .Position()
            .ok()?;
        if !(-90.0..=90.0).contains(&position.Latitude)
            || !(-180.0..=180.0).contains(&position.Longitude)
        {
            return None;
        }
        Some(format!(
            "https://maps.google.com/?q={:.6},{:.6}",
            position.Latitude, position.Longitude
        ))
    })();
    unsafe { RoUninitialize() };
    result
}
