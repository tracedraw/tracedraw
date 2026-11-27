//! CNC toolpaths: G-code for routers and mills.
//!
//! The outlines are flattened into polygons within the tolerance. Each
//! closed outline is cut on its outside, its inside or on the line:
//!
//! - **Automatic**: an outline inside an odd number of other closed
//!   outlines (of any object) is a hole and is cut on its inside, every
//!   other closed outline on its outside; open curves on the line.
//! - **Outside**, **Inside**, **On the line**: every closed outline the
//!   same way; open curves are always cut on the line.
//!
//! The tool's centre follows the outline offset by the tool's radius
//! (`tracedraw_core::shaping::offset_polygon`, round corners outside,
//! sharp ones inside). A hole smaller than the tool gives no toolpath and
//! is counted as skipped.
//!
//! Direction, with the spindle turning clockwise (M3): **conventional**
//! milling keeps the part on the left of the tool, **climb** milling on
//! its right (outside cuts run clockwise, holes counter-clockwise).
//!
//! The toolpaths are ordered by [`crate::cut::order::plan`] from the
//! origin, inner ones first, so holes are cut before the outline that
//! frees the part. Each toolpath is cut down to the total depth in
//! passes of equal depth, no deeper than the depth per pass, before the
//! next one: `n = ceil(depth / pass depth)`, pass `k` at `-depth k / n`.
//!
//! Holding tabs leave `tab height` of material at the bottom of closed
//! outside cuts: `count` tabs spread evenly by length, the first half a
//! spacing from the start, each `tab width + tool diameter` long on the
//! toolpath so the material left is `tab width` wide. Passes below the
//! top of a tab rise to it over the tab. Tabs that would take more than
//! nine tenths of a toolpath are left out of it.
//!
//! Ramps take the tool down along the path instead of straight down:
//! **smooth** goes down over the ramp length as it moves along a closed
//! toolpath, goes round at depth and cuts the ramp stretch again at the
//! end (the next pass starts where that one ended); **zig-zag** goes
//! forward over the ramp length to half way down and back to the start
//! the rest of the way. Open toolpaths always zig-zag. Straight-down
//! moves then only reach the top of the material or the last pass.
//!
//! Leads bring the tool into closed outside and inside cuts from the
//! waste side (outside an outline, inside a hole) and take it back out:
//! a **line** square to the toolpath or a **quarter circle** meeting it
//! along its direction, the lead length long (the circle's radius).
//! Such a toolpath starts half way along its longest side; a lead that
//! would cross the part is halved until it fits, or left out. With
//! leads every pass goes in at the far end of the lead-in, and a ramp
//! goes down along the lead-in.
//!
//! The program is plain RS-274 style G-code in millimetres: `G21`,
//! `G90`, `G94`, a rapid to the safe height, `M3 S..`, then for every
//! toolpath `G0` to its start, `G1 Z.. F<plunge>` down and `G1 X.. Y..
//! F<feed>` along, `G0` back up to the safe height; `M5` and `M30` at
//! the end. X0 Y0 is the bottom-left corner of the objects' box (or a
//! given origin), Z0 the top of the material.

use crate::cut::order::{self, Direction, RouteItem, RouteSpec, StartAt};
use crate::cut::{fin, finite, inside, poly_bounds, polygon_path, polys, signed_area};
use tracedraw_core::geometry::{BezPath, Point, Rect};

/// Which side of an outline the tool cuts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Side {
    /// Holes inside, outlines outside, open curves on the line.
    #[default]
    Auto,
    Outside,
    Inside,
    OnLine,
}

/// Which way the tool goes round an outline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Milling {
    /// The part on the left of the tool.
    #[default]
    Conventional,
    /// The part on the right of the tool.
    Climb,
}

/// Where X0 Y0 of the program is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Zero {
    /// The bottom-left corner of the objects' box.
    #[default]
    JobCorner,
    /// The point the caller gives (TraceDraw passes the ruler origin).
    Origin,
}

/// How the tool goes down into the material at the start of a pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Ramp {
    /// Straight down at the plunge rate.
    #[default]
    Off,
    /// Down along the path over the ramp length.
    Smooth,
    /// Forward over the ramp length to half way down, back to the
    /// start the rest of the way.
    ZigZag,
}

/// How the tool comes into and leaves a closed outside or inside cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Lead {
    /// Starts and ends on the toolpath.
    #[default]
    Off,
    /// A straight line from the waste side, square to the toolpath.
    Line,
    /// A quarter circle from the waste side, meeting the toolpath along it.
    Arc,
}

/// Settings of [`program`]; lengths in millimetres, rates in mm a minute.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct CncSpec {
    pub tool_diameter: f64,
    pub side: Side,
    pub milling: Milling,
    /// Total depth of the cut below the top of the material.
    pub depth: f64,
    /// The most one pass goes down.
    pub pass_depth: f64,
    /// Height above the material for moves between cuts.
    pub safe_z: f64,
    pub feed: f64,
    pub plunge: f64,
    /// Revolutions a minute; 0 leaves out the spindle commands.
    pub spindle: f64,
    /// How far the line segments may stray from curves.
    pub tolerance: f64,
    pub zero: Zero,
    pub tabs: bool,
    pub tab_count: u32,
    /// Width of the material each tab leaves.
    pub tab_width: f64,
    /// Thickness of the material each tab leaves.
    pub tab_height: f64,
    pub ramp: Ramp,
    /// Length of path a ramp goes down over.
    pub ramp_length: f64,
    pub lead: Lead,
    /// Length of a line lead, radius of an arc lead.
    pub lead_length: f64,
}

impl Default for CncSpec {
    fn default() -> Self {
        CncSpec {
            tool_diameter: 3.175,
            side: Side::Auto,
            milling: Milling::Conventional,
            depth: 6.0,
            pass_depth: 2.0,
            safe_z: 5.0,
            feed: 1000.0,
            plunge: 300.0,
            spindle: 18000.0,
            tolerance: 0.02,
            zero: Zero::JobCorner,
            tabs: false,
            tab_count: 4,
            tab_width: 5.0,
            tab_height: 1.5,
            ramp: Ramp::Off,
            ramp_length: 10.0,
            lead: Lead::Off,
            lead_length: 5.0,
        }
    }
}

/// The ranges the settings are kept in.
pub const TOOL_RANGE: (f64, f64) = (0.01, 100.0);
pub const DEPTH_RANGE: (f64, f64) = (0.01, 500.0);
pub const SAFE_Z_RANGE: (f64, f64) = (0.1, 500.0);
pub const RATE_RANGE: (f64, f64) = (1.0, 100_000.0);
pub const SPINDLE_RANGE: (f64, f64) = (0.0, 100_000.0);
pub const TOLERANCE_RANGE: (f64, f64) = (0.001, 1.0);
pub const TAB_COUNT_RANGE: (u32, u32) = (1, 100);
pub const TAB_RANGE: (f64, f64) = (0.01, 500.0);
pub const RAMP_RANGE: (f64, f64) = (0.1, 1000.0);
pub const LEAD_RANGE: (f64, f64) = (0.1, 200.0);

/// The most passes one toolpath gets.
pub const MAX_PASSES: usize = 1000;

impl CncSpec {
    /// The settings with every value finite and in its range.
    pub fn clamped(&self) -> CncSpec {
        let d = CncSpec::default();
        let c = |v: f64, fallback: f64, r: (f64, f64)| fin(v, fallback).clamp(r.0, r.1);
        CncSpec {
            tool_diameter: c(self.tool_diameter, d.tool_diameter, TOOL_RANGE),
            depth: c(self.depth, d.depth, DEPTH_RANGE),
            pass_depth: c(self.pass_depth, d.pass_depth, DEPTH_RANGE),
            safe_z: c(self.safe_z, d.safe_z, SAFE_Z_RANGE),
            feed: c(self.feed, d.feed, RATE_RANGE),
            plunge: c(self.plunge, d.plunge, RATE_RANGE),
            spindle: c(self.spindle, d.spindle, SPINDLE_RANGE),
            tolerance: c(self.tolerance, d.tolerance, TOLERANCE_RANGE),
            tab_count: self.tab_count.clamp(TAB_COUNT_RANGE.0, TAB_COUNT_RANGE.1),
            tab_width: c(self.tab_width, d.tab_width, TAB_RANGE),
            tab_height: c(self.tab_height, d.tab_height, TAB_RANGE),
            ramp_length: c(self.ramp_length, d.ramp_length, RAMP_RANGE),
            lead_length: c(self.lead_length, d.lead_length, LEAD_RANGE),
            ..*self
        }
    }

    /// The Z of every pass, top first: equal steps no deeper than the
    /// depth per pass, the last at the total depth.
    pub fn pass_levels(&self) -> Vec<f64> {
        let s = self.clamped();
        let n = ((s.depth / s.pass_depth) - 1e-9).ceil().max(1.0);
        let n = (n as usize).min(MAX_PASSES);
        (1..=n).map(|k| -s.depth * k as f64 / n as f64).collect()
    }
}

/// One toolpath: the tool centre's polygon in page space, how it was
/// worked out, and the stretches (by length from its start) cut at the
/// top of a tab.
#[derive(Debug, Clone, PartialEq)]
pub struct Toolpath {
    pub pts: Vec<Point>,
    pub closed: bool,
    /// Outside, Inside or OnLine (never Auto).
    pub side: Side,
    pub tabs: Vec<(f64, f64)>,
}

impl Toolpath {
    /// The toolpath as a path (for the preview).
    pub fn path(&self) -> BezPath {
        if self.closed {
            polygon_path(&self.pts)
        } else {
            tracedraw_core::geometry::polyline_path(&self.pts, false)
        }
    }

    /// Its length, the closing segment included.
    pub fn length(&self) -> f64 {
        let open: f64 = self.pts.windows(2).map(|w| (w[1] - w[0]).hypot()).sum();
        match (self.closed, self.pts.first(), self.pts.last()) {
            (true, Some(a), Some(b)) => open + (*a - *b).hypot(),
            _ => open,
        }
    }
}

/// A G-code program and what it does.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Program {
    pub gcode: String,
    /// In cut order.
    pub toolpaths: Vec<Toolpath>,
    /// Moves at the safe height, page space.
    pub rapids: Vec<(Point, Point)>,
    /// The lead-in and lead-out moves, page space (for the preview).
    pub leads: Vec<Vec<Point>>,
    /// Passes per toolpath.
    pub passes: usize,
    /// Length cut at the feed rate, every pass (mm).
    pub cut_length: f64,
    /// Time of the moves at the feed and plunge rates (minutes).
    pub minutes: f64,
    /// Closed outlines too small for the tool on their side.
    pub skipped: usize,
    /// The page point that is X0 Y0.
    pub zero: Point,
}

/// A flattened outline with its resolved side.
struct Outline {
    pts: Vec<Point>,
    closed: bool,
    side: Side,
}

/// The outlines of `objects`, each given its side.
fn outlines(objects: &[Vec<BezPath>], spec: &CncSpec) -> Vec<Outline> {
    let mut all: Vec<(Vec<Point>, bool)> = Vec::new();
    for paths in objects {
        for p in paths {
            for poly in polys(p, spec.tolerance) {
                let closed = poly.closed && poly.pts.len() >= 3;
                all.push((poly.pts, closed));
            }
        }
    }
    let closed: Vec<(usize, Rect, f64)> = all
        .iter()
        .enumerate()
        .filter(|(_, (_, c))| *c)
        .filter_map(|(i, (pts, _))| Some((i, poly_bounds(pts)?, signed_area(pts).abs())))
        .collect();
    all.iter()
        .enumerate()
        .map(|(i, (pts, is_closed))| {
            let side = if !is_closed {
                Side::OnLine
            } else {
                match spec.side {
                    Side::Auto => {
                        let level = depth_in(i, pts, &closed, &all);
                        if level % 2 == 1 {
                            Side::Inside
                        } else {
                            Side::Outside
                        }
                    }
                    s => s,
                }
            };
            Outline {
                pts: pts.clone(),
                closed: *is_closed,
                side,
            }
        })
        .collect()
}

/// How many closed outlines the outline `i` lies in: its box inside
/// theirs, its area smaller and its first point inside them.
fn depth_in(
    i: usize,
    pts: &[Point],
    closed: &[(usize, Rect, f64)],
    all: &[(Vec<Point>, bool)],
) -> usize {
    let (Some(b), Some(first)) = (poly_bounds(pts), pts.first()) else {
        return 0;
    };
    let area = signed_area(pts).abs();
    closed
        .iter()
        .filter(|(j, jb, ja)| {
            *j != i
                && *ja > area
                && jb.x0 <= b.x0 + 1e-9
                && jb.y0 <= b.y0 + 1e-9
                && jb.x1 >= b.x1 - 1e-9
                && jb.y1 >= b.y1 - 1e-9
                && all.get(*j).is_some_and(|(o, _)| inside(*first, o))
        })
        .count()
}

/// The loops of the tool centre for a closed outline cut on `side`, in
/// the direction `milling` asks for.
fn offset_loops(pts: &[Point], side: Side, spec: &CncSpec) -> Vec<Vec<Point>> {
    let r = spec.tool_diameter / 2.0;
    let d = match side {
        Side::Outside => r,
        Side::Inside => -r,
        _ => 0.0,
    };
    let mut loops: Vec<Vec<Point>> = if d == 0.0 {
        vec![pts.to_vec()]
    } else {
        let grown = tracedraw_core::shaping::offset_polygon(&polygon_path(pts), d);
        polys(&grown, spec.tolerance)
            .into_iter()
            .filter(|p| p.closed && p.pts.len() >= 3 && signed_area(&p.pts).abs() > 1e-9)
            .map(|p| p.pts)
            .collect()
    };
    if side == Side::OnLine {
        return loops;
    }
    // The offset's area lies on the left of each loop once outer loops
    // run counter-clockwise and the loops inside them clockwise.
    let levels: Vec<usize> = loops
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let Some(first) = l.first() else { return 0 };
            loops
                .iter()
                .enumerate()
                .filter(|(j, o)| *j != i && inside(*first, o))
                .count()
        })
        .collect();
    // Outside: the part lies in the offset's area, on its left;
    // inside: outside the area, on its right. Conventional milling keeps
    // the part on the left of the tool.
    let part_on_left = side == Side::Outside;
    let want_left = spec.milling == Milling::Conventional;
    for (l, level) in loops.iter_mut().zip(levels) {
        let outer = level % 2 == 0;
        let ccw = signed_area(l) > 0.0;
        let area_on_left = ccw == outer;
        let left = if part_on_left {
            area_on_left
        } else {
            !area_on_left
        };
        if left != want_left {
            l.reverse();
        }
    }
    loops
}

/// The toolpaths of `objects` (page-space outlines per object) in cut
/// order from `from`, and how many closed outlines were too small.
pub fn toolpaths(objects: &[Vec<BezPath>], spec: &CncSpec, from: Point) -> (Vec<Toolpath>, usize) {
    let spec = spec.clamped();
    let mut paths: Vec<Toolpath> = Vec::new();
    let mut skipped = 0;
    for o in outlines(objects, &spec) {
        if !o.closed {
            paths.push(Toolpath {
                pts: o.pts,
                closed: false,
                side: Side::OnLine,
                tabs: Vec::new(),
            });
            continue;
        }
        let loops = offset_loops(&o.pts, o.side, &spec);
        if loops.is_empty() {
            skipped += 1;
        }
        paths.extend(loops.into_iter().map(|pts| Toolpath {
            pts,
            closed: true,
            side: o.side,
            tabs: Vec::new(),
        }));
    }
    let items: Vec<RouteItem> = paths
        .iter()
        .map(|t| RouteItem {
            paths: vec![t.path()],
            editable: true,
        })
        .collect();
    let route = order::plan(
        &items,
        &RouteSpec {
            direction: Direction::Keep,
            start: StartAt::Nearest,
            inner_first: true,
            from: if finite(from) { from } else { Point::ZERO },
        },
    );
    let mut ordered = Vec::with_capacity(paths.len());
    for i in &route.order {
        let (Some(t), Some(cuts)) = (paths.get(*i), route.cuts.get(*i)) else {
            continue;
        };
        // The planner may start a loop elsewhere or run an open path the
        // other way; its points are the toolpath's own.
        let pts = cuts
            .first()
            .and_then(|c| polys(c, spec.tolerance).into_iter().next())
            .map(|p| p.pts)
            .filter(|p| p.len() >= 2)
            .unwrap_or_else(|| t.pts.clone());
        let mut t = Toolpath { pts, ..t.clone() };
        if spec.lead != Lead::Off && t.closed && matches!(t.side, Side::Outside | Side::Inside) {
            t.pts = start_mid_longest(&t.pts);
        }
        if spec.tabs && t.closed && t.side == Side::Outside && spec.tab_height < spec.depth {
            t.tabs = tab_ranges(t.length(), &spec);
        }
        ordered.push(t);
    }
    (ordered, skipped)
}

/// The stretches of a closed toolpath of length `len` cut at the top of
/// a tab.
fn tab_ranges(len: f64, spec: &CncSpec) -> Vec<(f64, f64)> {
    let gap = spec.tab_width + spec.tool_diameter;
    let mut n = spec.tab_count as usize;
    while n > 0 && n as f64 * gap > 0.9 * len {
        n -= 1;
    }
    if n == 0 {
        return Vec::new();
    }
    let step = len / n as f64;
    (0..n)
        .map(|i| {
            let c = (i as f64 + 0.5) * step;
            (c - gap / 2.0, c + gap / 2.0)
        })
        .collect()
}

/// A coordinate for the program: three decimals, never `-0.000`.
fn num(v: f64) -> String {
    let s = format!("{v:.3}");
    if s == "-0.000" {
        "0.000".into()
    } else {
        s
    }
}

/// Writes the moves and keeps count of what they cost.
struct Writer {
    out: String,
    zero: Point,
    feed: f64,
    plunge: f64,
    at: Point,
    z: f64,
    f: Option<f64>,
    cut_length: f64,
    minutes: f64,
}

impl Writer {
    fn line(&mut self, s: &str) {
        self.out.push_str(s);
        self.out.push('\n');
    }

    fn xy(&self, p: Point) -> String {
        format!("X{} Y{}", num(p.x - self.zero.x), num(p.y - self.zero.y))
    }

    /// `G1` with the feed rate when it changes.
    fn feed_word(&mut self, f: f64) -> String {
        if self.f == Some(f) {
            String::new()
        } else {
            self.f = Some(f);
            format!(" F{f:.0}")
        }
    }

    fn rapid_z(&mut self, z: f64) {
        if (self.z - z).abs() > 1e-9 {
            let s = format!("G0 Z{}", num(z));
            self.line(&s);
            self.z = z;
        }
    }

    fn rapid_xy(&mut self, p: Point) {
        if (self.at - p).hypot() > 1e-9 {
            let s = format!("G0 {}", self.xy(p));
            self.line(&s);
            self.at = p;
        }
    }

    fn feed_z(&mut self, z: f64) {
        if (self.z - z).abs() > 1e-9 {
            let f = self.feed_word(self.plunge);
            let s = format!("G1 Z{}{f}", num(z));
            self.line(&s);
            self.minutes += (self.z - z).abs() / self.plunge;
            self.z = z;
        }
    }

    /// `G1` along and down (or up) at once, at the feed rate.
    fn feed_xyz(&mut self, p: Point, z: f64) {
        let d = (p - self.at).hypot();
        let dz = (z - self.z).abs();
        if dz <= 1e-9 {
            self.feed_xy(p);
            return;
        }
        if d <= 1e-9 {
            self.feed_z(z);
            return;
        }
        let f = self.feed_word(self.feed);
        let s = format!("G1 {} Z{}{f}", self.xy(p), num(z));
        self.line(&s);
        self.cut_length += d;
        self.minutes += d.hypot(dz) / self.feed;
        self.at = p;
        self.z = z;
    }

    fn feed_xy(&mut self, p: Point) {
        let d = (p - self.at).hypot();
        if d > 1e-9 {
            let f = self.feed_word(self.feed);
            let s = format!("G1 {}{f}", self.xy(p));
            self.line(&s);
            self.cut_length += d;
            self.minutes += d / self.feed;
            self.at = p;
        }
    }
}

/// A closed ring started half way along its longest side, so a lead
/// meets it on a straight stretch, not at a corner.
fn start_mid_longest(pts: &[Point]) -> Vec<Point> {
    let n = pts.len();
    if n < 3 {
        return pts.to_vec();
    }
    let seg = |i: usize| (pts[(i + 1) % n] - pts[i]).hypot();
    let i = (0..n).max_by(|a, b| seg(*a).total_cmp(&seg(*b))).unwrap_or(0);
    let mid = pts[i].midpoint(pts[(i + 1) % n]);
    let mut out = Vec::with_capacity(n + 1);
    out.push(mid);
    out.extend((1..=n).map(|k| pts[(i + k) % n]));
    out
}

/// A polyline measured by length from its first point; a closed one
/// (its first point repeated at the end) wraps round.
struct Poly {
    pts: Vec<Point>,
    /// Length from the start to each point.
    cum: Vec<f64>,
    len: f64,
    closed: bool,
}

impl Poly {
    fn new(pts: Vec<Point>, closed: bool) -> Poly {
        let mut cum = Vec::with_capacity(pts.len());
        let mut d = 0.0;
        for (i, p) in pts.iter().enumerate() {
            if i > 0 {
                d += (*p - pts[i - 1]).hypot();
            }
            cum.push(d);
        }
        Poly {
            pts,
            cum,
            len: d,
            closed,
        }
    }

    /// The length along it, wrapped round a closed one and kept on an
    /// open one.
    fn local(&self, s: f64) -> f64 {
        if self.closed && self.len > 1e-12 {
            s.rem_euclid(self.len)
        } else {
            s.clamp(0.0, self.len)
        }
    }

    fn at(&self, s: f64) -> Point {
        let s = self.local(s);
        let Some(&first) = self.pts.first() else {
            return Point::ZERO;
        };
        let i = self.cum.partition_point(|c| *c < s);
        if i == 0 {
            return first;
        }
        if i >= self.pts.len() {
            return *self.pts.last().unwrap_or(&first);
        }
        let (a, b) = (self.pts[i - 1], self.pts[i]);
        let seg = self.cum[i] - self.cum[i - 1];
        if seg <= 1e-12 {
            return b;
        }
        a + (b - a) * ((s - self.cum[i - 1]) / seg)
    }

    /// The lengths in `a..b` (unwrapped) where it has a point or one of
    /// `edges` (local lengths) falls.
    fn breaks(&self, a: f64, b: f64, edges: &[f64]) -> Vec<f64> {
        let mut out = Vec::new();
        let laps = if self.closed && self.len > 1e-12 {
            ((a / self.len).floor() as i64, (b / self.len).ceil() as i64)
        } else {
            (0, 0)
        };
        for lap in laps.0..=laps.1 {
            let base = lap as f64 * self.len;
            for c in self.cum.iter().chain(edges) {
                let v = base + c;
                if v > a + 1e-9 && v < b - 1e-9 {
                    out.push(v);
                }
            }
        }
        out.push(b);
        out.sort_by(f64::total_cmp);
        out.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
        out
    }
}

/// Cuts along `line` from length `a` to `b` with the depth `z_at` gives
/// for a local length. Level stretches go down or up at each change;
/// `ramp` stretches go down as they go, in steps of at most 0.5 mm.
fn walk(w: &mut Writer, line: &Poly, a: f64, b: f64, edges: &[f64], z_at: impl Fn(f64) -> f64, ramp: bool) {
    if b <= a + 1e-12 || line.pts.len() < 2 {
        return;
    }
    let mut from = a;
    for c in line.breaks(a, b, edges) {
        if ramp {
            let n = ((c - from) / 0.5).ceil().max(1.0) as usize;
            for k in 1..=n {
                let s = from + (c - from) * k as f64 / n as f64;
                w.feed_xyz(line.at(s), z_at(line.local(s)));
            }
        } else {
            w.feed_z(z_at(line.local((from + c) / 2.0)));
            w.feed_xy(line.at(c));
        }
        from = c;
    }
}

/// A zig-zag ramp from the start of `line`: forward over `r` from `top`
/// to half way to `z`, then back to the start the rest of the way, never
/// below `floor` (the tabs).
fn zigzag(w: &mut Writer, line: &Poly, r: f64, top: f64, z: f64, floor: impl Fn(f64) -> f64) {
    let mid = (top + z) / 2.0;
    let r = r.max(1e-9);
    let fwd = |s: f64| (top + (mid - top) * (s / r).min(1.0)).max(floor(s));
    walk(w, line, 0.0, r, &[], fwd, true);
    let n = (r / 0.5).ceil().max(1.0) as usize;
    for k in 1..=n {
        let s = r - r * k as f64 / n as f64;
        let zz = (mid + (z - mid) * (k as f64 / n as f64)).max(floor(s));
        w.feed_xyz(line.at(s), zz);
    }
}

/// The lead-in (ending at the start) and lead-out (starting there) of a
/// closed toolpath cut outside or inside, on its waste side; none when
/// leads are off, the toolpath is on the line or no lead fits.
fn lead_moves(pts: &[Point], side: Side, spec: &CncSpec) -> Option<(Vec<Point>, Vec<Point>)> {
    if spec.lead == Lead::Off || pts.len() < 3 || !matches!(side, Side::Outside | Side::Inside) {
        return None;
    }
    let s = pts[0];
    let unit = |v: tracedraw_core::geometry::Vec2| {
        let l = v.hypot();
        (l > 1e-12).then(|| v / l)
    };
    let t_in = unit(pts[1] - s)?;
    let t_out = unit(s - pts[pts.len() - 1])?;
    // The outward normal of a ring of the given turn.
    let ccw = signed_area(pts) > 0.0;
    let out_of = |t: tracedraw_core::geometry::Vec2| {
        if ccw {
            tracedraw_core::geometry::Vec2::new(t.y, -t.x)
        } else {
            tracedraw_core::geometry::Vec2::new(-t.y, t.x)
        }
    };
    let flip = if side == Side::Inside { -1.0 } else { 1.0 };
    let (n_in, n_out) = (out_of(t_in) * flip, out_of(t_out) * flip);
    let mut len = spec.lead_length;
    for _ in 0..6 {
        let (a, b) = match spec.lead {
            Lead::Line => (vec![s + n_in * len, s], vec![s, s + n_out * len]),
            _ => {
                let steps = 12;
                let c_in = s + n_in * len;
                let c_out = s + n_out * len;
                let q = std::f64::consts::FRAC_PI_2;
                let a = (0..=steps)
                    .map(|i| {
                        let f = q * i as f64 / steps as f64;
                        c_in + (t_in * -f.cos() - n_in * f.sin()) * len
                    })
                    .collect::<Vec<_>>();
                let b = (0..=steps)
                    .map(|i| {
                        let f = q * i as f64 / steps as f64;
                        c_out + (n_out * -f.cos() + t_out * f.sin()) * len
                    })
                    .collect::<Vec<_>>();
                (a, b)
            }
        };
        // Every lead point must be on the waste side.
        let waste = |p: &Point| inside(*p, pts) == (side == Side::Inside);
        if a[..a.len() - 1].iter().all(waste) && b[1..].iter().all(waste) {
            return Some((a, b));
        }
        len /= 2.0;
    }
    None
}

/// The program that cuts `objects` (page-space outlines per object) as
/// `spec` says; `origin` is where X0 Y0 goes with [`Zero::Origin`].
pub fn program(objects: &[Vec<BezPath>], spec: &CncSpec, origin: Point) -> Program {
    let spec = spec.clamped();
    let all: Vec<BezPath> = objects.iter().flatten().cloned().collect();
    let corner = crate::cut::bounds(&all)
        .map(|b| Point::new(b.x0, b.y0))
        .unwrap_or(Point::ZERO);
    let zero = match spec.zero {
        Zero::JobCorner => corner,
        Zero::Origin if finite(origin) => origin,
        Zero::Origin => Point::ZERO,
    };
    let (paths, skipped) = toolpaths(objects, &spec, zero);
    let levels = spec.pass_levels();
    let tab_z = -(spec.depth - spec.tab_height);
    let mut w = Writer {
        out: String::new(),
        zero,
        feed: spec.feed,
        plunge: spec.plunge,
        at: zero,
        z: spec.safe_z,
        f: None,
        cut_length: 0.0,
        minutes: 0.0,
    };
    w.line("(TraceDraw CNC toolpath)");
    let head = format!(
        "(Units mm, tool {} mm, depth {} mm in {} passes, {} toolpaths)",
        num(spec.tool_diameter),
        num(spec.depth),
        levels.len(),
        paths.len()
    );
    w.line(&head);
    w.line("(X0 Y0 at the origin chosen, Z0 on the top of the material)");
    w.line("G21");
    w.line("G90");
    w.line("G94");
    let safe = format!("G0 Z{}", num(spec.safe_z));
    w.line(&safe);
    if spec.spindle > 0.0 {
        let s = format!("M3 S{:.0}", spec.spindle);
        w.line(&s);
    }
    let mut rapids = Vec::new();
    let mut all_leads = Vec::new();
    for (i, t) in paths.iter().enumerate() {
        let Some(&start) = t.pts.first() else {
            continue;
        };
        let name = match t.side {
            Side::Outside => "outside",
            Side::Inside => "inside",
            _ => "on the line",
        };
        let s = format!("(Toolpath {}: {name})", i + 1);
        w.line(&s);
        let mut ring = t.pts.clone();
        if t.closed {
            ring.push(start);
        }
        let line = Poly::new(ring, t.closed);
        let leads = if t.closed {
            lead_moves(&t.pts, t.side, &spec)
        } else {
            None
        };
        if let Some((a, b)) = &leads {
            all_leads.push(a.clone());
            all_leads.push(b.clone());
        }
        // Where along a closed ring a smooth ramp left the tool.
        let mut off = 0.0;
        let mut top = 0.0;
        for (k, &z) in levels.iter().enumerate() {
            let deep = |s: f64| {
                if z < tab_z && t.tabs.iter().any(|(a, b)| s > *a && s < *b) {
                    tab_z
                } else {
                    z
                }
            };
            let edges: Vec<f64> = t.tabs.iter().flat_map(|(a, b)| [*a, *b]).collect();
            if let Some((lead_in, lead_out)) = &leads {
                // In from the waste side every pass, out the same way.
                let lin = Poly::new(lead_in.clone(), false);
                w.rapid_z(spec.safe_z);
                if (w.at - lin.pts[0]).hypot() > 1e-9 {
                    rapids.push((w.at, lin.pts[0]));
                }
                w.rapid_xy(lin.pts[0]);
                match spec.ramp {
                    Ramp::Off => {
                        w.feed_z(z);
                        walk(&mut w, &lin, 0.0, lin.len, &[], |_| z, false);
                    }
                    Ramp::Smooth => {
                        w.feed_z(top);
                        let r = lin.len.max(1e-9);
                        walk(&mut w, &lin, 0.0, lin.len, &[], |s| top + (z - top) * (s / r).min(1.0), true);
                    }
                    Ramp::ZigZag => {
                        w.feed_z(top);
                        zigzag(&mut w, &lin, lin.len, top, z, |_| z);
                    }
                }
                walk(&mut w, &line, 0.0, line.len, &edges, deep, false);
                let lout = Poly::new(lead_out.clone(), false);
                walk(&mut w, &lout, 0.0, lout.len, &[], |_| z, false);
            } else {
                // A closed toolpath goes down again where it ended; an
                // open one goes back to its start at the safe height.
                if k == 0 || !t.closed {
                    w.rapid_z(spec.safe_z);
                    let p0 = line.at(off);
                    if (w.at - p0).hypot() > 1e-9 {
                        rapids.push((w.at, p0));
                    }
                    w.rapid_xy(p0);
                }
                let r = spec.ramp_length.min(line.len);
                match spec.ramp {
                    Ramp::Smooth if t.closed && r > 1e-9 => {
                        // Down along the ring, round it at depth, then
                        // over the ramp again at depth.
                        w.feed_z(top);
                        let ramp = |s: f64| (top + (z - top) * ((s - off) / r).min(1.0)).max(deep(s));
                        walk(&mut w, &line, off, off + r, &edges, ramp, true);
                        walk(&mut w, &line, off + r, off + line.len + r, &edges, deep, false);
                        off = (off + r) % line.len;
                    }
                    Ramp::Smooth | Ramp::ZigZag if r > 1e-9 => {
                        w.feed_z(top);
                        zigzag(&mut w, &line, r, top, z, deep);
                        walk(&mut w, &line, 0.0, line.len, &edges, deep, false);
                    }
                    _ => {
                        w.feed_z(deep(off));
                        walk(&mut w, &line, off, off + line.len, &edges, deep, false);
                    }
                }
            }
            top = z;
        }
        w.rapid_z(spec.safe_z);
    }
    w.rapid_z(spec.safe_z);
    if spec.spindle > 0.0 {
        w.line("M5");
    }
    if (w.at - zero).hypot() > 1e-9 {
        rapids.push((w.at, zero));
    }
    w.rapid_xy(zero);
    w.line("M30");
    Program {
        gcode: w.out,
        passes: levels.len(),
        cut_length: w.cut_length,
        minutes: w.minutes,
        toolpaths: paths,
        rapids,
        leads: all_leads,
        skipped,
        zero,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> BezPath {
        crate::cut::rect_path(Rect::new(x0, y0, x1, y1))
    }

    fn circle(c: Point, r: f64) -> BezPath {
        tracedraw_core::geometry::ellipse_path(Rect::new(c.x - r, c.y - r, c.x + r, c.y + r))
    }

    fn spec() -> CncSpec {
        CncSpec {
            tool_diameter: 6.0,
            ..Default::default()
        }
    }

    /// The distance from `p` to the rectangle's outline.
    fn to_rect(p: Point, r: Rect) -> f64 {
        let dx = (r.x0 - p.x).max(0.0).max(p.x - r.x1);
        let dy = (r.y0 - p.y).max(0.0).max(p.y - r.y1);
        if dx > 0.0 || dy > 0.0 {
            dx.hypot(dy)
        } else {
            (p.x - r.x0).min(r.x1 - p.x).min(p.y - r.y0).min(r.y1 - p.y)
        }
    }

    #[test]
    fn a_rectangle_is_cut_a_tool_radius_outside() {
        let r = Rect::new(0.0, 0.0, 100.0, 50.0);
        let (paths, skipped) =
            toolpaths(&[vec![rect(0.0, 0.0, 100.0, 50.0)]], &spec(), Point::ZERO);
        assert_eq!(skipped, 0);
        assert_eq!(paths.len(), 1);
        let t = &paths[0];
        assert_eq!(t.side, Side::Outside);
        let b = poly_bounds(&t.pts).expect("box");
        assert!(
            (b.x0 + 3.0).abs() < 0.01 && (b.x1 - 103.0).abs() < 0.01,
            "{b:?}"
        );
        assert!(
            (b.y0 + 3.0).abs() < 0.01 && (b.y1 - 53.0).abs() < 0.01,
            "{b:?}"
        );
        // Every point of the tool centre is a radius from the part, the
        // corners rounded.
        for p in &t.pts {
            assert!((to_rect(*p, r) - 3.0).abs() < 0.05, "{p:?}");
        }
        assert!(t.pts.len() > 8);
    }

    #[test]
    fn a_rectangle_cut_inside_keeps_sharp_corners() {
        let r = Rect::new(0.0, 0.0, 100.0, 50.0);
        let s = CncSpec {
            side: Side::Inside,
            ..spec()
        };
        let (paths, _) = toolpaths(&[vec![rect(0.0, 0.0, 100.0, 50.0)]], &s, Point::ZERO);
        assert_eq!(paths.len(), 1);
        let b = poly_bounds(&paths[0].pts).expect("box");
        assert!(
            (b.x0 - 3.0).abs() < 0.01 && (b.x1 - 97.0).abs() < 0.01,
            "{b:?}"
        );
        assert!(
            (b.y0 - 3.0).abs() < 0.01 && (b.y1 - 47.0).abs() < 0.01,
            "{b:?}"
        );
        assert_eq!(paths[0].pts.len(), 4);
        for p in &paths[0].pts {
            assert!((to_rect(*p, r) - 3.0).abs() < 0.01);
        }
    }

    #[test]
    fn a_circle_grows_and_shrinks_by_the_radius() {
        let c = Point::new(50.0, 50.0);
        for (side, want) in [
            (Side::Outside, 23.0),
            (Side::Inside, 17.0),
            (Side::OnLine, 20.0),
        ] {
            let s = CncSpec { side, ..spec() };
            let (paths, _) = toolpaths(&[vec![circle(c, 20.0)]], &s, Point::ZERO);
            assert_eq!(paths.len(), 1, "{side:?}");
            for p in &paths[0].pts {
                assert!(((*p - c).hypot() - want).abs() < 0.06, "{side:?} {p:?}");
            }
        }
    }

    #[test]
    fn holes_are_cut_inside_and_first() {
        // A plate with a round hole, as one object and as two.
        let plate = rect(0.0, 0.0, 100.0, 100.0);
        let hole = circle(Point::new(50.0, 50.0), 20.0);
        let mut one = plate.clone();
        one.extend(hole.elements().iter().copied());
        for objects in [
            vec![vec![one]],
            vec![vec![plate.clone()], vec![hole.clone()]],
        ] {
            let (paths, _) = toolpaths(&objects, &spec(), Point::ZERO);
            assert_eq!(paths.len(), 2);
            assert_eq!(paths[0].side, Side::Inside);
            assert_eq!(paths[1].side, Side::Outside);
            let hb = poly_bounds(&paths[0].pts).expect("hole");
            assert!((hb.width() - 34.0).abs() < 0.1, "{hb:?}");
        }
        // Forced to the outside, the hole's toolpath grows instead.
        let s = CncSpec {
            side: Side::Outside,
            ..spec()
        };
        let (paths, _) = toolpaths(&[vec![plate], vec![hole]], &s, Point::ZERO);
        let small = paths
            .iter()
            .filter_map(|t| poly_bounds(&t.pts))
            .map(|b| b.width())
            .fold(f64::INFINITY, f64::min);
        assert!((small - 46.0).abs() < 0.1, "{small}");
    }

    #[test]
    fn conventional_and_climb_run_opposite_ways() {
        let plate = rect(0.0, 0.0, 100.0, 100.0);
        let hole = rect(40.0, 40.0, 60.0, 60.0);
        let dir = |m: Milling| {
            let s = CncSpec {
                milling: m,
                ..spec()
            };
            let (paths, _) = toolpaths(&[vec![plate.clone()], vec![hole.clone()]], &s, Point::ZERO);
            paths
                .iter()
                .map(|t| (t.side, signed_area(&t.pts) > 0.0))
                .collect::<Vec<_>>()
        };
        // Conventional: the part on the left, so outside counter-clockwise
        // and holes clockwise; climb the other way round.
        assert_eq!(
            dir(Milling::Conventional),
            vec![(Side::Inside, false), (Side::Outside, true)]
        );
        assert_eq!(
            dir(Milling::Climb),
            vec![(Side::Inside, true), (Side::Outside, false)]
        );
    }

    #[test]
    fn a_hole_smaller_than_the_tool_is_skipped() {
        let s = CncSpec {
            side: Side::Inside,
            ..spec()
        };
        let (paths, skipped) =
            toolpaths(&[vec![circle(Point::new(5.0, 5.0), 2.0)]], &s, Point::ZERO);
        assert!(paths.is_empty());
        assert_eq!(skipped, 1);
    }

    #[test]
    fn open_curves_are_cut_on_the_line() {
        let line = tracedraw_core::geometry::polyline_path(
            &[
                Point::new(0.0, 0.0),
                Point::new(50.0, 0.0),
                Point::new(50.0, 20.0),
            ],
            false,
        );
        let s = CncSpec {
            side: Side::Outside,
            zero: Zero::Origin,
            ..spec()
        };
        let p = program(&[vec![line]], &s, Point::new(-10.0, -10.0));
        assert_eq!(p.toolpaths.len(), 1);
        assert_eq!(p.toolpaths[0].side, Side::OnLine);
        assert!(!p.toolpaths[0].closed);
        // Each pass goes back to the start at the safe height.
        let rapids_to_start = p.gcode.matches("G0 X10.000 Y10.000").count();
        assert_eq!(rapids_to_start, 3, "{}", p.gcode);
        assert!((p.cut_length - 70.0 * 3.0).abs() < 1e-6);
    }

    #[test]
    fn passes_go_down_in_equal_steps_to_the_depth() {
        let levels = |depth: f64, pass: f64| {
            CncSpec {
                depth,
                pass_depth: pass,
                ..Default::default()
            }
            .pass_levels()
        };
        assert_eq!(levels(6.0, 2.0), vec![-2.0, -4.0, -6.0]);
        let l = levels(5.0, 2.0);
        assert_eq!(l.len(), 3);
        assert!((l[2] + 5.0).abs() < 1e-12);
        assert!(l.windows(2).all(|w| (w[0] - w[1]) <= 2.0 + 1e-12));
        assert_eq!(levels(1.0, 3.0), vec![-1.0]);
        // Nonsense settings still give a pass.
        assert_eq!(levels(f64::NAN, f64::NAN).len(), 3);
        assert_eq!(levels(6.0, 0.0).len(), 600);
        let p = program(
            &[vec![rect(0.0, 0.0, 10.0, 10.0)]],
            &CncSpec {
                depth: 6.0,
                pass_depth: 2.0,
                ..spec()
            },
            Point::ZERO,
        );
        assert_eq!(p.passes, 3);
        for z in ["Z-2.000", "Z-4.000", "Z-6.000"] {
            assert_eq!(p.gcode.matches(z).count(), 1, "{z}\n{}", p.gcode);
        }
        // One loop of 40 + 2 pi 3 each pass.
        let loop_len = 40.0 + 2.0 * std::f64::consts::PI * 3.0;
        assert!(
            (p.cut_length - 3.0 * loop_len).abs() < 0.5,
            "{}",
            p.cut_length
        );
    }

    #[test]
    fn the_program_has_its_header_and_footer() {
        let p = program(&[vec![rect(10.0, 20.0, 30.0, 40.0)]], &spec(), Point::ZERO);
        let lines: Vec<&str> = p.gcode.lines().collect();
        let at = |s: &str| lines.iter().position(|l| *l == s);
        let g21 = at("G21").expect("G21");
        let g90 = at("G90").expect("G90");
        let m3 = at("M3 S18000").expect("M3");
        let m5 = at("M5").expect("M5");
        assert!(g21 < g90 && g90 < m3 && m3 < m5);
        assert_eq!(lines.last().copied(), Some("M30"));
        assert!(lines[0].starts_with('('));
        // The safe height before the spindle starts and before it stops.
        assert_eq!(lines.get(m3 - 1).copied(), Some("G0 Z5.000"));
        assert!(lines[..m5].iter().rev().find(|l| l.starts_with("G0 Z")) == Some(&"G0 Z5.000"));
        // Feeds: the plunge rate going down, the feed rate along.
        assert!(p.gcode.contains("G1 Z-2.000 F300"));
        assert!(p.gcode.contains(" F1000"));
        // Coordinates from the job's bottom-left corner: the toolpath runs
        // 3 mm round the part, so it starts below and left of X0 Y0 at
        // the most.
        let xs: Vec<f64> = p
            .gcode
            .lines()
            .filter(|l| l.starts_with("G1 X"))
            .filter_map(|l| l.split_whitespace().nth(1)?.strip_prefix('X')?.parse().ok())
            .collect();
        let min = xs.iter().copied().fold(f64::INFINITY, f64::min);
        let max = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        assert!(
            (min + 3.0).abs() < 0.01 && (max - 23.0).abs() < 0.01,
            "{min} {max}"
        );
        // Without a spindle speed there are no spindle commands; nothing
        // is ever written as -0.000.
        let q = program(
            &[vec![rect(0.0, 0.0, 10.0, 10.0)]],
            &CncSpec {
                spindle: 0.0,
                zero: Zero::Origin,
                ..spec()
            },
            Point::ZERO,
        );
        assert!(!q.gcode.lines().any(|l| l.starts_with("M3 ") || l == "M5"));
        assert!(!q.gcode.contains("-0.000"));
        // Nothing to cut still gives a program that starts and ends well.
        let e = program(&[], &spec(), Point::ZERO);
        assert!(e.toolpaths.is_empty());
        assert!(e.gcode.contains("G21") && e.gcode.trim_end().ends_with("M30"));
    }

    #[test]
    fn tabs_lift_the_deep_passes_over_them() {
        let s = CncSpec {
            tabs: true,
            tab_count: 4,
            tab_width: 4.0,
            tab_height: 1.5,
            depth: 6.0,
            pass_depth: 2.0,
            ..spec()
        };
        let objects = [
            vec![rect(0.0, 0.0, 100.0, 100.0)],
            vec![rect(40.0, 40.0, 60.0, 60.0)],
        ];
        let p = program(&objects, &s, Point::ZERO);
        // Only the outside cut gets tabs.
        let outside: Vec<&Toolpath> = p
            .toolpaths
            .iter()
            .filter(|t| t.side == Side::Outside)
            .collect();
        let inside: Vec<&Toolpath> = p
            .toolpaths
            .iter()
            .filter(|t| t.side == Side::Inside)
            .collect();
        assert_eq!(outside.len(), 1);
        assert_eq!(outside[0].tabs.len(), 4);
        assert!(inside[0].tabs.is_empty());
        // Each tab is the width asked for plus the tool, spread evenly.
        let len = outside[0].length();
        for (i, (a, b)) in outside[0].tabs.iter().enumerate() {
            assert!((b - a - 10.0).abs() < 1e-9);
            assert!(((a + b) / 2.0 - (i as f64 + 0.5) * len / 4.0).abs() < 1e-9);
        }
        // The top of a tab is 1.5 mm above the bottom: the passes at -2
        // and -4 run over the tabs, the pass at -6 rises to -4.5 over each.
        let up = p.gcode.matches("G1 Z-4.500").count();
        assert_eq!(up, 4, "{}", p.gcode);
        // Down to -6 once in the hole, once in the outline and after each tab.
        assert_eq!(p.gcode.matches("G1 Z-6.000").count(), 1 + 1 + 4);
        // Tabs higher than the cut is deep are left out.
        let none = program(
            &objects,
            &CncSpec {
                tab_height: 10.0,
                ..s
            },
            Point::ZERO,
        );
        assert!(none.toolpaths.iter().all(|t| t.tabs.is_empty()));
        // Too many tabs for a short outline: fewer of them.
        assert!(tab_ranges(25.0, &s).len() == 2);
        assert!(tab_ranges(5.0, &s).is_empty());
    }

    #[test]
    fn damaged_input_gives_an_empty_job() {
        let mut bad = BezPath::new();
        bad.move_to((f64::NAN, 0.0));
        bad.line_to((1.0, 1.0));
        bad.line_to((0.0, 1.0));
        bad.close_path();
        let p = program(
            &[vec![bad, BezPath::new()]],
            &spec(),
            Point::new(f64::NAN, 0.0),
        );
        assert!(p.toolpaths.is_empty());
        assert!(p.gcode.trim_end().ends_with("M30"));
        let s = CncSpec {
            tool_diameter: f64::INFINITY,
            feed: -5.0,
            ..Default::default()
        }
        .clamped();
        assert!(s.tool_diameter.is_finite() && s.feed >= 1.0);
    }
    /// The Z of every straight-down `G1` move.
    fn plunges(gcode: &str) -> Vec<f64> {
        gcode
            .lines()
            .filter(|l| l.starts_with("G1 Z"))
            .filter_map(|l| l[4..].split(' ').next()?.parse().ok())
            .collect()
    }

    #[test]
    fn a_smooth_ramp_goes_down_along_the_path_not_straight_down() {
        let objs = [vec![rect(0.0, 0.0, 40.0, 40.0)]];
        let plain = program(&objs, &spec(), Point::ZERO);
        let ramped = program(
            &objs,
            &CncSpec {
                ramp: Ramp::Smooth,
                ramp_length: 10.0,
                ..spec()
            },
            Point::ZERO,
        );
        // Only the move down to the top of the material is straight.
        assert!(plunges(&ramped.gcode).iter().all(|z| *z >= -1e-9), "{}", ramped.gcode);
        assert!(plunges(&plain.gcode).iter().any(|z| *z < -1.0));
        assert!(ramped.gcode.lines().any(|l| l.contains(" X") && l.contains(" Z-")));
        assert!(ramped.gcode.contains("Z-6.000"));
        // Each pass cuts its ramp again at depth.
        let extra = ramped.cut_length - plain.cut_length;
        assert!((extra - 3.0 * 10.0).abs() < 0.5, "{extra}");
    }

    #[test]
    fn a_zigzag_ramp_comes_back_to_the_start_at_depth() {
        let mut line = BezPath::new();
        line.move_to((0.0, 0.0));
        line.line_to((50.0, 0.0));
        let p = program(
            &[vec![line]],
            &CncSpec {
                ramp: Ramp::ZigZag,
                ramp_length: 8.0,
                depth: 2.0,
                pass_depth: 2.0,
                ..spec()
            },
            Point::ZERO,
        );
        assert!(plunges(&p.gcode).iter().all(|z| *z >= -1e-9), "{}", p.gcode);
        let lines: Vec<&str> = p.gcode.lines().collect();
        // Half way down at the far end of the ramp, all the way back at the start.
        assert!(lines.iter().any(|l| l.starts_with("G1 X8.000 Y0.000 Z-1.000")), "{}", p.gcode);
        assert!(lines.iter().any(|l| l.starts_with("G1 X0.000 Y0.000 Z-2.000")), "{}", p.gcode);
        assert!(lines.iter().any(|l| l.starts_with("G1 X50.000 Y0.000")));
    }

    #[test]
    fn leads_come_in_from_the_waste_side() {
        let part = rect(0.0, 0.0, 60.0, 60.0);
        let hole = rect(20.0, 20.0, 40.0, 40.0);
        for lead in [Lead::Line, Lead::Arc] {
            let p = program(
                &[vec![part.clone(), hole.clone()]],
                &CncSpec {
                    lead,
                    lead_length: 4.0,
                    ..spec()
                },
                Point::ZERO,
            );
            assert_eq!(p.leads.len(), 4, "{lead:?}");
            for (t, pair) in p.toolpaths.iter().zip(p.leads.chunks(2)) {
                let start = t.pts[0];
                let (lin, lout) = (&pair[0], &pair[1]);
                assert!((*lin.last().unwrap() - start).hypot() < 1e-9);
                assert!((lout[0] - start).hypot() < 1e-9);
                let r = Rect::new(0.0, 0.0, 60.0, 60.0);
                let h = Rect::new(20.0, 20.0, 40.0, 40.0);
                for q in lin[..lin.len() - 1].iter().chain(&lout[1..]) {
                    if t.side == Side::Inside {
                        assert!(h.contains(*q), "{lead:?} {q:?}");
                    } else {
                        assert!(!r.inflate(2.9, 2.9).contains(*q), "{lead:?} {q:?}");
                    }
                }
                // The tool goes in at the far end of the lead-in.
                let gx = format!("G0 X{} Y{}", num(lin[0].x), num(lin[0].y));
                assert!(p.gcode.contains(&gx), "{lead:?}\n{}", p.gcode);
            }
        }
    }

    #[test]
    fn a_lead_too_long_for_a_hole_is_shortened() {
        let p = program(
            &[vec![rect(0.0, 0.0, 60.0, 60.0), circle(Point::new(30.0, 30.0), 6.0)]],
            &CncSpec {
                lead: Lead::Line,
                lead_length: 20.0,
                ..spec()
            },
            Point::ZERO,
        );
        let hole = &p.leads[0];
        // The tool centre runs 3 mm inside the 6 mm hole.
        assert!((hole[0] - Point::new(30.0, 30.0)).hypot() < 3.0, "{hole:?}");
    }

    #[test]
    fn leads_and_a_ramp_go_down_along_the_lead() {
        let p = program(
            &[vec![rect(0.0, 0.0, 40.0, 40.0)]],
            &CncSpec {
                lead: Lead::Arc,
                ramp: Ramp::Smooth,
                ..spec()
            },
            Point::ZERO,
        );
        assert!(plunges(&p.gcode).iter().all(|z| *z >= -4.0 - 1e-9), "{}", p.gcode);
        assert!(p.gcode.contains("Z-6.000"));
    }

}
