//! The pointer's shape on the shared screen. The viewer draws its own pointer
//! (so it moves without delay) and gives it the host's shape: resize arrows
//! over a window edge, the text beam over text, a hand over a link.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
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
#[cfg(any(target_os = "linux", target_os = "macos"))]
const POLL: Duration = Duration::from_millis(30);

/// Watch the pointer's shape from a thread of its own, calling `changed` with
/// each new one, until `stop` is set.
pub fn watch(
    stop: Arc<AtomicBool>,
    mut changed: impl FnMut(CursorImage) + Send + 'static,
) -> anyhow::Result<std::thread::JoinHandle<()>> {
    #[cfg(target_os = "linux")]
    let mut shapes = linux::Shapes::open()?;
    #[cfg(target_os = "macos")]
    let mut shapes = macos::Shapes::open()?;
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    anyhow::bail!("pointer shapes aren't available on this system");
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    Ok(std::thread::Builder::new().name("sunna-cursor".into()).spawn(move || {
        let mut last = None;
        while !stop.load(Ordering::Relaxed) {
            match shapes.next() {
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
#[cfg(any(target_os = "macos", test))]
fn shape_id(width: usize, height: usize, hot: (u16, u16), rgba: &[u8]) -> u64 {
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
                x11rb::connect(None).context("connecting to the X server (is DISPLAY set?)")?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shape_keeps_its_id_and_a_change_gets_a_new_one() {
        let arrow = shape_id(2, 1, (0, 0), &[0, 0, 0, 255, 255, 255, 255, 255]);
        assert_eq!(arrow, shape_id(2, 1, (0, 0), &[0, 0, 0, 255, 255, 255, 255, 255]));
        assert_ne!(arrow, shape_id(2, 1, (1, 0), &[0, 0, 0, 255, 255, 255, 255, 255]));
        assert_ne!(arrow, shape_id(2, 1, (0, 0), &[0, 0, 0, 255, 0, 0, 0, 255]));
    }
}
