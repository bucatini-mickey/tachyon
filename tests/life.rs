//! Creatures: bodies and wounds, needs, and minds.

use tachyon::life::body::{bruise_color, Body, BodyPlan, Cause, DamageKind, Height, WoundKind};
use tachyon::life::mind::{Control, Goal, Routine, Task};
use tachyon::life::path;
use tachyon::life::species;
use tachyon::prelude::*;

/// Open ground: granite at z = 0, grass floor at z = 1, open sky above,
/// two seconds a tick.
fn field(w: i32, h: i32) -> Scene {
    let mats = Materials::standard();
    let (granite, grass) = (mats.id("granite"), mats.id("grass"));
    let mut world = World::new(w, h, 4, mats);
    for y in 0..h {
        for x in 0..w {
            world.set_tile(Pos::new(x, y, 0), Tile::Wall, granite);
            world.set_tile(Pos::new(x, y, 1), Tile::Floor, grass);
        }
    }
    let mut s = Scene::new(world, Things::new(), Clock::new(2.0, 12.0), 5);
    s.sim.fluids = false;
    let water = s.world.mats.id("water");
    s.life.drinkable.push(water);
    s.life.grazing.push(grass);
    s
}

struct Kinds {
    person: KindId,
    dog: KindId,
    rat: KindId,
    corpse: KindId,
    bread: KindId,
    bed: KindId,
    door: KindId,
}

fn kinds(s: &mut Scene) -> Kinds {
    let t = &mut s.things;
    Kinds {
        person: t.add_kind(ThingKind::new("person", '@', Rgb::WHITE, Layer::Creature).blocking()),
        dog: t.add_kind(ThingKind::new("dog", 'd', Rgb::WHITE, Layer::Creature).blocking()),
        rat: t.add_kind(ThingKind::new("rat", 'r', Rgb::WHITE, Layer::Creature)),
        corpse: t.add_kind(ThingKind::new("corpse", '%', Rgb::WHITE, Layer::Item).food(0.6, true)),
        bread: t.add_kind(ThingKind::new("bread", '%', Rgb::WHITE, Layer::Item).food(0.5, false)),
        bed: t.add_kind(ThingKind::new("bed", 'Θ', Rgb::WHITE, Layer::Furniture).bed()),
        door: t.add_kind(ThingKind::new("door", '+', Rgb::WHITE, Layer::Furniture).blocking().door('\'')),
    }
}

fn spawn(s: &mut Scene, kind: KindId, sp: SpeciesId, p: Pos) -> ThingId {
    let id = s.things.spawn(kind, p);
    s.life.add(id, sp, Control::Ai, 1);
    id
}

fn pos(s: &Scene, id: ThingId) -> Pos {
    s.things.get(id).unwrap().pos()
}

#[test]
fn a_punch_bruises_and_the_bruise_ages_and_heals() {
    let plan = BodyPlan::humanoid(70.0);
    let mut b = Body::new(&plan);
    let mut rng = Rng::new(1);
    let arm = plan.find("left forearm").unwrap();
    let inj = b.strike(&plan, arm, DamageKind::Blunt, 14.0, &mut rng);
    let (_, kind, tissue, _) = inj.worst().expect("a punch leaves a mark");
    assert_eq!((kind, tissue), (WoundKind::Bruise, Tissue::Skin));
    assert!(b.parts[arm].wounds.iter().all(|w| w.kind == WoundKind::Bruise), "a punch to the arm breaks nothing");
    assert_eq!(b.bleeding(), 0.0, "a bruise doesn't bleed outside");
    // Red when fresh, purple-blue after two days, yellow after a week.
    let (fresh, old) = (bruise_color(0.0), bruise_color(7.5 * 86400.0));
    assert!(fresh.0 > fresh.2 && old.1 > old.2, "fresh {fresh:?}, old {old:?}");
    for _ in 0..24 * 14 {
        b.step(&plan, 3600.0, 1.0);
    }
    assert!(b.parts[arm].wounds.is_empty(), "healed after two weeks");
}

#[test]
fn a_heavy_blow_breaks_bone_and_a_broken_leg_slows_you() {
    let plan = BodyPlan::humanoid(70.0);
    let mut b = Body::new(&plan);
    let mut rng = Rng::new(2);
    let fit = b.capacities(&plan).moving;
    let shin = plan.find("right shin").unwrap();
    b.strike(&plan, shin, DamageKind::Blunt, 90.0, &mut rng);
    assert!(b.parts[shin].wounds.iter().any(|w| w.kind == WoundKind::Fracture), "{:?}", b.parts[shin].wounds);
    let lame = b.capacities(&plan).moving;
    assert!(lame < fit * 0.75, "moving {fit} -> {lame}");
    // The foot below the break is little use either.
    let foot = plan.find("right foot").unwrap();
    assert!(b.effective(&plan, foot) < 0.7);
}

#[test]
fn blades_cut_layers_bleed_clot_and_can_sever() {
    let plan = BodyPlan::humanoid(70.0);
    let mut rng = Rng::new(3);
    // A knife slash: through the skin into the muscle; bleeds, then clots.
    let mut b = Body::new(&plan);
    let arm = plan.find("left upper arm").unwrap();
    let inj = b.strike(&plan, arm, DamageKind::Edge, 18.0, &mut rng);
    assert!(inj.wounds.iter().any(|w| w.2 == Tissue::Muscle), "{inj:?}");
    assert!(inj.severed.is_none());
    assert!(b.bleeding() > 0.3, "bleeding {}", b.bleeding());
    let mut lost = 0.0;
    for _ in 0..120 {
        lost += b.step(&plan, 30.0, 1.0);
    }
    assert!(b.bleeding() < 0.01, "clotted after an hour");
    assert!(lost > 50.0 && b.blood > 0.8 * b.max_blood, "lost {lost} ml");

    // An axe through the forearm takes the hand with it.
    let mut b = Body::new(&plan);
    let fa = plan.find("right forearm").unwrap();
    let inj = b.strike(&plan, fa, DamageKind::Edge, 200.0, &mut rng);
    assert_eq!(inj.severed, Some(fa));
    let hand = plan.find("right hand").unwrap();
    assert!(b.parts[hand].severed);
    assert!(b.capacities(&plan).grasp < 1.0 + 1e-6);

    // A blade through the neck kills.
    let mut b = Body::new(&plan);
    let neck = plan.find("neck").unwrap();
    b.strike(&plan, neck, DamageKind::Edge, 200.0, &mut rng);
    assert_eq!(b.fatal(&plan), Some(Cause::Severed(neck)));
}

#[test]
fn a_stab_to_the_heart_is_fatal() {
    let plan = BodyPlan::humanoid(70.0);
    let mut b = Body::new(&plan);
    let mut rng = Rng::new(4);
    let chest = plan.find("chest").unwrap();
    let heart = plan.find("heart").unwrap();
    // Strike until the blade finds the heart (other organs are in the way).
    for _ in 0..40 {
        let inj = b.strike(&plan, chest, DamageKind::Point, 80.0, &mut rng);
        if inj.organ == Some(heart) {
            break;
        }
    }
    let mut t = 0.0;
    while b.fatal(&plan).is_none() && t < 600.0 {
        b.step(&plan, 5.0, 1.0);
        t += 5.0;
    }
    assert!(matches!(b.fatal(&plan), Some(Cause::BledOut | Cause::Destroyed(_))), "{:?} after {t} s", b.fatal(&plan));
}

#[test]
fn small_creatures_reach_only_the_legs() {
    let plan = BodyPlan::humanoid(70.0);
    let b = Body::new(&plan);
    let mut rng = Rng::new(5);
    for _ in 0..200 {
        let p = b.pick_part(&plan, Height::Low, &mut rng).unwrap();
        assert_eq!(plan.parts[p].height, Height::Low, "{}", plan.parts[p].name);
    }
}

#[test]
fn bleeding_stains_the_floor_and_rain_washes_it() {
    let mut s = field(12, 12);
    let k = kinds(&mut s);
    let human = s.life.add_species(species::human());
    let who = s.things.spawn(k.person, Pos::new(5, 5, 1));
    s.life.add(who, human, Control::Player, 1);
    let mut rng = Rng::new(6);
    let c = s.life.get_mut(who).unwrap();
    let plan = BodyPlan::humanoid(70.0);
    let thigh = plan.find("left thigh").unwrap();
    c.body.strike(&plan, thigh, DamageKind::Edge, 40.0, &mut rng);
    for _ in 0..30 {
        s.tick();
    }
    let st = *s.life.stains.get(Pos::new(5, 5, 1)).expect("blood on the floor");
    assert!(st.ml > 20.0, "{st:?}");
    s.weather.rain = 1.0;
    s.life.get_mut(who).unwrap().body = Body::new(&plan); // stop the bleeding
    for _ in 0..400 {
        s.tick();
    }
    assert!(s.life.stains.get(Pos::new(5, 5, 1)).is_none(), "rain washed it away");
}

#[test]
fn path_goes_round_walls_and_through_doors() {
    let mut s = field(20, 10);
    let k = kinds(&mut s);
    let stone = s.world.mats.id("stone block");
    for y in 0..10 {
        if y != 7 {
            s.world.set_tile(Pos::new(10, y, 1), Tile::Wall, stone);
        }
    }
    s.things.spawn(k.door, Pos::new(10, 7, 1));
    let p = path::find(&s.world, &s.things, Pos::new(2, 2, 1), Pos::new(17, 2, 1), 0, 4000).expect("a way through the door");
    assert!(p.contains(&Pos::new(10, 7, 1)));
    assert_eq!(p.first(), Some(&Pos::new(17, 2, 1)));
}

#[test]
fn hungry_and_thirsty_creatures_find_food_and_water() {
    let mut s = field(30, 12);
    let k = kinds(&mut s);
    let human = s.life.add_species(species::human());
    let water = s.world.mats.id("water");
    let a = spawn(&mut s, k.person, human, Pos::new(3, 5, 1));
    let bread = s.things.spawn(k.bread, Pos::new(20, 9, 1));
    s.life.get_mut(a).unwrap().needs.hunger = 0.8;
    for _ in 0..80 {
        s.tick();
    }
    assert!(s.things.get(bread).is_none(), "the bread was eaten (hunger {})", s.life.get(a).unwrap().needs.hunger);
    assert!(s.life.get(a).unwrap().needs.hunger < 0.4);

    // A pond, out of reach of nothing.
    for (x, y) in [(25, 2), (26, 2), (25, 3), (26, 3)] {
        s.world.set_tile(Pos::new(x, y, 1), Tile::Open, AIR);
        s.world.set_fluid(Pos::new(x, y, 1), water, FULL);
    }
    s.life.get_mut(a).unwrap().needs.thirst = 0.9;
    for _ in 0..80 {
        s.tick();
    }
    let c = s.life.get(a).unwrap();
    assert!(c.needs.thirst < 0.2, "thirst {} at {:?}, doing {:?}", c.needs.thirst, pos(&s, a), c.mind.goal);
}

#[test]
fn a_routine_sends_people_to_work_and_to_bed() {
    let mut s = field(30, 12);
    let k = kinds(&mut s);
    let human = s.life.add_species(species::human());
    let bed = Pos::new(3, 3, 1);
    s.things.spawn(k.bed, bed);
    let a = spawn(&mut s, k.person, human, Pos::new(15, 6, 1));
    let shop = Pos::new(25, 8, 1);
    s.life.get_mut(a).unwrap().mind.routine = Routine::new().with(22.0, 6.0, Task::Sleep { at: Some(bed) }).with(8.0, 18.0, Task::Work { at: shop, radius: 2 });
    // Noon: at work.
    for _ in 0..60 {
        s.tick();
    }
    assert!(pos(&s, a).dist2d(shop) <= 2, "at work: {:?}", pos(&s, a));
    // Jump to 22:30: off to bed, and asleep.
    let now = s.clock.hour();
    s.clock.skip((22.5 - now) * 3600.0);
    s.life.get_mut(a).unwrap().mind.next_plan = 0;
    for _ in 0..100 {
        s.tick();
    }
    let c = s.life.get(a).unwrap();
    assert_eq!(pos(&s, a), bed, "in bed, doing {:?}", c.mind.goal);
    let ev: Vec<String> = s.life.events.iter().map(|e| s.life.narrate(&s.things, e)).collect();
    assert!(c.mind.asleep, "{:?} {:?} {}", c.mind.goal, ev, s.clock.label());
}

#[test]
fn attacked_creatures_fight_back_or_flee() {
    let mut s = field(30, 30);
    let k = kinds(&mut s);
    let human = s.life.add_species(species::human());
    let dog = s.life.add_species(species::dog());
    let rat = s.life.add_species(species::rat());
    let you = s.things.spawn(k.person, Pos::new(10, 10, 1));
    s.life.add(you, human, Control::Player, 0);

    // A dog you kick bites back.
    let d = spawn(&mut s, k.dog, dog, Pos::new(11, 10, 1));
    s.life.attack(&s.things, you, d);
    for _ in 0..6 {
        s.tick();
    }
    assert_eq!(s.life.get(d).unwrap().mind.goal, Goal::Attack(you));
    let bitten = s.life.events.iter().any(|e| matches!(e, LifeEvent::Struck { attacker, .. } if *attacker == d));
    assert!(bitten, "the dog should have attacked: {:?}", s.life.events);

    // A rat runs.
    let r = spawn(&mut s, k.rat, rat, Pos::new(20, 20, 1));
    s.things.move_to(you, Pos::new(19, 20, 1));
    let start = pos(&s, r).dist2d(pos(&s, you));
    for _ in 0..10 {
        s.tick();
    }
    let c = s.life.get(r).unwrap();
    assert!(matches!(c.mind.goal, Goal::Flee(_)) || pos(&s, r).dist2d(pos(&s, you)) > start + 2, "{:?}", c.mind.goal);
    assert!(pos(&s, r).dist2d(pos(&s, you)) > start);
}

#[test]
fn friends_defend_each_other() {
    let mut s = field(30, 30);
    let k = kinds(&mut s);
    let human = s.life.add_species(species::human());
    let dog = s.life.add_species(species::dog());
    let you = s.things.spawn(k.person, Pos::new(10, 10, 1));
    s.life.add(you, human, Control::Player, 0);
    let villager = spawn(&mut s, k.person, human, Pos::new(11, 10, 1));
    let d = spawn(&mut s, k.dog, dog, Pos::new(16, 14, 1));
    s.life.attack(&s.things, you, villager);
    let mut bit = false;
    for _ in 0..30 {
        s.tick();
        bit |= s.life.events.iter().any(|e| matches!(e, LifeEvent::Struck { attacker, target, .. } if *attacker == d && *target == you));
        assert!(!s.life.events.iter().any(|e| matches!(e, LifeEvent::Struck { attacker, target, .. } if *attacker == d && *target == villager)), "the dog turned on its friend");
        s.life.events.clear();
    }
    assert!(bit, "the dog came to its friend's defence");
}

#[test]
fn hunters_kill_prey_and_eat_it() {
    let mut s = field(20, 20);
    let k = kinds(&mut s);
    let mut cat = species::cat();
    let mut rat = species::rat();
    cat.corpse = Some(k.corpse);
    rat.corpse = Some(k.corpse);
    let (cat, rat) = (s.life.add_species(cat), s.life.add_species(rat));
    let c = spawn(&mut s, k.dog, cat, Pos::new(5, 5, 1));
    let r = spawn(&mut s, k.rat, rat, Pos::new(9, 7, 1));
    s.life.get_mut(c).unwrap().needs.hunger = 0.7;
    for _ in 0..200 {
        s.tick();
        if s.things.get(r).is_none() {
            break;
        }
    }
    assert!(s.things.get(r).is_none(), "rat caught and eaten; cat {:?}", s.life.get(c).unwrap().mind.goal);
    assert!(s.life.get(c).unwrap().needs.hunger < 0.4);
}

#[test]
fn creatures_drown_burn_and_fall() {
    let mut s = field(12, 12);
    let k = kinds(&mut s);
    let mut human = species::human();
    human.corpse = Some(k.corpse);
    let human = s.life.add_species(human);
    let water = s.world.mats.id("water");

    // Held under water: breath runs out, then life.
    let p = Pos::new(2, 2, 1);
    s.world.set_tile(p, Tile::Open, AIR);
    s.world.set_tile(p.up(), Tile::Floor, s.world.mats.id("stone block")); // a lid
    s.world.set_fluid(p, water, FULL);
    let a = s.things.spawn(k.person, p);
    s.life.add(a, human, Control::Player, 1);
    for _ in 0..120 {
        s.tick();
    }
    let c = s.life.get(a).unwrap();
    assert_eq!(c.death, Some(Cause::Drowned));
    assert_eq!(s.things.get(a).unwrap().kind, k.corpse);

    // In fire: burns, and an AI walks out of it.
    let q = Pos::new(8, 8, 1);
    let b = spawn(&mut s, k.person, human, q);
    s.world.ignite(q);
    s.tick();
    s.tick();
    s.tick();
    let c = s.life.get(b).unwrap();
    assert!(c.body.parts.iter().flat_map(|p| &p.wounds).any(|w| w.kind == WoundKind::Burn), "burned");
    assert_ne!(pos(&s, b), q, "got out of the fire");

    // The floor gives way under someone two levels up.
    let mut s = field(8, 8);
    let k = kinds(&mut s);
    let human = s.life.add_species(species::human());
    let top = Pos::new(4, 4, 3);
    let granite = s.world.mats.id("granite");
    s.world.set_tile(top, Tile::Floor, granite);
    s.world.set_tile(Pos::new(4, 4, 1), Tile::Open, AIR);
    let c = s.things.spawn(k.person, top);
    s.life.add(c, human, Control::Player, 1);
    s.world.set_tile(top, Tile::Open, AIR); // the floor gives way
    for _ in 0..6 {
        s.tick();
    }
    assert_eq!(pos(&s, c).z, 1);
    let fell = s.life.events.iter().find(|e| matches!(e, LifeEvent::Fell { .. })).cloned();
    assert!(matches!(fell, Some(LifeEvent::Fell { levels: 2.., injury: Some(_), .. })), "{fell:?}");
}

#[test]
fn needs_rise_with_game_time() {
    let mut s = field(8, 8);
    let k = kinds(&mut s);
    let human = s.life.add_species(species::human());
    let a = s.things.spawn(k.person, Pos::new(4, 4, 1));
    s.life.add(a, human, Control::Player, 1);
    s.tick();
    let before = s.life.get(a).unwrap().needs.clone();
    s.clock.skip(6.0 * 3600.0);
    s.tick();
    let after = &s.life.get(a).unwrap().needs;
    assert!(after.hunger > before.hunger + 0.2 && after.thirst > before.thirst + 0.3 && after.fatigue > before.fatigue + 0.2, "{before:?} -> {after:?}");
    let words = s.life.describe(&s.things, a);
    assert!(words.iter().any(|(l, _)| l.contains("hungry") || l.contains("peckish")), "{words:?}");
}
