//! Object > Insert > Serial Numbers: a run of numbers (01 to 99, any
//! start, end, step and number of digits) or a set of random codes of a
//! given length, each optionally on a background shape (a circle, a
//! star, a burst, a diamond or a fine guilloche rosette), laid out in a
//! grid in the middle of the page as one group of small groups, one undo
//! step. Every number is ordinary text, so it can be edited afterwards.

use crate::app::App;
use crate::i18n::tr;
use crate::ui::dialogs::{ok_cancel, window};
use crate::ui::field::NumField;
use crate::ui::tips::RichTip;
use egui::Context;
use tracedraw_core::document::{Shape, ShapeKind};
use tracedraw_core::geometry::{BezPath, Circle, Point, Shape as _, Vec2};
use tracedraw_core::style::{Fill, Stroke};
use tracedraw_core::{Color, Command, ShapeId, TextSpan};

/// A run of numbers or random codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SerialMode {
    #[default]
    Sequence,
    Random,
}

/// The characters random codes are made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Charset {
    #[default]
    Digits,
    /// Capitals and digits, leaving out 0, O, 1 and I, which are easy to
    /// mistake for each other.
    Alphanumeric,
    Hex,
}

impl Charset {
    fn chars(self) -> &'static [u8] {
        match self {
            Charset::Digits => b"0123456789",
            Charset::Alphanumeric => b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ",
            Charset::Hex => b"0123456789ABCDEF",
        }
    }
}

/// The shape behind each number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Backdrop {
    #[default]
    None,
    Circle,
    Star,
    Burst,
    Diamond,
    /// A rosette of fine looping lines, hard to copy by hand.
    Guilloche,
}

impl Backdrop {
    pub const ALL: [Backdrop; 6] = [
        Backdrop::None,
        Backdrop::Circle,
        Backdrop::Star,
        Backdrop::Burst,
        Backdrop::Diamond,
        Backdrop::Guilloche,
    ];

    fn key(self) -> &'static str {
        match self {
            Backdrop::None => "serials.back_none",
            Backdrop::Circle => "serials.back_circle",
            Backdrop::Star => "serials.back_star",
            Backdrop::Burst => "serials.back_burst",
            Backdrop::Diamond => "serials.back_diamond",
            Backdrop::Guilloche => "serials.back_guilloche",
        }
    }
}

/// The dialog's settings, kept for the next time.
#[derive(Debug, Clone, PartialEq)]
pub struct Serials {
    pub mode: SerialMode,
    pub start: i64,
    pub end: i64,
    pub step: i64,
    /// Numbers are padded with zeros to at least this many digits; random
    /// codes have exactly this many characters.
    pub digits: u32,
    pub count: u32,
    pub charset: Charset,
    pub prefix: String,
    pub suffix: String,
    pub backdrop: Backdrop,
    /// Width and height of each background shape (or cell), mm.
    pub size: f64,
    /// Columns of the grid.
    pub columns: u32,
    /// Space between the cells, mm.
    pub gap: f64,
    pub text_color: Color,
    pub back_color: Color,
}

impl Default for Serials {
    fn default() -> Self {
        Serials {
            mode: SerialMode::Sequence,
            start: 1,
            end: 99,
            step: 1,
            digits: 2,
            count: 20,
            charset: Charset::Digits,
            prefix: String::new(),
            suffix: String::new(),
            backdrop: Backdrop::Circle,
            size: 15.0,
            columns: 10,
            gap: 3.0,
            text_color: Color::BLACK,
            back_color: Color::rgb8(255, 205, 60),
        }
    }
}

/// The most numbers one insert makes.
pub const MAX_ITEMS: usize = 5000;

/// The next number of a xorshift generator, 0..1.
fn next(s: &mut u64) -> f64 {
    *s ^= *s << 13;
    *s ^= *s >> 7;
    *s ^= *s << 17;
    (*s >> 11) as f64 / (1u64 << 53) as f64
}

impl Serials {
    /// The texts, prefix and suffix included; random codes never repeat
    /// and come from `seed`.
    pub fn texts(&self, seed: u64) -> Vec<String> {
        let wrap = |s: String| format!("{}{s}{}", self.prefix, self.suffix);
        match self.mode {
            SerialMode::Sequence => {
                let step = if self.step == 0 { 1 } else { self.step.abs() };
                let (a, b) = (self.start, self.end);
                let width = self.digits.min(30) as usize;
                let mut out = Vec::new();
                let mut v = a;
                while out.len() < MAX_ITEMS && ((a <= b && v <= b) || (a > b && v >= b)) {
                    let n = if v < 0 {
                        format!("-{:0width$}", v.unsigned_abs())
                    } else {
                        format!("{v:0width$}")
                    };
                    out.push(wrap(n));
                    v = if a <= b { v.saturating_add(step) } else { v.saturating_sub(step) };
                    if v == i64::MAX || v == i64::MIN {
                        break;
                    }
                }
                out
            }
            SerialMode::Random => {
                let chars = self.charset.chars();
                let len = self.digits.clamp(1, 64) as usize;
                // There may be fewer codes of that length than asked for.
                let possible = (chars.len() as f64).powi(len as i32);
                let want = (self.count as usize).min(MAX_ITEMS).min(possible as usize);
                let mut s = seed | 1;
                let mut seen = std::collections::HashSet::new();
                let mut out = Vec::new();
                let mut tries = 0;
                while out.len() < want && tries < want * 50 + 100 {
                    tries += 1;
                    let code: String = (0..len)
                        .map(|_| {
                            let i = (next(&mut s) * chars.len() as f64) as usize;
                            chars[i.min(chars.len() - 1)] as char
                        })
                        .collect();
                    if seen.insert(code.clone()) {
                        out.push(wrap(code));
                    }
                }
                out
            }
        }
    }
}

/// A star of `points` tips round `c`, outer radius `r`, inner `r * k`.
fn star(c: Point, r: f64, points: usize, k: f64) -> BezPath {
    let mut p = BezPath::new();
    let n = points * 2;
    for i in 0..n {
        let a = -std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * i as f64 / points as f64;
        let rr = if i % 2 == 0 { r } else { r * k };
        let q = c + Vec2::new(a.cos(), a.sin()) * rr;
        if i == 0 {
            p.move_to(q);
        } else {
            p.line_to(q);
        }
    }
    p.close_path();
    p
}

/// A rosette of looping lines (a hypotrochoid) filling a circle of
/// radius `r` round `c`.
fn guilloche(c: Point, r: f64) -> BezPath {
    // Rolling circle 7/24 of the fixed one, the pen past its rim: the
    // curve closes after 7 turns with 24 loops.
    let (big, small) = (24.0, 7.0);
    let pen = 9.0;
    let reach = big - small + pen;
    let k = r / reach;
    let steps = 2400;
    let mut p = BezPath::new();
    for i in 0..=steps {
        let t = std::f64::consts::TAU * small * i as f64 / steps as f64;
        let x = (big - small) * t.cos() + pen * ((big - small) / small * t).cos();
        let y = (big - small) * t.sin() - pen * ((big - small) / small * t).sin();
        let q = c + Vec2::new(x, y) * k;
        if i == 0 {
            p.move_to(q);
        } else {
            p.line_to(q);
        }
    }
    p
}

/// The background path of `b` filling a `size` square round `c`.
fn backdrop_path(b: Backdrop, c: Point, size: f64) -> Option<BezPath> {
    let r = size / 2.0;
    Some(match b {
        Backdrop::None => return None,
        Backdrop::Circle => Circle::new(c, r).to_path(0.01),
        Backdrop::Star => star(c, r, 5, 0.45),
        Backdrop::Burst => star(c, r, 16, 0.78),
        Backdrop::Diamond => {
            let mut p = BezPath::new();
            p.move_to(c + Vec2::new(0.0, -r));
            p.line_to(c + Vec2::new(r, 0.0));
            p.line_to(c + Vec2::new(0.0, r));
            p.line_to(c + Vec2::new(-r, 0.0));
            p.close_path();
            p
        }
        Backdrop::Guilloche => guilloche(c, r),
    })
}

/// How much of the cell the text may take, across and down: less inside
/// a star or a diamond, which narrow towards their tips.
fn text_room(b: Backdrop) -> (f64, f64) {
    match b {
        Backdrop::None => (0.9, 0.8),
        Backdrop::Circle | Backdrop::Guilloche => (0.66, 0.5),
        Backdrop::Burst => (0.6, 0.45),
        Backdrop::Star | Backdrop::Diamond => (0.42, 0.3),
    }
}

impl App {
    pub fn open_serials_dialog(&mut self) {
        self.dialog = crate::ui::dialogs::Dialog::Serials(Box::new(self.last_serials.clone()));
    }

    /// A text shape of `text` in the current font, `pt` points, centred.
    fn serial_text(&mut self, text: &str, pt: f64, color: Color) -> Shape {
        let mut s = Shape::new(
            self.engine.new_shape_id(),
            ShapeKind::Text {
                spans: vec![TextSpan::new(text, self.text_font.clone(), pt)],
                origin: Point::ZERO,
                frame: None,
                align: tracedraw_core::TextAlign::Center,
                para: Default::default(),
                on_path: None,
            },
        );
        s.fill = Fill::Solid(color);
        s.stroke = None;
        s
    }

    /// Insert the numbers `st` gives in the middle of the page, as one
    /// group of one group per number, selected. One undo step.
    pub fn insert_serials(&mut self, st: &Serials) -> Option<ShapeId> {
        let seed = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15);
        self.insert_serials_seeded(st, seed)
    }

    pub fn insert_serials_seeded(&mut self, st: &Serials, seed: u64) -> Option<ShapeId> {
        let layer = self.active_layer()?;
        self.last_serials = st.clone();
        let texts = st.texts(seed);
        if texts.is_empty() {
            return None;
        }
        let size = st.size.clamp(1.0, 1000.0);
        let gap = st.gap.clamp(0.0, 1000.0);
        let cols = (st.columns.max(1) as usize).min(texts.len());
        let rows = texts.len().div_ceil(cols);
        // One size for every number, so the widest fits.
        let (room_w, room_h) = text_room(st.backdrop);
        let longest = texts.iter().max_by_key(|t| t.chars().count()).cloned().unwrap_or_default();
        let probe = self.serial_text(&longest, 100.0, st.text_color);
        let b = self.visual_bounds(&probe);
        let pt = if b.width() > 1e-9 && b.height() > 1e-9 {
            (100.0 * (size * room_w / b.width()).min(size * room_h / b.height())).clamp(1.0, 2000.0)
        } else {
            12.0
        };
        let pitch = size + gap;
        let page = self.page_rect();
        let total = Vec2::new(cols as f64 * pitch - gap, rows as f64 * pitch - gap);
        let corner = page.center() - total / 2.0;
        let mut items = Vec::with_capacity(texts.len());
        for (i, t) in texts.iter().enumerate() {
            let (r, c) = (i / cols, i % cols);
            let mid = corner + Vec2::new(c as f64 * pitch + size / 2.0, r as f64 * pitch + size / 2.0);
            let mut text = self.serial_text(t, pt, st.text_color);
            // Centre the letters themselves on the cell.
            let tb = self.visual_bounds(&text);
            if let ShapeKind::Text { origin, .. } = &mut text.kind {
                *origin += mid - tb.center();
            }
            let mut kids = Vec::new();
            if let Some(path) = backdrop_path(st.backdrop, mid, size) {
                let line = st.backdrop == Backdrop::Guilloche;
                let mut s = Shape::new(
                    self.engine.new_shape_id(),
                    ShapeKind::Path { path, closed: !line },
                );
                if line {
                    s.fill = Fill::None;
                    s.stroke = Some(Stroke {
                        color: st.back_color,
                        width: (size * 0.004).clamp(0.05, 0.5),
                        ..Default::default()
                    });
                } else {
                    s.fill = Fill::Solid(st.back_color);
                    s.stroke = None;
                }
                kids.push(s);
            }
            kids.push(text);
            let mut g = Shape::new(self.engine.new_shape_id(), ShapeKind::Group { children: kids });
            g.fill = Fill::None;
            g.stroke = None;
            g.name = Some(t.clone());
            items.push(g);
        }
        let id = self.engine.new_shape_id();
        let mut group = Shape::new(id, ShapeKind::Group { children: items });
        group.fill = Fill::None;
        group.stroke = None;
        group.name = Some(tr("serials.object_name"));
        self.run(Command::AddShape { layer, shape: group });
        self.select(vec![id]);
        Some(id)
    }
}

pub fn dialog(app: &mut App, ctx: &Context, st: &mut Serials, close: &mut bool) {
    let units = app.units;
    window(ctx, tr("serials.title")).show(ctx, |ui| {
        ui.set_width(340.0);
        let length = |ui: &mut egui::Ui, v: &mut f64, lo: f64, key: &str| {
            let mut u = units.from_mm(*v);
            if ui
                .add(
                    NumField::new(&mut u)
                        .range(units.from_mm(lo)..=units.from_mm(1000.0))
                        .speed(0.2)
                        .suffix(format!(" {}", units.short())),
                )
                .rich_tip(crate::ui::tips::keyed(key))
                .changed()
            {
                *v = units.to_mm(u);
            }
        };
        let whole = |ui: &mut egui::Ui, v: &mut f64, lo: f64, hi: f64, key: &str| {
            ui.add(NumField::new(v).range(lo..=hi).speed(0.2).fixed_decimals(0))
                .rich_tip(crate::ui::tips::keyed(key));
        };
        ui.horizontal(|ui| {
            ui.radio_value(&mut st.mode, SerialMode::Sequence, tr("serials.sequence"))
                .rich_tip(crate::ui::tips::keyed("serials.sequence"));
            ui.radio_value(&mut st.mode, SerialMode::Random, tr("serials.random"))
                .rich_tip(crate::ui::tips::keyed("serials.random"));
        });
        ui.add_space(4.0);
        egui::Grid::new("serials").num_columns(2).show(ui, |ui| {
            match st.mode {
                SerialMode::Sequence => {
                    for (key, v) in [
                        ("serials.start", &mut st.start),
                        ("serials.end", &mut st.end),
                        ("serials.step", &mut st.step),
                    ] {
                        ui.label(tr(key));
                        let mut x = *v as f64;
                        whole(ui, &mut x, -1e12, 1e12, key);
                        *v = x.round() as i64;
                        ui.end_row();
                    }
                    ui.label(tr("serials.digits"));
                    let mut x = f64::from(st.digits);
                    whole(ui, &mut x, 0.0, 30.0, "serials.digits");
                    st.digits = x.round().clamp(0.0, 30.0) as u32;
                    ui.end_row();
                }
                SerialMode::Random => {
                    ui.label(tr("serials.count"));
                    let mut x = f64::from(st.count);
                    whole(ui, &mut x, 1.0, MAX_ITEMS as f64, "serials.count");
                    st.count = x.round().clamp(1.0, MAX_ITEMS as f64) as u32;
                    ui.end_row();
                    ui.label(tr("serials.length"));
                    let mut x = f64::from(st.digits);
                    whole(ui, &mut x, 1.0, 64.0, "serials.length");
                    st.digits = x.round().clamp(1.0, 64.0) as u32;
                    ui.end_row();
                    ui.label(tr("serials.charset"));
                    let name = |c: Charset| match c {
                        Charset::Digits => tr("serials.charset_digits"),
                        Charset::Alphanumeric => tr("serials.charset_alnum"),
                        Charset::Hex => tr("serials.charset_hex"),
                    };
                    crate::ui::keynav::ComboBox::from_id_salt("serials_charset")
                        .selected_text(name(st.charset))
                        .show_ui(ui, |ui| {
                            for c in [Charset::Digits, Charset::Alphanumeric, Charset::Hex] {
                                ui.selectable_value(&mut st.charset, c, name(c));
                            }
                        })
                        .response
                        .rich_tip(crate::ui::tips::keyed("serials.charset"));
                    ui.end_row();
                }
            }
            ui.label(tr("serials.prefix"));
            ui.add(egui::TextEdit::singleline(&mut st.prefix).desired_width(120.0))
                .rich_tip(crate::ui::tips::keyed("serials.prefix"));
            ui.end_row();
            ui.label(tr("serials.suffix"));
            ui.add(egui::TextEdit::singleline(&mut st.suffix).desired_width(120.0))
                .rich_tip(crate::ui::tips::keyed("serials.suffix"));
            ui.end_row();
            ui.label(tr("serials.backdrop"));
            crate::ui::keynav::ComboBox::from_id_salt("serials_backdrop")
                .selected_text(tr(st.backdrop.key()))
                .show_ui(ui, |ui| {
                    for b in Backdrop::ALL {
                        ui.selectable_value(&mut st.backdrop, b, tr(b.key()));
                    }
                })
                .response
                .rich_tip(crate::ui::tips::keyed("serials.backdrop"));
            ui.end_row();
            ui.label(tr("serials.size"));
            length(ui, &mut st.size, 1.0, "serials.size");
            ui.end_row();
            ui.label(tr("serials.columns"));
            let mut x = f64::from(st.columns);
            whole(ui, &mut x, 1.0, 500.0, "serials.columns");
            st.columns = x.round().clamp(1.0, 500.0) as u32;
            ui.end_row();
            ui.label(tr("serials.gap"));
            length(ui, &mut st.gap, 0.0, "serials.gap");
            ui.end_row();
            ui.label(tr("serials.text_color"));
            crate::ui::color_picker::color_button(ui, &mut st.text_color);
            ui.end_row();
            if st.backdrop != Backdrop::None {
                ui.label(tr("serials.back_color"));
                crate::ui::color_picker::color_button(ui, &mut st.back_color);
                ui.end_row();
            }
        });
        // What the first numbers will read.
        let sample = st.texts(1);
        let mut shown: Vec<&str> = sample.iter().take(4).map(String::as_str).collect();
        if sample.len() > 4 {
            shown.push("…");
        }
        ui.add_space(4.0);
        ui.weak(crate::i18n::trf(
            "serials.preview",
            &[("n", &sample.len().to_string()), ("list", &shown.join(", "))],
        ));
        ui.add_space(6.0);
        if ok_cancel(ui, close) {
            let st = st.clone();
            app.insert_serials(&st);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_is_padded_and_can_count_down() {
        let st = Serials::default();
        let t = st.texts(1);
        assert_eq!(t.len(), 99);
        assert_eq!((t[0].as_str(), t[98].as_str()), ("01", "99"));
        let down = Serials {
            start: 10,
            end: 0,
            step: 5,
            digits: 3,
            prefix: "N".into(),
            suffix: "-A".into(),
            ..Default::default()
        };
        assert_eq!(down.texts(1), vec!["N010-A", "N005-A", "N000-A"]);
    }

    #[test]
    fn random_codes_have_their_length_and_never_repeat() {
        let st = Serials {
            mode: SerialMode::Random,
            count: 200,
            digits: 6,
            charset: Charset::Alphanumeric,
            ..Default::default()
        };
        let t = st.texts(42);
        assert_eq!(t.len(), 200);
        assert!(t.iter().all(|c| c.len() == 6 && !c.contains(['0', 'O', '1', 'I'])));
        let set: std::collections::HashSet<_> = t.iter().collect();
        assert_eq!(set.len(), 200);
        // Ten one-digit codes is all there are.
        let few = Serials {
            mode: SerialMode::Random,
            count: 50,
            digits: 1,
            ..Default::default()
        };
        assert_eq!(few.texts(7).len(), 10);
    }

    #[test]
    fn numbers_come_in_a_grid_on_their_shapes_in_one_step() {
        let mut app = App::headless();
        let st = Serials {
            end: 12,
            columns: 5,
            backdrop: Backdrop::Star,
            ..Default::default()
        };
        let id = app.insert_serials_seeded(&st, 1).expect("serials");
        let g = app.doc().find_shape(id).expect("group").clone();
        let ShapeKind::Group { children } = &g.kind else {
            panic!("{:?}", g.kind);
        };
        assert_eq!(children.len(), 12);
        let first = &children[0];
        let ShapeKind::Group { children: parts } = &first.kind else {
            panic!("{:?}", first.kind);
        };
        assert_eq!(parts.len(), 2);
        assert!(matches!(parts[1].kind, ShapeKind::Text { .. }));
        // The number sits in the middle of its star, inside it.
        let star_b = app.visual_bounds(&parts[0]);
        let text_b = app.visual_bounds(&parts[1]);
        assert!((star_b.center().x - text_b.center().x).abs() < 0.2, "{star_b:?} {text_b:?}");
        assert!(star_b.contains(text_b.origin()) && text_b.width() < star_b.width() * 0.5);
        // Five across: the sixth starts the second row.
        let a = app.visual_bounds(&children[0]);
        let f = app.visual_bounds(&children[5]);
        assert!((a.center().x - f.center().x).abs() < 0.5 && f.center().y > a.center().y + 10.0);
        // The grid is centred on the page.
        let all = app.visual_bounds(&g);
        let pc = app.page_rect().center();
        assert!((all.center().x - pc.x).abs() < 1.0, "{all:?}");
        assert_eq!(app.last_serials, st);
        app.undo();
        assert!(app.doc().find_shape(id).is_none());
    }

    #[test]
    fn a_guilloche_backdrop_is_a_fine_line_in_its_cell() {
        let p = backdrop_path(Backdrop::Guilloche, Point::new(10.0, 10.0), 20.0).expect("path");
        let b = p.bounding_box();
        assert!(b.width() <= 20.0 + 1e-6 && b.width() > 15.0, "{b:?}");
        assert!(backdrop_path(Backdrop::None, Point::ZERO, 10.0).is_none());
    }
}
