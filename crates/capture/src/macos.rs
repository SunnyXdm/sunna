//! macOS screen capture via CGDisplayStream.
//!
//! Why CGDisplayStream and not ScreenCaptureKit (the research/03 §1 pick):
//! it is a small, stable C API — the right risk profile for the first
//! testable build. It delivers BGRA IOSurfaces on a dispatch queue at
//! compositor cadence with optional GPU downscaling, and it requires the same
//! Screen Recording permission SCK does. The SCK backend (zero-copy IOSurface
//! straight into VideoToolbox, per-window capture, HDR) replaces this in a
//! later milestone; the `FrameSource` seam is unchanged.
//!
//! Frames stay on the GPU: each captured IOSurface is held (see
//! [`SurfaceFrame`]) and handed to VideoToolbox as a CVPixelBuffer, with no
//! CPU pixel copy between capture and encode.

use std::ffi::{c_char, c_void, CString};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use block2::RcBlock;
use bytes::Bytes;
use core_foundation::base::TCFType;
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use core_foundation_sys::dictionary::CFDictionaryRef;
use core_foundation_sys::string::CFStringRef;

use crate::{FrameData, FrameSource, PixelFormat, VideoFrame};

type CGDisplayStreamRef = *mut c_void;
type IOSurfaceRef = *mut c_void;
type CVPixelBufferRef = *mut c_void;
type DispatchQueueT = *mut c_void;

const PIXEL_FORMAT_BGRA: i32 = 0x42475241; // 'BGRA'
/// kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange ('420v', NV12).
const PIXEL_FORMAT_NV12_VIDEO: i32 = 0x34323076;
const FRAME_STATUS_COMPLETE: i32 = 0; // kCGDisplayStreamFrameStatusFrameComplete
const IOSURFACE_LOCK_READ_ONLY: u32 = 1;
/// Surfaces in the stream's pool. We can hold up to five at once (the last
/// frame, a pending newer one, the one being encoded, and the fast-lane
/// worker's pending and current captures); the compositor needs one free to
/// draw into. Maximum allowed is 8.
const STREAM_QUEUE_DEPTH: i32 = 6;
/// On a static screen, re-send the last frame this often. Each re-encode of
/// unchanged pixels lets the encoder refine quality, but more than a few per
/// second only burns power and bandwidth.
const IDLE_REDELIVERY: Duration = Duration::from_millis(100);

type CGDisplayModeRef = *mut c_void;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct CGRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

/// kCGDisplayStreamUpdateReducedDirtyRects: changed regions, merged.
const UPDATE_REDUCED_DIRTY_RECTS: i32 = 3;
/// Fast lane only for small changes: at most this share of the frame.
/// Anything bigger is left to the video encoder.
const TILE_MAX_AREA_FRACTION: f64 = 0.04;
/// And at most this many rectangles after merging.
const TILE_MAX_RECTS: usize = 32;
/// Change detection granularity, in pixels. macOS's own dirty rects are
/// far too coarse for this (typing in VS Code reported 50-100% of the screen
/// changed), so the tile worker diffs frames itself in blocks this size.
const TILE_BLOCK: usize = 32;

/// Why captures did or didn't take the fast lane, logged every 2 s so the
/// thresholds can be tuned from real sessions.
#[derive(Default)]
struct TileDecisions {
    since: Option<std::time::Instant>,
    captures: u32,
    sent: u32,
    no_rects: u32,
    no_change: u32,
    too_large: u32,
    diff_us: Vec<u64>,
    /// Changed-area fractions (percent) and rect counts seen, for the log.
    area_pct: Vec<f64>,
    rect_counts: Vec<usize>,
}

impl TileDecisions {
    fn note(&mut self, outcome: TileOutcome, area_pct: f64, rects: usize, diff_us: u64) {
        let now = std::time::Instant::now();
        let since = *self.since.get_or_insert(now);
        self.captures += 1;
        match outcome {
            TileOutcome::Sent => self.sent += 1,
            TileOutcome::NoRects => self.no_rects += 1,
            TileOutcome::NoChange => self.no_change += 1,
            TileOutcome::TooLarge => self.too_large += 1,
        }
        if diff_us > 0 {
            self.diff_us.push(diff_us);
        }
        if rects > 0 {
            self.area_pct.push(area_pct);
            self.rect_counts.push(rects);
        }
        if now.duration_since(since) >= Duration::from_secs(2) {
            let median = |values: &mut Vec<f64>| -> f64 {
                if values.is_empty() {
                    return 0.0;
                }
                values.sort_by(|a, b| a.total_cmp(b));
                values[values.len() / 2]
            };
            let mut counts: Vec<f64> = self.rect_counts.iter().map(|&count| count as f64).collect();
            let mut diff_ms: Vec<f64> = self.diff_us.iter().map(|&us| us as f64 / 1000.0).collect();
            tracing::info!(
                captures = self.captures,
                sent = self.sent,
                no_rects = self.no_rects,
                no_change = self.no_change,
                too_large = self.too_large,
                diff_ms_median = format!("{:.2}", median(&mut diff_ms)),
                area_pct_median = format!("{:.2}", median(&mut self.area_pct)),
                area_pct_max = format!("{:.2}", self.area_pct.iter().cloned().fold(0.0, f64::max)),
                tiles_median = median(&mut counts),
                tiles_max = self.rect_counts.iter().copied().max().unwrap_or(0),
                "fast lane decisions"
            );
            *self = TileDecisions::default();
        }
    }
}

#[derive(Clone, Copy)]
enum TileOutcome {
    Sent,
    /// macOS reported nothing changed.
    NoRects,
    /// macOS reported a change, but the pixels are identical.
    NoChange,
    TooLarge,
}

static TILE_DECISIONS: Mutex<Option<TileDecisions>> = Mutex::new(None);

fn note_tile_decision(outcome: TileOutcome, area_pct: f64, rects: usize, diff_us: u64) {
    TILE_DECISIONS
        .lock()
        .unwrap()
        .get_or_insert_with(TileDecisions::default)
        .note(outcome, area_pct, rects, diff_us);
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGMainDisplayID() -> u32;
    fn CGGetActiveDisplayList(max: u32, displays: *mut u32, count: *mut u32) -> i32;
    fn CGDisplayStreamUpdateGetRects(
        update: *mut c_void,
        rect_type: i32,
        rect_count: *mut usize,
    ) -> *const CGRect;
    fn CGDisplayIsBuiltin(display: u32) -> u32;
    fn CGDisplayPixelsWide(display: u32) -> usize;
    fn CGDisplayPixelsHigh(display: u32) -> usize;
    fn CGDisplayCopyDisplayMode(display: u32) -> CGDisplayModeRef;
    fn CGDisplayModeGetPixelWidth(mode: CGDisplayModeRef) -> usize;
    fn CGDisplayModeGetPixelHeight(mode: CGDisplayModeRef) -> usize;
    fn CGDisplayStreamCreateWithDispatchQueue(
        display: u32,
        output_width: usize,
        output_height: usize,
        pixel_format: i32,
        properties: CFDictionaryRef,
        queue: DispatchQueueT,
        handler: *const c_void,
    ) -> CGDisplayStreamRef;
    fn CGDisplayStreamStart(stream: CGDisplayStreamRef) -> i32;
    fn CGDisplayStreamStop(stream: CGDisplayStreamRef) -> i32;
    fn CGPreflightScreenCaptureAccess() -> u8;
    fn CGRequestScreenCaptureAccess() -> u8;
    static kCGDisplayStreamQueueDepth: CFStringRef;
    static kCGDisplayStreamMinimumFrameTime: CFStringRef;
    static kCGDisplayStreamYCbCrMatrix: CFStringRef;
    static kCGDisplayStreamYCbCrMatrix_ITU_R_709_2: CFStringRef;
    static kCGDisplayStreamColorSpace: CFStringRef;
    static kCGDisplayStreamShowCursor: CFStringRef;
    static kCGColorSpaceDisplayP3: CFStringRef;
    fn CGColorSpaceCreateWithName(name: CFStringRef) -> *const c_void;
}

#[link(name = "IOSurface", kind = "framework")]
extern "C" {
    fn IOSurfaceLock(buffer: IOSurfaceRef, options: u32, seed: *mut u32) -> i32;
    fn IOSurfaceUnlock(buffer: IOSurfaceRef, options: u32, seed: *mut u32) -> i32;
    fn IOSurfaceGetBaseAddress(buffer: IOSurfaceRef) -> *mut c_void;
    fn IOSurfaceGetBytesPerRow(buffer: IOSurfaceRef) -> usize;
    fn IOSurfaceGetWidth(buffer: IOSurfaceRef) -> usize;
    fn IOSurfaceGetHeight(buffer: IOSurfaceRef) -> usize;
    fn IOSurfaceGetPixelFormat(buffer: IOSurfaceRef) -> u32;
    fn IOSurfaceIncrementUseCount(buffer: IOSurfaceRef);
    fn IOSurfaceDecrementUseCount(buffer: IOSurfaceRef);
}

#[link(name = "CoreVideo", kind = "framework")]
extern "C" {
    static kCVPixelBufferIOSurfacePropertiesKey: CFStringRef;
    fn CVPixelBufferCreate(
        allocator: *const c_void,
        width: usize,
        height: usize,
        pixel_format: u32,
        attributes: CFDictionaryRef,
        out: *mut CVPixelBufferRef,
    ) -> i32;
    fn CVPixelBufferCreateWithIOSurface(
        allocator: *const c_void,
        surface: IOSurfaceRef,
        attributes: CFDictionaryRef,
        out: *mut CVPixelBufferRef,
    ) -> i32;
    fn CVPixelBufferGetIOSurface(pixel_buffer: CVPixelBufferRef) -> IOSurfaceRef;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRetain(cf: *const c_void) -> *const c_void;
    fn CFRelease(cf: *const c_void);
}

// libdispatch ships in libSystem, which every binary links.
extern "C" {
    fn dispatch_queue_create(label: *const c_char, attr: *const c_void) -> DispatchQueueT;
}

/// The main display's size in points (logical resolution).
pub fn main_display_size() -> (u32, u32) {
    unsafe {
        let display = CGMainDisplayID();
        (
            CGDisplayPixelsWide(display) as u32,
            CGDisplayPixelsHigh(display) as u32,
        )
    }
}

/// The main display's size in backing pixels (retina resolution) — what
/// screen capture should default to for sharp text.
pub fn main_display_pixel_size() -> (u32, u32) {
    unsafe {
        let mode = CGDisplayCopyDisplayMode(CGMainDisplayID());
        if mode.is_null() {
            return main_display_size();
        }
        let size = (
            CGDisplayModeGetPixelWidth(mode) as u32,
            CGDisplayModeGetPixelHeight(mode) as u32,
        );
        CFRelease(mode as _);
        if size.0 == 0 || size.1 == 0 {
            main_display_size()
        } else {
            size
        }
    }
}

/// One line per active display (main first): id, built-in or external,
/// size in points and in backing pixels. Logged at startup so a session's
/// stream size can be explained from the logs.
pub fn describe_displays() -> Vec<String> {
    unsafe {
        let mut ids = [0u32; 16];
        let mut count = 0u32;
        if CGGetActiveDisplayList(ids.len() as u32, ids.as_mut_ptr(), &mut count) != 0 {
            return Vec::new();
        }
        let main = CGMainDisplayID();
        let mut lines: Vec<(bool, String)> = ids[..count as usize]
            .iter()
            .map(|&id| {
                let (pw, ph) = (CGDisplayPixelsWide(id), CGDisplayPixelsHigh(id));
                let mode = CGDisplayCopyDisplayMode(id);
                let (bw, bh) = if mode.is_null() {
                    (0, 0)
                } else {
                    let size = (CGDisplayModeGetPixelWidth(mode), CGDisplayModeGetPixelHeight(mode));
                    CFRelease(mode as _);
                    size
                };
                let kind = if CGDisplayIsBuiltin(id) != 0 { "built-in" } else { "external" };
                let main_tag = if id == main { " (main)" } else { "" };
                (id == main, format!("display {id}{main_tag}: {kind}, {pw}x{ph} pt, {bw}x{bh} px"))
            })
            .collect();
        lines.sort_by_key(|(is_main, _)| !is_main);
        lines.into_iter().map(|(_, line)| line).collect()
    }
}

/// Check (and if needed, request) the Screen Recording permission. Returns
/// false when not granted; the request makes macOS show its one-time prompt /
/// System Settings deep link for the *hosting* process (e.g. your terminal).
pub fn ensure_screen_capture_access() -> bool {
    unsafe {
        if CGPreflightScreenCaptureAccess() != 0 {
            return true;
        }
        CGRequestScreenCaptureAccess() != 0
    }
}

/// A captured frame that stays on the GPU: a retained BGRA IOSurface plus a
/// CVPixelBuffer wrapping it, ready to hand straight to VideoToolbox.
///
/// CGDisplayStream recycles a surface as soon as the frame handler returns
/// unless its use count is raised, so a frame from the stream also holds the
/// use count until dropped. Clones share one surface.
#[derive(Clone)]
pub struct SurfaceFrame {
    inner: Arc<SurfaceHold>,
}

struct SurfaceHold {
    surface: IOSurfaceRef,
    pixel_buffer: CVPixelBufferRef,
    holds_use_count: bool,
}

// IOSurface and CVPixelBuffer are reference-counted CF objects that may be
// retained, read, and released from any thread.
unsafe impl Send for SurfaceHold {}
unsafe impl Sync for SurfaceHold {}

impl Drop for SurfaceHold {
    fn drop(&mut self) {
        unsafe {
            CFRelease(self.pixel_buffer as _);
            if self.holds_use_count {
                IOSurfaceDecrementUseCount(self.surface);
            }
            CFRelease(self.surface as _);
        }
    }
}

impl SurfaceFrame {
    /// Hold a surface delivered to a CGDisplayStream frame handler.
    ///
    /// Safety: `surface` must be a valid IOSurfaceRef for the duration of the call.
    unsafe fn from_display_stream(surface: IOSurfaceRef) -> Option<Self> {
        CFRetain(surface as _);
        IOSurfaceIncrementUseCount(surface);
        let mut pixel_buffer: CVPixelBufferRef = std::ptr::null_mut();
        let status = CVPixelBufferCreateWithIOSurface(
            std::ptr::null(),
            surface,
            std::ptr::null(),
            &mut pixel_buffer,
        );
        let hold = SurfaceHold {
            surface,
            pixel_buffer,
            holds_use_count: true,
        };
        if status != 0 || pixel_buffer.is_null() {
            // Drop would release the null pixel buffer; undo the rest by hand.
            std::mem::forget(hold);
            IOSurfaceDecrementUseCount(surface);
            CFRelease(surface as _);
            return None;
        }
        Some(Self {
            inner: Arc::new(hold),
        })
    }

    /// Hold an IOSurface-backed CVPixelBuffer (e.g. a VideoToolbox decoder
    /// output). Retains it and raises the surface's use count so the
    /// decoder's buffer pool won't recycle it while it's still on screen.
    ///
    /// Safety: `pixel_buffer` must be a valid CVPixelBufferRef.
    pub unsafe fn from_pixel_buffer(pixel_buffer: *mut c_void) -> Option<Self> {
        let surface = CVPixelBufferGetIOSurface(pixel_buffer);
        if surface.is_null() {
            return None;
        }
        CFRetain(pixel_buffer as _);
        CFRetain(surface as _);
        IOSurfaceIncrementUseCount(surface);
        Some(Self {
            inner: Arc::new(SurfaceHold {
                surface,
                pixel_buffer,
                holds_use_count: true,
            }),
        })
    }

    /// The IOSurface itself (borrowed), e.g. for use as CALayer contents.
    pub fn iosurface(&self) -> *mut c_void {
        self.inner.surface
    }

    /// An IOSurface-backed BGRA frame filled from tightly packed CPU bytes.
    /// For tests and synthetic sources; real capture never copies.
    pub fn from_bgra(width: u32, height: u32, bgra: &[u8]) -> anyhow::Result<Self> {
        let (w, h) = (width as usize, height as usize);
        anyhow::ensure!(bgra.len() >= w * h * 4, "BGRA buffer smaller than {w}x{h}");
        let attributes = CFDictionary::from_CFType_pairs(&[(
            unsafe { CFString::wrap_under_get_rule(kCVPixelBufferIOSurfacePropertiesKey) }
                .as_CFType(),
            CFDictionary::<CFString, CFNumber>::from_CFType_pairs(&[]).as_CFType(),
        )]);
        unsafe {
            let mut pixel_buffer: CVPixelBufferRef = std::ptr::null_mut();
            let status = CVPixelBufferCreate(
                std::ptr::null(),
                w,
                h,
                PIXEL_FORMAT_BGRA as u32,
                attributes.as_concrete_TypeRef(),
                &mut pixel_buffer,
            );
            anyhow::ensure!(
                status == 0 && !pixel_buffer.is_null(),
                "CVPixelBufferCreate failed: {status}"
            );
            let surface = CVPixelBufferGetIOSurface(pixel_buffer);
            if surface.is_null() {
                CFRelease(pixel_buffer as _);
                anyhow::bail!("pixel buffer has no IOSurface backing");
            }
            CFRetain(surface as _);
            let frame = Self {
                inner: Arc::new(SurfaceHold {
                    surface,
                    pixel_buffer,
                    holds_use_count: false,
                }),
            };
            anyhow::ensure!(
                IOSurfaceLock(surface, 0, std::ptr::null_mut()) == 0,
                "IOSurfaceLock failed"
            );
            let stride = IOSurfaceGetBytesPerRow(surface);
            let base = IOSurfaceGetBaseAddress(surface) as *mut u8;
            for row in 0..h {
                std::ptr::copy_nonoverlapping(
                    bgra.as_ptr().add(row * w * 4),
                    base.add(row * stride),
                    w * 4,
                );
            }
            IOSurfaceUnlock(surface, 0, std::ptr::null_mut());
            Ok(frame)
        }
    }

    /// The CVPixelBuffer to hand to VideoToolbox. Borrowed: valid while this
    /// frame lives; CFRetain it to keep it longer.
    pub fn pixel_buffer(&self) -> *mut c_void {
        self.inner.pixel_buffer
    }

    pub fn width(&self) -> u32 {
        unsafe { IOSurfaceGetWidth(self.inner.surface) as u32 }
    }

    pub fn height(&self) -> u32 {
        unsafe { IOSurfaceGetHeight(self.inner.surface) as u32 }
    }

    /// Copy the pixels out as tightly packed BGRA. Off the hot path only;
    /// BGRA surfaces only.
    pub fn to_bytes(&self) -> anyhow::Result<Bytes> {
        let surface = self.inner.surface;
        unsafe {
            anyhow::ensure!(
                IOSurfaceGetPixelFormat(surface) == PIXEL_FORMAT_BGRA as u32,
                "to_bytes supports BGRA surfaces only"
            );
            anyhow::ensure!(
                IOSurfaceLock(surface, IOSURFACE_LOCK_READ_ONLY, std::ptr::null_mut()) == 0,
                "IOSurfaceLock failed"
            );
            let width = IOSurfaceGetWidth(surface);
            let height = IOSurfaceGetHeight(surface);
            let stride = IOSurfaceGetBytesPerRow(surface);
            let base = IOSurfaceGetBaseAddress(surface) as *const u8;
            let result = if base.is_null() {
                Err(anyhow::anyhow!("IOSurface has no base address"))
            } else {
                let row_bytes = width * 4;
                let mut data = vec![0u8; row_bytes * height];
                for row in 0..height {
                    std::ptr::copy_nonoverlapping(
                        base.add(row * stride),
                        data.as_mut_ptr().add(row * row_bytes),
                        row_bytes,
                    );
                }
                Ok(Bytes::from(data))
            };
            IOSurfaceUnlock(surface, IOSURFACE_LOCK_READ_ONLY, std::ptr::null_mut());
            result
        }
    }
}

impl std::fmt::Debug for SurfaceFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SurfaceFrame")
            .field("width", &self.width())
            .field("height", &self.height())
            .finish()
    }
}

#[derive(Default)]
struct Latest {
    frame: Mutex<Option<VideoFrame>>,
    ready: Condvar,
    /// Set once the fast lane is on; the capture callback feeds it.
    tiles: Mutex<Option<Arc<TileQueue>>>,
}

/// Captures the main display. Frames arrive on a dispatch queue when the
/// screen changes; `next_frame` re-delivers the last frame at the target fps
/// when the screen is static so the encoder keeps its cadence.
pub struct ScreenSource {
    stream: CGDisplayStreamRef,
    // Owned so the handler outlives the stream; CG retains its own reference.
    _handler: RcBlock<dyn Fn(i32, u64, *mut c_void, *mut c_void)>,
    latest: Arc<Latest>,
    last: Option<VideoFrame>,
    /// Output frame ids are assigned here, not in the capture handler, so
    /// re-delivered static frames and fresh frames share one monotonic series.
    next_id: u64,
    width: u32,
    height: u32,
    fps: u32,
    format: PixelFormat,
}

// Raw pointers are only touched from `next_frame`/`Drop` (single owner) and
// the dispatch-queue handler, which synchronizes through `Latest`.
unsafe impl Send for ScreenSource {}

impl ScreenSource {
    pub fn new(width: Option<u32>, height: Option<u32>, fps: u32) -> anyhow::Result<Self> {
        anyhow::ensure!(
            ensure_screen_capture_access(),
            "Screen Recording permission is not granted. macOS should have shown a prompt; \
             enable it in System Settings → Privacy & Security → Screen Recording for the \
             app that launched sunnad (e.g. your terminal), then run again."
        );

        let (native_w, native_h) = main_display_size();
        let width = width.unwrap_or(native_w).max(2) & !1; // encoders want even dims
        let height = height.unwrap_or(native_h).max(2) & !1;

        // NV12 by default: the compositor converts to YUV while it draws, so
        // VideoToolbox skips its own RGB→YUV pass (encode was ~20 ms/frame at
        // 2846x1778 on an M1 with BGRA input). SUNNA_CAPTURE_BGRA=1 restores
        // BGRA for A/B comparison.
        // The fast lane sends exact RGB tiles, so it needs BGRA capture
        // (encode time is the same either way; dogfood build 5).
        let bgra = std::env::var("SUNNA_CAPTURE_BGRA").is_ok_and(|value| value == "1")
            || crate::fast_lane_enabled();
        let (pixel_format, format) = if bgra {
            (PIXEL_FORMAT_BGRA, PixelFormat::Bgra8)
        } else {
            (PIXEL_FORMAT_NV12_VIDEO, PixelFormat::Nv12)
        };

        let latest = Arc::new(Latest::default());
        let handler = {
            let latest = Arc::clone(&latest);
            RcBlock::new(
                move |status: i32, _display_time: u64, surface: *mut c_void, update: *mut c_void| {
                    if status != FRAME_STATUS_COMPLETE || surface.is_null() {
                        return;
                    }
                    if let Some(frame) = hold_surface(surface, format) {
                        let queue = latest.tiles.lock().unwrap().clone();
                        if let (Some(queue), FrameData::Surface(held)) = (queue, &frame.data) {
                            let region = if update.is_null() {
                                None
                            } else {
                                os_dirty_region(update, frame.width as usize, frame.height as usize)
                            };
                            match region {
                                Some(region) => queue.submit(TileJob {
                                    surface: held.clone(),
                                    capture_ts_us: frame.capture_ts_us,
                                    region,
                                }),
                                None => note_tile_decision(TileOutcome::NoRects, 0.0, 0, 0),
                            }
                        }
                        *latest.frame.lock().unwrap() = Some(frame);
                        latest.ready.notify_one();
                    }
                },
            )
        };

        // Cap delivery at the stream's fps: a 120 Hz display (or bursts of
        // updates) otherwise hands us more frames than we'd ever send.
        let mut properties = vec![
            (
                unsafe { CFString::wrap_under_get_rule(kCGDisplayStreamQueueDepth) }.as_CFType(),
                CFNumber::from(STREAM_QUEUE_DEPTH).as_CFType(),
            ),
            (
                unsafe { CFString::wrap_under_get_rule(kCGDisplayStreamMinimumFrameTime) }
                    .as_CFType(),
                CFNumber::from(1.0 / fps.max(1) as f64).as_CFType(),
            ),
        ];
        // The viewer draws its own (local, zero-latency) cursor, so keep the
        // host's out of the video; otherwise there are two pointers and the
        // one in the video trails by the full round trip.
        // SUNNA_REMOTE_CURSOR=1 puts it back.
        let show_cursor = std::env::var("SUNNA_REMOTE_CURSOR").is_ok_and(|value| value == "1");
        properties.push((
            unsafe { CFString::wrap_under_get_rule(kCGDisplayStreamShowCursor) }.as_CFType(),
            core_foundation::boolean::CFBoolean::from(show_cursor).as_CFType(),
        ));
        let color = crate::ColorMode::from_env();
        if color == crate::ColorMode::DisplayP3 {
            // Convert to a known colour space so the stream can be tagged
            // honestly (see ColorMode).
            let p3 = unsafe {
                core_foundation::base::CFType::wrap_under_create_rule(CGColorSpaceCreateWithName(
                    kCGColorSpaceDisplayP3,
                ))
            };
            properties.push((
                unsafe { CFString::wrap_under_get_rule(kCGDisplayStreamColorSpace) }.as_CFType(),
                p3,
            ));
        }
        tracing::info!(?color, show_cursor, "capture colour and cursor");
        if !bgra {
            // Pin the RGB→YUV matrix so the encoder can tag it (Rec. 709).
            properties.push((
                unsafe { CFString::wrap_under_get_rule(kCGDisplayStreamYCbCrMatrix) }.as_CFType(),
                unsafe { CFString::wrap_under_get_rule(kCGDisplayStreamYCbCrMatrix_ITU_R_709_2) }
                    .as_CFType(),
            ));
        }
        let properties = CFDictionary::from_CFType_pairs(&properties);
        let stream = unsafe {
            let label = CString::new("app.sunna.capture").expect("static label");
            let queue = dispatch_queue_create(label.as_ptr(), std::ptr::null());
            CGDisplayStreamCreateWithDispatchQueue(
                CGMainDisplayID(),
                width as usize,
                height as usize,
                pixel_format,
                properties.as_concrete_TypeRef(),
                queue,
                &*handler as *const block2::Block<_> as *const c_void,
            )
        };
        anyhow::ensure!(
            !stream.is_null(),
            "CGDisplayStreamCreate failed (is Screen Recording permission granted?)"
        );
        let status = unsafe { CGDisplayStreamStart(stream) };
        if status != 0 {
            unsafe { CFRelease(stream as _) };
            anyhow::bail!("CGDisplayStreamStart failed: CGError {status}");
        }
        tracing::info!(width, height, ?format, "screen capture started (CGDisplayStream)");

        Ok(Self {
            stream,
            _handler: handler,
            latest,
            last: None,
            next_id: 0,
            width,
            height,
            fps: fps.max(1),
            format,
        })
    }
}

/// Pixel region, in output pixels: x0, y0, x1, y1 (exclusive).
type Region = (usize, usize, usize, usize);

fn union(a: Region, b: Region) -> Region {
    (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3))
}

/// Bounding box of the capture's changed rects, in output pixels, or `None`
/// if macOS reports no change.
fn os_dirty_region(update: *mut c_void, width: usize, height: usize) -> Option<Region> {
    let mut count = 0usize;
    let rects = unsafe { CGDisplayStreamUpdateGetRects(update, UPDATE_REDUCED_DIRTY_RECTS, &mut count) };
    if rects.is_null() || count == 0 {
        return None;
    }
    let rects = unsafe { std::slice::from_raw_parts(rects, count) };
    // Update rects are in display points (as in Chromium's CGDisplayStream
    // capturer); map them onto the (possibly scaled) output pixels.
    let (points_w, points_h) = main_display_size();
    let scale_x = width as f64 / points_w.max(1) as f64;
    let scale_y = height as f64 / points_h.max(1) as f64;
    let mut region: Option<Region> = None;
    for rect in rects {
        let x0 = ((rect.x * scale_x).floor() as i64).clamp(0, width as i64) as usize;
        let y0 = ((rect.y * scale_y).floor() as i64).clamp(0, height as i64) as usize;
        let x1 = (((rect.x + rect.width) * scale_x).ceil() as i64).clamp(0, width as i64) as usize;
        let y1 = (((rect.y + rect.height) * scale_y).ceil() as i64).clamp(0, height as i64) as usize;
        if x1 > x0 && y1 > y0 {
            let rect = (x0, y0, x1, y1);
            region = Some(region.map_or(rect, |region| union(region, rect)));
        }
    }
    region
}

/// A capture waiting for the tile worker. If the worker falls behind, jobs
/// merge: the newest surface, with the union of the regions it skipped.
struct TileJob {
    surface: SurfaceFrame,
    capture_ts_us: u64,
    region: Region,
}

#[derive(Default)]
struct TileQueue {
    job: Mutex<Option<TileJob>>,
    ready: Condvar,
    /// Set when the capture ends; the worker exits.
    closed: std::sync::atomic::AtomicBool,
}

impl TileQueue {
    fn submit(&self, job: TileJob) {
        let mut slot = self.job.lock().unwrap();
        let region = match slot.take() {
            Some(pending) => union(pending.region, job.region),
            None => job.region,
        };
        *slot = Some(TileJob { region, ..job });
        self.ready.notify_one();
    }
}

/// Fast-lane worker: diffs each capture against the previous one in
/// `TILE_BLOCK` blocks (only inside the region macOS says changed) and, when
/// the truly changed area is small, sends those blocks as lossless tiles.
/// Runs off the capture callback so video capture never waits on it.
fn run_tile_worker(queue: Arc<TileQueue>, sink: crate::TileSink) {
    let mut previous: Vec<u8> = Vec::new();
    let mut size = (0usize, 0usize);
    loop {
        let job = {
            let mut slot = queue.job.lock().unwrap();
            loop {
                if queue.closed.load(std::sync::atomic::Ordering::Acquire) {
                    return;
                }
                if let Some(job) = slot.take() {
                    break job;
                }
                slot = queue.ready.wait(slot).unwrap();
            }
        };
        let started = std::time::Instant::now();
        let surface = job.surface.inner.surface;
        unsafe {
            if IOSurfaceLock(surface, IOSURFACE_LOCK_READ_ONLY, std::ptr::null_mut()) != 0 {
                continue;
            }
            let (width, height) = (IOSurfaceGetWidth(surface), IOSurfaceGetHeight(surface));
            let stride = IOSurfaceGetBytesPerRow(surface);
            let base = IOSurfaceGetBaseAddress(surface) as *const u8;
            if base.is_null() {
                IOSurfaceUnlock(surface, IOSURFACE_LOCK_READ_ONLY, std::ptr::null_mut());
                continue;
            }
            let row_bytes = width * 4;
            if size != (width, height) {
                // First frame (or a size change): take a full copy, send nothing.
                size = (width, height);
                previous = vec![0u8; row_bytes * height];
                for row in 0..height {
                    let src = std::slice::from_raw_parts(base.add(row * stride), row_bytes);
                    previous[row * row_bytes..(row + 1) * row_bytes].copy_from_slice(src);
                }
                IOSurfaceUnlock(surface, IOSURFACE_LOCK_READ_ONLY, std::ptr::null_mut());
                continue;
            }
            // Compare the whole frame block by block: macOS's region only
            // says *that* something changed (its coordinates are coarse, and
            // trusting them risks missing changes), and a full compare costs a
            // few ms on this worker thread. Changed rows are copied into
            // `previous` as we go, so it mirrors the last capture we looked at.
            let _ = job.region;
            let (x0, y0, x1, y1) = (0, 0, width, height);
            let (bx0, by0) = (x0 / TILE_BLOCK, y0 / TILE_BLOCK);
            let (bx1, by1) = (x1.div_ceil(TILE_BLOCK), y1.div_ceil(TILE_BLOCK));
            let blocks_x = width.div_ceil(TILE_BLOCK);
            let mut changed = vec![false; blocks_x * height.div_ceil(TILE_BLOCK)];
            let mut changed_count = 0usize;
            for by in by0..by1 {
                let row_start = by * TILE_BLOCK;
                let row_end = (row_start + TILE_BLOCK).min(height);
                for bx in bx0..bx1 {
                    let col_start = bx * TILE_BLOCK * 4;
                    let col_end = ((bx + 1) * TILE_BLOCK).min(width) * 4;
                    let mut differs = false;
                    for row in row_start..row_end {
                        let src = std::slice::from_raw_parts(
                            base.add(row * stride + col_start),
                            col_end - col_start,
                        );
                        let dst = &mut previous[row * row_bytes + col_start..row * row_bytes + col_end];
                        if src != dst {
                            dst.copy_from_slice(src);
                            differs = true;
                        }
                    }
                    if differs {
                        changed[by * blocks_x + bx] = true;
                        changed_count += 1;
                    }
                }
            }
            IOSurfaceUnlock(surface, IOSURFACE_LOCK_READ_ONLY, std::ptr::null_mut());
            let diff_us = started.elapsed().as_micros() as u64;

            let area = changed_count * TILE_BLOCK * TILE_BLOCK;
            let area_pct = area as f64 * 100.0 / (width * height).max(1) as f64;
            if changed_count == 0 {
                note_tile_decision(TileOutcome::NoChange, 0.0, 0, diff_us);
                continue;
            }
            if area_pct > TILE_MAX_AREA_FRACTION * 100.0 {
                note_tile_decision(TileOutcome::TooLarge, area_pct, changed_count, diff_us);
                continue;
            }
            // Merge changed blocks into rectangles: horizontal runs per block
            // row, then runs stacked with identical spans in consecutive rows.
            // (Rects are in block units: x0, x1 exclusive, y0, y1 exclusive.)
            let mut rects: Vec<(usize, usize, usize, usize)> = Vec::new();
            let mut open: Vec<(usize, usize, usize, usize)> = Vec::new();
            for by in by0..by1 {
                let mut runs = Vec::new();
                let mut bx = bx0;
                while bx < bx1 {
                    if !changed[by * blocks_x + bx] {
                        bx += 1;
                        continue;
                    }
                    let run_start = bx;
                    while bx < bx1 && changed[by * blocks_x + bx] {
                        bx += 1;
                    }
                    runs.push((run_start, bx));
                }
                let mut next_open = Vec::with_capacity(runs.len());
                for (start, end) in runs {
                    if let Some(index) = open
                        .iter()
                        .position(|&(x0, x1, _, y1)| x0 == start && x1 == end && y1 == by)
                    {
                        let (x0, x1, y0, _) = open.swap_remove(index);
                        next_open.push((x0, x1, y0, by + 1));
                    } else {
                        next_open.push((start, end, by, by + 1));
                    }
                }
                rects.append(&mut open); // runs that didn't continue are done
                open = next_open;
            }
            rects.append(&mut open);
            if rects.len() > TILE_MAX_RECTS {
                note_tile_decision(TileOutcome::TooLarge, area_pct, rects.len(), diff_us);
                continue;
            }
            // Cut tiles from `previous` (which now holds this capture's pixels).
            let mut tiles = Vec::with_capacity(rects.len());
            for (bx_start, bx_end, by_start, by_end) in rects {
                let px0 = bx_start * TILE_BLOCK;
                let px1 = (bx_end * TILE_BLOCK).min(width);
                let py0 = by_start * TILE_BLOCK;
                let py1 = (by_end * TILE_BLOCK).min(height);
                let (tile_w, tile_h) = (px1 - px0, py1 - py0);
                let mut pixels = Vec::with_capacity(tile_w * tile_h * 4);
                for row in py0..py1 {
                    pixels.extend_from_slice(&previous[row * row_bytes + px0 * 4..row * row_bytes + px1 * 4]);
                }
                // Opaque: the capture's alpha byte isn't meaningful.
                for alpha in pixels.iter_mut().skip(3).step_by(4) {
                    *alpha = 255;
                }
                if let Ok(qoi) = qoi::encode_to_vec(&pixels, tile_w as u32, tile_h as u32) {
                    tiles.push(sunna_proto::tiles::Tile {
                        x: px0 as u32,
                        y: py0 as u32,
                        width: tile_w as u32,
                        height: tile_h as u32,
                        qoi,
                    });
                }
            }
            note_tile_decision(TileOutcome::Sent, area_pct, tiles.len(), diff_us);
            sink(sunna_proto::tiles::TileBatch {
                capture_ts_us: job.capture_ts_us,
                stream_width: width as u32,
                stream_height: height as u32,
                tiles,
            });
        }
    }
}

/// Wrap a stream surface as a frame without touching its pixels.
fn hold_surface(surface: IOSurfaceRef, format: PixelFormat) -> Option<VideoFrame> {
    // Stamp before anything else so the timestamp is as close to the
    // compositor's delivery as this API allows.
    let capture_ts_us = sunna_proto::now_us();
    let surface = unsafe { SurfaceFrame::from_display_stream(surface)? };
    Some(VideoFrame {
        frame_id: 0, // assigned by `next_frame`
        width: surface.width(),
        height: surface.height(),
        format,
        data: FrameData::Surface(surface),
        capture_ts_us,
    })
}

impl FrameSource for ScreenSource {
    fn set_tile_sink(&mut self, sink: crate::TileSink) {
        if self.format != PixelFormat::Bgra8 {
            tracing::warn!("fast lane needs BGRA capture; tiles disabled");
            return;
        }
        let queue = Arc::new(TileQueue::default());
        let worker_queue = Arc::clone(&queue);
        match std::thread::Builder::new()
            .name("sunna-tiles".into())
            .spawn(move || run_tile_worker(worker_queue, sink))
        {
            Ok(_) => *self.latest.tiles.lock().unwrap() = Some(queue),
            Err(error) => tracing::warn!(%error, "couldn't start the tile worker"),
        }
    }

    fn next_frame(&mut self) -> anyhow::Result<VideoFrame> {
        let interval = IDLE_REDELIVERY.max(Duration::from_secs_f64(1.0 / self.fps as f64));
        let mut guard = self.latest.frame.lock().unwrap();
        loop {
            if let Some(mut frame) = guard.take() {
                drop(guard);
                frame.frame_id = self.next_id;
                self.next_id += 1;
                self.last = Some(frame.clone());
                return Ok(frame);
            }
            let (next_guard, timeout) = self.latest.ready.wait_timeout(guard, interval).unwrap();
            guard = next_guard;
            if timeout.timed_out() {
                // Static screen: re-deliver the last frame with a fresh id and
                // timestamp so cadence (and latency stats) stay honest.
                if let Some(mut frame) = self.last.clone() {
                    frame.frame_id = self.next_id;
                    self.next_id += 1;
                    frame.capture_ts_us = sunna_proto::now_us();
                    self.last = Some(frame.clone());
                    return Ok(frame);
                }
                // No frame ever arrived yet: keep waiting (first frame can
                // take a moment after stream start).
            }
        }
    }

    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn fps(&self) -> u32 {
        self.fps
    }
}

impl Drop for ScreenSource {
    fn drop(&mut self) {
        if let Some(queue) = self.latest.tiles.lock().unwrap().take() {
            queue.closed.store(true, std::sync::atomic::Ordering::Release);
            let _guard = queue.job.lock().unwrap();
            queue.ready.notify_all();
        }
        unsafe {
            CGDisplayStreamStop(self.stream);
            CFRelease(self.stream as _);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_frame_roundtrips_pixels() {
        let (width, height) = (37u32, 11u32); // odd width: rows are padded
        let bgra: Vec<u8> = (0..width * height * 4).map(|i| (i % 251) as u8).collect();
        let frame = SurfaceFrame::from_bgra(width, height, &bgra).unwrap();
        assert_eq!((frame.width(), frame.height()), (width, height));
        assert!(!frame.pixel_buffer().is_null());
        let copy = frame.clone();
        drop(frame);
        assert_eq!(&copy.to_bytes().unwrap()[..], &bgra[..]);
    }
}
