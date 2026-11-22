//! A TraceDraw that stops responding says so, and offers to report it.
//!
//! The interface's thread marks when each frame begins and ends, and the
//! steps it passes (`frame_stats::mark`); a watch on a thread of its own
//! looks every second. A frame still being built after [`HANG_AFTER`] is
//! a hang: `hang.log` in the settings folder is written with what the
//! program was doing (the version, the system, the renderer, the last
//! step the frame got through, the tool, the last command, how big the
//! drawing is, never its name or contents) and the end of `tracedraw.log`,
//! and rewritten while it lasts. A frame that comes through at last adds
//! how long it took, and the window offers to send the report. Killed
//! while stuck, the next start offers it, as it does for a `crash.log`
//! that a panic left (`startup_log.rs`): such reports went unseen.
//!
//! Nothing is sent by itself: Send Report opens a new issue on GitHub
//! with a summary filled in and the folder holding the report, for the
//! user to look at, attach and send (see docs/behavior/about-and-startup.md).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

/// How long a frame may take before the program counts as stuck.
pub const HANG_AFTER: Duration = Duration::from_secs(10);

/// Reports older than this are not offered at a start: a `crash.log`
/// left from long ago is no news.
const OFFER_WITHIN: Duration = Duration::from_secs(14 * 24 * 3600);

/// The report of a hang, in `dir`.
pub fn hang_path(dir: &Path) -> PathBuf {
    dir.join("hang.log")
}

/// Where a report offered goes, so that it is offered once: `hang.log`
/// becomes `last-hang.log`, `crash.log` `last-crash.log`.
fn offered_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    path.with_file_name(format!("last-{name}"))
}

/// The line a hang that came through ends with.
const RECOVERED: &str = "Responding again after";

/// Milliseconds since the watch started, for the atomics.
fn now_ms() -> u64 {
    static START: OnceLock<web_time::Instant> = OnceLock::new();
    START.get_or_init(web_time::Instant::now).elapsed().as_millis() as u64 + 1
}

/// When the frame being built began (`now_ms`), 0 between frames.
static FRAME_BEGAN: AtomicU64 = AtomicU64::new(0);
/// The frame a hang was written for (its `FRAME_BEGAN`), 0 for none.
static REPORTED: AtomicU64 = AtomicU64::new(0);
/// How long the hang lasted that the window should offer to report (ms),
/// 0 for none.
static CAME_THROUGH: AtomicU64 = AtomicU64::new(0);
/// The last step the frame got through.
static STEP: Mutex<&'static str> = Mutex::new("the start of the frame");
/// What the program is doing, for the report: see [`note_doing`].
static DOING: Mutex<String> = Mutex::new(String::new());
static STARTED: AtomicBool = AtomicBool::new(false);
/// The settings folder the watch writes in (set by [`start`]).
static DIR: OnceLock<PathBuf> = OnceLock::new();

/// A frame being built: begun when made, ended when dropped (every early
/// return of the frame included).
pub struct Frame(());

impl Frame {
    pub fn begin() -> Frame {
        if let Ok(mut s) = STEP.lock() {
            *s = "the start of the frame";
        }
        FRAME_BEGAN.store(now_ms(), Ordering::SeqCst);
        Frame(())
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        let began = FRAME_BEGAN.swap(0, Ordering::SeqCst);
        if began != 0 && REPORTED.load(Ordering::SeqCst) == began {
            let lasted = now_ms().saturating_sub(began);
            CAME_THROUGH.store(lasted.max(1), Ordering::SeqCst);
            if let Some(dir) = DIR.get() {
                let path = hang_path(dir);
                if let Ok(mut text) = std::fs::read_to_string(&path) {
                    text.push_str(&format!("\n{RECOVERED} {} s.\n", lasted / 1000));
                    let _ = std::fs::write(&path, text);
                }
            }
        }
    }
}

/// `ask`, a dialog of the system that holds the frame until the user
/// answers it (a file dialog): no hang however long it stays open; the
/// frame counts again from its answer.
pub fn waiting_for_user<T>(ask: impl FnOnce() -> T) -> T {
    let began = FRAME_BEGAN.swap(0, Ordering::SeqCst);
    let answer = ask();
    if began != 0 {
        FRAME_BEGAN.store(now_ms(), Ordering::SeqCst);
    }
    answer
}

/// The frame got through step `name` (`frame_stats::mark`).
pub fn mark(name: &'static str) {
    if let Ok(mut s) = STEP.lock() {
        *s = name;
    }
}

/// What the program is doing, for a report (the tool, the last command,
/// the drawing's size): asked at most once a second.
pub fn note_doing(make: impl FnOnce() -> String) {
    static LAST: AtomicU64 = AtomicU64::new(0);
    let now = now_ms();
    let last = LAST.load(Ordering::Relaxed);
    if last != 0 && now.saturating_sub(last) < 1000 {
        return;
    }
    LAST.store(now, Ordering::Relaxed);
    let text = make();
    if let Ok(mut d) = DOING.lock() {
        *d = text;
    }
}

/// Start the watch (once): a thread that looks every second.
pub fn start() {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let Some(dir) = crate::settings::config_dir() else {
        return;
    };
    let _ = DIR.set(dir.clone());
    let _ = std::thread::Builder::new()
        .name("hang watch".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_secs(1));
            look(&dir, now_ms());
        });
}

/// One look of the watch at `now`: a frame stuck past [`HANG_AFTER`] has
/// its report written, and rewritten while it lasts.
fn look(dir: &Path, now: u64) {
    let began = FRAME_BEGAN.load(Ordering::SeqCst);
    if began == 0 {
        return;
    }
    let stuck = now.saturating_sub(began);
    if stuck < HANG_AFTER.as_millis() as u64 {
        return;
    }
    let first = REPORTED.swap(began, Ordering::SeqCst) != began;
    if first {
        log::error!(
            "the interface has not responded for {} s (after {})",
            stuck / 1000,
            STEP.lock().map(|s| *s).unwrap_or("?")
        );
    }
    let _ = std::fs::create_dir_all(dir);
    let _ = std::fs::write(hang_path(dir), report(dir, stuck));
}

/// The report of a hang `stuck` ms long so far.
fn report(dir: &Path, stuck: u64) -> String {
    let step = STEP.lock().map(|s| *s).unwrap_or("?");
    let doing = DOING.lock().map(|d| d.clone()).unwrap_or_default();
    let renderer = crate::gpu::mode_in_use().map_or("unknown", |m| m.name());
    let when = crate::time_tracker::date_time(
        crate::time_tracker::unix_now(),
        &crate::time_tracker::local_zone(),
    );
    let mut text = format!(
        "TraceDraw {} stopped responding\n\
         When: {when}\n\
         Not responding for: {} s (still stuck when this was written)\n\
         Last step of the frame done: {step}\n\
         Doing: {doing}\n\
         System: {}, renderer {renderer}\n",
        env!("CARGO_PKG_VERSION"),
        stuck / 1000,
        crate::startup_log::system(),
    );
    let tail = log_tail(&crate::startup_log::log_path(dir), 40);
    if !tail.is_empty() {
        text.push_str("\nThe end of tracedraw.log:\n");
        text.push_str(&tail);
    }
    text
}

/// The last `n` lines of the file at `path`.
fn log_tail(path: &Path, n: usize) -> String {
    let Ok(text) = std::fs::read_to_string(path) else {
        return String::new();
    };
    let lines: Vec<&str> = text.lines().collect();
    let from = lines.len().saturating_sub(n);
    let mut out = lines[from..].join("\n");
    out.push('\n');
    out
}

/// A hang that came through this run, once: how long it lasted.
pub fn take_came_through() -> Option<Duration> {
    let ms = CAME_THROUGH.swap(0, Ordering::SeqCst);
    (ms != 0).then(|| Duration::from_millis(ms))
}

/// Once a frame, from the window: what the program is doing noted for a
/// report, and a report offered when no other dialog is up (the one the
/// last run left, once, and one for a hang this run came through).
pub fn poll(app: &mut crate::app::App) {
    note_doing(|| doing(app));
    static LOOKED: AtomicBool = AtomicBool::new(false);
    static WAITING: Mutex<Option<Offer>> = Mutex::new(None);
    let Some(dir) = DIR.get() else {
        return;
    };
    if let Some(lasted) = take_came_through() {
        if let Some(offer) = hang_this_run(dir, lasted) {
            if let Ok(mut w) = WAITING.lock() {
                *w = Some(offer);
            }
        }
    }
    if !matches!(app.dialog, crate::ui::dialogs::Dialog::None) {
        return;
    }
    if !LOOKED.swap(true, Ordering::SeqCst) {
        if let Some(offer) = left_by_last_run(dir) {
            app.dialog = crate::ui::dialogs::Dialog::ProblemReport(offer);
            return;
        }
    }
    if let Some(offer) = WAITING.lock().ok().and_then(|mut w| w.take()) {
        app.dialog = crate::ui::dialogs::Dialog::ProblemReport(offer);
    }
}

/// What `app` is doing, for a report: the tool, the last command, the
/// open drawings and the size of the active one. Never names or
/// contents.
fn doing(app: &crate::app::App) -> String {
    let doc = app.doc();
    let objects: usize = doc
        .pages
        .iter()
        .flat_map(|p| &p.layers)
        .map(|l| l.shapes.len())
        .sum();
    format!(
        "tool {:?}, last command {}, {} drawing(s) open, the active one {} page(s) and {} object(s) on its layers, {} picture(s), dialog {}",
        app.tool,
        app.engine.undo_label().unwrap_or("none"),
        app.docs.len().max(1),
        doc.pages.len(),
        objects,
        doc.pages
            .iter()
            .flat_map(|p| &p.layers)
            .flat_map(|l| &l.shapes)
            .filter(|s| matches!(s.kind, tracedraw_core::ShapeKind::Bitmap { .. }))
            .count(),
        if matches!(app.dialog, crate::ui::dialogs::Dialog::None) {
            "none"
        } else {
            "one open"
        },
    )
}

/// What a report offered is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    /// The program stopped responding this run and came through after
    /// this many seconds.
    HangNow(u64),
    /// The last run stopped responding and was ended.
    HangLast,
    /// The last run ended with an error (a panic).
    CrashLast,
}

/// A report offered to be sent.
#[derive(Debug, Clone, PartialEq)]
pub struct Offer {
    pub problem: Problem,
    /// The report, moved aside so that it is offered once.
    pub path: PathBuf,
    /// Its first lines, for the issue.
    pub summary: String,
}

/// The report the last run left in `dir`, if it is news (written within
/// [`OFFER_WITHIN`], and for a hang, one it did not come through: that
/// one was offered then). Moved to `last-…` so that it is offered once.
/// A crash goes before a hang.
pub fn left_by_last_run(dir: &Path) -> Option<Offer> {
    let fresh = |p: &Path| {
        std::fs::metadata(p)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age <= OFFER_WITHIN)
    };
    let crash = crate::startup_log::crash_path(dir);
    let hang = hang_path(dir);
    let found = if crash.is_file() && fresh(&crash) {
        Some((Problem::CrashLast, crash))
    } else if hang.is_file() && fresh(&hang) {
        let text = std::fs::read_to_string(&hang).unwrap_or_default();
        (!text.contains(RECOVERED)).then_some((Problem::HangLast, hang))
    } else {
        None
    };
    // Stale or already offered reports are moved aside too.
    for p in [crate::startup_log::crash_path(dir), hang_path(dir)] {
        if p.is_file() && found.as_ref().is_none_or(|(_, f)| *f != p) {
            let _ = std::fs::rename(&p, offered_path(&p));
        }
    }
    let (problem, path) = found?;
    offer(problem, &path)
}

/// The report of the hang this run came through, offered once.
pub fn hang_this_run(dir: &Path, lasted: Duration) -> Option<Offer> {
    offer(Problem::HangNow(lasted.as_secs()), &hang_path(dir))
}

fn offer(problem: Problem, path: &Path) -> Option<Offer> {
    let text = std::fs::read_to_string(path).ok()?;
    let moved = offered_path(path);
    let path = if std::fs::rename(path, &moved).is_ok() {
        moved
    } else {
        path.to_path_buf()
    };
    let summary: String = text
        .lines()
        .take_while(|l| !l.trim().is_empty())
        .take(12)
        .collect::<Vec<_>>()
        .join("\n");
    Some(Offer {
        problem,
        path,
        summary,
    })
}

/// The address of a new issue for `offer`, the bug report form with its
/// fields filled in: the summary (never the log, which the user attaches
/// after looking at it), the version and the system.
pub fn issue_url(offer: &Offer) -> String {
    let title = match offer.problem {
        Problem::HangNow(_) | Problem::HangLast => "TraceDraw stopped responding",
        Problem::CrashLast => "TraceDraw closed with an error",
    };
    let file = offer
        .path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let what = format!(
        "{title}. What I was doing:\n\n\n```\n{}\n```",
        offer.summary.chars().take(3000).collect::<String>()
    );
    let files = format!("The report {file} is attached.");
    let platform = if cfg!(windows) {
        "Windows"
    } else if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(target_os = "android") {
        "Android"
    } else {
        "Linux"
    };
    format!(
        "https://github.com/tracedraw/tracedraw/issues/new?template=bug_report.yml&labels=bug&title={}&what={}&platform={}&version={}&files={}",
        encode(title),
        encode(&what),
        encode(platform),
        encode(env!("CARGO_PKG_VERSION")),
        encode(&files)
    )
}

/// `text` for a query string: every byte but the unreserved ones escaped.
fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tests of the frame share its atomics: one at a time.
    static SERIAL: Mutex<()> = Mutex::new(());

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("td_hang_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("dir");
        d
    }

    /// A frame stuck past HANG_AFTER writes the report, with the step it
    /// got through and what the program was doing; once it comes
    /// through, the report says how long it took and the window is told
    /// once. A frame that ends in time writes nothing.
    #[test]
    fn a_stuck_frame_is_reported_and_its_end_told() {
        let _one = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let d = dir("stuck");
        std::fs::write(crate::startup_log::log_path(&d), "a\nb\nlast line\n").expect("log");
        note_doing(|| "tool Pick, last command Move".into());
        {
            let _quick = Frame::begin();
            look(&d, now_ms());
        }
        assert!(!hang_path(&d).exists(), "a frame in time");
        let frame = Frame::begin();
        mark("the menus");
        let began = FRAME_BEGAN.load(Ordering::SeqCst);
        look(&d, began + HANG_AFTER.as_millis() as u64 + 2000);
        let text = std::fs::read_to_string(hang_path(&d)).expect("report");
        assert!(text.contains("stopped responding"), "{text}");
        assert!(text.contains("Not responding for: 12 s"), "{text}");
        assert!(text.contains("Last step of the frame done: the menus"), "{text}");
        assert!(text.contains("tool Pick, last command Move"), "{text}");
        assert!(text.contains("last line"), "{text}");
        assert!(!text.contains(RECOVERED));
        // Coming through: told once. The report is written where the
        // settings are, so the test looks at the flag alone.
        drop(frame);
        assert!(take_came_through().is_some());
        assert!(take_came_through().is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A file dialog held open is the user's time, not a hang: the frame
    /// counts again from its answer.
    #[test]
    fn a_system_dialog_held_open_is_no_hang() {
        let _one = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let d = dir("dialog");
        let _frame = Frame::begin();
        let answer = waiting_for_user(|| {
            look(&d, now_ms() + HANG_AFTER.as_millis() as u64 * 6);
            7
        });
        assert_eq!(answer, 7);
        assert!(!hang_path(&d).exists());
        assert_ne!(FRAME_BEGAN.load(Ordering::SeqCst), 0, "counts again");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// At a start, a report the last run left is offered once: a crash
    /// before a hang, a hang the run came through not again, and the
    /// report moved aside to last-… so the next start says nothing.
    #[test]
    fn a_report_the_last_run_left_is_offered_once() {
        let d = dir("left");
        assert_eq!(left_by_last_run(&d), None);
        std::fs::write(hang_path(&d), "TraceDraw 0.50.3 stopped responding\nWhen: now\n\nlog")
            .expect("hang");
        let offer = left_by_last_run(&d).expect("offered");
        assert_eq!(offer.problem, Problem::HangLast);
        assert_eq!(offer.path, d.join("last-hang.log"));
        assert!(offer.summary.contains("When: now") && !offer.summary.contains("log"));
        assert!(offer.path.exists() && !hang_path(&d).exists());
        assert_eq!(left_by_last_run(&d), None, "once");
        std::fs::write(crate::startup_log::crash_path(&d), "panicked at x").expect("crash");
        std::fs::write(hang_path(&d), "stopped responding").expect("hang");
        let offer = left_by_last_run(&d).expect("offered");
        assert_eq!(offer.problem, Problem::CrashLast);
        assert!(!hang_path(&d).exists(), "the other one is moved aside too");
        std::fs::write(hang_path(&d), format!("stopped responding\n{RECOVERED} 12 s.\n"))
            .expect("hang");
        assert_eq!(left_by_last_run(&d), None, "offered when it came through");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The issue is the bug report form with the summary, the system and
    /// the version filled in, escaped for the address; the log is not in
    /// it.
    #[test]
    fn the_issue_is_filled_in_with_the_summary() {
        let offer = Offer {
            problem: Problem::HangNow(25),
            path: PathBuf::from("/x/last-hang.log"),
            summary: "TraceDraw stopped responding\nDoing: tool Pick & more".into(),
        };
        let url = issue_url(&offer);
        assert!(url.starts_with("https://github.com/tracedraw/tracedraw/issues/new?template=bug_report.yml"));
        assert!(url.contains("&title=TraceDraw%20stopped%20responding&"));
        assert!(url.contains("tool%20Pick%20%26%20more"), "{url}");
        assert!(url.contains(&format!("&version={}", env!("CARGO_PKG_VERSION"))));
        assert!(url.contains("last-hang.log%20is%20attached"));
        assert!(!url.contains(' '));
    }
}
