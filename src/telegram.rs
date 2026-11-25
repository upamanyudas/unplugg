use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;

pub const TOKEN_HELP: (&str, &str) = (
    "How to get a bot token",
    "https://core.telegram.org/bots/features#botfather",
);
pub const LOUD_HELP: &str = concat!(
    "Telegram bots can't force a loud alert. Set this up once on your phone.\n\n",
    "iPhone\n",
    "• Settings › Focus › each Focus › Apps › add Telegram.\n",
    "• Telegram › Settings › Notifications and Sounds: give your bot's chat a loud sound.\n",
    "• Silent mode on? Only vibration gets through: ",
    "Settings › Sounds & Haptics › Haptics › Always Play.\n\n",
    "Android (menu names vary by phone)\n",
    "• When the first alert arrives, long-press it › Priority.\n",
    "• Settings › Do Not Disturb › People › Conversations › Priority conversations."
);
pub const EVENT_LABEL: &[(&str, &str)] = &[
    ("power_unplugged", "Power adapter unplugged"),
    ("screen_moved", "Screen moved"),
    ("screen_closed", "Screen closed"),
    ("device_moved", "Device moved"),
    ("shutdown", "Shutdown or sleep attempted"),
];

fn agent(timeout: u64) -> Result<ureq::Agent, native_tls::Error> {
    Ok(ureq::AgentBuilder::new()
        .tls_connector(Arc::new(native_tls::TlsConnector::new()?))
        .timeout_connect(Duration::from_secs(timeout))
        .timeout_read(Duration::from_secs(timeout))
        .timeout_write(Duration::from_secs(timeout))
        .build())
}

pub fn send_alert(
    event: &str,
    config: &Value,
    photo: Option<&Path>,
    video: Option<&Path>,
    location_url: Option<&str>,
) {
    send_alert_with_document(event, config, photo, video, location_url, None);
}

pub fn send_alert_with_document(
    event: &str,
    config: &Value,
    photo: Option<&Path>,
    video: Option<&Path>,
    location_url: Option<&str>,
    document: Option<&Path>,
) {
    let Some(destinations) = config
        .get("telegram_destinations")
        .and_then(Value::as_array)
    else {
        return;
    };
    if destinations.is_empty() {
        return;
    }
    let label = EVENT_LABEL
        .iter()
        .find(|(key, _)| *key == event)
        .map_or(event, |(_, label)| label);
    let mut text = format!(
        "🚨 unplugg alert: {}\n🕒 {}",
        label,
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    );
    if let Some(url) = location_url.filter(|url| !url.is_empty()) {
        text.push_str(&format!("\n📍 {url}"));
    }
    for destination in destinations {
        let (Some(token), Some(chat)) = (
            destination.get("token").and_then(Value::as_str),
            destination.get("chat_id"),
        ) else {
            continue;
        };
        let chat = chat
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| chat.to_string());
        let base = format!("https://api.telegram.org/bot{token}");
        post(
            &format!("{base}/sendMessage"),
            &[("chat_id", &chat), ("text", &text)],
        );
        send_file(&format!("{base}/sendPhoto"), &chat, "photo", photo);
        send_file(&format!("{base}/sendVideo"), &chat, "video", video);
        send_file(&format!("{base}/sendDocument"), &chat, "document", document);
    }
}

fn post(url: &str, fields: &[(&str, &str)]) {
    let result = agent(30).map_err(|e| e.to_string()).and_then(|agent| {
        agent
            .post(url)
            .send_form(fields)
            .map(|_| ())
            .or_else(|error| match error {
                ureq::Error::Status(_, _) => Ok(()),
                _ => Err(error.to_string()),
            })
    });
    if let Err(error) = result {
        println!("telegram send failed: {error}");
    }
}

fn multipart_body(
    boundary: &str,
    chat: &str,
    field: &str,
    filename: &str,
    bytes: &[u8],
) -> Vec<u8> {
    let filename = filename
        .replace('%', "%25")
        .replace('"', "%22")
        .replace('\r', "%0D")
        .replace('\n', "%0A");
    let mut body = format!("--{boundary}\r\nContent-Disposition: form-data; name=\"chat_id\"\r\n\r\n{chat}\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"{field}\"; filename=\"{filename}\"\r\n\r\n").into_bytes();
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}

fn send_file(url: &str, chat: &str, field: &str, path: Option<&Path>) {
    let Some(path) = path.filter(|path| path.exists()) else {
        return;
    };
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            println!("telegram send failed: {error}");
            return;
        }
    };
    let filename = path.file_name().unwrap_or_default().to_string_lossy();
    let boundary = format!(
        "unplugg-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    let body = multipart_body(&boundary, chat, field, &filename, &bytes);
    let result = agent(30).map_err(|e| e.to_string()).and_then(|agent| {
        agent
            .post(url)
            .set(
                "Content-Type",
                &format!("multipart/form-data; boundary={boundary}"),
            )
            .send_bytes(&body)
            .map(|_| ())
            .or_else(|error| match error {
                ureq::Error::Status(_, _) => Ok(()),
                _ => Err(error.to_string()),
            })
    });
    if let Err(error) = result {
        println!("telegram send failed: {error}");
    }
}

pub fn call(
    token: &str,
    method: &str,
    data: &[(&str, &str)],
    timeout: u64,
) -> Result<Value, String> {
    let unreachable = "Couldn't reach Telegram. Check your internet connection.";
    let agent = agent(15 + timeout).map_err(|_| unreachable.to_string())?;
    let response = match agent
        .post(&format!("https://api.telegram.org/bot{token}/{method}"))
        .send_form(data)
    {
        Ok(response) | Err(ureq::Error::Status(_, response)) => response,
        Err(_) => return Err(unreachable.to_string()),
    };
    let json: Value =
        serde_json::from_reader(response.into_reader()).map_err(|_| unreachable.to_string())?;
    if json.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        Ok(json.get("result").cloned().unwrap_or(Value::Null))
    } else {
        Err(json
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("Unknown error")
            .to_owned())
    }
}

pub fn find_chat_id(token: &str, wait: u64) -> Result<String, String> {
    let timeout = wait.to_string();
    match call(token, "getUpdates", &[("timeout", &timeout)], wait) {
        Ok(updates) => {
            if let Some(updates) = updates.as_array() {
                for update in updates.iter().rev() {
                    let post = update
                        .get("message")
                        .filter(|v| !v.is_null())
                        .or_else(|| update.get("channel_post"));
                    if let Some(id) = post.and_then(|p| p.get("chat")).and_then(|c| c.get("id")) {
                        return Ok(id
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| id.to_string()));
                    }
                }
            }
            Err("No message arrived. Send your bot a message, then try again.".to_owned())
        }
        Err(error) if error.starts_with("Conflict") => Err(
            "Another app reads this bot's messages. Give unplugg its own bot, or type the chat id."
                .to_owned(),
        ),
        Err(error) => Err(error),
    }
}

pub fn check(token: &str, chat_id: &str) -> Result<(), String> {
    call(token, "getChat", &[("chat_id", chat_id)], 0).map(|_| ())
}

pub fn send_test(token: &str, chat_id: &str) -> Result<(), String> {
    call(
        token,
        "sendMessage",
        &[
            ("chat_id", chat_id),
            ("text", "✅ unplugg test: alerts will arrive here."),
        ],
        0,
    )
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::multipart_body;

    #[test]
    fn multipart_escapes_filename() {
        let body = multipart_body("boundary", "123", "video", "a\"\r\nb.mp4", b"video");
        assert!(String::from_utf8(body)
            .unwrap()
            .contains("filename=\"a%22%0D%0Ab.mp4\""));
    }

    #[test]
    fn multipart_preserves_binary_and_formats_fields() {
        assert_eq!(multipart_body("boundary", "123", "photo", "a.jpg", &[0, 255]),
            b"--boundary\r\nContent-Disposition: form-data; name=\"chat_id\"\r\n\r\n123\r\n--boundary\r\nContent-Disposition: form-data; name=\"photo\"; filename=\"a.jpg\"\r\n\r\n\x00\xff\r\n--boundary--\r\n");
    }
}
