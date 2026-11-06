//! Weather: fetched from Open-Meteo only while the user has chosen a place,
//! once on start and then hourly. The thread blocks on its channel and a
//! one-hour timeout, so it costs one wakeup an hour while on and none off.

use super::{debug_log, Sink};
use islet_core::activity::Report;
use islet_core::prefs::Place;
use islet_core::weather::{self, Weather};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::time::Duration;

pub fn fetch(place: &Place) -> Result<Weather, String> {
    let mut resp =
        crate::net::agent().get(&weather::forecast_url(place)).call().map_err(|e| format!("network: {e}"))?;
    let body = resp.body_mut().read_to_string().map_err(|e| format!("network: {e}"))?;
    weather::parse_forecast(&body, &place.name)
}

pub fn search(query: &str) -> Result<Vec<Place>, String> {
    let mut resp = crate::net::agent().get(&weather::geocode_url(query)).call().map_err(|e| format!("network: {e}"))?;
    let body = resp.body_mut().read_to_string().map_err(|e| format!("network: {e}"))?;
    weather::parse_places(&body)
}

/// Send the place to follow (None turns weather off).
pub fn start(sink: Sink) -> Sender<Option<Place>> {
    let (tx, rx) = channel::<Option<Place>>();
    let _ = std::thread::Builder::new().name("islet-weather".into()).spawn(move || {
        let mut place: Option<Place> = None;
        loop {
            let msg = match &place {
                Some(_) => rx.recv_timeout(Duration::from_secs(weather::REFRESH_SECS)),
                None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
            };
            match msg {
                Ok(p) if p == place => continue, // unchanged: keep the schedule
                Ok(p) => {
                    place = p;
                    if place.is_none() {
                        sink.send(Report::Weather(None));
                        continue;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            if let Some(p) = &place {
                match fetch(p) {
                    Ok(w) => sink.send(Report::Weather(Some(w))),
                    Err(e) => debug_log(&format!("weather: {e}")),
                }
            }
        }
    });
    tx
}
