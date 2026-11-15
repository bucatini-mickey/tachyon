//! Sound: loss through walls and doors, the noise floor, and creatures
//! hearing (ROADMAP section 5).

use tachyon::life::mind::Goal;
use tachyon::life::species;
use tachyon::prelude::*;
use tachyon::sound::{self, level, HUMAN_HEARING};

/// A `w` x 12 x 3 scene: granite ground at z = 0, open air above.
fn field(w: i32) -> Scene {
    let mats = Materials::standard();
    let granite = mats.id("granite");
    let mut world = World::new(w, 12, 3, mats);
    for y in 0..12 {
        for x in 0..w {
            world.set_tile(Pos::new(x, y, 0), Tile::Wall, granite);
        }
    }
    Scene::new(world, Things::new(), Clock::new(1.0, 12.0), 3)
}

/// A wall across the map at `x`, on the level people stand on.
fn wall_at(s: &mut Scene, x: i32, mat: &str) {
    let m = s.world.mats.id(mat);
    for y in 0..12 {
        s.world.set_tile(Pos::new(x, y, 1), Tile::Wall, m);
    }
}

fn shout_heard(s: &mut Scene, from: Pos, at: Pos) -> Option<f32> {
    s.sounds = vec![Sound::new(from, level::SHOUT, SoundKind::Speech(Voice::Shout))];
    s.hear(at, HUMAN_HEARING).first().map(|h| h.db_over)
}

#[test]
fn a_shout_is_heard_through_one_wooden_wall_but_not_four_stone_ones() {
    let (from, at) = (Pos::new(5, 5, 1), Pos::new(15, 5, 1));
    let mut s = field(24);
    wall_at(&mut s, 10, "wood");
    let over = shout_heard(&mut s, from, at).expect("a shout through one wooden wall was not heard at 10 m");
    // 80 dB - 20 (10 m) - 0.05 (air) - 22 (wood) = about 38, over a quiet 25.
    assert!((over - 12.95).abs() < 0.1, "heard {over} dB over the floor");

    let mut s = field(24);
    for x in [7, 9, 11, 13] {
        wall_at(&mut s, x, "stone block");
    }
    assert_eq!(shout_heard(&mut s, from, at), None, "a shout was heard through four stone walls");
}

#[test]
fn closing_a_door_between_two_rooms_drops_the_level_by_at_least_15_db() {
    let mut s = field(24);
    wall_at(&mut s, 10, "stone block");
    let gap = Pos::new(10, 5, 1);
    s.world.set_tile(gap, Tile::Open, AIR);
    let kind = s.things.add_kind(ThingKind::new("door", '+', Rgb::WHITE, Layer::Furniture).blocking().door('\''));
    let door = s.things.spawn(kind, gap);
    let (from, at) = (Pos::new(6, 5, 1), Pos::new(14, 5, 1));
    s.sounds = vec![Sound::new(from, level::SPEECH, SoundKind::Speech(Voice::Talk))];

    s.things.get_mut(door).unwrap().open = true;
    let open = s.hear(at, HUMAN_HEARING).first().map(|h| h.db_over).expect("talk through an open door not heard");
    s.things.get_mut(door).unwrap().open = false;
    let shut = s.hear(at, HUMAN_HEARING).first().map(|h| h.db_over).unwrap_or(f32::NEG_INFINITY);
    assert!(open - shut >= 15.0, "closing the door only took {} dB off", open - shut);
}

#[test]
fn rain_drowns_a_whisper_outdoors_but_not_indoors() {
    let mut s = field(32);
    // A stone hut with a slate roof, in the same chunk column as the field.
    let (stone, slate) = (s.world.mats.id("stone block"), s.world.mats.id("slate"));
    for y in 2..=8 {
        for x in 20..=28 {
            if x == 20 || x == 28 || y == 2 || y == 8 {
                s.world.set_tile(Pos::new(x, y, 1), Tile::Wall, stone);
            }
            s.world.set_tile(Pos::new(x, y, 2), Tile::Floor, slate);
        }
    }
    let whisper = |s: &mut Scene, from: Pos, at: Pos| {
        s.sounds = vec![Sound::new(from, level::WHISPER, SoundKind::Speech(Voice::Whisper))];
        !s.hear(at, HUMAN_HEARING).is_empty()
    };
    let (out_from, out_at) = (Pos::new(3, 5, 1), Pos::new(6, 5, 1));
    let (in_from, in_at) = (Pos::new(22, 5, 1), Pos::new(25, 5, 1));

    s.tick(); // works out the sky and the noise floor
    assert!(whisper(&mut s, out_from, out_at), "a whisper at 3 m was lost outdoors on a dry, still day");

    s.weather.rain = 1.0;
    s.tick();
    assert!(s.world.open_sky[s.world.index(out_at).unwrap()] && !s.world.open_sky[s.world.index(in_at).unwrap()]);
    let (outside, inside) = (s.noise.floor(&s.world, out_at), s.noise.floor(&s.world, in_at));
    assert!(outside > inside + 15.0, "rain lifted the floor to {outside} dB outside and {inside} dB inside");
    assert!(!whisper(&mut s, out_from, out_at), "a whisper at 3 m was heard outdoors in a downpour");
    assert!(whisper(&mut s, in_from, in_at), "a whisper at 3 m was lost indoors while it rained");
}

#[test]
fn a_fire_starting_is_heard_and_burning_lifts_the_floor() {
    let mut s = field(24);
    wall_at(&mut s, 12, "wood");
    let at = Pos::new(4, 5, 1);
    s.tick();
    let quiet = s.noise.floor(&s.world, at);
    assert!((quiet - sound::QUIET).abs() < 0.01, "a still, dry field has a floor of {quiet} dB");
    s.world.ignite(Pos::new(12, 5, 1));
    s.tick();
    assert!(s.sounds.iter().any(|x| x.kind == SoundKind::Fire), "a new fire made no sound");
    for _ in 0..20 {
        s.tick();
    }
    let burning = s.noise.floor(&s.world, at);
    assert!(burning >= quiet + 10.0, "a burning wall only lifted the floor to {burning} dB");
}

/// A stone room (x 4..=12, y 2..=8) with a door in its east wall at (12, 5)
/// opening on a walled corridor that runs east along y = 5.
fn room_and_corridor() -> (Scene, ThingId) {
    let mut s = field(40);
    let stone = s.world.mats.id("stone block");
    for y in 2..=8 {
        for x in 4..=12 {
            if x == 4 || x == 12 || y == 2 || y == 8 {
                s.world.set_tile(Pos::new(x, y, 1), Tile::Wall, stone);
            }
        }
    }
    for x in 13..=35 {
        s.world.set_tile(Pos::new(x, 4, 1), Tile::Wall, stone);
        s.world.set_tile(Pos::new(x, 6, 1), Tile::Wall, stone);
    }
    let gap = Pos::new(12, 5, 1);
    s.world.set_tile(gap, Tile::Open, AIR);
    let kind = s.things.add_kind(ThingKind::new("door", '+', Rgb::WHITE, Layer::Furniture).blocking().door('\''));
    let door = s.things.spawn(kind, gap);
    (s, door)
}

#[test]
fn a_scream_in_a_stone_room_is_heard_down_the_corridor_from_the_open_door() {
    let (mut s, door) = room_and_corridor();
    let (scream, at) = (Pos::new(8, 3, 1), Pos::new(30, 5, 1));
    s.sounds = vec![Sound::new(scream, level::SCREAM, SoundKind::Scream)];

    s.things.get_mut(door).unwrap().open = true;
    let heard = s.hear(at, HUMAN_HEARING);
    let h = heard.first().expect("a scream was not heard down the corridor through an open door");
    assert_eq!(h.from, (12 - 30, 0, 0), "it should seem to come from the door");
    assert_eq!(s.sound_searches(), 1);

    s.things.get_mut(door).unwrap().open = false;
    assert!(s.hear(at, HUMAN_HEARING).is_empty(), "a scream was heard at the end of the corridor through a shut door");
}

#[test]
fn a_sound_with_nothing_in_the_way_never_runs_the_search() {
    let mut s = field(24);
    s.sounds = vec![Sound::new(Pos::new(3, 5, 1), level::SHOUT, SoundKind::Speech(Voice::Shout))];
    assert!(!s.hear(Pos::new(15, 5, 1), HUMAN_HEARING).is_empty());
    assert_eq!(s.sound_searches(), 0);
    // With a wall across the line, the search does run.
    wall_at(&mut s, 10, "wood");
    s.hear(Pos::new(15, 5, 1), HUMAN_HEARING);
    assert_eq!(s.sound_searches(), 1);
}

/// Open ground with a grass floor (soft underfoot), two seconds a tick, noon.
fn meadow(w: i32, h: i32) -> Scene {
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
    s
}

fn creature_kind(s: &mut Scene) -> KindId {
    s.things.add_kind(ThingKind::new("creature", '@', Rgb::WHITE, Layer::Creature).blocking())
}

#[test]
fn a_footstep_wakes_a_sleeping_dog_but_not_a_sleeping_person() {
    let mut s = meadow(24, 20);
    let (human, dog) = (s.life.add_species(species::human()), s.life.add_species(species::dog()));
    let k = creature_kind(&mut s);
    // Two sleepers whose routine says sleep all day, 3 m either side of
    // where someone will walk past.
    let sleeper = |s: &mut Scene, sp: SpeciesId, p: Pos| {
        let id = s.things.spawn(k, p);
        let c = s.life.add(id, sp, Control::Ai, 1);
        c.mind.routine = Routine::new().with(0.0, 24.0, Task::Sleep { at: None });
        c.mind.goal = Goal::Sleep(None);
        c.mind.asleep = true;
        id
    };
    let rex = sleeper(&mut s, dog, Pos::new(10, 7, 1));
    let ann = sleeper(&mut s, human, Pos::new(10, 13, 1));
    let walker = s.things.spawn(k, Pos::new(9, 10, 1));
    s.life.add(walker, human, Control::Player, 0);
    let asleep = |s: &Scene, id: ThingId| s.life.get(id).unwrap().mind.asleep;
    for _ in 0..5 {
        s.tick();
    }
    assert!(asleep(&s, rex) && asleep(&s, ann), "something else woke them");

    assert!(matches!(movement::step(&s.world, &mut s.things, walker, 1, 0), Step::Moved(_)));
    s.tick();
    let step = s.sounds.iter().find(|x| x.kind == SoundKind::Footstep && x.source == Some(walker)).expect("the step made no sound");
    // A person on grass: 18 + 10 log10(70) - 6 = about 30 dB.
    assert!((step.db - 30.45).abs() < 0.1, "a footstep on grass of {} dB", step.db);
    assert!(!asleep(&s, rex), "the dog slept through a footstep 3 m away");
    assert!(asleep(&s, ann), "a soft footstep 3 m away woke the person");
    // The person heard it, just not enough to wake.
    assert!(!s.hear(Pos::new(10, 13, 1), HUMAN_HEARING).is_empty());
}

#[test]
fn villagers_walk_toward_a_scream() {
    let mut s = meadow(48, 24);
    let human = s.life.add_species(species::human());
    let k = creature_kind(&mut s);
    let villagers: Vec<ThingId> = [(36, 2), (38, 12), (36, 22)]
        .iter()
        .map(|&(x, y)| {
            let id = s.things.spawn(k, Pos::new(x, y, 1));
            s.life.add(id, human, Control::Ai, 1);
            id
        })
        .collect();
    let scream = Pos::new(6, 12, 1);
    let dist = |s: &Scene, id: ThingId| s.things.get(id).unwrap().pos().dist2d(scream);
    let before: Vec<i32> = villagers.iter().map(|&v| dist(&s, v)).collect();
    s.sounds.push(Sound::new(scream, level::SCREAM, SoundKind::Scream));
    s.tick();
    for &v in &villagers {
        let m = &s.life.get(v).unwrap().mind;
        assert!(matches!(m.goal, Goal::Look(_)), "{:?} did not go to look", m.goal);
        let turned = m.heard.expect("did not turn toward the scream");
        assert!(turned.x < 20, "turned toward {turned:?}");
    }
    for _ in 0..3 {
        s.tick();
    }
    for (k, &v) in villagers.iter().enumerate() {
        let now = dist(&s, v);
        assert!(now <= before[k] - 3, "villager {k} went from {} to {now} cells from the scream", before[k]);
    }
}
#[test]
fn a_floor_slab_muffles_footsteps_overhead() {
    let mut s = field(12);
    let wood = s.world.mats.id("wood");
    s.world.set_tile(Pos::new(5, 5, 2), Tile::Floor, wood);
    let (above, below) = (Pos::new(5, 5, 2), Pos::new(5, 5, 1));
    assert_eq!(sound::wall_loss(&s.world, &s.things, above, below), 35.0);
    assert_eq!(sound::wall_loss(&s.world, &s.things, below, above), 35.0);
    // Beside it on the same level nothing is in the way.
    assert_eq!(sound::wall_loss(&s.world, &s.things, above, Pos::new(8, 5, 2)), 0.0);
}