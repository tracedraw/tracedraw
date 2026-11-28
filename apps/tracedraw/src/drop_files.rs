//! Files dropped on the window from the desktop or a file manager (or
//! from the computer onto the page, in a browser). Let go over the
//! drawing window of an open drawing, what File > Import reads goes into
//! it, centred at the pointer. Let go over the title, menus, toolbars
//! or document tabs, the file opens in a tab of its own like File >
//! Open (#1142). Without a drawing each file opens like File > Open,
//! and a picture gets a new drawing. The place is known on every
//! platform: Windows is asked for the cursor; on Linux (X11 and
//! Wayland), macOS and in the browser the windowing layer reports the
//! pointer during the drag as a pointer move (`vendor/README.md`).
//! Should it ever be missing, a file that could go either way is asked
//! about. See `docs/behavior/drop-files.md`.

use crate::app::{App, IMPORT_EXTENSIONS, OPEN_EXTENSIONS};
use crate::import_place::Placement;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracedraw_core::geometry::Point;

/// What a dropped file does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dropped {
    /// Into the open drawing, like File > Import.
    Import,
    /// In a tab of its own, like File > Open.
    Open,
    /// A new drawing, with the file imported into it (pictures).
    NewWithImport,
    /// The place is not known and the file could go into the drawing or
    /// open in a tab of its own: the user is asked (#1142).
    Ask,
    /// Neither File > Open nor File > Import reads it.
    Unsupported,
}

/// Where files are let go.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DropOn {
    /// The drawing window of the drawing shown, with the point in page space.
    Drawing(Point),
    /// Elsewhere on the window while a drawing is shown: its title,
    /// menus, toolbars, document tabs, dockers.
    Frame,
    /// A drawing is shown but the windowing layer did not say where
    /// the pointer is (none of the supported platforms leaves it out).
    Unknown,
    /// No drawing is shown (Welcome Screen, or no drawing open).
    NoDrawing,
}

impl App {
    /// Files dropped on the window this frame; where they were let go
    /// decides for all of them.
    pub fn take_dropped_files(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        let hovered: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .hovered_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        let held = !hovered.is_empty() && dropped.is_empty() && drag_held();
        let lost = self
            .file_drag
            .step(&hovered, !dropped.is_empty(), held, now);
        if cfg!(windows) {
            if let Some(delay) = self.file_drag.recovery_repaint_delay() {
                ctx.request_repaint_after(delay);
            }
        }
        if !lost.is_empty() {
            self.drop_lost(ctx, &lost);
        }
        if dropped.is_empty() || self.file_drag.late_drop(&dropped, now) {
            return;
        }
        let on = self.drop_on(drop_pointer(ctx));
        crate::files::take_dropped(ctx, self, move |app, path| app.drop_file(&path, on));
    }

    /// Files let go over the window whose drop never reached the program
    /// (#2686): what a drop at the cursor would do, done. Let go outside
    /// the window, they were taken away and nothing happens.
    fn drop_lost(&mut self, ctx: &egui::Context, files: &[PathBuf]) {
        let Some(p) = drop_pointer(ctx).filter(|p| ctx.content_rect().contains(*p)) else {
            return;
        };
        let on = self.drop_on(Some(p));
        for path in files {
            self.drop_file(path, on);
        }
    }

    /// A drawing is open and shown (not the Welcome Screen); without one
    /// the property bar and the toolbox are pale too.
    pub(crate) fn drawing_shown(&self) -> bool {
        self.has_document() && !self.show_welcome
    }

    /// Where files let go with the pointer at `pointer` (window points,
    /// when known) land.
    pub fn drop_on(&self, pointer: Option<egui::Pos2>) -> DropOn {
        if !self.drawing_shown() {
            return DropOn::NoDrawing;
        }
        match pointer {
            Some(p) if self.canvas_rect.contains(p) => DropOn::Drawing(self.view.to_page(p)),
            Some(_) => DropOn::Frame,
            None => DropOn::Unknown,
        }
    }

    /// What dropping `path` on `on` does.
    pub fn dropped_kind(&self, path: &Path, on: DropOn) -> Dropped {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let importable =
            IMPORT_EXTENSIONS.contains(&ext.as_str()) || self.import_filter_for(&ext).is_some();
        let openable = OPEN_EXTENSIONS.contains(&ext.as_str());
        match on {
            DropOn::Drawing(_) if importable => Dropped::Import,
            // Where the pointer is not known, a drawing, which goes either
            // way (.cdr, .svg, .tdraw...), is asked about; a picture goes
            // into the drawing (#1142).
            DropOn::Unknown if importable && openable => Dropped::Ask,
            DropOn::Unknown if importable => Dropped::Import,
            _ if openable => Dropped::Open,
            DropOn::Frame | DropOn::NoDrawing if importable => Dropped::NewWithImport,
            _ => Dropped::Unsupported,
        }
    }

    /// Open or import one dropped file. A .zip holding drawings or
    /// pictures (a .cdr a messaging app sent as `name.cdr.zip`) has them
    /// unpacked and each dropped in its turn (#3853).
    pub fn drop_file(&mut self, path: &Path, on: DropOn) {
        let inside = unpack_drawings(path);
        if !inside.is_empty() {
            for p in &inside {
                self.drop_file(p, on);
            }
            return;
        }
        match self.dropped_kind(path, on) {
            Dropped::Import => {
                let at = match on {
                    DropOn::Drawing(at) => at,
                    _ => self.view_center(),
                };
                let mark = self.engine.begin_group();
                self.import_at_placement(path, Placement::Center(at));
                self.engine.end_group(mark, "Import");
            }
            Dropped::Open => self.open_path(PathBuf::from(path)),
            Dropped::NewWithImport => {
                self.new_document();
                let at = self.page_rect().center();
                self.import_path_placed(path, Some(at));
                self.fit_pending = true;
            }
            Dropped::Ask => self.drop_question.push(path.to_path_buf()),
            Dropped::Unsupported => {
                self.status = crate::i18n::trf(
                    "status.drop_unsupported",
                    &[("p", &path.display().to_string())],
                );
            }
        }
    }

    /// Answer the question about files dropped with no known place:
    /// `Some(true)` imports them into the drawing, `Some(false)` opens
    /// each in a tab of its own, `None` drops them (#1142).
    pub fn answer_drop_question(&mut self, import: Option<bool>) {
        let paths = std::mem::take(&mut self.drop_question);
        let on = match import {
            Some(true) => DropOn::Drawing(self.view_center()),
            Some(false) => DropOn::Frame,
            None => return,
        };
        for path in paths {
            self.drop_file(&path, on);
        }
    }

    /// The question about files dropped with no known place, in the
    /// middle of the window: import them, open them, or Cancel (Esc).
    pub fn drop_question_window(&mut self, ctx: &egui::Context) {
        if self.drop_question.is_empty() {
            return;
        }
        let mut answer = None;
        let mut answered = false;
        egui::Window::new(crate::i18n::tr("hint.drop_ask"))
            .id(egui::Id::new("drop_question"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                for path in &self.drop_question {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    ui.label(name);
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(crate::ui::btn_icons::Ib::new("hint.drop_ask_import"))
                        .clicked()
                    {
                        (answer, answered) = (Some(true), true);
                    }
                    if ui
                        .add(crate::ui::btn_icons::Ib::new("hint.drop_ask_open"))
                        .clicked()
                    {
                        (answer, answered) = (Some(false), true);
                    }
                    if ui.button(crate::i18n::tr("dialog.cancel")).clicked() {
                        answered = true;
                    }
                });
            });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            answered = true;
        }
        if answered {
            self.answer_drop_question(answer);
        }
    }
}

/// How long, seconds, the buttons are up over files still listed as
/// hovered before their drop counts as lost (#2686).
pub const LOST_DROP_WAIT: f64 = 0.5;

/// A file drag over the window (#2686). On Windows the drop or leave
/// event can be lost, leaving the files listed as hovered. After every
/// mouse button has been up for [`LOST_DROP_WAIT`], the files count as
/// let go at the cursor once. Other platforms rely on the reported
/// drop and leave events.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FileDrag {
    /// The files of the drag going on.
    files: Vec<PathBuf>,
    /// When every button was found up during it, seconds.
    released: Option<f64>,
    /// The drag is over: its drop came, or counts as lost.
    over: bool,
    /// The files last let go for a lost drop, and when.
    lost: Option<(Vec<PathBuf>, f64)>,
    /// The files listed as hovered when a drop was lost: the windowing
    /// layer keeps listing them, ahead of a later drag's own, until a drop
    /// or a leave does reach the program. They belong to a drag that is
    /// over and are not let go again (#2940).
    stale: Vec<PathBuf>,
}

impl FileDrag {
    /// One frame: the files hovered (with their paths), whether files were
    /// dropped, whether a mouse button is held, the time. Gives the files
    /// whose drop is lost, once.
    pub fn step(
        &mut self,
        hovered: &[PathBuf],
        dropped: bool,
        held: bool,
        now: f64,
    ) -> Vec<PathBuf> {
        if dropped || hovered.is_empty() {
            self.files.clear();
            self.released = None;
            self.over = false;
            self.stale.clear();
            return Vec::new();
        }
        if !hovered.starts_with(&self.stale) {
            self.stale.clear();
        }
        if hovered != self.files.as_slice() {
            self.files = hovered.to_vec();
            self.released = None;
            self.over = false;
        }
        if self.over {
            return Vec::new();
        }
        if held {
            self.released = None;
            return Vec::new();
        }
        let at = *self.released.get_or_insert(now);
        if now - at < LOST_DROP_WAIT {
            return Vec::new();
        }
        self.over = true;
        let files = self.fresh(hovered).to_vec();
        self.stale = hovered.to_vec();
        if files.is_empty() {
            return files;
        }
        self.lost = Some((files.clone(), now));
        files
    }

    /// The files of `hovered` that belong to the drag going on: without
    /// those of an earlier lost drop still listed ahead of them. Before,
    /// they counted again, and a file dropped, then deleted, came back
    /// with every later drop until TraceDraw was restarted (#2940).
    fn fresh<'a>(&self, hovered: &'a [PathBuf]) -> &'a [PathBuf] {
        hovered
            .strip_prefix(self.stale.as_slice())
            .unwrap_or(hovered)
    }

    /// Poll for a release while dragging, then allow the drop event
    /// time to arrive before recovering it.
    fn recovery_repaint_delay(&self) -> Option<Duration> {
        if self.files.is_empty() || self.over {
            return None;
        }
        Some(if self.released.is_some() {
            Duration::from_secs_f64(LOST_DROP_WAIT + 0.05)
        } else {
            Duration::from_millis(50)
        })
    }

    /// `dropped` are the files of a lost drop, coming in late: not taken
    /// again.
    pub fn late_drop(&mut self, dropped: &[PathBuf], now: f64) -> bool {
        match &self.lost {
            Some((files, at)) if files.as_slice() == dropped && now - at < 10.0 => {
                self.lost = None;
                true
            }
            _ => false,
        }
    }
}

/// The drawings (files TraceDraw opens) a .zip at `path` holds, unpacked into a
/// folder of their own under the temporary folder; none when `path` is
/// not a readable .zip or holds none (folders and other files are left).
pub fn unpack_drawings(path: &Path) -> Vec<PathBuf> {
    let is_zip = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"));
    if !is_zip {
        return Vec::new();
    }
    let Ok(file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let Ok(mut archive) = zip::ZipArchive::new(std::io::BufReader::new(file)) else {
        return Vec::new();
    };
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "tracedraw-zip-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    // A .cdr is itself a ZIP archive: one renamed `name.cdr.zip` to be
    // sent (sites take no .cdr) is the drawing, opened as a .cdr; it held
    // no drawing inside and did not open (#4435).
    let is_cdr = ["content/root.dat", "content/riffData.cdr"]
        .iter()
        .any(|m| archive.index_for_name(m).is_some());
    if is_cdr {
        drop(archive);
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("drawing")
            .to_string();
        let name = if Path::new(&stem)
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "cdr" | "cdt" | "cdrt"))
        {
            stem
        } else {
            format!("{stem}.cdr")
        };
        let to = dir.join(name);
        return match std::fs::create_dir_all(&dir).and_then(|_| std::fs::copy(path, &to)) {
            Ok(_) => vec![to],
            Err(_) => Vec::new(),
        };
    }
    let wanted = |name: &str| {
        let p = Path::new(name);
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        !name.starts_with("__MACOSX")
            && !p
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("._"))
            && OPEN_EXTENSIONS.contains(&ext.as_str())
            // Texts and tables beside a drawing are notes, not drawings.
            && !matches!(
                ext.as_str(),
                "txt" | "rtf" | "docx" | "xlsx" | "csv" | "tsv" | "zip"
            )
    };
    let mut out = Vec::new();
    for i in 0..archive.len() {
        let Ok(mut entry) = archive.by_index(i) else {
            continue;
        };
        let Ok(entry_name) = entry.name().map(|n| n.to_string()) else {
            continue;
        };
        if !entry.is_file() || !wanted(&entry_name) {
            continue;
        }
        // Only the file's own name: no folder of the archive is made, and
        // no name can climb out of the folder.
        let Some(name) = entry
            .enclosed_name()
            .and_then(|p| p.file_name().map(|n| n.to_owned()))
        else {
            continue;
        };
        if std::fs::create_dir_all(&dir).is_err() {
            return Vec::new();
        }
        let to = dir.join(name);
        let Ok(mut f) = std::fs::File::create(&to) else {
            continue;
        };
        if std::io::copy(&mut entry, &mut f).is_ok() {
            out.push(to);
        }
    }
    out
}

/// The pointer over the window, in points, while files are dragged over
/// it or let go. On Windows the system drag sends the window no pointer
/// moves, so the cursor is asked for; on Linux (X11) and macOS the
/// vendored windowing layer turns the drag's place into pointer moves,
/// and in the browser the vendored eframe does (`vendor/README.md`),
/// so it is what egui last saw (#1142).
fn drop_pointer(ctx: &egui::Context) -> Option<egui::Pos2> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::{Foundation::POINT, UI::WindowsAndMessaging::GetCursorPos};
        let inner = ctx.input(|i| i.viewport().inner_rect);
        let mut p = POINT { x: 0, y: 0 };
        // SAFETY: GetCursorPos writes the cursor's screen position, in
        // physical pixels, into `p`.
        if let Some(inner) = inner {
            if unsafe { GetCursorPos(&mut p) } != 0 {
                let ppp = ctx.pixels_per_point();
                let at = egui::pos2(p.x as f32 / ppp, p.y as f32 / ppp);
                return Some(at - inner.min.to_vec2());
            }
        }
    }
    ctx.input(|i| i.pointer.hover_pos())
}

/// Whether a mouse button is held during the file drag. Windows checks
/// the system button state to recover from a missing drop event.
fn drag_held() -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
            GetAsyncKeyState, VK_LBUTTON, VK_MBUTTON, VK_RBUTTON,
        };
        // SAFETY: GetAsyncKeyState only reads the key's state; the high
        // bit (a negative value) says the key is down now.
        [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON]
            .into_iter()
            .any(|k| unsafe { GetAsyncKeyState(i32::from(k)) } < 0)
    }
    #[cfg(not(windows))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// A file dropped by the windowing layer, as egui hands it over.
    #[derive(Debug)]
    struct File(PathBuf);

    impl egui::DroppedFile for File {
        fn path(&self) -> &Path {
            &self.0
        }

        fn bytes(&self) -> Result<Vec<u8>, String> {
            std::fs::read(&self.0).map_err(|e| e.to_string())
        }
    }

    fn frame(ctx: &egui::Context, app: &mut App, dropped: Vec<PathBuf>, hovered: bool) {
        frame_at(ctx, app, dropped, hovered, None);
    }

    /// A frame with the pointer at `pointer` (none: not known, as in a
    /// desktop drag on Linux).
    fn frame_at(
        ctx: &egui::Context,
        app: &mut App,
        dropped: Vec<PathBuf>,
        hovered: bool,
        pointer: Option<egui::Pos2>,
    ) {
        let input = egui::RawInput {
            events: pointer
                .map(|p| vec![egui::Event::PointerMoved(p)])
                .unwrap_or_default(),
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1600.0, 1000.0),
            )),
            dropped_files: dropped
                .into_iter()
                .map(|p| Arc::new(File(p)) as egui::DroppedFileHandle)
                .collect(),
            hovered_files: if hovered {
                vec![egui::HoveredFile::default()]
            } else {
                Vec::new()
            },
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| crate::ui::root(app, ui));
        out.textures_delta.clear();
    }

    fn setup() -> (egui::Context, App) {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        for _ in 0..3 {
            frame(&ctx, &mut app, Vec::new(), false);
        }
        app.zoom_to_page();
        frame(&ctx, &mut app, Vec::new(), false);
        (ctx, app)
    }

    fn dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("tracedraw-drop-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        dir
    }

    fn svg(dir: &Path) -> PathBuf {
        let path = dir.join("square.svg");
        std::fs::write(
            &path,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="200mm" height="200mm" viewBox="0 0 200 200"><rect x="150" y="150" width="20" height="20" fill="red"/></svg>"#,
        )
        .expect("svg");
        path
    }

    fn png(dir: &Path) -> PathBuf {
        let path = dir.join("dot.png");
        image::RgbaImage::from_pixel(4, 2, image::Rgba([0, 120, 215, 255]))
            .save(&path)
            .expect("png");
        path
    }

    fn shape_count(app: &App) -> usize {
        app.doc().all_layers().map(|l| l.shapes.len()).sum()
    }

    fn close(a: tracedraw_core::geometry::Point, b: tracedraw_core::geometry::Point) -> bool {
        (a - b).hypot() < 0.01
    }

    #[test]
    fn what_a_drop_does_by_kind_and_place() {
        use DropOn::*;
        let app = App::headless();
        let kind = |p: &str, on| app.dropped_kind(Path::new(p), on);
        let drawing = Drawing(Point::ZERO);
        assert_eq!(kind("a.svg", drawing), Dropped::Import);
        assert_eq!(kind("a.cdr", drawing), Dropped::Import);
        assert_eq!(kind("a.PSD", drawing), Dropped::Import);
        assert_eq!(kind("a.png", drawing), Dropped::Import);
        // TraceDraw's own drawings are imported like any other (#1142).
        assert_eq!(kind("a.tdraw", drawing), Dropped::Import);
        // On the title, menus or tabs a drawing opens in its own tab and
        // a picture gets a drawing of its own (#1142).
        assert_eq!(kind("a.cdr", Frame), Dropped::Open);
        assert_eq!(kind("a.svg", Frame), Dropped::Open);
        assert_eq!(kind("a.tdraw", Frame), Dropped::Open);
        assert_eq!(kind("a.png", Frame), Dropped::NewWithImport);
        // Where the pointer is not known, a drawing, which goes either
        // way, is asked about and a picture goes into the drawing.
        assert_eq!(kind("a.cdr", Unknown), Dropped::Ask);
        assert_eq!(kind("a.svg", Unknown), Dropped::Ask);
        assert_eq!(kind("a.tdraw", Unknown), Dropped::Ask);
        assert_eq!(kind("a.png", Unknown), Dropped::Import);
        assert_eq!(kind("a.svg", NoDrawing), Dropped::Open);
        assert_eq!(kind("a.cdr", NoDrawing), Dropped::Open);
        assert_eq!(kind("a.psd", NoDrawing), Dropped::Open);
        assert_eq!(kind("a.jpg", NoDrawing), Dropped::NewWithImport);
        assert_eq!(kind("a.zip", drawing), Dropped::Unsupported);
        assert_eq!(kind("a.zip", Frame), Dropped::Unsupported);
        assert_eq!(kind("noextension", NoDrawing), Dropped::Unsupported);
    }

    #[test]
    fn the_pointer_decides_between_the_drawing_window_and_the_rest() {
        let (_ctx, mut app) = setup();
        let inside = app.canvas_rect.center();
        assert_eq!(
            app.drop_on(Some(inside)),
            DropOn::Drawing(app.view.to_page(inside))
        );
        assert_eq!(app.drop_on(Some(egui::pos2(5.0, 5.0))), DropOn::Frame);
        assert_eq!(app.drop_on(None), DropOn::Unknown);
        app.show_welcome = true;
        assert_eq!(app.drop_on(Some(inside)), DropOn::NoDrawing);
    }

    #[test]
    fn a_drawing_dropped_on_the_menus_opens_in_a_tab_of_its_own() {
        let (ctx, mut app) = setup();
        let dir = dir("frame");
        let docs = app.docs.len();
        frame_at(
            &ctx,
            &mut app,
            vec![svg(&dir)],
            false,
            Some(egui::pos2(5.0, 5.0)),
        );
        assert_eq!(app.docs.len(), docs + 1, "opened in a tab of its own");
        assert_eq!(shape_count(&app), 1);
        // A picture there gets a drawing of its own too.
        frame_at(
            &ctx,
            &mut app,
            vec![png(&dir)],
            false,
            Some(egui::pos2(5.0, 5.0)),
        );
        assert_eq!(app.docs.len(), docs + 2, "a new drawing");
        assert_eq!(shape_count(&app), 1);
    }

    /// Should the place ever be missing, a picture goes into the drawing
    /// and a drawing that could go either way is asked about: imported
    /// or opened as answered, or nothing on Cancel (#1142).
    #[test]
    fn where_the_pointer_is_unknown_a_drawing_is_asked_about() {
        let (ctx, mut app) = setup();
        let dir = dir("unknown-place");
        let docs = app.docs.len();
        frame(&ctx, &mut app, vec![png(&dir)], false);
        assert_eq!(app.docs.len(), docs, "imported, no new tab");
        assert_eq!(shape_count(&app), 1);
        frame(&ctx, &mut app, vec![svg(&dir)], false);
        assert_eq!(app.docs.len(), docs, "asked first");
        assert_eq!(app.drop_question.len(), 1);
        frame(&ctx, &mut app, Vec::new(), false);
        app.answer_drop_question(Some(true));
        assert_eq!(app.docs.len(), docs, "imported");
        assert_eq!(shape_count(&app), 2);
        let bounds = app.selection_bounds().expect("import bounds");
        assert!(close(bounds.center(), app.view_center()));
        frame(&ctx, &mut app, vec![svg(&dir)], false);
        app.answer_drop_question(Some(false));
        assert_eq!(app.docs.len(), docs + 1, "opened in a tab of its own");
        frame(&ctx, &mut app, vec![svg(&dir)], false);
        app.answer_drop_question(None);
        assert!(app.drop_question.is_empty());
        assert_eq!(app.docs.len(), docs + 1, "cancelled");
    }

    /// A drag over the window reports its place as pointer moves on
    /// every platform (on Linux and macOS from the vendored windowing
    /// layer, in the browser from the vendored eframe): a .cdr or .svg
    /// let go over the drawing is imported, and over the menus it opens
    /// in its own tab (#1142).
    #[test]
    fn the_place_of_a_drag_decides_on_every_platform() {
        let (ctx, mut app) = setup();
        let dir = dir("drag-place");
        let docs = app.docs.len();
        let inside = app.canvas_rect.center();
        // Dragged over the drawing, then let go there.
        frame_at(&ctx, &mut app, Vec::new(), true, Some(inside));
        assert_eq!(
            app.drop_on(drop_pointer(&ctx)),
            DropOn::Drawing(app.view.to_page(inside))
        );
        frame_at(&ctx, &mut app, vec![svg(&dir)], false, Some(inside));
        assert_eq!(app.docs.len(), docs, "imported");
        assert_eq!(shape_count(&app), 1);
        // Dragged over the menus, then let go there.
        frame_at(&ctx, &mut app, Vec::new(), true, Some(egui::pos2(5.0, 5.0)));
        assert_eq!(app.drop_on(drop_pointer(&ctx)), DropOn::Frame);
        frame_at(
            &ctx,
            &mut app,
            vec![svg(&dir)],
            false,
            Some(egui::pos2(5.0, 5.0)),
        );
        assert_eq!(app.docs.len(), docs + 1, "opened");
    }

    #[test]
    fn hovering_files_does_not_paint_a_drop_overlay() {
        let (ctx, mut app) = setup();
        let input = egui::RawInput {
            events: vec![egui::Event::PointerMoved(app.canvas_rect.center())],
            screen_rect: Some(ctx.content_rect()),
            hovered_files: vec![egui::HoveredFile {
                path: Some(PathBuf::from("drawing.svg")),
                ..Default::default()
            }],
            ..Default::default()
        };
        let overlay = egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("drop_files_hint"));
        let _ = ctx.run_ui(input, |ui| {
            crate::ui::root(&mut app, ui);
            assert!(
                ctx.graphics(|graphics| graphics.get(overlay).is_none_or(|layer| layer.is_empty()))
            );
        });
        assert_eq!(shape_count(&app), 0, "hovering does not import the file");
    }

    #[test]
    fn a_file_dropped_on_a_drawing_is_imported_in_the_middle_of_the_view() {
        let (ctx, mut app) = setup();
        let path = svg(&dir("import"));
        let docs = app.docs.len();
        let inside = app.canvas_rect.center();
        frame_at(&ctx, &mut app, vec![path], false, Some(inside));
        assert_eq!(app.docs.len(), docs, "no new tab");
        assert_eq!(shape_count(&app), 1);
        assert_eq!(app.selection.len(), 1, "what came is selected");
        let b = app.selection_bounds().expect("bounds");
        assert!(close(b.center(), app.view_center()), "{b:?}");
        // One undo step takes it all back.
        app.undo();
        assert_eq!(shape_count(&app), 0);
    }

    #[test]
    fn a_file_drop_uses_the_release_point_with_a_zoomed_and_panned_view() {
        let (ctx, mut app) = setup();
        let center = app.canvas_rect.center();
        app.view.zoom_at(center, 1.75);
        app.view.pan(egui::vec2(91.0, -37.0));
        app.settings.formats.import_place = crate::prefs::ImportPlace::AsInFile;
        frame_at(&ctx, &mut app, Vec::new(), true, Some(center));

        let release = center + egui::vec2(122.0, 87.0);
        let fixtures = dir("release-point");
        for path in [svg(&fixtures), png(&fixtures)] {
            frame_at(&ctx, &mut app, vec![path], false, Some(release));
            assert_eq!(shape_count(&app), 1);
            assert_eq!(app.selection.len(), 1);
            let bounds = app.selection_bounds().expect("import bounds");
            assert!(app.view.to_screen(bounds.center()).distance(release) < 0.1);
            app.undo();
            assert_eq!(shape_count(&app), 0);
        }
    }

    #[test]
    fn a_pdf_drop_keeps_the_release_point_while_the_import_dialog_is_open() {
        let (ctx, mut app) = setup();
        let fixtures = dir("pdf-release-point");
        let mut source = App::headless();
        source.import_path(&svg(&fixtures));
        let path = fixtures.join("square.pdf");
        std::fs::write(&path, tracedraw_io::pdf::document_to_pdf(source.doc())).expect("pdf");
        app.settings.formats.pdf_ask_text = true;
        let release = app.canvas_rect.center() + egui::vec2(-116.0, 69.0);
        let at = app.view.to_page(release);
        frame_at(&ctx, &mut app, vec![path], false, Some(release));
        assert!(matches!(app.dialog, crate::ui::dialogs::Dialog::PdfText(_)));
        assert_eq!(shape_count(&app), 0);

        app.view.pan(egui::vec2(75.0, 34.0));
        let mut out = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ui| crate::ui::dialogs::show(&mut app, ui.ctx()),
        );
        out.textures_delta.clear();
        frame(&ctx, &mut app, Vec::new(), false);
        assert_eq!(shape_count(&app), 1, "{}", app.status);
        let bounds = app.selection_bounds().expect("import bounds");
        assert!(close(bounds.center(), at));
        app.undo();
        assert_eq!(shape_count(&app), 0);
    }

    /// A `.tdraw` let go over the drawing window is imported into it like
    /// a `.cdr`, not opened in a tab of its own; over the menus it opens
    /// (#1142).
    #[test]
    fn a_tdraw_dropped_on_a_drawing_is_imported_into_it() {
        let (ctx, mut app) = setup();
        let dir = dir("tdraw");
        // A drawing of one square, saved as TraceDraw's own format.
        let mut other = App::headless();
        other.import_path(&svg(&dir));
        assert_eq!(shape_count(&other), 1);
        let tdraw = dir.join("square.tdraw");
        other.save_to(tdraw.clone());
        assert!(tdraw.is_file(), "{}", other.status);

        let docs = app.docs.len();
        let inside = app.canvas_rect.center();
        frame_at(&ctx, &mut app, vec![tdraw.clone()], false, Some(inside));
        assert_eq!(app.docs.len(), docs, "imported, no new tab");
        assert_eq!(shape_count(&app), 1, "{}", app.status);
        assert_eq!(app.selection.len(), 1, "what came is selected");
        let b = app.selection_bounds().expect("bounds");
        assert!(close(b.center(), app.view_center()), "{b:?}");
        app.undo();
        assert_eq!(shape_count(&app), 0, "one undo step");
        // Over the menus it still opens in a tab of its own.
        frame_at(
            &ctx,
            &mut app,
            vec![tdraw],
            false,
            Some(egui::pos2(5.0, 5.0)),
        );
        assert_eq!(app.docs.len(), docs + 1, "opened in a tab of its own");
    }

    #[test]
    fn without_a_drawing_a_drop_opens_the_file_and_a_picture_gets_a_new_drawing() {
        let (ctx, mut app) = setup();
        let dir = dir("open");
        app.show_welcome = true;
        let docs = app.docs.len();
        frame(&ctx, &mut app, vec![svg(&dir)], false);
        assert_eq!(app.docs.len(), docs + 1, "opened in a tab of its own");
        assert!(!app.show_welcome);
        assert_eq!(shape_count(&app), 1);

        app.show_welcome = true;
        frame(&ctx, &mut app, vec![png(&dir)], false);
        assert_eq!(app.docs.len(), docs + 2, "a new drawing");
        assert!(!app.show_welcome);
        assert_eq!(shape_count(&app), 1);
        let b = app.selection_bounds().expect("bounds");
        assert!(close(b.center(), app.page_rect().center()), "{b:?}");
    }

    #[test]
    fn several_files_dropped_on_the_welcome_screen_open_side_by_side() {
        let (ctx, mut app) = setup();
        let dir = dir("several");
        app.show_welcome = true;
        let docs = app.docs.len();
        frame(&ctx, &mut app, vec![svg(&dir), png(&dir)], false);
        assert_eq!(app.docs.len(), docs + 2, "each file in its own tab");
    }

    #[test]
    fn an_unknown_file_only_reports_its_name_in_the_status_bar() {
        let (ctx, mut app) = setup();
        let path = dir("unknown").join("notes.zip");
        std::fs::write(&path, b"PK").expect("zip");
        frame(&ctx, &mut app, vec![path], false);
        assert_eq!(shape_count(&app), 0);
        assert!(app.status.contains("notes.zip"), "{}", app.status);
    }

    /// #3853: a drawing sent zipped (`sample.svg.zip`) and dropped opens
    /// as the drawing inside; the archive's other files are left.
    #[test]
    fn a_zipped_drawing_dropped_opens_as_the_drawing_inside() {
        let (ctx, mut app) = setup();
        let dir = dir("zipped");
        let svg = std::fs::read(svg(&dir)).expect("svg");
        let path = dir.join("sample.svg.zip");
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let o = zip::write::SimpleFileOptions::default();
        zip.start_file("notes.txt", o).expect("txt");
        std::io::Write::write_all(&mut zip, b"hello").expect("write");
        zip.start_file("art/sample.svg", o).expect("svg");
        std::io::Write::write_all(&mut zip, &svg).expect("write");
        std::fs::write(&path, zip.finish().expect("zip").into_inner()).expect("zip file");
        let inside = super::unpack_drawings(&path);
        assert_eq!(inside.len(), 1, "{inside:?}");
        assert_eq!(
            inside[0].file_name().and_then(|n| n.to_str()),
            Some("sample.svg")
        );
        app.show_welcome = true;
        let docs = app.docs.len();
        frame(&ctx, &mut app, vec![path], false);
        assert_eq!(app.docs.len(), docs + 1, "{}", app.status);
        assert!(shape_count(&app) > 0);
    }

    /// #4435: a .cdr renamed `name.cdr.zip` to be sent is the drawing
    /// itself (a .cdr is a ZIP archive), opened as a .cdr; it held no
    /// drawing inside and did not open.
    #[test]
    fn a_cdr_sent_as_a_zip_opens_as_the_cdr() {
        let dir = dir("cdr-zip");
        let mut doc = tracedraw_core::Document::new(
            "icons",
            tracedraw_core::geometry::Size::new(100.0, 100.0),
        );
        let id = doc.ids_mut().shape();
        doc.pages[0].layers[0]
            .shapes
            .push(tracedraw_core::Shape::new(
                id,
                tracedraw_core::ShapeKind::Rect {
                    rect: tracedraw_core::Rect::new(10.0, 10.0, 40.0, 30.0),
                    radius: 0.0,
                    corners: None,
                },
            ));
        let bytes = tracedraw_cdr::write::document_to_cdr(&doc);
        for (name, inside) in [("icons.cdr.zip", "icons.cdr"), ("icons.zip", "icons.cdr")] {
            let path = dir.join(name);
            std::fs::write(&path, &bytes).expect("zip");
            let got = super::unpack_drawings(&path);
            assert_eq!(got.len(), 1, "{name}: {got:?}");
            assert_eq!(got[0].file_name().and_then(|n| n.to_str()), Some(inside));
            assert_eq!(std::fs::read(&got[0]).expect("copy"), bytes);
        }
        // File > Open of it opens the drawing.
        let mut app = App::headless();
        let docs = app.docs.len();
        app.open_path(dir.join("icons.cdr.zip"));
        for _ in 0..400 {
            if app.docs.len() > docs {
                break;
            }
            app.poll_opening();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(app.docs.len(), docs + 1, "{}", app.status);
    }

    /// #4496: several .cdr files opened at once (File > Open with Ctrl or
    /// Shift) each open in a tab; the second one dropped the first, read
    /// off the interface's thread.
    #[test]
    fn several_cdr_files_opened_at_once_all_open() {
        let dir = dir("cdr-many");
        let doc =
            tracedraw_core::Document::new("one", tracedraw_core::geometry::Size::new(100.0, 100.0));
        let bytes = tracedraw_cdr::write::document_to_cdr(&doc);
        let paths: Vec<PathBuf> = ["a.cdr", "b.cdr", "c.cdr"]
            .iter()
            .map(|n| {
                let p = dir.join(n);
                std::fs::write(&p, &bytes).expect("cdr");
                p
            })
            .collect();
        let mut app = App::headless();
        let docs = app.docs.len();
        for p in &paths {
            app.open_path(p.clone());
        }
        for _ in 0..1200 {
            if app.docs.len() >= docs + 3 {
                break;
            }
            app.poll_opening();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(app.docs.len(), docs + 3, "{}", app.status);
    }

    /// #2686: when Windows loses a drop, the app polls while the button
    /// is held and waits briefly on release. Recovery runs once, then
    /// polling stops even if the files remain listed as hovered.
    #[test]
    fn a_file_drag_whose_drop_is_lost_ends_once_the_buttons_are_up() {
        let a = vec![PathBuf::from("a.svg")];
        let mut d = FileDrag::default();
        let poll = Some(Duration::from_millis(50));
        let wait = Some(Duration::from_secs_f64(LOST_DROP_WAIT + 0.05));
        assert_eq!(d.recovery_repaint_delay(), None);
        assert!(d.step(&a, false, true, 0.0).is_empty());
        assert_eq!(d.recovery_repaint_delay(), poll);
        assert!(d.step(&a, false, false, 1.0).is_empty());
        assert_eq!(d.recovery_repaint_delay(), wait);
        assert!(d
            .step(&a, false, false, 1.0 + LOST_DROP_WAIT / 2.0)
            .is_empty());
        // No drop came: the files count as let go, once.
        assert_eq!(d.step(&a, false, false, 1.0 + LOST_DROP_WAIT), a);
        assert!(d.step(&a, false, false, 3.0).is_empty());
        // A later click with the stale files listed does not resume polling.
        assert!(d.step(&a, false, true, 4.0).is_empty());
        assert_eq!(d.recovery_repaint_delay(), None);
        // The real drop coming in late is not taken twice.
        assert!(d.late_drop(&a, 4.5));
        assert!(!d.late_drop(&a, 4.6));
        // A drop that does come in time ends the drag as before.
        let mut d = FileDrag::default();
        d.step(&a, false, true, 0.0);
        d.step(&a, false, false, 0.1);
        assert!(d.step(&a, true, false, 0.2).is_empty());
        assert_eq!(d.recovery_repaint_delay(), None);
        assert!(d.step(&[], false, false, 5.0).is_empty());
        // A button pressed again before the wait is out keeps it a drag.
        let mut d = FileDrag::default();
        d.step(&a, false, false, 0.0);
        d.step(&a, false, true, 0.3);
        assert!(d.step(&a, false, false, 0.6).is_empty());
        assert_eq!(d.recovery_repaint_delay(), wait);
        // Other files start a new drag.
        let b = vec![PathBuf::from("b.png")];
        assert!(d.step(&b, false, true, 0.7).is_empty());
        assert_eq!(d.recovery_repaint_delay(), poll);
    }

    /// #2940: after a lost drop its files stay listed as hovered, ahead of
    /// the next drag's own, until a drop or a leave reaches the program.
    /// They are not let go again: a file dropped and then deleted came
    /// back with every later drop until TraceDraw was restarted.
    #[test]
    fn a_lost_drops_files_still_listed_do_not_come_back_with_the_next() {
        let a = PathBuf::from("a.svg");
        let b = PathBuf::from("b.png");
        let mut d = FileDrag::default();
        d.step(&[a.clone()], false, true, 0.0);
        d.step(&[a.clone()], false, false, 0.1);
        assert_eq!(d.step(&[a.clone()], false, false, 1.0), [a.clone()]);
        // Still listed alone: no polling and nothing let go.
        assert!(d.step(&[a.clone()], false, true, 2.0).is_empty());
        assert_eq!(d.recovery_repaint_delay(), None);
        // The next drag lists its file after the old one.
        let both = vec![a.clone(), b.clone()];
        assert!(d.step(&both, false, true, 3.0).is_empty());
        assert!(d.recovery_repaint_delay().is_some());
        assert_eq!(d.fresh(&both), [b.clone()]);
        d.step(&both, false, false, 3.1);
        assert_eq!(d.step(&both, false, false, 4.0), [b.clone()]);
        // A third drag, of the first file again, lets that go.
        let again = vec![a.clone(), b.clone(), a.clone()];
        d.step(&again, false, true, 5.0);
        assert!(d.recovery_repaint_delay().is_some());
        d.step(&again, false, false, 5.1);
        assert_eq!(d.step(&again, false, false, 6.0), [a.clone()]);
        // A drop that reaches the program ends it all.
        assert!(d.step(&again, true, false, 7.0).is_empty());
        assert_eq!(d.fresh(&again), again.as_slice());
    }

    /// #2686: the files of a lost drop let go over the drawing window are
    /// imported there; let go outside the window nothing happens.
    #[test]
    fn a_lost_drop_over_the_drawing_imports_and_outside_does_nothing() {
        let (ctx, mut app) = setup();
        let dir = dir("lost");
        let file = svg(&dir);
        let inside = app.canvas_rect.center() + egui::vec2(93.0, -54.0);
        frame_at(&ctx, &mut app, Vec::new(), false, Some(inside));
        let before = shape_count(&app);
        app.drop_lost(&ctx, std::slice::from_ref(&file));
        assert_eq!(shape_count(&app), before + 1, "imported into the drawing");
        let bounds = app.selection_bounds().expect("import bounds");
        assert!(app.view.to_screen(bounds.center()).distance(inside) < 0.1);
        frame_at(
            &ctx,
            &mut app,
            Vec::new(),
            false,
            Some(egui::pos2(-50.0, -50.0)),
        );
        app.drop_lost(&ctx, std::slice::from_ref(&file));
        assert_eq!(shape_count(&app), before + 1, "nothing outside the window");
    }
}
