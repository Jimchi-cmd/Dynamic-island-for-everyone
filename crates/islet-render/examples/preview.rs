//! Renders the island's states side by side on a light background:
//! `cargo run -p islet-render --example preview -- out.png`

use islet_render::{draw, PillFrame, Squircle};
use tiny_skia::{Color, Pixmap, PixmapPaint, Transform};

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "preview.png".into());
    let scale = 2.0;
    // (w, h, radius) for idle, compact, minimal pill, expanded, large.
    let states =
        [(126.0, 37.0, 18.5), (236.0, 37.0, 18.5), (162.0, 37.0, 18.5), (372.0, 160.0, 44.0), (384.0, 230.0, 48.0)];
    let (cell_w, cell_h) = (420.0_f32, 260.0_f32);
    let mut canvas = Pixmap::new((cell_w * scale) as u32, (cell_h * scale * states.len() as f32) as u32).unwrap();
    canvas.fill(Color::from_rgba8(232, 236, 241, 255));
    for (i, (w, h, r)) in states.into_iter().enumerate() {
        let mut cell = Pixmap::new((cell_w * scale) as u32, (cell_h * scale) as u32).unwrap();
        let frame = PillFrame::pill(Squircle { x: (cell_w - w) / 2.0, y: 12.0, w, h, radius: r, smoothing: 0.6 });
        draw(&mut cell, &frame, scale);
        let y = (i as f32 * cell_h * scale) as i32;
        canvas.draw_pixmap(0, y, cell.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
    }
    canvas.save_png(&out).expect("write png");
    println!("wrote {out}");
}
