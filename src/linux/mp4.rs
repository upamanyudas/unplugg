//! A small MP4 writer: one H.264 track and an optional AAC track, the index up
//! front ("fast start") so Telegram can play it while it downloads.

use std::fs;
use std::io;
use std::path::Path;

pub const VIDEO_SCALE: u32 = 90_000; // ticks per second for video sample times

pub struct Sample {
    pub data: Vec<u8>,
    pub duration: u32,
    pub sync: bool,
}

pub struct Video {
    pub width: u16,
    pub height: u16,
    pub sps: Vec<u8>,
    pub pps: Vec<u8>,
    pub samples: Vec<Sample>,
}

pub struct Audio {
    pub rate: u32,
    pub channels: u16,
    // AudioSpecificConfig, and the raw frames of 1024 samples each
    pub config: Vec<u8>,
    pub frames: Vec<Vec<u8>>,
}

fn bx(kind: &[u8; 4], parts: &[&[u8]]) -> Vec<u8> {
    let size = 8 + parts.iter().map(|p| p.len()).sum::<usize>();
    let mut out = Vec::with_capacity(size);
    out.extend((size as u32).to_be_bytes());
    out.extend(kind);
    parts.iter().for_each(|part| out.extend(*part));
    out
}

/// A box with a version and flags word.
fn full(kind: &[u8; 4], flags: u32, parts: &[&[u8]]) -> Vec<u8> {
    let head = flags.to_be_bytes();
    bx(kind, &[&[&head[..]], parts].concat())
}

fn be(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_be_bytes()).collect()
}

const MATRIX: [u32; 9] = [0x10000, 0, 0, 0, 0x10000, 0, 0, 0, 0x4000_0000];

fn tkhd(id: u32, duration: u32, audio: bool, width: u16, height: u16) -> Vec<u8> {
    let volume: u32 = if audio { 0x0100_0000 } else { 0 };
    let mut body = be(&[0, 0, id, 0, duration, 0, 0, 0, volume]);
    body.extend(be(&MATRIX));
    body.extend(be(&[(width as u32) << 16, (height as u32) << 16]));
    full(b"tkhd", 3, &[&body])
}

fn mdhd(scale: u32, duration: u32) -> Vec<u8> {
    // language "und"
    full(b"mdhd", 0, &[&be(&[0, 0, scale, duration]), &[0x55, 0xc4, 0, 0]])
}

fn hdlr(kind: &[u8; 4], name: &str) -> Vec<u8> {
    full(b"hdlr", 0, &[&be(&[0]), kind, &be(&[0, 0, 0]), name.as_bytes(), &[0]])
}

fn dinf() -> Vec<u8> {
    let url = full(b"url ", 1, &[]);
    bx(b"dinf", &[&full(b"dref", 0, &[&be(&[1]), &url])])
}

/// Sample tables, with one chunk per sample at the given file offsets.
fn tables(entry: Vec<u8>, durations: &[u32], sizes: &[u32], offsets: &[u32], sync: Option<Vec<u32>>) -> Vec<u8> {
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for &duration in durations {
        match runs.last_mut() {
            Some((count, last)) if *last == duration => *count += 1,
            _ => runs.push((1, duration)),
        }
    }
    let stts: Vec<u32> = std::iter::once(runs.len() as u32).chain(runs.iter().flat_map(|&(c, d)| [c, d])).collect();
    let stsz: Vec<u32> = [0, sizes.len() as u32].into_iter().chain(sizes.iter().copied()).collect();
    let stco: Vec<u32> = std::iter::once(offsets.len() as u32).chain(offsets.iter().copied()).collect();
    let mut boxes = vec![
        full(b"stsd", 0, &[&be(&[1]), &entry]),
        full(b"stts", 0, &[&be(&stts)]),
        full(b"stsc", 0, &[&be(&[1, 1, 1, 1])]),
        full(b"stsz", 0, &[&be(&stsz)]),
        full(b"stco", 0, &[&be(&stco)]),
    ];
    if let Some(sync) = sync {
        let stss: Vec<u32> = std::iter::once(sync.len() as u32).chain(sync).collect();
        boxes.insert(2, full(b"stss", 0, &[&be(&stss)]));
    }
    let parts: Vec<&[u8]> = boxes.iter().map(Vec::as_slice).collect();
    bx(b"stbl", &parts)
}

fn avc1(video: &Video) -> Vec<u8> {
    let sps = &video.sps;
    let mut avcc = vec![1, sps[1], sps[2], sps[3], 0xff, 0xe1];
    avcc.extend((sps.len() as u16).to_be_bytes());
    avcc.extend(sps);
    avcc.push(1);
    avcc.extend((video.pps.len() as u16).to_be_bytes());
    avcc.extend(&video.pps);
    let mut entry = vec![0, 0, 0, 0, 0, 0, 0, 1];
    entry.extend([0; 16]);
    entry.extend(video.width.to_be_bytes());
    entry.extend(video.height.to_be_bytes());
    entry.extend(be(&[0x0048_0000, 0x0048_0000, 0]));
    entry.extend(1u16.to_be_bytes());
    entry.extend([0; 32]);
    entry.extend([0x00, 0x18, 0xff, 0xff]);
    bx(b"avc1", &[&entry, &bx(b"avcC", &[&avcc])])
}

fn mp4a(audio: &Audio) -> Vec<u8> {
    // ES_Descriptor > DecoderConfigDescriptor > DecoderSpecificInfo, plus SLConfig
    let specific = [&[0x05, audio.config.len() as u8][..], &audio.config].concat();
    let mut decoder = vec![0x04, (13 + specific.len()) as u8, 0x40, 0x15, 0, 0x18, 0];
    decoder.extend(be(&[0, 0]));
    decoder.extend(&specific);
    let mut es = vec![0x03, (3 + decoder.len() + 3) as u8, 0, 1, 0];
    es.extend(&decoder);
    es.extend([0x06, 1, 2]);
    let mut entry = vec![0, 0, 0, 0, 0, 0, 0, 1];
    entry.extend([0; 8]);
    entry.extend(audio.channels.to_be_bytes());
    entry.extend(16u16.to_be_bytes());
    entry.extend([0; 4]);
    entry.extend((audio.rate << 16).to_be_bytes());
    bx(b"mp4a", &[&entry, &full(b"esds", 0, &[&es])])
}

fn moov(video: &Video, audio: Option<&Audio>, start: u32) -> Vec<u8> {
    let video_ticks: u32 = video.samples.iter().map(|s| s.duration).sum();
    let video_ms = (video_ticks as u64 * 1000 / VIDEO_SCALE as u64) as u32;
    let mut offset = start;
    let mut place = |sizes: &[u32]| -> Vec<u32> {
        sizes
            .iter()
            .map(|size| {
                let at = offset;
                offset += size;
                at
            })
            .collect()
    };
    let sizes: Vec<u32> = video.samples.iter().map(|s| s.data.len() as u32).collect();
    let durations: Vec<u32> = video.samples.iter().map(|s| s.duration).collect();
    let sync: Vec<u32> = video.samples.iter().enumerate().filter(|(_, s)| s.sync).map(|(i, _)| i as u32 + 1).collect();
    let stbl = tables(avc1(video), &durations, &sizes, &place(&sizes), Some(sync));
    let vmhd = full(b"vmhd", 1, &[&[0; 8]]);
    let minf = bx(b"minf", &[&vmhd, &dinf(), &stbl]);
    let mdia = bx(b"mdia", &[&mdhd(VIDEO_SCALE, video_ticks), &hdlr(b"vide", "VideoHandler"), &minf]);
    let mut tracks = vec![bx(b"trak", &[&tkhd(1, video_ms, false, video.width, video.height), &mdia])];
    let mut duration = video_ms;
    if let Some(audio) = audio {
        let sizes: Vec<u32> = audio.frames.iter().map(|f| f.len() as u32).collect();
        let ticks = 1024 * audio.frames.len() as u32;
        let ms = (ticks as u64 * 1000 / audio.rate as u64) as u32;
        duration = duration.max(ms);
        let stbl = tables(mp4a(audio), &vec![1024; sizes.len()], &sizes, &place(&sizes), None);
        let smhd = full(b"smhd", 0, &[&[0; 4]]);
        let minf = bx(b"minf", &[&smhd, &dinf(), &stbl]);
        let mdia = bx(b"mdia", &[&mdhd(audio.rate, ticks), &hdlr(b"soun", "SoundHandler"), &minf]);
        tracks.push(bx(b"trak", &[&tkhd(2, ms, true, 0, 0), &mdia]));
    }
    let mut mvhd = be(&[0, 0, 1000, duration, 0x10000]);
    mvhd.extend([1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    mvhd.extend(be(&MATRIX));
    mvhd.extend([0; 24]);
    mvhd.extend(be(&[tracks.len() as u32 + 1]));
    let mvhd = full(b"mvhd", 0, &[&mvhd]);
    let parts: Vec<&[u8]> = std::iter::once(mvhd.as_slice()).chain(tracks.iter().map(Vec::as_slice)).collect();
    bx(b"moov", &parts)
}

pub fn write(path: &Path, video: &Video, audio: Option<&Audio>) -> io::Result<()> {
    let ftyp = bx(b"ftyp", &[b"isom", &be(&[0x200]), b"isomiso2avc1mp41"]);
    // the moov's size doesn't depend on the offsets in it, so lay it out twice
    let size = moov(video, audio, 0).len() as u32;
    let start = ftyp.len() as u32 + size + 8;
    let moov = moov(video, audio, start);
    let payload: Vec<&[u8]> = video
        .samples
        .iter()
        .map(|s| s.data.as_slice())
        .chain(audio.into_iter().flat_map(|a| a.frames.iter().map(Vec::as_slice)))
        .collect();
    let mdat = bx(b"mdat", &payload);
    fs::write(path, [ftyp, moov, mdat].concat())
}
