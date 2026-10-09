use crate::{
    pipewire::{Frame, PipeWire},
    session::{self, Session},
};
use anyhow::{bail, ensure};
use bytes::Bytes;
use std::sync::{atomic::Ordering, Arc};
use std::time::{Duration, Instant};
use sunna_capture::{FrameData, FrameSource, PixelFormat, VideoFrame};

pub struct PortalSource {
    pipewire: PipeWire,
    session: Arc<Session>,
    _cursor: sunna_capture::cursor::ProviderGuard,
    size: (u32, u32),
    fps: u32,
    serial: u64,
    frame_id: u64,
    last_sent: Instant,
}
impl PortalSource {
    pub fn new(width: Option<u32>, height: Option<u32>, fps: u32) -> anyhow::Result<Self> {
        let session = session::acquire()?;
        let pipewire = PipeWire::new(session.open_pipewire()?, session.stream.node, fps)?;
        // The portal's size is for input. Negotiated pixels may be 2x as large.
        let deadline = Instant::now() + Duration::from_secs(15);
        let native = {
            let mut state = pipewire.shared.state.lock().unwrap();
            loop {
                ensure!(!session.closed.load(Ordering::Acquire), session::STOPPED);
                if let Some(error) = &state.error {
                    bail!("{error}");
                }
                if let Some(frame) = &state.frame {
                    break (frame.width, frame.height);
                }
                ensure!(
                    Instant::now() < deadline,
                    "PipeWire did not deliver a screen frame within 15 seconds"
                );
                state = pipewire
                    .shared
                    .changed
                    .wait_timeout(state, Duration::from_millis(100))
                    .unwrap()
                    .0;
            }
        };
        let size = (
            width.unwrap_or(native.0).min(native.0).max(2) & !1,
            height.unwrap_or(native.1).min(native.1).max(2) & !1,
        );
        tracing::info!(
            node = session.stream.node,
            devices = session.devices,
            coordinates = format!("{}x{}", session.stream.size.0, session.stream.size.1),
            screen = format!("{}x{}", native.0, native.1),
            stream = format!("{}x{}", size.0, size.1),
            metadata_cursor = session.metadata_cursor,
            "portal capture (PipeWire)"
        );
        let shared = Arc::downgrade(&pipewire.shared);
        let metadata = session.metadata_cursor;
        let cursor = sunna_capture::cursor::register_provider(Arc::new(move || {
            // Embedded mode must not also display an Xwayland pointer.
            if !metadata {
                return Some(sunna_capture::cursor::CursorImage {
                    id: 0,
                    width: 1,
                    height: 1,
                    hot_x: 0,
                    hot_y: 0,
                    screen_width: native.0,
                    rgba: vec![0; 4],
                });
            }
            shared
                .upgrade()
                .and_then(|s| s.state.lock().unwrap().cursor.clone())
        }));
        Ok(Self {
            pipewire,
            session,
            _cursor: cursor,
            size,
            fps: fps.max(1),
            serial: 0,
            frame_id: 0,
            last_sent: Instant::now() - Duration::from_secs(1),
        })
    }
}
fn scale(frame: &Frame, size: (u32, u32)) -> Bytes {
    if (frame.width, frame.height) == size {
        return frame.bytes.clone();
    }
    let (dw, dh) = (size.0 as usize, size.1 as usize);
    let mut out = vec![0; dw * dh * 4];
    for y in 0..dh {
        let sy = y * frame.height as usize / dh;
        for x in 0..dw {
            let sx = x * frame.width as usize / dw;
            let src = (sy * frame.width as usize + sx) * 4;
            out[(y * dw + x) * 4..(y * dw + x + 1) * 4].copy_from_slice(&frame.bytes[src..src + 4]);
        }
    }
    out.into()
}
impl FrameSource for PortalSource {
    fn next_frame(&mut self) -> anyhow::Result<VideoFrame> {
        let period = Duration::from_secs_f64(1.0 / self.fps as f64);
        let mut state = self.pipewire.shared.state.lock().unwrap();
        let frame = loop {
            ensure!(
                !self.session.closed.load(Ordering::Acquire),
                session::STOPPED
            );
            if let Some(error) = &state.error {
                bail!("{error}");
            }
            let elapsed = self.last_sent.elapsed();
            if let Some(frame) = &state.frame {
                if elapsed >= period
                    && (frame.serial != self.serial || elapsed >= Duration::from_millis(100))
                {
                    break frame.clone();
                }
            }
            let wait = if elapsed < period {
                period - elapsed
            } else {
                Duration::from_millis(100).saturating_sub(elapsed)
            };
            state = self
                .pipewire
                .shared
                .changed
                .wait_timeout(
                    state,
                    wait.max(Duration::from_millis(1))
                        .min(Duration::from_millis(100)),
                )
                .unwrap()
                .0;
        };
        drop(state);
        self.serial = frame.serial;
        self.last_sent = Instant::now();
        let result = VideoFrame {
            frame_id: self.frame_id,
            width: self.size.0,
            height: self.size.1,
            format: PixelFormat::Bgra8,
            data: FrameData::Cpu(scale(&frame, self.size)),
            capture_ts_us: sunna_proto::now_us(),
        };
        self.frame_id += 1;
        Ok(result)
    }
    fn width(&self) -> u32 {
        self.size.0
    }
    fn height(&self) -> u32 {
        self.size.1
    }
    fn fps(&self) -> u32 {
        self.fps
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nearest_neighbour_uses_native_pixels() {
        let frame = Frame {
            bytes: Bytes::from((0u8..32).collect::<Vec<_>>()),
            width: 4,
            height: 2,
            serial: 1,
        };
        assert_eq!(
            scale(&frame, (2, 2)).as_ref(),
            &[0, 1, 2, 3, 8, 9, 10, 11, 16, 17, 18, 19, 24, 25, 26, 27]
        );
    }
}
