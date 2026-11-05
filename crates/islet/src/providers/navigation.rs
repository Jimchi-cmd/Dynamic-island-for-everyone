//! Navigation ETA from the Google Routes API, with the user's own key.
//!
//! Network use is limited to what the user turned on: requests are made
//! only while a trip is active, every minute while the card is open and
//! every 2–5 minutes otherwise (`islet_core::nav::refresh_interval`). Each
//! request is billed to the user's Google project, so nothing is retried
//! in a tight loop. Route content is not cached (Google's terms).

use super::{debug_log, Sink};
use crate::{config, secrets};
use islet_core::activity::{NavInfo, Report};
use islet_core::nav::{self, Trip};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

const ENDPOINT: &str = "https://routes.googleapis.com/directions/v2:computeRoutes";

#[derive(Debug, Clone)]
pub enum NavMsg {
    Start(Trip),
    Stop,
    /// The navigation card opened (true) or closed.
    CardOpen(bool),
}

fn fetch(agent: &ureq::Agent, endpoint: &str, key: &str, trip: &Trip) -> Result<nav::RouteInfo, String> {
    let mut resp = agent
        .post(endpoint)
        .header("X-Goog-Api-Key", key)
        .header("X-Goog-FieldMask", nav::FIELD_MASK)
        .header("Content-Type", "application/json")
        .send(trip.request_body().to_string())
        .map_err(|e| format!("network: {e}"))?;
    let body = resp.body_mut().read_to_string().map_err(|e| format!("network: {e}"))?;
    nav::parse_routes(&body)
}

pub fn start(sink: Sink) -> Sender<NavMsg> {
    let (tx, rx) = channel::<NavMsg>();
    let _ = std::thread::Builder::new().name("islet-nav".into()).spawn(move || {
        let agent = crate::net::agent();
        // Demos and perf runs never touch the network or the user's trip.
        let scripted = std::env::var_os("ISLET_DEMO").is_some() || std::env::var_os("ISLET_PERF_DEMO").is_some();
        let mut trip: Option<Trip> = if scripted { None } else { config::load_trip() };
        let mut last: Option<nav::RouteInfo> = None;
        let mut card_open = false;
        let mut next: Option<Instant> = trip.as_ref().map(|_| Instant::now());
        let mut requests = 0u64;
        loop {
            let msg = match next {
                Some(at) => rx.recv_timeout(at.saturating_duration_since(Instant::now())),
                None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
            };
            match msg {
                Ok(NavMsg::Start(t)) => {
                    config::save_trip(Some(&t));
                    trip = Some(t);
                    last = None;
                    if let Some(t) = &trip {
                        sink.send(Report::Navigation(Some(NavInfo { trip: t.clone(), route: None, error: None })));
                    }
                    next = Some(Instant::now());
                    continue;
                }
                Ok(NavMsg::Stop) => {
                    config::save_trip(None);
                    trip = None;
                    last = None;
                    next = None;
                    sink.send(Report::Navigation(None));
                    continue;
                }
                Ok(NavMsg::CardOpen(open)) => {
                    card_open = open;
                    // Opening the card refreshes if the last route is over a minute old.
                    if open && trip.is_some() {
                        let due = Instant::now() + Duration::from_secs(60);
                        next = Some(next.map_or(due, |n| n.min(due)));
                    }
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => return,
                Err(RecvTimeoutError::Timeout) => {}
            }
            let Some(t) = &trip else {
                next = None;
                continue;
            };
            let (route, error) = match secrets::routes_key().and_then(|key| {
                requests += 1;
                debug_log(&format!("Routes API request #{requests}"));
                fetch(&agent, ENDPOINT, &key, t)
            }) {
                Ok(r) => (Some(r), None),
                // Keep showing the last good route if a refresh fails.
                Err(e) => (last.clone(), Some(e)),
            };
            last = route.clone();
            let remaining = route.as_ref().map(|r| r.duration);
            sink.send(Report::Navigation(Some(NavInfo { trip: t.clone(), route, error })));
            next = Some(Instant::now() + Duration::from_secs_f64(nav::refresh_interval(card_open, remaining)));
        }
    });
    tx
}

/// Open a hand-off link in the default browser / app.
pub fn open_url(url: &str) {
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let r = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).spawn();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let r = std::process::Command::new("xdg-open").arg(url).spawn();
    if let Err(e) = r {
        debug_log(&format!("could not open {url}: {e}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;

    /// One-shot HTTP server: returns the request head + body it received.
    fn serve_once(status: &str, body: &'static str) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/directions/v2:computeRoutes", listener.local_addr().unwrap());
        let status = status.to_string();
        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut head = String::new();
            let mut len = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap();
                }
                if line == "\r\n" {
                    break;
                }
                head.push_str(&line);
            }
            let mut body_in = vec![0; len];
            reader.read_exact(&mut body_in).unwrap();
            let mut out = stream;
            write!(out, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            head + "\n" + &String::from_utf8(body_in).unwrap()
        });
        (url, handle)
    }

    fn trip() -> Trip {
        Trip {
            label: String::new(),
            destination: "Kampala Road".into(),
            origin: "Ntinda".into(),
            mode: nav::TravelMode::Drive,
        }
    }

    #[test]
    fn requests_a_route_with_key_and_field_mask() {
        let (url, server) =
            serve_once("200 OK", r#"{"routes":[{"duration":"900s","staticDuration":"600s","distanceMeters":5200}]}"#);
        let r = fetch(&crate::net::agent(), &url, "test-key", &trip()).unwrap();
        assert_eq!((r.duration, r.delay(), r.distance_m), (900.0, 300.0, 5200.0));
        let req = server.join().unwrap().to_ascii_lowercase();
        assert!(req.starts_with("post /directions/v2:computeroutes"));
        assert!(req.contains("x-goog-api-key: test-key"));
        assert!(req.contains(&format!("x-goog-fieldmask: {}", nav::FIELD_MASK.to_ascii_lowercase())));
        assert!(req.contains(r#""routingpreference":"traffic_aware""#));
    }

    #[test]
    fn surfaces_api_errors() {
        let (url, server) = serve_once("403 Forbidden", r#"{"error":{"code":403,"message":"API key not valid."}}"#);
        assert_eq!(fetch(&crate::net::agent(), &url, "bad", &trip()).unwrap_err(), "API key not valid.");
        server.join().unwrap();
    }
}
