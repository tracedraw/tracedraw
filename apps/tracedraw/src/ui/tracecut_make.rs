//! The settings of TraceCut's Make tools in the TraceCut window: eyelets
//! and the box maker.
//! See docs/behavior/tracecut.md.

use crate::app::Units;
use crate::i18n::tr;
use crate::tracecut::{ToolId, TraceCutState};
use crate::ui::tips::RichTip;
use crate::ui::tracecut::{length, row};
use egui::Ui;
use tracedraw_tracecut::boxes::{self, Sizes};
use tracedraw_tracecut::eyelets::{self, Along};

/// A whole-number field with its tip.
fn whole(ui: &mut Ui, v: &mut u32, range: (u32, u32), tip: &str) {
    let mut x = f64::from(*v);
    let r = ui
        .add_sized(
            [110.0, 22.0],
            crate::ui::field::NumField::new(&mut x)
                .range(f64::from(range.0)..=f64::from(range.1))
                .max_decimals(0),
        )
        .rich_tip(crate::ui::tips::keyed(tip));
    if r.changed() && x.is_finite() {
        *v = x.round().clamp(f64::from(range.0), f64::from(range.1)) as u32;
    }
}

/// A list of `options`, each named by its i18n key.
fn choice<T: PartialEq + Copy>(ui: &mut Ui, id: &str, v: &mut T, options: &[(T, &str)], tip: &str) {
    let shown = options
        .iter()
        .find(|(o, _)| o == v)
        .map(|(_, k)| tr(k))
        .unwrap_or_default();
    crate::ui::keynav::ComboBox::from_id_salt(("tracecut", id))
        .selected_text(shown)
        .width(200.0)
        .show_ui(ui, |ui| {
            for (o, k) in options {
                ui.selectable_value(v, *o, tr(k));
            }
        })
        .response
        .rich_tip(crate::ui::tips::keyed(tip));
}

/// Box Maker: the sizes, the material, the fingers and the layout.
fn box_settings(ui: &mut Ui, units: Units, st: &mut TraceCutState) {
    let s = &mut st.settings.boxes;
    for (key, v) in [
        ("tracecut.box_width", &mut s.width),
        ("tracecut.box_depth", &mut s.depth),
        ("tracecut.box_height", &mut s.height),
    ] {
        row(ui, &tr(key), |ui| {
            length(ui, units, v, boxes::SIZE_RANGE, key)
        });
    }
    row(ui, &tr("tracecut.box_sizes"), |ui| {
        choice(
            ui,
            "box_sizes",
            &mut s.sizes,
            &[
                (Sizes::Outside, "tracecut.box_outside"),
                (Sizes::Inside, "tracecut.box_inside"),
            ],
            "tracecut.box_sizes",
        )
    });
    row(ui, &tr("tracecut.box_thickness"), |ui| {
        length(
            ui,
            units,
            &mut s.thickness,
            boxes::THICKNESS_RANGE,
            "tracecut.box_thickness",
        )
    });
    row(ui, &tr("tracecut.box_finger"), |ui| {
        length(
            ui,
            units,
            &mut s.finger,
            boxes::FINGER_RANGE,
            "tracecut.box_finger",
        )
    });
    row(ui, "", |ui| {
        ui.checkbox(&mut s.open_top, tr("tracecut.box_open_top"))
            .rich_tip(crate::ui::tips::keyed("tracecut.box_open_top"));
    });
    row(ui, &tr("tracecut.box_gap"), |ui| {
        length(ui, units, &mut s.gap, boxes::GAP_RANGE, "tracecut.box_gap")
    });
}

/// The settings of a Make tool; nothing for the other tools.
pub fn settings(ui: &mut Ui, units: Units, st: &mut TraceCutState) {
    if st.tool == ToolId::BoxMaker {
        box_settings(ui, units, st);
    }
    if st.tool == ToolId::Eyelets {
        let s = &mut st.settings.eyelets;
        row(ui, &tr("tracecut.eyelet_along"), |ui| {
            choice(
                ui,
                "eyelet_along",
                &mut s.along,
                &[
                    (Along::Box, "tracecut.eyelet_along_box"),
                    (Along::Curve, "tracecut.eyelet_along_curve"),
                ],
                "tracecut.eyelet_along",
            )
        });
        row(ui, &tr("tracecut.eyelet_diameter"), |ui| {
            length(
                ui,
                units,
                &mut s.diameter,
                eyelets::DIAMETER_RANGE,
                "tracecut.eyelet_diameter",
            )
        });
        row(ui, &tr("tracecut.eyelet_inset"), |ui| {
            length(
                ui,
                units,
                &mut s.inset,
                eyelets::INSET_RANGE,
                "tracecut.eyelet_inset",
            )
        });
        row(ui, &tr("tracecut.eyelet_place"), |ui| {
            choice(
                ui,
                "eyelet_place",
                &mut s.by_count,
                &[
                    (false, "tracecut.eyelet_by_spacing"),
                    (true, "tracecut.eyelet_by_count"),
                ],
                "tracecut.eyelet_place",
            )
        });
        if s.by_count {
            row(ui, &tr("tracecut.eyelet_count"), |ui| {
                whole(
                    ui,
                    &mut s.count,
                    eyelets::COUNT_RANGE,
                    "tracecut.eyelet_count",
                )
            });
        } else {
            row(ui, &tr("tracecut.eyelet_spacing"), |ui| {
                length(
                    ui,
                    units,
                    &mut s.spacing,
                    eyelets::SPACING_RANGE,
                    "tracecut.eyelet_spacing",
                )
            });
        }
    }
}
