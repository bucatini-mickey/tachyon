//! A scene bundles one simulated place: the grid, the things in it, its
//! clock and the physics state. A game can hold several (a world map and the
//! local area you are in, a ship and the sea around it).

use crate::clock::{Clock, Span};
use crate::sim::Sim;
use crate::geom::Pos;
use crate::life::mind::Control;
use crate::life::{Life, LifeEvent};
use crate::projectile::{BlastReport, Flight, Projectile};
use crate::world::CHUNK;
use std::cell::RefCell;
use std::time::Instant;

use crate::sound::{self, Heard, Noise, Router, Sound, SoundKind};
use crate::things::{KindId, ThingId, Things};
use crate::vessel::Vessel;
use crate::weather::Weather;
use crate::world::World;

pub struct Scene {
    pub world: World,
    pub things: Things,
    pub clock: Clock,
    pub sim: Sim,
    pub vessels: Vec<Vessel>,
    /// Creatures: bodies, needs, minds.
    pub life: Life,
    pub weather: Weather,
    pub projectiles: Vec<Projectile>,
    /// Things that vanish by themselves after some ticks (a flash, a spark).
    pub effects: Vec<(ThingId, u16)>,
    /// Kind spawned as a brief flash where something explodes, if any.
    pub flash: Option<KindId>,
    /// What happened this tick that a game may want to report; the game
    /// drains it.
    pub events: Vec<SceneEvent>,
    /// Sounds made during the last tick, and any the game has pushed since
    /// (the player shouting). Each lasts until the start of the tick after
    /// the one it was made in, so everything gets one chance to be heard.
    pub sounds: Vec<Sound>,
    /// The background noise, recomputed each tick (see [`Scene::hear`]).
    pub noise: Noise,
    // `RefCell` lets `hear(&self)` borrow this scratch memory mutably at run
    // time (checked then, not by the compiler), so listening needs no `&mut`.
    router: RefCell<Router>,
    /// How many of `sounds` were there when the last tick ended.
    old_sounds: usize,
    /// `World::fires_started` when we last looked, to notice new fires.
    fires_seen: u64,
    /// Where each creature stood at the end of the last tick, to hear its
    /// footsteps (it may have been moved by the game in between: the player).
    last_pos: Vec<(ThingId, Pos)>,
    /// Microseconds spent on sound in the last tick: the noise floor, making
    /// the tick's sounds, and every creature listening.
    pub sound_us: u64,
    /// The part of `sound_us` spent on the noise floor.
    pub noise_us: u64,
}

#[derive(Clone, Copy, Debug)]
pub enum SceneEvent {
    Exploded(Pos, BlastReport),
    Fizzled(Pos),
}

impl Scene {
    pub fn new(world: World, things: Things, clock: Clock, seed: u64) -> Scene {
        let fires_seen = world.fires_started; // read before `world` moves into the struct
        Scene { world, things, clock, sim: Sim::new(seed), vessels: Vec::new(), life: Life::new(seed),
            weather: Weather::new(seed as u32 ^ 0x3EA7),
            projectiles: Vec::new(),
            effects: Vec::new(),
            flash: None,
            events: Vec::new(),
            sounds: Vec::new(),
            noise: Noise::new(),
            router: RefCell::new(Router::new()),
            old_sounds: 0,
            fires_seen,
            last_pos: Vec::new(),
            sound_us: 0,
            noise_us: 0,
        }
    }

    /// Advance the physics one tick.
    pub fn tick(&mut self) {
        // `drain(..n)` removes the first n items (the sounds already heard
        // once); dropping its result right away is enough.
        self.sounds.drain(..self.old_sounds.min(self.sounds.len()));
        self.world.ambient = self.clock.ambient_temp();
        self.weather.apply(&mut self.world, &self.clock);
        self.sim.step(&mut self.world, self.clock.tick);
        // A new fire this tick, or one the game lit since the last tick.
        if self.world.fires_started > self.fires_seen {
            self.fires_seen = self.world.fires_started;
            if let Some(at) = self.world.last_fire_start {
                self.sounds.push(Sound::new(at, sound::level::FIRE, SoundKind::Fire));
            }
        }
        let (wx, wy) = self.world.wind;
        let t0 = Instant::now();
        self.noise.update(&self.world, wx.hypot(wy) / self.clock.secs_per_tick as f32);
        self.noise_us = t0.elapsed().as_micros() as u64;
        crate::sim::flames::step(&self.world, &mut self.things, &mut self.sim.rng, self.clock.secs_per_tick as f32);
        for v in &mut self.vessels {
            v.step(&mut self.world, &mut self.things);
        }
        self.vessels.retain(|v| !v.parts.is_empty());
        let mut i = 0;
        while i < self.projectiles.len() {
            match self.projectiles[i].step(&mut self.world, &mut self.things, &mut self.sim.rng) {
                Flight::Flying => {
                    i += 1;
                    continue;
                }
                Flight::Exploded(at, report) => {
                    let (blast, owner) = (self.projectiles[i].blast, self.projectiles[i].owner);
                    self.life.blast(&self.world, &self.things, at, &blast, owner);
                    if let Some(k) = self.flash {
                        let id = self.things.spawn(k, at);
                        self.effects.push((id, 3));
                    }
                    self.sounds.push(Sound { source: owner, ..Sound::new(at, sound::blast_db(blast.power), SoundKind::Blast) });
                    self.events.push(SceneEvent::Exploded(at, report));
                }
                Flight::Fizzled(at) => self.events.push(SceneEvent::Fizzled(at)),
                Flight::Gone => {}
            }
            self.projectiles.swap_remove(i);
        }
        let things = &mut self.things;
        self.effects.retain_mut(|(id, ttl)| {
            *ttl = ttl.saturating_sub(1);
            if *ttl == 0 {
                things.despawn(*id);
            }
            *ttl > 0
        });
        self.life.step(&mut self.world, &mut self.things, &self.clock);
        let t0 = Instant::now();
        self.gather_sounds();
        self.listen();
        self.sound_us = self.noise_us + t0.elapsed().as_micros() as u64;
        self.clock.advance();
        self.old_sounds = self.sounds.len();
    }

    /// Turn what happened this tick into sounds: creatures' noises, walls
    /// giving way, doors opening, and footsteps.
    fn gather_sounds(&mut self) {
        // `append` moves everything across and leaves the source empty.
        self.sounds.append(&mut self.life.sounds);
        // Walls coming down: one sound per chunk and level, however many.
        let start = self.sounds.len();
        for p in std::mem::take(&mut self.world.crumbled) {
            let near = |s: &Sound| s.pos.z == p.z && s.pos.x / CHUNK == p.x / CHUNK && s.pos.y / CHUNK == p.y / CHUNK;
            if !self.sounds[start..].iter().any(near) {
                self.sounds.push(Sound::new(p, sound::level::COLLAPSE, SoundKind::Collapse));
            }
        }
        for p in self.things.doors_opened.drain(..) {
            self.sounds.push(Sound::new(p, sound::level::DOOR, SoundKind::Door));
        }
        self.footsteps();
    }

    /// A footstep for every creature that moved to a new cell since the last
    /// tick, if someone is near enough to hear it.
    fn footsteps(&mut self) {
        let Scene { world, things, life, sounds, last_pos, .. } = self;
        let now = |id: ThingId| things.get(id).map(|t| t.pos());
        // Listeners: anyone awake or asleep, not knocked out or dead.
        let listening = |c: &crate::life::Creature| c.alive && !c.out;
        if last_pos.len() == life.creatures.len() {
            for (k, c) in life.creatures.iter().enumerate() {
                let (id, was) = last_pos[k];
                let Some(p) = now(c.thing) else { continue };
                // A step is a change of cell sideways; a fall is straight down.
                if id != c.thing || !c.alive || (p.x == was.x && p.y == was.y) {
                    continue;
                }
                let Some(i) = world.index(p) else { continue };
                // Swimming makes no footsteps; nor does riding a deck, which
                // moves everyone aboard each tick (and a ship creaks louder).
                if world.fluid[i] >= crate::movement::SWIM_DEPTH || things.deck_at(p) {
                    continue;
                }
                let footfall = if world.tile[i].has_floor() {
                    world.mats.get(world.mat[i]).footfall
                } else if p.z > 0 {
                    world.mats.get(world.mat[i - world.layer()]).footfall
                } else {
                    0.0
                };
                let sp = life.species_of(c);
                let db = sound::footstep_db(sp.mass, footfall);
                let heard_by = |o: &crate::life::Creature| {
                    let Some(q) = now(o.thing) else { return false };
                    let (dx, dy, dz) = ((q.x - p.x) as f32, (q.y - p.y) as f32, (q.z - p.z) as f32);
                    let r = sound::reach(db, life.species_of(o).hearing);
                    dx * dx + dy * dy + dz * dz <= r * r
                };
                if life.creatures.iter().any(|o| o.thing != c.thing && listening(o) && heard_by(o)) {
                    sounds.push(Sound { source: Some(c.thing), ..Sound::new(p, db, SoundKind::Footstep) });
                }
            }
        }
        last_pos.clear();
        last_pos.extend(life.creatures.iter().map(|c| (c.thing, now(c.thing).unwrap_or_default())));
    }

    /// Every creature with a mind hears this tick's sounds and reacts.
    fn listen(&mut self) {
        if self.sounds.is_empty() {
            return;
        }
        let Scene { world, things, life, sounds, noise, router, clock, .. } = self;
        // `get_mut` borrows the scratch memory without the run-time check:
        // with `&mut self` the compiler already knows nobody else has it.
        let router = router.get_mut();
        for i in 0..life.creatures.len() {
            let c = &life.creatures[i];
            if !c.alive || c.out || c.mind.control != Control::Ai {
                continue;
            }
            let Some(pos) = things.get(c.thing).map(|t| t.pos()) else { continue };
            let hearing = life.species_of(c).hearing;
            let heard = sound::hear(world, things, router, sounds, pos, noise.floor(world, pos), hearing);
            if !heard.is_empty() {
                crate::life::ai::react(life, i, world, things, sounds, &heard, pos, clock);
            }
        }
    }

    /// What a listener at `at` hears of `self.sounds`. `hearing_db` is how far
    /// below the background it can still pick a sound out
    /// (`sound::HUMAN_HEARING`, `sound::DOG_HEARING`).
    pub fn hear(&self, at: Pos, hearing_db: f32) -> Vec<Heard> {
        let floor = self.noise.floor(&self.world, at);
        sound::hear(&self.world, &self.things, &mut self.router.borrow_mut(), &self.sounds, at, floor, hearing_db)
    }

    /// How many round-the-corner searches `hear` has run so far.
    pub fn sound_searches(&self) -> u64 {
        self.router.borrow().searches
    }

    /// The vessel whose deck is at `p`, if any.
    pub fn vessel_at(&self, p: Pos) -> Option<usize> {
        self.vessels.iter().position(|v| v.contains(p))
    }

    /// Run the physics without a clock-driven day (for settling a fresh map).
    pub fn settle(&mut self, ticks: u32) {
        for _ in 0..ticks {
            self.tick();
        }
    }

    /// Run time on without drawing (sleep, a long job, travel) for up to
    /// `span` of game time, stopping early after the first tick in which one
    /// of the `stop` events happened. Every system ticks as usual, so fire and
    /// water behave exactly as they would at normal speed; it is only quicker
    /// because nothing is drawn. Events pile up in `self.events` and
    /// `self.life.events` for the game to report afterwards.
    pub fn fast_forward(&mut self, span: Span, stop: StopOn) -> Forwarded {
        let n = self.clock.ticks_in(span);
        for done in 1..=n {
            let (life0, scene0, fires0) = (self.life.events.len(), self.events.len(), self.world.fires_started);
            self.tick();
            if let Some(who) = stop.creature {
                if let Some(e) = self.life.events[life0..].iter().find(|e| e.who().contains(&who)) {
                    return Forwarded { ticks: done, stopped: Stopped::Life(e.clone()) };
                }
            }
            if stop.explosion {
                if let Some(at) = self.events[scene0..].iter().find_map(|e| match e {
                    SceneEvent::Exploded(at, _) => Some(*at),
                    _ => None,
                }) {
                    return Forwarded { ticks: done, stopped: Stopped::Exploded(at) };
                }
            }
            if stop.fire && self.world.fires_started > fires0 {
                let at = self.world.last_fire_start.unwrap_or_default();
                return Forwarded { ticks: done, stopped: Stopped::FireStarted(at) };
            }
        }
        Forwarded { ticks: n, stopped: Stopped::Done }
    }
}

/// What ends a [`Scene::fast_forward`] early. `Default` stops on nothing.
#[derive(Clone, Copy, Debug, Default)]
pub struct StopOn {
    /// Any `LifeEvent` about this creature (it wakes, is struck, burns...).
    pub creature: Option<ThingId>,
    /// Anything exploding.
    pub explosion: bool,
    /// A new fire starting anywhere (not a burning fire spreading).
    pub fire: bool,
}

/// How a fast-forward ended.
#[derive(Clone, Debug, PartialEq)]
pub enum Stopped {
    /// The whole span ran.
    Done,
    Life(LifeEvent),
    Exploded(Pos),
    FireStarted(Pos),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Forwarded {
    /// Ticks actually run.
    pub ticks: u64,
    pub stopped: Stopped,
}
