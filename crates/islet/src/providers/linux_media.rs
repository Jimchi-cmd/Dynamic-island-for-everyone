//! Linux: MPRIS media sessions over D-Bus (Spotify, browsers, VLC, mpv…).
//!
//! Event-driven: we subscribe to `PropertiesChanged` and `Seeked` from
//! players and to `NameOwnerChanged` for players appearing and leaving.
//! Nothing is polled. Album art is read only from local `file://` URLs the
//! player provides; nothing is downloaded.

use super::{art, dbus_util::*, Bus, Sink};
use islet_core::activity::{ArtImage, MediaCaps, MediaCommand, MediaInfo, Report};
use std::collections::HashMap;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use zbus::blocking::{fdo::DBusProxy, Connection, MessageIterator};
use zbus::message::Type as MsgType;
use zbus::zvariant::{ObjectPath, OwnedValue};
use zbus::MatchRule;

const PREFIX: &str = "org.mpris.MediaPlayer2.";
const PATH: &str = "/org/mpris/MediaPlayer2";
const ROOT_IFACE: &str = "org.mpris.MediaPlayer2";
const PLAYER_IFACE: &str = "org.mpris.MediaPlayer2.Player";

#[derive(Debug, Clone, Default)]
struct Player {
    bus_name: String,
    identity: String,
    status: String,
    title: String,
    artist: String,
    album: String,
    art_url: String,
    /// xesam:url, the playing file or stream.
    url: String,
    track_id: Option<String>,
    length_us: Option<i64>,
    /// Position and when it was read (for extrapolation while playing).
    position_us: Option<i64>,
    position_at: Option<Instant>,
    rate: f64,
    caps: MediaCaps,
    /// When the player last started playing: the most recent wins.
    playing_since: Option<Instant>,
    touched: Option<Instant>,
}

impl Player {
    fn playing(&self) -> bool {
        self.status == "Playing"
    }

    fn apply(&mut self, props: &HashMap<String, OwnedValue>) {
        let now = Instant::now();
        self.touched = Some(now);
        if let Some(s) = props.get("PlaybackStatus").and_then(|v| str_of(v)) {
            if s == "Playing" && !self.playing() {
                self.playing_since = Some(now);
            }
            self.status = s;
        }
        if let Some(r) = props.get("Rate").and_then(|v| f64_of(v)) {
            self.rate = r;
        }
        if let Some(p) = props.get("Position").and_then(|v| i64_of(v)) {
            self.position_us = Some(p);
            self.position_at = Some(now);
        }
        let flag = |k: &str| props.get(k).and_then(|v| bool_of(v));
        if let Some(b) = flag("CanGoNext") {
            self.caps.next = b;
        }
        if let Some(b) = flag("CanGoPrevious") {
            self.caps.previous = b;
        }
        if let Some(b) = flag("CanSeek") {
            self.caps.seek = b;
        }
        if let Some(b) = flag("CanPlay").or(flag("CanPause")).or(flag("CanControl")) {
            self.caps.play_pause = b;
        }
        if let Some(meta) = props.get("Metadata").map(|v| dict_of(v)) {
            let get = |k: &str| meta.get(k);
            let old_track = self.track_id.clone();
            self.title = get("xesam:title").and_then(str_of).unwrap_or_default();
            self.artist = get("xesam:artist").map(str_list_of).unwrap_or_default().join(", ");
            self.album = get("xesam:album").and_then(str_of).unwrap_or_default();
            self.art_url = get("mpris:artUrl").and_then(str_of).unwrap_or_default();
            self.url = get("xesam:url").and_then(str_of).unwrap_or_default();
            self.length_us = get("mpris:length").and_then(i64_of).filter(|l| *l > 0);
            self.track_id = get("mpris:trackid").and_then(str_of);
            if self.track_id != old_track && !props.contains_key("Position") {
                // A new track starts from the beginning unless told otherwise.
                self.position_us = Some(0);
                self.position_at = Some(now);
            }
        }
    }

    fn position_now(&self) -> Option<f64> {
        let p = self.position_us? as f64 / 1e6;
        let elapsed = self.position_at.map_or(0.0, |at| at.elapsed().as_secs_f64());
        Some(if self.playing() { p + elapsed * self.rate.max(0.0) } else { p })
    }
}

#[derive(Default)]
struct State {
    /// Keyed by unique bus name (":1.42"), which is what signals carry.
    players: HashMap<String, Player>,
    art: Option<(String, Option<ArtImage>)>,
}

impl State {
    /// The session to show: the one that most recently started playing,
    /// else the most recently active one.
    fn active(&self) -> Option<(&String, &Player)> {
        self.players
            .iter()
            .filter(|(_, p)| p.status != "Stopped" || !p.title.is_empty())
            .max_by_key(|(_, p)| (p.playing(), p.playing_since, p.touched))
    }

    fn art_for(&mut self, url: &str) -> Option<ArtImage> {
        if let Some((cached, img)) = &self.art {
            if cached == url {
                return img.clone();
            }
        }
        let bytes = match local_path(url) {
            Some(p) => std::fs::read(p).ok(),
            // Web artwork only when the user turned it on.
            None if url.starts_with("https://") && super::WEB_ARTWORK.load(std::sync::atomic::Ordering::Relaxed) => {
                crate::net::fetch_bytes(url, 3 * 1024 * 1024).ok()
            }
            None => None,
        };
        let img = bytes.and_then(|b| art::decode(&b));
        self.art = Some((url.to_string(), img.clone()));
        img
    }

    fn report(&mut self) -> Report {
        let Some((_, p)) = self.active() else { return Report::Media(None) };
        let p = p.clone();
        let art = if p.art_url.is_empty() { None } else { self.art_for(&p.art_url) };
        Report::Media(Some(MediaInfo {
            source: p.bus_name.clone(),
            app_name: if p.identity.is_empty() {
                p.bus_name.trim_start_matches(PREFIX).to_string()
            } else {
                p.identity.clone()
            },
            title: p.title.clone(),
            artist: p.artist.clone(),
            album: p.album.clone(),
            playing: p.playing(),
            position: p.position_now(),
            position_at: 0.0,
            duration: p.length_us.map(|l| l as f64 / 1e6),
            rate: if p.rate > 0.0 { p.rate } else { 1.0 },
            art,
            caps: p.caps,
            file: local_path(&p.url),
        }))
    }
}

/// `file:///home/me/a%20b.jpg` → `/home/me/a b.jpg`; other schemes → None.
pub fn local_path(url: &str) -> Option<String> {
    let rest = url.strip_prefix("file://")?;
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?, 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).ok()
}

fn get_all(conn: &Connection, dest: &str, iface: &str) -> zbus::Result<HashMap<String, OwnedValue>> {
    let reply = conn.call_method(Some(dest), PATH, Some("org.freedesktop.DBus.Properties"), "GetAll", &(iface,))?;
    reply.body().deserialize()
}

fn refresh(conn: &Connection, state: &Mutex<State>, unique: &str, name: &str) {
    let Ok(props) = get_all(conn, unique, PLAYER_IFACE) else { return };
    let identity = get_all(conn, unique, ROOT_IFACE)
        .ok()
        .and_then(|root| root.get("Identity").and_then(|v| str_of(v)))
        .unwrap_or_default();
    let mut st = state.lock().unwrap();
    let p = st.players.entry(unique.to_string()).or_default();
    p.bus_name = name.to_string();
    p.identity = identity;
    p.apply(&props);
}

fn rules() -> zbus::Result<Vec<MatchRule<'static>>> {
    Ok(vec![
        MatchRule::builder()
            .msg_type(MsgType::Signal)
            .interface("org.freedesktop.DBus.Properties")?
            .member("PropertiesChanged")?
            .path(PATH)?
            .build(),
        MatchRule::builder().msg_type(MsgType::Signal).interface(PLAYER_IFACE)?.member("Seeked")?.path(PATH)?.build(),
        MatchRule::builder()
            .msg_type(MsgType::Signal)
            .sender("org.freedesktop.DBus")?
            .interface("org.freedesktop.DBus")?
            .member("NameOwnerChanged")?
            .arg0ns("org.mpris.MediaPlayer2")?
            .build(),
    ])
}

fn listen(conn: Connection, state: Arc<Mutex<State>>, sink: Sink) -> zbus::Result<()> {
    let dbus = DBusProxy::new(&conn)?;
    for rule in rules()? {
        dbus.add_match_rule(rule)?;
    }
    let messages = MessageIterator::from(&conn);
    for name in dbus.list_names()? {
        if name.as_str().starts_with(PREFIX) {
            if let Ok(owner) = dbus.get_name_owner(name.as_ref()) {
                refresh(&conn, &state, owner.as_str(), name.as_str());
            }
        }
    }
    sink.send(state.lock().unwrap().report());

    for msg in messages {
        let Ok(msg) = msg else { continue };
        let header = msg.header();
        if header.message_type() != MsgType::Signal {
            continue;
        }
        let sender = header.sender().map(|s| s.to_string()).unwrap_or_default();
        let member = header.member().map(|m| m.as_str().to_string()).unwrap_or_default();
        let changed = match member.as_str() {
            "PropertiesChanged" => {
                let Ok((iface, props, _)) =
                    msg.body().deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>()
                else {
                    continue;
                };
                if iface != PLAYER_IFACE {
                    continue;
                }
                let known = state.lock().unwrap().players.contains_key(&sender);
                if !known {
                    continue; // not an MPRIS player we track
                }
                if let Some(p) = state.lock().unwrap().players.get_mut(&sender) {
                    p.apply(&props);
                }
                if props.contains_key("PlaybackStatus") {
                    // Position is never signalled; resync it on play/pause.
                    if let Ok(all) = get_all(&conn, &sender, PLAYER_IFACE) {
                        if let Some(p) = state.lock().unwrap().players.get_mut(&sender) {
                            p.apply(&all);
                        }
                    }
                }
                true
            }
            "Seeked" => {
                let Ok(pos) = msg.body().deserialize::<i64>() else { continue };
                let mut st = state.lock().unwrap();
                match st.players.get_mut(&sender) {
                    Some(p) => {
                        p.position_us = Some(pos);
                        p.position_at = Some(Instant::now());
                        true
                    }
                    None => false,
                }
            }
            "NameOwnerChanged" => {
                let Ok((name, old, new)) = msg.body().deserialize::<(String, String, String)>() else { continue };
                if !name.starts_with(PREFIX) {
                    continue;
                }
                if !old.is_empty() {
                    state.lock().unwrap().players.remove(&old);
                }
                if !new.is_empty() {
                    refresh(&conn, &state, &new, &name);
                }
                true
            }
            _ => false,
        };
        if changed {
            sink.send(state.lock().unwrap().report());
        }
    }
    Ok(())
}

fn command(conn: &Connection, state: &Mutex<State>, cmd: MediaCommand) -> zbus::Result<()> {
    let (dest, track, pos) = {
        let st = state.lock().unwrap();
        let Some((unique, p)) = st.active() else { return Ok(()) };
        (unique.clone(), p.track_id.clone(), p.position_now())
    };
    let call = |method: &str| conn.call_method(Some(dest.as_str()), PATH, Some(PLAYER_IFACE), method, &()).map(|_| ());
    match cmd {
        MediaCommand::PlayPause => call("PlayPause"),
        MediaCommand::Next => call("Next"),
        MediaCommand::Previous => call("Previous"),
        MediaCommand::SeekTo(secs) => {
            let target = (secs * 1e6) as i64;
            match track.as_deref().and_then(|t| ObjectPath::try_from(t).ok()) {
                Some(track) => conn
                    .call_method(Some(dest.as_str()), PATH, Some(PLAYER_IFACE), "SetPosition", &(track, target))
                    .map(|_| ()),
                None => {
                    let offset = target - (pos.unwrap_or(0.0) * 1e6) as i64;
                    conn.call_method(Some(dest.as_str()), PATH, Some(PLAYER_IFACE), "Seek", &(offset,)).map(|_| ())
                }
            }
        }
    }
}

/// Start the MPRIS provider. Returns the command channel, or None if the
/// session bus is unavailable (then the island simply shows no media).
pub fn start(sink: Sink, bus: Bus) -> Option<Sender<MediaCommand>> {
    let conn = match bus.connect() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("islet: media sessions unavailable (D-Bus: {e})");
            return None;
        }
    };
    let state = Arc::new(Mutex::new(State::default()));
    let (tx, rx) = channel::<MediaCommand>();
    {
        let (conn, state) = (conn.clone(), state.clone());
        std::thread::Builder::new()
            .name("islet-mpris".into())
            .spawn(move || {
                if let Err(e) = listen(conn, state, sink) {
                    eprintln!("islet: MPRIS listener stopped: {e}");
                }
            })
            .ok()?;
    }
    std::thread::Builder::new()
        .name("islet-mpris-cmd".into())
        .spawn(move || {
            for cmd in rx {
                if let Err(e) = command(&conn, &state, cmd) {
                    eprintln!("islet: media command failed: {e}");
                }
            }
        })
        .ok()?;
    Some(tx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_urls_become_paths() {
        assert_eq!(local_path("file:///home/me/a%20b.jpg").as_deref(), Some("/home/me/a b.jpg"));
        assert_eq!(local_path("https://example.com/a.jpg"), None);
        assert_eq!(local_path("file:///bad%zz").as_deref(), Some("/bad%zz"));
    }
}
