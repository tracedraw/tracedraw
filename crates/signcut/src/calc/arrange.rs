//! Geometry of the Set Size and Replace windows: the ways Set Size
//! works out each object's new size (the largest or smallest of the
//! selection, a size, a change by a distance, a scale), an optional
//! turn, and which of several master objects Replace puts in each place.
//!
//! Built on [`crate::sizing`]: every size goes through
//! [`sizing::size_transform`], so the rule (every object, only larger,
//! only smaller), the lock of the proportions and the fixed point work
//! the same in every mode.

use crate::sizing::{self, Anchor, Frame, SizeRule, SizeSpec};
use tracedraw_core::geometry::{Affine, Rect};

/// How Set Size works out the size of each object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SizeMode {
    /// Every object as large as the largest one (its width and height).
    Largest,
    /// Every object as small as the smallest one.
    Smallest,
    /// A width and height.
    #[default]
    Size,
    /// The object's own size plus a distance on each side.
    Change,
    /// The object's own size times a percentage.
    Scale,
}

impl SizeMode {
    pub const ALL: [SizeMode; 5] = [
        SizeMode::Largest,
        SizeMode::Smallest,
        SizeMode::Size,
        SizeMode::Change,
        SizeMode::Scale,
    ];
}

/// Which sides a Set Size button changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sides {
    Width,
    Height,
    Both,
}

/// Everything Set Size needs (lengths in mm, the scale in percent, the
/// angle in degrees counter-clockwise).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResizeSpec {
    pub mode: SizeMode,
    pub width: f64,
    pub height: f64,
    pub dx: f64,
    pub dy: f64,
    pub scale: f64,
    /// Keep the proportions (with both sides the object fits inside).
    pub lock_ratio: bool,
    pub rule: SizeRule,
    pub anchor: Anchor,
    /// Turn every object by `angle` about the fixed point too.
    pub rotate: bool,
    pub angle: f64,
}

impl Default for ResizeSpec {
    fn default() -> Self {
        ResizeSpec {
            mode: SizeMode::Size,
            width: 100.0,
            height: 100.0,
            dx: 0.0,
            dy: 0.0,
            scale: 100.0,
            lock_ratio: true,
            rule: SizeRule::Exact,
            anchor: Anchor::Center,
            rotate: false,
            angle: 0.0,
        }
    }
}

/// Smallest side that counts as a size (mm).
const TINY: f64 = 1e-9;

fn usable(r: &Rect) -> bool {
    let r = r.abs();
    r.x0.is_finite() && r.y0.is_finite() && r.x1.is_finite() && r.y1.is_finite()
}

/// The largest (`largest`) or smallest width and height above zero
/// among `bounds`, each taken on its own.
fn extreme(bounds: &[Rect], largest: bool) -> (Option<f64>, Option<f64>) {
    let pick = |vals: Vec<f64>| -> Option<f64> {
        vals.into_iter()
            .filter(|v| v.is_finite() && *v > TINY)
            .reduce(|a, b| if largest { a.max(b) } else { a.min(b) })
    };
    let ok: Vec<Rect> = bounds
        .iter()
        .filter(|r| usable(r))
        .map(|r| r.abs())
        .collect();
    (
        pick(ok.iter().map(|r| r.width()).collect()),
        pick(ok.iter().map(|r| r.height()).collect()),
    )
}

/// The width and height wanted for an object with bounds `b`, before the
/// sides and the rule are applied.
fn wanted(
    spec: &ResizeSpec,
    b: Rect,
    extremes: (Option<f64>, Option<f64>),
) -> (Option<f64>, Option<f64>) {
    let positive = |v: f64| (v.is_finite() && v > TINY).then_some(v);
    let (w, h) = (b.width(), b.height());
    match spec.mode {
        SizeMode::Largest | SizeMode::Smallest => extremes,
        SizeMode::Size => (positive(spec.width), positive(spec.height)),
        SizeMode::Change => (positive(w + spec.dx), positive(h + spec.dy)),
        SizeMode::Scale => {
            let f = positive(spec.scale).map(|s| s / 100.0);
            (
                f.and_then(|f| positive(w * f)),
                f.and_then(|f| positive(h * f)),
            )
        }
    }
}

/// The page-space transform of each object of `bounds` (one per entry,
/// in order), or `None` where the object stays as it is. Objects are
/// sized on their own about the fixed point of their own box; with
/// `rotate` each is then turned about the same point of its new box.
pub fn resize_transforms(bounds: &[Rect], spec: &ResizeSpec, sides: Sides) -> Vec<Option<Affine>> {
    let extremes = match spec.mode {
        SizeMode::Largest => extreme(bounds, true),
        SizeMode::Smallest => extreme(bounds, false),
        _ => (None, None),
    };
    let angle = spec.angle.to_radians().rem_euclid(std::f64::consts::TAU);
    let turn = spec.rotate && angle.is_finite() && angle.abs() > TINY;
    bounds
        .iter()
        .map(|b| {
            if !usable(b) {
                return None;
            }
            let b = b.abs();
            let (w, h) = wanted(spec, b, extremes);
            let (w, h) = match sides {
                Sides::Width => (w, None),
                Sides::Height => (None, h),
                Sides::Both => (w, h),
            };
            let size = SizeSpec {
                width: w,
                height: h,
                proportional: spec.lock_ratio,
                rule: spec.rule,
                anchor: spec.anchor,
            };
            let scaled = sizing::size_transform(b, &size);
            if !turn {
                return scaled;
            }
            let t = scaled.unwrap_or(Affine::IDENTITY);
            let nb = t.transform_rect_bbox(b);
            let at = spec.anchor.point(nb).to_vec2();
            let r = Affine::translate(at) * Affine::rotate(angle) * Affine::translate(-at);
            Some(r * t)
        })
        .collect()
}

/// How Replace picks one of several masters for each object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MasterPick {
    /// The masters one after another, in the order they were set.
    #[default]
    InTurn,
    /// The master whose proportions (width to height) are closest to the
    /// object's.
    ClosestShape,
}

/// The proportions of a frame as a logarithm (0 for a square), or `None`
/// for a flat or broken one.
fn aspect(f: &Frame) -> Option<f64> {
    let (w, h) = (f.size.x.abs(), f.size.y.abs());
    (w.is_finite() && h.is_finite() && w > TINY && h > TINY).then(|| (w / h).ln())
}

/// The index of the master (of `masters`, their frames) for the
/// `index`-th object to replace, whose frame is `target`. `None` without
/// masters.
pub fn pick_master(
    masters: &[Frame],
    target: &Frame,
    index: usize,
    pick: MasterPick,
) -> Option<usize> {
    if masters.is_empty() {
        return None;
    }
    let in_turn = index % masters.len();
    match pick {
        MasterPick::InTurn => Some(in_turn),
        MasterPick::ClosestShape => {
            let Some(t) = aspect(target) else {
                return Some(in_turn);
            };
            masters
                .iter()
                .enumerate()
                .filter_map(|(i, m)| aspect(m).map(|a| (i, (a - t).abs())))
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i)
                .or(Some(in_turn))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::{Point, Vec2};

    fn r(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect::new(x, y, x + w, y + h)
    }

    fn after(t: Option<Affine>, b: Rect) -> Rect {
        t.map_or(b, |t| t.transform_rect_bbox(b))
    }

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn largest_makes_every_object_as_large_as_the_largest() {
        let b = [r(0.0, 0.0, 10.0, 5.0), r(20.0, 0.0, 40.0, 20.0)];
        let spec = ResizeSpec {
            mode: SizeMode::Largest,
            lock_ratio: false,
            anchor: Anchor::BottomLeft,
            ..Default::default()
        };
        let t = resize_transforms(&b, &spec, Sides::Both);
        let a = after(t[0], b[0]);
        assert!(near(a.width(), 40.0) && near(a.height(), 20.0));
        assert!(near(a.x0, 0.0) && near(a.y0, 0.0));
        // The largest one stays.
        assert!(t[1].is_none());
    }

    #[test]
    fn smallest_with_the_ratio_locked_fits_inside() {
        let b = [r(0.0, 0.0, 10.0, 5.0), r(20.0, 0.0, 40.0, 40.0)];
        let spec = ResizeSpec {
            mode: SizeMode::Smallest,
            lock_ratio: true,
            ..Default::default()
        };
        let t = resize_transforms(&b, &spec, Sides::Both);
        assert!(t[0].is_none());
        let a = after(t[1], b[1]);
        // Inside 10 x 5 with its proportions: 5 x 5.
        assert!(near(a.width(), 5.0) && near(a.height(), 5.0));
        // About the centre.
        assert!(near(a.center().x, 40.0) && near(a.center().y, 20.0));
    }

    #[test]
    fn width_only_leaves_the_height_unless_the_ratio_is_locked() {
        let b = [r(0.0, 0.0, 10.0, 5.0)];
        let mut spec = ResizeSpec {
            width: 30.0,
            height: 7.0,
            lock_ratio: false,
            ..Default::default()
        };
        let a = after(resize_transforms(&b, &spec, Sides::Width)[0], b[0]);
        assert!(near(a.width(), 30.0) && near(a.height(), 5.0));
        let a = after(resize_transforms(&b, &spec, Sides::Height)[0], b[0]);
        assert!(near(a.width(), 10.0) && near(a.height(), 7.0));
        spec.lock_ratio = true;
        let a = after(resize_transforms(&b, &spec, Sides::Width)[0], b[0]);
        assert!(near(a.width(), 30.0) && near(a.height(), 15.0));
    }

    #[test]
    fn change_adds_a_distance_and_scale_a_percentage() {
        let b = [r(0.0, 0.0, 10.0, 5.0)];
        let spec = ResizeSpec {
            mode: SizeMode::Change,
            dx: 5.0,
            dy: -1.0,
            lock_ratio: false,
            ..Default::default()
        };
        let a = after(resize_transforms(&b, &spec, Sides::Both)[0], b[0]);
        assert!(near(a.width(), 15.0) && near(a.height(), 4.0));
        // A change that leaves nothing keeps that side.
        let spec = ResizeSpec { dx: -20.0, ..spec };
        let a = after(resize_transforms(&b, &spec, Sides::Both)[0], b[0]);
        assert!(near(a.width(), 10.0) && near(a.height(), 4.0));
        let spec = ResizeSpec {
            mode: SizeMode::Scale,
            scale: 250.0,
            ..Default::default()
        };
        let a = after(resize_transforms(&b, &spec, Sides::Both)[0], b[0]);
        assert!(near(a.width(), 25.0) && near(a.height(), 12.5));
    }

    #[test]
    fn rotate_turns_about_the_fixed_point_even_without_a_size() {
        let b = [r(0.0, 0.0, 10.0, 4.0)];
        let spec = ResizeSpec {
            mode: SizeMode::Scale,
            scale: 100.0,
            rotate: true,
            angle: 90.0,
            anchor: Anchor::BottomLeft,
            ..Default::default()
        };
        let t = resize_transforms(&b, &spec, Sides::Both)[0].expect("a turn");
        let p = t * Point::new(10.0, 0.0);
        assert!(near(p.x, 0.0) && near(p.y, 10.0));
        let p = t * Point::ZERO;
        assert!(near(p.x, 0.0) && near(p.y, 0.0));
        // Off: nothing to do.
        let spec = ResizeSpec {
            rotate: false,
            ..spec
        };
        assert!(resize_transforms(&b, &spec, Sides::Both)[0].is_none());
    }

    #[test]
    fn bad_numbers_and_flat_objects_do_not_panic() {
        let b = [
            r(0.0, 0.0, 10.0, 0.0),
            Rect::new(f64::NAN, 0.0, 1.0, 1.0),
            r(0.0, 0.0, f64::INFINITY, 1.0),
        ];
        for mode in SizeMode::ALL {
            let spec = ResizeSpec {
                mode,
                width: f64::NAN,
                height: -3.0,
                dx: f64::INFINITY,
                dy: f64::NAN,
                scale: f64::NAN,
                rotate: true,
                angle: f64::NAN,
                ..Default::default()
            };
            let t = resize_transforms(&b, &spec, Sides::Both);
            assert_eq!(t.len(), 3);
            assert!(t[1].is_none() && t[2].is_none());
        }
        // A flat line keeps its zero height.
        let spec = ResizeSpec {
            width: 20.0,
            ..Default::default()
        };
        let a = after(resize_transforms(&b[..1], &spec, Sides::Both)[0], b[0]);
        assert!(near(a.width(), 20.0) && near(a.height(), 0.0));
        assert!(resize_transforms(&[], &spec, Sides::Both).is_empty());
    }

    fn frame(w: f64, h: f64) -> Frame {
        Frame {
            center: Point::ZERO,
            size: Vec2::new(w, h),
            angle: 0.0,
        }
    }

    #[test]
    fn masters_are_picked_in_turn_or_by_proportions() {
        let masters = [frame(10.0, 10.0), frame(40.0, 10.0)];
        let wide = frame(80.0, 20.0);
        assert_eq!(pick_master(&masters, &wide, 0, MasterPick::InTurn), Some(0));
        assert_eq!(pick_master(&masters, &wide, 3, MasterPick::InTurn), Some(1));
        assert_eq!(
            pick_master(&masters, &wide, 0, MasterPick::ClosestShape),
            Some(1)
        );
        assert_eq!(
            pick_master(&masters, &frame(5.0, 6.0), 1, MasterPick::ClosestShape),
            Some(0)
        );
        // A flat target or flat masters fall back to the turn.
        assert_eq!(
            pick_master(&masters, &frame(5.0, 0.0), 1, MasterPick::ClosestShape),
            Some(1)
        );
        assert_eq!(
            pick_master(&[frame(f64::NAN, 1.0)], &wide, 0, MasterPick::ClosestShape),
            Some(0)
        );
        assert_eq!(pick_master(&[], &wide, 0, MasterPick::InTurn), None);
    }
}
