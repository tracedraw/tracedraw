//! CNC and Laser: G-code, hot wire paths, finger joints, door panels and centre lines.
//!
//! Pure geometry over page-space outlines in millimetres, y up. The
//! application turns the results into commands
//! (`apps/tracedraw/src/signcut_cnc.rs`).
//!
//! - [`gcode`]: machines, tools, jobs by outline colour, tool paths and
//!   the G-code text.
//! - [`hotwire`]: one continuous path through closed outlines.
//! - [`fingerbox`]: the flat panels of a box with finger joints.
//! - [`joints`]: finger joints cut into straight edges.
//! - [`door`]: cabinet door and panel outlines.
//! - [`centerline`]: an approximate centre line of closed shapes.

pub mod centerline;
pub mod door;
pub mod fingerbox;
pub mod gcode;
pub mod hotwire;
pub mod joints;

use tracedraw_core::geometry::{BezPath, PathEl, Point};

/// A flattened subpath: its points and whether it closes. A closed
/// contour does not repeat its first point at the end.
#[derive(Debug, Clone, PartialEq)]
pub struct Contour {
    pub pts: Vec<Point>,
    pub closed: bool,
}

impl Contour {
    /// Length along the points, the closing side included.
    pub fn length(&self) -> f64 {
        polyline_length(&self.pts, self.closed)
    }

    /// The contour as a path.
    pub fn to_path(&self) -> BezPath {
        let mut p = BezPath::new();
        let mut it = self.pts.iter();
        if let Some(first) = it.next() {
            p.move_to(*first);
            for q in it {
                p.line_to(*q);
            }
            if self.closed {
                p.close_path();
            }
        }
        p
    }
}

pub(crate) fn finite(p: Point) -> bool {
    p.x.is_finite() && p.y.is_finite()
}

/// Flatten `path` within `tol` into contours. Points that are not
/// finite are dropped, as are repeated points; a contour needs two
/// points (three when closed).
pub fn flatten(path: &BezPath, tol: f64) -> Vec<Contour> {
    let tol = if tol.is_finite() && tol > 1e-6 {
        tol
    } else {
        0.05
    };
    let mut out = Vec::new();
    for sp in crate::subpaths(path) {
        let mut pts: Vec<Point> = Vec::new();
        tracedraw_core::geometry::flatten(&sp.path, tol, &mut |el| match el {
            PathEl::MoveTo(p) | PathEl::LineTo(p)
                if finite(p) && pts.last().is_none_or(|l| (*l - p).hypot() > 1e-9) =>
            {
                pts.push(p);
            }
            _ => {}
        });
        if sp.closed && pts.len() > 1 {
            if let (Some(a), Some(b)) = (pts.first(), pts.last()) {
                if (*a - *b).hypot() <= 1e-9 {
                    pts.pop();
                }
            }
        }
        let enough = if sp.closed { 3 } else { 2 };
        if pts.len() >= enough {
            out.push(Contour {
                pts,
                closed: sp.closed,
            });
        }
    }
    out
}

/// Signed area of a closed polygon: positive counter-clockwise (y up).
pub fn signed_area(pts: &[Point]) -> f64 {
    let n = pts.len();
    if n < 3 {
        return 0.0;
    }
    let mut a = 0.0;
    for i in 0..n {
        let p = pts[i];
        let q = pts[(i + 1) % n];
        a += p.x * q.y - q.x * p.y;
    }
    a / 2.0
}

/// Whether `p` lies inside the closed polygon `pts` (even-odd).
pub fn contains(pts: &[Point], p: Point) -> bool {
    let n = pts.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let a = pts[i];
        let b = pts[j];
        if (a.y > p.y) != (b.y > p.y) {
            let x = a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y);
            if p.x < x {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

/// Whether `p` lies inside the closed contours (even-odd over all).
pub fn contains_any(contours: &[Contour], p: Point) -> bool {
    contours
        .iter()
        .filter(|c| c.closed && contains(&c.pts, p))
        .count()
        % 2
        == 1
}

/// Length of a polyline, closed or not.
pub fn polyline_length(pts: &[Point], closed: bool) -> f64 {
    let mut l: f64 = pts.windows(2).map(|w| (w[1] - w[0]).hypot()).sum();
    if closed && pts.len() > 2 {
        if let (Some(a), Some(b)) = (pts.first(), pts.last()) {
            l += (*a - *b).hypot();
        }
    }
    l
}

/// How many other closed contours contain each closed contour (by a
/// point of it, larger ones only). Open contours get 0.
pub fn nesting_depths(contours: &[Contour]) -> Vec<usize> {
    let areas: Vec<f64> = contours.iter().map(|c| signed_area(&c.pts).abs()).collect();
    contours
        .iter()
        .enumerate()
        .map(|(i, c)| {
            if !c.closed {
                return 0;
            }
            let Some(probe) = c.pts.first().copied() else {
                return 0;
            };
            contours
                .iter()
                .enumerate()
                .filter(|(j, o)| {
                    *j != i
                        && o.closed
                        && areas.get(*j).copied().unwrap_or(0.0)
                            > areas.get(i).copied().unwrap_or(0.0)
                        && contains(&o.pts, probe)
                })
                .count()
        })
        .collect()
}

/// Douglas-Peucker simplification of an open polyline within `tol`.
pub fn simplify(pts: &[Point], tol: f64) -> Vec<Point> {
    if pts.len() < 3 || tol.is_nan() || tol <= 0.0 {
        return pts.to_vec();
    }
    let mut keep = vec![false; pts.len()];
    keep[0] = true;
    keep[pts.len() - 1] = true;
    let mut stack = vec![(0usize, pts.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        if b <= a + 1 {
            continue;
        }
        let (pa, pb) = (pts[a], pts[b]);
        let mut worst = 0.0;
        let mut at = a;
        for (k, p) in pts.iter().enumerate().take(b).skip(a + 1) {
            let d = seg_dist(*p, pa, pb);
            if d > worst {
                worst = d;
                at = k;
            }
        }
        if worst > tol {
            keep[at] = true;
            stack.push((a, at));
            stack.push((at, b));
        }
    }
    pts.iter()
        .zip(keep)
        .filter_map(|(p, k)| k.then_some(*p))
        .collect()
}

/// Distance from `p` to the segment `a`-`b`.
pub(crate) fn seg_dist(p: Point, a: Point, b: Point) -> f64 {
    (p - nearest_on_seg(p, a, b).0).hypot()
}

/// The point of segment `a`-`b` nearest to `p`, with its parameter.
pub(crate) fn nearest_on_seg(p: Point, a: Point, b: Point) -> (Point, f64) {
    let d = b - a;
    let l2 = d.hypot2();
    if l2.is_nan() || l2 <= 1e-18 {
        return (a, 0.0);
    }
    let t = ((p - a).dot(d) / l2).clamp(0.0, 1.0);
    (a + d * t, t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::{Rect, Shape as _};

    #[test]
    fn flattening_keeps_closed_and_open_subpaths() {
        let mut p = Rect::new(0.0, 0.0, 10.0, 5.0).to_path(0.01);
        p.move_to((0.0, 20.0));
        p.line_to((5.0, 20.0));
        p.move_to((f64::NAN, 1.0));
        p.line_to((f64::INFINITY, 2.0));
        let c = flatten(&p, 0.1);
        assert_eq!(c.len(), 2);
        assert!(c[0].closed && c[0].pts.len() == 4);
        assert!(!c[1].closed);
        assert!((c[0].length() - 30.0).abs() < 1e-9);
        assert!((signed_area(&c[0].pts).abs() - 50.0).abs() < 1e-9);
    }

    #[test]
    fn nesting_finds_holes_and_islands() {
        let mut p = Rect::new(0.0, 0.0, 30.0, 30.0).to_path(0.01);
        p.extend(Rect::new(5.0, 5.0, 25.0, 25.0).to_path(0.01));
        p.extend(Rect::new(10.0, 10.0, 20.0, 20.0).to_path(0.01));
        let c = flatten(&p, 0.1);
        assert_eq!(nesting_depths(&c), vec![0, 1, 2]);
        assert!(contains_any(&c, Point::new(2.0, 2.0)));
        assert!(!contains_any(&c, Point::new(7.0, 7.0)));
        assert!(contains_any(&c, Point::new(15.0, 15.0)));
    }

    #[test]
    fn simplify_drops_points_on_a_line() {
        let pts: Vec<Point> = (0..=10).map(|i| Point::new(i as f64, 0.0)).collect();
        assert_eq!(simplify(&pts, 0.1).len(), 2);
        let bent = vec![
            Point::new(0.0, 0.0),
            Point::new(5.0, 3.0),
            Point::new(10.0, 0.0),
        ];
        assert_eq!(simplify(&bent, 0.1).len(), 3);
        assert_eq!(simplify(&bent, f64::NAN).len(), 3);
        assert!(simplify(&[], 0.1).is_empty());
        assert!(!contains(&[], Point::ZERO));
        assert_eq!(signed_area(&[Point::ZERO]), 0.0);
    }
}
