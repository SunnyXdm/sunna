//! Decoding on Android: the phone's own video decoder (MediaCodec) draws
//! straight onto the session's SurfaceView; the picture never passes through
//! our memory. The app hands the surface over as it comes and goes
//! ([`set_surface`]): frames that arrive while there's none (the app is in
//! the background) are dropped, and the next surface starts at a keyframe.
//!
//! Decoded frames are shown the moment they come out, newest only: when
//! several are ready at once (a burst after a stall), the older ones are
//! skipped rather than queued behind each other.

use std::collections::VecDeque;
use std::ffi::{c_char, c_int, c_long, c_void, CStr, CString};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use anyhow::{bail, ensure};
use bytes::Bytes;
use sunna_capture::{FrameData, PixelFormat};

use crate::{Color, DecodedFrame, Decoder};

#[repr(C)]
pub struct ANativeWindow {
    _private: [u8; 0],
}

#[repr(C)]
struct AMediaCodec {
    _private: [u8; 0],
}

#[repr(C)]
struct AMediaFormat {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct BufferInfo {
    offset: i32,
    size: i32,
    presentation_time_us: i64,
    flags: u32,
}

#[repr(C)]
struct Timespec {
    seconds: c_long,
    nanoseconds: c_long,
}

const OK: c_int = 0;
const TRY_AGAIN_LATER: isize = -1;
const OUTPUT_FORMAT_CHANGED: isize = -2;
const OUTPUT_BUFFERS_CHANGED: isize = -3;
const CLOCK_MONOTONIC: c_int = 1;

#[link(name = "mediandk")]
extern "C" {
    fn AMediaCodec_createDecoderByType(mime: *const c_char) -> *mut AMediaCodec;
    fn AMediaCodec_configure(
        codec: *mut AMediaCodec,
        format: *const AMediaFormat,
        surface: *mut ANativeWindow,
        crypto: *mut c_void,
        flags: u32,
    ) -> c_int;
    fn AMediaCodec_start(codec: *mut AMediaCodec) -> c_int;
    fn AMediaCodec_stop(codec: *mut AMediaCodec) -> c_int;
    fn AMediaCodec_delete(codec: *mut AMediaCodec) -> c_int;
    fn AMediaCodec_dequeueInputBuffer(codec: *mut AMediaCodec, timeout_us: i64) -> isize;
    fn AMediaCodec_getInputBuffer(codec: *mut AMediaCodec, index: usize, size: *mut usize) -> *mut u8;
    fn AMediaCodec_queueInputBuffer(codec: *mut AMediaCodec, index: usize, offset: c_long, size: usize, time_us: u64, flags: u32) -> c_int;
    fn AMediaCodec_dequeueOutputBuffer(codec: *mut AMediaCodec, info: *mut BufferInfo, timeout_us: i64) -> isize;
    fn AMediaCodec_getOutputFormat(codec: *mut AMediaCodec) -> *mut AMediaFormat;
    fn AMediaCodec_releaseOutputBuffer(codec: *mut AMediaCodec, index: usize, render: bool) -> c_int;
    fn AMediaCodec_releaseOutputBufferAtTime(codec: *mut AMediaCodec, index: usize, timestamp_ns: i64) -> c_int;
    fn AMediaCodec_setOutputSurface(codec: *mut AMediaCodec, surface: *mut ANativeWindow) -> c_int;
    fn AMediaCodec_getName(codec: *mut AMediaCodec, name: *mut *mut c_char) -> c_int;
    fn AMediaCodec_releaseName(codec: *mut AMediaCodec, name: *mut c_char);
    fn AMediaFormat_new() -> *mut AMediaFormat;
    fn AMediaFormat_delete(format: *mut AMediaFormat) -> c_int;
    fn AMediaFormat_setString(format: *mut AMediaFormat, name: *const c_char, value: *const c_char);
    fn AMediaFormat_setInt32(format: *mut AMediaFormat, name: *const c_char, value: i32);
    fn AMediaFormat_setBuffer(format: *mut AMediaFormat, name: *const c_char, data: *const c_void, size: usize);
    fn AMediaFormat_getInt32(format: *mut AMediaFormat, name: *const c_char, out: *mut i32) -> bool;
}

#[link(name = "android")]
extern "C" {
    fn ANativeWindow_release(window: *mut ANativeWindow);
}

extern "C" {
    fn clock_gettime(clock: c_int, time: *mut Timespec) -> c_int;
}

fn monotonic_ns() -> i64 {
    let mut time = Timespec { seconds: 0, nanoseconds: 0 };
    // SAFETY: plain libc call into a local.
    unsafe { clock_gettime(CLOCK_MONOTONIC, &mut time) };
    time.seconds * 1_000_000_000 + time.nanoseconds
}

/// A surface to draw on: one reference to the app's `ANativeWindow`.
struct Window(*mut ANativeWindow);

// SAFETY: ANativeWindow references may be used and released from any thread.
unsafe impl Send for Window {}

impl Drop for Window {
    fn drop(&mut self) {
        // SAFETY: we own one reference.
        unsafe { ANativeWindow_release(self.0) }
    }
}

/// The surface the app shows the session on, and the decoder drawing onto it.
struct Slot {
    window: Option<Window>,
    active: Option<Arc<Active>>,
}

static SLOT: Mutex<Slot> = Mutex::new(Slot { window: None, active: None });

/// Give the decoder the session's surface: an `ANativeWindow` reference
/// (from `ANativeWindow_fromSurface`) that this takes over, or null when the
/// surface goes away. Once this returns, nothing draws on the old surface.
///
/// # Safety
/// `window` is null or a valid `ANativeWindow` reference the caller owns.
pub unsafe fn set_surface(window: *mut ANativeWindow) {
    let window = (!window.is_null()).then(|| Window(window));
    let mut slot = SLOT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(active) = slot.active.take() {
        // A new surface for the same picture, if the decoder can switch;
        // otherwise it stops, and the next keyframe starts a new one.
        let moved = window.as_ref().is_some_and(|window| active.move_to(window));
        if moved {
            slot.active = Some(active);
        } else {
            active.stop();
        }
    }
    slot.window = window;
}

/// What the decoder has been doing, for the stats bar.
#[derive(Debug, Clone, Default)]
pub struct DecodeStats {
    /// Frames put on the screen, and frames skipped for a newer one.
    pub shown: u64,
    pub skipped: u64,
    /// Into the decoder → out onto the screen, median over the last second.
    pub delay_ms: Option<f64>,
    /// The decoder's name: "c2.qti.hevc.decoder", "c2.android.avc.decoder"...
    pub name: String,
}

#[derive(Default)]
struct Shared {
    stats: DecodeStats,
    /// (into the decoder, out of it) in the last second, for `delay_ms`.
    delays: VecDeque<(Instant, f64)>,
}

static STATS: Mutex<Option<Shared>> = Mutex::new(None);

pub fn stats() -> DecodeStats {
    let mut shared = STATS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let shared = shared.get_or_insert_with(Shared::default);
    let horizon = Instant::now() - Duration::from_secs(1);
    while shared.delays.front().is_some_and(|(at, _)| *at < horizon) {
        shared.delays.pop_front();
    }
    let mut delays: Vec<f64> = shared.delays.iter().map(|(_, delay)| *delay).collect();
    delays.sort_by(f64::total_cmp);
    DecodeStats { delay_ms: delays.get(delays.len() / 2).copied(), ..shared.stats.clone() }
}

fn note(change: impl FnOnce(&mut Shared)) {
    let mut shared = STATS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    change(shared.get_or_insert_with(Shared::default));
}

struct Codec(*mut AMediaCodec);

// SAFETY: MediaCodec takes input and output calls from different threads;
// stopping it is kept apart from both by `Active::codec`'s lock.
unsafe impl Send for Codec {}
unsafe impl Sync for Codec {}

/// One running MediaCodec and the thread showing what it decodes.
struct Active {
    codec: RwLock<Option<Codec>>,
    /// Stopped by `set_surface`, or failed: the decoder makes a new one.
    dead: AtomicBool,
    /// When each input went in, by its presentation time (our sequence).
    queued: Mutex<VecDeque<(u64, Instant)>>,
    /// A frame bigger than the input buffers: the size the next one needs.
    needs_input: AtomicU64,
}

/// MediaCodec format keys that ask for frames out as soon as they're in.
/// Each vendor has its own; unknown keys are ignored, and if a decoder
/// refuses them anyway it's configured again without.
const VENDOR_LOW_LATENCY: &[(&str, i32)] = &[
    ("vendor.qti-ext-dec-low-latency.enable", 1),
    ("vendor.qti-ext-dec-picture-order.enable", 1),
    ("vendor.rtc-ext-dec-low-latency.enable", 1),
    ("vendor.hisi-ext-low-latency-video-dec.video-scene-for-low-latency-req", 1),
    ("vendor.hisi-ext-low-latency-video-dec.video-scene-for-low-latency-rdy", -1),
    ("vendor.low-latency.enable", 1),
];

fn key(name: &str) -> CString {
    CString::new(name).expect("format keys have no NULs")
}

impl Active {
    /// `config` is the stream's parameter sets ("csd-0", "csd-1"): they come
    /// with every keyframe too, but some decoders want them up front.
    fn start(mime: &str, width: u32, height: u32, max_input: usize, config: &[(&str, Vec<u8>)], window: &Window) -> anyhow::Result<Arc<Self>> {
        let mime_c = key(mime);
        // Every low-latency hint first, then the standard ones, then none.
        let mut last_error = 0;
        for tier in 0..3 {
            // SAFETY: plain constructor; checked below.
            let raw = unsafe { AMediaCodec_createDecoderByType(mime_c.as_ptr()) };
            ensure!(!raw.is_null(), "this phone has no {mime} decoder");
            // SAFETY: `format` lives until deleted below; keys are C strings.
            let status = unsafe {
                let format = AMediaFormat_new();
                AMediaFormat_setString(format, key("mime").as_ptr(), mime_c.as_ptr());
                AMediaFormat_setInt32(format, key("width").as_ptr(), width as i32);
                AMediaFormat_setInt32(format, key("height").as_ptr(), height as i32);
                AMediaFormat_setInt32(format, key("max-input-size").as_ptr(), max_input as i32);
                for (name, data) in config {
                    AMediaFormat_setBuffer(format, key(name).as_ptr(), data.as_ptr().cast(), data.len());
                }
                if tier < 2 {
                    AMediaFormat_setInt32(format, key("low-latency").as_ptr(), 1);
                    // Real-time, at full clock.
                    AMediaFormat_setInt32(format, key("priority").as_ptr(), 0);
                }
                if tier == 0 {
                    AMediaFormat_setInt32(format, key("operating-rate").as_ptr(), i16::MAX as i32);
                    for (name, value) in VENDOR_LOW_LATENCY {
                        AMediaFormat_setInt32(format, key(name).as_ptr(), *value);
                    }
                }
                let status = AMediaCodec_configure(raw, format, window.0, std::ptr::null_mut(), 0);
                AMediaFormat_delete(format);
                if status == OK {
                    AMediaCodec_start(raw)
                } else {
                    status
                }
            };
            if status != OK {
                // SAFETY: created above, not started or failed to start.
                unsafe { AMediaCodec_delete(raw) };
                last_error = status;
                tracing::debug!(tier, status, "decoder refused this configuration");
                continue;
            }
            let name = codec_name(raw);
            tracing::info!(mime, width, height, decoder = %name, tier, "MediaCodec decoder started");
            note(|shared| shared.stats.name = name);
            let active = Arc::new(Self {
                codec: RwLock::new(Some(Codec(raw))),
                dead: AtomicBool::new(false),
                queued: Mutex::new(VecDeque::new()),
                needs_input: AtomicU64::new(0),
            });
            let output = Arc::clone(&active);
            std::thread::Builder::new().name("sunna-show".into()).spawn(move || output.show_loop())?;
            return Ok(active);
        }
        bail!("couldn't start the {mime} decoder (error {last_error})")
    }

    fn is_dead(&self) -> bool {
        self.dead.load(Ordering::Acquire)
    }

    /// Stop decoding; waits for a call in progress to finish.
    fn stop(&self) {
        self.dead.store(true, Ordering::Release);
        let codec = self.codec.write().unwrap_or_else(|poisoned| poisoned.into_inner()).take();
        if let Some(codec) = codec {
            // SAFETY: no other call can be using it: we hold it alone now.
            unsafe {
                AMediaCodec_stop(codec.0);
                AMediaCodec_delete(codec.0);
            }
        }
    }

    /// Carry on onto another surface.
    fn move_to(&self, window: &Window) -> bool {
        let codec = self.codec.read().unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(codec) = codec.as_ref() else { return false };
        // SAFETY: a live codec and window.
        let status = unsafe { AMediaCodec_setOutputSurface(codec.0, window.0) };
        if status != OK {
            tracing::info!(status, "decoder can't switch surfaces; starting over at a keyframe");
        }
        status == OK
    }

    /// Put one access unit into the decoder.
    fn queue(&self, data: &[u8], sequence: u64) -> anyhow::Result<()> {
        // The decoder hands input buffers back as it finishes frames; a short
        // wait covers a busy moment, a long one means it has stalled. The lock
        // is taken per wait, so stopping (a surface going away) never waits
        // on more than one.
        let deadline = Instant::now() + Duration::from_millis(100);
        loop {
            let guard = self.codec.read().unwrap_or_else(|poisoned| poisoned.into_inner());
            let Some(codec) = guard.as_ref() else { bail!("the decoder stopped") };
            // SAFETY: a live codec.
            let index = unsafe { AMediaCodec_dequeueInputBuffer(codec.0, 10_000) };
            if index >= 0 {
                return self.fill(codec, index as usize, data, sequence);
            }
            if index != TRY_AGAIN_LATER {
                self.dead.store(true, Ordering::Release);
                bail!("decoder input failed ({index})");
            }
            drop(guard);
            if Instant::now() > deadline {
                // A new decoder at the next keyframe, rather than this one forever.
                self.dead.store(true, Ordering::Release);
                bail!("decoder stalled: no input buffer for 100 ms");
            }
        }
    }

    fn fill(&self, codec: &Codec, index: usize, data: &[u8], sequence: u64) -> anyhow::Result<()> {
        let mut capacity = 0usize;
        // SAFETY: the buffer at `index` is ours until queued.
        let buffer = unsafe { AMediaCodec_getInputBuffer(codec.0, index, &mut capacity) };
        if buffer.is_null() || data.len() > capacity {
            // Bigger buffers need a new decoder (stopping it takes back the
            // buffer we hold).
            self.needs_input.store(data.len() as u64, Ordering::Relaxed);
            self.dead.store(true, Ordering::Release);
            bail!("a {} KB frame doesn't fit the decoder's {} KB input", data.len() / 1024, capacity / 1024);
        }
        // SAFETY: `buffer` holds `capacity` >= data.len() bytes.
        unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), buffer, data.len()) };
        {
            let mut queued = self.queued.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            if queued.len() >= 64 {
                queued.pop_front();
            }
            queued.push_back((sequence, Instant::now()));
        }
        // SAFETY: queueing the filled buffer.
        let status = unsafe { AMediaCodec_queueInputBuffer(codec.0, index, 0, data.len(), sequence, 0) };
        if status != OK {
            self.dead.store(true, Ordering::Release);
            bail!("decoder refused the frame ({status})");
        }
        Ok(())
    }

    /// Show frames as they come out, until the decoder stops.
    fn show_loop(self: Arc<Self>) {
        let mut info = BufferInfo::default();
        loop {
            let codec = self.codec.read().unwrap_or_else(|poisoned| poisoned.into_inner());
            let Some(codec) = codec.as_ref() else { return };
            // SAFETY: a live codec; `info` is ours.
            let index = unsafe { AMediaCodec_dequeueOutputBuffer(codec.0, &mut info, 10_000) };
            match index {
                index if index >= 0 => {
                    let mut newest = (index as usize, info);
                    let mut skipped = 0;
                    loop {
                        let mut next = BufferInfo::default();
                        // SAFETY: as above, without waiting.
                        let more = unsafe { AMediaCodec_dequeueOutputBuffer(codec.0, &mut next, 0) };
                        if more < 0 {
                            if more == OUTPUT_FORMAT_CHANGED {
                                log_format(codec.0);
                            }
                            break;
                        }
                        // SAFETY: returning an output buffer unshown.
                        unsafe { AMediaCodec_releaseOutputBuffer(codec.0, newest.0, false) };
                        skipped += 1;
                        newest = (more as usize, next);
                    }
                    // Now: shown at the next refresh, ahead of anything older.
                    // SAFETY: rendering the buffer we hold.
                    unsafe { AMediaCodec_releaseOutputBufferAtTime(codec.0, newest.0, monotonic_ns()) };
                    let shown = Instant::now();
                    let queued_at = {
                        let mut queued = self.queued.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                        let sequence = newest.1.presentation_time_us as u64;
                        while queued.front().is_some_and(|(earlier, _)| *earlier < sequence) {
                            queued.pop_front();
                        }
                        queued.front().filter(|(this, _)| *this == sequence).map(|(_, when)| *when)
                    };
                    note(|shared| {
                        shared.stats.shown += 1;
                        shared.stats.skipped += skipped;
                        if let Some(queued_at) = queued_at {
                            shared.delays.push_back((shown, shown.duration_since(queued_at).as_secs_f64() * 1000.0));
                            if shared.delays.len() > 240 {
                                shared.delays.pop_front();
                            }
                        }
                    });
                }
                OUTPUT_FORMAT_CHANGED => log_format(codec.0),
                TRY_AGAIN_LATER | OUTPUT_BUFFERS_CHANGED => {}
                error => {
                    tracing::warn!(error, "decoder output failed; starting over at a keyframe");
                    self.dead.store(true, Ordering::Release);
                    return;
                }
            }
        }
    }
}

impl Drop for Active {
    fn drop(&mut self) {
        if let Some(codec) = self.codec.get_mut().unwrap_or_else(|poisoned| poisoned.into_inner()).take() {
            // SAFETY: the last reference; nothing else can be using it.
            unsafe {
                AMediaCodec_stop(codec.0);
                AMediaCodec_delete(codec.0);
            }
        }
    }
}

/// The parameter sets at the start of a keyframe, as MediaCodec's "csd"
/// buffers: H.264's SPS and PPS apart, HEVC's VPS, SPS and PPS together.
fn codec_config(data: &[u8], hevc: bool) -> Vec<(&'static str, Vec<u8>)> {
    let with_start_code = |nal: &[u8]| [&[0u8, 0, 0, 1][..], nal].concat();
    let units = crate::h264::split_annexb(data);
    if hevc {
        let sets: Vec<u8> = units
            .iter()
            .filter(|nal| nal.first().is_some_and(|byte| matches!((byte >> 1) & 0x3f, 32..=34)))
            .flat_map(|nal| with_start_code(nal))
            .collect();
        return if sets.is_empty() { Vec::new() } else { vec![("csd-0", sets)] };
    }
    let find = |kind| units.iter().find(|nal| crate::h264::nal_type(nal) == Some(kind)).map(|nal| with_start_code(nal));
    match (find(crate::h264::NAL_SPS), find(crate::h264::NAL_PPS)) {
        (Some(sps), Some(pps)) => vec![("csd-0", sps), ("csd-1", pps)],
        _ => Vec::new(),
    }
}

fn codec_name(codec: *mut AMediaCodec) -> String {
    let mut name: *mut c_char = std::ptr::null_mut();
    // SAFETY: the name is copied out and released.
    unsafe {
        if AMediaCodec_getName(codec, &mut name) != OK || name.is_null() {
            return String::new();
        }
        let text = CStr::from_ptr(name).to_string_lossy().into_owned();
        AMediaCodec_releaseName(codec, name);
        text
    }
}

fn log_format(codec: *mut AMediaCodec) {
    // SAFETY: the format is read and deleted here.
    unsafe {
        let format = AMediaCodec_getOutputFormat(codec);
        if format.is_null() {
            return;
        }
        let (mut width, mut height, mut color) = (0, 0, 0);
        AMediaFormat_getInt32(format, key("width").as_ptr(), &mut width);
        AMediaFormat_getInt32(format, key("height").as_ptr(), &mut height);
        AMediaFormat_getInt32(format, key("color-format").as_ptr(), &mut color);
        tracing::info!(width, height, color, "decoder output format");
        AMediaFormat_delete(format);
    }
}

/// H.264 or HEVC through MediaCodec, onto the surface from [`set_surface`].
pub struct MediaCodecDecoder {
    mime: &'static str,
    width: u32,
    height: u32,
    active: Option<Arc<Active>>,
    /// Input buffer size to ask for: a large keyframe can raise it.
    max_input: usize,
    sequence: u64,
}

impl MediaCodecDecoder {
    pub fn new(codec: &str, width: u32, height: u32) -> anyhow::Result<Self> {
        let mime = match codec {
            "h264" => "video/avc",
            "hevc" => "video/hevc",
            other => bail!("no Android decoder for {other:?}"),
        };
        Ok(Self {
            mime,
            width,
            height,
            active: None,
            // A keyframe of a busy screen at a high bitrate runs to a few MB.
            max_input: (width as usize * height as usize).clamp(2 << 20, 16 << 20),
            sequence: 0,
        })
    }

    /// The picture is on the screen already: this only says it was decoded.
    fn shown(&self, frame_id: u64, capture_ts_us: u64) -> DecodedFrame {
        DecodedFrame {
            frame_id,
            width: self.width,
            height: self.height,
            format: PixelFormat::Nv12,
            data: FrameData::Cpu(Bytes::new()),
            capture_ts_us,
            color: Color::default(),
        }
    }
}

impl Decoder for MediaCodecDecoder {
    fn decode(&mut self, frame_id: u64, capture_ts_us: u64, keyframe: bool, data: &[u8]) -> anyhow::Result<DecodedFrame> {
        if let Some(active) = self.active.take_if(|active| active.is_dead()) {
            let needed = active.needs_input.load(Ordering::Relaxed) as usize;
            if needed > self.max_input {
                self.max_input = (needed * 3 / 2).min(64 << 20);
            }
            active.stop();
        }
        if self.active.is_none() {
            let mut slot = SLOT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            if slot.window.is_none() {
                // Nowhere to show it (the app is in the background).
                return Ok(self.shown(frame_id, capture_ts_us));
            }
            ensure!(keyframe, "waiting for a keyframe to start the decoder");
            if let Some(previous) = slot.active.take() {
                previous.stop();
            }
            let config = codec_config(data, self.mime == "video/hevc");
            let window = slot.window.as_ref().expect("checked above");
            let active = Active::start(self.mime, self.width, self.height, self.max_input, &config, window)?;
            slot.active = Some(Arc::clone(&active));
            self.active = Some(active);
        }
        let active = self.active.as_ref().expect("started above");
        self.sequence += 1;
        active.queue(data, self.sequence)?;
        Ok(self.shown(frame_id, capture_ts_us))
    }
}

impl Drop for MediaCodecDecoder {
    fn drop(&mut self) {
        if let Some(active) = self.active.take() {
            let mut slot = SLOT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            if slot.active.as_ref().is_some_and(|current| Arc::ptr_eq(current, &active)) {
                slot.active = None;
            }
            drop(slot);
            active.stop();
        }
    }
}
