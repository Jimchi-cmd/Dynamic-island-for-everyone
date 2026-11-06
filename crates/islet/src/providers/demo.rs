//! `ISLET_DEMO=media`: a scripted session that exercises the real provider
//! pipeline (thread → channel → waker → island) without any OS services.
//! Music starts, earbuds connect, the earbuds run low, a mouse connects; the
//! player responds to play/pause, next, previous and seeking.

use super::{art, Sink};
use islet_core::activity::{BatteryDevice, DeviceKind, MediaCaps, MediaCommand, MediaInfo, Part, Report};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};
use tiny_skia::{Color, GradientStop, LinearGradient, Paint, Pixmap, Point, Rect, SpreadMode, Transform};

const TRACKS: [(&str, &str, f64); 3] = [
    ("Midnight Signals", "The Placeholders", 214.0),
    ("Paper Planets", "Lumen & Co.", 187.0),
    ("Long Way to the Top of the Screen", "Island Radio", 241.0),
];

fn cover(i: usize) -> Option<islet_core::activity::ArtImage> {
    let colours = [[(88, 40, 200), (230, 80, 120)], [(20, 140, 160), (240, 200, 60)], [(30, 30, 40), (240, 120, 40)]];
    let [(r1, g1, b1), (r2, g2, b2)] = colours[i % colours.len()];
    let mut pm = Pixmap::new(256, 256)?;
    let shader = LinearGradient::new(
        Point::from_xy(0.0, 0.0),
        Point::from_xy(256.0, 256.0),
        vec![
            GradientStop::new(0.0, Color::from_rgba8(r1, g1, b1, 255)),
            GradientStop::new(1.0, Color::from_rgba8(r2, g2, b2, 255)),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    )?;
    let paint = Paint { shader, ..Paint::default() };
    pm.fill_rect(Rect::from_xywh(0.0, 0.0, 256.0, 256.0)?, &paint, Transform::identity(), None);
    Some(art::from_pixmap(&pm))
}

struct Player {
    track: usize,
    playing: bool,
    /// Position at `since`.
    position: f64,
    since: Instant,
}

impl Player {
    fn position(&self) -> f64 {
        let p = if self.playing { self.position + self.since.elapsed().as_secs_f64() } else { self.position };
        p.min(TRACKS[self.track].2)
    }

    fn info(&self) -> MediaInfo {
        let (title, artist, duration) = TRACKS[self.track];
        MediaInfo {
            source: "demo".into(),
            app_name: "Islet Demo".into(),
            title: title.into(),
            artist: artist.into(),
            album: "Demo Album".into(),
            playing: self.playing,
            position: Some(self.position()),
            position_at: 0.0,
            duration: Some(duration),
            rate: 1.0,
            art: cover(self.track),
            file: None,
            caps: MediaCaps { play_pause: true, next: true, previous: true, seek: true },
        }
    }

    fn apply(&mut self, cmd: MediaCommand) {
        let now = self.position();
        match cmd {
            MediaCommand::PlayPause => self.playing = !self.playing,
            MediaCommand::Next => {
                self.track = (self.track + 1) % TRACKS.len();
                self.playing = true;
                self.position = 0.0;
                self.since = Instant::now();
                return;
            }
            MediaCommand::Previous => {
                self.position = 0.0;
                self.since = Instant::now();
                return;
            }
            MediaCommand::SeekTo(s) => {
                self.position = s.clamp(0.0, TRACKS[self.track].2);
                self.since = Instant::now();
                return;
            }
        }
        self.position = now;
        self.since = Instant::now();
    }
}

fn earbuds(percent: u8) -> BatteryDevice {
    BatteryDevice {
        id: "demo:earbuds".into(),
        name: "Demo Earbuds".into(),
        kind: DeviceKind::Earbuds,
        percent,
        charging: false,
        parts: vec![(Part::Left, percent), (Part::Right, percent.saturating_add(6).min(100)), (Part::Case, 64)],
    }
}

pub fn start(sink: Sink) -> Sender<MediaCommand> {
    let (tx, rx) = channel::<MediaCommand>();
    std::thread::Builder::new()
        .name("islet-demo".into())
        .spawn(move || {
            let start = Instant::now();
            let mut player = Player { track: 0, playing: true, position: 37.0, since: start };
            let mouse = BatteryDevice {
                id: "demo:mouse".into(),
                name: "Demo Mouse".into(),
                kind: DeviceKind::Mouse,
                percent: 64,
                charging: false,
                parts: vec![],
            };
            let script: Vec<(f64, Report)> = vec![
                (0.5, Report::Media(Some(player.info()))),
                (4.0, Report::DeviceConnected(earbuds(87))),
                (12.0, Report::DeviceChanged(earbuds(20))),
                (20.0, Report::DeviceConnected(mouse)),
            ];
            let mut next = 0;
            loop {
                let wait = script
                    .get(next)
                    .map(|(at, _)| Duration::from_secs_f64(at - start.elapsed().as_secs_f64()).max(Duration::ZERO));
                let received = match wait {
                    Some(d) => rx.recv_timeout(d),
                    None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
                };
                match received {
                    Ok(cmd) => {
                        player.apply(cmd);
                        sink.send(Report::Media(Some(player.info())));
                    }
                    Err(RecvTimeoutError::Timeout) => {
                        let report = match &script[next].1 {
                            Report::Media(_) => Report::Media(Some(player.info())),
                            other => other.clone(),
                        };
                        sink.send(report);
                        next += 1;
                    }
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
        })
        .expect("spawn demo provider");
    tx
}
