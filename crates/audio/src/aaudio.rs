//! Playback on Android through AAudio, the system's low-latency path: its
//! callback thread pulls 48 kHz stereo from the jitter buffer. When the
//! output changes under it (headphones in or out, a Bluetooth speaker), the
//! stream is opened again on whatever plays now.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::bail;

use crate::player::Ring;
use crate::{Output, CHANNELS, SAMPLE_RATE};

#[repr(C)]
struct AAudioStreamBuilder {
    _private: [u8; 0],
}

#[repr(C)]
struct AAudioStream {
    _private: [u8; 0],
}

type DataCallback = unsafe extern "C" fn(*mut AAudioStream, *mut c_void, *mut c_void, i32) -> i32;
type ErrorCallback = unsafe extern "C" fn(*mut AAudioStream, *mut c_void, i32);

const OK: i32 = 0;
const DIRECTION_OUTPUT: i32 = 0;
const FORMAT_PCM_I16: i32 = 1;
const SHARING_MODE_SHARED: i32 = 1;
const PERFORMANCE_MODE_LOW_LATENCY: i32 = 12;
const USAGE_MEDIA: i32 = 1;
const CONTENT_TYPE_MUSIC: i32 = 2;
const CALLBACK_CONTINUE: i32 = 0;

#[link(name = "aaudio")]
extern "C" {
    fn AAudio_createStreamBuilder(builder: *mut *mut AAudioStreamBuilder) -> i32;
    fn AAudioStreamBuilder_setDirection(builder: *mut AAudioStreamBuilder, direction: i32);
    fn AAudioStreamBuilder_setSampleRate(builder: *mut AAudioStreamBuilder, rate: i32);
    fn AAudioStreamBuilder_setChannelCount(builder: *mut AAudioStreamBuilder, channels: i32);
    fn AAudioStreamBuilder_setFormat(builder: *mut AAudioStreamBuilder, format: i32);
    fn AAudioStreamBuilder_setSharingMode(builder: *mut AAudioStreamBuilder, mode: i32);
    fn AAudioStreamBuilder_setPerformanceMode(builder: *mut AAudioStreamBuilder, mode: i32);
    fn AAudioStreamBuilder_setUsage(builder: *mut AAudioStreamBuilder, usage: i32);
    fn AAudioStreamBuilder_setContentType(builder: *mut AAudioStreamBuilder, content: i32);
    fn AAudioStreamBuilder_setDataCallback(builder: *mut AAudioStreamBuilder, callback: DataCallback, user: *mut c_void);
    fn AAudioStreamBuilder_setErrorCallback(builder: *mut AAudioStreamBuilder, callback: ErrorCallback, user: *mut c_void);
    fn AAudioStreamBuilder_openStream(builder: *mut AAudioStreamBuilder, stream: *mut *mut AAudioStream) -> i32;
    fn AAudioStreamBuilder_delete(builder: *mut AAudioStreamBuilder) -> i32;
    fn AAudioStream_requestStart(stream: *mut AAudioStream) -> i32;
    fn AAudioStream_requestStop(stream: *mut AAudioStream) -> i32;
    fn AAudioStream_close(stream: *mut AAudioStream) -> i32;
    fn AAudioStream_getFramesPerBurst(stream: *mut AAudioStream) -> i32;
    fn AAudioStream_setBufferSizeInFrames(stream: *mut AAudioStream, frames: i32) -> i32;
}

struct Shared {
    ring: Arc<Ring>,
    /// The device went away: open the stream again.
    lost: AtomicBool,
    stop: AtomicBool,
}

/// An open stream; closing it (on drop) stops its callbacks, and it keeps
/// what they read alive until then.
struct Stream {
    raw: *mut AAudioStream,
    _shared: Arc<Shared>,
}

// SAFETY: AAudio streams may be stopped and closed from any thread.
unsafe impl Send for Stream {}

impl Drop for Stream {
    fn drop(&mut self) {
        // SAFETY: an open stream, closed once; no callback runs after close.
        unsafe {
            AAudioStream_requestStop(self.raw);
            AAudioStream_close(self.raw);
        }
    }
}

unsafe extern "C" fn pull(_: *mut AAudioStream, user: *mut c_void, audio: *mut c_void, frames: i32) -> i32 {
    // SAFETY: `user` is the `Shared` the output keeps alive while the stream
    // is open; `audio` holds `frames` interleaved stereo i16 frames.
    let shared = &*(user as *const Shared);
    let out = std::slice::from_raw_parts_mut(audio as *mut i16, frames.max(0) as usize * CHANNELS);
    shared.ring.pull(out);
    CALLBACK_CONTINUE
}

unsafe extern "C" fn failed(_: *mut AAudioStream, user: *mut c_void, error: i32) {
    // SAFETY: as in `pull`. Reopening is left to the output's thread: AAudio
    // doesn't allow it from this callback.
    let shared = &*(user as *const Shared);
    tracing::info!(error, "sound output changed");
    shared.lost.store(true, Ordering::Release);
}

fn open(shared: &Arc<Shared>) -> anyhow::Result<Stream> {
    let user = Arc::as_ptr(shared) as *mut c_void;
    // SAFETY: the builder is used and deleted here; the stream is checked.
    unsafe {
        let mut builder = std::ptr::null_mut();
        if AAudio_createStreamBuilder(&mut builder) != OK {
            bail!("AAudio isn't available");
        }
        AAudioStreamBuilder_setDirection(builder, DIRECTION_OUTPUT);
        AAudioStreamBuilder_setSampleRate(builder, SAMPLE_RATE as i32);
        AAudioStreamBuilder_setChannelCount(builder, CHANNELS as i32);
        AAudioStreamBuilder_setFormat(builder, FORMAT_PCM_I16);
        AAudioStreamBuilder_setSharingMode(builder, SHARING_MODE_SHARED);
        AAudioStreamBuilder_setPerformanceMode(builder, PERFORMANCE_MODE_LOW_LATENCY);
        AAudioStreamBuilder_setUsage(builder, USAGE_MEDIA);
        AAudioStreamBuilder_setContentType(builder, CONTENT_TYPE_MUSIC);
        AAudioStreamBuilder_setDataCallback(builder, pull, user);
        AAudioStreamBuilder_setErrorCallback(builder, failed, user);
        let mut stream = std::ptr::null_mut();
        let result = AAudioStreamBuilder_openStream(builder, &mut stream);
        AAudioStreamBuilder_delete(builder);
        if result != OK || stream.is_null() {
            bail!("couldn't open the sound output (AAudio {result})");
        }
        // Two bursts: as little as the device plays without glitches. The
        // jitter buffer in front already absorbs the network.
        let burst = AAudioStream_getFramesPerBurst(stream);
        if burst > 0 {
            AAudioStream_setBufferSizeInFrames(stream, burst * 2);
        }
        let result = AAudioStream_requestStart(stream);
        if result != OK {
            AAudioStream_close(stream);
            bail!("couldn't start the sound output (AAudio {result})");
        }
        tracing::info!(burst, "playing through AAudio");
        Ok(Stream { raw: stream, _shared: Arc::clone(shared) })
    }
}

pub(crate) struct AAudioOutput {
    shared: Arc<Shared>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl AAudioOutput {
    pub(crate) fn start(ring: Arc<Ring>) -> anyhow::Result<Self> {
        let shared = Arc::new(Shared { ring, lost: AtomicBool::new(false), stop: AtomicBool::new(false) });
        let first = open(&shared)?;
        let watched = Arc::clone(&shared);
        let thread = std::thread::Builder::new().name("sunna-sound".into()).spawn(move || {
            let mut stream = Some(first);
            while !watched.stop.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(200));
                if watched.lost.swap(false, Ordering::AcqRel) || stream.is_none() {
                    drop(stream.take());
                    match open(&watched) {
                        Ok(reopened) => stream = Some(reopened),
                        Err(error) => tracing::debug!(%error, "sound output not back yet"),
                    }
                }
            }
            drop(stream);
        })?;
        Ok(Self { shared, thread: Some(thread) })
    }
}

impl Output for AAudioOutput {}

impl Drop for AAudioOutput {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
