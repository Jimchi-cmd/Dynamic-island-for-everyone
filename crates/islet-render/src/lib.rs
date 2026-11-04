//! CPU vector renderer for the island. Everything is drawn into a
//! premultiplied RGBA pixmap with tiny-skia; the platform layer only has to
//! put those pixels on screen. Rendering is deterministic, so the visual
//! tests compare against golden images on every OS.

pub mod elements;
pub mod metaball;
pub mod squircle;
pub mod text;

pub use metaball::Circle;
pub use squircle::Squircle;
use tiny_skia::{Color, FillRule, Mask, Paint, PathBuilder, Pixmap, Stroke, Transform};

/// Everything needed to draw one frame of the island (logical px).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PillFrame {
    pub shape: Squircle,
    /// The detached circle of the Minimal presentation. While it is close to
    /// the pill, a metaball neck joins the two.
    pub bubble: Option<Circle>,
    /// Opacity of the 1 px inner highlight (0 = none).
    pub highlight: f32,
    /// Square top corners: on a notched MacBook the island hangs from the
    /// top edge, so it reads as the notch growing.
    pub flat_top: bool,
}

impl PillFrame {
    pub fn pill(shape: Squircle) -> Self {
        Self { shape, bubble: None, highlight: 0.0, flat_top: false }
    }

    fn outline(&self) -> Option<tiny_skia::Path> {
        if self.flat_top {
            self.shape.path_flat_top()
        } else {
            self.shape.path()
        }
    }

    /// The round end of the pill that the bubble buds from: the top-right
    /// corner's circle (for a capsule, its whole right end).
    pub fn pill_end(&self) -> Circle {
        let s = self.shape;
        let r = s.effective_radius();
        Circle { x: s.x + s.w - r, y: s.y + r, r }
    }
}

/// A clip mask of the island's outline (pill, bubble and neck), so content
/// is never drawn outside it.
pub fn clip_mask(frame: &PillFrame, width: u32, height: u32, scale: f32) -> Option<Mask> {
    let ts = Transform::from_scale(scale, scale);
    let mut mask = Mask::new(width, height)?;
    mask.fill_path(&frame.outline()?, FillRule::Winding, true, ts);
    if let Some(b) = frame.bubble {
        if let Some(circle) = PathBuilder::from_circle(b.x, b.y, b.r) {
            mask.fill_path(&circle, FillRule::Winding, true, ts);
        }
        if let Some(neck) = metaball::neck(frame.pill_end(), b) {
            mask.fill_path(&neck, FillRule::Winding, true, ts);
        }
    }
    Some(mask)
}

/// Draw `frame` at `scale` into `pixmap`, clearing it first.
pub fn draw(pixmap: &mut Pixmap, frame: &PillFrame, scale: f32) {
    pixmap.fill(Color::TRANSPARENT);
    let ts = Transform::from_scale(scale, scale);
    let Some(path) = frame.outline() else { return };
    let mut paint = Paint::default();
    paint.set_color_rgba8(0, 0, 0, 255);
    paint.anti_alias = true;
    pixmap.fill_path(&path, &paint, FillRule::Winding, ts, None);

    if let Some(b) = frame.bubble {
        if let Some(circle) = PathBuilder::from_circle(b.x, b.y, b.r) {
            pixmap.fill_path(&circle, &paint, FillRule::Winding, ts, None);
        }
        if let Some(neck) = metaball::neck(frame.pill_end(), b) {
            pixmap.fill_path(&neck, &paint, FillRule::Winding, ts, None);
        }
    }

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
