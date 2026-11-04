//! Golden-image tests: key island shapes and content rendered and compared
//! with reference PNGs in tests/golden/. Rendering is pure Rust (tiny-skia,
//! swash), so every OS must match within a small tolerance.
//!
//! Regenerate after an intended visual change:
//! `ISLET_UPDATE_GOLDEN=1 cargo test -p islet-render --test golden`

use islet_render::elements::{draw_elements, Element, Icon};
use islet_render::metaball::Circle;
use islet_render::text::{TextEngine, TextItem, TextStyle};
use islet_render::{clip_mask, draw, PillFrame, Squircle};
use std::path::PathBuf;
use tiny_skia::Pixmap;

const SCALE: f32 = 2.0;

fn scene(name: &str, w: u32, h: u32, frame: PillFrame, elements: &[Element]) {
    let mut pm = Pixmap::new((w as f32 * SCALE) as u32, (h as f32 * SCALE) as u32).unwrap();
    draw(&mut pm, &frame, SCALE);
    let clip = clip_mask(&frame, pm.width(), pm.height(), SCALE);
    let mut text = TextEngine::bundled_only();
    draw_elements(&mut pm, elements, &mut text, SCALE, clip.as_ref());
    compare(name, &pm);
}

fn compare(name: &str, actual: &Pixmap) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(format!("{name}.png"));
    if std::env::var_os("ISLET_UPDATE_GOLDEN").is_some() || !path.exists() {
        actual.save_png(&path).unwrap();
        return;
    }
    let expected = Pixmap::load_png(&path).unwrap();
    assert_eq!((expected.width(), expected.height()), (actual.width(), actual.height()), "{name}: size changed");
    let (mut off, total) = (0usize, expected.pixels().len());
    for (a, b) in expected.pixels().iter().zip(actual.pixels()) {
        let d = [
            a.red().abs_diff(b.red()),
            a.green().abs_diff(b.green()),
            a.blue().abs_diff(b.blue()),
            a.alpha().abs_diff(b.alpha()),
        ];
        if d.iter().any(|&c| c > 3) {
            off += 1;
        }
    }
    if off * 200 > total {
        let out = std::env::temp_dir().join(format!("{name}.actual.png"));
        actual.save_png(&out).unwrap();
        panic!("{name}: {off} of {total} pixels differ (actual saved to {})", out.display());
    }
}

fn shape(w: f32, h: f32, r: f32) -> Squircle {
    Squircle { x: (420.0 - w) / 2.0, y: 0.0, w, h, radius: r, smoothing: 0.6 }
}

#[test]
fn idle_pill() {
    scene("idle", 420, 60, PillFrame::pill(shape(126.0, 37.0, 18.5)), &[]);
}

#[test]
fn notch_shape() {
    let frame = PillFrame { flat_top: true, ..PillFrame::pill(shape(292.0, 32.0, 13.0)) };
    scene("notch", 420, 60, frame, &[]);
}

#[test]
fn minimal_split_with_ring() {
    let s = shape(162.0, 37.0, 18.5);
    let bubble = Circle { x: s.x + s.w + 8.0 + 18.5, y: 18.5, r: 18.5 };
    let frame = PillFrame { bubble: Some(bubble), ..PillFrame::pill(s) };
    let ring = Element::Ring {
        cx: bubble.x,
        cy: bubble.y,
        r: 11.0,
        width: 2.6,
        fraction: 0.87,
        color: [255; 3],
        opacity: 1.0,
    };
    scene("minimal", 420, 60, frame, &[ring]);
}

#[test]
fn expanded_card_with_text_and_controls() {
    let s = shape(372.0, 160.0, 44.0);
    let (x, y) = (s.x, s.y);
    let title = TextStyle { size: 16.0, weight: 600, ..TextStyle::default() };
    let elements = vec![
        Element::Art { x: x + 20.0, y: y + 20.0, size: 56.0, radius: 12.0, image: None, opacity: 1.0 },
        Element::Text(TextItem { text: "Golden Track".into(), style: title, x: x + 88.0, baseline: y + 44.0 }),
        Element::Bar { x: x + 20.0, y: y + 90.0, w: 332.0, h: 4.0, color: [255; 3], opacity: 0.2 },
        Element::Bar { x: x + 20.0, y: y + 90.0, w: 120.0, h: 4.0, color: [255; 3], opacity: 1.0 },
        Element::Icon {
            icon: Icon::Previous,
            x: x + 186.0 - 77.0,
            y: y + 123.0,
            size: 26.0,
            color: [255; 3],
            opacity: 1.0,
        },
        Element::Icon { icon: Icon::Pause, x: x + 171.0, y: y + 121.0, size: 30.0, color: [255; 3], opacity: 1.0 },
        Element::Icon {
            icon: Icon::Next,
            x: x + 186.0 + 51.0,
            y: y + 123.0,
            size: 26.0,
            color: [255; 3],
            opacity: 1.0,
        },
    ];
    scene("expanded", 420, 170, PillFrame::pill(s), &elements);
}
