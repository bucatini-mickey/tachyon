//! Life: creatures with bodies, needs and minds.
//!
//! A creature is a [`Thing`](crate::things::Thing) on the creature layer (so
//! it is drawn, moved and found like any other thing) plus a [`Creature`]
//! record here: its species, its body (parts, tissues, wounds, blood), its
//! needs (hunger, thirst, sleep, warmth, breath) and its mind (routine,
//! grudges, current goal).
//!
//! [`Life::step`] runs once a tick, after the physics, and ties creatures to
//! the world: fire burns them, water drowns them, falls break their legs,
//! rain soaks and chills them, and their blood stains the floor. Then each
//! creature with a mind decides what to do (see [`ai`]).
//!
//! Everything that happens is pushed to [`Life::events`]; [`Life::narrate`]
//! turns an event into a sentence, and the game decides which ones to show.

pub mod ai;
pub mod body;
pub mod mind;
pub mod needs;
pub mod path;
pub mod species;

use crate::clock::Clock;
use crate::color::Rgb;
use crate::geom::Pos;
use crate::material::MatId;
use crate::movement;
use crate::projectile::{shielding, Blast};
use crate::rng::Rng;
use crate::things::{ThingId, Things};
use crate::world::{World, FULL};
use body::{Body, Capacities, Cause, DamageKind, Does, Height, Injury, Tissue, WoundKind};
use mind::{Control, Goal, Mind};
use needs::{Needs, Surroundings};
use species::{Attack, Species};
use std::collections::HashMap;

pub type SpeciesId = u16;

#[derive(Clone, Debug)]
pub struct Creature {
    pub thing: ThingId,
    pub species: SpeciesId,
    pub body: Body,
    pub needs: Needs,
    pub mind: Mind,
    /// A held weapon, used instead of the species' own attacks while it has
    /// a working hand.
    pub weapon: Option<Attack>,
    pub alive: bool,
    pub death: Option<Cause>,
    /// Knocked out (by pain, blood loss, cold, a blow to the head ...).
    pub out: bool,
    /// What its body can do, as of the last step.
    pub caps: Capacities,
    /// Levels fallen so far in the current fall.
    falling: i32,
    /// Seconds of exposure to fire since the last burn.
    scorch: f32,
}

/// Blood (or anything else) spilled on a floor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stain {
    pub ml: f32,
    pub color: Rgb,
    /// Seconds since it was last added to.
    pub age: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Stains {
    map: HashMap<Pos, Stain>,
}

impl Stains {
    pub fn add(&mut self, p: Pos, ml: f32, color: Rgb) {
        let s = self.map.entry(p).or_insert(Stain { ml: 0.0, color, age: 0.0 });
        s.ml += ml;
        s.age = 0.0;
        s.color = color;
    }

    pub fn get(&self, p: Pos) -> Option<&Stain> {
        self.map.get(&p)
    }

    /// The colour to tint a floor, and how strongly (0..1). Fresh blood is
    /// bright; it dries to brown over a few hours.
    pub fn look(&self, p: Pos) -> Option<(Rgb, f32)> {
        let s = self.map.get(&p)?;
        let dried = s.color.lerp(Rgb(64, 30, 20), s.age / (4.0 * 3600.0));
        Some((dried, (s.ml / 300.0).sqrt().min(1.0)))
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Water and rain wash stains away; old ones fade.
    pub fn step(&mut self, world: &World, dt: f32) {
        self.map.retain(|p, s| {
            s.age += dt;
            if let Some(i) = world.index(*p) {
                if world.fluid[i] >= 8 {
                    s.ml -= dt * 20.0;
                } else if world.open_sky[i] && world.rain > 0.0 {
                    s.ml -= dt * world.rain * 0.5;
                }
            }
            s.ml -= dt * s.ml / (6.0 * 86400.0);
            s.ml > 3.0
        });
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum LifeEvent {
    /// `injury` is None for a miss.
    Struck { attacker: ThingId, target: ThingId, attack: Attack, injury: Option<Injury> },
    Blasted { who: ThingId, injuries: Vec<Injury> },
    Fell { who: ThingId, levels: i32, injury: Option<Injury> },
    Burned { who: ThingId, injury: Injury },
    Collapsed(ThingId),
    CameTo(ThingId),
    Drowning(ThingId),
    Died { who: ThingId, cause: Cause },
    Ate { who: ThingId, what: String },
    Drank(ThingId),
    FellAsleep(ThingId),
    Woke(ThingId),
}

impl LifeEvent {
    /// The creatures it is about (the first is the one doing it).
    pub fn who(&self) -> Vec<ThingId> {
        match self {
            LifeEvent::Struck { attacker, target, .. } => vec![*attacker, *target],
            LifeEvent::Blasted { who, .. }
            | LifeEvent::Fell { who, .. }
            | LifeEvent::Burned { who, .. }
            | LifeEvent::Died { who, .. }
            | LifeEvent::Ate { who, .. } => vec![*who],
            LifeEvent::Collapsed(w) | LifeEvent::CameTo(w) | LifeEvent::Drowning(w) | LifeEvent::Drank(w) | LifeEvent::FellAsleep(w) | LifeEvent::Woke(w) => vec![*w],
        }
    }
}

/// ml in one unit of liquid (a 64th of a cubic metre).
const ML_PER_UNIT: f32 = 1_000_000.0 / FULL as f32;
/// Fall damage per level beyond the first, roughly.
const FALL_FORCE: f32 = 14.0;

pub struct Life {
    pub species: Vec<Species>,
    pub creatures: Vec<Creature>,
    index: HashMap<ThingId, usize>,
    pub stains: Stains,
    pub events: Vec<LifeEvent>,
    /// Liquids creatures drink.
    pub drinkable: Vec<MatId>,
    /// Floors grazing animals eat.
    pub grazing: Vec<MatId>,
    pub rng: Rng,
    last_secs: Option<f64>,
    /// Water drunk but not yet taken from the world (it goes a unit at a time).
    drunk_ml: f32,
}

impl Life {
    pub fn new(seed: u64) -> Life {
        Life {
            species: Vec::new(),
            creatures: Vec::new(),
            index: HashMap::new(),
            stains: Stains::default(),
            events: Vec::new(),
            drinkable: Vec::new(),
            grazing: Vec::new(),
            rng: Rng::new(seed ^ 0x11FE),
            last_secs: None,
            drunk_ml: 0.0,
        }
    }

    pub fn add_species(&mut self, s: Species) -> SpeciesId {
        self.species.push(s);
        (self.species.len() - 1) as SpeciesId
    }

    pub fn find_species(&self, name: &str) -> Option<SpeciesId> {
        self.species.iter().position(|s| s.name == name).map(|i| i as SpeciesId)
    }

    /// Give a thing (already spawned on the creature layer) a body and a mind.
    pub fn add(&mut self, thing: ThingId, species: SpeciesId, control: Control, faction: u16) -> &mut Creature {
        let sp = &self.species[species as usize];
        let c = Creature {
            thing,
            species,
            body: Body::new(&sp.plan),
            needs: Needs::default(),
            mind: Mind::new(control, faction),
            weapon: None,
            alive: true,
            death: None,
            out: false,
            caps: Capacities::default(),
            falling: 0,
            scorch: 0.0,
        };
        let c = Creature { caps: c.body.capacities(&sp.plan), ..c };
        self.creatures.push(c);
        let i = self.creatures.len() - 1;
        self.index.insert(thing, i);
        &mut self.creatures[i]
    }

    pub fn get(&self, id: ThingId) -> Option<&Creature> {
        self.index.get(&id).map(|&i| &self.creatures[i])
    }

    pub fn get_mut(&mut self, id: ThingId) -> Option<&mut Creature> {
        self.index.get(&id).map(|&i| &mut self.creatures[i])
    }

    pub fn species_of(&self, c: &Creature) -> &Species {
        &self.species[c.species as usize]
    }

    /// "you", "Agnes", "the rat".
    pub fn name(&self, things: &Things, id: ThingId) -> String {
        match self.get(id) {
            Some(c) if c.mind.control == Control::Player => "you".into(),
            Some(c) => match things.get(id).and_then(|t| t.name.clone()) {
                Some(n) => n,
                None => format!("the {}", self.species_of(c).name),
            },
            None => things.get(id).map_or("something".into(), |t| format!("the {}", things.kind_of(t).name)),
        }
    }

    fn is_player(&self, id: ThingId) -> bool {
        self.get(id).is_some_and(|c| c.mind.control == Control::Player)
    }

    /// Advance every creature by the game time since the last step.
    pub fn step(&mut self, world: &mut World, things: &mut Things, clock: &Clock) {
        let now = clock.seconds();
        let dt = match self.last_secs {
            Some(l) => (now - l).max(0.0) as f32,
            None => clock.secs_per_tick as f32,
        };
        self.last_secs = Some(now);
        // Forget creatures whose things are gone (eaten corpses).
        let before = self.creatures.len();
        self.creatures.retain(|c| things.get(c.thing).is_some());
        if self.creatures.len() != before {
            self.index = self.creatures.iter().enumerate().map(|(i, c)| (c.thing, i)).collect();
        }
        for i in 0..self.creatures.len() {
            self.physiology(i, world, things, dt);
        }
        let hour = clock.hour() as f32;
        for i in 0..self.creatures.len() {
            ai::act(self, i, world, things, clock.tick, hour);
        }
        self.stains.step(world, dt);
    }

    fn physiology(&mut self, i: usize, world: &mut World, things: &mut Things, dt: f32) {
        // Borrow the fields separately so a creature and its species can be
        // used at the same time.
        let Life { species, creatures, stains, events, rng, .. } = self;
        let c = &mut creatures[i];
        if !c.alive {
            return;
        }
        let sp = &species[c.species as usize];
        let plan = &sp.plan;
        let id = c.thing;

        // Falling, a level a tick; landing hurts.
        if movement::fall(world, things, id) {
            c.falling += 1;
        } else if c.falling > 0 {
            let levels = c.falling;
            c.falling = 0;
            let p = things.get(id).map(|t| t.pos()).unwrap_or_default();
            let soft = world.index(p).is_some_and(|i| world.fluid[i] >= movement::SWIM_DEPTH);
            let mut injury = None;
            if levels >= 2 && !soft {
                let force = FALL_FORCE * ((levels - 1) as f32).powf(1.3);
                let mut all = Injury::default();
                for (reach, f) in [(Height::Low, force), (Height::Low, force), (Height::High, force * 0.5)] {
                    if let Some(part) = c.body.pick_part(plan, reach, rng) {
                        let inj = c.body.strike(plan, part, DamageKind::Blunt, f, rng);
                        all.wounds.extend(inj.wounds);
                        all.part = part;
                    }
                }
                c.mind.asleep = false;
                injury = Some(all);
            }
            events.push(LifeEvent::Fell { who: id, levels, injury });
        }

        let Some(pos) = things.get(id).map(|t| t.pos()) else { return };
        let Some(ix) = world.index(pos) else { return };
        let fluid = world.fluid[ix];
        let on_deck = things.deck_at(pos);
        let swimming = fluid >= movement::SWIM_DEPTH && !on_deck;
        let above = world.index(pos.up());
        let covered = above.map_or(true, |j| world.fluid[j] >= movement::SWIM_DEPTH || world.tile[j].is_wall() || world.tile[j].seals_below());
        let underwater = swimming && fluid >= FULL && covered;
        let struggling = swimming && !sp.swims;
        let s = Surroundings {
            air: world.temp[ix],
            underwater: underwater || (struggling && rng.chance(0.3)),
            in_water: fluid >= 16 && !on_deck,
            rain: if world.open_sky[ix] { world.rain } else { 0.0 },
            asleep: c.mind.asleep,
        };
        let had_breath = c.needs.breath > 0.0;
        c.needs.step(&sp.needs, &s, dt);
        if had_breath && c.needs.breath <= 0.0 {
            events.push(LifeEvent::Drowning(id));
        }

        // Wounds bleed, clot and heal, in steps of half a minute at most (so a
        // skipped hour is not one giant step).
        let heal = if c.needs.hunger >= 1.0 { 0.2 } else { 1.0 } * if c.mind.asleep { 1.5 } else { 1.0 };
        let mut left = dt;
        let mut spilt = 0.0;
        while left > 0.0 {
            let h = left.min(30.0);
            spilt += c.body.step(plan, h, heal);
            left -= h;
        }
        if spilt > 0.0 && fluid < 8 {
            stains.add(pos, spilt, sp.blood);
        }

        // Fire and great heat burn.
        let air = world.temp[ix];
        let fire = world.fire[ix] > 0;
        if fire || air > 70.0 {
            if c.scorch == 0.0 {
                c.scorch = 3.0; // the first touch burns
            }
            c.scorch += dt * if fire { 1.0 } else { ((air - 70.0) / 100.0).min(1.0) };
            while c.scorch >= 3.0 {
                c.scorch -= 3.0;
                if let Some(part) = c.body.pick_part(plan, Height::High, rng) {
                    let force = if fire { 20.0 } else { 10.0 + (air - 70.0) / 20.0 };
                    let injury = c.body.strike(plan, part, DamageKind::Burn, force, rng);
                    if !injury.wounds.is_empty() {
                        c.mind.alarmed = true;
                        c.mind.asleep = false;
                        events.push(LifeEvent::Burned { who: id, injury });
                    }
                }
            }
        } else {
            c.scorch = 0.0;
        }

        // Grudges cool.
        c.mind.grudges.retain_mut(|g| {
            g.anger -= dt / 1800.0;
            g.anger > 0.0
        });

        // What it can do now.
        let mut caps = c.body.capacities(plan);
        let f = c.needs.factor();
        caps.conscious *= f;
        caps.moving = caps.moving.min(caps.conscious * 1.2);
        caps.grasp = caps.grasp.min(caps.conscious);
        c.caps = caps;
        if !c.out && caps.conscious < 0.2 {
            c.out = true;
            c.mind.asleep = false;
            c.mind.path.clear();
            events.push(LifeEvent::Collapsed(id));
        } else if c.out && caps.conscious > 0.3 {
            c.out = false;
            events.push(LifeEvent::CameTo(id));
        }

        if let Some(cause) = c.body.fatal(plan).or_else(|| c.needs.fatal(underwater)) {
            Self::kill(c, sp, things, events, cause);
        }
    }

    fn kill(c: &mut Creature, sp: &Species, things: &mut Things, events: &mut Vec<LifeEvent>, cause: Cause) {
        c.alive = false;
        c.out = false;
        c.death = Some(cause);
        c.mind.asleep = false;
        c.mind.path.clear();
        c.mind.goal = Goal::Idle(u64::MAX);
        if let (Some(k), Some(t)) = (sp.corpse, things.get_mut(c.thing)) {
            t.kind = k;
            t.color = None;
        }
        events.push(LifeEvent::Died { who: c.thing, cause });
    }

    /// The attack a creature would use now, if it has one it can make.
    pub fn choose_attack(&mut self, who: ThingId) -> Option<Attack> {
        let c = self.get(who)?;
        if let Some(w) = &c.weapon {
            if c.caps.grasp > 0.2 {
                return Some(w.clone());
            }
        }
        let usable: Vec<Attack> = self
            .species_of(c)
            .attacks
            .iter()
            .filter(|a| {
                if a.uses.has(Does::GRASP) {
                    c.caps.grasp > 0.2
                } else if a.uses.has(Does::STAND) {
                    c.caps.moving > 0.3
                } else if a.uses.has(Does::BITE) {
                    c.caps.biting > 0.3
                } else {
                    true
                }
            })
            .cloned()
            .collect();
        if usable.is_empty() {
            return None;
        }
        let k = self.rng.below(usable.len() as u32) as usize;
        Some(usable[k].clone())
    }

    /// `attacker` attacks `target` (who should be next to it). Returns the
    /// injury if it hit. The target wakes, bears a grudge, and its friends
    /// nearby take its side.
    pub fn attack(&mut self, things: &Things, attacker: ThingId, target: ThingId) -> Option<Injury> {
        let (&ai, &ti) = (self.index.get(&attacker)?, self.index.get(&target)?);
        if ai == ti || !self.creatures[ai].alive || !self.creatures[ti].alive {
            return None;
        }
        let attack = self.choose_attack(attacker)?;
        let a_pos = things.get(attacker)?.pos();
        let t_pos = things.get(target)?.pos();
        let (a_caps, a_mass) = (self.creatures[ai].caps, self.species_of(&self.creatures[ai]).mass);

        let Life { species, creatures, events, rng, .. } = self;
        let t = &mut creatures[ti];
        let sp = &species[t.species as usize];
        let down = t.out || t.mind.asleep;
        let chance = if down { 0.97 } else { (0.85 - 0.3 * t.caps.moving + 0.15 * a_caps.moving).clamp(0.25, 0.95) };
        let hit = rng.chance(chance);
        t.mind.asleep = false;
        let injury = if hit {
            // Small things can only reach the legs of big ones.
            let ratio = a_mass / sp.mass;
            let reach = if down || ratio >= 0.4 {
                Height::High
            } else if ratio >= 0.12 {
                Height::Mid
            } else {
                Height::Low
            };
            let part = t.body.pick_part(&sp.plan, reach, rng);
            part.map(|part| {
                let strength = if a_caps.conscious > 0.0 { 0.5 + 0.5 * a_caps.conscious.min(1.0) } else { 0.5 };
                let force = attack.force * strength * (0.75 + 0.5 * rng.f32());
                t.body.strike(&sp.plan, part, attack.kind, force, rng)
            })
        } else {
            None
        };
        let sting = injury.as_ref().and_then(|i| i.worst()).map_or(0.0, |w| w.3);
        t.mind.resent(attacker, if hit { 0.6 + sting } else { 0.3 }, a_pos);
        let faction = t.mind.faction;
        // Hitting back at someone who started it doesn't make enemies of
        // their friends.
        let retaliating = creatures[ai].mind.anger_at(target) > 0.1 && creatures[ai].mind.control == Control::Ai;
        events.push(LifeEvent::Struck { attacker, target, attack, injury: injury.clone() });

        // Friends close by come to help.
        for (j, other) in creatures.iter_mut().enumerate() {
            if retaliating || j == ti || j == ai || !other.alive || other.mind.faction != faction || other.mind.control != Control::Ai {
                continue;
            }
            let loyalty = species[other.species as usize].temperament.loyalty;
            if loyalty <= 0.0 {
                continue;
            }
            if let Some(p) = things.get(other.thing).map(|t| t.pos()) {
                if p.z == t_pos.z && p.dist2d(t_pos) <= 12 {
                    other.mind.resent(attacker, loyalty * 0.8, a_pos);
                }
            }
        }
        injury
    }

    /// An explosion at `at`: creatures in reach are thrown about and burned.
    pub fn blast(&mut self, world: &World, things: &Things, at: Pos, b: &Blast, owner: Option<ThingId>) {
        let Life { species, creatures, events, rng, .. } = self;
        for c in creatures.iter_mut().filter(|c| c.alive) {
            let Some(p) = things.get(c.thing).map(|t| t.pos()) else { continue };
            let d = (((p.x - at.x).pow(2) + (p.y - at.y).pow(2) + (p.z - at.z).pow(2)) as f32).sqrt();
            if d > b.radius {
                continue;
            }
            let fall = 1.0 - d / (b.radius + 1.0);
            let full = b.power * fall;
            let f = full - shielding(world, at, p);
            if f <= 0.0 {
                continue;
            }
            let plan = &species[c.species as usize].plan;
            let mut injuries = Vec::new();
            let blows = 1 + (f / 40.0) as usize;
            for _ in 0..blows.min(4) {
                if let Some(part) = c.body.pick_part(plan, Height::High, rng) {
                    injuries.push(c.body.strike(plan, part, DamageKind::Blunt, f * 0.35, rng));
                }
            }
            let heat = b.heat * fall * (f / full) / 12.0;
            for _ in 0..2 {
                if let Some(part) = c.body.pick_part(plan, Height::High, rng) {
                    injuries.push(c.body.strike(plan, part, DamageKind::Burn, heat, rng));
                }
            }
            c.mind.asleep = false;
            c.mind.alarmed = true;
            if let Some(o) = owner {
                if o != c.thing {
                    c.mind.resent(o, 1.0, things.get(o).map(|t| t.pos()).unwrap_or(at));
                }
            }
            events.push(LifeEvent::Blasted { who: c.thing, injuries });
        }
    }

    /// Eat a food thing in reach. Returns true if it did.
    pub fn eat(&mut self, things: &mut Things, who: ThingId, food: ThingId) -> bool {
        let (Some(c), Some(f)) = (self.get(who), things.get(food)) else { return false };
        let Some(meal) = things.kind_of(f).food else { return false };
        let diet = self.species_of(c).diet;
        let (Some(me), fp) = (things.get(who).map(|t| t.pos()), f.pos()) else { return false };
        if (meal.meat && !diet.meat) || (!meal.meat && !diet.plants) || fp.z != me.z || fp.dist2d(me) > 1 {
            return false;
        }
        let what = if meal.stocked { format!("food from the {}", things.kind_of(f).name) } else { things.kind_of(f).name.clone() };
        if !meal.stocked {
            things.despawn(food);
        }
        let c = self.get_mut(who).unwrap();
        c.needs.hunger = (c.needs.hunger - meal.nutrition).max(0.0);
        self.events.push(LifeEvent::Ate { who, what });
        true
    }

    /// Edible things a creature could eat right where it is.
    pub fn food_in_reach(&self, things: &Things, who: ThingId) -> Option<ThingId> {
        let c = self.get(who)?;
        let diet = self.species_of(c).diet;
        let me = things.get(who)?.pos();
        for dy in -1..=1 {
            for dx in -1..=1 {
                for (id, t) in things.at(me.offset(dx, dy, 0)) {
                    if let Some(f) = things.kind_of(t).food {
                        if (f.meat && diet.meat) || (!f.meat && diet.plants) {
                            return Some(id);
                        }
                    }
                }
            }
        }
        None
    }

    /// Drinkable liquid next to (or just below) `p`, if any.
    pub fn water_near(&self, world: &World, p: Pos) -> Option<Pos> {
        for dz in [0, -1] {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let q = p.offset(dx, dy, dz);
                    if let Some(i) = world.index(q) {
                        if world.fluid[i] >= 2 && self.drinkable.contains(&world.fluid_mat[i]) {
                            return Some(q);
                        }
                    }
                }
            }
        }
        None
    }

    /// Take a drink from the liquid at `water`. Returns true if it did.
    pub fn drink(&mut self, world: &mut World, things: &Things, who: ThingId, water: Pos) -> bool {
        let Some(me) = things.get(who).map(|t| t.pos()) else { return false };
        if self.water_near(world, me) != Some(water) && !(water.dist2d(me) <= 1 && (me.z - water.z == 0 || me.z - water.z == 1)) {
            return false;
        }
        let Some(i) = world.index(water) else { return false };
        if world.fluid[i] < 2 || !self.drinkable.contains(&world.fluid_mat[i]) {
            return false;
        }
        let Some(c) = self.get_mut(who) else { return false };
        c.needs.thirst = (c.needs.thirst - 0.35).max(0.0);
        let mass = self.species_of(self.get(who).unwrap()).mass;
        self.drunk_ml += 500.0 * mass / 70.0;
        while self.drunk_ml >= ML_PER_UNIT {
            world.take_fluid(water, 1);
            self.drunk_ml -= ML_PER_UNIT;
        }
        self.events.push(LifeEvent::Drank(who));
        true
    }

    /// Lines for an inspection panel: name, state, blood and pain, what it
    /// can do, how it feels, and each hurt part (coloured by its worst wound).
    pub fn describe(&self, things: &Things, id: ThingId) -> Vec<(String, Rgb)> {
        let Some(c) = self.get(id) else { return Vec::new() };
        let sp = self.species_of(c);
        let txt = Rgb(200, 200, 205);
        let dim = Rgb(140, 140, 150);
        let mut out = Vec::new();
        let name = things.get(id).and_then(|t| t.name.clone());
        out.push((
            match name {
                Some(n) => format!("{n}, {}", sp.name),
                None => sp.name.clone(),
            },
            Rgb(240, 200, 90),
        ));
        let state = if !c.alive {
            format!("DEAD: {}", c.death.map_or("dead".into(), |d| d.describe(&sp.plan)))
        } else if c.out {
            "unconscious".into()
        } else if c.mind.asleep {
            "asleep".into()
        } else if c.mind.control == Control::Player {
            "awake".into()
        } else {
            self.doing(things, c)
        };
        out.push((state, if c.alive { txt } else { Rgb(220, 60, 60) }));
        let k = &c.caps;
        let bleed = c.body.bleeding();
        let mut l = format!("blood {:.0}%  pain {:.1}", k.blood * 100.0, k.pain);
        if bleed > 0.01 {
            l += &format!("  losing {bleed:.1} ml/s");
        }
        out.push((l, if k.blood < 0.7 || bleed > 1.0 { Rgb(230, 90, 80) } else { txt }));
        if c.alive {
            out.push((format!("moves {:.0}%  hands {:.0}%  sight {:.0}%", k.moving * 100.0, k.grasp * 100.0, k.sight * 100.0), dim));
            let words = c.needs.words();
            if !words.is_empty() {
                out.push((words.join(", "), Rgb(200, 180, 120)));
            }
        }
        let report = c.body.report(&sp.plan);
        if report.is_empty() {
            out.push(("unhurt".into(), dim));
        }
        out.extend(report);
        out
    }

    /// What a creature is up to, in a few words.
    pub fn doing(&self, things: &Things, c: &Creature) -> String {
        match &c.mind.goal {
            Goal::Idle(_) => "idling".into(),
            Goal::Go(_) => "walking".into(),
            Goal::Eat(_) => "going to eat".into(),
            Goal::Graze(_) => "grazing".into(),
            Goal::Drink { .. } => "going for a drink".into(),
            Goal::Sleep(_) => "going to bed".into(),
            Goal::Attack(t) => format!("attacking {}", self.name(things, *t)),
            Goal::Flee(t) => format!("fleeing from {}", self.name(things, *t)),
            Goal::Escape => "escaping the fire".into(),
        }
    }

    /// One sentence for an event.
    pub fn narrate(&self, things: &Things, e: &LifeEvent) -> String {
        let n = |id: ThingId| self.name(things, id);
        let you = |id: ThingId| self.is_player(id);
        // Third person or second: "Agnes falls", "you fall".
        let v = |id: ThingId, they: &str, me: &str| if you(id) { me.to_string() } else { they.to_string() };
        let part_name = |id: ThingId, part: usize| self.get(id).map_or(String::new(), |c| self.species_of(c).plan.parts[part].name.clone());
        let effect = |id: ThingId, inj: &Injury| -> String {
            if let Some(p) = inj.severed {
                return format!(", cutting the {} clean off!", part_name(id, p));
            }
            let Some((_, wk, tissue, sev)) = inj.worst() else { return ", but it does no harm".into() };
            let organ = inj.organ.filter(|&o| inj.wounds.iter().any(|w| w.0 == o));
            let worst_in_organ = organ.is_some_and(|o| inj.worst().is_some_and(|w| w.0 == o));
            let mut s = match (wk, tissue) {
                (WoundKind::Bruise, Tissue::Organ) if worst_in_organ => format!(", bruising the {}", part_name(id, organ.unwrap())),
                (WoundKind::Cut | WoundKind::Puncture, Tissue::Organ) if worst_in_organ => format!(", piercing the {}", part_name(id, organ.unwrap())),
                (WoundKind::Bruise, Tissue::Skin) => ", bruising it".to_string(),
                (WoundKind::Bruise, Tissue::Organ) => ", bruising what is inside".to_string(),
                (WoundKind::Bruise, _) => ", bruising the muscle".to_string(),
                (WoundKind::Cut, t) => format!(", cutting into the {}", t.name()),
                (WoundKind::Puncture, t) => format!(", piercing the {}", t.name()),
                (WoundKind::Fracture, _) if sev < 0.45 => ", cracking the bone".to_string(),
                (WoundKind::Fracture, _) => ", breaking the bone".to_string(),
                (WoundKind::Burn, Tissue::Fur) => ", singeing the fur".to_string(),
                (WoundKind::Burn, _) => ", burning it".to_string(),
            };
            if let (Some(o), false) = (organ, worst_in_organ) {
                s += &format!(" and the {}", part_name(id, o));
            }
            s + "."
        };
        let s = match e {
            LifeEvent::Struck { attacker, target, attack, injury } => {
                let verb = if you(*attacker) { attack.name.clone() } else { attack.verb.clone() };
                match injury {
                    None => format!("{} {verb} at {} but {}.", n(*attacker), n(*target), v(*attacker, "misses", "miss")),
                    Some(inj) => format!("{} {verb} {} in the {}{}", n(*attacker), n(*target), part_name(*target, inj.part), effect(*target, inj)),
                }
            }
            LifeEvent::Blasted { who, injuries } => {
                let worst = injuries.iter().filter(|i| !i.wounds.is_empty()).max_by(|a, b| a.worst().map_or(0.0, |w| w.3).total_cmp(&b.worst().map_or(0.0, |w| w.3)));
                match worst {
                    Some(inj) => format!("{} {} caught in the blast{}", n(*who), v(*who, "is", "are"), effect(*who, inj)),
                    None => format!("{} {} caught in the blast, unhurt.", n(*who), v(*who, "is", "are")),
                }
            }
            LifeEvent::Fell { who, levels, injury } => {
                let base = format!("{} {} {} levels", n(*who), v(*who, "falls", "fall"), levels);
                match injury {
                    Some(inj) if !inj.wounds.is_empty() => {
                        let (p, ..) = inj.worst().unwrap();
                        format!("{base} and {} hard on the {}{}", v(*who, "lands", "land"), part_name(*who, p), effect(*who, &Injury { part: p, ..inj.clone() }))
                    }
                    _ => format!("{base}."),
                }
            }
            LifeEvent::Burned { who, .. } => format!("{} {} burning!", n(*who), v(*who, "is", "are")),
            LifeEvent::Collapsed(w) => format!("{} {}.", n(*w), v(*w, "collapses", "black out")),
            LifeEvent::CameTo(w) => format!("{} {} to.", n(*w), v(*w, "comes", "come")),
            LifeEvent::Drowning(w) => format!("{} {} drowning!", n(*w), v(*w, "is", "are")),
            LifeEvent::Died { who, cause } => {
                let why = self.get(*who).map_or(String::new(), |c| cause.describe(&self.species_of(c).plan));
                format!("{} {} ({why}).", n(*who), v(*who, "dies", "die"))
            }
            LifeEvent::Ate { who, what } => format!("{} {} the {what}.", n(*who), v(*who, "eats", "eat")),
            LifeEvent::Drank(w) => format!("{} {}.", n(*w), v(*w, "drinks", "drink")),
            LifeEvent::FellAsleep(w) => format!("{} {} asleep.", n(*w), v(*w, "falls", "fall")),
            LifeEvent::Woke(w) => format!("{} {} up.", n(*w), v(*w, "wakes", "wake")),
        };
        capitalise(&s)
    }
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}
