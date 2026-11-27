//! The Quick Export docker's state and export actions.
//!
//! Quick Export is a small, single-purpose front end over the existing
//! export pipeline. It keeps the common choices close to the drawing while
//! leaving the full Export dialog and asset list unchanged.

use crate::app::App;
use crate::export::{
    export_page_indexes, object_pages_document, prepare_export, prepare_export_document, ExportWork,
};
use crate::files::{self, FolderAction, FolderUse};
use crate::i18n::tr;
use crate::ui::dialogs::{
    export_done, export_format_of, export_with_file_dialog, write_prepared_export, ExportState,
    EXPORT_FORMATS,
};
use std::path::{Path, PathBuf};
use tracedraw_core::Shape;

/// The area of the drawing to export.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum QuickExportScope {
    #[default]
    Selection,
    Page,
    AllPages,
}

/// The choices kept by the Quick Export docker.
#[derive(Debug, Clone)]
pub struct QuickExportState {
    /// The common export options shared with File > Export.
    pub export: ExportState,
    /// A file stem. Empty means the drawing's name is used.
    pub name: String,
    /// The destination being typed. With no edit, show the most recent folder.
    pub folder_input: Option<String>,
    pub scope: QuickExportScope,
    /// Give each object in the export range its own file or PDF page.
    pub separate_objects: bool,
    /// Replace files already in the desktop destination folder.
    pub overwrite_same_name: bool,
}

impl Default for QuickExportState {
    fn default() -> Self {
        let mut export = ExportState::default();
        export.format = format_index("png");
        export.dpi = 100.0;
        export.scale = 1.0;
        export.transparent = true;
        export.anti_alias = true;
        QuickExportState {
            export,
            name: String::new(),
            folder_input: None,
            scope: QuickExportScope::default(),
            separate_objects: true,
            overwrite_same_name: false,
        }
    }
}

pub(crate) const RECENT_FOLDER_LIMIT: usize = 6;

/// Formats offered by the compact panel. The full Export dialog remains the
/// place for the less common interchange formats.
pub const QUICK_FORMATS: [&str; 9] = [
    "png", "jpg", "bmp", "tif", "ico", "webp", "pdf", "svg", "psd",
];

/// The index of `ext` in the shared export format table.
pub fn format_index(ext: &str) -> usize {
    EXPORT_FORMATS
        .iter()
        .position(|(_, candidate)| *candidate == ext)
        .unwrap_or(0)
}

/// The extension of a Quick Export format index.
pub fn format_extension(index: usize) -> &'static str {
    EXPORT_FORMATS
        .get(index)
        .map(|(_, ext)| *ext)
        .filter(|ext| QUICK_FORMATS.contains(ext))
        .unwrap_or("png")
}

impl App {
    /// A name suitable for the filename hint and the default export stem.
    pub fn quick_export_default_name(&self) -> String {
        self.file
            .as_ref()
            .and_then(|file| file.file_stem())
            .map(|stem| stem.to_string_lossy().to_string())
            .filter(|stem| !stem.trim().is_empty())
            .unwrap_or_else(|| "drawing".into())
    }

    fn quick_export_stem(&self) -> String {
        let typed = self.quick_export.name.trim();
        let default = self.quick_export_default_name();
        let format = format_index(format_extension(self.quick_export.export.format));
        let stem = if typed.is_empty() || typed == default {
            default.as_str()
        } else if export_format_of(Path::new(typed)) == Some(format) {
            typed.rsplit_once('.').map_or(typed, |(stem, _)| stem)
        } else {
            typed
        };
        crate::files::safe_file_name(&crate::files::clean_file_name(stem))
    }

    fn quick_export_options(&self) -> (ExportState, bool) {
        let mut st = self.quick_export.export.clone();
        st.format = format_index(format_extension(st.format));
        let selection_scope = self.quick_export.scope == QuickExportScope::Selection;
        st.selection_only = selection_scope && !self.selection.is_empty();
        st.all_pages = self.quick_export.scope == QuickExportScope::AllPages;
        st.page_list = false;
        st.pages.clear();
        st.area = None;
        // An empty selection falls back to one whole artboard.
        let split_objects =
            self.quick_export.separate_objects && (!selection_scope || st.selection_only);
        (st, split_objects)
    }

    fn quick_export_objects(&self, st: &ExportState) -> Vec<Shape> {
        if st.selection_only {
            return self.selected_shapes();
        }
        self.doc()
            .with_master_layers_on_pages(&export_page_indexes(self, st))
            .pages
            .into_iter()
            .flat_map(|page| page.layers)
            .filter(|layer| layer.visible && layer.printable)
            .flat_map(|layer| layer.shapes)
            .filter(|shape| shape.visible)
            .collect()
    }

    fn prepare_quick_object_export(
        &self,
        path: &Path,
        st: &ExportState,
        stem: &str,
    ) -> Result<ExportWork, String> {
        let shapes = self.quick_export_objects(st);
        if shapes.is_empty() {
            return Err(tr("quick_export.no_exportable_objects"));
        }
        let mut doc = object_pages_document(self, shapes);
        if format_extension(st.format) == "pdf" {
            return Ok(prepare_export_document(self, path, st, doc));
        }
        for (i, page) in doc.pages.iter_mut().enumerate() {
            page.name = if page.name.trim().is_empty() {
                format!("{stem} {}", i + 1)
            } else {
                crate::export_assets::file_stem(&page.name)
            };
        }
        Ok(prepare_export_document(self, path, st, doc).naming_object_files())
    }

    pub(crate) fn quick_export_folder_text(&self) -> String {
        self.quick_export.folder_input.clone().unwrap_or_else(|| {
            self.settings
                .quick_export_folders
                .first()
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default()
        })
    }

    pub(crate) fn quick_export_folder(&self) -> Option<PathBuf> {
        let Some(text) = &self.quick_export.folder_input else {
            return self.settings.quick_export_folders.first().cloned();
        };
        let path = text.trim().trim_matches('"');
        (!path.is_empty()).then(|| PathBuf::from(path))
    }

    /// Set the destination and move it to the front of the local history.
    pub(crate) fn remember_quick_export_folder(&mut self, path: &Path) {
        let path = files::plain_path(&files::canonical(path));
        let folders = &mut self.settings.quick_export_folders;
        folders.retain(|folder| !files::same_path(folder, &path));
        folders.insert(0, path);
        folders.truncate(RECENT_FOLDER_LIMIT);
        self.quick_export.folder_input = None;
    }

    pub(crate) fn remove_quick_export_folder(&mut self, path: &Path) {
        if self
            .quick_export_folder()
            .is_some_and(|folder| files::same_path(&folder, path))
        {
            self.quick_export.folder_input = None;
        }
        self.settings
            .quick_export_folders
            .retain(|folder| !files::same_path(folder, path));
    }

    /// Start a quick export in the chosen folder or through the save picker.
    pub fn quick_export_start(&mut self) {
        if !self.has_document() || self.show_welcome {
            return;
        }
        if let FolderAction::PickFolder(token) = FolderUse::ExportAssets.action() {
            if let Some(folder) = self.quick_export_folder() {
                self.quick_export_to_folder(folder);
            } else {
                files::pick_folder_with(token, None, |app, folder| {
                    app.quick_export_to_folder(folder);
                });
            }
            return;
        }
        let (st, split_objects) = self.quick_export_options();
        let stem = self.quick_export_stem();
        if split_objects {
            self.quick_export_objects_through_picker(&st, stem);
        } else {
            export_with_file_dialog(self, &st, false, Some(&stem));
        }
    }

    fn quick_export_objects_through_picker(&mut self, st: &ExportState, stem: String) {
        let (_, ext) = EXPORT_FORMATS[st.format];
        let (label, suffix) = if ext == "pdf" {
            ("PDF", "pdf")
        } else {
            ("ZIP", "zip")
        };
        let filename = format!("{stem}.{suffix}");
        let mut work = match self.prepare_quick_object_export(Path::new(&filename), st, &stem) {
            Ok(work) => work,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        files::Dialog::new()
            .add_filter(label, &[suffix])
            .set_file_name(filename)
            .save_file(self, move |app, path| {
                if suffix == "pdf" {
                    work.path = path;
                    write_prepared_export(app, work, false);
                } else {
                    let mut result = Ok(String::new());
                    let zip = files::zip_written(&stem, |folder| {
                        work.path = folder.join(format!("{stem}.{ext}"));
                        result = work
                            .keeping_existing_files()
                            .run(&std::sync::atomic::AtomicUsize::new(0));
                    });
                    let result = result.and_then(|status| {
                        files::write(path, zip?).map_err(|error| error.to_string())?;
                        Ok(status)
                    });
                    export_done(app, result, false);
                }
            });
    }

    /// Export to the chosen desktop folder.
    pub fn quick_export_to_folder(&mut self, folder: PathBuf) {
        if !files::is_dir(&folder) {
            self.status = tr("dialog.no_such_folder");
            return;
        }
        self.remember_quick_export_folder(&folder);
        let (st, split_objects) = self.quick_export_options();
        let ext = format_extension(st.format);
        let stem = self.quick_export_stem();
        let path = folder.join(format!("{stem}.{ext}"));
        let work = if split_objects {
            match self.prepare_quick_object_export(&path, &st, &stem) {
                Ok(work) => work,
                Err(error) => {
                    self.status = error;
                    return;
                }
            }
        } else {
            prepare_export(self, &path, &st)
        };
        let work = if self.quick_export.overwrite_same_name {
            work
        } else {
            work.keeping_existing_files()
        };
        write_prepared_export(self, work, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quick_formats_round_trip_through_shared_export_table() {
        for ext in QUICK_FORMATS {
            assert_eq!(format_extension(format_index(ext)), ext);
        }
        assert_eq!(format_extension(usize::MAX), "png");
    }

    #[test]
    fn quick_export_remembers_six_unique_folders_in_settings() {
        let mut app = App::headless();
        let folders: Vec<_> = (0..8)
            .map(|i| files::plain_path(&files::canonical(Path::new(&format!("exports/{i}")))))
            .collect();
        for folder in &folders {
            app.remember_quick_export_folder(folder);
        }
        app.remember_quick_export_folder(&folders[4]);
        let expected = [4, 7, 6, 5, 3, 2].map(|i| folders[i].clone());
        assert_eq!(app.settings.quick_export_folders, expected);
        assert_eq!(app.quick_export_folder().as_ref(), Some(&expected[0]));

        app.remove_quick_export_folder(&folders[6]);
        assert_eq!(app.quick_export_folder().as_ref(), Some(&expected[0]));
        app.remove_quick_export_folder(&folders[4]);
        let saved = serde_json::to_string(&app.settings).expect("saved preferences");
        let restored: crate::settings::Settings =
            serde_json::from_str(&saved).expect("preferences");
        let mut next = App::headless();
        next.apply_settings(restored.upgrade());
        assert_eq!(
            next.settings.quick_export_folders,
            [7, 5, 3, 2].map(|i| folders[i].clone())
        );
        assert_eq!(next.quick_export_folder(), Some(folders[7].clone()));
        next.quick_export.folder_input = Some(" ".into());
        assert_eq!(next.quick_export_folder(), None);
    }

    #[test]
    fn quick_export_invalid_typed_folder_stays_out_of_history() {
        let mut app = App::headless();
        let invalid = PathBuf::from("nonexistent-quick-export-folder");
        app.quick_export.folder_input = Some(invalid.display().to_string());
        app.quick_export_to_folder(invalid);
        assert!(app.settings.quick_export_folders.is_empty());
        assert_eq!(app.status, tr("dialog.no_such_folder"));
    }

    #[test]
    fn quick_export_defaults_to_a_transparent_png_at_compact_resolution() {
        let state = QuickExportState::default();
        assert_eq!(format_extension(state.export.format), "png");
        assert_eq!(state.export.dpi, 100.0);
        assert_eq!(state.export.scale, 1.0);
        assert!(state.export.transparent);
        assert!(state.export.anti_alias);
        assert_eq!(state.scope, QuickExportScope::Selection);
        assert!(state.separate_objects);
        assert!(!state.overwrite_same_name);
    }

    #[test]
    fn quick_export_preserves_dots_in_the_filename_stem() {
        let mut app = App::headless();
        app.file = Some(PathBuf::from("Artwork.v2.tdraw"));
        app.quick_export.export.format = format_index("ico");
        assert_eq!(app.quick_export_stem(), "Artwork.v2");
        app.quick_export.name = app.quick_export_default_name();
        assert_eq!(app.quick_export_stem(), "Artwork.v2");
        for (typed, stem) in [
            ("Artwork.v2.ICO", "Artwork.v2"),
            ("Artwork.jpg", "Artwork.jpg"),
        ] {
            app.quick_export.name = typed.into();
            assert_eq!(app.quick_export_stem(), stem);
        }
        app.quick_export.export.format = format_index("jpg");
        app.quick_export.name = "Artwork.v2.JPEG".into();
        assert_eq!(app.quick_export_stem(), "Artwork.v2");
    }

    #[test]
    fn quick_export_page_files_only_replace_existing_files_with_overwrite_enabled() {
        let mut app = App::headless();
        app.add_page();
        app.quick_export.name = "Artwork.v2".into();
        app.quick_export.separate_objects = false;
        app.quick_export.export.format = format_index("ico");
        app.quick_export.export.ico_size = 16;
        let originals = ["Artwork.v2.ico", "Artwork.v2-1.ico"];
        let zip = crate::files::zip_written("icons", |folder| {
            for name in originals {
                crate::files::write(folder.join(name), b"original").expect("existing file");
            }
            app.quick_export_to_folder(folder.to_path_buf());
            app.quick_export.scope = QuickExportScope::AllPages;
            app.quick_export_to_folder(folder.to_path_buf());
            for name in originals {
                let bytes = crate::files::read(folder.join(name)).expect("retained file");
                assert_eq!(bytes.as_slice(), b"original");
            }
            app.quick_export.overwrite_same_name = true;
            app.quick_export_to_folder(folder.to_path_buf());
            app.quick_export.scope = QuickExportScope::Page;
            app.quick_export_to_folder(folder.to_path_buf());
            for name in originals {
                let bytes = files::read(folder.join(name)).expect("overwritten file");
                let icon = image::load_from_memory_with_format(&bytes, image::ImageFormat::Ico)
                    .expect("new ICO data");
                assert_eq!(icon.width(), 16);
            }
        })
        .expect("ZIP export");
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).expect("ZIP decode");
        assert_eq!(archive.len(), 5);
        for name in [
            "Artwork.v2 (2).ico",
            "Artwork.v2-1 (2).ico",
            "Artwork.v2-2.ico",
        ] {
            let name = format!("icons/{name}");
            assert!(archive.by_name(&name).is_ok(), "missing {name}");
        }
    }

    #[test]
    fn quick_export_pdf_puts_each_selected_object_on_its_own_cropped_page() {
        use tracedraw_core::{Color, Command, Fill, Rect, Shape, ShapeKind, Stroke};

        let mut app = App::headless();
        let layer = app.active_layer().expect("a layer");
        let red = Color::rgb8(255, 0, 0);
        let blue = Color::rgb8(0, 0, 255);
        let mut ids = Vec::new();
        for (rect, color, stroke) in [
            (Rect::new(5.0, 8.0, 25.0, 18.0), red, None),
            (
                Rect::new(70.0, 50.0, 100.0, 68.0),
                blue,
                Some(Stroke::new(Color::BLACK, 2.0)),
            ),
        ] {
            let id = app.engine.new_shape_id();
            let mut shape = Shape::new(
                id,
                ShapeKind::Rect {
                    rect,
                    radius: 0.0,
                    corners: None,
                },
            );
            shape.fill = Fill::Solid(color);
            shape.stroke = stroke;
            app.run(Command::AddShape { layer, shape });
            ids.push(id);
        }
        app.add_page();
        app.goto_page(0);
        app.select(ids);
        app.quick_export.export.format = format_index("pdf");
        app.quick_export.scope = QuickExportScope::Selection;
        app.quick_export.name = "Samples".into();
        app.quick_export.separate_objects = false;
        let before = app.doc().clone();
        let selection = app.selection.clone();

        let zip = crate::files::zip_written("pdfs", |folder| {
            app.quick_export_to_folder(folder.to_path_buf());
            app.quick_export.separate_objects = true;
            app.quick_export_to_folder(folder.to_path_buf());
            app.quick_export.export.pdf_cmyk = true;
            app.quick_export_to_folder(folder.to_path_buf());
            let read_pdf = |filename| {
                let bytes = crate::files::read(folder.join(filename)).expect("PDF file");
                lopdf::Document::load_mem(&bytes).expect("valid PDF")
            };
            assert_eq!(read_pdf("Samples.pdf").get_pages().len(), 1);
            let split = read_pdf("Samples (2).pdf");
            let pages = split.get_pages();
            assert_eq!(pages.len(), 2);
            for (&id, (size, fill, other_fill)) in pages.values().zip([
                ([20.0, 10.0], "1 0 0 rg", "0 0 1 rg"),
                ([32.0, 20.0], "0 0 1 rg", "1 0 0 rg"),
            ]) {
                let bounds = split
                    .get_dictionary(id)
                    .and_then(|page| page.get(b"MediaBox"))
                    .and_then(lopdf::Object::as_array)
                    .expect("page size");
                for (value, mm) in bounds.iter().zip([0.0, 0.0, size[0], size[1]]) {
                    let pt = mm * 72.0 / 25.4;
                    assert!((f64::from(value.as_float().expect("coordinate")) - pt).abs() < 0.001);
                }
                let contents = split.get_page_content(id);
                let contents = String::from_utf8_lossy(&contents);
                assert!(contents.contains(fill), "{contents}");
                assert!(!contents.contains(other_fill), "{contents}");
            }
            let cmyk = read_pdf("Samples (3).pdf");
            let pages = cmyk.get_pages();
            assert_eq!(pages.len(), 2);
            for id in pages.into_values() {
                let content = cmyk.get_and_decode_page_content(id).expect("page content");
                assert!(content.operations.iter().any(|op| op.operator == "k"));
                assert!(!content.operations.iter().any(|op| op.operator == "rg"));
            }
            app.quick_export.scope = QuickExportScope::AllPages;
            app.quick_export.separate_objects = false;
            app.quick_export_to_folder(folder.to_path_buf());
            let all_pages = read_pdf("Samples (4).pdf");
            let pages = all_pages.get_pages();
            assert_eq!(pages.len(), 2);
            let first = *pages.values().next().expect("first page");
            let bounds = all_pages
                .get_dictionary(first)
                .and_then(|page| page.get(b"MediaBox"))
                .and_then(lopdf::Object::as_array)
                .expect("page size");
            let width = bounds[2].as_float().expect("page width");
            assert!((f64::from(width) - before.pages[0].size.width * 72.0 / 25.4).abs() < 0.001);
        })
        .expect("ZIP export");
        let archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).expect("ZIP decode");
        assert_eq!(archive.len(), 4);
        assert_eq!(app.doc(), &before);
        assert_eq!(app.selection, selection);
    }

    #[test]
    fn quick_export_separates_visible_objects_in_each_range() {
        use tracedraw_core::{Color, Command, Fill, Rect, Shape, ShapeKind};

        let mut app = App::headless();
        let first_page = app.page;
        let add = |app: &mut App, name: &str, visible: bool| {
            let id = app.engine.new_shape_id();
            let mut shape = Shape::new(
                id,
                ShapeKind::Rect {
                    rect: Rect::new(5.0, 8.0, 25.0, 18.0),
                    radius: 0.0,
                    corners: None,
                },
            );
            shape.name = Some(name.into());
            shape.fill = Fill::Solid(Color::rgb8(255, 0, 0));
            shape.stroke = None;
            shape.visible = visible;
            let layer = app.active_layer().expect("active layer");
            app.run(Command::AddShape { layer, shape });
            id
        };
        let selected = add(&mut app, "One", true);
        add(&mut app, "Two", true);
        add(&mut app, "Hidden", false);
        app.run(Command::AddLayer {
            page: first_page,
            name: "Excluded".into(),
        });
        let excluded = app.active_layer().expect("added layer");
        add(&mut app, "Excluded", true);
        app.run(Command::SetLayerPrintable {
            layer: excluded,
            printable: false,
        });
        app.add_page();
        add(&mut app, "Three", true);
        app.goto_page(0);
        app.select(vec![selected]);
        let before = app.doc().clone();
        let selection = app.selection.clone();
        for (scope, names) in [
            (QuickExportScope::Selection, &["One"][..]),
            (QuickExportScope::Page, &["One", "Two"][..]),
            (QuickExportScope::AllPages, &["One", "Two", "Three"][..]),
        ] {
            app.quick_export.scope = scope;
            app.quick_export.export.format = format_index("pdf");
            let zip = crate::files::zip_written("pdf", |folder| {
                app.quick_export_to_folder(folder.to_path_buf());
                let bytes = crate::files::read(folder.join("drawing.pdf")).expect("PDF file");
                let pdf = lopdf::Document::load_mem(&bytes).expect("PDF decode");
                assert_eq!(pdf.get_pages().len(), names.len());
                for id in pdf.get_pages().into_values() {
                    let page = pdf.get_dictionary(id).expect("page");
                    let bounds = page
                        .get(b"MediaBox")
                        .expect("bounds")
                        .as_array()
                        .expect("array");
                    let width = f64::from(bounds[2].as_float().expect("width"));
                    assert!((width - 20.0 * 72.0 / 25.4).abs() < 0.001);
                }
            })
            .expect("PDF export");
            let archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).expect("PDF ZIP");
            assert_eq!(archive.len(), 1);

            app.quick_export.export.format = format_index("png");
            let zip = crate::files::zip_written("images", |folder| {
                app.quick_export_to_folder(folder.to_path_buf());
            })
            .expect("image export");
            let archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).expect("image ZIP");
            assert_eq!(archive.len(), names.len());
            for name in names {
                let filename = format!("images/{name}.png");
                assert!(
                    archive.index_for_name(&filename).is_some(),
                    "missing {filename}"
                );
            }
        }
        assert_eq!(app.doc(), &before);
        assert_eq!(app.selection, selection);
        assert_eq!(app.page, first_page);
    }

    #[test]
    fn quick_export_defaults_to_selection_with_whole_current_artboard_fallback() {
        use tracedraw_core::{Color, Command, Fill, Rect, ShapeKind};

        let mut app = App::headless();
        let id = app.engine.new_shape_id();
        let mut shape = Shape::new(
            id,
            ShapeKind::Rect {
                rect: Rect::new(5.0, 8.0, 25.0, 18.0),
                radius: 0.0,
                corners: None,
            },
        );
        shape.fill = Fill::Solid(Color::rgb8(255, 0, 0));
        shape.stroke = None;
        let layer = app.active_layer().expect("active layer");
        app.run(Command::AddShape { layer, shape });
        app.add_page();
        app.goto_page(0);
        let page_width = app.page_size().width;
        app.quick_export.export.format = format_index("pdf");
        let zip = files::zip_written("pdf", |folder| {
            app.remember_quick_export_folder(folder);
            app.quick_export_start();
            app.select(vec![id]);
            app.quick_export_start();
            for (name, expected_width) in [("drawing.pdf", page_width), ("drawing (2).pdf", 20.0)] {
                let bytes = files::read(folder.join(name)).expect("PDF file");
                let pdf = lopdf::Document::load_mem(&bytes).expect("PDF decode");
                let pages = pdf.get_pages();
                assert_eq!(pages.len(), 1);
                let page = pdf
                    .get_dictionary(*pages.values().next().expect("page"))
                    .expect("dict");
                let bounds = page
                    .get(b"MediaBox")
                    .expect("page bounds")
                    .as_array()
                    .expect("array");
                let width = f64::from(bounds[2].as_float().expect("width"));
                assert!((width - expected_width * 72.0 / 25.4).abs() < 0.001);
            }
        })
        .expect("export");
        let archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).expect("ZIP decode");
        assert_eq!(archive.len(), 2);
        assert_eq!(app.quick_export.scope, QuickExportScope::Selection);
        assert!(app.quick_export.separate_objects);
    }

    #[test]
    fn quick_export_empty_artboard_reports_that_there_are_no_objects() {
        let mut app = App::headless();
        app.quick_export.scope = QuickExportScope::Page;
        let zip = crate::files::zip_written("empty", |folder| {
            app.quick_export_to_folder(folder.to_path_buf());
        })
        .expect("empty folder");
        let archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).expect("ZIP decode");
        assert!(archive.is_empty());
        assert_eq!(app.status, tr("quick_export.no_exportable_objects"));
    }

    #[test]
    fn quick_export_each_object_uses_ico_options_and_distinct_names() {
        use std::io::{Cursor, Read};
        use tracedraw_core::{Color, Command, Fill, Rect, Shape, ShapeKind};

        let mut app = App::headless();
        let layer = app.active_layer().expect("a layer");
        let mut ids = Vec::new();
        for x in [0.0, 80.0] {
            let id = app.engine.new_shape_id();
            let mut shape = Shape::new(
                id,
                ShapeKind::Rect {
                    rect: Rect::new(x, 0.0, x + 20.0, 10.0),
                    radius: 0.0,
                    corners: None,
                },
            );
            shape.name = Some("Badge".into());
            shape.fill = Fill::Solid(Color::rgb8(255, 0, 0));
            shape.stroke = None;
            app.run(Command::AddShape { layer, shape });
            ids.push(id);
        }
        app.select(ids);
        app.add_page_assets();
        let old_assets = app.export_assets.assets.clone();
        let old_selection = app.selection.clone();
        let old_page = app.page;
        app.quick_export.scope = QuickExportScope::Selection;
        app.quick_export.separate_objects = true;
        app.quick_export.export.format = format_index("ico");
        app.quick_export.export.ico_size = 32;
        app.quick_export.export.transparent = false;
        app.quick_export.export.anti_alias = false;
        for overwrite in [false, true] {
            app.quick_export.overwrite_same_name = overwrite;
            let zip = crate::files::zip_written("icons", |folder| {
                let existing = folder.join("Badge.ico");
                crate::files::write(&existing, b"original").expect("existing icon");
                app.quick_export_to_folder(folder.to_path_buf());
                let bytes = crate::files::read(existing).expect("icon at the original path");
                assert_eq!(bytes.as_slice() == b"original", !overwrite);
            })
            .expect("ZIP export");
            assert_eq!(app.selection, old_selection);
            assert_eq!(app.page, old_page);
            assert_eq!(app.export_assets.assets, old_assets);
            let mut archive = zip::ZipArchive::new(Cursor::new(zip)).expect("ZIP decode");
            assert_eq!(archive.len(), if overwrite { 2 } else { 3 });
            let names = if overwrite {
                ["icons/Badge.ico", "icons/Badge (2).ico"]
            } else {
                ["icons/Badge (2).ico", "icons/Badge (3).ico"]
            };
            for name in names {
                let mut bytes = Vec::new();
                archive
                    .by_name(name)
                    .expect("distinct name")
                    .read_to_end(&mut bytes)
                    .expect("ICO bytes");
                let icon = image::load_from_memory_with_format(&bytes, image::ImageFormat::Ico)
                    .expect("ICO decode")
                    .to_rgba8();
                assert_eq!(icon.dimensions(), (32, 32));
                assert_eq!(icon.get_pixel(16, 0).0, [255; 4]);
                assert_eq!(icon.get_pixel(16, 16).0, [255, 0, 0, 255]);
            }
        }
    }
}
