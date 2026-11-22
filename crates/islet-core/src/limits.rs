//! How much Islet keeps and how smoothly it draws, by the Performance
//! setting (`MODES`):
//! - "full": long histories, sharp album art, and a waveform at 60 fps;
//! - "light": tuned for a 4 GB laptop: short histories, small album art,
//!   and a waveform at 30 fps;
//! - "saver": light, and nothing on the island moves on its own;
//! - "auto" (the default): full, or light on a low-end computer.

/// The caps for one mode. Every list still has an end, so a runaway script
/// can't fill the memory; with `FULL` it is far past what anyone reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Clipboard entries kept, and characters kept of each.
    pub clipboard_items: usize,
    pub clipboard_chars: usize,
    /// Files on the shelf.
    pub shelf: usize,
    /// Notifications remembered.
    pub notes: usize,
    /// Album art is stored as a square of this many pixels.
    pub art_px: u32,
    /// Frames a second for things that move on their own (the waveform, a
    /// ringing bell, a spinning ring). 0: they stand still.
    pub ambient_fps: u32,
}

/// Light: for a 4 GB laptop.
pub const LIGHT: Limits =
    Limits { clipboard_items: 20, clipboard_chars: 10_000, shelf: 12, notes: 20, art_px: 112, ambient_fps: 30 };

/// Full: no low-end limits.
pub const FULL: Limits =
    Limits { clipboard_items: 500, clipboard_chars: 100_000, shelf: 100, notes: 200, art_px: 256, ambient_fps: 60 };

/// Power saver: as light as Light, and still. The shelf keeps its room, so
/// switching this on never drops a file.
pub const SAVER: Limits = Limits { ambient_fps: 0, shelf: FULL.shelf, ..LIGHT };

/// The Performance setting's choices, as `islet performance <mode>` takes them.
pub const MODES: [&str; 4] = ["auto", "full", "light", "saver"];

/// A computer Islet treats as low-end in "auto": 4 GB of memory or less, or
/// two cores or fewer. Memory it couldn't read (0) doesn't count against it.
pub const fn is_low_end(memory_bytes: u64, cores: usize) -> bool {
    let four_and_a_bit_gb = 4_608 * 1024 * 1024;
    (memory_bytes > 0 && memory_bytes <= four_and_a_bit_gb) || cores <= 2
}

impl Limits {
    /// The limits for a Performance mode. Unknown modes count as "auto".
    pub fn pick(mode: &str, low_end: bool) -> Self {
        match mode {
            "full" => FULL,
            "light" => LIGHT,
            "saver" => SAVER,
            _ if low_end => LIGHT,
            _ => FULL,
        }
    }

    /// Microseconds between ambient frames; None when nothing moves.
    pub const fn ambient_frame_micros(&self) -> Option<u64> {
        if self.ambient_fps == 0 {
            None
        } else {
            // Rounded, so 60 fps is the same 16,667 µs the shape animates at.
            let fps = self.ambient_fps as u64;
            Some((1_000_000 + fps / 2) / fps)
        }
    }
}

impl Default for Limits {
    fn default() -> Self {
        FULL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_lifts_every_limit_and_auto_follows_the_computer() {
        let (light, full) = (Limits::pick("light", false), Limits::pick("full", true));
        assert_eq!((light, full), (LIGHT, FULL), "a chosen mode is kept whatever the computer");
        assert!(full.clipboard_items > light.clipboard_items && full.clipboard_chars > light.clipboard_chars);
        assert!(full.shelf > light.shelf && full.notes > light.notes);
        assert!(full.art_px > light.art_px && full.ambient_fps > light.ambient_fps);
        assert_eq!(Limits::pick("auto", false), FULL);
        assert_eq!(Limits::pick("auto", true), LIGHT, "auto is light on a low-end computer");
        assert_eq!(Limits::pick("nonsense", false), FULL);
        assert_eq!(Limits::default(), FULL);
    }

    #[test]
    fn power_saver_holds_the_island_still() {
        let saver = Limits::pick("saver", false);
        assert_eq!(saver.ambient_fps, 0);
        assert_eq!(saver.ambient_frame_micros(), None, "nothing wakes the computer");
        assert_eq!((saver.clipboard_items, saver.art_px), (LIGHT.clipboard_items, LIGHT.art_px));
        assert_eq!(saver.shelf, FULL.shelf, "files already on the shelf stay");
    }

    #[test]
    fn low_end_computers() {
        const GB: u64 = 1024 * 1024 * 1024;
        assert!(is_low_end(4 * GB, 4), "4 GB");
        assert!(is_low_end(16 * GB, 2), "two cores");
        assert!(!is_low_end(8 * GB, 4));
        assert!(!is_low_end(0, 4), "unknown memory doesn't count");
    }

    #[test]
    fn frame_times() {
        assert_eq!(LIGHT.ambient_frame_micros(), Some(33_333));
        assert_eq!(FULL.ambient_frame_micros(), Some(16_667));
    }
}
