//! The island's presentation state machine (docs/design-spec.md §5).
//!
//! Pure logic: signals in, presentation out. The app turns presentation
//! changes into spring targets; timers (long-press, auto-collapse) are
//! measured by the app and delivered here as signals.

use crate::spring::Spring;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Presentation {
    /// Nothing live: the small pill (or nothing, on a notched Mac).
    Idle,
    /// Pointer over an idle island: a slight grow that says "interactive".
    Peek,
    /// One live activity: leading and trailing views beside the pill.
    Compact,
    /// Two or more live activities: a pill plus a detached circle.
    Minimal,
    /// The full card for one activity.
    Expanded,
    /// The biggest view (click-and-hold).
    Large,
}

impl Presentation {
    pub fn is_open(self) -> bool {
        matches!(self, Self::Expanded | Self::Large)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Signal {
    PointerEnter,
    PointerLeave,
    /// Primary click on the island (press and release without a long press).
    Click,
    /// The pointer has been held down on the island for the long-press time.
    LongPress,
    /// Click anywhere outside the island.
    ClickOutside,
    Escape,
    /// The pointer left an open island and stayed away for the auto-collapse time.
    AutoCollapse,
    /// The number of live activities changed.
    LiveActivities(usize),
}

/// Long-press threshold.
pub const LONG_PRESS_SECS: f64 = 0.45;
/// An open island collapses this long after the pointer leaves it.
pub const AUTO_COLLAPSE_SECS: f64 = 5.0;

#[derive(Debug, Clone)]
pub struct Island {
    presentation: Presentation,
    live: usize,
    hovered: bool,
}

impl Default for Island {
    fn default() -> Self {
        Self::new()
    }
}

impl Island {
    pub fn new() -> Self {
        Self { presentation: Presentation::Idle, live: 0, hovered: false }
    }

    pub fn presentation(&self) -> Presentation {
        self.presentation
    }

    pub fn live(&self) -> usize {
        self.live
    }

    /// The resting presentation for the current number of live activities.
    fn base(&self) -> Presentation {
        match self.live {
            0 if self.hovered => Presentation::Peek,
            0 => Presentation::Idle,
            1 => Presentation::Compact,
            _ => Presentation::Minimal,
        }
    }

    /// Apply a signal. Returns the new presentation if it changed.
    pub fn apply(&mut self, s: Signal) -> Option<Presentation> {
        use Presentation::*;
        let cur = self.presentation;
        let next = match s {
            Signal::PointerEnter => {
                self.hovered = true;
                if cur == Idle {
                    Peek
                } else {
                    cur
                }
            }
            Signal::PointerLeave => {
                self.hovered = false;
                if cur == Peek {
                    self.base()
                } else {
                    cur
                }
            }
            Signal::Click => {
                if cur.is_open() {
                    self.base()
                } else {
                    Expanded
                }
            }
            Signal::LongPress => Large,
            Signal::ClickOutside | Signal::Escape | Signal::AutoCollapse => {
                if cur.is_open() {
                    self.base()
                } else {
                    cur
                }
            }
            Signal::LiveActivities(n) => {
                self.live = n;
                if cur.is_open() {
                    cur
                } else {
                    self.base()
                }
            }
        };
        (next != cur).then(|| {
            self.presentation = next;
            next
        })
    }
}

/// Target outline for a presentation: (width, height, corner radius) in
/// logical px. `content_h` lets Expanded shrink to fit short content.
pub fn target_size(p: Presentation, content_h: Option<f64>) -> (f64, f64, f64) {
    match p {
        Presentation::Idle => (126.0, 37.0, 18.5),
        Presentation::Peek => (136.0, 39.0, 19.5),
        Presentation::Compact => (236.0, 37.0, 18.5),
        Presentation::Minimal => (162.0, 37.0, 18.5),
        Presentation::Expanded => (372.0, content_h.unwrap_or(160.0).clamp(84.0, 160.0), 44.0),
        Presentation::Large => (384.0, 230.0, 48.0),
    }
}

/// Diameter of the detached circle in Minimal and its gap from the pill.
pub const MINIMAL_CIRCLE: f64 = 37.0;
pub const MINIMAL_GAP: f64 = 8.0;

/// The spring for a transition (design-spec §4).
pub fn spring_for(from: Presentation, to: Presentation) -> Spring {
    use Presentation::*;
    match (from, to) {
        (_, Expanded | Large) if !from.is_open() => Spring::new(0.50, 0.76),
        (Expanded, Large) | (Large, Expanded) => Spring::new(0.50, 0.76),
        (Expanded | Large, _) => Spring::new(0.42, 0.88),
        (_, Minimal) | (Minimal, _) => Spring::new(0.55, 0.72),
        (_, Peek) | (Peek, _) => Spring::new(0.30, 0.80),
        _ => Spring::new(0.45, 0.80),
    }
}

#[cfg(test)]
mod tests {
    use super::Presentation::*;
    use super::*;

    #[test]
    fn hover_peeks_only_when_idle() {
        let mut i = Island::new();
        assert_eq!(i.apply(Signal::PointerEnter), Some(Peek));
        assert_eq!(i.apply(Signal::PointerLeave), Some(Idle));
        i.apply(Signal::LiveActivities(1));
        assert_eq!(i.apply(Signal::PointerEnter), None, "compact stays compact");
    }

    #[test]
    fn click_toggles_expanded_and_returns_to_base() {
        let mut i = Island::new();
        i.apply(Signal::LiveActivities(1));
        assert_eq!(i.presentation(), Compact);
        assert_eq!(i.apply(Signal::Click), Some(Expanded));
        assert_eq!(i.apply(Signal::Click), Some(Compact));
    }

    #[test]
    fn long_press_opens_large_and_escape_closes() {
        let mut i = Island::new();
        assert_eq!(i.apply(Signal::LongPress), Some(Large));
        assert_eq!(i.apply(Signal::Escape), Some(Idle));
    }

    #[test]
    fn closing_while_hovered_returns_to_peek() {
        let mut i = Island::new();
        i.apply(Signal::PointerEnter);
        i.apply(Signal::Click);
        assert_eq!(i.apply(Signal::Click), Some(Peek));
    }

    #[test]
    fn activities_change_resting_state_but_not_an_open_island() {
        let mut i = Island::new();
        assert_eq!(i.apply(Signal::LiveActivities(2)), Some(Minimal));
        assert_eq!(i.apply(Signal::LiveActivities(1)), Some(Compact));
        i.apply(Signal::Click);
        assert_eq!(i.apply(Signal::LiveActivities(0)), None, "stays expanded");
        assert_eq!(i.apply(Signal::AutoCollapse), Some(Idle));
    }

    #[test]
    fn outside_click_and_auto_collapse_ignore_closed_island() {
        let mut i = Island::new();
        assert_eq!(i.apply(Signal::ClickOutside), None);
        assert_eq!(i.apply(Signal::AutoCollapse), None);
    }

    #[test]
    fn sizes_follow_the_spec_and_fit_the_window() {
        for p in [Idle, Peek, Compact, Minimal, Expanded, Large] {
            let (w, h, r) = target_size(p, None);
            assert!(r <= w.min(h) / 2.0 + 1e-9, "{p:?} radius too large");
            // Window 420 x 260 must hold the state plus ~3 % overshoot.
            assert!(w * 1.03 <= 420.0 && h * 1.03 <= 260.0, "{p:?} does not fit");
        }
        assert_eq!(target_size(Expanded, Some(40.0)).1, 84.0);
        assert_eq!(target_size(Expanded, Some(120.0)).1, 120.0);
    }

    #[test]
    fn opening_springs_bounce_more_than_closing() {
        assert!(spring_for(Compact, Expanded).damping < spring_for(Expanded, Compact).damping);
        assert!(spring_for(Compact, Minimal).damping < 0.8, "the metaball split is bouncy");
    }
}
