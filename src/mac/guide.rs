use std::cell::Cell;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{sel, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSAutoresizingMaskOptions, NSBackingStoreType, NSColor, NSImageView,
    NSTextAlignment, NSView, NSWindow, NSWindowStyleMask, NSWindowTitleVisibility,
};
use objc2_foundation::{NSPoint, NSSize, NSString};

use super::app::App;
use super::prefs::{
    self, button, group, group_views, info, label, page, permissions_page, rect, refresh_perms,
    switch, telegram_group, tile, Prefs, Steps, GUIDE, INSET, PAD, TALL, W,
};

const ICON: f64 = 72.0;
const FOOTER: f64 = 56.0;

type Color = fn() -> Retained<NSColor>;
type Body = fn(&Prefs, &NSView, f64) -> f64;

const HOW: [(&str, &str, &str, Color); 4] = [
    ("Plug in", "unplugg watches the power cable and the lid.", "bolt.fill", NSColor::systemGreenColor),
    ("Activate", "Click unplugg in the menu bar before you step away.", "lock.fill", NSColor::systemBlueColor),
    ("If it's unplugged or moved", "A siren sounds. Photos, video and location go to your phone.", "bell.fill", NSColor::systemRedColor),
    ("Only you can stop it", "Turning it off asks for Touch ID or your password.", "touchid", NSColor::systemPinkColor),
];

fn how_it_works(_: &Prefs, view: &NSView, y: f64) -> f64 {
    let width = W - 2.0 * PAD - 2.0 * INSET;
    let rows = HOW
        .iter()
        .map(|(title, line, symbol, color)| (info(title, line, &tile(symbol, &color()), width), None))
        .collect();
    group_views(view, y, None, rows, TALL)
}

fn all_set(ctrl: &Prefs, view: &NSView, y: f64) -> f64 {
    group(view, y, None, vec![
        ("Launch at login", switch(ctrl, "launch_at_login")),
        ("Stay awake with the lid closed", switch(ctrl, "block_lid_sleep")),
    ], prefs::ROW)
}

const STEPS: [(&str, &str, Body); 4] = [
    ("Welcome to unplugg", "Your Mac, guarded while you step away.", how_it_works),
    ("Permissions", "Each one turns on one feature. Skip any you don't need.", permissions_page),
    (
        "Get alerts on your phone",
        "When your Mac is disturbed, a Telegram bot sends you photos, video and where it is. Click + to add one.",
        telegram_group,
    ),
    ("You're all set", "Before you step away, click unplugg in the menu bar and choose Activate.", all_set),
];

/// Opens on first launch, and from the menu's Setup Guide…. Closing it early still counts as seen.
pub fn open(app: &App) {
    let ctrl = GUIDE.with(|guide| guide.borrow().clone()).unwrap_or_else(|| {
        let ctrl = build(app);
        ctrl.ivars().ui.borrow().window.as_ref().unwrap().center();
        GUIDE.with(|guide| *guide.borrow_mut() = Some(ctrl.clone()));
        ctrl
    });
    prefs::front(&ctrl);
}

fn build(app: &App) -> Retained<Prefs> {
    let ctrl = Prefs::new(app);
    let mtm = ctrl.mtm();
    let pages: Vec<_> = STEPS.iter().map(|(title, subtitle, body)| step_page(&ctrl, title, subtitle, *body)).collect();
    // show() fits the window to each step
    let height = pages[0].frame().size.height + FOOTER;
    let root = page(mtm, height);
    for step in &pages {
        // the root is flipped, so this pins the step to the top
        step.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMaxYMargin);
        root.addSubview(step);
    }

    // a dot per step on the left, Back / Continue on the right
    let dots: Vec<_> = STEPS.iter().map(|_| label("●", 10.0, None, false, None)).collect();
    for (i, dot) in dots.iter().enumerate() {
        dot.setFrameOrigin(NSPoint::new(PAD + 14.0 * i as f64, height - (FOOTER + dot.frame().size.height) / 2.0));
    }
    let back = button("Back", &ctrl, sel!(back:));
    let next = button("Continue", &ctrl, sel!(next:));
    // the default button
    next.setKeyEquivalent(&NSString::from_str("\r"));
    for (i, btn) in [&next, &back].into_iter().enumerate() {
        let size = NSSize::new(96.0, btn.frame().size.height);
        btn.setFrameSize(size);
        btn.setFrameOrigin(NSPoint::new(W - PAD - (size.width + 8.0) * i as f64 - size.width, height - (FOOTER + size.height) / 2.0));
    }
    let mut footer: Vec<Retained<NSView>> = dots.iter().map(|dot| Retained::into_super(Retained::into_super(dot.clone()))).collect();
    footer.extend([&back, &next].map(|btn| Retained::into_super(Retained::into_super(btn.clone()))));
    for view in &footer {
        // pinned to the bottom
        view.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
        root.addSubview(view);
    }

    let style = NSWindowStyleMask::Titled | NSWindowStyleMask::Closable | NSWindowStyleMask::FullSizeContentView;
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(NSWindow::alloc(mtm), rect(0.0, 0.0, W, height), style, NSBackingStoreType::Buffered, false)
    };
    // hidden, but named in the Window menu and to VoiceOver
    window.setTitle(&NSString::from_str("Setup Guide"));
    window.setTitleVisibility(NSWindowTitleVisibility::Hidden);
    window.setTitlebarAppearsTransparent(true);
    window.setMovableByWindowBackground(true);
    window.setContentView(Some(&root));
    unsafe { window.setReleasedWhenClosed(false) };
    window.setDelegate(Some(ProtocolObject::from_ref(&*ctrl)));
    {
        let mut ui = ctrl.ivars().ui.borrow_mut();
        ui.window = Some(window);
        ui.steps = Some(Steps { pages, dots, back, next, step: Cell::new(0) });
    }
    show(&ctrl, 0);
    refresh_perms(&ctrl);
    ctrl
}

/// App icon, centred title and subtitle, then the step's rows.
fn step_page(ctrl: &Prefs, title: &str, subtitle: &str, body: Body) -> Retained<NSView> {
    let mtm = ctrl.mtm();
    let view = page(mtm, 0.0);
    if let Some(image) = NSApplication::sharedApplication(mtm).applicationIconImage() {
        let icon = NSImageView::imageViewWithImage(&image, mtm);
        // clear of the traffic lights
        icon.setFrame(rect((W - ICON) / 2.0, 36.0, ICON, ICON));
        view.addSubview(&icon);
    }
    let mut y = 36.0 + ICON + 14.0;
    for (text, size, bold, color) in [(title, 22.0, true, None), (subtitle, 13.0, false, Some(NSColor::secondaryLabelColor()))] {
        let line = label(text, size, color.as_deref(), bold, Some(W - 4.0 * PAD));
        line.setAlignment(NSTextAlignment::Center);
        line.setFrameOrigin(NSPoint::new(2.0 * PAD, y));
        view.addSubview(&line);
        y += line.frame().size.height + 6.0;
    }
    let height = body(ctrl, &view, y + 18.0);
    view.setFrameSize(NSSize::new(W, height));
    view
}

fn show(ctrl: &Prefs, step: usize) {
    let ui = ctrl.ivars().ui.borrow();
    let steps = ui.steps.as_ref().unwrap();
    steps.step.set(step);
    for (i, (page, dot)) in steps.pages.iter().zip(&steps.dots).enumerate() {
        page.setHidden(i != step);
        let color = if i == step { NSColor::controlAccentColor() } else { NSColor::tertiaryLabelColor() };
        dot.setTextColor(Some(&color));
    }
    steps.back.setHidden(step == 0);
    let last = step == STEPS.len() - 1;
    steps.next.setTitle(&NSString::from_str(if last { "Done" } else { "Continue" }));
    // fit the step, keeping the top edge still, as Settings does between panes
    let window = ui.window.as_ref().unwrap();
    let frame = window.frame();
    let height = steps.pages[step].frame().size.height + FOOTER;
    let fitted = rect(frame.origin.x, frame.origin.y + frame.size.height - height, W, height);
    window.setFrame_display_animate(fitted, true, window.isVisible());
}

pub(super) fn next(ctrl: &Prefs) {
    let (step, window) = {
        let ui = ctrl.ivars().ui.borrow();
        (ui.steps.as_ref().unwrap().step.get(), ui.window.clone().unwrap())
    };
    if step == STEPS.len() - 1 {
        window.close();
    } else {
        show(ctrl, step + 1);
    }
}

pub(super) fn back(ctrl: &Prefs) {
    let step = ctrl.ivars().ui.borrow().steps.as_ref().unwrap().step.get();
    show(ctrl, step - 1);
}
