//! Small thin letters of an enlarged picture drawn again by their
//! strokes. See docs/behavior/bitmap-tracing.md, "Small letters by
//! their strokes".
//!
//! A letter a few pixels tall in the picture before it was enlarged has
//! strokes about two pixels wide there. The enlarging gives it an
//! outline, but not a true one: its stems lean, its bowls bulge, its
//! strokes thicken and thin. Such a letter was drawn with a pen of one
//! width, and that says more about it than its outline does: the line
//! down the middle of each stroke is found (the letter thinned to one
//! pixel), fitted with straight lines, arcs and corners, and drawn
//! again with the one width the letter has, its ends cut square. The
//! letter drawn again is compared with the letter of the picture, and
//! where the two do not agree the letter is left as the picture has it.

use tracedraw_core::geometry::{BezPath, ParamCurve, Point, Vec2};

use crate::trace_fit::Faithful;

/// The longest side of a letter drawn again, pixels of the picture
/// before it was enlarged.
const BOX_MOST: f64 = 20.0;
/// The widths of stroke drawn again, the same pixels.
const WIDE_MOST: f64 = 2.7;
const WIDE_LEAST: f64 = 0.8;
/// A letter is of one width when at least [`EVEN_SHARE`] of its middle
/// line lies within [`EVEN`] of the middle width (and a point of the
/// grid).
const EVEN_SHARE: f64 = 0.8;
const EVEN: f64 = 0.3;
/// A branch of the middle line shorter than this many widths is the
/// thinning's (it forks into the corners of a square end).
const SPUR: f64 = 0.9;
/// Two branches at a meeting go straight through it when they turn
/// less than this, radians.
const THROUGH: f64 = 0.5;
/// How far the middle line may stray from the thinned one, and how far
/// the letter drawn again from the picture's, pixels of the picture
/// before it was enlarged; and the share of the letter that may lie
/// further.
const FIT_TOLERANCE: f64 = 0.2;
/// The longest piece between two lines taken for a cut corner, widths.
const CHAMFER: f64 = 0.9;
/// A stroke has a width of its own when it differs from the letter's by
/// more than [`OWN_LEAST`] of it, and by [`OWN_MOST`] at most.
const OWN_LEAST: f64 = 0.12;
const OWN_MOST: f64 = 0.3;
/// How much of the tip of a thinned line is left out, widths.
const TIP: f64 = 0.6;
const STRAY_MOST: f64 = 0.6;
const STRAY_SHARE: f64 = 0.015;
/// The same distance is no more than this share of the stroke's width.
const STRAY_WIDE: f64 = 0.3;
/// A turn of the middle line sharper than this is a corner, drawn with
/// a point, up to [`MITER_MOST`].
const MITER_LEAST: f64 = 0.45;
const MITER_MOST: f64 = 2.4;
/// Two colours this near are shades of one, a letter in both one letter.
const SHADE_NEAR: i64 = 48;

fn unit(v: Vec2) -> Vec2 {
    let l = v.hypot();
    if l > 1e-12 && l.is_finite() {
        v / l
    } else {
        Vec2::ZERO
    }
}

/// A stretch of the middle line: its points, the meeting each end is at
/// (none for a free end) and whether it is a loop.
struct Chain {
    pts: Vec<Point>,
    ends: [Option<usize>; 2],
    closed: bool,
    alive: bool,
}

impl Chain {
    fn length(&self) -> f64 {
        self.pts.windows(2).map(|p| (p[1] - p[0]).hypot()).sum()
    }
}

/// The index of the point of `pts` at least `far` along it from its
/// first point (its last when it is shorter).
fn index_at(pts: &[Point], far: f64) -> usize {
    let mut d = 0.0;
    for i in 1..pts.len() {
        d += (pts[i] - pts[i - 1]).hypot();
        if d >= far {
            return i;
        }
    }
    pts.len().saturating_sub(1)
}

/// `pts` again with its points `step` apart.
fn resample(pts: &[Point], step: f64, closed: bool) -> Vec<Point> {
    let mut src = pts.to_vec();
    if closed {
        if let Some(first) = src.first().copied() {
            src.push(first);
        }
    }
    let total: f64 = src.windows(2).map(|p| (p[1] - p[0]).hypot()).sum();
    if src.len() < 2 || total < step {
        return pts.to_vec();
    }
    let n = (total / step).round().max(1.0) as usize;
    let each = total / n as f64;
    let mut out = Vec::with_capacity(n + 1);
    out.push(src[0]);
    let (mut seg, mut before) = (0usize, 0.0);
    for i in 1..n {
        let want = each * i as f64;
        loop {
            let len = (src[seg + 1] - src[seg]).hypot();
            if before + len >= want || seg + 2 >= src.len() {
                let t = if len > 1e-12 {
                    ((want - before) / len).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                out.push(src[seg].lerp(src[seg + 1], t));
                break;
            }
            before += len;
            seg += 1;
        }
    }
    if !closed {
        out.push(src[src.len() - 1]);
    }
    out
}

/// How far each set point of `mask` (`w` x `h`) lies from the nearest
/// unset one (a point outside the box is unset).
fn distances(mask: &[bool], w: usize, h: usize) -> Vec<f32> {
    const DIAGONAL: f32 = std::f32::consts::SQRT_2;
    let mut d: Vec<f32> = mask
        .iter()
        .map(|m| if *m { f32::MAX } else { 0.0 })
        .collect();
    let at = |d: &[f32], x: i64, y: i64| -> f32 {
        if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 {
            0.0
        } else {
            d[y as usize * w + x as usize]
        }
    };
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            let i = y as usize * w + x as usize;
            if d[i] == 0.0 {
                continue;
            }
            let best = (at(&d, x - 1, y) + 1.0)
                .min(at(&d, x, y - 1) + 1.0)
                .min(at(&d, x - 1, y - 1) + DIAGONAL)
                .min(at(&d, x + 1, y - 1) + DIAGONAL);
            d[i] = d[i].min(best);
        }
    }
    for y in (0..h as i64).rev() {
        for x in (0..w as i64).rev() {
            let i = y as usize * w + x as usize;
            if d[i] == 0.0 {
                continue;
            }
            let best = (at(&d, x + 1, y) + 1.0)
                .min(at(&d, x, y + 1) + 1.0)
                .min(at(&d, x + 1, y + 1) + DIAGONAL)
                .min(at(&d, x - 1, y + 1) + DIAGONAL);
            d[i] = d[i].min(best);
        }
    }
    d
}

/// How many 4-connected patches of unset points `mask` has.
fn grounds(mask: &[bool], w: usize, h: usize) -> usize {
    let mut seen = vec![false; w * h];
    let (mut count, mut stack) = (0usize, Vec::new());
    for start in 0..w * h {
        if mask[start] || seen[start] {
            continue;
        }
        count += 1;
        seen[start] = true;
        stack.push(start);
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            let mut visit = |j: usize| {
                if !mask[j] && !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            };
            if x > 0 {
                visit(i - 1);
            }
            if x + 1 < w {
                visit(i + 1);
            }
            if y > 0 {
                visit(i - w);
            }
            if y + 1 < h {
                visit(i + w);
            }
        }
    }
    count
}

/// The eight points round `i`, clockwise from the one above; none
/// outside the box.
fn ring(i: usize, w: usize, h: usize) -> [Option<usize>; 8] {
    const ROUND: [(i64, i64); 8] = [
        (0, -1),
        (1, -1),
        (1, 0),
        (1, 1),
        (0, 1),
        (-1, 1),
        (-1, 0),
        (-1, -1),
    ];
    let (x, y) = ((i % w) as i64, (i / w) as i64);
    ROUND.map(|(dx, dy)| {
        let (u, v) = (x + dx, y + dy);
        (u >= 0 && v >= 0 && u < w as i64 && v < h as i64).then(|| v as usize * w + u as usize)
    })
}

/// The middle line `thin` (`w` x `h`, one point wide) as chains between
/// its meetings and its free ends, and how many meetings it has.
fn chains_of(thin: &[bool], w: usize, h: usize) -> (Vec<Chain>, Vec<Point>) {
    let centre = |i: usize| Point::new((i % w) as f64 + 0.5, (i / w) as f64 + 0.5);
    let set = |j: Option<usize>| j.is_some_and(|j| thin[j]);
    // 1 a point of a chain, 2 a free end, 3 a meeting.
    let mut kind = vec![0u8; w * h];
    for i in 0..w * h {
        if !thin[i] {
            continue;
        }
        let r = ring(i, w, h);
        let turns = (0..8)
            .filter(|k| !set(r[*k]) && set(r[(k + 1) % 8]))
            .count();
        let around = r.iter().filter(|j| set(**j)).count();
        kind[i] = if around == 0 {
            0
        } else if turns <= 1 {
            2
        } else if turns >= 3 {
            3
        } else {
            1
        };
    }
    // The points of a meeting that touch are one meeting.
    let mut node = vec![usize::MAX; w * h];
    let mut nodes: Vec<Point> = Vec::new();
    let mut stack = Vec::new();
    for start in 0..w * h {
        if kind[start] != 3 || node[start] != usize::MAX {
            continue;
        }
        let id = nodes.len();
        let (mut sum, mut count) = (Vec2::ZERO, 0.0);
        node[start] = id;
        stack.push(start);
        while let Some(i) = stack.pop() {
            sum += centre(i).to_vec2();
            count += 1.0;
            for j in ring(i, w, h).into_iter().flatten() {
                if kind[j] == 3 && node[j] == usize::MAX {
                    node[j] = id;
                    stack.push(j);
                }
            }
        }
        nodes.push((sum / count).to_point());
    }
    let mut seen = vec![false; w * h];
    let mut chains: Vec<Chain> = Vec::new();
    // Walk from `first`, a point of a chain beside where it starts.
    let walk = |first: usize, from: Option<usize>, start: Point, seen: &mut Vec<bool>| {
        let mut pts = vec![start, centre(first)];
        seen[first] = true;
        let (mut at, mut cells) = (first, 1usize);
        let beside = |i: usize, n: usize| {
            ring(i, w, h)
                .into_iter()
                .flatten()
                .any(|j| kind[j] == 3 && node[j] == n)
        };
        let end = loop {
            let r = ring(at, w, h);
            // Where it ends: a meeting or a free end beside it.
            let mut found = None;
            for j in r.into_iter().flatten() {
                if kind[j] == 3 && (Some(node[j]) != from || cells > 2) {
                    found = Some((nodes[node[j]], Some(node[j])));
                    break;
                }
            }
            if found.is_none() {
                for j in r.into_iter().flatten() {
                    if kind[j] == 2 && !seen[j] {
                        seen[j] = true;
                        found = Some((centre(j), None));
                        break;
                    }
                }
            }
            if let Some((p, n)) = found {
                pts.push(p);
                break n;
            }
            // Straight neighbours before diagonal ones.
            let mut next = None;
            for k in [0usize, 2, 4, 6, 1, 3, 5, 7] {
                let Some(j) = r[k] else { continue };
                if kind[j] != 1 || seen[j] {
                    continue;
                }
                if cells <= 2 && from.is_some_and(|n| beside(j, n)) {
                    continue;
                }
                next = Some(j);
                break;
            }
            let Some(j) = next else { break None };
            seen[j] = true;
            pts.push(centre(j));
            at = j;
            cells += 1;
        };
        Chain {
            pts,
            ends: [from, end],
            closed: false,
            alive: true,
        }
    };
    for i in 0..w * h {
        if kind[i] != 3 {
            continue;
        }
        for j in ring(i, w, h).into_iter().flatten() {
            if kind[j] == 1 && !seen[j] {
                chains.push(walk(j, Some(node[i]), nodes[node[i]], &mut seen));
            }
        }
    }
    for i in 0..w * h {
        if kind[i] != 2 || seen[i] {
            continue;
        }
        seen[i] = true;
        let first = ring(i, w, h)
            .into_iter()
            .flatten()
            .find(|j| kind[*j] == 1 && !seen[*j]);
        if let Some(j) = first {
            chains.push(walk(j, None, centre(i), &mut seen));
        }
    }
    // What is left has no meeting and no end: loops.
    for i in 0..w * h {
        if kind[i] != 1 || seen[i] {
            continue;
        }
        let mut c = walk(i, None, centre(i), &mut seen);
        c.pts.remove(0);
        let n = c.pts.len();
        if n >= 8 && (c.pts[0] - c.pts[n - 1]).hypot() < 1.5 {
            c.closed = true;
        }
        chains.push(c);
    }
    (chains, nodes)
}

/// Join the chains at the meetings: the branches the thinning made are
/// dropped, two chains going straight through a meeting are one, and a
/// branch ends on the chain it meets. Which meetings a chain goes
/// through.
fn join(chains: &mut [Chain], nodes: &[Point], wide: f64) {
    for c in chains.iter_mut() {
        let len = c.length();
        let spur = matches!(c.ends, [Some(_), None] | [None, Some(_)]) && len < SPUR * wide;
        let knot = c.ends[0].is_some() && c.ends[0] == c.ends[1] && len < 2.0 * SPUR * wide;
        if spur || knot {
            c.alive = false;
        }
    }
    let mut through = vec![false; nodes.len()];
    for (n, at) in nodes.iter().enumerate() {
        loop {
            let mut halves: Vec<(usize, usize)> = Vec::new();
            for (e, c) in chains.iter().enumerate() {
                if !c.alive || c.closed {
                    continue;
                }
                for side in 0..2 {
                    if c.ends[side] == Some(n) {
                        halves.push((e, side));
                    }
                }
            }
            if halves.len() < 2 {
                if let [(e, side)] = halves[..] {
                    if !through[n] {
                        chains[e].ends[side] = None;
                    }
                }
                break;
            }
            // The way each leaves the meeting.
            let way = |e: usize, side: usize| -> Vec2 {
                let c = &chains[e];
                let mut pts = c.pts.clone();
                if side == 1 {
                    pts.reverse();
                }
                unit(pts[index_at(&pts, wide)] - *at)
            };
            let pair = if halves.len() == 2 {
                Some((0, 1, false))
            } else {
                let mut best: Option<(usize, usize, f64)> = None;
                for i in 0..halves.len() {
                    for j in i + 1..halves.len() {
                        let (a, b) = (way(halves[i].0, halves[i].1), way(halves[j].0, halves[j].1));
                        let turn = (-a).cross(b).atan2((-a).dot(b)).abs();
                        if turn < THROUGH && best.is_none_or(|x| turn < x.2) {
                            best = Some((i, j, turn));
                        }
                    }
                }
                best.map(|(i, j, _)| (i, j, true))
            };
            let Some((i, j, flat)) = pair else { break };
            let ((e1, s1), (e2, s2)) = (halves[i], halves[j]);
            if e1 == e2 {
                let c = &mut chains[e1];
                c.pts.pop();
                c.closed = true;
                c.ends = [None, None];
                break;
            }
            let mut a = chains[e1].pts.clone();
            if s1 == 0 {
                a.reverse();
            }
            let mut b = chains[e2].pts.clone();
            if s2 == 1 {
                b.reverse();
            }
            let ends = [chains[e1].ends[1 - s1], chains[e2].ends[1 - s2]];
            let middle = a.len() - 1;
            a.extend_from_slice(&b[1..]);
            if flat {
                // The thinning pulls the line towards the branch: it is
                // straight across the meeting.
                through[n] = true;
                let mut back: Vec<Point> = a[..=middle].to_vec();
                back.reverse();
                let lo = middle - index_at(&back, wide);
                let hi = middle + index_at(&a[middle..], wide);
                if hi > lo + 1 {
                    let (p, q) = (a[lo], a[hi]);
                    for k in lo + 1..hi {
                        a[k] = p.lerp(q, (k - lo) as f64 / (hi - lo) as f64);
                    }
                }
            }
            chains[e1].pts = a;
            chains[e1].ends = ends;
            chains[e2].alive = false;
            if !flat {
                break;
            }
        }
    }
    // A branch ends straight on the chain it meets.
    for c in chains.iter_mut() {
        if !c.alive || c.closed {
            continue;
        }
        // A bar between two such chains (the bar of an H) is pulled
        // at both ends: where its middle half is straight, it is that
        // line from one to the other.
        if let [Some(n0), Some(n1)] = c.ends {
            let len = c.length();
            if through[n0] && through[n1] && len > wide {
                let (i, m, j) = (
                    index_at(&c.pts, 0.25 * len),
                    index_at(&c.pts, 0.5 * len),
                    index_at(&c.pts, 0.75 * len),
                );
                let d = unit(c.pts[j] - c.pts[i]);
                let straight = c.pts[i..=j]
                    .iter()
                    .all(|p| (*p - c.pts[i]).cross(d).abs() <= 0.25 * wide);
                if d != Vec2::ZERO && straight {
                    let mid = c.pts[m];
                    let (t0, t1) = ((nodes[n0] - mid).dot(d), (nodes[n1] - mid).dot(d));
                    if t0 < 0.0 && t1 > 0.0 {
                        c.pts = vec![mid + d * t0, mid, mid + d * t1];
                        continue;
                    }
                }
            }
        }
        for side in 0..2 {
            let Some(n) = c.ends[side] else { continue };
            if !through[n] {
                continue;
            }
            if side == 0 {
                c.pts.reverse();
            }
            let len = c.length();
            if len > 0.75 * wide + 2.0 {
                let mut back = c.pts.clone();
                back.reverse();
                let near = index_at(&back, 0.75 * wide);
                let far = index_at(&back, (1.75 * wide).min(len));
                let d = unit(back[near] - back[far]);
                let t = (nodes[n] - back[near]).dot(d);
                if d != Vec2::ZERO && t > 0.0 {
                    let keep = c.pts.len() - near;
                    c.pts.truncate(keep);
                    c.pts.push(back[near] + d * t);
                }
            }
            if side == 0 {
                c.pts.reverse();
            }
        }
    }
}

/// How far the letter goes on past each free end of a chain: the
/// thinning stops short of the end by about half the stroke.
fn past_ends(c: &Chain, inside: &[f32], w: usize, h: usize) -> [f64; 2] {
    let at = |p: Point| -> f64 {
        if p.x >= 0.0 && p.y >= 0.0 && (p.x as usize) < w && (p.y as usize) < h {
            (inside[p.y as usize * w + p.x as usize] as f64 - 0.5).max(0.0)
        } else {
            0.0
        }
    };
    let n = c.pts.len();
    [
        if c.ends[0].is_none() && !c.closed && n > 0 {
            at(c.pts[0])
        } else {
            0.0
        },
        if c.ends[1].is_none() && !c.closed && n > 0 {
            at(c.pts[n - 1])
        } else {
            0.0
        },
    ]
}

/// A fitted middle line ready to be drawn: its points, the corners
/// that take a point (where, the way in, the way out) and which of its
/// ends are cut square.
struct Stroke {
    line: Vec<Point>,
    corners: Vec<(Point, Vec2, Vec2)>,
    /// The square ends: where, and the way the stroke was going.
    cuts: Vec<(Point, Vec2)>,
    /// Half its width.
    half: f64,
}

/// The fitted `path` of a chain, carried `past` its two ends along its
/// own way there, with its free ends (`square`) cut square.
fn stroke_of(
    path: &BezPath,
    closed: bool,
    square: [bool; 2],
    past: [f64; 2],
    wide: f64,
    reach: &dyn Fn(Point, Vec2, f64) -> f64,
) -> Stroke {
    use tracedraw_core::geometry::{Line, PathSeg};
    let mut line: Vec<Point> = Vec::new();
    let mut corners = Vec::new();
    let mut ways: Vec<(Point, Vec2, Vec2)> = Vec::new();
    // A short piece between two lines that turn is the thinning's way
    // round a corner: the two lines meet where they cross.
    let mut segs: Vec<PathSeg> = path.segments().collect();
    let mut i = 1;
    while i + 1 < segs.len() {
        if let (PathSeg::Line(a), PathSeg::Line(b)) = (segs[i - 1], segs[i + 1]) {
            let (da, db) = (unit(a.p1 - a.p0), unit(b.p1 - b.p0));
            let cross = da.cross(db);
            let short = (segs[i].eval(1.0) - segs[i].eval(0.0)).hypot() <= CHAMFER * wide;
            if short && cross.abs() > MITER_LEAST.sin() {
                let t = (b.p0 - a.p0).cross(db) / cross;
                let at = a.p0 + da * t;
                if (at - segs[i].eval(0.5)).hypot() <= CHAMFER * wide {
                    segs[i - 1] = PathSeg::Line(Line::new(a.p0, at));
                    segs[i + 1] = PathSeg::Line(Line::new(at, b.p1));
                    segs.remove(i);
                    continue;
                }
            }
        }
        i += 1;
    }
    for seg in segs {
        let (from, to) = (seg.eval(0.0), seg.eval(1.0));
        if (to - from).hypot() < 1e-9 {
            continue;
        }
        let steps = match seg {
            tracedraw_core::geometry::PathSeg::Line(_) => 1,
            _ => 16,
        };
        if line.is_empty() {
            line.push(from);
        }
        for i in 1..=steps {
            line.push(seg.eval(i as f64 / steps as f64));
        }
        ways.push((from, unit(seg.eval(0.03) - from), unit(to - seg.eval(0.97))));
    }
    let n = ways.len();
    for i in 0..n {
        let next = if i + 1 < n {
            i + 1
        } else if closed {
            0
        } else {
            break;
        };
        let (a, b) = (ways[i].2, ways[next].1);
        let turn = a.cross(b).atan2(a.dot(b)).abs();
        if (MITER_LEAST..=MITER_MOST).contains(&turn) {
            corners.push((ways[next].0, a, b));
        }
    }
    let mut cuts = Vec::new();
    if !closed && n > 0 && line.len() >= 2 {
        let (first, last) = (-ways[0].1, ways[n - 1].2);
        if square[0] && first != Vec2::ZERO {
            let far = reach(line[0], first, 2.0 * past[0] + 4.0);
            line.insert(0, line[0] + first * far);
            cuts.push((line[0], first));
        }
        if square[1] && last != Vec2::ZERO {
            let from = line[line.len() - 1];
            let end = from + last * reach(from, last, 2.0 * past[1] + 4.0);
            line.push(end);
            cuts.push((end, last));
        }
    }
    Stroke {
        line,
        corners,
        cuts,
        half: wide / 2.0,
    }
}

impl Stroke {
    /// Whether `q` is within `half` of the line: inside the stroke.
    fn holds(&self, q: Point) -> bool {
        let half = self.half;
        let n = self.line.len();
        // Past a square end, beside it, is outside.
        for (end, way) in &self.cuts {
            let p = q - *end;
            if p.dot(*way) > 0.0 && p.hypot2() <= 2.25 * half * half {
                return false;
            }
        }
        for i in 1..n {
            let (a, b) = (self.line[i - 1], self.line[i]);
            let ab = b - a;
            let len2 = ab.hypot2();
            if len2 < 1e-18 {
                continue;
            }
            let t = (q - a).dot(ab) / len2;
            let near = a + ab * t.clamp(0.0, 1.0);
            if (q - near).hypot2() <= half * half {
                return true;
            }
        }
        // The point of a corner: between its two outer sides.
        for (v, a, b) in &self.corners {
            let s = if a.cross(*b) > 0.0 { 1.0 } else { -1.0 };
            let (n1, n2) = (Vec2::new(a.y, -a.x) * s, Vec2::new(b.y, -b.x) * s);
            let p = q - *v;
            if p.dot(n1) <= half
                && p.dot(n2) <= half
                && p.dot(n1) >= 0.0
                && p.dot(n2) >= 0.0
                && p.dot(*a) >= -1e-9
                && p.dot(*b) <= 1e-9
            {
                return true;
            }
        }
        false
    }
}

/// One letter: `mask` (`w` x `h`, with room round it) drawn again by
/// its strokes, or none when it is not such a letter or the two do not
/// agree.
fn letter(mask: &[bool], w: usize, h: usize, unit_px: f64, f: &Faithful) -> Option<Vec<bool>> {
    let area = mask.iter().filter(|m| **m).count();
    let inside = distances(mask, w, h);
    let mut thin = mask.to_vec();
    if !crate::trace::thin_unless(&mut thin, w, h, None) {
        return None;
    }
    let mut on: Vec<f32> = (0..w * h).filter(|i| thin[*i]).map(|i| inside[i]).collect();
    if on.len() < 4 {
        return None;
    }
    on.sort_by(|a, b| a.total_cmp(b));
    let middle = on[on.len() / 2] as f64;
    let wide = 2.0 * middle - 1.0;
    if !(WIDE_LEAST * unit_px..=WIDE_MOST * unit_px).contains(&wide) {
        return None;
    }
    let even = on
        .iter()
        .filter(|d| (**d as f64 - middle).abs() <= EVEN * middle + 1.0)
        .count();
    if (even as f64) < EVEN_SHARE * on.len() as f64 || (on.len() as f64) < 1.5 * wide {
        return None;
    }
    let (mut chains, nodes) = chains_of(&thin, w, h);
    join(&mut chains, &nodes, wide);
    let fit = Faithful {
        k: f.k,
        tolerance: FIT_TOLERANCE * unit_px / f.k,
        corner: f.corner,
        scale: f.scale,
    };
    // How far the letter goes on from `p` along `d`, `most` at most.
    let reach = |p: Point, d: Vec2, most: f64| -> f64 {
        let held = |q: Point| {
            q.x >= 0.0
                && q.y >= 0.0
                && (q.x as usize) < w
                && (q.y as usize) < h
                && mask[q.y as usize * w + q.x as usize]
        };
        let mut t = 0.0;
        while t < most && held(p + d * (t + 0.25)) {
            t += 0.25;
        }
        t
    };
    let mut strokes = Vec::new();
    for c in chains.iter().filter(|c| c.alive) {
        let mut past = past_ends(c, &inside, w, h);
        // A stroke long enough to say so has its own width (the bar
        // of a T thinner than its stem).
        let mut on: Vec<f32> = c
            .pts
            .iter()
            .filter(|p| p.x >= 0.0 && p.y >= 0.0 && (p.x as usize) < w && (p.y as usize) < h)
            .map(|p| inside[p.y as usize * w + p.x as usize])
            .collect();
        on.sort_by(|a, b| a.total_cmp(b));
        let mut own = wide;
        if c.length() >= 2.0 * wide && !on.is_empty() {
            let mine = 2.0 * on[on.len() / 2] as f64 - 1.0;
            if (mine - wide).abs() > OWN_LEAST * wide {
                own = mine.clamp((1.0 - OWN_MOST) * wide, (1.0 + OWN_MOST) * wide);
            }
        }
        // The tip of a thinned line veers to a corner of its end: it
        // is left out, and the line carried on as far.
        let mut pts = c.pts.clone();
        if !c.closed && c.length() >= 3.0 * wide {
            if c.ends[1].is_none() {
                let mut back = pts.clone();
                back.reverse();
                let cut = index_at(&back, TIP * wide);
                let keep = pts.len() - cut;
                past[1] += (pts[pts.len() - 1] - pts[keep - 1]).hypot();
                pts.truncate(keep);
            }
            if c.ends[0].is_none() {
                let cut = index_at(&pts, TIP * wide);
                past[0] += (pts[0] - pts[cut]).hypot();
                pts.drain(..cut);
            }
        }
        let pts = resample(&pts, 1.0, c.closed);
        if pts.len() < 2 {
            continue;
        }
        let path = crate::trace_fit::fit(&pts, c.closed, &fit, None).path;
        let s = stroke_of(
            &path,
            c.closed,
            [c.ends[0].is_none(), c.ends[1].is_none()],
            past,
            own,
            &reach,
        );
        if s.line.len() >= 2 {
            strokes.push(s);
        }
    }
    if strokes.is_empty() {
        return None;
    }
    let mut drawn = vec![false; w * h];
    for (i, d) in drawn.iter_mut().enumerate() {
        let q = Point::new((i % w) as f64 + 0.5, (i / w) as f64 + 0.5);
        *d = strokes.iter().any(|s| s.holds(q));
    }
    // The check: the letter drawn again against the picture's.
    let outside = distances(&mask.iter().map(|m| !*m).collect::<Vec<_>>(), w, h);
    let most = (STRAY_MOST * unit_px).min(STRAY_WIDE * wide) as f32;
    let mut off = 0usize;
    for i in 0..w * h {
        if drawn[i] != mask[i] {
            let d = if mask[i] { inside[i] } else { outside[i] };
            if d > most {
                off += 1;
            }
        }
        // Nothing drawn on the rim of the box.
        let (x, y) = (i % w, i / w);
        if drawn[i] && (x == 0 || y == 0 || x + 1 == w || y + 1 == h) {
            return None;
        }
    }
    if off as f64 > STRAY_SHARE * area as f64 || grounds(&drawn, w, h) != grounds(mask, w, h) {
        return None;
    }
    Some(drawn)
}

/// Draw the small thin letters of `labels` (`w` x `h`, `unit_px` points
/// of it to a pixel of the picture before it was enlarged) again by
/// their strokes. Gives the points whose borders are drawn already, to
/// be fitted as they are and not placed by the picture again.
pub(crate) fn redraw(
    labels: &mut [usize],
    w: usize,
    h: usize,
    unit_px: f64,
    f: &Faithful,
    palette: &[[u8; 3]],
    cancel: Option<&crate::trace::Cancel>,
) -> Option<Vec<bool>> {
    let mut redrawn = vec![false; w * h];
    if w * h == 0 || !unit_px.is_finite() || unit_px < 2.0 {
        return Some(redrawn);
    }
    let most = (BOX_MOST * unit_px) as usize;
    let least = (unit_px * unit_px * 2.0) as usize;
    let pad = unit_px.ceil() as usize + 3;
    let mut seen = vec![false; w * h];
    let (mut stack, mut cells) = (Vec::new(), Vec::new());
    let mut changes: Vec<(usize, usize)> = Vec::new();
    for start in 0..w * h {
        if seen[start] || labels[start] == usize::MAX {
            continue;
        }
        if start % (w * 64) == 0 && crate::trace::cancelled(cancel) {
            return None;
        }
        let label = labels[start];
        // A letter in two shades of its colour is one letter.
        let alike = |l: usize| -> bool {
            l == label
                || match (palette.get(l), palette.get(label)) {
                    (Some(a), Some(b)) => {
                        let d = |i: usize| (a[i] as i64 - b[i] as i64).pow(2);
                        d(0) + d(1) + d(2) <= SHADE_NEAR * SHADE_NEAR
                    }
                    _ => false,
                }
        };
        cells.clear();
        seen[start] = true;
        stack.push(start);
        let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0usize, 0usize);
        let mut small = true;
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            if small {
                if x1 - x0 > most || y1 - y0 > most {
                    small = false;
                    cells.clear();
                } else {
                    cells.push(i);
                }
            }
            let mut visit = |j: usize| {
                if !seen[j] && alike(labels[j]) {
                    seen[j] = true;
                    stack.push(j);
                }
            };
            if x > 0 {
                visit(i - 1);
            }
            if x + 1 < w {
                visit(i + 1);
            }
            if y > 0 {
                visit(i - w);
            }
            if y + 1 < h {
                visit(i + w);
            }
        }
        if !small || cells.len() < least || x0 < pad || y0 < pad || x1 + pad >= w || y1 + pad >= h {
            continue;
        }
        let (bx, by) = (x0 - pad, y0 - pad);
        let (lw, lh) = (x1 - x0 + 1 + 2 * pad, y1 - y0 + 1 + 2 * pad);
        let mut mask = vec![false; lw * lh];
        for i in &cells {
            mask[(i / w - by) * lw + (i % w - bx)] = true;
        }
        let Some(drawn) = letter(&mask, lw, lh, unit_px, f) else {
            continue;
        };
        // The shade most of it has.
        let mut shades: Vec<(usize, usize)> = Vec::new();
        for i in &cells {
            match shades.iter_mut().find(|s| s.0 == labels[*i]) {
                Some(s) => s.1 += 1,
                None => shades.push((labels[*i], 1)),
            }
        }
        let label = shades.iter().max_by_key(|s| s.1).map_or(label, |s| s.0);
        // What the letter no longer covers takes the nearest label
        // round it.
        let mut near: Vec<usize> = (0..lw * lh)
            .map(|i| {
                if mask[i] {
                    usize::MAX - 1
                } else {
                    labels[(by + i / lw) * w + bx + i % lw]
                }
            })
            .collect();
        let mut queue: std::collections::VecDeque<usize> =
            (0..lw * lh).filter(|i| !mask[*i]).collect();
        while let Some(i) = queue.pop_front() {
            let (x, y) = (i % lw, i / lw);
            let l = near[i];
            let mut visit = |j: usize| {
                if near[j] == usize::MAX - 1 {
                    near[j] = l;
                    queue.push_back(j);
                }
            };
            if x > 0 {
                visit(i - 1);
            }
            if x + 1 < lw {
                visit(i + 1);
            }
            if y > 0 {
                visit(i - lw);
            }
            if y + 1 < lh {
                visit(i + lw);
            }
        }
        for i in 0..lw * lh {
            let at = (by + i / lw) * w + bx + i % lw;
            redrawn[at] = true;
            if drawn[i] && labels[at] != label {
                changes.push((at, label));
            } else if !drawn[i] && mask[i] && near[i] != usize::MAX - 1 {
                changes.push((at, near[i]));
            }
        }
    }
    for (at, l) in changes {
        labels[at] = l;
    }
    Some(redrawn)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIDE: usize = 160;

    fn faithful() -> Faithful {
        Faithful {
            k: 2.0,
            tolerance: 0.85,
            corner: 1.0,
            scale: 4.0,
        }
    }

    /// An H of strokes 16 wide as an enlarging leaves it: its sides
    /// wavering by a point, the corners of its ends worn round.
    fn soft_h(x: usize, y: usize) -> bool {
        let (x, y) = (x as f64 + 0.5, y as f64 + 0.5);
        let waver = (1.2 * (y / 5.0).sin()).round();
        let stem = |x0: f64| {
            let inside = x >= x0 + waver && x < x0 + 16.0 + waver && (40.0..104.0).contains(&y);
            // The ends: half discs.
            let end = |cy: f64| (x - x0 - 8.0).hypot((y - cy) * 1.6) < 8.0;
            inside && ((46.0..98.0).contains(&y) || end(46.0) || end(98.0))
        };
        let bar = (48.0..96.0).contains(&x) && (64.0..80.0).contains(&y);
        stem(40.0) || stem(88.0) || bar
    }

    fn row(labels: &[usize], y: usize) -> Vec<usize> {
        (0..SIDE).filter(|x| labels[y * SIDE + x] == 1).collect()
    }

    /// The letter drawn again has stems with straight upright sides, all
    /// of one width, and ends cut square at the height the letter had.
    #[test]
    fn a_soft_letter_comes_back_with_straight_strokes_and_square_ends() {
        let mut labels: Vec<usize> = (0..SIDE * SIDE)
            .map(|i| soft_h(i % SIDE, i / SIDE) as usize)
            .collect();
        let palette = [[255, 255, 255], [0, 0, 0]];
        let redrawn =
            redraw(&mut labels, SIDE, SIDE, 8.0, &faithful(), &palette, None).expect("not stopped");
        assert!(redrawn.iter().any(|r| *r), "the letter was not drawn again");
        // The left stem above the bar: the same run of points on
        // every row.
        let first = row(&labels, 44);
        let left: Vec<usize> = first.iter().copied().filter(|x| *x < 70).collect();
        assert!((14..=18).contains(&left.len()), "{} wide", left.len());
        for y in 41..62 {
            let here: Vec<usize> = row(&labels, y).into_iter().filter(|x| *x < 70).collect();
            assert_eq!(here, left, "row {y}");
        }
        // Its end is square: the top row is as wide as the stem, and
        // within a point and a half of where the letter's top was.
        let top = (0..SIDE)
            .find(|y| !row(&labels, *y).is_empty())
            .expect("a letter");
        assert!((39..=42).contains(&top), "top at {top}");
        let across: Vec<usize> = row(&labels, top).into_iter().filter(|x| *x < 70).collect();
        assert_eq!(across, left, "the end is not square");
        // The bar joins the stems: one piece.
        assert!((56..88).all(|x| labels[72 * SIDE + x] == 1));
    }

    /// A shape that is not a thin stroke (a disc, a wide block) is left
    /// as it is.
    #[test]
    fn a_shape_that_is_no_stroke_is_left_alone() {
        let shape = |x: usize, y: usize| {
            let (dx, dy) = (x as f64 - 50.0, y as f64 - 50.0);
            dx * dx + dy * dy < 30.0 * 30.0 || ((90..150).contains(&x) && (90..130).contains(&y))
        };
        let before: Vec<usize> = (0..SIDE * SIDE)
            .map(|i| shape(i % SIDE, i / SIDE) as usize)
            .collect();
        let mut labels = before.clone();
        let palette = [[255, 255, 255], [0, 0, 0]];
        let redrawn =
            redraw(&mut labels, SIDE, SIDE, 8.0, &faithful(), &palette, None).expect("not stopped");
        assert_eq!(labels, before);
        assert!(redrawn.iter().all(|r| !*r));
    }

    /// A letter in two shades of its colour is drawn again as one
    /// letter, in the shade most of it has; a part in another colour
    /// altogether is not taken for part of it.
    #[test]
    fn a_letter_in_two_shades_is_drawn_as_one() {
        let make = |other: usize| -> Vec<usize> {
            (0..SIDE * SIDE)
                .map(|i| {
                    let (x, y) = (i % SIDE, i / SIDE);
                    if !soft_h(x, y) {
                        0
                    } else if x >= 88 && y > 84 {
                        other
                    } else {
                        1
                    }
                })
                .collect()
        };
        let palette = [[255, 255, 255], [20, 20, 20], [40, 30, 30], [220, 40, 40]];
        let mut labels = make(2);
        redraw(&mut labels, SIDE, SIDE, 8.0, &faithful(), &palette, None).expect("not stopped");
        assert!(labels.iter().all(|l| *l != 2), "the second shade is left");
        assert!(labels[95 * SIDE + 96] == 1, "the foot of the stem is gone");
        let mut labels = make(3);
        redraw(&mut labels, SIDE, SIDE, 8.0, &faithful(), &palette, None).expect("not stopped");
        assert!(labels[95 * SIDE + 96] == 3, "another colour was taken over");
    }

    /// The middle line of a T is its bar and its stem, the stem ending
    /// on the bar; the branches the thinning grows into the corners of
    /// the square ends are dropped.
    #[test]
    fn the_middle_line_of_a_t_is_two_strokes() {
        let (w, h) = (100usize, 100usize);
        let mask: Vec<bool> = (0..w * h)
            .map(|i| {
                let (x, y) = (i % w, i / w);
                ((20..80).contains(&x) && (20..32).contains(&y))
                    || ((44..56).contains(&x) && (20..84).contains(&y))
            })
            .collect();
        let mut thin = mask.clone();
        assert!(crate::trace::thin_unless(&mut thin, w, h, None));
        let (mut chains, nodes) = chains_of(&thin, w, h);
        join(&mut chains, &nodes, 12.0);
        let alive: Vec<&Chain> = chains.iter().filter(|c| c.alive).collect();
        assert_eq!(alive.len(), 2, "{} strokes", alive.len());
        let bar = alive
            .iter()
            .find(|c| c.ends == [None, None])
            .expect("a bar with two free ends");
        assert!(
            bar.pts.iter().all(|p| (p.y - 26.0).abs() < 2.5),
            "{:?}",
            bar.pts
        );
        let stem = alive
            .iter()
            .find(|c| c.ends != [None, None])
            .expect("a stem ending on the bar");
        assert!(
            stem.pts.iter().all(|p| (p.x - 50.0).abs() < 2.5),
            "{:?}",
            stem.pts
        );
    }
}
