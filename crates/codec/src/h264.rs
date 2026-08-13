//! H.264 Annex B utilities.
//!
//! Wire format decision: encoded frames travel as Annex B (start-code
//! delimited NAL units), with SPS/PPS prepended to every keyframe so a client
//! can join and recover statelessly. Hardware APIs mostly speak AVCC
//! (length-prefixed), so both directions of conversion live here, pure and
//! unit-tested on every platform.

/// NAL unit types we care about.
pub const NAL_SPS: u8 = 7;
pub const NAL_PPS: u8 = 8;
pub const NAL_IDR: u8 = 5;

const START_CODE: [u8; 4] = [0, 0, 0, 1];

pub fn nal_type(nal: &[u8]) -> Option<u8> {
    nal.first().map(|&byte| byte & 0x1f)
}

/// Split an Annex B stream into NAL unit payloads (start codes stripped).
/// Accepts both 3-byte and 4-byte start codes.
pub fn split_annexb(data: &[u8]) -> Vec<&[u8]> {
    let mut units = Vec::new();
    let mut search_from = 0;
    let mut current_start: Option<usize> = None;
    while search_from + 3 <= data.len() {
        let three = &data[search_from..search_from + 3];
        if three == [0, 0, 1] {
            let code_len = if search_from >= 1 && data[search_from - 1] == 0 {
                4
            } else {
                3
            };
            let nal_begin = search_from + 3;
            if let Some(begin) = current_start {
                let end = search_from - (code_len - 3);
                if end > begin {
                    units.push(&data[begin..end]);
                }
            }
            current_start = Some(nal_begin);
            search_from = nal_begin;
        } else {
            search_from += 1;
        }
    }
    if let Some(begin) = current_start {
        if data.len() > begin {
            units.push(&data[begin..]);
        }
    }
    units
}

/// Convert AVCC (4-byte big-endian length prefixes) to Annex B, appending to `out`.
pub fn avcc_to_annexb(avcc: &[u8], out: &mut Vec<u8>) {
    let mut offset = 0;
    while offset + 4 <= avcc.len() {
        let len = u32::from_be_bytes([
            avcc[offset],
            avcc[offset + 1],
            avcc[offset + 2],
            avcc[offset + 3],
        ]) as usize;
        offset += 4;
        if len == 0 || offset + len > avcc.len() {
            break;
        }
        out.extend_from_slice(&START_CODE);
        out.extend_from_slice(&avcc[offset..offset + len]);
        offset += len;
    }
}

/// Append one NAL unit to an Annex B stream.
pub fn push_annexb_nal(out: &mut Vec<u8>, nal: &[u8]) {
    out.extend_from_slice(&START_CODE);
    out.extend_from_slice(nal);
}

/// Convert an Annex B stream to AVCC, dropping SPS/PPS units (the decoder
/// receives those out of band via its format description). Returns the AVCC
/// buffer plus any SPS/PPS units encountered.
pub struct AnnexbFrame<'a> {
    pub avcc: Vec<u8>,
    pub sps: Vec<&'a [u8]>,
    pub pps: Vec<&'a [u8]>,
    pub has_idr: bool,
}

pub fn annexb_to_avcc(data: &[u8]) -> AnnexbFrame<'_> {
    let mut frame = AnnexbFrame {
        avcc: Vec::with_capacity(data.len()),
        sps: Vec::new(),
        pps: Vec::new(),
        has_idr: false,
    };
    for nal in split_annexb(data) {
        match nal_type(nal) {
            Some(NAL_SPS) => frame.sps.push(nal),
            Some(NAL_PPS) => frame.pps.push(nal),
            other => {
                if other == Some(NAL_IDR) {
                    frame.has_idr = true;
                }
                frame
                    .avcc
                    .extend_from_slice(&(nal.len() as u32).to_be_bytes());
                frame.avcc.extend_from_slice(nal);
            }
        }
    }
    frame
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_handles_three_and_four_byte_codes() {
        let mut data = vec![0, 0, 0, 1, 0x67, 1, 2];
        data.extend_from_slice(&[0, 0, 1, 0x68, 3]);
        data.extend_from_slice(&[0, 0, 0, 1, 0x65, 9, 9, 9]);
        let units = split_annexb(&data);
        assert_eq!(units.len(), 3);
        assert_eq!(nal_type(units[0]), Some(NAL_SPS));
        assert_eq!(nal_type(units[1]), Some(NAL_PPS));
        assert_eq!(nal_type(units[2]), Some(NAL_IDR));
        assert_eq!(units[2], &[0x65, 9, 9, 9]);
    }

    #[test]
    fn avcc_annexb_roundtrip() {
        let nal_a = [0x65u8, 1, 2, 3];
        let nal_b = [0x41u8, 4, 5];
        let mut avcc = Vec::new();
        avcc.extend_from_slice(&(nal_a.len() as u32).to_be_bytes());
        avcc.extend_from_slice(&nal_a);
        avcc.extend_from_slice(&(nal_b.len() as u32).to_be_bytes());
        avcc.extend_from_slice(&nal_b);

        let mut annexb = Vec::new();
        push_annexb_nal(&mut annexb, &[0x67, 0xaa]); // SPS
        push_annexb_nal(&mut annexb, &[0x68, 0xbb]); // PPS
        avcc_to_annexb(&avcc, &mut annexb);

        let frame = annexb_to_avcc(&annexb);
        assert_eq!(frame.sps, vec![&[0x67u8, 0xaa][..]]);
        assert_eq!(frame.pps, vec![&[0x68u8, 0xbb][..]]);
        assert!(frame.has_idr);
        assert_eq!(frame.avcc, avcc);
    }
}
