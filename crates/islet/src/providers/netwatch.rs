//! Notices when the computer's network changes (Wi-Fi joins or leaves a
//! network) and asks the connectivity thread to read the Wi-Fi state, so
//! the island can say "Connected to Home". Each system tells us itself:
//! NetworkManager's signals on Linux, the network-status event on Windows,
//! and the system's network-change notification on macOS. Nothing polls.

use islet_core::connectivity::ConnCommand;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

/// A change usually comes in a burst (address, route, DNS): read the state
/// once things have been quiet this long.
const SETTLE: Duration = Duration::from_secs(2);

/// Read the state now (the baseline: no pop-up for it), then after each
/// burst of changes.
fn relay(changes: &Receiver<()>, conn: &Sender<ConnCommand>) {
    if conn.send(ConnCommand::Status).is_err() {
        return;
    }
    while changes.recv().is_ok() {
        loop {
            match changes.recv_timeout(SETTLE) {
                Ok(()) => continue,
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
        if conn.send(ConnCommand::Status).is_err() {
            return;
        }
    }
}

pub fn start(conn: Sender<ConnCommand>) {
    let (tx, rx) = channel::<()>();
    let _ = std::thread::Builder::new().name("islet-netwatch".into()).spawn(move || relay(&rx, &conn));
    if let Err(e) = watch(tx) {
        super::debug_log(&format!("network changes: not watched ({e})"));
    }
}

#[cfg(target_os = "linux")]
fn watch(tx: Sender<()>) -> Result<(), String> {
    use zbus::blocking::{fdo::DBusProxy, MessageIterator};
    use zbus::message::Type as MsgType;
    use zbus::MatchRule;
    std::thread::Builder::new()
        .name("islet-netwatch-nm".into())
        .spawn(move || {
            let run = || -> zbus::Result<()> {
                let conn = super::Bus::System.connect()?;
                let rule = MatchRule::builder()
                    .msg_type(MsgType::Signal)
                    .path("/org/freedesktop/NetworkManager")?
                    .interface("org.freedesktop.DBus.Properties")?
                    .member("PropertiesChanged")?
                    .build();
                DBusProxy::new(&conn)?.add_match_rule(rule)?;
                for msg in MessageIterator::from(&conn) {
                    let Ok(msg) = msg else { continue };
                    let Ok((iface, changed, _)) = msg.body().deserialize::<(
                        String,
                        std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
                        Vec<String>,
                    )>() else {
                        continue;
                    };
                    let relevant = ["PrimaryConnection", "State", "ActiveConnections"];
                    if iface == "org.freedesktop.NetworkManager"
                        && changed.keys().any(|k| relevant.contains(&k.as_str()))
                        && tx.send(()).is_err()
                    {
                        break;
                    }
                }
                Ok(())
            };
            if let Err(e) = run() {
                super::debug_log(&format!("network changes: NetworkManager unavailable ({e})"));
            }
        })
        .map(drop)
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "windows")]
fn watch(tx: Sender<()>) -> Result<(), String> {
    use windows::Networking::Connectivity::{NetworkInformation, NetworkStatusChangedEventHandler};
    let tx = std::sync::Mutex::new(tx);
    let handler = NetworkStatusChangedEventHandler::new(move |_| {
        if let Ok(tx) = tx.lock() {
            let _ = tx.send(());
        }
        Ok(())
    });
    // The subscription lasts as long as the island: its token is never removed.
    NetworkInformation::NetworkStatusChanged(&handler).map(drop).map_err(|e| e.message().to_string())
}

#[cfg(target_os = "macos")]
fn watch(tx: Sender<()>) -> Result<(), String> {
    use std::os::raw::{c_char, c_int};
    extern "C" {
        // libSystem's notify(3): a file descriptor that becomes readable
        // each time the named notification is posted.
        fn notify_register_file_descriptor(
            name: *const c_char,
            notify_fd: *mut c_int,
            flags: c_int,
            out_token: *mut c_int,
        ) -> u32;
    }
    let name = c"com.apple.system.config.network_change";
    let (mut fd, mut token) = (-1 as c_int, 0 as c_int);
    // SAFETY: a valid C string and out-pointers to locals.
    let status = unsafe { notify_register_file_descriptor(name.as_ptr(), &mut fd, 0, &mut token) };
    if status != 0 || fd < 0 {
        return Err(format!("notify_register_file_descriptor failed ({status})"));
    }
    std::thread::Builder::new()
        .name("islet-netwatch-mac".into())
        .spawn(move || {
            let mut buf = [0u8; 4];
            loop {
                // SAFETY: reading into a local buffer from our own descriptor.
                let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
                if n <= 0 || tx.send(()).is_err() {
                    break;
                }
            }
        })
        .map(drop)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_once_at_start_then_once_per_burst() {
        let (changes_tx, changes) = channel();
        let (conn, commands) = channel();
        std::thread::spawn(move || relay(&changes, &conn));
        assert_eq!(commands.recv_timeout(Duration::from_secs(1)).unwrap(), ConnCommand::Status, "the baseline");
        for _ in 0..5 {
            changes_tx.send(()).unwrap();
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(commands.recv_timeout(Duration::from_millis(1500)).is_err(), "not while it is still changing");
        assert_eq!(
            commands.recv_timeout(Duration::from_secs(3)).unwrap(),
            ConnCommand::Status,
            "once, after it settles"
        );
        assert!(commands.recv_timeout(Duration::from_millis(2500)).is_err(), "and only once");
    }
}
