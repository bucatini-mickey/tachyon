//! The Forge: Tachyon's workshop for making things, for any game.
//!
//! A project is a folder under `forge/`: a library of materials and kinds
//! (what things are made of, and the things themselves: a glyph and colours
//! each, and tags), and parts: pieces built cell by cell over several levels,
//! with sockets where they join other parts (a castle wall to a tower, a deck
//! to a hull), for games and procedural generation to put together.
//!
//!     cargo run --release --example forge                 (project "default")
//!     cargo run --release --example forge -- broadside    (or any project)
//!
//! Parts save as `forge/<project>/parts/<name>.tpart`: a Tachyon blueprint
//! (any game can stamp it) with its sockets after the `end` line.
//!
//! Keys (Part mode):  arrows move  Space paint  Del clear (keep)
//!   w f o  brush: wall, floor, open      u n  stairs up, down
//!   [ ]  material     { }  thing (k: things on/off)     t: pick under cursor
//!   PgUp PgDn  level     + -  add or remove a top level
//!   s  socket here (again to turn it, x to remove)     Ctrl+S save
//!   Ctrl+N new part  Ctrl+O open part     Tab: Library mode     Esc: quit
//! Keys (Library mode): up down choose   m new material  k new kind
//!   g then a key: glyph (Shift: the floor glyph)   1-6 colour (Ctrl: background)
//!   r rename   Del remove   Ctrl+S save   Tab: back to the part

use std::fs;
use std::io;
use std::path::PathBuf;
use tachyon::blueprint::{Blueprint, BpCell};
use tachyon::prelude::*;
use tachyon::tile::Tile;

const PANEL_W: i32 = 34;
const BG: Rgb = Rgb(14, 13, 12);
const INK: Rgb = Rgb(222, 214, 196);
const DIM: Rgb = Rgb(120, 112, 100);
const GOLD: Rgb = Rgb(220, 180, 90);
const RED: Rgb = Rgb(220, 100, 80);

// ---- the library: materials and kinds, as text ------------------------------------------------

#[derive(Clone, Debug)]
struct Entry {
    kind: bool,
    name: String,
    glyph: char,
    floor: char,
    fg: Rgb,
    bg: Rgb,
    tags: Vec<String>,
}

fn rgb(s: &str) -> Option<Rgb> {
    let v: Vec<u8> = s.split_whitespace().filter_map(|x| x.parse().ok()).collect();
    (v.len() == 3).then(|| Rgb(v[0], v[1], v[2]))
}

/// `material oak | # . | 140 100 60 | 50 34 20 | wood` and `kind cannon | o | 70 70 76 | metal heavy`.
fn parse_library(text: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let (word, rest) = line.split_once(' ').unwrap_or((line, ""));
        if word != "material" && word != "kind" {
            continue;
        }
        let f: Vec<&str> = rest.split('|').map(str::trim).collect();
        let glyphs: Vec<char> = f.get(1).unwrap_or(&"?").chars().filter(|c| *c != ' ').collect();
        out.push(Entry {
            kind: word == "kind",
            name: f[0].to_string(),
            glyph: *glyphs.first().unwrap_or(&'?'),
            floor: *glyphs.get(1).unwrap_or(&'.'),
            fg: f.get(2).and_then(|s| rgb(s)).unwrap_or(INK),
            bg: f.get(3).and_then(|s| rgb(s)).unwrap_or(BG),
            tags: f.get(4).map(|s| s.split_whitespace().map(String::from).collect()).unwrap_or_default(),
        });
    }
    out
}

fn library_text(project: &str, lib: &[Entry]) -> String {
    let mut s = format!("tachyon-library 1\n# The {project} library: materials (wall and floor glyphs) and kinds (things).\n");
    for e in lib {
        let c = |c: Rgb| format!("{} {} {}", c.0, c.1, c.2);
        if e.kind {
            s += &format!("kind {} | {} | {} | {} | {}\n", e.name, e.glyph, c(e.fg), c(e.bg), e.tags.join(" "));
        } else {
            s += &format!("material {} | {} {} | {} | {} | {}\n", e.name, e.glyph, e.floor, c(e.fg), c(e.bg), e.tags.join(" "));
        }
    }
    s
}

/// A start for a new project: stone and timber, metals, the sea, the stars.
const SEED_LIBRARY: &str = "tachyon-library 1
material stone block | # . | 178 176 168 | 52 50 48 | stone masonry
material granite | % , | 130 128 136 | 40 38 44 | stone rock
material oak | # + | 150 108 64 | 46 32 18 | wood timber
material pine plank | # = | 196 160 104 | 58 44 26 | wood timber deck
material iron | # : | 150 150 160 | 34 34 40 | metal
material steel plate | # - | 176 184 196 | 30 34 40 | metal hull
material brass | # . | 214 172 92 | 52 40 18 | metal fitting
material glass | ' . | 170 214 232 | 30 50 60 | clear window
material earth | # , | 128 92 58 | 44 30 18 | soil
material grass | \" , | 84 150 58 | 22 40 16 | soil plant
material water | ~ ~ | 96 150 196 | 16 38 64 | liquid
material hull alloy | # . | 120 200 210 | 18 30 36 | metal scifi
material concrete | # . | 160 160 156 | 46 46 44 | stone modern
kind door | ' | 200 150 90 | 0 0 0 | opening
kind bed | B | 180 140 110 | 0 0 0 | furniture
kind table | T | 170 120 70 | 0 0 0 | furniture
kind chair | h | 170 120 70 | 0 0 0 | furniture
kind chest | = | 190 150 80 | 0 0 0 | container
kind barrel | 0 | 160 110 60 | 0 0 0 | container
kind torch | ! | 255 190 90 | 0 0 0 | light
kind ladder | H | 190 150 90 | 0 0 0 | climb
kind cannon | o | 90 90 96 | 0 0 0 | weapon ship
kind mast | | | 200 170 110 | 0 0 0 | ship
kind wheel | * | 190 140 80 | 0 0 0 | ship helm
kind anchor | J | 140 140 150 | 0 0 0 | ship
kind console | & | 120 220 200 | 0 0 0 | scifi
kind vent | # | 110 120 130 | 0 0 0 | scifi
kind crate | x | 170 130 80 | 0 0 0 | container
";

// ---- parts: a blueprint and its sockets -------------------------------------------------------

const DIRS: [&str; 6] = ["n", "e", "s", "w", "up", "down"];

#[derive(Clone, Debug)]
struct Socket {
    x: i32,
    y: i32,
    z: i32,
    dir: usize,
    tag: String,
}

struct Part {
    bp: Blueprint,
    sockets: Vec<Socket>,
}

impl Part {
    fn new(name: &str, w: i32, h: i32, d: i32) -> Part {
        Part { bp: Blueprint { name: name.into(), w, h, d, cells: vec![None; (w * h * d) as usize] }, sockets: Vec::new() }
    }
    fn idx(&self, x: i32, y: i32, z: i32) -> usize {
        ((z * self.bp.h + y) * self.bp.w + x) as usize
    }
    fn text(&self) -> String {
        let mut s = self.bp.to_text();
        for k in &self.sockets {
            s += &format!("socket {} {} {} {} {}\n", k.x, k.y, k.z, DIRS[k.dir], k.tag);
        }
        s
    }
    fn from_text(text: &str) -> Result<Part, String> {
        let bp = Blueprint::from_text(text)?;
        let mut sockets = Vec::new();
        for line in text.lines().skip_while(|l| l.trim() != "end") {
            let v: Vec<&str> = line.split_whitespace().collect();
            if v.first() == Some(&"socket") && v.len() >= 5 {
                let n = |i: usize| v[i].parse::<i32>().unwrap_or(0);
                sockets.push(Socket { x: n(1), y: n(2), z: n(3), dir: DIRS.iter().position(|d| *d == v[4]).unwrap_or(0), tag: v.get(5).unwrap_or(&"any").to_string() });
            }
        }
        Ok(Part { bp, sockets })
    }
    /// A level more on top (or one fewer), keeping what is built.
    fn resize_depth(&mut self, d: i32) {
        let d = d.max(1);
        let plane = (self.bp.w * self.bp.h) as usize;
        self.bp.cells.resize(plane * d as usize, None);
        self.bp.d = d;
        self.sockets.retain(|k| k.z < d);
    }
}

// ---- the workshop ----------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Part,
    Library,
}

#[derive(Clone, PartialEq)]
enum Ask {
    New,
    Open,
    Rename,
    NewEntry(bool),
    SocketTag,
    Glyph(bool),
}

struct Forge {
    project: String,
    dir: PathBuf,
    lib: Vec<Entry>,
    part: Part,
    mode: Mode,
    cx: i32,
    cy: i32,
    z: i32,
    tile: Tile,
    mat: usize,
    thing: usize,
    with_thing: bool,
    pick: usize,
    ask: Option<(Ask, String)>,
    note: String,
    view: (i32, i32, i32, i32),
}

impl Forge {
    fn mats(&self) -> Vec<usize> {
        (0..self.lib.len()).filter(|&i| !self.lib[i].kind).collect()
    }
    fn kinds(&self) -> Vec<usize> {
        (0..self.lib.len()).filter(|&i| self.lib[i].kind).collect()
    }
    fn entry(&self, name: &str, kind: bool) -> Option<&Entry> {
        self.lib.iter().find(|e| e.kind == kind && e.name == name)
    }
    fn brush(&self) -> Option<BpCell> {
        let mats = self.mats();
        let mat = mats.get(self.mat % mats.len().max(1)).map(|&i| self.lib[i].name.clone()).unwrap_or_default();
        let kinds = self.kinds();
        let things = if self.with_thing { kinds.get(self.thing % kinds.len().max(1)).map(|&i| vec![self.lib[i].name.clone()]).unwrap_or_default() } else { vec![] };
        Some(BpCell { tile: self.tile, mat: if self.tile == Tile::Open { String::new() } else { mat }, things })
    }
    fn paint(&mut self, cell: Option<BpCell>) {
        let (x, y, z) = (self.cx, self.cy, self.z);
        if x < 0 || y < 0 || x >= self.part.bp.w || y >= self.part.bp.h {
            return;
        }
        let i = self.part.idx(x, y, z);
        self.part.bp.cells[i] = cell;
    }
    fn save_part(&mut self) {
        let path = self.dir.join("parts").join(format!("{}.tpart", self.part.bp.name));
        let _ = fs::create_dir_all(self.dir.join("parts"));
        self.note = match fs::write(&path, self.part.text()) {
            Ok(_) => format!("Saved {}", path.display()),
            Err(e) => format!("Could not save: {e}"),
        };
    }
    fn save_lib(&mut self) {
        let path = self.dir.join("library.tlib");
        self.note = match fs::write(&path, library_text(&self.project, &self.lib)) {
            Ok(_) => format!("Saved {}", path.display()),
            Err(e) => format!("Could not save: {e}"),
        };
    }
    fn look(&self, c: &BpCell) -> Cell {
        if let Some(t) = c.things.last() {
            if let Some(e) = self.entry(t, true) {
                let bg = self.entry(&c.mat, false).map(|m| m.bg).unwrap_or(BG);
                return Cell::new(e.glyph, e.fg, bg);
            }
            return Cell::new(t.chars().next().unwrap_or('?'), RED, BG);
        }
        let m = self.entry(&c.mat, false);
        let (fg, bg) = m.map(|m| (m.fg, m.bg)).unwrap_or((RED, BG));
        match c.tile {
            Tile::Open => Cell::new(' ', DIM, BG),
            Tile::Wall => Cell::new(m.map(|m| m.glyph).unwrap_or('#'), fg, bg),
            Tile::Floor => Cell::new(m.map(|m| m.floor).unwrap_or('.'), fg.lerp(bg, 0.35), bg),
            Tile::Ramp => Cell::new('^', fg, bg),
            Tile::StairUp => Cell::new('<', fg, bg),
            Tile::StairDown => Cell::new('>', fg, bg),
            Tile::StairUpDown => Cell::new('X', fg, bg),
        }
    }
    fn answer(&mut self, what: Ask, text: String) {
        let t = text.trim().to_string();
        match what {
            Ask::New => {
                let v: Vec<&str> = t.split_whitespace().collect();
                let n = |i: usize, d: i32| v.get(i).and_then(|s| s.parse().ok()).unwrap_or(d);
                if let Some(name) = v.first() {
                    self.part = Part::new(name, n(1, 12).clamp(1, 200), n(2, 8).clamp(1, 200), n(3, 1).clamp(1, 32));
                    (self.cx, self.cy, self.z) = (0, 0, 0);
                    self.note = format!("New part {name}");
                }
            }
            Ask::Open => {
                let path = self.dir.join("parts").join(format!("{t}.tpart"));
                match fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|s| Part::from_text(&s)) {
                    Ok(p) => {
                        self.part = p;
                        (self.cx, self.cy, self.z) = (0, 0, 0);
                        self.note = format!("Opened {t}");
                    }
                    Err(e) => self.note = format!("Could not open {t}: {e}"),
                }
            }
            Ask::Rename => {
                if !t.is_empty() {
                    if let Some(e) = self.lib.get_mut(self.pick) {
                        e.name = t;
                    }
                }
            }
            Ask::NewEntry(kind) => {
                if !t.is_empty() {
                    self.lib.push(Entry { kind, name: t, glyph: if kind { '?' } else { '#' }, floor: '.', fg: INK, bg: if kind { Rgb(0, 0, 0) } else { BG }, tags: vec![] });
                    self.pick = self.lib.len() - 1;
                }
            }
            Ask::SocketTag => {
                let (x, y, z) = (self.cx, self.cy, self.z);
                self.part.sockets.retain(|k| !(k.x == x && k.y == y && k.z == z));
                self.part.sockets.push(Socket { x, y, z, dir: 0, tag: if t.is_empty() { "any".into() } else { t.replace(' ', "-") } });
            }
            Ask::Glyph(_) => {}
        }
    }
}

impl Game for Forge {
    fn tick(&mut self) {}

    fn ticks_per_second(&self) -> f64 {
        1.0
    }

    fn on_key(&mut self, k: KeyEvent) -> Flow {
        // A question being answered.
        if let Some((what, mut text)) = self.ask.take() {
            if let Ask::Glyph(floor) = what {
                if let Key::Char(c) = k.key {
                    if let Some(e) = self.lib.get_mut(self.pick) {
                        if floor { e.floor = c } else { e.glyph = c }
                    }
                }
                return Flow::Continue;
            }
            match k.key {
                Key::Enter => self.answer(what, text),
                Key::Esc => {}
                Key::Backspace => {
                    text.pop();
                    self.ask = Some((what, text));
                }
                Key::Char(c) if !k.ctrl => {
                    text.push(c);
                    self.ask = Some((what, text));
                }
                _ => self.ask = Some((what, text)),
            }
            return Flow::Continue;
        }
        if k.key == Key::Esc {
            return Flow::Quit;
        }
        if k.key == Key::Tab {
            self.mode = if self.mode == Mode::Part { Mode::Library } else { Mode::Part };
            return Flow::Continue;
        }
        if k.ctrl {
            match k.key {
                Key::Char('s') => if self.mode == Mode::Part { self.save_part() } else { self.save_lib() },
                Key::Char('n') => self.ask = Some((Ask::New, String::new())),
                Key::Char('o') => self.ask = Some((Ask::Open, String::new())),
                _ => {}
            }
            return Flow::Continue;
        }
        match self.mode {
            Mode::Library => self.library_key(k),
            Mode::Part => self.part_key(k),
        }
        Flow::Continue
    }

    fn on_mouse(&mut self, m: MouseEvent) -> Flow {
        let (vx, vy, vw, vh) = self.view;
        if self.mode == Mode::Part && m.x >= vx && m.y >= vy && m.x < vx + vw && m.y < vy + vh {
            self.cx = (m.x - vx).min(self.part.bp.w - 1);
            self.cy = (m.y - vy).min(self.part.bp.h - 1);
            if matches!(m.kind, MouseKind::Down(MouseButton::Left) | MouseKind::Drag(MouseButton::Left)) {
                let b = self.brush();
                self.paint(b);
            } else if matches!(m.kind, MouseKind::Down(MouseButton::Right) | MouseKind::Drag(MouseButton::Right)) {
                self.paint(None);
            }
        }
        Flow::Continue
    }

    fn draw(&mut self, f: &mut Frame, _stats: &LoopStats) {
        f.clear(BG);
        let (fw, fh) = (f.width(), f.height());
        let canvas = Rect { x: 0, y: 1, w: (fw - PANEL_W).max(10), h: fh - 3 };
        f.text_clip(1, 0, fw - 2, &format!("The Forge · project {} · {}", self.project, if self.mode == Mode::Part { "Part" } else { "Library" }), GOLD, BG);
        match self.mode {
            Mode::Part => self.draw_part(f, canvas),
            Mode::Library => self.draw_library(f, canvas),
        }
        let foot = match &self.ask {
            Some((Ask::Glyph(fl), _)) => format!("Press the key for the {} glyph", if *fl { "floor" } else { "wall/thing" }),
            Some((Ask::New, t)) => format!("New part: name width height levels > {t}_"),
            Some((Ask::Open, t)) => format!("Open part > {t}_"),
            Some((Ask::Rename, t)) => format!("Rename to > {t}_"),
            Some((Ask::NewEntry(k), t)) => format!("New {} > {t}_", if *k { "kind" } else { "material" }),
            Some((Ask::SocketTag, t)) => format!("Socket tag (what fits here: wall, door, deck...) > {t}_"),
            None => self.note.clone(),
        };
        f.text_clip(1, fh - 1, fw - 2, &foot, if self.ask.is_some() { GOLD } else { DIM }, BG);
    }
}

impl Forge {
    fn part_key(&mut self, k: KeyEvent) {
        let (w, h, d) = (self.part.bp.w, self.part.bp.h, self.part.bp.d);
        match k.key {
            Key::Left => self.cx = (self.cx - 1).max(0),
            Key::Right => self.cx = (self.cx + 1).min(w - 1),
            Key::Up => self.cy = (self.cy - 1).max(0),
            Key::Down => self.cy = (self.cy + 1).min(h - 1),
            Key::PageUp => self.z = (self.z + 1).min(d - 1),
            Key::PageDown => self.z = (self.z - 1).max(0),
            Key::Delete => self.paint(None),
            Key::Char(' ') => {
                let b = self.brush();
                self.paint(b);
            }
            Key::Char('w') => self.tile = Tile::Wall,
            Key::Char('f') => self.tile = Tile::Floor,
            Key::Char('o') => self.tile = Tile::Open,
            Key::Char('u') => self.tile = Tile::StairUp,
            Key::Char('n') => self.tile = Tile::StairDown,
            Key::Char('[') => self.mat = (self.mat + self.mats().len().max(1) - 1) % self.mats().len().max(1),
            Key::Char(']') => self.mat = (self.mat + 1) % self.mats().len().max(1),
            Key::Char('{') => self.thing = (self.thing + self.kinds().len().max(1) - 1) % self.kinds().len().max(1),
            Key::Char('}') => self.thing = (self.thing + 1) % self.kinds().len().max(1),
            Key::Char('k') => self.with_thing = !self.with_thing,
            Key::Char('+') => self.part.resize_depth(d + 1),
            Key::Char('-') => {
                self.part.resize_depth(d - 1);
                self.z = self.z.min(self.part.bp.d - 1);
            }
            Key::Char('t') => {
                if let Some(Some(c)) = self.part.bp.cells.get(self.part.idx(self.cx, self.cy, self.z)).cloned() {
                    self.tile = c.tile;
                    if let Some(i) = self.mats().iter().position(|&i| self.lib[i].name == c.mat) {
                        self.mat = i;
                    }
                    self.with_thing = !c.things.is_empty();
                    if let Some(i) = c.things.last().and_then(|t| self.kinds().iter().position(|&i| &self.lib[i].name == t)) {
                        self.thing = i;
                    }
                }
            }
            Key::Char('s') => {
                let (x, y, z) = (self.cx, self.cy, self.z);
                if let Some(s) = self.part.sockets.iter_mut().find(|s| s.x == x && s.y == y && s.z == z) {
                    s.dir = (s.dir + 1) % DIRS.len();
                } else {
                    self.ask = Some((Ask::SocketTag, String::new()));
                }
            }
            Key::Char('x') => {
                let (x, y, z) = (self.cx, self.cy, self.z);
                self.part.sockets.retain(|s| !(s.x == x && s.y == y && s.z == z));
            }
            _ => {}
        }
    }

    fn library_key(&mut self, k: KeyEvent) {
        let n = self.lib.len().max(1);
        let step = |v: u8, up: bool| if up { v.saturating_add(8) } else { v.saturating_sub(8) };
        match k.key {
            Key::Up => self.pick = (self.pick + n - 1) % n,
            Key::Down => self.pick = (self.pick + 1) % n,
            Key::Char('m') => self.ask = Some((Ask::NewEntry(false), String::new())),
            Key::Char('k') => self.ask = Some((Ask::NewEntry(true), String::new())),
            Key::Char('r') => self.ask = Some((Ask::Rename, String::new())),
            Key::Char('g') => self.ask = Some((Ask::Glyph(false), String::new())),
            Key::Char('G') => self.ask = Some((Ask::Glyph(true), String::new())),
            Key::Delete => {
                if self.pick < self.lib.len() {
                    self.lib.remove(self.pick);
                    self.pick = self.pick.saturating_sub(1);
                }
            }
            Key::Char(c @ '1'..='6') => {
                if let Some(e) = self.lib.get_mut(self.pick) {
                    let i = (c as u8 - b'1') as usize;
                    let col = if k.ctrl || k.alt { &mut e.bg } else { &mut e.fg };
                    let up = i % 2 == 1;
                    match i / 2 {
                        0 => col.0 = step(col.0, up),
                        1 => col.1 = step(col.1, up),
                        _ => col.2 = step(col.2, up),
                    }
                }
            }
            _ => {}
        }
    }

    fn draw_part(&mut self, f: &mut Frame, r: Rect) {
        let p = &self.part;
        // The part, centred where it fits, with the level below showing faintly through open cells.
        let (vw, vh) = (p.bp.w.min(r.w - 2), p.bp.h.min(r.h - 2));
        let ox = r.x + 1 + ((r.w - 2 - vw) / 2).max(0);
        let oy = r.y + 1 + ((r.h - 2 - vh) / 2).max(0);
        self.view = (ox, oy, vw, vh);
        f.border(Rect { x: ox - 1, y: oy - 1, w: vw + 2, h: vh + 2 }, DIM, BG, &format!(" {} · level {} of {} ", p.bp.name, self.z + 1, p.bp.d));
        for y in 0..vh {
            for x in 0..vw {
                let here = p.bp.cells[p.idx(x, y, self.z)].as_ref();
                let cell = match here {
                    Some(c) if c.tile != Tile::Open || !c.things.is_empty() => self.look(c),
                    _ => match (self.z > 0).then(|| p.bp.cells[p.idx(x, y, self.z - 1)].as_ref()).flatten() {
                        Some(below) => {
                            let c = self.look(below);
                            Cell::new(c.ch, c.fg.lerp(BG, 0.7), c.bg.lerp(BG, 0.7))
                        }
                        None => Cell::new(if here.is_some() { ' ' } else { '·' }, Rgb(40, 38, 34), BG),
                    },
                };
                f.set(ox + x, oy + y, cell);
            }
        }
        for s in p.sockets.iter().filter(|s| s.z == self.z && s.x < vw && s.y < vh) {
            let ch = ['^', '>', 'v', '<', '+', '-'][s.dir];
            f.set(ox + s.x, oy + s.y, Cell::new(ch, Rgb(20, 16, 10), GOLD));
        }
        let cur = f.get(ox + self.cx, oy + self.cy);
        f.set(ox + self.cx, oy + self.cy, Cell::new(cur.ch, BG, INK));

        // The panel: the brush, the palette, the sockets, the keys.
        let px = f.width() - PANEL_W;
        let mut y = r.y;
        let mut line = |f: &mut Frame, s: &str, c: Rgb| {
            f.text_clip(px + 1, y, PANEL_W - 2, s, c, BG);
            y += 1;
        };
        line(f, &format!("Cursor {},{}  level {}", self.cx, self.cy, self.z + 1), INK);
        line(f, &format!("Size {} x {} x {}", p.bp.w, p.bp.h, p.bp.d), DIM);
        line(f, "", INK);
        let tile = match self.tile { Tile::Wall => "wall", Tile::Floor => "floor", Tile::Open => "open", Tile::StairUp => "stairs up", Tile::StairDown => "stairs down", _ => "other" };
        line(f, &format!("Brush: {tile}"), GOLD);
        let mats = self.mats();
        let kinds = self.kinds();
        for (k, &i) in mats.iter().enumerate() {
            if (k as i32 - self.mat as i32).abs() <= 3 {
                let e = &self.lib[i];
                let on = k == self.mat % mats.len().max(1);
                f.set(px + 1, y, Cell::new(e.glyph, e.fg, e.bg));
                f.set(px + 2, y, Cell::new(e.floor, e.fg.lerp(e.bg, 0.35), e.bg));
                f.text_clip(px + 4, y, PANEL_W - 5, &format!("{}{}", if on { "> " } else { "  " }, e.name), if on { INK } else { DIM }, BG);
                y += 1;
            }
        }
        let mut line = |f: &mut Frame, s: &str, c: Rgb| {
            f.text_clip(px + 1, y, PANEL_W - 2, s, c, BG);
            y += 1;
        };
        line(f, &format!("Thing ({}): ", if self.with_thing { "on" } else { "off, k" }), GOLD);
        let mut y2 = y;
        for (k, &i) in kinds.iter().enumerate() {
            if (k as i32 - self.thing as i32).abs() <= 2 {
                let e = &self.lib[i];
                let on = k == self.thing % kinds.len().max(1);
                f.set(px + 1, y2, Cell::new(e.glyph, e.fg, BG));
                f.text_clip(px + 4, y2, PANEL_W - 5, &format!("{}{}", if on { "> " } else { "  " }, e.name), if on && self.with_thing { INK } else { DIM }, BG);
                y2 += 1;
            }
        }
        let mut y = y2 + 1;
        f.text_clip(px + 1, y, PANEL_W - 2, &format!("Sockets ({})", p.sockets.len()), GOLD, BG);
        y += 1;
        for s in p.sockets.iter().take(6) {
            f.text_clip(px + 1, y, PANEL_W - 2, &format!("{},{} L{} {} {}", s.x, s.y, s.z + 1, DIRS[s.dir], s.tag), DIM, BG);
            y += 1;
        }
        y += 1;
        for help in ["Space paint  Del clear  t pick", "w f o u n  wall floor open stairs", "[ ] material  { } thing  k on/off", "PgUp PgDn level  + - levels", "s socket (turn)  x remove", "^S save ^N new ^O open  Tab library"] {
            f.text_clip(px + 1, y, PANEL_W - 2, help, DIM, BG);
            y += 1;
        }
    }

    fn draw_library(&mut self, f: &mut Frame, r: Rect) {
        let mut y = r.y + 1;
        for (title, kind) in [("Materials (wall, floor)", false), ("Kinds (things)", true)] {
            f.text_clip(r.x + 2, y, r.w - 4, title, GOLD, BG);
            y += 1;
            for (i, e) in self.lib.iter().enumerate().filter(|(_, e)| e.kind == kind) {
                if y >= r.y + r.h - 1 {
                    break;
                }
                let on = i == self.pick;
                f.set(r.x + 2, y, Cell::new(e.glyph, e.fg, e.bg));
                if !kind {
                    f.set(r.x + 3, y, Cell::new(e.floor, e.fg.lerp(e.bg, 0.35), e.bg));
                }
                let info = format!("{}{:<16} fg {:>3} {:>3} {:>3}  {}", if on { "> " } else { "  " }, e.name, e.fg.0, e.fg.1, e.fg.2, e.tags.join(" "));
                f.text_clip(r.x + 5, y, r.w - 7, &info, if on { INK } else { DIM }, BG);
                y += 1;
            }
            y += 1;
        }
        let px = f.width() - PANEL_W;
        let mut y = r.y;
        for help in ["up down  choose", "m new material  k new kind", "g glyph  G floor glyph", "1 2 red - +   3 4 green", "5 6 blue  (Ctrl: background)", "r rename  Del remove", "^S save  Tab back to the part"] {
            f.text_clip(px + 1, y, PANEL_W - 2, help, DIM, BG);
            y += 1;
        }
    }
}

fn main() -> io::Result<()> {
    let project = std::env::args().nth(1).unwrap_or_else(|| "default".into());
    let dir = PathBuf::from("forge").join(&project);
    fs::create_dir_all(dir.join("parts"))?;
    let lib_path = dir.join("library.tlib");
    if !lib_path.exists() {
        fs::write(&lib_path, SEED_LIBRARY)?;
    }
    let lib = parse_library(&fs::read_to_string(&lib_path)?);
    let mut forge = Forge {
        project,
        dir,
        lib,
        part: Part::new("untitled", 16, 10, 2),
        mode: Mode::Part,
        cx: 0,
        cy: 0,
        z: 0,
        tile: Tile::Wall,
        mat: 0,
        thing: 0,
        with_thing: false,
        pick: 0,
        ask: None,
        note: "Ctrl+N for a new part, Ctrl+O to open one. Tab for the library.".into(),
        view: (0, 0, 0, 0),
    };
    tachyon::app::run(&mut forge, 30)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_round_trips() {
        let lib = parse_library(SEED_LIBRARY);
        assert!(lib.iter().any(|e| e.name == "stone block" && !e.kind && e.glyph == '#' && e.floor == '.'));
        assert!(lib.iter().any(|e| e.name == "cannon" && e.kind && e.glyph == 'o'));
        let again = parse_library(&library_text("t", &lib));
        assert_eq!(again.len(), lib.len());
        assert_eq!(again[0].fg, lib[0].fg);
    }

    #[test]
    fn a_part_keeps_its_sockets_and_stays_a_blueprint() {
        let mut p = Part::new("tower", 5, 5, 3);
        let i = p.idx(0, 0, 1);
        p.bp.cells[i] = Some(BpCell { tile: Tile::Wall, mat: "stone block".into(), things: vec![] });
        p.sockets.push(Socket { x: 2, y: 4, z: 0, dir: 2, tag: "curtain-wall".into() });
        let text = p.text();
        let back = Part::from_text(&text).unwrap();
        assert_eq!(back.bp, p.bp);
        assert_eq!(back.sockets.len(), 1);
        assert_eq!(back.sockets[0].tag, "curtain-wall");
        assert_eq!(back.sockets[0].dir, 2);
        // Any game can still read it as a plain blueprint.
        assert!(Blueprint::from_text(&text).is_ok());
    }
}
