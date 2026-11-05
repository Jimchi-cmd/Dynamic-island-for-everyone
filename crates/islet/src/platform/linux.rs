//! Linux picks its window system at startup: native Wayland where the
//! compositor offers wlr layer-shell, otherwise X11 (including XWayland on
//! GNOME). `ISLET_BACKEND=x11` or `=wayland` forces one.

use super::wayland::Wayland;
use super::x11::X11;
use super::{Backend, Event, Result, Waker, WindowSpec};
use islet_core::geometry::Rect;
use std::time::Instant;
use tiny_skia::Pixmap;

pub enum Linux {
    X11(Box<X11>),
    Wayland(Box<Wayland>),
}

impl Backend for Linux {
    fn create(spec: WindowSpec) -> Result<Self> {
        let forced = std::env::var("ISLET_BACKEND").ok();
        let want_wayland = match forced.as_deref() {
            Some("x11") => false,
            Some("wayland") => true,
            _ => std::env::var_os("WAYLAND_DISPLAY").is_some(),
        };
        if want_wayland {
            match Wayland::create(spec) {
                Ok(w) => return Ok(Linux::Wayland(Box::new(w))),
                Err(e) if forced.as_deref() == Some("wayland") => return Err(e),
                Err(e) => crate::providers::debug_log(&format!("using X11 ({e})")),
            }
        }
        X11::create(spec).map(|x| Linux::X11(Box::new(x)))
    }

    fn scale(&self) -> f64 {
        match self {
            Linux::X11(x) => x.scale(),
            Linux::Wayland(w) => w.scale(),
        }
    }

    fn size_px(&self) -> (u32, u32) {
        match self {
            Linux::X11(x) => x.size_px(),
            Linux::Wayland(w) => w.size_px(),
        }
    }

    fn present(&mut self, frame: &Pixmap) -> Result<()> {
        match self {
            Linux::X11(x) => x.present(frame),
            Linux::Wayland(w) => w.present(frame),
        }
    }

    fn set_input_region(&mut self, rect: Rect) -> Result<()> {
        match self {
            Linux::X11(x) => x.set_input_region(rect),
            Linux::Wayland(w) => w.set_input_region(rect),
        }
    }

    fn wait(&mut self, deadline: Option<Instant>, out: &mut Vec<Event>) -> Result<()> {
        match self {
            Linux::X11(x) => x.wait(deadline, out),
            Linux::Wayland(w) => w.wait(deadline, out),
        }
    }

    fn waker(&self) -> Waker {
        match self {
            Linux::X11(x) => x.waker(),
            Linux::Wayland(w) => w.waker(),
        }
    }

    fn set_hotkey(&mut self, hotkey: Option<islet_core::hotkey::Hotkey>) -> Result<()> {
        match self {
            Linux::X11(x) => x.set_hotkey(hotkey),
            // Wayland gives global keys only to the compositor: bind a key to
            // `islet toggle` in its settings instead.
            Linux::Wayland(_) if hotkey.is_some() => {
                Err("on Wayland, bind a key to `islet toggle` in your desktop's keyboard settings".into())
            }
            Linux::Wayland(_) => Ok(()),
        }
    }

    fn screen_origin(&self) -> (f64, f64) {
        match self {
            Linux::X11(x) => x.screen_origin(),
            // Wayland doesn't tell clients where they are.
            Linux::Wayland(_) => (0.0, 0.0),
        }
    }
}
