//! A frame: the picture to show, as a grid of character cells. Everything
//! draws into a `Frame`; a backend (the terminal, later a web canvas) puts it
//! on screen.

use crate::color::Rgb;
use crate::geom::Rect;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Cell {
    pub ch: char,
    pub fg: Rgb,
    pub bg: Rgb,
}

impl Default for Cell {
    fn default() -> Self {
        Cell { ch: ' ', fg: Rgb(200, 200, 200), bg: Rgb::BLACK }
    }
}

impl Cell {
    pub const fn new(ch: char, fg: Rgb, bg: Rgb) -> Cell {
        Cell { ch, fg, bg }
    }
}

#[derive(Clone, Debug)]
pub struct Frame {
    w: i32,
    h: i32,
    cells: Vec<Cell>,
}

impl Frame {
    pub fn new(w: u16, h: u16) -> Frame {
        Frame { w: w as i32, h: h as i32, cells: vec![Cell::default(); w as usize * h as usize] }
    }

    pub fn width(&self) -> i32 {
        self.w
    }

    pub fn height(&self) -> i32 {
        self.h
    }

    pub fn rect(&self) -> Rect {
        Rect::new(0, 0, self.w, self.h)
    }

    pub fn resize(&mut self, w: u16, h: u16) {
        self.w = w as i32;
        self.h = h as i32;
        self.cells = vec![Cell::default(); w as usize * h as usize];
    }

    pub fn clear(&mut self, bg: Rgb) {
        self.cells.fill(Cell { ch: ' ', fg: Rgb(200, 200, 200), bg });
    }

    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    #[inline]
    pub fn set(&mut self, x: i32, y: i32, c: Cell) {
        if x >= 0 && y >= 0 && x < self.w && y < self.h {
            self.cells[(y * self.w + x) as usize] = c;
        }
    }

    #[inline]
    pub fn get(&self, x: i32, y: i32) -> Cell {
        if x >= 0 && y >= 0 && x < self.w && y < self.h {
            self.cells[(y * self.w + x) as usize]
        } else {
            Cell::default()
        }
    }

    /// Write text, clipped to `max_w` columns. Returns columns written.
    pub fn text_clip(&mut self, x: i32, y: i32, max_w: i32, s: &str, fg: Rgb, bg: Rgb) -> i32 {
        let mut n = 0;
        for ch in s.chars() {
            if n >= max_w {
                break;
            }
            self.set(x + n, y, Cell { ch, fg, bg });
            n += 1;
        }
        n
    }

    /// Write text to the edge of the frame.
    pub fn text(&mut self, x: i32, y: i32, s: &str, fg: Rgb, bg: Rgb) -> i32 {
        self.text_clip(x, y, self.w - x, s, fg, bg)
    }

    pub fn fill(&mut self, r: Rect, c: Cell) {
        for y in r.y..r.bottom() {
            for x in r.x..r.right() {
                self.set(x, y, c);
            }
        }
    }

    /// A single-line box with an optional title in the top edge.
    pub fn border(&mut self, r: Rect, fg: Rgb, bg: Rgb, title: &str) {
        if r.w < 2 || r.h < 2 {
            return;
        }
        let (x1, y1) = (r.right() - 1, r.bottom() - 1);
        for x in r.x + 1..x1 {
            self.set(x, r.y, Cell::new('─', fg, bg));
            self.set(x, y1, Cell::new('─', fg, bg));
        }
        for y in r.y + 1..y1 {
            self.set(r.x, y, Cell::new('│', fg, bg));
            self.set(x1, y, Cell::new('│', fg, bg));
        }
        self.set(r.x, r.y, Cell::new('┌', fg, bg));
        self.set(x1, r.y, Cell::new('┐', fg, bg));
        self.set(r.x, y1, Cell::new('└', fg, bg));
        self.set(x1, y1, Cell::new('┘', fg, bg));
        if !title.is_empty() {
            self.text_clip(r.x + 2, r.y, r.w - 4, title, fg, bg);
        }
    }

    /// The frame as plain text, one line per row (for tests and snapshots).
    pub fn to_text(&self) -> String {
        let mut s = String::with_capacity(self.cells.len() + self.h as usize);
        for y in 0..self.h {
            for x in 0..self.w {
                s.push(self.get(x, y).ch);
            }
            s.push('\n');
        }
        s
    }

    /// The frame as a coloured HTML page (for looking at frames without a terminal).
    pub fn to_html(&self) -> String {
        let mut s = String::from(
            "<!doctype html><meta charset=utf-8><body style=\"background:#000;margin:0\">\
             <pre style=\"font:14px/1.0 'Cascadia Mono','DejaVu Sans Mono',monospace;margin:8px\">",
        );
        for y in 0..self.h {
            for x in 0..self.w {
                let c = self.get(x, y);
                let ch = match c.ch {
                    '<' => "&lt;".to_string(),
                    '>' => "&gt;".to_string(),
                    '&' => "&amp;".to_string(),
                    ch => ch.to_string(),
                };
                s.push_str(&format!(
                    "<span style=\"color:#{:02x}{:02x}{:02x};background:#{:02x}{:02x}{:02x}\">{}</span>",
                    c.fg.0, c.fg.1, c.fg.2, c.bg.0, c.bg.1, c.bg.2, ch
                ));
            }
            s.push('\n');
        }
        s.push_str("</pre></body>");
        s
    }
}
