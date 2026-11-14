//! Flames carried by things: torches, candles, lanterns, braziers.
//!
//! Plain rules rather than chemistry (no oxygen model):
//! - liquid around a flame puts it out and soaks the thing;
//! - out under the open sky, rain and strong gusts put out exposed flames
//!   (rain only dampens a sheltered one; see [`Flame`]);
//! - a soaked thing dries over time, faster when it is warm, and is hard to
//!   light until it has (see [`try_light`]);
//! - fire or great heat in its cell relights a dry flame.
//!
//! Everything here runs on game seconds, not ticks (see the time contract in
//! [`crate::sim`]): `dt` is the clock's `secs_per_tick`, so a torch dries, and
//! a gale blows it out, in the same game time whatever tick a game uses.

use crate::rng::Rng;
use crate::things::{Flame, ThingId, Things};
use crate::world::World;

/// Game seconds a fully soaked thing takes to dry at ordinary temperatures
/// (ten minutes). `Thing::wet` counts down from this.
pub const SOAKED: f32 = 600.0;
/// Liquid (of 64) around a flame that puts it out.
pub const DOUSE_DEPTH: u8 = 16;
/// Wind speed (m/s) above which exposed flames start blowing out.
pub const GUST_LIMIT: f32 = 0.7;

/// The chance of something happening within one tick of `dt` seconds, when
/// it has chance `per_sec` within any one second. At `dt = 1` it is
/// `per_sec`; two half-second ticks give the same odds as one second.
pub fn chance_in(per_sec: f32, dt: f32) -> f32 {
    1.0 - (1.0 - per_sec.clamp(0.0, 1.0)).powf(dt)
}

/// Advance carried flames by one tick of `dt` game seconds.
pub fn step(world: &World, things: &mut Things, rng: &mut Rng, dt: f32) {
    // `world.wind` is in cells per tick (what the cellular automata use);
    // cells are 1 m, so dividing by the tick length gives metres per second.
    let speed = (world.wind.0 * world.wind.0 + world.wind.1 * world.wind.1).sqrt() / dt.max(1e-6);
    let flames: Vec<(ThingId, Flame)> = things
        .iter()
        .filter_map(|(id, t)| {
            let f = things.kind(t.kind).flame;
            (f != Flame::None).then_some((id, f))
        })
        .collect();
    for (id, flame) in flames {
        let Some(t) = things.get_mut(id) else { continue };
        let Some(i) = world.index(t.pos()) else { continue };
        if world.fluid[i] >= DOUSE_DEPTH {
            t.lit = false;
            t.wet = SOAKED;
            continue;
        }
        let outside = world.open_sky[i];
        if outside && world.rain > 0.0 {
            // Rain soaks it a little at a time: seconds of drying added per second.
            let soak = (world.rain * 6.0).floor() + 1.0;
            t.wet = (t.wet + soak * dt).min(SOAKED);
            if flame == Flame::Exposed && t.lit && rng.chance(chance_in(world.rain * 0.02, dt)) {
                t.lit = false;
            }
        } else if t.wet > 0.0 {
            // Warm things dry four times as fast.
            let warm = world.temp[i] > 40.0 || (t.lit && flame == Flame::Sheltered);
            let rate = if warm { 4.0 } else { 1.0 };
            t.wet = (t.wet - rate * dt).max(0.0);
        }
        if outside && flame == Flame::Exposed && t.lit && speed > GUST_LIMIT && rng.chance(chance_in((speed - GUST_LIMIT) * 0.04, dt)) {
            t.lit = false;
        }
        if !t.lit && t.wet == 0.0 && (world.fire[i] > 0 || world.temp[i] > 250.0) {
            t.lit = true;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lighting {
    Lit,
    AlreadyLit,
    /// Damp: it might take with a few more tries.
    Sputters,
    Underwater,
    NotAFlame,
}

/// Try to light a thing by hand. The wetter it is, the less likely: soaked
/// through, it hardly ever takes.
pub fn try_light(world: &World, things: &mut Things, id: ThingId, rng: &mut Rng) -> Lighting {
    let Some(t) = things.get(id) else { return Lighting::NotAFlame };
    if things.kind(t.kind).flame == Flame::None {
        return Lighting::NotAFlame;
    }
    if t.lit {
        return Lighting::AlreadyLit;
    }
    if world.index(t.pos()).is_some_and(|i| world.fluid[i] >= DOUSE_DEPTH) {
        return Lighting::Underwater;
    }
    let dry = 1.0 - t.wet / SOAKED;
    let ok = rng.chance(0.03 + 0.97 * dry * dry);
    let t = things.get_mut(id).unwrap();
    if ok {
        t.lit = true;
        Lighting::Lit
    } else {
        Lighting::Sputters
    }
}
