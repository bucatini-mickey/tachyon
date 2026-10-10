//! Atlas mode: painting the world map.

use crate::palette::{self, BIOMES, FEATURES, ROCKS};
use crate::{Ask, Editor, Mode, Panel, DIM, TITLE, TXT};
use tachyon::atlas::{Atlas, Feature, River, GROUND, REGION};
use tachyon::frame::Cell;
use tachyon::noise::fbm2;
use tachyon::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    Terrain,
    Height,
    Rock,
    Biome,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Raise,
    Height,
    Smooth,
    Rock,
    Biome,
    River,
    Feature,
}

const TOOLS: [(Tool, &str); 7] = [
    (Tool::Raise, "raise / lower"),
    (Tool::Height, "set height"),
    (Tool::Smooth, "smooth"),
    (Tool::Rock, "rock"),
    (Tool::Biome, "biome"),
    (Tool::River, "river"),
    (Tool::Feature, "feature"),
];

pub struct AtlasState {
    pub cam: (i32, i32),
    pub cursor: (i32, i32),
    pub view: View,
    pub tool: Tool,
    pub radius: f32,
    pub step: i16,
    pub target: i16,
    pub rock: usize,
    pub biome: usize,
    pub feature: usize,
    pub river_width: u8,
    pub draft: Vec<(i32, i32)>,
    pub help: bool,
    last_cell: Option<(i32, i32)>,
}

impl AtlasState {
    pub fn new(a: &Atlas) -> AtlasState {
        AtlasState {
            cam: (0, 0),
            cursor: (a.w / 2, a.h / 2),
            view: View::Terrain,
            tool: Tool::Height,
            radius: 2.0,
            step: 2,
            target: 6,
            rock: 0,
            biome: 2,
            feature: 0,
            river_width: 3,
            draft: Vec::new(),
            help: true,
            last_cell: None,
        }
    }
}

/// A rough island from noise, as a starting point to paint over.
pub fn rough_island(w: i32, h: i32, seed: u32) -> Atlas {
    let biome_names = BIOMES.map(|b| b.name);
    let mut a = Atlas::new("rough island", w, h, &ROCKS, &biome_names, 12);
    let bi = |n: &str| biome_names.iter().position(|b| *b == n).unwrap() as u8;
    for y in 0..h {
        for x in 0..w {
            let (dx, dy) = ((x as f32 / w as f32 - 0.5) * 2.0, (y as f32 / h as f32 - 0.5) * 2.0);
            let r2 = dx * dx + dy * dy;
            let n = fbm2(seed, x as f32 / 9.0, y as f32 / 9.0, 4);
            let e = ((n - 0.12 - r2 * 0.62) * 95.0).round().clamp(-20.0, 60.0) as i16;
            let i = a.idx(x, y);
            a.elevation[i] = e;
            let wet = fbm2(seed ^ 0x55, x as f32 / 7.0, y as f32 / 7.0, 3);
            a.biome[i] = match e {
                e if e < 0 => bi("ocean"),
                0..=1 => bi("beach"),
                2..=14 if wet > 0.58 => bi("marsh"),
                2..=14 if wet < 0.45 => bi("forest"),
                2..=14 => bi("grassland"),
                15..=26 => if wet > 0.5 { bi("moor") } else { bi("forest") },
                27..=40 => bi("rock"),
                _ => bi("snow"),
            };
            let band = fbm2(seed ^ 0x77, x as f32 / 12.0, y as f32 / 12.0, 2);
            a.rock[i] = if e > 30 { 0 } else if band < 0.42 { 2 } else if band > 0.6 { 3 } else { 0 };
        }
    }
    // One river, from the highest point downhill to the sea.
    if let Some(top) = (0..a.elevation.len()).max_by_key(|&i| a.elevation[i]) {
        let (mut x, mut y) = (top as i32 % w, top as i32 / w);
        // Start a little below the summit, on its flank.
        let mut pts = vec![(x, y)];
        for _ in 0..200 {
            let here = a.elev(x, y);
            if here < 0.0 {
                break;
            }
            let next = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (-1, 1), (1, -1), (-1, -1)]
                .iter()
                .map(|&(dx, dy)| (x + dx, y + dy))
                .filter(|&(nx, ny)| a.contains(nx, ny) && !pts.contains(&(nx, ny)))
                .min_by(|p, q| a.elev(p.0, p.1).partial_cmp(&a.elev(q.0, q.1)).unwrap());
            let Some((nx, ny)) = next else { break };
            x = nx;
            y = ny;
            pts.push((x, y));
        }
        if pts.len() > 3 {
            a.rivers.push(River { width: 3, points: pts.into_iter().skip(1).collect() });
        }
    }
    a
}

fn height_color(e: i16) -> Rgb {
    let e = e as f32;
    if e < 0.0 {
        Rgb(40, 110, 190).lerp(Rgb(8, 24, 80), (-e / 25.0).min(1.0))
    } else if e < 20.0 {
        Rgb(70, 150, 70).lerp(Rgb(200, 190, 100), e / 20.0)
    } else if e < 45.0 {
        Rgb(200, 190, 100).lerp(Rgb(130, 92, 64), (e - 20.0) / 25.0)
    } else {
        Rgb(130, 92, 64).lerp(Rgb(240, 240, 245), ((e - 45.0) / 30.0).min(1.0))
    }
}

impl Editor {
    fn tool_name(&self) -> &'static str {
        TOOLS.iter().find(|(t, _)| *t == self.a.tool).map(|(_, n)| *n).unwrap_or("")
    }

    /// Feature kinds on offer: the built-in ones, then blueprints on disk.
    fn feature_kinds() -> Vec<String> {
        FEATURES.iter().map(|s| s.to_string()).chain(Editor::blueprint_names().into_iter().map(|n| format!("bp:{n}"))).collect()
    }

    pub fn atlas_move(&mut self, dx: i32, dy: i32) {
        let (x, y) = self.a.cursor;
        self.a.cursor = ((x + dx).clamp(0, self.atlas.w - 1), (y + dy).clamp(0, self.atlas.h - 1));
    }

    fn map_cell(&self, map: Rect, sx: i32, sy: i32) -> Option<(i32, i32)> {
        if !map.contains(sx, sy) {
            return None;
        }
        let (x, y) = (self.a.cam.0 + (sx - map.x) / 2, self.a.cam.1 + (sy - map.y));
        self.atlas.contains(x, y).then_some((x, y))
    }

    /// Apply the current tool at a cell. `primary` is left click / Space.
    fn apply(&mut self, cx: i32, cy: i32, primary: bool) {
        let ocean = BIOMES.iter().position(|b| b.name == "ocean").unwrap_or(0) as u8;
        let (beach, grass) = (BIOMES.iter().position(|b| b.name == "beach").unwrap_or(1) as u8, self.a.biome.max(2) as u8);
        let fix_biome = |a: &mut Atlas, i: usize| {
            let e = a.elevation[i];
            if e >= 0 && a.biome[i] == ocean {
                a.biome[i] = if e <= 1 { beach } else { grass };
            } else if e < 0 {
                a.biome[i] = ocean;
            }
        };
        match self.a.tool {
            Tool::Raise => {
                let s = if primary { self.a.step } else { -self.a.step } as f32;
                for (i, w) in self.atlas.brush(cx, cy, self.a.radius) {
                    self.atlas.elevation[i] = (self.atlas.elevation[i] as f32 + s * w).round().clamp(-200.0, 400.0) as i16;
                    fix_biome(&mut self.atlas, i);
                }
            }
            Tool::Height => {
                if !primary {
                    self.a.target = self.atlas.elevation[self.atlas.idx(cx, cy)];
                    return;
                }
                let t = self.a.target as f32;
                for (i, w) in self.atlas.brush(cx, cy, self.a.radius) {
                    let e = self.atlas.elevation[i] as f32;
                    self.atlas.elevation[i] = (e + (t - e) * w.min(1.0) * 1.2).clamp(e.min(t), e.max(t)).round() as i16;
                    fix_biome(&mut self.atlas, i);
                }
            }
            Tool::Smooth => {
                let old = self.atlas.elevation.clone();
                for (i, w) in self.atlas.brush(cx, cy, self.a.radius) {
                    let (x, y) = (i as i32 % self.atlas.w, i as i32 / self.atlas.w);
                    let mut sum = 0.0;
                    for dy in -1..=1 {
                        for dx in -1..=1 {
                            let (nx, ny) = ((x + dx).clamp(0, self.atlas.w - 1), (y + dy).clamp(0, self.atlas.h - 1));
                            sum += old[self.atlas.idx(nx, ny)] as f32;
                        }
                    }
                    let e = old[i] as f32;
                    self.atlas.elevation[i] = (e + (sum / 9.0 - e) * w * 0.7).round() as i16;
                    fix_biome(&mut self.atlas, i);
                }
            }
            Tool::Rock | Tool::Biome => {
                let i = self.atlas.idx(cx, cy);
                if !primary {
                    if self.a.tool == Tool::Rock {
                        self.a.rock = self.atlas.rock[i] as usize;
                    } else {
                        self.a.biome = self.atlas.biome[i] as usize;
                    }
                    return;
                }
                for (j, w) in self.atlas.brush(cx, cy, self.a.radius) {
                    if w > 0.25 {
                        if self.a.tool == Tool::Rock {
                            self.atlas.rock[j] = self.a.rock as u8;
                        } else {
                            self.atlas.biome[j] = self.a.biome as u8;
                        }
                    }
                }
            }
            Tool::River => {
                if primary {
                    if self.a.draft.last() != Some(&(cx, cy)) {
                        self.a.draft.push((cx, cy));
                    }
                } else if self.a.draft.pop().is_none() {
                    let before = self.atlas.rivers.len();
                    self.atlas.rivers.retain(|r| !Atlas::river_cells(r).contains(&(cx, cy)));
                    if self.atlas.rivers.len() < before {
                        self.say("River removed.");
                    }
                }
            }
            Tool::Feature => {
                if primary {
                    let kinds = Editor::feature_kinds();
                    let kind = kinds[self.a.feature % kinds.len()].clone();
                    self.atlas.features.push(Feature { x: cx * REGION + REGION / 2, y: cy * REGION + REGION / 2, z: GROUND, kind: kind.clone(), label: String::new() });
                    let i = self.atlas.features.len() - 1;
                    self.ask(&format!("Name this {kind} (Enter to leave unnamed):"), "", Ask::FeatureLabel(i));
                } else {
                    let before = self.atlas.features.len();
                    self.atlas.features.retain(|f| f.cell() != (cx, cy));
                    let n = before - self.atlas.features.len();
                    if n > 0 {
                        self.say(format!("Removed {n} feature(s)."));
                    }
                }
            }
        }
    }

    fn finish_river(&mut self) {
        if self.a.draft.len() < 2 {
            self.say("A river needs at least two points: its source and where it goes.");
            return;
        }
        self.checkpoint();
        let points = std::mem::take(&mut self.a.draft);
        self.atlas.rivers.push(River { width: self.a.river_width, points });
        self.say("River added. Its first point is the source.");
    }

    pub fn atlas_key(&mut self, k: KeyEvent) -> Flow {
        let now = std::time::Instant::now();
        let dir = match k.key {
            Key::Up => Some((0, -1)),
            Key::Down => Some((0, 1)),
            Key::Left => Some((-1, 0)),
            Key::Right => Some((1, 0)),
            Key::Home => Some((-1, -1)),
            Key::End => Some((-1, 1)),
            _ => None,
        };
        if let Some(d) = dir {
            self.pad.press(k.key, d, now);
            return Flow::Continue;
        }
        let ctrl = |c: char| k.ctrl && k.key == Key::Char(c);
        if ctrl('s') {
            self.save();
            return Flow::Continue;
        }
        if ctrl('z') {
            self.atlas_undo();
            return Flow::Continue;
        }
        match k.key {
            Key::Char(c @ '1'..='7') => {
                self.a.tool = TOOLS[(c as u8 - b'1') as usize].0;
                let n = self.tool_name();
                self.say(format!("Tool: {n}."));
            }
            Key::Tab => {
                self.a.view = match self.a.view {
                    View::Terrain => View::Height,
                    View::Height => View::Rock,
                    View::Rock => View::Biome,
                    View::Biome => View::Terrain,
                };
            }
            Key::Char('[') => self.a.radius = (self.a.radius - 0.5).max(0.0),
            Key::Char(']') => self.a.radius = (self.a.radius + 0.5).min(16.0),
            Key::Char('+') | Key::Char('=') => self.adjust(1),
            Key::Char('-') => self.adjust(-1),
            Key::PageUp => self.adjust(10),
            Key::PageDown => self.adjust(-10),
            Key::Char(',') => self.cycle(-1),
            Key::Char('.') => self.cycle(1),
            Key::Char(' ') => {
                self.checkpoint();
                let (x, y) = self.a.cursor;
                self.apply(x, y, true);
            }
            Key::Backspace | Key::Delete => {
                self.checkpoint();
                let (x, y) = self.a.cursor;
                self.apply(x, y, false);
            }
            Key::Enter => {
                if self.a.tool == Tool::River && !self.a.draft.is_empty() {
                    self.finish_river();
                } else {
                    let (x, y) = self.a.cursor;
                    self.enter_local(x, y);
                }
            }
            Key::Esc => {
                if !self.a.draft.is_empty() {
                    self.a.draft.clear();
                    self.say("River drawing cancelled.");
                }
            }
            Key::Char('u') => self.atlas_undo(),
            Key::Char('S') => self.save(),
            Key::Char('N') => self.ask("Start again with a blank sea? (y / n)", "", Ask::NewAtlas),
            Key::Char('I') => {
                self.checkpoint();
                let seed = (self.atlas.elevation.len() as u32).wrapping_mul(2654435761) ^ self.undo.len() as u32;
                self.atlas = rough_island(self.atlas.w, self.atlas.h, seed);
                self.say("A new rough island. Undo (u) brings the old map back.");
            }
            Key::Char('?') => self.a.help = !self.a.help,
            Key::Char('q') => {
                if self.dirty {
                    self.ask("Unsaved changes. Quit anyway? (y / n)", "", Ask::Quit);
                } else {
                    return Flow::Quit;
                }
            }
            _ => {}
        }
        Flow::Continue
    }

    fn atlas_undo(&mut self) {
        match self.undo.pop() {
            Some(a) => {
                self.atlas = a;
                self.say("Undone.");
            }
            None => self.say("Nothing to undo."),
        }
    }

    fn adjust(&mut self, d: i16) {
        match self.a.tool {
            Tool::Raise => self.a.step = (self.a.step + d.signum()).clamp(1, 20),
            Tool::River => self.a.river_width = (self.a.river_width as i16 + d.signum()).clamp(1, 12) as u8,
            _ => self.a.target = (self.a.target + d).clamp(-200, 400),
        }
    }

    fn cycle(&mut self, d: i32) {
        let wrap = |v: usize, n: usize| ((v as i32 + d).rem_euclid(n as i32)) as usize;
        match self.a.tool {
            Tool::Rock => self.a.rock = wrap(self.a.rock, self.atlas.rocks.len()),
            Tool::Feature => self.a.feature = wrap(self.a.feature, Editor::feature_kinds().len()),
            _ => self.a.biome = wrap(self.a.biome, self.atlas.biomes.len()),
        }
    }

    pub fn atlas_mouse(&mut self, m: MouseEvent) {
        let map = self.map;
        match m.kind {
            MouseKind::ScrollUp => self.a.radius = (self.a.radius + 0.5).min(16.0),
            MouseKind::ScrollDown => self.a.radius = (self.a.radius - 0.5).max(0.0),
            MouseKind::Down(b) | MouseKind::Drag(b) => {
                let Some(c) = self.map_cell(map, m.x, m.y) else { return };
                self.a.cursor = c;
                let fresh = matches!(m.kind, MouseKind::Down(_));
                if b == MouseButton::Middle {
                    return;
                }
                // Rivers and features are placed per click, not smeared.
                let once = matches!(self.a.tool, Tool::River | Tool::Feature);
                if fresh {
                    self.checkpoint();
                    self.painting = Some(b);
                } else if once || self.a.last_cell == Some(c) {
                    return;
                }
                self.a.last_cell = Some(c);
                self.apply(c.0, c.1, b == MouseButton::Left);
            }
            MouseKind::Up(_) => {
                self.painting = None;
                self.a.last_cell = None;
            }
            MouseKind::Move => {
                if let Some(c) = self.map_cell(map, m.x, m.y) {
                    self.a.cursor = c;
                }
            }
        }
    }

    fn cell_look(&self, x: i32, y: i32) -> (char, char, Rgb, Rgb) {
        let a = &self.atlas;
        let i = a.idx(x, y);
        let e = a.elevation[i];
        let b = palette::biome(&a.biomes[a.biome[i] as usize % a.biomes.len().max(1)]);
        match self.a.view {
            View::Height => {
                let c = height_color(e);
                let g = if e < 0 { '~' } else { char::from_digit(((e / 10).min(9)) as u32, 10).unwrap() };
                (g, ' ', c.scale(0.45), c)
            }
            View::Rock => {
                let name = &a.rocks[a.rock[i] as usize % a.rocks.len().max(1)];
                let mats = Materials::standard();
                let m = mats.get(mats.find(name).unwrap_or(1));
                (m.wall_glyph, m.wall_glyph, m.fg, m.bg)
            }
            View::Biome => (b.glyph, ' ', b.color, b.color.scale(0.3)),
            View::Terrain => {
                if e < 0 {
                    let c = height_color(e);
                    let g = if e < -6 { '≈' } else { '~' };
                    (g, ' ', c.lerp(Rgb::WHITE, 0.3), c.scale(0.5))
                } else {
                    // Hill shading, light from the north-west.
                    let slope = a.elev(x - 1, y - 1) - a.elev(x + 1, y + 1);
                    let shade = (1.0 + slope * 0.035).clamp(0.55, 1.5);
                    let height = 0.75 + (e as f32 / 80.0).min(0.5);
                    let fg = b.color.scale(shade * height);
                    let bg = b.color.scale(0.22 * shade).lerp(height_color(e), 0.15);
                    (b.glyph, ' ', fg, bg)
                }
            }
        }
    }

    pub fn atlas_draw(&mut self, f: &mut Frame, map: Rect, side: Rect) {
        let (cols, rows) = (map.w / 2, map.h);
        // Keep the cursor on screen.
        let (cx, cy) = self.a.cursor;
        let m = 3;
        if cx < self.a.cam.0 + m || cx >= self.a.cam.0 + cols - m {
            self.a.cam.0 = cx - cols / 2;
        }
        if cy < self.a.cam.1 + m || cy >= self.a.cam.1 + rows - m {
            self.a.cam.1 = cy - rows / 2;
        }
        self.a.cam.0 = self.a.cam.0.clamp(-2, (self.atlas.w - cols + 2).max(-2));
        self.a.cam.1 = self.a.cam.1.clamp(-2, (self.atlas.h - rows + 2).max(-2));

        let rivers: Vec<(i32, i32)> = self.atlas.rivers.iter().flat_map(Atlas::river_cells).collect();
        let sources: Vec<(i32, i32)> = self.atlas.rivers.iter().filter_map(|r| r.points.first().copied()).collect();
        let draft = if self.a.draft.is_empty() {
            Vec::new()
        } else {
            let mut d = self.a.draft.clone();
            d.push(self.a.cursor);
            Atlas::river_cells(&River { width: 1, points: d })
        };
        let brush: Vec<(usize, f32)> =
            if matches!(self.a.tool, Tool::River | Tool::Feature) { Vec::new() } else { self.atlas.brush(cx, cy, self.a.radius) };
        for sy in 0..rows {
            for sx in 0..cols {
                let (x, y) = (self.a.cam.0 + sx, self.a.cam.1 + sy);
                let (px, py) = (map.x + sx * 2, map.y + sy);
                if !self.atlas.contains(x, y) {
                    f.set(px, py, Cell::new(' ', Rgb::BLACK, Rgb(4, 4, 8)));
                    f.set(px + 1, py, Cell::new(' ', Rgb::BLACK, Rgb(4, 4, 8)));
                    continue;
                }
                let (mut g1, mut g2, mut fg, mut bg) = self.cell_look(x, y);
                if rivers.contains(&(x, y)) && self.a.view != View::Rock {
                    g1 = if sources.contains(&(x, y)) { '◉' } else { '≈' };
                    g2 = ' ';
                    fg = Rgb(110, 180, 255);
                }
                if draft.contains(&(x, y)) {
                    g1 = if self.a.draft.contains(&(x, y)) { '•' } else { '·' };
                    fg = Rgb(160, 220, 255);
                }
                if let Some((_, ft)) = self.atlas.features_in(x, y).next() {
                    g1 = if ft.blueprint().is_some() { '■' } else { ft.kind.chars().next().unwrap_or('?').to_ascii_uppercase() };
                    g2 = ' ';
                    fg = Rgb(255, 220, 90);
                }
                let i = self.atlas.idx(x, y);
                if let Some((_, w)) = brush.iter().find(|(j, _)| *j == i) {
                    bg = bg.lerp(Rgb(200, 200, 120), 0.12 + 0.2 * w);
                }
                if (x, y) == self.a.cursor {
                    bg = Rgb(230, 200, 70);
                    fg = Rgb::BLACK;
                }
                f.set(px, py, Cell::new(g1, fg, bg));
                f.set(px + 1, py, Cell::new(g2, fg, bg));
            }
        }
        self.atlas_panel(f, side);
    }

    fn atlas_panel(&self, f: &mut Frame, side: Rect) {
        if side.w <= 0 {
            return;
        }
        let mut p = Panel::new(f, side);
        let a = &self.atlas;
        p.line(f, "TACHYON  world editor", TITLE);
        p.line(f, &format!("{}{}", self.path, if self.dirty { "  (unsaved)" } else { "" }), TXT);
        p.line(f, &format!("{}  {}x{} cells of {} m", a.name, a.w, a.h, REGION), DIM);
        let view = match self.a.view {
            View::Terrain => "terrain",
            View::Height => "height",
            View::Rock => "rock",
            View::Biome => "biome",
        };
        p.line(f, &format!("view: {view}   (Tab)"), TXT);
        p.gap();

        p.line(f, &format!("TOOL  {}", self.tool_name().to_uppercase()), TITLE);
        match self.a.tool {
            Tool::Raise => p.line(f, &format!("step {} m   brush {:.1}", self.a.step, self.a.radius), TXT),
            Tool::Height => {
                p.swatch(f, Cell::new(' ', Rgb::BLACK, height_color(self.a.target)), &format!("height {} m   brush {:.1}", self.a.target, self.a.radius), TXT)
            }
            Tool::Smooth => p.line(f, &format!("brush {:.1}", self.a.radius), TXT),
            Tool::Rock => {
                let mats = Materials::standard();
                let name = &a.rocks[self.a.rock % a.rocks.len()];
                let m = mats.get(mats.find(name).unwrap_or(1));
                p.swatch(f, Cell::new(m.wall_glyph, m.fg, m.bg), &format!("{name}   brush {:.1}", self.a.radius), TXT);
            }
            Tool::Biome => {
                let b = palette::biome(&a.biomes[self.a.biome % a.biomes.len()]);
                p.swatch(f, Cell::new(b.glyph, b.color, b.color.scale(0.3)), &format!("{}   brush {:.1}", b.name, self.a.radius), TXT);
            }
            Tool::River => {
                p.line(f, &format!("width {} m   points {}", self.a.river_width, self.a.draft.len()), TXT);
                p.line(f, "click: add point (first = source)", DIM);
                p.line(f, "Enter: finish  right: undo point", DIM);
            }
            Tool::Feature => {
                let kinds = Editor::feature_kinds();
                p.line(f, &format!("kind: {}", kinds[self.a.feature % kinds.len()]), TXT);
                p.line(f, "click: place   right: remove", DIM);
            }
        }
        p.gap();

        let (x, y) = self.a.cursor;
        let i = a.idx(x, y);
        p.line(f, &format!("CELL {x},{y}  ({} m, {} m)", x * REGION, y * REGION), TITLE);
        let e = a.elevation[i];
        p.line(f, &if e < 0 { format!("sea, {} m deep", -e) } else { format!("{e} m above sea") }, TXT);
        p.line(f, &format!("{} on {}", a.biomes.get(a.biome[i] as usize).map_or("?", |s| s), a.rocks.get(a.rock[i] as usize).map_or("?", |s| s)), TXT);
        for (_, ft) in a.features_in(x, y) {
            let label = if ft.label.is_empty() { String::new() } else { format!(" \"{}\"", ft.label) };
            p.line(f, &format!("* {}{}", ft.kind, label), Rgb(255, 220, 90));
        }
        if a.rivers.iter().any(|r| Atlas::river_cells(r).contains(&(x, y))) {
            p.line(f, "a river runs here", Rgb(110, 180, 255));
        }
        p.gap();

        if self.a.help {
            for (i, s) in [
                "KEYS",
                "1 raise 2 height 3 smooth 4 rock",
                "5 biome 6 river 7 feature",
                "arrows / mouse  move cursor",
                "Space / left    use tool",
                "Bksp / right    lower / pick / remove",
                "[ ] / wheel     brush size",
                "+ - PgUp PgDn   height, step, width",
                ", .             choose rock/biome/kind",
                "Tab  view       Enter  walk in (1 m)",
                "u  undo   S  save   I  rough island",
                "N  blank sea    ?  help   q  quit",
            ]
            .iter()
            .enumerate()
            {
                p.line(f, s, if i == 0 { TITLE } else { DIM });
            }
        }
        let _ = Mode::Atlas;
    }
}
