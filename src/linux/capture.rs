use std::path::PathBuf;

use serde_json::Value;

pub fn capture(_config: &Value) -> (Option<PathBuf>, Option<PathBuf>) {
    (None, None)
}
