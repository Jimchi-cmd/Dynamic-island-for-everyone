//! The thin OS layer: one window that shows premultiplied pixels with
//! per-pixel alpha, takes pointer input only over the pill, stays above
//! other windows and never steals focus. One implementation per OS,
//! selected at compile time; the event loop itself is portable (main.rs).

use islet_core::geometry::Rect;
use std::sync::Arc;
use std::time::Instant;
use tiny_skia::Pixmap;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
mod wayland;
#[cfg(target_os = "linux")]
mod x11;
#[cfg(target_os = "linux")]
pub use self::linux::Linux as Native;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use self::windows::Win32 as Native;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use self::macos::Mac as Native;

pub type Result<T> = std::result::Result<T, String>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Primary,
    Secondary,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Escape,
}

/// Input and window events, in logical px relative to the window.
/// Not every backend produces every event (layered Win32 windows never need
/// a redraw; X11 reads its scale once), hence the dead-code allowance.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum Event {
    PointerMove {
        x: f64,
        y: f64,
    },
    PointerLeave,
    PointerDown {
        x: f64,
        y: f64,
        button: Button,
    },
    PointerUp {
        x: f64,
        y: f64,
        button: Button,
    },
    Scroll {
        dx: f64,
        dy: f64,
    },
    Key(Key),
    /// Caps Lock turned on or off (for the caps-lock HUD).
    CapsLock(bool),
    /// Files are being dragged over the window, at (x, y).
    DragOver {
        x: f64,
        y: f64,
    },
    /// The drag left without dropping.
    DragLeave,
    /// Files were dropped on the window.
    Drop(Vec<String>),
    /// A screen reader pressed the island (None) or one of its buttons.
    A11yPress(Option<usize>),
    /// The OS lost our pixels (expose); present the last frame again.
    Redraw,
    ScaleChanged(f64),
    Quit,
}

/// Size of the overlay window in logical px.
#[derive(Debug, Clone, Copy)]
pub struct WindowSpec {
    pub width: f64,
    pub height: f64,
}

/// Wakes the event loop from another thread (media and battery providers),
/// so updates arrive immediately without the loop ever polling.
#[derive(Clone)]
pub struct Waker(Arc<dyn Fn() + Send + Sync>);

impl Waker {
    pub fn new(f: impl Fn() + Send + Sync + 'static) -> Self {
        Self(Arc::new(f))
    }

    pub fn wake(&self) {
        (self.0)()
    }
}

pub trait Backend: Sized {
    /// Create the window at the top centre of the primary display and show it.
    fn create(spec: WindowSpec) -> Result<Self>;

    /// Physical px per logical px.
    fn scale(&self) -> f64;

    /// Size of the pixel buffer `present` expects.
    fn size_px(&self) -> (u32, u32);

    /// Show a premultiplied RGBA frame of exactly `size_px()`.
    fn present(&mut self, frame: &Pixmap) -> Result<()>;

    /// Accept pointer input only inside `rect` (logical px). Backends whose
    /// OS hit-tests by alpha may ignore this.
    fn set_input_region(&mut self, rect: Rect) -> Result<()>;

    /// Block until at least one event arrives or `deadline` passes, and
    /// append events to `out`. `None` waits indefinitely: when nothing is
    /// animating the process sleeps with zero wakeups.
    fn wait(&mut self, deadline: Option<Instant>, out: &mut Vec<Event>) -> Result<()>;

    /// A thread-safe handle that makes a blocked `wait` return early.
    fn waker(&self) -> Waker;

    /// Start or stop watching Caps Lock (Windows needs a keyboard hook for
    /// it, installed only while the caps-lock pop-up is on). Elsewhere the
    /// lock state comes with the window system's own events.
    fn watch_caps_lock(&mut self, _on: bool) {}

    /// The window's top-left corner on screen, in physical px.
    fn screen_origin(&self) -> (f64, f64) {
        (0.0, 0.0)
    }

    /// The native window handle (HWND on Windows), for accessibility.
    fn native_handle(&self) -> isize {
        0
    }

    /// What VoiceOver reads for the island (macOS handles it in the view).
    fn set_a11y_label(&mut self, _label: &str) {}

    /// The display's camera notch (width, height in logical px), if the
    /// island sits on one (notched MacBooks).
    fn notch(&self) -> Option<(f64, f64)> {
        None
    }
}
