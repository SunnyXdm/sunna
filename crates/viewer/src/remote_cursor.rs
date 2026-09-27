//! The window's pointer takes the host's shape (see `CursorShape`): resize
//! arrows over a window edge, the text beam over text, a hand over a link.
//! The pointer itself stays this Mac's, so it moves without delay.
//!
//! The `NSCursor` is built here rather than through winit, whose custom
//! cursors are one pixel per point: blurry on Retina, or twice the size.

use std::collections::HashMap;
use std::ffi::CString;

use objc2::encode::{Encode, Encoding};
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyObject, Bool};
use objc2::{class, msg_send};
use sunna_proto::messages::CursorShape;

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

unsafe impl Encode for Point {
    const ENCODING: Encoding = Encoding::Struct("CGPoint", &[f64::ENCODING, f64::ENCODING]);
}
unsafe impl Encode for Size {
    const ENCODING: Encoding = Encoding::Struct("CGSize", &[f64::ENCODING, f64::ENCODING]);
}

#[derive(Default)]
pub struct RemoteCursor {
    /// Built cursors, by shape and size (points per image pixel, in 1/1000s).
    built: HashMap<(u64, u32), Retained<AnyObject>>,
    current: Option<Retained<AnyObject>>,
    /// The shape and size in use.
    key: Option<(u64, u32)>,
}

impl RemoteCursor {
    /// Wear `shape`, sized so it matches the picture: `points_per_pixel` is
    /// how many window points one pixel of the host's screen takes.
    pub fn show(&mut self, shape: &CursorShape, points_per_pixel: f64) {
        let key = (shape.id, (points_per_pixel * 1000.0).round() as u32);
        if self.key != Some(key) {
            if !self.built.contains_key(&key) {
                // SAFETY: AppKit object creation with checked sizes.
                match unsafe { build(shape, points_per_pixel) } {
                    Some(cursor) => {
                        self.built.insert(key, cursor);
                    }
                    None => return,
                }
            }
            self.key = Some(key);
            self.current = self.built.get(&key).cloned();
        }
        self.apply();
    }

    /// Put the shape back on: macOS resets the pointer when it enters the
    /// window (winit's cursor rect), so this follows pointer moves.
    pub fn apply(&self) {
        if let Some(cursor) = &self.current {
            // SAFETY: a live NSCursor.
            unsafe {
                let _: () = msg_send![&**cursor, set];
            }
        }
    }
}

unsafe fn build(shape: &CursorShape, points_per_pixel: f64) -> Option<Retained<AnyObject>> {
    let (width, height) = (shape.width as usize, shape.height as usize);
    if width == 0 || height == 0 || shape.rgba.len() != width * height * 4 || !points_per_pixel.is_finite() {
        return None;
    }
    let space = CString::new("NSDeviceRGBColorSpace").expect("no NUL");
    let space: Retained<AnyObject> = msg_send![class!(NSString), stringWithUTF8String: space.as_ptr()];
    // Premultiplied RGBA (the default for NSBitmapImageRep), as sent.
    let rep: Allocated<AnyObject> = msg_send![class!(NSBitmapImageRep), alloc];
    let rep: Option<Retained<AnyObject>> = msg_send![
        rep,
        initWithBitmapDataPlanes: std::ptr::null_mut::<*mut u8>(),
        pixelsWide: width as isize,
        pixelsHigh: height as isize,
        bitsPerSample: 8isize,
        samplesPerPixel: 4isize,
        hasAlpha: Bool::YES,
        isPlanar: Bool::NO,
        colorSpaceName: &*space,
        bytesPerRow: (width * 4) as isize,
        bitsPerPixel: 32isize
    ];
    let rep = rep?;
    let pixels: *mut u8 = msg_send![&*rep, bitmapData];
    if pixels.is_null() {
        return None;
    }
    std::ptr::copy_nonoverlapping(shape.rgba.as_ptr(), pixels, shape.rgba.len());

    // All the pixels, drawn at the size the host's pointer has in the picture.
    let points = Size { width: width as f64 * points_per_pixel, height: height as f64 * points_per_pixel };
    let image: Allocated<AnyObject> = msg_send![class!(NSImage), alloc];
    let image: Option<Retained<AnyObject>> = msg_send![image, initWithSize: points];
    let image = image?;
    let _: () = msg_send![&*image, addRepresentation: &*rep];
    let hot = Point { x: shape.hot_x as f64 * points_per_pixel, y: shape.hot_y as f64 * points_per_pixel };
    let cursor: Allocated<AnyObject> = msg_send![class!(NSCursor), alloc];
    let cursor: Option<Retained<AnyObject>> = msg_send![cursor, initWithImage: &*image, hotSpot: hot];
    cursor
}
