//! Cameras, microphones and speakers for the Settings pickers, saved by name.
//! A saved device that's gone falls back to the default, so a thief pulling a
//! dock cable can't leave the alarm without a camera. The siren's speaker goes
//! to 100% while it sounds, through PulseAudio/PipeWire's pactl.

use std::process::Command;

/// (name, description) of each sink or source, as `pactl list` shows them.
fn pactl_list(kind: &str) -> Vec<(String, String)> {
    // C locale: the field names are translated otherwise
    let Ok(output) = Command::new("pactl").args(["list", kind]).env("LC_ALL", "C").output() else { return Vec::new() };
    let mut devices = Vec::new();
    let mut name = None;
    for line in String::from_utf8_lossy(&output.stdout).lines().map(str::trim) {
        if let Some(value) = line.strip_prefix("Name: ") {
            name = Some(value.to_owned());
        } else if let (Some(value), Some(name)) = (line.strip_prefix("Description: "), name.take()) {
            devices.push((name, value.to_owned()));
        }
    }
    devices
}

fn sinks() -> Vec<(String, String)> {
    pactl_list("sinks")
}

fn sources() -> Vec<(String, String)> {
    // a monitor records what the speakers play, not a microphone
    pactl_list("sources").into_iter().filter(|(name, _)| !name.ends_with(".monitor")).collect()
}

/// (path, card name) of each video capture device.
fn video_devices() -> Vec<(String, String)> {
    let mut devices: Vec<_> = v4l::context::enum_devices()
        .into_iter()
        .filter_map(|node| {
            let path = node.path().to_string_lossy().into_owned();
            let caps = v4l::Device::with_path(&path).ok()?.query_caps().ok()?;
            // a webcam also exposes a metadata node, which can't capture
            caps.capabilities.contains(v4l::capability::Flags::VIDEO_CAPTURE).then(|| (path, caps.card))
        })
        .collect();
    devices.sort();
    devices
}

pub fn cameras() -> Vec<String> {
    video_devices().into_iter().map(|(_, card)| card).collect()
}

pub fn microphones() -> Vec<String> {
    sources().into_iter().map(|(_, description)| description).collect()
}

pub fn speakers() -> Vec<String> {
    sinks().into_iter().map(|(_, description)| description).collect()
}

/// The device called `name` while it's connected, else the default.
fn pick(devices: Vec<(String, String)>, name: &str) -> Option<(String, String)> {
    devices.into_iter().find(|(_, description)| description == name)
}

/// (path, name) of the camera to use: the chosen one, else the first.
pub fn camera(name: &str) -> Option<(String, String)> {
    let devices = video_devices();
    pick(devices.clone(), name).or_else(|| devices.into_iter().next())
}

/// The source's pulse name, or None for the default microphone.
pub fn microphone(name: &str) -> Option<String> {
    pick(sources(), name).map(|(id, _)| id)
}

/// The sink's pulse name, or None for the default speaker.
pub fn speaker(name: &str) -> Option<String> {
    pick(sinks(), name).map(|(id, _)| id)
}

/// The default microphone's description, which Settings shows when none is chosen.
pub fn default_microphone() -> Option<String> {
    let output = Command::new("pactl").arg("get-default-source").output().ok()?;
    let name = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    sources().into_iter().find(|(id, _)| *id == name).map(|(_, description)| description)
}

/// Unmutes `sink` at 100%; returns what puts it back.
pub fn max_volume(sink: Option<&str>) -> impl FnOnce() {
    let sink = sink.unwrap_or("@DEFAULT_SINK@").to_owned();
    let pactl = |args: &[&str]| Command::new("pactl").args(args).env("LC_ALL", "C").output().ok().map(|out| String::from_utf8_lossy(&out.stdout).into_owned());
    let level = pactl(&["get-sink-volume", &sink]).and_then(|out| out.split('%').next()?.rsplit(' ').next()?.trim().parse::<u32>().ok());
    let muted = pactl(&["get-sink-mute", &sink]).is_some_and(|out| out.contains("yes"));
    pactl(&["set-sink-mute", &sink, "0"]);
    pactl(&["set-sink-volume", &sink, "100%"]);
    move || {
        // no pactl, or a sink that's gone: nothing to put back
        if let Some(level) = level {
            pactl(&["set-sink-volume", &sink, &format!("{level}%")]);
            pactl(&["set-sink-mute", &sink, if muted { "1" } else { "0" }]);
        }
    }
}
