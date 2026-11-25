use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, RwLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub const POLL: Duration = Duration::from_millis(200);
pub const MOVE: f64 = 0.1;

pub trait Sensors: Send + Sync + 'static {
    fn is_plugged_in(&self) -> bool;
    fn is_lid_closed(&self) -> bool;
    fn lid_angle(&self) -> Option<f64>;
    fn acceleration(&self) -> Option<[f64; 3]>;
}

#[derive(Clone, Copy)]
pub struct MonitorConfig {
    pub monitor_power: bool,
    pub monitor_lid: bool,
    pub monitor_motion: bool,
    pub angle_threshold: f64,
}

struct State<S: Sensors> {
    sensors: S,
    config: Arc<RwLock<MonitorConfig>>,
    on_unsafe: Box<dyn Fn(&str) + Send + Sync>,
    running: AtomicBool,
    fired: AtomicBool,
    baseline: RwLock<(Option<f64>, Option<[f64; 3]>)>,
}

impl<S: Sensors> State<S> {
    fn fire(&self, reason: &str) {
        if !self.fired.swap(true, Ordering::SeqCst) {
            (self.on_unsafe)(reason);
        }
    }

    fn unsafe_reason(&self) -> Option<&'static str> {
        let cfg = *self.config.read().unwrap();
        let (angle, motion) = *self.baseline.read().unwrap();
        if cfg.monitor_power && !self.sensors.is_plugged_in() {
            return Some("power_unplugged");
        }
        if cfg.monitor_lid {
            if let Some(baseline) = angle {
                if self
                    .sensors
                    .lid_angle()
                    .is_some_and(|now| (now - baseline).abs() > cfg.angle_threshold)
                {
                    return Some("screen_moved");
                }
            } else if self.sensors.is_lid_closed() {
                return Some("screen_closed");
            }
        }
        if cfg.monitor_motion && motion.is_some() {
            if let (Some(baseline), Some(now)) = (motion, self.sensors.acceleration()) {
                let distance = baseline
                    .iter()
                    .zip(now)
                    .map(|(a, b)| (a - b).powi(2))
                    .sum::<f64>()
                    .sqrt();
                if distance > MOVE {
                    return Some("device_moved");
                }
            }
        }
        None
    }
}

pub struct Monitor<S: Sensors> {
    state: Arc<State<S>>,
    thread: Option<(JoinHandle<()>, mpsc::Receiver<()>)>,
}

impl<S: Sensors> Monitor<S> {
    pub fn new(
        on_unsafe: impl Fn(&str) + Send + Sync + 'static,
        config: Arc<RwLock<MonitorConfig>>,
        sensors: S,
    ) -> Self {
        Self {
            state: Arc::new(State {
                sensors,
                config,
                on_unsafe: Box::new(on_unsafe),
                running: AtomicBool::new(false),
                fired: AtomicBool::new(false),
                baseline: RwLock::new((None, None)),
            }),
            thread: None,
        }
    }

    pub fn start(&mut self) {
        if self.state.running.load(Ordering::SeqCst) {
            return;
        }
        self.state.fired.store(false, Ordering::SeqCst);
        *self.state.baseline.write().unwrap() = (
            self.state.sensors.lid_angle(),
            self.state.sensors.acceleration(),
        );
        self.state.running.store(true, Ordering::SeqCst);
        let state = Arc::clone(&self.state);
        let (done, finished) = mpsc::channel();
        let handle = thread::spawn(move || {
            while state.running.load(Ordering::SeqCst) {
                if !state.fired.load(Ordering::SeqCst) {
                    if let Some(reason) = state.unsafe_reason() {
                        state.fire(reason);
                    }
                }
                thread::sleep(POLL);
            }
            let _ = done.send(());
        });
        self.thread = Some((handle, finished));
    }

    pub fn stop(&mut self) {
        self.state.running.store(false, Ordering::SeqCst);
        if let Some((handle, finished)) = self.thread.take() {
            if finished.recv_timeout(Duration::from_secs(2)).is_ok() {
                let _ = handle.join();
            }
        }
    }

    pub fn trip(&self, reason: &str) {
        if self.state.running.load(Ordering::SeqCst) {
            self.state.fire(reason);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Clone)]
    struct Fake(Arc<Mutex<(bool, bool, Option<f64>, Option<[f64; 3]>)>>);

    impl Sensors for Fake {
        fn is_plugged_in(&self) -> bool {
            self.0.lock().unwrap().0
        }
        fn is_lid_closed(&self) -> bool {
            self.0.lock().unwrap().1
        }
        fn lid_angle(&self) -> Option<f64> {
            self.0.lock().unwrap().2
        }
        fn acceleration(&self) -> Option<[f64; 3]> {
            self.0.lock().unwrap().3
        }
    }

    fn setup() -> (Monitor<Fake>, Fake, Arc<Mutex<Vec<String>>>) {
        let sensors = Fake(Arc::new(Mutex::new((
            true,
            false,
            Some(90.0),
            Some([0.0; 3]),
        ))));
        let events = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&events);
        let config = Arc::new(RwLock::new(MonitorConfig {
            monitor_power: true,
            monitor_lid: true,
            monitor_motion: true,
            angle_threshold: 5.0,
        }));
        let monitor = Monitor::new(
            move |reason| captured.lock().unwrap().push(reason.to_owned()),
            config,
            sensors.clone(),
        );
        (monitor, sensors, events)
    }

    #[test]
    fn policy_respects_baselines_thresholds_and_priority() {
        let (monitor, sensors, _) = setup();
        *monitor.state.baseline.write().unwrap() = (Some(90.0), Some([0.0; 3]));
        {
            let mut reading = sensors.0.lock().unwrap();
            reading.2 = Some(95.0);
            reading.3 = Some([0.1, 0.0, 0.0]);
        }
        assert_eq!(monitor.state.unsafe_reason(), None);
        sensors.0.lock().unwrap().3 = Some([0.11, 0.0, 0.0]);
        assert_eq!(monitor.state.unsafe_reason(), Some("device_moved"));
        sensors.0.lock().unwrap().2 = Some(96.0);
        assert_eq!(monitor.state.unsafe_reason(), Some("screen_moved"));
        sensors.0.lock().unwrap().0 = false;
        assert_eq!(monitor.state.unsafe_reason(), Some("power_unplugged"));
        sensors.0.lock().unwrap().0 = true;
        *monitor.state.baseline.write().unwrap() = (None, None);
        sensors.0.lock().unwrap().1 = true;
        assert_eq!(monitor.state.unsafe_reason(), Some("screen_closed"));
        monitor.state.config.write().unwrap().monitor_lid = false;
        assert_eq!(monitor.state.unsafe_reason(), None);
    }

    #[test]
    fn trip_fires_once_per_activation() {
        let (mut monitor, _, events) = setup();
        monitor.trip("shutdown");
        monitor.start();
        monitor.trip("shutdown");
        monitor.trip("again");
        monitor.stop();
        monitor.start();
        monitor.trip("again");
        monitor.stop();
        assert_eq!(*events.lock().unwrap(), ["shutdown", "again"]);
    }
}
