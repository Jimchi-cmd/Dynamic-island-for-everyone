//! How much Islet keeps and how smoothly it draws.
//!
//! The free app is tuned for a 4 GB laptop: short histories, small album
//! art, and a waveform at 30 fps. Islet Pro lifts those limits, and has a
//! Power saver switch that goes the other way: the light limits again, and
//! nothing on the island moves on its own.

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

/// The free app.
pub const LIGHT: Limits =
    Limits { clipboard_items: 20, clipboard_chars: 10_000, shelf: 12, notes: 20, art_px: 112, ambient_fps: 30 };

/// Islet Pro.
pub const FULL: Limits =
    Limits { clipboard_items: 500, clipboard_chars: 100_000, shelf: 100, notes: 200, art_px: 256, ambient_fps: 60 };

/// Islet Pro with Power saver on: as light as the free app, and still.
/// The shelf keeps its room, so switching this on never drops a file.
pub const SAVER: Limits = Limits { ambient_fps: 0, shelf: FULL.shelf, ..LIGHT };

impl Limits {
    /// The limits in force. Power saver is a Pro switch: without Pro it
    /// does nothing, because the free app is already light.
    pub const fn pick(pro: bool, power_saver: bool) -> Self {
        match (pro, power_saver) {
            (false, _) => LIGHT,
            (true, false) => FULL,
            (true, true) => SAVER,
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
        LIGHT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pro_lifts_every_limit() {
        let (free, pro) = (Limits::pick(false, false), Limits::pick(true, false));
        assert_eq!(free, LIGHT);
        assert!(pro.clipboard_items > free.clipboard_items && pro.clipboard_chars > free.clipboard_chars);
        assert!(pro.shelf > free.shelf && pro.notes > free.notes);
        assert!(pro.art_px > free.art_px && pro.ambient_fps > free.ambient_fps);
    }

    #[test]
    fn power_saver_is_a_pro_switch() {
        assert_eq!(Limits::pick(false, true), LIGHT, "without Pro the switch changes nothing");
        let saver = Limits::pick(true, true);
        assert_eq!(saver.ambient_fps, 0);
        assert_eq!(saver.ambient_frame_micros(), None, "nothing wakes the computer");
        assert_eq!((saver.clipboard_items, saver.art_px), (LIGHT.clipboard_items, LIGHT.art_px));
        assert_eq!(saver.shelf, FULL.shelf, "files already on the shelf stay");
    }

    #[test]
    fn frame_times() {
        assert_eq!(LIGHT.ambient_frame_micros(), Some(33_333));
        assert_eq!(FULL.ambient_frame_micros(), Some(16_667));
    }
}
