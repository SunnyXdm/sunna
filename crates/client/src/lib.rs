//! Client pipeline: datagrams → reassemble (latest-frame-wins, XOR-FEC
//! recovery) → keyframe-gated decode → stats. Headless for Milestone 0 — the
//! render surface (winit + wgpu, present-on-arrival, VRR-aware) arrives with
//! real capture.
//!
//! Recovery model: FEC repairs single losses per parity group with no feedback
//! delay; when a frame is lost anyway, the client requests a keyframe and skips
//! non-keyframes until it arrives (P-frames referencing a missing frame would
//! decode to corruption). LTR/reference invalidation replaces this in M1+.
//!
//! Latency samples use an NTP-style clock offset estimated from Ping/Pong at
//! the lowest observed RTT, so cross-machine numbers are meaningful to within
//! path asymmetry. Same-machine, the offset converges near zero.

use std::time::Duration;

use sunna_codec::make_decoder;
use sunna_proto::media::Reassembler;
use sunna_proto::messages::{ControlMessage, InputEvent};
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

/// Run a receive session until the connection closes or `duration` elapses.
///
/// `on_frame` is the render-on-arrival hook: called with every decoded frame,
/// in arrival order, from the network task. Keep it cheap (store + wake a
/// renderer); heavy work here delays the receive loop.
///
/// `input` carries local input events to forward to the host; drop the sender
/// (or pass a channel that never sends) for view-only sessions.
pub async fn run_client(
    connection: Connection,
    client_name: &str,
    duration: Option<Duration>,
    mut on_frame: impl FnMut(sunna_codec::DecodedFrame) + Send,
    mut input: tokio::sync::mpsc::UnboundedReceiver<InputEvent>,
) -> anyhow::Result<BenchReport> {
    let mut control = ControlChannel::open(&connection).await?;
    control
        .send(&ControlMessage::Hello {
            version: sunna_proto::PROTOCOL_VERSION,
            name: client_name.to_string(),
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
        other => anyhow::bail!("expected HelloAck, got {other:?}"),
    };
    tracing::info!(
        host = %info.host_name,
        width = info.width,
        height = info.height,
        fps = info.fps,
        codec = %info.codec,
        "session established"
    );
    let mut decoder = make_decoder(&info.codec, info.width, info.height)?;

    let mut reassembler = Reassembler::new();
    let started = std::time::Instant::now();
    let mut bytes_received: u64 = 0;
    let mut e2e_samples: Vec<u64> = Vec::new();
    let mut rtt_samples: Vec<u64> = Vec::new();
    let mut ping_seq: u32 = 0;

    // Keyframe-gated recovery state.
    let mut awaiting_keyframe = false;
    let mut keyframes_requested: u64 = 0;
    let mut frames_skipped: u64 = 0;
    let mut seen_dropped: u64 = 0;
    // Continuity tracking: a frame whose every datagram was lost never appears
    // in the reassembler at all — only a gap in decoded frame ids reveals it.
    let mut last_decoded: Option<u64> = None;
    let mut gap_lost: u64 = 0;

    // NTP-style offset (host clock minus client clock) at the lowest RTT seen.
    let mut min_rtt_us: Option<u64> = None;
    let mut clock_offset_us: Option<i64> = None;

    // Per-second progress window, also the receiver-report cadence.
    let mut window_frames: u64 = 0;
    let mut window_bytes: u64 = 0;
    let mut window_samples: Vec<u64> = Vec::new();
    let mut window_dropped_base: u64 = 0;
    let mut window_recovered_base: u64 = 0;

    let mut ping_interval = tokio::time::interval(Duration::from_millis(500));
    let mut report_interval = tokio::time::interval_at(
        tokio::time::Instant::now() + Duration::from_secs(1),
        Duration::from_secs(1),
    );
    let deadline_sleep =
        tokio::time::sleep(duration.unwrap_or(Duration::from_secs(60 * 60 * 24 * 365)));
    tokio::pin!(deadline_sleep);
    let mut input_open = true;

    loop {
        tokio::select! {
            event = input.recv(), if input_open => {
                match event {
                    Some(event) => control.send(&ControlMessage::Input(event)).await?,
                    None => input_open = false,
                }
            }
            datagram = connection.read_datagram() => {
                let Ok(datagram) = datagram else { break };
                bytes_received += datagram.len() as u64;
                window_bytes += datagram.len() as u64;
                let completed = reassembler.push(&datagram);

                // Frame loss (a newer frame superseded a partial): the next
                // decodable frame must be an IDR.
                if reassembler.dropped_frames > seen_dropped {
                    seen_dropped = reassembler.dropped_frames;
                    if !awaiting_keyframe {
                        awaiting_keyframe = true;
                        keyframes_requested += 1;
                        control.send(&ControlMessage::RequestKeyframe).await?;
                    }
                }

                if let Some(frame) = completed {
                    if !awaiting_keyframe {
                        if let Some(last) = last_decoded {
                            if frame.frame_id > last + 1 {
                                gap_lost += frame.frame_id - last - 1;
                                // A keyframe resets references, so a gap before
                                // one is harmless; before a P-frame it is not.
                                if !frame.keyframe {
                                    awaiting_keyframe = true;
                                    keyframes_requested += 1;
                                    control.send(&ControlMessage::RequestKeyframe).await?;
                                }
                            }
                        }
                    }
                    if awaiting_keyframe && !frame.keyframe {
                        frames_skipped += 1;
                        continue;
                    }
                    match decoder.decode(
                        frame.frame_id,
                        frame.capture_ts_us,
                        frame.keyframe,
                        &frame.data,
                    ) {
                        Ok(decoded) => {
                            awaiting_keyframe = false;
                            last_decoded = Some(decoded.frame_id);
                            let now = sunna_proto::now_us() as i64;
                            let capture = decoded.capture_ts_us as i64;
                            let sample =
                                (now - capture + clock_offset_us.unwrap_or(0)).max(0) as u64;
                            e2e_samples.push(sample);
                            window_samples.push(sample);
                            window_frames += 1;
                            on_frame(decoded);
                        }
                        Err(error) => {
                            tracing::debug!(frame_id = frame.frame_id, %error, "decode failed");
                            if !awaiting_keyframe {
                                awaiting_keyframe = true;
                                keyframes_requested += 1;
                                control.send(&ControlMessage::RequestKeyframe).await?;
                            }
                        }
                    }
                }
            }
            message = control.recv() => {
                match message {
                    Ok(ControlMessage::Pong { t_us, peer_t_us, .. }) => {
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
                    Ok(other) => tracing::debug!(?other, "unexpected control message"),
                    Err(_) => break,
                }
            }
            _ = ping_interval.tick() => {
                ping_seq += 1;
                control.send(&ControlMessage::Ping { seq: ping_seq, t_us: sunna_proto::now_us() }).await?;
            }
            _ = report_interval.tick() => {
                let total_dropped = reassembler.dropped_frames + gap_lost;
                let dropped = total_dropped - window_dropped_base;
                window_dropped_base = total_dropped;
                let recovered = reassembler.recovered_chunks - window_recovered_base;
                window_recovered_base = reassembler.recovered_chunks;
                let window = Percentiles::from_samples(std::mem::take(&mut window_samples));
                control.send(&ControlMessage::ReceiverReport {
                    frames_complete: window_frames.min(u32::MAX as u64) as u32,
                    frames_dropped: dropped.min(u32::MAX as u64) as u32,
                    chunks_recovered: recovered.min(u32::MAX as u64) as u32,
                    e2e_p95_us: window.as_ref().map_or(0, |stats| stats.p95_us),
                }).await?;
                tracing::info!(
                    fps = window_frames,
                    dropped,
                    recovered,
                    mbps = format!("{:.1}", window_bytes as f64 * 8.0 / 1_000_000.0),
                    latency = %window.map(|w| w.to_string()).unwrap_or_else(|| "-".into()),
                    rtt_ms = rtt_samples.last().map(|&rtt| rtt as f64 / 1000.0),
                    "window"
                );
                window_frames = 0;
                window_bytes = 0;
            }
            _ = &mut deadline_sleep, if duration.is_some() => {
                let _ = control.send(&ControlMessage::Bye).await;
                break;
            }
        }
    }

    Ok(BenchReport {
        info,
        elapsed: started.elapsed(),
        frames_completed: reassembler.completed_frames,
        frames_dropped: reassembler.dropped_frames + gap_lost,
        frames_skipped_awaiting_keyframe: frames_skipped,
        keyframes_requested,
        chunks_recovered: reassembler.recovered_chunks,
        stale_datagrams: reassembler.stale_datagrams,
        bytes_received,
        e2e_latency: Percentiles::from_samples(e2e_samples),
        rtt: Percentiles::from_samples(rtt_samples),
        clock_offset_us,
    })
}
