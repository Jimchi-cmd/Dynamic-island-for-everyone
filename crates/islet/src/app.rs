//! Portable island logic: pointer input and provider reports → presentation
//! state machine → springs → frame and content. No OS calls in here, so all
//! of it is unit-tested.

use crate::content::{self, Control};
use crate::platform::{Button, Event, Key};
use islet_core::activity::{Activities, Live, MediaCommand, MediaInfo, Report};
use islet_core::clock::{secs_to_next_minute, LocalTime};
use islet_core::geometry::Rect;
use islet_core::island::{
    spring_for, target_size, Island, Presentation, Signal, AUTO_COLLAPSE_SECS, LONG_PRESS_SECS, MINIMAL_CIRCLE,
    MINIMAL_GAP,
};
use islet_core::spring::{Animated, Spring};
use islet_render::elements::Element;
use islet_render::text::TextEngine;
use islet_render::{Circle, PillFrame, Squircle};
use std::time::Duration;

/// Logical window size: the largest state plus spring overshoot (design spec §2).
pub const WINDOW: (f64, f64) = (420.0, 260.0);
/// Gap between the screen's top edge and the pill.
pub const TOP_GAP: f64 = 0.0;
const SMOOTHING: f32 = 0.6;

/// Frame interval while a spring is moving (60 fps)…
pub const FRAME: Duration = Duration::from_micros(16_667);
/// …and while only the waveform moves (30 fps).
pub const WAVE_FRAME: Duration = Duration::from_micros(33_333);

/// Distance from the pill's round end to the centre of the detached circle
/// in Minimal: half the circle, the gap, and the other half.
const BUBBLE_OFFSET: f64 = MINIMAL_CIRCLE / 2.0 + MINIMAL_GAP + MINIMAL_CIRCLE / 2.0;
/// Merging back is quick and bounce-free.
const BUBBLE_MERGE: Spring = Spring::new(0.32, 1.0);
/// Above this pill height the bubble is fully merged.
const BUBBLE_MAX_H: f64 = 60.0;
/// Expanded content never shows below this pill height…
const CONTENT_FROM_H: f64 = 55.0;
/// …fades in over (at most) the last this-many px of the card's growth…
const CONTENT_FADE_H: f64 = 60.0;
/// …and cards are treated as at least this tall for the fade.
const CONTENT_MIN_FULL_H: f64 = 84.0;
const IDLE_W: f64 = 126.0;
/// Height of the expanded clock and media cards.
const CLOCK_H: f64 = 124.0;
const MEDIA_H: f64 = 160.0;

fn smooth(c: f64) -> f64 {
    let c = c.clamp(0.0, 1.0);
    c * c * (3.0 - 2.0 * c)
}

/// Scripted demos, selected with `ISLET_DEMO` (or the legacy
/// `ISLET_PERF_DEMO`), used by the perf harness and for visual review.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Demo {
    None,
    /// Expand/collapse every 1.2 s (perf harness).
    Toggle,
    /// Walk through every presentation every 1.6 s.
    Tour,
}

impl Demo {
    fn period(self) -> f64 {
        match self {
            Demo::Tour => 1.6,
            _ => 1.2,
        }
    }

    fn signals(self) -> &'static [Signal] {
        match self {
            Demo::None => &[],
            Demo::Toggle => &[Signal::Click],
            Demo::Tour => &[
                Signal::LiveActivities(1), // Compact
                Signal::LiveActivities(2), // Minimal: the bubble splits off
                Signal::Click,             // Expanded
                Signal::LongPress,         // Large
                Signal::Escape,            // back to Minimal
                Signal::LiveActivities(1), // the bubble merges back
                Signal::LiveActivities(0), // Idle
                Signal::PointerEnter,      // Peek
                Signal::PointerLeave,
            ],
        }
    }
}

struct Press {
    at: f64,
    long_fired: bool,
}

/// What an open island shows.
enum OpenContent<'a> {
    Media(&'a MediaInfo),
    Battery,
    Clock,
}

pub struct App {
    island: Island,
    activities: Activities,
    w: Animated,
    h: Animated,
    r: Animated,
    /// Bubble centre relative to the pill's round end: 0 = merged.
    bubble: Animated,
    hovered: bool,
    press: Option<Press>,
    /// A media control being pressed, and the scrub position while dragging.
    control: Option<Control>,
    scrub: Option<f64>,
    commands: Vec<MediaCommand>,
    /// When the pointer left an open island (auto-collapse timer).
    left_open_at: Option<f64>,
    demo: Demo,
    demo_step: usize,
    demo_next: f64,
    reduce_motion: bool,
    /// Input region while animating: start and end of the transition.
    anim_region: Rect,
    quit: bool,
    /// Latest wall-clock time (the clock), and 12/24-hour style.
    clock: Option<LocalTime>,
    h24: bool,
    /// Height of the card when the island last opened: content fades
    /// relative to it, so short cards (a device list) fade in fully too.
    open_h: f64,
    /// The last closed presentation (Compact or Minimal): its layout is
    /// used for compact content while the island opens or closes.
    closed: Presentation,
    /// The MacBook notch (width, height), when the island wraps one.
    notch: Option<(f64, f64)>,
}

impl App {
    pub fn new(demo: Demo, reduce_motion: bool) -> Self {
        let (w, h, r) = target_size(Presentation::Idle, None);
        let s = Spring::new(0.5, 0.8);
        let mut app = Self {
            island: Island::new(),
            activities: Activities::new(),
            w: Animated::new(w, s),
            h: Animated::new(h, s),
            r: Animated::new(r, s),
            bubble: Animated::new(0.0, s),
            hovered: false,
            press: None,
            control: None,
            scrub: None,
            commands: Vec::new(),
            left_open_at: None,
            demo,
            demo_step: 0,
            demo_next: demo.period(),
            reduce_motion,
            anim_region: Rect::new(0.0, 0.0, 0.0, 0.0),
            quit: false,
            clock: None,
            h24: true,
            open_h: MEDIA_H,
            closed: Presentation::Idle,
            notch: None,
        };
        app.anim_region = app.target_bounds();
        app
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn presentation(&self) -> Presentation {
        self.island.presentation()
    }

    pub fn wants_quit(&self) -> bool {
        self.quit
    }

    pub fn set_clock_style(&mut self, h24: bool) {
        self.h24 = h24;
    }

    /// Wrap a physical notch (width, height in logical px): idle becomes the
    /// notch itself, compact content sits either side of it and expanded
    /// content below it, and the top corners are square.
    pub fn set_notch(&mut self, notch: Option<(f64, f64)>, t: f64) {
        self.notch = notch;
        let p = self.island.presentation();
        let dims = self.target_dims(p, t);
        for (a, v) in [(&mut self.w, dims.0), (&mut self.h, dims.1), (&mut self.r, dims.2)] {
            a.snap(v);
        }
        self.anim_region = self.target_bounds();
    }

    /// Where expanded content goes: below the notch, if there is one.
    fn content_rect(&self, pill: Rect) -> Rect {
        match self.notch {
            Some((_, nh)) => Rect::new(pill.x, pill.y + nh, pill.w, (pill.h - nh).max(0.0)),
            None => pill,
        }
    }

    /// Commands for the media session, collected since the last call.
    pub fn take_commands(&mut self) -> Vec<MediaCommand> {
        std::mem::take(&mut self.commands)
    }

    /// Apply a provider report. Returns true if a redraw is needed.
    pub fn report(&mut self, r: Report, t: f64) -> bool {
        let changed = self.activities.report(r, t);
        self.sync_live(t) || changed
    }

    /// Store the current wall-clock time. Returns true when the displayed
    /// minute changed while the clock is on screen (a redraw is needed).
    pub fn set_time(&mut self, now: LocalTime, t: f64) -> bool {
        let changed = self.clock.is_none_or(|c| (c.hour, c.minute, c.day) != (now.hour, now.minute, now.day));
        self.clock = Some(now);
        changed && self.clock_shown(t)
    }

    fn live(&self, t: f64) -> Vec<Live> {
        self.activities.live(t)
    }

    fn open_content(&self, live: &[Live]) -> OpenContent<'_> {
        if matches!(live.first(), Some(Live::Battery { .. })) {
            OpenContent::Battery
        } else if let Some(m) = self.activities.media() {
            OpenContent::Media(m)
        } else {
            OpenContent::Clock
        }
    }

    fn clock_shown(&self, t: f64) -> bool {
        (self.expanded_opacity(t) > 0.0 || self.island.presentation().is_open())
            && matches!(self.open_content(&self.live(t)), OpenContent::Clock)
    }

    /// Expanded content fades and rises in as the pill grows taller, and out
    /// as it shrinks, so it never spills outside a closing island.
    pub fn expanded_opacity(&self, t: f64) -> f32 {
        // Fade over the last stretch of the card's growth (at most 60 px),
        // and never below 55 px so nothing shows in a compact-sized pill.
        let full = self.open_h.max(CONTENT_MIN_FULL_H);
        let from = (full - CONTENT_FADE_H).max(CONTENT_FROM_H);
        smooth((self.h.value(t) - from) / (full - from)) as f32
    }

    /// Compact content shows while the pill is wide and short.
    fn compact_opacity(&self, t: f64, live: &[Live]) -> f32 {
        let p = self.pill_rect(t);
        let full_w = match self.island.presentation() {
            Presentation::Minimal => target_size(Presentation::Minimal, None).0,
            _ => live.first().map_or(content::COMPACT_MEDIA_W, content::compact_width),
        };
        let span = (full_w.min(190.0) - IDLE_W - 6.0).max(1.0);
        (smooth((p.w - IDLE_W - 6.0) / span) * smooth((60.0 - p.h) / 20.0)) as f32
    }

    /// Whether anything is drawn inside the island right now (cheap check, so
    /// fonts are only loaded once there is something to show).
    pub fn has_content(&self, t: f64) -> bool {
        self.expanded_opacity(t) > 0.0 || self.compact_opacity(t, &self.live(t)) > 0.0
    }

    /// Everything drawn inside the island this frame.
    pub fn elements(&self, t: f64, engine: &mut TextEngine) -> Vec<Element> {
        let live = self.live(t);
        let p = self.pill_rect(t);
        let mut out = Vec::new();
        let eo = self.expanded_opacity(t);
        let cr = self.content_rect(p);
        if eo > 0.0 {
            match self.open_content(&live) {
                OpenContent::Media(m) => out.extend(content::expanded_media(cr, m, t, eo, self.scrub, engine)),
                OpenContent::Battery => out.extend(content::expanded_battery(
                    cr,
                    &self.activities.devices(),
                    self.activities.laptop(),
                    eo,
                    engine,
                )),
                OpenContent::Clock => {
                    if let Some(now) = self.clock {
                        out.extend(content::expanded_clock(cr, &now, self.h24, eo, engine));
                    }
                }
            }
        }
        let co = self.compact_opacity(t, &live);
        if co > 0.0 {
            // While opening or closing, keep the layout of the closed state.
            let shape = match self.island.presentation() {
                p if p.is_open() => self.closed,
                p => p,
            };
            match (shape, live.first()) {
                (Presentation::Minimal, Some(first)) => {
                    // The lasting activity (music) keeps the pill; a transient
                    // battery peek is the one that buds off and merges back.
                    let (pill, bubble) = match (first, live.get(1)) {
                        (Live::Battery { .. }, Some(second)) => (second, Some(first)),
                        _ => (first, live.get(1)),
                    };
                    out.extend(content::minimal(p, self.bubble_circle(t), pill, bubble, t, co));
                }
                (_, Some(first)) => out.extend(content::compact(p, first, t, co, engine)),
                _ => {}
            }
        }
        out
    }

    /// Size for a presentation, adapted to what it shows.
    fn target_dims(&self, p: Presentation, t: f64) -> (f64, f64, f64) {
        let (w, h, r) = target_size(p, None);
        let live = self.live(t);
        let dims = match p {
            Presentation::Compact => (live.first().map_or(w, content::compact_width), h, r),
            Presentation::Expanded => {
                let h = match self.open_content(&live) {
                    OpenContent::Media(_) => MEDIA_H,
                    OpenContent::Clock => CLOCK_H,
                    OpenContent::Battery => {
                        let rows = self.activities.devices().len() + usize::from(self.activities.laptop().is_some());
                        content::battery_list_height(rows)
                    }
                };
                (w, h, r)
            }
            _ => (w, h, r),
        };
        match self.notch {
            None => dims,
            Some((nw, nh)) => notch_dims(p, dims, nw, nh),
        }
    }

    fn set_targets(&mut self, (w, h, r): (f64, f64, f64), spring: Spring, t: f64) {
        for (a, v) in [(&mut self.w, w), (&mut self.h, h), (&mut self.r, r)] {
            if self.reduce_motion {
                a.snap(v);
            } else if (a.target() - v).abs() > 1e-9 {
                a.set_target(v, t, spring);
            }
        }
    }

    /// Feed a signal to the state machine and retarget the springs.
    fn signal(&mut self, s: Signal, t: f64) -> bool {
        let from = self.island.presentation();
        let Some(to) = self.island.apply(s) else { return false };
        let spring = spring_for(from, to);
        let start = self.bounds(t);
        let dims = self.target_dims(to, t);
        self.set_targets(dims, spring, t);
        if to.is_open() {
            self.open_h = dims.1;
        } else {
            self.closed = to;
        }
        // The bubble buds off with the bouncy split spring but is absorbed
        // quickly, before the pill has grown much (as on iOS).
        let (bubble, bubble_spring) =
            if to == Presentation::Minimal { (BUBBLE_OFFSET, spring) } else { (0.0, BUBBLE_MERGE) };
        if self.reduce_motion {
            self.bubble.snap(bubble);
        } else if self.bubble.target() != bubble {
            self.bubble.set_target(bubble, t, bubble_spring);
        }
        self.anim_region = start.union(&self.target_bounds());
        if !to.is_open() {
            self.left_open_at = None;
            self.control = None;
            self.scrub = None;
        }
        true
    }

    /// Bring the presentation and size in line with what is live.
    fn sync_live(&mut self, t: f64) -> bool {
        let n = self.live(t).len();
        let mut changed = self.signal(Signal::LiveActivities(n), t);
        // Same presentation, new content size (a battery peek handing over to
        // media): glide to the new size.
        let p = self.island.presentation();
        let dims = self.target_dims(p, t);
        if (self.w.target() - dims.0).abs() > 0.5 || (self.h.target() - dims.1).abs() > 0.5 {
            let start = self.bounds(t);
            self.set_targets(dims, spring_for(p, p), t);
            if p.is_open() {
                self.open_h = dims.1;
            }
            self.anim_region = start.union(&self.target_bounds());
            changed = true;
        }
        changed
    }

    fn pointer_at(&mut self, x: f64, y: f64, t: f64) -> bool {
        let inside = self.hit(x, y, t);
        if inside == self.hovered {
            return false;
        }
        self.hovered = inside;
        if inside {
            self.left_open_at = None;
            self.signal(Signal::PointerEnter, t)
        } else {
            if self.island.presentation().is_open() {
                self.left_open_at = Some(t);
            }
            self.press = None;
            self.signal(Signal::PointerLeave, t)
        }
    }

    /// The media control under (x, y) on an open "Now Playing" card.
    fn control_at(&self, x: f64, y: f64, t: f64) -> Option<Control> {
        if !self.island.presentation().is_open() || self.animating(t) {
            return None;
        }
        if !matches!(self.open_content(&self.live(t)), OpenContent::Media(_)) {
            return None;
        }
        let cr = self.content_rect(self.pill_rect(t));
        content::media_controls(cr).into_iter().find(|(r, _)| r.contains(x, y)).map(|(_, c)| c)
    }

    /// Returns true when the event changed what is on screen.
    pub fn handle(&mut self, e: Event, t: f64) -> bool {
        match e {
            Event::PointerMove { x, .. } if self.control == Some(Control::Scrub) => {
                self.scrub = Some(content::scrub_fraction(self.content_rect(self.pill_rect(t)), x));
                true
            }
            Event::PointerMove { x, y } => self.pointer_at(x, y, t),
            Event::PointerLeave => self.pointer_at(f64::NAN, f64::NAN, t),
            Event::PointerDown { x, y, button: Button::Primary } if self.hit(x, y, t) => {
                match self.control_at(x, y, t) {
                    Some(c) => {
                        self.control = Some(c);
                        if c == Control::Scrub && self.activities.media().is_some_and(|m| m.caps.seek) {
                            self.scrub = Some(content::scrub_fraction(self.content_rect(self.pill_rect(t)), x));
                        }
                        true
                    }
                    None => {
                        self.press = Some(Press { at: t, long_fired: false });
                        false
                    }
                }
            }
            Event::PointerUp { x, y, button: Button::Primary } => {
                if let Some(c) = self.control.take() {
                    let scrub = self.scrub.take();
                    let media = self.activities.media();
                    let command = match c {
                        Control::Scrub => {
                            scrub.zip(media.and_then(|m| m.duration)).map(|(f, d)| MediaCommand::SeekTo(f * d))
                        }
                        _ if self.control_at(x, y, t) != Some(c) => None, // released elsewhere
                        Control::PlayPause => Some(MediaCommand::PlayPause),
                        Control::Next => Some(MediaCommand::Next),
                        Control::Previous => Some(MediaCommand::Previous),
                    };
                    self.commands.extend(command);
                    return true;
                }
                match self.press.take() {
                    Some(p) if !p.long_fired => self.signal(Signal::Click, t),
                    _ => false,
                }
            }
            // Until the context menu exists: right-click the island to quit.
            Event::PointerDown { x, y, button: Button::Secondary } if self.hit(x, y, t) => {
                self.quit = true;
                false
            }
            Event::Key(Key::Escape) => self.signal(Signal::Escape, t),
            Event::Redraw | Event::ScaleChanged(_) => true,
            _ => false,
        }
    }

    /// Advance timers. Returns true if anything changed.
    pub fn tick(&mut self, t: f64) -> bool {
        let mut changed = self.sync_live(t);
        if let Some(p) = &mut self.press {
            if !p.long_fired && t - p.at >= LONG_PRESS_SECS {
                p.long_fired = true;
                changed |= self.signal(Signal::LongPress, t);
            }
        }
        if let Some(left) = self.left_open_at {
            if t - left >= AUTO_COLLAPSE_SECS {
                self.left_open_at = None;
                changed |= self.signal(Signal::AutoCollapse, t);
            }
        }
        let signals = self.demo.signals();
        if !signals.is_empty() && t >= self.demo_next {
            self.demo_next += self.demo.period();
            let s = signals[self.demo_step % signals.len()];
            self.demo_step += 1;
            changed |= self.signal(s, t);
        }
        changed
    }

    pub fn animating(&self, t: f64) -> bool {
        !(self.w.is_settled(t) && self.h.is_settled(t) && self.r.is_settled(t) && self.bubble.is_settled(t))
    }

    /// How often to draw while something moves on its own: 60 fps for the
    /// shape, 30 fps for a waveform; None when nothing moves (zero wakeups).
    pub fn frame_interval(&self, t: f64) -> Option<Duration> {
        if self.animating(t) {
            return Some(FRAME);
        }
        let live = self.live(t);
        let playing = self.activities.media().is_some_and(|m| m.playing);
        let compact_wave = matches!(live.first(), Some(Live::Media(_))) && self.compact_opacity(t, &live) > 0.0;
        let card_wave = self.expanded_opacity(t) > 0.0 && matches!(self.open_content(&live), OpenContent::Media(_));
        (playing && (compact_wave || card_wave)).then_some(WAVE_FRAME)
    }

    /// Seconds from `t` until the next timer, if any.
    pub fn next_timer(&self, t: f64) -> Option<f64> {
        let mut next: Option<f64> = None;
        let mut at = |deadline: f64| next = Some(next.map_or(deadline, |n: f64| n.min(deadline)));
        if let Some(p) = &self.press {
            if !p.long_fired {
                at(p.at + LONG_PRESS_SECS);
            }
        }
        if let Some(left) = self.left_open_at {
            at(left + AUTO_COLLAPSE_SECS);
        }
        if self.demo != Demo::None {
            at(self.demo_next);
        }
        if let Some(change) = self.activities.next_change(t) {
            at(change);
        }
        // The clock wakes the island once a minute, and only while it is shown.
        if let Some(now) = self.clock.filter(|_| self.clock_shown(t)) {
            at(t + secs_to_next_minute(&now));
        }
        next.map(|n| (n - t).max(0.0))
    }

    fn pill_at(&self, w: f64, h: f64) -> Rect {
        Rect::new((WINDOW.0 - w) / 2.0, TOP_GAP, w, h)
    }

    pub fn pill_rect(&self, t: f64) -> Rect {
        self.pill_at(self.w.value(t).max(0.0), self.h.value(t).max(0.0))
    }

    /// The detached circle, while it is (or is becoming) visible.
    fn bubble_circle(&self, t: f64) -> Option<Circle> {
        let p = self.pill_rect(t);
        // The circle only separates while the pill is capsule-height: it is
        // pulled in as the pill grows into a card and buds off only once a
        // card has shrunk back, so it never pokes out of a big corner.
        let c = ((BUBBLE_MAX_H - p.h) / (BUBBLE_MAX_H - MINIMAL_CIRCLE)).clamp(0.0, 1.0);
        let off = self.bubble.value(t) * c * c * (3.0 - 2.0 * c);
        if off.abs() < 0.25 {
            return None; // merged: hidden inside the pill
        }
        // Anchored on the centre of the top-right corner's circle, so a merged
        // bubble is always fully inside the outline.
        let corner = self.r.value(t).max(0.0).min(p.w / 2.0).min(p.h / 2.0);
        let r = MINIMAL_CIRCLE / 2.0;
        Some(Circle { x: (p.right() - corner + off) as f32, y: (p.y + corner.max(r)) as f32, r: r as f32 })
    }

    /// Pill plus bubble.
    fn bounds(&self, t: f64) -> Rect {
        let pill = self.pill_rect(t);
        match self.bubble_circle(t) {
            Some(c) => pill.union(&circle_rect(c)),
            None => pill,
        }
    }

    fn target_bounds(&self) -> Rect {
        let pill = self.pill_at(self.w.target(), self.h.target());
        if self.bubble.target() > 0.0 {
            let r = MINIMAL_CIRCLE / 2.0;
            let cx = pill.right() - r + self.bubble.target();
            pill.union(&Rect::new(cx - r, pill.y, 2.0 * r, 2.0 * r))
        } else {
            pill
        }
    }

    /// Is (x, y) on the island (pill or bubble)?
    fn hit(&self, x: f64, y: f64, t: f64) -> bool {
        if !(x.is_finite() && y.is_finite()) {
            return false;
        }
        let in_bubble = self.bubble_circle(t).is_some_and(|c| {
            let (dx, dy) = (x - c.x as f64, y - c.y as f64);
            dx * dx + dy * dy <= (c.r as f64) * (c.r as f64)
        });
        self.pill_rect(t).contains(x, y) || in_bubble
    }

    /// Where pointer input should be accepted. While animating it covers both
    /// ends of the animation, so it changes twice per transition, not per frame.
    pub fn input_region(&self, t: f64) -> Rect {
        let r = if self.animating(t) { self.anim_region } else { self.target_bounds() };
        r.inflate(1.0)
    }

    pub fn frame(&self, t: f64) -> PillFrame {
        let r = self.pill_rect(t);
        PillFrame {
            shape: Squircle {
                x: r.x as f32,
                y: r.y as f32,
                w: r.w as f32,
                h: r.h as f32,
                radius: self.r.value(t).max(0.0) as f32,
                smoothing: SMOOTHING,
            },
            bubble: self.bubble_circle(t),
            highlight: 0.0,
            flat_top: self.notch.is_some(),
        }
    }
}

/// Space beside the notch for compact content (art on the left, waveform
/// or ring on the right); the notch itself hides whatever is drawn there.
const NOTCH_SIDE: f64 = 46.0;
/// The physical notch's bottom corner radius (approximate).
const NOTCH_R: f64 = 9.0;

/// Sizes when wrapping a notch of `nw` × `nh`.
fn notch_dims(p: Presentation, (w, h, r): (f64, f64, f64), nw: f64, nh: f64) -> (f64, f64, f64) {
    match p {
        Presentation::Idle => (nw, nh, NOTCH_R),
        Presentation::Peek => (nw + 12.0, nh + 3.0, NOTCH_R + 2.0),
        Presentation::Compact => (w.max(nw + 2.0 * NOTCH_SIDE), nh, NOTCH_R + 4.0),
        Presentation::Minimal => (nw + 2.0 * NOTCH_SIDE - 12.0, nh, NOTCH_R + 4.0),
        Presentation::Expanded | Presentation::Large => (w.max(nw + 40.0), h + nh, r),
    }
}

fn circle_rect(c: Circle) -> Rect {
    let (x, y, r) = (c.x as f64, c.y as f64, c.r as f64);
    Rect::new(x - r, y - r, 2.0 * r, 2.0 * r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use islet_core::activity::{BatteryDevice, DeviceKind, MediaCaps, PEEK_CONNECTED};
    use Presentation::*;

    fn centre(app: &App, t: f64) -> (f64, f64) {
        let r = app.pill_rect(t);
        (r.x + r.w / 2.0, r.y + r.h / 2.0)
    }

    fn click(app: &mut App, t: f64) {
        let (x, y) = centre(app, t);
        app.handle(Event::PointerDown { x, y, button: Button::Primary }, t);
        app.handle(Event::PointerUp { x, y, button: Button::Primary }, t + 0.05);
    }

    fn texts(app: &App, t: f64, engine: &mut TextEngine) -> Vec<String> {
        app.elements(t, engine)
            .into_iter()
            .filter_map(|e| if let Element::Text(i) = e { Some(i.text) } else { None })
            .collect()
    }

    fn song(playing: bool) -> MediaInfo {
        MediaInfo {
            source: "test".into(),
            app_name: "Player".into(),
            title: "Song".into(),
            artist: "Artist".into(),
            playing,
            position: Some(30.0),
            position_at: 0.0,
            duration: Some(180.0),
            rate: 1.0,
            caps: MediaCaps { play_pause: true, next: true, previous: true, seek: true },
            ..MediaInfo::default()
        }
    }

    fn buds() -> BatteryDevice {
        BatteryDevice {
            id: "b".into(),
            name: "Earbuds".into(),
            kind: DeviceKind::Earbuds,
            percent: 87,
            charging: false,
            parts: vec![],
        }
    }

    const NOW: LocalTime = LocalTime { year: 2026, month: 11, day: 4, weekday: 3, hour: 14, minute: 5, second: 42 };

    #[test]
    fn hover_peeks_and_leaving_restores_idle() {
        let mut app = App::new(Demo::None, false);
        let (x, y) = centre(&app, 0.0);
        assert!(app.handle(Event::PointerMove { x, y }, 0.0));
        assert_eq!(app.presentation(), Peek);
        assert!(app.handle(Event::PointerLeave, 1.0));
        assert_eq!(app.presentation(), Idle);
    }

    #[test]
    fn click_expands_and_click_again_collapses() {
        let mut app = App::new(Demo::None, false);
        click(&mut app, 0.0);
        assert_eq!(app.presentation(), Expanded);
        assert!(app.animating(0.1));
        assert!(!app.animating(5.0));
        assert!((app.pill_rect(5.0).w - 372.0).abs() < 0.01);
        click(&mut app, 5.0);
        assert_ne!(app.presentation(), Expanded);
    }

    #[test]
    fn holding_opens_large_without_a_click_on_release() {
        let mut app = App::new(Demo::None, false);
        let (x, y) = centre(&app, 0.0);
        app.handle(Event::PointerMove { x, y }, 0.0);
        app.handle(Event::PointerDown { x, y, button: Button::Primary }, 0.0);
        assert!((app.next_timer(0.0).unwrap() - LONG_PRESS_SECS).abs() < 1e-9);
        assert!(app.tick(LONG_PRESS_SECS));
        assert_eq!(app.presentation(), Large);
        app.handle(Event::PointerUp { x, y, button: Button::Primary }, 0.6);
        assert_eq!(app.presentation(), Large, "release after a long press is not a click");
    }

    #[test]
    fn open_island_auto_collapses_after_pointer_leaves() {
        let mut app = App::new(Demo::None, false);
        let (x, y) = centre(&app, 0.0);
        app.handle(Event::PointerMove { x, y }, 0.0);
        click(&mut app, 0.0);
        app.handle(Event::PointerMove { x: 1.0, y: 250.0 }, 3.0);
        assert!(!app.tick(5.0));
        assert!(app.tick(3.0 + AUTO_COLLAPSE_SECS));
        assert_eq!(app.presentation(), Idle);
    }

    #[test]
    fn escape_collapses_and_right_click_quits() {
        let mut app = App::new(Demo::None, false);
        click(&mut app, 0.0);
        app.handle(Event::Key(Key::Escape), 1.0);
        assert_eq!(app.presentation(), Idle);
        let (x, y) = centre(&app, 6.0);
        app.handle(Event::PointerDown { x, y, button: Button::Secondary }, 6.0);
        assert!(app.wants_quit());
    }

    #[test]
    fn clicks_outside_the_island_are_ignored() {
        let mut app = App::new(Demo::None, false);
        app.handle(Event::PointerDown { x: 1.0, y: 250.0, button: Button::Primary }, 0.0);
        app.handle(Event::PointerUp { x: 1.0, y: 250.0, button: Button::Primary }, 0.1);
        assert_eq!(app.presentation(), Idle);
    }

    #[test]
    fn playing_music_shows_compact_and_animates_the_waveform() {
        let mut app = App::new(Demo::None, false);
        let mut engine = TextEngine::new();
        assert!(app.report(Report::Media(Some(song(true))), 0.0));
        assert_eq!(app.presentation(), Compact);
        assert!((app.pill_rect(5.0).w - content::COMPACT_MEDIA_W).abs() < 0.01);
        let els = app.elements(5.0, &mut engine);
        assert!(els.iter().any(|e| matches!(e, Element::Art { .. })), "album art on the left");
        assert_eq!(els.iter().filter(|e| matches!(e, Element::Bar { .. })).count(), 4, "waveform on the right");
        assert_eq!(app.frame_interval(5.0), Some(WAVE_FRAME), "the waveform moves at 30 fps");

        app.report(Report::Media(Some(song(false))), 6.0);
        assert_eq!(app.frame_interval(6.5), None, "paused: nothing moves, no frames");
        app.tick(6.0 + 15.0);
        assert_eq!(app.presentation(), Idle, "a paused session leaves after the linger");
    }

    #[test]
    fn expanded_player_sends_commands_and_seeks_by_scrubbing() {
        let mut app = App::new(Demo::None, false);
        let mut engine = TextEngine::new();
        app.report(Report::Media(Some(song(true))), 0.0);
        click(&mut app, 1.0);
        assert_eq!(app.presentation(), Expanded);
        let t = 4.0;
        assert!((app.pill_rect(t).h - MEDIA_H).abs() < 0.01);
        assert!(texts(&app, t, &mut engine).contains(&"Song".to_string()));

        let controls = content::media_controls(app.pill_rect(t));
        let (play, _) = controls.iter().find(|(_, c)| *c == Control::PlayPause).unwrap();
        let (px, py) = (play.x + play.w / 2.0, play.y + play.h / 2.0);
        app.handle(Event::PointerDown { x: px, y: py, button: Button::Primary }, t);
        app.handle(Event::PointerUp { x: px, y: py, button: Button::Primary }, t + 0.05);
        assert_eq!(app.take_commands(), [MediaCommand::PlayPause]);
        assert_eq!(app.presentation(), Expanded, "pressing a control does not collapse the card");

        let (bar, _) = controls.iter().find(|(_, c)| *c == Control::Scrub).unwrap();
        let y = bar.y + bar.h / 2.0;
        app.handle(Event::PointerDown { x: bar.x, y, button: Button::Primary }, t);
        app.handle(Event::PointerMove { x: bar.x + bar.w / 2.0, y }, t + 0.1);
        app.handle(Event::PointerUp { x: bar.x + bar.w / 2.0, y, button: Button::Primary }, t + 0.2);
        match app.take_commands()[..] {
            [MediaCommand::SeekTo(s)] => assert!((s - 90.0).abs() < 0.5, "seek to the middle: {s}"),
            ref other => panic!("expected a seek, got {other:?}"),
        }
    }

    #[test]
    fn connecting_a_device_peeks_wide_then_hands_back_to_music() {
        let mut app = App::new(Demo::None, false);
        let mut engine = TextEngine::new();
        app.report(Report::Media(Some(song(true))), 0.0);
        app.report(Report::DeviceConnected(buds()), 2.0);
        // Battery peek + media = two live activities: Minimal (pill + bubble).
        assert_eq!(app.presentation(), Minimal);
        assert!(app.frame(5.0).bubble.is_some());
        assert!(
            app.elements(5.0, &mut engine).iter().any(|e| matches!(e, Element::Ring { .. })),
            "battery ring in the bubble"
        );
        app.tick(2.0 + PEEK_CONNECTED);
        assert_eq!(app.presentation(), Compact, "the peek ends, music stays");

        // With nothing else live, a connect peek is the wide compact pill.
        let mut solo = App::new(Demo::None, false);
        solo.report(Report::DeviceConnected(buds()), 0.0);
        assert_eq!(solo.presentation(), Compact);
        assert!((solo.pill_rect(3.0).w - content::COMPACT_BATTERY_W).abs() < 0.01);
        assert_eq!(texts(&solo, 3.0, &mut engine), ["Earbuds", "87%"]);
    }

    #[test]
    fn idle_island_shows_no_clock_and_never_wakes_for_it() {
        let mut app = App::new(Demo::None, false);
        assert!(!app.set_time(NOW, 0.0), "nothing to redraw while idle");
        let mut engine = TextEngine::new();
        assert!(app.elements(0.0, &mut engine).is_empty());
        assert_eq!(app.next_timer(0.0), None, "zero wakeups when idle");
        assert_eq!(app.frame_interval(0.0), None);
    }

    #[test]
    fn open_island_shows_the_clock_and_ticks_each_minute() {
        let mut app = App::new(Demo::None, false);
        app.set_time(NOW, 0.0);
        click(&mut app, 0.0);
        let mut engine = TextEngine::new();
        assert_eq!(texts(&app, 5.0, &mut engine), ["14:05", "Wednesday, 4 November"]);
        assert!((app.next_timer(5.0).unwrap() - 18.0).abs() < 1e-9, "wakes at the next minute");
        let later = LocalTime { minute: 6, second: 0, ..NOW };
        assert!(app.set_time(later, 23.0), "a new minute redraws");
        assert!(!app.set_time(LocalTime { second: 1, ..later }, 24.0), "seconds alone don't");
        app.set_clock_style(false);
        assert_eq!(&texts(&app, 25.0, &mut engine)[..2], ["2:06", "PM"]);
    }

    #[test]
    fn content_fades_out_before_the_pill_is_small() {
        let mut app = App::new(Demo::None, false);
        app.set_time(NOW, 0.0);
        click(&mut app, 0.0);
        click(&mut app, 5.0);
        let gone = (0..400).map(|i| 5.0 + i as f64 * 0.005).find(|&t| app.expanded_opacity(t) == 0.0).unwrap();
        let h = app.pill_rect(gone).h;
        assert!(h > 55.0, "content was still visible until the pill was {h} px tall");
    }

    #[test]
    fn idle_without_demo_has_no_timers_or_animation() {
        let app = App::new(Demo::None, false);
        assert!(!app.animating(0.0));
        assert_eq!(app.next_timer(0.0), None);
    }

    #[test]
    fn reduced_motion_snaps() {
        let mut app = App::new(Demo::None, true);
        click(&mut app, 0.0);
        assert!(!app.animating(0.05));
        assert_eq!(app.pill_rect(0.05).w, 372.0);
    }

    #[test]
    fn demos_drive_the_state_machine() {
        let mut toggle = App::new(Demo::Toggle, false);
        assert!(toggle.tick(1.2));
        assert_eq!(toggle.presentation(), Expanded);
        let mut tour = App::new(Demo::Tour, false);
        let mut seen = std::collections::HashSet::new();
        for i in 1..=10 {
            tour.tick(i as f64 * 1.6);
            seen.insert(tour.presentation());
        }
        for p in [Compact, Minimal, Expanded, Large, Idle, Peek] {
            assert!(seen.contains(&p), "tour never showed {p:?}");
        }
    }

    #[test]
    fn input_region_covers_both_ends_and_is_stable_while_animating() {
        let mut app = App::new(Demo::None, false);
        click(&mut app, 0.0);
        let region = app.input_region(0.06);
        assert!(region.w >= 372.0 && region.h >= CLOCK_H);
        assert_eq!(region, app.input_region(0.2));
        assert!(app.input_region(10.0).w < 380.0);
    }

    #[test]
    fn notch_mode_hides_idle_in_the_notch_and_keeps_content_beside_or_below_it() {
        let mut app = App::new(Demo::None, false);
        let mut engine = TextEngine::new();
        app.set_notch(Some((200.0, 32.0)), 0.0);
        let idle = app.pill_rect(0.0);
        assert_eq!((idle.w, idle.h), (200.0, 32.0), "idle is exactly the notch");
        assert!(app.frame(0.0).flat_top);

        app.report(Report::Media(Some(song(true))), 0.0);
        let p = app.pill_rect(5.0);
        let notch_left = (WINDOW.0 - 200.0) / 2.0;
        let art = app.elements(5.0, &mut engine).into_iter().find_map(|e| match e {
            Element::Art { x, size, .. } => Some((x as f64, size as f64)),
            _ => None,
        });
        let (ax, asize) = art.expect("album art in compact");
        assert!(ax + asize <= notch_left, "art sits left of the notch, not under it");
        assert!(p.w >= 200.0 + 2.0 * NOTCH_SIDE);

        click(&mut app, 5.0);
        let t = 10.0;
        let card = app.pill_rect(t);
        assert!((card.h - (MEDIA_H + 32.0)).abs() < 0.01, "the card grows by the notch height");
        let (r, _) = content::media_controls(app.content_rect(card))[0];
        assert!(r.y >= card.y + 32.0, "controls are below the notch");
    }

    #[test]
    fn every_state_fits_the_window_with_overshoot() {
        for p in [Idle, Peek, Compact, Minimal, Expanded, Large] {
            let (w, h, _) = target_size(p, None);
            let w = if p == Compact { content::COMPACT_BATTERY_W } else { w };
            let extra = if p == Minimal { MINIMAL_GAP + MINIMAL_CIRCLE } else { 0.0 };
            assert!((w + extra) * 1.03 <= WINDOW.0 && h * 1.03 + TOP_GAP <= WINDOW.1, "{p:?}");
        }
    }

    /// Renders a scripted session (music, a device connecting, the open
    /// player, the device list) into one PNG for visual review:
    /// `ISLET_CONTACT_SHEET=/path/out.png cargo test -p islet contact_sheet`
    #[test]
    fn contact_sheet() {
        let Some(out) = std::env::var_os("ISLET_CONTACT_SHEET") else { return };
        use islet_core::activity::ArtImage;
        use std::sync::Arc;
        use tiny_skia::{Color, Pixmap, PixmapPaint, Transform};
        // A generated gradient stands in for album art.
        let (aw, ah) = (64u32, 64u32);
        let mut rgba = Vec::with_capacity((aw * ah * 4) as usize);
        for y in 0..ah {
            for x in 0..aw {
                rgba.extend_from_slice(&[(40 + x * 3) as u8, (20 + y * 2) as u8, 160, 255]);
            }
        }
        let art = ArtImage { width: aw, height: ah, rgba: Arc::new(rgba), dominant: [120, 70, 160] };
        let mut app = App::new(Demo::None, false);
        app.set_time(NOW, 0.0);
        if std::env::var_os("ISLET_CONTACT_NOTCH").is_some() {
            app.set_notch(Some((200.0, 32.0)), 0.0);
        }
        let mut engine = TextEngine::new();
        let mut m = MediaInfo {
            art: Some(art),
            title: "Midnight Signals".into(),
            artist: "The Placeholders".into(),
            ..song(true)
        };
        m.position = Some(37.0);
        type Step = (f64, Box<dyn Fn(&mut App, f64)>);
        let script: Vec<Step> = vec![
            (
                0.2,
                Box::new(move |a: &mut App, t| {
                    a.report(Report::Media(Some(m.clone())), t);
                }),
            ),
            (
                2.0,
                Box::new(|a: &mut App, t| {
                    a.report(Report::DeviceConnected(buds()), t);
                }),
            ),
            (7.0, Box::new(|a: &mut App, t| click(a, t))),
            (10.0, Box::new(|a: &mut App, t| click(a, t))),
            (
                12.0,
                Box::new(|a: &mut App, t| {
                    a.report(
                        Report::DeviceConnected(BatteryDevice {
                            id: "m".into(),
                            name: "Wireless Mouse".into(),
                            kind: DeviceKind::Mouse,
                            percent: 15,
                            charging: false,
                            parts: vec![],
                        }),
                        t,
                    );
                }),
            ),
            (12.6, Box::new(|a: &mut App, t| click(a, t))),
        ];
        let times: Vec<f64> = (0..48).map(|i| i as f64 * 0.3).collect();
        let cols = 6usize;
        let rows = times.len().div_ceil(cols);
        let (cw, ch) = (WINDOW.0 as u32, WINDOW.1 as u32);
        let mut sheet = Pixmap::new(cw * cols as u32, ch * rows as u32).unwrap();
        sheet.fill(Color::from_rgba8(226, 230, 236, 255));
        let mut cell = Pixmap::new(cw, ch).unwrap();
        let mut next = 0;
        for (i, &t) in times.iter().enumerate() {
            while next < script.len() && script[next].0 <= t {
                (script[next].1)(&mut app, script[next].0);
                next += 1;
            }
            app.tick(t);
            let frame = app.frame(t);
            islet_render::draw(&mut cell, &frame, 1.0);
            let clip = islet_render::clip_mask(&frame, cw, ch, 1.0);
            let els = app.elements(t, &mut engine);
            islet_render::elements::draw_elements(&mut cell, &els, &mut engine, 1.0, clip.as_ref());
            let (x, y) = ((i % cols) as i32 * cw as i32, (i / cols) as i32 * ch as i32);
            sheet.draw_pixmap(x, y, cell.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        }
        sheet.save_png(out).unwrap();
    }
}
