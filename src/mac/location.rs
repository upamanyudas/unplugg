use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{define_class, msg_send, AnyThread};
use objc2_core_location::{CLLocation, CLLocationManager, CLLocationManagerDelegate};
use objc2_foundation::{NSArray, NSDate, NSError, NSObject, NSObjectProtocol, NSRunLoop};

use crate::live::Fix;

struct DelegateState {
    callback: Box<dyn Fn(Fix) + Send>,
    fresh_only: bool,
    failed: Box<dyn Fn() + Send>,
}

define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = AnyThread]
    #[ivars = DelegateState]
    struct LocationDelegate;

    unsafe impl NSObjectProtocol for LocationDelegate {}
    unsafe impl CLLocationManagerDelegate for LocationDelegate {
        #[unsafe(method(locationManager:didUpdateLocations:))]
        unsafe fn updated(&self, _manager: &CLLocationManager, locations: &NSArray<CLLocation>) {
            use objc2::DefinedClass;
            let Some(location) = locations.lastObject() else {
                if !self.ivars().fresh_only {
                    (self.ivars().failed)();
                }
                return;
            };
            let age = -location.timestamp().timeIntervalSinceNow();
            let accuracy = location.horizontalAccuracy();
            let coordinate = location.coordinate();
            if self.ivars().fresh_only
                && (!(0.0..=60.0).contains(&age)
                    || accuracy < 0.0
                    || !(-90.0..=90.0).contains(&coordinate.latitude)
                    || !(-180.0..=180.0).contains(&coordinate.longitude))
            {
                return;
            }
            (self.ivars().callback)(Fix {
                latitude: coordinate.latitude,
                longitude: coordinate.longitude,
                accuracy: Some(accuracy),
                observed: Instant::now()
                    - Duration::from_secs_f64(if age.is_finite() {
                        age.clamp(0.0, 60.0)
                    } else {
                        0.0
                    }),
            });
        }

        #[unsafe(method(locationManager:didFailWithError:))]
        unsafe fn failed(&self, _manager: &CLLocationManager, _error: &NSError) {
            use objc2::DefinedClass;
            (self.ivars().failed)();
        }
    }
);

pub struct Source {
    stop: Arc<AtomicBool>,
}

impl Source {
    pub fn start(callback: impl Fn(Fix) + Send + 'static) -> Self {
        Self::start_with_freshness(callback, true, || {})
    }

    fn start_with_freshness(
        callback: impl Fn(Fix) + Send + 'static,
        fresh_only: bool,
        failed: impl Fn() + Send + 'static,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let done = stop.clone();
        thread::spawn(move || {
            objc2::rc::autoreleasepool(|_| {
                let delegate: Retained<LocationDelegate> = unsafe {
                    msg_send![
                        super(LocationDelegate::alloc().set_ivars(DelegateState {
                            callback: Box::new(callback),
                            fresh_only,
                            failed: Box::new(failed)
                        })),
                        init
                    ]
                };
                let manager = unsafe { CLLocationManager::new() };
                unsafe {
                    manager.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
                    manager.setDesiredAccuracy(100.0);
                    let available: bool = msg_send![&*manager, respondsToSelector: objc2::sel!(requestWhenInUseAuthorization)];
                    if available {
                        manager.requestWhenInUseAuthorization();
                    }
                    manager.startUpdatingLocation();
                }
                while !done.load(Ordering::Relaxed) {
                    NSRunLoop::currentRunLoop()
                        .runUntilDate(&NSDate::dateWithTimeIntervalSinceNow(0.25));
                }
                unsafe { manager.stopUpdatingLocation() };
            });
        });
        Self { stop }
    }

    pub fn stop(self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

pub fn maps_link(timeout: Duration) -> Option<String> {
    let (send, receive) = mpsc::sync_channel(1);
    let failure = send.clone();
    let source = Source::start_with_freshness(
        move |fix| {
            let _ = send.try_send(Some(fix));
        },
        false,
        move || {
            let _ = failure.try_send(None);
        },
    );
    let fix = receive.recv_timeout(timeout).ok().flatten();
    source.stop();
    fix.map(|fix| {
        format!(
            "https://maps.google.com/?q={:.6},{:.6}",
            fix.latitude, fix.longitude
        )
    })
}
