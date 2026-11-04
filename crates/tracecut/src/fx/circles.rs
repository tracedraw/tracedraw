//! Circle fill: fill closed outlines with circles that do not overlap.
//!
//! Random points are tried in the shape's box (seeded, so the same seed
//! gives the same fill). At each point inside the shape a radius is drawn
//! between the minimum and the maximum and shrunk to keep the gap from
//! the outline and from every circle placed so far; the circle is kept
//! when it is still at least the minimum. The tries stop at the count or
//! after a fixed number of attempts.

use super::region::{Region, FLATTEN_TOL};
use super::rng::Rng;
use std::collections::HashMap;
use tracedraw_core::geometry::{BezPath, Point};

/// Most circles one fill may place.
pub const MAX_COUNT: usize = 20_000;
/// Most random points one fill tries.
pub const MAX_TRIES: usize = 400_000;

/// The settings of the circle fill.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct CircleSpec {
    /// Radius range (mm).
    pub r_min: f64,
    pub r_max: f64,
    /// Space kept between circles and from the outline (mm).
    pub gap: f64,
    /// Most circles placed.
    pub count: usize,
    pub seed: u64,
}

impl Default for CircleSpec {
    fn default() -> Self {
        CircleSpec {
            r_min: 2.0,
            r_max: 10.0,
            gap: 1.0,
            count: 200,
            seed: 1,
        }
    }
}

/// A placed circle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Disc {
    pub center: Point,
    pub radius: f64,
}

/// Circles inside the outlines (even-odd, holes stay empty).
pub fn circle_fill(outlines: &[BezPath], spec: &CircleSpec) -> Vec<Disc> {
    let region = Region::from_paths(outlines, FLATTEN_TOL);
    fill_region(&region, spec)
}

/// Circles inside a region.
pub fn fill_region(region: &Region, spec: &CircleSpec) -> Vec<Disc> {
    let ok = |v: f64| v.is_finite() && v > 0.0;
    let gap = if spec.gap.is_finite() {
        spec.gap.max(0.0)
    } else {
        0.0
    };
    if !ok(spec.r_min) || !ok(spec.r_max) || spec.count == 0 {
        return Vec::new();
    }
    let (r_min, r_max) = if spec.r_min <= spec.r_max {
        (spec.r_min, spec.r_max)
    } else {
        (spec.r_max, spec.r_min)
    };
    let Some(b) = region.bbox() else {
        return Vec::new();
    };
    let count = spec.count.min(MAX_COUNT);
    let tries = (count * 60 + 2000).min(MAX_TRIES);
    // Placed circles by grid cell; a cell is as wide as the largest
    // reach between two circles, so neighbours are in the 3 x 3 cells.
    let cell = 2.0 * r_max + gap;
    let mut grid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    let key = |p: Point| ((p.x / cell).floor() as i64, (p.y / cell).floor() as i64);
    let mut out: Vec<Disc> = Vec::new();
    let mut rng = Rng::new(spec.seed);
    for _ in 0..tries {
        if out.len() >= count {
            break;
        }
        let p = Point::new(rng.range(b.x0, b.x1), rng.range(b.y0, b.y1));
        let want = rng.range(r_min, r_max);
        if !region.contains(p) {
            continue;
        }
        let mut r = want;
        if let Some(d) = region.edge_within(p, r_max + gap) {
            r = r.min(d - gap);
        }
        if r < r_min {
            continue;
        }
        let (kx, ky) = key(p);
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(ids) = grid.get(&(kx + dx, ky + dy)) {
                    for i in ids {
                        if let Some(o) = out.get(*i) {
                            r = r.min((o.center - p).hypot() - o.radius - gap);
                        }
                    }
                }
            }
        }
        if r < r_min {
            continue;
        }
        grid.entry((kx, ky)).or_default().push(out.len());
        out.push(Disc {
            center: p,
            radius: r,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::{Rect, Shape as _};

    fn check(region: &Region, discs: &[Disc], spec: &CircleSpec) {
        for (i, a) in discs.iter().enumerate() {
            assert!(a.radius >= spec.r_min - 1e-9 && a.radius <= spec.r_max + 1e-9);
            assert!(region.contains(a.center));
            assert!(region.edge_distance(a.center) >= a.radius + spec.gap - 1e-9);
            for b in &discs[..i] {
                let d = (a.center - b.center).hypot();
                assert!(d >= a.radius + b.radius + spec.gap - 1e-9);
            }
        }
    }

    #[test]
    fn circles_stay_inside_apart_and_in_range() {
        let sq = Rect::new(0.0, 0.0, 100.0, 100.0).to_path(0.01);
        let spec = CircleSpec::default();
        let discs = circle_fill(std::slice::from_ref(&sq), &spec);
        assert!(discs.len() > 40, "{}", discs.len());
        check(&Region::from_paths(&[sq], FLATTEN_TOL), &discs, &spec);
    }

    #[test]
    fn holes_stay_empty_and_the_count_is_kept() {
        let mut p = Rect::new(0.0, 0.0, 100.0, 100.0).to_path(0.01);
        p.extend(Rect::new(20.0, 20.0, 80.0, 80.0).to_path(0.01));
        let spec = CircleSpec {
            r_min: 1.0,
            r_max: 3.0,
            count: 30,
            ..Default::default()
        };
        let discs = circle_fill(std::slice::from_ref(&p), &spec);
        assert_eq!(discs.len(), 30);
        let region = Region::from_paths(&[p], FLATTEN_TOL);
        check(&region, &discs, &spec);
        for d in &discs {
            assert!(!Rect::new(20.0, 20.0, 80.0, 80.0).contains(d.center));
        }
    }

    #[test]
    fn the_seed_repeats_the_fill() {
        let sq = Rect::new(0.0, 0.0, 50.0, 50.0).to_path(0.01);
        let spec = CircleSpec::default();
        assert_eq!(
            circle_fill(std::slice::from_ref(&sq), &spec),
            circle_fill(std::slice::from_ref(&sq), &spec)
        );
        let other = CircleSpec { seed: 9, ..spec };
        assert_ne!(
            circle_fill(std::slice::from_ref(&sq), &spec),
            circle_fill(&[sq], &other)
        );
    }

    #[test]
    fn bad_settings_and_shapes_give_nothing() {
        let sq = Rect::new(0.0, 0.0, 50.0, 50.0).to_path(0.01);
        for spec in [
            CircleSpec {
                r_min: 0.0,
                ..Default::default()
            },
            CircleSpec {
                r_max: f64::NAN,
                ..Default::default()
            },
            CircleSpec {
                count: 0,
                ..Default::default()
            },
            // Too large for the shape.
            CircleSpec {
                r_min: 30.0,
                r_max: 40.0,
                ..Default::default()
            },
        ] {
            assert!(circle_fill(std::slice::from_ref(&sq), &spec).is_empty());
        }
        // Swapped radii and a bad gap still work.
        let spec = CircleSpec {
            r_min: 5.0,
            r_max: 2.0,
            gap: f64::NAN,
            count: 5,
            ..Default::default()
        };
        assert_eq!(circle_fill(&[sq], &spec).len(), 5);
        let mut bad = BezPath::new();
        bad.move_to((f64::INFINITY, 0.0));
        bad.line_to((1.0, 1.0));
        bad.line_to((0.0, 1.0));
        bad.close_path();
        assert!(circle_fill(&[bad], &CircleSpec::default()).is_empty());
    }
}
