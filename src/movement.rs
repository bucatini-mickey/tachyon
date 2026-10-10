//! Moving creatures through the grid: walking, ramps, stairs, doors,
//! swimming and falling. Shared by the player and by AI so both obey the
//! same rules.

use crate::geom::Pos;
use crate::things::{Layer, ThingId, Things};
use crate::tile::Tile;
use crate::world::World;

/// Liquid deep enough to swim in (out of 64).
pub const SWIM_DEPTH: u8 = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Moved(Pos),
    OpenedDoor(ThingId),
    /// Would walk off an edge with nothing below to land in.
    Ledge,
    Blocked,
}

fn swimming(world: &World, p: Pos) -> bool {
    world.index(p).is_some_and(|i| world.fluid[i] >= SWIM_DEPTH)
}

/// Can a creature be in this cell at all (supported, on a deck, or swimming)?
pub fn can_occupy(world: &World, things: &Things, p: Pos) -> bool {
    world.standable(p) || swimming(world, p) || things.deck_at(p)
}

/// Try to step one cell sideways (dx, dy in -1..=1).
pub fn step(world: &World, things: &mut Things, who: ThingId, dx: i32, dy: i32) -> Step {
    let Some(me) = things.get(who) else { return Step::Blocked };
    let to = match land(world, things, me.pos(), dx, dy) {
        Ok(p) => p,
        Err(s) => return s,
    };
    if let Some(door) = things.door_at(to) {
        if things.get(door).is_some_and(|d| !d.open) {
            if let Some(d) = things.get_mut(door) {
                d.open = true;
            }
            return Step::OpenedDoor(door);
        }
    }
    if things.blocks_move(to) {
        return Step::Blocked;
    }
    things.move_to(who, to);
    Step::Moved(to)
}

/// Where a step would take a creature, for planning a route: doors count as
/// open and other creatures as out of the way. `None` if it can't go there.
pub fn destination(world: &World, things: &Things, from: Pos, dx: i32, dy: i32) -> Option<Pos> {
    let to = land(world, things, from, dx, dy).ok()?;
    let blocked = things.at(to).any(|(_, t)| {
        let k = things.kind(t.kind);
        k.blocks_move && !k.door && k.layer != Layer::Creature
    });
    (!blocked).then_some(to)
}

/// Where a step lands, judged on the terrain alone (doors and things are
/// left to the caller).
fn land(world: &World, things: &Things, from: Pos, dx: i32, dy: i32) -> Result<Pos, Step> {
    let mut to = from.offset(dx, dy, 0);

    // Climb out of the water onto a deck floating above it.
    let on_deck = things.deck_at(from);
    if !on_deck && swimming(world, from) && things.deck_at(to.up()) && !world.tile(from.up()).seals_below() {
        to = to.up();
    }

    // Walk up: from a ramp, a deck, or out of water, onto the level above a wall.
    if world.tile(to).is_wall() {
        let can_climb = world.tile(from) == Tile::Ramp || swimming(world, from) || on_deck;
        // Headroom: no ceiling over our head between here and the level above.
        let headroom = !world.tile(from.up()).seals_below();
        let up = to.up();
        if can_climb && headroom && world.standable(up) {
            to = up;
        } else {
            return Err(Step::Blocked);
        }
    }

    if !can_occupy(world, things, to) {
        // Walk down a ramp, or step down into deep water.
        let below = to.down();
        if world.tile(below) == Tile::Ramp && world.standable(below) {
            to = below;
        } else if swimming(world, below) && !world.tile(to).seals_below() {
            to = below;
        } else {
            return Err(Step::Ledge);
        }
    }
    Ok(to)
}

/// Climb stairs. Returns the new position if it worked.
pub fn climb(world: &World, things: &mut Things, who: ThingId, up: bool) -> Option<Pos> {
    let from = things.get(who)?.pos();
    let here = world.tile(from);
    let to = if up { from.up() } else { from.down() };
    let there = world.tile(to);
    let ok = if up {
        matches!(here, Tile::StairUp | Tile::StairUpDown) && matches!(there, Tile::StairDown | Tile::StairUpDown)
    } else {
        matches!(here, Tile::StairDown | Tile::StairUpDown) && matches!(there, Tile::StairUp | Tile::StairUpDown)
    };
    if ok && !things.blocks_move(to) {
        things.move_to(who, to);
        Some(to)
    } else {
        None
    }
}

/// Drop a creature one level if nothing holds it up. Returns true if it fell.
pub fn fall(world: &World, things: &mut Things, who: ThingId) -> bool {
    let Some(me) = things.get(who) else { return false };
    let p = me.pos();
    if can_occupy(world, things, p) || p.z == 0 || world.tile(p).seals_below() {
        return false;
    }
    let below = p.down();
    if world.tile(below).is_wall() {
        return false;
    }
    things.move_to(who, below);
    true
}

/// Close any open door next to `p`. Returns how many were closed.
pub fn close_doors_near(things: &mut Things, p: Pos) -> usize {
    let mut n = 0;
    for dy in -1..=1 {
        for dx in -1..=1 {
            let q = p.offset(dx, dy, 0);
            if q == p || things.creature_at(q).is_some() {
                continue;
            }
            if let Some(d) = things.door_at(q) {
                if let Some(t) = things.get_mut(d) {
                    if t.open {
                        t.open = false;
                        n += 1;
                    }
                }
            }
        }
    }
    n
}
