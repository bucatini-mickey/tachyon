//! Physics over the cell grid. Each system is a cellular automaton that only
//! visits awake chunks, so cost scales with how much is happening rather than
//! with the size of the world.
//!
//! Order within a tick: liquids fall and spread, gases rise and spread, then
//! heat conducts and drives phase changes and fire. Sky exposure is
//! recomputed when the terrain has changed.
//!
//! # The time contract
//!
//! One physics tick is one tick of the scene's [`Clock`](crate::clock::Clock),
//! lasting `secs_per_tick` game seconds. Each game keeps its own tick (decided
//! 11 Nov: the sandbox uses 2 s, Broadside 0.5 s, tests 1 s). Rates come in
//! two kinds, and every rate is one or the other:
//!
//! **Per tick (cellular automata).** These are steps of a grid rule, so they
//! cannot follow the clock: a rule moves at most about one cell per tick. In
//! game time they run faster on a short tick and slower on a long one. At the
//! reference tick of 1 s (the tests) "per tick" also means "per second".
//! - Liquid: falls one level and spreads per tick (`fluid::step`); pressure
//!   equalises per tick (`fluid::pressure`); a [`Source`] gives `rate` units
//!   per tick; rain drops [`RAIN_DROPS`] per column per tick.
//! - Heat: conduction between neighbours per tick (`heat::step`), the drift to
//!   the outdoor and deep-earth temperature, phase changes, and
//!   `heat::FIRE_HEAT` degrees a burning cell gains per tick.
//! - Gas: `rise` and `decay` of each gas material are fractions per tick (decay
//!   four times faster under open sky); sideways diffusion per tick.
//! - Fire in cells: `Material::burn_ticks` (how long a cell burns), `spread`
//!   (chance per tick to light a neighbour), rain putting out an exposed fire
//!   (`rain * 0.06` per tick), wind fanning it (from `World::wind`, cells per
//!   tick).
//! - Things in flight: `Projectile` speed in cells per tick; `Scene::effects`
//!   last a number of ticks; vessels move in cells per tick with per-tick
//!   `windage` and `drag`. Sky exposure is recomputed at most every 4 ticks.
//!
//! **Game seconds (follow the clock).** These are not grid rules, so a game
//! gets the same result in the same game time whatever its tick:
//! - Carried flames (`flames`): drying ([`flames::SOAKED`] seconds when
//!   soaked, four times faster when warm), rain soaking them, rain and gusts
//!   blowing them out (chances per second, turned into per-tick chances with
//!   [`flames::chance_in`]), and [`flames::GUST_LIMIT`] in m/s.
//! - Weather (`weather`): the wind is given in m/s; gusts and veer have periods
//!   of [`weather::GUST_SECS`] and [`weather::VEER_SECS`] game seconds. The wind
//!   reaches the grid as `World::wind` in cells per tick (m/s times
//!   `secs_per_tick`, cells being 1 m), which is the unit the per-tick rules
//!   above read. Gas drifts at most half a cell per tick, so on any tick a
//!   strong wind saturates the drift.
//! - Life (`life`): needs, wounds, bleeding, stains and healing run on game
//!   seconds and the day length (`Clock::day_secs`, `Span`). Walking is still
//!   ticks per step until speeds move to m/s.
//!
//! [`Scene::fast_forward`](crate::scene::Scene::fast_forward) runs whole ticks
//! without drawing, so both kinds behave exactly as at normal speed.

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
