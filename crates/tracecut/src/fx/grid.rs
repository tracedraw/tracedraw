//! Grids and lines: a grid of cells as lines or as rectangles, and lines
//! dividing a box.

use tracedraw_core::geometry::{Point, Rect, Size};

/// Most columns or rows of a grid.
pub const MAX_SIDE: usize = 1000;
/// Most cells of a grid.
pub const MAX_CELLS: usize = 100_000;
/// Most lines of Create Lines.
pub const MAX_LINES: usize = 10_000;

/// The settings of a grid.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct GridSpec {
    pub cols: usize,
    pub rows: usize,
    /// Cell width and height (mm).
    pub cell_w: f64,
    pub cell_h: f64,
}

impl Default for GridSpec {
    fn default() -> Self {
        GridSpec {
            cols: 4,
            rows: 3,
            cell_w: 20.0,
            cell_h: 20.0,
        }
    }
}

impl GridSpec {
    /// Columns and rows within the limits, or `None` for an empty grid.
    fn counts(&self) -> Option<(usize, usize)> {
        let (c, r) = (self.cols.min(MAX_SIDE), self.rows.min(MAX_SIDE));
        (c > 0 && r > 0 && c * r <= MAX_CELLS).then_some((c, r))
    }

    fn cell(&self) -> Option<Size> {
        let ok = |v: f64| v.is_finite() && v > 0.0;
        (ok(self.cell_w) && ok(self.cell_h)).then(|| Size::new(self.cell_w, self.cell_h))
    }

    /// The box the grid covers with its bottom-left corner at `origin`.
    pub fn bounds(&self, origin: Point) -> Option<Rect> {
        let (c, r) = self.counts()?;
        let cell = self.cell()?;
        finite(origin).then(|| {
            Rect::new(
                origin.x,
                origin.y,
                origin.x + c as f64 * cell.width,
                origin.y + r as f64 * cell.height,
            )
        })
    }

    /// The cell size that makes the grid fill `area`.
    pub fn fill(&self, area: Rect) -> Option<GridSpec> {
        let (c, r) = self.counts()?;
        let a = area.abs();
        let w = a.width() / c as f64;
        let h = a.height() / r as f64;
        let ok = |v: f64| v.is_finite() && v > 0.0;
        (ok(w) && ok(h) && finite(Point::new(a.x0, a.y0))).then_some(GridSpec {
            cols: c,
            rows: r,
            cell_w: w,
            cell_h: h,
        })
    }
}

fn finite(p: Point) -> bool {
    p.x.is_finite() && p.y.is_finite()
}

/// The grid's lines from `origin` (its bottom-left corner): the vertical
/// ones from left to right, then the horizontal ones from the bottom up.
pub fn grid_lines(origin: Point, spec: &GridSpec) -> Vec<[Point; 2]> {
    let (Some(b), Some((c, r)), Some(cell)) = (spec.bounds(origin), spec.counts(), spec.cell())
    else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(c + r + 2);
    for i in 0..=c {
        let x = b.x0 + i as f64 * cell.width;
        out.push([Point::new(x, b.y0), Point::new(x, b.y1)]);
    }
    for j in 0..=r {
        let y = b.y0 + j as f64 * cell.height;
        out.push([Point::new(b.x0, y), Point::new(b.x1, y)]);
    }
    out
}

/// The grid's cells from `origin`, row by row from the bottom, each row
/// from the left.
pub fn grid_cells(origin: Point, spec: &GridSpec) -> Vec<Rect> {
    let (Some(b), Some((c, r)), Some(cell)) = (spec.bounds(origin), spec.counts(), spec.cell())
    else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(c * r);
    for j in 0..r {
        for i in 0..c {
            let x = b.x0 + i as f64 * cell.width;
            let y = b.y0 + j as f64 * cell.height;
            out.push(Rect::new(x, y, x + cell.width, y + cell.height));
        }
    }
    out
}

/// `n` lines across `area`, dividing it into `n + 1` equal strips:
/// horizontal lines from the bottom up, or vertical ones from the left.
pub fn across_lines(area: Rect, n: usize, vertical: bool) -> Vec<[Point; 2]> {
    let a = area.abs();
    let n = n.min(MAX_LINES);
    if !(finite(Point::new(a.x0, a.y0)) && finite(Point::new(a.x1, a.y1))) {
        return Vec::new();
    }
    (1..=n)
        .map(|i| {
            let f = i as f64 / (n + 1) as f64;
            if vertical {
                let x = a.x0 + a.width() * f;
                [Point::new(x, a.y0), Point::new(x, a.y1)]
            } else {
                let y = a.y0 + a.height() * f;
                [Point::new(a.x0, y), Point::new(a.x1, y)]
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_grid_has_its_lines_and_cells() {
        let spec = GridSpec {
            cols: 3,
            rows: 2,
            cell_w: 10.0,
            cell_h: 5.0,
        };
        let o = Point::new(1.0, 2.0);
        let lines = grid_lines(o, &spec);
        assert_eq!(lines.len(), 4 + 3);
        assert_eq!(lines[3], [Point::new(31.0, 2.0), Point::new(31.0, 12.0)]);
        assert_eq!(lines[6], [Point::new(1.0, 12.0), Point::new(31.0, 12.0)]);
        let cells = grid_cells(o, &spec);
        assert_eq!(cells.len(), 6);
        assert_eq!(cells[0], Rect::new(1.0, 2.0, 11.0, 7.0));
        assert_eq!(cells[5], Rect::new(21.0, 7.0, 31.0, 12.0));
    }

    #[test]
    fn a_grid_fills_a_box() {
        let spec = GridSpec {
            cols: 4,
            rows: 2,
            ..Default::default()
        };
        let f = spec.fill(Rect::new(10.0, 10.0, 50.0, 30.0)).expect("fits");
        assert_eq!((f.cell_w, f.cell_h), (10.0, 10.0));
        assert_eq!(
            f.bounds(Point::new(10.0, 10.0)),
            Some(Rect::new(10.0, 10.0, 50.0, 30.0))
        );
        assert!(spec.fill(Rect::new(0.0, 0.0, 0.0, 10.0)).is_none());
        assert!(spec.fill(Rect::new(0.0, 0.0, f64::NAN, 10.0)).is_none());
    }

    #[test]
    fn lines_divide_a_box_evenly() {
        let r = Rect::new(0.0, 0.0, 40.0, 30.0);
        let h = across_lines(r, 2, false);
        assert_eq!(
            h,
            vec![
                [Point::new(0.0, 10.0), Point::new(40.0, 10.0)],
                [Point::new(0.0, 20.0), Point::new(40.0, 20.0)]
            ]
        );
        let v = across_lines(r, 3, true);
        assert_eq!(v.len(), 3);
        assert_eq!(v[0][0].x, 10.0);
        assert!(across_lines(r, 0, true).is_empty());
        assert_eq!(across_lines(r, usize::MAX, true).len(), MAX_LINES);
        assert!(across_lines(Rect::new(0.0, 0.0, f64::INFINITY, 1.0), 2, true).is_empty());
    }

    #[test]
    fn bad_grids_are_empty() {
        let o = Point::ZERO;
        for spec in [
            GridSpec {
                cols: 0,
                ..Default::default()
            },
            GridSpec {
                cell_w: f64::NAN,
                ..Default::default()
            },
            GridSpec {
                cell_h: -1.0,
                ..Default::default()
            },
            GridSpec {
                cols: MAX_SIDE,
                rows: MAX_SIDE,
                ..Default::default()
            },
        ] {
            assert!(grid_lines(o, &spec).is_empty());
            assert!(grid_cells(o, &spec).is_empty());
        }
        assert!(grid_lines(Point::new(f64::NAN, 0.0), &GridSpec::default()).is_empty());
        // Oversized counts are capped, not refused.
        let big = GridSpec {
            cols: usize::MAX,
            rows: 1,
            ..Default::default()
        };
        assert_eq!(grid_cells(o, &big).len(), MAX_SIDE);
    }
}
