//! Portable island logic: state, springs, input → state, frame building.
//! Phase-0 spike scope: idle ↔ expanded. Phase 1 replaces this with the full
//! state machine from docs/design-spec.md.

use crate::platform::{Button, Event, Key};
use islet_core::geometry::Rect;
use islet_core::spring::{Animated, Spring};
use islet_render::{PillFrame, Squircle};
use std::time::Duration;

/// Logical window size (contains the largest state).
pub const WINDOW: (f64, f64) = (400.0, 220.0);
/// Gap between the screen's top edge and the pill.
pub const TOP_GAP: f64 = 0.0;

const IDLE: (f64, f64, f64) = (126.0, 37.0, 18.5);
const EXPANDED: (f64, f64, f64) = (372.0, 160.0, 44.0);
const SMOOTHING: f32 = 0.6;

const EXPAND: Spring = Spring::new(0.5, 0.75);
const COLLAPSE: Spring = Spring::new(0.45, 0.9);

pub const FRAME: Duration = Duration::from_micros(16_667);
const DEMO_PERIOD: f64 = 1.2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Idle,
    Expanded,
}

pub struct App {
    state: State,
    w: Animated,
    h: Animated,
    r: Animated,
    demo_next: Option<f64>,
    reduce_motion: bool,
    /// Input region while animating: the pill where the animation started
    /// united with where it will end (computed once per state change).
    anim_region: Rect,
}

impl App {
    pub fn new(demo: bool, reduce_motion: bool) -> Self {
        let s = EXPAND;
        Self {
            state: State::Idle,
            w: Animated::new(IDLE.0, s),
            h: Animated::new(IDLE.1, s),
            r: Animated::new(IDLE.2, s),
            demo_next: demo.then_some(DEMO_PERIOD),
            reduce_motion,
            anim_region: Rect::new((WINDOW.0 - IDLE.0) / 2.0, TOP_GAP, IDLE.0, IDLE.1),
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn state(&self) -> State {
        self.state
    }

    pub fn set_state(&mut self, next: State, t: f64) {
        if next == self.state {
            return;
        }
        self.state = next;
        let from = self.pill_rect(t);
        let (dims, spring) = match next {
            State::Idle => (IDLE, COLLAPSE),
            State::Expanded => (EXPANDED, EXPAND),
        };
        for (a, v) in [(&mut self.w, dims.0), (&mut self.h, dims.1), (&mut self.r, dims.2)] {
            if self.reduce_motion {
                a.snap(v);
            } else {
                a.set_target(v, t, spring);
            }
        }
        self.anim_region = from.union(&self.target_rect());
    }

    fn target_rect(&self) -> Rect {
        Rect::new((WINDOW.0 - self.w.target()) / 2.0, TOP_GAP, self.w.target(), self.h.target())
    }

    /// Returns true when the event changed what is on screen.
    pub fn handle(&mut self, e: Event, t: f64) -> bool {
        match e {
            Event::PointerDown { x, y, button: Button::Primary } if self.pill_rect(t).contains(x, y) => {
                let next = if self.state == State::Idle { State::Expanded } else { State::Idle };
                self.set_state(next, t);
                true
            }
            Event::Key(Key::Escape) => {
                self.set_state(State::Idle, t);
                true
            }
            Event::Redraw | Event::ScaleChanged(_) => true,
            _ => false,
        }
    }

    /// Advance timers (perf demo). Returns true if the state changed.
    pub fn tick(&mut self, t: f64) -> bool {
        match self.demo_next {
            Some(next) if t >= next => {
                self.demo_next = Some(next + DEMO_PERIOD);
                let s = if self.state == State::Idle { State::Expanded } else { State::Idle };
                self.set_state(s, t);
                true
            }
            _ => false,
        }
    }

    pub fn animating(&self, t: f64) -> bool {
        !(self.w.is_settled(t) && self.h.is_settled(t) && self.r.is_settled(t))
    }

    /// Seconds from `t` until the next timer, if any.
    pub fn next_timer(&self, t: f64) -> Option<f64> {
        self.demo_next.map(|n| (n - t).max(0.0))
    }

    pub fn pill_rect(&self, t: f64) -> Rect {
        let (w, h) = (self.w.value(t).max(0.0), self.h.value(t).max(0.0));
        Rect::new((WINDOW.0 - w) / 2.0, TOP_GAP, w, h)
    }

    /// Where pointer input should be accepted. While animating it covers both
    /// ends of the animation, so it changes twice per transition, not per frame.
    pub fn input_region(&self, t: f64) -> Rect {
        let r = if self.animating(t) { self.anim_region } else { self.target_rect() };
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
            highlight: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn click_on_pill_expands_and_settles() {
        let mut app = App::new(false, false);
        let r = app.pill_rect(0.0);
        assert!(app.handle(Event::PointerDown { x: r.x + 5.0, y: r.y + 5.0, button: Button::Primary }, 0.0));
        assert_eq!(app.state(), State::Expanded);
        assert!(app.animating(0.1));
        assert!(!app.animating(5.0));
        assert!((app.pill_rect(5.0).w - EXPANDED.0).abs() < 0.01);
    }

    #[test]
    fn click_outside_pill_is_ignored() {
        let mut app = App::new(false, false);
        assert!(!app.handle(Event::PointerDown { x: 1.0, y: 200.0, button: Button::Primary }, 0.0));
        assert_eq!(app.state(), State::Idle);
    }

    #[test]
    fn escape_collapses() {
        let mut app = App::new(false, false);
        app.set_state(State::Expanded, 0.0);
        app.handle(Event::Key(Key::Escape), 1.0);
        assert_eq!(app.state(), State::Idle);
    }

    #[test]
    fn reduced_motion_snaps() {
        let mut app = App::new(false, true);
        app.set_state(State::Expanded, 0.0);
        assert!(!app.animating(0.0));
        assert_eq!(app.pill_rect(0.0).w, EXPANDED.0);
    }

    #[test]
    fn idle_without_demo_has_no_timers() {
        let app = App::new(false, false);
        assert!(!app.animating(0.0));
        assert_eq!(app.next_timer(0.0), None);
    }

    #[test]
    fn demo_toggles_periodically() {
        let mut app = App::new(true, false);
        assert!(!app.tick(1.0));
        assert!(app.tick(DEMO_PERIOD));
        assert_eq!(app.state(), State::Expanded);
        assert!(app.tick(DEMO_PERIOD * 2.0));
        assert_eq!(app.state(), State::Idle);
    }

    #[test]
    fn input_region_covers_both_ends_and_is_stable_while_animating() {
        let mut app = App::new(false, false);
        app.set_state(State::Expanded, 0.0);
        let region = app.input_region(0.01);
        assert!(region.w >= EXPANDED.0 && region.h >= EXPANDED.1);
        assert_eq!(region, app.input_region(0.2));
        app.set_state(State::Idle, 5.0);
        assert!(app.input_region(5.01).w >= EXPANDED.0, "still covers the start of the collapse");
        assert!(app.input_region(10.0).w < EXPANDED.0, "shrinks once settled");
    }

    #[test]
    fn every_state_fits_the_window() {
        for (w, h, _) in [IDLE, EXPANDED] {
            assert!(w <= WINDOW.0 && h + TOP_GAP <= WINDOW.1);
        }
    }
}
