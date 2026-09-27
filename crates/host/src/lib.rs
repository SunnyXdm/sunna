//! Host pipeline: frame source → encoder → packetize → QUIC datagrams,
//! plus the control stream (handshake, input → injector, RTT pings).
//!
//! The media loop runs on a dedicated OS thread: the frame source paces it
//! (event-driven capture on real backends), and `send_datagram` is synchronous
//! fire-and-forget, so no async hop sits between capture and the wire.
//!
//! One viewer owns the pipeline; probes and busy refusals run concurrently.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sunna_capture::FrameSource;
use sunna_codec::Encoder;
use sunna_input::InputInjector;
use sunna_proto::media::packetize;
use sunna_proto::messages::{ControlMessage, CursorShape, HostStats, StreamSettings};
use sunna_transport::quinn::{Connection, SendDatagramError};
use sunna_transport::{ControlChannel, Server};

#[derive(Debug, Clone)]
pub struct StreamConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub codec: String,
    pub bitrate_bps: u32,
    pub fast_lane: bool,
}

impl StreamConfig {
    fn apply(&self, native: (u32, u32), settings: &StreamSettings) -> anyhow::Result<Self> {
        let mut next = self.clone();
        if let Some(codec) = &settings.codec {
            anyhow::ensure!(
                matches!(codec.as_str(), "raw" | "h264" | "hevc"),
                "unknown codec: {codec}"
            );
            next.codec = codec.clone();
        }
        if let Some(size) = settings.max_size {
            anyhow::ensure!(size.0 >= 2 && size.1 >= 2, "max_size must be at least 2x2");
            (next.width, next.height) = fit_within(native, Some(size));
        }
        if let Some(fps) = settings.fps {
            anyhow::ensure!(fps > 0, "fps must be positive");
            next.fps = fps;
        }
        if let Some(kbps) = settings.max_bitrate_kbps {
            next.bitrate_bps = kbps
                .checked_mul(1000)
                .filter(|bps| *bps > 0)
                .ok_or_else(|| anyhow::anyhow!("invalid bitrate"))?;
        }
        if let Some(fast_lane) = settings.fast_lane {
            next.fast_lane = fast_lane;
        }
        Ok(next)
    }
}

/// Factories run on the media thread when a viewer changes settings.
pub type SourceFactory =
    Box<dyn Fn(&StreamConfig) -> anyhow::Result<Box<dyn FrameSource>> + Send + Sync>;
pub type EncoderFactory =
    Box<dyn Fn(&StreamConfig) -> anyhow::Result<Box<dyn Encoder>> + Send + Sync>;

pub type InjectorFactory = Box<dyn Fn() -> Box<dyn InputInjector> + Send + Sync>;

/// Build the encoder for `stream`. If its codec can't be encoded here (a
/// viewer asking for HEVC from a host without an HEVC encoder), use the
/// host's own codec instead and record it in `stream`: that's what the viewer
/// is told, and it decodes accordingly.
fn open_encoder(
    new_encoder: &EncoderFactory,
    stream: &mut StreamConfig,
    fallback: &str,
) -> anyhow::Result<Box<dyn Encoder>> {
    match new_encoder(stream) {
        Ok(encoder) => Ok(encoder),
        Err(error) if stream.codec != fallback => {
            tracing::warn!(
                requested = %stream.codec,
                using = fallback,
                reason = %format!("{error:#}"),
                "codec unavailable here; falling back"
            );
            stream.codec = fallback.to_string();
            new_encoder(stream)
        }
        Err(error) => Err(error),
    }
}

#[derive(Debug, Clone)]
pub struct HostConfig {
    /// Share text and images with the authenticated viewer.
    pub clipboard: bool,
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
    pub fast_lane: bool,
    /// Dev-only: drop this fraction of outgoing media datagrams (0.0..1.0)
    /// to exercise FEC and keyframe recovery.
    pub simulate_loss: f64,
    /// OS and device, told to launchers that probe with the right token.
    pub about: sunna_proto::messages::HostAbout,
    /// Share this machine's sound with viewers that want it.
    pub audio: bool,
}

/// Signals from the control loop into the media thread. This is the seam where
/// congestion control couples to the encoder: decisions apply to the *next*
/// frame (research/03 §4 integration rule).
struct SessionSignals {
    force_keyframe: AtomicBool,
    target_bitrate_bps: AtomicU32,
    max_bitrate_bps: AtomicU32,
    reconfigure: Mutex<Option<StreamSettings>>,
    /// Input events injected since the last host window (stats only).
    input_events: AtomicU32,
    /// The last host window's stats, waiting to go to the viewer.
    stats: std::sync::Mutex<Option<HostStats>>,
    /// Data chunks per parity datagram for delta frames, chosen from the loss
    /// QUIC measured recently (see `fec_group_for_loss`).
    fec_group: AtomicU32,
    /// Recently sent video, to send again what the viewer missed.
    sent: Mutex<SentFrames>,
    /// Datagrams sent again since the last host window (stats only).
    resent: AtomicU32,
}

/// The last second or so of video sent (bounded in bytes too), so the
/// viewer can ask for datagrams again: that takes a round trip, where
/// waiting for a keyframe took several and a big frame.
#[derive(Default)]
struct SentFrames {
    frames: std::collections::VecDeque<SentFrame>,
    bytes: usize,
}

struct SentFrame {
    epoch: u8,
    frame_id: u64,
    /// `datagrams[..data_count]` are the data chunks, in order; parity follows.
    data_count: usize,
    datagrams: Vec<bytes::Bytes>,
}

impl SentFrames {
    const MAX_FRAMES: usize = 90;
    const MAX_BYTES: usize = 8 << 20;

    fn push(&mut self, frame: SentFrame) {
        self.bytes += frame.datagrams.iter().map(|datagram| datagram.len()).sum::<usize>();
        self.frames.push_back(frame);
        while self.frames.len() > Self::MAX_FRAMES || self.bytes > Self::MAX_BYTES {
            let Some(old) = self.frames.pop_front() else { break };
            self.bytes -= old.datagrams.iter().map(|datagram| datagram.len()).sum::<usize>();
        }
    }

    /// The datagrams to send again: `chunks` of the frame's data, or all of
    /// it when empty. `None` if the frame is gone (or was never sent).
    fn resend(&self, epoch: u8, frame_id: u64, chunks: &[u16]) -> Option<Vec<bytes::Bytes>> {
        let frame = self.frames.iter().rev().find(|frame| frame.epoch == epoch && frame.frame_id == frame_id)?;
        let data = &frame.datagrams[..frame.data_count];
        Some(if chunks.is_empty() {
            data.to_vec()
        } else {
            chunks.iter().filter_map(|&chunk| data.get(chunk as usize).cloned()).collect()
        })
    }
}

/// Parity group size for a measured packet loss rate: more parity on lossy
/// links, where interleaved groups also cover longer bursts.
fn fec_group_for_loss(loss: f64) -> u32 {
    if loss >= 0.02 {
        3
    } else if loss >= 0.005 {
        4
    } else {
        sunna_proto::media::PARITY_GROUP as u32
    }
}

/// Keyframes are big (so more likely to lose a packet) and losing one costs a
/// freeze until the next: protect them more than delta frames.
fn keyframe_group(delta_group: u32) -> u32 {
    if delta_group < sunna_proto::media::PARITY_GROUP as u32 {
        2
    } else {
        4
    }
}

struct HostState {
    config: HostConfig,
    new_source: SourceFactory,
    new_encoder: EncoderFactory,
    new_injector: InjectorFactory,
    busy: AtomicBool,
    /// The connected viewer, while `busy`.
    viewer: std::sync::Mutex<Option<Viewer>>,
}

/// Who holds the viewer slot.
struct Viewer {
    name: String,
    device: String,
    connection: Connection,
}

struct SessionSlot(Arc<HostState>);

impl Drop for SessionSlot {
    fn drop(&mut self) {
        *self.0.viewer.lock().unwrap() = None;
        self.0.busy.store(false, Ordering::Release);
    }
}

impl HostState {
    /// Take the viewer slot. A viewer coming back from the same computer
    /// (its old session left behind by a crash, a quit, or a lost network)
    /// takes over its own session instead of finding the host in use.
    async fn claim(&self, name: &str, device: &str, connection: &Connection) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if self.busy.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_ok() {
                *self.viewer.lock().unwrap() = Some(Viewer {
                    name: name.to_string(),
                    device: device.to_string(),
                    connection: connection.clone(),
                });
                return Ok(());
            }
            let old = {
                let viewer = self.viewer.lock().unwrap();
                match viewer.as_ref() {
                    Some(old) if !device.is_empty() && old.device == device => Some(old.connection.clone()),
                    // Already leaving (its connection closed): wait for it.
                    Some(old) if old.connection.close_reason().is_some() => None,
                    Some(old) => return Err(format!("busy: {} is connected", old.name)),
                    // Between the flag and the record (just claimed or released).
                    None => None,
                }
            };
            if let Some(old) = old {
                tracing::info!(viewer = name, "the same viewer reconnected: ending its old session");
                old.close(0u32.into(), b"replaced by a new session from the same viewer");
            }
            if Instant::now() >= deadline {
                return Err("busy: the previous session is still ending".into());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

struct StopOnDrop(Arc<AtomicBool>);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// Accept-and-serve loop. Returns when the endpoint is closed.
pub async fn run_host(
    server: Server,
    config: HostConfig,
    new_source: SourceFactory,
    new_encoder: EncoderFactory,
    new_injector: InjectorFactory,
) -> anyhow::Result<()> {
    let host = Arc::new(HostState {
        config,
        new_source,
        new_encoder,
        new_injector,
        busy: AtomicBool::new(false),
        viewer: std::sync::Mutex::new(None),
    });
    let mut sessions = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            incoming = server.endpoint.accept() => {
                let Some(incoming) = incoming else { return Ok(()) };
                let host = Arc::clone(&host);
                sessions.spawn(async move {
                    match incoming.await {
                        Ok(connection) => {
                            let remote = connection.remote_address();
                            if let Err(error) = serve(&connection, host).await {
                                tracing::info!(%remote, %error, "session ended");
                            }
                        }
                        Err(error) => tracing::warn!(%error, "incoming connection failed"),
                    }
                });
            }
            Some(result) = sessions.join_next() => {
                if let Err(error) = result {
                    tracing::warn!(%error, "session task failed");
                }
            }
        }
    }
}

async fn serve(connection: &Connection, host: Arc<HostState>) -> anyhow::Result<()> {
    let config = &host.config;
    let mut control = ControlChannel::accept(connection).await?;
    let first = match control.recv().await {
        Ok(message) => message,
        // A first message this host can't read is from another version of
        // Sunna: say so, rather than just dropping the connection.
        Err(sunna_transport::TransportError::Codec(error)) => {
            let reason = format!(
                "protocol version mismatch: host {} (update both sides)",
                sunna_proto::PROTOCOL_VERSION
            );
            control.send(&ControlMessage::Refused { reason: reason.clone() }).await?;
            control.finish().await?;
            anyhow::bail!("{reason}; couldn't read the client's first message: {error}");
        }
        Err(error) => return Err(error.into()),
    };
    let settings = match first {
        ControlMessage::Probe { token } => {
            let token_ok = config.token.is_empty() || tokens_match(&token, &config.token);
            control
                .send(&ControlMessage::ProbeAck {
                    name: if token_ok {
                        config.name.clone()
                    } else {
                        String::new()
                    },
                    version: sunna_proto::PROTOCOL_VERSION,
                    busy: token_ok && host.busy.load(Ordering::Acquire),
                    token_ok,
                })
                .await?;
            if token_ok {
                let mut about = config.about.clone();
                about.width = config.width;
                about.height = config.height;
                control.send(&ControlMessage::HostInfo(about)).await?;
                let viewer = host
                    .viewer
                    .lock()
                    .unwrap()
                    .as_ref()
                    .map(|viewer| (viewer.name.clone(), viewer.device.clone()));
                if let Some((viewer, device)) = viewer {
                    control.send(&ControlMessage::InUse { viewer, device }).await?;
                }
            }
            control.finish().await?;
            return Ok(());
        }
        ControlMessage::Hello {
            version,
            name,
            token,
            stream,
            device,
        } => {
            if !config.token.is_empty() && !tokens_match(&token, &config.token) {
                control
                    .send(&ControlMessage::Refused {
                        reason: "wrong session token".into(),
                    })
                    .await?;
                control.finish().await?;
                anyhow::bail!("client {name:?} presented a wrong session token");
            }
            if version != sunna_proto::PROTOCOL_VERSION {
                let reason = format!(
                    "protocol version mismatch: client {version}, host {} (rebuild both sides)",
                    sunna_proto::PROTOCOL_VERSION
                );
                control
                    .send(&ControlMessage::Refused {
                        reason: reason.clone(),
                    })
                    .await?;
                control.finish().await?;
                anyhow::bail!(reason);
            }
            if let Err(reason) = host.claim(&name, &device, connection).await {
                control.send(&ControlMessage::Refused { reason }).await?;
                control.finish().await?;
                return Ok(());
            }
            tracing::info!(viewer = name, "viewer connected");
            stream
        }
        other => anyhow::bail!("expected Hello or Probe, got {other:?}"),
    };
    let slot = SessionSlot(Arc::clone(&host));
    let wants_audio = settings.audio.unwrap_or(true);
    let (width, height) = fit_within((config.width, config.height), None);
    let initial = StreamConfig {
        width,
        height,
        fps: config.fps,
        codec: config.codec.clone(),
        bitrate_bps: config.max_bitrate_bps,
        fast_lane: config.fast_lane,
    };
    // Refuse before acknowledging if the requested pipeline cannot be built.
    let builder = Arc::clone(&host);
    let pipeline = tokio::task::spawn_blocking(move || {
        initial
            .apply((builder.config.width, builder.config.height), &settings)
            .and_then(|mut stream| {
                let source = (builder.new_source)(&stream)?;
                let encoder =
                    open_encoder(&builder.new_encoder, &mut stream, &builder.config.codec)?;
                stream.fast_lane &= source.supports_tiles();
                Ok((stream, source, encoder))
            })
    })
    .await?;
    let (stream, source, encoder) = match pipeline {
        Ok(pipeline) => pipeline,
        Err(error) => {
            control
                .send(&ControlMessage::Refused {
                    reason: format!("host pipeline failed: {error}"),
                })
                .await?;
            control.finish().await?;
            return Err(error);
        }
    };
    let mut audio = AudioControl { available: config.audio, connection: connection.clone(), sender: None };
    audio.set(wants_audio);
    control
        .send(&ControlMessage::HelloAck {
            version: sunna_proto::PROTOCOL_VERSION,
            name: config.name.clone(),
            width: stream.width,
            height: stream.height,
            fps: stream.fps,
            codec: stream.codec.clone(),
            fast_lane: stream.fast_lane,
            audio: audio.on(),
        })
        .await?;

    let stop = Arc::new(AtomicBool::new(false));
    let _stop_on_drop = StopOnDrop(Arc::clone(&stop));
    let signals = Arc::new(SessionSignals {
        force_keyframe: AtomicBool::new(true),
        target_bitrate_bps: AtomicU32::new(stream.bitrate_bps.min(15_000_000)),
        max_bitrate_bps: AtomicU32::new(stream.bitrate_bps),
        reconfigure: Mutex::new(None),
        input_events: AtomicU32::new(0),
        stats: Mutex::new(None),
        fec_group: AtomicU32::new(sunna_proto::media::PARITY_GROUP as u32),
        sent: Mutex::new(SentFrames::default()),
        resent: AtomicU32::new(0),
    });
    let (updates_tx, updates_rx) = tokio::sync::mpsc::unbounded_channel();
    // The pointer's shape, which the viewer gives its own pointer. A shape's
    // pixels go once per session; after that its id is enough.
    // Its own channel: the session ends when `updates` closes (the media
    // thread is gone), which a watcher holding it would prevent.
    let (cursor_tx, cursor_rx) = tokio::sync::mpsc::unbounded_channel();
    let cursor_watch = {
        let mut sent = std::collections::HashSet::new();
        sunna_capture::cursor::watch(Arc::clone(&stop), move |image| {
            let first = sent.insert(image.id);
            let _ = cursor_tx.send(ControlMessage::Cursor(CursorShape {
                id: image.id,
                width: image.width,
                height: image.height,
                hot_x: image.hot_x,
                hot_y: image.hot_y,
                screen_width: image.screen_width,
                rgba: if first { image.rgba } else { Vec::new() },
            }));
        })
    };
    if let Err(error) = &cursor_watch {
        tracing::info!(%error, "no pointer shapes this session");
    }
    let media_thread = {
        let connection = connection.clone();
        let stop = Arc::clone(&stop);
        let signals = Arc::clone(&signals);
        let host = Arc::clone(&host);
        let runtime = tokio::runtime::Handle::current();
        std::thread::spawn(move || {
            let _slot = slot;
            media_loop(
                connection, source, encoder, stop, signals, host, stream, updates_tx, runtime,
            )
        })
    };
    let mut injector = (host.new_injector)();
    let result = control_loop(
        connection,
        control,
        injector.as_mut(),
        &signals,
        updates_rx,
        cursor_rx,
        config.clipboard,
        &mut audio,
    )
    .await;
    drop(audio);
    injector.release_all();
    stop.store(true, Ordering::Relaxed);
    // Teardown can flush hardware callbacks; keep the async runtime responsive.
    let _ = tokio::task::spawn_blocking(move || media_thread.join()).await;
    result
}

/// Sends fast-lane tile batches on their own reliable stream, in capture
/// order, and logs per-second totals.
async fn tile_writer(
    connection: Connection,
    mut batches: tokio::sync::mpsc::UnboundedReceiver<sunna_proto::tiles::TileBatch>,
) {
    let mut stream = match connection.open_uni().await {
        Ok(stream) => stream,
        Err(error) => {
            tracing::warn!(%error, "couldn't open the tile stream; fast lane off");
            return;
        }
    };
    if stream
        .write_all(&sunna_proto::tiles::TILE_STREAM_MAGIC)
        .await
        .is_err()
    {
        return;
    }
    let mut report = tokio::time::interval(Duration::from_secs(1));
    let (mut batch_count, mut tile_count, mut bytes, mut skipped) = (0u32, 0u32, 0usize, 0u32);
    // Token bucket: tiles only accelerate what the video will show a moment
    // later, so skipping a batch is always safe. Cap them so they never
    // compete with the video on slower links (a test session peaked at
    // ~1.8 MB/s of tiles).
    const BUDGET_BYTES_PER_SEC: f64 = 500_000.0; // ~4 Mbit/s
    const BURST_BYTES: f64 = 256.0 * 1024.0;
    let mut tokens = BURST_BYTES;
    let mut refilled = Instant::now();
    loop {
        tokio::select! {
            batch = batches.recv() => {
                let Some(batch) = batch else {
                    let _ = stream.finish();
                    return;
                };
                let Ok(encoded) = sunna_proto::tiles::encode(&batch) else { continue };
                let now = Instant::now();
                tokens = (tokens + now.duration_since(refilled).as_secs_f64() * BUDGET_BYTES_PER_SEC)
                    .min(BURST_BYTES);
                refilled = now;
                if encoded.len() as f64 > tokens {
                    skipped += 1;
                    continue;
                }
                tokens -= encoded.len() as f64;
                let mut framed = Vec::with_capacity(4 + encoded.len());
                framed.extend_from_slice(&(encoded.len() as u32).to_be_bytes());
                framed.extend_from_slice(&encoded);
                if let Err(error) = stream.write_all(&framed).await {
                    tracing::debug!(%error, "tile stream closed");
                    return;
                }
                batch_count += 1;
                tile_count += batch.tiles.len() as u32;
                bytes += framed.len();
            }
            _ = report.tick() => {
                if batch_count > 0 || skipped > 0 {
                    tracing::info!(
                        batches = batch_count,
                        tiles = tile_count,
                        kb = bytes / 1024,
                        over_budget = skipped,
                        "fast lane window"
                    );
                }
                batch_count = 0;
                tile_count = 0;
                bytes = 0;
                skipped = 0;
            }
        }
    }
}

/// Bitrate floor: below ~1/8 of the ceiling (and 3 Mbps) a desktop stream
/// turns to mush, which is worse than an occasional dropped frame.
fn min_bitrate(max_bitrate_bps: u32) -> u32 {
    (max_bitrate_bps / 8).max(3_000_000).min(max_bitrate_bps)
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

/// Frames allowed inside the encoder at once. The M1 has one encode engine
/// that works strictly frame by frame: with 2 in flight the second frame just
/// queued inside VideoToolbox (capture->encoded ~30 ms, no fps gain; test
/// build 7). With 1, the newest capture goes in the moment the engine is
/// free. Chips with several engines may benefit from 2:
/// SUNNA_ENCODE_IN_FLIGHT=2.
fn max_in_flight() -> usize {
    std::env::var("SUNNA_ENCODE_IN_FLIGHT")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .map_or(1, |value| value.clamp(1, 4))
}

/// Above this queued-bytes level the path is congested no matter how much
/// buffer space remains. That is ~160 ms of queue at 15 Mbps, far more than
/// we want; a time-based admission rule replaces it (research/08 §5.7).
const MAX_SENDER_BACKLOG: usize = 300 * 1024;

/// Immediate bitrate cut on a sender-side drop — the fastest congestion
/// signal we have — at most every 500 ms. Shared by both media threads.
struct LocalCut {
    last: std::sync::Mutex<Instant>,
}

impl LocalCut {
    fn cut(&self, signals: &SessionSignals, reason: &str, backlog: usize) {
        let mut last = self.last.lock().unwrap();
        if last.elapsed() > Duration::from_millis(500) {
            *last = Instant::now();
            let current = signals.target_bitrate_bps.load(Ordering::Relaxed);
            let next =
                (current / 4 * 3).max(min_bitrate(signals.max_bitrate_bps.load(Ordering::Relaxed)));
            signals.target_bitrate_bps.store(next, Ordering::Relaxed);
            tracing::debug!(backlog, bitrate = next, reason, "cutting bitrate");
        }
    }
}

fn send_backlog(connection: &Connection) -> usize {
    sunna_transport::DATAGRAM_SEND_BUFFER_SIZE
        .saturating_sub(connection.datagram_send_buffer_space())
}

/// Capture → encoder (this thread) and encoder output → wire (a send thread),
/// so finished frames go out the moment the encoder hands them back.
#[allow(clippy::too_many_arguments)]
fn media_loop(
    connection: Connection,
    mut source: Box<dyn FrameSource>,
    mut encoder: Box<dyn Encoder>,
    stop: Arc<AtomicBool>,
    signals: Arc<SessionSignals>,
    host: Arc<HostState>,
    mut stream: StreamConfig,
    updates: tokio::sync::mpsc::UnboundedSender<ControlMessage>,
    runtime: tokio::runtime::Handle,
) {
    let admission_drops = Arc::new(AtomicU32::new(0));
    let cuts = Arc::new(LocalCut {
        last: Mutex::new(Instant::now() - Duration::from_secs(1)),
    });
    let mut epoch = 0u8;
    let mut wire_frame_id = 0;
    loop {
        let writer = if stream.fast_lane {
            let (tiles_tx, tiles_rx) = tokio::sync::mpsc::unbounded_channel();
            source.set_tile_sink(Arc::new(move |batch| {
                let _ = tiles_tx.send(batch);
            }));
            Some(runtime.spawn(tile_writer(connection.clone(), tiles_rx)))
        } else {
            None
        };
        let (sink, outputs) = std::sync::mpsc::channel();
        let sender = {
            let connection = connection.clone();
            let stop = Arc::clone(&stop);
            let signals = Arc::clone(&signals);
            let admission_drops = Arc::clone(&admission_drops);
            let cuts = Arc::clone(&cuts);
            let simulate_loss = host.config.simulate_loss;
            std::thread::spawn(move || {
                send_loop(
                    connection,
                    outputs,
                    stop,
                    signals,
                    admission_drops,
                    cuts,
                    simulate_loss,
                    epoch,
                    wire_frame_id,
                )
            })
        };
        let replacement = loop {
            let Some(settings) = submit_loop(
                &connection,
                source.as_mut(),
                encoder.as_mut(),
                &stop,
                &signals,
                &sink,
                &admission_drops,
                &cuts,
            ) else {
                break None;
            };
            let started = Instant::now();
            let pipeline = stream
                .apply((host.config.width, host.config.height), &settings)
                .and_then(|mut next| {
                    let source = (host.new_source)(&next)?;
                    let encoder = open_encoder(&host.new_encoder, &mut next, &host.config.codec)?;
                    next.fast_lane &= source.supports_tiles();
                    Ok((next, source, encoder))
                });
            match pipeline {
                Ok(pipeline) => break Some((pipeline, started)),
                Err(error) => {
                    tracing::info!(elapsed_ms = started.elapsed().as_millis() as u64,
                        %error, "stream reconfigure failed");
                    let _ = updates.send(ControlMessage::SetStreamFailed {
                        reason: error.to_string(),
                    });
                }
            }
        };
        // Flush all old callbacks before changing the wire epoch.
        drop(encoder);
        drop(source);
        drop(sink);
        let sent = sender.join();
        if sent.is_err() {
            stop.store(true, Ordering::Relaxed);
        }
        wire_frame_id = sent.unwrap_or(wire_frame_id);
        if let Some(writer) = writer {
            writer.abort();
        }
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let Some(((next, next_source, next_encoder), started)) = replacement else {
            break;
        };
        stream = next;
        source = next_source;
        encoder = next_encoder;
        epoch = epoch.wrapping_add(1);
        signals
            .max_bitrate_bps
            .store(stream.bitrate_bps, Ordering::Relaxed);
        signals
            .target_bitrate_bps
            .store(stream.bitrate_bps.min(15_000_000), Ordering::Relaxed);
        signals.force_keyframe.store(true, Ordering::Relaxed);
        let _ = updates.send(ControlMessage::StreamChanged {
            epoch,
            width: stream.width,
            height: stream.height,
            fps: stream.fps,
            codec: stream.codec.clone(),
            fast_lane: stream.fast_lane,
        });
        tracing::info!(
            epoch,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "stream reconfigured"
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn submit_loop(
    connection: &Connection,
    source: &mut dyn FrameSource,
    encoder: &mut dyn Encoder,
    stop: &AtomicBool,
    signals: &SessionSignals,
    sink: &sunna_codec::EncoderSink,
    admission_drops: &AtomicU32,
    cuts: &LocalCut,
) -> Option<StreamSettings> {
    // The encoder factory configured the ceiling; align it with the actual
    // starting target before the first frame.
    let mut applied_bitrate = signals
        .target_bitrate_bps
        .load(Ordering::Relaxed)
        .min(signals.max_bitrate_bps.load(Ordering::Relaxed));
    encoder.set_target_bitrate(applied_bitrate);
    let mut consecutive_failures: u32 = 0;
    let max_in_flight = max_in_flight();
    tracing::info!(max_in_flight, "encode pipeline");

    while !stop.load(Ordering::Relaxed) {
        if let Some(settings) = signals.reconfigure.lock().unwrap().take() {
            return Some(settings);
        }
        if signals.force_keyframe.swap(false, Ordering::Relaxed) {
            encoder.request_keyframe();
        }
        let target_bitrate = signals
            .target_bitrate_bps
            .load(Ordering::Relaxed)
            .min(signals.max_bitrate_bps.load(Ordering::Relaxed));
        if target_bitrate != applied_bitrate {
            encoder.set_target_bitrate(target_bitrate);
            applied_bitrate = target_bitrate;
        }

        // Wait for an encoder slot *before* taking a frame, so the frame we
        // submit is the newest one.
        let waiting_since = Instant::now();
        while encoder.in_flight() >= max_in_flight && !stop.load(Ordering::Relaxed) {
            if waiting_since.elapsed() > Duration::from_secs(2) {
                tracing::warn!(in_flight = encoder.in_flight(), "encoder stalled for 2 s");
                break;
            }
            std::thread::sleep(Duration::from_micros(250));
        }

        let frame = match source.next_frame() {
            Ok(frame) => frame,
            Err(error) => {
                tracing::warn!(%error, "frame source failed, stopping media loop");
                stop.store(true, Ordering::Relaxed);
                break;
            }
        };
        // Admission before encode: if the path is already backed up, skip this
        // capture. The encoder never sees it, so the reference chain stays intact.
        let backlog = send_backlog(connection);
        if backlog > MAX_SENDER_BACKLOG {
            admission_drops.fetch_add(1, Ordering::Relaxed);
            cuts.cut(signals, "backlog before encode", backlog);
            continue;
        }
        match encoder.submit(&frame, sink) {
            Ok(()) => consecutive_failures = 0,
            Err(error) => {
                consecutive_failures += 1;
                if consecutive_failures >= 120 {
                    tracing::warn!(%error, "encoder failing persistently, stopping media loop");
                    stop.store(true, Ordering::Relaxed);
                    break;
                }
                tracing::debug!(%error, "submit failed, skipping frame");
            }
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn send_loop(
    connection: Connection,
    outputs: std::sync::mpsc::Receiver<sunna_codec::EncoderOutput>,
    stop: Arc<AtomicBool>,
    signals: Arc<SessionSignals>,
    admission_drops: Arc<AtomicU32>,
    cuts: Arc<LocalCut>,
    simulate_loss: f64,
    epoch: u8,
    mut wire_frame_id: u64,
) -> u64 {
    use sunna_codec::EncoderOutput;

    let max_datagram = connection.max_datagram_size().unwrap_or(1200).min(1200);
    // Wire frame ids count frames that left the encoder — decoupled from
    // capture ids so skipped captures and encoder drops (which the reference
    // chain survives) don't look like loss to the client's gap detection. An
    // encoded frame dropped at the sender still consumes its id: that gap is
    // real, because later frames reference it.
    let mut consecutive_failures: u32 = 0;
    let mut window = HostWindow::new();
    let mut quic_totals = QuicTotals::default();
    // Deterministic xorshift for dev loss simulation; no RNG dependency.
    let mut rng_state: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut roll = move || {
        rng_state ^= rng_state << 13;
        rng_state ^= rng_state >> 7;
        rng_state ^= rng_state << 17;
        rng_state as f64 / u64::MAX as f64
    };

    loop {
        let output = match outputs.recv_timeout(Duration::from_millis(250)) {
            Ok(output) => Some(output),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => None,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        };
        window.sender_drops += admission_drops.swap(0, Ordering::Relaxed) as u64;
        if stop.load(Ordering::Relaxed) {
            continue; // drain until the encoder side hangs up
        }
        match output {
            None => {}
            Some(EncoderOutput::Dropped { frame_id }) => {
                // Normal under load/rate-control pressure; the next emitted
                // frame still references the last emitted one.
                tracing::debug!(frame_id, "encoder dropped frame");
                window.encoder_drops += 1;
            }
            Some(EncoderOutput::Failed { frame_id, error }) => {
                consecutive_failures += 1;
                tracing::debug!(frame_id, %error, "encode failed, skipping frame");
                if consecutive_failures >= 120 {
                    tracing::warn!(%error, "encoder failing persistently, stopping media loop");
                    stop.store(true, Ordering::Relaxed);
                }
            }
            Some(EncoderOutput::Frame(encoded)) => {
                consecutive_failures = 0;
                window.encoded(&encoded);
                let delta_group = signals.fec_group.load(Ordering::Relaxed);
                let group = if encoded.keyframe { keyframe_group(delta_group) } else { delta_group };
                let datagrams = packetize(
                    epoch,
                    wire_frame_id,
                    encoded.capture_ts_us,
                    encoded.keyframe,
                    &encoded.data,
                    max_datagram,
                    group as usize,
                );
                // Latest-frame-wins at the sender: a frame that doesn't fit the
                // send buffer is dropped rather than queued as stale video. It
                // was already encoded, so later frames reference it: burn its
                // wire id (the client sees the gap and won't decode P-frames
                // against a missing reference) and force a keyframe.
                let frame_bytes: usize = datagrams.iter().map(|datagram| datagram.len()).sum();
                let space = connection.datagram_send_buffer_space();
                if space < frame_bytes {
                    window.sender_drops += 1;
                    wire_frame_id += 1;
                    signals.force_keyframe.store(true, Ordering::Relaxed);
                    cuts.cut(
                        &signals,
                        "frame larger than send buffer space",
                        send_backlog(&connection),
                    );
                } else {
                    let data_count = datagrams.iter().filter(|datagram| {
                        sunna_proto::media::MediaHeader::parse(datagram)
                            .is_some_and(|(header, _)| header.flags & sunna_proto::media::FLAG_PARITY == 0)
                    }).count();
                    signals.sent.lock().unwrap().push(SentFrame {
                        epoch,
                        frame_id: wire_frame_id,
                        data_count,
                        datagrams: datagrams.clone(),
                    });
                    wire_frame_id += 1;
                    for datagram in datagrams {
                        if simulate_loss > 0.0 && roll() < simulate_loss {
                            continue;
                        }
                        match connection.send_datagram(datagram) {
                            Ok(()) => {}
                            Err(SendDatagramError::ConnectionLost(_)) => {
                                stop.store(true, Ordering::Relaxed);
                                break;
                            }
                            Err(error) => {
                                // Non-fatal (e.g. transient too-large after PMTU
                                // change): drop the rest of this frame.
                                tracing::debug!(%error, "datagram send failed, dropping frame remainder");
                                break;
                            }
                        }
                    }
                    window.sent_frames += 1;
                    window.sent_bytes += frame_bytes as u64;
                }
            }
        }
        window.maybe_report(
            signals.target_bitrate_bps.load(Ordering::Relaxed),
            send_backlog(&connection),
            &signals,
            &connection,
            &mut quic_totals,
        );
    }
    wire_frame_id
}

/// QUIC's own counters at the last report, to log per-window changes.
#[derive(Default)]
struct QuicTotals {
    sent_packets: u64,
    lost_packets: u64,
    congestion_events: u64,
    /// Windows in a row that asked for less parity than we use.
    clean_windows: u32,
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

    fn encoded(&mut self, frame: &sunna_codec::EncodedFrame) {
        self.encode_us.push(frame.encode_us);
        self.capture_to_encoded_us
            .push(frame.encode_done_ts_us.saturating_sub(frame.capture_ts_us));
        if frame.keyframe {
            self.keyframes += 1;
        }
    }

    fn maybe_report(
        &mut self,
        bitrate_bps: u32,
        backlog_bytes: usize,
        signals: &SessionSignals,
        connection: &Connection,
        quic: &mut QuicTotals,
    ) {
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
        let sent_kbps = (self.sent_bytes as f64 * 8.0 / elapsed.as_secs_f64() / 1000.0) as u32;
        let path = connection.stats().path;
        let encode_p50 = pct(&mut self.encode_us, 50);
        let encode_p95 = pct(&mut self.encode_us, 95);
        *signals.stats.lock().unwrap() = Some(HostStats {
            fps: self.sent_frames as u32,
            sent_kbps,
            target_kbps: bitrate_bps / 1000,
            encode_us_p50: (encode_p50 * 1000.0) as u32,
            encode_us_p95: (encode_p95 * 1000.0) as u32,
            keyframes: self.keyframes as u32,
        });
        tracing::info!(
            fps = self.sent_frames,
            mbps = format!(
                "{:.1}",
                self.sent_bytes as f64 * 8.0 / elapsed.as_secs_f64() / 1e6
            ),
            target_mbps = format!("{:.1}", bitrate_bps as f64 / 1e6),
            keyframes = self.keyframes,
            encoder_drops = self.encoder_drops,
            sender_drops = self.sender_drops,
            backlog_kb = backlog_bytes / 1024,
            encode_ms_p50 = encode_p50,
            encode_ms_p95 = encode_p95,
            capture_to_encoded_ms_p50 = pct(&mut self.capture_to_encoded_us, 50),
            capture_to_encoded_ms_p95 = pct(&mut self.capture_to_encoded_us, 95),
            input_events = signals.input_events.swap(0, Ordering::Relaxed),
            // What QUIC itself saw this window: loss it detected, how often
            // its congestion control backed off, its window and round trip.
            quic_sent = path.sent_packets - quic.sent_packets,
            quic_lost = path.lost_packets - quic.lost_packets,
            quic_backoffs = path.congestion_events - quic.congestion_events,
            quic_cwnd_kb = path.cwnd / 1024,
            quic_rtt_ms = path.rtt.as_secs_f64() * 1000.0,
            fec_group = signals.fec_group.load(Ordering::Relaxed),
            resent = signals.resent.swap(0, Ordering::Relaxed),
            "host window"
        );
        // More parity as soon as loss shows up; back off only after a run of
        // clean windows, so a lull in a lossy link doesn't strip protection.
        let sent = path.sent_packets - quic.sent_packets;
        let lost = path.lost_packets - quic.lost_packets;
        let wanted = fec_group_for_loss(lost as f64 / sent.max(1) as f64);
        let current = signals.fec_group.load(Ordering::Relaxed);
        if wanted < current {
            signals.fec_group.store(wanted, Ordering::Relaxed);
            quic.clean_windows = 0;
        } else if wanted > current {
            quic.clean_windows += 1;
            if quic.clean_windows >= 5 {
                signals.fec_group.store(wanted, Ordering::Relaxed);
                quic.clean_windows = 0;
            }
        } else {
            quic.clean_windows = 0;
        }
        *quic = QuicTotals {
            sent_packets: path.sent_packets,
            lost_packets: path.lost_packets,
            congestion_events: path.congestion_events,
            clean_windows: quic.clean_windows,
        };
        *self = Self::new();
    }
}

async fn control_loop(
    connection: &Connection,
    control: ControlChannel,
    injector: &mut dyn InputInjector,
    signals: &SessionSignals,
    mut updates: tokio::sync::mpsc::UnboundedReceiver<ControlMessage>,
    mut cursor: tokio::sync::mpsc::UnboundedReceiver<ControlMessage>,
    clipboard: bool,
    audio: &mut AudioControl,
) -> anyhow::Result<()> {
    // `recv` isn't cancellation-safe, so it gets its own task (as in the
    // client) and the loop below can also wake up to send stats.
    let (mut control, mut control_rx) = control.into_split();
    let (message_tx, mut messages) = tokio::sync::mpsc::channel(4);
    let reader = tokio::spawn(async move {
        loop {
            let message = control_rx.recv().await;
            let closed = message.is_err();
            if message_tx.send(message).await.is_err() || closed {
                break;
            }
        }
    });
    let mut clipboard = sunna_clipboard::ClipboardSession::start(clipboard);
    let (incoming_tx, mut incoming) = tokio::sync::mpsc::channel(2);
    let acceptor = tokio::spawn(accept_clipboard_streams(connection.clone(), incoming_tx));
    let result = handle_control(
        connection,
        &mut control,
        &mut messages,
        injector,
        signals,
        &mut updates,
        &mut cursor,
        &mut clipboard,
        &mut incoming,
        audio,
    )
    .await;
    reader.abort();
    acceptor.abort();
    result
}

/// The session's sound: while on, a thread captures, encodes and sends it.
struct AudioControl {
    /// Sound sharing is enabled and hasn't failed this session.
    available: bool,
    connection: Connection,
    sender: Option<AudioSender>,
}

impl AudioControl {
    fn set(&mut self, on: bool) {
        if !on {
            self.sender = None;
            return;
        }
        if self.sender.is_some() || !self.available {
            return;
        }
        match AudioSender::start(self.connection.clone()) {
            Ok(sender) => self.sender = Some(sender),
            Err(error) => {
                tracing::info!(reason = %format!("{error:#}"), "no sound this session");
                self.available = false;
            }
        }
    }

    fn on(&self) -> bool {
        self.sender.is_some()
    }
}

struct AudioSender {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl AudioSender {
    /// 128 kbps stereo Opus: transparent for music, and small next to video.
    const BITRATE: i32 = 128_000;
    /// Stop sending after this many silent frames (200 ms); the viewer's
    /// buffer runs dry into silence, and the link carries nothing.
    const QUIET_FRAMES: u32 = 20;

    fn start(connection: Connection) -> anyhow::Result<Self> {
        let mut capture = sunna_audio::open_capture()?;
        let mut encoder = sunna_audio::Encoder::new(Self::BITRATE)?;
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let stop = Arc::clone(&stop);
            std::thread::Builder::new().name("sunna-audio-send".into()).spawn(move || {
                let mut frame = [0i16; sunna_audio::FRAME_LEN];
                let mut previous: Vec<u8> = Vec::new();
                let (mut seq, mut quiet) = (0u64, 0u32);
                let (mut window_start, mut sent, mut bytes, mut skipped) = (Instant::now(), 0u64, 0u64, 0u64);
                while !stop.load(Ordering::Relaxed) {
                    if let Err(error) = capture.read(&mut frame) {
                        tracing::warn!(reason = %format!("{error:#}"), "sound capture stopped");
                        break;
                    }
                    seq += 1;
                    quiet = if sunna_audio::is_silent(&frame) { quiet + 1 } else { 0 };
                    if quiet > Self::QUIET_FRAMES {
                        previous.clear();
                        skipped += 1;
                        continue;
                    }
                    let packet = match encoder.encode(&frame) {
                        Ok(packet) => packet,
                        Err(error) => {
                            tracing::debug!(%error, "audio encode failed");
                            continue;
                        }
                    };
                    let datagram = sunna_proto::media::audio_datagram(seq, sunna_proto::now_us(), &packet, &previous);
                    bytes += datagram.len() as u64;
                    match connection.send_datagram(datagram) {
                        Ok(()) => sent += 1,
                        Err(SendDatagramError::ConnectionLost(_)) => break,
                        Err(error) => tracing::debug!(%error, "audio datagram not sent"),
                    }
                    previous = packet;
                    if window_start.elapsed() >= Duration::from_secs(10) {
                        tracing::info!(
                            sent,
                            silent_skipped = skipped,
                            kbps = bytes * 8 / window_start.elapsed().as_millis().max(1) as u64,
                            "audio window"
                        );
                        (window_start, sent, bytes, skipped) = (Instant::now(), 0, 0, 0);
                    }
                }
            })?
        };
        tracing::info!("sharing sound");
        Ok(Self { stop, thread: Some(thread) })
    }
}

impl Drop for AudioSender {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The viewer sends each clipboard copy on a unidirectional stream of its
/// own, so a large image never delays input on the control stream.
async fn accept_clipboard_streams(
    connection: Connection,
    incoming: tokio::sync::mpsc::Sender<sunna_proto::messages::ClipboardData>,
) {
    while let Ok(mut stream) = connection.accept_uni().await {
        let incoming = incoming.clone();
        tokio::spawn(async move {
            let mut magic = [0u8; 4];
            if stream.read_exact(&mut magic).await.is_err()
                || magic != sunna_transport::CLIPBOARD_STREAM_MAGIC
            {
                tracing::debug!("ignoring an unknown unidirectional stream");
                return;
            }
            match sunna_transport::read_clipboard(stream).await {
                Ok(data) => {
                    let _ = incoming.send(data).await;
                }
                Err(error) => tracing::debug!(%error, "clipboard stream failed"),
            }
        });
    }
}

/// Send a clipboard copy on its own stream, off the control loop.
fn spawn_clipboard_send(connection: &Connection, data: sunna_proto::messages::ClipboardData) {
    let connection = connection.clone();
    tokio::spawn(async move {
        let (kind, size) = (data.kind, data.data.len());
        match sunna_transport::send_clipboard(&connection, &data).await {
            Ok(()) => tracing::info!(?kind, size, "clipboard sent"),
            Err(error) => tracing::warn!(%error, "clipboard send failed"),
        }
    });
}

#[allow(clippy::too_many_arguments)]
async fn handle_control(
    connection: &Connection,
    control: &mut sunna_transport::ControlSender,
    messages: &mut tokio::sync::mpsc::Receiver<sunna_transport::Result<ControlMessage>>,
    injector: &mut dyn InputInjector,
    signals: &SessionSignals,
    updates: &mut tokio::sync::mpsc::UnboundedReceiver<ControlMessage>,
    cursor: &mut tokio::sync::mpsc::UnboundedReceiver<ControlMessage>,
    clipboard: &mut sunna_clipboard::ClipboardSession,
    incoming_clipboard: &mut tokio::sync::mpsc::Receiver<sunna_proto::messages::ClipboardData>,
    audio: &mut AudioControl,
) -> anyhow::Result<()> {
    // AIMD v0.2: multiplicative decrease on real loss (>= 5% of frames in
    // the window) or on latency inflation; hold on minor loss (Wi-Fi drops
    // the odd frame, and cutting for it parks desktop streams at an ugly
    // floor); slow additive recovery on clean windows. Placeholder until
    // delay-based CC (research/08 §5.7).
    let mut p95_baseline_us: Option<u64> = None;
    let mut stats_interval = tokio::time::interval(Duration::from_secs(1));
    loop {
        let message = tokio::select! {
            update = updates.recv() => {
                let Some(update) = update else { return Ok(()) };
                control.send(&update).await?;
                continue;
            }
            Some(shape) = cursor.recv() => {
                control.send(&shape).await?;
                continue;
            }
            data = clipboard.next() => {
                spawn_clipboard_send(connection, data);
                continue;
            }
            Some(data) = incoming_clipboard.recv() => {
                clipboard.receive(data);
                continue;
            }
            message = messages.recv() => match message {
                Some(message) => message,
                None => return Ok(()),
            },
            _ = stats_interval.tick() => {
                let stats = signals.stats.lock().unwrap().take();
                if let Some(stats) = stats {
                    control.send(&ControlMessage::HostStats(stats)).await?;
                }
                continue;
            }
        };
        match message {
            Ok(ControlMessage::SetAudio(on)) => audio.set(on),
            Ok(ControlMessage::SetStream(settings)) => {
                *signals.reconfigure.lock().unwrap() = Some(settings);
            }
            Ok(ControlMessage::Input(event)) => {
                injector.inject(&event)?;
                signals.input_events.fetch_add(1, Ordering::Relaxed);
            }
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
            Ok(ControlMessage::Resend { epoch, frame_id, chunks }) => {
                let resend = signals.sent.lock().unwrap().resend(epoch, frame_id, &chunks);
                match resend {
                    Some(datagrams) => {
                        signals.resent.fetch_add(datagrams.len() as u32, Ordering::Relaxed);
                        for datagram in datagrams {
                            if connection.send_datagram(datagram).is_err() {
                                break;
                            }
                        }
                    }
                    // Too old, or dropped before sending: start over.
                    None => signals.force_keyframe.store(true, Ordering::Relaxed),
                }
            }
            Ok(ControlMessage::ReceiverReport {
                frames_complete,
                frames_dropped,
                chunks_recovered,
                e2e_p95_us,
            }) => {
                let max_bitrate_bps = signals.max_bitrate_bps.load(Ordering::Relaxed);
                let min_bitrate_bps = min_bitrate(max_bitrate_bps);
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
                    p95_baseline_us.map_or(false, |baseline| e2e_p95_us > baseline + 40_000)
                } else {
                    false
                };
                // The client requests keyframes itself when it actually needs
                // one; forcing another here only adds IDR bursts.
                // Loss alone doesn't cut: on Wi-Fi it's mostly random, so a
                // lower bitrate wouldn't reduce it, only the picture. Parity
                // handles loss; QUIC's BBR and the send backlog handle real
                // congestion; here, only growing delay cuts.
                let next = if inflated {
                    (current / 4 * 3).max(min_bitrate_bps)
                } else if holding || frames_dropped > 0 {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_codec_falls_back_to_the_hosts_own() {
        // A host that can only do H.264, asked for HEVC.
        let factory: EncoderFactory = Box::new(|stream: &StreamConfig| {
            anyhow::ensure!(stream.codec != "hevc", "no HEVC encoder");
            sunna_codec::make_encoder("raw", stream.width, stream.height, stream.fps, stream.bitrate_bps)
        });
        let mut stream = StreamConfig {
            width: 64,
            height: 64,
            fps: 30,
            codec: "hevc".into(),
            bitrate_bps: 1_000_000,
            fast_lane: false,
        };
        assert!(open_encoder(&factory, &mut stream, "h264").is_ok());
        assert_eq!(stream.codec, "h264", "the viewer must be told the codec actually used");

        // No fallback left: the error surfaces.
        let mut stream = StreamConfig { codec: "hevc".into(), ..stream };
        assert!(open_encoder(&factory, &mut stream, "hevc").is_err());
    }
}
