//! Carrier (backing board): one outline around letters.
//!
//! Each object's closed outlines (holes by the even-odd rule) are offset
//! outwards by the distance plus the rounding with round corners, all of
//! them welded together, then offset back inwards by the rounding: the
//! inner corners and the narrow gaps between letters get that radius
//! (a gap narrower than twice the distance plus the rounding closes).
//! Optionally the holes are filled: only the outer outlines stay.
//! Letters far apart give separate pieces.
//!
//! The rectangle method makes a rectangle round all the outlines instead,
//! grown by a margin on each side, its corners optionally rounded.

use super::{outer_contours, region, Sides};
use tracedraw_core::geometry::{BezPath, Rect, RoundedRect, Shape as _};
use tracedraw_core::shaping::{self, Join, Op};

/// The carrier's shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum CarrierMethod {
    /// Following the letters at a distance.
    #[default]
    Contour,
    /// A rectangle round all the letters.
    Rectangle,
}

/// The carrier settings (millimetres).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct CarrierSpec {
    pub method: CarrierMethod,
    pub distance: f64,
    pub rounding: f64,
    pub fill_holes: bool,
    /// Rectangle: the space round the letters on each side.
    pub margins: Sides,
    /// Rectangle: the corner radius.
    pub corner_radius: f64,
}

impl Default for CarrierSpec {
    fn default() -> Self {
        CarrierSpec {
            method: CarrierMethod::Contour,
            distance: 10.0,
            rounding: 0.0,
            fill_holes: true,
            margins: Sides::all(10.0),
            corner_radius: 0.0,
        }
    }
}

/// The carrier as the spec's method makes it.
pub fn make(objects: &[Vec<BezPath>], spec: &CarrierSpec) -> BezPath {
    match spec.method {
        CarrierMethod::Contour => carrier(objects, spec),
        CarrierMethod::Rectangle => rectangle(objects, spec.margins, spec.corner_radius),
    }
}

/// A rectangle round the objects' outlines (open ones too), grown by the
/// margins, its corners rounded by `radius` (at most half the shorter
/// side). Empty when there is nothing to go round.
pub fn rectangle(objects: &[Vec<BezPath>], margins: Sides, radius: f64) -> BezPath {
    let bounds = objects
        .iter()
        .flatten()
        .filter(|p| super::finite_path(p) && !p.elements().is_empty())
        .map(|p| p.bounding_box())
        .reduce(|a, b| a.union(b));
    let Some(b) = bounds else {
        return BezPath::new();
    };
    let m = margins.clean();
    let r = Rect::new(b.x0 - m.left, b.y0 - m.bottom, b.x1 + m.right, b.y1 + m.top);
    if r.width() < 1e-9 || r.height() < 1e-9 || !r.area().is_finite() {
        return BezPath::new();
    }
    let rad = if radius.is_finite() {
        radius.max(0.0)
    } else {
        0.0
    }
    .min(r.width().min(r.height()) / 2.0);
    if rad > 1e-9 {
        RoundedRect::from_rect(r, rad).to_path(0.01)
    } else {
        r.to_path(0.01)
    }
}

/// The carrier's outline around the objects (their outlines in page
/// space). Empty when none of them has a closed outline.
pub fn carrier(objects: &[Vec<BezPath>], spec: &CarrierSpec) -> BezPath {
    let clean = |v: f64| if v.is_finite() { v.max(0.0) } else { 0.0 };
    let d = clean(spec.distance);
    let r = clean(spec.rounding);
    let mut acc: Option<BezPath> = None;
    for paths in objects {
        let reg = region(paths);
        if reg.elements().is_empty() {
            continue;
        }
        let grown = if d + r > 1e-9 {
            shaping::offset_with(&reg, d + r, Join::Round)
        } else {
            // Only simplified: welding needs clean outlines.
            shaping::overlay(&reg, &BezPath::new(), Op::Weld)
        };
        if grown.elements().is_empty() {
            continue;
        }
        acc = Some(match acc {
            None => grown,
            Some(a) => shaping::overlay(&a, &grown, Op::Weld),
        });
    }
    let Some(mut out) = acc else {
        return BezPath::new();
    };
    if r > 1e-9 {
        out = shaping::offset_with(&out, -r, Join::Round);
    }
    if spec.fill_holes {
        out = outer_contours(&out);
    }
    if out.area().abs() < 1e-9 {
        return BezPath::new();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::Rect;

    fn square(x: f64, y: f64, s: f64) -> BezPath {
        Rect::new(x, y, x + s, y + s).to_path(0.01)
    }

    fn pieces(p: &BezPath) -> usize {
        crate::subpaths(p).len()
    }

    #[test]
    fn close_letters_get_one_outline() {
        // Two squares 15 apart, distance 10: the offsets overlap.
        let c = carrier(
            &[vec![square(0.0, 0.0, 50.0)], vec![square(65.0, 0.0, 50.0)]],
            &CarrierSpec::default(),
        );
        assert_eq!(pieces(&c), 1);
        let b = c.bounding_box();
        assert!((b.width() - 135.0).abs() < 0.05 && (b.height() - 70.0).abs() < 0.05);
    }

    #[test]
    fn far_letters_stay_apart_unless_rounding_closes_the_gap() {
        let objs = [vec![square(0.0, 0.0, 50.0)], vec![square(80.0, 0.0, 50.0)]];
        let c = carrier(&objs, &CarrierSpec::default());
        assert_eq!(pieces(&c), 2);
        let c = carrier(
            &objs,
            &CarrierSpec {
                rounding: 10.0,
                ..Default::default()
            },
        );
        assert_eq!(pieces(&c), 1);
    }

    #[test]
    fn holes_are_filled_or_kept() {
        // A ring 200 wide with a 160 hole: offset 10 leaves a 140 hole.
        let mut o = square(0.0, 0.0, 200.0);
        o.extend(square(20.0, 20.0, 160.0));
        let filled = carrier(&[vec![o.clone()]], &CarrierSpec::default());
        assert_eq!(pieces(&filled), 1);
        let kept = carrier(
            &[vec![o]],
            &CarrierSpec {
                fill_holes: false,
                ..Default::default()
            },
        );
        assert_eq!(pieces(&kept), 2);
        assert!(kept.area().abs() < filled.area().abs());
    }

    #[test]
    fn nothing_closed_makes_no_carrier() {
        let mut open = BezPath::new();
        open.move_to((0.0, 0.0));
        open.line_to((10.0, 0.0));
        assert!(carrier(&[vec![open]], &CarrierSpec::default())
            .elements()
            .is_empty());
        assert!(carrier(&[], &CarrierSpec::default()).elements().is_empty());
        let odd = CarrierSpec {
            distance: f64::NAN,
            rounding: f64::INFINITY,
            fill_holes: true,
            ..Default::default()
        };
        let c = carrier(&[vec![square(0.0, 0.0, 10.0)]], &odd);
        assert!((c.area().abs() - 100.0).abs() < 1e-6);
    }

    #[test]
    fn a_rectangle_goes_round_everything_with_its_margins() {
        let mut open = BezPath::new();
        open.move_to((200.0, 0.0));
        open.line_to((250.0, 30.0));
        let objs = [vec![square(0.0, 0.0, 50.0)], vec![open]];
        let spec = CarrierSpec {
            method: CarrierMethod::Rectangle,
            margins: Sides {
                left: 5.0,
                right: 10.0,
                bottom: 15.0,
                top: 20.0,
            },
            ..Default::default()
        };
        let c = make(&objs, &spec);
        assert_eq!(pieces(&c), 1);
        let b = c.bounding_box();
        assert_eq!(b, Rect::new(-5.0, -15.0, 260.0, 70.0));
        assert!((c.area().abs() - 265.0 * 85.0).abs() < 1e-6);
        // Rounded corners take a little area off.
        let r = make(
            &objs,
            &CarrierSpec {
                corner_radius: 10.0,
                ..spec
            },
        );
        let cut = (4.0 - std::f64::consts::PI) * 100.0;
        assert!((r.area().abs() - (265.0 * 85.0 - cut)).abs() < 0.5);
        // The contour method is the plain carrier.
        let c = make(&objs, &CarrierSpec::default());
        assert_eq!(c, carrier(&objs, &CarrierSpec::default()));
    }

    #[test]
    fn a_rectangle_of_nothing_is_empty() {
        let spec = CarrierSpec {
            method: CarrierMethod::Rectangle,
            margins: Sides::all(f64::NAN),
            corner_radius: f64::INFINITY,
            ..Default::default()
        };
        assert!(make(&[], &spec).elements().is_empty());
        let mut bad = BezPath::new();
        bad.move_to((f64::NAN, 0.0));
        bad.line_to((1.0, 1.0));
        assert!(make(&[vec![bad]], &spec).elements().is_empty());
        let c = make(&[vec![square(0.0, 0.0, 10.0)]], &spec);
        assert!((c.area().abs() - 100.0).abs() < 1e-6);
    }
}
