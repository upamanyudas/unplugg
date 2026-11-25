use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{c_char, c_void};
use std::ptr::{self, NonNull};
use std::rc::Rc;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol, ProtocolObject};
use objc2::{class, define_class, msg_send, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAppearance, NSAppearanceNameDarkAqua, NSApplication,
    NSApplicationDidChangeScreenParametersNotification, NSBackingStoreType, NSColor, NSEvent,
    NSEventMask, NSFont, NSFontWeightSemibold, NSImageScaling, NSImageView, NSScreen,
    NSScreenSaverWindowLevel, NSTextAlignment, NSTextField, NSView, NSVisualEffectBlendingMode,
    NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView, NSWindow,
    NSWindowCollectionBehavior, NSWindowStyleMask, NSWorkspace,
};
use objc2_foundation::{
    NSNotification, NSNotificationCenter, NSOperationQueue, NSPoint, NSRect, NSSize, NSString,
    NSTimer,
};

struct Cover {
    window: Retained<CoverWindow>,
    dot: Retained<NSView>,
    text: Retained<NSTextField>,
    white: Retained<NSView>,
}

#[derive(Default)]
struct State {
    covers: Vec<Cover>,
    monitors: Vec<Retained<AnyObject>>,
    observer: Option<Retained<ProtocolObject<dyn NSObjectProtocol>>>,
    callback: Option<Rc<dyn Fn()>>,
    brightness: HashMap<u32, f32>,
    alarm: bool,
    flashing: bool,
    generation: u64,
}

thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }

define_class!(
    #[unsafe(super(NSWindow))]
    #[thread_kind = objc2::MainThreadOnly]
    struct CoverWindow;
    unsafe impl NSObjectProtocol for CoverWindow {}
    impl CoverWindow {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key(&self) -> bool { true }
    }
);

unsafe extern "C" {
    fn dlopen(path: *const c_char, mode: i32) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

type GetBrightness = unsafe extern "C" fn(u32, *mut f32) -> i32;
type SetBrightness = unsafe extern "C" fn(u32, f32) -> i32;

fn display_services() -> Option<(GetBrightness, SetBrightness)> {
    unsafe {
        let handle = dlopen(
            c"/System/Library/PrivateFrameworks/DisplayServices.framework/DisplayServices".as_ptr(),
            1,
        );
        if handle.is_null() {
            return None;
        }
        let get = dlsym(handle, c"DisplayServicesGetBrightness".as_ptr());
        let set = dlsym(handle, c"DisplayServicesSetBrightness".as_ptr());
        if get.is_null() || set.is_null() {
            return None;
        }
        Some((
            std::mem::transmute::<*mut c_void, GetBrightness>(get),
            std::mem::transmute::<*mut c_void, SetBrightness>(set),
        ))
    }
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
}

pub fn show(callback: Rc<dyn Fn()>, alarm: bool, flash: bool) {
    let mtm = MainThreadMarker::new().expect("main thread");
    let first = STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.alarm = alarm;
        state.flashing |= flash;
        if state.observer.is_some() {
            return false;
        }
        state.callback = Some(callback);
        let block = RcBlock::new(|_: NonNull<NSNotification>| rebuild());
        state.observer = Some(unsafe {
            NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
                Some(NSApplicationDidChangeScreenParametersNotification),
                None,
                Some(&NSOperationQueue::mainQueue()),
                &block,
            )
        });
        true
    });
    if first {
        rebuild();
        NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
        listen();
    } else {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            set_badges(&state);
            if flash {
                flash_screens(&mut state);
            }
        });
    }
}

fn rebuild() {
    let mtm = MainThreadMarker::new().expect("main thread");
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        for cover in state.covers.drain(..) {
            cover.window.orderOut(None);
        }
        for screen in NSScreen::screens(mtm).iter() {
            state.covers.push(cover(&screen, mtm));
        }
        for cover in state.covers.iter().skip(1) {
            cover.window.orderFrontRegardless();
        }
        if let Some(cover) = state.covers.first() {
            cover.window.makeKeyAndOrderFront(None);
        }
        set_badges(&state);
        if state.flashing {
            flash_screens(&mut state);
        }
    });
}

fn color(view: &NSView, color: &NSColor) {
    view.setWantsLayer(true);
    unsafe {
        let layer: *mut AnyObject = msg_send![view, layer];
        let cg: *mut c_void = msg_send![color, CGColor];
        let _: () = msg_send![layer, setBackgroundColor: cg];
    }
}

fn cover(screen: &NSScreen, mtm: MainThreadMarker) -> Cover {
    let frame = screen.frame();
    let (w, h) = (frame.size.width, frame.size.height);
    let window: Retained<CoverWindow> = unsafe {
        msg_send![CoverWindow::alloc(mtm), initWithContentRect: frame, styleMask: NSWindowStyleMask::Borderless, backing: NSBackingStoreType::Buffered, defer: false]
    };
    window.setLevel(NSScreenSaverWindowLevel - 1);
    window.setAcceptsMouseMovedEvents(true);
    window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::FullScreenAuxiliary,
    );
    unsafe {
        let _: () = msg_send![&*window, setAppearance: NSAppearance::appearanceNamed(NSAppearanceNameDarkAqua).as_deref()];
    }
    window.setOpaque(false);
    window.setBackgroundColor(Some(&NSColor::clearColor()));
    let blur =
        NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), rect(0., 0., w, h));
    blur.setMaterial(NSVisualEffectMaterial::FullScreenUI);
    blur.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    blur.setState(NSVisualEffectState::Active);
    window.setContentView(Some(&blur));
    let logo = NSImageView::initWithFrame(
        NSImageView::alloc(mtm),
        rect(w / 2. - 80., h / 2. + 20., 160., 160.),
    );
    logo.setImage(
        NSApplication::sharedApplication(mtm)
            .applicationIconImage()
            .as_deref(),
    );
    logo.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
    blur.addSubview(&logo);
    blur.addSubview(&label(
        "Protected by unplugg",
        34.,
        rect(0., h / 2. - 36., w, 44.),
        true,
        true,
        mtm,
    ));
    let instruction = label(
        "Press any key or move the mouse to unlock.",
        15.,
        rect(0., h / 2. - 70., w, 22.),
        false,
        true,
        mtm,
    );
    instruction.setTextColor(Some(&NSColor::secondaryLabelColor()));
    blur.addSubview(&instruction);
    let y = h / 2. - 116.;
    let text = label("", 13., rect(0., y, 0., 18.), false, false, mtm);
    text.setTextColor(Some(&NSColor::secondaryLabelColor()));
    let dot = NSView::initWithFrame(NSView::alloc(mtm), rect(0., y + 4., 10., 10.));
    dot.setWantsLayer(true);
    unsafe {
        let layer: *mut AnyObject = msg_send![&*dot, layer];
        let _: () = msg_send![layer, setCornerRadius: 5.0f64];
        if !NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion() {
            let animation: *mut AnyObject = msg_send![class!(CABasicAnimation), animationWithKeyPath: &*NSString::from_str("opacity")];
            let from: *mut AnyObject = msg_send![class!(NSNumber), numberWithDouble: 1.0f64];
            let to: *mut AnyObject = msg_send![class!(NSNumber), numberWithDouble: 0.25f64];
            let _: () = msg_send![animation, setFromValue: from];
            let _: () = msg_send![animation, setToValue: to];
            let _: () = msg_send![animation, setDuration: 1.0f64];
            let _: () = msg_send![animation, setAutoreverses: true];
            let _: () = msg_send![animation, setRepeatCount: f32::INFINITY];
            let _: () =
                msg_send![layer, addAnimation: animation, forKey: &*NSString::from_str("pulse")];
        }
    }
    blur.addSubview(&dot);
    blur.addSubview(&text);
    let white = NSView::initWithFrame(NSView::alloc(mtm), rect(0., 0., w, h));
    color(&white, &NSColor::whiteColor());
    white.setHidden(true);
    blur.addSubview(&white);
    Cover {
        window,
        dot,
        text,
        white,
    }
}

fn label(
    text: &str,
    size: f64,
    frame: NSRect,
    bold: bool,
    center: bool,
    mtm: MainThreadMarker,
) -> Retained<NSTextField> {
    let label = NSTextField::labelWithString(&NSString::from_str(text), mtm);
    label.setFrame(frame);
    if center {
        label.setAlignment(NSTextAlignment::Center);
    }
    let font = if bold {
        NSFont::systemFontOfSize_weight(size, unsafe { NSFontWeightSemibold })
    } else {
        NSFont::systemFontOfSize(size)
    };
    label.setFont(Some(&font));
    label
}

fn set_badges(state: &State) {
    let badge_color = if state.alarm {
        NSColor::systemRedColor()
    } else {
        NSColor::systemGreenColor()
    };
    for cover in &state.covers {
        color(&cover.dot, &badge_color);
        cover
            .text
            .setStringValue(&NSString::from_str(if state.alarm {
                "Alarm triggered"
            } else {
                "Armed"
            }));
        cover.text.sizeToFit();
        let x = (cover.window.frame().size.width - cover.text.frame().size.width - 16.) / 2.;
        cover
            .dot
            .setFrameOrigin(NSPoint::new(x, cover.dot.frame().origin.y));
        cover
            .text
            .setFrameOrigin(NSPoint::new(x + 16., cover.text.frame().origin.y));
    }
}

fn flash_screens(state: &mut State) {
    for cover in &state.covers {
        cover.white.setHidden(false);
    }
    let Some((get, set)) = display_services() else {
        return;
    };
    for screen in NSScreen::screens(MainThreadMarker::new().unwrap()).iter() {
        let id: u32 = unsafe {
            let dict: *mut AnyObject = msg_send![&*screen, deviceDescription];
            let number: *mut AnyObject =
                msg_send![dict, objectForKey: &*NSString::from_str("NSScreenNumber")];
            if number.is_null() {
                continue;
            }
            msg_send![number, unsignedIntValue]
        };
        let mut value = 0.;
        if unsafe { get(id, &mut value) } == 0 {
            state.brightness.entry(id).or_insert(value);
            unsafe {
                set(id, 1.);
            }
        }
    }
}

pub fn listen() {
    let generation = STATE.with(|state| state.borrow().generation);
    let block = RcBlock::new(move |_: NonNull<NSTimer>| {
        if STATE.with(|state| state.borrow().generation == generation) {
            watch();
        }
    });
    unsafe {
        NSTimer::scheduledTimerWithTimeInterval_repeats_block(1.5, false, &block);
    }
}

fn watch() {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.observer.is_none() || !state.monitors.is_empty() {
            return;
        }
        let mask = NSEventMask::KeyDown
            | NSEventMask::FlagsChanged
            | NSEventMask::MouseMoved
            | NSEventMask::LeftMouseDown
            | NSEventMask::RightMouseDown
            | NSEventMask::OtherMouseDown
            | NSEventMask::ScrollWheel;
        let local = RcBlock::new(|_: NonNull<NSEvent>| -> *mut NSEvent {
            input();
            ptr::null_mut()
        });
        let global = RcBlock::new(|_: NonNull<NSEvent>| input());
        if let Some(monitor) =
            unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &local) }
        {
            state.monitors.push(monitor);
        }
        if let Some(monitor) = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(mask, &global)
        {
            state.monitors.push(monitor);
        }
    });
}

fn input() {
    let callback = STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.monitors.is_empty() {
            return None;
        }
        unwatch(&mut state);
        state.callback.clone()
    });
    if let Some(callback) = callback {
        let block = RcBlock::new(move |_: NonNull<NSTimer>| callback());
        unsafe {
            NSTimer::scheduledTimerWithTimeInterval_repeats_block(0., false, &block);
        }
    }
}

fn unwatch(state: &mut State) {
    for monitor in state.monitors.drain(..) {
        unsafe {
            NSEvent::removeMonitor(&monitor);
        }
    }
}

pub fn hide() {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        unwatch(&mut state);
        if let Some(observer) = state.observer.take() {
            unsafe {
                let _: () =
                    msg_send![&*NSNotificationCenter::defaultCenter(), removeObserver: &*observer];
            }
        }
        if let Some((_, set)) = display_services() {
            for (id, value) in state.brightness.drain() {
                unsafe {
                    set(id, value);
                }
            }
        }
        for cover in state.covers.drain(..) {
            cover.window.orderOut(None);
        }
        state.callback = None;
        state.flashing = false;
        state.generation = state.generation.wrapping_add(1);
    });
}
