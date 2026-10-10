//! Deciding and doing. Each creature with a mind, when it is its turn to
//! act, may reconsider its goal and then takes one step toward it.
//!
//! Reconsidering goes in order of urgency:
//!
//! 1. Danger: standing in fire (get out), or a threat in sight: someone it
//!    has a grudge against, a predator, something it is wary of. It fights
//!    if its nerve (courage, aggression, anger, health) is up to the danger
//!    (the threat's strength over its own), or if it is cornered; otherwise
//!    it flees.
//! 2. Hunting: hungry meat-eaters go after prey they can see.
//! 3. Needs: thirst, hunger and tiredness, scored by how pressing they are.
//! 4. The routine for this hour (work, wander, stay, sleep).
//! 5. Idling and wandering near home.
//!
//! Urgent things (1 and 2) are checked every few ticks; the rest when the
//! current goal is finished or every few dozen ticks. A creature keeps its
//! goal if the new choice is the same kind of thing, so it doesn't keep
//! changing its mind about where to wander.

use super::mind::{Control, Goal, Task};
use super::path;
use super::{Life, LifeEvent};
use crate::geom::{Pos, DIRS8};
use crate::movement::{self, Step};
use crate::things::{ThingId, Things};
use crate::world::World;
use std::collections::HashSet;
use std::mem::discriminant;

/// How far creatures notice each other, in cells.
pub const SIGHT: i32 = 12;
/// Ticks between checks for danger.
const URGENT_EVERY: u64 = 6;
/// Ticks between full reconsiderations.
const PLAN_EVERY: u64 = 60;
/// How far creatures look for food and water.
const FORAGE: usize = 3000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Progress {
    Continue,
    Done,
    Failed,
}

/// Another creature in sight.
#[derive(Clone, Copy, Debug)]
struct Seen {
    idx: usize,
    id: ThingId,
    pos: Pos,
    dist: i32,
}

/// Can `a` see `b`? Same level, nothing opaque on the line between.
pub fn sees(world: &World, things: &Things, a: Pos, b: Pos) -> bool {
    if a.z != b.z {
        return a.dist2d(b) <= 1 && (a.z - b.z).abs() <= 1;
    }
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let n = dx.abs().max(dy.abs());
    for s in 1..n {
        let x = a.x + (dx * s + if dx >= 0 { n / 2 } else { -(n / 2) }) / n;
        let y = a.y + (dy * s + if dy >= 0 { n / 2 } else { -(n / 2) }) / n;
        let p = Pos::new(x, y, a.z);
        match world.index(p) {
            Some(i) if !world.opaque_idx(i) && !things.blocks_sight(p) => {}
            _ => return false,
        }
    }
    true
}

/// How dangerous a creature is in a fight: size, its best blow, and how fit it is.
fn strength(life: &Life, idx: usize) -> f32 {
    let c = &life.creatures[idx];
    let sp = life.species_of(c);
    let blow = c.weapon.as_ref().map(|w| w.force).into_iter().chain(sp.attacks.iter().map(|a| a.force)).fold(1.0, f32::max);
    sp.mass.cbrt() * blow * (0.3 + 0.7 * c.caps.moving) * c.caps.conscious.clamp(0.1, 1.0)
}

/// Is it time to sleep, by its routine or (without one) by its nature?
fn bedtime(life: &Life, idx: usize, hour: f32) -> bool {
    let c = &life.creatures[idx];
    match c.mind.routine.now(hour) {
        Some(t) => matches!(t, Task::Sleep { .. }),
        None if life.species_of(c).nocturnal => (8.0..17.0).contains(&hour),
        None => !(6.0..22.0).contains(&hour),
    }
}

pub(super) fn act(life: &mut Life, i: usize, world: &mut World, things: &mut Things, tick: u64, hour: f32) {
    {
        let c = &life.creatures[i];
        if !c.alive || c.out || c.mind.control != Control::Ai {
            return;
        }
    }
    let me = life.creatures[i].thing;
    let Some(pos) = things.get(me).map(|t| t.pos()) else { return };

    // Asleep: wake when disturbed, when hunger or thirst gnaw, or when
    // rested and it is no longer time for bed.
    if life.creatures[i].mind.asleep {
        let bed = bedtime(life, i, hour);
        let c = &mut life.creatures[i];
        let n = &c.needs;
        // (Hunger and thirst only wake it if it might find something.)
        let hungry = n.hunger > 0.85 && tick >= c.mind.no_food_until;
        let thirsty = n.thirst > 0.85 && tick >= c.mind.no_water_until;
        let hot = path::hazard(world, pos) || n.temp > 38.5;
        let wake = c.mind.alarmed || hungry || thirsty || hot || (!bed && n.fatigue < 0.35);
        if !wake {
            return;
        }
        c.mind.asleep = false;
        c.mind.next_think = 0;
        life.events.push(LifeEvent::Woke(me));
    }
    if tick < life.creatures[i].mind.next_act {
        return;
    }

    let seen = look_around(life, i, world, things, pos);
    // Keep track of where the ones we resent are.
    for s in &seen {
        if let Some(g) = life.creatures[i].mind.grudges.iter_mut().find(|g| g.who == s.id) {
            g.seen = s.pos;
        }
    }

    let m = &life.creatures[i].mind;
    let replan = m.alarmed || tick >= m.next_think;
    let full = tick >= m.next_plan;
    if replan {
        life.creatures[i].mind.next_think = tick + URGENT_EVERY;
        life.creatures[i].mind.alarmed = false;
        match urgent(life, i, world, things, pos, &seen) {
            Some(g) => set_goal(life, i, g, false),
            // The danger has passed.
            None if matches!(life.creatures[i].mind.goal, Goal::Flee(_) | Goal::Escape | Goal::Attack(_)) => {
                let g = plan(life, i, world, things, pos, &seen, tick, hour);
                set_goal(life, i, g, true);
            }
            None => {}
        }
    }
    if full {
        life.creatures[i].mind.next_plan = tick + PLAN_EVERY;
        let g = plan(life, i, world, things, pos, &seen, tick, hour);
        set_goal(life, i, g, false);
    }

    let mut progress = pursue(life, i, world, things, pos, tick);
    if progress != Progress::Continue {
        let g = plan(life, i, world, things, pos, &seen, tick, hour);
        set_goal(life, i, g, true);
        // Start on the new goal now unless it is just to stand about.
        if !matches!(life.creatures[i].mind.goal, Goal::Idle(_)) {
            let pos = things.get(me).map(|t| t.pos()).unwrap_or(pos);
            progress = pursue(life, i, world, things, pos, tick);
        }
    }
    let _ = progress;

    // When to act next: running is twice as fast as walking; wounds slow it.
    let c = &life.creatures[i];
    let walk = life.species_of(c).walk;
    let pace = match c.mind.goal {
        Goal::Attack(_) | Goal::Flee(_) | Goal::Escape => 0.5,
        _ => 1.0,
    };
    let ticks = (walk * pace / c.caps.moving.max(0.15)).ceil().max(1.0) as u64;
    life.creatures[i].mind.next_act = tick + ticks;
}

fn set_goal(life: &mut Life, i: usize, g: Goal, force: bool) {
    let m = &mut life.creatures[i].mind;
    if !force && discriminant(&g) == discriminant(&m.goal) {
        return;
    }
    if m.goal != g {
        m.goal = g;
        m.path.clear();
        m.stuck = 0;
    }
}

fn look_around(life: &Life, i: usize, world: &World, things: &Things, pos: Pos) -> Vec<Seen> {
    let mut out = Vec::new();
    if life.creatures[i].caps.sight <= 0.0 {
        return out;
    }
    for (j, o) in life.creatures.iter().enumerate() {
        if j == i || !o.alive {
            continue;
        }
        let Some(p) = things.get(o.thing).map(|t| t.pos()) else { continue };
        let dist = p.dist2d(pos);
        if dist <= SIGHT && (p.z - pos.z).abs() <= 1 && sees(world, things, pos, p) {
            out.push(Seen { idx: j, id: o.thing, pos: p, dist });
        }
    }
    out.sort_by_key(|s| s.dist);
    out
}

/// Danger and hunting: the goals that can't wait.
fn urgent(life: &mut Life, i: usize, world: &World, _things: &Things, pos: Pos, seen: &[Seen]) -> Option<Goal> {
    // Get out of the fire, or out of the heat when overheating.
    let c = &life.creatures[i];
    let sp = life.species_of(c);
    if let Some(ix) = world.index(pos) {
        if path::hazard(world, pos) || (c.needs.temp > 38.5 && world.temp[ix] > sp.needs.comfort.1) {
            return Some(Goal::Escape);
        }
    }
    let my_name = sp.name.clone();

    // Threats: grudges, predators, things it is wary of. Unconscious ones
    // are no threat.
    let mut danger = 0.0;
    let mut nearest: Option<(Seen, f32)> = None;
    for s in seen {
        let o = &life.creatures[s.idx];
        if o.out {
            continue;
        }
        let osp = life.species_of(o);
        let anger = c.mind.anger_at(s.id);
        let hunted = osp.prey.iter().any(|p| *p == my_name) && s.dist <= 8 && !o.mind.asleep;
        let wary = sp.wary_of.iter().any(|w| *w == osp.name) && s.dist <= 5;
        if anger > 0.1 || hunted || wary {
            danger += strength(life, s.idx);
            if nearest.is_none() {
                nearest = Some((*s, anger));
            }
        }
    }
    if let Some((th, anger)) = nearest {
        let mine = strength(life, i).max(0.1);
        let t = sp.temperament;
        let health = c.caps.blood.min(1.0) * c.caps.moving.max(0.2);
        let nerve = t.courage + 0.5 * t.aggression + 0.5 * anger.min(2.0) + 0.3 * health - 0.05 * c.caps.pain - 0.6 * (1.0 - health);
        let odds = danger / mine;
        // Only an angry creature starts a fight; one that is merely wary or
        // hunted keeps its distance.
        return Some(if anger > 0.1 && nerve >= 0.5 * odds { Goal::Attack(th.id) } else { Goal::Flee(th.id) });
    }

    // Hunting.
    if c.needs.hunger > 0.4 && sp.diet.meat && !sp.prey.is_empty() {
        for s in seen {
            let o = &life.creatures[s.idx];
            if sp.prey.iter().any(|p| *p == life.species_of(o).name) {
                let (id, at) = (s.id, s.pos);
                life.creatures[i].mind.resent(id, 0.5, at);
                return Some(Goal::Attack(id));
            }
        }
    }
    None
}

/// Choose what to do next, all things considered.
#[allow(clippy::too_many_arguments)]
fn plan(life: &mut Life, i: usize, world: &World, things: &Things, pos: Pos, seen: &[Seen], tick: u64, hour: f32) -> Goal {
    if let Some(g) = urgent(life, i, world, things, pos, seen) {
        return g;
    }
    let bed = bedtime(life, i, hour);
    let c = &life.creatures[i];
    let n = c.needs.clone();
    let routine = c.mind.routine.now(hour).cloned();
    let home = c.mind.home;

    // Score the needs; try the most pressing first.
    let mut options: Vec<(f32, u8)> = Vec::new();
    if n.thirst > 0.35 && tick >= c.mind.no_water_until {
        options.push((n.thirst * n.thirst * 2.0 + 0.1, 0));
    }
    if n.hunger > 0.35 && tick >= c.mind.no_food_until {
        options.push((n.hunger * n.hunger * 1.6 + 0.1, 1));
    }
    if bed || n.fatigue > 0.75 {
        options.push((if bed { 0.6 + n.fatigue } else { n.fatigue * 1.2 }, 2));
    }
    options.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (_, what) in options {
        match what {
            0 => match find_water(life, world, things, pos) {
                Some((stand, water)) => return Goal::Drink { stand, water },
                None => life.creatures[i].mind.no_water_until = tick + 300,
            },
            1 => match find_food(life, i, world, things, pos) {
                Some(g) => return g,
                None => life.creatures[i].mind.no_food_until = tick + 300,
            },
            _ => {
                let at = match &routine {
                    Some(Task::Sleep { at }) => *at,
                    _ => None,
                };
                return Goal::Sleep(at);
            }
        }
    }

    // The routine for this hour, or wandering near home.
    match routine {
        Some(Task::Work { at, radius }) => {
            if pos.z != at.z || pos.dist2d(at) > radius {
                Goal::Go(at)
            } else if life.rng.chance(0.6) {
                Goal::Idle(tick + life.rng.range(4, 25) as u64)
            } else {
                let to = pick_spot(world, things, at, radius, &mut life.rng);
                Goal::Go(to.unwrap_or(at))
            }
        }
        Some(Task::Wander { around: c, radius }) => {
            if life.rng.chance(0.4) {
                Goal::Idle(tick + life.rng.range(4, 30) as u64)
            } else {
                Goal::Go(pick_spot(world, things, c, radius, &mut life.rng).unwrap_or(c))
            }
        }
        Some(Task::Stay { at }) => {
            if pos == at {
                Goal::Idle(tick + 20)
            } else {
                Goal::Go(at)
            }
        }
        _ => {
            if life.rng.chance(0.6) {
                Goal::Idle(tick + life.rng.range(10, 60) as u64)
            } else {
                let c = home.unwrap_or(pos);
                Goal::Go(pick_spot(world, things, c, 8, &mut life.rng).unwrap_or(pos))
            }
        }
    }
}

/// A random cell a creature could stand on within `r` of `c`.
fn pick_spot(world: &World, things: &Things, c: Pos, r: i32, rng: &mut crate::rng::Rng) -> Option<Pos> {
    for _ in 0..12 {
        let p = c.offset(rng.range(-r, r + 1), rng.range(-r, r + 1), 0);
        let wet = world.index(p).is_some_and(|i| world.fluid[i] >= movement::SWIM_DEPTH);
        if movement::can_occupy(world, things, p) && !wet && !things.blocks_move(p) && !path::hazard(world, p) {
            return Some(p);
        }
    }
    None
}

fn find_water(life: &Life, world: &World, things: &Things, pos: Pos) -> Option<(Pos, Pos)> {
    let path = path::search(world, things, pos, FORAGE, |p| life.water_near(world, p).is_some(), |_| 0)?;
    let stand = path.first().copied().unwrap_or(pos);
    Some((stand, life.water_near(world, stand)?))
}

fn find_food(life: &Life, i: usize, world: &World, things: &Things, pos: Pos) -> Option<Goal> {
    let c = &life.creatures[i];
    let diet = life.species_of(c).diet;
    let mut spots: HashSet<Pos> = HashSet::new();
    for (_, t) in things.iter() {
        let p = t.pos();
        if p.z != pos.z || p.dist2d(pos) > 30 {
            continue;
        }
        if let Some(f) = things.kind_of(t).food {
            if (f.meat && diet.meat) || (!f.meat && diet.plants) {
                spots.insert(p);
            }
        }
    }
    if !spots.is_empty() {
        let near = |p: Pos| (-1..=1).any(|dy| (-1..=1).any(|dx| spots.contains(&p.offset(dx, dy, 0))));
        if let Some(path) = path::search(world, things, pos, FORAGE, near, |_| 0) {
            let stand = path.first().copied().unwrap_or(pos);
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let q = stand.offset(dx, dy, 0);
                    if let Some((id, _)) = things.at(q).find(|(_, t)| things.kind_of(t).food.is_some_and(|f| (f.meat && diet.meat) || (!f.meat && diet.plants))) {
                        return Some(Goal::Eat(id));
                    }
                }
            }
        }
    }
    if diet.grazes {
        let grass = |p: Pos| grazable(life, world, p);
        if let Some(path) = path::search(world, things, pos, FORAGE, grass, |_| 0) {
            return Some(Goal::Graze(path.first().copied().unwrap_or(pos)));
        }
    }
    None
}

/// A floor of something grazers eat (grass), here or as the ground below.
fn grazable(life: &Life, world: &World, p: Pos) -> bool {
    let Some(i) = world.index(p) else { return false };
    if world.tile[i] == crate::tile::Tile::Floor && life.grazing.contains(&world.mat[i]) {
        return true;
    }
    world.tile[i] == crate::tile::Tile::Open && p.z > 0 && {
        let b = i - world.layer();
        world.tile[b].is_wall() && life.grazing.contains(&world.mat[b])
    }
}

/// Take one step of the current goal.
fn pursue(life: &mut Life, i: usize, world: &mut World, things: &mut Things, pos: Pos, tick: u64) -> Progress {
    let me = life.creatures[i].thing;
    let goal = life.creatures[i].mind.goal.clone();
    match goal {
        Goal::Idle(until) => {
            if tick >= until {
                Progress::Done
            } else {
                Progress::Continue
            }
        }
        Goal::Go(to) => walk_to(life, i, world, things, pos, to, 0),
        Goal::Eat(food) => {
            let Some(fp) = things.get(food).map(|t| t.pos()) else { return Progress::Done };
            if fp.z == pos.z && fp.dist2d(pos) <= 1 {
                if life.eat(things, me, food) {
                    Progress::Done
                } else {
                    Progress::Failed
                }
            } else {
                walk_to(life, i, world, things, pos, fp, 1)
            }
        }
        Goal::Graze(at) => {
            if grazable(life, world, pos) {
                let c = &mut life.creatures[i];
                c.needs.hunger = (c.needs.hunger - 0.03).max(0.0);
                if c.needs.hunger < 0.08 {
                    Progress::Done
                } else {
                    Progress::Continue
                }
            } else {
                walk_to(life, i, world, things, pos, at, 0)
            }
        }
        Goal::Drink { stand, water } => {
            if let Some(w) = life.water_near(world, pos) {
                let _ = water;
                life.drink(world, things, me, w);
                if life.creatures[i].needs.thirst < 0.05 {
                    Progress::Done
                } else {
                    Progress::Continue
                }
            } else {
                walk_to(life, i, world, things, pos, stand, 0)
            }
        }
        Goal::Sleep(at) => {
            let arrived = match at {
                None => Progress::Done,
                Some(b) => {
                    let mut p = walk_to(life, i, world, things, pos, b, 0);
                    if things.get(me).is_some_and(|t| t.pos() == b) {
                        p = Progress::Done;
                    }
                    // Someone in the bed, or no way there: sleep where we are.
                    if p == Progress::Continue && pos.dist2d(b) <= 1 && life.creatures[i].mind.stuck > 2 {
                        Progress::Done
                    } else if p == Progress::Failed {
                        Progress::Done
                    } else {
                        p
                    }
                }
            };
            if arrived == Progress::Done {
                let c = &mut life.creatures[i];
                if !c.mind.asleep {
                    c.mind.asleep = true;
                    life.events.push(LifeEvent::FellAsleep(me));
                }
            }
            Progress::Continue
        }
        Goal::Attack(target) => {
            // A downed foe is left alone, unless it is prey being hunted.
            let hunting = life.get(target).is_some_and(|t| life.species_of(&life.creatures[i]).prey.contains(&life.species_of(t).name));
            let gone = life.get(target).map_or(true, |t| !t.alive || (t.out && !hunting));
            let Some(tp) = things.get(target).map(|t| t.pos()) else { return Progress::Done };
            if gone || tp.dist2d(pos) > SIGHT * 2 {
                return Progress::Done;
            }
            if tp.z == pos.z && tp.dist2d(pos) <= 1 {
                life.attack(things, me, target);
                Progress::Continue
            } else {
                walk_to(life, i, world, things, pos, tp, 1)
            }
        }
        Goal::Flee(from) => {
            let Some(fp) = things.get(from).map(|t| t.pos()) else { return Progress::Done };
            if fp.dist2d(pos) > SIGHT || fp.z != pos.z {
                return Progress::Done;
            }
            let d2 = |p: Pos| (p.x - fp.x).pow(2) + (p.y - fp.y).pow(2);
            let best = DIRS8
                .iter()
                .filter_map(|&(dx, dy)| movement::destination(world, things, pos, dx, dy).map(|q| (dx, dy, q)))
                .filter(|&(_, _, q)| !things.blocks_move(q))
                .max_by_key(|&(_, _, q)| d2(q));
            match best {
                Some((dx, dy, q)) if d2(q) > d2(pos) => {
                    movement::step(world, things, me, dx, dy);
                    Progress::Continue
                }
                _ => {
                    // Cornered: turn and fight.
                    let c = &mut life.creatures[i];
                    c.mind.resent(from, 0.5, fp);
                    c.mind.goal = Goal::Attack(from);
                    Progress::Continue
                }
            }
        }
        Goal::Escape => {
            // Head for the coolest cell around until it is comfortable again.
            let c = &life.creatures[i];
            let comfy = life.species_of(c).needs.comfort.1;
            let heat = |p: Pos| world.index(p).map_or(f32::MAX, |ix| if world.fire[ix] > 0 { 1000.0 } else { world.temp[ix] });
            if !path::hazard(world, pos) && (c.needs.temp <= 38.0 || heat(pos) <= comfy) {
                return Progress::Done;
            }
            let way = DIRS8
                .iter()
                .filter_map(|&(dx, dy)| movement::destination(world, things, pos, dx, dy).map(|q| (dx, dy, q)))
                .filter(|&(_, _, q)| !things.blocks_move(q))
                .min_by(|a, b| heat(a.2).total_cmp(&heat(b.2)));
            let (dx, dy) = match way {
                Some((dx, dy, q)) if heat(q) < heat(pos) => (dx, dy),
                _ => DIRS8[life.rng.below(8) as usize],
            };
            movement::step(world, things, me, dx, dy);
            Progress::Continue
        }
    }
}

/// One step along a route to within `near` of `to`, planning one if needed.
#[allow(clippy::too_many_arguments)]
fn walk_to(life: &mut Life, i: usize, world: &mut World, things: &mut Things, pos: Pos, to: Pos, near: i32) -> Progress {
    if pos.z == to.z && pos.dist2d(to) <= near {
        return Progress::Done;
    }
    let me = life.creatures[i].thing;
    let m = &mut life.creatures[i].mind;
    let fits = m.path.first().is_some_and(|e| e.z == to.z && e.dist2d(to) <= near) && m.path.last().is_some_and(|n| n.dist2d(pos) <= 1 && (n.z - pos.z).abs() <= 1);
    if !fits {
        match path::find(world, things, pos, to, near, path::DEFAULT_BUDGET) {
            Some(p) if !p.is_empty() => m.path = p,
            _ => return Progress::Failed,
        }
    }
    let next = *m.path.last().unwrap();
    let moved = if next.x == pos.x && next.y == pos.y {
        // Stairs.
        movement::climb(world, things, me, next.z > pos.z).is_some()
    } else {
        match movement::step(world, things, me, (next.x - pos.x).signum(), (next.y - pos.y).signum()) {
            Step::Moved(p) => {
                if p != next {
                    m.path.clear(); // landed somewhere else (a ramp); plan again
                }
                true
            }
            Step::OpenedDoor(_) => return Progress::Continue,
            Step::Blocked | Step::Ledge => false,
        }
    };
    if moved {
        if m.path.last() == Some(&next) {
            m.path.pop();
        }
        m.stuck = 0;
        Progress::Continue
    } else {
        m.stuck += 1;
        if m.stuck > 10 {
            m.stuck = 0;
            return Progress::Failed;
        }
        if m.stuck > 3 {
            // Someone in the way: try to step round them.
            m.path.clear();
            let (dx, dy) = DIRS8[life.rng.below(8) as usize];
            movement::step(world, things, me, dx, dy);
        }
        Progress::Continue
    }
}
