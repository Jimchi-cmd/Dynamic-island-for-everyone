//! Content drawn inside the island: a small display list of text, artwork,
//! icons, rings and bars, all clipped to the pill. Icons are original vector
//! drawings on a 24 × 24 grid (no third-party or Apple glyphs).

use crate::text::{TextEngine, TextItem};
use islet_core::activity::ArtImage;
use tiny_skia::{
    Color, FillRule, FilterQuality, LineCap, LineJoin, Mask, Paint, Path, PathBuilder, Pattern, Pixmap, PixmapRef,
    Rect, SpreadMode, Stroke, Transform,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Play,
    Pause,
    Next,
    Previous,
    Music,
    Headphones,
    Earbuds,
    Speaker,
    Mouse,
    Keyboard,
    Gamepad,
    Phone,
    Tablet,
    Laptop,
    Bluetooth,
    Bolt,
    Navigation,
    Close,
    /// A bell, tilted by this many degrees (it swings while ringing).
    Bell(i8),
    /// Wi-Fi with this many lit bars (0..=3; unlit ones are faint).
    Wifi(u8),
    Lock,
    Check,
    /// A speaker with 0..=3 sound waves; 255 = muted (a slash).
    Volume(u8),
    Sun,
    CapsLock,
    Moon,
    Cloud,
    PartlyCloudy,
    Rain,
    Snow,
    Storm,
    Fog,
    Calendar,
    Message,
    File,
    Folder,
    /// A dial with a needle (the system monitor).
    Gauge,
    /// A clipboard: a board with a clip at the top.
    Clipboard,
    /// A horizontal battery filled to this percentage.
    Battery(u8),
    /// A chevron pointing left ("back") or right.
    Back,
    Forward,
    /// Two switches, one on and one off (Control Centre).
    Controls,
    /// A microphone; struck through when muted.
    Mic(bool),
    /// A stopwatch: a dial with a button on top and a hand.
    Stopwatch,
    /// A checklist: three rows, the first ticked.
    Checklist,
    /// A screenshot: four corner brackets around a frame.
    Screenshot,
    /// A rubbish bin.
    Trash,
    Plus,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Element {
    Text(TextItem),
    /// Album art (or a placeholder when `image` is None) in a rounded square.
    Art {
        x: f32,
        y: f32,
        size: f32,
        radius: f32,
        image: Option<ArtImage>,
        opacity: f32,
    },
    Icon {
        icon: Icon,
        x: f32,
        y: f32,
        size: f32,
        color: [u8; 3],
        opacity: f32,
    },
    /// Battery ring: a faint full track plus an arc for `fraction`.
    Ring {
        cx: f32,
        cy: f32,
        r: f32,
        width: f32,
        fraction: f32,
        color: [u8; 3],
        opacity: f32,
    },
    /// A fully rounded bar (progress track/fill, waveform bar).
    Bar {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [u8; 3],
        opacity: f32,
    },
}

impl Element {
    /// The same element, `k` times as opaque (the whole island fading).
    pub fn faded(self, k: f32) -> Element {
        match self {
            Element::Text(mut t) => {
                t.style.opacity *= k;
                Element::Text(t)
            }
            Element::Art { x, y, size, radius, image, opacity } => {
                Element::Art { x, y, size, radius, image, opacity: opacity * k }
            }
            Element::Icon { icon, x, y, size, color, opacity } => {
                Element::Icon { icon, x, y, size, color, opacity: opacity * k }
            }
            Element::Ring { cx, cy, r, width, fraction, color, opacity } => {
                Element::Ring { cx, cy, r, width, fraction, color, opacity: opacity * k }
            }
            Element::Bar { x, y, w, h, color, opacity } => Element::Bar { x, y, w, h, color, opacity: opacity * k },
        }
    }
}

fn paint(color: [u8; 3], opacity: f32) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(Color::from_rgba8(color[0], color[1], color[2], (opacity.clamp(0.0, 1.0) * 255.0).round() as u8));
    p.anti_alias = true;
    p
}

fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<Path> {
    crate::Squircle { x, y, w, h, radius: r, smoothing: 0.6 }.path()
}

/// Circular arc from `start` (radians, 0 = 3 o'clock, clockwise on screen)
/// sweeping `sweep` radians, as cubic segments.
fn arc(pb: &mut PathBuilder, cx: f32, cy: f32, r: f32, start: f32, sweep: f32, move_to: bool) {
    let n = (sweep.abs() / std::f32::consts::FRAC_PI_2).ceil().max(1.0) as usize;
    let step = sweep / n as f32;
    let k = 4.0 / 3.0 * (step / 4.0).tan() * r;
    let pt = |a: f32| (cx + r * a.cos(), cy + r * a.sin());
    let (x0, y0) = pt(start);
    if move_to {
        pb.move_to(x0, y0);
    } else {
        pb.line_to(x0, y0);
    }
    for i in 0..n {
        let (a0, a1) = (start + step * i as f32, start + step * (i + 1) as f32);
        let (p0, p1) = (pt(a0), pt(a1));
        pb.cubic_to(p0.0 - k * a0.sin(), p0.1 + k * a0.cos(), p1.0 + k * a1.sin(), p1.1 - k * a1.cos(), p1.0, p1.1);
    }
}

/// Fill and/or stroke shapes on the 24-unit icon grid.
struct IconPainter<'a> {
    pm: &'a mut Pixmap,
    ts: Transform,
    paint: Paint<'static>,
    clip: Option<&'a Mask>,
}

impl IconPainter<'_> {
    fn fill(&mut self, path: Option<Path>) {
        if let Some(p) = path {
            self.pm.fill_path(&p, &self.paint, FillRule::Winding, self.ts, self.clip);
        }
    }

    fn stroke(&mut self, path: Option<Path>, width: f32) {
        if let Some(p) = path {
            let s = Stroke { width, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Stroke::default() };
            self.pm.stroke_path(&p, &self.paint, &s, self.ts, self.clip);
        }
    }

    fn poly(points: &[(f32, f32)]) -> Option<Path> {
        let mut pb = PathBuilder::new();
        let (first, rest) = points.split_first()?;
        pb.move_to(first.0, first.1);
        for p in rest {
            pb.line_to(p.0, p.1);
        }
        pb.close();
        pb.finish()
    }

    /// A filled triangle with softened corners.
    fn soft_triangle(&mut self, pts: [(f32, f32); 3]) {
        let p = Self::poly(&pts);
        self.fill(p.clone());
        self.stroke(p, 2.0);
    }

    fn line(&mut self, a: (f32, f32), b: (f32, f32), width: f32) {
        let mut pb = PathBuilder::new();
        pb.move_to(a.0, a.1);
        pb.line_to(b.0, b.1);
        self.stroke(pb.finish(), width);
    }

    fn circle(&mut self, cx: f32, cy: f32, r: f32) -> Option<Path> {
        PathBuilder::from_circle(cx, cy, r)
    }

    fn draw(&mut self, icon: Icon) {
        match icon {
            Icon::Play => self.soft_triangle([(7.5, 4.5), (19.5, 12.0), (7.5, 19.5)]),
            Icon::Pause => {
                self.fill(rounded_rect(5.5, 4.5, 4.5, 15.0, 1.6));
                self.fill(rounded_rect(14.0, 4.5, 4.5, 15.0, 1.6));
            }
            Icon::Next => {
                self.soft_triangle([(3.5, 6.0), (11.5, 12.0), (3.5, 18.0)]);
                self.soft_triangle([(12.5, 6.0), (20.5, 12.0), (12.5, 18.0)]);
            }
            Icon::Previous => {
                self.soft_triangle([(20.5, 6.0), (12.5, 12.0), (20.5, 18.0)]);
                self.soft_triangle([(11.5, 6.0), (3.5, 12.0), (11.5, 18.0)]);
            }
            Icon::Music => {
                let heads = [self.circle(7.5, 17.0, 2.8), self.circle(16.5, 15.0, 2.8)];
                for h in heads {
                    self.fill(h);
                }
                self.line((10.2, 17.0), (10.2, 6.5), 1.8);
                self.line((19.2, 15.0), (19.2, 4.5), 1.8);
                self.line((10.2, 6.5), (19.2, 4.5), 2.6);
            }
            Icon::Headphones => {
                let mut pb = PathBuilder::new();
                arc(&mut pb, 12.0, 13.5, 8.0, std::f32::consts::PI, std::f32::consts::PI, true);
                self.stroke(pb.finish(), 2.0);
                self.fill(rounded_rect(3.0, 13.0, 4.5, 7.5, 1.8));
                self.fill(rounded_rect(16.5, 13.0, 4.5, 7.5, 1.8));
            }
            Icon::Earbuds => {
                for cx in [8.0, 16.0] {
                    let head = self.circle(cx, 8.5, 3.3);
                    self.fill(head);
                    let stem_x = if cx < 12.0 { cx + 0.6 } else { cx - 2.8 };
                    self.fill(rounded_rect(stem_x, 9.5, 2.2, 10.0, 1.1));
                }
            }
            Icon::Speaker => {
                self.stroke(rounded_rect(6.0, 2.5, 12.0, 19.0, 3.0), 1.8);
                let cone = self.circle(12.0, 15.0, 3.2);
                self.stroke(cone, 1.8);
                let tweeter = self.circle(12.0, 7.5, 1.2);
                self.fill(tweeter);
            }
            Icon::Mouse => {
                self.stroke(rounded_rect(6.5, 2.5, 11.0, 19.0, 5.5), 1.9);
                self.line((12.0, 3.0), (12.0, 8.5), 1.6);
            }
            Icon::Keyboard => {
                self.stroke(rounded_rect(2.0, 6.0, 20.0, 12.5, 2.2), 1.8);
                for (i, x) in [6.0, 9.33, 12.66, 16.0].iter().enumerate() {
                    let _ = i;
                    self.fill(rounded_rect(*x - 0.8, 9.0, 1.6, 1.6, 0.5));
                    self.fill(rounded_rect(*x + 0.85, 11.8, 1.6, 1.6, 0.5));
                }
                self.line((8.0, 15.4), (16.0, 15.4), 1.6);
            }
            Icon::Gamepad => {
                self.stroke(rounded_rect(2.0, 7.0, 20.0, 11.0, 5.5), 1.8);
                self.line((5.6, 12.5), (9.4, 12.5), 1.6);
                self.line((7.5, 10.6), (7.5, 14.4), 1.6);
                let a = self.circle(16.2, 11.2, 1.1);
                self.fill(a);
                let b = self.circle(18.4, 13.6, 1.1);
                self.fill(b);
            }
            Icon::Phone => {
                self.stroke(rounded_rect(7.0, 2.0, 10.0, 20.0, 2.6), 1.8);
                self.line((10.5, 18.6), (13.5, 18.6), 1.6);
            }
            Icon::Tablet => {
                self.stroke(rounded_rect(4.0, 2.5, 16.0, 19.0, 2.6), 1.8);
                self.line((10.5, 18.6), (13.5, 18.6), 1.6);
            }
            Icon::Laptop => {
                self.stroke(rounded_rect(5.0, 5.0, 14.0, 10.0, 1.6), 1.8);
                self.line((2.5, 18.5), (21.5, 18.5), 2.0);
            }
            Icon::Bluetooth => {
                let mut pb = PathBuilder::new();
                for (i, p) in
                    [(7.0, 7.5), (16.5, 16.5), (12.0, 21.0), (12.0, 3.0), (16.5, 7.5), (7.0, 16.5)].iter().enumerate()
                {
                    if i == 0 {
                        pb.move_to(p.0, p.1);
                    } else {
                        pb.line_to(p.0, p.1);
                    }
                }
                self.stroke(pb.finish(), 1.9);
            }
            Icon::Navigation => {
                // A direction arrow pointing up and to the right.
                let p = Self::poly(&[(20.0, 4.0), (12.5, 20.5), (10.8, 13.2), (3.5, 11.5)]);
                self.fill(p.clone());
                self.stroke(p, 1.6);
            }
            Icon::Back | Icon::Forward => {
                let mut pb = PathBuilder::new();
                let (tip, tail) = if icon == Icon::Back { (8.5, 15.0) } else { (15.5, 9.0) };
                pb.move_to(tail, 5.5);
                pb.line_to(tip, 12.0);
                pb.line_to(tail, 18.5);
                self.stroke(pb.finish(), 2.6);
            }
            Icon::Mic(muted) => {
                // A rounded capsule on a stand.
                self.fill(rounded_rect(9.0, 3.0, 6.0, 11.5, 3.0));
                let mut pb = PathBuilder::new();
                arc(&mut pb, 12.0, 10.5, 6.0, 0.0, std::f32::consts::PI, true);
                self.stroke(pb.finish(), 1.8);
                self.line((12.0, 16.5), (12.0, 20.0), 1.8);
                self.line((8.5, 20.5), (15.5, 20.5), 1.8);
                if muted {
                    self.line((4.5, 3.5), (19.5, 20.5), 2.0);
                }
            }
            Icon::Controls => {
                // Two toggle tracks: the top one on (knob right), the bottom off.
                self.stroke(rounded_rect(3.5, 4.5, 17.0, 6.5, 3.25), 1.8);
                self.fill(PathBuilder::from_circle(17.25, 7.75, 2.2));
                self.stroke(rounded_rect(3.5, 13.0, 17.0, 6.5, 3.25), 1.8);
                self.fill(PathBuilder::from_circle(6.75, 16.25, 2.2));
            }
            Icon::Close => {
                self.line((6.5, 6.5), (17.5, 17.5), 2.2);
                self.line((17.5, 6.5), (6.5, 17.5), 2.2);
            }
            Icon::Wifi(lit) => {
                // A dot and three arcs fanning up from it.
                let full = self.paint.clone();
                for i in 0..3u8 {
                    let r = 5.5 + f32::from(i) * 5.0;
                    let mut pb = PathBuilder::new();
                    arc(&mut pb, 12.0, 19.0, r, -2.36, 1.57, true);
                    if i >= lit {
                        let mut faint = full.clone();
                        if let tiny_skia::Shader::SolidColor(mut c) = faint.shader {
                            c.set_alpha(c.alpha() * 0.3);
                            faint.shader = tiny_skia::Shader::SolidColor(c);
                        }
                        self.paint = faint;
                    }
                    self.stroke(pb.finish(), 2.4);
                    self.paint = full.clone();
                }
                let dot = self.circle(12.0, 19.0, 1.9);
                self.fill(dot);
            }
            Icon::Lock => {
                self.fill(rounded_rect(5.5, 10.5, 13.0, 10.0, 2.2));
                let mut pb = PathBuilder::new();
                pb.move_to(8.5, 10.8);
                pb.line_to(8.5, 8.0);
                pb.cubic_to(8.5, 4.2, 15.5, 4.2, 15.5, 8.0);
                pb.line_to(15.5, 10.8);
                self.stroke(pb.finish(), 2.0);
            }
            Icon::Volume(waves) => {
                let body = Self::poly(&[(3.5, 9.0), (7.5, 9.0), (12.5, 4.5), (12.5, 19.5), (7.5, 15.0), (3.5, 15.0)]);
                self.fill(body.clone());
                self.stroke(body, 1.2);
                if waves == 255 {
                    self.line((15.5, 9.0), (21.0, 15.0), 2.0);
                    self.line((21.0, 9.0), (15.5, 15.0), 2.0);
                } else {
                    for i in 0..waves.min(3) {
                        let r = 3.5 + f32::from(i) * 3.3;
                        let mut pb = PathBuilder::new();
                        arc(&mut pb, 13.0, 12.0, r, -0.9, 1.8, true);
                        self.stroke(pb.finish(), 1.9);
                    }
                }
            }
            Icon::Sun => {
                let core = self.circle(12.0, 12.0, 4.2);
                self.fill(core);
                for i in 0..8 {
                    let a = i as f32 * std::f32::consts::FRAC_PI_4;
                    let (c, s) = (a.cos(), a.sin());
                    self.line((12.0 + 6.8 * c, 12.0 + 6.8 * s), (12.0 + 9.3 * c, 12.0 + 9.3 * s), 2.0);
                }
            }
            Icon::File => {
                let page = Self::poly(&[(6.0, 3.0), (14.0, 3.0), (19.0, 8.0), (19.0, 21.0), (6.0, 21.0)]);
                self.stroke(page, 1.8);
                self.line((14.0, 3.5), (14.0, 8.0), 1.6);
                self.line((14.0, 8.0), (18.5, 8.0), 1.6);
                self.line((9.0, 13.0), (16.0, 13.0), 1.6);
                self.line((9.0, 16.5), (16.0, 16.5), 1.6);
            }
            Icon::Clipboard => {
                self.stroke(rounded_rect(5.5, 5.0, 13.0, 16.0, 2.5), 1.8);
                self.fill(rounded_rect(8.5, 3.0, 7.0, 4.0, 1.5));
                self.line((8.5, 12.0), (15.5, 12.0), 1.6);
                self.line((8.5, 15.5), (13.5, 15.5), 1.6);
            }
            Icon::Stopwatch => {
                let dial = self.circle(12.0, 13.5, 7.5);
                self.stroke(dial, 2.0);
                self.line((10.0, 3.2), (14.0, 3.2), 2.0);
                self.line((12.0, 3.2), (12.0, 6.0), 2.0);
                self.line((17.6, 6.6), (18.9, 7.9), 2.0);
                self.line((12.0, 13.5), (12.0, 9.2), 2.0);
            }
            Icon::Checklist => {
                self.line((3.5, 6.8), (5.3, 8.6), 1.8);
                self.line((5.3, 8.6), (8.3, 5.2), 1.8);
                let a = self.circle(5.5, 12.5, 1.6);
                self.stroke(a, 1.5);
                let b = self.circle(5.5, 18.0, 1.6);
                self.stroke(b, 1.5);
                for y in [7.0, 12.5, 18.0] {
                    self.line((10.5, y), (20.5, y), 1.8);
                }
            }
            Icon::Screenshot => {
                for (cx, cy, dx, dy) in
                    [(4.0, 5.0, 1.0, 1.0), (20.0, 5.0, -1.0, 1.0), (4.0, 19.0, 1.0, -1.0), (20.0, 19.0, -1.0, -1.0)]
                {
                    self.line((cx, cy), (cx + 4.5 * dx, cy), 1.9);
                    self.line((cx, cy), (cx, cy + 4.0 * dy), 1.9);
                }
                self.fill(rounded_rect(8.0, 9.0, 8.0, 6.0, 1.5));
            }
            Icon::Trash => {
                self.line((4.5, 6.5), (19.5, 6.5), 1.9);
                self.line((9.5, 6.5), (10.0, 3.8), 1.8);
                self.line((10.0, 3.8), (14.0, 3.8), 1.8);
                self.line((14.0, 3.8), (14.5, 6.5), 1.8);
                let body = Self::poly(&[(6.3, 6.5), (17.7, 6.5), (16.6, 20.0), (7.4, 20.0)]);
                self.stroke(body, 1.8);
                self.line((10.2, 10.0), (10.4, 16.5), 1.6);
                self.line((13.8, 10.0), (13.6, 16.5), 1.6);
            }
            Icon::Plus => {
                self.line((12.0, 5.0), (12.0, 19.0), 2.2);
                self.line((5.0, 12.0), (19.0, 12.0), 2.2);
            }
            Icon::Gauge => {
                // A three-quarter dial open at the bottom, needle up-right.
                let mut pb = PathBuilder::new();
                arc(&mut pb, 12.0, 13.0, 8.5, 0.75 * std::f32::consts::PI, 1.5 * std::f32::consts::PI, true);
                self.stroke(pb.finish(), 2.0);
                self.line((12.0, 13.0), (16.2, 8.6), 2.0);
                let hub = self.circle(12.0, 13.0, 1.9);
                self.fill(hub);
            }
            Icon::Folder => {
                let tab = Self::poly(&[(3.0, 6.5), (9.5, 6.5), (11.5, 8.5), (21.0, 8.5), (21.0, 19.0), (3.0, 19.0)]);
                self.fill(tab);
            }
            Icon::Message => {
                // A speech bubble with a tail.
                let mut pb = PathBuilder::new();
                arc(&mut pb, 12.0, 11.0, 8.0, 2.3, 5.7, true);
                pb.line_to(5.0, 19.5);
                pb.close();
                self.fill(pb.finish());
            }
            Icon::Calendar => {
                self.stroke(rounded_rect(4.0, 5.5, 16.0, 15.0, 3.0), 1.8);
                self.fill(rounded_rect(4.0, 5.5, 16.0, 4.5, 2.0));
                self.line((8.5, 3.5), (8.5, 7.0), 1.8);
                self.line((15.5, 3.5), (15.5, 7.0), 1.8);
                for (x, y) in [(8.5, 13.5), (12.0, 13.5), (15.5, 13.5), (8.5, 17.0), (12.0, 17.0)] {
                    let dot = self.circle(x, y, 1.1);
                    self.fill(dot);
                }
            }
            Icon::Moon => {
                // A crescent: the big disc minus where a second disc covers it.
                let (c1, r1, c2, r2) = ((11.0f32, 12.5f32), 8.0f32, (15.5f32, 9.0f32), 6.8f32);
                let (dx, dy) = (c2.0 - c1.0, c2.1 - c1.1);
                let d = (dx * dx + dy * dy).sqrt();
                let a = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
                let h = (r1 * r1 - a * a).max(0.0).sqrt();
                let (px, py) = (c1.0 + a * dx / d, c1.1 + a * dy / d);
                let i1 = (px + h * dy / d, py - h * dx / d);
                let i2 = (px - h * dy / d, py + h * dx / d);
                let ang = |c: (f32, f32), p: (f32, f32)| (p.1 - c.1).atan2(p.0 - c.0);
                let tau = std::f32::consts::TAU;
                // Outer edge: the long way round the big disc, from i1 to i2.
                let (o1, o2) = (ang(c1, i1), ang(c1, i2));
                let mut outer = o2 - o1;
                if outer > 0.0 {
                    outer -= tau;
                }
                // Inner edge: the short way round the cutting disc, back to i1.
                let (n1, n2) = (ang(c2, i2), ang(c2, i1));
                let mut inner = n2 - n1;
                if inner < 0.0 {
                    inner += tau;
                }
                if inner > std::f32::consts::PI {
                    inner -= tau;
                }
                let mut pb = PathBuilder::new();
                arc(&mut pb, c1.0, c1.1, r1, o1, outer, true);
                arc(&mut pb, c2.0, c2.1, r2, n1, inner, false);
                pb.close();
                self.fill(pb.finish());
            }
            Icon::Cloud | Icon::Rain | Icon::Snow | Icon::Storm | Icon::PartlyCloudy | Icon::Fog => {
                if icon == Icon::PartlyCloudy {
                    let sun = self.circle(8.0, 8.0, 3.6);
                    self.fill(sun);
                    for i in 0..5 {
                        let a = -2.8 + i as f32 * 0.7;
                        let (c, s) = (a.cos(), a.sin());
                        self.line((8.0 + 5.4 * c, 8.0 + 5.4 * s), (8.0 + 7.2 * c, 8.0 + 7.2 * s), 1.6);
                    }
                }
                // A cloud: three bumps on a flat base.
                let base = if matches!(icon, Icon::Cloud | Icon::PartlyCloudy) { 17.5 } else { 13.5 };
                let mut pb = PathBuilder::new();
                pb.move_to(6.5, base);
                arc(&mut pb, 7.0, base - 3.5, 3.5, std::f32::consts::FRAC_PI_2, std::f32::consts::PI, false);
                arc(&mut pb, 11.5, base - 5.5, 4.8, 3.3, 2.6, false);
                arc(&mut pb, 17.0, base - 3.8, 3.8, 4.9, 2.95, false);
                pb.close();
                let cloud = pb.finish();
                self.fill(cloud);
                match icon {
                    Icon::Rain => {
                        for x in [8.0, 12.0, 16.0] {
                            self.line((x, 16.5), (x - 1.2, 20.5), 1.8);
                        }
                    }
                    Icon::Snow => {
                        for x in [8.0, 12.0, 16.0] {
                            let flake = self.circle(x, 18.5, 1.3);
                            self.fill(flake);
                        }
                    }
                    Icon::Storm => self.fill(Self::poly(&[
                        (12.5, 14.5),
                        (9.0, 19.0),
                        (11.5, 19.0),
                        (10.5, 22.5),
                        (15.0, 17.5),
                        (12.5, 17.5),
                    ])),
                    Icon::Fog => {
                        self.line((5.0, 17.0), (19.0, 17.0), 1.8);
                        self.line((7.0, 20.5), (17.0, 20.5), 1.8);
                    }
                    _ => {}
                }
            }
            Icon::CapsLock => {
                let arrow = Self::poly(&[
                    (12.0, 3.5),
                    (20.0, 11.5),
                    (15.5, 11.5),
                    (15.5, 15.5),
                    (8.5, 15.5),
                    (8.5, 11.5),
                    (4.0, 11.5),
                ]);
                self.fill(arrow.clone());
                self.stroke(arrow, 1.2);
                self.fill(rounded_rect(8.5, 17.5, 7.0, 3.0, 1.0));
            }
            Icon::Check => {
                let mut pb = PathBuilder::new();
                pb.move_to(5.0, 12.5);
                pb.line_to(10.0, 17.5);
                pb.line_to(19.0, 7.0);
                self.stroke(pb.finish(), 2.6);
            }
            Icon::Bell(tilt) => {
                // Swings about the knob at the top.
                self.ts = self.ts.pre_concat(Transform::from_rotate_at(f32::from(tilt), 12.0, 3.5));
                let mut pb = PathBuilder::new();
                pb.move_to(6.4, 16.2);
                pb.line_to(6.4, 11.0);
                pb.cubic_to(6.4, 7.6, 8.8, 5.0, 12.0, 5.0);
                pb.cubic_to(15.2, 5.0, 17.6, 7.6, 17.6, 11.0);
                pb.line_to(17.6, 16.2);
                pb.line_to(19.4, 18.2);
                pb.line_to(4.6, 18.2);
                pb.close();
                let body = pb.finish();
                self.fill(body.clone());
                self.stroke(body, 1.2);
                let (knob, clapper) = (self.circle(12.0, 3.9, 1.5), self.circle(12.0, 20.3, 2.0));
                self.fill(knob);
                self.fill(clapper);
            }
            Icon::Battery(percent) => {
                self.stroke(rounded_rect(1.5, 6.5, 19.0, 11.0, 3.0), 1.5);
                self.fill(rounded_rect(21.3, 10.0, 1.8, 4.0, 0.9));
                let w = 15.0 * f32::from(percent.min(100)) / 100.0;
                if w > 0.5 {
                    self.fill(rounded_rect(3.5, 8.5, w.max(2.0), 7.0, 1.6));
                }
            }
            Icon::Bolt => {
                let p = Self::poly(&[(13.5, 2.0), (5.0, 13.5), (11.0, 13.5), (10.0, 22.0), (19.0, 10.0), (13.0, 10.0)]);
                self.fill(p);
            }
        }
    }
}

/// Draw `elements` at `scale` (physical px per logical px), clipped to `clip`.
pub fn draw_elements(pm: &mut Pixmap, elements: &[Element], text: &mut TextEngine, scale: f32, clip: Option<&Mask>) {
    draw_elements_in(pm, elements, text, scale, clip, None);
}

/// The box an element covers (logical px).
fn bounds(el: &Element, text: &mut TextEngine) -> Option<(f32, f32, f32, f32)> {
    match el {
        Element::Text(t) => {
            let w = text.measure(&t.text, &t.style);
            let left = match t.style.align {
                crate::text::Align::Left => t.x,
                crate::text::Align::Center => t.x - w / 2.0,
                crate::text::Align::Right => t.x - w,
            };
            // Ascenders and descenders stay within about 1.1 × the size.
            let s = t.style.size;
            let (mut l, mut r) = (left - 2.0, left + w + 2.0);
            if let Some((wl, wr)) = t.window {
                (l, r) = (l.max(wl), r.min(wr));
            }
            Some((l, t.baseline - s * 1.1, (r - l).max(0.0), s * 1.5))
        }
        Element::Bar { x, y, w, h, .. } => Some((*x, *y, *w, *h)),
        Element::Icon { x, y, size, .. } | Element::Art { x, y, size, .. } => Some((*x, *y, *size, *size)),
        Element::Ring { cx, cy, r, width, .. } => {
            let e = r + width / 2.0;
            Some((cx - e, cy - e, 2.0 * e, 2.0 * e))
        }
    }
}

/// Whether any element reaches outside `frame`'s outline (so a clip mask
/// is needed at all).
pub fn needs_clip(frame: &crate::PillFrame, elements: &[Element], text: &mut TextEngine) -> bool {
    elements.iter().any(|el| !bounds(el, text).is_some_and(|(x, y, w, h)| frame.contains_rect(x, y, w, h)))
}

/// Like `draw_elements`, but elements wholly inside `frame`'s outline skip
/// the clip mask (masked fills are several times slower).
pub fn draw_elements_in(
    pm: &mut Pixmap,
    elements: &[Element],
    text: &mut TextEngine,
    scale: f32,
    clip: Option<&Mask>,
    frame: Option<&crate::PillFrame>,
) {
    let base = Transform::from_scale(scale, scale);
    for el in elements {
        let clip = match (frame, clip.and_then(|_| bounds(el, text))) {
            (Some(f), Some((x, y, w, h))) if f.contains_rect(x, y, w, h) => None,
            _ => clip,
        };
        match el {
            Element::Text(item) => text.draw(pm, item, scale, clip),
            Element::Bar { x, y, w, h, color, opacity } => {
                // Fully rounded along its short side: a capsule lying down
                // (progress) or standing up (waveform, scroll bar).
                if let Some(p) = rounded_rect(*x, *y, *w, *h, w.min(*h) / 2.0) {
                    pm.fill_path(&p, &paint(*color, *opacity), FillRule::Winding, base, clip);
                }
            }
            Element::Ring { cx, cy, r, width, fraction, color, opacity } => {
                let s = Stroke { width: *width, line_cap: LineCap::Round, ..Stroke::default() };
                if let Some(track) = PathBuilder::from_circle(*cx, *cy, *r) {
                    pm.stroke_path(&track, &paint([255, 255, 255], 0.22 * opacity), &s, base, clip);
                }
                let f = fraction.clamp(0.0, 1.0);
                if f > 0.0 {
                    let mut pb = PathBuilder::new();
                    let start = -std::f32::consts::FRAC_PI_2;
                    arc(&mut pb, *cx, *cy, *r, start, f * std::f32::consts::TAU, true);
                    if let Some(p) = pb.finish() {
                        pm.stroke_path(&p, &paint(*color, *opacity), &s, base, clip);
                    }
                }
            }
            Element::Icon { icon, x, y, size, color, opacity } => {
                let k = size / 24.0;
                let ts = base.pre_translate(*x, *y).pre_scale(k, k);
                let mut p = IconPainter { pm, ts, paint: paint(*color, *opacity), clip };
                p.draw(*icon);
            }
            Element::Art { x, y, size, radius, image, opacity } => {
                let Some(shape) = rounded_rect(*x, *y, *size, *size, *radius) else { continue };
                match image
                    .as_ref()
                    .and_then(|img| PixmapRef::from_bytes(img.rgba.as_slice(), img.width, img.height).map(|r| (img, r)))
                {
                    Some((img, pixmap_ref)) => {
                        let k = size / img.width.max(img.height) as f32;
                        let shader_ts = Transform::from_translate(*x, *y).pre_scale(k, k);
                        let shader = Pattern::new(
                            pixmap_ref,
                            SpreadMode::Pad,
                            FilterQuality::Bicubic,
                            opacity.clamp(0.0, 1.0),
                            shader_ts,
                        );
                        let art_paint = Paint { shader, anti_alias: true, ..Paint::default() };
                        pm.fill_path(&shape, &art_paint, FillRule::Winding, base, clip);
                    }
                    None => {
                        // Placeholder: a dark tile with a music note.
                        pm.fill_path(&shape, &paint([44, 44, 48], *opacity), FillRule::Winding, base, clip);
                        let note = size * 0.56;
                        let ts = base.pre_translate(x + (size - note) / 2.0, y + (size - note) / 2.0);
                        let ts = ts.pre_scale(note / 24.0, note / 24.0);
                        let mut p = IconPainter { pm, ts, paint: paint([150, 150, 156], *opacity), clip };
                        p.draw(Icon::Music);
                    }
                }
            }
        }
    }
}

/// Rect helper for layouts.
pub fn rect(x: f32, y: f32, w: f32, h: f32) -> Option<Rect> {
    Rect::from_xywh(x, y, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn ink(p: &Pixmap) -> usize {
        p.pixels().iter().filter(|c| c.alpha() > 0).count()
    }

    const ALL: [Icon; 53] = [
        Icon::Stopwatch,
        Icon::Checklist,
        Icon::Screenshot,
        Icon::Trash,
        Icon::Plus,
        Icon::Mic(false),
        Icon::Mic(true),
        Icon::Back,
        Icon::Forward,
        Icon::Controls,
        Icon::Battery(0),
        Icon::Battery(70),
        Icon::Clipboard,
        Icon::Gauge,
        Icon::File,
        Icon::Folder,
        Icon::Message,
        Icon::Calendar,
        Icon::Moon,
        Icon::Cloud,
        Icon::PartlyCloudy,
        Icon::Rain,
        Icon::Snow,
        Icon::Storm,
        Icon::Fog,
        Icon::Volume(0),
        Icon::Volume(255),
        Icon::Sun,
        Icon::CapsLock,
        Icon::Wifi(0),
        Icon::Wifi(3),
        Icon::Lock,
        Icon::Check,
        Icon::Bell(0),
        Icon::Bell(-14),
        Icon::Navigation,
        Icon::Close,
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

    #[test]
    fn every_icon_draws_inside_its_box() {
        let mut text = TextEngine::new();
        for icon in ALL {
            let mut pm = Pixmap::new(40, 40).unwrap();
            let el = Element::Icon { icon, x: 8.0, y: 8.0, size: 24.0, color: [255; 3], opacity: 1.0 };
            draw_elements(&mut pm, &[el], &mut text, 1.0, None);
            assert!(ink(&pm) > 20, "{icon:?} drew nothing");
            for y in 0..40u32 {
                for x in 0..40u32 {
                    if pm.pixel(x, y).is_some_and(|c| c.alpha() > 0) {
                        assert!((6..=34).contains(&x) && (6..=34).contains(&y), "{icon:?} spills at {x},{y}");
                    }
                }
            }
        }
    }

    #[test]
    fn ring_fraction_controls_the_arc() {
        let mut text = TextEngine::new();
        let draw = |fraction: f32, text: &mut TextEngine| {
            let mut pm = Pixmap::new(40, 40).unwrap();
            let ring =
                Element::Ring { cx: 20.0, cy: 20.0, r: 12.0, width: 3.0, fraction, color: [0, 255, 0], opacity: 1.0 };
            draw_elements(&mut pm, &[ring], text, 1.0, None);
            pm.pixels().iter().filter(|c| c.green() > 150 && c.red() < 100).count()
        };
        let (quarter, full) = (draw(0.25, &mut text), draw(1.0, &mut text));
        assert!(quarter > 0 && full > quarter * 3, "quarter {quarter} full {full}");
        assert_eq!(draw(0.0, &mut text), 0);
    }

    #[test]
    fn art_is_drawn_rounded_and_placeholder_without_image() {
        let mut text = TextEngine::new();
        // A 4x4 opaque red image.
        let rgba = Arc::new([255u8, 0, 0, 255].repeat(16));
        let img = ArtImage { width: 4, height: 4, rgba, dominant: [255, 0, 0] };
        let mut pm = Pixmap::new(40, 40).unwrap();
        let art = Element::Art { x: 5.0, y: 5.0, size: 30.0, radius: 8.0, image: Some(img), opacity: 1.0 };
        draw_elements(&mut pm, &[art], &mut text, 1.0, None);
        let centre = pm.pixel(20, 20).unwrap();
        assert!(centre.red() > 200 && centre.alpha() == 255);
        assert_eq!(pm.pixel(5, 5).unwrap().alpha(), 0, "corner is rounded away");

        let mut pm = Pixmap::new(40, 40).unwrap();
        let ph = Element::Art { x: 5.0, y: 5.0, size: 30.0, radius: 8.0, image: None, opacity: 1.0 };
        draw_elements(&mut pm, &[ph], &mut text, 1.0, None);
        assert!(ink(&pm) > 500, "placeholder tile drawn");
    }
}
