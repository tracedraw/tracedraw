//! Cast shadow: the transform that turns an object's outline into its
//! flat shadow.
//!
//! - On a wall: the outline moved by a distance in a direction (the
//!   object stands off a wall behind it).
//! - On the floor: the outline flattened to a fraction of its height and
//!   slanted, standing on the object's bottom edge; it falls in front
//!   (below the bottom edge) or behind (above it, mostly hidden by the
//!   object).

use super::region::{Region, FLATTEN_TOL};
use crate::subpaths;
use tracedraw_core::geometry::{Affine, BezPath, Rect, Vec2};
use tracedraw_core::shaping::{self, Op};

/// Steepest floor slant, degrees either way.
pub const MAX_SKEW: f64 = 80.0;
/// Range of the floor's height factor.
pub const MIN_SCALE: f64 = 0.01;
pub const MAX_SCALE: f64 = 5.0;

/// Where the shadow falls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum ShadowKind {
    #[default]
    Wall,
    Floor,
}

/// The settings of the shadow.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ShadowSpec {
    pub kind: ShadowKind,
    /// Wall: the direction of the move, degrees counter-clockwise from x.
    pub angle: f64,
    /// Wall: the length of the move (mm).
    pub distance: f64,
    /// Floor: the shadow's height as a fraction of the object's (0.3).
    pub scale: f64,
    /// Floor: the slant, degrees from upright; positive leans right.
    pub skew: f64,
    /// Floor: the shadow falls in front, below the bottom edge (else
    /// behind, above it).
    pub in_front: bool,
}

impl Default for ShadowSpec {
    fn default() -> Self {
        ShadowSpec {
            kind: ShadowKind::Wall,
            angle: -45.0,
            distance: 3.0,
            scale: 0.3,
            skew: 45.0,
            in_front: true,
        }
    }
}

/// The page-space transform from an object with page bounds `bounds` to
/// its shadow; `None` for settings or bounds that are not finite.
pub fn shadow_transform(bounds: Rect, spec: &ShadowSpec) -> Option<Affine> {
    if ![bounds.x0, bounds.y0, bounds.x1, bounds.y1]
        .iter()
        .all(|v| v.is_finite())
    {
        return None;
    }
    let b = bounds.abs();
    match spec.kind {
        ShadowKind::Wall => {
            if !(spec.angle.is_finite() && spec.distance.is_finite()) {
                return None;
            }
            let a = spec.angle.to_radians();
            Some(Affine::translate(
                Vec2::new(a.cos(), a.sin()) * spec.distance,
            ))
        }
        ShadowKind::Floor => {
            if !(spec.scale.is_finite() && spec.skew.is_finite()) {
                return None;
            }
            let k = spec.scale.clamp(MIN_SCALE, MAX_SCALE);
            let lean = spec.skew.clamp(-MAX_SKEW, MAX_SKEW).to_radians().tan();
            let y0 = b.y0;
            // A point h above the bottom edge goes h * k from it (down in
            // front, up behind) and h * k * tan(skew) sideways.
            let d = if spec.in_front { -k } else { k };
            let c = k * lean;
            Some(Affine::new([1.0, 0.0, c, d, -c * y0, y0 - d * y0]))
        }
    }
}

/// The area an object covers, as one closed path: the object's outline
/// when it is a single path of closed subpaths (curves kept), otherwise
/// the closed subpaths of every outline welded together (flattened,
/// 0.02 mm). Open subpaths cover nothing. Empty when nothing is closed.
pub fn shadow_outline(outlines: &[BezPath]) -> BezPath {
    if let [one] = outlines {
        let parts = subpaths(one);
        let finite = one
            .elements()
            .iter()
            .filter_map(|e| e.end_point())
            .all(|p| p.x.is_finite() && p.y.is_finite());
        if !parts.is_empty() && finite && parts.iter().all(|s| s.closed) {
            let mut p = BezPath::new();
            for s in parts {
                p.extend(s.path);
                if !matches!(
                    p.elements().last(),
                    Some(tracedraw_core::geometry::PathEl::ClosePath)
                ) {
                    p.close_path();
                }
            }
            return p;
        }
    }
    let mut acc: Option<BezPath> = None;
    for o in outlines {
        let r = Region::from_paths(std::slice::from_ref(o), FLATTEN_TOL);
        if r.is_empty() {
            continue;
        }
        let p = r.to_path();
        acc = Some(match acc {
            None => shaping::overlay(&p, &BezPath::new(), Op::Weld),
            Some(a) => shaping::overlay(&a, &p, Op::Weld),
        });
    }
    acc.unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::{Point, Shape as _};

    #[test]
    fn the_outline_keeps_curves_or_welds_pieces() {
        let c = tracedraw_core::geometry::Circle::new((0.0, 0.0), 5.0).to_path(0.01);
        let one = shadow_outline(std::slice::from_ref(&c));
        assert_eq!(one, c);
        // Two overlapping squares weld into one outline.
        let a = Rect::new(0.0, 0.0, 10.0, 10.0).to_path(0.01);
        let b = Rect::new(5.0, 0.0, 15.0, 10.0).to_path(0.01);
        let w = shadow_outline(&[a, b]);
        assert!((w.area().abs() - 150.0).abs() < 1e-6, "{}", w.area());
        // Open lines cover nothing.
        let mut open = BezPath::new();
        open.move_to((0.0, 0.0));
        open.line_to((10.0, 0.0));
        assert!(shadow_outline(&[open.clone()]).elements().is_empty());
        assert!(shadow_outline(&[]).elements().is_empty());
        let mut bad = BezPath::new();
        bad.move_to((f64::NAN, 0.0));
        bad.line_to((1.0, 1.0));
        bad.line_to((0.0, 1.0));
        bad.close_path();
        assert!(shadow_outline(&[bad, open]).elements().is_empty());
    }

    #[test]
    fn a_wall_shadow_moves_by_the_distance() {
        let spec = ShadowSpec {
            angle: -90.0,
            distance: 5.0,
            ..Default::default()
        };
        let t = shadow_transform(Rect::new(0.0, 0.0, 10.0, 10.0), &spec).expect("finite");
        let p = t * Point::new(3.0, 4.0);
        assert!((p.x - 3.0).abs() < 1e-9 && (p.y + 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_floor_shadow_stands_on_the_bottom_edge() {
        let spec = ShadowSpec {
            kind: ShadowKind::Floor,
            scale: 0.5,
            skew: 45.0,
            in_front: true,
            ..Default::default()
        };
        let r = Rect::new(10.0, 20.0, 30.0, 60.0);
        let t = shadow_transform(r, &spec).expect("finite");
        // The bottom edge stays.
        let p = t * Point::new(10.0, 20.0);
        assert!((p - Point::new(10.0, 20.0)).hypot() < 1e-9);
        // The top (40 up) goes 20 down and 20 right.
        let q = t * Point::new(10.0, 60.0);
        assert!((q - Point::new(30.0, 0.0)).hypot() < 1e-9, "{q:?}");
        let behind = ShadowSpec {
            in_front: false,
            ..spec
        };
        let q = shadow_transform(r, &behind).expect("finite") * Point::new(10.0, 60.0);
        assert!((q - Point::new(30.0, 40.0)).hypot() < 1e-9, "{q:?}");
    }

    #[test]
    fn extreme_and_bad_settings() {
        let r = Rect::new(0.0, 0.0, 10.0, 10.0);
        let floor = ShadowSpec {
            kind: ShadowKind::Floor,
            skew: 90.0,
            scale: 100.0,
            ..Default::default()
        };
        let t = shadow_transform(r, &floor).expect("clamped");
        assert!(t.as_coeffs().iter().all(|c| c.is_finite()));
        for spec in [
            ShadowSpec {
                distance: f64::NAN,
                ..Default::default()
            },
            ShadowSpec {
                kind: ShadowKind::Floor,
                skew: f64::INFINITY,
                ..Default::default()
            },
        ] {
            assert!(shadow_transform(r, &spec).is_none());
        }
        assert!(
            shadow_transform(Rect::new(0.0, f64::NAN, 1.0, 1.0), &ShadowSpec::default()).is_none()
        );
    }
}
