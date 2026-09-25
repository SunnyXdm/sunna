//! Encoder/decoder traits and the passthrough (raw) codec used by Milestone 0a.
//!
//! Real backends (research/03 §2) are per-vendor FFI, called directly with no
//! wrapper layers: NVENC (ultra-low-latency tuning, CBR + 1-frame VBV, no
//! B-frames, infinite GOP + intra-refresh, LTR + reference invalidation),
//! AMD AMF, Intel QuickSync via libvpl, Apple VideoToolbox with
//! `EnableLowLatencyRateControl`. Codec ladder: AV1 > HEVC > H.264.

pub mod h264;
#[cfg(target_os = "macos")]
pub mod videotoolbox;

use bytes::Bytes;
use sunna_capture::{FrameData, PixelFormat, VideoFrame};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    /// Uncompressed passthrough — Milestone 0a only.
    Raw,
    H264,
    Hevc,
    Av1,
}

impl Codec {
    pub fn name(self) -> &'static str {
        match self {
            Codec::Raw => "raw",
            Codec::H264 => "h264",
            Codec::Hevc => "hevc",
            Codec::Av1 => "av1",
        }
    }
}

#[derive(Debug, Clone)]
pub struct EncodedFrame {
    pub frame_id: u64,
    pub codec: Codec,
    pub keyframe: bool,
    pub data: Bytes,
    pub capture_ts_us: u64,
    /// Stamped by the encoder when encoding finished (`sunna_proto::now_us`).
    pub encode_done_ts_us: u64,
    /// Submit-to-output time inside the encoder, in microseconds.
    pub encode_us: u64,
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
}

#[derive(Debug, Clone)]
pub struct DecodedFrame {
    pub frame_id: u64,
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    /// GPU surface from hardware decoders (display it directly); CPU bytes
    /// from the passthrough codec.
    pub data: FrameData,
    pub capture_ts_us: u64,
}

/// One result per submitted frame, delivered in submission order.
#[derive(Debug)]
pub enum EncoderOutput {
    Frame(EncodedFrame),
    /// The encoder dropped this frame (load/rate control): normal under
    /// pressure; the reference chain continues from the last emitted frame.
    Dropped { frame_id: u64 },
    Failed { frame_id: u64, error: String },
}

pub type EncoderSink = std::sync::mpsc::Sender<EncoderOutput>;

pub trait Encoder: Send {
    /// `Ok(None)` means the encoder dropped this frame (load/rate control) —
    /// a normal event under pressure, not an error. The reference chain is
    /// unbroken: the next emitted frame references the last *emitted* one.
    fn encode(&mut self, frame: &VideoFrame) -> anyhow::Result<Option<EncodedFrame>>;
    /// Queue `frame` and return; its [`EncoderOutput`] arrives on `sink`,
    /// possibly from another thread. Hardware encoders overlap several
    /// frames this way. Default: encode synchronously.
    fn submit(&mut self, frame: &VideoFrame, sink: &EncoderSink) -> anyhow::Result<()> {
        let output = match self.encode(frame) {
            Ok(Some(encoded)) => EncoderOutput::Frame(encoded),
            Ok(None) => EncoderOutput::Dropped { frame_id: frame.frame_id },
            Err(error) => EncoderOutput::Failed {
                frame_id: frame.frame_id,
                error: format!("{error:#}"),
            },
        };
        let _ = sink.send(output);
        Ok(())
    }
    /// Frames submitted whose output hasn't been delivered yet.
    fn in_flight(&self) -> usize {
        0
    }
    /// Congestion-control hook: applies from the *next* frame (research/03 §4).
    fn set_target_bitrate(&mut self, bits_per_second: u32);
    fn request_keyframe(&mut self);
}

pub trait Decoder: Send {
    fn decode(&mut self, frame_id: u64, capture_ts_us: u64, keyframe: bool, data: &[u8])
        -> anyhow::Result<DecodedFrame>;
}

/// No-op codec: "encodes" by carrying raw pixels with a tiny header.
/// Every frame is independent, so every frame is a keyframe.
pub struct Passthrough {
    width: u32,
    height: u32,
    format: PixelFormat,
}

impl Passthrough {
    pub fn new(width: u32, height: u32, format: PixelFormat) -> Self {
        Self {
            width,
            height,
            format,
        }
    }
}

impl Encoder for Passthrough {
    fn encode(&mut self, frame: &VideoFrame) -> anyhow::Result<Option<EncodedFrame>> {
        let started = std::time::Instant::now();
        let data = frame.data.to_cpu()?;
        Ok(Some(EncodedFrame {
            frame_id: frame.frame_id,
            codec: Codec::Raw,
            keyframe: true,
            data,
            capture_ts_us: frame.capture_ts_us,
            encode_done_ts_us: sunna_proto::now_us(),
            encode_us: started.elapsed().as_micros() as u64,
            width: frame.width,
            height: frame.height,
            format: frame.format,
        }))
    }

    fn set_target_bitrate(&mut self, _bits_per_second: u32) {}

    fn request_keyframe(&mut self) {}
}

impl Decoder for Passthrough {
    fn decode(
        &mut self,
        frame_id: u64,
        capture_ts_us: u64,
        _keyframe: bool,
        data: &[u8],
    ) -> anyhow::Result<DecodedFrame> {
        Ok(DecodedFrame {
            frame_id,
            width: self.width,
            height: self.height,
            format: self.format,
            data: FrameData::Cpu(Bytes::copy_from_slice(data)),
            capture_ts_us,
        })
    }
}

/// The codec this platform should offer by default when hosting.
pub fn default_codec_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "h264"
    } else {
        "raw"
    }
}

/// Build an encoder by negotiated codec name.
pub fn make_encoder(
    codec: &str,
    width: u32,
    height: u32,
    fps: u32,
    bitrate_bps: u32,
) -> anyhow::Result<Box<dyn Encoder>> {
    // Unused on platforms with no hardware backend yet.
    let _ = (fps, bitrate_bps);
    match codec {
        "raw" => Ok(Box::new(Passthrough::new(width, height, PixelFormat::Bgra8))),
        #[cfg(target_os = "macos")]
        "h264" => Ok(Box::new(videotoolbox::VtEncoder::new(
            Codec::H264, width, height, fps, bitrate_bps,
        )?)),
        #[cfg(target_os = "macos")]
        "hevc" => Ok(Box::new(videotoolbox::VtEncoder::new(
            Codec::Hevc, width, height, fps, bitrate_bps,
        )?)),
        other => anyhow::bail!("no encoder for codec {other:?} on this platform"),
    }
}

/// Build a decoder by negotiated codec name.
pub fn make_decoder(codec: &str, width: u32, height: u32) -> anyhow::Result<Box<dyn Decoder>> {
    match codec {
        "raw" => Ok(Box::new(Passthrough::new(width, height, PixelFormat::Bgra8))),
        #[cfg(target_os = "macos")]
        "h264" => Ok(Box::new(videotoolbox::VtDecoder::new(Codec::H264))),
        #[cfg(target_os = "macos")]
        "hevc" => Ok(Box::new(videotoolbox::VtDecoder::new(Codec::Hevc))),
        other => anyhow::bail!("no decoder for codec {other:?} on this platform"),
    }
}
