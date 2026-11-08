//! The settings of TraceCut's Make tools in the TraceCut window: eyelets.
//! See docs/behavior/tracecut.md.

use crate::app::Units;
use crate::i18n::tr;
use crate::tracecut::{ToolId, TraceCutState};
use crate::ui::tips::RichTip;
use crate::ui::tracecut::{length, row};
use egui::Ui;
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

/// The settings of a Make tool; nothing for the other tools.
pub fn settings(ui: &mut Ui, units: Units, st: &mut TraceCutState) {
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
