//! Smart divide: large objects cut into tiles of a material size.
//!
//! Tiles are `width` x `height` and overlap their neighbours by
//! `overlap`, so a column starts every `width - overlap` from the grid's
//! origin (and a row every `height - overlap`). The grid covers the
//! objects' bounds with as few columns and rows as it takes from the
//! origin's column and row on. Each tile holds each object cut to the
//! tile's rectangle: closed outlines by a boolean intersection (holes
//! kept), open lines clipped as lines. Tiles are named by a row letter
//! from the top (A, B, ... Z, AA, ...) and a column number from the left
//! (A1 is the top left tile).

use super::{finite_path, polylines, region, FLAT_TOL};
use tracedraw_core::geometry::{BezPath, Point, Rect, Shape as _, Vec2};
use tracedraw_core::shaping::{self, Op};

/// The tile settings (millimetres).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct TileSpec {
    pub width: f64,
    pub height: f64,
    pub overlap: f64,
    /// Space added between tiles when they are spread apart (0: they stay
    /// in place).
    pub gap: f64,
}

impl Default for TileSpec {
    fn default() -> Self {
        TileSpec {
            width: 1220.0,
            height: 2440.0,
            overlap: 10.0,
            gap: 0.0,
        }
    }
}

/// The grid of tiles over some bounds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TileGrid {
    /// Bottom-left corner of the first (bottom-left) tile.
    pub start: Point,
    pub step: Vec2,
    pub size: Vec2,
    pub cols: usize,
    pub rows: usize,
}

/// Most tiles one division makes.
pub const MAX_TILES: usize = 2_500;

impl TileGrid {
    /// The tile in column `col` (from the left) and row `row` (from the
    /// bottom).
    pub fn rect(&self, col: usize, row: usize) -> Rect {
        let x = self.start.x + col as f64 * self.step.x;
        let y = self.start.y + row as f64 * self.step.y;
        Rect::new(x, y, x + self.size.x, y + self.size.y)
    }

    /// The tile's name: row letter counted from the top, column number.
    pub fn label(&self, col: usize, row: usize) -> String {
        let from_top = self.rows.saturating_sub(row + 1);
        format!("{}{}", row_letters(from_top), col + 1)
    }

    /// How far a tile moves when the tiles are spread by `gap`.
    pub fn spread(&self, col: usize, row: usize, gap: f64) -> Vec2 {
        let g = if gap.is_finite() { gap.max(0.0) } else { 0.0 };
        Vec2::new(col as f64 * g, row as f64 * g)
    }
}

/// A, B, ... Z, AA, AB, ...
pub fn row_letters(mut n: usize) -> String {
    let mut s = Vec::new();
    loop {
        s.push(b'A' + (n % 26) as u8);
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }
    s.reverse();
    String::from_utf8(s).unwrap_or_default()
}

/// The grid over `bounds` with columns and rows counted from `origin`.
/// `None` when the sizes make no tiles (overlap not under the tile size,
/// a size not finite) or more than [`MAX_TILES`].
pub fn grid(bounds: Rect, origin: Point, spec: &TileSpec) -> Option<TileGrid> {
    let b = bounds.abs();
    let fin = |v: f64| v.is_finite();
    if ![b.x0, b.y0, b.x1, b.y1, origin.x, origin.y]
        .iter()
        .all(|v| fin(*v))
    {
        return None;
    }
    let ov = if fin(spec.overlap) {
        spec.overlap.max(0.0)
    } else {
        0.0
    };
    let (w, h) = (spec.width, spec.height);
    if !(fin(w) && fin(h) && w > ov + 1e-6 && h > ov + 1e-6) {
        return None;
    }
    let (sx, sy) = (w - ov, h - ov);
    // First column: the last one starting at or left of the bounds.
    let c0 = ((b.x0 - origin.x) / sx).floor();
    let r0 = ((b.y0 - origin.y) / sy).floor();
    let x_start = origin.x + c0 * sx;
    let y_start = origin.y + r0 * sy;
    // Columns until a tile reaches the right side.
    let cols = (((b.x1 - x_start - w) / sx - 1e-9).ceil().max(0.0)) + 1.0;
    let rows = (((b.y1 - y_start - h) / sy - 1e-9).ceil().max(0.0)) + 1.0;
    if !(cols.is_finite() && rows.is_finite()) || cols * rows > MAX_TILES as f64 {
        return None;
    }
    Some(TileGrid {
        start: Point::new(x_start, y_start),
        step: Vec2::new(sx, sy),
        size: Vec2::new(w, h),
        cols: cols as usize,
        rows: rows as usize,
    })
}

/// The grid of `cols` x `rows` tiles that just covers `bounds`, the
/// tiles overlapping by `overlap`: each tile is `(W + (cols - 1) ov) /
/// cols` wide (and the same for the height). `None` when the sizes make
/// no tiles (a count 0, the overlap not under the tile size, a size not
/// finite) or more than [`MAX_TILES`].
pub fn grid_by_count(bounds: Rect, cols: usize, rows: usize, overlap: f64) -> Option<TileGrid> {
    let b = bounds.abs();
    if ![b.x0, b.y0, b.x1, b.y1].iter().all(|v| v.is_finite())
        || cols == 0
        || rows == 0
        || cols.saturating_mul(rows) > MAX_TILES
    {
        return None;
    }
    let ov = if overlap.is_finite() {
        overlap.max(0.0)
    } else {
        0.0
    };
    let w = (b.width() + (cols - 1) as f64 * ov) / cols as f64;
    let h = (b.height() + (rows - 1) as f64 * ov) / rows as f64;
    if !(w > ov + 1e-6 && h > ov + 1e-6) {
        return None;
    }
    Some(TileGrid {
        start: Point::new(b.x0, b.y0),
        step: Vec2::new(w - ov, h - ov),
        size: Vec2::new(w, h),
        cols,
        rows,
    })
}

/// The part of an object (its outlines) inside `rect`: the closed
/// outlines intersected with it, the open ones clipped. Empty when
/// nothing of the object is inside.
pub fn clip(paths: &[BezPath], rect: Rect) -> BezPath {
    let mut out = BezPath::new();
    let reg = region(paths);
    if !reg.elements().is_empty() {
        let r = rect.to_path(0.01);
        let part = shaping::overlay(&reg, &r, Op::Intersect);
        if part.area().abs() > 1e-9 {
            out.extend(part.elements().iter().copied());
        }
    }
    for p in paths.iter().filter(|p| finite_path(p)) {
        for poly in polylines(p, FLAT_TOL).into_iter().filter(|p| !p.closed) {
            for run in clip_polyline(&poly.pts, rect) {
                out.extend(
                    tracedraw_core::geometry::polyline_path(&run, false)
                        .elements()
                        .iter()
                        .copied(),
                );
            }
        }
    }
    out
}

/// The runs of a polyline inside a rectangle.
fn clip_polyline(pts: &[Point], r: Rect) -> Vec<Vec<Point>> {
    let mut runs: Vec<Vec<Point>> = Vec::new();
    let mut cur: Vec<Point> = Vec::new();
    for w in pts.windows(2) {
        match clip_segment(w[0], w[1], r) {
            Some((a, b)) => {
                if cur.last().is_none_or(|l| (*l - a).hypot() > 1e-9) {
                    if cur.len() >= 2 {
                        runs.push(std::mem::take(&mut cur));
                    }
                    cur.clear();
                    cur.push(a);
                }
                cur.push(b);
            }
            None => {
                if cur.len() >= 2 {
                    runs.push(std::mem::take(&mut cur));
                }
                cur.clear();
            }
        }
    }
    if cur.len() >= 2 {
        runs.push(cur);
    }
    runs.retain(|r| r.windows(2).map(|w| (w[1] - w[0]).hypot()).sum::<f64>() > 1e-9);
    runs
}

/// The part of the segment from `a` to `b` inside `r` (Liang-Barsky).
fn clip_segment(a: Point, b: Point, r: Rect) -> Option<(Point, Point)> {
    let d = b - a;
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (p, q) in [
        (-d.x, a.x - r.x0),
        (d.x, r.x1 - a.x),
        (-d.y, a.y - r.y0),
        (d.y, r.y1 - a.y),
    ] {
        if p.abs() < 1e-15 {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
    }
    (t0 <= t1).then(|| (a + d * t0, a + d * t1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grid_covers_the_bounds_with_overlapping_tiles() {
        // 3000 x 1000 from the origin, tiles 1220 x 2440 overlapping 10:
        // columns start every 1210: 0, 1210, 2420 (reaching 3640).
        let g = grid(
            Rect::new(0.0, 0.0, 3000.0, 1000.0),
            Point::ZERO,
            &TileSpec::default(),
        )
        .expect("a grid");
        assert_eq!((g.cols, g.rows), (3, 1));
        assert_eq!(g.rect(1, 0), Rect::new(1210.0, 0.0, 2430.0, 2440.0));
        // Exactly one tile wide: one column.
        let g = grid(
            Rect::new(0.0, 0.0, 1220.0, 2440.0),
            Point::ZERO,
            &TileSpec::default(),
        )
        .expect("a grid");
        assert_eq!((g.cols, g.rows), (1, 1));
        // Counted from an origin to the left: the column holding the bounds.
        let g = grid(
            Rect::new(1300.0, 0.0, 1400.0, 100.0),
            Point::ZERO,
            &TileSpec::default(),
        )
        .expect("a grid");
        assert_eq!(g.cols, 1);
        assert!((g.start.x - 1210.0).abs() < 1e-9);
    }

    #[test]
    fn labels_count_rows_from_the_top() {
        let g = grid(
            Rect::new(0.0, 0.0, 100.0, 250.0),
            Point::ZERO,
            &TileSpec {
                width: 100.0,
                height: 100.0,
                overlap: 0.0,
                gap: 0.0,
            },
        )
        .expect("a grid");
        assert_eq!(g.rows, 3);
        assert_eq!(g.label(0, 2), "A1");
        assert_eq!(g.label(0, 0), "C1");
        assert_eq!(row_letters(25), "Z");
        assert_eq!(row_letters(26), "AA");
        assert_eq!(row_letters(27), "AB");
        assert_eq!(g.spread(2, 1, 10.0), Vec2::new(20.0, 10.0));
    }

    #[test]
    fn clipping_keeps_the_part_inside_with_its_holes() {
        let mut o = Rect::new(0.0, 0.0, 200.0, 100.0).to_path(0.01);
        o.extend(Rect::new(80.0, 20.0, 120.0, 80.0).to_path(0.01));
        let left = clip(&[o.clone()], Rect::new(0.0, 0.0, 105.0, 100.0));
        // 105 x 100 less the hole's part 25 x 60.
        assert!((left.area().abs() - (10500.0 - 1500.0)).abs() < 1e-6);
        let none = clip(&[o], Rect::new(300.0, 0.0, 400.0, 100.0));
        assert!(none.elements().is_empty());
    }

    #[test]
    fn open_lines_are_clipped_as_lines() {
        let mut l = BezPath::new();
        l.move_to((-50.0, 50.0));
        l.line_to((150.0, 50.0));
        l.line_to((150.0, 150.0));
        let part = clip(&[l], Rect::new(0.0, 0.0, 100.0, 100.0));
        let b = part.bounding_box();
        assert_eq!(b, Rect::new(0.0, 50.0, 100.0, 50.0));
        assert!((crate::measure::length(&part) - 100.0).abs() < 1e-9);
    }

    #[test]
    fn overlapping_tiles_rebuild_the_whole() {
        let o = Rect::new(0.0, 0.0, 500.0, 300.0).to_path(0.01);
        let spec = TileSpec {
            width: 200.0,
            height: 200.0,
            overlap: 20.0,
            gap: 0.0,
        };
        let g = grid(o.bounding_box(), Point::ZERO, &spec).expect("a grid");
        assert_eq!((g.cols, g.rows), (3, 2));
        let mut total = 0.0;
        for c in 0..g.cols {
            for r in 0..g.rows {
                total += clip(std::slice::from_ref(&o), g.rect(c, r)).area().abs();
            }
        }
        // Each overlap strip counted twice: columns overlap 20 x 300 twice,
        // rows 500 x 20 once, the crossings 20 x 20 twice.
        let expect = 500.0 * 300.0 + 2.0 * 20.0 * 300.0 + 500.0 * 20.0 + 2.0 * 400.0;
        assert!((total - expect).abs() < 1e-6, "{total} {expect}");
    }

    #[test]
    fn bad_sizes_make_no_grid() {
        let b = Rect::new(0.0, 0.0, 100.0, 100.0);
        let bad = |w: f64, h: f64, ov: f64| TileSpec {
            width: w,
            height: h,
            overlap: ov,
            gap: 0.0,
        };
        assert!(grid(b, Point::ZERO, &bad(10.0, 10.0, 10.0)).is_none());
        assert!(grid(b, Point::ZERO, &bad(f64::NAN, 10.0, 0.0)).is_none());
        assert!(grid(b, Point::ZERO, &bad(0.001, 0.001, 0.0)).is_none());
        assert!(grid(b, Point::new(f64::NAN, 0.0), &bad(10.0, 10.0, 0.0)).is_none());
        assert!(grid(
            Rect::new(0.0, 0.0, f64::INFINITY, 1.0),
            Point::ZERO,
            &bad(10.0, 10.0, 0.0)
        )
        .is_none());
        assert!(clip(&[], b).elements().is_empty());
    }

    #[test]
    fn a_table_of_tiles_covers_the_bounds() {
        let b = Rect::new(0.0, 0.0, 1000.0, 400.0);
        let g = grid_by_count(b, 4, 2, 20.0).expect("a grid");
        assert_eq!((g.cols, g.rows), (4, 2));
        // (1000 + 3 x 20) / 4 = 265 wide, (400 + 20) / 2 = 210 high.
        assert!((g.size.x - 265.0).abs() < 1e-9 && (g.size.y - 210.0).abs() < 1e-9);
        let last = g.rect(3, 1);
        assert!((last.x1 - 1000.0).abs() < 1e-9 && (last.y1 - 400.0).abs() < 1e-9);
        assert!((g.rect(1, 0).x0 - 245.0).abs() < 1e-9);
        assert!(grid_by_count(b, 0, 2, 0.0).is_none());
        assert!(grid_by_count(b, 100, 100, 0.0).is_none());
        assert!(grid_by_count(b, 2, 2, 5000.0).is_none());
        assert!(grid_by_count(Rect::new(0.0, 0.0, f64::NAN, 1.0), 2, 2, 0.0).is_none());
        assert!(grid_by_count(b, 2, 2, f64::NAN).is_some());
    }
}
