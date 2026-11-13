//! Downloads in progress (opt-in): watches the Downloads folder for the
//! files browsers write while downloading and shows the newest as a live
//! activity (name, size, speed), then "Downloaded" for a few seconds, which
//! opens the file when tapped. The thread sleeps until the folder changes
//! (inotify, kqueue, or a change notification on Windows); only while a
//! download runs does it look at sizes, once a second. Nothing leaves the
//! computer.

use super::Sink;
use islet_core::activity::{Custom, Report};
use islet_core::downloads::{self, ACTIVITY_ID};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

const BLUE: [u8; 3] = [10, 132, 255];
const GREEN: [u8; 3] = [48, 209, 88];
/// "Downloaded" shows this long.
const DONE_FOR: Duration = Duration::from_secs(6);
/// How often sizes are read while something downloads.
const EVERY: Duration = Duration::from_secs(1);

/// Stops the watcher when dropped (turned off in Settings).
pub struct Watcher {
    on: Arc<AtomicBool>,
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.on.store(false, Ordering::Relaxed);
    }
}

pub fn start(sink: Sink) -> Option<Watcher> {
    let dir = folder()?;
    let on = Arc::new(AtomicBool::new(true));
    let flag = on.clone();
    std::thread::Builder::new().name("islet-downloads".into()).spawn(move || run(&dir, &flag, &sink)).ok()?;
    Some(Watcher { on })
}

/// The user's Downloads folder.
fn folder() -> Option<PathBuf> {
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    #[cfg(target_os = "linux")]
    let dir = {
        let home = env("HOME")?;
        let config = env("XDG_CONFIG_HOME").unwrap_or_else(|| format!("{home}/.config"));
        std::fs::read_to_string(format!("{config}/user-dirs.dirs"))
            .ok()
            .and_then(|s| downloads::xdg_download_dir(&s, &home))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(&home).join("Downloads"))
    };
    #[cfg(target_os = "windows")]
    let dir = PathBuf::from(env("USERPROFILE")?).join("Downloads");
    #[cfg(target_os = "macos")]
    let dir = PathBuf::from(env("HOME")?).join("Downloads");
    dir.is_dir().then_some(dir)
}

/// A download seen in the folder.
struct Seen {
    finished_name: Option<String>,
    size: u64,
    at: Instant,
    per_sec: Option<f64>,
}

/// Size of a file, or of the files in a Safari `.download` bundle.
fn size_of(path: &Path) -> u64 {
    match std::fs::metadata(path) {
        Ok(m) if m.is_dir() => std::fs::read_dir(path)
            .map(|d| d.flatten().filter_map(|e| e.metadata().ok()).filter(|m| m.is_file()).map(|m| m.len()).sum())
            .unwrap_or(0),
        Ok(m) => m.len(),
        Err(_) => 0,
    }
}

/// Downloads in progress: (file name, finished name, size, last change).
fn scan(dir: &Path) -> Vec<(String, Option<String>, u64, SystemTime)> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let finished = downloads::partial(&name)?;
            let modified = e.metadata().and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
            Some((name, finished, size_of(&e.path()), modified))
        })
        .collect()
}

fn run(dir: &Path, on: &AtomicBool, sink: &Sink) {
    let Some(mut change) = Change::new(dir) else {
        super::debug_log("downloads: can't watch the Downloads folder");
        return;
    };
    let mut seen: HashMap<String, Seen> = HashMap::new();
    let mut shown = false;
    let mut done_until: Option<Instant> = None;
    let send = |r: Report| {
        if on.load(Ordering::Relaxed) {
            sink.send(r);
        }
    };
    while on.load(Ordering::Relaxed) {
        let now = Instant::now();
        let current = scan(dir);
        // Downloads that are gone: finished if the file they became is there.
        let mut finished: Option<(String, PathBuf)> = None;
        for (name, s) in &seen {
            if current.iter().any(|(n, ..)| n == name) {
                continue;
            }
            if let Some(f) = &s.finished_name {
                let path = dir.join(f);
                if size_of(&path) > 0 {
                    finished = Some((f.clone(), path));
                }
            }
        }
        seen.retain(|name, _| current.iter().any(|(n, ..)| n == name));
        for (name, finished_name, size, _) in &current {
            let s = seen.entry(name.clone()).or_insert(Seen {
                finished_name: finished_name.clone(),
                size: *size,
                at: now,
                per_sec: None,
            });
            let dt = now.duration_since(s.at).as_secs_f64();
            if dt >= 0.5 {
                let rate = size.saturating_sub(s.size) as f64 / dt;
                // Smooth it, so the speed doesn't jump around every second.
                s.per_sec = Some(s.per_sec.map_or(rate, |r| r * 0.6 + rate * 0.4));
                s.size = *size;
                s.at = now;
            }
            s.finished_name = finished_name.clone();
        }
        if let Some((name, ..)) = current.iter().max_by_key(|(_, _, _, modified)| *modified) {
            let s = &seen[name];
            let mut line = downloads::progress_line(s.size, s.per_sec);
            if current.len() > 1 {
                line.push_str(&format!(" · {} downloads", current.len()));
            }
            send(Report::Custom(Custom {
                id: ACTIVITY_ID.into(),
                title: s.finished_name.clone().unwrap_or_else(|| "Downloading".into()),
                subtitle: line,
                progress: None,
                color: Some(BLUE),
                open: None,
                image: None,
            }));
            shown = true;
            done_until = None;
        } else if let Some((name, path)) = finished {
            send(Report::Custom(Custom {
                id: ACTIVITY_ID.into(),
                title: name,
                subtitle: format!("Downloaded · {} · tap to open", downloads::format_size(size_of(&path))),
                progress: Some(1.0),
                color: Some(GREEN),
                open: Some(path.to_string_lossy().into_owned()),
                image: None,
            }));
            shown = true;
            done_until = Some(now + DONE_FOR);
        } else if shown && done_until.is_none() {
            // Cancelled.
            send(Report::CustomEnded(ACTIVITY_ID.into()));
            shown = false;
        }
        if done_until.is_some_and(|d| now >= d) {
            send(Report::CustomEnded(ACTIVITY_ID.into()));
            shown = false;
            done_until = None;
        }
        let timeout =
            if !current.is_empty() { Some(EVERY) } else { done_until.map(|d| d.saturating_duration_since(now)) };
        change.wait(timeout);
    }
}

// ---- Folder change notifications ---------------------------------------------------

#[cfg(target_os = "linux")]
pub(super) struct Change {
    fd: std::os::fd::OwnedFd,
}

#[cfg(target_os = "linux")]
impl Change {
    pub(super) fn new(dir: &Path) -> Option<Self> {
        use std::os::fd::FromRawFd;
        use std::os::unix::ffi::OsStrExt;
        let path = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
        // SAFETY: a new inotify descriptor (checked) that we own; a valid path.
        unsafe {
            let fd = libc::inotify_init1(libc::IN_CLOEXEC | libc::IN_NONBLOCK);
            if fd < 0 {
                return None;
            }
            let fd = std::os::fd::OwnedFd::from_raw_fd(fd);
            use std::os::fd::AsRawFd;
            let mask = libc::IN_CREATE | libc::IN_DELETE | libc::IN_MOVED_FROM | libc::IN_MOVED_TO;
            (libc::inotify_add_watch(fd.as_raw_fd(), path.as_ptr(), mask) >= 0).then_some(Change { fd })
        }
    }

    pub(super) fn wait(&mut self, timeout: Option<Duration>) {
        use std::os::fd::AsRawFd;
        let ms = timeout.map_or(-1, |t| t.as_millis().min(i32::MAX as u128) as libc::c_int);
        let mut fds = [libc::pollfd { fd: self.fd.as_raw_fd(), events: libc::POLLIN, revents: 0 }];
        let mut buf = [0u8; 4096];
        // SAFETY: a valid pollfd array and buffer for the calls' duration.
        unsafe {
            libc::poll(fds.as_mut_ptr(), 1, ms);
            while libc::read(self.fd.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) > 0 {}
        }
    }
}

#[cfg(target_os = "macos")]
pub(super) struct Change {
    kq: std::os::fd::OwnedFd,
    _dir: std::os::fd::OwnedFd,
}

#[cfg(target_os = "macos")]
impl Change {
    pub(super) fn new(dir: &Path) -> Option<Self> {
        use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
        use std::os::unix::ffi::OsStrExt;
        let path = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
        // SAFETY: descriptors are checked and owned; the kevent struct is
        // fully initialised before it's registered.
        unsafe {
            let dfd = libc::open(path.as_ptr(), libc::O_EVTONLY);
            if dfd < 0 {
                return None;
            }
            let dir_fd = OwnedFd::from_raw_fd(dfd);
            let kq = libc::kqueue();
            if kq < 0 {
                return None;
            }
            let kq = OwnedFd::from_raw_fd(kq);
            let mut ev: libc::kevent = std::mem::zeroed();
            ev.ident = dir_fd.as_raw_fd() as usize;
            ev.filter = libc::EVFILT_VNODE;
            ev.flags = libc::EV_ADD | libc::EV_CLEAR;
            ev.fflags = libc::NOTE_WRITE;
            if libc::kevent(kq.as_raw_fd(), &ev, 1, std::ptr::null_mut(), 0, std::ptr::null()) < 0 {
                return None;
            }
            Some(Change { kq, _dir: dir_fd })
        }
    }

    pub(super) fn wait(&mut self, timeout: Option<Duration>) {
        use std::os::fd::AsRawFd;
        let ts =
            timeout.map(|t| libc::timespec { tv_sec: t.as_secs() as libc::time_t, tv_nsec: t.subsec_nanos() as _ });
        // SAFETY: an out-buffer for one event and an optional timespec.
        unsafe {
            let mut ev: libc::kevent = std::mem::zeroed();
            let tsp = ts.as_ref().map_or(std::ptr::null(), |t| t as *const libc::timespec);
            libc::kevent(self.kq.as_raw_fd(), std::ptr::null(), 0, &mut ev, 1, tsp);
        }
    }
}

#[cfg(target_os = "windows")]
pub(super) struct Change {
    handle: windows::Win32::Foundation::HANDLE,
}

#[cfg(target_os = "windows")]
impl Change {
    pub(super) fn new(dir: &Path) -> Option<Self> {
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::{
            FindFirstChangeNotificationW, FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME,
        };
        let wide: Vec<u16> = dir.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
        // SAFETY: a NUL-terminated path; the handle is closed on drop.
        let h = unsafe {
            FindFirstChangeNotificationW(
                PCWSTR(wide.as_ptr()),
                false,
                FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_DIR_NAME,
            )
        }
        .ok()?;
        Some(Change { handle: h })
    }

    pub(super) fn wait(&mut self, timeout: Option<Duration>) {
        use windows::Win32::Storage::FileSystem::FindNextChangeNotification;
        use windows::Win32::System::Threading::{WaitForSingleObject, INFINITE};
        let ms = timeout.map_or(INFINITE, |t| t.as_millis().min(u128::from(INFINITE - 1)) as u32);
        // SAFETY: our change-notification handle, alive until drop.
        unsafe {
            WaitForSingleObject(self.handle, ms);
            let _ = FindNextChangeNotification(self.handle);
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for Change {
    fn drop(&mut self) {
        use windows::Win32::Storage::FileSystem::FindCloseChangeNotification;
        // SAFETY: closing our own handle once.
        unsafe {
            let _ = FindCloseChangeNotification(self.handle);
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    #[test]
    fn a_browser_download_shows_progress_then_done() {
        let dir = std::env::temp_dir().join(format!("islet-dl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (tx, rx) = channel();
        let sink = Sink { tx, waker: crate::platform::Waker::new(|| {}) };
        let on = Arc::new(AtomicBool::new(true));
        let (d, flag) = (dir.clone(), on.clone());
        std::thread::spawn(move || run(&d, &flag, &sink));
        let next = |want: &dyn Fn(&Report) -> bool| -> Report {
            let until = Instant::now() + Duration::from_secs(5);
            loop {
                let left = until.saturating_duration_since(Instant::now());
                let (r, _) = rx.recv_timeout(left).expect("a report in time");
                if want(&r) {
                    return r;
                }
            }
        };
        std::thread::sleep(Duration::from_millis(200));
        // The browser starts writing "report.pdf.crdownload".
        std::fs::write(dir.join("report.pdf.crdownload"), vec![0u8; 300_000]).unwrap();
        let r = next(&|r| matches!(r, Report::Custom(c) if c.title == "report.pdf"));
        let Report::Custom(c) = r else { unreachable!() };
        assert!(c.progress.is_none() && c.subtitle.contains("KB"), "{c:?}");
        // It finishes: the partial file becomes report.pdf.
        std::fs::rename(dir.join("report.pdf.crdownload"), dir.join("report.pdf")).unwrap();
        let r = next(&|r| matches!(r, Report::Custom(c) if c.progress == Some(1.0)));
        let Report::Custom(c) = r else { unreachable!() };
        assert!(c.subtitle.starts_with("Downloaded") && c.open.as_deref().is_some_and(|p| p.ends_with("report.pdf")));
        on.store(false, Ordering::Relaxed);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
