//! Turning part of an [`Atlas`] into a full 1 m world.
//!
//! - Height: the atlas elevation, smoothly interpolated, plus a little noise
//!   whose size the biome sets (flat grassland, rough mountains).
//! - Columns: the biome's surface (grass, sand, snow; or bare rock), its
//!   subsoil, and the region's rock below. Biome and rock boundaries are
//!   jittered with noise so they don't follow the atlas grid. Steep slopes
//!   show bare rock. Ramps join every one-level step.
//! - The sea fills everything below sea level, and is held there by springs
//!   and drains along the edge of the generated area.
//! - Rivers are carved along their path with a bed that only ever goes
//!   downhill, filled with water, with a spring at the source and a drain
//!   where the river ends inland or leaves the area.
//! - Trees and marsh puddles from the biome.
//! - Features are returned with their local positions, for the game to stamp
//!   (blueprints) or act on.
//!
//! The same atlas and seed always give the same world.

use crate::atlas::{Atlas, Feature, GROUND, REGION};
use crate::geom::Pos;
use crate::material::{MatId, Materials};
use crate::noise::{fbm2, value2};
use crate::rng::hash3;
use crate::sim::Source;
use crate::tile::Tile;
use crate::world::{World, FULL};

/// What a biome means on the ground.
#[derive(Clone, Debug)]
pub struct BiomeDef {
    pub name: String,
    /// Floor on top; `None` shows the bare rock beneath.
    pub surface: Option<MatId>,
    /// Material between the surface and the rock.
    pub subsoil: MatId,
    /// Levels of subsoil above the rock.
    pub soil_depth: i32,
    /// Chance per cell of a tree.
    pub trees: f32,
    /// Height noise, in metres.
    pub roughness: f32,
    /// Share of the ground under shallow standing water (marsh).
    pub puddles: f32,
}

#[derive(Clone, Debug)]
pub struct GenSettings {
    /// The materials every `MatId` here refers to; the world gets a copy.
    /// Rock types are looked up in it by the atlas's rock names.
    pub materials: Materials,
    pub biomes: Vec<BiomeDef>,
    pub water: MatId,
    /// Sea and river beds.
    pub bed: MatId,
    pub trunk: MatId,
    pub leaves: MatId,
    /// Empty levels kept above the highest ground (trees, buildings).
    pub headroom: i32,
    pub seed: u32,
}

pub struct Generated {
    pub world: World,
    /// World metres of local cell (0, 0).
    pub origin: (i32, i32),
    /// Local level of the sea surface: ground at elevation 0 has its floor
    /// here, and the sea is full up to the level below.
    pub sea_z: i32,
    /// Springs and drains for the scene's sim.
    pub sources: Vec<Source>,
    /// Features in the area and where they land locally.
    pub features: Vec<(Feature, Pos)>,
}

impl Generated {
    /// Local position of a point given in world metres and metres above sea.
    pub fn local(&self, x: i32, y: i32, z: i32) -> Pos {
        Pos::new(x - self.origin.0, y - self.origin.1, self.sea_z + z)
    }
    /// The other way: world metres, and metres above sea level.
    pub fn global(&self, p: Pos) -> (i32, i32, i32) {
        (p.x + self.origin.0, p.y + self.origin.1, p.z - self.sea_z)
    }
}

/// Generate atlas cells `ax0..ax0+aw`, `ay0..ay0+ah`.
pub fn generate(atlas: &Atlas, set: &GenSettings, ax0: i32, ay0: i32, aw: i32, ah: i32) -> Generated {
    let (w, h) = (aw * REGION, ah * REGION);
    let (ox, oy) = (ax0 * REGION, ay0 * REGION);
    let s = set.seed;
    // Atlas palette index -> biome definition / rock material, by name.
    let biome_of: Vec<&BiomeDef> = atlas
        .biomes
        .iter()
        .map(|n| set.biomes.iter().find(|b| &b.name == n).unwrap_or(&set.biomes[0]))
        .collect();

    // Which atlas cell a local cell takes its biome and rock from: the
    // nearest, after jittering the position so borders wander.
    let cell_at = |x: i32, y: i32, salt: u32| -> (i32, i32) {
        let (gx, gy) = ((ox + x) as f32, (oy + y) as f32);
        let jx = (fbm2(s ^ salt, gx / 23.0, gy / 23.0, 3) - 0.5) * REGION as f32 * 0.6;
        let jy = (fbm2(s ^ salt ^ 0x99, gx / 23.0, gy / 23.0, 3) - 0.5) * REGION as f32 * 0.6;
        let cx = ((gx + jx) / REGION as f32).floor() as i32;
        let cy = ((gy + jy) / REGION as f32).floor() as i32;
        (cx.clamp(0, atlas.w - 1), cy.clamp(0, atlas.h - 1))
    };

    // 1. Heights (metres above sea level) and per-cell biome and rock.
    let n = (w * h) as usize;
    let mut height = vec![0i32; n];
    let mut biome = vec![0usize; n];
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            let (bx, by) = cell_at(x, y, 0xB10E);
            let b = atlas.biome[atlas.idx(bx, by)] as usize;
            biome[i] = b;
            let (fx, fy) = ((ox + x) as f32 / REGION as f32, (oy + y) as f32 / REGION as f32);
            let e = atlas.elevation_at(fx, fy);
            let rough = biome_of.get(b).map_or(1.0, |d| d.roughness);
            let noise = (fbm2(s ^ 0x4E1, (ox + x) as f32 / 14.0, (oy + y) as f32 / 14.0, 3) - 0.5) * 2.0 * rough;
            // Keep the coast where the author drew it: little noise near sea level.
            let near_sea = (e.abs() / 3.0).min(1.0);
            height[i] = (e + noise * near_sea).round() as i32;
        }
    }
    // Rock types: atlas palette index -> material (unknown names fall back to granite).
    let rock_ids: Vec<MatId> = atlas
        .rocks
        .iter()
        .map(|name| set.materials.find(name).or_else(|| set.materials.find("granite")).unwrap_or(set.bed))
        .collect();
    let rock_mat = |x: i32, y: i32| -> MatId {
        let (rx, ry) = cell_at(x, y, 0x50C4);
        rock_ids.get(atlas.rock[atlas.idx(rx, ry)] as usize).copied().unwrap_or(set.bed)
    };

    // 2. Rivers: carve beds that only go downhill along each path.
    // `bed[i]` is the river bed's level (metres above sea) where a river runs.
    let mut bed: Vec<Option<i32>> = vec![None; n];
    let mut sources: Vec<(i32, i32, i32, f32)> = Vec::new();
    let mut river_ends: Vec<(i32, i32, i32, f32)> = Vec::new();
    for (ri, r) in atlas.rivers.iter().enumerate() {
        if r.points.is_empty() {
            continue;
        }
        // Local polyline through the cells, with a gentle meander.
        let pts: Vec<(f32, f32)> = r
            .points
            .iter()
            .enumerate()
            .map(|(k, &(cx, cy))| {
                let ends = k == 0 || k == r.points.len() - 1;
                let j = if ends { 0.0 } else { REGION as f32 * 0.22 };
                let jx = (value2(s ^ ri as u32, cx as f32 * 1.7, cy as f32 * 1.3) - 0.5) * 2.0 * j;
                let jy = (value2(s ^ ri as u32 ^ 0x77, cx as f32 * 1.3, cy as f32 * 1.7) - 0.5) * 2.0 * j;
                ((cx * REGION + REGION / 2 - ox) as f32 + jx, (cy * REGION + REGION / 2 - oy) as f32 + jy)
            })
            .collect();
        let half = r.width as f32 / 2.0;
        let deep = 1 + r.width as i32 / 3;
        let mut level = i32::MAX;
        let (mut first_in, mut last_in): (Option<(i32, i32, i32)>, Option<(i32, i32, i32)>) = (None, None);
        let mut walk = |px: f32, py: f32| {
            let (cx, cy) = (px.round() as i32, py.round() as i32);
            if cx < 0 || cy < 0 || cx >= w || cy >= h {
                return;
            }
            // Cut below the lowest ground on either bank, so a river crossing
            // a slope does not spill over its downhill side.
            let rr = half.ceil() as i32;
            let mut ground = i32::MAX;
            for dy in -rr - 1..=rr + 1 {
                for dx in -rr - 1..=rr + 1 {
                    let (x, y) = ((cx + dx).clamp(0, w - 1), (cy + dy).clamp(0, h - 1));
                    ground = ground.min(height[(y * w + x) as usize]);
                }
            }
            level = level.min(ground - deep);
            for dy in -rr..=rr {
                for dx in -rr..=rr {
                    let (x, y) = (cx + dx, cy + dy);
                    if x < 0 || y < 0 || x >= w || y >= h || ((dx * dx + dy * dy) as f32).sqrt() > half + 0.3 {
                        continue;
                    }
                    let i = (y * w + x) as usize;
                    bed[i] = Some(bed[i].map_or(level, |b| b.min(level)));
                }
            }
            if first_in.is_none() {
                first_in = Some((cx, cy, level));
            }
            last_in = Some((cx, cy, level));
        };
        if pts.len() == 1 {
            walk(pts[0].0, pts[0].1);
        }
        for seg in pts.windows(2) {
            let ((x0, y0), (x1, y1)) = (seg[0], seg[1]);
            let len = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
            let steps = (len / 0.5).ceil().max(1.0) as i32;
            for k in 0..=steps {
                let t = k as f32 / steps as f32;
                walk(x0 + (x1 - x0) * t, y0 + (y1 - y0) * t);
            }
        }
        if let Some((x, y, b)) = first_in {
            sources.push((x, y, b, half));
        }
        if let Some((x, y, b)) = last_in {
            river_ends.push((x, y, b, half));
        }
    }

    // 3. Levels: fit everything between z = 1 and the headroom.
    let lowest = height.iter().copied().chain(bed.iter().flatten().copied()).min().unwrap_or(0).min(-1);
    let highest = height.iter().copied().max().unwrap_or(0).max(1);
    let sea_z = 2 - lowest;
    let d = (sea_z + highest + set.headroom).max(sea_z + 4);
    let mut world = World::new(w, h, d, set.materials.clone());

    // 4. Columns.
    let col_h = |x: i32, y: i32| -> i32 {
        let (x, y) = (x.clamp(0, w - 1), y.clamp(0, h - 1));
        let i = (y * w + x) as usize;
        bed[i].unwrap_or(height[i])
    };
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            let def = biome_of[biome[i].min(biome_of.len() - 1)];
            let g = col_h(x, y);
            let floor_z = sea_z + g;
            let rmat = rock_mat(x, y);
            let steep = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|&(dx, dy)| (col_h(x + dx, y + dy) - g).abs() >= 3);
            let under_water = g < 0 || bed[i].is_some();
            let soil = if steep || def.surface.is_none() { 0 } else { def.soil_depth };
            for z in 0..floor_z.min(d) {
                let m = if z >= floor_z - soil { def.subsoil } else { rmat };
                world.set_tile(Pos::new(x, y, z), Tile::Wall, m);
            }
            if floor_z < d {
                let surf = if under_water {
                    set.bed
                } else if steep {
                    rmat
                } else {
                    def.surface.unwrap_or(rmat)
                };
                world.set_tile(Pos::new(x, y, floor_z), Tile::Floor, surf);
            }
            // Sea up to sea level; river water one level deep in its bed.
            if g < 0 {
                for z in floor_z..sea_z {
                    world.set_fluid(Pos::new(x, y, z), set.water, FULL);
                }
            } else if bed[i].is_some() {
                world.set_fluid(Pos::new(x, y, floor_z), set.water, FULL);
            } else if def.puddles > 0.0
                && fbm2(s ^ 0xFAD, (ox + x) as f32 / 9.0, (oy + y) as f32 / 9.0, 3) > 1.0 - def.puddles
                && [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().all(|&(dx, dy)| col_h(x + dx, y + dy) >= g)
            {
                // Standing water only where the ground is level or a hollow,
                // or it would just run off downhill.
                world.set_fluid(Pos::new(x, y, floor_z), set.water, 24);
            }
        }
    }
    // Ramps where dry ground steps up one level.
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let i = (y * w + x) as usize;
            let g = col_h(x, y);
            if bed[i].is_some() || g < 0 {
                continue;
            }
            if [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|&(dx, dy)| col_h(x + dx, y + dy) == g + 1) {
                let p = Pos::new(x, y, sea_z + g);
                if let Some(j) = world.index(p) {
                    let m = world.mat[j];
                    world.set_tile(p, Tile::Ramp, m);
                }
            }
        }
    }

    // 5. Trees.
    let mut taken = vec![false; n];
    for y in 2..h - 2 {
        for x in 2..w - 2 {
            let i = (y * w + x) as usize;
            let def = biome_of[biome[i].min(biome_of.len() - 1)];
            if def.trees <= 0.0 || bed[i].is_some() || height[i] < 1 {
                continue;
            }
            let roll = (hash3(ox + x, oy + y, 0, s ^ 0x7EE) & 0xFFFF) as f32 / 65535.0;
            if roll > def.trees {
                continue;
            }
            if (-2..=2).any(|dy| (-2..=2).any(|dx| taken[((y + dy) * w + x + dx) as usize])) {
                continue;
            }
            let g = sea_z + height[i];
            let p = Pos::new(x, y, g);
            if g + 3 >= d || world.tile(p) != Tile::Floor || world.fluid[world.index(p).unwrap()] > 0 {
                continue;
            }
            taken[i] = true;
            world.set_tile(p, Tile::Wall, set.trunk);
            for (dx, dy) in [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)] {
                let q = Pos::new(x + dx, y + dy, g + 1);
                if world.tile(q) == Tile::Open {
                    world.set_tile(q, Tile::Floor, set.leaves);
                }
            }
            world.set_tile(Pos::new(x, y, g + 2), Tile::Floor, set.leaves);
        }
    }

    // 6. Springs and drains. Rivers: a spring at the first bed cell in the
    // area, a drain at the last unless the river runs into the sea.
    let mut out_sources = Vec::new();
    // Springs across the whole width of the source, so it feeds a river
    // rather than a trickle.
    for &(x, y, _, half) in &sources {
        let r = half.ceil() as i32;
        for dy in -r..=r {
            for dx in -r..=r {
                let (qx, qy) = (x + dx, y + dy);
                if qx >= 0 && qy >= 0 && qx < w && qy < h {
                    if let Some(b) = bed[(qy * w + qx) as usize] {
                        out_sources.push(Source { pos: Pos::new(qx, qy, sea_z + b), mat: set.water, rate: 16 });
                    }
                }
            }
        }
    }
    for &(x, y, b, half) in &river_ends {
        let at_sea = height[(y * w + x) as usize] < 0 || b < 0;
        if at_sea {
            continue;
        }
        // Drain the whole width of the channel where it ends or leaves the area.
        let r = half.ceil() as i32;
        for dy in -r..=r {
            for dx in -r..=r {
                let (qx, qy) = (x + dx, y + dy);
                if qx >= 0 && qy >= 0 && qx < w && qy < h && bed[(qy * w + qx) as usize].is_some() {
                    let z = sea_z + bed[(qy * w + qx) as usize].unwrap();
                    out_sources.push(Source { pos: Pos::new(qx, qy, z + 1), mat: set.water, rate: -24 });
                }
            }
        }
    }
    // The sea along the edge: kept full below sea level, overflow drained.
    for k in 0..(2 * (w + h)) {
        let (x, y) = if k < w {
            (k, 0)
        } else if k < 2 * w {
            (k - w, h - 1)
        } else if k < 2 * w + h {
            (0, k - 2 * w)
        } else {
            (w - 1, k - 2 * w - h)
        };
        if k % 3 != 0 || height[(y * w + x) as usize] >= 0 {
            continue;
        }
        out_sources.push(Source { pos: Pos::new(x, y, sea_z - 1), mat: set.water, rate: 16 });
        out_sources.push(Source { pos: Pos::new(x, y, sea_z), mat: set.water, rate: -16 });
    }

    // 7. Features in the area.
    let mut features = Vec::new();
    for f in &atlas.features {
        let (lx, ly) = (f.x - ox, f.y - oy);
        if lx < 0 || ly < 0 || lx >= w || ly >= h {
            continue;
        }
        let z = if f.z == GROUND { sea_z + col_h(lx, ly).max(0) } else { sea_z + f.z };
        features.push((f.clone(), Pos::new(lx, ly, z)));
    }

    crate::sim::light::compute_sky(&mut world, &mut std::collections::VecDeque::new());
    world.wake.wake_all();
    for i in 0..world.len() {
        if !world.open_sky[i] && world.sky[i] == 0 {
            world.temp[i] = world.ground_temp;
        }
    }
    Generated { world, origin: (ox, oy), sea_z, sources: out_sources, features }
}
