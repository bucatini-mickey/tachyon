//! Positions, directions and rectangles.

/// A cell position. `z` is the level: 0 is the bottom of the world.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, PartialOrd, Ord)]
pub struct Pos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl Pos {
    #[inline]
    pub const fn new(x: i32, y: i32, z: i32) -> Pos {
        Pos { x, y, z }
    }
    #[inline]
    pub const fn offset(self, dx: i32, dy: i32, dz: i32) -> Pos {
        Pos { x: self.x + dx, y: self.y + dy, z: self.z + dz }
    }
    #[inline]
    pub const fn up(self) -> Pos {
        self.offset(0, 0, 1)
    }
    #[inline]
    pub const fn down(self) -> Pos {
        self.offset(0, 0, -1)
    }
    /// Chessboard distance on the level (diagonal steps count as one).
    #[inline]
    pub fn dist2d(self, o: Pos) -> i32 {
        (self.x - o.x).abs().max((self.y - o.y).abs())
    }
}

/// North, east, south, west. Screen y grows downward, so north is -y.
pub const DIRS4: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];
/// The four orthogonal directions followed by the four diagonals.
pub const DIRS8: [(i32, i32); 8] =
    [(0, -1), (1, 0), (0, 1), (-1, 0), (1, -1), (1, 1), (-1, 1), (-1, -1)];

/// A screen or map rectangle.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }
    #[inline]
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
    pub fn right(&self) -> i32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }
    /// The rectangle shrunk by `m` on every side.
    pub fn inset(&self, m: i32) -> Rect {
        Rect::new(self.x + m, self.y + m, (self.w - 2 * m).max(0), (self.h - 2 * m).max(0))
    }
}
