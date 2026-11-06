//! The "gooey" neck between the pill and the detached circle (Minimal).
//!
//! Two circles (the pill's round end and the bubble) are joined by a neck
//! made of two cubic curves. Each curve leaves one circle tangentially and
//! arrives tangentially at the other, and the neck thins as the circles
//! separate, then disappears. Filled together with both shapes, this gives
//! the metaball look without any blur filter. Built from the classic 2D
//! metaball connector construction (spread angles + tangent handles).

use std::f32::consts::{FRAC_PI_2, PI};
use tiny_skia::{Path, PathBuilder};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Circle {
    pub x: f32,
    pub y: f32,
    pub r: f32,
}

/// How far the neck reaches before snapping, as a multiple of the smaller
/// radius beyond touching (design spec: snaps at gap ≈ 1.2 r).
pub const SNAP_GAP: f32 = 1.2;
/// Spread of the neck where it leaves each circle (0..1).
const SPREAD: f32 = 0.5;
const HANDLE: f32 = 2.4;

fn dir(a: f32) -> (f32, f32) {
    (a.cos(), a.sin())
}

/// The neck joining `a` and `b`, or `None` when they are too far apart or
/// one contains the other.
pub fn neck(a: Circle, b: Circle) -> Option<Path> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let d = (dx * dx + dy * dy).sqrt();
    let (r1, r2) = (a.r, b.r);
    if !(d.is_finite() && r1 > 0.0 && r2 > 0.0) {
        return None;
    }
    let max_d = r1 + r2 + SNAP_GAP * r1.min(r2);
    if d >= max_d || d <= (r1 - r2).abs() {
        return None;
    }
    // Overlap angles while the circles intersect.
    let (u1, u2) = if d < r1 + r2 {
        (
            ((r1 * r1 + d * d - r2 * r2) / (2.0 * r1 * d)).clamp(-1.0, 1.0).acos(),
            ((r2 * r2 + d * d - r1 * r1) / (2.0 * r2 * d)).clamp(-1.0, 1.0).acos(),
        )
    } else {
        (0.0, 0.0)
    };
    let between = dy.atan2(dx);
    let max_spread = ((r1 - r2) / d).clamp(-1.0, 1.0).acos();
    // 0 while touching or overlapping, 1 at the snap distance.
    let stretch = ((d - (r1 + r2)) / (max_d - (r1 + r2))).clamp(0.0, 1.0);
    // The flares where the neck meets each shape narrow as it stretches.
    let spread = SPREAD * (1.0 - 0.45 * stretch);
    let a1 = between + u1 + (max_spread - u1) * spread;
    let b1 = between - u1 - (max_spread - u1) * spread;
    let a2 = between + PI - u2 - (PI - u2 - max_spread) * spread;
    let b2 = between - PI + u2 + (PI - u2 - max_spread) * spread;

    let on = |c: Circle, ang: f32| {
        let (cx, cy) = dir(ang);
        V(c.x + c.r * cx, c.y + c.r * cy)
    };
    let (p1a, p1b, p2a, p2b) = (on(a, a1), on(a, b1), on(b, a2), on(b, b2));

    // Tangent handles at the attach points (the classic construction).
    let span = p2a.sub(p1a).len();
    let k = (SPREAD * HANDLE).min(span / (r1 + r2)) * (d * 2.0 / (r1 + r2)).min(1.0);
    let (h1, h2) = (r1 * k, r2 * k);
    let off = |p: V, ang: f32, len: f32| p.add(V::from_angle(ang).mul(len));
    let h1a = off(p1a, a1 - FRAC_PI_2, h1);
    let h1b = off(p1b, b1 + FRAC_PI_2, h1);
    let h2a = off(p2a, a2 + FRAC_PI_2, h2);
    let h2b = off(p2b, b2 - FRAC_PI_2, h2);

    // Liquid-bridge waist: each side passes through a waist point whose
    // distance from the axis shrinks to zero at the snap distance, so the
    // neck pinches like an hourglass instead of popping.
    let pinch = 1.0 - stretch * stretch * (3.0 - 2.0 * stretch); // smoothstep
    let axis = V(dx / d, dy / d);
    let origin = V(a.x, a.y);
    let waist = |p0: V, c0: V, c1: V, p1: V| {
        // Midpoint of the classic curve, pulled towards the axis.
        let m = p0.add(c0.mul(3.0)).add(c1.mul(3.0)).add(p1).mul(0.125);
        let along = m.sub(origin).dot(axis);
        let on_axis = origin.add(axis.mul(along));
        on_axis.add(m.sub(on_axis).mul(pinch))
    };
    let ma = waist(p1a, h1a, h2a, p2a);
    let mb = waist(p1b, h1b, h2b, p2b);
    let reach = span / 6.0;

    let mut pb = PathBuilder::new();
    pb.move_to(p1a.0, p1a.1);
    side(&mut pb, p1a, h1a, ma, axis, reach, h2a, p2a);
    pb.line_to(p2b.0, p2b.1);
    side(&mut pb, p2b, h2b, mb, axis.mul(-1.0), reach, h1b, p1b);
    pb.close();
    pb.finish()
}

/// One side of the neck: from `p0` (handle `c0`) through the waist `m`
/// (tangent `t`, parallel to the axis) to `p1` (handle `c1`), as two cubics.
#[allow(clippy::too_many_arguments)]
fn side(pb: &mut PathBuilder, p0: V, c0: V, m: V, t: V, reach: f32, c1: V, p1: V) {
    let c0 = p0.add(c0.sub(p0).mul(0.5));
    let c1 = p1.add(c1.sub(p1).mul(0.5));
    let (m_in, m_out) = (m.sub(t.mul(reach)), m.add(t.mul(reach)));
    pb.cubic_to(c0.0, c0.1, m_in.0, m_in.1, m.0, m.1);
    pb.cubic_to(m_out.0, m_out.1, c1.0, c1.1, p1.0, p1.1);
}

#[derive(Clone, Copy, Debug)]
struct V(f32, f32);

impl V {
    fn from_angle(a: f32) -> V {
        let (x, y) = dir(a);
        V(x, y)
    }
    fn add(self, o: V) -> V {
        V(self.0 + o.0, self.1 + o.1)
    }
    fn sub(self, o: V) -> V {
        V(self.0 - o.0, self.1 - o.1)
    }
    fn mul(self, k: f32) -> V {
        V(self.0 * k, self.1 * k)
    }
    fn dot(self, o: V) -> f32 {
        self.0 * o.0 + self.1 * o.1
    }
    fn len(self) -> f32 {
        self.dot(self).sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PILL_END: Circle = Circle { x: 100.0, y: 18.5, r: 18.5 };

    fn bubble_at(gap: f32) -> Circle {
        Circle { x: PILL_END.x + PILL_END.r + gap + 18.5, y: 18.5, r: 18.5 }
    }

    #[test]
    fn neck_exists_while_close_and_snaps_when_far() {
        assert!(neck(PILL_END, bubble_at(-10.0)).is_some(), "overlapping");
        assert!(neck(PILL_END, bubble_at(5.0)).is_some(), "just separated");
        assert!(neck(PILL_END, bubble_at(18.5 * SNAP_GAP + 0.5)).is_none(), "past the snap distance");
    }

    #[test]
    fn neck_stays_between_the_circles() {
        let b = bubble_at(4.0);
        let bounds = neck(PILL_END, b).unwrap().bounds();
        assert!(bounds.left() >= PILL_END.x - PILL_END.r && bounds.right() <= b.x + b.r);
        assert!(bounds.top() >= 0.0 - 0.01 && bounds.bottom() <= 37.0 + 0.01, "{bounds:?}");
    }

    /// Covered pixels in the column halfway between the two circle edges.
    fn waist(gap: f32) -> usize {
        use tiny_skia::{FillRule, Paint, Pixmap, Transform};
        let b = bubble_at(gap);
        let path = neck(PILL_END, b).unwrap();
        let mut pm = Pixmap::new(220, 40).unwrap();
        let mut paint = Paint::default();
        paint.set_color_rgba8(0, 0, 0, 255);
        pm.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
        let col = ((PILL_END.x + PILL_END.r + b.x - b.r) / 2.0) as u32;
        (0..pm.height()).filter(|&y| pm.pixel(col, y).is_some_and(|p| p.alpha() > 127)).count()
    }

    #[test]
    fn neck_thins_as_circles_separate() {
        let (near, mid, far) = (waist(2.0), waist(10.0), waist(20.0));
        assert!(near > mid && mid > far, "waist should shrink: {near} {mid} {far}");
        assert!(near < 37, "the waist is narrower than the pill");
    }

    #[test]
    fn contained_or_degenerate_circles_have_no_neck() {
        assert!(neck(PILL_END, Circle { x: 100.0, y: 18.5, r: 5.0 }).is_none());
        assert!(neck(PILL_END, Circle { x: f32::NAN, y: 0.0, r: 5.0 }).is_none());
        assert!(neck(PILL_END, Circle { x: 120.0, y: 18.5, r: 0.0 }).is_none());
    }
}
