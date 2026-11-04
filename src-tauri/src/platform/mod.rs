//! Platform abstraction. Every OS-specific capability is a trait here, with
//! exactly one implementation per OS selected at compile time. Later phases
//! add `MediaProvider`, `NotificationProvider`, `SystemStatsProvider`, etc.

use crate::geometry::PhysRect;
use tauri::WebviewWindow;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::Native;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::Native;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use self::windows::Native;

pub type Result<T> = std::result::Result<T, String>;

/// Window-level behaviour that Tauri does not expose portably.
pub trait WindowPlacement {
    /// Called once before the window is shown: window level, spaces,
    /// taskbar/dock behaviour.
    fn prepare(window: &WebviewWindow) -> Result<()>;

    /// Restrict mouse input to `rect` (physical px, window-relative). Clicks
    /// anywhere else in the transparent window fall through to the desktop.
    fn set_input_region(window: &WebviewWindow, rect: PhysRect) -> Result<()>;
}

/// Must run before any GUI toolkit initialises.
pub fn early_init() {
    #[cfg(target_os = "linux")]
    linux::early_init();
}
