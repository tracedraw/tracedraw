//! Sign Making: LED modules, neon tubes, sign boxes, letter returns, tiling and carriers.
//!
//! Pure geometry over page-space outlines in millimetres, y up. The
//! application turns the results into commands
//! (`apps/tracedraw/src/signcut_sign.rs`).
//!
//! - [`led`]: LED modules laid out in closed shapes, power and price.
//! - [`neon`]: neon tubes along centre lines, split to a maximum length.
//! - [`sign_box`]: the flat pattern of a folded sign box.
//! - [`returns`]: letter return strips and liquid acrylic moulds.
//! - [`tiles`]: dividing large objects into overlapping tiles.
//! - [`carrier`]: one backing board outline around letters.

pub mod carrier;
pub mod led;
pub mod neon;
pub mod returns;
pub mod sign_box;
pub mod tiles;

use tracedraw_core::geometry::{self as geo, BezPath, PathEl, Point, Rect, Shape as _};

/// Tolerance of the polylines the tools work on, in millimetres.
pub const FLAT_TOL: f64 = 0.05;

/// Every coordinate of the path is a finite number.
pub fn finite_path(path: &BezPath) -> bool {
    path.elements().iter().all(|el| match *el {
        PathEl::MoveTo(p) | PathEl::LineTo(p) => finite_pt(p),
        PathEl::QuadTo(a, p) => finite_pt(a) && finite_pt(p),
        PathEl::CurveTo(a, b, p) => finite_pt(a) && finite_pt(b) && finite_pt(p),
        PathEl::ClosePath => true,
    })
}

pub(crate) fn finite_pt(p: Point) -> bool {
    p.x.is_finite() && p.y.is_finite()
}

/// The closed subpaths of an object's outlines as one path (each one
/// closed), the region they enclose by the even-odd rule. Open subpaths
/// and paths with non-finite coordinates are left out.
pub fn region(paths: &[BezPath]) -> BezPath {
    let mut out = BezPath::new();
    for p in paths.iter().filter(|p| finite_path(p)) {
        for s in crate::subpaths(p) {
            if !s.closed {
                continue;
            }
            for el in s.path.elements() {
                if !matches!(el, PathEl::ClosePath) {
                    out.push(*el);
                }
            }
            out.close_path();
        }
    }
    out
}

/// A flattened subpath.
#[derive(Debug, Clone)]
pub(crate) struct Poly {
    pub pts: Vec<Point>,
    pub closed: bool,
}

/// Flatten every subpath of `path` within `tol`. Closed ones do not
/// repeat their first point at the end.
pub(crate) fn polylines(path: &BezPath, tol: f64) -> Vec<Poly> {
    let mut out = Vec::new();
    if !finite_path(path) {
        return out;
    }
    for s in crate::subpaths(path) {
        let mut pts: Vec<Point> = Vec::new();
        geo::flatten(&s.path, tol, &mut |el| {
            if let PathEl::MoveTo(p) | PathEl::LineTo(p) = el {
                if pts.last().is_none_or(|q| (*q - p).hypot() > 1e-9) {
                    pts.push(p);
                }
            }
        });
        if s.closed && pts.len() > 1 {
            if let (Some(a), Some(b)) = (pts.first(), pts.last()) {
                if (*a - *b).hypot() < 1e-9 {
                    pts.pop();
                }
            }
        }
        let enough = if s.closed {
            pts.len() >= 3
        } else {
            pts.len() >= 2
        };
        if enough {
            out.push(Poly {
                pts,
                closed: s.closed,
            });
        }
    }
    out
}

/// The closed polygons of a region path (see [`region`]), flattened.
pub(crate) fn region_polys(path: &BezPath, tol: f64) -> Vec<Vec<Point>> {
    polylines(path, tol)
        .into_iter()
        .filter(|p| p.closed)
        .map(|p| p.pts)
        .collect()
}

/// The edges of closed polygons.
pub(crate) fn edges(polys: &[Vec<Point>]) -> impl Iterator<Item = (Point, Point)> + '_ {
    polys.iter().flat_map(|poly| {
        let n = poly.len();
        (0..n).filter_map(move |i| Some((*poly.get(i)?, *poly.get((i + 1) % n)?)))
    })
}

/// `p` lies inside the polygons by the even-odd rule.
pub(crate) fn inside(polys: &[Vec<Point>], p: Point) -> bool {
    let mut odd = false;
    for (a, b) in edges(polys) {
        if (a.y > p.y) != (b.y > p.y) {
            let x = a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x);
            if p.x < x {
                odd = !odd;
            }
        }
    }
    odd
}

/// Distance from `p` to the nearest point of the line from `a` to `b`.
pub(crate) fn seg_dist(p: Point, a: Point, b: Point) -> f64 {
    let ab = b - a;
    let len2 = ab.hypot2();
    if len2 < 1e-18 {
        return (p - a).hypot();
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    (p - (a + ab * t)).hypot()
}

/// Distance from `p` to the nearest edge of the polygons.
pub(crate) fn edge_dist(polys: &[Vec<Point>], p: Point) -> f64 {
    edges(polys)
        .map(|(a, b)| seg_dist(p, a, b))
        .fold(f64::INFINITY, f64::min)
}

/// Which closed subpaths of a path are holes: those inside an odd
/// number of larger ones (even-odd, as the measuring tools count).
pub(crate) fn hole_flags(closed: &[BezPath]) -> Vec<bool> {
    let probes: Vec<(f64, Point)> = closed
        .iter()
        .map(|p| {
            use tracedraw_core::geometry::ParamCurve;
            let probe = p
                .segments()
                .next()
                .map(|s| s.eval(0.5))
                .unwrap_or(Point::ZERO);
            (p.area().abs(), probe)
        })
        .collect();
    probes
        .iter()
        .enumerate()
        .map(|(i, (area, probe))| {
            let depth = closed
                .iter()
                .zip(&probes)
                .enumerate()
                .filter(|(j, (other, (oa, _)))| {
                    *j != i && *oa > *area && other.winding(*probe) != 0
                })
                .count();
            depth % 2 == 1
        })
        .collect()
}

/// Keep only the outer outlines of a region (no holes, nothing inside a
/// hole).
pub fn outer_contours(path: &BezPath) -> BezPath {
    let closed: Vec<BezPath> = crate::subpaths(path)
        .into_iter()
        .filter(|s| s.closed)
        .map(|s| {
            let mut p = s.path;
            if !matches!(p.elements().last(), Some(PathEl::ClosePath)) {
                p.close_path();
            }
            p
        })
        .collect();
    let mut out = BezPath::new();
    // An island inside a hole is not outer either: only depth 0 stays.
    for (i, p) in closed.iter().enumerate() {
        use tracedraw_core::geometry::ParamCurve;
        let probe = p
            .segments()
            .next()
            .map(|s| s.eval(0.5))
            .unwrap_or(Point::ZERO);
        let area = p.area().abs();
        let inside_other = closed
            .iter()
            .enumerate()
            .any(|(j, o)| j != i && o.area().abs() > area && o.winding(probe) != 0);
        if !inside_other {
            out.extend(p.elements().iter().copied());
        }
    }
    out
}

/// Bounding box of points, when there are any finite ones.
pub(crate) fn points_bbox(pts: impl IntoIterator<Item = Point>) -> Option<Rect> {
    let mut r: Option<Rect> = None;
    for p in pts.into_iter().filter(|p| finite_pt(*p)) {
        r = Some(match r {
            Some(r) => r.union_pt(p),
            None => Rect::from_points(p, p),
        });
    }
    r
}

/// The closed subpaths of a path, each a path of its own ending with a
/// close (paths with non-finite coordinates give none).
pub fn closed_subpaths(path: &BezPath) -> Vec<BezPath> {
    if !finite_path(path) {
        return Vec::new();
    }
    crate::subpaths(path)
        .into_iter()
        .filter(|s| s.closed)
        .map(|s| {
            let mut p = s.path;
            if !matches!(p.elements().last(), Some(PathEl::ClosePath)) {
                p.close_path();
            }
            p
        })
        .collect()
}

/// A circle path.
pub fn circle(center: Point, diameter: f64) -> BezPath {
    tracedraw_core::geometry::Circle::new(center, diameter.abs() / 2.0).to_path(0.01)
}

/// One value for each side of a rectangle (millimetres).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Sides {
    pub left: f64,
    pub right: f64,
    pub bottom: f64,
    pub top: f64,
}

impl Default for Sides {
    fn default() -> Self {
        Sides::all(0.0)
    }
}

impl Sides {
    /// The same value on every side.
    pub fn all(v: f64) -> Self {
        Sides {
            left: v,
            right: v,
            bottom: v,
            top: v,
        }
    }

    /// Every value finite and at least zero (others become zero).
    pub fn clean(self) -> Self {
        let c = |v: f64| if v.is_finite() { v.max(0.0) } else { 0.0 };
        Sides {
            left: c(self.left),
            right: c(self.right),
            bottom: c(self.bottom),
            top: c(self.top),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::Rect;

    fn square(x: f64, y: f64, s: f64) -> BezPath {
        Rect::new(x, y, x + s, y + s).to_path(0.01)
    }

    #[test]
    fn region_keeps_closed_subpaths_only() {
        let mut p = square(0.0, 0.0, 10.0);
        p.move_to((20.0, 0.0));
        p.line_to((30.0, 0.0));
        let r = region(&[p]);
        assert_eq!(region_polys(&r, FLAT_TOL).len(), 1);
        let mut bad = BezPath::new();
        bad.move_to((f64::NAN, 0.0));
        bad.line_to((1.0, 1.0));
        bad.line_to((0.0, 1.0));
        bad.close_path();
        assert!(region(&[bad]).elements().is_empty());
    }

    #[test]
    fn inside_and_distance_follow_even_odd() {
        let mut p = square(0.0, 0.0, 30.0);
        p.extend(square(10.0, 10.0, 10.0));
        let polys = region_polys(&p, FLAT_TOL);
        assert!(inside(&polys, Point::new(5.0, 5.0)));
        assert!(!inside(&polys, Point::new(15.0, 15.0)));
        assert!(!inside(&polys, Point::new(-1.0, 5.0)));
        assert!((edge_dist(&polys, Point::new(5.0, 15.0)) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn closed_subpaths_and_sides() {
        let mut p = square(0.0, 0.0, 10.0);
        p.move_to((20.0, 0.0));
        p.line_to((30.0, 0.0));
        p.extend(square(40.0, 0.0, 5.0));
        let c = closed_subpaths(&p);
        assert_eq!(c.len(), 2);
        assert!(c
            .iter()
            .all(|s| matches!(s.elements().last(), Some(PathEl::ClosePath))));
        let mut bad = square(0.0, 0.0, 1.0);
        bad.line_to((f64::NAN, 0.0));
        assert!(closed_subpaths(&bad).is_empty());
        let s = Sides {
            left: f64::NAN,
            right: -1.0,
            bottom: 2.0,
            top: f64::INFINITY,
        }
        .clean();
        assert_eq!(
            s,
            Sides {
                left: 0.0,
                right: 0.0,
                bottom: 2.0,
                top: 0.0
            }
        );
        assert_eq!(Sides::all(3.0).top, 3.0);
    }

    #[test]
    fn outer_contours_drop_holes_and_islands() {
        let mut p = square(0.0, 0.0, 30.0);
        p.extend(square(5.0, 5.0, 20.0));
        p.extend(square(10.0, 10.0, 10.0));
        p.extend(square(50.0, 0.0, 10.0));
        let o = outer_contours(&p);
        assert_eq!(crate::subpaths(&o).len(), 2);
        assert!((o.area().abs() - 1000.0).abs() < 1e-9);
    }
}
