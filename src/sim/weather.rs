//! Weather: wind that gusts and veers, and rain.
//!
//! A game sets the prevailing conditions (`wind_speed`, `wind_dir`, `rain`)
//! and the weather turns them into the world's actual wind each tick, with
//! smooth random gusts and wander so it never feels like a constant vector.
//! The wind then acts through the rest of the engine: it drifts gas (smoke,
//! steam, fog) under open sky, fans fires downwind, blows out exposed flames,
//! pushes vessels, and slants the rain.

use crate::noise::value2;
use crate::world::World;

#[derive(Clone, Debug)]
pub struct Weather {
    /// Mean wind speed, cells per tick.
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

    /// Calm, with the given gusts. Set the wind with [`Weather::set_wind`].
    pub fn set_wind(&mut self, speed: f32, dir: f32) {
        self.wind_speed = speed;
        self.wind_dir = dir;
    }

    /// Update the world's wind and rain for this tick.
    pub fn apply(&self, world: &mut World, tick: u64) {
        let t = tick as f32;
        // Gusts come and go over ~5-20 ticks; the direction drifts over ~100.
        let gust = 0.6 * value2(self.seed, t / 7.0, 0.5) + 0.4 * value2(self.seed ^ 0x55, t / 19.0, 3.5);
        let speed = self.wind_speed * (1.0 + self.gustiness * (gust * 2.0 - 1.0)).max(0.0);
        let dir = self.wind_dir + self.veer * (value2(self.seed ^ 0xAA, t / 110.0, 7.5) * 2.0 - 1.0);
        world.wind = (dir.cos() * speed, dir.sin() * speed);
        world.rain = self.rain;
    }
}

/// Words for a wind speed (cells per tick).
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
