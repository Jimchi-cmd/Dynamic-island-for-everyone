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
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{define_class, msg_send, AnyThread, ClassType, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSColor, NSDragOperation, NSDraggingInfo,
    NSEvent, NSEventMask, NSEventModifierFlags, NSEventType, NSPanel, NSPasteboardTypeFileURL, NSResponder, NSScreen,
    NSStatusWindowLevel, NSTrackingArea, NSTrackingAreaOptions, NSView, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_core_foundation::CFRetained;
use objc2_core_graphics::{
    kCGColorSpaceSRGB, CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGDataProvider, CGImage, CGImageAlphaInfo,
    CGImageByteOrderInfo,
};
use objc2_foundation::{NSArray, NSDate, NSDefaultRunLoopMode, NSObject, NSPoint, NSRect, NSSize, NSString, NSURL};
use objc2_quartz_core::CATransaction;
use std::cell::{Cell, RefCell};
use std::time::Instant;
use tiny_skia::Pixmap;

thread_local! {
    static QUEUE: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
    static CAPS: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
    static A11Y_LABEL: RefCell<String> = RefCell::new("Islet".to_string());
    /// The input region (logical px, top-left origin) and the panel, for
    /// click-through: the panel ignores the mouse except over the island.
    static REGION: Cell<(f64, f64, f64, f64)> = const { Cell::new((0.0, 0.0, 0.0, 0.0)) };
    static PANEL: RefCell<Option<Retained<NSPanel>>> = const { RefCell::new(None) };
    static INTERACTIVE: Cell<bool> = const { Cell::new(false) };
}

fn in_region(x: f64, y: f64) -> bool {
    let (rx, ry, rw, rh) = REGION.with(|r| r.get());
    x >= rx && x < rx + rw && y >= ry && y < ry + rh
}

/// Let the mouse reach the island (true) or pass through to the apps
/// below (false).
fn set_interactive(on: bool) {
    if INTERACTIVE.with(|i| i.replace(on)) != on {
        PANEL.with(|p| {
            if let Some(panel) = &*p.borrow() {
                panel.setIgnoresMouseEvents(!on);
            }
        });
    }
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
            if !in_region(x, y) {
                set_interactive(false); // off the island: let clicks through again
            }
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, _event: &NSEvent) {
            push(Event::PointerLeave);
            set_interactive(false);
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

        // VoiceOver: the island is one element that reads what it shows and
        // can be pressed (open or close).
        #[unsafe(method(isAccessibilityElement))]
        fn is_accessibility_element(&self) -> bool {
            true
        }

        #[unsafe(method_id(accessibilityRole))]
        fn accessibility_role(&self) -> Option<Retained<NSString>> {
            Some(NSString::from_str("AXButton"))
        }

        #[unsafe(method_id(accessibilityLabel))]
        fn accessibility_label(&self) -> Option<Retained<NSString>> {
            Some(NSString::from_str(&A11Y_LABEL.with(|l| l.borrow().clone())))
        }

        #[unsafe(method(accessibilityPerformPress))]
        fn accessibility_perform_press(&self) -> bool {
            push(Event::A11yPress(None));
            true
        }

        // File drops for the shelf.
        #[unsafe(method(draggingEntered:))]
        fn dragging_entered(&self, info: &ProtocolObject<dyn NSDraggingInfo>) -> NSDragOperation {
            self.drag_at(info);
            NSDragOperation::Copy
        }

        #[unsafe(method(draggingUpdated:))]
        fn dragging_updated(&self, info: &ProtocolObject<dyn NSDraggingInfo>) -> NSDragOperation {
            self.drag_at(info);
            NSDragOperation::Copy
        }

        #[unsafe(method(draggingExited:))]
        fn dragging_exited(&self, _info: Option<&ProtocolObject<dyn NSDraggingInfo>>) {
            push(Event::DragLeave);
        }

        #[unsafe(method(performDragOperation:))]
        fn perform_drag(&self, info: &ProtocolObject<dyn NSDraggingInfo>) -> bool {
            let classes = NSArray::from_slice(&[NSURL::class()]);
            // SAFETY: asks the pasteboard for NSURL objects only.
            let objects = unsafe { info.draggingPasteboard().readObjectsForClasses_options(&classes, None) };
            let paths: Vec<String> = objects
                .map(|a| a.to_vec())
                .unwrap_or_default()
                .into_iter()
                .filter_map(|o| o.downcast::<NSURL>().ok())
                .filter(|u| u.isFileURL())
                .filter_map(|u| u.path().map(|p| p.to_string()))
                .collect();
            let ok = !paths.is_empty();
            push(if ok { Event::Drop(paths) } else { Event::DragLeave });
            ok
        }
    }
);

impl IsletView {
    fn drag_at(&self, info: &ProtocolObject<dyn NSDraggingInfo>) {
        // Window coordinates → the (flipped) view's top-left coordinates.
        let p = self.convertPoint_fromView(info.draggingLocation(), None);
        set_interactive(true);
        push(Event::DragOver { x: p.x, y: p.y });
    }

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
    notch: Option<(f64, f64)>,
    /// Keeps the global mouse monitor registered.
    _monitor: Option<Retained<AnyObject>>,
    _caps_monitor: Option<Retained<AnyObject>>,
    /// Whether another app is fullscreen on the island's display.
    fullscreen: bool,
    /// How frames reach Core Animation (ISLET_MAC_PRESENT, for measuring):
    /// a CGImage per frame, or two IOSurfaces taken in turn.
    present_mode: PresentMode,
    surfaces: Vec<iosurface::Surface>,
    next_surface: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PresentMode {
    Image,
    /// IOSurface rows written top-down, or bottom-up (`flip`).
    Surface {
        flip: bool,
    },
}

impl PresentMode {
    fn from_env() -> Self {
        match std::env::var("ISLET_MAC_PRESENT").as_deref() {
            Ok("iosurface") => PresentMode::Surface { flip: false },
            Ok("iosurface-flip") => PresentMode::Surface { flip: true },
            _ => PresentMode::Image,
        }
    }
}

/// The camera notch of `screen`: the gap between the menu-bar areas on
/// either side of it (macOS 12+), with the safe-area height.
fn notch_of(screen: &NSScreen) -> Option<(f64, f64)> {
    let top = screen.safeAreaInsets().top;
    if top <= 0.0 {
        return None;
    }
    let (left, right) = (screen.auxiliaryTopLeftArea(), screen.auxiliaryTopRightArea());
    let width = screen.frame().size.width - left.size.width - right.size.width;
    (width > 0.0 && left.size.width > 0.0).then_some((width, top))
}

impl Backend for Mac {
    fn create(spec: WindowSpec) -> Result<Self> {
        let mtm = MainThreadMarker::new().ok_or("the island must run on the main thread")?;
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        app.finishLaunching();

        // The display chosen in Settings, else the primary one (with the
        // menu bar), which is the first screen.
        let screens = NSScreen::screens(mtm);
        let wanted = super::preferred_display();
        let screen = screens
            .iter()
            .find(|s| wanted.as_deref() == Some(s.localizedName().to_string().as_str()))
            .or_else(|| screens.firstObject())
            .ok_or("no screens")?;
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
        // SAFETY: reading an AppKit constant.
        view.registerForDraggedTypes(&NSArray::from_slice(&[unsafe { NSPasteboardTypeFileURL }]));
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
        // Click-through: ignore the mouse until it moves over the island. A
        // global monitor sees moves destined for other apps (no permission
        // is needed for mouse events) and turns the island interactive.
        panel.setIgnoresMouseEvents(true);
        PANEL.with(|p| *p.borrow_mut() = Some(panel.clone()));
        let watch = panel.clone();
        let handler = block2::RcBlock::new(move |_event: std::ptr::NonNull<NSEvent>| {
            let p = NSEvent::mouseLocation();
            let f = watch.frame();
            let (x, y) = (p.x - f.origin.x, f.origin.y + f.size.height - p.y);
            if in_region(x, y) {
                set_interactive(true);
                push(Event::PointerMove { x, y });
            }
        });
        let monitor = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(
            NSEventMask::MouseMoved | NSEventMask::LeftMouseDragged,
            &handler,
        );
        // Caps Lock: modifier-flag changes only (no key presses are read).
        let caps_handler = block2::RcBlock::new(move |event: std::ptr::NonNull<NSEvent>| {
            // SAFETY: AppKit passes a valid event for the duration of the call.
            let flags = unsafe { event.as_ref() }.modifierFlags();
            let on = flags.contains(objc2_app_kit::NSEventModifierFlags::CapsLock);
            if CAPS.with(|c| c.replace(Some(on))) != Some(on) {
                push(Event::CapsLock(on));
            }
        });
        CAPS.with(|c| {
            c.set(Some(NSEvent::modifierFlags_class().contains(objc2_app_kit::NSEventModifierFlags::CapsLock)))
        });
        let caps_monitor =
            NSEvent::addGlobalMonitorForEventsMatchingMask_handler(NSEventMask::FlagsChanged, &caps_handler);
        panel.orderFrontRegardless();

        // SAFETY: reading a CoreGraphics constant.
        let color_space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))
            .or_else(CGColorSpace::new_device_rgb)
            .ok_or("no colour space")?;
        let size = ((spec.width * scale).round() as u32, (spec.height * scale).round() as u32);
        let notch = notch_of(&screen);
        Ok(Self {
            app,
            panel,
            view,
            color_space,
            scale,
            size,
            notch,
            _monitor: monitor,
            _caps_monitor: caps_monitor,
            fullscreen: false,
            present_mode: PresentMode::from_env(),
            surfaces: Vec::new(),
            next_surface: 0,
        })
    }

    fn show(&mut self) {
        fullscreen::start();
    }

    fn scale(&self) -> f64 {
        self.scale
    }

    fn set_a11y_label(&mut self, label: &str) {
        A11Y_LABEL.with(|l| {
            if *l.borrow() != label {
                *l.borrow_mut() = label.to_string();
            }
        });
    }

    fn size_px(&self) -> (u32, u32) {
        self.size
    }

    fn present(&mut self, frame: &Pixmap) -> Result<()> {
        if let PresentMode::Surface { flip } = self.present_mode {
            return self.present_surface(frame, flip);
        }
        let (w, h) = (frame.width() as usize, frame.height() as usize);
        // Convert straight into a pooled buffer that the image owns until
        // Core Graphics lets it go (no per-frame allocation or extra copy).
        let mut buf = take_buffer(frame.data().len());
        islet_render::rgba_to_bgra(frame.data(), &mut buf);
        let size = buf.len();
        let data = Box::into_raw(buf) as *mut u8;
        // SAFETY: `data` is a leaked Box<[u8]> of `size` bytes, handed to the
        // provider, which gives it back to `give_back` exactly once.
        let provider = unsafe { CGDataProvider::with_data(std::ptr::null_mut(), data.cast(), size, Some(give_back)) }
            .ok_or("CGDataProvider failed")?;
        // Premultiplied B,G,R,A bytes = 32-bit little-endian ARGB words.
        let info = CGBitmapInfo(CGImageAlphaInfo::PremultipliedFirst.0 | CGImageByteOrderInfo::Order32Little.0);
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

    fn set_input_region(&mut self, rect: Rect) -> Result<()> {
        REGION.with(|r| r.set((rect.x, rect.y, rect.w, rect.h)));
        Ok(())
    }

    fn notch(&self) -> Option<(f64, f64)> {
        self.notch
    }

    fn set_hotkey(&mut self, hotkey: Option<islet_core::hotkey::Hotkey>) -> Result<()> {
        carbon::set(hotkey)
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
        if fullscreen::take_dirty() {
            let now = self.panel.screen().is_some_and(|s| fullscreen::active(&s));
            if now != self.fullscreen {
                self.fullscreen = now;
                out.push(Event::Fullscreen(now));
            }
        }
        Ok(())
    }
}

/// Frame buffers waiting to be reused (a couple are in flight at once).
static FRAMES: std::sync::Mutex<Vec<Box<[u8]>>> = std::sync::Mutex::new(Vec::new());

/// A buffer of exactly `len` bytes: a pooled one when the size matches.
fn take_buffer(len: usize) -> Box<[u8]> {
    let mut pool = FRAMES.lock().unwrap_or_else(|e| e.into_inner());
    match pool.iter().position(|b| b.len() == len) {
        Some(i) => pool.swap_remove(i),
        None => vec![0u8; len].into_boxed_slice(),
    }
}

/// Core Graphics is done with a frame's pixels: keep the buffer for the next frame.
unsafe extern "C-unwind" fn give_back(
    _info: *mut std::ffi::c_void,
    data: std::ptr::NonNull<std::ffi::c_void>,
    size: usize,
) {
    // SAFETY: `data` and `size` are the Box<[u8]> leaked in `present`.
    let buf = unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(data.as_ptr().cast::<u8>(), size)) };
    let mut pool = FRAMES.lock().unwrap_or_else(|e| e.into_inner());
    if pool.len() < 3 {
        pool.push(buf);
    }
}

/// Is another app fullscreen on the island's display? A fullscreen video
/// or app gets its own Space, so the island re-checks when the active
/// Space changes or another app comes to the front (no polling): it looks
/// for another app's ordinary window covering the whole display.
mod fullscreen {
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, ProtocolObject};
    use objc2::{class, msg_send};
    use objc2_app_kit::NSScreen;
    use objc2_foundation::{NSNotification, NSNotificationCenter, NSObjectProtocol, NSOperationQueue, NSString};
    use std::cell::Cell;
    use std::ffi::c_void;
    use std::ptr::NonNull;

    thread_local! {
        static DIRTY: Cell<bool> = const { Cell::new(true) };
    }

    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct Rect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGWindowListCopyWindowInfo(option: u32, relative_to: u32) -> *const c_void;
        fn CGRectMakeWithDictionaryRepresentation(dict: *const c_void, rect: *mut Rect) -> bool;
        fn CGDisplayBounds(display: u32) -> Rect;
        static kCGWindowLayer: *const c_void;
        static kCGWindowBounds: *const c_void;
        static kCGWindowOwnerPID: *const c_void;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFArrayGetCount(array: *const c_void) -> isize;
        fn CFArrayGetValueAtIndex(array: *const c_void, i: isize) -> *const c_void;
        fn CFDictionaryGetValue(dict: *const c_void, key: *const c_void) -> *const c_void;
        fn CFNumberGetValue(number: *const c_void, kind: isize, out: *mut c_void) -> u8;
        fn CFRelease(cf: *const c_void);
    }

    const ON_SCREEN_ONLY: u32 = 1;
    const EXCLUDE_DESKTOP: u32 = 16;
    const SINT32: isize = 3;

    /// Re-check on Space changes and app switches (call once, on the main thread).
    pub fn start() {
        // SAFETY: NSWorkspace is a class with these methods; the observers
        // stay registered for the process lifetime (kept alive below).
        unsafe {
            let ws: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
            if ws.is_null() {
                return;
            }
            let center: Retained<NSNotificationCenter> = msg_send![ws, notificationCenter];
            let queue = NSOperationQueue::mainQueue();
            let mut observers: Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>> = Vec::new();
            for name in ["NSWorkspaceActiveSpaceDidChangeNotification", "NSWorkspaceDidActivateApplicationNotification"]
            {
                let block = RcBlock::new(|_note: NonNull<NSNotification>| DIRTY.with(|d| d.set(true)));
                observers.push(center.addObserverForName_object_queue_usingBlock(
                    Some(&NSString::from_str(name)),
                    None,
                    Some(&queue),
                    &block,
                ));
            }
            std::mem::forget(observers);
        }
    }

    pub fn take_dirty() -> bool {
        DIRTY.with(|d| d.replace(false))
    }

    fn display_id(screen: &NSScreen) -> Option<u32> {
        // SAFETY: deviceDescription is an NSDictionary; NSScreenNumber an NSNumber.
        unsafe {
            let desc: *mut AnyObject = msg_send![screen, deviceDescription];
            if desc.is_null() {
                return None;
            }
            let key = NSString::from_str("NSScreenNumber");
            let num: *mut AnyObject = msg_send![desc, objectForKey: &*key];
            if num.is_null() {
                return None;
            }
            let id: u32 = msg_send![num, unsignedIntValue];
            Some(id)
        }
    }

    pub fn active(screen: &NSScreen) -> bool {
        let Some(display) = display_id(screen) else { return false };
        let own = std::process::id() as i32;
        // SAFETY: Core Graphics returns a +1 CFArray of CFDictionaries (or
        // null), released below; values are read with their documented types.
        unsafe {
            let bounds = CGDisplayBounds(display);
            let list = CGWindowListCopyWindowInfo(ON_SCREEN_ONLY | EXCLUDE_DESKTOP, 0);
            if list.is_null() {
                return false;
            }
            let mut found = false;
            for i in 0..CFArrayGetCount(list) {
                let w = CFArrayGetValueAtIndex(list, i);
                let number = |key: *const c_void| {
                    let v = CFDictionaryGetValue(w, key);
                    let mut n = 0i32;
                    (!v.is_null() && CFNumberGetValue(v, SINT32, (&mut n as *mut i32).cast()) != 0).then_some(n)
                };
                if number(kCGWindowLayer) != Some(0) || number(kCGWindowOwnerPID) == Some(own) {
                    continue;
                }
                let mut r = Rect::default();
                let dict = CFDictionaryGetValue(w, kCGWindowBounds);
                if dict.is_null() || !CGRectMakeWithDictionaryRepresentation(dict, &mut r) {
                    continue;
                }
                let near = |a: f64, b: f64| (a - b).abs() <= 1.0;
                if near(r.x, bounds.x) && near(r.y, bounds.y) && near(r.w, bounds.w) && near(r.h, bounds.h) {
                    found = true;
                    break;
                }
            }
            CFRelease(list);
            found
        }
    }
}

impl Mac {
    /// Hand the frame over in an IOSurface that Core Animation shows
    /// directly (no image to make and copy each frame). Two surfaces are
    /// used in turn, so the one on screen is never the one being written.
    fn present_surface(&mut self, frame: &Pixmap, flip: bool) -> Result<()> {
        let (w, h) = (frame.width(), frame.height());
        if self.surfaces.len() != 2 || self.surfaces[0].size() != (w, h) {
            self.surfaces.clear();
            for _ in 0..2 {
                self.surfaces.push(iosurface::Surface::new(w, h).ok_or("IOSurfaceCreate failed")?);
            }
        }
        let surface = &self.surfaces[self.next_surface];
        self.next_surface ^= 1;
        surface.write(frame.data(), w as usize, h as usize, flip);
        let layer = self.view.layer().ok_or("view has no layer")?;
        layer.setContentsScale(self.scale);
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        // SAFETY: an IOSurfaceRef is toll-free bridged to the IOSurface
        // class, which `contents` accepts; the layer retains it.
        unsafe { layer.setContents(Some(&*surface.as_object())) };
        CATransaction::commit();
        Ok(())
    }
}

/// IOSurfaces: pixel buffers Core Animation can show without copying.
mod iosurface {
    use objc2::runtime::AnyObject;
    use std::ffi::c_void;

    #[link(name = "IOSurface", kind = "framework")]
    extern "C" {
        fn IOSurfaceCreate(properties: *const c_void) -> *mut c_void;
        fn IOSurfaceLock(buffer: *mut c_void, options: u32, seed: *mut u32) -> i32;
        fn IOSurfaceUnlock(buffer: *mut c_void, options: u32, seed: *mut u32) -> i32;
        fn IOSurfaceGetBaseAddress(buffer: *mut c_void) -> *mut c_void;
        fn IOSurfaceGetBytesPerRow(buffer: *mut c_void) -> usize;
        static kIOSurfaceWidth: *const c_void;
        static kIOSurfaceHeight: *const c_void;
        static kIOSurfaceBytesPerElement: *const c_void;
        static kIOSurfacePixelFormat: *const c_void;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFDictionaryCreateMutable(
            allocator: *const c_void,
            capacity: isize,
            keys: *const u8,
            values: *const u8,
        ) -> *mut c_void;
        fn CFDictionarySetValue(dict: *mut c_void, key: *const c_void, value: *const c_void);
        fn CFNumberCreate(allocator: *const c_void, kind: isize, value: *const c_void) -> *const c_void;
        fn CFRelease(cf: *const c_void);
        static kCFTypeDictionaryKeyCallBacks: u8;
        static kCFTypeDictionaryValueCallBacks: u8;
    }

    const SINT32: isize = 3;
    /// 'BGRA': premultiplied B, G, R, A bytes.
    const BGRA: i32 = 0x4247_5241;

    pub struct Surface {
        ptr: *mut c_void,
        size: (u32, u32),
    }

    impl Surface {
        pub fn new(w: u32, h: u32) -> Option<Self> {
            // SAFETY: a CF dictionary of CF numbers built and released here;
            // IOSurfaceCreate returns a +1 reference (or null), released on drop.
            unsafe {
                let dict = CFDictionaryCreateMutable(
                    std::ptr::null(),
                    0,
                    &kCFTypeDictionaryKeyCallBacks,
                    &kCFTypeDictionaryValueCallBacks,
                );
                if dict.is_null() {
                    return None;
                }
                for (key, value) in [
                    (kIOSurfaceWidth, w as i32),
                    (kIOSurfaceHeight, h as i32),
                    (kIOSurfaceBytesPerElement, 4),
                    (kIOSurfacePixelFormat, BGRA),
                ] {
                    let n = CFNumberCreate(std::ptr::null(), SINT32, (&value as *const i32).cast());
                    CFDictionarySetValue(dict, key, n);
                    CFRelease(n);
                }
                let ptr = IOSurfaceCreate(dict);
                CFRelease(dict);
                (!ptr.is_null()).then_some(Surface { ptr, size: (w, h) })
            }
        }

        pub fn size(&self) -> (u32, u32) {
            self.size
        }

        /// Copy premultiplied RGBA rows in as BGRA (bottom row first when `flip`).
        pub fn write(&self, rgba: &[u8], w: usize, h: usize, flip: bool) {
            // SAFETY: the surface is locked while its rows (bytes_per_row
            // apart, each at least w*4 bytes) are written.
            unsafe {
                if IOSurfaceLock(self.ptr, 0, std::ptr::null_mut()) != 0 {
                    return;
                }
                let base = IOSurfaceGetBaseAddress(self.ptr).cast::<u8>();
                let stride = IOSurfaceGetBytesPerRow(self.ptr);
                for row in 0..h {
                    let src = &rgba[(if flip { h - 1 - row } else { row }) * w * 4..][..w * 4];
                    let dst = std::slice::from_raw_parts_mut(base.add(row * stride), w * 4);
                    islet_render::rgba_to_bgra(src, dst);
                }
                IOSurfaceUnlock(self.ptr, 0, std::ptr::null_mut());
            }
        }

        pub fn as_object(&self) -> *const AnyObject {
            self.ptr as *const AnyObject
        }
    }

    impl Drop for Surface {
        fn drop(&mut self) {
            // SAFETY: our +1 reference from IOSurfaceCreate.
            unsafe { CFRelease(self.ptr) };
        }
    }
}

impl Drop for Mac {
    fn drop(&mut self) {
        self.panel.close();
    }
}

/// Screen names, primary first.
pub fn displays() -> Vec<String> {
    let Some(mtm) = MainThreadMarker::new() else { return Vec::new() };
    NSScreen::screens(mtm).iter().map(|s| s.localizedName().to_string()).collect()
}

/// The global shortcut through Carbon's hot-key API: it needs no
/// Accessibility permission (unlike watching all key presses) and is
/// delivered on the main run loop.
mod carbon {
    use super::push;
    use crate::platform::{Event, Result};
    use islet_core::hotkey::{Hotkey, Key};
    use std::ffi::c_void;

    #[repr(C)]
    struct EventTypeSpec {
        class: u32,
        kind: u32,
    }

    #[repr(C)]
    struct EventHotKeyId {
        signature: u32,
        id: u32,
    }

    type Handler = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> i32;

    #[link(name = "Carbon", kind = "framework")]
    extern "C" {
        fn GetApplicationEventTarget() -> *mut c_void;
        fn InstallEventHandler(
            target: *mut c_void,
            handler: Handler,
            count: u32,
            types: *const EventTypeSpec,
            user_data: *mut c_void,
            out: *mut *mut c_void,
        ) -> i32;
        fn RegisterEventHotKey(
            key_code: u32,
            modifiers: u32,
            id: EventHotKeyId,
            target: *mut c_void,
            options: u32,
            out: *mut *mut c_void,
        ) -> i32;
        fn UnregisterEventHotKey(hot_key: *mut c_void) -> i32;
    }

    const KEYBOARD: u32 = u32::from_be_bytes(*b"keyb");
    const HOT_KEY_PRESSED: u32 = 5;
    const CMD: u32 = 1 << 8;
    const SHIFT: u32 = 1 << 9;
    const OPTION: u32 = 1 << 11;
    const CONTROL: u32 = 1 << 12;

    thread_local! {
        static STATE: std::cell::Cell<(bool, *mut c_void)> = const { std::cell::Cell::new((false, std::ptr::null_mut())) };
    }

    unsafe extern "C" fn pressed(_call: *mut c_void, _event: *mut c_void, _data: *mut c_void) -> i32 {
        push(Event::Hotkey);
        0
    }

    /// macOS virtual key codes (US layout positions).
    fn key_code(key: Key) -> Option<u32> {
        const LETTERS: [u32; 26] = [
            0x00, 0x0B, 0x08, 0x02, 0x0E, 0x03, 0x05, 0x04, 0x22, 0x26, 0x28, 0x25, 0x2E, 0x2D, 0x1F, 0x23, 0x0C, 0x0F,
            0x01, 0x11, 0x20, 0x09, 0x0D, 0x07, 0x10, 0x06,
        ];
        const DIGITS: [u32; 10] = [0x1D, 0x12, 0x13, 0x14, 0x15, 0x17, 0x16, 0x1A, 0x1C, 0x19];
        const FKEYS: [u32; 12] = [0x7A, 0x78, 0x63, 0x76, 0x60, 0x61, 0x62, 0x64, 0x65, 0x6D, 0x67, 0x6F];
        match key {
            Key::Char(c @ 'A'..='Z') => Some(LETTERS[(c as u8 - b'A') as usize]),
            Key::Char(c @ '0'..='9') => Some(DIGITS[(c as u8 - b'0') as usize]),
            Key::Char(_) => None,
            Key::Space => Some(0x31),
            Key::F(n) => FKEYS.get(usize::from(n).wrapping_sub(1)).copied(),
        }
    }

    pub fn set(hotkey: Option<Hotkey>) -> Result<()> {
        let (installed, current) = STATE.with(|s| s.get());
        // SAFETY: Carbon calls on the main thread with valid arguments.
        unsafe {
            if !current.is_null() {
                UnregisterEventHotKey(current);
            }
            STATE.with(|s| s.set((installed, std::ptr::null_mut())));
            let Some(hk) = hotkey else { return Ok(()) };
            let code = key_code(hk.key).ok_or("that key can't be a shortcut")?;
            if !installed {
                let spec = EventTypeSpec { class: KEYBOARD, kind: HOT_KEY_PRESSED };
                let r = InstallEventHandler(
                    GetApplicationEventTarget(),
                    pressed,
                    1,
                    &spec,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                );
                if r != 0 {
                    return Err(format!("couldn't install the shortcut handler ({r})"));
                }
            }
            let mut mods = 0;
            for (on, m) in [(hk.logo, CMD), (hk.shift, SHIFT), (hk.alt, OPTION), (hk.ctrl, CONTROL)] {
                if on {
                    mods |= m;
                }
            }
            let mut out = std::ptr::null_mut();
            let id = EventHotKeyId { signature: u32::from_be_bytes(*b"islt"), id: 1 };
            let r = RegisterEventHotKey(code, mods, id, GetApplicationEventTarget(), 0, &mut out);
            STATE.with(|s| s.set((true, if r == 0 { out } else { std::ptr::null_mut() })));
            if r != 0 {
                return Err(format!("{} is already used by another app", hk.label()));
            }
        }
        Ok(())
    }
}
