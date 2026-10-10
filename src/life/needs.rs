//! Needs: hunger, thirst, sleep, body heat and breath. They rise with game
//! time (not ticks), so skipping an hour makes everyone an hour hungrier.

use super::body::Cause;

/// How fast a species' needs grow, and what it can stand.
#[derive(Clone, Copy, Debug)]
pub struct NeedRates {
    /// Hunger gained per day awake (0 fed, 1 starving).
    pub hunger: f32,
    pub thirst: f32,
    /// Tiredness gained per day awake; sleeping takes off three times this.
    pub fatigue: f32,
    /// Air temperatures it is comfortable in (clothes or fur included).
    pub comfort: (f32, f32),
    /// Seconds it can hold its breath.
    pub breath: f32,
}

impl Default for NeedRates {
    fn default() -> Self {
        NeedRates { hunger: 1.0, thirst: 1.3, fatigue: 1.1, comfort: (10.0, 30.0), breath: 60.0 }
    }
}

/// What a creature's surroundings are doing to it this step.
#[derive(Clone, Copy, Debug, Default)]
pub struct Surroundings {
    /// Temperature of the cell, C.
    pub air: f32,
    /// Head under the surface.
    pub underwater: bool,
    /// Standing or swimming in liquid.
    pub in_water: bool,
    /// Rain falling on it, 0..1.
    pub rain: f32,
    pub asleep: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Needs {
    pub hunger: f32,
    pub thirst: f32,
    pub fatigue: f32,
    /// Core temperature, C.
    pub temp: f32,
    /// 0 dry .. 1 soaked.
    pub wet: f32,
    /// Breath left, 1 full .. 0 none.
    pub breath: f32,
    /// Seconds spent starving, parched, or without air.
    pub starving: f32,
    pub parched: f32,
    pub airless: f32,
}

impl Default for Needs {
    fn default() -> Self {
        Needs { hunger: 0.1, thirst: 0.1, fatigue: 0.2, temp: 37.0, wet: 0.0, breath: 1.0, starving: 0.0, parched: 0.0, airless: 0.0 }
    }
}

const DAY: f32 = 86400.0;

impl Needs {
    pub fn step(&mut self, r: &NeedRates, s: &Surroundings, dt: f32) {
        let awake = if s.asleep { 0.6 } else { 1.0 };
        self.hunger = (self.hunger + r.hunger * awake * dt / DAY).min(1.0);
        self.thirst = (self.thirst + r.thirst * awake * dt / DAY).min(1.0);
        self.fatigue = if s.asleep { self.fatigue - 3.0 * r.fatigue * dt / DAY } else { self.fatigue + r.fatigue * dt / DAY }.clamp(0.0, 1.0);
        self.starving = if self.hunger >= 1.0 { self.starving + dt } else { 0.0 };
        self.parched = if self.thirst >= 1.0 { self.parched + dt } else { 0.0 };

        // Wet: soaked in water, damp in rain, drying slowly.
        if s.in_water {
            self.wet = 1.0;
        } else {
            self.wet = (self.wet + s.rain * dt / 600.0 - dt / 7200.0).clamp(0.0, 1.0);
        }

        // Body heat drifts when the air is outside the comfortable range
        // (faster when wet), and recovers inside it.
        let feels = s.air - 8.0 * self.wet;
        let (lo, hi) = r.comfort;
        if feels < lo {
            self.temp -= (lo - feels) * dt / (3600.0 * 8.0);
        } else if feels > hi {
            self.temp += (feels - hi) * dt / (3600.0 * 6.0);
        } else {
            let back = 2.0 * dt / 3600.0;
            self.temp += (37.0 - self.temp).clamp(-back, back);
        }

        if s.underwater {
            self.breath = (self.breath - dt / r.breath.max(1.0)).max(0.0);
        } else {
            self.breath = (self.breath + dt / 10.0).min(1.0);
        }
        self.airless = if self.breath <= 0.0 { self.airless + dt } else { 0.0 };
    }

    /// How these needs weigh on consciousness, 0..1.
    pub fn factor(&self) -> f32 {
        let mut f = 1.0;
        if self.hunger >= 1.0 {
            f *= 0.75;
        }
        if self.thirst >= 1.0 {
            f *= 0.6;
        }
        if self.fatigue > 0.9 {
            f *= 0.7;
        }
        if self.temp < 33.0 {
            f *= ((self.temp - 28.0) / 5.0).clamp(0.0, 1.0);
        }
        if self.temp > 40.0 {
            f *= ((43.0 - self.temp) / 3.0).clamp(0.0, 1.0);
        }
        if self.breath <= 0.0 {
            f *= 0.1;
        }
        f
    }

    pub fn fatal(&self, underwater: bool) -> Option<Cause> {
        if self.airless > 90.0 {
            Some(if underwater { Cause::Drowned } else { Cause::Suffocated })
        } else if self.temp < 26.0 {
            Some(Cause::Froze)
        } else if self.temp > 43.5 {
            Some(Cause::Overheated)
        } else if self.parched > 3.0 * DAY {
            Some(Cause::Thirst)
        } else if self.starving > 30.0 * DAY {
            Some(Cause::Starved)
        } else {
            None
        }
    }

    /// Short words for how it feels: "hungry", "tired", "cold".
    pub fn words(&self) -> Vec<String> {
        let mut v = Vec::new();
        let grade = |x: f32, a: &str, b: &str, c: &str| -> Option<String> {
            if x >= 0.9 {
                Some(c.into())
            } else if x >= 0.6 {
                Some(b.into())
            } else if x >= 0.35 {
                Some(a.into())
            } else {
                None
            }
        };
        v.extend(grade(self.hunger, "peckish", "hungry", "starving"));
        v.extend(grade(self.thirst, "thirsty", "parched", "dying of thirst"));
        v.extend(grade(self.fatigue, "tired", "very tired", "exhausted"));
        if self.temp < 35.0 {
            v.push(format!("hypothermic ({:.1} C)", self.temp));
        } else if self.temp < 36.3 {
            v.push("cold".into());
        } else if self.temp > 39.5 {
            v.push(format!("overheating ({:.1} C)", self.temp));
        }
        if self.wet > 0.6 {
            v.push("soaked".into());
        } else if self.wet > 0.2 {
            v.push("damp".into());
        }
        if self.breath < 1.0 {
            v.push(if self.breath <= 0.0 { "DROWNING".into() } else { format!("holding breath ({:.0}%)", self.breath * 100.0) });
        }
        v
    }
}
