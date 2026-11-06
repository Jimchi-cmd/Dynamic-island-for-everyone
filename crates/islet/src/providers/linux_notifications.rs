//! Desktop notifications on Linux, mirrored on the island (opt-in).
//!
//! A separate session-bus connection becomes a D-Bus *monitor* for
//! `org.freedesktop.Notifications.Notify` calls only. A monitor can't send
//! anything, so the desktop's own notifications are untouched. Turning the
//! feature off closes the connection.

use super::{debug_log, Bus, Sink};
use islet_core::activity::{Note, Report};
use std::collections::HashMap;
use zbus::blocking::{Connection, MessageIterator};
use zbus::message::Type as MsgType;
use zbus::zvariant::OwnedValue;

const RULE: &str = "type='method_call',interface='org.freedesktop.Notifications',member='Notify'";

type NotifyArgs = (String, u32, String, String, String, Vec<String>, HashMap<String, OwnedValue>, i32);

/// Strip the small HTML subset notification bodies may use (<b>, <i>, <a>…).
pub fn plain_text(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut in_tag = false;
    for c in body.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'")
}

fn monitor(conn: &Connection, sink: &Sink) -> zbus::Result<()> {
    conn.call_method(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        Some("org.freedesktop.DBus.Monitoring"),
        "BecomeMonitor",
        &(vec![RULE], 0u32),
    )?;
    for msg in MessageIterator::from(conn) {
        let Ok(msg) = msg else { continue };
        let h = msg.header();
        if h.message_type() != MsgType::MethodCall || h.member().map(|m| m.as_str()) != Some("Notify") {
            continue;
        }
        let Ok((app, _replaces, _icon, summary, body, _actions, _hints, _timeout)) =
            msg.body().deserialize::<NotifyArgs>()
        else {
            continue;
        };
        if summary.trim().is_empty() && body.trim().is_empty() {
            continue;
        }
        let app = if app.trim().is_empty() { "Notification".to_string() } else { app };
        sink.send(Report::Notification(Note { app, summary: plain_text(&summary), body: plain_text(&body) }));
    }
    Ok(())
}

/// The running monitor, if notifications are on.
#[derive(Default)]
pub struct Notifications {
    conn: Option<Connection>,
}

impl Notifications {
    pub fn set(&mut self, on: bool, sink: &Sink, bus: &Bus) {
        match (on, self.conn.take()) {
            (true, None) => match bus.connect() {
                Ok(conn) => {
                    self.conn = Some(conn.clone());
                    let sink = sink.clone();
                    let _ = std::thread::Builder::new().name("islet-notifications".into()).spawn(move || {
                        if let Err(e) = monitor(&conn, &sink) {
                            debug_log(&format!("notifications: {e}"));
                        }
                    });
                }
                Err(e) => debug_log(&format!("notifications unavailable (session bus: {e})")),
            },
            (false, Some(conn)) => {
                // Closing ends the monitor thread's message stream.
                let _ = conn.close();
            }
            (_, keep) => self.conn = keep,
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn strips_markup() {
        assert_eq!(
            super::plain_text("<b>Amina</b>: see you at 5 &amp; bring snacks"),
            "Amina: see you at 5 & bring snacks"
        );
    }
}
