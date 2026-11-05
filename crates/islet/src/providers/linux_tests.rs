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

// ---------------------------------------------- NetworkManager + BlueZ ---

/// What the fake NetworkManager's networks and connections are doing.
#[derive(Default)]
struct NmState {
    active_ap: String,
    active_state: u32,
    last_scan: i64,
    added: Vec<String>,
    deleted: bool,
}

type Shared = Arc<Mutex<NmState>>;
const NMP: &str = "/org/freedesktop/NetworkManager";

fn opath(s: &str) -> OwnedObjectPath {
    OwnedObjectPath::try_from(s.to_string()).unwrap()
}

struct FakeNm(Shared);

#[zbus::interface(name = "org.freedesktop.NetworkManager")]
impl FakeNm {
    fn get_devices(&self) -> Vec<OwnedObjectPath> {
        vec![opath(&format!("{NMP}/Devices/1")), opath(&format!("{NMP}/Devices/2"))]
    }

    #[zbus(property)]
    fn wireless_enabled(&self) -> bool {
        true
    }

    fn activate_connection(&self, _c: OwnedObjectPath, _d: OwnedObjectPath, ap: OwnedObjectPath) -> OwnedObjectPath {
        let mut s = self.0.lock().unwrap();
        s.active_ap = ap.to_string();
        s.active_state = 2;
        opath(&format!("{NMP}/ActiveConnection/1"))
    }

    fn add_and_activate_connection(
        &self,
        settings: HashMap<String, HashMap<String, OwnedValue>>,
        _d: OwnedObjectPath,
        ap: OwnedObjectPath,
    ) -> (OwnedObjectPath, OwnedObjectPath) {
        let mut s = self.0.lock().unwrap();
        let psk = settings
            .get("802-11-wireless-security")
            .and_then(|sec| sec.get("psk"))
            .and_then(|v| <&str>::try_from(v).ok().map(String::from));
        s.added.push(psk.clone().unwrap_or_default());
        // "right" is the right password; open networks need none.
        let ok = psk.as_deref().is_none_or(|p| p == "right");
        s.active_state = if ok { 2 } else { 4 };
        if ok {
            s.active_ap = ap.to_string();
        }
        (opath(&format!("{NMP}/Settings/9")), opath(&format!("{NMP}/ActiveConnection/1")))
    }
}

struct FakeNmDevice(u32);

#[zbus::interface(name = "org.freedesktop.NetworkManager.Device")]
impl FakeNmDevice {
    #[zbus(property)]
    fn device_type(&self) -> u32 {
        self.0
    }
}

struct FakeWireless(Shared);

#[zbus::interface(name = "org.freedesktop.NetworkManager.Device.Wireless")]
impl FakeWireless {
    #[zbus(property)]
    fn active_access_point(&self) -> OwnedObjectPath {
        let ap = self.0.lock().unwrap().active_ap.clone();
        opath(if ap.is_empty() { "/" } else { &ap })
    }

    #[zbus(property)]
    fn last_scan(&self) -> i64 {
        self.0.lock().unwrap().last_scan
    }

    fn get_all_access_points(&self) -> Vec<OwnedObjectPath> {
        (1..=3).map(|i| opath(&format!("{NMP}/AccessPoint/{i}"))).collect()
    }

    fn request_scan(&self, _options: HashMap<String, OwnedValue>) {
        self.0.lock().unwrap().last_scan += 1;
    }
}

struct FakeAp {
    ssid: &'static str,
    strength: u8,
    secure: bool,
}

#[zbus::interface(name = "org.freedesktop.NetworkManager.AccessPoint")]
impl FakeAp {
    #[zbus(property)]
    fn ssid(&self) -> Vec<u8> {
        self.ssid.as_bytes().to_vec()
    }

    #[zbus(property)]
    fn strength(&self) -> u8 {
        self.strength
    }

    #[zbus(property)]
    fn flags(&self) -> u32 {
        u32::from(self.secure)
    }

    #[zbus(property)]
    fn wpa_flags(&self) -> u32 {
        0
    }

    #[zbus(property)]
    fn rsn_flags(&self) -> u32 {
        if self.secure {
            0x100
        } else {
            0
        }
    }
}

struct FakeNmSettings;

#[zbus::interface(name = "org.freedesktop.NetworkManager.Settings")]
impl FakeNmSettings {
    fn list_connections(&self) -> Vec<OwnedObjectPath> {
        vec![opath(&format!("{NMP}/Settings/1"))]
    }
}

struct FakeSavedConnection(Shared);

#[zbus::interface(name = "org.freedesktop.NetworkManager.Settings.Connection")]
impl FakeSavedConnection {
    fn get_settings(&self) -> HashMap<String, HashMap<String, OwnedValue>> {
        let mut wireless = HashMap::new();
        wireless.insert("ssid".to_string(), ov(Value::from(b"Home".to_vec())));
        HashMap::from([("802-11-wireless".to_string(), wireless)])
    }

    fn delete(&self) {
        self.0.lock().unwrap().deleted = true;
    }
}

struct FakeActive(Shared);

#[zbus::interface(name = "org.freedesktop.NetworkManager.Connection.Active")]
impl FakeActive {
    #[zbus(property)]
    fn state(&self) -> u32 {
        self.0.lock().unwrap().active_state
    }
}

struct FakeAdapter;

#[zbus::interface(name = "org.bluez.Adapter1")]
impl FakeAdapter {
    #[zbus(property)]
    fn powered(&self) -> bool {
        true
    }
}

struct FakeBtDevice {
    alias: &'static str,
    icon: &'static str,
    paired: bool,
    connected: bool,
}

#[zbus::interface(name = "org.bluez.Device1")]
impl FakeBtDevice {
    #[zbus(property)]
    fn alias(&self) -> String {
        self.alias.into()
    }

    #[zbus(property)]
    fn icon(&self) -> String {
        self.icon.into()
    }

    #[zbus(property)]
    fn paired(&self) -> bool {
        self.paired
    }

    #[zbus(property)]
    fn connected(&self) -> bool {
        self.connected
    }

    fn connect(&mut self) {
        self.connected = true;
    }

    fn disconnect(&mut self) {
        self.connected = false;
    }
}

#[test]
fn wifi_and_bluetooth_through_networkmanager_and_bluez() {
    use super::connectivity::{Backend, ConnectError};
    let bus = bus_or_skip!();
    let state: Shared = Arc::default();
    let s = |st: &Shared| FakeSavedConnection(st.clone());
    let _nm = zbus::blocking::connection::Builder::address(bus.address.as_str())
        .unwrap()
        .name("org.freedesktop.NetworkManager")
        .unwrap()
        .serve_at(NMP, FakeNm(state.clone()))
        .unwrap()
        .serve_at(format!("{NMP}/Devices/1"), FakeNmDevice(1)) // ethernet
        .unwrap()
        .serve_at(format!("{NMP}/Devices/2"), FakeNmDevice(2)) // Wi-Fi
        .unwrap()
        .serve_at(format!("{NMP}/Devices/2"), FakeWireless(state.clone()))
        .unwrap()
        .serve_at(format!("{NMP}/AccessPoint/1"), FakeAp { ssid: "Home", strength: 70, secure: true })
        .unwrap()
        .serve_at(format!("{NMP}/AccessPoint/2"), FakeAp { ssid: "Cafe", strength: 90, secure: true })
        .unwrap()
        .serve_at(format!("{NMP}/AccessPoint/3"), FakeAp { ssid: "Library", strength: 30, secure: false })
        .unwrap()
        .serve_at(format!("{NMP}/Settings"), FakeNmSettings)
        .unwrap()
        .serve_at(format!("{NMP}/Settings/1"), s(&state))
        .unwrap()
        .serve_at(format!("{NMP}/Settings/9"), s(&state))
        .unwrap()
        .serve_at(format!("{NMP}/ActiveConnection/1"), FakeActive(state.clone()))
        .unwrap()
        .build()
        .unwrap();
    let _bluez = zbus::blocking::connection::Builder::address(bus.address.as_str())
        .unwrap()
        .name("org.bluez")
        .unwrap()
        .serve_at("/", zbus::fdo::ObjectManager)
        .unwrap()
        .serve_at("/org/bluez/hci0", FakeAdapter)
        .unwrap()
        .serve_at(
            "/org/bluez/hci0/dev_AA_BB",
            FakeBtDevice { alias: "Buds", icon: "audio-headset", paired: true, connected: false },
        )
        .unwrap()
        .serve_at(
            "/org/bluez/hci0/dev_CC_DD",
            FakeBtDevice { alias: "Stranger's phone", icon: "phone", paired: false, connected: false },
        )
        .unwrap()
        .build()
        .unwrap();

    let mut b = super::linux_conn::Linux::new(Bus::Address(bus.address.clone()));

    // The list: saved Home first, then by signal; the ethernet device is skipped.
    let w = b.wifi(true);
    assert!(w.available && w.enabled && w.current.is_none(), "{w:?}");
    let names: Vec<_> = w.networks.iter().map(|n| (n.ssid.as_str(), n.known, n.secure)).collect();
    assert_eq!(names, [("Home", true, true), ("Cafe", false, true), ("Library", false, false)]);
    assert_eq!(state.lock().unwrap().last_scan, 1, "a scan was requested");

    // A saved network connects without a password.
    b.wifi_connect("Home", None).unwrap();
    assert_eq!(b.wifi(false).current.as_deref(), Some("Home"));

    // A new secured network asks for one; a wrong one fails and is cleaned up.
    assert_eq!(b.wifi_connect("Cafe", None), Err(ConnectError::NeedsPassword));
    assert!(matches!(b.wifi_connect("Cafe", Some("wrong")), Err(ConnectError::Failed(m)) if m.contains("password")));
    assert!(state.lock().unwrap().deleted, "the failed profile was deleted");
    b.wifi_connect("Cafe", Some("right")).unwrap();
    assert_eq!(b.wifi(false).current.as_deref(), Some("Cafe"));
    b.wifi_connect("Library", None).unwrap();
    assert_eq!(state.lock().unwrap().added, ["wrong", "right", ""]);

    // Bluetooth: only paired devices; connect and disconnect.
    let bt = b.bluetooth();
    assert!(bt.available && bt.powered && bt.can_connect);
    assert_eq!(bt.devices.len(), 1);
    assert_eq!((bt.devices[0].name.as_str(), bt.devices[0].kind), ("Buds", DeviceKind::Earbuds));
    b.bt_set("/org/bluez/hci0/dev_AA_BB", true).unwrap();
    assert!(b.bluetooth().devices[0].connected);
    b.bt_set("/org/bluez/hci0/dev_AA_BB", false).unwrap();
    assert!(!b.bluetooth().devices[0].connected);
}

#[test]
fn missing_services_are_reported_not_fatal() {
    use super::connectivity::Backend;
    let bus = bus_or_skip!();
    let mut b = super::linux_conn::Linux::new(Bus::Address(bus.address.clone()));
    let w = b.wifi(false);
    assert!(!w.available && w.note.as_deref() == Some("NetworkManager isn't running"), "{w:?}");
    let bt = b.bluetooth();
    assert!(!bt.available && bt.note.is_some());
}
