//! macOS presenter: decoded IOSurfaces become the contents of a CALayer.
//!
//! Zero copies from decode to screen: VideoToolbox writes BGRA into an
//! IOSurface, the layer displays that surface, and Core Animation draws it
//! 1:1 when it fits the window (scaled down on the GPU otherwise). Contents
//! are set as soon as a frame arrives, not on the next winit redraw.
//!
//! Fast-lane tiles (small changed regions sent losslessly ahead of the video)
//! are sublayers over the video, each retired once a video frame captured at
//! the same time or later is on screen — so a tile never hides newer content.

use std::collections::VecDeque;
use std::ffi::{c_void, CString};

use objc2::encode::{Encode, Encoding};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool};
use objc2::{class, msg_send};
use sunna_capture::macos::SurfaceFrame;
use sunna_proto::tiles::TileBatch;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

use crate::viewer::stream_scale;

/// Frames kept alive after being shown. Core Animation may still be reading
/// a surface for a frame or two after it's replaced; holding it (and its use
/// count) stops the decoder's pool from overwriting it on screen.
const KEEP_ALIVE: usize = 4;
/// Upper bound on tile overlays alive at once (oldest retired first).
const MAX_OVERLAYS: usize = 512;

#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CGSize {
    width: f64,
    height: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

unsafe impl Encode for CGPoint {
    const ENCODING: Encoding = Encoding::Struct("CGPoint", &[f64::ENCODING, f64::ENCODING]);
}
unsafe impl Encode for CGSize {
    const ENCODING: Encoding = Encoding::Struct("CGSize", &[f64::ENCODING, f64::ENCODING]);
}
unsafe impl Encode for CGRect {
    const ENCODING: Encoding = Encoding::Struct("CGRect", &[CGPoint::ENCODING, CGSize::ENCODING]);
}

/// kCGImageAlphaNoneSkipFirst | kCGBitmapByteOrder32Little: B, G, R, X bytes.
const BITMAP_BGRX: u32 = 6 | (2 << 12);

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    static kCGColorSpaceDisplayP3: *const c_void;
    static kCGColorSpaceSRGB: *const c_void;
    fn CGColorSpaceCreateWithName(name: *const c_void) -> *mut c_void;
    fn CGColorSpaceRelease(space: *mut c_void);
    fn CGDataProviderCreateWithCFData(data: *const c_void) -> *mut c_void;
    fn CGDataProviderRelease(provider: *mut c_void);
    #[allow(clippy::too_many_arguments)]
    fn CGImageCreate(
        width: usize,
        height: usize,
        bits_per_component: usize,
        bits_per_pixel: usize,
        bytes_per_row: usize,
        space: *mut c_void,
        bitmap_info: u32,
        provider: *mut c_void,
        decode: *const f64,
        should_interpolate: bool,
        intent: i32,
    ) -> *mut c_void;
    fn CGImageRelease(image: *mut c_void);
    fn CGColorCreateSRGB(red: f64, green: f64, blue: f64, alpha: f64) -> *mut c_void;
    fn CGColorRelease(color: *mut c_void);
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFDataCreate(allocator: *const c_void, bytes: *const u8, length: isize) -> *const c_void;
    fn CFRelease(cf: *const c_void);
}

pub struct LayerPresenter {
    layer: Retained<AnyObject>,
    recent: VecDeque<SurfaceFrame>,
    stream_size: (u32, u32),
    /// Currently drawing 1:1 (true) or scaled to fit (false).
    one_to_one: Option<bool>,
    window_px: (u32, u32),
    backing_scale: f64,
    /// Capture time of the video frame on screen.
    video_ts_us: Option<u64>,
    /// Tile sublayers with the capture time they came from.
    overlays: VecDeque<(u64, Retained<AnyObject>)>,
    /// The capture's colour space (tiles are raw capture pixels), so tiles
    /// and video render identically.
    color_space: *mut c_void,
    /// SUNNA_TILE_DEBUG=1 outlines each tile, to check placement.
    debug_tiles: bool,
    /// Tiles placed so far (the first few are logged with their geometry).
    placed: u64,
}

fn ns_string(text: &str) -> Retained<AnyObject> {
    let text = CString::new(text).expect("no interior NUL");
    unsafe { msg_send![class!(NSString), stringWithUTF8String: text.as_ptr()] }
}

impl LayerPresenter {
    pub fn new(window: &Window, stream_size: (u32, u32)) -> anyhow::Result<Self> {
        let RawWindowHandle::AppKit(handle) = window.window_handle()?.as_raw() else {
            anyhow::bail!("not an AppKit window");
        };
        let view = handle.ns_view.as_ptr() as *mut AnyObject;
        let layer: Retained<AnyObject> = unsafe { msg_send![class!(CALayer), new] };
        unsafe {
            // Layer-hosting view: set the layer first, then turn layers on.
            let _: () = msg_send![view, setLayer: &*layer];
            let _: () = msg_send![view, setWantsLayer: Bool::YES];
        }
        let mut presenter = Self {
            layer,
            recent: VecDeque::with_capacity(KEEP_ALIVE + 1),
            stream_size,
            one_to_one: None,
            window_px: (1, 1),
            backing_scale: window.scale_factor(),
            video_ts_us: None,
            overlays: VecDeque::new(),
            color_space: unsafe {
                CGColorSpaceCreateWithName(match sunna_capture::ColorMode::from_env() {
                    sunna_capture::ColorMode::DisplayP3 => kCGColorSpaceDisplayP3,
                    _ => kCGColorSpaceSRGB,
                })
            },
            debug_tiles: std::env::var("SUNNA_TILE_DEBUG").is_ok_and(|value| value == "1"),
            placed: 0,
        };
        let size = window.inner_size();
        presenter.fit(window, (size.width, size.height));
        Ok(presenter)
    }

    /// Draw the stream pixel-for-pixel (centred) when it fits the window,
    /// otherwise scale it down to fit. Scaling screen content blurs text,
    /// so 1:1 wins whenever possible.
    pub fn fit(&mut self, window: &Window, window_px: (u32, u32)) {
        self.backing_scale = window.scale_factor();
        if self.window_px != window_px {
            // Overlays were placed for the old geometry; the video catches up.
            self.clear_overlays();
            self.window_px = window_px;
        }
        let stream = (self.stream_size.0 as f64, self.stream_size.1 as f64);
        let fits = stream_scale(stream, (window_px.0 as f64, window_px.1 as f64)) == 1.0;
        if self.one_to_one == Some(fits) {
            return;
        }
        self.one_to_one = Some(fits);
        let gravity = if fits { "center" } else { "resizeAspect" }; // kCAGravity*
        unsafe {
            let gravity = ns_string(gravity);
            let _: () = msg_send![&*self.layer, setContentsGravity: &*gravity];
            // One content pixel per screen pixel on Retina displays.
            let _: () = msg_send![&*self.layer, setContentsScale: self.backing_scale];
        }
        tracing::info!(
            one_to_one = fits,
            stream = format!("{}x{}", self.stream_size.0, self.stream_size.1),
            window = format!("{}x{}", window_px.0, window_px.1),
            "viewer scaling"
        );
    }

    /// Show `frame` (captured at `capture_ts_us`) now, retiring tiles it
    /// already contains. Must run on the main thread.
    pub fn show(&mut self, frame: SurfaceFrame, capture_ts_us: u64) {
        unsafe {
            let _: () = msg_send![class!(CATransaction), begin];
            // No implicit cross-fade between frames.
            let _: () = msg_send![class!(CATransaction), setDisableActions: Bool::YES];
            let _: () = msg_send![&*self.layer, setContents: frame.iosurface() as *mut AnyObject];
            while self
                .overlays
                .front()
                .is_some_and(|(tile_ts, _)| *tile_ts <= capture_ts_us)
            {
                if let Some((_, overlay)) = self.overlays.pop_front() {
                    let _: () = msg_send![&*overlay, removeFromSuperlayer];
                }
            }
            let _: () = msg_send![class!(CATransaction), commit];
        }
        self.video_ts_us = Some(capture_ts_us);
        self.recent.push_back(frame);
        while self.recent.len() > KEEP_ALIVE {
            self.recent.pop_front();
        }
    }

    /// Draw a fast-lane batch over the video, unless the video on screen is
    /// already as new. Must run on the main thread.
    pub fn add_tiles(&mut self, window: &Window, batch: TileBatch) {
        let Some(video_ts) = self.video_ts_us else { return };
        if batch.capture_ts_us <= video_ts
            || (batch.stream_width, batch.stream_height) != self.stream_size
        {
            return;
        }
        self.backing_scale = window.scale_factor();
        unsafe {
            let _: () = msg_send![class!(CATransaction), begin];
            let _: () = msg_send![class!(CATransaction), setDisableActions: Bool::YES];
            for tile in &batch.tiles {
                if let Some(overlay) = self.tile_layer(tile) {
                    let _: () = msg_send![&*self.layer, addSublayer: &*overlay];
                    self.overlays.push_back((batch.capture_ts_us, overlay));
                }
            }
            while self.overlays.len() > MAX_OVERLAYS {
                if let Some((_, overlay)) = self.overlays.pop_front() {
                    let _: () = msg_send![&*overlay, removeFromSuperlayer];
                }
            }
            let _: () = msg_send![class!(CATransaction), commit];
        }
    }

    fn clear_overlays(&mut self) {
        unsafe {
            for (_, overlay) in self.overlays.drain(..) {
                let _: () = msg_send![&*overlay, removeFromSuperlayer];
            }
        }
    }

    /// Where a tile (stream pixels, top-left origin) lands in the root
    /// layer, in points. Uses the layer's real bounds (the full-screen window
    /// changes height as the menu bar shows and hides) and its real
    /// orientation, rather than assuming either.
    fn tile_frame(&mut self, x: u32, y: u32, width: u32, height: u32) -> CGRect {
        let bounds: CGRect = unsafe { msg_send![&*self.layer, bounds] };
        let flipped: bool = unsafe { msg_send![&*self.layer, isGeometryFlipped] };
        let points = self.backing_scale.max(1.0);
        // Everything below in backing pixels, top-left origin.
        let (layer_w, layer_h) = (bounds.size.width * points, bounds.size.height * points);
        let (stream_w, stream_h) = (self.stream_size.0 as f64, self.stream_size.1 as f64);
        // Same rule as the contents gravity.
        let scale = stream_scale((stream_w, stream_h), (layer_w, layer_h));
        let origin_x = (layer_w - stream_w * scale) / 2.0;
        let origin_y = (layer_h - stream_h * scale) / 2.0;
        let px_x = origin_x + x as f64 * scale;
        let px_y = origin_y + y as f64 * scale;
        let px_w = width as f64 * scale;
        let px_h = height as f64 * scale;
        // Unflipped Core Animation geometry has its origin bottom-left.
        let y_points = if flipped {
            px_y / points
        } else {
            (layer_h - (px_y + px_h)) / points
        };
        let frame = CGRect {
            origin: CGPoint {
                x: bounds.origin.x + px_x / points,
                y: bounds.origin.y + y_points,
            },
            size: CGSize {
                width: px_w / points,
                height: px_h / points,
            },
        };
        if self.placed < 3 {
            tracing::info!(
                tile = format!("{x},{y} {width}x{height}"),
                layer_bounds_pt = format!(
                    "{},{} {}x{}",
                    bounds.origin.x, bounds.origin.y, bounds.size.width, bounds.size.height
                ),
                flipped,
                scale,
                frame_pt = format!(
                    "{:.1},{:.1} {:.1}x{:.1}",
                    frame.origin.x, frame.origin.y, frame.size.width, frame.size.height
                ),
                "tile placement"
            );
        }
        self.placed += 1;
        frame
    }

    fn tile_layer(&mut self, tile: &sunna_proto::tiles::Tile) -> Option<Retained<AnyObject>> {
        // Validate geometry before decoding (and allocating) anything.
        let inside = tile.width > 0
            && tile.height > 0
            && tile.x.checked_add(tile.width).is_some_and(|x1| x1 <= self.stream_size.0)
            && tile.y.checked_add(tile.height).is_some_and(|y1| y1 <= self.stream_size.1);
        let header = qoi::decode_header(&tile.qoi).ok()?;
        if !inside || (header.width, header.height) != (tile.width, tile.height) {
            return None;
        }
        let (header, pixels) = qoi::decode_to_vec(&tile.qoi).ok()?;
        let (width, height) = (header.width as usize, header.height as usize);
        if header.channels.as_u8() != 4
            || (width, height) != (tile.width as usize, tile.height as usize)
            || pixels.len() != width * height * 4
        {
            return None;
        }
        unsafe {
            let data = CFDataCreate(std::ptr::null(), pixels.as_ptr(), pixels.len() as isize);
            if data.is_null() {
                return None;
            }
            let provider = CGDataProviderCreateWithCFData(data);
            CFRelease(data);
            if provider.is_null() {
                return None;
            }
            let image = CGImageCreate(
                width,
                height,
                8,
                32,
                width * 4,
                self.color_space,
                BITMAP_BGRX,
                provider,
                std::ptr::null(),
                false,
                0,
            );
            CGDataProviderRelease(provider);
            if image.is_null() {
                return None;
            }
            let overlay: Retained<AnyObject> = msg_send![class!(CALayer), new];
            let _: () = msg_send![&*overlay, setContents: image as *mut AnyObject];
            CGImageRelease(image); // the layer holds its own reference
            let frame = self.tile_frame(tile.x, tile.y, tile.width, tile.height);
            let _: () = msg_send![&*overlay, setFrame: frame];
            if self.debug_tiles {
                let color = CGColorCreateSRGB(1.0, 0.2, 0.6, 0.9);
                let _: () = msg_send![&*overlay, setBorderColor: color as *mut AnyObject];
                let _: () = msg_send![&*overlay, setBorderWidth: 1.0f64];
                CGColorRelease(color);
            }
            Some(overlay)
        }
    }
}

impl Drop for LayerPresenter {
    fn drop(&mut self) {
        if !self.color_space.is_null() {
            unsafe { CGColorSpaceRelease(self.color_space) };
        }
    }
}
