//! Weather from Open-Meteo (open-meteo.com; no key, free for non-commercial
//! use, data under CC BY 4.0 with attribution). Request building and
//! parsing here; the island fetches once an hour, only while it is on.

use crate::prefs::Place;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sky {
    Clear,
    PartlyCloudy,
    Cloudy,
    Fog,
    Rain,
    Snow,
    Storm,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Weather {
    pub place: String,
    pub temp_c: f64,
    pub high_c: Option<f64>,
    pub low_c: Option<f64>,
    pub sky: Sky,
    pub is_day: bool,
}

/// WMO weather interpretation codes, as Open-Meteo reports them.
pub fn sky(code: u32) -> Sky {
    match code {
        0 | 1 => Sky::Clear,
        2 => Sky::PartlyCloudy,
        3 => Sky::Cloudy,
        45 | 48 => Sky::Fog,
        51..=67 | 80..=82 => Sky::Rain,
        71..=77 | 85 | 86 => Sky::Snow,
        95..=99 => Sky::Storm,
        _ => Sky::Cloudy,
    }
}

pub fn describe(s: Sky) -> &'static str {
    match s {
        Sky::Clear => "Clear",
        Sky::PartlyCloudy => "Partly cloudy",
        Sky::Cloudy => "Cloudy",
        Sky::Fog => "Fog",
        Sky::Rain => "Rain",
        Sky::Snow => "Snow",
        Sky::Storm => "Thunderstorm",
    }
}

pub fn forecast_url(p: &Place) -> String {
    format!(
        "https://api.open-meteo.com/v1/forecast?latitude={:.4}&longitude={:.4}\
         &current=temperature_2m,weather_code,is_day&daily=temperature_2m_max,temperature_2m_min\
         &timezone=auto&forecast_days=1",
        p.lat, p.lon
    )
}

pub fn parse_forecast(json: &str, place: &str) -> Result<Weather, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("bad response: {e}"))?;
    if v.get("error").and_then(|e| e.as_bool()) == Some(true) {
        return Err(v["reason"].as_str().unwrap_or("request failed").to_string());
    }
    let cur = v.get("current").ok_or("no current weather")?;
    let first = |k: &str| v.pointer(&format!("/daily/{k}/0")).and_then(|x| x.as_f64());
    Ok(Weather {
        place: place.to_string(),
        temp_c: cur["temperature_2m"].as_f64().ok_or("no temperature")?,
        high_c: first("temperature_2m_max"),
        low_c: first("temperature_2m_min"),
        sky: sky(cur["weather_code"].as_u64().unwrap_or(3) as u32),
        is_day: cur["is_day"].as_u64() != Some(0),
    })
}

/// Open-Meteo's geocoding search (a city name → places).
pub fn geocode_url(query: &str) -> String {
    let q: String = query
        .trim()
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
            b' ' => "+".into(),
            _ => format!("%{b:02X}"),
        })
        .collect();
    format!("https://geocoding-api.open-meteo.com/v1/search?name={q}&count=5&language=en&format=json")
}

/// Places from a geocoding response, named "Kampala, Central Region, Uganda".
pub fn parse_places(json: &str) -> Result<Vec<Place>, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("bad response: {e}"))?;
    let Some(results) = v.get("results").and_then(|r| r.as_array()) else { return Ok(Vec::new()) };
    Ok(results
        .iter()
        .filter_map(|r| {
            let parts: Vec<&str> =
                ["name", "admin1", "country"].iter().filter_map(|k| r.get(*k).and_then(|x| x.as_str())).collect();
            let mut name = Vec::new();
            for p in parts {
                if !name.contains(&p) {
                    name.push(p);
                }
            }
            Some(Place { name: name.join(", "), lat: r.get("latitude")?.as_f64()?, lon: r.get("longitude")?.as_f64()? })
        })
        .collect())
}

/// "24°" in Celsius or Fahrenheit.
pub fn format_temp(c: f64, fahrenheit: bool) -> String {
    let v = if fahrenheit { c * 9.0 / 5.0 + 32.0 } else { c };
    format!("{}°", v.round() as i64)
}

/// Refresh hourly while weather is on.
pub const REFRESH_SECS: u64 = 3600;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_forecast() {
        let json = r#"{"current":{"temperature_2m":24.3,"weather_code":61,"is_day":1},
            "daily":{"temperature_2m_max":[27.1],"temperature_2m_min":[18.4]}}"#;
        let w = parse_forecast(json, "Kampala").unwrap();
        assert_eq!((w.temp_c, w.sky, w.is_day), (24.3, Sky::Rain, true));
        assert_eq!((w.high_c, w.low_c), (Some(27.1), Some(18.4)));
        assert!(parse_forecast(r#"{"error":true,"reason":"Latitude must be in range"}"#, "x")
            .unwrap_err()
            .contains("Latitude"));
    }

    #[test]
    fn geocodes_and_formats() {
        let json = r#"{"results":[{"name":"Kampala","latitude":0.3163,"longitude":32.5822,"country":"Uganda","admin1":"Central Region"},
            {"name":"Paris","latitude":48.85,"longitude":2.35,"country":"France","admin1":"Paris"}]}"#;
        let p = parse_places(json).unwrap();
        assert_eq!(p[0].name, "Kampala, Central Region, Uganda");
        assert_eq!(p[1].name, "Paris, France", "repeated parts are dropped");
        assert!(parse_places("{}").unwrap().is_empty());
        assert_eq!(
            geocode_url("São Paulo"),
            "https://geocoding-api.open-meteo.com/v1/search?name=S%C3%A3o+Paulo&count=5&language=en&format=json"
        );
        assert_eq!(format_temp(24.3, false), "24°");
        assert_eq!(format_temp(24.3, true), "76°");
        assert_eq!(sky(95), Sky::Storm);
        assert!(forecast_url(&Place { name: "x".into(), lat: 0.31634, lon: 32.58219 })
            .contains("latitude=0.3163&longitude=32.5822&current="));
    }
}
