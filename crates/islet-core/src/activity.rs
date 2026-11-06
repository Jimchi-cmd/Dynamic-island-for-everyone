//! Live activities: what the island is showing and why.
//!
//! Providers (OS media sessions, battery services) report facts; this module
//! decides which activities are live and in what order. Pure logic: times are
//! app-clock seconds supplied by the caller.

use std::collections::HashMap;
use std::sync::Arc;

/// Decoded artwork: premultiplied RGBA, row-major, `width * height * 4` bytes.
#[derive(Clone, PartialEq, Eq)]
pub struct ArtImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<Vec<u8>>,
    /// Average colour of the saturated pixels, for tinting (High tier).
    pub dominant: [u8; 3],
}

impl std::fmt::Debug for ArtImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ArtImage({}x{}, dominant {:?})", self.width, self.height, self.dominant)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MediaCaps {
    pub play_pause: bool,
    pub next: bool,
    pub previous: bool,
    pub seek: bool,
}

/// What is playing, from the OS media session.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MediaInfo {
    /// Stable identifier of the source (D-Bus name, app id, bundle id).
    pub source: String,
    /// Human name of the source app ("Spotify").
    pub app_name: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub playing: bool,
    /// Seconds into the track at `position_at` (app clock).
    pub position: Option<f64>,
    pub position_at: f64,
    pub duration: Option<f64>,
    /// Playback rate (1.0 normally).
    pub rate: f64,
    pub art: Option<ArtImage>,
    pub caps: MediaCaps,
    /// The song's file on this computer, when the player says (Linux
    /// MPRIS): lyrics may sit beside it as an .lrc file.
    pub file: Option<String>,
}

impl MediaInfo {
    /// Position now, extrapolated while playing and clamped to the track.
    pub fn position_now(&self, t: f64) -> Option<f64> {
        let p = self.position?;
        let p = if self.playing { p + (t - self.position_at).max(0.0) * self.rate.max(0.0) } else { p };
        Some(match self.duration {
            Some(d) if d > 0.0 => p.clamp(0.0, d),
            _ => p.max(0.0),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeviceKind {
    Headphones,
    Earbuds,
    Speaker,
    Mouse,
    Keyboard,
    Gamepad,
    Phone,
    Tablet,
    Laptop,
    Other,
}

/// A component of a multi-part device (true-wireless earbuds).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Part {
    Left,
    Right,
    Case,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BatteryDevice {
    pub id: String,
    pub name: String,
    pub kind: DeviceKind,
    /// 0..=100
    pub percent: u8,
    pub charging: bool,
    pub parts: Vec<(Part, u8)>,
}

/// An active trip and its latest route summary (or why there isn't one).
#[derive(Debug, Clone, PartialEq)]
pub struct NavInfo {
    pub trip: crate::nav::Trip,
    pub route: Option<crate::nav::RouteInfo>,
    pub error: Option<String>,
}

/// A level change the island shows briefly (like the iPhone's volume HUD).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Hud {
    /// Output volume 0..=1, and whether it is muted.
    Volume {
        level: f32,
        muted: bool,
    },
    /// Screen brightness 0..=1.
    Brightness(f32),
    CapsLock(bool),
}

/// A live activity started by a script or another app (`islet activity`):
/// a build, a download, a delivery…
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Custom {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    /// 0..=1, or None for "in progress" without a known amount.
    #[serde(default)]
    pub progress: Option<f32>,
    /// Accent colour (RGB), default blue.
    #[serde(default)]
    pub color: Option<[u8; 3]>,
}

/// At most this many script activities at once.
pub const MAX_CUSTOM: usize = 8;

/// A desktop notification mirrored on the island.
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub app: String,
    pub summary: String,
    pub body: String,
}

/// How long a new notification shows, and how many are kept.
pub const NOTE_SECS: f64 = 5.0;
pub const NOTE_HISTORY: usize = 20;

/// How long a HUD stays after the last change.
pub const HUD_SECS: f64 = 1.6;

/// Commands the island sends to the current media session.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MediaCommand {
    PlayPause,
    Next,
    Previous,
    /// Seek to this many seconds into the track.
    SeekTo(f64),
}

/// Facts reported by providers.
#[derive(Debug, Clone, PartialEq)]
pub enum Report {
    /// The current media session, or none.
    Media(Option<MediaInfo>),
    /// A device that reports a battery level appeared (connected).
    DeviceConnected(BatteryDevice),
    /// Its level or charging state changed.
    DeviceChanged(BatteryDevice),
    DeviceGone(String),
    /// The laptop's own battery: charger plugged/unplugged or level changed.
    Laptop(BatteryDevice),
    /// The active trip (None when navigation stops).
    Navigation(Option<NavInfo>),
    /// An alarm started ringing (None when it stops or snoozes).
    Alarm(Option<crate::alarm::Alarm>),
    /// Wi-Fi / Bluetooth lists and connection progress.
    Connectivity(crate::connectivity::ConnUpdate),
    /// The running (or paused) timer, or None.
    Timer(Option<crate::timer::Timer>),
    /// Volume, brightness or caps lock changed.
    Hud(Hud),
    /// Current weather (None when weather is turned off).
    Weather(Option<crate::weather::Weather>),
    /// Upcoming calendar events (None when calendar is turned off).
    Calendar(Option<Vec<crate::calendar::CalEvent>>),
    /// The event to show as "up next", and when it starts on the app clock.
    UpNext(Option<(crate::calendar::CalEvent, f64)>),
    /// A desktop notification arrived.
    Notification(Note),
    /// Playback on another device (Spotify Connect), or None.
    RemoteMedia(Option<MediaInfo>),
    /// A script's live activity started or changed…
    Custom(Custom),
    /// …or ended.
    CustomEnded(String),
    /// Which apps are using the camera and the microphone right now.
    Privacy(Privacy),
    /// A system monitor reading (None when the monitor stops).
    System(Option<crate::sysmon::Stats>),
    /// Text was copied (clipboard history, when it's on).
    Clipboard(String),
    /// Synced lyrics for a song (`Track::key`), or None when it has none.
    Lyrics(String, Option<std::sync::Arc<crate::lyrics::Lyrics>>),
    /// The output volume (0..=1) and mute, quietly (no pop-up): sent when
    /// the volume watcher starts, for the Now Playing card's slider.
    Volume(f32, bool),
}

/// Whether the camera and the microphone are in use, and by which apps
/// when the OS says (macOS doesn't: its lists stay empty). The island shows
/// a green dot for the camera and an orange one for the microphone.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Privacy {
    pub camera: Option<Vec<String>>,
    pub mic: Option<Vec<String>>,
}

impl Privacy {
    pub fn active(&self) -> bool {
        self.camera.is_some() || self.mic.is_some()
    }

    /// What a screen reader says, e.g. "Camera in use by Zoom".
    pub fn describe(&self) -> Option<String> {
        let part = |what: &str, apps: &Option<Vec<String>>| {
            apps.as_ref().map(|v| {
                if v.is_empty() {
                    format!("{what} in use")
                } else {
                    format!("{what} in use by {}", v.join(", "))
                }
            })
        };
        let parts: Vec<String> =
            [part("Camera", &self.camera), part("Microphone", &self.mic)].into_iter().flatten().collect();
        (!parts.is_empty()).then(|| parts.join("; "))
    }
}

/// Why a battery peek is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeekReason {
    Connected,
    Low,
    ChargerChanged,
    /// The battery reached 100 % while plugged in.
    Full,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Live {
    /// A ringing alarm: always first.
    Alarm(crate::alarm::Alarm),
    /// A volume / brightness / caps-lock change, for a moment.
    Hud(Hud),
    /// A notification that just arrived, for a few seconds.
    Note(Note),
    Battery {
        device: BatteryDevice,
        reason: PeekReason,
    },
    Timer(crate::timer::Timer),
    /// A script's live activity.
    Custom(Custom),
    /// A calendar event starting soon (`starts_at` on the app clock).
    Event {
        event: crate::calendar::CalEvent,
        starts_at: f64,
    },
    Navigation(NavInfo),
    Media(MediaInfo),
}

/// Seconds a paused session stays on the island before it goes away.
pub const MEDIA_LINGER: f64 = 15.0;
pub const PEEK_CONNECTED: f64 = 3.5;
pub const PEEK_LOW: f64 = 5.0;
pub const PEEK_CHARGER: f64 = 3.0;
pub const PEEK_FULL: f64 = 3.5;
/// At most one connect peek per device in this many seconds, so a flaky
/// Bluetooth link can't make the island bounce.
pub const PEEK_COOLDOWN: f64 = 30.0;
/// Low-battery alerts fire when a device drops to or below these levels.
pub const LOW_LEVELS: [u8; 2] = [20, 10];

#[derive(Debug, Default)]
pub struct Activities {
    alarm: Option<crate::alarm::Alarm>,
    conn: crate::connectivity::Connectivity,
    timer: Option<crate::timer::Timer>,
    hud: Option<(Hud, f64)>,
    weather: Option<crate::weather::Weather>,
    calendar: Vec<crate::calendar::CalEvent>,
    up_next: Option<(crate::calendar::CalEvent, f64)>,
    notes: std::collections::VecDeque<Note>,
    remote: Option<MediaInfo>,
    custom: Vec<Custom>,
    privacy: Privacy,
    system: Option<crate::sysmon::Stats>,
    note_until: f64,
    nav: Option<NavInfo>,
    media: Option<MediaInfo>,
    paused_since: Option<f64>,
    devices: HashMap<String, BatteryDevice>,
    laptop: Option<BatteryDevice>,
    peek: Option<(BatteryDevice, PeekReason, f64)>,
    last_peek: HashMap<String, f64>,
}

impl Activities {
    pub fn new() -> Self {
        Self::default()
    }

    /// The session the island shows: this computer's when it's playing (or
    /// just paused and nothing plays elsewhere), else another device's.
    pub fn media(&self) -> Option<&MediaInfo> {
        let remote = self.remote.as_ref().filter(|r| r.playing);
        match (&self.media, remote) {
            (Some(local), _) if local.playing => Some(local),
            (_, Some(r)) => Some(r),
            (local, None) => local.as_ref().or(self.remote.as_ref()),
        }
    }

    /// Connected devices with a battery level, sorted by name.
    pub fn devices(&self) -> Vec<&BatteryDevice> {
        let mut v: Vec<_> = self.devices.values().collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }

    pub fn laptop(&self) -> Option<&BatteryDevice> {
        self.laptop.as_ref()
    }

    pub fn connectivity(&self) -> &crate::connectivity::Connectivity {
        &self.conn
    }

    /// Script activities, newest first.
    pub fn custom(&self) -> &[Custom] {
        &self.custom
    }

    /// Recent notifications, newest first.
    pub fn notes(&self) -> impl Iterator<Item = &Note> {
        self.notes.iter()
    }

    pub fn calendar(&self) -> &[crate::calendar::CalEvent] {
        &self.calendar
    }

    pub fn system(&self) -> Option<&crate::sysmon::Stats> {
        self.system.as_ref()
    }

    pub fn privacy(&self) -> &Privacy {
        &self.privacy
    }

    pub fn weather(&self) -> Option<&crate::weather::Weather> {
        self.weather.as_ref()
    }

    pub fn timer(&self) -> Option<&crate::timer::Timer> {
        self.timer.as_ref()
    }

    pub fn alarm(&self) -> Option<&crate::alarm::Alarm> {
        self.alarm.as_ref()
    }

    pub fn nav(&self) -> Option<&NavInfo> {
        self.nav.as_ref()
    }

    fn peek(&mut self, device: BatteryDevice, reason: PeekReason, t: f64) {
        let secs = match reason {
            PeekReason::Connected => PEEK_CONNECTED,
            PeekReason::Low => PEEK_LOW,
            PeekReason::ChargerChanged => PEEK_CHARGER,
            PeekReason::Full => PEEK_FULL,
        };
        self.last_peek.insert(device.id.clone(), t);
        self.peek = Some((device, reason, t + secs));
    }

    fn cooled_down(&self, id: &str, t: f64) -> bool {
        self.last_peek.get(id).is_none_or(|&last| t - last >= PEEK_COOLDOWN)
    }

    /// Apply a provider report. Returns true if what is live may have changed.
    pub fn report(&mut self, r: Report, t: f64) -> bool {
        match r {
            Report::Media(m) => {
                match (&m, &self.media) {
                    (Some(new), _) if new.playing => self.paused_since = None,
                    (Some(_), Some(old)) if old.playing => self.paused_since = Some(t),
                    (Some(_), None) => self.paused_since = Some(t - MEDIA_LINGER), // first seen paused: don't pop up
                    _ => {}
                }
                self.media = m;
                true
            }
            Report::DeviceConnected(d) => {
                let show = self.cooled_down(&d.id, t);
                self.devices.insert(d.id.clone(), d.clone());
                if show {
                    self.peek(d, PeekReason::Connected, t);
                }
                show
            }
            Report::DeviceChanged(d) => {
                let crossed = self.devices.get(&d.id).is_some_and(|old| {
                    !d.charging && LOW_LEVELS.iter().any(|&lvl| old.percent > lvl && d.percent <= lvl)
                });
                // Earbuds back in their case, a mouse on its cable: a charging
                // pop-up (not more than once per cool-down).
                let started = self.devices.get(&d.id).is_some_and(|old| !old.charging && d.charging)
                    && self.cooled_down(&d.id, t);
                self.devices.insert(d.id.clone(), d.clone());
                if let Some((peeked, _, _)) = &mut self.peek {
                    if peeked.id == d.id {
                        *peeked = d.clone();
                    }
                }
                if crossed {
                    self.peek(d, PeekReason::Low, t);
                } else if started {
                    self.peek(d, PeekReason::ChargerChanged, t);
                }
                true
            }
            Report::DeviceGone(id) => {
                self.devices.remove(&id);
                if self.peek.as_ref().is_some_and(|(d, _, _)| d.id == id) {
                    self.peek = None;
                }
                true
            }
            Report::Navigation(n) => {
                self.nav = n;
                true
            }
            Report::Alarm(a) => {
                self.alarm = a;
                true
            }
            Report::Connectivity(u) => {
                self.conn.apply(u);
                true
            }
            Report::Timer(t) => {
                self.timer = t;
                true
            }
            Report::Hud(h) => {
                self.hud = Some((h, t + HUD_SECS));
                true
            }
            Report::Weather(w) => {
                self.weather = w;
                true
            }
            // The island keeps the history (it knows whether it's on).
            Report::Clipboard(_) | Report::Lyrics(..) | Report::Volume(..) => false,
            Report::System(s) => {
                self.system = s;
                true
            }
            Report::Privacy(p) => {
                let changed = self.privacy != p;
                self.privacy = p;
                changed
            }
            Report::Calendar(c) => {
                self.calendar = c.unwrap_or_default();
                true
            }
            Report::Custom(c) => {
                match self.custom.iter_mut().find(|x| x.id == c.id) {
                    Some(x) => *x = c,
                    None => {
                        self.custom.insert(0, c);
                        self.custom.truncate(MAX_CUSTOM);
                    }
                }
                true
            }
            Report::CustomEnded(id) => {
                let before = self.custom.len();
                self.custom.retain(|c| c.id != id);
                self.custom.len() != before
            }
            Report::RemoteMedia(m) => {
                self.remote = m;
                true
            }
            Report::Notification(n) => {
                self.notes.push_front(n);
                self.notes.truncate(NOTE_HISTORY);
                self.note_until = t + NOTE_SECS;
                true
            }
            Report::UpNext(u) => {
                let changed = self.up_next.as_ref().map(|(e, _)| e) != u.as_ref().map(|(e, _)| e);
                self.up_next = u;
                changed
            }
            Report::Laptop(d) => {
                let charger_changed = self.laptop.as_ref().is_some_and(|old| old.charging != d.charging);
                let crossed = self.laptop.as_ref().is_some_and(|old| {
                    !d.charging && LOW_LEVELS.iter().any(|&lvl| old.percent > lvl && d.percent <= lvl)
                });
                let full = d.charging
                    && d.percent >= 100
                    && self.laptop.as_ref().is_some_and(|old| old.charging && old.percent < 100);
                self.laptop = Some(d.clone());
                if charger_changed {
                    self.peek(d, PeekReason::ChargerChanged, t);
                } else if crossed {
                    self.peek(d, PeekReason::Low, t);
                } else if full {
                    self.peek(d, PeekReason::Full, t);
                }
                charger_changed || crossed || full
            }
        }
    }

    /// Live activities in priority order: a ringing alarm, a battery peek,
    /// a timer, navigation, then media.
    pub fn live(&self, t: f64) -> Vec<Live> {
        let mut out = Vec::new();
        if let Some(a) = &self.alarm {
            out.push(Live::Alarm(a.clone()));
        }
        if let Some((h, until)) = &self.hud {
            if t < *until {
                out.push(Live::Hud(*h));
            }
        }
        if t < self.note_until {
            if let Some(n) = self.notes.front() {
                out.push(Live::Note(n.clone()));
            }
        }
        if let Some((d, reason, until)) = &self.peek {
            if t < *until {
                out.push(Live::Battery { device: d.clone(), reason: *reason });
            }
        }
        if let Some(tm) = &self.timer {
            out.push(Live::Timer(tm.clone()));
        }
        if let Some(c) = self.custom.first() {
            out.push(Live::Custom(c.clone()));
        }
        if let Some((event, starts_at)) = &self.up_next {
            out.push(Live::Event { event: event.clone(), starts_at: *starts_at });
        }
        if let Some(n) = &self.nav {
            out.push(Live::Navigation(n.clone()));
        }
        let local =
            self.media.as_ref().filter(|m| m.playing || self.paused_since.is_some_and(|p| t - p < MEDIA_LINGER));
        match (local, self.remote.as_ref().filter(|r| r.playing)) {
            (Some(m), _) if m.playing => out.push(Live::Media(m.clone())),
            (_, Some(r)) => out.push(Live::Media(r.clone())),
            (Some(m), None) => out.push(Live::Media(m.clone())),
            (None, None) => {}
        }
        out
    }

    /// Next moment the live set changes on its own (a peek or linger ends).
    pub fn next_change(&self, t: f64) -> Option<f64> {
        let peek_end = self.peek.as_ref().map(|(_, _, until)| *until).filter(|&u| u > t);
        let hud_end = self.hud.as_ref().map(|(_, until)| *until).filter(|&u| u > t);
        let note_end = Some(self.note_until).filter(|&u| u > t);
        let hud_end = match (hud_end, note_end) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        let peek_end = match (peek_end, hud_end) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        let linger_end = match (&self.media, self.paused_since) {
            (Some(m), Some(p)) if !m.playing => Some(p + MEDIA_LINGER).filter(|&e| e > t),
            _ => None,
        };
        match (peek_end, linger_end) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(playing: bool) -> MediaInfo {
        MediaInfo {
            source: "org.mpris.MediaPlayer2.spotify".into(),
            app_name: "Spotify".into(),
            title: "Song".into(),
            artist: "Artist".into(),
            playing,
            position: Some(10.0),
            position_at: 0.0,
            duration: Some(200.0),
            rate: 1.0,
            ..MediaInfo::default()
        }
    }

    fn buds(percent: u8) -> BatteryDevice {
        BatteryDevice {
            id: "bt:11:22".into(),
            name: "Earbuds".into(),
            kind: DeviceKind::Earbuds,
            percent,
            charging: false,
            parts: vec![],
        }
    }

    #[test]
    fn position_extrapolates_only_while_playing() {
        let m = song(true);
        assert_eq!(m.position_now(5.0), Some(15.0));
        assert_eq!(m.position_now(500.0), Some(200.0), "clamped to the duration");
        assert_eq!(song(false).position_now(5.0), Some(10.0));
    }

    #[test]
    fn playing_media_is_live_and_lingers_after_pause() {
        let mut a = Activities::new();
        a.report(Report::Media(Some(song(true))), 0.0);
        assert!(matches!(a.live(1.0)[..], [Live::Media(_)]));
        a.report(Report::Media(Some(song(false))), 10.0);
        assert_eq!(a.live(20.0).len(), 1, "still lingering");
        assert_eq!(a.next_change(20.0), Some(10.0 + MEDIA_LINGER));
        assert!(a.live(10.0 + MEDIA_LINGER).is_empty(), "gone after the linger");
    }

    #[test]
    fn a_session_first_seen_paused_does_not_pop_up() {
        let mut a = Activities::new();
        a.report(Report::Media(Some(song(false))), 100.0);
        assert!(a.live(100.0).is_empty());
    }

    #[test]
    fn connecting_a_device_peeks_once_per_cooldown() {
        let mut a = Activities::new();
        assert!(a.report(Report::DeviceConnected(buds(87)), 0.0));
        assert!(matches!(&a.live(1.0)[..], [Live::Battery { reason: PeekReason::Connected, .. }]));
        assert!(a.live(PEEK_CONNECTED).is_empty(), "the peek ends");
        a.report(Report::DeviceGone("bt:11:22".into()), 5.0);
        assert!(!a.report(Report::DeviceConnected(buds(87)), 6.0), "flaky reconnect: no second peek");
        assert!(a.report(Report::DeviceConnected(buds(87)), 6.0 + PEEK_COOLDOWN));
    }

    #[test]
    fn low_battery_alerts_when_crossing_20_and_10() {
        let mut a = Activities::new();
        a.report(Report::DeviceConnected(buds(40)), 0.0);
        a.report(Report::DeviceChanged(buds(21)), 100.0);
        assert!(a.live(100.0).is_empty(), "no alert above 20%");
        a.report(Report::DeviceChanged(buds(20)), 200.0);
        assert!(matches!(&a.live(200.0)[..], [Live::Battery { reason: PeekReason::Low, .. }]));
        a.report(Report::DeviceChanged(buds(19)), 300.0);
        assert!(a.live(300.0).is_empty(), "no repeat until the next threshold");
        a.report(Report::DeviceChanged(buds(10)), 400.0);
        assert!(!a.live(400.0).is_empty());
    }

    #[test]
    fn charger_plug_peeks_with_the_laptop_battery() {
        let mut a = Activities::new();
        let laptop = BatteryDevice {
            id: "laptop".into(),
            name: "Laptop".into(),
            kind: DeviceKind::Laptop,
            percent: 64,
            charging: false,
            parts: vec![],
        };
        assert!(!a.report(Report::Laptop(laptop.clone()), 0.0), "first report is just the baseline");
        assert!(a.report(Report::Laptop(BatteryDevice { charging: true, ..laptop }), 10.0));
        assert!(matches!(&a.live(10.5)[..], [Live::Battery { reason: PeekReason::ChargerChanged, .. }]));
    }

    #[test]
    fn this_computer_wins_unless_only_another_device_plays() {
        let mut a = Activities::new();
        let phone =
            MediaInfo { source: "spotify-connect".into(), title: "On the phone".into(), playing: true, ..song(true) };
        a.report(Report::RemoteMedia(Some(phone)), 0.0);
        assert_eq!(a.media().map(|m| m.title.as_str()), Some("On the phone"));
        a.report(Report::Media(Some(song(true))), 1.0);
        assert_eq!(a.media().map(|m| m.source.as_str()), Some(song(true).source.as_str()), "local playback wins");
        a.report(Report::Media(Some(song(false))), 2.0);
        assert_eq!(a.media().map(|m| m.title.as_str()), Some("On the phone"), "paused here, playing there");
        assert!(matches!(&a.live(3.0)[..], [Live::Media(m)] if m.title == "On the phone"));
    }

    #[test]
    fn battery_peek_comes_before_media() {
        let mut a = Activities::new();
        a.report(Report::Media(Some(song(true))), 0.0);
        a.report(Report::DeviceConnected(buds(87)), 1.0);
        let live = a.live(2.0);
        assert!(matches!(live[..], [Live::Battery { .. }, Live::Media(_)]));
        assert_eq!(a.next_change(2.0), Some(1.0 + PEEK_CONNECTED));
    }

    #[test]
    fn privacy_reports_only_changes() {
        let mut a = Activities::new();
        assert!(!a.privacy().active());
        let p = Privacy { camera: Some(vec!["Zoom".into()]), mic: None };
        assert!(a.report(Report::Privacy(p.clone()), 0.0));
        assert!(!a.report(Report::Privacy(p), 1.0));
        assert_eq!(a.privacy().describe().as_deref(), Some("Camera in use by Zoom"));
        let both = Privacy { camera: Some(vec!["Zoom".into()]), mic: Some(vec!["Zoom".into(), "OBS".into()]) };
        assert_eq!(both.describe().as_deref(), Some("Camera in use by Zoom; Microphone in use by Zoom, OBS"));
        let unknown = Privacy { camera: None, mic: Some(vec![]) };
        assert_eq!(unknown.describe().as_deref(), Some("Microphone in use"));
        assert!(a.report(Report::Privacy(Privacy::default()), 2.0));
        assert_eq!(a.privacy().describe(), None);
        assert!(a.live(2.0).is_empty(), "the dots never make the island open");
    }

    #[test]
    fn charging_notifications() {
        let laptop = |percent: u8, charging: bool| BatteryDevice {
            id: "laptop".into(),
            name: "Battery".into(),
            kind: DeviceKind::Laptop,
            percent,
            charging,
            parts: vec![],
        };
        let mut a = Activities::new();
        a.report(Report::Laptop(laptop(98, true)), 0.0);
        assert!(a.report(Report::Laptop(laptop(100, true)), 10.0));
        assert!(matches!(&a.live(10.5)[..], [Live::Battery { reason: PeekReason::Full, .. }]));
        assert!(!a.report(Report::Laptop(laptop(100, true)), 20.0), "only once");

        // A device that starts charging pops up, but not again right away.
        let buds = |charging: bool| BatteryDevice {
            id: "buds".into(),
            name: "Buds".into(),
            kind: DeviceKind::Earbuds,
            percent: 50,
            charging,
            parts: vec![],
        };
        let mut b = Activities::new();
        b.report(Report::DeviceConnected(buds(false)), 0.0);
        b.report(Report::DeviceChanged(buds(true)), 100.0);
        assert!(matches!(&b.live(100.5)[..], [Live::Battery { reason: PeekReason::ChargerChanged, .. }]));
        b.report(Report::DeviceChanged(buds(false)), 110.0);
        b.report(Report::DeviceChanged(buds(true)), 115.0);
        assert!(b.live(120.0).is_empty(), "cool-down");
    }
}
