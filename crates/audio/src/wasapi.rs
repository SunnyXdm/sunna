//! Shared-mode capture of the default Windows render endpoint.
//!
//! COM and WASAPI stay on a dedicated MTA thread. The public capture can move
//! between host threads without moving apartment-owned interfaces or handles.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_FAILED};
use windows::Win32::Media::Audio::*;
use windows::Win32::Media::KernelStreaming::{KSDATAFORMAT_SUBTYPE_PCM, WAVE_FORMAT_EXTENSIBLE};
use windows::Win32::Media::Multimedia::{KSDATAFORMAT_SUBTYPE_IEEE_FLOAT, WAVE_FORMAT_IEEE_FLOAT};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
    COINIT_MULTITHREADED,
};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};

use crate::wasapi_convert::{Converter, Format};
use crate::{Capture, FRAME_DURATION, FRAME_LEN, SAMPLE_RATE};

const POLL: Duration = Duration::from_millis(5);
const DEVICE_CHECK: Duration = Duration::from_millis(500);
type Frame = [i16; FRAME_LEN];

pub(crate) struct WasapiCapture {
    frames: mpsc::Receiver<Result<Frame>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl WasapiCapture {
    pub(crate) fn open() -> Result<Self> {
        let (frames_tx, frames) = mpsc::sync_channel(3);
        let (ready_tx, ready) = mpsc::sync_channel(1);
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("sunna-wasapi".into())
            .spawn(move || {
                if let Err(error) = capture_thread(&worker_stop, &ready_tx, &frames_tx) {
                    let _ = ready_tx.try_send(Err(anyhow::anyhow!("{error:#}")));
                    let _ = frames_tx.try_send(Err(error));
                }
            })
            .context("starting WASAPI capture thread")?;
        let capture = Self {
            frames,
            stop,
            thread: Some(thread),
        };
        ready
            .recv()
            .context("WASAPI capture thread stopped during startup")??;
        Ok(capture)
    }
}

impl Capture for WasapiCapture {
    fn read(&mut self, frame: &mut Frame) -> Result<()> {
        *frame = self
            .frames
            .recv()
            .context("WASAPI capture thread stopped")??;
        Ok(())
    }
}

impl Drop for WasapiCapture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Com;
impl Com {
    fn initialize() -> Result<Self> {
        // SAFETY: called only on our new worker thread; every successful call,
        // including S_FALSE, is balanced before that thread exits.
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        }
        Ok(Self)
    }
}
impl Drop for Com {
    fn drop(&mut self) {
        // SAFETY: all COM objects on this thread have already been dropped.
        unsafe {
            CoUninitialize();
        }
    }
}

struct Event(HANDLE);
impl Drop for Event {
    fn drop(&mut self) {
        // SAFETY: owned event, closed after the audio client has stopped.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

struct MixFormat(*mut WAVEFORMATEX);
impl Drop for MixFormat {
    fn drop(&mut self) {
        // SAFETY: GetMixFormat allocates this block with the COM allocator.
        unsafe {
            CoTaskMemFree(Some(self.0.cast()));
        }
    }
}

struct Stream {
    capture: IAudioCaptureClient,
    client: IAudioClient,
    event: Option<Event>,
    id: String,
    converter: Converter,
}

impl Stream {
    fn open(device: &IMMDevice, id: String) -> Result<Self> {
        let mut last_error = None;
        for convert in [true, false] {
            for events in [true, false] {
                match Self::try_open(device, id.clone(), convert, events) {
                    Ok(stream) => {
                        tracing::info!(
                            convert,
                            events,
                            rate = stream.converter.format.rate,
                            channels = stream.converter.format.channels,
                            "audio capture started (WASAPI loopback)"
                        );
                        return Ok(stream);
                    }
                    Err(error) => last_error = Some(error),
                }
            }
        }
        Err(last_error.unwrap())
            .context("opening WASAPI loopback with converted or native mix format")
    }

    fn try_open(device: &IMMDevice, id: String, convert: bool, events: bool) -> Result<Self> {
        // SAFETY: device and all returned interfaces are used on this MTA only.
        let client: IAudioClient = unsafe { device.Activate(CLSCTX_ALL, None)? };
        let requested = WAVEFORMATEX {
            wFormatTag: WAVE_FORMAT_IEEE_FLOAT as u16,
            nChannels: 2,
            nSamplesPerSec: SAMPLE_RATE,
            nAvgBytesPerSec: SAMPLE_RATE * 8,
            nBlockAlign: 8,
            wBitsPerSample: 32,
            cbSize: 0,
        };
        let mix = if convert {
            None
        } else {
            // SAFETY: live audio client; the returned allocation is freed by RAII.
            Some(MixFormat(unsafe { client.GetMixFormat()? }))
        };
        let format_ptr = mix
            .as_ref()
            .map_or(&requested as *const _, |mix| mix.0.cast_const());
        // SAFETY: either a stack WAVEFORMATEX or the full GetMixFormat allocation.
        let format = unsafe { read_format(format_ptr)? };
        let mut flags = AUDCLNT_STREAMFLAGS_LOOPBACK;
        if convert {
            flags |= AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;
        }
        let event = if events {
            flags |= AUDCLNT_STREAMFLAGS_EVENTCALLBACK;
            // SAFETY: unnamed auto-reset event, owned until the stream stops.
            Some(Event(unsafe {
                CreateEventW(None, false, false, PCWSTR::null())?
            }))
        } else {
            None
        };
        // SAFETY: format stays alive through Initialize. A fresh client is used
        // for every attempt because failed initialization need not be reusable.
        unsafe {
            client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, 0, 0, format_ptr, None)?;
            if let Some(event) = &event {
                client.SetEventHandle(event.0)?;
            }
        }
        // SAFETY: initialized loopback stream exposes IAudioCaptureClient.
        let capture = unsafe { client.GetService()? };
        let stream = Self {
            capture,
            client,
            event,
            id,
            converter: Converter::new(format),
        };
        // SAFETY: initialization and optional event setup completed successfully.
        unsafe {
            stream.client.Start()?;
        }
        Ok(stream)
    }

    fn drain(&mut self) -> Result<()> {
        // Bound each pass so a busy device cannot starve frame delivery.
        for _ in 0..64 {
            // SAFETY: live capture service used by its owning thread only.
            if unsafe { self.capture.GetNextPacketSize()? } == 0 {
                break;
            }
            let (mut data, mut frames, mut flags) = (std::ptr::null_mut(), 0, 0);
            // SAFETY: output pointers are valid. The returned buffer is borrowed
            // until ReleaseBuffer below, on this same thread.
            unsafe {
                self.capture
                    .GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
            }
            if flags & AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY.0 as u32 != 0 {
                self.converter.reset();
            }
            if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || frames == 0 {
                self.converter.push(None, frames as usize);
            } else if !data.is_null() {
                // SAFETY: WASAPI supplies frames * nBlockAlign bytes, with the
                // format validated at open. Byte decoding avoids alignment assumptions.
                let bytes = unsafe {
                    std::slice::from_raw_parts(
                        data,
                        frames as usize * self.converter.format.block_align,
                    )
                };
                self.converter.push(Some(bytes), frames as usize);
            } else {
                // SAFETY: always release a successfully acquired packet.
                unsafe {
                    self.capture.ReleaseBuffer(frames)?;
                }
                anyhow::bail!("WASAPI returned a null non-silent buffer");
            }
            // SAFETY: exactly the frame count returned by GetBuffer is released.
            unsafe {
                self.capture.ReleaseBuffer(frames)?;
            }
        }
        Ok(())
    }

    fn wait(&self, duration: Duration) -> Result<()> {
        if let Some(event) = &self.event {
            // Always time out within 5 ms: pre-1703 Windows may accept event
            // loopback but never signal it, and idle endpoints produce no packets.
            // Both cases still drain by polling and keep silence paced in real time.
            // SAFETY: this stream owns the live event for the duration of the wait.
            if unsafe { WaitForSingleObject(event.0, duration.as_millis().max(1) as u32) }
                == WAIT_FAILED
            {
                return Err(windows::core::Error::from_thread().into());
            }
        } else {
            std::thread::sleep(duration);
        }
        Ok(())
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        // SAFETY: stop on the creating thread before releasing interfaces/event.
        unsafe {
            let _ = self.client.Stop();
        }
    }
}

/// Read the optional extension only when the format explicitly includes it.
unsafe fn read_format(pointer: *const WAVEFORMATEX) -> Result<Format> {
    anyhow::ensure!(!pointer.is_null(), "WASAPI returned no mix format");
    // SAFETY: caller provides a WAVEFORMATEX and its declared extension bytes.
    let base = unsafe { pointer.read_unaligned() };
    let (float, channel_mask) = match u32::from(base.wFormatTag) {
        WAVE_FORMAT_PCM => (false, 0),
        WAVE_FORMAT_IEEE_FLOAT => (true, 0),
        WAVE_FORMAT_EXTENSIBLE => {
            anyhow::ensure!(
                base.cbSize as usize
                    >= std::mem::size_of::<WAVEFORMATEXTENSIBLE>()
                        - std::mem::size_of::<WAVEFORMATEX>(),
                "truncated WASAPI extensible format"
            );
            // SAFETY: the declared extension has the full extensible structure.
            let extended = unsafe { pointer.cast::<WAVEFORMATEXTENSIBLE>().read_unaligned() };
            let subtype = extended.SubFormat;
            anyhow::ensure!(
                subtype == KSDATAFORMAT_SUBTYPE_PCM || subtype == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT,
                "unsupported WASAPI mix subtype: {subtype:?}"
            );
            (
                subtype == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT,
                extended.dwChannelMask,
            )
        }
        tag => anyhow::bail!("unsupported WASAPI mix format tag: {tag}"),
    };
    Format {
        channels: base.nChannels as usize,
        rate: base.nSamplesPerSec,
        bits: base.wBitsPerSample,
        float,
        block_align: base.nBlockAlign as usize,
        channel_mask,
    }
    .validate()
}

fn default_device(enumerator: &IMMDeviceEnumerator) -> Result<(IMMDevice, String)> {
    // SAFETY: the enumerator was created on this thread and remains live.
    let device = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole) }
        .context("no default Windows audio output device; loopback capture is unavailable")?;
    // SAFETY: GetId returns a COM-allocated, NUL-terminated string.
    let id = unsafe {
        let pointer = device.GetId()?;
        let text = pointer.to_string();
        CoTaskMemFree(Some(pointer.0.cast()));
        text?
    };
    Ok((device, id))
}

fn capture_thread(
    stop: &AtomicBool,
    ready: &mpsc::SyncSender<Result<()>>,
    frames: &mpsc::SyncSender<Result<Frame>>,
) -> Result<()> {
    let _com = Com::initialize().context("initializing COM for WASAPI capture")?;
    // SAFETY: COM is initialized on this worker and outlives all its interfaces.
    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
    let (device, id) = default_device(&enumerator)?;
    let mut stream = Some(Stream::open(&device, id)?);
    ready
        .send(Ok(()))
        .context("WASAPI startup receiver closed")?;
    let mut due = Instant::now() + FRAME_DURATION;
    let mut device_check = Instant::now() + DEVICE_CHECK;
    let mut failures = 0u64;
    while !stop.load(Ordering::Relaxed) {
        if Instant::now() >= device_check {
            let result = default_device(&enumerator).and_then(|(device, id)| {
                if stream.as_ref().is_none_or(|stream| stream.id != id) {
                    // Drop the old endpoint and its queued samples before reopening.
                    stream = None;
                    stream = Some(Stream::open(&device, id)?);
                }
                Ok(())
            });
            if let Err(error) = result {
                stream = None;
                log_recovery(&mut failures, &error);
            }
            device_check = Instant::now() + DEVICE_CHECK;
        }
        if let Some(active) = &mut stream {
            if let Err(error) = active.drain() {
                // This includes AUDCLNT_E_DEVICE_INVALIDATED and resource/service
                // invalidation. Re-enumerate the default endpoint, never reuse the
                // invalid client. During unplug/replug gaps the stream stays silent.
                log_recovery(&mut failures, &error);
                stream = None;
                device_check = Instant::now();
            }
        }
        let now = Instant::now();
        if now >= due {
            let mut frame = [0; FRAME_LEN];
            if let Some(active) = &mut stream {
                active.converter.frame(&mut frame);
            }
            match frames.try_send(Ok(frame)) {
                Ok(()) | Err(mpsc::TrySendError::Full(_)) => {}
                Err(mpsc::TrySendError::Disconnected(_)) => break,
            }
            due += FRAME_DURATION;
            // Don't emit a burst of old silence after a scheduler stall.
            if due <= now {
                due = now + FRAME_DURATION;
            }
        }
        let wait = due.saturating_duration_since(Instant::now()).min(POLL);
        if let Some(active) = &stream {
            active.wait(wait)?;
        } else {
            std::thread::sleep(wait);
        }
    }
    Ok(())
}

fn log_recovery(failures: &mut u64, error: &anyhow::Error) {
    *failures = failures.saturating_add(1);
    if failures.is_power_of_two() {
        tracing::warn!(
            failures = *failures,
            "WASAPI endpoint unavailable; retrying with silence: {error:#}"
        );
    }
}
