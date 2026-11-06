//! Screen brightness on Linux, from the desktop's own power service on the
//! session bus: GNOME (gnome-settings-daemon, `Power.Screen.Brightness`) or
//! KDE Plasma (`BrightnessControl.brightnessChanged`). Signals only.

use super::dbus_util::i64_of;
use super::{debug_log, Bus, Sink};
use islet_core::activity::{Hud, Report};
use std::collections::HashMap;
use zbus::blocking::{fdo::DBusProxy, Connection, MessageIterator};
use zbus::message::Type as MsgType;
use zbus::zvariant::OwnedValue;
use zbus::MatchRule;

const GNOME_PATH: &str = "/org/gnome/SettingsDaemon/Power";
const GNOME_SCREEN: &str = "org.gnome.SettingsDaemon.Power.Screen";
const KDE: &str = "org.kde.Solid.PowerManagement";
const KDE_PATH: &str = "/org/kde/Solid/PowerManagement/Actions/BrightnessControl";
const KDE_IFACE: &str = "org.kde.Solid.PowerManagement.Actions.BrightnessControl";

fn rules() -> zbus::Result<Vec<MatchRule<'static>>> {
    Ok(vec![
        MatchRule::builder()
            .msg_type(MsgType::Signal)
            .path(GNOME_PATH)?
            .interface("org.freedesktop.DBus.Properties")?
            .member("PropertiesChanged")?
            .build(),
        MatchRule::builder()
            .msg_type(MsgType::Signal)
            .path(KDE_PATH)?
            .interface(KDE_IFACE)?
            .member("brightnessChanged")?
            .build(),
    ])
}

fn kde_max(conn: &Connection) -> f32 {
    conn.call_method(Some(KDE), KDE_PATH, Some(KDE_IFACE), "brightnessMax", &())
        .ok()
        .and_then(|m| m.body().deserialize::<i32>().ok())
        .filter(|m| *m > 0)
        .unwrap_or(100) as f32
}

fn run(conn: Connection, sink: Sink) -> zbus::Result<()> {
    let dbus = DBusProxy::new(&conn)?;
    for rule in rules()? {
        dbus.add_match_rule(rule)?;
    }
    let mut max: Option<f32> = None;
    for msg in MessageIterator::from(&conn) {
        let Ok(msg) = msg else { continue };
        let header = msg.header();
        if header.message_type() != MsgType::Signal {
            continue;
        }
        let level = match header.member().map(|m| m.as_str()) {
            Some("PropertiesChanged") => {
                let Ok((iface, changed, _)) =
                    msg.body().deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>()
                else {
                    continue;
                };
                if iface != GNOME_SCREEN {
                    continue;
                }
                let Some(pct) = changed.get("Brightness").and_then(|v| i64_of(v)) else { continue };
                if pct < 0 {
                    continue; // no backlight
                }
                pct as f32 / 100.0
            }
            Some("brightnessChanged") => {
                let Ok(v) = msg.body().deserialize::<i32>() else { continue };
                let max = *max.get_or_insert_with(|| kde_max(&conn));
                v as f32 / max
            }
            _ => continue,
        };
        sink.send(Report::Hud(Hud::Brightness(level.clamp(0.0, 1.0))));
    }
    Ok(())
}

pub fn start(sink: Sink, bus: Bus) {
    let _ = std::thread::Builder::new().name("islet-brightness".into()).spawn(move || match bus.connect() {
        Ok(conn) => {
            if let Err(e) = run(conn, sink) {
                debug_log(&format!("brightness pop-ups unavailable: {e}"));
            }
        }
        Err(e) => debug_log(&format!("brightness pop-ups unavailable (session bus: {e})")),
    });
}
