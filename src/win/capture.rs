use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::Value;
use windows::Win32::Media::Audio::{
    eCapture, IAudioCaptureClient, IAudioClient, IMMDevice, AUDCLNT_BUFFERFLAGS_SILENT,
    AUDCLNT_SHAREMODE_SHARED, WAVEFORMATEX,
};
use windows::Win32::System::Com::{
    CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
};
// Returns a WAV only on systems without Media Foundation.
pub fn capture(config: &Value) -> Option<PathBuf> {
    if unsafe { super::media::MediaFoundation::load().is_some() }
        || !config
            .get("record_audio")
            .and_then(Value::as_bool)
            .unwrap_or(true)
    {
        return None;
    }
    let home = crate::config::home();
    let dir = match config
        .get("media_dir")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        Some("~") => home,
        Some(s) if s.starts_with("~/") || s.starts_with("~\\") => home.join(&s[2..]),
        Some(s) => PathBuf::from(s),
        None => home.join("Pictures/unplugg"),
    };
    std::fs::create_dir_all(&dir).ok()?;
    let stamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let path = dir.join(format!("{stamp}.wav"));
    unsafe {
        if CoInitializeEx(None, COINIT_MULTITHREADED).is_err() {
            return None;
        }
        let result = super::devices::endpoint(
            eCapture,
            config.get("mic").and_then(Value::as_str).unwrap_or(""),
        )
        .ok()
        .and_then(|device| record_wav(&device, &path).ok());
        CoUninitialize();
        result.map(|_| path)
    }
}

// The shared-mode mix format can be WAVEFORMATEXTENSIBLE, so keep the extra bytes.
pub unsafe fn record_wav(
    device: &IMMDevice,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
    let format = client.GetMixFormat()?;
    if format.is_null() {
        return Err("missing audio format".into());
    }
    let len = std::mem::size_of::<WAVEFORMATEX>() + (*format).cbSize as usize;
    let bytes = std::slice::from_raw_parts(format.cast::<u8>(), len).to_vec();
    let block_align = (*format).nBlockAlign as usize;
    let max_frames = (*format).nSamplesPerSec as usize * 10;
    let init = client.Initialize(AUDCLNT_SHAREMODE_SHARED, 0, 10_000_000, 0, format, None);
    CoTaskMemFree(Some(format.cast()));
    init?;
    if block_align == 0 {
        return Err("invalid audio format".into());
    }
    let capture: IAudioCaptureClient = client.GetService()?;
    let mut data = Vec::new();
    client.Start()?;
    let until = Instant::now() + Duration::from_secs(15);
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        while data.len() / block_align < max_frames {
            if Instant::now() >= until {
                return Err("audio capture timed out".into());
            }
            while data.len() / block_align < max_frames && capture.GetNextPacketSize()? != 0 {
                let mut buffer = std::ptr::null_mut();
                let mut frames = 0;
                let mut flags = 0;
                capture.GetBuffer(&mut buffer, &mut frames, &mut flags, None, None)?;
                let size = (frames as usize)
                    .min(max_frames.saturating_sub(data.len() / block_align))
                    * block_align;
                if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 {
                    data.resize(data.len() + size, 0);
                } else if !buffer.is_null() {
                    data.extend_from_slice(std::slice::from_raw_parts(buffer, size));
                }
                capture.ReleaseBuffer(frames)?;
            }
            if data.len() / block_align >= max_frames {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    })();
    let _ = client.Stop();
    result?;
    let fmt_len = bytes.len() as u32;
    let data_len = u32::try_from(data.len())?;
    let pad = fmt_len & 1;
    let data_pad = data_len & 1;
    let mut file = std::fs::File::create(path)?;
    file.write_all(b"RIFF")?;
    file.write_all(&(4 + 8 + fmt_len + pad + 12 + 8 + data_len + data_pad).to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&fmt_len.to_le_bytes())?;
    file.write_all(&bytes)?;
    if pad != 0 {
        file.write_all(&[0])?;
    }
    file.write_all(b"fact")?;
    file.write_all(&4u32.to_le_bytes())?;
    file.write_all(&(data_len / block_align as u32).to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&data_len.to_le_bytes())?;
    file.write_all(&data)?;
    if data_pad != 0 {
        file.write_all(&[0])?;
    }
    Ok(())
}

pub struct Captured {
    pub photo: Option<PathBuf>,
    pub video: Option<PathBuf>,
    pub document: Option<PathBuf>,
}

pub fn capture_full(config: &Value) -> Captured {
    let config = config.clone();
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let result = capture_all(&config);
        let _ = sender.send(result);
    });
    receiver
        .recv_timeout(Duration::from_secs(25))
        .unwrap_or(Captured {
            photo: None,
            video: None,
            document: None,
        })
}

fn capture_all(config: &Value) -> Captured {
    let mut result = Captured {
        photo: None,
        video: None,
        document: None,
    };
    unsafe {
        let Some(api) = super::media::MediaFoundation::load() else {
            result.document = capture(config);
            return result;
        };
        if CoInitializeEx(None, COINIT_MULTITHREADED).is_err() {
            return result;
        }
        let camera =
            super::devices::camera(config.get("camera").and_then(Value::as_str).unwrap_or(""));
        if let Some(camera) = camera {
            let home = crate::config::home();
            let dir = match config
                .get("media_dir")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                Some("~") => home,
                Some(s) if s.starts_with("~/") || s.starts_with("~\\") => home.join(&s[2..]),
                Some(s) => PathBuf::from(s),
                None => home.join("Pictures/unplugg"),
            };
            if std::fs::create_dir_all(&dir).is_ok() {
                let stamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
                let video = dir.join(format!("{stamp}.mp4"));
                let microphone = config
                    .get("record_audio")
                    .and_then(Value::as_bool)
                    .unwrap_or(true)
                    .then(|| config.get("mic").and_then(Value::as_str).unwrap_or(""));
                if let Ok(photo) = super::media::record_video(&api, &camera, microphone, &video) {
                    result.photo = photo;
                    if video.is_file() {
                        result.video = Some(video);
                    }
                }
            }
        }
        drop(api);
        CoUninitialize();
    }
    result
}
