//! Linux: battery levels from UPower over D-Bus — Bluetooth headphones,
//! earbuds, mice, keyboards and controllers (via BlueZ), wireless mice with
//! USB receivers (via the kernel), and the laptop's own battery.
//!
//! Event-driven: `DeviceAdded` / `DeviceRemoved` and `PropertiesChanged`
//! signals. Nothing is polled.

use super::{dbus_util::*, Bus, Sink};
use islet_core::activity::{BatteryDevice, DeviceKind, Report};
use std::collections::HashMap;
use zbus::blocking::{fdo::DBusProxy, Connection, MessageIterator};
use zbus::message::Type as MsgType;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
use zbus::MatchRule;

const SERVICE: &str = "org.freedesktop.UPower";
const ROOT: &str = "/org/freedesktop/UPower";
const DEVICE_IFACE: &str = "org.freedesktop.UPower.Device";

/// UPower's device kinds (UpDeviceKind) that are peripherals we show.
pub fn kind_of(upower_type: u32) -> Option<DeviceKind> {
    Some(match upower_type {
        5 => DeviceKind::Mouse,
        6 | 14 => DeviceKind::Keyboard, // keyboard, touchpad
        8 => DeviceKind::Phone,
        10 => DeviceKind::Tablet,
        12 => DeviceKind::Gamepad,
        17 | 19 => DeviceKind::Headphones, // headset, headphones
        18 => DeviceKind::Speaker,
        13 | 21 | 22 | 26 | 27 | 28 => DeviceKind::Other, // pen, other audio, remote, wearable, toy, Bluetooth
        _ => return None,
    })
}

/// UPower `State`: 1 charging, 4 fully charged, 5 pending charge.
fn is_charging(state: u32) -> bool {
    matches!(state, 1 | 4 | 5)
}

fn get_all(conn: &Connection, path: &str, iface: &str) -> zbus::Result<HashMap<String, OwnedValue>> {
    let reply = conn.call_method(Some(SERVICE), path, Some("org.freedesktop.DBus.Properties"), "GetAll", &(iface,))?;
    reply.body().deserialize()
}

/// A peripheral with a battery, or None for line power, UPS, the laptop…
pub fn peripheral(path: &str, props: &HashMap<String, OwnedValue>) -> Option<BatteryDevice> {
    let num = |k: &str| props.get(k).and_then(|v| i64_of(v));
    let kind = kind_of(num("Type")? as u32)?;
    if props.get("PowerSupply").and_then(|v| bool_of(v)) == Some(true) {
        return None; // powers the computer: not a peripheral
    }
    let percent = props.get("Percentage").and_then(|v| f64_of(v))?;
    let name = ["Model", "Vendor"]
        .iter()
        .filter_map(|k| props.get(*k).and_then(|v| str_of(v)))
        .find(|s| !s.trim().is_empty())
        .unwrap_or_else(|| format!("{kind:?}"));
    Some(BatteryDevice {
        id: path.to_string(),
        name,
        kind,
        percent: percent.round().clamp(0.0, 100.0) as u8,
        charging: num("State").is_some_and(|s| is_charging(s as u32)),
        parts: vec![],
    })
}

/// The laptop's own battery from the display device, if it has one.
pub fn laptop(display: &HashMap<String, OwnedValue>, on_battery: Option<bool>) -> Option<BatteryDevice> {
    let present = display.get("IsPresent").and_then(|v| bool_of(v)).unwrap_or(false);
    let ty = display.get("Type").and_then(|v| i64_of(v)).unwrap_or(0);
    if !present || ty != 2 {
        return None;
    }
    let percent = display.get("Percentage").and_then(|v| f64_of(v))?;
    let state = display.get("State").and_then(|v| i64_of(v)).unwrap_or(0) as u32;
    Some(BatteryDevice {
        id: "laptop".into(),
        name: "This computer".into(),
        kind: DeviceKind::Laptop,
        percent: percent.round().clamp(0.0, 100.0) as u8,
        // On mains power counts as "charging" for the charger pop-up.
        charging: on_battery.map_or(is_charging(state), |b| !b),
        parts: vec![],
    })
}

fn rules() -> zbus::Result<Vec<MatchRule<'static>>> {
    let base = || MatchRule::builder().msg_type(MsgType::Signal).sender(SERVICE);
    Ok(vec![
        base()?.interface(SERVICE)?.member("DeviceAdded")?.build(),
        base()?.interface(SERVICE)?.member("DeviceRemoved")?.build(),
        base()?.interface("org.freedesktop.DBus.Properties")?.member("PropertiesChanged")?.build(),
    ])
}

struct Battery {
    conn: Connection,
    sink: Sink,
    display: Option<String>,
}

impl Battery {
    fn report_laptop(&self) {
        let Some(display) = &self.display else { return };
        let on_battery =
            get_all(&self.conn, ROOT, SERVICE).ok().and_then(|p| p.get("OnBattery").and_then(|v| bool_of(v)));
        if let Some(d) = get_all(&self.conn, display, DEVICE_IFACE).ok().and_then(|p| laptop(&p, on_battery)) {
            self.sink.send(Report::Laptop(d));
        }
    }

    fn device(&self, path: &str) -> Option<BatteryDevice> {
        get_all(&self.conn, path, DEVICE_IFACE).ok().and_then(|p| peripheral(path, &p))
    }

    fn run(self) -> zbus::Result<()> {
        let dbus = DBusProxy::new(&self.conn)?;
        for rule in rules()? {
            dbus.add_match_rule(rule)?;
        }
        let messages = MessageIterator::from(&self.conn);

        // Devices already connected at startup are recorded without a pop-up.
        let paths: Vec<OwnedObjectPath> =
            self.conn.call_method(Some(SERVICE), ROOT, Some(SERVICE), "EnumerateDevices", &())?.body().deserialize()?;
        for p in &paths {
            if let Some(d) = self.device(p.as_str()) {
                self.sink.send(Report::DeviceChanged(d));
            }
        }
        self.report_laptop();

        for msg in messages {
            let Ok(msg) = msg else { continue };
            let header = msg.header();
            if header.message_type() != MsgType::Signal {
                continue;
            }
            let member = header.member().map(|m| m.as_str().to_string()).unwrap_or_default();
            let path = header.path().map(|p| p.to_string()).unwrap_or_default();
            match member.as_str() {
                "DeviceAdded" => {
                    if let Ok(p) = msg.body().deserialize::<OwnedObjectPath>() {
                        if let Some(d) = self.device(p.as_str()) {
                            self.sink.send(Report::DeviceConnected(d));
                        }
                    }
                }
                "DeviceRemoved" => {
                    if let Ok(p) = msg.body().deserialize::<OwnedObjectPath>() {
                        self.sink.send(Report::DeviceGone(p.to_string()));
                    }
                }
                "PropertiesChanged" if path == ROOT || Some(&path) == self.display.as_ref() => self.report_laptop(),
                "PropertiesChanged" => {
                    if let Some(d) = self.device(&path) {
                        self.sink.send(Report::DeviceChanged(d));
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

pub fn start(sink: Sink, bus: Bus) {
    let conn = match bus.connect() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("islet: battery levels unavailable (D-Bus: {e})");
            return;
        }
    };
    let display = conn
        .call_method(Some(SERVICE), ROOT, Some(SERVICE), "GetDisplayDevice", &())
        .and_then(|r| r.body().deserialize::<OwnedObjectPath>())
        .ok()
        .map(|p| p.to_string());
    let battery = Battery { conn, sink, display };
    let _ = std::thread::Builder::new().name("islet-upower".into()).spawn(move || {
        if let Err(e) = battery.run() {
            eprintln!("islet: UPower listener stopped: {e}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::Value;

    fn props(pairs: &[(&str, Value<'static>)]) -> HashMap<String, OwnedValue> {
        pairs.iter().map(|(k, v)| (k.to_string(), OwnedValue::try_from(v.try_clone().unwrap()).unwrap())).collect()
    }

    #[test]
    fn classifies_peripherals() {
        let mouse = props(&[
            ("Type", Value::from(5u32)),
            ("PowerSupply", Value::from(false)),
            ("Percentage", Value::from(64.4f64)),
            ("Model", Value::from("MX Anywhere 3")),
            ("State", Value::from(2u32)),
        ]);
        let d = peripheral("/org/freedesktop/UPower/devices/mouse_1", &mouse).unwrap();
        assert_eq!((d.kind, d.percent, d.charging, d.name.as_str()), (DeviceKind::Mouse, 64, false, "MX Anywhere 3"));

        let laptop_cell = props(&[
            ("Type", Value::from(2u32)),
            ("PowerSupply", Value::from(true)),
            ("Percentage", Value::from(50.0f64)),
        ]);
        assert!(peripheral("/x", &laptop_cell).is_none(), "the laptop battery is not a peripheral");
        let line_power = props(&[("Type", Value::from(1u32))]);
        assert!(peripheral("/y", &line_power).is_none());
    }

    #[test]
    fn laptop_battery_follows_the_charger() {
        let display = props(&[
            ("Type", Value::from(2u32)),
            ("IsPresent", Value::from(true)),
            ("Percentage", Value::from(80.0f64)),
            ("State", Value::from(2u32)),
        ]);
        assert!(!laptop(&display, Some(true)).unwrap().charging);
        assert!(laptop(&display, Some(false)).unwrap().charging);
        let desktop = props(&[("Type", Value::from(2u32)), ("IsPresent", Value::from(false))]);
        assert!(laptop(&desktop, None).is_none(), "desktops have no battery");
    }
}
