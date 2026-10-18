//! Smooth value noise for map generation.

use crate::rng::hash3;

#[inline]
fn lattice(seed: u32, x: i32, y: i32) -> f32 {
    (hash3(x, y, 0, seed) & 0xFFFF) as f32 / 65535.0
}

/// Smooth noise in `[0, 1]` with features about one unit across.
pub fn value2(seed: u32, x: f32, y: f32) -> f32 {
    let (xf, yf) = (x.floor(), y.floor());
    let (ix, iy) = (xf as i32, yf as i32);
    let (fx, fy) = (x - xf, y - yf);
    let sx = fx * fx * (3.0 - 2.0 * fx);
    let sy = fy * fy * (3.0 - 2.0 * fy);
    let a = lattice(seed, ix, iy);
    let b = lattice(seed, ix + 1, iy);
    let c = lattice(seed, ix, iy + 1);
    let d = lattice(seed, ix + 1, iy + 1);
    let top = a + (b - a) * sx;
    let bot = c + (d - c) * sx;
    top + (bot - top) * sy
}

/// Fractal (layered) value noise in `[0, 1]`.
pub fn fbm2(seed: u32, x: f32, y: f32, octaves: u32) -> f32 {
    let (mut sum, mut amp, mut freq, mut norm) = (0.0, 0.5, 1.0, 0.0);
    for o in 0..octaves {
        sum += value2(seed.wrapping_add(o * 101), x * freq, y * freq) * amp;
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / norm
}
