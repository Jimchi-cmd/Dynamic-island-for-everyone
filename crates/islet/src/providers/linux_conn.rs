//! Wi-Fi through NetworkManager and Bluetooth through BlueZ (system bus).
//! Calls are made only on request; nothing is subscribed or polled while idle.

use super::connectivity::{Backend, ConnectError};
use super::Bus;
use islet_core::connectivity::{self, BtDevice, BtState, WifiNetwork, WifiState};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

const NM: &str = "org.freedesktop.NetworkManager";
const NM_PATH: &str = "/org/freedesktop/NetworkManager";
const NM_SETTINGS: &str = "/org/freedesktop/NetworkManager/Settings";
const DEVICE: &str = "org.freedesktop.NetworkManager.Device";
const WIRELESS: &str = "org.freedesktop.NetworkManager.Device.Wireless";
const AP: &str = "org.freedesktop.NetworkManager.AccessPoint";
const ACTIVE: &str = "org.freedesktop.NetworkManager.Connection.Active";
const BLUEZ: &str = "org.bluez";
const DEVICE_TYPE_WIFI: u32 = 2;
/// NM_802_11_AP_SEC_KEY_MGMT_SAE (WPA3 personal).
const KEY_MGMT_SAE: u32 = 0x400;

type Settings = HashMap<String, HashMap<String, OwnedValue>>;

pub struct Linux {
    bus: Bus,
    conn: Option<Connection>,
}

struct Ap {
    path: OwnedObjectPath,
    net: WifiNetwork,
    sae: bool,
}

impl Linux {
    pub fn new(bus: Bus) -> Self {
        Self { bus, conn: None }
    }

    fn conn(&mut self) -> zbus::Result<Connection> {
        if let Some(c) = &self.conn {
            return Ok(c.clone());
        }
        let c = self.bus.connect()?;
        self.conn = Some(c.clone());
        Ok(c)
    }

    fn proxy(c: &Connection, dest: &'static str, path: &str, iface: &'static str) -> zbus::Result<Proxy<'static>> {
        zbus::blocking::proxy::Builder::new(c)
            .destination(dest)?
            .path(zbus::zvariant::ObjectPath::try_from(path.to_string())?)?
            .interface(iface)?
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
    }

    fn wifi_device(c: &Connection) -> zbus::Result<Option<OwnedObjectPath>> {
        let nm = Self::proxy(c, NM, NM_PATH, NM)?;
        let devices: Vec<OwnedObjectPath> = nm.call("GetDevices", &())?;
        for d in devices {
            let dev = Self::proxy(c, NM, d.as_str(), DEVICE)?;
            if dev.get_property::<u32>("DeviceType").ok() == Some(DEVICE_TYPE_WIFI) {
                return Ok(Some(d));
            }
        }
        Ok(None)
    }

    /// Saved Wi-Fi connections: SSID → connection path.
    fn saved(c: &Connection) -> HashMap<String, OwnedObjectPath> {
        let mut out = HashMap::new();
        let Ok(settings) = Self::proxy(c, NM, NM_SETTINGS, "org.freedesktop.NetworkManager.Settings") else {
            return out;
        };
        let Ok(list) = settings.call::<_, _, Vec<OwnedObjectPath>>("ListConnections", &()) else { return out };
        for path in list {
            let Ok(p) = Self::proxy(c, NM, path.as_str(), "org.freedesktop.NetworkManager.Settings.Connection") else {
                continue;
            };
            let Ok(s) = p.call::<_, _, Settings>("GetSettings", &()) else { continue };
            if let Some(ssid) = s.get("802-11-wireless").and_then(|w| w.get("ssid")).and_then(ssid_of) {
                out.insert(ssid, path);
            }
        }
        out
    }

    fn access_points(c: &Connection, dev: &OwnedObjectPath) -> zbus::Result<(Vec<Ap>, Option<String>)> {
        let w = Self::proxy(c, NM, dev.as_str(), WIRELESS)?;
        let active: Option<OwnedObjectPath> = w.get_property("ActiveAccessPoint").ok();
        let paths: Vec<OwnedObjectPath> = w.call("GetAllAccessPoints", &())?;
        let saved = Self::saved(c);
        let mut aps = Vec::new();
        let mut current = None;
        for path in paths {
            let p = Self::proxy(c, NM, path.as_str(), AP)?;
            let ssid = String::from_utf8_lossy(&p.get_property::<Vec<u8>>("Ssid").unwrap_or_default()).into_owned();
            let flags = |name: &str| p.get_property::<u32>(name).unwrap_or(0);
            let (wpa, rsn) = (flags("WpaFlags"), flags("RsnFlags"));
            let is_active = active.as_ref().is_some_and(|a| *a == path);
            if is_active {
                current = Some(ssid.clone());
            }
            aps.push(Ap {
                net: WifiNetwork {
                    known: saved.contains_key(&ssid),
                    strength: p.get_property::<u8>("Strength").unwrap_or(0),
                    secure: flags("Flags") & 1 != 0 || wpa != 0 || rsn != 0,
                    active: is_active,
                    ssid,
                },
                sae: rsn & KEY_MGMT_SAE != 0,
                path,
            });
        }
        Ok((aps, current))
    }

    fn try_wifi(&mut self, scan: bool) -> zbus::Result<WifiState> {
        let c = self.conn()?;
        let nm = Self::proxy(&c, NM, NM_PATH, NM)?;
        let enabled = nm.get_property::<bool>("WirelessEnabled").unwrap_or(true);
        let Some(dev) = Self::wifi_device(&c)? else {
            return Ok(WifiState { note: Some("No Wi-Fi adapter found".into()), ..WifiState::default() });
        };
        if !enabled {
            return Ok(WifiState { available: true, note: Some("Wi-Fi is off".into()), ..WifiState::default() });
        }
        if scan {
            let w = Self::proxy(&c, NM, dev.as_str(), WIRELESS)?;
            let before: i64 = w.get_property("LastScan").unwrap_or(0);
            // NetworkManager may refuse a scan it just did; the list is fresh then.
            if w.call::<_, _, ()>("RequestScan", &(HashMap::<String, Value>::new(),)).is_ok() {
                let until = Instant::now() + Duration::from_secs(6);
                while Instant::now() < until && w.get_property::<i64>("LastScan").unwrap_or(before) == before {
                    std::thread::sleep(Duration::from_millis(250));
                }
            }
        }
        let (aps, current) = Self::access_points(&c, &dev)?;
        Ok(WifiState {
            available: true,
            enabled: true,
            current,
            networks: connectivity::merge_networks(aps.into_iter().map(|a| a.net).collect()),
            note: None,
        })
    }

    fn try_connect(&mut self, ssid: &str, password: Option<&str>) -> Result<(), ConnectError> {
        let fail = |e: zbus::Error| match e {
            zbus::Error::MethodError(_, Some(msg), _) => ConnectError::Failed(format!("Couldn't join {ssid}: {msg}")),
            e => ConnectError::Failed(unavailable("NetworkManager", e)),
        };
        let c = self.conn().map_err(fail)?;
        let dev = Self::wifi_device(&c).map_err(fail)?.ok_or(ConnectError::Failed("No Wi-Fi adapter".into()))?;
        let (aps, _) = Self::access_points(&c, &dev).map_err(fail)?;
        let ap = aps
            .iter()
            .filter(|a| a.net.ssid == ssid)
            .max_by_key(|a| a.net.strength)
            .ok_or_else(|| ConnectError::Failed(format!("{ssid} is out of range")))?;
        let nm = Self::proxy(&c, NM, NM_PATH, NM).map_err(fail)?;
        let saved = Self::saved(&c);
        let (active, added): (OwnedObjectPath, Option<OwnedObjectPath>) = match (saved.get(ssid), password) {
            (Some(conn), None) => (nm.call("ActivateConnection", &(conn, &dev, &ap.path)).map_err(fail)?, None),
            (_, pw) => {
                if ap.net.secure && pw.is_none() {
                    return Err(ConnectError::NeedsPassword);
                }
                let mut wireless = HashMap::new();
                wireless.insert("ssid", Value::from(ssid.as_bytes().to_vec()));
                let mut settings: HashMap<&str, HashMap<&str, Value>> = HashMap::new();
                settings.insert("802-11-wireless", wireless);
                if let Some(pw) = pw {
                    let mut sec = HashMap::new();
                    sec.insert("key-mgmt", Value::from(if ap.sae { "sae" } else { "wpa-psk" }));
                    sec.insert("psk", Value::from(pw));
                    settings.insert("802-11-wireless-security", sec);
                }
                let (path, active): (OwnedObjectPath, OwnedObjectPath) =
                    nm.call("AddAndActivateConnection", &(settings, &dev, &ap.path)).map_err(fail)?;
                (active, Some(path))
            }
        };
        // Wait for the connection to come up (or fail).
        let p = Self::proxy(&c, NM, active.as_str(), ACTIVE).map_err(fail)?;
        let until = Instant::now() + Duration::from_secs(30);
        while Instant::now() < until {
            match p.get_property::<u32>("State") {
                Ok(2) => return Ok(()),
                Ok(3) | Ok(4) | Err(_) => break,
                _ => std::thread::sleep(Duration::from_millis(300)),
            }
        }
        // Don't leave a broken profile behind (a wrong password, say).
        if let Some(path) = added {
            if let Ok(conn) = Self::proxy(&c, NM, path.as_str(), "org.freedesktop.NetworkManager.Settings.Connection") {
                let _ = conn.call::<_, _, ()>("Delete", &());
            }
        }
        Err(ConnectError::Failed(if password.is_some() {
            format!("Couldn't join {ssid}. Check the password.")
        } else {
            format!("Couldn't join {ssid}")
        }))
    }

    fn try_bluetooth(&mut self) -> zbus::Result<BtState> {
        let c = self.conn()?;
        let om = Self::proxy(&c, BLUEZ, "/", "org.freedesktop.DBus.ObjectManager")?;
        let objects: HashMap<OwnedObjectPath, Settings> = om.call("GetManagedObjects", &())?;
        let mut state = BtState { can_connect: true, ..BtState::default() };
        for (path, ifaces) in objects {
            if let Some(a) = ifaces.get("org.bluez.Adapter1") {
                state.available = true;
                state.powered |= a.get("Powered").and_then(|v| bool::try_from(v).ok()).unwrap_or(false);
            }
            let Some(d) = ifaces.get("org.bluez.Device1") else { continue };
            let flag = |k: &str| d.get(k).and_then(|v| bool::try_from(v).ok()).unwrap_or(false);
            if !flag("Paired") {
                continue;
            }
            let text = |k: &str| d.get(k).and_then(|v| <&str>::try_from(v).ok().map(String::from));
            let name = text("Alias").or_else(|| text("Name")).unwrap_or_else(|| "Unknown device".into());
            let hint = format!("{} {}", text("Icon").unwrap_or_default(), name);
            state.devices.push(BtDevice {
                id: path.to_string(),
                kind: connectivity::kind_from_hint(&hint),
                connected: flag("Connected"),
                name,
            });
        }
        if !state.available {
            state.note = Some("No Bluetooth adapter found".into());
        } else if !state.powered {
            state.note = Some("Bluetooth is off".into());
        }
        connectivity::sort_devices(&mut state.devices);
        Ok(state)
    }

    fn try_bt_set(&mut self, id: &str, connect: bool) -> zbus::Result<()> {
        let c = self.conn()?;
        if connect {
            // Turn the adapter on first if needed (the device path starts with it).
            if let Some(adapter) = id.rsplit_once("/dev_").map(|(a, _)| a) {
                let props = Self::proxy(&c, BLUEZ, adapter, "org.freedesktop.DBus.Properties")?;
                let _ = props.call::<_, _, ()>("Set", &("org.bluez.Adapter1", "Powered", Value::from(true)));
            }
        }
        let dev = Self::proxy(&c, BLUEZ, id, "org.bluez.Device1")?;
        dev.call::<_, _, ()>(if connect { "Connect" } else { "Disconnect" }, &())
    }
}

/// An SSID stored as bytes (`ay`).
fn ssid_of(v: &OwnedValue) -> Option<String> {
    let bytes = Vec::<u8>::try_from(v.try_clone().ok()?).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

fn unavailable(what: &str, e: zbus::Error) -> String {
    super::debug_log(&format!("{what}: {e}"));
    match e {
        zbus::Error::MethodError(name, ..) if name.as_str() == "org.freedesktop.DBus.Error.ServiceUnknown" => {
            format!("{what} isn't running")
        }
        zbus::Error::InputOutput(_) | zbus::Error::Address(_) => format!("{what} isn't available"),
        _ => format!("{what} isn't available"),
    }
}

impl Backend for Linux {
    fn wifi(&mut self, scan: bool) -> WifiState {
        self.try_wifi(scan)
            .unwrap_or_else(|e| WifiState { note: Some(unavailable("NetworkManager", e)), ..WifiState::default() })
    }

    fn wifi_connect(&mut self, ssid: &str, password: Option<&str>) -> Result<(), ConnectError> {
        self.try_connect(ssid, password)
    }

    fn bluetooth(&mut self) -> BtState {
        self.try_bluetooth()
            .unwrap_or_else(|e| BtState { note: Some(unavailable("Bluetooth (BlueZ)", e)), ..BtState::default() })
    }

    fn bt_set(&mut self, id: &str, connect: bool) -> Result<(), String> {
        self.try_bt_set(id, connect).map_err(|e| match e {
            zbus::Error::MethodError(_, Some(msg), _) => format!("Couldn't connect: {msg}"),
            e => format!("Couldn't connect: {e}"),
        })
    }
}
