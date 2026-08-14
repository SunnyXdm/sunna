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

use sunna_capture::FrameSource;
use sunna_codec::Encoder;
use sunna_input::InputInjector;
use sunna_proto::media::packetize;
use sunna_proto::messages::ControlMessage;
use sunna_transport::quinn::{Connection, SendDatagramError};
use sunna_transport::{ControlChannel, Server};

pub type SourceFactory = Box<dyn Fn() -> Box<dyn FrameSource> + Send + Sync>;
pub type EncoderFactory = Box<dyn Fn() -> Box<dyn Encoder> + Send + Sync>;
pub type InjectorFactory = Box<dyn Fn() -> Box<dyn InputInjector> + Send + Sync>;

#[derive(Debug, Clone)]
pub struct HostConfig {
    pub name: String,
    pub width: u32,
    pub height: u32,
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
    match control.recv().await? {
        ControlMessage::Hello { version, name } => {
            anyhow::ensure!(
                version == sunna_proto::PROTOCOL_VERSION,
                "protocol version mismatch: client {version}, host {}",
                sunna_proto::PROTOCOL_VERSION
            );
            tracing::info!(client = %name, "hello received");
        }
        other => anyhow::bail!("expected Hello, got {other:?}"),
    }
    control
        .send(&ControlMessage::HelloAck {
            version: sunna_proto::PROTOCOL_VERSION,
            name: config.name.clone(),
            width: config.width,
            height: config.height,
            fps: config.fps,
            codec: config.codec.clone(),
        })
        .await?;

    let stop = Arc::new(AtomicBool::new(false));
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
        let source = new_source();
        let encoder = new_encoder();
        let simulate_loss = config.simulate_loss;
        std::thread::spawn(move || {
            media_loop(connection, source, encoder, stop, signals, simulate_loss)
        })
    };

    let injector = new_injector();
    let result = control_loop(&mut control, injector, &signals, config.max_bitrate_bps).await;

    stop.store(true, Ordering::Relaxed);
    let _ = media_thread.join();
    result
}

fn media_loop(
    connection: Connection,
    mut source: Box<dyn FrameSource>,
    mut encoder: Box<dyn Encoder>,
    stop: Arc<AtomicBool>,
    signals: Arc<SessionSignals>,
    simulate_loss: f64,
) {
    let max_datagram = connection.max_datagram_size().unwrap_or(1200).min(1200);
    let mut sent_frames: u64 = 0;
    // Wire frame ids are a contiguous series over frames actually *sent* —
    // decoupled from capture ids so encoder drops don't look like network
    // loss to the client's gap detection (which would request keyframes).
    let mut wire_frame_id: u64 = 0;
    let mut consecutive_encode_failures: u32 = 0;
    let mut sender_dropped: u64 = 0;
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
        let encoded = match encoder.encode(&frame) {
            Ok(Some(encoded)) => {
                consecutive_encode_failures = 0;
                encoded
            }
            Ok(None) => {
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
        // Latest-frame-wins at the sender: if the network is behind and this
        // frame won't fit in the send buffer, drop it here rather than queue
        // stale video (delivering seconds-old frames is worse than skipping).
        // Keyframes are always sent — overflow evicts older queued datagrams,
        // which is exactly the stale data we want gone.
        let frame_bytes: usize = datagrams.iter().map(|datagram| datagram.len()).sum();
        if !encoded.keyframe && connection.datagram_send_buffer_space() < frame_bytes {
            sender_dropped += 1;
            if sender_dropped % 30 == 1 {
                tracing::debug!(sender_dropped, "network backlog: dropping frames at sender");
            }
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
        sent_frames += 1;
        if sent_frames % 300 == 0 {
            tracing::debug!(sent_frames, "media loop alive");
        }
    }
}

async fn control_loop(
    control: &mut ControlChannel,
    mut injector: Box<dyn InputInjector>,
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
                let next = if frames_dropped > 0 || inflated {
                    if frames_dropped > 0 {
                        signals.force_keyframe.store(true, Ordering::Relaxed);
                    }
                    (current * 3 / 4).max(min_bitrate_bps)
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
