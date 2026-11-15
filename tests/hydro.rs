//! Flow routing on the atlas (tach-020, ROADMAP step R-a) and stream-power
//! erosion (tach-021, step R-b).

use tachyon::atlas::{Atlas, Relief, River};
use tachyon::hydro::{drainage, erode, extract_rivers, route, upsample, with_rivers, End, ErodeSettings, HydroSettings, RELIEF_SCALE};
use tachyon::noise::fbm2;

/// A square island: a cone 40 m high in the middle of a 41 x 41 map, sea round it.
/// A small tilt toward the south-west makes the catchments unequal.
fn cone() -> Atlas {
    let n = 41;
    let mut a = Atlas::new("cone", n, n, &["granite"], &["grassland"], 5);
    for y in 0..n {
        for x in 0..n {
            let (dx, dy) = ((x - 20) as f32, (y - 20) as f32);
            let r = (dx * dx + dy * dy).sqrt();
            let h = 40.0 - 2.4 * r + 0.05 * (x - y) as f32 + 0.013 * ((x * 7 + y * 13) % 5) as f32;
            let i = a.idx(x, y);
            a.elevation[i] = if h > 0.0 { h.round() as i16 } else { -5 };
        }
    }
    a
}

fn settings() -> HydroSettings {
    HydroSettings { rainfall: 0.2, threshold: 1.0, width_k: 1.0 }
}

#[test]
fn every_river_reaches_the_sea_or_a_lake_downhill() {
    let a = cone();
    let h = route(&a, &settings());
    let found = extract_rivers(&h, &settings());
    assert!(found.len() >= 3, "expected several rivers, got {}", found.len());
    for e in &found {
        // Never uphill, on the filled surface and on the raw heights (no lakes here).
        let heights: Vec<f32> = e.river.points.iter().map(|&(x, y)| a.elevation[a.idx(x, y)] as f32).collect();
        assert!(heights.windows(2).all(|p| p[1] <= p[0]), "river runs uphill: {:?}", heights);
        // Follow joins until the end is the sea or a lake.
        let mut cur = e;
        for _ in 0..found.len() + 1 {
            if cur.end != End::River {
                break;
            }
            let last = *cur.river.points.last().unwrap();
            cur = found.iter().find(|o| !std::ptr::eq(*o, cur) && o.river.points.contains(&last)).expect("a joined river exists");
        }
        assert!(matches!(cur.end, End::Sea | End::Lake(_)), "river ends at {:?}", cur.end);
    }
    // And none of them stops at the map edge on a sea-ringed island.
    assert!(found.iter().all(|e| e.end != End::Edge));
}

#[test]
fn biggest_catchment_is_widest() {
    let a = cone();
    let h = route(&a, &settings());
    let found = extract_rivers(&h, &settings());
    let big = found.iter().max_by_key(|e| e.catchment).unwrap();
    assert!(found.iter().all(|e| e.river.width <= big.river.width));
    assert!(found.iter().any(|e| e.river.width < big.river.width), "all rivers equally wide, test proves nothing");
    assert!(big.discharge > found.iter().map(|e| e.discharge).fold(f32::MAX, f32::min));
}

#[test]
fn pits_become_recorded_lakes_and_rivers_end_in_them() {
    let mut a = cone();
    // Dig a pit on the north-east flank, 12 m above the sea.
    for (x, y) in [(26, 14), (27, 14), (26, 15), (27, 15)] {
        let i = a.idx(x, y);
        a.elevation[i] -= 8;
    }
    let h = route(&a, &settings());
    assert_eq!(h.lakes.len(), 1, "the pit is one lake");
    let lake = &h.lakes[0];
    assert!(lake.cells.contains(&(26, 14)) && lake.level > a.elev(26, 14));
    let (ox, oy) = lake.outlet.expect("a lake that fills has an outlet");
    assert!(h.downstream(ox, oy).map_or(false, |(x, y)| h.lake_of[h.idx(x, y)] != 0));
    let found = extract_rivers(&h, &settings());
    assert!(found.iter().any(|e| e.end == End::Lake(0)), "a river runs into the lake");
    // The terrain itself is not changed: the pit is still a pit in the atlas.
    assert!(h.filled[h.idx(26, 14)] > a.elev(26, 14));
}

#[test]
fn drawn_river_is_pinned_and_unchanged() {
    let mut a = cone();
    let drawn = River { width: 7, points: vec![(20, 14), (20, 8), (20, 3)] };
    a.rivers.push(drawn.clone());
    let before = a.clone();
    let h = route(&a, &settings());
    let found = extract_rivers(&h, &settings());
    assert_eq!(a, before, "routing does not touch the atlas");
    // The pinned line drains along itself, northwards.
    for y in 4..14 {
        assert!(h.pinned[h.idx(20, y)]);
        assert_eq!(h.downstream(20, y), Some((20, y - 1)));
    }
    // No extracted river runs through a drawn cell except to end on it.
    let drawn_cells = Atlas::river_cells(&drawn);
    for e in &found {
        let n = e.river.points.len();
        assert!(e.river.points[..n - 1].iter().all(|p| !drawn_cells.contains(p)));
    }
    let merged = with_rivers(&a, &found);
    assert_eq!(merged.rivers[0], drawn);
    assert_eq!(merged.rivers.len(), 1 + found.len());
}

// ---- erosion (tach-021, step R-b) -------------------------------------------

/// An oval island 40 x 28 atlas cells, 50 m high, mirror-symmetric east to
/// west, with rolling noise; `rock(x)` picks each cell's rock (0 granite,
/// 1 chalk). Sea 10 m deep round it.
fn isle(rock: impl Fn(i32) -> u8) -> Atlas {
    let (w, h) = (40, 28);
    let mut a = Atlas::new("isle", w, h, &["granite", "chalk"], &["grassland"], 10);
    for y in 0..h {
        for x in 0..w {
            let (dx, dy) = ((x as f32 - 19.5) / 20.0, (y as f32 - 13.5) / 14.0);
            let r2 = dx * dx + dy * dy;
            let roll = fbm2(9, dx.abs() * 6.0, y as f32 / 5.0, 3) - 0.5;
            let e = 50.0 * (1.0 - 1.3 * r2) + 12.0 * roll;
            let i = a.idx(x, y);
            a.elevation[i] = if e > 0.0 { e.round() as i16 } else { -10 };
            a.rock[i] = rock(x);
        }
    }
    a
}

fn quick() -> ErodeSettings {
    ErodeSettings { steps: 80, seed: 5, ..ErodeSettings::default() }
}

fn land(r: &Relief) -> impl Iterator<Item = usize> + '_ {
    (0..r.height.len()).filter(move |&i| r.height[i] > 0.0)
}

#[test]
fn erosion_is_deterministic_for_a_seed_and_leaves_the_input_alone() {
    let a = isle(|_| 0);
    let copy = a.clone();
    let (e1, e2) = (erode(&a, &quick()), erode(&a, &quick()));
    assert_eq!(a, copy, "the input atlas is not changed");
    assert_eq!(e1.after, e2.after);
    assert_eq!(e1.budget, e2.budget);
    assert_eq!(e1.atlas, e2.atlas);
    let other = erode(&a, &ErodeSettings { seed: 6, ..quick() });
    assert_ne!(e1.after, other.after, "the seed matters");
}

#[test]
fn soft_rock_loses_more_height_than_hard() {
    // West half chalk, east half granite, on an island the same both sides.
    let a = isle(|x| if x < 20 { 1 } else { 0 });
    let out = erode(&a, &quick());
    let (b, r) = (&out.before, &out.after);
    let mean_loss = |west: bool| {
        let cells: Vec<usize> = land(b).filter(|&i| ((i as i32 % b.w) < b.w / 2) == west).collect();
        cells.iter().map(|&i| (b.height[i] - r.height[i]) as f64).sum::<f64>() / cells.len() as f64
    };
    let (chalk, granite) = (mean_loss(true), mean_loss(false));
    assert!(granite > 0.0, "granite erodes too, just slower");
    assert!(chalk > 2.0 * granite, "chalk lost {chalk:.2} m on average, granite {granite:.2} m");
}

#[test]
fn valley_cells_sit_lower_than_their_banks() {
    let a = isle(|_| 1);
    let out = erode(&a, &quick());
    let r = &out.after;
    let d = drainage(r);
    let cell2 = (r.cell_m() * r.cell_m()) as f64;
    // Valley floors: cells draining at least 100 cells (0.64 ha), inland.
    let k = 3; // banks 3 cells (24 m) to each side across the flow
    let (mut valleys, mut lower, mut deepen) = (0, 0, 0.0f64);
    for i in land(r) {
        let (x, y) = (i as i32 % r.w, i as i32 / r.w);
        if d.area[i] < 100.0 * cell2 || r.height[i] < 2.0 || x < 8 || y < 8 || x >= r.w - 8 || y >= r.h - 8 {
            continue;
        }
        let j = d.receiver[i];
        let (fx, fy) = (j as i32 % r.w - x, j as i32 / r.w - y);
        let (px, py) = (-fy, fx); // across the flow
        let bank = |s: &Relief, sign: i32| s.at(x + sign * px * k, y + sign * py * k);
        valleys += 1;
        if bank(r, 1) > r.height[i] && bank(r, -1) > r.height[i] {
            lower += 1;
        }
        let depth = |s: &Relief| (bank(s, 1) + bank(s, -1)) as f64 / 2.0 - s.height[i] as f64;
        deepen += depth(r) - depth(&out.before);
    }
    assert!(valleys > 30, "only {valleys} valley cells");
    assert!(lower as f64 >= 0.9 * valleys as f64, "{lower} of {valleys} valley cells below both banks");
    assert!(deepen / valleys as f64 > 1.0, "valleys deepened by {:.2} m on average", deepen / valleys as f64);
}

#[test]
fn rock_removed_equals_rock_deposited_plus_what_reached_the_sea() {
    let mut a = isle(|x| if x < 20 { 1 } else { 0 });
    // A trench 10 m deep across the north flank: the streams off the summit
    // run into it, so part of their sediment settles in a lake.
    let (tx, ty) = (14..26, 6..8);
    for y in ty.clone() {
        for x in tx.clone() {
            let i = a.idx(x, y);
            a.elevation[i] -= 10;
        }
    }
    let out = erode(&a, &quick());
    let (b, r) = (&out.before, &out.after);
    let cell2 = (r.cell_m() * r.cell_m()) as f64;
    let (mut removed, mut laid) = (0.0f64, 0.0f64);
    for i in 0..b.height.len() {
        let dh = (r.height[i] - b.height[i]) as f64;
        if dh < 0.0 {
            removed -= dh * cell2;
        } else {
            laid += dh * cell2;
        }
    }
    let sea = out.budget.to_sea;
    assert!(removed > 1e5, "too little erosion to test: {removed:.0} m3");
    assert!(laid > 0.005 * removed, "too little deposited to test: {laid:.0} of {removed:.0} m3");
    assert!(sea > 0.0);
    let gap = (removed - (laid + sea)).abs();
    assert!(gap <= 0.01 * removed, "removed {removed:.0} m3, deposited {laid:.0} + to sea {sea:.0}");
    // Tighter than the deposit itself, so leaving deposition out would fail.
    assert!(gap <= 0.1 * laid, "gap {gap:.0} m3 against {laid:.0} m3 deposited");
    // The trench floor (a lake) silted up in places, by a metre or more.
    let s = RELIEF_SCALE;
    let silted = (ty.start * s..ty.end * s)
        .flat_map(|y| (tx.start * s..tx.end * s).map(move |x| (x, y)))
        .filter(|&(x, y)| r.at(x, y) > b.at(x, y) + 1.0)
        .count();
    assert!(silted >= 10, "only {silted} trench cells silted up");
    // The solver's own tallies balance too.
    let t = out.budget;
    assert!((t.eroded - t.deposited - t.to_sea).abs() <= 1e-6 * t.eroded);
}

#[test]
fn eroded_atlas_carries_its_relief_and_worldgen_reads_it() {
    let a = isle(|x| if x < 20 { 1 } else { 0 });
    let out = erode(&a, &quick());
    let e = &out.atlas;
    let rel = e.relief.as_ref().expect("relief set");
    assert_eq!((rel.scale, rel.w, rel.h), (RELIEF_SCALE, a.w * RELIEF_SCALE, a.h * RELIEF_SCALE));
    // elevation_at (what worldgen samples) now answers from the relief.
    let (x, y) = (50, 40);
    let at_centre = e.elevation_at((x as f32 + 0.5) / RELIEF_SCALE as f32, (y as f32 + 0.5) / RELIEF_SCALE as f32);
    assert!((at_centre - rel.at(x, y)).abs() < 1e-3);
    // Saved and read back unchanged; the sea stays sea.
    let back = Atlas::from_text(&e.to_text()).expect("parses");
    assert_eq!(&back, e);
    for i in 0..a.elevation.len() {
        assert_eq!(a.elevation[i] < 0, e.elevation[i] < 0, "coastline moved at cell {i}");
    }
    // Eroding an eroded atlas carries on from its relief.
    assert_eq!(upsample(e, 99, 1.0), *rel);
}

#[test]
fn routing_is_deterministic() {
    let a = cone();
    let (h1, h2) = (route(&a, &settings()), route(&a, &settings()));
    assert_eq!(h1.flow, h2.flow);
    assert_eq!(extract_rivers(&h1, &settings()), extract_rivers(&h2, &settings()));
}
