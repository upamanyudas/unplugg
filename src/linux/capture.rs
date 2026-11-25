//! Webcam capture: a 10 s H.264 + AAC MP4 with mic audio, which Telegram plays
//! inline, plus stills from the same feed at 1, 2 and 3 s, so the camera opens
//! once. Runs on the alarm's thread and blocks while it records.

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use gtk::gdk_pixbuf::{Colorspace, Pixbuf, PixbufLoader};
use gtk::prelude::*;
use openh264::encoder::{BitRate, Encoder, EncoderConfig, FrameRate, FrameType, RateControlMode};
use openh264::formats::{RgbSliceU8, YUVBuffer};
use openh264::OpenH264API;
use serde_json::Value;
use v4l::buffer::Type;
use v4l::io::traits::CaptureStream;
use v4l::video::Capture;
use v4l::FourCC;

use super::{devices, faac, mp4, pulse};

const SECONDS: u64 = 10;
const BITRATE: u32 = 4_000_000; // ~4 MB a clip: quick to upload mid-theft
const RATE: u32 = 44_100;

fn media_dir(config: &Value) -> PathBuf {
    let dir = config["media_dir"].as_str().filter(|d| !d.is_empty()).map_or_else(|| crate::config::home().join("Pictures/unplugg"), |dir| {
        dir.strip_prefix('~').map_or_else(|| dir.into(), |rest| crate::config::home().join(rest.trim_start_matches('/')))
    });
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// CLOCK_MONOTONIC in µs, the clock V4L2 stamps frames with.
fn monotonic() -> i64 {
    #[repr(C)]
    struct Time {
        sec: i64,
        nsec: i64,
    }
    unsafe extern "C" {
        fn clock_gettime(clock: i32, time: *mut Time) -> i32;
    }
    let mut time = Time { sec: 0, nsec: 0 };
    unsafe { clock_gettime(1, &mut time) };
    time.sec * 1_000_000 + time.nsec / 1000
}

/// Records, then returns (photo, video); either may be None.
pub fn capture(config: &Value) -> (Option<PathBuf>, Option<PathBuf>) {
    let dir = media_dir(config);
    let stamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let (sound, heard) = mpsc::channel();
    if config["record_audio"] != false {
        let mic = devices::microphone(config["mic"].as_str().unwrap_or(""));
        std::thread::spawn(move || {
            let _ = sound.send(record_audio(mic.as_deref()));
        });
    }
    let camera = devices::camera(config["camera"].as_str().unwrap_or(""));
    let Some((photo, video, first_frame)) = camera.and_then(|(path, _)| record_video(&path, &dir, &stamp)) else {
        return (None, None);
    };
    // line the sound up with the first frame
    let audio = heard.recv_timeout(Duration::from_secs(SECONDS + 5)).ok().flatten().and_then(|(started, mut pcm)| {
        let skip = ((first_frame - started).max(0) as u64 * RATE as u64 / 1_000_000) as usize;
        pcm.drain(..skip.min(pcm.len()));
        let (config, frames) = faac::encode(&pcm, RATE, 1)?;
        Some(mp4::Audio { rate: RATE, channels: 1, config, frames })
    });
    let path = dir.join(format!("{stamp}.mp4"));
    let video = video.and_then(|video| mp4::write(&path, &video, audio.as_ref()).ok().map(|_| path));
    (photo, video)
}

/// (when it started, 10 s of mono 16-bit PCM)
fn record_audio(mic: Option<&str>) -> Option<(i64, Vec<i16>)> {
    let mut stream = pulse::Stream::open(true, mic, RATE, 1, "alarm video")?;
    let started = monotonic();
    let mut pcm = vec![0i16; (RATE as u64 * (SECONDS + 1)) as usize];
    for chunk in pcm.chunks_mut(RATE as usize / 10) {
        if !stream.read(chunk) {
            return None;
        }
    }
    Some((started, pcm))
}

/// Video frames as I420, stills as RGB.
enum Frame {
    Yuyv,
    Mjpg,
}

fn record_video(path: &str, dir: &std::path::Path, stamp: &str) -> Option<(Option<PathBuf>, Option<mp4::Video>, i64)> {
    let device = v4l::Device::with_path(path).ok()?;
    let mut format = device.format().ok()?;
    (format.width, format.height, format.fourcc) = (1280, 720, FourCC::new(b"YUYV"));
    let mut format = device.set_format(&format).ok()?;
    if format.fourcc != FourCC::new(b"YUYV") {
        format.fourcc = FourCC::new(b"MJPG");
        format = device.set_format(&format).ok()?;
    }
    let kind = match &format.fourcc.repr {
        b"YUYV" => Frame::Yuyv,
        b"MJPG" => Frame::Mjpg,
        _ => return None,
    };
    let (width, height) = (format.width as usize & !1, format.height as usize & !1);
    let mut stream = v4l::io::mmap::Stream::with_buffers(&device, Type::VideoCapture, 4).ok()?;
    stream.set_timeout(Duration::from_secs(2));
    let config = EncoderConfig::new()
        .bitrate(BitRate::from_bps(BITRATE))
        .rate_control_mode(RateControlMode::Bitrate)
        .max_frame_rate(FrameRate::from_hz(30.0));
    let mut encoder = Encoder::with_api_config(OpenH264API::from_source(), config).ok()?;

    let (mut sps, mut pps, mut samples, mut times) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut photo = None;
    let started = Instant::now();
    let mut stills = 1;
    while started.elapsed() < Duration::from_secs(SECONDS) {
        let Ok((buffer, meta)) = stream.next() else { break };
        let data = &buffer[..meta.bytesused as usize];
        let rgb = match kind {
            Frame::Mjpg => match decode_jpeg(data, width, height) {
                Some(rgb) => Some(rgb),
                None => continue,
            },
            Frame::Yuyv => None,
        };
        // the live frame at 1, 2 and 3 s; the last one is sent, once exposure has settled
        if stills <= 3 && started.elapsed() >= Duration::from_secs(stills) {
            let rgb = rgb.clone().unwrap_or_else(|| yuyv_to_rgb(data, width, height, format.stride as usize));
            let still = dir.join(format!("{stamp}_{stills}.jpg"));
            let image = Pixbuf::from_mut_slice(rgb, Colorspace::Rgb, false, 8, width as i32, height as i32, width as i32 * 3);
            if image.savev(&still, "jpeg", &[("quality", "90")]).is_ok() {
                photo = Some(still);
            }
            stills += 1;
        }
        let yuv = match &rgb {
            Some(rgb) => YUVBuffer::from_rgb8_source(RgbSliceU8::new(rgb, (width, height))),
            None => YUVBuffer::from_vec(yuyv_to_i420(data, width, height, format.stride as usize), width, height),
        };
        let Some(sample) = h264(&mut encoder, &yuv, &mut sps, &mut pps) else { continue };
        samples.push(sample);
        times.push(meta.timestamp.sec as i64 * 1_000_000 + meta.timestamp.usec as i64);
    }
    let first = *times.first()?;
    // each frame lasts until the next; the last as long as the average
    let average = if times.len() > 1 { (times[times.len() - 1] - first) / (times.len() as i64 - 1) } else { 33_333 };
    for (i, sample) in samples.iter_mut().enumerate() {
        let lasted = times.get(i + 1).map_or(average, |next| next - times[i]).max(1);
        sample.duration = (lasted as u64 * mp4::VIDEO_SCALE as u64 / 1_000_000) as u32;
    }
    let video = (!sps.is_empty() && !pps.is_empty()).then(|| mp4::Video { width: width as u16, height: height as u16, sps, pps, samples });
    Some((photo, video, first))
}

/// One encoded frame as an MP4 sample; the parameter sets go to `sps` and `pps`.
fn h264(encoder: &mut Encoder, yuv: &YUVBuffer, sps: &mut Vec<u8>, pps: &mut Vec<u8>) -> Option<mp4::Sample> {
    let encoded = encoder.encode(yuv).ok()?;
    let mut data = Vec::new();
    for layer in (0..encoded.num_layers()).filter_map(|i| encoded.layer(i)) {
        for nal in (0..layer.nal_count()).filter_map(|i| layer.nal_unit(i)) {
            // Annex B start code off, a length in front: what MP4 wants
            let nal = nal.strip_prefix(&[0, 0, 0, 1]).or_else(|| nal.strip_prefix(&[0, 0, 1])).unwrap_or(nal);
            match nal.first().map(|b| b & 0x1f) {
                Some(7) => *sps = nal.to_vec(),
                Some(8) => *pps = nal.to_vec(),
                Some(_) => {
                    data.extend((nal.len() as u32).to_be_bytes());
                    data.extend(nal);
                }
                None => (),
            }
        }
    }
    let sync = matches!(encoded.frame_type(), FrameType::IDR | FrameType::I);
    (!data.is_empty()).then_some(mp4::Sample { data, duration: 0, sync })
}

/// Tightly packed RGB from an MJPEG frame.
fn decode_jpeg(data: &[u8], width: usize, height: usize) -> Option<Vec<u8>> {
    let loader = PixbufLoader::with_type("jpeg").ok()?;
    loader.write(data).ok()?;
    loader.close().ok()?;
    let image = loader.pixbuf()?;
    if image.width() as usize != width || image.height() as usize != height {
        return None;
    }
    let (stride, channels) = (image.rowstride() as usize, image.n_channels() as usize);
    let bytes = image.read_pixel_bytes();
    let mut rgb = Vec::with_capacity(width * height * 3);
    for y in 0..height {
        for pixel in bytes[y * stride..][..width * channels].chunks_exact(channels) {
            rgb.extend(&pixel[..3]);
        }
    }
    Some(rgb)
}

/// YUYV (Y0 U Y1 V for two pixels) to planar I420: chroma averaged over 2×2 blocks.
fn yuyv_to_i420(data: &[u8], width: usize, height: usize, stride: usize) -> Vec<u8> {
    let mut out = vec![0u8; width * height * 3 / 2];
    let (luma, chroma) = out.split_at_mut(width * height);
    let (u_plane, v_plane) = chroma.split_at_mut(width * height / 4);
    for y in 0..height {
        let row = &data[y * stride..][..width * 2];
        for x in 0..width {
            luma[y * width + x] = row[x * 2];
        }
    }
    for y in (0..height).step_by(2) {
        let (top, bottom) = (&data[y * stride..][..width * 2], &data[(y + 1) * stride..][..width * 2]);
        for x in (0..width).step_by(2) {
            let at = (y / 2) * (width / 2) + x / 2;
            u_plane[at] = ((top[x * 2 + 1] as u16 + bottom[x * 2 + 1] as u16) / 2) as u8;
            v_plane[at] = ((top[x * 2 + 3] as u16 + bottom[x * 2 + 3] as u16) / 2) as u8;
        }
    }
    out
}

/// YUYV to RGB with BT.601's limited-range matrix, for the stills.
fn yuyv_to_rgb(data: &[u8], width: usize, height: usize, stride: usize) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(width * height * 3);
    for y in 0..height {
        for pair in data[y * stride..][..width * 2].chunks_exact(4) {
            let (u, v) = (pair[1] as i32 - 128, pair[3] as i32 - 128);
            for luma in [pair[0], pair[2]] {
                let c = 298 * (luma as i32 - 16);
                let clamp = |value: i32| ((value + 128) >> 8).clamp(0, 255) as u8;
                rgb.extend([clamp(c + 409 * v), clamp(c - 100 * u - 208 * v), clamp(c + 516 * u)]);
            }
        }
    }
    rgb
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two seconds of moving colour and a tone, written where the host can probe it.
    #[test]
    fn writes_a_playable_mp4() {
        let (width, height) = (320, 240);
        let config = EncoderConfig::new().bitrate(BitRate::from_bps(BITRATE)).rate_control_mode(RateControlMode::Bitrate);
        let mut encoder = Encoder::with_api_config(OpenH264API::from_source(), config).unwrap();
        let (mut sps, mut pps, mut samples) = (Vec::new(), Vec::new(), Vec::new());
        for frame in 0..60 {
            let mut yuyv = vec![0u8; width * height * 2];
            for (i, byte) in yuyv.iter_mut().enumerate() {
                *byte = if i % 2 == 0 { ((i / 2 % width + frame * 4) % 256) as u8 } else { (frame * 3 % 256) as u8 };
            }
            let yuv = YUVBuffer::from_vec(yuyv_to_i420(&yuyv, width, height, width * 2), width, height);
            let mut sample = h264(&mut encoder, &yuv, &mut sps, &mut pps).unwrap();
            sample.duration = mp4::VIDEO_SCALE / 30;
            samples.push(sample);
        }
        let video = mp4::Video { width: width as u16, height: height as u16, sps, pps, samples };
        let tone: Vec<i16> = (0..RATE * 2).map(|i| ((i as f32 * 440.0 * std::f32::consts::TAU / RATE as f32).sin() * 8000.0) as i16).collect();
        let audio = faac::encode(&tone, RATE, 1).map(|(config, frames)| mp4::Audio { rate: RATE, channels: 1, config, frames });
        let out = std::env::temp_dir().join("unplugg-test.mp4");
        mp4::write(&out, &video, audio.as_ref()).unwrap();
        println!("wrote {} with audio: {}", out.display(), audio.is_some());
    }
}
