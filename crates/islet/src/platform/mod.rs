//! The thin OS layer: one window that shows premultiplied pixels with
//! per-pixel alpha, takes pointer input only over the pill, stays above
//! other windows and never steals focus. One implementation per OS,
//! selected at compile time; the event loop itself is portable (main.rs).

use islet_core::geometry::Rect;
use std::time::Instant;
use tiny_skia::Pixmap;

#[cfg(target_os = "linux")]
mod x11;
#[cfg(target_os = "linux")]
pub use self::x11::X11 as Native;

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
#[derive(Debug, Clone, Copy, PartialEq)]
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
}
