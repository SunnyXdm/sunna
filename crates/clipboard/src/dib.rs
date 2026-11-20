//! Packed Windows DIBs have no BMP file header. Bound allocations before decoding.

use std::io::Cursor;

use crate::{within_limit, MAX_CONTENT_BYTES};

fn u16_at(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        data.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn u32_at(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        data.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn rgba_size(width: usize, height: usize) -> Option<usize> {
    let size = width.checked_mul(height)?.checked_mul(4)?;
    (width != 0 && height != 0 && within_limit(size)).then_some(size)
}

fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(rgba).ok()?;
        writer.finish().ok()?;
    }
    within_limit(bytes.len()).then_some(bytes)
}

/// Accept contiguous, disjoint bitfields and scale each channel to eight bits.
fn valid_masks(masks: &[u32; 4]) -> bool {
    let mut used = 0;
    for (index, &mask) in masks.iter().enumerate() {
        if mask == 0 {
            if index != 3 {
                return false;
            }
            continue;
        }
        let bits = mask >> mask.trailing_zeros();
        if bits & bits.wrapping_add(1) != 0 || used & mask != 0 {
            return false;
        }
        used |= mask;
    }
    true
}

fn channel(pixel: u32, mask: u32) -> u8 {
    if mask == 0 {
        return 255;
    }
    let shift = mask.trailing_zeros();
    let maximum = u64::from(mask >> shift);
    ((u64::from((pixel & mask) >> shift) * 255 + maximum / 2) / maximum) as u8
}

pub(crate) fn dib_to_png(data: &[u8]) -> Option<Vec<u8>> {
    if !within_limit(data.len()) {
        return None;
    }
    let header = u32_at(data, 0)? as usize;
    if !matches!(header, 40 | 108 | 124) || data.len() < header {
        return None;
    }
    let width = u32_at(data, 4)? as i32;
    let signed_height = u32_at(data, 8)? as i32;
    if width <= 0 || u16_at(data, 12)? != 1 {
        return None;
    }
    let (width, height) = (width as usize, signed_height.checked_abs()? as usize);
    let size = rgba_size(width, height)?;
    let bpp = u16_at(data, 14)?;
    let compression = u32_at(data, 16)?;
    if !matches!((bpp, compression), (24 | 32, 0) | (32, 3)) {
        return None;
    }
    let mut offset = header;
    let mut masks = [0x00ff0000, 0x0000ff00, 0x000000ff, 0xff000000];
    if compression == 3 {
        masks = [u32_at(data, 40)?, u32_at(data, 44)?, u32_at(data, 48)?, 0];
        if header == 40 {
            offset += 12;
        } else {
            masks[3] = u32_at(data, 52)?;
        }
        if !valid_masks(&masks) {
            return None;
        }
    }
    // True-color DIBs may still include an optional RGBQUAD color table.
    offset = offset.checked_add((u32_at(data, 32)? as usize).checked_mul(4)?)?;
    let pixel_bytes = usize::from(bpp / 8);
    let stride = width.checked_mul(pixel_bytes)?.checked_add(3)? & !3;
    let pixels = data.get(offset..offset.checked_add(stride.checked_mul(height)?)?)?;
    let mut rgba = vec![0; size];
    for y in 0..height {
        let source_y = if signed_height < 0 { y } else { height - 1 - y };
        let row = &pixels[source_y * stride..(source_y + 1) * stride];
        for x in 0..width {
            let source = &row[x * pixel_bytes..(x + 1) * pixel_bytes];
            let target = &mut rgba[(y * width + x) * 4..(y * width + x + 1) * 4];
            if bpp == 24 {
                target.copy_from_slice(&[source[2], source[1], source[0], 255]);
            } else {
                let pixel = u32::from_le_bytes(source.try_into().ok()?);
                for (out, mask) in target.iter_mut().zip(masks) {
                    *out = channel(pixel, mask);
                }
            }
        }
    }
    // GDI frequently leaves the reserved alpha bytes entirely zero.
    if bpp == 32 && rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 0) {
        for pixel in rgba.as_chunks_mut::<4>().0 {
            pixel[3] = 255;
        }
    }
    encode_png(width as u32, height as u32, &rgba)
}

pub(crate) fn png_to_dib(data: &[u8]) -> Option<Vec<u8>> {
    if !within_limit(data.len()) {
        return None;
    }
    let mut decoder = png::Decoder::new(Cursor::new(data));
    decoder.set_limits(png::Limits {
        bytes: MAX_CONTENT_BYTES,
    });
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let (width, height) = (reader.info().width as usize, reader.info().height as usize);
    let size = rgba_size(width, height)?;
    if !within_limit(size.checked_add(40)?) {
        return None;
    }
    let buffer_size = reader.output_buffer_size()?;
    if !within_limit(buffer_size) {
        return None;
    }
    let mut pixels = vec![0; buffer_size];
    let info = reader.next_frame(&mut pixels).ok()?;
    if info.width as usize != width || info.height as usize != height {
        return None;
    }
    let channels = info.color_type.samples();
    let mut dib = vec![0; 40 + size];
    dib[0..4].copy_from_slice(&40u32.to_le_bytes());
    dib[4..8].copy_from_slice(&(width as i32).to_le_bytes());
    dib[8..12].copy_from_slice(&(height as i32).to_le_bytes());
    dib[12..14].copy_from_slice(&1u16.to_le_bytes());
    dib[14..16].copy_from_slice(&32u16.to_le_bytes());
    dib[20..24].copy_from_slice(&(size as u32).to_le_bytes());
    for y in 0..height {
        for x in 0..width {
            let start = (y * width + x) * channels;
            let p = pixels.get(start..start + channels)?;
            let rgba = match info.color_type {
                png::ColorType::Rgba => [p[0], p[1], p[2], p[3]],
                png::ColorType::Rgb => [p[0], p[1], p[2], 255],
                png::ColorType::GrayscaleAlpha => [p[0], p[0], p[0], p[1]],
                png::ColorType::Grayscale => [p[0], p[0], p[0], 255],
                png::ColorType::Indexed => return None,
            };
            let start = 40 + ((height - 1 - y) * width + x) * 4;
            dib[start..start + 4].copy_from_slice(&[rgba[2], rgba[1], rgba[0], rgba[3]]);
        }
    }
    Some(dib)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(width: i32, height: i32, bpp: u16) -> Vec<u8> {
        let mut dib = vec![0; 40];
        dib[0..4].copy_from_slice(&40u32.to_le_bytes());
        dib[4..8].copy_from_slice(&width.to_le_bytes());
        dib[8..12].copy_from_slice(&height.to_le_bytes());
        dib[12..14].copy_from_slice(&1u16.to_le_bytes());
        dib[14..16].copy_from_slice(&bpp.to_le_bytes());
        dib
    }

    fn decode(data: &[u8]) -> Vec<u8> {
        let mut reader = png::Decoder::new(Cursor::new(data)).read_info().unwrap();
        assert_eq!(reader.info().color_type, png::ColorType::Rgba);
        assert_eq!(reader.info().bit_depth, png::BitDepth::Eight);
        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut pixels).unwrap();
        pixels
    }

    #[test]
    fn roundtrip_preserves_straight_alpha() {
        let rgba = [255, 0, 0, 255, 10, 20, 30, 128, 70, 80, 90, 0, 0, 255, 0, 1];
        let png = encode_png(2, 2, &rgba).unwrap();
        let dib = png_to_dib(&png).unwrap();
        assert_eq!(u32_at(&dib, 20), Some(16));
        assert_eq!(&dib[40..44], &[90, 80, 70, 0]);
        assert_eq!(decode(&dib_to_png(&dib).unwrap()), rgba);
    }

    #[test]
    fn bottom_up_24_bit_with_padding() {
        let mut dib = header(3, 2, 24);
        dib.extend_from_slice(&[90, 80, 70, 120, 110, 100, 150, 140, 130, 9, 9, 9]);
        dib.extend_from_slice(&[0, 0, 255, 0, 255, 0, 255, 0, 0, 9, 9, 9]);
        assert_eq!(
            decode(&dib_to_png(&dib).unwrap()),
            [
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 70, 80, 90, 255, 100, 110, 120,
                255, 130, 140, 150, 255,
            ]
        );
    }

    #[test]
    fn top_down_bitfields_in_each_header() {
        for size in [40, 108, 124] {
            let mut dib = header(1, -2, 32);
            dib.resize(size.max(52), 0);
            dib[0..4].copy_from_slice(&(size as u32).to_le_bytes());
            dib[16..20].copy_from_slice(&3u32.to_le_bytes());
            // Deliberately use RGBA byte order instead of Windows' usual BGRA.
            for (offset, mask) in [(40, 0xffu32), (44, 0xff00), (48, 0xff0000)] {
                dib[offset..offset + 4].copy_from_slice(&mask.to_le_bytes());
            }
            if size > 40 {
                dib[52..56].copy_from_slice(&0xff000000u32.to_le_bytes());
            }
            dib.extend_from_slice(&[10, 20, 30, 128, 40, 50, 60, 255]);
            let alpha = if size == 40 { 255 } else { 128 };
            assert_eq!(
                decode(&dib_to_png(&dib).unwrap()),
                [10, 20, 30, alpha, 40, 50, 60, 255]
            );
        }
    }

    #[test]
    fn zero_alpha_is_opaque() {
        let mut dib = header(2, 1, 32);
        dib.extend_from_slice(&[30, 20, 10, 0, 60, 50, 40, 0]);
        assert_eq!(
            decode(&dib_to_png(&dib).unwrap()),
            [10, 20, 30, 255, 40, 50, 60, 255]
        );
    }

    #[test]
    fn rejects_invalid_or_truncated_data() {
        let mut valid = header(1, 1, 32);
        valid.extend_from_slice(&[1, 2, 3, 255]);
        for length in 0..valid.len() {
            assert!(dib_to_png(&valid[..length]).is_none());
        }
        for data in [
            vec![],
            vec![255; 200],
            header(i32::MAX, i32::MAX, 32),
            header(1, i32::MIN, 32),
            header(0, 1, 32),
            header(-1, 1, 32),
            header(1, 1, 8),
        ] {
            assert!(dib_to_png(&data).is_none());
            assert!(png_to_dib(&data).is_none());
        }
        let png = encode_png(1, 1, &[1, 2, 3, 255]).unwrap();
        assert!(png_to_dib(&png[..png.len() / 2]).is_none());
        valid[16..20].copy_from_slice(&1u32.to_le_bytes());
        assert!(dib_to_png(&valid).is_none());
        assert!(!valid_masks(&[0xff, 0xff, 0xff0000, 0]));
        assert!(!valid_masks(&[0x55, 0xff00, 0xff0000, 0]));
    }
}
