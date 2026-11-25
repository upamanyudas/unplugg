use std::ffi::c_void;

use objc2::rc::Retained;
use objc2_foundation::NSString;

#[repr(C)]
struct Address {
    selector: u32,
    scope: u32,
    element: u32,
}

#[link(name = "CoreAudio", kind = "framework")]
extern "C" {
    fn AudioObjectGetPropertyDataSize(
        id: u32,
        address: *const Address,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: *mut u32,
    ) -> i32;
    fn AudioObjectGetPropertyData(
        id: u32,
        address: *const Address,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: *mut u32,
        data: *mut c_void,
    ) -> i32;
    fn AudioObjectSetPropertyData(
        id: u32,
        address: *const Address,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: u32,
        data: *const c_void,
    ) -> i32;
}

fn address(selector: u32, scope: u32) -> Address {
    Address {
        selector,
        scope,
        element: 0,
    }
}

fn get(id: u32, selector: u32, scope: u32) -> Vec<u8> {
    let address = address(selector, scope);
    let mut size = 0;
    unsafe {
        if AudioObjectGetPropertyDataSize(id, &address, 0, std::ptr::null(), &mut size) != 0 {
            return Vec::new();
        }
        let mut data = vec![0; size as usize];
        if AudioObjectGetPropertyData(
            id,
            &address,
            0,
            std::ptr::null(),
            &mut size,
            data.as_mut_ptr().cast(),
        ) != 0
        {
            return Vec::new();
        }
        data.truncate(size as usize);
        data
    }
}

const GLOBAL: u32 = u32::from_be_bytes(*b"glob");
const OUTPUT: u32 = u32::from_be_bytes(*b"outp");
const DEFAULT_OUTPUT: u32 = u32::from_be_bytes(*b"dOut");

fn outputs() -> Vec<(String, u32)> {
    get(1, u32::from_be_bytes(*b"dev#"), GLOBAL)
        .chunks_exact(4)
        .filter_map(|bytes| {
            let id = u32::from_ne_bytes(bytes.try_into().ok()?);
            if get(id, u32::from_be_bytes(*b"stm#"), OUTPUT).is_empty() {
                return None;
            }
            let raw = get(id, u32::from_be_bytes(*b"lnam"), GLOBAL);
            let pointer = usize::from_ne_bytes(raw.as_slice().try_into().ok()?) as *mut NSString;
            let name = unsafe { Retained::from_raw(pointer)? };
            Some((name.to_string(), id))
        })
        .collect()
}

pub fn speakers() -> Vec<String> {
    outputs().into_iter().map(|(name, _)| name).collect()
}

pub fn current_speaker() -> Option<String> {
    let raw = get(1, DEFAULT_OUTPUT, GLOBAL);
    let id = u32::from_ne_bytes(raw.as_slice().try_into().ok()?);
    outputs()
        .into_iter()
        .find(|(_, output)| *output == id)
        .map(|(name, _)| name)
}

pub fn set_speaker(name: &str) {
    if let Some((_, id)) = outputs().into_iter().find(|(output, _)| output == name) {
        unsafe {
            AudioObjectSetPropertyData(
                1,
                &address(DEFAULT_OUTPUT, GLOBAL),
                0,
                std::ptr::null(),
                4,
                (&id as *const u32).cast(),
            );
        }
    }
}

fn capture_devices(
    media: &objc2_av_foundation::AVMediaType,
) -> Vec<Retained<objc2_av_foundation::AVCaptureDevice>> {
    #[allow(deprecated)]
    unsafe {
        objc2_av_foundation::AVCaptureDevice::devicesWithMediaType(media).to_vec()
    }
}

fn names(media: Option<&objc2_av_foundation::AVMediaType>) -> Vec<String> {
    media
        .map(|media| {
            capture_devices(media)
                .iter()
                .map(|device| unsafe { device.localizedName().to_string() })
                .collect()
        })
        .unwrap_or_default()
}

pub fn cameras() -> Vec<String> {
    names(unsafe { objc2_av_foundation::AVMediaTypeVideo })
}

pub fn microphones() -> Vec<String> {
    names(unsafe { objc2_av_foundation::AVMediaTypeAudio })
}

fn pick(media: Option<&objc2_av_foundation::AVMediaType>, name: &str) -> Option<String> {
    let devices = capture_devices(media?);
    let chosen = devices
        .iter()
        .find(|device| unsafe { device.localizedName().to_string() == name })
        .or_else(|| {
            devices.iter().find(|device| unsafe {
                device.transportType() as u32 == u32::from_be_bytes(*b"bltn")
            })
        })
        .or_else(|| devices.first())?;
    Some(unsafe { chosen.localizedName().to_string() })
}

pub fn camera(name: &str) -> Option<String> {
    pick(unsafe { objc2_av_foundation::AVMediaTypeVideo }, name)
}

pub fn microphone(name: &str) -> Option<String> {
    pick(unsafe { objc2_av_foundation::AVMediaTypeAudio }, name)
}

pub fn device(
    media: &objc2_av_foundation::AVMediaType,
    name: &str,
) -> Option<Retained<objc2_av_foundation::AVCaptureDevice>> {
    capture_devices(media)
        .into_iter()
        .find(|device| unsafe { device.localizedName().to_string() == name })
}
