//! Thumbnails of `.tdraw` and `.cdr` drawings in the Windows Explorer: a
//! thumbnail provider (`IThumbnailProvider` with `IInitializeWithStream`)
//! the installer registers for the `.tdraw` extension, and for `.cdr`
//! where no other program gives those files thumbnails (#3239). Explorer
//! hands it the file as a stream; it draws the drawing's first page on white paper,
//! with what lies beside it on the desktop (#2357), at the size asked and returns it as a 32-bit bitmap with alpha.
//!
//! The same DLL holds a preview handler (`IPreviewHandler`) for the
//! Explorer's Preview Pane (#4455): every page of the drawing, one under
//! another at the width of the pane, scrolled with the scroll bar, the
//! mouse wheel or the keys.
//!
//! The pictures are made by [`thumbnail_rgba`], [`preview_layout`] and
//! [`page_bgra`], which every platform builds and tests; only the COM glue
//! and the pane's window are Windows code.

use tracedraw_core::Document;

/// The class id the installer registers (`packaging/windows/tracedraw.iss`)
/// under `.tdraw\ShellEx\{e357fccd-a995-4576-b01f-234630154e96}`.
pub const CLSID_TEXT: &str = "{6B1E5D3A-2F41-4C8E-9B7A-1D2C3E4F5A60}";

/// The preview handler's class id (Preview Pane, #4455), registered under
/// `.tdraw\ShellEx\{8895b1c6-b41f-4c1c-a562-0d564250836f}`.
pub const PREVIEW_CLSID_TEXT: &str = "{3C9B0727-3F4F-4DFB-9993-68CAF8283667}";

/// Pixels round and between the pages in the Preview Pane.
pub const PREVIEW_GAP: i32 = 12;
/// The most pages the Preview Pane shows.
pub const PREVIEW_MAX_PAGES: usize = 500;
/// The widest a page is drawn in the Preview Pane, in pixels.
pub const PREVIEW_MAX_WIDTH: i32 = 2400;

/// Where a page sits in the Preview Pane: pixels from the left and from
/// the top of the first page's gap, and its size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// The pages of `sizes` (width and height in mm) one under another in a
/// pane `width` pixels wide, the widest filling it but for the gaps and
/// the others to the same scale, centred: each page's place and the
/// height of them all with their gaps.
pub fn preview_layout(sizes: &[(f64, f64)], width: i32) -> (Vec<Slot>, i32) {
    let widest = sizes
        .iter()
        .map(|s| s.0)
        .filter(|w| w.is_finite() && *w > 0.0)
        .fold(0.0, f64::max);
    let room = (width - 2 * PREVIEW_GAP).clamp(16, PREVIEW_MAX_WIDTH);
    let scale = if widest > 0.0 {
        f64::from(room) / widest
    } else {
        0.0
    };
    let mut y = PREVIEW_GAP;
    let mut slots = Vec::with_capacity(sizes.len());
    for &(w, h) in sizes {
        let px = |mm: f64| {
            if mm.is_finite() && mm > 0.0 {
                ((mm * scale).round() as i32).clamp(1, PREVIEW_MAX_WIDTH * 8)
            } else {
                1
            }
        };
        let (pw, ph) = (px(w).min(room), px(h));
        slots.push(Slot {
            x: ((width - pw) / 2).max(0),
            y,
            w: pw,
            h: ph,
        });
        y += ph + PREVIEW_GAP;
    }
    (slots, y)
}

/// The sizes (mm) of the drawing's pages the Preview Pane shows.
pub fn page_sizes(doc: &Document) -> Vec<(f64, f64)> {
    doc.pages
        .iter()
        .take(PREVIEW_MAX_PAGES)
        .map(|p| (p.size.width, p.size.height))
        .collect()
}

/// Page `index` of `doc` on white paper, `width` pixels across: width,
/// height and pixels as blue, green, red and alpha, as the Windows
/// drawing calls take them. `None` when there is no such page.
pub fn page_bgra(doc: &Document, index: usize, width: u32) -> Option<(u32, u32, Vec<u8>)> {
    let page = doc.pages.get(index)?;
    let mm = page.size.width;
    if !(mm.is_finite() && mm > 0.0) || width == 0 {
        return None;
    }
    let width = width.min(PREVIEW_MAX_WIDTH as u32);
    let pm = tracedraw_render::render_page_image(doc, page.id, f64::from(width) * 25.4 / mm)?;
    let mut out = Vec::with_capacity(pm.data().len());
    for px in pm.pixels() {
        // On white: the paper shows through what is see-through.
        let a = u32::from(px.alpha());
        let on_white = |c: u8| (u32::from(c) + 255 - a).min(255) as u8;
        out.extend_from_slice(&[
            on_white(px.blue()),
            on_white(px.green()),
            on_white(px.red()),
            255,
        ]);
    }
    Some((pm.width(), pm.height(), out))
}

/// The drawing in `bytes` for the Preview Pane, as for the thumbnail.
pub fn preview_document(bytes: &[u8]) -> Option<Document> {
    read_drawing(bytes)
}

/// The first page of the drawing in `bytes` (a `.tdraw` file), with what
/// lies beside it on the desktop (#2357), drawn with its long side `size` pixels on white paper: width, height and RGBA
/// pixels (straight, not premultiplied). `None` when the file cannot be
/// read or has no page; never panics on what the file holds.
pub fn thumbnail_rgba(bytes: &[u8], size: u32) -> Option<(u32, u32, Vec<u8>)> {
    let doc = read_drawing(bytes)?;
    let page = doc.pages.first()?;
    let size = f64::from(size.clamp(16, 2048));
    let pm = tracedraw_render::render_whole_page_image(&doc, page.id, size)?;
    let mut rgba = Vec::with_capacity(pm.data().len());
    for px in pm.pixels() {
        let c = px.demultiply();
        rgba.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    Some((pm.width(), pm.height(), rgba))
}

/// The drawing in `bytes`: a `.tdraw` file, packed (gzip) or plain JSON
/// as every version saved it (#2849), or a `.cdr` (or CMX) file, the RIFF
/// or zip container of any version the reader takes (#3239).
fn read_drawing(bytes: &[u8]) -> Option<Document> {
    if bytes.starts_with(b"RIFF") || bytes.starts_with(b"PK") || bytes.starts_with(b"WL") {
        return tracedraw_cdr::open_bytes(bytes, "")
            .ok()
            .map(|(doc, _)| doc);
    }
    Document::from_tdraw_bytes(bytes).ok()
}

#[cfg(windows)]
mod com {
    use std::cell::RefCell;
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicI32, Ordering};
    use std::sync::Once;
    use windows::core::{implement, Interface, Ref, BOOL, GUID, HRESULT};
    use windows::Win32::Foundation::{
        CLASS_E_CLASSNOTAVAILABLE, CLASS_E_NOAGGREGATION, E_FAIL, E_POINTER, S_FALSE, S_OK,
    };
    use windows::Win32::Graphics::Gdi::{
        CreateDIBSection, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP,
    };
    use windows::Win32::System::Com::{
        IClassFactory, IClassFactory_Impl, IStream, STATFLAG_NONAME, STATSTG,
    };
    use windows::Win32::UI::Shell::PropertiesSystem::{
        IInitializeWithStream, IInitializeWithStream_Impl,
    };
    use windows::Win32::UI::Shell::{
        IThumbnailProvider, IThumbnailProvider_Impl, WTSAT_ARGB, WTS_ALPHATYPE,
    };

    /// The provider's class id, the same as [`super::CLSID_TEXT`].
    const CLSID: GUID = GUID::from_u128(0x6B1E5D3A_2F41_4C8E_9B7A_1D2C3E4F5A60);
    /// The preview handler's, the same as [`super::PREVIEW_CLSID_TEXT`].
    const PREVIEW_CLSID: GUID = GUID::from_u128(0x3C9B0727_3F4F_4DFB_9993_68CAF8283667);

    /// Objects alive, for `DllCanUnloadNow`.
    pub(crate) static LIVE: AtomicI32 = AtomicI32::new(0);
    pub(crate) static FONTS: Once = Once::new();

    /// Files past this size are not drawn (Explorer shows the icon).
    pub(crate) const MAX_BYTES: u64 = 256 * 1024 * 1024;

    #[implement(IThumbnailProvider, IInitializeWithStream)]
    struct Provider {
        data: RefCell<Option<Vec<u8>>>,
    }

    impl Provider {
        fn new() -> Self {
            LIVE.fetch_add(1, Ordering::SeqCst);
            Provider {
                data: RefCell::new(None),
            }
        }
    }

    impl Drop for Provider {
        fn drop(&mut self) {
            LIVE.fetch_sub(1, Ordering::SeqCst);
        }
    }

    impl IInitializeWithStream_Impl for Provider_Impl {
        fn Initialize(
            &self,
            pstream: Ref<'_, IStream>,
            _grfmode: u32,
        ) -> windows::core::Result<()> {
            *self.data.borrow_mut() = Some(read_stream(pstream.ok()?)?);
            Ok(())
        }
    }

    /// The whole file Explorer hands over as a stream.
    pub(crate) fn read_stream(stream: &IStream) -> windows::core::Result<Vec<u8>> {
        let mut stat = STATSTG::default();
        unsafe { stream.Stat(&mut stat, STATFLAG_NONAME)? };
        if stat.cbSize > MAX_BYTES {
            return Err(E_FAIL.into());
        }
        let mut out = Vec::with_capacity(stat.cbSize as usize);
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let mut read = 0u32;
            let hr =
                unsafe { stream.Read(buf.as_mut_ptr().cast(), buf.len() as u32, Some(&mut read)) };
            if hr.is_err() {
                return Err(hr.into());
            }
            if read == 0 {
                break;
            }
            out.extend_from_slice(&buf[..read as usize]);
            if out.len() as u64 > MAX_BYTES {
                return Err(E_FAIL.into());
            }
        }
        Ok(out)
    }

    impl IThumbnailProvider_Impl for Provider_Impl {
        fn GetThumbnail(
            &self,
            cx: u32,
            phbmp: *mut HBITMAP,
            pdwalpha: *mut WTS_ALPHATYPE,
        ) -> windows::core::Result<()> {
            if phbmp.is_null() || pdwalpha.is_null() {
                return Err(E_POINTER.into());
            }
            FONTS.call_once(tracedraw_text::install);
            let data = self.data.borrow();
            let bytes = data.as_deref().ok_or(windows::core::Error::from(E_FAIL))?;
            let (w, h, rgba) = std::panic::catch_unwind(|| super::thumbnail_rgba(bytes, cx))
                .ok()
                .flatten()
                .ok_or(windows::core::Error::from(E_FAIL))?;
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w as i32,
                    // Negative: rows top-down, as the pixels are.
                    biHeight: -(h as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits: *mut c_void = std::ptr::null_mut();
            let hbmp =
                unsafe { CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0)? };
            if bits.is_null() {
                return Err(E_FAIL.into());
            }
            // BGRA, premultiplied, as WTSAT_ARGB asks.
            let dst = unsafe { std::slice::from_raw_parts_mut(bits.cast::<u8>(), rgba.len()) };
            for (d, s) in dst.chunks_exact_mut(4).zip(rgba.chunks_exact(4)) {
                let a = u32::from(s[3]);
                let pm = |c: u8| ((u32::from(c) * a + 127) / 255) as u8;
                d[0] = pm(s[2]);
                d[1] = pm(s[1]);
                d[2] = pm(s[0]);
                d[3] = s[3];
            }
            unsafe {
                *phbmp = hbmp;
                *pdwalpha = WTSAT_ARGB;
            }
            Ok(())
        }
    }

    #[implement(IClassFactory)]
    struct Factory {
        /// The preview handler's factory, else the thumbnail provider's.
        preview: bool,
    }

    impl IClassFactory_Impl for Factory_Impl {
        fn CreateInstance(
            &self,
            punkouter: Ref<'_, windows::core::IUnknown>,
            riid: *const GUID,
            ppvobject: *mut *mut c_void,
        ) -> windows::core::Result<()> {
            if ppvobject.is_null() || riid.is_null() {
                return Err(E_POINTER.into());
            }
            unsafe { *ppvobject = std::ptr::null_mut() };
            if !punkouter.is_null() {
                return Err(CLASS_E_NOAGGREGATION.into());
            }
            let unknown: windows::core::IUnknown = if self.preview {
                super::pane::Previewer::new().into()
            } else {
                Provider::new().into()
            };
            unsafe { unknown.query(&*riid, ppvobject).ok() }
        }

        fn LockServer(&self, _flock: BOOL) -> windows::core::Result<()> {
            Ok(())
        }
    }

    /// # Safety
    /// Called by COM with valid pointers.
    #[no_mangle]
    pub unsafe extern "system" fn DllGetClassObject(
        rclsid: *const GUID,
        riid: *const GUID,
        ppv: *mut *mut c_void,
    ) -> HRESULT {
        if rclsid.is_null() || riid.is_null() || ppv.is_null() {
            return E_POINTER;
        }
        *ppv = std::ptr::null_mut();
        let preview = *rclsid == PREVIEW_CLSID;
        if *rclsid != CLSID && !preview {
            return CLASS_E_CLASSNOTAVAILABLE;
        }
        let factory: IClassFactory = Factory { preview }.into();
        factory.query(&*riid, ppv)
    }

    #[no_mangle]
    pub extern "system" fn DllCanUnloadNow() -> HRESULT {
        if LIVE.load(Ordering::SeqCst) == 0 {
            S_OK
        } else {
            S_FALSE
        }
    }

    #[allow(dead_code)]
    fn _assert_interfaces() {
        let _ = IThumbnailProvider::IID;
        let _ = IInitializeWithStream::IID;
    }
}

#[cfg(windows)]
mod pane;

#[cfg(test)]
mod tests {
    use super::*;

    /// The sample drawing gives a picture with its long side as asked,
    /// white where the page is empty; anything else gives none.
    #[test]
    fn the_first_page_is_drawn_at_the_size_asked() {
        let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../sample.tdraw"))
            .expect("sample");
        let (w, h, rgba) = thumbnail_rgba(&bytes, 256).expect("drawn");
        assert_eq!(w.max(h), 256);
        assert_eq!(rgba.len(), (w * h * 4) as usize);
        assert_eq!(&rgba[..4], &[255, 255, 255, 255]);
        assert!(thumbnail_rgba(b"not a drawing", 256).is_none());
        assert!(thumbnail_rgba(&[0xff, 0xfe, 0x00], 256).is_none());
    }

    /// A `.cdr` file gets its first page drawn too; a broken one never
    /// panics (#3239).
    #[test]
    fn a_cdr_file_gets_its_first_page_drawn() {
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../crates/cdr/tests/fixtures/gaussian-blur.cdr"),
        )
        .expect("fixture");
        let (w, h, rgba) = thumbnail_rgba(&bytes, 128).expect("a picture");
        assert_eq!(w.max(h), 128, "{w}x{h}");
        assert_eq!(rgba.len(), (w * h * 4) as usize);
        // Cut short or not a drawing at all: no panic, whatever comes.
        let _ = thumbnail_rgba(&bytes[..bytes.len() / 3], 128);
        let _ = thumbnail_rgba(b"RIFF\0\0\0\0junk", 128);
        assert!(thumbnail_rgba(b"PK\x03\x04", 128).is_none());
    }

    /// #4455: the Preview Pane lays the pages out one under another, the
    /// widest filling the pane but for the gaps, the others to its scale
    /// and centred, and draws each on white at the width asked.
    #[test]
    fn the_preview_pane_shows_every_page_one_under_another() {
        let (slots, total) = preview_layout(&[(200.0, 100.0), (100.0, 100.0)], 224);
        assert_eq!(
            slots[0],
            Slot {
                x: 12,
                y: 12,
                w: 200,
                h: 100
            }
        );
        assert_eq!(
            slots[1],
            Slot {
                x: 62,
                y: 124,
                w: 100,
                h: 100
            }
        );
        assert_eq!(total, 236);
        // No page, or a pane too narrow: never a panic or a nonsense size.
        assert_eq!(preview_layout(&[], 300), (Vec::new(), PREVIEW_GAP));
        let (s, _) = preview_layout(&[(210.0, 297.0), (f64::NAN, 0.0)], 0);
        assert!(s.iter().all(|s| s.w >= 1 && s.h >= 1), "{s:?}");

        let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../sample.tdraw"))
            .expect("sample");
        let doc = preview_document(&bytes).expect("read");
        let sizes = page_sizes(&doc);
        assert_eq!(sizes.len(), doc.pages.len().min(PREVIEW_MAX_PAGES));
        let (w, h, px) = page_bgra(&doc, 0, 300).expect("drawn");
        assert_eq!(w, 300);
        let want_h = (300.0 * sizes[0].1 / sizes[0].0).round() as u32;
        assert!(h.abs_diff(want_h) <= 1, "{h} vs {want_h}");
        assert_eq!(px.len(), (w * h * 4) as usize);
        // Opaque everywhere, white where the page is empty.
        assert!(px.chunks_exact(4).all(|p| p[3] == 255));
        assert_eq!(&px[..4], &[255, 255, 255, 255]);
        assert!(page_bgra(&doc, doc.pages.len(), 300).is_none());
        assert!(preview_document(b"not a drawing").is_none());
    }

    /// #4455: the setup and the Store package register the preview handler
    /// where Explorer looks for the Preview Pane's handler of `.tdraw`
    /// (and, in the fallback key alone, `.cdr`), run in the preview host.
    #[test]
    fn the_setup_and_the_store_package_register_the_preview_handler() {
        const PREVIEW_IID: &str = "{8895b1c6-b41f-4c1c-a562-0d564250836f}";
        let iss = repo_file("packaging/windows/tracedraw.iss");
        let clsid = PREVIEW_CLSID_TEXT.replacen('{', "{{", 1);
        let iid = PREVIEW_IID.replacen('{', "{{", 1);
        let lines: Vec<&str> = iss.lines().map(str::trim).collect();
        let has = |want: &[&str]| lines.iter().any(|l| want.iter().all(|w| l.contains(w)));
        let class = format!("Software\\Classes\\CLSID\\{clsid}");
        assert!(has(&[
            &format!("Subkey: \"{class}\\InprocServer32\""),
            "ValueData: \"{app}\\tracedraw_thumbnailer.dll\""
        ]));
        assert!(has(&[
            &format!("Subkey: \"{class}\""),
            "ValueName: \"AppID\"",
            "ValueData: \"{{6d2b5079-2f0b-48dd-ab7f-97cec514d30b}\""
        ]));
        assert!(has(&[
            "Subkey: \"Software\\Microsoft\\Windows\\CurrentVersion\\PreviewHandlers\"",
            &format!("ValueName: \"{clsid}\""),
            "uninsdeletevalue"
        ]));
        for owner in [
            "Software\\Classes\\.tdraw",
            "Software\\Classes\\TraceDraw.Document",
            "Software\\Classes\\SystemFileAssociations\\.tdraw",
            "Software\\Classes\\SystemFileAssociations\\.cdr",
        ] {
            assert!(
                has(&[
                    &format!("Subkey: \"{owner}\\ShellEx\\{iid}\""),
                    &format!("ValueData: \"{clsid}\""),
                    "uninsdeletekey"
                ]),
                "{owner} names the preview handler"
            );
        }
        // Inno Setup writes a literal brace as "{{": a lone one would be
        // read as one of its constants.
        for l in lines.iter().filter(|l| l.contains("3C9B0727")) {
            assert!(!l.contains("\"{3C9B0727"), "{l}");
        }
        let src = repo_file("apps/tracedraw-thumbnailer/src/lib.rs");
        let hex = PREVIEW_CLSID_TEXT
            .trim_matches(['{', '}'])
            .replace('-', "_");
        assert!(src.contains(&format!("GUID::from_u128(0x{hex})")), "{hex}");

        let manifest = repo_file("packaging/windows/msix/AppxManifest.xml");
        let bare = PREVIEW_CLSID_TEXT.trim_matches(['{', '}']);
        let tdraw = manifest
            .split("<uap:FileTypeAssociation Name=\"tdraw\">")
            .nth(1)
            .and_then(|s| s.split("</uap:FileTypeAssociation>").next())
            .expect("the .tdraw association");
        assert!(tdraw.contains(&format!("<desktop2:DesktopPreviewHandler Clsid=\"{bare}\"")));
        assert!(manifest.contains(&format!(
            "<com:Class Id=\"{bare}\" Path=\"tracedraw_thumbnailer.dll\" ThreadingModel=\"STA\""
        )));
    }

    fn repo_file(path: &str) -> String {
        let full = format!("{}/../../{path}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&full).unwrap_or_else(|e| panic!("{full}: {e}"))
    }

    /// The setup copies the provider and registers it where Explorer looks
    /// for the thumbnail handler of `.tdraw` files (the extension, its
    /// ProgID and the system's per-extension key), under the class id the
    /// DLL serves, as an in-process server in an apartment; the release
    /// build makes, signs and hands the DLL to the setup and to the Store
    /// package, whose manifest declares it too (#1740, live test of 0.41).
    #[test]
    fn the_setup_and_the_release_build_ship_and_register_the_provider() {
        // IThumbnailProvider's interface id: the ShellEx key Explorer reads.
        const THUMB_IID: &str = "{e357fccd-a995-4576-b01f-234630154e96}";
        let iss = repo_file("packaging/windows/tracedraw.iss");
        // Inno Setup writes "{{" for a literal brace.
        let clsid = CLSID_TEXT.replacen('{', "{{", 1);
        let iid = THUMB_IID.replacen('{', "{{", 1);
        let lines: Vec<&str> = iss.lines().map(str::trim).collect();
        let has = |want: &[&str]| lines.iter().any(|l| want.iter().all(|w| l.contains(w)));
        assert!(
            has(&[
                "Source: \"{#SrcDir}\\tracedraw_thumbnailer.dll\"",
                "DestDir: \"{app}\""
            ]),
            "the setup copies the DLL"
        );
        let class = format!("Software\\Classes\\CLSID\\{clsid}");
        assert!(has(&["Root: HKA", &format!("Subkey: \"{class}\"")]));
        assert!(has(&[
            "Root: HKA",
            &format!("Subkey: \"{class}\\InprocServer32\""),
            "ValueName: \"\"",
            "ValueData: \"{app}\\tracedraw_thumbnailer.dll\""
        ]));
        assert!(has(&[
            &format!("Subkey: \"{class}\\InprocServer32\""),
            "ValueName: \"ThreadingModel\"",
            "ValueData: \"Apartment\""
        ]));
        for owner in [
            "Software\\Classes\\.tdraw",
            "Software\\Classes\\TraceDraw.Document",
            "Software\\Classes\\SystemFileAssociations\\.tdraw",
            "Software\\Classes\\SystemFileAssociations\\.cdr",
        ] {
            assert!(
                has(&[
                    "Root: HKA",
                    &format!("Subkey: \"{owner}\\ShellEx\\{iid}\""),
                    "ValueName: \"\"",
                    &format!("ValueData: \"{clsid}\""),
                    "uninsdeletekey"
                ]),
                "{owner} names the provider"
            );
        }
        // HKA: the current user's keys for an install for that user alone,
        // the machine's for one for everybody; both are read by Explorer.
        assert!(iss.contains("PrivilegesRequiredOverridesAllowed=dialog"));
        assert!(iss.contains("ChangesAssociations=yes"));
        // The class id the DLL answers to is the one registered.
        let src = repo_file("apps/tracedraw-thumbnailer/src/lib.rs");
        let hex = CLSID_TEXT.trim_matches(['{', '}']).replace('-', "_");
        assert!(src.contains(&format!("GUID::from_u128(0x{hex})")), "{hex}");

        // The release build makes the DLL, signs it with the programs and
        // the setup is built from the folder it lies in.
        let ci = repo_file(".github/workflows/build.yml");
        let build = ci
            .lines()
            .find(|l| l.contains("cargo build --release") && l.contains("-p tracedraw "))
            .expect("the release build");
        assert!(build.contains("-p tracedraw-thumbnailer"), "{build}");
        assert!(
            ci.matches("release\\tracedraw_thumbnailer.dll").count()
                + ci.matches("release/tracedraw_thumbnailer.dll").count()
                >= 2,
            "both signing providers sign the DLL"
        );
        assert!(ci.contains("signed-programs\\*.dll"));
        assert!(ci.contains("/DSrcDir=\"$PWD\\$src\" packaging\\windows\\tracedraw.iss"));

        // The Store package: staged and declared, the same class id.
        let msix = repo_file("packaging/windows/build-msix.ps1");
        assert!(msix.contains("Copy-Item (Join-Path $SrcDir 'tracedraw_thumbnailer.dll') $stage"));
        let manifest = repo_file("packaging/windows/msix/AppxManifest.xml");
        let bare = CLSID_TEXT.trim_matches(['{', '}']);
        assert!(manifest.contains(&format!("<desktop2:ThumbnailHandler Clsid=\"{bare}\"")));
        assert!(manifest.contains(&format!(
            "<com:Class Id=\"{bare}\" Path=\"tracedraw_thumbnailer.dll\" ThreadingModel=\"STA\""
        )));
        // The handler sits in the .tdraw association, not the .cdr one.
        let tdraw = manifest
            .split("<uap:FileTypeAssociation Name=\"tdraw\">")
            .nth(1)
            .and_then(|s| s.split("</uap:FileTypeAssociation>").next())
            .expect("the .tdraw association");
        assert!(tdraw.contains("desktop2:ThumbnailHandler"));
    }
}
