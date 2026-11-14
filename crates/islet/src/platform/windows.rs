//! Windows backend: a layered popup window updated with
//! `UpdateLayeredWindow` from a 32-bit premultiplied DIB.
//!
//! - Per-pixel alpha with no DWM tricks;
//! - layered windows hit-test by alpha, so clicks on fully transparent
//!   pixels fall through to the window below (no input region needed);
//! - `WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE`: above other
//!   windows, not in the taskbar or Alt-Tab, never steals focus;
//! - the loop sleeps in `MsgWaitForMultipleObjectsEx`: zero wakeups idle;
//!   while animating it wakes on a high-resolution waitable timer, because
//!   a plain millisecond timeout is rounded up to Windows' 15.6 ms tick,
//!   which turns 60 fps into about 32 (the island felt laggy).

use super::{Backend, Button, Event, Key, Result, Waker, WindowSpec};
use islet_core::geometry::Rect;
use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::time::Instant;
use tiny_skia::Pixmap;
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetMonitorInfoW, MonitorFromPoint, SelectObject,
    AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HBITMAP, HDC,
    HGDIOBJ, MONITORINFO, MONITOR_DEFAULTTOPRIMARY,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    GetDpiForMonitor, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, MDT_EFFECTIVE_DPI,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VK_ESCAPE};
use windows::Win32::UI::WindowsAndMessaging::*;

// Defined in Win32_UI_Controls / Win32_System_Threading; declared here to
// avoid pulling in two more feature modules for one constant each.
const WM_MOUSELEAVE: u32 = 0x02A3;
const INFINITE: u32 = u32::MAX;
/// Posted by `Waker` from other threads; only its arrival matters.
const WM_ISLET_WAKE: u32 = WM_APP + 1;
/// Posted by the Caps Lock hook thread: wParam = 1 (on) or 0 (off).
const WM_ISLET_CAPS: u32 = WM_APP + 2;

/// File drops for the shelf (OLE drag and drop). OLE calls these on the
/// window's thread while it pumps messages; events join the usual queue.
#[windows_core::implement(windows::Win32::System::Ole::IDropTarget)]
struct DropTarget {
    hwnd: isize,
}

impl DropTarget {
    /// Screen pixels → the window's logical coordinates.
    fn local(&self, pt: &windows::Win32::Foundation::POINTL) -> (f64, f64) {
        let mut r = windows::Win32::Foundation::RECT::default();
        // SAFETY: our own window handle; `r` is a valid out-pointer.
        let _ = unsafe { GetWindowRect(HWND(self.hwnd as *mut c_void), &mut r) };
        let s = SCALE.with(|s| s.get()).max(0.1);
        ((pt.x - r.left) as f64 / s, (pt.y - r.top) as f64 / s)
    }
}

/// The other half: files dragged out of the shelf. OLE asks this whether
/// the drag goes on (the left button still down) and which cursor to show.
#[windows_core::implement(windows::Win32::System::Ole::IDropSource)]
struct DropSource;

impl windows::Win32::System::Ole::IDropSource_Impl for DropSource_Impl {
    fn QueryContinueDrag(
        &self,
        escape: windows_core::BOOL,
        keys: windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS,
    ) -> windows_core::HRESULT {
        use windows::Win32::Foundation::{DRAGDROP_S_CANCEL, DRAGDROP_S_DROP};
        if escape.as_bool() {
            DRAGDROP_S_CANCEL
        } else if keys.0 & windows::Win32::System::SystemServices::MK_LBUTTON.0 == 0 {
            DRAGDROP_S_DROP
        } else {
            windows_core::HRESULT(0)
        }
    }

    fn GiveFeedback(&self, _effect: windows::Win32::System::Ole::DROPEFFECT) -> windows_core::HRESULT {
        windows::Win32::Foundation::DRAGDROP_S_USEDEFAULTCURSORS
    }
}

/// Drag files out to Explorer, the desktop or any app: the shell makes the
/// data object (as Explorer's own drags do) and OLE runs the drag until the
/// button is let go.
fn drag_files_out(paths: &[String]) -> bool {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::System::Com::IDataObject;
    use windows::Win32::System::Ole::{DoDragDrop, IDropSource, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_LINK};
    use windows::Win32::UI::Shell::Common::ITEMIDLIST;
    use windows::Win32::UI::Shell::{
        BHID_DataObject, IShellItemArray, SHCreateShellItemArrayFromIDLists, SHParseDisplayName,
    };
    // SAFETY: shell and OLE calls on the window's thread (OLE is initialised
    // there); every PIDL from SHParseDisplayName is freed below.
    unsafe {
        let mut pidls: Vec<*const ITEMIDLIST> = Vec::new();
        for p in paths {
            let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
            let name = windows_core::HSTRING::from(p.as_str());
            if SHParseDisplayName(&name, None, &mut pidl, 0, None).is_ok() && !pidl.is_null() {
                pidls.push(pidl);
            }
        }
        let data: windows_core::Result<IDataObject> = if pidls.is_empty() {
            Err(windows_core::Error::empty())
        } else {
            SHCreateShellItemArrayFromIDLists(&pidls)
                .and_then(|items: IShellItemArray| items.BindToHandler(None, &BHID_DataObject))
        };
        for p in pidls {
            CoTaskMemFree(Some(p.cast()));
        }
        let Ok(data) = data else { return false };
        let source: IDropSource = DropSource.into();
        let mut effect = DROPEFFECT::default();
        // Returns once the drop is done or cancelled.
        let _ = DoDragDrop(&data, &source, DROPEFFECT_COPY | DROPEFFECT_LINK, &mut effect);
        true
    }
}

fn hdrop_format() -> windows::Win32::System::Com::FORMATETC {
    windows::Win32::System::Com::FORMATETC {
        cfFormat: windows::Win32::System::Ole::CF_HDROP.0,
        ptd: std::ptr::null_mut(),
        dwAspect: windows::Win32::System::Com::DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: windows::Win32::System::Com::TYMED_HGLOBAL.0 as u32,
    }
}

impl windows::Win32::System::Ole::IDropTarget_Impl for DropTarget_Impl {
    fn DragEnter(
        &self,
        data: windows_core::Ref<windows::Win32::System::Com::IDataObject>,
        _keys: windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS,
        pt: &windows::Win32::Foundation::POINTL,
        effect: *mut windows::Win32::System::Ole::DROPEFFECT,
    ) -> windows_core::Result<()> {
        use windows::Win32::System::Ole::{DROPEFFECT_COPY, DROPEFFECT_NONE};
        // SAFETY: OLE passes a valid data object and effect pointer.
        let files = data.as_ref().is_some_and(|d| unsafe { d.QueryGetData(&hdrop_format()) }.is_ok());
        unsafe { *effect = if files { DROPEFFECT_COPY } else { DROPEFFECT_NONE } };
        if files {
            let (x, y) = self.local(pt);
            push(Event::DragOver { x, y });
        }
        Ok(())
    }

    fn DragOver(
        &self,
        _keys: windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS,
        pt: &windows::Win32::Foundation::POINTL,
        effect: *mut windows::Win32::System::Ole::DROPEFFECT,
    ) -> windows_core::Result<()> {
        // SAFETY: OLE passes a valid effect pointer.
        unsafe { *effect = windows::Win32::System::Ole::DROPEFFECT_COPY };
        let (x, y) = self.local(pt);
        push(Event::DragOver { x, y });
        Ok(())
    }

    fn DragLeave(&self) -> windows_core::Result<()> {
        push(Event::DragLeave);
        Ok(())
    }

    fn Drop(
        &self,
        data: windows_core::Ref<windows::Win32::System::Com::IDataObject>,
        _keys: windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS,
        _pt: &windows::Win32::Foundation::POINTL,
        effect: *mut windows::Win32::System::Ole::DROPEFFECT,
    ) -> windows_core::Result<()> {
        use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};
        let Some(data) = data.as_ref() else { return Ok(()) };
        // SAFETY: CF_HDROP data is an HGLOBAL holding a DROPFILES list; the
        // medium is released after reading it.
        unsafe {
            let mut medium = data.GetData(&hdrop_format())?;
            let drop = HDROP(medium.u.hGlobal.0);
            let count = DragQueryFileW(drop, u32::MAX, None);
            let mut paths = Vec::new();
            for i in 0..count {
                let len = DragQueryFileW(drop, i, None) as usize;
                let mut buf = vec![0u16; len + 1];
                DragQueryFileW(drop, i, Some(&mut buf));
                paths.push(String::from_utf16_lossy(&buf[..len]));
            }
            windows::Win32::System::Ole::ReleaseStgMedium(&mut medium);
            *effect = windows::Win32::System::Ole::DROPEFFECT_COPY;
            if paths.is_empty() {
                push(Event::DragLeave);
            } else {
                push(Event::Drop(paths));
            }
        }
        Ok(())
    }
}

/// The island window (for the hook thread to post to) and Caps Lock's state.
static CAPS_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
static CAPS_ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static CAPS_HELD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Low-level keyboard hook. It looks only at the Caps Lock key (to flip the
/// lock state it shows) and passes every key on untouched; nothing else is
/// read or kept.
unsafe extern "system" fn caps_hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    use std::sync::atomic::Ordering::Relaxed;
    use windows::Win32::UI::Input::KeyboardAndMouse::VK_CAPITAL;
    if code >= 0 {
        // SAFETY: for WH_KEYBOARD_LL, lParam points to a KBDLLHOOKSTRUCT.
        let k = unsafe { &*(lp.0 as *const KBDLLHOOKSTRUCT) };
        if k.vkCode == u32::from(VK_CAPITAL.0) {
            let down = matches!(wp.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
            // Auto-repeat sends more key-downs: flip once per press.
            if down && !CAPS_HELD.swap(true, Relaxed) {
                let on = !CAPS_ON.load(Relaxed);
                CAPS_ON.store(on, Relaxed);
                let hwnd = HWND(CAPS_HWND.load(Relaxed) as *mut c_void);
                // SAFETY: posting to our own window.
                let _ = unsafe { PostMessageW(Some(hwnd), WM_ISLET_CAPS, WPARAM(on as usize), LPARAM(0)) };
            } else if !down {
                CAPS_HELD.store(false, Relaxed);
            }
        }
    }
    // SAFETY: passes the event on to the next hook, as required.
    unsafe { CallNextHookEx(None, code, wp, lp) }
}

thread_local! {
    static QUEUE: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
    static SCALE: Cell<f64> = const { Cell::new(1.0) };
    static TRACKING: Cell<bool> = const { Cell::new(false) };
}

fn push(e: Event) {
    QUEUE.with(|q| q.borrow_mut().push(e));
}

/// Is an app fullscreen on the island's display? Windows says when the
/// foreground window changes, and (from a hook scoped to the foreground
/// app) when that window moves or resizes, so the island never polls.
mod fullscreen {
    use std::cell::Cell;
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetForegroundWindow, GetWindowLongW, GetWindowRect, GetWindowThreadProcessId,
        EVENT_OBJECT_LOCATIONCHANGE, EVENT_SYSTEM_FOREGROUND, GWL_STYLE, OBJID_WINDOW, WINEVENT_OUTOFCONTEXT,
        WINEVENT_SKIPOWNPROCESS, WS_CAPTION,
    };

    thread_local! {
        static DIRTY: Cell<bool> = const { Cell::new(true) };
        static LOCATION_HOOK: Cell<isize> = const { Cell::new(0) };
    }

    unsafe extern "system" fn on_event(
        _hook: HWINEVENTHOOK,
        event: u32,
        hwnd: HWND,
        id_object: i32,
        _child: i32,
        _thread: u32,
        _time: u32,
    ) {
        if event == EVENT_SYSTEM_FOREGROUND {
            follow(hwnd);
        } else if id_object != OBJID_WINDOW.0 || hwnd != unsafe { GetForegroundWindow() } {
            return; // a child window, the caret or the cursor moving
        }
        DIRTY.with(|d| d.set(true));
    }

    /// Watch the foreground app's windows move and resize (entering and
    /// leaving fullscreen is a resize), and nobody else's.
    fn follow(fg: HWND) {
        // SAFETY: unhooking our own hook; reading a window's process id.
        unsafe {
            let old = LOCATION_HOOK.with(|h| h.replace(0));
            if old != 0 {
                let _ = UnhookWinEvent(HWINEVENTHOOK(old as *mut _));
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(fg, Some(&mut pid));
            if pid == 0 || pid == std::process::id() {
                return;
            }
            let hook = SetWinEventHook(
                EVENT_OBJECT_LOCATIONCHANGE,
                EVENT_OBJECT_LOCATIONCHANGE,
                None,
                Some(on_event),
                pid,
                0,
                WINEVENT_OUTOFCONTEXT,
            );
            LOCATION_HOOK.with(|h| h.set(hook.0 as isize));
        }
    }

    pub fn start() {
        // SAFETY: an out-of-context hook delivered to this (the UI) thread's
        // message loop; it lives as long as the process.
        unsafe {
            SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                None,
                Some(on_event),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            );
            follow(GetForegroundWindow());
        }
    }

    /// The program the foreground window belongs to ("chrome", "Code").
    pub fn front_app() -> Option<String> {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        // SAFETY: plain calls; the process handle is closed before returning.
        unsafe {
            let fg = GetForegroundWindow();
            let mut pid = 0u32;
            GetWindowThreadProcessId(fg, Some(&mut pid));
            if pid == 0 || pid == std::process::id() {
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
            if !ok {
                return None;
            }
            let path = String::from_utf16_lossy(&buf[..len as usize]);
            let base = path.rsplit('\\').next().unwrap_or(&path);
            Some(base.strip_suffix(".exe").or_else(|| base.strip_suffix(".EXE")).unwrap_or(base).to_string())
        }
    }

    /// Whether something changed that may have entered or left fullscreen.
    pub fn take_dirty() -> bool {
        DIRTY.with(|d| d.replace(false))
    }

    /// The foreground window covers the island's whole display and has no
    /// title bar (a maximised window keeps its title bar, even when the
    /// taskbar hides).
    pub fn active(island: HWND) -> bool {
        // SAFETY: reading properties of the foreground window and monitors.
        unsafe {
            let fg = GetForegroundWindow();
            if fg.is_invalid() || fg == island {
                return false;
            }
            let mut class = [0u16; 64];
            let n = GetClassNameW(fg, &mut class).max(0) as usize;
            let class = String::from_utf16_lossy(&class[..n]);
            if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd") {
                return false; // the desktop or the taskbar
            }
            if (GetWindowLongW(fg, GWL_STYLE) as u32) & WS_CAPTION.0 == WS_CAPTION.0 {
                return false;
            }
            let monitor = MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST);
            if monitor != MonitorFromWindow(island, MONITOR_DEFAULTTONEAREST) {
                return false; // fullscreen on another display
            }
            let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            let mut r = RECT::default();
            if !GetMonitorInfoW(monitor, &mut mi).as_bool() || GetWindowRect(fg, &mut r).is_err() {
                return false;
            }
            let m = mi.rcMonitor;
            r.left <= m.left && r.top <= m.top && r.right >= m.right && r.bottom >= m.bottom
        }
    }
}

pub struct Win32 {
    hwnd: HWND,
    /// The Caps Lock hook thread, while the caps-lock pop-up is on.
    caps_thread: Option<u32>,
    spec: WindowSpec,
    mem_dc: HDC,
    dib: HBITMAP,
    old_obj: HGDIOBJ,
    bits: *mut u8,
    size: (u32, u32),
    pos: POINT,
    scale: f64,
    /// Wakes the loop for the next frame (high resolution where Windows has it).
    timer: windows::Win32::Foundation::HANDLE,
    /// Last fullscreen state reported.
    fullscreen: bool,
    /// The front app last reported.
    front: String,
}

/// A waitable timer that keeps time to well under a millisecond (Windows 10
/// 1803+); an ordinary one before that.
fn frame_timer() -> windows::Win32::Foundation::HANDLE {
    use windows::Win32::System::Threading::{
        CreateWaitableTimerExW, CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, TIMER_ALL_ACCESS,
    };
    // SAFETY: no attributes or name; the handle is closed on drop.
    unsafe {
        CreateWaitableTimerExW(None, None, CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, TIMER_ALL_ACCESS.0)
            .or_else(|_| CreateWaitableTimerExW(None, None, 0, TIMER_ALL_ACCESS.0))
            .unwrap_or_default()
    }
}

fn we<E: std::fmt::Display>(e: E) -> String {
    format!("Win32: {e}")
}

/// Every monitor with its device name ("DISPLAY2"), primary first.
fn monitors() -> Vec<(windows::Win32::Graphics::Gdi::HMONITOR, String)> {
    use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR, MONITORINFOEXW};
    unsafe extern "system" fn each(
        m: HMONITOR,
        _: HDC,
        _: *mut windows::Win32::Foundation::RECT,
        data: LPARAM,
    ) -> windows_core::BOOL {
        // SAFETY: `data` is the Vec passed below, alive for the call.
        let list = unsafe { &mut *(data.0 as *mut Vec<(HMONITOR, String, bool)>) };
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        // SAFETY: a correctly sized MONITORINFOEXW.
        if unsafe { GetMonitorInfoW(m, &mut info as *mut MONITORINFOEXW as *mut MONITORINFO) }.as_bool() {
            let end = info.szDevice.iter().position(|&c| c == 0).unwrap_or(info.szDevice.len());
            let name = String::from_utf16_lossy(&info.szDevice[..end]);
            list.push((m, name.trim_start_matches(r"\\.\").to_string(), info.monitorInfo.dwFlags & 1 != 0));
        }
        true.into()
    }
    let mut list: Vec<(HMONITOR, String, bool)> = Vec::new();
    // SAFETY: the callback only touches `list` during the call.
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(each), LPARAM(&mut list as *mut _ as isize));
    }
    list.sort_by_key(|m| !m.2);
    list.into_iter().map(|(m, n, _)| (m, n)).collect()
}

pub fn displays() -> Vec<String> {
    monitors().into_iter().map(|(_, n)| n).collect()
}

/// The chosen monitor's rect (physical px) and scale factor; else the primary one.
fn primary_monitor() -> ((i32, i32), (u32, u32), f64) {
    // SAFETY: plain Win32 queries with valid out-pointers.
    unsafe {
        let wanted = super::preferred_display();
        let mon = monitors()
            .into_iter()
            .find(|(_, n)| wanted.as_deref() == Some(n.as_str()))
            .map(|(m, _)| m)
            .unwrap_or_else(|| MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY));
        let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let rc = if GetMonitorInfoW(mon, &mut info).as_bool() {
            info.rcMonitor
        } else {
            windows::Win32::Foundation::RECT { left: 0, top: 0, right: 1920, bottom: 1080 }
        };
        let (mut dx, mut dy) = (96u32, 96u32);
        let scale =
            if GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy).is_ok() { dx as f64 / 96.0 } else { 1.0 };
        ((rc.left, rc.top), ((rc.right - rc.left) as u32, (rc.bottom - rc.top) as u32), scale)
    }
}

impl Win32 {
    /// (Re)create the DIB and move/resize the window for the current monitor and scale.
    fn configure(&mut self) -> Result<()> {
        let (mpos, msize, scale) = primary_monitor();
        self.scale = scale;
        SCALE.with(|s| s.set(scale));
        self.size = ((self.spec.width * scale).round() as u32, (self.spec.height * scale).round() as u32);
        let (x, y) = super::placement().origin(mpos, msize, self.size);
        self.pos = POINT { x, y };
        // SAFETY: GDI objects are created and selected on this thread and
        // released in drop/reconfigure; `bits` stays valid while `dib` lives.
        unsafe {
            if !self.dib.is_invalid() {
                SelectObject(self.mem_dc, self.old_obj);
                let _ = DeleteObject(self.dib.into());
            }
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: self.size.0 as i32,
                    biHeight: -(self.size.1 as i32), // top-down rows
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits: *mut c_void = std::ptr::null_mut();
            self.dib = CreateDIBSection(Some(self.mem_dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0).map_err(we)?;
            self.bits = bits.cast();
            self.old_obj = SelectObject(self.mem_dc, self.dib.into());
            SetWindowPos(self.hwnd, Some(HWND_TOPMOST), x, y, self.size.0 as i32, self.size.1 as i32, SWP_NOACTIVATE)
                .map_err(we)?;
        }
        Ok(())
    }
}

impl Backend for Win32 {
    fn create(spec: WindowSpec) -> Result<Self> {
        // SAFETY: standard window-class registration and window creation.
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let hinstance = GetModuleHandleW(None).map_err(we)?;
            let class = w!("IsletIsland");
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(wndproc),
                hInstance: hinstance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW).map_err(we)?,
                lpszClassName: class,
                ..Default::default()
            };
            if RegisterClassExW(&wc) == 0 {
                return Err("RegisterClassExW failed".into());
            }
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class,
                w!("Islet"),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(hinstance.into()),
                None,
            )
            .map_err(we)?;
            let mem_dc = CreateCompatibleDC(None);
            let mut me = Self {
                hwnd,
                spec,
                mem_dc,
                dib: HBITMAP::default(),
                caps_thread: None,
                old_obj: HGDIOBJ::default(),
                bits: std::ptr::null_mut(),
                size: (1, 1),
                pos: POINT::default(),
                scale: 1.0,
                timer: frame_timer(),
                fullscreen: false,
                front: String::new(),
            };
            me.configure()?;
            // Accept dropped files for the shelf (OLE needs a single-threaded
            // apartment on this thread).
            let trace = std::env::var_os("ISLET_TRACE_STARTUP").is_some();
            if trace {
                eprintln!("ISLET_TRACE window configured; registering the drop target");
            }
            if windows::Win32::System::Ole::OleInitialize(None).is_ok() {
                let target: windows::Win32::System::Ole::IDropTarget = DropTarget { hwnd: hwnd.0 as isize }.into();
                let _ = windows::Win32::System::Ole::RegisterDragDrop(hwnd, &target);
            }
            if trace {
                eprintln!("ISLET_TRACE drop target done");
            }
            Ok(me)
        }
    }

    fn show(&mut self) {
        // SAFETY: our own window handle.
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
        }
        fullscreen::start();
    }

    fn scale(&self) -> f64 {
        self.scale
    }

    fn set_hotkey(&mut self, slot: u8, hotkey: Option<islet_core::hotkey::Hotkey>) -> Result<()> {
        use islet_core::hotkey::Key as K;
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
        };
        // Each slot has its own id; WM_HOTKEY says which one fired.
        let id = i32::from(slot) + 1;
        // SAFETY: our own window; unregistering an unused id is harmless.
        unsafe {
            let _ = UnregisterHotKey(Some(self.hwnd), id);
        }
        let Some(hk) = hotkey else { return Ok(()) };
        let mut mods = MOD_NOREPEAT;
        for (on, m) in [(hk.alt, MOD_ALT), (hk.ctrl, MOD_CONTROL), (hk.shift, MOD_SHIFT), (hk.logo, MOD_WIN)] {
            if on {
                mods |= m;
            }
        }
        let vk = match hk.key {
            K::Char(c) => c as u32, // 'A'–'Z' and '0'–'9' are their own key codes
            K::Space => 0x20,
            K::F(n) => 0x70 + u32::from(n) - 1,
        };
        // SAFETY: registering for our own window.
        unsafe { RegisterHotKey(Some(self.hwnd), id, HOT_KEY_MODIFIERS(mods.0), vk) }
            .map_err(|_| format!("{} is already used by another app", hk.label()))
    }

    fn size_px(&self) -> (u32, u32) {
        self.size
    }

    fn present(&mut self, frame: &Pixmap) -> Result<()> {
        if (frame.width(), frame.height()) != self.size || self.bits.is_null() {
            return Ok(()); // a resize is pending; the next frame will match
        }
        let src = frame.data();
        // SAFETY: the DIB holds exactly width*height*4 bytes (top-down, 32bpp).
        let dst = unsafe { std::slice::from_raw_parts_mut(self.bits, src.len()) };
        islet_render::rgba_to_bgra(src, dst);
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let size = SIZE { cx: self.size.0 as i32, cy: self.size.1 as i32 };
        let origin = POINT { x: 0, y: 0 };
        // SAFETY: all pointers refer to live locals; mem_dc has our DIB selected.
        unsafe {
            UpdateLayeredWindow(
                self.hwnd,
                None,
                Some(&self.pos),
                Some(&size),
                Some(self.mem_dc),
                Some(&origin),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            )
            .map_err(we)
        }
    }

    fn set_input_region(&mut self, _rect: Rect) -> Result<()> {
        // Layered windows already ignore clicks on fully transparent pixels.
        Ok(())
    }

    fn native_handle(&self) -> isize {
        self.hwnd.0 as isize
    }

    fn start_drag(&mut self, paths: &[String]) -> bool {
        drag_files_out(paths)
    }

    fn watch_caps_lock(&mut self, on: bool) {
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CAPITAL};
        match (on, self.caps_thread.take()) {
            (true, None) => {
                CAPS_HWND.store(self.hwnd.0 as isize, std::sync::atomic::Ordering::Relaxed);
                // SAFETY: GetKeyState has no preconditions; bit 0 is the toggle state.
                let now = unsafe { GetKeyState(i32::from(VK_CAPITAL.0)) } & 1 != 0;
                CAPS_ON.store(now, std::sync::atomic::Ordering::Relaxed);
                let (tx, rx) = std::sync::mpsc::channel();
                let _ = std::thread::Builder::new().name("islet-capslock".into()).spawn(move || {
                    // SAFETY: the hook lives on this thread, which pumps messages
                    // until WM_QUIT, then removes it.
                    unsafe {
                        use windows::Win32::System::Threading::GetCurrentThreadId;
                        let Ok(hook) = SetWindowsHookExW(WH_KEYBOARD_LL, Some(caps_hook), None, 0) else { return };
                        let _ = tx.send(GetCurrentThreadId());
                        let mut msg = MSG::default();
                        while GetMessageW(&mut msg, None, 0, 0).as_bool() {}
                        let _ = UnhookWindowsHookEx(hook);
                    }
                });
                self.caps_thread = rx.recv().ok();
            }
            (false, Some(thread)) => {
                // SAFETY: asks the hook thread to quit (it unhooks itself).
                let _ = unsafe { PostThreadMessageW(thread, WM_QUIT, WPARAM(0), LPARAM(0)) };
            }
            (_, keep) => self.caps_thread = keep,
        }
    }

    fn waker(&self) -> Waker {
        // HWND is not Send; the handle value is, and PostMessageW is thread-safe.
        let hwnd = self.hwnd.0 as isize;
        Waker::new(move || {
            // SAFETY: posting a message to our own window from any thread.
            let _ = unsafe { PostMessageW(Some(HWND(hwnd as *mut c_void)), WM_ISLET_WAKE, WPARAM(0), LPARAM(0)) };
        })
    }

    fn wait(&mut self, deadline: Option<Instant>, out: &mut Vec<Event>) -> Result<()> {
        use windows::Win32::System::Threading::SetWaitableTimer;
        let wait = deadline.map(|d| d.saturating_duration_since(Instant::now()));
        let timer = [self.timer];
        // SAFETY: standard message pump on the thread that owns the window;
        // the timer handle lives as long as `self`.
        unsafe {
            match wait {
                // Nothing scheduled: sleep until a message arrives.
                None => {
                    MsgWaitForMultipleObjectsEx(None, INFINITE, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
                }
                Some(w) if w.is_zero() => {}
                Some(w) if !self.timer.is_invalid() => {
                    // Relative due time, in 100 ns units (negative = relative).
                    let due = -((w.as_nanos() / 100).clamp(1, i64::MAX as u128) as i64);
                    if SetWaitableTimer(self.timer, &due, 0, None, None, false).is_ok() {
                        MsgWaitForMultipleObjectsEx(Some(&timer), INFINITE, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
                    } else {
                        let ms = w.as_micros().div_ceil(1000).min(u32::MAX as u128 - 1) as u32;
                        MsgWaitForMultipleObjectsEx(None, ms, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
                    }
                }
                Some(w) => {
                    let ms = w.as_micros().div_ceil(1000).min(u32::MAX as u128 - 1) as u32;
                    MsgWaitForMultipleObjectsEx(None, ms, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
                }
            }
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    push(Event::Quit);
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        let events = QUEUE.with(|q| std::mem::take(&mut *q.borrow_mut()));
        for e in events {
            if let Event::ScaleChanged(_) = e {
                self.configure()?;
            }
            out.push(e);
        }
        if fullscreen::take_dirty() {
            let now = fullscreen::active(self.hwnd);
            if now != self.fullscreen {
                self.fullscreen = now;
                out.push(Event::Fullscreen(now));
            }
            if let Some(app) = fullscreen::front_app().filter(|a| *a != self.front) {
                self.front = app.clone();
                out.push(Event::FrontApp(app));
            }
        }
        Ok(())
    }
}

impl Drop for Win32 {
    fn drop(&mut self) {
        // SAFETY: releasing GDI objects and the window we created.
        unsafe {
            SelectObject(self.mem_dc, self.old_obj);
            let _ = DeleteObject(self.dib.into());
            let _ = DeleteDC(self.mem_dc);
            let _ = DestroyWindow(self.hwnd);
            if !self.timer.is_invalid() {
                let _ = windows::Win32::Foundation::CloseHandle(self.timer);
            }
        }
    }
}

fn loword(v: isize) -> i16 {
    (v & 0xffff) as u16 as i16
}

fn hiword(v: isize) -> i16 {
    ((v >> 16) & 0xffff) as u16 as i16
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let s = SCALE.with(|c| c.get());
    let (x, y) = (loword(lp.0) as f64 / s, hiword(lp.0) as f64 / s);
    let handled = |e: Event| {
        push(e);
        LRESULT(0)
    };
    match msg {
        WM_MOUSEMOVE => {
            if !TRACKING.with(|t| t.replace(true)) {
                let mut tme = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                // SAFETY: tme is a valid, correctly sized struct.
                let _ = unsafe { TrackMouseEvent(&mut tme) };
            }
            handled(Event::PointerMove { x, y })
        }
        WM_MOUSELEAVE => {
            TRACKING.with(|t| t.set(false));
            handled(Event::PointerLeave)
        }
        WM_LBUTTONDOWN => handled(Event::PointerDown { x, y, button: Button::Primary }),
        WM_RBUTTONDOWN => handled(Event::PointerDown { x, y, button: Button::Secondary }),
        WM_MBUTTONDOWN => handled(Event::PointerDown { x, y, button: Button::Middle }),
        WM_LBUTTONUP => handled(Event::PointerUp { x, y, button: Button::Primary }),
        WM_RBUTTONUP => handled(Event::PointerUp { x, y, button: Button::Secondary }),
        WM_MBUTTONUP => handled(Event::PointerUp { x, y, button: Button::Middle }),
        WM_MOUSEWHEEL => handled(Event::Scroll { dx: 0.0, dy: -(hiword(wp.0 as isize) as f64) / 120.0 }),
        WM_MOUSEHWHEEL => handled(Event::Scroll { dx: hiword(wp.0 as isize) as f64 / 120.0, dy: 0.0 }),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_KEYDOWN if wp.0 == VK_ESCAPE.0 as usize => handled(Event::Key(Key::Escape)),
        WM_ISLET_CAPS => handled(Event::CapsLock(wp.0 != 0)),
        WM_DPICHANGED | WM_DISPLAYCHANGE => handled(Event::ScaleChanged(s)),
        WM_CLOSE => handled(Event::Quit),
        WM_ISLET_WAKE => LRESULT(0),
        WM_HOTKEY if (1..=crate::platform::HOTKEY_SLOTS).contains(&wp.0) => handled(Event::Hotkey((wp.0 - 1) as u8)),
        // SAFETY: default handling for everything else.
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}
