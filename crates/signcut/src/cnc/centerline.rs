//! An approximate centre line (medial axis) of closed shapes, for
//! engraving with a single line and for neon.
//!
//! The outline is sampled densely, the samples are triangulated
//! (Delaunay, Bowyer-Watson with a sweep), and the triangles inside the
//! shape are joined across their shared sides: each triangle stands for
//! the centre of its circumscribed circle, the Voronoi vertex of its
//! three samples, which lies on the medial axis as the samples get
//! dense. Short side branches (towards corners) are pruned, the rest is
//! split into lines between ends and junctions and simplified.

use super::{contains_any, flatten, simplify, Contour};
use std::collections::{HashMap, HashSet};
use tracedraw_core::geometry::{BezPath, Point};

/// How fine and how clean the centre line is.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct CenterSpec {
    /// Simplification tolerance and the finest sample spacing (mm).
    pub tolerance: f64,
    /// Side branches shorter than this are removed (mm); so are those
    /// shorter than 1.5 times the half width where they branch off.
    pub prune: f64,
}

impl Default for CenterSpec {
    fn default() -> Self {
        CenterSpec {
            tolerance: 0.2,
            prune: 2.0,
        }
    }
}

/// Most samples per shape: the spacing grows for large shapes.
pub const MAX_SAMPLES: f64 = 6000.0;

#[derive(Clone, Copy)]
struct Tri {
    v: [usize; 3],
    cx: f64,
    cy: f64,
    r2: f64,
}

fn tri(pts: &[Point], v: [usize; 3]) -> Tri {
    let (a, b, c) = (pts[v[0]], pts[v[1]], pts[v[2]]);
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if d.abs() < 1e-18 || !d.is_finite() {
        // Flat: replaced by the next point inserted anywhere near.
        return Tri {
            v,
            cx: (a.x + b.x + c.x) / 3.0,
            cy: (a.y + b.y + c.y) / 3.0,
            r2: f64::INFINITY,
        };
    }
    let (a2, b2, c2) = (
        a.to_vec2().hypot2(),
        b.to_vec2().hypot2(),
        c.to_vec2().hypot2(),
    );
    let cx = (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d;
    let cy = (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d;
    let r2 = (a.x - cx).powi(2) + (a.y - cy).powi(2);
    Tri { v, cx, cy, r2 }
}

/// Delaunay triangles of `pts` (indices into it).
fn delaunay(pts: &[Point]) -> Vec<[usize; 3]> {
    let n = pts.len();
    if n < 3 {
        return Vec::new();
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in pts {
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    let size = (x1 - x0).max(y1 - y0).max(1e-6);
    let (mx, my) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let mut all: Vec<Point> = pts.to_vec();
    all.push(Point::new(mx - 20.0 * size, my - 10.0 * size));
    all.push(Point::new(mx + 20.0 * size, my - 10.0 * size));
    all.push(Point::new(mx, my + 20.0 * size));
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| {
        all[a]
            .x
            .total_cmp(&all[b].x)
            .then(all[a].y.total_cmp(&all[b].y))
    });
    let mut open = vec![tri(&all, [n, n + 1, n + 2])];
    let mut done: Vec<Tri> = Vec::new();
    let mut edges: HashMap<(usize, usize), (usize, usize, u8)> = HashMap::new();
    for &ip in &order {
        let p = all[ip];
        let mut keep = Vec::with_capacity(open.len() + 2);
        edges.clear();
        for t in open.drain(..) {
            let dx = p.x - t.cx;
            if dx > 0.0 && dx * dx > t.r2 {
                done.push(t);
                continue;
            }
            let d2 = dx * dx + (p.y - t.cy).powi(2);
            if d2 < t.r2 {
                for (a, b) in [(t.v[0], t.v[1]), (t.v[1], t.v[2]), (t.v[2], t.v[0])] {
                    let e = edges.entry((a.min(b), a.max(b))).or_insert((a, b, 0));
                    e.2 += 1;
                }
            } else {
                keep.push(t);
            }
        }
        open = keep;
        for (a, b, k) in edges.values() {
            if *k == 1 {
                open.push(tri(&all, [*a, *b, ip]));
            }
        }
    }
    done.extend(open);
    done.into_iter()
        .map(|t| t.v)
        .filter(|v| v.iter().all(|&i| i < n))
        .collect()
}

/// A small, repeatable shift of sample `i` (breaks up points lying on
/// one circle, as on a rectangle).
fn jitter(i: usize, amount: f64) -> (f64, f64) {
    let mut h = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03;
    h ^= h >> 29;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 32;
    let a = (h & 0xFFFF) as f64 / 65535.0 - 0.5;
    let b = ((h >> 16) & 0xFFFF) as f64 / 65535.0 - 0.5;
    (a * amount, b * amount)
}

/// The centre lines of the closed subpaths of `path` (one shape, holes
/// by even-odd). Each line is its points and whether it closes (round a
/// hole).
pub fn center_lines(path: &BezPath, spec: &CenterSpec) -> Vec<(Vec<Point>, bool)> {
    let tol = if spec.tolerance.is_finite() && spec.tolerance > 0.0 {
        spec.tolerance.max(0.001)
    } else {
        0.2
    };
    let prune = if spec.prune.is_finite() {
        spec.prune.max(0.0)
    } else {
        0.0
    };
    let contours: Vec<Contour> = flatten(path, (tol / 4.0).max(0.005))
        .into_iter()
        .filter(|c| c.closed)
        .collect();
    let perimeter: f64 = contours.iter().map(|c| c.length()).sum();
    if contours.is_empty() || !perimeter.is_finite() || perimeter <= 0.0 {
        return Vec::new();
    }
    let step = tol.max(perimeter / MAX_SAMPLES);
    // Samples, with their contour and place in it.
    let mut pts: Vec<Point> = Vec::new();
    let mut place: Vec<(usize, usize)> = Vec::new();
    let mut counts: Vec<usize> = Vec::new();
    for (ci, c) in contours.iter().enumerate() {
        let m = c.pts.len();
        let mut k = 0usize;
        for i in 0..m {
            let a = c.pts[i];
            let b = c.pts[(i + 1) % m];
            let parts = ((b - a).hypot() / step).ceil().clamp(1.0, 100_000.0) as usize;
            for j in 0..parts {
                let (jx, jy) = jitter(pts.len(), step * 1e-3);
                let q = a.lerp(b, j as f64 / parts as f64);
                pts.push(Point::new(q.x + jx, q.y + jy));
                place.push((ci, k));
                k += 1;
            }
        }
        counts.push(k);
    }
    let tris = delaunay(&pts);
    // Triangles inside the shape, but not slivers: three samples
    // following each other along the outline, or lying along one side.
    let mut nodes: Vec<(Point, f64)> = Vec::new();
    let mut inside: Vec<[usize; 3]> = Vec::new();
    for v in tris {
        let (a, b, c) = (pts[v[0]], pts[v[1]], pts[v[2]]);
        let g = Point::new((a.x + b.x + c.x) / 3.0, (a.y + b.y + c.y) / 3.0);
        if !contains_any(&contours, g) {
            continue;
        }
        if m3_consecutive(&place, &counts, v) {
            continue;
        }
        // Slivers of samples along one straight side.
        let longest = (a - b).hypot().max((b - c).hypot()).max((c - a).hypot());
        let twice_area = ((b - a).cross(c - a)).abs();
        let height = twice_area / longest;
        if height.is_nan() || height < 0.05 * step {
            continue;
        }
        let t = tri(&pts, v);
        let cc = Point::new(t.cx, t.cy);
        let pos = if t.r2.is_finite() && (cc - g).hypot() <= longest && contains_any(&contours, cc)
        {
            cc
        } else {
            g
        };
        let r = (pos - a)
            .hypot()
            .min((pos - b).hypot())
            .min((pos - c).hypot());
        nodes.push((pos, r));
        inside.push(v);
    }
    // Join triangles sharing a side.
    let mut by_edge: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
    for (i, v) in inside.iter().enumerate() {
        for (a, b) in [(v[0], v[1]), (v[1], v[2]), (v[2], v[0])] {
            by_edge.entry((a.min(b), a.max(b))).or_default().push(i);
        }
    }
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
    for list in by_edge.values() {
        if let [x, y] = list[..] {
            adj[x].push(y);
            adj[y].push(x);
        }
    }
    prune_branches(&nodes, &mut adj, prune);
    let lines = trace(&nodes, &adj);
    lines
        .into_iter()
        .filter_map(|(line, closed)| {
            let s = simplify(&line, tol);
            let len = super::polyline_length(&s, false);
            if len < tol.max(prune) {
                return None;
            }
            if closed {
                let mut s = s;
                if s.len() > 3 {
                    s.pop();
                    return Some((s, true));
                }
                return None;
            }
            Some((s, false))
        })
        .collect()
}

/// Whether the three samples follow each other along one outline.
fn m3_consecutive(place: &[(usize, usize)], counts: &[usize], v: [usize; 3]) -> bool {
    let c = place[v[0]].0;
    if place[v[1]].0 != c || place[v[2]].0 != c {
        return false;
    }
    let m = counts.get(c).copied().unwrap_or(1).max(1);
    let mut k: Vec<usize> = v.iter().map(|&i| place[i].1).collect();
    k.sort_unstable();
    let gaps = [k[1] - k[0], k[2] - k[1], m + k[0] - k[2]];
    gaps.iter().filter(|&&g| g == 1).count() >= 2
}

fn dist(nodes: &[(Point, f64)], a: usize, b: usize) -> f64 {
    (nodes[a].0 - nodes[b].0).hypot()
}

/// Remove side branches: a run from a free end to a junction shorter
/// than `prune` or 1.5 times the half width at the junction. A
/// junction always keeps one branch.
fn prune_branches(nodes: &[(Point, f64)], adj: &mut [Vec<usize>], prune: f64) {
    for _ in 0..200 {
        // (nodes of the run, junction, length)
        let mut runs: Vec<(Vec<usize>, usize, f64)> = Vec::new();
        for leaf in 0..adj.len() {
            if adj[leaf].len() != 1 {
                continue;
            }
            let mut run = vec![leaf];
            let mut prev = leaf;
            let mut cur = adj[leaf][0];
            let mut len = dist(nodes, prev, cur);
            while adj[cur].len() == 2 && cur != leaf {
                let nxt = if adj[cur][0] == prev {
                    adj[cur][1]
                } else {
                    adj[cur][0]
                };
                run.push(cur);
                len += dist(nodes, cur, nxt);
                prev = cur;
                cur = nxt;
            }
            if adj[cur].len() >= 3 {
                let limit = prune.max(1.5 * nodes[cur].1);
                if len < limit {
                    runs.push((run, cur, len));
                }
            }
        }
        if runs.is_empty() {
            return;
        }
        // A junction keeps at least one branch: its longest candidate
        // stays when every branch would go.
        let mut at: HashMap<usize, Vec<usize>> = HashMap::new();
        for (i, r) in runs.iter().enumerate() {
            at.entry(r.1).or_default().push(i);
        }
        let mut skip: HashSet<usize> = HashSet::new();
        for (j, list) in &at {
            if list.len() >= adj[*j].len() {
                if let Some(&longest) = list
                    .iter()
                    .max_by(|&&a, &&b| runs[a].2.total_cmp(&runs[b].2))
                {
                    skip.insert(longest);
                }
            }
        }
        let mut removed = false;
        for (i, (run, _, _)) in runs.iter().enumerate() {
            if skip.contains(&i) {
                continue;
            }
            for &n in run {
                for m in std::mem::take(&mut adj[n]) {
                    adj[m].retain(|&x| x != n);
                }
                removed = true;
            }
        }
        if !removed {
            return;
        }
    }
}

/// Split the graph into lines between ends and junctions, and loops.
fn trace(nodes: &[(Point, f64)], adj: &[Vec<usize>]) -> Vec<(Vec<Point>, bool)> {
    let mut used: HashSet<(usize, usize)> = HashSet::new();
    let key = |a: usize, b: usize| (a.min(b), a.max(b));
    let mut out = Vec::new();
    let walk = |start: usize, first: usize, used: &mut HashSet<(usize, usize)>| {
        let mut line = vec![nodes[start].0];
        let mut prev = start;
        let mut cur = first;
        used.insert(key(prev, cur));
        loop {
            line.push(nodes[cur].0);
            if adj[cur].len() != 2 || cur == start {
                break;
            }
            let nxt = if adj[cur][0] == prev {
                adj[cur][1]
            } else {
                adj[cur][0]
            };
            if used.contains(&key(cur, nxt)) {
                break;
            }
            used.insert(key(cur, nxt));
            prev = cur;
            cur = nxt;
        }
        let closed = cur == start;
        (line, closed)
    };
    for (s, next) in adj.iter().enumerate() {
        if next.is_empty() || next.len() == 2 {
            continue;
        }
        for &f in next {
            if !used.contains(&key(s, f)) {
                out.push(walk(s, f, &mut used));
            }
        }
    }
    // What is left are loops of nodes with two neighbours.
    for (s, next) in adj.iter().enumerate() {
        let [f, _] = next[..] else {
            continue;
        };
        if !used.contains(&key(s, f)) {
            out.push(walk(s, f, &mut used));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::{Rect, Shape as _};

    fn length(l: &[Point], closed: bool) -> f64 {
        super::super::polyline_length(l, closed)
    }

    #[test]
    fn a_bar_has_one_line_down_its_middle() {
        let p = Rect::new(0.0, 0.0, 100.0, 10.0).to_path(0.01);
        let lines = center_lines(&p, &CenterSpec::default());
        assert_eq!(lines.len(), 1, "{lines:?}");
        let (l, closed) = &lines[0];
        assert!(!closed);
        assert!(l.iter().all(|q| (q.y - 5.0).abs() < 0.3), "{l:?}");
        // From about half the width in from each end.
        let len = length(l, false);
        assert!((len - 90.0).abs() < 1.5, "{len}");
    }

    #[test]
    fn a_ring_has_one_closed_loop() {
        let mut p = Rect::new(0.0, 0.0, 40.0, 40.0).to_path(0.01);
        p.extend(Rect::new(10.0, 10.0, 30.0, 30.0).to_path(0.01));
        let lines = center_lines(&p, &CenterSpec::default());
        let loops: Vec<_> = lines.iter().filter(|(_, c)| *c).collect();
        assert_eq!(loops.len(), 1, "{lines:?}");
        let len = length(&loops[0].0, true);
        // Half way between the outlines; round the hole's corners the
        // line keeps away from them (it meets the diagonal at x = 5.86),
        // so it is a little shorter than the square of 30.
        assert!(len > 108.0 && len < 120.5, "{len}");
        let near = |v: f64, at: f64| (v - at).abs() < 1.0;
        assert!(loops[0]
            .0
            .iter()
            .all(|q| near(q.x, 5.0) || near(q.x, 35.0) || near(q.y, 5.0) || near(q.y, 35.0)));
    }

    #[test]
    fn a_round_ring_has_its_middle_circle() {
        use tracedraw_core::geometry::Circle;
        let mut p = Circle::new((0.0, 0.0), 20.0).to_path(0.01);
        p.extend(Circle::new((0.0, 0.0), 10.0).to_path(0.01));
        let lines = center_lines(&p, &CenterSpec::default());
        assert_eq!(lines.len(), 1, "{lines:?}");
        let (l, closed) = &lines[0];
        assert!(*closed);
        assert!(l.iter().all(|q| (q.to_vec2().hypot() - 15.0).abs() < 0.3));
        let len = length(l, true);
        assert!(
            (len - 2.0 * std::f64::consts::PI * 15.0).abs() < 1.0,
            "{len}"
        );
    }

    #[test]
    fn a_long_bar_is_sampled_within_the_cap() {
        let p = Rect::new(0.0, 0.0, 1000.0, 20.0).to_path(0.01);
        let lines = center_lines(&p, &CenterSpec::default());
        assert_eq!(lines.len(), 1);
        assert!((length(&lines[0].0, false) - 980.0).abs() < 3.0);
    }

    #[test]
    fn an_l_keeps_both_arms() {
        let mut p = BezPath::new();
        p.move_to((0.0, 0.0));
        p.line_to((60.0, 0.0));
        p.line_to((60.0, 10.0));
        p.line_to((10.0, 10.0));
        p.line_to((10.0, 60.0));
        p.line_to((0.0, 60.0));
        p.close_path();
        let lines = center_lines(&p, &CenterSpec::default());
        let total: f64 = lines.iter().map(|(l, c)| length(l, *c)).sum();
        // Arms from x = 55 and y = 55 to the corner at (5, 5); the
        // diagonal to the outer corner is pruned.
        assert!(total > 95.0 && total < 115.0, "{total} {lines:?}");
        let far = |x: f64, y: f64| {
            lines
                .iter()
                .flat_map(|(l, _)| l.iter())
                .any(|q| (q.x - x).abs() < 1.0 && (q.y - y).abs() < 1.0)
        };
        assert!(far(55.0, 5.0) && far(5.0, 55.0));
    }

    #[test]
    fn bad_and_empty_input_gives_nothing() {
        assert!(center_lines(&BezPath::new(), &CenterSpec::default()).is_empty());
        let mut line = BezPath::new();
        line.move_to((0.0, 0.0));
        line.line_to((10.0, 0.0));
        assert!(center_lines(&line, &CenterSpec::default()).is_empty());
        let mut bad = BezPath::new();
        bad.move_to((f64::NAN, 0.0));
        bad.line_to((10.0, f64::INFINITY));
        bad.line_to((5.0, 5.0));
        bad.close_path();
        assert!(center_lines(&bad, &CenterSpec::default()).is_empty());
        let odd = CenterSpec {
            tolerance: f64::NAN,
            prune: f64::INFINITY,
        };
        let p = Rect::new(0.0, 0.0, 10.0, 2.0).to_path(0.01);
        // Infinite pruning leaves the longest branch at most.
        assert!(center_lines(&p, &odd).len() <= 1);
        assert!(delaunay(&[Point::ZERO, Point::new(1.0, 0.0)]).is_empty());
    }
}
