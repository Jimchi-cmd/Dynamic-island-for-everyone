//! What is drawn inside the island for each activity and presentation
//! (docs/design-spec.md §3). Pure layout: rects in, display list out.

use islet_core::activity::{BatteryDevice, Custom, DeviceKind, Hud, Live, MediaInfo, NavInfo, Note, Part};
use islet_core::alarm::{self, Alarm};
use islet_core::clock::{format_date, format_time, LocalTime};
use islet_core::connectivity::{self, BtState, Connectivity, WifiState};
use islet_core::geometry::Rect;
use islet_core::nav;
use islet_core::timer::{self, Phase, Timer};
use islet_render::elements::{Element, Icon};
use islet_render::text::{Align, TextEngine, TextItem, TextStyle};
use islet_render::Circle;

pub const WHITE: [u8; 3] = [255, 255, 255];
pub const AMBER: [u8; 3] = [255, 176, 32];
pub const RED: [u8; 3] = [255, 69, 58];
pub const GREEN: [u8; 3] = [48, 209, 88];

/// Buttons of the expanded navigation card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavControl {
    GoogleMaps,
    Waze,
    Stop,
}

pub const GREEN_NAV: [u8; 3] = [52, 199, 89];
pub const ORANGE: [u8; 3] = [255, 159, 10];
/// Control-centre blue for "on" buttons, and a lighter one for text on black.
pub const BLUE: [u8; 3] = [10, 132, 255];
pub const BLUE_TEXT: [u8; 3] = [64, 156, 255];

/// The Wi-Fi and Bluetooth lists that open from the clock card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Wifi,
    Bluetooth,
    Shelf,
}

/// What a tap on the clock card or a list lands on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnHit {
    /// A button on the clock card.
    Open(Panel),
    /// The list header: back to the clock.
    Back,
    Row(usize),
    /// The footer button ("Enter the password in Settings").
    Footer,
    /// The × on a shelf row.
    Remove(usize),
}

pub const ROW_H: f64 = 36.0;
pub const MAX_ROWS: usize = 5;
const LIST_TOP: f64 = 52.0;

fn hud_icon(h: &Hud) -> Icon {
    match *h {
        Hud::Volume { muted: true, .. } => Icon::Volume(255),
        Hud::Volume { level, .. } if level <= 0.001 => Icon::Volume(0),
        Hud::Volume { level, .. } => Icon::Volume(if level < 0.34 {
            1
        } else if level < 0.67 {
            2
        } else {
            3
        }),
        Hud::Brightness(_) => Icon::Sun,
        Hud::CapsLock(_) => Icon::CapsLock,
    }
}

/// A level bar (track plus fill) ending at `right`.
fn level_bar(level: f32, right: f32, cy: f32, w: f32, dim: bool, o: f32) -> [Element; 2] {
    let (x, h) = (right - w, 6.0);
    let fill = (w * level.clamp(0.0, 1.0)).max(if level > 0.0 { h } else { 0.0 });
    [
        Element::Bar { x, y: cy - h / 2.0, w, h, color: WHITE, opacity: o * 0.25 },
        Element::Bar { x, y: cy - h / 2.0, w: fill, h, color: WHITE, opacity: o * if dim { 0.45 } else { 1.0 } },
    ]
}

/// The HUD's right-hand side: a level bar, or caps lock's state.
fn hud_trailing(h: &Hud, right: f32, cy: f32, bar_w: f32, o: f32) -> Vec<Element> {
    match *h {
        Hud::Volume { level, muted } => level_bar(level, right, cy, bar_w, muted, o).to_vec(),
        Hud::Brightness(level) => level_bar(level, right, cy, bar_w, false, o).to_vec(),
        Hud::CapsLock(on) => {
            let style = TextStyle {
                size: 14.0,
                weight: 600,
                opacity: o * if on { 1.0 } else { 0.6 },
                color: if on { GREEN } else { WHITE },
                align: Align::Right,
                ..TextStyle::default()
            };
            vec![text(if on { "On" } else { "Off" }, style, right, cy + 5.0)]
        }
    }
}

/// "in 8 min" / "now", from app-clock times.
fn relative_label(starts_at: f64, t: f64) -> String {
    islet_core::calendar::relative(starts_at.ceil() as i64, t.floor() as i64)
}

pub const NOTE_ROWS: usize = 3;

pub fn notes_height(rows: usize) -> f64 {
    46.0 + rows.clamp(1, NOTE_ROWS) as f64 * 52.0 + 4.0
}

/// Recent notifications: app, title and the first line of the text.
pub fn expanded_notes<'a>(
    pill: Rect,
    notes: impl Iterator<Item = &'a Note>,
    o: f32,
    engine: &mut TextEngine,
) -> Vec<Element> {
    let (x, y, w) = (pill.x as f32, pill.y as f32, pill.w as f32);
    let pad = PAD as f32;
    let head = TextStyle { size: 16.0, weight: 600, opacity: o, ..TextStyle::default() };
    let mut v = vec![
        Element::Icon { icon: Icon::Message, x: x + pad, y: y + 18.0, size: 20.0, color: GREEN, opacity: o },
        text("Notifications", head, x + pad + 28.0, y + 33.0),
    ];
    let app = TextStyle { size: 12.0, opacity: o * 0.6, ..TextStyle::default() };
    let title = TextStyle { size: 14.5, weight: 600, opacity: o, ..TextStyle::default() };
    let body = TextStyle { size: 13.0, opacity: o * 0.8, ..TextStyle::default() };
    let width = w - 2.0 * pad;
    for (i, n) in notes.take(NOTE_ROWS).enumerate() {
        let ry = y + 46.0 + i as f32 * 52.0;
        v.push(text(engine.ellipsize(&n.app, &app, width), app, x + pad, ry + 12.0));
        v.push(text(engine.ellipsize(&n.summary, &title, width), title, x + pad, ry + 29.0));
        let first = n.body.lines().next().unwrap_or("");
        if !first.is_empty() {
            v.push(text(engine.ellipsize(first, &body, width), body, x + pad, ry + 45.0));
        }
    }
    v
}

/// A row of the agenda card: start time, title, place.
pub struct AgendaRow {
    pub time: String,
    pub title: String,
    pub location: Option<String>,
    pub now: bool,
}

pub const AGENDA_ROWS: usize = 4;

pub fn agenda_height(rows: usize) -> f64 {
    48.0 + rows.clamp(1, AGENDA_ROWS) as f64 * 40.0 + 6.0
}

/// The agenda card: the next few events.
pub fn expanded_agenda(pill: Rect, rows: &[AgendaRow], o: f32, engine: &mut TextEngine) -> Vec<Element> {
    let (x, y, w) = (pill.x as f32, pill.y as f32, pill.w as f32);
    let pad = PAD as f32;
    let title_style = TextStyle { size: 16.0, weight: 600, opacity: o, ..TextStyle::default() };
    let mut v = vec![
        Element::Icon { icon: Icon::Calendar, x: x + pad, y: y + 18.0, size: 20.0, color: RED, opacity: o },
        text("Up next", title_style, x + pad + 28.0, y + 33.0),
    ];
    let time_style = TextStyle { size: 14.0, weight: 600, opacity: o, tabular: true, ..TextStyle::default() };
    let name_style = TextStyle { size: 14.5, weight: 600, opacity: o, ..TextStyle::default() };
    let small = TextStyle { size: 12.0, opacity: o * 0.6, ..TextStyle::default() };
    for (i, r) in rows.iter().take(AGENDA_ROWS).enumerate() {
        let ry = y + 48.0 + i as f32 * 40.0;
        let color = if r.now { RED } else { WHITE };
        v.push(text(r.time.clone(), TextStyle { color, ..time_style }, x + pad, ry + 18.0));
        let name = engine.ellipsize(&r.title, &name_style, w - 2.0 * pad - 78.0);
        v.push(text(name, name_style, x + pad + 78.0, ry + 18.0));
        if let Some(loc) = &r.location {
            v.push(text(engine.ellipsize(loc, &small, w - 2.0 * pad - 78.0), small, x + pad + 78.0, ry + 34.0));
        }
    }
    if rows.is_empty() {
        let style = TextStyle { size: 14.0, opacity: o * 0.6, ..TextStyle::default() };
        v.push(text("Nothing else today", style, x + pad, y + 70.0));
    }
    v
}

pub fn custom_color(c: &Custom) -> [u8; 3] {
    c.color.unwrap_or(BLUE_TEXT)
}

/// "40%", or the subtitle when there's no known amount.
fn custom_right(c: &Custom) -> String {
    match c.progress {
        Some(p) => format!("{}%", (p.clamp(0.0, 1.0) * 100.0).round() as i32),
        None => String::new(),
    }
}

/// A progress ring, or a turning arc when the amount isn't known.
fn custom_ring(c: &Custom, cx: f32, cy: f32, r: f32, t: f64, o: f32) -> Element {
    let color = custom_color(c);
    match c.progress {
        Some(p) => Element::Ring { cx, cy, r, width: 2.6, fraction: p.clamp(0.0, 1.0), color, opacity: o },
        // Ring draws from 12 o'clock; vary the length so it visibly works.
        None => {
            Element::Ring { cx, cy, r, width: 2.6, fraction: 0.25 + 0.15 * (t * 3.0).sin() as f32, color, opacity: o }
        }
    }
}

pub const CUSTOM_H: f64 = 112.0;

/// A script activity's card: title, subtitle and a progress bar.
pub fn expanded_custom(pill: Rect, c: &Custom, t: f64, o: f32, engine: &mut TextEngine) -> Vec<Element> {
    let (x, y, w) = (pill.x as f32, pill.y as f32, pill.w as f32);
    let pad = PAD as f32;
    let color = custom_color(c);
    let title = TextStyle { size: 16.0, weight: 600, opacity: o, ..TextStyle::default() };
    let pct = custom_right(c);
    let rs = TextStyle { size: 22.0, weight: 600, opacity: o, color, tabular: true, align: Align::Right };
    let room = w - 2.0 * pad - 40.0 - engine.measure(&pct, &rs) - 12.0;
    let mut v = vec![
        custom_ring(c, x + pad + 13.0, y + 36.0, 11.0, t, o),
        text(engine.ellipsize(&c.title, &title, room), title, x + pad + 38.0, y + 34.0),
        text(pct, rs, x + w - pad, y + 42.0),
    ];
    if !c.subtitle.is_empty() {
        let small = TextStyle { size: 13.0, opacity: o * 0.6, ..TextStyle::default() };
        v.push(text(engine.ellipsize(&c.subtitle, &small, room), small, x + pad + 38.0, y + 53.0));
    }
    let (bx, by, bw) = (x + pad, y + 78.0, w - 2.0 * pad);
    v.push(Element::Bar { x: bx, y: by, w: bw, h: 6.0, color: WHITE, opacity: o * 0.2 });
    let fill = match c.progress {
        Some(p) => bw * p.clamp(0.0, 1.0),
        None => bw * 0.3,
    };
    let start = match c.progress {
        Some(_) => bx,
        // Unknown amount: a segment sweeping back and forth.
        None => bx + (bw - fill) * (0.5 + 0.5 * (t * 1.6).sin() as f32),
    };
    v.push(Element::Bar { x: start, y: by, w: fill.max(6.0), h: 6.0, color, opacity: o });
    v
}

/// Buttons of the timer card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerControl {
    PauseResume,
    Stop,
}

/// The ringing card shown when a timer finishes uses this alarm id.
pub const TIMER_ALARM_ID: u32 = 0;

fn timer_color(tm: &Timer) -> [u8; 3] {
    if tm.phase == Phase::Break {
        GREEN
    } else {
        ORANGE
    }
}

/// A countdown ring: the remaining share of the phase.
fn timer_ring(tm: &Timer, cx: f32, cy: f32, r: f32, t: f64, o: f32) -> Element {
    Element::Ring {
        cx,
        cy,
        r,
        width: 2.6,
        fraction: 1.0 - tm.progress(t),
        color: timer_color(tm),
        opacity: o * if tm.running() { 1.0 } else { 0.6 },
    }
}

fn countdown(tm: &Timer, right: f32, cy: f32, size: f32, t: f64, o: f32) -> Element {
    let style = TextStyle {
        size,
        weight: 600,
        opacity: o * if tm.running() { 1.0 } else { 0.6 },
        tabular: true,
        color: timer_color(tm),
        align: Align::Right,
    };
    text(timer::format_countdown(tm.remaining(t)), style, right, cy + size * 0.36)
}

/// Buttons of the ringing-alarm card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlarmControl {
    Snooze,
    Stop,
}

/// The bell's swing while ringing: a quick shake, then a pause, repeating.
pub fn bell_tilt(t: f64) -> i8 {
    let phase = t.rem_euclid(1.2);
    if phase < 0.6 {
        let fade = 1.0 - phase / 0.6;
        (18.0 * fade * (phase * std::f64::consts::TAU * 7.0).sin()).round() as i8
    } else {
        0
    }
}

fn bell(cx: f32, cy: f32, size: f32, t: f64, o: f32) -> Element {
    Element::Icon {
        icon: Icon::Bell(bell_tilt(t)),
        x: cx - size / 2.0,
        y: cy - size / 2.0,
        size,
        color: ORANGE,
        opacity: o,
    }
}

/// Interactive areas of the expanded media card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Previous,
    PlayPause,
    Next,
    Scrub,
}

pub const COMPACT_MEDIA_W: f64 = 236.0;
pub const COMPACT_BATTERY_W: f64 = 300.0;

pub fn compact_width(live: &Live) -> f64 {
    match live {
        Live::Media(_) | Live::Navigation(_) | Live::Alarm(_) | Live::Timer(_) | Live::Hud(_) | Live::Event { .. } => {
            COMPACT_MEDIA_W
        }
        Live::Note(_) => COMPACT_BATTERY_W,
        Live::Custom(_) => COMPACT_BATTERY_W,
        Live::Battery { .. } => COMPACT_BATTERY_W,
    }
}

pub fn device_icon(kind: DeviceKind) -> Icon {
    match kind {
        DeviceKind::Headphones => Icon::Headphones,
        DeviceKind::Earbuds => Icon::Earbuds,
        DeviceKind::Speaker => Icon::Speaker,
        DeviceKind::Mouse => Icon::Mouse,
        DeviceKind::Keyboard => Icon::Keyboard,
        DeviceKind::Gamepad => Icon::Gamepad,
        DeviceKind::Phone => Icon::Phone,
        DeviceKind::Tablet => Icon::Tablet,
        DeviceKind::Laptop => Icon::Laptop,
        DeviceKind::Other => Icon::Bluetooth,
    }
}

/// Ring and percentage colour: green while charging, red ≤ 10 %, amber ≤ 20 %.
pub fn level_color(percent: u8, charging: bool) -> [u8; 3] {
    if charging {
        GREEN
    } else if percent <= 10 {
        RED
    } else if percent <= 20 {
        AMBER
    } else {
        WHITE
    }
}

fn text(s: impl Into<String>, style: TextStyle, x: f32, baseline: f32) -> Element {
    Element::Text(TextItem { text: s.into(), style, x, baseline })
}

/// "3:07", "1:02:03"
pub fn format_duration(secs: f64) -> String {
    let s = secs.max(0.0).round() as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// Animated equaliser bars, centred on `cy`. Static stubs while paused.
pub fn waveform(x: f32, cy: f32, n: usize, t: f64, playing: bool, color: [u8; 3], o: f32) -> Vec<Element> {
    const SPEEDS: [f64; 6] = [1.7, 2.3, 1.3, 2.9, 2.1, 1.5];
    const PHASES: [f64; 6] = [0.0, 1.1, 2.3, 0.6, 1.9, 2.8];
    (0..n)
        .map(|i| {
            let h = if playing {
                let s = (t * SPEEDS[i % 6] * std::f64::consts::TAU + PHASES[i % 6]).sin();
                let s2 = (t * SPEEDS[(i + 3) % 6] * 4.1 + PHASES[(i + 1) % 6]).sin();
                (4.0 + 11.0 * (0.5 + 0.35 * s + 0.15 * s2)) as f32
            } else {
                3.0
            };
            Element::Bar { x: x + i as f32 * 5.5, y: cy - h / 2.0, w: 3.0, h, color, opacity: o }
        })
        .collect()
}

/// Compact: one activity beside the pill.
pub fn compact(pill: Rect, live: &Live, t: f64, o: f32, engine: &mut TextEngine) -> Vec<Element> {
    let (x, right) = (pill.x as f32, pill.right() as f32);
    let cy = (pill.y + pill.h / 2.0) as f32;
    match live {
        Live::Timer(tm) => vec![timer_ring(tm, x + 20.0, cy, 8.5, t, o), countdown(tm, right - 14.0, cy, 15.0, t, o)],
        Live::Custom(c) => {
            let color = custom_color(c);
            let title = TextStyle { size: 13.5, weight: 600, opacity: o, ..TextStyle::default() };
            let right_text = custom_right(c);
            let rs = TextStyle { size: 13.0, weight: 600, opacity: o, color, tabular: true, align: Align::Right };
            let room = (right - x) - 40.0 - engine.measure(&right_text, &rs) - 24.0;
            vec![
                custom_ring(c, x + 20.0, cy, 8.5, t, o),
                text(engine.ellipsize(&c.title, &title, room), title, x + 38.0, cy + 4.5),
                text(right_text, rs, right - 14.0, cy + 4.5),
            ]
        }
        Live::Note(n) => {
            let app = TextStyle { size: 12.5, opacity: o * 0.6, ..TextStyle::default() };
            let title = TextStyle { size: 13.5, weight: 600, opacity: o, ..TextStyle::default() };
            let app_label = engine.ellipsize(&n.app, &app, 90.0);
            let app_w = engine.measure(&app_label, &app);
            let line = if n.summary.is_empty() { n.body.clone() } else { n.summary.clone() };
            let room = (right - x) - 38.0 - app_w - 26.0;
            vec![
                Element::Icon { icon: Icon::Message, x: x + 11.0, y: cy - 9.0, size: 18.0, color: GREEN, opacity: o },
                text(app_label, app, x + 36.0, cy + 4.5),
                text(engine.ellipsize(&line, &title, room), title, x + 36.0 + app_w + 8.0, cy + 4.5),
            ]
        }
        Live::Event { event, starts_at } => {
            let when = TextStyle {
                size: 13.0,
                weight: 600,
                opacity: o,
                color: RED,
                align: Align::Right,
                ..TextStyle::default()
            };
            let label = relative_label(*starts_at, t);
            let title = TextStyle { size: 13.5, weight: 600, opacity: o, ..TextStyle::default() };
            let room = (right - x) - 50.0 - engine.measure(&label, &when) - 18.0;
            vec![
                Element::Icon { icon: Icon::Calendar, x: x + 10.0, y: cy - 10.0, size: 20.0, color: RED, opacity: o },
                text(engine.ellipsize(&event.summary, &title, room), title, x + 38.0, cy + 4.5),
                text(label, when, right - 14.0, cy + 4.5),
            ]
        }
        Live::Hud(h) => {
            let mut v = vec![Element::Icon {
                icon: hud_icon(h),
                x: x + 10.0,
                y: cy - 10.0,
                size: 20.0,
                color: WHITE,
                opacity: o,
            }];
            if let Hud::CapsLock(_) = h {
                let style = TextStyle { size: 14.0, weight: 600, opacity: o, ..TextStyle::default() };
                v.push(text("Caps Lock", style, x + 38.0, cy + 5.0));
            }
            v.extend(hud_trailing(h, right - 16.0, cy, 96.0, o));
            v
        }
        Live::Alarm(a) => {
            // The alarm's name on the left, the swinging bell on the right.
            let style = TextStyle { size: 14.0, weight: 600, opacity: o, color: ORANGE, ..TextStyle::default() };
            let title = engine.ellipsize(a.title(), &style, (right - x) - 70.0);
            vec![text(title, style, x + 16.0, cy + 5.0), bell(right - 22.0, cy, 20.0, t, o)]
        }
        Live::Media(m) => {
            let tint = m.art.as_ref().map_or(WHITE, |a| brighten(a.dominant));
            let mut v = vec![Element::Art {
                x: x + 6.0,
                y: cy - 12.5,
                size: 25.0,
                radius: 7.0,
                image: m.art.clone(),
                opacity: o,
            }];
            v.extend(waveform(right - 31.0, cy, 4, t, m.playing, tint, o));
            v
        }
        Live::Navigation(n) => {
            vec![
                Element::Icon {
                    icon: Icon::Navigation,
                    x: x + 9.0,
                    y: cy - 10.0,
                    size: 20.0,
                    color: GREEN_NAV,
                    opacity: o,
                },
                nav_eta(n, right - 13.0, cy, o),
            ]
        }
        Live::Battery { device: d, .. } => {
            let color = level_color(d.percent, d.charging);
            let name_style = TextStyle { size: 13.0, opacity: o * 0.7, ..TextStyle::default() };
            let pct_style =
                TextStyle { size: 14.0, weight: 600, opacity: o, tabular: true, color, align: Align::Right };
            let mut v = vec![Element::Icon {
                icon: device_icon(d.kind),
                x: x + 11.0,
                y: cy - 10.0,
                size: 20.0,
                color: WHITE,
                opacity: o,
            }];
            let name = engine.ellipsize(&d.name, &name_style, 150.0);
            v.push(text(name, name_style, x + 39.0, cy + 4.5));
            v.push(text(format!("{}%", d.percent), pct_style, right - 40.0, cy + 5.0));
            v.push(Element::Ring {
                cx: right - 22.0,
                cy,
                r: 8.0,
                width: 2.6,
                fraction: d.percent as f32 / 100.0,
                color,
                opacity: o,
            });
            if d.charging {
                v.push(Element::Icon {
                    icon: Icon::Bolt,
                    x: right - 28.0,
                    y: cy - 6.0,
                    size: 12.0,
                    color: GREEN,
                    opacity: o,
                });
            }
            v
        }
    }
}

/// The ETA at the trailing edge of the pill ("26 min", "…" while loading, "!" on error).
fn nav_eta(n: &NavInfo, right: f32, cy: f32, o: f32) -> Element {
    let (label, color) = match &n.route {
        Some(r) => (nav::format_minutes(r.duration), WHITE),
        None if n.error.is_some() => ("!".to_string(), AMBER),
        None => ("…".to_string(), WHITE),
    };
    let style = TextStyle { size: 14.0, weight: 600, opacity: o, tabular: true, color, align: Align::Right };
    text(label, style, right, cy + 5.0)
}

/// Minimal: the primary activity's leading view in the pill and the
/// secondary activity in the detached circle.
pub fn minimal(
    pill: Rect,
    bubble: Option<Circle>,
    primary: &Live,
    secondary: Option<&Live>,
    t: f64,
    o: f32,
    h24: bool,
) -> Vec<Element> {
    let cy = (pill.y + pill.h / 2.0) as f32;
    let x = pill.x as f32;
    let mut v = leading(primary, x + 6.0, cy, o, h24);
    if let Live::Media(m) = primary {
        let tint = m.art.as_ref().map_or(WHITE, |a| brighten(a.dominant));
        v.extend(waveform(pill.right() as f32 - 31.0, cy, 4, t, m.playing, tint, o));
    }
    if let Live::Navigation(n) = primary {
        v.push(nav_eta(n, pill.right() as f32 - 13.0, cy, o));
    }
    if let Live::Alarm(_) = primary {
        v.push(bell(pill.right() as f32 - 22.0, cy, 20.0, t, o));
    }
    if let Live::Timer(tm) = primary {
        v.push(countdown(tm, pill.right() as f32 - 13.0, cy, 14.0, t, o));
    }
    if let Live::Hud(h) = primary {
        v.extend(hud_trailing(h, pill.right() as f32 - 14.0, cy, 56.0, o));
    }
    if let Live::Custom(c) = primary {
        let rs = TextStyle {
            size: 13.0,
            weight: 600,
            opacity: o,
            color: custom_color(c),
            tabular: true,
            align: Align::Right,
        };
        v.push(text(custom_right(c), rs, pill.right() as f32 - 13.0, cy + 4.5));
    }
    if let Live::Event { starts_at, .. } = primary {
        let when =
            TextStyle { size: 13.0, weight: 600, opacity: o, color: RED, align: Align::Right, ..TextStyle::default() };
        v.push(text(relative_label(*starts_at, t), when, pill.right() as f32 - 13.0, cy + 4.5));
    }
    if let (Some(b), Some(s)) = (bubble, secondary) {
        match s {
            Live::Battery { device: d, .. } => v.push(Element::Ring {
                cx: b.x,
                cy: b.y,
                r: b.r - 7.5,
                width: 2.6,
                fraction: d.percent as f32 / 100.0,
                color: level_color(d.percent, d.charging),
                opacity: o,
            }),
            Live::Alarm(_) => v.push(bell(b.x, b.y, 20.0, t, o)),
            Live::Timer(tm) => v.push(timer_ring(tm, b.x, b.y, b.r - 7.5, t, o)),
            Live::Custom(c) => v.push(custom_ring(c, b.x, b.y, b.r - 7.5, t, o)),
            Live::Note(_) => v.push(Element::Icon {
                icon: Icon::Message,
                x: b.x - 9.0,
                y: b.y - 9.0,
                size: 18.0,
                color: GREEN,
                opacity: o,
            }),
            Live::Event { .. } => v.push(Element::Icon {
                icon: Icon::Calendar,
                x: b.x - 9.0,
                y: b.y - 9.0,
                size: 18.0,
                color: RED,
                opacity: o,
            }),
            Live::Hud(h) => v.push(Element::Icon {
                icon: hud_icon(h),
                x: b.x - 9.0,
                y: b.y - 9.0,
                size: 18.0,
                color: WHITE,
                opacity: o,
            }),
            Live::Navigation(_) => v.push(Element::Icon {
                icon: Icon::Navigation,
                x: b.x - 10.0,
                y: b.y - 10.0,
                size: 20.0,
                color: GREEN_NAV,
                opacity: o,
            }),
            Live::Media(m) => v.push(Element::Art {
                x: b.x - 12.5,
                y: b.y - 12.5,
                size: 25.0,
                radius: 12.5,
                image: m.art.clone(),
                opacity: o,
            }),
        }
    }
    v
}

fn leading(live: &Live, x: f32, cy: f32, o: f32, h24: bool) -> Vec<Element> {
    match live {
        Live::Custom(c) => vec![Element::Ring {
            cx: x + 12.0,
            cy,
            r: 8.5,
            width: 2.6,
            fraction: c.progress.unwrap_or(0.3),
            color: custom_color(c),
            opacity: o,
        }],
        Live::Note(_) => {
            vec![Element::Icon { icon: Icon::Message, x: x + 3.0, y: cy - 9.0, size: 18.0, color: GREEN, opacity: o }]
        }
        Live::Event { .. } => {
            vec![Element::Icon { icon: Icon::Calendar, x: x + 3.0, y: cy - 10.0, size: 20.0, color: RED, opacity: o }]
        }
        Live::Hud(h) => {
            vec![Element::Icon { icon: hud_icon(h), x: x + 3.0, y: cy - 10.0, size: 20.0, color: WHITE, opacity: o }]
        }
        Live::Timer(tm) => vec![Element::Ring {
            cx: x + 12.0,
            cy,
            r: 8.5,
            width: 2.6,
            fraction: 1.0,
            color: timer_color(tm),
            opacity: o,
        }],
        Live::Alarm(a) => {
            let style =
                TextStyle { size: 14.0, weight: 600, opacity: o, color: ORANGE, tabular: true, ..TextStyle::default() };
            vec![text(alarm::format_hm(a.hour, a.minute, h24), style, x + 8.0, cy + 5.0)]
        }
        Live::Navigation(_) => {
            vec![Element::Icon {
                icon: Icon::Navigation,
                x: x + 3.0,
                y: cy - 10.0,
                size: 20.0,
                color: GREEN_NAV,
                opacity: o,
            }]
        }
        Live::Media(m) => {
            vec![Element::Art { x, y: cy - 12.5, size: 25.0, radius: 7.0, image: m.art.clone(), opacity: o }]
        }
        Live::Battery { device: d, .. } => {
            vec![Element::Icon {
                icon: device_icon(d.kind),
                x: x + 3.0,
                y: cy - 10.0,
                size: 20.0,
                color: WHITE,
                opacity: o,
            }]
        }
    }
}

const PAD: f64 = 20.0;

/// Hit areas of the expanded media card, relative to the pill.
pub fn media_controls(pill: Rect) -> [(Rect, Control); 4] {
    let cx = pill.x + pill.w / 2.0;
    let cy = pill.y + 136.0;
    let button = |dx: f64| Rect::new(cx + dx - 22.0, cy - 22.0, 44.0, 44.0);
    [
        (button(-64.0), Control::Previous),
        (button(0.0), Control::PlayPause),
        (button(64.0), Control::Next),
        (Rect::new(pill.x + PAD, pill.y + 82.0, pill.w - 2.0 * PAD, 22.0), Control::Scrub),
    ]
}

/// Fraction along the progress bar for a pointer x.
pub fn scrub_fraction(pill: Rect, x: f64) -> f64 {
    ((x - pill.x - PAD) / (pill.w - 2.0 * PAD)).clamp(0.0, 1.0)
}

/// The expanded "Now Playing" card.
pub fn expanded_media(
    pill: Rect,
    m: &MediaInfo,
    t: f64,
    o: f32,
    scrub: Option<f64>,
    engine: &mut TextEngine,
) -> Vec<Element> {
    let (x, y, w) = (pill.x as f32, pill.y as f32, pill.w as f32);
    let pad = PAD as f32;
    let tint = m.art.as_ref().map_or(WHITE, |a| brighten(a.dominant));
    let mut v =
        vec![Element::Art { x: x + pad, y: y + pad, size: 56.0, radius: 12.0, image: m.art.clone(), opacity: o }];

    let text_x = x + 88.0;
    let text_w = w - 88.0 - 52.0;
    let title_style = TextStyle { size: 16.0, weight: 600, opacity: o, ..TextStyle::default() };
    let artist_style = TextStyle { size: 14.0, opacity: o * 0.6, ..TextStyle::default() };
    let title = if m.title.is_empty() { m.app_name.clone() } else { m.title.clone() };
    v.push(text(engine.ellipsize(&title, &title_style, text_w), title_style, text_x, y + 44.0));
    let artist = if m.artist.is_empty() { m.app_name.clone() } else { m.artist.clone() };
    v.push(text(engine.ellipsize(&artist, &artist_style, text_w), artist_style, text_x, y + 64.0));
    v.extend(waveform(x + w - pad - 19.5, y + 38.0, 4, t, m.playing, tint, o));

    // Progress bar with elapsed / remaining time.
    let track_w = w - 2.0 * pad;
    let (by, bh) = (y + 92.0, if scrub.is_some() { 7.0 } else { 4.0 });
    v.push(Element::Bar { x: x + pad, y: by - bh / 2.0 + 2.0, w: track_w, h: bh, color: WHITE, opacity: o * 0.2 });
    if let (Some(pos), Some(dur)) = (m.position_now(t), m.duration.filter(|d| *d > 0.0)) {
        let frac = scrub.unwrap_or(pos / dur).clamp(0.0, 1.0);
        let shown = frac * dur;
        v.push(Element::Bar {
            x: x + pad,
            y: by - bh / 2.0 + 2.0,
            w: (track_w * frac as f32).max(bh),
            h: bh,
            color: tint,
            opacity: o,
        });
        let time_style = TextStyle { size: 11.0, opacity: o * 0.6, tabular: true, ..TextStyle::default() };
        v.push(text(format_duration(shown), time_style, x + pad, y + 112.0));
        let remaining = TextStyle { align: Align::Right, ..time_style };
        v.push(text(format!("-{}", format_duration(dur - shown)), remaining, x + w - pad, y + 112.0));
    }

    // Transport controls.
    let cx = x + w / 2.0;
    let cy = y + 136.0;
    let dim = |enabled: bool| if enabled { o } else { o * 0.35 };
    let play_icon = if m.playing { Icon::Pause } else { Icon::Play };
    v.push(Element::Icon {
        icon: Icon::Previous,
        x: cx - 64.0 - 13.0,
        y: cy - 13.0,
        size: 26.0,
        color: WHITE,
        opacity: dim(m.caps.previous),
    });
    v.push(Element::Icon {
        icon: play_icon,
        x: cx - 15.0,
        y: cy - 15.0,
        size: 30.0,
        color: WHITE,
        opacity: dim(m.caps.play_pause),
    });
    v.push(Element::Icon {
        icon: Icon::Next,
        x: cx + 64.0 - 13.0,
        y: cy - 13.0,
        size: 26.0,
        color: WHITE,
        opacity: dim(m.caps.next),
    });
    v
}

/// Buttons along the bottom of the navigation card.
/// Height of the timer card.
pub const TIMER_H: f64 = 132.0;

pub fn timer_controls(pill: Rect) -> [(Rect, TimerControl); 2] {
    let [(a, _), (b, _)] = alarm_controls(pill);
    [(a, TimerControl::PauseResume), (b, TimerControl::Stop)]
}

/// The timer card: phase, a big countdown, Pause/Resume and Stop.
pub fn expanded_timer(pill: Rect, tm: &Timer, t: f64, o: f32, engine: &mut TextEngine) -> Vec<Element> {
    let (x, y, w) = (pill.x as f32, pill.y as f32, pill.w as f32);
    let pad = PAD as f32;
    let mut v = vec![timer_ring(tm, x + pad + 13.0, y + 38.0, 12.0, t, o)];
    let title_style = TextStyle { size: 16.0, weight: 600, opacity: o, ..TextStyle::default() };
    v.push(text(engine.ellipsize(&tm.title(), &title_style, w * 0.45), title_style, x + pad + 38.0, y + 34.0));
    let small = TextStyle { size: 13.0, opacity: o * 0.6, ..TextStyle::default() };
    let sub = if tm.running() { "Running" } else { "Paused" };
    v.push(text(sub, small, x + pad + 38.0, y + 53.0));
    v.push(countdown(tm, x + w - pad, y + 38.0, 34.0, t, o));
    let label = TextStyle { size: 14.0, weight: 600, opacity: o, align: Align::Center, ..TextStyle::default() };
    for (r, c) in timer_controls(pill) {
        let (bx, by, bw, bh) = (r.x as f32, r.y as f32, r.w as f32, r.h as f32);
        let (fill, alpha, word, color) = match c {
            TimerControl::PauseResume => (WHITE, 0.16, if tm.running() { "Pause" } else { "Resume" }, WHITE),
            TimerControl::Stop => (timer_color(tm), 1.0, "Stop", [0, 0, 0]),
        };
        v.push(Element::Bar { x: bx, y: by, w: bw, h: bh, color: fill, opacity: o * alpha });
        v.push(text(word, TextStyle { color, ..label }, bx + bw / 2.0, by + 22.0));
    }
    v
}

/// Height of the ringing-alarm card.
pub const ALARM_H: f64 = 132.0;

pub fn alarm_controls(pill: Rect) -> [(Rect, AlarmControl); 2] {
    let y = pill.y + 84.0;
    let (x, w) = (pill.x + PAD, pill.w - 2.0 * PAD);
    let half = (w - 10.0) / 2.0;
    [
        (Rect::new(x, y, half, 34.0), AlarmControl::Snooze),
        (Rect::new(x + half + 10.0, y, half, 34.0), AlarmControl::Stop),
    ]
}

/// The ringing-alarm card: bell, name, time, Snooze and Stop.
pub fn expanded_alarm(pill: Rect, a: &Alarm, h24: bool, t: f64, o: f32, engine: &mut TextEngine) -> Vec<Element> {
    let (x, y, w) = (pill.x as f32, pill.y as f32, pill.w as f32);
    let pad = PAD as f32;
    let mut v = vec![bell(x + pad + 13.0, y + 36.0, 28.0, t, o)];
    let time_style =
        TextStyle { size: 26.0, weight: 600, opacity: o, tabular: true, align: Align::Right, ..TextStyle::default() };
    let time = alarm::format_hm(a.hour, a.minute, h24);
    let time_w = engine.measure(&time, &time_style);
    let title_style = TextStyle { size: 16.0, weight: 600, opacity: o, ..TextStyle::default() };
    let title = engine.ellipsize(a.title(), &title_style, w - 2.0 * pad - 40.0 - time_w - 12.0);
    v.push(text(title, title_style, x + pad + 38.0, y + 34.0));
    let small = TextStyle { size: 13.0, opacity: o * 0.6, ..TextStyle::default() };
    let sub = if a.id == TIMER_ALARM_ID { "Timer".to_string() } else { alarm::describe_days(a.days) };
    v.push(text(sub, small, x + pad + 38.0, y + 53.0));
    v.push(text(time, time_style, x + w - pad, y + 46.0));
    let label = TextStyle { size: 14.0, weight: 600, opacity: o, align: Align::Center, ..TextStyle::default() };
    for (r, c) in alarm_controls(pill) {
        let (bx, by, bw, bh) = (r.x as f32, r.y as f32, r.w as f32, r.h as f32);
        let (fill, alpha, word, color) = match c {
            AlarmControl::Snooze if a.id == TIMER_ALARM_ID => (WHITE, 0.16, "Repeat", WHITE),
            AlarmControl::Snooze => (WHITE, 0.16, "Snooze", WHITE),
            AlarmControl::Stop => (ORANGE, 1.0, "Stop", [0, 0, 0]),
        };
        v.push(Element::Bar { x: bx, y: by, w: bw, h: bh, color: fill, opacity: o * alpha });
        v.push(text(word, TextStyle { color, ..label }, bx + bw / 2.0, by + 22.0));
    }
    v
}

pub fn nav_controls(pill: Rect) -> [(Rect, NavControl); 3] {
    let y = pill.y + 118.0;
    let (x, w) = (pill.x + PAD, pill.w - 2.0 * PAD);
    let gap = 8.0;
    let stop_w = 44.0;
    let half = (w - stop_w - 2.0 * gap) / 2.0;
    [
        (Rect::new(x, y, half, 30.0), NavControl::GoogleMaps),
        (Rect::new(x + half + gap, y, half, 30.0), NavControl::Waze),
        (Rect::new(x + w - stop_w, y, stop_w, 30.0), NavControl::Stop),
    ]
}

/// The expanded navigation card: destination, ETA, arrival, distance,
/// traffic, the first instruction, and hand-off buttons.
pub fn expanded_nav(pill: Rect, n: &NavInfo, now: Option<&LocalTime>, o: f32, engine: &mut TextEngine) -> Vec<Element> {
    let (x, y, w) = (pill.x as f32, pill.y as f32, pill.w as f32);
    let pad = PAD as f32;
    let mut v = vec![Element::Icon {
        icon: Icon::Navigation,
        x: x + pad,
        y: y + 22.0,
        size: 24.0,
        color: GREEN_NAV,
        opacity: o,
    }];
    let title_style = TextStyle { size: 16.0, weight: 600, opacity: o, ..TextStyle::default() };
    let eta_style =
        TextStyle { size: 22.0, weight: 600, opacity: o, tabular: true, align: Align::Right, ..TextStyle::default() };
    let small = TextStyle { size: 13.0, opacity: o * 0.6, ..TextStyle::default() };
    let eta = n.route.as_ref().map(|r| nav::format_minutes(r.duration)).unwrap_or_else(|| "…".into());
    let eta_w = engine.measure(&eta, &eta_style);
    v.push(text(
        engine.ellipsize(n.trip.title(), &title_style, w - 2.0 * pad - 36.0 - eta_w - 12.0),
        title_style,
        x + pad + 34.0,
        y + 40.0,
    ));
    v.push(text(eta, eta_style, x + w - pad, y + 42.0));
    match (&n.route, &n.error) {
        (Some(r), _) => {
            let mut line = nav::format_distance(r.distance_m);
            if let Some(now) = now {
                line = format!("Arrive {} · {line}", nav::arrival(now.hour, now.minute, r.duration));
            }
            v.push(text(line, small, x + pad + 34.0, y + 62.0));
            if r.delay() >= 60.0 {
                let style = TextStyle { color: AMBER, opacity: o, align: Align::Right, ..small };
                v.push(text(format!("+{} traffic", nav::format_minutes(r.delay())), style, x + w - pad, y + 62.0));
            }
            // Routes data shown without a Google map must credit Google Maps.
            let credit = TextStyle { size: 11.0, opacity: o * 0.5, align: Align::Right, ..small };
            v.push(text("Google Maps", credit, x + w - pad, y + 92.0));
            if let Some(step) = &r.first_step {
                let style = TextStyle { opacity: o * 0.8, ..small };
                let line = engine.ellipsize(&format!("Start: {step}"), &style, w - 2.0 * pad - 84.0);
                v.push(text(line, style, x + pad, y + 92.0));
            }
        }
        (None, Some(err)) => {
            let style = TextStyle { color: AMBER, opacity: o, ..small };
            v.push(text(engine.ellipsize(err, &style, w - 2.0 * pad - 34.0), style, x + pad + 34.0, y + 62.0));
        }
        (None, None) => v.push(text("Getting the route…", small, x + pad + 34.0, y + 62.0)),
    }
    let label = TextStyle { size: 13.0, weight: 600, opacity: o, align: Align::Center, ..TextStyle::default() };
    for (r, c) in nav_controls(pill) {
        let (bx, by, bw) = (r.x as f32, r.y as f32, r.w as f32);
        v.push(Element::Bar { x: bx, y: by, w: bw, h: 30.0, color: WHITE, opacity: o * 0.14 });
        match c {
            NavControl::GoogleMaps => v.push(text("Google Maps", label, bx + bw / 2.0, by + 19.5)),
            NavControl::Waze => v.push(text("Waze", label, bx + bw / 2.0, by + 19.5)),
            NavControl::Stop => v.push(Element::Icon {
                icon: Icon::Close,
                x: bx + bw / 2.0 - 9.0,
                y: by + 6.0,
                size: 18.0,
                color: WHITE,
                opacity: o,
            }),
        }
    }
    v
}

/// Height the expanded battery list needs.
pub fn battery_list_height(rows: usize) -> f64 {
    (24.0 + rows as f64 * 34.0 + 10.0).clamp(84.0, 230.0)
}

/// The expanded list of connected devices (plus this computer's battery).
pub fn expanded_battery(
    pill: Rect,
    devices: &[&BatteryDevice],
    laptop: Option<&BatteryDevice>,
    o: f32,
    engine: &mut TextEngine,
) -> Vec<Element> {
    let (x, y, w) = (pill.x as f32, pill.y as f32, pill.w as f32);
    let mut v = Vec::new();
    let rows = laptop.into_iter().chain(devices.iter().copied());
    for (i, d) in rows.enumerate() {
        let cy = y + 30.0 + i as f32 * 34.0;
        if cy + 14.0 > (pill.bottom() as f32) {
            break;
        }
        let color = level_color(d.percent, d.charging);
        v.push(Element::Icon {
            icon: device_icon(d.kind),
            x: x + 22.0,
            y: cy - 11.0,
            size: 22.0,
            color: WHITE,
            opacity: o,
        });
        let name_style = TextStyle { size: 15.0, weight: 600, opacity: o, ..TextStyle::default() };
        let name = engine.ellipsize(&d.name, &name_style, w - 56.0 - 170.0);
        v.push(text(name, name_style, x + 56.0, cy + 5.0));
        let detail = parts_label(&d.parts);
        if !detail.is_empty() {
            let detail_style =
                TextStyle { size: 12.0, opacity: o * 0.6, tabular: true, align: Align::Right, ..TextStyle::default() };
            v.push(text(detail, detail_style, x + w - 92.0, cy + 4.5));
        }
        let pct = TextStyle { size: 14.0, weight: 600, opacity: o, tabular: true, color, align: Align::Right };
        v.push(text(format!("{}%", d.percent), pct, x + w - 44.0, cy + 5.0));
        v.push(Element::Ring {
            cx: x + w - 28.0,
            cy,
            r: 8.0,
            width: 2.6,
            fraction: d.percent as f32 / 100.0,
            color,
            opacity: o,
        });
    }
    v
}

fn parts_label(parts: &[(Part, u8)]) -> String {
    parts
        .iter()
        .map(|(p, pct)| {
            let name = match p {
                Part::Left => "L",
                Part::Right => "R",
                Part::Case => "Case",
            };
            format!("{name} {pct}%")
        })
        .collect::<Vec<_>>()
        .join("  ")
}

/// The expanded clock (an open island with nothing live).
pub fn expanded_clock(pill: Rect, now: &LocalTime, h24: bool, o: f32, engine: &mut TextEngine) -> Vec<Element> {
    let rise = (1.0 - o) * 8.0; // content settles upward into place
    let (left, top) = (pill.x as f32 + 26.0, pill.y as f32 + rise);
    let (time, suffix) = format_time(now, h24);
    let time_style = TextStyle { size: 52.0, weight: 600, opacity: o, tabular: true, ..TextStyle::default() };
    let mut v = vec![text(time.clone(), time_style, left, top + 74.0)];
    if let Some(suffix) = suffix {
        let x = left + engine.measure(&time, &time_style) + 6.0;
        v.push(text(suffix, TextStyle { size: 20.0, ..time_style }, x, top + 74.0));
    }
    let date_style = TextStyle { size: 15.0, opacity: o * 0.6, ..TextStyle::default() };
    v.push(text(format_date(now), date_style, left + 2.0, top + 102.0));
    v
}

pub fn sky_icon(w: &islet_core::weather::Weather) -> Icon {
    use islet_core::weather::Sky;
    match (w.sky, w.is_day) {
        (Sky::Clear, true) => Icon::Sun,
        (Sky::Clear, false) => Icon::Moon,
        (Sky::PartlyCloudy, true) => Icon::PartlyCloudy,
        (Sky::PartlyCloudy, false) | (Sky::Cloudy, _) => Icon::Cloud,
        (Sky::Fog, _) => Icon::Fog,
        (Sky::Rain, _) => Icon::Rain,
        (Sky::Snow, _) => Icon::Snow,
        (Sky::Storm, _) => Icon::Storm,
    }
}

/// Weather in the clock card's top-right corner, with Open-Meteo's credit.
pub fn clock_weather(pill: Rect, w: &islet_core::weather::Weather, fahrenheit: bool, o: f32) -> Vec<Element> {
    use islet_core::weather::format_temp;
    let (right, top) = ((pill.right() - PAD) as f32, pill.y as f32 + (1.0 - o) * 8.0);
    let temp =
        TextStyle { size: 30.0, weight: 600, opacity: o, tabular: true, align: Align::Right, ..TextStyle::default() };
    let small = TextStyle { size: 12.5, opacity: o * 0.6, align: Align::Right, ..TextStyle::default() };
    let mut v = vec![
        text(format_temp(w.temp_c, fahrenheit), temp, right, top + 58.0),
        Element::Icon { icon: sky_icon(w), x: right - 104.0, y: top + 32.0, size: 28.0, color: WHITE, opacity: o },
    ];
    if let (Some(h), Some(l)) = (w.high_c, w.low_c) {
        v.push(text(
            format!("H {}  L {}", format_temp(h, fahrenheit), format_temp(l, fahrenheit)),
            small,
            right,
            top + 80.0,
        ));
    }
    v.push(text("Open-Meteo", TextStyle { size: 10.0, opacity: o * 0.4, ..small }, right, top + 102.0));
    v
}

/// The Wi-Fi and Bluetooth buttons along the bottom of the clock card.
/// The clock card's buttons: Wi-Fi and Bluetooth, plus the shelf when it
/// has something on it.
pub fn clock_chips(pill: Rect, shelf: bool) -> Vec<(Rect, Panel)> {
    let (x, w) = (pill.x + PAD, pill.w - 2.0 * PAD);
    let y = pill.y + 122.0;
    let panels: &[Panel] =
        if shelf { &[Panel::Wifi, Panel::Bluetooth, Panel::Shelf] } else { &[Panel::Wifi, Panel::Bluetooth] };
    let n = panels.len() as f64;
    let cw = (w - 10.0 * (n - 1.0)) / n;
    panels.iter().enumerate().map(|(i, p)| (Rect::new(x + i as f64 * (cw + 10.0), y, cw, 32.0), *p)).collect()
}

pub fn clock_chip_elements(
    pill: Rect,
    c: &Connectivity,
    shelf: usize,
    o: f32,
    engine: &mut TextEngine,
) -> Vec<Element> {
    let mut v = Vec::new();
    let style = TextStyle { size: 13.0, weight: 600, opacity: o, ..TextStyle::default() };
    for (r, panel) in clock_chips(pill, shelf > 0) {
        let (x, y, w, h) = (r.x as f32, r.y as f32, r.w as f32, r.h as f32);
        let (icon, label, on) = match panel {
            Panel::Wifi => (
                Icon::Wifi(c.wifi.networks.iter().find(|n| n.active).map_or(3, |n| connectivity::bars(n.strength))),
                c.wifi.current.clone().unwrap_or_else(|| "Wi-Fi".into()),
                c.wifi.current.is_some(),
            ),
            Panel::Bluetooth => (
                Icon::Bluetooth,
                c.connected_device().map_or_else(|| "Bluetooth".into(), |d| d.name.clone()),
                c.connected_device().is_some(),
            ),
            Panel::Shelf => (Icon::Folder, format!("Shelf · {shelf}"), false),
        };
        let bg = if on { BLUE } else { WHITE };
        v.push(Element::Bar { x, y, w, h, color: bg, opacity: o * if on { 0.9 } else { 0.14 } });
        v.push(Element::Icon { icon, x: x + 10.0, y: y + h / 2.0 - 9.0, size: 18.0, color: WHITE, opacity: o });
        let label = engine.ellipsize(&label, &style, w - 44.0);
        v.push(text(label, style, x + 34.0, y + h / 2.0 + 4.5));
    }
    v
}

/// Height of a Wi-Fi or Bluetooth list with `rows` entries.
pub fn list_height(rows: usize, footer: bool) -> f64 {
    LIST_TOP + rows.clamp(1, MAX_ROWS) as f64 * ROW_H + if footer { 46.0 } else { 14.0 }
}

pub fn list_rows(pill: Rect, rows: usize) -> Vec<Rect> {
    (0..rows.min(MAX_ROWS))
        .map(|i| Rect::new(pill.x + 10.0, pill.y + LIST_TOP + i as f64 * ROW_H, pill.w - 20.0, ROW_H))
        .collect()
}

pub fn list_header(pill: Rect) -> Rect {
    Rect::new(pill.x, pill.y, pill.w * 0.6, LIST_TOP - 4.0)
}

pub fn list_footer(pill: Rect, rows: usize) -> Rect {
    let y = pill.y + LIST_TOP + rows.clamp(1, MAX_ROWS) as f64 * ROW_H + 6.0;
    Rect::new(pill.x + PAD, y, pill.w - 2.0 * PAD, 30.0)
}

/// A footer line: a status or note, or a button when it leads to Settings.
fn list_footer_elements(
    pill: Rect,
    rows: usize,
    line: &str,
    button: bool,
    o: f32,
    engine: &mut TextEngine,
) -> Vec<Element> {
    let r = list_footer(pill, rows);
    let (x, y, w, h) = (r.x as f32, r.y as f32, r.w as f32, r.h as f32);
    let mut v = Vec::new();
    if button {
        v.push(Element::Bar { x, y, w, h, color: BLUE, opacity: o * 0.9 });
        let style = TextStyle { size: 13.0, weight: 600, opacity: o, align: Align::Center, ..TextStyle::default() };
        v.push(text(engine.ellipsize(line, &style, w - 20.0), style, x + w / 2.0, y + 19.5));
    } else {
        let style = TextStyle { size: 12.5, opacity: o * 0.7, ..TextStyle::default() };
        v.push(text(engine.ellipsize(line, &style, w), style, x, y + 19.0));
    }
    v
}

fn list_header_elements(
    pill: Rect,
    icon: Icon,
    title: &str,
    status: Option<&str>,
    o: f32,
    engine: &mut TextEngine,
) -> Vec<Element> {
    let (x, y, w) = (pill.x as f32, pill.y as f32, pill.w as f32);
    let title_style = TextStyle { size: 16.0, weight: 600, opacity: o, ..TextStyle::default() };
    let mut v = vec![
        text("‹", TextStyle { size: 22.0, opacity: o * 0.7, ..title_style }, x + PAD as f32 - 2.0, y + 35.0),
        Element::Icon { icon, x: x + PAD as f32 + 14.0, y: y + 19.0, size: 20.0, color: WHITE, opacity: o },
        text(title, title_style, x + PAD as f32 + 40.0, y + 35.0),
    ];
    if let Some(s) = status {
        let style = TextStyle { size: 12.5, opacity: o * 0.6, align: Align::Right, ..TextStyle::default() };
        v.push(text(engine.ellipsize(s, &style, w * 0.45), style, x + w - PAD as f32, y + 34.0));
    }
    v
}

/// What the Wi-Fi list's footer says, and whether it is a Settings button.
pub fn wifi_footer(c: &Connectivity) -> Option<(String, bool)> {
    match (&c.failure, &c.wifi.note) {
        (Some((_, Some(ssid))), _) => Some((format!("Enter the password for {ssid} in Settings"), true)),
        (Some((msg, None)), _) => Some((msg.clone(), false)),
        (None, Some(note)) => Some((note.clone(), false)),
        _ => None,
    }
}

pub fn bt_footer(c: &Connectivity) -> Option<(String, bool)> {
    match (&c.failure, &c.bt.note) {
        (Some((msg, _)), _) => Some((msg.clone(), false)),
        (None, Some(note)) if !c.bt.can_connect && c.bt.available => Some(("Open Bluetooth settings".into(), true)),
        (None, Some(note)) => Some((note.clone(), false)),
        _ => None,
    }
}

pub fn expanded_wifi(pill: Rect, c: &Connectivity, o: f32, engine: &mut TextEngine) -> Vec<Element> {
    let w: &WifiState = &c.wifi;
    let mut v = list_header_elements(pill, Icon::Wifi(3), "Wi-Fi", c.busy.as_deref(), o, engine);
    let rows = list_rows(pill, w.networks.len());
    let name_style = TextStyle { size: 15.0, weight: 600, opacity: o, ..TextStyle::default() };
    for (r, n) in rows.iter().zip(&w.networks) {
        let (x, cy, rw) = (r.x as f32, (r.y + r.h / 2.0) as f32, r.w as f32);
        v.push(Element::Icon {
            icon: Icon::Wifi(connectivity::bars(n.strength)),
            x: x + 10.0,
            y: cy - 10.0,
            size: 20.0,
            color: WHITE,
            opacity: o,
        });
        let name = engine.ellipsize(&n.ssid, &name_style, rw - 100.0);
        v.push(text(
            name,
            TextStyle { color: if n.active { BLUE_TEXT } else { WHITE }, ..name_style },
            x + 40.0,
            cy + 5.0,
        ));
        if n.active {
            v.push(Element::Icon {
                icon: Icon::Check,
                x: x + rw - 30.0,
                y: cy - 9.0,
                size: 18.0,
                color: BLUE_TEXT,
                opacity: o,
            });
        } else if n.secure && !n.known {
            v.push(Element::Icon {
                icon: Icon::Lock,
                x: x + rw - 28.0,
                y: cy - 8.0,
                size: 15.0,
                color: WHITE,
                opacity: o * 0.5,
            });
        }
    }
    if w.networks.is_empty() {
        let style = TextStyle { size: 14.0, opacity: o * 0.6, ..TextStyle::default() };
        let msg = if c.busy.is_some() {
            "Looking for networks…"
        } else if w.available {
            "No networks found"
        } else {
            "Wi-Fi isn't available"
        };
        v.push(text(msg, style, pill.x as f32 + 50.0, (pill.y + LIST_TOP) as f32 + 23.0));
    }
    if let Some((line, button)) = wifi_footer(c) {
        v.extend(list_footer_elements(pill, w.networks.len(), &line, button, o, engine));
    }
    v
}

/// The shelf: dropped files (tap to open, × to remove).
pub fn expanded_shelf(
    pill: Rect,
    items: &[islet_core::shelf::ShelfItem],
    dragging: bool,
    o: f32,
    engine: &mut TextEngine,
) -> Vec<Element> {
    let status =
        (!items.is_empty()).then(|| format!("{} item{}", items.len(), if items.len() == 1 { "" } else { "s" }));
    let mut v = list_header_elements(pill, Icon::Folder, "Shelf", status.as_deref(), o, engine);
    let rows = list_rows(pill, items.len());
    let name_style = TextStyle { size: 15.0, weight: 600, opacity: o, ..TextStyle::default() };
    for (r, item) in rows.iter().zip(items) {
        let (x, cy, rw) = (r.x as f32, (r.y + r.h / 2.0) as f32, r.w as f32);
        let icon = if item.dir { Icon::Folder } else { Icon::File };
        v.push(Element::Icon { icon, x: x + 10.0, y: cy - 10.0, size: 20.0, color: WHITE, opacity: o });
        v.push(text(engine.ellipsize(item.name(), &name_style, rw - 90.0), name_style, x + 40.0, cy + 5.0));
        v.push(Element::Icon {
            icon: Icon::Close,
            x: x + rw - 28.0,
            y: cy - 8.0,
            size: 16.0,
            color: WHITE,
            opacity: o * 0.5,
        });
    }
    if items.is_empty() && !dragging {
        let style = TextStyle { size: 14.0, opacity: o * 0.6, ..TextStyle::default() };
        v.push(text(
            "Drag files here to keep them handy",
            style,
            pill.x as f32 + 50.0,
            (pill.y + LIST_TOP) as f32 + 23.0,
        ));
    }
    if let Some((line, button)) = shelf_footer(items.len(), dragging) {
        v.extend(list_footer_elements(pill, items.len(), &line, button, o, engine));
    }
    v
}

pub fn shelf_footer(items: usize, dragging: bool) -> Option<(String, bool)> {
    if dragging {
        Some(("Drop to keep here".into(), true))
    } else if items > 0 {
        Some(("Files stay where they are; the shelf keeps a shortcut".into(), false))
    } else {
        None
    }
}

/// Is (x, y) on a shelf row's × button?
pub fn shelf_remove_hit(row: Rect, x: f64) -> bool {
    x >= row.right() - 40.0
}

pub fn expanded_bluetooth(pill: Rect, c: &Connectivity, o: f32, engine: &mut TextEngine) -> Vec<Element> {
    let b: &BtState = &c.bt;
    let mut v = list_header_elements(pill, Icon::Bluetooth, "Bluetooth", c.busy.as_deref(), o, engine);
    let rows = list_rows(pill, b.devices.len());
    let name_style = TextStyle { size: 15.0, weight: 600, opacity: o, ..TextStyle::default() };
    let state_style = TextStyle { size: 12.5, opacity: o * 0.6, align: Align::Right, ..TextStyle::default() };
    for (r, d) in rows.iter().zip(&b.devices) {
        let (x, cy, rw) = (r.x as f32, (r.y + r.h / 2.0) as f32, r.w as f32);
        v.push(Element::Icon {
            icon: device_icon(d.kind),
            x: x + 10.0,
            y: cy - 10.0,
            size: 20.0,
            color: WHITE,
            opacity: o,
        });
        let name = engine.ellipsize(&d.name, &name_style, rw - 140.0);
        v.push(text(name, name_style, x + 40.0, cy + 5.0));
        let (label, color, alpha) =
            if d.connected { ("Connected", BLUE_TEXT, 1.0) } else { ("Not connected", WHITE, 0.6) };
        v.push(text(label, TextStyle { color, opacity: o * alpha, ..state_style }, x + rw - 10.0, cy + 4.5));
    }
    if b.devices.is_empty() {
        let style = TextStyle { size: 14.0, opacity: o * 0.6, ..TextStyle::default() };
        let msg = if b.available { "No paired devices" } else { "Bluetooth isn't available" };
        v.push(text(msg, style, pill.x as f32 + 50.0, (pill.y + LIST_TOP) as f32 + 23.0));
    }
    if let Some((line, button)) = bt_footer(c) {
        v.extend(list_footer_elements(pill, b.devices.len(), &line, button, o, engine));
    }
    v
}

/// Lift dark album colours so a tinted waveform stays visible on black.
pub fn brighten(c: [u8; 3]) -> [u8; 3] {
    let max = c.iter().copied().max().unwrap_or(0).max(1) as f32;
    if max >= 160.0 {
        return c;
    }
    let k = 200.0 / max;
    c.map(|v| (v as f32 * k).min(255.0) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use islet_core::activity::{MediaCaps, PeekReason};

    fn song() -> MediaInfo {
        MediaInfo {
            app_name: "Spotify".into(),
            title: "A very long song title that will certainly not fit on one line".into(),
            artist: "Artist".into(),
            playing: true,
            position: Some(65.0),
            duration: Some(200.0),
            rate: 1.0,
            caps: MediaCaps { play_pause: true, next: true, previous: false, seek: true },
            ..MediaInfo::default()
        }
    }

    fn texts(v: &[Element]) -> Vec<String> {
        v.iter().filter_map(|e| if let Element::Text(t) = e { Some(t.text.clone()) } else { None }).collect()
    }

    #[test]
    fn durations() {
        assert_eq!(format_duration(65.4), "1:05");
        assert_eq!(format_duration(3723.0), "1:02:03");
        assert_eq!(format_duration(-3.0), "0:00");
    }

    #[test]
    fn expanded_media_shows_title_times_and_controls() {
        let mut engine = TextEngine::new();
        let pill = Rect::new(24.0, 0.0, 372.0, 160.0);
        let v = expanded_media(pill, &song(), 0.0, 1.0, None, &mut engine);
        let t = texts(&v);
        assert!(t[0].ends_with('…'), "long title is ellipsised: {}", t[0]);
        assert!(t.contains(&"1:05".to_string()) && t.contains(&"-2:15".to_string()), "{t:?}");
        let icons: Vec<Icon> =
            v.iter().filter_map(|e| if let Element::Icon { icon, .. } = e { Some(*icon) } else { None }).collect();
        assert_eq!(icons, [Icon::Previous, Icon::Pause, Icon::Next]);
        // Disabled "previous" is dimmed.
        let prev_opacity = v.iter().find_map(|e| match e {
            Element::Icon { icon: Icon::Previous, opacity, .. } => Some(*opacity),
            _ => None,
        });
        assert!(prev_opacity.unwrap() < 0.5);
    }

    #[test]
    fn controls_sit_inside_the_card_and_scrub_maps_x() {
        let pill = Rect::new(24.0, 0.0, 372.0, 160.0);
        for (r, _) in media_controls(pill) {
            assert!(r.x >= pill.x && r.right() <= pill.right() && r.bottom() <= pill.bottom() + 1.0, "{r:?}");
        }
        assert_eq!(scrub_fraction(pill, pill.x), 0.0);
        assert_eq!(scrub_fraction(pill, pill.right()), 1.0);
        assert!((scrub_fraction(pill, pill.x + pill.w / 2.0) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn battery_compact_and_list() {
        let mut engine = TextEngine::new();
        let d = BatteryDevice {
            id: "a".into(),
            name: "Earbuds".into(),
            kind: DeviceKind::Earbuds,
            percent: 18,
            charging: false,
            parts: vec![(Part::Left, 18), (Part::Right, 40)],
        };
        let live = Live::Battery { device: d.clone(), reason: PeekReason::Connected };
        let pill = Rect::new(60.0, 0.0, COMPACT_BATTERY_W, 37.0);
        let v = compact(pill, &live, 0.0, 1.0, &mut engine);
        assert_eq!(texts(&v), ["Earbuds", "18%"]);
        assert!(v.iter().any(|e| matches!(e, Element::Ring { color, .. } if *color == AMBER)), "low level is amber");

        let list = expanded_battery(Rect::new(24.0, 0.0, 372.0, 120.0), &[&d], None, 1.0, &mut engine);
        assert!(texts(&list).contains(&"L 18%  R 40%".to_string()));
        assert_eq!(battery_list_height(1), 84.0);
        assert!(battery_list_height(5) > 160.0);
    }

    #[test]
    fn navigation_card_shows_eta_traffic_and_hand_off_buttons() {
        use islet_core::nav::{RouteInfo, TravelMode, Trip};
        let mut engine = TextEngine::new();
        let n = NavInfo {
            trip: Trip {
                label: "Work".into(),
                destination: "Kampala Road".into(),
                origin: "Ntinda".into(),
                mode: TravelMode::Drive,
            },
            route: Some(RouteInfo {
                duration: 1530.0,
                free_flow: 1200.0,
                distance_m: 12345.0,
                first_step: Some("Turn left".into()),
                maneuver: None,
            }),
            error: None,
        };
        let now = LocalTime { year: 2026, month: 11, day: 4, weekday: 3, hour: 8, minute: 30, second: 0 };
        let pill = Rect::new(24.0, 0.0, 372.0, 160.0);
        let t = texts(&expanded_nav(pill, &n, Some(&now), 1.0, &mut engine));
        for want in
            ["Work", "26 min", "Arrive 08:56 · 12.3 km", "+6 min traffic", "Start: Turn left", "Google Maps", "Waze"]
        {
            assert!(t.contains(&want.to_string()), "missing {want:?} in {t:?}");
        }
        for (r, _) in nav_controls(pill) {
            assert!(r.right() <= pill.right() && r.bottom() <= pill.bottom(), "{r:?}");
        }
        let live = Live::Navigation(n);
        assert_eq!(texts(&compact(Rect::new(92.0, 0.0, 236.0, 37.0), &live, 0.0, 1.0, &mut engine)), ["26 min"]);
    }

    #[test]
    fn level_colours() {
        assert_eq!(level_color(80, false), WHITE);
        assert_eq!(level_color(20, false), AMBER);
        assert_eq!(level_color(9, false), RED);
        assert_eq!(level_color(9, true), GREEN);
    }

    #[test]
    fn waveform_moves_only_while_playing() {
        let heights = |t: f64, playing: bool| -> Vec<f32> {
            waveform(0.0, 10.0, 4, t, playing, WHITE, 1.0)
                .iter()
                .map(|e| match e {
                    Element::Bar { h, .. } => *h,
                    _ => 0.0,
                })
                .collect()
        };
        assert_ne!(heights(0.0, true), heights(0.1, true));
        assert_eq!(heights(0.0, false), heights(5.0, false));
        assert!(heights(0.3, true).iter().all(|h| (3.0..=15.5).contains(h)));
    }
}
