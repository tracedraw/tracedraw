//! Preview for "Create Merged Document": the imposed pages as they will
//! look, with layout options (n-up, sheet, margins, gutter) on the left
//! panel and "Create Document" / "Cancel" in the top bar.  Completely
//! separate from the print-preview code — no print settings are changed.

use crate::app::App;
use crate::export::print_sheets;
use crate::i18n::{tr, trf};
use crate::theme::Tokens;
use crate::ui::dialogs::PrintState;
use crate::ui::tips::RichTip;
use egui::Ui;
use tracedraw_core::{geometry::Size, Document};

const OPTIONS_MIN: f32 = 180.0;
const OPTIONS_DEFAULT: f32 = 240.0;
const OPTIONS_MAX: f32 = 500.0;

/// State for the merge-document preview panel.
pub struct MergePreview {
    pub st: PrintState,
    /// Index of the sheet page shown.
    pub sheet: usize,
    /// Zoom over the fitted size (1 = sheet fits the window).
    pub zoom: f32,
    options_need: f32,
    built: Option<(String, Document)>,
    texture: Option<(String, egui::TextureHandle)>,
}

impl MergePreview {
    pub fn new() -> Self {
        let mut st = PrintState::default();
        st.range = 0;
        st.all_pages = true;
        MergePreview {
            st,
            sheet: 0,
            zoom: 1.0,
            options_need: 0.0,
            built: None,
            texture: None,
        }
    }
}

/// A key that changes whenever the preview needs to be rebuilt.
fn settings_key(st: &PrintState) -> String {
    format!(
        "{}|{}x{}|{}|{:?}|{:?}",
        st.layout_kind,
        st.cols,
        st.rows,
        st.sheet,
        st.margins,
        st.gutter,
    )
}

/// Build (or return the cached) imposed document for the current settings.
fn sheets<'a>(app: &App, pv: &'a mut MergePreview) -> &'a Document {
    let key = settings_key(&pv.st);
    if pv.built.as_ref().map(|(k, _)| k != &key).unwrap_or(true) {
        pv.built = Some((key, print_sheets(app, &pv.st)));
        pv.texture = None;
    }
    &pv.built
        .get_or_insert_with(|| (String::new(), Document::default()))
        .1
}

/// Render one sheet page to a bitmap (white background).
fn render_page(
    page: &tracedraw_core::document::Page,
    w: u32,
    h: u32,
    zoom: f64,
) -> Option<egui::ColorImage> {
    let mut doc = Document::new("sheet", page.size);
    doc.pages = vec![page.clone()];
    let opts = tracedraw_render::RenderOptions {
        width: w,
        height: h,
        view: tracedraw_render::ViewTransform {
            zoom,
            origin_x: 0.0,
            origin_y: page.size.height * zoom,
        },
        preview: None,
        wireframe: false,
        ..tracedraw_render::RenderOptions::default()
    };
    let pm = tracedraw_render::render_page(&doc, page.id, &opts)?;
    let mut white = tiny_skia::Pixmap::new(pm.width(), pm.height())?;
    white.fill(tiny_skia::Color::WHITE);
    white.draw_pixmap(
        0,
        0,
        pm.as_ref(),
        &Default::default(),
        tiny_skia::Transform::identity(),
        None,
    );
    Some(egui::ColorImage::from_rgba_premultiplied(
        [white.width() as usize, white.height() as usize],
        white.data(),
    ))
}

/// Show the merge-document preview overlay.  Returns true when the user
/// confirmed ("Create Document") so the caller can apply the layout.
pub fn show(app: &mut App, ui: &mut Ui) {
    let Some(mut pv) = app.merge_preview.take() else {
        return;
    };

    let mut close = false;
    let mut confirmed = false;
    let ctx = ui.ctx().clone();

    // ── Top bar ────────────────────────────────────────────────────────
    egui::Panel::top("merge_preview_bar")
        .frame(
            egui::Frame::new()
                .fill(Tokens::menu_bar())
                .inner_margin(egui::Margin::symmetric(8, 4)),
        )
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                if ui.button(tr("merge.create_document")).clicked() {
                    confirmed = true;
                    close = true;
                }
                ui.separator();
                let n = sheets(app, &mut pv).pages.len().max(1);
                pv.sheet = pv.sheet.min(n - 1);
                if ui
                    .add_enabled(pv.sheet > 0, egui::Button::new("<"))
                    .rich_tip(tr("status.previous_page"))
                    .clicked()
                {
                    pv.sheet -= 1;
                }
                ui.label(trf(
                    "print.sheet_of",
                    &[
                        ("n", &(pv.sheet + 1).to_string()),
                        ("total", &n.to_string()),
                    ],
                ));
                if ui
                    .add_enabled(pv.sheet + 1 < n, egui::Button::new(">"))
                    .rich_tip(tr("status.next_page"))
                    .clicked()
                {
                    pv.sheet += 1;
                }
                ui.separator();
                ui.label(tr("print.zoom"));
                let mut percent = pv.zoom * 100.0;
                if ui
                    .add(
                        crate::ui::Slider::new(&mut percent, 25.0..=400.0)
                            .logarithmic(true)
                            .suffix(" %")
                            .max_decimals(0),
                    )
                    .changed()
                {
                    pv.zoom = percent / 100.0;
                }
                if ui.button(tr("print.fit")).clicked() {
                    pv.zoom = 1.0;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(tr("dialog.close"))
                        .rich_tip(crate::ui::tips::keyed("dialog.close"))
                        .clicked()
                    {
                        close = true;
                    }
                });
            });
        });

    // ── Left panel: layout options ──────────────────────────────────────
    let widest = (ui.max_rect().width() * 0.5).max(OPTIONS_MIN);
    let need = pv.options_need.clamp(OPTIONS_MIN, widest);
    let panel = egui::Panel::left("merge_preview_opts")
        .resizable(true)
        .default_size(OPTIONS_DEFAULT.max(need))
        .size_range(need..=OPTIONS_MAX.max(need))
        .show(ui, |ui| {
            egui::ScrollArea::both().show(ui, |ui| {
                let st = &mut pv.st;
                crate::ui::dialogs::print_layout_fields(app, ui, st);
            })
        });

    let scroll = &panel.inner;
    let chrome = panel.response.rect.width() - scroll.inner_rect.width();
    let want = scroll.content_size.x + chrome;
    if scroll.content_size.x > scroll.inner_rect.width() + 0.5 && want > pv.options_need + 0.5 {
        pv.options_need = want;
        if want <= widest {
            ctx.request_discard("merge preview options wider");
        }
    }

    // ── Central area: page thumbnail ────────────────────────────────────
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(egui::Color32::from_gray(90)))
        .show(ui, |ui| {
            let area = ui.available_rect_before_wrap();
            let sheet_idx = pv.sheet;
            let page = match sheets(app, &mut pv).pages.get(sheet_idx).cloned() {
                Some(p) => p,
                None => return,
            };
            let margin = 24.0;
            let fit = ((area.width() - 2.0 * margin) as f64 / page.size.width.max(1e-6))
                .min((area.height() - 2.0 * margin) as f64 / page.size.height.max(1e-6))
                .max(0.01);
            let zoom = (fit * pv.zoom as f64)
                .min(8000.0 / page.size.width.max(page.size.height).max(1e-6));
            let w = (page.size.width * zoom).ceil().max(1.0) as u32;
            let h = (page.size.height * zoom).ceil().max(1.0) as u32;
            let key = format!(
                "{}|{sheet_idx}|{w}x{h}",
                pv.built.as_ref().map(|(k, _)| k.as_str()).unwrap_or("")
            );
            if pv.texture.as_ref().map(|(k, _)| k != &key).unwrap_or(true) {
                if let Some(img) = render_page(&page, w, h, zoom) {
                    let tex =
                        ctx.load_texture("merge_preview", img, egui::TextureOptions::LINEAR);
                    pv.texture = Some((key, tex));
                }
            }
            if let Some((_, tex)) = &pv.texture {
                let size = egui::vec2(w as f32, h as f32);
                let sheet_rect = egui::Rect::from_center_size(area.center(), size);
                // Shadow
                ui.painter().rect_filled(
                    sheet_rect.translate(egui::vec2(3.0, 3.0)),
                    0.0,
                    egui::Color32::from_black_alpha(80),
                );
                let mut mesh = egui::Mesh::with_texture(tex.id());
                mesh.add_rect_with_uv(
                    sheet_rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
                ui.painter().add(mesh);
                // Margin lines (blue dashed)
                let k = zoom as f32;
                let m = &pv.st.margins;
                let left = sheet_rect.left() + m[0] as f32 * k;
                let right = sheet_rect.right() - m[1] as f32 * k;
                let top = sheet_rect.top() + m[2] as f32 * k;
                let bottom = sheet_rect.bottom() - m[3] as f32 * k;
                let p = ui.painter();
                let blue = egui::Color32::from_rgb(80, 140, 220);
                for (a, b) in [
                    (egui::pos2(left, sheet_rect.top()), egui::pos2(left, sheet_rect.bottom())),
                    (egui::pos2(right, sheet_rect.top()), egui::pos2(right, sheet_rect.bottom())),
                    (egui::pos2(sheet_rect.left(), top), egui::pos2(sheet_rect.right(), top)),
                    (egui::pos2(sheet_rect.left(), bottom), egui::pos2(sheet_rect.right(), bottom)),
                ] {
                    p.extend(egui::Shape::dashed_line(&[a, b], egui::Stroke::new(1.0, blue), 4.0, 2.0));
                }
            }
        });

    let (prev, next, esc) = ctx.input(|i| {
        (
            i.key_pressed(egui::Key::PageUp),
            i.key_pressed(egui::Key::PageDown),
            i.key_pressed(egui::Key::Escape),
        )
    });
    if prev {
        pv.sheet = pv.sheet.saturating_sub(1);
    }
    if next {
        pv.sheet += 1;
    }
    if esc {
        close = true;
    }

    if confirmed {
        // Apply the chosen imposition to the current document (the
        // intermediate merged doc, one record per page).
        let doc = app.engine.document().clone();
        let first_size = doc
            .pages
            .first()
            .map(|p| p.size)
            .unwrap_or(Size::new(210.0, 297.0));
        let options = crate::imposition::Options {
            layout: pv.st.layout(),
            sheet: pv.st.sheet_size_for(first_size),
            gutter: pv.st.gutter.max(0.0),
            gutter_y: Some(pv.st.gutter_y.max(0.0)),
            order: crate::imposition::PageOrder::from_index(pv.st.page_order),
            paper_margins: pv.st.margins,
            ..Default::default()
        };
        let imposed = crate::imposition::impose(&doc, &options);
        let n = imposed.pages.len();
        app.open_document(imposed, None);
        app.status = crate::i18n::trf("status.merge_done", &[("n", &n.to_string())]);
    }

    if !close {
        app.merge_preview = Some(pv);
    }
}
