//! PulseAudio's simple API, loaded at run time (libpulse-simple.so.0 comes
//! with PulseAudio and with PipeWire's pulse server): record the monitor of
//! the output (what's playing) on hosts, play on viewers.

use std::ffi::{c_char, c_int, c_void, CString};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use anyhow::{anyhow, bail, Context};
use libloading::Library;

use crate::player::Ring;
use crate::{Capture, Output, CHANNELS, FRAME_LEN, SAMPLE_RATE};

const PA_STREAM_PLAYBACK: c_int = 1;
const PA_STREAM_RECORD: c_int = 2;
const PA_SAMPLE_S16LE: c_int = 3;
const FRAME_BYTES: u32 = (FRAME_LEN * 2) as u32;

#[repr(C)]
struct SampleSpec {
    format: c_int,
    rate: u32,
    channels: u8,
}

#[repr(C)]
struct BufferAttr {
    maxlength: u32,
    tlength: u32,
    prebuf: u32,
    minreq: u32,
    fragsize: u32,
}

type New = unsafe extern "C" fn(
    server: *const c_char,
    name: *const c_char,
    dir: c_int,
    dev: *const c_char,
    stream_name: *const c_char,
    spec: *const SampleSpec,
    map: *const c_void,
    attr: *const BufferAttr,
    error: *mut c_int,
) -> *mut c_void;
type Read = unsafe extern "C" fn(s: *mut c_void, data: *mut c_void, bytes: usize, error: *mut c_int) -> c_int;
type Write = unsafe extern "C" fn(s: *mut c_void, data: *const c_void, bytes: usize, error: *mut c_int) -> c_int;
type Free = unsafe extern "C" fn(s: *mut c_void);

struct Api {
    new: New,
    read: Read,
    write: Write,
    free: Free,
    _library: Library,
}

fn api() -> anyhow::Result<&'static Api> {
    static API: OnceLock<Result<Api, String>> = OnceLock::new();
    API.get_or_init(|| {
        // SAFETY: loading a system library and looking up its documented symbols.
        unsafe {
            let library = Library::new("libpulse-simple.so.0").map_err(|error| error.to_string())?;
            let new: New = *library.get(b"pa_simple_new\0").map_err(|error| error.to_string())?;
            let read: Read = *library.get(b"pa_simple_read\0").map_err(|error| error.to_string())?;
            let write: Write = *library.get(b"pa_simple_write\0").map_err(|error| error.to_string())?;
            let free: Free = *library.get(b"pa_simple_free\0").map_err(|error| error.to_string())?;
            Ok(Api { new, read, write, free, _library: library })
        }
    })
    .as_ref()
    .map_err(|error| anyhow!("PulseAudio isn't available ({error})"))
}

/// A connection to the sound server, recording or playing.
struct Stream {
    raw: *mut c_void,
}

// SAFETY: a pa_simple handle is used from one thread at a time.
unsafe impl Send for Stream {}

impl Stream {
    fn open(direction: c_int, device: Option<&str>, what: &str, attr: BufferAttr) -> anyhow::Result<Self> {
        let api = api()?;
        let spec = SampleSpec { format: PA_SAMPLE_S16LE, rate: SAMPLE_RATE, channels: CHANNELS as u8 };
        let name = CString::new("Sunna")?;
        let what = CString::new(what)?;
        let device = device.map(CString::new).transpose()?;
        let mut error = 0;
        // SAFETY: all pointers are valid for the call; a null server and
        // channel map mean the defaults.
        let raw = unsafe {
            (api.new)(
                std::ptr::null(),
                name.as_ptr(),
                direction,
                device.as_ref().map_or(std::ptr::null(), |device| device.as_ptr()),
                what.as_ptr(),
                &spec,
                std::ptr::null(),
                &attr,
                &mut error,
            )
        };
        if raw.is_null() {
            bail!("couldn't open the sound server (error {error})");
        }
        Ok(Self { raw })
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        if let Ok(api) = api() {
            // SAFETY: `raw` came from pa_simple_new and is freed once.
            unsafe { (api.free)(self.raw) }
        }
    }
}

/// Records `device` (a source name, e.g. `@DEFAULT_MONITOR@`).
pub(crate) struct PulseCapture {
    stream: Stream,
}

impl PulseCapture {
    pub(crate) fn open(device: &str) -> anyhow::Result<Self> {
        // Deliver every 10 ms, so a packet leaves as soon as it's captured.
        let attr = BufferAttr { maxlength: u32::MAX, tlength: u32::MAX, prebuf: u32::MAX, minreq: u32::MAX, fragsize: FRAME_BYTES };
        let stream = Stream::open(PA_STREAM_RECORD, Some(device), "What's playing", attr)
            .with_context(|| format!("recording {device}"))?;
        tracing::info!(device, "audio capture started (PulseAudio)");
        Ok(Self { stream })
    }
}

impl Capture for PulseCapture {
    fn read(&mut self, frame: &mut [i16; FRAME_LEN]) -> anyhow::Result<()> {
        let api = api()?;
        let mut error = 0;
        // SAFETY: `frame` is FRAME_LEN i16s; pa_simple_read blocks until full.
        let result = unsafe { (api.read)(self.stream.raw, frame.as_mut_ptr().cast(), FRAME_LEN * 2, &mut error) };
        if result < 0 {
            bail!("reading sound failed (error {error})");
        }
        Ok(())
    }
}

/// Plays the ring on the default output from its own thread; stops on drop.
pub(crate) struct PulseOutput {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl PulseOutput {
    pub(crate) fn start(ring: Arc<Ring>) -> anyhow::Result<Self> {
        // ~30 ms in the server: low latency, but safe from scheduling hiccups.
        let attr = BufferAttr { maxlength: u32::MAX, tlength: 3 * FRAME_BYTES, prebuf: u32::MAX, minreq: FRAME_BYTES, fragsize: u32::MAX };
        let stream = Stream::open(PA_STREAM_PLAYBACK, None, "Remote computer", attr)?;
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let stop = Arc::clone(&stop);
            std::thread::Builder::new().name("sunna-audio-out".into()).spawn(move || {
                let stream = stream; // move the whole (Send) stream, not its raw pointer
                let Ok(api) = api() else { return };
                let mut frame = [0i16; FRAME_LEN];
                while !stop.load(Ordering::Relaxed) {
                    ring.pull(&mut frame);
                    let mut error = 0;
                    // SAFETY: `frame` is FRAME_LEN i16s; blocks while the server's buffer is full.
                    if unsafe { (api.write)(stream.raw, frame.as_ptr().cast(), FRAME_LEN * 2, &mut error) } < 0 {
                        tracing::warn!(error, "audio playback stopped");
                        break;
                    }
                }
            })?
        };
        tracing::info!("audio playback started (PulseAudio)");
        Ok(Self { stop, thread: Some(thread) })
    }
}

impl Output for PulseOutput {}

impl Drop for PulseOutput {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
