//! The settings of the image editing mode (Bitmaps > Edit Inside, a
//! double click on a picture): the tool in use's options, shown in the
//! property bar, and the bodies of the image panels (layers, selection,
//! retouching, distortion, effects, the picture and its histogram). See
//! `pixel_mode.rs` and docs/behavior/edit-inside-picture.md.

use crate::app::App;
use crate::i18n::tr;
use crate::pixel_edit::{self, PixelTool};
use crate::theme::Tokens;
use crate::ui::tips::RichTip;
use egui::{Color32, RichText, Stroke};

/// Every frame while a picture is edited: the edit ends when the
/// picture went away, and the lights are kept only for the Light tool.
pub fn show(app: &mut App) {
    if app.pixel_edit.is_none() {
        return;
    }
    // The picture went away under the edit (undone, deleted elsewhere).
    let gone = app
        .pixel_edit
        .as_ref()
        .is_some_and(|pe| app.doc().find_shape(pe.shape).is_none());
    if gone {
        app.cancel_pixel_edit();
        return;
    }
    // The lights stay live only while the Light tool is in use.
    if let Some(pe) = app.pixel_edit.as_mut() {
        if pe.tool != PixelTool::Light {
            pe.lights.bake();
        }
    }
}

fn percent(ui: &mut egui::Ui, key: &str, v: &mut f32) {
    let mut p = (*v * 100.0).round();
    if ui
        .add(
            egui::Slider::new(&mut p, 0.0..=100.0)
                .suffix(" %")
                .text(tr(key)),
        )
        .changed()
    {
        *v = p / 100.0;
    }
}

pub(crate) fn tool_options(ui: &mut egui::Ui, pe: &mut crate::pixel_edit_app::PixelEdit) {
    let t = pe.tool;
    if t == PixelTool::Light {
        super::pixel_lights::options(ui, pe);
        return;
    }
    let o = &mut pe.opts;
    if t == PixelTool::Liquify {
        egui::ComboBox::from_id_salt("pixel_liquify_mode")
            .selected_text(tr(o.liquify.key()))
            .show_ui(ui, |ui| {
                for m in crate::pixel_liquify::LiquifyMode::ALL {
                    ui.selectable_value(&mut o.liquify, m, tr(m.key()));
                }
            });
        ui.add(
            egui::Slider::new(&mut o.size, 1.0..=1000.0)
                .logarithmic(true)
                .suffix(" px")
                .text(tr("pixel.opt.size")),
        );
        percent(ui, "pixel.opt.hardness", &mut o.hardness);
        percent(ui, "pixel.opt.strength", &mut o.flow);
    }
    if t == PixelTool::Remove {
        ui.add(
            egui::Slider::new(&mut o.size, 1.0..=1000.0)
                .logarithmic(true)
                .suffix(" px")
                .text(tr("pixel.opt.size")),
        );
        ui.add(
            egui::Slider::new(&mut o.remove_grow, 0.0..=40.0)
                .suffix(" px")
                .text(tr("pixel.opt.remove_grow")),
        )
        .rich_tip(tr("pixel.opt.remove_grow_tip"));
    }
    if matches!(t, PixelTool::Brush | PixelTool::Clone) {
        // Following the surface of the photo, in a menu.
        ui.menu_button(tr("pixel.opt.surface"), |ui| {
            ui.checkbox(&mut o.surface, tr("pixel.opt.surface_on"))
                .rich_tip(tr("pixel.opt.surface_tip"));
            ui.add_enabled(
                o.surface,
                egui::Slider::new(&mut o.surface_depth, 0.0..=40.0)
                    .suffix(" px")
                    .text(tr("pixel.opt.surface_depth")),
            );
            let mut l = (o.surface_light * 100.0).round();
            if ui
                .add_enabled(
                    o.surface,
                    egui::Slider::new(&mut l, 0.0..=100.0)
                        .suffix(" %")
                        .text(tr("pixel.opt.surface_light")),
                )
                .changed()
            {
                o.surface_light = l / 100.0;
            }
        });
    }
    if t.is_brush() {
        ui.add(
            egui::Slider::new(&mut o.size, 1.0..=1000.0)
                .logarithmic(true)
                .suffix(" px")
                .text(tr("pixel.opt.size")),
        );
        percent(ui, "pixel.opt.hardness", &mut o.hardness);
        percent(ui, "pixel.opt.opacity", &mut o.opacity);
        if t != PixelTool::Smudge {
            percent(ui, "pixel.opt.flow", &mut o.flow);
        }
        // The tip's spacing, scatter, angle and roundness, in a menu.
        ui.menu_button(tr("pixel.opt.tip"), |ui| {
            let mut sp = (o.spacing * 100.0).round();
            if ui
                .add(
                    egui::Slider::new(&mut sp, 1.0..=300.0)
                        .suffix(" %")
                        .text(tr("pixel.opt.spacing")),
                )
                .changed()
            {
                o.spacing = sp / 100.0;
            }
            let mut sc = (o.scatter * 100.0).round();
            if ui
                .add(
                    egui::Slider::new(&mut sc, 0.0..=300.0)
                        .suffix(" %")
                        .text(tr("pixel.opt.scatter")),
                )
                .changed()
            {
                o.scatter = sc / 100.0;
            }
            ui.add(
                egui::Slider::new(&mut o.angle, 0.0..=180.0)
                    .suffix("\u{b0}")
                    .text(tr("pixel.opt.angle")),
            );
            let mut r = (o.roundness * 100.0).round();
            if ui
                .add(
                    egui::Slider::new(&mut r, 5.0..=100.0)
                        .suffix(" %")
                        .text(tr("pixel.opt.roundness")),
                )
                .changed()
            {
                o.roundness = r / 100.0;
            }
        });
    }
    if matches!(
        t,
        PixelTool::Brush
            | PixelTool::Fill
            | PixelTool::Gradient
            | PixelTool::Eyedropper
            | PixelTool::Tint
            | PixelTool::ReplaceColor
            | PixelTool::Shape
    ) {
        let mut c = Color32::from_rgba_unmultiplied(o.color[0], o.color[1], o.color[2], o.color[3]);
        ui.label(tr("pixel.opt.color"));
        if egui::color_picker::color_edit_button_srgba(
            ui,
            &mut c,
            egui::color_picker::Alpha::OnlyBlend,
        )
        .changed()
        {
            let [r, g, b, a] = c.to_srgba_unmultiplied();
            o.color = [r, g, b, a];
        }
    }
    if matches!(t, PixelTool::Fill | PixelTool::Gradient | PixelTool::Shape) {
        percent(ui, "pixel.opt.opacity", &mut o.opacity);
    }
    if matches!(t, PixelTool::MagicWand | PixelTool::Fill) {
        ui.add(egui::Slider::new(&mut o.tolerance, 0..=255).text(tr("pixel.opt.tolerance")));
        ui.checkbox(&mut o.contiguous, tr("pixel.opt.contiguous"));
    }
    if t == PixelTool::ReplaceColor {
        ui.add(egui::Slider::new(&mut o.tolerance, 0..=255).text(tr("pixel.opt.tolerance")));
    }
    if t == PixelTool::Shape {
        shape_options(ui, o);
    }
    match t {
        PixelTool::Sponge => {
            ui.checkbox(&mut o.desaturate, tr("pixel.opt.desaturate"));
        }
        PixelTool::Gradient => {
            egui::ComboBox::from_id_salt("pixel_gradient_kind")
                .selected_text(tr(o.gradient.key()))
                .show_ui(ui, |ui| {
                    for k in crate::pixel_paint::GradientKind::ALL {
                        ui.selectable_value(&mut o.gradient, k, tr(k.key()));
                    }
                });
            ui.checkbox(&mut o.gradient_to_clear, tr("pixel.opt.to_clear"));
        }
        _ => {}
    }
    // What to do with the tool.
    ui.label(
        RichText::new(tr(&format!("{}_hint", t.key())))
            .color(Tokens::text_dim())
            .small(),
    );
}

/// The Shape tool's settings: the shape, fill, outline, its colour and
/// width, and the polygon's sides.
fn shape_options(ui: &mut egui::Ui, o: &mut pixel_edit::BrushOpts) {
    use crate::pixel_paint::PixelShape;
    egui::ComboBox::from_id_salt("pixel_shape_kind")
        .selected_text(tr(o.shape.key()))
        .show_ui(ui, |ui| {
            for k in PixelShape::ALL {
                ui.selectable_value(&mut o.shape, k, tr(k.key()));
            }
        });
    if o.shape == PixelShape::Polygon {
        ui.add(egui::Slider::new(&mut o.sides, 3..=24).text(tr("pixel.opt.sides")));
    }
    if o.shape != PixelShape::Line {
        ui.checkbox(&mut o.shape_fill, tr("pixel.opt.fill"));
        ui.checkbox(&mut o.shape_outline, tr("pixel.opt.outline"));
    }
    if o.shape == PixelShape::Line || o.shape_outline {
        ui.add(
            egui::Slider::new(&mut o.line_width, 0.5..=200.0)
                .logarithmic(true)
                .suffix(" px")
                .text(tr("pixel.opt.width")),
        );
    }
    if o.shape != PixelShape::Line && o.shape_outline {
        let c = o.outline_color;
        let mut c = Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]);
        ui.label(tr("pixel.opt.outline_color"));
        if egui::color_picker::color_edit_button_srgba(
            ui,
            &mut c,
            egui::color_picker::Alpha::OnlyBlend,
        )
        .changed()
        {
            o.outline_color = c.to_srgba_unmultiplied();
        }
    }
}

/// The name a new layer gets: "Layer" and the first number not in use.
pub(crate) fn new_layer_name(pe: &crate::pixel_edit_app::PixelEdit) -> String {
    let base = tr("pixel.layer.name");
    let mut n = pe.pi.layers.len();
    loop {
        let name = format!("{base} {n}");
        if !pe.pi.layers.iter().any(|l| l.name == name) {
            return name;
        }
        n += 1;
    }
}

/// How a layer is named in the list: its name, or Background for the
/// first one when it has none.
pub(crate) fn layer_label(i: usize, name: &str) -> String {
    if name.is_empty() {
        if i == 0 {
            tr("pixel.layer.background")
        } else {
            format!("{} {i}", tr("pixel.layer.name"))
        }
    } else {
        name.to_string()
    }
}

pub(crate) fn layers_section(ui: &mut egui::Ui, pe: &mut crate::pixel_edit_app::PixelEdit) {
    // Top layer first, as they stack.
    let n = pe.pi.layers.len();
    let mut pick: Option<usize> = None;
    let mut toggled = false;
    for i in (0..n).rev() {
        let Some(l) = pe.pi.layers.get(i) else {
            continue;
        };
        let (mut vis, mut lock) = (l.visible, l.locked);
        let label = layer_label(i, &l.name);
        ui.horizontal(|ui| {
            if ui
                .checkbox(&mut vis, "")
                .rich_tip(tr("pixel.layer.visible_tip"))
                .changed()
            {
                toggled = true;
            }
            if ui
                .checkbox(&mut lock, "")
                .rich_tip(tr("pixel.layer.locked_tip"))
                .changed()
            {
                toggled = true;
            }
            if ui.selectable_label(pe.pi.active == i, label).clicked() {
                pick = Some(i);
            }
        });
        if (vis, lock) != (l.visible, l.locked) {
            pe.apply_all(true, |pi| {
                let l = pi.layers.get_mut(i)?;
                l.visible = vis;
                l.locked = lock;
                Some([0, 0, pi.img.width(), pi.img.height()])
            });
        }
    }
    if let Some(i) = pick {
        if !toggled {
            pe.apply_all(true, |pi| {
                pi.set_active(i);
                Some([0, 0, 0, 0])
            });
        }
    }
    // The settings of the layer in use. A drag of the opacity
    // slider is one undo step.
    let i = pe.pi.active;
    if let Some(l) = pe.pi.layers.get(i).cloned() {
        let mut name = l.name.clone();
        let mut op = (l.opacity * 100.0).round();
        let mut merge = l.merge;
        ui.horizontal(|ui| {
            ui.label(tr("pixel.layer.rename"));
            ui.text_edit_singleline(&mut name);
        });
        let slider = ui.add(
            egui::Slider::new(&mut op, 0.0..=100.0)
                .suffix(" %")
                .text(tr("pixel.opt.opacity")),
        );
        egui::ComboBox::from_id_salt("pixel_layer_merge")
            .width(160.0)
            .selected_text(merge.name())
            .show_ui(ui, |ui| {
                for m in pixel_edit::MergeMode::ALL {
                    ui.selectable_value(&mut merge, m, m.name());
                }
            });
        let changed =
            name != l.name || (op / 100.0 - l.opacity).abs() > 1e-4 || merge != l.merge;
        if changed {
            if !pe.layer_drag {
                pe.remember();
            }
            if let Some(m) = pe.pi.layers.get_mut(i) {
                m.name = name;
                m.opacity = op / 100.0;
                m.merge = merge;
            }
            pe.view.borrow_mut().full = true;
        }
        pe.layer_drag = slider.dragged();
        layer_mask_row(ui, pe, i, l.clip, l.mask.is_some());
    }
    ui.horizontal_wrapped(|ui| {
        let name = new_layer_name(pe);
        if ui.button(tr("pixel.layer.new")).clicked() {
            pe.apply_all(true, |pi| {
                pi.add_layer(name.clone());
                Some([0, 0, pi.img.width(), pi.img.height()])
            });
        }
        if ui.button(tr("pixel.layer.duplicate")).clicked() {
            pe.apply_all(true, |pi| {
                pi.duplicate_layer(name.clone());
                Some([0, 0, pi.img.width(), pi.img.height()])
            });
        }
        if ui
            .add_enabled(n > 1, egui::Button::new(tr("pixel.layer.delete")))
            .clicked()
        {
            pe.apply_all(true, |pi| {
                pi.delete_layer()
                    .then(|| [0, 0, pi.img.width(), pi.img.height()])
            });
        }
        if ui
            .add_enabled(i + 1 < n, egui::Button::new(tr("pixel.layer.up")))
            .clicked()
        {
            pe.apply_all(true, |pi| {
                pi.move_layer(true)
                    .then(|| [0, 0, pi.img.width(), pi.img.height()])
            });
        }
        if ui
            .add_enabled(i > 0, egui::Button::new(tr("pixel.layer.down")))
            .clicked()
        {
            pe.apply_all(true, |pi| {
                pi.move_layer(false)
                    .then(|| [0, 0, pi.img.width(), pi.img.height()])
            });
        }
        if ui
            .add_enabled(i > 0, egui::Button::new(tr("pixel.layer.merge_down")))
            .clicked()
        {
            pe.apply_all(true, |pi| {
                pi.merge_down()
                    .then(|| [0, 0, pi.img.width(), pi.img.height()])
            });
        }
        if ui
            .add_enabled(
                pe.pi.is_layered(),
                egui::Button::new(tr("pixel.layer.flatten")),
            )
            .clicked()
        {
            pe.apply_all(true, |pi| {
                pi.flatten()
                    .then(|| [0, 0, pi.img.width(), pi.img.height()])
            });
        }
    });
    ui.horizontal_wrapped(|ui| {
        let name = new_layer_name(pe);
        if ui
            .button(tr("pixel.layer.copy_to"))
            .rich_tip(tr("pixel.layer.copy_to_tip"))
            .clicked()
        {
            pe.apply_all(true, |pi| {
                pi.selection_to_layer(name.clone(), false)
                    .then(|| [0, 0, pi.img.width(), pi.img.height()])
            });
        }
        if ui
            .button(tr("pixel.layer.cut_to"))
            .rich_tip(tr("pixel.layer.cut_to_tip"))
            .clicked()
        {
            pe.apply_all(true, |pi| {
                pi.selection_to_layer(name.clone(), true)
                    .then(|| [0, 0, pi.img.width(), pi.img.height()])
            });
        }
        if ui
            .button(tr("pixel.layer.paste"))
            .rich_tip(tr("pixel.layer.paste_tip"))
            .clicked()
        {
            if let Some(img) = crate::clipboard::read_system().image {
                pe.apply_all(true, |pi| {
                    pixel_edit::paste_as_layer(pi, name.clone(), &img)
                });
            }
        }
        // The effect chosen below as a layer that keeps it
        // changeable.
        let effect = pe.effect.clone();
        if ui
            .add_enabled(
                effect.is_some(),
                egui::Button::new(tr("pixel.layer.adjustment")),
            )
            .rich_tip(tr("pixel.layer.adjustment_tip"))
            .clicked()
        {
            if let Some(e) = effect {
                let label = tr(&format!("fx.{}", e.id));
                pe.apply_all(true, |pi| {
                    pi.add_adjustment_layer(label, e);
                    Some([0, 0, pi.img.width(), pi.img.height()])
                });
                pe.effect = None;
                pe.preview = None;
            }
        }
        // The layer laid onto the photo beneath it, with the
        // depth and light of the brush's surface options.
        if ui
            .add_enabled(
                pe.pi.active > 0 && !pe.pi.active_locked(),
                egui::Button::new(tr("pixel.layer.fit_surface")),
            )
            .rich_tip(tr("pixel.layer.fit_surface_tip"))
            .clicked()
        {
            let map = crate::pixel_surface::SurfaceMap::new(
                &crate::pixel_surface::seen_beneath(&pe.pi),
                pe.depth.as_ref(),
            );
            let (d, l) = (pe.opts.surface_depth, pe.opts.surface_light);
            pe.apply(true, |pi| {
                crate::pixel_surface::fit_to_surface(pi, &map, d, l)
            });
        }
    });
}

pub(crate) fn selection_section(ui: &mut egui::Ui, pe: &mut crate::pixel_edit_app::PixelEdit) {
    let (w, h) = pe.pi.img.dimensions();
    ui.horizontal_wrapped(|ui| {
        if ui.button(tr("pixel.sel.all")).clicked() {
            pe.set_selection(Some(image::GrayImage::from_pixel(w, h, image::Luma([255]))));
        }
        if ui
            .add_enabled(pe.pi.sel.is_some(), egui::Button::new(tr("pixel.sel.none")))
            .clicked()
        {
            pe.set_selection(None);
        }
        if ui.button(tr("pixel.sel.invert")).clicked() {
            let inv = pixel_edit::invert(pe.pi.sel.as_ref(), w, h);
            pe.set_selection(Some(inv));
        }
    });
    let has = pe.pi.sel.is_some();
    ui.horizontal(|ui| {
        ui.add(
            egui::DragValue::new(&mut pe.feather_px)
                .range(1..=200)
                .suffix(" px"),
        );
        if ui
            .add_enabled(has, egui::Button::new(tr("pixel.sel.feather")))
            .clicked()
        {
            if let Some(m) = pe
                .pi
                .sel
                .as_ref()
                .map(|m| pixel_edit::feather(m, pe.feather_px))
            {
                pe.set_selection(Some(m));
            }
        }
    });
    ui.horizontal(|ui| {
        ui.add(
            egui::DragValue::new(&mut pe.grow_px)
                .range(-200..=200)
                .suffix(" px"),
        );
        if ui
            .add_enabled(has, egui::Button::new(tr("pixel.sel.grow")))
            .clicked()
        {
            if let Some(m) = pe.pi.sel.as_ref().map(|m| pixel_edit::grow(m, pe.grow_px)) {
                pe.set_selection(Some(m));
            }
        }
    });
    ui.horizontal_wrapped(|ui| {
        if ui
            .button(tr("pixel.sel.delete"))
            .rich_tip(tr("pixel.sel.delete_tip"))
            .clicked()
        {
            pe.apply(false, pixel_edit::clear_selection);
        }
        if ui.button(tr("pixel.sel.fill")).clicked() {
            let (c, o) = (pe.opts.color, pe.opts.opacity);
            pe.apply(false, |pi| pixel_edit::fill_selection(pi, c, o));
        }
        if ui
            .add_enabled(has, egui::Button::new(tr("pixel.sel.crop")))
            .clicked()
        {
            pe.apply_all(true, pixel_edit::crop_to_selection);
        }
    });
    more_selection(ui, pe);
}

/// Clip to the layer below, and the mask of the layer in use.
fn layer_mask_row(
    ui: &mut egui::Ui,
    pe: &mut crate::pixel_edit_app::PixelEdit,
    i: usize,
    clip: bool,
    has_mask: bool,
) {
    use crate::pixel_edit_app::MaskTarget;
    let mut c = clip;
    if i > 0
        && ui
            .checkbox(&mut c, tr("pixel.layer.clip"))
            .rich_tip(tr("pixel.layer.clip_tip"))
            .changed()
    {
        pe.apply_all(true, |pi| {
            pi.layers.get_mut(i)?.clip = c;
            Some([0, 0, pi.img.width(), pi.img.height()])
        });
    }
    ui.horizontal_wrapped(|ui| {
        if !has_mask {
            if ui
                .button(tr("pixel.mask.add"))
                .rich_tip(tr("pixel.mask.add_tip"))
                .clicked()
                && pe.apply_all(true, |pi| {
                    pi.add_layer_mask()
                        .then(|| [0, 0, pi.img.width(), pi.img.height()])
                })
            {
                pe.set_edit_layer_mask(true);
            }
            return;
        }
        let mut editing = pe.target == MaskTarget::LayerMask;
        if ui
            .toggle_value(&mut editing, tr("pixel.mask.edit"))
            .rich_tip(tr("pixel.mask.edit_tip"))
            .changed()
        {
            pe.set_edit_layer_mask(editing);
        }
        if ui.button(tr("pixel.mask.invert")).clicked() {
            pe.apply_all(true, |pi| {
                pi.invert_layer_mask()
                    .then(|| [0, 0, pi.img.width(), pi.img.height()])
            });
        }
        if ui
            .button(tr("pixel.mask.apply"))
            .rich_tip(tr("pixel.mask.apply_tip"))
            .clicked()
        {
            pe.apply(true, |pi| {
                pi.apply_layer_mask()
                    .then(|| [0, 0, pi.img.width(), pi.img.height()])
            });
            pe.set_edit_layer_mask(false);
        }
        if ui.button(tr("pixel.mask.delete")).clicked() {
            pe.apply_all(true, |pi| {
                pi.delete_layer_mask()
                    .then(|| [0, 0, pi.img.width(), pi.img.height()])
            });
            pe.set_edit_layer_mask(false);
        }
    });
}

/// Quick mask, selections by tone and by channel, border, smooth, and
/// the selections kept while editing.
fn more_selection(ui: &mut egui::Ui, pe: &mut crate::pixel_edit_app::PixelEdit) {
    use crate::pixel_edit_app::MaskTarget;
    use pixel_edit::Tones;
    let mut quick = pe.target == MaskTarget::QuickMask;
    if ui
        .toggle_value(&mut quick, tr("pixel.sel.quick_mask"))
        .rich_tip(tr("pixel.sel.quick_mask_tip"))
        .changed()
    {
        pe.set_quick_mask(quick);
    }
    ui.horizontal_wrapped(|ui| {
        ui.label(tr("pixel.sel.by_tone"));
        for (t, key) in [
            (Tones::Shadows, "pixel.sel.shadows"),
            (Tones::Midtones, "pixel.sel.midtones"),
            (Tones::Highlights, "pixel.sel.highlights"),
        ] {
            if ui.button(tr(key)).clicked() {
                let m = pixel_edit::tone_mask(&pe.pi.img, t);
                pe.set_selection(Some(m));
            }
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.label(tr("pixel.sel.by_channel"));
        for (ch, key) in [
            (0, "pixel.sel.red"),
            (1, "pixel.sel.green"),
            (2, "pixel.sel.blue"),
            (3, "pixel.sel.alpha"),
        ] {
            if ui.button(tr(key)).clicked() {
                let m = pixel_edit::channel_mask(&pe.pi.img, ch);
                pe.set_selection(Some(m));
            }
        }
    });
    let has = pe.pi.sel.is_some();
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(has, egui::Button::new(tr("pixel.sel.border")))
            .rich_tip(tr("pixel.sel.border_tip"))
            .clicked()
        {
            if let Some(m) = pe
                .pi
                .sel
                .as_ref()
                .map(|m| pixel_edit::border(m, pe.feather_px))
            {
                pe.set_selection(Some(m));
            }
        }
        if ui
            .add_enabled(has, egui::Button::new(tr("pixel.sel.smooth")))
            .rich_tip(tr("pixel.sel.smooth_tip"))
            .clicked()
        {
            if let Some(m) = pe
                .pi
                .sel
                .as_ref()
                .map(|m| pixel_edit::smooth_selection(m, pe.feather_px))
            {
                pe.set_selection(Some(m));
            }
        }
        if ui
            .add_enabled(has, egui::Button::new(tr("pixel.sel.save")))
            .rich_tip(tr("pixel.sel.save_tip"))
            .clicked()
        {
            if let Some(m) = pe.pi.sel.clone() {
                pe.saved_sels.push(m);
            }
        }
    });
    let mut load: Option<usize> = None;
    let mut drop: Option<usize> = None;
    for (k, _) in pe.saved_sels.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.label(format!("{} {}", tr("pixel.sel.saved"), k + 1));
            if ui.small_button(tr("pixel.sel.load")).clicked() {
                load = Some(k);
            }
            if ui.small_button("x").clicked() {
                drop = Some(k);
            }
        });
    }
    if let Some(k) = load {
        let m = pe.saved_sels.get(k).cloned();
        if m.as_ref().map(|m| m.dimensions()) == Some(pe.pi.img.dimensions()) {
            pe.set_selection(m);
        }
    }
    if let Some(k) = drop {
        if k < pe.saved_sels.len() {
            pe.saved_sels.remove(k);
        }
    }
}

pub(crate) fn retouch_section(ui: &mut egui::Ui, pe: &mut crate::pixel_edit_app::PixelEdit) {
    let has = pe.pi.sel.is_some();
    if ui
        .add_enabled(has, egui::Button::new(tr("pixel.content_fill")))
        .rich_tip(tr("pixel.content_fill_tip"))
        .clicked()
    {
        pe.apply(false, pixel_edit::content_fill);
    }
    if ui
        .button(tr("pixel.red_eye"))
        .rich_tip(tr("pixel.red_eye_tip"))
        .clicked()
    {
        pe.apply(false, pixel_edit::red_eye_selection);
    }
}

pub(crate) fn effects_section(ui: &mut egui::Ui, pe: &mut crate::pixel_edit_app::PixelEdit) {
    // The layer in use is an adjustment layer: its effect's
    // settings, changed in place (a drag is one undo step).
    let i = pe.pi.active;
    if let Some(adj) = pe.pi.layers.get(i).and_then(|l| l.adjust.clone()) {
        ui.label(
            RichText::new(format!(
                "{} {}",
                tr("pixel.adjustment_of"),
                tr(&format!("fx.{}", adj.id))
            ))
            .strong(),
        );
        if let Some(spec) = crate::fx::spec(&adj.id) {
            let mut params = adj.params.clone();
            let mut changed = false;
            for p in spec.params {
                changed |=
                    crate::ui::effect_dialog::param_stacked(ui, p, &mut params, None);
            }
            if changed {
                if !pe.adjust_drag {
                    pe.remember();
                }
                if let Some(Some(e)) = pe.pi.layers.get_mut(i).map(|l| l.adjust.as_mut()) {
                    e.params = params;
                }
                pe.view.borrow_mut().full = true;
            }
            let down = ui.input(|r| r.pointer.any_down());
            pe.adjust_drag = (changed || pe.adjust_drag) && down;
        }
        ui.separator();
    }
    ui.label(
        RichText::new(tr("pixel.effects_hint"))
            .small()
            .color(Tokens::text_dim()),
    );
    // Light first: the effects most asked of a photo.
    ui.horizontal_wrapped(|ui| {
        for spec in crate::fx::light::EFFECTS {
            let on = pe.effect.as_ref().is_some_and(|e| e.id == spec.id);
            if ui
                .selectable_label(on, tr(&format!("fx.{}", spec.id)))
                .clicked()
            {
                pe.effect = Some(crate::fx::new_effect(spec.id));
                pe.refresh_preview();
            }
        }
    });
    let current = pe
        .effect
        .as_ref()
        .map(|e| tr(&format!("fx.{}", e.id)))
        .unwrap_or_else(|| tr("pixel.choose_effect"));
    let mut chosen: Option<&'static str> = None;
    egui::ComboBox::from_id_salt("pixel_effect")
        .width(220.0)
        .selected_text(current)
        .show_ui(ui, |ui| {
            for (group, list) in crate::fx::groups() {
                ui.label(RichText::new(tr(group)).strong());
                for spec in list.iter() {
                    if ui
                        .selectable_label(
                            false,
                            format!("   {}", tr(&format!("fx.{}", spec.id))),
                        )
                        .clicked()
                    {
                        chosen = Some(spec.id);
                    }
                }
            }
        });
    if let Some(id) = chosen {
        pe.effect = Some(crate::fx::new_effect(id));
        pe.refresh_preview();
    }
    let Some(spec) = pe.effect.as_ref().and_then(|e| crate::fx::spec(&e.id)) else {
        return;
    };
    let mut changed = false;
    if let Some(e) = pe.effect.as_mut() {
        for p in spec.params {
            changed |= crate::ui::effect_dialog::param_stacked(ui, p, &mut e.params, None);
        }
    }
    ui.horizontal(|ui| {
        if ui
            .checkbox(&mut pe.preview_on, tr("pixel.preview"))
            .changed()
        {
            changed = true;
        }
        if ui.button(tr("pixel.apply")).clicked() {
            if let Some(e) = pe.effect.clone() {
                pe.apply(true, |pi| pixel_edit::apply_effect(pi, &e));
            }
        }
    });
    if changed {
        pe.refresh_preview();
    }
}

/// The histogram of what is seen inside the selection: lightness filled,
/// red, green and blue as lines, with the mean and the pixel count.
pub(crate) fn histogram_section(ui: &mut egui::Ui, pe: &mut crate::pixel_edit_app::PixelEdit) {
    let seen = if pe.pi.is_layered() {
        pe.pi.composite()
    } else {
        pe.pi.img.clone()
    };
    let h = crate::pixel_adjust::histogram(&seen, pe.pi.sel.as_ref());
    let (rect, _) = ui.allocate_exact_size(egui::vec2(220.0, 80.0), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 2.0, Tokens::field());
    let top = h.luma.iter().copied().max().unwrap_or(1).max(1) as f32;
    let x_at = |i: usize| rect.left() + i as f32 / 255.0 * rect.width();
    let y_at = |v: u32| rect.bottom() - (v as f32 / top).min(1.0) * rect.height();
    for (i, v) in h.luma.iter().enumerate() {
        p.line_segment(
            [
                egui::pos2(x_at(i), rect.bottom()),
                egui::pos2(x_at(i), y_at(*v)),
            ],
            Stroke::new(1.0, Tokens::text_dim()),
        );
    }
    for (ch, c) in [
        (&h.red, Color32::from_rgb(0xE0, 0x40, 0x40)),
        (&h.green, Color32::from_rgb(0x40, 0xB0, 0x40)),
        (&h.blue, Color32::from_rgb(0x40, 0x70, 0xE0)),
    ] {
        let pts: Vec<_> = ch
            .iter()
            .enumerate()
            .map(|(i, v)| egui::pos2(x_at(i), y_at(*v)))
            .collect();
        p.add(egui::Shape::line(pts, Stroke::new(1.0, c)));
    }
    ui.label(
        RichText::new(format!(
            "{} {:.0}   {} {}",
            tr("pixel.histogram.mean"),
            h.mean,
            tr("pixel.histogram.pixels"),
            h.count
        ))
        .small()
        .color(Tokens::text_dim()),
    );
}

/// Stretch Pixels and Warp.
pub(crate) fn distort_section(ui: &mut egui::Ui, pe: &mut crate::pixel_edit_app::PixelEdit) {
    use crate::pixel_warp::StretchDir;
    ui.horizontal_wrapped(|ui| {
        ui.label(tr("pixel.stretch"))
            .rich_tip(tr("pixel.stretch_tip"));
        for (d, label) in [
            (StretchDir::Up, "\u{23f6}"),
            (StretchDir::Down, "\u{23f7}"),
            (StretchDir::Left, "\u{25c0}"),
            (StretchDir::Right, "\u{25b6}"),
        ] {
            if ui.button(label).clicked() {
                if let Some(img) = crate::pixel_warp::stretch_pixels(&pe.pi, d) {
                    let name = tr("pixel.stretch_layer");
                    pe.apply_all(true, |pi| {
                        pi.insert_layer(name, img);
                        Some([0, 0, pi.img.width(), pi.img.height()])
                    });
                }
            }
        }
    });
    ui.horizontal_wrapped(|ui| {
        let busy = pe.warp.is_some();
        ui.add_enabled(
            !busy,
            egui::Slider::new(&mut pe.warp_grid, 2..=8).text(tr("pixel.warp.grid")),
        );
        if !busy {
            if ui
                .button(tr("pixel.warp.start"))
                .rich_tip(tr("pixel.warp.start_tip"))
                .clicked()
                && !pe.pi.active_locked()
            {
                pe.start_warp();
                pe.tool = PixelTool::Warp;
            }
        } else {
            if ui.button(tr("pixel.apply")).clicked() {
                pe.apply_warp();
            }
            if ui.button(tr("pixel.warp.reset")).clicked() {
                pe.start_warp();
                pe.tool = PixelTool::Warp;
            }
            if ui.button(tr("pixel.cancel")).clicked() {
                pe.cancel_warp();
            }
        }
    });
}

pub(crate) fn picture_section(ui: &mut egui::Ui, pe: &mut crate::pixel_edit_app::PixelEdit) {
    ui.horizontal_wrapped(|ui| {
        if ui.button(tr("pixel.flip_h")).clicked() {
            pe.apply_all(true, |pi| {
                pixel_edit::flip(pi, true);
                Some([0, 0, pi.img.width(), pi.img.height()])
            });
        }
        if ui.button(tr("pixel.flip_v")).clicked() {
            pe.apply_all(true, |pi| {
                pixel_edit::flip(pi, false);
                Some([0, 0, pi.img.width(), pi.img.height()])
            });
        }
    });
    let (w, h) = pe.pi.img.dimensions();
    ui.label(
        RichText::new(format!("{w} × {h} px"))
            .small()
            .color(Tokens::text_dim()),
    );
    // Smart Resize: the least noticed seams taken away or added.
    ui.label(tr("pixel.smart_resize"))
        .rich_tip(tr("pixel.smart_resize_tip"));
    ui.add(
        egui::Slider::new(&mut pe.smart_size.0, 10.0..=300.0)
            .suffix(" %")
            .text(tr("pixel.smart.width")),
    );
    ui.add(
        egui::Slider::new(&mut pe.smart_size.1, 10.0..=300.0)
            .suffix(" %")
            .text(tr("pixel.smart.height")),
    );
    let nw = ((w as f32 * pe.smart_size.0 / 100.0).round() as u32).max(1);
    let nh = ((h as f32 * pe.smart_size.1 / 100.0).round() as u32).max(1);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("{nw} × {nh} px"))
                .small()
                .color(Tokens::text_dim()),
        );
        if ui
            .add_enabled((nw, nh) != (w, h), egui::Button::new(tr("pixel.apply")))
            .clicked()
        {
            pe.apply_all(true, |pi| crate::pixel_liquify::smart_resize(pi, nw, nh));
            pe.smart_size = (100.0, 100.0);
        }
    });
}
