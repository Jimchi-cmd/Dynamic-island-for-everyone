//! A local control channel so `islet nav start …` reaches the running
//! island. It listens on 127.0.0.1 only, on a random port, and every
//! request must carry a random token stored in a file only the user can
//! read (config dir / control.json).

use crate::config;
use crate::platform::Waker;
use islet_core::nav::Trip;
use islet_core::timer::TimerCommand;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::Sender;

#[derive(Debug, Clone, PartialEq)]
pub enum Control {
    NavStart(Trip),
    NavStop,
    /// Quit the island (Settings restarts it after an update).
    Quit,
    /// alarms.json changed: reload it.
    AlarmsChanged,
    AlarmStop,
    AlarmSnooze,
    Timer(TimerCommand),
    /// island.json changed: reload it.
    PrefsChanged,
    /// shelf.json changed: reload it.
    ShelfChanged,
    /// A script's live activity started or changed (`islet activity`)…
    Activity(islet_core::activity::Custom),
    /// …or ended.
    ActivityEnd(String),
    /// Is an island running? Answered by the control thread itself.
    Ping,
}

fn token() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    // Two independently seeded SipHash outputs (seeded from OS randomness).
    let mut out = String::new();
    for _ in 0..2 {
        let mut h = RandomState::new().build_hasher();
        h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
        h.write_u32(std::process::id());
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}

/// Start listening; requests are forwarded to `tx` and wake the loop.
pub fn serve(tx: Sender<Control>, waker: Waker) {
    let Ok(listener) = TcpListener::bind("127.0.0.1:0") else { return };
    let Ok(port) = listener.local_addr().map(|a| a.port()) else { return };
    let token = token();
    let Some(path) = config::control_file() else { return };
    let info = serde_json::json!({ "port": port, "token": token, "pid": std::process::id() });
    if config::write_private(&path, &info.to_string()).is_err() {
        return;
    }
    let _ = std::thread::Builder::new().name("islet-control".into()).spawn(move || {
        for stream in listener.incoming().flatten() {
            let reply = match handle(&stream, &token) {
                Ok(Control::Ping) => "ok".to_string(),
                Ok(c) => {
                    let ok = tx.send(c).is_ok();
                    waker.wake();
                    if ok {
                        "ok".to_string()
                    } else {
                        "error island is shutting down".into()
                    }
                }
                Err(e) => format!("error {e}"),
            };
            let _ = writeln!(&stream, "{reply}");
        }
    });
}

fn handle(stream: &TcpStream, token: &str) -> Result<Control, String> {
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).map_err(|e| e.to_string())?;
    let mut parts = line.trim_end().splitn(3, '\t');
    if parts.next() != Some(token) {
        return Err("bad token".into());
    }
    match (parts.next(), parts.next()) {
        (Some("nav-start"), Some(json)) => serde_json::from_str(json).map(Control::NavStart).map_err(|e| e.to_string()),
        (Some("nav-stop"), _) => Ok(Control::NavStop),
        (Some("quit"), _) => Ok(Control::Quit),
        (Some("alarms-changed"), _) => Ok(Control::AlarmsChanged),
        (Some("alarm-stop"), _) => Ok(Control::AlarmStop),
        (Some("alarm-snooze"), _) => Ok(Control::AlarmSnooze),
        (Some("prefs-changed"), _) => Ok(Control::PrefsChanged),
        (Some("shelf-changed"), _) => Ok(Control::ShelfChanged),
        (Some("activity"), Some(json)) => serde_json::from_str(json).map(Control::Activity).map_err(|e| e.to_string()),
        (Some("activity-end"), Some(id)) => Ok(Control::ActivityEnd(id.to_string())),
        (Some("timer"), Some(json)) => serde_json::from_str(json).map(Control::Timer).map_err(|e| e.to_string()),
        (Some("ping"), _) => Ok(Control::Ping),
        _ => Err("unknown command".into()),
    }
}

/// Send a command to the running island. Err if none is running.
pub fn send(cmd: &Control) -> Result<(), String> {
    let path = config::control_file().ok_or("no config directory")?;
    let info: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).map_err(|_| "Islet is not running")?).map_err(|e| e.to_string())?;
    let port = info["port"].as_u64().ok_or("bad control file")? as u16;
    let token = info["token"].as_str().ok_or("bad control file")?;
    // A loopback connect either succeeds at once or nobody is listening (a
    // stale file from an earlier run). Windows retries a refused loopback
    // connect for about two seconds, which would delay every start after a
    // crash or shutdown, so don't wait for that.
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream =
        TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(250)).map_err(|_| "Islet is not running")?;
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(10)));
    let line = match cmd {
        Control::NavStart(trip) => {
            format!("{token}\tnav-start\t{}", serde_json::to_string(trip).map_err(|e| e.to_string())?)
        }
        Control::NavStop => format!("{token}\tnav-stop"),
        Control::Quit => format!("{token}\tquit"),
        Control::AlarmsChanged => format!("{token}\talarms-changed"),
        Control::AlarmStop => format!("{token}\talarm-stop"),
        Control::AlarmSnooze => format!("{token}\talarm-snooze"),
        Control::PrefsChanged => format!("{token}\tprefs-changed"),
        Control::ShelfChanged => format!("{token}\tshelf-changed"),
        Control::Activity(c) => format!("{token}\tactivity\t{}", serde_json::to_string(c).map_err(|e| e.to_string())?),
        Control::ActivityEnd(id) => format!("{token}\tactivity-end\t{id}"),
        Control::Timer(cmd) => format!("{token}\ttimer\t{}", serde_json::to_string(cmd).map_err(|e| e.to_string())?),
        Control::Ping => format!("{token}\tping"),
    };
    writeln!(stream, "{line}").map_err(|e| e.to_string())?;
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply).map_err(|e| e.to_string())?;
    match reply.trim() {
        "ok" => Ok(()),
        other => Err(other.trim_start_matches("error ").to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use islet_core::nav::TravelMode;

    #[test]
    fn round_trip_through_the_control_channel() {
        let dir = std::env::temp_dir().join(format!("islet-control-test-{}", std::process::id()));
        std::env::set_var("ISLET_CONFIG_DIR", &dir);
        let (tx, rx) = std::sync::mpsc::channel();
        serve(tx, Waker::new(|| {}));
        let trip = Trip {
            label: "Work".into(),
            destination: "Kampala Road".into(),
            origin: "Ntinda".into(),
            mode: TravelMode::Drive,
        };
        send(&Control::NavStart(trip.clone())).unwrap();
        assert_eq!(rx.recv().unwrap(), Control::NavStart(trip));
        send(&Control::NavStop).unwrap();
        assert_eq!(rx.recv().unwrap(), Control::NavStop);
        send(&Control::Ping).unwrap();
        assert!(rx.try_recv().is_err(), "pings are answered without waking the island");
        let _ = std::fs::remove_dir_all(dir);
    }
}
