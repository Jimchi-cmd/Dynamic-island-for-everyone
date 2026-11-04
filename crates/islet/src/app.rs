//! Portable island logic: pointer input → presentation state machine →
//! springs → frame. No OS calls in here, so all of it is unit-tested.

use crate::platform::{Button, Event, Key};
use islet_core::clock::{format_date, format_time, secs_to_next_minute, LocalTime};
use islet_core::geometry::Rect;
use islet_core::island::{
    spring_for, target_size, Island, Presentation, Signal, AUTO_COLLAPSE_SECS, LONG_PRESS_SECS, MINIMAL_CIRCLE,
    MINIMAL_GAP,
};
use islet_core::spring::{Animated, Spring};
use islet_render::text::{Align, TextEngine, TextItem, TextStyle};
use islet_render::{Circle, PillFrame, Squircle};
use std::time::Duration;

/// Logical window size: the largest state plus spring overshoot (design spec §2).
pub const WINDOW: (f64, f64) = (420.0, 260.0);
/// Gap between the screen's top edge and the pill.
pub const TOP_GAP: f64 = 0.0;
const SMOOTHING: f32 = 0.6;

pub const FRAME: Duration = Duration::from_micros(16_667);

/// Distance from the pill's round end to the centre of the detached circle
/// in Minimal: half the circle, the gap, and the other half.
const BUBBLE_OFFSET: f64 = MINIMAL_CIRCLE / 2.0 + MINIMAL_GAP + MINIMAL_CIRCLE / 2.0;
/// Merging back is quick and bounce-free.
const BUBBLE_MERGE: Spring = Spring::new(0.32, 1.0);
/// Above this pill height the bubble is fully merged.
const BUBBLE_MAX_H: f64 = 60.0;
/// Content starts to appear above this pill height and is fully shown at the next.
const CONTENT_FROM_H: f64 = 90.0;
const CONTENT_FULL_H: f64 = 150.0;

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

pub struct App {
    island: Island,
    w: Animated,
    h: Animated,
    r: Animated,
    /// Bubble centre relative to the pill's round end: 0 = merged.
    bubble: Animated,
    hovered: bool,
    press: Option<Press>,
    /// When the pointer left an open island (auto-collapse timer).
    left_open_at: Option<f64>,
    demo: Demo,
    demo_step: usize,
    demo_next: f64,
    reduce_motion: bool,
    /// Input region while animating: start and end of the transition.
    anim_region: Rect,
    quit: bool,
    /// Latest wall-clock time (the clock activity), and 12/24-hour style.
    clock: Option<LocalTime>,
    h24: bool,
}

impl App {
    pub fn new(demo: Demo, reduce_motion: bool) -> Self {
        let (w, h, r) = target_size(Presentation::Idle, None);
        let s = Spring::new(0.5, 0.8);
        let mut app = Self {
            island: Island::new(),
            w: Animated::new(w, s),
            h: Animated::new(h, s),
            r: Animated::new(r, s),
            bubble: Animated::new(0.0, s),
            hovered: false,
            press: None,
            left_open_at: None,
            demo,
            demo_step: 0,
            demo_next: demo.period(),
            reduce_motion,
            anim_region: Rect::new(0.0, 0.0, 0.0, 0.0),
            quit: false,
            clock: None,
            h24: true,
        };
        app.anim_region = app.target_bounds();
        app
    }

    #[cfg_attr(not(test), allow(dead_code))] // used by tests; activities will use it in phase 2
    pub fn presentation(&self) -> Presentation {
        self.island.presentation()
    }

    pub fn wants_quit(&self) -> bool {
        self.quit
    }

    pub fn set_clock_style(&mut self, h24: bool) {
        self.h24 = h24;
    }

    /// Store the current wall-clock time. Returns true when the displayed
    /// minute changed while the clock is on screen (a redraw is needed).
    pub fn set_time(&mut self, now: LocalTime, t: f64) -> bool {
        let changed = self.clock.is_none_or(|c| (c.hour, c.minute, c.day) != (now.hour, now.minute, now.day));
        self.clock = Some(now);
        changed && self.content_visible(t)
    }

    /// Content fades and rises in as the pill grows past compact height,
    /// and out as it shrinks, so it never spills outside a closing island.
    pub fn content_opacity(&self, t: f64) -> f32 {
        let c = ((self.h.value(t) - CONTENT_FROM_H) / (CONTENT_FULL_H - CONTENT_FROM_H)).clamp(0.0, 1.0);
        (c * c * (3.0 - 2.0 * c)) as f32
    }

    fn content_visible(&self, t: f64) -> bool {
        self.content_opacity(t) > 0.0 || self.island.presentation().is_open()
    }

    /// The text on the island this frame. With no live activities yet, an
    /// open island shows the clock.
    pub fn text_items(&self, t: f64, engine: &mut TextEngine) -> Vec<TextItem> {
        let o = self.content_opacity(t);
        let Some(now) = self.clock.filter(|_| o > 0.0) else { return Vec::new() };
        let p = self.pill_rect(t);
        let rise = (1.0 - o) * 8.0; // content settles upward into place
        let (left, top) = (p.x as f32 + 26.0, p.y as f32 + rise);
        let (time, suffix) = format_time(&now, self.h24);
        let time_style = TextStyle { size: 52.0, weight: 600, opacity: o, tabular: true, ..TextStyle::default() };
        let mut items = vec![TextItem { text: time.clone(), style: time_style, x: left, baseline: top + 74.0 }];
        if let Some(suffix) = suffix {
            let x = left + engine.measure(&time, &time_style) + 6.0;
            let style = TextStyle { size: 20.0, ..time_style };
            items.push(TextItem { text: suffix.to_string(), style, x, baseline: top + 74.0 });
        }
        let date_style = TextStyle { size: 15.0, opacity: o * 0.6, align: Align::Left, ..TextStyle::default() };
        items.push(TextItem { text: format_date(&now), style: date_style, x: left + 2.0, baseline: top + 102.0 });
        items
    }

    /// Feed a signal to the state machine and retarget the springs.
    fn signal(&mut self, s: Signal, t: f64) -> bool {
        let from = self.island.presentation();
        let Some(to) = self.island.apply(s) else { return false };
        let (w, h, r) = target_size(to, None);
        let spring = spring_for(from, to);
        let start = self.bounds(t);
        for (a, v) in [(&mut self.w, w), (&mut self.h, h), (&mut self.r, r)] {
            if self.reduce_motion {
                a.snap(v);
            } else {
                a.set_target(v, t, spring);
            }
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
        }
        true
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

    /// Returns true when the event changed what is on screen.
    pub fn handle(&mut self, e: Event, t: f64) -> bool {
        match e {
            Event::PointerMove { x, y } => self.pointer_at(x, y, t),
            Event::PointerLeave => self.pointer_at(f64::NAN, f64::NAN, t),
            Event::PointerDown { x, y, button: Button::Primary } if self.hit(x, y, t) => {
                self.press = Some(Press { at: t, long_fired: false });
                false
            }
            Event::PointerUp { button: Button::Primary, .. } => match self.press.take() {
                Some(p) if !p.long_fired => self.signal(Signal::Click, t),
                _ => false,
            },
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
        let mut changed = false;
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
        // The clock wakes the island once a minute, and only while it is shown.
        if let Some(now) = self.clock.filter(|_| self.content_visible(t)) {
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
        }
    }
}

fn circle_rect(c: Circle) -> Rect {
    let (x, y, r) = (c.x as f64, c.y as f64, c.r as f64);
    Rect::new(x - r, y - r, 2.0 * r, 2.0 * r)
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn second_activity_splits_off_a_bubble_that_merges_back() {
        let mut app = App::new(Demo::None, false);
        app.signal(Signal::LiveActivities(2), 0.0);
        assert_eq!(app.presentation(), Minimal);
        let b = app.frame(5.0).bubble.expect("bubble visible in Minimal");
        let pill = app.pill_rect(5.0);
        let gap = (b.x - b.r) as f64 - pill.right();
        assert!((gap - MINIMAL_GAP).abs() < 0.01, "gap {gap}");
        app.signal(Signal::LiveActivities(1), 5.0);
        assert!(app.frame(5.05).bubble.is_some(), "still merging");
        assert!(app.frame(10.0).bubble.is_none(), "merged");
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
        let t = 0.06;
        let region = app.input_region(t);
        assert!(region.w >= 372.0 && region.h >= 160.0);
        assert_eq!(region, app.input_region(0.2));
        assert!(app.input_region(10.0).w < 380.0);
    }

    const NOW: LocalTime = LocalTime { year: 2026, month: 11, day: 4, weekday: 3, hour: 14, minute: 5, second: 42 };

    #[test]
    fn idle_island_shows_no_clock_and_never_wakes_for_it() {
        let mut app = App::new(Demo::None, false);
        assert!(!app.set_time(NOW, 0.0), "nothing to redraw while idle");
        let mut engine = TextEngine::new();
        assert!(app.text_items(0.0, &mut engine).is_empty());
        assert_eq!(app.next_timer(0.0), None, "zero wakeups when idle");
    }

    #[test]
    fn open_island_shows_the_clock_and_ticks_each_minute() {
        let mut app = App::new(Demo::None, false);
        app.set_time(NOW, 0.0);
        click(&mut app, 0.0);
        let mut engine = TextEngine::new();
        let items = app.text_items(5.0, &mut engine);
        let texts: Vec<&str> = items.iter().map(|i| i.text.as_str()).collect();
        assert_eq!(texts, ["14:05", "Wednesday, 4 November"]);
        assert!((items[0].style.opacity - 1.0).abs() < 1e-6);
        assert!((app.next_timer(5.0).unwrap() - 18.0).abs() < 1e-9, "wakes at the next minute");
        let later = LocalTime { minute: 6, second: 0, ..NOW };
        assert!(app.set_time(later, 23.0), "a new minute redraws");
        assert!(!app.set_time(LocalTime { second: 1, ..later }, 24.0), "seconds alone don't");

        app.set_clock_style(false);
        let texts: Vec<String> = app.text_items(25.0, &mut engine).into_iter().map(|i| i.text).collect();
        assert_eq!(&texts[..2], ["2:06", "PM"]);
    }

    #[test]
    fn content_fades_out_before_the_pill_is_small() {
        let mut app = App::new(Demo::None, false);
        app.set_time(NOW, 0.0);
        click(&mut app, 0.0);
        click(&mut app, 5.0);
        // The first moment the text is fully gone…
        let gone = (0..400).map(|i| 5.0 + i as f64 * 0.005).find(|&t| app.content_opacity(t) == 0.0).unwrap();
        // …the pill is still clearly taller than a compact pill (37 px).
        let h = app.pill_rect(gone).h;
        assert!(h > 55.0, "content was still visible until the pill was {h} px tall");
    }

    /// Renders frames from the demo tour into one PNG for visual review:
    /// `ISLET_CONTACT_SHEET=/path/out.png cargo test -p islet contact_sheet`
    #[test]
    fn contact_sheet() {
        let Some(out) = std::env::var_os("ISLET_CONTACT_SHEET") else { return };
        use tiny_skia::{Color, Pixmap, PixmapPaint, Transform};
        let mut app = App::new(Demo::Tour, false);
        app.set_time(NOW, 0.0);
        let mut engine = TextEngine::new();
        let (cols, scale) = (6usize, 1.0f32);
        // Sample 0.1 s apart through two tour steps around each transition.
        let times: Vec<f64> = (0..48).map(|i| 1.55 + (i / 6) as f64 * 1.6 + (i % 6) as f64 * 0.07).collect();
        let rows = times.len().div_ceil(cols);
        let (cw, ch) = (WINDOW.0 as u32, WINDOW.1 as u32);
        let mut sheet = Pixmap::new(cw * cols as u32, ch * rows as u32).unwrap();
        sheet.fill(Color::from_rgba8(226, 230, 236, 255));
        let mut cell = Pixmap::new(cw, ch).unwrap();
        for (i, &t) in times.iter().enumerate() {
            app.tick(t);
            let frame = app.frame(t);
            islet_render::draw(&mut cell, &frame, scale);
            let clip = islet_render::clip_mask(&frame, cell.width(), cell.height(), scale);
            for item in app.text_items(t, &mut engine) {
                engine.draw(&mut cell, &item, scale, clip.as_ref());
            }
            let (x, y) = ((i % cols) as i32 * cw as i32, (i / cols) as i32 * ch as i32);
            sheet.draw_pixmap(x, y, cell.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        }
        sheet.save_png(out).unwrap();
    }

    #[test]
    fn every_state_fits_the_window_with_overshoot() {
        for p in [Idle, Peek, Compact, Minimal, Expanded, Large] {
            let (w, h, _) = target_size(p, None);
            let extra = if p == Minimal { MINIMAL_GAP + MINIMAL_CIRCLE } else { 0.0 };
            assert!((w + extra) * 1.03 <= WINDOW.0 && h * 1.03 + TOP_GAP <= WINDOW.1, "{p:?}");
        }
    }
}
