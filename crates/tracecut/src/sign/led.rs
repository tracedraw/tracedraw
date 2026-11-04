//! LED modules laid out inside closed letters and shapes.
//!
//! A module is a rectangle (W x H, a number of diodes) or a single round
//! diode (diameter W). Four layouts:
//!
//! - [`LedMode::Matrix`]: rows `row_pitch` apart, modules `module_pitch`
//!   apart (centre to centre) in each row, inside the shape shrunk by the
//!   edge distance. Each row is a band of the module's height; where the
//!   whole band lies inside, the modules of that stretch are centred in it.
//! - [`LedMode::Shifted`]: the same with every second row moved by half a
//!   module pitch (a brick pattern for even light).
//! - [`LedMode::Centre`]: along the centre lines of narrow strokes. The
//!   shape is sampled on a grid, the distance of each cell to the outline
//!   computed, and modules put on the ridge of that distance (the cells
//!   farther from the outline than their neighbours across), one module
//!   pitch apart along it, turned along the stroke.
//! - [`LedMode::Outline`]: along the outline at a distance inside (the
//!   shape offset inwards), evenly spread, turned along it.
//! - [`LedMode::Island`]: rings along the outline: the shape offset
//!   inwards again and again by the row spacing, modules evenly spread on
//!   each ring.
//! - [`LedMode::Random`]: scattered at random (a seed makes it
//!   repeatable), no two centres closer than the module spacing.
//! - [`LedMode::OnCurve`]: on the curves themselves (open or closed),
//!   evenly spread, turned along them; no inside check.
//! - [`LedMode::AroundCurve`]: two rows along the curves, one each side
//!   at the side distance; no inside check.
//!
//! In every fill mode a module is kept only when it lies inside the shape
//! at least the edge distance from the outline (checked at its corners
//! and the middles of its sides; a round diode by its centre). With "fix
//! overlaps" a module that comes closer than the overlap gap to one kept
//! before it is left out.

use super::{edge_dist, finite_pt, inside, region, region_polys, FLAT_TOL};
use tracedraw_core::geometry::{Affine, BezPath, Point, Rect, Shape as _, Vec2};
use tracedraw_core::shaping::{self, Join};

/// One entry of the module catalogue.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Module {
    pub name: String,
    /// Width (mm); the diameter of a round diode.
    pub width: f64,
    /// Height (mm); not used by a round diode.
    pub height: f64,
    /// A single round diode instead of a rectangular module.
    pub round: bool,
    /// Diodes on the module.
    pub diodes: u32,
    /// Power of one module (W).
    pub power: f64,
    /// Price of one module.
    pub price: f64,
    /// Colour the modules are drawn in (RGB).
    pub color: [u8; 3],
}

impl Default for Module {
    fn default() -> Self {
        Module {
            name: String::new(),
            width: 40.0,
            height: 12.0,
            round: false,
            diodes: 2,
            power: 0.48,
            price: 0.40,
            color: [255, 200, 0],
        }
    }
}

impl Module {
    /// The footprint size: width along the module's direction and height
    /// across it, positive and finite (at least 0.1 mm).
    pub fn footprint(&self, rotate: bool) -> (f64, f64) {
        let clean = |v: f64| if v.is_finite() { v.abs().max(0.1) } else { 0.1 };
        let w = clean(self.width);
        if self.round {
            return (w, w);
        }
        let h = clean(self.height);
        if rotate {
            (h, w)
        } else {
            (w, h)
        }
    }
}

/// The catalogue new documents start with (editable in the panel).
pub fn catalogue() -> Vec<Module> {
    let m = |name: &str, w, h, round, diodes, power, price, color| Module {
        name: name.to_string(),
        width: w,
        height: h,
        round,
        diodes,
        power,
        price,
        color,
    };
    vec![
        m(
            "2-LED module 0.48 W",
            40.0,
            12.0,
            false,
            2,
            0.48,
            0.40,
            [255, 200, 0],
        ),
        m(
            "3-LED module 0.72 W",
            75.0,
            15.0,
            false,
            3,
            0.72,
            0.55,
            [255, 170, 0],
        ),
        m(
            "1-LED lens module 0.5 W",
            30.0,
            15.0,
            false,
            1,
            0.5,
            0.45,
            [0, 190, 255],
        ),
        m(
            "3-LED high power module 1.5 W",
            70.0,
            20.0,
            false,
            3,
            1.5,
            1.20,
            [255, 90, 0],
        ),
        m(
            "Round diode 8 mm",
            8.0,
            8.0,
            true,
            1,
            0.06,
            0.08,
            [120, 220, 60],
        ),
    ]
}

/// How the modules are laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum LedMode {
    #[default]
    Matrix,
    Shifted,
    Centre,
    Outline,
    Island,
    Random,
    OnCurve,
    AroundCurve,
}

impl LedMode {
    /// The modes that follow curves.
    pub const CURVE: [LedMode; 3] = [LedMode::OnCurve, LedMode::AroundCurve, LedMode::Outline];
    /// The modes that fill closed shapes.
    pub const FILL: [LedMode; 5] = [
        LedMode::Matrix,
        LedMode::Shifted,
        LedMode::Island,
        LedMode::Centre,
        LedMode::Random,
    ];

    /// The mode follows curves rather than filling shapes.
    pub fn on_curve(self) -> bool {
        Self::CURVE.contains(&self)
    }
}

/// The layout settings (millimetres).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct LedSpec {
    pub mode: LedMode,
    /// Distance between rows, centre to centre.
    pub row_pitch: f64,
    /// Distance between modules along a row, a centre line or the
    /// outline, centre to centre.
    pub module_pitch: f64,
    /// Smallest distance from a module to the outline.
    pub edge: f64,
    /// Distance from the outline to the module centres in Outline mode.
    pub outline_offset: f64,
    /// Turn the modules a quarter (rows of upright modules).
    pub rotate: bool,
    /// Distance from the curve to each row in AroundCurve mode.
    pub side_distance: f64,
    /// Seed of the Random mode.
    pub seed: u64,
    /// Leave out modules that come too close to one placed before.
    pub fix_overlaps: bool,
    /// Smallest gap between two modules' footprints with `fix_overlaps`.
    pub overlap_gap: f64,
}

impl Default for LedSpec {
    fn default() -> Self {
        LedSpec {
            mode: LedMode::Matrix,
            row_pitch: 40.0,
            module_pitch: 60.0,
            edge: 5.0,
            outline_offset: 15.0,
            rotate: false,
            side_distance: 10.0,
            seed: 1,
            fix_overlaps: false,
            overlap_gap: 0.0,
        }
    }
}

/// Where a module goes: its centre and its turn (radians, counter-
/// clockwise; 0 runs along x).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub center: Point,
    pub angle: f64,
}

/// Most modules one layout makes.
pub const MAX_MODULES: usize = 20_000;

/// Lay modules out in the closed shapes of one object (its outlines in
/// page space), or along its curves in the curve modes.
pub fn layout(paths: &[BezPath], module: &Module, spec: &LedSpec) -> Vec<Placement> {
    let (fw, fh) = module.footprint(spec.rotate);
    let clean = |v: f64, min: f64| if v.is_finite() { v.max(min) } else { min };
    let edge = clean(spec.edge, 0.0);
    let row_pitch = clean(spec.row_pitch, fh);
    let pitch = clean(spec.module_pitch, fw);
    let mut out = match spec.mode {
        LedMode::OnCurve | LedMode::AroundCurve => {
            let side = (spec.mode == LedMode::AroundCurve).then(|| clean(spec.side_distance, 0.0));
            on_curves(paths, pitch, side)
        }
        _ => {
            let region = region(paths);
            if region.elements().is_empty() {
                return Vec::new();
            }
            let polys = region_polys(&region, FLAT_TOL);
            let mut out = match spec.mode {
                LedMode::Matrix | LedMode::Shifted => matrix(
                    &region,
                    fw,
                    fh,
                    row_pitch,
                    pitch,
                    edge,
                    spec.mode == LedMode::Shifted,
                ),
                LedMode::Centre => centre(&polys, module.round, fw, fh, pitch, edge),
                LedMode::Island => {
                    let first = edge + if module.round { fw / 2.0 } else { fh / 2.0 };
                    island(&region, pitch, first, row_pitch)
                }
                LedMode::Random => random(&polys, module.round, (fw, fh), pitch, edge, spec.seed),
                _ => {
                    let off = clean(spec.outline_offset, 0.0);
                    along_outline(&region, pitch, off)
                }
            };
            if matches!(spec.mode, LedMode::Outline | LedMode::Island) {
                out.retain(|p| fits(&polys, *p, fw, fh, edge, module.round));
            }
            out
        }
    };
    if spec.fix_overlaps {
        out = remove_overlaps(out, fw, fh, clean(spec.overlap_gap, 0.0), module.round);
    }
    out.truncate(MAX_MODULES);
    out
}

/// Positions spread evenly along a polyline, each turned along the line
/// there: on a closed one `n` at the middles of `n` equal stretches, on
/// an open one from end to end (one in the middle when it is shorter than
/// the pitch).
fn spread_along(pts: &[Point], closed: bool, pitch: f64) -> Vec<Placement> {
    let n = pts.len();
    let seg_count = if closed { n } else { n.saturating_sub(1) };
    let segs: Vec<(Point, Point, f64)> = (0..seg_count)
        .filter_map(|i| {
            let a = *pts.get(i)?;
            let b = *pts.get((i + 1) % n)?;
            Some((a, b, (b - a).hypot()))
        })
        .collect();
    let total: f64 = segs.iter().map(|s| s.2).sum();
    if !total.is_finite() || total < 1e-6 || !(pitch.is_finite() && pitch > 0.0) {
        return Vec::new();
    }
    let raw = if closed {
        (total / pitch).floor().max(1.0)
    } else {
        (total / pitch + 1e-9).floor() + 1.0
    };
    if raw > MAX_MODULES as f64 {
        return Vec::new();
    }
    let count = raw as usize;
    let wants: Vec<f64> = if closed {
        let step = total / count as f64;
        (0..count).map(|k| (k as f64 + 0.5) * step).collect()
    } else if count < 2 {
        vec![total / 2.0]
    } else {
        let step = total / (count - 1) as f64;
        (0..count).map(|k| k as f64 * step).collect()
    };
    let mut out = Vec::with_capacity(wants.len());
    let mut run = 0.0;
    let mut k = 0usize;
    for (i, (a, b, len)) in segs.iter().enumerate() {
        let slack = if i + 1 == segs.len() { 1e-6 } else { 0.0 };
        while let Some(want) = wants.get(k) {
            if *want > run + len + slack {
                break;
            }
            let t = if *len > 1e-12 {
                ((want - run) / len).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let d = *b - *a;
            out.push(Placement {
                center: *a + d * t,
                angle: d.y.atan2(d.x),
            });
            k += 1;
        }
        run += len;
    }
    out
}

/// Modules on the curves themselves, or (with `side`) in two rows at
/// `side` from them.
fn on_curves(paths: &[BezPath], pitch: f64, side: Option<f64>) -> Vec<Placement> {
    let mut out = Vec::new();
    for p in paths {
        for poly in super::polylines(p, FLAT_TOL) {
            for at in spread_along(&poly.pts, poly.closed, pitch) {
                match side {
                    None => out.push(at),
                    Some(d) => {
                        let (s, c) = at.angle.sin_cos();
                        let n = Vec2::new(-s, c) * d;
                        out.push(Placement {
                            center: at.center + n,
                            angle: at.angle,
                        });
                        out.push(Placement {
                            center: at.center - n,
                            angle: at.angle,
                        });
                    }
                }
                if out.len() >= MAX_MODULES {
                    return out;
                }
            }
        }
    }
    out
}

/// Rings: the shape offset inwards by `first`, then again and again by
/// `step`, modules spread along each ring.
fn island(region: &BezPath, pitch: f64, first: f64, step: f64) -> Vec<Placement> {
    let mut out = Vec::new();
    let mut off = first;
    let step = step.max(0.1);
    for _ in 0..10_000 {
        let ring = if off > 1e-9 {
            shaping::offset_with(region, -off, Join::Round)
        } else {
            region.clone()
        };
        let polys = region_polys(&ring, FLAT_TOL);
        if polys.is_empty() {
            break;
        }
        for poly in &polys {
            out.extend(spread_along(poly, true, pitch));
            if out.len() >= MAX_MODULES {
                return out;
            }
        }
        off += step;
    }
    out
}

/// A small repeatable random number generator (xorshift).
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    /// A number in [0, 1).
    fn next(&mut self) -> f64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Scattered modules: random tries inside the shape's box, a module kept
/// when it fits and no kept centre is closer than the pitch.
fn random(
    polys: &[Vec<Point>],
    round: bool,
    (fw, fh): (f64, f64),
    pitch: f64,
    edge: f64,
    seed: u64,
) -> Vec<Placement> {
    let Some(bb) = super::points_bbox(polys.iter().flatten().copied()) else {
        return Vec::new();
    };
    let area = bb.width() * bb.height();
    let tries = (30.0 * area / (pitch * pitch) + 100.0).min(200_000.0);
    if !tries.is_finite() {
        return Vec::new();
    }
    let mut rng = Rng::new(seed);
    let mut grid = Buckets::new(pitch);
    let mut out = Vec::new();
    for _ in 0..tries as usize {
        let c = Point::new(
            bb.x0 + rng.next() * bb.width(),
            bb.y0 + rng.next() * bb.height(),
        );
        if grid
            .near(c, pitch)
            .any(|q| (q.center - c).hypot() < pitch - 1e-9)
        {
            continue;
        }
        let at = Placement {
            center: c,
            angle: 0.0,
        };
        if !fits(polys, at, fw, fh, edge, round) {
            continue;
        }
        grid.add(at);
        out.push(at);
        if out.len() >= MAX_MODULES {
            break;
        }
    }
    out
}

/// Placements bucketed on a square grid to find near ones fast.
struct Buckets {
    cell: f64,
    map: std::collections::HashMap<(i64, i64), Vec<Placement>>,
}

impl Buckets {
    fn new(cell: f64) -> Self {
        Buckets {
            cell: if cell.is_finite() && cell > 1e-6 {
                cell
            } else {
                1.0
            },
            map: std::collections::HashMap::new(),
        }
    }

    fn key(&self, p: Point) -> (i64, i64) {
        (
            (p.x / self.cell).floor() as i64,
            (p.y / self.cell).floor() as i64,
        )
    }

    fn add(&mut self, p: Placement) {
        let k = self.key(p.center);
        self.map.entry(k).or_default().push(p);
    }

    /// The placements whose centres may be within `r` of `p`.
    fn near(&self, p: Point, r: f64) -> impl Iterator<Item = &Placement> + '_ {
        let (kx, ky) = self.key(p);
        let n = (r / self.cell).ceil().clamp(1.0, 50.0) as i64;
        (-n..=n)
            .flat_map(move |dx| (-n..=n).map(move |dy| (kx + dx, ky + dy)))
            .filter_map(|k| self.map.get(&k))
            .flatten()
    }
}

/// Two turned rectangles of half sizes `hw` x `hh` overlap (separating
/// axis test).
fn boxes_overlap(a: Placement, b: Placement, hw: f64, hh: f64) -> bool {
    let axes = |p: Placement| {
        let (s, c) = p.angle.sin_cos();
        [Vec2::new(c, s), Vec2::new(-s, c)]
    };
    let (aa, ba) = (axes(a), axes(b));
    let d = b.center - a.center;
    for axis in aa.iter().chain(ba.iter()) {
        let ra = hw * aa[0].dot(*axis).abs() + hh * aa[1].dot(*axis).abs();
        let rb = hw * ba[0].dot(*axis).abs() + hh * ba[1].dot(*axis).abs();
        if d.dot(*axis).abs() >= ra + rb - 1e-9 {
            return false;
        }
    }
    true
}

/// Leave out every module that comes closer than `gap` to one kept
/// before it (footprints `fw` x `fh`, or round of diameter `fw`).
pub fn remove_overlaps(
    placements: Vec<Placement>,
    fw: f64,
    fh: f64,
    gap: f64,
    round: bool,
) -> Vec<Placement> {
    let (hw, hh) = ((fw + gap) / 2.0, (fh + gap) / 2.0);
    let reach = 2.0 * (hw * hw + hh * hh).sqrt();
    let mut grid = Buckets::new(reach);
    let mut out = Vec::new();
    for p in placements {
        if !finite_pt(p.center) {
            continue;
        }
        let clash = grid.near(p.center, reach).any(|q| {
            if round {
                (q.center - p.center).hypot() < fw + gap - 1e-9
            } else {
                boxes_overlap(*q, p, hw, hh)
            }
        });
        if !clash {
            grid.add(p);
            out.push(p);
        }
    }
    out
}

/// The module's outline at a placement: a rectangle turned by the
/// placement's angle, or a circle.
pub fn module_path(module: &Module, rotate: bool, p: Placement) -> BezPath {
    let (fw, fh) = module.footprint(rotate);
    if module.round {
        return super::circle(p.center, fw);
    }
    let r = Rect::new(-fw / 2.0, -fh / 2.0, fw / 2.0, fh / 2.0).to_path(0.01);
    Affine::translate(p.center.to_vec2()) * Affine::rotate(p.angle) * r
}

/// The corners and side middles of a module's footprint.
fn footprint_points(p: Placement, fw: f64, fh: f64) -> [Point; 8] {
    let (s, c) = p.angle.sin_cos();
    let u = Vec2::new(c, s) * (fw / 2.0);
    let v = Vec2::new(-s, c) * (fh / 2.0);
    let o = p.center;
    [
        o - u - v,
        o + u - v,
        o + u + v,
        o - u + v,
        o - v,
        o + u,
        o + v,
        o - u,
    ]
}

/// The module lies inside the polygons at least `edge` from the outline.
fn fits(polys: &[Vec<Point>], p: Placement, fw: f64, fh: f64, edge: f64, round: bool) -> bool {
    if !finite_pt(p.center) {
        return false;
    }
    if round {
        return inside(polys, p.center) && edge_dist(polys, p.center) >= fw / 2.0 + edge - 1e-6;
    }
    footprint_points(p, fw, fh)
        .iter()
        .all(|q| inside(polys, *q) && edge_dist(polys, *q) >= edge - 1e-6)
}

/// The x ranges where a horizontal band from `ylo` to `yhi` lies wholly
/// inside the polygons (even-odd). Exact for polygons: the outline is
/// straight between its vertices, so the band is tested at its two edges
/// and at every vertex height in between.
pub(crate) fn band_intervals(polys: &[Vec<Point>], ylo: f64, yhi: f64) -> Vec<(f64, f64)> {
    const EPS: f64 = 1e-7;
    let (lo, hi) = (ylo + EPS, yhi - EPS);
    if lo > hi {
        return Vec::new();
    }
    let band: Vec<(Point, Point)> = super::edges(polys)
        .filter(|(a, b)| a.y.max(b.y) >= ylo && a.y.min(b.y) <= yhi)
        .collect();
    let mut ys = vec![lo, hi];
    for (a, _) in &band {
        if a.y > lo && a.y < hi {
            ys.push(a.y);
        }
    }
    ys.sort_by(f64::total_cmp);
    ys.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    let mut acc: Option<Vec<(f64, f64)>> = None;
    for y in ys {
        let mut xs: Vec<f64> = band
            .iter()
            .filter(|(a, b)| (a.y <= y) != (b.y <= y))
            .map(|(a, b)| a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x))
            .filter(|x| x.is_finite())
            .collect();
        xs.sort_by(f64::total_cmp);
        let spans: Vec<(f64, f64)> = xs.chunks_exact(2).map(|c| (c[0], c[1])).collect();
        acc = Some(match acc {
            None => spans,
            Some(prev) => intersect_spans(&prev, &spans),
        });
        if acc.as_ref().is_some_and(|a| a.is_empty()) {
            break;
        }
    }
    acc.unwrap_or_default()
}

/// The overlap of two sorted lists of disjoint ranges.
fn intersect_spans(a: &[(f64, f64)], b: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    while let (Some(x), Some(y)) = (a.get(i), b.get(j)) {
        let lo = x.0.max(y.0);
        let hi = x.1.min(y.1);
        if hi > lo {
            out.push((lo, hi));
        }
        if x.1 < y.1 {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

fn matrix(
    region: &BezPath,
    fw: f64,
    fh: f64,
    row_pitch: f64,
    pitch: f64,
    edge: f64,
    shifted: bool,
) -> Vec<Placement> {
    let inner = if edge > 1e-9 {
        shaping::offset_with(region, -edge, Join::Miter)
    } else {
        region.clone()
    };
    let polys: Vec<Vec<Point>> = region_polys(&inner, FLAT_TOL)
        .into_iter()
        .map(|p| tracedraw_core::geometry::simplify(&p, 0.01))
        .filter(|p| p.len() >= 3)
        .collect();
    let Some(bb) = super::points_bbox(polys.iter().flatten().copied()) else {
        return Vec::new();
    };
    let h = bb.height();
    if h < fh {
        return Vec::new();
    }
    let rows = ((h - fh) / row_pitch).floor() + 1.0;
    if !rows.is_finite() || rows > 100_000.0 {
        return Vec::new();
    }
    let rows = rows as usize;
    let y_first = bb.y0 + fh / 2.0 + (h - fh - (rows as f64 - 1.0) * row_pitch) / 2.0;
    let mut out = Vec::new();
    for r in 0..rows {
        let yc = y_first + r as f64 * row_pitch;
        for (a, b) in band_intervals(&polys, yc - fh / 2.0, yc + fh / 2.0) {
            let len = b - a;
            if len < fw - 1e-9 {
                continue;
            }
            let n = ((len - fw) / pitch + 1e-9).floor() + 1.0;
            if n > MAX_MODULES as f64 {
                return Vec::new();
            }
            let n = n as usize;
            let mid = (a + b) / 2.0;
            let xs: Vec<f64> = (0..n)
                .map(|k| mid + (k as f64 - (n as f64 - 1.0) / 2.0) * pitch)
                .collect();
            let lo = a + fw / 2.0 - 1e-9;
            let hi = b - fw / 2.0 + 1e-9;
            let shifted_xs: Vec<f64> = if shifted && r % 2 == 1 {
                let s: Vec<f64> = xs
                    .iter()
                    .map(|x| x + pitch / 2.0)
                    .filter(|x| *x >= lo && *x <= hi)
                    .collect();
                if s.is_empty() {
                    xs
                } else {
                    s
                }
            } else {
                xs
            };
            for x in shifted_xs {
                out.push(Placement {
                    center: Point::new(x, yc),
                    angle: 0.0,
                });
                if out.len() >= MAX_MODULES {
                    return out;
                }
            }
        }
    }
    out
}

fn along_outline(region: &BezPath, pitch: f64, offset: f64) -> Vec<Placement> {
    let line = if offset > 1e-9 {
        shaping::offset_with(region, -offset, Join::Round)
    } else {
        region.clone()
    };
    let mut out = Vec::new();
    for poly in region_polys(&line, FLAT_TOL) {
        let n = poly.len();
        let segs: Vec<(Point, Point, f64)> = (0..n)
            .filter_map(|i| {
                let a = *poly.get(i)?;
                let b = *poly.get((i + 1) % n)?;
                Some((a, b, (b - a).hypot()))
            })
            .collect();
        let total: f64 = segs.iter().map(|s| s.2).sum();
        if !total.is_finite() || total < 1e-6 {
            continue;
        }
        let count = (total / pitch).floor().max(1.0);
        if count > MAX_MODULES as f64 {
            return Vec::new();
        }
        let count = count as usize;
        let step = total / count as f64;
        let mut run = 0.0;
        let mut k = 0usize;
        for (a, b, len) in &segs {
            while k < count {
                let want = (k as f64 + 0.5) * step;
                if want > run + len {
                    break;
                }
                let t = if *len > 1e-12 {
                    (want - run) / len
                } else {
                    0.0
                };
                let d = *b - *a;
                out.push(Placement {
                    center: *a + d * t,
                    angle: d.y.atan2(d.x),
                });
                k += 1;
            }
            run += len;
        }
    }
    out
}

/// Grid of the Centre layout: cells of `step` with their squared
/// distance (in cells) to the nearest cell outside the shape.
struct Field {
    origin: Point,
    step: f64,
    nx: usize,
    ny: usize,
    d2: Vec<f64>,
}

impl Field {
    fn get(&self, i: isize, j: isize) -> f64 {
        if i < 0 || j < 0 || i as usize >= self.nx || j as usize >= self.ny {
            return 0.0;
        }
        self.d2
            .get(j as usize * self.nx + i as usize)
            .copied()
            .unwrap_or(0.0)
    }

    fn center(&self, i: usize, j: usize) -> Point {
        Point::new(
            self.origin.x + (i as f64 + 0.5) * self.step,
            self.origin.y + (j as f64 + 0.5) * self.step,
        )
    }
}

/// Most cells of the Centre layout's grid.
const MAX_CELLS: f64 = 400_000.0;

fn field(polys: &[Vec<Point>], fine: f64) -> Option<Field> {
    let bb = super::points_bbox(polys.iter().flatten().copied())?;
    let area = bb.width() * bb.height();
    if !area.is_finite() || area <= 0.0 {
        return None;
    }
    let step = fine.max((area / MAX_CELLS).sqrt()).max(1e-3);
    let nx = (bb.width() / step).ceil() as usize + 2;
    let ny = (bb.height() / step).ceil() as usize + 2;
    if (nx * ny) as f64 > MAX_CELLS * 1.5 {
        return None;
    }
    let origin = Point::new(bb.x0 - step, bb.y0 - step);
    const INF: f64 = 1e18;
    let mut d2 = vec![0.0; nx * ny];
    let edges: Vec<(Point, Point)> = super::edges(polys).collect();
    for j in 0..ny {
        let y = origin.y + (j as f64 + 0.5) * step;
        let mut xs: Vec<f64> = edges
            .iter()
            .filter(|(a, b)| (a.y <= y) != (b.y <= y))
            .map(|(a, b)| a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x))
            .filter(|x| x.is_finite())
            .collect();
        xs.sort_by(f64::total_cmp);
        for c in xs.chunks_exact(2) {
            let i0 = ((c[0] - origin.x) / step - 0.5).ceil().max(0.0) as usize;
            let i1 = ((c[1] - origin.x) / step - 0.5).floor();
            if i1 < 0.0 {
                continue;
            }
            let i1 = (i1 as usize).min(nx - 1);
            for i in i0..=i1 {
                if let Some(v) = d2.get_mut(j * nx + i) {
                    *v = INF;
                }
            }
        }
    }
    // Exact squared Euclidean distance transform, columns then rows.
    let mut col = vec![0.0; ny];
    let mut tmp = vec![0.0; ny.max(nx)];
    for i in 0..nx {
        for (j, c) in col.iter_mut().enumerate() {
            *c = d2.get(j * nx + i).copied().unwrap_or(0.0);
        }
        edt_1d(&col, &mut tmp[..ny]);
        for (j, t) in tmp.iter().take(ny).enumerate() {
            if let Some(v) = d2.get_mut(j * nx + i) {
                *v = *t;
            }
        }
    }
    let mut row = vec![0.0; nx];
    for j in 0..ny {
        row.copy_from_slice(&d2[j * nx..(j + 1) * nx]);
        edt_1d(&row, &mut tmp[..nx]);
        d2[j * nx..(j + 1) * nx].copy_from_slice(&tmp[..nx]);
    }
    Some(Field {
        origin,
        step,
        nx,
        ny,
        d2,
    })
}

/// One-dimensional squared distance transform of sampled function `f`
/// (the lower envelope of parabolas).
fn edt_1d(f: &[f64], out: &mut [f64]) {
    let n = f.len().min(out.len());
    if n == 0 {
        return;
    }
    let mut v = vec![0usize; n];
    let mut z = vec![0.0f64; n + 1];
    let mut k = 0usize;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    let key = |q: usize| f[q] + (q * q) as f64;
    for q in 1..n {
        let mut s = (key(q) - key(v[k])) / (2.0 * (q - v[k]) as f64);
        while k > 0 && s <= z[k] {
            k -= 1;
            s = (key(q) - key(v[k])) / (2.0 * (q - v[k]) as f64);
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f64::INFINITY;
    }
    k = 0;
    for (q, o) in out.iter_mut().enumerate().take(n) {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let p = v[k];
        let d = q as f64 - p as f64;
        *o = d * d + f[p];
    }
}

fn centre(
    polys: &[Vec<Point>],
    round: bool,
    fw: f64,
    fh: f64,
    pitch: f64,
    edge: f64,
) -> Vec<Placement> {
    let fine = (fw.min(fh) / 4.0).clamp(0.25, 5.0);
    let Some(f) = field(polys, fine) else {
        return Vec::new();
    };
    let half_across = if round { fw / 2.0 } else { fh / 2.0 };
    // Cell distance d (cells) stands for about (d - 0.5) * step mm.
    let need = (half_across + edge) / f.step + 0.5;
    let need2 = need * need - 1e-9;
    let (nx, ny) = (f.nx, f.ny);
    let mut ridge = vec![false; nx * ny];
    for j in 0..ny {
        for i in 0..nx {
            let (ii, jj) = (i as isize, j as isize);
            let d = f.get(ii, jj);
            if d < need2 || d <= 0.0 {
                continue;
            }
            let is_ridge = [(1, 0), (0, 1), (1, 1), (1, -1)].iter().any(|(dx, dy)| {
                let a = f.get(ii - dx, jj - dy);
                let b = f.get(ii + dx, jj + dy);
                d >= a && d >= b && (d > a || d > b)
            });
            if is_ridge {
                if let Some(r) = ridge.get_mut(j * nx + i) {
                    *r = true;
                }
            }
        }
    }
    let neighbours = |c: usize| -> Vec<usize> {
        let (i, j) = ((c % nx) as isize, (c / nx) as isize);
        let mut out = Vec::with_capacity(8);
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (x, y) = (i + dx, j + dy);
                if (dx, dy) == (0, 0) || x < 0 || y < 0 || x as usize >= nx || y as usize >= ny {
                    continue;
                }
                out.push(y as usize * nx + x as usize);
            }
        }
        out
    };
    let bfs = |start: usize, ridge: &[bool]| -> Vec<usize> {
        let mut seen = std::collections::HashSet::new();
        let mut order = vec![start];
        seen.insert(start);
        let mut k = 0;
        while let Some(&c) = order.get(k) {
            k += 1;
            for n in neighbours(c) {
                if ridge.get(n).copied().unwrap_or(false) && seen.insert(n) {
                    order.push(n);
                }
            }
        }
        order
    };
    let mut done = vec![false; nx * ny];
    let mut placed: Vec<Placement> = Vec::new();
    let cell_pt = |c: usize| f.center(c % nx, c / nx);
    for start in 0..nx * ny {
        if !ridge.get(start).copied().unwrap_or(false) || done.get(start).copied().unwrap_or(true) {
            continue;
        }
        // Walk the component from one of its far ends.
        let first = bfs(start, &ridge);
        let far = *first.last().unwrap_or(&start);
        let order = bfs(far, &ridge);
        for c in &order {
            if let Some(d) = done.get_mut(*c) {
                *d = true;
            }
        }
        let pts: Vec<Point> = order.iter().map(|c| cell_pt(*c)).collect();
        for p in &pts {
            if placed
                .iter()
                .any(|q| (q.center - *p).hypot() < pitch - 1e-9)
            {
                continue;
            }
            let angle = local_direction(&pts, *p, (pitch / 2.0).max(2.0 * f.step));
            let at = Placement { center: *p, angle };
            if !fits(polys, at, fw, fh, edge, round) {
                continue;
            }
            placed.push(at);
            if placed.len() >= MAX_MODULES {
                return placed;
            }
        }
    }
    placed
}

/// Direction of the ridge around `p`: the main axis of the ridge cells
/// within `radius`.
fn local_direction(pts: &[Point], p: Point, radius: f64) -> f64 {
    let near: Vec<Point> = pts
        .iter()
        .filter(|q| (**q - p).hypot() <= radius)
        .copied()
        .collect();
    if near.len() < 2 {
        return 0.0;
    }
    let n = near.len() as f64;
    let mx = near.iter().map(|q| q.x).sum::<f64>() / n;
    let my = near.iter().map(|q| q.y).sum::<f64>() / n;
    let (mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0);
    for q in &near {
        let (dx, dy) = (q.x - mx, q.y - my);
        sxx += dx * dx;
        syy += dy * dy;
        sxy += dx * dy;
    }
    0.5 * (2.0 * sxy).atan2(sxx - syy)
}

/// What a layout costs and draws.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LedReport {
    pub modules: usize,
    pub diodes: u64,
    /// Total power (W).
    pub power: f64,
    pub price: f64,
    pub supply: Option<PowerSupply>,
}

/// A power supply suggestion: `count` units of `watts` each.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PowerSupply {
    pub watts: f64,
    pub count: u32,
}

/// The standard power supply sizes (W).
pub const SUPPLY_SIZES: [f64; 6] = [30.0, 60.0, 100.0, 150.0, 200.0, 300.0];

/// The reserve a supply keeps over the modules' power.
pub const SUPPLY_RESERVE: f64 = 1.2;

/// The supply for `power` watts: power x 1.2 rounded up to a standard
/// size; above 300 W, as many 300 W units as needed.
pub fn power_supply(power: f64) -> Option<PowerSupply> {
    let need = power * SUPPLY_RESERVE;
    if !need.is_finite() || need <= 0.0 {
        return None;
    }
    if let Some(w) = SUPPLY_SIZES.iter().find(|w| **w >= need - 1e-9) {
        return Some(PowerSupply {
            watts: *w,
            count: 1,
        });
    }
    let max = SUPPLY_SIZES[SUPPLY_SIZES.len() - 1];
    let count = (need / max - 1e-9).ceil();
    (count < u32::MAX as f64).then_some(PowerSupply {
        watts: max,
        count: count as u32,
    })
}

/// Count, power, price and supply of `modules` modules of `module`.
pub fn report(module: &Module, modules: usize) -> LedReport {
    let clean = |v: f64| if v.is_finite() { v.max(0.0) } else { 0.0 };
    let power = clean(module.power) * modules as f64;
    LedReport {
        modules,
        diodes: u64::from(module.diodes) * modules as u64,
        power,
        price: clean(module.price) * modules as f64,
        supply: power_supply(power),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::FRAC_PI_2;
    use tracedraw_core::geometry::Rect;

    fn module(w: f64, h: f64) -> Module {
        Module {
            name: "test".into(),
            width: w,
            height: h,
            round: false,
            diodes: 2,
            power: 0.5,
            price: 1.0,
            color: [255, 0, 0],
        }
    }

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> BezPath {
        Rect::new(x0, y0, x1, y1).to_path(0.01)
    }

    fn all_fit(paths: &[BezPath], m: &Module, spec: &LedSpec, out: &[Placement]) {
        let polys = region_polys(&region(paths), FLAT_TOL);
        let (fw, fh) = m.footprint(spec.rotate);
        for p in out {
            assert!(
                fits(&polys, *p, fw, fh, spec.edge - 1e-6, m.round),
                "{p:?} sticks out"
            );
        }
    }

    #[test]
    fn matrix_fills_a_rectangle_row_by_row() {
        // 200 x 100, edge 5: inner 190 x 90. Modules 40 x 10 at 50 / 20.
        let paths = [rect(0.0, 0.0, 200.0, 100.0)];
        let m = module(40.0, 10.0);
        let spec = LedSpec {
            row_pitch: 20.0,
            module_pitch: 50.0,
            edge: 5.0,
            ..Default::default()
        };
        let out = layout(&paths, &m, &spec);
        // Rows: floor((90 - 10) / 20) + 1 = 5; per row floor((190 - 40) / 50) + 1 = 4.
        assert_eq!(out.len(), 20);
        all_fit(&paths, &m, &spec, &out);
        // Centred: the first row's modules are symmetric about x = 100.
        let xs: Vec<f64> = out.iter().take(4).map(|p| p.center.x).collect();
        assert!((xs[0] + xs[3] - 200.0).abs() < 1e-6);
        assert!((out[0].center.y - 10.0).abs() < 1e-6);
    }

    #[test]
    fn shifted_rows_move_by_half_a_pitch() {
        let paths = [rect(0.0, 0.0, 300.0, 100.0)];
        let m = module(40.0, 10.0);
        let spec = LedSpec {
            mode: LedMode::Shifted,
            row_pitch: 20.0,
            module_pitch: 50.0,
            edge: 5.0,
            ..Default::default()
        };
        let out = layout(&paths, &m, &spec);
        let row0: Vec<f64> = out
            .iter()
            .filter(|p| (p.center.y - out[0].center.y).abs() < 1e-6)
            .map(|p| p.center.x)
            .collect();
        let y1 = out[0].center.y + 20.0;
        let row1: Vec<f64> = out
            .iter()
            .filter(|p| (p.center.y - y1).abs() < 1e-6)
            .map(|p| p.center.x)
            .collect();
        assert!(!row1.is_empty());
        assert!((row1[0] - row0[0] - 25.0).abs() < 1e-6);
        all_fit(&paths, &m, &spec, &out);
    }

    #[test]
    fn matrix_skips_holes_and_narrow_parts() {
        // A frame 100 wide with a 60 x 60 hole: strokes 20 wide.
        let mut p = rect(0.0, 0.0, 100.0, 100.0);
        p.extend(rect(20.0, 20.0, 80.0, 80.0));
        let paths = [p];
        let m = module(10.0, 5.0);
        let spec = LedSpec {
            row_pitch: 10.0,
            module_pitch: 15.0,
            edge: 2.0,
            ..Default::default()
        };
        let out = layout(&paths, &m, &spec);
        assert!(!out.is_empty());
        for q in &out {
            let c = q.center;
            assert!(!(c.x > 20.0 && c.x < 80.0 && c.y > 20.0 && c.y < 80.0));
        }
        all_fit(&paths, &m, &spec, &out);
    }

    #[test]
    fn centre_follows_a_narrow_stroke() {
        // A bar 300 x 20: modules 40 x 10 along its centre line y = 10.
        let paths = [rect(0.0, 0.0, 300.0, 20.0)];
        let m = module(40.0, 10.0);
        let spec = LedSpec {
            mode: LedMode::Centre,
            module_pitch: 50.0,
            edge: 2.0,
            ..Default::default()
        };
        let out = layout(&paths, &m, &spec);
        assert!(out.len() >= 4, "{out:?}");
        for p in &out {
            assert!((p.center.y - 10.0).abs() < 1.5, "{p:?}");
            assert!(p.angle.sin().abs() < 0.2, "{p:?} not along the bar");
        }
        for (i, a) in out.iter().enumerate() {
            for b in &out[i + 1..] {
                assert!((a.center - b.center).hypot() >= 50.0 - 1e-6);
            }
        }
        all_fit(&paths, &m, &spec, &out);
        // An upright bar: the modules turn with it.
        let paths = [rect(0.0, 0.0, 20.0, 300.0)];
        let out = layout(&paths, &m, &spec);
        assert!(!out.is_empty());
        for p in &out {
            assert!(p.angle.cos().abs() < 0.2, "{p:?} not along the bar");
        }
    }

    #[test]
    fn outline_mode_spreads_modules_evenly() {
        // A 200 x 200 square, modules 30 mm in: a 140 x 140 line of 560 mm.
        let paths = [rect(0.0, 0.0, 200.0, 200.0)];
        let m = module(20.0, 10.0);
        let spec = LedSpec {
            mode: LedMode::Outline,
            module_pitch: 70.0,
            outline_offset: 30.0,
            edge: 5.0,
            ..Default::default()
        };
        let out = layout(&paths, &m, &spec);
        assert_eq!(out.len(), 8);
        for p in &out {
            let c = p.center;
            let d = c.x.min(c.y).min(200.0 - c.x).min(200.0 - c.y);
            assert!((d - 30.0).abs() < 1.0, "{p:?}");
        }
        all_fit(&paths, &m, &spec, &out);
    }

    #[test]
    fn round_diodes_keep_their_distance() {
        let paths = [rect(0.0, 0.0, 100.0, 50.0)];
        let mut m = module(8.0, 8.0);
        m.round = true;
        let spec = LedSpec {
            row_pitch: 15.0,
            module_pitch: 15.0,
            edge: 3.0,
            ..Default::default()
        };
        let out = layout(&paths, &m, &spec);
        assert!(!out.is_empty());
        all_fit(&paths, &m, &spec, &out);
        let path = module_path(&m, false, out[0]);
        assert!((path.bounding_box().width() - 8.0).abs() < 1e-6);
    }

    #[test]
    fn a_shape_too_small_gets_nothing_and_bad_input_does_not_panic() {
        let m = module(40.0, 10.0);
        let spec = LedSpec::default();
        assert!(layout(&[rect(0.0, 0.0, 20.0, 20.0)], &m, &spec).is_empty());
        assert!(layout(&[], &m, &spec).is_empty());
        let mut open = BezPath::new();
        open.move_to((0.0, 0.0));
        open.line_to((100.0, 0.0));
        assert!(layout(&[open], &m, &spec).is_empty());
        let mut nan = BezPath::new();
        nan.move_to((f64::NAN, 0.0));
        nan.line_to((100.0, f64::INFINITY));
        nan.line_to((0.0, 100.0));
        nan.close_path();
        for mode in [
            LedMode::Matrix,
            LedMode::Shifted,
            LedMode::Centre,
            LedMode::Outline,
        ] {
            let spec = LedSpec {
                mode,
                row_pitch: f64::NAN,
                module_pitch: -5.0,
                edge: f64::INFINITY,
                outline_offset: f64::NAN,
                rotate: true,
                ..Default::default()
            };
            assert!(layout(&[nan.clone()], &m, &spec).is_empty());
            let _ = layout(&[rect(0.0, 0.0, 100.0, 100.0)], &m, &spec);
        }
        let mut bad = module(f64::NAN, -3.0);
        bad.power = f64::NAN;
        let _ = layout(&[rect(0.0, 0.0, 100.0, 100.0)], &bad, &LedSpec::default());
        assert_eq!(report(&bad, 10).power, 0.0);
    }

    #[test]
    fn report_suggests_a_standard_supply() {
        let m = module(40.0, 10.0);
        let r = report(&m, 100);
        // 100 x 0.5 W = 50 W, x 1.2 = 60 W.
        assert!((r.power - 50.0).abs() < 1e-9);
        assert_eq!(r.diodes, 200);
        assert!((r.price - 100.0).abs() < 1e-9);
        assert_eq!(
            r.supply,
            Some(PowerSupply {
                watts: 60.0,
                count: 1
            })
        );
        assert_eq!(power_supply(10.0).map(|s| s.watts), Some(30.0));
        assert_eq!(power_supply(130.0).map(|s| s.watts), Some(200.0));
        assert_eq!(
            power_supply(400.0),
            Some(PowerSupply {
                watts: 300.0,
                count: 2
            })
        );
        assert_eq!(power_supply(0.0), None);
        assert_eq!(power_supply(f64::NAN), None);
    }

    #[test]
    fn band_intervals_are_exact_for_a_notch() {
        // A square with a V notch from the left reaching x = 50 at y = 50.
        let poly = vec![
            Point::new(0.0, 0.0),
            Point::new(100.0, 0.0),
            Point::new(100.0, 100.0),
            Point::new(0.0, 100.0),
            Point::new(0.0, 60.0),
            Point::new(50.0, 50.0),
            Point::new(0.0, 40.0),
        ];
        let spans = band_intervals(&[poly], 45.0, 55.0);
        assert_eq!(spans.len(), 1);
        assert!((spans[0].0 - 50.0).abs() < 1e-6 && (spans[0].1 - 100.0).abs() < 1e-6);
    }

    #[test]
    fn the_catalogue_is_usable() {
        let c = catalogue();
        assert!(c.len() >= 4);
        assert!(c
            .iter()
            .all(|m| m.width > 0.0 && m.power > 0.0 && !m.name.is_empty()));
    }

    fn line(pts: &[(f64, f64)]) -> BezPath {
        let pts: Vec<Point> = pts.iter().map(|p| Point::new(p.0, p.1)).collect();
        tracedraw_core::geometry::polyline_path(&pts, false)
    }

    #[test]
    fn on_curve_spreads_modules_from_end_to_end() {
        let m = module(10.0, 5.0);
        let spec = LedSpec {
            mode: LedMode::OnCurve,
            module_pitch: 25.0,
            ..Default::default()
        };
        let out = layout(&[line(&[(0.0, 0.0), (100.0, 0.0)])], &m, &spec);
        assert_eq!(out.len(), 5);
        assert!((out[0].center.x).abs() < 1e-9 && (out[4].center.x - 100.0).abs() < 1e-6);
        assert!(out
            .iter()
            .all(|p| p.angle.abs() < 1e-9 && p.center.y.abs() < 1e-9));
        // A line shorter than the pitch gets one module in its middle.
        let short = layout(&[line(&[(0.0, 0.0), (10.0, 0.0)])], &m, &spec);
        assert_eq!(short.len(), 1);
        assert!((short[0].center.x - 5.0).abs() < 1e-9);
        // A closed outline: modules at the middles of equal stretches.
        let sq = layout(&[rect(0.0, 0.0, 100.0, 100.0)], &m, &spec);
        assert_eq!(sq.len(), 16);
    }

    #[test]
    fn around_curve_makes_two_rows() {
        let m = module(10.0, 5.0);
        let spec = LedSpec {
            mode: LedMode::AroundCurve,
            module_pitch: 50.0,
            side_distance: 8.0,
            ..Default::default()
        };
        let out = layout(&[line(&[(0.0, 0.0), (100.0, 0.0)])], &m, &spec);
        assert_eq!(out.len(), 6);
        assert_eq!(
            out.iter()
                .filter(|p| (p.center.y - 8.0).abs() < 1e-9)
                .count(),
            3
        );
        assert_eq!(
            out.iter()
                .filter(|p| (p.center.y + 8.0).abs() < 1e-9)
                .count(),
            3
        );
    }

    #[test]
    fn island_fills_rings_inside() {
        let m = module(10.0, 5.0);
        let spec = LedSpec {
            mode: LedMode::Island,
            module_pitch: 20.0,
            row_pitch: 20.0,
            edge: 5.0,
            ..Default::default()
        };
        let paths = [rect(0.0, 0.0, 100.0, 100.0)];
        let out = layout(&paths, &m, &spec);
        assert!(out.len() > 16, "{}", out.len());
        all_fit(&paths, &m, &spec, &out);
        // More than one ring: centres at different distances from the edge.
        let d = |p: &Placement| {
            p.center
                .x
                .min(p.center.y)
                .min(100.0 - p.center.x)
                .min(100.0 - p.center.y)
        };
        assert!(out.iter().any(|p| d(p) < 9.0) && out.iter().any(|p| d(p) > 25.0));
    }

    #[test]
    fn random_is_repeatable_and_keeps_its_spacing() {
        let m = module(10.0, 5.0);
        let spec = LedSpec {
            mode: LedMode::Random,
            module_pitch: 15.0,
            edge: 2.0,
            seed: 7,
            ..Default::default()
        };
        let paths = [rect(0.0, 0.0, 100.0, 60.0)];
        let a = layout(&paths, &m, &spec);
        let b = layout(&paths, &m, &spec);
        assert_eq!(a, b);
        assert!(a.len() > 5);
        all_fit(&paths, &m, &spec, &a);
        for (i, p) in a.iter().enumerate() {
            for q in a.iter().skip(i + 1) {
                assert!((p.center - q.center).hypot() >= 15.0 - 1e-9);
            }
        }
        let c = layout(&paths, &m, &LedSpec { seed: 8, ..spec });
        assert_ne!(a, c);
    }

    #[test]
    fn fix_overlaps_drops_touching_modules() {
        let at = |x: f64, angle: f64| Placement {
            center: Point::new(x, 0.0),
            angle,
        };
        let ps = vec![at(0.0, 0.0), at(8.0, 0.0), at(20.0, 0.0), at(31.0, 0.0)];
        let kept = remove_overlaps(ps.clone(), 10.0, 5.0, 0.0, false);
        assert_eq!(kept.len(), 3);
        // With a gap of 2 the module at 31 is too close to the one at 20.
        let kept = remove_overlaps(ps, 10.0, 5.0, 2.0, false);
        assert_eq!(kept.len(), 2);
        // Turned a quarter, 10 x 5 modules 8 apart no longer touch.
        let turned = vec![at(0.0, FRAC_PI_2), at(8.0, FRAC_PI_2)];
        assert_eq!(remove_overlaps(turned, 10.0, 5.0, 0.0, false).len(), 2);
        // Round: by the centre distance; non-finite ones are dropped.
        let round = vec![at(0.0, 0.0), at(7.0, 0.0), at(f64::NAN, 0.0)];
        assert_eq!(remove_overlaps(round, 8.0, 8.0, 0.0, true).len(), 1);
    }

    #[test]
    fn curve_modes_survive_bad_input() {
        let m = module(10.0, 5.0);
        for mode in [
            LedMode::OnCurve,
            LedMode::AroundCurve,
            LedMode::Island,
            LedMode::Random,
        ] {
            let spec = LedSpec {
                mode,
                module_pitch: f64::NAN,
                row_pitch: f64::INFINITY,
                side_distance: f64::NAN,
                fix_overlaps: true,
                overlap_gap: f64::NAN,
                ..Default::default()
            };
            let mut bad = BezPath::new();
            bad.move_to((f64::NAN, 0.0));
            bad.line_to((10.0, f64::INFINITY));
            assert!(layout(&[bad], &m, &spec).is_empty());
            assert!(layout(&[], &m, &spec).is_empty());
            let _ = layout(&[rect(0.0, 0.0, 50.0, 50.0)], &m, &spec);
        }
    }
}
