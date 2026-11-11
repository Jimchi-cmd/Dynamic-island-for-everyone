//! Calendar: the private iCal (ICS) link from the keychain, fetched every
//! 15 minutes while calendar is on. The thread blocks on its channel and the
//! refresh timeout: no wakeups while calendar is off.

use super::{debug_log, Sink};
use crate::{clock, secrets};
use islet_core::activity::Report;
use islet_core::calendar::{self, CalEvent};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::time::Duration;

/// Events from a day ago to six weeks ahead (deadlines count down from afar).
pub fn fetch() -> Result<Vec<CalEvent>, String> {
    let link = secrets::calendar_url().ok_or("No calendar link yet. Add one with: islet calendar set")?;
    let mut resp = crate::net::agent().get(&calendar::feed_url(&link)).call().map_err(|e| format!("network: {e}"))?;
    if resp.status() != 200 {
        return Err(format!("the calendar link answered {} (is it still valid?)", resp.status()));
    }
    let body =
        resp.body_mut().with_config().limit(16 * 1024 * 1024).read_to_string().map_err(|e| format!("network: {e}"))?;
    let now = clock::epoch() as i64;
    Ok(calendar::parse_ics(&body, now - 86_400, now + 42 * 86_400, clock::utc_offset()))
}

/// Send `true` to turn calendar on (or refresh now), `false` to turn it off.
pub fn start(sink: Sink) -> Sender<bool> {
    let (tx, rx) = channel::<bool>();
    let _ = std::thread::Builder::new().name("islet-calendar".into()).spawn(move || {
        let mut on = false;
        loop {
            let msg = if on {
                rx.recv_timeout(Duration::from_secs(calendar::REFRESH_SECS))
            } else {
                rx.recv().map_err(|_| RecvTimeoutError::Disconnected)
            };
            match msg {
                Ok(false) => {
                    if on {
                        sink.send(Report::Calendar(None));
                    }
                    on = false;
                    continue;
                }
                Ok(true) => on = true,
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            match fetch() {
                Ok(events) => sink.send(Report::Calendar(Some(events))),
                Err(e) => debug_log(&format!("calendar: {e}")),
            }
        }
    });
    tx
}
