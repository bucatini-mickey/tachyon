//! Ships as scenes posed on the sea.

use tachyon::ballistics::ShotEvent;
use tachyon::craft::{Craft, CraftEvent, Floating, Foil, HullSpec};
use tachyon::prelude::*;
use tachyon::sea::{Sea, SeaSettings};

const KEEL: i32 = 1;
const DECK: i32 = 5;

fn footprint(x: i32, y: i32) -> bool {
    if !(3..37).contains(&y) {
        return false;
    }
    // A pointed bow.
    let half = match y {
        3 => 2,
        4 => 3,
        5 => 4,
        _ => 5,
    };
    (8 - half..8 + half).contains(&x)
}

/// A 34 m x 10 m hull with a pointed bow: bottom plating on level 1, sides
/// up to the deck on level 5 (its rim is the bulwark), a hold on levels 2-4,
/// and three cells of open margin all round.
fn hull(mass: f32) -> Craft {
    let mats = Materials::standard();
    let (wood, water) = (mats.id("wood"), mats.id("water"));
    let mut world = World::new(16, 40, 9, mats);
    for y in 0..40 {
        for x in 0..16 {
            if !footprint(x, y) {
                continue;
            }
            let rim = DIRS4.iter().any(|(dx, dy)| !footprint(x + dx, y + dy));
            world.set_tile(Pos::new(x, y, KEEL), Tile::Wall, wood);
            for z in KEEL + 1..=DECK {
                if rim {
                    world.set_tile(Pos::new(x, y, z), Tile::Wall, wood);
                } else if z == DECK {
                    world.set_tile(Pos::new(x, y, z), Tile::Floor, wood);
                }
            }
        }
    }
    let mut scene = Scene::new(world, Things::new(), Clock::new(1.0, 12.0), 7);
    scene.sim.heat = false;
    let mut c = Craft::new("Test", scene, HullSpec { mass, gm: 1.2, water, fouling: 1.0 }, 500.0, 500.0, 0.0);
    c.foils.push(Foil::rudder([8.0, 36.5, 2.5], 6.0));
    c
}

/// Mass that floats her 2.5 m deep (waterline on level 3.5): her waterplane
/// is 4 + 6 + 8 cells of bow and 31 rows of 10.
const MASS: f32 = 328.0 * 2.5 * 1025.0;

fn calm() -> Sea {
    Sea::new(64, 64, SeaSettings { tide_range: 0.0, ..Default::default() }, |_, _| 40.0)
}

fn rig(c: &mut Craft) {
    c.foils.push(Foil::square_sail("fore", [8.0, 12.0, 14.0], 220.0));
    c.foils.push(Foil::square_sail("main", [8.0, 24.0, 14.0], 220.0));
}

fn run(c: &mut Craft, sea: &mut Sea, wind: (f32, f32), ticks: usize) {
    for _ in 0..ticks {
        sea.step(1.0, wind);
        c.step(sea, wind);
    }
}

#[test]
fn she_floats_upright_at_her_waterline_and_dry() {
    let mut sea = calm();
    let mut c = hull(MASS);
    assert!((c.waterline - 3.5).abs() < 0.01, "waterline {:.2}", c.waterline);
    assert!((c.draft() - 2.5).abs() < 0.01);
    run(&mut c, &mut sea, (0.0, 0.0), 120);
    assert!(c.z.abs() < 0.05, "heave settles at the surface: {:.3}", c.z);
    assert!(c.roll.abs() < 0.005 && c.pitch.abs() < 0.005);
    assert_eq!(c.state, Floating::Afloat);
    assert!(c.water_inside < 1.0, "her hold stays dry: {:.1} kg", c.water_inside);
    let w = &c.scene.world;
    let water = w.mats.id("water");
    assert_eq!(w.fluid[w.idx(1, 20, 2)], FULL, "the sea stands along her side below the waterline");
    assert_eq!(w.fluid_mat[w.idx(1, 20, 2)], water);
    assert_eq!(w.fluid[w.idx(1, 20, 4)], 0, "and not above it");
    assert_eq!(w.fluid[w.idx(8, 20, 2)], 0, "her hold is empty");
}

#[test]
fn she_runs_before_the_wind_and_answers_her_helm() {
    let mut sea = calm();
    let mut c = hull(MASS);
    rig(&mut c);
    let wind = (0.0, -10.0); // blowing toward the north, from dead astern
    run(&mut c, &mut sea, wind, 300);
    let (vx, vy) = (c.vx, c.vy);
    assert!(-vy > 2.0 && -vy < 7.0, "running north at a few m/s: {:.2}", -vy);
    assert!(vx.abs() < 0.2, "straight downwind: vx {vx:.3}");
    assert!(c.heading.min(std::f32::consts::TAU - c.heading) < 0.05, "holding her course");
    c.helm(0.35);
    run(&mut c, &mut sea, wind, 60);
    assert!(c.heading > 0.2 && c.heading < 2.5, "turned to starboard: heading {:.2}", c.heading);
}

#[test]
fn a_beam_wind_heels_her_to_leeward_and_she_makes_leeway() {
    let mut sea = calm();
    let mut c = hull(MASS);
    rig(&mut c);
    for f in c.foils.iter_mut().filter(|f| f.name == "fore" || f.name == "main") {
        f.angle = -0.8; // braced round ~45 degrees, leading edges toward the wind (port)
    }
    let wind = (10.0, 0.0); // from the west, onto her port beam
    for _ in 0..200 {
        // The helmsman holds her on north (her foresail wants to pay her off).
        let err = (c.heading + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        c.helm(-1.5 * err - 4.0 * c.yaw_rate);
        sea.step(1.0, wind);
        c.step(&sea, wind);
    }
    assert!(c.roll > 0.02, "heeled to starboard: {:.3} rad", c.roll);
    assert!(c.vx > 0.0, "sagging to leeward");
    assert!(-c.vy > 0.5, "and still forging ahead: {:.2}", -c.vy);
    assert!(c.vx < -c.vy, "more headway than leeway: ({:.2}, {:.2})", c.vx, -c.vy);
}

#[test]
fn the_current_carries_her() {
    let mut sea = Sea::new(64, 64, SeaSettings { tide_range: 0.0, steady: (1.0, 0.0), ..Default::default() }, |_, _| 40.0);
    // From rest over the ground: the stream takes hold of her.
    let mut c = hull(MASS);
    run(&mut c, &mut sea, (0.0, 0.0), 600);
    assert!(c.vx > 0.6, "carried east by the stream: {:.2}", c.vx);
    // Drifting with it already, she keeps drifting with it.
    let mut d = hull(MASS);
    d.vx = 1.0;
    run(&mut d, &mut sea, (0.0, 0.0), 600);
    assert!((d.vx - 1.0).abs() < 0.03 && d.vy.abs() < 0.03, "drifting with the stream: ({:.3}, {:.3})", d.vx, d.vy);
}

#[test]
fn a_beam_sea_rolls_her_but_she_lives() {
    let mut sea = Sea::new(64, 64, SeaSettings { tide_range: 0.0, ..Default::default() }, |_, _| 60.0);
    sea.settle((14.0, 0.0)); // big waves running east, onto her beam
    let mut c = hull(MASS);
    let (mut max_roll, mut lo, mut hi) = (0.0f32, f32::MAX, f32::MIN);
    for _ in 0..300 {
        sea.step(1.0, (0.0, 0.0));
        c.step(&sea, (0.0, 0.0));
        max_roll = max_roll.max(c.roll.abs());
        lo = lo.min(c.z);
        hi = hi.max(c.z);
    }
    assert!(max_roll > 0.03, "rolling: {max_roll:.3}");
    assert!(hi - lo > 0.3, "heaving: {:.2} m", hi - lo);
    assert_eq!(c.state, Floating::Afloat);
}

#[test]
fn a_shot_hole_below_the_waterline_leaks_until_plugged() {
    let mut sea = calm();
    let mut c = hull(MASS);
    run(&mut c, &mut sea, (0.0, 0.0), 20);
    let wall = Pos::new(3, 20, 2);
    let hit = ShotEvent::Struck {
        body: 0,
        cell: wall,
        mat: c.scene.world.mats.id("wood"),
        energy: 8e5,
        through: true,
        broke: false,
        from: Some(Pos::new(2, 20, 2)),
        to: Some(Pos::new(4, 20, 2)),
        area: 0.0159,
    };
    c.take_hits(0, &[hit], &sea);
    assert!(c.events.iter().any(|e| matches!(e, CraftEvent::Holed { below_water: true, .. })));
    let wl0 = c.waterline;
    run(&mut c, &mut sea, (0.0, 0.0), 600);
    // ~0.04 m^3/s through a 24-pound shot hole a metre down.
    assert!(c.water_inside > 10_000.0 && c.water_inside < 40_000.0, "{:.0} kg in her", c.water_inside);
    assert!(c.waterline > wl0 + 0.02, "she settles: {:.3} -> {:.3}", wl0, c.waterline);
    c.plug(wall);
    let before = c.water_inside;
    run(&mut c, &mut sea, (0.0, 0.0), 200);
    assert!((c.water_inside - before).abs() < 500.0, "plugged: {before:.0} -> {:.0}", c.water_inside);
}

/// Stave in five planks of her port side on level 2, a metre below the
/// waterline, and run until she founders or `ticks` pass.
fn stove_in(c: &mut Craft, sea: &mut Sea, ticks: usize) -> Option<usize> {
    run(c, sea, (0.0, 0.0), 10);
    for y in 18..23 {
        c.scene.world.set_tile(Pos::new(3, y, 2), Tile::Open, AIR);
    }
    for t in 0..ticks {
        sea.step(1.0, (0.0, 0.0));
        c.step(sea, (0.0, 0.0));
        if c.events.iter().any(|e| matches!(e, CraftEvent::Foundered)) {
            return Some(t);
        }
    }
    None
}

#[test]
fn stove_in_with_a_tight_deck_she_floods_to_the_sea_and_floats_waterlogged() {
    let mut sea = calm();
    let mut c = hull(MASS);
    let sank = stove_in(&mut c, &mut sea, 6000);
    let w = &c.scene.world;
    assert!(sank.is_none(), "her watertight deck keeps her up");
    // Pressure lifted the flood above the breach (level 2) to the top of the hold.
    assert_eq!(w.fluid[w.idx(8, 30, 3)], FULL, "level 3 flooded");
    assert_eq!(w.fluid[w.idx(8, 30, 4)], FULL, "level 4 flooded");
    assert!(c.water_inside > 600_000.0, "{:.0} t of sea in her", c.water_inside / 1000.0);
    assert!(c.waterline > 4.6, "down to her deck: waterline {:.2}", c.waterline);
}

#[test]
fn stove_in_with_an_open_hatch_she_founders() {
    let mut sea = calm();
    let mut c = hull(MASS);
    for y in 26..28 {
        for x in 7..9 {
            c.scene.world.set_tile(Pos::new(x, y, DECK), Tile::Open, AIR);
        }
    }
    let sank = stove_in(&mut c, &mut sea, 8000);
    assert!(sank.is_some(), "the sea comes up through the hatch and she goes down ({:.0} t in her, waterline {:.2})", c.water_inside / 1000.0, c.waterline);
}

#[test]
fn ships_that_meet_are_knocked_apart() {
    let mut sea = calm();
    let mut crafts = vec![hull(MASS), hull(MASS)];
    crafts[1].x = 511.0; // alongside, overlapping by a metre
    crafts[0].vx = 0.5;
    for _ in 0..30 {
        sea.step(1.0, (0.0, 0.0));
        for c in crafts.iter_mut() {
            c.step(&sea, (0.0, 0.0));
        }
        tachyon::craft::collide(&mut crafts);
    }
    assert!(crafts[0].events.iter().any(|e| matches!(e, CraftEvent::Collided { .. })));
    assert!(crafts[1].x - crafts[0].x >= 10.0, "pushed clear: {:.2} apart", crafts[1].x - crafts[0].x);
}

#[test]
fn she_takes_the_ground_on_a_shoal() {
    // Deep water, then a bank 1.5 m deep to the north.
    let mut sea = Sea::new(64, 96, SeaSettings { tide_range: 0.0, ..Default::default() }, |_, y| if y < 300.0 { 1.5 } else { 40.0 });
    let mut c = hull(MASS);
    rig(&mut c);
    let wind = (0.0, -10.0);
    let mut grounded = false;
    for _ in 0..400 {
        sea.step(1.0, wind);
        c.step(&sea, wind);
        grounded |= c.events.iter().any(|e| matches!(e, CraftEvent::Grounded { .. }));
        if grounded {
            break;
        }
    }
    assert!(grounded, "she runs onto the bank");
    run(&mut c, &mut sea, wind, 60);
    assert!(c.speed() < 0.3, "and is held there: {:.2} m/s", c.speed());
    assert_eq!(c.state, Floating::Aground);
}
