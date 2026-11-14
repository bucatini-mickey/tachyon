//! Weather: wind that gusts and veers, and rain.
//!
//! A game sets the prevailing conditions (`wind_speed`, `wind_dir`, `rain`)
//! and the weather turns them into the world's actual wind each tick, with
//! smooth random gusts and wander so it never feels like a constant vector.
//! The wind then acts through the rest of the engine: it drifts gas (smoke,
//! steam, fog) under open sky, fans fires downwind, blows out exposed flames,
//! pushes vessels, and slants the rain.
//!
//! Weather runs on game seconds (see the time contract in [`crate::sim`]):
//! the wind is given in metres per second and the gusts and veer have
//! periods in seconds, so a game with half-second ticks sees the same gusts
//! at the same game time as one with two-second ticks. Only at the end is the
//! wind converted to cells per tick for the cellular automata.

use crate::clock::Clock;
use crate::noise::value2;
use crate::world::World;

/// Gusts come and go over these periods (game seconds), the direction
/// wanders over the last.
pub const GUST_SECS: (f64, f64) = (7.0, 19.0);
pub const VEER_SECS: f64 = 110.0;

#[derive(Clone, Debug)]
pub struct Weather {
    /// Mean wind speed, metres per second.
    pub wind_speed: f32,
    /// Direction the wind blows *toward*, in radians (0 = east, PI/2 = south,
    /// since y grows southward).
    pub wind_dir: f32,
    /// How much the speed swings in gusts (0 = steady, 1 = from calm to double).
    pub gustiness: f32,
    /// How far the direction wanders either side of `wind_dir`, radians.
    pub veer: f32,
    /// Rain, 0 (dry) to 1 (a downpour).
    pub rain: f32,
    seed: u32,
}

impl Weather {
    pub fn new(seed: u32) -> Weather {
        Weather { wind_speed: 0.0, wind_dir: 0.0, gustiness: 0.5, veer: 0.35, rain: 0.0, seed }
    }

    /// Set the prevailing wind: `speed` in metres per second, `dir` the
    /// direction it blows toward (radians).
    pub fn set_wind(&mut self, speed: f32, dir: f32) {
        self.wind_speed = speed;
        self.wind_dir = dir;
    }

    /// The wind right now in metres per second (x, y), gusts and veer
    /// included. It depends on game seconds only, not on the tick length.
    pub fn wind_at(&self, secs: f64) -> (f32, f32) {
        // Noise coordinate in "periods"; wrapped so f32 keeps its precision
        // over a long game (a jump once every ~5 game days at the shortest).
        let at = |period: f64| ((secs / period) % 65536.0) as f32;
        let gust = 0.6 * value2(self.seed, at(GUST_SECS.0), 0.5) + 0.4 * value2(self.seed ^ 0x55, at(GUST_SECS.1), 3.5);
        let speed = self.wind_speed * (1.0 + self.gustiness * (gust * 2.0 - 1.0)).max(0.0);
        let dir = self.wind_dir + self.veer * (value2(self.seed ^ 0xAA, at(VEER_SECS), 7.5) * 2.0 - 1.0);
        (dir.cos() * speed, dir.sin() * speed)
    }

    /// Update the world's wind and rain for this tick. `world.wind` is in
    /// cells per tick: the wind in m/s times the clock's seconds per tick
    /// (cells are 1 m).
    pub fn apply(&self, world: &mut World, clock: &Clock) {
        // Seconds since the scene began (tick 0), not the time of day.
        let secs = clock.tick as f64 * clock.secs_per_tick;
        let (wx, wy) = self.wind_at(secs);
        let dt = clock.secs_per_tick as f32;
        world.wind = (wx * dt, wy * dt);
        world.rain = self.rain;
    }
}

/// Words for a wind as the grid feels it, in cells per tick (`World::wind`).
pub fn describe_wind(speed: f32) -> &'static str {
    match speed {
        s if s < 0.05 => "calm",
        s if s < 0.25 => "a light breeze",
        s if s < 0.6 => "a breeze",
        s if s < 1.0 => "a strong wind",
        _ => "a gale",
    }
}

/// Compass point the wind comes *from*, for a wind vector (x east, y south).
pub fn wind_from(wx: f32, wy: f32) -> &'static str {
    if wx.abs() + wy.abs() < 1e-4 {
        return "nowhere";
    }
    // The wind comes from the opposite direction to where it blows.
    // Angle of the source, with y flipped so north is up: the source lies at
    // (-wx, -wy) on screen, which is (-wx, wy) with y pointing up.
    let a = wy.atan2(-wx);
    let names = ["east", "north-east", "north", "north-west", "west", "south-west", "south", "south-east"];
    let k = ((a / std::f32::consts::FRAC_PI_4).round() as i32).rem_euclid(8) as usize;
    names[k]
}
