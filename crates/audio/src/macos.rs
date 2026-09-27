//! macOS: playback through AudioQueue (AudioToolbox), fed from the ring on
//! AudioQueue's own thread; capture of the system's sound through
//! ScreenCaptureKit.

use std::ffi::c_void;
use std::sync::Arc;

use anyhow::{bail, ensure};

use crate::player::Ring;
use crate::{Capture, Output, CHANNELS, FRAME_LEN, SAMPLE_RATE};

type OSStatus = i32;
type AudioQueueRef = *mut c_void;

#[repr(C)]
struct AudioStreamBasicDescription {
    sample_rate: f64,
    format_id: u32,
    format_flags: u32,
    bytes_per_packet: u32,
    frames_per_packet: u32,
    bytes_per_frame: u32,
    channels_per_frame: u32,
    bits_per_channel: u32,
    reserved: u32,
}

#[repr(C)]
struct AudioQueueBuffer {
    capacity: u32,
    data: *mut c_void,
    byte_size: u32,
    user_data: *mut c_void,
    packet_description_capacity: u32,
    packet_descriptions: *mut c_void,
    packet_description_count: u32,
}

type AudioQueueOutputCallback = extern "C" fn(user: *mut c_void, queue: AudioQueueRef, buffer: *mut AudioQueueBuffer);

#[link(name = "AudioToolbox", kind = "framework")]
extern "C" {
    fn AudioQueueNewOutput(
        format: *const AudioStreamBasicDescription,
        callback: AudioQueueOutputCallback,
        user: *mut c_void,
        run_loop: *const c_void,
        run_loop_mode: *const c_void,
        flags: u32,
        queue: *mut AudioQueueRef,
    ) -> OSStatus;
    fn AudioQueueAllocateBuffer(queue: AudioQueueRef, size: u32, buffer: *mut *mut AudioQueueBuffer) -> OSStatus;
    fn AudioQueueEnqueueBuffer(queue: AudioQueueRef, buffer: *mut AudioQueueBuffer, descriptions: u32, packets: *const c_void) -> OSStatus;
    fn AudioQueueStart(queue: AudioQueueRef, start_time: *const c_void) -> OSStatus;
    fn AudioQueueStop(queue: AudioQueueRef, immediate: u8) -> OSStatus;
    fn AudioQueueDispose(queue: AudioQueueRef, immediate: u8) -> OSStatus;
}

const K_AUDIO_FORMAT_LINEAR_PCM: u32 = u32::from_be_bytes(*b"lpcm");
const K_FLAG_SIGNED_INTEGER: u32 = 1 << 2;
const K_FLAG_PACKED: u32 = 1 << 3;
/// Buffers in flight: three 10 ms buffers keep the device fed with ~30 ms of latency.
const BUFFERS: usize = 3;

extern "C" fn fill(user: *mut c_void, queue: AudioQueueRef, buffer: *mut AudioQueueBuffer) {
    // SAFETY: `user` is the `Ring` kept alive by `QueueOutput` until the
    // queue is disposed; `buffer` was allocated for FRAME_LEN samples.
    unsafe {
        let ring = &*(user as *const Ring);
        let samples = std::slice::from_raw_parts_mut((*buffer).data as *mut i16, FRAME_LEN);
        ring.pull(samples);
        (*buffer).byte_size = (FRAME_LEN * 2) as u32;
        AudioQueueEnqueueBuffer(queue, buffer, 0, std::ptr::null());
    }
}

pub(crate) struct QueueOutput {
    queue: AudioQueueRef,
    ring: *const Ring,
}

// SAFETY: the queue is only started and disposed from the owning thread;
// AudioQueue calls `fill` on its own thread, reading the ring (a Mutex).
unsafe impl Send for QueueOutput {}

impl QueueOutput {
    pub(crate) fn start(ring: Arc<Ring>) -> anyhow::Result<Self> {
        let format = AudioStreamBasicDescription {
            sample_rate: SAMPLE_RATE as f64,
            format_id: K_AUDIO_FORMAT_LINEAR_PCM,
            format_flags: K_FLAG_SIGNED_INTEGER | K_FLAG_PACKED,
            bytes_per_packet: (CHANNELS * 2) as u32,
            frames_per_packet: 1,
            bytes_per_frame: (CHANNELS * 2) as u32,
            channels_per_frame: CHANNELS as u32,
            bits_per_channel: 16,
            reserved: 0,
        };
        let ring = Arc::into_raw(ring);
        let mut queue: AudioQueueRef = std::ptr::null_mut();
        // SAFETY: a null run loop runs callbacks on AudioQueue's own thread.
        let status = unsafe {
            AudioQueueNewOutput(&format, fill, ring as *mut c_void, std::ptr::null(), std::ptr::null(), 0, &mut queue)
        };
        if status != 0 {
            // SAFETY: reclaim the reference handed to the failed queue.
            drop(unsafe { Arc::from_raw(ring) });
            bail!("AudioQueueNewOutput failed: {status}");
        }
        let output = Self { queue, ring };
        for _ in 0..BUFFERS {
            let mut buffer = std::ptr::null_mut();
            // SAFETY: `queue` is live; each buffer holds one 10 ms frame.
            let status = unsafe { AudioQueueAllocateBuffer(queue, (FRAME_LEN * 2) as u32, &mut buffer) };
            ensure!(status == 0, "AudioQueueAllocateBuffer failed: {status}");
            fill(ring as *mut c_void, queue, buffer);
        }
        // SAFETY: start as soon as possible.
        let status = unsafe { AudioQueueStart(queue, std::ptr::null()) };
        ensure!(status == 0, "AudioQueueStart failed: {status}");
        tracing::info!("audio playback started (AudioQueue)");
        Ok(output)
    }
}

impl Output for QueueOutput {}

impl Drop for QueueOutput {
    fn drop(&mut self) {
        // SAFETY: stop and dispose synchronously, so no callback runs after
        // the ring's reference is released.
        unsafe {
            AudioQueueStop(self.queue, 1);
            AudioQueueDispose(self.queue, 1);
            drop(Arc::from_raw(self.ring));
        }
    }
}

// ---- Capture: the Mac's own sound, through ScreenCaptureKit (macOS 13+) ----

use std::collections::VecDeque;
use std::ffi::{c_char, CStr};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use objc2::rc::{autoreleasepool, Allocated, Retained};
use objc2::runtime::{AnyObject, Bool, NSObject};
use objc2::{class, define_class, msg_send, ClassType};

#[link(name = "ScreenCaptureKit", kind = "framework")]
extern "C" {}

#[repr(C)]
#[derive(Clone, Copy)]
struct AudioBuffer {
    channels: u32,
    byte_size: u32,
    data: *mut c_void,
}

/// An AudioBufferList with room for two buffers (non-interleaved stereo).
#[repr(C)]
struct AudioBufferList2 {
    count: u32,
    buffers: [AudioBuffer; 2],
}

#[link(name = "CoreMedia", kind = "framework")]
extern "C" {
    fn CMSampleBufferGetFormatDescription(buffer: *mut c_void) -> *const c_void;
    fn CMAudioFormatDescriptionGetStreamBasicDescription(description: *const c_void) -> *const AudioStreamBasicDescription;
    fn CMSampleBufferGetNumSamples(buffer: *mut c_void) -> isize;
    fn CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
        buffer: *mut c_void,
        size_needed: *mut usize,
        list: *mut AudioBufferList2,
        list_size: usize,
        allocator: *const c_void,
        block_allocator: *const c_void,
        flags: u32,
        block_buffer: *mut *const c_void,
    ) -> OSStatus;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(object: *const c_void);
}

extern "C" {
    fn dispatch_queue_create(label: *const c_char, attributes: *const c_void) -> *mut AnyObject;
    fn objc_getProtocol(name: *const c_char) -> *const c_void;
    fn class_addProtocol(class: *const c_void, protocol: *const c_void) -> Bool;
}

const K_FLAG_IS_FLOAT: u32 = 1;
const K_FLAG_NON_INTERLEAVED: u32 = 1 << 5;
/// SCStreamOutputType.
const OUTPUT_SCREEN: isize = 0;
const OUTPUT_AUDIO: isize = 1;
/// At most a second of sound waits for the encoder.
const MAX_QUEUED: usize = SAMPLE_RATE as usize * CHANNELS;

/// Sound captured on ScreenCaptureKit's queue, waiting for `read`.
struct Captured {
    samples: Mutex<VecDeque<i16>>,
    ready: Condvar,
}

/// The capture in progress (one per process: one session at a time).
static CURRENT: Mutex<Option<Arc<Captured>>> = Mutex::new(None);

define_class!(
    // SAFETY: NSObject has no subclassing requirements; no Drop impl.
    #[unsafe(super(NSObject))]
    #[name = "SunnaAudioOutput"]
    struct StreamOutput;

    impl StreamOutput {
        /// SCStreamOutput: a buffer of sound (or, ignored, of screen).
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        fn did_output(&self, _stream: *mut AnyObject, buffer: *mut c_void, kind: isize) {
            if kind == OUTPUT_AUDIO {
                // SAFETY: ScreenCaptureKit passes a valid audio CMSampleBuffer.
                unsafe { take_sound(buffer) }
            }
        }
    }
);

unsafe fn take_sound(buffer: *mut c_void) {
    let Some(captured) = CURRENT.lock().unwrap().clone() else { return };
    let description = CMSampleBufferGetFormatDescription(buffer);
    if description.is_null() {
        return;
    }
    let format = &*CMAudioFormatDescriptionGetStreamBasicDescription(description);
    let frames = CMSampleBufferGetNumSamples(buffer).max(0) as usize;
    let mut list = AudioBufferList2 {
        count: 0,
        buffers: [AudioBuffer { channels: 0, byte_size: 0, data: std::ptr::null_mut() }; 2],
    };
    let mut block: *const c_void = std::ptr::null();
    let status = CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
        buffer,
        std::ptr::null_mut(),
        &mut list,
        std::mem::size_of::<AudioBufferList2>(),
        std::ptr::null(),
        std::ptr::null(),
        1, // 16-byte alignment
        &mut block,
    );
    if status != 0 || list.count == 0 {
        return;
    }
    let float = format.format_flags & K_FLAG_IS_FLOAT != 0;
    let planar = format.format_flags & K_FLAG_NON_INTERLEAVED != 0;
    let channels = format.channels_per_frame.max(1) as usize;
    let read = |buffer: usize, index: usize| -> i16 {
        let data = list.buffers[buffer.min(list.count as usize - 1)].data;
        if float {
            let value = *(data as *const f32).add(index);
            (value.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
        } else {
            *(data as *const i16).add(index)
        }
    };
    let mut interleaved = Vec::with_capacity(frames * CHANNELS);
    for frame in 0..frames {
        let (left, right) = if planar {
            (read(0, frame), read(1, frame))
        } else {
            let base = frame * channels;
            (read(0, base), read(0, base + (channels > 1) as usize))
        };
        interleaved.push(left);
        interleaved.push(right);
    }
    if !block.is_null() {
        CFRelease(block);
    }
    let mut samples = captured.samples.lock().unwrap();
    samples.extend(interleaved);
    if samples.len() > MAX_QUEUED {
        let excess = samples.len() - MAX_QUEUED;
        samples.drain(..excess);
    }
    captured.ready.notify_one();
}

/// An object handed from a completion handler to the waiting thread.
struct Handoff(Retained<AnyObject>);

// SAFETY: the object is only passed across, then used by one thread.
unsafe impl Send for Handoff {}

/// Wait for an Objective-C completion handler's single call.
fn wait_for<T: Send + 'static>(what: &str, start: impl FnOnce(std::sync::mpsc::Sender<T>)) -> anyhow::Result<T> {
    let (sender, receiver) = std::sync::mpsc::channel();
    start(sender);
    receiver
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| anyhow::anyhow!("{what} didn't answer"))
}

unsafe fn error_text(error: *mut AnyObject) -> String {
    if error.is_null() {
        return "unknown error".into();
    }
    let text: *mut AnyObject = msg_send![error, localizedDescription];
    let utf8: *const c_char = msg_send![text, UTF8String];
    if utf8.is_null() {
        "unknown error".into()
    } else {
        CStr::from_ptr(utf8).to_string_lossy().into_owned()
    }
}

pub(crate) struct SystemAudioCapture {
    stream: Retained<AnyObject>,
    _output: Retained<StreamOutput>,
    captured: Arc<Captured>,
}

// SAFETY: the stream is only started and stopped (thread-safe in
// ScreenCaptureKit); samples arrive through `Captured`'s Mutex.
unsafe impl Send for SystemAudioCapture {}

impl SystemAudioCapture {
    pub(crate) fn open() -> anyhow::Result<Self> {
        autoreleasepool(|_| unsafe { Self::open_inner() })
    }

    unsafe fn open_inner() -> anyhow::Result<Self> {
        // ScreenCaptureKit looks for the SCStreamOutput protocol on outputs.
        let protocol = objc_getProtocol(c"SCStreamOutput".as_ptr());
        if !protocol.is_null() {
            class_addProtocol(StreamOutput::class() as *const _ as *const c_void, protocol);
        }

        // What can be captured: we need a display to hang the stream on.
        let content: Retained<AnyObject> = wait_for("ScreenCaptureKit", |sender| {
            let handler = block2::RcBlock::new(move |content: *mut AnyObject, error: *mut AnyObject| {
                let result = Retained::retain(content).map(Handoff).ok_or_else(|| error_text(error));
                let _ = sender.send(result);
            });
            let _: () = msg_send![class!(SCShareableContent), getShareableContentWithCompletionHandler: &*handler];
        })?
        .map_err(|error| anyhow::anyhow!("can't capture sound ({error}); allow Screen Recording for this app"))?
        .0;
        let displays: *mut AnyObject = msg_send![&*content, displays];
        let display: *mut AnyObject = msg_send![displays, firstObject];
        ensure!(!display.is_null(), "no display to capture sound with");

        let empty: *mut AnyObject = msg_send![class!(NSArray), array];
        let filter: Allocated<AnyObject> = msg_send![class!(SCContentFilter), alloc];
        let filter: Retained<AnyObject> = msg_send![filter, initWithDisplay: display, excludingWindows: empty];

        let config: Retained<AnyObject> = msg_send![class!(SCStreamConfiguration), new];
        let _: () = msg_send![&*config, setCapturesAudio: Bool::YES];
        let _: () = msg_send![&*config, setExcludesCurrentProcessAudio: Bool::YES];
        let _: () = msg_send![&*config, setSampleRate: SAMPLE_RATE as isize];
        let _: () = msg_send![&*config, setChannelCount: CHANNELS as isize];
        // Only the sound is used: keep the video side as small as it goes.
        let _: () = msg_send![&*config, setWidth: 2usize];
        let _: () = msg_send![&*config, setHeight: 2usize];
        let _: () = msg_send![&*config, setShowsCursor: Bool::NO];

        let stream: Allocated<AnyObject> = msg_send![class!(SCStream), alloc];
        let stream: Retained<AnyObject> =
            msg_send![stream, initWithFilter: &*filter, configuration: &*config, delegate: std::ptr::null_mut::<AnyObject>()];

        let captured = Arc::new(Captured { samples: Mutex::new(VecDeque::new()), ready: Condvar::new() });
        *CURRENT.lock().unwrap() = Some(Arc::clone(&captured));
        let output: Retained<StreamOutput> = msg_send![StreamOutput::class(), new];
        let queue = dispatch_queue_create(c"dev.sunna.audio".as_ptr(), std::ptr::null());
        for kind in [OUTPUT_AUDIO, OUTPUT_SCREEN] {
            let mut error: *mut AnyObject = std::ptr::null_mut();
            let added: Bool = msg_send![&*stream, addStreamOutput: &*output, type: kind, sampleHandlerQueue: queue, error: &mut error];
            ensure!(added.as_bool(), "can't receive sound: {}", error_text(error));
        }

        let started = wait_for("ScreenCaptureKit", |sender| {
            let handler = block2::RcBlock::new(move |error: *mut AnyObject| {
                let _ = sender.send(if error.is_null() { Ok(()) } else { Err(error_text(error)) });
            });
            let _: () = msg_send![&*stream, startCaptureWithCompletionHandler: &*handler];
        })?;
        if let Err(error) = started {
            *CURRENT.lock().unwrap() = None;
            bail!("can't capture sound ({error}); allow Screen Recording for this app");
        }
        tracing::info!("audio capture started (ScreenCaptureKit)");
        Ok(Self { stream, _output: output, captured })
    }
}

impl Capture for SystemAudioCapture {
    fn read(&mut self, frame: &mut [i16; FRAME_LEN]) -> anyhow::Result<()> {
        let deadline = Instant::now() + Duration::from_millis(100);
        let mut samples = self.captured.samples.lock().unwrap();
        while samples.len() < FRAME_LEN {
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                // Nothing delivered for a while: silence (the host then stops sending).
                frame.fill(0);
                return Ok(());
            };
            samples = self.captured.ready.wait_timeout(samples, left).unwrap().0;
        }
        for (slot, sample) in frame.iter_mut().zip(samples.drain(..FRAME_LEN)) {
            *slot = sample;
        }
        Ok(())
    }
}

impl Drop for SystemAudioCapture {
    fn drop(&mut self) {
        *CURRENT.lock().unwrap() = None;
        let handler = block2::RcBlock::new(|_error: *mut AnyObject| {});
        // SAFETY: stopping a started stream; the handler outlives the call.
        unsafe {
            let _: () = msg_send![&*self.stream, stopCaptureWithCompletionHandler: &*handler];
        }
    }
}
