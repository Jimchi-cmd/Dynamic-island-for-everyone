//! End-to-end tests of the Linux providers against fake services on a
//! private D-Bus daemon: a fake MPRIS player and a fake UPower.
//! Skipped (with a note) when `dbus-daemon` is not installed.

use super::{linux_battery, linux_media, Bus, Sink};
use crate::platform::Waker;
use islet_core::activity::{DeviceKind, MediaCommand, Report};
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

/// A private bus daemon, killed on drop.
struct TestBus {
    child: Child,
    address: String,
}

impl TestBus {
    fn start() -> Option<Self> {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let mut line = String::new();
        BufReader::new(child.stdout.take()?).read_line(&mut line).ok()?;
        let address = line.trim().to_string();
        (!address.is_empty()).then_some(Self { child, address })
    }
}

impl Drop for TestBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn sink() -> (Sink, Receiver<(Report, Instant)>) {
    let (tx, rx) = channel();
    (Sink { tx, waker: Waker::new(|| {}) }, rx)
}

/// Wait for the first report matching `want`, skipping others.
fn expect(rx: &Receiver<(Report, Instant)>, what: &str, want: impl Fn(&Report) -> bool) -> Report {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left) {
            Ok((r, _)) if want(&r) => return r,
            Ok(_) => continue,
            Err(_) => panic!("timed out waiting for: {what}"),
        }
    }
}

macro_rules! bus_or_skip {
    () => {
        match TestBus::start() {
            Some(b) => b,
            None => {
                eprintln!("skipping: dbus-daemon not available");
                return;
            }
        }
    };
}

// ---------------------------------------------------------------- MPRIS ---

struct FakeRoot;

#[zbus::interface(name = "org.mpris.MediaPlayer2")]
impl FakeRoot {
    #[zbus(property)]
    fn identity(&self) -> String {
        "Fake Player".into()
    }
}

struct FakePlayer {
    playing: bool,
    calls: Arc<Mutex<Vec<String>>>,
}

fn ov(v: Value<'static>) -> OwnedValue {
    OwnedValue::try_from(v).unwrap()
}

#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl FakePlayer {
    async fn play_pause(&mut self, #[zbus(signal_emitter)] emitter: SignalEmitter<'_>) {
        self.calls.lock().unwrap().push("PlayPause".into());
        self.playing = !self.playing;
        let _ = self.playback_status_changed(&emitter).await;
    }

    fn next(&mut self) {
        self.calls.lock().unwrap().push("Next".into());
    }

    fn previous(&mut self) {
        self.calls.lock().unwrap().push("Previous".into());
    }

    fn set_position(&mut self, _track: ObjectPath<'_>, position: i64) {
        self.calls.lock().unwrap().push(format!("SetPosition {position}"));
    }

    #[zbus(property)]
    fn playback_status(&self) -> String {
        if self.playing { "Playing" } else { "Paused" }.into()
    }

    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        HashMap::from([
            ("xesam:title".to_string(), ov(Value::from("Song A"))),
            ("xesam:artist".to_string(), ov(Value::from(vec!["Band"]))),
            ("xesam:album".to_string(), ov(Value::from("Album"))),
            ("mpris:length".to_string(), ov(Value::from(200_000_000i64))),
            ("mpris:trackid".to_string(), ov(Value::from(ObjectPath::try_from("/track/1").unwrap()))),
        ])
    }

    #[zbus(property)]
    fn position(&self) -> i64 {
        30_000_000
    }

    #[zbus(property)]
    fn rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_play(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_seek(&self) -> bool {
        true
    }
}

#[test]
fn mpris_player_is_reported_controlled_and_removed() {
    let bus = bus_or_skip!();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let player = zbus::blocking::connection::Builder::address(bus.address.as_str())
        .unwrap()
        .name("org.mpris.MediaPlayer2.fake")
        .unwrap()
        .serve_at("/org/mpris/MediaPlayer2", FakeRoot)
        .unwrap()
        .serve_at("/org/mpris/MediaPlayer2", FakePlayer { playing: true, calls: calls.clone() })
        .unwrap()
        .build()
        .unwrap();

    let (sink, rx) = sink();
    let commands = linux_media::start(sink, Bus::Address(bus.address.clone())).expect("provider starts");
    let Report::Media(Some(m)) = expect(&rx, "the playing session", |r| matches!(r, Report::Media(Some(_)))) else {
        unreachable!()
    };
    assert_eq!((m.title.as_str(), m.artist.as_str(), m.app_name.as_str()), ("Song A", "Band", "Fake Player"));
    assert!(m.playing);
    assert_eq!(m.duration, Some(200.0));
    assert!((m.position.unwrap() - 30.0).abs() < 1.0);
    assert!(m.caps.next && m.caps.seek && m.caps.play_pause);

    commands.send(MediaCommand::PlayPause).unwrap();
    expect(&rx, "paused after PlayPause", |r| matches!(r, Report::Media(Some(m)) if !m.playing));
    commands.send(MediaCommand::SeekTo(42.0)).unwrap();
    commands.send(MediaCommand::Next).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while calls.lock().unwrap().len() < 3 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(*calls.lock().unwrap(), ["PlayPause", "SetPosition 42000000", "Next"]);

    drop(player); // the player quits
    expect(&rx, "no session once the player leaves", |r| matches!(r, Report::Media(None)));
}

// --------------------------------------------------------------- UPower ---

const DEV: &str = "/org/freedesktop/UPower/devices";

struct FakeUPower {
    devices: Vec<OwnedObjectPath>,
    on_battery: bool,
}

#[zbus::interface(name = "org.freedesktop.UPower")]
impl FakeUPower {
    fn enumerate_devices(&self) -> Vec<OwnedObjectPath> {
        self.devices.clone()
    }

    fn get_display_device(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from(format!("{DEV}/DisplayDevice")).unwrap()
    }

    #[zbus(property)]
    fn on_battery(&self) -> bool {
        self.on_battery
    }

    #[zbus(signal)]
    async fn device_added(emitter: &SignalEmitter<'_>, device: ObjectPath<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn device_removed(emitter: &SignalEmitter<'_>, device: ObjectPath<'_>) -> zbus::Result<()>;
}

struct FakeDevice {
    kind: u32,
    percent: f64,
    model: String,
    power_supply: bool,
}

#[zbus::interface(name = "org.freedesktop.UPower.Device")]
impl FakeDevice {
    #[zbus(property, name = "Type")]
    fn kind(&self) -> u32 {
        self.kind
    }

    #[zbus(property)]
    fn percentage(&self) -> f64 {
        self.percent
    }

    #[zbus(property)]
    fn model(&self) -> String {
        self.model.clone()
    }

    #[zbus(property)]
    fn power_supply(&self) -> bool {
        self.power_supply
    }

    #[zbus(property)]
    fn is_present(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn state(&self) -> u32 {
        2 // discharging
    }
}

#[test]
fn upower_devices_connect_change_and_leave() {
    let bus = bus_or_skip!();
    let mouse = format!("{DEV}/mouse_1");
    let upower = zbus::blocking::connection::Builder::address(bus.address.as_str())
        .unwrap()
        .name("org.freedesktop.UPower")
        .unwrap()
        .serve_at(
            "/org/freedesktop/UPower",
            FakeUPower { devices: vec![OwnedObjectPath::try_from(mouse.as_str()).unwrap()], on_battery: true },
        )
        .unwrap()
        .serve_at(
            format!("{DEV}/DisplayDevice"),
            FakeDevice { kind: 2, percent: 64.0, model: String::new(), power_supply: true },
        )
        .unwrap()
        .serve_at(
            mouse.as_str(),
            FakeDevice { kind: 5, percent: 40.0, model: "Test Mouse".into(), power_supply: false },
        )
        .unwrap()
        .build()
        .unwrap();

    let (sink, rx) = sink();
    linux_battery::start(sink, Bus::Address(bus.address.clone()));
    // At startup: the mouse is recorded (no pop-up) and the laptop baseline.
    let Report::DeviceChanged(d) = expect(&rx, "the mouse at startup", |r| matches!(r, Report::DeviceChanged(_)))
    else {
        unreachable!()
    };
    assert_eq!((d.name.as_str(), d.kind, d.percent), ("Test Mouse", DeviceKind::Mouse, 40));
    expect(&rx, "the laptop on battery", |r| matches!(r, Report::Laptop(l) if l.percent == 64 && !l.charging));

    // Headphones connect.
    let headset = format!("{DEV}/headset_1");
    upower
        .object_server()
        .at(
            headset.as_str(),
            FakeDevice { kind: 19, percent: 87.0, model: "Test Headphones".into(), power_supply: false },
        )
        .unwrap();
    let root = upower.object_server().interface::<_, FakeUPower>("/org/freedesktop/UPower").unwrap();
    zbus::block_on(FakeUPower::device_added(root.signal_emitter(), ObjectPath::try_from(headset.as_str()).unwrap()))
        .unwrap();
    expect(
        &rx,
        "headphones connected",
        |r| matches!(r, Report::DeviceConnected(d) if d.kind == DeviceKind::Headphones && d.percent == 87),
    );

    // The mouse runs low.
    let mouse_ref = upower.object_server().interface::<_, FakeDevice>(mouse.as_str()).unwrap();
    mouse_ref.get_mut().percent = 15.0;
    zbus::block_on(mouse_ref.get().percentage_changed(mouse_ref.signal_emitter())).unwrap();
    expect(&rx, "the mouse at 15%", |r| matches!(r, Report::DeviceChanged(d) if d.percent == 15));

    // The charger is plugged in.
    root.get_mut().on_battery = false;
    zbus::block_on(root.get().on_battery_changed(root.signal_emitter())).unwrap();
    expect(&rx, "the laptop charging", |r| matches!(r, Report::Laptop(l) if l.charging));

    // The headphones disconnect.
    zbus::block_on(FakeUPower::device_removed(root.signal_emitter(), ObjectPath::try_from(headset.as_str()).unwrap()))
        .unwrap();
    expect(&rx, "headphones gone", |r| matches!(r, Report::DeviceGone(id) if *id == headset));
}
