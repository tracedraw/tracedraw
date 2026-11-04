//! G-code for routers, lasers and hot wire cutters.
//!
//! A [`Machine`] says how the text looks (header, footer, tool change,
//! comments, decimals, whether it has a Z axis), a [`Tool`] how fast and
//! deep it cuts, and a [`Job`] what to do with the outlines of one
//! outline colour. [`plan_job`] turns outlines into [`Cut`]s (tool paths
//! in page space), [`emit`] writes them as G-code in machine
//! coordinates. Only `G0` and `G1` are written, in millimetres; arcs of
//! lead-ins are written as short straight moves.

use super::{flatten, nesting_depths, signed_area, Contour};
use tracedraw_core::geometry::{BezPath, Point, Rect, Vec2};
use tracedraw_core::shaping::{self, Join};

/// Flattening tolerance of tool paths (mm).
pub const TOLERANCE: f64 = 0.02;

/// Where the machine's zero lies on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Origin {
    #[default]
    BottomLeft,
    BottomRight,
    TopLeft,
    TopRight,
    Center,
}

impl Origin {
    pub const ALL: [Origin; 5] = [
        Origin::BottomLeft,
        Origin::BottomRight,
        Origin::TopLeft,
        Origin::TopRight,
        Origin::Center,
    ];

    /// What to add to a page point to get machine coordinates, for a
    /// page of `w` x `h` mm.
    pub fn offset(self, w: f64, h: f64) -> Vec2 {
        match self {
            Origin::BottomLeft => Vec2::ZERO,
            Origin::BottomRight => Vec2::new(-w, 0.0),
            Origin::TopLeft => Vec2::new(0.0, -h),
            Origin::TopRight => Vec2::new(-w, -h),
            Origin::Center => Vec2::new(-w / 2.0, -h / 2.0),
        }
    }

    /// The table of `w` x `h` mm in machine coordinates, its zero at
    /// this corner.
    pub fn table(self, w: f64, h: f64) -> Rect {
        let o = self.offset(w, h);
        Rect::new(o.x, o.y, o.x + w, o.y + h)
    }
}

/// How comments are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum CommentStyle {
    /// `(text)`
    #[default]
    Parentheses,
    /// `; text`
    Semicolon,
    /// No comments.
    Off,
}

/// A machine: its table and how its G-code looks.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Machine {
    pub name: String,
    /// Table size (mm); a warning is given when tool paths leave it.
    pub width: f64,
    pub height: f64,
    pub origin: Origin,
    /// Lines written first and last.
    pub header: String,
    pub footer: String,
    /// Lines written at each change of tool. Placeholders: `{tool}`
    /// (number from 1), `{name}`, `{diameter}`, `{rpm}`, `{feed}`.
    pub tool_change: String,
    /// With a Z axis the tool plunges and retracts; without one (laser,
    /// hot wire) `beam_on` and `beam_off` start and stop cutting.
    pub z_axis: bool,
    pub beam_on: String,
    pub beam_off: String,
    pub comments: CommentStyle,
    /// Decimals of coordinates (0 to 6).
    pub decimals: usize,
}

impl Machine {
    /// A router with a Z axis.
    pub fn router() -> Self {
        Machine {
            name: "Router".into(),
            width: 1300.0,
            height: 2500.0,
            origin: Origin::BottomLeft,
            header: "G21\nG90\nG17".into(),
            footer: "M5\nM30".into(),
            tool_change: "T{tool} M6\nM3 S{rpm}".into(),
            z_axis: true,
            beam_on: "M3 S{rpm}".into(),
            beam_off: "M5".into(),
            comments: CommentStyle::Parentheses,
            decimals: 3,
        }
    }

    /// A laser: no Z moves, the beam switched on and off.
    pub fn laser() -> Self {
        Machine {
            name: "Laser".into(),
            width: 1300.0,
            height: 900.0,
            origin: Origin::BottomLeft,
            header: "G21\nG90".into(),
            footer: "M5\nM2".into(),
            tool_change: String::new(),
            z_axis: false,
            beam_on: "M3 S{rpm}".into(),
            beam_off: "M5".into(),
            comments: CommentStyle::Semicolon,
            decimals: 3,
        }
    }

    /// A hot wire foam cutter: XY only, the wire heated all along.
    pub fn hot_wire() -> Self {
        Machine {
            name: "Hot wire".into(),
            width: 1000.0,
            height: 1000.0,
            origin: Origin::BottomLeft,
            header: "G21\nG90".into(),
            footer: "M2".into(),
            tool_change: String::new(),
            z_axis: false,
            beam_on: "M3 S{rpm}".into(),
            beam_off: "M5".into(),
            comments: CommentStyle::Parentheses,
            decimals: 2,
        }
    }
}

impl Default for Machine {
    fn default() -> Self {
        Machine::router()
    }
}

/// A cutting tool (an end mill, a laser beam, a hot wire).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Tool {
    pub name: String,
    /// Diameter (mm): the cut side offsets by half of it.
    pub diameter: f64,
    /// Spindle speed (or laser power, wire heat) for `{rpm}`.
    pub rpm: f64,
    /// Feed along the cut and down into the material (mm/min).
    pub feed: f64,
    pub plunge: f64,
    /// Deepest cut of one pass (mm); 0: no limit.
    pub max_depth: f64,
    /// Height of travel moves above the material (mm).
    pub safe_z: f64,
    /// Go to the park position at the end.
    pub park: bool,
    pub park_x: f64,
    pub park_y: f64,
}

impl Tool {
    pub fn end_mill(diameter: f64) -> Self {
        Tool {
            name: format!("End mill {diameter} mm"),
            diameter,
            rpm: 18000.0,
            feed: 1200.0,
            plunge: 300.0,
            max_depth: (diameter / 2.0).max(0.5),
            safe_z: 5.0,
            park: false,
            park_x: 0.0,
            park_y: 0.0,
        }
    }

    pub fn laser_beam() -> Self {
        Tool {
            name: "Laser beam".into(),
            diameter: 0.1,
            rpm: 1000.0,
            feed: 1500.0,
            plunge: 1500.0,
            max_depth: 0.0,
            safe_z: 0.0,
            park: false,
            park_x: 0.0,
            park_y: 0.0,
        }
    }

    pub fn wire() -> Self {
        Tool {
            name: "Hot wire".into(),
            diameter: 0.0,
            rpm: 50.0,
            feed: 300.0,
            plunge: 300.0,
            max_depth: 0.0,
            safe_z: 0.0,
            park: false,
            park_x: 0.0,
            park_y: 0.0,
        }
    }
}

impl Default for Tool {
    fn default() -> Self {
        Tool::end_mill(3.0)
    }
}

/// Which side of the outline the tool runs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Side {
    /// The tool's centre on the outline (engraving, lasers).
    On,
    /// Outside: the part keeps its size (holes are cut inside them).
    #[default]
    Outside,
    /// Inside: the opening keeps its size.
    Inside,
}

/// Direction of travel for a spindle turning clockwise (seen from
/// above).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Direction {
    /// The kept material on the right of the travel.
    #[default]
    Climb,
    /// The kept material on the left.
    Conventional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum LeadIn {
    #[default]
    None,
    /// A straight line square to the outline, from the waste side.
    Straight,
    /// A quarter circle that meets the outline along it.
    Arc,
}

/// What to do with the outlines of one colour.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Job {
    /// The outline colour of the objects this job cuts.
    pub color: [u8; 3],
    pub enabled: bool,
    /// Index in the tool list.
    pub tool: usize,
    /// Total depth (mm, positive down); 0 for a laser.
    pub depth: f64,
    /// Passes the depth is split into evenly (more when the tool's
    /// deepest pass asks for it).
    pub passes: u32,
    pub side: Side,
    pub direction: Direction,
    /// Outlines inside others are cut before them.
    pub inner_first: bool,
    pub lead_in: LeadIn,
    pub lead_length: f64,
    /// Bridges per closed outline (0: none), their length and height.
    pub bridges: u32,
    pub bridge_length: f64,
    pub bridge_height: f64,
}

impl Job {
    pub fn new(color: [u8; 3]) -> Self {
        Job {
            color,
            enabled: true,
            tool: 0,
            depth: 3.0,
            passes: 1,
            side: Side::Outside,
            direction: Direction::Climb,
            inner_first: true,
            lead_in: LeadIn::None,
            lead_length: 3.0,
            bridges: 0,
            bridge_length: 4.0,
            bridge_height: 1.0,
        }
    }
}

impl Default for Job {
    fn default() -> Self {
        Job::new([0, 0, 0])
    }
}

/// One tool path: a contour cut in one or more passes, in page space.
#[derive(Debug, Clone, PartialEq)]
pub struct Cut {
    pub tool: usize,
    /// The points cut; a closed contour repeats its first point at the
    /// end.
    pub path: Vec<Point>,
    pub closed: bool,
    /// Lead-in points before `path[0]`, from where the tool goes down.
    pub lead: Vec<Point>,
    /// Z of each pass (0 or below).
    pub depths: Vec<f64>,
    /// Stretches along `path` (arc length from its start) left as
    /// bridges, and the Z the tool rises to over them.
    pub bridges: Vec<(f64, f64)>,
    pub bridge_z: f64,
}

impl Cut {
    /// The whole path from where the tool goes down: lead-in and cut.
    pub fn preview(&self) -> Vec<Point> {
        let mut v = self.lead.clone();
        v.extend(self.path.iter().copied());
        v
    }

    pub fn length(&self) -> f64 {
        super::polyline_length(&self.preview(), false)
    }
}

/// A contour with the side its kept material lies on.
struct Item {
    c: Contour,
    kept_inside: bool,
    area: f64,
}

/// The Z of each pass for `depth` with `passes` asked and the tool's
/// deepest pass.
pub fn pass_depths(depth: f64, passes: u32, max_depth: f64) -> Vec<f64> {
    let d = if depth.is_finite() {
        depth.max(0.0)
    } else {
        0.0
    };
    let mut n = passes.clamp(1, 1000);
    if d > 0.0 && max_depth.is_finite() && max_depth > 0.0 {
        let need = (d / max_depth - 1e-9).ceil();
        if need.is_finite() && need > f64::from(n) {
            n = (need as u32).clamp(1, 1000);
        }
    }
    (1..=n)
        .map(|k| {
            let z = -d * f64::from(k) / f64::from(n);
            if z == 0.0 {
                0.0
            } else {
                z
            }
        })
        .collect()
}

/// Turn the outlines of one job into tool paths, starting from `from`
/// (the tool's position). Returns the cuts and where the tool ends.
pub fn plan_job(job: &Job, tool: &Tool, outlines: &[BezPath], from: Point) -> (Vec<Cut>, Point) {
    let r = if tool.diameter.is_finite() {
        tool.diameter.max(0.0) / 2.0
    } else {
        0.0
    };
    let mut items: Vec<Item> = Vec::new();
    for path in outlines {
        let all = flatten(path, TOLERANCE);
        let (closed, open): (Vec<Contour>, Vec<Contour>) = all.into_iter().partition(|c| c.closed);
        for c in open {
            items.push(Item {
                c,
                kept_inside: true,
                area: 0.0,
            });
        }
        if closed.is_empty() {
            continue;
        }
        let offset = match job.side {
            Side::On => 0.0,
            Side::Outside => r,
            Side::Inside => -r,
        };
        let contours = if offset.abs() > 1e-9 {
            let mut p = BezPath::new();
            for c in &closed {
                p.extend(c.to_path());
            }
            flatten(&shaping::offset_with(&p, offset, Join::Round), TOLERANCE)
        } else {
            closed
        };
        let depths = nesting_depths(&contours);
        for (c, d) in contours.into_iter().zip(depths) {
            let filled_inside = d % 2 == 0;
            let kept_inside = match job.side {
                Side::On | Side::Outside => filled_inside,
                Side::Inside => !filled_inside,
            };
            let area = signed_area(&c.pts).abs();
            items.push(Item {
                c,
                kept_inside,
                area,
            });
        }
    }
    // Which contours lie inside which (by a point, larger ones only).
    let inside_of: Vec<Vec<usize>> = (0..items.len())
        .map(|i| {
            let Some(probe) = items[i].c.pts.first().copied() else {
                return Vec::new();
            };
            (0..items.len())
                .filter(|&j| {
                    j != i
                        && items[j].c.closed
                        && items[j].area > items[i].area
                        && super::contains(&items[j].c.pts, probe)
                })
                .collect()
        })
        .collect();
    let depths = pass_depths(job.depth, job.passes, tool.max_depth);
    let total = depths.last().copied().unwrap_or(0.0);
    let bridge_z = (total + job.bridge_height.max(0.0)).min(0.0);
    let mut done = vec![false; items.len()];
    let mut pos = from;
    let mut cuts = Vec::new();
    for _ in 0..items.len() {
        // Ready: every contour inside it is cut (or the order is free).
        let mut best: Option<(usize, usize, f64)> = None;
        for i in 0..items.len() {
            if done[i] {
                continue;
            }
            if job.inner_first
                && (0..items.len()).any(|j| !done[j] && j != i && inside_of[j].contains(&i))
            {
                continue;
            }
            let c = &items[i].c;
            let (k, d) = if c.closed {
                nearest_vertex(&c.pts, pos)
            } else {
                let a = c.pts.first().map_or(f64::INFINITY, |p| (*p - pos).hypot());
                let b = c.pts.last().map_or(f64::INFINITY, |p| (*p - pos).hypot());
                if b < a {
                    (1, b)
                } else {
                    (0, a)
                }
            };
            if best.is_none_or(|(_, _, bd)| d < bd) {
                best = Some((i, k, d));
            }
        }
        let Some((i, k, _)) = best else {
            break;
        };
        done[i] = true;
        let item = &items[i];
        let cut = make_cut(job, tool, item, k, &depths, bridge_z);
        if let Some(p) = cut.path.last() {
            pos = *p;
        }
        cuts.push(cut);
    }
    (cuts, pos)
}

fn nearest_vertex(pts: &[Point], p: Point) -> (usize, f64) {
    let mut best = (0, f64::INFINITY);
    for (i, q) in pts.iter().enumerate() {
        let d = (*q - p).hypot();
        if d < best.1 {
            best = (i, d);
        }
    }
    best
}

fn make_cut(
    job: &Job,
    tool: &Tool,
    item: &Item,
    start: usize,
    depths: &[f64],
    bridge_z: f64,
) -> Cut {
    let c = &item.c;
    let mut lead = Vec::new();
    let mut bridges = Vec::new();
    let path = if c.closed {
        let n = c.pts.len();
        let mut pts: Vec<Point> = (0..n).map(|k| c.pts[(start + k) % n]).collect();
        let ccw = signed_area(&pts) > 0.0;
        // Climb: the kept material on the right of the travel; a
        // counter-clockwise loop has its inside on the left.
        let want_ccw = match job.direction {
            Direction::Climb => !item.kept_inside,
            Direction::Conventional => item.kept_inside,
        };
        if ccw != want_ccw {
            pts[1..].reverse();
        }
        let kept_left = want_ccw == item.kept_inside;
        if let Some(first) = pts.first().copied() {
            pts.push(first);
        }
        if job.lead_in != LeadIn::None && job.lead_length.is_finite() && job.lead_length > 0.0 {
            lead = lead_in(&pts, job.lead_in, job.lead_length, kept_left);
        }
        let len = super::polyline_length(&pts, false);
        let b = job.bridges.min(1000);
        let zone = job.bridge_length.max(0.0) + tool.diameter.max(0.0);
        if b > 0
            && job.bridge_length.is_finite()
            && job.bridge_length > 0.0
            && zone.is_finite()
            && f64::from(b) * zone < len * 0.8
        {
            for i in 0..b {
                let mid = (f64::from(i) + 0.5) * len / f64::from(b);
                bridges.push((mid - zone / 2.0, mid + zone / 2.0));
            }
        }
        pts
    } else if start == 1 {
        c.pts.iter().rev().copied().collect()
    } else {
        c.pts.clone()
    };
    Cut {
        tool: job.tool,
        path,
        closed: c.closed,
        lead,
        depths: depths.to_vec(),
        bridges,
        bridge_z,
    }
}

/// Lead-in points ending just before `path[0]`, coming from the waste
/// side (the side away from the kept material).
fn lead_in(path: &[Point], kind: LeadIn, len: f64, kept_left: bool) -> Vec<Point> {
    let (Some(p0), Some(p1)) = (path.first().copied(), path.get(1).copied()) else {
        return Vec::new();
    };
    let d = p1 - p0;
    let l = d.hypot();
    if l.is_nan() || l <= 1e-9 {
        return Vec::new();
    }
    let d = d / l;
    let left = Vec2::new(-d.y, d.x);
    let waste = if kept_left { -left } else { left };
    match kind {
        LeadIn::None => Vec::new(),
        LeadIn::Straight => vec![p0 + waste * len],
        LeadIn::Arc => {
            let c = p0 + waste * len;
            (0..8)
                .map(|k| {
                    let phi = f64::from(k) / 8.0 * std::f64::consts::FRAC_PI_2;
                    c + (-d * phi.cos() - waste * phi.sin()) * len
                })
                .collect()
        }
    }
}

/// What a set of cuts amounts to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Summary {
    pub cuts: usize,
    /// Cut length over all passes (mm).
    pub length: f64,
    /// Bounds of the tool paths in machine coordinates.
    pub bounds: Option<Rect>,
    /// Every tool path lies on the machine's table.
    pub on_table: bool,
}

pub fn summary(m: &Machine, cuts: &[Cut], page_w: f64, page_h: f64) -> Summary {
    let off = m.origin.offset(page_w, page_h);
    let mut bounds: Option<Rect> = None;
    let mut length = 0.0;
    for c in cuts {
        length += c.length() * c.depths.len().max(1) as f64;
        for p in c.preview() {
            let q = p + off;
            let r = Rect::from_points(q, q);
            bounds = Some(bounds.map_or(r, |b| b.union(r)));
        }
    }
    let table = m.origin.table(m.width, m.height);
    let on_table = bounds.is_none_or(|b| {
        b.x0 >= table.x0 - 1e-6
            && b.y0 >= table.y0 - 1e-6
            && b.x1 <= table.x1 + 1e-6
            && b.y1 <= table.y1 + 1e-6
    });
    Summary {
        cuts: cuts.len(),
        length,
        bounds,
        on_table,
    }
}

/// A number with `d` decimals, never `-0`.
pub fn num(v: f64, d: usize) -> String {
    let s = format!("{:.*}", d.min(6), v);
    if s.starts_with('-') && s[1..].chars().all(|c| c == '0' || c == '.') {
        s[1..].to_string()
    } else {
        s
    }
}

/// A feed or speed: whole numbers without decimals, else one.
fn rate(v: f64) -> String {
    let v = if v.is_finite() { v.max(0.0) } else { 0.0 };
    if (v - v.round()).abs() < 1e-9 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.1}")
    }
}

struct Writer<'a> {
    m: &'a Machine,
    off: Vec2,
    out: String,
    z: Option<f64>,
    feed: Option<f64>,
    beam: bool,
}

impl<'a> Writer<'a> {
    fn new(m: &'a Machine, page_w: f64, page_h: f64) -> Self {
        Writer {
            m,
            off: m.origin.offset(page_w, page_h),
            out: String::new(),
            z: None,
            feed: None,
            beam: false,
        }
    }

    fn line(&mut self, s: &str) {
        self.out.push_str(s);
        self.out.push('\n');
    }

    fn comment(&mut self, text: &str) {
        let t: String = text
            .chars()
            .map(|c| match c {
                '(' => '[',
                ')' => ']',
                '\n' | '\r' => ' ',
                c => c,
            })
            .collect();
        match self.m.comments {
            CommentStyle::Parentheses => self.line(&format!("({t})")),
            CommentStyle::Semicolon => self.line(&format!("; {t}")),
            CommentStyle::Off => {}
        }
    }

    /// Template lines with the tool's values filled in.
    fn lines(&mut self, text: &str, tool: Option<(usize, &Tool)>) {
        for l in text.lines() {
            let l = l.trim();
            if l.is_empty() {
                continue;
            }
            let l = match tool {
                Some((i, t)) => l
                    .replace("{tool}", &(i + 1).to_string())
                    .replace("{name}", &t.name)
                    .replace("{diameter}", &num(t.diameter, self.m.decimals))
                    .replace("{rpm}", &rate(t.rpm))
                    .replace("{feed}", &rate(t.feed)),
                None => l.to_string(),
            };
            self.line(&l);
        }
    }

    fn xy(&self, p: Point) -> String {
        let q = p + self.off;
        format!(
            "X{} Y{}",
            num(q.x, self.m.decimals),
            num(q.y, self.m.decimals)
        )
    }

    fn rapid(&mut self, p: Point) {
        if p.x.is_finite() && p.y.is_finite() {
            let s = format!("G0 {}", self.xy(p));
            self.line(&s);
        }
    }

    fn feed_to(&mut self, p: Point, feed: f64) {
        if !(p.x.is_finite() && p.y.is_finite()) {
            return;
        }
        let mut s = format!("G1 {}", self.xy(p));
        if self.feed != Some(feed) {
            s.push_str(&format!(" F{}", rate(feed)));
            self.feed = Some(feed);
        }
        self.line(&s);
    }

    fn z_to(&mut self, z: f64, feed: f64) {
        if !self.m.z_axis || self.z == Some(z) {
            return;
        }
        let mut s = format!("G1 Z{}", num(z, self.m.decimals));
        if self.feed != Some(feed) {
            s.push_str(&format!(" F{}", rate(feed)));
            self.feed = Some(feed);
        }
        self.line(&s);
        self.z = Some(z);
    }

    fn retract(&mut self, tool: &Tool) {
        if self.m.z_axis {
            if self.z != Some(tool.safe_z) {
                let s = format!("G0 Z{}", num(tool.safe_z, self.m.decimals));
                self.line(&s);
                self.z = Some(tool.safe_z);
            }
        } else {
            self.beam_set(false, tool);
        }
    }

    fn beam_set(&mut self, on: bool, tool: &Tool) {
        if self.m.z_axis || self.beam == on {
            return;
        }
        let text = if on {
            self.m.beam_on.clone()
        } else {
            self.m.beam_off.clone()
        };
        self.lines(&text, Some((0, tool)));
        self.beam = on;
    }

    /// Start cutting at the current point at `z`.
    fn engage(&mut self, z: f64, tool: &Tool) {
        if self.m.z_axis {
            self.z_to(z, tool.plunge);
        } else {
            self.beam_set(true, tool);
        }
    }

    fn cut(&mut self, cut: &Cut, tool: &Tool) {
        let Some(entry) = cut.lead.first().or(cut.path.first()).copied() else {
            return;
        };
        for (k, &z) in cut.depths.iter().enumerate() {
            let in_place = k > 0 && cut.closed && cut.lead.is_empty();
            if !in_place {
                self.retract(tool);
                self.rapid(entry);
            }
            self.engage(z, tool);
            if !cut.lead.is_empty() {
                for p in cut.lead.iter().skip(1).chain(cut.path.first()) {
                    self.feed_to(*p, tool.feed);
                }
            }
            self.contour(cut, tool, z);
            let next_in_place = cut.closed && cut.lead.is_empty();
            if !next_in_place {
                self.retract(tool);
            }
        }
        self.retract(tool);
    }

    /// Along the cut, rising over the bridges.
    fn contour(&mut self, cut: &Cut, tool: &Tool, z: f64) {
        let use_bridges = !cut.bridges.is_empty() && (!self.m.z_axis || z < cut.bridge_z - 1e-9);
        let in_zone = |s: f64| cut.bridges.iter().any(|(a, b)| s > *a && s < *b);
        let mut lifted = false;
        let mut s = 0.0;
        for w in cut.path.windows(2) {
            let (a, b) = (w[0], w[1]);
            let len = (b - a).hypot();
            if use_bridges && len > 1e-12 {
                let mut at: Vec<f64> = cut
                    .bridges
                    .iter()
                    .flat_map(|(x0, x1)| [*x0, *x1])
                    .filter(|x| *x > s + 1e-9 && *x < s + len - 1e-9)
                    .collect();
                at.sort_by(f64::total_cmp);
                for x in at {
                    let p = a.lerp(b, (x - s) / len);
                    self.step(p, lifted, tool);
                    let now = in_zone(x + 1e-7);
                    if now != lifted {
                        self.lift(now, z, cut.bridge_z, tool);
                        lifted = now;
                    }
                }
            }
            self.step(b, lifted, tool);
            s += len;
        }
        if lifted {
            self.lift(false, z, cut.bridge_z, tool);
        }
    }

    fn step(&mut self, p: Point, lifted: bool, tool: &Tool) {
        if lifted && !self.m.z_axis {
            self.rapid(p);
        } else {
            self.feed_to(p, tool.feed);
        }
    }

    fn lift(&mut self, up: bool, z: f64, bridge_z: f64, tool: &Tool) {
        if self.m.z_axis {
            self.z_to(if up { bridge_z } else { z }, tool.plunge);
        } else {
            self.beam_set(!up, tool);
        }
    }
}

/// The G-code of `cuts` (in order) for machine `m` and its tools, the
/// page `page_w` x `page_h` mm placing the machine's zero.
pub fn emit(m: &Machine, tools: &[Tool], cuts: &[Cut], page_w: f64, page_h: f64) -> String {
    let mut g = Writer::new(m, page_w, page_h);
    g.comment(&m.name);
    g.lines(&m.header, None);
    let mut current: Option<usize> = None;
    for cut in cuts {
        let Some(tool) = tools.get(cut.tool) else {
            continue;
        };
        if current != Some(cut.tool) {
            if let Some(prev) = current.and_then(|i| tools.get(i)) {
                g.retract(prev);
            }
            g.comment(&format!(
                "Tool {}: {}, diameter {}",
                cut.tool + 1,
                tool.name,
                num(tool.diameter, m.decimals)
            ));
            g.lines(&m.tool_change, Some((cut.tool, tool)));
            current = Some(cut.tool);
        }
        g.cut(cut, tool);
    }
    if let Some(tool) = current.and_then(|i| tools.get(i)) {
        g.retract(tool);
        if tool.park {
            g.rapid(Point::new(tool.park_x, tool.park_y) - g.off);
        }
    }
    g.lines(&m.footer, None);
    g.out
}

/// G-code of one continuous path cut in XY only (a hot wire): to the
/// start, heat on, along the path at the tool's feed, heat off.
pub fn emit_xy(m: &Machine, tool: &Tool, path: &[Point], page_w: f64, page_h: f64) -> String {
    let xy = Machine {
        z_axis: false,
        ..m.clone()
    };
    let mut g = Writer::new(&xy, page_w, page_h);
    g.comment(&m.name);
    g.lines(&m.header, None);
    if let Some(first) = path.first() {
        g.rapid(*first);
        g.beam_set(true, tool);
        for p in path.iter().skip(1) {
            g.feed_to(*p, tool.feed);
        }
        g.beam_set(false, tool);
    }
    g.lines(&m.footer, None);
    g.out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::Shape as _;

    fn square(x: f64, y: f64, s: f64) -> BezPath {
        Rect::new(x, y, x + s, y + s).to_path(0.01)
    }

    fn router() -> Machine {
        Machine {
            header: "G21\nG90".into(),
            tool_change: "T{tool} M6\nM3 S{rpm}".into(),
            ..Machine::router()
        }
    }

    fn tool() -> Tool {
        Tool {
            name: "Mill".into(),
            diameter: 2.0,
            rpm: 10000.0,
            feed: 600.0,
            plunge: 100.0,
            max_depth: 0.0,
            safe_z: 5.0,
            ..Tool::default()
        }
    }

    #[test]
    fn pass_depths_split_evenly() {
        assert_eq!(pass_depths(3.0, 1, 0.0), vec![-3.0]);
        assert_eq!(pass_depths(3.0, 3, 0.0), vec![-1.0, -2.0, -3.0]);
        // The tool's deepest pass asks for more.
        assert_eq!(pass_depths(3.0, 1, 1.5), vec![-1.5, -3.0]);
        assert_eq!(pass_depths(0.0, 2, 1.0), vec![0.0, 0.0]);
        assert_eq!(pass_depths(f64::NAN, 0, f64::NAN), vec![0.0]);
    }

    #[test]
    fn golden_square_on_the_line() {
        let mut job = Job::new([0, 0, 0]);
        job.side = Side::On;
        job.depth = 1.0;
        job.direction = Direction::Conventional;
        let (cuts, end) = plan_job(&job, &tool(), &[square(0.0, 0.0, 10.0)], Point::ZERO);
        assert_eq!(cuts.len(), 1);
        assert_eq!(end, Point::ZERO);
        let g = emit(&router(), &[tool()], &cuts, 100.0, 100.0);
        let want = "(Router)\nG21\nG90\n(Tool 1: Mill, diameter 2.000)\nT1 M6\nM3 S10000\n\
G0 Z5.000\nG0 X0.000 Y0.000\nG1 Z-1.000 F100\nG1 X10.000 Y0.000 F600\n\
G1 X10.000 Y10.000\nG1 X0.000 Y10.000\nG1 X0.000 Y0.000\nG0 Z5.000\nM5\nM30\n";
        assert_eq!(g, want);
    }

    #[test]
    fn golden_two_passes_step_down_in_place_with_origin_and_park() {
        let mut job = Job::new([0, 0, 0]);
        job.side = Side::On;
        job.depth = 2.0;
        job.passes = 2;
        job.direction = Direction::Climb;
        let mut t = tool();
        t.park = true;
        t.park_x = 0.0;
        t.park_y = 0.0;
        let (cuts, _) = plan_job(&job, &t, &[square(10.0, 10.0, 5.0)], Point::ZERO);
        let m = Machine {
            origin: Origin::TopLeft,
            comments: CommentStyle::Off,
            decimals: 1,
            tool_change: String::new(),
            footer: "M30".into(),
            ..router()
        };
        let g = emit(&m, &[t], &cuts, 100.0, 50.0);
        // Climb on an outer outline: clockwise, from the corner nearest
        // the start; the page's top left is zero.
        let want = "G21\nG90\nG0 Z5.0\nG0 X10.0 Y-40.0\nG1 Z-1.0 F100\nG1 X10.0 Y-35.0 F600\n\
G1 X15.0 Y-35.0\nG1 X15.0 Y-40.0\nG1 X10.0 Y-40.0\nG1 Z-2.0 F100\nG1 X10.0 Y-35.0 F600\n\
G1 X15.0 Y-35.0\nG1 X15.0 Y-40.0\nG1 X10.0 Y-40.0\nG0 Z5.0\nG0 X0.0 Y0.0\nM30\n";
        assert_eq!(g, want);
    }

    #[test]
    fn golden_laser_without_z_and_bridges_as_gaps() {
        let mut job = Job::new([255, 0, 0]);
        job.side = Side::On;
        job.depth = 0.0;
        job.bridges = 1;
        job.bridge_length = 2.0;
        job.direction = Direction::Conventional;
        let mut t = Tool::laser_beam();
        t.diameter = 0.0;
        t.feed = 1000.0;
        t.rpm = 500.0;
        let (cuts, _) = plan_job(&job, &t, &[square(0.0, 0.0, 10.0)], Point::ZERO);
        assert_eq!(cuts[0].bridges, vec![(19.0, 21.0)]);
        let m = Machine {
            header: "G21".into(),
            footer: "M2".into(),
            comments: CommentStyle::Off,
            decimals: 0,
            ..Machine::laser()
        };
        let g = emit(&m, &[t], &cuts, 100.0, 100.0);
        let want = "G21\nG0 X0 Y0\nM3 S500\nG1 X10 Y0 F1000\nG1 X10 Y9\nM5\nG0 X10 Y10\n\
G0 X9 Y10\nM3 S500\nG1 X0 Y10\nG1 X0 Y0\nM5\nM2\n";
        assert_eq!(g, want);
    }

    #[test]
    fn bridges_lift_only_the_last_passes() {
        let mut job = Job::new([0, 0, 0]);
        job.side = Side::On;
        job.depth = 2.0;
        job.passes = 2;
        job.bridges = 2;
        job.bridge_length = 1.0;
        job.bridge_height = 1.0;
        let t = tool();
        let (cuts, _) = plan_job(&job, &t, &[square(0.0, 0.0, 20.0)], Point::ZERO);
        assert_eq!(cuts[0].bridges.len(), 2);
        assert!((cuts[0].bridge_z + 1.0).abs() < 1e-9);
        let g = emit(&router(), std::slice::from_ref(&t), &cuts, 100.0, 100.0);
        // The first pass at -1 runs over the bridges' top: no lift; the
        // second at -2 rises to -1 twice and goes down again after each.
        assert_eq!(g.matches("G1 Z-1.000 F100").count(), 3);
        assert_eq!(g.matches("G1 Z-2.000 F100").count(), 3);
        // Bridges too long for the outline are left out.
        job.bridge_length = 50.0;
        let (cuts, _) = plan_job(&job, &t, &[square(0.0, 0.0, 20.0)], Point::ZERO);
        assert!(cuts[0].bridges.is_empty());
    }

    #[test]
    fn outside_and_inside_offset_by_the_tool_radius() {
        let mut job = Job::new([0, 0, 0]);
        let t = tool();
        let (cuts, _) = plan_job(&job, &t, &[square(0.0, 0.0, 10.0)], Point::ZERO);
        let b = bounds(&cuts[0].path);
        assert!((b.width() - 12.0).abs() < 0.05, "{b:?}");
        job.side = Side::Inside;
        let (cuts, _) = plan_job(&job, &t, &[square(0.0, 0.0, 10.0)], Point::ZERO);
        let b = bounds(&cuts[0].path);
        assert!((b.width() - 8.0).abs() < 0.05, "{b:?}");
    }

    fn bounds(pts: &[Point]) -> Rect {
        pts.iter()
            .map(|p| Rect::from_points(*p, *p))
            .reduce(|a, b| a.union(b))
            .unwrap_or_default()
    }

    #[test]
    fn inner_outlines_are_cut_first_and_holes_run_the_other_way() {
        let mut ring = square(0.0, 0.0, 30.0);
        ring.extend(square(10.0, 10.0, 10.0));
        let mut job = Job::new([0, 0, 0]);
        job.side = Side::On;
        let (cuts, _) = plan_job(&job, &tool(), &[ring.clone()], Point::ZERO);
        assert_eq!(cuts.len(), 2);
        // The hole first even though the outer outline is nearer.
        assert!((bounds(&cuts[0].path).width() - 10.0).abs() < 1e-9);
        // Climb: the hole counter-clockwise, the outer outline clockwise.
        assert!(signed_area(&cuts[0].path) > 0.0);
        assert!(signed_area(&cuts[1].path) < 0.0);
        job.inner_first = false;
        let (cuts, _) = plan_job(&job, &tool(), &[ring], Point::ZERO);
        assert!((bounds(&cuts[0].path).width() - 30.0).abs() < 1e-9);
        // Conventional turns both round.
        job.direction = Direction::Conventional;
        let (cuts, _) = plan_job(&job, &tool(), &[square(0.0, 0.0, 5.0)], Point::ZERO);
        assert!(signed_area(&cuts[0].path) > 0.0);
    }

    #[test]
    fn lead_ins_come_from_the_waste_side() {
        let mut job = Job::new([0, 0, 0]);
        job.side = Side::On;
        job.lead_in = LeadIn::Straight;
        job.lead_length = 3.0;
        job.direction = Direction::Conventional;
        let (cuts, _) = plan_job(&job, &tool(), &[square(0.0, 0.0, 10.0)], Point::ZERO);
        // Counter-clockwise from (0, 0) along +x: the part is on the
        // left, the lead comes from below.
        assert_eq!(cuts[0].lead, vec![Point::new(0.0, -3.0)]);
        job.lead_in = LeadIn::Arc;
        let (cuts, _) = plan_job(&job, &tool(), &[square(0.0, 0.0, 10.0)], Point::ZERO);
        let first = cuts[0].lead[0];
        assert!((first.x + 3.0).abs() < 1e-9 && (first.y + 3.0).abs() < 1e-9);
        assert!(cuts[0].lead.iter().all(|p| p.y < 0.0));
        // Open paths have none and start at the nearer end.
        let mut line = BezPath::new();
        line.move_to((50.0, 0.0));
        line.line_to((5.0, 0.0));
        let (cuts, end) = plan_job(&job, &tool(), &[line], Point::ZERO);
        assert!(cuts[0].lead.is_empty());
        assert_eq!(cuts[0].path[0], Point::new(5.0, 0.0));
        assert_eq!(end, Point::new(50.0, 0.0));
    }

    #[test]
    fn tool_changes_and_table_check() {
        let mut a = Job::new([0, 0, 0]);
        a.side = Side::On;
        let mut b = a.clone();
        b.tool = 1;
        let tools = [tool(), Tool::end_mill(6.0)];
        let (mut cuts, end) = plan_job(&a, &tools[0], &[square(0.0, 0.0, 10.0)], Point::ZERO);
        let (more, _) = plan_job(&b, &tools[1], &[square(20.0, 0.0, 10.0)], end);
        cuts.extend(more);
        let g = emit(&router(), &tools, &cuts, 100.0, 100.0);
        assert!(g.contains("T1 M6") && g.contains("T2 M6\nM3 S18000"));
        let s = summary(&router(), &cuts, 100.0, 100.0);
        assert_eq!(s.cuts, 2);
        assert!((s.length - 80.0).abs() < 1e-9);
        assert!(s.on_table);
        let small = Machine {
            width: 25.0,
            ..router()
        };
        assert!(!summary(&small, &cuts, 100.0, 100.0).on_table);
    }

    #[test]
    fn golden_hot_wire_xy() {
        let m = Machine {
            header: "G21".into(),
            ..Machine::hot_wire()
        };
        let pts = [
            Point::new(0.0, 0.0),
            Point::new(5.0, 0.0),
            Point::new(5.0, 5.0),
        ];
        let g = emit_xy(&m, &Tool::wire(), &pts, 10.0, 10.0);
        let want = "(Hot wire)\nG21\nG0 X0.00 Y0.00\nM3 S50\nG1 X5.00 Y0.00 F300\n\
G1 X5.00 Y5.00\nM5\nM2\n";
        assert_eq!(g, want);
    }

    #[test]
    fn bad_input_gives_no_cuts_and_no_panic() {
        let mut p = BezPath::new();
        p.move_to((f64::NAN, 0.0));
        p.line_to((1.0, f64::INFINITY));
        let job = Job {
            depth: f64::NAN,
            lead_length: f64::NAN,
            bridge_length: f64::INFINITY,
            bridges: 3,
            ..Job::new([0, 0, 0])
        };
        let t = Tool {
            diameter: f64::NAN,
            ..tool()
        };
        let (cuts, _) = plan_job(&job, &t, &[p, BezPath::new()], Point::ZERO);
        assert!(cuts.is_empty());
        let (cuts, _) = plan_job(&job, &t, &[square(0.0, 0.0, 5.0)], Point::ZERO);
        assert_eq!(cuts.len(), 1);
        let g = emit(&router(), &[t], &cuts, f64::NAN, 10.0);
        assert!(g.ends_with("M30\n"));
        assert_eq!(num(-0.0001, 3), "0.000");
        assert_eq!(num(-1.5, 1), "-1.5");
        // A cut whose tool is missing is skipped.
        assert!(!emit(&router(), &[], &cuts, 1.0, 1.0).contains("G1"));
    }
}
