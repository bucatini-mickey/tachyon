//! Blueprints: structures you build once and stamp anywhere.
//!
//! A blueprint is a box of cells over one or more levels. Each cell is
//! either "keep" (leave the world as it is) or a tile shape, a material and
//! the things in it (furniture, items). Materials and kinds are stored by
//! name, so a blueprint works in any game that has them.
//!
//! The text form is one character per cell, a level at a time, with a key
//! saying what each character means:
//!
//! ```text
//! tachyon-blueprint 1
//! name chapel
//! size 7 5 1
//! key # wall stone block
//! key . floor stone block
//! key d floor stone block | door
//! level 0
//! #######
//! #.....#
//! #.....#
//! #.....#
//! ###d###
//! end
//! ```
//!
//! A space means "keep". Level 0 is the lowest.

use crate::geom::Pos;
use crate::material::AIR;
use crate::things::{Layer, Things};
use crate::tile::Tile;
use crate::world::World;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BpCell {
    pub tile: Tile,
    /// Material name ("" for open air).
    pub mat: String,
    /// Kind names of the things here, lowest layer first.
    pub things: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Blueprint {
    pub name: String,
    pub w: i32,
    pub h: i32,
    pub d: i32,
    /// `None` = keep whatever is there.
    pub cells: Vec<Option<BpCell>>,
}

/// What stamping did, so a game can warn about missing names.
#[derive(Clone, Debug, Default)]
pub struct StampReport {
    pub cells: usize,
    pub unknown_materials: Vec<String>,
    pub unknown_kinds: Vec<String>,
}

fn tile_word(t: Tile) -> &'static str {
    match t {
        Tile::Open => "open",
        Tile::Floor => "floor",
        Tile::Wall => "wall",
        Tile::Ramp => "ramp",
        Tile::StairUp => "stair-up",
        Tile::StairDown => "stair-down",
        Tile::StairUpDown => "stair-updown",
    }
}

fn parse_tile(s: &str) -> Option<Tile> {
    Some(match s {
        "open" => Tile::Open,
        "floor" => Tile::Floor,
        "wall" => Tile::Wall,
        "ramp" => Tile::Ramp,
        "stair-up" => Tile::StairUp,
        "stair-down" => Tile::StairDown,
        "stair-updown" => Tile::StairUpDown,
        _ => return None,
    })
}

impl Blueprint {
    #[inline]
    fn idx(&self, x: i32, y: i32, z: i32) -> usize {
        ((z * self.h + y) * self.w + x) as usize
    }

    pub fn get(&self, x: i32, y: i32, z: i32) -> Option<&BpCell> {
        if x < 0 || y < 0 || z < 0 || x >= self.w || y >= self.h || z >= self.d {
            return None;
        }
        self.cells[self.idx(x, y, z)].as_ref()
    }

    /// Copy a box of the world, `origin` being its lowest corner. Creatures
    /// are left out; everything else in the box is taken, open air included.
    pub fn capture(name: &str, world: &World, things: &Things, origin: Pos, w: i32, h: i32, d: i32) -> Blueprint {
        let mut cells = Vec::with_capacity((w * h * d) as usize);
        for z in 0..d {
            for y in 0..h {
                for x in 0..w {
                    let p = origin.offset(x, y, z);
                    cells.push(world.index(p).map(|i| {
                        let mut ts: Vec<(Layer, String)> = things
                            .at(p)
                            .map(|(_, t)| things.kind_of(t))
                            .filter(|k| k.layer < Layer::Creature)
                            .map(|k| (k.layer, k.name.clone()))
                            .collect();
                        ts.sort();
                        let tile = world.tile[i];
                        let mat = if tile == Tile::Open { String::new() } else { world.mats.get(world.mat[i]).name.clone() };
                        BpCell { tile, mat, things: ts.into_iter().map(|(_, n)| n).collect() }
                    }));
                }
            }
        }
        Blueprint { name: name.into(), w, h, d, cells }
    }

    /// Build it into the world with its lowest corner at `origin`. Cells it
    /// covers lose their old furniture and items; creatures are not touched.
    pub fn stamp(&self, world: &mut World, things: &mut Things, origin: Pos) -> StampReport {
        let mut report = StampReport::default();
        for z in 0..self.d {
            for y in 0..self.h {
                for x in 0..self.w {
                    let Some(c) = &self.cells[self.idx(x, y, z)] else { continue };
                    let p = origin.offset(x, y, z);
                    if world.index(p).is_none() {
                        continue;
                    }
                    let mat = if c.tile == Tile::Open {
                        AIR
                    } else {
                        match world.mats.find(&c.mat) {
                            Some(m) => m,
                            None => {
                                if !report.unknown_materials.contains(&c.mat) {
                                    report.unknown_materials.push(c.mat.clone());
                                }
                                continue;
                            }
                        }
                    };
                    world.set_tile(p, c.tile, mat);
                    let old: Vec<_> = things.at(p).filter(|(_, t)| things.kind_of(t).layer < Layer::Creature).map(|(id, _)| id).collect();
                    for id in old {
                        things.despawn(id);
                    }
                    for name in &c.things {
                        match things.find_kind(name) {
                            Some(k) => {
                                things.spawn(k, p);
                            }
                            None => {
                                if !report.unknown_kinds.contains(name) {
                                    report.unknown_kinds.push(name.clone());
                                }
                            }
                        }
                    }
                    report.cells += 1;
                }
            }
        }
        report
    }

    // ---- text form ------------------------------------------------------

    pub fn to_text(&self) -> String {
        // Give each distinct cell a character, preferring obvious ones.
        let mut keys: Vec<(char, &BpCell)> = Vec::new();
        let pool: Vec<char> = "#.+_^<>X=%&*ABCDEFGHIJKLMNOPQRSTUVWYZabcdefghijklmnopqrstuvwxyz0123456789!$,-/:;?@[]{}~|'\"`()"
            .chars()
            .collect();
        let preferred = |c: &BpCell| -> &'static [char] {
            match (c.tile, c.things.is_empty()) {
                (Tile::Wall, _) => &['#', 'W', 'H', 'M'],
                (Tile::Floor, true) => &['.', ',', '+', ':'],
                (Tile::Open, true) => &['_'],
                (Tile::Ramp, _) => &['^'],
                (Tile::StairUp, _) => &['<'],
                (Tile::StairDown, _) => &['>'],
                (Tile::StairUpDown, _) => &['X'],
                _ => &[],
            }
        };
        for c in self.cells.iter().flatten() {
            if keys.iter().any(|(_, k)| *k == c) {
                continue;
            }
            let used = |ch: &char| keys.iter().any(|(k, _)| k == ch);
            // Furniture: the first letter of the first thing (b for a bed).
            let initial: Vec<char> = c.things.first().and_then(|t| t.chars().next()).map(|f| vec![f.to_ascii_lowercase(), f.to_ascii_uppercase()]).unwrap_or_default();
            let ch = initial.iter().chain(preferred(c)).chain(pool.iter()).find(|ch| ch.is_ascii_graphic() && !used(ch)).copied().unwrap_or('?');
            keys.push((ch, c));
        }
        let mut s = format!("tachyon-blueprint 1\nname {}\nsize {} {} {}\n", self.name, self.w, self.h, self.d);
        for (ch, c) in &keys {
            s += &format!("key {} {}", ch, tile_word(c.tile));
            if !c.mat.is_empty() {
                s += &format!(" {}", c.mat);
            }
            if !c.things.is_empty() {
                s += &format!(" | {}", c.things.join(", "));
            }
            s.push('\n');
        }
        for z in 0..self.d {
            s += &format!("level {z}\n");
            for y in 0..self.h {
                let row: String = (0..self.w)
                    .map(|x| match &self.cells[self.idx(x, y, z)] {
                        None => ' ',
                        Some(c) => keys.iter().find(|(_, k)| *k == c).map(|(ch, _)| *ch).unwrap_or('?'),
                    })
                    .collect();
                s += row.trim_end();
                s.push('\n');
            }
        }
        s += "end\n";
        s
    }

    pub fn from_text(text: &str) -> Result<Blueprint, String> {
        let mut lines = text.lines();
        let first = lines.next().ok_or("empty file")?;
        if !first.starts_with("tachyon-blueprint") {
            return Err("not a blueprint (no 'tachyon-blueprint' header)".into());
        }
        let mut bp = Blueprint { name: String::new(), w: 0, h: 0, d: 0, cells: Vec::new() };
        let mut keys: Vec<(char, BpCell)> = Vec::new();
        let num = |s: &str| s.trim().parse::<i32>().map_err(|e| format!("bad number '{s}': {e}"));
        while let Some(line) = lines.next() {
            let line = line.trim_end_matches('\r');
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            let (word, rest) = line.split_once(' ').unwrap_or((line, ""));
            match word {
                "name" => bp.name = rest.to_string(),
                "size" => {
                    let v: Vec<&str> = rest.split_whitespace().collect();
                    if v.len() != 3 {
                        return Err("size needs width, height and levels".into());
                    }
                    bp.w = num(v[0])?;
                    bp.h = num(v[1])?;
                    bp.d = num(v[2])?;
                    bp.cells = vec![None; (bp.w * bp.h * bp.d) as usize];
                }
                "key" => {
                    let mut chars = rest.chars();
                    let ch = chars.next().ok_or("key needs a character")?;
                    let rest: String = chars.collect();
                    let (body, things) = match rest.split_once('|') {
                        Some((b, t)) => (b, t.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()),
                        None => (rest.as_str(), Vec::new()),
                    };
                    let body = body.trim();
                    let (tw, mat) = body.split_once(' ').unwrap_or((body, ""));
                    let tile = parse_tile(tw).ok_or(format!("unknown tile '{tw}' (use open, floor, wall, ramp, stair-up, stair-down, stair-updown)"))?;
                    keys.push((ch, BpCell { tile, mat: mat.trim().to_string(), things }));
                }
                "level" => {
                    let z = num(rest)?;
                    if z < 0 || z >= bp.d {
                        return Err(format!("level {z} is outside size"));
                    }
                    for y in 0..bp.h {
                        let row = lines.next().ok_or("level ends early")?.trim_end_matches('\r');
                        for (x, ch) in row.chars().enumerate().take(bp.w as usize) {
                            if ch == ' ' {
                                continue;
                            }
                            let c = keys.iter().find(|(k, _)| *k == ch).ok_or(format!("character '{ch}' has no key"))?;
                            let i = bp.idx(x as i32, y, z);
                            bp.cells[i] = Some(c.1.clone());
                        }
                    }
                }
                "end" => break,
                other => return Err(format!("unknown line '{other}'")),
            }
        }
        if bp.cells.is_empty() {
            return Err("blueprint has no size".into());
        }
        Ok(bp)
    }
}
