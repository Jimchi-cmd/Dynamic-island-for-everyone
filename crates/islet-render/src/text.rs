//! Text for the island: shaping and glyph rasterisation with cosmic-text
//! (swash), composited straight into the island pixmap and clipped to the
//! pill so content never spills outside a shrinking shape.
//!
//! Fonts (design spec §1): Inter is built in (SIL OFL 1.1, see
//! assets/fonts/OFL.txt). On macOS the OS system font, San Francisco, is
//! loaded from the system at runtime and preferred; it is never bundled.
//! Font data is referenced, not copied, so the 1.2 MB of embedded Inter only
//! costs memory for the pages actually read.

use cosmic_text::{
    fontdb, Attrs, Buffer, Family, FeatureTag, FontFeatures, FontSystem, Metrics, Shaping, SwashCache, SwashContent,
    Weight,
};
use std::sync::Arc;
use tiny_skia::{Mask, Pixmap};

static INTER_REGULAR: &[u8] = include_bytes!("../../../assets/fonts/Inter-Regular.otf");
static INTER_SEMIBOLD: &[u8] = include_bytes!("../../../assets/fonts/Inter-SemiBold.otf");

const INTER: &str = "Inter";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// How a run of text looks. Sizes are logical px.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    pub size: f32,
    /// CSS-style weight: 400 regular, 600 semibold.
    pub weight: u16,
    /// Straight (not premultiplied) RGB.
    pub color: [u8; 3],
    /// 0..1, multiplied into every glyph.
    pub opacity: f32,
    /// Tabular figures, so changing digits don't make the text jiggle.
    pub tabular: bool,
    pub align: Align,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self { size: 15.0, weight: 400, color: [255, 255, 255], opacity: 1.0, tabular: false, align: Align::Left }
    }
}

/// One line of text to draw: `x` is the left, centre or right edge depending
/// on `style.align`; `baseline` is the baseline y. Logical px.
#[derive(Debug, Clone, PartialEq)]
pub struct TextItem {
    pub text: String,
    pub style: TextStyle,
    pub x: f32,
    pub baseline: f32,
}

pub struct TextEngine {
    fonts: FontSystem,
    cache: SwashCache,
    family: String,
    buffer: Option<Buffer>,
}

impl Default for TextEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TextEngine {
    pub fn new() -> Self {
        Self::build(true)
    }

    /// Only the bundled Inter, ignoring the OS font: identical output on
    /// every OS (golden-image tests).
    pub fn bundled_only() -> Self {
        Self::build(false)
    }

    fn build(system_font: bool) -> Self {
        let mut db = fontdb::Database::new();
        db.load_font_source(fontdb::Source::Binary(Arc::new(INTER_REGULAR)));
        db.load_font_source(fontdb::Source::Binary(Arc::new(INTER_SEMIBOLD)));
        let family = system_font.then(|| system_ui_family(&mut db)).flatten().unwrap_or_else(|| INTER.to_string());
        let locale = sys_locale::get_locale().unwrap_or_else(|| "en-US".to_string());
        let fonts = FontSystem::new_with_locale_and_db(locale, db);
        Self { fonts, cache: SwashCache::new(), family, buffer: None }
    }

    /// The family used for island text ("Inter", or the macOS system font).
    pub fn family(&self) -> &str {
        &self.family
    }

    fn layout(&mut self, text: &str, style: &TextStyle) -> &Buffer {
        let metrics = Metrics::new(style.size, style.size * 1.2);
        let buffer = self.buffer.get_or_insert_with(|| Buffer::new_empty(metrics));
        buffer.set_metrics(metrics);
        buffer.set_size(None, None);
        let mut features = FontFeatures::new();
        if style.tabular {
            features.enable(FeatureTag::new(b"tnum"));
        }
        let attrs =
            Attrs::new().family(Family::Name(&self.family)).weight(Weight(style.weight)).font_features(features);
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.fonts, false);
        buffer
    }

    /// Width of `text` in logical px.
    pub fn measure(&mut self, text: &str, style: &TextStyle) -> f32 {
        self.layout(text, style).layout_runs().map(|run| run.line_w).fold(0.0, f32::max)
    }

    /// `text`, shortened with "…" so it fits in `max_width`.
    pub fn ellipsize(&mut self, text: &str, style: &TextStyle, max_width: f32) -> String {
        if self.measure(text, style) <= max_width {
            return text.to_string();
        }
        let chars: Vec<char> = text.chars().collect();
        // Binary search for the longest prefix that fits with the ellipsis.
        let (mut lo, mut hi) = (0usize, chars.len());
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            let candidate: String = chars[..mid].iter().collect::<String>().trim_end().to_string() + "…";
            if self.measure(&candidate, style) <= max_width {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        chars[..lo].iter().collect::<String>().trim_end().to_string() + "…"
    }

    /// Draw `item` at `scale` (physical px per logical px), clipped to `clip`.
    pub fn draw(&mut self, pixmap: &mut Pixmap, item: &TextItem, scale: f32, clip: Option<&Mask>) {
        let style = item.style;
        let opacity = style.opacity.clamp(0.0, 1.0);
        if opacity <= 0.0 || item.text.is_empty() {
            return;
        }
        let width = self.measure(&item.text, &style);
        let left = match style.align {
            Align::Left => item.x,
            Align::Center => item.x - width / 2.0,
            Align::Right => item.x - width,
        };
        // Lay out at physical size so glyphs are rasterised crisply.
        let physical = TextStyle { size: style.size * scale, ..style };
        self.layout(&item.text, &physical);
        let Some(buffer) = self.buffer.take() else { return };
        let origin = (left * scale, item.baseline * scale);
        for run in buffer.layout_runs() {
            for glyph in run.glyphs {
                let pg = glyph.physical((origin.0, origin.1 - run.line_y), 1.0);
                let Some(image) = self.cache.get_image(&mut self.fonts, pg.cache_key) else { continue };
                let gx = pg.x + image.placement.left;
                let gy = (run.line_y as i32) + pg.y - image.placement.top;
                blit(pixmap, image, gx, gy, style.color, opacity, clip);
            }
        }
        self.buffer = Some(buffer);
    }
}

/// On macOS, register the system UI font (San Francisco) from the OS and
/// return its family name. Elsewhere: None (Inter is used).
fn system_ui_family(db: &mut fontdb::Database) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let path = std::path::Path::new("/System/Library/Fonts/SFNS.ttf");
        let before = db.len();
        if db.load_font_file(path).is_ok() && db.len() > before {
            return db.faces().last().and_then(|f| f.families.first()).map(|(name, _)| name.clone());
        }
    }
    let _ = db;
    None
}

/// Composite one glyph image over premultiplied RGBA pixels.
fn blit(
    pixmap: &mut Pixmap,
    image: &cosmic_text::SwashImage,
    gx: i32,
    gy: i32,
    color: [u8; 3],
    opacity: f32,
    clip: Option<&Mask>,
) {
    let (pw, ph) = (pixmap.width() as i32, pixmap.height() as i32);
    let (iw, ih) = (image.placement.width as i32, image.placement.height as i32);
    let mask = clip.map(|m| (m.data(), m.width() as i32));
    let pixels = pixmap.data_mut();
    for y in 0..ih {
        let py = gy + y;
        if py < 0 || py >= ph {
            continue;
        }
        for x in 0..iw {
            let px = gx + x;
            if px < 0 || px >= pw {
                continue;
            }
            let i = (y * iw + x) as usize;
            // Source colour (straight) and coverage for this pixel.
            let (src, cov) = match image.content {
                SwashContent::Mask => (color, image.data[i] as f32 / 255.0),
                SwashContent::SubpixelMask => {
                    let s = &image.data[i * 4..i * 4 + 3];
                    (color, (s[0] as f32 + s[1] as f32 + s[2] as f32) / (3.0 * 255.0))
                }
                SwashContent::Color => {
                    let s = &image.data[i * 4..i * 4 + 4];
                    ([s[0], s[1], s[2]], s[3] as f32 / 255.0)
                }
            };
            let clip_a = mask.map_or(1.0, |(d, mw)| d[(py * mw + px) as usize] as f32 / 255.0);
            let a = cov * opacity * clip_a;
            if a <= 0.0 {
                continue;
            }
            let o = ((py * pw + px) * 4) as usize;
            let d = &mut pixels[o..o + 4];
            // Source-over with premultiplied destination.
            for c in 0..3 {
                d[c] = (src[c] as f32 * a + d[c] as f32 * (1.0 - a)).round() as u8;
            }
            d[3] = (255.0 * a + d[3] as f32 * (1.0 - a)).round() as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ink(p: &Pixmap) -> (usize, Option<(u32, u32, u32, u32)>) {
        let mut n = 0;
        let mut bb: Option<(u32, u32, u32, u32)> = None;
        for y in 0..p.height() {
            for x in 0..p.width() {
                if p.pixel(x, y).is_some_and(|c| c.alpha() > 0) {
                    n += 1;
                    bb = Some(bb.map_or((x, y, x, y), |(a, b, c, d)| (a.min(x), b.min(y), c.max(x), d.max(y))));
                }
            }
        }
        (n, bb)
    }

    #[test]
    fn inter_is_built_in_and_draws_glyphs_on_the_baseline() {
        let mut engine = TextEngine::new();
        let mut p = Pixmap::new(200, 60).unwrap();
        let item = TextItem {
            text: "12:34".into(),
            style: TextStyle { size: 32.0, weight: 600, tabular: true, ..TextStyle::default() },
            x: 10.0,
            baseline: 45.0,
        };
        engine.draw(&mut p, &item, 1.0, None);
        let (n, bb) = ink(&p);
        assert!(n > 200, "expected visible glyphs, got {n} pixels");
        let (x0, y0, _x1, y1) = bb.unwrap();
        assert!(x0 >= 10, "starts at x {x0}");
        assert!(y1 <= 46 && y0 >= 45 - 32, "glyphs sit on the baseline: {bb:?}");
    }

    #[test]
    fn tabular_digits_have_equal_width() {
        let mut engine = TextEngine::new();
        let s = TextStyle { size: 20.0, tabular: true, ..TextStyle::default() };
        let (w1, w8) = (engine.measure("11:11", &s), engine.measure("88:88", &s));
        assert!((w1 - w8).abs() < 0.01, "{w1} vs {w8}");
    }

    #[test]
    fn alignment_and_clipping() {
        let mut engine = TextEngine::new();
        let style = TextStyle { size: 20.0, align: Align::Right, ..TextStyle::default() };
        let mut p = Pixmap::new(200, 40).unwrap();
        engine.draw(&mut p, &TextItem { text: "Islet".into(), style, x: 190.0, baseline: 30.0 }, 1.0, None);
        let (_, bb) = ink(&p);
        assert!(bb.unwrap().2 <= 191, "right-aligned text ends at x: {bb:?}");

        // A mask covering only the left half clips the right half away.
        let mut clip = Mask::new(200, 40).unwrap();
        let rect = tiny_skia::PathBuilder::from_rect(tiny_skia::Rect::from_xywh(0.0, 0.0, 100.0, 40.0).unwrap());
        clip.fill_path(&rect, tiny_skia::FillRule::Winding, false, tiny_skia::Transform::identity());
        let mut q = Pixmap::new(200, 40).unwrap();
        let centred = TextStyle { align: Align::Center, ..style };
        engine.draw(
            &mut q,
            &TextItem { text: "Clipped".into(), style: centred, x: 100.0, baseline: 30.0 },
            1.0,
            Some(&clip),
        );
        let (_, bb) = ink(&q);
        assert!(bb.unwrap().2 < 100, "nothing drawn right of the clip: {bb:?}");
    }

    #[test]
    fn zero_opacity_draws_nothing() {
        let mut engine = TextEngine::new();
        let mut p = Pixmap::new(100, 40).unwrap();
        let style = TextStyle { opacity: 0.0, ..TextStyle::default() };
        engine.draw(&mut p, &TextItem { text: "Hidden".into(), style, x: 0.0, baseline: 30.0 }, 1.0, None);
        assert_eq!(ink(&p).0, 0);
    }
}
