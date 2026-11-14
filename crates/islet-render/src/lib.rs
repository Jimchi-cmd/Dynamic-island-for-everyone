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
use tiny_skia::{
    Color, FillRule, GradientStop, LinearGradient, Mask, Paint, PathBuilder, Pixmap, Point, SpreadMode, Stroke,
    Transform,
};

/// What the island's body is made of.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Skin {
    /// The body's colour…
    pub color: [u8; 3],
    /// …and how solid it is (1 = opaque; less lets the desktop show through).
    pub alpha: f32,
    /// A glassy sheen: light falling from the top edge and a bright rim.
    pub glass: bool,
    /// A soft glow in this colour just inside the edge (the album's colour
    /// while music plays). Inside, so the island's outline never changes.
    pub glow: Option<[u8; 3]>,
    /// A crisp border in this colour just inside the edge (the playing
    /// song's cover colour). Inside too, so the outline never changes.
    pub border: Option<[u8; 3]>,
}

impl Default for Skin {
    /// Solid black, like the real thing.
    fn default() -> Self {
        Skin { color: [0, 0, 0], alpha: 1.0, glass: false, glow: None, border: None }
    }
}

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
    /// How visible the island is (1 = fully). Never drawn fully clear:
    /// Windows hit-tests layered windows by alpha, so a hidden island keeps
    /// the faintest alpha to still notice the pointer arriving.
    pub opacity: f32,
    pub skin: Skin,
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
        Self { shape, bubble: None, highlight: 0.0, flat_top: false, opacity: 1.0, skin: Skin::default() }
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
    let shown = frame.opacity.clamp(0.0, 1.0);
    let skin = frame.skin;
    let alpha = (shown * skin.alpha.clamp(0.0, 1.0) * 255.0).round().max(1.0) as u8;
    let [r, g, b] = skin.color;
    paint.set_color_rgba8(r, g, b, alpha);
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

    if skin.glass && shown > 0.02 {
        // Glass: light falls on the top edge and fades down the body, and a
        // rim catches it all the way round (brightest at the top).
        let s = frame.shape;
        let white = |a: f32| Color::from_rgba(1.0, 1.0, 1.0, (a * shown).clamp(0.0, 1.0)).unwrap_or(Color::TRANSPARENT);
        let fall = |top: f32, bottom: f32, reach: f32| {
            LinearGradient::new(
                Point::from_xy(s.x, s.y),
                Point::from_xy(s.x, s.y + (s.h * reach).max(1.0)),
                vec![GradientStop::new(0.0, white(top)), GradientStop::new(1.0, white(bottom))],
                SpreadMode::Pad,
                Transform::identity(),
            )
        };
        if let Some(shader) = fall(0.14, 0.0, 0.55) {
            let sheen = Paint { shader, anti_alias: true, ..Paint::default() };
            pixmap.fill_path(&path, &sheen, FillRule::Winding, ts, None);
        }
        let inset = 0.6;
        let inner = Squircle {
            x: s.x + inset,
            y: s.y + inset,
            w: s.w - 2.0 * inset,
            h: s.h - 2.0 * inset,
            radius: (s.radius - inset).max(0.0),
            smoothing: s.smoothing,
        };
        let rim_path = if frame.flat_top { inner.path_flat_top() } else { inner.path() };
        if let (Some(p), Some(shader)) = (rim_path, fall(0.55, 0.14, 1.0)) {
            let rim = Paint { shader, anti_alias: true, ..Paint::default() };
            pixmap.stroke_path(&p, &rim, &Stroke { width: 1.2, ..Stroke::default() }, ts, None);
        }
    }

    if let Some([r, g, b]) = skin.glow.filter(|_| shown > 0.02) {
        // Wide and faint first, then narrower and brighter: a glow that
        // falls off towards the middle, ending in a bright rim.
        let s = frame.shape;
        for (width, alpha) in [(9.0f32, 0.07f32), (5.0, 0.12), (2.4, 0.22), (1.1, 0.75)] {
            let inset = width / 2.0;
            let inner = Squircle {
                x: s.x + inset,
                y: s.y + inset,
                w: s.w - 2.0 * inset,
                h: s.h - 2.0 * inset,
                radius: (s.radius - inset).max(0.0),
                smoothing: s.smoothing,
            };
            if inner.w <= 1.0 || inner.h <= 1.0 {
                continue;
            }
            let ring = if frame.flat_top { inner.path_flat_top() } else { inner.path() };
            if let Some(p) = ring {
                let mut glow = Paint::default();
                glow.set_color_rgba8(r, g, b, (alpha * shown * 255.0).round() as u8);
                glow.anti_alias = true;
                pixmap.stroke_path(&p, &glow, &Stroke { width, ..Stroke::default() }, ts, None);
            }
        }
    }

    if let Some([r, g, b]) = skin.border.filter(|_| shown > 0.02) {
        const WIDTH: f32 = 2.0;
        let s = frame.shape;
        let inset = WIDTH / 2.0;
        let inner = Squircle {
            x: s.x + inset,
            y: s.y + inset,
            w: s.w - 2.0 * inset,
            h: s.h - 2.0 * inset,
            radius: (s.radius - inset).max(0.0),
            smoothing: s.smoothing,
        };
        let ring = if frame.flat_top { inner.path_flat_top() } else { inner.path() };
        if let (true, Some(p)) = (inner.w > 1.0 && inner.h > 1.0, ring) {
            let mut border = Paint::default();
            border.set_color_rgba8(r, g, b, (0.95 * shown * 255.0).round() as u8);
            border.anti_alias = true;
            pixmap.stroke_path(&p, &border, &Stroke { width: WIDTH, ..Stroke::default() }, ts, None);
        }
    }

    if frame.highlight > 0.0 && frame.opacity >= 1.0 {
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
    fn the_border_rims_the_edge_in_its_colour() {
        let shape = Squircle { x: 0.0, y: 0.0, w: 120.0, h: 40.0, radius: 20.0, smoothing: 0.6 };
        let mut frame = PillFrame::pill(shape);
        frame.skin.border = Some([200, 60, 90]);
        let mut pm = Pixmap::new(120, 40).unwrap();
        draw(&mut pm, &frame, 1.0);
        let px = |x: u32, y: u32| {
            let p = pm.pixel(x, y).unwrap();
            [p.red(), p.green(), p.blue()]
        };
        assert!(px(60, 1)[0] > 150 && px(60, 1)[1] < 90, "the top edge: {:?}", px(60, 1));
        assert_eq!(px(60, 20), [0, 0, 0], "the middle stays black");
    }

    #[test]
    fn rects_inside_the_outline() {
        let f = PillFrame::pill(Squircle { x: 0.0, y: 0.0, w: 300.0, h: 160.0, radius: 40.0, smoothing: 0.6 });
        assert!(f.contains_rect(100.0, 60.0, 100.0, 40.0), "the middle");
        assert!(f.contains_rect(20.0, 70.0, 50.0, 20.0), "the left edge, between corners");
        assert!(!f.contains_rect(2.0, 2.0, 30.0, 30.0), "a corner");
        assert!(!f.contains_rect(280.0, 70.0, 40.0, 10.0), "past the edge");
        let mut bubbled = f;
        // A hidden island keeps the faintest alpha, so Windows still sends
        // it the pointer.
        let mut pm = tiny_skia::Pixmap::new(300, 160).unwrap();
        draw(&mut pm, &PillFrame { opacity: 0.0, ..f }, 1.0);
        assert_eq!(pm.pixel(150, 80).map(|p| p.alpha()), Some(1));
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
