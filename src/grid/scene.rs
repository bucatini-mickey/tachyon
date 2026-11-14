//! A scene bundles one simulated place: the grid, the things in it, its
//! clock and the physics state. A game can hold several (a world map and the
//! local area you are in, a ship and the sea around it).

use crate::clock::{Clock, Span};
use crate::sim::Sim;
use crate::geom::Pos;
use crate::life::{Life, LifeEvent};
use crate::projectile::{BlastReport, Flight, Projectile};
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
}

#[derive(Clone, Copy, Debug)]
pub enum SceneEvent {
    Exploded(Pos, BlastReport),
    Fizzled(Pos),
}

impl Scene {
    pub fn new(world: World, things: Things, clock: Clock, seed: u64) -> Scene {
        Scene { world, things, clock, sim: Sim::new(seed), vessels: Vec::new(), life: Life::new(seed),
            weather: Weather::new(seed as u32 ^ 0x3EA7),
            projectiles: Vec::new(),
            effects: Vec::new(),
            flash: None,
            events: Vec::new(),
        }
    }

    /// Advance the physics one tick.
    pub fn tick(&mut self) {
        self.world.ambient = self.clock.ambient_temp();
        self.weather.apply(&mut self.world, &self.clock);
        self.sim.step(&mut self.world, self.clock.tick);
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
        self.clock.advance();
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
