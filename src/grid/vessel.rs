//! Vessels: rafts, boats and ships. A vessel is a rigid group of deck things
//! (its parts) that moves as one over the liquid in the world.
//!
//! Its position is continuous (cells, as floats) and its parts are drawn at
//! the rounded cell, so a slow raft creeps across the grid one cell at a time.
//! Each tick it:
//!
//! - **floats** at the height of the liquid under it, moving up or down a
//!   level when the surface does (with some hysteresis, so it doesn't flicker);
//! - is **dragged** toward the speed of the liquid under its wet parts (the
//!   world's flow field, so a river carries it and a drain pulls it in), and
//!   pushed by the **wind**;
//! - **stops** against walls, things that block movement, and shallows too
//!   thin to float in (it runs aground rather than climbing onto the shore);
//! - **pushes water aside**: each cell its bow advances into gives up a few
//!   units of liquid, which are piled up beside and ahead of it. Mass is
//!   conserved, and the fluid sim spreads the pile out again as a wake.
//!
//! Anything standing or lying on a part moves with the vessel.
//!
//! This works cell by cell, which suits rafts and boats up to a few dozen
//! cells. A large ship on an open sea can use the same type, but would want a
//! cheaper model of the sea itself (see the roadmap in CLAUDE.md).

use crate::geom::Pos;
use crate::things::{KindId, ThingId, Things};
use crate::world::World;

#[derive(Clone, Copy, Debug)]
pub struct Part {
    /// Offset from the vessel's origin cell.
    pub dx: i32,
    pub dy: i32,
    pub id: ThingId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Afloat {
    /// Floating freely.
    Afloat,
    /// Some parts rest on the bottom; it drags.
    Touching,
    /// No liquid under it at all.
    Aground,
}

#[derive(Clone, Debug)]
pub struct Vessel {
    pub name: String,
    pub parts: Vec<Part>,
    /// Origin position in cells; the origin cell is the rounded value.
    pub x: f32,
    pub y: f32,
    /// The level the deck is on.
    pub z: i32,
    /// Velocity in cells per tick.
    pub vx: f32,
    pub vy: f32,
    /// Fraction of the difference from the water's speed removed per tick.
    /// Higher is a lighter, flatter hull that the current grabs quickly.
    pub drag: f32,
    /// The same for the wind.
    pub windage: f32,
    pub max_speed: f32,
    /// Depth of liquid (in cells) needed under a part for it to float over a
    /// floor, rather than scrape the bottom.
    pub draft: f32,
    /// Units of liquid each bow cell pushes aside per cell moved.
    pub wake: u8,
    pub state: Afloat,
    cell: (i32, i32),
}

impl Vessel {
    /// Spawn a vessel from a plan, one string per row, with the origin at the
    /// top-left character placed at `origin`. `legend` maps characters to
    /// the kind of part (a deck kind); spaces and unmapped characters are
    /// empty.
    pub fn build(name: &str, things: &mut Things, origin: Pos, plan: &[&str], legend: impl Fn(char) -> Option<KindId>) -> Vessel {
        let mut parts = Vec::new();
        for (dy, row) in plan.iter().enumerate() {
            for (dx, ch) in row.chars().enumerate() {
                if let Some(kind) = legend(ch) {
                    let (dx, dy) = (dx as i32, dy as i32);
                    let id = things.spawn(kind, origin.offset(dx, dy, 0));
                    parts.push(Part { dx, dy, id });
                }
            }
        }
        Vessel {
            name: name.into(),
            parts,
            x: origin.x as f32,
            y: origin.y as f32,
            z: origin.z,
            vx: 0.0,
            vy: 0.0,
            drag: 0.08,
            windage: 0.004,
            max_speed: 0.4,
            draft: 0.15,
            wake: 2,
            state: Afloat::Afloat,
            cell: (origin.x, origin.y),
        }
    }

    pub fn origin(&self) -> Pos {
        Pos::new(self.cell.0, self.cell.1, self.z)
    }

    pub fn speed(&self) -> f32 {
        (self.vx * self.vx + self.vy * self.vy).sqrt()
    }

    /// Is `p` one of this vessel's deck cells?
    pub fn contains(&self, p: Pos) -> bool {
        p.z == self.z && self.has_offset(p.x - self.cell.0, p.y - self.cell.1)
    }

    fn has_offset(&self, dx: i32, dy: i32) -> bool {
        self.parts.iter().any(|p| p.dx == dx && p.dy == dy)
    }

    /// Give it a shove (paddling, a collision): adds to the velocity.
    pub fn push(&mut self, dx: f32, dy: f32) {
        self.vx += dx;
        self.vy += dy;
        self.clamp_speed();
    }

    fn clamp_speed(&mut self) {
        let s = self.speed();
        if s > self.max_speed {
            self.vx *= self.max_speed / s;
            self.vy *= self.max_speed / s;
        }
    }

    /// Can a part sit at (x, y) on level `z`? It must not be inside a wall
    /// or something solid, and must have liquid under it deep enough to float.
    fn fits(&self, world: &World, things: &Things, x: i32, y: i32, z: i32) -> bool {
        let p = Pos::new(x, y, z);
        let Some(i) = world.index(p) else { return false };
        if world.tile[i].is_wall() {
            return false;
        }
        if things.at(p).any(|(id, t)| things.kind(t.kind).blocks_move && !self.parts.iter().any(|q| q.id == id)) {
            return false;
        }
        match world.surface(x, y, z, 1) {
            None => false,
            Some(h) => h >= z as f32 - 0.5 && !(world.tile[i].has_floor() && h < z as f32 + self.draft),
        }
    }

    /// Advance one tick.
    pub fn step(&mut self, world: &mut World, things: &mut Things) {
        // Parts can be destroyed (a blast): what is left floats on.
        self.parts.retain(|p| things.get(p.id).is_some());
        if self.parts.is_empty() {
            return;
        }
        // Float: what is under each part?
        let (mut wet, mut touching, mut level, mut cur) = (0, 0, 0.0, (0.0, 0.0));
        for p in &self.parts {
            let (x, y) = (self.cell.0 + p.dx, self.cell.1 + p.dy);
            let Some(h) = world.surface(x, y, self.z, 1) else { continue };
            if h < self.z as f32 - 0.75 {
                continue;
            }
            wet += 1;
            level += h;
            let floor = world.index(Pos::new(x, y, self.z)).is_some_and(|i| world.tile[i].has_floor());
            if floor && h < self.z as f32 + self.draft {
                touching += 1;
            }
            for dz in [0, -1] {
                let (cx, cy) = world.current(Pos::new(x, y, self.z + dz));
                cur.0 += cx;
                cur.1 += cy;
            }
        }
        let n = self.parts.len().max(1) as f32;
        self.state = if wet == 0 {
            Afloat::Aground
        } else if touching > 0 {
            Afloat::Touching
        } else {
            Afloat::Afloat
        };
        let mut dz = 0;
        if wet > 0 {
            let level = level / wet as f32;
            if level >= (self.z + 1) as f32 {
                dz = 1;
            } else if level < self.z as f32 - 0.25 {
                dz = -1;
            }
            // The current under the wet part of the hull drags it along.
            let k = self.drag * wet as f32 / n;
            self.vx += (cur.0 / wet as f32 - self.vx) * k;
            self.vy += (cur.1 / wet as f32 - self.vy) * k;
        }
        let (wx, wy) = world.wind;
        self.vx += (wx - self.vx) * self.windage;
        self.vy += (wy - self.vy) * self.windage;
        let grip = match self.state {
            Afloat::Afloat => 1.0,
            Afloat::Touching => 1.0 - 0.6 * touching as f32 / n,
            Afloat::Aground => 0.0,
        };
        self.vx *= grip;
        self.vy *= grip;
        self.clamp_speed();

        // Move, one axis at a time so it can slide along a bank.
        let (ox, oy, oz) = (self.cell.0, self.cell.1, self.z);
        if dz != 0 && self.parts.iter().any(|p| world.tile(Pos::new(ox + p.dx, oy + p.dy, oz + dz)).is_wall()) {
            dz = 0;
        }
        let z = oz + dz;
        let mut cx = ox;
        self.x += self.vx;
        let nx = self.x.round() as i32;
        if nx != cx {
            if self.parts.iter().all(|p| self.fits(world, things, nx + p.dx, oy + p.dy, z)) {
                cx = nx;
            } else {
                self.x = cx as f32;
                self.vx *= -0.2;
            }
        }
        let mut cy = oy;
        self.y += self.vy;
        let ny = self.y.round() as i32;
        if ny != cy {
            if self.parts.iter().all(|p| self.fits(world, things, cx + p.dx, ny + p.dy, z)) {
                cy = ny;
            } else {
                self.y = cy as f32;
                self.vy *= -0.2;
            }
        }
        let (mx, my) = (cx - ox, cy - oy);
        if (mx, my, dz) == (0, 0, 0) {
            return;
        }

        // Carry everything aboard, then the deck itself.
        let mut moves: Vec<(ThingId, Pos)> = Vec::new();
        for p in &self.parts {
            let at = Pos::new(ox + p.dx, oy + p.dy, oz);
            for (id, _) in things.at(at) {
                moves.push((id, at.offset(mx, my, dz)));
            }
        }
        for (id, to) in moves {
            things.move_to(id, to);
        }
        self.cell = (cx, cy);
        self.z = z;

        // Push the water aside at the bow: lift a little from under each
        // newly covered cell and pile it beside and ahead of it.
        if self.wake > 0 && (mx, my) != (0, 0) {
            let bows: Vec<(i32, i32)> =
                self.parts.iter().filter(|p| !self.has_offset(p.dx - mx, p.dy - my)).map(|p| (cx + p.dx, cy + p.dy)).collect();
            for (x, y) in bows {
                let from = [Pos::new(x, y, z), Pos::new(x, y, z - 1)];
                let Some(&src) = from.iter().find(|&&q| world.index(q).is_some_and(|i| world.fluid[i] > 0)) else { continue };
                let mat = world.fluid_mat[world.index(src).unwrap()];
                // Ahead, and to either side of the direction of travel.
                let aside = [(mx, my), (-my, mx), (my, -mx)];
                for (ax, ay) in aside {
                    let q = Pos::new(x + ax, y + ay, z);
                    if self.contains(q) || world.tile(q).is_wall() {
                        continue;
                    }
                    let got = world.take_fluid(src, self.wake);
                    let put = world.add_fluid(q, mat, got);
                    if put < got {
                        world.add_fluid(src, mat, got - put);
                    }
                }
            }
        }
    }
}
