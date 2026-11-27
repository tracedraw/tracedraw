//! The preview handler of the Explorer's Preview Pane (#4455): a child
//! window of the pane that shows every page of the drawing one under
//! another at the pane's width, white paper on grey, scrolled with the
//! scroll bar, the mouse wheel or the arrow, Page Up/Down, Home and End
//! keys. Pages are drawn when they first come into view and again only
//! when the pane's width changes.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::sync::atomic::Ordering;
use std::sync::OnceLock;

use tracedraw_core::Document;
use windows::core::{implement, w, Interface, Ref, BOOL, GUID, PCWSTR};
use windows::Win32::Foundation::{
    COLORREF, E_FAIL, E_NOINTERFACE, E_NOTIMPL, HINSTANCE, HMODULE, HWND, LPARAM, LRESULT, RECT,
    S_FALSE, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateSolidBrush, DeleteDC,
    DeleteObject, EndPaint, FillRect, FrameRect, InvalidateRect, SelectObject, SetDIBitsToDevice,
    BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBRUSH, PAINTSTRUCT, SRCCOPY,
};
use windows::Win32::System::Com::IStream;
use windows::Win32::System::LibraryLoader::{
    GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
    GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
};
use windows::Win32::System::Ole::{
    IObjectWithSite, IObjectWithSite_Impl, IOleWindow, IOleWindow_Impl,
};
use windows::Win32::UI::Controls::SetScrollInfo;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetFocus, SetFocus, VK_DOWN, VK_END, VK_HOME, VK_NEXT, VK_PRIOR, VK_UP,
};
use windows::Win32::UI::Shell::PropertiesSystem::{
    IInitializeWithStream, IInitializeWithStream_Impl,
};
use windows::Win32::UI::Shell::{IPreviewHandler, IPreviewHandlerFrame, IPreviewHandler_Impl};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetScrollInfo,
    GetWindowLongPtrW, IsWindow, LoadCursorW, RegisterClassW, SetParent, SetWindowLongPtrW,
    SetWindowPos, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, GWLP_USERDATA, IDC_ARROW, MSG, SB_BOTTOM,
    SB_LINEDOWN, SB_LINEUP, SB_PAGEDOWN, SB_PAGEUP, SB_THUMBPOSITION, SB_THUMBTRACK, SB_TOP,
    SB_VERT, SCROLLINFO, SIF_ALL, SIF_DISABLENOSCROLL, SIF_PAGE, SIF_POS, SIF_RANGE,
    SWP_NOACTIVATE, SWP_NOZORDER, WINDOW_EX_STYLE, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDOWN,
    WM_MOUSEWHEEL, WM_NCCREATE, WM_NCDESTROY, WM_PAINT, WM_SIZE, WM_VSCROLL, WNDCLASSW, WS_CHILD,
    WS_CLIPSIBLINGS, WS_VISIBLE, WS_VSCROLL,
};

use super::com::{read_stream, FONTS, LIVE};
use crate::Slot;

/// The grey round the pages, and the line round each.
const DESK: COLORREF = COLORREF(0x00E6_E6E6);
const EDGE: COLORREF = COLORREF(0x00A0_A0A0);
/// Pixels one arrow key or one notch of the wheel moves.
const STEP: i32 = 48;

#[implement(IPreviewHandler, IInitializeWithStream, IObjectWithSite, IOleWindow)]
pub(crate) struct Previewer {
    data: RefCell<Option<Vec<u8>>>,
    parent: Cell<HWND>,
    rect: Cell<RECT>,
    hwnd: Cell<HWND>,
    site: RefCell<Option<windows::core::IUnknown>>,
}

impl Previewer {
    pub(crate) fn new() -> Self {
        LIVE.fetch_add(1, Ordering::SeqCst);
        Previewer {
            data: RefCell::new(None),
            parent: Cell::new(HWND::default()),
            rect: Cell::new(RECT::default()),
            hwnd: Cell::new(HWND::default()),
            site: RefCell::new(None),
        }
    }

    fn window(&self) -> Option<HWND> {
        let h = self.hwnd.get();
        (!h.is_invalid() && unsafe { IsWindow(Some(h)) }.as_bool()).then_some(h)
    }

    fn place(&self) {
        if let Some(h) = self.window() {
            let r = self.rect.get();
            unsafe {
                let _ = SetWindowPos(
                    h,
                    None,
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
        }
    }

    fn close(&self) {
        if let Some(h) = self.window() {
            unsafe {
                let _ = DestroyWindow(h);
            }
        }
        self.hwnd.set(HWND::default());
    }
}

impl Drop for Previewer {
    fn drop(&mut self) {
        self.close();
        LIVE.fetch_sub(1, Ordering::SeqCst);
    }
}

impl IInitializeWithStream_Impl for Previewer_Impl {
    fn Initialize(&self, pstream: Ref<'_, IStream>, _grfmode: u32) -> windows::core::Result<()> {
        *self.data.borrow_mut() = Some(read_stream(pstream.ok()?)?);
        Ok(())
    }
}

impl IPreviewHandler_Impl for Previewer_Impl {
    fn SetWindow(&self, hwnd: HWND, prc: *const RECT) -> windows::core::Result<()> {
        self.parent.set(hwnd);
        if !prc.is_null() {
            self.rect.set(unsafe { *prc });
        }
        if let Some(h) = self.window() {
            unsafe {
                let _ = SetParent(h, Some(hwnd));
            }
            self.place();
        }
        Ok(())
    }

    fn SetRect(&self, prc: *const RECT) -> windows::core::Result<()> {
        if prc.is_null() {
            return Err(windows::Win32::Foundation::E_INVALIDARG.into());
        }
        self.rect.set(unsafe { *prc });
        self.place();
        Ok(())
    }

    fn DoPreview(&self) -> windows::core::Result<()> {
        if self.window().is_some() {
            return Ok(());
        }
        FONTS.call_once(tracedraw_text::install);
        let doc = {
            let data = self.data.borrow();
            let bytes = data.as_deref().ok_or(windows::core::Error::from(E_FAIL))?;
            std::panic::catch_unwind(|| crate::preview_document(bytes))
                .ok()
                .flatten()
                .ok_or(windows::core::Error::from(E_FAIL))?
        };
        // The file's bytes are not needed once the drawing is read.
        *self.data.borrow_mut() = None;
        let pane = Box::new(Pane::new(doc));
        let instance = module();
        let class = window_class(instance)?;
        let r = self.rect.get();
        let raw = Box::into_raw(pane);
        TAKEN.with(|t| t.set(false));
        let made = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class,
                w!(""),
                WS_CHILD | WS_VISIBLE | WS_VSCROLL | WS_CLIPSIBLINGS,
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                Some(self.parent.get()),
                None,
                Some(instance),
                Some(raw as *const c_void),
            )
        };
        match made {
            Ok(h) => {
                self.hwnd.set(h);
                Ok(())
            }
            Err(e) => {
                // WM_NCDESTROY frees the pane once the window took it;
                // when no window got as far as that it is still ours.
                if !TAKEN.with(Cell::get) {
                    drop(unsafe { Box::from_raw(raw) });
                }
                Err(e)
            }
        }
    }

    fn Unload(&self) -> windows::core::Result<()> {
        self.close();
        *self.data.borrow_mut() = None;
        Ok(())
    }

    fn SetFocus(&self) -> windows::core::Result<()> {
        if let Some(h) = self.window() {
            unsafe {
                let _ = SetFocus(Some(h));
            }
        }
        Ok(())
    }

    fn QueryFocus(&self) -> windows::core::Result<HWND> {
        let h = unsafe { GetFocus() };
        if h.is_invalid() {
            Err(E_FAIL.into())
        } else {
            Ok(h)
        }
    }

    fn TranslateAccelerator(&self, pmsg: *const MSG) -> windows::core::Result<()> {
        // The Explorer's own keys (F5, Alt+arrows…) go back to it.
        let frame = self
            .site
            .borrow()
            .as_ref()
            .and_then(|s| s.cast::<IPreviewHandlerFrame>().ok());
        match frame {
            Some(f) => unsafe { f.TranslateAccelerator(pmsg) },
            None => Err(windows::core::Error::from_hresult(S_FALSE)),
        }
    }
}

impl IObjectWithSite_Impl for Previewer_Impl {
    fn SetSite(&self, punksite: Ref<'_, windows::core::IUnknown>) -> windows::core::Result<()> {
        *self.site.borrow_mut() = punksite.ok().ok().cloned();
        Ok(())
    }

    fn GetSite(&self, riid: *const GUID, ppvsite: *mut *mut c_void) -> windows::core::Result<()> {
        if ppvsite.is_null() || riid.is_null() {
            return Err(windows::Win32::Foundation::E_POINTER.into());
        }
        unsafe { *ppvsite = std::ptr::null_mut() };
        match self.site.borrow().as_ref() {
            Some(s) => unsafe { s.query(&*riid, ppvsite).ok() },
            None => Err(E_NOINTERFACE.into()),
        }
    }
}

impl IOleWindow_Impl for Previewer_Impl {
    fn GetWindow(&self) -> windows::core::Result<HWND> {
        self.window().ok_or(windows::core::Error::from(E_FAIL))
    }

    fn ContextSensitiveHelp(&self, _fentermode: BOOL) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }
}

#[allow(dead_code)]
fn _assert_interfaces() {
    let _ = IPreviewHandler::IID;
    let _ = IObjectWithSite::IID;
    let _ = IOleWindow::IID;
}

/// This DLL, which the window class belongs to.
fn module() -> HINSTANCE {
    let mut m = HMODULE::default();
    unsafe {
        let _ = GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            PCWSTR(window_proc as *const u16),
            &mut m,
        );
    }
    HINSTANCE(m.0)
}

/// The pane's window class, registered once in the process.
fn window_class(instance: HINSTANCE) -> windows::core::Result<PCWSTR> {
    static DONE: OnceLock<bool> = OnceLock::new();
    let name = w!("TraceDrawPreviewPane");
    let ok = *DONE.get_or_init(|| {
        let class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            hCursor: unsafe { LoadCursorW(None, IDC_ARROW) }.unwrap_or_default(),
            lpszClassName: name,
            ..Default::default()
        };
        unsafe { RegisterClassW(&class) != 0 }
    });
    if ok {
        Ok(name)
    } else {
        Err(E_FAIL.into())
    }
}

thread_local! {
    /// Whether the window being made took its pane (WM_NCCREATE).
    static TAKEN: Cell<bool> = const { Cell::new(false) };
}

/// What the pane's window shows.
struct Pane {
    doc: Document,
    sizes: Vec<(f64, f64)>,
    /// The width the pages were laid out and drawn for.
    width: i32,
    slots: Vec<Slot>,
    total: i32,
    scroll: i32,
    /// Each page as drawn for `width` (BGRA), or `None` while not drawn;
    /// an empty picture for one that could not be drawn.
    drawn: Vec<Option<(u32, u32, Vec<u8>)>>,
}

impl Pane {
    fn new(doc: Document) -> Self {
        let sizes = crate::page_sizes(&doc);
        Pane {
            drawn: vec![None; sizes.len()],
            doc,
            sizes,
            width: 0,
            slots: Vec::new(),
            total: 0,
            scroll: 0,
        }
    }

    fn lay_out(&mut self, width: i32) {
        if width != self.width {
            self.width = width;
            let (slots, total) = crate::preview_layout(&self.sizes, width);
            self.slots = slots;
            self.total = total;
            self.drawn.iter_mut().for_each(|d| *d = None);
        }
    }

    fn scroll_to(&mut self, pos: i32, height: i32) {
        self.scroll = pos.clamp(0, (self.total - height).max(0));
    }

    fn page(&mut self, i: usize) -> Option<&(u32, u32, Vec<u8>)> {
        if self.drawn[i].is_none() {
            let w = self.slots[i].w.max(1) as u32;
            let doc = &self.doc;
            let pic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::page_bgra(doc, i, w)
            }))
            .ok()
            .flatten()
            .unwrap_or((0, 0, Vec::new()));
            self.drawn[i] = Some(pic);
        }
        self.drawn[i].as_ref().filter(|p| p.0 > 0 && p.1 > 0)
    }
}

fn pane_of(hwnd: HWND) -> Option<&'static mut Pane> {
    let p = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut Pane;
    unsafe { p.as_mut() }
}

fn client(hwnd: HWND) -> RECT {
    let mut r = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut r);
    }
    r
}

/// The scroll bar for the pane's place and the window's height.
fn update_bar(hwnd: HWND, pane: &Pane, height: i32) {
    let info = SCROLLINFO {
        cbSize: std::mem::size_of::<SCROLLINFO>() as u32,
        fMask: SIF_RANGE | SIF_PAGE | SIF_POS | SIF_DISABLENOSCROLL,
        nMin: 0,
        nMax: (pane.total - 1).max(0),
        nPage: height.max(0) as u32,
        nPos: pane.scroll,
        nTrackPos: 0,
    };
    unsafe {
        SetScrollInfo(hwnd, SB_VERT, &info, true);
    }
}

fn scroll_by(hwnd: HWND, pane: &mut Pane, to: i32) {
    let h = client(hwnd).bottom;
    let before = pane.scroll;
    pane.scroll_to(to, h);
    if pane.scroll != before {
        update_bar(hwnd, pane, h);
        unsafe {
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    }
}

fn paint(hwnd: HWND, pane: &mut Pane) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = unsafe { BeginPaint(hwnd, &mut ps) };
    let r = client(hwnd);
    let (cw, ch) = (r.right.max(1), r.bottom.max(1));
    pane.lay_out(cw);
    unsafe {
        // Drawn off screen first, so a scroll does not flicker.
        let mem = CreateCompatibleDC(Some(hdc));
        let bmp = CreateCompatibleBitmap(hdc, cw, ch);
        let old = SelectObject(mem, bmp.into());
        let desk = CreateSolidBrush(DESK);
        let edge = CreateSolidBrush(EDGE);
        let white = CreateSolidBrush(COLORREF(0x00FF_FFFF));
        FillRect(
            mem,
            &RECT {
                left: 0,
                top: 0,
                right: cw,
                bottom: ch,
            },
            desk,
        );
        for i in 0..pane.slots.len() {
            let s = pane.slots[i];
            let top = s.y - pane.scroll;
            if top + s.h < 0 || top > ch {
                continue;
            }
            let frame = RECT {
                left: s.x - 1,
                top: top - 1,
                right: s.x + s.w + 1,
                bottom: top + s.h + 1,
            };
            FrameRect(mem, &frame, edge);
            let paper = RECT {
                left: s.x,
                top,
                right: s.x + s.w,
                bottom: top + s.h,
            };
            FillRect(mem, &paper, white);
            if let Some((w, h, px)) = pane.page(i) {
                let info = BITMAPINFO {
                    bmiHeader: BITMAPINFOHEADER {
                        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                        biWidth: *w as i32,
                        biHeight: -(*h as i32),
                        biPlanes: 1,
                        biBitCount: 32,
                        biCompression: BI_RGB.0,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                let (dw, dh) = ((*w).min(s.w as u32), (*h).min(s.h as u32));
                SetDIBitsToDevice(
                    mem,
                    s.x,
                    top,
                    dw,
                    dh,
                    0,
                    0,
                    0,
                    *h,
                    px.as_ptr().cast(),
                    &info,
                    DIB_RGB_COLORS,
                );
            }
        }
        let _ = BitBlt(hdc, 0, 0, cw, ch, Some(mem), 0, 0, SRCCOPY);
        for b in [desk, edge, white] {
            let _ = DeleteObject(HBRUSH(b.0).into());
        }
        SelectObject(mem, old);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(mem);
        let _ = EndPaint(hwnd, &ps);
    }
}

extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            let cs = lparam.0 as *const CREATESTRUCTW;
            if let Some(cs) = unsafe { cs.as_ref() } {
                unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize) };
                TAKEN.with(|t| t.set(true));
            }
        }
        WM_NCDESTROY => {
            let p = unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) } as *mut Pane;
            if !p.is_null() {
                drop(unsafe { Box::from_raw(p) });
            }
        }
        WM_ERASEBKGND => return LRESULT(1),
        WM_PAINT => {
            if let Some(pane) = pane_of(hwnd) {
                paint(hwnd, pane);
                return LRESULT(0);
            }
        }
        WM_SIZE => {
            if let Some(pane) = pane_of(hwnd) {
                let r = client(hwnd);
                pane.lay_out(r.right.max(1));
                pane.scroll_to(pane.scroll, r.bottom);
                update_bar(hwnd, pane, r.bottom);
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                return LRESULT(0);
            }
        }
        WM_VSCROLL => {
            if let Some(pane) = pane_of(hwnd) {
                let page = client(hwnd).bottom;
                let code = (wparam.0 & 0xFFFF) as i32;
                let to = match code {
                    c if c == SB_LINEUP.0 => pane.scroll - STEP,
                    c if c == SB_LINEDOWN.0 => pane.scroll + STEP,
                    c if c == SB_PAGEUP.0 => pane.scroll - page,
                    c if c == SB_PAGEDOWN.0 => pane.scroll + page,
                    c if c == SB_TOP.0 => 0,
                    c if c == SB_BOTTOM.0 => pane.total,
                    c if c == SB_THUMBTRACK.0 || c == SB_THUMBPOSITION.0 => {
                        let mut info = SCROLLINFO {
                            cbSize: std::mem::size_of::<SCROLLINFO>() as u32,
                            fMask: SIF_ALL,
                            ..Default::default()
                        };
                        unsafe {
                            let _ = GetScrollInfo(hwnd, SB_VERT, &mut info);
                        }
                        info.nTrackPos
                    }
                    _ => pane.scroll,
                };
                scroll_by(hwnd, pane, to);
                return LRESULT(0);
            }
        }
        WM_MOUSEWHEEL => {
            if let Some(pane) = pane_of(hwnd) {
                let delta = ((wparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
                let to = pane.scroll - delta * STEP / 40;
                scroll_by(hwnd, pane, to);
                return LRESULT(0);
            }
        }
        WM_KEYDOWN => {
            if let Some(pane) = pane_of(hwnd) {
                let page = client(hwnd).bottom;
                let key = (wparam.0 & 0xFFFF) as u16;
                let to = match key {
                    k if k == VK_UP.0 => Some(pane.scroll - STEP),
                    k if k == VK_DOWN.0 => Some(pane.scroll + STEP),
                    k if k == VK_PRIOR.0 => Some(pane.scroll - page),
                    k if k == VK_NEXT.0 => Some(pane.scroll + page),
                    k if k == VK_HOME.0 => Some(0),
                    k if k == VK_END.0 => Some(pane.total),
                    _ => None,
                };
                if let Some(to) = to {
                    scroll_by(hwnd, pane, to);
                    return LRESULT(0);
                }
            }
        }
        WM_LBUTTONDOWN => unsafe {
            let _ = SetFocus(Some(hwnd));
        },
        _ => {}
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}
