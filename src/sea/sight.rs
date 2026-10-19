//! Seeing far: the horizon, hills in the way, haze, smoke and darkness.
//!
//! The 1 m field of view (`view::fov`) answers "what can I see in this
//! room?". This answers "can the lookout make out that sail?", for things
//! kilometres off:
//!
//! - **The curve of the earth**, with standard refraction (an effective
//!   radius 1/(1-0.13) times the real one), so the sea horizon from 1.7 m is
//!   about 5 km and from a 30 m masthead about 21 km. Beyond it, a ship is
//!   hull down: her canvas shows and her hull does not.
//! - **High ground** between eye and target (sampled from a ground-height
//!   function, e.g. [`crate::sea::Sea::ground`]), and any **obstacles** the
//!   game lists (other ships' hulls and canvas, a tower).
//! - **Waves**: in a heavy sea the crests stand between you and the far
//!   water, raising the horizon, and a boat low in the water drops out of
//!   sight in the troughs.
//! - **Contrast**: what survives the haze over the distance (Koschmieder:
//!   meteorological visibility is where contrast falls to 2%), plus any
//!   **murk** the line passes through (a fog bank, powder smoke, a rain
//!   shaft), against the eye's threshold, which rises for small targets and
//!   in poor light. A glass magnifies the target but not the contrast, so it
//!   helps more on a clear day than in haze.

pub const EARTH_RADIUS: f32 = 6_371_000.0;
/// Standard terrestrial refraction coefficient.
pub const REFRACTION: f32 = 0.13;
/// Smallest contrast the eye can see in daylight, for a large target.
pub const EYE_CONTRAST: f32 = 0.02;
/// Angular size (radians, ~1.5 arcminutes) below which small targets need
/// much more contrast to be seen.
pub const EYE_ANGLE: f32 = 4.4e-4;

/// Effective earth radius with refraction.
pub fn effective_radius() -> f32 {
    EARTH_RADIUS / (1.0 - REFRACTION)
}

/// Distance (m) to the sea horizon from an eye `h` metres above the water.
pub fn horizon(h: f32) -> f32 {
    (2.0 * effective_radius() * h.max(0.0)).sqrt()
}

/// How far below a flat plane the surface lies at distance `d` (m).
#[inline]
pub fn drop(d: f32) -> f32 {
    d * d / (2.0 * effective_radius())
}

/// A patch of air that light struggles through: fog, smoke, a rain shaft.
#[derive(Clone, Copy, Debug)]
pub struct Murk {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    /// Height of its top above the sea (m).
    pub top: f32,
    /// Visibility inside it (m): dense fog 50, powder smoke 30, rain 2000.
    pub visibility: f32,
}

/// Something standing up that hides what is behind it.
#[derive(Clone, Copy, Debug)]
pub struct Obstacle {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    /// Height of its top above the sea (m).
    pub top: f32,
}

/// The conditions for looking.
#[derive(Clone, Debug)]
pub struct Air {
    /// Meteorological visibility (m): 50 km very clear, 10 km hazy, 1 km mist.
    pub visibility: f32,
    /// Daylight, 0 (a dark night) to 1 (full day).
    pub light: f32,
    /// Height of the sea surface now (the tide, m).
    pub sea_level: f32,
    /// Significant wave height (m).
    pub waves: f32,
    pub murk: Vec<Murk>,
    pub obstacles: Vec<Obstacle>,
}

impl Default for Air {
    fn default() -> Self {
        Air { visibility: 30_000.0, light: 1.0, sea_level: 0.0, waves: 0.0, murk: Vec::new(), obstacles: Vec::new() }
    }
}

/// What is to be seen: a vertical extent (waterline to masthead) with a
/// breadth and how much it stands out from what is behind it.
#[derive(Clone, Copy, Debug)]
pub struct Target {
    pub x: f32,
    pub y: f32,
    /// Heights above the sea of its lowest and highest parts (m).
    pub bottom: f32,
    pub top: f32,
    /// Height of its hull above `bottom` (m): hidden up to here, it is hull down.
    pub hull: f32,
    /// Breadth as seen (m).
    pub breadth: f32,
    /// Contrast against the sky or sea behind it, 0..1 (white canvas against
    /// sky ~0.7, a dark hull ~0.8, a grey rock ~0.3).
    pub contrast: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Seen {
    pub range: f32,
    /// Compass bearing from the eye (radians).
    pub bearing: f32,
    /// Height on the target (m above the sea) below which it is hidden by the
    /// horizon, high ground or an obstacle.
    pub hidden_below: f32,
    /// How much of its height shows (m).
    pub showing: f32,
    /// Its hull is hidden but its top shows.
    pub hull_down: bool,
    /// Angular size of what shows (radians, after magnification).
    pub size: f32,
    /// Contrast left after the haze and murk.
    pub contrast: f32,
    /// How plainly it can be made out: 0 not at all, 1 at the threshold,
    /// higher is clearer. Games turn this into "a sail", "a brig, hull up".
    pub clarity: f32,
}

/// Look from `eye` (x, y, height above the sea) at `target` through `air`,
/// with a glass of `magnification` (1 for the naked eye), over ground whose
/// height above mean sea level is `ground(x, y)` (negative under the sea).
pub fn look(eye: (f32, f32, f32), target: &Target, air: &Air, magnification: f32, ground: impl Fn(f32, f32) -> f32) -> Seen {
    let (dx, dy) = (target.x - eye.0, target.y - eye.1);
    let range = (dx * dx + dy * dy).sqrt().max(1.0);
    let bearing = dx.atan2(-dy).rem_euclid(std::f32::consts::TAU);
    let eye_h = eye.2 - air.sea_level;
    // Steepest elevation angle (radians, small) of anything in the way, as a
    // slope: height gained per metre.
    // The sea: its crests stand ~0.6 Hs above the mean; the horizon is the
    // tangent to that raised curve.
    let crest = 0.6 * air.waves;
    let hz = horizon((eye_h - crest).max(0.01));
    let mut block = if hz < range { (crest - eye_h - drop(hz)) / hz } else { f32::NEG_INFINITY };
    // High ground, sampled along the line.
    let n = ((range / 25.0) as usize).clamp(8, 400);
    for k in 1..n {
        let d = range * k as f32 / n as f32;
        let (x, y) = (eye.0 + dx * d / range, eye.1 + dy * d / range);
        let g = ground(x, y) - air.sea_level;
        if g > 0.0 {
            block = block.max((g - eye_h - drop(d)) / d);
        }
    }
    // Obstacles: anything whose disc the line passes through.
    for o in &air.obstacles {
        let (ox, oy) = (o.x - eye.0, o.y - eye.1);
        let t = (ox * dx + oy * dy) / (range * range);
        if t <= 0.0 || t >= 1.0 {
            continue;
        }
        let (px, py) = (ox - dx * t, oy - dy * t);
        if px * px + py * py <= o.radius * o.radius {
            let d = t * range;
            block = block.max((o.top - air.sea_level - eye_h - drop(d)) / d);
        }
    }
    let mut hidden_below = if block.is_finite() { eye_h + drop(range) + block * range } else { f32::NEG_INFINITY };
    // The crests just short of anything a few wavelengths off stand between
    // it and the eye: its lowest ~0.6 Hs is lost in the sea.
    hidden_below = hidden_below.max(crest * (1.0 - (-range / 200.0).exp()));
    let (bottom, top) = (target.bottom - air.sea_level, target.top - air.sea_level);
    let low = bottom.max(hidden_below);
    let showing = (top - low).max(0.0);
    let mut seen = Seen {
        range,
        bearing,
        hidden_below: hidden_below + air.sea_level,
        showing,
        hull_down: showing > 0.0 && hidden_below >= bottom + target.hull,
        ..Seen::default()
    };
    if showing <= 0.0 {
        return seen;
    }
    // Contrast through the haze and the murk.
    let mut extinction = 3.912 / air.visibility.max(1.0) * range;
    let sight_h = |d: f32| eye_h + (top.min(eye_h.max(low)) - eye_h) * d / range;
    for m in &air.murk {
        let (mx, my) = (m.x - eye.0, m.y - eye.1);
        let t = (mx * dx + my * dy) / (range * range);
        let (px, py) = (mx - dx * t.clamp(0.0, 1.0), my - dy * t.clamp(0.0, 1.0));
        let off2 = px * px + py * py;
        if off2 >= m.radius * m.radius {
            continue;
        }
        let half = (m.radius * m.radius - off2).sqrt();
        let (a, b) = ((t * range - half).max(0.0), (t * range + half).min(range));
        if b <= a || sight_h(0.5 * (a + b)) > m.top {
            continue;
        }
        extinction += 3.912 / m.visibility.max(1.0) * (b - a);
    }
    let contrast = target.contrast * (-extinction).exp();
    let size = (showing * target.breadth).sqrt() / range * magnification.max(1.0);
    // The eye needs more contrast for small things and in poor light.
    let light = air.light.clamp(0.0, 1.0);
    let need = EYE_CONTRAST * (1.0 + (EYE_ANGLE / size).powi(2)) * (1.0 + 40.0 * (1.0 - light).powi(2));
    seen.size = size;
    seen.contrast = contrast;
    seen.clarity = contrast / need;
    seen
}

/// The skyline along a bearing: for each step out to `max` metres, the
/// elevation angle (radians, small) of the ground there as seen from `eye`,
/// after the earth's curve. For drawing what a glass shows.
pub fn skyline(eye: (f32, f32, f32), bearing: f32, max: f32, steps: usize, sea_level: f32, ground: impl Fn(f32, f32) -> f32) -> Vec<(f32, f32)> {
    let (sx, sy) = (bearing.sin(), -bearing.cos());
    let eye_h = eye.2 - sea_level;
    (1..=steps.max(1))
        .map(|k| {
            let d = max * k as f32 / steps.max(1) as f32;
            let g = (ground(eye.0 + sx * d, eye.1 + sy * d) - sea_level).max(0.0);
            (d, ((g - eye_h - drop(d)) / d).atan())
        })
        .collect()
}

/// Elevation angle (radians) of a point `h` metres above the sea at range
/// `d`, seen from an eye `eye_h` metres up, after the earth's curve.
#[inline]
pub fn elevation(eye_h: f32, d: f32, h: f32) -> f32 {
    ((h - eye_h - drop(d)) / d.max(0.1)).atan()
}

/// The field of a first-person view (a glass, or the naked eye) laid over a
/// grid of `cols` x `rows` cells: which bearing each column looks along and
/// which elevation each row is at. A cell is taken to be `cell_aspect` times
/// taller than it is wide (2 for a terminal).
#[derive(Clone, Copy, Debug)]
pub struct Field {
    /// Compass bearing of the centre of the field (radians).
    pub bearing: f32,
    /// Width of the field (radians).
    pub width: f32,
    /// Height of the field (radians).
    pub height: f32,
    /// Elevation of the centre of the field (radians, up is positive).
    pub pitch: f32,
    pub cols: i32,
    pub rows: i32,
}

impl Field {
    /// A field `width` radians wide; its height follows the grid's shape
    /// (cells `cell_aspect` times taller than wide), but never less than
    /// `min_height`, so a strong glass is not a letterbox.
    pub fn new(bearing: f32, width: f32, pitch: f32, cols: i32, rows: i32, cell_aspect: f32, min_height: f32) -> Field {
        let (cols, rows) = (cols.max(1), rows.max(1));
        let height = (width * rows as f32 / cols as f32 * cell_aspect).max(min_height);
        Field { bearing, width, height, pitch, cols, rows }
    }

    /// The bearing down the middle of column `col`.
    pub fn bearing_of(&self, col: i32) -> f32 {
        (self.bearing - self.width / 2.0 + self.width * (col as f32 + 0.5) / self.cols as f32).rem_euclid(std::f32::consts::TAU)
    }

    /// Where a bearing falls across the field, in columns (fractional; off
    /// the field is < 0 or >= cols).
    pub fn column(&self, bearing: f32) -> f32 {
        let off = (bearing - self.bearing + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        (off + self.width / 2.0) / self.width * self.cols as f32
    }

    /// Where an elevation falls down the field, in rows (0 is the top).
    pub fn row(&self, elevation: f32) -> f32 {
        (self.pitch + self.height / 2.0 - elevation) / self.height * self.rows as f32
    }

    /// The elevation at the middle of row `row`.
    pub fn elevation_of(&self, row: i32) -> f32 {
        self.pitch + self.height / 2.0 - self.height * (row as f32 + 0.5) / self.rows as f32
    }

    /// The row the sea horizon falls on, from an eye `eye_h` above the sea.
    pub fn horizon_row(&self, eye_h: f32) -> f32 {
        let hz = horizon(eye_h.max(0.01));
        self.row(elevation(eye_h, hz, 0.0))
    }
}

/// A stretch of ground showing in one column of a [`Field`]: rows `top` to
/// `bottom` (exclusive), its face `range` metres off and `height` above the
/// sea.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    pub top: i32,
    pub bottom: i32,
    pub range: f32,
    pub height: f32,
}

/// What ground shows in each column of `field` from `eye` (x, y, height
/// above mean sea level), out to `max` metres: a heightfield raycast. Each
/// column's ray is marched outward and every time the ground clears
/// everything nearer, the rows it newly fills become a band, so the picture
/// builds from the bottom up and a far hill shows only above a near one.
/// Bands come nearest first. Ground at or below `sea_level` is sea and gives
/// no band.
pub fn ground_bands(eye: (f32, f32, f32), field: &Field, max: f32, sea_level: f32, ground: impl Fn(f32, f32) -> f32) -> Vec<Vec<Band>> {
    let eye_h = eye.2 - sea_level;
    (0..field.cols)
        .map(|col| {
            let b = field.bearing_of(col);
            let (sx, sy) = (b.sin(), -b.cos());
            let mut out = Vec::new();
            let mut low = field.rows; // first row not yet filled, from the bottom
            let mut d = 5.0f32;
            while d < max && low > 0 {
                // The sea fills rows too (up to its horizon), so a coast
                // stands on the water rather than on the bottom of the field.
                let g = ground(eye.0 + sx * d, eye.1 + sy * d) - sea_level;
                let top = field.row(elevation(eye_h, d, g.max(0.0))).round().max(0.0) as i32;
                if top < low {
                    if g > 0.0 {
                        out.push(Band { top, bottom: low, range: d, height: g });
                    }
                    low = top;
                }
                d += (d * 0.03).max(5.0);
            }
            out
        })
        .collect()
}
