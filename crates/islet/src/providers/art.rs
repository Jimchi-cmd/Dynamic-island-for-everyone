//! Album art: decode (JPEG, PNG), crop to a square, scale to the size the
//! island draws it at (2× the 56 px card art) and find a dominant colour.
//! Only images the OS hands us are decoded; nothing is downloaded.

use islet_core::activity::ArtImage;
use std::sync::Arc;
use tiny_skia::{FilterQuality, Pixmap, PixmapPaint, Transform};

/// Edge length art is stored at (physical px).
pub const ART_PX: u32 = 112;

// Used by the OS media providers; allowed while some platforms lack one.
#[cfg_attr(not(test), allow(dead_code))]
pub fn decode(bytes: &[u8]) -> Option<ArtImage> {
    let src = if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Pixmap::decode_png(bytes).ok()?
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        decode_jpeg(bytes)?
    } else {
        return None;
    };
    Some(from_pixmap(&src))
}

#[cfg_attr(not(test), allow(dead_code))]
fn decode_jpeg(bytes: &[u8]) -> Option<Pixmap> {
    use zune_jpeg::zune_core::colorspace::ColorSpace;
    use zune_jpeg::zune_core::options::DecoderOptions;
    let opts = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut dec = zune_jpeg::JpegDecoder::new_with_options(zune_jpeg::zune_core::bytestream::ZCursor::new(bytes), opts);
    let rgba = dec.decode().ok()?;
    let (w, h) = dec.dimensions()?;
    let mut pm = Pixmap::new(w as u32, h as u32)?;
    // JPEG is opaque, so straight RGBA is already premultiplied.
    pm.data_mut().copy_from_slice(rgba.get(..(w * h * 4))?);
    Some(pm)
}

/// Centre-crop to a square, scale to `ART_PX`, and pick the dominant colour.
pub fn from_pixmap(src: &Pixmap) -> ArtImage {
    let side = src.width().min(src.height()).max(1) as f32;
    let k = ART_PX as f32 / side;
    let (ox, oy) = ((src.width() as f32 - side) / 2.0, (src.height() as f32 - side) / 2.0);
    let mut out = Pixmap::new(ART_PX, ART_PX).expect("non-zero art size");
    let paint = PixmapPaint { quality: FilterQuality::Bicubic, ..PixmapPaint::default() };
    let ts = Transform::from_scale(k, k).pre_translate(-ox, -oy);
    out.draw_pixmap(0, 0, src.as_ref(), &paint, ts, None);
    let dominant = dominant_colour(out.data());
    ArtImage { width: ART_PX, height: ART_PX, rgba: Arc::new(out.take()), dominant }
}

/// Average of the most colourful pixels (falls back to the overall average).
pub fn dominant_colour(rgba: &[u8]) -> [u8; 3] {
    let (mut sum, mut n) = ([0u64; 3], 0u64);
    let (mut all, mut m) = ([0u64; 3], 0u64);
    for px in rgba.chunks_exact(4) {
        let (r, g, b) = (px[0] as i32, px[1] as i32, px[2] as i32);
        let sat = r.max(g).max(b) - r.min(g).min(b);
        for (i, c) in [r, g, b].into_iter().enumerate() {
            all[i] += c as u64;
            if sat > 60 {
                sum[i] += c as u64;
            }
        }
        m += 1;
        if sat > 60 {
            n += 1;
        }
    }
    let (s, d) = if n * 10 >= m { (sum, n) } else { (all, m.max(1)) };
    [(s[0] / d) as u8, (s[1] / d) as u8, (s[2] / d) as u8]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crops_scales_and_finds_the_colour() {
        let mut src = Pixmap::new(300, 200).unwrap();
        src.fill(tiny_skia::Color::from_rgba8(200, 40, 40, 255));
        let art = from_pixmap(&src);
        assert_eq!((art.width, art.height), (ART_PX, ART_PX));
        assert_eq!(art.rgba.len(), (ART_PX * ART_PX * 4) as usize);
        assert_eq!(art.dominant, [200, 40, 40]);
    }

    #[test]
    fn decodes_png_and_rejects_unknown_bytes() {
        let mut src = Pixmap::new(8, 8).unwrap();
        src.fill(tiny_skia::Color::from_rgba8(10, 120, 220, 255));
        let png = src.encode_png().unwrap();
        let art = decode(&png).expect("png decodes");
        assert_eq!(art.dominant, [10, 120, 220]);
        assert!(decode(b"not an image").is_none());
    }

    #[test]
    fn grey_art_falls_back_to_the_average() {
        let grey = [90u8, 90, 90, 255].repeat(16);
        assert_eq!(dominant_colour(&grey), [90, 90, 90]);
    }
}
