//! The world: a 3D grid of one-metre cells.
//!
//! Storage is structure-of-arrays: each property of a cell (shape, material,
//! liquid, gas, temperature, ...) is its own flat `Vec`, indexed by
//! `(z * h + y) * w + x`. A system that only cares about temperature walks one
//! tightly packed array instead of hopping over whole cell structs, which is
//! what keeps the per-cell physics fast.
//!
//! The grid is divided into chunks of 16x16 cells on one level. Only chunks
//! where something changed recently are simulated (see [`Wake`]); a quiet
//! cave costs nothing per tick.

use crate::geom::Pos;
use crate::material::{MatId, Material, Materials, AIR};
use crate::tile::Tile;

/// A cell full of liquid. Liquid is counted in 64ths of a cell.
pub const FULL: u8 = 64;
/// Brightness of open sky in the sky-exposure field.
pub const SKY_MAX: u8 = 15;
/// What a quenched fire is cooled to (degrees C).
pub const QUENCH_TEMP: f32 = 100.0;
/// Chunk edge length (cells).
pub const CHUNK: i32 = 16;
const CHUNK_SHIFT: i32 = 4;

/// Which chunks are awake. Any change to a cell wakes its chunk (and the
/// neighbouring chunk when the cell is on a border) for the next tick. Chunks
/// with nothing changing fall asleep and are skipped.
#[derive(Clone, Debug)]
pub struct Wake {
    pub cw: i32,
    pub ch: i32,
    pub d: i32,
    w: i32,
    h: i32,
    layer: usize,
    cur: Vec<bool>,
    next: Vec<bool>,
    sweep: usize,
}

impl Wake {
    fn new(w: i32, h: i32, d: i32) -> Wake {
        let cw = (w + CHUNK - 1) / CHUNK;
        let ch = (h + CHUNK - 1) / CHUNK;
        let n = (cw * ch * d) as usize;
        Wake { cw, ch, d, w, h, layer: (w * h) as usize, cur: vec![true; n], next: vec![true; n], sweep: 0 }
    }

    #[inline]
    fn mark(&mut self, cx: i32, cy: i32, z: i32) {
        if cx >= 0 && cy >= 0 && z >= 0 && cx < self.cw && cy < self.ch && z < self.d {
            self.next[((z * self.ch + cy) * self.cw + cx) as usize] = true;
        }
    }

    /// Wake the chunk holding cell `i` (and neighbours it borders) next tick.
    #[inline]
    pub fn touch(&mut self, i: usize) {
        let z = (i / self.layer) as i32;
        let r = i % self.layer;
        let x = (r % self.w as usize) as i32;
        let y = (r / self.w as usize) as i32;
        self.touch_xyz(x, y, z);
    }

    #[inline]
    pub fn touch_xyz(&mut self, x: i32, y: i32, z: i32) {
        let (cx, cy) = (x >> CHUNK_SHIFT, y >> CHUNK_SHIFT);
        self.mark(cx, cy, z);
        let (lx, ly) = (x & (CHUNK - 1), y & (CHUNK - 1));
        if lx == 0 {
            self.mark(cx - 1, cy, z);
        } else if lx == CHUNK - 1 {
            self.mark(cx + 1, cy, z);
        }
        if ly == 0 {
            self.mark(cx, cy - 1, z);
        } else if ly == CHUNK - 1 {
            self.mark(cx, cy + 1, z);
        }
        self.mark(cx, cy, z - 1);
        self.mark(cx, cy, z + 1);
    }

    pub fn wake_all(&mut self) {
        self.cur.fill(true);
        self.next.fill(true);
    }

    /// The chunks awake this tick, bottom level first.
    pub fn collect(&self, out: &mut Vec<usize>) {
        out.clear();
        out.extend(self.cur.iter().enumerate().filter(|(_, &a)| a).map(|(i, _)| i));
    }

    /// Cell bounds of a chunk: `(x0, y0, x1, y1, z)`, upper bounds exclusive.
    #[inline]
    pub fn bounds(&self, c: usize) -> (i32, i32, i32, i32, i32) {
        let per = (self.cw * self.ch) as usize;
        let z = (c / per) as i32;
        let r = (c % per) as i32;
        let (cx, cy) = (r % self.cw, r / self.cw);
        let (x0, y0) = (cx * CHUNK, cy * CHUNK);
        (x0, y0, (x0 + CHUNK).min(self.w), (y0 + CHUNK).min(self.h), z)
    }

    pub fn chunk_of(&self, x: i32, y: i32, z: i32) -> usize {
        ((z * self.ch + (y >> CHUNK_SHIFT)) * self.cw + (x >> CHUNK_SHIFT)) as usize
    }

    pub fn is_awake(&self, c: usize) -> bool {
        self.cur[c]
    }

    pub fn awake_count(&self) -> usize {
        self.cur.iter().filter(|&&a| a).count()
    }

    pub fn total(&self) -> usize {
        self.cur.len()
    }

    /// End the tick: the chunks touched this tick become next tick's awake
    /// set. A slow rolling sweep also wakes a few chunks every tick, so that
    /// slow drifts (the day warming the ground) still reach sleeping chunks.
    pub fn finish(&mut self) {
        let n = self.cur.len();
        for _ in 0..(n / 256).max(1) {
            self.next[self.sweep] = true;
            self.sweep = (self.sweep + 1) % n;
        }
        std::mem::swap(&mut self.cur, &mut self.next);
        self.next.fill(false);
    }
}

/// The cell grid. Fields are public so systems can work on the raw arrays;
/// use the methods for edits from game code, since they keep the wake set
/// and sky light up to date.
#[derive(Clone, Debug)]
pub struct World {
    pub w: i32,
    pub h: i32,
    pub d: i32,
    pub mats: Materials,

    /// Shape of each cell.
    pub tile: Vec<Tile>,
    /// Material of the wall or floor.
    pub mat: Vec<MatId>,
    /// Liquid amount, 0..=FULL.
    pub fluid: Vec<u8>,
    pub fluid_mat: Vec<MatId>,
    /// Gas density, 0..=255.
    pub gas: Vec<u8>,
    pub gas_mat: Vec<MatId>,
    /// Temperature, degrees C.
    pub temp: Vec<f32>,
    /// Remaining burn time in ticks; 0 = not burning.
    pub fire: Vec<u16>,
    /// Exposure to the sky, 0..=SKY_MAX. Derived; see `sim::light`. This is
    /// light: it leaks sideways through doors and windows.
    pub sky: Vec<u8>,
    /// Nothing at all overhead (no roof, floor, or glass): the weather (rain,
    /// wind) reaches this cell. Derived with `sky`.
    pub open_sky: Vec<bool>,
    /// How liquid has been moving through each cell lately: a smoothed net
    /// flux in units (64ths) per tick along x and y. Derived; see `sim::fluid`.
    /// Divide by the depth for a speed in cells per tick (see [`World::current`]).
    pub flow_x: Vec<f32>,
    pub flow_y: Vec<f32>,

    /// Outdoor air temperature right now (set from the clock each tick).
    pub ambient: f32,
    /// Deep-earth temperature that buried cells drift toward.
    pub ground_temp: f32,
    /// Wind over open ground, in cells per tick (x, y). Set by `Weather`.
    pub wind: (f32, f32),
    /// Rain, 0 (dry) to 1 (a downpour). Set by `Weather`.
    pub rain: f32,
    /// Terrain changed: sky exposure needs recomputing.
    pub sky_dirty: bool,
    pub wake: Wake,
}

impl World {
    /// An empty world of open air at 15 C.
    pub fn new(w: i32, h: i32, d: i32, mats: Materials) -> World {
        assert!(w > 0 && h > 0 && d > 0, "world dimensions must be positive");
        let n = (w as usize) * (h as usize) * (d as usize);
        World {
            w,
            h,
            d,
            mats,
            tile: vec![Tile::Open; n],
            mat: vec![AIR; n],
            fluid: vec![0; n],
            fluid_mat: vec![AIR; n],
            gas: vec![0; n],
            gas_mat: vec![AIR; n],
            temp: vec![15.0; n],
            fire: vec![0; n],
            sky: vec![0; n],
            open_sky: vec![false; n],
            flow_x: vec![0.0; n],
            flow_y: vec![0.0; n],
            ambient: 15.0,
            ground_temp: 10.0,
            wind: (0.0, 0.0),
            rain: 0.0,
            sky_dirty: true,
            wake: Wake::new(w, h, d),
        }
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.tile.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tile.is_empty()
    }

    /// Cells per level.
    #[inline(always)]
    pub fn layer(&self) -> usize {
        (self.w * self.h) as usize
    }

    #[inline(always)]
    pub fn contains(&self, x: i32, y: i32, z: i32) -> bool {
        x >= 0 && y >= 0 && z >= 0 && x < self.w && y < self.h && z < self.d
    }

    #[inline(always)]
    pub fn idx(&self, x: i32, y: i32, z: i32) -> usize {
        ((z * self.h + y) * self.w + x) as usize
    }

    #[inline]
    pub fn index(&self, p: Pos) -> Option<usize> {
        if self.contains(p.x, p.y, p.z) {
            Some(self.idx(p.x, p.y, p.z))
        } else {
            None
        }
    }

    pub fn pos(&self, i: usize) -> Pos {
        let layer = self.layer();
        let z = (i / layer) as i32;
        let r = i % layer;
        Pos::new((r % self.w as usize) as i32, (r / self.w as usize) as i32, z)
    }

    /// Shape at `p`. Outside the world counts as solid wall.
    #[inline]
    pub fn tile(&self, p: Pos) -> Tile {
        match self.index(p) {
            Some(i) => self.tile[i],
            None => Tile::Wall,
        }
    }

    pub fn material_at(&self, p: Pos) -> &Material {
        match self.index(p) {
            Some(i) => self.mats.get(self.mat[i]),
            None => self.mats.get(AIR),
        }
    }

    /// Can a creature stand here? It needs a floor, or solid ground below.
    pub fn standable(&self, p: Pos) -> bool {
        let Some(i) = self.index(p) else { return false };
        let t = self.tile[i];
        if t.is_wall() {
            return false;
        }
        t.has_floor() || (p.z > 0 && self.tile[i - self.layer()].is_wall())
    }

    #[inline]
    pub fn passable(&self, p: Pos) -> bool {
        !self.tile(p).is_wall()
    }

    /// Does this cell block sight? Opaque walls and thick gas do.
    #[inline]
    pub fn opaque_idx(&self, i: usize) -> bool {
        if self.tile[i].is_wall() {
            return !self.mats.get(self.mat[i]).transparent;
        }
        let g = self.gas[i];
        g > 0 && g > self.mats.get(self.gas_mat[i]).gas_props().2
    }

    /// Does this cell block light? Like sight, but gas only dims light.
    #[inline]
    pub fn blocks_light_idx(&self, i: usize) -> bool {
        self.tile[i].is_wall() && !self.mats.get(self.mat[i]).transparent
    }

    /// Can light (or air) pass from cell `i` down into the cell below it?
    #[inline]
    pub fn opens_down_idx(&self, i: usize) -> bool {
        let t = self.tile[i];
        if t.is_wall() {
            return self.mats.get(self.mat[i]).transparent;
        }
        !t.seals_below()
    }

    #[inline]
    pub fn touch(&mut self, i: usize) {
        self.wake.touch(i);
    }

    // ---- edits -----------------------------------------------------------

    /// Change the shape and material of a cell. Walls displace liquid and gas.
    pub fn set_tile(&mut self, p: Pos, tile: Tile, mat: MatId) {
        let Some(i) = self.index(p) else { return };
        self.tile[i] = tile;
        self.mat[i] = mat;
        self.fire[i] = 0;
        if tile.is_wall() {
            self.fluid[i] = 0;
            self.fluid_mat[i] = AIR;
            self.gas[i] = 0;
            self.gas_mat[i] = AIR;
        }
        self.touch(i);
        self.sky_dirty = true;
    }

    /// Set the liquid in a cell outright.
    pub fn set_fluid(&mut self, p: Pos, mat: MatId, amount: u8) {
        let Some(i) = self.index(p) else { return };
        if self.tile[i].is_wall() {
            return;
        }
        let amount = amount.min(FULL);
        self.fluid[i] = amount;
        self.fluid_mat[i] = if amount == 0 { AIR } else { mat };
        if let Some(t) = self.mats.get(mat).self_temp {
            if amount > 0 {
                self.temp[i] = t;
            }
        }
        self.touch(i);
    }

    /// Pour liquid into a cell. Returns how much fitted.
    pub fn add_fluid(&mut self, p: Pos, mat: MatId, amount: u8) -> u8 {
        let Some(i) = self.index(p) else { return 0 };
        if self.tile[i].is_wall() || (self.fluid[i] > 0 && self.fluid_mat[i] != mat) {
            return 0;
        }
        let add = amount.min(FULL - self.fluid[i]);
        if add > 0 {
            let cur = self.fluid[i];
            self.set_fluid(p, mat, cur + add);
        }
        add
    }

    /// Take up to `amount` of whatever liquid is in a cell. Returns how much.
    pub fn take_fluid(&mut self, p: Pos, amount: u8) -> u8 {
        let Some(i) = self.index(p) else { return 0 };
        let take = amount.min(self.fluid[i]);
        if take > 0 {
            let (mat, left) = (self.fluid_mat[i], self.fluid[i] - take);
            self.set_fluid(p, mat, left);
        }
        take
    }

    /// The water surface over column (x, y), searching down from level `z`
    /// through `below` more levels: the height of the top of the liquid, in
    /// cells (level 4 full to the brim is 5.0), or `None` if it is dry.
    pub fn surface(&self, x: i32, y: i32, z: i32, below: i32) -> Option<f32> {
        for zz in (z - below..=z).rev() {
            if let Some(i) = self.index(Pos::new(x, y, zz)) {
                if self.fluid[i] > 0 {
                    return Some(zz as f32 + self.fluid[i] as f32 / FULL as f32);
                }
                if self.tile[i].seals_below() {
                    return None;
                }
            }
        }
        None
    }

    /// Speed of the liquid in a cell, in cells per tick (x, y).
    pub fn current(&self, p: Pos) -> (f32, f32) {
        let Some(i) = self.index(p) else { return (0.0, 0.0) };
        let depth = (self.fluid[i] as f32).max(8.0);
        (self.flow_x[i] / depth, self.flow_y[i] / depth)
    }

    /// Release gas into a cell. A different gas already there is displaced
    /// only by a larger amount.
    pub fn add_gas_idx(&mut self, i: usize, mat: MatId, amount: u8) {
        if self.tile[i].is_wall() || amount == 0 {
            return;
        }
        if self.gas[i] == 0 || self.gas_mat[i] == mat {
            self.gas[i] = self.gas[i].saturating_add(amount);
            self.gas_mat[i] = mat;
        } else if amount > self.gas[i] {
            self.gas[i] = amount;
            self.gas_mat[i] = mat;
        }
        self.touch(i);
    }

    pub fn add_gas(&mut self, p: Pos, mat: MatId, amount: u8) {
        if let Some(i) = self.index(p) {
            self.add_gas_idx(i, mat, amount);
        }
    }

    pub fn set_temp(&mut self, p: Pos, t: f32) {
        if let Some(i) = self.index(p) {
            self.temp[i] = t;
            self.touch(i);
        }
    }

    /// Set a cell alight if its wall or floor can burn. Returns success.
    pub fn ignite(&mut self, p: Pos) -> bool {
        let Some(i) = self.index(p) else { return false };
        if self.tile[i] == Tile::Open || self.fluid[i] >= 8 {
            return false;
        }
        let m = self.mats.get(self.mat[i]);
        if m.ignite.is_none() {
            return false;
        }
        self.fire[i] = m.burn_ticks.max(1);
        self.temp[i] = self.temp[i].max(m.ignite.unwrap_or(0.0) + 50.0);
        self.touch(i);
        true
    }

    /// Put out a fire with water. The water boils off and takes the heat with
    /// it, so the cell is left too cool to catch again straight away, with a
    /// puff of steam. Returns whether anything was burning.
    pub fn quench(&mut self, i: usize) -> bool {
        if self.fire[i] == 0 {
            return false;
        }
        self.fire[i] = 0;
        self.temp[i] = self.temp[i].min(QUENCH_TEMP);
        if let Some(steam) = self.mats.find("steam") {
            let layer = self.layer();
            let above = i + layer;
            let target = if !self.tile[i].is_wall() { i } else if above < self.len() { above } else { i };
            self.add_gas_idx(target, steam, 40);
        }
        self.touch(i);
        true
    }

    /// Turn the liquid in cell `i` into a solid: a full wall if there was a
    /// lot of it, a crust of floor if only a little.
    pub fn solidify(&mut self, i: usize, solid: MatId) {
        let amount = self.fluid[i];
        self.fluid[i] = 0;
        self.fluid_mat[i] = AIR;
        if amount >= 40 {
            self.tile[i] = Tile::Wall;
            self.mat[i] = solid;
            self.gas[i] = 0;
            self.gas_mat[i] = AIR;
        } else if matches!(self.tile[i], Tile::Open | Tile::Floor) {
            self.tile[i] = Tile::Floor;
            self.mat[i] = solid;
        }
        self.fire[i] = 0;
        self.touch(i);
        self.sky_dirty = true;
    }

    /// A rough count of how much liquid of a material exists (for tests).
    pub fn total_fluid(&self, mat: MatId) -> u64 {
        self.fluid
            .iter()
            .zip(&self.fluid_mat)
            .filter(|(_, &m)| m == mat)
            .map(|(&f, _)| f as u64)
            .sum()
    }
}
