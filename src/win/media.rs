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
