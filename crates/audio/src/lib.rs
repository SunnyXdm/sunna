//! Sound for Sunna sessions: capture what the host plays, encode it as Opus
//! in 10 ms packets, and play it on the viewer through a small buffer that
//! absorbs network jitter.
//!
//! Capture and playback are per platform: PulseAudio's simple API on Linux
//! (loaded at run time, so nothing is needed to build; PipeWire provides it
//! too), AudioQueue for playback on macOS, AAudio for playback on Android. Tests and the loopback bench use a
//! synthetic tone and a raw-PCM file instead (see [`open_capture`] and
//! [`Player::start`]).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[cfg(target_os = "android")]
mod aaudio;
mod opus;
mod player;
#[cfg(target_os = "linux")]
mod pulse;
#[cfg(target_os = "macos")]
mod macos;

pub use opus::{Decoder, Encoder};
pub use player::{Player, PlayerStats};

pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: usize = 2;
/// Samples per channel in one packet: 10 ms.
pub const FRAME_SAMPLES: usize = 480;
/// Interleaved samples in one packet.
pub const FRAME_LEN: usize = FRAME_SAMPLES * CHANNELS;
/// One packet's duration.
pub const FRAME_DURATION: Duration = Duration::from_millis(10);

/// A source of the host's sound, 10 ms at a time.
pub trait Capture: Send {
    /// Block until the next 10 ms of interleaved stereo is in `frame`.
    fn read(&mut self, frame: &mut [i16; FRAME_LEN]) -> anyhow::Result<()>;
}

/// Open the host's sound. `SUNNA_AUDIO_SOURCE` picks what to record: a
/// PulseAudio source name on Linux (default: the monitor of the default
/// output, i.e. whatever is playing), or `synthetic` for a test tone.
pub fn open_capture() -> anyhow::Result<Box<dyn Capture>> {
    let source = std::env::var("SUNNA_AUDIO_SOURCE").ok();
    if source.as_deref() == Some("synthetic") {
        return Ok(Box::new(ToneCapture::new(440.0)));
    }
    #[cfg(target_os = "linux")]
    {
        let device = source.unwrap_or_else(|| "@DEFAULT_MONITOR@".to_string());
        Ok(Box::new(pulse::PulseCapture::open(&device)?))
    }
    #[cfg(target_os = "macos")]
    {
        let _ = source;
        Ok(Box::new(macos::SystemAudioCapture::open()?))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = source;
        anyhow::bail!("no audio capture on this platform yet")
    }
}

/// A test tone, paced like a real device: a sine wave that steps up a fifth
/// every second (easy to hear, and to check for gaps in recordings).
pub struct ToneCapture {
    phase: f64,
    base_hz: f64,
    produced: u64,
    started: Instant,
}

impl ToneCapture {
    pub fn new(base_hz: f64) -> Self {
        Self { phase: 0.0, base_hz, produced: 0, started: Instant::now() }
    }
}

impl Capture for ToneCapture {
    fn read(&mut self, frame: &mut [i16; FRAME_LEN]) -> anyhow::Result<()> {
        let due = self.started + FRAME_DURATION * self.produced as u32;
        if let Some(wait) = due.checked_duration_since(Instant::now()) {
            std::thread::sleep(wait);
        }
        let second = self.produced / 100;
        let hz = self.base_hz * if second % 2 == 0 { 1.0 } else { 1.5 };
        for sample in frame.chunks_exact_mut(CHANNELS) {
            let value = (self.phase.sin() * 0.25 * i16::MAX as f64) as i16;
            sample.fill(value);
            self.phase = (self.phase + std::f64::consts::TAU * hz / SAMPLE_RATE as f64) % std::f64::consts::TAU;
        }
        self.produced += 1;
        Ok(())
    }
}

/// Where decoded sound goes. Playback runs until the returned handle drops.
pub(crate) trait Output: Send {}

/// Open the viewer's speakers, pulling from `ring`. `SUNNA_AUDIO_OUTPUT` set
/// to a path writes raw 48 kHz stereo s16le there instead (tests).
pub(crate) fn open_output(ring: Arc<player::Ring>) -> anyhow::Result<Box<dyn Output>> {
    if let Ok(path) = std::env::var("SUNNA_AUDIO_OUTPUT") {
        return Ok(Box::new(FileOutput::start(&path, ring)?));
    }
    #[cfg(target_os = "linux")]
    {
        Ok(Box::new(pulse::PulseOutput::start(ring)?))
    }
    #[cfg(target_os = "macos")]
    {
        Ok(Box::new(macos::QueueOutput::start(ring)?))
    }
    #[cfg(target_os = "android")]
    {
        Ok(Box::new(aaudio::AAudioOutput::start(ring)?))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "android")))]
    {
        let _ = ring;
        anyhow::bail!("no audio output on this platform yet")
    }
}

/// Real-time paced writer into a raw PCM file, for tests.
struct FileOutput {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl FileOutput {
    fn start(path: &str, ring: Arc<player::Ring>) -> anyhow::Result<Self> {
        use std::io::Write;
        let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let stop = Arc::clone(&stop);
            std::thread::Builder::new().name("sunna-audio-file".into()).spawn(move || {
                let started = Instant::now();
                let mut frame = [0i16; FRAME_LEN];
                let mut written: u32 = 0;
                while !stop.load(Ordering::Relaxed) {
                    let due = started + FRAME_DURATION * written;
                    if let Some(wait) = due.checked_duration_since(Instant::now()) {
                        std::thread::sleep(wait);
                    }
                    ring.pull(&mut frame);
                    let bytes: Vec<u8> = frame.iter().flat_map(|sample| sample.to_le_bytes()).collect();
                    if file.write_all(&bytes).is_err() {
                        break;
                    }
                    written += 1;
                }
                let _ = file.flush();
            })?
        };
        Ok(Self { stop, thread: Some(thread) })
    }
}

impl Output for FileOutput {}

impl Drop for FileOutput {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// True when a frame carries no sound worth sending.
pub fn is_silent(frame: &[i16]) -> bool {
    frame.iter().all(|sample| sample.unsigned_abs() <= 2)
}
