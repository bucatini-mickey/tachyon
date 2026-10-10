//! Physics invariants: things that must stay true however the tuning changes.

use tachyon::prelude::*;
use tachyon::sim::Sim;
use tachyon::view::fov::FovMap;

/// A `w` x `h` x `d` world of open air over a solid granite floor at z = 0.
fn flat(w: i32, h: i32, d: i32) -> World {
    let mats = Materials::standard();
    let granite = mats.id("granite");
    let mut world = World::new(w, h, d, mats);
    for y in 0..h {
        for x in 0..w {
            world.set_tile(Pos::new(x, y, 0), Tile::Wall, granite);
        }
    }
    world
}

fn run(world: &mut World, ticks: u32) {
    let mut sim = Sim::new(7);
    for t in 0..ticks {
        sim.step(world, t as u64);
    }
}

/// A walled basin on z = 1 with a `n` x `n` interior starting at (1, 1).
fn basin(world: &mut World, n: i32) {
    let granite = world.mats.id("granite");
    for z in 1..world.d {
        for i in 0..=n + 1 {
            for &(x, y) in &[(i, 0), (i, n + 1), (0, i), (n + 1, i)] {
                world.set_tile(Pos::new(x, y, z), Tile::Wall, granite);
            }
        }
    }
}

/// A liquid that never evaporates, so its total must not change at all.
fn add_oil(world: &mut World) -> MatId {
    world.mats.add(Material {
        name: "oil".into(),
        kind: MatKind::Liquid { viscosity: 2 },
        ..Default::default()
    })
}

#[test]
fn liquid_mass_is_conserved() {
    let mut world = flat(24, 24, 5);
    basin(&mut world, 10);
    let oil = add_oil(&mut world);
    for x in 2..6 {
        for y in 2..5 {
            world.set_fluid(Pos::new(x, y, 4), oil, FULL);
        }
    }
    let before = world.total_fluid(oil);
    let mut sim = Sim::new(3);
    for t in 0..800 {
        sim.step(&mut world, t);
        assert_eq!(world.total_fluid(oil), before, "liquid changed at tick {t}");
    }
}

#[test]
fn liquid_falls_and_levels() {
    let n = 8;
    let mut world = flat(16, 16, 5);
    basin(&mut world, n);
    let oil = add_oil(&mut world);
    // Twelve full cells dropped from the top level into a 64-cell basin: 12
    // units each. The surface search is short-range, so allow 2 units of slack.
    for x in 1..5 {
        for y in 1..4 {
            world.set_fluid(Pos::new(x, y, 4), oil, FULL);
        }
    }
    run(&mut world, 1500);
    let amount = |x, y, z| world.fluid[world.idx(x, y, z)];
    for z in 2..5 {
        for y in 1..=n {
            for x in 1..=n {
                assert_eq!(amount(x, y, z), 0, "liquid left hanging at ({x},{y},{z})");
            }
        }
    }
    let floor: Vec<u8> = (1..=n).flat_map(|y| (1..=n).map(move |x| (x, y))).map(|(x, y)| amount(x, y, 1)).collect();
    let (lo, hi) = (*floor.iter().min().unwrap(), *floor.iter().max().unwrap());
    assert!(hi - lo <= 2, "not level: depths range {lo}..={hi}");
}

/// Heat content relative to `base`, weighted by each cell's heat capacity
/// (mirrors `sim::heat::thermal`).
fn excess_heat(world: &World, base: f32) -> f64 {
    let air = world.mats.get(AIR).heat_cap;
    (0..world.len())
        .map(|i| {
            let cap = if world.tile[i].is_wall() {
                world.mats.get(world.mat[i]).heat_cap
            } else if world.fluid[i] >= 16 {
                world.mats.get(world.fluid_mat[i]).heat_cap
            } else {
                air
            };
            (cap * (world.temp[i] - base)) as f64
        })
        .sum()
}

/// Solid granite with a sealed air pocket and two hot spots. No sky, so the
/// only sink is the slow drift toward ground temperature, which shrinks every
/// simulated cell's excess by the same factor each tick. Conduction itself
/// must not add or lose any heat.
fn heat_box(ticks: u32, wake_all: bool) -> (f64, f64) {
    let mats = Materials::standard();
    let granite = mats.id("granite");
    let mut world = World::new(40, 40, 6, mats);
    for i in 0..world.len() {
        let p = world.pos(i);
        let pocket = (12..28).contains(&p.x) && (12..28).contains(&p.y) && (2..4).contains(&p.z);
        world.set_tile(p, if pocket { Tile::Open } else { Tile::Wall }, if pocket { AIR } else { granite });
    }
    world.ground_temp = 15.0;
    world.ambient = 15.0;
    world.set_temp(Pos::new(20, 20, 2), 1015.0);
    world.set_temp(Pos::new(11, 20, 3), 515.0); // inside the rock, next to a chunk border
    let e0 = excess_heat(&world, 15.0);
    let mut sim = Sim::new(7);
    for t in 0..ticks {
        if wake_all {
            world.wake.wake_all();
        }
        sim.step(&mut world, t as u64);
    }
    (e0, excess_heat(&world, 15.0))
}

#[test]
fn heat_is_conserved() {
    // Everything simulated every tick: the sink is the only change.
    let ticks = 300;
    let (e0, got) = heat_box(ticks, true);
    let expected = e0 * (1.0 - 0.0004f64).powi(ticks as i32);
    let err = (got - expected).abs() / expected;
    assert!(err < 0.001, "heat drifted: expected {expected:.1}, got {got:.1} ({:.3}% off)", err * 100.0);
}

#[test]
fn heat_does_not_leak_into_sleeping_chunks() {
    // Sleeping chunks skip the sink too, so heat can only end up between
    // "everything sank" and "nothing sank".
    let ticks = 300;
    let (e0, got) = heat_box(ticks, false);
    let floor = e0 * (1.0 - 0.0004f64).powi(ticks as i32);
    assert!(got >= floor * 0.999 && got <= e0 * 1.001, "heat out of bounds: {got:.1} not in {floor:.1}..={e0:.1}");
}

#[test]
fn fov_is_symmetric() {
    let (w, h) = (48, 48);
    let mut rng = Rng::new(42);
    let opaque: Vec<bool> = (0..w * h).map(|_| rng.chance(0.25)).collect();
    let is_opaque = |x: i32, y: i32| opaque[(y * w + x) as usize];
    let open: Vec<(i32, i32)> = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).filter(|&(x, y)| !is_opaque(x, y)).collect();

    let mut maps: Vec<FovMap> = Vec::new();
    let sample: Vec<(i32, i32)> = open.iter().step_by(7).copied().collect();
    for &o in &sample {
        let mut m = FovMap::new();
        m.compute(w, h, o, 100, is_opaque);
        maps.push(m);
    }
    for (a, ma) in sample.iter().zip(&maps) {
        for (b, mb) in sample.iter().zip(&maps) {
            assert_eq!(ma.visible(b.0, b.1), mb.visible(a.0, a.1), "{a:?} and {b:?} disagree about seeing each other");
        }
    }
}

#[test]
fn wood_wall_burns_to_ash() {
    let mut world = flat(16, 16, 3);
    let wood = world.mats.id("wood");
    let ash = world.mats.id("ash");
    let p = Pos::new(8, 8, 1);
    world.set_tile(p, Tile::Wall, wood);
    assert!(world.ignite(p));
    run(&mut world, 600);
    let i = world.index(p).unwrap();
    assert_eq!(world.fire[i], 0, "still burning");
    assert_eq!(world.tile[i], Tile::Floor, "a burnt wall on solid ground leaves a floor");
    assert_eq!(world.mat[i], ash);
}

#[test]
fn fire_spreads_along_a_wooden_wall() {
    let mut world = flat(32, 16, 3);
    let wood = world.mats.id("wood");
    for x in 4..12 {
        world.set_tile(Pos::new(x, 8, 1), Tile::Wall, wood);
    }
    world.ignite(Pos::new(4, 8, 1));
    let far = world.idx(11, 8, 1);
    let mut sim = Sim::new(1);
    let caught = (0..3000u64).find(|&t| {
        sim.step(&mut world, t);
        world.fire[far] > 0
    });
    assert!(caught.is_some(), "fire never reached the end of an 8-cell wooden wall");
}

#[test]
fn water_on_lava_makes_obsidian_and_steam() {
    let mut world = flat(16, 16, 4);
    basin(&mut world, 6);
    let (water, lava) = (world.mats.id("water"), world.mats.id("lava"));
    let (obsidian, steam) = (world.mats.id("obsidian"), world.mats.id("steam"));
    for x in 1..=6 {
        for y in 1..=6 {
            world.set_fluid(Pos::new(x, y, 1), lava, FULL);
        }
    }
    for x in 2..5 {
        world.set_fluid(Pos::new(x, 3, 3), water, FULL);
    }
    // Steam vents off the top of this shallow, open-air world within a few
    // ticks, so watch for it as it happens.
    let mut sim = Sim::new(5);
    let mut made_steam = false;
    for t in 0..60 {
        sim.step(&mut world, t);
        made_steam |= world.gas_mat.iter().zip(&world.gas).any(|(&m, &g)| m == steam && g > 0);
    }
    let made_obsidian = world.mat.contains(&obsidian);
    assert!(made_obsidian, "no obsidian formed");
    assert!(made_steam, "no steam formed");
}
