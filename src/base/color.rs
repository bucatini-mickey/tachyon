//! 24-bit colour and the small amount of colour maths the renderer needs.

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub struct Rgb(pub u8, pub u8, pub u8);

#[inline(always)]
fn c8(v: f32) -> u8 {
    if v <= 0.0 {
        0
    } else if v >= 255.0 {
        255
    } else {
        v as u8
    }
}

impl Rgb {
    pub const BLACK: Rgb = Rgb(0, 0, 0);
    pub const WHITE: Rgb = Rgb(255, 255, 255);

    pub const fn hex(v: u32) -> Rgb {
        Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
    }

    #[inline]
    pub fn scale(self, f: f32) -> Rgb {
        Rgb(c8(self.0 as f32 * f), c8(self.1 as f32 * f), c8(self.2 as f32 * f))
    }

    /// Multiply by a per-channel light level (1.0 = unchanged).
    #[inline]
    pub fn mul3(self, l: [f32; 3]) -> Rgb {
        Rgb(c8(self.0 as f32 * l[0]), c8(self.1 as f32 * l[1]), c8(self.2 as f32 * l[2]))
    }

    #[inline]
    pub fn lerp(self, o: Rgb, t: f32) -> Rgb {
        let t = t.clamp(0.0, 1.0);
        let f = |a: u8, b: u8| c8(a as f32 + (b as f32 - a as f32) * t);
        Rgb(f(self.0, o.0), f(self.1, o.1), f(self.2, o.2))
    }

    #[inline]
    pub fn add(self, o: Rgb) -> Rgb {
        Rgb(self.0.saturating_add(o.0), self.1.saturating_add(o.1), self.2.saturating_add(o.2))
    }

    #[inline]
    pub fn luma(self) -> f32 {
        0.299 * self.0 as f32 + 0.587 * self.1 as f32 + 0.114 * self.2 as f32
    }

    #[inline]
    pub fn max_channel(self) -> u8 {
        self.0.max(self.1).max(self.2)
    }
}

/// A heat-map ramp for overlays: 0.0 = deep blue, 1.0 = white-hot.
pub fn ramp(t: f32) -> Rgb {
    const STOPS: [Rgb; 7] = [
        Rgb(10, 10, 60),
        Rgb(20, 60, 170),
        Rgb(20, 160, 170),
        Rgb(60, 170, 60),
        Rgb(220, 200, 40),
        Rgb(220, 60, 20),
        Rgb(255, 240, 220),
    ];
    let t = t.clamp(0.0, 1.0) * (STOPS.len() - 1) as f32;
    let i = (t as usize).min(STOPS.len() - 2);
    STOPS[i].lerp(STOPS[i + 1], t - i as f32)
}
