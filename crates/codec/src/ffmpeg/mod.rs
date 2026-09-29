//! Decoding with the system's FFmpeg (Linux viewer): NVIDIA's decoder
//! through CUDA, VA-API on Intel and AMD, or FFmpeg's own software decoder,
//! for H.264 and HEVC. The C side is `shim.c`; frames come out as tightly
//! packed NV12 or I420 for the renderer to convert on the GPU.

use std::ffi::{c_char, c_int, CStr};

use anyhow::{bail, Context};
use bytes::Bytes;
use sunna_capture::{FrameData, PixelFormat};

use crate::{Color, DecodedFrame, Decoder, Matrix};

#[repr(C)]
struct RawFrame {
    width: c_int,
    height: c_int,
    layout: c_int,
    planes: [*const u8; 3],
    strides: [c_int; 3],
    full_range: c_int,
    matrix: c_int,
}

#[repr(C)]
struct RawDecoder {
    _private: [u8; 0],
}

extern "C" {
    fn sunna_ff_open(hevc: c_int, hardware: c_int, error: *mut c_char, size: c_int) -> *mut RawDecoder;
    fn sunna_ff_backend(dec: *mut RawDecoder) -> *const c_char;
    fn sunna_ff_hardware_available(hevc: c_int) -> c_int;
    fn sunna_ff_has_decoder(hevc: c_int) -> c_int;
    fn sunna_ff_decode(dec: *mut RawDecoder, data: *const u8, size: c_int, out: *mut RawFrame, error: *mut c_char, size: c_int) -> c_int;
    fn sunna_ff_close(dec: *mut RawDecoder);
}

/// Whether this machine can decode `codec` ("h264", "hevc") on its GPU.
pub fn hardware_decodes(codec: &str) -> bool {
    let hevc = match codec {
        "h264" => 0,
        "hevc" => 1,
        _ => return false,
    };
    // SAFETY: plain function call; it opens and closes a device.
    unsafe { sunna_ff_hardware_available(hevc) != 0 }
}

/// Whether FFmpeg here can decode `codec` at all (hardware or software).
pub fn decodes(codec: &str) -> bool {
    let hevc = match codec {
        "h264" => 0,
        "hevc" => 1,
        _ => return false,
    };
    // SAFETY: plain function call.
    unsafe { sunna_ff_has_decoder(hevc) != 0 }
}

pub struct FfmpegDecoder {
    raw: *mut RawDecoder,
    backend: String,
}

// SAFETY: the decoder is used from one thread at a time (the decode thread).
unsafe impl Send for FfmpegDecoder {}

impl FfmpegDecoder {
    /// `SUNNA_DECODE=software` skips the GPU.
    pub fn new(codec: &str) -> anyhow::Result<Self> {
        let hevc = match codec {
            "h264" => 0,
            "hevc" => 1,
            other => bail!("FFmpeg decoder: unsupported codec {other:?}"),
        };
        let hardware = std::env::var("SUNNA_DECODE").as_deref() != Ok("software");
        let mut error = [0 as c_char; 256];
        // SAFETY: the error buffer is valid for its length.
        let raw = unsafe { sunna_ff_open(hevc, hardware as c_int, error.as_mut_ptr(), error.len() as c_int) };
        if raw.is_null() {
            // SAFETY: the shim NUL-terminates what it writes.
            let text = unsafe { CStr::from_ptr(error.as_ptr()) }.to_string_lossy().into_owned();
            bail!("FFmpeg decoder: {text}");
        }
        // SAFETY: `raw` is valid; the name is a static string.
        let backend = unsafe { CStr::from_ptr(sunna_ff_backend(raw)) }.to_string_lossy().into_owned();
        tracing::info!(codec, backend = %backend, "using FFmpeg decoder");
        Ok(Self { raw, backend })
    }

    /// "nvdec", "vaapi" or "software" (it can fall back once a stream starts).
    pub fn backend(&self) -> String {
        // SAFETY: `raw` is valid while self lives.
        unsafe { CStr::from_ptr(sunna_ff_backend(self.raw)) }.to_string_lossy().into_owned()
    }
}

impl Drop for FfmpegDecoder {
    fn drop(&mut self) {
        // SAFETY: `raw` came from sunna_ff_open and is closed once.
        unsafe { sunna_ff_close(self.raw) }
    }
}

/// Copy `rows` rows of `row_bytes` from a strided plane into `out`.
///
/// # Safety
/// `plane` must be valid for `rows` rows of `stride` bytes.
unsafe fn copy_plane(out: &mut Vec<u8>, plane: *const u8, stride: usize, row_bytes: usize, rows: usize) {
    for row in 0..rows {
        let start = plane.add(row * stride);
        out.extend_from_slice(std::slice::from_raw_parts(start, row_bytes));
    }
}

impl Decoder for FfmpegDecoder {
    fn decode(&mut self, frame_id: u64, capture_ts_us: u64, _keyframe: bool, data: &[u8]) -> anyhow::Result<DecodedFrame> {
        let mut frame = std::mem::MaybeUninit::<RawFrame>::zeroed();
        let mut error = [0 as c_char; 256];
        let size = c_int::try_from(data.len()).context("frame too large")?;
        // SAFETY: all pointers are valid for the call.
        let code = unsafe { sunna_ff_decode(self.raw, data.as_ptr(), size, frame.as_mut_ptr(), error.as_mut_ptr(), error.len() as c_int) };
        if code < 0 {
            // SAFETY: the shim NUL-terminates what it writes.
            let text = unsafe { CStr::from_ptr(error.as_ptr()) }.to_string_lossy().into_owned();
            bail!("FFmpeg ({}): {text}", self.backend);
        }
        if code == 0 {
            bail!("decoder produced no picture yet");
        }
        // SAFETY: code 1 means the shim filled the frame.
        let frame = unsafe { frame.assume_init() };
        let (width, height) = (frame.width as usize, frame.height as usize);
        let (chroma_w, chroma_h) = (width.div_ceil(2), height.div_ceil(2));
        let nv12 = frame.layout == 0;
        let mut bytes = Vec::with_capacity(width * height + chroma_w * chroma_h * 2);
        // SAFETY: FFmpeg's planes hold `height` rows at their strides.
        unsafe {
            copy_plane(&mut bytes, frame.planes[0], frame.strides[0] as usize, width, height);
            if nv12 {
                copy_plane(&mut bytes, frame.planes[1], frame.strides[1] as usize, chroma_w * 2, chroma_h);
            } else {
                copy_plane(&mut bytes, frame.planes[1], frame.strides[1] as usize, chroma_w, chroma_h);
                copy_plane(&mut bytes, frame.planes[2], frame.strides[2] as usize, chroma_w, chroma_h);
            }
        }
        let matrix = match frame.matrix {
            0 => Matrix::Bt601,
            2 => Matrix::Bt2020,
            // BT.709, or unsaid: every Sunna host encodes BT.709.
            _ => Matrix::Bt709,
        };
        Ok(DecodedFrame {
            frame_id,
            width: width as u32,
            height: height as u32,
            format: if nv12 { PixelFormat::Nv12 } else { PixelFormat::I420 },
            data: FrameData::Cpu(Bytes::from(bytes)),
            capture_ts_us,
            color: Color { matrix, full_range: frame.full_range != 0 },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openh264_codec::OpenH264Encoder;
    use crate::Encoder;
    use sunna_capture::VideoFrame;

    /// A flat grey frame and a gradient, encoded with OpenH264, come back
    /// out of FFmpeg as I420/NV12 of the right size and brightness.
    #[test]
    fn decodes_what_openh264_encodes() {
        let (w, h) = (320usize, 180usize);
        let mut encoder = OpenH264Encoder::new(60, 4_000_000).unwrap();
        let mut decoder = FfmpegDecoder::new("h264").unwrap();
        for i in 0..6u64 {
            let mut data = vec![0u8; w * h * 4];
            for (n, px) in data.chunks_exact_mut(4).enumerate() {
                let x = n % w;
                let v = if i % 2 == 0 { 128 } else { (x * 255 / w) as u8 };
                px[..3].fill(v);
                px[3] = 255;
            }
            let frame = VideoFrame {
                frame_id: i,
                width: w as u32,
                height: h as u32,
                format: PixelFormat::Bgra8,
                data: FrameData::Cpu(Bytes::from(data)),
                capture_ts_us: 0,
            };
            let encoded = encoder.encode(&frame).unwrap().expect("a frame");
            let decoded = decoder.decode(i, 0, encoded.keyframe, &encoded.data).unwrap();
            assert_eq!((decoded.width, decoded.height), (w as u32, h as u32));
            assert!(matches!(decoded.format, PixelFormat::I420 | PixelFormat::Nv12));
            let bytes = decoded.data.to_cpu().unwrap();
            assert_eq!(bytes.len(), w * h * 3 / 2);
            let luma = &bytes[..w * h];
            if i % 2 == 0 {
                // Grey 128 in BT.709 video range: Y ≈ 16 + 128 * 219/255 ≈ 126.
                let mean = luma.iter().map(|&v| v as u32).sum::<u32>() / luma.len() as u32;
                assert!((120..=132).contains(&mean), "grey luma {mean}");
            } else {
                // Left dark, right bright.
                let row = &luma[h / 2 * w..h / 2 * w + w];
                assert!(row[8] < 40 && row[w - 8] > 200, "gradient {} .. {}", row[8], row[w - 8]);
            }
            assert_eq!(decoded.color.matrix, Matrix::Bt709);
            assert!(!decoded.color.full_range);
        }
        println!("backend {}", decoder.backend());
    }

    /// NVENC-encoded H.264 and HEVC decode on the GPU (`hevc`/`h264` via
    /// NVDEC). Needs an NVIDIA GPU: `cargo test -p sunna-codec nvdec -- --ignored`.
    #[test]
    #[ignore]
    fn nvdec_round_trip() {
        use crate::nvenc::{supports, NvencEncoder};
        use crate::Codec;
        let (w, h) = (1920usize, 1080usize);
        for codec in [Codec::H264, Codec::Hevc] {
            assert!(supports(codec), "no NVENC for {codec:?}");
            let mut encoder = NvencEncoder::new(codec, w as u32, h as u32, 60, 20_000_000).unwrap();
            let mut decoder = FfmpegDecoder::new(codec.name()).unwrap();
            let mut times = Vec::new();
            for i in 0..30u64 {
                let mut data = vec![0u8; w * h * 4];
                for (n, px) in data.chunks_exact_mut(4).enumerate() {
                    let x = (n % w + i as usize * 16) % w;
                    px[..3].fill((x * 255 / w) as u8);
                    px[3] = 255;
                }
                let frame = VideoFrame { frame_id: i, width: w as u32, height: h as u32, format: PixelFormat::Bgra8, data: FrameData::Cpu(Bytes::from(data)), capture_ts_us: 0 };
                let Some(encoded) = encoder.encode(&frame).unwrap() else { continue };
                let started = std::time::Instant::now();
                let decoded = decoder.decode(i, 0, encoded.keyframe, &encoded.data).unwrap();
                times.push(started.elapsed().as_secs_f64() * 1000.0);
                assert_eq!((decoded.width, decoded.height), (w as u32, h as u32));
            }
            times.sort_by(|a, b| a.partial_cmp(b).unwrap());
            println!("{:?}: backend {}, decode+copy p50 {:.2} ms, max {:.2} ms", codec, decoder.backend(), times[times.len() / 2], times[times.len() - 1]);
            assert_eq!(decoder.backend(), "nvdec");
        }
    }
}
