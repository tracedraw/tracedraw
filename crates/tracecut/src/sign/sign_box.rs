//! The flat pattern of a folded sign box (cassette, light box).
//!
//! The face W x H with a flange on each side, folded back along the
//! face's edges; each side has its own depth (first fold) and return lip
//! (second fold). A side of depth 0 has no flange. The corners are
//! notched out: each flange is the face's side long less a corner gap at
//! each end, so the folded flanges do not touch. A return lip folds once
//! more along its flange's outer edge; its ends are cut at 45 degrees (it
//! is the lip length shorter at each end) so the lips can fold in at the
//! corners. Optional round relief holes at the four face corners stop
//! the material tearing where two folds meet. Optional fixing holes sit
//! in the middle of each flange's depth, evenly spread at most the hole
//! spacing apart. No bend allowance: the sizes are the flat sizes.
//!
//! Methods:
//!
//! - [`BoxMethod::Classic`]: the flanges meet at the notched corners.
//! - [`BoxMethod::CornerTabs`]: the bottom and top flanges carry a tab at
//!   each end, in the corner notch, that folds round the corner onto the
//!   side flange to be riveted or glued there. The tab is `tab_width`
//!   long (at most the side flange's depth plus the corner gap), set in
//!   from the flange's edges by the corner gap (at least 0.5 mm, at most
//!   a quarter of the depth), its free corners cut at 45 degrees. Each
//!   tab has a fold line and, with fixing holes, a hole in its middle.

use super::Sides;
use tracedraw_core::geometry::{BezPath, Point, Rect};
use tracedraw_core::shaping::{self, Op};

/// How the corners are made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum BoxMethod {
    #[default]
    Classic,
    CornerTabs,
}

/// The box settings (millimetres).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct BoxSpec {
    pub method: BoxMethod,
    /// Depth of the box on each side: the flanges' width (first fold).
    pub depth: Sides,
    /// Return lip on each flange (second fold, 0: none).
    pub lip: Sides,
    /// Corner gap at each end of a flange.
    pub corner_gap: f64,
    /// Diameter of the relief holes at the face corners (0: none).
    pub relief: f64,
    /// Fixing holes in the flanges.
    pub holes: bool,
    pub hole_spacing: f64,
    pub hole_diameter: f64,
    /// Length of the corner tabs.
    pub tab_width: f64,
}

impl Default for BoxSpec {
    fn default() -> Self {
        BoxSpec {
            method: BoxMethod::Classic,
            depth: Sides::all(50.0),
            lip: Sides::all(0.0),
            corner_gap: 0.5,
            relief: 0.0,
            holes: false,
            hole_spacing: 300.0,
            hole_diameter: 5.0,
            tab_width: 20.0,
        }
    }
}

/// The flat pattern: the cut outline, the fold lines and the holes.
#[derive(Debug, Clone, PartialEq)]
pub struct BoxPattern {
    pub outline: BezPath,
    pub folds: Vec<(Point, Point)>,
    pub holes: Vec<Point>,
    pub hole_diameter: f64,
    /// The face, where the pattern was built around.
    pub face: Rect,
}

/// The pattern of a box whose face is `face`. `None` when the sizes do
/// not make a box (a side of the face zero, no flange deeper than zero,
/// or a size not finite).
pub fn sign_box(face: Rect, spec: &BoxSpec) -> Option<BoxPattern> {
    let f = face.abs();
    let fin = |v: f64| v.is_finite();
    if !(fin(f.x0) && fin(f.y0) && fin(f.x1) && fin(f.y1)) {
        return None;
    }
    let (w, h) = (f.width(), f.height());
    let d = spec.depth.clean();
    let deepest = d.left.max(d.right).max(d.bottom).max(d.top);
    if !(w > 1e-6 && h > 1e-6 && deepest > 1e-6) || w.max(h).max(deepest) > 1e7 {
        return None;
    }
    let clean = |v: f64| if fin(v) { v.max(0.0) } else { 0.0 };
    let side = w.min(h);
    let g = clean(spec.corner_gap).min(side / 4.0);
    // A lip may take at most half of its flange each side.
    let lips = spec.lip.clean();
    let lip_of = |depth: f64, lip: f64, len: f64| {
        if depth > 1e-9 {
            lip.min(((len - 2.0 * g) / 2.0) * 0.99)
        } else {
            0.0
        }
    };
    let lb = lip_of(d.bottom, lips.bottom, w);
    let lt = lip_of(d.top, lips.top, w);
    let ll = lip_of(d.left, lips.left, h);
    let lr = lip_of(d.right, lips.right, h);
    let tabs = spec.method == BoxMethod::CornerTabs;
    let (x0, y0, x1, y1) = (f.x0, f.y0, f.x1, f.y1);
    let p = Point::new;
    let mut folds: Vec<(Point, Point)> = Vec::new();
    let mut tab_holes: Vec<Point> = Vec::new();
    // A tab on the end of a horizontal flange of depth `dh` beside a
    // vertical flange of depth `dv`: (length, inset, chamfer), or None.
    let tab = |dh: f64, dv: f64| -> Option<(f64, f64, f64)> {
        if !tabs || dh <= 1e-9 || dv <= 1e-9 {
            return None;
        }
        let tw = clean(spec.tab_width).min(dv + g);
        let c = g.max(0.5).min(dh / 4.0);
        let ch = (tw / 2.0).min((dh - 2.0 * c) / 4.0);
        (tw > 1e-6 && dh - 2.0 * c - 2.0 * ch > 1e-6).then_some((tw, c, ch))
    };
    let mut pts: Vec<Point> = Vec::new();
    // Bottom flange, left to right.
    pts.push(p(x0, y0));
    let db = d.bottom;
    if db > 1e-9 {
        pts.push(p(x0 + g, y0));
        if let Some((tw, c, ch)) = tab(db, d.left) {
            let x = x0 + g;
            pts.extend([
                p(x, y0 - c),
                p(x - tw + ch, y0 - c),
                p(x - tw, y0 - c - ch),
                p(x - tw, y0 - db + c + ch),
                p(x - tw + ch, y0 - db + c),
                p(x, y0 - db + c),
            ]);
            folds.push((p(x, y0 - c), p(x, y0 - db + c)));
            tab_holes.push(p(x - tw / 2.0, y0 - db / 2.0));
        }
        pts.push(p(x0 + g, y0 - db));
        if lb > 0.0 {
            pts.push(p(x0 + g + lb, y0 - db - lb));
            pts.push(p(x1 - g - lb, y0 - db - lb));
        }
        pts.push(p(x1 - g, y0 - db));
        if let Some((tw, c, ch)) = tab(db, d.right) {
            let x = x1 - g;
            pts.extend([
                p(x, y0 - db + c),
                p(x + tw - ch, y0 - db + c),
                p(x + tw, y0 - db + c + ch),
                p(x + tw, y0 - c - ch),
                p(x + tw - ch, y0 - c),
                p(x, y0 - c),
            ]);
            folds.push((p(x, y0 - db + c), p(x, y0 - c)));
            tab_holes.push(p(x + tw / 2.0, y0 - db / 2.0));
        }
        pts.push(p(x1 - g, y0));
    }
    pts.push(p(x1, y0));
    // Right flange, bottom to top.
    let dr = d.right;
    if dr > 1e-9 {
        pts.push(p(x1, y0 + g));
        pts.push(p(x1 + dr, y0 + g));
        if lr > 0.0 {
            pts.push(p(x1 + dr + lr, y0 + g + lr));
            pts.push(p(x1 + dr + lr, y1 - g - lr));
        }
        pts.push(p(x1 + dr, y1 - g));
        pts.push(p(x1, y1 - g));
    }
    pts.push(p(x1, y1));
    // Top flange, right to left.
    let dt = d.top;
    if dt > 1e-9 {
        pts.push(p(x1 - g, y1));
        if let Some((tw, c, ch)) = tab(dt, d.right) {
            let x = x1 - g;
            pts.extend([
                p(x, y1 + c),
                p(x + tw - ch, y1 + c),
                p(x + tw, y1 + c + ch),
                p(x + tw, y1 + dt - c - ch),
                p(x + tw - ch, y1 + dt - c),
                p(x, y1 + dt - c),
            ]);
            folds.push((p(x, y1 + c), p(x, y1 + dt - c)));
            tab_holes.push(p(x + tw / 2.0, y1 + dt / 2.0));
        }
        pts.push(p(x1 - g, y1 + dt));
        if lt > 0.0 {
            pts.push(p(x1 - g - lt, y1 + dt + lt));
            pts.push(p(x0 + g + lt, y1 + dt + lt));
        }
        pts.push(p(x0 + g, y1 + dt));
        if let Some((tw, c, ch)) = tab(dt, d.left) {
            let x = x0 + g;
            pts.extend([
                p(x, y1 + dt - c),
                p(x - tw + ch, y1 + dt - c),
                p(x - tw, y1 + dt - c - ch),
                p(x - tw, y1 + c + ch),
                p(x - tw + ch, y1 + c),
                p(x, y1 + c),
            ]);
            folds.push((p(x, y1 + dt - c), p(x, y1 + c)));
            tab_holes.push(p(x - tw / 2.0, y1 + dt / 2.0));
        }
        pts.push(p(x0 + g, y1));
    }
    pts.push(p(x0, y1));
    // Left flange, top to bottom.
    let dl = d.left;
    if dl > 1e-9 {
        pts.push(p(x0, y1 - g));
        pts.push(p(x0 - dl, y1 - g));
        if ll > 0.0 {
            pts.push(p(x0 - dl - ll, y1 - g - ll));
            pts.push(p(x0 - dl - ll, y0 + g + ll));
        }
        pts.push(p(x0 - dl, y0 + g));
        pts.push(p(x0, y0 + g));
    }
    // Drop repeated points (a zero gap).
    pts.dedup_by(|a, b| (*a - *b).hypot() < 1e-9);
    if pts.len() > 1 && (pts[0] - pts[pts.len() - 1]).hypot() < 1e-9 {
        pts.pop();
    }
    let mut outline = tracedraw_core::geometry::polyline_path(&pts, true);
    let relief = clean(spec.relief).min(side / 2.0);
    if relief > 1e-6 {
        let mut holes = BezPath::new();
        for c in [p(x0, y0), p(x1, y0), p(x1, y1), p(x0, y1)] {
            holes.extend(super::circle(c, relief).elements().iter().copied());
        }
        outline = shaping::overlay(&outline, &holes, Op::Trim);
    }
    // The first folds along the face's edges, then the lips' folds.
    let mut main_folds = Vec::new();
    if db > 1e-9 {
        main_folds.push((p(x0 + g, y0), p(x1 - g, y0)));
    }
    if dr > 1e-9 {
        main_folds.push((p(x1, y0 + g), p(x1, y1 - g)));
    }
    if dt > 1e-9 {
        main_folds.push((p(x1 - g, y1), p(x0 + g, y1)));
    }
    if dl > 1e-9 {
        main_folds.push((p(x0, y1 - g), p(x0, y0 + g)));
    }
    if lb > 0.0 {
        main_folds.push((p(x0 + g, y0 - db), p(x1 - g, y0 - db)));
    }
    if lr > 0.0 {
        main_folds.push((p(x1 + dr, y0 + g), p(x1 + dr, y1 - g)));
    }
    if lt > 0.0 {
        main_folds.push((p(x1 - g, y1 + dt), p(x0 + g, y1 + dt)));
    }
    if ll > 0.0 {
        main_folds.push((p(x0 - dl, y1 - g), p(x0 - dl, y0 + g)));
    }
    main_folds.extend(folds);
    let mut holes = Vec::new();
    let spacing = spec.hole_spacing;
    if spec.holes && fin(spacing) && spacing > 1e-6 {
        // Each flange: along its length, at half its depth.
        let flanges = [
            (db, p(x0 + g, y0 - db / 2.0), p(x1 - g, y0 - db / 2.0)),
            (dr, p(x1 + dr / 2.0, y0 + g), p(x1 + dr / 2.0, y1 - g)),
            (dt, p(x1 - g, y1 + dt / 2.0), p(x0 + g, y1 + dt / 2.0)),
            (dl, p(x0 - dl / 2.0, y1 - g), p(x0 - dl / 2.0, y0 + g)),
        ];
        for (depth, a, b) in flanges {
            if depth <= 1e-9 {
                continue;
            }
            let len = (b - a).hypot();
            let n = (len / spacing - 1e-9).ceil().max(1.0);
            if n > 10_000.0 {
                continue;
            }
            let n = n as usize;
            for k in 0..n {
                let t = (k as f64 + 0.5) / n as f64;
                holes.push(a + (b - a) * t);
            }
        }
        holes.extend(tab_holes);
    }
    Some(BoxPattern {
        outline,
        folds: main_folds,
        holes,
        hole_diameter: clean(spec.hole_diameter),
        face: f,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::Shape as _;

    #[test]
    fn a_box_has_a_cross_shaped_pattern() {
        let face = Rect::new(0.0, 0.0, 1000.0, 500.0);
        let spec = BoxSpec {
            corner_gap: 0.0,
            ..Default::default()
        };
        let b = sign_box(face, &spec).expect("a box");
        let bb = b.outline.bounding_box();
        assert!((bb.width() - 1100.0).abs() < 1e-9);
        assert!((bb.height() - 600.0).abs() < 1e-9);
        // Face plus four flanges, the corner squares cut away.
        let area = 1000.0 * 500.0 + 2.0 * 1000.0 * 50.0 + 2.0 * 500.0 * 50.0;
        assert!((b.outline.area().abs() - area).abs() < 1e-6);
        assert_eq!(b.folds.len(), 4);
        assert!(b.holes.is_empty());
    }

    #[test]
    fn corner_gaps_and_lips_shorten_the_flanges() {
        let face = Rect::new(0.0, 0.0, 400.0, 200.0);
        let spec = BoxSpec {
            depth: Sides::all(40.0),
            lip: Sides::all(10.0),
            corner_gap: 1.0,
            ..Default::default()
        };
        let b = sign_box(face, &spec).expect("a box");
        assert_eq!(b.folds.len(), 8);
        let bb = b.outline.bounding_box();
        assert!((bb.width() - (400.0 + 2.0 * 50.0)).abs() < 1e-9);
        // Bottom flange 398 x 40, its lip a trapezoid 398 / 378 x 10.
        let face_a = 400.0 * 200.0;
        let flanges = 2.0 * (398.0 * 40.0) + 2.0 * (198.0 * 40.0);
        let lips = 2.0 * ((398.0 + 378.0) / 2.0 * 10.0) + 2.0 * ((198.0 + 178.0) / 2.0 * 10.0);
        assert!((b.outline.area().abs() - (face_a + flanges + lips)).abs() < 1e-6);
        // The first fold runs along the face's bottom edge.
        let (a, c) = b.folds[0];
        assert!((a.y).abs() < 1e-9 && (c.y).abs() < 1e-9 && (c.x - a.x - 398.0).abs() < 1e-9);
    }

    #[test]
    fn each_side_has_its_own_depth_and_lip() {
        let face = Rect::new(0.0, 0.0, 400.0, 200.0);
        let spec = BoxSpec {
            depth: Sides {
                left: 20.0,
                right: 30.0,
                bottom: 40.0,
                top: 0.0,
            },
            lip: Sides {
                left: 0.0,
                right: 0.0,
                bottom: 10.0,
                top: 10.0,
            },
            corner_gap: 0.0,
            ..Default::default()
        };
        let b = sign_box(face, &spec).expect("a box");
        let bb = b.outline.bounding_box();
        assert!((bb.x0 + 20.0).abs() < 1e-9 && (bb.x1 - 430.0).abs() < 1e-9);
        // No top flange: the face's top edge is the pattern's top.
        assert!((bb.y1 - 200.0).abs() < 1e-9 && (bb.y0 + 50.0).abs() < 1e-9);
        // Three first folds and the bottom lip's fold; no top lip.
        assert_eq!(b.folds.len(), 4);
        let area = 400.0 * 200.0
            + 400.0 * 40.0
            + 200.0 * 20.0
            + 200.0 * 30.0
            + (400.0 + 380.0) / 2.0 * 10.0;
        assert!((b.outline.area().abs() - area).abs() < 1e-6);
    }

    #[test]
    fn corner_tabs_sit_in_the_notches() {
        let face = Rect::new(0.0, 0.0, 400.0, 200.0);
        let classic = BoxSpec {
            corner_gap: 1.0,
            ..Default::default()
        };
        let spec = BoxSpec {
            method: BoxMethod::CornerTabs,
            tab_width: 20.0,
            holes: true,
            hole_spacing: 1000.0,
            ..classic
        };
        let plain = sign_box(face, &classic).expect("a box");
        let b = sign_box(face, &spec).expect("a box");
        // Four tabs: each 20 long, set in by 1 from 50 deep, 45 degree
        // corners of 10: 20 x 48 less two 10 x 10 / 2 triangles.
        let tab = 20.0 * 48.0 - 2.0 * 50.0;
        assert!((b.outline.area().abs() - plain.outline.area().abs() - 4.0 * tab).abs() < 1e-6);
        assert_eq!(b.folds.len(), 4 + 4);
        // One hole a flange and one a tab.
        assert_eq!(b.holes.len(), 4 + 4);
        // The pattern stays inside the classic one's box.
        let (pb, bb) = (plain.outline.bounding_box(), b.outline.bounding_box());
        assert!(bb.x0 >= pb.x0 - 1e-9 && bb.x1 <= pb.x1 + 1e-9);
        // A side without a flange gets no tabs next to it.
        let no_left = BoxSpec {
            depth: Sides {
                left: 0.0,
                ..Sides::all(50.0)
            },
            ..spec
        };
        let b = sign_box(face, &no_left).expect("a box");
        assert_eq!(b.folds.len(), 3 + 2);
    }

    #[test]
    fn fixing_holes_are_spread_along_each_flange() {
        let face = Rect::new(0.0, 0.0, 1000.0, 500.0);
        let spec = BoxSpec {
            holes: true,
            hole_spacing: 300.0,
            corner_gap: 0.0,
            ..Default::default()
        };
        let b = sign_box(face, &spec).expect("a box");
        // 1000 / 300: 4 holes each long side, 500 / 300: 2 each short side.
        assert_eq!(b.holes.len(), 12);
        assert!(b.holes.iter().any(|h| (h.y + 25.0).abs() < 1e-9));
    }

    #[test]
    fn relief_holes_take_material_at_the_corners() {
        let face = Rect::new(0.0, 0.0, 300.0, 300.0);
        let plain = sign_box(face, &BoxSpec::default()).expect("a box");
        let relieved = sign_box(
            face,
            &BoxSpec {
                relief: 6.0,
                ..Default::default()
            },
        )
        .expect("a box");
        assert!(relieved.outline.area().abs() < plain.outline.area().abs() - 50.0);
    }

    #[test]
    fn degenerate_sizes_make_no_box() {
        let spec = BoxSpec::default();
        assert!(sign_box(Rect::new(0.0, 0.0, 0.0, 100.0), &spec).is_none());
        assert!(sign_box(Rect::new(0.0, 0.0, f64::NAN, 100.0), &spec).is_none());
        let flat = BoxSpec {
            depth: Sides::all(0.0),
            ..Default::default()
        };
        assert!(sign_box(Rect::new(0.0, 0.0, 10.0, 10.0), &flat).is_none());
        let nan = BoxSpec {
            depth: Sides::all(f64::NAN),
            ..Default::default()
        };
        assert!(sign_box(Rect::new(0.0, 0.0, 10.0, 10.0), &nan).is_none());
        let wild = BoxSpec {
            method: BoxMethod::CornerTabs,
            depth: Sides::all(5.0),
            lip: Sides::all(f64::INFINITY),
            corner_gap: f64::NAN,
            relief: -3.0,
            holes: true,
            hole_spacing: f64::NAN,
            hole_diameter: f64::NAN,
            tab_width: f64::INFINITY,
        };
        let b = sign_box(Rect::new(0.0, 0.0, 10.0, 10.0), &wild).expect("a box");
        assert!(b.holes.is_empty());
        assert_eq!(b.hole_diameter, 0.0);
    }
}
