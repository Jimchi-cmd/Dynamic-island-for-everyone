//! Continuous-curvature ("squircle") rounded rectangles.
//!
//! A plain rounded rectangle joins a straight edge to a circular arc, so the
//! curvature jumps from 0 to 1/R and the corner looks "pinched". iOS uses
//! continuous corners instead: each corner eases from the straight edge
//! into a shortened circular arc through a cubic transition whose first
//! three control points lie on the edge, so curvature starts at zero.
//!
//! Geometry (derived independently, after the construction Figma describes
//! publicly as "corner smoothing"): for corner radius `R` and smoothing `s`
//! in 0..1, each corner consumes `p = (1 + s) * R` of each edge; the arc
//! keeps `90 * (1 - s)` degrees of the circle and each transition covers
//! `45 * s` degrees of turning.
//!
//! When an edge is too short for the full transition (a capsule, where
//! `R = h / 2`), smoothing is reduced on that edge only, so a capsule keeps
//! a perfectly round tip but still eases in along its long edges.

use tiny_skia::{Path, PathBuilder};

/// A rounded rectangle with continuous corners, in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Squircle {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Requested corner radius; clamped to `min(w, h) / 2`.
    pub radius: f32,
    /// 0 = circular corners, 1 = maximum smoothing. iOS-like ≈ 0.6.
    pub smoothing: f32,
}

#[derive(Clone, Copy)]
struct V(f32, f32);

impl V {
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
}

/// Smoothing that fits an edge whose half-length is `budget`.
fn fit_smoothing(s: f32, radius: f32, budget: f32) -> f32 {
    if radius <= 0.0 {
        return 0.0;
    }
    s.min(budget / radius - 1.0).clamp(0.0, 1.0)
}

impl Squircle {
    pub fn effective_radius(&self) -> f32 {
        self.radius.max(0.0).min(self.w.min(self.h) / 2.0)
    }

    /// Smoothing actually used on (horizontal, vertical) edges.
    pub fn edge_smoothing(&self) -> (f32, f32) {
        let r = self.effective_radius();
        let s = self.smoothing.clamp(0.0, 1.0);
        (fit_smoothing(s, r, self.w / 2.0), fit_smoothing(s, r, self.h / 2.0))
    }

    pub fn path(&self) -> Option<Path> {
        if !(self.w > 0.0 && self.h > 0.0) || !self.x.is_finite() || !self.y.is_finite() {
            return None;
        }
        let r = self.effective_radius();
        let mut pb = PathBuilder::new();
        if r <= 0.01 {
            pb.push_rect(tiny_skia::Rect::from_xywh(self.x, self.y, self.w, self.h)?);
            return pb.finish();
        }
        let (sh, sv) = self.edge_smoothing();
        let (x0, y0, x1, y1) = (self.x, self.y, self.x + self.w, self.y + self.h);
        // Corners clockwise (y down): point, incoming direction, outgoing
        // direction, incoming smoothing, outgoing smoothing.
        let corners = [
            (V(x1, y0), V(1.0, 0.0), V(0.0, 1.0), sh, sv),
            (V(x1, y1), V(0.0, 1.0), V(-1.0, 0.0), sv, sh),
            (V(x0, y1), V(-1.0, 0.0), V(0.0, -1.0), sh, sv),
            (V(x0, y0), V(0.0, -1.0), V(1.0, 0.0), sv, sh),
        ];
        let start = V(x0 + (1.0 + sh) * r, y0);
        pb.move_to(start.0, start.1);
        for (k, u, v, s_in, s_out) in corners {
            corner(&mut pb, k, u, v, r, s_in, s_out);
        }
        pb.close();
        pb.finish()
    }
}

/// One corner: line to the start of the corner, incoming transition,
/// circular arc, outgoing transition.
fn corner(pb: &mut PathBuilder, k: V, u: V, v: V, r: f32, s_in: f32, s_out: f32) {
    // Circle centre and local frame: e1 points along the incoming edge,
    // e2 points away from the outgoing edge. P(psi) = O + e1 R sin + e2 R cos.
    let o = k.sub(u.mul(r)).add(v.mul(r));
    let (e1, e2) = (u, v.mul(-1.0));
    let p_at = |psi: f32| o.add(e1.mul(r * psi.sin())).add(e2.mul(r * psi.cos()));
    let d_at = |psi: f32| e1.mul(r * psi.cos()).sub(e2.mul(r * psi.sin()));

    let b_in = (45.0 * s_in).to_radians();
    let b_out = (45.0 * s_out).to_radians();

    // Incoming transition: E_in → S_in, first three control points on the edge.
    let e_in = k.sub(u.mul((1.0 + s_in) * r));
    let s_in_pt = p_at(b_in);
    pb.line_to(e_in.0, e_in.1);
    if s_in > 0.0 {
        let l = r * (b_in / 2.0).tan();
        let c = l * b_in.cos();
        let ab = s_in_pt.sub(e_in).dot(u) - c;
        let (b, a) = (ab / 3.0, 2.0 * ab / 3.0);
        let c1 = e_in.add(u.mul(a));
        let c2 = e_in.add(u.mul(a + b));
        pb.cubic_to(c1.0, c1.1, c2.0, c2.1, s_in_pt.0, s_in_pt.1);
    } else {
        pb.line_to(s_in_pt.0, s_in_pt.1);
    }

    // Arc from psi = b_in to psi = 90° - b_out, as cubics of at most 45°.
    let (start, end) = (b_in, std::f32::consts::FRAC_PI_2 - b_out);
    let sweep = end - start;
    if sweep > 1e-4 {
        let n = (sweep / std::f32::consts::FRAC_PI_4).ceil().max(1.0) as usize;
        let step = sweep / n as f32;
        let kappa = 4.0 / 3.0 * (step / 4.0).tan();
        for i in 0..n {
            let (a0, a1) = (start + step * i as f32, start + step * (i + 1) as f32);
            let (p0, p1) = (p_at(a0), p_at(a1));
            let c1 = p0.add(d_at(a0).mul(kappa));
            let c2 = p1.sub(d_at(a1).mul(kappa));
            pb.cubic_to(c1.0, c1.1, c2.0, c2.1, p1.0, p1.1);
        }
    }

    // Outgoing transition: S_out → E_out, last three control points on the edge.
    let s_out_pt = p_at(std::f32::consts::FRAC_PI_2 - b_out);
    let e_out = k.add(v.mul((1.0 + s_out) * r));
    if s_out > 0.0 {
        let l = r * (b_out / 2.0).tan();
        let c = l * b_out.cos();
        let ab = e_out.sub(s_out_pt).dot(v) - c;
        let (b, a) = (ab / 3.0, 2.0 * ab / 3.0);
        let c1 = e_out.sub(v.mul(a + b));
        let c2 = e_out.sub(v.mul(a));
        pb.cubic_to(c1.0, c1.1, c2.0, c2.1, e_out.0, e_out.1);
    } else {
        pb.line_to(e_out.0, e_out.1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(s: &Squircle) -> tiny_skia::Rect {
        s.path().expect("path").bounds()
    }

    #[test]
    fn path_fills_its_box_exactly() {
        for (w, h, r) in [(126.0, 37.0, 18.5), (372.0, 160.0, 44.0), (50.0, 50.0, 25.0), (200.0, 80.0, 0.0)] {
            let s = Squircle { x: 10.0, y: 5.0, w, h, radius: r, smoothing: 0.6 };
            let b = bounds(&s);
            assert!((b.left() - 10.0).abs() < 0.01, "{s:?} {b:?}");
            assert!((b.top() - 5.0).abs() < 0.01, "{s:?} {b:?}");
            assert!((b.right() - (10.0 + w)).abs() < 0.01, "{s:?} {b:?}");
            assert!((b.bottom() - (5.0 + h)).abs() < 0.01, "{s:?} {b:?}");
        }
    }

    #[test]
    fn capsule_keeps_round_tip_but_smooths_long_edge() {
        let s = Squircle { x: 0.0, y: 0.0, w: 126.0, h: 37.0, radius: 100.0, smoothing: 0.6 };
        assert_eq!(s.effective_radius(), 18.5);
        let (sh, sv) = s.edge_smoothing();
        assert!((sh - 0.6).abs() < 1e-6);
        assert_eq!(sv, 0.0);
    }

    #[test]
    fn smoothing_shrinks_when_edges_are_short() {
        let s = Squircle { x: 0.0, y: 0.0, w: 100.0, h: 100.0, radius: 40.0, smoothing: 0.6 };
        let (sh, sv) = s.edge_smoothing();
        assert!((sh - 0.25).abs() < 1e-6 && (sv - 0.25).abs() < 1e-6, "{sh} {sv}");
    }

    #[test]
    fn degenerate_inputs_do_not_panic() {
        for (w, h) in [(0.0, 10.0), (10.0, 0.0), (-5.0, 5.0), (f32::NAN, 5.0)] {
            let s = Squircle { x: 0.0, y: 0.0, w, h, radius: 4.0, smoothing: 0.6 };
            assert!(s.path().is_none());
        }
        let tiny = Squircle { x: 0.0, y: 0.0, w: 0.5, h: 0.5, radius: 10.0, smoothing: 1.0 };
        assert!(tiny.path().is_some());
    }
}
