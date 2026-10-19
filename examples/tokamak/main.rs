//! The Tokamak: Tachyon's workshop for making things, for any game.
//!
//! A project is a folder under `tokamak/`: a library of materials and kinds
//! (what things are made of, and the things themselves: a glyph and colours
//! each, and tags), and parts: pieces built cell by cell over several levels,
//! with sockets where they join other parts (a castle wall to a tower, a deck
//! to a hull), for games and procedural generation to put together.
//!
//!     cargo run --release --example tokamak                 (the last project, or "default")
//!     cargo run --release --example tokamak -- broadside    (or any project)
//!     tokamak.bat [project]                                 (the same, from Explorer)
//!
//! Art saves as `tokamak/<project>/art/<name>.tart`: the glyphs, then their
//! colours as a small palette, all plain text.
//!
//! Parts save as `tokamak/<project>/parts/<name>.tpart`: a Tachyon blueprint
//! (any game can stamp it) with its sockets after the `end` line.
//!
//! Press ? (or F1) in the Tokamak for every key. The ones that matter most:
//!   Tab or click the tabs: Part, Library, Art     mouse: left paints, right clears
//!   Ctrl+Z undo  Ctrl+Y redo  Ctrl+S save  Ctrl+O open  Ctrl+N new  Ctrl+P project
//!   r rectangle, b room/box (press at two corners)  F flood fill  y mirror

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use tachyon::blueprint::{Blueprint, BpCell};
use tachyon::prelude::*;
use tachyon::tile::Tile;

const PANEL_W: i32 = 34;
const BG: Rgb = Rgb(14, 13, 12);
const INK: Rgb = Rgb(222, 214, 196);
const DIM: Rgb = Rgb(120, 112, 100);
const FAINT: Rgb = Rgb(40, 38, 34);
const GOLD: Rgb = Rgb(220, 180, 90);
const RED: Rgb = Rgb(220, 100, 80);
const UNDO_DEPTH: usize = 200;

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
        // Fields part at " | ", so a glyph can itself be a | (a mast).
        let f: Vec<&str> = rest.split(" | ").map(|s| s.trim().trim_end_matches(" |")).collect();
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

#[derive(Clone)]
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
    fn inside(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.bp.w && y < self.bp.h
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
    /// The level below, built again on level `z`: walls and floors carry up, stairs up meet
    /// stairs down, and the things stay where they were.
    fn copy_level_up(&mut self, z: i32) {
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
const PALETTE: [Rgb; 16] = [
    Rgb(0, 0, 0), Rgb(222, 214, 196), Rgb(120, 112, 100), Rgb(60, 56, 50),
    Rgb(220, 180, 90), Rgb(200, 120, 50), Rgb(220, 90, 70), Rgb(150, 40, 40),
    Rgb(120, 190, 90), Rgb(50, 110, 60), Rgb(110, 170, 220), Rgb(50, 80, 150),
    Rgb(180, 120, 200), Rgb(110, 200, 190), Rgb(170, 130, 80), Rgb(255, 255, 255),
];

fn nearest(c: Rgb) -> usize {
    let d = |p: &Rgb| (p.0 as i32 - c.0 as i32).pow(2) + (p.1 as i32 - c.1 as i32).pow(2) + (p.2 as i32 - c.2 as i32).pow(2);
    (0..PALETTE.len()).min_by_key(|&i| d(&PALETTE[i])).unwrap_or(1)
}

/// The glyph seen in a mirror, for symmetric art.
fn flip(c: char) -> char {
    match c {
        '(' => ')', ')' => '(', '[' => ']', ']' => '[', '{' => '}', '}' => '{',
        '<' => '>', '>' => '<', '/' => '\\', '\\' => '/', 'd' => 'b', 'b' => 'd', 'p' => 'q', 'q' => 'p',
        c => c,
    }
}

#[derive(Clone)]
struct Art {
    name: String,
    w: i32,
    h: i32,
    cells: Vec<Cell>,
}

impl Art {
    fn new(name: &str, w: i32, h: i32) -> Art {
        Art { name: name.into(), w, h, cells: vec![Cell::new(' ', PALETTE[1], PALETTE[0]); (w * h) as usize] }
    }
    fn inside(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.w && y < self.h
    }
    fn text(&self) -> String {
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
    fn from_text(text: &str) -> Result<Art, String> {
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
                    cols.push((k, rgb(it.next().unwrap_or("")).unwrap_or(INK)));
                }
                "glyphs" | "ink" | "paper" => grids.push((0..h).map(|_| lines.next().unwrap_or("").chars().collect()).collect()),
                "end" => break,
                _ => {}
            }
        }
        if w <= 0 || h <= 0 || grids.len() < 3 {
            return Err("art has no size or rows".into());
        }
        let col = |k: char| cols.iter().find(|c| c.0 == k).map(|c| c.1).unwrap_or(INK);
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

/// The cells of a 4-connected patch on a `w` x `h` grid that match the one at `(x, y)`.
fn flood(w: i32, h: i32, x: i32, y: i32, same: impl Fn(i32, i32) -> bool) -> Vec<(i32, i32)> {
    let mut seen = vec![false; (w * h) as usize];
    let (mut out, mut todo) = (Vec::new(), vec![(x, y)]);
    while let Some((x, y)) = todo.pop() {
        if x < 0 || y < 0 || x >= w || y >= h || seen[(y * w + x) as usize] || !same(x, y) {
            continue;
        }
        seen[(y * w + x) as usize] = true;
        out.push((x, y));
        todo.extend([(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)]);
    }
    out
}

/// The files in `dir` ending `.ext`, the most recently changed first.
fn list_files(dir: &Path, ext: &str) -> Vec<String> {
    let mut v: Vec<(std::time::SystemTime, String)> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if p.extension()? != ext {
                return None;
            }
            Some((e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH), p.file_stem()?.to_string_lossy().into_owned()))
        })
        .collect();
    v.sort_by(|a, b| b.0.cmp(&a.0));
    v.into_iter().map(|x| x.1).collect()
}

fn list_projects() -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir("tokamak").into_iter().flatten().flatten().filter(|e| e.path().is_dir()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    v.sort();
    v
}

// ---- the workshop ----------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Debug)]
enum Mode {
    Part,
    Library,
    Art,
}

const MODES: [Mode; 3] = [Mode::Part, Mode::Library, Mode::Art];

impl Mode {
    fn i(self) -> usize {
        self as usize
    }
    fn name(self) -> &'static str {
        ["Part", "Library", "Art"][self.i()]
    }
}

/// What undo puts back.
#[derive(Clone)]
enum Snap {
    Part(Part),
    Lib(Vec<Entry>),
    Art(Art),
}

/// Something that throws away unsaved work, so it asks first.
#[derive(Clone, PartialEq, Debug)]
enum Act {
    Quit,
    NewPart(String, i32, i32, i32),
    NewArt(String, i32, i32),
    OpenPart(String),
    OpenArt(String),
    Project(String),
}

#[derive(Clone, Copy, PartialEq)]
enum Listing {
    Parts,
    Arts,
    Projects,
}

struct Picker {
    what: Listing,
    items: Vec<String>,
    sel: usize,
    filter: String,
}

impl Picker {
    fn shown(&self) -> Vec<String> {
        let f = self.filter.to_lowercase();
        self.items.iter().filter(|s| s.to_lowercase().contains(&f)).cloned().collect()
    }
}

/// A place on screen the mouse can click.
#[derive(Clone, Copy, PartialEq)]
enum Hit {
    Mode(Mode),
    Tile(Tile),
    Mat(usize),
    Thing(usize),
    Colour(usize),
    Lib(usize),
    Item(usize),
}

#[derive(Clone, PartialEq)]
enum Ask {
    New,
    Rename,
    Tags,
    Colours,
    NewEntry(bool),
    SocketTag,
    Glyph(bool),
    ArtGlyph,
}

const TILES: [(&str, Tile); 5] = [("wall", Tile::Wall), ("floor", Tile::Floor), ("open", Tile::Open), ("up", Tile::StairUp), ("down", Tile::StairDown)];

const HELP_PART: &[&str] = &[
    "Move: arrows or the mouse.   Paint: Space or left-click.   Clear: Del or right-click.",
    "Brush: w wall  f floor  o open  u stairs up  n stairs down   t takes the brush from the cursor",
    "Material: [ ]   Thing: { }   k things on/off   (or click any of them in the panel)",
    "",
    "r  rectangle: r at one corner, r (or a click) at the other; fills it with the brush",
    "b  room: b at one corner, b at the other; walls round the edge, floor inside",
    "F  flood fill the patch under the cursor     y  mirror left and right while painting",
    "c  build the level below again up here (stairs up below become stairs down here)",
    "",
    "Levels: PgUp PgDn or the mouse wheel     +  a new level on top     -  remove the top level",
    "Sockets: s adds one here (s again turns it, x removes it); a tag says what fits there",
    "",
    "Ctrl+Z undo  Ctrl+Y redo  Ctrl+S save  Ctrl+N new part  Ctrl+O open  Ctrl+P project",
];

const HELP_LIBRARY: &[&str] = &[
    "Choose: up down, the mouse or the wheel.",
    "m new material   k new kind   d duplicate   r rename   e tags   Del remove",
    "g then a key: the glyph       G then a key: the floor glyph (materials)",
    "c  colours typed as numbers: 200 150 90 / 20 18 16 (ink, then background)",
    "1 2  red down, up     3 4  green     5 6  blue     (hold Ctrl or Alt: the background)",
    "",
    "Ctrl+Z undo  Ctrl+Y redo  Ctrl+S save the library  Ctrl+P project",
];

const HELP_ART: &[&str] = &[
    "Move: arrows or the mouse.   Paint: Space or left-click.   Clear: Del or right-click.",
    "g then a key: the brush glyph     t takes glyph and colours from the cursor",
    "c C ink colour   v V paper colour   (or click the palette: left ink, right paper)",
    "Enter: type text at the cursor (Enter or Esc stops)",
    "",
    "r  rectangle filled with the brush (press at two corners)",
    "b  box outline in + - |  (press at two corners)",
    "F  flood fill     y  mirror left and right (brackets and slashes turn round)",
    "",
    "Ctrl+Z undo  Ctrl+Y redo  Ctrl+S save  Ctrl+N new art  Ctrl+O open  Ctrl+P project",
];

struct Tokamak {
    project: String,
    dir: PathBuf,
    lib: Vec<Entry>,
    part: Part,
    mode: Mode,
    cx: i32,
    cy: i32,
    z: i32,
    poff: (i32, i32),
    tile: Tile,
    mat: usize,
    thing: usize,
    with_thing: bool,
    pick: usize,
    loff: i32,
    ask: Option<(Ask, String)>,
    note: String,
    view: (i32, i32, i32, i32),
    art: Art,
    ax: i32,
    ay: i32,
    aoff: (i32, i32),
    ink: usize,
    paper: usize,
    aglyph: char,
    typing: bool,
    hist: [(Vec<Snap>, Vec<Snap>); 3],
    dirty: [bool; 3],
    confirm: Option<Act>,
    picker: Option<Picker>,
    help: bool,
    /// A corner set by r or b, and whether it is a room (b).
    anchor: Option<(i32, i32, bool)>,
    mirror: bool,
    stroke: bool,
    hits: Vec<(i32, i32, i32, Hit)>,
    last_tag: String,
}

impl Tokamak {
    fn blank(project: &str, lib: Vec<Entry>) -> Tokamak {
        Tokamak {
            project: project.into(),
            dir: PathBuf::from("tokamak").join(project),
            lib,
            part: Part::new("untitled", 16, 10, 2),
            mode: Mode::Part,
            cx: 0,
            cy: 0,
            z: 0,
            poff: (0, 0),
            tile: Tile::Wall,
            mat: 0,
            thing: 0,
            with_thing: false,
            pick: 0,
            loff: 0,
            ask: None,
            note: String::new(),
            view: (0, 0, 0, 0),
            art: Art::new("logo", 40, 10),
            ax: 0,
            ay: 0,
            aoff: (0, 0),
            ink: 1,
            paper: 0,
            aglyph: '#',
            typing: false,
            hist: Default::default(),
            dirty: [false; 3],
            confirm: None,
            picker: None,
            help: false,
            anchor: None,
            mirror: false,
            stroke: false,
            hits: Vec::new(),
            last_tag: "wall".into(),
        }
    }

    /// Opens a project (making it if new), with its most recent part and art.
    fn load_project(&mut self, project: &str) -> io::Result<()> {
        let dir = PathBuf::from("tokamak").join(project);
        fs::create_dir_all(dir.join("parts"))?;
        fs::create_dir_all(dir.join("art"))?;
        let lib_path = dir.join("library.tlib");
        if !lib_path.exists() {
            fs::write(&lib_path, SEED_LIBRARY)?;
        }
        let fresh = Tokamak::blank(project, parse_library(&fs::read_to_string(&lib_path)?));
        let mode = self.mode;
        *self = fresh;
        self.mode = mode;
        self.dir = dir;
        let _ = fs::write(Path::new("tokamak").join(".last"), project);
        let mut opened = Vec::new();
        if let Some(name) = list_files(&self.dir.join("parts"), "tpart").first() {
            self.open_part(name);
            opened.push(format!("part {name}"));
        }
        if let Some(name) = list_files(&self.dir.join("art"), "tart").first() {
            self.open_art(name);
            opened.push(format!("art {name}"));
        }
        self.note = if opened.is_empty() {
            format!("Project {project}. Ctrl+N starts a part, ? shows every key.")
        } else {
            format!("Project {project}: back to {}. ? shows every key.", opened.join(" and "))
        };
        Ok(())
    }

    fn mats(&self) -> Vec<usize> {
        (0..self.lib.len()).filter(|&i| !self.lib[i].kind).collect()
    }
    fn kinds(&self) -> Vec<usize> {
        (0..self.lib.len()).filter(|&i| self.lib[i].kind).collect()
    }
    /// The library in the order it is shown: materials, then kinds.
    fn order(&self) -> Vec<usize> {
        let mut v = self.mats();
        v.extend(self.kinds());
        v
    }
    fn entry(&self, name: &str, kind: bool) -> Option<&Entry> {
        self.lib.iter().find(|e| e.kind == kind && e.name == name)
    }
    fn mat_name(&self) -> String {
        let mats = self.mats();
        mats.get(self.mat % mats.len().max(1)).map(|&i| self.lib[i].name.clone()).unwrap_or_default()
    }
    fn brush(&self) -> Option<BpCell> {
        let kinds = self.kinds();
        let things = if self.with_thing { kinds.get(self.thing % kinds.len().max(1)).map(|&i| vec![self.lib[i].name.clone()]).unwrap_or_default() } else { vec![] };
        Some(BpCell { tile: self.tile, mat: if self.tile == Tile::Open { String::new() } else { self.mat_name() }, things })
    }

    // -- undo, and work not yet saved --

    fn snap(&self, mode: Mode) -> Snap {
        match mode {
            Mode::Part => Snap::Part(self.part.clone()),
            Mode::Library => Snap::Lib(self.lib.clone()),
            Mode::Art => Snap::Art(self.art.clone()),
        }
    }
    /// Call before any change: it can be undone, and it marks the work unsaved.
    fn checkpoint(&mut self) {
        let m = self.mode.i();
        let s = self.snap(self.mode);
        let (undo, redo) = &mut self.hist[m];
        undo.push(s);
        if undo.len() > UNDO_DEPTH {
            undo.remove(0);
        }
        redo.clear();
        self.dirty[m] = true;
    }
    fn undo(&mut self, back: bool) {
        let m = self.mode.i();
        let popped = if back { self.hist[m].0.pop() } else { self.hist[m].1.pop() };
        let Some(s) = popped else {
            self.note = format!("Nothing to {}", if back { "undo" } else { "redo" });
            return;
        };
        let cur = self.snap(self.mode);
        if back { self.hist[m].1.push(cur) } else { self.hist[m].0.push(cur) }
        match s {
            Snap::Part(p) => self.part = p,
            Snap::Lib(l) => self.lib = l,
            Snap::Art(a) => self.art = a,
        }
        self.dirty[m] = true;
        self.cx = self.cx.min(self.part.bp.w - 1);
        self.cy = self.cy.min(self.part.bp.h - 1);
        self.z = self.z.min(self.part.bp.d - 1);
        self.ax = self.ax.min(self.art.w - 1);
        self.ay = self.ay.min(self.art.h - 1);
        self.pick = self.pick.min(self.lib.len().saturating_sub(1));
        self.note = if back { "Undone".into() } else { "Redone".into() };
    }
    fn guard(&mut self, act: Act) -> Flow {
        let unsaved = match act {
            Act::Quit | Act::Project(_) => self.dirty.iter().any(|d| *d),
            Act::NewPart(..) | Act::OpenPart(_) => self.dirty[Mode::Part.i()],
            Act::NewArt(..) | Act::OpenArt(_) => self.dirty[Mode::Art.i()],
        };
        if unsaved {
            self.confirm = Some(act);
            return Flow::Continue;
        }
        self.run(act)
    }
    fn run(&mut self, act: Act) -> Flow {
        match act {
            Act::Quit => return Flow::Quit,
            Act::NewPart(name, w, h, d) => {
                self.part = Part::new(&name, w, h, d);
                (self.cx, self.cy, self.z) = (0, 0, 0);
                self.reset(Mode::Part);
                self.note = format!("New part {name}, {w} x {h}, {d} level(s)");
            }
            Act::NewArt(name, w, h) => {
                self.art = Art::new(&name, w, h);
                (self.ax, self.ay) = (0, 0);
                self.reset(Mode::Art);
                self.note = format!("New art {name}, {w} x {h}");
            }
            Act::OpenPart(name) => self.open_part(&name),
            Act::OpenArt(name) => self.open_art(&name),
            Act::Project(name) => {
                if let Err(e) = self.load_project(&name) {
                    self.note = format!("Could not open project {name}: {e}");
                }
            }
        }
        Flow::Continue
    }
    fn reset(&mut self, mode: Mode) {
        self.hist[mode.i()] = Default::default();
        self.dirty[mode.i()] = false;
        self.anchor = None;
    }
    fn open_part(&mut self, name: &str) {
        let path = self.dir.join("parts").join(format!("{name}.tpart"));
        match fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|s| Part::from_text(&s)) {
            Ok(p) => {
                self.part = p;
                (self.cx, self.cy, self.z) = (0, 0, 0);
                self.reset(Mode::Part);
                self.note = format!("Opened part {name}");
            }
            Err(e) => self.note = format!("Could not open {name}: {e}"),
        }
    }
    fn open_art(&mut self, name: &str) {
        let path = self.dir.join("art").join(format!("{name}.tart"));
        match fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|s| Art::from_text(&s)) {
            Ok(a) => {
                self.art = a;
                (self.ax, self.ay) = (0, 0);
                self.reset(Mode::Art);
                self.note = format!("Opened art {name}");
            }
            Err(e) => self.note = format!("Could not open {name}: {e}"),
        }
    }
    fn save(&mut self, mode: Mode) {
        let (path, text) = match mode {
            Mode::Part => (self.dir.join("parts").join(format!("{}.tpart", self.part.bp.name)), self.part.text()),
            Mode::Library => (self.dir.join("library.tlib"), library_text(&self.project, &self.lib)),
            Mode::Art => (self.dir.join("art").join(format!("{}.tart", self.art.name)), self.art.text()),
        };
        if let Some(d) = path.parent() {
            let _ = fs::create_dir_all(d);
        }
        self.note = match fs::write(&path, text) {
            Ok(_) => {
                self.dirty[mode.i()] = false;
                format!("Saved {}", path.display())
            }
            Err(e) => format!("Could not save: {e}"),
        };
    }
    fn save_all(&mut self) {
        for m in MODES {
            if self.dirty[m.i()] {
                self.save(m);
            }
        }
    }

    // -- painting parts --

    /// Sets a cell on the current level, and its mirror image when mirroring.
    fn put(&mut self, x: i32, y: i32, cell: Option<BpCell>) {
        let z = self.z;
        for x in if self.mirror { vec![x, self.part.bp.w - 1 - x] } else { vec![x] } {
            if self.part.inside(x, y) {
                let i = self.part.idx(x, y, z);
                self.part.bp.cells[i] = cell.clone();
            }
        }
    }
    fn paint(&mut self, cell: Option<BpCell>) {
        self.put(self.cx, self.cy, cell);
    }
    /// The rectangle from the anchor to the cursor: the brush, or a room (walls round, floor in).
    fn part_rect(&mut self, (x0, y0, room): (i32, i32, bool)) {
        self.checkpoint();
        let (xa, xb, ya, yb) = (x0.min(self.cx), x0.max(self.cx), y0.min(self.cy), y0.max(self.cy));
        let brush = self.brush();
        let mat = self.mat_name();
        for y in ya..=yb {
            for x in xa..=xb {
                let edge = x == xa || x == xb || y == ya || y == yb;
                let cell = if room { Some(BpCell { tile: if edge { Tile::Wall } else { Tile::Floor }, mat: mat.clone(), things: vec![] }) } else { brush.clone() };
                self.put(x, y, cell);
            }
        }
        self.note = format!("{} {} x {}", if room { "Room" } else { "Rectangle" }, xb - xa + 1, yb - ya + 1);
    }
    fn part_flood(&mut self) {
        self.checkpoint();
        let p = &self.part;
        let (z, b) = (self.z, self.brush());
        let mut seeds = vec![(self.cx, self.cy)];
        if self.mirror {
            seeds.push((p.bp.w - 1 - self.cx, self.cy));
        }
        let mut cells = Vec::new();
        for (sx, sy) in seeds {
            let target = p.bp.cells[p.idx(sx, sy, z)].clone();
            cells.extend(flood(p.bp.w, p.bp.h, sx, sy, |x, y| p.bp.cells[p.idx(x, y, z)] == target));
        }
        let n = cells.len();
        for (x, y) in cells {
            let i = self.part.idx(x, y, z);
            self.part.bp.cells[i] = b.clone();
        }
        self.note = format!("Filled {n} cells");
    }

    // -- painting art --

    fn art_set(&mut self, x: i32, y: i32, ch: char) {
        let c = Cell::new(ch, PALETTE[self.ink], PALETTE[self.paper]);
        let mut at = vec![(x, ch)];
        if self.mirror {
            at.push((self.art.w - 1 - x, flip(ch)));
        }
        for (x, ch) in at {
            if self.art.inside(x, y) {
                self.art.cells[(y * self.art.w + x) as usize] = Cell { ch, ..c };
            }
        }
    }
    fn art_put(&mut self, ch: char) {
        self.art_set(self.ax, self.ay, ch);
    }
    fn art_rect(&mut self, (x0, y0, boxed): (i32, i32, bool)) {
        self.checkpoint();
        let (xa, xb, ya, yb) = (x0.min(self.ax), x0.max(self.ax), y0.min(self.ay), y0.max(self.ay));
        for y in ya..=yb {
            for x in xa..=xb {
                let (ex, ey) = (x == xa || x == xb, y == ya || y == yb);
                let ch = match (boxed, ex, ey) {
                    (false, _, _) => self.aglyph,
                    (true, true, true) => '+',
                    (true, false, true) => '-',
                    (true, true, false) => '|',
                    _ => continue,
                };
                self.art_set(x, y, ch);
            }
        }
    }
    fn art_flood(&mut self) {
        self.checkpoint();
        let a = &self.art;
        let target = a.cells[(self.ay * a.w + self.ax) as usize];
        let cells = flood(a.w, a.h, self.ax, self.ay, |x, y| a.cells[(y * a.w + x) as usize] == target);
        let n = cells.len();
        for (x, y) in cells {
            self.art_set(x, y, self.aglyph);
        }
        self.note = format!("Filled {n} cells");
    }

    fn answer(&mut self, what: Ask, text: String) -> Flow {
        let t = text.trim().to_string();
        match what {
            Ask::New => {
                let v: Vec<&str> = t.split_whitespace().collect();
                let n = |i: usize, d: i32| v.get(i).and_then(|s| s.parse().ok()).unwrap_or(d);
                if let Some(name) = v.first() {
                    let act = if self.mode == Mode::Art {
                        Act::NewArt(name.to_string(), n(1, 40).clamp(1, 300), n(2, 10).clamp(1, 200))
                    } else {
                        Act::NewPart(name.to_string(), n(1, 16).clamp(1, 200), n(2, 10).clamp(1, 200), n(3, 1).clamp(1, 32))
                    };
                    return self.guard(act);
                }
            }
            Ask::Rename | Ask::Tags | Ask::Colours => {
                if t.is_empty() || self.pick >= self.lib.len() {
                    return Flow::Continue;
                }
                self.checkpoint();
                let e = &mut self.lib[self.pick];
                match what {
                    Ask::Rename => e.name = t,
                    Ask::Tags => e.tags = t.split_whitespace().map(String::from).collect(),
                    _ => {
                        let mut halves = t.split('/');
                        if let Some(c) = halves.next().and_then(rgb) {
                            e.fg = c;
                        }
                        if let Some(c) = halves.next().and_then(rgb) {
                            e.bg = c;
                        }
                    }
                }
            }
            Ask::NewEntry(kind) => {
                if !t.is_empty() {
                    self.checkpoint();
                    self.lib.push(Entry { kind, name: t, glyph: if kind { '?' } else { '#' }, floor: '.', fg: INK, bg: if kind { Rgb(0, 0, 0) } else { BG }, tags: vec![] });
                    self.pick = self.lib.len() - 1;
                    self.note = "Now g for its glyph, c for its colours, e for its tags".into();
                }
            }
            Ask::SocketTag => {
                self.checkpoint();
                let (x, y, z) = (self.cx, self.cy, self.z);
                let tag = if t.is_empty() { "any".to_string() } else { t.replace(' ', "-") };
                self.last_tag = tag.clone();
                self.part.sockets.retain(|k| !(k.x == x && k.y == y && k.z == z));
                self.part.sockets.push(Socket { x, y, z, dir: 0, tag });
                self.note = "Socket added: s again turns it".into();
            }
            Ask::Glyph(_) | Ask::ArtGlyph => {}
        }
        Flow::Continue
    }

    fn open_picker(&mut self, what: Listing) {
        let items = match what {
            Listing::Parts => list_files(&self.dir.join("parts"), "tpart"),
            Listing::Arts => list_files(&self.dir.join("art"), "tart"),
            Listing::Projects => list_projects(),
        };
        self.picker = Some(Picker { what, items, sel: 0, filter: String::new() });
    }
    fn pick_item(&mut self, i: usize) -> Flow {
        let Some(p) = self.picker.take() else { return Flow::Continue };
        let shown = p.shown();
        let name = match shown.get(i) {
            Some(s) => s.clone(),
            None if p.what == Listing::Projects && !p.filter.trim().is_empty() => p.filter.trim().replace(' ', "-"),
            None => {
                self.note = "Nothing by that name".into();
                return Flow::Continue;
            }
        };
        self.guard(match p.what {
            Listing::Parts => Act::OpenPart(name),
            Listing::Arts => Act::OpenArt(name),
            Listing::Projects => Act::Project(name),
        })
    }
    fn picker_key(&mut self, k: KeyEvent) -> Flow {
        let Some(p) = self.picker.as_mut() else { return Flow::Continue };
        let n = p.shown().len().max(1);
        match k.key {
            Key::Esc => self.picker = None,
            Key::Enter => {
                let sel = p.sel;
                return self.pick_item(sel);
            }
            Key::Up => p.sel = (p.sel + n - 1) % n,
            Key::Down => p.sel = (p.sel + 1) % n,
            Key::Backspace => {
                p.filter.pop();
                p.sel = 0;
            }
            Key::Char(c) if !k.ctrl => {
                p.filter.push(c);
                p.sel = 0;
            }
            _ => {}
        }
        Flow::Continue
    }
}

impl Game for Tokamak {
    fn tick(&mut self) {}

    fn ticks_per_second(&self) -> f64 {
        1.0
    }

    fn on_key(&mut self, k: KeyEvent) -> Flow {
        // Unsaved work: y throws it away, s saves it first, anything else keeps it.
        if let Some(act) = self.confirm.take() {
            return match k.key {
                Key::Char('y') => self.run(act),
                Key::Char('s') => {
                    self.save_all();
                    self.run(act)
                }
                _ => {
                    self.note = "Kept it".into();
                    Flow::Continue
                }
            };
        }
        if self.help {
            self.help = false;
            return Flow::Continue;
        }
        if self.picker.is_some() {
            return self.picker_key(k);
        }
        // A question being answered.
        if let Some((what, mut text)) = self.ask.take() {
            if what == Ask::ArtGlyph {
                if let Key::Char(c) = k.key {
                    self.aglyph = c;
                }
                return Flow::Continue;
            }
            if let Ask::Glyph(floor) = what {
                if let Key::Char(c) = k.key {
                    self.checkpoint();
                    if let Some(e) = self.lib.get_mut(self.pick) {
                        if floor { e.floor = c } else { e.glyph = c }
                    }
                }
                return Flow::Continue;
            }
            match k.key {
                Key::Enter => return self.answer(what, text),
                Key::Esc => {}
                Key::Backspace => {
                    text.pop();
                    self.ask = Some((what, text));
                }
                Key::Char('u') if k.ctrl => self.ask = Some((what, String::new())),
                Key::Char(c) if !k.ctrl => {
                    text.push(c);
                    self.ask = Some((what, text));
                }
                _ => self.ask = Some((what, text)),
            }
            return Flow::Continue;
        }
        if k.key == Key::Esc {
            if self.typing {
                self.typing = false;
            } else if self.anchor.take().is_some() {
                self.note = "Corner dropped".into();
            } else {
                return self.guard(Act::Quit);
            }
            return Flow::Continue;
        }
        if k.key == Key::F(1) {
            self.help = true;
            return Flow::Continue;
        }
        if matches!(k.key, Key::Tab | Key::BackTab) {
            let step = if k.key == Key::BackTab || k.shift { 2 } else { 1 };
            self.mode = MODES[(self.mode.i() + step) % 3];
            self.typing = false;
            self.anchor = None;
            return Flow::Continue;
        }
        if k.ctrl {
            match k.key {
                Key::Char('s') => self.save(self.mode),
                Key::Char('n') => self.ask = Some((Ask::New, String::new())),
                Key::Char('o') => self.open_picker(if self.mode == Mode::Art { Listing::Arts } else { Listing::Parts }),
                Key::Char('p') => self.open_picker(Listing::Projects),
                Key::Char('z') => self.undo(true),
                Key::Char('y') => self.undo(false),
                _ => {}
            }
            return Flow::Continue;
        }
        if k.key == Key::Char('?') && !self.typing {
            self.help = true;
            return Flow::Continue;
        }
        match self.mode {
            Mode::Library => self.library_key(k),
            Mode::Part => self.part_key(k),
            Mode::Art => self.art_key(k),
        }
        Flow::Continue
    }

    fn on_mouse(&mut self, m: MouseEvent) -> Flow {
        let left = matches!(m.kind, MouseKind::Down(MouseButton::Left));
        let right = matches!(m.kind, MouseKind::Down(MouseButton::Right));
        if self.help && (left || right) {
            self.help = false;
            return Flow::Continue;
        }
        if matches!(m.kind, MouseKind::Up(_)) {
            self.stroke = false;
        }
        // The panels, tabs and lists.
        if left || right {
            if let Some(&(_, _, _, hit)) = self.hits.iter().find(|(x, y, w, _)| m.y == *y && m.x >= *x && m.x < x + w) {
                match hit {
                    Hit::Mode(md) => {
                        self.mode = md;
                        self.typing = false;
                        self.anchor = None;
                    }
                    Hit::Tile(t) => self.tile = t,
                    Hit::Mat(k) => self.mat = k,
                    Hit::Thing(k) => {
                        self.with_thing = !(self.with_thing && self.thing == k);
                        self.thing = k;
                    }
                    Hit::Colour(i) => if right { self.paper = i } else { self.ink = i },
                    Hit::Lib(i) => self.pick = i,
                    Hit::Item(i) => return self.pick_item(i),
                }
                return Flow::Continue;
            }
        }
        let scroll = match m.kind { MouseKind::ScrollUp => 1, MouseKind::ScrollDown => -1, _ => 0 };
        if self.picker.is_some() || self.confirm.is_some() || self.ask.is_some() {
            if let (Some(p), true) = (self.picker.as_mut(), scroll != 0) {
                let n = p.shown().len().max(1);
                p.sel = if scroll > 0 { (p.sel + n - 1) % n } else { (p.sel + 1) % n };
            }
            return Flow::Continue;
        }
        if self.mode == Mode::Library && scroll != 0 {
            let order = self.order();
            if let Some(at) = order.iter().position(|&i| i == self.pick) {
                let at = (at as i32 - scroll).clamp(0, order.len() as i32 - 1);
                self.pick = order[at as usize];
            }
            return Flow::Continue;
        }
        let (vx, vy, vw, vh) = self.view;
        if m.x < vx || m.y < vy || m.x >= vx + vw || m.y >= vy + vh {
            return Flow::Continue;
        }
        let paint = matches!(m.kind, MouseKind::Drag(MouseButton::Left)) && self.stroke;
        let clear = matches!(m.kind, MouseKind::Drag(MouseButton::Right)) && self.stroke;
        match self.mode {
            Mode::Art => {
                (self.ax, self.ay) = ((m.x - vx + self.aoff.0).min(self.art.w - 1), (m.y - vy + self.aoff.1).min(self.art.h - 1));
                if left && self.anchor.is_some() {
                    let a = self.anchor.take().unwrap();
                    self.art_rect(a);
                } else if left || right {
                    self.checkpoint();
                    self.stroke = true;
                    self.art_put(if left { self.aglyph } else { ' ' });
                } else if paint || clear {
                    self.art_put(if paint { self.aglyph } else { ' ' });
                }
            }
            Mode::Part => {
                if scroll != 0 {
                    self.z = (self.z + scroll).clamp(0, self.part.bp.d - 1);
                    return Flow::Continue;
                }
                (self.cx, self.cy) = ((m.x - vx + self.poff.0).min(self.part.bp.w - 1), (m.y - vy + self.poff.1).min(self.part.bp.h - 1));
                if left && self.anchor.is_some() {
                    let a = self.anchor.take().unwrap();
                    self.part_rect(a);
                } else if left || right {
                    self.checkpoint();
                    self.stroke = true;
                    let b = if left { self.brush() } else { None };
                    self.paint(b);
                } else if paint || clear {
                    let b = if paint { self.brush() } else { None };
                    self.paint(b);
                }
            }
            Mode::Library => {}
        }
        Flow::Continue
    }

    fn draw(&mut self, f: &mut Frame, _stats: &LoopStats) {
        f.clear(BG);
        self.hits.clear();
        let (fw, fh) = (f.width(), f.height());
        let canvas = Rect { x: 0, y: 1, w: (fw - PANEL_W).max(10), h: fh - 3 };

        // The top bar: the project, then a tab for each mode (a * when it has unsaved work).
        let title = format!("The Tokamak · {} ", self.project);
        f.text_clip(1, 0, fw - 2, &title, GOLD, BG);
        let mut x = 2 + title.chars().count() as i32;
        for md in MODES {
            let label = format!(" {}{} ", md.name(), if self.dirty[md.i()] { "*" } else { "" });
            let on = md == self.mode;
            let w = label.chars().count() as i32;
            f.text_clip(x, 0, w, &label, if on { BG } else { DIM }, if on { GOLD } else { BG });
            self.hits.push((x, 0, w, Hit::Mode(md)));
            x += w + 1;
        }
        f.text_clip(x + 1, 0, fw - x - 2, "? keys  ^P project  ^O open", FAINT.lerp(INK, 0.4), BG);

        match self.mode {
            Mode::Part => self.draw_part(f, canvas),
            Mode::Library => self.draw_library(f, canvas),
            Mode::Art => self.draw_art(f, canvas),
        }
        if self.picker.is_some() {
            self.draw_picker(f, canvas);
        }
        if self.help {
            self.draw_help(f);
        }

        let mode_word = if self.mode == Mode::Art { "art" } else { "part" };
        let foot = match (&self.confirm, &self.ask) {
            (Some(_), _) => "Unsaved changes.   y: throw them away   s: save first   any other key: keep working".to_string(),
            (_, Some((Ask::ArtGlyph, _))) => "Press the key for the brush glyph".to_string(),
            (_, Some((Ask::Glyph(fl), _))) => format!("Press the key for the {} glyph", if *fl { "floor" } else { "wall/thing" }),
            (_, Some((Ask::New, t))) => {
                if self.mode == Mode::Art { format!("New art: name width height (e.g. title 60 8) > {t}_") } else { format!("New part: name width height levels (e.g. tower 9 9 3) > {t}_") }
            }
            (_, Some((Ask::Rename, t))) => format!("Rename to > {t}_   (Ctrl+U clears)"),
            (_, Some((Ask::Tags, t))) => format!("Tags, spaces between > {t}_   (Ctrl+U clears)"),
            (_, Some((Ask::Colours, t))) => format!("Colours: r g b / r g b (ink / background) > {t}_"),
            (_, Some((Ask::NewEntry(k), t))) => format!("New {} name > {t}_", if *k { "kind" } else { "material" }),
            (_, Some((Ask::SocketTag, t))) => format!("Socket tag (what fits here: wall, door, deck...) > {t}_"),
            _ if self.anchor.is_some() => format!("Corner set: move to the other corner, then the same key again (or click). Esc drops it."),
            _ if self.picker.is_some() => format!("Type to filter, Enter or click to open, Esc to close{}", if self.picker.as_ref().is_some_and(|p| p.what == Listing::Projects) { "; a new name makes a new project" } else { "" }),
            _ if self.note.is_empty() => format!("? for every key · Ctrl+N new {mode_word} · Ctrl+O open {mode_word}"),
            _ => self.note.clone(),
        };
        let alert = self.confirm.is_some() || self.ask.is_some() || self.anchor.is_some();
        f.text_clip(1, fh - 1, fw - 2, &foot, if alert { GOLD } else { DIM }, BG);
    }
}

/// Keeps `cur` in view: the scroll offset for a span of `len` seen through a window of `win`.
fn follow(off: i32, cur: i32, len: i32, win: i32) -> i32 {
    let off = if cur < off { cur } else if cur >= off + win { cur - win + 1 } else { off };
    off.clamp(0, (len - win).max(0))
}

impl Tokamak {
    fn part_key(&mut self, k: KeyEvent) {
        let (w, h, d) = (self.part.bp.w, self.part.bp.h, self.part.bp.d);
        let big = if k.shift { 5 } else { 1 };
        match k.key {
            Key::Left => self.cx = (self.cx - big).max(0),
            Key::Right => self.cx = (self.cx + big).min(w - 1),
            Key::Up => self.cy = (self.cy - big).max(0),
            Key::Down => self.cy = (self.cy + big).min(h - 1),
            Key::Home => self.cx = 0,
            Key::End => self.cx = w - 1,
            Key::PageUp => self.z = (self.z + 1).min(d - 1),
            Key::PageDown => self.z = (self.z - 1).max(0),
            Key::Delete => {
                self.checkpoint();
                self.paint(None);
            }
            Key::Char(' ') => {
                self.checkpoint();
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
            Key::Char('y') => {
                self.mirror = !self.mirror;
                self.note = format!("Mirror {}", if self.mirror { "on: painting is copied left to right" } else { "off" });
            }
            Key::Char('+') | Key::Char('=') => {
                self.checkpoint();
                self.part.resize_depth(d + 1);
                self.z = d;
                self.note = format!("Level {} added; c builds the level below again here", d + 1);
            }
            Key::Char('-') => {
                if d > 1 {
                    self.checkpoint();
                    self.part.resize_depth(d - 1);
                    self.z = self.z.min(self.part.bp.d - 1);
                }
            }
            Key::Char('c') => {
                if self.z > 0 {
                    self.checkpoint();
                    self.part.copy_level_up(self.z);
                    self.note = format!("Level {} built again on level {}", self.z, self.z + 1);
                } else {
                    self.note = "Nothing below the first level".into();
                }
            }
            Key::Char(c @ ('r' | 'b')) => match self.anchor.take() {
                Some(a) => self.part_rect(a),
                None => self.anchor = Some((self.cx, self.cy, c == 'b')),
            },
            Key::Char('F') => self.part_flood(),
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
                if self.part.sockets.iter().any(|s| s.x == x && s.y == y && s.z == z) {
                    self.checkpoint();
                    if let Some(s) = self.part.sockets.iter_mut().find(|s| s.x == x && s.y == y && s.z == z) {
                        s.dir = (s.dir + 1) % DIRS.len();
                    }
                } else {
                    self.ask = Some((Ask::SocketTag, self.last_tag.clone()));
                }
            }
            Key::Char('x') => {
                let (x, y, z) = (self.cx, self.cy, self.z);
                self.checkpoint();
                self.part.sockets.retain(|s| !(s.x == x && s.y == y && s.z == z));
            }
            _ => {}
        }
    }

    fn art_key(&mut self, k: KeyEvent) {
        let (w, h) = (self.art.w, self.art.h);
        let big = if k.shift { 5 } else { 1 };
        let mv = |s: &mut Tokamak, key: Key| match key {
            Key::Left => s.ax = (s.ax - big).max(0),
            Key::Right => s.ax = (s.ax + big).min(w - 1),
            Key::Up => s.ay = (s.ay - big).max(0),
            Key::Down => s.ay = (s.ay + big).min(h - 1),
            Key::Home => s.ax = 0,
            Key::End => s.ax = w - 1,
            _ => {}
        };
        if self.typing {
            match k.key {
                Key::Enter => self.typing = false,
                Key::Backspace => {
                    self.checkpoint();
                    self.ax = (self.ax - 1).max(0);
                    self.art_put(' ');
                }
                Key::Char(c) => {
                    self.checkpoint();
                    self.art_put(c);
                    self.ax = (self.ax + 1).min(w - 1);
                }
                key => mv(self, key),
            }
            return;
        }
        let n = PALETTE.len();
        match k.key {
            Key::Char(' ') => {
                self.checkpoint();
                self.art_put(self.aglyph);
            }
            Key::Delete => {
                self.checkpoint();
                self.art_put(' ');
            }
            Key::Enter => {
                self.typing = true;
                self.note = "Typing: Enter or Esc stops".into();
            }
            Key::Char('g') => self.ask = Some((Ask::ArtGlyph, String::new())),
            Key::Char('c') => self.ink = (self.ink + 1) % n,
            Key::Char('C') => self.ink = (self.ink + n - 1) % n,
            Key::Char('v') => self.paper = (self.paper + 1) % n,
            Key::Char('V') => self.paper = (self.paper + n - 1) % n,
            Key::Char('y') => {
                self.mirror = !self.mirror;
                self.note = format!("Mirror {}", if self.mirror { "on" } else { "off" });
            }
            Key::Char('t') => {
                let c = self.art.cells[(self.ay * self.art.w + self.ax) as usize];
                (self.aglyph, self.ink, self.paper) = (if c.ch == ' ' { self.aglyph } else { c.ch }, nearest(c.fg), nearest(c.bg));
            }
            Key::Char(c @ ('r' | 'b')) => match self.anchor.take() {
                Some(a) => self.art_rect(a),
                None => self.anchor = Some((self.ax, self.ay, c == 'b')),
            },
            Key::Char('F') => self.art_flood(),
            key => mv(self, key),
        }
    }

    fn library_key(&mut self, k: KeyEvent) {
        let order = self.order();
        let n = order.len().max(1);
        let at = order.iter().position(|&i| i == self.pick).unwrap_or(0);
        let step = |v: u8, up: bool| if up { v.saturating_add(8) } else { v.saturating_sub(8) };
        let cur = self.lib.get(self.pick).cloned();
        match k.key {
            Key::Up => self.pick = *order.get((at + n - 1) % n).unwrap_or(&0),
            Key::Down => self.pick = *order.get((at + 1) % n).unwrap_or(&0),
            Key::PageUp => self.pick = *order.get(at.saturating_sub(10)).unwrap_or(&0),
            Key::PageDown => self.pick = *order.get((at + 10).min(n - 1)).unwrap_or(&0),
            Key::Char('m') => self.ask = Some((Ask::NewEntry(false), String::new())),
            Key::Char('k') => self.ask = Some((Ask::NewEntry(true), String::new())),
            Key::Char('g') => self.ask = Some((Ask::Glyph(false), String::new())),
            Key::Char('G') => self.ask = Some((Ask::Glyph(true), String::new())),
            Key::Char('r') => self.ask = cur.map(|e| (Ask::Rename, e.name)),
            Key::Char('e') => self.ask = cur.map(|e| (Ask::Tags, e.tags.join(" "))),
            Key::Char('c') => self.ask = cur.map(|e| (Ask::Colours, format!("{} {} {} / {} {} {}", e.fg.0, e.fg.1, e.fg.2, e.bg.0, e.bg.1, e.bg.2))),
            Key::Char('d') => {
                if let Some(mut e) = cur {
                    self.checkpoint();
                    e.name = format!("{} 2", e.name);
                    self.lib.insert(self.pick + 1, e);
                    self.pick += 1;
                    self.note = "Duplicated: r to rename it".into();
                }
            }
            Key::Delete => {
                if self.pick < self.lib.len() {
                    self.checkpoint();
                    let gone = self.lib.remove(self.pick);
                    let order = self.order();
                    self.pick = *order.get(at.min(order.len().saturating_sub(1))).unwrap_or(&0);
                    self.note = format!("Removed {} (Ctrl+Z brings it back)", gone.name);
                }
            }
            Key::Char(c @ '1'..='6') => {
                if self.pick < self.lib.len() {
                    self.checkpoint();
                    let e = &mut self.lib[self.pick];
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

    fn panel_text(&self, f: &mut Frame, y: &mut i32, s: &str, c: Rgb) {
        f.text_clip(f.width() - PANEL_W + 1, *y, PANEL_W - 2, s, c, BG);
        *y += 1;
    }

    fn draw_part(&mut self, f: &mut Frame, r: Rect) {
        // The part, centred where it fits and scrolled to the cursor where it does not,
        // with the level below showing faintly through open cells.
        let (pw, ph) = (self.part.bp.w, self.part.bp.h);
        let (vw, vh) = (pw.min(r.w - 2), ph.min(r.h - 2));
        self.poff = (follow(self.poff.0, self.cx, pw, vw), follow(self.poff.1, self.cy, ph, vh));
        let (sx, sy) = self.poff;
        let ox = r.x + 1 + ((r.w - 2 - vw) / 2).max(0);
        let oy = r.y + 1 + ((r.h - 2 - vh) / 2).max(0);
        self.view = (ox, oy, vw, vh);
        let p = &self.part;
        let scrolled = if vw < pw || vh < ph { format!(" · from {},{}", sx, sy) } else { String::new() };
        f.border(Rect { x: ox - 1, y: oy - 1, w: vw + 2, h: vh + 2 }, DIM, BG, &format!(" {} · level {} of {}{} ", p.bp.name, self.z + 1, p.bp.d, scrolled));
        for y in 0..vh {
            for x in 0..vw {
                let (px, py) = (x + sx, y + sy);
                let here = p.bp.cells[p.idx(px, py, self.z)].as_ref();
                let cell = match here {
                    Some(c) if c.tile != Tile::Open || !c.things.is_empty() => self.look(c),
                    _ => match (self.z > 0).then(|| p.bp.cells[p.idx(px, py, self.z - 1)].as_ref()).flatten() {
                        Some(below) => {
                            let c = self.look(below);
                            Cell::new(c.ch, c.fg.lerp(BG, 0.7), c.bg.lerp(BG, 0.7))
                        }
                        None => Cell::new(if here.is_some() { ' ' } else { '·' }, FAINT, BG),
                    },
                };
                f.set(ox + x, oy + y, cell);
            }
        }
        if self.mirror && pw <= vw {
            // The mirror line, faintly, top and bottom.
            for mx in [(pw - 1) / 2, pw / 2] {
                f.set(ox + mx, oy - 1, Cell::new('v', GOLD, BG));
                f.set(ox + mx, oy + vh, Cell::new('^', GOLD, BG));
            }
        }
        for s in p.sockets.iter().filter(|s| s.z == self.z && s.x >= sx && s.y >= sy && s.x < sx + vw && s.y < sy + vh) {
            let ch = ['^', '>', 'v', '<', '+', '-'][s.dir];
            f.set(ox + s.x - sx, oy + s.y - sy, Cell::new(ch, Rgb(20, 16, 10), GOLD));
        }
        if let Some((ax, ay, _)) = self.anchor {
            // The rectangle being drawn.
            for y in ay.min(self.cy)..=ay.max(self.cy) {
                for x in ax.min(self.cx)..=ax.max(self.cx) {
                    if x >= sx && y >= sy && x < sx + vw && y < sy + vh {
                        let c = f.get(ox + x - sx, oy + y - sy);
                        f.set(ox + x - sx, oy + y - sy, Cell::new(c.ch, c.fg, Rgb(70, 56, 24)));
                    }
                }
            }
        }
        let cur = f.get(ox + self.cx - sx, oy + self.cy - sy);
        f.set(ox + self.cx - sx, oy + self.cy - sy, Cell::new(cur.ch, BG, INK));

        // The panel: the brush (all clickable), the sockets, a few keys.
        let px = f.width() - PANEL_W;
        let mut y = r.y;
        let under = p.bp.cells[p.idx(self.cx, self.cy, self.z)].as_ref().map(|c| format!("{} {}", c.mat, c.things.join(" "))).unwrap_or_default();
        self.panel_text(f, &mut y, &format!("Cursor {},{}  level {}/{}  {}x{}", self.cx, self.cy, self.z + 1, p.bp.d, pw, ph), INK);
        self.panel_text(f, &mut y, &format!("Under it: {}", if under.trim().is_empty() { "nothing" } else { under.trim() }), DIM);
        y += 1;
        let tools = format!("Brush{}{}", if self.mirror { " · mirror" } else { "" }, if self.anchor.is_some() { " · corner set" } else { "" });
        self.panel_text(f, &mut y, &tools, GOLD);
        let mut x = px + 1;
        for (name, t) in TILES {
            let w = name.chars().count() as i32;
            let on = t == self.tile;
            f.text_clip(x, y, w, name, if on { BG } else { DIM }, if on { INK } else { BG });
            self.hits.push((x, y, w, Hit::Tile(t)));
            x += w + 1;
        }
        y += 1;
        let mats = self.mats();
        let cur_mat = self.mat % mats.len().max(1);
        for (k, &i) in mats.iter().enumerate() {
            if (k as i32 - cur_mat as i32).abs() <= 3 || (cur_mat < 3 && k < 7) || (cur_mat + 4 > mats.len() && k + 7 >= mats.len()) {
                let e = &self.lib[i];
                let on = k == cur_mat;
                f.set(px + 1, y, Cell::new(e.glyph, e.fg, e.bg));
                f.set(px + 2, y, Cell::new(e.floor, e.fg.lerp(e.bg, 0.35), e.bg));
                f.text_clip(px + 4, y, PANEL_W - 5, &format!("{}{}", if on { "> " } else { "  " }, e.name), if on { INK } else { DIM }, BG);
                self.hits.push((px + 1, y, PANEL_W - 2, Hit::Mat(k)));
                y += 1;
            }
        }
        y += 1;
        self.panel_text(f, &mut y, &format!("Thing: {}", if self.with_thing { "on (k: off)" } else { "off (k or click: on)" }), GOLD);
        let kinds = self.kinds();
        let cur_kind = self.thing % kinds.len().max(1);
        for (k, &i) in kinds.iter().enumerate() {
            if (k as i32 - cur_kind as i32).abs() <= 2 || (cur_kind < 2 && k < 5) || (cur_kind + 3 > kinds.len() && k + 5 >= kinds.len()) {
                let e = &self.lib[i];
                let on = k == cur_kind;
                f.set(px + 1, y, Cell::new(e.glyph, e.fg, BG));
                f.text_clip(px + 4, y, PANEL_W - 5, &format!("{}{}", if on { "> " } else { "  " }, e.name), if on && self.with_thing { INK } else { DIM }, BG);
                self.hits.push((px + 1, y, PANEL_W - 2, Hit::Thing(k)));
                y += 1;
            }
        }
        y += 1;
        self.panel_text(f, &mut y, &format!("Sockets ({})", p.sockets.len()), GOLD);
        for s in p.sockets.iter().take(5) {
            self.panel_text(f, &mut y, &format!("{},{} L{} {} {}", s.x, s.y, s.z + 1, DIRS[s.dir], s.tag), DIM);
        }
        y += 1;
        for help in ["Space/L paint  Del/R clear", "r rect  b room  F fill  y mirror", "c copy level below  + - levels", "^Z undo  ? every key"] {
            self.panel_text(f, &mut y, help, DIM);
        }
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

    fn draw_art(&mut self, f: &mut Frame, r: Rect) {
        let (w, h) = (self.art.w, self.art.h);
        let (vw, vh) = (w.min(r.w - 2), h.min(r.h - 2));
        self.aoff = (follow(self.aoff.0, self.ax, w, vw), follow(self.aoff.1, self.ay, h, vh));
        let (sx, sy) = self.aoff;
        let ox = r.x + 1 + ((r.w - 2 - vw) / 2).max(0);
        let oy = r.y + 1 + ((r.h - 2 - vh) / 2).max(0);
        self.view = (ox, oy, vw, vh);
        let a = &self.art;
        f.border(Rect { x: ox - 1, y: oy - 1, w: vw + 2, h: vh + 2 }, DIM, BG, &format!(" {} ", a.name));
        for y in 0..vh {
            for x in 0..vw {
                f.set(ox + x, oy + y, a.cells[((y + sy) * a.w + x + sx) as usize]);
            }
        }
        if let Some((x0, y0, _)) = self.anchor {
            for y in y0.min(self.ay)..=y0.max(self.ay) {
                for x in x0.min(self.ax)..=x0.max(self.ax) {
                    if x >= sx && y >= sy && x < sx + vw && y < sy + vh {
                        let c = f.get(ox + x - sx, oy + y - sy);
                        f.set(ox + x - sx, oy + y - sy, Cell::new(c.ch, c.fg, Rgb(70, 56, 24)));
                    }
                }
            }
        }
        let cur = f.get(ox + self.ax - sx, oy + self.ay - sy);
        f.set(ox + self.ax - sx, oy + self.ay - sy, Cell::new(if cur.ch == ' ' { '_' } else { cur.ch }, BG, if self.typing { GOLD } else { INK }));
        let px = f.width() - PANEL_W;
        let mut y = r.y;
        self.panel_text(f, &mut y, &format!("Cursor {},{}  {} x {}", self.ax, self.ay, w, h), INK);
        y += 1;
        self.panel_text(f, &mut y, &format!("Brush{}", if self.mirror { " · mirror" } else { "" }), GOLD);
        f.set(px + 8, y - 1, Cell::new(self.aglyph, PALETTE[self.ink], PALETTE[self.paper]));
        y += 1;
        self.panel_text(f, &mut y, "Ink (left click), paper (right)", GOLD);
        for (i, c) in PALETTE.iter().enumerate() {
            let mark = if i == self.ink { 'i' } else if i == self.paper { 'p' } else { ' ' };
            let x = px + 1 + i as i32 * 2;
            f.set(x, y, Cell::new(mark, if c.0 as u32 + c.1 as u32 + c.2 as u32 > 300 { BG } else { INK }, *c));
            f.set(x + 1, y, Cell::new(' ', INK, *c));
            self.hits.push((x, y, 2, Hit::Colour(i)));
        }
        y += 2;
        for help in ["Space/L paint  Del/R clear", "g glyph  c C ink  v V paper", "t pick from cursor", "Enter: type text (Enter stops)", "r rect  b box  F fill  y mirror", "^Z undo  ? every key"] {
            self.panel_text(f, &mut y, help, DIM);
        }
        if self.typing {
            self.panel_text(f, &mut (y + 1), "TYPING", GOLD);
        }
    }

    fn draw_library(&mut self, f: &mut Frame, r: Rect) {
        // The rows: a title, the materials, a gap, a title, the kinds.
        let mut rows: Vec<Option<usize>> = vec![None];
        rows.extend(self.mats().into_iter().map(Some));
        rows.extend([None, None]);
        rows.extend(self.kinds().into_iter().map(Some));
        let win = r.h - 2;
        let at = rows.iter().position(|e| *e == Some(self.pick)).unwrap_or(0) as i32;
        self.loff = follow(self.loff, at, rows.len() as i32, win);
        let kinds_title = self.mats().len() + 2;
        for (row, e) in rows.iter().enumerate().skip(self.loff as usize).take(win.max(0) as usize) {
            let y = r.y + 1 + row as i32 - self.loff;
            let Some(i) = *e else {
                if row == 0 || row == kinds_title {
                    f.text_clip(r.x + 2, y, r.w - 4, if row == 0 { "Materials (wall, floor)" } else { "Kinds (things)" }, GOLD, BG);
                }
                continue;
            };
            let e = &self.lib[i];
            let on = i == self.pick;
            f.set(r.x + 2, y, Cell::new(e.glyph, e.fg, e.bg));
            if !e.kind {
                f.set(r.x + 3, y, Cell::new(e.floor, e.fg.lerp(e.bg, 0.35), e.bg));
            }
            let info = format!("{}{:<16} {}", if on { "> " } else { "  " }, e.name, e.tags.join(" "));
            f.text_clip(r.x + 5, y, r.w - 7, &info, if on { INK } else { DIM }, BG);
            self.hits.push((r.x + 2, y, r.w - 4, Hit::Lib(i)));
        }

        // The panel: the chosen entry up close.
        let px = f.width() - PANEL_W;
        let mut y = r.y;
        if let Some(e) = self.lib.get(self.pick).cloned() {
            self.panel_text(f, &mut y, &format!("{} ({})", e.name, if e.kind { "kind" } else { "material" }), GOLD);
            // A swatch: a wall round a floor, or the thing on the floor.
            for sy in 0..3 {
                for sx in 0..8 {
                    let c = if e.kind {
                        if (sx, sy) == (3, 1) { Cell::new(e.glyph, e.fg, BG) } else { Cell::new('.', FAINT, BG) }
                    } else if sy == 0 || sx == 0 {
                        Cell::new(e.glyph, e.fg, e.bg)
                    } else {
                        Cell::new(e.floor, e.fg.lerp(e.bg, 0.35), e.bg)
                    };
                    f.set(px + 1 + sx, y + sy, c);
                }
            }
            y += 4;
            self.panel_text(f, &mut y, &format!("glyph {}{}", e.glyph, if e.kind { String::new() } else { format!("  floor {}", e.floor) }), DIM);
            self.panel_text(f, &mut y, &format!("ink {} {} {}", e.fg.0, e.fg.1, e.fg.2), DIM);
            self.panel_text(f, &mut y, &format!("bg  {} {} {}", e.bg.0, e.bg.1, e.bg.2), DIM);
            self.panel_text(f, &mut y, &format!("tags {}", e.tags.join(" ")), DIM);
            y += 1;
        }
        for help in ["m new material  k new kind", "d duplicate  r rename  e tags", "g glyph  G floor glyph", "c colours as numbers", "1-6 nudge rgb (Ctrl: bg)", "Del remove  ^Z undo  ^S save", "? every key"] {
            self.panel_text(f, &mut y, help, DIM);
        }
    }

    fn draw_picker(&mut self, f: &mut Frame, r: Rect) {
        let Some(p) = &self.picker else { return };
        let shown = p.shown();
        let what = match p.what { Listing::Parts => "Open a part", Listing::Arts => "Open art", Listing::Projects => "Switch project" };
        let w = 40.min(r.w - 4);
        let h = (shown.len() as i32 + 4).clamp(6, r.h - 2);
        let b = Rect { x: r.x + (r.w - w) / 2, y: r.y + 1, w, h };
        f.fill(b, Cell::new(' ', INK, BG));
        f.border(b, GOLD, BG, &format!(" {what} "));
        f.text_clip(b.x + 2, b.y + 1, w - 4, &format!("> {}_", p.filter), INK, BG);
        let room = (h - 3).max(1) as usize;
        let first = p.sel.saturating_sub(room - 1);
        if shown.is_empty() {
            let msg = if p.what == Listing::Projects && !p.filter.is_empty() { "Enter: a new project by this name" } else { "Nothing here yet" };
            f.text_clip(b.x + 2, b.y + 2, w - 4, msg, DIM, BG);
        }
        let mut hits = Vec::new();
        for (i, name) in shown.iter().enumerate().skip(first).take(room) {
            let y = b.y + 2 + (i - first) as i32;
            let on = i == p.sel;
            let mark = if p.what == Listing::Projects && *name == self.project { "  (open now)" } else { "" };
            f.text_clip(b.x + 2, y, w - 4, &format!("{}{name}{mark}", if on { "> " } else { "  " }), if on { BG } else { INK }, if on { GOLD } else { BG });
            hits.push((b.x + 1, y, w - 2, Hit::Item(i)));
        }
        // The picker sits above the panels: only its own rows are clickable.
        self.hits.retain(|h| matches!(h.3, Hit::Mode(_)));
        self.hits.extend(hits);
    }

    fn draw_help(&mut self, f: &mut Frame) {
        let lines = match self.mode { Mode::Part => HELP_PART, Mode::Library => HELP_LIBRARY, Mode::Art => HELP_ART };
        let w = (lines.iter().map(|l| l.chars().count()).max().unwrap_or(40) as i32 + 4).min(f.width() - 2);
        let h = lines.len() as i32 + 5;
        let b = Rect { x: (f.width() - w) / 2, y: ((f.height() - h) / 2).max(1), w, h };
        f.fill(b, Cell::new(' ', INK, BG));
        f.border(b, GOLD, BG, &format!(" {} keys ", self.mode.name()));
        for (i, l) in lines.iter().enumerate() {
            f.text_clip(b.x + 2, b.y + 1 + i as i32, w - 4, l, INK, BG);
        }
        f.text_clip(b.x + 2, b.y + h - 3, w - 4, "Tab, Shift+Tab or click the tabs: Part, Library, Art.  Esc: quit (asks if unsaved).", DIM, BG);
        f.text_clip(b.x + 2, b.y + h - 2, w - 4, "Any key closes this.", GOLD, BG);
        self.hits.clear();
    }
}

fn main() -> io::Result<()> {
    let project = std::env::args()
        .nth(1)
        .or_else(|| fs::read_to_string("tokamak/.last").ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()))
        .unwrap_or_else(|| "default".into());
    let mut app = Tokamak::blank(&project, Vec::new());
    app.load_project(&project)?;
    tachyon::app::run(&mut app, 30)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shop() -> Tokamak {
        let mut t = Tokamak::blank("test", parse_library(SEED_LIBRARY));
        t.part = Part::new("p", 8, 6, 2);
        t
    }

    #[test]
    fn art_round_trips() {
        let mut a = Art::new("sign", 6, 2);
        a.cells[0] = Cell::new('T', PALETTE[4], PALETTE[0]);
        a.cells[7] = Cell::new('~', PALETTE[10], PALETTE[11]);
        let back = Art::from_text(&a.text()).unwrap();
        assert_eq!((back.w, back.h, back.name.as_str()), (6, 2, "sign"));
        assert_eq!(back.cells[0].ch, 'T');
        assert_eq!(back.cells[0].fg, PALETTE[4]);
        assert_eq!(back.cells[7].bg, PALETTE[11]);
    }

    #[test]
    fn library_round_trips() {
        let lib = parse_library(SEED_LIBRARY);
        assert!(lib.iter().any(|e| e.name == "stone block" && !e.kind && e.glyph == '#' && e.floor == '.'));
        assert!(lib.iter().any(|e| e.name == "cannon" && e.kind && e.glyph == 'o'));
        let mast = lib.iter().find(|e| e.name == "mast").unwrap();
        assert_eq!((mast.glyph, mast.fg, mast.tags.join(" ").as_str()), ('|', Rgb(200, 170, 110), "ship"));
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

    #[test]
    fn a_room_is_walls_round_a_floor_and_undo_takes_it_away() {
        let mut t = shop();
        (t.cx, t.cy) = (4, 3);
        t.part_rect((0, 0, true));
        let at = |t: &Tokamak, x, y| t.part.bp.cells[t.part.idx(x, y, 0)].as_ref().map(|c| c.tile);
        assert_eq!(at(&t, 0, 0), Some(Tile::Wall));
        assert_eq!(at(&t, 4, 2), Some(Tile::Wall));
        assert_eq!(at(&t, 2, 1), Some(Tile::Floor));
        assert_eq!(at(&t, 5, 1), None);
        assert!(t.dirty[0]);
        t.undo(true);
        assert_eq!(at(&t, 0, 0), None);
        t.undo(false);
        assert_eq!(at(&t, 0, 0), Some(Tile::Wall));
    }

    #[test]
    fn flood_fills_inside_a_room_only() {
        let mut t = shop();
        (t.cx, t.cy) = (4, 4);
        t.part_rect((0, 0, true));
        (t.cx, t.cy, t.tile) = (2, 2, Tile::Wall);
        t.mat = 2;
        t.part_flood();
        let mats: Vec<String> = (0..5).map(|x| t.part.bp.cells[t.part.idx(x, 2, 0)].as_ref().unwrap().mat.clone()).collect();
        assert_eq!(mats[0], "stone block");
        assert_eq!(mats[2], "oak");
        assert_eq!(mats[4], "stone block");
        // The outside was never touched.
        assert!(t.part.bp.cells[t.part.idx(6, 2, 0)].is_none());
    }

    #[test]
    fn mirror_paints_both_sides() {
        let mut t = shop();
        t.mirror = true;
        (t.cx, t.cy) = (1, 1);
        let b = t.brush();
        t.paint(b);
        assert!(t.part.bp.cells[t.part.idx(1, 1, 0)].is_some());
        assert!(t.part.bp.cells[t.part.idx(6, 1, 0)].is_some());
    }

    #[test]
    fn copying_a_level_up_turns_stairs_round() {
        let mut p = Part::new("house", 3, 1, 2);
        p.bp.cells[0] = Some(BpCell { tile: Tile::Wall, mat: "oak".into(), things: vec![] });
        p.bp.cells[1] = Some(BpCell { tile: Tile::StairUp, mat: "oak".into(), things: vec!["chest".into()] });
        p.copy_level_up(1);
        assert_eq!(p.bp.cells[p.idx(0, 0, 1)].as_ref().unwrap().tile, Tile::Wall);
        let stair = p.bp.cells[p.idx(1, 0, 1)].as_ref().unwrap();
        assert_eq!((stair.tile, stair.things.len()), (Tile::StairDown, 0));
        assert!(p.bp.cells[p.idx(2, 0, 1)].is_none());
    }

    #[test]
    fn unsaved_work_asks_before_it_goes() {
        let mut t = shop();
        assert!(matches!(t.guard(Act::Quit), Flow::Quit));
        t.checkpoint();
        assert!(matches!(t.guard(Act::Quit), Flow::Continue));
        assert_eq!(t.confirm, Some(Act::Quit));
        // Art has nothing unsaved, so opening art does not ask.
        t.confirm = None;
        t.guard(Act::NewArt("a".into(), 4, 2));
        assert!(t.confirm.is_none());
    }

    #[test]
    fn every_screen_draws_at_any_size() {
        let mut t = shop();
        t.part = Part::new("big", 120, 60, 3);
        (t.cx, t.cy, t.z) = (100, 50, 1);
        t.anchor = Some((90, 45, true));
        for (w, h) in [(120, 36), (50, 14)] {
            for mode in MODES {
                for overlay in 0..3 {
                    t.mode = mode;
                    t.help = overlay == 1;
                    t.picker = (overlay == 2).then(|| Picker { what: Listing::Projects, items: vec!["broadside".into(), "gothic".into()], sel: 1, filter: String::new() });
                    let mut f = Frame::new(w, h);
                    t.draw(&mut f, &LoopStats::default());
                    if std::env::var("TOKAMAK_SHOW").is_ok() && w == 120 {
                        println!("{}\n", f.to_text());
                    }
                }
            }
        }
    }

    #[test]
    fn art_boxes_and_mirrors() {
        let mut t = shop();
        t.mode = Mode::Art;
        t.art = Art::new("b", 6, 3);
        (t.ax, t.ay) = (2, 2);
        t.art_rect((0, 0, true));
        let row: String = (0..6).map(|x| t.art.cells[x].ch).collect();
        assert_eq!(row, "+-+   ");
        t.mirror = true;
        (t.ax, t.ay) = (0, 1);
        t.art_put('(');
        assert_eq!(t.art.cells[6 + 5].ch, ')');
    }
}
