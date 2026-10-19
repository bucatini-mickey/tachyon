//! Drawing the world: one z-level at a time, Dwarf Fortress style.
//!
//! For each cell on screen the view finds what to show at the camera's level:
//! a thing (creature over furniture over item), fire, a wall, liquid, a floor.
//! If the cell is open space it looks down, level by level, and draws what it
//! finds there dimmed and hazed by depth, so cliffs, pits and the ground
//! beneath a tower read naturally.
//!
//! Looking up works the other way round: where the camera's level is solid
//! natural ground (a hillside) with a surface one or two levels above it,
//! that surface is drawn instead, faintly hazed, so a hill shows its grass
//! rather than a wall of soil. Built walls and deep rock stay as they are.
//!
//! With an observer set, only what the observer can see on their level (and
//! on the open levels just above their head) is drawn in full; cells seen before are drawn from memory in grey, and the
//! rest is black. Lighting multiplies sky light (scaled by the time of day)
//! and coloured dynamic light into every colour.
//!
//! Backgrounds are kept for what they mean, classic-roguelike style: floors
//! and walls are coloured glyphs on black, and only liquid, fire, gas, blood,
//! items on furniture, highlights and overlays colour a cell's background.
//! `fill` brings back the old look, every cell filled with its material.

pub mod fov;
pub mod inspect;

use crate::color::{ramp, Rgb};
use crate::frame::{Cell, Frame};
use crate::geom::{Pos, Rect};
use crate::material::WallStyle;
use crate::rng::hash3;
use crate::scene::Scene;
use crate::sim::light::{gather_sources, LightMap, LightSource};
use crate::things::Layer;
use crate::tile::Tile;
use crate::world::World;
use fov::FovMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Overlay {
    None,
    Temperature,
    Liquid,
    Gas,
    Light,
    Awake,
}

impl Overlay {
    pub fn next(self) -> Overlay {
        match self {
            Overlay::None => Overlay::Temperature,
            Overlay::Temperature => Overlay::Liquid,
            Overlay::Liquid => Overlay::Gas,
            Overlay::Gas => Overlay::Light,
            Overlay::Light => Overlay::Awake,
            Overlay::Awake => Overlay::None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Overlay::None => "none",
            Overlay::Temperature => "temperature",
            Overlay::Liquid => "liquid",
            Overlay::Gas => "gas",
            Overlay::Light => "light",
            Overlay::Awake => "active chunks",
        }
    }
}

#[derive(Clone, Copy, Default)]
struct Stamp {
    top: Option<(char, Rgb, Layer, bool)>, // glyph, colour, layer, emits light
    item: Option<Rgb>,
}

/// What one map cell looks like before lighting.
#[derive(Clone, Copy)]
struct Look {
    ch: char,
    ch2: char,
    fg: Rgb,
    bg: Rgb,
    emissive: bool,
    /// Levels below the camera that this was found at.
    k: i32,
    /// Where to sample light from.
    light_at: Pos,
    /// A wall face: light it from its brightest open neighbour.
    face: bool,
    /// The cell drawn (for overlays and inspection).
    at: Pos,
}

/// Tint a floor with what has been spilled on it (blood).
fn stain(scene: &Scene, at: Pos, l: &mut Look) {
    if let Some((c, a)) = scene.life.stains.look(at) {
        let i = scene.world.idx(at.x, at.y, at.z);
        if !scene.world.tile[i].is_wall() && scene.world.fluid[i] < 8 {
            l.bg = l.bg.lerp(c, 0.75 * a);
            l.fg = l.fg.lerp(c, a);
        }
    }
}

const VOID: Rgb = Rgb(6, 6, 9);
const HAZE: Rgb = Rgb(26, 34, 60);
/// Surfaces above the camera's level are hazed toward this, a little per level.
const UP_HAZE: Rgb = Rgb(150, 160, 180);
const AMBIENT: f32 = 0.05;

pub struct MapView {
    /// The cell at the centre of the view; its z is the level drawn.
    pub center: Pos,
    /// Two characters per cell: closer to square cells, and line walls join up.
    pub wide: bool,
    pub overlay: Overlay,
    /// How many levels to look down through open space.
    pub depth: i32,
    /// How many levels up a hillside's surface still shows (0 turns it off).
    pub up_depth: i32,
    pub lighting: bool,
    /// Fill floor and wall backgrounds with their material's colour (a tiled
    /// look). Off by default: they are drawn on black.
    pub fill: bool,
    /// Whose eyes we see through. `None` shows everything.
    pub observer: Option<Pos>,
    pub sight_radius: i32,
    pub cursor: Option<Pos>,
    /// Cells to highlight (an aiming line, a selected area).
    pub marks: Vec<Pos>,
    fov: FovMap,
    /// Sight on the open levels above the observer's head (1, 2, ...).
    fov_up: Vec<FovMap>,
    up_open: i32,
    seen: Vec<bool>,
    light: LightMap,
    sources: Vec<LightSource>,
    light_key: Option<(u64, i32, i32, i32, i32, i32)>,
    stamp: Vec<Stamp>,
}

impl Default for MapView {
    fn default() -> Self {
        Self::new()
    }
}

impl MapView {
    pub fn new() -> MapView {
        MapView {
            center: Pos::default(),
            wide: false,
            overlay: Overlay::None,
            depth: 4,
            up_depth: 2,
            lighting: true,
            fill: false,
            observer: None,
            sight_radius: 40,
            cursor: None,
            marks: Vec::new(),
            fov: FovMap::new(),
            fov_up: Vec::new(),
            up_open: 0,
            seen: Vec::new(),
            light: LightMap::new(),
            sources: Vec::new(),
            light_key: None,
            stamp: Vec::new(),
        }
    }

    /// Forget everything the observer has seen.
    pub fn forget(&mut self) {
        self.seen.clear();
    }

    pub fn has_seen(&self, world: &World, p: Pos) -> bool {
        world.index(p).is_some_and(|i| self.seen.get(i).copied().unwrap_or(false))
    }

    /// Can the observer see this cell right now?
    pub fn visible(&self, p: Pos) -> bool {
        match self.observer {
            None => true,
            Some(o) => p.z == o.z && self.fov.visible(p.x, p.y),
        }
    }

    /// Can the observer see this cell, on a level above their own, right now?
    fn visible_up(&self, p: Pos) -> bool {
        match self.observer {
            None => true,
            Some(o) => {
                let k = p.z - o.z;
                k >= 1 && k <= self.up_open && self.fov_up[k as usize - 1].visible(p.x, p.y)
            }
        }
    }

    /// Dynamic light at a cell from the last draw.
    pub fn light_at(&self, p: Pos) -> [u8; 3] {
        self.light.get(p.x, p.y, p.z)
    }

    /// Visible area in world cells for a screen area.
    pub fn cells_for(&self, area: Rect) -> (i32, i32) {
        (if self.wide { area.w / 2 } else { area.w }, area.h)
    }

    pub fn draw(&mut self, scene: &Scene, frame: &mut Frame, area: Rect) {
        let world = &scene.world;
        let things = &scene.things;
        let tick = scene.clock.tick;
        let day = scene.clock.daylight().light;
        let (cols, rows) = self.cells_for(area);
        if cols <= 0 || rows <= 0 {
            return;
        }
        let z = self.center.z.clamp(0, world.d - 1);
        let x0 = self.center.x - cols / 2;
        let y0 = self.center.y - rows / 2;
        let depth = self.depth.max(0);

        // Sight and memory.
        if self.seen.len() != world.len() {
            self.seen = vec![false; world.len()];
        }
        if let Some(o) = self.observer {
            if world.index(o).is_some() {
                let oz = o.z;
                self.fov.compute(world.w, world.h, (o.x, o.y), self.sight_radius, |x, y| {
                    world.opaque_idx(world.idx(x, y, oz)) || things.blocks_sight(Pos::new(x, y, oz))
                });
                let r = self.sight_radius;
                for y in (o.y - r).max(0)..(o.y + r + 1).min(world.h) {
                    for x in (o.x - r).max(0)..(o.x + r + 1).min(world.w) {
                        if !self.fov.visible(x, y) {
                            continue;
                        }
                        let mut zz = oz;
                        loop {
                            let i = world.idx(x, y, zz);
                            self.seen[i] = true;
                            if world.tile[i] != Tile::Open || world.fluid[i] > 0 || zz == 0 || oz - zz >= depth {
                                break;
                            }
                            zz -= 1;
                        }
                    }
                }
            }
        }

        // Sight on the levels just above the observer's head, as far up as
        // nothing overhead (a roof, a ceiling) blocks it.
        self.up_open = 0;
        if let Some(o) = self.observer {
            for k in 1..=self.up_depth {
                let zz = o.z + k;
                if world.index(o).is_none() || zz >= world.d || world.tile[world.idx(o.x, o.y, zz)].seals_below() {
                    break;
                }
                if self.fov_up.len() < k as usize {
                    self.fov_up.push(FovMap::new());
                }
                let fov = &mut self.fov_up[k as usize - 1];
                fov.compute(world.w, world.h, (o.x, o.y), self.sight_radius, |x, y| {
                    world.opaque_idx(world.idx(x, y, zz)) || things.blocks_sight(Pos::new(x, y, zz))
                });
                let r = self.sight_radius;
                for y in (o.y - r).max(0)..(o.y + r + 1).min(world.h) {
                    for x in (o.x - r).max(0)..(o.x + r + 1).min(world.w) {
                        if fov.visible(x, y) {
                            self.seen[world.idx(x, y, zz)] = true;
                        }
                    }
                }
                self.up_open = k;
            }
        }

        // Dynamic light for the region (recomputed once per tick or on camera move).
        if self.lighting {
            let m = 18;
            let key = (tick, x0, y0, z, cols, rows);
            if self.light_key != Some(key) {
                let (lx0, ly0, lz0, lx1, ly1, lz1) = (x0 - m, y0 - m, z - depth - 1, x0 + cols + m, y0 + rows + m, z + self.up_depth.max(1) + 1);
                gather_sources(world, things, lx0, ly0, lz0, lx1, ly1, lz1, tick, &mut self.sources);
                self.light.compute(world, lx0, ly0, lz0, lx1, ly1, lz1, &self.sources);
                self.light_key = Some(key);
            }
        }

        // Stamp things into a per-cell, per-level buffer: the top one wins.
        let levels = (depth + 1) as usize;
        let n = cols as usize * rows as usize * levels;
        self.stamp.clear();
        self.stamp.resize(n, Stamp::default());
        for (_, t) in things.iter() {
            let p = t.pos();
            let (sx, sy, k) = (p.x - x0, p.y - y0, z - p.z);
            if sx < 0 || sy < 0 || sx >= cols || sy >= rows || k < 0 || k > depth {
                continue;
            }
            let kind = things.kind_of(t);
            let s = &mut self.stamp[(k as usize * rows as usize + sy as usize) * cols as usize + sx as usize];
            let lit = things.shines(t) || kind.light.is_none();
            // A flame that has gone out is drawn as a dark, charred stick.
            let color = if lit { t.color.unwrap_or(kind.fg) } else { Rgb(96, 80, 68) };
            if kind.layer == Layer::Item && s.item.is_none() {
                s.item = Some(color);
            }
            if s.top.map_or(true, |(_, _, l, _)| kind.layer > l) {
                let glyph = if kind.door && t.open { kind.open_glyph } else { kind.glyph };
                s.top = Some((glyph, color, kind.layer, kind.light.is_some() && lit));
            }
        }

        // Compose.
        for sy in 0..rows {
            for sx in 0..cols {
                let (wx, wy) = (x0 + sx, y0 + sy);
                let (c1, c2) = self.cell(scene, wx, wy, z, sx, sy, cols, rows, day, tick);
                let px = area.x + if self.wide { sx * 2 } else { sx };
                let py = area.y + sy;
                frame.set(px, py, c1);
                if self.wide {
                    frame.set(px + 1, py, c2);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn cell(&self, scene: &Scene, x: i32, y: i32, z: i32, sx: i32, sy: i32, cols: i32, rows: i32, day: [f32; 3], tick: u64) -> (Cell, Cell) {
        let world = &scene.world;
        let blank = Cell::new(' ', Rgb::BLACK, Rgb::BLACK);
        if !world.contains(x, y, z) {
            let c = Cell::new(' ', VOID, VOID);
            return (c, c);
        }
        let p = Pos::new(x, y, z);
        let i0 = world.idx(x, y, z);
        if self.up_depth > 0 && world.tile[i0].is_wall() && world.mats.get(world.mat[i0]).style == WallStyle::Solid {
            if let Some((mut look, vis)) = self.above(scene, x, y, z, tick) {
                if vis {
                    if self.lighting && !look.emissive {
                        let l = self.light_level(world, look.light_at, false, day);
                        look.fg = look.fg.mul3(l);
                        look.bg = look.bg.mul3(l);
                    }
                    let h = 0.16 * look.k as f32;
                    look.fg = look.fg.lerp(UP_HAZE, h);
                    if self.fill {
                        look.bg = look.bg.scale(0.7).lerp(UP_HAZE, h * 0.35);
                    }
                } else {
                    look = Self::remembered(look);
                }
                return self.finish(look, p);
            }
        }
        let visible = self.visible(p);
        let remembered = !visible && self.seen.get(i0).copied().unwrap_or(false);
        if !visible && !remembered {
            return (blank, blank);
        }

        let mut look = self.look(scene, x, y, z, sx, sy, cols, rows, tick, remembered);

        if visible {
            // Gas hanging in the air at this level.
            let g = world.gas[i0];
            if g > 0 {
                let m = world.mats.get(world.gas_mat[i0]);
                let (_, _, opacity) = m.gas_props();
                let t = g as f32 / 255.0;
                look.bg = look.bg.lerp(m.bg, (t * 1.6).min(0.85));
                if g > opacity {
                    look.ch = if g > 200 { '▒' } else { '░' };
                    look.ch2 = look.ch;
                    look.fg = m.fg;
                    look.emissive = false;
                }
            }
            // Depth: dim and haze what lies below.
            if look.k > 0 {
                let f = (1.0 - 0.17 * look.k as f32).max(0.2);
                let h = 0.14 * look.k as f32;
                look.fg = look.fg.scale(f).lerp(HAZE, h);
                look.bg = if self.fill { look.bg.scale(f).lerp(HAZE, h * 0.6) } else { look.bg.scale(f) };
            }
            if self.lighting && !look.emissive {
                let l = self.light_level(world, look.light_at, look.face, day);
                look.fg = look.fg.mul3(l);
                look.bg = look.bg.mul3(l);
            }
            // Rain falling through open air, streaked along the wind.
            if world.rain > 0.0 && world.open_sky[i0] && !world.tile[i0].is_wall() && (hash3(x, y, z, tick as u32) & 1023) as f32 / 1024.0 < world.rain * 0.22 {
                let (wx, wy) = world.wind;
                look.ch = if wx.abs() + wy.abs() < 0.15 {
                    ','
                } else if wx.abs() > 2.0 * wy.abs() {
                    '-'
                } else if wy.abs() > 2.0 * wx.abs() {
                    '|'
                } else if (wx > 0.0) == (wy < 0.0) {
                    '/'
                } else {
                    '\\'
                };
                look.ch2 = if look.ch == '-' { '-' } else { ' ' };
                look.fg = Rgb(150, 170, 205).lerp(look.fg, 0.3);
            }
        } else {
            look = Self::remembered(look);
        }

        if self.overlay != Overlay::None && visible {
            self.apply_overlay(world, &mut look, i0, day);
        }

        self.finish(look, p)
    }

    /// Seen before but not now: grey and dim.
    fn remembered(mut look: Look) -> Look {
        let fl = look.fg.luma() * 0.32 + 18.0;
        look.fg = Rgb(fl as u8, fl as u8, (fl * 1.15).min(255.0) as u8);
        let bl = look.bg.luma() * 0.18;
        look.bg = Rgb(bl as u8, bl as u8, (bl * 1.2) as u8);
        look
    }

    /// Highlights and the cursor, then the cell pair.
    fn finish(&self, mut look: Look, p: Pos) -> (Cell, Cell) {
        if self.marks.contains(&p) {
            look.bg = look.bg.lerp(Rgb(200, 90, 30), 0.45);
        }
        let mut c1 = Cell::new(look.ch, look.fg, look.bg);
        let mut c2 = Cell::new(look.ch2, look.fg, look.bg);
        if self.cursor == Some(p) {
            let hl = Rgb(240, 210, 60);
            c1 = Cell::new(if c1.ch == ' ' { 'X' } else { c1.ch }, Rgb::BLACK, hl);
            c2 = Cell::new(' ', Rgb::BLACK, hl);
        }
        (c1, c2)
    }

    /// The surface on top of solid natural ground at (x, y, z), if it lies
    /// within `up_depth` levels and can be (or was) seen: its look, with `k`
    /// set to the levels up, and whether it is visible now.
    fn above(&self, scene: &Scene, x: i32, y: i32, z: i32, tick: u64) -> Option<(Look, bool)> {
        let world = &scene.world;
        let things = &scene.things;
        for k in 1..=self.up_depth {
            let zz = z + k;
            if zz >= world.d {
                return None;
            }
            let p = Pos::new(x, y, zz);
            let j = world.idx(x, y, zz);
            let tile = world.tile[j];
            if tile.is_wall() {
                if world.mats.get(world.mat[j]).style != WallStyle::Solid {
                    return None; // something built stands on the hill: keep the wall
                }
                continue;
            }
            // Open air straight above our own wall: that is just its top.
            let bare = !tile.has_floor() && world.fluid[j] == 0;
            if bare && k == 1 {
                return None;
            }
            let visible = self.visible_up(p);
            if !visible && !self.seen.get(j).copied().unwrap_or(false) {
                return None;
            }
            let mut l = Look { ch: ' ', ch2: ' ', fg: Rgb::BLACK, bg: Rgb::BLACK, emissive: false, k, light_at: p, face: false, at: p };
            if bare {
                // The top of the solid ground one level down.
                let m = world.mats.get(world.mat[j - world.layer()]);
                l.ch = m.floor_glyphs[(hash3(x, y, zz - 1, 7) as usize) % m.floor_glyphs.len().max(1)];
                l.fg = m.fg;
                l.bg = self.ground_bg(m);
            } else {
                l = self.terrain(world, things, x, y, zz, tick, l);
                l.k = k;
                l.light_at = p;
                l.face = false;
            }
            if visible && world.fire[j] > 0 {
                l.ch = ['^', '"', '*', '\''][(hash3(x, y, zz, (tick / 2) as u32) >> 4) as usize % 4];
                l.fg = Rgb(255, 150, 40);
                l.bg = Rgb(96, 22, 0);
                l.emissive = true;
            }
            // Whatever stands up there (only furniture is remembered).
            let top = things.at(p).map(|(_, t)| (t, things.kind_of(t))).filter(|(_, kd)| visible || kd.layer == Layer::Furniture).max_by_key(|(_, kd)| kd.layer);
            if let Some((t, kd)) = top {
                l.ch = if kd.door && t.open { kd.open_glyph } else { kd.glyph };
                l.fg = t.color.unwrap_or(kd.fg);
            }
            return Some((l, visible));
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    fn look(&self, scene: &Scene, x: i32, y: i32, z: i32, sx: i32, sy: i32, cols: i32, rows: i32, tick: u64, memory: bool) -> Look {
        let world = &scene.world;
        let things = &scene.things;
        let mut zz = z;
        let mut k = 0;
        loop {
            let i = world.idx(x, y, zz);
            let at = Pos::new(x, y, zz);
            let tile = world.tile[i];
            let stamp = if (k as usize) < self.stamp.len() / (cols as usize * rows as usize).max(1) {
                self.stamp[(k as usize * rows as usize + sy as usize) * cols as usize + sx as usize]
            } else {
                Stamp::default()
            };
            let mut base = Look { ch: ' ', ch2: ' ', fg: Rgb::BLACK, bg: Rgb::BLACK, emissive: false, k, light_at: at, face: false, at };

            // Things. From memory, only furniture is remembered.
            if let Some((glyph, fg, layer, emits)) = stamp.top {
                if !memory || layer == Layer::Furniture {
                    let mut under = self.terrain(world, things, x, y, zz, tick, base);
                    stain(scene, at, &mut under);
                    base.ch = glyph;
                    base.fg = fg;
                    base.bg = under.bg;
                    base.emissive = emits;
                    if layer == Layer::Furniture {
                        if let Some(item) = stamp.item {
                            base.bg = base.bg.lerp(item, 0.35);
                        }
                    }
                    return base;
                }
            }

            if world.fire[i] > 0 && !memory {
                let h = hash3(x, y, zz, (tick / 2) as u32);
                let flames = [Rgb(255, 232, 100), Rgb(255, 150, 40), Rgb(232, 72, 20)];
                base.fg = flames[(h % 3) as usize];
                base.bg = Rgb(96, 22, 0);
                base.emissive = true;
                if tile.is_wall() {
                    let w = self.terrain(world, things, x, y, zz, tick, base);
                    base.ch = w.ch;
                    base.ch2 = w.ch2;
                } else {
                    base.ch = ['^', '"', '*', '\''][(h >> 4) as usize % 4];
                }
                return base;
            }

            if tile != Tile::Open || world.fluid[i] > 0 {
                let mut l = self.terrain(world, things, x, y, zz, tick, base);
                stain(scene, at, &mut l);
                if tile.is_wall() {
                    if k == 0 {
                        l.face = true;
                    } else {
                        // The top of a wall seen from above: lit from the cell above it.
                        l.light_at = at.up();
                        let m = world.mats.get(world.mat[i]);
                        if m.style == WallStyle::Solid {
                            l.ch = m.floor_glyphs[(hash3(x, y, zz, 7) as usize) % m.floor_glyphs.len().max(1)];
                            l.ch2 = ' ';
                        }
                    }
                }
                return l;
            }

            if k >= self.depth || zz == 0 {
                base.bg = Rgb(4, 5, 10);
                return base;
            }
            zz -= 1;
            k += 1;
        }
    }

    /// The background behind a floor or wall of this material.
    #[inline]
    fn ground_bg(&self, m: &crate::material::Material) -> Rgb {
        if self.fill { m.bg } else { Rgb::BLACK }
    }

    /// Terrain look of a cell: liquid over floor, or the wall.
    #[allow(clippy::too_many_arguments)]
    fn terrain(&self, world: &World, things: &crate::things::Things, x: i32, y: i32, z: i32, tick: u64, mut l: Look) -> Look {
        let i = world.idx(x, y, z);
        let tile = world.tile[i];
        let m = world.mats.get(world.mat[i]);
        if tile.is_wall() {
            l.fg = m.fg;
            l.bg = self.ground_bg(m);
            match m.style {
                WallStyle::Solid => {
                    l.ch = m.wall_glyph;
                    l.ch2 = m.wall_glyph;
                }
                style => {
                    let joins = |dx: i32, dy: i32| {
                        let q = Pos::new(x + dx, y + dy, z);
                        match world.index(q) {
                            Some(j) => {
                                (world.tile[j].is_wall() && world.mats.get(world.mat[j]).style != WallStyle::Solid)
                                    || things.door_at(q).is_some()
                            }
                            None => false,
                        }
                    };
                    let mask = joins(0, -1) as usize | (joins(1, 0) as usize) << 1 | (joins(0, 1) as usize) << 2 | (joins(-1, 0) as usize) << 3;
                    let (set, horiz) = if style == WallStyle::DoubleLines {
                        (DOUBLE, '═')
                    } else {
                        (SINGLE, '─')
                    };
                    l.ch = if mask == 0 { m.wall_glyph } else { set[mask] };
                    l.ch2 = if mask & 2 != 0 { horiz } else { ' ' };
                }
            }
            return l;
        }
        // Floor first, liquid on top of it.
        l.fg = m.fg;
        l.bg = self.ground_bg(m);
        l.ch = match tile {
            Tile::Floor => m.floor_glyphs[(hash3(x, y, z, 7) as usize) % m.floor_glyphs.len().max(1)],
            Tile::Ramp => '▲',
            Tile::StairUp => '<',
            Tile::StairDown => '>',
            Tile::StairUpDown => 'X',
            _ => ' ',
        };
        if matches!(tile, Tile::StairUp | Tile::StairDown | Tile::StairUpDown) {
            l.fg = l.fg.lerp(Rgb::WHITE, 0.4);
        }
        l.ch2 = ' ';
        let f = world.fluid[i];
        if f > 0 {
            let fm = world.mats.get(world.fluid_mat[i]);
            let deep = f >= 56;
            l.ch = if deep {
                if hash3(x, y, z, (tick / 6) as u32) & 7 == 0 {
                    '~'
                } else {
                    '≈'
                }
            } else {
                char::from_digit((f as u32 * 7).div_ceil(64), 10).unwrap_or('~')
            };
            l.ch2 = if deep { l.ch } else { ' ' };
            l.fg = fm.fg;
            l.bg = fm.bg.scale(0.45 + 0.55 * f as f32 / 64.0);
            l.emissive = fm.glow.is_some();
        }
        l
    }

    fn light_level(&self, world: &World, p: Pos, face: bool, day: [f32; 3]) -> [f32; 3] {
        let sample = |q: Pos| -> [f32; 3] {
            let Some(i) = world.index(q) else { return [0.0; 3] };
            let s = world.sky[i] as f32 / 15.0;
            let s = s * s;
            let d = self.light.get(q.x, q.y, q.z);
            [
                s * day[0] + d[0] as f32 / 255.0 * 1.3,
                s * day[1] + d[1] as f32 / 255.0 * 1.3,
                s * day[2] + d[2] as f32 / 255.0 * 1.3,
            ]
        };
        let mut l = if face {
            // A wall is lit by the brightest open cell beside it.
            let mut best = [0.0f32; 3];
            for (dx, dy) in crate::geom::DIRS8 {
                let q = p.offset(dx, dy, 0);
                if world.passable(q) {
                    let s = sample(q);
                    for c in 0..3 {
                        best[c] = best[c].max(s[c]);
                    }
                }
            }
            best
        } else {
            sample(p)
        };
        for v in l.iter_mut() {
            *v = (*v + AMBIENT).min(1.35);
        }
        l
    }

    fn apply_overlay(&self, world: &World, look: &mut Look, i0: usize, day: [f32; 3]) {
        let Some(i) = world.index(look.at) else { return };
        let v = match self.overlay {
            Overlay::None => return,
            Overlay::Temperature => {
                let t = world.temp[i];
                Some(if t < 100.0 { (t + 20.0) / 240.0 } else { 0.5 + ((t - 100.0) / 1100.0).min(1.0) * 0.5 })
            }
            Overlay::Liquid => (world.fluid[i] > 0).then(|| world.fluid[i] as f32 / 64.0),
            Overlay::Gas => (world.gas[i0] > 0).then(|| world.gas[i0] as f32 / 255.0),
            Overlay::Light => {
                let l = self.light_level(world, look.light_at, look.face, day);
                Some((l[0] + l[1] + l[2]) / 3.0 / 1.35)
            }
            Overlay::Awake => {
                let p = look.at;
                Some(if world.wake.is_awake(world.wake.chunk_of(p.x, p.y, p.z)) { 0.8 } else { 0.1 })
            }
        };
        match v {
            Some(v) => {
                look.bg = ramp(v);
                look.fg = if look.bg.luma() > 110.0 { Rgb(20, 20, 20) } else { Rgb(220, 220, 220) };
            }
            None => {
                look.fg = look.fg.scale(0.4);
                look.bg = look.bg.scale(0.3);
            }
        }
    }
}

// Box-drawing joins, indexed by N=1, E=2, S=4, W=8.
const DOUBLE: [char; 16] = ['■', '║', '═', '╚', '║', '║', '╔', '╠', '═', '╝', '═', '╩', '╗', '╣', '╦', '╬'];
const SINGLE: [char; 16] = ['o', '│', '─', '└', '│', '│', '┌', '├', '─', '┘', '─', '┴', '┐', '┤', '┬', '┼'];
