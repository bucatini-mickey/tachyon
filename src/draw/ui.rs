//! A small toolkit for the screens around the map: titled panels, wrapped
//! text, a scrolling log, menus, tables with a selection, tabs and a key
//! bar. Everything draws into a [`Frame`] in the same character cells as
//! the map, so a game's interface looks like part of its world rather than
//! a separate program. Widgets hold only their own state (a selection, a
//! scroll position); the game owns its data and passes it in to draw.

use crate::color::Rgb;
use crate::frame::{Cell, Frame};
use crate::geom::Rect;
use crate::input::{Key, KeyEvent};

/// The colours a game's screens are drawn in.
#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub bg: Rgb,
    pub frame: Rgb,
    pub title: Rgb,
    pub text: Rgb,
    pub dim: Rgb,
    pub pick_bg: Rgb,
    pub pick_fg: Rgb,
    pub good: Rgb,
    pub warn: Rgb,
    pub bad: Rgb,
}

impl Default for Theme {
    fn default() -> Self {
        use crate::palette::*;
        Theme::slots(INK, SLATE, CREAM, SILVER, GREY, DEEP, CREAM, GREEN, GOLD, RUBRIC)
    }
}

impl Theme {
    /// A theme from palette slots, in field order.
    #[allow(clippy::too_many_arguments)]
    pub fn slots(bg: u8, frame: u8, title: u8, text: u8, dim: u8, pick_bg: u8, pick_fg: u8, good: u8, warn: u8, bad: u8) -> Theme {
        let c = |i: u8| crate::palette::PALETTE[i as usize % 16];
        Theme { bg: c(bg), frame: c(frame), title: c(title), text: c(text), dim: c(dim), pick_bg: c(pick_bg), pick_fg: c(pick_fg), good: c(good), warn: c(warn), bad: c(bad) }
    }
}

/// Clear `r`, draw a border with `title`, and return the inside.
pub fn panel(f: &mut Frame, r: Rect, title: &str, t: &Theme) -> Rect {
    f.fill(r, Cell::new(' ', t.text, t.bg));
    if r.w < 3 || r.h < 3 {
        return r;
    }
    f.border(r, t.frame, t.bg, "");
    if !title.is_empty() {
        let s = format!(" {title} ");
        f.text_clip(r.x + 2, r.y, r.w - 4, &s, t.title, t.bg);
    }
    r.inset(1)
}

/// Break text into lines no wider than `width`, at spaces where it can.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut out = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        let mut len = 0usize;
        for word in para.split(' ') {
            let wl = word.chars().count();
            if len > 0 && len + 1 + wl > width {
                out.push(std::mem::take(&mut line));
                len = 0;
            }
            if wl > width {
                // A word longer than the line: cut it.
                let chars: Vec<char> = word.chars().collect();
                for chunk in chars.chunks(width) {
                    if len > 0 {
                        out.push(std::mem::take(&mut line));
                    }
                    line = chunk.iter().collect();
                    len = chunk.len();
                }
                continue;
            }
            if len > 0 {
                line.push(' ');
                len += 1;
            }
            line.push_str(word);
            len += wl;
        }
        out.push(line);
    }
    out
}

/// Draw wrapped text from the top of `r`; returns the rows used.
pub fn paragraph(f: &mut Frame, r: Rect, text: &str, fg: Rgb, bg: Rgb) -> i32 {
    let lines = wrap(text, r.w.max(1) as usize);
    for (k, l) in lines.iter().enumerate().take(r.h.max(0) as usize) {
        f.text_clip(r.x, r.y + k as i32, r.w, l, fg, bg);
    }
    lines.len().min(r.h.max(0) as usize) as i32
}

/// A running log of coloured lines; drawn newest at the bottom, wrapped,
/// and scrollable back through its history.
#[derive(Clone, Debug, Default)]
pub struct Log {
    pub lines: Vec<(String, Rgb)>,
    /// Lines scrolled back from the newest (0 = following the end).
    pub back: usize,
    pub cap: usize,
}

impl Log {
    pub fn new(cap: usize) -> Log {
        Log { lines: Vec::new(), back: 0, cap }
    }

    pub fn push(&mut self, s: impl Into<String>, c: Rgb) {
        self.lines.push((s.into(), c));
        if self.cap > 0 && self.lines.len() > self.cap {
            let extra = self.lines.len() - self.cap;
            self.lines.drain(..extra);
        }
    }

    pub fn scroll(&mut self, by: i32) {
        self.back = (self.back as i32 + by).clamp(0, self.lines.len().saturating_sub(1) as i32) as usize;
    }

    pub fn draw(&self, f: &mut Frame, r: Rect, bg: Rgb) {
        if r.w <= 0 || r.h <= 0 {
            return;
        }
        let mut rows: Vec<(String, Rgb)> = Vec::new();
        let end = self.lines.len().saturating_sub(self.back);
        for (s, c) in self.lines[..end].iter().rev() {
            let w = wrap(s, r.w as usize);
            for l in w.into_iter().rev() {
                rows.push((l, *c));
            }
            if rows.len() >= r.h as usize {
                break;
            }
        }
        rows.truncate(r.h as usize);
        for (k, (s, c)) in rows.iter().rev().enumerate() {
            let y = r.y + r.h - rows.len() as i32 + k as i32;
            f.text_clip(r.x, y, r.w, s, *c, bg);
        }
    }
}

/// What a key did to a list or menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    None,
    Moved,
    Chosen(usize),
    Back,
}

/// A vertical list with one item selected, scrolled to keep it in view.
#[derive(Clone, Debug, Default)]
pub struct List {
    pub selected: usize,
    pub top: usize,
}

impl List {
    /// Arrows, Home/End and PgUp/PgDn move; Enter chooses; Esc goes back.
    pub fn key(&mut self, k: &KeyEvent, len: usize, page: usize) -> Pick {
        if len == 0 {
            return if k.key == Key::Esc { Pick::Back } else { Pick::None };
        }
        let before = self.selected;
        match k.key {
            Key::Up => self.selected = self.selected.saturating_sub(1),
            Key::Down => self.selected = (self.selected + 1).min(len - 1),
            Key::PageUp => self.selected = self.selected.saturating_sub(page.max(1)),
            Key::PageDown => self.selected = (self.selected + page.max(1)).min(len - 1),
            Key::Home => self.selected = 0,
            Key::End => self.selected = len - 1,
            Key::Enter => return Pick::Chosen(self.selected),
            Key::Esc => return Pick::Back,
            _ => return Pick::None,
        }
        if self.selected != before { Pick::Moved } else { Pick::None }
    }

    fn follow(&mut self, rows: usize) {
        if self.selected < self.top {
            self.top = self.selected;
        } else if rows > 0 && self.selected >= self.top + rows {
            self.top = self.selected + 1 - rows;
        }
    }

    /// Draw items (one line each) in `r`, the selected one highlighted.
    pub fn draw(&mut self, f: &mut Frame, r: Rect, items: &[(String, Rgb)], t: &Theme) {
        self.selected = self.selected.min(items.len().saturating_sub(1));
        self.follow(r.h.max(0) as usize);
        for row in 0..r.h {
            let i = self.top + row as usize;
            let Some((s, c)) = items.get(i) else { break };
            let pick = i == self.selected;
            let (fg, bg) = if pick { (t.pick_fg, t.pick_bg) } else { (*c, t.bg) };
            f.fill(Rect::new(r.x, r.y + row, r.w, 1), Cell::new(' ', fg, bg));
            f.text_clip(r.x + 1, r.y + row, r.w - 1, s, fg, bg);
        }
    }
}

/// A menu: titled items with a line of notes under the one selected.
#[derive(Clone, Debug)]
pub struct Menu {
    pub items: Vec<(String, String)>,
    pub list: List,
}

impl Menu {
    pub fn new(items: Vec<(String, String)>) -> Menu {
        Menu { items, list: List::default() }
    }

    pub fn key(&mut self, k: &KeyEvent) -> Pick {
        // Number keys choose directly.
        if let Key::Char(c) = k.key {
            if let Some(d) = c.to_digit(10) {
                let i = d as usize;
                if i >= 1 && i <= self.items.len() {
                    self.list.selected = i - 1;
                    return Pick::Chosen(i - 1);
                }
            }
        }
        self.list.key(k, self.items.len(), 5)
    }

    pub fn draw(&mut self, f: &mut Frame, r: Rect, t: &Theme) {
        let notes_h = 3.min(r.h / 3);
        let rows: Vec<(String, Rgb)> = self.items.iter().enumerate().map(|(i, (s, _))| (format!("{}  {s}", i + 1), t.text)).collect();
        self.list.draw(f, Rect::new(r.x, r.y, r.w, r.h - notes_h - 1), &rows, t);
        if let Some((_, note)) = self.items.get(self.list.selected) {
            paragraph(f, Rect::new(r.x + 1, r.bottom() - notes_h, r.w - 2, notes_h), note, t.dim, t.bg);
        }
    }
}

/// A table: a header row and rows of cells, with a selected row.
#[derive(Clone, Debug, Default)]
pub struct Table {
    pub list: List,
}

impl Table {
    pub fn draw(&mut self, f: &mut Frame, r: Rect, header: &[(&str, i32)], rows: &[Vec<(String, Rgb)>], t: &Theme) {
        if r.h < 2 {
            return;
        }
        let mut x = r.x;
        for (h, w) in header {
            f.text_clip(x + 1, r.y, *w - 1, h, t.title, t.bg);
            x += w;
        }
        self.list.selected = self.list.selected.min(rows.len().saturating_sub(1));
        let body = Rect::new(r.x, r.y + 1, r.w, r.h - 1);
        self.list.follow(body.h as usize);
        for row in 0..body.h {
            let i = self.list.top + row as usize;
            let Some(cells) = rows.get(i) else { break };
            let pick = i == self.list.selected;
            let bg = if pick { t.pick_bg } else { t.bg };
            f.fill(Rect::new(body.x, body.y + row, body.w, 1), Cell::new(' ', t.text, bg));
            let mut x = body.x;
            for ((s, c), (_, w)) in cells.iter().zip(header) {
                let fg = if pick && *c == t.text { t.pick_fg } else { *c };
                f.text_clip(x + 1, body.y + row, (*w - 1).min(body.right() - x - 1), s, fg, bg);
                x += w;
            }
        }
    }
}

/// A row of tabs, `1 log  2 crew  3 orders`, the active one highlighted.
pub fn tabs(f: &mut Frame, x: i32, y: i32, w: i32, names: &[&str], active: usize, t: &Theme) {
    let mut cx = x;
    for (i, n) in names.iter().enumerate() {
        let s = format!(" {} {} ", i + 1, n);
        let (fg, bg) = if i == active { (t.pick_fg, t.pick_bg) } else { (t.dim, t.bg) };
        if cx + s.chars().count() as i32 > x + w {
            break;
        }
        cx += f.text(cx, y, &s, fg, bg) + 1;
    }
}

/// Key hints along a line: `[a/d] helm  [w/s] sail`, wrapped onto as many
/// rows as `r` has. Returns the rows used.
pub fn keybar(f: &mut Frame, r: Rect, keys: &[(&str, &str)], t: &Theme) -> i32 {
    let (mut x, mut y) = (r.x, r.y);
    for (k, what) in keys {
        let need = (k.chars().count() + what.chars().count() + 4) as i32;
        if x + need > r.right() && x > r.x {
            x = r.x;
            y += 1;
        }
        if y >= r.bottom() {
            break;
        }
        x += f.text(x, y, &format!("[{k}]"), t.title, t.bg);
        x += f.text(x, y, &format!(" {what}  "), t.dim, t.bg);
    }
    (y - r.y + 1).min(r.h)
}

/// A horizontal bar of `w` cells filled to `frac` (0..1).
pub fn bar(f: &mut Frame, x: i32, y: i32, w: i32, frac: f32, fg: Rgb, bg: Rgb) {
    let n = (frac.clamp(0.0, 1.0) * w as f32).round() as i32;
    for i in 0..w {
        f.set(x + i, y, Cell::new(if i < n { '=' } else { '-' }, if i < n { fg } else { Rgb(60, 64, 72) }, bg));
    }
}
