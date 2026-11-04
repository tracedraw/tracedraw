//! The flat panels of a box joined by finger joints, for a laser or a
//! router.
//!
//! The box is W (x) by D (y) by H (z) outside. Each of its six panels
//! is a slab of the material thickness T; where two panels meet, the
//! T x T strip along the edge is split into an odd number of equal
//! fingers between the edge's corner cubes, given in turn to the two
//! panels (the end fingers to the panel of lower rank: bottom and top,
//! then left and right, then front and back). The eight corner cubes
//! belong to the front and back panels. Every bit of the box's shell
//! is therefore in exactly one panel and the panels interlock.

use super::{signed_area, Contour};
use tracedraw_core::geometry::{BezPath, Point, Vec2};
use tracedraw_core::shaping::{self, Join};

/// What to build.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct BoxSpec {
    /// Width (x), depth (y) and height (z), mm.
    pub width: f64,
    pub depth: f64,
    pub height: f64,
    /// Material thickness (mm).
    pub thickness: f64,
    /// Wanted finger width (mm); the real one divides the edge evenly.
    pub finger: f64,
    /// Width the cut takes away (mm): each outline grows by half of it.
    pub kerf: f64,
    /// The sizes are inside sizes (else outside).
    pub inside: bool,
    /// No top panel.
    pub open_top: bool,
    /// Space between the laid out panels (mm).
    pub gap: f64,
}

impl Default for BoxSpec {
    fn default() -> Self {
        BoxSpec {
            width: 200.0,
            depth: 150.0,
            height: 100.0,
            thickness: 3.0,
            finger: 10.0,
            kerf: 0.1,
            inside: false,
            open_top: false,
            gap: 5.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Bottom,
    Top,
    Front,
    Back,
    Left,
    Right,
}

impl Panel {
    fn rank(self) -> u8 {
        match self {
            Panel::Bottom | Panel::Top => 1,
            Panel::Left | Panel::Right => 2,
            Panel::Front | Panel::Back => 3,
        }
    }

    /// Front and back own the corner cubes.
    fn owns_corners(self) -> bool {
        self.rank() == 3
    }
}

/// The number of fingers along an edge of outside length `len`: odd,
/// their width nearest to `finger` over the length between the corner
/// cubes (`len - 2 t`).
pub fn finger_count(len: f64, t: f64, finger: f64) -> usize {
    let inner = len - 2.0 * t;
    if !(inner.is_finite() && finger.is_finite()) || inner <= 0.0 || finger <= 0.0 {
        return 1;
    }
    let n = (inner / finger).round().clamp(1.0, 999.0) as usize;
    if n % 2 == 1 {
        return n;
    }
    let lo = n - 1;
    let hi = n + 1;
    let err = |k: usize| (inner / k as f64 - finger).abs();
    if lo >= 1 && err(lo) <= err(hi) {
        lo
    } else {
        hi
    }
}

/// The outside sizes (W, D, H) of the box.
pub fn outside_size(spec: &BoxSpec) -> (f64, f64, f64) {
    let t = spec.thickness;
    if spec.inside {
        let top = if spec.open_top { t } else { 2.0 * t };
        (
            spec.width + 2.0 * t,
            spec.depth + 2.0 * t,
            spec.height + top,
        )
    } else {
        (spec.width, spec.depth, spec.height)
    }
}

/// The outline of one panel in its own frame (u along its width, v up
/// its height, the outside rectangle from 0 to `lu`, `lv`), its edges
/// bottom, right, top and left meeting `next` (`None`: a free edge).
fn panel_outline(
    p: Panel,
    lu: f64,
    lv: f64,
    next: [Option<Panel>; 4],
    spec: &BoxSpec,
) -> Vec<Point> {
    let t = spec.thickness;
    let corner = if p.owns_corners() { 0.0 } else { t };
    let mut pts: Vec<Point> = Vec::new();
    for (side, q) in next.iter().enumerate() {
        let len = if side % 2 == 0 { lu } else { lv };
        let at = |s: f64, d: f64| match side {
            0 => Point::new(s, d),
            1 => Point::new(lu - d, s),
            2 => Point::new(lu - s, lv - d),
            _ => Point::new(d, lv - s),
        };
        // The corner at the start of the edge.
        if corner == 0.0 {
            pts.push(at(0.0, 0.0));
        }
        pts.push(at(t, corner));
        let n = finger_count(len, t, spec.finger);
        let f = (len - 2.0 * t) / n as f64;
        for k in 0..n {
            let mine = match q {
                None => true,
                Some(q) => {
                    if k % 2 == 0 {
                        p.rank() < q.rank()
                    } else {
                        p.rank() > q.rank()
                    }
                }
            };
            let d = if mine { 0.0 } else { t };
            pts.push(at(t + k as f64 * f, d));
            pts.push(at(t + (k + 1) as f64 * f, d));
        }
        pts.push(at(len - t, corner));
        if corner == 0.0 {
            pts.push(at(len, 0.0));
        }
    }
    clean(pts)
}

/// Drop repeated points and points in the middle of a straight run.
fn clean(pts: Vec<Point>) -> Vec<Point> {
    let mut v: Vec<Point> = Vec::with_capacity(pts.len());
    for p in pts {
        if v.last().is_none_or(|l| (*l - p).hypot() > 1e-9) {
            v.push(p);
        }
    }
    if v.len() > 1
        && v.first()
            .zip(v.last())
            .is_some_and(|(a, b)| (*a - *b).hypot() <= 1e-9)
    {
        v.pop();
    }
    loop {
        let n = v.len();
        if n < 4 {
            return v;
        }
        let drop = (0..n).find(|&i| {
            let a = v[(i + n - 1) % n];
            let b = v[i];
            let c = v[(i + 1) % n];
            ((b - a).cross(c - b)).abs() < 1e-9
        });
        match drop {
            Some(i) => {
                v.remove(i);
            }
            None => return v,
        }
    }
}

/// The panels of the box laid out from `origin` (bottom left): front,
/// back, left and right in a row above bottom and top, `gap` apart.
/// Each is a closed outline in page space, grown by half the kerf.
/// `None` when the sizes cannot make a box.
pub fn box_panels(spec: &BoxSpec, origin: Point) -> Option<Vec<(Panel, BezPath)>> {
    let (w, d, h) = outside_size(spec);
    let t = spec.thickness;
    let ok = |v: f64| v.is_finite() && v > 2.0 * t + 1e-6;
    if !(t.is_finite() && t > 0.0 && ok(w) && ok(d) && ok(h))
        || !(spec.finger.is_finite() && spec.finger > 0.0)
        || !(origin.x.is_finite() && origin.y.is_finite())
    {
        return None;
    }
    let gap = if spec.gap.is_finite() {
        spec.gap.max(0.0)
    } else {
        0.0
    } + spec.kerf.max(0.0);
    let top = (!spec.open_top).then_some(Panel::Top);
    use Panel::*;
    // Panel, its size and the panels at its bottom, right, top and left.
    let mut list: Vec<(Panel, f64, f64, [Option<Panel>; 4])> = vec![
        (Front, w, h, [Some(Bottom), Some(Right), top, Some(Left)]),
        (Back, w, h, [Some(Bottom), Some(Left), top, Some(Right)]),
        (Left, d, h, [Some(Bottom), Some(Front), top, Some(Back)]),
        (Right, d, h, [Some(Bottom), Some(Back), top, Some(Front)]),
        (
            Bottom,
            w,
            d,
            [Some(Front), Some(Right), Some(Back), Some(Left)],
        ),
    ];
    if !spec.open_top {
        list.push((
            Top,
            w,
            d,
            [Some(Front), Some(Right), Some(Back), Some(Left)],
        ));
    }
    let mut out = Vec::new();
    let mut x = origin.x;
    let mut x2 = origin.x;
    for (p, lu, lv, next) in list {
        let pts = panel_outline(p, lu, lv, next, spec);
        let lower = matches!(p, Bottom | Top);
        let at = if lower {
            let at = Vec2::new(x2, origin.y);
            x2 += lu + gap;
            at
        } else {
            let at = Vec2::new(x, origin.y + d + gap);
            x += lu + gap;
            at
        };
        let c = Contour {
            pts: pts.into_iter().map(|q| q + at).collect(),
            closed: true,
        };
        let mut path = c.to_path();
        if spec.kerf.is_finite() && spec.kerf > 0.0 {
            path = shaping::offset_with(&path, spec.kerf / 2.0, Join::Miter);
        }
        out.push((p, path));
    }
    Some(out)
}

/// Area of a panel outline (for checks).
pub fn outline_area(path: &BezPath) -> f64 {
    super::flatten(path, 0.01)
        .iter()
        .map(|c| signed_area(&c.pts))
        .sum::<f64>()
        .abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(w: f64, d: f64, h: f64) -> BoxSpec {
        BoxSpec {
            width: w,
            depth: d,
            height: h,
            thickness: 3.0,
            finger: 10.0,
            kerf: 0.0,
            inside: false,
            open_top: false,
            gap: 5.0,
        }
    }

    #[test]
    fn finger_counts_are_odd_and_near_the_width() {
        assert_eq!(finger_count(100.0, 3.0, 10.0), 9);
        assert_eq!(finger_count(106.0, 3.0, 10.0), 11);
        assert_eq!(finger_count(26.0, 3.0, 10.0), 3);
        assert_eq!(finger_count(16.0, 3.0, 10.0), 1);
        assert_eq!(finger_count(5.0, 3.0, 10.0), 1);
        assert_eq!(finger_count(f64::NAN, 3.0, 10.0), 1);
        assert_eq!(finger_count(100.0, 3.0, 0.0), 1);
    }

    /// The panels fill the box's shell exactly once: their areas times
    /// the thickness are its volume.
    #[test]
    fn panels_fill_the_shell_exactly() {
        for (w, d, h) in [
            (100.0, 80.0, 60.0),
            (200.0, 150.0, 100.0),
            (47.0, 31.0, 29.0),
        ] {
            let s = spec(w, d, h);
            let p = box_panels(&s, Point::ZERO).expect("panels");
            assert_eq!(p.len(), 6);
            let area: f64 = p.iter().map(|(_, q)| outline_area(q)).sum();
            let t = 3.0;
            let shell = w * d * h - (w - 2.0 * t) * (d - 2.0 * t) * (h - 2.0 * t);
            assert!(
                (area * t - shell).abs() < 1e-6,
                "{w} {d} {h}: {}",
                area * t - shell
            );
        }
    }

    #[test]
    fn open_top_and_inside_sizes() {
        let mut s = spec(94.0, 74.0, 57.0);
        s.inside = true;
        s.open_top = true;
        assert_eq!(outside_size(&s), (100.0, 80.0, 60.0));
        let p = box_panels(&s, Point::ZERO).expect("panels");
        assert_eq!(p.len(), 5);
        assert!(p.iter().all(|(q, _)| *q != Panel::Top));
        let area: f64 = p.iter().map(|(_, q)| outline_area(q)).sum();
        let shell = 100.0 * 80.0 * 60.0 - 94.0 * 74.0 * 57.0;
        assert!((area * 3.0 - shell).abs() < 1e-6);
    }

    #[test]
    fn a_front_panel_has_full_corners_and_slots_along_its_bottom() {
        let s = spec(100.0, 80.0, 60.0);
        let p = box_panels(&s, Point::ZERO).expect("panels");
        let (_, front) = &p[0];
        let c = super::super::flatten(front, 0.01);
        assert_eq!(c.len(), 1);
        // Laid out above the bottom row: from y = 80 + 5.
        let b = tracedraw_core::geometry::Shape::bounding_box(front);
        assert!((b.x0 - 0.0).abs() < 1e-9 && (b.y0 - 85.0).abs() < 1e-9);
        assert!((b.width() - 100.0).abs() < 1e-9 && (b.height() - 60.0).abs() < 1e-9);
        // Bottom edge: corner, then 9 fingers of 94 / 9: the first
        // (the bottom panel's) is a slot 3 mm up.
        let f = 94.0 / 9.0;
        assert!(c[0].pts.contains(&Point::new(0.0, 85.0)));
        assert!(c[0].pts.contains(&Point::new(3.0, 88.0)));
        assert!(c[0].pts.contains(&Point::new(3.0 + f, 85.0)));
    }

    #[test]
    fn kerf_grows_every_outline() {
        let mut s = spec(100.0, 80.0, 60.0);
        let a: f64 = box_panels(&s, Point::ZERO)
            .expect("panels")
            .iter()
            .map(|(_, q)| outline_area(q))
            .sum();
        s.kerf = 0.2;
        let b: f64 = box_panels(&s, Point::ZERO)
            .expect("panels")
            .iter()
            .map(|(_, q)| outline_area(q))
            .sum();
        assert!(b > a + 10.0);
    }

    #[test]
    fn impossible_sizes_give_nothing() {
        assert!(box_panels(&spec(5.0, 80.0, 60.0), Point::ZERO).is_none());
        assert!(box_panels(&spec(f64::NAN, 80.0, 60.0), Point::ZERO).is_none());
        let mut s = spec(100.0, 80.0, 60.0);
        s.thickness = 0.0;
        assert!(box_panels(&s, Point::ZERO).is_none());
        s.thickness = 3.0;
        s.finger = f64::INFINITY;
        assert!(box_panels(&s, Point::ZERO).is_none());
        assert!(box_panels(&spec(100.0, 80.0, 60.0), Point::new(f64::NAN, 0.0)).is_none());
    }
}
