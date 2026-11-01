//! Material x process interactions from docs/INTERACTIONS.md that had code but no test.

use std::collections::VecDeque;
use tachyon::prelude::*;
use tachyon::projectile::explode;
use tachyon::sim::Sim;

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

/// Granite walls ring the interior `x0..=x1`, `y0..=y1` on levels `z0..=z1`.
fn ring(world: &mut World, (x0, y0, x1, y1): (i32, i32, i32, i32), (z0, z1): (i32, i32)) {
    let granite = world.mats.id("granite");
    for z in z0..=z1 {
        for x in x0 - 1..=x1 + 1 {
            for y in y0 - 1..=y1 + 1 {
                if x < x0 || x > x1 || y < y0 || y > y1 {
                    world.set_tile(Pos::new(x, y, z), Tile::Wall, granite);
                }
            }
        }
    }
}

fn recompute_sky(world: &mut World) {
    tachyon::sim::light::compute_sky(world, &mut VecDeque::new());
}

#[test]
fn ice_melts_to_water_when_warm() {
    let mut world = flat(10, 10, 4);
    let (ice, water) = (world.mats.id("ice"), world.mats.id("water"));
    let p = Pos::new(5, 5, 1);
    world.set_tile(p, Tile::Wall, ice);
    world.set_temp(p, 20.0);
    run(&mut world, 3);
    let i = world.index(p).unwrap();
    assert_ne!(world.tile[i], Tile::Wall, "ice stayed solid at 20 C");
    assert!(world.total_fluid(water) > 0, "melted ice left no water");
}

#[test]
fn water_freezes_to_ice_when_cold() {
    let mut world = flat(10, 10, 4);
    let (ice, water) = (world.mats.id("ice"), world.mats.id("water"));
    let p = Pos::new(5, 5, 1);
    world.set_fluid(p, water, FULL);
    world.set_temp(p, -20.0);
    let mut sim = Sim::new(1);
    sim.fluids = false; // keep the water in its cell
    for t in 0..3 {
        sim.step(&mut world, t);
    }
    let i = world.index(p).unwrap();
    assert_eq!(world.tile[i], Tile::Wall);
    assert_eq!(world.mat[i], ice);
    assert_eq!(world.fluid[i], 0);
}

#[test]
fn gas_is_conserved_in_a_sealed_room() {
    let mut world = flat(14, 14, 6);
    ring(&mut world, (3, 3, 8, 8), (1, 3));
    let granite = world.mats.id("granite");
    for x in 3..=8 {
        for y in 3..=8 {
            world.set_tile(Pos::new(x, y, 4), Tile::Wall, granite); // roof
        }
    }
    // A gas that neither decays nor rises, so only diffusion acts on it.
    let inert = world.mats.add(Material { name: "inert".into(), kind: MatKind::Gas { rise: 0.0, decay: 0.0, opacity: 255 }, ..Default::default() });
    world.add_gas(Pos::new(5, 5, 2), inert, 200);
    let total = |w: &World| w.gas.iter().map(|&g| g as u32).sum::<u32>();
    let mut sim = Sim::new(2);
    for t in 0..300 {
        sim.step(&mut world, t);
        assert_eq!(total(&world), 200, "gas changed at tick {t}");
    }
    let spread = world.gas.iter().filter(|&&g| g > 0).count();
    assert!(spread > 20, "gas only spread to {spread} cells");
}

#[test]
fn smoke_rises_decays_and_pools_under_a_ceiling() {
    let mut world = flat(12, 12, 9);
    ring(&mut world, (3, 3, 7, 7), (1, 6));
    let granite = world.mats.id("granite");
    for x in 3..=7 {
        for y in 3..=7 {
            world.set_tile(Pos::new(x, y, 6), Tile::Wall, granite); // ceiling
        }
    }
    let smoke = world.mats.id("smoke");
    world.add_gas(Pos::new(5, 5, 1), smoke, 250);
    let level = |w: &World, z: i32| -> u32 { (0..w.len()).filter(|&i| w.pos(i).z == z).map(|i| w.gas[i] as u32).sum() };
    run(&mut world, 120);
    let under_ceiling = level(&world, 5);
    assert!(under_ceiling > level(&world, 1), "smoke did not climb: top {under_ceiling}, floor {}", level(&world, 1));
    for z in 6..9 {
        assert_eq!(level(&world, z), 0, "smoke got through the ceiling at z={z}");
    }
    let before = (1..=5).map(|z| level(&world, z)).sum::<u32>();
    run(&mut world, 600);
    let after = (1..=5).map(|z| level(&world, z)).sum::<u32>();
    assert!(after < before && after > 0, "smoke should thin out slowly: {before} -> {after}");
}

#[test]
fn a_blast_sets_nearby_wood_alight() {
    let mut world = flat(20, 20, 4);
    let wood = world.mats.id("wood");
    for x in 6..=14 {
        for y in 6..=14 {
            world.set_tile(Pos::new(x, y, 1), Tile::Floor, wood);
        }
    }
    let mut things = Things::new();
    let blast = Blast { radius: 5.0, power: 40.0, heat: 0.0, ignite: 1.0 };
    let report = explode(&mut world, &mut things, Pos::new(10, 10, 2), &blast, &mut Rng::new(3));
    assert!(report.ignited > 0, "blast ignited nothing");
    assert!(world.fire.iter().any(|&f| f > 0), "no cell is burning");
    let far = world.idx(6, 6, 1);
    assert_eq!(world.fire[far], 0, "a cell outside the blast radius caught fire");
}

/// A 9 x 5 room on z = 1..2 with a roof at z = 3 and optionally a doorway on the west side.
fn lit_room(door: bool, roof: &str) -> World {
    let mut world = flat(24, 12, 6);
    ring(&mut world, (6, 3, 14, 7), (1, 2));
    let roof = world.mats.id(roof);
    for x in 5..=15 {
        for y in 2..=8 {
            world.set_tile(Pos::new(x, y, 3), Tile::Wall, roof);
        }
    }
    if door {
        for z in 1..=2 {
            world.set_tile(Pos::new(5, 5, z), Tile::Open, AIR);
        }
    }
    recompute_sky(&mut world);
    world
}

#[test]
fn sky_light_leaks_through_a_doorway_and_fades() {
    let sealed = lit_room(false, "granite");
    let open = lit_room(true, "granite");
    let at = |w: &World, x: i32| w.sky[w.idx(x, 5, 1)];
    assert_eq!(at(&sealed, 6), 0, "a sealed room should be dark");
    assert_eq!(at(&open, 4), SKY_MAX, "outside the door is full sky");
    let (near, far) = (at(&open, 6), at(&open, 14));
    assert!(near > 0, "no light reached just inside the door");
    assert!(far < near, "light should fade into the room: near {near}, far {far}");
}

#[test]
fn glass_and_ice_roofs_pass_sky_light_but_stone_stops_it() {
    let mid = |w: &World| w.sky[w.idx(10, 5, 1)];
    assert_eq!(mid(&lit_room(false, "granite")), 0);
    for roof in ["glass", "ice"] {
        let w = lit_room(false, roof);
        assert!(mid(&w) >= SKY_MAX - 1, "{roof} roof let only {} through", mid(&w));
        // Rain and wind still stop at the glass.
        assert!(!w.open_sky[w.idx(10, 5, 1)]);
    }
}

#[test]
fn lava_sets_adjacent_wood_alight() {
    let mut world = flat(12, 12, 4);
    let (lava, wood) = (world.mats.id("lava"), world.mats.id("wood"));
    world.set_fluid(Pos::new(5, 5, 1), lava, FULL);
    world.set_tile(Pos::new(6, 5, 1), Tile::Wall, wood);
    let target = world.idx(6, 5, 1);
    let mut sim = Sim::new(4);
    sim.fluids = false; // keep the lava in its cell
    let caught = (0..200u64).find(|&t| {
        sim.step(&mut world, t);
        world.fire[target] > 0
    });
    assert!(caught.is_some(), "wood next to lava never ignited (temp {})", world.temp[target]);
    assert!(world.temp[world.idx(5, 5, 1)] > 1000.0, "lava did not hold its temperature");
}

#[test]
fn burning_floor_over_empty_space_leaves_a_hole() {
    let mut world = flat(16, 16, 4);
    let wood = world.mats.id("wood");
    let over_air = Pos::new(4, 4, 2);
    let on_ground = Pos::new(10, 10, 1);
    world.set_tile(over_air, Tile::Floor, wood);
    world.set_tile(on_ground, Tile::Floor, wood);
    assert!(world.ignite(over_air) && world.ignite(on_ground));
    run(&mut world, 700);
    let (a, g) = (world.index(over_air).unwrap(), world.index(on_ground).unwrap());
    assert_eq!(world.fire[a], 0, "still burning");
    assert_eq!(world.tile[a], Tile::Open, "a burnt floor over open space should leave a hole");
    assert_eq!(world.tile[g], Tile::Floor, "a burnt floor on solid ground leaves ash");
    assert_eq!(world.mat[g], world.mats.id("ash"));
}

#[test]
fn fire_cannot_start_in_a_wet_cell() {
    let mut world = flat(12, 12, 4);
    let wood = world.mats.id("wood");
    // A liquid that neither boils nor freezes, so only wetness is at play.
    let brine = world.mats.add(Material { name: "brine".into(), kind: MatKind::Liquid { viscosity: 1 }, ..Default::default() });
    let (wet, dry) = (Pos::new(3, 3, 1), Pos::new(8, 8, 1));
    for p in [wet, dry] {
        world.set_tile(p, Tile::Floor, wood);
    }
    world.set_fluid(wet, brine, 12);
    assert!(!world.ignite(wet), "ignite() lit a wet floor");
    // Heat alone must not light it either.
    world.set_temp(wet, 600.0);
    world.set_temp(dry, 600.0);
    let mut sim = Sim::new(9);
    sim.fluids = false;
    let (w, d) = (world.idx(3, 3, 1), world.idx(8, 8, 1));
    let mut dry_lit = false;
    for t in 0..10 {
        sim.step(&mut world, t);
        assert_eq!(world.fire[w], 0, "wet floor caught fire at tick {t}");
        dry_lit |= world.fire[d] > 0;
    }
    assert!(dry_lit, "control: the dry floor at 600 C should have ignited");
}
