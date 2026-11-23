//! Control Centre's other layouts (Settings → Look → Control Centre):
//! Modules, Sliders and Dock. Classic is drawn by `expanded_clock` and the
//! functions beside it. Every layout shows the same buttons, in the order
//! the user set, and a tap on one does what it does in Classic.

use super::*;
use islet_core::weather::Weather;

/// How Control Centre is laid out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CcStyle {
    /// The time, Wi-Fi and Bluetooth with their names, the tools in a row.
    #[default]
    Classic,
    /// A block of round buttons, with the timer beside it.
    Modules,
    /// Volume and brightness as tall sliders beside a grid of buttons.
    Sliders,
    /// A big clock over a colour wash, every button in one labelled row.
    Dock,
}

impl CcStyle {
    /// From its name in the settings (`prefs::CC_STYLES`); anything else is Classic.
    pub fn parse(name: &str) -> CcStyle {
        match name {
            "modules" => CcStyle::Modules,
            "sliders" => CcStyle::Sliders,
            "dock" => CcStyle::Dock,
            _ => CcStyle::Classic,
        }
    }
}

/// A Control Centre button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CcButton {
    Wifi,
    Bluetooth,
    Timer,
    System,
    Controls,
    Calendar,
    Screenshot,
    Lock,
    Shelf,
    Todo,
    Stopwatch,
    Clipboard,
    Shortcuts,
}

impl CcButton {
    /// Every button, in the usual order (`prefs::CC_BUTTONS`).
    pub const ALL: [CcButton; 13] = [
        CcButton::Wifi,
        CcButton::Bluetooth,
        CcButton::Timer,
        CcButton::System,
        CcButton::Controls,
        CcButton::Calendar,
        CcButton::Screenshot,
        CcButton::Lock,
        CcButton::Shelf,
        CcButton::Todo,
        CcButton::Stopwatch,
        CcButton::Clipboard,
        CcButton::Shortcuts,
    ];

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn id(self) -> &'static str {
        let i = Self::ALL.iter().position(|b| *b == self).unwrap_or(0);
        islet_core::prefs::CC_BUTTONS[i]
    }

    pub fn from_id(id: &str) -> Option<CcButton> {
        islet_core::prefs::CC_BUTTONS.iter().position(|b| *b == id).map(|i| Self::ALL[i])
    }

    /// What a tap does: the same as the button in Classic.
    pub fn hit(self) -> ConnHit {
        match self {
            CcButton::Wifi => ConnHit::Open(Panel::Wifi),
            CcButton::Bluetooth => ConnHit::Open(Panel::Bluetooth),
            CcButton::Timer => ConnHit::Quick(Quick::Start),
            CcButton::System => ConnHit::Open(Panel::System),
            CcButton::Controls => ConnHit::Open(Panel::Controls),
            CcButton::Calendar => ConnHit::Quick(Quick::Calendar),
            CcButton::Screenshot => ConnHit::Quick(Quick::Screenshot),
            CcButton::Lock => ConnHit::Quick(Quick::Lock),
            CcButton::Shelf => ConnHit::Open(Panel::Shelf),
            CcButton::Todo => ConnHit::Open(Panel::Todo),
            CcButton::Stopwatch => ConnHit::Open(Panel::Stopwatch),
            CcButton::Clipboard => ConnHit::Open(Panel::Clipboard),
            CcButton::Shortcuts => ConnHit::Open(Panel::Shortcuts),
        }
    }

    fn icon(self, c: &Connectivity) -> Icon {
        match self {
            CcButton::Wifi => {
                Icon::Wifi(c.wifi.networks.iter().find(|n| n.active).map_or(3, |n| connectivity::bars(n.strength)))
            }
            CcButton::Bluetooth => Icon::Bluetooth,
            CcButton::Timer => Icon::Play,
            CcButton::System => Icon::Gauge,
            CcButton::Controls => Icon::Controls,
            CcButton::Calendar => Icon::Calendar,
            CcButton::Screenshot => Icon::Screenshot,
            CcButton::Lock => Icon::Lock,
            CcButton::Shelf => Icon::Folder,
            CcButton::Todo => Icon::Checklist,
            CcButton::Stopwatch => Icon::Stopwatch,
            CcButton::Clipboard => Icon::Clipboard,
            CcButton::Shortcuts => Icon::Bolt,
        }
    }

    /// Its name under it in the dock: a word or two.
    fn label(self, minutes: u32) -> String {
        match self {
            CcButton::Timer => tf("{} min", &[&minutes]),
            CcButton::Wifi => tr("Wi-Fi").into(),
            CcButton::Bluetooth => tr("Bluetooth").into(),
            CcButton::System => tr("System‖short button label").into(),
            CcButton::Controls => tr("Controls‖short button label").into(),
            CcButton::Calendar => tr("Calendar").into(),
            CcButton::Screenshot => tr("Capture‖short label for the screenshot button").into(),
            CcButton::Lock => tr("Lock").into(),
            CcButton::Shelf => tr("Shelf").into(),
            CcButton::Todo => tr("To-do").into(),
            CcButton::Stopwatch => tr("Stopwatch").into(),
            CcButton::Clipboard => tr("Clipboard").into(),
            CcButton::Shortcuts => tr("Shortcuts").into(),
        }
    }

    /// Lit in the accent colour while on (Wi-Fi, Bluetooth).
    fn on(self, c: &Connectivity) -> bool {
        match self {
            CcButton::Wifi => c.wifi_on(),
            CcButton::Bluetooth => c.bt_on(),
            _ => false,
        }
    }
}

/// The buttons in the user's order (a copy rides in `ClockRow`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CcOrder(pub [CcButton; 13]);

impl Default for CcOrder {
    fn default() -> Self {
        CcOrder(CcButton::ALL)
    }
}

impl CcOrder {
    /// From the ids in the settings; any left out follow in the usual order.
    pub fn from_ids<S: AsRef<str>>(ids: &[S]) -> CcOrder {
        let mut v: Vec<CcButton> = Vec::new();
        for b in ids.iter().filter_map(|id| CcButton::from_id(id.as_ref())) {
            if !v.contains(&b) {
                v.push(b);
            }
        }
        for b in CcButton::ALL {
            if !v.contains(&b) {
                v.push(b);
            }
        }
        let mut out = CcButton::ALL;
        out.copy_from_slice(&v[..13]);
        CcOrder(out)
    }
}

/// The buttons Control Centre shows, in the user's order (switched-off
/// ones, and the shelf while it is empty, left out).
pub fn cc_buttons(row: ClockRow) -> Vec<CcButton> {
    row.order
        .0
        .iter()
        .copied()
        .filter(|b| match b {
            CcButton::Wifi | CcButton::Bluetooth => true,
            CcButton::Timer => !row.no_timer,
            CcButton::System => !row.no_system,
            CcButton::Controls => !row.no_controls,
            CcButton::Calendar => !row.no_calendar,
            CcButton::Screenshot => !row.no_screenshot,
            CcButton::Lock => !row.no_lock,
            CcButton::Shelf => row.shelf,
            CcButton::Todo => row.todo,
            CcButton::Stopwatch => row.stopwatch,
            CcButton::Clipboard => row.clipboard,
            CcButton::Shortcuts => row.shortcuts,
        })
        .collect()
}

/// How a spot is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    /// A round button (Modules' block).
    Disc,
    /// A rounded square with an icon.
    Tile,
    /// A tile with a round icon, a name and how it is (Sliders' Wi-Fi, Bluetooth).
    Wide,
    /// The dock: a round button with its name under it.
    Docked,
    /// The timer's tile with its length (no button of its own).
    TimerTile,
    /// The orange start button, and − and +.
    Start,
    Step,
}

/// A place in a layout: where, which button, what a tap does (None for
/// what is only drawn) and how it looks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spot {
    pub r: Rect,
    pub button: CcButton,
    pub hit: Option<ConnHit>,
    pub look: Look,
}

impl Spot {
    fn new(r: Rect, button: CcButton, look: Look) -> Spot {
        Spot { r, button, hit: Some(button.hit()), look }
    }
}

const TOP: f64 = 14.0;
/// Room for "Now Playing ›" above the time, when Control Centre was
/// opened from an activity.
const HOME_DROP: f64 = 24.0;
/// Modules' and Sliders' header: the time, the date and the weather.
const HEAD_H: f64 = 66.0;
const BODY_H: f64 = 114.0;
/// Dock's header: the network, a big clock and the date.
const DOCK_HEAD: f64 = 100.0;
const DOCK_H: f64 = 72.0;
/// The most buttons the dock's row holds.
pub const DOCK_MAX: usize = 9;
const SLIDER_W: f64 = 48.0;

fn top(pill: Rect, row: ClockRow) -> f64 {
    pill.y + TOP + if row.home { HOME_DROP } else { 0.0 }
}

fn world_h(row: ClockRow) -> f64 {
    if row.world {
        WORLD_H
    } else {
        0.0
    }
}

/// Control Centre's height in a style.
pub fn cc_height(row: ClockRow) -> f64 {
    let home = if row.home { HOME_DROP } else { 0.0 };
    match row.style {
        CcStyle::Classic => 168.0 + quick_h(row) + world_h(row),
        CcStyle::Modules | CcStyle::Sliders => TOP + home + HEAD_H + world_h(row) + BODY_H + 18.0,
        CcStyle::Dock => TOP + home + DOCK_HEAD + world_h(row) + DOCK_H + 16.0,
    }
}

/// Where the body (under the header) starts.
fn body_y(pill: Rect, row: ClockRow) -> f64 {
    top(pill, row)
        + world_h(row)
        + match row.style {
            CcStyle::Dock => DOCK_HEAD,
            _ => HEAD_H,
        }
}

/// Every place in the layout (empty for Classic).
pub fn cc_layout(pill: Rect, row: ClockRow) -> Vec<Spot> {
    match row.style {
        CcStyle::Classic => Vec::new(),
        CcStyle::Modules => modules(pill, row),
        CcStyle::Sliders => sliders(pill, row),
        CcStyle::Dock => dock(pill, row),
    }
}

/// Modules: the block's columns, and how many tiles go beside it.
fn modules_shape(row: ClockRow) -> (usize, usize) {
    let all = cc_buttons(row);
    let timer = all.contains(&CcButton::Timer);
    let others = all.len() - usize::from(timer);
    let beside = if timer { 4 } else { 8 };
    let cols = if others > 4 + beside { 3 } else { 2 };
    (cols, beside)
}

fn modules_block(pill: Rect, row: ClockRow) -> Rect {
    let (cols, _) = modules_shape(row);
    Rect::new(pill.x + PAD, body_y(pill, row), 10.0 + cols as f64 * 52.0, BODY_H)
}

fn modules(pill: Rect, row: ClockRow) -> Vec<Spot> {
    let all = cc_buttons(row);
    let (cols, beside) = modules_shape(row);
    let block = modules_block(pill, row);
    let b = block.y;
    let others: Vec<CcButton> = all.iter().copied().filter(|x| *x != CcButton::Timer).collect();
    let mut v = Vec::new();
    for (i, button) in others.iter().take(cols * 2).enumerate() {
        let (c, r) = ((i % cols) as f64, (i / cols) as f64);
        v.push(Spot::new(Rect::new(block.x + 10.0 + c * 52.0, b + 10.0 + r * 52.0, 42.0, 42.0), *button, Look::Disc));
    }
    let rx = block.right() + 10.0;
    let rw = pill.right() - PAD - rx;
    let mut ry = b;
    if all.contains(&CcButton::Timer) {
        let tile = Rect::new(rx, b, rw, 52.0);
        let go = Rect::new(tile.right() - 10.0 - 32.0, b + 10.0, 32.0, 32.0);
        let plus = Rect::new(go.x - 6.0 - 28.0, b + 12.0, 28.0, 28.0);
        let minus = Rect::new(plus.x - 6.0 - 28.0, b + 12.0, 28.0, 28.0);
        v.push(Spot {
            r: minus,
            button: CcButton::Timer,
            hit: Some(ConnHit::Quick(Quick::Step(-1))),
            look: Look::Step,
        });
        v.push(Spot { r: plus, button: CcButton::Timer, hit: Some(ConnHit::Quick(Quick::Step(1))), look: Look::Step });
        v.push(Spot::new(go, CcButton::Timer, Look::Start));
        v.push(Spot { r: tile, button: CcButton::Timer, hit: None, look: Look::TimerTile });
        ry += 62.0;
    }
    let rest: Vec<CcButton> = others.iter().skip(cols * 2).take(beside).copied().collect();
    let per = rest.len().clamp(1, 4) as f64;
    let w = (rw - (per - 1.0) * 8.0) / per;
    for (i, button) in rest.iter().enumerate() {
        let (c, r) = ((i % 4) as f64, (i / 4) as f64);
        v.push(Spot::new(Rect::new(rx + c * (w + 8.0), ry + r * 62.0, w, 52.0), *button, Look::Tile));
    }
    v
}

/// Sliders: the volume and brightness sliders (the ones the computer
/// reports a level for), tall, at the left of the body.
pub fn cc_sliders(pill: Rect, row: ClockRow) -> Vec<(Rect, Tune)> {
    if row.style != CcStyle::Sliders {
        return Vec::new();
    }
    let b = body_y(pill, row);
    [(Tune::Volume, row.volume), (Tune::Brightness, row.brightness)]
        .into_iter()
        .filter(|(_, known)| *known)
        .enumerate()
        .map(|(i, (tune, _))| (Rect::new(pill.x + PAD + i as f64 * (SLIDER_W + 10.0), b, SLIDER_W, BODY_H), tune))
        .collect()
}

/// The level (0..=1) for a pointer at y on a tall slider: full at the top.
pub fn cc_slider_level(r: Rect, y: f64) -> f32 {
    ((r.bottom() - y) / r.h).clamp(0.0, 1.0) as f32
}

fn sliders(pill: Rect, row: ClockRow) -> Vec<Spot> {
    let n = cc_sliders(pill, row).len() as f64;
    let gx = pill.x + PAD + n * (SLIDER_W + 10.0);
    let gw = pill.right() - PAD - gx;
    let b = body_y(pill, row);
    let rh = (BODY_H - 16.0) / 3.0;
    let u = (gw - 24.0) / 4.0;
    let all = cc_buttons(row);
    // Four units a row, three rows. Wi-Fi and Bluetooth take two (with
    // their names), the timer three (with − and +); when that is too many,
    // they shrink to one each.
    let pack = |wide: bool| {
        let (mut r, mut c) = (0usize, 0usize);
        let mut out = Vec::new();
        for button in &all {
            let units = match button {
                CcButton::Wifi | CcButton::Bluetooth if wide => 2,
                CcButton::Timer if wide => 3,
                _ => 1,
            };
            if c + units > 4 {
                r += 1;
                c = 0;
            }
            if r >= 3 {
                break;
            }
            out.push((*button, r, c, units));
            c += units;
        }
        out
    };
    let mut placed = pack(true);
    if placed.len() < all.len() {
        placed = pack(false);
    }
    let mut v = Vec::new();
    for (button, r, c, units) in placed {
        let rect = Rect::new(
            gx + c as f64 * (u + 8.0),
            b + r as f64 * (rh + 8.0),
            units as f64 * u + (units as f64 - 1.0) * 8.0,
            rh,
        );
        match units {
            3 => {
                let go = Rect::new(rect.x + 4.0, rect.y + 4.0, rh - 8.0, rh - 8.0);
                let minus = Rect::new(go.right() + 4.0, rect.y, 26.0, rh);
                let plus = Rect::new(rect.right() - 28.0, rect.y, 26.0, rh);
                v.push(Spot { r: minus, button, hit: Some(ConnHit::Quick(Quick::Step(-1))), look: Look::Step });
                v.push(Spot { r: plus, button, hit: Some(ConnHit::Quick(Quick::Step(1))), look: Look::Step });
                v.push(Spot::new(go, button, Look::Start));
                v.push(Spot { r: rect, button, hit: None, look: Look::TimerTile });
            }
            2 => v.push(Spot::new(rect, button, Look::Wide)),
            _ => v.push(Spot::new(rect, button, Look::Tile)),
        }
    }
    v
}

fn dock_tile(pill: Rect, row: ClockRow) -> Rect {
    Rect::new(pill.x + PAD - 6.0, body_y(pill, row), pill.w - 2.0 * PAD + 12.0, DOCK_H)
}

fn dock(pill: Rect, row: ClockRow) -> Vec<Spot> {
    let tile = dock_tile(pill, row);
    let all: Vec<CcButton> = cc_buttons(row).into_iter().take(DOCK_MAX).collect();
    let n = all.len() as f64;
    let cw = tile.w / n.max(6.0);
    let x0 = tile.x + (tile.w - n * cw) / 2.0;
    all.iter()
        .enumerate()
        .map(|(i, b)| Spot::new(Rect::new(x0 + i as f64 * cw, tile.y + 4.0, cw, DOCK_H - 8.0), *b, Look::Docked))
        .collect()
}

/// What Control Centre shows besides its buttons.
pub struct CcView<'a> {
    pub now: Option<&'a LocalTime>,
    pub h24: bool,
    pub note: Option<ClockNote<'a>>,
    pub weather: Option<&'a Weather>,
    pub fahrenheit: bool,
    pub conn: &'a Connectivity,
    pub shelf: usize,
    pub todo_left: usize,
    pub minutes: u32,
    pub volume: Option<(f32, bool)>,
    pub brightness: Option<f32>,
    pub world: &'a [(String, LocalTime, i32)],
    /// A background photo is drawn under it (no colour wash then).
    pub photo: bool,
}

/// Control Centre in one of the new styles.
pub fn cc_elements(pill: Rect, row: ClockRow, view: &CcView, o: f32, engine: &mut TextEngine) -> Vec<Element> {
    let mut v = Vec::new();
    match row.style {
        CcStyle::Classic => return v,
        CcStyle::Dock => {
            if !view.photo {
                // A soft wash of the accent colour into near-black.
                let a = accent();
                let deep = a.map(|c| (f32::from(c) * 0.32 + 10.0) as u8);
                v.push(Element::Tile {
                    x: pill.x as f32,
                    y: pill.y as f32,
                    w: pill.w as f32,
                    h: pill.h as f32,
                    radius: 40.0,
                    color: deep,
                    color2: [10, 10, 20],
                    opacity: o,
                });
            }
            v.extend(dock_header(pill, row, view, o, engine));
            let t = dock_tile(pill, row);
            v.push(tile(t, 26.0, WHITE, o * 0.09));
        }
        CcStyle::Modules | CcStyle::Sliders => {
            v.extend(header(pill, row, view, o, engine));
            if row.style == CcStyle::Modules {
                v.push(tile(modules_block(pill, row), 24.0, WHITE, o * 0.10));
            }
            for (r, tune) in cc_sliders(pill, row) {
                let level = match tune {
                    Tune::Volume => view.volume.map_or(0.0, |(l, _)| l),
                    _ => view.brightness.unwrap_or(0.0),
                };
                v.extend(tall_slider(r, tune, level, view.volume.is_some_and(|(_, m)| m), o));
            }
        }
    }
    let spots = cc_layout(pill, row);
    // The timer's tile first: its buttons sit on it.
    for s in spots.iter().filter(|s| s.look == Look::TimerTile) {
        v.extend(spot_elements(s, row, view, o, engine));
    }
    for s in spots.iter().filter(|s| s.look != Look::TimerTile) {
        v.extend(spot_elements(s, row, view, o, engine));
    }
    v
}

fn tile(r: Rect, radius: f32, color: [u8; 3], opacity: f32) -> Element {
    Element::Tile { x: r.x as f32, y: r.y as f32, w: r.w as f32, h: r.h as f32, radius, color, color2: color, opacity }
}

fn centred_icon(icon: Icon, r: Rect, size: f32, color: [u8; 3], o: f32) -> Element {
    Element::Icon {
        icon,
        x: (r.x + r.w / 2.0) as f32 - size / 2.0,
        y: (r.y + r.h / 2.0) as f32 - size / 2.0,
        size,
        color,
        opacity: o,
    }
}

/// How many files or to-dos, in a badge on a button's corner.
fn badge(button: CcButton, view: &CcView, right: f32, top: f32, o: f32) -> Vec<Element> {
    let count = match button {
        CcButton::Shelf => view.shelf,
        CcButton::Todo => view.todo_left,
        _ => 0,
    };
    if count == 0 {
        return Vec::new();
    }
    let n = if count > 9 { "9+".to_string() } else { count.to_string() };
    let style =
        TextStyle { size: 10.0, weight: 700, opacity: o, align: Align::Center, tabular: true, ..TextStyle::default() };
    let (bx, by) = (right - 3.0, top + 3.0);
    vec![
        Element::Ring { cx: bx, cy: by, r: 4.0, width: 8.0, fraction: 1.0, color: accent(), opacity: o },
        text(n, style, bx, by + 3.6),
    ]
}

fn spot_elements(s: &Spot, row: ClockRow, view: &CcView, o: f32, engine: &mut TextEngine) -> Vec<Element> {
    let mut v = Vec::new();
    let r = s.r;
    let (x, y, w, h) = (r.x as f32, r.y as f32, r.w as f32, r.h as f32);
    let on = s.button.on(view.conn);
    let disc_color = if on { accent() } else { WHITE };
    let disc_alpha = if on { 0.92 } else { 0.16 };
    match s.look {
        Look::Disc => {
            v.push(Element::Bar { x, y, w, h, color: disc_color, opacity: o * disc_alpha });
            v.push(centred_icon(s.button.icon(view.conn), r, 20.0, WHITE, o));
            v.extend(badge(s.button, view, x + w, y, o));
        }
        Look::Tile => {
            if s.button == CcButton::Timer {
                v.push(tile(r, 16.0, ORANGE, o));
                v.push(centred_icon(Icon::Play, r, 18.0, [0, 0, 0], o));
            } else {
                v.push(tile(r, 16.0, WHITE, o * 0.10));
                v.push(centred_icon(s.button.icon(view.conn), r, 18.0, WHITE, o));
                v.extend(badge(s.button, view, x + w, y, o));
            }
        }
        Look::Wide => {
            v.push(tile(r, 14.0, WHITE, o * 0.10));
            let d = 24.0f32;
            let (dx, dy) = (x + 5.0, y + (h - d) / 2.0);
            v.push(Element::Bar { x: dx, y: dy, w: d, h: d, color: disc_color, opacity: o * disc_alpha });
            v.push(Element::Icon {
                icon: s.button.icon(view.conn),
                x: dx + 5.0,
                y: dy + 5.0,
                size: 14.0,
                color: WHITE,
                opacity: o,
            });
            let (name, state) = match s.button {
                CcButton::Wifi => (
                    tr("Wi-Fi").to_string(),
                    match view.conn.wifi_network() {
                        _ if !view.conn.wifi_on() => tr("Off").to_string(),
                        Some(n) => n.to_string(),
                        None => tr("On").to_string(),
                    },
                ),
                _ => (
                    tr("Bluetooth").to_string(),
                    match view.conn.connected_device() {
                        _ if !view.conn.bt_on() => tr("Off").to_string(),
                        Some(d) => d.name.clone(),
                        None => tr("On").to_string(),
                    },
                ),
            };
            let room = w - 40.0;
            let name_style = TextStyle { size: 12.0, weight: 600, opacity: o, ..TextStyle::default() };
            let state_style = TextStyle { size: 10.5, weight: 500, opacity: o * 0.55, ..TextStyle::default() };
            v.push(text(engine.ellipsize(&name, &name_style, room), name_style, x + 35.0, y + h / 2.0 - 1.0));
            v.push(text(engine.ellipsize(&state, &state_style, room), state_style, x + 35.0, y + h / 2.0 + 11.0));
        }
        Look::Docked => {
            let d = 36.0f32;
            let (dx, dy) = (x + w / 2.0 - d / 2.0, y + 6.0);
            let disc = Rect::new(f64::from(dx), f64::from(dy), f64::from(d), f64::from(d));
            if s.button == CcButton::Timer {
                v.push(Element::Bar { x: dx, y: dy, w: d, h: d, color: ORANGE, opacity: o });
                v.push(centred_icon(Icon::Play, disc, 16.0, [0, 0, 0], o));
            } else {
                v.push(Element::Bar { x: dx, y: dy, w: d, h: d, color: disc_color, opacity: o * disc_alpha });
                v.push(centred_icon(s.button.icon(view.conn), disc, 18.0, WHITE, o));
                v.extend(badge(s.button, view, dx + d, dy, o));
            }
            let style = TextStyle {
                size: 9.5,
                weight: 500,
                opacity: o * 0.78,
                align: Align::Center,
                tabular: true,
                ..TextStyle::default()
            };
            // A long name gets a little smaller before it is cut short.
            let words = s.button.label(view.minutes);
            let mut style = style;
            while style.size > 8.0 && engine.measure(&words, &style) > w - 3.0 {
                style.size -= 0.25;
            }
            let label = engine.ellipsize(&words, &style, w - 3.0);
            v.push(text(label, style, x + w / 2.0, y + 56.0));
        }
        Look::TimerTile => {
            let minutes = tf("{} min", &[&view.minutes]);
            match row.style {
                CcStyle::Modules => {
                    v.push(tile(r, 20.0, WHITE, o * 0.10));
                    // The buttons on it, for the room left for the words.
                    let minus_x = cc_layout_minus_x(s, row);
                    let room = (minus_x as f32 - x - 18.0).max(10.0);
                    let small = TextStyle { size: 11.0, weight: 500, opacity: o * 0.55, ..TextStyle::default() };
                    let big = TextStyle { size: 17.0, weight: 600, opacity: o, tabular: true, ..TextStyle::default() };
                    let word = tr("Timer");
                    v.push(text(engine.ellipsize(word, &small, room), small, x + 14.0, y + 21.0));
                    v.push(text(engine.ellipsize(&minutes, &big, room), big, x + 14.0, y + 42.0));
                }
                _ => {
                    v.push(tile(r, 14.0, WHITE, o * 0.10));
                    let (left, right) = (x + h + 30.0, x + w - 28.0);
                    let style = TextStyle {
                        size: 13.0,
                        weight: 600,
                        opacity: o,
                        align: Align::Center,
                        tabular: true,
                        ..TextStyle::default()
                    };
                    let label = engine.ellipsize(&minutes, &style, (right - left).max(10.0));
                    v.push(text(label, style, (left + right) / 2.0, y + h / 2.0 + 4.5));
                }
            }
        }
        Look::Start => {
            v.push(Element::Bar { x, y, w, h, color: ORANGE, opacity: o });
            v.push(centred_icon(Icon::Play, r, (w * 0.5).min(16.0), [0, 0, 0], o));
        }
        Look::Step => {
            let d = w.min(h).min(28.0);
            let (dx, dy) = (x + (w - d) / 2.0, y + (h - d) / 2.0);
            v.push(Element::Bar { x: dx, y: dy, w: d, h: d, color: WHITE, opacity: o * 0.16 });
            let minus = s.hit == Some(ConnHit::Quick(Quick::Step(-1)));
            if minus {
                v.push(Element::Bar {
                    x: dx + d / 2.0 - 5.0,
                    y: dy + d / 2.0 - 1.0,
                    w: 10.0,
                    h: 2.0,
                    color: WHITE,
                    opacity: o,
                });
            } else {
                v.push(Element::Icon {
                    icon: Icon::Plus,
                    x: dx + d / 2.0 - 7.0,
                    y: dy + d / 2.0 - 7.0,
                    size: 14.0,
                    color: WHITE,
                    opacity: o,
                });
            }
        }
    }
    v
}

/// Where the timer tile's − button starts (Modules).
fn cc_layout_minus_x(timer: &Spot, row: ClockRow) -> f64 {
    let _ = row;
    timer.r.right() - 10.0 - 32.0 - 6.0 - 28.0 - 6.0 - 28.0
}

/// A tall slider: a rounded track filling from the bottom, its icon at the foot.
fn tall_slider(r: Rect, tune: Tune, level: f32, muted: bool, o: f32) -> Vec<Element> {
    let mut v = vec![tile(r, 18.0, WHITE, o * 0.13)];
    let fill = f64::from(level.clamp(0.0, 1.0)) * r.h;
    if fill > 1.0 {
        let f = Rect::new(r.x, r.bottom() - fill, r.w, fill);
        v.push(tile(f, 18.0f32.min(fill as f32 / 2.0), WHITE, o * 0.92));
    }
    let icon = match tune {
        Tune::Volume if muted || level <= 0.001 => Icon::Volume(255),
        Tune::Volume => Icon::Volume(((level * 3.0).ceil() as u8).clamp(1, 3)),
        _ => Icon::Sun,
    };
    let on_fill = fill > 34.0;
    let size = 20.0f32;
    v.push(Element::Icon {
        icon,
        x: (r.x + r.w / 2.0) as f32 - size / 2.0,
        y: r.bottom() as f32 - 12.0 - size,
        size,
        color: if on_fill { [0, 0, 0] } else { WHITE },
        opacity: o * if on_fill { 0.75 } else { 1.0 },
    });
    v
}

/// The weather at the top right: the sky, the temperature, the day's high
/// and low, and Open-Meteo's credit.
fn weather_corner(
    pill: Rect,
    top: f32,
    w: &Weather,
    fahrenheit: bool,
    o: f32,
    engine: &mut TextEngine,
) -> Vec<Element> {
    use islet_core::weather::format_temp;
    let right = (pill.right() - PAD) as f32;
    let temp_style =
        TextStyle { size: 22.0, weight: 600, opacity: o, tabular: true, align: Align::Right, ..TextStyle::default() };
    let temp = format_temp(w.temp_c, fahrenheit);
    let tw = engine.measure(&temp, &temp_style);
    let small = TextStyle { size: 11.0, weight: 500, opacity: o * 0.55, align: Align::Right, ..TextStyle::default() };
    let mut v = vec![
        text(temp, temp_style, right, top + 28.0),
        Element::Icon { icon: sky_icon(w), x: right - tw - 28.0, y: top + 9.0, size: 22.0, color: WHITE, opacity: o },
    ];
    if let (Some(h), Some(l)) = (w.high_c, w.low_c) {
        v.push(text(
            format!("H {}  L {}", format_temp(h, fahrenheit), format_temp(l, fahrenheit)),
            small,
            right,
            top + 45.0,
        ));
    }
    v.push(text("Open-Meteo", TextStyle { size: 9.0, opacity: o * 0.4, ..small }, right, top + 58.0));
    v
}

/// The time, the date (or the next alarm) and the weather: Modules and Sliders.
fn header(pill: Rect, row: ClockRow, view: &CcView, o: f32, engine: &mut TextEngine) -> Vec<Element> {
    let mut v = Vec::new();
    let t = top(pill, row) as f32;
    let left = (pill.x + PAD) as f32 + 2.0;
    let right = (pill.right() - PAD) as f32;
    if let Some(now) = view.now {
        let (time, suffix) = format_time(now, view.h24);
        let style = TextStyle { size: 34.0, weight: 600, opacity: o, tabular: true, ..TextStyle::default() };
        let x = left + engine.measure(&time, &style) + 5.0;
        v.push(text(time, style, left, t + 34.0));
        if let Some(s) = suffix {
            v.push(text(s, TextStyle { size: 15.0, ..style }, x, t + 34.0));
        }
        let room_end = right - if view.weather.is_some() { 92.0 } else { 0.0 };
        v.extend(date_line(now, view.note, left, t + 55.0, room_end, false, o, engine));
    }
    if let Some(w) = view.weather {
        v.extend(weather_corner(pill, t, w, view.fahrenheit, o, engine));
    }
    if !view.world.is_empty() {
        let pieces = world_pieces(view.world, view.h24, right - left, o, engine);
        v.extend(pieces.into_iter().map(|(s, style, dx)| text(s, style, left + dx, t + 76.0)));
    }
    v
}

/// The date, or the short date and the note beside it; centred when `centre`.
#[allow(clippy::too_many_arguments)]
fn date_line(
    now: &LocalTime,
    note: Option<ClockNote<'_>>,
    left: f32,
    baseline: f32,
    right: f32,
    centre: bool,
    o: f32,
    engine: &mut TextEngine,
) -> Vec<Element> {
    let date_style = TextStyle { size: 13.0, weight: 500, opacity: o * 0.6, ..TextStyle::default() };
    let Some(note) = note else {
        let date = engine.ellipsize(&format_date(now), &date_style, right - left);
        let x = if centre { (left + right) / 2.0 - engine.measure(&date, &date_style) / 2.0 } else { left };
        return vec![text(date, date_style, x, baseline)];
    };
    let date = islet_core::clock::format_date_short(now);
    let dw = engine.measure(&date, &date_style);
    let note_style =
        TextStyle { size: 12.5, weight: 600, opacity: o, color: note.color, tabular: true, ..TextStyle::default() };
    let room = (right - left - dw - 28.0).max(20.0);
    let words = engine.ellipsize(note.text, &note_style, room);
    let total = dw + 10.0 + 17.0 + engine.measure(&words, &note_style);
    let x = if centre { (left + right) / 2.0 - total / 2.0 } else { left };
    vec![
        text(date, date_style, x, baseline),
        Element::Icon {
            icon: note.icon,
            x: x + dw + 10.0,
            y: baseline - 11.0,
            size: 13.0,
            color: note.color,
            opacity: o,
        },
        text(words, note_style, x + dw + 27.0, baseline - 0.5),
    ]
}

/// Dock's header: the network at the top left, the weather at the top
/// right, a big clock and the date in the middle.
fn dock_header(pill: Rect, row: ClockRow, view: &CcView, o: f32, engine: &mut TextEngine) -> Vec<Element> {
    let mut v = Vec::new();
    let t = top(pill, row) as f32;
    let left = (pill.x + PAD) as f32;
    let right = (pill.right() - PAD) as f32;
    let centre = (pill.x + pill.w / 2.0) as f32;
    let small = TextStyle { size: 12.0, weight: 500, opacity: o * 0.7, ..TextStyle::default() };
    let c = view.conn;
    let net = match c.wifi_network() {
        _ if !c.wifi_on() => tr("Wi-Fi off").to_string(),
        Some(n) => n.to_string(),
        None => tr("Wi-Fi").to_string(),
    };
    v.push(Element::Icon { icon: CcButton::Wifi.icon(c), x: left, y: t, size: 14.0, color: WHITE, opacity: o * 0.7 });
    v.push(text(engine.ellipsize(&net, &small, 150.0), small, left + 19.0, t + 11.0));
    if let Some(w) = view.weather {
        use islet_core::weather::format_temp;
        let temp_style = TextStyle {
            size: 13.0,
            weight: 600,
            opacity: o,
            tabular: true,
            align: Align::Right,
            ..TextStyle::default()
        };
        let temp = format_temp(w.temp_c, view.fahrenheit);
        let tw = engine.measure(&temp, &temp_style);
        v.push(text(temp, temp_style, right, t + 11.0));
        v.push(Element::Icon {
            icon: sky_icon(w),
            x: right - tw - 21.0,
            y: t - 2.0,
            size: 17.0,
            color: WHITE,
            opacity: o,
        });
        let credit = TextStyle { size: 9.0, opacity: o * 0.4, align: Align::Right, ..TextStyle::default() };
        v.push(text("Open-Meteo", credit, right, t + 24.0));
    }
    if let Some(now) = view.now {
        let (time, suffix) = format_time(now, view.h24);
        let style = TextStyle { size: 50.0, weight: 600, opacity: o, tabular: true, ..TextStyle::default() };
        let suffix_style = TextStyle { size: 18.0, opacity: o * 0.8, ..style };
        let tw = engine.measure(&time, &style);
        let sw = suffix.as_ref().map_or(0.0, |s| engine.measure(s, &suffix_style) + 5.0);
        let x = centre - (tw + sw) / 2.0;
        v.push(text(time, style, x, t + 66.0));
        if let Some(s) = suffix {
            v.push(text(s, suffix_style, x + tw + 5.0, t + 66.0));
        }
        v.extend(date_line(now, view.note, left, t + 88.0, right, true, o, engine));
    }
    if !view.world.is_empty() {
        let pieces = world_pieces(view.world, view.h24, right - left, o, engine);
        let total = pieces.last().map_or(0.0, |(s, style, dx)| dx + engine.measure(s, style));
        let x = centre - total / 2.0;
        v.extend(pieces.into_iter().map(|(s, style, dx)| text(s, style, x + dx, t + 110.0)));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pill(row: ClockRow) -> Rect {
        Rect::new(24.0, 0.0, 372.0, cc_height(row))
    }

    fn inside(outer: Rect, r: Rect) -> bool {
        r.x >= outer.x - 0.01
            && r.y >= outer.y - 0.01
            && r.right() <= outer.right() + 0.01
            && r.bottom() <= outer.bottom() + 0.01
    }

    fn rows() -> Vec<ClockRow> {
        let mut out = Vec::new();
        for style in [CcStyle::Modules, CcStyle::Sliders, CcStyle::Dock] {
            for (world, home, extras) in
                [(false, false, false), (true, true, true), (false, true, true), (true, false, false)]
            {
                for (volume, brightness) in [(true, true), (false, false), (true, false)] {
                    out.push(ClockRow {
                        style,
                        world,
                        home,
                        shelf: extras,
                        clipboard: extras,
                        todo: extras,
                        stopwatch: extras,
                        shortcuts: extras,
                        volume,
                        brightness,
                        ..ClockRow::default()
                    });
                }
            }
        }
        out
    }

    #[test]
    fn every_style_fits_the_window_and_keeps_its_buttons_apart() {
        for row in rows() {
            let p = pill(row);
            assert!(p.h <= 260.0, "{row:?} is {} tall", p.h);
            let spots = cc_layout(p, row);
            let taps: Vec<&Spot> = spots.iter().filter(|s| s.hit.is_some()).collect();
            for s in &spots {
                assert!(inside(p, s.r), "{:?} {:?} outside the card in {row:?}", s.button, s.look);
            }
            for (i, a) in taps.iter().enumerate() {
                for b in &taps[i + 1..] {
                    let e = 0.01;
                    let overlap = a.r.x + e < b.r.right()
                        && b.r.x + e < a.r.right()
                        && a.r.y + e < b.r.bottom()
                        && b.r.y + e < a.r.bottom();
                    assert!(!overlap, "{:?} and {:?} overlap in {row:?}", a.button, b.button);
                }
            }
            for (r, _) in cc_sliders(p, row) {
                assert!(inside(p, r));
                assert!(taps.iter().all(|s| s.r.x >= r.right()), "a button under a slider in {row:?}");
            }
        }
    }

    #[test]
    fn the_usual_buttons_all_show_and_follow_the_order() {
        for style in [CcStyle::Modules, CcStyle::Sliders, CcStyle::Dock] {
            let row = ClockRow { style, volume: true, brightness: true, ..ClockRow::default() };
            let shown: Vec<CcButton> =
                cc_layout(pill(row), row).iter().filter(|s| s.hit.is_some()).map(|s| s.button).collect();
            for b in cc_buttons(row) {
                assert!(shown.contains(&b), "{b:?} missing from {style:?}");
            }
        }
        // Lock first: the dock starts with it.
        let order = CcOrder::from_ids(&["lock", "nonsense", "lock", "wifi"]);
        assert_eq!(&order.0[..3], &[CcButton::Lock, CcButton::Wifi, CcButton::Bluetooth]);
        let row = ClockRow { style: CcStyle::Dock, order, ..ClockRow::default() };
        let first = cc_layout(pill(row), row).into_iter().min_by(|a, b| a.r.x.total_cmp(&b.r.x)).unwrap();
        assert_eq!(first.button, CcButton::Lock);
        assert_eq!(first.hit, Some(ConnHit::Quick(Quick::Lock)));
        for b in CcButton::ALL {
            assert_eq!(CcButton::from_id(b.id()), Some(b));
        }
    }

    #[test]
    fn tall_sliders_fill_from_the_bottom() {
        let row = ClockRow { style: CcStyle::Sliders, volume: true, brightness: true, ..ClockRow::default() };
        let s = cc_sliders(pill(row), row);
        assert_eq!(s.iter().map(|(_, t)| *t).collect::<Vec<_>>(), vec![Tune::Volume, Tune::Brightness]);
        let r = s[0].0;
        assert_eq!(cc_slider_level(r, r.bottom()), 0.0);
        assert_eq!(cc_slider_level(r, r.y), 1.0);
        assert!((cc_slider_level(r, r.y + r.h / 4.0) - 0.75).abs() < 0.01);
        assert!(cc_sliders(pill(row), ClockRow { style: CcStyle::Dock, ..row }).is_empty());
    }

    #[test]
    fn styles_parse_with_classic_as_the_fallback() {
        for (name, style) in islet_core::prefs::CC_STYLES.iter().zip([
            CcStyle::Classic,
            CcStyle::Modules,
            CcStyle::Sliders,
            CcStyle::Dock,
        ]) {
            assert_eq!(CcStyle::parse(name), style);
        }
        assert_eq!(CcStyle::parse("nonsense"), CcStyle::Classic);
    }
}
