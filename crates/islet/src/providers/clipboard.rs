//! Clipboard history (opt-in). A watcher runs only while the preference is
//! on, reports each copied text, and can put an older entry back on the
//! clipboard. Copies marked secret by password managers are skipped
//! (`islet_core::clipboard::is_secret`).
//!
//! - Linux: X11 XFixes selection events (also covers GNOME and other
//!   Wayland desktops, whose clipboard XWayland mirrors);
//! - Windows: `AddClipboardFormatListener` (WM_CLIPBOARDUPDATE);
//! - macOS: no notification exists, so the pasteboard's change counter is
//!   checked twice a second while history is on.

use super::Sink;

pub enum Cmd {
    Copy(String),
    Stop,
}

/// A running watcher; dropping it stops the thread.
pub struct Watcher {
    tx: std::sync::mpsc::Sender<Cmd>,
    wake: Box<dyn Fn() + Send>,
}

impl Watcher {
    /// Put `text` on the clipboard (choosing an older history entry).
    pub fn copy(&self, text: String) {
        let _ = self.tx.send(Cmd::Copy(text));
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
    }

    pub fn start(sink: Sink) -> Option<Watcher> {
        let (conn, screen) = x11rb::connect(None).ok()?;
        let conn = Arc::new(conn);
        let root = conn.setup().roots[screen].root;
        let win = conn.generate_id().ok()?;
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
            &CreateWindowAux::new(),
        )
        .ok()?;
        let atom = |name: &str| conn.intern_atom(false, name.as_bytes()).ok()?.reply().ok().map(|r| r.atom);
        let atoms = Atoms {
            clipboard: atom("CLIPBOARD")?,
            targets: atom("TARGETS")?,
            utf8: atom("UTF8_STRING")?,
            text: atom("TEXT")?,
            incr: atom("INCR")?,
            prop: atom("ISLET_CLIPBOARD")?,
        };
        conn.xfixes_query_version(5, 0).ok()?.reply().ok()?;
        conn.xfixes_select_selection_input(win, atoms.clipboard, SelectionEventMask::SET_SELECTION_OWNER).ok()?;
        conn.flush().ok()?;
        let (tx, rx) = channel();
        let c = conn.clone();
        std::thread::Builder::new().name("clipboard".into()).spawn(move || run(&c, win, &atoms, &rx, &sink)).ok()?;
        // Wake the watcher with a message to its own window.
        let wake = move || {
            let ev = ClientMessageEvent::new(32, win, AtomEnum::NOTICE, [0u32; 5]);
            let _ = conn.send_event(false, win, EventMask::NO_EVENT, ev);
            let _ = conn.flush();
        };
        Some(Watcher { tx, wake: Box::new(wake) })
    }

    fn run(conn: &RustConnection, win: Window, a: &Atoms, rx: &Receiver<Cmd>, sink: &Sink) {
        let mut owned: Option<String> = None;
        let mut names: HashMap<Atom, String> = HashMap::new();
        loop {
            let Ok(ev) = conn.wait_for_event() else { return };
            for cmd in rx.try_iter() {
                match cmd {
                    Cmd::Copy(text) => {
                        owned = Some(text);
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
                    // Ask what the new copy offers before taking any text.
                    let _ = conn.convert_selection(win, a.clipboard, a.targets, a.prop, e.selection_timestamp);
                }
                Event::SelectionNotify(e) if e.requestor == win && e.property != NONE => {
                    let Some(reply) = conn
                        .get_property(
                            true,
                            win,
                            a.prop,
                            AtomEnum::ANY,
                            0,
                            (islet_core::clipboard::READ_CHARS * 4) as u32,
                        )
                        .ok()
                        .and_then(|c| c.reply().ok())
                    else {
                        continue;
                    };
                    if e.target == a.targets {
                        let offered: Vec<Atom> = reply.value32().map(|v| v.collect()).unwrap_or_default();
                        let formats: Vec<String> = offered.iter().map(|&t| name(conn, &mut names, t)).collect();
                        if islet_core::clipboard::is_secret(&formats) {
                            continue;
                        }
                        let want = if offered.contains(&a.utf8) {
                            a.utf8
                        } else if offered.contains(&AtomEnum::STRING.into()) {
                            AtomEnum::STRING.into()
                        } else {
                            continue; // images, files…: text only
                        };
                        let _ = conn.convert_selection(win, a.clipboard, want, a.prop, e.time);
                    } else if reply.type_ != a.incr {
                        // Huge copies (INCR transfers) are skipped.
                        sink.send(Report::Clipboard(String::from_utf8_lossy(&reply.value).into_owned()));
                    }
                }
                Event::SelectionRequest(e) => serve(conn, a, &e, owned.as_deref()),
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

    /// Hand our text to an app that pastes it.
    fn serve(conn: &RustConnection, a: &Atoms, e: &SelectionRequestEvent, text: Option<&str>) {
        let property = if e.property == NONE { e.target } else { e.property };
        let string: Atom = AtomEnum::STRING.into();
        let ok = match text {
            Some(_) if e.selection != a.clipboard => false,
            Some(_) if e.target == a.targets => conn
                .change_property32(
                    PropMode::REPLACE,
                    e.requestor,
                    property,
                    AtomEnum::ATOM,
                    &[a.targets, a.utf8, string, a.text],
                )
                .is_ok(),
            Some(t) if e.target == a.utf8 || e.target == string || e.target == a.text => {
                let ty = if e.target == string { string } else { a.utf8 };
                conn.change_property8(PropMode::REPLACE, e.requestor, property, ty, t.as_bytes()).is_ok()
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
    use std::sync::atomic::{AtomicIsize, Ordering};
    use std::sync::mpsc::{channel, Receiver};
    use std::sync::Arc;
    use windows::core::w;
    use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::DataExchange::{
        AddClipboardFormatListener, CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
        GetClipboardFormatNameW, IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW,
        RemoveClipboardFormatListener, SetClipboardData,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, PostMessageW, RegisterClassW,
        HWND_MESSAGE, MSG, WINDOW_EX_STYLE, WM_APP, WM_CLIPBOARDUPDATE, WNDCLASSW, WS_OVERLAPPED,
    };

    const CF_UNICODETEXT: u32 = 13;
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
                    if let Some(text) = read(hwnd) {
                        sink.send(Report::Clipboard(text));
                    }
                }
                WAKE => {
                    for cmd in rx.try_iter() {
                        match cmd {
                            Cmd::Copy(text) => write(hwnd, &text),
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

    fn read(hwnd: HWND) -> Option<String> {
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
            // SAFETY: the clipboard is open; the handle is read under GlobalLock.
            let h = unsafe { GetClipboardData(CF_UNICODETEXT) }.ok()?;
            let bytes = global_bytes(h)?;
            let wide: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
            Some(String::from_utf16_lossy(&wide[..end]))
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

    fn write(hwnd: HWND, text: &str) {
        if !open(hwnd) {
            return;
        }
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: allocate, fill and hand a movable global to the clipboard,
        // which owns it afterwards.
        unsafe {
            let _ = EmptyClipboard();
            if let Ok(g) = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2) {
                let p = GlobalLock(g) as *mut u16;
                if !p.is_null() {
                    std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
                    let _ = GlobalUnlock(g);
                    let _ = SetClipboardData(CF_UNICODETEXT, Some(HANDLE(g.0)));
                }
            }
            let _ = CloseClipboard();
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{Cmd, Sink, Watcher};
    use islet_core::activity::Report;
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
    use std::sync::mpsc::{channel, RecvTimeoutError};
    use std::time::Duration;

    const EVERY: Duration = Duration::from_millis(500);

    pub fn start(sink: Sink) -> Option<Watcher> {
        let (tx, rx) = channel();
        std::thread::Builder::new()
            .name("clipboard".into())
            .spawn(move || {
                let pb = NSPasteboard::generalPasteboard();
                let mut seen = pb.changeCount();
                loop {
                    match rx.recv_timeout(EVERY) {
                        Ok(Cmd::Copy(text)) => unsafe {
                            pb.clearContents();
                            pb.setString_forType(&objc2_foundation::NSString::from_str(&text), NSPasteboardTypeString);
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
                        if let Some(s) = pb.stringForType(NSPasteboardTypeString) {
                            sink.send(Report::Clipboard(s.to_string()));
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
        if std::env::var_os("DISPLAY").is_none() || x11rb::connect(None).is_err() {
            eprintln!("skipping: no X display");
            return;
        }
        let (tx, rx) = channel();
        let sink = Sink { tx, waker: crate::platform::Waker::new(|| {}) };
        let watcher = start(sink).expect("XFixes watcher");
        let next = |ms: u64| match rx.recv_timeout(Duration::from_millis(ms)) {
            Ok((Report::Clipboard(t), _)) => Some(t),
            _ => None,
        };

        let app = other_app_copies("hello from an app", false, 1.0);
        assert_eq!(next(2000).as_deref(), Some("hello from an app"));
        app.join().unwrap();

        let manager = other_app_copies("hunter2", true, 1.0);
        assert_eq!(next(800), None, "a password manager's copy is never recorded");
        manager.join().unwrap();

        // Choosing a history entry puts it back on the clipboard.
        watcher.copy("from the history".into());
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(paste().as_deref(), Some("from the history"));
        assert_eq!(next(300), None, "Islet's own copy isn't recorded twice");
        drop(watcher);
    }
}
