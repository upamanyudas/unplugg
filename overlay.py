from pyobjc_framework_AppKit import (
    NSApplication, NSWindow, NSBorderlessWindowMask, NSBackingStoreBuffered,
    NSFloatingWindowLevel, NSScreen, NSImageView, NSImage
)
from pyobjc_framework_Foundation import NSBundle, NSAutoreleasePool

def show_overlay():
    """Show full-screen overlay with logo and text."""
    pool = NSAutoreleasePool.alloc().init()

    # Create window
    frame = NSScreen.mainScreen().frame()
    window = NSWindow.alloc().initWithContentRect_styleMask_backing_defer_(
        frame, NSBorderlessWindowMask, NSBackingStoreBuffered, False
    )
    window.setLevel_(NSFloatingWindowLevel)
    window.setOpaque_(False)
    window.setBackgroundColor_(NSColor.colorWithCalibratedWhite_alpha_(0.7, 0.8))  # Semi-transparent
    window.makeKeyAndOrderFront_(None)
    window.setAcceptsMouseMovedEvents_(True)  # To detect input

    # Add logo and text (simple NSImageView; use NSView for complex)
    logo_view = NSImageView.alloc().initWithFrame_((100, 100, 200, 200))
    logo_img = NSImage.alloc().initWithContentsOfFile_("assets/overlay_bg.png")  # Or composite
    logo_view.setImage_(logo_img)
    window.contentView().addSubview_(logo_view)

    # Activate and wait for input (simplified; use event loop)
    NSApplication.sharedApplication().activateIgnoringOtherApps_(True)

    # To dismiss on input: Would need runloop observer, but for demo, manual quit or timeout
    # In full impl, add NSWindowDelegate to close on key/mouse

    pool.drain()
