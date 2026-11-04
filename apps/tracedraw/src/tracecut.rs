//! TraceCut: the tools for cutting plotters, in a window of their own
//! (Launch > TraceCut, Tools > TraceCut). The geometry lives in the
//! `tracedraw-tracecut` crate; this module lists the tools, keeps their
//! settings and presets, works out a tool's result for the preview and
//! turns it into commands, one undo step for OK. The window itself is
//! `ui/tracecut.rs`. See docs/behavior/tracecut.md.
//!
//! Adapted from the Sign & Cut tools of #170 by LaMka21.

use crate::app::App;
use crate::i18n::{tr, trf};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tracedraw_core::geometry::{BezPath, Point, Rect, Shape as _};
use tracedraw_core::{Command, ShapeId};
use tracedraw_tracecut::{cleanup, crossings};

/// The groups of the tool list, in the order they are shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Cut,
    Nesting,
    Measure,
}

impl Group {
    pub const ALL: [Group; 3] = [Group::Cut, Group::Nesting, Group::Measure];

    /// The i18n key of the group's name.
    pub fn key(self) -> &'static str {
        match self {
            Group::Cut => "tracecut.group_cut",
            Group::Nesting => "tracecut.group_nesting",
            Group::Measure => "tracecut.group_measure",
        }
    }

    /// The tools of the group, in the order they are listed.
    pub fn tools(self) -> Vec<ToolId> {
        ToolId::ALL
            .iter()
            .copied()
            .filter(|t| t.group() == self)
            .collect()
    }
}

/// A TraceCut tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ToolId {
    #[default]
    FindDuplicates,
    FindCrossings,
}

impl ToolId {
    pub const ALL: [ToolId; 2] = [ToolId::FindDuplicates, ToolId::FindCrossings];

    pub fn group(self) -> Group {
        match self {
            ToolId::FindDuplicates | ToolId::FindCrossings => Group::Cut,
        }
    }

    /// The name used for the tool's presets (never translated).
    pub fn id(self) -> &'static str {
        match self {
            ToolId::FindDuplicates => "find_duplicates",
            ToolId::FindCrossings => "find_crossings",
        }
    }

    /// The i18n key of the tool's name; `<key>_desc` describes it.
    pub fn key(self) -> &'static str {
        match self {
            ToolId::FindDuplicates => "tracecut.find_duplicates",
            ToolId::FindCrossings => "tracecut.find_crossings",
        }
    }
}

/// What TraceCut keeps in the settings: whether it is shown and the
/// tool picked last.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TraceCutPrefs {
    /// Options > General: TraceCut is listed in Launch and in the Tools
    /// menu.
    pub show: bool,
    pub tool: ToolId,
}

impl Default for TraceCutPrefs {
    fn default() -> Self {
        TraceCutPrefs {
            show: true,
            tool: ToolId::default(),
        }
    }
}

/// Find Duplicates: objects lying on top of an earlier object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DuplicatesSettings {
    /// How far two outlines may lie apart and still be the same (mm).
    pub tolerance: f64,
    /// OK deletes the repeats (else it selects them).
    pub delete: bool,
}

impl Default for DuplicatesSettings {
    fn default() -> Self {
        DuplicatesSettings {
            tolerance: 0.05,
            delete: true,
        }
    }
}

/// Find Crossings: places where outlines cross.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CrossingsSettings {
    /// Crossings closer than this are one (mm).
    pub tolerance: f64,
    /// Also crossings between different objects (else each object's own).
    pub between: bool,
}

impl Default for CrossingsSettings {
    fn default() -> Self {
        CrossingsSettings {
            tolerance: 0.05,
            between: false,
        }
    }
}

/// Tolerances the clean-up tools accept (mm).
pub const TOLERANCE_RANGE: (f64, f64) = (0.001, 10.0);

/// The settings of every tool, as the window shows them.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ToolSettings {
    pub duplicates: DuplicatesSettings,
    pub crossings: CrossingsSettings,
}

impl ToolSettings {
    /// `tool`'s settings as JSON (for its presets and to tell when a
    /// result is out of date).
    pub fn to_json(&self, tool: ToolId) -> serde_json::Value {
        let v = match tool {
            ToolId::FindDuplicates => serde_json::to_value(&self.duplicates),
            ToolId::FindCrossings => serde_json::to_value(&self.crossings),
        };
        v.unwrap_or(serde_json::Value::Null)
    }

    /// Load `tool`'s settings from JSON; false when it does not read.
    pub fn load_json(&mut self, tool: ToolId, v: &serde_json::Value) -> bool {
        match tool {
            ToolId::FindDuplicates => serde_json::from_value(v.clone())
                .map(|s| self.duplicates = s)
                .is_ok(),
            ToolId::FindCrossings => serde_json::from_value(v.clone())
                .map(|s| self.crossings = s)
                .is_ok(),
        }
    }

    /// Put `tool`'s settings back to their defaults.
    pub fn reset(&mut self, tool: ToolId) {
        match tool {
            ToolId::FindDuplicates => self.duplicates = DuplicatesSettings::default(),
            ToolId::FindCrossings => self.crossings = CrossingsSettings::default(),
        }
    }
}

/// The named presets of one tool, kept as JSON values.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolPresets {
    /// The preset chosen last.
    pub current: Option<String>,
    pub items: BTreeMap<String, serde_json::Value>,
}

/// Every tool's presets, kept in `tracecut_presets.json` in the settings
/// folder (in memory only where there is none, as in the browser, and in
/// tests).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PresetStore {
    #[serde(default)]
    pub tools: BTreeMap<String, ToolPresets>,
    #[serde(skip)]
    loaded: bool,
}

/// The file the presets are kept in, in the settings folder.
pub const PRESETS_FILE: &str = "tracecut_presets.json";

impl PresetStore {
    fn path() -> Option<std::path::PathBuf> {
        if cfg!(test) {
            return None;
        }
        crate::settings::config_dir().map(|d| d.join(PRESETS_FILE))
    }

    /// The store read from `text`; anything that does not read gives an
    /// empty store.
    pub fn from_json(text: &str) -> PresetStore {
        match serde_json::from_str::<PresetStore>(text) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("TraceCut presets: {e}");
                PresetStore::default()
            }
        }
    }

    fn load(&mut self) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        let Some(p) = Self::path() else { return };
        let Ok(text) = crate::files::read_to_string(&p) else {
            return;
        };
        self.tools = PresetStore::from_json(&text).tools;
    }

    fn save(&self) {
        let Some(p) = Self::path() else { return };
        match serde_json::to_string_pretty(self) {
            Ok(text) => {
                if let Err(e) = crate::files::write_creating(&p, text) {
                    log::warn!("TraceCut presets: {e}");
                }
            }
            Err(e) => log::warn!("TraceCut presets: {e}"),
        }
    }

    /// The presets of `tool`, read from disk the first time.
    pub fn tool(&mut self, tool: ToolId) -> &mut ToolPresets {
        self.load();
        self.tools.entry(tool.id().to_string()).or_default()
    }

    /// Keep `settings` of `tool` as the preset `name` (replacing one of
    /// that name) and make it the current one.
    pub fn save_preset(&mut self, tool: ToolId, name: &str, settings: &ToolSettings) -> bool {
        let name = name.trim();
        if name.is_empty() {
            return false;
        }
        let v = settings.to_json(tool);
        let p = self.tool(tool);
        p.items.insert(name.to_string(), v);
        p.current = Some(name.to_string());
        self.save();
        true
    }

    /// Load the preset `name` of `tool` into `settings`.
    pub fn apply_preset(&mut self, tool: ToolId, name: &str, settings: &mut ToolSettings) -> bool {
        let p = self.tool(tool);
        let Some(v) = p.items.get(name).cloned() else {
            return false;
        };
        p.current = Some(name.to_string());
        let ok = settings.load_json(tool, &v);
        self.save();
        ok
    }

    /// Delete the preset `name` of `tool`.
    pub fn delete_preset(&mut self, tool: ToolId, name: &str) -> bool {
        let p = self.tool(tool);
        let gone = p.items.remove(name.trim()).is_some();
        if p.current.as_deref() == Some(name.trim()) {
            p.current = None;
        }
        if gone {
            self.save();
        }
        gone
    }
}

/// What a tool found, ready to show in the preview and to apply.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Each object repeating an earlier one, with that one.
    Duplicates(Vec<(ShapeId, ShapeId)>),
    /// The places where outlines cross, and the objects that have them.
    Crossings {
        points: Vec<Point>,
        objects: Vec<ShapeId>,
    },
}

impl Outcome {
    /// The line under the preview that says what was found.
    pub fn summary(&self) -> String {
        match self {
            Outcome::Duplicates(d) if d.is_empty() => tr("tracecut.no_duplicates"),
            Outcome::Duplicates(d) => {
                trf("tracecut.duplicates_found", &[("n", &d.len().to_string())])
            }
            Outcome::Crossings { points, .. } if points.is_empty() => tr("tracecut.no_crossings"),
            Outcome::Crossings { points, objects } => trf(
                "tracecut.crossings_found",
                &[
                    ("n", &points.len().to_string()),
                    ("objects", &objects.len().to_string()),
                ],
            ),
        }
    }
}

/// A result worked out by Apply, with what it was worked out from, so
/// that a change of the drawing, the selection or the settings shows it
/// is out of date.
#[derive(Debug, Clone, PartialEq)]
pub struct Worked {
    pub tool: ToolId,
    pub settings: serde_json::Value,
    pub revision: u64,
    pub targets: Vec<ShapeId>,
    pub outcome: Outcome,
}

/// The TraceCut window between frames.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TraceCutState {
    pub tool: ToolId,
    pub settings: ToolSettings,
    /// The last result of Apply.
    pub worked: Option<Worked>,
    pub presets: PresetStore,
    /// The preset row's name field and whether it is shown.
    pub preset_name: String,
    pub preset_edit: bool,
}

impl TraceCutState {
    /// The window opened on `tool`.
    pub fn new(tool: ToolId) -> TraceCutState {
        TraceCutState {
            tool,
            ..Default::default()
        }
    }
}

/// What the preview draws: page-space outlines with their roles.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scene {
    /// The objects the tool works on.
    pub objects: Vec<BezPath>,
    /// Outlines the result points at (drawn over the objects).
    pub marked: Vec<BezPath>,
    /// Points the result points at (drawn as rings).
    pub points: Vec<Point>,
    pub extent: Option<Rect>,
}

impl App {
    /// Open the TraceCut window on the tool picked last.
    pub fn open_tracecut(&mut self) {
        self.dialog = crate::ui::dialogs::Dialog::TraceCut(Box::new(TraceCutState::new(
            self.settings.tracecut.tool,
        )));
    }

    /// The objects the tools work on: the selection, in stacking order
    /// (bottom first); objects picked inside groups come after, in the
    /// order picked.
    pub fn tracecut_targets(&self) -> Vec<ShapeId> {
        let doc = self.doc();
        let mut ordered: Vec<ShapeId> = Vec::new();
        for layer in self.page_layers() {
            for s in &layer.shapes {
                if self.selection.contains(&s.id) {
                    ordered.push(s.id);
                }
            }
        }
        for id in &self.selection {
            if !ordered.contains(id) && doc.find_shape(*id).is_some() {
                ordered.push(*id);
            }
        }
        ordered
    }

    /// The outlines of each object in page space.
    pub fn tracecut_outlines(&self, ids: &[ShapeId]) -> Vec<Vec<BezPath>> {
        let doc = self.doc();
        ids.iter()
            .map(|id| {
                doc.shape_on_page(*id)
                    .map(|s| tracedraw_tracecut::outlines(&s))
                    .unwrap_or_default()
            })
            .collect()
    }

    /// Work out what `tool` finds on the targets with `settings`.
    pub fn tracecut_work_out(&self, tool: ToolId, settings: &ToolSettings) -> Worked {
        let targets = self.tracecut_targets();
        let outlines = self.tracecut_outlines(&targets);
        let clamp = |t: f64| {
            if t.is_finite() {
                t.clamp(TOLERANCE_RANGE.0, TOLERANCE_RANGE.1)
            } else {
                DuplicatesSettings::default().tolerance
            }
        };
        let outcome = match tool {
            ToolId::FindDuplicates => {
                let found =
                    cleanup::find_duplicates(&outlines, clamp(settings.duplicates.tolerance));
                Outcome::Duplicates(
                    found
                        .iter()
                        .filter_map(|(i, o)| Some((*targets.get(*i)?, *targets.get(*o)?)))
                        .collect(),
                )
            }
            ToolId::FindCrossings => {
                let found = crossings::find_crossings(
                    &outlines,
                    settings.crossings.between,
                    clamp(settings.crossings.tolerance),
                );
                let mut objects: Vec<ShapeId> = Vec::new();
                for c in &found {
                    for i in [c.a, c.b] {
                        if let Some(id) = targets.get(i) {
                            if !objects.contains(id) {
                                objects.push(*id);
                            }
                        }
                    }
                }
                Outcome::Crossings {
                    points: found.iter().map(|c| c.point).collect(),
                    objects,
                }
            }
        };
        Worked {
            tool,
            settings: settings.to_json(tool),
            revision: self.engine.revision(),
            targets,
            outcome,
        }
    }

    /// Whether `w` still matches the drawing, the selection and the
    /// window's tool and settings.
    pub fn tracecut_current(&self, w: &Worked, tool: ToolId, settings: &ToolSettings) -> bool {
        w.tool == tool
            && w.revision == self.engine.revision()
            && w.settings == settings.to_json(tool)
            && w.targets == self.tracecut_targets()
    }

    /// What the preview shows: the targets and, when `worked` is current,
    /// what it found.
    pub fn tracecut_scene(&self, worked: Option<&Worked>) -> Scene {
        let targets = self.tracecut_targets();
        let outlines = self.tracecut_outlines(&targets);
        let mut scene = Scene::default();
        for o in &outlines {
            scene.objects.extend(o.iter().cloned());
        }
        if let Some(w) = worked {
            match &w.outcome {
                Outcome::Duplicates(d) => {
                    for (dup, _) in d {
                        if let Some(i) = targets.iter().position(|t| t == dup) {
                            if let Some(o) = outlines.get(i) {
                                scene.marked.extend(o.iter().cloned());
                            }
                        }
                    }
                }
                Outcome::Crossings { points, .. } => scene.points = points.clone(),
            }
        }
        scene.extent = scene
            .objects
            .iter()
            .map(|p| p.bounding_box())
            .filter(|b| {
                b.x0.is_finite() && b.y0.is_finite() && b.x1.is_finite() && b.y1.is_finite()
            })
            .reduce(|a, b| a.union(b));
        scene
    }

    /// Apply `w` to the drawing (OK): Find Duplicates deletes the repeats
    /// in one undo step (or selects them), Find Crossings selects the
    /// objects that have crossings. Returns whether anything was done;
    /// the status line says what.
    pub fn tracecut_commit(&mut self, w: &Worked, settings: &ToolSettings) -> bool {
        match &w.outcome {
            Outcome::Duplicates(d) => {
                let doc = self.doc();
                let dups: Vec<ShapeId> = d
                    .iter()
                    .map(|(id, _)| *id)
                    .filter(|id| doc.find_shape(*id).is_some_and(|s| !s.locked))
                    .collect();
                let n = dups.len();
                if n == 0 {
                    self.status = tr("tracecut.no_duplicates");
                    return false;
                }
                if settings.duplicates.delete {
                    if let Err(e) = self.engine.run_batch(
                        "Delete Duplicates",
                        &[Command::DeleteShapes {
                            shapes: dups.clone(),
                        }],
                    ) {
                        self.status = e.to_string();
                        return false;
                    }
                    self.selection.retain(|id| !dups.contains(id));
                    self.status = trf("tracecut.duplicates_deleted", &[("n", &n.to_string())]);
                } else {
                    self.select(dups);
                    self.status = trf("tracecut.duplicates_selected", &[("n", &n.to_string())]);
                }
                true
            }
            Outcome::Crossings { points, objects } => {
                if points.is_empty() {
                    self.status = tr("tracecut.no_crossings");
                    return false;
                }
                self.select(objects.clone());
                self.status = w.outcome.summary();
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::dialogs::Dialog;
    use tracedraw_core::ShapeKind;

    fn rect(app: &mut App, x: f64, y: f64, w: f64, h: f64) -> ShapeId {
        app.new_shape(ShapeKind::Rect {
            rect: Rect::new(x, y, x + w, y + h),
            radius: 0.0,
            corners: None,
        })
        .expect("rect")
    }

    #[test]
    fn every_tool_is_in_one_group_and_has_strings() {
        let mut listed = Vec::new();
        for g in Group::ALL {
            assert_ne!(tr(g.key()), g.key(), "{:?}", g);
            listed.extend(g.tools());
        }
        assert_eq!(listed, ToolId::ALL.to_vec());
        for t in ToolId::ALL {
            assert_ne!(tr(t.key()), t.key(), "{t:?}");
            let desc = format!("{}_desc", t.key());
            assert_ne!(tr(&desc), desc, "{t:?}");
        }
    }

    #[test]
    fn duplicates_are_found_and_ok_deletes_them_in_one_step() {
        let mut app = App::headless();
        let a = rect(&mut app, 10.0, 10.0, 40.0, 20.0);
        let b = rect(&mut app, 10.0, 10.0, 40.0, 20.0);
        let c = rect(&mut app, 80.0, 10.0, 10.0, 10.0);
        app.select(vec![a, b, c]);
        let settings = ToolSettings::default();
        let w = app.tracecut_work_out(ToolId::FindDuplicates, &settings);
        assert_eq!(w.outcome, Outcome::Duplicates(vec![(b, a)]));
        // Working it out changes nothing.
        assert!(app.doc().find_shape(b).is_some());
        let undo_before = app.engine.history_labels().0.len();
        assert!(app.tracecut_commit(&w, &settings));
        assert!(app.doc().find_shape(b).is_none());
        assert!(app.doc().find_shape(a).is_some() && app.doc().find_shape(c).is_some());
        let (undo, _) = app.engine.history_labels();
        assert_eq!(undo.len(), undo_before + 1);
        assert_eq!(undo.last().copied(), Some("Delete Duplicates"));
        app.undo();
        assert!(app.doc().find_shape(b).is_some());
    }

    #[test]
    fn duplicates_can_be_selected_instead() {
        let mut app = App::headless();
        let a = rect(&mut app, 0.0, 0.0, 20.0, 20.0);
        let b = rect(&mut app, 0.0, 0.0, 20.0, 20.0);
        app.select(vec![a, b]);
        let mut settings = ToolSettings::default();
        settings.duplicates.delete = false;
        let w = app.tracecut_work_out(ToolId::FindDuplicates, &settings);
        let rev = app.engine.revision();
        assert!(app.tracecut_commit(&w, &settings));
        assert_eq!(app.selection, vec![b]);
        assert_eq!(app.engine.revision(), rev);
    }

    #[test]
    fn crossings_select_the_objects_that_cross() {
        let mut app = App::headless();
        let a = rect(&mut app, 0.0, 0.0, 20.0, 20.0);
        let b = rect(&mut app, 10.0, 10.0, 20.0, 20.0);
        let c = rect(&mut app, 100.0, 100.0, 5.0, 5.0);
        app.select(vec![a, b, c]);
        let mut settings = ToolSettings::default();
        let w = app.tracecut_work_out(ToolId::FindCrossings, &settings);
        assert!(matches!(&w.outcome, Outcome::Crossings { points, .. } if points.is_empty()));
        settings.crossings.between = true;
        let w = app.tracecut_work_out(ToolId::FindCrossings, &settings);
        let Outcome::Crossings { points, objects } = &w.outcome else {
            panic!("crossings");
        };
        assert_eq!(points.len(), 2);
        assert_eq!(objects, &vec![a, b]);
        assert!(app.tracecut_commit(&w, &settings));
        assert_eq!(app.selection, vec![a, b]);
    }

    #[test]
    fn a_result_goes_out_of_date_with_the_drawing_selection_or_settings() {
        let mut app = App::headless();
        let a = rect(&mut app, 0.0, 0.0, 20.0, 20.0);
        let b = rect(&mut app, 0.0, 0.0, 20.0, 20.0);
        app.select(vec![a, b]);
        let mut settings = ToolSettings::default();
        let w = app.tracecut_work_out(ToolId::FindDuplicates, &settings);
        assert!(app.tracecut_current(&w, ToolId::FindDuplicates, &settings));
        assert!(!app.tracecut_current(&w, ToolId::FindCrossings, &settings));
        settings.duplicates.tolerance = 1.0;
        assert!(!app.tracecut_current(&w, ToolId::FindDuplicates, &settings));
        settings.duplicates.tolerance = DuplicatesSettings::default().tolerance;
        app.select(vec![a]);
        assert!(!app.tracecut_current(&w, ToolId::FindDuplicates, &settings));
        app.select(vec![a, b]);
        rect(&mut app, 50.0, 50.0, 5.0, 5.0);
        app.select(vec![a, b]);
        assert!(!app.tracecut_current(&w, ToolId::FindDuplicates, &settings));
    }

    #[test]
    fn the_scene_marks_what_was_found() {
        let mut app = App::headless();
        let a = rect(&mut app, 0.0, 0.0, 20.0, 20.0);
        let b = rect(&mut app, 0.0, 0.0, 20.0, 20.0);
        app.select(vec![a, b]);
        let w = app.tracecut_work_out(ToolId::FindDuplicates, &ToolSettings::default());
        let scene = app.tracecut_scene(Some(&w));
        assert_eq!(scene.objects.len(), 2);
        assert_eq!(scene.marked.len(), 1);
        let e = scene.extent.expect("extent");
        assert!((e.width() - 20.0).abs() < 1e-6);
        assert!(app.tracecut_scene(None).marked.is_empty());
    }

    #[test]
    fn presets_save_load_and_delete() {
        let mut store = PresetStore::default();
        let mut s = ToolSettings::default();
        s.duplicates.tolerance = 0.5;
        s.duplicates.delete = false;
        assert!(!store.save_preset(ToolId::FindDuplicates, "  ", &s));
        assert!(store.save_preset(ToolId::FindDuplicates, "Coarse", &s));
        let mut t = ToolSettings::default();
        assert!(store.apply_preset(ToolId::FindDuplicates, "Coarse", &mut t));
        assert_eq!(t.duplicates, s.duplicates);
        // Each tool keeps its own.
        assert!(!store.apply_preset(ToolId::FindCrossings, "Coarse", &mut t));
        // The file round trip.
        let text = serde_json::to_string(&store).expect("json");
        let mut back = PresetStore::from_json(&text);
        assert_eq!(
            back.tool(ToolId::FindDuplicates).current.as_deref(),
            Some("Coarse")
        );
        assert!(back.delete_preset(ToolId::FindDuplicates, "Coarse"));
        assert!(back.tool(ToolId::FindDuplicates).items.is_empty());
        assert!(back.tool(ToolId::FindDuplicates).current.is_none());
        // A damaged file gives no presets.
        assert!(PresetStore::from_json("{not json").tools.is_empty());
    }

    #[test]
    fn a_preset_of_another_shape_keeps_the_defaults_it_lacks() {
        let mut s = ToolSettings::default();
        assert!(s.load_json(
            ToolId::FindCrossings,
            &serde_json::json!({ "between": true, "unknown": 3 })
        ));
        assert!(s.crossings.between);
        assert_eq!(
            s.crossings.tolerance,
            CrossingsSettings::default().tolerance
        );
        s.reset(ToolId::FindCrossings);
        assert!(!s.crossings.between);
    }

    #[test]
    fn open_tracecut_starts_on_the_tool_picked_last() {
        let mut app = App::headless();
        app.settings.tracecut.tool = ToolId::FindCrossings;
        app.open_tracecut();
        match &app.dialog {
            Dialog::TraceCut(st) => assert_eq!(st.tool, ToolId::FindCrossings),
            d => panic!("{d:?}"),
        }
    }
}
