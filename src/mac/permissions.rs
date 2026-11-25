use std::cell::{Cell, RefCell};
use std::sync::Mutex;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{Bool, ProtocolObject};
use objc2::{define_class, msg_send, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::NSWorkspace;
use objc2_av_foundation::{AVCaptureDevice, AVMediaType, AVMediaTypeAudio, AVMediaTypeVideo};
use objc2_core_location::{CLAuthorizationStatus, CLLocationManager, CLLocationManagerDelegate};
use objc2_foundation::{
    ns_string, NSBundle, NSObject, NSObjectProtocol, NSOperationQueue, NSString, NSURL,
};

static LISTENER: Mutex<Option<fn()>> = Mutex::new(None);
thread_local! {
    static LOCATION: RefCell<Option<(Retained<CLLocationManager>, Retained<LocationDelegate>)>> = const { RefCell::new(None) };
    static LOCATION_STATUS: Cell<isize> = const { Cell::new(0) };
}

define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    struct LocationDelegate;

    unsafe impl NSObjectProtocol for LocationDelegate {}
    unsafe impl CLLocationManagerDelegate for LocationDelegate {
        #[unsafe(method(locationManager:didChangeAuthorizationStatus:))]
        unsafe fn changed(&self, _manager: &CLLocationManager, status: CLAuthorizationStatus) {
            LOCATION_STATUS.with(|value| value.set(status.0 as isize));
            changed();
        }
    }
);

pub fn set_listener(listener: fn()) {
    *LISTENER.lock().unwrap() = Some(listener);
}

fn changed() {
    let listener = *LISTENER.lock().unwrap();
    if let Some(listener) = listener {
        listener();
    }
}

pub fn init() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    LOCATION.with(|location| {
        if location.borrow().is_none() {
            let delegate: Retained<LocationDelegate> =
                unsafe { msg_send![super(LocationDelegate::alloc(mtm).set_ivars(())), init] };
            let manager = unsafe { CLLocationManager::new() };
            unsafe { manager.setDelegate(Some(ProtocolObject::from_ref(&*delegate))) };
            *location.borrow_mut() = Some((manager, delegate));
        }
    });
}

fn media(key: &str) -> Option<&'static AVMediaType> {
    unsafe {
        match key {
            "camera" => AVMediaTypeVideo,
            "microphone" => AVMediaTypeAudio,
            _ => None,
        }
    }
}

pub fn status(key: &str) -> &'static str {
    if let Some(media) = media(key) {
        return match unsafe { AVCaptureDevice::authorizationStatusForMediaType(media) }.0 {
            3 => "granted",
            1 | 2 => "denied",
            _ => "unset",
        };
    }
    if key == "location" {
        if NSBundle::mainBundle()
            .objectForInfoDictionaryKey(ns_string!("NSLocationUsageDescription"))
            .is_none()
        {
            return "via-terminal";
        }
        return LOCATION_STATUS.with(|value| match value.get() {
            3 | 4 => "granted",
            1 | 2 => "denied",
            _ => "unset",
        });
    }
    "n/a"
}

pub fn request(key: &str) {
    if let Some(media) = media(key) {
        let handler = RcBlock::new(|_: Bool| {
            let update = RcBlock::new(changed);
            unsafe { NSOperationQueue::mainQueue().addOperationWithBlock(&update) };
        });
        unsafe { AVCaptureDevice::requestAccessForMediaType_completionHandler(media, &handler) };
    } else if key == "location" {
        LOCATION.with(|location| {
            if let Some((manager, _)) = location.borrow().as_ref() {
                unsafe { manager.requestWhenInUseAuthorization() };
            }
        });
    } else {
        open_settings(key);
    }
}

pub fn open_settings(key: &str) {
    let pane = match key {
        "camera" => "Privacy_Camera",
        "microphone" => "Privacy_Microphone",
        "location" => "Privacy_LocationServices",
        _ => "Privacy",
    };
    let url = NSString::from_str(&format!(
        "x-apple.systempreferences:com.apple.preference.security?{pane}"
    ));
    if let Some(url) = NSURL::URLWithString(&url) {
        NSWorkspace::sharedWorkspace().openURL(&url);
    }
}
