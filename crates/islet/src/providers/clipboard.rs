//! Clipboard history (opt-in). A watcher runs only while the preference is
//! on, reports each copied text or picture (PNG) with the app that copied
//! it, and can put an older entry back on the clipboard. Copies marked
//! secret by password managers are skipped
//! (`islet_core::clipboard::is_secret`); the island also skips the apps on
//! the user's exclude list.
//!
//! - Linux: X11 XFixes selection events (also covers GNOME and other
//!   Wayland desktops, whose clipboard XWayland mirrors);
//! - Windows: `AddClipboardFormatListener` (WM_CLIPBOARDUPDATE);
//! - macOS: no notification exists, so the pasteboard's change counter is
//!   checked twice a second while history is on.

use super::Sink;
use islet_core::clipboard::{Clip, ClipImage, MAX_IMAGE_BYTES};

pub enum Cmd {
    Copy(Clip),
    Stop,
}

/// A copied picture as a history entry (None if it isn't a PNG we can read).
fn image(png: Vec<u8>) -> Option<Clip> {
    if png.is_empty() || png.len() > MAX_IMAGE_BYTES {
        return None;
    }
    let (thumb, width, height) = super::art::png_thumbnail(&png)?;
    Some(Clip::Image(ClipImage { png: std::sync::Arc::new(png), width, height, thumb: Some(thumb) }))
}

/// A program's name from its path: "/usr/bin/keepassxc" → "keepassxc".
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn program_name(path: &str) -> String {
    let base = path.rsplit(['/', '\\']).next().unwrap_or(path);
    base.strip_suffix(".exe").or_else(|| base.strip_suffix(".EXE")).unwrap_or(base).to_string()
}

/// A running watcher; dropping it stops the thread.
pub struct Watcher {
    tx: std::sync::mpsc::Sender<Cmd>,
    wake: Box<dyn Fn() + Send>,
}

impl Watcher {
    /// Put an entry on the clipboard (choosing an older history entry).
    pub fn copy(&self, clip: Clip) {
        let _ = self.tx.send(Cmd::Copy(clip));
        (self.wake)();
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Stop);
        (self.wake)();
    }
}

pub fn start(sink: Sink) -> Option<Watcher> {
    platform::start(sink)
}

#[cfg(target_os = "linux")]
mod platform {
    use super::{Cmd, Sink, Watcher};
    use islet_core::activity::Report;
    use islet_core::clipboard::Clip;
    use std::collections::HashMap;
    use std::sync::mpsc::{channel, Receiver};
    use std::sync::Arc;
    use x11rb::connection::Connection;
    use x11rb::protocol::xfixes::{ConnectionExt as _, SelectionEventMask};
    use x11rb::protocol::xproto::{
        Atom, AtomEnum, ClientMessageEvent, ConnectionExt as _, CreateWindowAux, EventMask, PropMode,
        SelectionNotifyEvent, SelectionRequestEvent, Window, WindowClass, SELECTION_NOTIFY_EVENT,
    };
    use x11rb::protocol::Event;
    use x11rb::rust_connection::RustConnection;
    use x11rb::wrapper::ConnectionExt as _;
    use x11rb::{COPY_DEPTH_FROM_PARENT, CURRENT_TIME, NONE};

    struct Atoms {
        clipboard: Atom,
        targets: Atom,
        utf8: Atom,
        text: Atom,
        incr: Atom,
        prop: Atom,
        png: Atom,
        pid: Atom,
    }

    pub fn start(sink: Sink) -> Option<Watcher> {
        match try_start(sink) {
            Ok(w) => Some(w),
            Err(why) => {
                crate::providers::debug_log(&format!("clipboard history unavailable: {why}"));
                None
            }
        }
    }

    /// Start watching, or say which step failed.
    pub(super) fn try_start(sink: Sink) -> Result<Watcher, String> {
        let e = |step: &'static str| move |err: &dyn std::fmt::Debug| format!("{step}: {err:?}");
        let (conn, screen) = x11rb::connect(None).map_err(|x| e("connect")(&x))?;
        let conn = Arc::new(conn);
        let root = conn.setup().roots[screen].root;
        let win = conn.generate_id().map_err(|x| e("window id")(&x))?;
        conn.create_window(
            COPY_DEPTH_FROM_PARENT,
            win,
            root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_ONLY,
            0,
            // Property changes: big copies arrive in pieces (INCR).
            &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )
        .map_err(|x| e("create window")(&x))?
        .check()
        .map_err(|x| e("create window")(&x))?;
        let atom = |name: &str| -> Result<Atom, String> {
            let cookie = conn.intern_atom(false, name.as_bytes()).map_err(|x| format!("atom {name}: {x:?}"))?;
            cookie.reply().map(|r| r.atom).map_err(|x| format!("atom {name}: {x:?}"))
        };
        let atoms = Atoms {
            clipboard: atom("CLIPBOARD")?,
            targets: atom("TARGETS")?,
            utf8: atom("UTF8_STRING")?,
            text: atom("TEXT")?,
            incr: atom("INCR")?,
            prop: atom("ISLET_CLIPBOARD")?,
            png: atom("image/png")?,
            pid: atom("_NET_WM_PID")?,
        };
        conn.xfixes_query_version(5, 0).map_err(|x| e("xfixes")(&x))?.reply().map_err(|x| e("xfixes")(&x))?;
        conn.xfixes_select_selection_input(win, atoms.clipboard, SelectionEventMask::SET_SELECTION_OWNER)
            .map_err(|x| e("xfixes select")(&x))?
            .check()
            .map_err(|x| e("xfixes select")(&x))?;
        conn.flush().map_err(|x| e("flush")(&x))?;
        let (tx, rx) = channel();
        let c = conn.clone();
        std::thread::Builder::new()
            .name("clipboard".into())
            .spawn(move || run(&c, win, &atoms, &rx, &sink))
            .map_err(|x| e("thread")(&x))?;
        // Wake the watcher with a message to its own window.
        let wake = move || {
            let ev = ClientMessageEvent::new(32, win, AtomEnum::NOTICE, [0u32; 5]);
            let _ = conn.send_event(false, win, EventMask::NO_EVENT, ev);
            let _ = conn.flush();
        };
        Ok(Watcher { tx, wake: Box::new(wake) })
    }

    /// A transfer in progress: what was asked for, who copied it, and the
    /// pieces of an INCR transfer so far.
    struct Pending {
        target: Atom,
        app: Option<String>,
        incr: Option<Vec<u8>>,
    }

    /// The app that owns the clipboard: the owner window's process
    /// (`_NET_WM_PID`), as named in /proc.
    fn owner_app(conn: &RustConnection, a: &Atoms, owner: Window) -> Option<String> {
        let reply = conn.get_property(false, owner, a.pid, AtomEnum::CARDINAL, 0, 1).ok()?.reply().ok()?;
        let pid = reply.value32()?.next()?;
        let exe = std::fs::read_link(format!("/proc/{pid}/exe")).ok();
        let name = match exe {
            Some(path) => super::program_name(&path.to_string_lossy()),
            None => std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?.trim().to_string(),
        };
        (!name.is_empty()).then_some(name)
    }

    fn deliver(sink: &Sink, a: &Atoms, target: Atom, bytes: Vec<u8>, app: Option<String>) {
        let clip = if target == a.png {
            super::image(bytes)
        } else {
            Some(Clip::Text(String::from_utf8_lossy(&bytes).into_owned()))
        };
        if let Some(clip) = clip {
            sink.send(Report::Clipboard(clip, app));
        }
    }

    fn run(conn: &RustConnection, win: Window, a: &Atoms, rx: &Receiver<Cmd>, sink: &Sink) {
        let mut owned: Option<Clip> = None;
        let mut names: HashMap<Atom, String> = HashMap::new();
        let mut pending: Option<Pending> = None;
        let mut owner_now: Option<String> = None;
        let read = |delete: bool| {
            conn.get_property(delete, win, a.prop, AtomEnum::ANY, 0, (super::MAX_IMAGE_BYTES / 4 + 1) as u32)
                .ok()
                .and_then(|c| c.reply().ok())
        };
        loop {
            let Ok(ev) = conn.wait_for_event() else { return };
            for cmd in rx.try_iter() {
                match cmd {
                    Cmd::Copy(clip) => {
                        owned = Some(clip);
                        let _ = conn.set_selection_owner(win, a.clipboard, CURRENT_TIME);
                    }
                    Cmd::Stop => {
                        let _ = conn.destroy_window(win);
                        let _ = conn.flush();
                        return;
                    }
                }
            }
            match ev {
                Event::XfixesSelectionNotify(e) if e.owner != win && e.owner != NONE => {
                    owner_now = owner_app(conn, a, e.owner);
                    // Ask what the new copy offers before taking anything.
                    pending = None;
                    let _ = conn.convert_selection(win, a.clipboard, a.targets, a.prop, e.selection_timestamp);
                }
                Event::SelectionNotify(e) if e.requestor == win && e.property != NONE => {
                    let Some(reply) = read(true) else { continue };
                    if e.target == a.targets {
                        let offered: Vec<Atom> = reply.value32().map(|v| v.collect()).unwrap_or_default();
                        let formats: Vec<String> = offered.iter().map(|&t| name(conn, &mut names, t)).collect();
                        if islet_core::clipboard::is_secret(&formats) {
                            continue;
                        }
                        // Text first; a picture when there is no text.
                        let want = if offered.contains(&a.utf8) {
                            a.utf8
                        } else if offered.contains(&AtomEnum::STRING.into()) {
                            AtomEnum::STRING.into()
                        } else if offered.contains(&a.png) {
                            a.png
                        } else {
                            continue; // files and other kinds aren't kept
                        };
                        pending = Some(Pending { target: want, app: owner_now.clone(), incr: None });
                        let _ = conn.convert_selection(win, a.clipboard, want, a.prop, e.time);
                    } else if reply.type_ == a.incr {
                        // A big copy: it comes in pieces, each a property change.
                        if let Some(p) = pending.as_mut().filter(|p| p.target == e.target) {
                            p.incr = Some(Vec::new());
                        }
                    } else if let Some(p) = pending.take().filter(|p| p.target == e.target) {
                        deliver(sink, a, p.target, reply.value, p.app);
                    }
                }
                Event::PropertyNotify(e)
                    if e.window == win
                        && e.atom == a.prop
                        && e.state == x11rb::protocol::xproto::Property::NEW_VALUE
                        && pending.as_ref().is_some_and(|p| p.incr.is_some()) =>
                {
                    let Some(reply) = read(true) else { continue };
                    let Some(mut p) = pending.take() else { continue };
                    let buf = p.incr.get_or_insert_with(Vec::new);
                    if reply.value.is_empty() {
                        // The last, empty piece: the copy is complete.
                        let bytes = std::mem::take(buf);
                        deliver(sink, a, p.target, bytes, p.app);
                    } else if buf.len() + reply.value.len() <= super::MAX_IMAGE_BYTES {
                        buf.extend_from_slice(&reply.value);
                        pending = Some(p);
                    }
                    // Too big: dropped (pending stays None).
                }
                Event::SelectionRequest(e) => serve(conn, a, &e, owned.as_ref()),
                Event::SelectionClear(_) => owned = None,
                _ => {}
            }
            let _ = conn.flush();
        }
    }

    fn name(conn: &RustConnection, cache: &mut HashMap<Atom, String>, atom: Atom) -> String {
        cache
            .entry(atom)
            .or_insert_with(|| {
                conn.get_atom_name(atom)
                    .ok()
                    .and_then(|c| c.reply().ok())
                    .map(|r| String::from_utf8_lossy(&r.name).into_owned())
                    .unwrap_or_default()
            })
            .clone()
    }

    /// Hand our copy to an app that pastes it.
    fn serve(conn: &RustConnection, a: &Atoms, e: &SelectionRequestEvent, owned: Option<&Clip>) {
        let property = if e.property == NONE { e.target } else { e.property };
        let string: Atom = AtomEnum::STRING.into();
        let ok = match owned {
            Some(_) if e.selection != a.clipboard => false,
            Some(Clip::Text(_)) if e.target == a.targets => conn
                .change_property32(
                    PropMode::REPLACE,
                    e.requestor,
                    property,
                    AtomEnum::ATOM,
                    &[a.targets, a.utf8, string, a.text],
                )
                .is_ok(),
            Some(Clip::Text(t)) if e.target == a.utf8 || e.target == string || e.target == a.text => {
                let ty = if e.target == string { string } else { a.utf8 };
                conn.change_property8(PropMode::REPLACE, e.requestor, property, ty, t.as_bytes()).is_ok()
            }
            Some(Clip::Image(_)) if e.target == a.targets => conn
                .change_property32(PropMode::REPLACE, e.requestor, property, AtomEnum::ATOM, &[a.targets, a.png])
                .is_ok(),
            Some(Clip::Image(i)) if e.target == a.png => {
                conn.change_property8(PropMode::REPLACE, e.requestor, property, a.png, &i.png).is_ok()
            }
            _ => false,
        };
        let reply = SelectionNotifyEvent {
            response_type: SELECTION_NOTIFY_EVENT,
            sequence: 0,
            time: e.time,
            requestor: e.requestor,
            selection: e.selection,
            target: e.target,
            property: if ok { property } else { NONE },
        };
        let _ = conn.send_event(false, e.requestor, EventMask::NO_EVENT, reply);
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::{Cmd, Sink, Watcher};
    use islet_core::activity::Report;
    use islet_core::clipboard::Clip;
    use std::sync::atomic::{AtomicIsize, Ordering};
    use std::sync::mpsc::{channel, Receiver};
    use std::sync::Arc;
    use windows::core::w;
    use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::DataExchange::{
        AddClipboardFormatListener, CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
        GetClipboardFormatNameW, GetClipboardOwner, IsClipboardFormatAvailable, OpenClipboard,
        RegisterClipboardFormatW, RemoveClipboardFormatListener, SetClipboardData,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, GetWindowThreadProcessId,
        PostMessageW, RegisterClassW, HWND_MESSAGE, MSG, WINDOW_EX_STYLE, WM_APP, WM_CLIPBOARDUPDATE, WNDCLASSW,
        WS_OVERLAPPED,
    };

    const CF_UNICODETEXT: u32 = 13;
    const CF_DIB: u32 = 8;
    const WAKE: u32 = WM_APP + 7;

    unsafe extern "system" fn wndproc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        // SAFETY: default handling for a message-only window.
        unsafe { DefWindowProcW(h, m, w, l) }
    }

    pub fn start(sink: Sink) -> Option<Watcher> {
        let (tx, rx) = channel();
        let hwnd = Arc::new(AtomicIsize::new(0));
        let h = hwnd.clone();
        let (ready_tx, ready_rx) = channel();
        std::thread::Builder::new()
            .name("clipboard".into())
            .spawn(move || {
                // SAFETY: a message-only window owned by this thread.
                let win = unsafe { create() };
                let _ = ready_tx.send(win.is_some());
                if let Some(win) = win {
                    h.store(win.0 as isize, Ordering::SeqCst);
                    run(win, &rx, &sink);
                }
            })
            .ok()?;
        if !ready_rx.recv().unwrap_or(false) {
            return None;
        }
        let wake = move || {
            let w = HWND(hwnd.load(Ordering::SeqCst) as *mut std::ffi::c_void);
            // SAFETY: posting to our own window (ignored if it's gone).
            let _ = unsafe { PostMessageW(Some(w), WAKE, WPARAM(0), LPARAM(0)) };
        };
        Some(Watcher { tx, wake: Box::new(wake) })
    }

    unsafe fn create() -> Option<HWND> {
        // SAFETY: standard class registration and window creation.
        unsafe {
            let instance = GetModuleHandleW(None).ok()?;
            let class = w!("IsletClipboard");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                hInstance: instance.into(),
                lpszClassName: class,
                ..Default::default()
            };
            RegisterClassW(&wc);
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class,
                w!(""),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                Some(instance.into()),
                None,
            )
            .ok()?;
            AddClipboardFormatListener(hwnd).ok()?;
            Some(hwnd)
        }
    }

    fn run(hwnd: HWND, rx: &Receiver<Cmd>, sink: &Sink) {
        let mut msg = MSG::default();
        // SAFETY: a standard message loop for this thread's window.
        while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
            match msg.message {
                WM_CLIPBOARDUPDATE => {
                    if let Some((clip, app)) = read(hwnd) {
                        sink.send(Report::Clipboard(clip, app));
                    }
                }
                WAKE => {
                    for cmd in rx.try_iter() {
                        match cmd {
                            Cmd::Copy(clip) => write(hwnd, &clip),
                            Cmd::Stop => {
                                // SAFETY: our own window.
                                unsafe {
                                    let _ = RemoveClipboardFormatListener(hwnd);
                                    let _ = DestroyWindow(hwnd);
                                }
                                return;
                            }
                        }
                    }
                }
                // SAFETY: forwarding other messages.
                _ => unsafe {
                    DispatchMessageW(&msg);
                },
            }
        }
    }

    fn open(hwnd: HWND) -> bool {
        // Another app may hold the clipboard for a moment.
        for _ in 0..5 {
            // SAFETY: plain clipboard call.
            if unsafe { OpenClipboard(Some(hwnd)) }.is_ok() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        false
    }

    /// Names of the registered formats on the clipboard (the clipboard is open).
    fn format_names() -> Vec<String> {
        let mut out = Vec::new();
        let mut f = 0u32;
        loop {
            // SAFETY: enumerating formats while the clipboard is open.
            f = unsafe { EnumClipboardFormats(f) };
            if f == 0 {
                break;
            }
            let mut buf = [0u16; 128];
            // SAFETY: a sized buffer; standard formats have no name (0).
            let n = unsafe { GetClipboardFormatNameW(f, &mut buf) };
            if n > 0 {
                out.push(String::from_utf16_lossy(&buf[..n as usize]));
            }
        }
        out
    }

    /// The program that put this copy on the clipboard.
    fn owner_app() -> Option<String> {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        // SAFETY: plain calls; the process handle is closed before returning.
        unsafe {
            let owner = GetClipboardOwner().ok()?;
            let mut pid = 0u32;
            GetWindowThreadProcessId(owner, Some(&mut pid));
            if pid == 0 {
                return None;
            }
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buf = [0u16; 512];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_WIN32,
                windows::core::PWSTR(buf.as_mut_ptr()),
                &mut len,
            )
            .is_ok();
            let _ = CloseHandle(process);
            ok.then(|| super::program_name(&String::from_utf16_lossy(&buf[..len as usize])))
        }
    }

    /// A picture on the clipboard as PNG: the "PNG" format browsers and the
    /// Snipping Tool offer, else the bitmap every app offers, converted.
    fn read_image() -> Option<Vec<u8>> {
        // SAFETY: the clipboard is open; handles are read under GlobalLock.
        unsafe {
            let png = RegisterClipboardFormatW(w!("PNG"));
            if IsClipboardFormatAvailable(png).is_ok() {
                if let Some(bytes) = GetClipboardData(png).ok().and_then(global_bytes) {
                    return Some(bytes);
                }
            }
            if IsClipboardFormatAvailable(CF_DIB).is_ok() {
                let dib = GetClipboardData(CF_DIB).ok().and_then(global_bytes)?;
                return islet_core::bitmap::dib_to_rgba(&dib).and_then(|(w, h, rgba)| png_from_rgba(w, h, &rgba));
            }
        }
        None
    }

    fn png_from_rgba(w: u32, h: u32, rgba: &[u8]) -> Option<Vec<u8>> {
        let mut pm = tiny_skia::Pixmap::new(w, h)?;
        // Premultiply for tiny-skia.
        for (dst, src) in pm.data_mut().chunks_exact_mut(4).zip(rgba.chunks_exact(4)) {
            let a = u16::from(src[3]);
            for i in 0..3 {
                dst[i] = ((u16::from(src[i]) * a + 127) / 255) as u8;
            }
            dst[3] = src[3];
        }
        pm.encode_png().ok()
    }

    fn read(hwnd: HWND) -> Option<(Clip, Option<String>)> {
        if !open(hwnd) {
            return None;
        }
        let result = (|| {
            let mut names = format_names();
            // "Don't keep this in history" (set to 0 by password managers).
            // SAFETY: registering a well-known format name.
            let no_history = unsafe { RegisterClipboardFormatW(w!("CanIncludeInClipboardHistory")) };
            // SAFETY: the clipboard is open.
            if unsafe { IsClipboardFormatAvailable(no_history) }.is_ok() {
                if let Ok(h) = unsafe { GetClipboardData(no_history) } {
                    if global_bytes(h).is_some_and(|b| b.iter().all(|&x| x == 0)) {
                        names.push("ExcludeClipboardContentFromMonitorProcessing".into());
                    }
                }
            }
            if islet_core::clipboard::is_secret(&names) {
                return None;
            }
            let app = owner_app();
            // Text first; a picture when there is no text.
            // SAFETY: the clipboard is open; the handle is read under GlobalLock.
            if let Some(bytes) = unsafe { GetClipboardData(CF_UNICODETEXT) }.ok().and_then(global_bytes) {
                let wide: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
                return Some((Clip::Text(String::from_utf16_lossy(&wide[..end])), app));
            }
            Some((super::image(read_image()?)?, app))
        })();
        // SAFETY: we opened it.
        let _ = unsafe { CloseClipboard() };
        result
    }

    fn global_bytes(h: HANDLE) -> Option<Vec<u8>> {
        let g = HGLOBAL(h.0);
        // SAFETY: locking a clipboard-owned global for reading.
        unsafe {
            let p = GlobalLock(g) as *const u8;
            if p.is_null() {
                return None;
            }
            let v = std::slice::from_raw_parts(p, GlobalSize(g)).to_vec();
            let _ = GlobalUnlock(g);
            Some(v)
        }
    }

    /// Hand `bytes` to the clipboard as `format` (it owns them afterwards).
    unsafe fn put(format: u32, bytes: &[u8]) {
        // SAFETY: allocate, fill and hand over a movable global.
        unsafe {
            if let Ok(g) = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)) {
                let p = GlobalLock(g) as *mut u8;
                if !p.is_null() {
                    std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
                    let _ = GlobalUnlock(g);
                    let _ = SetClipboardData(format, Some(HANDLE(g.0)));
                }
            }
        }
    }

    fn write(hwnd: HWND, clip: &Clip) {
        if !open(hwnd) {
            return;
        }
        // SAFETY: the clipboard is open; `put` hands each format over.
        unsafe {
            let _ = EmptyClipboard();
            match clip {
                Clip::Text(text) => {
                    let wide: Vec<u8> =
                        text.encode_utf16().chain(std::iter::once(0)).flat_map(u16::to_le_bytes).collect();
                    put(CF_UNICODETEXT, &wide);
                }
                Clip::Image(img) => {
                    // As PNG for apps that take it, and as a bitmap for all others.
                    put(RegisterClipboardFormatW(w!("PNG")), &img.png);
                    if let Ok(pm) = tiny_skia::Pixmap::decode_png(&img.png) {
                        let dib = islet_core::bitmap::rgba_to_dib(pm.width(), pm.height(), &unpremultiply(pm.data()));
                        put(CF_DIB, &dib);
                    }
                }
            }
            let _ = CloseClipboard();
        }
    }

    fn unpremultiply(data: &[u8]) -> Vec<u8> {
        data.chunks_exact(4)
            .flat_map(|p| {
                let a = u16::from(p[3]);
                let un = |c: u8| ((u16::from(c) * 255 + a / 2).checked_div(a).unwrap_or(0)).min(255) as u8;
                [un(p[0]), un(p[1]), un(p[2]), p[3]]
            })
            .collect()
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{Cmd, Sink, Watcher};
    use islet_core::activity::Report;
    use islet_core::clipboard::Clip;
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypePNG, NSPasteboardTypeString, NSWorkspace};
    use objc2_foundation::{NSData, NSString};
    use std::sync::mpsc::{channel, RecvTimeoutError};
    use std::time::Duration;

    const EVERY: Duration = Duration::from_millis(500);

    /// The app in front: on macOS that is the one that just copied.
    fn front_app() -> Option<String> {
        let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
        app.localizedName().map(|n| n.to_string()).or_else(|| app.bundleIdentifier().map(|b| b.to_string()))
    }

    pub fn start(sink: Sink) -> Option<Watcher> {
        let (tx, rx) = channel();
        std::thread::Builder::new()
            .name("clipboard".into())
            .spawn(move || {
                let pb = NSPasteboard::generalPasteboard();
                let mut seen = pb.changeCount();
                loop {
                    match rx.recv_timeout(EVERY) {
                        // SAFETY: plain pasteboard writes.
                        Ok(Cmd::Copy(clip)) => unsafe {
                            pb.clearContents();
                            match clip {
                                Clip::Text(text) => {
                                    pb.setString_forType(&NSString::from_str(&text), NSPasteboardTypeString);
                                }
                                Clip::Image(img) => {
                                    pb.setData_forType(Some(&NSData::with_bytes(&img.png)), NSPasteboardTypePNG);
                                }
                            }
                            // Our own copy isn't news.
                            seen = pb.changeCount();
                        },
                        Ok(Cmd::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                        Err(RecvTimeoutError::Timeout) => {}
                    }
                    // SAFETY: plain pasteboard reads.
                    unsafe {
                        let count = pb.changeCount();
                        if count == seen {
                            continue;
                        }
                        seen = count;
                        let types: Vec<String> =
                            pb.types().map(|t| t.iter().map(|s| s.to_string()).collect()).unwrap_or_default();
                        if islet_core::clipboard::is_secret(&types) {
                            continue;
                        }
                        let app = front_app();
                        if let Some(s) = pb.stringForType(NSPasteboardTypeString) {
                            sink.send(Report::Clipboard(Clip::Text(s.to_string()), app));
                        } else if let Some(data) = pb.dataForType(NSPasteboardTypePNG) {
                            if let Some(clip) = super::image(data.to_vec()) {
                                sink.send(Report::Clipboard(clip, app));
                            }
                        }
                    }
                }
            })
            .ok()?;
        Some(Watcher { tx, wake: Box::new(|| {}) })
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    //! Needs an X display (`xvfb-run`); skips without one.
    use super::*;
    use islet_core::activity::Report;
    use std::sync::mpsc::channel;
    use std::time::{Duration, Instant};
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{
        AtomEnum, ConnectionExt as _, CreateWindowAux, EventMask, PropMode, SelectionNotifyEvent, WindowClass,
        SELECTION_NOTIFY_EVENT,
    };
    use x11rb::protocol::Event;
    use x11rb::wrapper::ConnectionExt as _;
    use x11rb::{COPY_DEPTH_FROM_PARENT, CURRENT_TIME, NONE};

    /// Another app: owns the clipboard with `text` for `secs`, offering a
    /// password-manager hint when `secret`.
    fn other_app_copies(text: &'static str, secret: bool, secs: f64) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            let (c, screen) = x11rb::connect(None).unwrap();
            let root = c.setup().roots[screen].root;
            let win = c.generate_id().unwrap();
            c.create_window(
                COPY_DEPTH_FROM_PARENT,
                win,
                root,
                0,
                0,
                1,
                1,
                0,
                WindowClass::INPUT_ONLY,
                0,
                &CreateWindowAux::new(),
            )
            .unwrap();
            let atom = |n: &str| c.intern_atom(false, n.as_bytes()).unwrap().reply().unwrap().atom;
            let (clipboard, targets, utf8, hint) =
                (atom("CLIPBOARD"), atom("TARGETS"), atom("UTF8_STRING"), atom("x-kde-passwordManagerHint"));
            c.set_selection_owner(win, clipboard, CURRENT_TIME).unwrap();
            c.flush().unwrap();
            let until = Instant::now() + Duration::from_secs_f64(secs);
            while Instant::now() < until {
                let Some(Event::SelectionRequest(e)) = c.poll_for_event().unwrap() else {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                };
                let mut offered = vec![targets, utf8];
                if secret {
                    offered.push(hint);
                }
                if e.target == targets {
                    c.change_property32(PropMode::REPLACE, e.requestor, e.property, AtomEnum::ATOM, &offered).unwrap();
                } else {
                    let body: &[u8] = if e.target == hint { b"secret" } else { text.as_bytes() };
                    c.change_property8(PropMode::REPLACE, e.requestor, e.property, utf8, body).unwrap();
                }
                let reply = SelectionNotifyEvent {
                    response_type: SELECTION_NOTIFY_EVENT,
                    sequence: 0,
                    time: e.time,
                    requestor: e.requestor,
                    selection: e.selection,
                    target: e.target,
                    property: e.property,
                };
                c.send_event(false, e.requestor, EventMask::NO_EVENT, reply).unwrap();
                c.flush().unwrap();
            }
        })
    }

    /// Another app copies a 64×40 picture (PNG only, no text), and says
    /// which process it is, as toolkits do.
    fn picture_app_copies(secs: f64) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            let mut pm = tiny_skia::Pixmap::new(64, 40).unwrap();
            pm.fill(tiny_skia::Color::from_rgba8(30, 120, 220, 255));
            let png = pm.encode_png().unwrap();
            let (c, screen) = x11rb::connect(None).unwrap();
            let root = c.setup().roots[screen].root;
            let win = c.generate_id().unwrap();
            c.create_window(
                COPY_DEPTH_FROM_PARENT,
                win,
                root,
                0,
                0,
                1,
                1,
                0,
                WindowClass::INPUT_ONLY,
                0,
                &CreateWindowAux::new(),
            )
            .unwrap();
            let atom = |n: &str| c.intern_atom(false, n.as_bytes()).unwrap().reply().unwrap().atom;
            let (clipboard, targets, image, pid) =
                (atom("CLIPBOARD"), atom("TARGETS"), atom("image/png"), atom("_NET_WM_PID"));
            c.change_property32(PropMode::REPLACE, win, pid, AtomEnum::CARDINAL, &[std::process::id()]).unwrap();
            c.set_selection_owner(win, clipboard, CURRENT_TIME).unwrap();
            c.flush().unwrap();
            let until = Instant::now() + Duration::from_secs_f64(secs);
            while Instant::now() < until {
                let Some(Event::SelectionRequest(e)) = c.poll_for_event().unwrap() else {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                };
                if e.target == targets {
                    c.change_property32(PropMode::REPLACE, e.requestor, e.property, AtomEnum::ATOM, &[targets, image])
                        .unwrap();
                } else {
                    c.change_property8(PropMode::REPLACE, e.requestor, e.property, image, &png).unwrap();
                }
                let reply = SelectionNotifyEvent {
                    response_type: SELECTION_NOTIFY_EVENT,
                    sequence: 0,
                    time: e.time,
                    requestor: e.requestor,
                    selection: e.selection,
                    target: e.target,
                    property: e.property,
                };
                c.send_event(false, e.requestor, EventMask::NO_EVENT, reply).unwrap();
                c.flush().unwrap();
            }
        })
    }

    /// Paste: what the clipboard holds now, as another app would read it.
    fn paste() -> Option<String> {
        let (c, screen) = x11rb::connect(None).ok()?;
        let root = c.setup().roots[screen].root;
        let win = c.generate_id().ok()?;
        c.create_window(
            COPY_DEPTH_FROM_PARENT,
            win,
            root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_ONLY,
            0,
            &CreateWindowAux::new(),
        )
        .ok()?;
        let atom = |n: &str| c.intern_atom(false, n.as_bytes()).ok()?.reply().ok().map(|r| r.atom);
        let (clipboard, utf8, prop) = (atom("CLIPBOARD")?, atom("UTF8_STRING")?, atom("PASTE")?);
        c.convert_selection(win, clipboard, utf8, prop, CURRENT_TIME).ok()?;
        c.flush().ok()?;
        let until = Instant::now() + Duration::from_secs(2);
        while Instant::now() < until {
            if let Some(Event::SelectionNotify(e)) = c.poll_for_event().ok()? {
                if e.property == NONE {
                    return None;
                }
                let r = c.get_property(true, win, prop, AtomEnum::ANY, 0, 1 << 16).ok()?.reply().ok()?;
                return Some(String::from_utf8_lossy(&r.value).into_owned());
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        None
    }

    #[test]
    fn records_copies_skips_secrets_and_copies_back() {
        // Held for the whole test: an X server whose last client leaves
        // starts afresh, and turns away whoever connects meanwhile.
        let keep = x11rb::connect(None);
        if std::env::var_os("DISPLAY").is_none() || keep.is_err() {
            eprintln!("skipping: no X display");
            return;
        }
        let (tx, rx) = channel();
        let sink = Sink { tx, waker: crate::platform::Waker::new(|| {}) };
        let watcher = match platform::try_start(sink) {
            Ok(w) => w,
            Err(why) => panic!("XFixes watcher: {why}"),
        };
        let next = |ms: u64| match rx.recv_timeout(Duration::from_millis(ms)) {
            Ok((Report::Clipboard(Clip::Text(t), _), _)) => Some(t),
            _ => None,
        };

        let app = other_app_copies("hello from an app", false, 1.0);
        assert_eq!(next(2000).as_deref(), Some("hello from an app"));
        app.join().unwrap();

        let manager = other_app_copies("hunter2", true, 1.0);
        assert_eq!(next(800), None, "a password manager's copy is never recorded");
        manager.join().unwrap();

        // A picture (and no text) is kept as a picture, with who copied it.
        let shot = picture_app_copies(1.5);
        let got = rx.recv_timeout(Duration::from_millis(2500));
        let Ok((Report::Clipboard(Clip::Image(img), app), _)) = got else { panic!("no picture: {got:?}") };
        assert_eq!((img.width, img.height), (64, 40));
        assert!(img.thumb.is_some());
        let me = std::env::current_exe().unwrap();
        assert_eq!(app.as_deref(), Some(super::program_name(&me.to_string_lossy()).as_str()), "named by its process");
        shot.join().unwrap();

        // Choosing a history entry puts it back on the clipboard.
        watcher.copy(Clip::Text("from the history".into()));
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(paste().as_deref(), Some("from the history"));
        assert_eq!(next(300), None, "Islet's own copy isn't recorded twice");
        drop(watcher);
    }
}
