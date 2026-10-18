//! Round shot and shells: real trajectories through sea space and real
//! holes in the grids they hit.
//!
//! A [`Shot`] flies in sea space (metres; x east, y south, z up from mean sea
//! level) under gravity and quadratic air drag, with the drag coefficient of
//! a sphere rising through the sound barrier, so a ball fired at 1.4 times
//! the speed of sound slows hard for its first few hundred metres. The drag
//! step is the exact solution for a quadratic drag, so it is stable even in
//! water, where a ball loses most of its speed within a few metres.
//!
//! What it can meet:
//! - **Bodies**: any grid of cells placed in sea space by a [`Placement`] -
//!   a ship (rotated, heeled and pitching with her), a fort, a town. The
//!   flight is traced cell by cell through the grid (a 3D DDA). A wall soaks
//!   up `resistance(material) x shot area x path length` joules; a shot with
//!   more than that goes through, slower, and leaves a hole; one with less
//!   lodges. Floors are thin slabs at the bottom of their cell. Creatures and
//!   heavy furniture in the way are struck and reported. Each hole adds to
//!   the cell's damage, and a cell holed enough times gives way.
//! - **The sea**: below a grazing angle of about 6 degrees a ball skips off
//!   the water (ricochet fire), losing speed each time; steeper, it plunges
//!   and carries on under water for a few metres, so a shot just short of a
//!   hull can still hole it below the waterline.
//! - **Land**: the sea's bathymetry above the surface.
//!
//! Shells carry a [`Blast`] and burst on impact or when the fuse runs out.

use crate::geom::Pos;
use crate::material::{Material, MatId, AIR};
use crate::projectile::{explode, Blast, BlastReport};
use crate::rng::Rng;
use crate::sea::{Sea, G, RHO_AIR, RHO_SEA};
use crate::things::{Layer, ThingId, Things};
use crate::tile::Tile;
use crate::world::World;
use std::collections::HashMap;

pub type V3 = [f32; 3];

/// Joules per cubic metre of path to push a shot through a material, per
/// point of its `strength`. Oak (wood, 80) comes to about 48 MJ/m^3, its
/// crushing strength, so a 24-pounder at 300 m/s goes through 0.6 m of it.
pub const RESISTANCE_PER_STRENGTH: f32 = 6.0e5;
/// Damage a 24-pound ball's hole does to a 1 m cell (it breaks after ~12).
pub const HOLE_DAMAGE: f32 = 0.08;
/// Cross-section of a 24-pound ball (m^2): the reference hole.
pub const HOLE_REF: f32 = 0.0159;
/// Floors are slabs this thick (m) at the bottom of their cell.
pub const FLOOR_THICK: f32 = 0.12;
/// Density of cast iron, kg/m^3.
pub const IRON: f32 = 7200.0;
/// Below this grazing angle (radians) a ball skips off the water: Johnson's
/// rule, 18 degrees over the square root of the density ratio.
pub const RICOCHET_ANGLE: f32 = 0.113;

/// Joules per cubic metre to drive a shot through `m`.
pub fn resistance(m: &Material) -> f32 {
    m.strength * RESISTANCE_PER_STRENGTH
}

#[inline]
fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
#[inline]
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
#[inline]
fn scale(a: V3, s: f32) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
#[inline]
fn len(a: V3) -> f32 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

/// How a grid of 1 m cells sits in sea space. Local coordinates are cell
/// units (metres): x across the grid, y down it, z up its levels, so cell
/// (x, y, z) spans `[x, x+1) x [y, y+1) x [z, z+1)`. A world map uses the
/// identity (north up); a ship's grid has her bow toward -y and turns,
/// pitches and heels with her.
#[derive(Clone, Copy, Debug)]
pub struct Placement {
    /// Sea-space position of the pivot.
    pub t: V3,
    /// Rotation from local axes to sea axes (row-major).
    pub r: [[f32; 3]; 3],
    /// The pivot in local coordinates.
    pub pivot: V3,
}

impl Placement {
    /// An unrotated grid whose corner (0, 0, 0) is at `origin`.
    pub fn fixed(origin: V3) -> Placement {
        Placement { t: origin, r: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], pivot: [0.0; 3] }
    }

    /// A grid with `pivot` at sea-space `at`, its -y axis pointing along
    /// compass `heading`, bow-up `pitch` and starboard-down `roll` (radians).
    pub fn pose(at: V3, pivot: V3, heading: f32, pitch: f32, roll: f32) -> Placement {
        let (sh, ch) = heading.sin_cos();
        let (sp, cp) = (-pitch).sin_cos();
        let (sr, cr) = roll.sin_cos();
        let rz = [[ch, -sh, 0.0], [sh, ch, 0.0], [0.0, 0.0, 1.0]];
        let rx = [[1.0, 0.0, 0.0], [0.0, cp, -sp], [0.0, sp, cp]];
        let ry = [[cr, 0.0, sr], [0.0, 1.0, 0.0], [-sr, 0.0, cr]];
        Placement { t: at, r: matmul(matmul(rz, rx), ry), pivot }
    }

    pub fn dir_to_world(&self, d: V3) -> V3 {
        let r = &self.r;
        [
            r[0][0] * d[0] + r[0][1] * d[1] + r[0][2] * d[2],
            r[1][0] * d[0] + r[1][1] * d[1] + r[1][2] * d[2],
            r[2][0] * d[0] + r[2][1] * d[1] + r[2][2] * d[2],
        ]
    }

    pub fn dir_to_local(&self, d: V3) -> V3 {
        let r = &self.r;
        [
            r[0][0] * d[0] + r[1][0] * d[1] + r[2][0] * d[2],
            r[0][1] * d[0] + r[1][1] * d[1] + r[2][1] * d[2],
            r[0][2] * d[0] + r[1][2] * d[1] + r[2][2] * d[2],
        ]
    }

    pub fn to_world(&self, l: V3) -> V3 {
        add(self.t, self.dir_to_world(sub(l, self.pivot)))
    }

    pub fn to_local(&self, w: V3) -> V3 {
        add(self.pivot, self.dir_to_local(sub(w, self.t)))
    }

    /// The centre of a cell in sea space.
    pub fn cell_to_world(&self, p: Pos) -> V3 {
        self.to_world([p.x as f32 + 0.5, p.y as f32 + 0.5, p.z as f32 + 0.5])
    }
}

fn matmul(a: [[f32; 3]; 3], b: [[f32; 3]; 3]) -> [[f32; 3]; 3] {
    let mut m = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            m[i][j] = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
        }
    }
    m
}

/// Accumulated shot damage per cell of a grid (0 = sound, 1 = gives way).
/// Kept beside the grid by whoever owns it (a ship, a fort).
#[derive(Clone, Debug, Default)]
pub struct Damage {
    pub cells: HashMap<usize, f32>,
}

impl Damage {
    pub fn get(&self, i: usize) -> f32 {
        self.cells.get(&i).copied().unwrap_or(0.0)
    }
}

/// A grid a shot can hit, lent to [`Ballistics::step`] for one step.
pub struct Body<'a> {
    /// Identifies the body in events (an index the game chooses).
    pub id: usize,
    pub world: &'a mut World,
    pub things: &'a mut Things,
    pub place: Placement,
    pub damage: &'a mut Damage,
}

#[derive(Clone, Debug)]
pub struct Shot {
    pub pos: V3,
    pub vel: V3,
    pub mass: f32,
    pub diameter: f32,
    /// A shell: bursts on impact, or when `fuse` seconds have passed.
    pub burst: Option<Blast>,
    pub fuse: f32,
    /// The body it was fired from, which it cannot hit in its first metres.
    pub owner: Option<usize>,
    pub age: f32,
    pub travelled: f32,
    pub ricochets: u8,
    pub in_water: bool,
    pub alive: bool,
    /// Where it was at the start of the last step (for drawing a streak).
    pub last: V3,
}

impl Shot {
    /// A solid iron ball of `mass` kg.
    pub fn ball(mass: f32, pos: V3, vel: V3) -> Shot {
        let diameter = (6.0 * mass / (std::f32::consts::PI * IRON)).cbrt();
        Shot {
            pos,
            vel,
            mass,
            diameter,
            burst: None,
            fuse: f32::INFINITY,
            owner: None,
            age: 0.0,
            travelled: 0.0,
            ricochets: 0,
            in_water: false,
            alive: true,
            last: pos,
        }
    }

    pub fn from(mut self, owner: usize) -> Shot {
        self.owner = Some(owner);
        self
    }

    /// Make it a shell that bursts with `blast` on impact or after `fuse` s.
    pub fn shell(mut self, blast: Blast, fuse: f32) -> Shot {
        self.burst = Some(blast);
        self.fuse = fuse;
        self
    }

    pub fn area(&self) -> f32 {
        std::f32::consts::PI * self.diameter * self.diameter / 4.0
    }

    pub fn speed(&self) -> f32 {
        len(self.vel)
    }

    pub fn energy(&self) -> f32 {
        0.5 * self.mass * len(self.vel).powi(2)
    }
}

#[derive(Clone, Debug)]
pub enum ShotEvent {
    /// Went into the sea (and will not come out).
    Splash { at: V3, speed: f32 },
    /// Skipped off the water.
    Ricochet { at: V3, speed: f32 },
    /// Struck the ground outside any body.
    Ground { at: V3, speed: f32 },
    /// Struck a wall or floor cell of a body. `from` and `to` are the open
    /// cells either side of it along the flight (for a hole through a hull:
    /// outside and inside). `energy` is what the cell soaked up (J).
    Struck { body: usize, cell: Pos, mat: MatId, energy: f32, through: bool, broke: bool, from: Option<Pos>, to: Option<Pos>, area: f32 },
    /// Struck a creature or a heavy piece of furniture.
    HitThing { body: usize, thing: ThingId, cell: Pos, energy: f32, dir: V3 },
    /// A shell burst.
    Burst { body: Option<usize>, at: V3, report: BlastReport },
}

/// Drag coefficient of a sphere by Mach number.
fn sphere_cd(mach: f32) -> f32 {
    const T: [(f32, f32); 8] = [(0.0, 0.47), (0.6, 0.50), (0.8, 0.60), (0.95, 0.80), (1.05, 0.95), (1.2, 1.00), (1.5, 0.98), (2.0, 0.95)];
    if mach >= 2.0 {
        return 0.95;
    }
    for w in T.windows(2) {
        let ((m0, c0), (m1, c1)) = (w[0], w[1]);
        if mach <= m1 {
            return c0 + (c1 - c0) * ((mach - m0) / (m1 - m0)).clamp(0.0, 1.0);
        }
    }
    0.95
}

const SOUND: f32 = 340.0;
const SUB_DT: f32 = 1.0 / 200.0;
/// A ball slower than this in the water has spent itself.
const SPENT_IN_WATER: f32 = 15.0;

/// One drag-and-gravity sub-step: exact for quadratic drag along the line of
/// flight, then gravity (unless floating in water).
fn fly(s: &mut Shot, h: f32, wind: (f32, f32)) {
    let air = [s.vel[0] - wind.0, s.vel[1] - wind.1, s.vel[2]];
    let v = len(air);
    let (rho, cd) = if s.in_water { (RHO_SEA, 0.47) } else { (RHO_AIR, sphere_cd(v / SOUND)) };
    let k = 0.5 * rho * cd * s.area() / s.mass;
    if v > 0.0 {
        let f = 1.0 / (1.0 + k * v * h);
        // Slow the air-relative velocity; the wind's share is carried along.
        let rel = scale(air, f);
        s.vel = [rel[0] + wind.0, rel[1] + wind.1, rel[2]];
    }
    // In water, buoyancy takes off an eighth of gravity for iron.
    let g = if s.in_water { G * (1.0 - RHO_SEA / IRON) } else { G };
    s.vel[2] -= g * h;
}

#[derive(Clone, Debug)]
pub struct Ballistics {
    pub shots: Vec<Shot>,
    /// What happened; the game drains it.
    pub events: Vec<ShotEvent>,
    pub rng: Rng,
    /// Shots older than this (s) are dropped.
    pub max_age: f32,
}

impl Ballistics {
    pub fn new(seed: u64) -> Ballistics {
        Ballistics { shots: Vec::new(), events: Vec::new(), rng: Rng::new(seed ^ 0xBA11), max_age: 40.0 }
    }

    pub fn fire(&mut self, s: Shot) {
        self.shots.push(s);
    }

    /// Advance every shot `dt` seconds through the sea (if any), the wind
    /// (m/s) and the given bodies.
    pub fn step(&mut self, dt: f32, sea: Option<&Sea>, wind: (f32, f32), bodies: &mut [Body]) {
        // Bounding spheres of the bodies in sea space.
        let spheres: Vec<(V3, f32)> = bodies
            .iter()
            .map(|b| {
                let (w, h, d) = (b.world.w as f32, b.world.h as f32, b.world.d as f32);
                (b.place.to_world([w / 2.0, h / 2.0, d / 2.0]), 0.5 * (w * w + h * h + d * d).sqrt() + 1.0)
            })
            .collect();
        let sea_top = sea.map(|s| s.max_height() + 0.5).unwrap_or(f32::NEG_INFINITY);
        let mut shots = std::mem::take(&mut self.shots);
        for s in &mut shots {
            s.last = s.pos;
            let mut t = 0.0;
            while t < dt && s.alive {
                let h = SUB_DT.min(dt - t);
                t += h;
                s.age += h;
                fly(s, h, if s.in_water { (0.0, 0.0) } else { wind });
                let p0 = s.pos;
                let p1 = add(p0, scale(s.vel, h));
                let seg = len(sub(p1, p0));
                s.pos = p1;
                s.travelled += seg;

                // Bodies.
                for (bi, b) in bodies.iter_mut().enumerate() {
                    if !s.alive {
                        break;
                    }
                    if s.owner == Some(b.id) && s.travelled < 4.0 {
                        continue;
                    }
                    let (c, r) = spheres[bi];
                    if seg_sphere_dist(p0, p1, c) > r {
                        continue;
                    }
                    trace_body(s, b, p0, p1, &mut self.events, &mut self.rng);
                }
                if !s.alive {
                    break;
                }

                // The sea and the land.
                if let Some(sea) = sea {
                    if s.pos[2] < sea_top || s.in_water {
                        let (x, y) = (s.pos[0], s.pos[1]);
                        let ground = sea.ground(x, y);
                        if ground > sea.tide() && s.pos[2] < ground {
                            self.burst_or(s, None, ShotEvent::Ground { at: s.pos, speed: s.speed() });
                            break;
                        }
                        let surface = sea.height(x, y);
                        if !s.in_water && s.pos[2] < surface {
                            let horiz = (s.vel[0].powi(2) + s.vel[1].powi(2)).sqrt();
                            let graze = (-s.vel[2]).atan2(horiz);
                            if graze < RICOCHET_ANGLE && s.speed() > 60.0 && s.ricochets < 8 && s.burst.is_none() {
                                s.pos[2] = surface + 0.02;
                                s.vel[2] = -s.vel[2] * 0.5;
                                s.vel[0] *= 0.8;
                                s.vel[1] *= 0.8;
                                s.ricochets += 1;
                                self.events.push(ShotEvent::Ricochet { at: s.pos, speed: s.speed() });
                            } else {
                                s.in_water = true;
                                if s.burst.is_some() {
                                    // A shell's fuse is drowned.
                                    s.burst = None;
                                }
                                self.events.push(ShotEvent::Splash { at: s.pos, speed: s.speed() });
                            }
                        } else if s.in_water {
                            if s.pos[2] > surface {
                                s.in_water = false;
                            } else if s.speed() < SPENT_IN_WATER || s.pos[2] < -sea.bed(x, y) {
                                s.alive = false;
                            }
                        }
                    }
                } else if s.pos[2] < -200.0 {
                    s.alive = false;
                }

                if s.burst.is_some() && s.age >= s.fuse && s.alive {
                    let at = s.pos;
                    self.burst_or(s, None, ShotEvent::Burst { body: None, at, report: BlastReport::default() });
                }
                if s.age > self.max_age {
                    s.alive = false;
                }
            }
        }
        shots.retain(|s| s.alive);
        // Shots fired during this step (none, but keep any the caller added).
        shots.append(&mut self.shots);
        self.shots = shots;
    }

    /// End a shot: a shell bursts in the open (reported as a burst), solid
    /// shot reports `ev`.
    fn burst_or(&mut self, s: &mut Shot, body: Option<usize>, ev: ShotEvent) {
        s.alive = false;
        if s.burst.take().is_some() {
            self.events.push(ShotEvent::Burst { body, at: s.pos, report: BlastReport::default() });
        } else {
            self.events.push(ev);
        }
    }
}

/// Distance from point `c` to the segment p0-p1.
fn seg_sphere_dist(p0: V3, p1: V3, c: V3) -> f32 {
    let d = sub(p1, p0);
    let l2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    let t = if l2 > 0.0 { (((c[0] - p0[0]) * d[0] + (c[1] - p0[1]) * d[1] + (c[2] - p0[2]) * d[2]) / l2).clamp(0.0, 1.0) } else { 0.0 };
    len(sub(add(p0, scale(d, t)), c))
}

/// Trace one sub-step's flight p0 -> p1 (sea space) through a body's grid.
fn trace_body(s: &mut Shot, b: &mut Body, p0: V3, p1: V3, events: &mut Vec<ShotEvent>, rng: &mut Rng) {
    let a = b.place.to_local(p0);
    let e = b.place.to_local(p1);
    let d = sub(e, a);
    let seg = len(d);
    if seg <= 0.0 {
        return;
    }
    let (w, h, dep) = (b.world.w, b.world.h, b.world.d);
    // Clip to the grid's box.
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for (k, hi) in [w as f32, h as f32, dep as f32].into_iter().enumerate() {
        if d[k].abs() < 1e-9 {
            if a[k] < 0.0 || a[k] >= hi {
                return;
            }
        } else {
            let (mut ta, mut tb) = ((0.0 - a[k]) / d[k], (hi - a[k]) / d[k]);
            if ta > tb {
                std::mem::swap(&mut ta, &mut tb);
            }
            t0 = t0.max(ta);
            t1 = t1.min(tb);
        }
    }
    if t0 >= t1 {
        return;
    }
    let unit = scale(d, 1.0 / seg);
    let start = add(a, scale(d, t0 + 1e-5));
    let mut cell = [start[0].floor() as i32, start[1].floor() as i32, start[2].floor() as i32];
    let step = [d[0].signum() as i32, d[1].signum() as i32, d[2].signum() as i32];
    let mut tmax = [0.0f32; 3];
    let mut tdelta = [0.0f32; 3];
    for k in 0..3 {
        if d[k].abs() < 1e-9 {
            tmax[k] = f32::INFINITY;
            tdelta[k] = f32::INFINITY;
        } else {
            let edge = if step[k] > 0 { cell[k] as f32 + 1.0 } else { cell[k] as f32 };
            tmax[k] = (edge - a[k]) / d[k];
            tdelta[k] = (1.0 / d[k]).abs();
        }
    }
    let mut te = t0;
    let mut prev_open: Option<Pos> = None;
    let layer = b.world.layer();
    loop {
        let (cx, cy, cz) = (cell[0], cell[1], cell[2]);
        if !b.world.contains(cx, cy, cz) || te >= t1 || !s.alive {
            break;
        }
        let axis = if tmax[0] <= tmax[1] && tmax[0] <= tmax[2] { 0 } else if tmax[1] <= tmax[2] { 1 } else { 2 };
        let tx = tmax[axis].min(t1);
        let pos = Pos::new(cx, cy, cz);
        let i = b.world.idx(cx, cy, cz);
        let tile = b.world.tile[i];
        let path = (tx - te) * seg;
        let thick = match tile {
            Tile::Wall => Some(path.max(0.05)),
            Tile::Open => None,
            _ => {
                // The floor slab at the bottom of the cell.
                let za = a[2] + d[2] * te - cz as f32;
                let zb = a[2] + d[2] * tx - cz as f32;
                if za.min(zb) < FLOOR_THICK {
                    Some((FLOOR_THICK / unit[2].abs().max(0.12)).min(path.max(0.05)))
                } else {
                    None
                }
            }
        };
        if let Some(th) = thick {
            let mat = b.world.mat[i];
            let need = resistance(b.world.mats.get(mat)) * s.area() * th;
            let e_in = s.energy();
            let through = e_in > need;
            let absorbed = if through { need } else { e_in };
            let frac = if through { 1.0 } else { (e_in / need.max(1.0)).min(1.0) };
            let dmg = b.damage.cells.entry(i).or_insert(0.0);
            *dmg += HOLE_DAMAGE * (s.area() / HOLE_REF) * frac;
            let broke = *dmg >= 1.0;
            // The open cell beyond, along the flight.
            let beyond = add(a, scale(d, tx + 1e-4));
            let next = Pos::new(beyond[0].floor() as i32, beyond[1].floor() as i32, beyond[2].floor() as i32);
            let to = (b.world.contains(next.x, next.y, next.z) && !b.world.tile[b.world.idx(next.x, next.y, next.z)].is_wall()).then_some(next);
            if broke {
                b.damage.cells.remove(&i);
                let solid_below = cz > 0 && b.world.tile[i - layer].is_wall();
                if tile.is_wall() && solid_below {
                    b.world.set_tile(pos, Tile::Floor, mat);
                } else {
                    b.world.set_tile(pos, Tile::Open, AIR);
                }
            }
            events.push(ShotEvent::Struck { body: b.id, cell: pos, mat, energy: absorbed, through, broke, from: prev_open, to, area: s.area() });
            if let Some(blast) = s.burst.take() {
                let at = prev_open.unwrap_or(pos);
                let report = explode(b.world, b.things, at, &blast, rng);
                events.push(ShotEvent::Burst { body: Some(b.id), at: b.place.cell_to_world(at), report });
                s.alive = false;
                s.pos = b.place.cell_to_world(at);
                break;
            }
            if through {
                let v = (2.0 * (e_in - need) / s.mass).sqrt();
                // A little deflection on the way through.
                let jit = [rng.f32() - 0.5, rng.f32() - 0.5, rng.f32() - 0.5];
                let dir = add(scale(s.vel, 1.0 / s.speed().max(1e-3)), scale(jit, 0.06));
                s.vel = scale(dir, v / len(dir).max(1e-3));
            } else {
                s.alive = false;
                s.vel = [0.0; 3];
                s.pos = b.place.to_world(add(a, scale(d, te)));
                break;
            }
        } else {
            prev_open = Some(pos);
        }

        // Creatures and heavy furniture in the cell.
        if s.alive && !tile.is_wall() {
            let hits: Vec<(ThingId, bool, bool)> = b
                .things
                .at(pos)
                .map(|(id, t)| {
                    let k = b.things.kind_of(t);
                    (id, k.layer == Layer::Creature, k.blocks_move && k.layer == Layer::Furniture)
                })
                .filter(|&(_, c, f)| c || f)
                .collect();
            for (id, creature, _) in hits {
                // A body is ~0.3 m of flesh; a gun carriage or a chest ~0.4 m of oak.
                let need = if creature { 2.0e6 * 0.3 } else { 4.8e7 * 0.4 } * s.area();
                let e_in = s.energy();
                let energy = e_in.min(need);
                events.push(ShotEvent::HitThing { body: b.id, thing: id, cell: pos, energy, dir: unit });
                if e_in > need {
                    s.vel = scale(s.vel, ((e_in - need) / e_in).sqrt());
                } else {
                    s.alive = false;
                    s.vel = [0.0; 3];
                    s.pos = b.place.cell_to_world(pos);
                    break;
                }
            }
        }

        te = tx;
        cell[axis] += step[axis];
        tmax[axis] += tdelta[axis];
    }
}

/// The path a shot would take with nothing in the way, as points every
/// `every` seconds until it falls below `floor` (sea level, say) or `max_t`.
pub fn trajectory(mass: f32, from: V3, vel: V3, wind: (f32, f32), floor: f32, every: f32, max_t: f32) -> Vec<V3> {
    let mut s = Shot::ball(mass, from, vel);
    let mut out = vec![from];
    let (mut t, mut next) = (0.0, every);
    while t < max_t {
        fly(&mut s, SUB_DT, wind);
        s.pos = add(s.pos, scale(s.vel, SUB_DT));
        t += SUB_DT;
        if t >= next {
            out.push(s.pos);
            next += every;
        }
        if s.pos[2] < floor {
            out.push(s.pos);
            break;
        }
    }
    out
}

/// Horizontal distance a ball travels before it comes down to `drop` metres
/// below the muzzle, fired at `elev` radians and `speed` m/s in still air.
pub fn range_at(mass: f32, speed: f32, elev: f32, drop: f32) -> f32 {
    let (s, c) = elev.sin_cos();
    let pts = trajectory(mass, [0.0, 0.0, 0.0], [speed * c, 0.0, speed * s], (0.0, 0.0), -drop, 0.01, 60.0);
    pts.last().map(|p| p[0]).unwrap_or(0.0)
}

/// The elevation (radians, the lower of the two solutions) that carries a
/// ball `range` metres to a mark `drop` metres below the muzzle (negative:
/// above it), or `None` if it is out of reach.
pub fn elevation_for(mass: f32, speed: f32, range: f32, drop: f32) -> Option<f32> {
    let (mut lo, mut hi) = (-0.3f32, 0.75f32);
    if range_at(mass, speed, hi, drop) < range {
        return None;
    }
    if range_at(mass, speed, lo, drop) > range {
        return Some(lo);
    }
    for _ in 0..30 {
        let mid = 0.5 * (lo + hi);
        if range_at(mass, speed, mid, drop) < range {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Some(0.5 * (lo + hi))
}
