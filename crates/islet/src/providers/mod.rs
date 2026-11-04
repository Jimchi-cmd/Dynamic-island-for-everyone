//! Providers: background readers of OS state (media sessions, battery
//! levels). Each runs on its own thread, blocks on OS events (never polls),
//! and hands reports to the island through a channel plus the backend's
//! waker. Commands for the media session travel the other way.

pub mod art;
#[cfg(target_os = "linux")]
mod dbus_util;
mod demo;
#[cfg(target_os = "linux")]
mod linux_battery;
#[cfg(target_os = "linux")]
mod linux_media;
#[cfg(all(test, target_os = "linux"))]
mod linux_tests;
#[cfg(target_os = "macos")]
mod macos_battery;
#[cfg(target_os = "macos")]
mod macos_media;
#[cfg(target_os = "macos")]
pub mod macos_mediaremote;
#[cfg(target_os = "windows")]
mod windows_battery;
#[cfg(target_os = "windows")]
mod windows_media;

use crate::platform::Waker;
use islet_core::activity::{MediaCommand, Report};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Instant;

/// Best-effort device kind from a product name (Windows and macOS report
/// names, not device classes, for most Bluetooth peripherals).
#[cfg_attr(target_os = "linux", allow(dead_code))]
pub fn kind_from_name(name: &str) -> islet_core::activity::DeviceKind {
    use islet_core::activity::DeviceKind;
    let n = name.to_ascii_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| n.contains(w));
    if has(&["mouse", "mx master", "mx anywhere"]) {
        DeviceKind::Mouse
    } else if has(&["keyboard", "keys"]) {
        DeviceKind::Keyboard
    } else if has(&["controller", "gamepad", "xbox", "dualsense", "dualshock", "joy-con"]) {
        DeviceKind::Gamepad
    } else if has(&["trackpad", "touchpad", "pencil", "pen"]) {
        DeviceKind::Other
    } else if has(&["buds", "airpods", "earbuds", "tws", "earphone", "pods"]) {
        DeviceKind::Earbuds
    } else if has(&["speaker", "soundbar", "boom", "flip", "charge"]) {
        DeviceKind::Speaker
    } else if has(&["phone", "iphone", "galaxy s", "pixel"]) {
        DeviceKind::Phone
    } else {
        DeviceKind::Headphones
    }
}

/// With `ISLET_DEBUG=1`, provider diagnostics go to stderr and to
/// `islet-debug.log` in the temp directory (GUI builds have no console).
pub fn debug_log(msg: &str) {
    use std::io::Write;
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if !*ON.get_or_init(|| std::env::var_os("ISLET_DEBUG").is_some_and(|v| v != "0")) {
        return;
    }
    eprintln!("islet: {msg}");
    let path = std::env::temp_dir().join("islet-debug.log");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let _ = writeln!(f, "{secs} {msg}");
    }
}

/// Where providers send reports. Cloneable across threads.
#[derive(Clone)]
pub struct Sink {
    tx: Sender<(Report, Instant)>,
    waker: Waker,
}

impl Sink {
    /// Send a report, stamped now (media positions are relative to this).
    pub fn send(&self, report: Report) {
        debug_log(&format!("report: {}", describe(&report)));
        if self.tx.send((report, Instant::now())).is_ok() {
            self.waker.wake();
        }
    }
}

fn describe(r: &Report) -> String {
    match r {
        Report::Media(Some(m)) => {
            format!("media {:?} by {:?} ({}) playing={}", m.title, m.artist, m.app_name, m.playing)
        }
        Report::Media(None) => "media none".into(),
        Report::DeviceConnected(d) => format!("connected {:?} {:?} {}%", d.name, d.kind, d.percent),
        Report::DeviceChanged(d) => format!("changed {:?} {}%", d.name, d.percent),
        Report::DeviceGone(id) => format!("gone {id}"),
        Report::Laptop(d) => format!("laptop {}% charging={}", d.percent, d.charging),
    }
}

pub struct Providers {
    pub reports: Receiver<(Report, Instant)>,
    media: Option<Sender<MediaCommand>>,
}

impl Providers {
    /// Start the providers for this OS (or the scripted demo when
    /// `ISLET_DEMO=media`).
    pub fn start(waker: Waker) -> Self {
        let (tx, reports) = channel();
        let sink = Sink { tx, waker };
        if std::env::var("ISLET_DEMO").as_deref() == Ok("media") {
            return Self { reports, media: Some(demo::start(sink)) };
        }
        let media = platform_start(sink);
        Self { reports, media }
    }

    pub fn command(&self, cmd: MediaCommand) {
        if let Some(tx) = &self.media {
            let _ = tx.send(cmd);
        }
    }
}

/// Which D-Bus bus a Linux provider talks to (tests use a private bus).
#[cfg(target_os = "linux")]
#[derive(Debug, Clone)]
pub enum Bus {
    Session,
    System,
    #[cfg_attr(not(test), allow(dead_code))]
    Address(String),
}

#[cfg(target_os = "linux")]
impl Bus {
    fn address(&self) -> String {
        match self {
            Bus::Session => std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap_or_default(),
            Bus::System => std::env::var("DBUS_SYSTEM_BUS_ADDRESS")
                .unwrap_or_else(|_| "unix:path=/var/run/dbus/system_bus_socket".into()),
            Bus::Address(a) => a.clone(),
        }
    }

    /// Connect to the bus. Unix-socket addresses are connected here, with
    /// the standard library, and handed to zbus ready-made: zbus would
    /// otherwise connect through a helper thread pool whose last thread
    /// wakes every 500 ms forever, breaking "zero wakeups when idle".
    pub fn connect(&self) -> zbus::Result<zbus::blocking::Connection> {
        let address = self.address();
        let mut last_err = None;
        for entry in address.split(';').filter(|e| !e.is_empty()) {
            match unix_socket(entry) {
                Some(Ok(stream)) => return zbus::blocking::connection::Builder::async_io_unix_stream(stream).build(),
                Some(Err(e)) => last_err = Some(zbus::Error::InputOutput(std::sync::Arc::new(e))),
                // Not a unix socket (tcp, launchd…): let zbus handle it.
                None => return zbus::blocking::connection::Builder::address(entry)?.build(),
            }
        }
        Err(last_err.unwrap_or(zbus::Error::Address(format!("no usable D-Bus address in {address:?}"))))
    }
}

/// Connect a `unix:path=…` or `unix:abstract=…` D-Bus address entry.
/// Returns None for other transports.
#[cfg(target_os = "linux")]
fn unix_socket(entry: &str) -> Option<std::io::Result<std::os::unix::net::UnixStream>> {
    use std::os::linux::net::SocketAddrExt;
    use std::os::unix::net::{SocketAddr, UnixStream};
    let params = entry.strip_prefix("unix:")?;
    let value = |key: &str| {
        params.split(',').find_map(|kv| kv.strip_prefix(key).and_then(|v| v.strip_prefix('='))).map(unescape)
    };
    if let Some(path) = value("path") {
        return Some(UnixStream::connect(path));
    }
    let name = value("abstract")?;
    Some(SocketAddr::from_abstract_name(name.as_bytes()).and_then(|addr| UnixStream::connect_addr(&addr)))
}

/// D-Bus addresses percent-escape bytes outside a safe set.
#[cfg(target_os = "linux")]
fn unescape(v: &str) -> String {
    let b = v.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(byte) = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok())
            {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(all(test, target_os = "linux"))]
mod bus_tests {
    use super::*;

    #[test]
    fn parses_unix_addresses() {
        assert!(unix_socket("tcp:host=localhost,port=1").is_none());
        assert_eq!(unescape("/tmp/a%20b"), "/tmp/a b");
        // A path that doesn't exist yields an error, not a panic.
        assert!(unix_socket("unix:path=/nonexistent/islet-test,guid=00").unwrap().is_err());
    }
}

#[cfg(target_os = "linux")]
fn platform_start(sink: Sink) -> Option<Sender<MediaCommand>> {
    linux_battery::start(sink.clone(), Bus::System);
    linux_media::start(sink, Bus::Session)
}

#[cfg(target_os = "windows")]
fn platform_start(sink: Sink) -> Option<Sender<MediaCommand>> {
    windows_battery::start(sink.clone());
    windows_media::start(sink)
}

#[cfg(target_os = "macos")]
fn platform_start(sink: Sink) -> Option<Sender<MediaCommand>> {
    macos_battery::start(sink.clone());
    // Every app's Now Playing via the MediaRemote helper; falls back to the
    // Spotify / Apple Music notifications if the helper can't run.
    macos_mediaremote::start(sink)
}
