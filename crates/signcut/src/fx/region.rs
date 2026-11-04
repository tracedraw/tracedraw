//! The inside of closed outlines as flat polygons, with the queries the
//! effects need: is a point inside (even-odd), how far is the edge, where
//! does a line cross it, which parts of a polyline lie inside.
//!
//! Edges are kept in a uniform grid so the queries look at the edges near
//! a point instead of all of them.

use crate::subpaths;
use tracedraw_core::geometry::{flatten, Affine, BezPath, PathEl, Point, Rect};

/// Flattening tolerance of curved outlines (mm), as the shaping tools use.
pub const FLATTEN_TOL: f64 = 0.02;

/// Largest number of grid cells along a side.
const MAX_CELLS: usize = 256;

/// The area inside a set of closed polygons, by the even-odd rule.
#[derive(Debug, Clone, Default)]
pub struct Region {
    contours: Vec<Vec<Point>>,
    edges: Vec<(Point, Point)>,
    bbox: Option<Rect>,
    cell: f64,
    gx: usize,
    gy: usize,
    cells: Vec<Vec<usize>>,
}

fn finite(p: Point) -> bool {
    p.x.is_finite() && p.y.is_finite()
}

impl Region {
    /// The closed subpaths of `paths` (open ones are ignored), flattened
    /// within `tol`. A subpath with a non-finite point is dropped.
    pub fn from_paths(paths: &[BezPath], tol: f64) -> Region {
        let tol = if tol.is_finite() && tol > 0.0 {
            tol
        } else {
            FLATTEN_TOL
        };
        let mut contours = Vec::new();
        for path in paths {
            for sp in subpaths(path) {
                if !sp.closed {
                    continue;
                }
                let mut pts: Vec<Point> = Vec::new();
                let mut bad = false;
                flatten(&sp.path, tol, &mut |el| match el {
                    PathEl::MoveTo(p) | PathEl::LineTo(p) => {
                        if !finite(p) {
                            bad = true;
                        } else if pts.last().is_none_or(|q| (*q - p).hypot() > 1e-9) {
                            pts.push(p);
                        }
                    }
                    _ => {}
                });
                if !bad {
                    contours.push(pts);
                }
            }
        }
        Region::from_polygons(contours)
    }

    /// Polygons given by their corners (closed implicitly). Polygons with
    /// fewer than three corners or a non-finite corner are dropped.
    pub fn from_polygons(polygons: Vec<Vec<Point>>) -> Region {
        let mut contours: Vec<Vec<Point>> = Vec::new();
        for mut c in polygons {
            if c.iter().any(|p| !finite(*p)) {
                continue;
            }
            while c.len() > 1
                && c.first()
                    .zip(c.last())
                    .is_some_and(|(a, b)| (*a - *b).hypot() < 1e-9)
            {
                c.pop();
            }
            if c.len() >= 3 {
                contours.push(c);
            }
        }
        let mut edges = Vec::new();
        let mut bbox: Option<Rect> = None;
        for c in &contours {
            for (i, a) in c.iter().enumerate() {
                let b = c[(i + 1) % c.len()];
                edges.push((*a, b));
                let r = Rect::from_points(*a, b);
                bbox = Some(bbox.map_or(r, |x| x.union(r)));
            }
        }
        let mut region = Region {
            contours,
            edges,
            bbox,
            cell: 1.0,
            gx: 0,
            gy: 0,
            cells: Vec::new(),
        };
        region.build_grid();
        region
    }

    fn build_grid(&mut self) {
        let Some(b) = self.bbox else {
            return;
        };
        let n = self.edges.len().max(1);
        let per_side = ((n as f64).sqrt().ceil() as usize).clamp(1, MAX_CELLS);
        let side = b.width().max(b.height());
        let cell = if side > 1e-9 {
            side / per_side as f64
        } else {
            1.0
        };
        let gx = ((b.width() / cell).ceil() as usize).clamp(1, MAX_CELLS + 1);
        let gy = ((b.height() / cell).ceil() as usize).clamp(1, MAX_CELLS + 1);
        self.cell = cell;
        self.gx = gx;
        self.gy = gy;
        self.cells = vec![Vec::new(); gx * gy];
        for (i, (a, e)) in self.edges.iter().enumerate() {
            let r = Rect::from_points(*a, *e);
            let (cx0, cy0, cx1, cy1) = self.cell_range(r);
            for cy in cy0..=cy1 {
                for cx in cx0..=cx1 {
                    if let Some(v) = self.cells.get_mut(cy * gx + cx) {
                        v.push(i);
                    }
                }
            }
        }
    }

    /// The grid cells `r` covers, clamped to the grid.
    fn cell_range(&self, r: Rect) -> (usize, usize, usize, usize) {
        let Some(b) = self.bbox else {
            return (0, 0, 0, 0);
        };
        let col = |x: f64| -> usize {
            let c = ((x - b.x0) / self.cell).floor();
            if c.is_nan() || c < 0.0 {
                0
            } else {
                (c as usize).min(self.gx.saturating_sub(1))
            }
        };
        let row = |y: f64| -> usize {
            let c = ((y - b.y0) / self.cell).floor();
            if c.is_nan() || c < 0.0 {
                0
            } else {
                (c as usize).min(self.gy.saturating_sub(1))
            }
        };
        (col(r.x0), row(r.y0), col(r.x1), row(r.y1))
    }

    /// The indices of the edges in the cells `r` covers (may repeat).
    fn edges_near(&self, r: Rect) -> impl Iterator<Item = usize> + '_ {
        let overlaps = self.bbox.is_some_and(|b| {
            r.x1 >= b.x0 - 1e-9 && r.x0 <= b.x1 + 1e-9 && r.y1 >= b.y0 - 1e-9 && r.y0 <= b.y1 + 1e-9
        });
        let (cx0, cy0, cx1, cy1) = if overlaps {
            self.cell_range(r)
        } else {
            (1, 1, 0, 0)
        };
        let gx = self.gx;
        (cy0..=cy1)
            .flat_map(move |cy| (cx0..=cx1).map(move |cx| cy * gx + cx))
            .filter_map(move |i| self.cells.get(i))
            .flat_map(|v| v.iter().copied())
    }

    pub fn is_empty(&self) -> bool {
        self.contours.is_empty()
    }

    /// The polygons, as given.
    pub fn contours(&self) -> &[Vec<Point>] {
        &self.contours
    }

    pub fn bbox(&self) -> Option<Rect> {
        self.bbox
    }

    /// The same region moved by `t`.
    pub fn transformed(&self, t: Affine) -> Region {
        Region::from_polygons(
            self.contours
                .iter()
                .map(|c| c.iter().map(|p| t * *p).collect())
                .collect(),
        )
    }

    /// The polygons as a closed path.
    pub fn to_path(&self) -> BezPath {
        let mut p = BezPath::new();
        for c in &self.contours {
            for (i, q) in c.iter().enumerate() {
                if i == 0 {
                    p.move_to(*q);
                } else {
                    p.line_to(*q);
                }
            }
            p.close_path();
        }
        p
    }

    /// Area inside, even-odd: a polygon inside an odd number of others
    /// is a hole.
    pub fn area(&self) -> f64 {
        let mut total = 0.0;
        for (i, c) in self.contours.iter().enumerate() {
            let a = polygon_area(c).abs();
            let Some(p) = c.first() else { continue };
            let depth = self
                .contours
                .iter()
                .enumerate()
                .filter(|(j, o)| *j != i && point_in_polygon(*p, o))
                .count();
            total += if depth % 2 == 0 { a } else { -a };
        }
        total
    }

    /// Whether `p` is inside (even-odd). Points on an edge may go either way.
    pub fn contains(&self, p: Point) -> bool {
        let Some(b) = self.bbox else {
            return false;
        };
        if !finite(p) || p.x < b.x0 || p.x > b.x1 || p.y < b.y0 || p.y > b.y1 {
            return false;
        }
        // The edges a ray to the right of p can meet.
        let ray = Rect::new(p.x, p.y, b.x1, p.y);
        let mut ids: Vec<usize> = self.edges_near(ray).collect();
        ids.sort_unstable();
        ids.dedup();
        let mut inside = false;
        for i in ids {
            let Some((a, e)) = self.edges.get(i) else {
                continue;
            };
            if (a.y > p.y) != (e.y > p.y) {
                let x = a.x + (p.y - a.y) * (e.x - a.x) / (e.y - a.y);
                if p.x < x {
                    inside = !inside;
                }
            }
        }
        inside
    }

    /// Distance from `p` to the nearest edge when it is at most `reach`;
    /// `None` when every edge is farther.
    pub fn edge_within(&self, p: Point, reach: f64) -> Option<f64> {
        if !finite(p) || !reach.is_finite() || reach < 0.0 {
            return None;
        }
        let r = Rect::new(p.x - reach, p.y - reach, p.x + reach, p.y + reach);
        let mut best: Option<f64> = None;
        for i in self.edges_near(r) {
            if let Some((a, b)) = self.edges.get(i) {
                let d = segment_distance(p, *a, *b);
                if d <= reach && best.is_none_or(|x| d < x) {
                    best = Some(d);
                }
            }
        }
        best
    }

    /// Distance from `p` to the nearest edge (infinite with no edges).
    pub fn edge_distance(&self, p: Point) -> f64 {
        self.edges
            .iter()
            .map(|(a, b)| segment_distance(p, *a, *b))
            .fold(f64::INFINITY, f64::min)
    }

    /// Whether the segment `a`-`b` crosses an edge (touching an edge at
    /// one of its own ends does not count).
    pub fn crosses(&self, a: Point, b: Point) -> bool {
        if !finite(a) || !finite(b) {
            return true;
        }
        let r = Rect::from_points(a, b);
        self.edges_near(r).any(|i| {
            self.edges.get(i).is_some_and(|(c, d)| {
                segment_hit(a, b, *c, *d).is_some_and(|(t, _)| t > 1e-9 && t < 1.0 - 1e-9)
            })
        })
    }

    /// Where the horizontal line at `y` crosses the edges, sorted by x.
    pub fn scan(&self, y: f64) -> Vec<f64> {
        let Some(b) = self.bbox else {
            return Vec::new();
        };
        let mut ids: Vec<usize> = self.edges_near(Rect::new(b.x0, y, b.x1, y)).collect();
        ids.sort_unstable();
        ids.dedup();
        let mut xs: Vec<f64> = ids
            .into_iter()
            .filter_map(|i| self.edges.get(i))
            .filter(|(a, e)| (a.y <= y && y < e.y) || (e.y <= y && y < a.y))
            .map(|(a, e)| a.x + (y - a.y) * (e.x - a.x) / (e.y - a.y))
            .filter(|x| x.is_finite())
            .collect();
        xs.sort_by(f64::total_cmp);
        xs
    }

    /// The parts of the polyline `line` that lie inside, each a polyline
    /// of its own.
    pub fn clip_polyline(&self, line: &[Point]) -> Vec<Vec<Point>> {
        let mut out: Vec<Vec<Point>> = Vec::new();
        let mut cur: Vec<Point> = Vec::new();
        for w in line.windows(2) {
            let (a, b) = (w[0], w[1]);
            if !finite(a) || !finite(b) || (b - a).hypot() < 1e-12 {
                continue;
            }
            let mut ts: Vec<f64> = vec![0.0, 1.0];
            for i in self.edges_near(Rect::from_points(a, b)) {
                if let Some((c, d)) = self.edges.get(i) {
                    if let Some((t, _)) = segment_hit(a, b, *c, *d) {
                        ts.push(t.clamp(0.0, 1.0));
                    }
                }
            }
            ts.sort_by(f64::total_cmp);
            ts.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
            for t in ts.windows(2) {
                let p = a.lerp(b, t[0]);
                let q = a.lerp(b, t[1]);
                if (q - p).hypot() < 1e-9 {
                    continue;
                }
                if self.contains(p.midpoint(q)) {
                    if cur.last().is_none_or(|l| (*l - p).hypot() > 1e-9) {
                        if cur.len() >= 2 {
                            out.push(std::mem::take(&mut cur));
                        }
                        cur.clear();
                        cur.push(p);
                    }
                    cur.push(q);
                } else if !cur.is_empty() {
                    if cur.len() >= 2 {
                        out.push(std::mem::take(&mut cur));
                    }
                    cur.clear();
                }
            }
        }
        if cur.len() >= 2 {
            out.push(cur);
        }
        out
    }
}

/// Signed area of a polygon (positive when counter-clockwise).
pub fn polygon_area(c: &[Point]) -> f64 {
    let n = c.len();
    if n < 3 {
        return 0.0;
    }
    let mut s = 0.0;
    for (i, a) in c.iter().enumerate() {
        let b = c[(i + 1) % n];
        s += a.x * b.y - b.x * a.y;
    }
    s / 2.0
}

/// Even-odd point in polygon.
pub fn point_in_polygon(p: Point, c: &[Point]) -> bool {
    let n = c.len();
    let mut inside = false;
    for (i, a) in c.iter().enumerate() {
        let b = c[(i + 1) % n];
        if (a.y > p.y) != (b.y > p.y) {
            let x = a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y);
            if p.x < x {
                inside = !inside;
            }
        }
    }
    inside
}

/// Distance from `p` to the segment `a`-`b`.
pub fn segment_distance(p: Point, a: Point, b: Point) -> f64 {
    let d = b - a;
    let len2 = d.hypot2();
    if len2 <= 1e-24 {
        return (p - a).hypot();
    }
    let t = ((p - a).dot(d) / len2).clamp(0.0, 1.0);
    (p - (a + d * t)).hypot()
}

/// Where the segments `a`-`b` and `c`-`d` meet: the parameters along
/// each (0 to 1). Parallel segments do not meet.
pub fn segment_hit(a: Point, b: Point, c: Point, d: Point) -> Option<(f64, f64)> {
    let r = b - a;
    let s = d - c;
    let den = r.cross(s);
    if den.abs() < 1e-15 * (r.hypot() * s.hypot()).max(1e-300) {
        return None;
    }
    let q = c - a;
    let t = q.cross(s) / den;
    let u = q.cross(r) / den;
    let eps = 1e-12;
    (t >= -eps && t <= 1.0 + eps && u >= -eps && u <= 1.0 + eps && t.is_finite() && u.is_finite())
        .then_some((t, u))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::{Circle, Shape as _};

    fn square(x: f64, y: f64, s: f64) -> BezPath {
        Rect::new(x, y, x + s, y + s).to_path(0.01)
    }

    #[test]
    fn inside_follows_even_odd_with_holes() {
        let mut p = square(0.0, 0.0, 10.0);
        p.extend(square(3.0, 3.0, 4.0));
        let r = Region::from_paths(&[p], FLATTEN_TOL);
        assert!(r.contains(Point::new(1.0, 1.0)));
        assert!(!r.contains(Point::new(5.0, 5.0)));
        assert!(!r.contains(Point::new(11.0, 5.0)));
        assert!(!r.contains(Point::new(f64::NAN, 5.0)));
        assert!((r.area() - 84.0).abs() < 1e-9);
    }

    #[test]
    fn open_paths_and_bad_points_make_no_region() {
        let mut open = BezPath::new();
        open.move_to((0.0, 0.0));
        open.line_to((10.0, 0.0));
        open.line_to((10.0, 10.0));
        let mut bad = BezPath::new();
        bad.move_to((0.0, 0.0));
        bad.line_to((f64::NAN, 1.0));
        bad.line_to((1.0, f64::INFINITY));
        bad.close_path();
        let r = Region::from_paths(&[open, bad, BezPath::new()], FLATTEN_TOL);
        assert!(r.is_empty());
        assert!(!r.contains(Point::new(1.0, 1.0)));
        assert!(r.scan(1.0).is_empty());
        assert!(r
            .clip_polyline(&[Point::new(0.0, 0.0), Point::new(5.0, 5.0)])
            .is_empty());
        assert_eq!(r.edge_within(Point::new(0.0, 0.0), 5.0), None);
    }

    #[test]
    fn distances_and_crossings() {
        let c = Circle::new((0.0, 0.0), 10.0).to_path(0.01);
        let r = Region::from_paths(&[c], FLATTEN_TOL);
        let d = r.edge_within(Point::new(0.0, 0.0), 20.0).expect("edge");
        assert!((d - 10.0).abs() < 0.05, "{d}");
        assert_eq!(r.edge_within(Point::new(0.0, 0.0), 5.0), None);
        assert!((r.edge_distance(Point::new(5.0, 0.0)) - 5.0).abs() < 0.05);
        assert!(r.crosses(Point::new(0.0, 0.0), Point::new(20.0, 0.0)));
        assert!(!r.crosses(Point::new(0.0, 0.0), Point::new(5.0, 5.0)));
    }

    #[test]
    fn polylines_are_cut_to_the_inside() {
        let mut p = square(0.0, 0.0, 10.0);
        p.extend(square(4.0, 0.0 + 4.0, 2.0));
        let r = Region::from_paths(&[p], FLATTEN_TOL);
        let parts = r.clip_polyline(&[Point::new(-5.0, 5.0), Point::new(15.0, 5.0)]);
        assert_eq!(parts.len(), 2);
        let len: f64 = parts
            .iter()
            .map(|l| l.windows(2).map(|w| (w[1] - w[0]).hypot()).sum::<f64>())
            .sum();
        assert!((len - 8.0).abs() < 1e-9, "{len}");
        let xs = r.scan(5.0);
        assert_eq!(xs.len(), 4);
    }
}
