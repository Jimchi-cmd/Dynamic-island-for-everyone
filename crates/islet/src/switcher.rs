//! The app switcher: the windows open on the desktop, and bringing one to
//! the front (`islet windows`, used by the launcher). X11 through the
//! window manager's EWMH lists, sway and Hyprland through their own
//! commands, Windows through its top-level windows, and macOS by app
//! (its window titles need the Screen Recording permission).

pub use islet_core::switcher::OpenWindow;
use islet_core::switcher::{keep, valid_id};

/// The windows open now, in the order the system gives them.
pub fn list() -> Result<Vec<OpenWindow>, String> {
    imp::list()
}

/// Bring one of the listed windows to the front.
pub fn focus(id: &str) -> Result<(), String> {
    if !valid_id(id) {
        return Err(format!("“{id}” isn't a window id from `islet windows`"));
    }
    if !list()?.iter().any(|w| w.id == id) {
        return Err("that window has closed".into());
    }
    imp::focus(id)
}

#[cfg(target_os = "windows")]
pub use imp::process_name;

#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    use super::{keep, OpenWindow};
    use std::process::Stdio;

    fn run(cmd: &str, args: &[&str]) -> Result<String, String> {
        let out = crate::hidden::command(cmd)
            .args(args)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .map_err(|e| format!("{cmd}: {e}"))?;
        if !out.status.success() {
            return Err(format!("{cmd} failed"));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    enum Desktop {
        Sway,
        Hyprland,
        OtherWayland,
        X11,
    }

    fn desktop() -> Desktop {
        let set = |k: &str| std::env::var_os(k).is_some_and(|v| !v.is_empty());
        if set("SWAYSOCK") {
            Desktop::Sway
        } else if set("HYPRLAND_INSTANCE_SIGNATURE") {
            Desktop::Hyprland
        } else if set("WAYLAND_DISPLAY") {
            Desktop::OtherWayland
        } else {
            Desktop::X11
        }
    }

    const NO_WAYLAND: &str = "this Wayland desktop doesn't let other programs list or switch its windows \
        (sway, Hyprland and X11 desktops do)";

    pub fn list() -> Result<Vec<OpenWindow>, String> {
        match desktop() {
            Desktop::Sway => Ok(islet_core::switcher::parse_sway_tree(&run("swaymsg", &["-t", "get_tree", "-r"])?)),
            Desktop::Hyprland => Ok(islet_core::switcher::parse_hyprland_clients(&run("hyprctl", &["clients", "-j"])?)),
            Desktop::OtherWayland => Err(NO_WAYLAND.into()),
            Desktop::X11 => x11::list(),
        }
    }

    pub fn focus(id: &str) -> Result<(), String> {
        match desktop() {
            Desktop::Sway => run("swaymsg", &[&format!("[con_id={id}]"), "focus"]).map(drop),
            Desktop::Hyprland => run("hyprctl", &["dispatch", "focuswindow", &format!("address:{id}")]).map(drop),
            Desktop::OtherWayland => Err(NO_WAYLAND.into()),
            Desktop::X11 => x11::focus(id),
        }
    }

    pub mod x11 {
        use super::{keep, OpenWindow};
        use x11rb::connection::Connection;
        use x11rb::protocol::xproto::*;
        use x11rb::rust_connection::RustConnection;

        fn e(err: impl std::fmt::Display) -> String {
            format!("X11: {err}")
        }

        fn atom(c: &RustConnection, name: &str) -> Result<Atom, String> {
            Ok(c.intern_atom(false, name.as_bytes()).map_err(e)?.reply().map_err(e)?.atom)
        }

        fn prop32(c: &RustConnection, w: Window, name: Atom, kind: impl Into<Atom>) -> Vec<u32> {
            c.get_property(false, w, name, kind, 0, 4096)
                .ok()
                .and_then(|r| r.reply().ok())
                .and_then(|r| r.value32().map(Iterator::collect))
                .unwrap_or_default()
        }

        fn text(c: &RustConnection, w: Window, name: Atom, kind: impl Into<Atom>) -> Option<String> {
            let r = c.get_property(false, w, name, kind, 0, 1024).ok()?.reply().ok()?;
            (!r.value.is_empty()).then(|| String::from_utf8_lossy(&r.value).into_owned())
        }

        /// The windows in the window manager's `_NET_CLIENT_LIST`, less
        /// those it keeps off the taskbar.
        pub fn list_on(c: &RustConnection, root: Window) -> Result<Vec<OpenWindow>, String> {
            let (clients, name, utf8, state, skip) = (
                atom(c, "_NET_CLIENT_LIST")?,
                atom(c, "_NET_WM_NAME")?,
                atom(c, "UTF8_STRING")?,
                atom(c, "_NET_WM_STATE")?,
                atom(c, "_NET_WM_STATE_SKIP_TASKBAR")?,
            );
            let mut out = Vec::new();
            for w in prop32(c, root, clients, AtomEnum::WINDOW) {
                if prop32(c, w, state, AtomEnum::ATOM).contains(&skip) {
                    continue;
                }
                let title = text(c, w, name, utf8)
                    .or_else(|| text(c, w, AtomEnum::WM_NAME.into(), AtomEnum::STRING))
                    .unwrap_or_default();
                // "instance\0Class\0": the class is the program's name.
                let class = text(c, w, AtomEnum::WM_CLASS.into(), AtomEnum::STRING).unwrap_or_default();
                let mut parts = class.split('\0').filter(|p| !p.is_empty());
                let instance = parts.next();
                let app = parts.next().or(instance).unwrap_or_default().to_string();
                out.push(OpenWindow { id: w.to_string(), title, app });
            }
            out.retain(keep);
            Ok(out)
        }

        pub fn list() -> Result<Vec<OpenWindow>, String> {
            let (c, n) = x11rb::connect(None).map_err(e)?;
            let root = c.setup().roots[n].root;
            list_on(&c, root)
        }

        /// Ask the window manager to activate the window, as a taskbar does.
        pub fn focus(id: &str) -> Result<(), String> {
            let w: Window = id.parse().map_err(e)?;
            let (c, n) = x11rb::connect(None).map_err(e)?;
            let root = c.setup().roots[n].root;
            let active = atom(&c, "_NET_ACTIVE_WINDOW")?;
            // Source 2: a pager or taskbar, which window managers obey.
            let ev = ClientMessageEvent::new(32, w, active, [2, x11rb::CURRENT_TIME, 0, 0, 0]);
            c.send_event(false, root, EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY, ev)
                .map_err(e)?;
            c.flush().map_err(e)?;
            Ok(())
        }
    }
}

#[cfg(target_os = "windows")]
mod imp {
    use super::{keep, OpenWindow};
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindow, GetWindowLongW, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
        SetForegroundWindow, ShowWindow, GWL_EXSTYLE, GW_OWNER, SW_RESTORE, WS_EX_TOOLWINDOW,
    };

    /// A program's name from its process id ("firefox").
    pub fn process_name(pid: u32) -> Option<String> {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        // SAFETY: plain calls; the handle is closed before returning.
        unsafe {
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
            let path = String::from_utf16_lossy(&buf[..len as usize]);
            let base = path.rsplit('\\').next().unwrap_or(&path).to_string();
            ok.then(|| base.strip_suffix(".exe").unwrap_or(&base).to_string())
        }
    }

    /// A window the taskbar would show: visible, not owned by another, not
    /// a tool window, and not hidden on another virtual desktop.
    fn on_taskbar(h: HWND) -> bool {
        // SAFETY: plain queries on a window handle the system just gave us.
        unsafe {
            if !IsWindowVisible(h).as_bool() || GetWindow(h, GW_OWNER).is_ok_and(|o| !o.is_invalid()) {
                return false;
            }
            if GetWindowLongW(h, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 != 0 {
                return false;
            }
            let mut cloaked = 0u32;
            let size = std::mem::size_of::<u32>() as u32;
            let _ = DwmGetWindowAttribute(h, DWMWA_CLOAKED, (&raw mut cloaked).cast(), size);
            cloaked == 0
        }
    }

    unsafe extern "system" fn each(h: HWND, out: LPARAM) -> windows::core::BOOL {
        // SAFETY: `out` is the Vec passed to EnumWindows below, alive for the call.
        let out = unsafe { &mut *(out.0 as *mut Vec<OpenWindow>) };
        if on_taskbar(h) {
            let mut buf = [0u16; 512];
            // SAFETY: a live window handle and a buffer of the size given.
            let n = unsafe { GetWindowTextW(h, &mut buf) }.max(0) as usize;
            let mut pid = 0u32;
            // SAFETY: as above.
            unsafe { GetWindowThreadProcessId(h, Some(&mut pid)) };
            out.push(OpenWindow {
                id: (h.0 as usize).to_string(),
                title: String::from_utf16_lossy(&buf[..n]),
                app: process_name(pid).unwrap_or_default(),
            });
        }
        true.into()
    }

    pub fn list() -> Result<Vec<OpenWindow>, String> {
        let mut out: Vec<OpenWindow> = Vec::new();
        // SAFETY: the callback only touches `out`, which outlives the call.
        unsafe { EnumWindows(Some(each), LPARAM(&raw mut out as isize)) }.map_err(|e| e.message())?;
        out.retain(keep);
        Ok(out)
    }

    pub fn focus(id: &str) -> Result<(), String> {
        let h = HWND(id.parse::<usize>().map_err(|e| e.to_string())? as *mut _);
        // SAFETY: a handle from the list just read; Windows checks it again.
        unsafe {
            if IsIconic(h).as_bool() {
                let _ = ShowWindow(h, SW_RESTORE);
            }
            if SetForegroundWindow(h).as_bool() {
                Ok(())
            } else {
                Err("Windows didn't let Islet bring that window to the front".into())
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::{keep, OpenWindow};
    use objc2_app_kit::{NSApplicationActivationPolicy, NSRunningApplication, NSWorkspace};

    /// macOS shows other apps' window titles only to apps allowed to record
    /// the screen, so the switcher lists the apps with windows instead.
    pub fn list() -> Result<Vec<OpenWindow>, String> {
        let mut out = Vec::new();
        for app in NSWorkspace::sharedWorkspace().runningApplications().iter() {
            if app.activationPolicy() != NSApplicationActivationPolicy::Regular {
                continue;
            }
            let name = app.localizedName().map(|n| n.to_string()).unwrap_or_default();
            out.push(OpenWindow { id: app.processIdentifier().to_string(), title: name.clone(), app: name });
        }
        out.retain(keep);
        Ok(out)
    }

    pub fn focus(id: &str) -> Result<(), String> {
        let pid: i32 = id.parse().map_err(|e: std::num::ParseIntError| e.to_string())?;
        let app = NSRunningApplication::runningApplicationWithProcessIdentifier(pid).ok_or("that app has quit")?;
        // Opening a running app's bundle brings it forward, which macOS
        // allows from any program (a plain activate may be ignored).
        let path = app.bundleURL().and_then(|u| u.path()).map(|p| p.to_string());
        let Some(path) = path else {
            return Err("that app can't be brought to the front".into());
        };
        let ok = crate::hidden::command("open")
            .arg(path)
            .stdin(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if ok {
            Ok(())
        } else {
            Err("macOS didn't bring that app to the front".into())
        }
    }
}

#[cfg(all(test, all(unix, not(target_os = "macos"))))]
mod tests {
    //! Needs an X display (`xvfb-run`); skips without one.
    use super::imp::x11::list_on;
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::*;
    use x11rb::wrapper::ConnectionExt as _;

    #[test]
    fn lists_the_window_managers_clients() {
        if std::env::var_os("DISPLAY").is_none() {
            return;
        }
        let Ok((c, n)) = x11rb::connect(None) else { return };
        let root = c.setup().roots[n].root;
        let atom = |s: &str| c.intern_atom(false, s.as_bytes()).unwrap().reply().unwrap().atom;
        let make = |title: &str, class: &[u8]| {
            let w = c.generate_id().unwrap();
            c.create_window(0, w, root, 0, 0, 10, 10, 0, WindowClass::INPUT_OUTPUT, 0, &Default::default()).unwrap();
            c.change_property8(PropMode::REPLACE, w, atom("_NET_WM_NAME"), atom("UTF8_STRING"), title.as_bytes())
                .unwrap();
            c.change_property8(PropMode::REPLACE, w, AtomEnum::WM_CLASS, AtomEnum::STRING, class).unwrap();
            w
        };
        let editor = make("notes.txt — Editor", b"editor\0Editor\0");
        let panel = make("Panel", b"panel\0Panel\0");
        let island = make("Islet", b"islet\0Islet\0");
        let skip = atom("_NET_WM_STATE_SKIP_TASKBAR");
        c.change_property32(PropMode::REPLACE, panel, atom("_NET_WM_STATE"), AtomEnum::ATOM, &[skip]).unwrap();
        // What a window manager keeps on the root window.
        let list = atom("_NET_CLIENT_LIST");
        let old = c.get_property(false, root, list, AtomEnum::WINDOW, 0, 4096).unwrap().reply().unwrap();
        c.change_property32(PropMode::REPLACE, root, list, AtomEnum::WINDOW, &[editor, panel, island]).unwrap();
        c.flush().unwrap();
        let found = list_on(&c, root).unwrap();
        // Put back what was there (another test may be using the display).
        if old.value.is_empty() {
            c.delete_property(root, list).unwrap();
        } else {
            c.change_property(PropMode::REPLACE, root, list, AtomEnum::WINDOW, 32, old.value_len, &old.value).unwrap();
        }
        c.flush().unwrap();
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!((found[0].title.as_str(), found[0].app.as_str()), ("notes.txt — Editor", "Editor"));
        assert_eq!(found[0].id, editor.to_string());
    }
}
