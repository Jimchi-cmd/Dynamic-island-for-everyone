//! Navigation: a trip (destination, origin, travel mode), the route summary
//! from the Google Routes API, formatting, refresh scheduling and the
//! documented hand-off links to Google Maps and Waze.
//!
//! Neither Google Maps nor Waze exposes live turn-by-turn state from the
//! user's running app, so the island shows an ETA it fetches itself (with
//! the user's own API key) and hands real navigation off to those apps.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TravelMode {
    #[default]
    Drive,
    TwoWheeler,
    Walk,
    Bicycle,
    Transit,
}

impl TravelMode {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_str() {
            "drive" | "car" | "driving" => Self::Drive,
            "two-wheeler" | "twowheeler" | "motorbike" | "boda" => Self::TwoWheeler,
            "walk" | "walking" => Self::Walk,
            "bicycle" | "bike" | "cycling" => Self::Bicycle,
            "transit" => Self::Transit,
            _ => return None,
        })
    }

    /// The Routes API `travelMode` value.
    pub fn api(self) -> &'static str {
        match self {
            Self::Drive => "DRIVE",
            Self::TwoWheeler => "TWO_WHEELER",
            Self::Walk => "WALK",
            Self::Bicycle => "BICYCLE",
            Self::Transit => "TRANSIT",
        }
    }

    /// The Google Maps URLs `travelmode` value.
    fn maps(self) -> &'static str {
        match self {
            Self::Drive => "driving",
            Self::TwoWheeler => "two-wheeler",
            Self::Walk => "walking",
            Self::Bicycle => "bicycling",
            Self::Transit => "transit",
        }
    }

    /// Traffic only matters on roads.
    pub fn traffic_aware(self) -> bool {
        matches!(self, Self::Drive | Self::TwoWheeler)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trip {
    /// Short name shown on the island ("Work"); defaults to the destination.
    #[serde(default)]
    pub label: String,
    pub destination: String,
    pub origin: String,
    #[serde(default)]
    pub mode: TravelMode,
    /// Google place ID of the destination, when it was picked from
    /// suggestions (more exact than the address text).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place_id: Option<String>,
}

impl Trip {
    pub fn title(&self) -> &str {
        if self.label.trim().is_empty() {
            &self.destination
        } else {
            &self.label
        }
    }

    /// The JSON body for `directions/v2:computeRoutes`.
    pub fn request_body(&self) -> serde_json::Value {
        let mut body = serde_json::json!({
            "origin": { "address": self.origin },
            "destination": match &self.place_id {
                Some(id) => serde_json::json!({ "placeId": id }),
                None => serde_json::json!({ "address": self.destination }),
            },
            "travelMode": self.mode.api(),
        });
        if self.mode.traffic_aware() {
            body["routingPreference"] = "TRAFFIC_AWARE".into();
        }
        body
    }
}

/// The fields requested from the Routes API (it bills by field set).
pub const FIELD_MASK: &str = "routes.duration,routes.staticDuration,routes.distanceMeters,\
routes.legs.steps.distanceMeters,routes.legs.steps.navigationInstruction";

#[derive(Debug, Clone, PartialEq)]
pub struct RouteInfo {
    /// Travel time now, with traffic where applicable (seconds).
    pub duration: f64,
    /// Travel time without traffic (seconds).
    pub free_flow: f64,
    pub distance_m: f64,
    /// The first instruction ("Turn left onto Kampala Road"), and its maneuver.
    pub first_step: Option<String>,
    pub maneuver: Option<String>,
}

impl RouteInfo {
    /// Extra time caused by traffic (seconds, never negative).
    pub fn delay(&self) -> f64 {
        (self.duration - self.free_flow).max(0.0)
    }
}

fn seconds(v: &serde_json::Value) -> Option<f64> {
    v.as_str()?.strip_suffix('s')?.parse().ok()
}

/// Parse a `computeRoutes` response.
pub fn parse_routes(json: &str) -> Result<RouteInfo, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("bad response: {e}"))?;
    if let Some(err) = v.get("error") {
        let msg = err.get("message").and_then(|m| m.as_str()).unwrap_or("request failed");
        return Err(msg.to_string());
    }
    let route = v.get("routes").and_then(|r| r.get(0)).ok_or("no route found between these places")?;
    let duration = route.get("duration").and_then(seconds).ok_or("route has no duration")?;
    let first = route.pointer("/legs/0/steps").and_then(|s| s.as_array()).and_then(|steps| {
        steps.iter().find_map(|s| {
            let n = s.get("navigationInstruction")?;
            Some((
                n.get("instructions")?.as_str()?.to_string(),
                n.get("maneuver").and_then(|m| m.as_str()).map(String::from),
            ))
        })
    });
    Ok(RouteInfo {
        duration,
        free_flow: route.get("staticDuration").and_then(seconds).unwrap_or(duration),
        distance_m: route.get("distanceMeters").and_then(|d| d.as_f64()).unwrap_or(0.0),
        first_step: first.as_ref().map(|f| f.0.clone()),
        maneuver: first.and_then(|f| f.1),
    })
}

/// "4 min", "58 min", "1 h 05".
pub fn format_minutes(secs: f64) -> String {
    let m = (secs.max(0.0) / 60.0).round() as u64;
    if m < 60 {
        format!("{} min", m.max(1))
    } else {
        format!("{} h {:02}", m / 60, m % 60)
    }
}

/// "850 m", "12.3 km", "120 km".
pub fn format_distance(m: f64) -> String {
    if m < 1000.0 {
        format!("{} m", ((m / 10.0).round() * 10.0) as u64)
    } else if m < 100_000.0 {
        format!("{:.1} km", m / 1000.0)
    } else {
        format!("{} km", (m / 1000.0).round() as u64)
    }
}

/// Arrival clock time "14:52" given the time now (hour, minute).
pub fn arrival(now_h: u8, now_m: u8, duration: f64) -> String {
    let total = now_h as u64 * 60 + now_m as u64 + (duration.max(0.0) / 60.0).round() as u64;
    format!("{:02}:{:02}", (total / 60) % 24, total % 60)
}

/// Seconds until the next refresh: every minute while the card is open,
/// otherwise every 5 minutes (2 when arrival is near). Each request is
/// billed, so the island never refreshes faster than this.
pub fn refresh_interval(expanded: bool, remaining: Option<f64>) -> f64 {
    if expanded {
        60.0
    } else if remaining.is_some_and(|r| r < 15.0 * 60.0) {
        120.0
    } else {
        300.0
    }
}

pub(crate) fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Google Maps URLs (documented, no key needed): start navigation there.
pub fn google_maps_url(trip: &Trip) -> String {
    let place = trip.place_id.as_ref().map_or(String::new(), |id| format!("&destination_place_id={}", encode(id)));
    format!(
        "https://www.google.com/maps/dir/?api=1&destination={}{place}&travelmode={}&dir_action=navigate",
        encode(&trip.destination),
        trip.mode.maps()
    )
}

/// Places API (New) autocomplete: where suggestions come from.
pub const PLACES_ENDPOINT: &str = "https://places.googleapis.com/v1/places:autocomplete";
/// Only the fields the list shows (Places bills by field set).
pub const PLACES_FIELD_MASK: &str = "suggestions.placePrediction.placeId,suggestions.placePrediction.text,\
suggestions.placePrediction.structuredFormat";

/// One destination suggestion.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaceSuggestion {
    pub place_id: String,
    /// "Garden City Mall"
    pub main: String,
    /// "Yusuf Lule Road, Kampala, Uganda"
    pub secondary: String,
    /// The whole text, used as the trip's destination.
    pub full: String,
}

/// The request body; `session` groups one typing session's requests for billing.
pub fn autocomplete_body(input: &str, session: Option<&str>) -> serde_json::Value {
    let mut body = serde_json::json!({ "input": input });
    if let Some(s) = session {
        body["sessionToken"] = s.into();
    }
    body
}

pub fn parse_autocomplete(body: &str) -> Result<Vec<PlaceSuggestion>, String> {
    let v: serde_json::Value = serde_json::from_str(body).map_err(|e| format!("unexpected reply: {e}"))?;
    if let Some(msg) = v["error"]["message"].as_str() {
        return Err(msg.to_string());
    }
    let text = |x: &serde_json::Value| x["text"].as_str().unwrap_or("").to_string();
    Ok(v["suggestions"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|s| {
                    let p = &s["placePrediction"];
                    let id = p["placeId"].as_str()?.to_string();
                    let full = text(&p["text"]);
                    let main = text(&p["structuredFormat"]["mainText"]);
                    Some(PlaceSuggestion {
                        place_id: id,
                        main: if main.is_empty() { full.clone() } else { main },
                        secondary: text(&p["structuredFormat"]["secondaryText"]),
                        full,
                    })
                })
                .collect()
        })
        .unwrap_or_default())
}

/// Waze deep link (documented): navigate to the destination.
pub fn waze_url(trip: &Trip) -> String {
    format!("https://waze.com/ul?q={}&navigate=yes", encode(&trip.destination))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trip() -> Trip {
        Trip {
            label: "Work".into(),
            destination: "Plot 5, Kampala Road".into(),
            origin: "Ntinda".into(),
            mode: TravelMode::Drive,
            place_id: None,
        }
    }

    #[test]
    fn builds_the_routes_request() {
        let body = trip().request_body();
        assert_eq!(body["destination"]["address"], "Plot 5, Kampala Road");
        assert_eq!(body["travelMode"], "DRIVE");
        assert_eq!(body["routingPreference"], "TRAFFIC_AWARE");
        let walk = Trip { mode: TravelMode::Walk, ..trip() };
        assert!(walk.request_body().get("routingPreference").is_none(), "traffic is irrelevant on foot");
    }

    #[test]
    fn parses_a_route() {
        let json = r#"{"routes":[{"distanceMeters":12345,"duration":"1530s","staticDuration":"1200s",
            "legs":[{"steps":[{"distanceMeters":50},{"distanceMeters":300,"navigationInstruction":
            {"maneuver":"TURN_LEFT","instructions":"Turn left onto Kampala Rd"}}]}]}]}"#;
        let r = parse_routes(json).unwrap();
        assert_eq!((r.duration, r.free_flow, r.distance_m), (1530.0, 1200.0, 12345.0));
        assert_eq!(r.delay(), 330.0);
        assert_eq!(r.first_step.as_deref(), Some("Turn left onto Kampala Rd"));
        assert_eq!(r.maneuver.as_deref(), Some("TURN_LEFT"));
    }

    #[test]
    fn reports_api_errors_and_empty_results() {
        assert_eq!(
            parse_routes(r#"{"error":{"code":403,"message":"API key not valid."}}"#).unwrap_err(),
            "API key not valid."
        );
        assert!(parse_routes("{}").unwrap_err().contains("no route"));
        assert!(parse_routes("not json").is_err());
    }

    #[test]
    fn formats() {
        assert_eq!(format_minutes(20.0), "1 min");
        assert_eq!(format_minutes(1530.0), "26 min");
        assert_eq!(format_minutes(3900.0), "1 h 05");
        assert_eq!(format_distance(847.0), "850 m");
        assert_eq!(format_distance(12345.0), "12.3 km");
        assert_eq!(format_distance(150_400.0), "150 km");
        assert_eq!(arrival(23, 50, 1530.0), "00:16");
    }

    #[test]
    fn refreshes_sparingly() {
        assert_eq!(refresh_interval(true, Some(3600.0)), 60.0);
        assert_eq!(refresh_interval(false, Some(3600.0)), 300.0);
        assert_eq!(refresh_interval(false, Some(600.0)), 120.0);
    }

    #[test]
    fn hand_off_links() {
        let t = trip();
        assert_eq!(
            google_maps_url(&t),
            "https://www.google.com/maps/dir/?api=1&destination=Plot+5%2C+Kampala+Road&travelmode=driving&dir_action=navigate"
        );
        assert_eq!(waze_url(&t), "https://waze.com/ul?q=Plot+5%2C+Kampala+Road&navigate=yes");
        assert_eq!(TravelMode::parse("boda"), Some(TravelMode::TwoWheeler));
    }

    #[test]
    fn place_suggestions() {
        let body = r#"{"suggestions":[
          {"placePrediction":{"placeId":"ChIJ1","text":{"text":"Garden City Mall, Yusuf Lule Road, Kampala, Uganda"},
            "structuredFormat":{"mainText":{"text":"Garden City Mall"},"secondaryText":{"text":"Yusuf Lule Road, Kampala, Uganda"}}}},
          {"queryPrediction":{"text":{"text":"garden city restaurants"}}},
          {"placePrediction":{"placeId":"ChIJ2","text":{"text":"Gardens"}}}
        ]}"#;
        let s = parse_autocomplete(body).unwrap();
        assert_eq!(s.len(), 2, "query predictions are skipped");
        assert_eq!(
            (s[0].main.as_str(), s[0].secondary.as_str()),
            ("Garden City Mall", "Yusuf Lule Road, Kampala, Uganda")
        );
        assert_eq!(s[1].main, "Gardens");
        assert_eq!(
            parse_autocomplete(r#"{"error":{"message":"Places API (New) has not been used"}}"#).unwrap_err(),
            "Places API (New) has not been used"
        );
        assert_eq!(parse_autocomplete("{}").unwrap(), vec![]);
        assert_eq!(autocomplete_body("gar", Some("tok"))["sessionToken"], "tok");
        assert!(autocomplete_body("gar", None).get("sessionToken").is_none());
    }

    #[test]
    fn picked_places_route_by_id() {
        let t = Trip { place_id: Some("ChIJ1".into()), ..trip() };
        assert_eq!(t.request_body()["destination"]["placeId"], "ChIJ1");
        assert!(google_maps_url(&t).contains("destination_place_id=ChIJ1"));
        assert!(trip().request_body()["destination"].get("placeId").is_none());
        // Trips saved before place IDs existed still load.
        let old: Trip = serde_json::from_str(r#"{"destination":"X","origin":"Y"}"#).unwrap();
        assert_eq!(old.place_id, None);
    }
}
