//! Software H.264 via Cisco's OpenH264, for hosts without a supported
//! hardware encoder yet (the headless Linux dev VM has no GPU). The macOS
//! viewer decodes its output with VideoToolbox like any other H.264.

use anyhow::{bail, Context};
use bytes::Bytes;
use openh264::encoder::{
    BitRate, Complexity, Encoder as OhEncoder, EncoderConfig, FrameRate, FrameType,
    IntraFramePeriod, MatrixCoefficients, RateControlMode, TransferCharacteristics, UsageType,
    VuiConfig,
};
use openh264::formats::{YUVSlices, YUVSource};
use openh264::OpenH264API;
use sunna_capture::{FrameData, PixelFormat, VideoFrame};

use crate::{Codec, DecodedFrame, Decoder, EncodedFrame, Encoder};

pub struct OpenH264Encoder {
    encoder: OhEncoder,
    /// I420 planes, reused across frames.
    planes: Vec<u8>,
    fps: u32,
    bitrate_bps: u32,
    force_keyframe: bool,
    warned_bitrate: bool,
}

impl OpenH264Encoder {
    pub fn new(fps: u32, bitrate_bps: u32) -> anyhow::Result<Self> {
        Ok(Self {
            encoder: Self::create(fps, bitrate_bps)?,
            planes: Vec::new(),
            fps,
            bitrate_bps,
            force_keyframe: true,
            warned_bitrate: false,
        })
    }

    fn create(fps: u32, bitrate_bps: u32) -> anyhow::Result<OhEncoder> {
        // OpenH264 only spreads work across threads by slice; size-limited
        // slices are the mode this crate exposes. ~20% faster P-frames and
        // half-price keyframes at 1080p on the dev VM.
        let slice_kb = env_u16("SUNNA_OH264_SLICE_KB").unwrap_or(32) as u32;
        let config = EncoderConfig::new()
            .max_slice_len(slice_kb * 1024)
            .usage_type(UsageType::ScreenContentRealTime)
            .rate_control_mode(RateControlMode::Bitrate)
            .bitrate(BitRate::from_bps(bitrate_bps))
            .max_frame_rate(FrameRate::from_hz(fps.max(1) as f32))
            .complexity(complexity())
            .num_threads(env_u16("SUNNA_OH264_THREADS").unwrap_or(0))
            // Keyframes only when a viewer asks (joins or loses data).
            .intra_frame_period(IntraFramePeriod::from_num_frames(0))
            .skip_frames(true)
            // Same colour tags as the macOS host's sRGB mode; must match
            // `bgra_to_i420` (BT.709 matrix, video range).
            .vui(
                VuiConfig::bt709()
                    .transfer_characteristics(TransferCharacteristics::Srgb)
                    .matrix_coefficients(MatrixCoefficients::Bt709),
            );
        OhEncoder::with_api_config(OpenH264API::from_source(), config)
            .context("creating the OpenH264 encoder")
    }
}

impl Encoder for OpenH264Encoder {
    fn encode(&mut self, frame: &VideoFrame) -> anyhow::Result<Option<EncodedFrame>> {
        anyhow::ensure!(
            frame.format == PixelFormat::Bgra8,
            "OpenH264 path expects BGRA"
        );
        let started = std::time::Instant::now();
        let bytes = frame.data.to_cpu()?;
        let (width, height) = (frame.width as usize, frame.height as usize);
        anyhow::ensure!(
            bytes.len() >= width * height * 4,
            "frame smaller than {width}x{height}"
        );
        let (luma, chroma) = (width * height, (width / 2) * (height / 2));
        self.planes.resize(luma + 2 * chroma, 0);
        let (y, uv) = self.planes.split_at_mut(luma);
        let (u, v) = uv.split_at_mut(chroma);
        bgra_to_i420(&bytes[..width * height * 4], width, height, y, u, v);
        let yuv = YUVSlices::new((y, u, v), (width, height), (width, width / 2, width / 2));
        if std::mem::take(&mut self.force_keyframe) {
            self.encoder.force_intra_frame();
        }
        let stream = self.encoder.encode(&yuv).context("OpenH264 encode")?;
        let keyframe = match stream.frame_type() {
            FrameType::IDR | FrameType::I => true,
            FrameType::P | FrameType::IPMixed => false,
            FrameType::Skip | FrameType::Invalid => return Ok(None),
        };
        let data = stream.to_vec(); // Annex B; SPS/PPS precede IDR frames
        Ok(Some(EncodedFrame {
            frame_id: frame.frame_id,
            codec: Codec::H264,
            keyframe,
            data: Bytes::from(data),
            capture_ts_us: frame.capture_ts_us,
            encode_done_ts_us: sunna_proto::now_us(),
            encode_us: started.elapsed().as_micros() as u64,
            width: frame.width,
            height: frame.height,
            format: frame.format,
        }))
    }

    fn set_target_bitrate(&mut self, bits_per_second: u32) {
        // The crate doesn't expose runtime bitrate changes; recreating the
        // encoder costs a keyframe, so only do it for large changes.
        let (old, new) = (self.bitrate_bps as f64, bits_per_second as f64);
        if (new - old).abs() / old.max(1.0) < 0.3 {
            return;
        }
        match Self::create(self.fps, bits_per_second) {
            Ok(encoder) => {
                self.encoder = encoder;
                self.bitrate_bps = bits_per_second;
                self.force_keyframe = true;
            }
            Err(error) => {
                if !std::mem::replace(&mut self.warned_bitrate, true) {
                    tracing::warn!(%error, "couldn't apply a new OpenH264 bitrate");
                }
            }
        }
    }

    fn request_keyframe(&mut self) {
        self.force_keyframe = true;
    }
}

pub struct OpenH264Decoder {
    decoder: openh264::decoder::Decoder,
}

impl OpenH264Decoder {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            decoder: openh264::decoder::Decoder::new().context("creating the OpenH264 decoder")?,
        })
    }
}

impl Decoder for OpenH264Decoder {
    fn decode(
        &mut self,
        frame_id: u64,
        capture_ts_us: u64,
        _keyframe: bool,
        data: &[u8],
    ) -> anyhow::Result<DecodedFrame> {
        let Some(yuv) = self.decoder.decode(data).context("OpenH264 decode")? else {
            bail!("decoder produced no picture yet");
        };
        let (width, height) = yuv.dimensions();
        let mut pixels = vec![0u8; width * height * 4];
        yuv.write_rgba8(&mut pixels);
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2); // RGBA -> BGRA
        }
        Ok(DecodedFrame {
            frame_id,
            width: width as u32,
            height: height as u32,
            format: PixelFormat::Bgra8,
            data: FrameData::Cpu(Bytes::from(pixels)),
            capture_ts_us,
        })
    }
}

fn env_u16(name: &str) -> Option<u16> {
    std::env::var(name).ok()?.parse().ok()
}

/// `SUNNA_OH264_COMPLEXITY=low|medium|high` (default low: speed matters
/// more than the last few percent of bitrate for software encoding).
fn complexity() -> Complexity {
    match std::env::var("SUNNA_OH264_COMPLEXITY").as_deref() {
        Ok("medium") => Complexity::Medium,
        Ok("high") => Complexity::High,
        _ => Complexity::Low,
    }
}

/// BGRA → I420, BT.709 matrix, video (limited) range, 2×2 averaged chroma.
/// Split across cores by row bands: at 1080p this is a few milliseconds
/// instead of the ~15 ms a per-pixel float conversion takes.
pub fn bgra_to_i420(
    src: &[u8],
    width: usize,
    height: usize,
    y: &mut [u8],
    u: &mut [u8],
    v: &mut [u8],
) {
    assert!(width % 2 == 0 && height % 2 == 0);
    let threads = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .min(8);
    let band_rows = (height / threads).max(2) & !1;
    std::thread::scope(|scope| {
        let bands = src
            .chunks(width * 4 * band_rows)
            .zip(y.chunks_mut(width * band_rows))
            .zip(u.chunks_mut(width / 2 * band_rows / 2))
            .zip(v.chunks_mut(width / 2 * band_rows / 2));
        for (((src, y), u), v) in bands {
            scope.spawn(move || convert_band(src, width, y, u, v));
        }
    });
}

fn convert_band(src: &[u8], width: usize, y: &mut [u8], u: &mut [u8], v: &mut [u8]) {
    // 16.16 fixed point; see BT.709: Kr 0.2126, Kb 0.0722, scaled to 16..235
    // (luma) and 16..240 (chroma).
    const Y: [i32; 3] = [11966, 40254, 4064];
    const U: [i32; 3] = [-6596, -22188, 28784];
    const V: [i32; 3] = [28784, -26145, -2639];
    let luma = |r: i32, g: i32, b: i32| ((Y[0] * r + Y[1] * g + Y[2] * b + 32768) >> 16) + 16;
    let rows = y.len() / width;
    for pair in 0..rows / 2 {
        let top = &src[pair * 2 * width * 4..][..width * 4];
        let bottom = &src[(pair * 2 + 1) * width * 4..][..width * 4];
        for x in 0..width / 2 {
            let mut sum = [0i32; 3];
            for (row, line) in [top, bottom].into_iter().enumerate() {
                for dx in 0..2 {
                    let px = &line[(x * 2 + dx) * 4..][..3];
                    let (b, g, r) = (px[0] as i32, px[1] as i32, px[2] as i32);
                    y[(pair * 2 + row) * width + x * 2 + dx] = luma(r, g, b) as u8;
                    sum[0] += r;
                    sum[1] += g;
                    sum[2] += b;
                }
            }
            // Sums of four pixels: shift by 2 more bits to average.
            let cb = ((U[0] * sum[0] + U[1] * sum[1] + U[2] * sum[2] + (1 << 17)) >> 18) + 128;
            let cr = ((V[0] * sum[0] + V[1] * sum[1] + V[2] * sum[2] + (1 << 17)) >> 18) + 128;
            u[pair * (width / 2) + x] = cb.clamp(16, 240) as u8;
            v[pair * (width / 2) + x] = cr.clamp(16, 240) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn convert(bgra: [u8; 4]) -> (u8, u8, u8) {
        let src: Vec<u8> = bgra.repeat(4);
        let (mut y, mut u, mut v) = ([0u8; 4], [0u8; 1], [0u8; 1]);
        bgra_to_i420(&src, 2, 2, &mut y, &mut u, &mut v);
        (y[0], u[0], v[0])
    }

    #[test]
    fn bt709_video_range() {
        assert_eq!(convert([0, 0, 0, 255]), (16, 128, 128));
        assert_eq!(convert([255, 255, 255, 255]), (235, 128, 128));
        // Pure red, BT.709: Y 63, Cb 102, Cr 240.
        assert_eq!(convert([0, 0, 255, 255]), (63, 102, 240));
    }

    /// `cargo test --release -p sunna-codec openh264 -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn timing_1080p() {
        let (w, h) = (1920usize, 1080usize);
        let mut encoder = OpenH264Encoder::new(60, 25_000_000).unwrap();
        let mut total = (0u128, 0u128);
        // SUNNA_TEST_FRAMES: raw 1920x1080 BGRA frames, e.g. from
        // `ffmpeg -f x11grab ... -pix_fmt bgra -f rawvideo`.
        let recorded = std::env::var("SUNNA_TEST_FRAMES")
            .ok()
            .map(|path| std::fs::read(path).unwrap());
        for i in 0..30u64 {
            let data = match &recorded {
                Some(frames) => frames[(i as usize % (frames.len() / (w * h * 4))) * w * h * 4..]
                    [..w * h * 4]
                    .to_vec(),
                None => {
                    let mut data = vec![40u8; w * h * 4];
                    for (n, px) in data.chunks_exact_mut(4).enumerate() {
                        let (x, y) = (n % w, n / w);
                        px[0] = ((x + i as usize * 8) % 256) as u8;
                        px[1] = (y % 256) as u8;
                    }
                    data
                }
            };
            let frame = VideoFrame {
                frame_id: i,
                width: w as u32,
                height: h as u32,
                format: PixelFormat::Bgra8,
                data: FrameData::Cpu(Bytes::from(data.clone())),
                capture_ts_us: 0,
            };
            let (mut yb, mut ub, mut vb) = (vec![0; w * h], vec![0; w * h / 4], vec![0; w * h / 4]);
            let t = std::time::Instant::now();
            bgra_to_i420(&data, w, h, &mut yb, &mut ub, &mut vb);
            total.0 += t.elapsed().as_micros();
            let t = std::time::Instant::now();
            encoder.encode(&frame).unwrap();
            total.1 += t.elapsed().as_micros();
        }
        println!(
            "convert {:.1} ms, convert+encode {:.1} ms",
            total.0 as f64 / 30000.0,
            total.1 as f64 / 30000.0
        );
        let first = std::time::Instant::now();
        encoder.request_keyframe();
        let frame = encoder.encode(&VideoFrame {
            frame_id: 99,
            width: w as u32,
            height: h as u32,
            format: PixelFormat::Bgra8,
            data: FrameData::Cpu(Bytes::from(vec![90u8; w * h * 4])),
            capture_ts_us: 0,
        });
        println!(
            "keyframe {:.1} ms ok={}",
            first.elapsed().as_secs_f64() * 1000.0,
            frame.is_ok()
        );
    }
}
