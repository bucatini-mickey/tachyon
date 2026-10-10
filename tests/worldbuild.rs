//! Atlas, blueprints and world generation.

use tachyon::atlas::{Atlas, Feature, River, GROUND, REGION};
use tachyon::blueprint::Blueprint;
use tachyon::prelude::*;
use tachyon::worldgen::{generate, BiomeDef, GenSettings};

fn settings() -> GenSettings {
    let mats = Materials::standard();
    let id = |n: &str| mats.find(n).unwrap();
    let def = |name: &str, surface: Option<MatId>, trees: f32| BiomeDef {
        name: name.into(),
        surface,
        subsoil: id("soil"),
        soil_depth: 2,
        trees,
        roughness: 1.0,
        puddles: 0.0,
    };
    GenSettings {
        biomes: vec![def("ocean", Some(id("sand")), 0.0), def("grassland", Some(id("grass")), 0.0), def("forest", Some(id("grass")), 0.1)],
        water: id("water"),
        bed: id("gravel"),
        trunk: id("wood"),
        leaves: id("leaves"),
        headroom: 8,
        seed: 3,
        materials: mats,
    }
}

/// A 6x6 atlas: sea round the edge, a plateau 8 m high in the middle, a
/// river from the plateau to the sea, and two features.
fn island() -> Atlas {
    let mut a = Atlas::new("test isle", 6, 6, &["granite", "limestone"], &["ocean", "grassland", "forest"], 6);
    for y in 1..5 {
        for x in 1..5 {
            let i = a.idx(x, y);
            a.elevation[i] = 8;
            a.biome[i] = if x == 4 { 2 } else { 1 };
            a.rock[i] = 1;
        }
    }
    a.rivers.push(River { width: 3, points: vec![(2, 2), (2, 4), (2, 5)] });
    a.features.push(Feature { x: 3 * REGION + 5, y: 2 * REGION + 7, z: GROUND, kind: "town".into(), label: "Saltmarsh".into() });
    a.features.push(Feature { x: 40, y: 50, z: 3, kind: "bp:hut".into(), label: "a hut".into() });
    a
}

#[test]
fn atlas_survives_a_round_trip_through_text() {
    let a = island();
    let text = a.to_text();
    let b = Atlas::from_text(&text).expect("parses");
    assert_eq!(a, b);
    assert!(text.contains("feature 101 71 - town Saltmarsh"));
}

#[test]
fn blueprint_round_trip_and_stamp() {
    let mats = Materials::standard();
    let (stone, wood) = (mats.id("stone block"), mats.id("wood"));
    let mut world = World::new(20, 20, 4, mats);
    let mut things = Things::new();
    let bed = things.add_kind(ThingKind::new("bed", 'Θ', Rgb(190, 70, 64), Layer::Furniture));
    let cat = things.add_kind(ThingKind::new("cat", 'c', Rgb::WHITE, Layer::Creature));
    for y in 2..7 {
        for x in 2..8 {
            let edge = x == 2 || x == 7 || y == 2 || y == 6;
            world.set_tile(Pos::new(x, y, 1), if edge { Tile::Wall } else { Tile::Floor }, if edge { stone } else { wood });
        }
    }
    things.spawn(bed, Pos::new(4, 4, 1));
    things.spawn(cat, Pos::new(5, 4, 1)); // creatures are not part of a blueprint

    let bp = Blueprint::capture("hut", &world, &things, Pos::new(2, 2, 1), 6, 5, 2);
    let text = bp.to_text();
    assert!(text.contains("key b floor wood | bed"), "bed should get its initial:\n{text}");
    let back = Blueprint::from_text(&text).expect("parses");
    assert_eq!(bp, back);

    let r = back.stamp(&mut world, &mut things, Pos::new(10, 10, 1));
    assert!(r.unknown_materials.is_empty() && r.unknown_kinds.is_empty());
    for y in 0..5 {
        for x in 0..6 {
            let (a, b) = (Pos::new(2 + x, 2 + y, 1), Pos::new(10 + x, 10 + y, 1));
            assert_eq!(world.tile(a), world.tile(b), "tile at {x},{y}");
            assert_eq!(world.mat[world.index(a).unwrap()], world.mat[world.index(b).unwrap()]);
        }
    }
    assert_eq!(things.at(Pos::new(12, 12, 1)).count(), 1, "the bed, and only the bed");
    assert!(things.at(Pos::new(13, 12, 1)).next().is_none(), "no cat copied");
}

#[test]
fn generated_island_has_sea_land_river_and_features() {
    let a = island();
    let set = settings();
    let g = generate(&a, &set, 0, 0, 6, 6);
    let w = &g.world;
    assert_eq!((w.w, w.h), (6 * REGION, 6 * REGION));

    // Open sea at the corner: full of water up to the level below sea level.
    let i = w.idx(5, 5, g.sea_z - 1);
    assert!(w.fluid[i] == FULL, "sea not full at the edge");
    assert_eq!(w.fluid[w.idx(5, 5, g.sea_z)], 0, "sea above sea level");

    // The middle of the plateau: floor 8 m up, give or take the noise.
    let (cx, cy) = (3 * REGION + 16, 3 * REGION + 16);
    let floor = (0..w.d).find(|&z| w.tile(Pos::new(cx, cy, z)).has_floor()).expect("ground in the middle");
    assert!((floor - (g.sea_z + 8)).abs() <= 2, "plateau at {} m, expected ~8", floor - g.sea_z);
    // Its rock is limestone, below the soil.
    let lime = w.mats.id("limestone");
    assert_eq!(w.mat[w.idx(cx, cy, floor - 4)], lime);

    // The river: water in a channel below the plateau's surface, near its path.
    let rx = 2 * REGION + 16;
    let wet = (3 * REGION..4 * REGION).filter(|&y| (rx - 8..rx + 8).any(|x| (0..w.d).any(|z| w.fluid[w.idx(x, y, z)] > 0))).count();
    assert!(wet > REGION as usize / 2, "river water along only {wet} rows");
    assert!(!g.sources.is_empty(), "the river needs a spring");

    // Features come back with local positions.
    let town = g.features.iter().find(|(f, _)| f.kind == "town").expect("town");
    assert_eq!((town.1.x, town.1.y), (3 * REGION + 5, 2 * REGION + 7));
    let hut = g.features.iter().find(|(f, _)| f.kind == "bp:hut").expect("hut");
    assert_eq!(hut.1.z, g.sea_z + 3);
}

#[test]
fn generation_is_deterministic_and_regions_line_up() {
    let a = island();
    let set = settings();
    let g1 = generate(&a, &set, 0, 0, 6, 6);
    let g2 = generate(&a, &set, 0, 0, 6, 6);
    assert_eq!(g1.world.tile, g2.world.tile);
    assert_eq!(g1.world.mat, g2.world.mat);
    // A sub-area generated on its own matches the same place in the whole.
    let g3 = generate(&a, &set, 2, 2, 2, 2);
    let (ox, oy) = (2 * REGION, 2 * REGION);
    let mut same = 0;
    let total = (2 * REGION * 2 * REGION) as usize;
    for y in 0..2 * REGION {
        for x in 0..2 * REGION {
            let col = |w: &World, x: i32, y: i32, sea: i32| (0..w.d).rev().find(|&z| w.tile(Pos::new(x, y, z)) != Tile::Open).map(|z| z - sea);
            if col(&g3.world, x, y, g3.sea_z) == col(&g1.world, ox + x, oy + y, g1.sea_z) {
                same += 1;
            }
        }
    }
    // Rivers carve against their whole path, so a few cells may differ.
    assert!(same * 100 / total >= 97, "only {same}/{total} columns match");
}
