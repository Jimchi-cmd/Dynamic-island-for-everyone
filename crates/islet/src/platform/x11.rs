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
        _NET_WM_STATE_FULLSCREEN,
        _NET_ACTIVE_WINDOW,
        _NET_WM_DESKTOP,
        _MOTIF_WM_HINTS,
        XdndAware,
        XdndEnter,
        XdndPosition,
        XdndStatus,
        XdndLeave,
        XdndDrop,
        XdndFinished,
        XdndSelection,
        XdndActionCopy,
        XdndTypeList,
        TEXT_URI_LIST: b"text/uri-list",
        ISLET_DND,
    }
}

/// An XDND drag over the island: who is dragging, and whether it offers files.
#[derive(Clone, Copy)]
struct Dnd {
    source: Window,
    files: bool,
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
    /// Caps Lock's last known state (XKB), to report only changes.
    caps: std::cell::Cell<Option<bool>>,
    /// The window's position on the root window (for XDND coordinates).
    pos: (i32, i32),
    /// The drag in progress (XDND), if any.
    dnd: std::cell::Cell<Option<Dnd>>,
    /// Frame converted to the server's pixel layout (BGRA on little-endian).
    buf: Vec<u8>,
    /// eventfd that other threads write to wake `poll`.
    wake_fd: Arc<OwnedFd>,
    root: Window,
    /// The grabbed global shortcut: key code and modifiers.
    hotkey: Option<(Keycode, u16)>,
    /// The island's display (root coordinates), the active window being
    /// watched, and whether it's fullscreen on that display.
    monitor: (i32, i32, u32, u32),
    active: std::cell::Cell<Window>,
    fullscreen: std::cell::Cell<bool>,
    fullscreen_checked: std::cell::Cell<bool>,
}

/// Lock-type modifiers (Caps, Num, Scroll) the shortcut must work under.
const LOCK_MODS: [u16; 4] = [0, 2, 16, 18];
/// Shift, Control, Alt (Mod1) and Super (Mod4): what the shortcut checks.
const SHORTCUT_MODS: u16 = 1 | 4 | 8 | 64;

fn hotkey_keysym(key: islet_core::hotkey::Key) -> u32 {
    use islet_core::hotkey::Key;
    match key {
        Key::Char(c) => c.to_ascii_lowercase() as u32,
        Key::Space => 0x20,
        Key::F(n) => 0xffbe + u32::from(n) - 1,
    }
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
/// A RandR monitor: name, primary, rect (x, y, w, h).
type Monitor = (String, bool, (i32, i32, u32, u32));

/// RandR monitors, primary first.
fn monitors(conn: &RustConnection, screen: &Screen) -> Vec<Monitor> {
    let Ok(Ok(reply)) = conn.randr_get_monitors(screen.root, true).map(|c| c.reply()) else {
        return Vec::new();
    };
    let mut v: Vec<_> = reply
        .monitors
        .iter()
        .map(|m| {
            let name = conn
                .get_atom_name(m.name)
                .ok()
                .and_then(|c| c.reply().ok())
                .map(|r| String::from_utf8_lossy(&r.name).into_owned())
                .unwrap_or_default();
            (name, m.primary, (m.x as i32, m.y as i32, m.width as u32, m.height as u32))
        })
        .collect();
    v.sort_by_key(|m| !m.1);
    v
}

/// The chosen monitor (Settings → Look), else the primary one.
fn primary_monitor(conn: &RustConnection, screen: &Screen) -> (i32, i32, u32, u32) {
    let fallback = (0, 0, screen.width_in_pixels as u32, screen.height_in_pixels as u32);
    let all = monitors(conn, screen);
    let wanted = super::preferred_display();
    all.iter().find(|m| wanted.as_deref() == Some(m.0.as_str())).or(all.first()).map_or(fallback, |m| m.2)
}

/// Monitor names, primary first (for `islet displays`).
pub fn displays() -> Vec<String> {
    let Ok((conn, n)) = x11rb::connect(None) else { return Vec::new() };
    let screen = conn.setup().roots[n].clone();
    monitors(&conn, &screen).into_iter().map(|m| m.0).collect()
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
        // Accept file drops (XDND version 5) for the shelf.
        conn.change_property32(PropMode::REPLACE, win, atoms.XdndAware, AtomEnum::ATOM, &[5]).map_err(xe)?;
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
        let caps = std::cell::Cell::new(watch_caps_lock(&conn));
        // Follow the active window (the window manager announces it on the
        // root window) to hide over fullscreen apps.
        conn.change_window_attributes(
            screen.root,
            &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )
        .map_err(xe)?;

        conn.map_window(win).map_err(xe)?;
        conn.flush().map_err(xe)?;
        // SAFETY: eventfd returns a new descriptor (or -1, checked) that we own.
        let raw = unsafe { libc::eventfd(0, libc::EFD_NONBLOCK | libc::EFD_CLOEXEC) };
        if raw < 0 {
            return Err(format!("eventfd: {}", std::io::Error::last_os_error()));
        }
        // SAFETY: `raw` is a valid, owned file descriptor.
        let wake_fd = Arc::new(unsafe { OwnedFd::from_raw_fd(raw) });
        Ok(Self {
            conn,
            win,
            gc,
            depth,
            lsb,
            scale,
            size,
            atoms,
            esc_keycode,
            has_shape,
            caps,
            pos: (x, y),
            dnd: std::cell::Cell::new(None),
            buf: Vec::new(),
            wake_fd,
            root: screen.root,
            hotkey: None,
            monitor: (mx, my, mw, mh),
            active: std::cell::Cell::new(x11rb::NONE),
            fullscreen: std::cell::Cell::new(false),
            fullscreen_checked: std::cell::Cell::new(false),
        })
    }

    fn set_hotkey(&mut self, hotkey: Option<islet_core::hotkey::Hotkey>) -> Result<()> {
        if let Some((code, mods)) = self.hotkey.take() {
            for lock in LOCK_MODS {
                let _ = self.conn.ungrab_key(code, self.root, ModMask::from(mods | lock));
            }
        }
        let Some(hk) = hotkey else { return self.conn.flush().map_err(xe) };
        let code = find_keycode(&self.conn, hotkey_keysym(hk.key)).ok_or("that key isn't on this keyboard")?;
        let mods = (if hk.shift { 1 } else { 0 })
            | (if hk.ctrl { 4 } else { 0 })
            | (if hk.alt { 8 } else { 0 })
            | (if hk.logo { 64 } else { 0 });
        for lock in LOCK_MODS {
            let grab = self.conn.grab_key(
                false,
                self.root,
                ModMask::from(mods | lock),
                code,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
            );
            if grab.map_err(xe)?.check().is_err() {
                for lock in LOCK_MODS {
                    let _ = self.conn.ungrab_key(code, self.root, ModMask::from(mods | lock));
                }
                let _ = self.conn.flush();
                return Err(format!("{} is already used by another app", hk.label()));
            }
        }
        self.hotkey = Some((code, mods));
        self.conn.flush().map_err(xe)
    }

    fn screen_origin(&self) -> (f64, f64) {
        (self.pos.0 as f64, self.pos.1 as f64)
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
        // tiny-skia: premultiplied R,G,B,A bytes. X: 0xAARRGGBB pixels.
        if self.lsb {
            islet_render::rgba_to_bgra(src, &mut self.buf);
        } else {
            for (d, s) in self.buf.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
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

/// Ask XKB for Caps Lock changes (no key events are read: XKB reports the
/// lock state itself). Returns the state now, or None without XKB.
fn watch_caps_lock(conn: &RustConnection) -> Option<bool> {
    use x11rb::protocol::xkb::{self, ConnectionExt as _};
    conn.extension_information(xkb::X11_EXTENSION_NAME).ok()??;
    if !conn.xkb_use_extension(1, 0).ok()?.reply().ok()?.supported {
        return None;
    }
    let spec: xkb::DeviceSpec = xkb::ID::USE_CORE_KBD.into();
    let lock = xkb::StatePart::MODIFIER_LOCK;
    let details = xkb::SelectEventsAux::new()
        .state_notify(xkb::SelectEventsAuxStateNotify { affect_state: lock, state_details: lock });
    conn.xkb_select_events(spec, 0u16.into(), 0u16.into(), 0u16.into(), 0u16.into(), &details).ok()?;
    let state = conn.xkb_get_state(spec).ok()?.reply().ok()?;
    Some(state.locked_mods.contains(ModMask::LOCK))
}

impl X11 {
    /// Re-read the active window and whether it's fullscreen on the
    /// island's display; Some(state) when that changed.
    fn check_fullscreen(&self) -> Option<Event> {
        let a = &self.atoms;
        let active = self
            .conn
            .get_property(false, self.root, a._NET_ACTIVE_WINDOW, AtomEnum::WINDOW, 0, 1)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32().and_then(|mut v| v.next()))
            .unwrap_or(x11rb::NONE);
        if active != self.active.get() {
            let watch = |w: Window, mask: EventMask| {
                if w != x11rb::NONE && w != self.win {
                    let _ = self.conn.change_window_attributes(w, &ChangeWindowAttributesAux::new().event_mask(mask));
                }
            };
            watch(self.active.get(), EventMask::NO_EVENT);
            watch(active, EventMask::PROPERTY_CHANGE);
            self.active.set(active);
        }
        let on = active != x11rb::NONE && active != self.win && self.is_fullscreen(active);
        let _ = self.conn.flush();
        (on != self.fullscreen.replace(on)).then_some(Event::Fullscreen(on))
    }

    /// The window says it's fullscreen and its centre is on our display.
    fn is_fullscreen(&self, w: Window) -> bool {
        let a = &self.atoms;
        let state = self
            .conn
            .get_property(false, w, a._NET_WM_STATE, AtomEnum::ATOM, 0, 64)
            .ok()
            .and_then(|c| c.reply().ok())
            .is_some_and(|r| r.value32().is_some_and(|mut v| v.any(|s| s == a._NET_WM_STATE_FULLSCREEN)));
        if !state {
            return false;
        }
        let Some(g) = self.conn.get_geometry(w).ok().and_then(|c| c.reply().ok()) else { return false };
        let Some(t) = self.conn.translate_coordinates(w, self.root, 0, 0).ok().and_then(|c| c.reply().ok()) else {
            return false;
        };
        let (cx, cy) = (i32::from(t.dst_x) + i32::from(g.width) / 2, i32::from(t.dst_y) + i32::from(g.height) / 2);
        let (mx, my, mw, mh) = self.monitor;
        cx >= mx && cx < mx + mw as i32 && cy >= my && cy < my + mh as i32
    }

    /// Send an XDND client message to the drag source.
    fn dnd_send(&self, to: Window, kind: Atom, data: [u32; 5]) {
        let ev = ClientMessageEvent::new(32, to, kind, data);
        let _ = self.conn.send_event(false, to, EventMask::NO_EVENT, ev);
        let _ = self.conn.flush();
    }

    fn drain(&mut self, out: &mut Vec<Event>) -> Result<()> {
        // Something may already be fullscreen when Islet starts.
        if !self.fullscreen_checked.replace(true) {
            out.extend(self.check_fullscreen());
        }
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
            XEvent::PropertyNotify(e)
                if (e.window == self.root && e.atom == self.atoms._NET_ACTIVE_WINDOW)
                    || (e.window == self.active.get() && e.atom == self.atoms._NET_WM_STATE) =>
            {
                return self.check_fullscreen();
            }
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
            XEvent::KeyPress(e)
                if e.event == self.root
                    && self.hotkey.is_some_and(|(code, mods)| {
                        e.detail == code && (u16::from(e.state) & SHORTCUT_MODS) == mods
                    }) =>
            {
                Event::Hotkey
            }
            XEvent::KeyPress(e) if Some(e.detail) == self.esc_keycode => Event::Key(Key::Escape),
            XEvent::XkbStateNotify(e) => {
                let on = e.locked_mods.contains(ModMask::LOCK);
                if self.caps.replace(Some(on)) == Some(on) {
                    return None;
                }
                Event::CapsLock(on)
            }
            XEvent::ClientMessage(e)
                if e.type_ == self.atoms.WM_PROTOCOLS && e.data.as_data32()[0] == self.atoms.WM_DELETE_WINDOW =>
            {
                Event::Quit
            }
            XEvent::ClientMessage(e) if e.type_ == self.atoms.XdndEnter => {
                let d = e.data.as_data32();
                let types: Vec<Atom> = if d[1] & 1 != 0 {
                    // More than three types: they're in XdndTypeList.
                    self.conn
                        .get_property(false, d[0], self.atoms.XdndTypeList, AtomEnum::ATOM, 0, 256)
                        .ok()
                        .and_then(|c| c.reply().ok())
                        .and_then(|r| r.value32().map(|v| v.collect()))
                        .unwrap_or_default()
                } else {
                    d[2..5].to_vec()
                };
                self.dnd.set(Some(Dnd { source: d[0], files: types.contains(&self.atoms.TEXT_URI_LIST) }));
                return None;
            }
            XEvent::ClientMessage(e) if e.type_ == self.atoms.XdndPosition => {
                let d = e.data.as_data32();
                let dnd = self.dnd.get().filter(|g| g.source == d[0])?;
                let (rx, ry) = ((d[2] >> 16) as i32, (d[2] & 0xffff) as i32);
                let action = if dnd.files { self.atoms.XdndActionCopy } else { 0 };
                // Accept (bit 0) and keep sending positions (bit 1).
                self.dnd_send(dnd.source, self.atoms.XdndStatus, [self.win, u32::from(dnd.files) | 2, 0, 0, action]);
                if !dnd.files {
                    return None;
                }
                Event::DragOver { x: (rx - self.pos.0) as f64 / s, y: (ry - self.pos.1) as f64 / s }
            }
            XEvent::ClientMessage(e) if e.type_ == self.atoms.XdndLeave => {
                self.dnd.take()?;
                Event::DragLeave
            }
            XEvent::ClientMessage(e) if e.type_ == self.atoms.XdndDrop => {
                let d = e.data.as_data32();
                let dnd = self.dnd.get().filter(|g| g.source == d[0])?;
                if !dnd.files {
                    self.dnd.set(None);
                    self.dnd_send(dnd.source, self.atoms.XdndFinished, [self.win, 0, 0, 0, 0]);
                    return Some(Event::DragLeave);
                }
                // Ask for the file list; it arrives as a SelectionNotify.
                let _ = self.conn.convert_selection(
                    self.win,
                    self.atoms.XdndSelection,
                    self.atoms.TEXT_URI_LIST,
                    self.atoms.ISLET_DND,
                    d[2],
                );
                let _ = self.conn.flush();
                return None;
            }
            XEvent::SelectionNotify(e) if e.property == self.atoms.ISLET_DND => {
                let dnd = self.dnd.take()?;
                let list = self
                    .conn
                    .get_property(true, self.win, self.atoms.ISLET_DND, AtomEnum::ANY, 0, 1 << 20)
                    .ok()
                    .and_then(|c| c.reply().ok())
                    .map(|r| String::from_utf8_lossy(&r.value).into_owned())
                    .unwrap_or_default();
                let paths = islet_core::shelf::paths_from_uri_list(&list);
                let ok = !paths.is_empty();
                let action = if ok { self.atoms.XdndActionCopy } else { 0 };
                self.dnd_send(dnd.source, self.atoms.XdndFinished, [self.win, u32::from(ok), action, 0, 0]);
                if ok {
                    Event::Drop(paths)
                } else {
                    Event::DragLeave
                }
            }
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::parse_xft_dpi;

    /// Plays an XDND drag source against the island's window: Enter,
    /// Position, Drop, then serves the file list. Needs an X server
    /// (`DISPLAY`), so it skips without one.
    /// Grabs Ctrl+Alt+I and presses it through XTEST (which, like a real
    /// keyboard, goes through key grabs). Needs an X server.
    #[test]
    fn global_shortcut_reaches_the_island() {
        use super::super::{Backend, Event, WindowSpec};
        use super::*;
        use std::time::{Duration, Instant};
        use x11rb::protocol::xtest::ConnectionExt as _;
        if std::env::var_os("DISPLAY").is_none() {
            eprintln!("skipping: no X display");
            return;
        }
        let Ok(mut island) = X11::create(WindowSpec { width: 420.0, height: 260.0 }) else {
            eprintln!("skipping: can't open the X display");
            return;
        };
        island.set_hotkey(Some(islet_core::hotkey::Hotkey::parse("Ctrl+Alt+I").unwrap())).unwrap();
        let (kb, _) = x11rb::connect(None).unwrap();
        let code = |sym: u32| find_keycode(&kb, sym).unwrap();
        let (ctrl, alt, i) = (code(0xffe3), code(0xffe9), code(u32::from(b'i')));
        let press = |key: u8, down: bool| {
            let kind = if down { 2 } else { 3 }; // KeyPress / KeyRelease
            kb.xtest_fake_input(kind, key, x11rb::CURRENT_TIME, x11rb::NONE, 0, 0, 0).unwrap();
        };
        let pump = |island: &mut X11| {
            let mut out = Vec::new();
            let until = Instant::now() + Duration::from_millis(400);
            while Instant::now() < until {
                island.wait(Some(Instant::now() + Duration::from_millis(20)), &mut out).unwrap();
            }
            out
        };
        let chord = |island: &mut X11, keys: &[u8]| {
            for &k in keys {
                press(k, true);
            }
            for &k in keys.iter().rev() {
                press(k, false);
            }
            kb.flush().unwrap();
            pump(island)
        };
        assert!(chord(&mut island, &[ctrl, alt, i]).contains(&Event::Hotkey));
        assert!(!chord(&mut island, &[ctrl, i]).contains(&Event::Hotkey), "only the exact combination");
        island.set_hotkey(None).unwrap();
        assert!(!chord(&mut island, &[ctrl, alt, i]).contains(&Event::Hotkey), "removed");
    }

    #[test]
    fn an_island_opens_on_the_chosen_monitor() {
        use super::super::{Backend, WindowSpec};
        use super::*;
        use x11rb::protocol::randr::MonitorInfo;
        if std::env::var_os("DISPLAY").is_none() {
            eprintln!("skipping: no X display");
            return;
        }
        let Ok((conn, n)) = x11rb::connect(None) else { return };
        let s = conn.setup().roots[n].clone();
        let (w, h) = (s.width_in_pixels, s.height_in_pixels);
        // Split the screen into two monitors, LEFT (primary) and RIGHT.
        let atom = |name: &str| conn.intern_atom(false, name.as_bytes()).unwrap().reply().unwrap().atom;
        let (left, right) = (atom("ISLET-LEFT"), atom("ISLET-RIGHT"));
        let half = w / 2;
        for (name, x, primary) in [(left, 0, true), (right, half as i16, false)] {
            let info = MonitorInfo {
                name,
                primary,
                automatic: false,
                x,
                y: 0,
                width: half,
                height: h,
                width_in_millimeters: 300,
                height_in_millimeters: 200,
                outputs: vec![],
            };
            if conn.randr_set_monitor(s.root, info).ok().and_then(|c| c.check().ok()).is_none() {
                eprintln!("skipping: no RandR 1.5 monitors here");
                return;
            }
        }
        conn.flush().unwrap();
        let names = displays();
        assert!(names.iter().any(|n| n == "ISLET-LEFT") && names.iter().any(|n| n == "ISLET-RIGHT"), "{names:?}");
        super::super::set_preferred_display(Some("ISLET-RIGHT".into()));
        let island = X11::create(WindowSpec { width: 420.0, height: 260.0 });
        super::super::set_preferred_display(None);
        let island = island.unwrap();
        assert!(island.pos.0 >= i32::from(half), "on the right monitor: {:?}", island.pos);
        drop(island);
        for name in [left, right] {
            let _ = conn.randr_delete_monitor(s.root, name);
        }
        conn.flush().unwrap();
    }

    #[test]
    fn a_fullscreen_window_hides_the_island() {
        use super::super::{Backend, Event, WindowSpec};
        use super::*;
        use std::time::{Duration, Instant};
        if std::env::var_os("DISPLAY").is_none() {
            eprintln!("skipping: no X display");
            return;
        }
        let Ok(mut island) = X11::create(WindowSpec { width: 420.0, height: 260.0 }) else {
            eprintln!("skipping: can't open the X display");
            return;
        };
        // Play the window manager: an app window covering the screen, made
        // active and then fullscreen through the EWMH properties.
        let (wm, screen) = x11rb::connect(None).unwrap();
        let s = &wm.setup().roots[screen];
        let (root, w, h) = (s.root, s.width_in_pixels, s.height_in_pixels);
        let atoms = Atoms::new(&wm).unwrap().reply().unwrap();
        let app = wm.generate_id().unwrap();
        wm.create_window(0, app, root, 0, 0, w, h, 0, WindowClass::INPUT_OUTPUT, 0, &CreateWindowAux::new()).unwrap();
        wm.map_window(app).unwrap();
        let pump = |island: &mut X11| {
            let mut out = Vec::new();
            let until = Instant::now() + Duration::from_millis(500);
            while Instant::now() < until {
                island.wait(Some(Instant::now() + Duration::from_millis(20)), &mut out).unwrap();
            }
            out
        };
        pump(&mut island);
        wm.change_property32(PropMode::REPLACE, root, atoms._NET_ACTIVE_WINDOW, AtomEnum::WINDOW, &[app]).unwrap();
        wm.flush().unwrap();
        assert!(!pump(&mut island).contains(&Event::Fullscreen(true)), "active, but not fullscreen yet");
        let full = [atoms._NET_WM_STATE_FULLSCREEN];
        wm.change_property32(PropMode::REPLACE, app, atoms._NET_WM_STATE, AtomEnum::ATOM, &full).unwrap();
        wm.flush().unwrap();
        assert!(pump(&mut island).contains(&Event::Fullscreen(true)));
        wm.change_property32(PropMode::REPLACE, app, atoms._NET_WM_STATE, AtomEnum::ATOM, &[]).unwrap();
        wm.flush().unwrap();
        assert!(pump(&mut island).contains(&Event::Fullscreen(false)));
        // Fullscreen again, then another window becomes active.
        wm.change_property32(PropMode::REPLACE, app, atoms._NET_WM_STATE, AtomEnum::ATOM, &full).unwrap();
        wm.flush().unwrap();
        assert!(pump(&mut island).contains(&Event::Fullscreen(true)));
        wm.change_property32(PropMode::REPLACE, root, atoms._NET_ACTIVE_WINDOW, AtomEnum::WINDOW, &[x11rb::NONE])
            .unwrap();
        wm.flush().unwrap();
        assert!(pump(&mut island).contains(&Event::Fullscreen(false)));
    }

    #[test]
    fn xdnd_drop_reaches_the_island() {
        use super::super::{Backend, Event, WindowSpec};
        use super::*;
        use std::time::{Duration, Instant};
        if std::env::var_os("DISPLAY").is_none() {
            eprintln!("skipping: no X display");
            return;
        }
        let Ok(mut island) = X11::create(WindowSpec { width: 420.0, height: 260.0 }) else {
            eprintln!("skipping: can't open the X display");
            return;
        };
        let (src, screen) = x11rb::connect(None).unwrap();
        let root = src.setup().roots[screen].root;
        let atoms = Atoms::new(&src).unwrap().reply().unwrap();
        let source = src.generate_id().unwrap();
        src.create_window(0, source, root, 0, 0, 1, 1, 0, WindowClass::INPUT_ONLY, 0, &CreateWindowAux::new()).unwrap();
        src.set_selection_owner(source, atoms.XdndSelection, x11rb::CURRENT_TIME).unwrap();
        let win = island.win;
        let send = |kind: Atom, data: [u32; 5]| {
            let ev = ClientMessageEvent::new(32, win, kind, data);
            src.send_event(false, win, EventMask::NO_EVENT, ev).unwrap();
            src.flush().unwrap();
        };
        let pump = |island: &mut X11, until: &dyn Fn(&Event) -> bool| -> Option<Event> {
            let deadline = Instant::now() + Duration::from_secs(3);
            while Instant::now() < deadline {
                let mut out = Vec::new();
                island.wait(Some(Instant::now() + Duration::from_millis(20)), &mut out).unwrap();
                if let Some(e) = out.into_iter().find(|e| until(e)) {
                    return Some(e);
                }
                // Answer the island's request for the file list.
                while let Some(ev) = src.poll_for_event().unwrap() {
                    if let XEvent::SelectionRequest(r) = ev {
                        let list = b"file:///home/a/Report%20final.pdf\r\nfile:///home/a/Photos\r\n";
                        src.change_property8(PropMode::REPLACE, r.requestor, r.property, r.target, list).unwrap();
                        let note = SelectionNotifyEvent {
                            response_type: SELECTION_NOTIFY_EVENT,
                            sequence: 0,
                            time: r.time,
                            requestor: r.requestor,
                            selection: r.selection,
                            target: r.target,
                            property: r.property,
                        };
                        src.send_event(false, r.requestor, EventMask::NO_EVENT, note).unwrap();
                        src.flush().unwrap();
                    }
                }
            }
            None
        };
        // Version 5, three types or fewer, the first is text/uri-list.
        send(atoms.XdndEnter, [source, 5 << 24, atoms.TEXT_URI_LIST, 0, 0]);
        let (x, y) = (island.pos.0 + 210, island.pos.1 + 15);
        send(atoms.XdndPosition, [source, 0, ((x as u32) << 16) | y as u32, 0, atoms.XdndActionCopy]);
        let over = pump(&mut island, &|e| matches!(e, Event::DragOver { .. }));
        assert!(
            matches!(over, Some(Event::DragOver { x, y }) if (x - 210.0 / island.scale).abs() < 1.0 && y > 0.0),
            "{over:?}"
        );
        send(atoms.XdndDrop, [source, 0, 0, 0, 0]);
        let dropped = pump(&mut island, &|e| matches!(e, Event::Drop(_)));
        assert_eq!(dropped, Some(Event::Drop(vec!["/home/a/Report final.pdf".into(), "/home/a/Photos".into()])));
    }

    #[test]
    fn parses_xft_dpi() {
        assert_eq!(parse_xft_dpi("Xft.antialias:\t1\nXft.dpi:\t192\nXft.hinting:\t1"), Some(192.0));
        assert_eq!(parse_xft_dpi("Xft.dpi: 120.5\n"), Some(120.5));
        assert_eq!(parse_xft_dpi("Xcursor.size: 24"), None);
    }
}
