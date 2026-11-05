//! Wi-Fi networks and Bluetooth devices the user can pick from the island.
//!
//! Lists are read on demand (when the user opens them), never polled, so an
//! idle island makes no wakeups for them. Pure model and ordering here; the
//! OS backends live in the island crate.

use crate::activity::DeviceKind;

#[derive(Debug, Clone, PartialEq)]
pub struct WifiNetwork {
    pub ssid: String,
    /// Signal 0..=100.
    pub strength: u8,
    /// Needs a password (WPA/WEP).
    pub secure: bool,
    /// Saved on this computer: connects without asking for a password.
    pub known: bool,
    /// Connected now.
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct WifiState {
    /// This computer has Wi-Fi Islet can use.
    pub available: bool,
    pub enabled: bool,
    pub current: Option<String>,
    pub networks: Vec<WifiNetwork>,
    /// Something the user should know ("Allow Location so Islet can see
    /// network names").
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BtDevice {
    pub id: String,
    pub name: String,
    pub kind: DeviceKind,
    pub connected: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BtState {
    pub available: bool,
    pub powered: bool,
    /// Paired devices.
    pub devices: Vec<BtDevice>,
    /// The OS lets Islet connect and disconnect devices (Windows doesn't:
    /// Islet opens Bluetooth settings there).
    pub can_connect: bool,
    pub note: Option<String>,
}

/// Requests from the island (or the command line) to the backend.
#[derive(Debug, Clone, PartialEq)]
pub enum ConnCommand {
    /// Current network and connected devices only (cheap; no scan).
    Status,
    ScanWifi,
    ListBluetooth,
    ConnectWifi {
        ssid: String,
        password: Option<String>,
    },
    ConnectBluetooth(String),
    DisconnectBluetooth(String),
}

/// Results from the backend.
#[derive(Debug, Clone, PartialEq)]
pub enum ConnUpdate {
    Wifi(WifiState),
    Bluetooth(BtState),
    /// Work in progress ("Connecting to Home…"), or done (None).
    Busy(Option<String>),
    /// A connection failed. `needs_password` names the network to ask for.
    Failed {
        message: String,
        needs_password: Option<String>,
    },
    /// Forget the last failure (the list was reopened).
    ClearFailure,
}

/// Everything the island knows, for drawing.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Connectivity {
    pub wifi: WifiState,
    pub bt: BtState,
    pub busy: Option<String>,
    pub failure: Option<(String, Option<String>)>,
}

impl Connectivity {
    pub fn apply(&mut self, u: ConnUpdate) {
        match u {
            ConnUpdate::Wifi(w) => self.wifi = w,
            ConnUpdate::Bluetooth(b) => self.bt = b,
            ConnUpdate::Busy(b) => {
                if b.is_some() {
                    self.failure = None;
                }
                self.busy = b;
            }
            ConnUpdate::Failed { message, needs_password } => {
                self.busy = None;
                self.failure = Some((message, needs_password));
            }
            ConnUpdate::ClearFailure => self.failure = None,
        }
    }

    /// The connected Bluetooth device shown on the button.
    pub fn connected_device(&self) -> Option<&BtDevice> {
        self.bt.devices.iter().find(|d| d.connected)
    }
}

/// One entry per network name (the strongest access point), hidden networks
/// dropped; connected first, then saved ones, then by signal.
pub fn merge_networks(raw: Vec<WifiNetwork>) -> Vec<WifiNetwork> {
    let mut out: Vec<WifiNetwork> = Vec::new();
    for n in raw.into_iter().filter(|n| !n.ssid.trim().is_empty()) {
        match out.iter_mut().find(|o| o.ssid == n.ssid) {
            Some(o) => {
                o.active |= n.active;
                o.known |= n.known;
                o.secure |= n.secure;
                o.strength = o.strength.max(n.strength);
            }
            None => out.push(n),
        }
    }
    out.sort_by(|a, b| {
        (b.active, b.known, b.strength).cmp(&(a.active, a.known, a.strength)).then_with(|| a.ssid.cmp(&b.ssid))
    });
    out
}

/// Connected first, then by name.
pub fn sort_devices(devices: &mut [BtDevice]) {
    devices
        .sort_by(|a, b| b.connected.cmp(&a.connected).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
}

/// Signal bars 1..=3 for the Wi-Fi icon.
pub fn bars(strength: u8) -> u8 {
    match strength {
        0..=34 => 1,
        35..=64 => 2,
        _ => 3,
    }
}

/// RSSI (dBm) to a 0..=100 signal, as the OS menus show it.
pub fn rssi_to_strength(rssi: i32) -> u8 {
    (2 * (rssi + 100)).clamp(0, 100) as u8
}

/// A device kind from a freedesktop icon name ("audio-headset") or a
/// device name, for the list icons.
pub fn kind_from_hint(hint: &str) -> DeviceKind {
    let h = hint.to_ascii_lowercase();
    if h.contains("headset") || h.contains("headphone") || h.contains("buds") || h.contains("airpods") {
        if h.contains("buds") || h.contains("airpods") {
            DeviceKind::Earbuds
        } else {
            DeviceKind::Headphones
        }
    } else if h.contains("mouse") || h.contains("trackpad") {
        DeviceKind::Mouse
    } else if h.contains("keyboard") {
        DeviceKind::Keyboard
    } else if h.contains("gaming") || h.contains("controller") || h.contains("gamepad") {
        DeviceKind::Gamepad
    } else if h.contains("phone") {
        DeviceKind::Phone
    } else if h.contains("speaker") || h.contains("audio-card") {
        DeviceKind::Speaker
    } else {
        DeviceKind::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn net(ssid: &str, strength: u8, known: bool, active: bool) -> WifiNetwork {
        WifiNetwork { ssid: ssid.into(), strength, secure: true, known, active }
    }

    #[test]
    fn merges_and_orders_networks() {
        let raw = vec![
            net("Cafe", 90, false, false),
            net("Home", 40, true, false),
            net("Home", 70, true, true),
            net("", 99, false, false),
            net("Office", 60, true, false),
        ];
        let names: Vec<_> = merge_networks(raw).into_iter().map(|n| (n.ssid, n.strength)).collect();
        assert_eq!(names, [("Home".into(), 70), ("Office".into(), 60), ("Cafe".into(), 90)]);
    }

    #[test]
    fn devices_bars_and_hints() {
        let mut d = vec![
            BtDevice { id: "1".into(), name: "mouse".into(), kind: DeviceKind::Mouse, connected: false },
            BtDevice { id: "2".into(), name: "Buds".into(), kind: DeviceKind::Earbuds, connected: true },
            BtDevice { id: "3".into(), name: "Keyboard".into(), kind: DeviceKind::Keyboard, connected: false },
        ];
        sort_devices(&mut d);
        assert_eq!(d.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(), ["2", "3", "1"]);
        assert_eq!((bars(20), bars(50), bars(90)), (1, 2, 3));
        assert_eq!(rssi_to_strength(-50), 100);
        assert_eq!(rssi_to_strength(-75), 50);
        assert_eq!(kind_from_hint("audio-headset"), DeviceKind::Headphones);
        assert_eq!(kind_from_hint("Galaxy Buds2"), DeviceKind::Earbuds);
        assert_eq!(kind_from_hint("input-gaming"), DeviceKind::Gamepad);
    }

    #[test]
    fn failures_clear_when_new_work_starts() {
        let mut c = Connectivity::default();
        c.apply(ConnUpdate::Failed { message: "Wrong password".into(), needs_password: Some("Home".into()) });
        assert!(c.failure.is_some());
        c.apply(ConnUpdate::Busy(Some("Connecting to Home…".into())));
        assert!(c.failure.is_none());
        assert_eq!(c.busy.as_deref(), Some("Connecting to Home…"));
    }
}
