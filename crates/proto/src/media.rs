//! Media datagram format: packetize an encoded frame into datagrams with
//! per-frame XOR parity; reassemble on the client, in order, with
//! single-loss-per-group FEC recovery and, for what parity can't rebuild,
//! datagrams asked for again (see [`Reassembler`]).
//!
//! Chunking is *balanced*: every data chunk of a frame has size
//! `ceil(frame_len / chunk_count)` (the last may be shorter), so a receiver can
//! derive any chunk's expected size from the header alone — which is what makes
//! parity recovery possible without extra bookkeeping on the wire.
//!
//! Parity: a frame's `n` data chunks form `G = ceil(n / S)` parity groups of
//! at most `S` chunks, *interleaved*: chunk `i` belongs to group `i % G`. Each
//! group gets one parity datagram, the XOR of its (zero-padded) chunks, sent
//! after the data. One lost datagram per group is recovered with zero feedback
//! delay, and because consecutive chunks sit in different groups, so is any
//! burst of up to `G` consecutive losses, which is how Wi-Fi loses packets.
//! The sender picks `S` per frame (more parity on lossy links, and for
//! keyframes) and carries it in the header flags. Anything worse is asked
//! for again, and past a deadline falls through to a keyframe request.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use bytes::{Buf, BufMut, Bytes, BytesMut};

/// Fixed header prepended to every media datagram.
///
/// Layout (big-endian): frame_id u64 | chunk_index u16 | chunk_count u16 |
/// frame_len u32 | capture_ts_us u64 | flags u8 | epoch u8 — 26 bytes.
///
/// For parity datagrams (`FLAG_PARITY`), `chunk_index` is the parity *group*
/// index and `chunk_count`/`frame_len` still describe the data frame. The high
/// four bits of `flags` carry the frame's parity group size `S` (1..=15).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaHeader {
    pub frame_id: u64,
    pub chunk_index: u16,
    pub chunk_count: u16,
    pub frame_len: u32,
    pub capture_ts_us: u64,
    pub flags: u8,
    pub epoch: u8,
}

pub const MEDIA_HEADER_LEN: usize = 26;
pub const FLAG_KEYFRAME: u8 = 0b0000_0001;
pub const FLAG_PARITY: u8 = 0b0000_0010;
/// An audio packet (see [`audio_datagram`]), not part of the video stream.
pub const FLAG_AUDIO: u8 = 0b0000_0100;

/// Default data chunks per parity datagram (≈12.5% overhead) on a clean link.
pub const PARITY_GROUP: usize = 8;
/// The largest group size the header can carry.
pub const MAX_PARITY_GROUP: usize = 15;
const GROUP_SHIFT: u8 = 4;

fn group_size_from_flags(flags: u8) -> usize {
    match (flags >> GROUP_SHIFT) as usize {
        0 => PARITY_GROUP,
        size => size,
    }
}

impl MediaHeader {
    pub fn write(&self, buf: &mut BytesMut) {
        buf.put_u64(self.frame_id);
        buf.put_u16(self.chunk_index);
        buf.put_u16(self.chunk_count);
        buf.put_u32(self.frame_len);
        buf.put_u64(self.capture_ts_us);
        buf.put_u8(self.flags);
        buf.put_u8(self.epoch);
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
            epoch: cursor.get_u8(),
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
/// `max_datagram` bytes each, with one parity datagram per `group_size` data
/// chunks (clamped to 1..=[`MAX_PARITY_GROUP`]).
pub fn packetize(
    epoch: u8,
    frame_id: u64,
    capture_ts_us: u64,
    keyframe: bool,
    payload: &[u8],
    max_datagram: usize,
    group_size: usize,
) -> Vec<Bytes> {
    assert!(max_datagram > MEDIA_HEADER_LEN, "datagram size too small");
    let max_chunk = max_datagram - MEDIA_HEADER_LEN;
    let chunk_count = payload.len().div_ceil(max_chunk).max(1);
    assert!(chunk_count <= u16::MAX as usize, "frame too large to packetize");
    let frame_len = payload.len() as u32;
    let group_size = group_size.clamp(1, MAX_PARITY_GROUP);
    let flags = (if keyframe { FLAG_KEYFRAME } else { 0 }) | ((group_size as u8) << GROUP_SHIFT);
    let standard = standard_chunk_size(payload.len(), chunk_count);

    let header = |chunk_index: u16, flags: u8| MediaHeader {
        epoch,
        frame_id,
        chunk_index,
        chunk_count: chunk_count as u16,
        frame_len,
        capture_ts_us,
        flags,
    };

    let group_count = chunk_count.div_ceil(group_size);
    let mut out = Vec::with_capacity(chunk_count + group_count);
    let mut parity = vec![vec![0u8; standard]; group_count];

    for index in 0..chunk_count {
        let begin = index * standard;
        let end = (begin + standard).min(payload.len());
        let chunk = &payload[begin..end.max(begin)];

        let mut buf = BytesMut::with_capacity(MEDIA_HEADER_LEN + chunk.len());
        header(index as u16, flags).write(&mut buf);
        buf.put_slice(chunk);
        out.push(buf.freeze());

        let group = &mut parity[index % group_count];
        for (parity_byte, &data_byte) in group.iter_mut().zip(chunk.iter()) {
            *parity_byte ^= data_byte;
        }
    }
    for (group_index, group) in parity.iter().enumerate() {
        let mut buf = BytesMut::with_capacity(MEDIA_HEADER_LEN + group.len());
        header(group_index as u16, flags | FLAG_PARITY).write(&mut buf);
        buf.put_slice(group);
        out.push(buf.freeze());
    }
    if payload.is_empty() {
        // Degenerate but keep the invariant: every frame yields >= 1 datagram.
        let mut buf = BytesMut::with_capacity(MEDIA_HEADER_LEN);
        header(0, flags).write(&mut buf);
        out.push(buf.freeze());
    }
    out
}

/// One 10 ms audio packet: `seq` counts frames (it keeps counting while the
/// host skips silence, so gaps show their length), and each datagram also
/// carries the previous packet, so one lost datagram costs nothing.
///
/// Payload: current length u16 | current Opus data | previous Opus data.
pub fn audio_datagram(seq: u64, capture_ts_us: u64, current: &[u8], previous: &[u8]) -> Bytes {
    let payload_len = 2 + current.len() + previous.len();
    let mut buf = BytesMut::with_capacity(MEDIA_HEADER_LEN + payload_len);
    MediaHeader {
        frame_id: seq,
        chunk_index: 0,
        chunk_count: 1,
        frame_len: payload_len as u32,
        capture_ts_us,
        flags: FLAG_AUDIO,
        epoch: 0,
    }
    .write(&mut buf);
    buf.put_u16(current.len() as u16);
    buf.put_slice(current);
    buf.put_slice(previous);
    buf.freeze()
}

/// Split an audio datagram's payload into (current, previous) Opus packets.
pub fn parse_audio(payload: &[u8]) -> Option<(&[u8], &[u8])> {
    let (len, rest) = payload.split_first_chunk::<2>()?;
    let len = u16::from_be_bytes(*len) as usize;
    (rest.len() >= len).then(|| rest.split_at(len))
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

/// A frame whose datagrams are still coming in.
struct Partial {
    first_arrival: Instant,
    last_arrival: Instant,
    capture_ts_us: u64,
    flags: u8,
    frame_len: usize,
    chunks: Vec<Option<Bytes>>,
    parity: Vec<Option<Bytes>>,
    received: usize,
    /// When its missing datagrams were last asked for again.
    asked: Option<Instant>,
}

impl Partial {
    fn new(header: &MediaHeader, now: Instant) -> Self {
        let chunk_count = header.chunk_count as usize;
        Self {
            first_arrival: now,
            last_arrival: now,
            capture_ts_us: header.capture_ts_us,
            flags: header.flags & !FLAG_PARITY,
            frame_len: header.frame_len as usize,
            chunks: vec![None; chunk_count],
            parity: vec![None; chunk_count.max(1).div_ceil(group_size_from_flags(header.flags))],
            received: 0,
            asked: None,
        }
    }

    fn is_keyframe(&self) -> bool {
        self.flags & FLAG_KEYFRAME != 0
    }

    fn is_complete(&self) -> bool {
        self.received == self.chunks.len()
    }

    /// Data chunks still missing.
    fn missing(&self) -> Vec<u16> {
        (0..self.chunks.len()).filter(|&index| self.chunks[index].is_none()).map(|index| index as u16).collect()
    }

    /// Stores a data or parity datagram; returns whether parity then
    /// recovered a chunk.
    fn add(&mut self, header: &MediaHeader, payload: &[u8]) -> bool {
        let index = header.chunk_index as usize;
        if header.flags & FLAG_PARITY != 0 {
            match self.parity.get_mut(index) {
                Some(slot @ None) => *slot = Some(Bytes::copy_from_slice(payload)),
                _ => return false, // a duplicate, or malformed
            }
            self.try_recover(index)
        } else {
            match self.chunks.get_mut(index) {
                Some(slot @ None) => *slot = Some(Bytes::copy_from_slice(payload)),
                _ => return false,
            }
            self.received += 1;
            self.try_recover(index % self.parity.len().max(1))
        }
    }

    /// Chunk indexes in parity group `group` (interleaved: `i % groups`).
    fn group_members(&self, group: usize) -> impl Iterator<Item = usize> + Clone {
        (group..self.chunks.len()).step_by(self.parity.len().max(1))
    }

    /// If exactly one chunk of `group` is missing and its parity is present,
    /// reconstruct it. Returns true on recovery.
    fn try_recover(&mut self, group: usize) -> bool {
        let Some(Some(parity)) = self.parity.get(group).cloned() else {
            return false;
        };
        let range = self.group_members(group);
        let mut missing = None;
        for index in range.clone() {
            if self.chunks[index].is_none() && missing.replace(index).is_some() {
                return false; // >1 missing: unrecoverable by XOR
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

    fn finish(self, frame_id: u64) -> CompleteFrame {
        let mut data = BytesMut::with_capacity(self.frame_len);
        for chunk in self.chunks {
            data.put_slice(&chunk.expect("all chunks received"));
        }
        data.truncate(self.frame_len);
        CompleteFrame {
            frame_id,
            capture_ts_us: self.capture_ts_us,
            keyframe: self.flags & FLAG_KEYFRAME != 0,
            data: data.freeze(),
            assembly_us: self.last_arrival.duration_since(self.first_arrival).as_micros() as u64,
        }
    }
}

/// Video datagrams to send again (see `ControlMessage::Resend`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResendRequest {
    pub frame_id: u64,
    /// Missing data chunks; empty when nothing of the frame arrived.
    pub chunks: Vec<u16>,
}

/// How long to wait for datagrams asked for again, from the round trip.
#[derive(Debug, Clone, Copy)]
pub struct Patience {
    /// Ask again after this long (and a frame's datagrams have stopped
    /// arriving after this long without it completing).
    pub retry: Duration,
    /// Give a frame up after this long, and wait for a keyframe instead.
    pub give_up: Duration,
}

impl Patience {
    pub fn for_rtt(rtt: Duration) -> Self {
        Self {
            retry: (rtt + Duration::from_millis(10)).clamp(Duration::from_millis(15), Duration::from_millis(60)),
            give_up: (rtt * 2 + Duration::from_millis(30)).clamp(Duration::from_millis(50), Duration::from_millis(150)),
        }
    }
}

/// What `Reassembler::poll` has for the caller.
#[derive(Debug, Default)]
pub struct Poll {
    /// Frames to decode, in order.
    pub frames: Vec<CompleteFrame>,
    /// Datagrams to ask the host for again.
    pub resend: Vec<ResendRequest>,
    /// A frame was given up: ask the host for a keyframe.
    pub want_keyframe: bool,
}

/// Frames held back waiting for a missing one, at most (1 s at 60 fps).
const MAX_HELD: usize = 60;

/// Puts frames back together and hands them on in order. When one is missing
/// datagrams that parity can't rebuild, it asks for them again and holds
/// later frames (which reference it) until they arrive, which takes about a
/// round trip; waiting for a keyframe instead is what a lost frame used to
/// cost. Past `Patience::give_up` it gives the frame up and waits for a
/// keyframe after all, asking for any of the keyframe's datagrams that go
/// missing too.
#[derive(Default)]
pub struct Reassembler {
    partial: BTreeMap<u64, Partial>,
    complete: BTreeMap<u64, CompleteFrame>,
    /// The next frame to hand on (at the start, the first one seen).
    next: Option<u64>,
    newest: Option<u64>,
    /// Since when `next` has been holding later frames up.
    blocked_since: Option<Instant>,
    /// A frame was given up: only a keyframe can be decoded until one comes.
    awaiting_keyframe: bool,
    keyframe_asked: Option<Instant>,
    /// Frames nothing of which arrived, and when they were last asked for.
    asked_whole: BTreeMap<u64, Instant>,
    pub completed_frames: u64,
    pub dropped_frames: u64,
    pub stale_datagrams: u64,
    pub recovered_chunks: u64,
    /// Datagrams asked for again (whole frames count once)...
    pub resend_requested: u64,
    /// ...and frames that completed after asking.
    pub resend_recovered: u64,
}

impl Reassembler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Discard the old epoch's frames without recording a loss.
    pub fn reset_epoch(&mut self) {
        let counters = (
            self.completed_frames,
            self.dropped_frames,
            self.stale_datagrams,
            self.recovered_chunks,
            self.resend_requested,
            self.resend_recovered,
        );
        *self = Self::default();
        (
            self.completed_frames,
            self.dropped_frames,
            self.stale_datagrams,
            self.recovered_chunks,
            self.resend_requested,
            self.resend_recovered,
        ) = counters;
    }

    /// Something is held or on its way: poll again soon, even with no
    /// datagrams arriving, so a stall gets noticed.
    pub fn is_waiting(&self) -> bool {
        !self.partial.is_empty() || !self.complete.is_empty()
    }

    pub fn push(&mut self, datagram: &[u8], now: Instant) {
        let Some((header, payload)) = MediaHeader::parse(datagram) else { return };
        let id = header.frame_id;
        let next = *self.next.get_or_insert(id);
        if id < next {
            // Parity trailing a frame that already went is expected.
            if id + 1 != next {
                self.stale_datagrams += 1;
            }
            return;
        }
        if self.complete.contains_key(&id) {
            return;
        }
        self.newest = Some(self.newest.map_or(id, |newest| newest.max(id)));
        let partial = self.partial.entry(id).or_insert_with(|| Partial::new(&header, now));
        partial.last_arrival = now;
        if partial.add(&header, payload) {
            self.recovered_chunks += 1;
        }
        if partial.is_complete() {
            let partial = self.partial.remove(&id).expect("just used");
            if partial.asked.is_some() || self.asked_whole.remove(&id).is_some() {
                self.resend_recovered += 1;
            }
            self.completed_frames += 1;
            self.complete.insert(id, partial.finish(id));
        }
    }

    pub fn poll(&mut self, now: Instant, patience: Patience) -> Poll {
        let mut poll = Poll::default();
        let Some(mut next) = self.next else { return poll };
        loop {
            if !self.awaiting_keyframe {
                if let Some(frame) = self.complete.remove(&next) {
                    poll.frames.push(frame);
                    next += 1;
                    self.blocked_since = None;
                    continue;
                }
            }
            // A complete keyframe needs nothing before it.
            match self.complete.range(next..).find(|(_, frame)| frame.keyframe).map(|(&id, _)| id) {
                Some(id) => {
                    self.forget_before(id);
                    next = id;
                    self.awaiting_keyframe = false;
                    self.blocked_since = None;
                }
                None => break,
            }
        }
        self.next = Some(next);
        let newest = self.newest.unwrap_or(next);

        if self.awaiting_keyframe {
            self.ask_for_keyframes(now, newest, patience, &mut poll);
            return poll;
        }
        let stalled = self.partial.get(&next).is_some_and(|partial| now - partial.last_arrival >= patience.retry);
        if newest <= next && !stalled {
            self.blocked_since = None;
            return poll;
        }
        let since = *self.blocked_since.get_or_insert(now);
        if now - since >= patience.give_up || self.partial.len() + self.complete.len() > MAX_HELD {
            self.dropped_frames += 1;
            self.awaiting_keyframe = true;
            self.blocked_since = None;
            self.partial.remove(&next);
            self.keyframe_asked = Some(now);
            poll.want_keyframe = true;
            self.ask_for_keyframes(now, newest, patience, &mut poll);
            return poll;
        }
        for id in next..=newest.min(next + MAX_HELD as u64) {
            if self.complete.contains_key(&id) {
                continue;
            }
            match self.partial.get_mut(&id) {
                Some(partial) => {
                    let arriving = id == newest && now - partial.last_arrival < patience.retry;
                    if arriving || partial.asked.is_some_and(|at| now - at < patience.retry) {
                        continue;
                    }
                    partial.asked = Some(now);
                    let chunks = partial.missing();
                    self.resend_requested += chunks.len() as u64;
                    poll.resend.push(ResendRequest { frame_id: id, chunks });
                }
                None => {
                    if self.asked_whole.get(&id).is_some_and(|&at| now - at < patience.retry) {
                        continue;
                    }
                    self.asked_whole.insert(id, now);
                    self.resend_requested += 1;
                    poll.resend.push(ResendRequest { frame_id: id, chunks: Vec::new() });
                }
            }
        }
        poll
    }

    /// While waiting for a keyframe: ask for the missing datagrams of any on
    /// its way, or for a new one if none is (after `give_up`, since the
    /// host's answer takes a while).
    fn ask_for_keyframes(&mut self, now: Instant, newest: u64, patience: Patience, poll: &mut Poll) {
        // Frames before the newest keyframe on its way can't be decoded.
        if let Some(id) = self.partial.iter().rev().find(|(_, partial)| partial.is_keyframe()).map(|(&id, _)| id) {
            self.forget_before(id);
        }
        let mut coming = false;
        for (&id, partial) in self.partial.iter_mut().filter(|(_, partial)| partial.is_keyframe()) {
            coming = true;
            let arriving = id == newest && now - partial.last_arrival < patience.retry;
            if arriving || partial.asked.is_some_and(|at| now - at < patience.retry) {
                continue;
            }
            partial.asked = Some(now);
            let chunks = partial.missing();
            self.resend_requested += chunks.len() as u64;
            poll.resend.push(ResendRequest { frame_id: id, chunks });
        }
        if !coming && self.keyframe_asked.is_none_or(|at| now - at >= patience.give_up) {
            self.keyframe_asked = Some(now);
            poll.want_keyframe = true;
        }
        // Bounded, oldest first.
        while self.partial.len() + self.complete.len() > MAX_HELD {
            let oldest_partial = self.partial.keys().next().copied().unwrap_or(u64::MAX);
            let oldest_complete = self.complete.keys().next().copied().unwrap_or(u64::MAX);
            if oldest_complete < oldest_partial {
                self.complete.pop_first();
            } else {
                self.partial.pop_first();
            }
        }
    }

    fn forget_before(&mut self, id: u64) {
        self.partial = self.partial.split_off(&id);
        self.complete = self.complete.split_off(&id);
        self.asked_whole = self.asked_whole.split_off(&id);
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
    fn audio_datagram_roundtrip() {
        let datagram = audio_datagram(77, 123, b"now", b"before");
        let (header, payload) = MediaHeader::parse(&datagram).unwrap();
        assert!(header.flags & FLAG_AUDIO != 0);
        assert_eq!((header.frame_id, header.capture_ts_us), (77, 123));
        assert_eq!(parse_audio(payload), Some((&b"now"[..], &b"before"[..])));
        assert_eq!(parse_audio(&[0, 9, 1]), None, "truncated");
    }

    #[test]
    fn header_roundtrip() {
        let header = MediaHeader {
            epoch: 173,
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

    fn patience() -> Patience {
        Patience { retry: Duration::from_millis(20), give_up: Duration::from_millis(60) }
    }

    /// Pushes datagrams at `now`, then polls.
    fn feed<'a>(reassembler: &mut Reassembler, datagrams: impl IntoIterator<Item = &'a Bytes>, now: Instant) -> Poll {
        for datagram in datagrams {
            reassembler.push(datagram, now);
        }
        reassembler.poll(now, patience())
    }

    fn ids(poll: &Poll) -> Vec<u64> {
        poll.frames.iter().map(|frame| frame.frame_id).collect()
    }

    /// Frame `id`'s datagrams, without data chunks `lost` (parity kept).
    fn without(datagrams: &[Bytes], lost: &[usize]) -> Vec<Bytes> {
        let mut data_index = 0;
        datagrams
            .iter()
            .filter(|datagram| {
                if is_parity(datagram) {
                    return true;
                }
                data_index += 1;
                !lost.contains(&(data_index - 1))
            })
            .cloned()
            .collect()
    }

    #[test]
    fn packetize_reassemble_roundtrip() {
        let payload = payload(10_000);
        let datagrams = packetize(0, 1, 99, true, &payload, 1200, PARITY_GROUP);
        let data_count = datagrams.iter().filter(|d| !is_parity(d)).count();
        let parity_count = datagrams.len() - data_count;
        assert!(data_count > 1);
        assert_eq!(parity_count, data_count.div_ceil(PARITY_GROUP));

        let mut reassembler = Reassembler::new();
        let poll = feed(&mut reassembler, &datagrams, Instant::now());
        let frame = &poll.frames[0];
        assert_eq!(frame.frame_id, 1);
        assert!(frame.keyframe);
        assert_eq!(&frame.data[..], &payload[..]);
        assert_eq!(reassembler.recovered_chunks, 0);
    }

    #[test]
    fn single_loss_per_group_is_recovered() {
        let payload = payload(9_500); // 9 chunks, 2 interleaved groups
        let datagrams = packetize(0, 7, 0, false, &payload, 1200, PARITY_GROUP);
        let mut reassembler = Reassembler::new();
        // One data datagram lost in each group: chunks 0 (group 0) and 1 (group 1).
        let poll = feed(&mut reassembler, &without(&datagrams, &[0, 1]), Instant::now());
        assert_eq!(&poll.frames[0].data[..], &payload[..], "FEC should recover every group");
        assert_eq!(reassembler.recovered_chunks, 2);
    }

    /// Drop `burst` consecutive data datagrams starting at data chunk `at`;
    /// return whether the frame still completed, intact, without asking.
    fn survives_burst(len: usize, group_size: usize, at: usize, burst: usize) -> bool {
        let payload = payload(len);
        let datagrams = packetize(0, 11, 0, false, &payload, 1200, group_size);
        let lost: Vec<usize> = (at..at + burst).collect();
        let mut reassembler = Reassembler::new();
        let poll = feed(&mut reassembler, &without(&datagrams, &lost), Instant::now());
        poll.frames.first().is_some_and(|frame| frame.data[..] == payload[..])
    }

    #[test]
    fn a_burst_as_long_as_the_group_count_is_recovered() {
        // 24 chunks in groups of 8: 3 interleaved groups, so any 3 in a row.
        let len = 24 * (1200 - MEDIA_HEADER_LEN);
        assert!(survives_burst(len, 8, 5, 3));
        assert!(survives_burst(len, 8, 21, 3));
        assert!(!survives_burst(len, 8, 5, 4), "4 in a row hits one group twice");
    }

    #[test]
    fn smaller_groups_survive_longer_bursts() {
        // 10 chunks in groups of 2: 5 groups, so 5 in a row, at 50% overhead.
        let len = 10 * (1200 - MEDIA_HEADER_LEN);
        let datagrams = packetize(0, 1, 0, true, &payload(len), 1200, 2);
        assert_eq!(datagrams.iter().filter(|d| is_parity(d)).count(), 5);
        let (header, _) = MediaHeader::parse(&datagrams[0]).unwrap();
        assert_eq!(group_size_from_flags(header.flags), 2);
        assert!(header.flags & FLAG_KEYFRAME != 0);
        assert!(survives_burst(len, 2, 3, 5));
        assert!(!survives_burst(len, 2, 3, 6));
    }

    #[test]
    fn last_short_chunk_is_recoverable() {
        let payload = payload(2_500); // 3 chunks at 1200 max: sizes 834/834/832
        let datagrams = packetize(0, 9, 0, false, &payload, 1200, PARITY_GROUP);
        let mut reassembler = Reassembler::new();
        let poll = feed(&mut reassembler, &without(&datagrams, &[2]), Instant::now());
        assert_eq!(&poll.frames[0].data[..], &payload[..], "short last chunk should be recovered");
        assert_eq!(reassembler.recovered_chunks, 1);
    }

    #[test]
    fn frames_come_out_in_order() {
        let mut reassembler = Reassembler::new();
        let now = Instant::now();
        let frames: Vec<Vec<Bytes>> =
            (1..=3).map(|id| packetize(0, id, 0, id == 1, &payload(3000), 1200, PARITY_GROUP)).collect();
        assert_eq!(ids(&feed(&mut reassembler, frames.iter().flatten(), now)), [1, 2, 3]);
    }

    #[test]
    fn what_parity_cant_rebuild_is_asked_for_and_later_frames_wait() {
        let now = Instant::now();
        let first = packetize(0, 1, 0, false, &payload(9_000), 1200, PARITY_GROUP);
        let second = packetize(0, 2, 0, false, &payload(3000), 1200, PARITY_GROUP);
        let mut reassembler = Reassembler::new();
        // Chunks 0 and 2 share a group: parity can't rebuild both.
        feed(&mut reassembler, &without(&first, &[0, 2]), now);
        let poll = feed(&mut reassembler, &second, now);
        assert!(poll.frames.is_empty(), "frame 2 references frame 1, so it waits");
        assert_eq!(poll.resend, [ResendRequest { frame_id: 1, chunks: vec![0, 2] }]);

        // The host sends them again.
        let resent: Vec<Bytes> = first.iter().filter(|d| !is_parity(d)).take(3).cloned().collect();
        let poll = feed(&mut reassembler, &resent, now + Duration::from_millis(15));
        assert_eq!(ids(&poll), [1, 2]);
        assert_eq!(reassembler.resend_recovered, 1);
        assert_eq!(reassembler.dropped_frames, 0);
    }

    #[test]
    fn asks_again_after_a_retry_interval() {
        let now = Instant::now();
        let first = packetize(0, 1, 0, false, &payload(9_000), 1200, PARITY_GROUP);
        let second = packetize(0, 2, 0, false, &payload(3000), 1200, PARITY_GROUP);
        let mut reassembler = Reassembler::new();
        feed(&mut reassembler, &without(&first, &[0, 2]), now);
        assert_eq!(feed(&mut reassembler, &second, now).resend.len(), 1);
        assert!(reassembler.poll(now + Duration::from_millis(10), patience()).resend.is_empty());
        assert_eq!(reassembler.poll(now + Duration::from_millis(25), patience()).resend.len(), 1);
    }

    #[test]
    fn a_frame_that_never_arrived_is_asked_for_whole() {
        let now = Instant::now();
        let mut reassembler = Reassembler::new();
        let one = packetize(0, 1, 0, true, &payload(3000), 1200, PARITY_GROUP);
        let three = packetize(0, 3, 0, false, &payload(3000), 1200, PARITY_GROUP);
        let poll = feed(&mut reassembler, one.iter().chain(&three), now);
        assert_eq!(ids(&poll), [1]);
        assert_eq!(poll.resend, [ResendRequest { frame_id: 2, chunks: vec![] }]);
    }

    #[test]
    fn a_stalled_frame_is_asked_for_even_with_nothing_after_it() {
        let now = Instant::now();
        let frame = packetize(0, 1, 0, false, &payload(9_000), 1200, PARITY_GROUP);
        let mut reassembler = Reassembler::new();
        let poll = feed(&mut reassembler, &without(&frame, &[0, 2]), now);
        assert!(poll.resend.is_empty(), "it may still be arriving");
        assert!(reassembler.is_waiting());
        let poll = reassembler.poll(now + Duration::from_millis(25), patience());
        assert_eq!(poll.resend, [ResendRequest { frame_id: 1, chunks: vec![0, 2] }]);
    }

    #[test]
    fn gives_up_after_the_deadline_and_waits_for_a_keyframe() {
        let now = Instant::now();
        let packet = |id, keyframe| packetize(0, id, 0, keyframe, &payload(3000), 1200, PARITY_GROUP);
        let first = packetize(0, 1, 0, false, &payload(9_000), 1200, PARITY_GROUP);
        let mut reassembler = Reassembler::new();
        feed(&mut reassembler, &without(&first, &[0, 2]), now);
        feed(&mut reassembler, &packet(2, false), now);
        let poll = reassembler.poll(now + Duration::from_millis(61), patience());
        assert!(poll.want_keyframe && poll.frames.is_empty());
        assert_eq!(reassembler.dropped_frames, 1);

        let later = now + Duration::from_millis(70);
        assert!(feed(&mut reassembler, &packet(3, false), later).frames.is_empty(), "nothing to decode it against");
        assert_eq!(ids(&feed(&mut reassembler, &packet(4, true), later)), [4]);
        assert_eq!(ids(&feed(&mut reassembler, &packet(5, false), later)), [5]);
    }

    #[test]
    fn while_waiting_for_a_keyframe_its_missing_datagrams_are_asked_for() {
        let now = Instant::now();
        let first = packetize(0, 1, 0, false, &payload(9_000), 1200, PARITY_GROUP);
        let second = packetize(0, 2, 0, false, &payload(3000), 1200, PARITY_GROUP);
        let mut reassembler = Reassembler::new();
        feed(&mut reassembler, first.iter().chain(&second).skip(1), now);
        reassembler.poll(now + Duration::from_millis(61), patience()); // gives up on 1
        let keyframe = packetize(0, 3, 0, true, &payload(9_000), 1200, PARITY_GROUP);
        let after = packetize(0, 4, 0, false, &payload(3000), 1200, PARITY_GROUP);
        let later = now + Duration::from_millis(70);
        feed(&mut reassembler, &without(&keyframe, &[0, 2]), later);
        let poll = feed(&mut reassembler, &after, later);
        assert!(!poll.want_keyframe, "one is on its way");
        assert_eq!(poll.resend, [ResendRequest { frame_id: 3, chunks: vec![0, 2] }]);
    }

    #[test]
    fn a_complete_keyframe_doesnt_wait_for_a_missing_frame() {
        let now = Instant::now();
        let first = packetize(0, 1, 0, false, &payload(9_000), 1200, PARITY_GROUP);
        let keyframe = packetize(0, 2, 0, true, &payload(3000), 1200, PARITY_GROUP);
        let mut reassembler = Reassembler::new();
        feed(&mut reassembler, &without(&first, &[0, 2]), now);
        assert_eq!(ids(&feed(&mut reassembler, &keyframe, now)), [2]);
        // Frame 1 is history now: its late datagrams are stale.
        reassembler.push(&first[0], now);
        assert_eq!(reassembler.stale_datagrams, 1);
        assert_eq!(reassembler.dropped_frames, 0);
    }
}
