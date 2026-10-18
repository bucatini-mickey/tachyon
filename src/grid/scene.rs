//! A scene bundles one simulated place: the grid, the things in it, its
//! clock and the physics state. A game can hold several (a world map and the
//! local area you are in, a ship and the sea around it).

use crate::clock::Clock;
use crate::sim::Sim;
use crate::geom::Pos;
use crate::life::Life;
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
        self.weather.apply(&mut self.world, self.clock.tick);
        self.sim.step(&mut self.world, self.clock.tick);
        crate::sim::flames::step(&self.world, &mut self.things, &mut self.sim.rng);
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
}
