//! The atlas: an authored world map.
//!
//! A coarse grid where each cell stands for a `REGION` x `REGION` metre patch
//! of the world. Each cell has an elevation (metres above sea level; negative
//! is under the sea), a rock type and a biome. On top of the grid sit rivers
//! (paths from a source, drawn cell to cell) and features (markers: a town,
//! a cave, a blueprint to stamp).
//!
//! The atlas is what an author paints; [`crate::worldgen`] turns any part of
//! it into a full 1 m world on demand. It saves as plain text, so it can be
//! read, diffed and hand-edited.

/// Metres (local cells) per atlas cell edge.
pub const REGION: i32 = 32;

/// `Feature::z` for "on the ground, wherever that is".
pub const GROUND: i32 = i32::MIN;

#[derive(Clone, Debug, PartialEq)]
pub struct River {
    /// Channel width in metres.
    pub width: u8,
    /// Atlas cells from the source (first) to the mouth (last).
    pub points: Vec<(i32, i32)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Feature {
    /// Position in world metres (atlas cell * REGION + offset).
    pub x: i32,
    pub y: i32,
    /// Metres above sea level of its lowest level, or [`GROUND`].
    pub z: i32,
    /// What it is: "town", "cave", ... or "bp:<name>" for a blueprint to
    /// stamp. One word (spaces are saved as dashes).
    pub kind: String,
    pub label: String,
}

impl Feature {
    pub fn cell(&self) -> (i32, i32) {
        (self.x.div_euclid(REGION), self.y.div_euclid(REGION))
    }
    /// The blueprint this feature stamps, if it is one.
    pub fn blueprint(&self) -> Option<&str> {
        self.kind.strip_prefix("bp:")
    }
}

/// A finer height layer over the whole map: `scale` x `scale` relief cells per
/// atlas cell (4 gives 8 m cells). Made by erosion ([`crate::hydro::erode`]);
/// when an atlas has one, [`Atlas::elevation_at`] (and so worldgen) reads it
/// instead of the coarse `elevation`.
#[derive(Clone, Debug, PartialEq)]
pub struct Relief {
    pub scale: i32,
    /// Size in relief cells (atlas size x `scale`).
    pub w: i32,
    pub h: i32,
    /// Metres above sea level, row by row.
    pub height: Vec<f32>,
}

impl Relief {
    pub fn new(scale: i32, w: i32, h: i32, fill: f32) -> Relief {
        Relief { scale, w, h, height: vec![fill; (w * h) as usize] }
    }
    /// Metres per relief cell edge.
    pub fn cell_m(&self) -> f32 {
        REGION as f32 / self.scale as f32
    }
    #[inline]
    pub fn idx(&self, x: i32, y: i32) -> usize {
        (y * self.w + x) as usize
    }
    /// Height of a cell, clamped to the map edge.
    pub fn at(&self, x: i32, y: i32) -> f32 {
        self.height[self.idx(x.clamp(0, self.w - 1), y.clamp(0, self.h - 1))]
    }
    /// Smooth height at a point given in *atlas* units, like `Atlas::elevation_at`.
    pub fn sample(&self, x: f32, y: f32) -> f32 {
        let s = self.scale as f32;
        catmull_rom(|i, j| self.at(i, j), x * s - 0.5, y * s - 0.5)
    }
}

/// Cubic (Catmull-Rom) interpolation of a grid at (gx, gy), where integer
/// coordinates are cell centres. Overshoot is clamped to the four nearest cells.
fn catmull_rom(get: impl Fn(i32, i32) -> f32, gx: f32, gy: f32) -> f32 {
    let (ix, iy) = (gx.floor() as i32, gy.floor() as i32);
    let (fx, fy) = (gx - ix as f32, gy - iy as f32);
    let cubic = |p0: f32, p1: f32, p2: f32, p3: f32, t: f32| {
        p1 + 0.5 * t * (p2 - p0 + t * (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3 + t * (3.0 * (p1 - p2) + p3 - p0)))
    };
    let row = |j: i32| cubic(get(ix - 1, iy + j), get(ix, iy + j), get(ix + 1, iy + j), get(ix + 2, iy + j), fx);
    let v = cubic(row(-1), row(0), row(1), row(2), fy);
    let c = [get(ix, iy), get(ix + 1, iy), get(ix, iy + 1), get(ix + 1, iy + 1)];
    v.clamp(c.iter().copied().fold(f32::MAX, f32::min), c.iter().copied().fold(f32::MIN, f32::max))
}

#[derive(Clone, Debug, PartialEq)]
pub struct Atlas {
    pub name: String,
    pub w: i32,
    pub h: i32,
    /// Metres above sea level.
    pub elevation: Vec<i16>,
    /// Index into `rocks`.
    pub rock: Vec<u8>,
    /// Index into `biomes`.
    pub biome: Vec<u8>,
    /// Rock types by material name.
    pub rocks: Vec<String>,
    /// Biome names (the game says what each means; see `worldgen`).
    pub biomes: Vec<String>,
    pub rivers: Vec<River>,
    pub features: Vec<Feature>,
    /// Finer heights from erosion, if this atlas has been eroded.
    pub relief: Option<Relief>,
}

impl Atlas {
    /// All sea, `depth` metres down.
    pub fn new(name: &str, w: i32, h: i32, rocks: &[&str], biomes: &[&str], depth: i16) -> Atlas {
        let n = (w * h) as usize;
        Atlas {
            name: name.into(),
            w,
            h,
            elevation: vec![-depth; n],
            rock: vec![0; n],
            biome: vec![0; n],
            rocks: rocks.iter().map(|s| s.to_string()).collect(),
            biomes: biomes.iter().map(|s| s.to_string()).collect(),
            rivers: Vec::new(),
            features: Vec::new(),
            relief: None,
        }
    }

    #[inline]
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.w && y < self.h
    }

    #[inline]
    pub fn idx(&self, x: i32, y: i32) -> usize {
        (y * self.w + x) as usize
    }

    /// Elevation of a cell, clamped to the map edge.
    pub fn elev(&self, x: i32, y: i32) -> f32 {
        let (x, y) = (x.clamp(0, self.w - 1), y.clamp(0, self.h - 1));
        self.elevation[self.idx(x, y)] as f32
    }

    /// Smooth elevation at a point in atlas units, where (0.5, 0.5) is the
    /// centre of cell (0, 0). Cubic (Catmull-Rom) interpolation, so slopes
    /// are smooth rather than faceted. An eroded atlas answers from its finer
    /// `relief` layer instead.
    pub fn elevation_at(&self, x: f32, y: f32) -> f32 {
        match &self.relief {
            Some(r) => r.sample(x, y),
            None => catmull_rom(|i, j| self.elev(i, j), x - 0.5, y - 0.5),
        }
    }

    /// Cells under a round brush of radius `r` (cells) at (cx, cy), with a
    /// weight falling from 1 at the centre to 0 at the rim.
    pub fn brush(&self, cx: i32, cy: i32, r: f32) -> Vec<(usize, f32)> {
        let ri = r.ceil() as i32;
        let mut out = Vec::new();
        for y in cy - ri..=cy + ri {
            for x in cx - ri..=cx + ri {
                if !self.contains(x, y) {
                    continue;
                }
                let d = (((x - cx) * (x - cx) + (y - cy) * (y - cy)) as f32).sqrt();
                if d <= r {
                    let t = d / (r + 0.5);
                    out.push((self.idx(x, y), 1.0 - t * t));
                }
            }
        }
        out
    }

    /// Features whose position lies in atlas cell (x, y).
    pub fn features_in(&self, x: i32, y: i32) -> impl Iterator<Item = (usize, &Feature)> {
        self.features.iter().enumerate().filter(move |(_, f)| f.cell() == (x, y))
    }

    /// Every atlas cell a river passes through, in order.
    pub fn river_cells(r: &River) -> Vec<(i32, i32)> {
        let mut out: Vec<(i32, i32)> = Vec::new();
        for pair in r.points.windows(2) {
            let ((x0, y0), (x1, y1)) = (pair[0], pair[1]);
            let n = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
            for s in 0..=n {
                let t = s as f32 / n as f32;
                let c = ((x0 as f32 + (x1 - x0) as f32 * t).round() as i32, (y0 as f32 + (y1 - y0) as f32 * t).round() as i32);
                if out.last() != Some(&c) {
                    out.push(c);
                }
            }
        }
        if r.points.len() == 1 {
            out.push(r.points[0]);
        }
        out
    }

    // ---- text form ------------------------------------------------------

    pub fn to_text(&self) -> String {
        let mut s = String::from("tachyon-atlas 1\n");
        s += &format!("name {}\nsize {} {}\nregion {}\n", self.name, self.w, self.h, REGION);
        s += &format!("rocks {}\n", self.rocks.iter().map(|r| r.replace(' ', "_")).collect::<Vec<_>>().join(" "));
        s += &format!("biomes {}\n", self.biomes.iter().map(|r| r.replace(' ', "_")).collect::<Vec<_>>().join(" "));
        s += "elevation\n";
        for y in 0..self.h {
            let row: Vec<String> = (0..self.w).map(|x| self.elevation[self.idx(x, y)].to_string()).collect();
            s += &row.join(" ");
            s.push('\n');
        }
        for (title, layer) in [("rock", &self.rock), ("biome", &self.biome)] {
            s += title;
            s.push('\n');
            for y in 0..self.h {
                for x in 0..self.w {
                    s.push(char::from_digit(layer[self.idx(x, y)] as u32, 36).unwrap_or('0'));
                }
                s.push('\n');
            }
        }
        // Relief heights print in Rust's shortest exact form ("12.3", "-4").
        if let Some(r) = &self.relief {
            s += &format!("relief {}\n", r.scale);
            for y in 0..r.h {
                let row: Vec<String> = (0..r.w).map(|x| r.height[r.idx(x, y)].to_string()).collect();
                s += &row.join(" ");
                s.push('\n');
            }
        }
        for r in &self.rivers {
            let pts: Vec<String> = r.points.iter().map(|(x, y)| format!("{x},{y}")).collect();
            s += &format!("river {} {}\n", r.width, pts.join(" "));
        }
        for f in &self.features {
            let z = if f.z == GROUND { "-".to_string() } else { f.z.to_string() };
            s += &format!("feature {} {} {} {} {}\n", f.x, f.y, z, f.kind.replace(' ', "-"), f.label);
        }
        s += "end\n";
        s
    }

    pub fn from_text(text: &str) -> Result<Atlas, String> {
        let mut lines = text.lines().map(str::trim_end).filter(|l| !l.trim().is_empty() && !l.starts_with('#'));
        let first = lines.next().ok_or("empty file")?;
        if !first.starts_with("tachyon-atlas") {
            return Err("not an atlas file (no 'tachyon-atlas' header)".into());
        }
        let mut a = Atlas::new("untitled", 1, 1, &[], &[], 0);
        let num = |s: &str| s.parse::<i32>().map_err(|e| format!("bad number '{s}': {e}"));
        while let Some(line) = lines.next() {
            let (word, rest) = line.split_once(' ').unwrap_or((line, ""));
            match word {
                "name" => a.name = rest.to_string(),
                "size" => {
                    let v: Vec<&str> = rest.split_whitespace().collect();
                    if v.len() != 2 {
                        return Err("size needs width and height".into());
                    }
                    let (w, h) = (num(v[0])?, num(v[1])?);
                    a = Atlas { name: a.name, rocks: a.rocks, biomes: a.biomes, ..Atlas::new("", w, h, &[], &[], 0) };
                }
                "region" => {
                    if num(rest.trim())? != REGION {
                        return Err(format!("atlas uses region size {rest}, this build uses {REGION}"));
                    }
                }
                "rocks" => a.rocks = rest.split_whitespace().map(|s| s.replace('_', " ")).collect(),
                "biomes" => a.biomes = rest.split_whitespace().map(|s| s.replace('_', " ")).collect(),
                "elevation" => {
                    for y in 0..a.h {
                        let row = lines.next().ok_or("elevation ends early")?;
                        let v: Vec<&str> = row.split_whitespace().collect();
                        if v.len() != a.w as usize {
                            return Err(format!("elevation row {y} has {} values, expected {}", v.len(), a.w));
                        }
                        for (x, s) in v.iter().enumerate() {
                            let i = a.idx(x as i32, y);
                            a.elevation[i] = num(s)? as i16;
                        }
                    }
                }
                "rock" | "biome" => {
                    for y in 0..a.h {
                        let row = lines.next().ok_or("layer ends early")?;
                        for (x, c) in row.chars().take(a.w as usize).enumerate() {
                            let v = c.to_digit(36).unwrap_or(0) as u8;
                            let i = a.idx(x as i32, y);
                            if word == "rock" {
                                a.rock[i] = v;
                            } else {
                                a.biome[i] = v;
                            }
                        }
                    }
                }
                "relief" => {
                    let scale = num(rest.trim())?;
                    if !(1..=REGION).contains(&scale) {
                        return Err(format!("relief scale {scale} out of range"));
                    }
                    let mut r = Relief::new(scale, a.w * scale, a.h * scale, 0.0);
                    for y in 0..r.h {
                        let row = lines.next().ok_or("relief ends early")?;
                        let v: Vec<&str> = row.split_whitespace().collect();
                        if v.len() != r.w as usize {
                            return Err(format!("relief row {y} has {} values, expected {}", v.len(), r.w));
                        }
                        for (x, s) in v.iter().enumerate() {
                            let i = r.idx(x as i32, y);
                            r.height[i] = s.parse::<f32>().map_err(|e| format!("bad height '{s}': {e}"))?;
                        }
                    }
                    a.relief = Some(r);
                }
                "river" => {
                    let mut it = rest.split_whitespace();
                    let width = num(it.next().ok_or("river needs a width")?)?.clamp(1, 255) as u8;
                    let mut points = Vec::new();
                    for p in it {
                        let (x, y) = p.split_once(',').ok_or(format!("bad river point '{p}'"))?;
                        points.push((num(x)?, num(y)?));
                    }
                    a.rivers.push(River { width, points });
                }
                "feature" => {
                    let mut it = rest.splitn(5, ' ');
                    let x = num(it.next().ok_or("feature needs x")?)?;
                    let y = num(it.next().ok_or("feature needs y")?)?;
                    let zs = it.next().ok_or("feature needs z")?;
                    let z = if zs == "-" { GROUND } else { num(zs)? };
                    let kind = it.next().ok_or("feature needs a kind")?.to_string();
                    let label = it.next().unwrap_or("").to_string();
                    a.features.push(Feature { x, y, z, kind, label });
                }
                "end" => break,
                other => return Err(format!("unknown line '{other}'")),
            }
        }
        Ok(a)
    }
}
