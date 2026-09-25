//! Host pipeline: frame source → encoder → packetize → QUIC datagrams,
//! plus the control stream (handshake, input → injector, RTT pings).
//!
//! The media loop runs on a dedicated OS thread: the frame source paces it
//! (event-driven capture on real backends), and `send_datagram` is synchronous
//! fire-and-forget, so no async hop sits between capture and the wire.
//!
//! Milestone 0 limitation: one session at a time; further connections are
//! served after the current one ends. Multi-guest co-play is Milestone 3.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use sunna_capture::FrameSource;
use sunna_codec::Encoder;
use sunna_input::InputInjector;
use sunna_proto::media::packetize;
use sunna_proto::messages::ControlMessage;
use sunna_transport::quinn::{Connection, SendDatagramError};
use sunna_transport::{ControlChannel, Server};

/// Builds a frame source producing frames of the given size.
pub type SourceFactory = Box<dyn Fn(u32, u32) -> anyhow::Result<Box<dyn FrameSource>> + Send + Sync>;
/// Builds an encoder for frames of the given size.
pub type EncoderFactory = Box<dyn Fn(u32, u32) -> anyhow::Result<Box<dyn Encoder>> + Send + Sync>;
pub type InjectorFactory = Box<dyn Fn() -> Box<dyn InputInjector> + Send + Sync>;

#[derive(Debug, Clone)]
pub struct HostConfig {
    pub name: String,
    /// Largest stream size (the capture's native size by default). Each
    /// session gets this, scaled down to fit the viewer's `max_size`.
    pub width: u32,
    pub height: u32,
    /// Shared session token clients must present; empty disables the check.
    pub token: String,
    pub fps: u32,
    /// Codec name announced in HelloAck; must match what the encoder factory builds.
    pub codec: String,
    /// Encoder ceiling; also the AIMD upper bound. Ignored by the raw codec.
    pub max_bitrate_bps: u32,
    /// Dev-only: drop this fraction of outgoing media datagrams (0.0..1.0)
    /// to exercise FEC and keyframe recovery.
    pub simulate_loss: f64,
}

/// Signals from the control loop into the media thread. This is the seam where
/// congestion control couples to the encoder: decisions apply to the *next*
/// frame (research/03 §4 integration rule).
struct SessionSignals {
    force_keyframe: AtomicBool,
    target_bitrate_bps: AtomicU32,
}

/// Accept-and-serve loop. Returns when the endpoint is closed.
pub async fn run_host(
    server: Server,
    config: HostConfig,
    new_source: SourceFactory,
    new_encoder: EncoderFactory,
    new_injector: InjectorFactory,
) -> anyhow::Result<()> {
    loop {
        let Some(accepted) = server.accept().await else {
            return Ok(());
        };
        match accepted {
            Ok(connection) => {
                let remote = connection.remote_address();
                tracing::info!(%remote, "session started");
                match serve(&connection, &config, &new_source, &new_encoder, &new_injector).await
                {
                    Ok(()) => tracing::info!(%remote, "session ended"),
                    Err(error) => tracing::info!(%remote, %error, "session ended"),
                }
            }
            Err(error) => tracing::warn!(%error, "incoming connection failed"),
        }
    }
}

async fn serve(
    connection: &Connection,
    config: &HostConfig,
    new_source: &SourceFactory,
    new_encoder: &EncoderFactory,
    new_injector: &InjectorFactory,
) -> anyhow::Result<()> {
    let mut control = ControlChannel::accept(connection).await?;
    let (width, height) = match control.recv().await? {
        ControlMessage::Hello {
            version,
            name,
            token,
            max_size,
        } => {
            if version != sunna_proto::PROTOCOL_VERSION {
                let reason = format!(
                    "protocol version mismatch: client {version}, host {} (rebuild both sides)",
                    sunna_proto::PROTOCOL_VERSION
                );
                let _ = control.send(&ControlMessage::Refused { reason: reason.clone() }).await;
                anyhow::bail!(reason);
            }
            if !config.token.is_empty() && !tokens_match(&token, &config.token) {
                let _ = control
                    .send(&ControlMessage::Refused { reason: "wrong session token".into() })
                    .await;
                anyhow::bail!("client {name:?} presented a wrong session token");
            }
            let size = fit_within((config.width, config.height), max_size);
            tracing::info!(
                client = %name,
                ?max_size,
                width = size.0,
                height = size.1,
                "hello received"
            );
            size
        }
        other => anyhow::bail!("expected Hello, got {other:?}"),
    };
    // Build the pipeline before acknowledging so a failure reaches the
    // client as a refusal instead of a silent stream.
    let pipeline = new_source(width, height).and_then(|source| Ok((source, new_encoder(width, height)?)));
    let (source, encoder) = match pipeline {
        Ok(pipeline) => pipeline,
        Err(error) => {
            let _ = control
                .send(&ControlMessage::Refused { reason: format!("host pipeline failed: {error}") })
                .await;
            return Err(error);
        }
    };
    control
        .send(&ControlMessage::HelloAck {
            version: sunna_proto::PROTOCOL_VERSION,
            name: config.name.clone(),
            width,
            height,
            fps: config.fps,
            codec: config.codec.clone(),
        })
        .await?;

    let stop = Arc::new(AtomicBool::new(false));
    let min_bitrate_bps = (config.max_bitrate_bps / 20).max(100_000);
    let signals = Arc::new(SessionSignals {
        force_keyframe: AtomicBool::new(false),
        // Start well below the ceiling and ramp up — starting hot congests
        // constrained paths for seconds before adaptation can react.
        target_bitrate_bps: AtomicU32::new(config.max_bitrate_bps.min(15_000_000)),
    });
    let media_thread = {
        let connection = connection.clone();
        let stop = Arc::clone(&stop);
        let signals = Arc::clone(&signals);
        let simulate_loss = config.simulate_loss;
        std::thread::spawn(move || {
            media_loop(
                connection,
                source,
                encoder,
                stop,
                signals,
                simulate_loss,
                min_bitrate_bps,
            )
        })
    };

    let mut injector = new_injector();
    let result = control_loop(
        &mut control,
        injector.as_mut(),
        &signals,
        config.max_bitrate_bps,
    )
    .await;
    injector.release_all();

    stop.store(true, Ordering::Relaxed);
    let _ = media_thread.join();
    result
}

/// Scale `native` down (never up) to fit `max`, keeping the aspect ratio;
/// even dimensions for the encoder.
fn fit_within(native: (u32, u32), max: Option<(u32, u32)>) -> (u32, u32) {
    let (width, height) = native;
    let scale = match max {
        Some((max_w, max_h)) if max_w > 0 && max_h > 0 => (max_w as f64 / width as f64)
            .min(max_h as f64 / height as f64)
            .min(1.0),
        _ => 1.0,
    };
    let even = |value: f64| ((value.round() as u32).max(2)) & !1;
    (even(width as f64 * scale), even(height as f64 * scale))
}

/// Constant-time comparison so response timing doesn't leak the token.
fn tokens_match(presented: &str, expected: &str) -> bool {
    let (a, b) = (presented.as_bytes(), expected.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn media_loop(
    connection: Connection,
    mut source: Box<dyn FrameSource>,
    mut encoder: Box<dyn Encoder>,
    stop: Arc<AtomicBool>,
    signals: Arc<SessionSignals>,
    simulate_loss: f64,
    min_bitrate_bps: u32,
) {
    /// Above this queued-bytes level the path is congested no matter how much
    /// buffer space remains. That is ~160 ms of queue at 15 Mbps, far more
    /// than we want; a time-based admission rule replaces it (research/07 step 5).
    const MAX_SENDER_BACKLOG: usize = 300 * 1024;

    let max_datagram = connection.max_datagram_size().unwrap_or(1200).min(1200);
    let mut last_local_cut = Instant::now() - Duration::from_secs(1);
    // Wire frame ids count frames that left the encoder — decoupled from
    // capture ids so skipped captures and encoder drops (which the reference
    // chain survives) don't look like loss to the client's gap detection. An
    // encoded frame dropped at the sender still consumes its id: that gap is
    // real, because later frames reference it.
    let mut wire_frame_id: u64 = 0;
    let mut consecutive_encode_failures: u32 = 0;
    let mut sender_dropped: u64 = 0;
    let mut window = HostWindow::new();
    // The encoder factory configured the ceiling; align it with the actual
    // starting target before the first frame.
    let mut applied_bitrate = signals.target_bitrate_bps.load(Ordering::Relaxed);
    encoder.set_target_bitrate(applied_bitrate);
    // Deterministic xorshift for dev loss simulation; no RNG dependency.
    let mut rng_state: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut roll = move || {
        rng_state ^= rng_state << 13;
        rng_state ^= rng_state >> 7;
        rng_state ^= rng_state << 17;
        rng_state as f64 / u64::MAX as f64
    };

    // A sender-side drop is the fastest congestion signal we have: cut the
    // bitrate immediately instead of waiting for receiver reports.
    let mut local_cut = |reason: &str, sender_dropped: u64, backlog: usize| {
        if last_local_cut.elapsed() > Duration::from_millis(500) {
            last_local_cut = Instant::now();
            let current = signals.target_bitrate_bps.load(Ordering::Relaxed);
            let next = (current * 3 / 4).max(min_bitrate_bps);
            signals.target_bitrate_bps.store(next, Ordering::Relaxed);
            tracing::debug!(
                sender_dropped,
                backlog,
                bitrate = next,
                reason,
                "cutting bitrate"
            );
        }
    };

    while !stop.load(Ordering::Relaxed) {
        if signals.force_keyframe.swap(false, Ordering::Relaxed) {
            encoder.request_keyframe();
        }
        let target_bitrate = signals.target_bitrate_bps.load(Ordering::Relaxed);
        if target_bitrate != applied_bitrate {
            encoder.set_target_bitrate(target_bitrate);
            applied_bitrate = target_bitrate;
        }

        let frame = match source.next_frame() {
            Ok(frame) => frame,
            Err(error) => {
                tracing::warn!(%error, "frame source failed, stopping media loop");
                break;
            }
        };
        // Admission before encode: if the path is already backed up, skip this
        // capture. The encoder never sees it, so the reference chain stays intact.
        let backlog = sunna_transport::DATAGRAM_SEND_BUFFER_SIZE
            .saturating_sub(connection.datagram_send_buffer_space());
        if backlog > MAX_SENDER_BACKLOG {
            sender_dropped += 1;
            window.sender_drops += 1;
            local_cut("backlog before encode", sender_dropped, backlog);
            window.maybe_report(applied_bitrate, backlog);
            continue;
        }
        let encode_started = Instant::now();
        let encoded = match encoder.encode(&frame) {
            Ok(Some(encoded)) => {
                consecutive_encode_failures = 0;
                window.encoded(encode_started.elapsed(), &encoded);
                encoded
            }
            Ok(None) => {
                window.encoder_drops += 1;
                // Normal under load/rate-control pressure; the next emitted
                // frame still references the last emitted one.
                tracing::debug!(frame_id = frame.frame_id, "encoder dropped frame");
                continue;
            }
            Err(error) => {
                consecutive_encode_failures += 1;
                if consecutive_encode_failures >= 120 {
                    tracing::warn!(%error, "encoder failing persistently, stopping media loop");
                    break;
                }
                tracing::debug!(%error, "encode failed, skipping frame");
                continue;
            }
        };
        let datagrams = packetize(
            wire_frame_id,
            encoded.capture_ts_us,
            encoded.keyframe,
            &encoded.data,
            max_datagram,
        );
        // Latest-frame-wins at the sender: a frame that doesn't fit the send
        // buffer is dropped rather than queued as stale video. It was already
        // encoded, so later frames reference it: burn its wire id (the client
        // sees the gap and won't decode P-frames against a missing reference)
        // and force a keyframe to restart the chain.
        let frame_bytes: usize = datagrams.iter().map(|datagram| datagram.len()).sum();
        let space = connection.datagram_send_buffer_space();
        if space < frame_bytes {
            sender_dropped += 1;
            window.sender_drops += 1;
            wire_frame_id += 1;
            signals.force_keyframe.store(true, Ordering::Relaxed);
            local_cut(
                "frame larger than send buffer space",
                sender_dropped,
                sunna_transport::DATAGRAM_SEND_BUFFER_SIZE.saturating_sub(space),
            );
            continue;
        }
        wire_frame_id += 1;
        for datagram in datagrams {
            if simulate_loss > 0.0 && roll() < simulate_loss {
                continue;
            }
            match connection.send_datagram(datagram) {
                Ok(()) => {}
                Err(SendDatagramError::ConnectionLost(_)) => return,
                Err(error) => {
                    // Non-fatal (e.g. transient too-large after PMTU change):
                    // drop the rest of this frame, keep streaming.
                    tracing::debug!(%error, "datagram send failed, dropping frame remainder");
                    break;
                }
            }
        }
        window.sent_frames += 1;
        window.sent_bytes += frame_bytes as u64;
        let backlog = sunna_transport::DATAGRAM_SEND_BUFFER_SIZE
            .saturating_sub(connection.datagram_send_buffer_space());
        window.maybe_report(applied_bitrate, backlog);
    }
}

/// Per-second host-side stats, logged as one `host window` event.
struct HostWindow {
    started: Instant,
    sent_frames: u64,
    sent_bytes: u64,
    keyframes: u64,
    encoder_drops: u64,
    sender_drops: u64,
    encode_us: Vec<u64>,
    capture_to_encoded_us: Vec<u64>,
}

impl HostWindow {
    fn new() -> Self {
        Self {
            started: Instant::now(),
            sent_frames: 0,
            sent_bytes: 0,
            keyframes: 0,
            encoder_drops: 0,
            sender_drops: 0,
            encode_us: Vec::with_capacity(128),
            capture_to_encoded_us: Vec::with_capacity(128),
        }
    }

    fn encoded(&mut self, took: Duration, frame: &sunna_codec::EncodedFrame) {
        self.encode_us.push(took.as_micros() as u64);
        self.capture_to_encoded_us
            .push(frame.encode_done_ts_us.saturating_sub(frame.capture_ts_us));
        if frame.keyframe {
            self.keyframes += 1;
        }
    }

    fn maybe_report(&mut self, bitrate_bps: u32, backlog_bytes: usize) {
        let elapsed = self.started.elapsed();
        if elapsed < Duration::from_secs(1) {
            return;
        }
        let pct = |samples: &mut Vec<u64>, p: usize| -> f64 {
            if samples.is_empty() {
                return 0.0;
            }
            samples.sort_unstable();
            samples[(samples.len() - 1) * p / 100] as f64 / 1000.0
        };
        tracing::info!(
            fps = self.sent_frames,
            mbps = format!("{:.1}", self.sent_bytes as f64 * 8.0 / elapsed.as_secs_f64() / 1e6),
            target_mbps = format!("{:.1}", bitrate_bps as f64 / 1e6),
            keyframes = self.keyframes,
            encoder_drops = self.encoder_drops,
            sender_drops = self.sender_drops,
            backlog_kb = backlog_bytes / 1024,
            encode_ms_p50 = pct(&mut self.encode_us, 50),
            encode_ms_p95 = pct(&mut self.encode_us, 95),
            capture_to_encoded_ms_p50 = pct(&mut self.capture_to_encoded_us, 50),
            capture_to_encoded_ms_p95 = pct(&mut self.capture_to_encoded_us, 95),
            "host window"
        );
        *self = Self::new();
    }
}

async fn control_loop(
    control: &mut ControlChannel,
    injector: &mut dyn InputInjector,
    signals: &SessionSignals,
    max_bitrate_bps: u32,
) -> anyhow::Result<()> {
    // AIMD v0.1: multiplicative decrease on dropped frames OR on latency
    // inflation (delayed frames never show up as drops — congestion queues
    // them instead), slow additive recovery on clean windows. Placeholder
    // until real delay-based CC (M1), but it stops multi-second spirals.
    let min_bitrate_bps = (max_bitrate_bps / 20).max(100_000);
    let mut p95_baseline_us: Option<u64> = None;
    loop {
        match control.recv().await {
            Ok(ControlMessage::Input(event)) => injector.inject(&event)?,
            Ok(ControlMessage::Ping { seq, t_us }) => {
                control
                    .send(&ControlMessage::Pong {
                        seq,
                        t_us: sunna_proto::now_us(),
                        peer_t_us: t_us,
                    })
                    .await?;
            }
            Ok(ControlMessage::RequestKeyframe) => {
                signals.force_keyframe.store(true, Ordering::Relaxed);
            }
            Ok(ControlMessage::ReceiverReport {
                frames_complete,
                frames_dropped,
                chunks_recovered,
                e2e_p95_us,
            }) => {
                let current = signals.target_bitrate_bps.load(Ordering::Relaxed);
                // Latency inflation vs the best p95 this session = queues are
                // building somewhere on the path.
                let inflated = if e2e_p95_us > 0 {
                    let baseline = p95_baseline_us.get_or_insert(e2e_p95_us);
                    if e2e_p95_us < *baseline {
                        *baseline = e2e_p95_us;
                    }
                    e2e_p95_us > *baseline + 100_000
                } else {
                    false
                };
                // Hold zone: mild inflation (> baseline + 40ms) means we're at
                // the path's edge — stop growing before we build a standing
                // queue, cut only on real inflation (> baseline + 100ms).
                let holding = if e2e_p95_us > 0 {
                    p95_baseline_us
                        .map_or(false, |baseline| e2e_p95_us > baseline + 40_000)
                } else {
                    false
                };
                let next = if frames_dropped > 0 || inflated {
                    if frames_dropped > 0 {
                        signals.force_keyframe.store(true, Ordering::Relaxed);
                    }
                    (current * 3 / 4).max(min_bitrate_bps)
                } else if holding {
                    current
                } else {
                    current
                        .saturating_add(max_bitrate_bps / 20)
                        .min(max_bitrate_bps)
                };
                if next != current {
                    signals.target_bitrate_bps.store(next, Ordering::Relaxed);
                }
                tracing::debug!(
                    frames_complete,
                    frames_dropped,
                    chunks_recovered,
                    e2e_p95_us,
                    bitrate = next,
                    "receiver report"
                );
            }
            Ok(ControlMessage::Bye) => return Ok(()),
            Ok(other) => tracing::debug!(?other, "unexpected control message"),
            // Peer closed or connection lost: treat as end of session.
            Err(error) => {
                tracing::debug!(%error, "control stream closed");
                return Ok(());
            }
        }
    }
}
