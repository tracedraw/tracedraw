//! Reusable vector motifs and Fit Objects to Path settings, kept locally.
//! Placement and live updates use the existing fit commands and algorithm.

use crate::ui::btn_icons::{Ib, IconUi};
use crate::ui::tips::RichTip;
use crate::{app::App, fit_path::FitPathSettings};
use serde::{Deserialize, Serialize};
use tracedraw_core::{Affine, Command, Shape, ShapeId, ShapeKind};

const VERSION: u32 = 1;
const MAX_PRESETS: usize = 100;
const MAX_BYTES: usize = 256 * 1024;
const MAX_LIBRARY_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SprayerPreset {
    pub version: u32,
    pub name: String,
    pub motifs: Vec<Shape>,
    pub settings: FitPathSettings,
}

fn bounded_json<T: Serialize>(value: &T) -> Option<Vec<u8>> {
    struct Limited(Vec<u8>);
    impl std::io::Write for Limited {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > MAX_BYTES.saturating_sub(self.0.len()) {
                return Err(std::io::Error::other("sprayer preset too large"));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut out = Limited(Vec::new());
    serde_json::to_writer(&mut out, value).ok()?;
    Some(out.0)
}

fn valid_shape(s: &Shape, depth: usize, remaining: &mut usize) -> bool {
    if depth > 16 || *remaining == 0 {
        return false;
    }
    *remaining -= 1;
    let coeffs = s.transform.as_coeffs();
    if !coeffs.iter().all(|v| v.is_finite() && v.abs() <= 1e6)
        || s.transform.determinant().abs() < 1e-12
        || s.style.is_some()
        || s.raster_fx.is_some()
        || !s.parked_effects.is_empty()
        || s.effects.iter().any(|e| !crate::style_presets::portable(e))
    {
        return false;
    }
    match &s.kind {
        ShapeKind::Rect { .. } | ShapeKind::Ellipse { .. } => true,
        ShapeKind::Polygon { points, .. } => (3..=1000).contains(points),
        ShapeKind::Path { path, .. } => path.elements().len() <= 10_000,
        ShapeKind::Group { children } => {
            !children.is_empty()
                && children
                    .iter()
                    .all(|c| valid_shape(c, depth + 1, remaining))
        }
        _ => false,
    }
}

// These fills and named inks can contain drawing-specific resources.
// Reject them throughout the object, including masks and additional paints.
fn independent_paint(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(m) => {
            if m.get("model").and_then(|v| v.as_str()) == Some("spot") {
                return false;
            }
            if m.get("fill")
                .and_then(|v| v.as_str())
                .is_some_and(|f| !matches!(f, "none" | "solid" | "fountain"))
            {
                return false;
            }
            m.values().all(independent_paint)
        }
        serde_json::Value::Array(a) => a.iter().all(independent_paint),
        _ => true,
    }
}

impl SprayerPreset {
    fn encoded(&self) -> Option<Vec<u8>> {
        if self.version != VERSION
            || self.name.trim().is_empty()
            || self.name.chars().count() > 80
            || self.motifs.is_empty()
            || self.motifs.len() > 32
        {
            return None;
        }
        let f = &self.settings;
        if ![
            f.distance,
            f.offset,
            f.end_offset,
            f.side,
            f.angle,
            f.start_angle,
            f.spin_angle,
            f.revolutions,
            f.range,
        ]
        .iter()
        .all(|v| v.is_finite() && v.abs() <= 1e6)
            || f.distance <= 0.0
            || f.count == 0
            || f.count > 1000
            || f.origin > 8
        {
            return None;
        }
        let mut remaining = 128;
        if !self
            .motifs
            .iter()
            .all(|s| valid_shape(s, 0, &mut remaining))
        {
            return None;
        }
        let bytes = bounded_json(self)?;
        let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
        if !independent_paint(&value) {
            return None;
        }
        // Non-finite model numbers serialize to null and cannot round-trip.
        serde_json::from_slice::<Self>(&bytes).ok()?;
        Some(bytes)
    }
}

/// A malformed or future preset must not discard unrelated preferences.
pub(crate) fn deserialize_library<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<SprayerPreset>, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    let Some(items) = value.as_array() else {
        return Ok(Vec::new());
    };
    let mut list: Vec<SprayerPreset> = Vec::new();
    let mut bytes = 0;
    for value in items.iter().take(MAX_PRESETS) {
        let Ok(preset) = serde_json::from_value::<SprayerPreset>(value.clone()) else {
            continue;
        };
        let Some(encoded) = preset.encoded() else {
            continue;
        };
        if bytes + encoded.len() > MAX_LIBRARY_BYTES || list.iter().any(|p| p.name == preset.name) {
            continue;
        }
        bytes += encoded.len();
        list.push(preset);
    }
    Ok(list)
}

fn detach(s: &mut Shape) {
    s.id = ShapeId(0);
    s.uid = None;
    s.style = None;
    s.data.clear();
    s.link = None;
    s.content_link = None;
    s.toc_level = None;
    s.toc = None;
    s.page_number = None;
    s.locked = false;
    if let ShapeKind::Group { children } = &mut s.kind {
        for child in children {
            detach(child);
        }
    }
}

impl App {
    fn sprayer_source(&self) -> Option<(Vec<Shape>, FitPathSettings)> {
        if let Some((path, fit)) = self.live_fit_for_selection() {
            let index = self.shapes_by_uid();
            // Do not silently omit other selected objects or another fit.
            if !self.selection.iter().all(|id| {
                *id == path
                    || self
                        .doc()
                        .find_shape(*id)
                        .and_then(|s| s.uid)
                        .map(crate::fit_live::uid_text)
                        .is_some_and(|uid| {
                            fit.members.iter().any(|m| m.uid == uid) || fit.copies.contains(&uid)
                        })
            }) {
                return None;
            }
            let mut sources = self.live_fit_sources(path, &fit, &index)?;
            sources.pop();
            if sources.len() != fit.members.len() {
                return None;
            }
            return Some((sources, fit.settings));
        }
        let mut shapes = self.selected_shapes();
        if shapes.len() > 1 {
            let guide = shapes.pop()?;
            if !matches!(guide.kind, ShapeKind::Path { .. }) {
                return None;
            }
        }
        (!shapes.is_empty()).then_some((shapes, self.fit_path))
    }

    /// Save a detached motif or the original motifs of a selected live fit.
    /// Replacement is explicit; a duplicate name does not overwrite silently.
    pub fn save_sprayer_preset(
        &mut self,
        name: &str,
        replace: Option<usize>,
    ) -> Result<usize, &'static str> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 80 {
            return Err("sprayer.invalid_name");
        }
        let list = &self.settings.sprayer_presets;
        if replace.is_some_and(|i| i >= list.len())
            || list
                .iter()
                .enumerate()
                .any(|(i, p)| p.name == name && replace != Some(i))
        {
            return Err("sprayer.duplicate_name");
        }
        if replace.is_none() && list.len() >= MAX_PRESETS {
            return Err("sprayer.limit");
        }
        let (mut motifs, mut settings) = self.sprayer_source().ok_or("sprayer.select_source")?;
        let mut remaining = 128;
        if !motifs.iter().all(|s| valid_shape(s, 0, &mut remaining)) {
            return Err("sprayer.unsupported");
        }
        // A common translation keeps spatial sorting of a motif sequence.
        let center = motifs
            .iter()
            .map(Shape::bounds)
            .reduce(|a, b| a.union(b))
            .ok_or("sprayer.select_source")?
            .center();
        for s in &mut motifs {
            if self.pivots.single.contains_key(&s.id) {
                return Err("sprayer.unsupported");
            }
            s.transform_by(Affine::translate(-center.to_vec2()));
            detach(s);
        }
        // Templates are placed directly; no unused original is inserted.
        settings.keep_originals = false;
        let preset = SprayerPreset {
            version: VERSION,
            name: name.into(),
            motifs,
            settings,
        };
        let encoded = preset.encoded().ok_or("sprayer.unsupported")?;
        let total = list
            .iter()
            .enumerate()
            .filter(|(i, _)| Some(*i) != replace)
            .try_fold(encoded.len(), |size, (_, p)| {
                Some(size + p.encoded()?.len())
            })
            .ok_or("sprayer.limit")?;
        if total > MAX_LIBRARY_BYTES {
            return Err("sprayer.limit");
        }
        let at = if let Some(i) = replace {
            self.settings.sprayer_presets[i] = preset;
            i
        } else {
            self.settings.sprayer_presets.push(preset);
            self.settings.sprayer_presets.len() - 1
        };
        self.settings.save();
        self.sprayer_preset_pictures.clear();
        Ok(at)
    }

    pub fn delete_sprayer_preset(&mut self, index: usize) {
        if index < self.settings.sprayer_presets.len() {
            self.settings.sprayer_presets.remove(index);
            self.sprayer_preset_pictures.clear();
            self.settings.save();
        }
    }

    /// Insert sources and place them in one undo step, with fresh identities.
    pub fn apply_sprayer_preset(&mut self, index: usize) -> Result<(), &'static str> {
        if self.selection.len() != 1 || self.live_fit_for_selection().is_some() {
            return Err("sprayer.select_path");
        }
        let Some(guide) = self.selected_shapes().into_iter().next() else {
            return Err("sprayer.select_path");
        };
        if !matches!(guide.kind, ShapeKind::Path { .. }) || guide.locked || !guide.visible {
            return Err("sprayer.select_path");
        }
        let top = self
            .doc()
            .ancestors(guide.id)
            .first()
            .copied()
            .unwrap_or(guide.id);
        let layer = self
            .doc()
            .locate(top)
            .map(|(l, _)| l)
            .map_err(|_| "sprayer.select_path")?;
        if !self
            .page_layers()
            .iter()
            .any(|l| l.id == layer && !l.locked && l.visible)
        {
            return Err("sprayer.select_path");
        }
        let preset = self
            .settings
            .sprayer_presets
            .get(index)
            .ok_or("sprayer.select_preset")?;
        preset.encoded().ok_or("sprayer.unsupported")?;
        let preset = preset.clone();
        let mut shapes = Vec::new();
        let mut commands = Vec::new();
        for mut source in preset.motifs {
            detach(&mut source);
            let id = self.engine.new_shape_id();
            let source = crate::app::reid_pub(source, id, &mut self.engine);
            commands.push(Command::AddShape {
                layer,
                shape: source.clone(),
            });
            shapes.push(source);
        }
        let guide_id = guide.id;
        shapes.push(guide);
        let previous = self.fit_path;
        self.fit_path = FitPathSettings {
            keep_originals: false,
            ..preset.settings
        };
        let applied = self.fit_shapes_to_path(&shapes, commands);
        self.fit_path = previous;
        if !applied {
            return Err("sprayer.cannot_fit");
        }
        self.select(vec![guide_id]);
        Ok(())
    }
}

/// Runtime-only thumbnail. Failed previews are cached as well.
/// At most one texture per library entry, capped at 128 x 128 pixels.
pub(crate) struct Thumbnail {
    pixels: u32,
    texture: Option<egui::TextureHandle>,
}

fn thumbnail(preset: &SprayerPreset, pixels: u32) -> Option<(Vec<u8>, u32, u32)> {
    let bounds = preset
        .motifs
        .iter()
        .map(Shape::visual_bounds)
        .reduce(|a, b| a.union(b))?;
    let pad = (bounds.width().max(bounds.height()) * 0.06).max(0.3);
    let bounds = bounds.inflate(pad, pad);
    let longest = bounds.width().max(bounds.height());
    if !longest.is_finite() || longest <= 0.0 {
        return None;
    }
    let pixels = pixels.clamp(8, 128) as f64;
    let zoom = pixels / longest;
    let w = (bounds.width() * zoom).ceil().clamp(1., pixels) as u32;
    let h = (bounds.height() * zoom).ceil().clamp(1., pixels) as u32;
    let mut doc = tracedraw_core::Document::new(
        "sprayer preview",
        tracedraw_core::Size::new(bounds.width(), bounds.height()),
    );
    let page = doc.pages.first()?.id;
    let layer = doc.pages.first()?.layers.first()?.id;
    // Detached templates have zero ids; preview objects need distinct ids too.
    fn identify(s: &mut Shape, next: &mut u64) {
        s.id = ShapeId(*next);
        *next += 1;
        if let ShapeKind::Group { children } = &mut s.kind {
            for c in children {
                identify(c, next);
            }
        }
    }
    let mut next = 1;
    for motif in &preset.motifs {
        let mut shape = motif.clone();
        identify(&mut shape, &mut next);
        shape.transform_by(Affine::translate((-bounds.x0, -bounds.y0)));
        doc.layer_mut(layer).ok()?.shapes.push(shape);
    }
    let image = tracedraw_render::render_page(
        &doc,
        page,
        &tracedraw_render::RenderOptions {
            width: w,
            height: h,
            view: tracedraw_render::ViewTransform {
                zoom,
                origin_x: 0.,
                origin_y: h as f64,
                stretch_y: 1.,
            },
            anti_alias: true,
            draft: true,
            ..Default::default()
        },
    )?;
    Some((image.data().to_vec(), w, h))
}

impl App {
    fn sprayer_picture(
        &mut self,
        ctx: &egui::Context,
        index: usize,
        pixels: u32,
    ) -> Option<egui::TextureHandle> {
        let pixels = pixels.clamp(8, 128);
        let preset = self.settings.sprayer_presets.get(index)?;
        if let Some(Some(entry)) = self.sprayer_preset_pictures.get(index) {
            if entry.pixels == pixels {
                return entry.texture.clone();
            }
        }
        let texture = thumbnail(preset, pixels).map(|(rgba, w, h)| {
            ctx.load_texture(
                format!("sprayer:{index}"),
                egui::ColorImage::from_rgba_premultiplied([w as usize, h as usize], &rgba),
                egui::TextureOptions::LINEAR,
            )
        });
        self.sprayer_preset_pictures
            .resize_with(self.settings.sprayer_presets.len(), || None);
        if let Some(entry) = self.sprayer_preset_pictures.get_mut(index) {
            *entry = Some(Thumbnail {
                pixels,
                texture: texture.clone(),
            });
        }
        texture
    }
}

fn tile_id(index: usize) -> egui::Id {
    egui::Id::new(("sprayer_tile", index))
}

#[derive(Default)]
pub(crate) struct LibraryBrowser {
    pub open: bool,
    search: String,
    list: bool,
    selected: Option<usize>,
    rename: String,
    focus_search: bool,
}

fn filtered_presets(presets: &[SprayerPreset], query: &str) -> Vec<usize> {
    let query = query.to_lowercase();
    let words: Vec<_> = query.split_whitespace().collect();
    let mut found: Vec<_> = presets
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            let name = p.name.to_lowercase();
            words.iter().all(|w| name.contains(w)).then_some(i)
        })
        .collect();
    found.sort_by_cached_key(|i| {
        presets
            .get(*i)
            .map(|p| p.name.to_lowercase())
            .unwrap_or_default()
    });
    found
}

fn paint_picture(ui: &egui::Ui, frame: egui::Rect, texture: Option<&egui::TextureHandle>) {
    crate::ui::before_after::checkerboard(ui.painter(), frame);
    if let Some(texture) = texture {
        let size = texture.size_vec2();
        let scale = (frame.width() / size.x).min(frame.height() / size.y);
        ui.painter().image(
            texture.id(),
            egui::Rect::from_center_size(frame.center(), size * scale),
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
            egui::Color32::WHITE,
        );
    }
}

fn preset_card(
    app: &mut App,
    ui: &mut egui::Ui,
    index: usize,
    selected: bool,
    list: bool,
    budget: &mut usize,
) -> egui::Response {
    let name = app
        .settings
        .sprayer_presets
        .get(index)
        .map(|p| p.name.clone())
        .unwrap_or_default();
    // One resolution in all views keeps the large preview and rows from
    // repeatedly replacing each other's cached textures.
    let cached = app
        .sprayer_preset_pictures
        .get(index)
        .and_then(|p| p.as_ref())
        .is_some_and(|p| p.pixels == 128);
    let picture = if cached {
        app.sprayer_picture(ui.ctx(), index, 128)
    } else if *budget > 0 {
        *budget -= 1;
        app.sprayer_picture(ui.ctx(), index, 128)
    } else {
        ui.ctx().request_repaint();
        None
    };
    let size = if list {
        egui::vec2(ui.available_width(), 48.)
    } else {
        egui::vec2(96., 88.)
    };
    let response = ui.add_sized(size, egui::Button::new("").selected(selected));
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &name));
    let rect = response.rect;
    let preview = if list {
        egui::Rect::from_min_size(rect.min + egui::vec2(4., 4.), egui::vec2(48., 40.))
    } else {
        egui::Rect::from_center_size(
            egui::pos2(rect.center().x, rect.top() + 33.),
            egui::vec2(84., 56.),
        )
    };
    paint_picture(ui, preview, picture.as_ref());
    let label = if list {
        egui::Rect::from_min_max(
            egui::pos2(preview.right() + 8., rect.top() + 4.),
            rect.max - egui::vec2(4., 4.),
        )
    } else {
        egui::Rect::from_min_max(
            egui::pos2(rect.left() + 4., rect.top() + 64.),
            rect.max - egui::vec2(4., 2.),
        )
    };
    // Paint the name instead of placing a selectable Label over the button:
    // text selection must not steal a row's click in list view.
    let mut job = egui::text::LayoutJob::simple_singleline(
        name.clone(),
        egui::FontId::proportional(12.),
        ui.visuals().text_color(),
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width(label.width());
    let galley = ui.painter().layout_job(job);
    let x = if list {
        label.left()
    } else {
        label.center().x - galley.size().x / 2.
    };
    ui.painter().galley(
        egui::pos2(x, label.center().y - galley.size().y / 2.),
        galley,
        ui.visuals().text_color(),
    );
    ui.ctx().data_mut(|d| d.insert_temp(tile_id(index), rect));
    response.on_hover_text(name)
}

fn browse_items(
    app: &mut App,
    ui: &mut egui::Ui,
    found: &[usize],
    height: f32,
) -> Option<(usize, bool)> {
    use crate::i18n::tr;
    if found.is_empty() {
        ui.label(tr("sprayer.no_matches"));
        return None;
    }
    let list = app.sprayer_library.list;
    let columns = if list {
        1
    } else {
        ((ui.available_width() + 8.) / 104.).floor().clamp(1., 8.) as usize
    };
    let row_height = if list { 48. } else { 88. };
    let mut choose = None;
    // Match the Shelf's bounded loading of newly visible thumbnails.
    let mut budget = 6;
    egui::ScrollArea::vertical()
        .id_salt("sprayer_library_items")
        .max_height(height)
        .auto_shrink([false, false])
        .show_rows(ui, row_height, found.len().div_ceil(columns), |ui, rows| {
            for row in rows {
                ui.horizontal(|ui| {
                    for position in row * columns..((row + 1) * columns).min(found.len()) {
                        let Some(index) = found.get(position).copied() else {
                            continue;
                        };
                        let selected = app.sprayer_library.selected == Some(index);
                        let response = preset_card(app, ui, index, selected, list, &mut budget);
                        if response.clicked() || response.double_clicked() {
                            choose = Some((index, response.double_clicked()));
                        }
                    }
                });
            }
        });
    choose
}

impl App {
    pub(crate) fn open_sprayer_library(&mut self) {
        self.sprayer_library.open = true;
        self.sprayer_library.search.clear();
        self.sprayer_library.focus_search = true;
        self.sprayer_library.selected = self
            .sprayer_preset_pick
            .filter(|i| *i < self.settings.sprayer_presets.len());
        self.sprayer_library.rename = self
            .sprayer_library
            .selected
            .and_then(|i| self.settings.sprayer_presets.get(i))
            .map(|p| p.name.clone())
            .unwrap_or_default();
    }

    fn use_sprayer_library_selection(&mut self) -> bool {
        let Some(index) = self.sprayer_library.selected else {
            return false;
        };
        let Some(preset) = self.settings.sprayer_presets.get(index) else {
            return false;
        };
        self.sprayer_preset_pick = Some(index);
        self.sprayer_preset_name = preset.name.clone();
        self.sprayer_library.open = false;
        true
    }

    fn rename_sprayer_preset(&mut self, index: usize, name: &str) -> Result<(), &'static str> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 80 {
            return Err("sprayer.invalid_name");
        }
        if self
            .settings
            .sprayer_presets
            .iter()
            .enumerate()
            .any(|(i, p)| i != index && p.name == name)
        {
            return Err("sprayer.duplicate_name");
        }
        let preset = self
            .settings
            .sprayer_presets
            .get_mut(index)
            .ok_or("sprayer.select_preset")?;
        preset.name = name.to_string();
        if self.sprayer_preset_pick == Some(index) {
            self.sprayer_preset_name = name.to_string();
        }
        self.settings.save();
        Ok(())
    }
}

/// A library using the application's resizable dialog chrome and modal
/// blocker. It chooses a preset; the path panel remains the Apply action.
pub(crate) fn library_window(app: &mut App, ctx: &egui::Context) {
    use crate::i18n::{tr, trf};
    if !app.sprayer_library.open || !matches!(app.dialog, crate::ui::dialogs::Dialog::None) {
        return;
    }
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        app.sprayer_library.open = false;
        return;
    }
    crate::ui::chrome::modal_blocker(ctx);
    let mut use_selected = false;
    let mut close = false;
    crate::ui::chrome::window(tr("sprayer.library_title"))
        .id_salt("sprayer_library")
        .resizable(egui::vec2(760., 560.), egui::vec2(560., 420.))
        .show(ctx, |ui| {
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::new("sprayer_library_body"), ui.max_rect()));
            ui.horizontal(|ui| {
                let search = ui
                    .add(
                        egui::TextEdit::singleline(&mut app.sprayer_library.search)
                            .hint_text(tr("sprayer.search"))
                            .desired_width((ui.available_width() - 180.).max(120.)),
                    )
                    .rich_tip(crate::ui::tips::keyed("sprayer.search"));
                if app.sprayer_library.focus_search {
                    search.request_focus();
                    app.sprayer_library.focus_search = false;
                }
                ui.selectable_value(
                    &mut app.sprayer_library.list,
                    false,
                    tr("sprayer.grid_view"),
                )
                .rich_tip(crate::ui::tips::keyed("sprayer.grid_view"));
                ui.selectable_value(&mut app.sprayer_library.list, true, tr("sprayer.list_view"))
                    .rich_tip(crate::ui::tips::keyed("sprayer.list_view"));
            });
            let found =
                filtered_presets(&app.settings.sprayer_presets, &app.sprayer_library.search);
            if app
                .sprayer_library
                .selected
                .is_some_and(|i| !found.contains(&i))
            {
                app.sprayer_library.selected = None;
                app.sprayer_library.rename.clear();
            }
            ui.label(trf(
                "sprayer.results",
                &[
                    ("shown", &found.len().to_string()),
                    ("total", &app.settings.sprayer_presets.len().to_string()),
                ],
            ));
            ui.separator();
            // Reserve the footer before laying out either scrolling pane.
            // A full-height ScrollArea must not push Use/Close below the window.
            let room = ui.available_rect_before_wrap();
            let content =
                egui::Rect::from_min_max(room.min, egui::pos2(room.right(), room.bottom() - 48.));
            let height = content.height().clamp(1., 900.);
            let mut panes = ui.new_child(egui::UiBuilder::new().max_rect(content));
            panes.set_clip_rect(content.intersect(ui.clip_rect()));
            panes.horizontal_top(|ui| {
                let width = (ui.available_width() - 220.).max(200.);
                ui.allocate_ui_with_layout(
                    egui::vec2(width, height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        if let Some((index, double)) = browse_items(app, ui, &found, height) {
                            app.sprayer_library.selected = Some(index);
                            app.sprayer_library.rename = app
                                .settings
                                .sprayer_presets
                                .get(index)
                                .map(|p| p.name.clone())
                                .unwrap_or_default();
                            use_selected |= double;
                        }
                    },
                );
                ui.separator();
                ui.vertical(|ui| {
                    ui.set_width(196.);
                    egui::ScrollArea::vertical()
                        .id_salt("sprayer_library_details")
                        .max_height(height)
                        .show(ui, |ui| {
                            if let Some(index) = app.sprayer_library.selected {
                                let picture = app.sprayer_picture(ui.ctx(), index, 128);
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(160., 160.),
                                    egui::Sense::hover(),
                                );
                                paint_picture(ui, rect, picture.as_ref());
                                if let Some(p) = app.settings.sprayer_presets.get(index) {
                                    ui.strong(&p.name);
                                    ui.label(trf(
                                        "sprayer.motif_count",
                                        &[("count", &p.motifs.len().to_string())],
                                    ));
                                    ui.label(tr(if p.settings.live {
                                        "sprayer.live_fit"
                                    } else {
                                        "sprayer.static_fit"
                                    }));
                                }
                                ui.add_space(8.);
                                ui.add(
                                    egui::TextEdit::singleline(&mut app.sprayer_library.rename)
                                        .char_limit(80)
                                        .desired_width(190.),
                                )
                                .rich_tip(crate::ui::tips::keyed("sprayer.name"));
                                ui.horizontal(|ui| {
                                    if ui
                                        .ibutton("sprayer.rename")
                                        .rich_tip(crate::ui::tips::keyed("sprayer.rename"))
                                        .clicked()
                                    {
                                        let name = app.sprayer_library.rename.clone();
                                        app.status = match app.rename_sprayer_preset(index, &name) {
                                            Ok(()) => tr("sprayer.saved"),
                                            Err(key) => tr(key),
                                        };
                                    }
                                    if ui
                                        .ibutton("sprayer.delete")
                                        .rich_tip(crate::ui::tips::keyed("sprayer.delete"))
                                        .clicked()
                                    {
                                        let active = app.sprayer_preset_pick;
                                        app.delete_sprayer_preset(index);
                                        app.sprayer_preset_pick = active.and_then(|i| {
                                            if i == index {
                                                None
                                            } else {
                                                Some(i - usize::from(i > index))
                                            }
                                        });
                                        if active == Some(index) {
                                            app.sprayer_preset_name.clear();
                                        }
                                        app.sprayer_library.selected = None;
                                        app.sprayer_library.rename.clear();
                                        app.status = tr("sprayer.deleted");
                                    }
                                });
                            } else {
                                ui.label(tr("sprayer.preview_hint"));
                            }
                        });
                });
            });
            let footer_rect =
                egui::Rect::from_min_max(egui::pos2(room.left(), room.bottom() - 40.), room.max);
            let mut footer = ui.new_child(egui::UiBuilder::new().max_rect(footer_rect));
            footer.separator();
            footer.horizontal(|ui| {
                let use_button = ui
                    .add_enabled(
                        app.sprayer_library.selected.is_some(),
                        Ib::new("sprayer.use_selected"),
                    )
                    .rich_tip_always(crate::ui::tips::keyed("sprayer.use_selected"));
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::new("sprayer_library_use"), use_button.rect)
                });
                use_selected |= use_button.clicked();
                close |= ui
                    .ibutton("sprayer.close_library")
                    .rich_tip(crate::ui::tips::keyed("sprayer.close_library"))
                    .clicked();
            });
        });
    if use_selected {
        app.use_sprayer_library_selection();
    }
    if close || crate::ui::chrome::take_close_request(ctx) {
        app.sprayer_library.open = false;
    }
}

/// Returns true when applying a preset changed the current drawing.
pub(crate) fn controls(app: &mut App, ui: &mut egui::Ui) -> bool {
    // Keep the existing placement fields reachable on ordinary window heights.
    // The library and saving controls are an optional extension of the panel.
    let section = egui::CollapsingHeader::new(crate::i18n::tr("sprayer.title"))
        .id_salt("sprayer_presets_section")
        .show(ui, |ui| preset_controls(app, ui));
    section
        .header_response
        .rich_tip(crate::ui::tips::pair("sprayer.title", "sprayer.hint"));
    section.body_returned.unwrap_or(false)
}

fn preset_controls(app: &mut App, ui: &mut egui::Ui) -> bool {
    use crate::i18n::tr;
    let pick = app
        .sprayer_preset_pick
        .filter(|i| *i < app.settings.sprayer_presets.len());
    ui.horizontal(|ui| {
        if let Some(index) = pick {
            let picture = app.sprayer_picture(ui.ctx(), index, 128);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(56., 44.), egui::Sense::hover());
            paint_picture(ui, rect, picture.as_ref());
            ui.label(
                app.settings
                    .sprayer_presets
                    .get(index)
                    .map(|p| p.name.as_str())
                    .unwrap_or_default(),
            );
        } else {
            ui.label(tr("sprayer.choose"));
        }
    });
    if ui
        .ibutton("sprayer.browse_library")
        .rich_tip(crate::ui::tips::keyed("sprayer.browse_library"))
        .clicked()
    {
        app.open_sprayer_library();
    }
    app.sprayer_preset_pick = pick;
    ui.add(
        egui::TextEdit::singleline(&mut app.sprayer_preset_name)
            .hint_text(tr("sprayer.name"))
            .char_limit(80)
            .desired_width(180.0),
    )
    .rich_tip(crate::ui::tips::keyed("sprayer.name"));
    let mut save = None;
    let mut delete = false;
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(
                !app.selection.is_empty() && !app.sprayer_preset_name.trim().is_empty(),
                Ib::new("sprayer.save"),
            )
            .rich_tip_always(crate::ui::tips::keyed("sprayer.save"))
            .clicked()
        {
            save = Some(None);
        }
        if ui
            .add_enabled(
                pick.is_some()
                    && !app.selection.is_empty()
                    && !app.sprayer_preset_name.trim().is_empty(),
                Ib::new("sprayer.replace"),
            )
            .rich_tip_always(crate::ui::tips::keyed("sprayer.replace"))
            .clicked()
        {
            save = Some(pick);
        }
        if ui
            .add_enabled(pick.is_some(), Ib::new("sprayer.delete"))
            .rich_tip_always(crate::ui::tips::keyed("sprayer.delete"))
            .clicked()
        {
            delete = true;
        }
    });
    if let Some(replace) = save {
        let name = app.sprayer_preset_name.clone();
        match app.save_sprayer_preset(&name, replace) {
            Ok(i) => {
                app.sprayer_preset_pick = Some(i);
                app.status = tr("sprayer.saved");
            }
            Err(key) => app.status = tr(key),
        }
    }
    if delete {
        if let Some(i) = pick {
            app.delete_sprayer_preset(i);
        }
        app.sprayer_preset_pick = None;
        app.sprayer_preset_name.clear();
        app.status = tr("sprayer.deleted");
    }
    ui.small(tr("sprayer.hint"));
    let mut applied = false;
    if ui
        .add_enabled(
            !delete && pick.is_some() && app.selection.len() == 1,
            Ib::new("sprayer.apply"),
        )
        .rich_tip_always(crate::ui::tips::keyed("sprayer.apply"))
        .clicked()
    {
        if let Some(i) = pick {
            match app.apply_sprayer_preset(i) {
                Ok(()) => {
                    app.status = tr("sprayer.applied");
                    applied = true;
                }
                Err(key) => app.status = tr(key),
            }
        }
    }
    ui.separator();
    applied
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fit_path::{FitCopies, FitRotation, FitSpacing};
    use tracedraw_core::{Color, Point, Rect};

    fn motif(app: &mut App, x: f64) -> ShapeId {
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(x, 10., x + 4., 12.),
                radius: 0.,
                corners: None,
            })
            .unwrap();
        app.run(Command::SetFill {
            shapes: vec![id],
            fill: tracedraw_core::Fill::Solid(Color::rgb8(20, 160, 80)),
        });
        id
    }
    fn guide(app: &mut App, y: f64) -> ShapeId {
        let mut path = tracedraw_core::BezPath::new();
        path.move_to((20., y));
        path.line_to((80., y));
        app.new_shape(ShapeKind::Path {
            path,
            closed: false,
        })
        .unwrap()
    }
    fn settings(app: &mut App) {
        app.fit_path = FitPathSettings {
            live: true,
            spacing: FitSpacing::Distance,
            distance: 10.,
            copies: FitCopies::Fill,
            rotation: FitRotation::Follow,
            ..Default::default()
        };
    }
    fn count(app: &App) -> usize {
        app.doc().all_layers().map(|l| l.shapes.len()).sum()
    }

    #[test]
    fn sprayer_library_restarts_and_applies_without_the_source_in_one_undo_step() {
        let mut app = App::headless();
        let source = motif(&mut app, 110.);
        let original = app.doc().find_shape(source).unwrap().clone();
        app.select(vec![source]);
        settings(&mut app);
        assert_eq!(app.save_sprayer_preset("Green marks", None), Ok(0));
        assert_eq!(app.doc().find_shape(source).unwrap(), &original);
        let preferences = serde_json::to_vec(&app.settings).unwrap();
        let mut next = App::headless();
        next.settings = serde_json::from_slice(&preferences).unwrap();
        let path = guide(&mut next, 20.);
        next.select(vec![path]);
        let before = next.doc().clone();
        next.apply_sprayer_preset(0).unwrap();
        assert_eq!(count(&next), 8); // guide and seven marks, with no unused template.
        let fit = next.live_fits().pop().unwrap().1;
        assert_eq!(fit.members.len(), 1);
        assert_eq!(fit.copies.len(), 6);
        let member = next.shapes_by_uid()[&fit.members[0].uid];
        assert!(
            (next.doc().find_shape(member).unwrap().bounds().center() - Point::new(20., 20.))
                .hypot()
                < 1e-6
        );
        let cdr = tracedraw_cdr::write::document_to_cdr_version(next.doc(), 27);
        let (mut reopened, _) = tracedraw_cdr::open_bytes(&cdr, "preset").unwrap();
        crate::fit_live::adopt_imported_strokes(&mut reopened);
        let mut reopened_app = App::headless();
        reopened_app.engine = tracedraw_core::Engine::new(reopened);
        assert_eq!(reopened_app.live_fits().len(), 1);
        next.undo();
        assert_eq!(next.doc().pages, before.pages);
        next.redo();
        assert_eq!(count(&next), 8);
        next.run(Command::TransformShapes {
            shapes: vec![path],
            transform: Affine::translate((0., 5.)),
        });
        next.refresh_live_fits();
        assert!((next.doc().find_shape(member).unwrap().bounds().center().y - 25.).abs() < 1e-6);
    }

    #[test]
    fn sprayer_saving_a_live_fit_uses_unplaced_motifs_and_keeps_order() {
        let mut app = App::headless();
        let a = motif(&mut app, 110.);
        let b = motif(&mut app, 130.);
        let path = guide(&mut app, 30.);
        app.select(vec![a, b, path]);
        app.fit_path_follow_selection();
        settings(&mut app);
        app.fit_path.angle = 45.;
        app.fit_path.keep_originals = true;
        app.fit_objects_to_path();
        let copy = app.selection[0];
        app.select(vec![copy]);
        app.save_sprayer_preset("Pair", None).unwrap();
        let preset = &app.settings.sprayer_presets[0];
        assert_eq!(preset.motifs.len(), 2);
        assert_eq!(preset.settings.angle, 45.);
        assert!(!preset.settings.keep_originals);
        for m in &preset.motifs {
            assert_eq!(m.uid, None);
            assert!(m.data.is_empty());
            assert!((m.bounds().width() - 4.).abs() < 1e-6);
        }
        assert!((preset.motifs[0].bounds().center().x + 10.).abs() < 1e-6);
        assert!((preset.motifs[1].bounds().center().x - 10.).abs() < 1e-6);
        // Spatial ordering must survive storage even when selection is reversed.
        let mut sorted = preset.clone();
        sorted.motifs.reverse();
        sorted.motifs[0].id = ShapeId(1);
        sorted.motifs[1].id = ShapeId(2);
        sorted.settings.order = crate::fit_path::FitOrder::LeftToRight;
        let mut shapes = sorted.motifs.clone();
        shapes.push(app.doc().find_shape(path).unwrap().clone());
        let plan = crate::fit_path::fit_to_path_plan(&shapes, &sorted.settings);
        assert_eq!(plan[0].source, ShapeId(2));
        let other = guide(&mut app, 60.);
        app.select(vec![other]);
        app.apply_sprayer_preset(0).unwrap();
        assert_eq!(app.live_fits().len(), 2);
        let fit = app
            .live_fits()
            .into_iter()
            .find(|(p, _)| *p == other)
            .unwrap()
            .1;
        assert_eq!(fit.members.len(), 2);
        assert!(fit.members.iter().all(|m| !app.live_fits()[0]
            .1
            .members
            .iter()
            .any(|old| old.uid == m.uid)));
    }

    #[test]
    fn sprayer_groups_have_fresh_child_identities_on_every_application() {
        let mut app = App::headless();
        let a = motif(&mut app, 10.);
        let b = motif(&mut app, 15.);
        app.run(Command::Group { shapes: vec![a, b] });
        let group = app.doc().ancestors(a)[0];
        app.select(vec![group]);
        settings(&mut app);
        app.save_sprayer_preset("Grouped", None).unwrap();
        for y in [40., 70.] {
            let path = guide(&mut app, y);
            app.select(vec![path]);
            app.apply_sprayer_preset(0).unwrap();
        }
        fn ids(s: &Shape, out: &mut Vec<(ShapeId, Option<tracedraw_core::document::Uid>)>) {
            out.push((s.id, s.uid));
            if let ShapeKind::Group { children } = &s.kind {
                for c in children {
                    ids(c, out);
                }
            }
        }
        let mut all = Vec::new();
        for s in app.doc().all_layers().flat_map(|l| &l.shapes) {
            ids(s, &mut all);
        }
        let unique_ids: std::collections::HashSet<_> = all.iter().map(|s| s.0).collect();
        let unique_uids: std::collections::HashSet<_> = all.iter().map(|s| s.1.unwrap()).collect();
        assert_eq!(all.len(), unique_ids.len());
        assert_eq!(all.len(), unique_uids.len());
        assert_eq!(app.live_fits().len(), 2);
    }

    #[test]
    fn sprayer_names_replacement_delete_and_invalid_sources_do_not_damage_artwork() {
        let mut app = App::headless();
        let source = motif(&mut app, 0.);
        app.select(vec![source]);
        settings(&mut app);
        let before = app.doc().clone();
        assert_eq!(
            app.save_sprayer_preset("  ", None),
            Err("sprayer.invalid_name")
        );
        app.save_sprayer_preset("Marks", None).unwrap();
        assert_eq!(
            app.save_sprayer_preset("Marks", None),
            Err("sprayer.duplicate_name")
        );
        app.fit_path.distance = 3.;
        app.save_sprayer_preset("Small gaps", Some(0)).unwrap();
        assert_eq!(app.settings.sprayer_presets.len(), 1);
        assert_eq!(app.settings.sprayer_presets[0].settings.distance, 3.);
        assert_eq!(app.doc().pages, before.pages);
        app.delete_sprayer_preset(0);
        assert!(app.settings.sprayer_presets.is_empty());
        app.run(Command::SetFill {
            shapes: vec![source],
            fill: tracedraw_core::Fill::Solid(Color::Spot {
                ink: 1,
                tint: 1.,
                c: 0.,
                m: 1.,
                y: 0.,
                k: 0.,
            }),
        });
        assert_eq!(
            app.save_sprayer_preset("Spot", None),
            Err("sprayer.unsupported")
        );
    }

    #[test]
    fn sprayer_optional_library_skips_invalid_entries_without_resetting_preferences() {
        let mut app = App::headless();
        let source = motif(&mut app, 0.);
        app.select(vec![source]);
        settings(&mut app);
        app.save_sprayer_preset("Good", None).unwrap();
        let good = serde_json::to_value(&app.settings.sprayer_presets[0]).unwrap();
        let mut future = good.clone();
        future["version"] = 99.into();
        let mut oversized = good.clone();
        oversized["name"] = "x".repeat(MAX_BYTES + 1).into();
        let mut value = serde_json::to_value(&app.settings).unwrap();
        value["nudge_mm"] = 1.75.into();
        value["sprayer_presets"] =
            serde_json::json!([null,{"broken":true},future,oversized,good,good]);
        let restored: crate::settings::Settings = serde_json::from_value(value).unwrap();
        assert_eq!(restored.nudge_mm, 1.75);
        assert_eq!(restored.sprayer_presets.len(), 1);
        let mut too_complex = restored.sprayer_presets[0].clone();
        too_complex.motifs = vec![too_complex.motifs[0].clone(); 33];
        assert!(too_complex.encoded().is_none());
        let mut bad_number = restored.sprayer_presets[0].clone();
        bad_number.settings.distance = f64::NAN;
        assert!(bad_number.encoded().is_none());
    }

    #[test]
    fn sprayer_invalid_targets_are_rejected_without_inserting_templates() {
        let mut app = App::headless();
        let source = motif(&mut app, 0.);
        app.select(vec![source]);
        settings(&mut app);
        app.save_sprayer_preset("Test", None).unwrap();
        let before = count(&app);
        assert_eq!(app.apply_sprayer_preset(0), Err("sprayer.select_path"));
        assert_eq!(count(&app), before);
        let empty = app
            .new_shape(ShapeKind::Path {
                path: tracedraw_core::BezPath::new(),
                closed: false,
            })
            .unwrap();
        app.select(vec![empty]);
        let before = count(&app);
        assert_eq!(app.apply_sprayer_preset(0), Err("sprayer.cannot_fit"));
        assert_eq!(count(&app), before);
        let path = guide(&mut app, 40.);
        app.select(vec![path]);
        app.apply_sprayer_preset(0).unwrap();
        let before = count(&app);
        assert_eq!(app.apply_sprayer_preset(0), Err("sprayer.select_path"));
        assert_eq!(count(&app), before);
        let other = motif(&mut app, 120.);
        app.select(vec![path, other]);
        assert_eq!(
            app.save_sprayer_preset("Mixed selection", None),
            Err("sprayer.select_source")
        );
    }

    #[test]
    fn sprayer_thumbnails_show_motif_colour_and_cache_until_the_preset_changes() {
        let mut app = App::headless();
        let source = motif(&mut app, 10.);
        app.select(vec![source]);
        settings(&mut app);
        app.save_sprayer_preset("Green", None).unwrap();
        let (green, w, h) = thumbnail(&app.settings.sprayer_presets[0], 64).unwrap();
        assert!(w <= 64 && h <= 64);
        assert!(green
            .chunks_exact(4)
            .any(|p| u16::from(p[1]) > u16::from(p[0]) + 20 && p[3] > 200));
        let ctx = crate::theme::ui_context();
        let first = app.sprayer_picture(&ctx, 0, 64).unwrap();
        let path = guide(&mut app, 40.);
        app.select(vec![path]);
        assert_eq!(
            app.sprayer_picture(&ctx, 0, 64).unwrap().id(),
            first.id(),
            "drawing changes do not regenerate library previews"
        );
        app.select(vec![source]);
        app.run(Command::SetFill {
            shapes: vec![source],
            fill: tracedraw_core::Fill::Solid(Color::rgb8(220, 20, 20)),
        });
        app.save_sprayer_preset("Red", Some(0)).unwrap();
        assert!(app.sprayer_preset_pictures.is_empty());
        let (red, _, _) = thumbnail(&app.settings.sprayer_presets[0], 64).unwrap();
        assert_ne!(red, green);
        let replaced = app.sprayer_picture(&ctx, 0, 64).unwrap();
        assert_ne!(replaced.id(), first.id());
        let large = app.sprayer_picture(&ctx, 0, 128).unwrap();
        assert_ne!(large.id(), replaced.id());
        assert_eq!(app.sprayer_preset_pictures.len(), 1);
        app.apply_settings(app.settings.clone());
        assert!(app.sprayer_preset_pictures.is_empty());
        let _ = app.sprayer_picture(&ctx, 0, 64).unwrap();
        app.delete_sprayer_preset(0);
        assert!(app.sprayer_preset_pictures.is_empty());
        assert!(app.sprayer_picture(&ctx, 0, 64).is_none());
    }

    #[test]
    fn sprayer_library_searches_sorts_and_manages_presets_without_drawing_changes() {
        let mut app = App::headless();
        let source = motif(&mut app, 10.);
        app.select(vec![source]);
        settings(&mut app);
        for name in [
            "Gold stars",
            "Blue rings",
            "Red triangles",
            "Зелені позначки",
        ] {
            app.save_sprayer_preset(name, None).unwrap();
        }
        assert_eq!(
            filtered_presets(&app.settings.sprayer_presets, " RED tri "),
            vec![2]
        );
        assert_eq!(
            filtered_presets(&app.settings.sprayer_presets, "ЗЕЛЕНІ"),
            vec![3]
        );
        assert!(filtered_presets(&app.settings.sprayer_presets, "missing").is_empty());
        assert_eq!(
            filtered_presets(&app.settings.sprayer_presets, ""),
            vec![1, 0, 2, 3]
        );
        app.sprayer_preset_pick = Some(1);
        let before = app.engine.revision();
        app.open_sprayer_library();
        assert_eq!(app.sprayer_library.selected, Some(1));
        app.sprayer_library.selected = Some(2);
        app.sprayer_library.open = false;
        assert_eq!(app.sprayer_preset_pick, Some(1), "closing is not choosing");
        app.open_sprayer_library();
        app.sprayer_library.selected = Some(2);
        assert!(app.use_sprayer_library_selection());
        assert_eq!(app.sprayer_preset_pick, Some(2));
        assert!(!app.sprayer_library.open);
        app.rename_sprayer_preset(2, "Warm triangles").unwrap();
        assert_eq!(app.sprayer_preset_name, "Warm triangles");
        assert_eq!(
            app.rename_sprayer_preset(2, "Blue rings"),
            Err("sprayer.duplicate_name")
        );
        assert_eq!(app.engine.revision(), before);
    }

    #[test]
    fn sprayer_library_selects_from_virtualized_list_without_editing_the_drawing() {
        let mut app = App::headless();
        let source = motif(&mut app, 10.);
        app.select(vec![source]);
        settings(&mut app);
        app.save_sprayer_preset("Marks", None).unwrap();
        let entry = app.settings.sprayer_presets[0].clone();
        for i in 1..100 {
            let mut p = entry.clone();
            p.name = format!("Marks {i:03}");
            app.settings.sprayer_presets.push(p);
        }
        let before = app.engine.revision();
        let ctx = crate::theme::ui_context();
        let found = filtered_presets(&app.settings.sprayer_presets, "");
        app.open_sprayer_library();
        app.sprayer_library.list = true;
        let frame = |app: &mut App, events: Vec<egui::Event>| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400., 500.),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    if let Some((i, _)) = browse_items(app, ui, &found, 180.) {
                        app.sprayer_library.selected = Some(i);
                    }
                },
            );
            output.textures_delta.clear();
        };
        frame(&mut app, vec![]);
        let cached = app
            .sprayer_preset_pictures
            .iter()
            .filter(|p| p.is_some())
            .count();
        assert!((1..20).contains(&cached));
        let at = ctx
            .data(|d| d.get_temp::<egui::Rect>(tile_id(1)))
            .unwrap()
            .center();
        frame(
            &mut app,
            vec![
                egui::Event::PointerMoved(at),
                egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        );
        frame(
            &mut app,
            vec![egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        );
        assert_eq!(app.sprayer_library.selected, Some(1));
        assert_eq!(app.sprayer_preset_pick, None);
        assert!(app.use_sprayer_library_selection());
        assert_eq!(app.sprayer_preset_pick, Some(1));
        assert_eq!(app.engine.revision(), before);
        // Library focus must not let Delete remove the selected guide/object.
        app.open_sprayer_library();
        let mut output = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Delete,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                }],
                ..Default::default()
            },
            |ui| {
                app.keyboard(ui.ctx());
            },
        );
        output.textures_delta.clear();
        assert!(app.doc().find_shape(source).is_some());
        assert_eq!(app.engine.revision(), before);
    }

    #[test]
    fn sprayer_library_footer_stays_inside_the_resized_dialog() {
        let mut app = App::headless();
        let source = motif(&mut app, 10.);
        app.select(vec![source]);
        settings(&mut app);
        app.save_sprayer_preset("Marks", None).unwrap();
        app.open_sprayer_library();
        let ctx = crate::theme::ui_context();
        for size in [egui::vec2(800., 600.), egui::vec2(620., 460.)] {
            for _ in 0..3 {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ui| library_window(&mut app, ui.ctx()),
                );
                output.textures_delta.clear();
            }
            let body = ctx
                .data(|d| d.get_temp::<egui::Rect>(egui::Id::new("sprayer_library_body")))
                .unwrap();
            let footer = ctx
                .data(|d| d.get_temp::<egui::Rect>(egui::Id::new("sprayer_library_use")))
                .unwrap();
            assert!(
                body.contains(footer.min) && body.contains(footer.max),
                "footer {footer:?} outside {body:?}"
            );
        }
    }

    /// Generate only original local demo artwork and settings for manual checks.
    #[test]
    #[ignore = "writes an optional local demo; set TRACEDRAW_SPRAYER_DEMO"]
    fn sprayer_write_local_demo() {
        let root = std::path::PathBuf::from(
            std::env::var("TRACEDRAW_SPRAYER_DEMO").expect("demo directory"),
        );
        std::fs::create_dir_all(&root).unwrap();
        let mut app = App::headless();
        let a = motif(&mut app, 30.);
        let b = motif(&mut app, 35.);
        app.run(Command::Group { shapes: vec![a, b] });
        let group = app.doc().ancestors(a)[0];
        app.select(vec![group]);
        settings(&mut app);
        app.save_sprayer_preset("Green pairs", None).unwrap();
        let path = guide(&mut app, 55.);
        app.select(vec![path]);
        app.apply_sprayer_preset(0).unwrap();
        let mut curve = tracedraw_core::BezPath::new();
        curve.move_to((20., 100.));
        curve.curve_to((40., 135.), (65., 65.), (100., 100.));
        app.new_shape(ShapeKind::Path {
            path: curve,
            closed: false,
        })
        .unwrap();
        tracedraw_io::save_native(app.doc(), root.join("sprayer-demo.tdraw")).unwrap();
        std::fs::write(
            root.join("demo-settings.json"),
            serde_json::to_vec_pretty(&app.settings).unwrap(),
        )
        .unwrap();
    }
}
