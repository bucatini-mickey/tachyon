//! The sandbox test map: rolling ground, a lake, a stone house with an upper
//! floor and a cellar, stairs down to a lava cave, a wooden hut (to burn), a
//! watchtower, a well, a pit, trees, villagers with daily routines, a dog, a
//! cat, rats in the cellar, sheep in the fields, and a canal running out
//! of the lake to a sluice, with a raft on the lake and a bridge over the canal.
//!
//! Everything here uses only the public engine API, so it doubles as an
//! example of how a game builds a world.

use std::collections::VecDeque;
use tachyon::noise::fbm2;
use tachyon::life::species;
use tachyon::prelude::*;

/// Ground level of the village plateau, and the water table.
pub const G: i32 = 5;

pub struct Ids {
    pub granite: MatId,
    pub soil: MatId,
    pub sand: MatId,
    pub grass: MatId,
    pub wood: MatId,
    pub leaves: MatId,
    pub block: MatId,
    pub slate: MatId,
    pub glass: MatId,
    pub water: MatId,
    pub lava: MatId,
    pub smoke: MatId,
    pub mist: MatId,

    pub you: KindId,
    pub you_lit: KindId,
    pub person: KindId,
    pub dog: KindId,
    pub cat: KindId,
    pub door: KindId,
    pub table: KindId,
    pub chair: KindId,
    pub bed: KindId,
    pub chest: KindId,
    pub shelf: KindId,
    pub barrel: KindId,
    pub torch: KindId,
    pub brazier: KindId,
    pub candle: KindId,
    pub mug: KindId,
    pub bread: KindId,
    pub book: KindId,
    pub coins: KindId,
    pub hay: KindId,
    pub raft: KindId,
    pub bridge: KindId,
    pub firebolt: KindId,
    pub flash: KindId,
}

pub struct Built {
    pub scene: Scene,
    pub ids: Ids,
    pub player: ThingId,
}

/// The canal: from inside the lake, east to the sluice (all inclusive).
pub const CANAL: (i32, i32, i32, i32) = (38, 64, 62, 66);

fn kinds(t: &mut Things) -> [KindId; 24] {
    let brown = Rgb(176, 124, 72);
    [
        t.add_kind(ThingKind::new("you", '@', Rgb(255, 255, 255), Layer::Creature).blocking()),
        t.add_kind(ThingKind::new("you, with a lantern", '@', Rgb(255, 240, 200), Layer::Creature).blocking().light(Rgb(230, 190, 120), true).flame(Flame::Sheltered)),
        t.add_kind(ThingKind::new("villager", '@', Rgb(200, 200, 200), Layer::Creature).blocking()),
        t.add_kind(ThingKind::new("dog", 'd', Rgb(170, 120, 64), Layer::Creature).blocking()),
        t.add_kind(ThingKind::new("cat", 'c', Rgb(210, 210, 200), Layer::Creature).blocking()),
        t.add_kind(ThingKind::new("door", '◘', brown, Layer::Furniture).blocking().door('\'')),
        t.add_kind(ThingKind::new("table", 'π', brown, Layer::Furniture).surface()),
        t.add_kind(ThingKind::new("chair", '╥', brown, Layer::Furniture)),
        t.add_kind(ThingKind::new("bed", 'Θ', Rgb(190, 70, 64), Layer::Furniture).bed()),
        t.add_kind(ThingKind::new("chest", '■', Rgb(150, 100, 52), Layer::Furniture).surface()),
        t.add_kind(ThingKind::new("bookshelf", '≡', brown, Layer::Furniture).blocking().surface()),
        t.add_kind(ThingKind::new("barrel", 'ö', Rgb(150, 104, 56), Layer::Furniture).blocking()),
        t.add_kind(ThingKind::new("torch", '¡', Rgb(255, 210, 110), Layer::Furniture).light(Rgb(255, 176, 92), true).flame(Flame::Exposed)),
        t.add_kind(ThingKind::new("brazier", 'Ω', Rgb(230, 130, 50), Layer::Furniture).blocking().light(Rgb(255, 150, 60), true).flame(Flame::Sheltered)),
        t.add_kind(ThingKind::new("candle", 'i', Rgb(250, 240, 200), Layer::Item).light(Rgb(170, 130, 70), true).flame(Flame::Exposed)),
        t.add_kind(ThingKind::new("mug", '!', Rgb(200, 205, 225), Layer::Item)),
        t.add_kind(ThingKind::new("loaf of bread", '%', Rgb(214, 170, 92), Layer::Item).food(0.4, false)),
        t.add_kind(ThingKind::new("book", '=', Rgb(160, 64, 64), Layer::Item)),
        t.add_kind(ThingKind::new("coins", '$', Rgb(240, 210, 64), Layer::Item)),
        t.add_kind(ThingKind::new("hay bale", '"', Rgb(220, 200, 100), Layer::Furniture)),
        t.add_kind(ThingKind::new("raft", '=', Rgb(190, 140, 80), Layer::Furniture).deck()),
        t.add_kind(ThingKind::new("plank bridge", '=', Rgb(150, 110, 70), Layer::Furniture).deck()),
        t.add_kind(ThingKind::new("firebolt", '*', Rgb(255, 200, 90), Layer::Effect).light(Rgb(255, 150, 50), true)),
        t.add_kind(ThingKind::new("blast", '☼', Rgb(255, 245, 200), Layer::Effect).light(Rgb(255, 230, 170), false)),
    ]
}

fn smooth01(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn build(w: i32, h: i32, d: i32, seed: u64) -> Built {
    assert!(w >= 150 && h >= 95 && d >= 12, "the sandbox needs at least 150x95x12");
    let mats = Materials::standard();
    let mut things = Things::new();
    let k = kinds(&mut things);
    let ids = Ids {
        granite: mats.id("granite"),
        soil: mats.id("soil"),
        sand: mats.id("sand"),
        grass: mats.id("grass"),
        wood: mats.id("wood"),
        leaves: mats.id("leaves"),
        block: mats.id("stone block"),
        slate: mats.id("slate"),
        glass: mats.id("glass"),
        water: mats.id("water"),
        lava: mats.id("lava"),
        smoke: mats.id("smoke"),
        mist: mats.id("mist"),
        you: k[0],
        you_lit: k[1],
        person: k[2],
        dog: k[3],
        cat: k[4],
        door: k[5],
        table: k[6],
        chair: k[7],
        bed: k[8],
        chest: k[9],
        shelf: k[10],
        barrel: k[11],
        torch: k[12],
        brazier: k[13],
        candle: k[14],
        mug: k[15],
        bread: k[16],
        book: k[17],
        coins: k[18],
        hay: k[19],
        raft: k[20],
        bridge: k[21],
        firebolt: k[22],
        flash: k[23],
    };
    let mut world = World::new(w, h, d, mats);
    let s = seed as u32;
    // Feature coordinates are laid out for a 160x100 map; centre them.
    let (ox, oy) = (w / 2 - 80, h / 2 - 50);
    let at = |x: i32, y: i32| (x + ox, y + oy);

    // ---- Heightmap --------------------------------------------------------
    let mut hm = vec![0i32; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            let n = fbm2(s, x as f32 / 30.0, y as f32 / 30.0, 4);
            let mut g = G as f32 + (n - 0.5) * 16.0;
            let (vx, vy) = at(80, 50);
            let (dx, dy) = ((x - vx) as f32 / 46.0, (y - vy) as f32 / 30.0);
            let flat = smooth01((1.2 - (dx * dx + dy * dy).sqrt()) / 0.35);
            g = g * (1.0 - flat) + G as f32 * flat;
            let (lx, ly) = at(30, 74);
            let (dx, dy) = ((x - lx) as f32 / 18.0, (y - ly) as f32 / 13.0);
            let lr = (dx * dx + dy * dy).sqrt();
            if lr < 1.0 {
                g -= (1.0 - lr) * 5.0;
            }
            hm[(y * w + x) as usize] = (g.round() as i32).clamp(1, d - 4);
        }
    }

    // ---- Ground -----------------------------------------------------------
    for y in 0..h {
        for x in 0..w {
            let g = hm[(y * w + x) as usize];
            let shore = g <= G
                && [(0, 1), (1, 0), (0, -1), (-1, 0)].iter().any(|&(dx, dy)| {
                    let (nx, ny) = (x + dx, y + dy);
                    nx >= 0 && ny >= 0 && nx < w && ny < h && hm[(ny * w + nx) as usize] < G
                });
            for z in 0..d {
                let p = Pos::new(x, y, z);
                if z < g {
                    world.set_tile(p, Tile::Wall, if z < g - 2 { ids.granite } else { ids.soil });
                } else if z == g {
                    world.set_tile(p, Tile::Floor, if g < G || shore { ids.sand } else { ids.grass });
                }
            }
            for z in g..G {
                world.set_fluid(Pos::new(x, y, z), ids.water, FULL);
            }
        }
    }
    // Ramps where the ground steps up by one.
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let g = hm[(y * w + x) as usize];
            if g < G {
                continue;
            }
            let up = [(0, 1), (1, 0), (0, -1), (-1, 0)].iter().any(|&(dx, dy)| hm[((y + dy) * w + x + dx) as usize] == g + 1);
            if up {
                let p = Pos::new(x, y, g);
                let m = world.mat[world.idx(x, y, g)];
                world.set_tile(p, Tile::Ramp, m);
            }
        }
    }

    // ---- Helpers ----------------------------------------------------------
    let set = |world: &mut World, x: i32, y: i32, z: i32, t: Tile, m: MatId| {
        let (x, y) = at(x, y);
        world.set_tile(Pos::new(x, y, z), t, m);
    };
    let rect_walls = |world: &mut World, x0: i32, y0: i32, x1: i32, y1: i32, z: i32, m: MatId| {
        for x in x0..=x1 {
            set(world, x, y0, z, Tile::Wall, m);
            set(world, x, y1, z, Tile::Wall, m);
        }
        for y in y0..=y1 {
            set(world, x0, y, z, Tile::Wall, m);
            set(world, x1, y, z, Tile::Wall, m);
        }
    };
    let fill = |world: &mut World, x0: i32, y0: i32, x1: i32, y1: i32, z: i32, t: Tile, m: MatId| {
        for y in y0..=y1 {
            for x in x0..=x1 {
                set(world, x, y, z, t, m);
            }
        }
    };
    let mut reserved: Vec<(i32, i32, i32, i32)> = Vec::new();
    let put = |things: &mut Things, kind: KindId, x: i32, y: i32, z: i32| {
        let (x, y) = at(x, y);
        things.spawn(kind, Pos::new(x, y, z))
    };

    // Level ground under every building.
    for &(x0, y0, x1, y1) in &[(58, 36, 82, 54), (93, 38, 104, 48), (102, 60, 110, 68), (97, 57, 103, 63)] {
        reserved.push((x0, y0, x1, y1));
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (wx, wy) = at(x, y);
                for z in 0..d {
                    let p = Pos::new(wx, wy, z);
                    if z < G {
                        world.set_tile(p, Tile::Wall, if z < G - 2 { ids.granite } else { ids.soil });
                    } else if z == G {
                        world.set_tile(p, Tile::Floor, ids.grass);
                    } else {
                        world.set_tile(p, Tile::Open, AIR);
                    }
                    world.set_fluid(p, ids.water, 0);
                }
            }
        }
    }

    // ---- The stone house: two storeys, slate roof, cellar ------------------
    fill(&mut world, 61, 39, 79, 51, G, Tile::Floor, ids.wood);
    rect_walls(&mut world, 60, 38, 80, 52, G, ids.block);
    rect_walls(&mut world, 60, 38, 80, 52, G + 1, ids.block);
    fill(&mut world, 61, 39, 79, 51, G + 1, Tile::Floor, ids.wood);
    fill(&mut world, 60, 38, 80, 52, G + 2, Tile::Floor, ids.slate);
    for y in 39..=51 {
        set(&mut world, 70, y, G, Tile::Wall, ids.block);
    }
    for x in 71..=79 {
        set(&mut world, x, 45, G + 1, Tile::Wall, ids.block);
    }
    for &(x, y, z) in &[(64, 38, G), (75, 38, G), (60, 44, G), (80, 44, G), (75, 52, G), (64, 38, G + 1), (75, 38, G + 1), (64, 52, G + 1), (75, 52, G + 1), (60, 44, G + 1), (80, 48, G + 1)] {
        set(&mut world, x, y, z, Tile::Wall, ids.glass);
    }
    // Doors: front, between the rooms, and upstairs.
    for &(x, y, z) in &[(65, 52, G), (70, 45, G), (75, 45, G + 1)] {
        set(&mut world, x, y, z, Tile::Floor, ids.wood);
        put(&mut things, ids.door, x, y, z);
    }
    // Stairs up to the first floor, and down to the cellar.
    set(&mut world, 78, 40, G, Tile::StairUp, ids.wood);
    set(&mut world, 78, 40, G + 1, Tile::StairDown, ids.wood);
    set(&mut world, 62, 40, G, Tile::StairDown, ids.wood);
    // Cellar, dug out of the soil one level down.
    fill(&mut world, 61, 39, 71, 47, G - 1, Tile::Floor, ids.soil);
    set(&mut world, 62, 40, G - 1, Tile::StairUp, ids.soil);
    // Stairs on down to the cave.
    set(&mut world, 70, 46, G - 1, Tile::StairDown, ids.soil);
    set(&mut world, 70, 46, G - 2, Tile::StairUpDown, ids.granite);
    set(&mut world, 70, 46, G - 3, Tile::StairUp, ids.granite);

    // Furniture.
    let hall = [(ids.table, 64, 44), (ids.table, 65, 44), (ids.chair, 63, 44), (ids.chair, 66, 44), (ids.chair, 64, 43), (ids.chair, 65, 45)];
    for &(kd, x, y) in &hall {
        put(&mut things, kd, x, y, G);
    }
    put(&mut things, ids.mug, 64, 44, G);
    put(&mut things, ids.bread, 65, 44, G);
    put(&mut things, ids.candle, 65, 44, G);
    put(&mut things, ids.torch, 61, 47, G);
    put(&mut things, ids.torch, 69, 41, G);
    put(&mut things, ids.bed, 78, 50, G);
    put(&mut things, ids.chest, 77, 50, G);
    put(&mut things, ids.coins, 77, 50, G);
    for x in 71..=73 {
        put(&mut things, ids.shelf, x, 39, G);
        put(&mut things, ids.book, x, 39, G);
    }
    put(&mut things, ids.torch, 74, 47, G);
    put(&mut things, ids.bed, 62, 50, G + 1);
    put(&mut things, ids.bed, 64, 50, G + 1);
    put(&mut things, ids.table, 68, 41, G + 1);
    put(&mut things, ids.mug, 68, 41, G + 1);
    put(&mut things, ids.candle, 68, 41, G + 1);
    put(&mut things, ids.bed, 78, 43, G + 1);
    for x in 61..=64 {
        put(&mut things, ids.barrel, x, 47, G - 1);
    }
    put(&mut things, ids.barrel, 71, 39, G - 1);
    put(&mut things, ids.chest, 66, 39, G - 1);

    // ---- The lava cave, two levels below the cellar -----------------------
    let cz = G - 3;
    let (ccx, ccy) = (86, 54);
    for y in ccy - 8..=ccy + 8 {
        for x in ccx - 14..=ccx + 14 {
            let (dx, dy) = ((x - ccx) as f32 / 13.0, (y - ccy) as f32 / 7.0);
            let wob = (tachyon::rng::hash3(x, y, 0, s) % 100) as f32 / 400.0;
            if dx * dx + dy * dy < 1.0 - wob {
                set(&mut world, x, y, cz, Tile::Floor, ids.granite);
            }
        }
    }
    for x in 70..=ccx {
        set(&mut world, x, 46, cz, Tile::Floor, ids.granite);
    }
    for y in 46..=ccy {
        set(&mut world, ccx, y, cz, Tile::Floor, ids.granite);
    }
    set(&mut world, 70, 46, cz, Tile::StairUp, ids.granite);
    let (lx, ly) = (90, 55);
    for y in ly - 4..=ly + 4 {
        for x in lx - 5..=lx + 5 {
            let (dx, dy) = ((x - lx) as f32 / 5.0, (y - ly) as f32 / 3.5);
            if dx * dx + dy * dy < 1.0 {
                set(&mut world, x, y, cz, Tile::Open, AIR);
                set(&mut world, x, y, cz - 1, Tile::Floor, ids.granite);
                let (wx, wy) = at(x, y);
                world.set_fluid(Pos::new(wx, wy, cz - 1), ids.lava, FULL);
            }
        }
    }

    // ---- The wooden hut, waiting for a spark ------------------------------
    fill(&mut world, 96, 41, 101, 45, G, Tile::Floor, ids.wood);
    rect_walls(&mut world, 95, 40, 102, 46, G, ids.wood);
    fill(&mut world, 95, 40, 102, 46, G + 1, Tile::Floor, ids.wood);
    set(&mut world, 98, 46, G, Tile::Floor, ids.wood);
    put(&mut things, ids.door, 98, 46, G);
    for &(x, y) in &[(96, 41), (97, 41), (96, 42), (101, 41)] {
        put(&mut things, ids.hay, x, y, G);
    }
    put(&mut things, ids.table, 100, 44, G);
    put(&mut things, ids.mug, 100, 44, G);

    // ---- The watchtower ---------------------------------------------------
    for z in G..G + 3 {
        rect_walls(&mut world, 104, 62, 108, 66, z, ids.block);
    }
    for z in G + 1..=G + 3 {
        fill(&mut world, 105, 63, 107, 65, z, Tile::Floor, ids.block);
    }
    fill(&mut world, 105, 63, 107, 65, G, Tile::Floor, ids.granite);
    for &(x, y) in &[(104, 62), (106, 62), (108, 62), (108, 64), (108, 66), (106, 66), (104, 66), (104, 64)] {
        set(&mut world, x, y, G + 3, Tile::Wall, ids.block);
    }
    set(&mut world, 105, 63, G, Tile::StairUp, ids.block);
    set(&mut world, 105, 63, G + 1, Tile::StairUpDown, ids.block);
    set(&mut world, 105, 63, G + 2, Tile::StairUpDown, ids.block);
    set(&mut world, 105, 63, G + 3, Tile::StairDown, ids.block);
    set(&mut world, 106, 66, G, Tile::Floor, ids.granite);
    put(&mut things, ids.door, 106, 66, G);
    set(&mut world, 108, 64, G + 1, Tile::Wall, ids.glass);
    set(&mut world, 104, 64, G + 2, Tile::Wall, ids.glass);
    put(&mut things, ids.torch, 107, 65, G);
    put(&mut things, ids.brazier, 107, 65, G + 3);

    // ---- The well ---------------------------------------------------------
    for &(x, y) in &[(99, 59), (101, 59), (99, 61), (101, 61)] {
        set(&mut world, x, y, G, Tile::Wall, ids.block);
    }
    for z in G - 2..=G {
        set(&mut world, 100, 60, z, Tile::Open, AIR);
    }
    set(&mut world, 100, 60, G - 3, Tile::Floor, ids.granite);
    let (wx, wy) = at(100, 60);
    world.set_fluid(Pos::new(wx, wy, G - 3), ids.water, FULL);
    world.set_fluid(Pos::new(wx, wy, G - 2), ids.water, 40);

    // ---- A pit ------------------------------------------------------------
    for y in 47..=52 {
        for x in 118..=121 {
            let (wx, wy) = at(x, y);
            let g = hm[(wy * w + wx) as usize];
            for z in 2..=g.max(2) {
                world.set_tile(Pos::new(wx, wy, z), Tile::Open, AIR);
                world.set_fluid(Pos::new(wx, wy, z), ids.water, 0);
            }
            world.set_tile(Pos::new(wx, wy, 1), Tile::Floor, ids.granite);
        }
    }
    reserved.push((116, 45, 123, 54));

    // ---- Outdoors: a brazier by the door, a path, trees -------------------
    put(&mut things, ids.brazier, 67, 54, G);
    for y in 53..=58 {
        set(&mut world, 65, y, G, Tile::Floor, ids.soil);
    }
    for x in 65..=98 {
        set(&mut world, x, 58, G, Tile::Floor, ids.soil);
    }
    for y in 47..=58 {
        set(&mut world, 98, y, G, Tile::Floor, ids.soil);
    }

    // ---- The canal ----------------------------------------------------------
    // Cut one level below the ground (a floor at G - 2, water up to the lake's
    // level), from inside the lake east to a sluice that drains it, so water
    // runs down it all the time. A plank bridge crosses it.
    let (cx0, cy0, cx1, cy1) = CANAL;
    reserved.push((cx0, cy0 - 1, cx1 + 1, cy1 + 1));
    for y in cy0..=cy1 {
        for x in cx0..=cx1 {
            let (wx, wy) = at(x, y);
            for z in G - 1..d {
                world.set_tile(Pos::new(wx, wy, z), Tile::Open, AIR);
            }
            world.set_tile(Pos::new(wx, wy, G - 2), Tile::Floor, ids.sand);
            world.set_fluid(Pos::new(wx, wy, G - 2), ids.water, FULL);
            world.set_fluid(Pos::new(wx, wy, G - 1), ids.water, FULL);
        }
    }
    for x in cx0 + 2..=cx1 {
        for &y in &[cy0 - 1, cy1 + 1] {
            let (wx, wy) = at(x, y);
            if hm[(wy * w + wx) as usize] >= G {
                world.set_tile(Pos::new(wx, wy, G - 1), Tile::Wall, ids.block);
            }
        }
    }
    // Springs at the mouth hold the lake at its level; the sluice drains the far end.
    let mut sources = Vec::new();
    for y in cy0..=cy1 {
        let (wx, wy) = at(cx0 - 2, y);
        sources.push(tachyon::sim::Source { pos: Pos::new(wx, wy, G - 1), mat: ids.water, rate: 4 });
    }
    for y in cy0..=cy1 {
        let (wx, wy) = at(cx1, y);
        sources.push(tachyon::sim::Source { pos: Pos::new(wx, wy, G - 1), mat: ids.water, rate: -6 });
    }
    for y in cy0 - 1..=cy1 + 1 {
        put(&mut things, ids.bridge, 54, y, G);
    }
    // A raft on the lake, west of the canal mouth.
    let (rx, ry) = at(29, 67);
    let raft = Vessel::build("raft", &mut things, Pos::new(rx, ry, G), &["===", "===", "==="], |c| (c == '=').then_some(ids.raft));

    let mut rng = Rng::new(seed ^ 0x7EE5);
    let mut trees: Vec<(i32, i32)> = Vec::new();
    for _ in 0..(w * h / 18) {
        let (x, y) = (rng.range(2, w - 2), rng.range(2, h - 2));
        let g = hm[(y * w + x) as usize];
        if g < G || g + 3 >= d {
            continue;
        }
        let forest = fbm2(s ^ 0xF0, x as f32 / 20.0, y as f32 / 20.0, 3);
        if forest < 0.52 {
            continue;
        }
        if reserved.iter().any(|&(x0, y0, x1, y1)| {
            let (a, b) = at(x0, y0);
            let (c, e) = at(x1, y1);
            x >= a - 1 && x <= c + 1 && y >= b - 1 && y <= e + 1
        }) {
            continue;
        }
        if trees.iter().any(|&(tx, ty)| (tx - x).abs() <= 2 && (ty - y).abs() <= 2) {
            continue;
        }
        let i = world.idx(x, y, g);
        if world.tile[i] != Tile::Floor || world.mat[i] != ids.grass {
            continue;
        }
        trees.push((x, y));
        world.set_tile(Pos::new(x, y, g), Tile::Wall, ids.wood);
        for &(dx, dy) in &[(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)] {
            let p = Pos::new(x + dx, y + dy, g + 1);
            if world.tile(p) == Tile::Open {
                world.set_tile(p, Tile::Floor, ids.leaves);
            }
        }
        world.set_tile(Pos::new(x, y, g + 2), Tile::Floor, ids.leaves);
    }

    // ---- People and animals -----------------------------------------------
    // Each creature is a thing (drawn and moved like any other) plus a body,
    // needs and a mind in `Life`. Positions are in map coordinates.
    let mut life = Life::new(seed);
    life.drinkable.push(ids.water);
    life.grazing.push(ids.grass);
    let add = |life: &mut Life, things: &mut Things, mut s: Species, c: Rgb| {
        let meal = (s.mass / 40.0).clamp(0.1, 1.0);
        s.corpse = Some(things.add_kind(ThingKind::new(&format!("{} corpse", s.name), '%', c, Layer::Item).food(meal, true)));
        life.add_species(s)
    };
    let human = add(&mut life, &mut things, species::human(), Rgb(170, 70, 70));
    let dog = add(&mut life, &mut things, species::dog(), Rgb(150, 100, 60));
    let cat = add(&mut life, &mut things, species::cat(), Rgb(180, 170, 160));
    let rat = add(&mut life, &mut things, species::rat(), Rgb(130, 110, 100));
    let sheep = add(&mut life, &mut things, species::sheep(), Rgb(220, 210, 190));
    let rat_kind = things.add_kind(ThingKind::new("rat", 'r', Rgb(150, 130, 112), Layer::Creature));
    let sheep_kind = things.add_kind(ThingKind::new("sheep", 's', Rgb(236, 236, 226), Layer::Creature).blocking());
    let apple = things.add_kind(ThingKind::new("apple", '•', Rgb(200, 60, 50), Layer::Item).food(0.15, false));
    let pos = |x: i32, y: i32, z: i32| {
        let (x, y) = at(x, y);
        Pos::new(x, y, z)
    };
    // The ground at a map spot (for animals out in the fields).
    let ground = |world: &World, x: i32, y: i32| {
        let p = pos(x, y, 0);
        let z = (1..d).rev().find(|&z| world.standable(Pos::new(p.x, p.y, z))).unwrap_or(G);
        Pos::new(p.x, p.y, z)
    };

    // Food about the place.
    put(&mut things, ids.bread, 66, 39, G - 1);
    put(&mut things, ids.bread, 66, 39, G - 1);
    put(&mut things, apple, 100, 44, G);
    put(&mut things, apple, 100, 44, G);
    put(&mut things, apple, 64, 44, G);
    let larder = things.add_kind(ThingKind::new("larder", 'Æ', Rgb(170, 120, 70), Layer::Furniture).food(0.5, false).stocked());
    put(&mut things, larder, 66, 45, G);

    let player = put(&mut things, ids.you, 65, 56, G);
    life.add(player, human, Control::Player, 0);

    // Villagers: asleep 22:00 to 6:30, working mornings and afternoons,
    // about the village at lunch and in the evening. Ilse keeps watch from
    // the tower in the evening.
    let village = pos(80, 52, G);
    let people = [
        ("Agnes", 66, 47, G, Rgb(220, 90, 80), pos(62, 50, G + 1), Task::Work { at: pos(64, 45, G), radius: 3 }),
        ("Tomas", 75, 48, G, Rgb(90, 140, 230), pos(78, 50, G), Task::Work { at: pos(98, 43, G), radius: 2 }),
        ("Marek", 88, 52, G, Rgb(110, 200, 110), pos(64, 50, G + 1), Task::Work { at: pos(90, 62, G), radius: 6 }),
        ("Ilse", 76, 42, G + 1, Rgb(220, 180, 220), pos(78, 43, G + 1), Task::Work { at: pos(72, 41, G), radius: 2 }),
    ];
    for (name, x, y, z, col, bed, work) in people {
        let id = put(&mut things, ids.person, x, y, z);
        if let Some(t) = things.get_mut(id) {
            t.name = Some(name.to_string());
            t.color = Some(col);
        }
        let evening = if name == "Ilse" { Task::Stay { at: pos(106, 64, G + 3) } } else { Task::Wander { around: village, radius: 14 } };
        let c = life.add(id, human, Control::Ai, 1);
        c.mind.home = Some(bed);
        c.mind.routine = Routine::new()
            .with(22.0, 6.5, Task::Sleep { at: Some(bed) })
            .with(8.0, 12.0, work.clone())
            .with(12.0, 13.5, Task::Wander { around: village, radius: 10 })
            .with(13.5, 18.0, work)
            .with(18.0, 22.0, evening);
    }
    // Animals keep near home and look after themselves.
    let mut animals = vec![(ids.dog, dog, pos(72, 57, G), "Bran", 1), (ids.cat, cat, pos(63, 49, G), "Mog", 1)];
    for (x, y) in [(64, 42), (67, 44), (69, 40)] {
        animals.push((rat_kind, rat, pos(x, y, G - 1), "", 3));
    }
    for (x, y) in [(86, 66), (89, 67), (92, 65), (88, 69)] {
        animals.push((sheep_kind, sheep, ground(&world, x, y), "", 4));
    }
    for (kind, sp, p, name, faction) in animals {
        let id = things.spawn(kind, p);
        if !name.is_empty() {
            if let Some(t) = things.get_mut(id) {
                t.name = Some(name.to_string());
            }
        }
        life.add(id, sp, Control::Ai, faction).mind.home = Some(p);
    }

    tachyon::sim::light::compute_sky(&mut world, &mut VecDeque::new());
    world.wake.wake_all();
    // Underground starts at deep-earth temperature.
    for i in 0..world.len() {
        if world.sky[i] == 0 {
            world.temp[i] = world.ground_temp;
        }
    }
    let mut scene = Scene::new(world, things, Clock::new(2.0, 15.0), seed);
    scene.sim.sources = sources;
    scene.vessels.push(raft);
    scene.flash = Some(ids.flash);
    scene.life = life;
    Built { scene, ids, player }
}
