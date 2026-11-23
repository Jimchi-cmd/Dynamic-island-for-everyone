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
    /// Keep the last few copied texts and pictures (in memory only).
    pub clipboard: bool,
    /// Apps in front of which the island steps aside (games, slides…),
    /// as it does over fullscreen windows.
    pub hide_with: Vec<String>,
    /// Apps whose copies are never kept (see `clipboard::excluded`).
    pub clipboard_exclude: Vec<String>,
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
    /// While a song plays, a border around the island in its cover's colour.
    pub album_border: bool,
    /// Show airplane mode and Do Not Disturb: a pop-up as they change and
    /// a small icon on the empty island while they're on.
    pub mode_indicators: bool,
    /// The user's own plugin widgets (scripts in the plugins folder).
    pub plugins: bool,
    /// A pop-up when Wi-Fi joins a network.
    pub wifi_popup: bool,
    /// Pointing at the empty island shows the weather, the battery and
    /// how busy the processor is, either side of the time.
    pub hover_status: bool,
    /// Listen for the paired phone (its notifications, music
    /// and battery). Set by `islet phone pair`; nothing listens without it.
    pub phone: bool,
    /// Show each new screenshot on the island for a moment.
    pub screenshot_preview: bool,
    /// A glow in the album's colour while music plays,
    /// and the progress bar drawn as moving bars.
    pub album_glow: bool,
    pub visualizer: bool,
    /// The island's outline, "pill" or "notch" (square at the
    /// top, as if it grew out of the screen's edge; see `SHAPES`).
    pub shape: String,
    /// What the empty island shows (see `CLOCK_FACES`).
    pub clock_face: String,
    /// 12- or 24-hour times: "auto" (as the system's language has it),
    /// "12" or "24" (see `CLOCK_HOURS`).
    pub clock_hours: String,
    /// The language Islet speaks: "auto" (the system's, when Islet has it)
    /// or a code from `i18n::LANGUAGES`.
    pub language: String,
    /// The island's text in a font installed on this computer ("" = the
    /// built-in one), and that font's files (read, never copied).
    pub font: String,
    pub font_files: Vec<String>,
    /// Say when one program hogs the processor or the memory, and offer
    /// to close it. Looks at the running programs every 20 seconds.
    pub performance_alert: bool,
    /// Hold pop-ups (notifications, volume, battery, calendar) while a
    /// focus session's work round runs; breaks let them through.
    pub focus_quiet: bool,
    /// How much Islet keeps and how smoothly it moves: "auto", "full",
    /// "light" or "saver" (see `limits::MODES`).
    pub performance: String,
    /// Older files had a Power saver switch instead; `migrated` turns it
    /// into `performance: "saver"`. Never written.
    #[serde(skip_serializing)]
    pub power_saver: bool,
    /// The global shortcut that opens and closes the island ("" = none),
    /// e.g. "Ctrl+Alt+I".
    pub hotkey: String,
    /// Shortcuts for the quick launcher and for a quick note ("" = none).
    pub launcher_hotkey: String,
    pub note_hotkey: String,
    /// Accent colour for buttons and highlights (see `ACCENTS`).
    pub accent: String,
    /// What the island is made of (see `STYLES`): "black", "tinted",
    /// "glass" or "tinted-glass".
    pub style: String,
    /// Island size: "small", "medium" or "large" (applies on restart).
    pub size: String,
    /// Gap between the top of the screen and the island, in px (0–40;
    /// applies on restart; ignored on notched Macs, where it hangs from the notch).
    pub offset: u32,
    /// Where on the display: "top", "top-left", "top-right" or "bottom"
    /// (see `geometry::Placement`; applies on restart).
    pub position: String,
    /// A size for particular displays (name → "small", "medium" or
    /// "large"), over `size`.
    pub display_sizes: std::collections::BTreeMap<String, String>,
    /// The display to show the island on, by name ("" = the primary one,
    /// `ALL_DISPLAYS` = one island on every display).
    pub display: String,
    /// World clocks on the clock card: city names from
    /// `worldclock::CITIES`, at most `worldclock::MAX`.
    pub world_clocks: Vec<String>,
    /// Your own buttons: each opens a folder, file, script or
    /// web page. At most `MAX_SHORTCUTS`.
    pub shortcuts: Vec<Shortcut>,
    /// Clock-card buttons the user switched off (ids from `CARD_BUTTONS`).
    pub card_hidden: Vec<String>,
    /// Buttons from `CARD_OPT_IN` the user switched on.
    pub card_shown: Vec<String>,
    /// How Control Centre is laid out (see `CC_STYLES`).
    pub cc_style: String,
    /// Control Centre's buttons in the user's order (ids from
    /// `CC_BUTTONS`; any left out follow in the usual order).
    pub cc_order: Vec<String>,
    /// A photo behind Control Centre (saved as `CC_PHOTO` in the config folder).
    pub cc_photo: bool,
    /// The colour of the tinted styles, "#rrggbb" ("" = the accent colour).
    pub tint: String,
}

/// Control Centre's layouts:
/// - "classic": the time, Wi-Fi and Bluetooth with their names, the tools in a row
/// - "modules": a block of round buttons, the timer beside it
/// - "sliders": volume and brightness as tall sliders beside a grid of buttons
/// - "dock": a big clock over a colour wash, every button in one labelled row
pub const CC_STYLES: [&str; 4] = ["classic", "modules", "sliders", "dock"];

/// Every Control Centre button, in the usual order.
pub const CC_BUTTONS: [&str; 13] = [
    "wifi",
    "bluetooth",
    "timer",
    "system",
    "controls",
    "calendar",
    "screenshot",
    "lock",
    "shelf",
    "todo",
    "stopwatch",
    "clipboard",
    "shortcuts",
];

/// The background photo's file in the config folder.
pub const CC_PHOTO: &str = "control-centre.png";

/// The colour in "#rrggbb" (or "rrggbb").
pub fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// The clock card's optional buttons: id and name. Wi-Fi and Bluetooth
/// always show.
pub const CARD_BUTTONS: [(&str, &str); 8] = [
    ("timer", "Quick timer"),
    ("calendar", "Calendar"),
    ("screenshot", "Screenshot"),
    ("lock", "Lock"),
    ("system", "System monitor"),
    ("controls", "Volume and brightness"),
    ("todo", "To-do list"),
    ("stopwatch", "Stopwatch"),
];

/// Clock-card buttons that show only once switched on in Settings: the
/// row keeps Wi-Fi's and Bluetooth's names with two tools beside them.
pub const CARD_OPT_IN: [&str; 2] = ["todo", "stopwatch"];

/// A button of your own: a name, and what it opens (a path or an
/// `https://` link). Paths open as they would from the file manager, so a
/// script or program runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shortcut {
    pub name: String,
    pub target: String,
}

pub const MAX_SHORTCUTS: usize = 50;

impl Shortcut {
    /// A shortcut from what the user typed, or why it can't be one.
    pub fn new(name: &str, target: &str) -> Result<Shortcut, String> {
        let (name, target) = (name.trim(), target.trim());
        if name.is_empty() || name.chars().count() > 40 {
            return Err(crate::i18n::t("Give the shortcut a short name.").into());
        }
        if target.is_empty() || target.len() > 1024 || target.starts_with('-') || target.contains(['\n', '\r', '\0']) {
            return Err(crate::i18n::t("Enter a folder, a file or an https:// link.").into());
        }
        // Links must be web links; anything else is a path on this computer.
        if target.contains("://") && !target.starts_with("https://") && !target.starts_with("http://") {
            return Err(crate::i18n::t("Only https:// and http:// links can be opened.").into());
        }
        Ok(Shortcut { name: name.to_string(), target: target.to_string() })
    }

    pub fn is_link(&self) -> bool {
        self.target.starts_with("https://") || self.target.starts_with("http://")
    }
}

/// Accent colours: name and RGB.
pub const ACCENTS: [(&str, [u8; 3]); 12] = [
    ("blue", [10, 132, 255]),
    ("indigo", [98, 96, 238]),
    ("purple", [160, 100, 255]),
    ("pink", [255, 70, 140]),
    ("red", [255, 69, 58]),
    ("orange", [255, 150, 30]),
    ("yellow", [250, 200, 20]),
    ("lime", [150, 215, 40]),
    ("green", [40, 200, 100]),
    ("mint", [70, 215, 180]),
    ("teal", [40, 190, 200]),
    ("graphite", [150, 152, 162]),
];

/// What the island is made of.
/// - "tinted": a deep shade of the accent colour.
/// - "glass": see-through dark glass with a bright rim.
/// - "tinted-glass": the same, coloured by the accent.
pub const STYLES: [&str; 4] = ["black", "tinted", "glass", "tinted-glass"];

/// A style's body: colour, how solid it is (0..=1) and whether it has the
/// glass sheen. Text stays white, so every body is dark enough to read on.
pub fn style_body(style: &str, accent: [u8; 3]) -> ([u8; 3], f32, bool) {
    // A deep shade: `share` of the colour over near-black, never so light
    // that white text stops reading on it (a custom tint can be white).
    let shade = |share: f32| {
        let c = accent.map(|c| f32::from(c) * share + 6.0);
        let lum = (0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]) / 255.0;
        let k = if lum > 0.23 { 0.23 / lum } else { 1.0 };
        c.map(|v| (v * k).round().min(255.0) as u8)
    };
    match style {
        "tinted" => (shade(0.26), 1.0, false),
        "glass" => ([22, 22, 26], 0.62, true),
        "tinted-glass" => (shade(0.22), 0.66, true),
        _ => ([0, 0, 0], 1.0, false),
    }
}

/// The island's outline. "notch" has square top corners and joins the
/// top of the screen, as if the island grew out of its edge.
pub const SHAPES: [&str; 2] = ["pill", "notch"];

/// What the empty island shows:
/// - "time": 14:05
/// - "date": Wed 4 · 14:05
/// - "seconds": 14:05:09 (redraws every second while it shows)
pub const CLOCK_FACES: [&str; 3] = ["time", "date", "seconds"];

/// 12- or 24-hour times: as the system's language has it, or always one.
pub const CLOCK_HOURS: [&str; 3] = ["auto", "12", "24"];

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
            hide_with: Vec::new(),
            clipboard_exclude: crate::clipboard::DEFAULT_EXCLUDE.iter().map(|s| s.to_string()).collect(),
            lyrics: false,
            auto_hide: false,
            idle_clock: true,
            calls: true,
            downloads: false,
            album_border: true,
            mode_indicators: true,
            plugins: false,
            performance: "auto".into(),
            power_saver: false,
            focus_quiet: false,
            hover_status: true,
            wifi_popup: true,
            performance_alert: false,
            phone: false,
            screenshot_preview: true,
            album_glow: true,
            visualizer: true,
            shape: "pill".into(),
            clock_face: "time".into(),
            clock_hours: "auto".into(),
            language: "auto".into(),
            font: String::new(),
            font_files: Vec::new(),
            hotkey: String::new(),
            launcher_hotkey: String::new(),
            note_hotkey: String::new(),
            accent: "blue".into(),
            style: "black".into(),
            size: "medium".into(),
            position: "top".into(),
            display_sizes: Default::default(),
            offset: 0,
            display: String::new(),
            world_clocks: Vec::new(),
            shortcuts: Vec::new(),
            card_hidden: Vec::new(),
            card_shown: Vec::new(),
            cc_style: "classic".into(),
            cc_order: Vec::new(),
            cc_photo: false,
            tint: String::new(),
        }
    }
}

/// The switches `islet set <key> on|off` understands.
pub const SWITCHES: [&str; 26] = [
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
    "focus-quiet",
    "hover-status",
    "performance-alert",
    "album-glow",
    "visualizer",
    "screenshot-preview",
    "phone",
    "wifi-popup",
    "plugins",
    "album-border",
    "mode-indicators",
];

impl IslandPrefs {
    /// Settings from an older island.json, brought up to date.
    pub fn migrated(mut self) -> Self {
        if std::mem::take(&mut self.power_saver) {
            self.performance = "saver".into();
        }
        self
    }

    /// 24-hour times? `locale_24h` is what the system's language prefers.
    pub fn h24(&self, locale_24h: bool) -> bool {
        match self.clock_hours.as_str() {
            "12" => false,
            "24" => true,
            _ => locale_24h,
        }
    }

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
            "album-border" => &mut self.album_border,
            "mode-indicators" => &mut self.mode_indicators,
            "plugins" => &mut self.plugins,
            "focus-quiet" => &mut self.focus_quiet,
            "hover-status" => &mut self.hover_status,
            "performance-alert" => &mut self.performance_alert,
            "album-glow" => &mut self.album_glow,
            "visualizer" => &mut self.visualizer,
            "screenshot-preview" => &mut self.screenshot_preview,
            "phone" => &mut self.phone,
            "wifi-popup" => &mut self.wifi_popup,
            _ => return Err(format!("unknown setting {key:?} (one of: {})", SWITCHES.join(", "))),
        })
    }
}

impl IslandPrefs {
    /// Did the user switch this clock-card button off?
    pub fn hides(&self, button: &str) -> bool {
        if CARD_OPT_IN.contains(&button) {
            !self.card_shown.iter().any(|b| b == button)
        } else {
            self.card_hidden.iter().any(|b| b == button)
        }
    }

    /// Show or hide a clock-card button (see `CARD_BUTTONS`).
    pub fn set_card_button(&mut self, button: &str, show: bool) -> Result<(), String> {
        let (id, _) = CARD_BUTTONS
            .iter()
            .find(|(id, _)| *id == button)
            .ok_or_else(|| format!("no such button: {button} (see: islet card)"))?;
        self.card_hidden.retain(|b| b != id);
        self.card_shown.retain(|b| b != id);
        if CARD_OPT_IN.contains(id) && show {
            self.card_shown.push((*id).to_string());
        } else if !CARD_OPT_IN.contains(id) && !show {
            self.card_hidden.push((*id).to_string());
        }
        Ok(())
    }

    /// The colour of the tinted styles: the user's own, else the accent.
    pub fn tint_rgb(&self) -> [u8; 3] {
        parse_hex(&self.tint).unwrap_or_else(|| self.accent_rgb())
    }

    /// Control Centre's buttons in the user's order; ones they left out
    /// follow in the usual order.
    pub fn cc_order(&self) -> Vec<&'static str> {
        let mut out: Vec<&'static str> = Vec::new();
        for id in &self.cc_order {
            if let Some(b) = CC_BUTTONS.iter().find(|b| **b == id.as_str()) {
                if !out.contains(b) {
                    out.push(b);
                }
            }
        }
        let rest: Vec<&'static str> = CC_BUTTONS.iter().copied().filter(|b| !out.contains(b)).collect();
        out.extend(rest);
        out
    }

    pub fn accent_rgb(&self) -> [u8; 3] {
        ACCENTS.iter().find(|(n, _)| n.eq_ignore_ascii_case(&self.accent)).map_or(ACCENTS[0].1, |(_, c)| *c)
    }

    /// How much bigger (or smaller) than normal the island is drawn.
    pub fn size_factor(&self) -> f64 {
        SIZES.iter().find(|(n, _)| n.eq_ignore_ascii_case(&self.size)).map_or(1.0, |(_, k)| *k)
    }

    /// The size on one display: its own, if it has one, else the usual.
    pub fn size_factor_on(&self, display: &str) -> f64 {
        match self.display_sizes.get(display) {
            Some(size) => SIZES.iter().find(|(n, _)| n.eq_ignore_ascii_case(size)).map_or(1.0, |(_, k)| *k),
            None => self.size_factor(),
        }
    }

    pub fn placement(&self) -> crate::geometry::Placement {
        crate::geometry::Placement::parse(&self.position).unwrap_or_default()
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
    fn each_display_can_have_its_own_size() {
        let mut p = IslandPrefs { size: "large".into(), ..IslandPrefs::default() };
        p.display_sizes.insert("DELL U2720Q".into(), "small".into());
        assert!((p.size_factor_on("DELL U2720Q") - 0.88).abs() < 1e-9);
        assert!((p.size_factor_on("Built-in display") - 1.15).abs() < 1e-9, "the others keep the usual size");
        assert_eq!(p.placement(), crate::geometry::Placement::Top);
        p.position = "bottom".into();
        assert_eq!(p.placement(), crate::geometry::Placement::Bottom);
        p.position = "nonsense".into();
        assert_eq!(p.placement(), crate::geometry::Placement::Top);
    }

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
    fn every_style_stays_dark_enough_for_white_text() {
        for (name, accent) in ACCENTS {
            for style in STYLES {
                let (c, alpha, glass) = style_body(style, accent);
                // Relative luminance of the body itself (the worst case for
                // glass is a white desktop behind it, checked below).
                let lum = |c: [u8; 3]| {
                    (0.2126 * f32::from(c[0]) + 0.7152 * f32::from(c[1]) + 0.0722 * f32::from(c[2])) / 255.0
                };
                assert!(lum(c) < 0.25, "{style} in {name} is too light: {c:?}");
                let over_white = lum(c) * alpha + (1.0 - alpha);
                assert!(over_white < 0.5, "{style} in {name} washes out on white: {over_white}");
                assert_eq!(glass, style.contains("glass"));
            }
        }
        assert_eq!(style_body("black", [255, 0, 0]), ([0, 0, 0], 1.0, false));
        assert_eq!(style_body("nonsense", [255, 0, 0]).0, [0, 0, 0], "unknown styles are black");
        assert_eq!(serde_json::from_str::<IslandPrefs>("{}").unwrap().style, "black");
    }

    #[test]
    fn a_tint_of_its_own_stays_dark_enough_and_falls_back_to_the_accent() {
        let mut p = IslandPrefs { accent: "green".into(), ..IslandPrefs::default() };
        assert_eq!(p.tint_rgb(), [40, 200, 100]);
        p.tint = "#FF00aa".into();
        assert_eq!(p.tint_rgb(), [255, 0, 170]);
        p.tint = "nonsense".into();
        assert_eq!(p.tint_rgb(), [40, 200, 100], "an unreadable tint uses the accent");
        for tint in [[255, 255, 255], [255, 255, 0], [0, 255, 0]] {
            let (c, _, _) = style_body("tinted", tint);
            let lum = (0.2126 * f32::from(c[0]) + 0.7152 * f32::from(c[1]) + 0.0722 * f32::from(c[2])) / 255.0;
            assert!(lum < 0.25, "{tint:?} gives {c:?}");
        }
        assert_eq!(parse_hex("3a7bd5"), Some([0x3a, 0x7b, 0xd5]));
        assert_eq!(parse_hex("#3a7bd"), None);
    }

    #[test]
    fn control_centre_keeps_every_button_in_the_users_order() {
        let p = IslandPrefs {
            cc_order: vec!["lock".into(), "x".into(), "lock".into(), "wifi".into()],
            ..IslandPrefs::default()
        };
        let o = p.cc_order();
        assert_eq!(&o[..3], &["lock", "wifi", "bluetooth"]);
        assert_eq!(o.len(), CC_BUTTONS.len());
        assert_eq!(IslandPrefs::default().cc_order(), CC_BUTTONS.to_vec());
        assert_eq!(IslandPrefs::default().cc_style, "classic");
    }

    #[test]
    fn shortcuts_take_paths_and_web_links_only() {
        let s = Shortcut::new("  Homework ", " /home/me/School ").unwrap();
        assert_eq!((s.name.as_str(), s.target.as_str(), s.is_link()), ("Homework", "/home/me/School", false));
        assert!(Shortcut::new("Docs", r"C:\Users\me\Documents").is_ok_and(|s| !s.is_link()));
        assert!(Shortcut::new("Mail", "https://mail.example.com/inbox").is_ok_and(|s| s.is_link()));
        // No other schemes, no option-looking targets, no empty names.
        for bad in ["javascript://alert(1)", "file:///etc/passwd", "ms-settings://x", "--help", "", "a\nb"] {
            assert!(Shortcut::new("x", bad).is_err(), "{bad:?}");
        }
        assert!(Shortcut::new(" ", "/tmp").is_err());
        assert!(Shortcut::new(&"n".repeat(41), "/tmp").is_err());
        // Old settings files have none.
        assert!(serde_json::from_str::<IslandPrefs>("{}").unwrap().shortcuts.is_empty());
    }

    #[test]
    fn media_exclusions_match_names_and_ids() {
        let p = IslandPrefs { media_exclude: vec!["Firefox".into(), "com.spotify".into()], ..IslandPrefs::default() };
        assert!(p.excludes("firefox", "org.mpris.MediaPlayer2.firefox.instance_1_23"));
        assert!(p.excludes("Spotify", "com.spotify.client"));
        assert!(!p.excludes("VLC", "org.mpris.MediaPlayer2.vlc"));
    }

    #[test]
    fn old_power_saver_becomes_a_performance_mode() {
        let old: IslandPrefs = serde_json::from_str(r#"{"powerSaver":true}"#).unwrap();
        let p = old.migrated();
        assert_eq!((p.performance.as_str(), p.power_saver), ("saver", false));
        assert!(!serde_json::to_string(&p).unwrap().contains("powerSaver"), "never written again");
        let fresh: IslandPrefs = serde_json::from_str("{}").unwrap();
        assert_eq!(fresh.migrated().performance, "auto");
    }

    #[test]
    fn twelve_or_twenty_four_hours() {
        let mut p = IslandPrefs::default();
        assert!(p.h24(true) && !p.h24(false), "auto follows the system");
        p.clock_hours = "12".into();
        assert!(!p.h24(true));
        p.clock_hours = "24".into();
        assert!(p.h24(false));
    }

    #[test]
    fn stopwatch_and_todo_buttons_wait_to_be_switched_on() {
        let mut p: IslandPrefs = serde_json::from_str(r#"{"cardHidden":[]}"#).unwrap();
        assert!(p.hides("todo") && p.hides("stopwatch") && !p.hides("timer"));
        p.set_card_button("todo", true).unwrap();
        p.set_card_button("timer", false).unwrap();
        assert!(!p.hides("todo") && p.hides("stopwatch") && p.hides("timer"));
        p.set_card_button("todo", false).unwrap();
        p.set_card_button("timer", true).unwrap();
        assert!(p.hides("todo") && !p.hides("timer"));
        assert!(p.card_hidden.is_empty() && p.card_shown.is_empty());
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
