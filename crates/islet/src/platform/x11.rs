//! X11 backend (also used under XWayland, e.g. on GNOME Wayland).
//!
//! - 32-bit ARGB visual so a compositor blends our premultiplied pixels;
//! - `_NET_WM_WINDOW_TYPE_DOCK` + `_NET_WM_STATE_ABOVE/STICKY`: above
//!   normal windows, on every workspace, not in the taskbar, no focus;
//! - XShape input region so clicks outside the pill reach the desktop;
//! - the loop sleeps in poll(2) on the X socket: zero wakeups when idle.

use super::{Backend, Button, Event, Key, Result, Waker, WindowSpec};
use islet_core::geometry::{top_center, Rect};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::Arc;
use std::time::Instant;
use tiny_skia::Pixmap;
use x11rb::connection::{Connection, RequestConnection};
use x11rb::properties::{WmHints, WmSizeHints, WmSizeHintsSpecification};
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::shape::{self, ConnectionExt as _};
use x11rb::protocol::xproto::*;
use x11rb::protocol::Event as XEvent;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

x11rb::atom_manager! {
    Atoms: AtomsCookie {
        WM_PROTOCOLS,
        WM_DELETE_WINDOW,
        UTF8_STRING,
        _NET_WM_NAME,
        _NET_WM_WINDOW_TYPE,
        _NET_WM_WINDOW_TYPE_DOCK,
        _NET_WM_STATE,
        _NET_WM_STATE_ABOVE,
        _NET_WM_STATE_STICKY,
        _NET_WM_STATE_SKIP_TASKBAR,
        _NET_WM_STATE_SKIP_PAGER,
        _NET_WM_DESKTOP,
        _MOTIF_WM_HINTS,
    }
}

pub struct X11 {
    conn: RustConnection,
    win: Window,
    gc: Gcontext,
    depth: u8,
    lsb: bool,
    scale: f64,
    size: (u32, u32),
    atoms: Atoms,
    esc_keycode: Option<Keycode>,
    has_shape: bool,
    /// Frame converted to the server's pixel layout (BGRA on little-endian).
    buf: Vec<u8>,
    /// eventfd that other threads write to wake `poll`.
    wake_fd: Arc<OwnedFd>,
}

fn xe<E: std::fmt::Display>(e: E) -> String {
    format!("X11: {e}")
}

/// Logical-to-physical scale from `Xft.dpi` (set by GNOME, KDE, xrdb).
fn read_scale(conn: &RustConnection, root: Window) -> f64 {
    let Ok(Ok(reply)) =
        conn.get_property(false, root, AtomEnum::RESOURCE_MANAGER, AtomEnum::STRING, 0, 1 << 16).map(|c| c.reply())
    else {
        return 1.0;
    };
    parse_xft_dpi(&String::from_utf8_lossy(&reply.value)).map_or(1.0, |dpi| (dpi / 96.0).clamp(1.0, 4.0))
}

pub(crate) fn parse_xft_dpi(resources: &str) -> Option<f64> {
    resources.lines().find_map(|l| l.strip_prefix("Xft.dpi:")).and_then(|v| v.trim().parse().ok())
}

/// Geometry (x, y, w, h) of the primary monitor.
fn primary_monitor(conn: &RustConnection, screen: &Screen) -> (i32, i32, u32, u32) {
    let fallback = (0, 0, screen.width_in_pixels as u32, screen.height_in_pixels as u32);
    let Ok(Ok(reply)) = conn.randr_get_monitors(screen.root, true).map(|c| c.reply()) else {
        return fallback;
    };
    reply
        .monitors
        .iter()
        .find(|m| m.primary)
        .or_else(|| reply.monitors.first())
        .map_or(fallback, |m| (m.x as i32, m.y as i32, m.width as u32, m.height as u32))
}

fn find_argb_visual(screen: &Screen) -> Option<Visualid> {
    screen
        .allowed_depths
        .iter()
        .filter(|d| d.depth == 32)
        .flat_map(|d| d.visuals.iter())
        .find(|v| v.class == VisualClass::TRUE_COLOR)
        .map(|v| v.visual_id)
}

fn find_keycode(conn: &RustConnection, keysym: u32) -> Option<Keycode> {
    let setup = conn.setup();
    let (min, max) = (setup.min_keycode, setup.max_keycode);
    let map = conn.get_keyboard_mapping(min, max - min + 1).ok()?.reply().ok()?;
    let per = map.keysyms_per_keycode as usize;
    if per == 0 {
        return None;
    }
    map.keysyms.chunks(per).position(|syms| syms.contains(&keysym)).map(|i| min + i as u8)
}

impl Backend for X11 {
    fn create(spec: WindowSpec) -> Result<Self> {
        let (conn, screen_num) = x11rb::connect(None).map_err(xe)?;
        let screen = conn.setup().roots[screen_num].clone();
        let atoms = Atoms::new(&conn).map_err(xe)?.reply().map_err(xe)?;
        let scale = read_scale(&conn, screen.root);
        let size = ((spec.width * scale).round() as u32, (spec.height * scale).round() as u32);
        let (mx, my, mw, mh) = primary_monitor(&conn, &screen);
        let (x, y) = top_center((mx, my), (mw, mh), size.0);

        let (depth, visual) = match find_argb_visual(&screen) {
            Some(v) => (32, v),
            None => {
                eprintln!("islet: no 32-bit visual; the pill will have a black background (no compositor?)");
                (screen.root_depth, screen.root_visual)
            }
        };
        let colormap = conn.generate_id().map_err(xe)?;
        conn.create_colormap(ColormapAlloc::NONE, colormap, screen.root, visual).map_err(xe)?;
        let win = conn.generate_id().map_err(xe)?;
        let events = EventMask::EXPOSURE
            | EventMask::BUTTON_PRESS
            | EventMask::BUTTON_RELEASE
            | EventMask::POINTER_MOTION
            | EventMask::LEAVE_WINDOW
            | EventMask::KEY_PRESS
            | EventMask::STRUCTURE_NOTIFY;
        let aux = CreateWindowAux::new().background_pixel(0).border_pixel(0).colormap(colormap).event_mask(events);
        conn.create_window(
            depth,
            win,
            screen.root,
            x as i16,
            y as i16,
            size.0 as u16,
            size.1 as u16,
            0,
            WindowClass::INPUT_OUTPUT,
            visual,
            &aux,
        )
        .map_err(xe)?;

        // Identity and window-manager hints.
        conn.change_property8(PropMode::REPLACE, win, AtomEnum::WM_NAME, AtomEnum::STRING, b"Islet").map_err(xe)?;
        conn.change_property8(PropMode::REPLACE, win, atoms._NET_WM_NAME, atoms.UTF8_STRING, b"Islet").map_err(xe)?;
        conn.change_property8(PropMode::REPLACE, win, AtomEnum::WM_CLASS, AtomEnum::STRING, b"islet\0Islet\0")
            .map_err(xe)?;
        conn.change_property32(
            PropMode::REPLACE,
            win,
            atoms._NET_WM_WINDOW_TYPE,
            AtomEnum::ATOM,
            &[atoms._NET_WM_WINDOW_TYPE_DOCK],
        )
        .map_err(xe)?;
        conn.change_property32(
            PropMode::REPLACE,
            win,
            atoms._NET_WM_STATE,
            AtomEnum::ATOM,
            &[
                atoms._NET_WM_STATE_ABOVE,
                atoms._NET_WM_STATE_STICKY,
                atoms._NET_WM_STATE_SKIP_TASKBAR,
                atoms._NET_WM_STATE_SKIP_PAGER,
            ],
        )
        .map_err(xe)?;
        conn.change_property32(PropMode::REPLACE, win, atoms._NET_WM_DESKTOP, AtomEnum::CARDINAL, &[0xFFFF_FFFF])
            .map_err(xe)?;
        // Motif hints: flags = decorations, decorations = none.
        conn.change_property32(PropMode::REPLACE, win, atoms._MOTIF_WM_HINTS, atoms._MOTIF_WM_HINTS, &[2, 0, 0, 0, 0])
            .map_err(xe)?;
        conn.change_property32(PropMode::REPLACE, win, atoms.WM_PROTOCOLS, AtomEnum::ATOM, &[atoms.WM_DELETE_WINDOW])
            .map_err(xe)?;
        let mut size_hints = WmSizeHints::new();
        size_hints.position = Some((WmSizeHintsSpecification::UserSpecified, x, y));
        size_hints.min_size = Some((size.0 as i32, size.1 as i32));
        size_hints.max_size = Some((size.0 as i32, size.1 as i32));
        size_hints.set_normal_hints(&conn, win).map_err(xe)?;
        // Never take keyboard focus from the user's app.
        let wm_hints = WmHints { input: Some(false), ..WmHints::new() };
        wm_hints.set(&conn, win).map_err(xe)?;

        let gc = conn.generate_id().map_err(xe)?;
        conn.create_gc(gc, win, &CreateGCAux::new().graphics_exposures(0)).map_err(xe)?;
        let has_shape = conn.extension_information(shape::X11_EXTENSION_NAME).map_err(xe)?.is_some();
        let lsb = conn.setup().image_byte_order == ImageOrder::LSB_FIRST;
        let esc_keycode = find_keycode(&conn, 0xff1b);

        conn.map_window(win).map_err(xe)?;
        conn.flush().map_err(xe)?;
        // SAFETY: eventfd returns a new descriptor (or -1, checked) that we own.
        let raw = unsafe { libc::eventfd(0, libc::EFD_NONBLOCK | libc::EFD_CLOEXEC) };
        if raw < 0 {
            return Err(format!("eventfd: {}", std::io::Error::last_os_error()));
        }
        // SAFETY: `raw` is a valid, owned file descriptor.
        let wake_fd = Arc::new(unsafe { OwnedFd::from_raw_fd(raw) });
        Ok(Self { conn, win, gc, depth, lsb, scale, size, atoms, esc_keycode, has_shape, buf: Vec::new(), wake_fd })
    }

    fn scale(&self) -> f64 {
        self.scale
    }

    fn size_px(&self) -> (u32, u32) {
        self.size
    }

    fn present(&mut self, frame: &Pixmap) -> Result<()> {
        let (w, h) = (frame.width(), frame.height());
        let src = frame.data();
        self.buf.resize(src.len(), 0);
        for (d, s) in self.buf.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
            // tiny-skia: premultiplied R,G,B,A bytes. X: 0xAARRGGBB pixels.
            if self.lsb {
                d.copy_from_slice(&[s[2], s[1], s[0], s[3]]);
            } else {
                d.copy_from_slice(&[s[3], s[0], s[1], s[2]]);
            }
        }
        // Stay under the server's maximum request size.
        let row = w as usize * 4;
        let max = self.conn.maximum_request_bytes().saturating_sub(64).max(row);
        let rows_per = (max / row).max(1);
        let mut y = 0usize;
        while y < h as usize {
            let n = rows_per.min(h as usize - y);
            self.conn
                .put_image(
                    ImageFormat::Z_PIXMAP,
                    self.win,
                    self.gc,
                    w as u16,
                    n as u16,
                    0,
                    y as i16,
                    0,
                    self.depth,
                    &self.buf[y * row..(y + n) * row],
                )
                .map_err(xe)?;
            y += n;
        }
        self.conn.flush().map_err(xe)
    }

    fn set_input_region(&mut self, r: Rect) -> Result<()> {
        if !self.has_shape {
            return Ok(());
        }
        let p = r.to_physical(self.scale);
        let rect = Rectangle { x: p.x as i16, y: p.y as i16, width: p.w.max(0) as u16, height: p.h.max(0) as u16 };
        self.conn
            .shape_rectangles(shape::SO::SET, shape::SK::INPUT, ClipOrdering::UNSORTED, self.win, 0, 0, &[rect])
            .map_err(xe)?;
        self.conn.flush().map_err(xe)
    }

    fn wait(&mut self, deadline: Option<Instant>, out: &mut Vec<Event>) -> Result<()> {
        self.drain(out)?;
        if !out.is_empty() {
            return Ok(());
        }
        let timeout_ms: libc::c_int = match deadline {
            None => -1,
            Some(d) => {
                let left = d.saturating_duration_since(Instant::now());
                // Round up so we never wake a hair early and spin.
                left.as_micros().div_ceil(1000).min(i32::MAX as u128) as libc::c_int
            }
        };
        let mut fds = [
            libc::pollfd { fd: self.conn.stream().as_raw_fd(), events: libc::POLLIN, revents: 0 },
            libc::pollfd { fd: self.wake_fd.as_raw_fd(), events: libc::POLLIN, revents: 0 },
        ];
        // SAFETY: fds is a valid array of two pollfds for the duration of the call.
        let rc = unsafe { libc::poll(fds.as_mut_ptr(), 2, timeout_ms) };
        if rc < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() != std::io::ErrorKind::Interrupted {
                return Err(format!("poll: {err}"));
            }
        }
        if fds[1].revents & libc::POLLIN != 0 {
            let mut counter = [0u8; 8];
            // SAFETY: reading 8 bytes from our non-blocking eventfd into a local buffer.
            unsafe { libc::read(self.wake_fd.as_raw_fd(), counter.as_mut_ptr().cast(), 8) };
        }
        self.drain(out)
    }

    fn waker(&self) -> Waker {
        let fd = self.wake_fd.clone();
        Waker::new(move || {
            let one = 1u64.to_ne_bytes();
            // SAFETY: writing 8 bytes to an eventfd we keep alive via the Arc.
            unsafe { libc::write(fd.as_raw_fd(), one.as_ptr().cast(), 8) };
        })
    }
}

impl X11 {
    fn drain(&mut self, out: &mut Vec<Event>) -> Result<()> {
        while let Some(ev) = self.conn.poll_for_event().map_err(xe)? {
            if let Some(e) = self.translate(ev) {
                out.push(e);
            }
        }
        Ok(())
    }

    fn translate(&self, ev: XEvent) -> Option<Event> {
        let s = self.scale;
        let button = |b: u8| match b {
            1 => Some(Button::Primary),
            2 => Some(Button::Middle),
            3 => Some(Button::Secondary),
            _ => None,
        };
        Some(match ev {
            XEvent::Expose(e) if e.count == 0 => Event::Redraw,
            XEvent::MotionNotify(e) => Event::PointerMove { x: e.event_x as f64 / s, y: e.event_y as f64 / s },
            XEvent::LeaveNotify(_) => Event::PointerLeave,
            XEvent::ButtonPress(e) => match e.detail {
                4 => Event::Scroll { dx: 0.0, dy: -1.0 },
                5 => Event::Scroll { dx: 0.0, dy: 1.0 },
                6 => Event::Scroll { dx: -1.0, dy: 0.0 },
                7 => Event::Scroll { dx: 1.0, dy: 0.0 },
                b => Event::PointerDown { x: e.event_x as f64 / s, y: e.event_y as f64 / s, button: button(b)? },
            },
            XEvent::ButtonRelease(e) => {
                Event::PointerUp { x: e.event_x as f64 / s, y: e.event_y as f64 / s, button: button(e.detail)? }
            }
            XEvent::KeyPress(e) if Some(e.detail) == self.esc_keycode => Event::Key(Key::Escape),
            XEvent::ClientMessage(e)
                if e.type_ == self.atoms.WM_PROTOCOLS && e.data.as_data32()[0] == self.atoms.WM_DELETE_WINDOW =>
            {
                Event::Quit
            }
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::parse_xft_dpi;

    #[test]
    fn parses_xft_dpi() {
        assert_eq!(parse_xft_dpi("Xft.antialias:\t1\nXft.dpi:\t192\nXft.hinting:\t1"), Some(192.0));
        assert_eq!(parse_xft_dpi("Xft.dpi: 120.5\n"), Some(120.5));
        assert_eq!(parse_xft_dpi("Xcursor.size: 24"), None);
    }
}
