//! Skills on creatures: registry, per-creature proficiency, clamping.

use tachyon::life::species;
use tachyon::prelude::*;

/// A Life with one species and a way to add creatures to it.
fn life_with_people() -> (Life, Things, KindId, SpeciesId) {
    let mut life = Life::new(1);
    let human = life.add_species(species::human());
    let mut things = Things::new();
    let person = things.add_kind(ThingKind::new("person", '@', Rgb::WHITE, Layer::Creature).blocking());
    (life, things, person, human)
}

fn add_person(life: &mut Life, things: &mut Things, kind: KindId, sp: SpeciesId, x: i32) -> ThingId {
    let id = things.spawn(kind, Pos::new(x, 0, 1));
    life.add(id, sp, Control::Ai, 1);
    id
}

#[test]
fn the_engine_ships_no_skills() {
    let (mut life, mut things, person, human) = life_with_people();
    assert!(life.skills.is_empty());
    let a = add_person(&mut life, &mut things, person, human, 0);
    assert!(life.get(a).unwrap().skills.is_empty());
}

#[test]
fn a_creature_added_before_a_skill_is_registered_gets_zero_for_it() {
    let (mut life, mut things, person, human) = life_with_people();
    let early = add_person(&mut life, &mut things, person, human, 0);
    let cooking = life.add_skill(SkillDef::new("cooking"));
    let smithing = life.add_skill(SkillDef::new("smithing"));
    // The early creature grew a slot for each skill, both untrained.
    assert_eq!(life.get(early).unwrap().skills.len(), 2);
    assert_eq!(life.get(early).unwrap().skill(cooking), 0.0);
    assert_eq!(life.get(early).unwrap().skill(smithing), 0.0);
    // One added afterwards starts the same way, and the vectors stay in step.
    let late = add_person(&mut life, &mut things, person, human, 1);
    assert_eq!(life.get(late).unwrap().skills, vec![0.0, 0.0]);
    life.get_mut(late).unwrap().set_skill(smithing, 0.7);
    assert_eq!(life.get(late).unwrap().skill(smithing), 0.7);
    assert_eq!(life.get(early).unwrap().skill(smithing), 0.0);
}

#[test]
fn set_skill_clamps_to_zero_and_one() {
    let (mut life, mut things, person, human) = life_with_people();
    let a = add_person(&mut life, &mut things, person, human, 0);
    let s = life.add_skill(SkillDef::new("sailing"));
    let c = life.get_mut(a).unwrap();
    assert!(c.set_skill(s, 1.5));
    assert_eq!(c.skill(s), 1.0);
    c.set_skill(s, -0.2);
    assert_eq!(c.skill(s), 0.0);
    c.set_skill(s, 0.45);
    assert_eq!(c.skill(s), 0.45);
    c.set_skill(s, f32::NAN);
    assert_eq!(c.skill(s), 0.0);
}

#[test]
fn two_registries_do_not_mix_ids() {
    let (mut one, mut things1, person1, human1) = life_with_people();
    let (mut two, mut things2, person2, human2) = life_with_people();
    let cooking = one.add_skill(SkillDef::new("cooking"));
    let smithing = one.add_skill(SkillDef::new("smithing"));
    let singing = two.add_skill(SkillDef::new("singing"));
    // Both registries start counting at 0.
    assert_eq!(cooking, singing);
    assert_eq!(one.skill_def(cooking).unwrap().name, "cooking");
    assert_eq!(two.skill_def(singing).unwrap().name, "singing");

    let a = add_person(&mut one, &mut things1, person1, human1, 0);
    let b = add_person(&mut two, &mut things2, person2, human2, 0);
    one.get_mut(a).unwrap().set_skill(smithing, 0.9);
    one.get_mut(a).unwrap().set_skill(cooking, 0.4);

    // The second registry has only one skill: the first's id 1 is unknown to it.
    let cb = two.get_mut(b).unwrap();
    assert_eq!(cb.skill(smithing), 0.0);
    assert!(!cb.set_skill(smithing, 1.0));
    assert_eq!(cb.skills.len(), 1);
    // Nothing set in the first leaked into the second, even on the shared id 0.
    assert_eq!(cb.skill(singing), 0.0);
    assert_eq!(one.get(a).unwrap().skill(cooking), 0.4);
}

#[test]
fn defaults_and_lookup() {
    let mut life = Life::new(1);
    let d = SkillDef::new("surgery");
    assert_eq!((d.spread, d.learn_rate), (1.0, 1.0));
    let id = life.add_skill(SkillDef { spread: 2.0, ..d });
    assert_eq!(life.find_skill("surgery"), Some(id));
    assert_eq!(life.find_skill("juggling"), None);
    assert_eq!(life.skill_def(id).unwrap().spread, 2.0);
    assert!(life.skill_def(SkillId(9)).is_none());
}
