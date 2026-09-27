//! The viewer's side: packets in (some lost, late or bunched up), sound out.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::{open_output, Decoder, Output, CHANNELS, FRAME_LEN, SAMPLE_RATE};

// How much sound is held back follows the network, much like WebRTC's
// NetEq: enough to cover how late packets have been arriving lately, no more,
// since every millisecond held back puts the sound behind the picture.

/// Held back before measurements come in.
const INITIAL_TARGET_MS: f64 = 40.0;
const MIN_TARGET_MS: f64 = 30.0;
const MAX_TARGET_MS: f64 = 250.0;
/// Headroom over the measured lateness.
const MARGIN_MS: f64 = 15.0;
/// Lateness is judged over the last 10 s of packets, and the buffer
/// covers the latest of them: in a stall only a few packets are very late,
/// so a quantile would miss exactly the stalls that cause gaps.
const JITTER_WINDOW: usize = 1000;
/// Sound starts in a burst (the host catching up with what it captured
/// while the session was being set up): the first 0.5 s isn't the network.
const WARMUP_PACKETS: u32 = 50;
/// A calmer network shrinks the buffer by 5 ms a second (per 10 ms packet).
const SHRINK_PER_PACKET_MS: f64 = 0.05;
/// This far past the target (a burst after a stall), skip straight back.
const MAX_OVER_TARGET: usize = 10 * FRAME_LEN; // 100 ms
/// Further past it than this (the target shrank, or the two machines'
/// clocks drift apart), drop 10 ms now and then until it's back.
const DRIFT_OVER_TARGET: usize = 2 * FRAME_LEN; // 20 ms
const SAMPLES_BETWEEN_DROPS: u64 = 20 * FRAME_LEN as u64; // 200 ms
/// Longest gap Opus's loss concealment fills. Longer ones are the host not
/// sending during silence: playback just resumes.
const MAX_CONCEAL: u64 = 8;

/// Decoded sound waiting for the speakers.
pub(crate) struct Ring {
    inner: Mutex<RingInner>,
}

struct RingInner {
    samples: VecDeque<i16>,
    /// Sound to hold back, in samples (see `Jitter`).
    target: usize,
    /// Played since the last catch-up drop.
    since_drop: u64,
    playing: bool,
    volume: f32,
    underruns: u64,
    skipped_samples: u64,
}

impl Ring {
    fn new() -> Self {
        Self {
            inner: Mutex::new(RingInner {
                samples: VecDeque::new(),
                target: ms_to_samples(INITIAL_TARGET_MS),
                since_drop: 0,
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
        if inner.samples.len() > inner.target + MAX_OVER_TARGET {
            let excess = inner.samples.len() - inner.target;
            let excess = excess - excess % CHANNELS;
            inner.samples.drain(..excess);
            inner.skipped_samples += excess as u64;
        }
    }

    fn set_target(&self, samples: usize) {
        self.inner.lock().unwrap().target = samples - samples % CHANNELS;
    }

    /// Fill `out` with the next sound, or silence while (re)buffering.
    pub(crate) fn pull(&self, out: &mut [i16]) {
        let mut inner = self.inner.lock().unwrap();
        if !inner.playing {
            if inner.samples.len() < inner.target {
                out.fill(0);
                return;
            }
            inner.playing = true;
        }
        inner.since_drop += out.len() as u64;
        if inner.samples.len() > inner.target + DRIFT_OVER_TARGET && inner.since_drop >= SAMPLES_BETWEEN_DROPS {
            inner.samples.drain(..FRAME_LEN);
            inner.skipped_samples += FRAME_LEN as u64;
            inner.since_drop = 0;
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
    /// What the buffer aims to hold, from the network's recent jitter.
    pub target_ms: u32,
}

fn ms_to_samples(ms: f64) -> usize {
    (ms * SAMPLE_RATE as f64 / 1000.0) as usize * CHANNELS
}

/// How late packets have been arriving, and so how much sound to hold back.
struct Jitter {
    start: Instant,
    /// Per packet: arrival time minus its place in the sound (ms). The
    /// smallest is the fastest trip; the rest are that much late.
    lateness: VecDeque<f64>,
    /// Packets since the sound (re)started.
    seen: u32,
    target_ms: f64,
}

impl Jitter {
    fn new() -> Self {
        Self {
            start: Instant::now(),
            lateness: VecDeque::with_capacity(JITTER_WINDOW),
            seen: 0,
            target_ms: INITIAL_TARGET_MS,
        }
    }

    /// The sound's timeline restarted (the host paused during silence).
    fn restart(&mut self) {
        self.lateness.clear();
        self.seen = 0;
    }

    /// Packet `seq` arrived at `now`; returns the target in ms.
    fn arrived(&mut self, seq: u64, now: Instant) -> f64 {
        self.seen = self.seen.saturating_add(1);
        if self.seen <= WARMUP_PACKETS {
            return self.target_ms;
        }
        let at_ms = now.saturating_duration_since(self.start).as_secs_f64() * 1000.0;
        if self.lateness.len() == JITTER_WINDOW {
            self.lateness.pop_front();
        }
        self.lateness.push_back(at_ms - seq as f64 * 10.0);
        let fastest = self.lateness.iter().copied().fold(f64::INFINITY, f64::min);
        let latest = self.lateness.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let wanted = (latest - fastest + MARGIN_MS).clamp(MIN_TARGET_MS, MAX_TARGET_MS);
        // Grow at once; shrink slowly, so one calm moment doesn't undo it.
        self.target_ms = if wanted >= self.target_ms { wanted } else { (self.target_ms - SHRINK_PER_PACKET_MS).max(wanted) };
        self.target_ms
    }
}

pub struct Player {
    ring: Arc<Ring>,
    decoder: Decoder,
    _output: Box<dyn Output>,
    expected: Option<u64>,
    jitter: Jitter,
    stats: PlayerStats,
}

impl Player {
    /// Start playing on this machine's speakers (see [`crate::open_output`]).
    pub fn start() -> anyhow::Result<Self> {
        let ring = Arc::new(Ring::new());
        let output = open_output(Arc::clone(&ring))?;
        Ok(Self {
            ring,
            decoder: Decoder::new()?,
            _output: output,
            expected: None,
            jitter: Jitter::new(),
            stats: PlayerStats::default(),
        })
    }

    /// One packet: `seq` counts 10 ms frames; `current` is this frame's Opus
    /// data and `previous` a copy of frame `seq - 1` (empty if none).
    pub fn push(&mut self, seq: u64, current: &[u8], previous: &[u8]) {
        if self.expected.is_none_or(|expected| seq > expected + MAX_CONCEAL) {
            // First packet, or sound again after the host paused for silence.
            self.jitter.restart();
        }
        let target_ms = self.jitter.arrived(seq, Instant::now());
        self.ring.set_target(ms_to_samples(target_ms));
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
            target_ms: self.jitter.target_ms.round() as u32,
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
    fn waits_for_the_target_then_plays() {
        let ring = ring_with(3);
        let mut out = [7i16; FRAME_LEN];
        ring.pull(&mut out);
        assert!(out.iter().all(|&s| s == 0), "silence until 40 ms are buffered");
        ring.push(&[1000i16; FRAME_LEN]);
        ring.pull(&mut out);
        assert!(out.iter().all(|&s| s == 1000));
    }

    #[test]
    fn a_bigger_target_waits_longer() {
        let ring = ring_with(6);
        ring.set_target(ms_to_samples(100.0));
        let mut out = [7i16; FRAME_LEN];
        ring.pull(&mut out);
        assert!(out.iter().all(|&s| s == 0), "60 ms isn't enough for a 100 ms target");
    }

    #[test]
    fn well_over_the_target_drops_10_ms_at_a_time() {
        let ring = ring_with(9); // 90 ms, target 40 ms
        let mut out = [0i16; FRAME_LEN];
        for _ in 0..20 {
            ring.pull(&mut out);
            ring.push(&[1000i16; FRAME_LEN]);
        }
        let inner = ring.inner.lock().unwrap();
        assert_eq!(inner.skipped_samples, FRAME_LEN as u64, "one 10 ms drop in 200 ms");
    }

    /// Feeds `seconds` of packets arriving on time, except every `every`th
    /// packet (and the ones queued behind it) held up by `stall_ms`.
    fn run(jitter: &mut Jitter, first_seq: u64, seconds: u64, stall_ms: u64, every: u64) -> f64 {
        let mut target = 0.0;
        for i in 0..seconds * 100 {
            let seq = first_seq + i;
            let mut at = seq * 10;
            if every > 0 {
                let into_cycle = seq % every * 10;
                if into_cycle < stall_ms {
                    at = seq * 10 - into_cycle + stall_ms; // released together
                }
            }
            target = jitter.arrived(seq, jitter.start + std::time::Duration::from_millis(at));
        }
        target
    }

    #[test]
    fn a_steady_network_keeps_the_buffer_small() {
        let mut jitter = Jitter::new();
        assert_eq!(run(&mut jitter, 0, 5, 0, 0), MIN_TARGET_MS);
    }

    #[test]
    fn stalls_grow_the_buffer_to_cover_them() {
        let mut jitter = Jitter::new();
        // 120 ms stalls twice a second, like the M2's Wi-Fi.
        let target = run(&mut jitter, 0, 5, 120, 50);
        assert!((120.0..=MAX_TARGET_MS).contains(&target), "{target}");
    }

    #[test]
    fn a_calm_network_shrinks_it_again_slowly() {
        let mut jitter = Jitter::new();
        let stalled = run(&mut jitter, 0, 5, 120, 50);
        let soon = run(&mut jitter, 500, 2, 0, 0);
        assert!(soon > stalled - 20.0, "shrinks by ~5 ms/s, got {soon}");
        let later = run(&mut jitter, 700, 40, 0, 0);
        assert_eq!(later, MIN_TARGET_MS);
    }

    #[test]
    fn the_burst_at_the_start_isnt_lateness() {
        let mut jitter = Jitter::new();
        // The first 20 packets all arrive 200 ms late, together.
        for seq in 0..20 {
            jitter.arrived(seq, jitter.start + std::time::Duration::from_millis(200));
        }
        assert_eq!(run(&mut jitter, 20, 5, 0, 0), MIN_TARGET_MS);
    }

    #[test]
    fn a_pause_for_silence_isnt_lateness() {
        let mut jitter = Jitter::new();
        run(&mut jitter, 0, 2, 0, 0);
        // The host paused for 3 s, counting a packet per 100 ms meanwhile.
        jitter.restart();
        let target = run(&mut jitter, 230, 2, 0, 0);
        assert_eq!(target, MIN_TARGET_MS);
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
        assert!(inner.samples.len() <= inner.target + MAX_OVER_TARGET);
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
