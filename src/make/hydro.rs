//! Rivers by flow routing on the atlas (ROADMAP 3.6 and 4.9, step R-a).
//!
//! Water runs downhill. Given only the atlas heights this module works out
//! where, in four passes that each run once over the map:
//!
//! 1. **Priority-flood depression filling** (Barnes et al. 2014). Starting from
//!    the sea and the map edge, always take the lowest unvisited cell next and
//!    raise any neighbour that is lower than it to the same level (plus a tiny
//!    slope, `EPS`, so flat ground still drains). A pit is raised to the height
//!    of its lowest rim: that flat is a **lake**, and it is *recorded*, not
//!    erased.
//! 2. **D8 flow directions**: each cell drains to the lowest of its 8 neighbours
//!    on the filled surface (steepest descent; ties go to the lowest direction
//!    number, so the result is deterministic). Cells of a drawn river are
//!    **pinned**: they drain along the drawn line whatever the land says.
//! 3. **Drainage area** by walking the flow graph from the top down (Kahn's
//!    topological order): every cell passes its catchment to the cell it drains
//!    to. Discharge is area times the `rainfall` setting.
//! 4. **River extraction**: cells with discharge above a threshold form the
//!    channels, cut into `River` polylines whose width grows as `k * sqrt(Q)`.
//!
//! Nothing here writes into the atlas: `route` and `extract_rivers` only read
//! it, and `with_rivers` returns a copy. Drawn rivers are never altered.
//!
//! Rust notes for a Python reader: `Vec<u8>` is a growable list of bytes; an
//! `enum` is a tagged union (like a Python Enum that may carry data); the
//! `impl Ord for Item` below teaches the standard `BinaryHeap` (a max-heap) to
//! act as the min-heap the flood needs.

use super::atlas::{Atlas, River};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, VecDeque};

/// Direction numbers: 0 = north, going clockwise (1 = north-east, 2 = east...).
const DX: [i32; 8] = [0, 1, 1, 1, 0, -1, -1, -1];
const DY: [i32; 8] = [-1, -1, 0, 1, 1, 1, 0, -1];
/// "Drains nowhere": the cell is an outlet (sea or map edge).
pub const NO_DIR: u8 = 255;
/// Slope added per step across flat ground so it drains (metres).
const EPS: f64 = 1e-4;
/// A cell is lake only when filling raised it by more than this (metres).
const LAKE_MIN_DEPTH: f64 = 0.5;

/// The knobs of the model.
#[derive(Clone, Debug)]
pub struct HydroSettings {
    /// Runoff each atlas cell contributes, in m^3/s. The one scale of the
    /// whole model: discharge = catchment cells x `rainfall`.
    pub rainfall: f32,
    /// A cell with at least this discharge (m^3/s) is a river.
    pub threshold: f32,
    /// River width in metres = `width_k` x sqrt(discharge).
    pub width_k: f32,
}

impl Default for HydroSettings {
    fn default() -> Self {
        HydroSettings { rainfall: 0.05, threshold: 1.0, width_k: 2.0 }
    }
}

/// A lake found while filling: a flat basin with one way out.
#[derive(Clone, Debug, PartialEq)]
pub struct Lake {
    /// Surface height in metres above sea level.
    pub level: f32,
    /// Atlas cells under water, in map order.
    pub cells: Vec<(i32, i32)>,
    /// The lake cell the water leaves from (`None` if it never leaves).
    pub outlet: Option<(i32, i32)>,
}

/// Where an extracted river finishes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum End {
    /// Runs into the sea.
    Sea,
    /// Runs into lake number `n` (an index into `Hydro::lakes`).
    Lake(usize),
    /// Leaves the map.
    Edge,
    /// Joins a bigger extracted river (its last point lies on that river).
    River,
    /// Joins a river drawn in the atlas.
    Drawn,
}

/// A river found by the model (as opposed to drawn).
#[derive(Clone, Debug, PartialEq)]
pub struct Extracted {
    /// Source first, mouth last, like a drawn river; width from the peak discharge.
    pub river: River,
    pub end: End,
    /// Highest discharge along it, m^3/s.
    pub discharge: f32,
    /// Atlas cells draining through its busiest cell.
    pub catchment: u32,
}

/// The routed map.
#[derive(Clone, Debug)]
pub struct Hydro {
    pub w: i32,
    pub h: i32,
    /// Height after filling pits.
    pub filled: Vec<f32>,
    /// Direction each cell drains to (0..8) or `NO_DIR`.
    pub flow: Vec<u8>,
    /// Cells draining through each cell, itself included.
    pub area: Vec<u32>,
    /// `area` x rainfall, m^3/s.
    pub discharge: Vec<f32>,
    /// Lake number of each cell, or -1.
    pub lake_of: Vec<i32>,
    pub lakes: Vec<Lake>,
    /// Cells on a drawn river.
    pub pinned: Vec<bool>,
    pub sea: Vec<bool>,
}

impl Hydro {
    pub fn idx(&self, x: i32, y: i32) -> usize {
        (y * self.w + x) as usize
    }
    /// The cell this one drains into, if any.
    pub fn downstream(&self, x: i32, y: i32) -> Option<(i32, i32)> {
        let d = self.flow[self.idx(x, y)];
        if d == NO_DIR {
            None
        } else {
            Some((x + DX[d as usize], y + DY[d as usize]))
        }
    }
}

/// Heap entry. `BinaryHeap` pops the *greatest* item, so `cmp` is reversed
/// (lowest level first, then first pushed) to get a min-heap.
struct Item {
    level: f64,
    seq: u64,
    idx: usize,
}
impl PartialEq for Item {
    fn eq(&self, o: &Self) -> bool {
        self.cmp(o) == Ordering::Equal
    }
}
impl Eq for Item {}
impl PartialOrd for Item {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Item {
    fn cmp(&self, o: &Self) -> Ordering {
        o.level.total_cmp(&self.level).then(o.seq.cmp(&self.seq))
    }
}

/// Fill, route and accumulate. Deterministic: no randomness, ties by index.
pub fn route(atlas: &Atlas, set: &HydroSettings) -> Hydro {
    let (w, h) = (atlas.w, atlas.h);
    let n = (w * h) as usize;
    let at = |x: i32, y: i32| (y * w + x) as usize;
    let elev: Vec<f64> = atlas.elevation.iter().map(|&e| e as f64).collect();
    let sea: Vec<bool> = atlas.elevation.iter().map(|&e| e < 0).collect();
    let on_edge = |x: i32, y: i32| x == 0 || y == 0 || x == w - 1 || y == h - 1;
    let outlet: Vec<bool> = (0..n).map(|i| sea[i] || on_edge(i as i32 % w, i as i32 / w)).collect();

    // 1. Priority flood from the outlets.
    let mut filled = elev.clone();
    let mut queued = vec![false; n];
    let mut heap = BinaryHeap::new();
    let mut seq = 0u64;
    for i in 0..n {
        if outlet[i] {
            queued[i] = true;
            heap.push(Item { level: elev[i], seq, idx: i });
            seq += 1;
        }
    }
    while let Some(Item { level, idx, .. }) = heap.pop() {
        let (x, y) = (idx as i32 % w, idx as i32 / w);
        for d in 0..8 {
            let (nx, ny) = (x + DX[d], y + DY[d]);
            if nx < 0 || ny < 0 || nx >= w || ny >= h || queued[at(nx, ny)] {
                continue;
            }
            let j = at(nx, ny);
            queued[j] = true;
            filled[j] = elev[j].max(level + EPS);
            heap.push(Item { level: filled[j], seq, idx: j });
            seq += 1;
        }
    }

    // 2. D8: steepest descent on the filled surface.
    let mut flow = vec![NO_DIR; n];
    for i in 0..n {
        if outlet[i] {
            continue;
        }
        let (x, y) = (i as i32 % w, i as i32 / w);
        let mut best = (0.0f64, NO_DIR);
        for d in 0..8 {
            let (nx, ny) = (x + DX[d], y + DY[d]);
            if nx < 0 || ny < 0 || nx >= w || ny >= h {
                continue;
            }
            let dist = if d % 2 == 1 { std::f64::consts::SQRT_2 } else { 1.0 };
            let slope = (filled[i] - filled[at(nx, ny)]) / dist;
            if slope > best.0 {
                best = (slope, d as u8);
            }
        }
        flow[i] = best.1;
    }
    // Pinned channels: a drawn river drains along its own line.
    let mut pinned = vec![false; n];
    for r in &atlas.rivers {
        let cells: Vec<(i32, i32)> = Atlas::river_cells(r).into_iter().filter(|&(x, y)| atlas.contains(x, y)).collect();
        for &(x, y) in &cells {
            pinned[at(x, y)] = true;
        }
        for pair in cells.windows(2) {
            let ((x, y), (nx, ny)) = (pair[0], pair[1]);
            let i = at(x, y);
            if outlet[i] {
                continue;
            }
            if let Some(d) = (0..8).find(|&d| x + DX[d] == nx && y + DY[d] == ny) {
                flow[i] = d as u8;
            }
        }
    }

    // 3. Drainage area, top of the catchment first (Kahn's algorithm). A
    // cell in a loop (possible only where a drawn river fights the land)
    // is never reached and just keeps its own area.
    let target = |i: usize| -> Option<usize> {
        let d = flow[i];
        (d != NO_DIR).then(|| at(i as i32 % w + DX[d as usize], i as i32 / w + DY[d as usize]))
    };
    let mut indeg = vec![0u32; n];
    for i in 0..n {
        if let Some(t) = target(i) {
            indeg[t] += 1;
        }
    }
    let mut area = vec![1u32; n];
    let mut ready: VecDeque<usize> = (0..n).filter(|&i| indeg[i] == 0).collect();
    while let Some(i) = ready.pop_front() {
        if let Some(t) = target(i) {
            area[t] += area[i];
            indeg[t] -= 1;
            if indeg[t] == 0 {
                ready.push_back(t);
            }
        }
    }
    let discharge: Vec<f32> = area.iter().map(|&a| a as f32 * set.rainfall).collect();

    // Lakes: connected (4-way) patches the filling raised.
    let mut lake_of = vec![-1i32; n];
    let mut lakes = Vec::new();
    let is_lake = |i: usize| !outlet[i] && filled[i] - elev[i] > LAKE_MIN_DEPTH;
    for s in 0..n {
        if lake_of[s] >= 0 || !is_lake(s) {
            continue;
        }
        let id = lakes.len() as i32;
        let mut stack = vec![s];
        lake_of[s] = id;
        let mut cells = Vec::new();
        while let Some(i) = stack.pop() {
            let (x, y) = (i as i32 % w, i as i32 / w);
            cells.push(i);
            for d in [0usize, 2, 4, 6] {
                let (nx, ny) = (x + DX[d], y + DY[d]);
                if nx < 0 || ny < 0 || nx >= w || ny >= h {
                    continue;
                }
                let j = at(nx, ny);
                if lake_of[j] < 0 && is_lake(j) {
                    lake_of[j] = id;
                    stack.push(j);
                }
            }
        }
        cells.sort_unstable();
        let level = cells.iter().map(|&i| filled[i]).fold(f64::MIN, f64::max) as f32;
        let outlet_cell = cells.iter().copied().find(|&i| match target(i) {
            None => true,
            Some(t) => lake_of[t] != id,
        });
        lakes.push(Lake {
            level,
            cells: cells.iter().map(|&i| (i as i32 % w, i as i32 / w)).collect(),
            outlet: outlet_cell.map(|i| (i as i32 % w, i as i32 / w)),
        });
    }

    Hydro { w, h, filled: filled.iter().map(|&v| v as f32).collect(), flow, area, discharge, lake_of, lakes, pinned, sea }
}

/// Cut the channels (discharge >= threshold) into rivers. Drawn rivers are not
/// included and not touched; a river that reaches one stops on it. Each channel
/// cell belongs to exactly one river: at a confluence the stem with the bigger
/// catchment carries on, the other ends on the shared cell.
pub fn extract_rivers(h: &Hydro, set: &HydroSettings) -> Vec<Extracted> {
    let n = (h.w * h.h) as usize;
    let at = |x: i32, y: i32| (y * h.w + x) as usize;
    let xy = |i: usize| (i as i32 % h.w, i as i32 / h.w);
    let channel = |i: usize| h.discharge[i] >= set.threshold && !h.sea[i] && h.lake_of[i] < 0 && !h.pinned[i];
    let next = |i: usize| -> Option<usize> {
        let (x, y) = xy(i);
        h.downstream(x, y).map(|(nx, ny)| at(nx, ny))
    };
    // Each channel cell's main stem: its channel inflow with the biggest catchment.
    let mut main_in: Vec<Option<usize>> = vec![None; n];
    for i in 0..n {
        if !channel(i) {
            continue;
        }
        if let Some(t) = next(i) {
            if channel(t) && main_in[t].map_or(true, |m| h.area[i] > h.area[m]) {
                main_in[t] = Some(i);
            }
        }
    }
    let mut out = Vec::new();
    for head in 0..n {
        // A river starts where nothing channel-sized flows in.
        if !channel(head) || main_in[head].is_some() {
            continue;
        }
        let mut pts = vec![xy(head)];
        let (mut cur, mut peak) = (head, head);
        let end = loop {
            if h.discharge[cur] > h.discharge[peak] {
                peak = cur;
            }
            let Some(nxt) = next(cur) else { break End::Edge };
            let p = xy(nxt);
            if h.sea[nxt] {
                pts.push(p);
                break End::Sea;
            }
            if h.lake_of[nxt] >= 0 {
                pts.push(p);
                break End::Lake(h.lake_of[nxt] as usize);
            }
            if h.pinned[nxt] {
                pts.push(p);
                break End::Drawn;
            }
            if !channel(nxt) {
                break End::Edge;
            }
            pts.push(p);
            if main_in[nxt] != Some(cur) {
                break End::River; // a tributary: ends on the stem it joins
            }
            cur = nxt;
        };
        let width = (set.width_k * h.discharge[peak].sqrt()).round().clamp(1.0, 255.0) as u8;
        out.push(Extracted { river: River { width, points: pts }, end, discharge: h.discharge[peak], catchment: h.area[peak] });
    }
    out
}

/// A copy of the atlas with the extracted rivers added after the drawn ones.
pub fn with_rivers(atlas: &Atlas, found: &[Extracted]) -> Atlas {
    let mut a = atlas.clone();
    a.rivers.extend(found.iter().map(|e| e.river.clone()));
    a
}
