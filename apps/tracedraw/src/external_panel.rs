//! External panel (docs/behavior/external-panel.md): a window of another
//! program shown docked as a panel, asked for through the open window's
//! MCP link (`dock_window`). Windows only: the window becomes a child of
//! TraceDraw's window and follows the panel's page every frame.

use crate::app::{App, DockerTab};

/// The panel's page this frame: (frame number, viewport, rect in points).
fn page_id() -> egui::Id {
    egui::Id::new("external_panel_page")
}

/// The panel's page, drawn by the docker: where the window goes. In a
/// panel window of its own the page stays empty.
// shortcut: only the main window holds the panel; a window of its own
// would need that viewport's handle as the parent.
pub fn page(ui: &egui::Ui, rect: egui::Rect) {
    let nr = ui.ctx().cumulative_frame_nr();
    let vp = ui.ctx().viewport_id();
    ui.ctx()
        .data_mut(|d| d.insert_temp(page_id(), (nr, vp, rect)));
}

/// The name the docker shows: the one the program gave.
pub fn title() -> String {
    imp::title()
}

/// Dock the top-level window `hwnd` of another program as the panel
/// `title`, replacing the one docked before.
pub fn dock(app: &mut App, hwnd: i64, title: &str) -> Result<String, String> {
    imp::dock(hwnd, title)?;
    app.open_docker(DockerTab::External);
    Ok(format!("docked {hwnd}"))
}

/// Close the panel; its window is handed back and asked to close.
pub fn undock(app: &mut App) -> String {
    app.close_docker(DockerTab::External);
    "undocked".into()
}

/// Once a frame, after the interface: the window follows the panel's
/// page, hides while the page is not on screen in the main window, and
/// is handed back when the panel closes or the window went away.
pub fn sync(app: &mut App, ctx: &egui::Context, frame: &eframe::Frame) {
    let open = app.docker_place(DockerTab::External).is_some();
    let page = ctx
        .data(|d| d.get_temp::<(u64, egui::ViewportId, egui::Rect)>(page_id()))
        .filter(|(nr, vp, _)| *nr == ctx.cumulative_frame_nr() && *vp == egui::ViewportId::ROOT)
        .map(|(_, _, r)| r * ctx.pixels_per_point());
    match imp::sync(open, page, frame) {
        imp::State::None => {
            // A panel kept from the last session has no window.
            if open {
                app.close_docker(DockerTab::External);
            }
        }
        imp::State::Gone => app.close_docker(DockerTab::External),
        // A window closed by its program is noticed within a second
        // while nothing else asks for a frame.
        imp::State::Docked => ctx.request_repaint_after(std::time::Duration::from_secs(1)),
    }
}

/// TraceDraw closes: the window is handed back before its parent goes.
pub fn release() {
    imp::release();
}

/// The window's messages before the windowing layer reads them: a click
/// on TraceDraw takes the keyboard back from the panel.
#[cfg(windows)]
pub fn message(m: &windows_sys::Win32::UI::WindowsAndMessaging::MSG) {
    imp::message(m);
}

#[cfg(windows)]
mod imp {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::sync::Mutex;
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetFocus, SetFocus};
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    pub enum State {
        None,
        Gone,
        Docked,
    }

    struct Panel {
        hwnd: HWND,
        title: String,
        /// The window's style before it was docked, given back with it.
        style: isize,
        /// TraceDraw's window, once the panel is its child.
        parent: Option<HWND>,
        /// Where it was last put, in pixels, or `None` while hidden.
        shown: Option<egui::Rect>,
    }

    static PANEL: Mutex<Option<Panel>> = Mutex::new(None);

    fn panel() -> std::sync::MutexGuard<'static, Option<Panel>> {
        PANEL.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn title() -> String {
        panel()
            .as_ref()
            .map(|p| p.title.clone())
            .unwrap_or_default()
    }

    pub fn dock(hwnd: i64, title: &str) -> Result<(), String> {
        let hwnd = hwnd as HWND;
        // SAFETY: plain calls on a window handle; a stale one fails them.
        unsafe {
            if IsWindow(hwnd) == 0 {
                return Err(format!("no such window: {hwnd}"));
            }
            if GetAncestor(hwnd, GA_ROOT) != hwnd {
                return Err("only a top-level window can be docked".into());
            }
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid == std::process::id() {
                return Err("a window of TraceDraw cannot be docked".into());
            }
        }
        let mut p = panel();
        if p.as_ref().is_some_and(|p| p.hwnd == hwnd) {
            if let Some(p) = p.as_mut() {
                p.title = title.to_string();
            }
            return Ok(());
        }
        if let Some(old) = p.take() {
            give_back(&old);
        }
        // SAFETY: as above.
        let style = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) };
        *p = Some(Panel {
            hwnd,
            title: title.to_string(),
            style,
            parent: None,
            shown: None,
        });
        Ok(())
    }

    pub fn sync(open: bool, page: Option<egui::Rect>, frame: &eframe::Frame) -> State {
        let mut guard = panel();
        let Some(p) = guard.as_mut() else {
            return State::None;
        };
        // SAFETY: as above.
        if unsafe { IsWindow(p.hwnd) } == 0 {
            *guard = None;
            return State::Gone;
        }
        if !open {
            if let Some(old) = guard.take() {
                give_back(&old);
            }
            return State::None;
        }
        if p.parent.is_none() {
            let Some(parent) = main_window(frame) else {
                return State::Docked;
            };
            let child =
                (p.style & !((WS_POPUP | WS_CAPTION | WS_THICKFRAME) as isize)) | WS_CHILD as isize;
            // SAFETY: as above; a child style before the new parent, as
            // SetParent asks.
            unsafe {
                ShowWindow(p.hwnd, SW_HIDE);
                SetWindowLongPtrW(p.hwnd, GWL_STYLE, child);
                SetParent(p.hwnd, parent);
            }
            p.parent = Some(parent);
            p.shown = None;
        }
        match page {
            Some(r) if p.shown != Some(r) => {
                // SAFETY: as above.
                unsafe {
                    SetWindowPos(
                        p.hwnd,
                        HWND_TOP,
                        r.left().round() as i32,
                        r.top().round() as i32,
                        r.width().round().max(1.0) as i32,
                        r.height().round().max(1.0) as i32,
                        SWP_NOACTIVATE | SWP_SHOWWINDOW | SWP_FRAMECHANGED,
                    );
                }
                p.shown = Some(r);
            }
            None if p.shown.is_some() => {
                // SAFETY: as above.
                unsafe { ShowWindow(p.hwnd, SW_HIDE) };
                p.shown = None;
            }
            _ => {}
        }
        State::Docked
    }

    pub fn release() {
        if let Some(p) = panel().take() {
            give_back(&p);
        }
    }

    pub fn message(m: &MSG) {
        if !matches!(m.message, WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN) {
            return;
        }
        let parent = panel().as_ref().and_then(|p| p.parent);
        // SAFETY: as above.
        unsafe {
            if parent == Some(m.hwnd) && GetFocus() != m.hwnd {
                SetFocus(m.hwnd);
            }
        }
    }

    /// The window goes back to the desktop as it was, hidden, and is
    /// asked to close: its program decides what that means.
    fn give_back(p: &Panel) {
        // SAFETY: as above; a window that went away fails these calls.
        unsafe {
            if IsWindow(p.hwnd) == 0 {
                return;
            }
            ShowWindow(p.hwnd, SW_HIDE);
            if p.parent.is_some() {
                SetParent(p.hwnd, 0);
                SetWindowLongPtrW(p.hwnd, GWL_STYLE, p.style & !(WS_VISIBLE as isize));
            }
            PostMessageW(p.hwnd, WM_CLOSE, 0, 0);
        }
    }

    fn main_window(frame: &eframe::Frame) -> Option<HWND> {
        match frame.window_handle().ok()?.as_raw() {
            RawWindowHandle::Win32(w) => Some(w.hwnd.get() as HWND),
            _ => None,
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub enum State {
        None,
        #[allow(dead_code)]
        Gone,
        #[allow(dead_code)]
        Docked,
    }

    pub fn title() -> String {
        String::new()
    }

    pub fn dock(_hwnd: i64, _title: &str) -> Result<(), String> {
        Err("external panels need Windows".into())
    }

    pub fn sync(_open: bool, _page: Option<egui::Rect>, _frame: &eframe::Frame) -> State {
        State::None
    }

    pub fn release() {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Handles that are no window of another program are refused and
    /// open no panel.
    #[test]
    fn strange_handles_are_refused() {
        let mut app = App::headless();
        assert!(dock(&mut app, 0, "Panel").is_err());
        assert_eq!(app.docker_place(DockerTab::External), None);
        #[cfg(windows)]
        {
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                CreateWindowExW, DestroyWindow, WS_OVERLAPPED,
            };
            // SAFETY: a plain window of this process, destroyed below.
            let own = unsafe {
                CreateWindowExW(
                    0,
                    windows_sys::w!("STATIC"),
                    windows_sys::w!("own"),
                    WS_OVERLAPPED,
                    0,
                    0,
                    10,
                    10,
                    0,
                    0,
                    0,
                    std::ptr::null(),
                )
            };
            assert_ne!(own, 0);
            let r = dock(&mut app, own as i64, "Panel");
            // SAFETY: the window made above.
            unsafe { DestroyWindow(own) };
            assert_eq!(r, Err("a window of TraceDraw cannot be docked".to_string()));
            assert_eq!(app.docker_place(DockerTab::External), None);
        }
    }

    /// `undock_window` takes the panel out of its column.
    #[test]
    fn undocking_closes_the_panel() {
        let mut app = App::headless();
        app.open_docker(DockerTab::External);
        assert!(app.docker_place(DockerTab::External).is_some());
        undock(&mut app);
        assert_eq!(app.docker_place(DockerTab::External), None);
    }
}
