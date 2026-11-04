//! CPU vector renderer for the island. Everything is drawn into a
//! premultiplied RGBA pixmap with tiny-skia; the platform layer only has to
//! put those pixels on screen. Rendering is deterministic, so the visual
//! tests compare against golden images on every OS.

pub mod squircle;

pub use squircle::Squircle;
use tiny_skia::{Color, FillRule, Paint, Pixmap, Stroke, Transform};

/// Everything needed to draw one frame of the pill (logical px).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PillFrame {
    pub shape: Squircle,
    /// Opacity of the 1 px inner highlight (0 = none).
    pub highlight: f32,
}

/// Draw `frame` at `scale` into `pixmap`, clearing it first.
pub fn draw(pixmap: &mut Pixmap, frame: &PillFrame, scale: f32) {
    pixmap.fill(Color::TRANSPARENT);
    let ts = Transform::from_scale(scale, scale);
    let Some(path) = frame.shape.path() else { return };
    let mut paint = Paint::default();
    paint.set_color_rgba8(0, 0, 0, 255);
    paint.anti_alias = true;
    pixmap.fill_path(&path, &paint, FillRule::Winding, ts, None);

    if frame.highlight > 0.0 {
        // A hairline just inside the edge so the pill reads on dark wallpapers.
        let inset = 0.5 / scale;
        let s = frame.shape;
        let inner = Squircle {
            x: s.x + inset,
            y: s.y + inset,
            w: s.w - 2.0 * inset,
            h: s.h - 2.0 * inset,
            radius: s.radius - inset,
            smoothing: s.smoothing,
        };
        if let Some(p) = inner.path() {
            let mut hl = Paint::default();
            hl.set_color(Color::from_rgba(1.0, 1.0, 1.0, frame.highlight.clamp(0.0, 1.0)).unwrap_or(Color::WHITE));
            hl.anti_alias = true;
            let stroke = Stroke { width: 1.0 / scale, ..Stroke::default() };
            pixmap.stroke_path(&p, &hl, &stroke, ts, None);
        }
    }
}
