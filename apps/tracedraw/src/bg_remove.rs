//! Image > Remove Background: makes the background of a picture
//! transparent on this computer, without an AI service.
//!
//! 1. The background colours are a list: each entry is Automatic (the
//!    colours learnt from the picture's border, a small k-means in Lab,
//!    so a gradient or a two-tone backdrop gives several colours) or a
//!    colour the user picked, with its own tolerance.
//! 2. Every pixel gets its colour distance (CIE76 Delta E) to the
//!    background colour it is closest to within that colour's
//!    tolerance; it is background when it is within the tolerance of any
//!    listed colour.
//! 3. With Contiguous on, the background is what a flood fill from the
//!    border (and from the points where colours were picked) reaches
//!    through such pixels; it may cross a few edge pixels (the
//!    anti-aliased rim) but never the subject, so background-coloured
//!    parts inside the subject stay. Off, every pixel near a background
//!    colour goes.
//! 4. Opacity ramps over a band past the tolerance (soft, anti-aliased
//!    edges), small loose specks are removed, and Edge softness feathers
//!    the edge.
//! 5. Clean edge colours takes the background's colour out of the
//!    half-transparent edge pixels, so no halo of the old backdrop shows
//!    on a new one.
//! 6. Brush strokes painted on the preview erase more or restore what
//!    went, in the order painted (`BgStroke`).
//! 7. The removed background is left transparent or filled with a
//!    colour or a gradient (`BgFill`).
//!
//! See docs/behavior/remove-background.md.

use image::RgbaImage;
use std::collections::VecDeque;

/// The dialog's settings.
#[derive(Debug, Clone, PartialEq)]
pub struct BgSettings {
    /// How far (Delta E) a colour may be from the background's and still
    /// be background: the tolerance of every listed colour that has none
    /// of its own.
    pub tolerance: f32,
    /// The feather of the edge, in pixels of the picture.
    pub softness: f32,
    /// Only the background connected to the picture's border goes.
    pub contiguous: bool,
    /// Loose specks left in the background go too.
    pub despeckle: bool,
    /// The backdrop's colour is taken out of half-transparent edge pixels.
    pub clean_edges: bool,
    /// The colours to remove; empty is one Automatic entry.
    pub colors: Vec<BgPick>,
    /// Brush strokes painted on the preview, in order: each erases more
    /// or restores the picture under it.
    pub strokes: Vec<BgStroke>,
    /// What the removed background becomes.
    pub fill: BgFill,
}

/// One stroke of the Erase or Restore brush.
#[derive(Debug, Clone, PartialEq)]
pub struct BgStroke {
    /// Erase (true) or Restore (false).
    pub erase: bool,
    /// The brush's diameter, in pixels of the bitmap.
    pub size: f32,
    /// How much of the brush is fully opaque before its edge fades,
    /// 0 to 100 percent.
    pub hardness: f32,
    /// The points the brush passed, as fractions of the picture's width
    /// and height; one point is a dab.
    pub points: Vec<[f32; 2]>,
}

/// What the removed background becomes.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum BgFill {
    /// Transparent.
    #[default]
    None,
    /// One colour.
    Color([u8; 3]),
    /// A linear gradient from `from` to `to` across the picture, along
    /// `angle` degrees (0 left to right, 90 top to bottom).
    Gradient {
        from: [u8; 3],
        to: [u8; 3],
        angle: f32,
    },
}

impl BgFill {
    /// The fill's colour at pixel (`x`, `y`) of a `w` x `h` picture; the
    /// gradient spans the picture along its angle whatever its size, so
    /// a preview and the bitmap agree.
    pub fn at(&self, x: u32, y: u32, w: u32, h: u32) -> Option<[f32; 3]> {
        match *self {
            BgFill::None => None,
            BgFill::Color(c) => Some(c.map(|v| v as f32)),
            BgFill::Gradient { from, to, angle } => {
                let a = if angle.is_finite() { angle } else { 0.0 }.to_radians();
                let (cos, sin) = (a.cos(), a.sin());
                let (wf, hf) = (w.max(1) as f32, h.max(1) as f32);
                let px = x as f32 + 0.5 - wf / 2.0;
                let py = y as f32 + 0.5 - hf / 2.0;
                let ext = (wf * cos.abs() + hf * sin.abs()) / 2.0;
                let t = if ext > 0.0 {
                    ((px * cos + py * sin) / ext * 0.5 + 0.5).clamp(0.0, 1.0)
                } else {
                    0.5
                };
                Some([0, 1, 2].map(|k| from[k] as f32 + (to[k] as f32 - from[k] as f32) * t))
            }
        }
    }
}

/// One colour to remove: Automatic (learnt from the border) or picked,
/// with its own tolerance or the dialog's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BgPick {
    /// The picked colour; None: Automatic.
    pub color: Option<[u8; 3]>,
    /// Its tolerance (Delta E); None: the dialog's Tolerance.
    pub tolerance: Option<f32>,
    /// Where it was picked, as fractions of the picture's width and
    /// height: with Contiguous on the fill starts there too, so a colour
    /// picked inside the subject (a hole) goes.
    pub seed: Option<[f32; 2]>,
}

impl BgPick {
    /// The entry that learns the background from the border.
    pub const AUTO: BgPick = BgPick {
        color: None,
        tolerance: None,
        seed: None,
    };

    /// A picked colour with the dialog's tolerance and no seed.
    pub fn color(c: [u8; 3]) -> Self {
        BgPick {
            color: Some(c),
            ..BgPick::AUTO
        }
    }

    /// This entry's tolerance, `global` when it has none of its own.
    pub fn tolerance_or(&self, global: f32) -> f32 {
        self.tolerance.unwrap_or(global).max(0.0)
    }
}

impl BgSettings {
    /// The colours to remove, never empty: an empty list is Automatic.
    pub fn entries(&self) -> Vec<BgPick> {
        if self.colors.is_empty() {
            vec![BgPick::AUTO]
        } else {
            self.colors.clone()
        }
    }

    /// Add a picked colour (with the seed where it was picked) unless
    /// the list has it already; true when added.
    pub fn add_color(&mut self, c: [u8; 3], seed: Option<[f32; 2]>) -> bool {
        if self.colors.is_empty() {
            self.colors.push(BgPick::AUTO);
        }
        if self.colors.iter().any(|p| p.color == Some(c)) {
            return false;
        }
        self.colors.push(BgPick {
            seed,
            ..BgPick::color(c)
        });
        true
    }

    /// Remove the listed entry nearest in colour to `c` (Automatic
    /// entries by the nearest of `learnt`, the colours learnt from the
    /// picture; on a tie the picked entry); the list never ends up empty: the last entry gone,
    /// Automatic is left. The index removed comes back.
    pub fn remove_nearest(&mut self, c: [u8; 3], learnt: &[BgColor]) -> Option<usize> {
        let l = lab(c);
        let entries = self.entries();
        let best = entries
            .iter()
            .enumerate()
            .filter_map(|(i, p)| {
                // On a tie a picked entry goes before Automatic (whose
                // learnt colours may hold the picked one too).
                let d = match p.color {
                    Some(pc) => (dist2(&lab(pc), &l), 0u8),
                    None => (
                        learnt.iter().map(|b| dist2(&b.lab, &l)).reduce(f32::min)?,
                        1u8,
                    ),
                };
                Some((i, d))
            })
            .min_by(|a, b| a.1 .0.total_cmp(&b.1 .0).then(a.1 .1.cmp(&b.1 .1)))?
            .0;
        self.remove(best);
        Some(best)
    }

    /// Remove entry `i`; the last one gone, Automatic is left.
    pub fn remove(&mut self, i: usize) {
        if self.colors.is_empty() {
            self.colors.push(BgPick::AUTO);
        }
        if i < self.colors.len() {
            self.colors.remove(i);
        }
        if self.colors.is_empty() {
            self.colors.push(BgPick::AUTO);
        }
    }
}

impl Default for BgSettings {
    fn default() -> Self {
        BgSettings {
            tolerance: 14.0,
            softness: 1.0,
            contiguous: true,
            despeckle: true,
            clean_edges: true,
            colors: vec![BgPick::AUTO],
            strokes: Vec::new(),
            fill: BgFill::None,
        }
    }
}

type Lab = [f32; 3];

fn lab(c: [u8; 3]) -> Lab {
    let (l, a, b) = tracedraw_core::color::rgb_to_lab(
        c[0] as f32 / 255.0,
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0,
    );
    [l, a, b]
}

fn dist2(a: &Lab, b: &Lab) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// A background colour: in Lab for distances, in RGB for cleaning edges.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BgColor {
    pub lab: Lab,
    pub rgb: [f32; 3],
}

impl BgColor {
    fn of(c: [u8; 3]) -> Self {
        BgColor {
            lab: lab(c),
            rgb: [c[0] as f32, c[1] as f32, c[2] as f32],
        }
    }
}

/// The pixels of the picture's border, two pixels deep (opaque ones).
fn border(img: &RgbaImage) -> Vec<[u8; 3]> {
    let (w, h) = img.dimensions();
    let depth = 2.min(w).min(h);
    let mut out = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let edge = x < depth || y < depth || x + depth >= w || y + depth >= h;
            if !edge {
                continue;
            }
            let p = img.get_pixel(x, y).0;
            if p[3] >= 128 {
                out.push([p[0], p[1], p[2]]);
            }
        }
    }
    out
}

/// The background colours learnt from the border: up to four k-means
/// clusters in Lab, each kept when it covers at least a tenth of the
/// border. None when the border is mostly transparent already.
pub fn learn_background(img: &RgbaImage) -> Vec<BgColor> {
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return Vec::new();
    }
    let px = border(img);
    let ring = (2 * (w as usize + h as usize)).max(1);
    if px.len() * 3 < ring {
        return Vec::new();
    }
    // A sample of at most 4096 pixels, evenly spread.
    let step = (px.len() / 4096).max(1);
    let pts: Vec<([u8; 3], Lab)> = px.iter().step_by(step).map(|c| (*c, lab(*c))).collect();
    // Farthest-point seeds: the first point, then the one farthest from
    // every centre so far, while it is far enough to be another colour.
    let mut centres: Vec<Lab> = vec![pts[0].1];
    while centres.len() < 6 {
        let (far, d) = pts
            .iter()
            .map(|(_, l)| {
                let d = centres.iter().map(|c| dist2(l, c)).fold(f32::MAX, f32::min);
                (*l, d)
            })
            .fold(([0.0; 3], -1.0), |a, b| if b.1 > a.1 { b } else { a });
        if d < 10.0f32.powi(2) {
            break;
        }
        centres.push(far);
    }
    let mut members = vec![0usize; pts.len()];
    for _ in 0..10 {
        let mut sum = vec![[0.0f64; 6]; centres.len()];
        let mut n = vec![0usize; centres.len()];
        for (i, (rgb, l)) in pts.iter().enumerate() {
            let k = nearest(&centres, l).0;
            members[i] = k;
            n[k] += 1;
            for j in 0..3 {
                sum[k][j] += l[j] as f64;
                sum[k][3 + j] += rgb[j] as f64;
            }
        }
        for (k, c) in centres.iter_mut().enumerate() {
            if n[k] > 0 {
                for j in 0..3 {
                    c[j] = (sum[k][j] / n[k] as f64) as f32;
                }
            }
        }
    }
    let mut out = Vec::new();
    for (k, centre) in centres.iter().enumerate() {
        let mine: Vec<&[u8; 3]> = pts
            .iter()
            .zip(&members)
            .filter(|(_, m)| **m == k)
            .map(|((rgb, _), _)| rgb)
            .collect();
        if mine.len() * 15 < pts.len() {
            continue;
        }
        let mut rgb = [0.0f32; 3];
        for c in &mine {
            for j in 0..3 {
                rgb[j] += c[j] as f32;
            }
        }
        for v in &mut rgb {
            *v /= mine.len() as f32;
        }
        out.push(BgColor { lab: *centre, rgb });
    }
    out
}

fn nearest(centres: &[Lab], l: &Lab) -> (usize, f32) {
    centres
        .iter()
        .enumerate()
        .map(|(i, c)| (i, dist2(l, c)))
        .fold((0, f32::MAX), |a, b| if b.1 < a.1 { b } else { a })
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The picture with its background removed: transparent, or the fill
/// the settings ask for, after the brush strokes. `scale` is the size of
/// `img` against the bitmap (previews are smaller), so the softness and
/// the brush keep their look.
pub fn remove_background(img: &RgbaImage, s: &BgSettings, scale: f32) -> RgbaImage {
    finish(img, &matte(img, s, scale), s, scale)
}

/// What the automatic removal found: each pixel's opacity (1 where the
/// subject stays) and the backdrop colour it was nearest to, for
/// cleaning edge colours. It depends on the colours, tolerance,
/// softness and boxes, not on the brush strokes or the fill, so the
/// preview keeps it while strokes are painted.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Matte {
    pub width: u32,
    pub height: u32,
    pub keep: Vec<f32>,
    /// Each pixel's backdrop colour; empty when nothing was removed.
    pub backdrop: Vec<[f32; 3]>,
}

impl Matte {
    /// Nothing removed: every pixel stays.
    fn whole(w: u32, h: u32) -> Self {
        Matte {
            width: w,
            height: h,
            keep: vec![1.0; w as usize * h as usize],
            backdrop: Vec::new(),
        }
    }
}

/// The automatic removal's matte of `img` (see [`Matte`]).
pub fn matte(img: &RgbaImage, s: &BgSettings, scale: f32) -> Matte {
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return Matte::whole(w, h);
    }
    // Every background colour with its tolerance: an Automatic entry
    // brings the colours learnt from the border, a picked one its own.
    let entries = s.entries();
    let mut bg: Vec<BgColor> = Vec::new();
    let mut tols: Vec<f32> = Vec::new();
    let mut learnt: Option<Vec<BgColor>> = None;
    for p in &entries {
        let t = p.tolerance_or(s.tolerance);
        match p.color {
            Some(c) => {
                bg.push(BgColor::of(c));
                tols.push(t);
            }
            None => {
                for b in learnt.get_or_insert_with(|| learn_background(img)).iter() {
                    bg.push(*b);
                    tols.push(t);
                }
            }
        }
    }
    if bg.is_empty() {
        return Matte::whole(w, h);
    }
    let (wu, hu) = (w as usize, h as usize);
    let n = wu * hu;
    // Each pixel in Lab, its distance to the background colour it is
    // closest to within that colour's tolerance, which one, and that
    // colour's tolerance: a pixel is background when it is within the
    // tolerance of any listed colour.
    let mut labs = vec![[0.0f32; 3]; n];
    let mut dist = vec![0.0f32; n];
    let mut which = vec![0u16; n];
    let mut tolp = vec![tols[0]; n];
    for (i, p) in img.pixels().enumerate() {
        if p.0[3] == 0 {
            continue;
        }
        let l = lab([p.0[0], p.0[1], p.0[2]]);
        let (k, d) = if bg.len() == 1 {
            (0, dist2(&bg[0].lab, &l).sqrt())
        } else {
            // The colour it is deepest inside (least distance past, or
            // most within, that colour's tolerance).
            let mut best = (0, f32::MAX, f32::MAX);
            for (k, (b, t)) in bg.iter().zip(&tols).enumerate() {
                let d = dist2(&b.lab, &l).sqrt();
                if d - t < best.2 {
                    best = (k, d, d - t);
                }
            }
            (best.0, best.1)
        };
        labs[i] = l;
        dist[i] = d;
        which[i] = k as u16;
        tolp[i] = tols[k];
    }
    // The ramp past the tolerance where edge pixels are partly opaque.
    let band = |tol: f32| (tol * 0.6).max(4.0);
    let ramp = |d: f32, tol: f32| smoothstep(tol, tol + band(tol), d);
    // Opacity of the subject, 1 where it stays.
    let mut keep = vec![1.0f32; n];
    if s.contiguous {
        // Flood fill from the border (and the points where colours were
        // picked): through background pixels freely, and at most `rim`
        // pixels into the ramp, so the fill takes the anti-aliased rim
        // and stops at the subject. It also walks on from a background
        // pixel to a neighbour of nearly the same colour (a step under
        // `step`), while that neighbour is within `reach` of a
        // background colour: so a backdrop that drifts (texture, a
        // gradient the border did not show, a soft shadow) goes whole,
        // while the subject's edge, a sharp step, stops it.
        let rim = 2 + s.softness.max(0.0).ceil() as u8;
        let step2 = |tol: f32| (tol / 3.0).max(3.0).powi(2);
        let reach = |tol: f32| (tol * 3.0).max(tol + band(tol));
        let mut depth = vec![u8::MAX; n];
        // Reached by such steps: background, whatever its distance.
        let mut drifted = vec![false; n];
        let mut queue = VecDeque::new();
        let seed = |i: usize, depth: &mut Vec<u8>, queue: &mut VecDeque<usize>| {
            let t = tolp[i];
            if depth[i] == u8::MAX && dist[i] < t + band(t) {
                depth[i] = if dist[i] <= t { 0 } else { 1 };
                if depth[i] <= rim {
                    queue.push_back(i);
                }
            }
        };
        for x in 0..wu {
            seed(x, &mut depth, &mut queue);
            seed((hu - 1) * wu + x, &mut depth, &mut queue);
        }
        for y in 0..hu {
            seed(y * wu, &mut depth, &mut queue);
            seed(y * wu + wu - 1, &mut depth, &mut queue);
        }
        // A picked point and the pixels around it (the point was taken
        // on a copy of another size).
        for at in entries.iter().filter_map(|p| p.seed) {
            if !(at[0].is_finite() && at[1].is_finite()) {
                continue;
            }
            let cx = ((at[0] * w as f32) as i64).clamp(0, wu as i64 - 1);
            let cy = ((at[1] * h as f32) as i64).clamp(0, hu as i64 - 1);
            for y in (cy - 1).max(0)..=(cy + 1).min(hu as i64 - 1) {
                for x in (cx - 1).max(0)..=(cx + 1).min(wu as i64 - 1) {
                    seed(y as usize * wu + x as usize, &mut depth, &mut queue);
                }
            }
        }
        while let Some(i) = queue.pop_front() {
            let (x, y) = (i % wu, i / wu);
            let here = depth[i];
            let step_i = step2(tolp[i]);
            let mut visit = |j: usize| {
                if depth[j] != u8::MAX {
                    return;
                }
                let t = tolp[j];
                let near = dist[j] < t + band(t);
                // A step from solid background only, never from the rim.
                let drift = here == 0
                    && dist[j] < reach(t)
                    && img.get_pixel((j % wu) as u32, (j / wu) as u32).0[3] > 0
                    && dist2(&labs[i], &labs[j]) <= step_i;
                if !near && !drift {
                    return;
                }
                let d = if dist[j] <= t || drift { 0 } else { here + 1 };
                if d > rim {
                    return;
                }
                depth[j] = d;
                drifted[j] = drift && dist[j] > t;
                queue.push_back(j);
            };
            if x > 0 {
                visit(i - 1);
            }
            if x + 1 < wu {
                visit(i + 1);
            }
            if y > 0 {
                visit(i - wu);
            }
            if y + 1 < hu {
                visit(i + wu);
            }
        }
        for i in 0..n {
            if drifted[i] {
                keep[i] = 0.0;
            } else if depth[i] != u8::MAX {
                keep[i] = ramp(dist[i], tolp[i]);
            }
        }
    } else {
        for i in 0..n {
            keep[i] = ramp(dist[i], tolp[i]);
        }
    }
    if s.despeckle {
        despeckle(&mut keep, wu, hu);
    }
    let radius = s.softness.max(0.0) * scale;
    if radius >= 0.5 {
        let r = radius.round() as usize;
        // Two box passes approach a Gaussian; only the edge changes,
        // since flat areas stay flat.
        box_blur(&mut keep, wu, hu, r);
        box_blur(&mut keep, wu, hu, r);
    }
    let backdrop = which
        .iter()
        .map(|&k| bg.get(k as usize).map_or([0.0; 3], |b| b.rgb))
        .collect();
    Matte {
        width: w,
        height: h,
        keep,
        backdrop,
    }
}

/// The opacity the brush strokes leave, starting from the matte's: each
/// stroke in turn erases (opacity times one less its cover) or restores
/// (towards fully kept by its cover). A stroke covers each pixel by its
/// nearest point on the stroke's path, so overlapping parts of one
/// stroke do not add up.
pub fn paint_strokes(keep: &mut [f32], w: u32, h: u32, strokes: &[BgStroke], scale: f32) {
    let (wu, hu) = (w as usize, h as usize);
    if wu == 0 || hu == 0 || keep.len() != wu * hu {
        return;
    }
    let mut cover: Vec<f32> = Vec::new();
    for st in strokes {
        let pts: Vec<[f32; 2]> = st
            .points
            .iter()
            .filter(|p| p[0].is_finite() && p[1].is_finite())
            .map(|p| [p[0] * w as f32, p[1] * h as f32])
            .collect();
        if pts.is_empty() {
            continue;
        }
        let size = if st.size.is_finite() { st.size } else { 1.0 };
        let r = (size.max(1.0) * scale.max(0.0) / 2.0).max(0.5);
        let hard = if st.hardness.is_finite() {
            (st.hardness / 100.0).clamp(0.0, 1.0)
        } else {
            1.0
        };
        // Fully covered out to `inner`, fading to nothing at `r`, over
        // at least one pixel so a hard edge is anti-aliased.
        let inner = (r * hard).min(r - 0.5).max(0.0);
        let fade = (r - inner).max(1.0);
        let profile = |d: f32| {
            if d <= inner {
                1.0
            } else {
                let t = (1.0 - (d - inner) / fade).clamp(0.0, 1.0);
                t * t * (3.0 - 2.0 * t)
            }
        };
        // The stroke's bounds, in pixels.
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in &pts {
            x0 = x0.min(p[0]);
            y0 = y0.min(p[1]);
            x1 = x1.max(p[0]);
            y1 = y1.max(p[1]);
        }
        let bx0 = (x0 - r - 1.0).floor().max(0.0) as usize;
        let by0 = (y0 - r - 1.0).floor().max(0.0) as usize;
        let bx1 = ((x1 + r + 1.0).ceil().max(0.0) as usize).min(wu);
        let by1 = ((y1 + r + 1.0).ceil().max(0.0) as usize).min(hu);
        if bx0 >= bx1 || by0 >= by1 {
            continue;
        }
        let bw = bx1 - bx0;
        cover.clear();
        cover.resize(bw * (by1 - by0), 0.0);
        // Each segment (or the lone dab) covers the pixels near it.
        let segs: Vec<([f32; 2], [f32; 2])> = if pts.len() == 1 {
            vec![(pts[0], pts[0])]
        } else {
            pts.windows(2).map(|s| (s[0], s[1])).collect()
        };
        for (a, b) in segs {
            let sx0 = ((a[0].min(b[0]) - r - 1.0).floor().max(bx0 as f32)) as usize;
            let sy0 = ((a[1].min(b[1]) - r - 1.0).floor().max(by0 as f32)) as usize;
            let sx1 = ((a[0].max(b[0]) + r + 1.0).ceil().max(0.0) as usize).min(bx1);
            let sy1 = ((a[1].max(b[1]) + r + 1.0).ceil().max(0.0) as usize).min(by1);
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let len2 = dx * dx + dy * dy;
            for y in sy0..sy1 {
                let py = y as f32 + 0.5;
                for x in sx0..sx1 {
                    let px = x as f32 + 0.5;
                    let t = if len2 > 0.0 {
                        (((px - a[0]) * dx + (py - a[1]) * dy) / len2).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let (qx, qy) = (a[0] + dx * t - px, a[1] + dy * t - py);
                    let d = (qx * qx + qy * qy).sqrt();
                    if d >= r + 0.5 {
                        continue;
                    }
                    let c = profile(d);
                    let slot = &mut cover[(y - by0) * bw + (x - bx0)];
                    if c > *slot {
                        *slot = c;
                    }
                }
            }
        }
        for y in by0..by1 {
            for x in bx0..bx1 {
                let c = cover[(y - by0) * bw + (x - bx0)];
                if c <= 0.0 {
                    continue;
                }
                let k = &mut keep[y * wu + x];
                *k = if st.erase {
                    *k * (1.0 - c)
                } else {
                    *k + (1.0 - *k) * c
                };
            }
        }
    }
}

/// The picture through the matte, the settings' brush strokes and fill.
/// A pixel the automatic removal made partly transparent loses the
/// backdrop's colour (Clean edge colours); one the brush alone thinned
/// keeps its colour. With a fill, the result is opaque: the picture over
/// the fill.
pub fn finish(img: &RgbaImage, m: &Matte, s: &BgSettings, scale: f32) -> RgbaImage {
    let (w, h) = img.dimensions();
    let n = w as usize * h as usize;
    if m.width != w || m.height != h || m.keep.len() != n {
        return img.clone();
    }
    let mut keep = m.keep.clone();
    paint_strokes(&mut keep, w, h, &s.strokes, scale);
    let mut out = img.clone();
    let clean = s.clean_edges && m.backdrop.len() == n;
    for (i, p) in out.pixels_mut().enumerate() {
        let a = keep[i];
        if a < 0.999 {
            let auto = m.keep[i];
            if clean && auto < 0.999 {
                // The pixel was u * subject + (1 - u) * backdrop: solve
                // for the subject's colour, with u what the automatic
                // removal found or more where the brush restored.
                let u = auto.max(a);
                if u > 0.05 {
                    for (ch, back) in p.0.iter_mut().zip(m.backdrop[i]) {
                        let c = *ch as f32;
                        *ch = ((c - (1.0 - u) * back) / u).round().clamp(0.0, 255.0) as u8;
                    }
                }
            }
            p.0[3] = (p.0[3] as f32 * a).round().clamp(0.0, 255.0) as u8;
        }
        if s.fill != BgFill::None && p.0[3] < 255 {
            let x = (i % w as usize) as u32;
            let y = (i / w as usize) as u32;
            if let Some(f) = s.fill.at(x, y, w, h) {
                let al = p.0[3] as f32 / 255.0;
                for (ch, fc) in p.0.iter_mut().zip(f) {
                    *ch = (*ch as f32 * al + fc * (1.0 - al)).round().clamp(0.0, 255.0) as u8;
                }
                p.0[3] = 255;
            }
        }
    }
    out
}

/// Loose specks: kept parts not touching anything larger, smaller than
/// a two-thousandth of the picture (at least 12 pixels), go.
fn despeckle(keep: &mut [f32], w: usize, h: usize) {
    let n = w * h;
    let limit = (n / 2000).max(12);
    let mut seen = vec![false; n];
    let mut part = Vec::new();
    let mut stack = Vec::new();
    for start in 0..n {
        if seen[start] || keep[start] < 0.5 {
            continue;
        }
        part.clear();
        stack.push(start);
        seen[start] = true;
        while let Some(i) = stack.pop() {
            part.push(i);
            let (x, y) = (i % w, i / w);
            let mut go = |j: usize| {
                if !seen[j] && keep[j] >= 0.5 {
                    seen[j] = true;
                    stack.push(j);
                }
            };
            if x > 0 {
                go(i - 1);
            }
            if x + 1 < w {
                go(i + 1);
            }
            if y > 0 {
                go(i - w);
            }
            if y + 1 < h {
                go(i + w);
            }
        }
        if part.len() < limit && part.len() * 4 < n {
            for &i in &part {
                keep[i] = 0.0;
            }
        }
    }
}

/// A box blur of radius `r` across rows and then columns.
fn box_blur(v: &mut [f32], w: usize, h: usize, r: usize) {
    if r == 0 {
        return;
    }
    let mut tmp = vec![0.0f32; v.len()];
    let pass = |src: &[f32],
                dst: &mut [f32],
                len: usize,
                count: usize,
                at: &dyn Fn(usize, usize) -> usize| {
        for line in 0..count {
            let mut sum = 0.0f32;
            let mut k = 0usize;
            // Running sum over the window, edges clamped.
            let get = |i: isize| src[at(line, i.clamp(0, len as isize - 1) as usize)];
            for i in -(r as isize)..=(r as isize) {
                sum += get(i);
                k += 1;
            }
            for i in 0..len {
                dst[at(line, i)] = sum / k as f32;
                sum += get(i as isize + r as isize + 1) - get(i as isize - r as isize);
            }
        }
    };
    pass(v, &mut tmp, w, h, &|line, i| line * w + i);
    pass(&tmp, v, h, w, &|line, i| i * w + line);
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    /// A red disc on a backdrop that goes from light grey to white, with
    /// a white dot inside the disc and one dark speck in the backdrop.
    fn picture() -> RgbaImage {
        RgbaImage::from_fn(80, 60, |x, y| {
            let (dx, dy) = (x as f32 - 40.0, y as f32 - 30.0);
            let r = (dx * dx + dy * dy).sqrt();
            if (38..40).contains(&x) && (28..30).contains(&y) {
                Rgba([255, 255, 255, 255])
            } else if r < 18.0 {
                Rgba([200, 30, 30, 255])
            } else if r < 19.0 {
                // The anti-aliased rim: half disc, half backdrop.
                Rgba([226, 140, 140, 255])
            } else if (5..7).contains(&x) && (5..7).contains(&y) {
                Rgba([40, 40, 40, 255])
            } else {
                let g = 225 + (x * 30 / 80) as u8;
                Rgba([g, g, g, 255])
            }
        })
    }

    #[test]
    fn the_backdrop_goes_and_the_subject_stays() {
        let img = picture();
        let out = remove_background(&img, &BgSettings::default(), 1.0);
        // Backdrop at both ends of the gradient: gone.
        assert_eq!(out.get_pixel(2, 50).0[3], 0);
        assert_eq!(out.get_pixel(77, 2).0[3], 0);
        // The subject: opaque and its colour unchanged.
        assert_eq!(out.get_pixel(40, 40).0, [200, 30, 30, 255]);
        // The white dot inside the subject is not reached by the fill.
        assert_eq!(out.get_pixel(38, 28).0[3], 255);
        // The loose dark speck in the backdrop is removed.
        assert!(out.get_pixel(5, 5).0[3] < 40, "{:?}", out.get_pixel(5, 5));
    }

    #[test]
    fn everywhere_mode_takes_background_coloured_parts_inside_too() {
        let img = picture();
        // No feather, which would blur the tiny hole partly back.
        let s = BgSettings {
            contiguous: false,
            softness: 0.0,
            ..BgSettings::default()
        };
        let out = remove_background(&img, &s, 1.0);
        assert_eq!(out.get_pixel(38, 28).0[3], 0);
        assert_eq!(out.get_pixel(40, 40).0[3], 255);
    }

    #[test]
    fn edge_pixels_lose_the_backdrops_colour() {
        let img = picture();
        let s = BgSettings {
            softness: 0.0,
            ..BgSettings::default()
        };
        let out = remove_background(&img, &s, 1.0);
        // A rim pixel: partly transparent, its colour pulled toward the
        // disc's red rather than the backdrop's pink.
        let rim = (0..80)
            .map(|x| *out.get_pixel(x, 30))
            .find(|p| p.0[3] > 10 && p.0[3] < 245);
        if let Some(p) = rim {
            assert!(p.0[0] > p.0[1] + 60, "{p:?}");
        }
        let plain = remove_background(
            &img,
            &BgSettings {
                clean_edges: false,
                ..s
            },
            1.0,
        );
        // Without cleaning, colours stay as they were.
        for (a, b) in img.pixels().zip(plain.pixels()) {
            assert_eq!(a.0[..3], b.0[..3]);
        }
    }

    #[test]
    fn learnt_colours_cover_a_two_tone_backdrop() {
        let img = RgbaImage::from_fn(40, 40, |x, y| {
            if (12..28).contains(&x) && (12..28).contains(&y) {
                Rgba([20, 160, 40, 255])
            } else if x < 20 {
                Rgba([250, 250, 250, 255])
            } else {
                Rgba([30, 60, 200, 255])
            }
        });
        assert_eq!(learn_background(&img).len(), 2);
        let out = remove_background(&img, &BgSettings::default(), 1.0);
        assert_eq!(out.get_pixel(1, 20).0[3], 0);
        assert_eq!(out.get_pixel(38, 20).0[3], 0);
        assert_eq!(out.get_pixel(20, 20).0[3], 255);
    }

    #[test]
    fn picked_colours_and_tolerance() {
        let img = picture();
        // A picked colour far from the backdrop: nothing goes.
        let s = BgSettings {
            colors: vec![BgPick::color([0, 0, 255])],
            ..BgSettings::default()
        };
        let out = remove_background(&img, &s, 1.0);
        assert!(out.pixels().all(|p| p.0[3] == 255));
        // Tolerance 0 on a gradient keeps most of it.
        let s = BgSettings {
            tolerance: 0.0,
            despeckle: false,
            softness: 0.0,
            ..BgSettings::default()
        };
        let out = remove_background(&img, &s, 1.0);
        let gone = out.pixels().filter(|p| p.0[3] == 0).count();
        let loose = remove_background(&img, &BgSettings::default(), 1.0)
            .pixels()
            .filter(|p| p.0[3] == 0)
            .count();
        assert!(gone < loose, "{gone} {loose}");
    }

    #[test]
    fn a_transparent_picture_is_left_alone() {
        let img = RgbaImage::from_fn(20, 20, |x, y| {
            if (5..15).contains(&x) && (5..15).contains(&y) {
                Rgba([10, 10, 10, 255])
            } else {
                Rgba([0, 0, 0, 0])
            }
        });
        assert_eq!(remove_background(&img, &BgSettings::default(), 1.0), img);
        let empty = RgbaImage::new(0, 0);
        assert_eq!(
            remove_background(&empty, &BgSettings::default(), 1.0),
            empty
        );
    }

    #[test]
    fn feathering_softens_only_the_edge() {
        let img = picture();
        let s = BgSettings {
            softness: 3.0,
            ..BgSettings::default()
        };
        let out = remove_background(&img, &s, 1.0);
        assert_eq!(out.get_pixel(40, 40).0[3], 255);
        assert_eq!(out.get_pixel(2, 50).0[3], 0);
        let partial = (0..80)
            .filter(|x| {
                let a = out.get_pixel(*x, 30).0[3];
                a > 0 && a < 255
            })
            .count();
        assert!(partial >= 4, "{partial}");
    }

    /// A backdrop that drifts away from the colours its border shows (a
    /// soft shadow under the subject, a texture) goes whole: the fill
    /// walks on through small steps of colour, while the subject's edge,
    /// a sharp step, stops it (#231).
    #[test]
    fn a_drifting_backdrop_goes_and_the_subject_stays() {
        // A white backdrop with a grey shadow fading in over 30 px under
        // a red disc, and a faint texture all over.
        let img = RgbaImage::from_fn(120, 90, |x, y| {
            let (dx, dy) = (x as f32 - 60.0, y as f32 - 40.0);
            if (dx * dx + dy * dy).sqrt() < 20.0 {
                return Rgba([200, 30, 30, 255]);
            }
            let (sx, sy) = (x as f32 - 60.0, y as f32 - 68.0);
            let shade = (1.0 - ((sx / 40.0).powi(2) + (sy / 10.0).powi(2)).sqrt()).max(0.0);
            let noise = ((x * 7 + y * 13) % 5) as i32 - 2;
            let v = (250.0 - 90.0 * shade) as i32 + noise;
            let v = v.clamp(0, 255) as u8;
            Rgba([v, v, v.saturating_sub(4), 255])
        });
        let out = remove_background(&img, &BgSettings::default(), 1.0);
        // The darkest of the shadow is 36 Delta E from white, past the
        // tolerance and its band, yet gone.
        assert_eq!(out.get_pixel(60, 68).0[3], 0, "{:?}", out.get_pixel(60, 68));
        assert_eq!(out.get_pixel(3, 3).0[3], 0);
        // The subject keeps its colour, to its edge.
        assert_eq!(out.get_pixel(60, 40).0, [200, 30, 30, 255]);
        assert_eq!(out.get_pixel(60, 24).0[3], 255);
        // (Counted at half opacity: the feather softens the rim.)
        let kept = out.pixels().filter(|p| p.0[3] >= 128).count();
        let disc = (std::f32::consts::PI * 20.0 * 20.0) as usize;
        assert!(
            kept >= disc * 9 / 10 && kept <= disc * 11 / 10,
            "{kept} vs {disc}"
        );
        // Everywhere mode (no fill) still goes by colour alone: the
        // shadow's dark middle stays.
        let s = BgSettings {
            contiguous: false,
            ..BgSettings::default()
        };
        assert_eq!(remove_background(&img, &s, 1.0).get_pixel(60, 68).0[3], 255);
    }

    /// A white backdrop with a red disc; a green patch on the backdrop
    /// (touching it, not the border) and a blue hole inside the disc.
    fn three_colours() -> RgbaImage {
        RgbaImage::from_fn(100, 80, |x, y| {
            let (dx, dy) = (x as f32 - 60.0, y as f32 - 40.0);
            let r = (dx * dx + dy * dy).sqrt();
            if r < 5.0 {
                Rgba([30, 60, 220, 255])
            } else if r < 20.0 {
                Rgba([200, 30, 30, 255])
            } else if (10..30).contains(&x) && (20..60).contains(&y) {
                Rgba([40, 180, 60, 255])
            } else {
                Rgba([250, 250, 250, 255])
            }
        })
    }

    fn sharp(colors: Vec<BgPick>) -> BgSettings {
        BgSettings {
            softness: 0.0,
            colors,
            ..BgSettings::default()
        }
    }

    /// With Contiguous, the fill goes through pixels matching any listed
    /// colour: Automatic alone stops at the green patch, Automatic and
    /// green take both, and the subject stays.
    #[test]
    fn the_fill_goes_through_every_listed_colour() {
        let img = three_colours();
        let auto = remove_background(&img, &sharp(vec![BgPick::AUTO]), 1.0);
        assert_eq!(auto.get_pixel(2, 2).0[3], 0);
        assert_eq!(auto.get_pixel(20, 40).0[3], 255, "green stays");
        let both = sharp(vec![BgPick::AUTO, BgPick::color([40, 180, 60])]);
        let out = remove_background(&img, &both, 1.0);
        assert_eq!(out.get_pixel(2, 2).0[3], 0);
        assert_eq!(out.get_pixel(20, 40).0[3], 0, "green goes too");
        assert_eq!(out.get_pixel(50, 40).0, [200, 30, 30, 255]);
        // The blue hole is not reached from the border.
        assert_eq!(out.get_pixel(60, 40).0[3], 255);
        // Two picked colours, no Automatic: the same.
        let picked = sharp(vec![
            BgPick::color([250, 250, 250]),
            BgPick::color([40, 180, 60]),
        ]);
        let out2 = remove_background(&img, &picked, 1.0);
        assert_eq!(out2.get_pixel(20, 40).0[3], 0);
        assert_eq!(out2.get_pixel(2, 2).0[3], 0);
        assert_eq!(out2.get_pixel(50, 40).0[3], 255);
    }

    /// A third colour picked inside the subject goes from the point it
    /// was picked at (Contiguous), everywhere without Contiguous; Remove
    /// specks, Edge softness and Clean edge colors leave the subject.
    #[test]
    fn a_colour_picked_inside_goes_from_its_seed() {
        let img = three_colours();
        let mut s = sharp(vec![BgPick::AUTO, BgPick::color([40, 180, 60])]);
        s.colors.push(BgPick::color([30, 60, 220]));
        let unseeded = remove_background(&img, &s, 1.0);
        assert_eq!(unseeded.get_pixel(60, 40).0[3], 255, "not reached");
        s.colors[2].seed = Some([0.6, 0.5]);
        let out = remove_background(&img, &s, 1.0);
        assert_eq!(out.get_pixel(60, 40).0[3], 0, "reached from its seed");
        assert_eq!(out.get_pixel(20, 40).0[3], 0);
        assert_eq!(out.get_pixel(50, 40).0, [200, 30, 30, 255]);
        // Without Contiguous (and no seed), the blue goes everywhere.
        s.colors[2].seed = None;
        let everywhere = BgSettings {
            contiguous: false,
            ..s.clone()
        };
        let out = remove_background(&img, &everywhere, 1.0);
        assert_eq!(out.get_pixel(60, 40).0[3], 0);
        assert_eq!(out.get_pixel(50, 40).0[3], 255);
        // Feathered, the union's edge softens and the subject's middle
        // stays.
        let soft = BgSettings { softness: 2.0, ..s };
        let out = remove_background(&img, &soft, 1.0);
        assert_eq!(out.get_pixel(50, 40).0[3], 255);
        assert_eq!(out.get_pixel(20, 40).0[3], 0);
    }

    /// Each colour has its own tolerance; one left unset follows the
    /// dialog's.
    #[test]
    fn each_colour_has_its_own_tolerance() {
        // A grey backdrop and a darker grey patch 10 Delta E or so away.
        let img = RgbaImage::from_fn(60, 40, |x, y| {
            if (20..40).contains(&x) && (10..30).contains(&y) {
                Rgba([200, 30, 30, 255])
            } else if x < 10 {
                Rgba([150, 150, 150, 255])
            } else {
                Rgba([180, 180, 180, 255])
            }
        });
        let tight = BgPick {
            tolerance: Some(1.0),
            ..BgPick::color([170, 170, 170])
        };
        let s = BgSettings {
            tolerance: 30.0,
            softness: 0.0,
            despeckle: false,
            colors: vec![tight],
            ..BgSettings::default()
        };
        // Its own tolerance of 1: neither grey goes.
        let out = remove_background(&img, &s, 1.0);
        assert_eq!(out.get_pixel(5, 5).0[3], 255);
        assert_eq!(out.get_pixel(50, 5).0[3], 255);
        // Following the dialog's 30: both go, the subject stays.
        let loose = BgSettings {
            colors: vec![BgPick::color([170, 170, 170])],
            ..s.clone()
        };
        let out = remove_background(&img, &loose, 1.0);
        assert_eq!(out.get_pixel(5, 5).0[3], 0);
        assert_eq!(out.get_pixel(50, 5).0[3], 0);
        assert_eq!(out.get_pixel(30, 20).0[3], 255);
        // Two colours, each tight: each takes its own grey.
        let two = BgSettings {
            colors: vec![
                BgPick {
                    tolerance: Some(2.0),
                    ..BgPick::color([150, 150, 150])
                },
                BgPick {
                    tolerance: Some(2.0),
                    ..BgPick::color([180, 180, 180])
                },
            ],
            ..s
        };
        let out = remove_background(&img, &two, 1.0);
        assert_eq!(out.get_pixel(5, 5).0[3], 0);
        assert_eq!(out.get_pixel(50, 5).0[3], 0);
        assert_eq!(out.get_pixel(30, 20).0[3], 255);
    }

    /// The list: adding skips a colour listed already, removing the
    /// nearest takes the closest entry and never leaves the list empty.
    #[test]
    fn the_colour_list_adds_and_removes() {
        let mut s = BgSettings {
            colors: Vec::new(),
            ..BgSettings::default()
        };
        assert_eq!(s.entries(), vec![BgPick::AUTO]);
        assert!(s.add_color([0, 200, 0], Some([0.1, 0.2])));
        assert!(!s.add_color([0, 200, 0], None));
        assert!(s.add_color([0, 0, 200], None));
        assert_eq!(s.colors.len(), 3);
        assert_eq!(s.colors[0], BgPick::AUTO);
        assert_eq!(s.colors[1].seed, Some([0.1, 0.2]));
        let learnt = [BgColor::of([255, 255, 255])];
        // Near blue: the blue entry goes.
        assert_eq!(s.remove_nearest([10, 10, 190], &learnt), Some(2));
        // Near white: Automatic (its learnt white) goes; green is first.
        assert_eq!(s.remove_nearest([250, 250, 250], &learnt), Some(0));
        assert_eq!(
            s.colors,
            vec![BgPick {
                seed: Some([0.1, 0.2]),
                ..BgPick::color([0, 200, 0])
            }]
        );
        assert_eq!(s.remove_nearest([0, 0, 0], &learnt), Some(0));
        assert_eq!(s.colors, vec![BgPick::AUTO]);
        // Automatic with nothing learnt cannot be the nearest.
        assert_eq!(s.remove_nearest([0, 0, 0], &[]), None);
        s.remove(7);
        assert_eq!(s.colors, vec![BgPick::AUTO]);
        // A tie between Automatic's learnt white and a picked white: the
        // picked one goes.
        let mut t = BgSettings {
            colors: vec![BgPick::AUTO, BgPick::color([255, 255, 255])],
            ..BgSettings::default()
        };
        assert_eq!(t.remove_nearest([255, 255, 255], &learnt), Some(1));
    }

    fn stroke(erase: bool, size: f32, hardness: f32, points: &[[f32; 2]]) -> BgStroke {
        BgStroke {
            erase,
            size,
            hardness,
            points: points.to_vec(),
        }
    }

    /// The Erase brush takes away the subject under it; the Restore
    /// brush brings back the removed backdrop with its own colours; the
    /// strokes count in the order painted, and what neither touches is
    /// as the automatic removal left it.
    #[test]
    fn the_brushes_erase_and_restore() {
        let img = picture();
        let plain = remove_background(&img, &BgSettings::default(), 1.0);
        assert_eq!(plain.get_pixel(40, 22).0[3], 255);
        assert_eq!(plain.get_pixel(70, 30).0[3], 0);
        // Erase across the disc's middle row, hard: the subject goes
        // there; the backdrop at (70, 30) is restored by the next stroke.
        let s = BgSettings {
            strokes: vec![
                stroke(true, 6.0, 100.0, &[[0.4, 0.5], [0.6, 0.5]]),
                stroke(false, 10.0, 100.0, &[[70.0 / 80.0, 0.5]]),
            ],
            ..BgSettings::default()
        };
        let out = remove_background(&img, &s, 1.0);
        assert_eq!(out.get_pixel(40, 30).0[3], 0, "erased");
        assert_eq!(out.get_pixel(44, 30).0[3], 0, "erased along the path");
        assert_eq!(out.get_pixel(40, 22).0[3], 255, "off the path, kept");
        assert_eq!(out.get_pixel(70, 30).0, img.get_pixel(70, 30).0, "restored");
        assert_eq!(out.get_pixel(70, 40).0[3], 0, "off the dab, removed");
        // The same strokes the other way round: restoring first, then
        // erasing over it, leaves it erased.
        let s = BgSettings {
            strokes: vec![
                stroke(false, 10.0, 100.0, &[[0.5, 0.5]]),
                stroke(true, 10.0, 100.0, &[[0.5, 0.5]]),
            ],
            ..BgSettings::default()
        };
        assert_eq!(remove_background(&img, &s, 1.0).get_pixel(40, 30).0[3], 0);
    }

    /// Hardness: a hard brush covers fully out to near its edge, a soft
    /// one fades from its centre, so half way out it erases only part.
    #[test]
    fn hardness_fades_the_brush_edge() {
        let img = RgbaImage::from_pixel(100, 100, Rgba([200, 30, 30, 255]));
        let s = |hardness: f32| BgSettings {
            colors: vec![BgPick::color([0, 0, 255])],
            strokes: vec![stroke(true, 40.0, hardness, &[[0.5, 0.5]])],
            ..BgSettings::default()
        };
        let hard = remove_background(&img, &s(100.0), 1.0);
        let soft = remove_background(&img, &s(0.0), 1.0);
        assert_eq!(hard.get_pixel(50, 50).0[3], 0);
        assert_eq!(hard.get_pixel(50 + 15, 50).0[3], 0);
        assert_eq!(hard.get_pixel(50 + 25, 50).0[3], 255, "outside the brush");
        assert!(soft.get_pixel(50, 50).0[3] < 5, "the centre is covered");
        let mid = soft.get_pixel(50 + 10, 50).0[3];
        assert!(mid > 20 && mid < 235, "half way out: {mid}");
        // The brush alone thinned it: its colour stays.
        assert_eq!(&soft.get_pixel(60, 50).0[..3], &[200, 30, 30]);
    }

    /// The brush's size is in pixels of the bitmap: on a copy at half
    /// the size it covers the same part of the picture.
    #[test]
    fn the_brush_keeps_its_size_on_a_smaller_copy() {
        let big = RgbaImage::from_pixel(200, 200, Rgba([200, 30, 30, 255]));
        let small = RgbaImage::from_pixel(100, 100, Rgba([200, 30, 30, 255]));
        let s = BgSettings {
            colors: vec![BgPick::color([0, 0, 255])],
            strokes: vec![stroke(true, 60.0, 100.0, &[[0.5, 0.5]])],
            ..BgSettings::default()
        };
        let gone = |img: &RgbaImage| img.pixels().filter(|p| p.0[3] == 0).count() as f32;
        let a = gone(&remove_background(&big, &s, 1.0)) / 40000.0;
        let b = gone(&remove_background(&small, &s, 0.5)) / 10000.0;
        assert!(a > 0.05, "{a}");
        assert!((a - b).abs() < 0.01, "{a} vs {b}");
    }

    /// A fill puts a colour or a gradient where the background went: the
    /// result is opaque, the subject keeps its colour, the rim is the
    /// subject over the new colour.
    #[test]
    fn the_background_is_filled_with_a_colour_or_a_gradient() {
        let img = picture();
        let s = BgSettings {
            fill: BgFill::Color([0, 0, 255]),
            ..BgSettings::default()
        };
        let out = remove_background(&img, &s, 1.0);
        assert!(out.pixels().all(|p| p.0[3] == 255), "opaque");
        assert_eq!(out.get_pixel(70, 30).0, [0, 0, 255, 255]);
        assert_eq!(out.get_pixel(40, 22).0, [200, 30, 30, 255]);
        // The gradient: left to right at 0 degrees, top to bottom at 90.
        let g = |angle: f32| BgSettings {
            fill: BgFill::Gradient {
                from: [0, 0, 0],
                to: [255, 255, 255],
                angle,
            },
            ..BgSettings::default()
        };
        let lr = remove_background(&img, &g(0.0), 1.0);
        assert!(lr.get_pixel(1, 30).0[0] < 10);
        assert!(lr.get_pixel(78, 30).0[0] > 245);
        assert_eq!(lr.get_pixel(40, 22).0, [200, 30, 30, 255]);
        let tb = remove_background(&img, &g(90.0), 1.0);
        assert!(tb.get_pixel(40, 0).0[0] < 10);
        assert!(tb.get_pixel(40, 59).0[0] > 245);
        // Nothing removed (a transparent picture left alone), the fill
        // still goes under its transparent parts.
        let clear = RgbaImage::from_pixel(10, 10, Rgba([0, 0, 0, 0]));
        let out = remove_background(&clear, &s, 1.0);
        assert_eq!(out.get_pixel(5, 5).0, [0, 0, 255, 255]);
        // The same gradient on a copy of half the size agrees.
        let f = g(30.0).fill;
        let a = f.at(40, 10, 80, 60).expect("colour");
        let b = f.at(20, 5, 40, 30).expect("colour");
        assert!((a[0] - b[0]).abs() < 4.0, "{a:?} {b:?}");
    }

    /// Strokes with no points, or points that are not numbers, change
    /// nothing; strokes off the picture are cut at its edge.
    #[test]
    fn odd_strokes_change_nothing() {
        let img = picture();
        let plain = remove_background(&img, &BgSettings::default(), 1.0);
        let s = BgSettings {
            strokes: vec![
                stroke(true, 10.0, 50.0, &[]),
                stroke(true, f32::NAN, f32::NAN, &[[f32::NAN, 0.5]]),
                stroke(true, 10.0, 50.0, &[[-3.0, -3.0], [-2.0, 5.0]]),
            ],
            ..BgSettings::default()
        };
        assert_eq!(remove_background(&img, &s, 1.0), plain);
    }
}
