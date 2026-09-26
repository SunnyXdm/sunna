//! Fast-lane tiles: small changed screen regions sent losslessly, ahead of
//! the video frame that will also contain them (research/08 §6: exact tiles).
//!
//! Typing or a cursor blink changes a few thousand pixels, but a hardware
//! encoder re-encodes the whole screen (~3.7 ms per megapixel on an M1,
//! regardless of how much changed). Tiles skip that: the host copies the
//! dirty rectangles, compresses them (QOI, lossless), and sends them on their
//! own reliable stream; the viewer draws them over the last video frame until
//! a video frame captured at the same time or later arrives.
//!
//! Stream framing: each batch is a 4-byte big-endian length, then the
//! postcard-encoded [`TileBatch`].

use serde::{Deserialize, Serialize};

/// First bytes on a tile stream, identifying its purpose.
pub const TILE_STREAM_MAGIC: [u8; 4] = *b"STL1";

/// Largest batch accepted on the wire.
pub const MAX_BATCH_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TileBatch {
    /// Capture timestamp of the frame these regions came from (same clock
    /// and meaning as the video stream's capture timestamps).
    pub capture_ts_us: u64,
    /// Stream (encoded video) dimensions the rectangles refer to.
    pub stream_width: u32,
    pub stream_height: u32,
    pub tiles: Vec<Tile>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tile {
    /// Top-left origin, in stream pixels.
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    /// QOI-compressed 4-channel pixels, in the capture's byte order (BGRA on
    /// macOS; QOI round-trips channel bytes exactly regardless of order).
    pub qoi: Vec<u8>,
}

pub fn encode(batch: &TileBatch) -> Result<Vec<u8>, postcard::Error> {
    postcard::to_stdvec(batch)
}

pub fn decode(bytes: &[u8]) -> Result<TileBatch, postcard::Error> {
    postcard::from_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let batch = TileBatch {
            capture_ts_us: 42,
            stream_width: 3360,
            stream_height: 2100,
            tiles: vec![Tile {
                x: 10,
                y: 20,
                width: 3,
                height: 2,
                qoi: vec![1, 2, 3],
            }],
        };
        assert_eq!(decode(&encode(&batch).unwrap()).unwrap(), batch);
    }
}
