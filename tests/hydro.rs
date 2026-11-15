//! Flow routing on the atlas (tach-020, ROADMAP step R-a).

use tachyon::atlas::{Atlas, River};
use tachyon::hydro::{extract_rivers, route, with_rivers, End, HydroSettings};

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

#[test]
fn routing_is_deterministic() {
    let a = cone();
    let (h1, h2) = (route(&a, &settings()), route(&a, &settings()));
    assert_eq!(h1.flow, h2.flow);
    assert_eq!(extract_rivers(&h1, &settings()), extract_rivers(&h2, &settings()));
}
