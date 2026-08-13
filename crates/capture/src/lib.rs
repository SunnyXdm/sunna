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

/// A captured frame. Milestone 0 carries CPU pixels; the real pipeline will
/// carry GPU surface handles end-to-end (zero-copy rule, research/03 §1-2).
#[derive(Debug, Clone)]
pub struct VideoFrame {
    pub frame_id: u64,
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub data: Bytes,
    /// Wall-clock capture timestamp (`sunna_proto::now_us`).
    pub capture_ts_us: u64,
}

/// Blocking frame producer. The host pipeline runs it on a dedicated thread;
/// implementations must pace themselves (event-driven on the compositor where
/// the platform allows, timer-paced for the synthetic source).
pub trait FrameSource: Send {
    fn next_frame(&mut self) -> anyhow::Result<VideoFrame>;
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn fps(&self) -> u32;
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

        let frame = VideoFrame {
            frame_id: self.frame_id,
            width: self.width,
            height: self.height,
            format: PixelFormat::Bgra8,
            data: Bytes::from(data),
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
        assert_eq!(first.data.len(), 64 * 32 * 4);
        assert!(second.capture_ts_us >= first.capture_ts_us);
    }
}
