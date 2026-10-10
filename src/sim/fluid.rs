//! Liquids. Each cell holds 0..=64 units of one liquid.
//!
//! Three passes per tick over awake chunks:
//! 1. **Gravity**, bottom level first: liquid drops into the cell below as far
//!    as it has room, so a falling column moves one level per tick.
//! 2. **Spread** from a snapshot: a resting cell sends a fifth of the
//!    difference to each lower neighbour. A fifth (four neighbours plus
//!    itself) means a cell can never give away more than it has, nor a
//!    neighbour receive more than it can hold, so liquid is conserved exactly.
//! 3. **Trickle**, in place and in random directions: moves single units
//!    across the small differences pass 2 rounds away, lets thin films pour
//!    off ledges, and evaporates thin films.
//!
//! Two different liquids meeting may react (see [`react`]): water on lava
//! flashes to steam and the lava crusts into obsidian.
//!
//! Every sideways move is also recorded in the world's flow field
//! (`flow_x`, `flow_y`), smoothed over a few ticks, so things floating in
//! the liquid can be carried along by it.

use crate::geom::DIRS4;
use crate::material::{MatId, AIR};
use crate::rng::Rng;
use crate::world::{World, FULL};

pub fn step(
    w: &mut World,
    chunks: &[usize],
    rng: &mut Rng,
    delta: &mut [i16],
    touched: &mut Vec<u32>,
    tick: u64,
) {
    let layer = w.layer();
    let (ww, hh) = (w.w, w.h);

    // 0. Let the flow field fade. A cell still flowing keeps its chunk awake,
    // so no stale current is left behind in a sleeping chunk.
    for &c in chunks {
        let (x0, y0, x1, y1, z) = w.wake.bounds(c);
        for y in y0..y1 {
            let row = w.idx(0, y, z);
            for x in x0..x1 {
                let i = row + x as usize;
                let (fx, fy) = (w.flow_x[i], w.flow_y[i]);
                if fx == 0.0 && fy == 0.0 {
                    continue;
                }
                if fx.abs() + fy.abs() < 0.01 {
                    w.flow_x[i] = 0.0;
                    w.flow_y[i] = 0.0;
                } else {
                    w.flow_x[i] = fx * FLOW_KEEP;
                    w.flow_y[i] = fy * FLOW_KEEP;
                    w.wake.touch(i);
                }
            }
        }
    }

    // 1. Gravity. `chunks` is ordered bottom level first.
    for &c in chunks {
        let (x0, y0, x1, y1, z) = w.wake.bounds(c);
        if z == 0 {
            continue;
        }
        for y in y0..y1 {
            let row = w.idx(0, y, z);
            for x in x0..x1 {
                let i = row + x as usize;
                let a = w.fluid[i];
                if a == 0 || w.tile[i].seals_below() {
                    continue;
                }
                let b = i - layer;
                if w.tile[b].is_wall() {
                    continue;
                }
                if w.fluid[b] > 0 && w.fluid_mat[b] != w.fluid_mat[i] {
                    react(w, i, b);
                    continue;
                }
                let room = FULL - w.fluid[b];
                if room > 0 {
                    transfer(w, i, b, a.min(room));
                }
            }
        }
    }

    // 2. Spread, from a snapshot into `delta`.
    touched.clear();
    for &c in chunks {
        let (x0, y0, x1, y1, z) = w.wake.bounds(c);
        for y in y0..y1 {
            let row = w.idx(0, y, z);
            for x in x0..x1 {
                let i = row + x as usize;
                let a = w.fluid[i];
                if a == 0 {
                    continue;
                }
                let fm = w.fluid_mat[i];
                let visc = w.mats.get(fm).viscosity() as u64;
                if visc > 1 && (tick + i as u64) % visc != 0 {
                    continue;
                }
                if falling(w, i, z, layer) {
                    continue;
                }
                for (dx, dy) in DIRS4 {
                    let (nx, ny) = (x + dx, y + dy);
                    if nx < 0 || ny < 0 || nx >= ww || ny >= hh {
                        continue;
                    }
                    let n = (i as isize + dx as isize + dy as isize * ww as isize) as usize;
                    if w.tile[n].is_wall() {
                        continue;
                    }
                    let f = w.fluid[n];
                    if f > 0 && w.fluid_mat[n] != fm {
                        continue;
                    }
                    if a > f {
                        let flow = ((a - f) / 5) as i16;
                        if flow > 0 {
                            delta[i] -= flow;
                            delta[n] += flow;
                            record(w, i, n, dx, dy, flow as f32);
                            if f == 0 {
                                w.fluid_mat[n] = fm;
                            }
                            touched.push(i as u32);
                            touched.push(n as u32);
                        }
                    }
                }
            }
        }
    }
    for &t in touched.iter() {
        let i = t as usize;
        let d = delta[i];
        if d == 0 {
            continue;
        }
        delta[i] = 0;
        let v = (w.fluid[i] as i16 + d).clamp(0, FULL as i16) as u8;
        w.fluid[i] = v;
        if v == 0 {
            w.fluid_mat[i] = AIR;
        }
        w.wake.touch(i);
    }

    // 3. Trickle and evaporation, in place.
    for &c in chunks {
        let (x0, y0, x1, y1, z) = w.wake.bounds(c);
        for y in y0..y1 {
            let row = w.idx(0, y, z);
            for x in x0..x1 {
                let i = row + x as usize;
                let a = w.fluid[i];
                if a == 0 {
                    continue;
                }
                let fm = w.fluid_mat[i];
                let m = w.mats.get(fm);
                let (visc, evap) = (m.viscosity() as u64, m.evaporate);
                if visc > 1 && (tick + i as u64) % visc != 0 {
                    continue;
                }
                if a <= 2 && evap > 0.0 && w.temp[i] > 0.0 {
                    let warm = 1.0 + (w.temp[i] - 15.0).max(0.0) / 15.0;
                    if rng.chance(evap * warm) {
                        w.fluid[i] -= 1;
                        if w.fluid[i] == 0 {
                            w.fluid_mat[i] = AIR;
                        }
                        w.wake.touch(i);
                        continue;
                    }
                }
                let (dx, dy) = DIRS4[rng.below(4) as usize];
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= ww || ny >= hh {
                    continue;
                }
                let n = (i as isize + dx as isize + dy as isize * ww as isize) as usize;
                if w.tile[n].is_wall() {
                    continue;
                }
                let f = w.fluid[n];
                if f > 0 && w.fluid_mat[n] != fm {
                    react(w, i, n);
                    continue;
                }
                if a >= f + 2 || (f < a && drains(w, n, z, layer)) {
                    transfer(w, i, n, 1);
                } else if a >= 2 && f + 1 >= a {
                    // A gentle slope (each cell one lower than the last) or a
                    // flat: no neighbour is low enough, so look further downhill.
                    if let Some(m) = seek_low(w, n, a, fm, z, layer, rng) {
                        transfer(w, i, m, 1);
                    }
                }
            }
        }
    }
}

/// Scratch for the pressure pass, kept between ticks.
#[derive(Clone, Debug, Default)]
pub struct Pressure {
    mark: Vec<u32>,
    stamp: u32,
    queue: Vec<u32>,
    /// Most cells one body of water is searched through per tick.
    pub body_cap: usize,
    /// Most cells searched per tick in all. The search starts from a different
    /// awake chunk each tick, so when this runs out the bodies take turns.
    pub budget: usize,
    /// Moves made last tick (for benchmarks and tests).
    pub moves: u32,
}

impl Pressure {
    pub fn new() -> Pressure {
        Pressure { body_cap: 4096, budget: 10_000, ..Default::default() }
    }
}

/// 4. **Pressure.** The other passes only move liquid at its surface, so
/// water poured into one arm of a U-tube never rises in the other, and a hole
/// below the waterline only lets in what reaches it sideways. This pass
/// treats each body of brim-full cells (of one liquid, connected through
/// faces it can flow through) as one pipe: of all the cells around its edge,
/// it finds the free surface standing highest and the opening lowest, and if
/// the first is above the second it moves liquid from one to the other,
/// half the difference at most, so it never overshoots. Heads are measured
/// in levels plus the fraction of the cell filled, so U-tubes and flooded
/// compartments come level to within 2/64 of a cell.
///
/// Only bodies with a full cell in an awake chunk are searched, so a still
/// lake costs nothing; a search may spill into sleeping chunks and wakes
/// what it changes.
pub fn pressure(w: &mut World, chunks: &[usize], p: &mut Pressure, tick: u64) {
    p.moves = 0;
    if chunks.is_empty() {
        return;
    }
    if p.mark.len() != w.len() {
        p.mark = vec![0; w.len()];
        p.stamp = 0;
    }
    p.stamp = p.stamp.wrapping_add(1);
    if p.stamp == 0 {
        p.mark.fill(0);
        p.stamp = 1;
    }
    let layer = w.layer();
    let (ww, hh, dd) = (w.w, w.h, w.d);
    let mut spent = 0usize;
    let start = (tick as usize) % chunks.len();
    for k in 0..chunks.len() {
        if spent >= p.budget {
            break;
        }
        let c = chunks[(start + k) % chunks.len()];
        let (x0, y0, x1, y1, z) = w.wake.bounds(c);
        for y in y0..y1 {
            let row = w.idx(0, y, z);
            for x in x0..x1 {
                let seed = row + x as usize;
                if w.fluid[seed] != FULL || p.mark[seed] == p.stamp {
                    continue;
                }
                let fm = w.fluid_mat[seed];
                // Highest free surface (cell to take from, head) and lowest
                // opening (cell to fill, head), in levels.
                let mut src: Option<(usize, f32)> = None;
                let mut dst: Option<(usize, f32)> = None;
                p.queue.clear();
                p.queue.push(seed as u32);
                p.mark[seed] = p.stamp;
                let mut qi = 0;
                while qi < p.queue.len() && qi < p.body_cap {
                    let cur = p.queue[qi] as usize;
                    qi += 1;
                    let cz = (cur / layer) as i32;
                    let r = cur % layer;
                    let (cx, cy) = ((r % ww as usize) as i32, (r / ww as usize) as i32);
                    let mut look = |n: usize, above: bool| {
                        let f = w.fluid[n];
                        if f > 0 && w.fluid_mat[n] != fm {
                            return None;
                        }
                        if f == FULL {
                            return Some(n);
                        }
                        let nz = (n / layer) as i32;
                        let head = nz as f32 + f as f32 / FULL as f32;
                        if dst.is_none_or(|(_, h)| head < h) {
                            dst = Some((n, head));
                        }
                        if f > 0 {
                            if src.is_none_or(|(_, h)| head > h) {
                                src = Some((n, head));
                            }
                        } else if above {
                            // A brim-full cell open to the air: its surface is its top.
                            let h = cz as f32 + 1.0;
                            if src.is_none_or(|(_, sh)| h > sh) {
                                src = Some((cur, h));
                            }
                        }
                        None
                    };
                    let mut next: [Option<usize>; 6] = [None; 6];
                    for (slot, (dx, dy)) in DIRS4.iter().enumerate() {
                        let (nx, ny) = (cx + dx, cy + dy);
                        if nx < 0 || ny < 0 || nx >= ww || ny >= hh {
                            continue;
                        }
                        let n = (cur as isize + *dx as isize + *dy as isize * ww as isize) as usize;
                        if !w.tile[n].is_wall() {
                            next[slot] = look(n, false);
                        }
                    }
                    if cz > 0 && !w.tile[cur].seals_below() && !w.tile[cur - layer].is_wall() {
                        next[4] = look(cur - layer, false);
                    }
                    if cz + 1 < dd && !w.tile[cur + layer].seals_below() {
                        next[5] = look(cur + layer, true);
                    }
                    for n in next.into_iter().flatten() {
                        if p.mark[n] != p.stamp {
                            p.mark[n] = p.stamp;
                            p.queue.push(n as u32);
                        }
                    }
                }
                spent += qi;
                let (Some((s, hs)), Some((d, hd))) = (src, dst) else { continue };
                if s == d {
                    continue;
                }
                let diff = ((hs - hd) * FULL as f32 / 2.0).floor();
                if diff < 1.0 {
                    continue;
                }
                let room = FULL - w.fluid[d];
                let amt = (diff as u8).min(w.fluid[s]).min(room).min(FULL / 2);
                if amt > 0 {
                    transfer(w, s, d, amt);
                    p.moves += 1;
                }
                if spent >= p.budget {
                    return;
                }
            }
        }
    }
}

/// Liquid in `i` still has somewhere to fall.
#[inline]
fn falling(w: &World, i: usize, z: i32, layer: usize) -> bool {
    if z == 0 || w.tile[i].seals_below() {
        return false;
    }
    let b = i - layer;
    !w.tile[b].is_wall() && w.fluid[b] < FULL && (w.fluid[b] == 0 || w.fluid_mat[b] == w.fluid_mat[i])
}

/// How far `seek_low` walks along a liquid surface.
const SEEK_STEPS: u32 = 8;

/// Walk downhill across the surface from `start`, never climbing, and return
/// the first cell holding at most `a - 2` units of liquid `fm` (or one it
/// would pour off). Without this a slope whose cells each differ by one unit
/// is stable forever, since no single step is steep enough to trickle down.
fn seek_low(w: &World, start: usize, a: u8, fm: MatId, z: i32, layer: usize, rng: &mut Rng) -> Option<usize> {
    let ww = w.w as usize;
    let (mut cur, mut prev) = (start, usize::MAX);
    for _ in 0..SEEK_STEPS {
        let lc = w.fluid[cur];
        let (cx, cy) = ((cur % layer) % ww, (cur % layer) / ww);
        let r = rng.below(4) as usize;
        let mut best: Option<(usize, u8)> = None;
        for k in 0..4 {
            let (dx, dy) = DIRS4[(r + k) & 3];
            let (nx, ny) = (cx as i32 + dx, cy as i32 + dy);
            if nx < 0 || ny < 0 || nx >= w.w || ny >= w.h {
                continue;
            }
            let n = (cur as isize + dx as isize + dy as isize * ww as isize) as usize;
            let f = w.fluid[n];
            if n == prev || w.tile[n].is_wall() || (f > 0 && w.fluid_mat[n] != fm) || f > lc {
                continue;
            }
            if best.is_none_or(|(_, bf)| f < bf) {
                best = Some((n, f));
            }
        }
        let (n, f) = best?;
        if f + 2 <= a || (f < a && drains(w, n, z, layer)) {
            return Some(n);
        }
        (prev, cur) = (cur, n);
    }
    None
}

/// Liquid entering cell `n` would fall straight through it (a ledge).
#[inline]
fn drains(w: &World, n: usize, z: i32, layer: usize) -> bool {
    if z == 0 || w.tile[n].seals_below() {
        return false;
    }
    let b = n - layer;
    !w.tile[b].is_wall() && w.fluid[b] < FULL
}

/// How much of the flow field survives each tick; the rest of each tick's
/// flux is added in, so it is a running average over roughly 1/(1-KEEP) ticks.
pub const FLOW_KEEP: f32 = 0.85;

/// Note `amt` units crossing from cell `i` to cell `n` in direction (dx, dy).
/// Both cells count it, as it passes through the face between them.
#[inline]
fn record(w: &mut World, i: usize, n: usize, dx: i32, dy: i32, amt: f32) {
    let f = amt * (1.0 - FLOW_KEEP);
    let (fx, fy) = (dx as f32 * f, dy as f32 * f);
    w.flow_x[i] += fx;
    w.flow_y[i] += fy;
    w.flow_x[n] += fx;
    w.flow_y[n] += fy;
}

/// Move `amt` units from one cell to another, mixing temperature.
#[inline]
fn transfer(w: &mut World, from: usize, to: usize, amt: u8) {
    if amt == 0 {
        return;
    }
    let layer = w.layer();
    if from / layer == to / layer {
        // Sideways (a trickle, or a hop along the surface): record the flow.
        let ww = w.w as usize;
        let (fx, fy) = ((from % layer % ww) as i32, (from % layer / ww) as i32);
        let (tx, ty) = ((to % layer % ww) as i32, (to % layer / ww) as i32);
        record(w, from, to, (tx - fx).signum(), (ty - fy).signum(), amt as f32);
    }
    let fm = w.fluid_mat[from];
    // The air in a cell counts for a little thermal mass.
    let tf = 8.0 + w.fluid[to] as f32;
    w.temp[to] = (w.temp[to] * tf + w.temp[from] * amt as f32) / (tf + amt as f32);
    w.fluid[from] -= amt;
    if w.fluid[from] == 0 {
        w.fluid_mat[from] = AIR;
    }
    w.fluid[to] += amt;
    w.fluid_mat[to] = fm;
    w.wake.touch(from);
    w.wake.touch(to);
}

fn liquid_temp(w: &World, i: usize) -> f32 {
    w.mats.get(w.fluid_mat[i]).self_temp.unwrap_or(w.temp[i])
}

/// Two different liquids touch. If the hotter one is hot enough to boil the
/// colder, the colder flashes to gas; if the colder one is cold enough to
/// freeze the hotter, the hotter solidifies. Returns whether anything
/// happened. Liquids that do not react simply do not mix.
pub fn react(w: &mut World, i: usize, j: usize) -> bool {
    let (ti, tj) = (liquid_temp(w, i), liquid_temp(w, j));
    let (hot, cold, th, tc) = if ti >= tj { (i, j, ti, tj) } else { (j, i, tj, ti) };
    let boil = w.mats.get(w.fluid_mat[cold]).boil.filter(|&(bt, _)| th > bt);
    let freeze = w.mats.get(w.fluid_mat[hot]).freeze.filter(|&(ft, _)| tc < ft);
    if boil.is_none() && freeze.is_none() {
        return false;
    }
    if let Some((_, gas)) = boil {
        let amt = w.fluid[cold];
        w.fluid[cold] = 0;
        w.fluid_mat[cold] = AIR;
        w.add_gas_idx(cold, gas, amt.saturating_mul(3));
    }
    if let Some((_, solid)) = freeze {
        w.solidify(hot, solid);
    }
    true
}
