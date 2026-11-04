//! Hatching: fill closed outlines with parallel lines, one continuous
//! zigzag, or a crossed grid of lines.
//!
//! The outlines are flattened, turned so the lines run along x, inset
//! when asked, and cut by horizontal scan lines; the pieces inside (even-
//! odd, so holes stay empty) are the hatch lines, turned back.

use super::region::{Region, FLATTEN_TOL};
use tracedraw_core::geometry::{Affine, BezPath, Point};
use tracedraw_core::shaping::{self, Join};

/// Most lines one direction of hatching may have; a finer spacing is
/// widened to keep to it.
pub const MAX_LINES: usize = 20_000;

/// Smallest spacing (mm).
pub const MIN_SPACING: f64 = 0.05;

/// How the lines are joined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum HatchStyle {
    /// Separate parallel lines.
    #[default]
    Lines,
    /// The lines joined end to end into zigzags.
    Snake,
    /// Lines in two directions at right angles.
    Grid,
}

/// The settings of the hatching.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct HatchSpec {
    pub style: HatchStyle,
    /// Direction of the lines, degrees counter-clockwise from the x axis.
    pub angle: f64,
    /// Distance between neighbouring lines (mm).
    pub spacing: f64,
    /// Distance kept from the outline (mm), 0 for none.
    pub inset: f64,
}

impl Default for HatchSpec {
    fn default() -> Self {
        HatchSpec {
            style: HatchStyle::Lines,
            angle: 0.0,
            spacing: 2.0,
            inset: 0.0,
        }
    }
}

/// The hatch lines of one object (its outlines in page space), each an
/// open polyline in page space. Simple and grid lines have two points;
/// a snake has as many as it joins.
pub fn hatch(outlines: &[BezPath], spec: &HatchSpec) -> Vec<Vec<Point>> {
    if !(spec.spacing.is_finite() && spec.spacing > 0.0) {
        return Vec::new();
    }
    let mut region = Region::from_paths(outlines, FLATTEN_TOL);
    if region.is_empty() {
        return Vec::new();
    }
    if spec.inset.is_finite() && spec.inset > 0.0 {
        let inner = shaping::offset_with(&region.to_path(), -spec.inset, Join::Miter);
        region = Region::from_paths(&[inner], FLATTEN_TOL);
        if region.is_empty() {
            return Vec::new();
        }
    }
    let angle = if spec.angle.is_finite() {
        spec.angle
    } else {
        0.0
    };
    let mut out = directional(
        &region,
        angle,
        spec.spacing,
        spec.style == HatchStyle::Snake,
    );
    if spec.style == HatchStyle::Grid {
        out.extend(directional(&region, angle + 90.0, spec.spacing, false));
    }
    out
}

/// Lines in one direction, joined into zigzags when `snake`.
fn directional(region: &Region, angle_deg: f64, spacing: f64, snake: bool) -> Vec<Vec<Point>> {
    let turn = Affine::rotate(angle_deg.to_radians());
    let flat = region.transformed(turn.inverse());
    let Some(b) = flat.bbox() else {
        return Vec::new();
    };
    let h = b.height();
    let spacing = spacing.max(MIN_SPACING).max(h / MAX_LINES as f64);
    let mut n = (h / spacing + 1e-9).floor() as usize;
    let first = if n == 0 {
        n = 1;
        b.y0 + h / 2.0
    } else {
        // The lines centred in the height: half a spacing (and half the
        // rest) from the bottom.
        b.y0 + (h - n as f64 * spacing) / 2.0 + spacing / 2.0
    };
    let rows: Vec<(f64, Vec<(f64, f64)>)> = (0..n)
        .map(|k| {
            let y = first + k as f64 * spacing;
            let xs = flat.scan(y);
            let segs = xs
                .chunks_exact(2)
                .map(|c| (c[0], c[1]))
                .filter(|(a, b)| b - a > 1e-9)
                .collect();
            (y, segs)
        })
        .collect();
    let lines = if snake {
        join_snake(&flat, &rows)
    } else {
        rows.iter()
            .flat_map(|(y, segs)| {
                segs.iter()
                    .map(move |(a, b)| vec![Point::new(*a, *y), Point::new(*b, *y)])
            })
            .collect()
    };
    lines
        .into_iter()
        .map(|l| l.into_iter().map(|p| turn * p).collect())
        .collect()
}

/// Join the pieces of consecutive scan lines into zigzags: a piece goes
/// on from the end of a zigzag that stopped on the line before when the
/// straight step between them stays inside; otherwise it starts a new one.
fn join_snake(region: &Region, rows: &[(f64, Vec<(f64, f64)>)]) -> Vec<Vec<Point>> {
    struct Chain {
        points: Vec<Point>,
        last_row: usize,
    }
    let mut chains: Vec<Chain> = Vec::new();
    for (k, (y, segs)) in rows.iter().enumerate() {
        for (a, b) in segs {
            let pa = Point::new(*a, *y);
            let pb = Point::new(*b, *y);
            // The nearest chain end on the line before that can step here.
            let mut best: Option<(usize, bool, f64)> = None;
            for (ci, c) in chains.iter().enumerate() {
                if k == 0 || c.last_row + 1 != k {
                    continue;
                }
                let Some(end) = c.points.last().copied() else {
                    continue;
                };
                for (to_a, p) in [(true, pa), (false, pb)] {
                    let d = (p - end).hypot();
                    if best.is_some_and(|(_, _, bd)| bd <= d) {
                        continue;
                    }
                    // Inside, or along the outline (the sides of a square).
                    let m = end.midpoint(p);
                    let inside = region.contains(m) || region.edge_within(m, 1e-6).is_some();
                    if inside && !region.crosses(end, p) {
                        best = Some((ci, to_a, d));
                    }
                }
            }
            match best.and_then(|(ci, to_a, _)| chains.get_mut(ci).map(|c| (c, to_a))) {
                Some((c, to_a)) => {
                    if to_a {
                        c.points.extend([pa, pb]);
                    } else {
                        c.points.extend([pb, pa]);
                    }
                    c.last_row = k;
                }
                None => {
                    // Alternate the direction line by line.
                    let points = if k % 2 == 0 {
                        vec![pa, pb]
                    } else {
                        vec![pb, pa]
                    };
                    chains.push(Chain {
                        points,
                        last_row: k,
                    });
                }
            }
        }
    }
    chains.into_iter().map(|c| c.points).collect()
}

/// A polyline as an open path.
pub fn polyline_path(points: &[Point]) -> BezPath {
    let mut p = BezPath::new();
    for (i, q) in points.iter().enumerate() {
        if i == 0 {
            p.move_to(*q);
        } else {
            p.line_to(*q);
        }
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::{Circle, Rect, Shape as _};

    fn len(l: &[Point]) -> f64 {
        l.windows(2).map(|w| (w[1] - w[0]).hypot()).sum()
    }

    fn square(x: f64, y: f64, s: f64) -> BezPath {
        Rect::new(x, y, x + s, y + s).to_path(0.01)
    }

    #[test]
    fn a_square_gets_centred_parallel_lines() {
        let lines = hatch(&[square(0.0, 0.0, 10.0)], &HatchSpec::default());
        assert_eq!(lines.len(), 5);
        let ys: Vec<f64> = lines.iter().map(|l| l[0].y).collect();
        for (y, want) in ys.iter().zip([1.0, 3.0, 5.0, 7.0, 9.0]) {
            assert!((y - want).abs() < 1e-9, "{ys:?}");
        }
        for l in &lines {
            assert!((len(l) - 10.0).abs() < 1e-9);
        }
    }

    #[test]
    fn holes_stay_empty() {
        let mut p = square(0.0, 0.0, 10.0);
        p.extend(square(2.5, 2.5, 5.0));
        let lines = hatch(&[p], &HatchSpec::default());
        // y = 1 and 9 cross the full width; 3, 5 and 7 are split in two.
        assert_eq!(lines.len(), 8);
        let total: f64 = lines.iter().map(|l| len(l)).sum();
        assert!((total - (2.0 * 10.0 + 3.0 * 5.0)).abs() < 1e-9, "{total}");
    }

    #[test]
    fn angle_turns_the_lines_and_inset_shortens_them() {
        let spec = HatchSpec {
            angle: 90.0,
            ..Default::default()
        };
        let lines = hatch(&[square(0.0, 0.0, 10.0)], &spec);
        assert_eq!(lines.len(), 5);
        for l in &lines {
            assert!((l[0].x - l[1].x).abs() < 1e-9, "vertical");
        }
        let spec = HatchSpec {
            inset: 1.0,
            ..Default::default()
        };
        let lines = hatch(&[square(0.0, 0.0, 10.0)], &spec);
        assert_eq!(lines.len(), 4);
        for l in &lines {
            assert!((len(l) - 8.0).abs() < 1e-6, "{}", len(l));
            assert!(l.iter().all(|p| p.x >= 1.0 - 1e-6 && p.x <= 9.0 + 1e-6));
        }
        // An inset larger than the shape leaves nothing.
        let spec = HatchSpec {
            inset: 6.0,
            ..Default::default()
        };
        assert!(hatch(&[square(0.0, 0.0, 10.0)], &spec).is_empty());
    }

    #[test]
    fn snake_joins_a_convex_shape_into_one_line() {
        let spec = HatchSpec {
            style: HatchStyle::Snake,
            ..Default::default()
        };
        let c = Circle::new((0.0, 0.0), 10.0).to_path(0.01);
        let lines = hatch(&[c], &spec);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].len() >= 18, "{}", lines[0].len());
        let lines = hatch(&[square(0.0, 0.0, 10.0)], &spec);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].len(), 10);
        assert!((len(&lines[0]) - (5.0 * 10.0 + 4.0 * 2.0)).abs() < 1e-9);
        // A U shape: the arms go on separately.
        let mut u = BezPath::new();
        u.move_to((0.0, 0.0));
        u.line_to((30.0, 0.0));
        u.line_to((30.0, 30.0));
        u.line_to((20.0, 30.0));
        u.line_to((20.0, 10.0));
        u.line_to((10.0, 10.0));
        u.line_to((10.0, 30.0));
        u.line_to((0.0, 30.0));
        u.close_path();
        let lines = hatch(std::slice::from_ref(&u), &spec);
        assert!(lines.len() >= 2 && lines.len() <= 3, "{}", lines.len());
        // Every step stays inside.
        let r = Region::from_paths(&[u], FLATTEN_TOL);
        for l in &lines {
            for w in l.windows(2) {
                assert!(!r.crosses(w[0], w[1]));
            }
        }
    }

    #[test]
    fn grid_crosses_the_lines() {
        let spec = HatchSpec {
            style: HatchStyle::Grid,
            ..Default::default()
        };
        let lines = hatch(&[square(0.0, 0.0, 10.0)], &spec);
        assert_eq!(lines.len(), 10);
    }

    #[test]
    fn degenerate_input_gives_nothing_and_never_panics() {
        let sq = square(0.0, 0.0, 10.0);
        for spacing in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let spec = HatchSpec {
                spacing,
                ..Default::default()
            };
            assert!(hatch(std::slice::from_ref(&sq), &spec).is_empty());
        }
        // A spacing too fine for the cap is widened.
        let spec = HatchSpec {
            spacing: 1e-9,
            ..Default::default()
        };
        assert!(hatch(std::slice::from_ref(&sq), &spec).len() <= MAX_LINES);
        let spec = HatchSpec {
            angle: f64::NAN,
            inset: f64::INFINITY,
            ..Default::default()
        };
        assert_eq!(hatch(&[sq], &spec).len(), 5);
        let mut bad = BezPath::new();
        bad.move_to((f64::NAN, 0.0));
        bad.line_to((1.0, f64::INFINITY));
        bad.line_to((1.0, 1.0));
        bad.close_path();
        assert!(hatch(&[bad, BezPath::new()], &HatchSpec::default()).is_empty());
        // A flat shape (zero height).
        let mut flat = BezPath::new();
        flat.move_to((0.0, 0.0));
        flat.line_to((10.0, 0.0));
        flat.line_to((5.0, 0.0));
        flat.close_path();
        assert!(hatch(&[flat], &HatchSpec::default()).is_empty());
    }
}
