//! Cutting commands of Object > Shaping: Cut Objects Below (the selected
//! object cuts everything under it and goes), Cut into Grid (each
//! selected object becomes rows and columns of boxes over its bounds,
//! with a gap between them), and Remove Strays (lone points, curves with
//! no fill and no outline, and empty texts on the page go). Each is one
//! undo step. See docs/help/en/lines-shapes/shaping.md.

use crate::app::App;
use tracedraw_core::geometry::{BezPath, Rect, Shape as _};
use tracedraw_core::shaping::{self, FillRule, Op};
use tracedraw_core::{Command, Fill, LayerId, Shape, ShapeId, ShapeKind};

/// The settings of Cut into Grid: the window is open while `open`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridCut {
    pub open: bool,
    pub rows: u32,
    pub cols: u32,
    /// The gap between boxes, mm.
    pub gap: f64,
}

impl Default for GridCut {
    fn default() -> Self {
        Self {
            open: false,
            rows: 2,
            cols: 2,
            gap: 0.0,
        }
    }
}

/// The most rows or columns Cut into Grid makes.
pub const GRID_CUT_MAX: u32 = 100;

/// A piece of `src` with area `area` (page space): its fill, outline and
/// opacity.
fn cut_piece(id: ShapeId, area: BezPath, src: &Shape) -> Shape {
    let mut s = Shape::new(
        id,
        ShapeKind::Path {
            path: area.clone(),
            closed: true,
        },
    );
    s.fill = src.fill.for_result(src.transform, &area);
    s.extra_fills = src.extra_fills.clone();
    s.stroke = src.stroke.clone();
    s.opacity = src.opacity;
    s.fill_rule = FillRule::EvenOdd;
    s
}

/// Whether `s` is an object a cut can split: a rectangle, ellipse,
/// polygon or curve that is closed or filled.
fn cuttable(s: &Shape) -> bool {
    match &s.kind {
        ShapeKind::Rect { .. } | ShapeKind::Ellipse { .. } | ShapeKind::Polygon { .. } => true,
        ShapeKind::Path { closed, .. } => *closed || s.fill != Fill::None,
        _ => false,
    }
}

/// The commands that put `made` (in that order, bottom first) where the
/// object `old` is, at `index` on `layer`, and take `old` out.
fn replace_with(
    cmds: &mut Vec<Command>,
    layer: LayerId,
    index: usize,
    old: ShapeId,
    made: Vec<Shape>,
) {
    for (k, s) in made.into_iter().enumerate() {
        let id = s.id;
        cmds.push(Command::AddShape { layer, shape: s });
        cmds.push(Command::Reorder {
            shape: id,
            layer,
            index: index + k,
        });
    }
    cmds.push(Command::DeleteShapes { shapes: vec![old] });
}

impl App {
    /// The one selected object that can cut the objects below it.
    fn cutter(&self) -> Option<Shape> {
        if self.selection.len() != 1 {
            return None;
        }
        let s = self.doc().shape(self.selection[0]).ok()?.1.clone();
        (!s.locked && cuttable(&s)).then_some(s)
    }

    /// Whether Cut Objects Below can run.
    pub fn cut_below_ready(&self) -> bool {
        self.cutter().is_some()
    }

    /// Cut Objects Below: every object under the selected one on the
    /// page (on unlocked, shown layers) that it overlaps is cut along its
    /// outline into the parts inside and outside it, each keeping the
    /// object's paint and place; the selected object goes. Groups, texts
    /// and pictures are left as they are.
    pub fn cut_objects_below(&mut self) {
        let Some(cutter) = self.cutter() else {
            return;
        };
        let doc = self.doc();
        let Ok(page) = doc.page(self.page) else {
            return;
        };
        let Ok((cut_layer, cut_index)) = doc.locate(cutter.id) else {
            return;
        };
        let Some(cut_rank) = page.layers.iter().position(|l| l.id == cut_layer) else {
            return;
        };
        let knife = crate::ops::shaping_area(&cutter);
        let reach = knife.bounding_box();
        // The objects to cut, from the top down, so a cut object's extra
        // pieces do not move the places of the objects still to cut.
        let mut work: Vec<(LayerId, usize, Shape, Vec<BezPath>)> = Vec::new();
        for layer in page.layers.iter().take(cut_rank + 1).rev() {
            if layer.locked || !layer.visible {
                continue;
            }
            let end = if layer.id == cut_layer {
                cut_index
            } else {
                layer.shapes.len()
            };
            for (i, s) in layer.shapes.iter().enumerate().take(end).rev() {
                if s.locked || !s.visible || !cuttable(s) {
                    continue;
                }
                if s.bounds().intersect(reach).area() <= 0.0 {
                    continue;
                }
                let area = crate::ops::shaping_area(s);
                let mut parts = Vec::new();
                for op in [Op::Trim, Op::Intersect] {
                    let side = shaping::overlay(&area, &knife, op);
                    for p in shaping::overlay_parts(&side, &BezPath::new(), Op::Weld) {
                        if shaping::filled_area(&p, FillRule::EvenOdd) > 1e-4 {
                            parts.push(p);
                        }
                    }
                }
                if parts.len() >= 2 {
                    work.push((layer.id, i, s.clone(), parts));
                }
            }
        }
        let mut cmds = Vec::new();
        let mut made_ids = Vec::new();
        for (layer, i, s, parts) in work {
            let made: Vec<Shape> = parts
                .into_iter()
                .map(|p| cut_piece(self.engine.new_shape_id(), p, &s))
                .collect();
            made_ids.extend(made.iter().map(|m| m.id));
            replace_with(&mut cmds, layer, i, s.id, made);
        }
        if cmds.is_empty() {
            self.status = crate::i18n::tr("status.no_pieces");
            return;
        }
        cmds.push(Command::DeleteShapes {
            shapes: vec![cutter.id],
        });
        if let Err(e) = self.engine.run_batch("Cut Objects Below", &cmds) {
            self.status = e.to_string();
            return;
        }
        self.select(made_ids);
    }

    /// The selected objects Cut into Grid works on: unlocked ones placed
    /// straight on a layer.
    fn grid_cut_targets(&self) -> Vec<(Shape, LayerId, usize)> {
        let doc = self.doc();
        self.selection
            .iter()
            .filter_map(|id| {
                let s = doc.shape(*id).ok()?.1;
                let (l, i) = doc.locate(*id).ok()?;
                (!s.locked).then(|| (s.clone(), l, i))
            })
            .collect()
    }

    /// Whether Cut into Grid can run.
    pub fn grid_cut_ready(&self) -> bool {
        !self.grid_cut_targets().is_empty()
    }

    /// Cut into Grid with the settings in `self.grid_cut`: each selected
    /// object becomes a group of `rows` by `cols` boxes filling its
    /// bounds with `gap` mm between them, in its paint and place.
    pub fn cut_into_grid(&mut self) {
        let GridCut {
            rows, cols, gap, ..
        } = self.grid_cut;
        let (rows, cols) = (rows.clamp(1, GRID_CUT_MAX), cols.clamp(1, GRID_CUT_MAX));
        let gap = if gap.is_finite() { gap.max(0.0) } else { 0.0 };
        let mut targets = self.grid_cut_targets();
        // From the top down on each layer, so places stay right.
        targets.sort_by_key(|t| std::cmp::Reverse(t.2));
        let mut cmds = Vec::new();
        let mut made_ids = Vec::new();
        for (s, layer, index) in targets {
            let b = s.bounds();
            let w = (b.width() - gap * f64::from(cols - 1)) / f64::from(cols);
            let h = (b.height() - gap * f64::from(rows - 1)) / f64::from(rows);
            if !(w > 1e-6 && h > 1e-6) {
                continue;
            }
            let mut cells = Vec::new();
            for r in 0..rows {
                for c in 0..cols {
                    let x = b.x0 + f64::from(c) * (w + gap);
                    let y = b.y0 + f64::from(r) * (h + gap);
                    let rect = Rect::new(x, y, x + w, y + h).to_path(0.1);
                    cells.push(cut_piece(self.engine.new_shape_id(), rect, &s));
                }
            }
            let group = if cells.len() == 1 {
                cells.pop().expect("one box")
            } else {
                Shape::new(
                    self.engine.new_shape_id(),
                    ShapeKind::Group { children: cells },
                )
            };
            made_ids.push(group.id);
            replace_with(&mut cmds, layer, index, s.id, vec![group]);
        }
        if cmds.is_empty() {
            return;
        }
        if let Err(e) = self.engine.run_batch("Cut into Grid", &cmds) {
            self.status = e.to_string();
            return;
        }
        self.select(made_ids);
    }

    /// The strays on the current page's unlocked, shown layers: curves of
    /// one node or none, curves with no fill, outline or effect, and
    /// artistic texts with nothing but spaces in them; inside groups too.
    pub fn strays(&self) -> Vec<ShapeId> {
        fn walk(shapes: &[Shape], out: &mut Vec<ShapeId>) {
            for s in shapes {
                if s.locked {
                    continue;
                }
                match &s.kind {
                    ShapeKind::Group { children } => walk(children, out),
                    ShapeKind::Path { path, .. } => {
                        let lone = tracedraw_core::nodes::nodes(path).len() <= 1;
                        let unpainted = s.fill == Fill::None
                            && s.stroke.is_none()
                            && s.extra_fills.is_empty()
                            && s.effects.is_empty()
                            && s.name.is_none();
                        if lone || unpainted {
                            out.push(s.id);
                        }
                    }
                    ShapeKind::Text {
                        spans,
                        frame: None,
                        on_path: None,
                        ..
                    } if spans.iter().all(|t| t.text.trim().is_empty()) => out.push(s.id),
                    _ => {}
                }
            }
        }
        let mut out = Vec::new();
        if let Ok(page) = self.doc().page(self.page) {
            for layer in page.layers.iter().filter(|l| !l.locked && l.visible) {
                walk(&layer.shapes, &mut out);
            }
        }
        out
    }

    /// Remove Strays: the objects [`Self::strays`] finds go, in one undo
    /// step; the status bar says how many.
    pub fn remove_strays(&mut self) {
        let gone = self.strays();
        let n = gone.len().to_string();
        if !gone.is_empty() {
            let cmds = vec![Command::DeleteShapes {
                shapes: gone.clone(),
            }];
            if let Err(e) = self.engine.run_batch("Remove Strays", &cmds) {
                self.status = e.to_string();
                return;
            }
            self.selection.retain(|id| !gone.contains(id));
        }
        self.status = crate::i18n::trf("status.strays_removed", &[("n", &n)]);
    }
}

/// The Cut into Grid window: rows, columns and gap, OK and Cancel.
pub fn grid_cut_window(app: &mut App, ctx: &egui::Context) {
    if !app.grid_cut.open {
        return;
    }
    use crate::i18n::tr;
    let mut keep = true;
    let mut done = false;
    let mut g = app.grid_cut;
    egui::Window::new(tr("grid_cut.title"))
        .id(egui::Id::new("grid_cut"))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            egui::Grid::new("grid_cut_fields")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label(tr("grid_cut.rows"));
                    ui.add(egui::DragValue::new(&mut g.rows).range(1..=GRID_CUT_MAX));
                    ui.end_row();
                    ui.label(tr("grid_cut.cols"));
                    ui.add(egui::DragValue::new(&mut g.cols).range(1..=GRID_CUT_MAX));
                    ui.end_row();
                    ui.label(tr("grid_cut.gap"));
                    ui.add(
                        egui::DragValue::new(&mut g.gap)
                            .range(0.0..=1000.0)
                            .speed(0.1)
                            .suffix(" mm"),
                    );
                    ui.end_row();
                });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button(tr("dialog.ok")).clicked()
                    || ui.input(|i| i.key_pressed(egui::Key::Enter))
                {
                    done = true;
                }
                if ui.button(tr("dialog.cancel")).clicked()
                    || ui.input(|i| i.key_pressed(egui::Key::Escape))
                {
                    keep = false;
                }
            });
        });
    app.grid_cut = g;
    if done {
        app.grid_cut.open = false;
        app.cut_into_grid();
    } else if !keep {
        app.grid_cut.open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::Color;

    fn rect(app: &mut App, r: Rect) -> ShapeId {
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: r,
                radius: 0.0,
                corners: None,
            })
            .expect("shape");
        app.engine
            .run(&Command::SetFill {
                shapes: vec![id],
                fill: Fill::Solid(Color::rgb8(200, 0, 0)),
            })
            .expect("fill");
        id
    }

    fn top_level(app: &App) -> Vec<Shape> {
        app.doc()
            .all_layers()
            .flat_map(|l| l.shapes.clone())
            .collect()
    }

    /// A square over the middle of a wide box cuts it into three parts
    /// (left, right and the middle under it) in the box's paint and
    /// place, and goes; a box it does not touch stays. One undo step
    /// brings it all back.
    #[test]
    fn a_square_cuts_the_objects_below_it() {
        let mut app = App::headless();
        let far = rect(&mut app, Rect::new(100.0, 0.0, 110.0, 10.0));
        let wide = rect(&mut app, Rect::new(0.0, 0.0, 30.0, 10.0));
        let knife = rect(&mut app, Rect::new(10.0, -5.0, 20.0, 15.0));
        app.select(vec![knife]);
        assert!(app.cut_below_ready());
        let depth = app.engine.history_labels().0.len();
        app.cut_objects_below();
        assert_eq!(app.engine.history_labels().0.len(), depth + 1);
        let shapes = top_level(&app);
        assert!(shapes.iter().all(|s| s.id != knife && s.id != wide));
        assert_eq!(shapes[0].id, far, "the untouched box stays at the bottom");
        assert_eq!(shapes.len(), 4);
        let mut areas: Vec<f64> = shapes[1..]
            .iter()
            .map(|s| shaping::filled_area(&s.page_path(), FillRule::EvenOdd).round())
            .collect();
        areas.sort_by(f64::total_cmp);
        assert_eq!(areas, vec![100.0, 100.0, 100.0]);
        assert!(shapes[1..]
            .iter()
            .all(|s| s.fill == Fill::Solid(Color::rgb8(200, 0, 0))));
        assert_eq!(app.selection.len(), 3);
        app.undo();
        assert_eq!(top_level(&app).len(), 3);
    }

    /// Cut into Grid makes rows by columns of boxes with the gap between
    /// them, filling the object's bounds, in its place.
    #[test]
    fn cut_into_grid_makes_boxes_with_gaps() {
        let mut app = App::headless();
        let under = rect(&mut app, Rect::new(50.0, 0.0, 60.0, 10.0));
        let id = rect(&mut app, Rect::new(0.0, 0.0, 22.0, 10.0));
        let over = rect(&mut app, Rect::new(70.0, 0.0, 80.0, 10.0));
        app.select(vec![id]);
        app.grid_cut = GridCut {
            open: false,
            rows: 1,
            cols: 3,
            gap: 2.0,
        };
        app.cut_into_grid();
        let shapes = top_level(&app);
        assert_eq!(shapes.len(), 3);
        assert_eq!((shapes[0].id, shapes[2].id), (under, over));
        let ShapeKind::Group { children } = &shapes[1].kind else {
            panic!("a group of boxes");
        };
        assert_eq!(children.len(), 3);
        let b: Vec<f64> = children.iter().map(|c| c.bounds().x0.round()).collect();
        assert_eq!(b, vec![0.0, 8.0, 16.0]);
        assert!(children
            .iter()
            .all(|c| (c.bounds().width() - 6.0).abs() < 1e-6));
    }

    /// Remove Strays takes lone points, unpainted curves and empty texts,
    /// and leaves painted objects.
    #[test]
    fn remove_strays_takes_points_unpainted_curves_and_empty_text() {
        let mut app = App::headless();
        let keep = rect(&mut app, Rect::new(0.0, 0.0, 10.0, 10.0));
        let mut dot = BezPath::new();
        dot.move_to((5.0, 5.0));
        let point = app
            .new_shape(ShapeKind::Path {
                path: dot,
                closed: false,
            })
            .expect("point");
        let mut line = BezPath::new();
        line.move_to((0.0, 0.0));
        line.line_to((10.0, 10.0));
        let bare = app
            .new_shape(ShapeKind::Path {
                path: line,
                closed: false,
            })
            .expect("line");
        app.engine
            .run(&Command::SetStroke {
                shapes: vec![bare],
                stroke: None,
            })
            .expect("no outline");
        app.engine
            .run(&Command::SetFill {
                shapes: vec![bare],
                fill: Fill::None,
            })
            .expect("no fill");
        let found = app.strays();
        assert!(found.contains(&point) && found.contains(&bare), "{found:?}");
        assert!(!found.contains(&keep));
        app.remove_strays();
        let left: Vec<ShapeId> = top_level(&app).iter().map(|s| s.id).collect();
        assert_eq!(left, vec![keep]);
    }
}
