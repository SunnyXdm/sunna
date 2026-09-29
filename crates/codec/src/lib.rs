//! Encoder/decoder traits and the passthrough (raw) codec used by Milestone 0a.
//!
//! Real backends (research/03 §2) are per-vendor FFI, called directly with no
//! wrapper layers: NVENC (ultra-low-latency tuning, CBR + 1-frame VBV, no
//! B-frames, infinite GOP + intra-refresh, LTR + reference invalidation),
//! AMD AMF, Intel QuickSync via libvpl, Apple VideoToolbox with
//! `EnableLowLatencyRateControl`. Codec ladder: AV1 > HEVC > H.264.

#[cfg(sunna_ffmpeg)]
pub mod ffmpeg;
pub mod h264;
#[cfg(target_os = "linux")]
pub mod nvenc;
#[cfg(target_os = "linux")]
pub mod openh264_codec;
#[cfg(target_os = "macos")]
pub mod videotoolbox;
#[cfg(target_os = "linux")]
mod yuv;

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
    /// How YUV pixels map to RGB (unused for BGRA).
    pub color: Color,
}

/// YUV → RGB matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Matrix {
    Bt601,
    /// What every Sunna host encodes.
    #[default]
    Bt709,
    Bt2020,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Color {
    pub matrix: Matrix,
    /// Full (0-255) rather than video (16-235) range.
    pub full_range: bool,
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
    /// Whether encoding runs in software on the CPU.
    fn is_software(&self) -> bool {
        false
    }

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
            color: Color::default(),
        })
    }
}

/// The codec this platform should offer by default when hosting.
pub fn default_codec_name() -> &'static str {
    // HEVC on macOS: same encode latency as H.264 on an M1 but ~40% fewer
    // bits for the same picture (test build 9); Apple's own Screen
    // Sharing streams HEVC too.
    if cfg!(target_os = "macos") {
        "hevc"
    } else if cfg!(target_os = "linux") {
        // HEVC needs NVENC; H.264 falls back to software (OpenH264).
        #[cfg(target_os = "linux")]
        if nvenc::supports(Codec::Hevc) {
            return "hevc";
        }
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
        #[cfg(target_os = "linux")]
        "h264" | "hevc" => {
            let selected = if codec == "h264" { Codec::H264 } else { Codec::Hevc };
            // SUNNA_NVENC=0 forces software H.264.
            let use_nvenc = if codec == "hevc" {
                anyhow::ensure!(nvenc::supports(Codec::Hevc), "this GPU and driver can't encode HEVC");
                true
            } else {
                std::env::var("SUNNA_NVENC").as_deref() != Ok("0") && nvenc::supports(Codec::H264)
            };
            if use_nvenc {
                match nvenc::NvencEncoder::new(selected, width, height, fps, bitrate_bps) {
                    Ok(encoder) => {
                        tracing::info!(
                            codec,
                            width,
                            height,
                            fps,
                            bitrate_bps,
                            preset = "P1",
                            tuning = "ultra_low_latency",
                            "using NVENC encoder"
                        );
                        return Ok(Box::new(encoder));
                    }
                    Err(error) if codec == "h264" => {
                        tracing::warn!(
                            reason = %format!("{error:#}"),
                            "NVENC unavailable; falling back to OpenH264"
                        );
                    }
                    Err(error) => return Err(error),
                }
            }
            let encoder = openh264_codec::OpenH264Encoder::new(fps, bitrate_bps)?;
            tracing::info!(codec, width, height, fps, bitrate_bps, "using OpenH264 encoder");
            Ok(Box::new(encoder))
        }
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
        #[cfg(target_os = "linux")]
        "h264" | "hevc" => linux_decoder(codec),
        other => anyhow::bail!("no decoder for codec {other:?} on this platform"),
    }
}

/// FFmpeg (GPU first) when it's there; OpenH264 for H.264 otherwise.
#[cfg(target_os = "linux")]
fn linux_decoder(codec: &str) -> anyhow::Result<Box<dyn Decoder>> {
    #[cfg(sunna_ffmpeg)]
    match ffmpeg::FfmpegDecoder::new(codec) {
        Ok(decoder) => return Ok(Box::new(decoder)),
        Err(error) if codec == "h264" => tracing::warn!(%error, "FFmpeg decoder unavailable; using OpenH264"),
        Err(error) => return Err(error),
    }
    match codec {
        "h264" => Ok(Box::new(openh264_codec::OpenH264Decoder::new()?)),
        other => anyhow::bail!("no {other} decoder here: this viewer was built without FFmpeg"),
    }
}

/// The codec a viewer here should ask for: None leaves it to the host.
/// On Linux: HEVC when the GPU decodes it (sharper for the bits), else
/// H.264, the cheaper of the two to decode in software.
pub fn preferred_decode_codec() -> Option<&'static str> {
    if let Ok(codec) = std::env::var("SUNNA_VIEW_CODEC") {
        return match codec.as_str() {
            "h264" => Some("h264"),
            "hevc" => Some("hevc"),
            _ => None,
        };
    }
    #[cfg(target_os = "linux")]
    {
        #[cfg(sunna_ffmpeg)]
        if ffmpeg::hardware_decodes("hevc") {
            return Some("hevc");
        }
        Some("h264")
    }
    #[cfg(not(target_os = "linux"))]
    None
}

/// How this viewer decodes, for the stats bar: "nvdec", "vaapi", "software".
pub fn decode_backend_hint(codec: &str) -> &'static str {
    #[cfg(sunna_ffmpeg)]
    {
        if ffmpeg::hardware_decodes(codec) {
            return "gpu";
        }
        return "software";
    }
    #[allow(unreachable_code)]
    {
        let _ = codec;
        if cfg!(target_os = "macos") { "gpu" } else { "software" }
    }
}
