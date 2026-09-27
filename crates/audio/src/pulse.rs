//! PulseAudio, loaded at run time (libpulse comes with PulseAudio and with
//! PipeWire's pulse server): record the monitor of the output (what's
//! playing) on hosts, play on viewers (through the simple API).

use std::collections::VecDeque;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context};
use libloading::Library;

use crate::player::Ring;
use crate::{Capture, Output, CHANNELS, FRAME_LEN, SAMPLE_RATE};

const PA_STREAM_PLAYBACK: c_int = 1;
const PA_SAMPLE_S16LE: c_int = 3;
const PA_ERR_NOENTITY: c_int = 5;
const PA_ERR_CONNECTIONREFUSED: c_int = 6;
const PA_ERR_KILLED: c_int = 12;
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
type Write = unsafe extern "C" fn(s: *mut c_void, data: *const c_void, bytes: usize, error: *mut c_int) -> c_int;
type Free = unsafe extern "C" fn(s: *mut c_void);

struct Api {
    new: New,
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
            let write: Write = *library.get(b"pa_simple_write\0").map_err(|error| error.to_string())?;
            let free: Free = *library.get(b"pa_simple_free\0").map_err(|error| error.to_string())?;
            Ok(Api { new, write, free, _library: library })
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
        let device_name = device.map(CString::new).transpose()?;
        let mut error = 0;
        // SAFETY: all pointers are valid for the call; a null server and
        // channel map mean the defaults.
        let raw = unsafe {
            (api.new)(
                std::ptr::null(),
                name.as_ptr(),
                direction,
                device_name.as_ref().map_or(std::ptr::null(), |device| device.as_ptr()),
                what.as_ptr(),
                &spec,
                std::ptr::null(),
                &attr,
                &mut error,
            )
        };
        if raw.is_null() {
            match error {
                PA_ERR_NOENTITY => bail!("no such sound device"),
                PA_ERR_CONNECTIONREFUSED => bail!("no sound server is running (PulseAudio or PipeWire)"),
                _ => bail!("couldn't open the sound server (error {error})"),
            }
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

// ---- Capture, through the full API ----
//
// When a recording's source goes away (headphones unplugged, a virtual
// desktop stopped), PulseAudio and PipeWire move the recording to the
// default source, which is usually the microphone. PA_STREAM_DONT_MOVE ends
// the recording instead; the simple API can't ask for that.

const PA_CONTEXT_READY: c_int = 4;
const PA_CONTEXT_FAILED: c_int = 5;
const PA_CONTEXT_TERMINATED: c_int = 6;
const PA_STREAM_READY: c_int = 2;
const PA_STREAM_FAILED: c_int = 3;
const PA_STREAM_TERMINATED: c_int = 4;
const PA_STREAM_DONT_MOVE: c_int = 0x0200;
const PA_STREAM_ADJUST_LATENCY: c_int = 0x2000;
/// At most a second of sound waits for `read`.
const MAX_QUEUED_BYTES: usize = SAMPLE_RATE as usize * CHANNELS * 2;

type Notify = unsafe extern "C" fn(object: *mut c_void, userdata: *mut c_void);
type Readable = unsafe extern "C" fn(stream: *mut c_void, bytes: usize, userdata: *mut c_void);

struct FullApi {
    mainloop_new: unsafe extern "C" fn() -> *mut c_void,
    mainloop_free: unsafe extern "C" fn(*mut c_void),
    mainloop_start: unsafe extern "C" fn(*mut c_void) -> c_int,
    mainloop_stop: unsafe extern "C" fn(*mut c_void),
    mainloop_lock: unsafe extern "C" fn(*mut c_void),
    mainloop_unlock: unsafe extern "C" fn(*mut c_void),
    mainloop_wait: unsafe extern "C" fn(*mut c_void),
    mainloop_signal: unsafe extern "C" fn(*mut c_void, c_int),
    mainloop_get_api: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    context_new: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
    context_unref: unsafe extern "C" fn(*mut c_void),
    context_connect: unsafe extern "C" fn(*mut c_void, *const c_char, c_int, *const c_void) -> c_int,
    context_disconnect: unsafe extern "C" fn(*mut c_void),
    context_get_state: unsafe extern "C" fn(*mut c_void) -> c_int,
    context_set_state_callback: unsafe extern "C" fn(*mut c_void, Option<Notify>, *mut c_void),
    context_errno: unsafe extern "C" fn(*mut c_void) -> c_int,
    stream_new: unsafe extern "C" fn(*mut c_void, *const c_char, *const SampleSpec, *const c_void) -> *mut c_void,
    stream_unref: unsafe extern "C" fn(*mut c_void),
    stream_connect_record: unsafe extern "C" fn(*mut c_void, *const c_char, *const BufferAttr, c_int) -> c_int,
    stream_disconnect: unsafe extern "C" fn(*mut c_void) -> c_int,
    stream_get_state: unsafe extern "C" fn(*mut c_void) -> c_int,
    stream_set_state_callback: unsafe extern "C" fn(*mut c_void, Option<Notify>, *mut c_void),
    stream_set_read_callback: unsafe extern "C" fn(*mut c_void, Option<Readable>, *mut c_void),
    stream_peek: unsafe extern "C" fn(*mut c_void, *mut *const c_void, *mut usize) -> c_int,
    stream_drop: unsafe extern "C" fn(*mut c_void) -> c_int,
    strerror: unsafe extern "C" fn(c_int) -> *const c_char,
    _library: Library,
}

fn full_api() -> anyhow::Result<&'static FullApi> {
    static API: OnceLock<Result<FullApi, String>> = OnceLock::new();
    API.get_or_init(|| {
        // SAFETY: loading a system library and looking up its documented
        // symbols, each with its C signature.
        unsafe {
            let library = Library::new("libpulse.so.0").map_err(|error| error.to_string())?;
            macro_rules! symbol {
                ($name:literal) => {
                    *library.get(concat!($name, "\0").as_bytes()).map_err(|error| error.to_string())?
                };
            }
            Ok(FullApi {
                mainloop_new: symbol!("pa_threaded_mainloop_new"),
                mainloop_free: symbol!("pa_threaded_mainloop_free"),
                mainloop_start: symbol!("pa_threaded_mainloop_start"),
                mainloop_stop: symbol!("pa_threaded_mainloop_stop"),
                mainloop_lock: symbol!("pa_threaded_mainloop_lock"),
                mainloop_unlock: symbol!("pa_threaded_mainloop_unlock"),
                mainloop_wait: symbol!("pa_threaded_mainloop_wait"),
                mainloop_signal: symbol!("pa_threaded_mainloop_signal"),
                mainloop_get_api: symbol!("pa_threaded_mainloop_get_api"),
                context_new: symbol!("pa_context_new"),
                context_unref: symbol!("pa_context_unref"),
                context_connect: symbol!("pa_context_connect"),
                context_disconnect: symbol!("pa_context_disconnect"),
                context_get_state: symbol!("pa_context_get_state"),
                context_set_state_callback: symbol!("pa_context_set_state_callback"),
                context_errno: symbol!("pa_context_errno"),
                stream_new: symbol!("pa_stream_new"),
                stream_unref: symbol!("pa_stream_unref"),
                stream_connect_record: symbol!("pa_stream_connect_record"),
                stream_disconnect: symbol!("pa_stream_disconnect"),
                stream_get_state: symbol!("pa_stream_get_state"),
                stream_set_state_callback: symbol!("pa_stream_set_state_callback"),
                stream_set_read_callback: symbol!("pa_stream_set_read_callback"),
                stream_peek: symbol!("pa_stream_peek"),
                stream_drop: symbol!("pa_stream_drop"),
                strerror: symbol!("pa_strerror"),
                _library: library,
            })
        }
    })
    .as_ref()
    .map_err(|error| anyhow!("PulseAudio isn't available ({error})"))
}

/// Why the sound server said no, in words.
fn describe(api: &FullApi, error: c_int) -> String {
    match error {
        PA_ERR_NOENTITY => "no such sound device".into(),
        PA_ERR_CONNECTIONREFUSED => "no sound server is running (PulseAudio or PipeWire)".into(),
        PA_ERR_KILLED => "its sound device went away".into(),
        _ => {
            // SAFETY: pa_strerror returns a static string (or null).
            let text = unsafe { (api.strerror)(error) };
            if text.is_null() {
                format!("sound server error {error}")
            } else {
                // SAFETY: a NUL-terminated static string.
                unsafe { CStr::from_ptr(text) }.to_string_lossy().into_owned()
            }
        }
    }
}

/// What PulseAudio's thread hands to `read`.
#[derive(Default)]
struct Captured {
    bytes: VecDeque<u8>,
    /// Set when the recording ended (its source went away, the server quit).
    ended: Option<String>,
}

/// Shared with the callbacks, which run on PulseAudio's thread.
struct Shared {
    mainloop: *mut c_void,
    context: *mut c_void,
    captured: Mutex<Captured>,
    ready: Condvar,
}

// SAFETY: the pointers are only used through PulseAudio's thread-safe
// mainloop calls or under its lock (callbacks hold it); the rest is Mutex'd.
unsafe impl Send for Shared {}
unsafe impl Sync for Shared {}

impl Shared {
    /// Never panics: the callbacks run inside C.
    fn captured(&self) -> MutexGuard<'_, Captured> {
        self.captured.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

unsafe extern "C" fn context_changed(_context: *mut c_void, userdata: *mut c_void) {
    let shared = &*(userdata as *const Shared);
    if let Ok(api) = full_api() {
        (api.mainloop_signal)(shared.mainloop, 0);
    }
}

unsafe extern "C" fn stream_changed(stream: *mut c_void, userdata: *mut c_void) {
    let shared = &*(userdata as *const Shared);
    let Ok(api) = full_api() else { return };
    let state = (api.stream_get_state)(stream);
    if state == PA_STREAM_FAILED || state == PA_STREAM_TERMINATED {
        let reason = describe(api, (api.context_errno)(shared.context));
        shared.captured().ended.get_or_insert(reason);
        shared.ready.notify_all();
    }
    (api.mainloop_signal)(shared.mainloop, 0);
}

unsafe extern "C" fn stream_readable(stream: *mut c_void, _bytes: usize, userdata: *mut c_void) {
    let shared = &*(userdata as *const Shared);
    let Ok(api) = full_api() else { return };
    let mut captured = shared.captured();
    loop {
        let (mut data, mut bytes) = (std::ptr::null(), 0usize);
        if (api.stream_peek)(stream, &mut data, &mut bytes) < 0 || bytes == 0 {
            break;
        }
        if data.is_null() {
            // A hole in the recording: silence.
            captured.bytes.extend(std::iter::repeat_n(0, bytes));
        } else {
            captured.bytes.extend(std::slice::from_raw_parts(data as *const u8, bytes));
        }
        (api.stream_drop)(stream);
    }
    if captured.bytes.len() > MAX_QUEUED_BYTES {
        // Whole sample frames (4 bytes), so the channels stay in step.
        let excess = (captured.bytes.len() - MAX_QUEUED_BYTES).next_multiple_of(4);
        captured.bytes.drain(..excess);
    }
    shared.ready.notify_one();
}

/// Records `device` (a source name, e.g. `@DEFAULT_MONITOR@`) until it goes
/// away; it never follows the server to another source.
pub(crate) struct PulseCapture {
    api: &'static FullApi,
    stream: *mut c_void,
    shared: Arc<Shared>,
}

// SAFETY: see Shared; `stream` is only touched under the mainloop lock.
unsafe impl Send for PulseCapture {}

impl PulseCapture {
    pub(crate) fn open(device: &str) -> anyhow::Result<Self> {
        let api = full_api()?;
        let device_name = CString::new(device)?;
        // SAFETY: the standard threaded-mainloop sequence; every object is
        // freed once, by Drop, and the callbacks' userdata (Shared) outlives
        // PulseAudio's thread.
        unsafe {
            let mainloop = (api.mainloop_new)();
            anyhow::ensure!(!mainloop.is_null(), "couldn't start PulseAudio's thread");
            let context = (api.context_new)((api.mainloop_get_api)(mainloop), c"Sunna".as_ptr());
            if context.is_null() {
                (api.mainloop_free)(mainloop);
                bail!("couldn't start PulseAudio's thread");
            }
            let shared = Arc::new(Shared { mainloop, context, captured: Mutex::default(), ready: Condvar::new() });
            let mut capture = Self { api, stream: std::ptr::null_mut(), shared };
            let userdata = Arc::as_ptr(&capture.shared) as *mut c_void;

            if (api.mainloop_start)(mainloop) < 0 {
                bail!("couldn't start PulseAudio's thread");
            }
            (api.mainloop_lock)(mainloop);
            let opened = capture.connect(&device_name, userdata);
            (api.mainloop_unlock)(mainloop);
            opened.with_context(|| format!("recording {device}"))?;
            tracing::info!(device, "audio capture started (PulseAudio)");
            Ok(capture)
        }
    }

    /// Connects and starts recording; called with the mainloop locked.
    unsafe fn connect(&mut self, device: &CStr, userdata: *mut c_void) -> anyhow::Result<()> {
        let api = self.api;
        let Shared { mainloop, context, .. } = *self.shared;
        (api.context_set_state_callback)(context, Some(context_changed), userdata);
        if (api.context_connect)(context, std::ptr::null(), 0, std::ptr::null()) < 0 {
            bail!(describe(api, (api.context_errno)(context)));
        }
        loop {
            match (api.context_get_state)(context) {
                PA_CONTEXT_READY => break,
                PA_CONTEXT_FAILED | PA_CONTEXT_TERMINATED => bail!(describe(api, (api.context_errno)(context))),
                _ => (api.mainloop_wait)(mainloop),
            }
        }

        let spec = SampleSpec { format: PA_SAMPLE_S16LE, rate: SAMPLE_RATE, channels: CHANNELS as u8 };
        self.stream = (api.stream_new)(context, c"What's playing".as_ptr(), &spec, std::ptr::null());
        anyhow::ensure!(!self.stream.is_null(), describe(api, (api.context_errno)(context)));
        (api.stream_set_state_callback)(self.stream, Some(stream_changed), userdata);
        (api.stream_set_read_callback)(self.stream, Some(stream_readable), userdata);
        // Deliver every 10 ms, so a packet leaves as soon as it's captured.
        let attr = BufferAttr { maxlength: u32::MAX, tlength: u32::MAX, prebuf: u32::MAX, minreq: u32::MAX, fragsize: FRAME_BYTES };
        let flags = PA_STREAM_DONT_MOVE | PA_STREAM_ADJUST_LATENCY;
        if (api.stream_connect_record)(self.stream, device.as_ptr(), &attr, flags) < 0 {
            bail!(describe(api, (api.context_errno)(context)));
        }
        loop {
            match (api.stream_get_state)(self.stream) {
                PA_STREAM_READY => return Ok(()),
                PA_STREAM_FAILED | PA_STREAM_TERMINATED => bail!(describe(api, (api.context_errno)(context))),
                _ => (api.mainloop_wait)(mainloop),
            }
        }
    }
}

impl Capture for PulseCapture {
    fn read(&mut self, frame: &mut [i16; FRAME_LEN]) -> anyhow::Result<()> {
        let wanted = FRAME_LEN * 2;
        let deadline = Instant::now() + Duration::from_millis(100);
        let mut captured = self.shared.captured();
        while captured.bytes.len() < wanted {
            if let Some(reason) = &captured.ended {
                bail!("recording ended: {reason}");
            }
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                // Nothing for a while (a silent, suspended output): silence,
                // so the caller's loop keeps turning.
                frame.fill(0);
                return Ok(());
            };
            captured = self.shared.ready.wait_timeout(captured, left).unwrap_or_else(PoisonError::into_inner).0;
        }
        let mut bytes = captured.bytes.drain(..wanted);
        for sample in frame.iter_mut() {
            let (Some(low), Some(high)) = (bytes.next(), bytes.next()) else { break };
            *sample = i16::from_le_bytes([low, high]);
        }
        Ok(())
    }
}

impl Drop for PulseCapture {
    fn drop(&mut self) {
        let api = self.api;
        let Shared { mainloop, context, .. } = *self.shared;
        // SAFETY: tears down what `open` made, callbacks first, so none runs
        // after this; the thread is stopped (unlocked) before it's freed.
        unsafe {
            (api.mainloop_lock)(mainloop);
            if !self.stream.is_null() {
                (api.stream_set_state_callback)(self.stream, None, std::ptr::null_mut());
                (api.stream_set_read_callback)(self.stream, None, std::ptr::null_mut());
                (api.stream_disconnect)(self.stream);
                (api.stream_unref)(self.stream);
            }
            (api.context_set_state_callback)(context, None, std::ptr::null_mut());
            (api.context_disconnect)(context);
            (api.context_unref)(context);
            (api.mainloop_unlock)(mainloop);
            (api.mainloop_stop)(mainloop);
            (api.mainloop_free)(mainloop);
        }
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
