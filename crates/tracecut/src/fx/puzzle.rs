//! Puzzle: cut a shape into jigsaw pieces.
//!
//! The shape's bounding box is divided into columns x rows cells. Every
//! inner edge between two cells gets a tab (a knob or a wave) on one side
//! or the other, its size a fraction of the cell and its place along the
//! edge moved at random as much as the randomness says; the seed makes it
//! repeatable. The result is either the cut lines (the inner edges, cut to
//! the shape) or the pieces (each cell outlined by its four edges, cut to
//! the shape by boolean intersection).

use super::region::{Region, FLATTEN_TOL};
use super::rng::Rng;
use tracedraw_core::geometry::{BezPath, PathEl, PathSeg, Point, Rect, Shape as _, Vec2};
use tracedraw_core::shaping::{self, Op};

/// Most columns or rows.
pub const MAX_SIDE: usize = 100;
/// Largest tab size, as a fraction of the cell.
pub const MAX_TAB: f64 = 0.35;

/// The outline of a tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum TabShape {
    /// A round head on a narrower neck.
    #[default]
    Classic,
    /// A smooth bump.
    Wave,
}

/// The settings of the puzzle.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct PuzzleSpec {
    pub cols: usize,
    pub rows: usize,
    /// Tab size as a fraction of the shorter side of a cell (0.2).
    pub tab: f64,
    /// How far tab places and sizes wander, 0 (none) to 1.
    pub randomness: f64,
    pub seed: u64,
    pub shape: TabShape,
}

impl Default for PuzzleSpec {
    fn default() -> Self {
        PuzzleSpec {
            cols: 4,
            rows: 3,
            tab: 0.2,
            randomness: 0.5,
            seed: 1,
            shape: TabShape::Classic,
        }
    }
}

/// The edges of a puzzle laid over a box.
struct Edges {
    xs: Vec<f64>,
    ys: Vec<f64>,
    /// Horizontal inner edges: `h[j - 1][i]` runs from (x_i, y_j) to
    /// (x_{i+1}, y_j), for rows boundaries j = 1..rows.
    h: Vec<Vec<Vec<PathSeg>>>,
    /// Vertical inner edges: `v[i - 1][j]` runs from (x_i, y_j) up to
    /// (x_i, y_{j+1}).
    v: Vec<Vec<Vec<PathSeg>>>,
}

fn edges(bounds: Rect, spec: &PuzzleSpec) -> Option<Edges> {
    let b = bounds.abs();
    let (cols, rows) = (spec.cols.min(MAX_SIDE), spec.rows.min(MAX_SIDE));
    let ok = |v: f64| v.is_finite() && v > 1e-9;
    if cols == 0
        || rows == 0
        || !ok(b.width())
        || !ok(b.height())
        || !b.x0.is_finite()
        || !b.y0.is_finite()
    {
        return None;
    }
    let cw = b.width() / cols as f64;
    let ch = b.height() / rows as f64;
    let tab = if spec.tab.is_finite() {
        spec.tab.clamp(0.0, MAX_TAB)
    } else {
        0.0
    };
    let size = tab * cw.min(ch);
    let rand = if spec.randomness.is_finite() {
        spec.randomness.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let xs: Vec<f64> = (0..=cols).map(|i| b.x0 + i as f64 * cw).collect();
    let ys: Vec<f64> = (0..=rows).map(|j| b.y0 + j as f64 * ch).collect();
    let mut rng = Rng::new(spec.seed);
    let mut edge = |a: Point, e: Point| -> Vec<PathSeg> {
        // Always draw the same numbers, so one edge's tab does not change
        // the others' when it is too short for one.
        let shift = rng.signed();
        let grow = rng.signed();
        let flip = rng.coin();
        let s = size * (1.0 + 0.2 * rand * grow);
        tab_edge(a, e, s, 0.5 + 0.2 * rand * shift, flip, spec.shape)
    };
    let mut h = Vec::new();
    for y in ys.iter().take(rows).skip(1) {
        let row = xs
            .windows(2)
            .map(|w| edge(Point::new(w[0], *y), Point::new(w[1], *y)))
            .collect();
        h.push(row);
    }
    let mut v = Vec::new();
    for x in xs.iter().take(cols).skip(1) {
        let col = ys
            .windows(2)
            .map(|w| edge(Point::new(*x, w[0]), Point::new(*x, w[1])))
            .collect();
        v.push(col);
    }
    Some(Edges { xs, ys, h, v })
}

/// One edge from `a` to `b` with a tab of size `s` centred at `center`
/// (a fraction of the edge), on the left of the direction or, with
/// `flip`, on the right. An edge too short for its tab stays straight.
fn tab_edge(a: Point, b: Point, s: f64, center: f64, flip: bool, shape: TabShape) -> Vec<PathSeg> {
    let d = b - a;
    let len = d.hypot();
    let straight = vec![PathSeg::Line(tracedraw_core::geometry::Line::new(a, b))];
    if len < 1e-9 || !(s.is_finite() && s > 1e-6) {
        return straight;
    }
    let u = d / len;
    let n = Vec2::new(-u.y, u.x) * if flip { -1.0 } else { 1.0 };
    // The tab's points in units of s, across the edge (x) and out (y).
    let half = match shape {
        TabShape::Classic => 0.4,
        TabShape::Wave => 0.6,
    } * s;
    let (lo, hi) = (half + 0.05 * len, len - half - 0.05 * len);
    if lo >= hi {
        return straight;
    }
    let c = if center.is_finite() {
        (center * len).clamp(lo, hi)
    } else {
        len / 2.0
    };
    let at = |x: f64, y: f64| a + u * (c + x * s) + n * (y * s);
    let mut p = BezPath::new();
    p.move_to(a);
    match shape {
        TabShape::Classic => {
            p.line_to(at(-0.4, 0.0));
            p.curve_to(at(-0.1, 0.0), at(-0.05, 0.2), at(-0.15, 0.35));
            p.curve_to(at(-0.3, 0.6), at(-0.25, 0.95), at(0.0, 0.95));
            p.curve_to(at(0.25, 0.95), at(0.3, 0.6), at(0.15, 0.35));
            p.curve_to(at(0.05, 0.2), at(0.1, 0.0), at(0.4, 0.0));
        }
        TabShape::Wave => {
            p.line_to(at(-0.6, 0.0));
            p.curve_to(at(-0.3, 0.0), at(-0.3, 0.6), at(0.0, 0.6));
            p.curve_to(at(0.3, 0.6), at(0.3, 0.0), at(0.6, 0.0));
        }
    }
    p.line_to(b);
    p.segments().collect()
}

fn append(path: &mut BezPath, segs: &[PathSeg]) {
    for s in segs {
        match s {
            PathSeg::Line(l) => path.line_to(l.p1),
            PathSeg::Quad(q) => path.quad_to(q.p1, q.p2),
            PathSeg::Cubic(c) => path.curve_to(c.p1, c.p2, c.p3),
        }
    }
}

fn reversed(segs: &[PathSeg]) -> Vec<PathSeg> {
    segs.iter().rev().map(|s| s.reverse()).collect()
}

/// The inner cut lines over `bounds`: the horizontal ones from the bottom
/// up, each across the whole width, then the vertical ones from the left.
pub fn puzzle_lines(bounds: Rect, spec: &PuzzleSpec) -> Vec<BezPath> {
    let Some(e) = edges(bounds, spec) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (row, y) in e.h.iter().zip(e.ys.iter().skip(1)) {
        let mut p = BezPath::new();
        p.move_to(Point::new(e.xs.first().copied().unwrap_or(0.0), *y));
        for segs in row {
            append(&mut p, segs);
        }
        out.push(p);
    }
    for (col, x) in e.v.iter().zip(e.xs.iter().skip(1)) {
        let mut p = BezPath::new();
        p.move_to(Point::new(*x, e.ys.first().copied().unwrap_or(0.0)));
        for segs in col {
            append(&mut p, segs);
        }
        out.push(p);
    }
    out
}

/// The pieces over `bounds`, closed, row by row from the bottom, each row
/// from the left.
pub fn puzzle_pieces(bounds: Rect, spec: &PuzzleSpec) -> Vec<BezPath> {
    let Some(e) = edges(bounds, spec) else {
        return Vec::new();
    };
    let cols = e.xs.len().saturating_sub(1);
    let rows = e.ys.len().saturating_sub(1);
    let line = |a: Point, b: Point| vec![PathSeg::Line(tracedraw_core::geometry::Line::new(a, b))];
    let mut out = Vec::new();
    for j in 0..rows {
        for i in 0..cols {
            let (Some(&x0), Some(&x1), Some(&y0), Some(&y1)) =
                (e.xs.get(i), e.xs.get(i + 1), e.ys.get(j), e.ys.get(j + 1))
            else {
                continue;
            };
            let bottom = j
                .checked_sub(1)
                .and_then(|k| e.h.get(k))
                .and_then(|r| r.get(i))
                .cloned()
                .unwrap_or_else(|| line(Point::new(x0, y0), Point::new(x1, y0)));
            let right =
                e.v.get(i)
                    .and_then(|c| c.get(j))
                    .cloned()
                    .unwrap_or_else(|| line(Point::new(x1, y0), Point::new(x1, y1)));
            let top =
                e.h.get(j)
                    .and_then(|r| r.get(i))
                    .map(|s| reversed(s))
                    .unwrap_or_else(|| line(Point::new(x1, y1), Point::new(x0, y1)));
            let left = i
                .checked_sub(1)
                .and_then(|k| e.v.get(k))
                .and_then(|c| c.get(j))
                .map(|s| reversed(s))
                .unwrap_or_else(|| line(Point::new(x0, y1), Point::new(x0, y0)));
            let mut p = BezPath::new();
            p.move_to(Point::new(x0, y0));
            for side in [&bottom, &right, &top, &left] {
                append(&mut p, side);
            }
            p.close_path();
            out.push(p);
        }
    }
    out
}

/// Whether the closed outlines are just their bounding box (a rectangle
/// standing upright), so nothing needs cutting to it.
fn is_box(region: &Region) -> bool {
    let Some(b) = region.bbox() else {
        return false;
    };
    let a = b.area();
    region.contours().len() == 1 && a > 0.0 && (region.area() - a).abs() <= 1e-6 * a
}

/// The puzzle of a shape given by its outlines: the cut lines inside it
/// (with `pieces` false) or the pieces cut to it. Curves are kept when
/// the shape is an upright rectangle; otherwise the result is flattened
/// (0.02 mm) where it is cut.
pub fn puzzle(outlines: &[BezPath], spec: &PuzzleSpec, pieces: bool) -> Vec<BezPath> {
    let region = Region::from_paths(outlines, FLATTEN_TOL);
    let Some(bounds) = region.bbox() else {
        return Vec::new();
    };
    let boxed = is_box(&region);
    if pieces {
        let all = puzzle_pieces(bounds, spec);
        if boxed {
            return all;
        }
        let shape = region.to_path();
        all.iter()
            .flat_map(|p| shaping::overlay_parts(p, &shape, Op::Intersect))
            .filter(|p| p.area().abs() > 1e-6)
            .collect()
    } else {
        let all = puzzle_lines(bounds, spec);
        if boxed {
            return all;
        }
        let mut out = Vec::new();
        for l in &all {
            let mut pts = Vec::new();
            tracedraw_core::geometry::flatten(l, FLATTEN_TOL, &mut |el| {
                if let PathEl::MoveTo(p) | PathEl::LineTo(p) = el {
                    pts.push(p);
                }
            });
            for part in region.clip_polyline(&pts) {
                out.push(super::hatch::polyline_path(&part));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::Circle;

    fn rect_path(w: f64, h: f64) -> BezPath {
        Rect::new(0.0, 0.0, w, h).to_path(0.01)
    }

    #[test]
    fn pieces_tile_the_box_exactly() {
        let spec = PuzzleSpec::default();
        let pieces = puzzle(&[rect_path(120.0, 90.0)], &spec, true);
        assert_eq!(pieces.len(), 12);
        let total: f64 = pieces.iter().map(|p| p.area().abs()).sum();
        assert!((total - 120.0 * 90.0).abs() < 1e-6, "{total}");
        // Every piece is a cell give or take its tabs.
        for p in &pieces {
            let a = p.area().abs();
            assert!(a > 900.0 * 0.7 && a < 900.0 * 1.3, "{a}");
        }
    }

    #[test]
    fn cut_lines_are_the_inner_edges() {
        let spec = PuzzleSpec {
            randomness: 0.0,
            ..Default::default()
        };
        let lines = puzzle(&[rect_path(120.0, 90.0)], &spec, false);
        assert_eq!(lines.len(), 2 + 3);
        for l in &lines {
            let b = l.bounding_box();
            assert!(b.x0 >= -1e-9 && b.x1 <= 120.0 + 1e-9 && b.y0 >= -1e-9 && b.y1 <= 90.0 + 1e-9);
            // Longer than straight because of the tabs.
            let straight = if b.width() > b.height() { 120.0 } else { 90.0 };
            assert!(crate::measure::length(l) > straight + 1.0);
        }
    }

    #[test]
    fn the_seed_repeats_the_puzzle_and_another_changes_it() {
        let spec = PuzzleSpec::default();
        let a = puzzle(&[rect_path(100.0, 100.0)], &spec, false);
        let b = puzzle(&[rect_path(100.0, 100.0)], &spec, false);
        assert_eq!(a, b);
        let c = puzzle(
            &[rect_path(100.0, 100.0)],
            &PuzzleSpec { seed: 2, ..spec },
            false,
        );
        assert_ne!(a, c);
    }

    #[test]
    fn wave_tabs_and_round_shapes_are_cut_to_the_shape() {
        let spec = PuzzleSpec {
            shape: TabShape::Wave,
            ..Default::default()
        };
        let circle = Circle::new((50.0, 50.0), 50.0).to_path(0.01);
        let lines = puzzle(std::slice::from_ref(&circle), &spec, false);
        assert!(lines.len() >= 5);
        for l in &lines {
            for el in l.elements() {
                if let Some(p) = el.end_point() {
                    assert!((p - Point::new(50.0, 50.0)).hypot() <= 50.0 + 1e-6);
                }
            }
        }
        let pieces = puzzle(&[circle], &spec, true);
        let total: f64 = pieces.iter().map(|p| p.area().abs()).sum();
        let disc = std::f64::consts::PI * 2500.0;
        assert!((total - disc).abs() < disc * 0.01, "{total} {disc}");
    }

    #[test]
    fn degenerate_input_gives_nothing() {
        let sq = rect_path(10.0, 10.0);
        for spec in [
            PuzzleSpec {
                cols: 0,
                ..Default::default()
            },
            PuzzleSpec {
                rows: 0,
                ..Default::default()
            },
        ] {
            assert!(puzzle(std::slice::from_ref(&sq), &spec, true).is_empty());
            assert!(puzzle(std::slice::from_ref(&sq), &spec, false).is_empty());
        }
        // One cell: one piece, no lines.
        let one = PuzzleSpec {
            cols: 1,
            rows: 1,
            ..Default::default()
        };
        assert_eq!(puzzle(std::slice::from_ref(&sq), &one, true).len(), 1);
        assert!(puzzle(std::slice::from_ref(&sq), &one, false).is_empty());
        // Non-finite settings fall back to straight edges.
        let odd = PuzzleSpec {
            tab: f64::NAN,
            randomness: f64::INFINITY,
            ..Default::default()
        };
        let lines = puzzle(std::slice::from_ref(&sq), &odd, false);
        assert_eq!(lines.len(), 5);
        assert!(lines.iter().all(|l| l
            .elements()
            .iter()
            .all(|e| !matches!(e, PathEl::CurveTo(..)))));
        let mut bad = BezPath::new();
        bad.move_to((f64::NAN, 0.0));
        bad.line_to((1.0, 1.0));
        bad.line_to((0.0, 1.0));
        bad.close_path();
        assert!(puzzle(&[bad, BezPath::new()], &PuzzleSpec::default(), true).is_empty());
        // Huge counts are capped.
        let many = PuzzleSpec {
            cols: usize::MAX,
            rows: 1,
            ..Default::default()
        };
        assert_eq!(
            puzzle(&[rect_path(1000.0, 10.0)], &many, false).len(),
            MAX_SIDE - 1
        );
    }
}
