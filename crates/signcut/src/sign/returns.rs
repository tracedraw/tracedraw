//! Letter returns (edge strips) and liquid acrylic moulds.
//!
//! Every closed outline of a letter gets a strip: its length is the
//! outline's length on the chosen face of the strip material, plus an
//! overlap to join the ends; its height is the return depth. The face:
//!
//! - Inner: the outline's own length (the strip's face lying on it).
//! - Middle: the strip's centre, `t / 2` away from the outline.
//! - Outer: the strip's far face, `t` away.
//!
//! Away from the letter means outwards for an outer outline and into the
//! hole for a hole, so the length is `L + 2 pi s` for an outer outline
//! and `L - 2 pi s` for a hole, with `s` the face's distance (exact for
//! convex outlines with the corners rounded by the material, close for
//! others). The strip starts at the outline's first node. A V notch on
//! its top edge marks every corner sharper than the notch angle (the
//! angle between the two sides meeting at a node; 180 is straight), at
//! the corner's place along the strip (scaled with the strip's length);
//! or short marks there ([`strip_marks`]). With a bend compensation `c`
//! the strip is `c` shorter at each such corner (the n-th corner moves
//! `c (n - 1/2)` towards the start). A strip longer than the maximum
//! length is split into the fewest equal pieces whose length plus the
//! overlap keeps under it; each piece gets the overlap.
//!
//! A liquid acrylic mould is the letter offset outwards by the wall
//! thickness (the base) and a strip on the middle face of the wall, as
//! high as the fill depth; optionally an inner contour, the letter offset
//! inwards.

use super::{finite_path, hole_flags};
use crate::measure;
use tracedraw_core::geometry::{BezPath, ParamCurveArclen as _, PathSeg, Point, Shape as _, Vec2};
use tracedraw_core::shaping::{self, Join};

/// Which face of the strip gets the outline's length.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum ReturnFace {
    #[default]
    Inner,
    Middle,
    Outer,
}

/// How the corners show on a strip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum ReturnMethod {
    /// V notches cut into the top edge.
    #[default]
    Notched,
    /// Short marks at both edges (for bending by hand or on a bender).
    Marked,
    /// A plain strip.
    Plain,
}

/// The return settings (millimetres, degrees).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ReturnSpec {
    /// The strip's height.
    pub depth: f64,
    /// Added to the length to join the ends.
    pub overlap: f64,
    /// Material thickness.
    pub thickness: f64,
    pub face: ReturnFace,
    /// Corners whose angle is under this get a notch (0: none).
    pub notch_angle: f64,
    /// Width and depth of a notch.
    pub notch_size: f64,
    /// Strips longer than this are split into equal pieces, each with
    /// the overlap (0: no limit).
    pub max_length: f64,
    /// Length the material gains at each bend: the strip is this much
    /// shorter at every notched corner (0: none).
    pub bend_comp: f64,
}

impl Default for ReturnSpec {
    fn default() -> Self {
        ReturnSpec {
            depth: 50.0,
            overlap: 10.0,
            thickness: 1.0,
            face: ReturnFace::Inner,
            notch_angle: 135.0,
            notch_size: 3.0,
            max_length: 0.0,
            bend_comp: 0.0,
        }
    }
}

/// One strip.
#[derive(Debug, Clone, PartialEq)]
pub struct Strip {
    /// Index of the object and of the outline in it (from 0).
    pub object: usize,
    pub contour: usize,
    /// The outline is a hole.
    pub hole: bool,
    /// The outline's own length.
    pub outline_length: f64,
    /// The cut length: the face's length plus the overlap.
    pub length: f64,
    pub height: f64,
    /// Places of the notches along the strip.
    pub notches: Vec<f64>,
    /// The piece of a split strip (from 0) and how many there are.
    pub part: usize,
    pub parts: usize,
}

/// Most strips one run makes.
pub const MAX_STRIPS: usize = 5_000;

fn clean(v: f64) -> f64 {
    if v.is_finite() {
        v.max(0.0)
    } else {
        0.0
    }
}

/// The strips of the closed outlines of each object.
pub fn returns(objects: &[Vec<BezPath>], spec: &ReturnSpec) -> Vec<Strip> {
    let height = clean(spec.depth);
    if height <= 1e-9 {
        return Vec::new();
    }
    let t = clean(spec.thickness);
    let s = match spec.face {
        ReturnFace::Inner => 0.0,
        ReturnFace::Middle => t / 2.0,
        ReturnFace::Outer => t,
    };
    let overlap = clean(spec.overlap);
    let mut out = Vec::new();
    for (oi, paths) in objects.iter().enumerate() {
        let closed: Vec<BezPath> = paths
            .iter()
            .filter(|p| finite_path(p))
            .flat_map(crate::subpaths)
            .filter(|sp| sp.closed)
            .map(|sp| sp.path)
            .collect();
        let holes = hole_flags(&closed);
        for (ci, path) in closed.iter().enumerate() {
            let len = measure::length(path);
            if len.is_nan() || len <= 1e-6 {
                continue;
            }
            let hole = holes.get(ci).copied().unwrap_or(false);
            let sign = if hole { -1.0 } else { 1.0 };
            let face = (len + sign * std::f64::consts::TAU * s).max(0.0);
            let scale = if len > 0.0 { face / len } else { 1.0 };
            let mut notches: Vec<f64> = corners(path, spec.notch_angle)
                .into_iter()
                .map(|p| p * scale)
                .collect();
            // Each bend gives material: the strip shortens there.
            let comp = clean(spec.bend_comp).min(face / (notches.len() as f64 + 1.0));
            for (i, n) in notches.iter_mut().enumerate() {
                *n -= comp * (i as f64 + 0.5);
            }
            let face = (face - comp * notches.len() as f64).max(0.0);
            // Split into pieces of at most the maximum length.
            let max = clean(spec.max_length);
            let parts = if max > overlap + 1e-6 && face + overlap > max + 1e-9 {
                (face / (max - overlap) - 1e-9).ceil().max(1.0)
            } else {
                1.0
            };
            if !parts.is_finite() || parts > MAX_STRIPS as f64 {
                continue;
            }
            let parts = parts as usize;
            let piece = face / parts as f64;
            for part in 0..parts {
                let (a, b) = (part as f64 * piece, (part + 1) as f64 * piece);
                out.push(Strip {
                    object: oi,
                    contour: ci,
                    hole,
                    outline_length: len,
                    length: piece + overlap,
                    height,
                    notches: notches
                        .iter()
                        .filter(|n| **n >= a && (**n < b || part + 1 == parts))
                        .map(|n| n - a)
                        .collect(),
                    part,
                    parts,
                });
                if out.len() >= MAX_STRIPS {
                    return out;
                }
            }
        }
    }
    out
}

/// Places along a closed outline (from its first node) of the nodes
/// whose corner angle is under `limit` degrees. The first node is the
/// strip's end and has no notch.
pub fn corners(path: &BezPath, limit: f64) -> Vec<f64> {
    if !(limit.is_finite() && limit > 0.0) {
        return Vec::new();
    }
    let segs: Vec<PathSeg> = path
        .segments()
        .filter(|s| {
            let l = s.arclen(measure::ARCLEN_ACCURACY);
            l.is_finite() && l > 1e-9
        })
        .collect();
    let mut out = Vec::new();
    let mut at = 0.0;
    for w in segs.windows(2) {
        at += w[0].arclen(measure::ARCLEN_ACCURACY);
        let (Some(a), Some(b)) = (end_dir(&w[0]), start_dir(&w[1])) else {
            continue;
        };
        let turn = a.dot(b).clamp(-1.0, 1.0).acos().to_degrees();
        if 180.0 - turn < limit {
            out.push(at);
        }
    }
    out
}

fn start_dir(s: &PathSeg) -> Option<Vec2> {
    let pts: Vec<Point> = match *s {
        PathSeg::Line(l) => vec![l.p0, l.p1],
        PathSeg::Quad(q) => vec![q.p0, q.p1, q.p2],
        PathSeg::Cubic(c) => vec![c.p0, c.p1, c.p2, c.p3],
    };
    let p0 = *pts.first()?;
    pts.iter().skip(1).find_map(|p| {
        let d = *p - p0;
        let l = d.hypot();
        (l > 1e-9).then(|| d / l)
    })
}

fn end_dir(s: &PathSeg) -> Option<Vec2> {
    let pts: Vec<Point> = match *s {
        PathSeg::Line(l) => vec![l.p1, l.p0],
        PathSeg::Quad(q) => vec![q.p2, q.p1, q.p0],
        PathSeg::Cubic(c) => vec![c.p3, c.p2, c.p1, c.p0],
    };
    let p0 = *pts.first()?;
    pts.iter().skip(1).find_map(|p| {
        let d = p0 - *p;
        let l = d.hypot();
        (l > 1e-9).then(|| d / l)
    })
}

/// A strip's outline with its bottom-left corner at `at`: a rectangle of
/// its length and height with V notches (`notch` wide and deep) cut into
/// its top edge.
pub fn strip_path(strip: &Strip, notch: f64, at: Point) -> BezPath {
    let (l, h) = (strip.length, strip.height);
    let n = clean(notch).min(h / 2.0);
    let mut pts = vec![
        at,
        Point::new(at.x + l, at.y),
        Point::new(at.x + l, at.y + h),
    ];
    if n > 1e-9 {
        // Top edge from right to left.
        let mut xs: Vec<f64> = strip
            .notches
            .iter()
            .copied()
            .filter(|x| *x - n / 2.0 > 0.0 && *x + n / 2.0 < l)
            .collect();
        xs.sort_by(|a, b| b.total_cmp(a));
        let mut last = l;
        for x in xs {
            if x + n / 2.0 > last {
                continue;
            }
            pts.push(Point::new(at.x + x + n / 2.0, at.y + h));
            pts.push(Point::new(at.x + x, at.y + h - n));
            pts.push(Point::new(at.x + x - n / 2.0, at.y + h));
            last = x - n / 2.0;
        }
    }
    pts.push(Point::new(at.x, at.y + h));
    tracedraw_core::geometry::polyline_path(&pts, true)
}

/// The corner marks of a strip with its bottom-left corner at `at`: at
/// each corner a line `len` long down from the top edge and one up from
/// the bottom edge (at most a third of the height each).
pub fn strip_marks(strip: &Strip, len: f64, at: Point) -> Vec<(Point, Point)> {
    let h = strip.height;
    let m = clean(len).min(h / 3.0);
    if m <= 1e-9 {
        return Vec::new();
    }
    strip
        .notches
        .iter()
        .filter(|x| x.is_finite() && **x > 0.0 && **x < strip.length)
        .flat_map(|x| {
            let px = at.x + x;
            [
                (Point::new(px, at.y + h), Point::new(px, at.y + h - m)),
                (Point::new(px, at.y), Point::new(px, at.y + m)),
            ]
        })
        .collect()
}

/// The face of a letter: its closed outlines offset inwards by `inset`
/// (the return's material thickness, so the face fits inside the
/// strip). Empty when nothing is left.
pub fn letter_face(paths: &[BezPath], inset: f64) -> BezPath {
    let region = super::region(paths);
    if region.elements().is_empty() {
        return region;
    }
    let d = clean(inset);
    let out = if d > 1e-9 {
        shaping::offset_with(&region, -d, Join::Miter)
    } else {
        shaping::overlay(&region, &BezPath::new(), shaping::Op::Weld)
    };
    if out.area().abs() < 1e-9 {
        return BezPath::new();
    }
    out
}

/// Where each strip goes: one under another, left edges at `x`, the
/// first one's top `gap` under `top`.
pub fn stack(strips: &[Strip], x: f64, top: f64, gap: f64) -> Vec<Point> {
    let gap = clean(gap);
    let mut y = top - gap;
    strips
        .iter()
        .map(|s| {
            y -= s.height;
            let p = Point::new(x, y);
            y -= gap;
            p
        })
        .collect()
}

/// The mould settings (millimetres).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct MouldSpec {
    pub wall: f64,
    pub depth: f64,
    pub overlap: f64,
    /// An inner contour: the letter offset inwards by this (0: none).
    pub inside: f64,
}

impl Default for MouldSpec {
    fn default() -> Self {
        MouldSpec {
            wall: 2.0,
            depth: 5.0,
            overlap: 10.0,
            inside: 0.0,
        }
    }
}

/// The inner contours of a mould: each object's letter offset inwards by
/// the spec's `inside` (empty paths where nothing is left or it is 0).
pub fn mould_inside(objects: &[Vec<BezPath>], spec: &MouldSpec) -> Vec<BezPath> {
    let d = clean(spec.inside);
    objects
        .iter()
        .map(|paths| {
            if d <= 1e-9 {
                return BezPath::new();
            }
            let region = super::region(paths);
            if region.elements().is_empty() {
                return region;
            }
            let out = shaping::offset_with(&region, -d, Join::Round);
            if out.area().abs() < 1e-9 {
                BezPath::new()
            } else {
                out
            }
        })
        .collect()
}

/// The mould of each object: its base (the letter offset outwards by the
/// wall) and the wall strips.
pub fn mould(objects: &[Vec<BezPath>], spec: &MouldSpec) -> (Vec<BezPath>, Vec<Strip>) {
    let wall = clean(spec.wall);
    let bases = objects
        .iter()
        .map(|paths| {
            let region = super::region(paths);
            if region.elements().is_empty() {
                return BezPath::new();
            }
            if wall > 1e-9 {
                shaping::offset_with(&region, wall, Join::Round)
            } else {
                region
            }
        })
        .collect();
    let strips = returns(
        objects,
        &ReturnSpec {
            depth: spec.depth,
            overlap: spec.overlap,
            thickness: wall,
            face: ReturnFace::Middle,
            notch_angle: 0.0,
            notch_size: 0.0,
            max_length: 0.0,
            bend_comp: 0.0,
        },
    );
    (bases, strips)
}

/// The number shown on a strip: the object's number, with more than one
/// outline the outline's after a dot, and for the pieces of a split
/// strip a letter ("3", "3.2", "3.2b").
pub fn strip_label(strip: &Strip, outlines_in_object: usize) -> String {
    let mut s = if outlines_in_object > 1 {
        format!("{}.{}", strip.object + 1, strip.contour + 1)
    } else {
        format!("{}", strip.object + 1)
    };
    if strip.parts > 1 {
        s.push_str(&super::tiles::row_letters(strip.part).to_lowercase());
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::{Circle, Rect};

    fn square(x: f64, y: f64, s: f64) -> BezPath {
        Rect::new(x, y, x + s, y + s).to_path(0.01)
    }

    #[test]
    fn a_square_gets_one_strip_with_three_notches() {
        let strips = returns(&[vec![square(0.0, 0.0, 100.0)]], &ReturnSpec::default());
        assert_eq!(strips.len(), 1);
        let s = &strips[0];
        assert!((s.outline_length - 400.0).abs() < 1e-9);
        assert!((s.length - 410.0).abs() < 1e-9);
        assert_eq!(s.height, 50.0);
        // Corners at 100, 200, 300; the first node is the strip's end.
        assert_eq!(s.notches.len(), 3);
        assert!((s.notches[0] - 100.0).abs() < 1e-9);
        let p = strip_path(s, 3.0, Point::new(0.0, -60.0));
        let bb = p.bounding_box();
        assert!((bb.width() - 410.0).abs() < 1e-9 && (bb.height() - 50.0).abs() < 1e-9);
        // The rectangle less three notches of 3 x 3 / 2.
        assert!((p.area().abs() - (410.0 * 50.0 - 3.0 * 4.5)).abs() < 1e-6);
    }

    #[test]
    fn faces_add_the_material_and_holes_take_it_away() {
        let mut o = square(0.0, 0.0, 100.0);
        o.extend(square(25.0, 25.0, 50.0));
        let spec = ReturnSpec {
            face: ReturnFace::Outer,
            thickness: 2.0,
            overlap: 0.0,
            notch_angle: 0.0,
            ..Default::default()
        };
        let strips = returns(&[vec![o]], &spec);
        assert_eq!(strips.len(), 2);
        let tau2 = std::f64::consts::TAU * 2.0;
        assert!(!strips[0].hole && strips[1].hole);
        assert!((strips[0].length - (400.0 + tau2)).abs() < 1e-9);
        assert!((strips[1].length - (200.0 - tau2)).abs() < 1e-9);
        assert!(strips.iter().all(|s| s.notches.is_empty()));
        assert_eq!(strip_label(&strips[1], 2), "1.2");
    }

    #[test]
    fn smooth_curves_have_no_corners() {
        let c = Circle::new((0.0, 0.0), 50.0).to_path(1e-4);
        let strips = returns(&[vec![c]], &ReturnSpec::default());
        assert_eq!(strips.len(), 1);
        assert!(strips[0].notches.is_empty());
        assert!((strips[0].outline_length - std::f64::consts::TAU * 50.0).abs() < 1e-3);
    }

    #[test]
    fn strips_stack_under_each_other() {
        let strips = returns(
            &[vec![square(0.0, 0.0, 10.0)], vec![square(20.0, 0.0, 10.0)]],
            &ReturnSpec {
                depth: 30.0,
                ..Default::default()
            },
        );
        let at = stack(&strips, 5.0, 0.0, 10.0);
        assert_eq!(at, vec![Point::new(5.0, -40.0), Point::new(5.0, -80.0)]);
    }

    #[test]
    fn a_mould_has_a_base_and_a_wall_strip() {
        let (bases, strips) = mould(&[vec![square(0.0, 0.0, 100.0)]], &MouldSpec::default());
        assert_eq!(bases.len(), 1);
        let bb = bases[0].bounding_box();
        assert!((bb.width() - 104.0).abs() < 0.01);
        assert_eq!(strips.len(), 1);
        // 400 + 2 pi (2 / 2) + 10.
        assert!((strips[0].length - (410.0 + std::f64::consts::TAU)).abs() < 1e-9);
        assert_eq!(strips[0].height, 5.0);
    }

    #[test]
    fn open_lines_and_bad_input_make_no_strips() {
        let mut open = BezPath::new();
        open.move_to((0.0, 0.0));
        open.line_to((10.0, 0.0));
        let mut nan = square(0.0, 0.0, 10.0);
        nan.line_to((f64::NAN, 1.0));
        assert!(returns(&[vec![open, nan]], &ReturnSpec::default()).is_empty());
        let bad = ReturnSpec {
            depth: f64::NAN,
            ..Default::default()
        };
        assert!(returns(&[vec![square(0.0, 0.0, 10.0)]], &bad).is_empty());
        let odd = ReturnSpec {
            overlap: f64::INFINITY,
            thickness: f64::NAN,
            notch_angle: f64::NAN,
            notch_size: f64::INFINITY,
            ..Default::default()
        };
        let s = returns(&[vec![square(0.0, 0.0, 10.0)]], &odd);
        assert_eq!(s.len(), 1);
        assert!((s[0].length - 40.0).abs() < 1e-9);
        let _ = strip_path(&s[0], f64::NAN, Point::ZERO);
        let (b, s) = mould(&[vec![]], &MouldSpec::default());
        assert!(b[0].elements().is_empty() && s.is_empty());
        let odd = ReturnSpec {
            max_length: f64::NAN,
            bend_comp: f64::INFINITY,
            ..Default::default()
        };
        assert_eq!(returns(&[vec![square(0.0, 0.0, 10.0)]], &odd).len(), 1);
        assert!(letter_face(&[], 1.0).elements().is_empty());
        assert!(strip_marks(&s0(), f64::NAN, Point::ZERO).is_empty());
    }

    fn s0() -> Strip {
        Strip {
            object: 0,
            contour: 0,
            hole: false,
            outline_length: 100.0,
            length: 100.0,
            height: 30.0,
            notches: vec![25.0, 50.0, f64::NAN],
            part: 0,
            parts: 1,
        }
    }

    #[test]
    fn long_strips_are_split_into_pieces() {
        // A 400 square, 10 overlap, at most 150 a piece: 400 / 140 = 3 pieces.
        let spec = ReturnSpec {
            max_length: 150.0,
            ..Default::default()
        };
        let strips = returns(&[vec![square(0.0, 0.0, 100.0)]], &spec);
        assert_eq!(strips.len(), 3);
        for (i, s) in strips.iter().enumerate() {
            assert_eq!((s.part, s.parts), (i, 3));
            assert!((s.length - (400.0 / 3.0 + 10.0)).abs() < 1e-9);
        }
        // The corners at 100, 200, 300 land on the pieces they fall in.
        let all: usize = strips.iter().map(|s| s.notches.len()).sum();
        assert_eq!(all, 3);
        assert!((strips[1].notches[0] - (200.0 - 400.0 / 3.0)).abs() < 1e-6);
        assert_eq!(strip_label(&strips[1], 1), "1b");
        // Under the limit: one piece.
        let one = returns(
            &[vec![square(0.0, 0.0, 10.0)]],
            &ReturnSpec {
                max_length: 150.0,
                ..Default::default()
            },
        );
        assert_eq!(one.len(), 1);
        assert_eq!(strip_label(&one[0], 1), "1");
    }

    #[test]
    fn bend_compensation_shortens_at_each_notch() {
        let spec = ReturnSpec {
            bend_comp: 0.5,
            overlap: 0.0,
            ..Default::default()
        };
        let s = returns(&[vec![square(0.0, 0.0, 100.0)]], &spec);
        assert!((s[0].length - (400.0 - 1.5)).abs() < 1e-9);
        assert!((s[0].notches[0] - 99.75).abs() < 1e-9);
        assert!((s[0].notches[2] - (300.0 - 1.25)).abs() < 1e-9);
    }

    #[test]
    fn marks_sit_at_the_corners_on_both_edges() {
        let m = strip_marks(&s0(), 5.0, Point::new(10.0, 0.0));
        assert_eq!(m.len(), 4);
        assert_eq!(m[0], (Point::new(35.0, 30.0), Point::new(35.0, 25.0)));
        assert_eq!(m[1], (Point::new(35.0, 0.0), Point::new(35.0, 5.0)));
        // At most a third of the height.
        let long = strip_marks(&s0(), 50.0, Point::ZERO);
        assert!((long[0].0.y - long[0].1.y - 10.0).abs() < 1e-9);
    }

    #[test]
    fn the_face_is_inset_by_the_thickness() {
        let mut o = square(0.0, 0.0, 100.0);
        o.extend(square(25.0, 25.0, 50.0));
        let f = letter_face(&[o.clone()], 2.0);
        let bb = f.bounding_box();
        assert!((bb.width() - 96.0).abs() < 0.01);
        // The hole grows: 96^2 - 54^2.
        assert!((f.area().abs() - (96.0 * 96.0 - 54.0 * 54.0)).abs() < 0.5);
        assert!((letter_face(&[o], 0.0).area().abs() - 7500.0).abs() < 0.01);
        assert!(letter_face(&[square(0.0, 0.0, 3.0)], 2.0)
            .elements()
            .is_empty());
    }

    #[test]
    fn a_mould_can_have_an_inner_contour() {
        let spec = MouldSpec {
            inside: 5.0,
            ..Default::default()
        };
        let inner = mould_inside(&[vec![square(0.0, 0.0, 100.0)], vec![]], &spec);
        assert_eq!(inner.len(), 2);
        assert!((inner[0].bounding_box().width() - 90.0).abs() < 0.01);
        assert!(inner[1].elements().is_empty());
        let none = mould_inside(&[vec![square(0.0, 0.0, 100.0)]], &MouldSpec::default());
        assert!(none[0].elements().is_empty());
    }
}
