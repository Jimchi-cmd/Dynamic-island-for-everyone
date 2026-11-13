//! What is drawn inside the island for each activity and presentation
//! (docs/design-spec.md §3). Pure layout: rects in, display list out.

use islet_core::activity::{
    BatteryDevice, Call, Custom, DeviceKind, Hud, Live, MediaInfo, NavInfo, Note, Part, PeekReason,
};
use islet_core::alarm::{self, Alarm};
use islet_core::clock::{format_date, format_time, LocalTime};
use islet_core::connectivity::{self, BtState, Connectivity, WifiState};
use islet_core::geometry::Rect;
use islet_core::lyrics::{self, Lyrics};
use islet_core::nav;
use islet_core::stopwatch::{self, Stopwatch};
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
/// The accent colour (Settings → Look), packed 0xRRGGBB.
static ACCENT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x0A84FF);

pub fn set_accent(rgb: [u8; 3]) {
    let packed = u32::from_be_bytes([0, rgb[0], rgb[1], rgb[2]]);
    ACCENT.store(packed, std::sync::atomic::Ordering::Relaxed);
}

/// Buttons and switches that are on.
pub fn accent() -> [u8; 3] {
    let [_, r, g, b] = ACCENT.load(std::sync::atomic::Ordering::Relaxed).to_be_bytes();
    [r, g, b]
}

/// The accent for text on black: a fifth of the way to white, for contrast.
pub fn accent_text() -> [u8; 3] {
    accent().map(|c| c + (255 - c) / 5)
}

/// The Wi-Fi and Bluetooth lists that open from the clock card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Wifi,
    Bluetooth,
    Shelf,
    /// CPU, memory and disk (sampled only while open).
    System,
    /// Recently copied texts (when clipboard history is on).
    Clipboard,
    /// The stopwatch before it starts (Islet Pro).
    Stopwatch,
    /// Quick notes and to-dos (Islet Pro).
    Todo,
    /// A month's days, with the calendar link's events.
    Calendar,
    /// Volume and brightness sliders, and the dark-mode switch.
    Controls,
    /// The user's own buttons (Islet Pro).
    Shortcuts,
}

/// What a tap on the clock card or a list lands on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnHit {
    /// The Wi-Fi / Bluetooth on/off switch.
    Power,
    /// A button on the clock card.
    Open(Panel),
    /// The list header: back to the clock.
    Back,
    Row(usize),
    /// The footer button ("Enter the password in Settings").
    Footer,
    /// The × on a shelf row.
    Remove(usize),
    /// "‹ Control Centre" at the top of an activity's card.
    Home,
    /// Back from Control Centre to the activity it was opened from.
    Return,
    /// A row of the right-click menu.
    Menu(usize),
    /// Lyrics timing: earlier (−1) or later (+1).
    LyricsShift(i8),
    /// The clock card's second row: quick timer, calendar, screenshot, lock.
    Quick(Quick),
    /// The calendar card: the month before (−1) or after (+1).
    Month(i8),
    /// The Controls panel's dark / light switch.
    DarkMode,
}

/// The clock card's quick actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quick {
    /// The quick timer: fewer (−1) or more (+1) minutes.
    Step(i8),
    /// Start the quick timer.
    Start,
    Calendar,
    Screenshot,
    Lock,
}

/// The quick timer's lengths, in minutes.
pub const QUICK_MINUTES: [u32; 12] = [1, 2, 3, 5, 10, 15, 20, 25, 30, 45, 60, 90];
/// Where the quick timer starts (5 minutes).
pub const QUICK_DEFAULT: usize = 3;
/// Height the quick row adds to the clock card.
pub const QUICK_H: f64 = 42.0;

/// The clock card's second row: − 5 min + and Start on the left; calendar,
/// screenshot and lock on the right.
/// The buttons the user switched off in Settings are left out; the round
/// ones keep to the right.
pub fn quick_row(pill: Rect, row: ClockRow) -> Vec<(Rect, Quick)> {
    let (x, w) = (pill.x + PAD, pill.w - 2.0 * PAD);
    let y = pill.y + 164.0 + if row.world { WORLD_H } else { 0.0 };
    let (h, right) = (32.0, x + w);
    let mut v = Vec::new();
    if !row.no_timer {
        v.push((Rect::new(x, y, 30.0, h), Quick::Step(-1)));
        v.push((Rect::new(x + 88.0, y, 30.0, h), Quick::Step(1)));
        v.push((Rect::new(x + 124.0, y, 76.0, h), Quick::Start));
    }
    let round: Vec<Quick> =
        [(row.no_calendar, Quick::Calendar), (row.no_screenshot, Quick::Screenshot), (row.no_lock, Quick::Lock)]
            .into_iter()
            .filter(|(hidden, _)| !hidden)
            .map(|(_, q)| q)
            .collect();
    for (i, q) in round.iter().enumerate() {
        let from_right = (round.len() - i) as f64;
        v.push((Rect::new(right - from_right * 32.0 - (from_right - 1.0) * 10.0, y, 32.0, h), *q));
    }
    v
}

/// The height the quick row takes: none when every button in it is off.
pub fn quick_h(row: ClockRow) -> f64 {
    if row.no_timer && row.no_calendar && row.no_screenshot && row.no_lock {
        0.0
    } else {
        QUICK_H
    }
}

pub fn quick_row_elements(pill: Rect, row: ClockRow, minutes: u32, o: f32) -> Vec<Element> {
    let mut v = Vec::new();
    let buttons = quick_row(pill, row);
    // The stepper: one capsule holding −, the length and +.
    let find = |q: Quick| buttons.iter().find(|(_, b)| *b == q).map(|(r, _)| *r);
    if let (Some(minus), Some(plus)) = (find(Quick::Step(-1)), find(Quick::Step(1))) {
        v.extend(quick_stepper(minus, plus, minutes, o));
    }
    v.extend(quick_buttons(&buttons, o));
    v
}

/// − 5 min + in one capsule.
fn quick_stepper(minus: Rect, plus: Rect, minutes: u32, o: f32) -> Vec<Element> {
    let mut v = Vec::new();
    let (sx, sy, sw, sh) = (minus.x as f32, minus.y as f32, (plus.right() - minus.x) as f32, minus.h as f32);
    v.push(Element::Bar { x: sx, y: sy, w: sw, h: sh, color: WHITE, opacity: o * 0.14 });
    v.push(Element::Bar { x: sx + 10.0, y: sy + sh / 2.0 - 1.0, w: 10.0, h: 2.0, color: WHITE, opacity: o });
    v.push(Element::Icon {
        icon: Icon::Plus,
        x: plus.x as f32 + 8.0,
        y: sy + 9.0,
        size: 14.0,
        color: WHITE,
        opacity: o,
    });
    let label =
        TextStyle { size: 13.0, weight: 600, opacity: o, align: Align::Center, tabular: true, ..TextStyle::default() };
    v.push(text(format!("{minutes} min"), label, sx + sw / 2.0, sy + sh / 2.0 + 4.5));
    v
}

/// Start, and the round calendar, screenshot and lock buttons.
fn quick_buttons(buttons: &[(Rect, Quick)], o: f32) -> Vec<Element> {
    let mut v = Vec::new();
    let label =
        TextStyle { size: 13.0, weight: 600, opacity: o, align: Align::Center, tabular: true, ..TextStyle::default() };
    for (r, q) in buttons.iter().copied() {
        let (x, y, w, h) = (r.x as f32, r.y as f32, r.w as f32, r.h as f32);
        let icon = match q {
            Quick::Step(_) => continue,
            Quick::Start => {
                v.push(Element::Bar { x, y, w, h, color: ORANGE, opacity: o });
                v.push(Element::Icon {
                    icon: Icon::Play,
                    x: x + 9.0,
                    y: y + 9.0,
                    size: 14.0,
                    color: [0, 0, 0],
                    opacity: o,
                });
                let style = TextStyle { color: [0, 0, 0], align: Align::Left, ..label };
                v.push(text("Timer", style, x + 27.0, y + h / 2.0 + 4.5));
                continue;
            }
            Quick::Calendar => Icon::Calendar,
            Quick::Screenshot => Icon::Screenshot,
            Quick::Lock => Icon::Lock,
        };
        v.push(Element::Bar { x, y, w, h, color: WHITE, opacity: o * 0.14 });
        v.push(Element::Icon {
            icon,
            x: x + w / 2.0 - 9.0,
            y: y + h / 2.0 - 9.0,
            size: 18.0,
            color: WHITE,
            opacity: o,
        });
    }
    v
}

/// Height of the calendar card.
pub const CALENDAR_H: f64 = LIST_TOP + 16.0 + 6.0 * CAL_ROW + 12.0;
const CAL_ROW: f64 = 23.0;

/// The calendar card's ‹ and › buttons.
pub fn calendar_arrows(pill: Rect) -> [(Rect, i8); 2] {
    let y = pill.y + 15.0;
    [
        (Rect::new(pill.right() - PAD - 62.0, y, 28.0, 28.0), -1),
        (Rect::new(pill.right() - PAD - 28.0, y, 28.0, 28.0), 1),
    ]
}

/// A month: its days in a Monday-first grid, today ringed, days with events
/// dotted; the next events beside it.
#[allow(clippy::too_many_arguments)]
/// A row beside the month grid: when, what, and whether it is a deadline
/// (then `when` counts down: "in 3 days").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upcoming {
    pub when: String,
    pub name: String,
    pub deadline: bool,
}

pub fn expanded_calendar(
    pill: Rect,
    (year, month): (i32, u8),
    today: &LocalTime,
    event_days: &[u8],
    upcoming: &[Upcoming],
    o: f32,
    engine: &mut TextEngine,
) -> Vec<Element> {
    use islet_core::calendar::{days_in_month, weekday_of};
    let title = format!("{} {year}", islet_core::clock::month_name(month));
    let mut v = list_header_elements(pill, Icon::Calendar, &title, None, o, engine);
    for (r, dir) in calendar_arrows(pill) {
        let (x, y, s) = (r.x as f32, r.y as f32, r.w as f32);
        v.push(Element::Bar { x, y, w: s, h: s, color: WHITE, opacity: o * 0.16 });
        let icon = if dir < 0 { Icon::Back } else { Icon::Forward };
        v.push(Element::Icon { icon, x: x + 6.0, y: y + 6.0, size: 16.0, color: WHITE, opacity: o });
    }
    let (left, top) = (pill.x as f32 + PAD as f32, (pill.y + LIST_TOP) as f32);
    let col = 28.0f32;
    let head = TextStyle { size: 10.5, weight: 600, opacity: o * 0.45, align: Align::Center, ..TextStyle::default() };
    for (i, d) in ["M", "T", "W", "T", "F", "S", "S"].iter().enumerate() {
        v.push(text(*d, head, left + col * (i as f32 + 0.5), top + 9.0));
    }
    // Monday-first column of the 1st.
    let first = (weekday_of(year, month, 1) + 6) % 7;
    let is_now = today.year == year && today.month == month;
    let day =
        TextStyle { size: 12.5, weight: 500, opacity: o, align: Align::Center, tabular: true, ..TextStyle::default() };
    for d in 1..=days_in_month(year, month) {
        let cell = usize::from(first) + usize::from(d) - 1;
        let (cx, cy) =
            (left + col * ((cell % 7) as f32 + 0.5), top + 16.0 + CAL_ROW as f32 * ((cell / 7) as f32 + 0.5));
        let today_cell = is_now && today.day == d;
        if today_cell {
            v.push(Element::Ring { cx, cy, r: 5.0, width: 10.0, fraction: 1.0, color: accent(), opacity: o });
        }
        let weekend = cell % 7 >= 5;
        let style = TextStyle {
            weight: if today_cell { 700 } else { 500 },
            opacity: if weekend && !today_cell { o * 0.55 } else { o },
            ..day
        };
        v.push(text(d.to_string(), style, cx, cy + 4.5));
        if event_days.contains(&d) && !today_cell {
            v.push(Element::Ring { cx, cy: cy + 9.5, r: 0.8, width: 1.6, fraction: 1.0, color: RED, opacity: o });
        }
    }
    // What's coming up, beside the grid.
    let (ex, ew) = (left + col * 7.0 + 14.0, pill.right() as f32 - PAD as f32 - (left + col * 7.0 + 14.0));
    let when = TextStyle { size: 11.0, weight: 600, opacity: o * 0.55, ..TextStyle::default() };
    let what = TextStyle { size: 12.5, weight: 600, opacity: o, ..TextStyle::default() };
    if upcoming.is_empty() {
        v.push(text("No events", when, ex, top + 26.0));
    }
    for (i, u) in upcoming.iter().take(4).enumerate() {
        let y = top + 12.0 + i as f32 * 36.0;
        // A deadline is orange and counts down; an event is red with its time.
        let (bar, when) =
            if u.deadline { (ORANGE, TextStyle { color: ORANGE, opacity: o, ..when }) } else { (RED, when) };
        v.push(Element::Bar { x: ex, y, w: 2.5, h: 28.0, color: bar, opacity: o });
        v.push(text(engine.ellipsize(&u.when, &when, ew - 10.0), when, ex + 9.0, y + 11.0));
        v.push(text(engine.ellipsize(&u.name, &what, ew - 10.0), what, ex + 9.0, y + 25.0));
    }
    v
}

pub const ROW_H: f64 = 36.0;
pub const MAX_ROWS: usize = 5;

/// Rows a list shows at once: one fewer when a footer line or button sits
/// under it, so the card always fits the window. Longer lists scroll.
pub fn max_rows(footer: bool) -> usize {
    if footer {
        MAX_ROWS - 1
    } else {
        MAX_ROWS
    }
}

/// How many rows are on screen for a list of `total` scrolled by `offset`.
pub fn shown_rows(total: usize, offset: usize, footer: bool) -> usize {
    total.saturating_sub(offset).min(max_rows(footer))
}

/// The furthest a list of `total` can scroll.
pub fn max_scroll(total: usize, footer: bool) -> usize {
    total.saturating_sub(max_rows(footer))
}

/// A thin scroll bar at the right edge while a list has more rows than fit.
fn scroll_bar(pill: Rect, total: usize, offset: usize, footer: bool, o: f32) -> Option<Element> {
    let shown = max_rows(footer);
    if total <= shown {
        return None;
    }
    let track = shown as f32 * ROW_H as f32 - 8.0;
    let h = (track * shown as f32 / total as f32).max(18.0);
    let y = (pill.y + LIST_TOP) as f32 + 4.0 + (track - h) * offset as f32 / (total - shown) as f32;
    Some(Element::Bar { x: (pill.right() - 11.0) as f32, y, w: 3.0, h, color: WHITE, opacity: o * 0.3 })
}
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
    /// Starting soon (or on now) with a video-call link: a Join button.
    pub join: bool,
}

pub const AGENDA_ROWS: usize = 4;

/// The Join button at the right of agenda row `i`.
pub fn agenda_join_button(pill: Rect, i: usize) -> Rect {
    Rect::new(pill.right() - PAD - 58.0, pill.y + 48.0 + i as f64 * 40.0 + 4.0, 58.0, 28.0)
}

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
        // The text stops short of the Join button.
        let room = w - 2.0 * pad - 78.0 - if r.join { 66.0 } else { 0.0 };
        let name = engine.ellipsize(&r.title, &name_style, room);
        v.push(text(name, name_style, x + pad + 78.0, ry + 18.0));
        if let Some(loc) = &r.location {
            v.push(text(engine.ellipsize(loc, &small, room), small, x + pad + 78.0, ry + 34.0));
        }
        if r.join {
            let b = agenda_join_button(pill, i);
            let (bx, by, bw, bh) = (b.x as f32, b.y as f32, b.w as f32, b.h as f32);
            let label = TextStyle { size: 13.0, weight: 700, opacity: o, ..TextStyle::default() };
            let tw = engine.measure("Join", &label);
            v.push(Element::Bar { x: bx, y: by, w: bw, h: bh, color: GREEN, opacity: o });
            v.push(text("Join", label, bx + (bw - tw) / 2.0, by + bh / 2.0 + 4.5));
        }
    }
    if rows.is_empty() {
        let style = TextStyle { size: 14.0, opacity: o * 0.6, ..TextStyle::default() };
        v.push(text("Nothing else today", style, x + pad, y + 70.0));
    }
    v
}

pub fn custom_color(c: &Custom) -> [u8; 3] {
    c.color.unwrap_or(accent_text())
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

/// The performance alert is a script-style activity with this id, and a
/// card of its own: two buttons where the progress bar would be.
pub const HOG_ID: &str = "islet.hog";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HogButton {
    Close,
    Ignore,
}

pub fn hog_buttons(pill: Rect) -> [(Rect, HogButton); 2] {
    let (x, y, w) = (pill.x + PAD, pill.y + 66.0, (pill.w - 2.0 * PAD - 10.0) / 2.0);
    [(Rect::new(x, y, w, 34.0), HogButton::Ignore), (Rect::new(x + w + 10.0, y, w, 34.0), HogButton::Close)]
}

/// The alert's card: which program, what it is using, and what to do.
pub fn expanded_hog(pill: Rect, c: &Custom, o: f32, engine: &mut TextEngine) -> Vec<Element> {
    let (x, y, w) = (pill.x as f32, pill.y as f32, pill.w as f32);
    let pad = PAD as f32;
    let title = TextStyle { size: 16.0, weight: 600, opacity: o, ..TextStyle::default() };
    let small = TextStyle { size: 13.0, opacity: o * 0.6, ..TextStyle::default() };
    let room = w - 2.0 * pad - 38.0;
    let mut v = vec![
        Element::Icon { icon: Icon::Gauge, x: x + pad, y: y + 22.0, size: 26.0, color: ORANGE, opacity: o },
        text(engine.ellipsize(&c.title, &title, room), title, x + pad + 38.0, y + 34.0),
        text(engine.ellipsize(&c.subtitle, &small, room), small, x + pad + 38.0, y + 53.0),
    ];
    let label = TextStyle { size: 14.0, weight: 600, opacity: o, align: Align::Center, ..TextStyle::default() };
    for (r, b) in hog_buttons(pill) {
        let (bx, by, bw, bh) = (r.x as f32, r.y as f32, r.w as f32, r.h as f32);
        let (fill, alpha, word, color) = match b {
            HogButton::Ignore => (WHITE, 0.16, "Ignore", WHITE),
            HogButton::Close => (ORANGE, 1.0, "Close it", [0, 0, 0]),
        };
        v.push(Element::Bar { x: bx, y: by, w: bw, h: bh, color: fill, opacity: o * alpha });
        v.push(text(word, TextStyle { color, ..label }, bx + bw / 2.0, by + 22.0));
    }
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
    /// The output volume slider.
    Volume,
}

pub const COMPACT_MEDIA_W: f64 = 236.0;
pub const COMPACT_BATTERY_W: f64 = 300.0;

pub fn compact_width(live: &Live) -> f64 {
    match live {
        Live::Media(_)
        | Live::Navigation(_)
        | Live::Alarm(_)
        | Live::Timer(_)
        | Live::Stopwatch(_)
        | Live::Hud(_)
        | Live::Event { .. }
        | Live::Call(_) => COMPACT_MEDIA_W,
        Live::Note(_) => COMPACT_BATTERY_W,
        Live::Custom(_) => COMPACT_BATTERY_W,
        Live::Battery { .. } => COMPACT_BATTERY_W,
    }
}

/// What the charging notification says: "Charging", "Fully charged",
/// "On battery", or "Buds charging" for a device.
pub fn charging_label(d: &BatteryDevice, reason: PeekReason) -> String {
    let laptop = d.kind == DeviceKind::Laptop;
    match (reason, d.charging, laptop) {
        (PeekReason::Full, _, _) => "Fully charged".into(),
        (_, true, true) => "Charging".into(),
        (_, false, true) => "On battery".into(),
        (_, true, false) => format!("{} charging", d.name),
        (_, false, false) => d.name.clone(),
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
    Element::Text(TextItem::new(s, style, x, baseline))
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

thread_local! {
    /// Power saver: the waveform holds one shape instead of moving. (Per
    /// thread: the island draws on one thread only.)
    static STILL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub fn set_still(on: bool) {
    STILL.with(|s| s.set(on));
}

/// Animated equaliser bars, centred on `cy`. Static stubs while paused.
pub fn waveform(x: f32, cy: f32, n: usize, t: f64, playing: bool, color: [u8; 3], o: f32) -> Vec<Element> {
    // Held still, the bars keep a shape that still reads as "playing".
    let t = if STILL.with(std::cell::Cell::get) { 0.37 } else { t };
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

/// The progress bar as a row of moving bars (Islet Pro). The motion is
/// drawn, not measured: Islet doesn't listen to what is playing.
#[allow(clippy::too_many_arguments)]
pub fn visualizer_track(
    x: f32,
    cy: f32,
    w: f32,
    played: f32,
    t: f64,
    playing: bool,
    color: [u8; 3],
    o: f32,
) -> Vec<Element> {
    const BARS: usize = 44;
    let still = STILL.with(std::cell::Cell::get);
    let t = if still { 0.37 } else { t };
    let step = w / BARS as f32;
    (0..BARS)
        .map(|i| {
            let f = i as f64;
            // Three waves of different speeds, so neighbours move together
            // but the row never repeats itself.
            let h = if playing {
                let a = (t * 2.1 + f * 0.55).sin();
                let b = (t * 3.7 - f * 0.31).sin();
                let c = (t * 1.3 + f * 1.7).sin();
                (3.0 + 6.5 * (1.0 + 0.5 * a + 0.3 * b + 0.2 * c)) as f32
            } else {
                3.0
            };
            let done = (i as f32 + 0.5) / BARS as f32 <= played;
            Element::Bar {
                x: x + i as f32 * step,
                y: cy - h / 2.0,
                w: (step - 2.0).max(1.5),
                h,
                color: if done { color } else { WHITE },
                opacity: o * if done { 1.0 } else { 0.25 },
            }
        })
        .collect()
}

/// Compact: one activity beside the pill.
pub fn compact(pill: Rect, live: &Live, t: f64, o: f32, engine: &mut TextEngine) -> Vec<Element> {
    let (x, right) = (pill.x as f32, pill.right() as f32);
    let cy = (pill.y + pill.h / 2.0) as f32;
    match live {
        Live::Timer(tm) => vec![timer_ring(tm, x + 20.0, cy, 8.5, t, o), countdown(tm, right - 14.0, cy, 15.0, t, o)],
        Live::Stopwatch(sw) => {
            vec![stopwatch_icon(sw, x + 10.0, cy, o), stopwatch_time(sw, right - 14.0, cy, 15.0, false, t, o)]
        }
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
        Live::Call(c) => {
            let muted = c.muted == Some(true);
            let color = if muted { RED } else { ORANGE };
            let style = TextStyle { size: 13.5, weight: 600, opacity: o, ..TextStyle::default() };
            let time = TextStyle { size: 13.5, weight: 600, opacity: o, color, tabular: true, align: Align::Right };
            let elapsed = format_duration(t - c.since);
            let room = (right - x) - 38.0 - engine.measure(&elapsed, &time) - 22.0;
            vec![
                Element::Icon { icon: Icon::Mic(muted), x: x + 10.0, y: cy - 10.0, size: 20.0, color, opacity: o },
                text(engine.ellipsize(call_title(c), &style, room), style, x + 36.0, cy + 4.5),
                text(elapsed, time, right - 14.0, cy + 4.5),
            ]
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
        Live::Battery { device: d, reason: reason @ (PeekReason::ChargerChanged | PeekReason::Full) } => {
            // The charging notification: bolt and "Charging" on the left, the
            // level and a filling battery on the right.
            let color = level_color(d.percent, d.charging);
            let label = charging_label(d, *reason);
            let label_style = TextStyle { size: 14.0, weight: 600, opacity: o, ..TextStyle::default() };
            let pct_style =
                TextStyle { size: 14.0, weight: 600, opacity: o, tabular: true, color, align: Align::Right };
            let mut v = Vec::new();
            let left = if d.charging {
                v.push(Element::Icon {
                    icon: Icon::Bolt,
                    x: x + 10.0,
                    y: cy - 10.0,
                    size: 20.0,
                    color: GREEN,
                    opacity: o,
                });
                x + 34.0
            } else {
                x + 16.0
            };
            v.push(text(engine.ellipsize(&label, &label_style, right - left - 90.0), label_style, left, cy + 5.0));
            v.push(text(format!("{}%", d.percent), pct_style, right - 44.0, cy + 5.0));
            v.push(Element::Icon {
                icon: Icon::Battery(d.percent),
                x: right - 38.0,
                y: cy - 12.0,
                size: 24.0,
                color,
                opacity: o,
            });
            v
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
    if let Live::Stopwatch(sw) = primary {
        v.push(stopwatch_time(sw, pill.right() as f32 - 13.0, cy, 14.0, false, t, o));
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
            Live::Stopwatch(sw) => v.push(stopwatch_icon(sw, b.x - 9.0, b.y, o)),
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
            Live::Call(c) => v.push(Element::Icon {
                icon: Icon::Mic(c.muted == Some(true)),
                x: b.x - 9.0,
                y: b.y - 9.0,
                size: 18.0,
                color: if c.muted == Some(true) { RED } else { ORANGE },
                opacity: o,
            }),
        }
    }
    v
}

fn leading(live: &Live, x: f32, cy: f32, o: f32, h24: bool) -> Vec<Element> {
    match live {
        Live::Call(c) => {
            let muted = c.muted == Some(true);
            let color = if muted { RED } else { ORANGE };
            vec![Element::Icon { icon: Icon::Mic(muted), x: x + 3.0, y: cy - 10.0, size: 20.0, color, opacity: o }]
        }
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
        Live::Stopwatch(sw) => vec![stopwatch_icon(sw, x + 3.0, cy, o)],
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

/// Centre of the transport buttons: the card's middle, or left of it when
/// the volume slider shares the row.
fn transport_x(pill: Rect, volume: bool) -> f64 {
    if volume {
        pill.x + 112.0
    } else {
        pill.x + pill.w / 2.0
    }
}

/// The volume slider's track (left of it, a speaker icon).
fn volume_track(pill: Rect) -> (f64, f64) {
    (pill.x + 236.0, pill.right() - PAD - 6.0)
}

/// Hit areas of the expanded media card, relative to the pill.
pub fn media_controls(pill: Rect, volume: bool) -> Vec<(Rect, Control)> {
    let cx = transport_x(pill, volume);
    let cy = pill.y + 136.0;
    let button = |dx: f64| Rect::new(cx + dx - 22.0, cy - 22.0, 44.0, 44.0);
    let mut v = vec![
        (button(-64.0), Control::Previous),
        (button(0.0), Control::PlayPause),
        (button(64.0), Control::Next),
        (Rect::new(pill.x + PAD, pill.y + 82.0, pill.w - 2.0 * PAD, 22.0), Control::Scrub),
    ];
    if volume {
        let (left, right) = volume_track(pill);
        v.push((Rect::new(left - 10.0, cy - 16.0, right - left + 20.0, 32.0), Control::Volume));
    }
    v
}

/// The volume (0..=1) for a pointer x on the slider.
pub fn volume_fraction(pill: Rect, x: f64) -> f32 {
    let (left, right) = volume_track(pill);
    ((x - left) / (right - left)).clamp(0.0, 1.0) as f32
}

/// Fraction along the progress bar for a pointer x.
pub fn scrub_fraction(pill: Rect, x: f64) -> f64 {
    ((x - pill.x - PAD) / (pill.w - 2.0 * PAD)).clamp(0.0, 1.0)
}

/// The expanded "Now Playing" card.
#[allow(clippy::too_many_arguments)]
pub fn expanded_media(
    pill: Rect,
    m: &MediaInfo,
    t: f64,
    o: f32,
    scrub: Option<f64>,
    volume: Option<(f32, bool)>,
    visualizer: bool,
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
        if visualizer && scrub.is_none() {
            // Islet Pro: the bar itself moves with the music. Bars up to
            // the playhead take the album's colour; the rest wait in grey.
            v.pop();
            v.extend(visualizer_track(x + pad, by + 2.0, track_w, frac as f32, t, m.playing, tint, o));
        } else {
            v.push(Element::Bar {
                x: x + pad,
                y: by - bh / 2.0 + 2.0,
                w: (track_w * frac as f32).max(bh),
                h: bh,
                color: tint,
                opacity: o,
            });
        }
        let time_style = TextStyle { size: 11.0, opacity: o * 0.6, tabular: true, ..TextStyle::default() };
        v.push(text(format_duration(shown), time_style, x + pad, y + 112.0));
        let remaining = TextStyle { align: Align::Right, ..time_style };
        v.push(text(format!("-{}", format_duration(dur - shown)), remaining, x + w - pad, y + 112.0));
    }

    // Transport controls, with the volume slider beside them when known.
    let cx = transport_x(pill, volume.is_some()) as f32;
    let cy = y + 136.0;
    if let Some((level, muted)) = volume {
        let (left, right) = volume_track(pill);
        let (left, right) = (left as f32, right as f32);
        let waves = if muted || level <= 0.0 { 255 } else { (level * 3.0).ceil().clamp(1.0, 3.0) as u8 };
        v.push(Element::Icon {
            icon: Icon::Volume(waves),
            x: left - 28.0,
            y: cy - 9.0,
            size: 18.0,
            color: WHITE,
            opacity: o * 0.8,
        });
        let shown = if muted { 0.0 } else { level.clamp(0.0, 1.0) };
        v.push(Element::Bar { x: left, y: cy - 2.0, w: right - left, h: 4.0, color: WHITE, opacity: o * 0.22 });
        v.push(Element::Bar {
            x: left,
            y: cy - 2.0,
            w: ((right - left) * shown).max(4.0),
            h: 4.0,
            color: WHITE,
            opacity: o * 0.9,
        });
        let kx = left + (right - left) * shown;
        v.push(Element::Bar { x: kx - 7.0, y: cy - 7.0, w: 14.0, h: 14.0, color: WHITE, opacity: o });
    }
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
pub fn expanded_timer(
    pill: Rect,
    tm: &Timer,
    streak: Option<&str>,
    t: f64,
    o: f32,
    engine: &mut TextEngine,
) -> Vec<Element> {
    let (x, y, w) = (pill.x as f32, pill.y as f32, pill.w as f32);
    let pad = PAD as f32;
    let mut v = vec![timer_ring(tm, x + pad + 13.0, y + 38.0, 12.0, t, o)];
    let title_style = TextStyle { size: 16.0, weight: 600, opacity: o, ..TextStyle::default() };
    v.push(text(engine.ellipsize(&tm.title(), &title_style, w * 0.45), title_style, x + pad + 38.0, y + 34.0));
    let small = TextStyle { size: 13.0, opacity: o * 0.6, ..TextStyle::default() };
    let state = if tm.running() { "Running" } else { "Paused" };
    // A focus session shows its streak ("3 days in a row") where it fits
    // beside the countdown.
    let sub = match streak.filter(|_| tm.focus) {
        Some(s) => engine.ellipsize(&format!("{state} · {s}"), &small, w * 0.5),
        None => state.to_string(),
    };
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

/// The stopwatch's colour.
pub const STOPWATCH: [u8; 3] = [100, 210, 255];

/// The stopwatch icon, 18 px, its left edge at `x`, centred on `cy`.
fn stopwatch_icon(sw: &Stopwatch, x: f32, cy: f32, o: f32) -> Element {
    let opacity = o * if sw.running() { 1.0 } else { 0.6 };
    Element::Icon { icon: Icon::Stopwatch, x, y: cy - 9.0, size: 18.0, color: STOPWATCH, opacity }
}

#[allow(clippy::too_many_arguments)]
fn stopwatch_time(sw: &Stopwatch, right: f32, cy: f32, size: f32, tenths: bool, t: f64, o: f32) -> Element {
    let style = TextStyle {
        size,
        weight: 600,
        opacity: o * if sw.running() { 1.0 } else { 0.6 },
        tabular: true,
        color: STOPWATCH,
        align: Align::Right,
    };
    text(stopwatch::format(sw.elapsed(t), tenths), style, right, cy + size * 0.36)
}

/// Buttons of the stopwatch card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopwatchControl {
    /// Lap while running; Reset while paused.
    LapReset,
    /// Start, Pause or Resume.
    StartPause,
}

pub const STOPWATCH_H: f64 = 132.0;

pub fn stopwatch_controls(pill: Rect) -> [(Rect, StopwatchControl); 2] {
    let [(a, _), (b, _)] = alarm_controls(pill);
    [(a, StopwatchControl::LapReset), (b, StopwatchControl::StartPause)]
}

/// The stopwatch card: the time to a tenth, the current lap and the last
/// two, Lap/Reset and Start/Pause. `sw` is None before it is started.
pub fn expanded_stopwatch(pill: Rect, sw: Option<&Stopwatch>, t: f64, o: f32, engine: &mut TextEngine) -> Vec<Element> {
    let (x, y, w) = (pill.x as f32, pill.y as f32, pill.w as f32);
    let pad = PAD as f32;
    let running = sw.is_some_and(Stopwatch::running);
    let mut v = vec![Element::Icon {
        icon: Icon::Stopwatch,
        x: x + pad,
        y: y + 25.0,
        size: 26.0,
        color: STOPWATCH,
        opacity: o,
    }];
    let title_style = TextStyle { size: 16.0, weight: 600, opacity: o, ..TextStyle::default() };
    v.push(text("Stopwatch", title_style, x + pad + 38.0, y + 34.0));
    let small = TextStyle { size: 13.0, opacity: o * 0.6, tabular: true, ..TextStyle::default() };
    let sub = match sw {
        None => "Ready".to_string(),
        Some(sw) if sw.lap_count() == 0 => if running { "Running" } else { "Paused" }.to_string(),
        Some(sw) => {
            let mut parts = vec![format!("Lap {} {}", sw.lap_count() + 1, stopwatch::format(sw.current_lap(t), true))];
            parts.extend(sw.laps().take(1).map(|(n, d)| format!("Lap {n} {}", stopwatch::format(d, true))));
            parts.join("  ·  ")
        }
    };
    v.push(text(engine.ellipsize(&sub, &small, w * 0.5), small, x + pad + 38.0, y + 53.0));
    let big = TextStyle {
        size: 34.0,
        weight: 600,
        opacity: o * if running || sw.is_none() { 1.0 } else { 0.6 },
        tabular: true,
        color: STOPWATCH,
        align: Align::Right,
    };
    let elapsed = sw.map_or(0.0, |s| s.elapsed(t));
    v.push(text(stopwatch::format(elapsed, true), big, x + w - pad, y + 38.0 + 34.0 * 0.36));
    let label = TextStyle { size: 14.0, weight: 600, opacity: o, align: Align::Center, ..TextStyle::default() };
    for (r, c) in stopwatch_controls(pill) {
        let (bx, by, bw, bh) = (r.x as f32, r.y as f32, r.w as f32, r.h as f32);
        let (fill, alpha, word, color) = match c {
            StopwatchControl::LapReset => {
                let word = if running { "Lap" } else { "Reset" };
                (WHITE, if sw.is_some() { 0.16 } else { 0.08 }, word, WHITE)
            }
            StopwatchControl::StartPause => {
                let word = match sw {
                    None => "Start",
                    Some(_) if running => "Pause",
                    Some(_) => "Resume",
                };
                (if running { ORANGE } else { GREEN }, 1.0, word, [0, 0, 0])
            }
        };
        v.push(Element::Bar { x: bx, y: by, w: bw, h: bh, color: fill, opacity: o * alpha });
        let text_o = if c == StopwatchControl::LapReset && sw.is_none() { 0.4 } else { 1.0 };
        v.push(text(word, TextStyle { color, opacity: o * text_o, ..label }, bx + bw / 2.0, by + 22.0));
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
/// `alarm`: the next alarm, as "7:30 · in 8 h 15 min" (the date shortens to
/// make room for it).
/// The line beside the date on the clock card: the next alarm, or where
/// the class timetable stands.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClockNote<'a> {
    pub icon: Icon,
    pub color: [u8; 3],
    pub text: &'a str,
}

pub fn expanded_clock(
    pill: Rect,
    now: &LocalTime,
    h24: bool,
    note: Option<ClockNote<'_>>,
    weather: bool,
    o: f32,
    engine: &mut TextEngine,
) -> Vec<Element> {
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
    let Some(note) = note else {
        v.push(text(format_date(now), date_style, left + 2.0, top + 102.0));
        return v;
    };
    let date = islet_core::clock::format_date_short(now);
    let x = left + 2.0 + engine.measure(&date, &date_style) + 10.0;
    v.push(text(date, date_style, left + 2.0, top + 102.0));
    v.push(Element::Icon { icon: note.icon, x, y: top + 90.0, size: 14.0, color: note.color, opacity: o });
    let note_style =
        TextStyle { size: 13.0, weight: 600, opacity: o, color: note.color, tabular: true, ..TextStyle::default() };
    // Stops short of the weather's credit line in the corner, when there is one.
    let room = (pill.right() - PAD) as f32 - if weather { 66.0 } else { 0.0 } - (x + 18.0);
    v.push(text(engine.ellipsize(note.text, &note_style, room), note_style, x + 18.0, top + 101.5));
    v
}

/// "in 8 h 15 min", "in 12 min", "in 2 days".
pub fn until_label(minutes: i64) -> String {
    match minutes {
        m if m < 60 => format!("in {m} min"),
        m if m < 24 * 60 && m % 60 == 0 => format!("in {} h", m / 60),
        m if m < 24 * 60 => format!("in {} h {} min", m / 60, m % 60),
        m if m < 48 * 60 => "tomorrow".into(),
        m => format!("in {} days", m / (24 * 60)),
    }
}

/// World clocks under the date: "London 14:05 · Tokyo 22:05 +1".
pub fn world_clocks(
    pill: Rect,
    clocks: &[(String, LocalTime, i32)],
    h24: bool,
    o: f32,
    engine: &mut TextEngine,
) -> Vec<Element> {
    let (left, top) = (pill.x as f32 + 28.0, pill.y as f32 + (1.0 - o) * 8.0);
    let name = TextStyle { size: 13.0, opacity: o * 0.6, ..TextStyle::default() };
    let time = TextStyle { size: 13.0, weight: 600, opacity: o, tabular: true, ..TextStyle::default() };
    let mut v = Vec::new();
    let mut x = left;
    let max = (pill.right() - PAD) as f32;
    for (i, (city, now, days)) in clocks.iter().enumerate() {
        let (hm, suffix) = format_time(now, h24);
        let when = match (suffix, days) {
            (Some(s), 0) => format!("{hm} {s}"),
            (Some(s), d) => format!("{hm} {s} {d:+}"),
            (None, 0) => hm,
            (None, d) => format!("{hm} {d:+}"),
        };
        let lead = if i == 0 { format!("{city} ") } else { format!("  ·  {city} ") };
        let need = engine.measure(&lead, &name) + engine.measure(&when, &time);
        if x + need > max {
            break;
        }
        v.push(text(lead.clone(), name, x, top + 126.0));
        x += engine.measure(&lead, &name);
        v.push(text(when.clone(), time, x, top + 126.0));
        x += engine.measure(&when, &time);
    }
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
/// What the clock card shows besides the time.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClockRow {
    /// The shelf has files on it.
    pub shelf: bool,
    pub clipboard: bool,
    /// Islet Pro: the stopwatch and to-do buttons.
    pub pro: bool,
    /// To-dos left (a badge on the to-do button).
    pub todo_left: usize,
    /// The user has shortcuts of their own (Islet Pro).
    pub shortcuts: bool,
    /// A line of world clocks under the date.
    pub world: bool,
    /// Buttons the user switched off in Settings.
    pub no_timer: bool,
    pub no_calendar: bool,
    pub no_screenshot: bool,
    pub no_lock: bool,
    pub no_system: bool,
    pub no_controls: bool,
    pub no_todo: bool,
    pub no_stopwatch: bool,
}

/// Height of the world-clock line.
pub const WORLD_H: f64 = 22.0;

pub fn clock_chips(pill: Rect, row: ClockRow) -> Vec<(Rect, Panel)> {
    let (x, w) = (pill.x + PAD, pill.w - 2.0 * PAD);
    let y = pill.y + 122.0 + if row.world { WORLD_H } else { 0.0 };
    let panels: &[Panel] = &[Panel::Wifi, Panel::Bluetooth];
    // Wi-Fi and Bluetooth share the row with their names; the tools are
    // round icons at the end (three labelled chips never fitted their names).
    let wanted = [
        (Panel::Shelf, row.shelf),
        (Panel::Shortcuts, row.shortcuts),
        (Panel::Todo, row.pro && !row.no_todo),
        (Panel::Stopwatch, row.pro && !row.no_stopwatch),
        (Panel::Clipboard, row.clipboard),
        (Panel::System, !row.no_system),
        (Panel::Controls, !row.no_controls),
    ];
    let mut icons: Vec<Panel> = wanted.iter().filter(|(_, on)| *on).map(|(p, _)| *p).collect();
    // The row holds Wi-Fi, Bluetooth and at most `MAX_TOOLS` tools. With
    // more switched on, the ones nearest the front of `KEEP` stay (the
    // rest can be chosen in Settings → General → Clock card buttons).
    if icons.len() > MAX_TOOLS {
        const KEEP: [Panel; 7] = [
            Panel::Shelf,
            Panel::Controls,
            Panel::System,
            Panel::Todo,
            Panel::Stopwatch,
            Panel::Shortcuts,
            Panel::Clipboard,
        ];
        let kept: Vec<Panel> = KEEP.iter().copied().filter(|p| icons.contains(p)).take(MAX_TOOLS).collect();
        icons.retain(|p| kept.contains(p));
    }
    let n = panels.len() as f64;
    let icons_w = icons.len() as f64 * (SYSTEM_CHIP + 10.0);
    let cw = (w - icons_w - 10.0 * (n - 1.0)) / n;
    let mut v: Vec<(Rect, Panel)> =
        panels.iter().enumerate().map(|(i, p)| (Rect::new(x + i as f64 * (cw + 10.0), y, cw, 32.0), *p)).collect();
    for (i, p) in icons.iter().enumerate() {
        let from_right = (icons.len() - i) as f64;
        v.push((Rect::new(x + w - from_right * SYSTEM_CHIP - (from_right - 1.0) * 10.0, y, SYSTEM_CHIP, 32.0), *p));
    }
    v
}

/// The most tool buttons the clock card's row has room for.
pub const MAX_TOOLS: usize = 5;

const SYSTEM_CHIP: f64 = 32.0;

pub fn clock_chip_elements(
    pill: Rect,
    c: &Connectivity,
    shelf: usize,
    row: ClockRow,
    o: f32,
    engine: &mut TextEngine,
) -> Vec<Element> {
    let mut v = Vec::new();
    let style = TextStyle { size: 13.0, weight: 600, opacity: o, ..TextStyle::default() };
    for (r, panel) in clock_chips(pill, row) {
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
            Panel::Shelf
            | Panel::System
            | Panel::Controls
            | Panel::Clipboard
            | Panel::Stopwatch
            | Panel::Todo
            | Panel::Calendar
            | Panel::Shortcuts => {
                let icon = match panel {
                    Panel::Shelf => Icon::Folder,
                    Panel::Controls => Icon::Controls,
                    Panel::Shortcuts => Icon::Bolt,
                    Panel::System => Icon::Gauge,
                    Panel::Clipboard => Icon::Clipboard,
                    Panel::Todo => Icon::Checklist,
                    Panel::Calendar => Icon::Calendar,
                    _ => Icon::Stopwatch,
                };
                v.push(Element::Bar { x, y, w, h, color: WHITE, opacity: o * 0.14 });
                v.push(Element::Icon {
                    icon,
                    x: x + w / 2.0 - 9.0,
                    y: y + h / 2.0 - 9.0,
                    size: 18.0,
                    color: WHITE,
                    opacity: o,
                });
                // How many files or to-dos, in a badge on the corner.
                let count = match panel {
                    Panel::Shelf => shelf,
                    Panel::Todo => row.todo_left,
                    _ => 0,
                };
                if count > 0 {
                    let n = if count > 9 { "9+".to_string() } else { count.to_string() };
                    let badge = TextStyle {
                        size: 10.0,
                        weight: 700,
                        opacity: o,
                        align: Align::Center,
                        tabular: true,
                        ..TextStyle::default()
                    };
                    let (bx, by) = (x + w - 3.0, y + 3.0);
                    v.push(Element::Ring {
                        cx: bx,
                        cy: by,
                        r: 4.0,
                        width: 8.0,
                        fraction: 1.0,
                        color: accent(),
                        opacity: o,
                    });
                    v.push(text(n, badge, bx, by + 3.6));
                }
                continue;
            }
        };
        let bg = if on { accent() } else { WHITE };
        v.push(Element::Bar { x, y, w, h, color: bg, opacity: o * if on { 0.9 } else { 0.14 } });
        // "Wi-Fi" and "Bluetooth" are never cut short: a crowded row shows
        // the icon alone. A network's or device's name may be shortened.
        let fits = engine.measure(&label, &style) <= w - 44.0;
        if w < 84.0 || (!on && !fits) {
            v.push(Element::Icon {
                icon,
                x: x + w / 2.0 - 9.0,
                y: y + h / 2.0 - 9.0,
                size: 18.0,
                color: WHITE,
                opacity: o,
            });
            continue;
        }
        v.push(Element::Icon { icon, x: x + 10.0, y: y + h / 2.0 - 9.0, size: 18.0, color: WHITE, opacity: o });
        let label = engine.ellipsize(&label, &style, w - 44.0);
        v.push(text(label, style, x + 34.0, y + h / 2.0 + 4.5));
    }
    v
}

/// Height of a Wi-Fi or Bluetooth list with `rows` entries.
pub fn list_height(rows: usize, footer: bool) -> f64 {
    LIST_TOP + rows.clamp(1, max_rows(footer)) as f64 * ROW_H + if footer { 52.0 } else { 14.0 }
}

pub fn list_rows(pill: Rect, rows: usize) -> Vec<Rect> {
    (0..rows.min(MAX_ROWS))
        .map(|i| Rect::new(pill.x + 10.0, pill.y + LIST_TOP + i as f64 * ROW_H, pill.w - 20.0, ROW_H))
        .collect()
}

/// The round back button at the top left of a list.
pub fn back_button(pill: Rect) -> Rect {
    Rect::new(pill.x + 14.0, pill.y + 15.0, 28.0, 28.0)
}

pub fn list_header(pill: Rect) -> Rect {
    Rect::new(pill.x, pill.y, pill.w * 0.6, LIST_TOP - 4.0)
}

pub fn list_footer(pill: Rect, rows: usize) -> Rect {
    // Inset so the card's round bottom corners never cut the button.
    let y = pill.y + LIST_TOP + rows.clamp(1, MAX_ROWS) as f64 * ROW_H + 6.0;
    Rect::new(pill.x + PAD + 18.0, y, pill.w - 2.0 * (PAD + 18.0), 30.0)
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
        v.push(Element::Bar { x, y, w, h, color: accent(), opacity: o * 0.9 });
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
    // The back button: a round button with a chevron, back to Control Centre.
    let b = back_button(pill);
    let (bx, by, bs) = (b.x as f32, b.y as f32, b.w as f32);
    let mut v = vec![
        Element::Bar { x: bx, y: by, w: bs, h: bs, color: WHITE, opacity: o * 0.16 },
        Element::Icon { icon: Icon::Back, x: bx + 6.0, y: by + 6.0, size: 16.0, color: WHITE, opacity: o },
        Element::Icon { icon, x: bx + bs + 10.0, y: y + 19.0, size: 20.0, color: WHITE, opacity: o },
        text(title, title_style, bx + bs + 36.0, y + 35.0),
    ];
    if let Some(s) = status {
        let style = TextStyle { size: 12.5, opacity: o * 0.6, align: Align::Right, ..TextStyle::default() };
        v.push(text(engine.ellipsize(s, &style, w * 0.45), style, x + w - PAD as f32, y + 34.0));
    }
    v
}

/// The on/off switch at the right of the Wi-Fi and Bluetooth headers.
pub fn power_switch(pill: Rect) -> Rect {
    Rect::new(pill.right() - PAD - 40.0, pill.y + 17.0, 40.0, 24.0)
}

/// Where a tap counts as on the switch: generously around it.
pub fn power_switch_hit(pill: Rect) -> Rect {
    let r = power_switch(pill);
    Rect::new(r.x - 14.0, r.y - 10.0, r.w + 26.0, r.h + 20.0)
}

/// The radio's switch (when the computer has that radio) and, left of it,
/// what is happening ("Searching…").
fn radio_header(pill: Rect, on: Option<bool>, status: Option<&str>, o: f32, engine: &mut TextEngine) -> Vec<Element> {
    let mut v = Vec::new();
    let sw = power_switch(pill);
    let mut right = (pill.right() - PAD) as f32;
    if let Some(on) = on {
        let (x, y, w, h) = (sw.x as f32, sw.y as f32, sw.w as f32, sw.h as f32);
        let (track, alpha) = if on { (GREEN, 1.0) } else { (WHITE, 0.22) };
        v.push(Element::Bar { x, y, w, h, color: track, opacity: o * alpha });
        let knob = h - 4.0;
        let kx = if on { x + w - knob - 2.0 } else { x + 2.0 };
        v.push(Element::Bar { x: kx, y: y + 2.0, w: knob, h: knob, color: WHITE, opacity: o });
        right = x - 10.0;
    }
    if let Some(s) = status {
        let style = TextStyle { size: 12.5, opacity: o * 0.6, align: Align::Right, ..TextStyle::default() };
        v.push(text(engine.ellipsize(s, &style, (pill.w * 0.4) as f32), style, right, pill.y as f32 + 34.0));
    }
    v
}

/// What the Wi-Fi list's footer says, and whether it is a Settings button.
pub fn wifi_footer(c: &Connectivity) -> Option<(String, bool)> {
    match (&c.failure, &c.wifi.note) {
        // Short enough to fit the button whatever the network is called;
        // Settings opens with that network chosen.
        (Some((_, Some(_))), _) => Some(("Enter the password in Settings".into(), true)),
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

pub fn expanded_wifi(pill: Rect, c: &Connectivity, scroll: usize, o: f32, engine: &mut TextEngine) -> Vec<Element> {
    let w: &WifiState = &c.wifi;
    let mut v = list_header_elements(pill, Icon::Wifi(3), "Wi-Fi", None, o, engine);
    v.extend(radio_header(pill, w.available.then_some(w.enabled), c.busy.as_deref(), o, engine));
    let footer = wifi_footer(c);
    let shown = shown_rows(w.networks.len(), scroll, footer.is_some());
    let rows = list_rows(pill, shown);
    v.extend(scroll_bar(pill, w.networks.len(), scroll, footer.is_some(), o));
    let name_style = TextStyle { size: 15.0, weight: 600, opacity: o, ..TextStyle::default() };
    for (r, n) in rows.iter().zip(w.networks.iter().skip(scroll)) {
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
            TextStyle { color: if n.active { accent_text() } else { WHITE }, ..name_style },
            x + 40.0,
            cy + 5.0,
        ));
        if n.active {
            v.push(Element::Icon {
                icon: Icon::Check,
                x: x + rw - 30.0,
                y: cy - 9.0,
                size: 18.0,
                color: accent_text(),
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
    if let Some((line, button)) = footer {
        v.extend(list_footer_elements(pill, shown, &line, button, o, engine));
    }
    v
}

/// The shelf: dropped files (tap to open, × to remove).
pub fn expanded_shelf(
    pill: Rect,
    items: &[islet_core::shelf::ShelfItem],
    dragging: bool,
    scroll: usize,
    o: f32,
    engine: &mut TextEngine,
) -> Vec<Element> {
    let status =
        (!items.is_empty()).then(|| format!("{} item{}", items.len(), if items.len() == 1 { "" } else { "s" }));
    let mut v = list_header_elements(pill, Icon::Folder, "Shelf", status.as_deref(), o, engine);
    let footer = shelf_footer(items.len(), dragging);
    let shown = shown_rows(items.len(), scroll, footer.is_some());
    let rows = list_rows(pill, shown);
    v.extend(scroll_bar(pill, items.len(), scroll, footer.is_some(), o));
    let name_style = TextStyle { size: 15.0, weight: 600, opacity: o, ..TextStyle::default() };
    for (r, item) in rows.iter().zip(items.iter().skip(scroll)) {
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
    if let Some((line, button)) = footer {
        v.extend(list_footer_elements(pill, shown, &line, button, o, engine));
    }
    v
}

/// The shortcuts list's footer button.
pub const SHORTCUTS_EDIT: &str = "Edit shortcuts in Settings";

/// Your own buttons: a tap opens the folder, file, script or page.
pub fn expanded_shortcuts(
    pill: Rect,
    items: &[islet_core::prefs::Shortcut],
    scroll: usize,
    o: f32,
    engine: &mut TextEngine,
) -> Vec<Element> {
    let mut v = list_header_elements(pill, Icon::Bolt, "Shortcuts", None, o, engine);
    let shown = shown_rows(items.len(), scroll, true);
    let rows = list_rows(pill, shown);
    v.extend(scroll_bar(pill, items.len(), scroll, true, o));
    let name_style = TextStyle { size: 15.0, weight: 600, opacity: o, ..TextStyle::default() };
    let small = TextStyle { size: 12.0, opacity: o * 0.5, align: Align::Right, ..TextStyle::default() };
    for (r, item) in rows.iter().zip(items.iter().skip(scroll)) {
        let (x, cy, rw) = (r.x as f32, (r.y + r.h / 2.0) as f32, r.w as f32);
        let icon = if item.is_link() { Icon::Navigation } else { Icon::Folder };
        v.push(Element::Icon { icon, x: x + 10.0, y: cy - 10.0, size: 20.0, color: accent_text(), opacity: o });
        let kind = if item.is_link() { "Web" } else { "Open" };
        let room = rw - 40.0 - 10.0 - engine.measure(kind, &small) - 12.0;
        v.push(text(engine.ellipsize(&item.name, &name_style, room), name_style, x + 40.0, cy + 5.0));
        v.push(text(kind, small, x + rw - 10.0, cy + 4.5));
    }
    if items.is_empty() {
        let style = TextStyle { size: 14.0, opacity: o * 0.6, ..TextStyle::default() };
        v.push(text("No shortcuts yet", style, pill.x as f32 + 50.0, (pill.y + LIST_TOP) as f32 + 23.0));
    }
    v.extend(list_footer_elements(pill, shown, SHORTCUTS_EDIT, true, o, engine));
    v
}

/// The to-do list's footer button.
pub const TODO_ADD: &str = "Add a note in Settings";

/// Notes and to-dos: to do first, then done (dimmed). A tap ticks an item;
/// × removes it. Typing happens in Settings (the island never takes the keyboard).
pub fn expanded_todo(
    pill: Rect,
    todo: &islet_core::todo::Todo,
    scroll: usize,
    o: f32,
    engine: &mut TextEngine,
) -> Vec<Element> {
    let items = todo.ordered();
    let left = todo.left();
    let status = (!items.is_empty()).then(|| if left == 0 { "All done".to_string() } else { format!("{left} to do") });
    let mut v = list_header_elements(pill, Icon::Checklist, "To-do", status.as_deref(), o, engine);
    let shown = shown_rows(items.len(), scroll, true);
    let rows = list_rows(pill, shown);
    v.extend(scroll_bar(pill, items.len(), scroll, true, o));
    let style = TextStyle { size: 14.5, weight: 500, opacity: o, ..TextStyle::default() };
    for (r, item) in rows.iter().zip(items.iter().skip(scroll)) {
        let (x, cy, rw) = (r.x as f32, (r.y + r.h / 2.0) as f32, r.w as f32);
        if item.done {
            v.push(Element::Ring { cx: x + 20.0, cy, r: 8.0, width: 8.0, fraction: 1.0, color: accent(), opacity: o });
            v.push(Element::Icon { icon: Icon::Check, x: x + 14.0, y: cy - 6.0, size: 12.0, color: WHITE, opacity: o });
        } else {
            v.push(Element::Ring {
                cx: x + 20.0,
                cy,
                r: 8.0,
                width: 1.6,
                fraction: 1.0,
                color: WHITE,
                opacity: o * 0.6,
            });
        }
        let line = TextStyle { opacity: if item.done { o * 0.45 } else { o }, ..style };
        v.push(text(engine.ellipsize(&item.text, &line, rw - 90.0), line, x + 40.0, cy + 5.0));
        v.push(Element::Icon {
            icon: Icon::Close,
            x: x + rw - 28.0,
            y: cy - 8.0,
            size: 16.0,
            color: WHITE,
            opacity: o * 0.5,
        });
    }
    if items.is_empty() {
        let style = TextStyle { size: 14.0, opacity: o * 0.6, ..TextStyle::default() };
        v.push(text("Nothing to do. Nice!", style, pill.x as f32 + 50.0, (pill.y + LIST_TOP) as f32 + 23.0));
    }
    v.extend(list_footer_elements(pill, shown, TODO_ADD, true, o, engine));
    v
}

/// Clipboard history: newest first; a tap copies an entry again.
pub fn expanded_clipboard(
    pill: Rect,
    history: &islet_core::clipboard::History,
    scroll: usize,
    o: f32,
    engine: &mut TextEngine,
) -> Vec<Element> {
    let n = history.len();
    let status = (n > 0).then(|| format!("{n} item{}", if n == 1 { "" } else { "s" }));
    let mut v = list_header_elements(pill, Icon::Clipboard, "Clipboard", status.as_deref(), o, engine);
    let shown = shown_rows(n, scroll, n > 0);
    let rows = list_rows(pill, shown);
    v.extend(scroll_bar(pill, n, scroll, n > 0, o));
    let style = TextStyle { size: 14.5, weight: 500, opacity: o, ..TextStyle::default() };
    for (r, item) in rows.iter().zip(history.items().skip(scroll)) {
        let (x, cy, rw) = (r.x as f32, (r.y + r.h / 2.0) as f32, r.w as f32);
        let line = islet_core::clipboard::preview(item);
        v.push(text(engine.ellipsize(&line, &style, rw - 20.0), style, x + 10.0, cy + 5.0));
    }
    if n == 0 {
        let style = TextStyle { size: 14.0, opacity: o * 0.6, ..TextStyle::default() };
        v.push(text("Copy some text to see it here", style, pill.x as f32 + 50.0, (pill.y + LIST_TOP) as f32 + 23.0));
    } else {
        v.extend(list_footer_elements(pill, shown, CLIPBOARD_CLEAR, true, o, engine));
    }
    v
}

pub const CLIPBOARD_CLEAR: &str = "Clear history";

/// Height of the system monitor panel.
pub const SYSTEM_H: f64 = LIST_TOP + 3.0 * METER_H + 14.0;
/// A line under the meters (network speed, battery health).
const SYSTEM_LINE_H: f64 = 20.0;

/// The System panel's height with `lines` extra lines under the meters.
pub fn system_height(lines: usize) -> f64 {
    SYSTEM_H + lines as f64 * SYSTEM_LINE_H
}

/// The lines under the meters: (label, value), for what is known.
pub fn system_lines(
    stats: Option<&islet_core::sysmon::Stats>,
    health: Option<islet_core::sysmon::BatteryHealth>,
) -> Vec<(&'static str, String)> {
    use islet_core::sysmon::format_rate;
    let mut v = Vec::new();
    if let Some((down, up)) = stats.and_then(|s| s.net) {
        v.push(("Network", format!("{} down · {} up", format_rate(down), format_rate(up))));
    }
    if let Some(h) = health {
        v.push(("Battery health", h.describe()));
    }
    v
}
const METER_H: f64 = 46.0;

/// CPU, memory and disk meters (dashes until the first reading).
pub fn expanded_system(
    pill: Rect,
    stats: Option<&islet_core::sysmon::Stats>,
    health: Option<islet_core::sysmon::BatteryHealth>,
    o: f32,
    engine: &mut TextEngine,
) -> Vec<Element> {
    use islet_core::sysmon::format_bytes_of;
    let mut v = list_header_elements(pill, Icon::Gauge, "System", None, o, engine);
    let label = TextStyle { size: 14.0, weight: 600, opacity: o, ..TextStyle::default() };
    let value = TextStyle { size: 14.0, opacity: o * 0.7, ..TextStyle::default() };
    let rows: [(&str, Option<(f32, String)>); 3] = match stats {
        Some(s) => [
            ("CPU", Some((s.cpu, format!("{}%", (s.cpu * 100.0).round() as i32)))),
            ("Memory", (s.mem_total > 0).then(|| (s.mem_fraction(), format_bytes_of(s.mem_used, s.mem_total)))),
            ("Disk", (s.disk_total > 0).then(|| (s.disk_fraction(), format_bytes_of(s.disk_used, s.disk_total)))),
        ],
        None => [("CPU", None), ("Memory", None), ("Disk", None)],
    };
    let (x, w) = ((pill.x + PAD) as f32, (pill.w - 2.0 * PAD) as f32);
    for (i, (name, reading)) in rows.into_iter().enumerate() {
        let top = (pill.y + LIST_TOP) as f32 + i as f32 * METER_H as f32;
        v.push(text(name, label, x, top + 16.0));
        let (frac, shown) = reading.unwrap_or((0.0, "–".into()));
        let vw = engine.measure(&shown, &value);
        v.push(text(shown, value, x + w - vw, top + 16.0));
        // Amber past 75 %, red past 90 %.
        let color = if frac > 0.9 {
            RED
        } else if frac > 0.75 {
            AMBER
        } else {
            GREEN
        };
        v.push(Element::Bar { x, y: top + 25.0, w, h: 6.0, color: WHITE, opacity: o * 0.16 });
        if frac > 0.0 {
            v.push(Element::Bar { x, y: top + 25.0, w: (w * frac).max(6.0), h: 6.0, color, opacity: o });
        }
    }
    for (i, (name, shown)) in system_lines(stats, health).into_iter().enumerate() {
        let base = (pill.y + LIST_TOP + 3.0 * METER_H + 14.0 + i as f64 * SYSTEM_LINE_H) as f32;
        v.push(text(name, label, x, base));
        let vw = engine.measure(&shown, &value);
        v.push(text(shown, value, x + w - vw, base));
    }
    v
}

pub fn shelf_footer(items: usize, dragging: bool) -> Option<(String, bool)> {
    if dragging {
        Some(("Drop to keep here".into(), true))
    } else if items > 0 {
        Some(("Shortcuts only: your files stay where they are".into(), false))
    } else {
        None
    }
}

/// Is (x, y) on a shelf row's × button?
pub fn shelf_remove_hit(row: Rect, x: f64) -> bool {
    x >= row.right() - 40.0
}

pub fn expanded_bluetooth(
    pill: Rect,
    c: &Connectivity,
    scroll: usize,
    o: f32,
    engine: &mut TextEngine,
) -> Vec<Element> {
    let b: &BtState = &c.bt;
    let mut v = list_header_elements(pill, Icon::Bluetooth, "Bluetooth", None, o, engine);
    v.extend(radio_header(pill, b.available.then_some(b.powered), c.busy.as_deref(), o, engine));
    let footer = bt_footer(c);
    let shown = shown_rows(b.devices.len(), scroll, footer.is_some());
    let rows = list_rows(pill, shown);
    v.extend(scroll_bar(pill, b.devices.len(), scroll, footer.is_some(), o));
    let name_style = TextStyle { size: 15.0, weight: 600, opacity: o, ..TextStyle::default() };
    let state_style = TextStyle { size: 12.5, opacity: o * 0.6, align: Align::Right, ..TextStyle::default() };
    for (r, d) in rows.iter().zip(b.devices.iter().skip(scroll)) {
        let (x, cy, rw) = (r.x as f32, (r.y + r.h / 2.0) as f32, r.w as f32);
        v.push(Element::Icon {
            icon: device_icon(d.kind),
            x: x + 10.0,
            y: cy - 10.0,
            size: 20.0,
            color: WHITE,
            opacity: o,
        });
        let (label, color, alpha) =
            if d.connected { ("Connected", accent_text(), 1.0) } else { ("Not connected", WHITE, 0.6) };
        // A long name stops clear of its status.
        let room = rw - 40.0 - 10.0 - engine.measure(label, &state_style) - 12.0;
        let name = engine.ellipsize(&d.name, &name_style, room);
        v.push(text(name, name_style, x + 40.0, cy + 5.0));
        v.push(text(label, TextStyle { color, opacity: o * alpha, ..state_style }, x + rw - 10.0, cy + 4.5));
    }
    if b.devices.is_empty() {
        let style = TextStyle { size: 14.0, opacity: o * 0.6, ..TextStyle::default() };
        let msg = if b.available { "No paired devices" } else { "Bluetooth isn't available" };
        v.push(text(msg, style, pill.x as f32 + 50.0, (pill.y + LIST_TOP) as f32 + 23.0));
    }
    if let Some((line, button)) = footer {
        v.extend(list_footer_elements(pill, shown, &line, button, o, engine));
    }
    v
}

// ---- The right-click menu -----------------------------------------------------

/// What a menu row does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    Settings,
    /// Hide the island for an hour (or show it again).
    Hide,
    /// Pause pop-ups for an hour (or resume them).
    Quiet,
    /// Turn lyrics on or off.
    Lyrics,
    /// Lyrics timing: − / + buttons at the row's right.
    LyricsTiming,
    /// The sleep timer: off → 15 → 30 → 45 → 60 minutes → off.
    Sleep,
    /// One of the user's own shortcuts (Islet Pro), by its place in the list.
    Shortcut(u8),
    Quit,
}

/// The menu never has more rows than fit the window.
pub const MENU_MAX_ROWS: usize = 7;

/// The sleep timer's choices, in minutes.
pub const SLEEP_MINUTES: [u32; 4] = [15, 30, 45, 60];

#[derive(Debug, Clone, PartialEq)]
pub struct MenuItem {
    pub action: MenuAction,
    pub icon: Icon,
    pub label: String,
    /// Greyed text at the right ("1 h", "+0.5 s").
    pub detail: Option<String>,
    /// A switch at the right, on or off.
    pub switch: Option<bool>,
}

const MENU_TOP: f64 = 14.0;
const MENU_ROW: f64 = 34.0;

/// A row's height: the longest menu (seven rows) tightens up to fit the window.
fn menu_row(rows: usize) -> f64 {
    if rows > 6 {
        MENU_ROW - 2.0
    } else {
        MENU_ROW
    }
}

pub fn menu_height(rows: usize) -> f64 {
    MENU_TOP + rows as f64 * menu_row(rows) + 12.0
}

pub fn menu_rows(pill: Rect, rows: usize) -> Vec<Rect> {
    let h = menu_row(rows);
    (0..rows).map(|i| Rect::new(pill.x + 14.0, pill.y + MENU_TOP + i as f64 * h, pill.w - 28.0, h)).collect()
}

/// The − and + buttons of the lyrics-timing row.
pub fn menu_stepper(row: Rect) -> (Rect, Rect) {
    let cy = row.y + row.h / 2.0;
    let plus = Rect::new(row.right() - 40.0, cy - 13.0, 34.0, 26.0);
    let minus = Rect::new(plus.x - 92.0, cy - 13.0, 34.0, 26.0);
    (minus, plus)
}

pub fn expanded_menu(pill: Rect, items: &[MenuItem], o: f32, engine: &mut TextEngine) -> Vec<Element> {
    let label = TextStyle { size: 14.5, weight: 600, opacity: o, ..TextStyle::default() };
    let detail = TextStyle { size: 12.5, opacity: o * 0.6, align: Align::Right, ..TextStyle::default() };
    let mut v = Vec::new();
    for (r, item) in menu_rows(pill, items.len()).into_iter().zip(items) {
        let (x, cy, w) = (r.x as f32, (r.y + r.h / 2.0) as f32, r.w as f32);
        let color = if item.action == MenuAction::Quit { RED } else { WHITE };
        v.push(Element::Icon { icon: item.icon, x: x + 6.0, y: cy - 9.0, size: 18.0, color, opacity: o * 0.9 });
        let text_style = TextStyle { color, ..label };
        v.push(text(engine.ellipsize(&item.label, &text_style, w - 150.0), text_style, x + 34.0, cy + 5.0));
        let mut right = x + w - 6.0;
        if let Some(on) = item.switch {
            let (sw, sh) = (36.0, 22.0);
            let sx = right - sw;
            let (track, alpha) = if on { (GREEN, 1.0) } else { (WHITE, 0.22) };
            v.push(Element::Bar { x: sx, y: cy - sh / 2.0, w: sw, h: sh, color: track, opacity: o * alpha });
            let knob = sh - 4.0;
            let kx = if on { sx + sw - knob - 2.0 } else { sx + 2.0 };
            v.push(Element::Bar { x: kx, y: cy - knob / 2.0, w: knob, h: knob, color: WHITE, opacity: o });
            right = sx - 10.0;
        }
        if item.action == MenuAction::LyricsTiming {
            let (minus, plus) = menu_stepper(r);
            for (b, sign) in [(minus, "−"), (plus, "+")] {
                let (bx, by, bw, bh) = (b.x as f32, b.y as f32, b.w as f32, b.h as f32);
                v.push(Element::Bar { x: bx, y: by, w: bw, h: bh, color: WHITE, opacity: o * 0.16 });
                let style =
                    TextStyle { size: 17.0, weight: 600, opacity: o, align: Align::Center, ..TextStyle::default() };
                v.push(text(sign, style, bx + bw / 2.0, by + bh / 2.0 + 6.0));
            }
            if let Some(d) = &item.detail {
                let style = TextStyle { align: Align::Center, opacity: o * 0.8, tabular: true, ..detail };
                v.push(text(d.clone(), style, ((minus.right() + plus.x) / 2.0) as f32, cy + 4.5));
            }
            continue;
        }
        if let Some(d) = &item.detail {
            v.push(text(d.clone(), detail, right, cy + 4.5));
        }
    }
    v
}

// ---- Calls ---------------------------------------------------------------------

/// Height of the call card (below its header).
pub const CALL_H: f64 = 100.0;

/// What a call is called on the island.
pub fn call_title(c: &Call) -> &str {
    if c.app.is_empty() {
        "Microphone on"
    } else {
        &c.app
    }
}

/// The round mute button on the call card.
pub fn call_mute_button(pill: Rect) -> Rect {
    Rect::new(pill.x + PAD, pill.y + 18.0, 58.0, 58.0)
}

pub fn expanded_call(pill: Rect, c: &Call, t: f64, o: f32, engine: &mut TextEngine) -> Vec<Element> {
    let muted = c.muted == Some(true);
    let b = call_mute_button(pill);
    let (bx, by, bs) = (b.x as f32, b.y as f32, b.w as f32);
    let mut v = vec![
        Element::Bar {
            x: bx,
            y: by,
            w: bs,
            h: bs,
            color: if muted { RED } else { WHITE },
            opacity: o * if muted { 1.0 } else { 0.16 },
        },
        Element::Icon {
            icon: Icon::Mic(muted),
            x: bx + bs / 2.0 - 13.0,
            y: by + bs / 2.0 - 13.0,
            size: 26.0,
            color: WHITE,
            opacity: o,
        },
    ];
    let (x, right) = (bx + bs + 16.0, (pill.right() - PAD) as f32);
    let elapsed = format_duration(t - c.since);
    let time =
        TextStyle { size: 22.0, weight: 600, opacity: o, tabular: true, align: Align::Right, ..TextStyle::default() };
    let tw = engine.measure(&elapsed, &time);
    v.push(text(elapsed, time, right, by + 30.0));
    let title = TextStyle { size: 16.0, weight: 600, opacity: o, ..TextStyle::default() };
    v.push(text(engine.ellipsize(call_title(c), &title, right - x - tw - 12.0), title, x, by + 26.0));
    let status = match c.muted {
        Some(true) => "Muted · tap the microphone to unmute",
        Some(false) => "On a call · tap the microphone to mute",
        None => "On a call",
    };
    let small = TextStyle { size: 13.0, opacity: o * 0.65, ..TextStyle::default() };
    v.push(text(engine.ellipsize(status, &small, right - x), small, x, by + 48.0));
    v
}

// ---- The time on the empty island --------------------------------------------

/// What the empty pill shows (see `islet_core::prefs::CLOCK_FACES`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClockFace {
    #[default]
    Time,
    /// "Wed 4 · 14:05"
    Date,
    /// "14:05:09"
    Seconds,
}

impl ClockFace {
    pub fn from_pref(name: &str) -> Self {
        match name {
            "date" => Self::Date,
            "seconds" => Self::Seconds,
            _ => Self::Time,
        }
    }
}

/// The time, centred in the empty pill ("14:05", or "2:05 PM").
pub fn idle_clock(
    pill: Rect,
    now: &LocalTime,
    h24: bool,
    face: ClockFace,
    o: f32,
    engine: &mut TextEngine,
) -> Vec<Element> {
    let (time, suffix) = format_time(now, h24);
    let style = TextStyle { size: 14.0, weight: 600, opacity: o, tabular: true, ..TextStyle::default() };
    let small = TextStyle { size: 11.0, opacity: o * 0.7, ..style };
    // The date goes in front, dimmer; seconds follow the minutes.
    let lead = match face {
        ClockFace::Date => {
            let day = islet_core::clock::format_date_short(now);
            // "Wed 4 Nov" without the month: the pill is small.
            day.rsplit_once(' ').map_or(day.clone(), |(d, _)| d.to_string())
        }
        _ => String::new(),
    };
    let time = if face == ClockFace::Seconds { format!("{time}:{:02}", now.second) } else { time };
    let lead_style = TextStyle { size: 12.5, weight: 600, opacity: o * 0.6, ..TextStyle::default() };
    let lw = if lead.is_empty() { 0.0 } else { engine.measure(&lead, &lead_style) + 7.0 };
    let tw = engine.measure(&time, &style);
    let sw = suffix.as_ref().map_or(0.0, |s| 3.0 + engine.measure(s, &small));
    let (cx, baseline) = ((pill.x + pill.w / 2.0) as f32, (pill.y + pill.h / 2.0) as f32 + 5.0);
    let left = cx - (lw + tw + sw) / 2.0;
    let mut v = Vec::with_capacity(3);
    if !lead.is_empty() {
        v.push(text(lead, lead_style, left, baseline - 0.5));
    }
    v.push(text(time, style, left + lw, baseline));
    if let Some(s) = suffix {
        v.push(text(s, small, left + lw + tw + 3.0, baseline));
    }
    v
}

// ---- Volume, brightness and dark mode -----------------------------------------

/// A slider on the Controls panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tune {
    Volume,
    Brightness,
}

const TUNE_ROW: f64 = 40.0;

/// The panel's height with one or two sliders above its button.
pub fn controls_height(sliders: usize) -> f64 {
    LIST_TOP + sliders.max(1) as f64 * TUNE_ROW + 50.0
}

/// Each slider's track: where it starts, where it ends, and its middle line.
fn tune_track(pill: Rect, row: usize) -> (f64, f64, f64) {
    (pill.x + PAD + 34.0, pill.right() - PAD - 50.0, pill.y + LIST_TOP + row as f64 * TUNE_ROW + TUNE_ROW / 2.0)
}

/// The sliders shown (volume first), with the area that grabs each.
pub fn tune_sliders(pill: Rect, volume: bool, brightness: bool) -> Vec<(Rect, Tune)> {
    [(Tune::Volume, volume), (Tune::Brightness, brightness)]
        .into_iter()
        .filter(|(_, on)| *on)
        .enumerate()
        .map(|(row, (tune, _))| {
            let (left, right, cy) = tune_track(pill, row);
            (Rect::new(left - 10.0, cy - 16.0, right - left + 20.0, 32.0), tune)
        })
        .collect()
}

/// The level (0..=1) for a pointer x on a slider (they share their span).
pub fn tune_fraction(pill: Rect, x: f64) -> f32 {
    let (left, right, _) = tune_track(pill, 0);
    ((x - left) / (right - left)).clamp(0.0, 1.0) as f32
}

/// The button that switches between the dark and the light look.
pub fn dark_mode_button(pill: Rect, sliders: usize) -> Rect {
    Rect::new(pill.x + PAD, pill.y + LIST_TOP + sliders.max(1) as f64 * TUNE_ROW + 4.0, pill.w - 2.0 * PAD, 32.0)
}

/// The Controls panel: a slider each for volume and brightness (when the
/// computer tells Islet the level), and the dark-mode switch.
pub fn expanded_controls(
    pill: Rect,
    volume: Option<(f32, bool)>,
    brightness: Option<f32>,
    o: f32,
    engine: &mut TextEngine,
) -> Vec<Element> {
    let mut v = list_header_elements(pill, Icon::Controls, "Controls", None, o, engine);
    let value = TextStyle {
        size: 13.0,
        weight: 600,
        opacity: o * 0.7,
        tabular: true,
        align: Align::Right,
        ..TextStyle::default()
    };
    let sliders = tune_sliders(pill, volume.is_some(), brightness.is_some());
    for (row, (_, tune)) in sliders.iter().enumerate() {
        let (left, right, cy) = tune_track(pill, row);
        let (level, icon, dim) = match tune {
            Tune::Volume => {
                let (level, muted) = volume.unwrap_or((0.0, false));
                let icon = if muted {
                    Icon::Volume(255)
                } else if level <= 0.001 {
                    Icon::Volume(0)
                } else {
                    Icon::Volume(if level < 0.34 {
                        1
                    } else if level < 0.67 {
                        2
                    } else {
                        3
                    })
                };
                (level, icon, muted)
            }
            Tune::Brightness => (brightness.unwrap_or(0.0), Icon::Sun, false),
        };
        let x = (pill.x + PAD) as f32;
        v.push(Element::Icon { icon, x, y: cy as f32 - 10.0, size: 20.0, color: WHITE, opacity: o });
        v.extend(level_bar(level, right as f32, cy as f32, (right - left) as f32, dim, o));
        // A round handle where the level is.
        let hx = left as f32 + (right - left) as f32 * level.clamp(0.0, 1.0);
        v.push(Element::Ring { cx: hx, cy: cy as f32, r: 3.5, width: 7.0, fraction: 1.0, color: WHITE, opacity: o });
        let pct = format!("{}%", (level.clamp(0.0, 1.0) * 100.0).round() as i32);
        v.push(text(pct, value, (pill.right() - PAD) as f32, cy as f32 + 4.5));
    }
    if sliders.is_empty() {
        let note = TextStyle { size: 13.0, opacity: o * 0.6, ..TextStyle::default() };
        v.push(text(
            "No volume or brightness to set here",
            note,
            (pill.x + PAD) as f32,
            (pill.y + LIST_TOP) as f32 + 24.0,
        ));
    }
    let b = dark_mode_button(pill, sliders.len());
    let (bx, by, bw, bh) = (b.x as f32, b.y as f32, b.w as f32, b.h as f32);
    let label = TextStyle { size: 13.5, weight: 600, opacity: o, ..TextStyle::default() };
    let words = "Switch dark / light mode";
    let tw = engine.measure(words, &label);
    let start = bx + (bw - tw - 24.0) / 2.0;
    v.push(Element::Bar { x: bx, y: by, w: bw, h: bh, color: WHITE, opacity: o * 0.14 });
    v.push(Element::Icon { icon: Icon::Moon, x: start, y: by + bh / 2.0 - 8.0, size: 16.0, color: WHITE, opacity: o });
    v.push(text(words, label, start + 24.0, by + bh / 2.0 + 4.5));
    v
}

// ---- Status on hover ----------------------------------------------------------

/// One thing beside the time on the hovered, empty island: the weather,
/// the battery, how busy the processor is.
#[derive(Debug, Clone, PartialEq)]
pub struct StatusItem {
    pub icon: Icon,
    pub text: String,
    pub color: [u8; 3],
}

/// Room for one item (icon and a short number), and between two of them.
const STATUS_ITEM_W: f64 = 56.0;
const STATUS_GAP: f64 = 4.0;
/// Room kept for the time in the middle ("12:05 PM" at its widest).
const STATUS_TIME_W: f64 = 76.0;

/// How many of `n` items sit left of the time; the rest sit right. They
/// alternate (left, right, left).
fn status_left(n: usize) -> usize {
    n.div_ceil(2)
}

/// The width `k` items take on one side of the time, with the gap to it.
fn status_side(k: usize) -> f64 {
    if k == 0 {
        0.0
    } else {
        k as f64 * STATUS_ITEM_W + (k - 1) as f64 * STATUS_GAP + 6.0
    }
}

/// The hovered pill's width with `n` items beside the time (0: no strip).
pub fn status_width(n: usize) -> f64 {
    if n == 0 {
        return 0.0;
    }
    let left = status_left(n);
    STATUS_TIME_W + status_side(left) + status_side(n - left) + 20.0
}

/// How far right of the pill's middle the time sits with `n` items, so the
/// whole row is centred when one side holds more than the other.
pub fn status_shift(n: usize) -> f64 {
    let left = status_left(n);
    (status_side(left) - status_side(n - left)) / 2.0
}

/// The items either side of the time. `pill` is centred on the time (the
/// time itself is `idle_clock`, drawn in the same rectangle).
pub fn status_strip(pill: Rect, items: &[StatusItem], o: f32) -> Vec<Element> {
    let (cx, cy) = ((pill.x + pill.w / 2.0) as f32, (pill.y + pill.h / 2.0) as f32);
    let style = TextStyle { size: 13.0, weight: 600, opacity: o, tabular: true, ..TextStyle::default() };
    let (item_w, gap, half) = (STATUS_ITEM_W as f32, STATUS_GAP as f32, STATUS_TIME_W as f32 / 2.0 + 6.0);
    let (mut left, mut right) = (0.0f32, 0.0f32);
    let mut v = Vec::with_capacity(items.len() * 2);
    for (i, item) in items.iter().enumerate() {
        // Even ones go left of the time, outwards; odd ones go right.
        let x = if i % 2 == 0 {
            left += item_w + if left > 0.0 { gap } else { 0.0 };
            cx - half - left
        } else {
            let x = cx + half + right;
            right += item_w + gap;
            x
        };
        v.push(Element::Icon { icon: item.icon, x, y: cy - 8.0, size: 16.0, color: item.color, opacity: o });
        v.push(text(item.text.clone(), TextStyle { color: item.color, ..style }, x + 20.0, cy + 4.5));
    }
    v
}

// ---- Back to Control Centre ------------------------------------------------

/// Height of the strip at the top of an activity card that holds the
/// "Control Centre" button; the card's own content starts below it.
pub const CARD_HEADER_H: f64 = 26.0;

/// The "‹ Control Centre" button at the top of an activity card.
pub fn home_button(pill: Rect) -> Rect {
    Rect::new(pill.x + 28.0, pill.y + 9.0, 132.0, 24.0)
}

/// A capsule button with a chevron and a label, as wide as its label.
fn capsule(r: Rect, label: &str, icon: Icon, icon_first: bool, o: f32, engine: &mut TextEngine) -> Vec<Element> {
    let style = TextStyle { size: 12.5, weight: 600, opacity: o * 0.9, ..TextStyle::default() };
    let label = engine.ellipsize(label, &style, r.w as f32 - 34.0);
    let tw = engine.measure(&label, &style);
    let (x, y, h) = (r.x as f32, r.y as f32, r.h as f32);
    let w = tw + 34.0;
    let (icon_x, text_x) = if icon_first { (x + 7.0, x + 24.0) } else { (x + w - 21.0, x + 11.0) };
    vec![
        Element::Bar { x, y, w, h, color: WHITE, opacity: o * 0.14 },
        Element::Icon { icon, x: icon_x, y: y + h / 2.0 - 7.0, size: 14.0, color: WHITE, opacity: o * 0.9 },
        text(label, style, text_x, y + h / 2.0 + 4.5),
    ]
}

pub fn home_button_elements(pill: Rect, o: f32, engine: &mut TextEngine) -> Vec<Element> {
    capsule(home_button(pill), "Control Centre", Icon::Back, true, o, engine)
}

/// On Control Centre opened from an activity: the button back to it
/// ("Now Playing ›"), where the clock card has room above the time.
pub fn return_button(pill: Rect) -> Rect {
    Rect::new(pill.x + 24.0, pill.y + 8.0, 170.0, 22.0)
}

pub fn return_button_elements(pill: Rect, label: &str, o: f32, engine: &mut TextEngine) -> Vec<Element> {
    capsule(return_button(pill), label, Icon::Forward, false, o, engine)
}

/// The card's area below its header.
pub fn below_header(pill: Rect) -> Rect {
    Rect::new(pill.x, pill.y + CARD_HEADER_H, pill.w, (pill.h - CARD_HEADER_H).max(0.0))
}

// ---- Lyrics ------------------------------------------------------------------

/// The compact pill while lyrics show: wide enough for a line or so.
pub const COMPACT_LYRICS_W: f64 = 340.0;
/// Where the Now Playing card's controls end.
const MEDIA_CONTENT_H: f64 = 152.0;
/// Extra height of the Now Playing card for two lines of lyrics.
pub const CARD_LYRICS_H: f64 = 46.0;
/// Lines that aren't being sung stay readable, just dimmer.
const LYRIC_DIM: f32 = 0.38;

/// Compact Now Playing with lyrics: art, a strip of lines sliding through
/// the pill with the one being sung highlighted, and the waveform.
pub fn compact_lyrics(
    pill: Rect,
    m: &MediaInfo,
    lyrics: &Lyrics,
    pos: f64,
    t: f64,
    o: f32,
    engine: &mut TextEngine,
) -> Vec<Element> {
    let (x, right) = (pill.x as f32, pill.right() as f32);
    let cy = (pill.y + pill.h / 2.0) as f32;
    let tint = m.art.as_ref().map_or(WHITE, |a| brighten(a.dominant));
    let mut v =
        vec![Element::Art { x: x + 6.0, y: cy - 12.5, size: 25.0, radius: 7.0, image: m.art.clone(), opacity: o }];
    v.extend(waveform(right - 31.0, cy, 4, t, m.playing, tint, o));
    let (left, end) = (x + 38.0, right - 40.0);
    v.extend(lyric_strip(left, end, cy + 4.5, lyrics, pos, o, engine));
    v
}

/// The lines of the strip that are in view between `left` and `end`.
fn lyric_strip(
    left: f32,
    end: f32,
    baseline: f32,
    lyrics: &Lyrics,
    pos: f64,
    o: f32,
    engine: &mut TextEngine,
) -> Vec<Element> {
    let style = TextStyle { size: 13.5, weight: 600, ..TextStyle::default() };
    let widths: Vec<f32> = (0..lyrics.lines.len()).map(|i| engine.measure(lyrics.text(i), &style)).collect();
    let room = end - left;
    let strip = lyrics::strip(lyrics, &widths, room, pos);
    let starts = lyrics::starts(&widths);
    let mut v = Vec::new();
    for (i, (&s, &w)) in starts.iter().zip(&widths).enumerate() {
        let lx = left + s - strip.scroll;
        if lx > end || lx + w < left {
            continue;
        }
        let lit = match strip.current {
            Some(c) if c == i => strip.mix,
            Some(c) if c == i + 1 => 1.0 - strip.mix,
            _ => 0.0,
        };
        let style = TextStyle { opacity: o * (LYRIC_DIM + (1.0 - LYRIC_DIM) * lit), ..style };
        let mut item = TextItem::new(lyrics.text(i), style, lx, baseline);
        item.window = Some((left, end));
        v.push(Element::Text(item));
    }
    v
}

/// Two lines under the Now Playing controls: the line being sung,
/// highlighted, and the next one dimmed; they roll up as the song goes on.
pub fn card_lyrics(pill: Rect, lyrics: &Lyrics, pos: f64, o: f32, engine: &mut TextEngine) -> Vec<Element> {
    const ROW: f32 = 21.0;
    // Centred, clear of the card's round bottom corners.
    let (cx, w) = ((pill.x + pill.w / 2.0) as f32, pill.w as f32 - 4.0 * PAD as f32);
    let top = pill.y as f32 + MEDIA_CONTENT_H as f32;
    let style = TextStyle { size: 15.0, weight: 600, align: Align::Center, ..TextStyle::default() };
    let (cur, mix) = match lyrics.current(pos) {
        Some(i) => (i as f32, ((pos + lyrics::LEAD - lyrics.lines[i].at) / lyrics::SLIDE_SECS).clamp(0.0, 1.0) as f32),
        None => (-1.0, 1.0),
    };
    let ease = 1.0 - (1.0 - mix).powi(3);
    // Row 0 is the line being sung; it slides up from row 1 as it starts.
    let scroll = cur - 1.0 + ease;
    let mut v = Vec::new();
    let first = (scroll.floor().max(0.0)) as usize;
    for i in first..(first + 3).min(lyrics.lines.len()) {
        let row = i as f32 - scroll; // 0 = top row, 1 = bottom row
        if !(-0.99..=1.99).contains(&row) {
            continue;
        }
        let lit = if i as f32 == cur {
            ease
        } else if i as f32 == cur - 1.0 {
            1.0 - ease
        } else {
            0.0
        };
        // Rows fade out as they leave the two-row window.
        let edge = (1.0 - (-row).max(row - 1.0).max(0.0)).clamp(0.0, 1.0);
        let style = TextStyle { opacity: o * edge * (LYRIC_DIM + (1.0 - LYRIC_DIM) * lit), ..style };
        let line = engine.ellipsize(lyrics.text(i), &style, w);
        v.push(text(line, style, cx, top + 18.0 + row * ROW));
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
    use islet_core::activity::MediaCaps;

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
        let v = expanded_media(pill, &song(), 0.0, 1.0, None, None, false, &mut engine);
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
        for (r, _) in media_controls(pill, true) {
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
                place_id: None,
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
