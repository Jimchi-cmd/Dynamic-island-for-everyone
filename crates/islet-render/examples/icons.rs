//! All island icons, a battery ring and art placeholder on black, 3x:
//! `cargo run -p islet-render --example icons -- out.png`
use islet_render::elements::{draw_elements, Element, Icon};
use islet_render::text::TextEngine;
use tiny_skia::{Color, Pixmap};

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "icons.png".into());
    let icons = [
        Icon::Play,
        Icon::Pause,
        Icon::Next,
        Icon::Previous,
        Icon::Music,
        Icon::Headphones,
        Icon::Earbuds,
        Icon::Speaker,
        Icon::Mouse,
        Icon::Keyboard,
        Icon::Gamepad,
        Icon::Phone,
        Icon::Tablet,
        Icon::Laptop,
        Icon::Bluetooth,
        Icon::Bolt,
    ];
    let scale = 3.0;
    let mut pm = Pixmap::new((40.0 * 9.0 * scale) as u32, (40.0 * 3.0 * scale) as u32).unwrap();
    pm.fill(Color::BLACK);
    let mut els: Vec<Element> = icons
        .iter()
        .enumerate()
        .map(|(i, &icon)| Element::Icon {
            icon,
            x: 8.0 + (i % 8) as f32 * 40.0,
            y: 8.0 + (i / 8) as f32 * 40.0,
            size: 24.0,
            color: [255, 255, 255],
            opacity: 1.0,
        })
        .collect();
    for (i, (f, c)) in [(0.87, [255, 255, 255]), (0.18, [255, 176, 32]), (0.08, [255, 69, 58]), (0.55, [48, 209, 88])]
        .iter()
        .enumerate()
    {
        els.push(Element::Ring {
            cx: 20.0 + i as f32 * 40.0,
            cy: 100.0,
            r: 11.0,
            width: 3.0,
            fraction: *f,
            color: *c,
            opacity: 1.0,
        });
    }
    els.push(Element::Art { x: 168.0, y: 84.0, size: 32.0, radius: 8.0, image: None, opacity: 1.0 });
    let mut text = TextEngine::new();
    draw_elements(&mut pm, &els, &mut text, scale, None);
    pm.save_png(&out).unwrap();
    println!("wrote {out}");
}
