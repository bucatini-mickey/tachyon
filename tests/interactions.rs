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

// ---- tach-002, second batch: viscosity, heat, lava, dynamic light, flames ----

use tachyon::sim::light::{gather_sources, LightMap, LightSource};

/// Cells holding `mat` on level `z`, and the widest reach from `centre`.
fn reach(world: &World, mat: MatId, z: i32, centre: (i32, i32)) -> (usize, i32) {
    let mut n = 0;
    let mut far = 0;
    for y in 0..world.h {
        for x in 0..world.w {
            let i = world.idx(x, y, z);
            if world.fluid[i] > 0 && world.fluid_mat[i] == mat {
                n += 1;
                far = far.max((x - centre.0).abs().max((y - centre.1).abs()));
            }
        }
    }
    (n, far)
}

#[test]
fn lava_spreads_more_slowly_than_water() {
    let spill = |name: &str| {
        let mut world = flat(31, 31, 4);
        let m = world.mats.id(name);
        world.set_fluid(Pos::new(15, 15, 1), m, FULL);
        let mut sim = Sim::new(5);
        sim.heat = false; // lava must not freeze or burn; this is about flow
        for t in 0..12 {
            sim.step(&mut world, t);
        }
        reach(&world, m, 1, (15, 15))
    };
    let (water_cells, water_far) = spill("water");
    let (lava_cells, lava_far) = spill("lava");
    assert!(water_far > lava_far, "water reached {water_far}, lava {lava_far}");
    assert!(water_cells > lava_cells, "water covers {water_cells} cells, lava {lava_cells}");
}

/// A sealed, roofed room so neither the sky nor wind interferes.
fn sealed_room() -> World {
    let mut world = flat(11, 11, 10);
    ring(&mut world, (2, 2, 8, 8), (1, 8));
    let granite = world.mats.id("granite");
    for x in 1..=9 {
        for y in 1..=9 {
            world.set_tile(Pos::new(x, y, 9), Tile::Wall, granite);
        }
    }
    recompute_sky(&mut world);
    world
}

#[test]
fn hot_air_rises_faster_than_it_sinks() {
    let mut world = sealed_room();
    let c = Pos::new(5, 5, 5);
    world.set_temp(c, 300.0);
    let mut sim = Sim::new(1);
    sim.fluids = false;
    sim.gases = false;
    for t in 0..6 {
        sim.step(&mut world, t);
    }
    let above = world.temp[world.index(c.offset(0, 0, 1)).unwrap()];
    let below = world.temp[world.index(c.offset(0, 0, -1)).unwrap()];
    let beside = world.temp[world.index(c.offset(1, 0, 0)).unwrap()];
    assert!(above > below * 1.5, "above {above:.1} C vs below {below:.1} C");
    assert!(above > beside, "heat should favour up over sideways: {above:.1} vs {beside:.1}");
}

#[test]
fn open_air_drifts_to_the_outdoor_temperature_but_a_roofed_room_does_not() {
    let mut world = flat(24, 12, 6);
    let granite = world.mats.id("granite");
    ring(&mut world, (14, 2, 21, 9), (1, 3));
    for x in 13..=22 {
        for y in 1..=10 {
            world.set_tile(Pos::new(x, y, 4), Tile::Wall, granite);
        }
    }
    recompute_sky(&mut world);
    world.ambient = 0.0;
    world.ground_temp = 30.0;
    for t in world.temp.iter_mut() {
        *t = 30.0; // walls too, or they would cool the room from the edges
    }
    world.wake.wake_all();
    let mut sim = Sim::new(2);
    sim.fluids = false;
    sim.gases = false;
    for t in 0..300 {
        sim.step(&mut world, t);
    }
    let outside = world.temp[world.index(Pos::new(4, 5, 1)).unwrap()];
    let inside = world.temp[world.index(Pos::new(17, 5, 1)).unwrap()];
    assert!(outside < 12.0, "open air only cooled to {outside:.1} C (ambient 0)");
    assert!(inside > outside + 8.0, "roofed room fell to {inside:.1} C, open air {outside:.1} C");
}

#[test]
fn lava_holds_its_temperature_and_glows() {
    let mut world = flat(12, 12, 4);
    let lava = world.mats.id("lava");
    let p = Pos::new(6, 6, 1);
    world.set_fluid(p, lava, FULL);
    world.set_temp(p, 20.0);
    let mut sim = Sim::new(3);
    sim.fluids = false;
    for t in 0..5 {
        sim.step(&mut world, t);
    }
    let t = world.temp[world.index(p).unwrap()];
    assert!((t - 1200.0).abs() < 1.0, "lava sits at {t} C, not its own 1200");
    let (mut buf, mut map) = (Vec::new(), LightMap::new());
    gather_sources(&world, &Things::new(), 0, 0, 0, 12, 12, 4, 0, &mut buf);
    assert!(buf.iter().any(|s| s.pos == p), "lava gave no light source");
    map.compute(&world, 0, 0, 0, 12, 12, 4, &buf);
    let near = map.get(7, 6, 1);
    let far = map.get(11, 6, 1);
    assert!(near[0] > far[0] && far[0] > 0, "light should fade with distance: {near:?} -> {far:?}");
    assert!(near[0] > near[2], "lava light should be red/orange, got {near:?}");
}

#[test]
fn rock_with_a_melting_point_turns_to_lava() {
    let mut world = flat(10, 10, 4);
    let lava = world.mats.id("lava");
    let slag = world.mats.add(Material {
        name: "slag".into(),
        melt: Some((800.0, lava)),
        ..Default::default()
    });
    let (a, b) = (Pos::new(3, 3, 1), Pos::new(7, 7, 1));
    world.set_tile(a, Tile::Wall, slag);
    world.set_tile(b, Tile::Wall, slag);
    world.set_temp(a, 900.0);
    world.set_temp(b, 300.0);
    let mut sim = Sim::new(4);
    sim.fluids = false;
    sim.step(&mut world, 0);
    let (ia, ib) = (world.index(a).unwrap(), world.index(b).unwrap());
    assert!(!world.tile[ia].is_wall() && world.fluid_mat[ia] == lava && world.fluid[ia] > 0, "hot slag stayed solid");
    assert!(world.tile[ib].is_wall(), "cool slag melted");
}

#[test]
fn metal_conducts_heat_far_better_than_wood() {
    let mut world = flat(20, 12, 4);
    let metal = world.mats.add(Material { name: "bar_metal".into(), conduct: 0.10, heat_cap: 2.0, ..Default::default() });
    let wood = world.mats.add(Material { name: "bar_wood".into(), conduct: 0.005, heat_cap: 2.0, ..Default::default() });
    for x in 2..12 {
        world.set_tile(Pos::new(x, 3, 1), Tile::Wall, metal);
        world.set_tile(Pos::new(x, 8, 1), Tile::Wall, wood);
    }
    for x in 2..5 {
        world.set_temp(Pos::new(x, 3, 1), 400.0);
        world.set_temp(Pos::new(x, 8, 1), 400.0);
    }
    let mut sim = Sim::new(6);
    sim.fluids = false;
    sim.gases = false;
    for t in 0..25 {
        sim.step(&mut world, t);
    }
    let at = |p: Pos| world.temp[world.index(p).unwrap()];
    let (m, w) = (at(Pos::new(6, 3, 1)), at(Pos::new(6, 8, 1)));
    assert!(m > w + 5.0, "metal bar {m:.1} C two cells past the heat vs wood {w:.1} C");
}

fn torch_kind(things: &mut Things) -> KindId {
    things.add_kind(ThingKind::new("torch", '!', Rgb(255, 210, 110), Layer::Furniture).light(Rgb(255, 176, 92), false).flame(Flame::Exposed))
}

#[test]
fn dynamic_light_comes_from_flames_and_stops_at_walls() {
    let mut world = flat(14, 9, 4);
    let granite = world.mats.id("granite");
    for y in 0..9 {
        for z in 1..4 {
            world.set_tile(Pos::new(8, y, z), Tile::Wall, granite); // floor to sky, no way over
        }
    }
    let mut things = Things::new();
    let torch = torch_kind(&mut things);
    let id = things.spawn(torch, Pos::new(5, 4, 1));
    let (mut buf, mut map) = (Vec::new(), LightMap::new());

    things.get_mut(id).unwrap().lit = false;
    gather_sources(&world, &things, 0, 0, 0, 14, 9, 4, 0, &mut buf);
    assert!(buf.is_empty(), "an unlit torch gave light");

    things.get_mut(id).unwrap().lit = true;
    gather_sources(&world, &things, 0, 0, 0, 14, 9, 4, 0, &mut buf);
    assert_eq!(buf.len(), 1);
    map.compute(&world, 0, 0, 0, 14, 9, 4, &buf);
    assert!(map.get(7, 4, 1)[0] > 0, "no light reached the wall");
    assert_eq!(map.get(10, 4, 1), [0, 0, 0], "light passed through a wall");

    // Burning cells light up too.
    let i = world.idx(11, 4, 1);
    world.fire[i] = 5;
    gather_sources(&world, &things, 0, 0, 0, 14, 9, 4, 0, &mut buf);
    assert!(buf.iter().any(|s: &LightSource| s.pos == Pos::new(11, 4, 1)), "fire gave no light");
}

fn lamp_field() -> (World, Things, ThingId) {
    let mut world = flat(10, 10, 4);
    let grass = world.mats.id("grass");
    for y in 0..10 {
        for x in 0..10 {
            world.set_tile(Pos::new(x, y, 1), Tile::Floor, grass);
        }
    }
    let mut things = Things::new();
    let torch = torch_kind(&mut things);
    let id = things.spawn(torch, Pos::new(5, 5, 1));
    (world, things, id)
}

#[test]
fn a_submerged_flame_goes_dark_and_gives_no_light() {
    let (mut world, mut things, id) = lamp_field();
    things.get_mut(id).unwrap().lit = true;
    let water = world.mats.id("water");
    world.set_fluid(Pos::new(5, 5, 1), water, FULL);
    let mut rng = Rng::new(1);
    tachyon::sim::flames::step(&world, &mut things, &mut rng);
    let mut buf = Vec::new();
    gather_sources(&world, &things, 0, 0, 0, 10, 10, 4, 0, &mut buf);
    assert!(!things.get(id).unwrap().lit);
    assert!(buf.is_empty(), "a flame under water still lit the room");
}

#[test]
fn wet_flames_dry_faster_when_warm() {
    let (mut world, mut things, a) = lamp_field();
    let torch = things.get(a).unwrap().kind;
    let b = things.spawn(torch, Pos::new(2, 2, 1));
    for (id, temp) in [(a, 60.0), (b, 15.0)] {
        let th = things.get_mut(id).unwrap();
        th.lit = false;
        th.wet = tachyon::sim::flames::SOAKED;
        let i = world.index(th.pos()).unwrap();
        world.temp[i] = temp;
    }
    let mut rng = Rng::new(1);
    for _ in 0..100 {
        tachyon::sim::flames::step(&world, &mut things, &mut rng);
    }
    let (warm, cold) = (things.get(a).unwrap().wet, things.get(b).unwrap().wet);
    assert!(warm < cold, "warm torch still {warm} wet, cold one {cold}");
    assert!(cold < tachyon::sim::flames::SOAKED, "a cold torch should still dry, slowly");
}

#[test]
fn fire_relights_a_dry_flame_but_not_a_soaked_one() {
    let (mut world, mut things, dry) = lamp_field();
    let torch = things.get(dry).unwrap().kind;
    let soaked = things.spawn(torch, Pos::new(2, 2, 1));
    for id in [dry, soaked] {
        things.get_mut(id).unwrap().lit = false;
    }
    things.get_mut(soaked).unwrap().wet = tachyon::sim::flames::SOAKED;
    for p in [Pos::new(5, 5, 1), Pos::new(2, 2, 1)] {
        let i = world.index(p).unwrap();
        world.fire[i] = 50;
    }
    let mut rng = Rng::new(1);
    tachyon::sim::flames::step(&world, &mut things, &mut rng);
    assert!(things.get(dry).unwrap().lit, "a dry torch beside fire did not relight");
    assert!(!things.get(soaked).unwrap().lit, "a soaked torch relit");
}
