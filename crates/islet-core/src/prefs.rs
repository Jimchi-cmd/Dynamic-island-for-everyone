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
    /// Show Spotify Connect playback (needs `islet spotify login`).
    pub spotify: bool,
    /// Green/orange dots while the camera or microphone is in use.
    pub privacy_dots: bool,
    /// Keep the last few copied texts (in memory only).
    pub clipboard: bool,
    /// Show synced lyrics for the playing song (fetched from LRCLIB).
    pub lyrics: bool,
    /// Hide the empty island until the pointer is over it. Anything live
    /// (music, a timer, a notification…) still shows.
    pub auto_hide: bool,
    /// The time on the empty island (when nothing is playing).
    pub idle_clock: bool,
    /// A call activity (with a mute button) while an app uses the microphone.
    pub calls: bool,
    /// Downloads in progress, from the Downloads folder.
    pub downloads: bool,
    /// The global shortcut that opens and closes the island ("" = none),
    /// e.g. "Ctrl+Alt+I".
    pub hotkey: String,
    /// Accent colour for buttons and highlights (see `ACCENTS`).
    pub accent: String,
    /// Island size: "small", "medium" or "large" (applies on restart).
    pub size: String,
    /// Gap between the top of the screen and the island, in px (0–40;
    /// applies on restart; ignored on notched Macs, where it hangs from the notch).
    pub offset: u32,
    /// The display to show the island on, by name ("" = the primary one,
    /// `ALL_DISPLAYS` = one island on every display).
    pub display: String,
}

/// Accent colours: name and RGB.
pub const ACCENTS: [(&str, [u8; 3]); 7] = [
    ("blue", [10, 132, 255]),
    ("purple", [160, 100, 255]),
    ("pink", [255, 70, 140]),
    ("red", [255, 69, 58]),
    ("orange", [255, 150, 30]),
    ("green", [40, 200, 100]),
    ("teal", [40, 190, 200]),
];

/// `display` value for an island on every display.
pub const ALL_DISPLAYS: &str = "all";

pub const SIZES: [(&str, f64); 3] = [("small", 0.88), ("medium", 1.0), ("large", 1.15)];
pub const MAX_OFFSET: u32 = 40;

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
            spotify: false,
            privacy_dots: true,
            clipboard: false,
            lyrics: false,
            auto_hide: false,
            idle_clock: true,
            calls: true,
            downloads: false,
            hotkey: String::new(),
            accent: "blue".into(),
            size: "medium".into(),
            offset: 0,
            display: String::new(),
        }
    }
}

/// The switches `islet set <key> on|off` understands.
pub const SWITCHES: [&str; 15] = [
    "volume-hud",
    "brightness-hud",
    "caps-lock-hud",
    "notifications",
    "fahrenheit",
    "calendar",
    "web-artwork",
    "spotify",
    "privacy-dots",
    "clipboard",
    "lyrics",
    "auto-hide",
    "idle-clock",
    "calls",
    "downloads",
];

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
        *self.slot(key)? = on;
        Ok(())
    }

    /// A switch's state (see `SWITCHES`).
    pub fn switch(&self, key: &str) -> Option<bool> {
        self.clone().slot(key).ok().map(|v| *v)
    }

    fn slot(&mut self, key: &str) -> Result<&mut bool, String> {
        Ok(match key {
            "volume-hud" => &mut self.volume_hud,
            "brightness-hud" => &mut self.brightness_hud,
            "caps-lock-hud" => &mut self.caps_lock_hud,
            "notifications" => &mut self.notifications,
            "fahrenheit" => &mut self.fahrenheit,
            "calendar" => &mut self.calendar,
            "web-artwork" => &mut self.web_artwork,
            "spotify" => &mut self.spotify,
            "privacy-dots" => &mut self.privacy_dots,
            "clipboard" => &mut self.clipboard,
            "lyrics" => &mut self.lyrics,
            "auto-hide" => &mut self.auto_hide,
            "idle-clock" => &mut self.idle_clock,
            "calls" => &mut self.calls,
            "downloads" => &mut self.downloads,
            _ => return Err(format!("unknown setting {key:?} (one of: {})", SWITCHES.join(", "))),
        })
    }
}

impl IslandPrefs {
    pub fn accent_rgb(&self) -> [u8; 3] {
        ACCENTS.iter().find(|(n, _)| n.eq_ignore_ascii_case(&self.accent)).map_or(ACCENTS[0].1, |(_, c)| *c)
    }

    /// How much bigger (or smaller) than normal the island is drawn.
    pub fn size_factor(&self) -> f64 {
        SIZES.iter().find(|(n, _)| n.eq_ignore_ascii_case(&self.size)).map_or(1.0, |(_, k)| *k)
    }

    pub fn top_offset(&self) -> f64 {
        f64::from(self.offset.min(MAX_OFFSET))
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
    fn look_and_layout() {
        let mut p = IslandPrefs::default();
        assert_eq!((p.accent_rgb(), p.size_factor(), p.top_offset()), ([10, 132, 255], 1.0, 0.0));
        p.accent = "Green".into();
        p.size = "large".into();
        p.offset = 500;
        assert_eq!((p.accent_rgb(), p.size_factor(), p.top_offset()), ([40, 200, 100], 1.15, 40.0));
        p.accent = "plaid".into();
        assert_eq!(p.accent_rgb(), [10, 132, 255], "unknown names fall back to blue");
        // Older island.json files without these keys still load.
        let old: IslandPrefs = serde_json::from_str(r#"{"volumeHud":false}"#).unwrap();
        assert_eq!((old.accent.as_str(), old.size.as_str(), old.offset), ("blue", "medium", 0));
    }

    #[test]
    fn private_by_default_and_tolerant_of_old_files() {
        let p: IslandPrefs = serde_json::from_str("{}").unwrap();
        assert!(p.volume_hud && !p.notifications && p.weather.is_none() && !p.calendar && !p.lyrics && !p.auto_hide);
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
        assert_eq!(p.switch("caps-lock-hud"), Some(false));
        assert_eq!(p.switch("rocket"), None);
        p.set_switch("auto-hide", true).unwrap();
        assert!(p.auto_hide && SWITCHES.iter().all(|k| p.switch(k).is_some()));
        assert_eq!(parse_on_off("ON"), Some(true));
        assert_eq!(parse_on_off("maybe"), None);
    }
}
