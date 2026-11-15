//! Sound (docs/ROADMAP.md section 5, first half).
//!
//! A sound is an **event**, not a field: something makes a noise during a
//! tick ([`Sound`], kept in `Scene::sounds`), and a listener asks what it
//! hears of it (`Scene::hear`). Nothing runs per tick when it is quiet except
//! the [`Noise`] floor, which reads a few numbers per awake chunk.
//!
//! Levels are decibels: a sound's `db` is its level 1 m from the source. On
//! the way to a listener it loses `20 log10(r) + 0.005 r` (spreading out, and
//! a little to the air) plus whatever walls and closed doors lie on the
//! straight line between them ([`path_loss`]). It is heard if what is left
//! stands far enough above the background noise where the listener is.
//!
//! Not yet (second half of section 5): the round-corner search through open
//! doors, floor slabs between levels, and wiring footsteps and life events.

use crate::geom::Pos;
use crate::things::{ThingId, Things};
use crate::world::{World, CHUNK};

/// Typical levels at 1 m, in dB.
pub mod level {
    pub const WHISPER: f32 = 30.0;
    pub const SPEECH: f32 = 60.0;
    pub const SHOUT: f32 = 80.0;
    pub const SCREAM: f32 = 90.0;
    /// Footsteps run from 20 (light, on sand) to 45 (heavy, on wood).
    pub const FOOTSTEP: (f32, f32) = (20.0, 45.0);
    pub const SWORD_ON_SHIELD: f32 = 95.0;
    pub const COLLAPSE: f32 = 100.0;
    pub const MUSKET: f32 = 140.0;
    pub const CANNON: f32 = 160.0;
    /// A fire catching.
    pub const FIRE: f32 = 60.0;
}

/// The level of a blast of this `power` (see `projectile::Blast`).
pub fn blast_db(power: f32) -> f32 {
    100.0 + 20.0 * power.max(1.0).log10()
}

/// How far below the background a listener can still pick a sound out, for
/// `Scene::hear`'s `hearing_db` (lower is better). A sound is heard when its
/// [`Heard::db_over`] is at least this.
pub const HUMAN_HEARING: f32 = -10.0;
pub const DOG_HEARING: f32 = -20.0;

/// How loud a voice is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Voice {
    Whisper,
    Talk,
    Shout,
}

impl Voice {
    pub fn db(self) -> f32 {
        match self {
            Voice::Whisper => level::WHISPER,
            Voice::Talk => level::SPEECH,
            Voice::Shout => level::SHOUT,
        }
    }
}

/// What made a sound, so a game can word it ("You hear a scream").
/// `Custom` carries a game's own id for anything else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundKind {
    Speech(Voice),
    Scream,
    Footstep,
    Impact,
    Blast,
    Collapse,
    Fire,
    Water,
    Animal,
    Bell,
    Custom(u16),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sound {
    pub pos: Pos,
    /// Level 1 m from the source, dB.
    pub db: f32,
    pub kind: SoundKind,
    /// Who or what made it, if anyone (so a creature can ignore its own voice).
    pub source: Option<ThingId>,
}

impl Sound {
    pub fn new(pos: Pos, db: f32, kind: SoundKind) -> Sound {
        Sound { pos, db, kind, source: None }
    }

    /// Someone speaking at the loudness of their voice.
    pub fn speech(pos: Pos, voice: Voice, who: ThingId) -> Sound {
        Sound { pos, db: voice.db(), kind: SoundKind::Speech(voice), source: Some(who) }
    }
}

/// One sound as a listener hears it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Heard {
    /// Index into the sounds it was asked about (`Scene::sounds`).
    pub sound: usize,
    /// How far it stands above the background noise where the listener is, dB
    /// (it can be below zero: a keen ear picks a sound out of the noise).
    pub db_over: f32,
    /// The way it seems to come from, as an offset from the listener. For now
    /// that is the source itself; the round-corner search will make it the
    /// first opening on the path (a shout round a corner comes from the door).
    pub from: (i32, i32, i32),
}

/// The most walls and doors on one line can take off a sound. Beyond this the
/// sound is carried by the building itself, so more stone stops mattering.
pub const MAX_WALL_LOSS: f32 = 60.0;

/// dB lost to walls and closed doors strictly between `a` and `b`, on the
/// straight line joining them (sampled like `projectile::shielding`), capped
/// at [`MAX_WALL_LOSS`].
pub fn wall_loss(world: &World, things: &Things, a: Pos, b: Pos) -> f32 {
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
        let Some(i) = world.index(c) else { continue };
        if world.tile[i].is_wall() {
            sum += world.mats.get(world.mat[i]).sound_loss;
        } else {
            // `.map(..).sum::<f32>()`: the `::<f32>` ("turbofish") tells
            // `sum` which type to add up into, since Rust cannot guess it here.
            sum += things
                .at(c)
                .map(|(_, t)| {
                    let k = things.kind(t.kind);
                    if k.door && t.open { 0.0 } else { k.sound_loss }
                })
                .sum::<f32>();
        }
        if sum >= MAX_WALL_LOSS {
            return MAX_WALL_LOSS;
        }
    }
    sum
}

/// Everything a sound loses from `a` to `b`: spreading, air, and walls.
pub fn path_loss(world: &World, things: &Things, a: Pos, b: Pos) -> f32 {
    spread_loss(dist(a, b)) + wall_loss(world, things, a, b)
}

/// Loss over `r` metres of open air.
pub fn spread_loss(r: f32) -> f32 {
    20.0 * r.max(1.0).log10() + 0.005 * r
}

fn dist(a: Pos, b: Pos) -> f32 {
    let (dx, dy, dz) = ((b.x - a.x) as f32, (b.y - a.y) as f32, (b.z - a.z) as f32);
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// What a listener at `at` hears of `sounds`, given the background `floor`
/// there and how keen its hearing is (`hearing_db`, see [`HUMAN_HEARING`]).
pub fn hear(world: &World, things: &Things, sounds: &[Sound], at: Pos, floor: f32, hearing_db: f32) -> Vec<Heard> {
    let need = floor + hearing_db;
    let mut out = Vec::new();
    for (n, s) in sounds.iter().enumerate() {
        let open_air = s.db - spread_loss(dist(s.pos, at));
        // Too faint even with nothing in the way: skip the line walk.
        if open_air < need {
            continue;
        }
        let got = open_air - wall_loss(world, things, s.pos, at);
        if got >= need {
            out.push(Heard { sound: n, db_over: got - floor, from: (s.pos.x - at.x, s.pos.y - at.y, s.pos.z - at.z) });
        }
    }
    out
}

/// The background level with nothing going on: a quiet night indoors.
pub const QUIET: f32 = 25.0;
/// How much a downpour (`World::rain` = 1) lifts the floor under open sky.
pub const RAIN_LIFT: f32 = 20.0;
/// How much the wind lifts it under open sky, reached at [`WIND_FULL`] m/s.
pub const WIND_LIFT: f32 = 25.0;
pub const WIND_FULL: f32 = 25.0;
/// Most that flowing water lifts it (beside a weir).
pub const WATER_LIFT: f32 = 30.0;
/// Burning cells lift it by 10 dB for one, up to this for a hundred.
pub const FIRE_LIFT: f32 = 30.0;
/// What a roof and walls take off the weather (rain and wind) indoors.
pub const SHELTER: f32 = 20.0;

/// The background noise: one number per chunk column (16 x 16 cells, all
/// levels), recomputed every tick, for listeners under open sky and for
/// listeners under cover. Rain and wind come from the world's weather;
/// flowing water and fire from the awake chunks of that column.
#[derive(Clone, Debug, Default)]
pub struct Noise {
    cw: i32,
    open: Vec<f32>,
    sheltered: Vec<f32>,
    // Scratch, kept between ticks so nothing is allocated per tick.
    chunks: Vec<usize>,
    flux: Vec<f32>,
    fires: Vec<u32>,
}

/// Lifts (dB above [`QUIET`]) from several sources at once. Noises add as
/// powers, not as decibels: two sources that each lift the floor by 10 dB
/// lift it by about 13 together.
fn combine(lifts: &[f32]) -> f32 {
    let p: f32 = lifts.iter().map(|&x| 10f32.powf(x.max(0.0) / 10.0) - 1.0).sum();
    QUIET + 10.0 * (1.0 + p).log10()
}

impl Noise {
    pub fn new() -> Noise {
        Noise::default()
    }

    /// Recompute the floor. `wind_ms` is the wind speed in m/s.
    pub fn update(&mut self, world: &World, wind_ms: f32) {
        let (cw, ch) = (world.wake.cw, world.wake.ch);
        let n = (cw * ch) as usize;
        if self.open.len() != n {
            self.cw = cw;
            self.open = vec![QUIET; n];
            self.sheltered = vec![QUIET; n];
            self.flux = vec![0.0; n];
            self.fires = vec![0; n];
        }
        self.flux.fill(0.0);
        self.fires.fill(0);
        // Only awake chunks: water that is moving and fire that is burning
        // keep their chunk awake, so sleeping chunks are silent.
        world.wake.collect(&mut self.chunks);
        for &c in &self.chunks {
            let (x0, y0, x1, y1, z) = world.wake.bounds(c);
            let col = ((y0 / CHUNK) * cw + x0 / CHUNK) as usize;
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = world.idx(x, y, z);
                    if world.fluid[i] > 0 {
                        self.flux[col] += world.flow_x[i].hypot(world.flow_y[i]);
                    }
                    if world.fire[i] > 0 {
                        self.fires[col] += 1;
                    }
                }
            }
        }
        let rain = RAIN_LIFT * world.rain.clamp(0.0, 1.0);
        let wind = WIND_LIFT * (wind_ms / WIND_FULL).clamp(0.0, 1.0);
        for col in 0..n {
            // One cell moving a full cell of water per tick lifts it ~9.5 dB.
            let water = (10.0 * (1.0 + self.flux[col] / 8.0).log10()).min(WATER_LIFT);
            let f = self.fires[col];
            let fire = if f == 0 { 0.0 } else { (10.0 + 10.0 * (f as f32).log10()).min(FIRE_LIFT) };
            self.open[col] = combine(&[rain, wind, water, fire]);
            self.sheltered[col] = combine(&[rain - SHELTER, wind - SHELTER, water, fire]);
        }
    }

    /// The background level at `p`, dB. Before the first update, [`QUIET`].
    pub fn floor(&self, world: &World, p: Pos) -> f32 {
        let Some(i) = world.index(p) else { return QUIET };
        let col = ((p.y / CHUNK) * self.cw + p.x / CHUNK) as usize;
        let v = if world.open_sky[i] { &self.open } else { &self.sheltered };
        // `get` gives `None` past the end (no update yet) instead of panicking.
        v.get(col).copied().unwrap_or(QUIET)
    }
}
