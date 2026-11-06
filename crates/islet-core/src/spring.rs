//! Damped springs, parameterised the way iOS/SwiftUI describe them:
//! `response` is the period of the undamped oscillation in seconds and
//! `damping` is the damping ratio (1.0 = critically damped, < 1 overshoots).
//!
//! Values are solved analytically (no integration error, frame-rate
//! independent), and an animation can be retargeted mid-flight while
//! keeping its velocity, which is what makes interrupted iOS animations
//! feel continuous.

use std::f64::consts::TAU;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spring {
    pub response: f64,
    pub damping: f64,
}

impl Spring {
    pub const fn new(response: f64, damping: f64) -> Self {
        Self { response, damping }
    }

    fn omega(&self) -> f64 {
        TAU / self.response.max(1e-3)
    }

    /// Displacement from the target and its velocity at time `t`, given the
    /// initial displacement `y0` and initial velocity `v0`.
    pub fn solve(&self, y0: f64, v0: f64, t: f64) -> (f64, f64) {
        let w0 = self.omega();
        let z = self.damping.max(0.0);
        let t = t.max(0.0);
        if z < 1.0 {
            let wd = w0 * (1.0 - z * z).sqrt();
            let a = y0;
            let b = (v0 + z * w0 * y0) / wd;
            let e = (-z * w0 * t).exp();
            let (s, c) = (wd * t).sin_cos();
            let y = e * (a * c + b * s);
            let v = e * ((b * wd - z * w0 * a) * c - (a * wd + z * w0 * b) * s);
            (y, v)
        } else if z == 1.0 {
            let b = v0 + w0 * y0;
            let e = (-w0 * t).exp();
            let y = e * (y0 + b * t);
            let v = e * (b - w0 * (y0 + b * t));
            (y, v)
        } else {
            let root = (z * z - 1.0).sqrt();
            let r1 = -w0 * (z - root);
            let r2 = -w0 * (z + root);
            let c2 = (v0 - r1 * y0) / (r2 - r1);
            let c1 = y0 - c2;
            let (e1, e2) = ((r1 * t).exp(), (r2 * t).exp());
            (c1 * e1 + c2 * e2, c1 * r1 * e1 + c2 * r2 * e2)
        }
    }
}

/// A single animated value driven by a spring.
#[derive(Debug, Clone, Copy)]
pub struct Animated {
    spring: Spring,
    target: f64,
    /// Displacement and velocity at `t0`.
    y0: f64,
    v0: f64,
    t0: f64,
}

/// Distance below which a value counts as settled (in its own units; the
/// island animates pixels and opacities, so 1/1000 is invisible).
const SETTLE_EPS: f64 = 1e-3;

impl Animated {
    pub fn new(value: f64, spring: Spring) -> Self {
        Self { spring, target: value, y0: 0.0, v0: 0.0, t0: 0.0 }
    }

    pub fn target(&self) -> f64 {
        self.target
    }

    pub fn value(&self, t: f64) -> f64 {
        self.target + self.spring.solve(self.y0, self.v0, t - self.t0).0
    }

    pub fn velocity(&self, t: f64) -> f64 {
        self.spring.solve(self.y0, self.v0, t - self.t0).1
    }

    /// Animate towards `target` from the current position, keeping momentum.
    pub fn set_target(&mut self, target: f64, t: f64, spring: Spring) {
        let (x, v) = (self.value(t), self.velocity(t));
        self.spring = spring;
        self.target = target;
        self.y0 = x - target;
        self.v0 = v;
        self.t0 = t;
    }

    /// Jump to `value` without animating (reduced motion).
    pub fn snap(&mut self, value: f64) {
        self.target = value;
        self.y0 = 0.0;
        self.v0 = 0.0;
    }

    pub fn is_settled(&self, t: f64) -> bool {
        let (y, v) = self.spring.solve(self.y0, self.v0, t - self.t0);
        y.abs() < SETTLE_EPS && v.abs() < SETTLE_EPS * 10.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPRINGS: [Spring; 4] =
        [Spring::new(0.5, 0.3), Spring::new(0.5, 0.8), Spring::new(0.5, 1.0), Spring::new(0.5, 1.6)];

    #[test]
    fn starts_at_initial_conditions() {
        for s in SPRINGS {
            let (y, v) = s.solve(1.0, 2.0, 0.0);
            assert!((y - 1.0).abs() < 1e-9, "{s:?}");
            assert!((v - 2.0).abs() < 1e-9, "{s:?}");
        }
    }

    #[test]
    fn velocity_is_derivative_of_position() {
        for s in SPRINGS {
            // t > 0: solve() clamps negative time, so a central difference at 0 is one-sided.
            for i in 1..50 {
                let t = i as f64 * 0.02;
                let h = 1e-6;
                let numeric = (s.solve(1.0, -3.0, t + h).0 - s.solve(1.0, -3.0, t - h).0) / (2.0 * h);
                let (_, v) = s.solve(1.0, -3.0, t);
                assert!((numeric - v).abs() < 1e-4, "{s:?} t={t} numeric={numeric} v={v}");
            }
        }
    }

    #[test]
    fn settles_and_underdamped_overshoots() {
        for s in SPRINGS {
            let (y, v) = s.solve(1.0, 0.0, 5.0);
            assert!(y.abs() < 1e-3 && v.abs() < 1e-2, "{s:?} y={y} v={v}");
        }
        let overshoot = (0..200).map(|i| Spring::new(0.5, 0.8).solve(1.0, 0.0, i as f64 * 0.005).0).fold(0.0, f64::min);
        assert!(overshoot < -0.01, "an underdamped spring should cross the target");
        let crit = (0..200).map(|i| Spring::new(0.5, 1.0).solve(1.0, 0.0, i as f64 * 0.005).0).fold(1.0, f64::min);
        assert!(crit >= -1e-9, "a critically damped spring must not overshoot");
    }

    #[test]
    fn retarget_keeps_position_and_velocity() {
        let s = Spring::new(0.5, 0.8);
        let mut a = Animated::new(0.0, s);
        a.set_target(100.0, 0.0, s);
        let (x, v) = (a.value(0.1), a.velocity(0.1));
        a.set_target(-50.0, 0.1, Spring::new(0.3, 1.0));
        assert!((a.value(0.1) - x).abs() < 1e-9);
        assert!((a.velocity(0.1) - v).abs() < 1e-9);
        assert!(!a.is_settled(0.1));
        assert!(a.is_settled(5.0));
        assert!((a.value(5.0) + 50.0).abs() < 1e-3);
    }

    #[test]
    fn snap_settles_immediately() {
        let mut a = Animated::new(0.0, Spring::new(0.5, 0.8));
        a.set_target(10.0, 0.0, Spring::new(0.5, 0.8));
        a.snap(10.0);
        assert!(a.is_settled(0.0));
        assert_eq!(a.value(0.0), 10.0);
    }
}
