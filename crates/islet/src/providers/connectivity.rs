//! Wi-Fi and Bluetooth: list networks and paired devices, connect to one.
//!
//! A worker thread blocks on a channel and only does anything when the
//! user opens a list or picks an entry, so an idle island makes no
//! wakeups for it. Each OS backend implements [`Backend`].

use super::Sink;
use islet_core::activity::Report;
use islet_core::connectivity::{BtState, ConnCommand, ConnUpdate, Radio, WifiState};
use std::sync::mpsc::{channel, Sender};

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectError {
    /// A secured network that isn't saved yet: ask for its password.
    NeedsPassword,
    Failed(String),
}

pub trait Backend {
    /// Wi-Fi state; `scan` asks the OS for a fresh scan first (slower).
    fn wifi(&mut self, scan: bool) -> WifiState;
    fn wifi_connect(&mut self, ssid: &str, password: Option<&str>) -> Result<(), ConnectError>;
    /// Paired Bluetooth devices.
    fn bluetooth(&mut self) -> BtState;
    fn bt_set(&mut self, id: &str, connect: bool) -> Result<(), String>;
    /// Turn the Wi-Fi or Bluetooth radio on or off.
    fn set_power(&mut self, radio: Radio, on: bool) -> Result<(), String>;
}

#[cfg(target_os = "linux")]
pub fn backend() -> Box<dyn Backend> {
    Box::new(super::linux_conn::Linux::new(super::Bus::System))
}

#[cfg(target_os = "windows")]
pub fn backend() -> Box<dyn Backend> {
    Box::new(super::windows_conn::Windows)
}

#[cfg(target_os = "macos")]
pub fn backend() -> Box<dyn Backend> {
    Box::new(super::macos_conn::Mac)
}

/// How long a radio is given to settle after being switched on or off.
#[cfg(not(test))]
const SETTLE: std::time::Duration = std::time::Duration::from_millis(1500);
#[cfg(test)]
const SETTLE: std::time::Duration = std::time::Duration::from_millis(1);

/// Run one command against a backend, reporting progress through `send`.
pub fn run(b: &mut dyn Backend, cmd: ConnCommand, send: &mut dyn FnMut(ConnUpdate)) {
    match cmd {
        ConnCommand::Status => {
            send(ConnUpdate::Wifi(b.wifi(false)));
            send(ConnUpdate::Bluetooth(b.bluetooth()));
        }
        ConnCommand::ScanWifi => {
            // What the OS already knows first, then a fresh scan.
            send(ConnUpdate::Wifi(b.wifi(false)));
            send(ConnUpdate::Busy(Some("Searching…".into())));
            let w = b.wifi(true);
            send(ConnUpdate::Wifi(w));
            send(ConnUpdate::Busy(None));
        }
        ConnCommand::ListBluetooth => send(ConnUpdate::Bluetooth(b.bluetooth())),
        ConnCommand::ConnectWifi { ssid, password } => {
            send(ConnUpdate::Busy(Some(format!("Connecting to {ssid}…"))));
            match b.wifi_connect(&ssid, password.as_deref()) {
                Ok(()) => send(ConnUpdate::Busy(None)),
                Err(ConnectError::NeedsPassword) => send(ConnUpdate::Failed {
                    message: format!("{ssid} needs a password"),
                    needs_password: Some(ssid.clone()),
                }),
                Err(ConnectError::Failed(message)) => send(ConnUpdate::Failed { message, needs_password: None }),
            }
            send(ConnUpdate::Wifi(b.wifi(false)));
        }
        ConnCommand::ConnectBluetooth(_) | ConnCommand::DisconnectBluetooth(_) if !b.bluetooth().can_connect => {
            send(ConnUpdate::Failed { message: "Connect it in Bluetooth settings".into(), needs_password: None });
        }
        ConnCommand::ConnectBluetooth(ref id) | ConnCommand::DisconnectBluetooth(ref id) => {
            let connect = matches!(cmd, ConnCommand::ConnectBluetooth(_));
            let id = id.clone();
            let name = b.bluetooth().devices.into_iter().find(|d| d.id == id).map(|d| d.name).unwrap_or_default();
            send(ConnUpdate::Busy(Some(if connect {
                format!("Connecting {name}…")
            } else {
                format!("Disconnecting {name}…")
            })));
            match b.bt_set(&id, connect) {
                Ok(()) => send(ConnUpdate::Busy(None)),
                Err(message) => send(ConnUpdate::Failed { message, needs_password: None }),
            }
            send(ConnUpdate::Bluetooth(b.bluetooth()));
        }
        ConnCommand::SetPower(radio, on) => {
            send(ConnUpdate::Busy(Some(format!("Turning {} {}…", radio.name(), if on { "on" } else { "off" }))));
            match b.set_power(radio, on) {
                Ok(()) => send(ConnUpdate::Busy(None)),
                Err(message) => send(ConnUpdate::Failed { message, needs_password: None }),
            }
            let mut read = |b: &mut dyn Backend| match radio {
                Radio::Wifi => send(ConnUpdate::Wifi(b.wifi(false))),
                Radio::Bluetooth => send(ConnUpdate::Bluetooth(b.bluetooth())),
            };
            read(b);
            // A radio takes a moment to settle (Windows may still name the
            // old network): read it again once it has.
            std::thread::sleep(SETTLE);
            read(b);
        }
    }
}

pub fn start(sink: Sink) -> Sender<ConnCommand> {
    let (tx, rx) = channel::<ConnCommand>();
    let _ = std::thread::Builder::new().name("islet-connectivity".into()).spawn(move || {
        let mut b = backend();
        for cmd in rx {
            run(b.as_mut(), cmd, &mut |u| sink.send(Report::Connectivity(u)));
        }
    });
    tx
}

#[cfg(test)]
mod tests {
    use super::*;
    use islet_core::activity::DeviceKind;
    use islet_core::connectivity::{BtDevice, WifiNetwork};

    #[derive(Default)]
    struct Fake {
        saved: Vec<String>,
        current: Option<String>,
        connected: bool,
        can_connect: bool,
        wifi_off: bool,
    }

    impl Backend for Fake {
        fn wifi(&mut self, _scan: bool) -> WifiState {
            let net = |ssid: &str| WifiNetwork {
                ssid: ssid.into(),
                strength: 80,
                secure: true,
                known: self.saved.iter().any(|s| s == ssid),
                active: self.current.as_deref() == Some(ssid),
            };
            WifiState {
                available: true,
                enabled: !self.wifi_off,
                current: self.current.clone(),
                networks: vec![net("Home"), net("Cafe")],
                note: None,
            }
        }
        fn wifi_connect(&mut self, ssid: &str, password: Option<&str>) -> Result<(), ConnectError> {
            if !self.saved.iter().any(|s| s == ssid) && password.is_none() {
                return Err(ConnectError::NeedsPassword);
            }
            self.current = Some(ssid.into());
            Ok(())
        }
        fn bluetooth(&mut self) -> BtState {
            BtState {
                available: true,
                powered: true,
                devices: vec![BtDevice {
                    id: "b1".into(),
                    name: "Buds".into(),
                    kind: DeviceKind::Earbuds,
                    connected: self.connected,
                }],
                can_connect: self.can_connect,
                note: None,
            }
        }
        fn bt_set(&mut self, _id: &str, connect: bool) -> Result<(), String> {
            self.connected = connect;
            Ok(())
        }
        fn set_power(&mut self, radio: Radio, on: bool) -> Result<(), String> {
            match radio {
                Radio::Wifi => {
                    self.wifi_off = !on;
                    Ok(())
                }
                Radio::Bluetooth => Err("Turn Bluetooth on or off in System Settings".into()),
            }
        }
    }

    #[test]
    fn radios_turn_on_and_off() {
        let mut b = Fake::default();
        let u = collect(&mut b, ConnCommand::SetPower(Radio::Wifi, false));
        assert!(
            matches!(&u[..], [ConnUpdate::Busy(Some(m)), ConnUpdate::Busy(None), ConnUpdate::Wifi(w), ConnUpdate::Wifi(again)]
                if m == "Turning Wi-Fi off…" && !w.enabled && !again.enabled),
            "read at once, then again once the radio has settled: {u:?}"
        );
        let u = collect(&mut b, ConnCommand::SetPower(Radio::Bluetooth, true));
        assert!(matches!(&u[1], ConnUpdate::Failed { .. }) && matches!(u.last(), Some(ConnUpdate::Bluetooth(_))));
    }

    fn collect(b: &mut Fake, cmd: ConnCommand) -> Vec<ConnUpdate> {
        let mut out = Vec::new();
        run(b, cmd, &mut |u| out.push(u));
        out
    }

    #[test]
    fn saved_networks_connect_and_new_ones_ask_for_a_password() {
        let mut b = Fake { saved: vec!["Home".into()], ..Fake::default() };
        let u = collect(&mut b, ConnCommand::ConnectWifi { ssid: "Home".into(), password: None });
        assert!(
            matches!(&u[..], [ConnUpdate::Busy(Some(_)), ConnUpdate::Busy(None), ConnUpdate::Wifi(w)] if w.current.as_deref() == Some("Home"))
        );
        let u = collect(&mut b, ConnCommand::ConnectWifi { ssid: "Cafe".into(), password: None });
        assert!(u.iter().any(|x| matches!(x, ConnUpdate::Failed { needs_password: Some(s), .. } if s == "Cafe")));
        let u = collect(&mut b, ConnCommand::ConnectWifi { ssid: "Cafe".into(), password: Some("secret".into()) });
        assert!(matches!(u.last(), Some(ConnUpdate::Wifi(w)) if w.current.as_deref() == Some("Cafe")));
    }

    #[test]
    fn bluetooth_toggles_or_hands_off_to_settings() {
        let mut b = Fake { can_connect: true, ..Fake::default() };
        let u = collect(&mut b, ConnCommand::ConnectBluetooth("b1".into()));
        assert!(matches!(u.last(), Some(ConnUpdate::Bluetooth(s)) if s.devices[0].connected));
        let u = collect(&mut b, ConnCommand::DisconnectBluetooth("b1".into()));
        assert!(matches!(u.last(), Some(ConnUpdate::Bluetooth(s)) if !s.devices[0].connected));
        let mut windows_like = Fake::default();
        let u = collect(&mut windows_like, ConnCommand::ConnectBluetooth("b1".into()));
        assert!(matches!(&u[..], [ConnUpdate::Failed { .. }]));
    }
}
