//! The viewer's side: packets in (some lost, late or bunched up), sound out.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::{open_output, Decoder, Output, CHANNELS, FRAME_LEN, SAMPLE_RATE};

/// Sound buffered before playback starts, and again after running dry:
/// enough to ride out Wi-Fi jitter without being noticeable.
const PREBUFFER: usize = 4 * FRAME_LEN; // 40 ms
/// Past this the buffer has built up (a burst after a stall, or the two
/// machines' clocks drifting apart): skip back to `TARGET`, so the sound
/// doesn't fall behind the picture.
const MAX_BUFFERED: usize = 15 * FRAME_LEN; // 150 ms
const TARGET: usize = 6 * FRAME_LEN; // 60 ms
/// Longest gap Opus's loss concealment fills. Longer ones are the host not
/// sending during silence: playback just resumes.
const MAX_CONCEAL: u64 = 8;

/// Decoded sound waiting for the speakers.
pub(crate) struct Ring {
    inner: Mutex<RingInner>,
}

struct RingInner {
    samples: VecDeque<i16>,
    playing: bool,
    volume: f32,
    underruns: u64,
    skipped_samples: u64,
}

impl Ring {
    fn new() -> Self {
        Self {
            inner: Mutex::new(RingInner {
                samples: VecDeque::with_capacity(MAX_BUFFERED + FRAME_LEN),
                playing: false,
                volume: 1.0,
                underruns: 0,
                skipped_samples: 0,
            }),
        }
    }

    fn push(&self, pcm: &[i16]) {
        let mut inner = self.inner.lock().unwrap();
        inner.samples.extend(pcm);
        if inner.samples.len() > MAX_BUFFERED {
            let excess = inner.samples.len() - TARGET;
            let excess = excess - excess % CHANNELS;
            inner.samples.drain(..excess);
            inner.skipped_samples += excess as u64;
        }
    }

    /// Fill `out` with the next sound, or silence while (re)buffering.
    pub(crate) fn pull(&self, out: &mut [i16]) {
        let mut inner = self.inner.lock().unwrap();
        if !inner.playing {
            if inner.samples.len() < PREBUFFER {
                out.fill(0);
                return;
            }
            inner.playing = true;
        }
        let volume = inner.volume;
        let available = out.len().min(inner.samples.len());
        for (slot, sample) in out.iter_mut().zip(inner.samples.drain(..available)) {
            *slot = if volume == 1.0 { sample } else { (sample as f32 * volume) as i16 };
        }
        if available < out.len() {
            out[available..].fill(0);
            inner.underruns += 1;
            inner.playing = false;
        }
    }
}

/// What playback has been through so far.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlayerStats {
    pub packets: u64,
    /// Lost packets rebuilt from the copy the next packet carries.
    pub recovered: u64,
    /// Lost packets papered over by Opus's concealment.
    pub concealed: u64,
    pub late: u64,
    /// Times the buffer ran dry (a short gap in the sound).
    pub underruns: u64,
    /// Sound skipped to catch up, in milliseconds.
    pub skipped_ms: u64,
    pub buffered_ms: u32,
}

pub struct Player {
    ring: Arc<Ring>,
    decoder: Decoder,
    _output: Box<dyn Output>,
    expected: Option<u64>,
    stats: PlayerStats,
}

impl Player {
    /// Start playing on this machine's speakers (see [`crate::open_output`]).
    pub fn start() -> anyhow::Result<Self> {
        let ring = Arc::new(Ring::new());
        let output = open_output(Arc::clone(&ring))?;
        Ok(Self { ring, decoder: Decoder::new()?, _output: output, expected: None, stats: PlayerStats::default() })
    }

    /// One packet: `seq` counts 10 ms frames; `current` is this frame's Opus
    /// data and `previous` a copy of frame `seq - 1` (empty if none).
    pub fn push(&mut self, seq: u64, current: &[u8], previous: &[u8]) {
        let mut pcm = [0i16; FRAME_LEN];
        match self.expected {
            Some(expected) if seq < expected => {
                self.stats.late += 1;
                return;
            }
            Some(expected) if seq > expected && seq - expected <= MAX_CONCEAL => {
                for missing in expected..seq {
                    let packet = (missing + 1 == seq && !previous.is_empty()).then_some(previous);
                    if packet.is_some() {
                        self.stats.recovered += 1;
                    } else {
                        self.stats.concealed += 1;
                    }
                    if self.decoder.decode(packet, &mut pcm).is_ok() {
                        self.ring.push(&pcm);
                    }
                }
            }
            _ => {}
        }
        self.expected = Some(seq + 1);
        match self.decoder.decode(Some(current), &mut pcm) {
            Ok(()) => {
                self.ring.push(&pcm);
                self.stats.packets += 1;
            }
            Err(error) => tracing::debug!(%error, seq, "audio packet didn't decode"),
        }
    }

    /// 0.0 (mute) to 1.0.
    pub fn set_volume(&self, volume: f32) {
        self.ring.inner.lock().unwrap().volume = volume.clamp(0.0, 1.0);
    }

    pub fn stats(&self) -> PlayerStats {
        let inner = self.ring.inner.lock().unwrap();
        let per_ms = (SAMPLE_RATE as u64 / 1000) * CHANNELS as u64;
        PlayerStats {
            underruns: inner.underruns,
            skipped_ms: inner.skipped_samples / per_ms,
            buffered_ms: (inner.samples.len() as u64 / per_ms) as u32,
            ..self.stats.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Capture, Encoder, ToneCapture};

    fn ring_with(frames: usize) -> Ring {
        let ring = Ring::new();
        for _ in 0..frames {
            ring.push(&[1000i16; FRAME_LEN]);
        }
        ring
    }

    #[test]
    fn waits_for_the_prebuffer_then_plays() {
        let ring = ring_with(3);
        let mut out = [7i16; FRAME_LEN];
        ring.pull(&mut out);
        assert!(out.iter().all(|&s| s == 0), "silence until 40 ms are buffered");
        ring.push(&[1000i16; FRAME_LEN]);
        ring.pull(&mut out);
        assert!(out.iter().all(|&s| s == 1000));
    }

    #[test]
    fn running_dry_counts_an_underrun_and_rebuffers() {
        let ring = ring_with(4);
        let mut out = [0i16; FRAME_LEN];
        for _ in 0..4 {
            ring.pull(&mut out);
        }
        ring.pull(&mut out);
        let inner = ring.inner.lock().unwrap();
        assert_eq!(inner.underruns, 1);
        assert!(!inner.playing);
    }

    #[test]
    fn a_build_up_is_skipped_back_to_the_target() {
        let ring = ring_with(20);
        let inner = ring.inner.lock().unwrap();
        assert!(inner.samples.len() <= MAX_BUFFERED);
        assert!(inner.skipped_samples > 0);
    }

    #[test]
    fn opus_roundtrip_keeps_the_tone() {
        let mut capture = ToneCapture::new(440.0);
        let mut encoder = Encoder::new(128_000).unwrap();
        let mut decoder = Decoder::new().unwrap();
        let mut frame = [0i16; FRAME_LEN];
        let mut decoded = [0i16; FRAME_LEN];
        let mut energy = 0f64;
        for _ in 0..10 {
            capture.read(&mut frame).unwrap();
            let packet = encoder.encode(&frame).unwrap();
            assert!(!packet.is_empty() && packet.len() < 400, "~128 kbps is ~160 bytes per 10 ms");
            decoder.decode(Some(&packet), &mut decoded).unwrap();
            energy += decoded.iter().map(|&s| (s as f64).powi(2)).sum::<f64>();
        }
        assert!(energy > 1e9, "the tone survives encoding");
    }
}
