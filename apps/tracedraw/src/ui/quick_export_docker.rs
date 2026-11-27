//! The compact Quick Export docker.

use crate::app::App;
use crate::files::{self, FolderAction, FolderUse};
use crate::i18n::tr;
use crate::quick_export::{
    format_extension, format_index, QuickExportScope, QuickExportState, QUICK_FORMATS,
};
use crate::theme::{theme, Theme, Tokens};
use crate::ui::btn_icons::Ib;
use crate::ui::dialogs::{
    ico_size_combo, ExportColorMode, EXPORT_COLOR_MODES, EXPORT_FORMATS, EXPORT_SCALE_RANGE,
    MIN_EXPORT_DPI,
};
use crate::ui::field::{parse, select_all_when_focused};
use crate::ui::keynav::{self, ComboBox};
use crate::ui::tips::{keyed, pair, RichTip};
use egui::{
    vec2, Align, Button, Checkbox, Id, Label, Layout, RadioButton, Response, TextEdit, TextStyle,
    Ui,
};
use std::ops::RangeInclusive;
use std::path::PathBuf;

const ROW_H: f32 = 24.0;
const PATH_ACTION_W: f32 = 30.0;
const DROP_ARROW_W: f32 = 22.0;
const DPI_CHOICES: [f64; 8] = [72.0, 96.0, 100.0, 150.0, 200.0, 300.0, 600.0, 1200.0];
const SCALE_CHOICES: [f64; 8] = [0.25, 0.5, 0.75, 1.0, 1.5, 2.0, 3.0, 4.0];

fn text_width(ui: &Ui, text: String, style: TextStyle) -> f32 {
    ui.painter()
        .layout_no_wrap(text, style.resolve(ui.style()), ui.visuals().text_color())
        .size()
        .x
        .ceil()
}

fn label_width(ui: &Ui) -> f32 {
    [
        "quick_export.name",
        "quick_export.format",
        "quick_export.destination",
        "quick_export.color",
        "quick_export.dpi",
        "quick_export.scale",
        "docker.size",
    ]
    .into_iter()
    .map(|key| text_width(ui, tr(key), TextStyle::Body))
    .fold(0.0, f32::max)
    .min((ui.available_width() * 0.36).min(100.0))
}

fn form_row(ui: &mut Ui, key: &str, width: f32, fields: impl FnOnce(&mut Ui, Id)) {
    ui.horizontal(|ui| {
        let label = ui
            .allocate_ui_with_layout(
                vec2(width, ROW_H),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.set_min_width(width);
                    ui.add(Label::new(tr(key)).wrap())
                },
            )
            .inner;
        fields(ui, label.id);
    });
}

fn format_combo(ui: &mut Ui, format: &mut usize, width: f32) -> Response {
    *format = format_index(format_extension(*format));
    ComboBox::from_id_salt("format")
        .field_arrow()
        .selected_text(EXPORT_FORMATS.get(*format).map_or("PNG", |(name, _)| *name))
        .width(width)
        .truncate()
        .show_ui(ui, |ui| {
            for ext in QUICK_FORMATS {
                let index = format_index(ext);
                if let Some((name, _)) = EXPORT_FORMATS.get(index) {
                    ui.selectable_value(format, index, *name);
                }
            }
        })
        .response
        .rich_tip(keyed("quick_export.format"))
}

fn file_fields(ui: &mut Ui, app: &mut App, label_w: f32) {
    let gap = ui.spacing().item_spacing.x;
    let name = app.quick_export_default_name();
    let take_name = tr("quick_export.take_name");
    let action_w = (text_width(ui, take_name.clone(), TextStyle::Button) + 16.0).max(30.0);
    form_row(ui, "quick_export.name", label_w, |ui, label| {
        let width = (ui.available_width() - action_w - gap).max(1.0);
        ui.add_sized(
            vec2(width, ROW_H),
            TextEdit::singleline(&mut app.quick_export.name).hint_text(&name),
        )
        .labelled_by(label)
        .rich_tip(pair("quick_export.name", "quick_export.name_desc"));
        if ui
            .add_sized(vec2(action_w, ROW_H), Button::new(take_name))
            .rich_tip(keyed("quick_export.take_name_desc"))
            .clicked()
        {
            app.quick_export.name = name;
        }
    });
    form_row(ui, "quick_export.format", label_w, |ui, label| {
        format_combo(
            ui,
            &mut app.quick_export.export.format,
            ui.available_width().min(132.0),
        )
        .labelled_by(label);
    });

    if let FolderAction::PickFolder(token) = FolderUse::ExportAssets.action() {
        form_row(ui, "quick_export.destination", label_w, |ui, label| {
            let width = ui.available_width() - 2.0 * (PATH_ACTION_W + gap);
            folder_field(ui, app, label, width);
            let folder = app.quick_export_folder();
            if path_button(ui, "quick_export.browse", "quick_export.browse", true)
                .rich_tip(pair("quick_export.browse", "quick_export.browse_desc"))
                .clicked()
            {
                files::pick_folder_with(token, folder.as_deref(), |app, folder| {
                    app.remember_quick_export_folder(&folder);
                });
            }
            let open = path_button(
                ui,
                "quick_export.open_folder",
                "options.open_folder",
                folder.is_some(),
            )
            .rich_tip(keyed("options.open_folder"))
            .rich_disabled_tip(keyed("quick_export.no_folder"));
            if let Some(folder) = folder.filter(|_| open.clicked()) {
                match files::open_folder(token, &folder) {
                    Ok(()) => app.remember_quick_export_folder(&folder),
                    Err(error) => app.status = error,
                }
            }
        });
    }
}

/// The text and arrow share one frame and anchor the same popup.
fn editable_dropdown(
    ui: &mut Ui,
    label: Id,
    menu_label: &str,
    text: TextEdit<'_>,
    width: f32,
) -> (Response, egui::Popup<'static>) {
    let background = ui.painter().add(egui::Shape::Noop);
    let row = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let text = text.frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(4, 2)));
        let field = ui
            .add_sized(vec2((width - DROP_ARROW_W).max(24.0), ROW_H), text)
            .labelled_by(label);
        let arrow = ui
            .add_sized(vec2(DROP_ARROW_W, ROW_H), Button::new("").frame(false))
            .rich_tip(keyed(menu_label));
        arrow.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, arrow.enabled(), tr(menu_label))
        });
        let popup_id = arrow.id.with("list");
        if field.has_focus()
            && ui.input_mut(|input| {
                input.consume_key(egui::Modifiers::ALT, egui::Key::ArrowDown)
                    || input.consume_key(egui::Modifiers::NONE, egui::Key::F4)
            })
        {
            arrow.request_focus();
            egui::Popup::open_id(ui.ctx(), popup_id);
            keynav::list_opened_by_keys(ui.ctx(), popup_id);
        }
        let c = arrow.rect.center();
        let ink = ui.style().interact(&arrow).text_color();
        ui.painter().add(egui::Shape::line(
            vec![
                c + vec2(-3.0, -1.5),
                c + vec2(0.0, 1.5),
                c + vec2(3.0, -1.5),
            ],
            egui::Stroke::new(1.25, ink),
        ));
        (field, arrow, popup_id)
    });
    let (field, arrow, popup_id) = row.inner;
    let anchor = field.clone().union(arrow.clone());
    let visuals = ui.visuals();
    let stroke =
        if field.has_focus() || arrow.has_focus() || egui::Popup::is_id_open(ui.ctx(), popup_id) {
            visuals.selection.stroke
        } else if anchor.hovered() {
            visuals.widgets.hovered.bg_stroke
        } else {
            visuals.widgets.inactive.bg_stroke
        };
    ui.painter().set(
        background,
        egui::epaint::RectShape::new(
            anchor.rect,
            visuals.widgets.inactive.corner_radius,
            visuals.text_edit_bg_color(),
            stroke,
            egui::StrokeKind::Inside,
        ),
    );
    let popup = egui::Popup::from_toggle_button_response(&arrow)
        .id(popup_id)
        .anchor(&anchor)
        .style(keynav::list_style())
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .layout(Layout::top_down_justified(Align::Min));
    keynav::list_button_keys(ui.ctx(), &arrow, popup_id);
    (field, popup)
}

fn folder_field(ui: &mut Ui, app: &mut App, label: Id, width: f32) {
    let mut path = app.quick_export_folder_text();
    let (field, popup) = editable_dropdown(
        ui,
        label,
        "quick_export.recent_folders",
        TextEdit::singleline(&mut path).hint_text(tr("quick_export.no_folder")),
        width,
    );
    let field = field.rich_tip(keyed("quick_export.destination_desc"));
    if field.changed() {
        app.quick_export.folder_input = Some(path);
    }
    if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        if let Some(folder) = app.quick_export_folder() {
            if files::is_dir(&folder) {
                app.remember_quick_export_folder(&folder);
            } else {
                app.status = tr("dialog.no_such_folder");
            }
        }
    }
    let mut chosen = None;
    let mut removed: Option<PathBuf> = None;
    let list_width = width.max(260.0).min(ui.ctx().content_rect().width() - 24.0);
    popup.width(list_width).show(|ui| {
        ui.set_width(list_width);
        if app.settings.quick_export_folders.is_empty() {
            ui.label(tr("quick_export.no_recent_folders"));
        }
        let current = app.quick_export_folder();
        for folder in &app.settings.quick_export_folders {
            ui.push_id(folder, |ui| {
                ui.horizontal(|ui| {
                    let text = folder.to_string_lossy();
                    let selected = current
                        .as_ref()
                        .is_some_and(|path| files::same_path(path, folder));
                    let row_width =
                        ui.available_width() - DROP_ARROW_W - ui.spacing().item_spacing.x;
                    if ui
                        .add_sized(
                            vec2(row_width, ROW_H),
                            Button::selectable(selected, text.as_ref()).truncate(),
                        )
                        .rich_tip_with(|| text.into_owned())
                        .clicked()
                    {
                        chosen = Some(folder.clone());
                        ui.close();
                    }
                    let remove = ui
                        .add_sized(
                            vec2(DROP_ARROW_W, ROW_H),
                            Button::new("×").frame_when_inactive(false),
                        )
                        .rich_tip(keyed("welcome.remove_recent"));
                    remove.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Button,
                            remove.enabled(),
                            tr("welcome.remove_recent"),
                        )
                    });
                    if remove.clicked() {
                        removed = Some(folder.clone());
                    }
                });
            });
        }
    });
    if let Some(folder) = removed {
        app.remove_quick_export_folder(&folder);
    }
    if let Some(folder) = chosen {
        if files::is_dir(&folder) {
            app.remember_quick_export_folder(&folder);
        } else {
            app.status = tr("dialog.no_such_folder");
        }
    }
}

fn path_button(ui: &mut Ui, icon: &str, label: &str, enabled: bool) -> Response {
    let response = ui.add_enabled(
        enabled,
        Ib::with_text(icon, "").min_size(vec2(PATH_ACTION_W, ROW_H)),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, response.enabled(), tr(label))
    });
    response
}

fn color_field(ui: &mut Ui, st: &mut crate::ui::dialogs::ExportState, ext: &str, label_w: f32) {
    if ext == "pdf" {
        let original = tr("fxo.original_color");
        let cmyk = ExportColorMode::Cmyk.label();
        form_row(ui, "quick_export.color", label_w, |ui, label| {
            ComboBox::from_id_salt("pdf_color_mode")
                .field_arrow()
                .selected_text(if st.pdf_cmyk { &cmyk } else { &original })
                .width(ui.available_width().min(132.0))
                .truncate()
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut st.pdf_cmyk, false, original);
                    ui.selectable_value(&mut st.pdf_cmyk, true, cmyk);
                })
                .response
                .labelled_by(label)
                .rich_tip(pair("quick_export.color", "quick_export.color_desc"));
        });
    } else if ExportColorMode::applies_to(ext) {
        form_row(ui, "quick_export.color", label_w, |ui, label| {
            ComboBox::from_id_salt("color_mode")
                .field_arrow()
                .selected_text(st.color_mode_for(ext).label())
                .width(ui.available_width().min(132.0))
                .truncate()
                .show_ui(ui, |ui| {
                    for (index, color) in EXPORT_COLOR_MODES.iter().enumerate() {
                        if color.offered_for(ext) {
                            ui.selectable_value(&mut st.color_mode, index, color.label());
                        }
                    }
                })
                .response
                .labelled_by(label)
                .rich_tip(pair("quick_export.color", "quick_export.color_desc"));
        });
    }
}

fn number_dropdown(
    ui: &mut Ui,
    label: Id,
    key: &str,
    value: &mut f64,
    choices: &[f64],
    range: RangeInclusive<f64>,
    suffix: &str,
    width: f32,
) {
    let id = label.with("number_input");
    let draft = ui.data(|data| data.get_temp::<String>(id));
    let mut text = draft.clone().unwrap_or_else(|| format!("{value}{suffix}"));
    let (field, popup) = editable_dropdown(ui, label, key, TextEdit::singleline(&mut text), width);
    let field = field.rich_tip(keyed(key));
    select_all_when_focused(ui, &field, draft.is_none(), &text);
    if field.has_focus() {
        ui.data_mut(|data| data.insert_temp(id, text));
    } else if draft.is_some() || field.changed() {
        ui.data_mut(|data| data.remove::<String>(id));
        if !ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            if let Some(number) = parse(&text, None) {
                *value = number.clamp(*range.start(), *range.end());
            }
        }
    }
    let list_width = width.max(84.0);
    popup.width(list_width).show(|ui| {
        ui.set_width(list_width);
        for &choice in choices {
            if ui
                .selectable_label(*value == choice, format!("{choice}{suffix}"))
                .clicked()
            {
                *value = choice;
                ui.close();
            }
        }
    });
}

fn raster_fields(ui: &mut Ui, st: &mut crate::ui::dialogs::ExportState, ext: &str, label_w: f32) {
    if ext == "ico" {
        form_row(ui, "docker.size", label_w, |ui, label| {
            ico_size_combo(ui, &mut st.ico_size, ui.available_width().min(132.0))
                .labelled_by(label);
        });
        return;
    }
    let scale_label = tr("quick_export.scale");
    let scale_fields_w = text_width(ui, scale_label.clone(), TextStyle::Body)
        + 76.0
        + 3.0 * ui.spacing().item_spacing.x;
    let inline_scale = ui.available_width() >= label_w + scale_fields_w + 104.0;
    let scale = |ui: &mut Ui, value: &mut f64, label: Id| {
        number_dropdown(
            ui,
            label,
            "quick_export.scale_desc",
            value,
            &SCALE_CHOICES,
            EXPORT_SCALE_RANGE,
            "×",
            ui.available_width().min(76.0),
        );
    };
    form_row(ui, "quick_export.dpi", label_w, |ui, label| {
        number_dropdown(
            ui,
            label,
            "quick_export.dpi_desc",
            &mut st.dpi,
            &DPI_CHOICES,
            MIN_EXPORT_DPI..=2400.0,
            " dpi",
            ui.available_width().min(94.0),
        );
        if inline_scale {
            ui.add_space(ui.spacing().item_spacing.x);
            let label = ui.label(scale_label);
            scale(ui, &mut st.scale, label.id);
        }
    });
    if !inline_scale {
        form_row(ui, "quick_export.scale", label_w, |ui, label| {
            scale(ui, &mut st.scale, label)
        });
    }
}

fn export_button(ui: &mut Ui, enabled: bool) -> Response {
    let size = vec2(ui.available_width(), 30.0);
    ui.allocate_ui_with_layout(size, Layout::top_down(Align::Center), |ui| {
        ui.spacing_mut().button_padding.x = 12.0;
        if theme() != Theme::HighContrast {
            ui.visuals_mut().widgets.inactive.weak_bg_fill = Tokens::tool_hover();
        }
        ui.add_enabled(
            enabled,
            Ib::new("quick_export.export").min_size(vec2(0.0, size.y)),
        )
        .rich_tip(pair("quick_export.export", "quick_export.export_desc"))
    })
    .inner
}

fn export_options(ui: &mut Ui, q: &mut QuickExportState, ext: &str, raster: bool, drawing: bool) {
    let (label, tip) = if ext == "pdf" {
        (
            "quick_export.object_pages",
            "quick_export.object_pages_desc",
        )
    } else {
        (
            "quick_export.separate_objects",
            "quick_export.separate_objects_desc",
        )
    };
    ui.horizontal_wrapped(|ui| {
        if raster && ext != "psd" {
            if crate::export::keeps_alpha(ext) {
                ui.checkbox(&mut q.export.transparent, tr("quick_export.transparent"))
                    .rich_tip(keyed("quick_export.transparent_desc"));
            }
            ui.checkbox(&mut q.export.anti_alias, tr("quick_export.smooth"))
                .rich_tip(keyed("quick_export.smooth_desc"));
        }
        ui.add_enabled(drawing, Checkbox::new(&mut q.separate_objects, tr(label)))
            .rich_tip(keyed(tip));
        if matches!(
            FolderUse::ExportAssets.action(),
            FolderAction::PickFolder(_)
        ) {
            ui.add_enabled(
                drawing,
                Checkbox::new(&mut q.overwrite_same_name, tr("quick_export.overwrite")),
            )
            .rich_tip(keyed("quick_export.overwrite_desc"));
        }
    });
}

pub fn quick_export_docker(app: &mut App, ui: &mut Ui) {
    let drawing = app.drawing_shown();

    egui::ScrollArea::vertical()
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 5.0);
            ui.spacing_mut().interact_size.y = ROW_H.max(ui.spacing().interact_size.y);
            ui.visuals_mut().widgets.inactive.weak_bg_fill = Tokens::field();
            let label_w = label_width(ui);
            file_fields(ui, app, label_w);

            let q = &mut app.quick_export;
            let ext = format_extension(q.export.format);
            let raster = matches!(ext, "png" | "jpg" | "bmp" | "tif" | "webp" | "ico" | "psd");
            crate::ui::docker_form::section(ui, None);
            color_field(ui, &mut q.export, ext, label_w);
            if raster {
                raster_fields(ui, &mut q.export, ext, label_w);
            }
            export_options(ui, q, ext, raster, drawing);

            let scope_label = ui.label(tr("quick_export.scope"));
            ui.horizontal_wrapped(|ui| {
                for (scope, key) in [
                    (QuickExportScope::Selection, "quick_export.selection"),
                    (QuickExportScope::Page, "quick_export.page"),
                    (QuickExportScope::AllPages, "quick_export.all_pages"),
                ] {
                    if ui
                        .add_enabled(drawing, RadioButton::new(q.scope == scope, tr(key)))
                        .labelled_by(scope_label.id)
                        .rich_tip(keyed(key))
                        .clicked()
                    {
                        q.scope = scope;
                    }
                }
            });

            ui.add_space(8.0);
            if export_button(ui, drawing).clicked() {
                app.quick_export_start();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quick_export_fields_fit_both_narrow_and_wide_panels() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        app.quick_export.name = "An intentionally very long export filename".repeat(5);
        app.quick_export.folder_input = Some("an/extended/folder/".repeat(24));

        for ext in ["png", "jpg", "tif", "ico", "pdf", "svg", "psd"] {
            app.quick_export.export.format = format_index(ext);
            for width in [180.0, 324.0, 480.0] {
                let mut frame = ctx.run_ui(Default::default(), |ui| {
                    let rect =
                        egui::Rect::from_min_size(egui::pos2(16.0, 36.0), vec2(width, 360.0));
                    let mut page = ui.new_child(egui::UiBuilder::new().max_rect(rect));
                    page.set_clip_rect(rect);
                    page.spacing_mut().item_spacing = vec2(6.0, 5.0);
                    let label_w = label_width(&page);
                    let expected_field_x = rect.left() + label_w + page.spacing().item_spacing.x;
                    for key in ["quick_export.name", "quick_export.color", "docker.size"] {
                        form_row(&mut page, key, label_w, |ui, _| {
                            let field_x = ui.next_widget_position().x;
                            assert!(
                                (field_x - expected_field_x).abs() < 1.0,
                                "{key}: field starts at {field_x}, expected {expected_field_x}"
                            );
                        });
                    }
                    file_fields(&mut page, &mut app, label_w);
                    color_field(&mut page, &mut app.quick_export.export, ext, label_w);
                    let raster = matches!(ext, "png" | "jpg" | "tif" | "ico" | "psd");
                    if raster {
                        raster_fields(&mut page, &mut app.quick_export.export, ext, label_w);
                    }
                    export_options(&mut page, &mut app.quick_export, ext, raster, true);
                    let button = export_button(&mut page, true).rect;
                    assert!((button.center().x - rect.center().x).abs() < 1.0);
                    assert!(
                        button.width() < width,
                        "button fills the {width}-point panel"
                    );
                    let used = page.min_rect().right() - rect.left();
                    assert!(
                        used <= width + 1.0,
                        "{ext}: fields use {used} of {width} points"
                    );
                });
                frame.textures_delta.clear();
            }
        }
    }
}
