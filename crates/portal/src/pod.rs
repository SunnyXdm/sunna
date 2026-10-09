//! The small subset of SPA PODs needed for raw video negotiation.
use anyhow::{bail, ensure};

pub const ID: u32 = 3;
pub const INT: u32 = 4;
pub const RECTANGLE: u32 = 10;
pub const FRACTION: u32 = 11;
pub const CHOICE: u32 = 19;
pub const FORMAT: u32 = 0x40003;
pub const BUFFERS: u32 = 0x40004;
pub const META: u32 = 0x40005;
pub const VIDEO_FORMAT: u32 = 0x20001;
pub const VIDEO_SIZE: u32 = 0x20003;
pub const BGRX: u32 = 8;
pub const BGRA: u32 = 12;
pub const RGBX: u32 = 7;
pub const RGBA: u32 = 11;

// u64 storage keeps every POD aligned for the C API.
#[derive(Debug)]
pub struct Pod(Vec<u64>);
impl Pod {
    fn from_bytes(mut bytes: Vec<u8>) -> Self {
        bytes.resize((bytes.len() + 7) & !7, 0);
        Self(
            bytes
                .as_chunks::<8>()
                .0
                .iter()
                .map(|b| u64::from_ne_bytes(*b))
                .collect(),
        )
    }
    pub fn as_ptr(&self) -> *const std::ffi::c_void {
        self.0.as_ptr().cast()
    }
    pub fn bytes(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.0.as_ptr().cast(), self.0.len() * 8) }
    }
}
fn words(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_ne_bytes()).collect()
}
pub fn value(kind: u32, values: &[u32]) -> Pod {
    let mut bytes = words(&[values.len() as u32 * 4, kind]);
    bytes.extend(words(values));
    Pod::from_bytes(bytes)
}
pub fn choice(kind: u32, child: u32, values: &[&[u32]]) -> Pod {
    assert!(!values.is_empty() && values.iter().all(|v| v.len() == values[0].len()));
    let size = values[0].len() as u32 * 4;
    let mut bytes = words(&[16 + size * values.len() as u32, 19, kind, 0, size, child]);
    for v in values {
        bytes.extend(words(v));
    }
    Pod::from_bytes(bytes)
}
pub fn object(kind: u32, id: u32, props: &[(u32, Pod)]) -> Pod {
    let mut bytes = words(&[0, 15, kind, id]);
    for (key, val) in props {
        bytes.extend(words(&[*key, 0]));
        bytes.extend(val.bytes());
    }
    let size = bytes.len() as u32 - 8;
    bytes[..4].copy_from_slice(&size.to_ne_bytes());
    Pod::from_bytes(bytes)
}
pub fn enum_format(fps: u32) -> Pod {
    object(
        FORMAT,
        3,
        &[
            (1, value(ID, &[2])),
            (2, value(ID, &[1])),
            (
                VIDEO_FORMAT,
                choice(3, ID, &[&[BGRX], &[BGRX], &[BGRA], &[RGBX], &[RGBA]]),
            ),
            (
                VIDEO_SIZE,
                choice(1, RECTANGLE, &[&[1920, 1080], &[1, 1], &[16384, 16384]]),
            ),
            (0x20004, value(FRACTION, &[0, 1])),
            (
                0x20005,
                choice(1, FRACTION, &[&[fps.max(1), 1], &[0, 1], &[fps.max(1), 1]]),
            ),
        ],
    )
}
pub fn buffer_params() -> Vec<Pod> {
    let meta = |kind, size| {
        object(
            META,
            6,
            &[(1, value(ID, &[kind])), (2, value(INT, &[size]))],
        )
    };
    vec![
        object(BUFFERS, 5, &[(6, choice(4, INT, &[&[6]]))]), // MemPtr | MemFd
        meta(1, 32),                                         // spa_meta_header
        meta(3, 16 * 16),                                    // sixteen damage rectangles
        meta(5, 28 + 20 + 256 * 256 * 4),
    ]
}
fn word(bytes: &[u8], offset: usize) -> anyhow::Result<u32> {
    Ok(u32::from_ne_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or_else(|| anyhow::anyhow!("truncated SPA POD"))?
            .try_into()?,
    ))
}
pub fn parse_format(bytes: &[u8]) -> anyhow::Result<(u32, u32, u32)> {
    let end = word(bytes, 0)? as usize + 8;
    ensure!(
        end <= bytes.len() && word(bytes, 4)? == 15 && word(bytes, 8)? == FORMAT,
        "invalid SPA format object"
    );
    let (mut format, mut size, mut media, mut subtype) = (None, None, None, None);
    let mut at = 16;
    while at < end {
        let key = word(bytes, at)?;
        let len = word(bytes, at + 8)? as usize;
        let start = at + 16;
        ensure!(start + len <= end, "truncated SPA property");
        // A negotiated value often comes as a choice of one (None), as
        // PipeWire's own parser accepts: its first value is the value.
        let (kind, value_len, value) = match word(bytes, at + 12)? {
            CHOICE => {
                ensure!(len >= 16, "truncated SPA choice");
                (
                    word(bytes, start + 12)?,
                    word(bytes, start + 8)? as usize,
                    start + 16,
                )
            }
            kind => (kind, len, start),
        };
        ensure!(value + value_len <= end, "truncated SPA property");
        let (len, start) = (value_len, value);
        match (key, kind, len) {
            (1, ID, 4) => media = Some(word(bytes, start)?),
            (2, ID, 4) => subtype = Some(word(bytes, start)?),
            (VIDEO_FORMAT, ID, 4) => format = Some(word(bytes, start)?),
            (VIDEO_SIZE, RECTANGLE, 8) => {
                size = Some((word(bytes, start)?, word(bytes, start + 4)?))
            }
            _ => {}
        }
        at = (at + 16 + word(bytes, at + 8)? as usize + 7) & !7;
    }
    ensure!(
        media == Some(2) && subtype == Some(1),
        "PipeWire did not negotiate raw video"
    );
    match (format, size) {
        (Some(f @ (BGRX | BGRA | RGBX | RGBA)), Some((w, h)))
            if w > 0 && h > 0 && w <= 16384 && h <= 16384 =>
        {
            Ok((f, w, h))
        }
        _ => bail!("unsupported PipeWire video format or size"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scalar_padding_and_object_bytes() {
        assert_eq!(value(ID, &[8]).bytes(), words(&[4, 3, 8, 0]));
        let p = object(FORMAT, 3, &[(1, value(ID, &[2])), (2, value(ID, &[1]))]);
        assert_eq!(
            p.bytes(),
            words(&[56, 15, FORMAT, 3, 1, 0, 4, 3, 2, 0, 2, 0, 4, 3, 1, 0])
        );
        assert_eq!(p.as_ptr() as usize % 8, 0);
    }
    #[test]
    fn choices_have_one_child_header_and_unpadded_values() {
        assert_eq!(
            choice(3, ID, &[&[8], &[8], &[12]]).bytes(),
            words(&[28, 19, 3, 0, 4, 3, 8, 8, 12, 0])
        );
        assert_eq!(
            choice(1, RECTANGLE, &[&[1920, 1080], &[1, 1], &[4096, 4096]]).bytes(),
            words(&[40, 19, 1, 0, 8, 10, 1920, 1080, 1, 1, 4096, 4096])
        );
    }
    #[test]
    fn buffer_flags_match_the_c_builder() {
        assert_eq!(
            buffer_params()[0].bytes(),
            words(&[48, 15, BUFFERS, 5, 6, 0, 20, 19, 4, 0, 4, INT, 6, 0])
        );
    }
    #[test]
    fn negotiated_format_and_truncation() {
        let p = object(
            FORMAT,
            4,
            &[
                (1, value(ID, &[2])),
                (2, value(ID, &[1])),
                (VIDEO_FORMAT, value(ID, &[BGRA])),
                (VIDEO_SIZE, value(RECTANGLE, &[3840, 2160])),
            ],
        );
        assert_eq!(parse_format(p.bytes()).unwrap(), (BGRA, 3840, 2160));
        for len in 0..p.bytes().len() {
            assert!(parse_format(&p.bytes()[..len]).is_err());
        }
    }
    #[test]
    fn negotiated_values_wrapped_in_a_choice_of_one() {
        let p = object(
            FORMAT,
            4,
            &[
                (1, choice(0, ID, &[&[2]])),
                (2, choice(0, ID, &[&[1]])),
                (VIDEO_FORMAT, choice(0, ID, &[&[BGRX]])),
                (VIDEO_SIZE, choice(0, RECTANGLE, &[&[1920, 1080]])),
            ],
        );
        assert_eq!(parse_format(p.bytes()).unwrap(), (BGRX, 1920, 1080));
    }
}
