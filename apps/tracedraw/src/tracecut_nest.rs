//! TraceCut's Nesting and Measure tools: shape nesting and rectangle
//! nesting on sheets or a roll, and the perimeter and area, cutting time
//! and cost of the selection. The geometry is `tracedraw_tracecut::nest`,
//! `measure` and `calc`; this module keeps the tools' settings, works
//! their results out and turns nesting into commands. See
//! docs/behavior/tracecut.md.
//!
//! Adapted from the Sign & Cut nesting and calculation tools of #170 by
//! LaMka21.

use crate::app::App;
use crate::i18n::{tr, trf};
use crate::tracecut_cut::{all_closed, fmt_len, object_color, outer_area, AddStyle, Change, Changes};
use serde::{Deserialize, Serialize};
use tracedraw_core::geometry::{Affine, BezPath, Point, Rect, Shape as _, Size, Vec2};
use tracedraw_core::{Color, Command, Fill, Shape, ShapeId, ShapeKind, Stroke};
use tracedraw_tracecut::calc::cost::{
    self, Material, MaterialUnit, Quantities, Service, ServiceUnit,
};
use tracedraw_tracecut::calc::time::{self, TimeProfile, TravelOrder};
use tracedraw_tracecut::measure;
use tracedraw_tracecut::nest::rects::{self, RectItem, RectSettings};
use tracedraw_tracecut::nest::shapes::{self, Rotations, ShapeSettings};
use tracedraw_tracecut::nest::{Layout, SheetSpec, StockSheet};

/// What a nesting run makes besides moving the objects (#4460): copies
/// of each object, a page for each sheet, a bleed round each object,
/// cut lines and crop marks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NestOutput {
    /// Copies of each object nested (1: the objects themselves).
    pub copies: u32,
    /// Each sheet becomes a new page of its size, its objects on it.
    pub pages: bool,
    /// A bleed this far round each object, in its colour (mm, 0: none);
    /// the objects are nested with room for it.
    pub bleed: f64,
    /// A CutContour line round each object.
    pub cut_lines: bool,
    /// Crop marks at the corners of each object's box.
    pub crop_marks: bool,
}

impl Default for NestOutput {
    fn default() -> Self {
        NestOutput {
            copies: 1,
            pages: false,
            bleed: 0.0,
            cut_lines: false,
            crop_marks: false,
        }
    }
}

impl NestOutput {
    /// The bleed made usable.
    pub fn bleed_mm(&self) -> f64 {
        if self.bleed.is_finite() {
            self.bleed
                .clamp(crate::tracecut_cut::BLEED_RANGE.0, crate::tracecut_cut::BLEED_RANGE.1)
        } else {
            0.0
        }
    }

    /// Copies of each object, 1 to [`MAX_COPIES`].
    pub fn copies(&self) -> usize {
        self.copies.clamp(1, MAX_COPIES) as usize
    }

    /// Whether the layout is only the objects moved, as before #4460.
    fn plain(&self) -> bool {
        self.copies() == 1
            && !self.pages
            && self.bleed_mm() <= 0.0
            && !self.cut_lines
            && !self.crop_marks
    }

    /// The material with room for the bleed: the spacing grows by two
    /// bleeds and the margin by one.
    fn sheet(&self, s: &SheetSpec) -> SheetSpec {
        let b = self.bleed_mm();
        SheetSpec {
            spacing: s.spacing + 2.0 * b,
            margin: s.margin + b,
            ..*s
        }
    }
}

/// Copies of each object a nesting run makes at most.
pub const MAX_COPIES: u32 = 1000;

/// Sheet sizes a Shape Nesting stock lists besides the first, at most.
pub const MAX_MORE_SHEETS: usize = 12;

/// The length of a crop mark and its gap from the object's box (mm).
const CROP_LEN: f64 = 3.0;
const CROP_GAP: f64 = 1.0;

/// One object a nesting layout places.
#[derive(Debug, Clone, PartialEq)]
pub struct NestItem {
    pub id: ShapeId,
    /// A new copy of the object (else the object itself moves).
    pub copy: bool,
    pub sheet: usize,
    /// The page-space transform that takes it to its place.
    pub transform: Affine,
    /// Its bleed, in place: the area and the colour.
    pub bleed: Option<(BezPath, Color)>,
    /// Its cut line, in place.
    pub cut: Option<BezPath>,
}

/// A nesting layout as it will be applied (#4460).
#[derive(Debug, Clone, PartialEq)]
pub struct NestPlan {
    pub items: Vec<NestItem>,
    /// The page-space rectangle of each sheet.
    pub sheets: Vec<Rect>,
    /// The crop marks of each sheet.
    pub marks: Vec<Vec<BezPath>>,
    /// Each sheet becomes a new page.
    pub pages: bool,
    /// The sheets are drawn (not with pages: the page is the sheet).
    pub draw_sheets: bool,
}

/// Crop marks at the corners of `b`: two short lines at each corner,
/// `gap` out from the box, along its edges.
pub fn crop_marks(b: Rect, gap: f64) -> Vec<BezPath> {
    let mut out = Vec::new();
    let line = |a: Point, z: Point| {
        let mut p = BezPath::new();
        p.move_to(a);
        p.line_to(z);
        p
    };
    for (x, sx) in [(b.x0, -1.0), (b.x1, 1.0)] {
        for (y, sy) in [(b.y0, -1.0), (b.y1, 1.0)] {
            // Across: along the horizontal edge's line, outward in x.
            out.push(line(
                Point::new(x + sx * gap, y),
                Point::new(x + sx * (gap + CROP_LEN), y),
            ));
            out.push(line(
                Point::new(x, y + sy * gap),
                Point::new(x, y + sy * (gap + CROP_LEN)),
            ));
        }
    }
    out
}

/// Shape nesting: the material, the turns, holes, accuracy and how long
/// the search may take.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShapeNestSettings {
    pub sheet: SheetSpec,
    pub rotations: Rotations,
    /// Smaller objects may go into the holes of larger ones.
    pub holes: bool,
    /// Accuracy of the outlines (mm).
    pub tolerance: f64,
    /// Rounds of the improvement loop.
    pub rounds: usize,
    /// The improvement loop stops after this many seconds.
    pub seconds: f64,
    /// Draw the sheets (or the roll's length used) as rectangles.
    pub draw_sheets: bool,
    /// Sheets of the first size there are (0: as many as it takes).
    pub count: u32,
    /// More sizes of sheet in stock, used after the first (#4460).
    pub more_sheets: Vec<StockSheet>,
    /// Each sheet becomes the smallest size in stock that holds it.
    pub smallest: bool,
    pub output: NestOutput,
}

impl ShapeNestSettings {
    /// The sheets in stock, the first size first.
    pub fn stock(&self) -> Vec<StockSheet> {
        let mut v = vec![StockSheet {
            width: self.sheet.width,
            height: self.sheet.height,
            count: self.count,
        }];
        v.extend(self.more_sheets.iter().take(MAX_MORE_SHEETS).copied());
        v
    }
}

impl Default for ShapeNestSettings {
    fn default() -> Self {
        ShapeNestSettings {
            sheet: SheetSpec::default(),
            rotations: Rotations::Quarter,
            holes: true,
            tolerance: 0.5,
            rounds: 30,
            seconds: 2.0,
            draw_sheets: true,
            count: 0,
            more_sheets: Vec::new(),
            smallest: false,
            output: NestOutput::default(),
        }
    }
}

/// Rectangle nesting: the material and whether boxes may turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RectNestSettings {
    pub sheet: SheetSpec,
    pub rotate: bool,
    pub draw_sheets: bool,
    pub output: NestOutput,
}

impl Default for RectNestSettings {
    fn default() -> Self {
        RectNestSettings {
            sheet: SheetSpec::default(),
            rotate: true,
            draw_sheets: true,
            output: NestOutput::default(),
        }
    }
}

/// Cutting time: the machine's speeds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TimeSettings {
    /// Speed while cutting (mm/s).
    pub cut_speed: f64,
    /// Speed of the moves with the tool up (mm/s).
    pub travel_speed: f64,
    /// Lift and lower per outline (s).
    pub lift_time: f64,
    /// Cutting and travel times are multiplied by this.
    pub accel_factor: f64,
    pub passes: u32,
    /// The nearest outline next (else the stacking order).
    pub nearest: bool,
}

impl Default for TimeSettings {
    fn default() -> Self {
        let p = TimeProfile::default();
        TimeSettings {
            cut_speed: p.cut_speed,
            travel_speed: p.travel_speed,
            lift_time: p.lift_time,
            accel_factor: p.accel_factor,
            passes: p.passes,
            nearest: true,
        }
    }
}

impl TimeSettings {
    pub fn profile(&self) -> TimeProfile {
        TimeProfile {
            name: String::new(),
            cut_speed: self.cut_speed,
            travel_speed: self.travel_speed,
            lift_time: self.lift_time,
            accel_factor: self.accel_factor,
            passes: self.passes,
        }
        .sanitized()
    }
}

/// Cost: the material and the prices billed on the job. Prices carry no
/// currency; `currency` is shown after the amounts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CostSettings {
    /// The material is sold by the running metre of a roll (else by the
    /// square metre).
    pub by_roll: bool,
    /// Price per square metre, or per running metre of roll.
    pub material_price: f64,
    /// Width of the roll (mm).
    pub roll_width: f64,
    /// Per metre of outline cut.
    pub cut_price: f64,
    /// Per square metre weeded.
    pub weed_price: f64,
    /// Per object.
    pub piece_price: f64,
    /// Per hour of cutting time (from the Cutting Time settings).
    pub hour_price: f64,
    /// Added on every side of each object's box (mm).
    pub reserve: f64,
    pub copies: u32,
    pub currency: String,
}

impl Default for CostSettings {
    fn default() -> Self {
        CostSettings {
            by_roll: true,
            material_price: 6.0,
            roll_width: 630.0,
            cut_price: 0.5,
            weed_price: 12.0,
            piece_price: 0.0,
            hour_price: 0.0,
            reserve: 5.0,
            copies: 1,
            currency: String::new(),
        }
    }
}

/// A result to read: lines for the window, the first one for the status
/// line.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Report {
    pub lines: Vec<String>,
}

/// An area in the document's units, squared.
fn fmt_area(u: crate::app::Units, mm2: f64) -> String {
    let k = u.world_from_mm(1.0);
    format!("{:.2} {}²", mm2 * k * k, u.short())
}

/// A price with the currency the user set.
fn fmt_money(v: f64, currency: &str) -> String {
    let c = currency.trim();
    if c.is_empty() {
        format!("{v:.2}")
    } else {
        format!("{v:.2} {c}")
    }
}

/// Seconds as h:mm:ss.
fn fmt_time(seconds: f64) -> String {
    let (h, m, s) = time::hms(seconds);
    format!("{h}:{m:02}:{s:02}")
}

impl App {
    /// The outlines of the selection's objects, by object.
    fn tracecut_parts(&self) -> (Vec<tracedraw_core::ShapeId>, Vec<Vec<BezPath>>) {
        let ids: Vec<_> = self
            .tracecut_targets()
            .into_iter()
            .filter(|id| self.doc().find_shape(*id).is_some_and(|s| !s.locked))
            .collect();
        let outlines = self.tracecut_outlines(&ids);
        (ids, outlines)
    }

    /// Turn a nesting layout into changes: each placed object moved, the
    /// sheets drawn when asked; with `output` (#4460) also copies, a page
    /// for each sheet, bleeds, cut lines and crop marks. `ids` holds
    /// each object once per copy.
    #[allow(clippy::too_many_arguments)]
    fn tracecut_layout_changes(
        &self,
        label: &'static str,
        ids: &[ShapeId],
        outlines: &[Vec<BezPath>],
        layout: &Layout,
        sheet: &SheetSpec,
        draw_sheets: bool,
        output: &NestOutput,
    ) -> Changes {
        let mut out = Changes {
            label,
            hide_objects: true,
            ..Default::default()
        };
        let plain = output.plain();
        let bleed = output.bleed_mm();
        // The bleed area and cut line of each object, worked out once.
        let mut areas: std::collections::HashMap<ShapeId, (BezPath, Color)> =
            std::collections::HashMap::new();
        let mut seen: std::collections::HashSet<ShapeId> = std::collections::HashSet::new();
        let mut items = Vec::new();
        for ((id, paths), p) in ids.iter().zip(outlines).zip(&layout.placements) {
            let first = seen.insert(*id);
            let Some(p) = p else {
                if first {
                    out.preview.extend(paths.iter().cloned());
                }
                continue;
            };
            out.preview
                .extend(paths.iter().map(|path| p.transform * path.clone()));
            if plain {
                out.changes
                    .push(Change::Cmd(Box::new(Command::TransformShapes {
                        shapes: vec![*id],
                        transform: p.transform,
                    })));
                continue;
            }
            if (bleed > 0.0 || output.cut_lines) && !areas.contains_key(id) {
                if let Some(sh) = self.doc().shape_on_page(*id) {
                    areas.insert(*id, (outer_area(&sh), object_color(&sh)));
                }
            }
            let area = areas.get(id).filter(|(a, _)| !a.elements().is_empty());
            let item_bleed = area
                .filter(|_| bleed > 0.0)
                .map(|(a, c)| (p.transform * tracedraw_core::shaping::offset(a, bleed), *c))
                .filter(|(a, _)| !a.elements().is_empty());
            let cut = output.cut_lines.then(|| {
                match area {
                    Some((a, _)) => p.transform * a.clone(),
                    // Nothing closed to go round: the box.
                    None => bounds_of(paths)
                        .map(|b| p.transform * b.to_path(0.01))
                        .unwrap_or_default(),
                }
            });
            if let Some((b, _)) = &item_bleed {
                out.preview.push(b.clone());
            }
            if let Some(c) = &cut {
                out.scored.push(c.clone());
            }
            items.push(NestItem {
                id: *id,
                copy: !first,
                sheet: p.sheet,
                transform: p.transform,
                bleed: item_bleed,
                cut,
            });
        }
        let sheets: Vec<BezPath> = layout
            .sheet_rects
            .iter()
            .map(|r| r.to_path(0.01))
            .collect();
        out.preview.extend(sheets.iter().cloned());
        if plain {
            if draw_sheets && !sheets.is_empty() {
                out.changes.push(Change::AddGroup {
                    name: "tracecut.sheets_group",
                    paths: sheets,
                    style: AddStyle::Hairline,
                });
            }
        } else if !items.is_empty() {
            let mut marks = vec![Vec::new(); layout.sheet_rects.len().max(1)];
            if output.crop_marks {
                for (it, paths) in items.iter().zip(
                    ids.iter()
                        .zip(outlines)
                        .zip(&layout.placements)
                        .filter(|(_, p)| p.is_some())
                        .map(|((_, o), _)| o),
                ) {
                    let moved: Vec<BezPath> =
                        paths.iter().map(|q| it.transform * q.clone()).collect();
                    let Some(b) = bounds_of(&moved) else {
                        continue;
                    };
                    let m = crop_marks(b, bleed + CROP_GAP);
                    out.guides.extend(m.iter().cloned());
                    let k = it.sheet.min(marks.len() - 1);
                    if let Some(v) = marks.get_mut(k) {
                        v.extend(m);
                    }
                }
            }
            out.changes.push(Change::Nest(Box::new(NestPlan {
                items,
                sheets: layout.sheet_rects.clone(),
                marks,
                pages: output.pages && !sheet.roll,
                draw_sheets,
            })));
        }
        let u = self.units;
        let left = ids.len().saturating_sub(layout.placed());
        let mut summary = if sheet.roll {
            trf(
                "tracecut.nest_roll",
                &[
                    ("n", &layout.placed().to_string()),
                    ("length", &fmt_len(u, layout.length)),
                    ("used", &format!("{:.0}", layout.utilisation() * 100.0)),
                ],
            )
        } else {
            trf(
                "tracecut.nest_sheets",
                &[
                    ("n", &layout.placed().to_string()),
                    ("sheets", &layout.sheets.to_string()),
                    ("used", &format!("{:.0}", layout.utilisation() * 100.0)),
                ],
            )
        };
        // The use of each sheet, when there are several (#4460).
        if !sheet.roll && layout.sheet_rects.len() > 1 {
            let areas: Vec<f64> = outlines
                .iter()
                .map(|p| measure::measure_object(p).area)
                .collect();
            let per = layout.sheet_parts_area(&areas);
            let shown: Vec<String> = per
                .iter()
                .zip(&layout.sheet_rects)
                .take(8)
                .map(|(a, r)| {
                    let m = r.width() * r.height();
                    let pc = if m > 1e-9 { (a / m * 100.0).clamp(0.0, 100.0) } else { 0.0 };
                    format!("{pc:.0}%")
                })
                .collect();
            let mut list = shown.join(", ");
            if per.len() > 8 {
                list.push_str(", …");
            }
            summary.push(' ');
            summary.push_str(&trf("tracecut.nest_per_sheet", &[("list", &list)]));
        }
        if output.pages && !sheet.roll && layout.placed() > 0 {
            summary.push(' ');
            summary.push_str(&trf(
                "tracecut.nest_pages",
                &[("n", &layout.sheet_rects.len().to_string())],
            ));
        }
        if left > 0 {
            summary.push(' ');
            summary.push_str(&trf("tracecut.nest_left_out", &[("n", &left.to_string())]));
        }
        out.summary = summary;
        out
    }

    /// The objects to nest, each once per copy, and their outlines.
    fn tracecut_nest_parts(&self, output: &NestOutput) -> (Vec<ShapeId>, Vec<Vec<BezPath>>) {
        let (ids, outlines) = self.tracecut_parts();
        let n = output.copies();
        if n <= 1 {
            return (ids, outlines);
        }
        let mut all_ids = Vec::with_capacity(ids.len() * n);
        let mut all = Vec::with_capacity(ids.len() * n);
        for _ in 0..n {
            all_ids.extend(ids.iter().copied());
            all.extend(outlines.iter().cloned());
        }
        (all_ids, all)
    }

    /// Shape nesting of the selection.
    pub fn tracecut_shape_nest(&self, s: &ShapeNestSettings) -> Changes {
        let (ids, outlines) = self.tracecut_nest_parts(&s.output);
        let settings = ShapeSettings {
            sheet: s.output.sheet(&s.sheet),
            stock: s.stock(),
            smallest: s.smallest,
            rotations: s.rotations,
            holes: s.holes,
            tolerance: s.tolerance,
            rounds: s.rounds.min(1000),
            seed: 1,
        };
        let limit = if s.seconds.is_finite() {
            s.seconds.clamp(0.0, 60.0)
        } else {
            2.0
        };
        let start = web_time::Instant::now();
        let mut out_of_time = || start.elapsed().as_secs_f64() > limit;
        let layout = shapes::nest_shapes(&outlines, &settings, &mut out_of_time);
        self.tracecut_layout_changes(
            "Shape Nesting",
            &ids,
            &outlines,
            &layout,
            &s.sheet,
            s.draw_sheets,
            &s.output,
        )
    }

    /// The commands that apply a nesting plan, and the new objects to
    /// select (#4460).
    pub(crate) fn tracecut_nest_commands(&mut self, plan: &NestPlan) -> (Vec<Command>, Vec<ShapeId>) {
        let mut cmds = Vec::new();
        let mut added = Vec::new();
        let cut_stroke = if plan.items.iter().any(|i| i.cut.is_some()) {
            let (color, add) = self.tracecut_contour_ink();
            cmds.extend(add);
            Some(Stroke::hairline(color))
        } else {
            None
        };
        let Some(active) = self.active_layer() else {
            return (cmds, added);
        };
        if plan.pages {
            self.tracecut_nest_pages(plan, active, cut_stroke.as_ref(), &mut cmds);
            return (cmds, added);
        }
        // On the page: the objects move, copies are added on the layer
        // of their object, then the bleeds go behind the lowest object
        // and the cut lines and crop marks on top.
        let mut lowest: Option<(tracedraw_core::LayerId, usize)> = None;
        let mut bleeds = Vec::new();
        let mut cuts = Vec::new();
        for it in &plan.items {
            let Ok((layer, index)) = self.doc().locate(it.id) else {
                // Inside a group: it moves, no copy.
                if !it.copy {
                    cmds.push(Command::TransformShapes {
                        shapes: vec![it.id],
                        transform: it.transform,
                    });
                }
                continue;
            };
            if lowest.is_none_or(|(l, i)| l == layer && index < i) {
                lowest = Some((layer, index));
            }
            if it.copy {
                let Some(src) = self.doc().find_shape(it.id).cloned() else {
                    continue;
                };
                let id = self.engine.new_shape_id();
                let copy = crate::app::reid_pub(src, id, &mut self.engine);
                cmds.push(Command::AddShape { layer, shape: copy });
                cmds.push(Command::TransformShapes {
                    shapes: vec![id],
                    transform: it.transform,
                });
                added.push(id);
            } else {
                cmds.push(Command::TransformShapes {
                    shapes: vec![it.id],
                    transform: it.transform,
                });
            }
            if let Some((b, c)) = &it.bleed {
                bleeds.push(self.tracecut_path_shape(b.clone(), Fill::Solid(*c), None, "tracecut.bleed_name"));
            }
            if let (Some(c), Some(st)) = (&it.cut, &cut_stroke) {
                cuts.push(self.tracecut_path_shape(c.clone(), Fill::None, Some(st.clone()), "tracecut.cut_line_name"));
            }
        }
        if !bleeds.is_empty() {
            let (layer, index) = lowest.unwrap_or((active, 0));
            let g = self.tracecut_group_of("tracecut.bleeds_group", bleeds);
            let gid = g.id;
            cmds.push(Command::AddShape { layer, shape: g });
            cmds.push(Command::Reorder {
                shape: gid,
                layer,
                index,
            });
        }
        if !cuts.is_empty() {
            let g = self.tracecut_group_of("tracecut.cut_lines_group", cuts);
            cmds.push(Command::AddShape {
                layer: active,
                shape: g,
            });
        }
        let marks: Vec<BezPath> = plan.marks.iter().flatten().cloned().collect();
        if !marks.is_empty() {
            let g = self.tracecut_group(active, "tracecut.crop_marks_group", &marks, AddStyle::Hairline);
            cmds.push(Command::AddShape {
                layer: active,
                shape: g,
            });
        }
        if plan.draw_sheets && !plan.sheets.is_empty() {
            let paths: Vec<BezPath> = plan.sheets.iter().map(|r| r.to_path(0.01)).collect();
            let g = self.tracecut_group(active, "tracecut.sheets_group", &paths, AddStyle::Hairline);
            added.push(g.id);
            cmds.push(Command::AddShape {
                layer: active,
                shape: g,
            });
        }
        (cmds, added)
    }

    /// A page for each sheet, of its size, after the last page: its
    /// bleeds, objects, cut lines and crop marks moved onto it; the
    /// objects leave their page (#4460).
    fn tracecut_nest_pages(
        &mut self,
        plan: &NestPlan,
        active: tracedraw_core::LayerId,
        cut: Option<&Stroke>,
        cmds: &mut Vec<Command>,
    ) {
        // The objects are moved in a copy of the drawing, then taken
        // from it as they end up.
        let mut scratch = self.doc().clone();
        let mut gone = Vec::new();
        let first = self.doc().pages.len();
        for (k, r) in plan.sheets.iter().enumerate() {
            let to_page = Affine::translate(-Vec2::new(r.x0, r.y0));
            let mut bleeds = Vec::new();
            let mut parts = Vec::new();
            let mut cuts = Vec::new();
            for it in plan.items.iter().filter(|i| i.sheet == k) {
                let Ok((layer, _)) = self.doc().locate(it.id) else {
                    continue;
                };
                let Some(src) = self.doc().find_shape(it.id).cloned() else {
                    continue;
                };
                let id = self.engine.new_shape_id();
                let copy = crate::app::reid_pub(src, id, &mut self.engine);
                let moved = Command::AddShape { layer, shape: copy }
                    .apply(&mut scratch)
                    .and_then(|_| {
                        Command::TransformShapes {
                            shapes: vec![id],
                            transform: to_page * it.transform,
                        }
                        .apply(&mut scratch)
                    });
                if moved.is_err() {
                    continue;
                }
                let Some(sh) = scratch.find_shape(id).cloned() else {
                    continue;
                };
                parts.push(sh);
                if !it.copy {
                    gone.push(it.id);
                }
                if let Some((b, c)) = &it.bleed {
                    bleeds.push(self.tracecut_path_shape(
                        to_page * b.clone(),
                        Fill::Solid(*c),
                        None,
                        "tracecut.bleed_name",
                    ));
                }
                if let (Some(c), Some(st)) = (&it.cut, cut) {
                    cuts.push(self.tracecut_path_shape(
                        to_page * c.clone(),
                        Fill::None,
                        Some(st.clone()),
                        "tracecut.cut_line_name",
                    ));
                }
            }
            if parts.is_empty() {
                continue;
            }
            let mut shapes = Vec::new();
            if !bleeds.is_empty() {
                shapes.push(self.tracecut_group_of("tracecut.bleeds_group", bleeds));
            }
            shapes.extend(parts);
            if !cuts.is_empty() {
                shapes.push(self.tracecut_group_of("tracecut.cut_lines_group", cuts));
            }
            let marks: Vec<BezPath> = plan
                .marks
                .get(k)
                .map(|m| m.iter().map(|p| to_page * p.clone()).collect())
                .unwrap_or_default();
            if !marks.is_empty() {
                shapes.push(self.tracecut_group(active, "tracecut.crop_marks_group", &marks, AddStyle::Hairline));
            }
            let u = self.units;
            cmds.push(Command::InsertPage {
                at: first + k,
                name: Some(trf(
                    "tracecut.sheet_page",
                    &[
                        ("n", &(k + 1).to_string()),
                        ("w", &fmt_len(u, r.width())),
                        ("h", &fmt_len(u, r.height())),
                    ],
                )),
                size: Size::new(r.width(), r.height()),
                layer: None,
                shapes,
            });
        }
        if !gone.is_empty() {
            cmds.push(Command::DeleteShapes { shapes: gone });
        }
    }

    /// A new curve named by the i18n key `name`.
    fn tracecut_path_shape(&mut self, path: BezPath, fill: Fill, stroke: Option<Stroke>, name: &str) -> Shape {
        let id = self.engine.new_shape_id();
        let closed = all_closed(&path);
        let mut sh = Shape::new(id, ShapeKind::Path { path, closed });
        sh.fill = fill;
        sh.stroke = stroke;
        sh.name = Some(tr(name));
        sh
    }

    /// Rectangle nesting of the selection, by the objects' boxes.
    pub fn tracecut_rect_nest(&self, s: &RectNestSettings) -> Changes {
        let (ids, outlines) = self.tracecut_nest_parts(&s.output);
        let items: Vec<RectItem> = outlines
            .iter()
            .map(|paths| {
                let m = measure::measure_object(paths);
                RectItem {
                    bounds: m.bbox.unwrap_or(Rect::ZERO),
                    area: m.area,
                }
            })
            .collect();
        let layout = rects::nest_rects(
            &items,
            &RectSettings {
                sheet: s.output.sheet(&s.sheet),
                rotate: s.rotate,
            },
        );
        self.tracecut_layout_changes(
            "Rectangle Nesting",
            &ids,
            &outlines,
            &layout,
            &s.sheet,
            s.draw_sheets,
            &s.output,
        )
    }

    /// Perimeter and area of the selection.
    pub fn tracecut_measure(&self) -> Report {
        let (_, outlines) = self.tracecut_parts();
        let m = measure::measure(&outlines);
        let u = self.units;
        Report {
            lines: vec![
                trf("tracecut.measure_length", &[("v", &fmt_len(u, m.length))]),
                trf("tracecut.measure_area", &[("v", &fmt_area(u, m.area))]),
                trf(
                    "tracecut.measure_holes",
                    &[
                        ("outer", &fmt_area(u, m.outer_area)),
                        ("holes", &fmt_area(u, m.hole_area)),
                    ],
                ),
                trf(
                    "tracecut.measure_box",
                    &[
                        ("w", &fmt_len(u, m.bbox.map_or(0.0, |b| b.width()))),
                        ("h", &fmt_len(u, m.bbox.map_or(0.0, |b| b.height()))),
                        ("a", &fmt_area(u, m.bbox_area)),
                    ],
                ),
                trf(
                    "tracecut.measure_counts",
                    &[
                        ("objects", &m.objects.to_string()),
                        ("closed", &m.closed_paths.to_string()),
                        ("open", &m.open_paths.to_string()),
                        ("nodes", &m.nodes.to_string()),
                    ],
                ),
            ],
        }
    }

    /// The cutting time of the selection, and its parts.
    pub fn tracecut_time(&self, s: &TimeSettings) -> (Report, time::TimeEstimate) {
        let (_, outlines) = self.tracecut_parts();
        let order = if s.nearest {
            TravelOrder::Nearest
        } else {
            TravelOrder::AsDrawn
        };
        let job = tracedraw_tracecut::calc::job(&outlines, Point::ZERO, order);
        let est = time::estimate(job.measurement.length, &job.travel, &s.profile());
        let u = self.units;
        let report = Report {
            lines: vec![
                trf("tracecut.time_total", &[("t", &fmt_time(est.total_s))]),
                trf(
                    "tracecut.time_parts",
                    &[
                        ("cut", &fmt_time(est.cut_s)),
                        ("travel", &fmt_time(est.travel_s)),
                        ("lifts", &fmt_time(est.lift_s)),
                    ],
                ),
                trf(
                    "tracecut.time_lengths",
                    &[
                        ("cut", &fmt_len(u, est.cut_length)),
                        ("travel", &fmt_len(u, est.travel.travel_length)),
                        ("n", &est.travel.outlines.to_string()),
                    ],
                ),
            ],
        };
        (report, est)
    }

    /// The cost of the selection: the material, cutting, weeding, pieces
    /// and machine time from the Cutting Time settings.
    pub fn tracecut_cost(&self, s: &CostSettings, t: &TimeSettings) -> Report {
        let (_, outlines) = self.tracecut_parts();
        let job = tracedraw_tracecut::calc::job(&outlines, Point::ZERO, TravelOrder::Nearest);
        let (_, est) = self.tracecut_time(t);
        let q = Quantities {
            blocks: job.blocks.clone(),
            cut_length: job.measurement.length,
            hours: est.total_s / 3600.0,
        };
        let material = Material {
            name: tr("tracecut.cost_material"),
            unit: if s.by_roll {
                MaterialUnit::Roll
            } else {
                MaterialUnit::Area
            },
            price: s.material_price,
            roll_width: s.roll_width,
        };
        let service = |key: &str, unit, price: f64| Service {
            name: tr(key),
            unit,
            price,
            selected: price > 0.0,
        };
        let services = [
            service("tracecut.cost_cutting", ServiceUnit::Metre, s.cut_price),
            service(
                "tracecut.cost_weeding",
                ServiceUnit::SquareMetre,
                s.weed_price,
            ),
            service("tracecut.cost_pieces", ServiceUnit::Piece, s.piece_price),
            service("tracecut.cost_hours", ServiceUnit::Hour, s.hour_price),
        ];
        let spec = cost::CostSpec {
            reserve: s.reserve,
            copies: s.copies.max(1),
            ..Default::default()
        };
        let r = cost::cost(&q, Some(&material), &services, &spec);
        let unit = |u: cost::QtyUnit| match u {
            cost::QtyUnit::SquareMetre => "m²",
            cost::QtyUnit::Metre => "m",
            cost::QtyUnit::Piece => "×",
            cost::QtyUnit::Hour => "h",
        };
        let mut lines = vec![trf(
            "tracecut.cost_total",
            &[("v", &fmt_money(r.total, &s.currency))],
        )];
        for l in &r.lines {
            lines.push(format!(
                "{}: {:.2} {} × {} = {}",
                l.name,
                l.quantity,
                unit(l.unit),
                fmt_money(l.unit_price, &s.currency),
                fmt_money(l.amount, &s.currency)
            ));
        }
        Report { lines }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::ShapeKind;

    fn rect(app: &mut App, x: f64, y: f64, w: f64, h: f64) -> tracedraw_core::ShapeId {
        app.new_shape(ShapeKind::Rect {
            rect: Rect::new(x, y, x + w, y + h),
            radius: 0.0,
            corners: None,
        })
        .expect("rect")
    }

    fn small_sheet() -> SheetSpec {
        SheetSpec {
            width: 100.0,
            height: 100.0,
            spacing: 2.0,
            ..Default::default()
        }
    }

    /// The top-level shapes on page `i`, all layers.
    fn on_page(app: &App, i: usize) -> Vec<Shape> {
        app.doc().pages[i]
            .layers
            .iter()
            .flat_map(|l| l.shapes.iter().cloned())
            .collect()
    }

    /// #4460: copies, a page per sheet of mixed sizes, bleeds, cut lines
    /// and crop marks in one undo step.
    #[test]
    fn nesting_onto_pages_with_copies_bleed_cut_lines_and_crop_marks() {
        let mut app = App::headless();
        let a = rect(&mut app, 300.0, 300.0, 40.0, 40.0);
        let b = rect(&mut app, 400.0, 300.0, 60.0, 20.0);
        let keep = rect(&mut app, 10.0, 10.0, 5.0, 5.0);
        app.select(vec![a, b]);
        let s = ShapeNestSettings {
            sheet: SheetSpec {
                width: 100.0,
                height: 100.0,
                spacing: 2.0,
                ..Default::default()
            },
            count: 1,
            more_sheets: vec![StockSheet {
                width: 80.0,
                height: 80.0,
                count: 0,
            }],
            rounds: 0,
            output: NestOutput {
                copies: 3,
                pages: true,
                bleed: 2.0,
                cut_lines: true,
                crop_marks: true,
            },
            ..Default::default()
        };
        let c = app.tracecut_shape_nest(&s);
        assert!(!c.scored.is_empty() && !c.guides.is_empty(), "preview shows cut lines and marks");
        let pages_before = app.doc().pages.len();
        let before = app.engine.history_labels().0.len();
        assert!(app.tracecut_apply_changes(&c), "{}", app.status);
        assert_eq!(app.engine.history_labels().0.len(), before + 1);
        let added = app.doc().pages.len() - pages_before;
        assert!(added >= 2, "{added} pages");
        // The first new page is the first stock size, later ones the next.
        assert_eq!(app.doc().pages[pages_before].size, Size::new(100.0, 100.0));
        assert_eq!(app.doc().pages[pages_before + added - 1].size, Size::new(80.0, 80.0));
        // The objects left their page; the other object stayed.
        assert!(app.doc().find_shape(a).is_none() && app.doc().find_shape(b).is_none());
        assert!(app.doc().find_shape(keep).is_some());
        // Six objects (three copies of two) on the new pages, each with
        // a bleed and a cut line, all within its page.
        let mut objects = 0;
        let mut bleeds = 0;
        let mut cuts = 0;
        let mut marks = 0;
        for i in pages_before..pages_before + added {
            let page = app.doc().pages[i].rect();
            for sh in on_page(&app, i) {
                match (&sh.kind, sh.name.as_deref()) {
                    (ShapeKind::Group { children }, Some(n)) if n == tr("tracecut.bleeds_group") => {
                        bleeds += children.len()
                    }
                    (ShapeKind::Group { children }, Some(n)) if n == tr("tracecut.cut_lines_group") => {
                        assert!(children.iter().all(|c| c.stroke.is_some() && c.fill == Fill::None));
                        cuts += children.len()
                    }
                    (ShapeKind::Group { children }, Some(n)) if n == tr("tracecut.crop_marks_group") => {
                        marks += children.len()
                    }
                    _ => {
                        objects += 1;
                        let bb = sh.page_path().bounding_box();
                        assert!(page.inset(1e-6).contains(bb.origin()), "{bb:?} off {page:?}");
                        assert!(bb.x1 <= page.x1 + 1e-6 && bb.y1 <= page.y1 + 1e-6, "{bb:?} off {page:?}");
                        // Two of bleed and the spacing kept from the edge.
                        assert!(bb.x0 >= 2.0 - 1e-3 && bb.y0 >= 2.0 - 1e-3, "{bb:?}");
                    }
                }
            }
        }
        assert_eq!((objects, bleeds, cuts), (6, 6, 6));
        assert_eq!(marks, 6 * 8);
        // The cut line ink was added to the drawing.
        assert!(app.doc().inks.iter().any(|i| i.name == crate::tracecut_cut::CUT_CONTOUR));
        // One undo brings it all back.
        app.engine.undo();
        assert_eq!(app.doc().pages.len(), pages_before);
        assert!(app.doc().find_shape(a).is_some() && app.doc().find_shape(b).is_some());
    }

    /// #4460: copies on the page, without pages: the object moves, its
    /// copies are new objects, bleeds go behind the objects.
    #[test]
    fn copies_and_bleeds_on_the_same_page() {
        let mut app = App::headless();
        let a = rect(&mut app, 300.0, 300.0, 20.0, 20.0);
        app.select(vec![a]);
        let shapes_before = on_page(&app, 0).len();
        let s = RectNestSettings {
            sheet: small_sheet(),
            draw_sheets: false,
            output: NestOutput {
                copies: 4,
                bleed: 1.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let c = app.tracecut_rect_nest(&s);
        assert!(app.tracecut_apply_changes(&c), "{}", app.status);
        let shapes = on_page(&app, 0);
        // The object, three copies and the bleeds group.
        assert_eq!(shapes.len(), shapes_before + 4);
        let bleed_at = shapes
            .iter()
            .position(|s| s.name.as_deref() == Some(tr("tracecut.bleeds_group").as_str()))
            .expect("bleeds");
        let obj_at = shapes.iter().position(|s| s.id == a).expect("object");
        assert!(bleed_at < obj_at, "the bleeds go behind");
        let boxes: Vec<Rect> = shapes
            .iter()
            .filter(|s| matches!(s.kind, ShapeKind::Rect { .. }))
            .map(|s| s.page_path().bounding_box())
            .collect();
        assert_eq!(boxes.len(), 4);
        for (i, p) in boxes.iter().enumerate() {
            assert!(p.x0 >= 1.0 - 1e-6 && p.x1 <= 99.0 + 1e-6, "{p:?}");
            for q in &boxes[i + 1..] {
                // Spacing 2 plus a bleed of 1 on each side.
                let gap = (q.x0 - p.x1).max(p.x0 - q.x1).max(q.y0 - p.y1).max(p.y0 - q.y1);
                assert!(gap >= 4.0 - 1e-6, "{p:?} {q:?}");
            }
        }
    }

    #[test]
    fn crop_marks_sit_outside_the_box() {
        let m = crop_marks(Rect::new(0.0, 0.0, 10.0, 10.0), 1.0);
        assert_eq!(m.len(), 8);
        for p in &m {
            let b = p.bounding_box();
            let inside = b.x0 > 0.0 && b.x1 < 10.0 && b.y0 > 0.0 && b.y1 < 10.0;
            assert!(!inside);
            assert!((b.width().max(b.height()) - CROP_LEN).abs() < 1e-9);
        }
    }

    #[test]
    fn rectangle_nesting_moves_the_objects_onto_sheets_in_one_step() {
        let mut app = App::headless();
        let ids: Vec<_> = (0..4)
            .map(|i| rect(&mut app, 300.0 + 70.0 * i as f64, 300.0, 40.0, 40.0))
            .collect();
        app.select(ids.clone());
        let s = RectNestSettings {
            sheet: small_sheet(),
            ..Default::default()
        };
        let c = app.tracecut_rect_nest(&s);
        let before = app.engine.history_labels().0.len();
        assert!(app.tracecut_apply_changes(&c));
        assert_eq!(app.engine.history_labels().0.len(), before + 1);
        for id in &ids {
            let b = app
                .doc()
                .shape_on_page(*id)
                .expect("shape")
                .page_path()
                .bounding_box();
            assert!(b.x0 >= -1e-6 && b.x1 <= 100.0 + 1e-6, "{b:?}");
            assert!(b.y0 >= -1e-6 && b.y1 <= 100.0 + 1e-6, "{b:?}");
        }
        // The sheet is drawn as a group.
        assert!(app.selection.len() == 1);
    }

    #[test]
    fn shape_nesting_places_the_objects_without_overlap() {
        let mut app = App::headless();
        let a = rect(&mut app, 500.0, 500.0, 30.0, 30.0);
        let b = rect(&mut app, 600.0, 500.0, 30.0, 30.0);
        app.select(vec![a, b]);
        let s = ShapeNestSettings {
            sheet: small_sheet(),
            rounds: 2,
            draw_sheets: false,
            ..Default::default()
        };
        let c = app.tracecut_shape_nest(&s);
        assert!(c.summary.contains('2'), "{}", c.summary);
        assert!(app.tracecut_apply_changes(&c));
        let ba = app
            .doc()
            .shape_on_page(a)
            .expect("a")
            .page_path()
            .bounding_box();
        let bb = app
            .doc()
            .shape_on_page(b)
            .expect("b")
            .page_path()
            .bounding_box();
        assert!(ba.x1 <= 100.0 + 1e-6 && bb.x1 <= 100.0 + 1e-6);
        let overlap = ba.intersect(bb);
        assert!(overlap.width() <= 1e-6 || overlap.height() <= 1e-6);
    }

    /// #2227: an object with an envelope is nested by what it draws. A
    /// 30 mm square whose envelope draws it 60 mm wide and high used to
    /// be placed as the 30 mm square it was before the envelope, so its
    /// neighbour landed on top of it.
    #[test]
    fn an_enveloped_object_is_nested_by_what_it_draws() {
        use tracedraw_core::envelope::Frame;
        use tracedraw_core::geometry::Affine;
        for shape_nest in [true, false] {
            let mut app = App::headless();
            let a = rect(&mut app, 500.0, 500.0, 30.0, 30.0);
            let b = rect(&mut app, 600.0, 500.0, 30.0, 30.0);
            app.select(vec![a]);
            app.apply_envelope_frame(|r| {
                Frame::around(r).transformed(
                    Affine::translate((r.x0, r.y0))
                        * Affine::scale(2.0)
                        * Affine::translate((-r.x0, -r.y0)),
                )
            });
            let drawn = |app: &App, id| app.doc().shape_on_page(id).expect("shape").visual_bounds();
            assert!((drawn(&app, a).width() - 60.0).abs() < 0.01);
            app.select(vec![a, b]);
            let c = if shape_nest {
                app.tracecut_shape_nest(&ShapeNestSettings {
                    sheet: small_sheet(),
                    rounds: 2,
                    draw_sheets: false,
                    ..Default::default()
                })
            } else {
                app.tracecut_rect_nest(&RectNestSettings {
                    sheet: small_sheet(),
                    draw_sheets: false,
                    ..Default::default()
                })
            };
            assert!(app.tracecut_apply_changes(&c));
            let (ba, bb) = (drawn(&app, a), drawn(&app, b));
            for r in [ba, bb] {
                assert!(r.x0 >= -0.01 && r.y0 >= -0.01, "{r:?}");
                assert!(r.x1 <= 100.01 && r.y1 <= 100.01, "{r:?}");
            }
            let overlap = ba.intersect(bb);
            assert!(
                overlap.width() <= 1e-6 || overlap.height() <= 1e-6,
                "{ba:?} and {bb:?} overlap (shape nesting: {shape_nest})"
            );
        }
    }

    /// The live test of 0.41: Shape Nesting at 1 mm accuracy left objects
    /// crossing each other and past the sheet's edge. Ellipses, stars,
    /// round-cornered rectangles, an L-shaped curve, some turned or
    /// scaled, nested from the window's tool and applied: in wireframe
    /// no two outlines cross or come closer than the spacing, every
    /// moved object lies on its sheet, and one too large stays where it
    /// was and is reported.
    #[test]
    fn nested_objects_never_cross_and_stay_on_the_sheet() {
        use tracedraw_core::geometry::{Affine, Shape as _};
        let mut app = App::headless();
        let mut ids = Vec::new();
        for i in 0..14 {
            let x = 400.0 + 60.0 * (i % 5) as f64;
            let y = 400.0 + 60.0 * (i / 5) as f64;
            let k = (i % 3) as f64;
            let r = Rect::new(x, y, x + 30.0 + 8.0 * k, y + 18.0 + 6.0 * k);
            let kind = match i % 4 {
                0 => ShapeKind::Ellipse { rect: r, arc: None },
                1 => ShapeKind::Polygon {
                    rect: r,
                    points: 5 + i as u32 % 3,
                    sharpness: 0.5,
                    complex: None,
                    twist: 0.0,
                    bend: 0.0,
                },
                2 => ShapeKind::Rect {
                    rect: r,
                    radius: 6.0,
                    corners: None,
                },
                _ => {
                    let mut p = BezPath::new();
                    p.move_to((x, y));
                    p.line_to((x + 40.0, y));
                    p.curve_to(
                        (x + 40.0, y + 10.0),
                        (x + 20.0, y + 6.0),
                        (x + 12.0, y + 12.0),
                    );
                    p.line_to((x + 12.0, y + 30.0));
                    p.line_to((x, y + 30.0));
                    p.close_path();
                    ShapeKind::Path {
                        path: p,
                        closed: true,
                    }
                }
            };
            let id = app.new_shape(kind).expect("shape");
            if i % 3 == 1 {
                let c = r.center().to_vec2();
                app.select(vec![id]);
                app.transform_selection(
                    Affine::translate(c)
                        * Affine::rotate(0.5)
                        * Affine::scale(1.2)
                        * Affine::translate(-c),
                );
            }
            ids.push(id);
        }
        let big = rect(&mut app, 900.0, 900.0, 300.0, 300.0);
        let mut all = ids.clone();
        all.push(big);
        app.select(all);
        let before_big = app.doc().shape_on_page(big).expect("big").page_path();
        let mut settings = crate::tracecut::ToolSettings::default();
        settings.shape_nest = ShapeNestSettings {
            sheet: SheetSpec {
                width: 150.0,
                height: 120.0,
                margin: 3.0,
                spacing: 2.0,
                ..Default::default()
            },
            rotations: Rotations::Eighth,
            tolerance: 1.0,
            rounds: 5,
            draw_sheets: false,
            ..Default::default()
        };
        let w = app.tracecut_work_out(crate::tracecut::ToolId::ShapeNesting, &settings);
        assert!(app.tracecut_commit(&w, &settings, None));
        assert!(app
            .status
            .contains(&trf("tracecut.nest_left_out", &[("n", "1")])));
        assert_eq!(
            app.doc().shape_on_page(big).expect("big").page_path(),
            before_big,
            "too large: left where it was"
        );
        let spec = settings.shape_nest.sheet;
        let placed: Vec<BezPath> = ids
            .iter()
            .map(|id| app.doc().shape_on_page(*id).expect("shape").page_path())
            .collect();
        let sheet_of = |p: &BezPath| {
            let b = p.bounding_box();
            (0..20).find(|i| {
                let s = spec.sheet_rect(*i, 0.0).inset(-3.0);
                b.x0 >= s.x0 - 1e-6
                    && b.y0 >= s.y0 - 1e-6
                    && b.x1 <= s.x1 + 1e-6
                    && b.y1 <= s.y1 + 1e-6
            })
        };
        for p in &placed {
            assert!(
                sheet_of(p).is_some(),
                "{:?} is off every sheet",
                p.bounding_box()
            );
        }
        for (i, a) in placed.iter().enumerate() {
            for b in &placed[i + 1..] {
                let shared =
                    tracedraw_core::shaping::overlay(a, b, tracedraw_core::shaping::Op::Intersect);
                assert!(
                    measure::measure_object(&[shared]).area < 1e-6,
                    "two objects cross"
                );
                // Grown by just under half the spacing, they still keep
                // apart.
                let ga = tracedraw_core::shaping::offset(a, 0.98);
                let gb = tracedraw_core::shaping::offset(b, 0.98);
                let shared = tracedraw_core::shaping::overlay(
                    &ga,
                    &gb,
                    tracedraw_core::shaping::Op::Intersect,
                );
                assert!(
                    measure::measure_object(&[shared]).area < 1e-6,
                    "two objects closer than the spacing"
                );
            }
        }
    }

    #[test]
    fn an_object_too_big_for_the_sheet_is_left_where_it_is() {
        let mut app = App::headless();
        let big = rect(&mut app, 0.0, 0.0, 300.0, 300.0);
        app.select(vec![big]);
        let c = app.tracecut_rect_nest(&RectNestSettings {
            sheet: small_sheet(),
            ..Default::default()
        });
        assert!(c
            .summary
            .contains(&trf("tracecut.nest_left_out", &[("n", "1")])));
        assert!(!c.changes.iter().any(
            |ch| matches!(ch, Change::Cmd(cmd) if matches!(**cmd, Command::TransformShapes { .. }))
        ));
    }

    #[test]
    fn perimeter_area_time_and_cost() {
        let mut app = App::headless();
        app.units = crate::app::Units::Millimeters;
        let a = rect(&mut app, 0.0, 0.0, 100.0, 50.0);
        app.select(vec![a]);
        let m = app.tracecut_measure();
        assert!(m.lines[0].contains("300.0"), "{:?}", m.lines);
        assert!(m.lines[1].contains("5000.00"), "{:?}", m.lines);
        let t = TimeSettings {
            cut_speed: 100.0,
            travel_speed: 100.0,
            lift_time: 0.0,
            accel_factor: 1.0,
            passes: 1,
            nearest: true,
        };
        let (_, est) = app.tracecut_time(&t);
        // 300 mm at 100 mm/s, plus the move to the start.
        assert!((est.cut_s - 3.0).abs() < 1e-9);
        let cost = app.tracecut_cost(
            &CostSettings {
                by_roll: false,
                material_price: 10.0,
                cut_price: 1.0,
                weed_price: 0.0,
                reserve: 0.0,
                currency: "EUR".into(),
                ..Default::default()
            },
            &t,
        );
        // 0.005 m2 x 10 + 0.3 m x 1 = 0.35
        assert!(cost.lines[0].contains("0.35 EUR"), "{:?}", cost.lines);
        assert_eq!(cost.lines.len(), 3);
    }
}

/// The box round `paths`, when they have one.
fn bounds_of(paths: &[BezPath]) -> Option<Rect> {
    paths
        .iter()
        .filter(|p| !p.elements().is_empty())
        .map(|p| p.bounding_box())
        .filter(|b| [b.x0, b.y0, b.x1, b.y1].iter().all(|v| v.is_finite()))
        .reduce(|a, b| a.union(b))
}
