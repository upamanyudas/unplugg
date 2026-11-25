use std::ffi::c_void;
use std::mem::transmute;
use std::ptr::null_mut;

use windows::core::HRESULT;
use windows::Win32::Media::MediaFoundation::{
    IMFAttributes, IMFMediaSource, IMFMediaType, IMFSinkWriter, IMFSourceReader,
};
use windows_sys::Win32::Foundation::{FreeLibrary, HMODULE};
use windows_sys::Win32::System::LibraryLoader::{
    GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32,
};

type Startup = unsafe extern "system" fn(u32, u32) -> HRESULT;
type Shutdown = unsafe extern "system" fn() -> HRESULT;
type Attributes = unsafe extern "system" fn(*mut Option<IMFAttributes>, u32) -> HRESULT;
type MediaType = unsafe extern "system" fn(*mut Option<IMFMediaType>) -> HRESULT;
type DeviceSource = unsafe extern "system" fn(*mut c_void, *mut Option<IMFMediaSource>) -> HRESULT;
type SourceReader =
    unsafe extern "system" fn(*mut c_void, *mut c_void, *mut Option<IMFSourceReader>) -> HRESULT;
type SinkWriter = unsafe extern "system" fn(
    *const u16,
    *mut c_void,
    *mut c_void,
    *mut Option<IMFSinkWriter>,
) -> HRESULT;

pub struct MediaFoundation {
    libraries: Vec<HMODULE>,
    pub startup: Startup,
    pub shutdown: Shutdown,
    pub attributes: Attributes,
    pub media_type: MediaType,
    pub device_source: DeviceSource,
    pub source_reader: SourceReader,
    pub sink_writer: SinkWriter,
}

impl MediaFoundation {
    pub unsafe fn load() -> Option<Self> {
        let mut libraries = Libraries(Vec::new());
        let plat = libraries.open("mfplat.dll")?;
        let source = libraries.open("mf.dll")?;
        let reader = libraries.open("mfreadwrite.dll")?;
        macro_rules! function {
            ($library:expr, $name:literal, $kind:ty) => {{
                let address = GetProcAddress($library, concat!($name, "\0").as_ptr())?;
                transmute::<unsafe extern "system" fn() -> isize, $kind>(address)
            }};
        }
        let result = Self {
            startup: function!(plat, "MFStartup", Startup),
            shutdown: function!(plat, "MFShutdown", Shutdown),
            attributes: function!(plat, "MFCreateAttributes", Attributes),
            media_type: function!(plat, "MFCreateMediaType", MediaType),
            device_source: function!(source, "MFCreateDeviceSource", DeviceSource),
            source_reader: function!(reader, "MFCreateSourceReaderFromMediaSource", SourceReader),
            sink_writer: function!(reader, "MFCreateSinkWriterFromURL", SinkWriter),
            libraries: std::mem::take(&mut libraries.0),
        };
        Some(result)
    }
}

impl Drop for MediaFoundation {
    fn drop(&mut self) {
        for library in self.libraries.drain(..).rev() {
            unsafe {
                FreeLibrary(library);
            }
        }
    }
}

struct Libraries(Vec<HMODULE>);

impl Libraries {
    unsafe fn open(&mut self, name: &str) -> Option<HMODULE> {
        let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        let library = LoadLibraryExW(name.as_ptr(), null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32);
        if library.is_null() {
            None
        } else {
            self.0.push(library);
            Some(library)
        }
    }
}

impl Drop for Libraries {
    fn drop(&mut self) {
        for library in self.0.drain(..).rev() {
            unsafe {
                FreeLibrary(library);
            }
        }
    }
}

unsafe fn attributes(api: &MediaFoundation) -> windows::core::Result<IMFAttributes> {
    let mut value = None;
    (api.attributes)(&mut value, 4).ok()?;
    value.ok_or_else(windows::core::Error::empty)
}

unsafe fn media_type(api: &MediaFoundation) -> windows::core::Result<IMFMediaType> {
    let mut value = None;
    (api.media_type)(&mut value).ok()?;
    value.ok_or_else(windows::core::Error::empty)
}

unsafe fn video_source(
    api: &MediaFoundation,
    camera: &str,
) -> windows::core::Result<(IMFMediaSource, IMFSourceReader)> {
    use windows::core::{Interface, PCWSTR};
    use windows::Win32::Media::MediaFoundation::*;

    let device_attrs = attributes(api)?;
    device_attrs.SetGUID(
        &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
        &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
    )?;
    let camera: Vec<u16> = camera.encode_utf16().chain(Some(0)).collect();
    device_attrs.SetString(
        &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
        PCWSTR(camera.as_ptr()),
    )?;
    let mut source = None;
    (api.device_source)(device_attrs.as_raw(), &mut source).ok()?;
    let source = source.ok_or_else(windows::core::Error::empty)?;
    let result = (|| {
        let options = attributes(api)?;
        options.SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1)?;
        let mut reader = None;
        (api.source_reader)(source.as_raw(), options.as_raw(), &mut reader).ok()?;
        let reader = reader.ok_or_else(windows::core::Error::empty)?;
        let format = media_type(api)?;
        format.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        format.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
        reader.SetCurrentMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32, None, &format)?;
        Ok(reader)
    })();
    if result.is_err() {
        let _ = source.Shutdown();
    }
    result.map(|reader| (source, reader))
}

unsafe fn audio_source(
    api: &MediaFoundation,
    microphone: &str,
) -> windows::core::Result<(IMFMediaSource, IMFSourceReader)> {
    use windows::core::{Interface, PCWSTR};
    use windows::Win32::Media::Audio::eCapture;
    use windows::Win32::Media::MediaFoundation::*;
    use windows::Win32::System::Com::CoTaskMemFree;

    let attributes = attributes(api)?;
    let device = super::devices::endpoint(eCapture, microphone)?;
    let id = device.GetId()?;
    let result = attributes.SetString(
        &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_AUDCAP_ENDPOINT_ID,
        PCWSTR(id.0),
    );
    CoTaskMemFree(Some(id.0.cast()));
    result?;
    attributes.SetGUID(
        &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
        &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_AUDCAP_GUID,
    )?;
    let mut source = None;
    (api.device_source)(attributes.as_raw(), &mut source).ok()?;
    let source = source.ok_or_else(windows::core::Error::empty)?;
    let result = (|| {
        let mut reader = None;
        (api.source_reader)(source.as_raw(), null_mut(), &mut reader).ok()?;
        let reader = reader.ok_or_else(windows::core::Error::empty)?;
        for (rate, channels) in [(48_000, 2), (44_100, 2), (48_000, 1), (44_100, 1)] {
            let format = media_type(api)?;
            format.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
            format.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)?;
            format.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
            format.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, channels)?;
            format.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, rate)?;
            if reader
                .SetCurrentMediaType(MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32, None, &format)
                .is_ok()
            {
                return Ok(reader);
            }
        }
        Err(windows::core::Error::empty())
    })();
    if result.is_err() {
        let _ = source.Shutdown();
    }
    result.map(|reader| (source, reader))
}

unsafe fn video_writer(
    api: &MediaFoundation,
    path: &std::path::Path,
    reader: &IMFSourceReader,
    audio: Option<&IMFSourceReader>,
) -> windows::core::Result<(IMFSinkWriter, u32, Option<u32>)> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Media::MediaFoundation::*;

    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut writer = None;
    (api.sink_writer)(path.as_ptr(), null_mut(), null_mut(), &mut writer).ok()?;
    let writer = writer.ok_or_else(windows::core::Error::empty)?;
    let input = reader.GetCurrentMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32)?;
    let output = media_type(api)?;
    output.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
    output.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
    output.SetUINT64(&MF_MT_FRAME_SIZE, input.GetUINT64(&MF_MT_FRAME_SIZE)?)?;
    output.SetUINT64(&MF_MT_FRAME_RATE, input.GetUINT64(&MF_MT_FRAME_RATE)?)?;
    output.SetUINT64(
        &MF_MT_PIXEL_ASPECT_RATIO,
        input
            .GetUINT64(&MF_MT_PIXEL_ASPECT_RATIO)
            .unwrap_or((1u64 << 32) | 1),
    )?;
    output.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
    output.SetUINT32(&MF_MT_AVG_BITRATE, 4_000_000)?;
    let stream = writer.AddStream(&output)?;
    writer.SetInputMediaType(stream, &input, None)?;
    let audio_stream = if let Some(audio) = audio {
        let input = audio.GetCurrentMediaType(MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32)?;
        let output = media_type(api)?;
        output.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
        output.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_AAC)?;
        output.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
        output.SetUINT32(
            &MF_MT_AUDIO_NUM_CHANNELS,
            input.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS)?,
        )?;
        output.SetUINT32(
            &MF_MT_AUDIO_SAMPLES_PER_SECOND,
            input.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND)?,
        )?;
        output.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, 12_000)?;
        let stream = writer.AddStream(&output)?;
        writer.SetInputMediaType(stream, &input, None)?;
        Some(stream)
    } else {
        None
    };
    writer.BeginWriting()?;
    Ok((writer, stream, audio_stream))
}

pub unsafe fn record_video(
    api: &MediaFoundation,
    camera: &str,
    microphone: Option<&str>,
    path: &std::path::Path,
    last_photo: &std::sync::Mutex<Option<std::path::PathBuf>>,
) -> (Option<std::path::PathBuf>, bool) {
    use std::time::{Duration, Instant};
    use windows::Win32::Media::MediaFoundation::*;

    if (api.startup)(MF_VERSION, 0).is_err() {
        return (None, false);
    }
    let mut photo = None;
    let result = (|| {
        let (source, reader) = video_source(api, camera)?;
        let mut audio = microphone.and_then(|name| audio_source(api, name).ok());
        let result = (|| {
            let writer = video_writer(api, path, &reader, audio.as_ref().map(|(_, reader)| reader));
            let (writer, stream, audio_stream) = match writer {
                Ok(writer) => writer,
                Err(_) if audio.is_some() => {
                    if let Some((source, _)) = audio.take() {
                        let _ = source.Shutdown();
                    }
                    let _ = std::fs::remove_file(path);
                    video_writer(api, path, &reader, None)?
                }
                Err(error) => return Err(error),
            };
            let mut first_audio = None;
            let mut audio_time = -1;
            let mut audio_ended = false;
            let mut audio_discontinuity = false;
            let audio_format = audio.as_ref().and_then(|(_, reader)| {
                let format = reader
                    .GetCurrentMediaType(MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32)
                    .ok()?;
                Some((
                    format.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS).ok()?,
                    format.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND).ok()?,
                ))
            });
            let format =
                reader.GetCurrentMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32)?;
            let size = format.GetUINT64(&MF_MT_FRAME_SIZE)?;
            let width = (size >> 32) as u32;
            let height = size as u32;
            let stride = format.GetUINT32(&MF_MT_DEFAULT_STRIDE).unwrap_or(width);
            let stem = path.file_stem().unwrap_or_default().to_string_lossy();
            let mut next_photo = 1;
            let mut first = None;
            let until = Instant::now() + Duration::from_secs(25);
            while Instant::now() < until {
                let mut flags = 0;
                let mut timestamp = 0;
                let mut sample = None;
                reader.ReadSample(
                    MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                    0,
                    None,
                    Some(&mut flags),
                    Some(&mut timestamp),
                    Some(&mut sample),
                )?;
                if flags
                    & (MF_SOURCE_READERF_ERROR.0 | MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0)
                        as u32
                    != 0
                {
                    return Err(windows::core::Error::empty());
                }
                if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                    break;
                }
                if let Some(sample) = sample {
                    let start = *first.get_or_insert(timestamp);
                    let elapsed = timestamp.saturating_sub(start);
                    if next_photo <= 3 && elapsed >= next_photo * 10_000_000 {
                        let image = path.with_file_name(format!("{stem}_{next_photo}.jpg"));
                        if save_jpeg(&sample, width, height, stride, &image).is_ok() {
                            photo = Some(image.clone());
                            *last_photo.lock().unwrap() = Some(image);
                        }
                        next_photo += 1;
                    }
                    if elapsed >= 100_000_000 {
                        break;
                    }
                    sample.SetSampleTime(elapsed)?;
                    writer.WriteSample(stream, &sample)?;
                    if let (Some((_, audio)), Some(audio_stream)) = (&audio, audio_stream) {
                        if audio_ended {
                            continue;
                        }
                        while audio_time < elapsed {
                            let mut flags = 0;
                            let mut timestamp = 0;
                            let mut audio_sample = None;
                            audio.ReadSample(
                                MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32,
                                0,
                                None,
                                Some(&mut flags),
                                Some(&mut timestamp),
                                Some(&mut audio_sample),
                            )?;
                            if flags
                                & (MF_SOURCE_READERF_ERROR.0
                                    | MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0)
                                    as u32
                                != 0
                            {
                                return Err(windows::core::Error::empty());
                            }
                            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                                audio_ended = true;
                                break;
                            }
                            if flags & MF_SOURCE_READERF_STREAMTICK.0 as u32 != 0 {
                                let start = *first_audio.get_or_insert(timestamp);
                                audio_time = timestamp.saturating_sub(start);
                                writer.SendStreamTick(audio_stream, audio_time)?;
                                audio_discontinuity = true;
                            }
                            if let Some(audio_sample) = audio_sample {
                                let start = *first_audio.get_or_insert(timestamp);
                                audio_time = timestamp.saturating_sub(start);
                                audio_sample.SetSampleTime(audio_time)?;
                                if audio_discontinuity {
                                    audio_sample.SetUINT32(&MFSampleExtension_Discontinuity, 1)?;
                                    audio_discontinuity = false;
                                }
                                if audio_sample.GetSampleDuration().unwrap_or(0) <= 0 {
                                    let (channels, rate) =
                                        audio_format.ok_or_else(windows::core::Error::empty)?;
                                    let frames = i64::from(audio_sample.GetTotalLength()?)
                                        / (i64::from(channels) * 2);
                                    if frames == 0 {
                                        continue;
                                    }
                                    audio_sample
                                        .SetSampleDuration(frames * 10_000_000 / i64::from(rate))?;
                                }
                                writer.WriteSample(audio_stream, &audio_sample)?;
                            }
                        }
                    }
                }
            }
            if first.is_none() {
                return Err(windows::core::Error::empty());
            }
            writer.Finalize()?;
            Ok(())
        })();
        if let Some((audio, _)) = audio {
            let _ = audio.Shutdown();
        }
        let _ = source.Shutdown();
        result
    })();
    let _ = (api.shutdown)();
    if result.is_err() {
        let _ = std::fs::remove_file(path);
    }
    (photo, result.is_ok())
}

unsafe fn save_jpeg(
    sample: &windows::Win32::Media::MediaFoundation::IMFSample,
    width: u32,
    height: u32,
    stride: u32,
    path: &std::path::Path,
) -> windows::core::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::{Error, PCWSTR};
    use windows::Win32::Foundation::GENERIC_WRITE;
    use windows::Win32::Graphics::Imaging::*;
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};

    let buffer = sample.ConvertToContiguousBuffer()?;
    let mut data = null_mut();
    let mut length = 0;
    buffer.Lock(&mut data, None, Some(&mut length))?;
    let result = (|| {
        let (w, h, s) = (width as usize, height as usize, stride as usize);
        if data.is_null()
            || w == 0
            || h == 0
            || w % 2 != 0
            || h % 2 != 0
            || s < w
            || s.checked_mul(h + h / 2).is_none_or(|n| n > length as usize)
        {
            return Err(Error::empty());
        }
        let nv12 = std::slice::from_raw_parts(data, length as usize);
        let size = w
            .checked_mul(h)
            .and_then(|n| n.checked_mul(3))
            .ok_or_else(Error::empty)?;
        let mut bgr = vec![0u8; size];
        for y in 0..h {
            for x in 0..w {
                let luma = nv12[y * s + x] as i32 - 16;
                let uv = s * h + (y / 2) * s + (x / 2) * 2;
                let u = nv12[uv] as i32 - 128;
                let v = nv12[uv + 1] as i32 - 128;
                let base = (y * w + x) * 3;
                bgr[base] = ((298 * luma + 516 * u + 128) >> 8).clamp(0, 255) as u8;
                bgr[base + 1] = ((298 * luma - 100 * u - 208 * v + 128) >> 8).clamp(0, 255) as u8;
                bgr[base + 2] = ((298 * luma + 409 * v + 128) >> 8).clamp(0, 255) as u8;
            }
        }
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let stream = factory.CreateStream()?;
        let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        stream.InitializeFromFilename(PCWSTR(path.as_ptr()), GENERIC_WRITE.0)?;
        let encoder = factory.CreateEncoder(&GUID_ContainerFormatJpeg, std::ptr::null())?;
        encoder.Initialize(&stream, WICBitmapEncoderNoCache)?;
        let mut frame = None;
        encoder.CreateNewFrame(&mut frame, std::ptr::null_mut())?;
        let frame = frame.ok_or_else(Error::empty)?;
        frame.Initialize(None)?;
        frame.SetSize(width, height)?;
        let mut format = GUID_WICPixelFormat24bppBGR;
        frame.SetPixelFormat(&mut format)?;
        if format != GUID_WICPixelFormat24bppBGR {
            return Err(Error::empty());
        }
        frame.WritePixels(height, width * 3, &bgr)?;
        frame.Commit()?;
        encoder.Commit()
    })();
    let _ = buffer.Unlock();
    if result.is_err() {
        let _ = std::fs::remove_file(path);
    }
    result
}
