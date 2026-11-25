use std::ffi::{c_char, c_int, c_void, CString};
use std::path::PathBuf;
use std::process::Command;

use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSImage};
use objc2_foundation::{NSBundle, NSString};

pub fn asset_path(name: &str) -> PathBuf {
    let bundle = NSBundle::mainBundle();
    let bundled = bundle.bundlePath().to_string().ends_with(".app");
    if bundled {
        if let Some(resources) = bundle.resourcePath() {
            return PathBuf::from(resources.to_string())
                .join("assets")
                .join(name);
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join(name)
}

pub fn show_in_dock(show: bool, mtm: MainThreadMarker) {
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(if show {
        NSApplicationActivationPolicy::Regular
    } else {
        NSApplicationActivationPolicy::Accessory
    });
    if !NSBundle::mainBundle()
        .bundlePath()
        .to_string()
        .ends_with(".app")
    {
        use objc2::AllocAnyThread;
        let path = NSString::from_str(&asset_path("logo.png").to_string_lossy());
        let image = NSImage::initWithContentsOfFile(NSImage::alloc(), &path);
        unsafe { app.setApplicationIconImage(image.as_deref()) };
    }
}

pub fn lock_screen() {
    unsafe extern "C" {
        fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
        fn dlclose(handle: *mut c_void) -> c_int;
    }
    let path =
        CString::new("/System/Library/PrivateFrameworks/login.framework/Versions/Current/login")
            .unwrap();
    let symbol = CString::new("SACLockScreenImmediate").unwrap();
    unsafe {
        let handle = dlopen(path.as_ptr(), 1);
        if !handle.is_null() {
            let function = dlsym(handle, symbol.as_ptr());
            if !function.is_null() {
                let lock: unsafe extern "C" fn() = std::mem::transmute(function);
                lock();
                dlclose(handle);
                return;
            }
            dlclose(handle);
        }
    }
    let _ = Command::new("osascript")
        .args([
            "-e",
            "tell application \"System Events\" to key code 12 using {command down, control down}",
        ])
        .status();
}
