//! Windows clipboard bitmaps (CF_DIB): a BITMAPINFOHEADER followed by the
//! pixels. Converted to and from straight RGBA, for the clipboard history.

fn u16_at(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(i..i + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?))
}

/// The largest picture read (pixels on a side).
const MAX_SIDE: u32 = 16_384;

/// A DIB's width, height and RGBA pixels (top row first). Uncompressed
/// 24- and 32-bit bitmaps, and 32-bit with colour masks, which is what
/// apps put on the clipboard.
pub fn dib_to_rgba(dib: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let header = u32_at(dib, 0)? as usize;
    if header < 40 || header > dib.len() {
        return None;
    }
    let width = i32::from_le_bytes(dib.get(4..8)?.try_into().ok()?);
    let height = i32::from_le_bytes(dib.get(8..12)?.try_into().ok()?);
    let bits = u16_at(dib, 14)?;
    let compression = u32_at(dib, 16)?;
    let (w, h) = (width.unsigned_abs(), height.unsigned_abs());
    if width <= 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE || !matches!(bits, 24 | 32) {
        return None;
    }
    // BI_RGB (0), or BI_BITFIELDS (3) whose three masks follow a 40-byte header.
    let pixels_at = match compression {
        0 => header,
        3 if header == 40 => header + 12,
        3 => header,
        _ => return None,
    };
    let stride = ((w * u32::from(bits)).div_ceil(32) * 4) as usize;
    let data = dib.get(pixels_at..pixels_at + stride * h as usize)?;
    // A 32-bit bitmap whose alpha is all zero means "no alpha": opaque.
    let has_alpha = bits == 32 && data.chunks_exact(4).any(|p| p[3] != 0);
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for row in 0..h as usize {
        // Positive height: bottom row first.
        let src = if height > 0 { h as usize - 1 - row } else { row };
        let line = &data[src * stride..src * stride + stride];
        for x in 0..w as usize {
            let p = &line[x * usize::from(bits / 8)..];
            let a = if has_alpha { p[3] } else { 255 };
            out.extend_from_slice(&[p[2], p[1], p[0], a]);
        }
    }
    Some((w, h, out))
}

/// RGBA pixels (top row first) as a 32-bit top-down DIB.
pub fn rgba_to_dib(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(40 + rgba.len());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(-(h as i32)).to_le_bytes()); // top-down
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    out.extend_from_slice(&(w * h * 4).to_le_bytes());
    out.extend_from_slice(&[0u8; 16]);
    for p in rgba.chunks_exact(4) {
        out.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_a_dib() {
        // 2×2: red, green / blue, half-transparent white.
        let rgba = [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 128];
        let dib = rgba_to_dib(2, 2, &rgba);
        assert_eq!(dib_to_rgba(&dib), Some((2, 2, rgba.to_vec())));
    }

    #[test]
    fn reads_bottom_up_24_bit_bitmaps() {
        // 1 wide, 2 high, rows padded to 4 bytes; the bottom (blue) row first.
        let mut dib = Vec::new();
        dib.extend_from_slice(&40u32.to_le_bytes());
        dib.extend_from_slice(&1i32.to_le_bytes());
        dib.extend_from_slice(&2i32.to_le_bytes());
        dib.extend_from_slice(&1u16.to_le_bytes());
        dib.extend_from_slice(&24u16.to_le_bytes());
        dib.extend_from_slice(&[0u8; 24]);
        dib.extend_from_slice(&[255, 0, 0, 0]); // blue (BGR) + padding
        dib.extend_from_slice(&[0, 0, 255, 0]); // red
        let (w, h, rgba) = dib_to_rgba(&dib).unwrap();
        assert_eq!((w, h), (1, 2));
        assert_eq!(rgba, [255, 0, 0, 255, 0, 0, 255, 255], "red on top, blue below");
    }

    #[test]
    fn opaque_when_32_bit_alpha_is_unused_and_refuses_garbage() {
        let mut dib = rgba_to_dib(1, 1, &[10, 20, 30, 0]);
        assert_eq!(dib_to_rgba(&dib).unwrap().2, [10, 20, 30, 255]);
        dib.truncate(30);
        assert_eq!(dib_to_rgba(&dib), None);
        assert_eq!(dib_to_rgba(b"nonsense"), None);
    }
}
