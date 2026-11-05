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
    /// Whether the rectangle (logical px) is certainly inside the outline,
    /// so drawing it needs no clip mask. Conservative: the corners are
    /// treated as circles of radius (1 + smoothing) × r, which lie inside
    /// the squircle's smoothed corners.
    pub fn contains_rect(&self, x: f32, y: f32, w: f32, h: f32) -> bool {
        if self.bubble.is_some() {
            return false;
        }
        let s = &self.shape;
        if x < s.x || y < s.y || x + w > s.x + s.w || y + h > s.y + s.h {
            return false;
        }
        let r = (s.radius * (1.0 + s.smoothing)).min(s.w / 2.0).min(s.h / 2.0);
        let (l, t, rr, b) = (s.x + r, s.y + r, s.x + s.w - r, s.y + s.h - r);
        [(x, y), (x + w, y), (x, y + h), (x + w, y + h)].iter().all(|&(px, py)| {
            let cx = if px < l {
                l
            } else if px > rr {
                rr
            } else {
                return true;
            };
            let cy = if py < t {
                if self.flat_top {
                    return true;
                }
                t
            } else if py > b {
                b
            } else {
                return true;
            };
            (px - cx).powi(2) + (py - cy).powi(2) <= r * r
        })
    }

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

/// Premultiplied RGBA bytes (tiny-skia) → premultiplied BGRA bytes, the
/// native layout of Windows DIBs, X11/Wayland ARGB32 (little-endian) and
/// Core Animation. Swaps red and blue a whole pixel at a time, which the
/// compiler turns into vector code.
pub fn rgba_to_bgra(src: &[u8], dst: &mut [u8]) {
    let n = src.len().min(dst.len()) / 4 * 4;
    let (src, dst) = (&src[..n], &mut dst[..n]);
    // SAFETY: any four bytes are a valid u32; align_to only reinterprets
    // the aligned middle of each slice.
    let (sp, sm, ss) = unsafe { src.align_to::<u32>() };
    let (dp, dm, ds) = unsafe { dst.align_to_mut::<u32>() };
    if sp.is_empty() && dp.is_empty() && ss.is_empty() && ds.is_empty() && sm.len() == dm.len() {
        // Pixel bytes R,G,B,A read as a little-endian word are 0xAABBGGRR.
        let swap = |p: u32| (p & 0xFF00_FF00) | ((p & 0x0000_00FF) << 16) | ((p >> 16) & 0x0000_00FF);
        if cfg!(target_endian = "little") {
            for (d, &p) in dm.iter_mut().zip(sm) {
                *d = swap(p);
            }
            return;
        }
    }
    for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
        d.copy_from_slice(&[s[2], s[1], s[0], s[3]]);
    }
}

#[cfg(test)]
mod contains_tests {
    use super::*;

    #[test]
    fn rects_inside_the_outline() {
        let f = PillFrame::pill(Squircle { x: 0.0, y: 0.0, w: 300.0, h: 160.0, radius: 40.0, smoothing: 0.6 });
        assert!(f.contains_rect(100.0, 60.0, 100.0, 40.0), "the middle");
        assert!(f.contains_rect(20.0, 70.0, 50.0, 20.0), "the left edge, between corners");
        assert!(!f.contains_rect(2.0, 2.0, 30.0, 30.0), "a corner");
        assert!(!f.contains_rect(280.0, 70.0, 40.0, 10.0), "past the edge");
        let mut bubbled = f;
        bubbled.bubble = Some(Circle { x: 320.0, y: 18.0, r: 18.0 });
        assert!(!bubbled.contains_rect(100.0, 60.0, 10.0, 10.0));
    }
}

#[cfg(test)]
mod swizzle_tests {
    #[test]
    fn red_and_blue_swap() {
        // Aligned buffers take the word path; an odd offset takes the byte path.
        let src: Vec<u8> = vec![1, 2, 3, 4, 250, 0, 9, 255];
        let mut dst = vec![0u8; 8];
        super::rgba_to_bgra(&src, &mut dst);
        assert_eq!(dst, [3, 2, 1, 4, 9, 0, 250, 255]);
        let mut odd = [0u8; 9];
        super::rgba_to_bgra(&src, &mut odd[1..]);
        assert_eq!(&odd[1..], [3, 2, 1, 4, 9, 0, 250, 255]);
    }
}
