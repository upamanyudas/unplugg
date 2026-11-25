use std::io::Cursor;
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::Duration;

use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::*;

pub struct Siren(Sender<()>, Option<thread::JoinHandle<()>>);

impl Siren {
    pub fn start(name: &str) -> Self {
        let name = name.to_owned();
        let (tx, rx) = mpsc::channel();
        let worker = thread::spawn(move || unsafe {
            if CoInitializeEx(None, COINIT_MULTITHREADED).is_err() {
                return;
            }
            if let Err(error) = play(&name, rx) {
                eprintln!("siren: {error}");
            }
            CoUninitialize();
        });
        Self(tx, Some(worker))
    }
}

impl Drop for Siren {
    fn drop(&mut self) {
        let _ = self.0.send(());
        if let Some(worker) = self.1.take() {
            let _ = worker.join();
        }
    }
}

unsafe fn play(name: &str, stop: mpsc::Receiver<()>) -> windows::core::Result<()> {
    let mut reader =
        claxon::FlacReader::new(Cursor::new(include_bytes!("../../assets/siren.flac")))
            .map_err(|_| windows::core::Error::empty())?;
    let samples: Vec<i16> = reader
        .samples()
        .filter_map(Result::ok)
        .map(|s| s as i16)
        .collect();
    if samples.is_empty() {
        return Ok(());
    }
    let device = super::devices::endpoint(eRender, name)?;
    let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
    let format = WAVEFORMATEX {
        wFormatTag: 1,
        nChannels: 1,
        nSamplesPerSec: 44_100,
        nAvgBytesPerSec: 88_200,
        nBlockAlign: 2,
        wBitsPerSample: 16,
        cbSize: 0,
    };
    client.Initialize(
        AUDCLNT_SHAREMODE_SHARED,
        AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
        1_000_000,
        0,
        &format,
        None,
    )?;
    let render: IAudioRenderClient = client.GetService()?;
    let frames = client.GetBufferSize()?;
    let volume: Option<IAudioEndpointVolume> = device.Activate(CLSCTX_ALL, None).ok();
    let saved = volume.as_ref().and_then(|v| {
        Some((
            v.GetMasterVolumeLevelScalar().ok()?,
            v.GetMute().ok()?.as_bool(),
        ))
    });
    if let (Some(v), Some(_)) = (&volume, saved) {
        let _ = v.SetMute(false, std::ptr::null());
        let _ = v.SetMasterVolumeLevelScalar(1.0, std::ptr::null());
    }
    let result = (|| {
        client.Start()?;
        let mut position = 0;
        while stop.try_recv().is_err() {
            let available = frames.saturating_sub(client.GetCurrentPadding()?);
            if available > 0 {
                let buffer = render.GetBuffer(available)? as *mut i16;
                for i in 0..available as usize {
                    buffer.add(i).write_unaligned(samples[position]);
                    position = (position + 1) % samples.len();
                }
                render.ReleaseBuffer(available, 0)?;
            }
            thread::sleep(Duration::from_millis(20));
        }
        Ok(())
    })();
    let _ = client.Stop();
    if let (Some(v), Some((level, muted))) = (volume, saved) {
        let _ = v.SetMasterVolumeLevelScalar(level, std::ptr::null());
        let _ = v.SetMute(muted, std::ptr::null());
    }
    result
}
