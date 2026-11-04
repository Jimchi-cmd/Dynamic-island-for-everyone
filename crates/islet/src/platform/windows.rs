//! Windows backend: a layered popup window updated with
//! `UpdateLayeredWindow` from a 32-bit premultiplied DIB.
//!
//! - Per-pixel alpha with no DWM tricks;
//! - layered windows hit-test by alpha, so clicks on fully transparent
//!   pixels fall through to the window below (no input region needed);
//! - `WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE`: above other
//!   windows, not in the taskbar or Alt-Tab, never steals focus;
//! - the loop sleeps in `MsgWaitForMultipleObjectsEx`: zero wakeups idle.

use super::{Backend, Button, Event, Key, Result, Waker, WindowSpec};
use islet_core::geometry::{top_center, Rect};
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

thread_local! {
    static QUEUE: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
    static SCALE: Cell<f64> = const { Cell::new(1.0) };
    static TRACKING: Cell<bool> = const { Cell::new(false) };
}

fn push(e: Event) {
    QUEUE.with(|q| q.borrow_mut().push(e));
}

pub struct Win32 {
    hwnd: HWND,
    spec: WindowSpec,
    mem_dc: HDC,
    dib: HBITMAP,
    old_obj: HGDIOBJ,
    bits: *mut u8,
    size: (u32, u32),
    pos: POINT,
    scale: f64,
}

fn we<E: std::fmt::Display>(e: E) -> String {
    format!("Win32: {e}")
}

/// Primary monitor rect (physical px) and its scale factor.
fn primary_monitor() -> ((i32, i32), (u32, u32), f64) {
    // SAFETY: plain Win32 queries with valid out-pointers.
    unsafe {
        let mon = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
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
        let (x, y) = top_center(mpos, msize, self.size.0);
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
                old_obj: HGDIOBJ::default(),
                bits: std::ptr::null_mut(),
                size: (1, 1),
                pos: POINT::default(),
                scale: 1.0,
            };
            me.configure()?;
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            Ok(me)
        }
    }

    fn scale(&self) -> f64 {
        self.scale
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
        for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
            // Premultiplied RGBA → premultiplied BGRA.
            d.copy_from_slice(&[s[2], s[1], s[0], s[3]]);
        }
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

    fn waker(&self) -> Waker {
        // HWND is not Send; the handle value is, and PostMessageW is thread-safe.
        let hwnd = self.hwnd.0 as isize;
        Waker::new(move || {
            // SAFETY: posting a message to our own window from any thread.
            let _ = unsafe { PostMessageW(Some(HWND(hwnd as *mut c_void)), WM_ISLET_WAKE, WPARAM(0), LPARAM(0)) };
        })
    }

    fn wait(&mut self, deadline: Option<Instant>, out: &mut Vec<Event>) -> Result<()> {
        let timeout = match deadline {
            None => INFINITE,
            Some(d) => {
                d.saturating_duration_since(Instant::now()).as_micros().div_ceil(1000).min(u32::MAX as u128 - 1) as u32
            }
        };
        // SAFETY: standard message pump on the thread that owns the window.
        unsafe {
            MsgWaitForMultipleObjectsEx(None, timeout, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
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
        WM_DPICHANGED | WM_DISPLAYCHANGE => handled(Event::ScaleChanged(s)),
        WM_CLOSE => handled(Event::Quit),
        WM_ISLET_WAKE => LRESULT(0),
        // SAFETY: default handling for everything else.
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}
