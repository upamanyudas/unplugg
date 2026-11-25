use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

#[derive(Clone, Copy, Debug)]
pub struct Fix {
    pub latitude: f64,
    pub longitude: f64,
    pub accuracy: Option<f64>,
    pub observed: Instant,
}

impl Fix {
    fn changed(self, other: Self) -> bool {
        self.latitude != other.latitude || self.longitude != other.longitude
    }
}

struct State {
    message: Option<i64>,
    last_fix: Option<Fix>,
    last_sent: Option<Instant>,
    retry_at: Option<Instant>,
}

impl State {
    fn new() -> Self {
        Self {
            message: None,
            last_fix: None,
            last_sent: None,
            retry_at: None,
        }
    }

    fn ready(&self, fix: Fix, now: Instant) -> bool {
        now.saturating_duration_since(fix.observed) <= Duration::from_secs(60)
            && self.retry_at.is_none_or(|time| now >= time)
            && self
                .last_sent
                .is_none_or(|time| now.saturating_duration_since(time) >= Duration::from_secs(30))
            && self.last_fix.is_none_or(|last| fix.changed(last))
    }

    fn sent(&mut self, fix: Fix, now: Instant, message: i64) {
        self.message = Some(message);
        self.last_fix = Some(fix);
        self.last_sent = Some(now);
        self.retry_at = None;
    }

    fn failed(&mut self, now: Instant, failure: Failure) {
        let wait = match failure {
            Failure::Retry(seconds) => seconds.max(1),
            Failure::Rejected => {
                // the live message is gone or expired, so the next fix starts a new one
                self.message = None;
                5
            }
            Failure::Network => 5,
        };
        self.retry_at = Some(now + Duration::from_secs(wait));
    }
}

#[derive(Clone)]
struct Destination {
    token: String,
    chat: Value,
}

enum Event {
    Fix(Fix),
    Stop,
}

enum Failure {
    Network,
    Retry(u64),
    Rejected,
}

#[derive(Clone)]
pub struct Tracker {
    workers: Vec<Sender<Event>>,
}

impl Tracker {
    pub fn start(config: &Value) -> Self {
        let destinations = config
            .get("telegram_destinations")
            .and_then(Value::as_array);
        let workers = destinations
            .into_iter()
            .flatten()
            .filter_map(|destination| {
                let token = destination.get("token")?.as_str()?.to_owned();
                let chat = destination.get("chat_id")?.clone();
                let (send, receive) = mpsc::channel();
                thread::spawn(move || worker(Destination { token, chat }, receive, call));
                Some(send)
            })
            .collect();
        Self { workers }
    }

    pub fn update(&self, fix: Fix) {
        for worker in &self.workers {
            let _ = worker.send(Event::Fix(fix));
        }
    }

    pub fn stop(self) {
        for worker in self.workers {
            let _ = worker.send(Event::Stop);
        }
    }
}

fn worker(
    destination: Destination,
    receive: Receiver<Event>,
    mut send: impl FnMut(&Destination, &str, Value) -> Result<Value, Failure>,
) {
    let mut state = State::new();
    let mut latest = None;
    loop {
        match receive.recv_timeout(Duration::from_secs(1)) {
            Ok(Event::Fix(fix)) => latest = Some(fix),
            Ok(Event::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                stop_message(&destination, &state, &mut send);
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => (),
        }
        while let Ok(event) = receive.try_recv() {
            match event {
                Event::Fix(fix) => latest = Some(fix),
                Event::Stop => {
                    stop_message(&destination, &state, &mut send);
                    return;
                }
            }
        }
        let now = Instant::now();
        if let Some(fix) = latest.filter(|fix| state.ready(*fix, now)) {
            let mut body = json!({"chat_id": destination.chat, "latitude": fix.latitude, "longitude": fix.longitude});
            if let Some(accuracy) = fix.accuracy.filter(|a| (0.0..=1500.0).contains(a)) {
                body["horizontal_accuracy"] = json!(accuracy);
            }
            let method = if let Some(message) = state.message {
                body["message_id"] = json!(message);
                "editMessageLiveLocation"
            } else {
                body["live_period"] = json!(2147483647);
                "sendLocation"
            };
            match send(&destination, method, body) {
                Ok(result) => {
                    if let Some(message) = state
                        .message
                        .or_else(|| result.get("message_id").and_then(Value::as_i64))
                    {
                        state.sent(fix, Instant::now(), message);
                    } else {
                        state.failed(Instant::now(), Failure::Network);
                    }
                }
                Err(failure) => state.failed(Instant::now(), failure),
            }
        }
    }
}

fn stop_message(
    destination: &Destination,
    state: &State,
    send: &mut impl FnMut(&Destination, &str, Value) -> Result<Value, Failure>,
) {
    let Some(message) = state.message else { return };
    if let Some(retry_at) = state.retry_at {
        thread::sleep(retry_at.saturating_duration_since(Instant::now()));
    }
    for _ in 0..3 {
        match send(
            destination,
            "stopMessageLiveLocation",
            json!({"chat_id": destination.chat, "message_id": message}),
        ) {
            Err(Failure::Retry(delay)) => thread::sleep(Duration::from_secs(delay.max(1))),
            _ => break,
        }
    }
}

fn call(destination: &Destination, method: &str, body: Value) -> Result<Value, Failure> {
    let connector = native_tls::TlsConnector::new().map_err(|_| Failure::Network)?;
    let agent = ureq::AgentBuilder::new()
        .tls_connector(std::sync::Arc::new(connector))
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(10))
        .timeout_write(Duration::from_secs(10))
        .build();
    let url = format!("https://api.telegram.org/bot{}/{method}", destination.token);
    let response = match agent
        .post(&url)
        .set("Content-Type", "application/json")
        .send_string(&body.to_string())
    {
        Ok(response) | Err(ureq::Error::Status(_, response)) => response,
        Err(_) => return Err(Failure::Network),
    };
    let value: Value =
        serde_json::from_reader(response.into_reader()).map_err(|_| Failure::Network)?;
    if value.get("ok").and_then(Value::as_bool) == Some(true) {
        return Ok(value.get("result").cloned().unwrap_or(Value::Null));
    }
    match value
        .get("parameters")
        .and_then(|p| p.get("retry_after"))
        .and_then(Value::as_u64)
    {
        Some(seconds) => Err(Failure::Retry(seconds)),
        None => Err(Failure::Rejected),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn throttles_changes_and_respects_retry() {
        let start = Instant::now();
        let a = Fix {
            latitude: 1.0,
            longitude: 2.0,
            accuracy: None,
            observed: start,
        };
        let b = Fix { latitude: 3.0, ..a };
        let mut state = State::new();
        assert!(state.ready(a, start));
        state.sent(a, start, 4);
        assert!(!state.ready(a, start + Duration::from_secs(30)));
        assert!(!state.ready(b, start + Duration::from_secs(29)));
        assert!(state.ready(b, start + Duration::from_secs(30)));
        state.failed(start + Duration::from_secs(30), Failure::Retry(20));
        assert!(!state.ready(b, start + Duration::from_secs(49)));
        assert!(state.ready(b, start + Duration::from_secs(50)));
        assert!(!state.ready(b, start + Duration::from_secs(61)));
    }

    #[test]
    fn rejected_edit_starts_a_new_message() {
        let start = Instant::now();
        let mut state = State::new();
        let a = Fix {
            latitude: 1.0,
            longitude: 2.0,
            accuracy: None,
            observed: start,
        };
        state.sent(a, start, 4);
        state.failed(start, Failure::Rejected);
        assert_eq!(state.message, None);
    }

    #[test]
    fn stop_reaches_active_message_once() {
        let (events, receive) = mpsc::channel();
        let (calls, results) = mpsc::channel();
        let worker = thread::spawn(move || {
            worker(
                Destination {
                    token: String::new(),
                    chat: json!(12),
                },
                receive,
                |_, method, body| {
                    calls.send((method.to_owned(), body)).unwrap();
                    Ok(json!({"message_id": 9}))
                },
            );
        });
        events
            .send(Event::Fix(Fix {
                latitude: 1.0,
                longitude: 2.0,
                accuracy: None,
                observed: Instant::now(),
            }))
            .unwrap();
        let (method, body) = results.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(method, "sendLocation");
        assert_eq!(body["live_period"], 2147483647);
        events.send(Event::Stop).unwrap();
        let (method, body) = results.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(method, "stopMessageLiveLocation");
        assert_eq!(body["message_id"], 9);
        worker.join().unwrap();
        assert!(results.try_recv().is_err());
    }
}
