//! Providers: background readers of OS state (media sessions, battery
//! levels). Each runs on its own thread, blocks on OS events (never polls),
//! and hands reports to the island through a channel plus the backend's
//! waker. Commands for the media session travel the other way.

pub mod art;
pub mod calendar;
mod clipboard;
pub mod connectivity;
#[cfg(target_os = "linux")]
mod dbus_util;
mod demo;
pub mod downloads;
pub mod hog;
#[cfg(target_os = "linux")]
mod linux_battery;
#[cfg(target_os = "linux")]
pub(crate) mod linux_brightness;
#[cfg(target_os = "linux")]
mod linux_conn;
#[cfg(target_os = "linux")]
mod linux_media;
#[cfg(target_os = "linux")]
pub(crate) mod linux_notifications;
#[cfg(target_os = "linux")]
mod linux_privacy;
#[cfg(all(test, target_os = "linux"))]
mod linux_tests;
#[cfg(target_os = "linux")]
mod linux_volume;
pub mod lyrics;
#[cfg(target_os = "macos")]
mod macos_battery;
#[cfg(target_os = "macos")]
mod macos_conn;
#[cfg(target_os = "macos")]
mod macos_media;
#[cfg(target_os = "macos")]
pub mod macos_mediaremote;
#[cfg(target_os = "macos")]
mod macos_privacy;
#[cfg(target_os = "macos")]
mod macos_volume;
pub mod mic;
pub mod navigation;
mod screenshots;
pub mod spotify;
pub mod sysmon;
pub mod weather;
#[cfg(target_os = "windows")]
mod windows_battery;
#[cfg(target_os = "windows")]
mod windows_brightness;
#[cfg(target_os = "windows")]
mod windows_conn;
#[cfg(target_os = "windows")]
mod windows_media;
#[cfg(target_os = "windows")]
mod windows_notifications;
#[cfg(target_os = "windows")]
mod windows_privacy;
#[cfg(target_os = "windows")]
mod windows_volume;

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

/// A readable app name from a Windows consent-store key: a packaged app's
/// family name ("Microsoft.WindowsCamera_8wekyb3d8bbwe" → "WindowsCamera")
/// or a desktop app's path with `#` for `\` ("C:#Program Files#Zoom#bin#Zoom.exe"
/// → "Zoom").
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub fn consent_app_name(key: &str) -> String {
    if key.contains('#') {
        let file = key.rsplit('#').next().unwrap_or(key);
        let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
        return stem.to_string();
    }
    let family = key.split('_').next().unwrap_or(key);
    family.rsplit('.').next().filter(|s| !s.is_empty()).unwrap_or(family).to_string()
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

/// Download artwork published as web links (an opt-in preference).
pub static WEB_ARTWORK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

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
        Report::Navigation(Some(n)) => format!(
            "navigation to {:?}: {}",
            n.trip.destination,
            n.route.as_ref().map_or_else(|| n.error.clone().unwrap_or_default(), |r| format!("{:.0} s", r.duration))
        ),
        Report::Navigation(None) => "navigation stopped".into(),
        Report::Alarm(Some(a)) => format!("alarm ringing: {:?} at {:02}:{:02}", a.title(), a.hour, a.minute),
        Report::Alarm(None) => "alarm stopped".into(),
        Report::Connectivity(u) => format!("connectivity: {u:?}"),
        Report::Timer(t) => format!("timer: {:?}", t.as_ref().map(|t| t.title())),
        Report::Stopwatch(s) => format!("stopwatch: {}", if s.is_some() { "on" } else { "off" }),
        Report::Hud(h) => format!("hud: {h:?}"),
        Report::Weather(w) => format!("weather: {w:?}"),
        Report::Calendar(c) => format!("calendar: {} events", c.as_ref().map_or(0, |c| c.len())),
        // Never log what a notification says.
        Report::Notification(n) => format!("notification from {:?}", n.app),
        Report::Custom(c) => format!("activity {:?}: {:?}", c.id, c.progress),
        Report::CustomEnded(id) => format!("activity {id:?} ended"),
        Report::RemoteMedia(m) => format!("remote media: {:?}", m.as_ref().map(|m| &m.app_name)),
        Report::System(s) => format!("system: {s:?}"),
        // Never log what was copied.
        Report::Clipboard(t) => format!("clipboard: {} chars", t.chars().count()),
        Report::Privacy(p) => format!("privacy: camera {:?}, mic {:?}", p.camera, p.mic),
        Report::Lyrics(key, l) => {
            format!("lyrics for {key:?}: {}", l.as_ref().map_or(0, |l| l.lines.len()))
        }
        Report::Volume(level, muted) => format!("volume {level:.2} muted={muted}"),
        Report::Brightness(level) => format!("brightness {level:.2}"),
        Report::BatteryHealth(h) => format!("battery health {:?}", h.map(|h| h.percent)),
        Report::Hog(h) => format!("hog {:?}", h.as_ref().map(|h| (&h.name, h.share()))),
        Report::MicMuted(m) => format!("microphone muted={m}"),
        Report::UpNext(u) => format!("up next: {:?}", u.as_ref().map(|(e, _)| &e.summary)),
    }
}

pub struct Providers {
    pub reports: Receiver<(Report, Instant)>,
    media: Option<Sender<MediaCommand>>,
    nav: Sender<navigation::NavMsg>,
    conn: Sender<islet_core::connectivity::ConnCommand>,
    weather: Sender<Option<islet_core::prefs::Place>>,
    calendar: Sender<bool>,
    spotify: Sender<spotify::SpotifyMsg>,
    sysmon: Sender<bool>,
    hog: Sender<bool>,
    lyrics: Sender<Option<islet_core::lyrics::Track>>,
    mic: Sender<islet_core::activity::MicCommand>,
    clipboard: Option<clipboard::Watcher>,
    downloads: Option<downloads::Watcher>,
    screenshots: Option<screenshots::Watcher>,
    sink: Sink,
    #[cfg(target_os = "linux")]
    notifications: linux_notifications::Notifications,
    #[cfg(target_os = "windows")]
    notifications: Option<windows_notifications::Listener>,
}

impl Providers {
    /// Start the providers for this OS (or the scripted demo when
    /// `ISLET_DEMO=media`).
    pub fn start(waker: Waker) -> Self {
        let (tx, reports) = channel();
        let sink = Sink { tx, waker };
        let nav = navigation::start(sink.clone());
        let conn = connectivity::start(sink.clone());
        let weather = weather::start(sink.clone());
        let calendar = calendar::start(sink.clone());
        let spotify = spotify::start(sink.clone());
        let sysmon = sysmon::start(sink.clone());
        let hog = hog::start(sink.clone());
        let lyrics = lyrics::start(sink.clone());
        let mic = mic::start(sink.clone());
        let media = if std::env::var("ISLET_DEMO").as_deref() == Ok("media") {
            Some(demo::start(sink.clone()))
        } else {
            platform_start(sink.clone())
        };
        Self {
            reports,
            media,
            nav,
            conn,
            weather,
            calendar,
            spotify,
            sysmon,
            hog,
            lyrics,
            mic,
            clipboard: None,
            downloads: None,
            screenshots: None,
            sink,
            #[cfg(target_os = "linux")]
            notifications: linux_notifications::Notifications::default(),
            #[cfg(target_os = "windows")]
            notifications: None,
        }
    }

    /// Mirror desktop notifications (Linux), or stop and close the monitor.
    pub fn notifications(&mut self, on: bool) {
        #[cfg(target_os = "linux")]
        self.notifications.set(on, &self.sink, &Bus::Session);
        #[cfg(target_os = "windows")]
        if !on {
            self.notifications = None;
        } else if self.notifications.is_none() {
            match windows_notifications::start(self.sink.clone()) {
                Ok(l) => self.notifications = Some(l),
                Err(e) => debug_log(&format!("notifications: {e}")),
            }
        }
        #[cfg(target_os = "macos")]
        let _ = (on, &self.sink);
    }

    /// Turn calendar on (and refresh now) or off.
    pub fn calendar(&self, on: bool) {
        let _ = self.calendar.send(on);
    }

    /// Follow this place's weather (None turns weather off).
    pub fn weather(&self, place: Option<islet_core::prefs::Place>) {
        let _ = self.weather.send(place);
    }

    /// A Wi-Fi / Bluetooth request (the worker only runs when asked).
    pub fn conn(&self, cmd: islet_core::connectivity::ConnCommand) {
        let _ = self.conn.send(cmd);
    }

    pub fn nav(&self, msg: navigation::NavMsg) {
        let _ = self.nav.send(msg);
    }

    /// Start or stop watching the clipboard (history is opt-in).
    pub fn clipboard(&mut self, on: bool) {
        if on && self.clipboard.is_none() {
            self.clipboard = clipboard::start(self.sink.clone());
        } else if !on {
            self.clipboard = None;
        }
    }

    /// Start or stop watching the Downloads folder (opt-in).
    pub fn downloads(&mut self, on: bool) {
        if on && self.downloads.is_none() {
            self.downloads = downloads::start(self.sink.clone());
        } else if !on && self.downloads.take().is_some() {
            self.sink.send(Report::CustomEnded(islet_core::downloads::ACTIVITY_ID.into()));
        }
    }

    /// Start or stop showing new screenshots (Islet Pro).
    pub fn screenshots(&mut self, on: bool) {
        if on && self.screenshots.is_none() {
            self.screenshots = screenshots::start(self.sink.clone());
        } else if !on && self.screenshots.take().is_some() {
            self.sink.send(Report::CustomEnded(islet_core::screenshot::ACTIVITY_ID.into()));
        }
    }

    /// Put a history entry back on the clipboard.
    pub fn clipboard_copy(&self, text: String) {
        if let Some(w) = &self.clipboard {
            w.copy(text);
        }
    }

    /// Start or stop the system monitor (it runs only while its panel is open).
    /// A way for one-off readers (the Controls panel's brightness) to report.
    pub fn sink(&self) -> Sink {
        self.sink.clone()
    }

    pub fn sysmon(&self, on: bool) {
        let _ = self.sysmon.send(on);
    }

    /// Watch for a program hogging the processor or memory (the opt-in
    /// performance alert), or stop.
    pub fn hog(&self, on: bool) {
        let _ = self.hog.send(on);
    }

    /// Read or set the microphone's mute switch (the call card).
    pub fn mic(&self, cmd: islet_core::activity::MicCommand) {
        let _ = self.mic.send(cmd);
    }

    /// Set the output volume (0..=1), from the Now Playing card's slider.
    pub fn set_volume(&self, level: f32) {
        let level = level.clamp(0.0, 1.0);
        #[cfg(target_os = "linux")]
        linux_volume::set(level);
        #[cfg(target_os = "windows")]
        windows_volume::set(level);
        #[cfg(target_os = "macos")]
        macos_volume::set(level);
    }

    /// Look up lyrics for the song that is playing (None: stop).
    pub fn lyrics(&self, track: Option<islet_core::lyrics::Track>) {
        let _ = self.lyrics.send(track);
    }

    pub fn spotify(&self, on: bool) {
        let _ = self.spotify.send(spotify::SpotifyMsg::Enable(on));
    }

    /// A playback command for the session the island shows.
    pub fn command_for(&self, source: &str, cmd: MediaCommand) {
        if source == spotify::SOURCE {
            let _ = self.spotify.send(spotify::SpotifyMsg::Command(cmd));
        } else {
            self.command(cmd);
        }
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
    linux_volume::start(sink.clone());
    linux_brightness::start(sink.clone(), Bus::Session);
    linux_privacy::start(sink.clone());
    linux_media::start(sink, Bus::Session)
}

#[cfg(target_os = "windows")]
fn platform_start(sink: Sink) -> Option<Sender<MediaCommand>> {
    windows_battery::start(sink.clone());
    windows_volume::start(sink.clone());
    windows_brightness::start(sink.clone());
    windows_privacy::start(sink.clone());
    windows_media::start(sink)
}

#[cfg(target_os = "macos")]
fn platform_start(sink: Sink) -> Option<Sender<MediaCommand>> {
    macos_battery::start(sink.clone());
    macos_volume::start(sink.clone());
    macos_privacy::start(sink.clone());
    // Every app's Now Playing via the MediaRemote helper; falls back to the
    // Spotify / Apple Music notifications if the helper can't run.
    macos_mediaremote::start(sink)
}

#[cfg(test)]
mod name_tests {
    #[test]
    fn consent_store_names() {
        use super::consent_app_name as n;
        assert_eq!(n("Microsoft.WindowsCamera_8wekyb3d8bbwe"), "WindowsCamera");
        assert_eq!(n("C:#Program Files#Zoom#bin#Zoom.exe"), "Zoom");
        assert_eq!(n("C:#Users#me#AppData#Local#Discord#app-1.0#Discord.exe"), "Discord");
        assert_eq!(n("SomeApp"), "SomeApp");
    }
}
