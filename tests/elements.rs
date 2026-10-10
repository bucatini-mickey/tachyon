//! Fire, water, wind and blasts acting on each other.

use tachyon::prelude::*;
use tachyon::projectile::{explode, Flight};
use tachyon::sim::flames::{self, Lighting};

/// Open ground: granite at z = 0, grass floor at z = 1, open sky above.
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
    Scene::new(world, Things::new(), Clock::new(1.0, 12.0), 11)
}

fn torch_kinds(s: &mut Scene) -> (KindId, KindId) {
    let torch = s.things.add_kind(ThingKind::new("torch", '¡', Rgb(255, 210, 110), Layer::Furniture).light(Rgb(255, 176, 92), true).flame(Flame::Exposed));
    let lantern = s.things.add_kind(ThingKind::new("lantern", 'ð', Rgb(255, 210, 110), Layer::Item).light(Rgb(230, 190, 120), true).flame(Flame::Sheltered));
    (torch, lantern)
}

#[test]
fn torch_goes_out_underwater_and_is_hard_to_light_wet() {
    let mut s = field(10, 10);
    let (torch, _) = torch_kinds(&mut s);
    let water = s.world.mats.id("water");
    let p = Pos::new(5, 5, 1);
    s.world.set_fluid(p, water, FULL);
    let t = s.things.spawn(torch, p);
    s.sim.fluids = false; // keep the water where it is
    s.tick();
    let th = s.things.get(t).unwrap();
    assert!(!th.lit && th.wet == flames::SOAKED, "a torch in water must go out and be soaked");
    let mut rng = Rng::new(1);
    assert_eq!(flames::try_light(&s.world, &mut s.things, t, &mut rng), Lighting::Underwater);

    // Out of the water but soaked: it rarely catches.
    s.things.move_to(t, Pos::new(2, 2, 1));
    let tries = 200;
    let lit = (0..tries)
        .filter(|_| {
            let ok = flames::try_light(&s.world, &mut s.things, t, &mut rng) == Lighting::Lit;
            s.things.get_mut(t).unwrap().lit = false;
            ok
        })
        .count();
    assert!(lit < tries / 10, "a soaked torch lit {lit} times out of {tries}");

    // Once dry, it lights.
    for _ in 0..flames::SOAKED {
        s.tick();
    }
    assert_eq!(s.things.get(t).unwrap().wet, 0);
    assert_eq!(flames::try_light(&s.world, &mut s.things, t, &mut rng), Lighting::Lit);
}

#[test]
fn gale_blows_out_torches_but_not_lanterns() {
    let mut s = field(10, 10);
    let (torch, lantern) = torch_kinds(&mut s);
    let a = s.things.spawn(torch, Pos::new(3, 3, 1));
    let b = s.things.spawn(lantern, Pos::new(6, 6, 1));
    s.weather.set_wind(1.5, 0.0);
    s.weather.gustiness = 0.0;
    for _ in 0..400 {
        s.tick();
    }
    assert!(!s.things.get(a).unwrap().lit, "an open torch should blow out in a gale");
    assert!(s.things.get(b).unwrap().lit, "a lantern shelters its flame");
}

#[test]
fn rain_puts_out_open_flames() {
    let mut s = field(10, 10);
    let (torch, _) = torch_kinds(&mut s);
    let a = s.things.spawn(torch, Pos::new(3, 3, 1));
    s.weather.rain = 1.0;
    for _ in 0..600 {
        s.tick();
    }
    let t = s.things.get(a).unwrap();
    assert!(!t.lit && t.wet > 0, "rain should put out an open torch and wet it");
}

fn centre_x(world: &World) -> f32 {
    let (mut sum, mut n) = (0.0, 0.0);
    for i in 0..world.len() {
        let g = world.gas[i] as f32;
        sum += world.pos(i).x as f32 * g;
        n += g;
    }
    sum / n.max(1.0)
}

#[test]
fn fog_drifts_downwind() {
    // Mist hugs the ground (smoke would rise out of this shallow world).
    let mut s = field(60, 20);
    let mist = s.world.mats.id("mist");
    s.weather.set_wind(0.6, 0.0);
    s.weather.gustiness = 0.0;
    s.weather.veer = 0.0;
    s.tick();
    s.world.add_gas(Pos::new(20, 10, 2), mist, 250);
    let x0 = centre_x(&s.world);
    for _ in 0..30 {
        s.tick();
    }
    let x1 = centre_x(&s.world);
    assert!(x1 > x0 + 3.0, "fog should drift east: {x0} -> {x1}");
}

/// Mean x of everything burnt or burning after a grass fire started at x = 30.
fn grass_fire(wind: f32) -> f32 {
    let mut s = field(60, 30);
    let ash = s.world.mats.id("ash");
    s.weather.set_wind(wind, 0.0);
    s.weather.gustiness = 0.0;
    s.weather.veer = 0.0;
    s.tick();
    for y in 13..17 {
        s.world.ignite(Pos::new(30, y, 1));
    }
    for _ in 0..300 {
        s.tick();
    }
    let (mut sum, mut n) = (0.0, 0.0);
    for i in 0..s.world.len() {
        if s.world.fire[i] > 0 || s.world.mat[i] == ash {
            sum += s.world.pos(i).x as f32;
            n += 1.0;
        }
    }
    assert!(n > 0.0);
    sum / n
}

#[test]
fn wind_drives_a_fire_downwind() {
    let calm = grass_fire(0.0);
    let windy = grass_fire(0.8);
    assert!(windy > calm + 1.5, "fire should run east with the wind: calm {calm:.1}, windy {windy:.1}");
}

#[test]
fn stone_shelters_wood_from_a_blast() {
    let mut s = field(20, 20);
    let (wood, stone) = (s.world.mats.id("wood"), s.world.mats.id("stone block"));
    // West of the blast: bare wood. East: wood behind a stone wall.
    s.world.set_tile(Pos::new(8, 10, 1), Tile::Wall, wood);
    s.world.set_tile(Pos::new(11, 10, 1), Tile::Wall, stone);
    s.world.set_tile(Pos::new(12, 10, 1), Tile::Wall, wood);
    let blast = Blast { radius: 3.0, power: 200.0, heat: 0.0, ignite: 0.0 };
    let mut rng = Rng::new(3);
    let r = explode(&mut s.world, &mut s.things, Pos::new(10, 10, 1), &blast, &mut rng);
    assert!(r.broken >= 1);
    assert!(!s.world.tile(Pos::new(8, 10, 1)).is_wall(), "exposed wood should be smashed");
    assert!(s.world.tile(Pos::new(11, 10, 1)).is_wall(), "the stone wall should stand");
    assert!(s.world.tile(Pos::new(12, 10, 1)).is_wall(), "wood behind stone should be sheltered");
}

#[test]
fn firebolt_bursts_at_a_wall_and_fizzles_in_water() {
    let mut s = field(30, 10);
    let bolt = s.things.add_kind(ThingKind::new("bolt", '*', Rgb(255, 200, 90), Layer::Effect));
    let wood = s.world.mats.id("wood");
    s.world.set_tile(Pos::new(20, 5, 1), Tile::Wall, wood);
    let blast = Blast { radius: 2.0, power: 120.0, heat: 300.0, ignite: 1.0 };
    let mut p = Projectile::launch(&mut s.things, bolt, Pos::new(5, 5, 1), Pos::new(20, 5, 1), 1.0, blast, None);
    let mut rng = Rng::new(9);
    let result = (0..40).find_map(|_| match p.step(&mut s.world, &mut s.things, &mut rng) {
        Flight::Flying => None,
        other => Some(other),
    });
    match result {
        Some(Flight::Exploded(at, _)) => assert_eq!(at, Pos::new(19, 5, 1), "should burst just short of the wall"),
        other => panic!("expected an explosion, got {other:?}"),
    }
    assert!(s.things.get(p.thing).is_none(), "the bolt is gone");

    let water = s.world.mats.id("water");
    for x in 10..15 {
        s.world.set_fluid(Pos::new(x, 2, 1), water, FULL);
    }
    let mut p = Projectile::launch(&mut s.things, bolt, Pos::new(5, 2, 1), Pos::new(25, 2, 1), 1.0, blast, None);
    let result = (0..40).find_map(|_| match p.step(&mut s.world, &mut s.things, &mut rng) {
        Flight::Flying => None,
        other => Some(other),
    });
    assert!(matches!(result, Some(Flight::Fizzled(_))), "expected a fizzle, got {result:?}");
}

/// A stone hut with a slate roof and an open doorway, on the open field.
fn hut(s: &mut Scene) -> Pos {
    let (stone, slate) = (s.world.mats.id("stone block"), s.world.mats.id("slate"));
    for y in 3..=9 {
        for x in 3..=9 {
            let edge = x == 3 || x == 9 || y == 3 || y == 9;
            if edge && !(x == 6 && y == 9) {
                s.world.set_tile(Pos::new(x, y, 2), Tile::Wall, stone);
            }
            s.world.set_tile(Pos::new(x, y, 3), Tile::Floor, slate);
        }
    }
    Pos::new(6, 8, 1) // just inside the doorway
}

#[test]
fn no_rain_or_wind_under_a_roof() {
    let mut s = field(14, 14);
    let inside = hut(&mut s);
    let (torch, _) = torch_kinds(&mut s);
    let t = s.things.spawn(torch, inside);
    s.weather.rain = 1.0;
    s.weather.set_wind(1.5, 0.0);
    for _ in 0..600 {
        s.tick();
    }
    let i = s.world.index(inside).unwrap();
    assert!(s.world.sky[i] > 0, "daylight should still reach in through the doorway");
    assert!(!s.world.open_sky[i], "but the weather should not");
    assert_eq!(s.world.fluid[i], 0, "rain fell indoors");
    let th = s.things.get(t).unwrap();
    assert!(th.lit && th.wet == 0, "a torch indoors was rained on or blown out");
}

#[test]
fn water_puts_out_fire_and_it_stays_out() {
    let mut s = field(16, 8);
    let (wood, water) = (s.world.mats.id("wood"), s.world.mats.id("water"));
    s.sim.fluids = false;
    // A burning wooden floor flooded, and a burning wooden wall with water beside it.
    let floor = Pos::new(4, 4, 1);
    s.world.set_tile(floor, Tile::Floor, wood);
    let wall = Pos::new(10, 4, 1);
    s.world.set_tile(wall, Tile::Wall, wood);
    s.world.ignite(floor);
    s.world.ignite(wall);
    s.tick();
    s.world.set_fluid(floor, water, 20);
    s.world.set_fluid(Pos::new(11, 4, 1), water, FULL);
    for _ in 0..60 {
        s.tick();
    }
    let (fi, wi) = (s.world.index(floor).unwrap(), s.world.index(wall).unwrap());
    assert_eq!(s.world.fire[fi], 0, "flooded floor still burning");
    assert_eq!(s.world.fire[wi], 0, "wall with water against it still burning");
    // Drain the water: it was cooled, so it does not flare up again.
    s.world.set_fluid(floor, water, 0);
    s.world.set_fluid(Pos::new(11, 4, 1), water, 0);
    for _ in 0..60 {
        s.tick();
    }
    assert_eq!(s.world.fire[fi], 0, "floor reignited once dry");
    assert_eq!(s.world.fire[wi], 0, "wall reignited once dry");
}
