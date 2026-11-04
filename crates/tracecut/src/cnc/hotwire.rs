//! One continuous path through closed outlines for a hot wire foam
//! cutter, which cannot lift the wire.
//!
//! The outlines are joined into a tree from the start point (Prim's
//! algorithm on the shortest straight connection between outlines, or
//! from the start point). The path goes from the start point to the
//! nearest outline, follows each outline once from where it is entered,
//! branches out along each connection where it leaves an outline and
//! comes back the same way, and ends at the start point: every
//! connection is cut twice, every outline once.

use super::{flatten, nearest_on_seg, Contour};
use tracedraw_core::geometry::{BezPath, Point};

/// Flattening tolerance of the outlines (mm).
pub const TOLERANCE: f64 = 0.05;

/// A closed outline with the arc length at each of its points.
struct Loop {
    pts: Vec<Point>,
    /// `cum[k]`: length from point 0 to point k; `cum[n]`: the whole.
    cum: Vec<f64>,
}

impl Loop {
    fn new(c: Contour) -> Option<Loop> {
        let n = c.pts.len();
        if n < 3 {
            return None;
        }
        let mut cum = Vec::with_capacity(n + 1);
        let mut s = 0.0;
        cum.push(0.0);
        for k in 0..n {
            s += (c.pts[(k + 1) % n] - c.pts[k]).hypot();
            cum.push(s);
        }
        (s > 1e-9 && s.is_finite()).then_some(Loop { pts: c.pts, cum })
    }

    fn len(&self) -> f64 {
        self.cum.last().copied().unwrap_or(0.0)
    }

    /// The point of the loop nearest to `p`: (distance, arc length, point).
    fn nearest(&self, p: Point) -> (f64, f64, Point) {
        let n = self.pts.len();
        let mut best = (f64::INFINITY, 0.0, p);
        for k in 0..n {
            let a = self.pts[k];
            let b = self.pts[(k + 1) % n];
            let (q, t) = nearest_on_seg(p, a, b);
            let d = (q - p).hypot();
            if d < best.0 {
                best = (d, self.cum[k] + t * (self.cum[k + 1] - self.cum[k]), q);
            }
        }
        best
    }

    /// Index of the segment holding arc length `s`.
    fn segment_at(&self, s: f64) -> usize {
        let n = self.pts.len();
        (0..n)
            .find(|&k| s < self.cum[k + 1])
            .unwrap_or(n.saturating_sub(1))
    }
}

/// The shortest straight connection between two loops: (distance, arc
/// length on `a`, point on `a`, arc length on `b`, point on `b`).
fn connection(a: &Loop, b: &Loop) -> (f64, f64, Point, f64, Point) {
    let mut best = (f64::INFINITY, 0.0, Point::ZERO, 0.0, Point::ZERO);
    for (k, p) in a.pts.iter().enumerate() {
        let (d, s, q) = b.nearest(*p);
        if d < best.0 {
            best = (d, a.cum[k], *p, s, q);
        }
    }
    for (k, p) in b.pts.iter().enumerate() {
        let (d, s, q) = a.nearest(*p);
        if d < best.0 {
            best = (d, s, q, b.cum[k], *p);
        }
    }
    best
}

/// Where a loop is entered: from the start point or from another loop.
#[derive(Clone, Copy)]
struct Link {
    /// The loop it leaves from (`None`: the start point).
    from: Option<usize>,
    /// Arc length on that loop and the point there.
    from_s: f64,
    from_p: Point,
    /// Arc length on this loop and the entry point.
    entry_s: f64,
    entry_p: Point,
}

/// The hot wire path through the closed outlines of `outlines` from
/// `start`, as points of one open polyline that starts and ends at
/// `start`. Open subpaths are ignored. `None` without closed outlines.
pub fn wire_path(outlines: &[BezPath], start: Point) -> Option<Vec<Point>> {
    if !(start.x.is_finite() && start.y.is_finite()) {
        return None;
    }
    let loops: Vec<Loop> = outlines
        .iter()
        .flat_map(|p| flatten(p, TOLERANCE))
        .filter(|c| c.closed)
        .filter_map(Loop::new)
        .collect();
    if loops.is_empty() {
        return None;
    }
    let n = loops.len();
    // Best known link of each loop not yet joined, with its length.
    let mut best: Vec<(f64, Link)> = loops
        .iter()
        .map(|l| {
            let (d, s, q) = l.nearest(start);
            (
                d,
                Link {
                    from: None,
                    from_s: 0.0,
                    from_p: start,
                    entry_s: s,
                    entry_p: q,
                },
            )
        })
        .collect();
    let mut joined: Vec<Option<Link>> = vec![None; n];
    let mut order: Vec<usize> = Vec::with_capacity(n);
    for _ in 0..n {
        let Some(i) = (0..n)
            .filter(|&i| joined[i].is_none())
            .min_by(|&a, &b| best[a].0.total_cmp(&best[b].0))
        else {
            break;
        };
        joined[i] = Some(best[i].1);
        order.push(i);
        for k in 0..n {
            if joined[k].is_some() {
                continue;
            }
            let (d, si, pi, sk, pk) = connection(&loops[i], &loops[k]);
            if d < best[k].0 {
                best[k] = (
                    d,
                    Link {
                        from: Some(i),
                        from_s: si,
                        from_p: pi,
                        entry_s: sk,
                        entry_p: pk,
                    },
                );
            }
        }
    }
    // Children of each loop and of the start point, in joining order.
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut roots = Vec::new();
    for &i in &order {
        match joined[i].and_then(|l| l.from) {
            Some(p) => children[p].push(i),
            None => roots.push(i),
        }
    }
    let mut out = vec![start];
    for r in roots {
        let Some(link) = joined[r] else {
            continue;
        };
        push(&mut out, link.entry_p);
        visit(r, &loops, &joined, &children, &mut out);
        push(&mut out, start);
    }
    (out.len() > 1).then_some(out)
}

fn push(out: &mut Vec<Point>, p: Point) {
    if out.last().is_none_or(|l| (*l - p).hypot() > 1e-9) {
        out.push(p);
    }
}

/// An event met while following a loop: one of its points, or the
/// connection to a child loop.
enum Ev {
    Point(Point),
    Child(usize),
}

/// Follow loop `j` once from its entry point (the last point of `out`)
/// round to it again, branching into its children on the way.
fn visit(
    j: usize,
    loops: &[Loop],
    joined: &[Option<Link>],
    children: &[Vec<usize>],
    out: &mut Vec<Point>,
) {
    let (Some(l), Some(link)) = (loops.get(j), joined.get(j).copied().flatten()) else {
        return;
    };
    let len = l.len();
    let n = l.pts.len();
    let rel = |s: f64| {
        let r = (s - link.entry_s).rem_euclid(len);
        if r.is_finite() {
            r
        } else {
            0.0
        }
    };
    // Points of the loop after the entry, and the children's links, by
    // their distance along the loop from the entry.
    let e = l.segment_at(link.entry_s);
    let mut evs: Vec<(f64, Ev)> = (1..=n)
        .map(|i| {
            let k = (e + i) % n;
            let mut r = rel(l.cum[k]);
            if r < 1e-12 {
                r = len;
            }
            (r, Ev::Point(l.pts[k]))
        })
        .collect();
    for &c in children.get(j).map(|v| v.as_slice()).unwrap_or(&[]) {
        if let Some(cl) = joined.get(c).copied().flatten() {
            evs.push((rel(cl.from_s), Ev::Child(c)));
        }
    }
    evs.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (_, ev) in evs {
        match ev {
            Ev::Point(p) => push(out, p),
            Ev::Child(c) => {
                let Some(cl) = joined.get(c).copied().flatten() else {
                    continue;
                };
                push(out, cl.from_p);
                push(out, cl.entry_p);
                visit(c, loops, joined, children, out);
                push(out, cl.from_p);
            }
        }
    }
    push(out, link.entry_p);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::{Rect, Shape as _};

    fn square(x: f64, y: f64, s: f64) -> BezPath {
        Rect::new(x, y, x + s, y + s).to_path(0.01)
    }

    fn length(pts: &[Point]) -> f64 {
        super::super::polyline_length(pts, false)
    }

    #[test]
    fn one_square_from_a_start_point() {
        let p = wire_path(&[square(10.0, 0.0, 10.0)], Point::new(0.0, 5.0)).expect("path");
        // In to the nearest point, round once, out again.
        assert_eq!(p.first(), Some(&Point::new(0.0, 5.0)));
        assert_eq!(p.last(), Some(&Point::new(0.0, 5.0)));
        assert_eq!(p[1], Point::new(10.0, 5.0));
        assert!((length(&p) - (10.0 + 40.0 + 10.0)).abs() < 1e-9);
    }

    #[test]
    fn outlines_are_joined_by_their_shortest_connections_cut_twice() {
        let shapes = [
            square(0.0, 0.0, 10.0),
            square(15.0, 0.0, 10.0),
            square(30.0, 0.0, 10.0),
        ];
        let p = wire_path(&shapes, Point::new(-5.0, 0.0)).expect("path");
        // Each outline once (3 x 40), each connection twice (5 mm each
        // between the squares, 5 mm from the start).
        assert!((length(&p) - (120.0 + 2.0 * (5.0 + 5.0 + 5.0))).abs() < 1e-9);
        assert_eq!(p.first(), p.last());
    }

    #[test]
    fn a_hole_is_entered_from_its_outline() {
        let mut ring = square(0.0, 0.0, 30.0);
        ring.extend(square(10.0, 10.0, 10.0));
        let p = wire_path(&[ring], Point::new(-10.0, 0.0)).expect("path");
        assert!((length(&p) - (120.0 + 40.0 + 2.0 * 10.0 + 2.0 * 10.0)).abs() < 1e-9);
    }

    #[test]
    fn nothing_closed_gives_nothing() {
        let mut line = BezPath::new();
        line.move_to((0.0, 0.0));
        line.line_to((10.0, 0.0));
        assert!(wire_path(&[line], Point::ZERO).is_none());
        assert!(wire_path(&[], Point::ZERO).is_none());
        assert!(wire_path(&[square(0.0, 0.0, 5.0)], Point::new(f64::NAN, 0.0)).is_none());
        let mut bad = BezPath::new();
        bad.move_to((f64::NAN, 0.0));
        bad.line_to((1.0, f64::INFINITY));
        bad.line_to((2.0, 2.0));
        bad.close_path();
        assert!(wire_path(&[bad], Point::ZERO).is_none());
    }
}
