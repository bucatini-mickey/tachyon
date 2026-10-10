//! Liquid pressure: water finds its level through full, buried passages.

use tachyon::prelude::*;

/// Head of the water in a one-cell-wide arm: the level of its surface.
fn head(w: &World, x: i32, y: i32) -> f32 {
    let mut h = 0.0;
    for z in 0..w.d {
        let i = w.idx(x, y, z);
        if w.fluid[i] > 0 {
            h = z as f32 + w.fluid[i] as f32 / FULL as f32;
        }
    }
    h
}

/// Solid granite with a U carved in it: two arms at x = 2 and x = 9 (z 1..9)
/// joined by a channel along z = 1.
fn u_tube() -> Scene {
    let mats = Materials::standard();
    let (granite, water) = (mats.id("granite"), mats.id("water"));
    let mut world = World::new(12, 3, 10, mats);
    for z in 0..10 {
        for y in 0..3 {
            for x in 0..12 {
                world.set_tile(Pos::new(x, y, z), Tile::Wall, granite);
            }
        }
    }
    for x in 2..=9 {
        world.set_tile(Pos::new(x, 1, 1), Tile::Open, AIR);
        world.set_fluid(Pos::new(x, 1, 1), water, FULL);
    }
    for z in 2..10 {
        world.set_tile(Pos::new(2, 1, z), Tile::Open, AIR);
        world.set_tile(Pos::new(9, 1, z), Tile::Open, AIR);
    }
    for z in 2..=8 {
        world.set_fluid(Pos::new(2, 1, z), water, FULL);
    }
    let mut s = Scene::new(world, Things::new(), Clock::new(1.0, 12.0), 9);
    s.sim.heat = false;
    s
}

#[test]
fn u_tube_comes_level() {
    let mut s = u_tube();
    let water = s.world.mats.id("water");
    let before = s.world.total_fluid(water);
    for _ in 0..600 {
        s.tick();
    }
    let (l, r) = (head(&s.world, 2, 1), head(&s.world, 9, 1));
    assert_eq!(s.world.total_fluid(water), before, "pressure conserves liquid");
    assert!((l - r).abs() <= 3.0 / 64.0, "arms level: left {l:.3}, right {r:.3}");
    // 15 cells of water: 8 in the channel, 3.5 up each arm.
    assert!((l - 5.5).abs() < 0.1, "surface at 5.5, got {l:.3}");
}

#[test]
fn without_pressure_the_far_arm_stays_dry() {
    let mut s = u_tube();
    s.sim.pressure.budget = 0;
    for _ in 0..300 {
        s.tick();
    }
    // The channel's own full cell reads as a head of 2.0; nothing above it.
    assert!(head(&s.world, 9, 1) <= 2.0, "only pressure can lift water up the far arm");
}

#[test]
fn a_still_lake_costs_no_pressure_work() {
    let mats = Materials::standard();
    let (granite, water) = (mats.id("granite"), mats.id("water"));
    let mut world = World::new(48, 48, 5, mats);
    for y in 0..48 {
        for x in 0..48 {
            world.set_tile(Pos::new(x, y, 0), Tile::Wall, granite);
            for z in 1..4 {
                world.set_fluid(Pos::new(x, y, z), water, FULL);
            }
        }
    }
    let mut s = Scene::new(world, Things::new(), Clock::new(1.0, 12.0), 1);
    s.sim.heat = false;
    for _ in 0..200 {
        s.tick();
    }
    assert_eq!(s.sim.pressure.moves, 0, "a level lake has nothing to push");
}
