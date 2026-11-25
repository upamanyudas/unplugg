//! The siren: the FLAC decoded once at launch, looped on the chosen speaker at
//! full volume until stop() puts the volume back.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;

use super::{devices, pulse, utils};

struct Sound {
    samples: Vec<i16>,
    rate: u32,
    channels: u8,
}

type Playing = (Arc<AtomicBool>, JoinHandle<()>, Box<dyn FnOnce() + Send>);

static PLAYING: Mutex<Option<Playing>> = Mutex::new(None);

fn sound() -> Option<&'static Sound> {
    static SOUND: OnceLock<Option<Sound>> = OnceLock::new();
    SOUND
        .get_or_init(|| {
            let mut reader = claxon::FlacReader::open(utils::asset_path("siren.flac")).ok()?;
            let info = reader.streaminfo();
            let samples = reader.samples().map(|s| s.map(|s| s as i16)).collect::<Result<_, _>>().ok()?;
            Some(Sound { samples, rate: info.sample_rate, channels: info.channels as u8 })
        })
        .as_ref()
}

/// Decode the siren at launch, so it starts at once when the alarm fires.
pub fn prepare() {
    sound();
}

/// Loop the siren at full volume on the chosen speaker; stop() restores it.
pub fn start(speaker: &str) {
    let mut playing = PLAYING.lock().unwrap();
    let Some(sound) = sound() else { return };
    if playing.is_some() {
        return;
    }
    let sink = devices::speaker(speaker);
    let restore = devices::max_volume(sink.as_deref());
    let stop = Arc::new(AtomicBool::new(false));
    let stopped = stop.clone();
    let thread = std::thread::spawn(move || {
        let Some(mut stream) = pulse::Stream::open(false, sink.as_deref(), sound.rate, sound.channels, "siren") else { return };
        // a tenth of a second at a time, so stop() is heard at once
        let chunk = sound.rate as usize * sound.channels as usize / 10;
        'siren: loop {
            for part in sound.samples.chunks(chunk) {
                if stopped.load(Ordering::Relaxed) || !stream.write(part) {
                    break 'siren;
                }
            }
        }
        stream.flush();
    });
    *playing = Some((stop, thread, Box::new(restore)));
}

pub fn stop() {
    let Some((stop, thread, restore)) = PLAYING.lock().unwrap().take() else { return };
    stop.store(true, Ordering::Relaxed);
    let _ = thread.join();
    restore();
}
