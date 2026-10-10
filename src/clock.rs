//! Simulation time and the day/night cycle.

/// Counts ticks and turns them into a time of day.
#[derive(Clone, Debug)]
pub struct Clock {
    pub tick: u64,
    /// Game seconds that pass per tick.
    pub secs_per_tick: f64,
    /// Game seconds at tick 0, measured from midnight of day 1.
    pub start_secs: f64,
}

/// Sunlight right now, as a per-channel multiplier for sky-lit cells.
#[derive(Clone, Copy, Debug)]
pub struct Daylight {
    pub light: [f32; 3],
    /// 0 at night, 1 at noon.
    pub sun: f32,
}

impl Clock {
    pub fn new(secs_per_tick: f64, start_hour: f64) -> Clock {
        Clock { tick: 0, secs_per_tick, start_secs: start_hour * 3600.0 }
    }

    pub fn advance(&mut self) {
        self.tick += 1;
    }

    /// Jump the clock forward without simulating (for testing light).
    pub fn skip(&mut self, secs: f64) {
        self.start_secs += secs;
    }

    pub fn seconds(&self) -> f64 {
        self.start_secs + self.tick as f64 * self.secs_per_tick
    }

    pub fn day(&self) -> u64 {
        (self.seconds() / 86400.0) as u64 + 1
    }

    pub fn hour(&self) -> f64 {
        (self.seconds() % 86400.0) / 3600.0
    }

    /// "Day 3, 14:05"
    pub fn label(&self) -> String {
        let h = self.hour();
        format!("Day {}, {:02}:{:02}", self.day(), h as u32, ((h.fract()) * 60.0) as u32)
    }

    pub fn daylight(&self) -> Daylight {
        let h = self.hour() as f32;
        // Sun elevation: -1 at midnight, +1 at noon.
        let elev = ((h - 6.0) / 12.0 * std::f32::consts::PI).sin();
        let sun = smoothstep(-0.12, 0.35, elev);
        let warm = smoothstep(0.45, 0.0, elev.abs()) * sun; // dawn and dusk
        let day = [1.0, 0.97, 0.92];
        let dusk = [1.0, 0.62, 0.40];
        let night = [0.10, 0.12, 0.20];
        let mut light = [0.0; 3];
        for c in 0..3 {
            let lit = day[c] + (dusk[c] - day[c]) * warm;
            light[c] = night[c] + (lit - night[c]) * sun;
        }
        Daylight { light, sun }
    }

    /// Outdoor air temperature: about 5 C before dawn, 19 C mid-afternoon.
    pub fn ambient_temp(&self) -> f32 {
        let h = self.hour() as f32;
        12.0 + 7.0 * ((h - 9.0) / 24.0 * std::f32::consts::TAU).sin()
    }
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
