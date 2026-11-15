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
//! **Erosion (step R-b)**, further down: `erode` upsamples the atlas to an 8 m
//! [`Relief`] and runs the stream-power law dh/dt = U - K A^m S^n on it with the
//! implicit O(n) solver of Braun and Willett (2013), K from each cell's rock,
//! plus a thermal slope limit. It returns a new atlas carrying the relief,
//! which `Atlas::elevation_at` (and so worldgen) reads; the input is untouched.
//!
//! Rust notes for a Python reader: `Vec<u8>` is a growable list of bytes; an
//! `enum` is a tagged union (like a Python Enum that may carry data); the
//! `impl Ord for Item` below teaches the standard `BinaryHeap` (a max-heap) to
//! act as the min-heap the flood needs.

use super::atlas::{Atlas, Relief, River};
use crate::noise::fbm2;
use crate::rng::hash3;
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

// ---- R-b: stream-power erosion on a finer relief --------------------------
//
// Each step, on the 8 m relief:
//  1. uplift every land cell by U dt;
//  2. route: priority-flood fill (so pits become lakes with a way out), then
//     each cell's receiver is its steepest-descent neighbour on the filled
//     surface, the Braun-Willett "stack" lists every cell after its receiver,
//     and drainage area A is summed from the top of the stack down;
//  3. erode, walking the stack outward from the sea: the new height of a cell
//     depends only on its receiver's *new* height, so the implicit equation
//        h' - h + K dt A^m ((h' - h'_r) / L)^n = 0
//     is solved cell by cell in one pass (closed form for n = 1, a few Newton
//     steps otherwise). That is why the solver is O(n) and stable at any dt;
//  4. carry the eroded rock downstream: sediment entering a lake settles there
//     (up to the water surface), the rest reaches the sea or leaves the map;
//  5. thermal slope limit: where ground is steeper than `talus` toward a
//     neighbour, loose rock slides down to it.
// Sea cells (height below 0) and the map edge are base level: they stay as
// they are and the land drains to sea level (0 m), not to the sea floor.
// Every move is counted, so removed = deposited + reached the sea, exactly.

/// Relief cells per atlas cell edge: 4 gives 8 m cells.
pub const RELIEF_SCALE: i32 = 4;

/// The knobs of the erosion model. Units are metres and years.
#[derive(Clone, Debug)]
pub struct ErodeSettings {
    /// Seeds the small-scale roughness added when upsampling.
    pub seed: u32,
    pub steps: u32,
    /// Years per step.
    pub dt: f64,
    /// Erodibility K of a rock with factor 1, in m^(1-2m) per year.
    pub k: f64,
    /// Exponents of the stream-power law: area A (m^2) to the m, slope to the n.
    pub m: f64,
    pub n: f64,
    /// Rock uplift U, metres per year, on every land cell.
    pub uplift: f64,
    /// Steepest stable ground (rise over run; 0.8 is about 39 degrees).
    pub talus: f64,
    /// Thermal passes per step.
    pub talus_passes: u32,
    /// Height of the roughness added when upsampling (metres), so channels
    /// have something to choose between. Fades out toward the coast.
    pub noise: f32,
    /// K factor by rock name (case ignored); rocks not listed take 1.
    pub erodibility: Vec<(String, f64)>,
}

impl Default for ErodeSettings {
    fn default() -> Self {
        let rocks = [("chalk", 4.0), ("sandstone", 3.0), ("limestone", 2.0), ("slate", 1.0), ("marble", 0.8), ("basalt", 0.5), ("granite", 0.4)];
        ErodeSettings {
            seed: 1,
            steps: 200,
            dt: 500.0,
            k: 5e-6,
            m: 0.5,
            n: 1.0,
            uplift: 0.0,
            talus: 0.8,
            talus_passes: 2,
            noise: 1.0,
            erodibility: rocks.iter().map(|&(r, f)| (r.to_string(), f)).collect(),
        }
    }
}

impl ErodeSettings {
    /// The K factor for a rock name.
    pub fn factor(&self, rock: &str) -> f64 {
        self.erodibility.iter().find(|(r, _)| r.eq_ignore_ascii_case(rock)).map_or(1.0, |&(_, f)| f)
    }
}

/// Where the rock went, in cubic metres, summed over the run.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Budget {
    /// Taken off the land (by rivers and by slides).
    pub eroded: f64,
    /// Laid down again on land (lake beds, foot of slides).
    pub deposited: f64,
    /// Carried into the sea or off the map edge.
    pub to_sea: f64,
    /// Added by uplift.
    pub uplifted: f64,
}

/// The result of `erode`.
#[derive(Clone, Debug)]
pub struct Eroded {
    /// A copy of the input with `relief` set (heights to 0.1 m) and each
    /// atlas cell's `elevation` moved by its block's mean change.
    pub atlas: Atlas,
    /// The relief before erosion (the upsampled atlas) and after (unrounded).
    pub before: Relief,
    pub after: Relief,
    pub budget: Budget,
}

/// Flow on a relief: what `drainage` returns.
#[derive(Clone, Debug)]
pub struct Drainage {
    /// The cell each cell drains to; an outlet (sea, map edge) names itself.
    pub receiver: Vec<usize>,
    /// Area draining through each cell, itself included, in m^2.
    pub area: Vec<f64>,
}

/// The atlas at 8 m: its smooth heights plus a little seeded roughness on
/// land. An atlas that already has a relief (eroded before) gives that back.
pub fn upsample(atlas: &Atlas, seed: u32, noise: f32) -> Relief {
    if let Some(r) = &atlas.relief {
        return r.clone();
    }
    let s = RELIEF_SCALE;
    let mut r = Relief::new(s, atlas.w * s, atlas.h * s, 0.0);
    let cell = r.cell_m();
    for y in 0..r.h {
        for x in 0..r.w {
            let e = atlas.elevation_at((x as f32 + 0.5) / s as f32, (y as f32 + 0.5) / s as f32);
            let i = r.idx(x, y);
            if e < 0.0 {
                r.height[i] = e;
                continue;
            }
            let bumps = (fbm2(seed, x as f32 * cell / 40.0, y as f32 * cell / 40.0, 3) - 0.5) * 2.0;
            let grain = hash3(x, y, 0, seed ^ 0x5EED) as f32 / u32::MAX as f32 - 0.5;
            let land = (e / 3.0).min(1.0);
            r.height[i] = (e + (bumps + 0.2 * grain) * noise * land).max(0.0);
        }
    }
    r
}

/// The routed relief for one step.
struct Routed {
    /// Filled surface (pits raised to their spill level).
    hf: Vec<f64>,
    /// Receiver of each cell (itself for an outlet) and the distance to it.
    rec: Vec<usize>,
    len: Vec<f64>,
    /// Every cell after its receiver (Braun and Willett's stack).
    stack: Vec<usize>,
    /// Drainage area, m^2.
    area: Vec<f64>,
}

fn route_relief(z: &[f64], fixed: &[bool], w: i32, h: i32, cell: f64) -> Routed {
    let n = z.len();
    let at = |x: i32, y: i32| (y * w + x) as usize;
    let inside = |x: i32, y: i32| x >= 0 && y >= 0 && x < w && y < h;
    // Priority flood from the outlets.
    let mut hf = z.to_vec();
    let mut queued = vec![false; n];
    let mut heap = BinaryHeap::new();
    let mut seq = 0u64;
    for i in 0..n {
        if fixed[i] {
            queued[i] = true;
            heap.push(Item { level: z[i], seq, idx: i });
            seq += 1;
        }
    }
    while let Some(Item { level, idx, .. }) = heap.pop() {
        let (x, y) = (idx as i32 % w, idx as i32 / w);
        for d in 0..8 {
            let (nx, ny) = (x + DX[d], y + DY[d]);
            if !inside(nx, ny) || queued[at(nx, ny)] {
                continue;
            }
            let j = at(nx, ny);
            queued[j] = true;
            hf[j] = z[j].max(level + EPS);
            heap.push(Item { level: hf[j], seq, idx: j });
            seq += 1;
        }
    }
    // Receivers: steepest descent on the filled surface (one always exists off
    // the outlets, since the flood left every cell above the one that reached it).
    let mut rec: Vec<usize> = (0..n).collect();
    let mut len = vec![cell; n];
    for i in 0..n {
        if fixed[i] {
            continue;
        }
        let (x, y) = (i as i32 % w, i as i32 / w);
        let mut best = 0.0;
        for d in 0..8 {
            let (nx, ny) = (x + DX[d], y + DY[d]);
            if !inside(nx, ny) {
                continue;
            }
            let dist = if d % 2 == 1 { cell * std::f64::consts::SQRT_2 } else { cell };
            let j = at(nx, ny);
            let slope = (hf[i] - hf[j]) / dist;
            if slope > best {
                best = slope;
                rec[i] = j;
                len[i] = dist;
            }
        }
    }
    // The stack: donors listed in one flat array (CSR, like a sparse matrix),
    // then a depth-first walk up from each outlet.
    let mut start = vec![0usize; n + 1];
    for i in 0..n {
        if rec[i] != i {
            start[rec[i] + 1] += 1;
        }
    }
    for i in 0..n {
        start[i + 1] += start[i];
    }
    let mut fill = start.clone();
    let mut donors = vec![0usize; start[n]];
    for i in 0..n {
        if rec[i] != i {
            donors[fill[rec[i]]] = i;
            fill[rec[i]] += 1;
        }
    }
    let mut stack = Vec::with_capacity(n);
    let mut todo = Vec::new();
    for b in 0..n {
        if rec[b] != b {
            continue;
        }
        todo.push(b);
        while let Some(c) = todo.pop() {
            stack.push(c);
            todo.extend_from_slice(&donors[start[c]..start[c + 1]]);
        }
    }
    let mut area = vec![cell * cell; n];
    for &i in stack.iter().rev() {
        if rec[i] != i {
            area[rec[i]] += area[i];
        }
    }
    Routed { hf, rec, len, stack, area }
}

/// Outlets of a relief: sea cells (below 0) and the map edge.
fn outlets(r: &Relief) -> Vec<bool> {
    (0..r.height.len())
        .map(|i| {
            let (x, y) = (i as i32 % r.w, i as i32 / r.w);
            r.height[i] < 0.0 || x == 0 || y == 0 || x == r.w - 1 || y == r.h - 1
        })
        .collect()
}

/// Where water goes on a relief (land drains to sea level).
pub fn drainage(r: &Relief) -> Drainage {
    let fixed = outlets(r);
    let z: Vec<f64> = r.height.iter().map(|&v| (v as f64).max(0.0)).collect();
    let rt = route_relief(&z, &fixed, r.w, r.h, r.cell_m() as f64);
    Drainage { receiver: rt.rec, area: rt.area }
}

/// Run the stream-power model on a relief. `k[i]` is the erodibility of cell
/// i (m^(1-2m)/yr). Deterministic: no randomness, ties by index.
pub fn erode_relief(relief: &Relief, k: &[f64], set: &ErodeSettings) -> (Relief, Budget) {
    let n = relief.height.len();
    let (w, h) = (relief.w, relief.h);
    let cell = relief.cell_m() as f64;
    let fixed = outlets(relief);
    // Working heights: the sea counts as sea level, so land drains to 0 m.
    let mut z: Vec<f64> = relief.height.iter().map(|&v| (v as f64).max(0.0)).collect();
    // Tallies in metres of height over one cell; turned into m^3 at the end.
    let (mut eroded, mut deposited, mut to_sea, mut uplifted) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    let mut e = vec![0.0f64; n];
    let mut qs = vec![0.0f64; n];
    let mut hn = vec![0.0f64; n];
    let mut delta = vec![0.0f64; n];
    for _ in 0..set.steps {
        // 1. Uplift.
        if set.uplift != 0.0 {
            for i in 0..n {
                if !fixed[i] {
                    z[i] += set.uplift * set.dt;
                    uplifted += set.uplift * set.dt;
                }
            }
        }
        // 2. Route.
        let rt = route_relief(&z, &fixed, w, h, cell);
        // 3. Implicit stream-power erosion, receivers first. `hn` is the new
        // surface water runs on: ground, or a lake's level where it is flooded.
        for &i in &rt.stack {
            let r = rt.rec[i];
            e[i] = 0.0;
            if r == i {
                hn[i] = z[i];
                continue;
            }
            if rt.hf[i] > z[i] {
                // Lake: still water does not cut; its level follows the outlet.
                hn[i] = z[i].max(hn[r] + (rt.hf[i] - rt.hf[r]));
                continue;
            }
            let (h0, hr) = (z[i], hn[r]);
            let f = k[i] * set.dt * rt.area[i].powf(set.m) / rt.len[i].powf(set.n);
            let x = if h0 <= hr {
                h0
            } else if set.n == 1.0 {
                (h0 + f * hr) / (1.0 + f)
            } else {
                // Newton on g(x) = x - h0 + f (x - hr)^n, from x = h0 down.
                let mut x = h0;
                for _ in 0..30 {
                    let s = (x - hr).max(1e-12);
                    let g = x - h0 + f * s.powf(set.n);
                    let dg = 1.0 + f * set.n * s.powf(set.n - 1.0);
                    let nx = (x - g / dg).clamp(hr, h0);
                    let done = (nx - x).abs() < 1e-9;
                    x = nx;
                    if done {
                        break;
                    }
                }
                x
            };
            hn[i] = x;
            e[i] = h0 - x;
        }
        // 4. Sediment, top of each catchment first.
        qs.iter_mut().for_each(|q| *q = 0.0);
        for &i in rt.stack.iter().rev() {
            let mut q = qs[i] + e[i];
            eroded += e[i];
            z[i] -= e[i];
            if fixed[i] {
                to_sea += q;
                continue;
            }
            let room = hn[i] - z[i];
            if room > 0.0 && q > 0.0 {
                let d = q.min(room);
                z[i] += d;
                deposited += d;
                q -= d;
            }
            qs[rt.rec[i]] += q;
        }
        // 5. Thermal slope limit (each pass reads one snapshot, then applies).
        for _ in 0..set.talus_passes {
            delta.iter_mut().for_each(|d| *d = 0.0);
            for i in 0..n {
                if fixed[i] {
                    continue;
                }
                let (x, y) = (i as i32 % w, i as i32 / w);
                let mut excess = [0.0f64; 8];
                let (mut sum, mut worst) = (0.0, 0.0f64);
                for d in 0..8 {
                    let (nx, ny) = (x + DX[d], y + DY[d]);
                    if nx < 0 || ny < 0 || nx >= w || ny >= h {
                        continue;
                    }
                    let dist = if d % 2 == 1 { cell * std::f64::consts::SQRT_2 } else { cell };
                    let over = z[i] - z[(ny * w + nx) as usize] - set.talus * dist;
                    if over > 0.0 {
                        excess[d] = over;
                        sum += over;
                        worst = worst.max(over);
                    }
                }
                if sum == 0.0 {
                    continue;
                }
                // Move half the worst excess, shared by how far over each side is.
                let out = 0.5 * worst;
                delta[i] -= out;
                eroded += out;
                for d in 0..8 {
                    if excess[d] > 0.0 {
                        let j = ((y + DY[d]) * w + x + DX[d]) as usize;
                        let share = out * excess[d] / sum;
                        if fixed[j] {
                            to_sea += share;
                        } else {
                            delta[j] += share;
                            deposited += share;
                        }
                    }
                }
            }
            for i in 0..n {
                z[i] += delta[i];
            }
        }
    }
    let a = cell * cell;
    let budget = Budget { eroded: eroded * a, deposited: deposited * a, to_sea: to_sea * a, uplifted: uplifted * a };
    let mut out = relief.clone();
    for i in 0..n {
        if !fixed[i] {
            out.height[i] = z[i] as f32;
        }
    }
    (out, budget)
}

/// Erode an atlas: upsample to an 8 m relief, run `set.steps` steps, and
/// return a new atlas carrying the result. The input is not changed.
pub fn erode(atlas: &Atlas, set: &ErodeSettings) -> Eroded {
    let before = upsample(atlas, set.seed, set.noise);
    let s = before.scale;
    let k: Vec<f64> = (0..before.height.len())
        .map(|i| {
            let (x, y) = ((i as i32 % before.w / s).min(atlas.w - 1), (i as i32 / before.w / s).min(atlas.h - 1));
            let rock = atlas.rocks.get(atlas.rock[atlas.idx(x, y)] as usize);
            set.k * rock.map_or(1.0, |r| set.factor(r))
        })
        .collect();
    let (after, budget) = erode_relief(&before, &k, set);
    let mut out = atlas.clone();
    if !out.name.ends_with("(eroded)") {
        out.name = format!("{} (eroded)", out.name);
    }
    // Atlas cells move by their block's mean change, so a cell nothing touched
    // keeps the height the author gave it.
    for ay in 0..atlas.h {
        for ax in 0..atlas.w {
            let mut sum = 0.0f64;
            for y in ay * s..(ay + 1) * s {
                for x in ax * s..(ax + 1) * s {
                    let i = before.idx(x, y);
                    sum += (after.height[i] - before.height[i]) as f64;
                }
            }
            let i = atlas.idx(ax, ay);
            let e = (atlas.elevation[i] as f64 + sum / (s * s) as f64).round().clamp(-32000.0, 32000.0) as i16;
            // Land stays land and sea stays sea: the coast is the author's.
            out.elevation[i] = if atlas.elevation[i] < 0 { e.min(-1) } else { e.max(0) };
        }
    }
    let mut rel = after.clone();
    // To 0.1 m for the file; a shallow sea cell stays sea (below 0).
    rel.height.iter_mut().for_each(|v| *v = if *v < 0.0 { ((*v * 10.0).round() / 10.0).min(-0.1) } else { (*v * 10.0).round() / 10.0 });
    out.relief = Some(rel);
    Eroded { atlas: out, before, after, budget }
}
