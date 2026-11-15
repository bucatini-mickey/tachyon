//! Second batch of docs/INTERACTIONS.md rows that had code but no test (tach-002, 15 Nov).

use tachyon::atlas::{Atlas, REGION};
use tachyon::movement;
use tachyon::life::path;
use tachyon::prelude::*;
use tachyon::projectile::explode;
use tachyon::sim::light::{gather_sources, LightSource};
use tachyon::sim::Sim;
use tachyon::weather::Weather;
use tachyon::worldgen::{generate, BiomeDef, GenSettings};

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

#[test]
fn a_thin_film_evaporates_when_warm_but_a_pool_does_not() {
    let mut world = flat(8, 8, 3);
    let water = world.mats.id("water");
    for i in 0..world.len() {
        world.temp[i] = 25.0;
    }
    world.set_fluid(Pos::new(2, 2, 1), water, 1); // a film
    // A pool in a stone pit, so it cannot spread into a film.
    for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        world.set_tile(Pos::new(6 + dx, 6 + dy, 1), Tile::Wall, world.mats.id("granite"));
    }
    world.set_fluid(Pos::new(6, 6, 1), water, 40);
    let mut sim = Sim::new(11);
    sim.heat = false; // keep the cells warm
    for t in 0..6000 {
        sim.step(&mut world, t);
    }
    assert_eq!(world.fluid[world.idx(2, 2, 1)], 0, "the film is still there");
    assert_eq!(world.total_fluid(water), 40, "the pool lost or gained water");
}

#[test]
fn rain_adds_water_to_open_floors() {
    let mut world = flat(20, 20, 4);
    let water = world.mats.id("water");
    let mut sim = Sim::new(5);
    sim.heat = false;
    for t in 0..100 {
        sim.step(&mut world, t);
    }
    assert_eq!(world.total_fluid(water), 0, "water appeared with no rain");
    world.rain = 1.0;
    for t in 100..400 {
        sim.step(&mut world, t);
    }
    // 400 columns x 0.001 drops x 300 ticks = about 120 units, minus the odd film that evaporates.
    let got = world.total_fluid(water);
    assert!((60..=180).contains(&got), "a downpour gave {got} units of water in 300 ticks");
}

#[test]
fn wind_gusts_and_veers_but_is_deterministic() {
    let mut w = Weather::new(9);
    w.set_wind(6.0, 0.0);
    let again = w.clone();
    let mut speeds = Vec::new();
    for s in 0..600 {
        let t = s as f64 * 3.0;
        let (x, y) = w.wind_at(t);
        assert_eq!((x, y), again.wind_at(t), "the same time gave a different wind");
        let speed = (x * x + y * y).sqrt();
        assert!(speed <= 6.0 * 1.5 + 1e-3, "gust of {speed} m/s is beyond gustiness");
        assert!(y.atan2(x).abs() <= w.veer + 1e-3, "the wind veered further than allowed");
        speeds.push(speed);
    }
    let (lo, hi) = speeds.iter().fold((f32::MAX, 0f32), |(l, h), &s| (l.min(s), h.max(s)));
    assert!(hi - lo > 1.0, "wind never gusted: {lo}..{hi}");
    // A different seed blows differently.
    let mut o = Weather::new(10);
    o.set_wind(6.0, 0.0);
    assert!((0..100).any(|s| o.wind_at(s as f64 * 3.0) != w.wind_at(s as f64 * 3.0)));
    // No gusts and no veer: a steady wind.
    let mut steady = Weather::new(1);
    steady.set_wind(4.0, 1.0);
    steady.gustiness = 0.0;
    steady.veer = 0.0;
    let (a, b) = (steady.wind_at(5.0), steady.wind_at(500.0));
    assert!((a.0 - b.0).abs() < 1e-4 && (a.1 - b.1).abs() < 1e-4);
}

#[test]
fn fire_light_flickers_in_steps_within_bounds() {
    let mut world = flat(8, 8, 3);
    let i = world.idx(4, 4, 1);
    world.fire[i] = 5;
    let things = Things::new();
    let red = |tick: u64| -> u8 {
        let mut out: Vec<LightSource> = Vec::new();
        gather_sources(&world, &things, 0, 0, 0, 8, 8, 3, tick, &mut out);
        assert_eq!(out.len(), 1);
        out[0].color.0
    };
    let reds: Vec<u8> = (0..80).map(red).collect();
    assert!(reds.iter().max() > reds.iter().min(), "the flame never flickered");
    for &r in &reds {
        assert!((178..=255).contains(&r), "flicker went to {r}, outside 70% to 100%");
    }
    for t in (0..80).step_by(2) {
        assert_eq!(red(t), red(t + 1), "flicker changed inside a two-tick step at {t}");
    }
}

#[test]
fn swimming_starts_at_half_depth_and_costs_more_to_path_through() {
    let mut world = flat(10, 10, 6);
    let water = world.mats.id("water");
    let things = Things::new();
    // Mid-air has nothing to stand on, but deep enough water can be swum in.
    let air = Pos::new(5, 5, 4);
    assert!(!movement::can_occupy(&world, &things, air));
    world.set_fluid(air, water, movement::SWIM_DEPTH - 1);
    assert!(!movement::can_occupy(&world, &things, air), "waded water held a swimmer up");
    world.set_fluid(air, water, movement::SWIM_DEPTH);
    assert!(movement::can_occupy(&world, &things, air), "deep water did not hold a swimmer");
    // On the floor, wading costs 10 a step and swimming three times that.
    let (shallow, deep) = (Pos::new(3, 5, 1), Pos::new(3, 6, 1));
    world.set_fluid(shallow, water, movement::SWIM_DEPTH - 1);
    world.set_fluid(deep, water, movement::SWIM_DEPTH);
    let mut out = Vec::new();
    path::neighbours(&world, &things, Pos::new(2, 5, 1), &mut out);
    let cost = |q: Pos| out.iter().find(|&&(p, _)| p == q).map(|&(_, c)| c);
    assert_eq!(cost(shallow), Some(10));
    assert_eq!(cost(deep), Some(42), "a diagonal swim should cost 14 x 3");
}

#[test]
fn paths_go_round_fire_and_scalding_air_and_fail_when_walled_in() {
    let mut world = flat(14, 11, 4);
    let things = Things::new();
    let (from, to) = (Pos::new(2, 5, 1), Pos::new(11, 5, 1));
    let straight = path::find(&world, &things, from, to, 0, path::DEFAULT_BUDGET).expect("open floor");
    assert!(straight.iter().any(|p| p.x == 6 && p.y == 5), "the direct line was not taken");

    // A fire in the way: the route bends round it.
    let i = world.idx(6, 5, 1);
    world.fire[i] = 5;
    let route = path::find(&world, &things, from, to, 0, path::DEFAULT_BUDGET).expect("a way round");
    assert!(route.iter().all(|&p| p != Pos::new(6, 5, 1)), "the route walked through fire");
    assert!(route.len() >= straight.len());

    // Scalding air counts the same.
    let j = world.idx(6, 4, 1);
    world.temp[j] = 80.0;
    assert!(path::hazard(&world, Pos::new(6, 4, 1)));
    let route = path::find(&world, &things, from, to, 0, path::DEFAULT_BUDGET).expect("a way round");
    assert!(route.iter().all(|&p| p != Pos::new(6, 4, 1) && p != Pos::new(6, 5, 1)));

    // A wall of fire across the whole room: no way.
    for y in 0..11 {
        let k = world.idx(6, y, 1);
        world.fire[k] = 5;
    }
    assert!(path::find(&world, &things, from, to, 0, path::DEFAULT_BUDGET).is_none(), "a path crossed a wall of fire");
}

// ---- world generation -------------------------------------------------------

fn gen_settings(biomes: &[(&str, Option<&str>, f32, f32)]) -> GenSettings {
    let mats = Materials::standard();
    let id = |n: &str| mats.find(n).unwrap();
    GenSettings {
        biomes: biomes
            .iter()
            .map(|&(name, surface, trees, puddles)| BiomeDef {
                name: name.into(),
                surface: surface.map(id),
                subsoil: id("soil"),
                soil_depth: 2,
                trees,
                roughness: 0.0,
                puddles,
            })
            .collect(),
        water: id("water"),
        bed: id("gravel"),
        trunk: id("wood"),
        leaves: id("leaves"),
        headroom: 8,
        seed: 4,
        materials: mats,
    }
}

/// A 6x6 atlas with sea round the edge and a plateau of the given height inside,
/// biome index `biome(x)` for atlas column x.
fn plateau(height: i16, biome: impl Fn(i32) -> u8, names: &[&str]) -> Atlas {
    let mut a = Atlas::new("test", 6, 6, &["granite"], names, 6);
    for y in 1..5 {
        for x in 1..5 {
            let i = a.idx(x, y);
            a.elevation[i] = height;
            a.biome[i] = biome(x);
            a.rock[i] = 0;
        }
    }
    a
}

/// The z of the topmost non-empty cell in a column.
fn top(w: &World, x: i32, y: i32) -> i32 {
    (0..w.d).rev().find(|&z| w.tile(Pos::new(x, y, z)) != Tile::Open).unwrap()
}

#[test]
fn each_biome_gets_its_own_surface_and_steep_ground_shows_rock() {
    let set = gen_settings(&[("ocean", Some("sand"), 0.0, 0.0), ("grassland", Some("grass"), 0.0, 0.0), ("dunes", Some("sand"), 0.0, 0.0), ("scree", None, 0.0, 0.0)]);
    let a = plateau(120, |x| x as u8, &["ocean", "grassland", "dunes", "scree", "grassland"]);
    let g = generate(&a, &set, 0, 0, 6, 6);
    let w = &g.world;
    let (grass, sand, granite) = (w.mats.id("grass"), w.mats.id("sand"), w.mats.id("granite"));
    let at = |bx: i32| {
        let (x, y) = (bx * REGION + REGION / 2, 3 * REGION + REGION / 2);
        let z = top(w, x, y);
        (w.tile(Pos::new(x, y, z)), w.mat[w.idx(x, y, z)], z - g.sea_z)
    };
    assert_eq!(at(1), (Tile::Floor, grass, 120));
    assert_eq!(at(2), (Tile::Floor, sand, 120));
    assert_eq!(at(3), (Tile::Floor, granite, 120), "a biome with no surface should show bare rock");
    // Where the plateau falls steeply to the sea the ground is bare rock, not grass.
    let y = 3 * REGION + 16;
    let mut n = 0;
    for x in 1..3 * REGION {
        let z = top(w, x, y);
        let steep = [1, -1].iter().any(|&dx| (top(w, x + dx, y) - z).abs() >= 3);
        if steep && z > g.sea_z {
            n += 1;
            assert_eq!(w.mat[w.idx(x, y, z)], granite, "not bare rock on a cliff at x={x}");
        }
    }
    assert!(n > 0, "the test found no cliff");
}

#[test]
fn forest_gets_trees_and_marsh_gets_shallow_puddles_and_nothing_else_does() {
    let set = gen_settings(&[("ocean", Some("sand"), 0.0, 0.0), ("grassland", Some("grass"), 0.0, 0.0), ("forest", Some("grass"), 0.3, 0.0), ("marsh", Some("mud"), 0.0, 0.6)]);
    let a = plateau(3, |x| x as u8, &["ocean", "grassland", "forest", "marsh", "marsh"]);
    let g = generate(&a, &set, 0, 0, 6, 6);
    let w = &g.world;
    let (wood, leaves, water) = (w.mats.id("wood"), w.mats.id("leaves"), w.mats.id("water"));
    // Atlas column 1 is grassland, 2 forest, 3 marsh; rows 2 and 3 are inland.
    let cells = |bx: i32| -> Vec<(i32, i32, i32)> {
        let mut v = Vec::new();
        for x in bx * REGION + 10..(bx + 1) * REGION - 10 {
            for y in 2 * REGION + 10..4 * REGION - 10 {
                for z in 0..w.d {
                    v.push((x, y, z));
                }
            }
        }
        v
    };
    let trunks = |bx: i32| cells(bx).into_iter().filter(|&(x, y, z)| w.tile(Pos::new(x, y, z)).is_wall() && w.mat[w.idx(x, y, z)] == wood).collect::<Vec<_>>();
    assert!(trunks(1).is_empty(), "trees in grassland");
    let forest = trunks(2);
    assert!(forest.len() > 20, "only {} trunks in the forest", forest.len());
    for &(x, y, z) in &forest {
        assert_eq!(w.mat[w.idx(x, y, z + 2)], leaves, "trunk at {x},{y} has no crown");
    }

    let puddles = |bx: i32| {
        let mut n = 0;
        for (x, y, z) in cells(bx) {
            let i = w.idx(x, y, z);
            if w.fluid[i] > 0 {
                assert_eq!(w.fluid_mat[i], water);
                assert!(w.fluid[i] < movement::SWIM_DEPTH, "a puddle you could swim in");
                n += 1;
            }
        }
        n
    };
    assert_eq!(puddles(1), 0, "puddles in grassland");
    assert_eq!(puddles(2), 0, "puddles in forest");
    assert!(puddles(3) > 20, "only {} puddle cells in the marsh", puddles(3));
}

#[test]
fn ramps_join_every_one_level_step_on_dry_land() {
    let set = gen_settings(&[("ocean", Some("sand"), 0.0, 0.0), ("hills", Some("grass"), 0.0, 0.0)]);
    let mut a = plateau(2, |_| 1, &["ocean", "hills"]);
    // Stairs of height: each atlas column one metre higher than the last.
    for y in 1..5 {
        for x in 1..5 {
            let i = a.idx(x, y);
            a.elevation[i] = (x + 1) as i16;
        }
    }
    let set = GenSettings { biomes: set.biomes.iter().cloned().map(|b| BiomeDef { roughness: 1.0, ..b }).collect(), ..set };
    let g = generate(&a, &set, 0, 0, 6, 6);
    let w = &g.world;
    let mut steps = 0;
    for y in 1..w.h - 1 {
        for x in 1..w.w - 1 {
            let z = top(w, x, y);
            if z <= g.sea_z || w.fluid[w.idx(x, y, z)] > 0 || !matches!(w.tile(Pos::new(x, y, z)), Tile::Floor | Tile::Ramp) {
                continue;
            }
            let rises = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|&(dx, dy)| {
                let (nx, ny) = (x + dx, y + dy);
                top(w, nx, ny) == z + 1 && w.fluid[w.idx(nx, ny, z + 1)] == 0
            });
            if rises {
                steps += 1;
                assert_eq!(w.tile(Pos::new(x, y, z)), Tile::Ramp, "no ramp at ({x},{y}) where ground steps up one level");
            }
        }
    }
    assert!(steps > 10, "only {steps} one-level steps found, the test is not exercising ramps");
}

// ---- projectiles ------------------------------------------------------------

#[test]
fn a_blast_breaks_glass_that_granite_survives() {
    let mut world = flat(20, 20, 4);
    let (glass, granite) = (world.mats.id("glass"), world.mats.id("granite"));
    let (g, r) = (Pos::new(12, 10, 2), Pos::new(8, 10, 2));
    world.set_tile(g, Tile::Wall, glass);
    world.set_tile(r, Tile::Wall, granite);
    let mut things = Things::new();
    let blast = Blast { radius: 5.0, power: 40.0, heat: 0.0, ignite: 0.0 };
    let report = explode(&mut world, &mut things, Pos::new(10, 10, 2), &blast, &mut Rng::new(3));
    assert!(report.broken >= 1);
    assert!(!world.tile(g).is_wall(), "the pane survived the blast");
    assert!(world.tile(r).is_wall(), "granite broke at the same range");
}
