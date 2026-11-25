use windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

use super::utils::{open_url, registry_string};

const STORE: &str =
    r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore";

fn name(key: &str) -> &str {
    match key {
        "camera" => "webcam",
        "microphone" => "microphone",
        "location" => "location",
        _ => key,
    }
}

pub fn status(key: &str) -> &'static str {
    let path = format!(r"{STORE}\{}", name(key));
    for (root, path) in [
        (HKEY_LOCAL_MACHINE, path.clone()),
        (HKEY_CURRENT_USER, path.clone()),
        (HKEY_CURRENT_USER, format!(r"{path}\NonPackaged")),
    ] {
        if registry_string(root, &path, "Value").as_deref() == Some("Deny") {
            return "denied";
        }
    }
    "granted"
}

pub fn open_settings(key: &str) {
    open_url(&format!("ms-settings:privacy-{}", name(key)));
}
