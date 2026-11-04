//! Finger joints cut into straight edges of closed outlines, so two
//! flat parts interlock at a corner.
//!
//! An edge of length L is split into an odd number n of equal fingers
//! (L / n nearest to the wanted width); every other finger is cut in
//! by the depth (the material thickness of the mating part), square to
//! the edge and into the material. Unflipped, the first and last
//! fingers stay (the slots are the odd ones); flipped, they are cut
//! (the slots are the even ones), which is the mating pattern. Only
//! straight segments take fingers; curved ones are kept as they are.

use super::{flatten, nesting_depths, signed_area};
use tracedraw_core::geometry::{BezPath, ParamCurve, PathSeg, Point, Vec2};

/// How to cut the fingers.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct JointSpec {
    /// Wanted finger width (mm).
    pub finger: f64,
    /// Depth of the slots: the thickness of the mating part (mm).
    pub depth: f64,
    /// Edges shorter than this are left alone (mm).
    pub min_length: f64,
    /// Cut the end fingers (the mating pattern).
    pub flip: bool,
}

impl Default for JointSpec {
    fn default() -> Self {
        JointSpec {
            finger: 10.0,
            depth: 3.0,
            min_length: 30.0,
            flip: false,
        }
    }
}

impl JointSpec {
    fn valid(&self) -> bool {
        self.finger.is_finite() && self.finger > 0.0 && self.depth.is_finite() && self.depth > 0.0
    }
}

/// A straight edge of a closed subpath.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edge {
    /// Subpath number (closed ones counted with the open ones).
    pub sub: usize,
    /// Segment number within the subpath.
    pub seg: usize,
    pub a: Point,
    pub b: Point,
    /// Unit vector square to the edge, into the material.
    pub inward: Vec2,
}

impl Edge {
    pub fn length(&self) -> f64 {
        (self.b - self.a).hypot()
    }

    pub fn mid(&self) -> Point {
        self.a.midpoint(self.b)
    }
}

/// An odd finger count for an edge of length `len`, the width nearest
/// to `finger`.
pub fn count(len: f64, finger: f64) -> usize {
    if !(len.is_finite() && finger.is_finite()) || len <= 0.0 || finger <= 0.0 {
        return 1;
    }
    let n = (len / finger).round().clamp(1.0, 999.0) as usize;
    if n % 2 == 1 {
        return n;
    }
    let err = |k: usize| (len / k as f64 - finger).abs();
    if n > 1 && err(n - 1) <= err(n + 1) {
        n - 1
    } else {
        n + 1
    }
}

fn is_straight(seg: &PathSeg) -> Option<(Point, Point)> {
    let line_dist = |p: Point, a: Point, b: Point| super::seg_dist(p, a, b);
    match *seg {
        PathSeg::Line(l) => Some((l.p0, l.p1)),
        PathSeg::Quad(q) => (line_dist(q.p1, q.p0, q.p2) < 1e-6).then_some((q.p0, q.p2)),
        PathSeg::Cubic(c) => (line_dist(c.p1, c.p0, c.p3) < 1e-6
            && line_dist(c.p2, c.p0, c.p3) < 1e-6)
            .then_some((c.p0, c.p3)),
    }
}

fn finite(p: Point) -> bool {
    p.x.is_finite() && p.y.is_finite()
}

/// The straight edges of the closed subpaths of `path` at least
/// `min_length` long, with the side their material lies on (even-odd:
/// a hole has its material outside).
pub fn straight_edges(path: &BezPath, min_length: f64) -> Vec<Edge> {
    let subs = crate::subpaths(path);
    let contours: Vec<_> = subs
        .iter()
        .map(|s| {
            flatten(&s.path, 0.05)
                .into_iter()
                .next()
                .map(|mut c| {
                    c.closed = s.closed;
                    c
                })
                .unwrap_or(super::Contour {
                    pts: Vec::new(),
                    closed: false,
                })
        })
        .collect();
    let depths = nesting_depths(&contours);
    let mut out = Vec::new();
    for (i, s) in subs.iter().enumerate() {
        if !s.closed {
            continue;
        }
        let Some(c) = contours.get(i) else {
            continue;
        };
        let ccw = signed_area(&c.pts) > 0.0;
        let hole = depths.get(i).copied().unwrap_or(0) % 2 == 1;
        let material_left = ccw != hole;
        for (k, seg) in s.path.segments().enumerate() {
            let Some((a, b)) = is_straight(&seg) else {
                continue;
            };
            let d = b - a;
            let len = d.hypot();
            if !(finite(a) && finite(b)) || !len.is_finite() || len < min_length.max(1e-9) {
                continue;
            }
            let left = Vec2::new(-d.y, d.x) / len;
            out.push(Edge {
                sub: i,
                seg: k,
                a,
                b,
                inward: if material_left { left } else { -left },
            });
        }
    }
    out
}

/// The points of a finger profile from `e.a` to `e.b` with `n` fingers.
fn profile(e: &Edge, n: usize, depth: f64, flip: bool) -> Vec<Point> {
    let n = n.max(1);
    let d = e.b - e.a;
    let mut pts = vec![e.a];
    for k in 0..n {
        let slot = (k % 2 == 1) != flip;
        let off = if slot { e.inward * depth } else { Vec2::ZERO };
        let s0 = k as f64 / n as f64;
        let s1 = (k + 1) as f64 / n as f64;
        pts.push(e.a + d * s0 + off);
        pts.push(e.a + d * s1 + off);
    }
    pts.push(e.b);
    let mut v: Vec<Point> = Vec::with_capacity(pts.len());
    for p in pts {
        if v.last().is_none_or(|l| (*l - p).hypot() > 1e-9) {
            v.push(p);
        }
    }
    v
}

/// `path` with fingers cut into the given edges: (edge, finger count,
/// flipped). Other segments are kept as they are.
pub fn cut(path: &BezPath, edges: &[(Edge, usize, bool)], depth: f64) -> BezPath {
    let mut out = BezPath::new();
    for (i, s) in crate::subpaths(path).iter().enumerate() {
        let mut started = false;
        for (k, seg) in s.path.segments().enumerate() {
            let start = seg.start();
            if !started {
                out.move_to(start);
                started = true;
            }
            match edges.iter().find(|(e, _, _)| e.sub == i && e.seg == k) {
                Some((e, n, flip)) => {
                    for p in profile(e, *n, depth, *flip).into_iter().skip(1) {
                        out.line_to(p);
                    }
                }
                None => match seg {
                    PathSeg::Line(l) => out.line_to(l.p1),
                    PathSeg::Quad(q) => out.quad_to(q.p1, q.p2),
                    PathSeg::Cubic(c) => out.curve_to(c.p1, c.p2, c.p3),
                },
            }
        }
        if started && s.closed {
            out.close_path();
        }
    }
    out
}

/// Fingers on every straight edge of `path` at least the minimum
/// length long. Returns the new path and how many edges took fingers;
/// `None` when none did.
pub fn on_all_edges(path: &BezPath, spec: &JointSpec) -> Option<(BezPath, usize)> {
    if !spec.valid() {
        return None;
    }
    let edges: Vec<(Edge, usize, bool)> = straight_edges(path, spec.min_length)
        .into_iter()
        .map(|e| (e, count(e.length(), spec.finger), spec.flip))
        .collect();
    if edges.is_empty() {
        return None;
    }
    Some((cut(path, &edges, spec.depth), edges.len()))
}

/// Join two parts: the straight edges (at least the minimum length) of
/// `a` and `b` whose middles lie nearest each other take matching
/// fingers, the same count on both (from the shorter edge), `b` with the
/// pattern of `a` flipped. Returns both new paths and the two edges.
pub fn between(
    a: &BezPath,
    b: &BezPath,
    spec: &JointSpec,
) -> Option<(BezPath, BezPath, Edge, Edge)> {
    if !spec.valid() {
        return None;
    }
    let ea = straight_edges(a, spec.min_length);
    let eb = straight_edges(b, spec.min_length);
    let mut best: Option<(f64, Edge, Edge)> = None;
    for x in &ea {
        for y in &eb {
            let d = (x.mid() - y.mid()).hypot();
            if best.is_none_or(|(bd, _, _)| d < bd) {
                best = Some((d, *x, *y));
            }
        }
    }
    let (_, x, y) = best?;
    let n = count(x.length().min(y.length()), spec.finger);
    Some((
        cut(a, &[(x, n, spec.flip)], spec.depth),
        cut(b, &[(y, n, !spec.flip)], spec.depth),
        x,
        y,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::{Rect, Shape as _};

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> BezPath {
        Rect::new(x0, y0, x1, y1).to_path(0.01)
    }

    fn area(p: &BezPath) -> f64 {
        flatten(p, 0.01)
            .iter()
            .map(|c| signed_area(&c.pts))
            .sum::<f64>()
            .abs()
    }

    #[test]
    fn counts_are_odd() {
        assert_eq!(count(90.0, 10.0), 9);
        assert_eq!(count(100.0, 10.0), 11);
        assert_eq!(count(96.0, 12.0), 9);
        assert_eq!(count(110.0, 10.0), 11);
        assert_eq!(count(5.0, 10.0), 1);
        assert_eq!(count(f64::NAN, 10.0), 1);
    }

    #[test]
    fn long_edges_only_with_material_inside() {
        let e = straight_edges(&rect(0.0, 0.0, 100.0, 20.0), 30.0);
        assert_eq!(e.len(), 2);
        // The bottom edge's material is above it.
        assert_eq!(e[0].a, Point::new(0.0, 0.0));
        assert!((e[0].inward.y - 1.0).abs() < 1e-12);
        // A hole has its material outside.
        let mut ring = rect(0.0, 0.0, 100.0, 100.0);
        ring.extend(rect(20.0, 20.0, 80.0, 80.0));
        let e = straight_edges(&ring, 30.0);
        assert_eq!(e.len(), 8);
        let hole_bottom = e
            .iter()
            .find(|e| e.sub == 1 && (e.a.y - 20.0).abs() < 1e-9 && (e.b.y - 20.0).abs() < 1e-9)
            .expect("hole edge");
        assert!((hole_bottom.inward.y + 1.0).abs() < 1e-12);
    }

    #[test]
    fn fingers_take_out_every_other_strip() {
        let spec = JointSpec {
            finger: 10.0,
            depth: 3.0,
            min_length: 50.0,
            flip: false,
        };
        let (p, n) = on_all_edges(&rect(0.0, 0.0, 90.0, 20.0), &spec).expect("fingers");
        assert_eq!(n, 2);
        // 9 fingers of 10 on each long side, 4 slots of 10 x 3 each.
        assert!((area(&p) - (1800.0 - 2.0 * 4.0 * 30.0)).abs() < 1e-6);
        let b = p.bounding_box();
        assert!((b.width() - 90.0).abs() < 1e-9 && (b.height() - 20.0).abs() < 1e-9);
        // Flipped: 5 slots per side, the corners notched.
        let f = JointSpec { flip: true, ..spec };
        let (p, _) = on_all_edges(&rect(0.0, 0.0, 90.0, 20.0), &f).expect("fingers");
        assert!((area(&p) - (1800.0 - 2.0 * 5.0 * 30.0)).abs() < 1e-6);
    }

    #[test]
    fn two_parts_take_mating_patterns_on_their_nearest_edges() {
        let spec = JointSpec::default();
        let a = rect(0.0, 0.0, 100.0, 50.0);
        let b = rect(0.0, 60.0, 100.0, 110.0);
        let (pa, pb, ea, eb) = between(&a, &b, &spec).expect("joint");
        // A's top and B's bottom.
        assert!((ea.mid().y - 50.0).abs() < 1e-9);
        assert!((eb.mid().y - 60.0).abs() < 1e-9);
        // 11 fingers of 100 / 11: A loses 5 slots, B 6.
        assert!((area(&pa) - (5000.0 - 5.0 * 100.0 / 11.0 * 3.0)).abs() < 1e-6);
        assert!((area(&pb) - (5000.0 - 6.0 * 100.0 / 11.0 * 3.0)).abs() < 1e-6);
    }

    #[test]
    fn curves_stay_and_bad_input_does_nothing() {
        let spec = JointSpec::default();
        let circle = tracedraw_core::geometry::Circle::new((0.0, 0.0), 50.0).to_path(0.01);
        assert!(on_all_edges(&circle, &spec).is_none());
        let bad = JointSpec {
            depth: f64::NAN,
            ..spec
        };
        assert!(on_all_edges(&rect(0.0, 0.0, 100.0, 100.0), &bad).is_none());
        let mut p = BezPath::new();
        p.move_to((f64::NAN, 0.0));
        p.line_to((100.0, 0.0));
        p.line_to((100.0, f64::INFINITY));
        p.close_path();
        assert!(on_all_edges(&p, &spec).is_none());
        assert!(between(&circle, &rect(0.0, 0.0, 100.0, 100.0), &spec).is_none());
        // A half-round part keeps its arc, the flat side takes fingers.
        let mut d = BezPath::new();
        d.move_to((0.0, 0.0));
        d.line_to((100.0, 0.0));
        d.curve_to((100.0, 60.0), (0.0, 60.0), (0.0, 0.0));
        d.close_path();
        let (q, n) = on_all_edges(&d, &spec).expect("fingers");
        assert_eq!(n, 1);
        assert!(q
            .elements()
            .iter()
            .any(|e| matches!(e, tracedraw_core::geometry::PathEl::CurveTo(..))));
    }
}
