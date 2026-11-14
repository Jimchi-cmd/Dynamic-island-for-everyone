//! Pure geometry helpers shared by all platforms (unit tested).

use serde::{Deserialize, Serialize};

/// A rectangle in logical (CSS) pixels, relative to the overlay window.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }

    pub fn right(&self) -> f64 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }

    pub fn contains(&self, px: f64, py: f64) -> bool {
        px >= self.x && px < self.right() && py >= self.y && py < self.bottom()
    }

    /// Smallest rect containing both.
    pub fn union(&self, o: &Rect) -> Rect {
        let x = self.x.min(o.x);
        let y = self.y.min(o.y);
        Rect { x, y, w: self.right().max(o.right()) - x, h: self.bottom().max(o.bottom()) - y }
    }

    /// Grow by `d` on every side.
    pub fn inflate(&self, d: f64) -> Rect {
        Rect { x: self.x - d, y: self.y - d, w: self.w + 2.0 * d, h: self.h + 2.0 * d }
    }
}

/// A rectangle in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    /// Convert to physical pixels, rounding outward so the region never
    /// clips the pill's anti-aliased edge.
    pub fn to_physical(self, scale: f64) -> PhysRect {
        let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
        let x0 = (self.x * scale).floor() as i32;
        let y0 = (self.y * scale).floor() as i32;
        let x1 = ((self.x + self.w.max(0.0)) * scale).ceil() as i32;
        let y1 = ((self.y + self.h.max(0.0)) * scale).ceil() as i32;
        PhysRect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
    }
}

/// Top-left physical position that centres a window horizontally at the top
/// of a monitor.
pub fn top_center(monitor_pos: (i32, i32), monitor_size: (u32, u32), window_width: u32) -> (i32, i32) {
    let free = monitor_size.0 as i64 - window_width as i64;
    (monitor_pos.0 + (free / 2) as i32, monitor_pos.1)
}

/// Where on its display the island sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Placement {
    /// Top centre, like the iPhone's.
    #[default]
    Top,
    TopLeft,
    TopRight,
    /// Bottom centre: it grows upward.
    Bottom,
}

impl Placement {
    pub const NAMES: [&'static str; 4] = ["top", "top-left", "top-right", "bottom"];

    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "top" => Self::Top,
            "top-left" => Self::TopLeft,
            "top-right" => Self::TopRight,
            "bottom" => Self::Bottom,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        Self::NAMES[self as usize]
    }

    /// The window's top-left physical position on a monitor.
    pub fn origin(self, monitor_pos: (i32, i32), monitor_size: (u32, u32), window: (u32, u32)) -> (i32, i32) {
        let (mx, my) = monitor_pos;
        let free_w = monitor_size.0 as i64 - window.0 as i64;
        let free_h = monitor_size.1 as i64 - window.1 as i64;
        match self {
            Self::Top => top_center(monitor_pos, monitor_size, window.0),
            Self::TopLeft => (mx, my),
            Self::TopRight => (mx + free_w as i32, my),
            Self::Bottom => (mx + (free_w / 2) as i32, my + free_h as i32),
        }
    }

    /// The island grows upward from the bottom of its window.
    pub fn from_bottom(self) -> bool {
        self == Self::Bottom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placements_put_the_window_where_they_say() {
        let (pos, size, win) = ((1920, 0), (2560, 1440), (400, 300));
        assert_eq!(Placement::Top.origin(pos, size, win), (1920 + 1080, 0));
        assert_eq!(Placement::TopLeft.origin(pos, size, win), (1920, 0));
        assert_eq!(Placement::TopRight.origin(pos, size, win), (1920 + 2160, 0));
        assert_eq!(Placement::Bottom.origin(pos, size, win), (1920 + 1080, 1140));
        for name in Placement::NAMES {
            assert_eq!(Placement::parse(name).map(Placement::name), Some(name));
        }
        assert_eq!(Placement::parse("side"), None);
        assert!(Placement::Bottom.from_bottom() && !Placement::TopLeft.from_bottom());
    }

    #[test]
    fn physical_rounds_outward() {
        let r = Rect { x: 10.5, y: 0.2, w: 100.2, h: 37.0 };
        assert_eq!(r.to_physical(1.0), PhysRect { x: 10, y: 0, w: 101, h: 38 });
    }

    #[test]
    fn physical_scales() {
        let r = Rect { x: 137.0, y: 8.0, w: 126.0, h: 37.0 };
        assert_eq!(r.to_physical(2.0), PhysRect { x: 274, y: 16, w: 252, h: 74 });
        assert_eq!(r.to_physical(1.25), PhysRect { x: 171, y: 10, w: 158, h: 47 });
    }

    #[test]
    fn physical_rejects_bad_scale_and_negative_size() {
        let r = Rect { x: 1.0, y: 1.0, w: -5.0, h: 2.0 };
        assert_eq!(r.to_physical(f64::NAN), PhysRect { x: 1, y: 1, w: 0, h: 2 });
    }

    #[test]
    fn centres_on_secondary_monitor() {
        assert_eq!(top_center((1920, 0), (2560, 1440), 800), (1920 + 880, 0));
        assert_eq!(top_center((-1280, -200), (1280, 1024), 400), (-1280 + 440, -200));
    }

    #[test]
    fn window_wider_than_monitor_still_centres() {
        assert_eq!(top_center((0, 0), (300, 200), 400), (-50, 0));
    }

    #[test]
    fn union_contains_inflate() {
        let a = Rect::new(10.0, 10.0, 10.0, 10.0);
        let b = Rect::new(0.0, 15.0, 5.0, 20.0);
        assert_eq!(a.union(&b), Rect::new(0.0, 10.0, 20.0, 25.0));
        assert!(a.contains(10.0, 19.9));
        assert!(!a.contains(20.0, 15.0));
        assert_eq!(a.inflate(1.0), Rect::new(9.0, 9.0, 12.0, 12.0));
    }

    #[test]
    fn rect_deserializes_from_ui_json() {
        let r: Rect = serde_json::from_str(r#"{"x":1,"y":2,"w":3,"h":4}"#).unwrap();
        assert_eq!(r, Rect { x: 1.0, y: 2.0, w: 3.0, h: 4.0 });
    }
}
