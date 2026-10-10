//! Heat, phase changes and fire.
//!
//! Temperature diffuses between the six face neighbours of every cell, with
//! conductance taken from what fills the cell (the wall's material, a liquid,
//! or air) and change scaled by heat capacity. The step is symmetric between
//! each pair of cells, so heat is conserved. Hot air rises: through open
//! cells, heat moves upward faster than downward.
//!
//! Cells under open sky drift toward the outdoor temperature; buried cells
//! drift very slowly toward the deep-earth temperature. That is the world's
//! heat sink.
//!
//! After conduction, each cell checks its phase (ice melts, water boils or
//! freezes) and fire (flammable walls and floors ignite above their ignition
//! point, burn for a while giving off heat and smoke, then fall to ash).

use crate::material::AIR;
use crate::rng::Rng;
use crate::tile::Tile;
use crate::world::{World, FULL};

/// Degrees a burning cell gains per tick, and the hottest a fire gets.
pub const FIRE_HEAT: f32 = 25.0;
pub const FIRE_MAX: f32 = 1000.0;

#[inline(always)]
fn thermal(w: &World, i: usize) -> (f32, f32) {
    let m = if w.tile[i].is_wall() {
        w.mats.get(w.mat[i])
    } else if w.fluid[i] >= 16 {
        w.mats.get(w.fluid_mat[i])
    } else {
        w.mats.get(AIR)
    };
    (m.conduct, m.heat_cap)
}

/// A change in temperature (per tick) big enough to keep a chunk awake.
const WAKE_CHANGE: f32 = 0.05;
/// Heat arriving in a sleeping cell wakes it only when it changes the cell by
/// this much in one tick (a fire next door). Slow seepage just accumulates
/// until the rolling sweep wakes the chunk.
const WAKE_JUMP: f32 = 0.5;

pub fn step(w: &mut World, chunks: &[usize], rng: &mut Rng, next: &mut [f32], edge: &mut Vec<(u32, f32)>) {
    let layer = w.layer();
    let (ww, hh, dd) = (w.w, w.h, w.d);
    let wwu = ww as usize;
    let (ambient, ground) = (w.ambient, w.ground_temp);
    let (cw, per) = (w.wake.cw as usize, (w.wake.cw * w.wake.ch) as usize);

    // 1. Conduction into `next`. Heat that flows across into a sleeping
    // chunk is collected in `edge` and handed over afterwards, so it is
    // conserved even though the sleeping side is not simulated.
    edge.clear();
    for &c in chunks {
        let (x0, y0, x1, y1, z) = w.wake.bounds(c);
        // Neighbouring chunks awake: west, east, north, south, below, above.
        let awake = [
            x0 > 0 && w.wake.is_awake(c - 1),
            x1 < ww && w.wake.is_awake(c + 1),
            y0 > 0 && w.wake.is_awake(c - cw),
            y1 < hh && w.wake.is_awake(c + cw),
            z > 0 && w.wake.is_awake(c - per),
            z < dd - 1 && w.wake.is_awake(c + per),
        ];
        for y in y0..y1 {
            let row = w.idx(0, y, z);
            for x in x0..x1 {
                let i = row + x as usize;
                let t = w.temp[i];
                let (ci, cap) = thermal(w, i);
                let mut flow = 0.0;
                // `conv` scales the flow (convection); `live`: the neighbour is simulated too.
                let mut side = |n: usize, conv: f32, live: bool| {
                    let (cn, _) = thermal(w, n);
                    let f = ci.min(cn) * conv * (w.temp[n] - t);
                    flow += f;
                    if !live {
                        edge.push((n as u32, -f));
                    }
                };
                if x > 0 {
                    side(i - 1, 1.0, x > x0 || awake[0]);
                }
                if x < ww - 1 {
                    side(i + 1, 1.0, x < x1 - 1 || awake[1]);
                }
                if y > 0 {
                    side(i - wwu, 1.0, y > y0 || awake[2]);
                }
                if y < hh - 1 {
                    side(i + wwu, 1.0, y < y1 - 1 || awake[3]);
                }
                if z > 0 {
                    let n = i - layer;
                    let open = !w.tile[i].seals_below() && !w.tile[n].is_wall();
                    let conv = if !open { 1.0 } else if w.temp[n] > t { 1.6 } else { 0.4 };
                    side(n, conv, awake[4]);
                }
                if z < dd - 1 {
                    let n = i + layer;
                    let open = !w.tile[n].seals_below() && !w.tile[i].is_wall();
                    let conv = if !open { 1.0 } else if t > w.temp[n] { 1.6 } else { 0.4 };
                    side(n, conv, awake[5]);
                }
                let mut nt = t + flow / cap;
                if !w.tile[i].is_wall() && w.sky[i] > 0 {
                    nt += (ambient - nt) * 0.01 * (w.sky[i] as f32 / 15.0);
                } else {
                    nt += (ground - nt) * 0.0004;
                }
                if w.fire[i] > 0 {
                    nt = (nt + FIRE_HEAT).min(FIRE_MAX.max(nt));
                }
                if w.fluid[i] > 0 {
                    if let Some(st) = w.mats.get(w.fluid_mat[i]).self_temp {
                        nt = st;
                    }
                }
                next[i] = nt;
            }
        }
    }
    for &(n, q) in edge.iter() {
        let n = n as usize;
        let d = q / thermal(w, n).1;
        w.temp[n] += d;
        if d.abs() > WAKE_JUMP {
            w.wake.touch(n);
        }
    }

    // 2. Write back, then phase changes and fire.
    for &c in chunks {
        let (x0, y0, x1, y1, z) = w.wake.bounds(c);
        for y in y0..y1 {
            let row = w.idx(0, y, z);
            for x in x0..x1 {
                let i = row + x as usize;
                // Always write back, however small the change: dropping tiny
                // gains while keeping the matching losses would leak heat.
                let nt = next[i];
                if (nt - w.temp[i]).abs() > WAKE_CHANGE {
                    w.wake.touch(i);
                }
                w.temp[i] = nt;
                phase(w, i);
                burn(w, i, x, y, z, layer, rng);
            }
        }
    }
}

fn phase(w: &mut World, i: usize) {
    let t = w.temp[i];
    let tile = w.tile[i];
    if tile != Tile::Open {
        if let Some((mt, liquid)) = w.mats.get(w.mat[i]).melt {
            if t > mt {
                let amount = if tile.is_wall() { FULL } else { 24 };
                w.tile[i] = Tile::Open;
                w.mat[i] = crate::material::AIR;
                if w.fluid[i] == 0 || w.fluid_mat[i] == liquid {
                    w.fluid[i] = w.fluid[i].saturating_add(amount).min(FULL);
                    w.fluid_mat[i] = liquid;
                }
                w.touch(i);
                w.sky_dirty = true;
                return;
            }
        }
    }
    let f = w.fluid[i];
    if f > 0 {
        let m = w.mats.get(w.fluid_mat[i]);
        let (freeze, boil, held) = (m.freeze, m.boil, m.self_temp.is_some());
        if !held {
            if let Some((ft, solid)) = freeze {
                if t < ft {
                    w.solidify(i, solid);
                    return;
                }
            }
        }
        if let Some((bt, gas)) = boil {
            if t > bt {
                let amt = f.min(4);
                w.fluid[i] -= amt;
                if w.fluid[i] == 0 {
                    w.fluid_mat[i] = AIR;
                }
                w.add_gas_idx(i, gas, amt * 3);
                w.temp[i] = bt; // boiling soaks up the heat
                w.touch(i);
            }
        }
    }
}

/// Is there water (a liquid that does not burn hot itself, unlike lava)
/// against any face of cell `i`?
fn water_beside(w: &World, i: usize, x: i32, y: i32, z: i32, layer: usize) -> bool {
    let ww = w.w as usize;
    let mut n = [None; 6];
    if x > 0 {
        n[0] = Some(i - 1);
    }
    if x < w.w - 1 {
        n[1] = Some(i + 1);
    }
    if y > 0 {
        n[2] = Some(i - ww);
    }
    if y < w.h - 1 {
        n[3] = Some(i + ww);
    }
    if z > 0 {
        n[4] = Some(i - layer);
    }
    if z < w.d - 1 {
        n[5] = Some(i + layer);
    }
    n.into_iter().flatten().any(|j| w.fluid[j] >= 8 && w.mats.get(w.fluid_mat[j]).self_temp.is_none())
}

fn burn(w: &mut World, i: usize, x: i32, y: i32, z: i32, layer: usize, rng: &mut Rng) {
    let fire = w.fire[i];
    if fire == 0 {
        if w.tile[i] == Tile::Open || w.fluid[i] >= 8 {
            return;
        }
        let m = w.mats.get(w.mat[i]);
        if let Some(ig) = m.ignite {
            if w.temp[i] > ig {
                w.fire[i] = m.burn_ticks.max(1);
                w.touch(i);
            }
        }
        return;
    }
    // Water puts fire out: in the cell (a burning floor), or splashing against
    // it (a burning wall holds no water, but water beside it soaks it).
    if w.fluid[i] >= 8 || (w.tile[i].is_wall() && water_beside(w, i, x, y, z, layer) && rng.chance(0.25)) {
        w.quench(i);
        return;
    }
    // Open to the weather: a floor under the sky, or a wall whose top is.
    let exposed = w.open_sky[i] || (z < w.d - 1 && w.open_sky[i + layer]);
    if exposed && w.rain > 0.0 && rng.chance(w.rain * 0.06) {
        w.quench(i);
        return;
    }
    w.fire[i] = fire - 1;
    w.touch(i);
    let gas = w.mats.get(w.mat[i]).burn_gas;
    if gas != AIR && rng.chance(0.5) {
        // Smoke comes out of the cell, or above a burning wall.
        let target = if !w.tile[i].is_wall() {
            Some(i)
        } else if z < w.d - 1 && !w.tile[i + layer].seals_below() {
            Some(i + layer)
        } else {
            None
        };
        if let Some(t) = target {
            w.add_gas_idx(t, gas, 6);
        }
    }
    // Wind fans an exposed fire: it spreads faster, and mostly downwind.
    let (wx, wy) = if exposed { w.wind } else { (0.0, 0.0) };
    let speed = (wx * wx + wy * wy).sqrt();
    let spread = w.mats.get(w.mat[i]).spread * (1.0 + 2.0 * speed.min(1.5));
    if spread > 0.0 && rng.chance(spread.min(1.0)) {
        let (dx, dy, dz) = if speed > 0.0 && rng.chance(speed.min(0.75)) {
            if rng.f32() < wx.abs() / (wx.abs() + wy.abs()) {
                (wx.signum() as i32, 0, 0)
            } else {
                (0, wy.signum() as i32, 0)
            }
        } else {
            [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)][rng.below(6) as usize]
        };
        let n = crate::geom::Pos::new(x + dx, y + dy, z + dz);
        if w.index(n).is_some_and(|j| w.fire[j] == 0) {
            w.ignite(n);
        }
    }
    if fire == 1 {
        burn_out(w, i, z, layer);
    }
}

/// A cell has burnt away. A wall collapses; a floor over open space leaves a
/// hole; on solid ground, whatever is left (ash) stays as a floor.
fn burn_out(w: &mut World, i: usize, z: i32, layer: usize) {
    let rest = w.mats.get(w.mat[i]).burns_to;
    let ground_below = z > 0 && w.tile[i - layer].is_wall();
    if ground_below && rest != AIR {
        w.tile[i] = Tile::Floor;
        w.mat[i] = rest;
    } else {
        w.tile[i] = Tile::Open;
        w.mat[i] = AIR;
    }
    w.touch(i);
    w.sky_dirty = true;
}
