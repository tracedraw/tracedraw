//! Fillet, scallop and chamfer the corners of curves (the Corners
//! panel). A corner is a node where two segments meet
//! at an angle; smooth and symmetrical nodes are not corners. Corners are
//! cut in path order, and one whose segments are too short for the cut,
//! after the cuts already made on them, is left as it is. A filleted
//! corner is found again by its arc ([`fillets`]) and can be taken back
//! off ([`unround`]), so its radius can change later (#872), between
//! curved sides too: there the arc is the circle of its radius tangent
//! to both sides (#2223).

use crate::geometry::{
    BezPath, CubicBez, Line, ParamCurve, ParamCurveArclen, PathEl, PathSeg, Point, Vec2,
};

/// How a corner is cut.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CornerCut {
    /// A circular arc of this radius, tangent to both segments.
    Fillet(f64),
    /// A circular notch of this radius centred on the corner.
    Scallop(f64),
    /// A straight cut starting `a` back along the segment that comes into
    /// the corner and ending `b` along the one that leaves it (the order
    /// the path is drawn in).
    Chamfer(f64, f64),
}

/// One subpath: its segments with the element index of each segment's
/// end node, the index of its move-to, whether it is closed and whether
/// its last segment is the closing line (drawn by the close itself).
struct Sub {
    segs: Vec<(PathSeg, usize)>,
    start_index: usize,
    closed: bool,
    closing_line: bool,
}

fn subpaths(path: &BezPath) -> Vec<Sub> {
    let path = crate::nodes::normalize(path);
    let mut out: Vec<Sub> = Vec::new();
    let mut cur: Option<Sub> = None;
    let (mut start, mut last) = (Point::ZERO, Point::ZERO);
    for (i, el) in path.elements().iter().enumerate() {
        match *el {
            PathEl::MoveTo(p) => {
                if let Some(s) = cur.take() {
                    out.push(s);
                }
                cur = Some(Sub {
                    segs: Vec::new(),
                    start_index: i,
                    closed: false,
                    closing_line: false,
                });
                start = p;
                last = p;
            }
            PathEl::LineTo(p) => {
                if let Some(s) = cur.as_mut() {
                    if (p - last).hypot() > 1e-12 {
                        s.segs.push((PathSeg::Line(Line::new(last, p)), i));
                    }
                }
                last = p;
            }
            PathEl::CurveTo(c1, c2, p) => {
                if let Some(s) = cur.as_mut() {
                    let degenerate = [c1, c2, p].iter().all(|q| (*q - last).hypot() < 1e-12);
                    if !degenerate {
                        s.segs
                            .push((PathSeg::Cubic(CubicBez::new(last, c1, c2, p)), i));
                    }
                }
                last = p;
            }
            PathEl::QuadTo(..) => {}
            PathEl::ClosePath => {
                if let Some(s) = cur.as_mut() {
                    if (last - start).hypot() > 1e-12 {
                        let index = s.start_index;
                        s.segs.push((PathSeg::Line(Line::new(last, start)), index));
                        s.closing_line = true;
                    }
                    s.closed = true;
                }
                last = start;
            }
        }
    }
    if let Some(s) = cur {
        out.push(s);
    }
    out
}

fn unit(v: Vec2) -> Option<Vec2> {
    let l = v.hypot();
    (l > 1e-12 && l.is_finite()).then(|| v / l)
}

/// Direction of travel at the start of a segment.
fn start_dir(seg: &PathSeg) -> Option<Vec2> {
    match seg {
        PathSeg::Line(l) => unit(l.p1 - l.p0),
        PathSeg::Quad(q) => unit(q.p1 - q.p0).or_else(|| unit(q.p2 - q.p0)),
        PathSeg::Cubic(c) => unit(c.p1 - c.p0)
            .or_else(|| unit(c.p2 - c.p0))
            .or_else(|| unit(c.p3 - c.p0)),
    }
}

/// Direction of travel at the end of a segment.
fn end_dir(seg: &PathSeg) -> Option<Vec2> {
    match seg {
        PathSeg::Line(l) => unit(l.p1 - l.p0),
        PathSeg::Quad(q) => unit(q.p2 - q.p1).or_else(|| unit(q.p2 - q.p0)),
        PathSeg::Cubic(c) => unit(c.p3 - c.p2)
            .or_else(|| unit(c.p3 - c.p1))
            .or_else(|| unit(c.p3 - c.p0)),
    }
}

/// Direction of travel at parameter `t`.
fn dir_at(seg: &PathSeg, t: f64) -> Option<Vec2> {
    let d = match seg {
        PathSeg::Line(l) => l.p1 - l.p0,
        PathSeg::Quad(q) => {
            let mt = 1.0 - t;
            ((q.p1 - q.p0) * mt + (q.p2 - q.p1) * t) * 2.0
        }
        PathSeg::Cubic(c) => {
            let mt = 1.0 - t;
            ((c.p1 - c.p0) * (mt * mt) + (c.p2 - c.p1) * (2.0 * mt * t) + (c.p3 - c.p2) * (t * t))
                * 3.0
        }
    };
    unit(d).or_else(|| {
        if t < 0.5 {
            start_dir(seg)
        } else {
            end_dir(seg)
        }
    })
}

const ACCURACY: f64 = 1e-6;

/// Parameter of the point `s` along the segment (arc length).
fn param_at(seg: &PathSeg, len: f64, s: f64) -> f64 {
    if s <= 0.0 {
        return 0.0;
    }
    if s >= len {
        return 1.0;
    }
    match seg {
        PathSeg::Line(_) => s / len,
        _ => seg.inv_arclen(s, ACCURACY),
    }
}

/// The elements of a circular arc around `center` from `from` turning by
/// `sweep` radians, ending exactly on `to`.
fn arc_to(out: &mut Vec<PathEl>, center: Point, from: Point, to: Point, sweep: f64) {
    let r = (from - center).hypot();
    if r < 1e-12 || sweep.abs() < 1e-9 {
        out.push(PathEl::LineTo(to));
        return;
    }
    let start = (from - center).atan2();
    let arc = crate::geometry::Arc {
        center,
        radii: Vec2::new(r, r),
        start_angle: start,
        sweep_angle: sweep,
        x_rotation: 0.0,
    };
    // One cubic per quarter turn or so, as a drawn corner has.
    let mut els: Vec<PathEl> = arc.append_iter((r * 1e-3).max(1e-9)).collect();
    // The arc ends where the cut meets the next segment.
    if let Some(last) = els.last_mut() {
        match last {
            PathEl::CurveTo(_, c2, p) => {
                let d = to - *p;
                *c2 += d;
                *p = to;
            }
            PathEl::LineTo(p) | PathEl::QuadTo(_, p) => *p = to,
            _ => {}
        }
    }
    if els.is_empty() {
        els.push(PathEl::LineTo(to));
    }
    out.extend(els);
}

/// Signed angle (radians) that turns `a` onto `b`, in (-pi, pi].
fn turn(a: Vec2, b: Vec2) -> f64 {
    a.cross(b).atan2(a.dot(b))
}

/// Velocity of a segment at parameter `t`, which may lie outside 0..1:
/// the segment's own curve carried on past its ends.
fn vel(seg: &PathSeg, t: f64) -> Vec2 {
    match seg {
        PathSeg::Line(l) => l.p1 - l.p0,
        PathSeg::Quad(q) => ((q.p1 - q.p0) * (1.0 - t) + (q.p2 - q.p1) * t) * 2.0,
        PathSeg::Cubic(c) => {
            let mt = 1.0 - t;
            ((c.p1 - c.p0) * (mt * mt) + (c.p2 - c.p1) * (2.0 * mt * t) + (c.p3 - c.p2) * (t * t))
                * 3.0
        }
    }
}

/// Signed curvature of a segment at `t` (positive where it turns left).
fn curvature(seg: &PathSeg, t: f64) -> f64 {
    let v = vel(seg, t);
    let a = match seg {
        PathSeg::Line(_) => return 0.0,
        PathSeg::Quad(q) => ((q.p2 - q.p1) - (q.p1 - q.p0)) * 2.0,
        PathSeg::Cubic(c) => {
            (((c.p2 - c.p1) - (c.p1 - c.p0)) * (1.0 - t) + ((c.p3 - c.p2) - (c.p2 - c.p1)) * t)
                * 6.0
        }
    };
    let speed = v.hypot();
    if speed < 1e-12 {
        return 0.0;
    }
    v.cross(a) / (speed * speed * speed)
}

/// The normal of a direction on the side a turn of sign `side` goes to.
fn normal_of(d: Vec2, side: f64) -> Vec2 {
    Vec2::new(-d.y, d.x) * side
}

/// Where the circle of radius `r` tangent to both sides of a corner
/// touches them (parameters on the side coming in and on the one going
/// out), found from `guess`: the two sides moved `r` towards the inside
/// of the corner (`side`: 1 when the path turns left there, -1 right)
/// meet at its centre. `None` when there is no such circle near the
/// guess, or when a side bends more tightly than the circle, which then
/// would not lie inside the corner (#2223).
fn tangent_circle(
    seg_in: &PathSeg,
    seg_out: &PathSeg,
    r: f64,
    side: f64,
    guess: (f64, f64),
) -> Option<(f64, f64)> {
    let (mut a, mut b) = guess;
    let tol = 1e-10 * (1.0 + r);
    for _ in 0..40 {
        let (va, vb) = (vel(seg_in, a), vel(seg_out, b));
        let (ta, tb) = (unit(va)?, unit(vb)?);
        let f = (seg_in.eval(a) + normal_of(ta, side) * r)
            - (seg_out.eval(b) + normal_of(tb, side) * r);
        // A side moved by `r` runs at its speed times this.
        let ga = 1.0 - side * r * curvature(seg_in, a);
        let gb = 1.0 - side * r * curvature(seg_out, b);
        if f.hypot() < tol {
            let inside = (-1e-9..=1.0 + 1e-9).contains(&a) && (-1e-9..=1.0 + 1e-9).contains(&b);
            // The arc turns the way the corner does.
            let turns = turn(ta, tb) * side > 1e-9;
            return (inside && turns && ga > 1e-9 && gb > 1e-9)
                .then_some((a.clamp(0.0, 1.0), b.clamp(0.0, 1.0)));
        }
        let (ja, jb) = (va * ga, vb * gb);
        let det = jb.cross(ja);
        if det.abs() < 1e-14 || !det.is_finite() {
            return None;
        }
        // ja da - jb db = -f.
        let da = (f.cross(jb) / det).clamp(-0.25, 0.25);
        let db = (f.cross(ja) / det).clamp(-0.25, 0.25);
        a = (a + da).clamp(-0.5, 1.5);
        b = (b + db).clamp(-0.5, 1.5);
        if !(a.is_finite() && b.is_finite()) {
            return None;
        }
    }
    None
}

/// Where a fillet of radius `r` leaves the side coming into a corner and
/// meets the side going out: the parameters on both and the lengths it
/// takes off them, measured along them. Between straight sides that is
/// `r / tan(theta / 2)` on each; a curved side is touched where the
/// circle of that radius is tangent to both (#2223).
struct FilletCut {
    ti: f64,
    to: f64,
    t_in: f64,
    t_out: f64,
}

fn fillet_cut(
    seg_in: &PathSeg,
    seg_out: &PathSeg,
    len_in: f64,
    len_out: f64,
    r: f64,
    accuracy: f64,
) -> Option<FilletCut> {
    let (u_in, u_out) = (end_dir(seg_in)?, start_dir(seg_out)?);
    let theta = (-u_in).dot(u_out).clamp(-1.0, 1.0).acos();
    let t = r / (theta / 2.0).tan();
    if let (PathSeg::Line(_), PathSeg::Line(_)) = (seg_in, seg_out) {
        return Some(FilletCut {
            ti: param_at(seg_in, len_in, len_in - t),
            to: param_at(seg_out, len_out, t),
            t_in: t,
            t_out: t,
        });
    }
    if !(len_in > 1e-12 && len_out > 1e-12) {
        return None;
    }
    let side = if turn(u_in, u_out) >= 0.0 { 1.0 } else { -1.0 };
    // From where straight sides would be touched, then nearer and
    // further.
    for k in [1.0, 0.5, 2.0, 0.25] {
        let guess = (
            (1.0 - k * t / len_in).clamp(0.0, 1.0),
            (k * t / len_out).clamp(0.0, 1.0),
        );
        let Some((ti, to)) = tangent_circle(seg_in, seg_out, r, side, guess) else {
            continue;
        };
        let t_in = match seg_in {
            PathSeg::Line(_) => len_in * (1.0 - ti),
            _ => seg_in.subsegment(ti..1.0).arclen(accuracy),
        };
        let t_out = match seg_out {
            PathSeg::Line(_) => len_out * to,
            _ => seg_out.subsegment(0.0..to).arclen(accuracy),
        };
        return Some(FilletCut {
            ti,
            to,
            t_in,
            t_out,
        });
    }
    None
}

/// The largest fillet radius the corner between two sides takes when it
/// may use `rooms.0` of the side coming in and `rooms.1` of the one
/// going out (lengths along them); `cap` when it takes that much, as
/// the caller then has no use for more.
fn largest_fillet(
    seg_in: &PathSeg,
    seg_out: &PathSeg,
    lens: (f64, f64),
    rooms: (f64, f64),
    theta: f64,
    cap: Option<f64>,
) -> f64 {
    let straight = rooms.0.min(rooms.1) * (theta / 2.0).tan();
    if let (PathSeg::Line(_), PathSeg::Line(_)) = (seg_in, seg_out) {
        return straight;
    }
    if !(straight.is_finite() && straight > 0.0) {
        return 0.0;
    }
    let fits = |r: f64| {
        fillet_cut(seg_in, seg_out, lens.0, lens.1, r, 1e-4)
            .is_some_and(|c| c.t_in <= rooms.0 && c.t_out <= rooms.1)
    };
    if let Some(cap) = cap {
        if fits(cap) {
            return cap;
        }
    }
    // Curved sides part faster or slower than straight ones: search
    // around what straight sides would take.
    let (mut lo, mut hi) = (0.0, straight);
    if fits(straight) {
        lo = straight;
        for _ in 0..20 {
            hi *= 2.0;
            if !fits(hi) {
                break;
            }
            lo = hi;
        }
        if lo >= hi {
            return lo;
        }
    }
    for _ in 0..40 {
        let mid = (lo + hi) / 2.0;
        if fits(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
        if hi - lo < 1e-5 * hi {
            break;
        }
    }
    lo
}

/// A corner of a curve, for the Shape tool's corner widgets: the node
/// (element index, as the Shape tool counts nodes), where it is, the
/// unit direction from it into the corner (halfway between its two
/// sides) and the angle between the sides (radians).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Corner {
    pub node: usize,
    pub at: Point,
    pub inside: Vec2,
    pub theta: f64,
}

/// The corners of `path` that [`cut_corners`] would cut, in path order.
/// Cheap: no lengths are measured.
pub fn corners(path: &BezPath) -> Vec<Corner> {
    corners_of(&subpaths(path))
}

/// The corners and the rounded corners of `path` ([`corners`] and
/// [`fillets`]), the curve read once.
pub fn corners_and_fillets(path: &BezPath) -> (Vec<Corner>, Vec<Fillet>) {
    let subs = subpaths(path);
    (
        corners_of(&subs),
        fillets_at(&subs).into_iter().map(|f| f.fillet).collect(),
    )
}

fn corners_of(subs: &[Sub]) -> Vec<Corner> {
    let mut out = Vec::new();
    for sub in subs {
        let n = sub.segs.len();
        if n == 0 {
            continue;
        }
        let count = if sub.closed { n } else { n - 1 };
        for k in 0..count {
            let (seg_in, index) = &sub.segs[k];
            let (seg_out, _) = &sub.segs[(k + 1) % n];
            let (Some(u_in), Some(u_out)) = (end_dir(seg_in), start_dir(seg_out)) else {
                continue;
            };
            let theta = (-u_in).dot(u_out).clamp(-1.0, 1.0).acos();
            if !(1e-3..=std::f64::consts::PI - 1e-3).contains(&theta) {
                continue;
            }
            let Some(inside) = unit(u_out - u_in) else {
                continue;
            };
            let at_start = sub.closed && k == n - 1;
            out.push(Corner {
                node: if at_start { sub.start_index } else { *index },
                at: seg_in.eval(1.0),
                inside,
                theta,
            });
        }
    }
    out
}

/// How far each corner of `path`, in the order [`corners`] gives them,
/// can be rounded on its own: the depth (mm) of the arc's middle into the
/// corner at the largest fillet radius it takes alone (the shorter of its
/// sides times `tan(theta / 2)`, as [`max_fillet`] finds for that corner
/// alone). A corner whose sides barely turn, or are very short, has
/// next to none: rounding it changes nothing that shows.
pub fn corner_reach(path: &BezPath) -> Vec<f64> {
    // Lengths to within a thousandth: enough to tell what shows.
    const ROUGH: f64 = 1e-3;
    let mut out = Vec::new();
    for sub in subpaths(path) {
        let n = sub.segs.len();
        if n == 0 {
            continue;
        }
        let count = if sub.closed { n } else { n - 1 };
        for k in 0..count {
            let (seg_in, _) = &sub.segs[k];
            let (seg_out, _) = &sub.segs[(k + 1) % n];
            let (Some(u_in), Some(u_out)) = (end_dir(seg_in), start_dir(seg_out)) else {
                continue;
            };
            let theta = (-u_in).dot(u_out).clamp(-1.0, 1.0).acos();
            if !(1e-3..=std::f64::consts::PI - 1e-3).contains(&theta) {
                continue;
            }
            if unit(u_out - u_in).is_none() {
                continue;
            }
            let room = seg_in.arclen(ROUGH).min(seg_out.arclen(ROUGH));
            let half = theta / 2.0;
            out.push(room * half.tan() * (1.0 / half.sin() - 1.0));
        }
    }
    out
}

/// The largest fillet radius every chosen corner of `path` takes (every
/// corner without `only`), the sides two chosen corners share split
/// between them; `None` when no corner is chosen. Between straight
/// sides that is the room times `tan(theta / 2)`; with a curved side it
/// is the largest circle tangent to both that still touches them within
/// the room.
pub fn max_fillet(path: &BezPath, only: Option<&[usize]>) -> Option<f64> {
    let mut best: Option<f64> = None;
    for sub in subpaths(path) {
        let n = sub.segs.len();
        if n == 0 {
            continue;
        }
        let count = if sub.closed { n } else { n - 1 };
        let lens: Vec<f64> = sub.segs.iter().map(|(s, _)| s.arclen(ACCURACY)).collect();
        // Corner k sits after segment k: its angle when it is a chosen
        // corner.
        let angle: Vec<Option<f64>> = (0..n)
            .map(|k| {
                if k >= count {
                    return None;
                }
                let (seg_in, index) = &sub.segs[k];
                let at_start = sub.closed && k == n - 1;
                if let Some(only) = only {
                    if !(only.contains(index) || (at_start && only.contains(&sub.start_index))) {
                        return None;
                    }
                }
                let (seg_out, _) = &sub.segs[(k + 1) % n];
                let (u_in, u_out) = (end_dir(seg_in)?, start_dir(seg_out)?);
                let theta = (-u_in).dot(u_out).clamp(-1.0, 1.0).acos();
                (1e-3..=std::f64::consts::PI - 1e-3)
                    .contains(&theta)
                    .then_some(theta)
            })
            .collect();
        for k in 0..count {
            let Some(theta) = angle[k] else {
                continue;
            };
            let out_k = (k + 1) % n;
            // The corner before segment k and the one after segment out_k.
            let before = if k > 0 {
                angle[k - 1].is_some()
            } else {
                sub.closed && angle[n - 1].is_some()
            };
            let after = angle[out_k].is_some() && (out_k < count);
            let room_in = if before { lens[k] / 2.0 } else { lens[k] };
            let room_out = if after {
                lens[out_k] / 2.0
            } else {
                lens[out_k]
            };
            let r = largest_fillet(
                &sub.segs[k].0,
                &sub.segs[out_k].0,
                (lens[k], lens[out_k]),
                (room_in, room_out),
                theta,
                best,
            );
            best = Some(best.map_or(r, |b: f64| b.min(r)));
        }
    }
    best
}

/// `path` with its corners cut. With `only`, just the corners at those/// `path` with its corners cut. With `only`, just the corners at those
/// nodes (element indices, as the Shape tool counts them); otherwise every
/// corner. Returns the new path and the number of corners cut.
pub fn cut_corners(path: &BezPath, cut: CornerCut, only: Option<&[usize]>) -> (BezPath, usize) {
    let (t_in_of, t_out_of) = match cut {
        CornerCut::Fillet(r) | CornerCut::Scallop(r) => (r, r),
        CornerCut::Chamfer(a, b) => (a, b),
    };
    let mut out = BezPath::new();
    let mut count = 0;
    if !(t_in_of.is_finite() && t_out_of.is_finite()) || t_in_of <= 0.0 || t_out_of <= 0.0 {
        return (path.clone(), 0);
    }
    for sub in subpaths(path) {
        let n = sub.segs.len();
        if n == 0 {
            continue;
        }
        let lens: Vec<f64> = sub.segs.iter().map(|(s, _)| s.arclen(ACCURACY)).collect();
        let mut start_trim = vec![0.0; n];
        let mut end_trim = vec![0.0; n];
        // Where a cut was made on a segment, when known exactly.
        let mut start_param: Vec<Option<f64>> = vec![None; n];
        let mut end_param: Vec<Option<f64>> = vec![None; n];
        // The piece that replaces the corner after segment k, if cut.
        let mut pieces: Vec<Option<(Point, Point, Vec<PathEl>)>> = vec![None; n];
        let corners = if sub.closed { n } else { n - 1 };
        for k in 0..corners {
            let (seg_in, index) = &sub.segs[k];
            let out_k = (k + 1) % n;
            let (seg_out, _) = &sub.segs[out_k];
            let at_start = sub.closed && k == n - 1;
            if let Some(only) = only {
                let chosen = only.contains(index) || (at_start && only.contains(&sub.start_index));
                if !chosen {
                    continue;
                }
            }
            let (Some(u_in), Some(u_out)) = (end_dir(seg_in), start_dir(seg_out)) else {
                continue;
            };
            // Interior angle between the two segments seen from the corner.
            let theta = (-u_in).dot(u_out).clamp(-1.0, 1.0).acos();
            if !(1e-3..=std::f64::consts::PI - 1e-3).contains(&theta) {
                continue;
            }
            // A fillet touches curved sides where its circle is tangent
            // to both; one that finds no such place does not fit.
            let touch = match cut {
                CornerCut::Fillet(r) => {
                    match fillet_cut(seg_in, seg_out, lens[k], lens[out_k], r, ACCURACY) {
                        Some(c) => Some(c),
                        None => continue,
                    }
                }
                _ => None,
            };
            let (t_in, t_out) = match &touch {
                Some(c) => (c.t_in, c.t_out),
                None => (t_in_of, t_out_of),
            };
            let eps = 1e-9;
            if start_trim[k] + t_in > lens[k] + eps || end_trim[out_k] + t_out > lens[out_k] + eps {
                continue;
            }
            if k == out_k && start_trim[k] + t_in + t_out > lens[k] + eps {
                continue;
            }
            let (ti, to) = match &touch {
                Some(c) => (c.ti, c.to),
                None => (
                    param_at(seg_in, lens[k], lens[k] - t_in),
                    param_at(seg_out, lens[out_k], t_out),
                ),
            };
            let p_in = seg_in.eval(ti);
            let p_out = seg_out.eval(to);
            let corner = seg_in.eval(1.0);
            let mut els = Vec::new();
            match cut {
                CornerCut::Chamfer(..) => els.push(PathEl::LineTo(p_out)),
                CornerCut::Fillet(_) => {
                    let (Some(d_in), Some(d_out)) = (dir_at(seg_in, ti), dir_at(seg_out, to))
                    else {
                        continue;
                    };
                    let sweep = turn(d_in, d_out);
                    // The circle tangent to the incoming side at the cut,
                    // on the side the path turns to.
                    let normal = if sweep >= 0.0 {
                        Vec2::new(-d_in.y, d_in.x)
                    } else {
                        Vec2::new(d_in.y, -d_in.x)
                    };
                    let half = sweep.abs() / 2.0;
                    let chord = (p_out - p_in).hypot();
                    let r = if half.sin() > 1e-9 {
                        chord / (2.0 * half.sin())
                    } else {
                        chord
                    };
                    arc_to(&mut els, p_in + normal * r, p_in, p_out, sweep);
                }
                CornerCut::Scallop(_) => {
                    let sweep = turn(p_in - corner, p_out - corner);
                    arc_to(&mut els, corner, p_in, p_out, sweep);
                }
            }
            end_trim[k] = t_in;
            start_trim[out_k] = t_out;
            if touch.is_some() {
                end_param[k] = Some(ti);
                start_param[out_k] = Some(to);
            }
            pieces[k] = Some((p_in, p_out, els));
            count += 1;
        }
        // Rebuild: each segment between its trims, then the piece that
        // replaces the corner after it.
        let first_t =
            start_param[0].unwrap_or_else(|| param_at(&sub.segs[0].0, lens[0], start_trim[0]));
        out.move_to(sub.segs[0].0.eval(first_t));
        for k in 0..n {
            let (seg, _) = &sub.segs[k];
            let t0 = start_param[k].unwrap_or_else(|| param_at(seg, lens[k], start_trim[k]));
            let t1 = end_param[k].unwrap_or_else(|| param_at(seg, lens[k], lens[k] - end_trim[k]));
            // The close draws the closing line when nothing follows it.
            let closes = sub.closing_line && k == n - 1 && pieces[k].is_none();
            if t1 - t0 > 1e-9 && !closes {
                match seg.subsegment(t0..t1) {
                    PathSeg::Line(l) => out.line_to(l.p1),
                    PathSeg::Quad(q) => out.quad_to(q.p1, q.p2),
                    PathSeg::Cubic(c) => out.curve_to(c.p1, c.p2, c.p3),
                }
            }
            if let Some((_, _, els)) = &pieces[k] {
                for el in els {
                    out.push(*el);
                }
            }
        }
        if sub.closed {
            out.close_path();
        }
    }
    (out, count)
}

/// A rounded corner of a curve: a circular arc (one cubic or more)
/// between two sides and tangent to both, as a fillet leaves it. The
/// sides, carried on, meet at the sharp corner it rounds (#872): straight
/// sides along their lines, curved ones along their own curves (#2223).
#[derive(Debug, Clone, PartialEq)]
pub struct Fillet {
    /// The arc's nodes (element indices, as the Shape tool counts them):
    /// where it leaves the incoming side, its inner nodes, and where it
    /// meets the outgoing side.
    pub nodes: Vec<usize>,
    /// The sharp corner: where the two sides meet when extended.
    pub corner: Point,
    /// The unit direction from the sharp corner into it.
    pub inside: Vec2,
    /// The angle between the sides (radians).
    pub theta: f64,
    /// The arc's radius.
    pub radius: f64,
    /// How far the arc's middle lies from the sharp corner, along
    /// `inside`. Between straight sides that is [`depth`](Self::depth);
    /// curved sides part from the corner at their own pace.
    pub middle: f64,
}

impl Fillet {
    /// How far the arc's middle lies from the sharp corner, along
    /// `inside`: `r (1 / sin(theta / 2) - 1)`.
    pub fn depth(&self) -> f64 {
        let s = (self.theta / 2.0).sin();
        if s > 1e-9 {
            self.radius * (1.0 / s - 1.0)
        } else {
            0.0
        }
    }
}

/// A fillet and where its pieces sit in its subpath: the subpath, the
/// incoming side, the arc's segments and the outgoing side (segment
/// positions in [`Sub::segs`]).
struct FilletAt {
    sub: usize,
    side_in: usize,
    arc: Vec<usize>,
    side_out: usize,
    /// Where the sharp corner lies on the two sides' own curves: past
    /// the end of the incoming one (above 1) and before the start of the
    /// outgoing one (below 0).
    ext_in: f64,
    ext_out: f64,
    fillet: Fillet,
}

/// The longest arc looked for between curved sides, in segments: a
/// fillet's arc has one per quarter turn or so.
const MAX_ARC: usize = 6;

fn fillets_at(subs: &[Sub]) -> Vec<FilletAt> {
    let mut out = Vec::new();
    for (si, sub) in subs.iter().enumerate() {
        let n = sub.segs.len();
        if n < 3 {
            continue;
        }
        let is_line = |k: usize| matches!(sub.segs[k].0, PathSeg::Line(_));
        let last = if sub.closed { n } else { n - 1 };
        for k in 0..last {
            if !is_line(k) {
                continue;
            }
            let mut arc = Vec::new();
            let mut m = k + 1;
            loop {
                if !sub.closed && m >= n {
                    break;
                }
                let j = m % n;
                if j == k || is_line(j) {
                    break;
                }
                arc.push(j);
                m += 1;
            }
            if arc.is_empty() || (!sub.closed && m >= n) {
                continue;
            }
            let side_out = m % n;
            if side_out == k {
                continue;
            }
            if let Some(fillet) = fillet_of(sub, k, &arc, side_out) {
                out.push(FilletAt {
                    sub: si,
                    side_in: k,
                    arc,
                    side_out,
                    ext_in: 1.0,
                    ext_out: 0.0,
                    fillet,
                });
            }
        }
        // Arcs with a curved side (#2223): after each segment, the run
        // of curves that makes a fillet with the segment after it.
        let from = out.len();
        for k in 0..last {
            if out.iter().any(|f| f.sub == si && f.side_in == k) {
                continue;
            }
            let mut arc = Vec::new();
            for len in 1..=MAX_ARC {
                let m = k + len;
                if !sub.closed && m + 1 >= n {
                    break;
                }
                let j = m % n;
                let side_out = (m + 1) % n;
                if j == k || side_out == k || is_line(j) || arc.contains(&side_out) {
                    break;
                }
                arc.push(j);
                if is_line(k) && is_line(side_out) {
                    continue;
                }
                if let Some((fillet, ext_in, ext_out)) = curved_fillet_of(sub, k, &arc, side_out) {
                    out.push(FilletAt {
                        sub: si,
                        side_in: k,
                        arc: arc.clone(),
                        side_out,
                        ext_in,
                        ext_out,
                        fillet,
                    });
                    break;
                }
            }
        }
        if out.len() > from {
            out.sort_by_key(|f| (f.sub, f.side_in));
        }
    }
    out
}

/// The fillet the segments `arc` of `sub` make between the sides
/// `side_in` and `side_out`, one of them curved, with where the sharp
/// corner lies on each side's own curve (#2223): a circular arc tangent
/// to both sides, which carried on past the arc cross at a corner, and
/// neither of which runs on along the arc's own circle (an arc of a
/// round shape is no rounded corner).
fn curved_fillet_of(
    sub: &Sub,
    side_in: usize,
    arc: &[usize],
    side_out: usize,
) -> Option<(Fillet, f64, f64)> {
    let (s_in, s_out) = (&sub.segs[side_in].0, &sub.segs[side_out].0);
    let d_in = end_dir(s_in)?;
    let d_out = start_dir(s_out)?;
    const SMOOTH: f64 = 1.0 - 1e-6;
    let segs: Vec<&PathSeg> = arc.iter().map(|j| &sub.segs[*j].0).collect();
    let first = segs.first()?;
    let last = segs.last()?;
    if start_dir(first)?.dot(d_in) < SMOOTH || end_dir(last)?.dot(d_out) < SMOOTH {
        return None;
    }
    for w in segs.windows(2) {
        if end_dir(w[0])?.dot(start_dir(w[1])?) < SMOOTH {
            return None;
        }
    }
    let sweep = turn(d_in, d_out);
    if !(1e-3..=std::f64::consts::PI - 1e-3).contains(&sweep.abs()) {
        return None;
    }
    let mut turned = 0.0;
    for s in &segs {
        turned += turn(start_dir(s)?, end_dir(s)?);
    }
    if (turned - sweep).abs() > 1e-3 {
        return None;
    }
    // The circle through both ends of the arc, tangent to the sides
    // there.
    let (p0, p1) = (s_in.eval(1.0), s_out.eval(0.0));
    let side = if sweep >= 0.0 { 1.0 } else { -1.0 };
    let radius = (p1 - p0).hypot() / (2.0 * (sweep.abs() / 2.0).sin());
    if !(radius > 1e-9 && radius.is_finite()) {
        return None;
    }
    let center = p0 + normal_of(d_in, side) * radius;
    let tol = 2e-3 * radius + 1e-7;
    for s in &segs {
        for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
            if ((s.eval(t) - center).hypot() - radius).abs() > tol {
                return None;
            }
        }
    }
    // A side that bends as the arc does carries the same circle on.
    let same = |k: f64| (side * k * radius - 1.0).abs() < 0.02;
    if same(curvature(s_in, 1.0)) || same(curvature(s_out, 0.0)) {
        return None;
    }
    // The sides carried on cross at the sharp corner: from where their
    // tangents meet, to where the curves themselves do.
    let (v_in, v_out) = (vel(s_in, 1.0), vel(s_out, 0.0));
    let reach = radius * (sweep.abs() / 2.0).tan();
    let (speed_in, speed_out) = (v_in.hypot(), v_out.hypot());
    if speed_in < 1e-9 || speed_out < 1e-9 {
        return None;
    }
    let (mut a, mut b) = (1.0 + reach / speed_in, -reach / speed_out);
    let mut met = false;
    for _ in 0..40 {
        let f = s_in.eval(a) - s_out.eval(b);
        if f.hypot() < 1e-10 * (1.0 + radius) {
            met = true;
            break;
        }
        let (ja, jb) = (vel(s_in, a), vel(s_out, b));
        let det = jb.cross(ja);
        if det.abs() < 1e-14 || !det.is_finite() {
            return None;
        }
        a += f.cross(jb) / det;
        b += f.cross(ja) / det;
        // Far past the ends the sides' own curves say nothing.
        if !(a.is_finite() && b.is_finite()) || a > 200.0 || b < -200.0 {
            return None;
        }
    }
    if !met || a <= 1.0 + 1e-9 || b >= -1e-9 {
        return None;
    }
    let corner = s_in.eval(a);
    let (u_in, u_out) = (unit(vel(s_in, a))?, unit(vel(s_out, b))?);
    let theta = (-u_in).dot(u_out).clamp(-1.0, 1.0).acos();
    if !(1e-3..=std::f64::consts::PI - 1e-3).contains(&theta) || turn(u_in, u_out) * side <= 0.0 {
        return None;
    }
    // The corner lies outside the arc, on the far side of its middle.
    let out_of = corner - center;
    if out_of.hypot() <= radius * (1.0 + 1e-9) {
        return None;
    }
    let inside = unit(u_out - u_in)?;
    let middle = (center + out_of * (radius / out_of.hypot()) - corner).dot(inside);
    let mut nodes = vec![sub.segs[side_in].1];
    nodes.extend(arc.iter().map(|j| sub.segs[*j].1));
    Some((
        Fillet {
            nodes,
            corner,
            inside,
            theta,
            radius,
            middle,
        },
        a,
        b,
    ))
}

/// The fillet the segments `arc` of `sub` make between the sides
/// `side_in` and `side_out`, if they are a circular arc tangent to both.
fn fillet_of(sub: &Sub, side_in: usize, arc: &[usize], side_out: usize) -> Option<Fillet> {
    let (PathSeg::Line(l_in), PathSeg::Line(l_out)) = (sub.segs[side_in].0, sub.segs[side_out].0)
    else {
        return None;
    };
    let d_in = unit(l_in.p1 - l_in.p0)?;
    let d_out = unit(l_out.p1 - l_out.p0)?;
    let theta = (-d_in).dot(d_out).clamp(-1.0, 1.0).acos();
    if !(1e-3..=std::f64::consts::PI - 1e-3).contains(&theta) {
        return None;
    }
    // Smooth all along: the arc leaves and meets the sides along them.
    const SMOOTH: f64 = 1.0 - 1e-6;
    let segs: Vec<&PathSeg> = arc.iter().map(|j| &sub.segs[*j].0).collect();
    let first = segs.first()?;
    let last = segs.last()?;
    if start_dir(first)?.dot(d_in) < SMOOTH || end_dir(last)?.dot(d_out) < SMOOTH {
        return None;
    }
    for w in segs.windows(2) {
        if end_dir(w[0])?.dot(start_dir(w[1])?) < SMOOTH {
            return None;
        }
    }
    // The sides meet at the corner, as far from both ends of the arc.
    let (p0, p1) = (l_in.p1, l_out.p0);
    let den = d_in.cross(d_out);
    if den.abs() < 1e-12 {
        return None;
    }
    let a = (p1 - p0).cross(d_out) / den;
    let b = d_in.cross(p1 - p0) / den;
    if !(a > 1e-9 && b > 1e-9 && a.is_finite() && b.is_finite()) {
        return None;
    }
    if (a - b).abs() > 1e-6 + 1e-3 * a.max(b) {
        return None;
    }
    let radius = (a + b) / 2.0 * (theta / 2.0).tan();
    let corner = p0 + d_in * a;
    // The arc turns the way the sides do, by as much, and keeps to the
    // circle tangent to both.
    let sweep = turn(d_in, d_out);
    let mut turned = 0.0;
    for s in &segs {
        turned += turn(start_dir(s)?, end_dir(s)?);
    }
    if (turned - sweep).abs() > 1e-3 {
        return None;
    }
    let normal = if sweep >= 0.0 {
        Vec2::new(-d_in.y, d_in.x)
    } else {
        Vec2::new(d_in.y, -d_in.x)
    };
    let center = p0 + normal * radius;
    let tol = 2e-3 * radius + 1e-7;
    for s in &segs {
        for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
            if ((s.eval(t) - center).hypot() - radius).abs() > tol {
                return None;
            }
        }
    }
    let inside = unit(d_out - d_in)?;
    let mut nodes = vec![sub.segs[side_in].1];
    nodes.extend(arc.iter().map(|j| sub.segs[*j].1));
    let s = (theta / 2.0).sin();
    Some(Fillet {
        nodes,
        corner,
        inside,
        theta,
        radius,
        middle: if s > 1e-9 {
            radius * (1.0 / s - 1.0)
        } else {
            0.0
        },
    })
}

/// The rounded corners of `path`: circular arcs between two sides,
/// straight or curved, tangent to both, as [`cut_corners`] fillets them,
/// in path order.
pub fn fillets(path: &BezPath) -> Vec<Fillet> {
    fillets_at(&subpaths(path))
        .into_iter()
        .map(|f| f.fillet)
        .collect()
}

/// `path` with the rounded corners `which` (positions in
/// [`fillets`]`(path)`; every one without it) taken back off: their sides
/// run on to the sharp corner again. Also returns, for each element index
/// of `path`, the index of the same node in the new path (the nodes of a
/// rounded corner taken off become its sharp corner); `None` for the
/// elements that are no node of it.
pub fn unround(path: &BezPath, which: Option<&[usize]>) -> (BezPath, Vec<Option<usize>>) {
    let normal = crate::nodes::normalize(path);
    let subs = subpaths(&normal);
    let all = fillets_at(&subs);
    let chosen: Vec<&FilletAt> = all
        .iter()
        .enumerate()
        .filter(|(i, _)| which.is_none_or(|w| w.contains(i)))
        .map(|(_, f)| f)
        .collect();
    let mut map = vec![None; normal.elements().len()];
    let mut out = BezPath::new();
    for (si, sub) in subs.iter().enumerate() {
        let n = sub.segs.len();
        let here: Vec<&&FilletAt> = chosen.iter().filter(|f| f.sub == si).collect();
        if n == 0 {
            // A lone move-to (and its close) stays.
            if let Some(PathEl::MoveTo(p)) = normal.elements().get(sub.start_index) {
                map[sub.start_index] = Some(out.elements().len());
                out.move_to(*p);
                if sub.closed {
                    out.close_path();
                }
            }
            continue;
        }
        let dropped = |k: usize| here.iter().any(|f| f.arc.contains(&k));
        let ends_at = |k: usize| {
            here.iter()
                .find(|f| f.side_in == k)
                .map(|f| f.fillet.corner)
        };
        let starts_at = |k: usize| {
            here.iter()
                .find(|f| f.side_out == k)
                .map(|f| f.fillet.corner)
        };
        // A closed subpath starts on a segment that stays.
        let first = if sub.closed {
            (0..n).find(|k| !dropped(*k)).unwrap_or(0)
        } else {
            0
        };
        let seg_start = |k: usize| starts_at(k).unwrap_or_else(|| sub.segs[k].0.eval(0.0));
        let move_index = out.elements().len();
        out.move_to(seg_start(first));
        if !sub.closed {
            map[sub.start_index] = Some(move_index);
        }
        let order: Vec<usize> = (0..n).map(|i| (first + i) % n).collect();
        let kept: Vec<usize> = order.iter().copied().filter(|k| !dropped(*k)).collect();
        for (pos, &k) in kept.iter().enumerate() {
            let (seg, index) = &sub.segs[k];
            let last = pos + 1 == kept.len();
            match seg {
                PathSeg::Line(l) => {
                    let to = ends_at(k).unwrap_or(l.p1);
                    if sub.closed && last {
                        // The close draws the side back to the start.
                        map[*index] = Some(move_index);
                    } else {
                        map[*index] = Some(out.elements().len());
                        out.line_to(to);
                    }
                }
                PathSeg::Quad(q) => {
                    map[*index] = Some(out.elements().len());
                    out.quad_to(q.p1, q.p2);
                }
                PathSeg::Cubic(c) => {
                    map[*index] = Some(out.elements().len());
                    // A curved side runs on along its own curve to the
                    // sharp corner (#2223).
                    let t0 = here.iter().find(|f| f.side_out == k).map(|f| f.ext_out);
                    let t1 = here.iter().find(|f| f.side_in == k).map(|f| f.ext_in);
                    if t0.is_some() || t1.is_some() {
                        let e = c.subsegment(t0.unwrap_or(0.0)..t1.unwrap_or(1.0));
                        out.curve_to(e.p1, e.p2, ends_at(k).unwrap_or(c.p3));
                    } else {
                        out.curve_to(c.p1, c.p2, c.p3);
                    }
                }
            }
        }
        if sub.closed {
            // The start node is the end of the last segment.
            map[sub.start_index] = sub.segs.last().and_then(|(_, i)| map[*i]);
            out.close_path();
        }
        for f in &here {
            let corner = map[sub.segs[f.side_in].1];
            for node in &f.fillet.nodes {
                map[*node] = corner;
            }
            if sub.closed && f.fillet.nodes.contains(&sub.start_index) {
                map[sub.start_index] = corner;
            }
        }
        // A rounded corner whose arc ended on the start: the start node
        // is now that corner.
        if sub.closed {
            if let Some((_, i)) = sub.segs.last() {
                if map[sub.start_index].is_none() {
                    map[sub.start_index] = map[*i];
                }
            }
        }
    }
    (out, map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Rect, Shape as _};

    /// A square's four corners point inwards at right angles; an open
    /// zigzag has corners at its middle nodes only; the largest fillet
    /// fits the shared sides.
    #[test]
    fn corners_and_the_largest_fillet_they_take() {
        let sq = Rect::new(0.0, 0.0, 10.0, 10.0).to_path(0.1);
        let c = corners(&sq);
        assert_eq!(c.len(), 4, "{c:?}");
        for k in &c {
            assert!((k.theta - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
            // Into the square, towards its centre.
            let to_centre = Point::new(5.0, 5.0) - k.at;
            assert!(k.inside.dot(to_centre) > 0.0, "{k:?}");
        }
        // Every corner: half of each 10 mm side, at 90 degrees.
        assert!((max_fillet(&sq, None).unwrap() - 5.0).abs() < 1e-6);
        // One corner alone takes the whole side.
        let one = [c[1].node];
        assert!((max_fillet(&sq, Some(&one)).unwrap() - 10.0).abs() < 1e-6);
        let mut zig = BezPath::new();
        zig.move_to((0.0, 0.0));
        zig.line_to((10.0, 0.0));
        zig.line_to((10.0, 10.0));
        zig.line_to((20.0, 10.0));
        let c = corners(&zig);
        assert_eq!(c.iter().map(|k| k.node).collect::<Vec<_>>(), [1, 2]);
        assert_eq!(max_fillet(&zig, Some(&[])), None);
        // The fillet at that radius cuts every corner.
        let r = max_fillet(&sq, None).unwrap();
        assert_eq!(cut_corners(&sq, CornerCut::Fillet(r * 0.999), None).1, 4);
    }

    fn square() -> BezPath {
        Rect::new(0.0, 0.0, 10.0, 10.0).to_path(0.01)
    }

    #[test]
    fn fillets_scallops_and_chamfers_cut_the_expected_area() {
        let pi = std::f64::consts::PI;
        for (cut, area) in [
            (CornerCut::Fillet(2.0), 100.0 - (4.0 - pi) * 4.0),
            (CornerCut::Scallop(2.0), 100.0 - pi * 4.0),
            (CornerCut::Chamfer(2.0, 2.0), 92.0),
            (CornerCut::Chamfer(1.0, 3.0), 94.0),
        ] {
            let (p, n) = cut_corners(&square(), cut, None);
            assert_eq!(n, 4, "{cut:?}");
            assert!(
                (p.area().abs() - area).abs() < 0.01,
                "{cut:?}: {}",
                p.area()
            );
            let b = p.bounding_box();
            assert!((b.width() - 10.0).abs() < 1e-6, "{cut:?}: {b:?}");
            // Each corner becomes two nodes; the last cut ends on the
            // start node, which counts once.
            assert_eq!(crate::nodes::nodes(&p).len(), 8, "{cut:?}: {p:?}");
        }
        // Fillets meet the sides smoothly: each arc leaves the side before
        // it along that side and arrives along the side after it.
        let (p, _) = cut_corners(&square(), CornerCut::Fillet(2.0), None);
        let els = p.elements();
        let mut checked = 0;
        for w in els.windows(3) {
            if let (PathEl::LineTo(a), PathEl::CurveTo(c1, c2, b), PathEl::LineTo(next)) =
                (w[0], w[1], w[2])
            {
                let prev = match els.iter().position(|e| *e == w[0]) {
                    Some(i) if i > 0 => match els[i - 1] {
                        PathEl::MoveTo(q) | PathEl::LineTo(q) | PathEl::CurveTo(_, _, q) => q,
                        _ => continue,
                    },
                    _ => continue,
                };
                assert!((a - prev).cross(c1 - a).abs() < 1e-9, "{p:?}");
                assert!((next - b).cross(b - c2).abs() < 1e-9, "{p:?}");
                checked += 1;
            }
        }
        assert!(checked >= 3, "{p:?}");
    }

    #[test]
    fn corners_whose_sides_are_too_short_are_skipped() {
        // A 10 x 4 rectangle with 3 mm fillets: every second corner fits.
        let p = Rect::new(0.0, 0.0, 10.0, 4.0).to_path(0.01);
        let (_, n) = cut_corners(&p, CornerCut::Fillet(3.0), None);
        assert_eq!(n, 2);
        let (_, n) = cut_corners(&p, CornerCut::Fillet(50.0), None);
        assert_eq!(n, 0);
    }

    /// How far each corner rounds on its own: a square's corner with
    /// 10 mm sides takes a 10 mm fillet, whose middle lies
    /// `10 (sqrt 2 - 1)` into it; a corner that barely turns, or whose
    /// sides are very short, rounds next to nothing.
    #[test]
    fn corner_reach_tells_how_far_a_corner_rounds() {
        let sq = Rect::new(0.0, 0.0, 10.0, 10.0).to_path(0.1);
        let reach = corner_reach(&sq);
        assert_eq!(reach.len(), corners(&sq).len());
        for r in &reach {
            assert!((r - 10.0 * (2f64.sqrt() - 1.0)).abs() < 1e-6, "{r}");
        }
        // A turn of half a degree between two 10 mm lines.
        let mut kink = BezPath::new();
        kink.move_to((0.0, 0.0));
        kink.line_to((10.0, 0.0));
        let a = 0.5f64.to_radians();
        kink.line_to((10.0 + 10.0 * a.cos(), 10.0 * a.sin()));
        let reach = corner_reach(&kink);
        assert_eq!((reach.len(), corners(&kink).len()), (1, 1));
        // About the side times the turn over four: a fiftieth of a mm.
        let quarter = 10.0 * a / 4.0;
        assert!((reach[0] - quarter).abs() < quarter * 0.01, "{}", reach[0]);
        // A right angle with a side a hundredth of a millimetre long.
        let mut short = BezPath::new();
        short.move_to((0.0, 0.0));
        short.line_to((10.0, 0.0));
        short.line_to((10.0, 0.01));
        assert!(corner_reach(&short)[0] < 0.01);
    }

    #[test]
    fn smooth_nodes_are_not_corners() {
        let circle = crate::geometry::Circle::new((0.0, 0.0), 5.0).to_path(0.01);
        let (p, n) = cut_corners(&circle, CornerCut::Fillet(1.0), None);
        assert_eq!(n, 0);
        assert!((p.area() - circle.area()).abs() < 1e-6);
    }

    #[test]
    fn only_the_chosen_nodes_are_cut() {
        let p = square();
        // Element 1 ends at the second corner.
        let (q, n) = cut_corners(&p, CornerCut::Chamfer(2.0, 2.0), Some(&[1]));
        assert_eq!(n, 1);
        assert!((q.area().abs() - 98.0).abs() < 1e-9);
        // The start node of a closed path is chosen by its move-to.
        let (_, n) = cut_corners(&p, CornerCut::Chamfer(2.0, 2.0), Some(&[0]));
        assert_eq!(n, 1);
        let (_, n) = cut_corners(&p, CornerCut::Chamfer(2.0, 2.0), Some(&[]));
        assert_eq!(n, 0);
    }

    #[test]
    fn open_curves_keep_their_ends_and_curved_sides_are_filleted() {
        let mut p = BezPath::new();
        p.move_to((0.0, 0.0));
        p.line_to((10.0, 0.0));
        p.curve_to((12.0, 3.0), (12.0, 7.0), (10.0, 10.0));
        let (q, n) = cut_corners(&p, CornerCut::Fillet(1.0), None);
        assert_eq!(n, 1);
        let els = q.elements();
        assert_eq!(els.first(), Some(&PathEl::MoveTo(Point::new(0.0, 0.0))));
        match els.last() {
            Some(PathEl::CurveTo(_, _, end)) => {
                assert!((*end - Point::new(10.0, 10.0)).hypot() < 1e-9)
            }
            other => panic!("{other:?}"),
        }
        // The fillet leaves the straight side along its direction.
        let nodes = crate::nodes::nodes(&q);
        assert!(nodes.len() > 3);
    }

    #[test]
    fn bad_values_change_nothing() {
        for cut in [
            CornerCut::Fillet(0.0),
            CornerCut::Fillet(f64::NAN),
            CornerCut::Scallop(-1.0),
            CornerCut::Chamfer(1.0, f64::INFINITY),
        ] {
            let (p, n) = cut_corners(&square(), cut, None);
            assert_eq!(n, 0);
            assert_eq!(p, square());
        }
        let (p, n) = cut_corners(&BezPath::new(), CornerCut::Fillet(1.0), None);
        assert_eq!(n, 0);
        assert!(p.elements().is_empty());
    }

    /// A filleted square's rounded corners are found again, with the
    /// radius they were cut with and the sharp corner they round; taking
    /// them off gives the square back, and the nodes of each arc map to
    /// its corner node (#872).
    #[test]
    fn rounded_corners_are_found_and_taken_back_off() {
        let sq = square();
        assert!(fillets(&sq).is_empty(), "a sharp square has none");
        let (round, n) = cut_corners(&sq, CornerCut::Fillet(3.0), None);
        assert_eq!(n, 4);
        let found = fillets(&round);
        assert_eq!(found.len(), 4, "{found:?}");
        assert_eq!(
            corners_and_fillets(&round),
            (corners(&round), found.clone())
        );
        let want = [
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
        ];
        for f in &found {
            assert!((f.radius - 3.0).abs() < 1e-6, "{}", f.radius);
            assert!((f.theta - std::f64::consts::FRAC_PI_2).abs() < 1e-6);
            assert!(
                want.iter().any(|w| (*w - f.corner).hypot() < 1e-6),
                "{:?}",
                f.corner
            );
            assert!(f.nodes.len() >= 2);
            // The arc's middle lies `depth` into the corner.
            assert!((f.depth() - 3.0 * (2f64.sqrt() - 1.0)).abs() < 1e-9);
        }
        let (back, map) = unround(&round, None);
        assert!((back.area() - sq.area()).abs() < 1e-6, "{}", back.area());
        assert_eq!(corners(&back).len(), 4);
        assert!(fillets(&back).is_empty());
        assert_eq!(map.len(), round.elements().len());
        let back_corners = corners(&back);
        for f in &found {
            let at = map[f.nodes[0]].expect("mapped");
            for node in &f.nodes {
                assert_eq!(map[*node], Some(at), "{node} of {:?}", f.nodes);
            }
            let c = back_corners
                .iter()
                .find(|c| c.node == at)
                .expect("a corner node");
            assert!((c.at - f.corner).hypot() < 1e-6);
        }
        // Rounded again with another radius: the same as rounding the
        // square with it.
        let (again, _) = cut_corners(&back, CornerCut::Fillet(4.0), None);
        let (direct, _) = cut_corners(&sq, CornerCut::Fillet(4.0), None);
        assert!((again.area() - direct.area()).abs() < 1e-6);
        assert!(fillets(&again)
            .iter()
            .all(|f| (f.radius - 4.0).abs() < 1e-6));
    }

    /// Only the rounded corners asked for are taken off; a corner rounded
    /// on its own, on an open curve, comes back too.
    #[test]
    fn chosen_rounded_corners_are_taken_off_alone() {
        let sq = square();
        // Round the corner at (10, 10) only: node 2.
        let (one, n) = cut_corners(&sq, CornerCut::Fillet(2.0), Some(&[2]));
        assert_eq!(n, 1);
        let found = fillets(&one);
        assert_eq!(found.len(), 1);
        assert!((found[0].corner - Point::new(10.0, 10.0)).hypot() < 1e-6);
        let (back, map) = unround(&one, Some(&[0]));
        assert!((back.area() - sq.area()).abs() < 1e-6);
        let node = map[found[0].nodes[0]].expect("mapped");
        let (rounded, n) = cut_corners(&back, CornerCut::Fillet(1.0), Some(&[node]));
        assert_eq!(n, 1);
        assert!((fillets(&rounded)[0].radius - 1.0).abs() < 1e-6);
        // None asked for: nothing changes.
        let (same, _) = unround(&one, Some(&[]));
        assert!((same.area() - one.area()).abs() < 1e-9);
        assert_eq!(fillets(&same).len(), 1);
        // An open zigzag's inner corners.
        let mut zig = BezPath::new();
        zig.move_to((0.0, 0.0));
        zig.line_to((10.0, 0.0));
        zig.line_to((10.0, 10.0));
        zig.line_to((20.0, 10.0));
        let (r, n) = cut_corners(&zig, CornerCut::Fillet(2.5), None);
        assert_eq!(n, 2);
        let found = fillets(&r);
        assert_eq!(found.len(), 2);
        let (back, _) = unround(&r, None);
        let pts: Vec<Point> = back
            .elements()
            .iter()
            .filter_map(|e| e.end_point())
            .collect();
        assert_eq!(pts.len(), 4, "{pts:?}");
        for (a, b) in pts.iter().zip(
            [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (20.0, 10.0)]
                .iter()
                .map(|p| Point::new(p.0, p.1)),
        ) {
            assert!((*a - b).hypot() < 1e-9, "{a:?} {b:?}");
        }
    }

    /// Curves that only look rounded are not rounded corners: an arc
    /// that is not tangent to its sides, a curve that is no circle, a
    /// circle with no sides, scallops and chamfers.
    #[test]
    fn other_curves_are_not_rounded_corners() {
        let sq = square();
        let (scallop, _) = cut_corners(&sq, CornerCut::Scallop(2.0), None);
        assert!(fillets(&scallop).is_empty());
        let (chamfer, _) = cut_corners(&sq, CornerCut::Chamfer(2.0, 2.0), None);
        assert!(fillets(&chamfer).is_empty());
        let circle = crate::geometry::Circle::new((0.0, 0.0), 5.0).to_path(1e-3);
        assert!(fillets(&circle).is_empty());
        // Tangent to both sides but no circular arc.
        let mut p = BezPath::new();
        p.move_to((0.0, 0.0));
        p.line_to((7.0, 0.0));
        p.curve_to((9.5, 0.0), (10.0, 0.5), (10.0, 3.0));
        p.line_to((10.0, 10.0));
        assert!(fillets(&p).is_empty());
        // Malformed values never panic.
        let mut bad = BezPath::new();
        bad.move_to((f64::NAN, 0.0));
        bad.line_to((1.0, f64::INFINITY));
        bad.curve_to((2.0, 2.0), (3.0, 3.0), (4.0, 4.0));
        bad.line_to((5.0, 0.0));
        let _ = fillets(&bad);
        let _ = unround(&bad, None);
        let _ = unround(&BezPath::new(), None);
    }

    /// Shapes whose corners lie between every pairing of straight and
    /// curved sides, the path's start node among them.
    fn mixed_shapes() -> Vec<(&'static str, BezPath)> {
        let mut out = Vec::new();
        // A lens of two arcs: both corners between two curves, one of
        // them the start node.
        let mut lens = BezPath::new();
        lens.move_to((0.0, 0.0));
        lens.curve_to((10.0, 12.0), (30.0, 12.0), (40.0, 0.0));
        lens.curve_to((30.0, -12.0), (10.0, -12.0), (0.0, 0.0));
        lens.close_path();
        out.push(("lens", lens));
        // A heart: the bottom point (start node) and the notch on top,
        // both between two curves.
        let mut heart = BezPath::new();
        heart.move_to((0.0, 0.0));
        heart.curve_to((-30.0, 20.0), (-15.0, 45.0), (0.0, 30.0));
        heart.curve_to((15.0, 45.0), (30.0, 20.0), (0.0, 0.0));
        heart.close_path();
        out.push(("heart", heart));
        // A half disc: line to curve and curve to line.
        let mut half = BezPath::new();
        half.move_to((0.0, 0.0));
        half.line_to((40.0, 0.0));
        half.curve_to((40.0, 26.0), (0.0, 26.0), (0.0, 0.0));
        half.close_path();
        out.push(("half disc", half));
        // The same, started on the curve, its line drawn by the close.
        let mut half2 = BezPath::new();
        half2.move_to((40.0, 0.0));
        half2.curve_to((40.0, 26.0), (0.0, 26.0), (0.0, 0.0));
        half2.close_path();
        out.push(("half disc, closing line", half2));
        // A drop: two concave curves meeting in a point, and a line.
        let mut drop = BezPath::new();
        drop.move_to((0.0, 0.0));
        drop.curve_to((8.0, 10.0), (12.0, 20.0), (15.0, 40.0));
        drop.curve_to((18.0, 20.0), (22.0, 10.0), (30.0, 0.0));
        drop.line_to((0.0, 0.0));
        drop.close_path();
        out.push(("drop", drop));
        out.push(("square", square()));
        out
    }

    /// Every corner of every pairing of sides is cut by each kind of cut
    /// at a size that fits, alone and together with the others, and no
    /// sharp corner is left where a fillet was made (#2223).
    #[test]
    fn corners_between_curves_are_cut_like_those_between_lines() {
        let mut failures = Vec::new();
        for (name, path) in mixed_shapes() {
            let all = corners(&path);
            assert!(all.len() >= 2, "{name}: {all:?}");
            for (cut_name, cut) in [
                ("fillet", CornerCut::Fillet(2.0)),
                ("scallop", CornerCut::Scallop(2.0)),
                ("chamfer", CornerCut::Chamfer(2.0, 2.0)),
            ] {
                let (q, n) = cut_corners(&path, cut, None);
                if n != all.len() {
                    failures.push(format!("{name} {cut_name} all: {n} of {}", all.len()));
                }
                for c in &all {
                    let (q1, n1) = cut_corners(&path, cut, Some(&[c.node]));
                    if n1 != 1 {
                        failures.push(format!("{name} {cut_name} node {}: {n1}", c.node));
                        continue;
                    }
                    for (which, q) in [("alone", &q1), ("all", &q)] {
                        // The corner point is cut away.
                        let near = crate::nodes::nodes(q)
                            .iter()
                            .any(|k| (k.pos - c.at).hypot() < 1e-6);
                        if near {
                            failures.push(format!(
                                "{name} {cut_name} node {} {which}: corner still there",
                                c.node
                            ));
                        }
                        // A fillet leaves no sharp corner around it.
                        if matches!(cut, CornerCut::Fillet(_)) {
                            for k in corners(q) {
                                if (k.at - c.at).hypot() < 6.0 {
                                    failures.push(format!(
                                        "{name} fillet node {} {which}: sharp at {:?}, turn {:.3} deg",
                                        c.node,
                                        k.at,
                                        180.0 - k.theta.to_degrees()
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// A fillet between curved sides is the circle of its radius tangent
    /// to both, so it is found again with that radius and the sharp
    /// corner it rounds, as one between straight sides is, and taking it
    /// back off gives the curve back (#2223). Before, the arc was only
    /// near a circle, was never found again, and the corner could not be
    /// rounded a second time.
    #[test]
    fn fillets_between_curved_sides_are_found_and_taken_back_off() {
        // The same segments, whether the close draws the last line or a
        // line-to does.
        let same = |a: &BezPath, b: &BezPath| {
            let pts = |p: &BezPath| -> Vec<Point> {
                p.segments()
                    .flat_map(|seg| {
                        let c = seg.to_cubic();
                        [c.p0, c.p1, c.p2, c.p3]
                    })
                    .collect()
            };
            let (pa, pb) = (pts(a), pts(b));
            pa.len() == pb.len() && pa.iter().zip(&pb).all(|(p, q)| (*p - *q).hypot() < 1e-6)
        };
        for (name, path) in mixed_shapes() {
            let sharp = corners(&path);
            let r = 1.5;
            for c in &sharp {
                let (q, n) = cut_corners(&path, CornerCut::Fillet(r), Some(&[c.node]));
                assert_eq!(n, 1, "{name} node {}", c.node);
                let found = fillets(&q);
                assert_eq!(found.len(), 1, "{name} node {}: {found:?}", c.node);
                let f = &found[0];
                assert!((f.radius - r).abs() < 1e-6, "{name}: {}", f.radius);
                assert!((f.corner - c.at).hypot() < 1e-6, "{name}: {:?}", f.corner);
                assert!((f.theta - c.theta).abs() < 1e-6, "{name}: {}", f.theta);
                assert!((f.inside - c.inside).hypot() < 1e-6, "{name}");
                assert!(f.middle > 0.0 && f.middle < r * 8.0, "{name}: {}", f.middle);
                // The arc's centre lies `r` from both sides: the fillet
                // touches them, it does not cross them.
                let els = q.elements();
                let at = |i: usize| match els[i] {
                    PathEl::MoveTo(p) | PathEl::LineTo(p) | PathEl::CurveTo(_, _, p) => p,
                    other => panic!("{other:?}"),
                };
                let PathEl::CurveTo(c1, c2, p3) = els[f.nodes[1]] else {
                    panic!("{name}: {:?}", els[f.nodes[1]]);
                };
                let p0 = at(f.nodes[0]);
                let mid = CubicBez::new(p0, c1, c2, p3).eval(0.5);
                // Equally far from three points of the arc.
                let (u, v) = (mid - p0, p3 - p0);
                let den = 2.0 * u.cross(v);
                let centre = p0
                    + Vec2::new(
                        v.y * u.hypot2() - u.y * v.hypot2(),
                        u.x * v.hypot2() - v.x * u.hypot2(),
                    ) / den;
                assert!(((centre - p0).hypot() - r).abs() < 1e-4, "{name}");
                let mut nearest = f64::INFINITY;
                for seg in path.segments() {
                    let d = kurbo::ParamCurveNearest::nearest(&seg, centre, 1e-9)
                        .distance_sq
                        .sqrt();
                    nearest = nearest.min(d);
                }
                assert!(
                    (nearest - r).abs() < 1e-4,
                    "{name} node {}: {nearest}",
                    c.node
                );
                // Taken off, the curve is the one that was rounded.
                let (back, _) = unround(&q, None);
                assert!(same(&back, &path), "{name} node {}:\n{back:?}", c.node);
                // And it rounds again, at another radius.
                let again = corners(&back)
                    .into_iter()
                    .find(|k| (k.at - c.at).hypot() < 1e-6)
                    .expect("the corner again");
                let (q2, n2) = cut_corners(&back, CornerCut::Fillet(0.5), Some(&[again.node]));
                assert_eq!(n2, 1, "{name}");
                let f2 = fillets(&q2);
                assert_eq!(f2.len(), 1, "{name}");
                assert!(
                    (f2[0].radius - 0.5).abs() < 1e-6,
                    "{name}: {}",
                    f2[0].radius
                );
            }
            // Every corner at once.
            let (q, n) = cut_corners(&path, CornerCut::Fillet(r), None);
            assert_eq!(n, sharp.len(), "{name}");
            let found = fillets(&q);
            assert_eq!(found.len(), sharp.len(), "{name}: {found:?}");
            assert!(same(&unround(&q, None).0, &path), "{name}");
        }
        // Round shapes have no rounded corners: each arc of a circle
        // carries its neighbours on.
        let circle = crate::geometry::Circle::new((0.0, 0.0), 5.0).to_path(1e-3);
        assert!(fillets(&circle).is_empty());
        let ellipse = crate::geometry::Ellipse::new((0.0, 0.0), (8.0, 5.0), 0.3).to_path(1e-3);
        assert!(fillets(&ellipse).is_empty());
    }

    /// The largest fillet between curved sides is the largest circle
    /// that touches both within them: a lens of two arcs 40 mm long and
    /// 18 mm thick takes 9 mm at both tips, the circle inside its middle,
    /// and every radius up to that one is cut (#2223). A half disc takes
    /// the same at both ends of its straight side.
    #[test]
    fn the_largest_fillet_between_curved_sides_is_the_circle_that_fits() {
        let shapes = mixed_shapes();
        let lens = &shapes[0].1;
        let max = max_fillet(lens, None).expect("corners");
        assert!((max - 9.0).abs() < 1e-3, "{max}");
        for k in [0.01, 0.1, 0.5, 0.9, 0.999] {
            let (q, n) = cut_corners(lens, CornerCut::Fillet(max * k), None);
            assert_eq!(n, 2, "{k}");
            assert!(corners(&q).is_empty(), "{k}");
        }
        // Nothing larger fits, and the lens stays as it is.
        let (q, n) = cut_corners(lens, CornerCut::Fillet(max * 1.01), None);
        assert_eq!((n, &q), (0, &crate::nodes::normalize(lens)));
        for (name, path) in &shapes {
            for c in corners(path) {
                let only = [c.node];
                let max = max_fillet(path, Some(&only)).expect("a corner");
                assert!(max > 1.0, "{name} node {}: {max}", c.node);
                let (_, n) = cut_corners(path, CornerCut::Fillet(max * 0.999), Some(&only));
                assert_eq!(n, 1, "{name} node {}: {max}", c.node);
            }
        }
        // Both ends of the half disc's straight side round alike.
        let half = &shapes[2].1;
        let c = corners(half);
        let area = |node: usize| {
            cut_corners(half, CornerCut::Fillet(3.0), Some(&[node]))
                .0
                .area()
        };
        assert!((area(c[0].node) - area(c[1].node)).abs() < 1e-6);
    }
}
