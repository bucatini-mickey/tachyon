//! Field of view on one level: symmetric shadowcasting (Albert Ford's
//! formulation). Symmetric means if A can see B then B can see A, which keeps
//! hiding and ambushes fair. Walls that bound visible floor are visible.

/// Visibility on a level, cleared in O(1) between computations by bumping a
/// stamp rather than wiping the array.
#[derive(Clone, Debug, Default)]
pub struct FovMap {
    w: i32,
    h: i32,
    stamp: u32,
    vis: Vec<u32>,
    pub origin: (i32, i32),
    pub radius: i32,
}

#[derive(Clone, Copy)]
struct Row {
    depth: i32,
    start: (i32, i32), // slope as numerator / denominator, denominator > 0
    end: (i32, i32),
}

#[inline]
fn floor_div(a: i32, b: i32) -> i32 {
    a.div_euclid(b)
}

#[inline]
fn ceil_div(a: i32, b: i32) -> i32 {
    -((-a).div_euclid(b))
}

impl FovMap {
    pub fn new() -> FovMap {
        FovMap::default()
    }

    #[inline]
    pub fn visible(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.w && y < self.h && self.vis[(y * self.w + x) as usize] == self.stamp
    }

    #[inline]
    fn mark(&mut self, x: i32, y: i32) {
        self.vis[(y * self.w + x) as usize] = self.stamp;
    }

    /// Compute what is visible from `origin` on a `w` x `h` level.
    /// `opaque(x, y)` says whether a cell blocks sight; it is only called for
    /// cells inside the level.
    pub fn compute(&mut self, w: i32, h: i32, origin: (i32, i32), radius: i32, mut opaque: impl FnMut(i32, i32) -> bool) {
        if self.w != w || self.h != h {
            self.w = w;
            self.h = h;
            self.vis = vec![0; (w * h) as usize];
            self.stamp = 0;
        }
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            self.vis.fill(0);
            self.stamp = 1;
        }
        self.origin = origin;
        self.radius = radius;
        let (ox, oy) = origin;
        if ox < 0 || oy < 0 || ox >= w || oy >= h {
            return;
        }
        self.mark(ox, oy);
        let r2 = radius * radius + radius;
        let mut stack: Vec<Row> = Vec::with_capacity(64);
        for quadrant in 0..4 {
            let transform = |depth: i32, col: i32| -> (i32, i32) {
                match quadrant {
                    0 => (ox + col, oy - depth), // north
                    1 => (ox + depth, oy + col), // east
                    2 => (ox + col, oy + depth), // south
                    _ => (ox - depth, oy + col), // west
                }
            };
            stack.clear();
            stack.push(Row { depth: 1, start: (-1, 1), end: (1, 1) });
            while let Some(mut row) = stack.pop() {
                if row.depth > radius {
                    continue;
                }
                // round_ties_up(depth * start) and round_ties_down(depth * end)
                let min_col = floor_div(2 * row.depth * row.start.0 + row.start.1, 2 * row.start.1);
                let max_col = ceil_div(2 * row.depth * row.end.0 - row.end.1, 2 * row.end.1);
                let mut prev_wall: Option<bool> = None;
                for col in min_col..=max_col {
                    let (x, y) = transform(row.depth, col);
                    let inside = x >= 0 && y >= 0 && x < w && y < h;
                    let wall = !inside || opaque(x, y);
                    let symmetric = col * row.start.1 >= row.depth * row.start.0 && col * row.end.1 <= row.depth * row.end.0;
                    if inside && (wall || symmetric) && col * col + row.depth * row.depth <= r2 {
                        self.mark(x, y);
                    }
                    if prev_wall == Some(true) && !wall {
                        row.start = (2 * col - 1, 2 * row.depth);
                    }
                    if prev_wall == Some(false) && wall {
                        stack.push(Row { depth: row.depth + 1, start: row.start, end: (2 * col - 1, 2 * row.depth) });
                    }
                    prev_wall = Some(wall);
                }
                if prev_wall == Some(false) {
                    stack.push(Row { depth: row.depth + 1, start: row.start, end: row.end });
                }
            }
        }
    }
}
