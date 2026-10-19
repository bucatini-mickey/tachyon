//! Physics over the cell grid. Each system is a cellular automaton that only
//! visits awake chunks, so cost scales with how much is happening rather than
//! with the size of the world.
//!
//! Order within a tick: liquids fall and spread, gases rise and spread, then
//! heat conducts and drives phase changes and fire. Sky exposure is
//! recomputed when the terrain has changed.

pub mod flames;
pub mod fluid;
pub mod gas;
pub mod heat;
pub mod light;
pub mod projectile;
pub mod weather;

use crate::geom::Pos;
use crate::material::MatId;
use crate::rng::Rng;
use crate::world::World;
use std::collections::VecDeque;
use std::time::Instant;

#[derive(Clone, Copy, Debug, Default)]
pub struct SimStats {
    pub awake_chunks: usize,
    pub total_chunks: usize,
    /// Wall-clock time of the last tick, microseconds.
    pub tick_us: u64,
    pub fluid_us: u64,
    pub gas_us: u64,
    pub heat_us: u64,
    pub sky_us: u64,
}

/// Raindrops per column per tick in a downpour (`World::rain` = 1).
pub const RAIN_DROPS: f32 = 0.001;

/// Drop rain on random columns: each drop adds a unit of water to the first
/// thing it meets on the way down (a floor, a puddle, the top of a wall).
fn rain(world: &mut World, water: MatId, rng: &mut Rng) {
    let n = world.rain * RAIN_DROPS * (world.w * world.h) as f32;
    let drops = n as u32 + rng.chance(n.fract()) as u32;
    for _ in 0..drops {
        let (x, y) = (rng.range(0, world.w), rng.range(0, world.h));
        for z in (0..world.d).rev() {
            let i = world.idx(x, y, z);
            let t = world.tile[i];
            if t == crate::tile::Tile::Open && world.fluid[i] == 0 && z > 0 {
                continue;
            }
            let land = if t.is_wall() { z + 1 } else { z };
            if land < world.d {
                world.add_fluid(Pos::new(x, y, land), water, 1);
            }
            break;
        }
    }
}

/// A spring or a drain: liquid appearing or vanishing at a cell every tick.
/// Rivers, the sea at the map's edge, a leaking hull, a bath's plughole.
#[derive(Clone, Copy, Debug)]
pub struct Source {
    pub pos: Pos,
    /// The liquid a spring gives (ignored by a drain).
    pub mat: MatId,
    /// Units per tick: positive is a spring that keeps `pos` topped up (it
    /// never overflows, so it holds a lake at its level); negative drains
    /// out of `pos`, then out of the cells below it.
    pub rate: i16,
}

/// Physics state that lives outside the grid: randomness and scratch
/// buffers, kept between ticks so nothing is allocated per tick.
pub struct Sim {
    pub rng: Rng,
    pub stats: SimStats,
    chunks: Vec<usize>,
    delta: Vec<i16>,
    touched: Vec<u32>,
    heat_scratch: Vec<[f32; 3]>,
    edge: Vec<(u32, f32)>,
    queue: VecDeque<u32>,
    /// Scratch and limits for liquid pressure (see `fluid::pressure`).
    pub pressure: fluid::Pressure,
    pub sources: Vec<Source>,
    /// Liquids on/off (for testing other systems in isolation).
    pub fluids: bool,
    pub gases: bool,
    pub heat: bool,
}

impl Sim {
    pub fn new(seed: u64) -> Sim {
        Sim {
            rng: Rng::new(seed),
            stats: SimStats::default(),
            chunks: Vec::new(),
            delta: Vec::new(),
            touched: Vec::new(),
            heat_scratch: Vec::new(),
            edge: Vec::new(),
            queue: VecDeque::new(),
            pressure: fluid::Pressure::new(),
            sources: Vec::new(),
            fluids: true,
            gases: true,
            heat: true,
        }
    }

    pub fn step(&mut self, world: &mut World, tick: u64) {
        let t0 = Instant::now();
        for s in &self.sources {
            let mut left = s.rate.unsigned_abs().min(255) as u8;
            if s.rate > 0 {
                world.add_fluid(s.pos, s.mat, left);
                continue;
            }
            for k in 0..3 {
                if left == 0 {
                    break;
                }
                left -= world.take_fluid(s.pos.offset(0, 0, -k), left);
            }
        }
        if world.rain > 0.0 {
            if let Some(water) = world.mats.find("water") {
                rain(world, water, &mut self.rng);
            }
        }
        world.wake.collect(&mut self.chunks);
        self.stats.awake_chunks = self.chunks.len();
        self.stats.total_chunks = world.wake.total();
        if self.delta.len() != world.len() {
            self.delta = vec![0; world.len()];
            self.heat_scratch = vec![[0.0; 3]; world.len()];
        }

        let t = Instant::now();
        if self.fluids {
            fluid::step(world, &self.chunks, &mut self.rng, &mut self.delta, &mut self.touched, tick);
            fluid::pressure(world, &self.chunks, &mut self.pressure, tick);
        }
        self.stats.fluid_us = t.elapsed().as_micros() as u64;

        let t = Instant::now();
        if self.gases {
            gas::step(world, &self.chunks, &mut self.rng, &mut self.delta, &mut self.touched);
        }
        self.stats.gas_us = t.elapsed().as_micros() as u64;

        let t = Instant::now();
        if self.heat {
            heat::step(world, &self.chunks, &mut self.rng, &mut self.heat_scratch, &mut self.edge);
        }
        self.stats.heat_us = t.elapsed().as_micros() as u64;

        world.wake.finish();

        let t = Instant::now();
        // Throttled: a big fire can change terrain every tick.
        if world.sky_dirty && tick % 4 == 0 {
            light::compute_sky(world, &mut self.queue);
        }
        self.stats.sky_us = t.elapsed().as_micros() as u64;
        self.stats.tick_us = t0.elapsed().as_micros() as u64;
    }
}
