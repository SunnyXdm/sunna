//! Hardware VideoToolbox encode→decode roundtrip (macOS only).
//! Runs headlessly — VT needs no screen-recording permission.

#![cfg(target_os = "macos")]

use bytes::Bytes;
use sunna_capture::{PixelFormat, VideoFrame};
use sunna_codec::videotoolbox::{VtDecoder, VtEncoder};
use sunna_codec::{h264, Decoder, Encoder};

fn test_frame(frame_id: u64, width: u32, height: u32) -> VideoFrame {
    let (w, h) = (width as usize, height as usize);
    let mut data = vec![0u8; w * h * 4];
    let bar = (frame_id as usize * 8) % w;
    for y in 0..h {
        for x in 0..w {
            let offset = (y * w + x) * 4;
            data[offset] = if x.abs_diff(bar) < 4 { 255 } else { (y % 256) as u8 };
            data[offset + 1] = (x % 256) as u8;
            data[offset + 2] = 128;
            data[offset + 3] = 255;
        }
    }
    VideoFrame {
        frame_id,
        width,
        height,
        format: PixelFormat::Bgra8,
        data: Bytes::from(data),
        capture_ts_us: sunna_proto::now_us(),
    }
}

#[test]
fn hardware_h264_roundtrip() {
    let (width, height, fps) = (320u32, 240u32, 30u32);
    let mut encoder =
        VtEncoder::new(width, height, fps, 2_000_000).expect("create hardware encoder");
    let mut decoder = VtDecoder::new();

    let mut encode_total_us = 0u64;
    let mut decode_total_us = 0u64;
    let frames = 30u64;
    let mut compressed_total = 0usize;

    for frame_id in 0..frames {
        let frame = test_frame(frame_id, width, height);

        let encode_start = std::time::Instant::now();
        let encoded = encoder.encode(&frame).expect("encode");
        encode_total_us += encode_start.elapsed().as_micros() as u64;

        assert_eq!(encoded.frame_id, frame_id);
        assert!(!encoded.data.is_empty());
        compressed_total += encoded.data.len();
        if frame_id == 0 {
            assert!(encoded.keyframe, "first frame must be a keyframe");
            let parsed = h264::annexb_to_avcc(&encoded.data);
            assert!(!parsed.sps.is_empty(), "keyframe must carry SPS");
            assert!(!parsed.pps.is_empty(), "keyframe must carry PPS");
        }

        let decode_start = std::time::Instant::now();
        let decoded = decoder
            .decode(encoded.frame_id, encoded.capture_ts_us, encoded.keyframe, &encoded.data)
            .expect("decode");
        decode_total_us += decode_start.elapsed().as_micros() as u64;

        assert_eq!(decoded.width, width);
        assert_eq!(decoded.height, height);
        assert_eq!(decoded.data.len(), (width * height * 4) as usize);
    }

    let raw_total = frames as usize * (width * height * 4) as usize;
    println!(
        "encode avg {:.2} ms, decode avg {:.2} ms, compression {:.0}x ({} -> {} bytes)",
        encode_total_us as f64 / frames as f64 / 1000.0,
        decode_total_us as f64 / frames as f64 / 1000.0,
        raw_total as f64 / compressed_total as f64,
        raw_total,
        compressed_total,
    );
}

#[test]
fn decoder_waits_for_keyframe() {
    let mut decoder = VtDecoder::new();
    // A P-frame-ish NAL with no SPS/PPS must fail gracefully, not crash.
    let mut annexb = Vec::new();
    h264::push_annexb_nal(&mut annexb, &[0x41, 0x9a, 0x00, 0x01]);
    assert!(decoder.decode(0, 0, false, &annexb).is_err());
}
