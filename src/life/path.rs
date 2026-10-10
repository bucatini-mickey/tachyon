//! Finding routes through the grid by the same rules creatures walk by
//! ([`crate::movement`]): ramps, stairs, doors (which count as open), deep
//! water (slow), and ledges (which can't be walked off). Fire and scalding
//! heat are avoided altogether.
//!
//! Paths come back reversed, so the next step is `path.last()` and taking
//! it is `path.pop()`.

use crate::geom::{Pos, DIRS8};
use crate::movement;
use crate::things::Things;
use crate::tile::Tile;
use crate::world::World;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

/// Cells a search may look at before giving up.
pub const DEFAULT_BUDGET: usize = 4000;

/// Too hot to walk into: burning, or air hot enough to scald.
pub fn hazard(world: &World, p: Pos) -> bool {
    world.index(p).is_some_and(|i| world.fire[i] > 0 || world.temp[i] > 55.0)
}

/// The cells one step from `p`, with the cost of going there (10 straight,
/// 14 diagonal, more for swimming).
pub fn neighbours(world: &World, things: &Things, p: Pos, out: &mut Vec<(Pos, i32)>) {
    out.clear();
    for (k, &(dx, dy)) in DIRS8.iter().enumerate() {
        if let Some(q) = movement::destination(world, things, p, dx, dy).filter(|&q| !hazard(world, q)) {
            let mut cost = if k < 4 { 10 } else { 14 };
            if world.index(q).is_some_and(|i| world.fluid[i] >= movement::SWIM_DEPTH) {
                cost *= 3;
            }
            out.push((q, cost));
        }
    }
    let here = world.tile(p);
    if matches!(here, Tile::StairUp | Tile::StairUpDown) && matches!(world.tile(p.up()), Tile::StairDown | Tile::StairUpDown) {
        out.push((p.up(), 12));
    }
    if matches!(here, Tile::StairDown | Tile::StairUpDown) && matches!(world.tile(p.down()), Tile::StairUp | Tile::StairUpDown) {
        out.push((p.down(), 12));
    }
}

/// Best-first search from `from` to the nearest cell where `goal` holds.
/// `guess` estimates the cost left (0 gives a plain nearest-first flood,
/// good for "find any water"). Returns the reversed path, without `from`;
/// an empty path if `from` is already a goal.
pub fn search(world: &World, things: &Things, from: Pos, budget: usize, goal: impl Fn(Pos) -> bool, guess: impl Fn(Pos) -> i32) -> Option<Vec<Pos>> {
    if goal(from) {
        return Some(Vec::new());
    }
    let mut came: HashMap<Pos, (Pos, i32)> = HashMap::new();
    let mut open = BinaryHeap::new();
    came.insert(from, (from, 0));
    open.push(Reverse((guess(from), 0, from)));
    let mut next = Vec::with_capacity(10);
    let mut seen = 0;
    while let Some(Reverse((_, g, p))) = open.pop() {
        if came.get(&p).is_some_and(|&(_, best)| best < g) {
            continue; // a stale entry: found a cheaper way here since
        }
        if goal(p) {
            let mut path = vec![p];
            let mut at = p;
            while let Some(&(prev, _)) = came.get(&at) {
                if prev == from {
                    break;
                }
                path.push(prev);
                at = prev;
            }
            return Some(path);
        }
        seen += 1;
        if seen > budget {
            return None;
        }
        neighbours(world, things, p, &mut next);
        for &(q, c) in &next {
            let ng = g + c;
            if came.get(&q).map_or(true, |&(_, best)| ng < best) {
                came.insert(q, (p, ng));
                open.push(Reverse((ng + guess(q), ng, q)));
            }
        }
    }
    None
}

/// A route to `to`, or to any cell within `near` of it on its level (1 to
/// stand beside something). Reversed; see the module notes.
pub fn find(world: &World, things: &Things, from: Pos, to: Pos, near: i32, budget: usize) -> Option<Vec<Pos>> {
    let guess = |p: Pos| ((p.dist2d(to) - near).max(0) + (p.z - to.z).abs()) * 10;
    search(world, things, from, budget, |p| p.z == to.z && p.dist2d(to) <= near, guess)
}
