//! The last-session picture the Computers screen shows. Taken once, when the
//! session ends: nothing is read back or written while it runs, and keeping
//! the newest frame costs only a reference count.
//!
//! `SUNNA_PICTURE_MODE=sharp` keeps a readable picture (the app's default).
//! Any other mode keeps only a blur: the screen averaged down to at most 32
//! pixels across, so its colours survive and nothing on it can be read.
//! Without `SUNNA_PICTURE_PATH` nothing is kept at all.
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use sunna_capture::PixelFormat;
use sunna_codec::{DecodedFrame, Matrix};

type Latest = Arc<Mutex<Option<DecodedFrame>>>;
/// The longest side of a sharp picture, and of what a blur is averaged from.
const SHARP: usize = 1280;
const BLUR_FROM: usize = 256;
/// Each blurred pixel is the average of an 8 × 8 block.
const BLOCK: usize = 8;

pub(crate) struct Recorder {
    latest: Option<Latest>,
    path: Option<PathBuf>,
    sharp: bool,
}
impl Recorder {
    pub fn from_env() -> Self {
        let path = std::env::var_os("SUNNA_PICTURE_PATH").map(PathBuf::from);
        Self {
            latest: path.as_ref().map(|_| Latest::default()),
            sharp: std::env::var("SUNNA_PICTURE_MODE").is_ok_and(|mode| mode == "sharp"),
            path,
        }
    }
    pub fn frames(&self) -> Option<Latest> {
        self.latest.clone()
    }
    /// The session is over: keep its last frame.
    pub fn finish(self) {
        let (Some(path), Some(latest)) = (self.path, self.latest) else {
            return;
        };
        let frame = latest.lock().unwrap().take();
        if let Some(frame) = frame {
            if let Err(error) = save(&path, &frame, self.sharp) {
                tracing::warn!("Couldn't save the last screen picture: {error}");
            }
        }
    }
}

/// Average `BLOCK` × `BLOCK` blocks: what's left has no detail to read.
fn soften(pixels: &[u8], w: usize, h: usize) -> (Vec<u8>, u16, u16) {
    let (ow, oh) = (w.div_ceil(BLOCK), h.div_ceil(BLOCK));
    let mut out = Vec::with_capacity(ow * oh * 3);
    for by in 0..oh {
        for bx in 0..ow {
            let (mut sum, mut count) = ([0u32; 3], 0u32);
            for y in by * BLOCK..((by + 1) * BLOCK).min(h) {
                for x in bx * BLOCK..((bx + 1) * BLOCK).min(w) {
                    for (channel, total) in sum.iter_mut().enumerate() {
                        *total += pixels[(y * w + x) * 3 + channel] as u32;
                    }
                    count += 1;
                }
            }
            out.extend(sum.map(|total| (total / count.max(1)) as u8));
        }
    }
    (out, ow as u16, oh as u16)
}

fn rgb(frame: &DecodedFrame, limit: usize) -> anyhow::Result<(Vec<u8>, u16, u16)> {
    let (w, h) = (frame.width as usize, frame.height as usize);
    anyhow::ensure!(
        w > 0 && h > 0 && w <= 32768 && h <= 32768,
        "invalid picture size"
    );
    let bytes = frame.data.to_cpu()?; // BGRA IOSurface on macOS, packed planes on Linux.
    let cw = w.div_ceil(2);
    let ch = h.div_ceil(2);
    let minimum = match frame.format {
        PixelFormat::Bgra8 => w * h * 4,
        PixelFormat::I420 | PixelFormat::Nv12 => w * h + 2 * cw * ch,
        PixelFormat::P010 => (w * h + 2 * cw * ch) * 2,
    };
    anyhow::ensure!(bytes.len() >= minimum, "short picture frame");
    let longest = w.max(h).max(limit);
    let ow = (w * limit / longest).max(1);
    let oh = (h * limit / longest).max(1);
    let mut out = Vec::with_capacity(ow * oh * 3);
    let (kr, kb) = match frame.color.matrix {
        Matrix::Bt601 => (0.299, 0.114),
        Matrix::Bt709 => (0.2126, 0.0722),
        Matrix::Bt2020 => (0.2627, 0.0593),
    };
    for y in 0..oh {
        for x in 0..ow {
            let (sx, sy) = (x * w / ow, y * h / oh);
            let i = sy * w + sx;
            if frame.format == PixelFormat::Bgra8 {
                out.extend_from_slice(&[bytes[i * 4 + 2], bytes[i * 4 + 1], bytes[i * 4]]);
                continue;
            }
            let uv = (sy / 2) * cw + sx / 2;
            let (y, u, v) = match frame.format {
                PixelFormat::I420 => (
                    bytes[i] as f32,
                    bytes[w * h + uv] as f32,
                    bytes[w * h + cw * ch + uv] as f32,
                ),
                PixelFormat::Nv12 => (
                    bytes[i] as f32,
                    bytes[w * h + uv * 2] as f32,
                    bytes[w * h + uv * 2 + 1] as f32,
                ),
                PixelFormat::P010 => {
                    let sample = |index| {
                        u16::from_le_bytes([bytes[index * 2], bytes[index * 2 + 1]]) as f32 / 256.0
                    };
                    (
                        sample(i),
                        sample(w * h + uv * 2),
                        sample(w * h + uv * 2 + 1),
                    )
                }
                PixelFormat::Bgra8 => unreachable!(),
            };
            let (y, u, v) = if frame.color.full_range {
                (y, u - 128.0, v - 128.0)
            } else {
                (
                    (y - 16.0) * 255.0 / 219.0,
                    (u - 128.0) * 255.0 / 224.0,
                    (v - 128.0) * 255.0 / 224.0,
                )
            };
            let r = y + 2.0 * (1.0 - kr) * v;
            let b = y + 2.0 * (1.0 - kb) * u;
            let g = (y - kr * r - kb * b) / (1.0 - kr - kb);
            out.extend([r, g, b].map(|c| c.round().clamp(0.0, 255.0) as u8));
        }
    }
    Ok((out, ow as u16, oh as u16))
}

fn save(path: &Path, frame: &DecodedFrame, sharp: bool) -> anyhow::Result<()> {
    use std::fs::{self, OpenOptions};
    let (pixels, width, height) = if sharp {
        rgb(frame, SHARP)?
    } else {
        let (pixels, width, height) = rgb(frame, BLUR_FROM)?;
        soften(&pixels, width as usize, height as usize)
    };
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("missing picture folder"))?;
    fs::create_dir_all(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    }
    let temporary = path.with_extension("jpg.tmp");
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(&temporary)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    jpeg_encoder::Encoder::new(&file, 82).encode(
        &pixels,
        width,
        height,
        jpeg_encoder::ColorType::Rgb,
    )?;
    file.sync_all()?;
    fs::rename(temporary, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sunna_capture::FrameData;
    use sunna_codec::Color;
    fn frame(format: PixelFormat, bytes: Vec<u8>) -> DecodedFrame {
        DecodedFrame {
            frame_id: 0,
            width: 2,
            height: 2,
            format,
            data: FrameData::Cpu(bytes.into()),
            capture_ts_us: 0,
            color: Color::default(),
        }
    }
    #[test]
    fn converts_bgra_and_video_range_planes() {
        let (rgb, _, _) =
            rgb(&frame(PixelFormat::Bgra8, [0, 0, 255, 255].repeat(4)), SHARP).unwrap();
        assert_eq!(rgb, [255, 0, 0].repeat(4));
        for format in [PixelFormat::I420, PixelFormat::Nv12] {
            assert_eq!(
                super::rgb(&frame(format, vec![16, 235, 16, 235, 128, 128]), SHARP)
                    .unwrap()
                    .0,
                vec![0, 0, 0, 255, 255, 255, 0, 0, 0, 255, 255, 255]
            );
        }
    }
    #[test]
    fn a_blur_keeps_no_detail() {
        // A 1920 × 1080 checkerboard of single pixels: sharp text at its worst.
        let (w, h) = (1920usize, 1080usize);
        let mut bytes = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            for x in 0..w {
                let v = if (x + y) % 2 == 0 { 255 } else { 0 };
                bytes.extend([v, v, v, 255]);
            }
        }
        let mut big = frame(PixelFormat::Bgra8, bytes);
        (big.width, big.height) = (w as u32, h as u32);
        let (pixels, width, height) = rgb(&big, BLUR_FROM).unwrap();
        let (soft, sw, sh) = soften(&pixels, width as usize, height as usize);
        assert!(sw <= 32 && sh <= 32, "{sw} x {sh}");
        assert_eq!(soft.len(), sw as usize * sh as usize * 3);
    }
    #[test]
    fn rejects_short_frames_and_writes_private_jpeg() {
        assert!(rgb(&frame(PixelFormat::Bgra8, vec![]), SHARP).is_err());
        let dir = std::env::temp_dir().join(format!("sunna-picture-test-{}", std::process::id()));
        let path = dir.join("test.jpg");
        save(
            &path,
            &frame(PixelFormat::Bgra8, [0, 0, 255, 255].repeat(4)),
            true,
        )
        .unwrap();
        assert_eq!(&std::fs::read(&path).unwrap()[..2], &[255, 216]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
