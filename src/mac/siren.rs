use std::cell::RefCell;

use objc2::{rc::Retained, AllocAnyThread};
use objc2_app_kit::NSSound;
use objc2_foundation::{NSAppleScript, NSString};

use super::{devices, utils};

const GET_VOLUME: &str = "output volume of (get volume settings)";
const MAX_VOLUME: &str = "set volume output volume 100 without output muted";

#[derive(Default)]
struct Siren {
    sound: Option<Retained<NSSound>>,
    volume: Option<String>,
    speaker: Option<String>,
    get_volume: Option<Retained<NSAppleScript>>,
    max_volume: Option<Retained<NSAppleScript>>,
}

thread_local! {
    static SIREN: RefCell<Siren> = RefCell::new(Siren::default());
}

fn script(source: &str) -> Option<Retained<NSAppleScript>> {
    NSAppleScript::initWithSource(NSAppleScript::alloc(), &NSString::from_str(source))
}

fn run(script: &NSAppleScript) -> Option<String> {
    unsafe {
        script
            .executeAndReturnError(None)
            .stringValue()
            .map(|value| value.to_string())
    }
}

fn sound() -> Option<Retained<NSSound>> {
    let path = NSString::from_str(&utils::asset_path("siren.flac").to_string_lossy());
    NSSound::initWithContentsOfFile_byReference(NSSound::alloc(), &path, true)
}

pub fn prepare() {
    SIREN.with_borrow_mut(|siren| {
        siren.get_volume = script(GET_VOLUME);
        siren.max_volume = script(MAX_VOLUME);
        for script in [&siren.get_volume, &siren.max_volume].into_iter().flatten() {
            unsafe { script.compileAndReturnError(None) };
        }
        if let Some(script) = &siren.get_volume {
            run(script);
        }
        if let Some(sound) = sound() {
            sound.setVolume(0.0);
            sound.play();
            sound.stop();
        }
    });
}

pub fn start(speaker: &str) {
    SIREN.with_borrow_mut(|siren| {
        if siren.sound.is_some() {
            return;
        }
        if !speaker.is_empty() {
            siren.speaker = devices::current_speaker();
            devices::set_speaker(speaker);
        }
        siren.volume = siren.get_volume.as_deref().and_then(run);
        if let Some(script) = &siren.max_volume {
            run(script);
        }
        siren.sound = sound();
        if let Some(sound) = &siren.sound {
            sound.setLoops(true);
            sound.play();
        }
    });
}

pub fn stop() {
    SIREN.with_borrow_mut(|siren| {
        if let Some(sound) = siren.sound.take() {
            sound.stop();
        }
        if let Some(volume) = siren.volume.take() {
            if let Some(script) = script(&format!("set volume output volume {volume}")) {
                run(&script);
            }
        }
        if let Some(speaker) = siren.speaker.take() {
            devices::set_speaker(&speaker);
        }
    });
}
