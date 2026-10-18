//! The shape of a cell. Together with its material, this is the "terrain" of
//! the cell; liquids, gases, fire and things sit on top.

/// Shapes follow Dwarf Fortress. A cell is one metre cubed; its floor (if it
/// has one) is a slab at the bottom of the cell.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub enum Tile {
    /// Empty space with no floor: things here fall.
    #[default]
    Open = 0,
    /// A floor slab with open space above it.
    Floor,
    /// Solid all the way through.
    Wall,
    /// A slope: from here you can walk up onto the level above.
    Ramp,
    /// Stairs leading up (the cell above should be `StairDown` or `StairUpDown`).
    StairUp,
    /// The top of stairs leading down. There is a hole in the floor.
    StairDown,
    /// A landing in the middle of a stairwell.
    StairUpDown,
}

impl Tile {
    #[inline(always)]
    pub fn is_wall(self) -> bool {
        self == Tile::Wall
    }

    /// Something to stand on inside the cell.
    #[inline(always)]
    pub fn has_floor(self) -> bool {
        !matches!(self, Tile::Open | Tile::Wall)
    }

    /// Nothing (liquid, gas, light) passes down out of this cell into the
    /// one below: it is solid or has an unbroken floor.
    #[inline(always)]
    pub fn seals_below(self) -> bool {
        matches!(self, Tile::Wall | Tile::Floor | Tile::Ramp | Tile::StairUp)
    }

    pub fn name(self) -> &'static str {
        match self {
            Tile::Open => "open space",
            Tile::Floor => "floor",
            Tile::Wall => "wall",
            Tile::Ramp => "ramp",
            Tile::StairUp => "up stair",
            Tile::StairDown => "down stair",
            Tile::StairUpDown => "up/down stair",
        }
    }
}
