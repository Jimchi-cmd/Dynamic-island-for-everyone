//! Filmstrip of the Minimal split: pill + bubble + metaball neck at
//! increasing separation. `cargo run -p islet-render --example split -- out.png`

use islet_render::metaball::{neck, Circle};
use islet_render::Squircle;
use tiny_skia::{Color, FillRule, Paint, Pixmap, Transform};

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "split.png".into());
    let scale = 3.0;
    let gaps = [-30.0, -18.0, -8.0, 0.0, 6.0, 12.0, 18.0, 21.0, 24.0];
    let (cell_w, cell_h) = (260.0_f32, 52.0_f32);
    let mut pm = Pixmap::new((cell_w * scale) as u32, (cell_h * scale * gaps.len() as f32) as u32).unwrap();
    pm.fill(Color::from_rgba8(232, 236, 241, 255));
    let mut paint = Paint::default();
    paint.set_color_rgba8(0, 0, 0, 255);
    paint.anti_alias = true;
    for (i, gap) in gaps.into_iter().enumerate() {
        let ts = Transform::from_scale(scale, scale).pre_translate(0.0, i as f32 * cell_h);
        let pill = Squircle { x: 40.0, y: 8.0, w: 162.0, h: 37.0, radius: 18.5, smoothing: 0.6 };
        let end = Circle { x: pill.x + pill.w - 18.5, y: pill.y + 18.5, r: 18.5 };
        let bubble = Circle { x: pill.x + pill.w + gap + 18.5, y: end.y, r: 18.5 };
        pm.fill_path(&pill.path().unwrap(), &paint, FillRule::Winding, ts, None);
        let circle = tiny_skia::PathBuilder::from_circle(bubble.x, bubble.y, bubble.r).unwrap();
        pm.fill_path(&circle, &paint, FillRule::Winding, ts, None);
        if let Some(n) = neck(end, bubble) {
            pm.fill_path(&n, &paint, FillRule::Winding, ts, None);
        }
    }
    pm.save_png(&out).expect("write png");
    println!("wrote {out}");
}
