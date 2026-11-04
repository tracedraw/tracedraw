//! Set Size, Reference Scale and Replace: the transforms that size and
//! place objects, worked out from bounds alone.

use tracedraw_core::geometry::{Affine, Point, Rect, Vec2};

/// The point of a box that stays put while it is resized.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Anchor {
    TopLeft,
    Top,
    TopRight,
    Left,
    #[default]
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl Anchor {
    /// The nine anchors, row by row from the top left.
    pub const ALL: [Anchor; 9] = [
        Anchor::TopLeft,
        Anchor::Top,
        Anchor::TopRight,
        Anchor::Left,
        Anchor::Center,
        Anchor::Right,
        Anchor::BottomLeft,
        Anchor::Bottom,
        Anchor::BottomRight,
    ];

    /// The anchor's point on `r` (page space, y up: top is the larger y).
    pub fn point(self, r: Rect) -> Point {
        let r = r.abs();
        let (x0, x1, y0, y1) = (r.x0, r.x1, r.y0, r.y1);
        let xm = (x0 + x1) / 2.0;
        let ym = (y0 + y1) / 2.0;
        match self {
            Anchor::TopLeft => Point::new(x0, y1),
            Anchor::Top => Point::new(xm, y1),
            Anchor::TopRight => Point::new(x1, y1),
            Anchor::Left => Point::new(x0, ym),
            Anchor::Center => Point::new(xm, ym),
            Anchor::Right => Point::new(x1, ym),
            Anchor::BottomLeft => Point::new(x0, y0),
            Anchor::Bottom => Point::new(xm, y0),
            Anchor::BottomRight => Point::new(x1, y0),
        }
    }
}

/// Which objects Set Size changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SizeRule {
    /// Every object gets the size.
    #[default]
    Exact,
    /// Only objects larger than the size shrink to it (a maximum size).
    ShrinkOnly,
    /// Only objects smaller than the size grow to it (a minimum size).
    GrowOnly,
}

/// What Set Size does to each object.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SizeSpec {
    /// The width wanted, or `None` to leave it to the height.
    pub width: Option<f64>,
    /// The height wanted, or `None` to leave it to the width.
    pub height: Option<f64>,
    /// Keep the proportions: one factor for both sides (with both sides
    /// given the object fits inside them).
    pub proportional: bool,
    pub rule: SizeRule,
    pub anchor: Anchor,
}

impl Default for SizeSpec {
    fn default() -> Self {
        SizeSpec {
            width: Some(100.0),
            height: None,
            proportional: true,
            rule: SizeRule::Exact,
            anchor: Anchor::Center,
        }
    }
}

const TINY: f64 = 1e-9;

/// The page-space transform that gives an object with `bounds` the size
/// of `spec`, or `None` when it stays as it is.
pub fn size_transform(bounds: Rect, spec: &SizeSpec) -> Option<Affine> {
    let b = bounds.abs();
    let (w, h) = (b.width(), b.height());
    if !(w.is_finite() && h.is_finite()) {
        return None;
    }
    let fx = spec
        .width
        .filter(|v| v.is_finite() && *v > TINY && w > TINY)
        .map(|v| v / w);
    let fy = spec
        .height
        .filter(|v| v.is_finite() && *v > TINY && h > TINY)
        .map(|v| v / h);
    let limit = |f: f64| match spec.rule {
        SizeRule::Exact => f,
        SizeRule::ShrinkOnly => f.min(1.0),
        SizeRule::GrowOnly => f.max(1.0),
    };
    let (sx, sy) = if spec.proportional {
        let f = match (fx, fy) {
            (Some(a), Some(b)) => a.min(b),
            (Some(a), None) | (None, Some(a)) => a,
            (None, None) => return None,
        };
        let f = limit(f);
        (f, f)
    } else {
        (limit(fx.unwrap_or(1.0)), limit(fy.unwrap_or(1.0)))
    };
    if (sx - 1.0).abs() < TINY && (sy - 1.0).abs() < TINY {
        return None;
    }
    Some(scale_about(spec.anchor.point(b), sx, sy))
}

/// Scale by `sx`, `sy` keeping `at` in place.
pub fn scale_about(at: Point, sx: f64, sy: f64) -> Affine {
    Affine::translate(at.to_vec2())
        * Affine::scale_non_uniform(sx, sy)
        * Affine::translate(-at.to_vec2())
}

/// What Reference Scale measures on the reference object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RefMeasure {
    #[default]
    Width,
    Height,
    /// Outline length (a dimension line, a scale bar, a curve).
    Length,
}

/// The value of `m` for an object with `bounds` and outline `length`.
pub fn reference_value(m: RefMeasure, bounds: Rect, length: f64) -> f64 {
    let b = bounds.abs();
    match m {
        RefMeasure::Width => b.width(),
        RefMeasure::Height => b.height(),
        RefMeasure::Length => length,
    }
}

/// The uniform factor that makes `current` become `wanted`.
pub fn reference_factor(current: f64, wanted: f64) -> Option<f64> {
    if !(current.is_finite() && wanted.is_finite()) || current <= TINY || wanted <= TINY {
        return None;
    }
    let f = wanted / current;
    ((f - 1.0).abs() > TINY).then_some(f)
}

/// How Replace sizes the master to each object it replaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReplaceFit {
    /// The master's own size.
    #[default]
    Keep,
    /// Fitted inside the object, proportions kept.
    Fit,
    /// Stretched to the object's width and height.
    Stretch,
}

/// A box with a turn: centre, size along its own axes and angle
/// (radians, counter-clockwise).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub center: Point,
    pub size: Vec2,
    pub angle: f64,
}

/// The turn of an object's transform (radians, counter-clockwise).
pub fn transform_angle(t: Affine) -> f64 {
    let [a, b, ..] = t.as_coeffs();
    b.atan2(a)
}

/// The page-space transform that puts a copy of the master (`master`)
/// in the place of `target`: centred on it, sized by `fit`, and turned by
/// the difference of their angles when `turn` is set.
pub fn replace_transform(master: Frame, target: Frame, fit: ReplaceFit, turn: bool) -> Affine {
    let (mw, mh) = (master.size.x.abs(), master.size.y.abs());
    let (tw, th) = (target.size.x.abs(), target.size.y.abs());
    let ratio = |t: f64, m: f64| if m > TINY && t > TINY { t / m } else { 1.0 };
    let (sx, sy) = match fit {
        ReplaceFit::Keep => (1.0, 1.0),
        ReplaceFit::Fit => {
            let f = match (mw > TINY && tw > TINY, mh > TINY && th > TINY) {
                (true, true) => (tw / mw).min(th / mh),
                (true, false) => tw / mw,
                (false, true) => th / mh,
                (false, false) => 1.0,
            };
            (f, f)
        }
        ReplaceFit::Stretch => (ratio(tw, mw), ratio(th, mh)),
    };
    let delta = if turn {
        target.angle - master.angle
    } else {
        0.0
    };
    // Undo the master's own turn, scale along its axes, turn to the target.
    let own = if turn { master.angle } else { 0.0 };
    Affine::translate(target.center.to_vec2())
        * Affine::rotate(own + delta)
        * Affine::scale_non_uniform(sx, sy)
        * Affine::rotate(-own)
        * Affine::translate(-master.center.to_vec2())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Point, b: Point) -> bool {
        (a - b).hypot() < 1e-9
    }

    #[test]
    fn anchors_sit_on_the_box_with_y_up() {
        let r = Rect::new(0.0, 0.0, 10.0, 20.0);
        assert!(close(Anchor::TopLeft.point(r), Point::new(0.0, 20.0)));
        assert!(close(Anchor::BottomRight.point(r), Point::new(10.0, 0.0)));
        assert!(close(Anchor::Center.point(r), Point::new(5.0, 10.0)));
    }

    #[test]
    fn width_alone_scales_proportionally_about_the_anchor() {
        let r = Rect::new(0.0, 0.0, 50.0, 20.0);
        let spec = SizeSpec {
            width: Some(100.0),
            height: None,
            proportional: true,
            rule: SizeRule::Exact,
            anchor: Anchor::BottomLeft,
        };
        let t = size_transform(r, &spec).expect("a change");
        let out = t.transform_rect_bbox(r);
        assert!((out.width() - 100.0).abs() < 1e-9);
        assert!((out.height() - 40.0).abs() < 1e-9);
        assert!(close(Point::new(out.x0, out.y0), Point::ZERO));
    }

    #[test]
    fn both_sides_proportional_fit_inside() {
        let r = Rect::new(0.0, 0.0, 50.0, 20.0);
        let spec = SizeSpec {
            width: Some(100.0),
            height: Some(30.0),
            ..Default::default()
        };
        let out = size_transform(r, &spec)
            .expect("a change")
            .transform_rect_bbox(r);
        assert!((out.width() - 75.0).abs() < 1e-9);
        assert!((out.height() - 30.0).abs() < 1e-9);
    }

    #[test]
    fn free_sizing_sets_each_side() {
        let r = Rect::new(0.0, 0.0, 50.0, 20.0);
        let spec = SizeSpec {
            width: Some(10.0),
            height: Some(10.0),
            proportional: false,
            ..Default::default()
        };
        let out = size_transform(r, &spec)
            .expect("a change")
            .transform_rect_bbox(r);
        assert!((out.width() - 10.0).abs() < 1e-9 && (out.height() - 10.0).abs() < 1e-9);
    }

    #[test]
    fn maximum_and_minimum_sizes_leave_the_others() {
        let small = Rect::new(0.0, 0.0, 50.0, 20.0);
        let big = Rect::new(0.0, 0.0, 150.0, 20.0);
        let max = SizeSpec {
            rule: SizeRule::ShrinkOnly,
            ..Default::default()
        };
        assert!(size_transform(small, &max).is_none());
        assert!(size_transform(big, &max).is_some());
        let min = SizeSpec {
            rule: SizeRule::GrowOnly,
            ..Default::default()
        };
        assert!(size_transform(small, &min).is_some());
        assert!(size_transform(big, &min).is_none());
    }

    #[test]
    fn a_flat_line_keeps_its_zero_height() {
        let r = Rect::new(0.0, 5.0, 40.0, 5.0);
        let spec = SizeSpec {
            width: Some(80.0),
            height: Some(10.0),
            proportional: false,
            ..Default::default()
        };
        let out = size_transform(r, &spec)
            .expect("a change")
            .transform_rect_bbox(r);
        assert!((out.width() - 80.0).abs() < 1e-9);
        assert!(out.height().abs() < 1e-9);
    }

    #[test]
    fn reference_scale_finds_the_factor() {
        let r = Rect::new(0.0, 0.0, 25.0, 10.0);
        let v = reference_value(RefMeasure::Width, r, 0.0);
        assert_eq!(reference_factor(v, 1000.0), Some(40.0));
        assert_eq!(reference_factor(v, 25.0), None);
        assert_eq!(reference_factor(0.0, 25.0), None);
        assert_eq!(reference_value(RefMeasure::Length, r, 7.5), 7.5);
    }

    #[test]
    fn replace_centres_and_fits_the_master() {
        let master = Frame {
            center: Point::new(0.0, 0.0),
            size: Vec2::new(10.0, 5.0),
            angle: 0.0,
        };
        let target = Frame {
            center: Point::new(100.0, 50.0),
            size: Vec2::new(40.0, 40.0),
            angle: 0.0,
        };
        let t = replace_transform(master, target, ReplaceFit::Fit, false);
        let out = t.transform_rect_bbox(Rect::new(-5.0, -2.5, 5.0, 2.5));
        assert!((out.width() - 40.0).abs() < 1e-9);
        assert!((out.height() - 20.0).abs() < 1e-9);
        assert!(close(out.center(), Point::new(100.0, 50.0)));
        let keep = replace_transform(master, target, ReplaceFit::Keep, false);
        assert!(
            (keep
                .transform_rect_bbox(Rect::new(-5.0, -2.5, 5.0, 2.5))
                .width()
                - 10.0)
                .abs()
                < 1e-9
        );
    }

    #[test]
    fn replace_takes_the_turn_of_the_object() {
        let master = Frame {
            center: Point::ZERO,
            size: Vec2::new(10.0, 2.0),
            angle: 0.0,
        };
        let target = Frame {
            center: Point::ZERO,
            size: Vec2::new(10.0, 2.0),
            angle: std::f64::consts::FRAC_PI_2,
        };
        let t = replace_transform(master, target, ReplaceFit::Keep, true);
        let out = t.transform_rect_bbox(Rect::new(-5.0, -1.0, 5.0, 1.0));
        assert!((out.width() - 2.0).abs() < 1e-9 && (out.height() - 10.0).abs() < 1e-9);
        assert!((transform_angle(Affine::rotate(0.3)) - 0.3).abs() < 1e-12);
    }
}
