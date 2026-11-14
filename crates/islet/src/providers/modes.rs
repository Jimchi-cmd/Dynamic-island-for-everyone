//! Airplane mode and Do Not Disturb, watched without polling where the
//! system allows. Linux: rfkill events for airplane mode; GNOME's banner
//! setting or KDE's notification inhibition for Do Not Disturb. Windows:
//! the radio-management registry value for airplane mode, and Windows 11's
//! Focus sessions for Do Not Disturb. macOS: Macs have no airplane mode,
//! and Apple shares Focus only with entitled App Store apps, so nothing.

use super::Sink;
#[cfg(not(target_os = "macos"))]
use islet_core::activity::Report;

#[cfg(not(target_os = "macos"))]
fn airplane(sink: &Sink, on: bool) {
    sink.send(Report::Modes { airplane: Some(on), dnd: None });
}

#[cfg(not(target_os = "macos"))]
fn dnd(sink: &Sink, on: bool) {
    sink.send(Report::Modes { airplane: None, dnd: Some(on) });
}

pub fn start(sink: Sink) {
    imp::start(sink);
}

#[cfg(target_os = "linux")]
mod imp {
    use super::{airplane, dnd, Sink};
    use islet_core::modes::{gnome_dnd, parse_rfkill, Radios};
    use std::io::{BufRead, BufReader};
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::process::{Command, Stdio};

    pub fn start(sink: Sink) {
        let s = sink.clone();
        let _ = std::thread::Builder::new().name("islet-rfkill".into()).spawn(move || watch_rfkill(&s));
        let _ = std::thread::Builder::new().name("islet-dnd".into()).spawn(move || watch_dnd(&sink));
    }

    /// /dev/rfkill sends every radio when opened, then each change.
    fn watch_rfkill(sink: &Sink) {
        // SAFETY: opening a device read-only; the fd is owned below.
        let fd = unsafe { libc::open(c"/dev/rfkill".as_ptr(), libc::O_RDONLY | libc::O_NONBLOCK | libc::O_CLOEXEC) };
        if fd < 0 {
            super::super::debug_log("airplane mode unavailable (no /dev/rfkill)");
            return;
        }
        // SAFETY: a valid descriptor we just opened.
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        let mut radios = Radios::default();
        let mut last = None;
        loop {
            // Read all that's waiting, then say how things stand.
            let mut buf = [0u8; 64];
            loop {
                // SAFETY: reading into a local buffer from our descriptor.
                let n = unsafe { libc::read(fd.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
                if n <= 0 {
                    break;
                }
                if let Some(e) = parse_rfkill(&buf[..n as usize]) {
                    radios.apply(e);
                }
            }
            let now = radios.airplane();
            if now != last {
                last = now;
                if let Some(on) = now {
                    airplane(sink, on);
                }
            }
            let mut p = libc::pollfd { fd: fd.as_raw_fd(), events: libc::POLLIN, revents: 0 };
            // SAFETY: one valid pollfd; blocks until rfkill has news.
            if unsafe { libc::poll(&mut p, 1, -1) } < 0
                && std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted
            {
                return;
            }
        }
    }

    fn watch_dnd(sink: &Sink) {
        const SCHEMA: [&str; 2] = ["org.gnome.desktop.notifications", "show-banners"];
        let get = Command::new("gsettings").arg("get").args(SCHEMA).stderr(Stdio::null()).output();
        if let Some(on) =
            get.ok().filter(|o| o.status.success()).and_then(|o| gnome_dnd(&String::from_utf8_lossy(&o.stdout)))
        {
            dnd(sink, on);
            // GNOME: `gsettings monitor` prints each change.
            let Ok(mut child) = Command::new("gsettings")
                .arg("monitor")
                .args(SCHEMA)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
            else {
                return;
            };
            if let Some(out) = child.stdout.take() {
                for line in BufReader::new(out).lines().map_while(Result::ok) {
                    if let Some(on) = gnome_dnd(&line) {
                        dnd(sink, on);
                    }
                }
            }
            let _ = child.wait();
            return;
        }
        // KDE Plasma (and others with the property): notifications inhibited.
        let _ = watch_inhibited(sink);
    }

    fn watch_inhibited(sink: &Sink) -> zbus::Result<()> {
        let conn = zbus::blocking::Connection::session()?;
        let proxy = zbus::blocking::Proxy::new(
            &conn,
            "org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            "org.freedesktop.Notifications",
        )?;
        let on: bool = proxy.get_property("Inhibited")?;
        dnd(sink, on);
        for change in proxy.receive_property_changed::<bool>("Inhibited") {
            if let Ok(on) = change.get() {
                dnd(sink, on);
            }
        }
        Ok(())
    }
}

#[cfg(target_os = "windows")]
mod imp {
    use super::{airplane, dnd, Sink};

    pub fn start(sink: Sink) {
        let s = sink.clone();
        let _ = std::thread::Builder::new().name("islet-airplane".into()).spawn(move || watch_airplane(&s));
        let _ = std::thread::Builder::new().name("islet-focus".into()).spawn(move || {
            if let Err(e) = watch_focus(&sink) {
                super::super::debug_log(&format!("Do Not Disturb unavailable: {}", e.message()));
            }
        });
    }

    /// Windows keeps airplane mode in HKLM\…\RadioManagement\SystemRadioState
    /// (1 = on); the key is watched for changes, so nothing polls.
    fn watch_airplane(sink: &Sink) {
        use windows::core::w;
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Registry::{
            RegCloseKey, RegNotifyChangeKeyValue, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_LOCAL_MACHINE,
            KEY_NOTIFY, KEY_QUERY_VALUE, REG_NOTIFY_CHANGE_LAST_SET,
        };
        use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject, INFINITE};
        // SAFETY: registry and event calls with handles owned here and
        // closed on the way out.
        unsafe {
            let mut key = HKEY::default();
            let path = w!("SYSTEM\\CurrentControlSet\\Control\\RadioManagement\\SystemRadioState");
            if RegOpenKeyExW(HKEY_LOCAL_MACHINE, path, None, KEY_QUERY_VALUE | KEY_NOTIFY, &mut key).is_err() {
                super::super::debug_log("airplane mode unavailable (no radio-management key)");
                return;
            }
            let Ok(event) = CreateEventW(None, false, false, None) else {
                let _ = RegCloseKey(key);
                return;
            };
            let read = || {
                let mut v = 0u32;
                let mut size = std::mem::size_of::<u32>() as u32;
                RegQueryValueExW(key, None, None, None, Some((&raw mut v).cast()), Some(&mut size))
                    .is_ok()
                    .then_some(v == 1)
            };
            let mut last = None;
            loop {
                let now = read();
                if now != last {
                    last = now;
                    if let Some(on) = now {
                        airplane(sink, on);
                    }
                }
                if RegNotifyChangeKeyValue(key, false, REG_NOTIFY_CHANGE_LAST_SET, Some(event), true).is_err() {
                    break;
                }
                WaitForSingleObject(event, INFINITE);
            }
            let _ = CloseHandle(event);
            let _ = RegCloseKey(key);
        }
    }

    /// Windows 11's Focus sessions (which turn on Do Not Disturb).
    fn watch_focus(sink: &Sink) -> windows::core::Result<()> {
        use windows::Foundation::TypedEventHandler;
        use windows::UI::Shell::FocusSessionManager;
        // SAFETY: COM on this thread before the WinRT calls.
        unsafe {
            let _ =
                windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
        }
        if !FocusSessionManager::IsSupported()? {
            return Err(windows::core::Error::new(windows::core::HRESULT(0), "Focus isn't on this version of Windows"));
        }
        let focus = FocusSessionManager::GetDefault()?;
        dnd(sink, focus.IsFocusActive()?);
        let out = sink.clone();
        let _token = focus.IsFocusActiveChanged(&TypedEventHandler::new(
            move |m: windows::core::Ref<FocusSessionManager>, _| {
                if let Some(m) = m.as_ref() {
                    dnd(&out, m.IsFocusActive()?);
                }
                Ok(())
            },
        ))?;
        // Changes arrive on WinRT's threads; keep the manager alive.
        loop {
            std::thread::park();
            let _ = &focus;
        }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::Sink;

    pub fn start(_sink: Sink) {}
}
