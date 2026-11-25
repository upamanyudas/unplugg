//! AAC-LC through FAAC 2.2, shipped beside unplugg as libfaac.so (LGPL: it can be
//! swapped for another build) and loaded at runtime; without it the video is silent.

use std::ffi::{c_char, c_int, c_void, CString};

use super::utils;

#[repr(C)]
struct Params {
    struct_size: u32,
    sample_rate: u32,
    num_channels: u32,
    mpeg_version: u32,
    object_type: u32,
    joint_mode: u32,
    use_lfe: bool,
    use_tns: bool,
    use_pns: bool,
    reserved: [u8; 1],
    bit_rate: u32,
    bandwidth: u32,
    quant_quality: u32,
    output_format: u32,
    input_format: u32,
    short_control: u32,
    channel_map: *const i32,
    channel_map_count: u32,
    max_bit_rate: u32,
    rate_control: u32,
}

#[repr(C)]
#[derive(Default)]
struct Info {
    struct_size: u32,
    frame_samples: u32,
    max_output_bytes: u32,
    sample_rate: u32,
    object_type: u32,
    mpeg_version: u32,
    bit_rate: u32,
    bandwidth: u32,
    quant_quality: u32,
    max_bit_rate: u32,
    encoder_delay: u32,
    rate_control: u32,
}

const OBJECT_LOW: u32 = 2; // AAC-LC
const STREAM_RAW: u32 = 0; // frames without ADTS headers; the MP4 carries the config
const INPUT_16BIT: u32 = 1;
const BIT_RATE: u32 = 96_000; // per channel

unsafe extern "C" {
    fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

/// (AudioSpecificConfig, raw AAC frames) for interleaved 16-bit PCM.
pub fn encode(pcm: &[i16], rate: u32, channels: u32) -> Option<(Vec<u8>, Vec<Vec<u8>>)> {
    type Init = unsafe extern "C" fn(*mut Params, u32) -> c_int;
    type Open = unsafe extern "C" fn(*const Params, *mut *mut c_void) -> c_int;
    type Close = unsafe extern "C" fn(*mut *mut c_void) -> c_int;
    type GetInfo = unsafe extern "C" fn(*mut c_void, *mut Info) -> c_int;
    type Asc = unsafe extern "C" fn(*mut c_void, *mut *const u8, *mut u32) -> c_int;
    type Encode = unsafe extern "C" fn(*mut c_void, *const c_void, u32, *mut u8, u32, *mut u32) -> c_int;

    let path = CString::new(utils::library_path("libfaac.so").to_string_lossy().as_bytes()).ok()?;
    unsafe {
        let library = dlopen(path.as_ptr(), 2);
        if library.is_null() {
            return None;
        }
        let symbol = |name: &std::ffi::CStr| Some(dlsym(library, name.as_ptr())).filter(|f| !f.is_null());
        let init: Init = std::mem::transmute(symbol(c"faac_params_init")?);
        let open: Open = std::mem::transmute(symbol(c"faac_encoder_open")?);
        let close: Close = std::mem::transmute(symbol(c"faac_encoder_close")?);
        let get_info: GetInfo = std::mem::transmute(symbol(c"faac_encoder_get_info")?);
        let asc: Asc = std::mem::transmute(symbol(c"faac_encoder_asc")?);
        let encode: Encode = std::mem::transmute(symbol(c"faac_encoder_encode")?);

        let mut params: Params = std::mem::zeroed();
        if init(&mut params, size_of::<Params>() as u32) != 0 {
            return None;
        }
        params.sample_rate = rate;
        params.num_channels = channels;
        params.object_type = OBJECT_LOW;
        params.bit_rate = BIT_RATE;
        params.output_format = STREAM_RAW;
        params.input_format = INPUT_16BIT;
        let mut encoder = std::ptr::null_mut();
        if open(&params, &mut encoder) != 0 {
            return None;
        }
        let mut info = Info { struct_size: size_of::<Info>() as u32, ..Default::default() };
        let (mut config, mut length) = (std::ptr::null(), 0);
        let mut frames = Vec::new();
        if get_info(encoder, &mut info) == 0 && asc(encoder, &mut config, &mut length) == 0 {
            let config = std::slice::from_raw_parts(config, length as usize).to_vec();
            let mut out = vec![0u8; info.max_output_bytes as usize];
            let mut written = 0;
            for chunk in pcm.chunks((info.frame_samples * channels) as usize) {
                if encode(encoder, chunk.as_ptr().cast(), chunk.len() as u32, out.as_mut_ptr(), out.len() as u32, &mut written) != 0 {
                    break;
                }
                if written > 0 {
                    frames.push(out[..written as usize].to_vec());
                }
            }
            // flush what it holds back
            while encode(encoder, std::ptr::null(), 0, out.as_mut_ptr(), out.len() as u32, &mut written) == 0 && written > 0 {
                frames.push(out[..written as usize].to_vec());
            }
            close(&mut encoder);
            return Some((config, frames));
        }
        close(&mut encoder);
        None
    }
}
