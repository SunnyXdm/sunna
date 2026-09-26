/// BGRA → I420, BT.709 matrix, video (limited) range, 2×2 averaged chroma.
/// Split across cores by row bands: at 1080p this is a few milliseconds
/// instead of the ~15 ms a per-pixel float conversion takes.
/// `pitch` is the Y stride; U and V use half that stride. Padding is untouched.
pub fn bgra_to_i420(
    src: &[u8],
    width: usize,
    height: usize,
    pitch: usize,
    y: &mut [u8],
    u: &mut [u8],
    v: &mut [u8],
) {
    assert!(width > 0 && height > 0 && width.is_multiple_of(2) && height.is_multiple_of(2));
    assert!(pitch >= width && pitch.is_multiple_of(2));
    assert!(src.len() >= width * height * 4);
    assert!(y.len() >= pitch * height);
    assert!(u.len() >= pitch / 2 * (height / 2) && v.len() >= pitch / 2 * (height / 2));
    let threads = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .min(8);
    let band_rows = (height / threads).max(2) & !1;
    std::thread::scope(|scope| {
        let bands = src[..width * height * 4]
            .chunks(width * 4 * band_rows)
            .zip(y[..pitch * height].chunks_mut(pitch * band_rows))
            .zip(u[..pitch / 2 * (height / 2)].chunks_mut(pitch / 2 * (band_rows / 2)))
            .zip(v[..pitch / 2 * (height / 2)].chunks_mut(pitch / 2 * (band_rows / 2)));
        for (((src, y), u), v) in bands {
            scope.spawn(move || convert_band(src, width, pitch, y, u, v));
        }
    });
}

fn convert_band(src: &[u8], width: usize, pitch: usize, y: &mut [u8], u: &mut [u8], v: &mut [u8]) {
    // 16.16 fixed point; see BT.709: Kr 0.2126, Kb 0.0722, scaled to 16..235
    // (luma) and 16..240 (chroma).
    const Y: [i32; 3] = [11966, 40254, 4064];
    const U: [i32; 3] = [-6596, -22188, 28784];
    const V: [i32; 3] = [28784, -26145, -2639];
    let luma = |r: i32, g: i32, b: i32| ((Y[0] * r + Y[1] * g + Y[2] * b + 32768) >> 16) + 16;
    let rows = y.len() / pitch;
    for pair in 0..rows / 2 {
        let top = &src[pair * 2 * width * 4..][..width * 4];
        let bottom = &src[(pair * 2 + 1) * width * 4..][..width * 4];
        for x in 0..width / 2 {
            let mut sum = [0i32; 3];
            for (row, line) in [top, bottom].into_iter().enumerate() {
                for dx in 0..2 {
                    let px = &line[(x * 2 + dx) * 4..][..3];
                    let (b, g, r) = (px[0] as i32, px[1] as i32, px[2] as i32);
                    y[(pair * 2 + row) * pitch + x * 2 + dx] = luma(r, g, b) as u8;
                    sum[0] += r;
                    sum[1] += g;
                    sum[2] += b;
                }
            }
            // Sums of four pixels: shift by 2 more bits to average.
            let cb = ((U[0] * sum[0] + U[1] * sum[1] + U[2] * sum[2] + (1 << 17)) >> 18) + 128;
            let cr = ((V[0] * sum[0] + V[1] * sum[1] + V[2] * sum[2] + (1 << 17)) >> 18) + 128;
            u[pair * (pitch / 2) + x] = cb.clamp(16, 240) as u8;
            v[pair * (pitch / 2) + x] = cr.clamp(16, 240) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn convert(bgra: [u8; 4]) -> (u8, u8, u8) {
        let src: Vec<u8> = bgra.repeat(4);
        let (mut y, mut u, mut v) = ([0u8; 4], [0u8; 1], [0u8; 1]);
        bgra_to_i420(&src, 2, 2, 2, &mut y, &mut u, &mut v);
        (y[0], u[0], v[0])
    }

    #[test]
    fn bt709_video_range() {
        assert_eq!(convert([0, 0, 0, 255]), (16, 128, 128));
        assert_eq!(convert([255, 255, 255, 255]), (235, 128, 128));
        // Pure red, BT.709: Y 63, Cb 102, Cr 240.
        assert_eq!(convert([0, 0, 255, 255]), (63, 102, 240));
    }

    #[test]
    fn padded_planes_match_tight_planes() {
        let (width, height, pitch) = (18, 38, 32);
        let src: Vec<u8> = (0..width * height * 4).map(|i| (i * 37) as u8).collect();
        let (mut y, mut u, mut v) = (
            vec![0; width * height],
            vec![0; width * height / 4],
            vec![0; width * height / 4],
        );
        bgra_to_i420(&src, width, height, width, &mut y, &mut u, &mut v);
        let mut padded = vec![0xa5; pitch * height * 3 / 2];
        let (py, uv) = padded.split_at_mut(pitch * height);
        let (pu, pv) = uv.split_at_mut(pitch * height / 4);
        bgra_to_i420(&src, width, height, pitch, py, pu, pv);
        for (tight, padded, width, pitch) in [
            (&y[..], &py[..], width, pitch),
            (&u[..], &pu[..], width / 2, pitch / 2),
            (&v[..], &pv[..], width / 2, pitch / 2),
        ] {
            for (expected, actual) in tight.chunks_exact(width).zip(padded.chunks_exact(pitch)) {
                assert_eq!(expected, &actual[..width]);
                assert!(actual[width..].iter().all(|&byte| byte == 0xa5));
            }
        }
    }
}
