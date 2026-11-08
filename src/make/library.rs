//! The Tokamak's file formats as a library any game can load: the project
//! library (`.tlib`: materials and kinds with glyphs, colours and tags), parts
//! (`.tpart`: a blueprint plus sockets where it joins other parts) and art
//! (`.tart`: glyph pictures). Each type parses from text and writes back to
//! the same text, so files round-trip byte for byte.
//!
//! Rust notes for the reader:
//! - Everything in a module is private to it by default. `pub` on a struct
//!   makes the type nameable from outside; its fields stay private unless each
//!   one is marked `pub` too. Same for `fn`, `const` and the methods in an `impl`.
//! - `pub mod library;` in `make/mod.rs` makes this file a module of the crate.
//!   `lib.rs` then re-exports it as `tachyon::library`, so a game (or the
//!   tokamak example) writes `use tachyon::library::{Part, Art};`.
//! - This file uses `crate::...` paths: `crate` is the root of this library.

use crate::blueprint::{Blueprint, BpCell};
use crate::color::Rgb;
use crate::frame::Cell;
use crate::tile::Tile;

/// Colours used when a file leaves one out (the Tokamak's ink and background).
pub const DEFAULT_INK: Rgb = Rgb(222, 214, 196);
pub const DEFAULT_BG: Rgb = Rgb(14, 13, 12);

// ---- the library: materials and kinds, as text ------------------------------------------------

/// One line of a `.tlib`: a material (`kind == false`, with a wall and a floor glyph) or a kind of thing.
#[derive(Clone, Debug)]
pub struct Entry {
    pub kind: bool,
    pub name: String,
    pub glyph: char,
    pub floor: char,
    pub fg: Rgb,
    pub bg: Rgb,
    pub tags: Vec<String>,
}

pub fn rgb(s: &str) -> Option<Rgb> {
    let v: Vec<u8> = s.split_whitespace().filter_map(|x| x.parse().ok()).collect();
    (v.len() == 3).then(|| Rgb(v[0], v[1], v[2]))
}

// ---- what may be typed: keep the text format and file paths safe -------------------------------
//
// A `.tlib` line splits its fields at " | " and the tags at spaces, so those characters
// typed into a name, tag or glyph would not read back the same. These checks return
// `Err(note)` (a sentence to show the user) instead of accepting the text.
// Rust note: `Result<(), String>` is "worked, nothing to return" or "failed, here is why";
// callers use `match` or `if let Err(note) = ...` to handle both.

/// A material or kind name: not empty, no `|` (it separates the fields). Spaces are kept
/// legal ("stone block" is in the starter set): the name is only the text before the first " | ".
pub fn check_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        Err("A name cannot be empty".into())
    } else if name.contains('|') {
        Err(format!("No | in names (it splits the library line): {name}"))
    } else if name.chars().any(char::is_control) {
        Err("No control characters in names".into())
    } else {
        Ok(())
    }
}

/// One tag: no `|`, and no space (a space would make it two tags).
pub fn check_tag(tag: &str) -> Result<(), String> {
    if tag.contains('|') {
        Err(format!("No | in tags: {tag}"))
    } else if tag.chars().any(|c| c.is_whitespace() || c.is_control()) {
        Err(format!("No spaces in a tag: {tag:?}"))
    } else {
        Ok(())
    }
}

/// A glyph: never a space (reading back drops spaces, sliding the floor glyph into the wall
/// slot). A material's floor glyph cannot be `|` either (it would read as a field separator);
/// a wall or thing glyph can (a mast).
pub fn check_glyph(c: char, floor: bool) -> Result<(), String> {
    if c.is_whitespace() || c.is_control() {
        Err("A glyph cannot be a space".into())
    } else if floor && c == '|' {
        Err("A floor glyph cannot be | (it splits the library line)".into())
    } else {
        Ok(())
    }
}

/// Windows will not make files with these names, with or without an extension.
const RESERVED: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9",
    "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Makes a part, art or project name safe as a file name: no slashes or other characters
/// Windows refuses, no `..`, no reserved device names. Returns the clean name and whether
/// anything changed (so the caller can say so). Never returns an empty name.
pub fn clean_file_name(raw: &str) -> (String, bool) {
    let s: String = raw
        .trim()
        .chars()
        .map(|c| if c.is_whitespace() || c.is_control() || "\\/:*?\"<>|".contains(c) { '-' } else { c })
        .collect();
    // `..` climbs out of the folder: squeeze every run of dots to one.
    let mut s = s;
    while s.contains("..") {
        s = s.replace("..", ".");
    }
    // A leading dot hides the file; a trailing dot is dropped by Windows.
    let s = s.trim_matches(|c| c == '.' || c == '-');
    let mut s: String = s.chars().take(64).collect();
    if s.is_empty() {
        s = "untitled".into();
    }
    // "con.txt" is reserved too, so test the part before the first dot.
    let stem = s.split('.').next().unwrap_or("").to_ascii_lowercase();
    if RESERVED.contains(&stem.as_str()) {
        s = format!("_{s}");
    }
    let changed = s != raw;
    (s, changed)
}

/// `material oak | # . | 140 100 60 | 50 34 20 | wood` and `kind cannon | o | 70 70 76 | metal heavy`.
pub fn parse_library(text: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let (word, rest) = line.split_once(' ').unwrap_or((line, ""));
        if word != "material" && word != "kind" {
            continue;
        }
        // Fields part at " | ", so a glyph can itself be a | (a mast).
        let f: Vec<&str> = rest.split(" | ").map(|s| s.trim().trim_end_matches(" |")).collect();
        let glyphs: Vec<char> = f.get(1).unwrap_or(&"?").chars().filter(|c| *c != ' ').collect();
        out.push(Entry {
            kind: word == "kind",
            name: f[0].to_string(),
            glyph: *glyphs.first().unwrap_or(&'?'),
            floor: *glyphs.get(1).unwrap_or(&'.'),
            fg: f.get(2).and_then(|s| rgb(s)).unwrap_or(DEFAULT_INK),
            bg: f.get(3).and_then(|s| rgb(s)).unwrap_or(DEFAULT_BG),
            tags: f.get(4).map(|s| s.split_whitespace().map(String::from).collect()).unwrap_or_default(),
        });
    }
    out
}

pub fn library_text(project: &str, lib: &[Entry]) -> String {
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
pub const SEED_LIBRARY: &str = "tachyon-library 1
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

pub const DIRS: [&str; 6] = ["n", "e", "s", "w", "up", "down"];

#[derive(Clone, Debug)]
pub struct Socket {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub dir: usize,
    pub tag: String,
}

#[derive(Clone)]
pub struct Part {
    pub bp: Blueprint,
    pub sockets: Vec<Socket>,
}

impl Part {
    pub fn new(name: &str, w: i32, h: i32, d: i32) -> Part {
        Part { bp: Blueprint { name: name.into(), w, h, d, cells: vec![None; (w * h * d) as usize] }, sockets: Vec::new() }
    }
    pub fn idx(&self, x: i32, y: i32, z: i32) -> usize {
        ((z * self.bp.h + y) * self.bp.w + x) as usize
    }
    pub fn inside(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.bp.w && y < self.bp.h
    }
    pub fn text(&self) -> String {
        let mut s = self.bp.to_text();
        for k in &self.sockets {
            s += &format!("socket {} {} {} {} {}\n", k.x, k.y, k.z, DIRS[k.dir], k.tag);
        }
        s
    }
    pub fn from_text(text: &str) -> Result<Part, String> {
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
    pub fn resize_depth(&mut self, d: i32) {
        let d = d.max(1);
        let plane = (self.bp.w * self.bp.h) as usize;
        self.bp.cells.resize(plane * d as usize, None);
        self.bp.d = d;
        self.sockets.retain(|k| k.z < d);
    }
    /// The level below, built again on level `z`: walls and floors carry up, stairs up meet
    /// stairs down, and the things stay where they were.
    pub fn copy_level_up(&mut self, z: i32) {
        if z <= 0 || z >= self.bp.d {
            return;
        }
        for y in 0..self.bp.h {
            for x in 0..self.bp.w {
                let below = self.bp.cells[self.idx(x, y, z - 1)].as_ref().map(|c| {
                    let tile = match c.tile {
                        Tile::StairUp => Tile::StairDown,
                        Tile::StairDown | Tile::Ramp => Tile::Floor,
                        t => t,
                    };
                    BpCell { tile, mat: c.mat.clone(), things: vec![] }
                });
                let i = self.idx(x, y, z);
                self.bp.cells[i] = below;
            }
        }
    }
}

// ---- art: glyph pictures (logos, title art, signs) ------------------------------------------------

/// The art palette: night black first, then inks.
pub const PALETTE: [Rgb; 16] = [
    Rgb(0, 0, 0), Rgb(222, 214, 196), Rgb(120, 112, 100), Rgb(60, 56, 50),
    Rgb(220, 180, 90), Rgb(200, 120, 50), Rgb(220, 90, 70), Rgb(150, 40, 40),
    Rgb(120, 190, 90), Rgb(50, 110, 60), Rgb(110, 170, 220), Rgb(50, 80, 150),
    Rgb(180, 120, 200), Rgb(110, 200, 190), Rgb(170, 130, 80), Rgb(255, 255, 255),
];

#[derive(Clone)]
pub struct Art {
    pub name: String,
    pub w: i32,
    pub h: i32,
    pub cells: Vec<Cell>,
}

impl Art {
    pub fn new(name: &str, w: i32, h: i32) -> Art {
        Art { name: name.into(), w, h, cells: vec![Cell::new(' ', PALETTE[1], PALETTE[0]); (w * h) as usize] }
    }
    pub fn inside(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.w && y < self.h
    }
    pub fn text(&self) -> String {
        // Every colour used gets a key; then the glyphs, the inks and the papers, a row a line.
        let keys: Vec<char> = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789".chars().collect();
        let mut cols: Vec<Rgb> = Vec::new();
        for c in &self.cells {
            for x in [c.fg, c.bg] {
                if !cols.contains(&x) && cols.len() < keys.len() {
                    cols.push(x);
                }
            }
        }
        let key = |x: Rgb| keys[cols.iter().position(|c| *c == x).unwrap_or(0)];
        let mut s = format!("tachyon-art 1\nname {}\nsize {} {}\n", self.name, self.w, self.h);
        for (i, c) in cols.iter().enumerate() {
            s += &format!("color {} {} {} {}\n", keys[i], c.0, c.1, c.2);
        }
        for (title, f) in [("glyphs", 0), ("ink", 1), ("paper", 2)] {
            s += title;
            s.push('\n');
            for y in 0..self.h {
                for x in 0..self.w {
                    let c = self.cells[(y * self.w + x) as usize];
                    s.push(match f { 0 => c.ch, 1 => key(c.fg), _ => key(c.bg) });
                }
                s.push('\n');
            }
        }
        s + "end\n"
    }
    pub fn from_text(text: &str) -> Result<Art, String> {
        let mut lines = text.lines();
        if !lines.next().unwrap_or("").starts_with("tachyon-art") {
            return Err("not Tachyon art".into());
        }
        let (mut name, mut w, mut h) = (String::from("art"), 0, 0);
        let mut cols: Vec<(char, Rgb)> = Vec::new();
        let mut grids: Vec<Vec<Vec<char>>> = Vec::new();
        while let Some(l) = lines.next() {
            let (word, rest) = l.split_once(' ').unwrap_or((l, ""));
            match word {
                "name" => name = rest.into(),
                "size" => {
                    let v: Vec<i32> = rest.split_whitespace().filter_map(|x| x.parse().ok()).collect();
                    (w, h) = (*v.first().unwrap_or(&0), *v.get(1).unwrap_or(&0));
                }
                "color" => {
                    let mut it = rest.splitn(2, ' ');
                    let k = it.next().and_then(|k| k.chars().next()).unwrap_or('a');
                    cols.push((k, rgb(it.next().unwrap_or("")).unwrap_or(DEFAULT_INK)));
                }
                "glyphs" | "ink" | "paper" => grids.push((0..h).map(|_| lines.next().unwrap_or("").chars().collect()).collect()),
                "end" => break,
                _ => {}
            }
        }
        if w <= 0 || h <= 0 || grids.len() < 3 {
            return Err("art has no size or rows".into());
        }
        let col = |k: char| cols.iter().find(|c| c.0 == k).map(|c| c.1).unwrap_or(DEFAULT_INK);
        let mut art = Art::new(&name, w, h);
        for y in 0..h as usize {
            for x in 0..w as usize {
                let g = |n: usize| grids[n].get(y).and_then(|r| r.get(x)).copied().unwrap_or(' ');
                art.cells[y * w as usize + x] = Cell::new(g(0), col(g(1)), col(g(2)));
            }
        }
        Ok(art)
    }
}
