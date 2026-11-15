//! Sound: loss through walls and doors, and the noise floor (ROADMAP section 5).

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
