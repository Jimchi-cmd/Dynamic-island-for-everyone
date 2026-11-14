//! Airplane mode and Do Not Disturb, as the system reports them. The
//! readers of each system's raw data are here, where they can be tested;
//! the watchers are in the island's `providers::modes`.

use std::collections::BTreeMap;

/// One Linux rfkill event, as read from /dev/rfkill (the 8-byte v1 form:
/// index, type, operation, soft block, hard block).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RfkillEvent {
    pub idx: u32,
    pub op: u8,
    pub blocked: bool,
}

pub const RFKILL_OP_ADD: u8 = 0;
pub const RFKILL_OP_DEL: u8 = 1;
pub const RFKILL_OP_CHANGE: u8 = 2;

pub fn parse_rfkill(buf: &[u8]) -> Option<RfkillEvent> {
    let b = buf.get(..8)?;
    let idx = u32::from_ne_bytes([b[0], b[1], b[2], b[3]]);
    Some(RfkillEvent { idx, op: b[5], blocked: b[6] != 0 || b[7] != 0 })
}

/// The radios rfkill knows of, and whether each is switched off.
#[derive(Debug, Default, Clone)]
pub struct Radios(BTreeMap<u32, bool>);

impl Radios {
    pub fn apply(&mut self, e: RfkillEvent) {
        match e.op {
            RFKILL_OP_ADD | RFKILL_OP_CHANGE => {
                self.0.insert(e.idx, e.blocked);
            }
            RFKILL_OP_DEL => {
                self.0.remove(&e.idx);
            }
            _ => {}
        }
    }

    /// Airplane mode: every radio switched off (what GNOME and KDE's
    /// switches do). None with no radios at all (a desktop without any).
    pub fn airplane(&self) -> Option<bool> {
        (!self.0.is_empty()).then(|| self.0.values().all(|b| *b))
    }
}

/// GNOME's `show-banners` setting ("true"/"false", or from `gsettings
/// monitor`, "show-banners: false"): banners off is Do Not Disturb.
pub fn gnome_dnd(line: &str) -> Option<bool> {
    match line.trim().rsplit(':').next()?.trim() {
        "false" => Some(true),
        "true" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(idx: u32, op: u8, soft: u8, hard: u8) -> [u8; 8] {
        let i = idx.to_ne_bytes();
        [i[0], i[1], i[2], i[3], 1, op, soft, hard]
    }

    #[test]
    fn airplane_mode_is_every_radio_off() {
        let mut r = Radios::default();
        assert_eq!(r.airplane(), None, "no radios: can't tell");
        r.apply(parse_rfkill(&ev(0, RFKILL_OP_ADD, 0, 0)).unwrap());
        r.apply(parse_rfkill(&ev(1, RFKILL_OP_ADD, 1, 0)).unwrap());
        assert_eq!(r.airplane(), Some(false), "Bluetooth off alone isn't airplane mode");
        r.apply(parse_rfkill(&ev(0, RFKILL_OP_CHANGE, 1, 0)).unwrap());
        assert_eq!(r.airplane(), Some(true));
        r.apply(parse_rfkill(&ev(1, RFKILL_OP_CHANGE, 0, 1)).unwrap());
        assert_eq!(r.airplane(), Some(true), "a hardware switch counts too");
        r.apply(parse_rfkill(&ev(2, RFKILL_OP_ADD, 0, 0)).unwrap());
        assert_eq!(r.airplane(), Some(false), "a new radio that's on");
        r.apply(parse_rfkill(&ev(2, RFKILL_OP_DEL, 0, 0)).unwrap());
        assert_eq!(r.airplane(), Some(true));
        assert_eq!(parse_rfkill(&[1, 2, 3]), None);
    }

    #[test]
    fn gnome_banners_off_is_do_not_disturb() {
        assert_eq!(gnome_dnd("false\n"), Some(true));
        assert_eq!(gnome_dnd("show-banners: true"), Some(false));
        assert_eq!(gnome_dnd("No such schema"), None);
    }
}
