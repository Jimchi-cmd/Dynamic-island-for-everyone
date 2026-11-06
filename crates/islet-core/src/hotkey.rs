//! The global shortcut that opens and closes the island, e.g. "Ctrl+Alt+I".
//! Off unless the user picks one.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hotkey {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// Windows key / Command / Super.
    pub logo: bool,
    pub key: Key,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// A–Z (upper case) or 0–9.
    Char(char),
    Space,
    /// F1–F12.
    F(u8),
}

impl Hotkey {
    /// Parse "Ctrl+Alt+I", "Shift+Cmd+Space", "Super+F5"… Needs at least one
    /// of Ctrl, Alt or the logo key, so typing can never trigger it.
    pub fn parse(s: &str) -> Result<Hotkey, String> {
        let mut hk = Hotkey { ctrl: false, alt: false, shift: false, logo: false, key: Key::Space };
        let mut key = None;
        for part in s.split('+').map(str::trim).filter(|p| !p.is_empty()) {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => hk.ctrl = true,
                "alt" | "option" | "opt" => hk.alt = true,
                "shift" => hk.shift = true,
                "super" | "win" | "windows" | "cmd" | "command" | "meta" | "logo" => hk.logo = true,
                "space" => key = Some(Key::Space),
                p if p.len() > 1 && p.starts_with('f') && p[1..].parse::<u8>().is_ok_and(|n| (1..=12).contains(&n)) => {
                    key = Some(Key::F(p[1..].parse().unwrap_or(1)))
                }
                p if p.chars().count() == 1 && p.chars().all(|c| c.is_ascii_alphanumeric()) => {
                    key = Some(Key::Char(p.chars().next().unwrap_or('I').to_ascii_uppercase()))
                }
                _ => return Err(format!("unknown key {part:?} in {s:?}")),
            }
        }
        hk.key = key.ok_or_else(|| format!("no key in {s:?} (e.g. Ctrl+Alt+I)"))?;
        if !(hk.ctrl || hk.alt || hk.logo) {
            return Err("a shortcut needs Ctrl, Alt or the Windows/Command key".into());
        }
        Ok(hk)
    }

    /// The canonical spelling, e.g. "Ctrl+Alt+I".
    pub fn label(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        for (on, name) in [(self.ctrl, "Ctrl"), (self.alt, "Alt"), (self.shift, "Shift"), (self.logo, "Super")] {
            if on {
                parts.push(name.into());
            }
        }
        parts.push(match self.key {
            Key::Char(c) => c.to_string(),
            Key::Space => "Space".into(),
            Key::F(n) => format!("F{n}"),
        });
        parts.join("+")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_shortcuts() {
        let hk = Hotkey::parse("ctrl + alt + i").unwrap();
        assert_eq!(hk, Hotkey { ctrl: true, alt: true, shift: false, logo: false, key: Key::Char('I') });
        assert_eq!(hk.label(), "Ctrl+Alt+I");
        assert_eq!(Hotkey::parse("Cmd+Shift+Space").unwrap().label(), "Shift+Super+Space");
        assert_eq!(Hotkey::parse("Super+F5").unwrap().key, Key::F(5));
        assert!(Hotkey::parse("Shift+A").is_err(), "typing must never trigger it");
        assert!(Hotkey::parse("Ctrl+Alt").is_err());
        assert!(Hotkey::parse("Ctrl+F13").is_err());
        assert!(Hotkey::parse("Ctrl+Banana").is_err());
    }
}
