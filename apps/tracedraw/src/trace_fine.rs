//! The fine part of the outline trace: where the edge between two colours
//! runs inside the pixels it crosses, and the lines, curves and corners
//! fitted to it. See docs/behavior/bitmap-tracing.md, "Fine edges".
//!
//! The pixels along an edge hold a blend of the two colours beside it,
//! and the share of each says where in the pixel the edge runs. The
//! labels of the picture are made again on a grid `k` times finer from
//! the picture's colours laid between its pixels, and the borders are
//! walked and fitted on that grid: the steps of the pixel grid are gone
//! before any curve is fitted, so no smoothing has to melt them away.

use crate::trace::{cancelled, fit_cubics, Cancel, Settings};
use tracedraw_core::geometry::{BezPath, Point, Vec2};

/// The most pixels the finer grid may have.
const FINE_PIXELS: usize = 6_000_000;

/// How many times finer than the picture the grid of its edges is: 4 for
/// a small picture, less as it grows, 1 (the pixels themselves) when it
/// is large enough for its own pixels to say where the edges run.
pub(crate) fn scale_for(w: usize, h: usize) -> usize {
    let k = ((FINE_PIXELS as f64) / ((w * h).max(1) as f64))
        .sqrt()
        .floor() as usize;
    k.clamp(1, 4)
}

fn dist2(a: [f32; 3], b: [u8; 3]) -> f32 {
    (0..3).map(|c| (a[c] - b[c] as f32).powi(2)).sum()
}

/// The most steps from a pixel up or down to the flat colour or the
/// line's core its edge leads to.
const CLIMB: usize = 6;
/// How far apart (RGB distance) the two ends of a pixel's edge must be.
const EDGE_APART: f32 = 48.0;
/// The least a step of the walk must gain in brightness (thousandths of
/// a level): less is the slope of a gradient or noise, not an edge.
const RISE: i32 = 5000;

/// For each pixel on an edge, the two pixels the edge runs between and
/// how far from the first to the second the pixel's colour lies.
///
/// Brightness rises steadily across the blurred edge of two colours, so
/// walking from a pixel to its brightest neighbour again and again ends
/// in the flat colour on the bright side (or on the core of a bright
/// line thinner than the blur), and to the darkest neighbour in the one
/// on the dark side. A pixel both walks of which end within [`CLIMB`]
/// steps, at colours at least [`EDGE_APART`] apart with its own between
/// them, is a blend of those two and nothing else: no third colour that
/// happens to look like the blend can claim it, and a faint thin line is
/// measured against its own core, not against the full colour it never
/// reaches.
pub(crate) struct Edges {
    /// (dark end, bright end, share of the bright end); `u32::MAX` ends
    /// for a pixel on no edge.
    of: Vec<(u32, u32, f32)>,
}

impl Edges {
    pub(crate) fn none(len: usize) -> Edges {
        Edges {
            of: vec![(u32::MAX, u32::MAX, 0.0); len],
        }
    }

    pub(crate) fn at(&self, i: usize) -> Option<(usize, usize, f32)> {
        let (lo, hi, t) = *self.of.get(i)?;
        (lo != u32::MAX).then_some((lo as usize, hi as usize, t))
    }

    pub(crate) fn find(
        colors: &[[u8; 3]],
        alpha: &[u8],
        w: usize,
        h: usize,
        cancel: Option<&Cancel>,
    ) -> Option<Edges> {
        let n = w * h;
        let mut out = Edges::none(n);
        if colors.len() != n || alpha.len() != n || w < 3 || h < 3 {
            return Some(out);
        }
        let luma: Vec<i32> = colors
            .iter()
            .map(|c| c[0] as i32 * 299 + c[1] as i32 * 587 + c[2] as i32 * 114)
            .collect();
        // Each pixel's brightest and darkest neighbour (itself when none
        // is brighter or darker).
        let mut up: Vec<u32> = (0..n as u32).collect();
        let mut down: Vec<u32> = (0..n as u32).collect();
        for y in 0..h {
            if y % 64 == 0 && cancelled(cancel) {
                return None;
            }
            for x in 0..w {
                let i = y * w + x;
                if alpha[i] < 128 {
                    continue;
                }
                let (mut hi, mut lo) = (luma[i], luma[i]);
                for v in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                    for u in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                        let j = v * w + u;
                        if alpha[j] < 128 {
                            continue;
                        }
                        if luma[j] > hi && luma[j] >= luma[i] + RISE {
                            hi = luma[j];
                            up[i] = j as u32;
                        }
                        if luma[j] < lo && luma[j] <= luma[i] - RISE {
                            lo = luma[j];
                            down[i] = j as u32;
                        }
                    }
                }
            }
        }
        let end = |next: &[u32], mut i: usize| -> Option<usize> {
            for _ in 0..CLIMB {
                let j = next[i] as usize;
                if j == i {
                    return Some(i);
                }
                i = j;
            }
            (next[i] as usize == i).then_some(i)
        };
        for i in 0..n {
            if i % 65536 == 0 && cancelled(cancel) {
                return None;
            }
            if alpha[i] < 128 {
                continue;
            }
            let (Some(hi), Some(lo)) = (end(&up, i), end(&down, i)) else {
                continue;
            };
            if hi == lo {
                continue;
            }
            let f = |c: [u8; 3]| [c[0] as f32, c[1] as f32, c[2] as f32];
            let (a, b, c) = (f(colors[lo]), f(colors[hi]), f(colors[i]));
            let ab: [f32; 3] = std::array::from_fn(|k| b[k] - a[k]);
            let ac: [f32; 3] = std::array::from_fn(|k| c[k] - a[k]);
            let len2: f32 = ab.iter().map(|v| v * v).sum();
            if len2 < EDGE_APART * EDGE_APART {
                continue;
            }
            let t = (ab.iter().zip(&ac).map(|(p, q)| p * q).sum::<f32>() / len2).clamp(0.0, 1.0);
            let off2: f32 = (0..3).map(|k| (ac[k] - t * ab[k]).powi(2)).sum();
            let slack = (0.12 * len2.sqrt()).max(16.0);
            if off2 > slack * slack {
                continue;
            }
            out.of[i] = (lo as u32, hi as u32, t);
        }
        Some(out)
    }
}

/// The share of the borders in the picture drawn without anti-aliasing
/// for the picture to count as drawn so.
const HARD_SHARE: f64 = 0.7;

/// Whether the picture's edges are drawn hard, with no pixels blending
/// the two colours (pixel art, a one-bit scan, a drawing saved without
/// anti-aliasing): of the places two neighbouring pixels have different
/// labels, at least [`HARD_SHARE`] have a pixel of full colour on both
/// sides. Such edges say nothing of where they run inside a pixel: they
/// are stair steps, and only smoothing makes slopes of them.
pub(crate) fn hard_edges(labels: &[usize], edges: &Edges, w: usize, h: usize) -> bool {
    if labels.len() != w * h {
        return false;
    }
    let full = |i: usize| {
        edges
            .at(i)
            .is_none_or(|(_, _, t)| !(0.1..=0.9).contains(&t))
    };
    let (mut borders, mut hard) = (0usize, 0usize);
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            for j in [(x + 1 < w).then(|| i + 1), (y + 1 < h).then(|| i + w)]
                .into_iter()
                .flatten()
            {
                if labels[i] != labels[j] && labels[i] != usize::MAX && labels[j] != usize::MAX {
                    borders += 1;
                    hard += (full(i) && full(j)) as usize;
                }
            }
        }
    }
    borders > 0 && hard as f64 >= HARD_SHARE * borders as f64
}

/// Give each pixel on an edge ([`Edges`]) the label of the end of the
/// edge its colour is nearer: `label_of` names the label of the pixel at
/// an end, given that pixel and the one at the other end.
pub(crate) fn settle_on_ends(
    labels: &mut [usize],
    edges: &Edges,
    label_of: impl Fn(usize, usize) -> usize,
) {
    for (i, l) in labels.iter_mut().enumerate() {
        if *l == usize::MAX {
            continue;
        }
        if let Some((lo, hi, t)) = edges.at(i) {
            *l = if t >= 0.5 {
                label_of(hi, lo)
            } else {
                label_of(lo, hi)
            };
        }
    }
}

/// The colour each pixel stands for when the labels are made finer. On
/// an edge whose two ends have different labels, one of them the
/// pixel's: the blend of the two labels' colours by the pixel's share
/// ([`Edges`]), kept on its own label's side, so the fine edge runs where
/// the blend is half and half and a faint thin line comes out as wide as
/// it is. Else its own colour where its label is the one its colour is
/// nearest among the labels around it, and its label's colour, just on
/// its side of the edge, where the cleaning gave it another.
fn stand_ins(
    labels: &[usize],
    colors: &[[u8; 3]],
    edges: &Edges,
    palette: &[[u8; 3]],
    w: usize,
    h: usize,
) -> Vec<[f32; 3]> {
    let own = |i: usize| -> [f32; 3] { std::array::from_fn(|c| colors[i][c] as f32) };
    let mut out: Vec<[f32; 3]> = (0..w * h).map(own).collect();
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let l = labels[i];
            let around = (y.saturating_sub(1)..=(y + 1).min(h - 1))
                .flat_map(|v| (x.saturating_sub(1)..=(x + 1).min(w - 1)).map(move |u| v * w + u));
            if l == usize::MAX {
                // No colour of its own: the mean of the opaque pixels
                // round it, so it does not tint the edge beside it.
                let (mut sum, mut n) = ([0f32; 3], 0f32);
                for j in around {
                    if labels[j] != usize::MAX {
                        for c in 0..3 {
                            sum[c] += colors[j][c] as f32;
                        }
                        n += 1.0;
                    }
                }
                if n > 0.0 {
                    out[i] = sum.map(|v| v / n);
                }
                continue;
            }
            let Some(&mine) = palette.get(l) else {
                continue;
            };
            let full: [f32; 3] = std::array::from_fn(|c| mine[c] as f32);
            if let Some((lo, hi, t)) = edges.at(i) {
                let (la, lb) = (labels[lo], labels[hi]);
                if la != lb && (l == la || l == lb) {
                    if let (Some(pa), Some(pb)) = (palette.get(la), palette.get(lb)) {
                        let t = if l == lb { t.max(0.55) } else { t.min(0.45) };
                        out[i] =
                            std::array::from_fn(|c| pa[c] as f32 * (1.0 - t) + pb[c] as f32 * t);
                        continue;
                    }
                }
            }
            let c = own(i);
            let mut best = (dist2(c, mine), l);
            for j in around {
                let m = labels[j];
                if m == l || m == usize::MAX {
                    continue;
                }
                if let Some(p) = palette.get(m) {
                    let d = dist2(c, *p);
                    if d < best.0 {
                        best = (d, m);
                    }
                }
            }
            if best.1 == l {
                continue;
            }
            let other = palette[best.1];
            out[i] = std::array::from_fn(|c| 0.6 * full[c] + 0.4 * other[c] as f32);
        }
    }
    out
}

/// The weights of the four samples round a place `t` of the way from the
/// second to the third (Catmull-Rom: the curve passes through the samples,
/// so the middle of a pixel keeps the pixel's own value).
fn weights(t: f32) -> [f32; 4] {
    let (t2, t3) = (t * t, t * t * t);
    [
        0.5 * (-t3 + 2.0 * t2 - t),
        0.5 * (3.0 * t3 - 5.0 * t2 + 2.0),
        0.5 * (-3.0 * t3 + 4.0 * t2 + t),
        0.5 * (t3 - t2),
    ]
}

/// The labels of a picture (`w` x `h`) on a grid `k` times finer: each
/// fine pixel takes, of the labels of the four pixels round it, the one
/// whose colour is nearest the picture's colour there (the pixels' colours
/// laid smoothly between them), so the edge between two colours runs
/// through the pixels blending them where the blend is half and half. A
/// pixel's middle keeps the pixel's label. `alpha` (the picture's, one
/// value a pixel) decides the same way where the picture ends. `None`
/// once `cancel` is set.
#[allow(clippy::too_many_arguments)]
pub(crate) fn finer_labels(
    labels: &[usize],
    colors: &[[u8; 3]],
    alpha: &[u8],
    edges: &Edges,
    palette: &[[u8; 3]],
    w: usize,
    h: usize,
    k: usize,
    cancel: Option<&Cancel>,
) -> Option<Vec<usize>> {
    if k <= 1 || labels.len() != w * h || colors.len() != w * h || alpha.len() != w * h {
        return Some(labels.to_vec());
    }
    let stand = stand_ins(labels, colors, edges, palette, w, h);
    let (fw, fh) = (w * k, h * k);
    let mut out = vec![usize::MAX; fw * fh];
    // Where each fine column and row lies between the pixels: the pixel
    // before it and how far on towards the next.
    let place = |q: usize, n: usize| -> (usize, usize, f32) {
        let u = (q as f32 + 0.5) / k as f32 - 0.5;
        let f = u.floor();
        let a = (f.max(0.0) as usize).min(n - 1);
        let b = ((f + 1.0).max(0.0) as usize).min(n - 1);
        (a, b, u - f)
    };
    let cols: Vec<(usize, usize, f32)> = (0..fw).map(|q| place(q, w)).collect();
    let clampi = |v: isize, n: usize| v.clamp(0, n as isize - 1) as usize;
    for fy in 0..fh {
        if fy % 64 == 0 && cancelled(cancel) {
            return None;
        }
        let (y0, y1, ty) = place(fy, h);
        let wy = weights(ty);
        let rows: [usize; 4] = std::array::from_fn(|n| clampi(y0 as isize + n as isize - 1, h));
        // At the picture's edge the place before the first row is the
        // first row itself: `ty` then counts from it.
        let rows = if (fy as f32 + 0.5) / (k as f32) < 0.5 {
            [0, 0, 0, 1.min(h - 1)]
        } else {
            rows
        };
        for (fx, &(x0, x1, tx)) in cols.iter().enumerate().take(fw) {
            let cell = [
                labels[y0 * w + x0],
                labels[y0 * w + x1],
                labels[y1 * w + x0],
                labels[y1 * w + x1],
            ];
            let o = fy * fw + fx;
            if cell[0] == cell[1] && cell[0] == cell[2] && cell[0] == cell[3] {
                out[o] = cell[0];
                continue;
            }
            if cell.contains(&usize::MAX) {
                let a = |x: usize, y: usize| alpha[y * w + x] as f32;
                let top = a(x0, y0) * (1.0 - tx) + a(x1, y0) * tx;
                let bottom = a(x0, y1) * (1.0 - tx) + a(x1, y1) * tx;
                if top * (1.0 - ty) + bottom * ty < 128.0 {
                    continue;
                }
            }
            let wx = weights(tx);
            let xs: [usize; 4] = if (fx as f32 + 0.5) / (k as f32) < 0.5 {
                [0, 0, 0, 1.min(w - 1)]
            } else {
                std::array::from_fn(|n| clampi(x0 as isize + n as isize - 1, w))
            };
            let mut c = [0f32; 3];
            for (r, wr) in rows.iter().zip(wy) {
                for (x, wc) in xs.iter().zip(wx) {
                    let s = stand[r * w + x];
                    for ch in 0..3 {
                        c[ch] += s[ch] * wr * wc;
                    }
                }
            }
            let mut best = (f32::MAX, usize::MAX);
            for l in cell {
                if let Some(p) = palette.get(l) {
                    let d = dist2(c, *p);
                    if d < best.0 || (d == best.0 && l < best.1) {
                        best = (d, l);
                    }
                }
            }
            out[o] = best.1;
        }
    }
    Some(out)
}

// ----- fitting -------------------------------------------------------------------

/// How a border of the finer grid is fitted. Lengths are in pixels of
/// the picture.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Fit {
    /// Fine pixels per pixel of the picture.
    pub k: f64,
    /// How much the border is smoothed (the deviation of the blur along
    /// it).
    pub sigma: f64,
    /// The turn that makes a corner, radians.
    pub corner: f64,
    /// How far the fitted curve may stray from the border.
    pub tolerance: f64,
    /// How much the border is smoothed before its corners are looked
    /// for.
    pub light: f64,
}

/// The least smoothing of a picture drawn with hard edges
/// ([`hard_edges`]), for the stretches between corners and for finding
/// the corners: its borders are stair steps a pixel high.
const HARD_SIGMA: f64 = 1.4;
const HARD_LIGHT: f64 = 0.9;

/// The length of border a corner's turn must fit in.
const CORNER_SPAN: f64 = 2.0;
/// The length of the chord a border's direction is taken along.
const CHORD: f64 = 1.0;
/// A turn of more than this within [`CORNER_SPAN`] is the round end of a
/// thin stroke, not a corner.
const CORNER_MOST: f64 = 2.6;
/// How much sharper than the border beside it a corner must turn.
const CORNER_STANDS_OUT: f64 = 1.6;
/// The length of border each side of a corner its direction is taken
/// from, and how far from the rounded corner it starts.
const SIDE_SPAN: f64 = 1.6;
const SIDE_GAP: f64 = 0.9;
/// How far the place two sides meet may lie from the rounded corner.
const CORNER_REACH: f64 = 1.6;

impl Fit {
    pub(crate) fn of(s: &Settings, k: usize, hard: bool) -> Fit {
        let detail = s.detail.clamp(0.0, 100.0);
        let sigma = 0.3 + s.smoothing.clamp(0.0, 100.0) / 100.0 * 1.5;
        Fit {
            k: k as f64,
            sigma: if hard { sigma.max(HARD_SIGMA) } else { sigma },
            light: if hard { HARD_LIGHT } else { 0.45 },
            corner: (35.0 + s.corner_smoothness.clamp(0.0, 100.0) * 1.3).to_radians(),
            tolerance: 0.2 + ((100.0 - detail) / 100.0).powf(1.5) * 0.6,
        }
    }
}

fn unit(v: Vec2) -> Vec2 {
    let l = v.hypot();
    if l > 1e-12 && l.is_finite() {
        v / l
    } else {
        Vec2::ZERO
    }
}

/// `pts` blurred along the border by `passes` rounds of quarter, half,
/// quarter; an open border keeps its two ends.
fn smooth(pts: &[Point], passes: usize, closed: bool) -> Vec<Point> {
    let n = pts.len();
    let mut cur = pts.to_vec();
    if n < 3 {
        return cur;
    }
    let mut prev = cur.clone();
    for _ in 0..passes {
        std::mem::swap(&mut cur, &mut prev);
        for i in 0..n {
            let (a, c) = if closed {
                (prev[(i + n - 1) % n], prev[(i + 1) % n])
            } else if i == 0 || i == n - 1 {
                cur[i] = prev[i];
                continue;
            } else {
                (prev[i - 1], prev[i + 1])
            };
            cur[i] = Point::new(
                (a.x + 2.0 * prev[i].x + c.x) / 4.0,
                (a.y + 2.0 * prev[i].y + c.y) / 4.0,
            );
        }
    }
    cur
}

fn passes_for(sigma_fine: f64) -> usize {
    (2.0 * sigma_fine * sigma_fine).round().max(1.0) as usize
}

/// A corner found on a border: the point of the border it sits at and
/// where its two sides meet.
struct Corner {
    at: usize,
    point: Point,
}

/// The corners of the border `p` (lightly smoothed points, about a fine
/// pixel apart): the places where it turns by more than `f.corner`
/// within [`CORNER_SPAN`], much more sharply than just before and after
/// (an arc turns as much everywhere and has none), each put where the
/// straight runs of its two sides meet, so the corner the picture's blur
/// rounded off is sharp again.
fn corners_of(p: &[Point], closed: bool, f: &Fit) -> Vec<Corner> {
    let n = p.len();
    let mut out = Vec::new();
    if n < 8 {
        return out;
    }
    let at = |i: isize| -> Point {
        if closed {
            p[i.rem_euclid(n as isize) as usize]
        } else {
            p[i.clamp(0, n as isize - 1) as usize]
        }
    };
    // The turn at each point, and the turn within half a span of it:
    // points lie about a fine pixel apart (half that after the middles
    // of the steps), so a span is counted in points by their mean gap.
    let mut total = 0.0;
    for i in 0..n - 1 {
        total += (p[i + 1] - p[i]).hypot();
    }
    if closed {
        total += (p[0] - p[n - 1]).hypot();
    }
    let gap = (total / n as f64).max(1e-6);
    let per = |len: f64| ((len * f.k / gap).round() as isize).max(1);
    let half = per(CORNER_SPAN / 2.0);
    // The border's direction at a point: along a chord of about a pixel
    // centred on it, so what is left of the grid's steps does not count
    // as turning.
    let chord = per(CHORD / 2.0);
    let dir = |i: isize| -> Vec2 {
        if !closed {
            let lo = chord.min((n as isize - 1) / 2);
            let i = i.clamp(lo, n as isize - 1 - lo);
            return unit(at(i + lo) - at(i - lo));
        }
        unit(at(i + chord) - at(i - chord))
    };
    let dirs: Vec<Vec2> = (0..n as isize).map(dir).collect();
    let d_at = |i: isize| -> Vec2 {
        if closed {
            dirs[i.rem_euclid(n as isize) as usize]
        } else {
            dirs[i.clamp(0, n as isize - 1) as usize]
        }
    };
    // The turn from one point to another, signed.
    let sum = |a: isize, b: isize| -> f64 {
        let (u, v) = (d_at(a), d_at(b));
        u.cross(v).atan2(u.dot(v))
    };
    let w: Vec<f64> = (0..n as isize).map(|i| sum(i - half, i + half)).collect();
    let turn: Vec<f64> = (0..n as isize).map(|i| sum(i - 1, i + 1).abs()).collect();
    let is_corner = |i: usize| -> bool {
        let t = w[i];
        if t.abs() < f.corner || t.abs() > CORNER_MOST {
            return false;
        }
        if !closed && (i < half as usize || i + half as usize >= n) {
            return false;
        }
        let i = i as isize;
        let before = sum(i - 3 * half, i - half);
        let after = sum(i + half, i + 3 * half);
        // Turning the same way beside it as much: an arc.
        let same = |s: f64| if s * t > 0.0 { s.abs() } else { 0.0 };
        t.abs() >= CORNER_STANDS_OUT * same(before).max(same(after))
    };
    let flags: Vec<bool> = (0..n).map(is_corner).collect();
    // Each run of flagged points is one corner, at the middle of its turn.
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < n {
        if !flags[i] {
            i += 1;
            continue;
        }
        let start = i;
        while i < n && flags[i] {
            i += 1;
        }
        runs.push((start, i - 1));
    }
    if closed && runs.len() > 1 {
        let (first, last) = (runs[0], runs[runs.len() - 1]);
        if first.0 == 0 && last.1 == n - 1 {
            runs[0] = (last.0, first.1 + n);
            runs.pop();
        }
    }
    let gap_pts = per(SIDE_GAP.max(3.0 * f.light));
    let span_pts = per(SIDE_SPAN);
    let reach = CORNER_REACH * f.k;
    let mut found: Vec<(usize, f64)> = Vec::new();
    for (a, b) in runs {
        // Where the turning of the run is centred.
        let (mut m, mut mw) = (0.0, 0.0);
        for j in a as isize - half..=b as isize + half {
            let t = turn[j.rem_euclid(n as isize) as usize];
            if closed || (0..n as isize).contains(&j) {
                m += t * j as f64;
                mw += t;
            }
        }
        let centre = if mw > 0.0 {
            (m / mw).round() as isize
        } else {
            (a + b) as isize / 2
        };
        let idx = if closed {
            centre.rem_euclid(n as isize) as usize
        } else {
            centre.clamp(1, n as isize - 2) as usize
        };
        found.push((idx, w[idx.min(n - 1)].abs()));
    }
    found.sort_by_key(|c| c.0);
    found.dedup_by_key(|c| c.0);
    let count = found.len();
    for (k, &(idx, _)) in found.iter().enumerate() {
        // Each side's straight run: from past the rounding to a span on,
        // no further than half way to the next corner.
        let room = |other: usize, forwards: bool| -> isize {
            if count < 2 && closed {
                return n as isize / 2;
            }
            let d = if forwards {
                (other + n - idx) % n
            } else {
                (idx + n - other) % n
            };
            (if d == 0 { n } else { d }) as isize / 2
        };
        let (prev_room, next_room) = if closed {
            (
                room(found[(k + count - 1) % count].0, false),
                room(found[(k + 1) % count].0, true),
            )
        } else {
            (
                if k == 0 {
                    idx as isize
                } else {
                    (idx - found[k - 1].0) as isize / 2
                },
                if k + 1 == count {
                    (n - 1 - idx) as isize
                } else {
                    (found[k + 1].0 - idx) as isize / 2
                },
            )
        };
        let i = idx as isize;
        let side = |room: isize, sign: isize| -> Option<(Point, Vec2)> {
            let far = (gap_pts + span_pts).min(room);
            let near = gap_pts.min(far - 2);
            if near < 1 || far - near < 2 {
                return None;
            }
            let (a, b) = (at(i + sign * far), at(i + sign * near));
            let d = unit(b - a);
            (d != Vec2::ZERO).then_some((b, d))
        };
        let mut point = p[idx];
        if let (Some((pa, da)), Some((pb, db))) = (side(prev_room, -1), side(next_room, 1)) {
            // The two lines pa + s da and pb + t db.
            let det = da.cross(db);
            if det.abs() > 0.2 {
                let s = (pb - pa).cross(db) / det;
                let x = pa + da * s;
                let t = (pa - pb).cross(da) / -det;
                // No further out than the sides are long: the corners of
                // small print have sides too short to aim by.
                let room = prev_room.min(next_room) as f64 * gap;
                let reach = reach.min(0.5 * room);
                if x.x.is_finite()
                    && x.y.is_finite()
                    && (x - p[idx]).hypot() <= reach
                    && s > 0.0
                    && t > 0.0
                {
                    point = x;
                }
            }
        }
        out.push(Corner { at: idx, point });
    }
    out
}

/// The furthest a straight line may stray from the border it stands for.
const LINE_MOST: f64 = 0.2;

/// Append the stretch of border `pts` (first and last points fixed) to
/// `out`: a straight line when no point strays from one by more than the
/// tolerance, else cubics.
fn fit_piece(pts: &[Point], tolerance: f64, k: f64, out: &mut BezPath) {
    let n = pts.len();
    if n < 2 {
        return;
    }
    let (a, b) = (pts[0], pts[n - 1]);
    let chord = b - a;
    let len = chord.hypot();
    if n == 2 || len < 1e-9 && n < 4 {
        out.line_to(b);
        return;
    }
    if len > 1e-9 {
        let straight = pts[1..n - 1].iter().all(|q| {
            let d = *q - a;
            let t = d.dot(chord) / (len * len);
            (d.cross(chord) / len).abs() <= tolerance.min(LINE_MOST * k)
                && (-0.05..=1.05).contains(&t)
        });
        if straight {
            out.line_to(b);
            return;
        }
    }
    let far = ((n - 1) / 3).clamp(1, 6);
    let mut t1 = unit(pts[far] - a);
    let mut t2 = unit(pts[n - 1 - far] - b);
    if t1 == Vec2::ZERO {
        t1 = unit(pts[1] - a);
    }
    if t2 == Vec2::ZERO {
        t2 = unit(pts[n - 2] - b);
    }
    fit_cubics(pts, t1, t2, tolerance, out);
}

/// The border through `corners` (the pixel corners of the finer grid it
/// passes, a unit step apart; a closed one without its first point
/// repeated) as lines and curves, in the finer grid's coordinates: the
/// middles of its steps, smoothed; its corners found and made sharp
/// ([`corners_of`]); each stretch between two corners a straight line or
/// as few cubics as stay within the tolerance. An open border keeps its
/// two ends, where the borders of the other regions meeting there end
/// too.
pub(crate) fn fit(corners: &[Point], closed: bool, f: &Fit) -> BezPath {
    let mut out = BezPath::new();
    let n = corners.len();
    if n == 0 {
        return out;
    }
    let mut mids: Vec<Point> = Vec::with_capacity(n + 2);
    if closed {
        for i in 0..n {
            mids.push(corners[i].midpoint(corners[(i + 1) % n]));
        }
    } else {
        mids.push(corners[0]);
        for e in corners.windows(2) {
            mids.push(e[0].midpoint(e[1]));
        }
        mids.push(corners[n - 1]);
    }
    let n = mids.len();
    if closed && n < 4 {
        return out;
    }
    if !closed && n < 6 {
        out.move_to(mids[0]);
        out.line_to(mids[n - 1]);
        return out;
    }
    // Lightly smoothed: the steps of the finer grid go, corners stay
    // within a fraction of a pixel.
    let light = smooth(&mids, passes_for((f.light * f.k).max(1.0)), closed);
    let found = corners_of(&light, closed, f);
    // Smoothed as the settings ask, for the stretches between corners.
    let sigma = (f.sigma * f.k).max(1.0);
    let soft = if f.sigma <= f.light {
        light
    } else {
        smooth(&mids, passes_for(sigma), closed)
    };
    let tolerance = f.tolerance * f.k;
    // How far from a corner the smoothed border is left out: the rounded
    // part, wider the more it is smoothed.
    let skip = ((SIDE_GAP.max(1.2 * f.sigma) * f.k) * 2.0).round().max(1.0) as usize;
    if closed && found.is_empty() {
        // No corner: quarters meeting smoothly.
        let reach = (n / 8).clamp(1, 6);
        let tangent = |i: usize| unit(soft[(i + reach) % n] - soft[(i + n - reach) % n]);
        out.move_to(soft[0]);
        for q in 0..4 {
            let (a, b) = (q * n / 4, ((q + 1) * n / 4) % n);
            let mut piece: Vec<Point> = if b > a {
                soft[a..=b].to_vec()
            } else {
                soft[a..].to_vec()
            };
            if b <= a {
                piece.push(soft[0]);
            }
            fit_cubics(&piece, tangent(a), -tangent(b), tolerance, &mut out);
        }
        out.close_path();
        return out;
    }
    // The fixed points: corners, and the two ends of an open border.
    let mut stops: Vec<(usize, Point, bool)> = Vec::new();
    if !closed {
        stops.push((0, mids[0], false));
    }
    for c in &found {
        stops.push((c.at, c.point, true));
    }
    if !closed {
        stops.push((n - 1, mids[n - 1], false));
    }
    let count = stops.len();
    out.move_to(stops[0].1);
    let pieces = if closed { count } else { count - 1 };
    let mut piece: Vec<Point> = Vec::new();
    for s in 0..pieces {
        let (a, pa, ca) = stops[s];
        let (b, pb, cb) = stops[(s + 1) % count];
        // The points strictly between the two stops.
        let len = if closed {
            let d = (b + n - a) % n;
            if d == 0 {
                n
            } else {
                d
            }
        } else {
            b - a
        };
        let from = if ca { skip } else { 1 };
        let to = if cb { skip } else { 1 };
        piece.clear();
        piece.push(pa);
        if len > from + to {
            for j in from..=len - to {
                piece.push(soft[(a + j) % n]);
            }
        }
        piece.push(pb);
        fit_piece(&piece, tolerance, f.k, &mut out);
    }
    if closed {
        out.close_path();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grid_grows_finer_for_small_pictures_only() {
        assert_eq!(scale_for(300, 300), 4);
        assert_eq!(scale_for(1024, 1024), 2);
        assert_eq!(scale_for(4000, 3000), 1);
    }
}
