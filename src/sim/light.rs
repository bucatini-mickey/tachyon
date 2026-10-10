//! Light. Two fields:
//!
//! * **Sky exposure** (`World::sky`, 0..=15): full straight down every open
//!   column until something stops it, then spread sideways losing one step
//!   per cell, so light leaks through doorways and windows and fades into a
//!   room. It is a property of the terrain, recomputed when terrain changes;
//!   the time of day scales it at render time.
//! * **Dynamic light** ([`LightMap`]): coloured light from torches, fires and
//!   glowing liquids, flood-filled per channel through open cells with
//!   falloff per step. It is computed for the region being looked at.
//!
//! Both flood through the same rules: walls stop light (unless transparent),
//! floors stop it going down, open cells and stairwells let it through.

use crate::color::Rgb;
use crate::geom::{Pos, DIRS8};
use crate::rng::hash3;
use crate::things::Things;
use crate::world::{World, SKY_MAX};
use std::collections::VecDeque;

pub fn compute_sky(w: &mut World, queue: &mut VecDeque<u32>) {
    let layer = w.layer();
    let (ww, hh, dd) = (w.w, w.h, w.d);
    for y in 0..hh {
        for x in 0..ww {
            let mut s = SKY_MAX;
            let mut open = true;
            for z in (0..dd).rev() {
                let i = w.idx(x, y, z);
                if w.blocks_light_idx(i) {
                    s = 0;
                }
                // Rain and wind stop at anything solid, glass included.
                let t = w.tile[i];
                if t.is_wall() {
                    open = false;
                }
                w.sky[i] = s;
                w.open_sky[i] = open;
                if !w.opens_down_idx(i) {
                    s = 0;
                }
                if t.seals_below() {
                    open = false;
                }
            }
        }
    }
    queue.clear();
    for i in 0..w.len() {
        if w.sky[i] > 1 {
            queue.push_back(i as u32);
        }
    }
    let wwu = ww as usize;
    while let Some(i) = queue.pop_front() {
        let i = i as usize;
        let s = w.sky[i];
        if s <= 1 {
            continue;
        }
        let ns = s - 1;
        let z = (i / layer) as i32;
        let r = i % layer;
        let (x, y) = ((r % wwu) as i32, (r / wwu) as i32);
        let relax = |w: &mut World, n: usize, q: &mut VecDeque<u32>| {
            if !w.blocks_light_idx(n) && w.sky[n] < ns {
                w.sky[n] = ns;
                q.push_back(n as u32);
            }
        };
        if x > 0 {
            relax(w, i - 1, queue);
        }
        if x < ww - 1 {
            relax(w, i + 1, queue);
        }
        if y > 0 {
            relax(w, i - wwu, queue);
        }
        if y < hh - 1 {
            relax(w, i + wwu, queue);
        }
        if z > 0 && w.opens_down_idx(i) {
            relax(w, i - layer, queue);
        }
        if z < dd - 1 && w.opens_down_idx(i + layer) {
            relax(w, i + layer, queue);
        }
    }
    w.sky_dirty = false;
}

/// A point of light.
#[derive(Clone, Copy, Debug)]
pub struct LightSource {
    pub pos: Pos,
    /// Brightness per channel; 255 reaches about 18 cells.
    pub color: Rgb,
}

/// Light lost per orthogonal step (and per diagonal step).
pub const FALLOFF: u8 = 14;
const FALLOFF_DIAG: u8 = 20;

/// Coloured light over a box-shaped region of the world.
#[derive(Default, Clone)]
pub struct LightMap {
    pub x0: i32,
    pub y0: i32,
    pub z0: i32,
    pub w: i32,
    pub h: i32,
    pub d: i32,
    pub rgb: Vec<[u8; 3]>,
    queue: VecDeque<u32>,
}

impl LightMap {
    pub fn new() -> LightMap {
        LightMap::default()
    }

    #[inline]
    pub fn get(&self, x: i32, y: i32, z: i32) -> [u8; 3] {
        let (lx, ly, lz) = (x - self.x0, y - self.y0, z - self.z0);
        if lx < 0 || ly < 0 || lz < 0 || lx >= self.w || ly >= self.h || lz >= self.d {
            return [0; 3];
        }
        self.rgb[((lz * self.h + ly) * self.w + lx) as usize]
    }

    /// Flood-fill light from `sources` over the box `[x0,x1) x [y0,y1) x [z0,z1)`.
    #[allow(clippy::too_many_arguments)]
    pub fn compute(&mut self, world: &World, x0: i32, y0: i32, z0: i32, x1: i32, y1: i32, z1: i32, sources: &[LightSource]) {
        let (x0, y0, z0) = (x0.max(0), y0.max(0), z0.max(0));
        let (x1, y1, z1) = (x1.min(world.w), y1.min(world.h), z1.min(world.d));
        self.x0 = x0;
        self.y0 = y0;
        self.z0 = z0;
        self.w = (x1 - x0).max(0);
        self.h = (y1 - y0).max(0);
        self.d = (z1 - z0).max(0);
        let n = (self.w * self.h * self.d) as usize;
        self.rgb.clear();
        self.rgb.resize(n, [0; 3]);
        self.queue.clear();
        if n == 0 {
            return;
        }
        let (lw, lh, ld) = (self.w, self.h, self.d);
        let lidx = |x: i32, y: i32, z: i32| ((z * lh + y) * lw + x) as usize;
        for s in sources {
            let (lx, ly, lz) = (s.pos.x - x0, s.pos.y - y0, s.pos.z - z0);
            if lx < 0 || ly < 0 || lz < 0 || lx >= lw || ly >= lh || lz >= ld {
                continue;
            }
            let li = lidx(lx, ly, lz);
            let c = &mut self.rgb[li];
            c[0] = c[0].max(s.color.0);
            c[1] = c[1].max(s.color.1);
            c[2] = c[2].max(s.color.2);
            self.queue.push_back(li as u32);
        }
        let layer = world.layer();
        let llayer = (lw * lh) as usize;
        while let Some(li) = self.queue.pop_front() {
            let li = li as usize;
            let c = self.rgb[li];
            let lz = (li / llayer) as i32;
            let r = li % llayer;
            let (lx, ly) = ((r % lw as usize) as i32, (r / lw as usize) as i32);
            let (wx, wy, wz) = (lx + x0, ly + y0, lz + z0);
            let wi = world.idx(wx, wy, wz);
            for (k, &(dx, dy)) in DIRS8.iter().enumerate() {
                let f = if k < 4 { FALLOFF } else { FALLOFF_DIAG };
                let nc = [c[0].saturating_sub(f), c[1].saturating_sub(f), c[2].saturating_sub(f)];
                if nc == [0, 0, 0] {
                    continue;
                }
                let (nx, ny) = (lx + dx, ly + dy);
                if nx < 0 || ny < 0 || nx >= lw || ny >= lh {
                    continue;
                }
                let nwi = world.idx(wx + dx, wy + dy, wz);
                if world.blocks_light_idx(nwi) {
                    continue;
                }
                if k >= 4
                    && (world.blocks_light_idx(world.idx(wx + dx, wy, wz))
                        || world.blocks_light_idx(world.idx(wx, wy + dy, wz)))
                {
                    continue;
                }
                Self::relax(&mut self.rgb, &mut self.queue, lidx(nx, ny, lz), nc);
            }
            let nc = [c[0].saturating_sub(FALLOFF), c[1].saturating_sub(FALLOFF), c[2].saturating_sub(FALLOFF)];
            if nc == [0, 0, 0] {
                continue;
            }
            if lz > 0 && world.opens_down_idx(wi) && !world.blocks_light_idx(wi - layer) {
                Self::relax(&mut self.rgb, &mut self.queue, lidx(lx, ly, lz - 1), nc);
            }
            if lz < ld - 1 && world.opens_down_idx(wi + layer) && !world.blocks_light_idx(wi + layer) {
                Self::relax(&mut self.rgb, &mut self.queue, lidx(lx, ly, lz + 1), nc);
            }
        }
    }

    #[inline]
    fn relax(rgb: &mut [[u8; 3]], q: &mut VecDeque<u32>, n: usize, nc: [u8; 3]) {
        let cur = rgb[n];
        if nc[0] > cur[0] || nc[1] > cur[1] || nc[2] > cur[2] {
            rgb[n] = [cur[0].max(nc[0]), cur[1].max(nc[1]), cur[2].max(nc[2])];
            q.push_back(n as u32);
        }
    }
}

#[inline]
fn flicker(x: i32, y: i32, z: i32, tick: u64) -> f32 {
    (hash3(x, y, z, (tick / 2) as u32) & 255) as f32 / 255.0
}

/// Collect every light source in a region: burning cells, glowing liquids
/// and things that carry light.
#[allow(clippy::too_many_arguments)]
pub fn gather_sources(
    world: &World,
    things: &Things,
    x0: i32,
    y0: i32,
    z0: i32,
    x1: i32,
    y1: i32,
    z1: i32,
    tick: u64,
    out: &mut Vec<LightSource>,
) {
    out.clear();
    let (x0, y0, z0) = (x0.max(0), y0.max(0), z0.max(0));
    let (x1, y1, z1) = (x1.min(world.w), y1.min(world.h), z1.min(world.d));
    for z in z0..z1 {
        for y in y0..y1 {
            let row = world.idx(0, y, z);
            for x in x0..x1 {
                let i = row + x as usize;
                if world.fire[i] > 0 {
                    let f = flicker(x, y, z, tick);
                    out.push(LightSource { pos: Pos::new(x, y, z), color: Rgb(255, 150, 60).scale(0.7 + 0.3 * f) });
                } else if world.fluid[i] > 0 {
                    if let Some(g) = world.mats.get(world.fluid_mat[i]).glow {
                        let k = 0.55 + 0.45 * world.fluid[i] as f32 / 64.0;
                        out.push(LightSource { pos: Pos::new(x, y, z), color: g.scale(k) });
                    }
                }
            }
        }
    }
    for (_, t) in things.iter() {
        let k = things.kind_of(t);
        let Some(c) = k.light else { continue };
        if !things.shines(t) {
            continue;
        }
        let p = t.pos();
        if p.x < x0 || p.y < y0 || p.z < z0 || p.x >= x1 || p.y >= y1 || p.z >= z1 {
            continue;
        }
        let c = if k.flicker { c.scale(0.85 + 0.15 * flicker(p.x, p.y, p.z, tick)) } else { c };
        out.push(LightSource { pos: p, color: c });
    }
}
