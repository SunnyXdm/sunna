//! The host's pointer shape as a cursor for this window (Linux, through
//! winit): straight alpha, resized to the size the picture shows the host's
//! pixels at, so it matches the rest of the screen.

use sunna_proto::messages::CursorShape;
use winit::window::{CustomCursor, CustomCursorSource};

/// `scale`: window pixels per host pixel.
pub fn scaled(shape: &CursorShape, scale: f64) -> Option<CustomCursorSource> {
    let (w, h) = (shape.width as usize, shape.height as usize);
    if w == 0 || h == 0 || shape.rgba.len() < w * h * 4 {
        return None;
    }
    let scale = scale.clamp(0.25, 4.0);
    let (ow, oh) = (((w as f64 * scale).round() as usize).max(1), ((h as f64 * scale).round() as usize).max(1));
    // Bilinear, in premultiplied space (no dark fringes), then unpremultiply.
    let src = &shape.rgba;
    let texel = |x: isize, y: isize, c: usize| -> f64 {
        let x = x.clamp(0, w as isize - 1) as usize;
        let y = y.clamp(0, h as isize - 1) as usize;
        src[(y * w + x) * 4 + c] as f64
    };
    let mut out = vec![0u8; ow * oh * 4];
    for oy in 0..oh {
        for ox in 0..ow {
            let fx = (ox as f64 + 0.5) / scale - 0.5;
            let fy = (oy as f64 + 0.5) / scale - 0.5;
            let (x0, y0) = (fx.floor() as isize, fy.floor() as isize);
            let (ax, ay) = (fx - x0 as f64, fy - y0 as f64);
            let mut px = [0f64; 4];
            for (c, value) in px.iter_mut().enumerate() {
                let top = texel(x0, y0, c) * (1.0 - ax) + texel(x0 + 1, y0, c) * ax;
                let bottom = texel(x0, y0 + 1, c) * (1.0 - ax) + texel(x0 + 1, y0 + 1, c) * ax;
                *value = top * (1.0 - ay) + bottom * ay;
            }
            let alpha = px[3];
            let o = (oy * ow + ox) * 4;
            for c in 0..3 {
                out[o + c] = if alpha > 0.0 { (px[c] * 255.0 / alpha).round().clamp(0.0, 255.0) as u8 } else { 0 };
            }
            out[o + 3] = alpha.round().clamp(0.0, 255.0) as u8;
        }
    }
    let hot = |v: u16, limit: usize| ((v as f64 * scale).round() as usize).min(limit - 1) as u16;
    CustomCursor::from_rgba(out, ow as u16, oh as u16, hot(shape.hot_x, ow), hot(shape.hot_y, oh)).ok()
}
