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
        }
    }
}

/// The switches `islet set <key> on|off` understands.
pub const SWITCHES: [&str; 6] =
    ["volume-hud", "brightness-hud", "caps-lock-hud", "notifications", "fahrenheit", "calendar"];

impl IslandPrefs {
    pub fn set_switch(&mut self, key: &str, on: bool) -> Result<(), String> {
        let slot = match key {
            "volume-hud" => &mut self.volume_hud,
            "brightness-hud" => &mut self.brightness_hud,
            "caps-lock-hud" => &mut self.caps_lock_hud,
            "notifications" => &mut self.notifications,
            "fahrenheit" => &mut self.fahrenheit,
            "calendar" => &mut self.calendar,
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
    fn switches() {
        let mut p = IslandPrefs::default();
        p.set_switch("caps-lock-hud", false).unwrap();
        assert!(!p.caps_lock_hud);
        assert!(p.set_switch("rocket", true).is_err());
        assert_eq!(parse_on_off("ON"), Some(true));
        assert_eq!(parse_on_off("maybe"), None);
    }
}
