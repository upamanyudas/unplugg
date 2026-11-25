use std::time::Duration;

use crate::live::Fix;

pub fn maps_link(_timeout: Duration) -> Option<String> {
    None
}

pub struct Source;

impl Source {
    pub fn start(_on_fix: impl Fn(Fix) + Send + 'static) -> Self {
        Source
    }
    pub fn stop(self) {}
}
