//! The phone companion's listener (Islet Pro): once a phone is paired, the
//! island accepts its connection on the local network and shows the
//! phone's notifications, music and battery. See `islet_core::phone` for
//! how the link is secured. Nothing listens until a phone is paired and
//! the link is switched on, and nothing here talks to the internet.

use super::{debug_log, Sink};
use islet_core::activity::{BatteryDevice, DeviceKind, MediaCaps, MediaCommand, MediaInfo, Note, Report};
use islet_core::phone::{Channel, FromPhone, Side, ToPhone, GREETING, KEY_LEN, MAX_FRAME, NONCE_LEN};
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The source name of music playing on the phone.
pub const SOURCE: &str = "islet-phone";
/// The battery pop-up's id for the phone.
const DEVICE_ID: &str = "islet-phone";
/// The first port tried; the next few follow if it is taken.
pub const PORT: u16 = 47821;
const PORT_TRIES: u16 = 6;
/// The phone pings every 30 seconds; silence this long ends the connection.
const SILENCE: Duration = Duration::from_secs(95);
const HANDSHAKE: Duration = Duration::from_secs(8);

/// The sending half of the live connection, if there is one.
type Outbox = Arc<Mutex<Option<(TcpStream, Channel)>>>;

/// Stops the listener when dropped (unpaired, switched off, Pro ended).
pub struct Link {
    on: Arc<AtomicBool>,
    outbox: Outbox,
    port: u16,
}

impl Drop for Link {
    fn drop(&mut self) {
        self.on.store(false, Ordering::Relaxed);
        if let Some((stream, _)) = self.outbox.lock().ok().and_then(|mut o| o.take()) {
            let _ = stream.shutdown(Shutdown::Both);
        }
        // Wake the accept loop so it sees the flag.
        let _ = TcpStream::connect_timeout(&([127, 0, 0, 1], self.port).into(), Duration::from_millis(200));
    }
}

impl Link {
    /// A button on the island's music card, for the phone's player.
    pub fn command(&self, cmd: MediaCommand) {
        let name = match cmd {
            MediaCommand::PlayPause => "play_pause",
            MediaCommand::Next => "next",
            MediaCommand::Previous => "previous",
            MediaCommand::SeekTo(_) => return,
        };
        if let Ok(mut outbox) = self.outbox.lock() {
            if let Some((stream, channel)) = outbox.as_mut() {
                let _ = send(stream, channel, &ToPhone::Command { r#do: name.into() });
            }
        }
    }
}

/// Bind the first free port from `wanted` on.
pub fn bind(wanted: u16) -> Option<(TcpListener, u16)> {
    (0..PORT_TRIES).find_map(|i| {
        let port = wanted.checked_add(i)?;
        TcpListener::bind(("0.0.0.0", port)).ok().map(|l| (l, port))
    })
}

/// Start listening for the paired phone. None if no port could be bound.
pub fn start(sink: Sink, key: [u8; KEY_LEN], wanted_port: u16, computer: String) -> Option<Link> {
    let (listener, port) = bind(wanted_port)?;
    let on = Arc::new(AtomicBool::new(true));
    let outbox: Outbox = Arc::default();
    let (flag, out) = (on.clone(), outbox.clone());
    std::thread::Builder::new()
        .name("islet-phone".into())
        .spawn(move || {
            for stream in listener.incoming() {
                if !flag.load(Ordering::Relaxed) {
                    break;
                }
                let Ok(stream) = stream else { continue };
                if let Err(why) = serve(stream, &key, &computer, &out, &sink) {
                    debug_log(&format!("phone: {why}"));
                    // Whoever it was didn't have the key: don't let them hurry.
                    std::thread::sleep(Duration::from_secs(1));
                }
                if let Ok(mut o) = out.lock() {
                    *o = None;
                }
                gone(&sink);
            }
        })
        .ok()?;
    Some(Link { on, outbox, port })
}

fn gone(sink: &Sink) {
    sink.send(Report::DeviceGone(DEVICE_ID.into()));
    sink.send(Report::RemoteMedia(None));
}

fn write_frame(stream: &mut TcpStream, frame: &[u8]) -> std::io::Result<()> {
    stream.write_all(&(frame.len() as u32).to_be_bytes())?;
    stream.write_all(frame)
}

fn read_frame(stream: &mut TcpStream) -> Result<Vec<u8>, String> {
    let mut len = [0u8; 4];
    stream.read_exact(&mut len).map_err(|e| format!("connection ended ({e})"))?;
    let len = u32::from_be_bytes(len) as usize;
    if len > MAX_FRAME {
        return Err("a message too long to be ours".into());
    }
    let mut frame = vec![0u8; len];
    stream.read_exact(&mut frame).map_err(|e| format!("connection ended ({e})"))?;
    Ok(frame)
}

fn send(stream: &mut TcpStream, channel: &mut Channel, msg: &ToPhone) -> std::io::Result<()> {
    let plain = serde_json::to_vec(msg).unwrap_or_default();
    write_frame(stream, &channel.seal(&plain))
}

/// One connection, from greeting to goodbye.
fn serve(
    mut stream: TcpStream,
    key: &[u8; KEY_LEN],
    computer: &str,
    outbox: &Outbox,
    sink: &Sink,
) -> Result<(), String> {
    let io = |e: std::io::Error| e.to_string();
    stream.set_read_timeout(Some(HANDSHAKE)).map_err(io)?;
    stream.set_write_timeout(Some(HANDSHAKE)).map_err(io)?;
    let _ = stream.set_nodelay(true);
    let mut server = [0u8; NONCE_LEN];
    getrandom::fill(&mut server).map_err(|e| e.to_string())?;
    stream.write_all(GREETING).map_err(io)?;
    stream.write_all(&server).map_err(io)?;
    let mut client = [0u8; NONCE_LEN];
    stream.read_exact(&mut client).map_err(|e| format!("no answer to the greeting ({e})"))?;
    let session = islet_core::phone::session_key(key, &server, &client);
    let mut reading = Channel::new(&session, Side::Computer);

    // The first message proves the phone has the key.
    let hello = reading.open(&read_frame(&mut stream)?).map_err(String::from)?;
    let Ok(FromPhone::Hello { name }) = serde_json::from_slice(&hello) else {
        return Err("the first message wasn't a hello".into());
    };
    let phone: String = name.chars().filter(|c| !c.is_control()).take(40).collect();
    let phone = if phone.trim().is_empty() { "Phone".to_string() } else { phone };
    let mut writing = Channel::new(&session, Side::Computer);
    let mut out = stream.try_clone().map_err(io)?;
    send(&mut out, &mut writing, &ToPhone::Ok { name: computer.to_string() }).map_err(io)?;
    if let Ok(mut o) = outbox.lock() {
        *o = Some((out, writing));
    }
    stream.set_read_timeout(Some(SILENCE)).map_err(io)?;

    let mut battery_seen = false;
    loop {
        let plain = reading.open(&read_frame(&mut stream)?).map_err(String::from)?;
        // A message this version doesn't know is skipped, not fatal.
        let Ok(msg) = serde_json::from_slice::<FromPhone>(&plain) else { continue };
        match msg {
            FromPhone::Hello { .. } => {}
            FromPhone::Ping => {
                if let Ok(mut o) = outbox.lock() {
                    if let Some((s, c)) = o.as_mut() {
                        send(s, c, &ToPhone::Pong).map_err(io)?;
                    }
                }
            }
            FromPhone::Battery { level, charging } => {
                let device = BatteryDevice {
                    id: DEVICE_ID.into(),
                    name: phone.clone(),
                    kind: DeviceKind::Phone,
                    percent: level.min(100),
                    charging,
                    parts: Vec::new(),
                };
                sink.send(if battery_seen { Report::DeviceChanged(device) } else { Report::DeviceConnected(device) });
                battery_seen = true;
            }
            FromPhone::Note { app, title, text } => {
                let clip = |s: String, n: usize| s.chars().filter(|c| !c.is_control()).take(n).collect::<String>();
                sink.send(Report::Notification(Note {
                    app: format!("{} · {phone}", clip(app, 40)),
                    summary: clip(title, 120),
                    body: clip(text, 300),
                }));
            }
            FromPhone::Media { title, artist, app, playing, duration, position } => {
                sink.send(Report::RemoteMedia(media(&phone, title, artist, app, playing, duration, position)));
            }
        }
    }
}

/// The phone's music as the island's own kind of media session.
fn media(
    phone: &str,
    title: String,
    artist: String,
    app: String,
    playing: bool,
    duration: f64,
    position: f64,
) -> Option<MediaInfo> {
    if title.trim().is_empty() {
        return None;
    }
    let clip = |s: String, n: usize| s.chars().filter(|c| !c.is_control()).take(n).collect::<String>();
    let app = clip(app, 40);
    let known = duration.is_finite() && duration > 0.0;
    Some(MediaInfo {
        source: SOURCE.into(),
        app_name: if app.is_empty() { phone.to_string() } else { format!("{app} on {phone}") },
        title: clip(title, 200),
        artist: clip(artist, 200),
        playing,
        position: known.then_some(position.clamp(0.0, duration.max(0.0))),
        duration: known.then_some(duration),
        rate: 1.0,
        caps: MediaCaps { play_pause: true, next: true, previous: true, seek: false },
        ..MediaInfo::default()
    })
}

/// This computer's address on the local network, as the phone should dial
/// it. No packet is sent: connecting a UDP socket only picks the route.
pub fn local_addresses() -> Vec<String> {
    let probe = |target: &str| {
        let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
        socket.connect(target).ok()?;
        let ip = socket.local_addr().ok()?.ip();
        (!ip.is_loopback() && !ip.is_unspecified()).then(|| ip.to_string())
    };
    // A public address and two private ones: whichever route exists answers.
    let mut found: Vec<String> =
        ["192.0.2.1:9", "10.255.255.255:9", "192.168.255.255:9"].iter().filter_map(|t| probe(t)).collect();
    found.dedup();
    found.sort();
    found.dedup();
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    /// Plays the phone's part of the link.
    struct FakePhone {
        stream: TcpStream,
        seal: Channel,
        open: Channel,
    }

    impl FakePhone {
        fn connect(port: u16, key: &[u8; KEY_LEN]) -> Self {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut greeting = [0u8; 6 + NONCE_LEN];
            stream.read_exact(&mut greeting).unwrap();
            assert_eq!(&greeting[..6], GREETING);
            let server: [u8; NONCE_LEN] = greeting[6..].try_into().unwrap();
            let client = [7u8; NONCE_LEN];
            stream.write_all(&client).unwrap();
            let session = islet_core::phone::session_key(key, &server, &client);
            Self { stream, seal: Channel::new(&session, Side::Phone), open: Channel::new(&session, Side::Phone) }
        }

        fn say(&mut self, json: &str) {
            self.try_say(json).unwrap();
        }

        /// Fails once the island has hung up.
        fn try_say(&mut self, json: &str) -> std::io::Result<()> {
            let frame = self.seal.seal(json.as_bytes());
            write_frame(&mut self.stream, &frame)
        }

        fn hear(&mut self) -> ToPhone {
            let frame = read_frame(&mut self.stream).unwrap();
            serde_json::from_slice(&self.open.open(&frame).unwrap()).unwrap()
        }
    }

    #[test]
    fn a_paired_phone_reports_and_is_controlled_and_strangers_are_not_heard() {
        let key = [9u8; KEY_LEN];
        let (tx, rx) = channel();
        let sink = Sink { tx, waker: crate::platform::Waker::new(|| {}) };
        // A port of its own, so the test never meets a running Islet.
        let link = start(sink, key, 47911, "Test laptop".into()).expect("a free port");
        let next = |want: &dyn Fn(&Report) -> bool| -> Report {
            loop {
                let (r, _) = rx.recv_timeout(Duration::from_secs(5)).expect("a report in time");
                if want(&r) {
                    return r;
                }
            }
        };

        // A stranger without the key: nothing it says is reported.
        let mut stranger = FakePhone::connect(link.port, &[1u8; KEY_LEN]);
        // (The island may hang up before the second message is even sent:
        // on Windows that write then fails, which is the point.)
        let _ = stranger.try_say(r#"{"t":"hello","name":"Mallory"}"#);
        let _ = stranger.try_say(r#"{"t":"note","app":"x","title":"fake","text":"fake"}"#);
        let mut end = [0u8; 1];
        assert!(matches!(stranger.stream.read(&mut end), Ok(0) | Err(_)), "the connection is dropped");

        let mut phone = FakePhone::connect(link.port, &key);
        phone.say(r#"{"t":"hello","name":"Pixel 8"}"#);
        assert_eq!(phone.hear(), ToPhone::Ok { name: "Test laptop".into() });
        phone.say(r#"{"t":"battery","level":82,"charging":true}"#);
        let Report::DeviceConnected(d) = next(&|r| matches!(r, Report::DeviceConnected(_))) else { unreachable!() };
        assert_eq!((d.name.as_str(), d.kind, d.percent, d.charging), ("Pixel 8", DeviceKind::Phone, 82, true));
        phone.say(r#"{"t":"note","app":"Messages","title":"Amina","text":"See you at 5"}"#);
        let Report::Notification(n) = next(&|r| matches!(r, Report::Notification(_))) else { unreachable!() };
        assert_eq!(
            (n.app.as_str(), n.summary.as_str(), n.body.as_str()),
            ("Messages · Pixel 8", "Amina", "See you at 5")
        );
        phone.say(r#"{"t":"media","title":"Song","artist":"Band","app":"Spotify","playing":true,"duration":200,"position":50}"#);
        let Report::RemoteMedia(Some(m)) = next(&|r| matches!(r, Report::RemoteMedia(Some(_)))) else { unreachable!() };
        assert_eq!((m.source.as_str(), m.app_name.as_str(), m.title.as_str()), (SOURCE, "Spotify on Pixel 8", "Song"));
        assert_eq!((m.playing, m.duration, m.position), (true, Some(200.0), Some(50.0)));

        // The island's buttons reach the phone; a ping is answered.
        link.command(MediaCommand::Next);
        assert_eq!(phone.hear(), ToPhone::Command { r#do: "next".into() });
        phone.say(r#"{"t":"ping"}"#);
        assert_eq!(phone.hear(), ToPhone::Pong);
        // A message from a newer app version is skipped, not fatal.
        phone.say(r#"{"t":"something_new","x":1}"#);
        phone.say(r#"{"t":"media"}"#);
        assert!(matches!(next(&|r| matches!(r, Report::RemoteMedia(None))), Report::RemoteMedia(None)));

        // Hanging up takes the phone's battery and music off the island.
        drop(phone);
        assert!(matches!(next(&|r| matches!(r, Report::DeviceGone(_))), Report::DeviceGone(id) if id == DEVICE_ID));
    }

    #[test]
    fn finds_an_address_without_sending_anything() {
        // A machine with no network at all has none: that is fine too.
        for a in local_addresses() {
            assert!(a.parse::<std::net::IpAddr>().is_ok(), "{a}");
        }
    }
}
