use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::{mpsc, Mutex};
use std::time::Duration;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{class, define_class, msg_send, AnyThread, DefinedClass, Encode, Encoding, RefEncode};
use objc2_foundation::{ns_string, NSObject, NSObjectProtocol, NSString, NSURL};
use serde_json::Value;

#[repr(C, packed(4))]
#[derive(Clone, Copy)]
struct Time {
    value: i64,
    scale: i32,
    flags: u32,
    epoch: i64,
}
unsafe impl Encode for Time {
    const ENCODING: Encoding = Encoding::Struct(
        "?",
        &[i64::ENCODING, i32::ENCODING, u32::ENCODING, i64::ENCODING],
    );
}

#[link(name = "CoreMedia", kind = "framework")]
extern "C" {
    fn CMTimeMakeWithSeconds(seconds: f64, scale: i32) -> Time;
}
#[link(name = "ImageIO", kind = "framework")]
extern "C" {
    fn CGImageDestinationCreateWithURL(
        url: *const AnyObject,
        kind: *const AnyObject,
        count: usize,
        options: *const AnyObject,
    ) -> *mut AnyObject;
    fn CGImageDestinationAddImage(
        destination: *mut AnyObject,
        image: *const AnyObject,
        options: *const AnyObject,
    );
    fn CGImageDestinationFinalize(destination: *mut AnyObject) -> bool;
}
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(value: *const std::ffi::c_void);
}
#[link(name = "AVFoundation", kind = "framework")]
extern "C" {
    static AVMediaTypeVideo: &'static NSString;
    static AVMediaTypeAudio: &'static NSString;
    static AVVideoCodecKey: &'static NSString;
    static AVVideoCodecTypeH264: &'static NSString;
    static AVVideoCompressionPropertiesKey: &'static NSString;
    static AVVideoAverageBitRateKey: &'static NSString;
    static AVAssetExportPresetPassthrough: &'static NSString;
    static AVFileTypeMPEG4: &'static NSString;
}

unsafe impl RefEncode for Time {
    const ENCODING_REF: Encoding = Encoding::Pointer(&Self::ENCODING);
}

define_class!(
    #[unsafe(super = NSObject)]
    #[ivars = Mutex<Option<mpsc::Sender<()>>>]
    struct Recorded;
    unsafe impl NSObjectProtocol for Recorded {}
    impl Recorded {
        #[unsafe(method(captureOutput:didFinishRecordingToOutputFileAtURL:fromConnections:error:))]
        unsafe fn finished(&self, _output: *mut AnyObject, _url: *mut AnyObject, _connections: *mut AnyObject, _error: *mut AnyObject) {
            if let Some(sender) = self.ivars().lock().unwrap().take() { let _ = sender.send(()); }
        }
    }
);

fn url(path: &Path) -> Retained<NSURL> {
    NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()))
}

unsafe fn input(media: &NSString, name: &str) -> Option<Retained<AnyObject>> {
    let status: isize = msg_send![class!(AVCaptureDevice), authorizationStatusForMediaType: media];
    if status != 3 {
        return None;
    }
    let devices: Retained<AnyObject> =
        msg_send![class!(AVCaptureDevice), devicesWithMediaType: media];
    let count: usize = msg_send![&*devices, count];
    let mut first: *mut AnyObject = ptr::null_mut();
    let mut builtin: *mut AnyObject = ptr::null_mut();
    let mut selected: *mut AnyObject = ptr::null_mut();
    for i in 0..count {
        let device: *mut AnyObject = msg_send![&*devices, objectAtIndex: i];
        if first.is_null() {
            first = device;
        }
        let transport: u32 = msg_send![device, transportType];
        if builtin.is_null() && transport == u32::from_be_bytes(*b"bltn") {
            builtin = device;
        }
        let label: *mut NSString = msg_send![device, localizedName];
        if !label.is_null() && (*label).to_string() == name {
            selected = device;
            break;
        }
    }
    if selected.is_null() {
        selected = if builtin.is_null() { first } else { builtin };
    }
    if selected.is_null() {
        return None;
    }
    let result: Option<Retained<AnyObject>> = msg_send![class!(AVCaptureDeviceInput), deviceInputWithDevice: selected, error: ptr::null_mut::<*mut AnyObject>()];
    result
}

pub fn capture(config: &Value) -> (Option<PathBuf>, Option<PathBuf>) {
    objc2::rc::autoreleasepool(|_| unsafe { capture_inner(config) })
}

unsafe fn capture_inner(config: &Value) -> (Option<PathBuf>, Option<PathBuf>) {
    let dir = config
        .get("media_dir")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(|s| {
            if s == "~" {
                std::env::var("HOME").unwrap_or_default()
            } else if let Some(rest) = s.strip_prefix("~/") {
                format!("{}/{rest}", std::env::var("HOME").unwrap_or_default())
            } else {
                s.to_owned()
            }
        })
        .unwrap_or_else(|| {
            format!(
                "{}/Pictures/unplugg",
                std::env::var("HOME").unwrap_or_default()
            )
        });
    let dir = PathBuf::from(dir);
    if std::fs::create_dir_all(&dir).is_err() {
        return (None, None);
    }
    let stamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let mov = dir.join(format!("{stamp}.mov"));
    let mp4 = dir.join(format!("{stamp}.mp4"));
    let camera = match input(
        AVMediaTypeVideo,
        config.get("camera").and_then(Value::as_str).unwrap_or(""),
    ) {
        Some(c) => c,
        None => return (None, None),
    };
    let session: Retained<AnyObject> = msg_send![class!(AVCaptureSession), new];
    let _: () = msg_send![&*session, addInput: &*camera];
    if config
        .get("record_audio")
        .and_then(Value::as_bool)
        .unwrap_or(true)
    {
        if let Some(mic) = input(
            AVMediaTypeAudio,
            config.get("mic").and_then(Value::as_str).unwrap_or(""),
        ) {
            let _: () = msg_send![&*session, addInput: &*mic];
        }
    }
    let output: Retained<AnyObject> = msg_send![class!(AVCaptureMovieFileOutput), new];
    let _: () = msg_send![&*output, setMaxRecordedDuration: CMTimeMakeWithSeconds(10.0, 600)];
    let _: () = msg_send![&*session, addOutput: &*output];
    let bitrate: Retained<AnyObject> = msg_send![class!(NSNumber), numberWithInt: 4_000_000i32];
    let compression: Retained<AnyObject> = msg_send![class!(NSDictionary), dictionaryWithObject: &*bitrate, forKey: AVVideoAverageBitRateKey];
    let settings: Retained<AnyObject> = msg_send![class!(NSMutableDictionary), new];
    let _: () = msg_send![&*settings, setObject: AVVideoCodecTypeH264, forKey: AVVideoCodecKey];
    let _: () =
        msg_send![&*settings, setObject: &*compression, forKey: AVVideoCompressionPropertiesKey];
    let connection: *mut AnyObject = msg_send![&*output, connectionWithMediaType: AVMediaTypeVideo];
    let _: () = msg_send![&*output, setOutputSettings: &*settings, forConnection: connection];
    let _: () = msg_send![&*session, startRunning];
    let (tx, rx) = mpsc::channel();
    let delegate: Retained<Recorded> = msg_send![
        super(Recorded::alloc().set_ivars(Mutex::new(Some(tx)))),
        init
    ];
    let _: () = msg_send![&*output, startRecordingToOutputFileURL: &*url(&mov), recordingDelegate: &*delegate];
    let _ = rx.recv_timeout(Duration::from_secs(25));
    let _: () = msg_send![&*session, stopRunning];
    if !mov.exists() {
        return (None, None);
    }
    let source: Retained<AnyObject> = msg_send![class!(AVURLAsset), URLAssetWithURL: &*url(&mov), options: ptr::null::<AnyObject>()];
    let export: Option<Retained<AnyObject>> = msg_send![class!(AVAssetExportSession), exportSessionWithAsset: &*source, presetName: AVAssetExportPresetPassthrough];
    if let Some(export) = export {
        let _: () = msg_send![&*export, setOutputURL: &*url(&mp4)];
        let _: () = msg_send![&*export, setOutputFileType: AVFileTypeMPEG4];
        let (tx, rx) = mpsc::channel();
        let block = RcBlock::new(move || {
            let _ = tx.send(());
        });
        let _: () = msg_send![&*export, exportAsynchronouslyWithCompletionHandler: &*block];
        let _ = rx.recv_timeout(Duration::from_secs(30));
    }
    let video = if mp4.exists() {
        let _ = std::fs::remove_file(&mov);
        mp4
    } else {
        mov
    };
    let asset: Retained<AnyObject> = msg_send![class!(AVURLAsset), URLAssetWithURL: &*url(&video), options: ptr::null::<AnyObject>()];
    let generator: Retained<AnyObject> =
        msg_send![class!(AVAssetImageGenerator), assetImageGeneratorWithAsset: &*asset];
    let mut photo = None;
    for second in 1..=3 {
        let image: *mut AnyObject = msg_send![&*generator, copyCGImageAtTime: CMTimeMakeWithSeconds(second as f64, 600), actualTime: ptr::null_mut::<Time>(), error: ptr::null_mut::<*mut AnyObject>()];
        if image.is_null() {
            continue;
        }
        let path = dir.join(format!("{stamp}_{second}.jpg"));
        let file_url = url(&path);
        let destination = CGImageDestinationCreateWithURL(
            (&*file_url as *const NSURL).cast(),
            (ns_string!("public.jpeg") as *const NSString).cast(),
            1,
            ptr::null(),
        );
        if !destination.is_null() {
            CGImageDestinationAddImage(destination, image, ptr::null());
            let _ = CGImageDestinationFinalize(destination);
            CFRelease(destination.cast());
            photo = Some(path);
        }
        CFRelease(image.cast());
    }
    (photo, Some(video))
}
