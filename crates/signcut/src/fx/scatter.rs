//! Random copies: places for copies of an object at random spots inside
//! a container, each turned and scaled at random within ranges.
//!
//! The object stands for its bounding circle (centred on its box, through
//! the box's corners), so whatever the turn the copy stays inside the
//! circle. A place is kept when the copy's circle lies wholly inside the
//! container and keeps the minimum distance from every copy placed
//! before. The seed makes it repeatable.

use super::region::Region;
use super::rng::Rng;
use std::collections::HashMap;
use tracedraw_core::geometry::{Affine, Point, Rect, Vec2};

/// Most copies.
pub const MAX_COUNT: usize = 5_000;
/// Random places tried per copy wanted.
pub const TRIES_PER_COPY: usize = 200;
/// Random places tried in a row without success before giving up.
pub const MAX_MISSES: usize = 20_000;
/// Smallest scale.
pub const MIN_SCALE: f64 = 0.01;

/// The settings of the random copies.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ScatterSpec {
    pub count: usize,
    /// Turn range, degrees.
    pub rot_min: f64,
    pub rot_max: f64,
    /// Scale range (1 is the original size).
    pub scale_min: f64,
    pub scale_max: f64,
    /// Space kept between the copies' bounding circles (mm).
    pub min_dist: f64,
    pub seed: u64,
}

impl Default for ScatterSpec {
    fn default() -> Self {
        ScatterSpec {
            count: 20,
            rot_min: 0.0,
            rot_max: 360.0,
            scale_min: 0.8,
            scale_max: 1.2,
            min_dist: 0.0,
            seed: 1,
        }
    }
}

fn ordered(a: f64, b: f64, fallback: f64) -> (f64, f64) {
    match (a.is_finite(), b.is_finite()) {
        (true, true) => (a.min(b), a.max(b)),
        (true, false) => (a, a),
        (false, true) => (b, b),
        (false, false) => (fallback, fallback),
    }
}

/// The page-space transforms that put copies of an object with page
/// bounds `source` into `container`; fewer than asked when no more fit.
pub fn scatter(source: Rect, container: &Region, spec: &ScatterSpec) -> Vec<Affine> {
    if ![source.x0, source.y0, source.x1, source.y1]
        .iter()
        .all(|v| v.is_finite())
    {
        return Vec::new();
    }
    let s = source.abs();
    let Some(b) = container.bbox() else {
        return Vec::new();
    };
    let center = s.center();
    let radius = (s.width().hypot(s.height()) / 2.0).max(1e-6);
    if !radius.is_finite() {
        return Vec::new();
    }
    let (rot0, rot1) = ordered(spec.rot_min, spec.rot_max, 0.0);
    let (sc0, sc1) = ordered(spec.scale_min, spec.scale_max, 1.0);
    let (sc0, sc1) = (sc0.max(MIN_SCALE), sc1.max(MIN_SCALE));
    let gap = if spec.min_dist.is_finite() {
        spec.min_dist.max(0.0)
    } else {
        0.0
    };
    let count = spec.count.min(MAX_COUNT);
    let mut rng = Rng::new(spec.seed);
    // Placed circles by grid cell, a cell as wide as the farthest reach
    // between two copies, so neighbours are in the 3 x 3 cells.
    let cell = 2.0 * radius * sc1 + gap;
    let key = |p: Point| ((p.x / cell).floor() as i64, (p.y / cell).floor() as i64);
    let mut grid: HashMap<(i64, i64), Vec<(Point, f64)>> = HashMap::new();
    let mut out = Vec::new();
    let mut misses = 0usize;
    for _ in 0..count.saturating_mul(TRIES_PER_COPY) {
        if out.len() >= count || misses >= MAX_MISSES {
            break;
        }
        misses += 1;
        let p = Point::new(rng.range(b.x0, b.x1), rng.range(b.y0, b.y1));
        let turn = rng.range(rot0, rot1);
        let k = rng.range(sc0, sc1);
        let r = radius * k;
        if !container.contains(p) || container.edge_within(p, r).is_some() {
            continue;
        }
        let (kx, ky) = key(p);
        let near = (-1..=1).flat_map(|dx| (-1..=1).map(move |dy| (kx + dx, ky + dy)));
        let hit = near
            .filter_map(|c| grid.get(&c))
            .flatten()
            .any(|(q, rq)| (*q - p).hypot() < r + rq + gap);
        if hit {
            continue;
        }
        misses = 0;
        grid.entry((kx, ky)).or_default().push((p, r));
        out.push(
            Affine::translate(p.to_vec2())
                * Affine::rotate(turn.to_radians())
                * Affine::scale(k)
                * Affine::translate(-Vec2::new(center.x, center.y)),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::region::FLATTEN_TOL;
    use tracedraw_core::geometry::Shape as _;

    fn page() -> Region {
        Region::from_paths(
            &[Rect::new(0.0, 0.0, 200.0, 200.0).to_path(0.01)],
            FLATTEN_TOL,
        )
    }

    #[test]
    fn copies_land_inside_and_apart() {
        let src = Rect::new(500.0, 500.0, 510.0, 506.0);
        let spec = ScatterSpec {
            min_dist: 2.0,
            ..Default::default()
        };
        let ts = scatter(src, &page(), &spec);
        assert_eq!(ts.len(), 20);
        let r = (10.0f64.hypot(6.0)) / 2.0;
        let mut centres = Vec::new();
        for t in &ts {
            let k = t.determinant().sqrt();
            assert!((0.8 - 1e-9..=1.2 + 1e-9).contains(&k));
            let c = *t * src.center();
            // The corners of the source stay inside the page.
            for p in [
                Point::new(src.x0, src.y0),
                Point::new(src.x1, src.y0),
                Point::new(src.x0, src.y1),
                Point::new(src.x1, src.y1),
            ] {
                let q = *t * p;
                assert!(q.x >= 0.0 && q.x <= 200.0 && q.y >= 0.0 && q.y <= 200.0);
            }
            centres.push((c, r * k));
        }
        for (i, (a, ra)) in centres.iter().enumerate() {
            for (b, rb) in &centres[..i] {
                assert!((*a - *b).hypot() >= ra + rb + 2.0 - 1e-9);
            }
        }
    }

    #[test]
    fn the_seed_repeats_and_ranges_hold() {
        let src = Rect::new(0.0, 0.0, 4.0, 4.0);
        let spec = ScatterSpec {
            rot_min: 10.0,
            rot_max: 10.0,
            scale_min: 2.0,
            scale_max: 2.0,
            ..Default::default()
        };
        let a = scatter(src, &page(), &spec);
        assert_eq!(a, scatter(src, &page(), &spec));
        for t in &a {
            let c = t.as_coeffs();
            let angle = c[1].atan2(c[0]).to_degrees();
            assert!((angle - 10.0).abs() < 1e-9);
            assert!((t.determinant() - 4.0).abs() < 1e-9);
        }
        assert_ne!(a, scatter(src, &page(), &ScatterSpec { seed: 5, ..spec }));
    }

    #[test]
    fn too_big_or_bad_input_places_nothing() {
        let page = page();
        assert!(scatter(
            Rect::new(0.0, 0.0, 300.0, 300.0),
            &page,
            &ScatterSpec::default()
        )
        .is_empty());
        assert!(scatter(
            Rect::new(0.0, f64::NAN, 1.0, 1.0),
            &page,
            &ScatterSpec::default()
        )
        .is_empty());
        assert!(scatter(
            Rect::new(0.0, 0.0, 1.0, 1.0),
            &Region::default(),
            &ScatterSpec::default()
        )
        .is_empty());
        let odd = ScatterSpec {
            rot_min: f64::NAN,
            rot_max: f64::INFINITY,
            scale_min: f64::NAN,
            scale_max: -3.0,
            min_dist: f64::NAN,
            count: 3,
            ..Default::default()
        };
        let ts = scatter(Rect::new(0.0, 0.0, 1.0, 1.0), &page, &odd);
        assert_eq!(ts.len(), 3);
        assert!(ts
            .iter()
            .all(|t| t.as_coeffs().iter().all(|c| c.is_finite())));
        // Only a few fit: as many as fit, no more.
        let few = ScatterSpec {
            count: 1000,
            scale_min: 1.0,
            scale_max: 1.0,
            ..Default::default()
        };
        let n = scatter(Rect::new(0.0, 0.0, 60.0, 60.0), &page, &few).len();
        assert!((1..=9).contains(&n), "{n}");
    }
}
