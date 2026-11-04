//! Cabinet doors and panels: the outer outline, the inner panel inside
//! a frame (square, with rounded corners or with an arched top) and
//! optional groove lines offset inside the inner panel.

use tracedraw_core::geometry::{Arc, BezPath, Point, Rect, RoundedRect, Shape as _, Vec2};
use tracedraw_core::shaping::{self, Join};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum InnerShape {
    #[default]
    Square,
    Rounded,
    Arch,
}

/// What to draw.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct DoorSpec {
    /// Outer size (mm), used when no object gives it.
    pub width: f64,
    pub height: f64,
    /// Width of the frame around the inner panel (mm).
    pub frame: f64,
    pub inner: InnerShape,
    /// Corner radius of a rounded inner panel (mm).
    pub radius: f64,
    /// Rise of an arched top above its springing line (mm).
    pub arch: f64,
    /// Number of groove lines inside the inner panel (0: none), the
    /// first at `groove_offset` inside it, the next `groove_spacing`
    /// further each.
    pub grooves: u32,
    pub groove_offset: f64,
    pub groove_spacing: f64,
}

impl Default for DoorSpec {
    fn default() -> Self {
        DoorSpec {
            width: 400.0,
            height: 600.0,
            frame: 60.0,
            inner: InnerShape::Square,
            radius: 20.0,
            arch: 60.0,
            grooves: 0,
            groove_offset: 10.0,
            groove_spacing: 5.0,
        }
    }
}

/// The outlines of a door.
#[derive(Debug, Clone)]
pub struct Door {
    pub outer: BezPath,
    pub inner: BezPath,
    pub grooves: Vec<BezPath>,
}

fn finite(v: f64) -> bool {
    v.is_finite()
}

/// The door filling `outer` (page space). `None` when the frame leaves
/// no inner panel or a value is not finite.
pub fn door(spec: &DoorSpec, outer: Rect) -> Option<Door> {
    let outer = outer.abs();
    let f = spec.frame;
    if ![outer.x0, outer.y0, outer.x1, outer.y1, f]
        .into_iter()
        .all(finite)
        || f < 0.0
    {
        return None;
    }
    let r = Rect::new(outer.x0 + f, outer.y0 + f, outer.x1 - f, outer.y1 - f);
    if r.width() <= 1e-6 || r.height() <= 1e-6 {
        return None;
    }
    let inner = match spec.inner {
        InnerShape::Square => r.to_path(0.01),
        InnerShape::Rounded => {
            let rad = if finite(spec.radius) {
                spec.radius.clamp(0.0, r.width().min(r.height()) / 2.0)
            } else {
                0.0
            };
            RoundedRect::from_rect(r, rad).to_path(0.01)
        }
        InnerShape::Arch => arch_path(r, spec.arch),
    };
    let mut grooves = Vec::new();
    let join = if spec.inner == InnerShape::Square {
        Join::Miter
    } else {
        Join::Round
    };
    for k in 0..spec.grooves.min(100) {
        let d = if k == 0 {
            spec.groove_offset
        } else {
            spec.groove_offset + f64::from(k) * spec.groove_spacing
        };
        if !finite(d) || d <= 0.0 {
            continue;
        }
        let g = shaping::offset_with(&inner, -d, join);
        if g.elements().is_empty() {
            break;
        }
        grooves.push(g);
    }
    Some(Door {
        outer: outer.to_path(0.01),
        inner,
        grooves,
    })
}

/// `r` with its top replaced by a circular arch rising `rise` above the
/// sides (at most half the width and the whole height).
pub fn arch_path(r: Rect, rise: f64) -> BezPath {
    let w = r.width();
    let h = if finite(rise) {
        rise.clamp(0.0, (w / 2.0).min(r.height()))
    } else {
        0.0
    };
    if h <= 1e-9 {
        return r.to_path(0.01);
    }
    let c = w / 2.0;
    let radius = (c * c + h * h) / (2.0 * h);
    let spring = r.y1 - h;
    let center = Point::new(r.x0 + c, r.y1 - radius);
    let a0 = (spring - center.y).atan2(r.x1 - center.x);
    let a1 = (spring - center.y).atan2(r.x0 - center.x);
    let arc = Arc::new(center, Vec2::new(radius, radius), a0, a1 - a0, 0.0);
    let mut p = BezPath::new();
    p.move_to((r.x0, r.y0));
    p.line_to((r.x1, r.y0));
    p.line_to((r.x1, spring));
    for el in arc.append_iter(0.01) {
        p.push(el);
    }
    p.close_path();
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(p: &BezPath) -> f64 {
        p.area().abs()
    }

    #[test]
    fn square_door_with_grooves() {
        let spec = DoorSpec {
            grooves: 2,
            ..DoorSpec::default()
        };
        let d = door(&spec, Rect::new(0.0, 0.0, 400.0, 600.0)).expect("door");
        assert!((area(&d.outer) - 240_000.0).abs() < 1e-6);
        assert!((area(&d.inner) - 280.0 * 480.0).abs() < 1e-6);
        assert_eq!(d.grooves.len(), 2);
        let g0 = d.grooves[0].bounding_box();
        assert!((g0.x0 - 70.0).abs() < 1e-6 && (g0.width() - 260.0).abs() < 1e-6);
        let g1 = d.grooves[1].bounding_box();
        assert!((g1.x0 - 75.0).abs() < 1e-6 && (g1.height() - 450.0).abs() < 1e-6);
    }

    #[test]
    fn rounded_corners_take_off_their_corners() {
        let spec = DoorSpec {
            inner: InnerShape::Rounded,
            radius: 20.0,
            ..DoorSpec::default()
        };
        let d = door(&spec, Rect::new(0.0, 0.0, 400.0, 600.0)).expect("door");
        let want = 280.0 * 480.0 - (4.0 - std::f64::consts::PI) * 400.0;
        assert!((area(&d.inner) - want).abs() < 0.5, "{}", area(&d.inner));
    }

    #[test]
    fn arch_rises_to_the_top_of_the_panel() {
        let spec = DoorSpec {
            inner: InnerShape::Arch,
            arch: 60.0,
            ..DoorSpec::default()
        };
        let d = door(&spec, Rect::new(0.0, 0.0, 400.0, 600.0)).expect("door");
        let b = d.inner.bounding_box();
        assert!((b.y1 - 540.0).abs() < 1e-6 && (b.width() - 280.0).abs() < 1e-6);
        // The circle through the springing points and the crown.
        let (c, h) = (140.0_f64, 60.0_f64);
        let r = (c * c + h * h) / (2.0 * h);
        let half = (c / r).asin();
        let segment = r * r * (2.0 * half - (2.0 * half).sin()) / 2.0;
        let want = 280.0 * (480.0 - 60.0) + segment;
        assert!(
            (area(&d.inner) - want).abs() < 0.5,
            "{} {want}",
            area(&d.inner)
        );
        // A full half circle at most.
        let p = arch_path(Rect::new(0.0, 0.0, 100.0, 100.0), 500.0);
        assert!((p.bounding_box().y1 - 100.0).abs() < 1e-6);
        let half_disc = 100.0 * 50.0 + std::f64::consts::PI * 2500.0 / 2.0;
        assert!((area(&p) - half_disc).abs() < 0.1);
    }

    #[test]
    fn a_frame_too_wide_gives_nothing() {
        let spec = DoorSpec {
            frame: 200.0,
            ..DoorSpec::default()
        };
        assert!(door(&spec, Rect::new(0.0, 0.0, 400.0, 600.0)).is_none());
        let bad = DoorSpec {
            frame: f64::NAN,
            ..DoorSpec::default()
        };
        assert!(door(&bad, Rect::new(0.0, 0.0, 400.0, 600.0)).is_none());
        assert!(door(
            &DoorSpec::default(),
            Rect::new(0.0, 0.0, f64::INFINITY, 1.0)
        )
        .is_none());
        let odd = DoorSpec {
            radius: f64::NAN,
            arch: f64::NAN,
            grooves: 3,
            groove_spacing: f64::NAN,
            inner: InnerShape::Arch,
            ..DoorSpec::default()
        };
        let d = door(&odd, Rect::new(0.0, 0.0, 400.0, 600.0)).expect("door");
        assert_eq!(d.grooves.len(), 1);
    }
}
