//! Simulation time and the day/night cycle.

/// Counts ticks and turns them into a time of day.
#[derive(Clone, Debug)]
pub struct Clock {
    pub tick: u64,
    /// Game seconds that pass per tick.
    pub secs_per_tick: f64,
    /// Game seconds at tick 0, measured from midnight of day 1.
    pub start_secs: f64,
    /// Game seconds in one day (default 86400). Gameplay durations are `Span`s
    /// (fractions of a day), so a game with a shorter day gets faster hunger,
    /// healing and sunsets while physics keeps its per-tick rates.
    pub day_secs: f64,
}

/// A length of game time, as a fraction of a day.
///
/// This is the "newtype" idiom: a tuple struct wrapping one `f64`. In Python
/// you would write a float and hope nobody adds hours to days; here `Span` is
/// a different type from `f64`, so the compiler rejects `span + 3.0` and you
/// must say `Span::hours(3.0)`. It costs nothing at run time (it is the same
/// eight bytes as the float). `Copy` means it is copied like a number, and
/// `PartialOrd` gives you `<` and `>`.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Default)]
pub struct Span(f64);

impl Span {
    pub fn days(d: f64) -> Span {
        Span(d)
    }

    pub fn hours(h: f64) -> Span {
        Span(h / 24.0)
    }

    pub fn minutes(m: f64) -> Span {
        Span(m / 1440.0)
    }

    /// The length as a fraction of a day.
    pub fn as_days(self) -> f64 {
        self.0
    }

    pub fn as_hours(self) -> f64 {
        self.0 * 24.0
    }

    pub fn as_minutes(self) -> f64 {
        self.0 * 1440.0
    }
}

// Operator overloading is done by implementing traits; `a + b` calls `Add::add`.
impl std::ops::Add for Span {
    type Output = Span;
    fn add(self, o: Span) -> Span {
        Span(self.0 + o.0)
    }
}

impl std::ops::Mul<f64> for Span {
    type Output = Span;
    fn mul(self, k: f64) -> Span {
        Span(self.0 * k)
    }
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
        Clock { tick: 0, secs_per_tick, start_secs: start_hour * 3600.0, day_secs: 86400.0 }
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
        (self.seconds() / self.day_secs) as u64 + 1
    }

    /// How far through today, 0 (midnight) up to 1.
    pub fn day_fraction(&self) -> f64 {
        (self.seconds() % self.day_secs) / self.day_secs
    }

    /// Hours through today on a 24-slice day, whatever the day length.
    pub fn hour(&self) -> f64 {
        self.day_fraction() * 24.0
    }

    /// Whole ticks a span takes (rounded, never less than 1).
    pub fn ticks_in(&self, span: Span) -> u64 {
        ((span.0 * self.day_secs / self.secs_per_tick).round() as u64).max(1)
    }

    /// A number of game seconds as a span of this clock's day.
    pub fn span_of_secs(&self, secs: f64) -> Span {
        Span(secs / self.day_secs)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_units_agree() {
        assert_eq!(Span::hours(24.0), Span::days(1.0));
        assert_eq!(Span::minutes(1440.0), Span::days(1.0));
        assert!((Span::hours(6.0).as_days() - 0.25).abs() < 1e-12);
        assert!((Span::days(0.5).as_hours() - 12.0).abs() < 1e-12);
        assert!((Span::hours(1.5).as_minutes() - 90.0).abs() < 1e-9);
        assert_eq!(Span::hours(1.0) + Span::minutes(60.0), Span::hours(2.0));
        assert_eq!(Span::hours(2.0) * 3.0, Span::hours(6.0));
        assert!(Span::minutes(5.0) < Span::hours(1.0));
    }

    #[test]
    fn ticks_and_seconds_round_trip() {
        let c = Clock::new(2.0, 0.0);
        assert_eq!(c.ticks_in(Span::hours(1.0)), 1800);
        assert_eq!(c.ticks_in(Span::minutes(0.001)), 1); // never zero
        assert!((c.span_of_secs(3600.0).as_hours() - 1.0).abs() < 1e-9);
        let mut short = c.clone();
        short.day_secs = 43200.0;
        assert_eq!(short.ticks_in(Span::hours(1.0)), 900);
    }

    #[test]
    fn a_shorter_day_scales_the_clock_and_the_sun() {
        let mut c = Clock::new(1.0, 0.0);
        c.day_secs = 43200.0;
        c.skip(10800.0); // a quarter of the short day = 06:00
        assert!((c.hour() - 6.0).abs() < 1e-9);
        assert_eq!(c.day(), 1);
        c.skip(43200.0);
        assert_eq!(c.day(), 2);
        // Tick count to reach dusk (18:00) from midnight, standard vs half day.
        let dusk = |day_secs: f64| {
            let mut c = Clock::new(1.0, 0.0);
            c.day_secs = day_secs;
            let mut n = 0;
            while c.hour() < 18.0 {
                c.advance();
                n += 1;
            }
            n
        };
        assert_eq!(dusk(86400.0), 2 * dusk(43200.0));
        assert!(c.daylight().sun >= 0.0);
    }
}
