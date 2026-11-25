use std::ffi::{c_char, c_void};
use std::process::Command;
use std::ptr;
use std::sync::OnceLock;

use crate::monitor::Sensors;

type CFRef = *const c_void;

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOPSGetTimeRemainingEstimate() -> f64;
    fn IOHIDManagerCreate(allocator: CFRef, options: u32) -> CFRef;
    fn IOHIDManagerSetDeviceMatching(manager: CFRef, matching: CFRef);
    fn IOHIDManagerCopyDevices(manager: CFRef) -> CFRef;
    fn IOHIDManagerScheduleWithRunLoop(manager: CFRef, run_loop: CFRef, mode: CFRef);
    fn IOHIDManagerUnscheduleFromRunLoop(manager: CFRef, run_loop: CFRef, mode: CFRef);
    fn IOHIDDeviceGetProperty(device: CFRef, key: CFRef) -> CFRef;
    fn IOHIDDeviceOpen(device: CFRef, options: u32) -> i32;
    fn IOHIDDeviceClose(device: CFRef, options: u32) -> i32;
    fn IOHIDDeviceGetReport(
        device: CFRef,
        kind: u32,
        id: isize,
        report: *mut u8,
        len: *mut isize,
    ) -> i32;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFRunLoopDefaultMode: CFRef;
    fn CFRelease(value: CFRef);
    fn CFRunLoopGetCurrent() -> CFRef;
    fn CFRunLoopRunInMode(mode: CFRef, seconds: f64, return_after_source: bool) -> i32;
    fn CFArrayGetTypeID() -> usize;
    fn CFArrayGetCount(array: CFRef) -> isize;
    fn CFArrayGetValueAtIndex(array: CFRef, index: isize) -> CFRef;
    fn CFDictionaryGetTypeID() -> usize;
    fn CFDictionaryGetValue(dictionary: CFRef, key: CFRef) -> CFRef;
    fn CFSetGetCount(set: CFRef) -> isize;
    fn CFSetGetValues(set: CFRef, values: *mut CFRef);
    fn CFStringCreateWithCString(allocator: CFRef, string: *const c_char, encoding: u32) -> CFRef;
    fn CFGetTypeID(value: CFRef) -> usize;
    fn CFNumberGetTypeID() -> usize;
    fn CFNumberGetValue(number: CFRef, kind: isize, value: *mut c_void) -> bool;
}

struct Owned(CFRef);

impl Drop for Owned {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) }
    }
}

fn string(name: &'static [u8]) -> Option<Owned> {
    let key = unsafe { CFStringCreateWithCString(ptr::null(), name.as_ptr().cast(), 0x08000100) };
    (!key.is_null()).then(|| Owned(key))
}

fn number(value: CFRef) -> Option<i32> {
    unsafe {
        if value.is_null() || CFGetTypeID(value) != CFNumberGetTypeID() {
            return None;
        }
        let mut number = 0i32;
        CFNumberGetValue(value, 3, (&mut number as *mut i32).cast()).then_some(number)
    }
}

fn property(device: CFRef, name: &'static [u8]) -> Option<i32> {
    let key = string(name)?;
    number(unsafe { IOHIDDeviceGetProperty(device, key.0) })
}

fn has_usage(device: CFRef) -> bool {
    if property(device, b"PrimaryUsagePage\0") == Some(0x20)
        && property(device, b"PrimaryUsage\0") == Some(0x8a)
    {
        return true;
    }
    unsafe {
        let (Some(pairs_key), Some(page_key), Some(usage_key)) = (
            string(b"DeviceUsagePairs\0"),
            string(b"DeviceUsagePage\0"),
            string(b"DeviceUsage\0"),
        ) else {
            return false;
        };
        let pairs = IOHIDDeviceGetProperty(device, pairs_key.0);
        if pairs.is_null() || CFGetTypeID(pairs) != CFArrayGetTypeID() {
            return false;
        }
        (0..CFArrayGetCount(pairs)).any(|i| {
            let pair = CFArrayGetValueAtIndex(pairs, i);
            !pair.is_null()
                && CFGetTypeID(pair) == CFDictionaryGetTypeID()
                && number(CFDictionaryGetValue(pair, page_key.0)) == Some(0x20)
                && number(CFDictionaryGetValue(pair, usage_key.0)) == Some(0x8a)
        })
    }
}

struct Scheduled(CFRef, CFRef);

impl Drop for Scheduled {
    fn drop(&mut self) {
        unsafe { IOHIDManagerUnscheduleFromRunLoop(self.0, self.1, kCFRunLoopDefaultMode) }
    }
}

fn read_angle() -> Option<f64> {
    unsafe {
        let manager = IOHIDManagerCreate(ptr::null(), 0);
        if manager.is_null() {
            return None;
        }
        let manager = Owned(manager);
        IOHIDManagerSetDeviceMatching(manager.0, ptr::null());
        let run_loop = CFRunLoopGetCurrent();
        IOHIDManagerScheduleWithRunLoop(manager.0, run_loop, kCFRunLoopDefaultMode);
        let _scheduled = Scheduled(manager.0, run_loop);
        while CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.001, false) == 4 {}
        let devices = IOHIDManagerCopyDevices(manager.0);
        if devices.is_null() {
            return None;
        }
        let devices = Owned(devices);
        let count = CFSetGetCount(devices.0);
        let mut values = vec![ptr::null(); count as usize];
        CFSetGetValues(devices.0, values.as_mut_ptr());
        for device in values {
            if property(device, b"VendorID\0") != Some(0x05ac)
                || property(device, b"ProductID\0") != Some(0x8104)
                || !has_usage(device)
                || IOHIDDeviceOpen(device, 0) != 0
            {
                continue;
            }
            let mut report = [0u8; 8];
            report[0] = 1;
            let mut len = report.len() as isize;
            let test = IOHIDDeviceGetReport(device, 2, 1, report.as_mut_ptr(), &mut len);
            let connected = test == 0 && len >= 3;
            let result = if connected {
                len = report.len() as isize;
                let read = IOHIDDeviceGetReport(device, 2, 1, report.as_mut_ptr(), &mut len);
                if read == 0 && len >= 3 {
                    Some(u16::from_le_bytes([report[1], report[2]]) as f64)
                } else {
                    None
                }
            } else {
                None
            };
            IOHIDDeviceClose(device, 0);
            if connected {
                return result;
            }
        }
        None
    }
}

pub fn has_angle_sensor() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| read_angle().is_some())
}

fn output_contains(program: &str, args: &[&str], text: &str) -> bool {
    Command::new(program)
        .args(args)
        .output()
        .ok()
        .is_some_and(|output| String::from_utf8_lossy(&output.stdout).contains(text))
}

pub fn is_laptop() -> bool {
    output_contains("pmset", &["-g", "batt"], "InternalBattery")
}

pub struct MacSensors;

impl Sensors for MacSensors {
    fn is_plugged_in(&self) -> bool {
        unsafe { IOPSGetTimeRemainingEstimate() == -2.0 }
    }

    fn is_lid_closed(&self) -> bool {
        output_contains(
            "ioreg",
            &["-r", "-k", "AppleClamshellState", "-d", "4"],
            "\"AppleClamshellState\" = Yes",
        )
    }

    fn lid_angle(&self) -> Option<f64> {
        has_angle_sensor().then(read_angle).flatten()
    }

    fn acceleration(&self) -> Option<[f64; 3]> {
        None
    }
}
