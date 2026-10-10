//! Gases: smoke, steam, miasma. Each cell holds 0..=255 of one gas.
//!
//! Per tick over awake chunks: gas decays (faster under open sky), part of it
//! rises into the cell above if nothing seals it, then it diffuses sideways
//! with the same conserving fifth-of-the-difference rule as liquids. Under
//! open sky the wind also carries part of it downwind each tick.

use crate::geom::DIRS4;
use crate::material::AIR;
use crate::rng::Rng;
use crate::world::World;

/// Round `x` up or down at random in proportion to its fraction, so that
/// small rates still act on average.
#[inline]
fn stochastic(x: f32, rng: &mut Rng) -> u8 {
    let f = x.floor();
    let extra = (rng.f32() < x - f) as u8;
    (f.min(255.0) as u8).saturating_add(extra)
}

pub fn step(w: &mut World, chunks: &[usize], rng: &mut Rng, delta: &mut [i16], touched: &mut Vec<u32>) {
    let layer = w.layer();
    let (ww, hh, top) = (w.w, w.h, w.d - 1);

    // 1. Decay and rise, top level first so a puff climbs one level per tick.
    for &c in chunks.iter().rev() {
        let (x0, y0, x1, y1, z) = w.wake.bounds(c);
        for y in y0..y1 {
            let row = w.idx(0, y, z);
            for x in x0..x1 {
                let i = row + x as usize;
                let g = w.gas[i];
                if g == 0 {
                    continue;
                }
                let gm = w.gas_mat[i];
                let (rise, decay, _) = w.mats.get(gm).gas_props();
                let rate = if w.open_sky[i] { decay * 4.0 } else { decay };
                let mut lose = stochastic(g as f32 * rate, rng);
                if z == top {
                    lose = lose.saturating_add(g / 3 + 1);
                }
                let mut g = g.saturating_sub(lose);
                w.gas[i] = g;
                w.wake.touch(i);
                if g == 0 {
                    w.gas_mat[i] = AIR;
                    continue;
                }
                if z < top {
                    let a = i + layer;
                    if !w.tile[a].seals_below() && (w.gas[a] == 0 || w.gas_mat[a] == gm) {
                        let mv = stochastic(g as f32 * rise, rng).min(255 - w.gas[a]).min(g);
                        if mv > 0 {
                            g -= mv;
                            w.gas[i] = g;
                            if g == 0 {
                                w.gas_mat[i] = AIR;
                            }
                            w.gas[a] += mv;
                            w.gas_mat[a] = gm;
                            w.wake.touch(a);
                        }
                    }
                }
            }
        }
    }

    // 2. Drift with the wind and diffuse sideways, from a snapshot.
    let (wx, wy) = w.wind;
    let speed = (wx * wx + wy * wy).sqrt();
    let drift = (speed * 0.5).min(0.5);
    let along_x = if speed > 0.0 { wx.abs() / (wx.abs() + wy.abs()) } else { 0.0 };
    touched.clear();
    for &c in chunks {
        let (x0, y0, x1, y1, z) = w.wake.bounds(c);
        let _ = z;
        for y in y0..y1 {
            let row = w.idx(0, y, z);
            for x in x0..x1 {
                let i = row + x as usize;
                let g = w.gas[i];
                if g == 0 {
                    continue;
                }
                let gm = w.gas_mat[i];
                let mut avail = (g as u32 * 4).div_ceil(5);
                if drift > 0.01 && w.open_sky[i] {
                    let (dx, dy) = if rng.f32() < along_x { (wx.signum() as i32, 0) } else { (0, wy.signum() as i32) };
                    let (nx, ny) = (x + dx, y + dy);
                    if nx >= 0 && ny >= 0 && nx < ww && ny < hh {
                        let n = (i as isize + dx as isize + dy as isize * ww as isize) as usize;
                        if !w.tile[n].is_wall() && (w.gas[n] == 0 || w.gas_mat[n] == gm) {
                            let mv = (stochastic(g as f32 * drift, rng) as u32).min(avail);
                            if mv > 0 {
                                avail -= mv;
                                delta[i] -= mv as i16;
                                delta[n] += mv as i16;
                                if w.gas[n] == 0 {
                                    w.gas_mat[n] = gm;
                                }
                                touched.push(i as u32);
                                touched.push(n as u32);
                            }
                        }
                    }
                }
                if g < 2 {
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
                    let f = w.gas[n];
                    if f > 0 && w.gas_mat[n] != gm {
                        continue;
                    }
                    if g > f {
                        let flow = (((g - f) as u32 + rng.below(5)) / 5).min(avail);
                        if flow > 0 {
                            avail -= flow;
                            delta[i] -= flow as i16;
                            delta[n] += flow as i16;
                            if f == 0 {
                                w.gas_mat[n] = gm;
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
        let v = (w.gas[i] as i16 + d).clamp(0, 255) as u8;
        w.gas[i] = v;
        if v == 0 {
            w.gas_mat[i] = AIR;
        }
        w.wake.touch(i);
    }
}
