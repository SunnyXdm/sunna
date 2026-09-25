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
const FRAME_STATUS_COMPLETE: i32 = 0; // kCGDisplayStreamFrameStatusFrameComplete
const IOSURFACE_LOCK_READ_ONLY: u32 = 1;
/// Surfaces in the stream's pool. We hold up to three at once (the last
/// frame, a pending newer one, and the one being encoded), so the default of
/// 3 would leave the compositor nothing to draw into.
const STREAM_QUEUE_DEPTH: i32 = 4;
/// On a static screen, re-send the last frame this often. Each re-encode of
/// unchanged pixels lets the encoder refine quality, but more than a few per
/// second only burns power and bandwidth.
const IDLE_REDELIVERY: Duration = Duration::from_millis(100);

type CGDisplayModeRef = *mut c_void;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGMainDisplayID() -> u32;
    fn CGGetActiveDisplayList(max: u32, displays: *mut u32, count: *mut u32) -> i32;
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
}

#[link(name = "IOSurface", kind = "framework")]
extern "C" {
    fn IOSurfaceLock(buffer: IOSurfaceRef, options: u32, seed: *mut u32) -> i32;
    fn IOSurfaceUnlock(buffer: IOSurfaceRef, options: u32, seed: *mut u32) -> i32;
    fn IOSurfaceGetBaseAddress(buffer: IOSurfaceRef) -> *mut c_void;
    fn IOSurfaceGetBytesPerRow(buffer: IOSurfaceRef) -> usize;
    fn IOSurfaceGetWidth(buffer: IOSurfaceRef) -> usize;
    fn IOSurfaceGetHeight(buffer: IOSurfaceRef) -> usize;
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

    /// Copy the pixels out as tightly packed BGRA. Off the hot path only.
    pub fn to_bytes(&self) -> anyhow::Result<Bytes> {
        let surface = self.inner.surface;
        unsafe {
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

        let latest = Arc::new(Latest::default());
        let handler = {
            let latest = Arc::clone(&latest);
            RcBlock::new(
                move |status: i32, _display_time: u64, surface: *mut c_void, _update: *mut c_void| {
                    if status != FRAME_STATUS_COMPLETE || surface.is_null() {
                        return;
                    }
                    if let Some(frame) = hold_surface(surface) {
                        *latest.frame.lock().unwrap() = Some(frame);
                        latest.ready.notify_one();
                    }
                },
            )
        };

        // Cap delivery at the stream's fps: a 120 Hz display (or bursts of
        // updates) otherwise hands us more frames than we'd ever send.
        let properties = CFDictionary::from_CFType_pairs(&[
            (
                unsafe { CFString::wrap_under_get_rule(kCGDisplayStreamQueueDepth) },
                CFNumber::from(STREAM_QUEUE_DEPTH),
            ),
            (
                unsafe { CFString::wrap_under_get_rule(kCGDisplayStreamMinimumFrameTime) },
                CFNumber::from(1.0 / fps.max(1) as f64),
            ),
        ]);
        let stream = unsafe {
            let label = CString::new("app.sunna.capture").expect("static label");
            let queue = dispatch_queue_create(label.as_ptr(), std::ptr::null());
            CGDisplayStreamCreateWithDispatchQueue(
                CGMainDisplayID(),
                width as usize,
                height as usize,
                PIXEL_FORMAT_BGRA,
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
        tracing::info!(width, height, "screen capture started (CGDisplayStream)");

        Ok(Self {
            stream,
            _handler: handler,
            latest,
            last: None,
            next_id: 0,
            width,
            height,
            fps: fps.max(1),
        })
    }
}

/// Wrap a stream surface as a frame without touching its pixels.
fn hold_surface(surface: IOSurfaceRef) -> Option<VideoFrame> {
    // Stamp before anything else so the timestamp is as close to the
    // compositor's delivery as this API allows.
    let capture_ts_us = sunna_proto::now_us();
    let surface = unsafe { SurfaceFrame::from_display_stream(surface)? };
    Some(VideoFrame {
        frame_id: 0, // assigned by `next_frame`
        width: surface.width(),
        height: surface.height(),
        format: PixelFormat::Bgra8,
        data: FrameData::Surface(surface),
        capture_ts_us,
    })
}

impl FrameSource for ScreenSource {
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
