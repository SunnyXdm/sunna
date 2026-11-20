//! The pointer's shape on the shared screen. The viewer draws its own pointer
//! (so it moves without delay) and gives it the host's shape: resize arrows
//! over a window edge, the text beam over text, a hand over a link.

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use std::sync::atomic::Ordering;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use std::time::Duration;

/// One pointer shape, as the screen shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorImage {
    /// The same shape keeps the same id.
    pub id: u64,
    pub width: u16,
    pub height: u16,
    pub hot_x: u16,
    pub hot_y: u16,
    /// The screen's width in the image's pixels.
    pub screen_width: u32,
    /// Premultiplied RGBA.
    pub rgba: Vec<u8>,
}

/// How often the shape is looked at where the system can't say it changed.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
const POLL: Duration = Duration::from_millis(30);

/// A capture backend can supply shapes without opening an X connection.
#[cfg(target_os = "linux")]
pub type Provider = Arc<dyn Fn() -> Option<CursorImage> + Send + Sync>;
#[cfg(target_os = "linux")]
static PROVIDER: std::sync::Mutex<Option<Provider>> = std::sync::Mutex::new(None);

#[cfg(target_os = "linux")]
pub struct ProviderGuard(Provider);
#[cfg(target_os = "linux")]
pub fn register_provider(provider: Provider) -> ProviderGuard {
    *PROVIDER.lock().unwrap() = Some(provider.clone());
    ProviderGuard(provider)
}
#[cfg(target_os = "linux")]
impl Drop for ProviderGuard {
    fn drop(&mut self) {
        let mut provider = PROVIDER.lock().unwrap();
        if provider.as_ref().is_some_and(|p| Arc::ptr_eq(p, &self.0)) {
            *provider = None;
        }
    }
}

/// Poll from a Linux session worker that already has a capture loop.
#[cfg(target_os = "linux")]
pub fn reader() -> anyhow::Result<Box<dyn FnMut() -> anyhow::Result<Option<CursorImage>>>> {
    if let Some(provider) = PROVIDER.lock().unwrap().clone() {
        return Ok(Box::new(move || Ok(provider())));
    }
    let mut shapes = linux::Shapes::open()?;
    Ok(Box::new(move || shapes.next()))
}

/// Watch the pointer's shape from a thread of its own, calling `changed` with
/// each new one, until `stop` is set.
#[cfg_attr(not(any(target_os = "linux", target_os = "macos", target_os = "windows")), allow(unused_variables, unused_mut))]
pub fn watch(
    stop: Arc<AtomicBool>,
    mut changed: impl FnMut(CursorImage) + Send + 'static,
) -> anyhow::Result<std::thread::JoinHandle<()>> {
    #[cfg(target_os = "linux")]
    let mut shapes = if PROVIDER.lock().unwrap().is_some() {
        None
    } else {
        Some(linux::Shapes::open()?)
    };
    #[cfg(target_os = "macos")]
    let mut shapes = macos::Shapes::open()?;
    #[cfg(target_os = "windows")]
    let mut shapes = windows::Shapes::open();
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    anyhow::bail!("pointer shapes aren't available on this system");
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    Ok(std::thread::Builder::new()
        .name("sunna-cursor".into())
        .spawn(move || {
            let mut last = None;
            while !stop.load(Ordering::Relaxed) {
                #[cfg(target_os = "linux")]
                let next = match &mut shapes {
                    Some(shapes) => shapes.next(),
                    None => Ok(PROVIDER
                        .lock()
                        .unwrap()
                        .clone()
                        .and_then(|provider| provider())),
                };
                #[cfg(any(target_os = "macos", target_os = "windows"))]
                let next = shapes.next();
                match next {
                    Ok(Some(image)) if last != Some(image.id) => {
                        last = Some(image.id);
                        changed(image);
                    }
                    Ok(_) => {}
                    Err(error) => {
                        tracing::warn!(%error, "pointer shapes stopped");
                        return;
                    }
                }
                std::thread::sleep(POLL);
            }
        })?)
}

/// FNV-1a: a stable id for a shape from its pixels and hot spot.
#[cfg(any(target_os = "linux", target_os = "macos", test))]
pub fn shape_id(width: usize, height: usize, hot: (u16, u16), rgba: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let header = [width as u64, height as u64, hot.0 as u64, hot.1 as u64];
    for byte in header.iter().flat_map(|value| value.to_le_bytes()).chain(rgba.iter().copied()) {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

#[cfg(target_os = "linux")]
mod linux {
    use anyhow::Context;
    use x11rb::connection::Connection;
    use x11rb::protocol::xfixes::{self, ConnectionExt as _};
    use x11rb::protocol::Event;
    use x11rb::rust_connection::RustConnection;

    use super::CursorImage;

    /// XFixes tells when the pointer's shape changes, and what it is.
    pub(super) struct Shapes {
        connection: RustConnection,
        screen_width: u32,
        /// Look at the shape at the first call, then only when told.
        changed: bool,
    }

    impl Shapes {
        pub(super) fn open() -> anyhow::Result<Self> {
            let (connection, screen) =
                crate::x11::connect(None).context("connecting to the X server (is DISPLAY set?)")?;
            connection
                .xfixes_query_version(5, 0)?
                .reply()
                .context("the X server has no XFixes")?;
            let root = connection.setup().roots[screen].root;
            let screen_width = connection.setup().roots[screen].width_in_pixels as u32;
            connection.xfixes_select_cursor_input(root, xfixes::CursorNotifyMask::DISPLAY_CURSOR)?;
            connection.flush()?;
            Ok(Self { connection, screen_width, changed: true })
        }

        pub(super) fn next(&mut self) -> anyhow::Result<Option<CursorImage>> {
            while let Some(event) = self.connection.poll_for_event()? {
                if let Event::XfixesCursorNotify(_) = event {
                    self.changed = true;
                }
            }
            if !std::mem::take(&mut self.changed) {
                return Ok(None);
            }
            let image = self.connection.xfixes_get_cursor_image()?.reply()?;
            // ARGB words, already premultiplied.
            let rgba = image
                .cursor_image
                .iter()
                .flat_map(|&argb| [(argb >> 16) as u8, (argb >> 8) as u8, argb as u8, (argb >> 24) as u8])
                .collect();
            Ok(Some(CursorImage {
                id: image.cursor_serial as u64,
                width: image.width,
                height: image.height,
                hot_x: image.xhot,
                hot_y: image.yhot,
                screen_width: self.screen_width,
                rgba,
            }))
        }
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::c_void;

    use objc2::encode::{Encode, Encoding};
    use objc2::rc::autoreleasepool;
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};

    use super::{shape_id, CursorImage};

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Point {
        x: f64,
        y: f64,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Size {
        width: f64,
        height: f64,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Rect {
        origin: Point,
        size: Size,
    }

    unsafe impl Encode for Point {
        const ENCODING: Encoding = Encoding::Struct("CGPoint", &[f64::ENCODING, f64::ENCODING]);
    }
    unsafe impl Encode for Size {
        const ENCODING: Encoding = Encoding::Struct("CGSize", &[f64::ENCODING, f64::ENCODING]);
    }

    #[link(name = "AppKit", kind = "framework")]
    extern "C" {}

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGMainDisplayID() -> u32;
        fn CGDisplayBounds(display: u32) -> Rect;
        fn CGImageGetWidth(image: *const c_void) -> usize;
        fn CGImageGetHeight(image: *const c_void) -> usize;
        fn CGColorSpaceCreateDeviceRGB() -> *mut c_void;
        fn CGColorSpaceRelease(space: *mut c_void);
        fn CGBitmapContextCreate(
            data: *mut c_void,
            width: usize,
            height: usize,
            bits_per_component: usize,
            bytes_per_row: usize,
            space: *mut c_void,
            bitmap_info: u32,
        ) -> *mut c_void;
        fn CGContextDrawImage(context: *mut c_void, rect: Rect, image: *const c_void);
        fn CGContextRelease(context: *mut c_void);
    }

    /// kCGImageAlphaPremultipliedLast | kCGBitmapByteOrder32Big: RGBA bytes.
    const RGBA_PREMULTIPLIED: u32 = 1 | (4 << 12);

    /// macOS doesn't announce shape changes to other processes: look at the
    /// cursor it shows (`NSCursor.currentSystemCursor`) every so often.
    pub(super) struct Shapes;

    impl Shapes {
        pub(super) fn open() -> anyhow::Result<Self> {
            Ok(Self)
        }

        pub(super) fn next(&mut self) -> anyhow::Result<Option<CursorImage>> {
            // SAFETY: AppKit and CoreGraphics calls with checked results; the
            // bitmap context draws into `rgba`, which outlives it.
            Ok(autoreleasepool(|_| unsafe {
                let cursor: *mut AnyObject = msg_send![class!(NSCursor), currentSystemCursor];
                if cursor.is_null() {
                    return None;
                }
                let image: *mut AnyObject = msg_send![cursor, image];
                if image.is_null() {
                    return None;
                }
                let hot: Point = msg_send![cursor, hotSpot];
                let size: Size = msg_send![image, size];
                let null = std::ptr::null_mut::<AnyObject>();
                let cg: *const c_void =
                    msg_send![image, CGImageForProposedRect: std::ptr::null_mut::<c_void>(), context: null, hints: null];
                if cg.is_null() || size.width <= 0.0 {
                    return None;
                }
                let (width, height) = (CGImageGetWidth(cg), CGImageGetHeight(cg));
                if width == 0 || height == 0 || width > 512 || height > 512 {
                    return None;
                }
                let mut rgba = vec![0u8; width * height * 4];
                let space = CGColorSpaceCreateDeviceRGB();
                let context = CGBitmapContextCreate(
                    rgba.as_mut_ptr().cast(),
                    width,
                    height,
                    8,
                    width * 4,
                    space,
                    RGBA_PREMULTIPLIED,
                );
                CGColorSpaceRelease(space);
                if context.is_null() {
                    return None;
                }
                let full = Rect { origin: Point { x: 0.0, y: 0.0 }, size: Size { width: width as f64, height: height as f64 } };
                CGContextDrawImage(context, full, cg);
                CGContextRelease(context);
                // The image may be drawn at 2x: hot spot and screen in its pixels.
                let scale = width as f64 / size.width;
                let hot = ((hot.x * scale).round() as u16, (hot.y * scale).round() as u16);
                let screen = CGDisplayBounds(CGMainDisplayID());
                Some(CursorImage {
                    id: shape_id(width, height, hot, &rgba),
                    width: width as u16,
                    height: height as u16,
                    hot_x: hot.0,
                    hot_y: hot.1,
                    screen_width: (screen.size.width * scale).round() as u32,
                    rgba,
                })
            }))
        }
    }
}

/// GDI expands both masks to top-down BGRA, so their scanlines have no padding.
#[cfg(any(target_os = "windows", test))]
fn windows_rgba(color: Option<&[u8]>, mask: &[u8], pixels: usize) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(pixels * 4);
    if let Some(color) = color {
        let has_alpha = color.as_chunks::<4>().0.iter().any(|pixel| pixel[3] != 0);
        for (pixel, and) in color.as_chunks::<4>().0.iter().zip(mask.as_chunks::<4>().0.iter()) {
            // Windows alpha cursors already contain premultiplied BGRA. Only
            // legacy cursors (all-zero alpha) take transparency from the mask.
            let alpha = if has_alpha {
                pixel[3]
            } else if and[0] == 0 {
                255
            } else {
                0
            };
            rgba.extend_from_slice(&if alpha == 0 {
                [0, 0, 0, 0]
            } else {
                [pixel[2], pixel[1], pixel[0], alpha]
            });
        }
    } else {
        let (and, xor) = mask.split_at(pixels * 4);
        for (and, xor) in and.as_chunks::<4>().0.iter().zip(xor.as_chunks::<4>().0.iter()) {
            let pixel = match (and[0] != 0, xor[0] != 0) {
                (true, false) => [0, 0, 0, 0],
                (false, true) => [255, 255, 255, 255],
                // Inversion depends on the desktop behind the cursor, which
                // a standalone viewer cursor cannot do. Use opaque black.
                _ => [0, 0, 0, 255],
            };
            rgba.extend_from_slice(&pixel);
        }
    }
    rgba
}

#[cfg(target_os = "windows")]
mod windows {
    use std::mem::size_of;

    use ::windows::Win32::Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC,
    };
    use ::windows::Win32::UI::WindowsAndMessaging::{
        GetCursorInfo, GetIconInfo, GetSystemMetrics, CURSORINFO, CURSOR_SHOWING, HCURSOR, HICON,
        ICONINFO, SM_CXSCREEN,
    };
    use anyhow::Context;

    use super::{windows_rgba, CursorImage};

    pub(super) struct Shapes {
        last: Option<usize>,
    }

    impl Shapes {
        pub(super) fn open() -> Self {
            Self { last: None }
        }

        pub(super) fn next(&mut self) -> anyhow::Result<Option<CursorImage>> {
            // A SYSTEM host sees the pointer on the lock screen and UAC
            // prompts too, from the desktop that has the keyboard.
            crate::windows::follow_input_desktop();
            let mut info = CURSORINFO {
                cbSize: size_of::<CURSORINFO>() as u32,
                ..Default::default()
            };
            // SAFETY: `info` has the required size and lives through the call.
            unsafe { GetCursorInfo(&mut info) }.context("reading the Windows cursor")?;
            let id = if info.flags == CURSOR_SHOWING {
                info.hCursor.0 as usize
            } else {
                0
            };
            if self.last == Some(id) {
                return Ok(None);
            }
            // The host shares the primary display. The process is per-monitor
            // DPI aware, so this width and the cursor bitmap are physical pixels.
            // SAFETY: GetSystemMetrics takes no pointers or owned resources.
            let screen_width = unsafe { GetSystemMetrics(SM_CXSCREEN) }.max(1) as u32;
            let image = if id == 0 {
                // The wire format has no visibility flag: a transparent shape
                // hides the viewer's cursor without leaving its previous image.
                CursorImage {
                    id: 0,
                    width: 1,
                    height: 1,
                    hot_x: 0,
                    hot_y: 0,
                    screen_width,
                    rgba: vec![0; 4],
                }
            } else {
                read_cursor(info.hCursor, screen_width)?
            };
            self.last = Some(id);
            Ok(Some(image))
        }
    }

    struct IconBitmaps(ICONINFO);

    impl Drop for IconBitmaps {
        fn drop(&mut self) {
            // SAFETY: GetIconInfo creates these bitmaps for the caller; no DC
            // has them selected. The HCURSOR itself is borrowed and stays alive.
            unsafe {
                if !self.0.hbmMask.0.is_null() {
                    let _ = DeleteObject(self.0.hbmMask.into());
                }
                if !self.0.hbmColor.0.is_null() {
                    let _ = DeleteObject(self.0.hbmColor.into());
                }
            }
        }
    }

    struct ScreenDc(HDC);

    impl Drop for ScreenDc {
        fn drop(&mut self) {
            // SAFETY: This DC came from GetDC(None) on this same thread.
            unsafe {
                ReleaseDC(None, self.0);
            }
        }
    }

    fn dimensions(bitmap: HBITMAP) -> anyhow::Result<(usize, usize)> {
        let mut object = BITMAP::default();
        // SAFETY: The bitmap is owned by IconBitmaps; the output is a BITMAP
        // with exactly the size passed to GetObjectW.
        let read = unsafe {
            GetObjectW(
                bitmap.into(),
                size_of::<BITMAP>() as i32,
                Some((&mut object as *mut BITMAP).cast()),
            )
        };
        anyhow::ensure!(
            read == size_of::<BITMAP>() as i32,
            "reading cursor bitmap dimensions"
        );
        // Bound allocations, including the double-height monochrome mask.
        anyhow::ensure!(
            (1..=512).contains(&object.bmWidth) && (1..=1024).contains(&object.bmHeight),
            "invalid cursor bitmap dimensions"
        );
        Ok((object.bmWidth as usize, object.bmHeight as usize))
    }

    fn bitmap_bgra(
        dc: HDC,
        bitmap: HBITMAP,
        width: usize,
        height: usize,
    ) -> anyhow::Result<Vec<u8>> {
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32), // top-down, including stacked AND/XOR masks
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bytes = vec![0; width * height * 4];
        // SAFETY: The bitmap is not selected into a DC. Dimensions were checked
        // before allocation; 32-bit rows are DWORD aligned with no extra padding.
        let lines = unsafe {
            GetDIBits(
                dc,
                bitmap,
                0,
                height as u32,
                Some(bytes.as_mut_ptr().cast()),
                &mut info,
                DIB_RGB_COLORS,
            )
        };
        anyhow::ensure!(lines == height as i32, "reading cursor bitmap pixels");
        Ok(bytes)
    }

    fn read_cursor(cursor: HCURSOR, screen_width: u32) -> anyhow::Result<CursorImage> {
        let mut icon = IconBitmaps(ICONINFO::default());
        // SAFETY: GetIconInfo accepts a cursor cast to HICON and initializes the
        // output. The guard frees any returned bitmaps, including on errors.
        unsafe { GetIconInfo(HICON(cursor.0), &mut icon.0) }
            .context("reading the Windows cursor image")?;
        let (width, mask_height) = dimensions(icon.0.hbmMask)?;
        let monochrome = icon.0.hbmColor.0.is_null();
        let height = if monochrome {
            anyhow::ensure!(
                mask_height % 2 == 0,
                "invalid monochrome cursor mask height"
            );
            mask_height / 2
        } else {
            let (color_width, color_height) = dimensions(icon.0.hbmColor)?;
            anyhow::ensure!(
                color_width == width && color_height == mask_height,
                "cursor mask and color dimensions differ"
            );
            color_height
        };
        anyhow::ensure!(height > 0 && height <= 512, "invalid cursor height");
        anyhow::ensure!(
            icon.0.xHotspot < width as u32 && icon.0.yHotspot < height as u32,
            "invalid cursor hot spot"
        );
        // SAFETY: Gets a screen DC, released on this same thread by the guard.
        let dc = ScreenDc(unsafe { GetDC(None) });
        anyhow::ensure!(!dc.0 .0.is_null(), "opening a DC for the cursor");
        let mask = bitmap_bgra(dc.0, icon.0.hbmMask, width, mask_height)?;
        let color = if monochrome {
            None
        } else {
            Some(bitmap_bgra(dc.0, icon.0.hbmColor, width, height)?)
        };
        Ok(CursorImage {
            id: cursor.0 as usize as u64,
            width: width as u16,
            height: height as u16,
            hot_x: icon.0.xHotspot as u16,
            hot_y: icon.0.yHotspot as u16,
            screen_width,
            rgba: windows_rgba(color.as_deref(), &mask, width * height),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_alpha_is_already_premultiplied_and_overrides_the_mask() {
        let color = [20, 40, 80, 128, 10, 20, 30, 0];
        let mask = [255; 8];
        assert_eq!(windows_rgba(Some(&color), &mask, 2), [80, 40, 20, 128, 0, 0, 0, 0]);
    }

    #[test]
    fn windows_legacy_color_uses_the_and_mask_for_alpha() {
        let color = [10, 20, 30, 0, 40, 50, 60, 0];
        let mask = [0, 0, 0, 0, 255, 255, 255, 0];
        assert_eq!(windows_rgba(Some(&color), &mask, 2), [30, 20, 10, 255, 0, 0, 0, 0]);
    }

    #[test]
    fn windows_monochrome_masks_cover_all_four_combinations() {
        // Two rows: black/white, then transparent/inverted; AND above XOR.
        let mask = [
            0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 0, 255, 255, 255, 0,
            0, 0, 0, 0, 255, 255, 255, 0, 0, 0, 0, 0, 255, 255, 255, 0,
        ];
        assert_eq!(windows_rgba(None, &mask, 4), [0, 0, 0, 255, 255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 255]);
    }

    #[test]
    fn a_shape_keeps_its_id_and_a_change_gets_a_new_one() {
        let arrow = shape_id(2, 1, (0, 0), &[0, 0, 0, 255, 255, 255, 255, 255]);
        assert_eq!(arrow, shape_id(2, 1, (0, 0), &[0, 0, 0, 255, 255, 255, 255, 255]));
        assert_ne!(arrow, shape_id(2, 1, (1, 0), &[0, 0, 0, 255, 255, 255, 255, 255]));
        assert_ne!(arrow, shape_id(2, 1, (0, 0), &[0, 0, 0, 255, 0, 0, 0, 255]));
    }
}
