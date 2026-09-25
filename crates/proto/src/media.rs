//! Media datagram format: packetize an encoded frame into datagrams with
//! per-frame XOR parity, reassemble on the client with a latest-frame-wins
//! policy (no jitter buffer) and single-loss-per-group FEC recovery.
//!
//! Chunking is *balanced*: every data chunk of a frame has size
//! `ceil(frame_len / chunk_count)` (the last may be shorter), so a receiver can
//! derive any chunk's expected size from the header alone — which is what makes
//! parity recovery possible without extra bookkeeping on the wire.
//!
//! Parity: data chunks are grouped in [`PARITY_GROUP`]s; each group gets one
//! parity datagram carrying the XOR of its (zero-padded) chunks. One lost
//! datagram per group is recovered with zero feedback delay (research/03 §4).
//! Multi-loss falls through to latest-frame-wins + keyframe request.

use bytes::{Buf, BufMut, Bytes, BytesMut};

/// Fixed header prepended to every media datagram.
///
/// Layout (big-endian): frame_id u64 | chunk_index u16 | chunk_count u16 |
/// frame_len u32 | capture_ts_us u64 | flags u8 — 25 bytes.
///
/// For parity datagrams (`FLAG_PARITY`), `chunk_index` is the parity *group*
/// index and `chunk_count`/`frame_len` still describe the data frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaHeader {
    pub frame_id: u64,
    pub chunk_index: u16,
    pub chunk_count: u16,
    pub frame_len: u32,
    pub capture_ts_us: u64,
    pub flags: u8,
}

pub const MEDIA_HEADER_LEN: usize = 25;
pub const FLAG_KEYFRAME: u8 = 0b0000_0001;
pub const FLAG_PARITY: u8 = 0b0000_0010;

/// Data chunks per parity datagram (≈12.5% overhead). Static for protocol v0;
/// adapting it to measured loss is congestion-control work (Milestone 1+).
pub const PARITY_GROUP: usize = 8;

impl MediaHeader {
    pub fn write(&self, buf: &mut BytesMut) {
        buf.put_u64(self.frame_id);
        buf.put_u16(self.chunk_index);
        buf.put_u16(self.chunk_count);
        buf.put_u32(self.frame_len);
        buf.put_u64(self.capture_ts_us);
        buf.put_u8(self.flags);
    }

    /// Parse a datagram into (header, payload). Returns `None` if too short.
    pub fn parse(datagram: &[u8]) -> Option<(MediaHeader, &[u8])> {
        if datagram.len() < MEDIA_HEADER_LEN {
            return None;
        }
        let mut cursor = datagram;
        let header = MediaHeader {
            frame_id: cursor.get_u64(),
            chunk_index: cursor.get_u16(),
            chunk_count: cursor.get_u16(),
            frame_len: cursor.get_u32(),
            capture_ts_us: cursor.get_u64(),
            flags: cursor.get_u8(),
        };
        Some((header, &datagram[MEDIA_HEADER_LEN..]))
    }
}

/// Size of every data chunk except possibly the last, derived from the header.
fn standard_chunk_size(frame_len: usize, chunk_count: usize) -> usize {
    frame_len.div_ceil(chunk_count.max(1)).max(1)
}

/// Expected size of chunk `index`, derived from the header.
fn chunk_size_at(frame_len: usize, chunk_count: usize, index: usize) -> usize {
    let standard = standard_chunk_size(frame_len, chunk_count);
    if index + 1 == chunk_count {
        frame_len - standard * (chunk_count - 1)
    } else {
        standard
    }
}

/// Split an encoded frame into data + parity datagrams of at most
/// `max_datagram` bytes each.
pub fn packetize(
    frame_id: u64,
    capture_ts_us: u64,
    keyframe: bool,
    payload: &[u8],
    max_datagram: usize,
) -> Vec<Bytes> {
    assert!(max_datagram > MEDIA_HEADER_LEN, "datagram size too small");
    let max_chunk = max_datagram - MEDIA_HEADER_LEN;
    let chunk_count = payload.len().div_ceil(max_chunk).max(1);
    assert!(chunk_count <= u16::MAX as usize, "frame too large to packetize");
    let frame_len = payload.len() as u32;
    let flags = if keyframe { FLAG_KEYFRAME } else { 0 };
    let standard = standard_chunk_size(payload.len(), chunk_count);

    let header = |chunk_index: u16, flags: u8| MediaHeader {
        frame_id,
        chunk_index,
        chunk_count: chunk_count as u16,
        frame_len,
        capture_ts_us,
        flags,
    };

    let group_count = chunk_count.div_ceil(PARITY_GROUP);
    let mut out = Vec::with_capacity(chunk_count + group_count);
    let mut parity = vec![0u8; standard];

    for index in 0..chunk_count {
        let begin = index * standard;
        let end = (begin + standard).min(payload.len());
        let chunk = &payload[begin..end.max(begin)];

        let mut buf = BytesMut::with_capacity(MEDIA_HEADER_LEN + chunk.len());
        header(index as u16, flags).write(&mut buf);
        buf.put_slice(chunk);
        out.push(buf.freeze());

        for (parity_byte, &data_byte) in parity.iter_mut().zip(chunk.iter()) {
            *parity_byte ^= data_byte;
        }
        let group_ends = index % PARITY_GROUP == PARITY_GROUP - 1 || index + 1 == chunk_count;
        if group_ends {
            let group_index = (index / PARITY_GROUP) as u16;
            let mut buf = BytesMut::with_capacity(MEDIA_HEADER_LEN + parity.len());
            header(group_index, flags | FLAG_PARITY).write(&mut buf);
            buf.put_slice(&parity);
            out.push(buf.freeze());
            parity.iter_mut().for_each(|byte| *byte = 0);
        }
    }
    if payload.is_empty() {
        // Degenerate but keep the invariant: every frame yields >= 1 datagram.
        let mut buf = BytesMut::with_capacity(MEDIA_HEADER_LEN);
        header(0, flags).write(&mut buf);
        out.push(buf.freeze());
    }
    out
}

#[derive(Debug, Clone)]
pub struct CompleteFrame {
    pub frame_id: u64,
    pub capture_ts_us: u64,
    pub keyframe: bool,
    pub data: Bytes,
    /// First datagram to completion, on the receiver's clock: how long the
    /// network spread this frame out (serialization, bursts, stalls).
    pub assembly_us: u64,
}

struct Partial {
    first_arrival: std::time::Instant,
    frame_id: u64,
    capture_ts_us: u64,
    flags: u8,
    frame_len: usize,
    chunks: Vec<Option<Bytes>>,
    parity: Vec<Option<Bytes>>,
    received: usize,
}

impl Partial {
    fn group_range(&self, group: usize) -> std::ops::Range<usize> {
        let begin = group * PARITY_GROUP;
        begin..(begin + PARITY_GROUP).min(self.chunks.len())
    }

    /// If exactly one chunk of `group` is missing and its parity is present,
    /// reconstruct it. Returns true on recovery.
    fn try_recover(&mut self, group: usize) -> bool {
        let Some(Some(parity)) = self.parity.get(group).cloned() else {
            return false;
        };
        let range = self.group_range(group);
        let mut missing = None;
        for index in range.clone() {
            if self.chunks[index].is_none() {
                if missing.replace(index).is_some() {
                    return false; // >1 missing: unrecoverable by XOR
                }
            }
        }
        let Some(missing) = missing else { return false };

        let mut recovered = parity.to_vec();
        for index in range {
            if index == missing {
                continue;
            }
            let chunk = self.chunks[index].as_ref().expect("checked above");
            for (recovered_byte, &data_byte) in recovered.iter_mut().zip(chunk.iter()) {
                *recovered_byte ^= data_byte;
            }
        }
        let expected = chunk_size_at(self.frame_len, self.chunks.len(), missing);
        if expected > recovered.len() {
            return false; // malformed parity
        }
        recovered.truncate(expected);
        self.chunks[missing] = Some(Bytes::from(recovered));
        self.received += 1;
        true
    }
}

/// Latest-frame-wins reassembler with XOR-parity recovery: at most one frame in
/// flight; a datagram from a newer frame abandons the current partial (counted
/// in `dropped_frames`), stale datagrams are discarded. No queueing, ever.
#[derive(Default)]
pub struct Reassembler {
    current: Option<Partial>,
    last_completed: Option<u64>,
    pub completed_frames: u64,
    pub dropped_frames: u64,
    pub stale_datagrams: u64,
    pub recovered_chunks: u64,
}

impl Reassembler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, datagram: &[u8]) -> Option<CompleteFrame> {
        let (header, payload) = MediaHeader::parse(datagram)?;

        if let Some(done) = self.last_completed {
            // Datagrams for the just-completed frame (typically parity arriving
            // after the data completed it) are expected — ignore silently.
            if header.frame_id == done {
                return None;
            }
            if header.frame_id < done {
                self.stale_datagrams += 1;
                return None;
            }
        }
        match &self.current {
            Some(partial) if header.frame_id < partial.frame_id => {
                self.stale_datagrams += 1;
                return None;
            }
            Some(partial) if header.frame_id > partial.frame_id => {
                self.dropped_frames += 1;
                self.current = None;
            }
            _ => {}
        }

        let chunk_count = header.chunk_count as usize;
        let partial = self.current.get_or_insert_with(|| Partial {
            first_arrival: std::time::Instant::now(),
            frame_id: header.frame_id,
            capture_ts_us: header.capture_ts_us,
            flags: header.flags & !FLAG_PARITY,
            frame_len: header.frame_len as usize,
            chunks: vec![None; chunk_count],
            parity: vec![None; chunk_count.div_ceil(PARITY_GROUP)],
            received: 0,
        });

        let index = header.chunk_index as usize;
        if header.flags & FLAG_PARITY != 0 {
            let Some(slot) = partial.parity.get_mut(index) else {
                return None; // malformed
            };
            if slot.is_none() {
                *slot = Some(Bytes::copy_from_slice(payload));
                if partial.try_recover(index) {
                    self.recovered_chunks += 1;
                }
            }
        } else {
            if index >= partial.chunks.len() {
                return None; // malformed
            }
            if partial.chunks[index].is_none() {
                partial.chunks[index] = Some(Bytes::copy_from_slice(payload));
                partial.received += 1;
                let group = index / PARITY_GROUP;
                if partial.try_recover(group) {
                    self.recovered_chunks += 1;
                }
            }
        }

        if partial.received < partial.chunks.len() {
            return None;
        }

        let partial = self.current.take().expect("just inserted");
        let mut data = BytesMut::with_capacity(partial.frame_len);
        for chunk in partial.chunks.into_iter() {
            data.put_slice(&chunk.expect("all chunks received"));
        }
        data.truncate(partial.frame_len);
        self.last_completed = Some(partial.frame_id);
        self.completed_frames += 1;
        Some(CompleteFrame {
            frame_id: partial.frame_id,
            capture_ts_us: partial.capture_ts_us,
            keyframe: partial.flags & FLAG_KEYFRAME != 0,
            data: data.freeze(),
            assembly_us: partial.first_arrival.elapsed().as_micros() as u64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(len: usize) -> Vec<u8> {
        (0..len).map(|value| (value * 7 + 3) as u8).collect()
    }

    fn is_parity(datagram: &[u8]) -> bool {
        MediaHeader::parse(datagram).unwrap().0.flags & FLAG_PARITY != 0
    }

    #[test]
    fn header_roundtrip() {
        let header = MediaHeader {
            frame_id: 42,
            chunk_index: 3,
            chunk_count: 9,
            frame_len: 12345,
            capture_ts_us: 1_234_567,
            flags: FLAG_KEYFRAME,
        };
        let mut buf = BytesMut::new();
        header.write(&mut buf);
        buf.put_slice(b"payload");
        let (parsed, rest) = MediaHeader::parse(&buf).unwrap();
        assert_eq!(parsed, header);
        assert_eq!(rest, b"payload");
    }

    #[test]
    fn packetize_reassemble_roundtrip() {
        let payload = payload(10_000);
        let datagrams = packetize(1, 99, true, &payload, 1200);
        let data_count = datagrams.iter().filter(|d| !is_parity(d)).count();
        let parity_count = datagrams.len() - data_count;
        assert!(data_count > 1);
        assert_eq!(parity_count, data_count.div_ceil(PARITY_GROUP));

        let mut reassembler = Reassembler::new();
        let mut complete = None;
        for datagram in &datagrams {
            complete = reassembler.push(datagram).or(complete);
        }
        let frame = complete.expect("frame should complete");
        assert_eq!(frame.frame_id, 1);
        assert!(frame.keyframe);
        assert_eq!(&frame.data[..], &payload[..]);
        assert_eq!(reassembler.recovered_chunks, 0);
    }

    #[test]
    fn single_loss_per_group_is_recovered() {
        let payload = payload(9_500);
        let datagrams = packetize(7, 0, false, &payload, 1200);
        let mut reassembler = Reassembler::new();
        let mut complete = None;
        // Drop the first data datagram of every parity group.
        let mut data_index = 0usize;
        for datagram in &datagrams {
            if !is_parity(datagram) {
                let drop = data_index % PARITY_GROUP == 0;
                data_index += 1;
                if drop {
                    continue;
                }
            }
            complete = reassembler.push(datagram).or(complete);
        }
        let frame = complete.expect("FEC should recover every group");
        assert_eq!(&frame.data[..], &payload[..]);
        assert!(reassembler.recovered_chunks > 0);
    }

    #[test]
    fn last_short_chunk_is_recoverable() {
        let payload = payload(2_500); // 3 chunks at 1200 max: sizes 834/834/832
        let datagrams = packetize(9, 0, false, &payload, 1200);
        let last_data_index = datagrams
            .iter()
            .enumerate()
            .filter(|(_, d)| !is_parity(d))
            .map(|(i, _)| i)
            .last()
            .unwrap();
        let mut reassembler = Reassembler::new();
        let mut complete = None;
        for (index, datagram) in datagrams.iter().enumerate() {
            if index == last_data_index {
                continue;
            }
            complete = reassembler.push(datagram).or(complete);
        }
        let frame = complete.expect("short last chunk should be recovered");
        assert_eq!(&frame.data[..], &payload[..]);
        assert_eq!(reassembler.recovered_chunks, 1);
    }

    #[test]
    fn double_loss_in_group_is_not_recovered() {
        let payload = payload(9_000);
        let datagrams = packetize(3, 0, false, &payload, 1200);
        let mut reassembler = Reassembler::new();
        let mut complete = None;
        let mut data_index = 0usize;
        for datagram in &datagrams {
            if !is_parity(datagram) {
                let drop = data_index < 2; // two losses in group 0
                data_index += 1;
                if drop {
                    continue;
                }
            }
            complete = reassembler.push(datagram).or(complete);
        }
        assert!(complete.is_none());
    }

    #[test]
    fn newer_frame_supersedes_partial() {
        let old = packetize(1, 0, false, &payload(5000), 1200);
        let new = packetize(2, 0, false, &payload(3000), 1200);
        let mut reassembler = Reassembler::new();
        // Only part of frame 1 arrives, then all of frame 2.
        reassembler.push(&old[0]);
        let mut complete = None;
        for datagram in &new {
            complete = reassembler.push(datagram).or(complete);
        }
        assert_eq!(complete.unwrap().frame_id, 2);
        assert_eq!(reassembler.dropped_frames, 1);
        // Late chunk of frame 1 is stale now.
        assert!(reassembler.push(&old[1]).is_none());
        assert_eq!(reassembler.stale_datagrams, 1);
    }
}
