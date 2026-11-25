//! PulseAudio's simple API, which PipeWire serves too. Loaded at runtime, so
//! unplugg still starts where it's missing; the siren and mic just stay quiet.

use std::ffi::{c_char, c_int, c_void, CString};
use std::sync::OnceLock;

const S16LE: c_int = 3;

#[repr(C)]
struct Spec {
    format: c_int,
    rate: u32,
    channels: u8,
}

type New = unsafe extern "C" fn(*const c_char, *const c_char, c_int, *const c_char, *const c_char, *const Spec, *const c_void, *const c_void, *mut c_int) -> *mut c_void;
type Io = unsafe extern "C" fn(*mut c_void, *mut c_void, usize, *mut c_int) -> c_int;
type Flush = unsafe extern "C" fn(*mut c_void, *mut c_int) -> c_int;
type Free = unsafe extern "C" fn(*mut c_void);

struct Api {
    new: New,
    write: Io,
    read: Io,
    flush: Flush,
    free: Free,
}

unsafe extern "C" {
    fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

fn api() -> Option<&'static Api> {
    static API: OnceLock<Option<Api>> = OnceLock::new();
    API.get_or_init(|| unsafe {
        let library = dlopen(c"libpulse-simple.so.0".as_ptr(), 2);
        if library.is_null() {
            return None;
        }
        let symbol = |name: &std::ffi::CStr| Some(dlsym(library, name.as_ptr())).filter(|f| !f.is_null());
        Some(Api {
            new: std::mem::transmute::<*mut c_void, New>(symbol(c"pa_simple_new")?),
            write: std::mem::transmute::<*mut c_void, Io>(symbol(c"pa_simple_write")?),
            read: std::mem::transmute::<*mut c_void, Io>(symbol(c"pa_simple_read")?),
            flush: std::mem::transmute::<*mut c_void, Flush>(symbol(c"pa_simple_flush")?),
            free: std::mem::transmute::<*mut c_void, Free>(symbol(c"pa_simple_free")?),
        })
    })
    .as_ref()
}

/// A 16-bit stream to a sink, or from a source; None is the default device.
pub struct Stream {
    handle: *mut c_void,
    api: &'static Api,
}

// each stream lives on the one thread that plays or records it
unsafe impl Send for Stream {}

impl Stream {
    pub fn open(record: bool, device: Option<&str>, rate: u32, channels: u8, what: &str) -> Option<Self> {
        let api = api()?;
        let device = device.map(CString::new).transpose().ok()?;
        let what = CString::new(what).ok()?;
        let spec = Spec { format: S16LE, rate, channels };
        let direction = if record { 2 } else { 1 };
        let mut error = 0;
        let handle = unsafe {
            (api.new)(std::ptr::null(), c"unplugg".as_ptr(), direction, device.as_ref().map_or(std::ptr::null(), |d| d.as_ptr()), what.as_ptr(), &spec, std::ptr::null(), std::ptr::null(), &mut error)
        };
        (!handle.is_null()).then_some(Self { handle, api })
    }

    pub fn write(&mut self, samples: &[i16]) -> bool {
        let mut error = 0;
        unsafe { (self.api.write)(self.handle, samples.as_ptr() as *mut c_void, size_of_val(samples), &mut error) == 0 }
    }

    pub fn read(&mut self, samples: &mut [i16]) -> bool {
        let mut error = 0;
        unsafe { (self.api.read)(self.handle, samples.as_mut_ptr().cast(), size_of_val(samples), &mut error) == 0 }
    }

    /// Drops what's queued, so a stop is heard at once.
    pub fn flush(&mut self) {
        let mut error = 0;
        unsafe { (self.api.flush)(self.handle, &mut error) };
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        unsafe { (self.api.free)(self.handle) };
    }
}
