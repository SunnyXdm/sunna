//! Screen capture: the `FrameSource` trait and a synthetic test-pattern source.
//!
//! Platform backends (research/03 §1) live in cfg-gated modules and are stubs
//! until Milestones 0b/0c:
//! - Windows: DXGI Desktop Duplication primary, Windows.Graphics.Capture fallback
//! - macOS: ScreenCaptureKit
//! - Linux: KMS/DRM headless-first, PipeWire portal fallback

use std::time::{Duration, Instant};

use bytes::Bytes;

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(target_os = "linux")]
pub mod linux;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// 8-bit BGRA, the common capture output before GPU color conversion.
    Bgra8,
    Nv12,
    P010,
}

impl PixelFormat {
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            PixelFormat::Bgra8 => 4,
            // Planar formats aren't byte-per-pixel; callers of the synthetic
            // path only use Bgra8. Real backends carry GPU surfaces instead.
            PixelFormat::Nv12 | PixelFormat::P010 => 0,
        }
    }
}

/// Colour space the macOS capture delivers and the encoder tags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    /// Capture converted to Display P3 (the Macs' native gamut), tagged
    /// P3 primaries + sRGB transfer: what screen pixels really are, so the
    /// viewer's colour management reproduces the host exactly.
    DisplayP3,
    /// Previous behaviour: native display pixels tagged as Rec. 709 video
    /// (dulls colours on P3 displays). For A/B only.
    Rec709,
}

impl ColorMode {
    /// `SUNNA_COLOR=709` selects the old tagging; default Display P3.
    pub fn from_env() -> Self {
        match std::env::var("SUNNA_COLOR").as_deref() {
            Ok("709") => ColorMode::Rec709,
            _ => ColorMode::DisplayP3,
        }
    }
}

/// Pixel storage for a captured frame. The synthetic source and tests carry
/// CPU bytes; real capture backends carry GPU surfaces so pixels flow from
/// compositor to encoder without touching system memory (zero-copy rule,
/// research/03 §1-2).
#[derive(Debug, Clone)]
pub enum FrameData {
    Cpu(Bytes),
    /// An IOSurface wrapped in a CVPixelBuffer, ready for VideoToolbox.
    #[cfg(target_os = "macos")]
    Surface(macos::SurfaceFrame),
}

impl FrameData {
    /// Materialize as tightly-packed CPU bytes. Cheap for `Cpu` (refcount
    /// bump); a full copy for `Surface` — only the passthrough codec and
    /// tests should need this.
    pub fn to_cpu(&self) -> anyhow::Result<Bytes> {
        match self {
            FrameData::Cpu(bytes) => Ok(bytes.clone()),
            #[cfg(target_os = "macos")]
            FrameData::Surface(surface) => surface.to_bytes(),
        }
    }
}

/// A captured frame. Real backends carry a GPU surface in `data`; use
/// `FrameData::to_cpu` only off the hot path.
#[derive(Debug, Clone)]
pub struct VideoFrame {
    pub frame_id: u64,
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub data: FrameData,
    /// Wall-clock capture timestamp (`sunna_proto::now_us`).
    pub capture_ts_us: u64,
}

/// Blocking frame producer. The host pipeline runs it on a dedicated thread;
/// implementations must pace themselves (event-driven on the compositor where
/// the platform allows, timer-paced for the synthetic source).
/// Receives fast-lane tile batches (see `sunna_proto::tiles`), possibly
/// from a capture thread.
pub type TileSink = std::sync::Arc<dyn Fn(sunna_proto::tiles::TileBatch) + Send + Sync>;

/// Whether the fast lane (lossless tiles for small changes) is enabled:
/// `SUNNA_FAST_LANE=1` on the host.
pub fn fast_lane_enabled() -> bool {
    std::env::var("SUNNA_FAST_LANE").is_ok_and(|value| value == "1")
}

pub trait FrameSource: Send {
    fn next_frame(&mut self) -> anyhow::Result<VideoFrame>;
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn fps(&self) -> u32;
    /// Start delivering fast-lane tiles for small changes to `sink`.
    /// Sources that can't report changed regions ignore this.
    fn set_tile_sink(&mut self, _sink: TileSink) {}
}

/// Timer-paced synthetic test pattern (moving vertical bar over a gradient).
/// Exists so the full pipeline runs end-to-end before any real capture code.
pub struct SyntheticSource {
    width: u32,
    height: u32,
    fps: u32,
    frame_id: u64,
    period: Duration,
    next_deadline: Instant,
    tiles: Option<TileSink>,
}

impl SyntheticSource {
    pub fn new(width: u32, height: u32, fps: u32) -> Self {
        let fps = fps.max(1);
        Self {
            width,
            height,
            fps,
            frame_id: 0,
            period: Duration::from_secs_f64(1.0 / fps as f64),
            next_deadline: Instant::now(),
            tiles: None,
        }
    }
}

impl FrameSource for SyntheticSource {
    fn next_frame(&mut self) -> anyhow::Result<VideoFrame> {
        let now = Instant::now();
        if self.next_deadline > now {
            std::thread::sleep(self.next_deadline - now);
        } else if now - self.next_deadline > self.period * 4 {
            // Fell badly behind (debugger, suspend): resync instead of bursting.
            self.next_deadline = now;
        }
        self.next_deadline += self.period;

        let capture_ts_us = sunna_proto::now_us();
        let (width, height) = (self.width as usize, self.height as usize);
        let bar = (self.frame_id as usize * 4) % width;
        let mut data = vec![0u8; width * height * 4];
        for y in 0..height {
            let row = y * width * 4;
            let shade = (y * 255 / height.max(1)) as u8;
            for x in 0..width {
                let offset = row + x * 4;
                if x.abs_diff(bar) < 4 {
                    data[offset] = 255;
                    data[offset + 1] = 255;
                    data[offset + 2] = 255;
                } else {
                    data[offset] = shade;
                    data[offset + 1] = 64;
                    data[offset + 2] = (x * 255 / width.max(1)) as u8;
                }
                data[offset + 3] = 255;
            }
        }

        // Fast lane test path: the moving bar's column as a lossless tile, so
        // the tile stream can be exercised without a real display.
        if let Some(sink) = &self.tiles {
            let tile_w = 8.min(width - bar);
            let mut pixels = Vec::with_capacity(tile_w * height * 4);
            for y in 0..height {
                let row = y * width * 4 + bar * 4;
                pixels.extend_from_slice(&data[row..row + tile_w * 4]);
            }
            if let Ok(qoi) = qoi::encode_to_vec(&pixels, tile_w as u32, height as u32) {
                sink(sunna_proto::tiles::TileBatch {
                    capture_ts_us,
                    stream_width: self.width,
                    stream_height: self.height,
                    tiles: vec![sunna_proto::tiles::Tile {
                        x: bar as u32,
                        y: 0,
                        width: tile_w as u32,
                        height: height as u32,
                        qoi,
                    }],
                });
            }
        }

        let frame = VideoFrame {
            frame_id: self.frame_id,
            width: self.width,
            height: self.height,
            format: PixelFormat::Bgra8,
            data: FrameData::Cpu(Bytes::from(data)),
            capture_ts_us,
        };
        self.frame_id += 1;
        Ok(frame)
    }

    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn fps(&self) -> u32 {
        self.fps
    }

    fn set_tile_sink(&mut self, sink: TileSink) {
        self.tiles = Some(sink);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_source_produces_paced_frames() {
        let mut source = SyntheticSource::new(64, 32, 240);
        let first = source.next_frame().unwrap();
        let second = source.next_frame().unwrap();
        assert_eq!(first.frame_id, 0);
        assert_eq!(second.frame_id, 1);
        assert_eq!(first.data.to_cpu().unwrap().len(), 64 * 32 * 4);
        assert!(second.capture_ts_us >= first.capture_ts_us);
    }
}
