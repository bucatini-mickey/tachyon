//! Things: everything in a cell that is not terrain, liquid or gas. Items,
//! furniture and creatures are all things, distinguished by their kind's
//! [`Layer`]. A cell can hold any number of things; it is drawn as the one on
//! the highest layer (a creature over a table over a mug), so a table with
//! things on it still reads as a table.
//!
//! Kinds are registered by the game. Things are stored in a generational
//! slot map (handles go stale safely when a thing is removed) plus a spatial
//! index from cell to the things in it.

use crate::color::Rgb;
use crate::geom::Pos;
use std::collections::HashMap;

pub type KindId = u16;

/// Draw priority, lowest first.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub enum Layer {
    Item = 0,
    Furniture = 1,
    Creature = 2,
    /// Drawn over everything: projectiles in flight, flashes.
    Effect = 3,
}

/// Something a creature can eat.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Food {
    /// How much hunger one serving takes away (1.0 = from starving to full).
    pub nutrition: f32,
    /// Meat (for carnivores) rather than plant food.
    pub meat: bool,
    /// A larder or a manger: eating from it doesn't use it up.
    pub stocked: bool,
}

/// Whether a thing's light is a flame, and how exposed it is.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Flame {
    /// Not a flame (glowing fungus, a magic crystal): nothing puts it out.
    #[default]
    None,
    /// An open flame (torch, candle): water, rain and strong wind put it out.
    Exposed,
    /// A flame behind glass or in a bowl (lantern, brazier): only water does.
    Sheltered,
}

#[derive(Clone, Debug)]
pub struct ThingKind {
    pub name: String,
    pub glyph: char,
    pub fg: Rgb,
    pub layer: Layer,
    /// Creatures cannot walk into a cell with this in it.
    pub blocks_move: bool,
    /// Blocks line of sight (a closed door, a wardrobe).
    pub blocks_sight: bool,
    /// A door: blocks movement and sight while closed; drawn with
    /// `open_glyph` when open.
    pub door: bool,
    pub open_glyph: char,
    /// Gives off light of this colour; the brightest channel sets the reach
    /// (255 is about 18 cells).
    pub light: Option<Rgb>,
    /// The light wavers like a flame.
    pub flicker: bool,
    /// Furniture that other things can be put on (tables, shelves). Drawn
    /// slightly differently when something is on it.
    pub surface: bool,
    /// A floor you can stand on wherever it is: a boat's deck, a rope
    /// bridge. Creatures on it neither fall nor swim.
    pub deck: bool,
    /// Its light is a flame that can go out (see [`Thing::lit`]).
    pub flame: Flame,
    /// Creatures can eat it (it is used up).
    pub food: Option<Food>,
    /// Somewhere to sleep.
    pub bed: bool,
}

impl ThingKind {
    pub fn new(name: &str, glyph: char, fg: Rgb, layer: Layer) -> ThingKind {
        ThingKind {
            name: name.into(),
            glyph,
            fg,
            layer,
            blocks_move: false,
            blocks_sight: false,
            door: false,
            open_glyph: glyph,
            light: None,
            flicker: false,
            surface: false,
            deck: false,
            flame: Flame::None,
            food: None,
            bed: false,
        }
    }
    pub fn blocking(mut self) -> Self {
        self.blocks_move = true;
        self
    }
    pub fn opaque(mut self) -> Self {
        self.blocks_sight = true;
        self
    }
    pub fn door(mut self, open_glyph: char) -> Self {
        self.door = true;
        self.open_glyph = open_glyph;
        self
    }
    pub fn light(mut self, color: Rgb, flicker: bool) -> Self {
        self.light = Some(color);
        self.flicker = flicker;
        self
    }
    pub fn surface(mut self) -> Self {
        self.surface = true;
        self
    }
    pub fn deck(mut self) -> Self {
        self.deck = true;
        self
    }
    pub fn flame(mut self, f: Flame) -> Self {
        self.flame = f;
        self
    }
    pub fn food(mut self, nutrition: f32, meat: bool) -> Self {
        self.food = Some(Food { nutrition, meat, stocked: false });
        self
    }
    /// Food that is never used up (a well-kept larder).
    pub fn stocked(mut self) -> Self {
        if let Some(f) = &mut self.food {
            f.stocked = true;
        }
        self
    }
    pub fn bed(mut self) -> Self {
        self.bed = true;
        self
    }
}

/// A handle to a thing. Stale handles (to removed things) return `None`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ThingId {
    pub index: u32,
    pub gen: u32,
}

#[derive(Clone, Debug)]
pub struct Thing {
    pub kind: KindId,
    pos: Pos,
    /// Doors: open or shut.
    pub open: bool,
    /// Overrides the kind's colour (a red-coated villager).
    pub color: Option<Rgb>,
    /// Free-form name for individuals ("Agnes").
    pub name: Option<String>,
    /// A flame thing is burning (things whose kind has no flame ignore this).
    pub lit: bool,
    /// How soaked it is, in ticks of drying left; a wet torch is hard to light.
    pub wet: u16,
}

impl Thing {
    #[inline]
    pub fn pos(&self) -> Pos {
        self.pos
    }
}

impl Things {
    /// Is this thing giving off light right now? (A flame that has gone out
    /// does not.)
    pub fn shines(&self, t: &Thing) -> bool {
        let k = self.kind(t.kind);
        k.light.is_some() && (k.flame == Flame::None || t.lit)
    }
}

#[derive(Clone, Debug)]
struct Slot {
    gen: u32,
    thing: Option<Thing>,
}

#[derive(Clone, Debug, Default)]
pub struct Things {
    kinds: Vec<ThingKind>,
    kind_names: HashMap<String, KindId>,
    slots: Vec<Slot>,
    free: Vec<u32>,
    at: HashMap<Pos, Vec<u32>>,
    count: usize,
}

impl Things {
    pub fn new() -> Things {
        Things::default()
    }

    pub fn add_kind(&mut self, k: ThingKind) -> KindId {
        let id = self.kinds.len() as KindId;
        self.kind_names.insert(k.name.clone(), id);
        self.kinds.push(k);
        id
    }

    pub fn find_kind(&self, name: &str) -> Option<KindId> {
        self.kind_names.get(name).copied()
    }

    /// Every registered kind, in order of registration.
    pub fn kinds(&self) -> impl Iterator<Item = (KindId, &ThingKind)> {
        self.kinds.iter().enumerate().map(|(i, k)| (i as KindId, k))
    }

    pub fn kind_id(&self, name: &str) -> KindId {
        *self.kind_names.get(name).unwrap_or_else(|| panic!("no thing kind named '{name}'"))
    }

    #[inline]
    pub fn kind(&self, id: KindId) -> &ThingKind {
        &self.kinds[id as usize]
    }

    pub fn kind_of(&self, t: &Thing) -> &ThingKind {
        &self.kinds[t.kind as usize]
    }

    pub fn spawn(&mut self, kind: KindId, pos: Pos) -> ThingId {
        let thing = Thing { kind, pos, open: false, color: None, name: None, lit: true, wet: 0 };
        let index = if let Some(i) = self.free.pop() {
            self.slots[i as usize].thing = Some(thing);
            i
        } else {
            self.slots.push(Slot { gen: 0, thing: Some(thing) });
            (self.slots.len() - 1) as u32
        };
        self.at.entry(pos).or_default().push(index);
        self.count += 1;
        ThingId { index, gen: self.slots[index as usize].gen }
    }

    pub fn despawn(&mut self, id: ThingId) {
        let Some(slot) = self.slots.get_mut(id.index as usize) else { return };
        if slot.gen != id.gen {
            return;
        }
        if let Some(t) = slot.thing.take() {
            slot.gen = slot.gen.wrapping_add(1);
            self.free.push(id.index);
            self.count -= 1;
            Self::unindex(&mut self.at, t.pos, id.index);
        }
    }

    fn unindex(at: &mut HashMap<Pos, Vec<u32>>, pos: Pos, index: u32) {
        if let Some(v) = at.get_mut(&pos) {
            v.retain(|&j| j != index);
            if v.is_empty() {
                at.remove(&pos);
            }
        }
    }

    #[inline]
    pub fn get(&self, id: ThingId) -> Option<&Thing> {
        let s = self.slots.get(id.index as usize)?;
        if s.gen == id.gen {
            s.thing.as_ref()
        } else {
            None
        }
    }

    /// Mutable access. Position is changed only through [`Things::move_to`].
    pub fn get_mut(&mut self, id: ThingId) -> Option<&mut Thing> {
        let s = self.slots.get_mut(id.index as usize)?;
        if s.gen == id.gen {
            s.thing.as_mut()
        } else {
            None
        }
    }

    pub fn move_to(&mut self, id: ThingId, pos: Pos) {
        let Some(s) = self.slots.get_mut(id.index as usize) else { return };
        if s.gen != id.gen {
            return;
        }
        let Some(t) = s.thing.as_mut() else { return };
        let old = t.pos;
        if old == pos {
            return;
        }
        t.pos = pos;
        Self::unindex(&mut self.at, old, id.index);
        self.at.entry(pos).or_default().push(id.index);
    }

    /// Everything in one cell.
    pub fn at(&self, pos: Pos) -> impl Iterator<Item = (ThingId, &Thing)> + '_ {
        self.at.get(&pos).into_iter().flatten().filter_map(move |&i| {
            let s = &self.slots[i as usize];
            s.thing.as_ref().map(|t| (ThingId { index: i, gen: s.gen }, t))
        })
    }

    /// Every thing in the world.
    pub fn iter(&self) -> impl Iterator<Item = (ThingId, &Thing)> + '_ {
        self.slots.iter().enumerate().filter_map(|(i, s)| {
            s.thing.as_ref().map(|t| (ThingId { index: i as u32, gen: s.gen }, t))
        })
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn blocks_move(&self, pos: Pos) -> bool {
        self.at(pos).any(|(_, t)| {
            let k = self.kind(t.kind);
            k.blocks_move && !(k.door && t.open)
        })
    }

    pub fn blocks_sight(&self, pos: Pos) -> bool {
        self.at(pos).any(|(_, t)| {
            let k = self.kind(t.kind);
            (k.blocks_sight || k.door) && !(k.door && t.open)
        })
    }

    /// Is there a deck to stand on here?
    pub fn deck_at(&self, pos: Pos) -> bool {
        self.at(pos).any(|(_, t)| self.kind(t.kind).deck)
    }

    pub fn door_at(&self, pos: Pos) -> Option<ThingId> {
        self.at(pos).find(|(_, t)| self.kind(t.kind).door).map(|(id, _)| id)
    }

    pub fn creature_at(&self, pos: Pos) -> Option<ThingId> {
        self.at(pos).find(|(_, t)| self.kind(t.kind).layer == Layer::Creature).map(|(id, _)| id)
    }

    /// Does any thing occupy cells? (Fast check used by the renderer.)
    pub fn has_any_at(&self, pos: Pos) -> bool {
        self.at.contains_key(&pos)
    }
}
