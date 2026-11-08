//! TraceCut's Make tools: eyelet marks along an edge. The geometry is
//! `tracedraw_tracecut::eyelets`; this module works the results out for
//! the preview and turns them into a new group, one undo step for OK.
//! See docs/behavior/tracecut.md.

use crate::app::App;
use crate::i18n::{tr, trf};
use crate::tracecut_cut::{AddStyle, Change, Changes};
use tracedraw_tracecut::eyelets::{self, EyeletSpec};

impl App {
    /// Eyelet marks along the selection's box or its closed curves, as
    /// a group of hairline circles.
    pub fn tracecut_eyelets(&self, spec: &EyeletSpec) -> Changes {
        let targets = self.tracecut_targets();
        let mut out = Changes {
            label: "Eyelets",
            ..Default::default()
        };
        if targets.is_empty() {
            out.summary = tr("tracecut.select_first");
            return out;
        }
        let outlines = self.tracecut_outlines(&targets);
        let centres = eyelets::eyelets(&outlines, spec);
        if centres.is_empty() {
            out.summary = tr("tracecut.no_eyelets");
            return out;
        }
        let paths = eyelets::marks(&centres, spec.diameter);
        out.summary = trf(
            "tracecut.eyelets_found",
            &[("n", &centres.len().to_string())],
        );
        out.preview = paths.clone();
        out.changes = vec![Change::AddGroup {
            name: "tracecut.eyelets_group",
            paths,
            style: AddStyle::Hairline,
        }];
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::{Rect, Shape as _};
    use tracedraw_core::{Fill, ShapeKind};
    use tracedraw_tracecut::eyelets::Along;

    #[test]
    fn eyelets_go_round_the_selection_as_one_group_in_one_step() {
        let mut app = App::headless();
        let banner = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 400.0, 200.0),
                radius: 0.0,
                corners: None,
            })
            .expect("banner");
        app.select(vec![banner]);
        let spec = EyeletSpec {
            spacing: 100.0,
            diameter: 8.0,
            ..Default::default()
        };
        let c = app.tracecut_eyelets(&spec);
        assert_eq!(c.preview.len(), 12);
        let before = app.engine.history_labels().0.len();
        assert!(app.tracecut_apply_changes(&c));
        assert_eq!(app.engine.history_labels().0.len(), before + 1);
        assert_eq!(
            app.engine.history_labels().0.last().copied(),
            Some("Eyelets")
        );
        let g = app
            .doc()
            .find_shape(app.selection[0])
            .expect("group")
            .clone();
        assert_eq!(
            g.name.as_deref(),
            Some(tr("tracecut.eyelets_group").as_str())
        );
        let ShapeKind::Group { children } = &g.kind else {
            panic!("a group");
        };
        assert_eq!(children.len(), 12);
        assert!(children
            .iter()
            .all(|s| s.fill == Fill::None && s.stroke.is_some()));
        let b = children[0].page_path().bounding_box();
        assert!((b.width() - 8.0).abs() < 1e-6);
        assert!((b.center().x - 15.0).abs() < 1e-6 && (b.center().y - 15.0).abs() < 1e-6);
        // Along the curve: the same places for a rectangle.
        app.select(vec![banner]);
        let along = app.tracecut_eyelets(&EyeletSpec {
            along: Along::Curve,
            ..spec
        });
        assert_eq!(along.preview.len(), 12);
        app.undo();
        assert!(app.doc().find_shape(g.id).is_none());
    }

    #[test]
    fn without_room_or_selection_nothing_is_added() {
        let mut app = App::headless();
        assert!(app
            .tracecut_eyelets(&EyeletSpec::default())
            .changes
            .is_empty());
        let small = app
            .new_shape(ShapeKind::Rect {
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                radius: 0.0,
                corners: None,
            })
            .expect("small");
        app.select(vec![small]);
        let c = app.tracecut_eyelets(&EyeletSpec {
            along: Along::Curve,
            ..Default::default()
        });
        assert!(c.changes.is_empty());
        assert_eq!(c.summary, tr("tracecut.no_eyelets"));
        assert!(!app.tracecut_apply_changes(&c));
    }
}
