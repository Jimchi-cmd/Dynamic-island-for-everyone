//! macOS backend: a borderless, non-activating `NSPanel` above the menu
//! bar whose layer shows our premultiplied pixels as a `CGImage`.
//!
//! - `NSStatusWindowLevel`, on every Space and beside fullscreen apps;
//! - non-activating: clicking the pill never steals focus;
//! - no Dock icon (`Accessory` activation policy);
//! - the loop sleeps in `nextEventMatchingMask:untilDate:`: zero wakeups
//!   when idle.
//!
//! Click-through outside the pill (toggling `ignoresMouseEvents` from a
//! global mouse monitor) and notch handling arrive in Phase 1.

use super::{Backend, Button, Event, Key, Result, Waker, WindowSpec};
use islet_core::geometry::Rect;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, AnyThread, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSColor, NSEvent, NSEventMask,
    NSEventModifierFlags, NSEventType, NSPanel, NSResponder, NSScreen, NSStatusWindowLevel, NSTrackingArea,
    NSTrackingAreaOptions, NSView, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_core_foundation::{CFData, CFRetained};
use objc2_core_graphics::{
    kCGColorSpaceSRGB, CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGDataProvider, CGImage, CGImageAlphaInfo,
};
use objc2_foundation::{NSDate, NSDefaultRunLoopMode, NSObject, NSPoint, NSRect, NSSize};
use objc2_quartz_core::CATransaction;
use std::cell::RefCell;
use std::time::Instant;
use tiny_skia::Pixmap;

thread_local! {
    static QUEUE: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
}

fn push(e: Event) {
    QUEUE.with(|q| q.borrow_mut().push(e));
}

/// macOS virtual key code for Escape.
const KEY_ESCAPE: u16 = 53;

define_class!(
    // SAFETY: NSView has no subclassing requirements and we don't implement Drop.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "IsletIslandView"]
    struct IsletView;

    impl IsletView {
        /// Top-left origin, like every other backend.
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        /// React to the first click even though our panel never becomes key.
        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            let (x, y) = self.local(event);
            push(Event::PointerDown { x, y, button: Button::Primary });
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            let (x, y) = self.local(event);
            push(Event::PointerUp { x, y, button: Button::Primary });
        }

        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, event: &NSEvent) {
            let (x, y) = self.local(event);
            push(Event::PointerDown { x, y, button: Button::Secondary });
        }

        #[unsafe(method(rightMouseUp:))]
        fn right_mouse_up(&self, event: &NSEvent) {
            let (x, y) = self.local(event);
            push(Event::PointerUp { x, y, button: Button::Secondary });
        }

        #[unsafe(method(otherMouseDown:))]
        fn other_mouse_down(&self, event: &NSEvent) {
            let (x, y) = self.local(event);
            push(Event::PointerDown { x, y, button: Button::Middle });
        }

        #[unsafe(method(otherMouseUp:))]
        fn other_mouse_up(&self, event: &NSEvent) {
            let (x, y) = self.local(event);
            push(Event::PointerUp { x, y, button: Button::Middle });
        }

        #[unsafe(method(mouseMoved:))]
        fn mouse_moved(&self, event: &NSEvent) {
            let (x, y) = self.local(event);
            push(Event::PointerMove { x, y });
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, _event: &NSEvent) {
            push(Event::PointerLeave);
        }

        #[unsafe(method(scrollWheel:))]
        fn scroll_wheel(&self, event: &NSEvent) {
            // Trackpads report points; wheels report lines. Normalise to lines.
            let k = if event.hasPreciseScrollingDeltas() { 0.1 } else { 1.0 };
            push(Event::Scroll { dx: -event.scrollingDeltaX() * k, dy: -event.scrollingDeltaY() * k });
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            if event.keyCode() == KEY_ESCAPE {
                push(Event::Key(Key::Escape));
            }
        }
    }
);

impl IsletView {
    fn new(mtm: MainThreadMarker, frame: NSRect) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        // SAFETY: NSView's designated initialiser.
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }

    /// Event location in view coordinates (points = logical px, top-left origin).
    fn local(&self, event: &NSEvent) -> (f64, f64) {
        let p = self.convertPoint_fromView(event.locationInWindow(), None);
        (p.x, p.y)
    }
}

pub struct Mac {
    app: Retained<NSApplication>,
    panel: Retained<NSPanel>,
    view: Retained<IsletView>,
    color_space: CFRetained<CGColorSpace>,
    scale: f64,
    size: (u32, u32),
}

impl Backend for Mac {
    fn create(spec: WindowSpec) -> Result<Self> {
        let mtm = MainThreadMarker::new().ok_or("the island must run on the main thread")?;
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        app.finishLaunching();

        // The primary display (with the menu bar) is the first screen.
        let screen = NSScreen::screens(mtm).firstObject().ok_or("no screens")?;
        let sf = screen.frame();
        let scale = screen.backingScaleFactor();
        let frame = NSRect::new(
            NSPoint::new(sf.origin.x + (sf.size.width - spec.width) / 2.0, sf.origin.y + sf.size.height - spec.height),
            NSSize::new(spec.width, spec.height),
        );

        let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
            NSPanel::alloc(mtm),
            frame,
            NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
            NSBackingStoreType::Buffered,
            false,
        );
        panel.setOpaque(false);
        panel.setBackgroundColor(Some(&NSColor::clearColor()));
        panel.setHasShadow(false);
        panel.setLevel(NSStatusWindowLevel);
        panel.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::IgnoresCycle
                | NSWindowCollectionBehavior::FullScreenAuxiliary,
        );
        panel.setFloatingPanel(true);
        panel.setHidesOnDeactivate(false);
        panel.setBecomesKeyOnlyIfNeeded(true);
        // SAFETY: we keep our own reference; AppKit must not free it on close.
        unsafe { panel.setReleasedWhenClosed(false) };

        let view = IsletView::new(mtm, NSRect::new(NSPoint::new(0.0, 0.0), frame.size));
        view.setWantsLayer(true);
        let tracking = NSTrackingAreaOptions::MouseMoved
            | NSTrackingAreaOptions::MouseEnteredAndExited
            | NSTrackingAreaOptions::ActiveAlways
            | NSTrackingAreaOptions::InVisibleRect;
        // SAFETY: the owner (our view) outlives the tracking area it owns.
        let area = unsafe {
            NSTrackingArea::initWithRect_options_owner_userInfo(
                NSTrackingArea::alloc(),
                NSRect::ZERO,
                tracking,
                Some(&view),
                None,
            )
        };
        view.addTrackingArea(&area);
        panel.setContentView(Some(&view));
        panel.orderFrontRegardless();

        // SAFETY: reading a CoreGraphics constant.
        let color_space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))
            .or_else(CGColorSpace::new_device_rgb)
            .ok_or("no colour space")?;
        let size = ((spec.width * scale).round() as u32, (spec.height * scale).round() as u32);
        Ok(Self { app, panel, view, color_space, scale, size })
    }

    fn scale(&self) -> f64 {
        self.scale
    }

    fn size_px(&self) -> (u32, u32) {
        self.size
    }

    fn present(&mut self, frame: &Pixmap) -> Result<()> {
        let (w, h) = (frame.width() as usize, frame.height() as usize);
        let data = CFData::from_bytes(frame.data());
        let provider = CGDataProvider::with_cf_data(Some(&data)).ok_or("CGDataProvider failed")?;
        // tiny-skia pixels are premultiplied R,G,B,A bytes.
        // Byte order "default" is 0, so the alpha info alone describes the layout.
        let info = CGBitmapInfo(CGImageAlphaInfo::PremultipliedLast.0);
        // SAFETY: the provider holds exactly w*h*4 bytes with the given layout.
        let image = unsafe {
            CGImage::new(
                w,
                h,
                8,
                32,
                w * 4,
                Some(&self.color_space),
                info,
                Some(&provider),
                std::ptr::null(),
                false,
                CGColorRenderingIntent::RenderingIntentDefault,
            )
        }
        .ok_or("CGImageCreate failed")?;
        let layer = self.view.layer().ok_or("view has no layer")?;
        layer.setContentsScale(self.scale);
        CATransaction::begin();
        CATransaction::setDisableActions(true); // no implicit cross-fade
                                                // SAFETY: CGImage is toll-free bridged to an Objective-C object, which
                                                // is what `contents` expects; the layer retains it.
        unsafe {
            let obj = &*(CFRetained::as_ptr(&image).as_ptr() as *const AnyObject);
            layer.setContents(Some(obj));
        }
        CATransaction::commit();
        Ok(())
    }

    fn waker(&self) -> Waker {
        // NSApp lives for the whole process; keep its address, not a Retained
        // (which is main-thread-only), and post from any thread.
        let app = Retained::as_ptr(&self.app) as usize;
        Waker::new(move || {
            let Some(event) =
                NSEvent::otherEventWithType_location_modifierFlags_timestamp_windowNumber_context_subtype_data1_data2(
                    NSEventType::ApplicationDefined,
                    NSPoint::new(0.0, 0.0),
                    NSEventModifierFlags::empty(),
                    0.0,
                    0,
                    None,
                    0,
                    0,
                    0,
                )
            else {
                return;
            };
            // SAFETY: `postEvent:atStart:` is documented as callable from
            // secondary threads; the application object outlives the process loop.
            unsafe {
                let app = app as *const NSApplication;
                let _: () = msg_send![app, postEvent: &*event, atStart: false];
            }
        })
    }

    fn set_input_region(&mut self, _rect: Rect) -> Result<()> {
        // Phase 1: toggle ignoresMouseEvents from a global mouse monitor.
        Ok(())
    }

    fn wait(&mut self, deadline: Option<Instant>, out: &mut Vec<Event>) -> Result<()> {
        let until = match deadline {
            None => NSDate::distantFuture(),
            Some(d) => NSDate::dateWithTimeIntervalSinceNow(d.saturating_duration_since(Instant::now()).as_secs_f64()),
        };
        // SAFETY: reading a Foundation constant.
        let mode = unsafe { NSDefaultRunLoopMode };
        let mut next =
            self.app.nextEventMatchingMask_untilDate_inMode_dequeue(NSEventMask::Any, Some(&until), mode, true);
        while let Some(ev) = next {
            self.app.sendEvent(&ev);
            let now = NSDate::distantPast();
            next = self.app.nextEventMatchingMask_untilDate_inMode_dequeue(NSEventMask::Any, Some(&now), mode, true);
        }
        out.extend(QUEUE.with(|q| std::mem::take(&mut *q.borrow_mut())));
        Ok(())
    }
}

impl Drop for Mac {
    fn drop(&mut self) {
        self.panel.close();
    }
}
