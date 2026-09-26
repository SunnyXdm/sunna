//! Client pipeline: datagrams → reassemble (latest-frame-wins, XOR-FEC
//! recovery) → decode thread (keyframe-gated) → `on_frame`.
//!
//! The network task only reassembles and forwards; decoding runs on its own
//! thread so a slow decode never delays input, pings or keyframe requests.
//!
//! Recovery model: FEC repairs single losses per parity group with no feedback
//! delay; when a frame is lost anyway (a gap in frame ids), the decode thread
//! skips non-keyframes until an IDR arrives (P-frames referencing a missing
//! frame would decode to corruption) and the network task requests one.
//! LTR/reference invalidation replaces this later (research/08 §5.6).
//!
//! Latency samples use an NTP-style clock offset estimated from Ping/Pong at
//! the lowest observed RTT, so cross-machine numbers are meaningful to within
//! path asymmetry. Same-machine, the offset converges near zero.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use sunna_codec::make_decoder;
use sunna_proto::media::{CompleteFrame, Reassembler};
use sunna_proto::messages::{ControlMessage, HostStats, InputEvent};
use sunna_proto::stats::Percentiles;
use sunna_transport::quinn::Connection;
use sunna_transport::ControlChannel;

#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub host_name: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub codec: String,
}

#[derive(Debug)]
pub struct BenchReport {
    pub info: SessionInfo,
    pub elapsed: Duration,
    pub frames_completed: u64,
    pub frames_dropped: u64,
    pub frames_skipped_awaiting_keyframe: u64,
    pub keyframes_requested: u64,
    pub chunks_recovered: u64,
    pub stale_datagrams: u64,
    pub bytes_received: u64,
    pub e2e_latency: Option<Percentiles>,
    pub rtt: Option<Percentiles>,
    /// Estimated host-minus-client clock offset applied to latency samples.
    pub clock_offset_us: Option<i64>,
}

impl std::fmt::Display for BenchReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let seconds = self.elapsed.as_secs_f64().max(f64::EPSILON);
        writeln!(f, "── sunna session report ─────────────────────────────")?;
        writeln!(
            f,
            "host: {} ({}x{} @ {} fps, codec {})",
            self.info.host_name, self.info.width, self.info.height, self.info.fps, self.info.codec
        )?;
        writeln!(
            f,
            "frames: {} reassembled ({:.1} fps), {} dropped, {} skipped awaiting keyframe",
            self.frames_completed,
            self.frames_completed as f64 / seconds,
            self.frames_dropped,
            self.frames_skipped_awaiting_keyframe,
        )?;
        writeln!(
            f,
            "recovery: {} chunks FEC-recovered, {} keyframes requested, {} stale datagrams",
            self.chunks_recovered, self.keyframes_requested, self.stale_datagrams,
        )?;
        writeln!(
            f,
            "throughput: {:.1} Mbit/s over {:.1}s",
            self.bytes_received as f64 * 8.0 / seconds / 1_000_000.0,
            seconds
        )?;
        match &self.e2e_latency {
            Some(stats) => writeln!(f, "capture→decode latency: {stats}")?,
            None => writeln!(f, "capture→decode latency: no samples")?,
        }
        match &self.rtt {
            Some(stats) => writeln!(f, "control RTT: {stats}")?,
            None => writeln!(f, "control RTT: no samples")?,
        }
        match self.clock_offset_us {
            Some(offset) => writeln!(
                f,
                "clock offset (host - client): {:.2} ms",
                offset as f64 / 1000.0
            )?,
            None => writeln!(f, "clock offset: not estimated")?,
        }
        write!(f, "─────────────────────────────────────────────────────")
    }
}

/// Connection options for [`run_client`].
#[derive(Debug, Clone, Default)]
pub struct ClientOptions {
    /// Share text and images with the host during this session.
    pub clipboard: bool,
    /// Name the host logs for this client.
    pub name: String,
    /// Session token the host expects (empty if the host has none).
    pub token: String,
    /// Largest stream this client can show 1:1, in physical pixels.
    pub max_size: Option<(u32, u32)>,
    /// Disconnect after this long; runs until the connection closes if `None`.
    pub duration: Option<Duration>,
    /// Updated every second with the session's numbers (stats overlay).
    pub live: Option<Arc<std::sync::Mutex<LiveStats>>>,
}

/// The session's latest per-second numbers, for a stats overlay.
#[derive(Debug, Clone, Default)]
pub struct LiveStats {
    pub codec: String,
    pub width: u32,
    pub height: u32,
    /// Frames decoded in the last second.
    pub fps: u32,
    pub mbps: f64,
    /// Capture → decoded, p50 and p95, corrected for clock offset.
    pub latency_ms: Option<(f64, f64)>,
    pub decode_ms_p50: Option<f64>,
    pub rtt_ms: Option<f64>,
    /// Frames lost in the last second.
    pub dropped: u64,
    /// Fast-lane tile batches in the last second.
    pub tile_batches: u32,
    pub host: Option<HostStats>,
}

/// Compressed frames in flight between the network task and the decode
/// thread. Deliberately roomy: every P-frame references the one before, so
/// dropping a compressed frame costs a keyframe round trip and a visible
/// stall. Frames arrive in bursts (Wi-Fi, Tailscale batching) and decode takes
/// ~2-3 ms, so the queue drains quickly; staleness is handled after decode,
/// where the viewer shows only the newest decoded frame. Overflowing this
/// means decode has stalled outright.
const DECODE_QUEUE: usize = 120;

/// Decode thread → network task.
enum DecodeEvent {
    /// Decoded; `age_us` is local-now minus the host capture stamp (the
    /// network task applies the clock offset).
    Decoded {
        frame_id: u64,
        keyframe: bool,
        age_us: i64,
        decode_us: u64,
        bytes: usize,
        assembly_us: u64,
    },
    /// A frame can't be decoded until the next keyframe (gap or decode error).
    NeedKeyframe,
}

#[derive(Default)]
struct DecodeCounters {
    decoded: AtomicU64,
    skipped_awaiting_keyframe: AtomicU64,
    gap_lost: AtomicU64,
    decode_errors: AtomicU64,
}

fn spawn_decoder(
    mut decoder: Box<dyn sunna_codec::Decoder>,
    frames: std::sync::mpsc::Receiver<CompleteFrame>,
    events: tokio::sync::mpsc::UnboundedSender<DecodeEvent>,
    counters: Arc<DecodeCounters>,
    mut on_frame: impl FnMut(sunna_codec::DecodedFrame) + Send + 'static,
) -> std::io::Result<std::thread::JoinHandle<()>> {
    std::thread::Builder::new().name("sunna-decode".into()).spawn(move || {
        let mut awaiting_keyframe = true; // nothing decodable before the first IDR
        let mut last_frame: Option<u64> = None;
        for frame in frames {
            // A gap in frame ids is a frame lost in the network (or dropped
            // here); later P-frames reference it.
            if let Some(last) = last_frame {
                if frame.frame_id > last + 1 {
                    counters.gap_lost.fetch_add(frame.frame_id - last - 1, Ordering::Relaxed);
                    if !frame.keyframe && !awaiting_keyframe {
                        awaiting_keyframe = true;
                        let _ = events.send(DecodeEvent::NeedKeyframe);
                    }
                }
            }
            last_frame = Some(last_frame.map_or(frame.frame_id, |last| last.max(frame.frame_id)));
            if awaiting_keyframe && !frame.keyframe {
                counters.skipped_awaiting_keyframe.fetch_add(1, Ordering::Relaxed);
                continue;
            }
            let started = Instant::now();
            match decoder.decode(frame.frame_id, frame.capture_ts_us, frame.keyframe, &frame.data) {
                Ok(decoded) => {
                    awaiting_keyframe = false;
                    counters.decoded.fetch_add(1, Ordering::Relaxed);
                    let age_us = sunna_proto::now_us() as i64 - decoded.capture_ts_us as i64;
                    let _ = events.send(DecodeEvent::Decoded {
                        frame_id: frame.frame_id,
                        keyframe: frame.keyframe,
                        age_us,
                        decode_us: started.elapsed().as_micros() as u64,
                        bytes: frame.data.len(),
                        assembly_us: frame.assembly_us,
                    });
                    on_frame(decoded);
                }
                Err(error) => {
                    counters.decode_errors.fetch_add(1, Ordering::Relaxed);
                    tracing::debug!(frame_id = frame.frame_id, %error, "decode failed");
                    if !awaiting_keyframe {
                        awaiting_keyframe = true;
                        let _ = events.send(DecodeEvent::NeedKeyframe);
                    }
                }
            }
        }
    })
}

/// Run a receive session until the connection closes or `options.duration`
/// elapses.
///
/// `on_frame` is the render-on-arrival hook: called with every decoded frame,
/// in order, from the decode thread. Keep it cheap (store + wake a renderer).
///
/// `input` carries local input events to forward to the host; drop the sender
/// (or pass a channel that never sends) for view-only sessions.
///
/// `on_tiles` receives fast-lane tile batches (small changed regions sent
/// ahead of the video; see `sunna_proto::tiles`), from a network task.
pub async fn run_client(
    connection: Connection,
    options: ClientOptions,
    on_frame: impl FnMut(sunna_codec::DecodedFrame) + Send + 'static,
    on_tiles: impl FnMut(sunna_proto::tiles::TileBatch) + Send + 'static,
    mut input: tokio::sync::mpsc::UnboundedReceiver<InputEvent>,
) -> anyhow::Result<BenchReport> {
    let mut control = ControlChannel::open(&connection).await?;
    control
        .send(&ControlMessage::Hello {
            version: sunna_proto::PROTOCOL_VERSION,
            name: options.name.clone(),
            token: options.token.clone(),
            max_size: options.max_size,
        })
        .await?;
    let info = match control.recv().await? {
        ControlMessage::HelloAck {
            version,
            name,
            width,
            height,
            fps,
            codec,
        } => {
            anyhow::ensure!(
                version == sunna_proto::PROTOCOL_VERSION,
                "protocol version mismatch: host {version}, client {}",
                sunna_proto::PROTOCOL_VERSION
            );
            SessionInfo {
                host_name: name,
                width,
                height,
                fps,
                codec,
            }
        }
        ControlMessage::Refused { reason } => anyhow::bail!("host refused the session: {reason}"),
        other => anyhow::bail!("expected HelloAck, got {other:?}"),
    };
    tracing::info!(
        host = %info.host_name,
        width = info.width,
        height = info.height,
        fps = info.fps,
        codec = %info.codec,
        requested_max = ?options.max_size,
        "session established"
    );
    let decoder = make_decoder(&info.codec, info.width, info.height)?;

    // The receive half gets its own task: `recv` isn't cancellation-safe, so
    // racing it in the select! below could desync the stream's framing.
    let (mut control, mut control_rx) = control.into_split();
    let (message_tx, mut messages) = tokio::sync::mpsc::channel(4);
    let reader = tokio::spawn(async move {
        while let Ok(message) = control_rx.recv().await {
            if message_tx.send(message).await.is_err() {
                break;
            }
        }
    });

    // Fast-lane tiles arrive on a unidirectional stream the host opens only
    // when enabled; (age, bytes) per batch feed the window stats.
    let (tile_stat_tx, mut tile_stats) = tokio::sync::mpsc::unbounded_channel::<(i64, usize)>();
    let (clipboard_tx, mut clipboard_rx) = tokio::sync::mpsc::channel(2);
    let tile_reader = tokio::spawn(accept_streams(
        connection.clone(),
        on_tiles,
        tile_stat_tx,
        clipboard_tx,
    ));

    let counters = Arc::new(DecodeCounters::default());
    let (frame_tx, frame_rx) = std::sync::mpsc::sync_channel::<CompleteFrame>(DECODE_QUEUE);
    let (event_tx, mut decode_events) = tokio::sync::mpsc::unbounded_channel();
    let decode_thread = spawn_decoder(decoder, frame_rx, event_tx, Arc::clone(&counters), on_frame)?;
    let mut frame_tx = Some(frame_tx);

    let mut reassembler = Reassembler::new();
    let started = Instant::now();
    let mut bytes_received: u64 = 0;
    let mut e2e_samples: Vec<u64> = Vec::new();
    let mut rtt_samples: Vec<u64> = Vec::new();
    let mut ping_seq: u32 = 0;

    // Keyframe requests are re-sent while waiting: the host may legitimately
    // drop an IDR under backlog and force a fresh one only when the path clears.
    let mut awaiting_keyframe_since: Option<Instant> = None;
    let mut last_keyframe_request = Instant::now() - Duration::from_secs(1);
    let mut keyframes_requested: u64 = 0;
    let mut seen_dropped: u64 = 0;
    let mut queue_full_drops: u64 = 0;

    // NTP-style offset (host clock minus client clock) at the lowest RTT seen.
    let mut min_rtt_us: Option<u64> = None;
    let mut clock_offset_us: Option<i64> = None;

    // Per-second progress window, also the receiver-report cadence.
    let mut window_frames: u64 = 0;
    let mut window_bytes: u64 = 0;
    let mut window_samples: Vec<u64> = Vec::new();
    let mut window_decode_us: Vec<u64> = Vec::new();
    let mut window_dropped_base: u64 = 0;
    let mut window_recovered_base: u64 = 0;
    // Network stall diagnostics, independent of clock sync: the longest
    // silence between datagrams, and frames that arrived very late.
    let mut window_tile_batches: u32 = 0;
    let mut window_tile_bytes: usize = 0;
    let mut window_tile_ages: Vec<u64> = Vec::new();
    let mut last_datagram_at: Option<Instant> = None;
    let mut window_max_gap_us: u64 = 0;
    let mut window_gaps_over_50ms: u32 = 0;
    let mut window_slow_logged: u32 = 0;
    let mut window_slow_frames: u32 = 0;

    let mut ping_interval = tokio::time::interval(Duration::from_millis(500));
    let mut report_interval = tokio::time::interval_at(
        tokio::time::Instant::now() + Duration::from_secs(1),
        Duration::from_secs(1),
    );
    let deadline_sleep = tokio::time::sleep(
        options.duration.unwrap_or(Duration::from_secs(60 * 60 * 24 * 365)),
    );
    tokio::pin!(deadline_sleep);
    let mut input_open = true;
    let mut clipboard = sunna_clipboard::ClipboardSession::start(options.clipboard);

    loop {
        tokio::select! {
            data = clipboard.next() => {
                // Its own stream, so a large image never delays input.
                let connection = connection.clone();
                tokio::spawn(async move {
                    let (kind, size) = (data.kind, data.data.len());
                    match sunna_transport::send_clipboard(&connection, &data).await {
                        Ok(()) => tracing::info!(?kind, size, "clipboard sent"),
                        Err(error) => tracing::warn!(%error, "clipboard send failed"),
                    }
                });
            }
            Some(data) = clipboard_rx.recv() => clipboard.receive(data),
            event = input.recv(), if input_open => {
                match event {
                    Some(event) => control.send(&ControlMessage::Input(event)).await?,
                    None => input_open = false,
                }
            }
            datagram = connection.read_datagram() => {
                let Ok(datagram) = datagram else { break };
                let now = Instant::now();
                if let Some(previous) = last_datagram_at.replace(now) {
                    let gap = now.duration_since(previous).as_micros() as u64;
                    window_max_gap_us = window_max_gap_us.max(gap);
                    if gap > 50_000 {
                        window_gaps_over_50ms += 1;
                    }
                }
                bytes_received += datagram.len() as u64;
                window_bytes += datagram.len() as u64;
                let completed = reassembler.push(&datagram);

                // Frame loss (a newer frame superseded a partial): ask for a
                // keyframe now rather than when the gap reaches the decoder.
                let mut need_keyframe = false;
                if reassembler.dropped_frames > seen_dropped {
                    seen_dropped = reassembler.dropped_frames;
                    need_keyframe = true;
                }
                if let (Some(frame), Some(tx)) = (completed, frame_tx.as_ref()) {
                    match tx.try_send(frame) {
                        Ok(()) => {}
                        // Decode has stalled for ~2 s: drop; the id gap makes
                        // the decode thread wait for a keyframe, which we request.
                        Err(std::sync::mpsc::TrySendError::Full(_)) => {
                            queue_full_drops += 1;
                            need_keyframe = true;
                        }
                        Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                            frame_tx = None;
                            tracing::warn!("decode thread exited");
                        }
                    }
                }
                if need_keyframe {
                    awaiting_keyframe_since.get_or_insert_with(Instant::now);
                    if last_keyframe_request.elapsed() > Duration::from_millis(100) {
                        keyframes_requested += 1;
                        last_keyframe_request = Instant::now();
                        control.send(&ControlMessage::RequestKeyframe).await?;
                    }
                }
            }
            event = decode_events.recv() => {
                match event {
                    Some(DecodeEvent::Decoded {
                        frame_id,
                        keyframe,
                        age_us,
                        decode_us,
                        bytes,
                        assembly_us,
                    }) => {
                        if keyframe {
                            if let Some(since) = awaiting_keyframe_since.take() {
                                tracing::debug!(
                                    stalled_ms = since.elapsed().as_millis() as u64,
                                    "recovered on keyframe"
                                );
                            }
                        }
                        let sample = (age_us + clock_offset_us.unwrap_or(0)).max(0) as u64;
                        if sample > 100_000 {
                            window_slow_frames += 1;
                            if window_slow_logged < 10 {
                                window_slow_logged += 1;
                                tracing::debug!(
                                    frame_id,
                                    e2e_ms = sample / 1000,
                                    assembly_ms = assembly_us / 1000,
                                    decode_ms = decode_us / 1000,
                                    bytes,
                                    keyframe,
                                    "slow frame"
                                );
                            }
                        }
                        e2e_samples.push(sample);
                        window_samples.push(sample);
                        window_decode_us.push(decode_us);
                        window_frames += 1;
                    }
                    Some(DecodeEvent::NeedKeyframe) => {
                        awaiting_keyframe_since.get_or_insert_with(Instant::now);
                        if last_keyframe_request.elapsed() > Duration::from_millis(100) {
                            keyframes_requested += 1;
                            last_keyframe_request = Instant::now();
                            control.send(&ControlMessage::RequestKeyframe).await?;
                        }
                    }
                    None => {}
                }
            }
            stat = tile_stats.recv() => {
                if let Some((age_us, bytes)) = stat {
                    window_tile_batches += 1;
                    window_tile_bytes += bytes;
                    window_tile_ages.push((age_us + clock_offset_us.unwrap_or(0)).max(0) as u64);
                }
            }
            message = messages.recv() => {
                match message {
                    Some(ControlMessage::Pong { t_us, peer_t_us, .. }) => {
                        let received = sunna_proto::now_us();
                        let rtt = received.saturating_sub(peer_t_us);
                        rtt_samples.push(rtt);
                        // Best offset estimate is the one at minimum RTT.
                        if min_rtt_us.map_or(true, |min| rtt <= min) {
                            min_rtt_us = Some(rtt);
                            clock_offset_us =
                                Some(t_us as i64 - ((peer_t_us + received) / 2) as i64);
                        }
                    }
                    Some(ControlMessage::HostStats(stats)) => {
                        if let Some(live) = &options.live {
                            live.lock().unwrap().host = Some(stats);
                        }
                    }
                    Some(other) => tracing::debug!(?other, "unexpected control message"),
                    None => break,
                }
            }
            _ = ping_interval.tick() => {
                ping_seq += 1;
                control.send(&ControlMessage::Ping { seq: ping_seq, t_us: sunna_proto::now_us() }).await?;
            }
            _ = report_interval.tick() => {
                if awaiting_keyframe_since.is_some()
                    && last_keyframe_request.elapsed() > Duration::from_millis(500)
                {
                    keyframes_requested += 1;
                    last_keyframe_request = Instant::now();
                    control.send(&ControlMessage::RequestKeyframe).await?;
                }
                // Every frame that never reached the decoder shows up as a gap
                // in frame ids there, whether lost in the network, abandoned
                // by the reassembler, or dropped because decode fell behind.
                let total_dropped = counters.gap_lost.load(Ordering::Relaxed);
                let dropped = total_dropped.saturating_sub(window_dropped_base);
                window_dropped_base = total_dropped;
                let recovered = reassembler.recovered_chunks - window_recovered_base;
                window_recovered_base = reassembler.recovered_chunks;
                let window = Percentiles::from_samples(std::mem::take(&mut window_samples));
                let decode = Percentiles::from_samples(std::mem::take(&mut window_decode_us));
                control.send(&ControlMessage::ReceiverReport {
                    frames_complete: window_frames.min(u32::MAX as u64) as u32,
                    frames_dropped: dropped.min(u32::MAX as u64) as u32,
                    chunks_recovered: recovered.min(u32::MAX as u64) as u32,
                    e2e_p95_us: window.as_ref().map_or(0, |stats| stats.p95_us),
                }).await?;
                if let Some(live) = &options.live {
                    let mut live = live.lock().unwrap();
                    live.codec = info.codec.clone();
                    (live.width, live.height) = (info.width, info.height);
                    live.fps = window_frames as u32;
                    live.mbps = window_bytes as f64 * 8.0 / 1_000_000.0;
                    live.latency_ms = window
                        .as_ref()
                        .map(|w| (w.p50_us as f64 / 1000.0, w.p95_us as f64 / 1000.0));
                    live.decode_ms_p50 = decode.as_ref().map(|d| d.p50_us as f64 / 1000.0);
                    live.rtt_ms = rtt_samples.last().map(|&rtt| rtt as f64 / 1000.0);
                    live.dropped = dropped;
                    live.tile_batches = window_tile_batches;
                }
                tracing::info!(
                    fps = window_frames,
                    dropped,
                    recovered,
                    mbps = format!("{:.1}", window_bytes as f64 * 8.0 / 1_000_000.0),
                    latency = %window.map(|w| w.to_string()).unwrap_or_else(|| "-".into()),
                    decode = %decode.map(|d| d.to_string()).unwrap_or_else(|| "-".into()),
                    rtt_ms = rtt_samples.last().map(|&rtt| rtt as f64 / 1000.0),
                    clock_offset_ms = clock_offset_us.map(|offset| offset as f64 / 1000.0),
                    awaiting_keyframe = awaiting_keyframe_since.is_some(),
                    decoder_behind_drops = queue_full_drops,
                    max_gap_ms = window_max_gap_us / 1000,
                    gaps_over_50ms = window_gaps_over_50ms,
                    slow_frames = window_slow_frames,
                    tile_batches = window_tile_batches,
                    tile_kb = window_tile_bytes / 1024,
                    tile_latency = %Percentiles::from_samples(std::mem::take(&mut window_tile_ages))
                        .map(|w| w.to_string())
                        .unwrap_or_else(|| "-".into()),
                    "window"
                );
                window_tile_batches = 0;
                window_tile_bytes = 0;
                window_frames = 0;
                window_bytes = 0;
                window_max_gap_us = 0;
                window_gaps_over_50ms = 0;
                window_slow_logged = 0;
                window_slow_frames = 0;
            }
            _ = &mut deadline_sleep, if options.duration.is_some() => {
                let _ = control.send(&ControlMessage::Bye).await;
                break;
            }
        }
    }

    reader.abort();
    tile_reader.abort();
    drop(frame_tx);
    let _ = decode_thread.join();
    Ok(BenchReport {
        info,
        elapsed: started.elapsed(),
        frames_completed: reassembler.completed_frames,
        frames_dropped: counters.gap_lost.load(Ordering::Relaxed),
        frames_skipped_awaiting_keyframe: counters.skipped_awaiting_keyframe.load(Ordering::Relaxed),
        keyframes_requested,
        chunks_recovered: reassembler.recovered_chunks,
        stale_datagrams: reassembler.stale_datagrams,
        bytes_received,
        e2e_latency: Percentiles::from_samples(e2e_samples),
        rtt: Percentiles::from_samples(rtt_samples),
        clock_offset_us,
    })
}

/// Read fast-lane tile batches until the stream or connection ends.
/// Unidirectional streams from the host: the fast-lane tile stream (one
/// per session, opened only when enabled) and one stream per clipboard copy.
async fn accept_streams(
    connection: Connection,
    on_tiles: impl FnMut(sunna_proto::tiles::TileBatch) + Send + 'static,
    stats: tokio::sync::mpsc::UnboundedSender<(i64, usize)>,
    clipboard: tokio::sync::mpsc::Sender<sunna_proto::messages::ClipboardData>,
) {
    let mut on_tiles = Some(on_tiles);
    while let Ok(mut stream) = connection.accept_uni().await {
        let mut magic = [0u8; 4];
        if stream.read_exact(&mut magic).await.is_err() {
            continue;
        }
        if magic == sunna_proto::tiles::TILE_STREAM_MAGIC {
            if let Some(on_tiles) = on_tiles.take() {
                tokio::spawn(read_tiles(stream, on_tiles, stats.clone()));
            }
        } else if magic == sunna_transport::CLIPBOARD_STREAM_MAGIC {
            let clipboard = clipboard.clone();
            tokio::spawn(async move {
                match sunna_transport::read_clipboard(stream).await {
                    Ok(data) => {
                        let _ = clipboard.send(data).await;
                    }
                    Err(error) => tracing::debug!(%error, "clipboard stream failed"),
                }
            });
        } else {
            tracing::debug!("ignoring an unknown unidirectional stream");
        }
    }
}

async fn read_tiles(
    mut stream: sunna_transport::quinn::RecvStream,
    mut on_tiles: impl FnMut(sunna_proto::tiles::TileBatch) + Send + 'static,
    stats: tokio::sync::mpsc::UnboundedSender<(i64, usize)>,
) {
    use sunna_proto::tiles;
    tracing::info!("fast lane: receiving tiles");
    loop {
        let mut len = [0u8; 4];
        if stream.read_exact(&mut len).await.is_err() {
            return;
        }
        let len = u32::from_be_bytes(len) as usize;
        if len > tiles::MAX_BATCH_BYTES {
            tracing::warn!(len, "oversized tile batch; closing the fast lane");
            return;
        }
        let mut body = vec![0u8; len];
        if stream.read_exact(&mut body).await.is_err() {
            return;
        }
        match tiles::decode(&body) {
            Ok(batch) => {
                let age_us = sunna_proto::now_us() as i64 - batch.capture_ts_us as i64;
                let _ = stats.send((age_us, len + 4));
                on_tiles(batch);
            }
            Err(error) => tracing::debug!(%error, "bad tile batch"),
        }
    }
}
