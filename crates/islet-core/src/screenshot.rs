//! The screenshot preview (Islet Pro): when the system saves a screenshot,
//! the island shows it for a few seconds; a tap opens it. Islet only looks
//! at the folder the system saves into, and reads the new picture to draw
//! its thumbnail. Nothing leaves the computer.

/// The preview is a script-style activity with this id.
pub const ACTIVITY_ID: &str = "islet.screenshot";
/// It shows this long.
pub const SHOWN_SECS: u64 = 8;

/// How macOS names a screenshot, in the languages it ships in most.
const MAC_NAMES: [&str; 9] = [
    "Screenshot",
    "Screen Shot",
    "Bildschirmfoto",
    "Capture d’écran",
    "Captura de pantalla",
    "Captura de Tela",
    "Schermafbeelding",
    "Istantanea",
    "Schermata",
];

/// Is a new file with this name a screenshot? `dedicated`: the folder
/// holds nothing but screenshots (Linux, Windows), so any picture counts;
/// otherwise (the macOS Desktop) the name has to say so.
pub fn is_screenshot(name: &str, dedicated: bool) -> bool {
    let lower = name.to_lowercase();
    let picture = [".png", ".jpg", ".jpeg"].iter().any(|ext| lower.ends_with(ext));
    // Hidden and half-written files don't count.
    if !picture || name.starts_with('.') {
        return false;
    }
    dedicated || MAC_NAMES.iter().any(|n| name.starts_with(n))
}

/// The file's name without its folder, for the card.
pub fn display_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_screenshots() {
        assert!(is_screenshot("Screenshot From 2026-11-04 14-05-09.png", true));
        assert!(is_screenshot("Screenshot 2026-11-04 140509.PNG", true));
        assert!(is_screenshot("anything.jpg", true), "a folder of screenshots holds only screenshots");
        assert!(!is_screenshot("notes.txt", true));
        assert!(!is_screenshot(".Screenshot 2026-11-04.png", true), "still being written");
        // On the macOS Desktop only files named as screenshots count.
        assert!(is_screenshot("Screenshot 2026-11-04 at 14.05.09.png", false));
        assert!(is_screenshot("Bildschirmfoto 2026-11-04 um 14.05.09.png", false));
        assert!(!is_screenshot("holiday.png", false));
        assert_eq!(display_name("/home/a/Pictures/Screenshots/Shot 1.png"), "Shot 1.png");
        assert_eq!(display_name("C:\\Users\\a\\Pictures\\Screenshots\\Shot 1.png"), "Shot 1.png");
    }
}
