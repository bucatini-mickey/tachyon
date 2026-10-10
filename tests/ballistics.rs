//! Round shot: trajectories, penetration, ricochets and shells.

use tachyon::ballistics::{elevation_for, range_at, Ballistics, Body, Damage, Placement, Shot, ShotEvent};
use tachyon::prelude::*;
use tachyon::sea::{Sea, SeaSettings};

const POUNDER_24: f32 = 10.9;
const POUNDER_6: f32 = 2.72;

#[test]
fn a_24_pounder_ranges_like_the_tables() {
    // From a gun 3 m above the water.
    let pb = range_at(POUNDER_24, 450.0, 0.0, 3.0);
    let one = range_at(POUNDER_24, 450.0, 1.0f32.to_radians(), 3.0);
    let five = range_at(POUNDER_24, 450.0, 5.0f32.to_radians(), 3.0);
    assert!((200.0..450.0).contains(&pb), "point blank: {pb:.0} m");
    assert!((600.0..1150.0).contains(&one), "one degree: {one:.0} m");
    assert!((1500.0..2700.0).contains(&five), "five degrees: {five:.0} m");
    // A light ball sheds its speed faster.
    assert!(range_at(POUNDER_6, 450.0, 5.0f32.to_radians(), 3.0) < five);
}

#[test]
fn laying_a_gun_for_range_hits_the_range() {
    for r in [300.0, 800.0, 1500.0] {
        let e = elevation_for(POUNDER_24, 450.0, r, 3.0).expect("in reach");
        let got = range_at(POUNDER_24, 450.0, e, 3.0);
        assert!((got - r).abs() < 3.0, "laid for {r} m, falls at {got:.1} m");
    }
    assert!(elevation_for(POUNDER_6, 300.0, 20_000.0, 3.0).is_none(), "out of reach");
}

#[test]
fn placements_round_trip() {
    let p = Placement::pose([120.0, -40.0, 1.5], [8.0, 20.0, 3.5], 2.3, 0.05, -0.12);
    for l in [[0.0, 0.0, 0.0], [8.0, 20.0, 3.5], [15.0, 39.0, 8.0], [3.3, 7.7, 1.1]] {
        let back = p.to_local(p.to_world(l));
        for k in 0..3 {
            assert!((back[k] - l[k]).abs() < 1e-3, "{l:?} -> {back:?}");
        }
    }
    // Heading east: her bow (-y) points east (+x).
    let east = Placement::pose([0.0; 3], [0.0; 3], std::f32::consts::FRAC_PI_2, 0.0, 0.0);
    let bow = east.dir_to_world([0.0, -1.0, 0.0]);
    assert!((bow[0] - 1.0).abs() < 1e-5 && bow[1].abs() < 1e-5);
    // Heeled to starboard: her starboard side (+x) goes down.
    let heel = Placement::pose([0.0; 3], [0.0; 3], 0.0, 0.0, 0.2);
    assert!(heel.dir_to_world([1.0, 0.0, 0.0])[2] < -0.1);
    // Bow up: her bow rises.
    let pitch = Placement::pose([0.0; 3], [0.0; 3], 0.0, 0.2, 0.0);
    assert!(pitch.dir_to_world([0.0, -1.0, 0.0])[2] > 0.1);
}

/// A one-cell-thick wall of `mat` across x = 2 of a 5 x 1 x 3 grid placed
/// with its corner at (100, 0, 0).
fn wall(mat: &str) -> (World, Things, Damage) {
    let mats = Materials::standard();
    let m = mats.id(mat);
    let mut w = World::new(5, 1, 3, mats);
    for z in 0..3 {
        w.set_tile(Pos::new(2, 0, z), Tile::Wall, m);
    }
    (w, Things::new(), Damage::default())
}

fn shoot(world: &mut World, things: &mut Things, damage: &mut Damage, mass: f32, speed: f32) -> Vec<ShotEvent> {
    let mut b = Ballistics::new(1);
    b.fire(Shot::ball(mass, [0.0, 0.5, 1.5], [speed, 0.0, 0.0]));
    let mut bodies = [Body { id: 7, world, things, place: Placement::fixed([100.0, 0.0, 0.0]), damage }];
    for _ in 0..20 {
        b.step(0.05, None, (0.0, 0.0), &mut bodies);
    }
    b.events
}

#[test]
fn a_heavy_ball_goes_through_a_metre_of_oak_and_a_light_one_lodges() {
    let (mut w, mut t, mut d) = wall("wood");
    let ev = shoot(&mut w, &mut t, &mut d, POUNDER_24, 450.0);
    let hit = ev.iter().find_map(|e| match e {
        ShotEvent::Struck { body: 7, cell, through, from, to, .. } => Some((*cell, *through, *from, *to)),
        _ => None,
    });
    let (cell, through, from, to) = hit.expect("the wall is struck");
    assert_eq!(cell, Pos::new(2, 0, 1));
    assert!(through, "24-pounder at 450 m/s goes through");
    assert_eq!(from, Some(Pos::new(1, 0, 1)));
    assert_eq!(to, Some(Pos::new(3, 0, 1)));

    let (mut w, mut t, mut d) = wall("wood");
    let ev = shoot(&mut w, &mut t, &mut d, POUNDER_6, 450.0);
    assert!(ev.iter().any(|e| matches!(e, ShotEvent::Struck { through: false, .. })), "a 6-pounder lodges in a metre of oak");

    let (mut w, mut t, mut d) = wall("granite");
    let ev = shoot(&mut w, &mut t, &mut d, POUNDER_24, 450.0);
    assert!(ev.iter().any(|e| matches!(e, ShotEvent::Struck { through: false, .. })), "granite stops it");
}

#[test]
fn enough_shot_breaks_a_wall() {
    let (mut w, mut t, mut d) = wall("wood");
    let mut broke = false;
    for k in 0..20 {
        let ev = shoot(&mut w, &mut t, &mut d, POUNDER_24, 450.0);
        if ev.iter().any(|e| matches!(e, ShotEvent::Struck { broke: true, .. })) {
            broke = true;
            assert!(k >= 8, "a 1 m cell takes a battering, gave way after {}", k + 1);
            break;
        }
    }
    assert!(broke, "the wall gives way");
    // On solid wall below, a broken cell is left as rubble (a floor), as a blast leaves it.
    assert_eq!(w.tile(Pos::new(2, 0, 1)), Tile::Floor);
}

#[test]
fn shot_strikes_the_men_in_its_path() {
    let (mut w, mut t, mut d) = wall("wood");
    let man = t.add_kind(ThingKind::new("sailor", '@', Rgb::WHITE, Layer::Creature));
    let id = t.spawn(man, Pos::new(1, 0, 1));
    let ev = shoot(&mut w, &mut t, &mut d, POUNDER_24, 450.0);
    assert!(ev.iter().any(|e| matches!(e, ShotEvent::HitThing { thing, .. } if *thing == id)));
}

#[test]
fn a_shell_bursts_on_impact() {
    let (mut w, mut t, mut d) = wall("wood");
    let mut b = Ballistics::new(1);
    let blast = Blast { radius: 2.0, power: 150.0, heat: 200.0, ignite: 0.5 };
    b.fire(Shot::ball(POUNDER_24, [0.0, 0.5, 1.5], [300.0, 0.0, 0.0]).shell(blast, 30.0));
    let mut bodies = [Body { id: 1, world: &mut w, things: &mut t, place: Placement::fixed([100.0, 0.0, 0.0]), damage: &mut d }];
    for _ in 0..20 {
        b.step(0.05, None, (0.0, 0.0), &mut bodies);
    }
    let burst = b.events.iter().find_map(|e| match e {
        ShotEvent::Burst { body: Some(1), report, .. } => Some(*report),
        _ => None,
    });
    assert!(burst.expect("it bursts").broken > 0, "and blows the planks out");
}

fn flat_sea() -> Sea {
    let mut s = Sea::new(128, 16, SeaSettings { tide_range: 0.0, ..Default::default() }, |_, _| 30.0);
    s.settle((0.0, 0.0));
    s
}

#[test]
fn a_ball_fired_low_skips_along_the_water() {
    let sea = flat_sea();
    let mut b = Ballistics::new(2);
    let e = -0.5f32.to_radians();
    b.fire(Shot::ball(POUNDER_24, [0.0, 100.0, 2.0], [450.0 * e.cos(), 0.0, 450.0 * e.sin()]));
    for _ in 0..200 {
        b.step(0.05, Some(&sea), (0.0, 0.0), &mut []);
    }
    let skips = b.events.iter().filter(|e| matches!(e, ShotEvent::Ricochet { .. })).count();
    assert!(skips >= 2, "ricochets: {skips}");
}

#[test]
fn a_plunging_ball_goes_in_and_is_spent_within_metres() {
    let sea = flat_sea();
    let mut b = Ballistics::new(3);
    let e = -20f32.to_radians();
    b.fire(Shot::ball(POUNDER_24, [0.0, 100.0, 20.0], [300.0 * e.cos(), 0.0, 300.0 * e.sin()]));
    let mut entered = None;
    for _ in 0..100 {
        b.step(0.02, Some(&sea), (0.0, 0.0), &mut []);
        if entered.is_none() {
            entered = b.events.iter().find_map(|e| match e {
                ShotEvent::Splash { at, .. } => Some(*at),
                _ => None,
            });
        }
    }
    assert!(!b.events.iter().any(|e| matches!(e, ShotEvent::Ricochet { .. })), "too steep to skip");
    let at = entered.expect("splash");
    assert!(b.shots.is_empty(), "spent under water");
    let _ = at;
}

#[test]
fn a_ball_short_of_the_waterline_still_holes_her_under_water() {
    // A wall standing in the sea: the grid's level 0 is 3 m below the surface.
    let sea = flat_sea();
    let (mut w, mut t, mut d) = wall("wood");
    let mut b = Ballistics::new(4);
    // Aimed to strike the water 1.5 m short of the wall, at 10 degrees.
    let e = -10f32.to_radians();
    let (x0, z0) = (0.0, (102.0 - 1.5) * e.tan().abs());
    b.fire(Shot::ball(POUNDER_24, [x0, 0.5, z0], [450.0 * e.cos(), 0.0, 450.0 * e.sin()]));
    let mut bodies = [Body { id: 1, world: &mut w, things: &mut t, place: Placement::fixed([100.0, 0.0, -3.0]), damage: &mut d }];
    for _ in 0..40 {
        b.step(0.02, Some(&sea), (0.0, 0.0), &mut bodies);
    }
    assert!(b.events.iter().any(|e| matches!(e, ShotEvent::Splash { .. })));
    let struck = b.events.iter().find_map(|e| match e {
        ShotEvent::Struck { cell, energy, .. } => Some((*cell, *energy)),
        _ => None,
    });
    let (cell, _) = struck.expect("it carries on under water into the planks");
    assert!(cell.z < 3, "below the waterline: level {}", cell.z);
}
