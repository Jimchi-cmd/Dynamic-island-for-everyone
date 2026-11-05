//! The island's preferences (island.json in the config folder), edited by
//! Settings through `islet set …`. Everything that uses the network or reads
//! other apps' content is off until the user turns it on.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Place {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct IslandPrefs {
    pub volume_hud: bool,
    pub brightness_hud: bool,
    pub caps_lock_hud: bool,
    /// Mirror desktop notifications on the island (Linux).
    pub notifications: bool,
    /// Weather for this place (None: weather off).
    pub weather: Option<Place>,
    /// Fahrenheit instead of Celsius.
    pub fahrenheit: bool,
    /// Show upcoming events from the calendar link in the keychain.
    pub calendar: bool,
    /// Apps whose Now Playing the island ignores (names or ids).
    pub media_exclude: Vec<String>,
    /// Download artwork that players publish as web links (Linux; e.g.
    /// Spotify's covers). Off: only local artwork files are read.
    pub web_artwork: bool,
}

impl Default for IslandPrefs {
    fn default() -> Self {
        Self {
            volume_hud: true,
            brightness_hud: true,
            caps_lock_hud: true,
            notifications: false,
            weather: None,
            fahrenheit: false,
            calendar: false,
            media_exclude: Vec::new(),
            web_artwork: false,
        }
    }
}

/// The switches `islet set <key> on|off` understands.
pub const SWITCHES: [&str; 7] =
    ["volume-hud", "brightness-hud", "caps-lock-hud", "notifications", "fahrenheit", "calendar", "web-artwork"];

impl IslandPrefs {
    /// Is this media session from an app the user hid?
    pub fn excludes(&self, app_name: &str, source: &str) -> bool {
        let (name, source) = (app_name.to_lowercase(), source.to_lowercase());
        self.media_exclude
            .iter()
            .map(|x| x.trim().to_lowercase())
            .filter(|x| !x.is_empty())
            .any(|x| name == x || source.contains(&x))
    }

    pub fn set_switch(&mut self, key: &str, on: bool) -> Result<(), String> {
        let slot = match key {
            "volume-hud" => &mut self.volume_hud,
            "brightness-hud" => &mut self.brightness_hud,
            "caps-lock-hud" => &mut self.caps_lock_hud,
            "notifications" => &mut self.notifications,
            "fahrenheit" => &mut self.fahrenheit,
            "calendar" => &mut self.calendar,
            "web-artwork" => &mut self.web_artwork,
            _ => return Err(format!("unknown setting {key:?} (one of: {})", SWITCHES.join(", "))),
        };
        *slot = on;
        Ok(())
    }
}

pub fn parse_on_off(v: &str) -> Option<bool> {
    match v.to_ascii_lowercase().as_str() {
        "on" | "true" | "yes" | "1" => Some(true),
        "off" | "false" | "no" | "0" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_by_default_and_tolerant_of_old_files() {
        let p: IslandPrefs = serde_json::from_str("{}").unwrap();
        assert!(p.volume_hud && !p.notifications && p.weather.is_none() && !p.calendar);
        let p: IslandPrefs = serde_json::from_str(r#"{"volumeHud":false,"unknownKey":1}"#).unwrap();
        assert!(!p.volume_hud);
    }

    #[test]
    fn media_exclusions_match_names_and_ids() {
        let p = IslandPrefs { media_exclude: vec!["Firefox".into(), "com.spotify".into()], ..IslandPrefs::default() };
        assert!(p.excludes("firefox", "org.mpris.MediaPlayer2.firefox.instance_1_23"));
        assert!(p.excludes("Spotify", "com.spotify.client"));
        assert!(!p.excludes("VLC", "org.mpris.MediaPlayer2.vlc"));
    }

    #[test]
    fn switches() {
        let mut p = IslandPrefs::default();
        p.set_switch("caps-lock-hud", false).unwrap();
        assert!(!p.caps_lock_hud);
        assert!(p.set_switch("rocket", true).is_err());
        assert_eq!(parse_on_off("ON"), Some(true));
        assert_eq!(parse_on_off("maybe"), None);
    }
}
