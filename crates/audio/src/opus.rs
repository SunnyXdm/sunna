//! Opus encoder and decoder, 48 kHz stereo, 10 ms frames.

use anyhow::{bail, ensure};
use unsafe_libopus as ffi;

use crate::{CHANNELS, FRAME_LEN, FRAME_SAMPLES, SAMPLE_RATE};

/// Largest encoded packet we accept (Opus's own ceiling is 1275 bytes/frame).
pub const MAX_PACKET: usize = 1500;

pub struct Encoder {
    raw: *mut ffi::OpusEncoder,
}

// SAFETY: the encoder state is only used through `&mut self`.
unsafe impl Send for Encoder {}

impl Encoder {
    /// Music-quality stereo at `bitrate` bits/s, in the restricted low-delay
    /// mode (CELT only: 2.5 ms of lookahead instead of ~6.5 ms).
    pub fn new(bitrate: i32) -> anyhow::Result<Self> {
        let mut error = 0;
        // SAFETY: plain constructor; the result is checked below.
        let raw = unsafe {
            ffi::opus_encoder_create(
                SAMPLE_RATE as i32,
                CHANNELS as i32,
                ffi::OPUS_APPLICATION_RESTRICTED_LOWDELAY,
                &mut error,
            )
        };
        ensure!(!raw.is_null() && error == ffi::OPUS_OK, "opus_encoder_create failed: {error}");
        let encoder = Self { raw };
        // SAFETY: `raw` is a live encoder; each request takes one i32.
        unsafe {
            ffi::opus_encoder_ctl!(raw, ffi::OPUS_SET_BITRATE_REQUEST, bitrate);
            ffi::opus_encoder_ctl!(raw, ffi::OPUS_SET_COMPLEXITY_REQUEST, 6);
            ffi::opus_encoder_ctl!(raw, ffi::OPUS_SET_SIGNAL_REQUEST, ffi::OPUS_SIGNAL_MUSIC);
        }
        Ok(encoder)
    }

    /// Encode one 10 ms frame of interleaved stereo.
    pub fn encode(&mut self, pcm: &[i16; FRAME_LEN]) -> anyhow::Result<Vec<u8>> {
        let mut out = vec![0u8; MAX_PACKET];
        // SAFETY: `pcm` holds FRAME_SAMPLES per channel; `out` has MAX_PACKET bytes.
        let len = unsafe {
            ffi::opus_encode(self.raw, pcm.as_ptr(), FRAME_SAMPLES as i32, out.as_mut_ptr(), MAX_PACKET as i32)
        };
        if len < 0 {
            bail!("opus_encode failed: {len}");
        }
        out.truncate(len as usize);
        Ok(out)
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        // SAFETY: created by opus_encoder_create, destroyed once.
        unsafe { ffi::opus_encoder_destroy(self.raw) }
    }
}

pub struct Decoder {
    raw: *mut ffi::OpusDecoder,
}

// SAFETY: the decoder state is only used through `&mut self`.
unsafe impl Send for Decoder {}

impl Decoder {
    pub fn new() -> anyhow::Result<Self> {
        let mut error = 0;
        // SAFETY: plain constructor; the result is checked below.
        let raw = unsafe { ffi::opus_decoder_create(SAMPLE_RATE as i32, CHANNELS as i32, &mut error) };
        ensure!(!raw.is_null() && error == ffi::OPUS_OK, "opus_decoder_create failed: {error}");
        Ok(Self { raw })
    }

    /// Decode one packet into `pcm`; `None` conceals a lost packet instead.
    pub fn decode(&mut self, packet: Option<&[u8]>, pcm: &mut [i16; FRAME_LEN]) -> anyhow::Result<()> {
        let (data, len) = match packet {
            Some(packet) => (packet.as_ptr(), packet.len() as i32),
            None => (std::ptr::null(), 0),
        };
        // SAFETY: `pcm` has room for FRAME_SAMPLES per channel; a null
        // packet asks for packet-loss concealment of that length.
        let samples = unsafe { ffi::opus_decode(self.raw, data, len, pcm.as_mut_ptr(), FRAME_SAMPLES as i32, 0) };
        if samples < 0 {
            bail!("opus_decode failed: {samples}");
        }
        if (samples as usize) < FRAME_SAMPLES {
            pcm[samples as usize * CHANNELS..].fill(0);
        }
        Ok(())
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        // SAFETY: created by opus_decoder_create, destroyed once.
        unsafe { ffi::opus_decoder_destroy(self.raw) }
    }
}
