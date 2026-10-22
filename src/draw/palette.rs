//! The engine's fixed palette: 16 inks. The default `Display` scheme snaps
//! every colour a game draws to the nearest of these, so a game can never
//! show more than 16 foregrounds and 16 backgrounds. The web terminal
//! (`web/pterm.ts`) uses the same table, so the two look alike.

use crate::color::Rgb;

pub const INK: u8 = 0;
pub const DEEP: u8 = 1;
pub const SLATE: u8 = 2;
pub const GREY: u8 = 3;
pub const SILVER: u8 = 4;
pub const CREAM: u8 = 5;
pub const GOLD: u8 = 6;
pub const ORANGE: u8 = 7;
pub const RUBRIC: u8 = 8;
pub const BLOOD: u8 = 9;
pub const GREEN: u8 = 10;
pub const MOSS: u8 = 11;
pub const BLUE: u8 = 12;
pub const NAVY: u8 = 13;
pub const VIOLET: u8 = 14;
pub const BROWN: u8 = 15;

pub const PALETTE: [Rgb; 16] = [
    Rgb(11, 13, 18),
    Rgb(38, 44, 58),
    Rgb(70, 86, 110),
    Rgb(126, 132, 146),
    Rgb(206, 206, 212),
    Rgb(255, 240, 190),
    Rgb(232, 196, 96),
    Rgb(230, 140, 60),
    Rgb(232, 96, 80),
    Rgb(130, 40, 34),
    Rgb(140, 200, 120),
    Rgb(46, 96, 56),
    Rgb(96, 150, 230),
    Rgb(36, 56, 116),
    Rgb(160, 100, 190),
    Rgb(110, 76, 44),
];

/// Index of the palette ink nearest to `c` (squared distance, green-weighted).
pub fn nearest(c: Rgb) -> usize {
    let d = |p: Rgb| {
        let (r, g, b) = (p.0 as i32 - c.0 as i32, p.1 as i32 - c.1 as i32, p.2 as i32 - c.2 as i32);
        2 * r * r + 4 * g * g + 3 * b * b
    };
    (0..16).min_by_key(|&i| d(PALETTE[i])).unwrap()
}
