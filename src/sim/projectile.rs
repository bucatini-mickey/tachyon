//! Projectiles and explosions.
//!
//! A projectile is a thing (so it is drawn and can carry light) moved along a
//! straight line in fractional steps. It stops when it would enter a wall,
//! pass through a floor, or meet something solid or a creature; it explodes
//! there, or at the end of its range. In deep liquid it fizzles out.
//!
//! [`explode`] applies a blast to the grid: damage that falls off with
//! distance and is soaked up by walls in between (so stone shelters what is
//! behind it), heat, fire, smoke, and broken walls and floors.

use crate::geom::Pos;
use crate::rng::Rng;
use crate::things::{KindId, Layer, ThingId, Things};
use crate::tile::Tile;
use crate::world::World;

#[derive(Clone, Copy, Debug)]
pub struct Blast {
    /// Reach in cells.
    pub radius: f32,
    /// Force at the centre; walls break where the force reaching them is more
    /// than their material's `strength`.
    pub power: f32,
    /// Degrees added at the centre.
    pub heat: f32,
    /// Chance of setting flammable cells alight at the centre (falls off outward).
    pub ignite: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BlastReport {
    pub broken: u32,
    pub ignited: u32,
    pub things_destroyed: u32,
}

/// Total `strength` of the walls strictly between two cells, sampled along
/// the straight line joining them.
pub fn shielding(world: &World, a: Pos, b: Pos) -> f32 {
    let (dx, dy, dz) = ((b.x - a.x) as f32, (b.y - a.y) as f32, (b.z - a.z) as f32);
    let steps = ((dx * dx + dy * dy + dz * dz).sqrt() / 0.4).ceil() as i32;
    let (mut sum, mut last) = (0.0, a);
    for s in 1..steps {
        let t = s as f32 / steps as f32;
        let c = Pos::new(a.x + (dx * t).round() as i32, a.y + (dy * t).round() as i32, a.z + (dz * t).round() as i32);
        if c == a || c == b || c == last {
            continue;
        }
        last = c;
        if let Some(i) = world.index(c) {
            if world.tile[i].is_wall() {
                sum += world.mats.get(world.mat[i]).strength;
            }
        }
    }
    sum
}

/// Blow up at `at`. Creatures are hurt by `life::Life::blast` (the scene
/// calls it); other things in the blast are destroyed if it is strong enough.
pub fn explode(world: &mut World, things: &mut Things, at: Pos, b: &Blast, rng: &mut Rng) -> BlastReport {
    let mut report = BlastReport::default();
    let r = b.radius.ceil() as i32;
    let layer = world.layer();
    let mut broken: Vec<(usize, f32)> = Vec::new();
    let mut ignite: Vec<Pos> = Vec::new();
    for dz in -r..=r {
        for dy in -r..=r {
            for dx in -r..=r {
                let d = ((dx * dx + dy * dy + dz * dz) as f32).sqrt();
                if d > b.radius {
                    continue;
                }
                let p = at.offset(dx, dy, dz);
                let Some(i) = world.index(p) else { continue };
                let f = 1.0 - d / (b.radius + 1.0);
                let full = b.power * f;
                let force = full - shielding(world, at, p);
                if force <= 0.0 {
                    continue;
                }
                world.temp[i] += b.heat * f * (force / full);
                world.touch(i);
                let m = world.mats.get(world.mat[i]);
                let tile = world.tile[i];
                let hanging = tile.has_floor() && (p.z == 0 || !world.tile[i - layer].is_wall());
                if (tile.is_wall() && force > m.strength) || (hanging && force > 1.5 * m.strength) {
                    broken.push((i, d));
                } else if m.ignite.is_some() && tile != Tile::Open && rng.chance(b.ignite * f) {
                    ignite.push(p);
                }
            }
        }
    }
    // Break walls and floors only now, so shielding was judged on the
    // original shape. Ground left hanging over a broken wall caves in too.
    for &(i, _) in &broken {
        let p = world.pos(i);
        let mat = world.mat[i];
        let solid_below = p.z > 0 && world.tile[i - layer].is_wall();
        if world.tile[i].is_wall() && solid_below {
            world.set_tile(p, Tile::Floor, mat); // rubble
        } else {
            world.set_tile(p, Tile::Open, crate::material::AIR);
        }
        report.broken += 1;
        let up = p.up();
        if let Some(j) = world.index(up) {
            if world.tile[j] != Tile::Open && !world.tile[j].is_wall() {
                world.set_tile(up, Tile::Open, crate::material::AIR);
            }
        }
    }
    for p in ignite {
        if world.ignite(p) {
            report.ignited += 1;
        }
    }
    if let Some(smoke) = world.mats.find("smoke") {
        world.add_gas(at, smoke, 220);
        for (dx, dy) in crate::geom::DIRS4 {
            world.add_gas(at.offset(dx, dy, 0), smoke, 90);
        }
    }

    let hit: Vec<(ThingId, f32, bool)> = things
        .iter()
        .filter_map(|(id, t)| {
            let p = t.pos();
            let d = (((p.x - at.x).pow(2) + (p.y - at.y).pow(2) + (p.z - at.z).pow(2)) as f32).sqrt();
            let k = things.kind(t.kind);
            (d <= b.radius && k.layer < Layer::Creature).then(|| (id, b.power * (1.0 - d / (b.radius + 1.0)) - shielding(world, at, p), k.flame != crate::things::Flame::None))
        })
        .collect();
    for (id, force, flame) in hit {
        if force > 60.0 {
            things.despawn(id);
            report.things_destroyed += 1;
        } else if flame && force > 0.0 {
            if let Some(t) = things.get_mut(id) {
                if t.wet == 0 {
                    t.lit = true;
                }
            }
        }
    }
    report
}

/// What happens when a projectile tries to move from one cell into the next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pass {
    Clear,
    /// Stopped short: it bursts in the cell it is in.
    Blocked,
    /// It struck something in the next cell and bursts there.
    Strike,
    /// Into deep liquid.
    Doused,
    Gone,
}

/// Deep enough liquid to put out a flaming projectile.
pub const DOUSE: u8 = 32;

fn pass(world: &World, things: &Things, owner: Option<ThingId>, from: Pos, to: Pos) -> Pass {
    let Some(i) = world.index(to) else { return Pass::Gone };
    if world.tile[i].is_wall() {
        return Pass::Blocked;
    }
    if to.z < from.z && world.tile(Pos::new(from.x, from.y, from.z)).seals_below() {
        return Pass::Blocked;
    }
    if to.z > from.z && world.tile[i].seals_below() {
        return Pass::Blocked;
    }
    if world.fluid[i] >= DOUSE {
        return Pass::Doused;
    }
    let solid = things.at(to).any(|(id, t)| {
        let k = things.kind(t.kind);
        Some(id) != owner && (k.layer == Layer::Creature || (k.blocks_move && !(k.door && t.open)))
    });
    if solid {
        Pass::Strike
    } else {
        Pass::Clear
    }
}

#[derive(Clone, Debug)]
pub struct Projectile {
    /// The thing drawn in flight.
    pub thing: ThingId,
    pub owner: Option<ThingId>,
    pos: (f32, f32, f32),
    vel: (f32, f32, f32),
    range: f32,
    pub blast: Blast,
    /// Leave a thin trail of this gas (smoke behind a fire bolt).
    pub trail: Option<crate::material::MatId>,
}

/// The result of one tick of flight.
#[derive(Clone, Copy, Debug)]
pub enum Flight {
    Flying,
    Exploded(Pos, BlastReport),
    Fizzled(Pos),
    Gone,
}

fn round(p: (f32, f32, f32)) -> Pos {
    Pos::new(p.0.round() as i32, p.1.round() as i32, p.2.round() as i32)
}

impl Projectile {
    /// Launch `kind` from `from` toward `to` at `speed` cells per tick.
    pub fn launch(things: &mut Things, kind: KindId, from: Pos, to: Pos, speed: f32, blast: Blast, owner: Option<ThingId>) -> Projectile {
        let (dx, dy, dz) = ((to.x - from.x) as f32, (to.y - from.y) as f32, (to.z - from.z) as f32);
        let len = (dx * dx + dy * dy + dz * dz).sqrt().max(0.01);
        Projectile {
            thing: things.spawn(kind, from),
            owner,
            pos: (from.x as f32, from.y as f32, from.z as f32),
            vel: (dx / len * speed, dy / len * speed, dz / len * speed),
            range: len,
            blast,
            trail: None,
        }
    }

    /// Advance one tick. Anything but `Flying` means it is finished (and its
    /// thing has been removed).
    pub fn step(&mut self, world: &mut World, things: &mut Things, rng: &mut Rng) -> Flight {
        let speed = (self.vel.0 * self.vel.0 + self.vel.1 * self.vel.1 + self.vel.2 * self.vel.2).sqrt();
        let n = (speed / 0.34).ceil().max(1.0) as i32;
        for _ in 0..n {
            let cur = round(self.pos);
            let next = (self.pos.0 + self.vel.0 / n as f32, self.pos.1 + self.vel.1 / n as f32, self.pos.2 + self.vel.2 / n as f32);
            let c = round(next);
            if c != cur {
                match pass(world, things, self.owner, cur, c) {
                    Pass::Clear => {}
                    Pass::Blocked => return self.burst(world, things, cur, rng),
                    Pass::Strike => return self.burst(world, things, c, rng),
                    Pass::Doused => {
                        things.despawn(self.thing);
                        if let Some(steam) = world.mats.find("steam") {
                            world.add_gas(c.up(), steam, 120);
                        }
                        return Flight::Fizzled(c);
                    }
                    Pass::Gone => {
                        things.despawn(self.thing);
                        return Flight::Gone;
                    }
                }
                things.move_to(self.thing, c);
                if let Some(g) = self.trail {
                    world.add_gas(cur, g, 10);
                }
            }
            self.pos = next;
            self.range -= speed / n as f32;
            if self.range <= 0.0 {
                return self.burst(world, things, c, rng);
            }
        }
        Flight::Flying
    }

    fn burst(&mut self, world: &mut World, things: &mut Things, at: Pos, rng: &mut Rng) -> Flight {
        things.despawn(self.thing);
        Flight::Exploded(at, explode(world, things, at, &self.blast, rng))
    }
}

/// The cells a projectile from `from` to `to` would pass through, ending
/// where it would burst (for drawing an aiming line).
pub fn trace(world: &World, things: &Things, owner: Option<ThingId>, from: Pos, to: Pos) -> Vec<Pos> {
    let (dx, dy, dz) = ((to.x - from.x) as f32, (to.y - from.y) as f32, (to.z - from.z) as f32);
    let len = (dx * dx + dy * dy + dz * dz).sqrt();
    let n = (len / 0.34).ceil().max(1.0) as i32;
    let mut out = Vec::new();
    let mut cur = from;
    for s in 1..=n {
        let t = s as f32 / n as f32;
        let c = Pos::new(from.x + (dx * t).round() as i32, from.y + (dy * t).round() as i32, from.z + (dz * t).round() as i32);
        if c == cur {
            continue;
        }
        match pass(world, things, owner, cur, c) {
            Pass::Clear => {}
            Pass::Strike | Pass::Doused => {
                out.push(c);
                return out;
            }
            Pass::Blocked | Pass::Gone => return out,
        }
        out.push(c);
        cur = c;
    }
    out
}
