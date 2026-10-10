//! Flames carried by things: torches, candles, lanterns, braziers.
//!
//! Plain rules rather than chemistry (no oxygen model):
//! - liquid around a flame puts it out and soaks the thing;
//! - out under the open sky, rain and strong gusts put out exposed flames
//!   (rain only dampens a sheltered one; see [`Flame`]);
//! - a soaked thing dries over time, faster when it is warm, and is hard to
//!   light until it has (see [`try_light`]);
//! - fire or great heat in its cell relights a dry flame.

use crate::rng::Rng;
use crate::things::{Flame, ThingId, Things};
use crate::world::World;

/// Ticks a fully soaked thing takes to dry at ordinary temperatures.
pub const SOAKED: u16 = 600;
/// Liquid (of 64) around a flame that puts it out.
pub const DOUSE_DEPTH: u8 = 16;
/// Wind speed (cells per tick) above which exposed flames start blowing out.
pub const GUST_LIMIT: f32 = 0.7;

pub fn step(world: &World, things: &mut Things, rng: &mut Rng) {
    let speed = (world.wind.0 * world.wind.0 + world.wind.1 * world.wind.1).sqrt();
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
            // Rain soaks it a little at a time.
            let soak = (world.rain * 6.0) as u16 + 1;
            t.wet = (t.wet + soak).min(SOAKED);
            if flame == Flame::Exposed && t.lit && rng.chance(world.rain * 0.02) {
                t.lit = false;
            }
        } else if t.wet > 0 {
            let warm = world.temp[i] > 40.0 || (t.lit && flame == Flame::Sheltered);
            t.wet = t.wet.saturating_sub(if warm { 4 } else { 1 });
        }
        if outside && flame == Flame::Exposed && t.lit && speed > GUST_LIMIT && rng.chance((speed - GUST_LIMIT) * 0.04) {
            t.lit = false;
        }
        if !t.lit && t.wet == 0 && (world.fire[i] > 0 || world.temp[i] > 250.0) {
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
    let dry = 1.0 - t.wet as f32 / SOAKED as f32;
    let ok = rng.chance(0.03 + 0.97 * dry * dry);
    let t = things.get_mut(id).unwrap();
    if ok {
        t.lit = true;
        Lighting::Lit
    } else {
        Lighting::Sputters
    }
}
