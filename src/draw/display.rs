//! How a frame reaches the screen: which characters and which colours.
//!
//! Games draw with whatever glyphs and 24-bit colours they like. On the way
//! out, the backend passes every cell through a [`Display`]:
//!
//! - **Charset.** `Ascii` (the default) folds every glyph to 7-bit ASCII, so
//!   the game is a true terminal game in any font, and no glyph can ever be
//!   drawn two columns wide. `Extended` sends box-drawing and CP437 glyphs
//!   as they are.
//! - **Colour scheme.** `Natural` keeps the game's colours. `Terminal` uses only the
//!   terminal's own 16 colours and its default background, so the game takes
//!   on whatever colour scheme the terminal has. `Phosphor`, `Amber` and
//!   `Paper` recolour everything from its brightness, like old monitors and
//!   printouts.
//!
//! Without 24-bit colour (no `COLORTERM=truecolor`, not Windows Terminal),
//! colours are sent as the nearest of the 256 xterm colours.
//!
//! Set from the environment by [`Display::from_env`]: `TACHYON_SCHEME`
//! (natural, terminal, phosphor, amber, paper) and `TACHYON_CHARSET` (ascii,
//! extended). In any game, Ctrl+T cycles the colour scheme and Ctrl+G the charset.

use crate::color::Rgb;
use crate::frame::{Cell, Frame};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Charset {
    #[default]
    Ascii,
    Extended,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Scheme {
    #[default]
    Natural,
    Terminal,
    Phosphor,
    Amber,
    Paper,
}

impl Scheme {
    pub const ALL: [Scheme; 5] = [Scheme::Natural, Scheme::Terminal, Scheme::Phosphor, Scheme::Amber, Scheme::Paper];

    pub fn name(self) -> &'static str {
        match self {
            Scheme::Natural => "natural",
            Scheme::Terminal => "terminal",
            Scheme::Phosphor => "phosphor",
            Scheme::Amber => "amber",
            Scheme::Paper => "paper",
        }
    }

    pub fn parse(s: &str) -> Option<Scheme> {
        Scheme::ALL.into_iter().find(|t| t.name().eq_ignore_ascii_case(s.trim()))
    }

    pub fn next(self) -> Scheme {
        Scheme::ALL[(Scheme::ALL.iter().position(|&t| t == self).unwrap_or(0) + 1) % Scheme::ALL.len()]
    }

    /// (dark, bright) ends of a monochrome scheme's ramp.
    fn ramp(self) -> Option<(Rgb, Rgb)> {
        match self {
            Scheme::Phosphor => Some((Rgb(2, 10, 4), Rgb(140, 255, 160))),
            Scheme::Amber => Some((Rgb(12, 6, 0), Rgb(255, 186, 72))),
            // Inverted: dark becomes the page, bright becomes the ink.
            Scheme::Paper => Some((Rgb(240, 232, 212), Rgb(30, 24, 20))),
            _ => None,
        }
    }
}

/// A colour as it goes to the terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ink {
    /// The terminal's own default foreground or background.
    Default,
    /// One of the terminal's 16 colours (0-7 normal, 8-15 bright).
    Ansi(u8),
    /// One of the 256 xterm colours.
    Xterm(u8),
    Rgb(Rgb),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Display {
    pub charset: Charset,
    pub scheme: Scheme,
    /// The terminal takes 24-bit colour; otherwise colours go out as xterm-256.
    pub truecolor: bool,
}

impl Default for Display {
    fn default() -> Self {
        Display { charset: Charset::Ascii, scheme: Scheme::Natural, truecolor: true }
    }
}

impl Display {
    /// Settings from `TACHYON_SCHEME`, `TACHYON_CHARSET` and the terminal's
    /// colour support (`COLORTERM`, or Windows Terminal's `WT_SESSION`).
    pub fn from_env() -> Display {
        let var = |k: &str| std::env::var(k).unwrap_or_default();
        let ct = var("COLORTERM").to_ascii_lowercase();
        Display {
            charset: if var("TACHYON_CHARSET").eq_ignore_ascii_case("extended") { Charset::Extended } else { Charset::Ascii },
            scheme: Scheme::parse(&var("TACHYON_SCHEME")).unwrap_or_default(),
            truecolor: ct.contains("truecolor") || ct.contains("24bit") || !var("WT_SESSION").is_empty() || cfg!(windows),
        }
    }

    #[inline]
    pub fn glyph(&self, ch: char) -> char {
        if self.charset == Charset::Ascii { ascii(ch) } else { ch }
    }

    /// The ink for a foreground (`bg` false) or background colour.
    #[inline]
    pub fn ink(&self, c: Rgb, bg: bool) -> Ink {
        match self.scheme {
            Scheme::Natural => {
                if self.truecolor {
                    Ink::Rgb(c)
                } else {
                    Ink::Xterm(xterm256(c))
                }
            }
            Scheme::Terminal => ansi16(c, bg),
            t => {
                let (dark, bright) = t.ramp().unwrap();
                // Lift the darks so dim things stay readable; backgrounds stay low.
                let l = c.luma() / 255.0;
                let v = if bg { l * 0.35 } else { 0.18 + 0.82 * l.powf(0.6) };
                let out = dark.lerp(bright, v);
                if self.truecolor { Ink::Rgb(out) } else { Ink::Xterm(xterm256(out)) }
            }
        }
    }

    /// What a cell looks like on screen, as 24-bit colour (for snapshots).
    pub fn cell(&self, c: Cell) -> Cell {
        let rgb = |i: Ink, bg: bool| match i {
            Ink::Default => if bg { Rgb::BLACK } else { ANSI[7] },
            Ink::Ansi(n) => ANSI[n as usize],
            Ink::Xterm(n) => xterm_rgb(n),
            Ink::Rgb(c) => c,
        };
        Cell { ch: self.glyph(c.ch), fg: rgb(self.ink(c.fg, false), false), bg: rgb(self.ink(c.bg, true), true) }
    }

    /// Rewrite a whole frame as it would appear (for snapshots and tests).
    pub fn apply(&self, f: &mut Frame) {
        let (w, h) = (f.width(), f.height());
        for y in 0..h {
            for x in 0..w {
                let c = self.cell(f.get(x, y));
                f.set(x, y, c);
            }
        }
    }

    /// Append the escape code that sets this ink.
    pub fn push_sgr(buf: &mut Vec<u8>, ink: Ink, bg: bool) {
        match ink {
            Ink::Default => buf.extend_from_slice(if bg { b"\x1b[49m" } else { b"\x1b[39m" }),
            Ink::Ansi(n) => {
                let base = match (bg, n >= 8) {
                    (false, false) => 30,
                    (false, true) => 90,
                    (true, false) => 40,
                    (true, true) => 100,
                };
                buf.extend_from_slice(b"\x1b[");
                push_num(buf, base + (n & 7) as u32);
                buf.push(b'm');
            }
            Ink::Xterm(n) => {
                buf.extend_from_slice(if bg { b"\x1b[48;5;" } else { b"\x1b[38;5;" });
                push_num(buf, n as u32);
                buf.push(b'm');
            }
            Ink::Rgb(c) => {
                buf.extend_from_slice(if bg { b"\x1b[48;2;" } else { b"\x1b[38;2;" });
                push_num(buf, c.0 as u32);
                buf.push(b';');
                push_num(buf, c.1 as u32);
                buf.push(b';');
                push_num(buf, c.2 as u32);
                buf.push(b'm');
            }
        }
    }
}

#[inline]
pub(crate) fn push_num(buf: &mut Vec<u8>, mut n: u32) {
    let mut tmp = [0u8; 10];
    let mut i = tmp.len();
    loop {
        i -= 1;
        tmp[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    buf.extend_from_slice(&tmp[i..]);
}

/// Fold a glyph to 7-bit ASCII. Box drawing becomes `+ - | =`, shading
/// becomes `: % #`, and the CP437 picture glyphs become their nearest
/// letter or sign. Anything unknown becomes `?`, so it shows up in testing.
pub fn ascii(ch: char) -> char {
    if ch.is_ascii() {
        return ch;
    }
    match ch {
        // Single lines, light and heavy.
        '─' | '━' | '┄' | '┅' | '┈' | '┉' | '╌' | '╍' | '╴' | '╶' | '╸' | '╺' | '╼' | '╾' => '-',
        '│' | '┃' | '┆' | '┇' | '┊' | '┋' | '╎' | '╏' | '╵' | '╷' | '╹' | '╻' | '╽' | '╿' => '|',
        // Double lines: horizontals as '=', verticals as '|'.
        '═' => '=',
        '║' => '|',
        // Every corner and junction, single or double.
        '┌'..='╋' | '╒'..='╬' | '╭'..='╰' => '+',
        '╱' => '/',
        '╲' => '\\',
        '╳' => 'X',
        // Shading and blocks.
        '░' => ':',
        '▒' => '%',
        '▓' | '█' | '■' | '▀' | '▄' | '▌' | '▐' => '#',
        '▬' | '≡' => '=',
        '≈' | '∼' => '~',
        '·' | '∙' | '•' | '°' | '˚' => '.',
        '▲' | '△' | '↑' => '^',
        '▼' | '▽' | '↓' => 'v',
        '◄' | '←' => '<',
        '►' | '→' => '>',
        '♣' | '♠' => '&',
        '☼' | '✶' | '✱' => '*',
        '○' | '◦' | '◉' | '◘' | '◙' | 'Θ' | 'θ' => 'o',
        'π' | '∩' => 'n',
        'Ω' => 'U',
        'Σ' => 'E',
        'ψ' | 'Ψ' => 'Y',
        '†' | '✝' => '+',
        '✓' | '✔' => 'v',
        '✗' | '✘' | '×' => 'x',
        '¡' => '!',
        '¿' => '?',
        '¶' => 'P',
        '§' => 'S',
        '¬' => '-',
        '«' => '<',
        '»' => '>',
        '—' | '–' => '-',
        '‘' | '’' => '\'',
        '“' | '”' => '"',
        '…' => '.',
        'Æ' => 'A',
        'æ' => 'a',
        'À'..='Å' => 'A',
        'à'..='å' => 'a',
        'Ç' => 'C',
        'ç' => 'c',
        'È'..='Ë' => 'E',
        'è'..='ë' => 'e',
        'Ì'..='Ï' => 'I',
        'ì'..='ï' => 'i',
        'Ñ' => 'N',
        'ñ' => 'n',
        'Ò'..='Ö' | 'Ø' => 'O',
        'ò'..='ö' | 'ø' => 'o',
        'Ù'..='Ü' => 'U',
        'ù'..='ü' => 'u',
        'Ý' => 'Y',
        'ý' | 'ÿ' => 'y',
        'ŝ' => 's',
        'ĝ' => 'g',
        _ => '?',
    }
}

/// Reference colours for the 16 terminal colours (Windows Terminal's Campbell),
/// used only to picture `Terminal` output in snapshots.
const ANSI: [Rgb; 16] = [
    Rgb(12, 12, 12),
    Rgb(197, 15, 31),
    Rgb(19, 161, 14),
    Rgb(193, 156, 0),
    Rgb(0, 55, 218),
    Rgb(136, 23, 152),
    Rgb(58, 150, 221),
    Rgb(204, 204, 204),
    Rgb(118, 118, 118),
    Rgb(231, 72, 86),
    Rgb(22, 198, 12),
    Rgb(249, 241, 165),
    Rgb(59, 120, 255),
    Rgb(180, 0, 158),
    Rgb(97, 214, 214),
    Rgb(242, 242, 242),
];

/// The terminal colour for an RGB colour, chosen by hue and brightness rather
/// than nearest distance, so dim colours keep their hue instead of all
/// turning black. Dark backgrounds become the terminal's own background.
fn ansi16(c: Rgb, bg: bool) -> Ink {
    let (r, g, b) = (c.0 as f32, c.1 as f32, c.2 as f32);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let luma = c.luma();
    if bg && luma < 120.0 {
        return Ink::Default;
    }
    if !bg && luma < 6.0 {
        return Ink::Ansi(0);
    }
    let sat = if max > 0.0 { (max - min) / max } else { 0.0 };
    if sat < 0.28 {
        // Greys: black, dark grey, light grey, white.
        return Ink::Ansi(if luma > 200.0 { 15 } else if luma > 70.0 { 7 } else { 8 });
    }
    // A colour is bright by its strongest channel (luma undersells red and blue).
    let bright = if bg { max > 200.0 } else { max > 165.0 };
    // Hue in sixths: red, yellow, green, cyan, blue, magenta.
    let d = max - min;
    let h = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    // ANSI order is black red green yellow blue magenta cyan white.
    const BY_HUE: [u8; 6] = [1, 3, 2, 6, 4, 5];
    let base = BY_HUE[(h + 0.5) as usize % 6];
    Ink::Ansi(if bright { base + 8 } else { base })
}

/// The nearest xterm-256 colour: the 6x6x6 cube or the 24-step grey ramp.
pub fn xterm256(c: Rgb) -> u8 {
    const LEVELS: [i32; 6] = [0, 95, 135, 175, 215, 255];
    let step = |v: u8| -> usize {
        let v = v as i32;
        (0..6).min_by_key(|&i| (LEVELS[i] - v).abs()).unwrap()
    };
    let (ri, gi, bi) = (step(c.0), step(c.1), step(c.2));
    let cube = Rgb(LEVELS[ri] as u8, LEVELS[gi] as u8, LEVELS[bi] as u8);
    let avg = (c.0 as i32 + c.1 as i32 + c.2 as i32) / 3;
    let gi2 = ((avg - 8).max(0) / 10).min(23);
    let grey = (8 + gi2 * 10) as u8;
    let dist = |a: Rgb| {
        let (dr, dg, db) = (a.0 as i32 - c.0 as i32, a.1 as i32 - c.1 as i32, a.2 as i32 - c.2 as i32);
        dr * dr + dg * dg + db * db
    };
    if dist(Rgb(grey, grey, grey)) < dist(cube) {
        232 + gi2 as u8
    } else {
        16 + (36 * ri + 6 * gi + bi) as u8
    }
}

fn xterm_rgb(n: u8) -> Rgb {
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    match n {
        0..=15 => ANSI[n as usize],
        16..=231 => {
            let i = n - 16;
            Rgb(LEVELS[(i / 36) as usize], LEVELS[(i / 6 % 6) as usize], LEVELS[(i % 6) as usize])
        }
        _ => {
            let g = 8 + (n - 232) * 10;
            Rgb(g, g, g)
        }
    }
}
